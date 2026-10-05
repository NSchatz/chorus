// The walk-through for a compact Wi-Fi speaker (src/speaker-setup.js) in the
// shell, over a scripted server: its steps are decision 0103's, it has no
// field for the network's passphrase or anything else, it stays open while
// the server is out of reach, and it completes by itself when a speaker that
// was not adopted when it began appears in the state.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { afterEach, test } from "node:test";

import { RETRY_MS, createClient } from "../src/api.js";
import "../src/chorus-app.js";
import { addressOf, routeOf } from "../src/routes.js";
import {
  ACCESS_POINT_PREFIX,
  ACCESS_POINT_SUFFIX_LENGTH,
  JOIN_PAGE,
  SETUP_SECRET_LENGTH,
  SETUP_STEPS,
  arrivals,
} from "../src/speaker-setup.js";
import { SPEAKERS_SCREEN, SPEAKER_SETUP_SCREEN } from "../src/speakers.js";
import { createStore } from "../src/state.js";
import { fakeServer, fakeTimers, settle, stateOf, zone } from "./fake-server.js";
import { getByLabel, queryAllByLabel } from "./label-query.js";

let stores = [];

afterEach(() => {
  for (const store of stores) store.stop();
  stores = [];
  document.body.replaceChildren();
  history.replaceState(null, "", "/");
  sessionStorage.clear();
});

const DECISION = readFileSync(
  new URL("../../docs/decisions/0103-wifi-provisioning-over-softap.md", import.meta.url),
  "utf8",
);
const SOURCE = readFileSync(new URL("../src/speaker-setup.js", import.meta.url), "utf8");

const OLD = "chorus-aaaaaaaaaaaa";
const ARRIVING = "chorus-0123456789ab";
const speaker = (id, more = {}) => ({
  id,
  name: `Speaker ${id.slice(-4)}`,
  named: false,
  room: null,
  present: true,
  software: "",
  link: "unknown",
  key: `fp-${id.slice(-4)}`,
  roles: [],
  ...more,
});
const KNOWN = speaker(OLD, { name: "Living left", named: true, room: "living" });
const house = (serial, speakers = [KNOWN]) =>
  stateOf(serial, [zone("living"), zone("kitchen")], speakers.length > 0 ? { speakers } : {});

async function rendered(app) {
  await settle();
  await app.updateComplete;
  await app.shadowRoot.querySelector("chorus-speaker-setup")?.updateComplete;
  await app.shadowRoot.querySelector("chorus-speakers")?.updateComplete;
}

async function open(server, address = addressOf(SPEAKER_SETUP_SCREEN)) {
  history.replaceState(null, "", `/app/${address}`);
  const timers = fakeTimers();
  const store = createStore(createClient({ fetch: server.fetch, base: server.base, timers }));
  stores.push(store);
  const app = document.createElement("chorus-app");
  app.store = store;
  document.body.append(app);
  store.start();
  await rendered(app);
  return { app, timers, store };
}

const walk = (app) => app.shadowRoot.querySelector("chorus-speaker-setup");
const text = (node) => node.textContent.replace(/\s+/g, " ").trim();
const steps = (app) => [...walk(app).shadowRoot.querySelectorAll("li[data-step]")];
const done = (app) => walk(app).shadowRoot.querySelector("[data-done]");
const arrived = (app) => [...walk(app).shadowRoot.querySelectorAll("[data-arrived]")].map((entry) => entry.dataset.arrived);
const waiting = (app) => walk(app).shadowRoot.querySelector("[data-waiting]");
const away = (app) => walk(app).shadowRoot.querySelector("[data-away]");
const STEPS_TEXT = SETUP_STEPS.map((step) => `${step.title}. ${step.text.join(" ")}`).join(" ");

