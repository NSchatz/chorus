// A room's correction screen (src/room-correction.js) and the microphone
// behind it (src/capture.js, src/capture-worklet.js), in the shell, over a
// scripted server and a scripted browser: a faked `navigator.mediaDevices`,
// AudioContext and AudioWorkletNode. No microphone, no audio and no server
// is real here; the real server and the fitter's fixtures are the live
// test's (live/room-correction.live.js).
//
// What is held: what the browser is asked for and what is shown of its
// answer; that every way of having no microphone ends in words; that the
// microphone's track is stopped when the walk ends or is left; each of the
// fitter's refusals with its own advice; that nothing is applied before the
// apply; and that the recording goes to this server's one route and nowhere
// else, and is not kept.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { afterEach, beforeEach, test } from "node:test";

import {
  MAX_RECORDING_BYTES,
  createClient,
  filterLiteral,
  measureSweepCommand,
  refusalName,
  roomEqCommand,
  roomEqEnabledCommand,
  roomEqUndoCommand,
} from "../src/api.js";
import { CAPTURE_PROCESSOR, CaptureError, REASONS, WANTED, WORKLET_URL, keptOn, openMicrophone, resample, toUploadRate, unreported, wavOf } from "../src/capture.js"; // prettier-ignore
import "../src/chorus-app.js";
import { ADVICE, ADVICE_OTHER, AFTER_MS, CORRECTION_SCREEN, GRACE_MS, filterWords } from "../src/room-correction.js";
import { addressOf } from "../src/routes.js";
import { correctionOf, createStore, measurementOf } from "../src/state.js";
import { fakeServer, fakeTimers, settle, stateOf, zone } from "./fake-server.js";
import { getByLabel, queryAllByLabel } from "./label-query.js";

const NAME = "Living Room";
const FILTERS = [
  { freq_hz: 45, gain_db: -9.32, q: 6.409 },
  { freq_hz: 55, gain_db: -12, q: 6.625 },
];
const EARLIER = [{ freq_hz: 80, gain_db: -4.5, q: 3 }];
const FIT = { v: 2, t: "room_fit", zone: "living", sweep_ms: 5000, filters: FILTERS, rms_before_db: 3.21, rms_after_db: 0.37 };

// ---- the scripted browser ----

// What a page is given in place of the browser's microphone and audio graph.
// `settings` is what the track's getSettings() says; `refuse` is the name of
// the DOMException getUserMedia rejects with. It remembers every constraint
// asked, every track and whether it was stopped, and every context.
function fakeBrowser({ settings = { echoCancellation: false, noiseSuppression: false, autoGainControl: false, channelCount: 1, sampleRate: 48000 }, refuse = null, rate = 48000 } = {}) {
  const browser = { asked: [], tracks: [], contexts: [], nodes: [], modules: [] };
  browser.mediaDevices = {
    async getUserMedia(constraints) {
      browser.asked.push(constraints);
      if (refuse) throw Object.assign(new Error(refuse), { name: refuse });
      const track = { kind: "audio", stopped: false, stop: () => (track.stopped = true), getSettings: () => ({ ...settings }) };
      browser.tracks.push(track);
      return { getAudioTracks: () => [track], getTracks: () => [track] };
    },
  };
  browser.AudioContext = class {
    constructor(options) {
      this.asked = options ?? null;
      this.sampleRate = rate;
      this.state = "running";
      this.closed = false;
      this.destination = {};
      this.audioWorklet = { addModule: async (url) => void browser.modules.push(url) };
      browser.contexts.push(this);
    }
    createMediaStreamSource() {
      return { connect() {}, disconnect() {} };
    }
    async close() {
      this.closed = true;
    }
  };
  browser.AudioWorkletNode = class {
    constructor(context, name) {
      this.name = name;
      this.said = [];
      this.port = {
        onmessage: null,
        postMessage: (message) => {
          this.said.push(message);
          // The audio thread answers a flush with what it held and its word.
          if (message === "flush") queueMicrotask(() => this.port.onmessage?.({ data: { flushed: true } }));
        },
      };
      browser.nodes.push(this);
    }
    connect() {}
    disconnect() {}
    // The audio thread hands the page a batch.
    feed(samples) {
      this.port.onmessage?.({ data: { samples } });
    }
  };
  return browser;
}

const OWN = ["isSecureContext", "AudioContext", "webkitAudioContext", "AudioWorkletNode"];
let saved;

// Make `browser` the page's: its mediaDevices on `navigator`, its audio
// classes as globals, and the page a secure context or not.
function install(browser, { secure = true, mediaDevices = true } = {}) {
  Object.defineProperty(navigator, "mediaDevices", { configurable: true, value: mediaDevices ? browser.mediaDevices : undefined });
  Object.defineProperty(globalThis, "isSecureContext", { configurable: true, writable: true, value: secure });
  globalThis.AudioContext = browser.AudioContext;
  globalThis.AudioWorkletNode = browser.AudioWorkletNode;
  return browser;
}

// Every storage a page has, watched: a write to any of them is remembered.
let storageWrites;
let restoreStorage;
function watchStorage() {
  storageWrites = [];
  const proto = Object.getPrototypeOf(localStorage);
  const setItem = proto.setItem;
  proto.setItem = function (key) {
    storageWrites.push(`storage.setItem(${key})`);
  };
  const trap = (name) =>
    new Proxy({}, { get: (_, member) => (typeof member === "symbol" ? undefined : () => void storageWrites.push(`${name}.${member}`)) });
  const before = { indexedDB: globalThis.indexedDB, caches: globalThis.caches };
  globalThis.indexedDB = trap("indexedDB");
  globalThis.caches = trap("caches");
  const cookie = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(document), "cookie");
  restoreStorage = () => {
    proto.setItem = setItem;
    globalThis.indexedDB = before.indexedDB;
    globalThis.caches = before.caches;
  };
  return cookie;
}

