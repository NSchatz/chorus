// The signed-out state: the household login in front of the server has
// lapsed, and what it answers in the server's place (a redirect to its page,
// or a 401) is told apart from the server, by the API client (src/api.js),
// the store (src/state.js) and the shell (src/chorus-app.js), which says
// "Signed out" with the way to sign in.

import assert from "node:assert/strict";
import { afterEach, test } from "node:test";

import { RETRY_MS, createClient, muteCommand, signedOut } from "../src/api.js";
import "../src/chorus-app.js";
import { createStore } from "../src/state.js";
import { fakeServer, fakeTimers, settle, stateOf, zone } from "./fake-server.js";
import { getByLabel, queryAllByLabel } from "./label-query.js";

let stores = [];

afterEach(() => {
  for (const store of stores) store.stop();
  stores = [];
  document.body.replaceChildren();
});

function clientOf(server, timers = fakeTimers()) {
  return createClient({ fetch: server.fetch, base: server.base, timers });
}

async function mountOver(server, timers = fakeTimers()) {
  const store = createStore(clientOf(server, timers));
  stores.push(store);
  const app = document.createElement("chorus-app");
  app.store = store;
  document.body.append(app);
  store.start();
  await rendered(app);
  return app;
}

async function rendered(app) {
  await settle();
  await app.updateComplete;
  const screen = app.shadowRoot.querySelector("chorus-rooms");
  await screen.updateComplete;
  await Promise.all([...screen.shadowRoot.querySelectorAll("chorus-room-card")].map((card) => card.updateComplete));
}

const banner = (app) => app.shadowRoot.querySelector("[data-signed-out]");
const words = (element) => element.textContent.replace(/\s+/g, " ").trim();
const headings = (app) =>
  [...app.shadowRoot.querySelector("chorus-rooms").shadowRoot.querySelectorAll("chorus-room-card")].map(
    (card) => card.shadowRoot.querySelector("h2").textContent,
  );

test("the login's answers are told from the server's", () => {
  assert.equal(signedOut({ type: "opaqueredirect", status: 0, ok: false }), true);
  assert.equal(signedOut({ type: "basic", status: 401, ok: false }), true);
  for (const status of [200, 400, 403, 404, 500, 502]) {
    assert.equal(signedOut({ type: "basic", status, ok: status === 200 }), false, String(status));
  }
  assert.equal(signedOut(undefined), false);
});

test("every request of the API client is made with redirect: manual", async () => {
  const server = fakeServer(stateOf(1, [zone("kitchen")]));
  const client = clientOf(server);
  await client.state();
  await client.command(muteCommand("kitchen", true));
  const close = client.events({ onState() {} });
  await settle();
  close();
  assert.deepEqual(server.requests, [
    { route: "api/state", redirect: "manual" },
    { route: "api/command", redirect: "manual" },
    { route: "api/events", redirect: "manual" },
  ]);
});

test("an opaqueredirect from an API call is signed out, for the snapshot, a command and the event stream", async () => {
  for (const login of ["redirect", "refuse"]) {
    const server = fakeServer(stateOf(1, [zone("kitchen")]));
    server.login = login;
    const timers = fakeTimers();
    const client = clientOf(server, timers);

    await assert.rejects(client.state(), (error) => error.signedOut === true && error.message === "Signed out", login);
    assert.deepEqual(await client.command(muteCommand("kitchen", true)), { ok: false, refusal: "Signed out", signedOut: true }, login);
    // The command never reached the server.
    assert.deepEqual(server.commands, [], login);

    const statuses = [];
    const states = [];
    const close = client.events({ onState: (state) => states.push(state.serial), onStatus: (status) => statuses.push(status) });
    await settle();
    assert.deepEqual(statuses, ["signed-out"], login);
    // It is asked again, and says the same while the login still answers.
    timers.fire(RETRY_MS);
    await settle();
    assert.deepEqual(statuses, ["signed-out", "signed-out"], login);

    // Signed in again (in another tab, say): the next attempt is the server's.
    server.login = null;
    timers.fire(RETRY_MS);
    await settle();
    server.send(stateOf(2, [zone("kitchen")]));
    await settle();
    assert.deepEqual(statuses.slice(2), ["live"], login);
    assert.deepEqual(states, [2], login);
    close();
  }
});

