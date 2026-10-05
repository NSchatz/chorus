// The app shell as a component: defined, rendered into its shadow root and
// updated by Lit, under happy-dom (test/setup.js), with no browser. Given a
// store over a scripted server it shows that server's rooms live, sends what
// a room card asks for, and shows a refusal in the server's words.

import assert from "node:assert/strict";
import { afterEach, test } from "node:test";

import { createClient } from "../src/api.js";
import "../src/chorus-app.js";
import { createStore } from "../src/state.js";
import { fakeServer, fakeTimers, settle, stateOf, zone } from "./fake-server.js";
import { getByLabel } from "./label-query.js";

let stores = [];

afterEach(() => {
  for (const store of stores) store.stop();
  stores = [];
  document.body.replaceChildren();
});

// The app over a store that follows `server`, started.
async function mountOver(server) {
  const store = createStore(createClient({ fetch: server.fetch, base: server.base, timers: fakeTimers() }));
  stores.push(store);
  const app = document.createElement("chorus-app");
  app.store = store;
  document.body.append(app);
  store.start();
  await rendered(app);
  return app;
}

// Everything queued has run and every element under the app has rendered.
async function rendered(app) {
  await settle();
  await app.updateComplete;
  const screen = app.shadowRoot.querySelector("chorus-rooms");
  await screen.updateComplete;
  await Promise.all([...screen.shadowRoot.querySelectorAll("chorus-room-card")].map((card) => card.updateComplete));
}

const headings = (app) =>
  [...app.shadowRoot.querySelector("chorus-rooms").shadowRoot.querySelectorAll("chorus-room-card")].map(
    (card) => card.shadowRoot.querySelector("h2").textContent,
  );

async function mount() {
  const app = document.createElement("chorus-app");
  document.body.append(app);
  await app.updateComplete;
  return app;
}

test("the shell renders its wordmark and a labelled main region", async () => {
  const app = await mount();
  assert.equal(app.shadowRoot.querySelector("h1").textContent, "chorus");
  const main = getByLabel(app, "Rooms");
  assert.equal(main.localName, "main");
  assert.ok(main.querySelector("slot"), "screens are slotted into the main region");
});

test("the app shows the server's rooms from the snapshot, then follows the event stream", async () => {
  const server = fakeServer(stateOf(1, [zone("kitchen", { name: "The Kitchen" }), zone("den")]));
  const app = await mountOver(server);
  assert.deepEqual(headings(app), ["The Kitchen", "den"]);
  assert.equal(getByLabel(app, "Volume for den").value, "500");

  // Another client renames a room, moves its volume and mutes it.
  server.send(stateOf(2, [zone("kitchen", { name: "The Kitchen" }), zone("den", { name: "Den", volume: 0.125, muted: true })]));
  await rendered(app);
  assert.deepEqual(headings(app), ["The Kitchen", "Den"]);
  assert.equal(getByLabel(app, "Volume for Den").value, "125");
  assert.equal(getByLabel(app, "Mute Den").getAttribute("aria-pressed"), "true");
});

test("a change made on a card is sent to the server and shown when the server says it", async () => {
  const server = fakeServer(stateOf(1, [zone("kitchen")]));
  const app = await mountOver(server);
  server.answer = () => ({ status: 200, body: JSON.stringify(stateOf(2, [zone("kitchen", { muted: true })])) });
  getByLabel(app, "Mute kitchen").click();
  await rendered(app);
  assert.deepEqual(server.commands, ['{"v":1,"t":"mute","zone":"kitchen","muted":true}']);
  assert.equal(getByLabel(app, "Mute kitchen").getAttribute("aria-pressed"), "true");
});

test("a refused command shows the server's refusal text on the room it was for", async () => {
  const server = fakeServer(stateOf(1, [zone("kitchen"), zone("den")]));
  const app = await mountOver(server);
  const detail =
    "the volume 2.000 is outside the range the catalog declares, which is 0.000 to 1.000 in steps of 0.001";
  server.answer = () => ({ status: 400, body: JSON.stringify({ v: 1, t: "error", field: "volume", detail }) });

  const slider = getByLabel(app, "Volume for den");
  slider.focus();
  slider.value = "900";
  slider.dispatchEvent(new Event("input", { bubbles: true }));
  slider.dispatchEvent(new Event("change", { bubbles: true }));
  await rendered(app);

  const alerts = [...app.shadowRoot.querySelector("chorus-rooms").shadowRoot.querySelectorAll("chorus-room-card")].map(
    (card) => card.shadowRoot.querySelector("[role=alert]").textContent.trim(),
  );
  assert.deepEqual(alerts, ["", `Refused: ${detail}`]);
  // The room is as the server still holds it.
  assert.equal(slider.value, "500");

  // The next command of that room clears the words; this one is accepted.
  server.answer = () => ({ status: 200, body: JSON.stringify(stateOf(2, [zone("kitchen"), zone("den", { muted: true })])) });
  getByLabel(app, "Mute den").click();
  await rendered(app);
  const den = app.shadowRoot.querySelector("chorus-rooms").shadowRoot.querySelectorAll("chorus-room-card")[1];
  assert.equal(den.shadowRoot.querySelector("[role=alert]").textContent.trim(), "");
});

test("an app taken off the page stops following its store", async () => {
  const server = fakeServer(stateOf(1, [zone("kitchen")]));
  const app = await mountOver(server);
  app.remove();
  server.send(stateOf(2, [zone("kitchen", { name: "Renamed" })]));
  await settle();
  await app.updateComplete;
  assert.deepEqual(headings(app), ["kitchen"]);
});

test("the layout is a reflected property, ordinary by default", async () => {
  const app = await mount();
  assert.equal(app.getAttribute("mode"), "app");
  app.mode = "kiosk";
  await app.updateComplete;
  assert.equal(app.getAttribute("mode"), "kiosk");
});

test("a layout that is not one of the two falls back to the ordinary one", async () => {
  const app = await mount();
  app.mode = "cinema";
  await app.updateComplete;
  assert.equal(app.mode, "app");
  assert.equal(app.getAttribute("mode"), "app");
});

test("its styles name tokens, never a literal colour or length", () => {
  const sheet = customElements.get("chorus-app").styles.cssText;
  assert.match(sheet, /var\(--wordmark-ink\)/);
  assert.doesNotMatch(sheet, /#[0-9a-f]{3,8}\b|\b\d+(px|rem|em)\b/i);
});
