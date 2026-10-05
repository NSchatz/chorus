// The house's autoplay rules (src/autoplay.js) in the shell, over a scripted
// server: every input the state lists has a switch and a target picker (the
// rooms and the saved groups), both show the server's rule, and each change
// sends exactly one `autoplay` command for that input.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { afterEach, test } from "node:test";

import { autoplayCommand, createClient } from "../src/api.js";
import { AUTOPLAY_SCREEN, autoplayRows } from "../src/autoplay.js";
import "../src/chorus-app.js";
import { addressOf, routeOf } from "../src/routes.js";
import { autoplayOf, createStore } from "../src/state.js";
import { fakeServer, fakeTimers, settle, stateOf, zone } from "./fake-server.js";
import { getByLabel } from "./label-query.js";

let stores = [];

afterEach(() => {
  for (const store of stores) store.stop();
  stores = [];
  document.body.replaceChildren();
  history.replaceState(null, "", "/");
});

const fixture = (name) => readFileSync(new URL(`../../fixtures/control/v2/${name}`, import.meta.url), "utf8").trim();

// The house: three rooms, a saved group, and three inputs offered, one of
// them with a name a person gave it. `hub/tv` has a rule; a fourth input,
// whose speaker is off, has a rule too and is not offered.
const DECK = "endpoint-c/line-1";
const TV = "hub/tv";
const AUX = "kitchen-amp/aux";
const GONE = "attic-amp/line-1";
const house = (serial, autoplay = [], more = {}) =>
  stateOf(serial, [zone("living", { name: "Living Room" }), zone("kitchen"), zone("den")], {
    saved_groups: [{ id: "downstairs", name: "Downstairs", zones: ["living", "kitchen"], active: false }],
    inputs: [DECK, TV, AUX],
    input_labels: [{ input: DECK, name: "Record Deck", role: "line-in" }],
    autoplay,
    ...more,
  });
const TV_RULE = { input: TV, target: "downstairs", enabled: true };

async function rendered(app) {
  await settle();
  await app.updateComplete;
  await app.shadowRoot.querySelector("chorus-autoplay")?.updateComplete;
}

async function open(server) {
  history.replaceState(null, "", `/app/${addressOf(AUTOPLAY_SCREEN)}`);
  const store = createStore(createClient({ fetch: server.fetch, base: server.base, timers: fakeTimers() }));
  stores.push(store);
  const app = document.createElement("chorus-app");
  app.store = store;
  document.body.append(app);
  store.start();
  await rendered(app);
  return app;
}

// A scripted server that holds the rules as the real one does: `autoplay`
// creates or replaces the rule of its input, and the state lists the rules
// sorted by input.
function rulesServer(start = []) {
  let rules = start;
  let serial = 1;
  const server = fakeServer(house(serial, rules));
  server.answer = (body) => {
    const { v: _v, t: _t, ...rule } = JSON.parse(body);
    rules = [...rules.filter((other) => other.input !== rule.input), rule].sort((a, b) => (a.input < b.input ? -1 : 1));
    serial += 1;
    server.snapshot = house(serial, rules);
    return { status: 200, body: JSON.stringify(server.snapshot) };
  };
  return server;
}

const screenOf = (app) => app.shadowRoot.querySelector("chorus-autoplay");
const text = (node) => node.textContent.replace(/\s+/g, " ").trim();
const rows = (app) => [...screenOf(app).shadowRoot.querySelectorAll("li[data-input]")];
const row = (app, input) => rows(app).find((item) => item.dataset.input === input);
const pressed = (button) => button.getAttribute("aria-pressed") === "true";
const switchOf = (app, label) => getByLabel(app, `Autoplay for ${label}`);
const pickerOf = (app, label) => getByLabel(app, `Autoplay target for ${label}`);
const says = (app, input) => text(row(app, input).querySelector('[data-value="enabled"]'));
const alertOf = (app, input) => text(row(app, input).querySelector("[role=alert]"));

// A person chooses a target from the list.
function choose(picker, target) {
  picker.value = target;
  picker.dispatchEvent(new Event("change", { bubbles: true }));
}

// The targets a picker offers, by group of the list.
const offered = (picker) =>
  Object.fromEntries(
    [...picker.querySelectorAll("optgroup")].map((group) => [
      group.label,
      [...group.querySelectorAll("option")].map((option) => [option.value, text(option)]),
    ]),
  );

test("the command is the catalog's own vector, and a rule's TV fields are written only when false", () => {
  assert.equal(autoplayCommand("endpoint-c/line-1", "living", true), fixture("autoplay.json"));
  assert.equal(
    autoplayCommand("hub/tv", "living", true, { stopOnStandby: false, lowLatency: false }),
    '{"v":2,"t":"autoplay","input":"hub/tv","target":"living","enabled":true,"stop_on_standby":false,"low_latency":false}',
  );
  assert.equal(
    autoplayCommand("hub/tv", "den", false, { stopOnStandby: true, lowLatency: false }),
    '{"v":2,"t":"autoplay","input":"hub/tv","target":"den","enabled":false,"low_latency":false}',
  );
  assert.deepEqual(autoplayOf({ autoplay: [JSON.parse(fixture("autoplay-tv.json")), { target: "no input" }] }).length, 1);
  assert.deepEqual(autoplayOf({ autoplay: [{ input: TV, target: "den", enabled: true, low_latency: false }] }), [
    { input: TV, target: "den", enabled: true, stopOnStandby: true, lowLatency: false },
  ]);
  assert.deepEqual(autoplayOf(null), []);
});

