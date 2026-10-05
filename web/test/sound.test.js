// A room's sound screen (src/sound.js) in the shell, over a scripted server:
// each control, found by its label, sends exactly one `sound` command that
// carries the one field it changes, and shows the value the server answers
// with, never one of its own; a refusal is shown with the field the server
// named and its words.

import assert from "node:assert/strict";
import { afterEach, test } from "node:test";

import { createClient } from "../src/api.js";
import "../src/chorus-app.js";
import { addressOf } from "../src/routes.js";
import { SOUND_SCREEN, decibels } from "../src/sound.js";
import { createStore } from "../src/state.js";
import { fakeServer, fakeTimers, settle, stateOf, zone } from "./fake-server.js";
import { getByLabel, queryAllByLabel } from "./label-query.js";

let stores = [];

afterEach(() => {
  for (const store of stores) store.stop();
  stores = [];
  document.body.replaceChildren();
  history.replaceState(null, "", "/");
});

const SOUND = { bass: 0, treble: 0, loudness: true, night: false, speech: false };
const NAME = "Living Room";

// The house: the room whose screen is open, and another that must not change.
const house = (serial, sound = SOUND, more = {}) =>
  stateOf(serial, [zone("living", { name: NAME, sound, ...more }), zone("den", { sound: SOUND })]);

async function rendered(app) {
  await settle();
  await app.updateComplete;
  await app.shadowRoot.querySelector("chorus-room-sound")?.updateComplete;
}

// The app, opened at the living room's sound screen, over `server`.
async function open(server, room = "living", fetch = server.fetch) {
  history.replaceState(null, "", `/app/${addressOf(SOUND_SCREEN, { room })}`);
  const store = createStore(createClient({ fetch, base: server.base, timers: fakeTimers() }));
  stores.push(store);
  const app = document.createElement("chorus-app");
  app.store = store;
  document.body.append(app);
  store.start();
  await rendered(app);
  return app;
}

const screenOf = (app) => app.shadowRoot.querySelector("chorus-room-sound");
const text = (node) => node.textContent.replace(/\s+/g, " ").trim();
const figure = (app, field) => text(screenOf(app).shadowRoot.querySelector(`[data-value="${field}"]`));
const alert = (app) => screenOf(app).shadowRoot.querySelector("[role=alert]");

// What the screen shows of the five settings, read off its controls.
const shown = (app) => ({
  bass: getByLabel(app, `Bass for ${NAME}`).value,
  treble: getByLabel(app, `Treble for ${NAME}`).value,
  loudness: getByLabel(app, `Loudness for ${NAME}`).getAttribute("aria-pressed"),
  night: getByLabel(app, `Night mode for ${NAME}`).getAttribute("aria-pressed"),
  speech: getByLabel(app, `Speech enhancement for ${NAME}`).getAttribute("aria-pressed"),
});

// A person moves a slider to `value` and lets go.
function slide(slider, value) {
  slider.focus();
  slider.value = String(value);
  slider.dispatchEvent(new Event("input", { bubbles: true }));
  slider.dispatchEvent(new Event("change", { bubbles: true }));
}

test("the screen shows the room's five settings as the server holds them, each control with its label", async () => {
  const app = await open(fakeServer(house(1, { bass: 3, treble: -2, loudness: false, night: true, speech: true })));
  assert.equal(getByLabel(app, `Sound of ${NAME}`).localName, "main");
  assert.equal(text(screenOf(app).shadowRoot.querySelector("h2")), `Sound of ${NAME}`);
  assert.deepEqual(shown(app), { bass: "3", treble: "-2", loudness: "false", night: "true", speech: "true" });
  assert.deepEqual(
    ["bass", "treble", "loudness", "night", "speech"].map((field) => figure(app, field)),
    ["+3 dB", "-2 dB", "Off", "On", "On"],
  );
  // The sliders are the catalog's range, in whole dB.
  for (const name of ["Bass", "Treble"]) {
    const slider = getByLabel(app, `${name} for ${NAME}`);
    assert.deepEqual([slider.type, slider.min, slider.max, slider.step], ["range", "-10", "10", "1"]);
  }
  assert.equal(getByLabel(app, `Bass for ${NAME}`).getAttribute("aria-valuetext"), "+3 dB");
  assert.deepEqual([decibels(0), decibels(10), decibels(-10)], ["0 dB", "+10 dB", "-10 dB"]);
  assert.equal(text(alert(app)), "");
});

