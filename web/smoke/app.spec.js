// The one browser test (`make web-smoke`, gate step `web-smoke`): headless
// Chromium loads the app from a real chorus-server, through the fake login of
// fake-login.js, and what it asserts is text the page rendered. Its tail is
// the install and the service worker: the manifest loads through the login,
// the worker becomes active and controls the page, no cache holds anything of
// the API after the page has read state and events through it, and with the
// login expired the page says "Signed out" and the login page is in no cache
// (docs/decisions/0190-the-app-installs-behind-the-login.md).
//
// The second test is the screens beyond the home (a room's sound, limits,
// theater and correction, the autoplay rules, the alarms, the speakers and
// the Wi-Fi walk-through): each is opened by the app's own links, one sound
// setting is changed and read back from the server's /api/state, and the
// correction screen asks for the microphone and is given Chromium's fake
// audio device, records while the server plays its sweep and uploads the
// recording. After all of it the worker has answered no request under /api/,
// the upload included, and no cache holds one. Its speaker is a scripted
// session (../live/endpoint.js): no device, no microphone and no room.
//
// The tests after it are the layouts, which only a browser can lay out
// (docs/decisions/0191-phone-and-desktop-layouts-and-the-kiosk.md): the phone
// layout at a phone's width, the desktop layout at a desktop's, the breakpoint
// between them, and the kiosk of a wall tablet (its switch, its wake lock, its
// touch targets and its memory). They run after the first two, in this file's
// order, against the same server, each in a browser context of its own.
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

import { offerLineIn } from "../live/endpoint.js";
import { WANTED } from "../src/capture.js";
import { DESKTOP_MIN_EM } from "../src/layout.js";
import { LOGIN_PATH, startFakeLogin } from "./fake-login.js";

// Chromium's fake audio device in the microphone's place, and its permission
// prompt answered "allow" (the correction screen, in the second test). They
// are the browser's own switches for a test: `getUserMedia` is the real one,
// and what it gives is a track of a device named "Fake ...".
test.use({ launchOptions: { args: ["--use-fake-device-for-media-stream", "--use-fake-ui-for-media-stream"] } });

// The test server's configuration: two rooms, one of them given a name its id
// does not hold, so the text asserted below can only be the server's.
const ROOMS = ["kitchen", "den"];
const NAMED = { id: "kitchen", name: "Smoke Test Kitchen" };

const LISTENING = /control listening on=\S*?:(\d+)/;
// Where the server takes its speakers' sessions.
const AUDIO_LISTENING = /chorus-server: listening on=\S*?:(\d+)/;
// The hub of the second test: a scripted session that is a speaker of the
// kitchen and offers a TV's optical input.
const HUB = "smoke-test-hub";

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
let hub;

// The files of the app as the page asks for them, sw.js aside: what the
// worker's one cache holds, and all it holds.
const appUrl = (file) => `${login.origin}/app/${file === "index.html" ? "" : file}`;
const shellOf = () => OUTPUT.filter((file) => file !== "sw.js").map(appUrl).sort();

// The server's own state, read the way any other client reads it.
async function serverState() {
  const answer = await fetch(`${serverOrigin}/api/state`);
  expect(answer.status).toBe(200);
  return answer.json();
}
const zoneOf = async (id) => (await serverState()).zones.find((zone) => zone.id === id);

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
    // A server lists an endpoint's input, and plays the measurement sweep
    // beside its slots, only when it has slots (as ../live starts it).
    "--slots", "4",
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
  if (hub) await hub.stop();
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
  const shell = shellOf();
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

// The screens beyond the home
// (docs/decisions/0197-further-screens-have-an-address-in-the-fragment.md).