let stores = [];

beforeEach(() => {
  saved = Object.fromEntries(OWN.map((name) => [name, Object.getOwnPropertyDescriptor(globalThis, name)]));
  watchStorage();
});

afterEach(() => {
  for (const store of stores) store.stop();
  stores = [];
  document.body.replaceChildren();
  history.replaceState(null, "", "/");
  restoreStorage();
  delete navigator.mediaDevices;
  for (const name of OWN) {
    if (saved[name]) Object.defineProperty(globalThis, name, saved[name]);
    else delete globalThis[name];
  }
});

// ---- the scripted server ----

// A server with the living room and a den: it answers the four commands the
// screen sends as the catalog describes them, and `POST api/room-fit` with
// whatever `house.fit` says ({ status, body }). Every request that carries a
// body is remembered whole.
function fakeHouse(roomEq = { enabled: true, filters: [] }) {
  const server = fakeServer();
  const house = { server, serial: 1, roomEq: { ...roomEq }, earlier: null, measurement: null, uploads: [], bodies: [] };
  house.fit = { status: 200, body: JSON.stringify(FIT) };
  house.sweep = null; // a refusal of measure_sweep: { field, detail }
  house.state = () =>
    stateOf(
      house.serial,
      [
        zone("living", { name: NAME, room_eq: { ...house.roomEq, ...(house.earlier ? { undo: true } : {}) } }),
        zone("den", { room_eq: { enabled: true, filters: [] } }),
      ],
      house.measurement ? { measurement: house.measurement } : {},
    );
  const changed = () => {
    house.serial += 1;
    server.snapshot = house.state();
    return { status: 200, body: JSON.stringify(server.snapshot) };
  };
  const refused = (field, detail) => ({ status: 400, body: JSON.stringify({ v: 2, t: "error", field, detail }) });
  server.snapshot = house.state();
  server.answer = (body) => {
    const message = JSON.parse(body);
    if (message.t === "measure_sweep") {
      if (house.sweep) return refused(house.sweep.field, house.sweep.detail);
      house.measurement = { id: (house.measurement?.id ?? 0) + 1, zone: message.zone, state: "playing", volume: 0.5, lead_ms: 500, sweep_ms: 5000, tail_ms: 1000 }; // prettier-ignore
      return changed();
    }
    if (message.t === "room_eq") {
      if (message.filters) {
        house.earlier = { ...house.roomEq };
        house.roomEq = { enabled: message.enabled ?? house.roomEq.enabled, filters: message.filters };
      } else house.roomEq = { ...house.roomEq, enabled: message.enabled };
      return changed();
    }
    if (message.t === "room_eq_undo") {
      if (!house.earlier) return refused("zone", "nothing-to-undo: room 'living' has no earlier correction");
      house.roomEq = house.earlier;
      house.earlier = null;
      return changed();
    }
    return refused("t", "not a command of this test");
  };
  // The sweep's program ends, or is called off: the server says so on the
  // event stream, as it does for every change.
  house.end = (state = "finished", reason) => {
    house.measurement = { ...house.measurement, state, ...(reason ? { reason } : {}) };
    house.serial += 1;
    server.snapshot = house.state();
    server.send(server.snapshot);
  };
  house.fetch = async (url, options = {}) => {
    if (options.body !== undefined) house.bodies.push({ url: String(url), body: options.body });
    const route = String(url).slice(server.base.length);
    if (route.startsWith("api/room-fit")) {
      house.uploads.push({ url: String(url), options });
      const { status, body } = house.fit;
      return { ok: status >= 200 && status < 300, status, text: async () => body, json: async () => JSON.parse(body) };
    }
    return server.fetch(url, options);
  };
  return house;
}

const refusalOfFit = (name, words) => ({
  status: 422,
  body: JSON.stringify({ v: 2, t: "error", field: "recording", detail: `${name}: ${words}` }),
});

const screenOf = (app) => app.shadowRoot.querySelector("chorus-room-correction");
const text = (node) => (node ? node.textContent.replace(/\s+/g, " ").trim() : null);
const part = (app, selector) => screenOf(app).shadowRoot.querySelector(selector);
const phase = (app) => part(app, "[data-phase]")?.getAttribute("data-phase") ?? (part(app, "[data-guide]") ? "guide" : part(app, "[data-step=measure]") ? "ready" : null);

async function rendered(app) {
  await settle();
  await app.updateComplete;
  await screenOf(app)?.updateComplete;
}

// The app, opened at the living room's correction screen, over `house`.
async function open(house, room = "living") {
  history.replaceState(null, "", `/app/${addressOf(CORRECTION_SCREEN, { room })}`);
  const store = createStore(createClient({ fetch: house.fetch, base: house.server.base, timers: fakeTimers() }));
  stores.push(store);
  const app = document.createElement("chorus-app");
  app.store = store;
  document.body.append(app);
  store.start();
  await rendered(app);
  const timers = fakeTimers();
  screenOf(app).timers = timers;
  return { app, timers, store };
}

const press = async (app, label) => {
  getByLabel(app, label).click();
  await rendered(app);
};

// A person's whole measurement, up to the server's answer to the recording:
// the microphone, the sweep, `samples` from the audio thread, the sweep's end.
async function measure({ app, timers }, house, browser, samples = new Float32Array([0, 0.25, -0.25, 0.5])) {
  await press(app, `Use the microphone to measure ${NAME}`);
  assert.equal(phase(app), "ready");
  await press(app, `Play the sweep in ${NAME} and record`);
  assert.equal(phase(app), "recording");
  browser.nodes.at(-1).feed(samples);
  house.end();
  await rendered(app);
  timers.fire(AFTER_MS);
  await rendered(app);
  await rendered(app);
}