test("the screen has an address of its own and opens from the home", async () => {
  assert.equal(addressOf(AUTOPLAY_SCREEN), "#/autoplay");
  assert.equal(routeOf("#/autoplay").screen, AUTOPLAY_SCREEN);
  const server = fakeServer(house(1));
  const store = createStore(createClient({ fetch: server.fetch, base: server.base, timers: fakeTimers() }));
  stores.push(store);
  const app = document.createElement("chorus-app");
  app.store = store;
  document.body.append(app);
  store.start();
  await rendered(app);
  const link = getByLabel(app, "Autoplay rules");
  assert.equal(link.getAttribute("href"), "#/autoplay");
  link.click();
  await rendered(app);
  assert.equal(location.hash, "#/autoplay");
  assert.equal(getByLabel(app, "Autoplay").localName, "main");
  assert.equal(rows(app).length, 3);
  getByLabel(app, "Back to rooms").click();
  await rendered(app);
  assert.equal(getByLabel(app, "Rooms").localName, "main");
});

test("every input the state lists has a switch and a target picker of the rooms and the saved groups", async () => {
  const server = fakeServer(house(1, [{ input: GONE, target: "den", enabled: false }, TV_RULE]));
  const app = await open(server);
  // The offered inputs in the server's order, then the one with a rule that
  // is not offered now.
  assert.deepEqual(rows(app).map((item) => item.dataset.input), [DECK, TV, AUX, GONE]); // prettier-ignore
  assert.deepEqual(
    autoplayRows(stores[0].view()).map((item) => [item.input, item.label, item.offered, item.rule?.target ?? null]),
    [
      [DECK, "Record Deck", true, null],
      [TV, TV, true, "downstairs"],
      [AUX, AUX, true, null],
      [GONE, GONE, false, "den"],
    ],
  );
  const targets = {
    Rooms: [
      ["living", "Living Room"],
      ["kitchen", "kitchen"],
      ["den", "den"],
    ],
    "Saved groups": [["downstairs", "Downstairs"]],
  };
  for (const label of ["Record Deck", TV, AUX, GONE]) {
    assert.equal(switchOf(app, label).localName, "button");
    const picker = pickerOf(app, label);
    assert.equal(picker.localName, "select");
    assert.deepEqual(offered(picker), targets, `the targets of ${label}`);
  }
  // What each shows is the server's rule.
  assert.deepEqual([pressed(switchOf(app, TV)), pickerOf(app, TV).value, says(app, TV)], [true, "downstairs", "On"]);
  assert.deepEqual([pressed(switchOf(app, GONE)), pickerOf(app, GONE).value, says(app, GONE)], [false, "den", "Off"]);
  assert.match(text(row(app, GONE).querySelector("[data-absent]")), /Not offered now/);
  assert.equal(row(app, TV).querySelector("[data-absent]"), null);
  // An input with no rule: nowhere yet, and a switch that waits for a target.
  assert.deepEqual(
    [pressed(switchOf(app, AUX)), switchOf(app, AUX).disabled, pickerOf(app, AUX).value],
    [false, true, ""],
  );
  assert.equal(says(app, AUX), "Choose where it plays, then switch it on");
  // A named input says its id too.
  assert.equal(text(row(app, DECK).querySelector("h3")), "Record Deck");
  assert.equal(text(row(app, DECK).querySelector("[data-id]")), DECK);
  assert.deepEqual(server.commands, [], "showing sends nothing");
});