test("the further screens open by the app's own links, a sound setting set on one is the server's, the correction screen records the sweep with the browser's fake microphone, and the worker answers nothing of the API, the upload included", async ({ page }) => {
  // The sweep alone is 6.5 s, and the recording goes on a second past it.
  test.setTimeout(120_000);
  const reported = await watch(page);
  // Every answer to a request under /api/ the page gets from here on: what
  // was asked, and whether a service worker's fetch handler gave the answer.
  const api = [];
  // The same for the app's own files, which the worker does answer: the
  // count that shows the browser's flag is one that is ever set.
  let appFromWorker = 0;
  page.on("response", (response) => {
    const { pathname } = new URL(response.url());
    if (pathname.startsWith("/app/") && response.fromServiceWorker()) appFromWorker += 1;
    if (!pathname.startsWith("/api/")) return;
    api.push({ asked: `${response.request().method()} ${pathname}`, status: response.status(), worker: response.fromServiceWorker() });
  });
  // What the page asks `getUserMedia` for and the tracks it is given, noted
  // on the way through: the call and its answer are the browser's own.
  await page.addInitScript(() => {
    window.chorusSmokeMicrophone = [];
    window.chorusSmokeTracks = [];
    const devices = navigator.mediaDevices;
    const ask = devices.getUserMedia.bind(devices);
    devices.getUserMedia = async (constraints) => {
      const stream = await ask(constraints);
      window.chorusSmokeTracks.push(...stream.getTracks());
      window.chorusSmokeMicrophone.push({
        constraints,
        tracks: stream.getTracks().map((track) => ({ kind: track.kind, label: track.label })),
      });
      return stream;
    };
  });

  // Signed in, with the worker in control: every request from here on passes
  // its fetch handler, as in the first test.
  await open(page);
  await page.evaluate(async () => {
    const worker = (await navigator.serviceWorker.ready).active;
    while (worker.state !== "activated") {
      await new Promise((resolve) => worker.addEventListener("statechange", resolve, { once: true }));
    }
  });
  await page.reload();
  expect(await page.evaluate(() => navigator.serviceWorker.controller?.scriptURL)).toBe(`${login.origin}/app/sw.js`);
  const rooms = page.getByRole("main", { name: "Rooms" });
  await expect(rooms.getByRole("heading", { level: 2 })).toHaveCount(ROOMS.length);
  const kitchen = (await zoneOf(NAMED.id)).name;
  expect(kitchen).toBe(NAMED.name);

  // A link on a card or under the rooms opens its screen: the address is the
  // screen's own, and the main region is the screen, by its title.
  const openScreen = async (link, address, title) => {
    await expect(link).toHaveAttribute("href", address);
    await link.click();
    await expect(page).toHaveURL(`${login.origin}/app/${address}`);
    const screen = page.getByRole("main", { name: title, exact: true });
    await expect(screen).toBeVisible();
    await expect(rooms).toHaveCount(0);
    return screen;
  };
  // "Back" on a screen opened from the home is the home again.
  const back = async () => {
    await page.getByRole("link", { name: "Back to rooms", exact: true }).click();
    await expect(rooms.getByRole("heading", { level: 2 })).toHaveCount(ROOMS.length);
  };
  const onCard = (name) => rooms.getByRole("link", { name: `${name} for ${kitchen}`, exact: true });

  // The theater screen is offered where there is a TV input: none yet.
  await expect(onCard("Sound")).toBeVisible();
  await expect(onCard("Theater")).toHaveCount(0);
  // The hub: a speaker's session that offers a TV's optical input, put in the
  // kitchen the way a speaker is. Its card then has the link, with no reload.
  const [, audioPort] = AUDIO_LISTENING.exec(serverLog);
  hub = await offerLineIn({ host: "127.0.0.1", port: Number(audioPort), endpoint: HUB, name: "tv", kind: "optical" });
  await expect.poll(async () => (await serverState()).input_kinds).toEqual([{ input: `${HUB}/tv`, kind: "optical", tv: true }]);
  await command({ v: 2, t: "speaker_room", speaker: HUB, room: NAMED.id });
  await expect(onCard("Theater")).toBeVisible();
  await expect(rooms.getByRole("link", { name: /^Theater for / })).toHaveCount(1);

  // A room's sound: one switch and one slider, each read back from the
  // server's own state, and from the screen.
  const sound = await openScreen(onCard("Sound"), "#/rooms/kitchen/sound", `Sound of ${kitchen}`);
  await expect(sound.getByRole("heading", { level: 2 })).toHaveText(`Sound of ${kitchen}`);
  // The switch is pressed to the opposite of what the server holds.
  const before = (await zoneOf(NAMED.id)).sound;
  expect(before.bass).toBe(0);
  const loudness = sound.getByRole("button", { name: `Loudness for ${kitchen}`, exact: true });
  await expect(loudness).toHaveAttribute("aria-pressed", String(before.loudness));
  await expect(sound.locator('[data-value="loudness"]')).toHaveText(before.loudness ? "On" : "Off");
  await loudness.click();
  await expect(loudness).toHaveAttribute("aria-pressed", String(!before.loudness));
  await expect(sound.locator('[data-value="loudness"]')).toHaveText(before.loudness ? "Off" : "On");
  expect((await zoneOf(NAMED.id)).sound).toEqual({ ...before, loudness: !before.loudness });
  await sound.getByRole("slider", { name: `Bass for ${kitchen}`, exact: true }).fill("3");
  await expect.poll(async () => (await zoneOf(NAMED.id)).sound.bass).toBe(3);
  await expect(sound.locator('[data-value="bass"]')).toHaveText("+3 dB");
  expect((await zoneOf(NAMED.id)).sound).toEqual({ ...before, loudness: !before.loudness, bass: 3 });
  expect((await zoneOf("den")).sound).toEqual(before);
  await expect(sound.getByRole("alert")).toHaveText("");
  await back();

  // Its limits.
  const limits = await openScreen(onCard("Limits"), "#/rooms/kitchen/limits", `Volume limits of ${kitchen}`);
  await expect(limits.getByRole("slider", { name: `Volume limit for ${kitchen}`, exact: true })).toBeVisible();
  await expect(limits.getByRole("button", { name: `Quiet hours for ${kitchen}`, exact: true })).toBeVisible();
  await back();

  // Its theater settings, with the hub's input as the TV's.
  const theater = await openScreen(onCard("Theater"), "#/rooms/kitchen/theater", `Theater of ${kitchen}`);
  await expect(theater.getByRole("slider", { name: `A/V trim for ${kitchen}`, exact: true })).toBeVisible();
  await expect(theater.getByRole("list", { name: "TV inputs" })).toContainText("Optical");
  await back();

  // The house's screens, under the rooms: autoplay, alarms, speakers.
  const autoplay = await openScreen(page.getByRole("link", { name: "Autoplay rules", exact: true }), "#/autoplay", "Autoplay");
  await expect(autoplay.getByRole("list", { name: "Inputs" })).toContainText(`${HUB}/tv`);
  await back();
  const alarms = await openScreen(page.getByRole("link", { name: "Alarms and sleep timers", exact: true }), "#/alarms", "Alarms and sleep timers");
  await expect(alarms.getByRole("heading", { level: 2 })).toHaveText(["Alarms", "Stored sources", "Sleep timers"]);
  await expect(alarms.getByRole("button", { name: "Save alarm", exact: true })).toBeVisible();
  await back();
  const speakers = await openScreen(page.getByRole("link", { name: "Speakers and their setup", exact: true }), "#/speakers", "Speakers");
  await expect(speakers.getByRole("list", { name: "Adopted speakers" })).toContainText(HUB);
  await expect(speakers.getByRole("list", { name: "Adopted speakers" })).toContainText("Connected");
  // And from the speakers, the walk-through for a Wi-Fi speaker.
  const setup = await openScreen(
    speakers.getByRole("link", { name: "Set up a Wi-Fi speaker", exact: true }),
    "#/speakers/setup",
    "Set up a Wi-Fi speaker",
  );
  await expect(setup.getByRole("list", { name: "Steps" }).getByRole("listitem")).toHaveCount(4);
  await page.goBack();
  await expect(speakers).toBeVisible();
  await back();

  // The room's correction. Nothing is asked of the browser by opening it.
  const correction = await openScreen(onCard("Correction"), "#/rooms/kitchen/correction", `Correction of ${kitchen}`);
  await expect(correction.locator("[data-guide]").getByRole("listitem")).toHaveCount(5);
  await expect(correction.locator("[data-held]")).toHaveText("This room has no correction.");
  expect(await page.evaluate(() => window.chorusSmokeMicrophone)).toEqual([]);
  // "Use the microphone": the page asks for it with the three kinds of
  // processing off and one channel, and the browser gives its fake device.
  await correction.getByRole("button", { name: `Use the microphone to measure ${kitchen}`, exact: true }).click();
  await expect(correction.getByRole("heading", { name: "What the browser granted" })).toBeVisible();
  expect(await page.evaluate(() => window.chorusSmokeMicrophone)).toEqual([
    { constraints: { audio: { ...WANTED }, video: false }, tracks: [{ kind: "audio", label: expect.stringMatching(/^Fake /) }] },
  ]);
  // "What the browser granted" is the track's own settings, line for line:
  // the processing off as asked, and the fake device's channels and rate.
  const settings = await page.evaluate(() => window.chorusSmokeTracks[0].getSettings());
  expect(settings).toMatchObject({ echoCancellation: false, noiseSuppression: false, autoGainControl: false });
  const granted = correction.locator("[data-granted]");
  for (const processing of ["echoCancellation", "noiseSuppression", "autoGainControl"]) {
    await expect(granted.locator(`[data-setting="${processing}"]`)).toHaveText("off, as asked");
  }
  await expect(granted.locator('[data-setting="channelCount"]')).toHaveText(String(settings.channelCount));
  await expect(granted.locator('[data-setting="sampleRate"]')).toHaveText(`${settings.sampleRate} Hz`);
  await expect(granted.locator('[data-setting="recordedAt"]')).toHaveText("48000 Hz");
  await expect(correction.locator("[data-flag]")).toHaveCount(0);
  console.log(`web-smoke: the fake microphone's settings: ${JSON.stringify(settings)}`);
  expect(await page.evaluate(() => window.chorusSmokeTracks.map((track) => track.readyState))).toEqual(["live"]);

  // "Play the sweep and record": the server plays its sweep to the hub, the
  // fake device is recorded, and the recording is uploaded and answered. What
  // the fake device makes is a beep and not the sweep, so the answer is the
  // fitter's own (a refusal by its name, or a fit of what it heard): either
  // way it is the server's, through the upload route, and nothing is applied.
  await correction.getByRole("button", { name: `Play the sweep in ${kitchen} and record`, exact: true }).click();
  await expect(correction.locator('[data-phase="recording"]')).toBeVisible();
  const outcome = correction.locator('[data-phase="proposed"], [data-phase="refused"], [data-phase="failed"]');
  await expect(outcome).toBeVisible({ timeout: 30_000 });
  const phase = await outcome.getAttribute("data-phase");
  console.log(`web-smoke: the measurement with the fake microphone ended "${phase}": ${(await outcome.textContent()).replace(/\s+/g, " ").trim()}`);
  expect(["proposed", "refused"], `the measurement ended "${phase}": ${await outcome.textContent()}`).toContain(phase);
  const state = await serverState();
  expect(state.measurement).toMatchObject({ zone: NAMED.id, state: "finished", sweep_ms: 5000 });
  expect(state.zones.find((zone) => zone.id === NAMED.id).room_eq).toEqual({ enabled: true, filters: [] });
  await expect(correction.locator("[data-held]")).toHaveText("This room has no correction.");
  const upload = api.filter((answer) => answer.asked === "POST /api/room-fit");
  expect(upload).toHaveLength(1);
  if (phase === "refused") {
    await expect(outcome).toHaveText(/^\s*The server refused the recording: \S/);
    await expect(correction.getByRole("button", { name: `Apply the proposed correction to ${kitchen}`, exact: true })).toHaveCount(0);
    expect(upload[0].status).toBe(422);
  } else {
    expect(upload[0].status).toBe(200);
  }
  // The microphone was let go.
  expect(await page.evaluate(() => window.chorusSmokeTracks.map((track) => track.readyState))).toEqual(["ended"]);
  await back();

  // The worker answered none of it: every answer under /api/ since the page
  // loaded was the network's, the state, the events, the commands and the
  // upload among them.
  expect(api.filter((answer) => answer.worker)).toEqual([]);
  expect(appFromWorker, "answers the worker gave for the app's own files").toBeGreaterThan(0);
  expect([...new Set(api.map((answer) => answer.asked))]).toEqual(
    expect.arrayContaining(["GET /api/state", "GET /api/events", "POST /api/command", "POST /api/room-fit"]),
  );
  // And it kept none of it: the caches hold the files of the app's output
  // and nothing else, with no entry under /api/.
  const cached = await readCaches(page);
  expect(cached.filter((entry) => entry.url.includes("/api/"))).toEqual([]);
  expect(cached.map((entry) => entry.url).sort()).toEqual(shellOf());
  expect(cached.filter((entry) => entry.status !== 200 || entry.type !== "basic" || entry.redirected)).toEqual([]);

  // The hub goes, and its input and the card's theater link with it: the
  // tests after this one have the rooms as they were.
  await hub.stop();
  hub = null;
  await expect(onCard("Theater")).toHaveCount(0);

  expect(reported.violations, "Content-Security-Policy violations the page reported").toEqual([]);
  // The one error the browser may log is its own line for the refused
  // upload, which the server answers 422; the page threw nothing.
  expect(reported.errors, "errors the page logged or threw").toEqual(
    phase === "refused" ? ["Failed to load resource: the server responded with a status of 422 (Unprocessable Entity)"] : [],
  );
});