// Each control: what a person does to it, the one command that is sent, and
// the state the scripted server answers with. The answer is never what was
// asked: the server says something else, and the screen must show the answer.
const CONTROLS = [
  {
    label: `Bass for ${NAME}`,
    act: (control) => slide(control, 6),
    command: '{"v":2,"t":"sound","zone":"living","bass":6}',
    answer: { ...SOUND, bass: 4 },
    then: { bass: "4", treble: "0", loudness: "true", night: "false", speech: "false" },
    figure: ["bass", "+4 dB"],
  },
  {
    label: `Treble for ${NAME}`,
    act: (control) => slide(control, -7),
    command: '{"v":2,"t":"sound","zone":"living","treble":-7}',
    answer: { ...SOUND, treble: -5 },
    then: { bass: "0", treble: "-5", loudness: "true", night: "false", speech: "false" },
    figure: ["treble", "-5 dB"],
  },
  {
    label: `Loudness for ${NAME}`,
    act: (control) => control.click(),
    command: '{"v":2,"t":"sound","zone":"living","loudness":false}',
    // The server turns loudness off and, as another client just asked, night mode on.
    answer: { ...SOUND, loudness: false, night: true },
    then: { bass: "0", treble: "0", loudness: "false", night: "true", speech: "false" },
    figure: ["loudness", "Off"],
  },
  {
    label: `Night mode for ${NAME}`,
    act: (control) => control.click(),
    command: '{"v":2,"t":"sound","zone":"living","night":true}',
    answer: { ...SOUND, night: true, bass: -1 },
    then: { bass: "-1", treble: "0", loudness: "true", night: "true", speech: "false" },
    figure: ["night", "On"],
  },
  {
    label: `Speech enhancement for ${NAME}`,
    act: (control) => control.click(),
    command: '{"v":2,"t":"sound","zone":"living","speech":true}',
    answer: { ...SOUND, speech: true, treble: 2 },
    then: { bass: "0", treble: "2", loudness: "true", night: "false", speech: "true" },
    figure: ["speech", "On"],
  },
];

for (const control of CONTROLS) {
  test(`"${control.label}" sends one sound command with only its field, and shows what the server answers`, async () => {
    const server = fakeServer(house(1));
    server.answer = () => ({ status: 200, body: JSON.stringify(house(2, control.answer)) });
    // The server holds its answer back: until it comes, nothing has changed.
    let release;
    const held = new Promise((resolve) => (release = resolve));
    const app = await open(server, "living", async (url, options) => {
      if (String(url).endsWith("api/command")) await held;
      return server.fetch(url, options);
    });
    const before = shown(app);

    const element = getByLabel(app, control.label);
    control.act(element);
    await rendered(app);
    assert.deepEqual(server.commands, [], "the answer has not come");
    element.blur();
    await rendered(app);
    assert.deepEqual(shown(app), before, "no optimistic value: the screen is as the server still holds it");

    release();
    await rendered(app);
    assert.deepEqual(server.commands, [control.command], "exactly one command, with only the changed field");
    assert.deepEqual(shown(app), control.then, "the screen shows the server's answer, not what was asked");
    assert.equal(figure(app, control.figure[0]), control.figure[1]);
    assert.equal(getByLabel(app, control.label), element, "the control is the element it was");
    assert.equal(text(alert(app)), "");
  });
}

test("a switch that is on asks for off, by the server's value and not the last click", async () => {
  const server = fakeServer(house(1, { ...SOUND, loudness: true }));
  const app = await open(server);
  // The server refuses to change anything: its answer is the state as it was.
  server.answer = () => ({ status: 200, body: JSON.stringify(house(2)) });
  getByLabel(app, `Loudness for ${NAME}`).click();
  await rendered(app);
  getByLabel(app, `Loudness for ${NAME}`).click();
  await rendered(app);
  const off = '{"v":2,"t":"sound","zone":"living","loudness":false}';
  assert.deepEqual(server.commands, [off, off], "both clicks ask for the opposite of what the server holds");
  assert.equal(shown(app).loudness, "true");
});

test("a refusal is shown with the field the server named and its detail, and the control returns to the server's value", async () => {
  const server = fakeServer(house(1, { ...SOUND, bass: 2 }));
  const app = await open(server);
  const detail = "the field 'bass' is 11 and the catalog declares a whole number from -10 to 10";
  server.answer = () => ({ status: 400, body: JSON.stringify({ v: 2, t: "error", field: "bass", detail }) });

  const bass = getByLabel(app, `Bass for ${NAME}`);
  slide(bass, 9);
  await rendered(app);
  assert.deepEqual(server.commands, ['{"v":2,"t":"sound","zone":"living","bass":9}']);
  assert.equal(text(alert(app)), `Refused (bass): ${detail}`);
  assert.equal(alert(app).getAttribute("data-refusal-field"), "bass");
  // Still held by the person, and still put back: the command did not happen.
  assert.equal(bass.value, "2");
  assert.equal(figure(app, "bass"), "+2 dB");

  // The refusal is this screen's: the room's card does not carry it.
  getByLabel(app, "Back to rooms").click();
  await settle();
  await app.updateComplete;
  const rooms = app.shadowRoot.querySelector("chorus-rooms");
  await rooms.updateComplete;
  const cards = [...rooms.shadowRoot.querySelectorAll("chorus-room-card")];
  await Promise.all(cards.map((card) => card.updateComplete));
  assert.deepEqual(cards.map((card) => text(card.shadowRoot.querySelector("[role=alert]"))), ["", ""]);
});