// ---- the microphone ----

test("the browser is asked for the microphone with its three kinds of processing off and one channel", async () => {
  const browser = install(fakeBrowser());
  const opened = await open(fakeHouse());
  assert.equal(phase(opened.app), "guide");
  assert.deepEqual(browser.asked, [], "nothing is asked before a person's own press");
  await press(opened.app, `Use the microphone to measure ${NAME}`);
  assert.deepEqual(browser.asked, [
    { audio: { echoCancellation: false, noiseSuppression: false, autoGainControl: false, channelCount: 1 }, video: false },
  ]);
  assert.deepEqual({ ...WANTED }, browser.asked[0].audio);
  // Uncompressed samples: an audio worklet, in a context asked for at the upload's rate.
  assert.deepEqual(browser.modules, [WORKLET_URL]);
  assert.equal(browser.nodes[0].name, CAPTURE_PROCESSOR);
  assert.deepEqual(browser.contexts[0].asked, { sampleRate: 48000 });
});

test("what getSettings() says is shown, and a kind of processing the browser kept on is flagged", async () => {
  // This browser switched the echo canceller off, kept noise suppression on,
  // and says nothing of its gain control.
  install(fakeBrowser({ settings: { echoCancellation: false, noiseSuppression: true, channelCount: 2, sampleRate: 44100 } }));
  const { app } = await open(fakeHouse());
  await press(app, `Use the microphone to measure ${NAME}`);
  const shown = (name) => text(part(app, `[data-setting="${name}"]`));
  assert.equal(shown("echoCancellation"), "off, as asked");
  assert.equal(shown("noiseSuppression"), "on (true): asked off, and the browser kept it on");
  assert.equal(shown("autoGainControl"), "not reported by this browser");
  assert.equal(shown("channelCount"), "2");
  assert.equal(shown("sampleRate"), "44100 Hz");
  assert.equal(shown("recordedAt"), "48000 Hz");
  assert.ok(part(app, '[data-setting="noiseSuppression"]').hasAttribute("data-kept"));
  assert.ok(!part(app, '[data-setting="echoCancellation"]').hasAttribute("data-kept"));
  assert.ok(!part(app, '[data-setting="autoGainControl"]').hasAttribute("data-kept"));
  assert.match(text(part(app, "[data-flag]")), /^This browser kept noise suppression on\. .*may be wrong\.$/);
  assert.equal(part(app, "[data-flag]").getAttribute("role"), "alert");
  assert.deepEqual(keptOn({ echoCancellation: "all", noiseSuppression: false }), ["echoCancellation"]);
  assert.deepEqual(unreported({ echoCancellation: false }), ["noiseSuppression", "autoGainControl"]);
});

test("a browser that switched all three off is not flagged", async () => {
  install(fakeBrowser());
  const { app } = await open(fakeHouse());
  await press(app, `Use the microphone to measure ${NAME}`);
  assert.equal(part(app, "[data-flag]"), null);
  assert.equal(screenOf(app).shadowRoot.querySelectorAll("[data-kept]").length, 0);
});

// Every way of having no microphone ends in its own words and a way to try
// again; none leaves the screen asking.
const NO_MICROPHONE = [
  { what: "a denied microphone", browser: { refuse: "NotAllowedError" }, reason: "denied", asked: 1 },
  { what: "a missing microphone", browser: { refuse: "NotFoundError" }, reason: "missing", asked: 1 },
  { what: "a microphone another app holds", browser: { refuse: "NotReadableError" }, reason: "busy", asked: 1 },
  { what: "an insecure context", page: { secure: false, mediaDevices: false }, reason: "insecure", asked: 0 },
  { what: "a browser with no mediaDevices", page: { mediaDevices: false }, reason: "unsupported", asked: 0 },
];

for (const { what, browser: options, page, reason, asked } of NO_MICROPHONE) {
  test(`${what} ends in a stated message, not a hang`, async () => {
    const browser = install(fakeBrowser(options), page);
    const house = fakeHouse();
    const { app } = await open(house);
    await press(app, `Use the microphone to measure ${NAME}`);
    assert.equal(phase(app), "failed");
    assert.equal(text(part(app, "[data-phase=failed]")), REASONS[reason]);
    assert.equal(part(app, "[data-phase=failed]").getAttribute("role"), "alert");
    assert.equal(browser.asked.length, asked);
    assert.equal(getByLabel(app, `Use the microphone to measure ${NAME}`).textContent.trim(), "Measure again");
    assert.deepEqual(house.server.commands, [], "no sweep was played");
    assert.deepEqual(house.uploads, []);
  });
}

test("the messages are five different ones, and HTTPS is named where it is the reason", () => {
  assert.equal(new Set(Object.values(REASONS)).size, Object.keys(REASONS).length);
  assert.match(REASONS.insecure, /HTTPS/);
  assert.match(new CaptureError("failed", "TypeError").message, /\(TypeError\)$/);
});

test("a browser with no audio worklet is told so, and a microphone opened on the way is stopped", async () => {
  const browser = fakeBrowser();
  const env = { isSecureContext: true, navigator: { mediaDevices: browser.mediaDevices }, AudioContext: browser.AudioContext };
  await assert.rejects(openMicrophone(env), (error) => error instanceof CaptureError && error.reason === "unsupported");
  assert.equal(browser.asked.length, 0, "the microphone is not asked for by a page that could not record it");
  // A context that cannot load the worklet: the track that was opened is let go.
  const broken = fakeBrowser();
  broken.AudioContext = class extends broken.AudioContext {
    constructor(options) {
      super(options);
      this.audioWorklet = { addModule: async () => Promise.reject(new Error("no module")) };
    }
  };
  await assert.rejects(
    openMicrophone({ isSecureContext: true, navigator: { mediaDevices: broken.mediaDevices }, AudioContext: broken.AudioContext, AudioWorkletNode: broken.AudioWorkletNode }), // prettier-ignore
    (error) => error.reason === "unsupported",
  );
  assert.deepEqual(broken.tracks.map((track) => track.stopped), [true]);
  assert.deepEqual(broken.contexts.map((context) => context.closed), [true, true], "at the upload's rate, then at its own");
});

