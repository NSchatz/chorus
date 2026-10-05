// The alarms screen (src/alarms.js) in the shell, over a scripted server: an
// alarm is set with each of the four K80 sources, every source the picker
// offers comes from the state, stored sources are added and forgotten from
// the screen, and a sleep timer is set, counted down and cancelled.

import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { afterEach, test } from "node:test";

import {
  alarmDeleteCommand,
  alarmSetCommand,
  alarmStopCommand,
  createClient,
  sleepCommand,
  sourceForgetCommand,
  sourceStoreCommand,
} from "../src/api.js";
import { ALARMS_SCREEN, leftWords } from "../src/alarms.js";
import "../src/chorus-app.js";
import { addressOf, routeOf } from "../src/routes.js";
import { alarmsOf, chimesOf, createStore, receiversOf, sleepOf, storedSourcesOf } from "../src/state.js";
import { fakeServer, fakeTimers, settle, stateOf, zone } from "./fake-server.js";
import { getByLabel, queryAllByLabel } from "./label-query.js";

let stores = [];

afterEach(() => {
  for (const store of stores) store.stop();
  stores = [];
  document.body.replaceChildren();
  history.replaceState(null, "", "/");
});

const fixture = (name) => readFileSync(new URL(`../../fixtures/control/v2/${name}`, import.meta.url), "utf8").trim();

// The house: three rooms (two of them playing together as a live group), a
// saved group, three chimes, two inputs (one a person named) and one stored
// source of each kind.
const DECK = "deck/line-1";
const TV = "hub/tv";
const RADIO = { id: "morning-radio", kind: "url", value: "https://radio.example/stream.mp3", name: "Morning radio" };
const PLAYLIST = { id: "wake-playlist", kind: "spotify", value: "spotify:playlist:37i9dQZF1DXexample0000", name: "Wake up" };
const CHIMES = ["bell", "ding-dong", "triad"];
const house = (serial, more = {}) =>
  stateOf(serial, [zone("bedroom", { name: "Bedroom" }), zone("kitchen", { group: "kitchen+den" }), zone("den", { group: "kitchen+den" })], {
    groups: [
      { id: "bedroom", kind: "room", zones: ["bedroom"], volume: 0.5, source: "stream" },
      { id: "kitchen+den", kind: "live", zones: ["kitchen", "den"], volume: 0.5, source: "stream" },
    ],
    saved_groups: [{ id: "upstairs", name: "Upstairs", zones: ["bedroom", "den"], active: false }],
    alarms: [],
    sleep: [],
    inputs: [DECK, TV],
    stored_sources: [RADIO, PLAYLIST],
    input_labels: [{ input: DECK, name: "Record Deck", role: "line-in" }],
    chimes: CHIMES,
    ...more,
  }); // prettier-ignore

const WAKE = {
  alarm: "wake",
  target: "bedroom",
  time: "06:45",
  days: ["mon", "tue", "wed", "thu", "fri"],
  source: "chime:bell",
  volume: 0.3,
  ramp_s: 30,
  duration_min: 60,
  enabled: true,
  ringing: false,
};

const screenOf = (app) => app.shadowRoot.querySelector("chorus-alarms");
const root = (app) => screenOf(app).shadowRoot;
const text = (node) => node.textContent.replace(/\s+/g, " ").trim();

async function rendered(app) {
  await settle();
  await app.updateComplete;
  await screenOf(app)?.updateComplete;
}

async function open(server) {
  history.replaceState(null, "", `/app/${addressOf(ALARMS_SCREEN)}`);
  const store = createStore(createClient({ fetch: server.fetch, base: server.base, timers: fakeTimers() }));
  stores.push(store);
  const app = document.createElement("chorus-app");
  app.store = store;
  document.body.append(app);
  store.start();
  await rendered(app);
  return app;
}