test("a refusal that names no field shows the server's words alone, and the next command clears it", async () => {
  const server = fakeServer(house(1));
  const app = await open(server);
  server.answer = () => ({ status: 503, body: "the server is shutting down\n" });
  getByLabel(app, `Night mode for ${NAME}`).click();
  await rendered(app);
  assert.equal(text(alert(app)), "Refused: the server is shutting down");
  assert.equal(alert(app).hasAttribute("data-refusal-field"), false);
  assert.equal(shown(app).night, "false");

  server.answer = () => ({ status: 200, body: JSON.stringify(house(2, { ...SOUND, night: true })) });
  getByLabel(app, `Night mode for ${NAME}`).click();
  await rendered(app);
  assert.equal(text(alert(app)), "");
  assert.equal(shown(app).night, "true");
});

test("a change another client makes appears on the screen, except under a slider a person has hold of", async () => {
  const server = fakeServer(house(1));
  const app = await open(server);
  const bass = getByLabel(app, `Bass for ${NAME}`);
  const treble = getByLabel(app, `Treble for ${NAME}`);

  server.send(house(2, { bass: 5, treble: -4, loudness: false, night: true, speech: true }));
  await rendered(app);
  assert.deepEqual(shown(app), { bass: "5", treble: "-4", loudness: "false", night: "true", speech: "true" });
  assert.equal(getByLabel(app, `Bass for ${NAME}`), bass, "the screen was patched, not painted again");

  // A person is dragging the bass: the figure follows the finger, and an
  // update moves the treble and leaves the bass slider where the finger is.
  bass.focus();
  bass.value = "-3";
  bass.dispatchEvent(new Event("input", { bubbles: true }));
  await rendered(app);
  assert.equal(figure(app, "bass"), "-3 dB");
  server.send(house(3, { bass: 8, treble: 1, loudness: false, night: true, speech: true }));
  await rendered(app);
  assert.equal(bass.value, "-3");
  assert.equal(treble.value, "1");
  // Let go without changing it: the slider takes the server's value.
  bass.blur();
  await rendered(app);
  assert.equal(bass.value, "8");
  assert.equal(figure(app, "bass"), "+8 dB");
  assert.deepEqual(server.commands, [], "following the server sends nothing");
});

test("a setting the state does not carry is said to be unavailable, not shown as a default", async () => {
  const server = fakeServer(stateOf(1, [zone("living", { name: NAME, sound: { treble: 1, night: true } })]));
  const app = await open(server);
  assert.deepEqual(queryAllByLabel(app, `Bass for ${NAME}`), []);
  assert.equal(figure(app, "bass"), "Unavailable");
  assert.equal(getByLabel(app, `Treble for ${NAME}`).value, "1");
  assert.equal(getByLabel(app, `Loudness for ${NAME}`).disabled, true);
  assert.equal(figure(app, "loudness"), "Unavailable");
  assert.equal(getByLabel(app, `Night mode for ${NAME}`).disabled, false);
});

test("before the first state the screen says it is reading, and a room the server does not have is said so", async () => {
  const server = fakeServer(null);
  const app = await open(server, "attic");
  const words = () => text(screenOf(app).shadowRoot.querySelector("[data-missing]"));
  assert.equal(words(), "Reading this server's rooms.");
  assert.equal(getByLabel(app, "Sound of attic").localName, "main");
  server.send(house(1));
  await rendered(app);
  assert.equal(words(), 'This server has no room "attic".');
  assert.equal(getByLabel(app, "Back to rooms").localName, "a");
  // The room appears (another client's doing): the screen is then its sound.
  server.send(stateOf(2, [zone("attic", { sound: { ...SOUND, bass: 1 } })]));
  await rendered(app);
  assert.equal(getByLabel(app, "Bass for attic").value, "1");
});

test("its styles name tokens, never a literal colour or length", () => {
  const sheet = customElements.get("chorus-room-sound").styles.cssText;
  assert.match(sheet, /min-height: var\(--control-size\)/);
  assert.doesNotMatch(sheet, /#[0-9a-f]{3,8}\b|\b\d+(px|rem|em)\b/i);
});