test("a browser that will not make a context at 48 kHz records at its own rate", async () => {
  const browser = fakeBrowser({ rate: 44100 });
  browser.AudioContext = class extends browser.AudioContext {
    constructor(options) {
      if (options?.sampleRate) throw new Error("NotSupportedError");
      super(options);
    }
  };
  const session = await openMicrophone({ isSecureContext: true, navigator: { mediaDevices: browser.mediaDevices }, AudioContext: browser.AudioContext, AudioWorkletNode: browser.AudioWorkletNode, setTimeout, clearTimeout }); // prettier-ignore
  assert.equal(session.sampleRate, 44100);
  session.start();
  browser.nodes[0].feed(new Float32Array(441).fill(0.5));
  const recording = await session.stop();
  assert.equal(recording.sampleRate, 44100);
  const samples = toUploadRate(recording);
  assert.equal(samples.length, 480);
  assert.ok(Math.abs(samples[240] - 0.5) < 1e-6, "a constant comes through as itself");
});

// ---- the track is stopped ----

test("the microphone track is stopped when the recording is taken, before the server has answered", async () => {
  const browser = install(fakeBrowser());
  const house = fakeHouse();
  let answer;
  const fetch = house.fetch;
  house.fetch = (url, options) =>
    String(url).includes("api/room-fit")
      ? new Promise((resolve) => (answer = () => resolve(fetch(url, options))))
      : fetch(url, options);
  const opened = await open(house);
  await measure(opened, house, browser);
  assert.equal(phase(opened.app), "fitting");
  assert.deepEqual(browser.tracks.map((track) => track.stopped), [true]);
  assert.deepEqual(browser.contexts.map((context) => context.closed), [true]);
  assert.match(text(part(opened.app, "[data-phase=fitting]")), /The microphone is off\.$/);
  answer();
  await rendered(opened.app);
  assert.equal(phase(opened.app), "proposed");
});

test("the microphone track is stopped when the walk is stopped, when the screen is left, and when a sweep fails", async () => {
  // Stopped by its own button.
  let browser = install(fakeBrowser());
  let house = fakeHouse();
  let opened = await open(house);
  await press(opened.app, `Use the microphone to measure ${NAME}`);
  assert.deepEqual(browser.tracks.map((track) => track.stopped), [false]);
  await press(opened.app, `Stop measuring ${NAME}`);
  assert.deepEqual(browser.tracks.map((track) => track.stopped), [true]);
  assert.equal(phase(opened.app), "guide");

  // Left by the app's own Back, with the microphone open.
  await press(opened.app, `Use the microphone to measure ${NAME}`);
  assert.deepEqual(browser.tracks.map((track) => track.stopped), [true, false]);
  await press(opened.app, "Back to rooms");
  assert.equal(screenOf(opened.app), null);
  assert.deepEqual(browser.tracks.map((track) => track.stopped), [true, true]);
  assert.deepEqual(browser.contexts.map((context) => context.closed), [true, true]);
  opened.app.remove();

  // Left while the sweep plays: stopped, and nothing is uploaded afterwards.
  browser = install(fakeBrowser());
  house = fakeHouse();
  opened = await open(house);
  await press(opened.app, `Use the microphone to measure ${NAME}`);
  await press(opened.app, `Play the sweep in ${NAME} and record`);
  const screen = screenOf(opened.app);
  screen.remove();
  assert.deepEqual(browser.tracks.map((track) => track.stopped), [true]);
  house.end();
  await settle();
  opened.timers.fire(AFTER_MS);
  await settle();
  assert.deepEqual(house.uploads, []);
  opened.app.remove();

  // A sweep the server refuses to play.
  browser = install(fakeBrowser());
  house = fakeHouse();
  house.sweep = { field: "zone", detail: "muted: room 'living' is muted, so the sweep would be silence" };
  opened = await open(house);
  await press(opened.app, `Use the microphone to measure ${NAME}`);
  await press(opened.app, `Play the sweep in ${NAME} and record`);
  assert.equal(text(part(opened.app, "[data-phase=failed]")), "The sweep was not played: muted: room 'living' is muted, so the sweep would be silence");
  assert.deepEqual(browser.tracks.map((track) => track.stopped), [true]);
  assert.deepEqual(house.uploads, []);
});

test("a sweep that is called off, or whose end the server never says, ends in words and uploads nothing", async () => {
  let browser = install(fakeBrowser());
  let house = fakeHouse();
  let opened = await open(house);
  await press(opened.app, `Use the microphone to measure ${NAME}`);
  await press(opened.app, `Play the sweep in ${NAME} and record`);
  house.end("cancelled", "alarm 'wake' rings in the room");
  await rendered(opened.app);
  assert.equal(text(part(opened.app, "[data-phase=failed]")), "The sweep was called off: alarm 'wake' rings in the room. Measure again.");
  assert.deepEqual(browser.tracks.map((track) => track.stopped), [true]);
  assert.deepEqual(house.uploads, []);
  opened.app.remove();

  browser = install(fakeBrowser());
  house = fakeHouse();
  opened = await open(house);
  await press(opened.app, `Use the microphone to measure ${NAME}`);
  await press(opened.app, `Play the sweep in ${NAME} and record`);
  // The program is 6.5 s; nothing is heard of it by then and GRACE_MS more.
  assert.equal(opened.timers.pending(6500 + GRACE_MS), 1);
  opened.timers.fire(6500 + GRACE_MS);
  await rendered(opened.app);
  assert.match(text(part(opened.app, "[data-phase=failed]")), /^The server did not say that the sweep ended/);
  assert.deepEqual(browser.tracks.map((track) => track.stopped), [true]);
  assert.deepEqual(house.uploads, []);
});