test("each change sends one autoplay command for that input, and the screen shows the server's rule", async () => {
  const server = rulesServer([TV_RULE]);
  const app = await open(server);

  // A target for an input with no rule makes the rule, switched off.
  choose(pickerOf(app, "Record Deck"), "living");
  await rendered(app);
  assert.deepEqual(server.commands, ['{"v":2,"t":"autoplay","input":"endpoint-c/line-1","target":"living","enabled":false}']);
  assert.deepEqual(
    [pickerOf(app, "Record Deck").value, pressed(switchOf(app, "Record Deck")), switchOf(app, "Record Deck").disabled],
    ["living", false, false],
  );
  assert.equal(says(app, DECK), "Off");

  // Its switch then turns it on: the catalog's own vector.
  switchOf(app, "Record Deck").click();
  await rendered(app);
  assert.equal(server.commands.length, 2);
  assert.equal(server.commands[1], fixture("autoplay.json"));
  assert.equal(pressed(switchOf(app, "Record Deck")), true);

  // Another input's switch: one command, for it, with its target as it was.
  switchOf(app, TV).click();
  await rendered(app);
  assert.equal(server.commands.length, 3);
  assert.equal(server.commands[2], '{"v":2,"t":"autoplay","input":"hub/tv","target":"downstairs","enabled":false}');
  assert.deepEqual([pressed(switchOf(app, TV)), pickerOf(app, TV).value], [false, "downstairs"]);

  // And its target: one command, with its switch as it was.
  choose(pickerOf(app, TV), "den");
  await rendered(app);
  assert.equal(server.commands.length, 4);
  assert.equal(server.commands[3], '{"v":2,"t":"autoplay","input":"hub/tv","target":"den","enabled":false}');
  assert.deepEqual([pressed(switchOf(app, TV)), pickerOf(app, TV).value], [false, "den"]);
  // The other input's rule was not touched.
  assert.deepEqual([pressed(switchOf(app, "Record Deck")), pickerOf(app, "Record Deck").value], [true, "living"]);

  // Choosing the target it already has sends nothing.
  choose(pickerOf(app, TV), "den");
  await rendered(app);
  assert.equal(server.commands.length, 4);
});

test("a rule's TV options, which this screen does not show, are kept through its changes", async () => {
  const server = rulesServer([{ ...TV_RULE, stop_on_standby: false, low_latency: false }]);
  const app = await open(server);
  switchOf(app, TV).click();
  await rendered(app);
  choose(pickerOf(app, TV), "kitchen");
  await rendered(app);
  assert.deepEqual(server.commands, [
    '{"v":2,"t":"autoplay","input":"hub/tv","target":"downstairs","enabled":false,"stop_on_standby":false,"low_latency":false}',
    '{"v":2,"t":"autoplay","input":"hub/tv","target":"kitchen","enabled":false,"stop_on_standby":false,"low_latency":false}',
  ]);
});

test("nothing is shown before the server answers: a switch and a picker follow the state, not the click", async () => {
  const server = fakeServer(house(1, [TV_RULE]));
  // The server takes the command and changes nothing.
  server.answer = () => ({ status: 200, body: JSON.stringify(house(2, [TV_RULE])) });
  const app = await open(server);
  switchOf(app, TV).click();
  await rendered(app);
  switchOf(app, TV).click();
  await rendered(app);
  const off = '{"v":2,"t":"autoplay","input":"hub/tv","target":"downstairs","enabled":false}';
  assert.deepEqual(server.commands, [off, off], "both clicks ask for the opposite of what the server holds");
  assert.equal(pressed(switchOf(app, TV)), true);
  choose(pickerOf(app, TV), "den");
  await rendered(app);
  assert.equal(pickerOf(app, TV).value, "downstairs");
});

test("a refusal is shown on the input it was for, in the server's words, and nothing changes", async () => {
  const server = fakeServer(house(1, [TV_RULE]));
  const app = await open(server);
  const detail = "'attic' is neither a room nor a saved group";
  server.answer = () => ({ status: 400, body: JSON.stringify({ v: 2, t: "error", field: "target", detail }) });
  choose(pickerOf(app, AUX), "den");
  await rendered(app);
  assert.equal(alertOf(app, AUX), `Refused: ${detail}`);
  assert.equal(alertOf(app, TV), "");
  assert.equal(pickerOf(app, AUX).value, "");
  assert.equal(switchOf(app, AUX).disabled, true);
});

test("a rule another client makes appears, and a target the server no longer has is still said", async () => {
  const server = fakeServer(house(1));
  const app = await open(server);
  const picker = pickerOf(app, AUX);
  server.send(house(2, [{ input: AUX, target: "den", enabled: true }]));
  await rendered(app);
  assert.equal(pickerOf(app, AUX), picker, "the screen was patched, not painted again");
  assert.deepEqual([picker.value, pressed(switchOf(app, AUX)), says(app, AUX)], ["den", true, "On"]);
  server.send(house(3, [{ input: AUX, target: "porch", enabled: true }]));
  await rendered(app);
  assert.equal(picker.value, "porch");
  assert.equal(text(picker.selectedOptions[0]), "porch (not on this server now)");
  assert.deepEqual(server.commands, []);
});

test("before the first state the screen says it is reading, and a house with no input says so", async () => {
  const server = fakeServer(null);
  const app = await open(server);
  const root = () => screenOf(app).shadowRoot;
  assert.equal(text(root().querySelector("[data-missing]")), "Reading this server's inputs.");
  server.send(stateOf(1, [zone("den")]));
  await rendered(app);
  assert.match(text(root().querySelector("[data-none]")), /offers no input now/);
  assert.equal(rows(app).length, 0);
});

test("its styles name tokens, never a literal colour or length", () => {
  const sheet = customElements.get("chorus-autoplay").styles.cssText;
  assert.match(sheet, /min-height: var\(--control-size\)/);
  assert.doesNotMatch(sheet, /#[0-9a-f]{3,8}\b|\b\d+(px|rem|em)\b/i);
});