// The layouts and the kiosk
// (docs/decisions/0191-phone-and-desktop-layouts-and-the-kiosk.md).

// A phone, a desktop and a wall tablet, in CSS pixels.
const PHONE = { width: 390, height: 844 };
const DESKTOP = { width: 1280, height: 800 };
const TABLET = { width: 1024, height: 768 };
// The breakpoint in CSS pixels: an em of a media query is the browser's
// initial font size, 16 CSS pixels here.
const BREAKPOINT = DESKTOP_MIN_EM * 16;
// The least border box of a control (web/src/tokens.css: --control-size) and
// of a control of the kiosk (web/src/app.css: --kiosk-control-size).
const CONTROL_MIN = 44;
const KIOSK_CONTROL_MIN = 64;

// Collect what the page reports wrong, as the first test does.
async function watch(page) {
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
  return { violations, errors };
}

// Open the app at `address` (a path under the login) in a browser that is not
// signed in yet, sign in, and wait for the server's rooms.
async function open(page, address = "/app/") {
  await page.goto(`${login.origin}${address}`);
  await expect(page).toHaveURL(`${login.origin}${LOGIN_PATH}?rd=${encodeURIComponent(address)}`);
  await page.getByLabel("Name").fill("layout");
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page).toHaveURL(`${login.origin}${address}`);
  await expect(page.getByRole("main", { name: "Rooms" }).getByRole("heading", { level: 2 })).toHaveCount(ROOMS.length);
}