// A scripted server that holds alarms, stored sources and sleep timers as
// the real one does: sorted by id or target, a stored source with a scheme
// that is not http or https refused in the catalog's words, a forgotten
// source an alarm plays refused naming the alarm, and `sleep` with 0 minutes
// taking the timer away.
function houseServer(start = {}) {
  let alarms = start.alarms ?? [];
  let stored = start.stored ?? [RADIO, PLAYLIST];
  let sleep = start.sleep ?? [];
  let serial = 1;
  const by = (key) => (a, b) => (a[key] < b[key] ? -1 : 1);
  const now = () => house(serial, { alarms, stored_sources: stored, sleep, ...(start.more ?? {}) });
  const refuse = (field, detail) => ({ status: 400, body: JSON.stringify({ v: 2, t: "error", field, detail }) });
  const server = fakeServer(now());
  server.answer = (body) => {
    const { v: _v, t, ...fields } = JSON.parse(body);
    if (t === "alarm_set") {
      const held = alarms.find((alarm) => alarm.alarm === fields.alarm);
      alarms = [...alarms.filter((alarm) => alarm !== held), { ...fields, ringing: held?.ringing ?? false }].sort(by("alarm"));
    } else if (t === "alarm_delete") alarms = alarms.filter((alarm) => alarm.alarm !== fields.alarm);
    else if (t === "alarm_stop") alarms = alarms.map((alarm) => (alarm.alarm === fields.alarm ? { ...alarm, ringing: false } : alarm));
    else if (t === "source_store") {
      if (fields.kind === "url" && !/^https?:\/\//.test(fields.value)) {
        return refuse("value", `'${fields.value}' is not a URL this server stores: it starts with 'http://' or 'https://'`);
      }
      stored = [...stored.filter((source) => source.id !== fields.id), fields].sort(by("id"));
    } else if (t === "source_forget") {
      const plays = alarms.filter((alarm) => alarm.source === `stored:${fields.id}`).map((alarm) => alarm.alarm);
      if (plays.length > 0) return refuse("id", `the stored source '${fields.id}' is played by the alarm '${plays.join("', '")}'`);
      stored = stored.filter((source) => source.id !== fields.id);
    } else if (t === "sleep") {
      sleep = sleep.filter((timer) => timer.target !== fields.target);
      if (fields.minutes > 0) sleep = [...sleep, { ...fields, remaining_s: fields.minutes * 60 }].sort(by("target"));
    }
    serial += 1;
    server.snapshot = now();
    return { status: 200, body: JSON.stringify(server.snapshot) };
  };
  return server;
}

// A person chooses from a list, types into a field, moves a slider.
function choose(control, value) {
  control.value = value;
  control.dispatchEvent(new Event("change", { bubbles: true }));
}
function type(control, value) {
  control.value = value;
  control.dispatchEvent(new Event("input", { bubbles: true }));
}

// What a picker offers, by group of the list: [value, words] each.
const offered = (picker) =>
  Object.fromEntries(
    [...picker.querySelectorAll("optgroup")].map((group) => [
      group.label,
      [...group.querySelectorAll("option")].map((option) => [option.value, text(option)]),
    ]),
  );

const alarmRows = (app) => [...root(app).querySelectorAll("li[data-alarm]")];
const alarmRow = (app, id) => alarmRows(app).find((item) => item.dataset.alarm === id);
const words = (item, name) => text(item.querySelector(`[data-value="${name}"]`));
const alertOf = (node) => text(node.querySelector("[role=alert]"));
const draftOf = (app, which) => root(app).querySelector(`[data-draft="${which}"]`);
const notes = (app) =>
  Object.fromEntries([...root(app).querySelectorAll("[data-unavailable]")].map((note) => [note.dataset.unavailable, text(note)]));

// Fill the alarm being written, as a person does, and save it.
async function setAlarm(app, { name, target, time, source, volume, ramp, duration }) {
  type(getByLabel(app, "Alarm name"), name);
  if (target) choose(getByLabel(app, "Alarm target"), target);
  if (time) choose(getByLabel(app, "Alarm time"), time);
  if (source) choose(getByLabel(app, "Alarm source"), source);
  if (volume !== undefined) type(getByLabel(app, "Alarm volume"), String(volume));
  if (ramp !== undefined) choose(getByLabel(app, "Alarm ramp, seconds"), String(ramp));
  if (duration !== undefined) choose(getByLabel(app, "Alarm duration, minutes"), String(duration));
  await rendered(app);
  getByLabel(app, "Save alarm").click();
  await rendered(app);
}

