// The live test of a room's sound screen (`make web-live`): the app's own
// elements, navigation and state layer, in node under happy-dom with no
// browser, against a real chorus-server.
//
// The screen is opened the way a person opens it, from the room's card. Every
// control is then changed through the screen and read back equal from the
// server's own `GET /api/state`, and a change a second client makes appears
// on the screen with no reload.
//
// CHORUS_SERVER_BIN names the built chorus-server; tools/web.sh refuses to
// run this file without it.

import assert from "node:assert/strict";
import { after, before, test } from "node:test";

import { createClient } from "../src/api.js";
import "../src/chorus-app.js";
import { createStore } from "../src/state.js";
import { getByLabel, queryAllByLabel } from "../test/label-query.js";
import { startHouse, until } from "./house.js";

// The house: two rooms, one with a name its id does not hold, so a label
// asserted below can only carry the server's name.
const LIVING = { id: "living", name: "Live Test Living Room" };
const DEN = { id: "den" };

// What a server that has been told nothing holds (docs/control-plane.md,
// "Per-room sound").
const DEFAULTS = { bass: 0, treble: 0, loudness: true, night: false, speech: false };

let house;
let store;
let app;

const otherClient = (message) => house.command(message);

// A room's five settings as the server's own state says them.
const serverSound = async (id) => {
  const { bass, treble, loudness, night, speech } = (await house.state()).zones.find((zone) => zone.id === id).sound;
  return { bass, treble, loudness, night, speech };
};

// The same five as the screen shows them, read off its controls, in the
// server's terms: a slider's value as a number, a switch as a boolean.
const control = (name) => queryAllByLabel(app, `${name} for ${LIVING.name}`)[0] ?? null;
const pressed = (button) => button.getAttribute("aria-pressed") === "true";
const shownSound = () => {
  const [bass, treble] = [control("Bass"), control("Treble")];
  const [loudness, night, speech] = [control("Loudness"), control("Night mode"), control("Speech enhancement")];
  if (!bass || !treble || !loudness || !night || !speech) return null;
  return {
    bass: Number(bass.value),
    treble: Number(treble.value),
    loudness: pressed(loudness),
    night: pressed(night),
    speech: pressed(speech),
  };
};

// A person moves a slider to `value` and lets go of it.
function slide(slider, value) {
  slider.focus();
  slider.value = String(value);
  slider.dispatchEvent(new Event("input", { bubbles: true }));
  slider.dispatchEvent(new Event("change", { bubbles: true }));
  slider.blur();
}

before(async () => {
  assert.ok(process.env.CHORUS_SERVER_BIN, "CHORUS_SERVER_BIN names a built chorus-server");
  house = await startHouse([LIVING.id, DEN.id]);
  await otherClient(JSON.stringify({ v: 1, t: "name", zone: LIVING.id, name: LIVING.name }));

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
  await house?.stop();
});

test("the sound screen opens from the room's card and shows the sound a real server holds", async () => {
  await until("the link on the living room's card", () => queryAllByLabel(app, `Sound for ${LIVING.name}`).length, 1);
  assert.deepEqual(await serverSound(LIVING.id), DEFAULTS);
  const entries = history.length;
  getByLabel(app, `Sound for ${LIVING.name}`).click();
  await until("the sound screen", () => shownSound(), DEFAULTS);
  assert.equal(getByLabel(app, `Sound of ${LIVING.name}`).localName, "main");
  assert.equal(location.hash, "#/rooms/living/sound", "the screen has an address of its own");
  assert.equal(history.length, entries + 1, "and an entry of the history");
  // The event stream is delivering, not only the snapshot.
  await until("the store's status", () => store.view().status, "live");
});

// Every control, changed through the screen, one after another: each is read
// back equal from the server's /api/state with the settings before it as they
// were left, and the screen then shows the same.
const CHANGES = [
  { name: "Bass", field: "bass", to: 4, act: (element) => slide(element, 4) },
  { name: "Treble", field: "treble", to: -3, act: (element) => slide(element, -3) },
  { name: "Loudness", field: "loudness", to: false, act: (element) => element.click() },
  { name: "Night mode", field: "night", to: true, act: (element) => element.click() },
  { name: "Speech enhancement", field: "speech", to: true, act: (element) => element.click() },
];