// The two regions and the navigation, with the boxes a browser gave them.
function regions(page) {
  return {
    app: page.locator("chorus-app"),
    groups: page.getByRole("region", { name: "Groups" }),
    rooms: page.getByRole("main", { name: "Rooms" }),
    nav: page.getByRole("navigation", { name: "Sections" }),
  };
}

// One column: the groups, then the rooms under them, each as wide as the page.
async function expectOneColumn(page) {
  const { app, groups, rooms } = regions(page);
  await expect(app).toHaveAttribute("layout", "phone");
  const width = page.viewportSize().width;
  const above = await groups.boundingBox();
  const below = await rooms.boundingBox();
  expect(above.x).toBe(0);
  expect(below.x).toBe(0);
  expect(above.width).toBe(width);
  expect(below.width).toBe(width);
  expect(below.y).toBeGreaterThanOrEqual(above.y + above.height);
}

// Two columns: the groups beside the rooms, their tops level, neither over the other.
async function expectTwoColumns(page) {
  const { app, groups, rooms } = regions(page);
  await expect(app).toHaveAttribute("layout", "desktop");
  const left = await groups.boundingBox();
  const right = await rooms.boundingBox();
  expect(right.y).toBe(left.y);
  expect(right.x).toBeGreaterThanOrEqual(left.x + left.width);
  expect(left.width).toBeGreaterThanOrEqual(320);
  expect(right.width).toBeGreaterThanOrEqual(left.width);
  expect(left.x + left.width + right.width).toBeLessThanOrEqual(page.viewportSize().width);
}