test("the steps name the access point pattern as decision 0103 gives it", () => {
  // The decision: the access point's name is `chorus-setup-<6>`, and its
  // WPA2 passphrase is the 12-character setup secret, printed on the console.
  assert.match(DECISION, /12 characters from a 31-symbol alphabet/);
  assert.match(DECISION, /6 more for the access point's name\s+\(`chorus-setup-<6>`\)/);
  assert.match(DECISION, /The secret is the access point's WPA2\s+passphrase/);
  assert.match(DECISION, /printed on the serial console/);
  assert.equal(`${ACCESS_POINT_PREFIX}<${ACCESS_POINT_SUFFIX_LENGTH}>`, "chorus-setup-<6>");
  assert.equal(SETUP_SECRET_LENGTH, 12);

  const byId = Object.fromEntries(SETUP_STEPS.map((step) => [step.id, step.text.join(" ")]));
  assert.deepEqual(Object.keys(byId), ["power", "access-point", "join-page", "return"]);
  assert.match(byId.power, /access point of its own, named chorus-setup- and 6 characters \(chorus-setup-<6 characters>\)/);
  assert.match(byId.power, /setup secret is 12 characters and is the access point's password/);
  assert.match(byId.power, /prints it, and the address of its join page, on its serial console/);
  assert.match(byId["access-point"], /join the network chorus-setup-<6 characters> with the setup secret as its password/);
});

test("the steps name the speaker's own join page as decision 0103 gives it", () => {
  // The decision: the speaker serves the page itself; `GET /` is a two-field
  // form and `POST /join` takes it; an open network is refused; a failed
  // join returns to the access point with the platform's word.
  assert.match(DECISION, /\*\*3\. The speaker serves its own join page\.\*\*/);
  assert.match(DECISION, /`GET \/`\s+is a two-field form and `POST \/join` takes it/);
  assert.match(DECISION, /An open network is refused/);
  assert.match(DECISION, /returns to `ap-up` with the platform's\s+word \(`auth-error`, `network-not-found`\)/);
  assert.deepEqual({ form: JOIN_PAGE.form, takes: JOIN_PAGE.takes }, { form: "GET /", takes: "POST /join" });

  const page = SETUP_STEPS.find((step) => step.id === "join-page").text.join(" ");
  assert.match(page, /open the address the speaker printed \(http:\/\/<address>\/\)/);
  assert.match(page, /The speaker serves the page itself, on its access point/);
  assert.match(page, /a form with two fields/);
  assert.match(page, /the house network's name and its passphrase into that page, and press Join/);
  assert.match(page, /the speaker refuses an open network/);
  assert.match(page, /auth-error for a wrong passphrase, network-not-found for a name it cannot see/);
  // And where the passphrase goes: the speaker, never this app.
  assert.match(page, /They go to the speaker and nowhere else: this app never asks for them\./);
});

test("the walk-through has an address of its own and opens from the speakers screen", async () => {
  assert.equal(addressOf(SPEAKER_SETUP_SCREEN), "#/speakers/setup");
  assert.equal(routeOf("#/speakers/setup").screen, SPEAKER_SETUP_SCREEN);
  const { app } = await open(fakeServer(house(1)), addressOf(SPEAKERS_SCREEN));
  getByLabel(app, "Set up a Wi-Fi speaker").click();
  await rendered(app);
  assert.equal(location.hash, "#/speakers/setup");
  assert.equal(getByLabel(app, "Set up a Wi-Fi speaker").localName, "main");
  assert.deepEqual(steps(app).map((step) => step.dataset.step), SETUP_STEPS.map((step) => step.id)); // prettier-ignore
  assert.deepEqual(
    steps(app).map((step) => text(step.querySelector("h3"))),
    [
      "1. Switch the speaker on",
      "2. Join the speaker's access point",
      "3. Open the speaker's join page",
      "4. Come back to the house's network",
    ],
  );
  // What is painted is the steps' own words.
  for (const [at, step] of SETUP_STEPS.entries()) {
    assert.deepEqual([...steps(app)[at].querySelectorAll("p")].map(text), step.text);
  }
  assert.match(text(waiting(app)), /^Waiting for a new speaker\./);
  assert.equal(done(app), null);
});

test("it never asks for the network's passphrase, and holds nothing but the ids it began with", async () => {
  const server = fakeServer(house(1));
  const { app } = await open(server);
  const root = walk(app).shadowRoot;
  // No field of any kind: nothing can be typed into the walk-through.
  assert.equal(root.querySelectorAll("input, textarea, select, form, [contenteditable]").length, 0);
  assert.deepEqual([...root.querySelectorAll("button")].map(text), [], "and no button while it waits");
  assert.doesNotMatch(SOURCE, /<input|<textarea|<form|<select|contenteditable/);
  // It sends nothing: no command, and no request beyond the store's own two.
  assert.deepEqual(server.commands, []);
  assert.deepEqual([...new Set(server.requests.map((request) => request.route))].sort(), ["api/events", "api/state"]);
  // What it keeps is the ids of the speakers adopted when it began.
  assert.deepEqual({ ...sessionStorage }, { "chorus-speaker-setup": JSON.stringify([OLD]) });
  assert.deepEqual(
    Object.keys(customElements.get("chorus-speaker-setup").properties).sort(),
    ["_baseline", "back", "speakers", "status", "storage"],
  );
  assert.match(text(root), /it never asks for the network's passphrase/);
});

test("it completes when a speaker absent at its start appears in the server's state", async () => {
  const server = fakeServer(house(1));
  const { app } = await open(server);
  assert.equal(done(app), null);
  assert.deepEqual(arrivals([OLD], [KNOWN]), []);

  // A state that changes something else is not an arrival.
  server.send(house(2, [{ ...KNOWN, present: false }]));
  await rendered(app);
  assert.equal(done(app), null);
  assert.match(text(waiting(app)), /^Waiting for a new speaker\./);

  // The new speaker is adopted: the walk-through says so by itself.
  server.send(house(3, [speaker(ARRIVING), KNOWN]));
  await rendered(app);
  assert.deepEqual(arrived(app), [ARRIVING]);
  assert.equal(text(done(app).querySelector("[data-arrived]")), `Speaker 89ab (${ARRIVING}) joined and was adopted.`);
  assert.equal(waiting(app), null);
  assert.equal(walk(app).shadowRoot.querySelector("ol").hasAttribute("data-complete"), true);
  assert.deepEqual(server.commands, [], "nothing was pressed and nothing was sent");

  // From there to the speakers screen, where it is new and can be named.
  const next = getByLabel(app, "Name the new speaker and give it a room");
  assert.equal(next.getAttribute("href"), addressOf(SPEAKERS_SCREEN));
  next.click();
  await rendered(app);
  assert.equal(location.hash, "#/speakers");
  const row = app.shadowRoot.querySelector("chorus-speakers").shadowRoot.querySelector(`li[data-speaker="${ARRIVING}"]`);
  assert.equal(row.hasAttribute("data-new"), true);
  assert.equal(sessionStorage.length, 0, "leaving the screen ends the walk-through");
});

test("it stays open while the server is out of reach, and completes when the server is back", async () => {
  const server = fakeServer(house(1));
  const { app, timers, store } = await open(server);
  server.send(house(1));
  await rendered(app);
  assert.equal(store.view().status, "live");

  // The phone joins the speaker's access point: the stream ends and the server cannot be reached.
  server.snapshot = null;
  const reachable = server.fetch;
  server.fetch = async () => {
    throw new TypeError("fetch failed");
  };
  server.drop();
  await rendered(app);
  assert.equal(store.view().status, "lost");
  assert.equal(steps(app).length, SETUP_STEPS.length, "the steps stay");
  assert.match(text(away(app)), /cannot reach the chorus server now\. That is expected while the phone is on the speaker's access point/);
  assert.equal(done(app), null);

  // Still away at the next retry.
  timers.fire(RETRY_MS);
  await rendered(app);
  assert.equal(store.view().status, "lost");
  assert.match(text(away(app)), /cannot reach the chorus server now/);

  // Back on the house's network: the store's own retry opens the stream
  // again, and the state it delivers holds the speaker that was adopted
  // while the phone was away.
  server.fetch = reachable;
  timers.fire(RETRY_MS);
  await rendered(app);
  server.send(house(2, [speaker(ARRIVING), KNOWN]));
  await rendered(app);
  assert.equal(store.view().status, "live");
  assert.equal(away(app), null);
  assert.deepEqual(arrived(app), [ARRIVING]);
});

test("signed out while it is open, it says so and goes on after the sign-in", async () => {
  const server = fakeServer(house(1));
  const { app, timers } = await open(server);
  server.login = "redirect";
  server.drop();
  await rendered(app);
  // The stream is opened again, and the login answers in the server's place.
  timers.fire(RETRY_MS);
  await rendered(app);
  assert.match(text(away(app)), /^Signed out of the chorus server: sign in again to go on\.$/);
  assert.equal(queryAllByLabel(app, "Sign in").length, 1, "the shell's way back in is there");
  assert.equal(steps(app).length, SETUP_STEPS.length);
  server.login = null;
  timers.fire(RETRY_MS);
  await rendered(app);
  server.send(house(2, [KNOWN, speaker(ARRIVING)]));
  await rendered(app);
  assert.deepEqual(arrived(app), [ARRIVING]);
});

test("a page loaded again while the walk-through was open goes on from the speakers it began with", async () => {
  // The phone dropped the page while its owner was in the Wi-Fi settings:
  // the tab's session storage still holds the ids, and the first state after
  // the reload already has the new speaker.
  sessionStorage.setItem("chorus-speaker-setup", JSON.stringify([OLD]));
  const { app } = await open(fakeServer(house(5, [speaker(ARRIVING), KNOWN])));
  assert.deepEqual(arrived(app), [ARRIVING]);
});

test("opened with the server out of reach it shows the steps, and begins at the first state", async () => {
  const server = fakeServer(null);
  const { app } = await open(server);
  assert.equal(steps(app).length, SETUP_STEPS.length);
  assert.equal(done(app), null);
  server.send(house(1));
  await rendered(app);
  assert.match(text(waiting(app)), /^Waiting for a new speaker\./);
  assert.equal(done(app), null, "the speakers adopted when it began are not news");
  server.send(house(2, [KNOWN, speaker(ARRIVING)]));
  await rendered(app);
  assert.deepEqual(arrived(app), [ARRIVING]);
});

test("after one speaker, another can be set up: it begins again from the speakers adopted now", async () => {
  const server = fakeServer(house(1));
  const { app } = await open(server);
  server.send(house(2, [speaker(ARRIVING), KNOWN]));
  await rendered(app);
  getByLabel(app, "Set up another speaker").click();
  await rendered(app);
  assert.equal(done(app), null);
  assert.match(text(waiting(app)), /^Waiting for a new speaker\./);
  const THIRD = "chorus-cccccccccccc";
  server.send(house(3, [speaker(ARRIVING), KNOWN, speaker(THIRD)]));
  await rendered(app);
  assert.deepEqual(arrived(app), [THIRD]);
});

test("its styles name tokens, never a literal colour or length", () => {
  const sheet = customElements.get("chorus-speaker-setup").styles.cssText;
  assert.match(sheet, /min-height: var\(--control-size\)/);
  assert.doesNotMatch(sheet, /#[0-9a-f]{3,8}\b|\b\d+(px|rem|em)\b/i);
});
