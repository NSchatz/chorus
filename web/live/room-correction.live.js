// The live test of a room's correction screen (`make web-live`): the app's
// own elements, navigation and state layer, in node under happy-dom with no
// browser, against a real chorus-server, its real sweep and its real fitter.
//
// There is no microphone in node, so the screen's capture seam (capture.js:
// the `capture` function the screen opens a session with) is fed the fitter's
// own recordings, `fixtures/roomfit/`, in the microphone's place. Everything
// else is what a person's phone does: the screen is opened from the room's
// card; "Use the microphone" and "Play the sweep and record" are pressed; the
// real server plays its sweep to a real player session in the room (a
// scripted endpoint, endpoint.js) and says when it has ended; the recording
// is uploaded to `POST /api/room-fit`; and what the fitter answers is shown,
// applied, switched and undone through the screen's controls. What is read
// back is the server's own `GET /api/state`.
//
// The fixtures are recordings of a 1 s sweep, not of the 5 s one the server
// plays here, so the session that plays one back names its sweep, as the seam
// lets a source do. What this holds is the walk and the server's answers:
// nothing here is evidence of how a real room, a real phone or a real
// microphone measures.
//
// CHORUS_SERVER_BIN names the built chorus-server; tools/web.sh refuses to
// run this file without it.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { after, before, test } from "node:test";

import { createClient } from "../src/api.js";
import "../src/chorus-app.js";
import { ADVICE, filterWords } from "../src/room-correction.js";
import { createStore } from "../src/state.js";
import { getByLabel, queryAllByLabel } from "../test/label-query.js";
import { offerLineIn } from "./endpoint.js";
import { startHouse, until } from "./house.js";

const LIVING = { id: "living", name: "Live Test Living Room" };
const DEN = { id: "den" };
const SPEAKER = "live-correction-speaker";

// The filters docs/room-correction.md's table gives for the three recordings
// that fit ("The fixtures and what the tests hold").
const FITS = [
  {
    file: "01-two-modes-one-null.wav",
    filters: [
      { freq_hz: 45, gain_db: -9.32, q: 6.409 },
      { freq_hz: 119, gain_db: -6.53, q: 5.135 },
    ],
  },
  {
    file: "02-three-modes.wav",
    filters: [
      { freq_hz: 38, gain_db: -7.23, q: 7.965 },
      { freq_hz: 94, gain_db: -5.94, q: 3.41 },
      { freq_hz: 152, gain_db: 1.37, q: 2.42 },
      { freq_hz: 211, gain_db: -8.7, q: 6.366 },
    ],
  },
  {
    file: "03-strong-mode.wav",
    filters: [
      { freq_hz: 55, gain_db: -12, q: 6.625 },
      { freq_hz: 55, gain_db: -3.06, q: 9.82 },
      { freq_hz: 170, gain_db: -4.97, q: 2.497 },
    ],
  },
];

// And the four it refuses, each by the fitter's name for it.
const REFUSED = [
  { file: "04-too-quiet.wav", name: "too_quiet" },
  { file: "05-clipped.wav", name: "clipped" },
  { file: "06-too-short.wav", name: "too_short" },
  { file: "07-too-noisy.wav", name: "too_noisy" },
];

// A sweep's whole program on the real server is 6.5 s, and the screen records
// a second past its end.
const A_SWEEP_MS = 30_000;

let house;
let speaker;
let store;
let app;
const sessions = [];

// A fixture's samples as the microphone's would arrive: 32-bit floats.
function samplesOf(file) {
  const bytes = readFileSync(new URL(`../../fixtures/roomfit/${file}`, import.meta.url));
  assert.equal(bytes.subarray(36, 40).toString("latin1"), "data", `${file} is a 44-byte header and its samples`);
  assert.deepEqual([bytes.readUInt16LE(22), bytes.readUInt32LE(24), bytes.readUInt16LE(34)], [1, 48000, 16]);
  const count = bytes.readUInt32LE(40) / 2;
  return Float32Array.from({ length: count }, (_, i) => bytes.readInt16LE(44 + 2 * i) / 32768);
}