// The page is no wider than its viewport: nothing scrolls sideways.
async function expectNoSidewaysScroll(page) {
  const wide = await page.evaluate(() => ({
    content: document.documentElement.scrollWidth,
    viewport: document.documentElement.clientWidth,
  }));
  expect(wide.content).toBeLessThanOrEqual(wide.viewport);
}

// Every control of the page a person can touch (button, link, input, select;
// shadow roots included) that is drawn, with its label and the box it has.
function touchTargets(page) {
  return page.evaluate(() => {
    const found = [];
    const visit = (root) => {
      for (const element of root.querySelectorAll("*")) {
        if (element.shadowRoot) visit(element.shadowRoot);
        if (!element.matches("button, a[href], input, select, textarea, summary")) continue;
        if (!element.checkVisibility({ visibilityProperty: true })) continue;
        const box = element.getBoundingClientRect();
        found.push({
          control: `${element.localName} "${element.getAttribute("aria-label") ?? element.textContent.trim()}"`,
          width: Math.round(box.width * 100) / 100,
          height: Math.round(box.height * 100) / 100,
        });
      }
    };
    visit(document);
    return found;
  });
}

async function expectTouchTargets(page, least, atLeast) {
  const targets = await touchTargets(page);
  expect(targets.length, "controls drawn on the page").toBeGreaterThanOrEqual(atLeast);
  expect(
    targets.filter((target) => target.width < least || target.height < least),
    `controls smaller than ${least} by ${least} CSS pixels`,
  ).toEqual([]);
}

