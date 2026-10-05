// The one browser test (`make web-smoke`, gate step `web-smoke`): headless
// Chromium loads the app from a real chorus-server, through the fake login of
// fake-login.js, and what it asserts is text the page rendered.
//
// It is the only place a browser runs. A later change that needs a browser
// (the service worker, the install) adds to this file; the unit tests of
// ../test stay in node.
//
// CHORUS_SERVER_BIN names the built chorus-server; tools/web.sh refuses to run
// this file without it.

import { spawn } from "node:child_process";

import { expect, test } from "@playwright/test";

import { LOGIN_PATH, startFakeLogin } from "./fake-login.js";

// The test server's configuration: two rooms, one of them given a name its id
// does not hold, so the text asserted below can only be the server's.
const ROOMS = ["kitchen", "den"];
const NAMED = { id: "kitchen", name: "Smoke Test Kitchen" };

const LISTENING = /control listening on=\S*?:(\d+)/;

let server;
let serverLog = "";
let login;

// Start chorus-server with no audio device on loopback ports of its own
// choosing, and resolve to its control port.
function startServer() {
  const args = [
    "--listen", "127.0.0.1:0",
    "--control-listen", "127.0.0.1:0",
    "--ephemeral-identity",
    "--allow-non-realtime",
    "--allow-unlocked-memory",
    "--source", "tone",
    "--serve-forever",
    ...ROOMS.flatMap((room) => ["--zone", room]),
  ]; // prettier-ignore
  server = spawn(process.env.CHORUS_SERVER_BIN, args, { stdio: ["ignore", "pipe", "pipe"] });
  return new Promise((resolve, reject) => {
    const timer = setTimeout(
      () => reject(new Error(`chorus-server did not say where it listens within 30 s:\n${serverLog}`)),
      30_000,
    );
    const read = (chunk) => {
      serverLog += chunk;
      const found = LISTENING.exec(serverLog);
      if (found) {
        clearTimeout(timer);
        resolve(Number(found[1]));
      }
    };
    server.stdout.setEncoding("utf8").on("data", read);
    server.stderr.setEncoding("utf8").on("data", read);
    server.once("error", (error) => {
      clearTimeout(timer);
      reject(error);
    });
    server.once("exit", (code, signal) => {
      clearTimeout(timer);
      reject(new Error(`chorus-server exited (${code ?? signal}) before it listened:\n${serverLog}`));
    });
  });
}

test.beforeAll(async () => {
  expect(process.env.CHORUS_SERVER_BIN, "CHORUS_SERVER_BIN names a built chorus-server").toBeTruthy();
  const port = await startServer();
  // The room's name, set the way any client sets it: a command to the server
  // itself, not through the login.
  const origin = `http://127.0.0.1:${port}`;
  const named = await fetch(`${origin}/api/command`, {
    method: "POST",
    headers: { "Content-Type": "application/json", Origin: origin },
    body: JSON.stringify({ v: 1, t: "name", zone: NAMED.id, name: NAMED.name }),
  });
  expect(named.ok, `the server took the room's name: ${await named.text()}`).toBe(true);
  login = await startFakeLogin(port);
});

test.afterAll(async () => {
  if (login) await login.close();
  if (server && server.exitCode === null) {
    const gone = new Promise((resolve) => server.once("exit", resolve));
    server.kill("SIGTERM");
    await gone;
  }
});

test("signed in through the fake login, the app shows the server's rooms with no policy violation", async ({ page }) => {
  // Every violation of the page's Content-Security-Policy the browser reports,
  // and every error it logs or throws.
  const violations = [];
  const errors = [];
  await page.exposeFunction("chorusSmokeViolation", (violation) => violations.push(violation));
  await page.addInitScript(() => {
    document.addEventListener("securitypolicyviolation", (event) => {
      window.chorusSmokeViolation(`${event.effectiveDirective} blocked ${event.blockedURI || "inline"}`);
    });
  });
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  page.on("pageerror", (error) => errors.push(String(error)));

  // Not signed in: the server is not reached, and the browser is sent to the login.
  const refused = await page.request.get(`${login.origin}/api/state`);
  expect(refused.status()).toBe(401);
  await page.goto(`${login.origin}/app/`);
  await expect(page).toHaveURL(`${login.origin}${LOGIN_PATH}?rd=${encodeURIComponent("/app/")}`);

  // Sign in; the login sends the browser back to the app.
  await page.getByLabel("Name").fill("smoke");
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page).toHaveURL(`${login.origin}/app/`);
  expect(login.users()).toEqual(["smoke"]);

  // The document is chorus-server's own, under its policy.
  const document = await page.request.get(`${login.origin}/app/`);
  expect(document.status()).toBe(200);
  expect(document.headers()["content-security-policy"]).toContain("default-src 'none'; script-src 'self'; style-src 'self'");

  // Rendered text: the wordmark, and the rooms by the names the server has for them.
  await expect(page.getByRole("heading", { name: "chorus" })).toBeVisible();
  const rooms = page.getByRole("main", { name: "Rooms" });
  const named = rooms.getByText(NAMED.name, { exact: true });
  await expect(named).toBeVisible();
  await expect(rooms.getByRole("heading", { level: 2 })).toHaveText([NAMED.name, "den"]);
  // Each room's card drew its controls: the volume slider, labelled for its room.
  await expect(rooms.getByLabel(`Volume for ${NAMED.name}`)).toBeVisible();
  // Drawn, not only present: the text takes up room on the page.
  const box = await named.boundingBox();
  expect(box.width).toBeGreaterThan(0);
  expect(box.height).toBeGreaterThan(0);

  expect(violations, "Content-Security-Policy violations the page reported").toEqual([]);
  expect(errors, "errors the page logged or threw").toEqual([]);

  // The count above is of a listener that hears: an inline style, which the
  // policy forbids, is reported.
  await page.evaluate(() => {
    const style = document.createElement("style");
    style.textContent = "chorus-app { display: block; }";
    document.head.append(style);
  });
  await expect.poll(() => violations).toEqual(["style-src-elem blocked inline"]);
});