// The capture seam's session for a fixture: what capture.js's `openMicrophone`
// resolves to, with the file in the microphone's place.
function sessionOf(file) {
  const session = {
    file,
    settings: { echoCancellation: false, noiseSuppression: false, autoGainControl: false, channelCount: 1, sampleRate: 48000 },
    kept: [],
    sampleRate: 48000,
    started: false,
    closed: false,
    start: () => (session.started = true),
    stop: async () => ({ samples: samplesOf(file), sampleRate: 48000, sweep: { sweepMs: 1000 } }),
    close: () => (session.closed = true),
  };
  sessions.push(session);
  return session;
}

const screen = () => app.shadowRoot.querySelector("chorus-room-correction");
const part = (selector) => screen()?.shadowRoot.querySelector(selector) ?? null;
const text = (node) => (node ? node.textContent.replace(/\s+/g, " ").trim() : null);
const listed = (selector) => [...(part(selector)?.querySelectorAll("li") ?? [])].map(text);
const control = (label) => getByLabel(app, `${label} ${LIVING.name}`);

// The room's `room_eq` as the server's own state says it.
const serverCorrection = async () => (await house.state()).zones.find((zone) => zone.id === LIVING.id).room_eq;

// A person measures the room, the "microphone" hearing `file`: the two
// presses, and then whatever the screen shows when the server has answered.
async function measure(file) {
  screen().capture = async () => sessionOf(file);
  control("Use the microphone to measure").click();
  await until("the microphone step", () => Boolean(part("[data-step=measure]")), true);
  assert.equal(text(part('[data-setting="echoCancellation"]')), "off, as asked");
  const before = (await house.state()).measurement?.id ?? 0;
  getByLabel(app, `Play the sweep in ${LIVING.name} and record`).click();
  // The real server plays its sweep in the room, and says so.
  await until(`the server's sweep for ${file}`, async () => {
    const { measurement } = await house.state();
    return measurement && measurement.id === before + 1 ? [measurement.zone, measurement.sweep_ms] : null;
  }, [LIVING.id, 5000]); // prettier-ignore
  const ended = () => ["proposed", "refused", "failed"].includes(part("[data-phase]")?.getAttribute("data-phase"));
  await until(`the server's answer to ${file}`, ended, true, A_SWEEP_MS);
  const session = sessions.at(-1);
  assert.equal(session.started, true, "the recording began before the sweep was asked for");
  assert.equal(session.closed, true, "the microphone was let go");
  assert.equal((await house.state()).measurement.state, "finished");
  return part("[data-phase]")?.getAttribute("data-phase") ?? null;
}

before(async () => {
  assert.ok(process.env.CHORUS_SERVER_BIN, "CHORUS_SERVER_BIN names a built chorus-server");
  // `--slots`: the sweep plays on a stream of its own beside the slots.
  house = await startHouse([LIVING.id, DEN.id], { extra: ["--slots", "4"] });
  await house.command(JSON.stringify({ v: 1, t: "name", zone: LIVING.id, name: LIVING.name }));
  // A speaker in the living room with a player session up: the server plays
  // a sweep only in a room that can play it.
  const [, port] = await house.said(/chorus-server: listening on=\S*?:(\d+)/);
  speaker = await offerLineIn({ host: "127.0.0.1", port: Number(port), endpoint: SPEAKER });
  await until("the speaker's session", async () => (await house.state()).inputs?.length ?? 0, 1);
  await house.command(JSON.stringify({ v: 2, t: "speaker_room", speaker: SPEAKER, room: LIVING.id }));

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
  await speaker?.stop();
  await house?.stop();
});

