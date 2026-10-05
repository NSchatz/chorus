// The live test of the alarms screen (`make web-live`): the app's own
// elements, navigation and state layer, in node under happy-dom with no
// browser, against a real chorus-server whose civil clock starts three
// schedule minutes before 07:00 on a Monday and whose schedule runs ten times
// faster (`--civil-time-from`, `--schedule-time-scale`, as
// crates/server/tests/alarms_sleep_autoplay.rs does), so an alarm set for
// 07:00 rings eighteen real seconds after the server starts.
//
// One alarm for each of the four source kinds (K80) is set through the
// screen, each in a room of its own, and each is then seen ringing in the
// server's own `GET /api/state` and on the screen, playing its own kind:
//
//   a chime               the room plays `chime:<name>`, a chime the state
//                         names that is not the fallback's bell
//   a line-in             an endpoint's session offers one (endpoint.js); the
//                         room plays `line-in:<endpoint>/<input>`
//   a stored stream URL   a stream on loopback (house.js, `startStream`), a
//                         server with `--players` and `--media-allow-loopback`;
//                         the room plays `player:<id>`, and the stream was
//                         fetched
//   a stored Spotify URI  the fake Soloist under the real receiver supervisor
//                         (house.js, `startReceiver`), a server with
//                         `--soloist-receivers` and `--soloist-alarms`; the
//                         room plays `soloist:<receiver>`, and the fake was
//                         told to play the URI
//
// A kind that falls back to the chime does not count: the room would then
// play `chime:bell` and the server's log would say `fallback=chime`, and both
// are checked. The screen's "Stop" then ends each one.
//
// Last, a sleep timer set through the screen ends on the scaled clock: its
// entry leaves the state and the screen.
//
// No real Spotify account, token or network is used: the fake Soloist only.
//
// CHORUS_SERVER_BIN names the built chorus-server; tools/web.sh refuses to
// run this file without it.

import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, before, test } from "node:test";

import { createClient } from "../src/api.js";
import "../src/chorus-app.js";
import { createStore } from "../src/state.js";
import { getByLabel, queryAllByLabel } from "../test/label-query.js";
import { offerLineIn } from "./endpoint.js";
import { startHouse, startReceiver, startStream, until } from "./house.js";

// The room whose Spotify receiver the test starts comes first: with one
// receiver, the first room is the one that is given it.
const ROOMS = ["study", "bedroom", "kitchen", "den"];
const RATE_HZ = 44_100;
const ENDPOINT = "live-test-amp";
const INPUT = `${ENDPOINT}/line-1`;
const RADIO = { id: "live-radio", name: "Live Test Radio" };
const PLAYLIST = { id: "live-list", name: "Live Test Wake Up", uri: "spotify:playlist:37i9dQZF1DXcBWIGoYBM5M" };
// Monday, three schedule minutes (eighteen real seconds) before the alarms.
const STARTS = "2026-10-05T06:57:00Z";
const RINGS = "07:00";
// How long anything that waits on the server's schedule may take.
const SCHEDULE_MS = 60_000;

// The four alarms: which room, which source, and what its room then plays.
const ALARMS = [
  { id: "live-chime", room: "bedroom", source: () => `chime:${chime}`, plays: () => `chime:${chime}` },
  { id: "live-deck", room: "kitchen", source: () => `line-in:${INPUT}`, plays: () => `line-in:${INPUT}` },
  { id: "live-radio-alarm", room: "den", source: () => `stored:${RADIO.id}`, plays: () => "player:p0" },
  { id: "live-spotify", room: "study", source: () => `stored:${PLAYLIST.id}`, plays: () => "soloist:r0" },
];

let scratch;
let stream;
let receiver;
let house;
let endpoint;
let store;
let app;
// The chime the test rings: one the server names that is not the bell.
let chime;

const text = (node) => node.textContent.replace(/\s+/g, " ").trim();
const screen = () => app.shadowRoot.querySelector("chorus-alarms");
const root = () => screen()?.shadowRoot ?? null;
const row = (id) => root()?.querySelector(`li[data-alarm="${id}"]`) ?? null;
const sourceOf = (state, room) => state.groups.find((group) => group.zones.includes(room))?.source ?? null;
const alarmOf = (state, id) => state.alarms.find((alarm) => alarm.alarm === id) ?? null;
const options = (label) => [...getByLabel(app, label).querySelectorAll("option")].map((option) => option.value);