test("every control changed through the screen is read back equal from the server's /api/state", async () => {
  const expected = { ...DEFAULTS };
  for (const change of CHANGES) {
    const element = control(change.name);
    change.act(element);
    expected[change.field] = change.to;
    await until(`the server's sound after "${change.name}"`, () => serverSound(LIVING.id), { ...expected });
    await until(`the screen after "${change.name}"`, () => shownSound(), { ...expected });
    assert.equal(control(change.name), element, "the control is the element it was");
  }
  assert.deepEqual(expected, { bass: 4, treble: -3, loudness: false, night: true, speech: true });
  assert.deepEqual(await serverSound(DEN.id), DEFAULTS, "the other room was not touched");

  // And back again, so a switch is proved in both directions and the sliders
  // at both ends of the catalog's range.
  slide(control("Bass"), -10);
  slide(control("Treble"), 10);
  await until("the server's tone at the ends of its range", async () => {
    const { bass, treble } = await serverSound(LIVING.id);
    return { bass, treble };
  }, { bass: -10, treble: 10 }); // prettier-ignore
  for (const name of ["Loudness", "Night mode", "Speech enhancement"]) {
    await until(`the screen before "${name}" is switched back`, () => shownSound()?.bass, -10);
    const was = pressed(control(name));
    control(name).click();
    await until(`"${name}" switched back on the screen`, () => pressed(control(name)), !was);
  }
  const back = { bass: -10, treble: 10, loudness: true, night: false, speech: false };
  await until("the server's sound", () => serverSound(LIVING.id), back);
  await until("the screen", () => shownSound(), back);
  const alert = app.shadowRoot.querySelector("chorus-room-sound").shadowRoot.querySelector("[role=alert]");
  assert.equal(alert.textContent.trim(), "", "nothing was refused");
});

test("a change a second client makes appears on the screen with no reload", async () => {
  const screen = app.shadowRoot.querySelector("chorus-room-sound");
  const bass = control("Bass");
  const night = control("Night mode");
  await otherClient('{"v":2,"t":"sound","zone":"living","bass":7,"treble":-6,"loudness":false,"night":true,"speech":true}');
  const theirs = { bass: 7, treble: -6, loudness: false, night: true, speech: true };
  await until("the screen", () => shownSound(), theirs);
  assert.deepEqual(await serverSound(LIVING.id), theirs);
  // A partial update from outside moves its one field and leaves the rest.
  await otherClient('{"v":2,"t":"sound","zone":"living","night":false}');
  await until("the screen", () => shownSound(), { ...theirs, night: false });
  // The same screen and the same controls: the page was patched, not loaded again.
  assert.equal(app.shadowRoot.querySelector("chorus-room-sound"), screen);
  assert.equal(control("Bass"), bass);
  assert.equal(control("Night mode"), night);
  // A rename from outside reaches the screen's labels too.
  await otherClient(JSON.stringify({ v: 1, t: "name", zone: LIVING.id, name: "Renamed Elsewhere" }));
  await until("the bass slider's label", () => queryAllByLabel(app, "Bass for Renamed Elsewhere")[0] === bass, true);
  assert.equal(getByLabel(app, "Sound of Renamed Elsewhere").localName, "main");
});

test("a command the real server refuses is shown with its field and its words, and Back returns to the rooms", async () => {
  // The screen's sliders cannot ask for more than the catalog's range, so the
  // one command here that no control sends is a bass one step past it, sent
  // the way the screen sends its own (the `chorus-command` event the shell
  // listens for): what is held is that the field and the words shown are the
  // real server's.
  const screen = app.shadowRoot.querySelector("chorus-room-sound");
  screen.dispatchEvent(
    new CustomEvent("chorus-command", {
      detail: { subject: "sound:living", body: '{"v":2,"t":"sound","zone":"living","bass":11}' },
      bubbles: true,
      composed: true,
    }),
  );
  const alert = screen.shadowRoot.querySelector("[role=alert]");
  await until("the refusal's field", () => alert.getAttribute("data-refusal-field"), "bass");
  assert.match(alert.textContent.trim(), /^Refused \(bass\): .*-10 to 10/);
  assert.equal((await serverSound(LIVING.id)).bass, 7, "nothing of it was applied");

  getByLabel(app, "Back to rooms").click();
  await until("the rooms", () => queryAllByLabel(app, "Sound for Renamed Elsewhere").length, 1);
  assert.equal(getByLabel(app, "Rooms").localName, "main");
  assert.equal(location.hash, "");
});