test("the correction screen opens from the room's card and shows a room with no correction", async () => {
  await until("the link on the living room's card", () => queryAllByLabel(app, `Correction for ${LIVING.name}`).length, 1);
  getByLabel(app, `Correction for ${LIVING.name}`).click();
  await until("the screen", () => text(part("[data-held]")), "This room has no correction.");
  assert.equal(getByLabel(app, `Correction of ${LIVING.name}`).localName, "main");
  assert.equal(location.hash, "#/rooms/living/correction");
  assert.ok(part("[data-guide]"), "the guidance is what the walk starts with");
  assert.deepEqual(await serverCorrection(), { enabled: true, filters: [] });
  assert.equal(control("Correction for").disabled, true);
  assert.equal(control("Undo the last correction of").disabled, true);
  await until("the store's status", () => store.view().status, "live");
});

test("fixtures 01, 02 and 03 through the screen end with the documented filters in the server's state", async () => {
  let earlier = { enabled: true, filters: [] };
  for (const { file, filters } of FITS) {
    assert.equal(await measure(file), "proposed", `${file}: ${text(part("[role=alert]"))}`);
    // Proposed, and shown; the room is as it was.
    assert.deepEqual(listed("[data-phase=proposed]"), filters.map(filterWords));
    const { undo: _, ...unchanged } = await serverCorrection();
    assert.deepEqual(unchanged, earlier, `${file}: nothing is applied by the measurement`);
    control("Apply the proposed correction to").click();
    await until(`the server's room_eq after ${file} is applied`, serverCorrection, { enabled: true, filters, undo: true });
    await until("the screen's own list", () => listed("[data-held]"), filters.map(filterWords));
    assert.equal(control("Correction for").getAttribute("aria-pressed"), "true");
    earlier = { enabled: true, filters };
  }
  // The den was never touched.
  assert.deepEqual((await house.state()).zones.find((zone) => zone.id === DEN.id).room_eq, { enabled: true, filters: [] });
});

test("fixtures 04 to 07 end in their named refusal, with the room's correction unchanged", async () => {
  const held = { enabled: true, filters: FITS[2].filters, undo: true };
  for (const { file, name } of REFUSED) {
    assert.equal(await measure(file), "refused", `${file}: ${text(part("[role=alert]"))}`);
    const alert = part("[data-phase=refused]");
    assert.equal(alert.getAttribute("data-refusal"), name);
    assert.match(text(alert), new RegExp(`^The server refused the recording: ${name}: \\S`));
    assert.equal(text(part("[data-advice]")), ADVICE[name]);
    assert.deepEqual(queryAllByLabel(app, `Apply the proposed correction to ${LIVING.name}`), []);
    // Switched off for the sweep, and back as it was.
    await until(`the server's room_eq after ${file}`, serverCorrection, held);
    assert.deepEqual(listed("[data-held]"), held.filters.map(filterWords));
  }
});

test("undo through the screen restores the earlier correction, and the enable switch is read back", async () => {
  // What stood before the last apply (fixture 03's) is fixture 02's fit.
  const earlier = FITS[1].filters;
  control("Undo the last correction of").click();
  await until("the server's room_eq after the undo", serverCorrection, { enabled: true, filters: earlier });
  await until("the screen's list", () => listed("[data-held]"), earlier.map(filterWords));
  await until("the undo button", () => control("Undo the last correction of").disabled, true);

  const toggle = control("Correction for");
  toggle.click();
  await until("the server's room_eq, switched off", serverCorrection, { enabled: false, filters: earlier });
  await until("the switch", () => toggle.getAttribute("aria-pressed"), "false");
  assert.equal(text(part('[data-value="enabled"]')), "Off");
  toggle.click();
  await until("the server's room_eq, switched on", serverCorrection, { enabled: true, filters: earlier });
  await until("the switch", () => toggle.getAttribute("aria-pressed"), "true");
  // A switch from another client reaches the screen with no reload.
  await house.command('{"v":2,"t":"room_eq","zone":"living","enabled":false}');
  await until("the switch", () => toggle.getAttribute("aria-pressed"), "false");
  assert.equal(text(part("[data-command-refusal]")), "", "nothing was refused");
  // The server took seven recordings and kept none: one line of counts each.
  assert.equal(house.log().match(/room fit: room 'living', a 1000 ms sweep/g).length, 7);
});