// A person chooses from a list, types into a field.
function choose(control, value) {
  control.value = value;
  control.dispatchEvent(new Event("change", { bubbles: true }));
}
function type(control, value) {
  control.value = value;
  control.dispatchEvent(new Event("input", { bubbles: true }));
}

// Write a stored source into the screen's form and store it.
async function store_(kind, { id, name }, value) {
  choose(getByLabel(app, "Stored source kind"), kind);
  type(getByLabel(app, "Stored source id"), id);
  type(getByLabel(app, "Stored source name"), name);
  type(getByLabel(app, "Stored source address"), value);
  await until("the form", () => getByLabel(app, "Store source").disabled, false);
  getByLabel(app, "Store source").click();
  await until(`the stored source ${id} on the screen`, () => root().querySelectorAll(`li[data-stored="${id}"]`).length, 1);
}

before(async () => {
  assert.ok(process.env.CHORUS_SERVER_BIN, "CHORUS_SERVER_BIN names a built chorus-server");
  scratch = await mkdtemp(join(tmpdir(), "cwl-"));
  stream = await startStream(RATE_HZ);
  receiver = await startReceiver(scratch);
  house = await startHouse(ROOMS, {
    extra: [
      // The receivers play at 44.1 kHz, where the TV path's latency floor is
      // just above its default (crates/server/tests/soloist_receivers.rs).
      "--rate", String(RATE_HZ),
      "--tv-latency-ms", "40",
      "--slots", "6",
      "--max-clients", "8",
      "--tz", new URL("../../fixtures/schedule/Etc_UTC.slim.tzif", import.meta.url).pathname,
      "--civil-time-from", STARTS,
      "--schedule-time-scale", "10",
      "--players", "1",
      "--media-allow-loopback",
      "--soloist-dir", receiver.dir,
      "--soloist-receivers", "1",
      "--soloist-alarms",
    ],
  }); // prettier-ignore
  const [, port] = await house.said(/chorus-server: listening on=\S*?:(\d+)/);
  endpoint = await offerLineIn({ host: "127.0.0.1", port: Number(port), endpoint: ENDPOINT });

  // The app, as main.js makes it, reading that server.
  store = createStore(createClient({ base: `${house.origin}/` }));
  app = document.createElement("chorus-app");
  app.store = store;
  document.body.append(app);
  store.start();
});

after(async () => {
  store?.stop();
  app?.remove();
  await endpoint?.stop();
  await house?.stop();
  await receiver?.stop();
  await stream?.stop();
  if (scratch) await rm(scratch, { recursive: true, force: true });
});

test("the alarms screen opens from the home and offers what a real server has", async () => {
  await until("the link under the rooms", () => queryAllByLabel(app, "Alarms and sleep timers").length, 1);
  getByLabel(app, "Alarms and sleep timers").click();
  await until("the screen", () => root()?.querySelectorAll('[data-draft="alarm"]').length ?? 0, 1);
  assert.equal(location.hash, "#/alarms", "the screen has an address of its own");
  await until("the store's status", () => store.view().status, "live");

  // The chimes are the server's own list, and the line-in is the endpoint's.
  const state = await house.state();
  assert.ok(Array.isArray(state.chimes) && state.chimes.length > 1, `the server names its chimes: ${JSON.stringify(state.chimes)}`);
  chime = state.chimes.find((name) => name !== "bell");
  await until("the input the endpoint offers", async () => (await house.state()).inputs, [INPUT]);
  await until("the picker's chimes and inputs", () => options("Alarm source"), [...state.chimes.map((name) => `chime:${name}`), `line-in:${INPUT}`]); // prettier-ignore
  assert.deepEqual(state.alarms, []);
  assert.deepEqual(options("Alarm target"), ROOMS);
});