test("the commands are the catalog's own vectors", () => {
  const wake = { alarm: "wake", target: "bedroom", time: "06:45", days: ["fri", "mon", "tue", "wed", "thu"], source: "chime:bell", volume: 300, rampS: 30, durationMin: 60, enabled: true }; // prettier-ignore
  assert.equal(alarmSetCommand(wake), fixture("alarm_set.json"));
  assert.equal(alarmSetCommand({ ...wake, alarm: "radio", source: "stored:morning-radio" }), fixture("alarm_set-stored.json"));
  assert.equal(alarmDeleteCommand("wake"), fixture("alarm_delete.json"));
  assert.equal(alarmStopCommand("wake"), fixture("alarm_stop.json"));
  assert.equal(sleepCommand("bedroom", 30), fixture("sleep.json"));
  assert.equal(sourceStoreCommand(RADIO.id, RADIO.kind, RADIO.value, RADIO.name), fixture("source_store.json"));
  assert.equal(sourceStoreCommand(PLAYLIST.id, PLAYLIST.kind, PLAYLIST.value, PLAYLIST.name), fixture("source_store-spotify.json"));
  assert.equal(sourceForgetCommand("morning-radio"), fixture("source_forget.json"));
  // A count is held to the catalog's bounds, and an alarm with no day rings once.
  assert.match(alarmSetCommand({ ...wake, days: [], rampS: 9999, durationMin: -4 }), /"days":\[\],.*"ramp_s":600,"duration_min":0,/);
  assert.equal(sleepCommand("den", 9999), '{"v":2,"t":"sleep","target":"den","minutes":720}');
});

test("the state's alarms, sleep timers, stored sources, chimes and receivers are read as the catalog writes them", () => {
  const facts = JSON.parse(fixture("state-facts.json"));
  assert.deepEqual(chimesOf(facts), ["bell", "ding-dong", "triad"]);
  assert.deepEqual(sleepOf(facts), [
    { target: "bedroom", minutes: 30, remainingS: 1042 },
    { target: "kitchen", minutes: 45, remainingS: 2700 },
  ]);
  const inputs = JSON.parse(fixture("state-inputs.json"));
  assert.deepEqual(storedSourcesOf(inputs), [RADIO, PLAYLIST]);
  assert.deepEqual(alarmsOf(inputs).map((alarm) => [alarm.id, alarm.source, alarm.ringing]), [["radio", "stored:morning-radio", false]]); // prettier-ignore
  assert.deepEqual(alarmsOf({ alarms: [WAKE] }), [
    { id: "wake", target: "bedroom", time: "06:45", days: WAKE.days, source: "chime:bell", volume: 300, rampS: 30, durationMin: 60, enabled: true, ringing: false }, // prettier-ignore
  ]);
  assert.deepEqual(receiversOf(JSON.parse(fixture("state-soloist.json"))), ["room:kitchen"]);
  // A server that says none of it: no chime is known, no receiver runs, and
  // a timer has no count.
  const empty = JSON.parse(fixture("state-empty.json"));
  assert.deepEqual([chimesOf(empty), receiversOf(empty), alarmsOf(empty), sleepOf(empty), storedSourcesOf(empty)], [null, null, [], [], []]);
  assert.deepEqual(sleepOf({ sleep: [{ target: "den", minutes: 20 }] }), [{ target: "den", minutes: 20, remainingS: null }]);
});

test("the screen has an address of its own and opens from the home", async () => {
  assert.equal(addressOf(ALARMS_SCREEN), "#/alarms");
  assert.equal(routeOf("#/alarms").screen, ALARMS_SCREEN);
  const server = fakeServer(house(1, { alarms: [WAKE] }));
  const store = createStore(createClient({ fetch: server.fetch, base: server.base, timers: fakeTimers() }));
  stores.push(store);
  const app = document.createElement("chorus-app");
  app.store = store;
  document.body.append(app);
  store.start();
  await settle();
  await app.updateComplete;
  const link = getByLabel(app, "Alarms and sleep timers");
  assert.equal(link.getAttribute("href"), "#/alarms");
  link.click();
  await rendered(app);
  assert.equal(location.hash, "#/alarms");
  assert.equal(getByLabel(app, "Alarms and sleep timers").localName, "main");
  assert.equal(alarmRows(app).length, 1);
  getByLabel(app, "Back to rooms").click();
  await settle();
  await app.updateComplete;
  assert.equal(getByLabel(app, "Rooms").localName, "main");
});