// ---- the fit, the refusals and the apply ----

test("the proposed filters are shown with the fitter's figures, and nothing is applied until the apply action", async () => {
  const browser = install(fakeBrowser());
  const house = fakeHouse();
  const opened = await open(house);
  const { app } = opened;
  await measure(opened, house, browser);
  assert.equal(phase(app), "proposed");
  assert.deepEqual(
    [...part(app, "[data-phase=proposed]").querySelectorAll("li")].map(text),
    ["45 Hz, -9.32 dB, Q 6.409", "55 Hz, -12.00 dB, Q 6.625"],
  );
  assert.match(text(part(app, "[data-rms]")), /3\.21 dB before, 0\.37 dB predicted after/);
  // Up to here the page has sent the sweep and nothing that changes the room.
  assert.deepEqual(house.server.commands, [measureSweepCommand("living")]);
  assert.deepEqual(house.roomEq, { enabled: true, filters: [] });
  assert.equal(text(part(app, "[data-held]")), "This room has no correction.");

  await press(app, `Apply the proposed correction to ${NAME}`);
  assert.deepEqual(house.server.commands, [
    measureSweepCommand("living"),
    '{"v":2,"t":"room_eq","zone":"living","filters":[{"freq_hz":45,"gain_db":-9.32,"q":6.409},{"freq_hz":55,"gain_db":-12.00,"q":6.625}],"enabled":true}',
  ]);
  // What is shown now is the server's state, not the page's proposal.
  assert.deepEqual([...part(app, "[data-held]").querySelectorAll("li")].map(text), FILTERS.map(filterWords));
  assert.equal(text(part(app, '[data-value="enabled"]')), "On");
  assert.equal(getByLabel(app, `Undo the last correction of ${NAME}`).disabled, false);
  assert.equal(phase(app), "guide");
});

test("a proposal that is discarded applies nothing", async () => {
  const browser = install(fakeBrowser());
  const house = fakeHouse();
  const opened = await open(house);
  await measure(opened, house, browser);
  await press(opened.app, `Discard the proposed correction for ${NAME}`);
  assert.equal(phase(opened.app), "guide");
  assert.deepEqual(house.server.commands, [measureSweepCommand("living")]);
  assert.deepEqual(house.roomEq, { enabled: true, filters: [] });
});

test("a fit with nothing to correct offers no apply", async () => {
  const browser = install(fakeBrowser());
  const house = fakeHouse();
  house.fit = { status: 200, body: JSON.stringify({ ...FIT, filters: [] }) };
  const opened = await open(house);
  await measure(opened, house, browser);
  assert.equal(text(part(opened.app, "[data-nothing]")), "The server found nothing to correct in this recording.");
  assert.equal(queryAllByLabel(opened.app, `Apply the proposed correction to ${NAME}`).length, 0);
});

// The fitter's four refusals, in its own words (docs/room-correction.md,
// "The pipeline", step 2), each with the advice that is its own.
const REFUSALS = [
  { name: "too_short", words: "the recording has 36000 samples, fewer than the 72000 the sweep and its response need", advice: /ended before the sweep/ },
  { name: "clipped", words: "115 consecutive samples are at full scale", advice: /Turn the room down/ },
  { name: "too_quiet", words: "the recording's peak is -65.2 dBFS, below -50.0 dBFS", advice: /Turn the room up, move closer/ },
  { name: "too_noisy", words: "the response stands 36.0 dB above the noise, less than 40.0 dB", advice: /Pause music elsewhere/ },
]; // prettier-ignore

for (const { name, words, advice } of REFUSALS) {
  test(`a recording refused ${name} says why in the fitter's words, with its own advice, and applies nothing`, async () => {
    const browser = install(fakeBrowser());
    const house = fakeHouse({ enabled: false, filters: EARLIER });
    house.fit = refusalOfFit(name, words);
    const opened = await open(house);
    const { app } = opened;
    await measure(opened, house, browser);
    const alert = part(app, "[data-phase=refused]");
    assert.equal(alert.getAttribute("data-refusal"), name);
    assert.equal(alert.getAttribute("role"), "alert");
    assert.equal(text(alert), `The server refused the recording: ${name}: ${words}`);
    assert.equal(text(part(app, "[data-advice]")), ADVICE[name]);
    assert.match(ADVICE[name], advice);
    assert.match(ADVICE[name], /measure again\.$/);
    // Nothing was applied, and nothing can be: there is no apply to press.
    assert.deepEqual(house.server.commands, [measureSweepCommand("living")]);
    assert.deepEqual(house.roomEq, { enabled: false, filters: EARLIER });
    assert.equal(queryAllByLabel(app, `Apply the proposed correction to ${NAME}`).length, 0);
    assert.deepEqual(browser.tracks.map((track) => track.stopped), [true]);
    // And the way to try again is there.
    await press(app, `Use the microphone to measure ${NAME}`);
    assert.equal(phase(app), "ready");
  });
}

test("the four refusals have four different pieces of advice, and another refusal is shown in the server's words", async () => {
  assert.deepEqual(Object.keys(ADVICE), ["too_short", "clipped", "too_quiet", "too_noisy"]);
  assert.equal(new Set(Object.values(ADVICE)).size, 4);
  const browser = install(fakeBrowser());
  const house = fakeHouse();
  house.fit = { status: 503, body: JSON.stringify({ v: 2, t: "error", field: "", detail: "busy: another recording is being fitted" }) };
  const opened = await open(house);
  await measure(opened, house, browser);
  assert.equal(text(part(opened.app, "[data-phase=refused]")), "The server refused the recording: busy: another recording is being fitted");
  assert.equal(text(part(opened.app, "[data-advice]")), ADVICE_OTHER);
});