test("a stream URL and a Spotify URI are stored through the screen, and a refused scheme is the server's to name", async () => {
  await store_("url", RADIO, stream.url);
  await store_("spotify", PLAYLIST, PLAYLIST.uri);
  assert.deepEqual((await house.state()).stored_sources, [
    { id: PLAYLIST.id, kind: "spotify", value: PLAYLIST.uri, name: PLAYLIST.name },
    { id: RADIO.id, kind: "url", value: stream.url, name: RADIO.name },
  ]);
  await until("the picker's stored sources", () => options("Alarm source").slice(-2), [`stored:${RADIO.id}`, `stored:${PLAYLIST.id}`]);

  // A scheme the real server does not store.
  choose(getByLabel(app, "Stored source kind"), "url");
  type(getByLabel(app, "Stored source id"), "live-ftp");
  type(getByLabel(app, "Stored source address"), "ftp://radio.example/stream.mp3");
  await until("the form", () => getByLabel(app, "Stored source address").value, "ftp://radio.example/stream.mp3");
  getByLabel(app, "Store source").click();
  const alert = () => root().querySelector('[data-draft="stored"] [role=alert]');
  await until("the refusal's field", () => alert().getAttribute("data-refusal-field"), "value");
  assert.match(text(alert()), /^Refused \(value\): .*'http:\/\/' or 'https:\/\/'/);
  assert.equal((await house.state()).stored_sources.length, 2, "nothing of it was stored");
});

test("an alarm of each of the four source kinds, set through the screen, rings as its own kind", async () => {
  // The receiver the supervisor runs is the study's, and a person has chosen
  // the device in the Spotify app: both before the alarm's minute.
  await until("the study's receiver", async () => (await house.state()).soloist?.receivers?.map((r) => [r.id, r.state, r.target]), [["r0", "running", "room:study"]], 30_000); // prettier-ignore
  await receiver.login();

  for (const alarm of ALARMS) {
    type(getByLabel(app, "Alarm name"), alarm.id);
    choose(getByLabel(app, "Alarm target"), alarm.room);
    choose(getByLabel(app, "Alarm time"), RINGS);
    choose(getByLabel(app, "Alarm source"), alarm.source());
    choose(getByLabel(app, "Alarm ramp, seconds"), "0");
    choose(getByLabel(app, "Alarm duration, minutes"), "0");
    await until("the form", () => [getByLabel(app, "Alarm name").value, getByLabel(app, "Alarm source").value], [alarm.id, alarm.source()]);
    getByLabel(app, "Save alarm").click();
    await until(`the alarm ${alarm.id} on the screen`, () => (row(alarm.id) ? 1 : 0), 1);
  }

  // What the server holds is what was asked for, field for field.
  const set = (await house.state()).alarms;
  assert.deepEqual(set.map((alarm) => alarm.alarm).sort(), ALARMS.map((alarm) => alarm.id).sort()); // prettier-ignore
  for (const alarm of ALARMS) {
    assert.deepEqual(alarmOf({ alarms: set }, alarm.id), {
      alarm: alarm.id,
      target: alarm.room,
      time: RINGS,
      days: ["mon", "tue", "wed", "thu", "fri"],
      source: alarm.source(),
      volume: 0.3,
      ramp_s: 0,
      duration_min: 0,
      enabled: true,
      ringing: false,
    });
    assert.equal(row(alarm.id).querySelector("[data-fallback]"), null, `the screen has no doubt about ${alarm.id}`);
  }

  // 07:00: every one rings, in the state and on the screen, and its room
  // plays its own kind of source.
  for (const alarm of ALARMS) {
    await until(`${alarm.id} ringing in the server's state`, async () => {
      const state = await house.state();
      return [alarmOf(state, alarm.id).ringing, sourceOf(state, alarm.room)];
    }, [true, alarm.plays()], SCHEDULE_MS); // prettier-ignore
    await until(`${alarm.id} ringing on the screen`, () => [row(alarm.id).hasAttribute("data-ringing"), text(row(alarm.id).querySelector('[data-value="ringing"]') ?? document.body)], [true, "Ringing now"]); // prettier-ignore
    assert.equal(queryAllByLabel(app, `Stop alarm ${alarm.id}`).length, 1);
  }

  // None of them is the fallback: no chime rang in another source's place,
  // the stream was fetched, and the fake Soloist was told to play the URI.
  assert.doesNotMatch(house.log(), /fallback=chime/);
  assert.match(house.log(), new RegExp(`schedule alarm=live-radio-alarm started stored=${RADIO.id} plays=player:p0`));
  assert.match(house.log(), new RegExp(`schedule alarm=live-spotify started stored=${PLAYLIST.id} plays=soloist:r0`));
  assert.equal(stream.opened(), 1, "the server fetched the stored stream");
  assert.ok((await receiver.commands()).includes(`play ${PLAYLIST.uri}`), "the fake Soloist was told to play the stored URI");
  const state = await house.state();
  assert.deepEqual(ALARMS.map((alarm) => sourceOf(state, alarm.room)), [`chime:${chime}`, `line-in:${INPUT}`, "player:p0", "soloist:r0"]); // prettier-ignore
});