test("the phone layout at a phone width: one column, and the navigation at the bottom edge, reachable by thumb", async ({ browser }) => {
  // A phone: its width, and a screen that is touched.
  const context = await browser.newContext({ viewport: PHONE, hasTouch: true });
  const page = await context.newPage();
  const reported = await watch(page);
  await open(page);
  const { rooms, groups, nav } = regions(page);

  await expectOneColumn(page);
  await expectNoSidewaysScroll(page);
  // The wordmark is at the top; the navigation is not: it is a bar across the
  // bottom edge of the screen, in its lowest fifth.
  await expect(page.getByRole("heading", { name: "chorus" })).toBeVisible();
  const bar = await nav.boundingBox();
  expect(bar.x).toBe(0);
  expect(bar.width).toBe(PHONE.width);
  expect(bar.y + bar.height).toBe(PHONE.height);
  expect(bar.y).toBeGreaterThan(PHONE.height * 0.8);
  await expect(nav).toHaveCSS("position", "fixed");
  // Its buttons share the bar's width, each a touch target, and so is every
  // other control of the page.
  const buttons = nav.getByRole("button");
  await expect(buttons).toHaveText(["Groups", "Rooms"]);
  for (const button of await buttons.all()) {
    const box = await button.boundingBox();
    expect(box.height).toBeGreaterThanOrEqual(CONTROL_MIN);
    expect(box.width).toBeGreaterThan(PHONE.width / 3);
    expect(box.y).toBeGreaterThan(PHONE.height * 0.8);
  }
  await expectTouchTargets(page, CONTROL_MIN, 6);

  // The bar covers nothing: scrolled to the end, the last room's card ends above it.
  await page.evaluate(() => window.scrollTo(0, document.documentElement.scrollHeight));
  const last = await rooms.locator("chorus-room-card").last().boundingBox();
  expect(last.y + last.height).toBeLessThanOrEqual((await nav.boundingBox()).y);
  expect((await nav.boundingBox()).y + bar.height).toBe(PHONE.height);

  // It navigates: a tap takes the focus to the region it names, in view.
  await nav.getByRole("button", { name: "Go to rooms" }).tap();
  await expect(rooms).toBeFocused();
  await expect(rooms).toBeInViewport();
  await nav.getByRole("button", { name: "Go to groups" }).tap();
  await expect(groups).toBeFocused();
  await expect(groups).toBeInViewport();

  expect(reported.violations, "Content-Security-Policy violations the page reported").toEqual([]);
  expect(reported.errors, "errors the page logged or threw").toEqual([]);
  await context.close();
});