test("the source picker offers all four kinds, each from the state: the chime names are the server's", async () => {
  const server = fakeServer(house(1));
  const app = await open(server);
  assert.deepEqual(offered(getByLabel(app, "Alarm source")), {
    Chimes: CHIMES.map((name) => [`chime:${name}`, name]),
    Inputs: [
      [`line-in:${DECK}`, "Record Deck"],
      [`line-in:${TV}`, TV],
    ],
    "Stored stream URLs": [["stored:morning-radio", "Morning radio"]],
    "Stored Spotify URIs": [["stored:wake-playlist", "Wake up"]],
  });
  assert.deepEqual(offered(getByLabel(app, "Alarm target")), {
    Rooms: [
      ["bedroom", "Bedroom"],
      ["kitchen", "kitchen"],
      ["den", "den"],
    ],
    "Saved groups": [["upstairs", "Upstairs"]],
  });

  // Another server, other chimes: the picker follows the state.
  server.send(house(2, { chimes: ["gong", "lark"] }));
  await rendered(app);
  assert.deepEqual(offered(getByLabel(app, "Alarm source")).Chimes, [
    ["chime:gong", "gong"],
    ["chime:lark", "lark"],
  ]);
  assert.deepEqual(server.commands, [], "showing sends nothing");

  // And no chime's name is written anywhere in web/src: a server's chimes
  // are in docs/chimes.md and in its state, not in the app.
  const names = [...readFileSync(new URL("../../docs/chimes.md", import.meta.url), "utf8").matchAll(/^\| `([a-z0-9-]+)` \|/gm)].map((found) => found[1]); // prettier-ignore
  assert.ok(names.includes("ding-dong") && names.includes("triad"), `docs/chimes.md lists the chimes: ${names}`);
  const src = new URL("../src/", import.meta.url);
  for (const file of readdirSync(src).filter((name) => name.endsWith(".js"))) {
    const code = readFileSync(new URL(file, src), "utf8").replace(/^\s*\/\/.*$/gm, "");
    for (const name of names) assert.doesNotMatch(code, new RegExp(`["'\`:]${name}["'\`]`), `${file} names the chime ${name}`);
  }
});

test("an alarm is set with each of the four source spellings, and alarm_set carries every required field", async () => {
  const server = houseServer();
  const app = await open(server);
  const set = [
    { name: "chime", source: "chime:ding-dong", target: "bedroom", time: "06:45", volume: 300, ramp: 30, duration: 60 },
    { name: "deck", source: `line-in:${DECK}`, target: "kitchen", time: "07:10", volume: 450, ramp: 0, duration: 0 },
    { name: "radio", source: "stored:morning-radio", target: "upstairs", time: "08:00", volume: 250, ramp: 600, duration: 15 },
    { name: "spotify", source: "stored:wake-playlist", target: "den", time: "09:30", volume: 1000, ramp: 45, duration: 720 },
  ];
  for (const alarm of set) await setAlarm(app, alarm);

  assert.equal(server.commands.length, 4);
  const REQUIRED = ["v", "t", "alarm", "target", "time", "days", "source", "volume", "ramp_s", "duration_min", "enabled"];
  for (const [at, body] of server.commands.entries()) {
    const sent = JSON.parse(body);
    assert.deepEqual(Object.keys(sent), REQUIRED, `the fields of ${body}, in the catalog's order`);
    assert.deepEqual(sent, {
      v: 2,
      t: "alarm_set",
      alarm: set[at].name,
      target: set[at].target,
      time: set[at].time,
      days: ["mon", "tue", "wed", "thu", "fri"],
      source: set[at].source,
      volume: set[at].volume / 1000,
      ramp_s: set[at].ramp,
      duration_min: set[at].duration,
      enabled: true,
    });
  }
  // The four spellings, and the volume as the catalog writes one.
  assert.deepEqual(server.commands.map((body) => JSON.parse(body).source), ["chime:ding-dong", "line-in:deck/line-1", "stored:morning-radio", "stored:wake-playlist"]); // prettier-ignore
  assert.match(server.commands[0], /"volume":0\.300,/);
  assert.match(server.commands[3], /"volume":1\.000,/);

  // The stored ones are the two kinds.
  const kinds = Object.fromEntries(storedSourcesOf(server.snapshot).map((source) => [`stored:${source.id}`, source.kind]));
  assert.deepEqual([kinds["stored:morning-radio"], kinds["stored:wake-playlist"]], ["url", "spotify"]);

  // And the screen lists what the server now holds, each by its kind and name.
  assert.deepEqual(alarmRows(app).map((item) => [item.dataset.alarm, words(item, "when"), words(item, "what")]), [
    ["chime", "06:45, Mon Tue Wed Thu Fri", "Chime: ding-dong in Bedroom, to 30% over 30 s, for 60 min"],
    ["deck", "07:10, Mon Tue Wed Thu Fri", "Input: Record Deck in kitchen, to 45% over 0 s, until stopped"],
    ["radio", "08:00, Mon Tue Wed Thu Fri", "Stream URL: Morning radio in Upstairs, to 25% over 600 s, for 15 min"],
    ["spotify", "09:30, Mon Tue Wed Thu Fri", "Spotify URI: Wake up in den, to 100% over 45 s, for 720 min"],
  ]);
});