test("a room whose correction is on is measured with it off, and gets it back on before anything is proposed", async () => {
  const browser = install(fakeBrowser());
  const house = fakeHouse({ enabled: true, filters: EARLIER });
  const during = [];
  const answer = house.server.answer;
  house.server.answer = (body) => {
    const result = answer(body);
    if (JSON.parse(body).t === "measure_sweep") during.push(house.roomEq.enabled);
    return result;
  };
  const fetch = house.fetch;
  house.fetch = (url, options) => {
    if (String(url).includes("api/room-fit")) during.push(house.roomEq.enabled);
    return fetch(url, options);
  };
  const opened = await open(house);
  const { app } = opened;
  await measure(opened, house, browser);
  assert.deepEqual(during, [false, false], "off for the sweep and for the upload");
  assert.deepEqual(house.server.commands, [
    roomEqEnabledCommand("living", false),
    measureSweepCommand("living"),
    roomEqEnabledCommand("living", true),
  ]);
  assert.deepEqual(house.roomEq, { enabled: true, filters: EARLIER });
  assert.equal(phase(app), "proposed");
  // Applied over the earlier correction, undo returns to it, switched on.
  await press(app, `Apply the proposed correction to ${NAME}`);
  assert.deepEqual(house.roomEq, { enabled: true, filters: JSON.parse(`[${FILTERS.map(filterLiteral).join(",")}]`) });
  await press(app, `Undo the last correction of ${NAME}`);
  assert.equal(house.server.commands.at(-1), roomEqUndoCommand("living"));
  assert.deepEqual(house.roomEq, { enabled: true, filters: EARLIER });
  assert.deepEqual([...part(app, "[data-held]").querySelectorAll("li")].map(text), EARLIER.map(filterWords));
  assert.equal(getByLabel(app, `Undo the last correction of ${NAME}`).disabled, true);
  assert.equal(text(part(app, '[data-value="undo"]')), "Nothing to undo");
});

test("a room left while its correction is off for the sweep gets it back on", async () => {
  install(fakeBrowser());
  const house = fakeHouse({ enabled: true, filters: EARLIER });
  const opened = await open(house);
  await press(opened.app, `Use the microphone to measure ${NAME}`);
  await press(opened.app, `Play the sweep in ${NAME} and record`);
  assert.equal(house.roomEq.enabled, false);
  await press(opened.app, "Back to rooms");
  await settle();
  assert.equal(house.roomEq.enabled, true);
  assert.equal(house.server.commands.at(-1), roomEqEnabledCommand("living", true));
  assert.deepEqual(house.uploads, []);
});

test("the switch and undo send their one command and show what the server answers", async () => {
  install(fakeBrowser());
  const house = fakeHouse({ enabled: true, filters: EARLIER });
  const { app } = await open(house);
  const toggle = getByLabel(app, `Correction for ${NAME}`);
  assert.equal(toggle.getAttribute("aria-pressed"), "true");
  await press(app, `Correction for ${NAME}`);
  assert.deepEqual(house.server.commands, ['{"v":2,"t":"room_eq","zone":"living","enabled":false}']);
  assert.equal(toggle.getAttribute("aria-pressed"), "false");
  assert.equal(text(part(app, '[data-value="enabled"]')), "Off");
  await press(app, `Correction for ${NAME}`);
  assert.equal(house.server.commands.at(-1), '{"v":2,"t":"room_eq","zone":"living","enabled":true}');
  assert.equal(toggle.getAttribute("aria-pressed"), "true");
  // Nothing to undo: the button is off; a refusal from the server is shown in its words.
  assert.equal(getByLabel(app, `Undo the last correction of ${NAME}`).disabled, true);
  screenOf(app)._onUndo();
  await rendered(app);
  assert.equal(text(part(app, "[data-command-refusal]")), "Refused: nothing-to-undo: room 'living' has no earlier correction");
  // A room with no filters has nothing to switch.
  const bare = await open(fakeHouse());
  assert.equal(getByLabel(bare.app, `Correction for ${NAME}`).disabled, true);
  assert.equal(text(part(bare.app, '[data-value="enabled"]')), "Nothing to switch");
});

test("a room's card links to its correction screen, and a room the server does not have is said", async () => {
  install(fakeBrowser());
  const house = fakeHouse();
  history.replaceState(null, "", "/app/");
  const store = createStore(createClient({ fetch: house.fetch, base: house.server.base, timers: fakeTimers() }));
  stores.push(store);
  const app = document.createElement("chorus-app");
  app.store = store;
  document.body.append(app);
  store.start();
  await settle();
  await app.updateComplete;
  await settle();
  const link = getByLabel(app, `Correction for ${NAME}`);
  assert.equal(link.getAttribute("href"), "#/rooms/living/correction");
  link.click();
  await rendered(app);
  assert.equal(getByLabel(app, `Correction of ${NAME}`).localName, "main");
  app.remove();
  const { app: other } = await open(house, "attic");
  assert.equal(text(part(other, "[data-missing]")), 'This server has no room "attic".');
});

// ---- where the recording goes ----

