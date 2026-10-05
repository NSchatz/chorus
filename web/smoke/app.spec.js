// The one browser test (`make web-smoke`, gate step `web-smoke`): headless
// Chromium loads the app from a real chorus-server, through the fake login of
// fake-login.js, and what it asserts is text the page rendered. Its tail is
// the install and the service worker: the manifest loads through the login,
// the worker becomes active and controls the page, no cache holds anything of
// the API after the page has read state and events through it, and with the
// login expired the page says "Signed out" and the login page is in no cache
// (docs/decisions/0190-the-app-installs-behind-the-login.md).
//
// It is the only place a browser runs. A later change that needs a browser
// (the service worker, the install) adds to this file; the unit tests of
// ../test stay in node.
//
// CHORUS_SERVER_BIN names the built chorus-server; tools/web.sh refuses to run
// this file without it.

import { spawn } from "node:child_process";
import { readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { expect, test } from "@playwright/test";

import { LOGIN_PATH, startFakeLogin } from "./fake-login.js";

// The test server's configuration: two rooms, one of them given a name its id
// does not hold, so the text asserted below can only be the server's.
const ROOMS = ["kitchen", "den"];
const NAMED = { id: "kitchen", name: "Smoke Test Kitchen" };

const LISTENING = /control listening on=\S*?:(\d+)/;

// The files of the app's committed output, as paths under /app/: what the
// server embeds, and so what the service worker is given to keep.
const DIST = path.join(path.dirname(fileURLToPath(import.meta.url)), "../dist");
const OUTPUT = readdirSync(DIST, { recursive: true, withFileTypes: true })
  .filter((entry) => entry.isFile())
  .map((entry) => path.relative(DIST, path.join(entry.parentPath, entry.name)).split(path.sep).join("/"));

// Every entry of every cache this origin has, read in the page: where it is
// from, what kind of response it is and, for a text file, what it says.
function readCaches(page) {
  return page.evaluate(async () => {
    const entries = [];
    for (const name of await caches.keys()) {
      const cache = await caches.open(name);
      for (const request of await cache.keys()) {
        const response = await cache.match(request);
        const type = response.headers.get("Content-Type") ?? "";
        entries.push({
          cache: name,
          url: request.url,
          status: response.status,
          type: response.type,
          redirected: response.redirected,
          text: /^(text\/|application\/(manifest\+)?json)/.test(type) ? await response.text() : "",
        });
      }
    }
    return entries;
  });
}

let server;
let serverLog = "";
let serverOrigin;
let login;

// One command to the server itself, the way any other client sends it: not
// through the login.
async function command(message) {
  const answer = await fetch(`${serverOrigin}/api/command`, {
    method: "POST",
    headers: { "Content-Type": "application/json", Origin: serverOrigin },
    body: JSON.stringify(message),
  });
  expect(answer.ok, `the server took ${JSON.stringify(message)}: ${await answer.text()}`).toBe(true);
}

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
  serverOrigin = `http://127.0.0.1:${port}`;
  // The room's name, set the way any client sets it.
  await command({ v: 1, t: "name", zone: NAMED.id, name: NAMED.name });
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

test("signed in through the fake login, the app shows the server's rooms with no policy violation, installs, keeps nothing of the API and says when it is signed out", async ({ page }) => {
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

  // Installable: the manifest link carries credentials, and the manifest the
  // browser loads through it, behind the login, is the app's own.
  const link = page.locator('link[rel="manifest"]');
  await expect(link).toHaveAttribute("crossorigin", "use-credentials");
  await expect(link).toHaveAttribute("href", "manifest.webmanifest");
  const devtools = await page.context().newCDPSession(page);
  const loaded = await devtools.send("Page.getAppManifest");
  expect(loaded.url).toBe(`${login.origin}/app/manifest.webmanifest`);
  expect(loaded.errors, "what the browser found wrong with the manifest").toEqual([]);
  expect(JSON.parse(loaded.data)).toMatchObject({
    name: "chorus",
    start_url: "/app/",
    scope: "/app/",
    display: "standalone",
    icons: expect.arrayContaining([expect.objectContaining({ sizes: "192x192" }), expect.objectContaining({ sizes: "512x512" })]),
  });
  // A manifest asked for without the session would have been refused by the
  // login; since the sign-in nothing was.
  const refusedBefore = login.refused();
  expect(refusedBefore).toEqual(["/api/state", "/app/"]);

  // The service worker reaches the active state, scoped to the app, and the
  // server serves its file to be revalidated.
  const workerUrl = `${login.origin}/app/sw.js`;
  const active = await page.evaluate(async () => {
    const registration = await navigator.serviceWorker.ready;
    const worker = registration.active;
    while (worker.state !== "activated") {
      await new Promise((resolve) => worker.addEventListener("statechange", resolve, { once: true }));
    }
    return { state: worker.state, script: worker.scriptURL, scope: registration.scope };
  });
  expect(active).toEqual({ state: "activated", script: workerUrl, scope: `${login.origin}/app/` });
  const workerFile = await page.request.get(workerUrl);
  expect(workerFile.headers()["cache-control"]).toBe("no-cache");

  // Loaded again, the page is the worker's: every request it makes from here
  // on, the API's included, passes the worker's fetch handler.
  await page.reload();
  expect(await page.evaluate(() => navigator.serviceWorker.controller?.scriptURL)).toBe(workerUrl);
  await expect(rooms.getByRole("heading", { level: 2 })).toHaveText([NAMED.name, "den"]);
  // It has read the state; now an event: another client renames a room, and
  // the page shows it with no reload.
  await command({ v: 1, t: "name", zone: "den", name: "Smoke Test Den" });
  await expect(rooms.getByRole("heading", { level: 2 })).toHaveText([NAMED.name, "Smoke Test Den"]);
  // And a command of the page's own, answered by the server through the worker.
  await rooms.getByLabel("Mute Smoke Test Den").click();
  await expect(rooms.getByLabel("Mute Smoke Test Den")).toHaveAttribute("aria-pressed", "true");

  // What the caches hold after that: the files of the app's output and
  // nothing else. No entry of any cache is under /api/.
  const appUrl = (file) => `${login.origin}/app/${file === "index.html" ? "" : file}`;
  const shell = OUTPUT.filter((file) => file !== "sw.js").map(appUrl).sort();
  expect(shell.length).toBeGreaterThan(3);
  const cached = await readCaches(page);
  expect(cached.filter((entry) => new URL(entry.url).pathname.startsWith("/api/"))).toEqual([]);
  expect(cached.filter((entry) => entry.url.includes("/api/"))).toEqual([]);
  expect(cached.map((entry) => entry.url).sort()).toEqual(shell);
  expect(new Set(cached.map((entry) => entry.cache)).size).toBe(1);
  for (const entry of cached) {
    expect({ url: entry.url, status: entry.status, type: entry.type, redirected: entry.redirected }).toEqual({
      url: entry.url,
      status: 200,
      type: "basic",
      redirected: false,
    });
  }
  expect(login.refused(), "requests that came without the session").toEqual(refusedBefore);
  expect(violations, "Content-Security-Policy violations the page reported").toEqual([]);
  expect(errors, "errors the page logged or threw").toEqual([]);

  // The login expires. The page's event stream ends, its next attempt is
  // redirected to the login, and the page says so, with the way to sign in
  // and the rooms it last knew.
  login.expire();
  await expect(page.getByText("Signed out.")).toBeVisible();
  const signIn = page.getByRole("link", { name: "Sign in" });
  await expect(signIn).toBeVisible();
  await expect(rooms.getByRole("heading", { level: 2 })).toHaveText([NAMED.name, "Smoke Test Den"]);
  expect(login.refused()).toContain("/api/events");

  // Signing in is a navigation: it passes the worker, which hands the
  // login's redirect to the browser and keeps nothing of it.
  await signIn.click();
  await expect(page).toHaveURL(`${login.origin}${LOGIN_PATH}?rd=${encodeURIComponent("/app/")}`);
  await expect(page.getByRole("heading", { name: "Sign in" })).toBeVisible();
  // The login page is in no cache: the entries are the ones from before, the
  // document is still the app's, and none says what the login page says.
  const after = await readCaches(page);
  expect(after.map((entry) => entry.url).sort()).toEqual(shell);
  expect(after.filter((entry) => entry.redirected || entry.status !== 200 || entry.type !== "basic")).toEqual([]);
  expect(after.filter((entry) => entry.url.includes(LOGIN_PATH) || entry.text.includes("Sign in (fake)"))).toEqual([]);
  expect(after.find((entry) => entry.url === appUrl("index.html")).text).toContain("<chorus-app>");

  // Signed in again, the app is back with the server's rooms.
  await page.getByLabel("Name").fill("smoke again");
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page).toHaveURL(`${login.origin}/app/`);
  await expect(rooms.getByRole("heading", { level: 2 })).toHaveText([NAMED.name, "Smoke Test Den"]);
  await expect(page.getByText("Signed out.")).toHaveCount(0);
  expect(login.users()).toEqual(["smoke again"]);
  expect(violations, "Content-Security-Policy violations the page reported").toEqual([]);

  // The count above is of a listener that hears: an inline style, which the
  // policy forbids, is reported.
  await page.evaluate(() => {
    const style = document.createElement("style");
    style.textContent = "chorus-app { display: block; }";
    document.head.append(style);
  });
  await expect.poll(() => violations).toEqual(["style-src-elem blocked inline"]);
});