test("an alarm with no day rings once, and nothing is sent until it is saved", async () => {
  const server = houseServer();
  const app = await open(server);
  assert.equal(getByLabel(app, "Save alarm").disabled, true, "an alarm has a name");
  for (const day of ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday"]) {
    getByLabel(app, `${day}, the alarm`).click();
    await rendered(app);
  }
  assert.match(words(draftOf(app, "alarm"), "days"), /rings once/);
  getByLabel(app, "Sunday, the alarm").click();
  getByLabel(app, "Alarm switched on").click();
  await rendered(app);
  assert.deepEqual(server.commands, []);
  await setAlarm(app, { name: "nap" });
  // The first room and the first source offered, until a person chooses.
  assert.deepEqual(server.commands, [
    '{"v":2,"t":"alarm_set","alarm":"nap","target":"bedroom","time":"07:00","days":["sun"],"source":"chime:bell","volume":0.300,"ramp_s":30,"duration_min":60,"enabled":false}',
  ]);
  assert.equal(words(alarmRow(app, "nap"), "enabled"), "Off");
});

test("an alarm is enabled, edited, stopped and deleted, each with one command, and the screen follows the state", async () => {
  const server = houseServer({ alarms: [{ ...WAKE, ringing: true }] });
  const app = await open(server);
  const row = () => alarmRow(app, "wake");
  assert.equal(row().hasAttribute("data-ringing"), true);
  assert.equal(words(row(), "ringing"), "Ringing now");

  // Stop: the alarm is kept and no longer rings.
  getByLabel(app, "Stop alarm wake").click();
  await rendered(app);
  assert.deepEqual(server.commands, [fixture("alarm_stop.json")]);
  assert.equal(row().hasAttribute("data-ringing"), false);
  assert.deepEqual(queryAllByLabel(app, "Stop alarm wake"), [], "an alarm that does not ring has nothing to stop");

  // Its switch: the alarm as the server holds it, with `enabled` the opposite.
  assert.equal(getByLabel(app, "Alarm wake").getAttribute("aria-pressed"), "true");
  getByLabel(app, "Alarm wake").click();
  await rendered(app);
  assert.equal(server.commands[1], fixture("alarm_set.json").replace('"enabled":true', '"enabled":false'));
  assert.deepEqual([getByLabel(app, "Alarm wake").getAttribute("aria-pressed"), words(row(), "enabled")], ["false", "Off"]);

  // Edit: the alarm goes into the draft, a field is changed, and saving
  // under the same name replaces it.
  getByLabel(app, "Edit alarm wake").click();
  await rendered(app);
  assert.deepEqual(
    [getByLabel(app, "Alarm name").value, getByLabel(app, "Alarm target").value, getByLabel(app, "Alarm time").value, getByLabel(app, "Alarm source").value],
    ["wake", "bedroom", "06:45", "chime:bell"],
  ); // prettier-ignore
  assert.match(text(draftOf(app, "alarm")), /Saving replaces the alarm "wake"/);
  assert.equal(server.commands.length, 2, "editing sends nothing");
  choose(getByLabel(app, "Alarm time"), "07:15");
  choose(getByLabel(app, "Alarm source"), "stored:morning-radio");
  getByLabel(app, "Alarm switched on").click();
  await rendered(app);
  getByLabel(app, "Save alarm").click();
  await rendered(app);
  assert.equal(
    server.commands[2],
    '{"v":2,"t":"alarm_set","alarm":"wake","target":"bedroom","time":"07:15","days":["mon","tue","wed","thu","fri"],"source":"stored:morning-radio","volume":0.300,"ramp_s":30,"duration_min":60,"enabled":true}',
  );
  assert.equal(alarmRows(app).length, 1);
  assert.deepEqual([words(row(), "when"), words(row(), "enabled")], ["07:15, Mon Tue Wed Thu Fri", "On"]);

  // Delete.
  getByLabel(app, "Delete alarm wake").click();
  await rendered(app);
  assert.equal(server.commands[3], fixture("alarm_delete.json"));
  assert.deepEqual(alarmRows(app), []);
  assert.match(text(root(app).querySelector('[data-none="alarms"]')), /no alarm/);
});