test("a server that is down or broken is lost, not signed out", async () => {
  const server = fakeServer(stateOf(1, [zone("kitchen")]));
  const inner = server.fetch;
  server.fetch = async (url, options) =>
    String(url).endsWith("api/events") ? { ok: false, status: 502, type: "basic", body: null } : inner(url, options);
  const statuses = [];
  const close = clientOf(server).events({ onState() {}, onStatus: (status) => statuses.push(status) });
  await settle();
  assert.deepEqual(statuses, ["lost"]);
  close();
});

test("the store's status is signed-out, and a command that meets the login says so at once", async () => {
  const server = fakeServer(stateOf(3, [zone("kitchen")]));
  const timers = fakeTimers();
  const store = createStore(clientOf(server, timers));
  stores.push(store);
  store.start();
  await settle();
  server.send(stateOf(3, [zone("kitchen")]));
  await settle();
  assert.equal(store.view().status, "live");

  // The login lapses while the event stream is still open: the stream has
  // not noticed, and the command is the first request to meet the login.
  server.login = "redirect";
  const result = await store.command(muteCommand("kitchen", true));
  assert.equal(result.ok, false);
  assert.equal(store.view().status, "signed-out");
  // What the server last said is still held.
  assert.equal(store.view().state.serial, 3);

  // The stream ends, and its next attempt meets the login too.
  server.drop();
  await settle();
  timers.fire(RETRY_MS);
  await settle();
  assert.equal(store.view().status, "signed-out");
});

test("signed out from the start, the app shows 'Signed out' and a link to sign in", async () => {
  const server = fakeServer(stateOf(1, [zone("kitchen")]));
  server.login = "redirect";
  const app = await mountOver(server);

  const alert = banner(app);
  assert.ok(alert, "the shell says it is signed out");
  assert.equal(alert.getAttribute("role"), "alert");
  assert.match(words(alert), /^Signed out\./);
  // The way to sign in: a link to the page's own address, which the login
  // answers with its page and returns from.
  const link = getByLabel(app, "Sign in");
  assert.equal(link.localName, "a");
  assert.equal(words(link), "Sign in");
  assert.equal(link.href, window.location.href);
  // No room was ever read, and nothing claims the server is unreachable.
  assert.deepEqual(headings(app), []);
  const status = app.shadowRoot.querySelector("chorus-rooms").shadowRoot.querySelector("[role=status]");
  assert.equal(words(status), "");
});

test("a login that lapses under a live app shows 'Signed out' over the last known rooms, and signing in clears it", async () => {
  const server = fakeServer(stateOf(1, [zone("kitchen", { name: "The Kitchen" })]));
  const timers = fakeTimers();
  const app = await mountOver(server, timers);
  server.send(stateOf(1, [zone("kitchen", { name: "The Kitchen" })]));
  await rendered(app);
  assert.equal(banner(app), null);
  assert.deepEqual(queryAllByLabel(app, "Sign in"), []);

  server.login = "redirect";
  server.drop();
  await rendered(app);
  timers.fire(RETRY_MS);
  await rendered(app);
  assert.match(words(banner(app)), /^Signed out\./);
  assert.equal(getByLabel(app, "Sign in").localName, "a");
  assert.deepEqual(headings(app), ["The Kitchen"]);
  const status = app.shadowRoot.querySelector("chorus-rooms").shadowRoot.querySelector("[role=status]");
  assert.equal(words(status), "This is the last known state.");

  // Signed in again elsewhere: the stream's next attempt is live, and the
  // words go.
  server.login = null;
  timers.fire(RETRY_MS);
  await rendered(app);
  server.send(stateOf(2, [zone("kitchen", { name: "Kitchen" })]));
  await rendered(app);
  assert.equal(banner(app), null);
  assert.deepEqual(headings(app), ["Kitchen"]);
});

test("the signed-out words are styled with tokens", () => {
  const sheet = customElements.get("chorus-app").styles.cssText;
  assert.match(sheet, /\[data-signed-out\]\s*\{[^}]*color: var\(--warn\)/);
  assert.doesNotMatch(sheet, /#[0-9a-f]{3,8}\b|\b\d+(px|rem|em)\b/i);
});