test("the recording leaves the page only in the upload to the same origin, and is not kept after the flow", async () => {
  const browser = install(fakeBrowser());
  const house = fakeHouse();
  const opened = await open(house);
  const { app } = opened;
  const samples = new Float32Array([0, 0.5, -0.5, 1, -1, 0.25]);
  await measure(opened, house, browser, samples);
  assert.equal(phase(app), "proposed");

  // One upload, to this server's one route, on the page's own origin.
  assert.equal(house.uploads.length, 1);
  const { url, options } = house.uploads[0];
  assert.equal(url, "http://chorus.test/api/room-fit?zone=living");
  assert.equal(new URL(url).origin, location.origin);
  assert.equal(options.method, "POST");
  assert.deepEqual(options.headers, { "Content-Type": "audio/wav" });
  assert.equal(options.redirect, "manual");
  // Its body is the samples, as the 16-bit mono 48 kHz WAV the route takes.
  assert.deepEqual([...options.body], [...wavOf(samples)]);
  const view = new DataView(options.body.buffer);
  assert.deepEqual([0, 1, 2, 3, 4, 5].map((i) => view.getInt16(44 + 2 * i, true)), [0, 16384, -16384, 32767, -32768, 8192]);

  // No other request of the page carried a sample: every other body is a
  // command's text, to the same origin.
  const others = house.bodies.filter((sent) => sent.body !== options.body);
  assert.deepEqual(others.map((sent) => [sent.url, typeof sent.body]), [["http://chorus.test/api/command", "string"]]);
  for (const sent of house.bodies) assert.equal(new URL(sent.url).origin, location.origin);
  assert.ok(!house.server.commands.join("").includes("16384"));

  // No storage was written, by the flow or the apply after it.
  await press(app, `Apply the proposed correction to ${NAME}`);
  assert.deepEqual(storageWrites, []);
  assert.equal(document.cookie, "");

  // And the page keeps no sample: nothing the screen holds is a buffer.
  const screen = screenOf(app);
  const held = [];
  const look = (value, path, depth) => {
    if (value === null || typeof value !== "object" || depth > 4) return;
    if (ArrayBuffer.isView(value) || value instanceof ArrayBuffer) held.push(path);
    else if (Array.isArray(value) || Object.getPrototypeOf(value) === Object.prototype) {
      for (const [key, inner] of Object.entries(value)) look(inner, `${path}.${key}`, depth + 1);
    }
  };
  for (const key of Object.getOwnPropertyNames(screen)) look(screen[key], key, 0);
  assert.deepEqual(held, []);
  assert.equal(screen._session, null);
});

test("the modules that hold a recording name no storage and no other way out of the page", () => {
  for (const name of ["capture.js", "capture-worklet.js", "room-correction.js"]) {
    const source = readFileSync(new URL(`../src/${name}`, import.meta.url), "utf8")
      .split("\n")
      .filter((line) => !line.trim().startsWith("//") && !line.trim().startsWith("*") && !line.trim().startsWith("/*"))
      .join("\n");
    for (const word of ["localStorage", "sessionStorage", "indexedDB", "caches", "cookie", "fetch(", "XMLHttpRequest", "WebSocket", "sendBeacon", "MediaRecorder", "createObjectURL", "download"]) {
      assert.ok(!source.includes(word), `${name} names ${word}`);
    }
  }
});

test("a recording over the route's bound is refused in the page and not sent", async () => {
  const sent = [];
  const client = createClient({ fetch: async (...args) => void sent.push(args), base: "http://chorus.test/" });
  const result = await client.roomFit("living", new Uint8Array(MAX_RECORDING_BYTES + 1));
  assert.equal(result.ok, false);
  assert.match(result.refusal, /over the 2097152 the server takes/);
  assert.deepEqual(sent, []);
});

test("a source that names its sweep has it sent in the query, and the answer's name is read off its words", async () => {
  const asked = [];
  const client = createClient({
    fetch: async (url) => {
      asked.push(String(url));
      const body = JSON.stringify({ v: 2, t: "error", field: "recording", detail: "too_quiet: the peak is low" });
      return { ok: false, status: 422, text: async () => body };
    },
    base: "http://chorus.test/",
  });
  const result = await client.roomFit("the den", new Uint8Array(44), { sweepMs: 1000, fadeInMs: 0 });
  assert.deepEqual(asked, ["http://chorus.test/api/room-fit?zone=the%20den&sweep_ms=1000&fade_in_ms=0"]);
  assert.deepEqual(result, { ok: false, refusal: "too_quiet: the peak is low", field: "recording", name: "too_quiet" });
  assert.equal(refusalName("there is no zone 'attic'"), "");
  assert.equal(refusalName("correction_on: switch it off"), "correction_on");
  assert.equal(refusalName("nothing-to-undo: none"), "nothing-to-undo");
});

// ---- the pieces ----

test("the commands are the catalog's bytes", () => {
  assert.equal(measureSweepCommand("kitchen"), '{"v":2,"t":"measure_sweep","zone":"kitchen"}');
  assert.equal(roomEqUndoCommand("living"), '{"v":2,"t":"room_eq_undo","zone":"living"}');
  assert.equal(roomEqEnabledCommand("living", false), '{"v":2,"t":"room_eq","zone":"living","enabled":false}');
  // docs/control-plane.md's own example of `room_eq`.
  assert.equal(
    roomEqCommand("living", [{ freq_hz: 42, gain_db: -6, q: 4.5 }, { freq_hz: 120, gain_db: -3.25, q: 2 }]), // prettier-ignore
    '{"v":2,"t":"room_eq","zone":"living","filters":[{"freq_hz":42,"gain_db":-6.00,"q":4.500},{"freq_hz":120,"gain_db":-3.25,"q":2.000}],"enabled":true}',
  );
  assert.equal(filterLiteral({ freq_hz: 152, gain_db: 1.37, q: 2.42 }), '{"freq_hz":152,"gain_db":1.37,"q":2.420}');
});