test("the desktop layout at a desktop width: the rooms and what is playing side by side, the navigation in the header", async ({ page }) => {
  const reported = await watch(page);
  // Two rooms play together, so the groups region has a group that says what it plays.
  await command({ v: 2, t: "join", zone: "den", target: "kitchen" });
  await page.setViewportSize(DESKTOP);
  await open(page);
  const { rooms, groups, nav } = regions(page);

  await expectTwoColumns(page);
  await expectNoSidewaysScroll(page);
  // What is playing, in the group's card, is level with the rooms' cards and
  // beside them: both are in view at once with no scrolling.
  const playing = groups.locator("chorus-playing").first();
  await expect(playing).toBeVisible();
  await expect(playing).toBeInViewport();
  const card = rooms.locator("chorus-room-card").first();
  await expect(card).toBeInViewport();
  const playingBox = await playing.boundingBox();
  const cardBox = await card.boundingBox();
  expect(playingBox.x + playingBox.width).toBeLessThanOrEqual(cardBox.x);
  expect(playingBox.y).toBeLessThan(cardBox.y + cardBox.height);
  expect(playingBox.y + playingBox.height).toBeGreaterThan(cardBox.y);

  // The navigation is in the header, at the top, beside the wordmark; it is not a bar at the bottom.
  const heading = await page.getByRole("heading", { name: "chorus" }).boundingBox();
  const bar = await nav.boundingBox();
  await expect(nav).not.toHaveCSS("position", "fixed");
  expect(bar.y + bar.height).toBeLessThan((await rooms.boundingBox()).y + 1);
  expect(bar.x).toBeGreaterThan(heading.x + heading.width);
  expect(bar.y).toBeLessThan(heading.y + heading.height);
  await expectTouchTargets(page, CONTROL_MIN, 6);
  await nav.getByRole("button", { name: "Go to rooms" }).click();
  await expect(rooms).toBeFocused();

  expect(reported.violations, "Content-Security-Policy violations the page reported").toEqual([]);
  expect(reported.errors, "errors the page logged or threw").toEqual([]);
});

test("the breakpoint between the phone and the desktop layout: one column below 48em, two columns from it, followed as the window is resized", async ({ page }) => {
  const reported = await watch(page);
  expect(BREAKPOINT).toBe(768);
  // One pixel under the breakpoint.
  await page.setViewportSize({ width: BREAKPOINT - 1, height: 900 });
  await open(page);
  expect(await page.evaluate((query) => matchMedia(query).matches, `(min-width: ${DESKTOP_MIN_EM}em)`)).toBe(false);
  await expectOneColumn(page);
  await expect(regions(page).nav).toHaveCSS("position", "fixed");
  await expectNoSidewaysScroll(page);

  // On it, with no reload.
  await page.setViewportSize({ width: BREAKPOINT, height: 900 });
  await expectTwoColumns(page);
  await expect(regions(page).nav).not.toHaveCSS("position", "fixed");
  await expectNoSidewaysScroll(page);

  // And back under it.
  await page.setViewportSize({ width: BREAKPOINT - 1, height: 900 });
  await expectOneColumn(page);

  // Loaded on the breakpoint, it is the desktop layout from the start.
  await page.setViewportSize({ width: BREAKPOINT, height: 900 });
  await page.reload();
  await expect(regions(page).rooms.getByRole("heading", { level: 2 })).toHaveCount(ROOMS.length);
  await expectTwoColumns(page);

  expect(reported.violations, "Content-Security-Policy violations the page reported").toEqual([]);
  expect(reported.errors, "errors the page logged or threw").toEqual([]);
});