test("the screen's stop ends each ringing alarm", async () => {
  for (const alarm of ALARMS) {
    getByLabel(app, `Stop alarm ${alarm.id}`).click();
    await until(`${alarm.id} stopped in the server's state`, async () => {
      const state = await house.state();
      return [alarmOf(state, alarm.id).ringing, sourceOf(state, alarm.room)];
    }, [false, "stream"], SCHEDULE_MS); // prettier-ignore
    await until(`${alarm.id} stopped on the screen`, () => [row(alarm.id).hasAttribute("data-ringing"), queryAllByLabel(app, `Stop alarm ${alarm.id}`).length], [false, 0]); // prettier-ignore
  }
  // Each stop ended its own alarm and no other; the alarms are kept.
  assert.equal((await house.state()).alarms.length, ALARMS.length);
  // The stored stream is closed, not left running, and the receiver paused.
  await until("the stream's connection", () => stream.closed(), 1);
  await until("the receiver", async () => (await receiver.commands()).includes("pause"), true);

  // Deleted through the screen, an alarm leaves the state and the screen.
  getByLabel(app, "Delete alarm live-chime").click();
  await until("the server's alarms", async () => (await house.state()).alarms.map((alarm) => alarm.alarm).includes("live-chime"), false);
  await until("the screen", () => row("live-chime"), null);
});

test("a sleep timer set through the screen ends on the scaled clock: it leaves the state and the screen", async () => {
  const timers = () => [...root().querySelectorAll("li[data-sleep]")].map((item) => item.dataset.sleep);
  assert.deepEqual((await house.state()).sleep, []);
  assert.deepEqual(timers(), []);

  // One minute for the bedroom: six real seconds at ten times the pace.
  choose(getByLabel(app, "Sleep timer target"), "bedroom");
  choose(getByLabel(app, "Sleep timer minutes"), "1");
  await until("the form", () => getByLabel(app, "Sleep timer minutes").value, "1");
  getByLabel(app, "Start sleep timer").click();
  await until("the timer on the screen", () => timers(), ["bedroom"]);
  const [timer] = (await house.state()).sleep;
  assert.deepEqual([timer.target, timer.minutes], ["bedroom", 1]);
  assert.ok(timer.remaining_s > 0 && timer.remaining_s <= 60, `the server counts it down: ${timer.remaining_s} s left`);
  assert.match(text(root().querySelector('li[data-sleep="bedroom"] [data-value="left"]')), /^(1 min 0 s|\d+ s) left$/);

  // It ends by itself: nothing here cancels it.
  await until("the server's sleep timers", async () => (await house.state()).sleep, [], SCHEDULE_MS);
  await until("the screen", () => timers(), []);
  assert.match(text(root().querySelector('[data-none="sleep"]')), /No sleep timer/);

  // And one that is cancelled through the screen goes at once.
  getByLabel(app, "Start sleep timer").click();
  await until("the timer on the screen", () => timers(), ["bedroom"]);
  getByLabel(app, "Cancel sleep timer for bedroom").click();
  await until("the server's sleep timers", async () => (await house.state()).sleep, []);
  await until("the screen", () => timers(), []);
});