test("a room's correction and the server's measurement are read from the state, and nothing is made up", () => {
  assert.deepEqual(correctionOf({}), { enabled: null, filters: [], undo: false });
  assert.deepEqual(correctionOf({ room_eq: { enabled: false, filters: [...EARLIER, { freq_hz: "x" }], undo: true } }), { enabled: false, filters: EARLIER, undo: true }); // prettier-ignore
  assert.equal(measurementOf({}), null);
  assert.deepEqual(
    measurementOf({ measurement: { id: 2, zone: "den", state: "cancelled", volume: 0.3, lead_ms: 500, sweep_ms: 5000, tail_ms: 1000, reason: "an alarm" } }),
    { id: 2, zone: "den", state: "cancelled", leadMs: 500, sweepMs: 5000, tailMs: 1000, reason: "an alarm" },
  );
});

test("the WAV is a 44-byte header for 16-bit mono at 48 kHz and the samples, held to the 16-bit range", () => {
  const bytes = wavOf(new Float32Array([0, 0.5, 2, -2, Number.NaN]));
  const view = new DataView(bytes.buffer);
  const tag = (at) => String.fromCharCode(...bytes.subarray(at, at + 4));
  assert.deepEqual([tag(0), tag(8), tag(12), tag(36)], ["RIFF", "WAVE", "fmt ", "data"]);
  assert.equal(view.getUint32(4, true), 36 + 10);
  assert.deepEqual([view.getUint32(16, true), view.getUint16(20, true), view.getUint16(22, true)], [16, 1, 1]);
  assert.deepEqual([view.getUint32(24, true), view.getUint32(28, true), view.getUint16(32, true), view.getUint16(34, true)], [48000, 96000, 2, 16]); // prettier-ignore
  assert.equal(view.getUint32(40, true), 10);
  assert.equal(bytes.length, 54);
  assert.deepEqual([0, 1, 2, 3, 4].map((i) => view.getInt16(44 + 2 * i, true)), [0, 16384, 32767, -32768, 0]);
});

test("a 16-bit recording through the page's samples comes back bit for bit", () => {
  // What the live test relies on: a fixture's samples, as floats, are the
  // fixture's samples again in the WAV that is uploaded.
  const ints = Int16Array.from({ length: 65536 }, (_, i) => i - 32768);
  const floats = Float32Array.from(ints, (value) => value / 32768);
  const view = new DataView(wavOf(floats).buffer);
  for (let i = 0; i < ints.length; i += 1) assert.equal(view.getInt16(44 + 2 * i, true), ints[i]);
});

test("the resampler brings a tone at 44.1 kHz to 48 kHz within a thousandth of full scale", () => {
  for (const [fromHz, toneHz] of [[44100, 100], [44100, 1000], [44100, 10000], [96000, 1000], [16000, 300]]) { // prettier-ignore
    const input = Float32Array.from({ length: fromHz / 10 }, (_, n) => 0.5 * Math.sin((2 * Math.PI * toneHz * n) / fromHz));
    const output = resample(input, fromHz, 48000);
    assert.equal(output.length, 4800);
    let worst = 0;
    // Away from the two ends, where the window runs off the recording.
    for (let n = 400; n < 4400; n += 1) {
      worst = Math.max(worst, Math.abs(output[n] - 0.5 * Math.sin((2 * Math.PI * toneHz * n) / 48000)));
    }
    assert.ok(worst < 1e-3, `${toneHz} Hz from ${fromHz} Hz: off by ${worst}`);
  }
  // Going down in rate, what the lower rate cannot carry is taken out.
  const high = Float32Array.from({ length: 9600 }, (_, n) => 0.5 * Math.sin((2 * Math.PI * 30000 * n) / 96000));
  const down = resample(high, 96000, 48000);
  assert.ok(Math.max(...down.subarray(400, 4400).map(Math.abs)) < 1e-3);
  const same = new Float32Array([1, 2, 3]);
  assert.equal(resample(same, 48000, 48000), same);
});

test("the worklet's processor hands over the first channel in batches, and what it holds when asked", async () => {
  const sent = [];
  let registered;
  globalThis.AudioWorkletProcessor = class {
    constructor() {
      this.port = { onmessage: null, postMessage: (message, transfer) => sent.push({ message, transfer }) };
    }
  };
  globalThis.registerProcessor = (name, processor) => (registered = { name, processor });
  try {
    const { BATCH, CAPTURE_PROCESSOR: name } = await import("../src/capture-worklet.js");
    assert.equal(registered.name, name);
    assert.equal(name, CAPTURE_PROCESSOR);
    const processor = new registered.processor();
    // Render quanta of 128 frames, two channels: the first is recorded.
    const quanta = Math.floor(BATCH / 128) + 3;
    for (let q = 0; q < quanta; q += 1) {
      const first = Float32Array.from({ length: 128 }, (_, i) => q * 128 + i);
      assert.equal(processor.process([[first, new Float32Array(128).fill(-1)]]), true);
    }
    assert.equal(sent.length, 1);
    assert.equal(sent[0].message.samples.length, BATCH);
    assert.deepEqual([sent[0].message.samples[0], sent[0].message.samples[BATCH - 1]], [0, BATCH - 1]);
    assert.deepEqual(sent[0].transfer, [sent[0].message.samples.buffer]);
    processor.port.onmessage({ data: "flush" });
    assert.equal(sent[1].message.samples.length, quanta * 128 - BATCH);
    assert.equal(sent[1].message.samples[0], BATCH);
    assert.deepEqual(sent[2].message, { flushed: true });
    // An input with nothing connected is not an error, and "stop" ends it.
    assert.equal(processor.process([[]]), true);
    processor.port.onmessage({ data: "stop" });
    assert.equal(processor.process([[new Float32Array(128)]]), false);
  } finally {
    delete globalThis.AudioWorkletProcessor;
    delete globalThis.registerProcessor;
  }
});