test("kiosk mode: entered by ?kiosk, it asks for a screen wake lock and asks again when the page is visible again, its touch targets are at least 64 CSS pixels, and it survives a reload", async ({ page }) => {
  const reported = await watch(page);
  // The headless browser has the Screen Wake Lock API and refuses every
  // request of it (NotAllowedError: it has no screen to keep on; seen here
  // 2026-10-05, with the permission granted over the DevTools protocol too).
  // So the test stands in for that one object, after noting the browser's own
  // is there: `navigator.wakeLock` records each request and answers it with a
  // sentinel that is released, with its `release` event, as the browser's is.
  // The page's own code (web/src/wake-lock.js) is what runs against it.
  let asked = [];
  await page.exposeFunction("chorusSmokeWakeLock", (type) => asked.push(type));
  await page.addInitScript(() => {
    window.chorusSmokeOwnWakeLock = window.isSecureContext && typeof navigator.wakeLock?.request === "function";
    window.chorusSmokeSentinels = [];
    const standIn = {
      async request(type) {
        const sentinel = new EventTarget();
        sentinel.type = type;
        sentinel.released = false;
        sentinel.release = async () => {
          if (sentinel.released) return;
          sentinel.released = true;
          sentinel.dispatchEvent(new Event("release"));
        };
        window.chorusSmokeSentinels.push(sentinel);
        window.chorusSmokeWakeLock(type);
        return sentinel;
      },
    };
    Object.defineProperty(Navigator.prototype, "wakeLock", { configurable: true, get: () => standIn });
  });
  // The page hidden, as the browser makes it: its wake locks are released
  // first, then it is told. And shown again.
  const setVisibility = (state) =>
    page.evaluate(async (to) => {
      if (to === "hidden") for (const sentinel of window.chorusSmokeSentinels) await sentinel.release();
      Object.defineProperty(document, "visibilityState", { configurable: true, get: () => to });
      Object.defineProperty(document, "hidden", { configurable: true, get: () => to === "hidden" });
      document.dispatchEvent(new Event("visibilitychange"));
    }, state);

  await page.setViewportSize(TABLET);
  await open(page, "/app/?kiosk");
  const { app, rooms, groups } = regions(page);

  // Its switch is the address; nothing of the app is drawn around the rooms.
  await expect(app).toHaveAttribute("mode", "kiosk");
  await expect(page.getByRole("heading", { name: "chorus" })).toBeHidden();
  await expect(regions(page).nav).toBeHidden();
  await expectTwoColumns(page);
  expect((await groups.boundingBox()).y).toBe(0);
  await expectNoSidewaysScroll(page);

  // This is a secure context and the browser has the API; the page asked for a screen wake lock, once.
  expect(await page.evaluate(() => window.chorusSmokeOwnWakeLock)).toBe(true);
  await expect.poll(() => asked).toEqual(["screen"]);
  expect(await page.evaluate(() => window.chorusSmokeSentinels.map((sentinel) => sentinel.released))).toEqual([false]);
  // Hidden, it asks for nothing; visible again, it asks again and holds the new one.
  await setVisibility("hidden");
  expect(await page.evaluate(() => window.chorusSmokeSentinels.map((sentinel) => sentinel.released))).toEqual([true]);
  expect(asked).toEqual(["screen"]);
  await setVisibility("visible");
  await expect.poll(() => asked).toEqual(["screen", "screen"]);
  expect(await page.evaluate(() => window.chorusSmokeSentinels.map((sentinel) => sentinel.released))).toEqual([true, false]);

  // Every control is a kiosk's touch target, and the text is the kiosk's size.
  await expectTouchTargets(page, KIOSK_CONTROL_MIN, 6);
  await expect(rooms.getByRole("heading", { level: 2 }).first()).toHaveCSS("font-size", "20px");

  // It survives a reload, and the kiosk asks for its wake lock again.
  asked = [];
  await page.reload();
  await expect(app).toHaveAttribute("mode", "kiosk");
  await expect(rooms.getByRole("heading", { level: 2 })).toHaveCount(ROOMS.length);
  await expect.poll(() => asked).toEqual(["screen"]);
  await expectTouchTargets(page, KIOSK_CONTROL_MIN, 6);
  // And the app's plain address, which is where the installed app starts: the choice was kept.
  asked = [];
  await page.goto(`${login.origin}/app/`);
  await expect(app).toHaveAttribute("mode", "kiosk");
  await expect(page.getByRole("heading", { name: "chorus" })).toBeHidden();
  await expect.poll(() => asked).toEqual(["screen"]);

  // A narrow kiosk keeps the navigation at the bottom edge, at the kiosk's size.
  await page.setViewportSize(PHONE);
  await expectOneColumn(page);
  await expect(regions(page).nav).toBeVisible();
  await expectTouchTargets(page, KIOSK_CONTROL_MIN, 8);
  await expectNoSidewaysScroll(page);
  await page.setViewportSize(TABLET);

  // ?kiosk=0 leaves it, and that is kept too: the ordinary app asks for no wake lock.
  asked = [];
  await page.goto(`${login.origin}/app/?kiosk=0`);
  await expect(app).toHaveAttribute("mode", "app");
  await expect(page.getByRole("heading", { name: "chorus" })).toBeVisible();
  await page.goto(`${login.origin}/app/`);
  await expect(app).toHaveAttribute("mode", "app");
  await expect(rooms.getByRole("heading", { level: 2 })).toHaveCount(ROOMS.length);
  await setVisibility("hidden");
  await setVisibility("visible");
  expect(asked).toEqual([]);

  expect(reported.violations, "Content-Security-Policy violations the page reported").toEqual([]);
  expect(reported.errors, "errors the page logged or threw").toEqual([]);
});