test("nothing is shown before the server answers, and a refused alarm shows the server's field and words", async () => {
  const server = fakeServer(house(1, { alarms: [WAKE] }));
  const app = await open(server);
  // The server takes the command and changes nothing: the switch stays.
  server.answer = () => ({ status: 200, body: JSON.stringify(house(2, { alarms: [WAKE] })) });
  getByLabel(app, "Alarm wake").click();
  await rendered(app);
  assert.equal(getByLabel(app, "Alarm wake").getAttribute("aria-pressed"), "true");

  const detail = "'Wake Up' is not an identifier: lower-case letters, digits and '-'";
  server.answer = () => ({ status: 400, body: JSON.stringify({ v: 2, t: "error", field: "alarm", detail }) });
  await setAlarm(app, { name: "Wake Up" });
  const alert = draftOf(app, "alarm").querySelector("[role=alert]");
  assert.equal(text(alert), `Refused (alarm): ${detail}`);
  assert.equal(alert.getAttribute("data-refusal-field"), "alarm");
  assert.equal(alertOf(alarmRow(app, "wake")), "", "the refusal is shown where it was asked");
  assert.equal(alarmRows(app).length, 1);
});

test("stored sources are added and forgotten from the screen, and a refused scheme is shown by name", async () => {
  const server = houseServer({ stored: [RADIO], alarms: [{ ...WAKE, alarm: "radio", source: "stored:morning-radio" }] });
  const app = await open(server);
  const stored = () => [...root(app).querySelectorAll("li[data-stored]")].map((item) => [item.dataset.stored, words(item, "kind"), words(item, "value")]); // prettier-ignore
  assert.deepEqual(stored(), [["morning-radio", "Stream URL", RADIO.value]]);
  assert.equal(getByLabel(app, "Store source").disabled, true, "a source has an id and an address");

  // A Spotify URI: source_store, the catalog's own vector.
  choose(getByLabel(app, "Stored source kind"), "spotify");
  type(getByLabel(app, "Stored source id"), PLAYLIST.id);
  type(getByLabel(app, "Stored source name"), PLAYLIST.name);
  type(getByLabel(app, "Stored source address"), PLAYLIST.value);
  await rendered(app);
  getByLabel(app, "Store source").click();
  await rendered(app);
  assert.deepEqual(server.commands, [fixture("source_store-spotify.json")]);
  assert.deepEqual(stored(), [
    ["morning-radio", "Stream URL", RADIO.value],
    ["wake-playlist", "Spotify URI", PLAYLIST.value],
  ]);
  // It is offered to an alarm at once.
  assert.deepEqual(offered(getByLabel(app, "Alarm source"))["Stored Spotify URIs"], [["stored:wake-playlist", "Wake up"]]);

  // A stream URL with a scheme the server does not store: refused, by name,
  // on the source being written, and nothing is stored.
  choose(getByLabel(app, "Stored source kind"), "url");
  type(getByLabel(app, "Stored source id"), "old-archive");
  type(getByLabel(app, "Stored source name"), "Old archive");
  type(getByLabel(app, "Stored source address"), "ftp://radio.example/stream.mp3");
  await rendered(app);
  getByLabel(app, "Store source").click();
  await rendered(app);
  assert.equal(
    server.commands[1],
    '{"v":2,"t":"source_store","id":"old-archive","kind":"url","value":"ftp://radio.example/stream.mp3","name":"Old archive"}',
  );
  const alert = draftOf(app, "stored").querySelector("[role=alert]");
  assert.equal(text(alert), "Refused (value): 'ftp://radio.example/stream.mp3' is not a URL this server stores: it starts with 'http://' or 'https://'"); // prettier-ignore
  assert.match(text(alert), /ftp:\/\//, "the refused scheme is named");
  assert.equal(alert.getAttribute("data-refusal-field"), "value");
  assert.equal(stored().length, 2);
  assert.equal(getByLabel(app, "Stored source address").value, "ftp://radio.example/stream.mp3", "what was written stays, to be put right");

  // Put right, it is stored, and the refusal goes.
  type(getByLabel(app, "Stored source address"), "https://archive.example/old.mp3");
  await rendered(app);
  getByLabel(app, "Store source").click();
  await rendered(app);
  assert.equal(text(alert.isConnected ? alert : draftOf(app, "stored").querySelector("[role=alert]")), "");
  assert.deepEqual(stored().map(([id]) => id), ["morning-radio", "old-archive", "wake-playlist"]); // prettier-ignore

  // Forget: source_forget, the catalog's own vector for the one an alarm
  // plays, which the server refuses on that source's row.
  getByLabel(app, "Forget stored source Morning radio").click();
  await rendered(app);
  assert.equal(server.commands.at(-1), fixture("source_forget.json"));
  const row = (id) => root(app).querySelector(`li[data-stored="${id}"]`);
  assert.equal(alertOf(row("morning-radio")), "Refused (id): the stored source 'morning-radio' is played by the alarm 'radio'");
  assert.equal(alertOf(row("old-archive")), "");
  getByLabel(app, "Forget stored source Old archive").click();
  await rendered(app);
  assert.equal(server.commands.at(-1), '{"v":2,"t":"source_forget","id":"old-archive"}');
  assert.deepEqual(stored().map(([id]) => id), ["morning-radio", "wake-playlist"]); // prettier-ignore
});

test("a sleep timer is set, its time left is the state's and counts down between states, and minutes 0 cancels", async () => {
  const server = houseServer();
  const app = await open(server);
  const timers = () => [...root(app).querySelectorAll("li[data-sleep]")].map((item) => [item.dataset.sleep, text(item.querySelector("h4")), words(item, "left")]); // prettier-ignore
  assert.deepEqual(timers(), []);
  assert.match(text(root(app).querySelector('[data-none="sleep"]')), /No sleep timer/);
  // A room or a group formed now.
  assert.deepEqual(offered(getByLabel(app, "Sleep timer target")), {
    Rooms: [
      ["bedroom", "Bedroom"],
      ["kitchen", "kitchen"],
      ["den", "den"],
    ],
    "Groups playing now": [["kitchen+den", "kitchen + den"]],
  });

  // The screen's clock is the test's, so a second passes when the test says.
  let now = 5_000;
  screenOf(app).clock = () => now;

  // Set: the catalog's own vector, and all of it is left.
  getByLabel(app, "Start sleep timer").click();
  await rendered(app);
  assert.deepEqual(server.commands, [fixture("sleep.json")]);
  assert.deepEqual(timers(), [["bedroom", "Bedroom", "30 min 0 s left"]]);

  // Between two states the screen counts down on its own.
  now += 61_400;
  screenOf(app).tick();
  await rendered(app);
  assert.deepEqual(timers(), [["bedroom", "Bedroom", "28 min 59 s left"]]);

  // A state says the truth, whatever the screen had counted to. This one
  // has the serial the screen already holds, as a stream opened again says
  // it: the serial moves only with the whole minutes, the count every second.
  server.send({ ...server.snapshot, sleep: [{ target: "bedroom", minutes: 30, remaining_s: 1042 }] });
  await rendered(app);
  assert.deepEqual(timers(), [["bedroom", "Bedroom", "17 min 22 s left"]]);
  now += 2_000;
  screenOf(app).tick();
  await rendered(app);
  assert.deepEqual(timers(), [["bedroom", "Bedroom", "17 min 20 s left"]]);
  // It never counts past nothing: the entry goes when the state says so.
  now += 3_000_000;
  screenOf(app).tick();
  await rendered(app);
  assert.deepEqual(timers(), [["bedroom", "Bedroom", "0 s left"]]);

  // A second one, for the group, with its own minutes.
  choose(getByLabel(app, "Sleep timer target"), "kitchen+den");
  choose(getByLabel(app, "Sleep timer minutes"), "90");
  await rendered(app);
  getByLabel(app, "Start sleep timer").click();
  await rendered(app);
  assert.equal(server.commands[1], '{"v":2,"t":"sleep","target":"kitchen+den","minutes":90}');
  assert.deepEqual(timers().map(([target, name, left]) => [target, name, left]).at(-1), ["kitchen+den", "kitchen + den", "1 h 30 min left"]); // prettier-ignore

  // Cancel is `sleep` with 0 minutes: the entry leaves the state and the screen.
  getByLabel(app, "Cancel sleep timer for Bedroom").click();
  await rendered(app);
  assert.equal(server.commands[2], '{"v":2,"t":"sleep","target":"bedroom","minutes":0}');
  assert.deepEqual(timers().map(([target]) => target), ["kitchen+den"]); // prettier-ignore

  // And so is 0 minutes asked for in the form.
  choose(getByLabel(app, "Sleep timer minutes"), "0");
  await rendered(app);
  getByLabel(app, "Start sleep timer").click();
  await rendered(app);
  assert.equal(server.commands[3], '{"v":2,"t":"sleep","target":"kitchen+den","minutes":0}');
  assert.deepEqual(timers(), []);

  assert.deepEqual([leftWords(3600), leftWords(59), leftWords(-3)], ["1 h 0 min left", "59 s left", "0 s left"]);
});

test("a server that does not count a timer down shows what was asked for", async () => {
  const app = await open(fakeServer(house(1, { sleep: [{ target: "den", minutes: 20 }] })));
  assert.equal(words(root(app).querySelector('li[data-sleep="den"]'), "left"), "20 min asked for");
});

test("the screen says why a source cannot be played, where the state says so", async () => {
  const spotify = { ...WAKE, alarm: "spotify", source: "stored:wake-playlist", target: "kitchen" };
  const deck = { ...WAKE, alarm: "deck", source: "line-in:attic/line-1" };
  const gone = { ...WAKE, alarm: "gone", source: "stored:lost" };
  const gong = { ...WAKE, alarm: "gong", source: "chime:gong" };
  const server = fakeServer(house(1, { alarms: [deck, gone, gong, spotify, WAKE] }));
  const app = await open(server);
  const fallback = (id) => {
    const note = alarmRow(app, id).querySelector("[data-fallback]");
    return note ? text(note) : null;
  };
  assert.equal(fallback("wake"), null);
  assert.equal(fallback("deck"), "The input attic/line-1 is not offered now: its speaker is not connected. The alarm rings the bell chime instead.");
  assert.equal(fallback("gone"), 'This server has no stored source "lost". The alarm rings the bell chime instead.');
  assert.equal(fallback("gong"), 'This server has no chime "gong". The alarm rings the bell chime instead.');
  // No `soloist` in the state: the server runs no Spotify receiver.
  assert.equal(fallback("spotify"), "This server runs no Spotify receiver. The alarm rings the bell chime instead.");
  assert.match(notes(app).spotify, /runs no Spotify receiver/);

  // Receivers, and one running for the kitchen: its alarm is not doubted,
  // and one for the den, which has none, would be.
  const soloist = { receivers: [{ id: "r0", state: "running", target: "room:kitchen", name: "kitchen" }, { id: "r1", state: "failed", target: "room:den", name: "den" }] }; // prettier-ignore
  server.send(house(2, { alarms: [spotify], soloist }));
  await rendered(app);
  assert.equal(fallback("spotify"), null);
  assert.equal(notes(app).spotify, undefined);
  choose(getByLabel(app, "Alarm source"), "stored:wake-playlist");
  choose(getByLabel(app, "Alarm target"), "den");
  await rendered(app);
  assert.equal(notes(app).spotify, "No Spotify receiver is running for den. The alarm would ring the bell chime instead.");
  choose(getByLabel(app, "Alarm target"), "kitchen");
  await rendered(app);
  assert.equal(notes(app).spotify, undefined);

});

test("a server that names no chime, offers no input and stores nothing says so for each kind", async () => {
  const app = await open(fakeServer(stateOf(3, [zone("den")])));
  assert.deepEqual(Object.keys(notes(app)).sort(), ["chime", "line-in", "spotify", "url"]);
  assert.deepEqual(offered(getByLabel(app, "Alarm source")), {});
  type(getByLabel(app, "Alarm name"), "nothing");
  await rendered(app);
  assert.equal(getByLabel(app, "Save alarm").disabled, true, "an alarm has a source");
});

test("before the first state the screen says it is reading", async () => {
  const server = fakeServer(null);
  const app = await open(server);
  assert.equal(text(root(app).querySelector("[data-missing]")), "Reading this server's alarms.");
  server.send(house(1));
  await rendered(app);
  assert.equal(root(app).querySelector("[data-missing]"), null);
  assert.equal(getByLabel(app, "Alarm source").localName, "select");
});

test("its styles name tokens, never a literal colour or length", () => {
  const sheet = customElements.get("chorus-alarms").styles.cssText;
  assert.match(sheet, /height: var\(--control-size\)/);
  assert.doesNotMatch(sheet, /#[0-9a-f]{3,8}\b|\b\d+(px|rem|em)\b/i);
});
