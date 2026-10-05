// The live test of a room's theater screen (`make web-live`): the app's own
// elements, navigation and state layer, in node under happy-dom with no
// browser, against a real chorus-server.
//
// The TV input is a real one: an endpoint's session offers an optical input
// (endpoint.js), so the server's state says its kind and no flag put it
// there, and the endpoint is put in a room the way a speaker is. The screen
// is opened the way a person opens it, from that room's card, which is the
// only card that has the link. The A/V trim, the TV input's autoplay rule
// with both its options, and the TV upmix are then set through the screen and
// read back equal from the server's own `GET /api/state`.
//
// Nothing here plays, and nothing here measures: what is held is that the
// numbers and words a person sets are the ones the server holds.
//
// CHORUS_SERVER_BIN names the built chorus-server; tools/web.sh refuses to
// run this file without it.

import assert from "node:assert/strict";
import { after, before, test } from "node:test";

import { createClient } from "../src/api.js";
import "../src/chorus-app.js";
import { createStore } from "../src/state.js";
import { getByLabel, queryAllByLabel } from "../test/label-query.js";
import { offerLineIn } from "./endpoint.js";
import { startHouse, until } from "./house.js";

// The house: the room the TV is in, with a name its id does not hold, and a
// room with no TV.
const LIVING = { id: "living", name: "Live Test Living Room" };
const DEN = { id: "den" };
// The hub the TV's optical out is wired to, and its input as the server lists it.
const HUB = "live-test-hub";
const INPUT = `${HUB}/tv`;

let house;
let hub;
let store;
let app;

const text = (node) => node.textContent.replace(/\s+/g, " ").trim();
const screen = () => app.shadowRoot.querySelector("chorus-room-theater");
const control = (label) => queryAllByLabel(app, label)[0] ?? null;
const pressed = (label) => control(label)?.getAttribute("aria-pressed") === "true";
const zoneOf = async (id) => (await house.state()).zones.find((zone) => zone.id === id);
const serverRules = async () => (await house.state()).autoplay;

// What the screen shows, in the server's terms: the trim in ms, the upmix
// word, and the TV input's rule as [target, on, stop on standby, low latency].
const shown = () => {
  const trim = control(`A/V trim for ${LIVING.name}`);
  const target = control(`Autoplay target for ${INPUT}`);
  if (!trim || !target) return null;
  return {
    av_trim_ms: Number(trim.value),
    tv_upmix: ["off", "ambient"].filter((word) => pressed(`TV upmix ${word} for ${LIVING.name}`)).join("+"),
    rule: [target.value, pressed(`Autoplay for ${INPUT}`), pressed(`Stop on standby for ${INPUT}`), pressed(`Low latency for ${INPUT}`)],
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
  // `--slots`: a server lists an endpoint's input only when it has slots to
  // play one in (as live/autoplay.live.js starts it).
  house = await startHouse([LIVING.id, DEN.id], { extra: ["--slots", "4"] });
  await house.command(JSON.stringify({ v: 1, t: "name", zone: LIVING.id, name: LIVING.name }));

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
  await hub?.stop();
  await house?.stop();
});

test("the theater screen is offered on the card of the room a real TV input is in, and on no other", async () => {
  await until("the living room's card", () => queryAllByLabel(app, `Sound for ${LIVING.name}`).length, 1);
  await until("the store's status", () => store.view().status, "live");
  assert.deepEqual(queryAllByLabel(app, `Theater for ${LIVING.name}`), [], "no TV input yet, and no theater set");

  // The hub's session offers the TV's optical input, and the hub is put in
  // the living room, as a speaker is.
  const [, port] = await house.said(/chorus-server: listening on=\S*?:(\d+)/);
  hub = await offerLineIn({ host: "127.0.0.1", port: Number(port), endpoint: HUB, name: "tv", kind: "optical" });
  await until("the input's kind, as the server says it", async () => (await house.state()).input_kinds, [
    { input: INPUT, kind: "optical", tv: true },
  ]);
  assert.deepEqual(queryAllByLabel(app, `Theater for ${LIVING.name}`), [], "the hub is in no room yet");
  await house.command(JSON.stringify({ v: 2, t: "speaker_room", speaker: HUB, room: LIVING.id }));
  await until("the link on the living room's card", () => queryAllByLabel(app, `Theater for ${LIVING.name}`).length, 1);
  assert.deepEqual(queryAllByLabel(app, `Theater for ${DEN.id}`), [], "the den has no TV input");

  const entries = history.length;
  getByLabel(app, `Theater for ${LIVING.name}`).click();
  // What a server that has been told nothing holds (docs/control-plane.md, "The TV path").
  await until("the theater screen", () => shown(), { av_trim_ms: 0, tv_upmix: "off", rule: ["", false, true, true] });
  assert.equal(getByLabel(app, `Theater of ${LIVING.name}`).localName, "main");
  assert.equal(location.hash, "#/rooms/living/theater", "the screen has an address of its own");
  assert.equal(history.length, entries + 1, "and an entry of the history");
  assert.equal((await zoneOf(LIVING.id)).av_trim_ms, 0);
  assert.deepEqual(await serverRules(), []);

  // The room has no bonded set, so no sub: the real server says bass
  // management is not in force, and the screen has no control for it.
  assert.equal((await zoneOf(LIVING.id)).bass_management.active, false);
  assert.deepEqual(queryAllByLabel(app, `Crossover for ${LIVING.name}`), []);
  assert.match(text(screen().shadowRoot.querySelector("[data-no-sub]")), /no sub/);
});

test("the A/V trim set through the screen is read back equal from the server's /api/state", async () => {
  const slider = control(`A/V trim for ${LIVING.name}`);
  slide(slider, -40);
  await until("the server's trim", async () => (await zoneOf(LIVING.id)).av_trim_ms, -40);
  await until("the screen's trim", () => shown().av_trim_ms, -40);

  control(`A/V trim 1 ms later for ${LIVING.name}`).click();
  await until("the server's trim, one later", async () => (await zoneOf(LIVING.id)).av_trim_ms, -39);
  await until("the screen's trim", () => shown().av_trim_ms, -39);

  // Both ends of the catalog's range, which are the slider's.
  for (const end of [Number(slider.min), Number(slider.max)]) {
    slide(slider, end);
    await until(`the server's trim at ${end}`, async () => (await zoneOf(LIVING.id)).av_trim_ms, end);
    await until("the screen's trim", () => shown().av_trim_ms, end);
  }
  assert.deepEqual([Number(slider.min), Number(slider.max)], [-100, 200]);
  assert.equal(control(`A/V trim 1 ms later for ${LIVING.name}`).disabled, true, "nothing later than the range");
  assert.equal((await zoneOf(DEN.id)).av_trim_ms, 0, "the other room was not touched");
  assert.equal(control(`A/V trim for ${LIVING.name}`), slider, "the control is the element it was");
});

test("a TV autoplay rule with both options set through the screen is read back equal from the server's /api/state", async () => {
  // The switch makes the rule: the TV plays in its own room. Both options are
  // at the catalog's defaults, which it writes by leaving them out.
  control(`Autoplay for ${INPUT}`).click();
  await until("the server's rule", () => serverRules(), [{ input: INPUT, target: LIVING.id, enabled: true }]);
  await until("the screen's rule", () => shown().rule, [LIVING.id, true, true, true]);

  control(`Stop on standby for ${INPUT}`).click();
  await until("the server's rule", () => serverRules(), [{ input: INPUT, target: LIVING.id, enabled: true, stop_on_standby: false }]);
  await until("the screen's rule", () => shown().rule, [LIVING.id, true, false, true]);

  control(`Low latency for ${INPUT}`).click();
  await until("the server's rule, both options off", () => serverRules(), [
    { input: INPUT, target: LIVING.id, enabled: true, stop_on_standby: false, low_latency: false },
  ]);
  await until("the screen's rule", () => shown().rule, [LIVING.id, true, false, false]);

  // The switch keeps both options, and each option goes back on by itself.
  control(`Autoplay for ${INPUT}`).click();
  await until("the server's rule, switched off", () => serverRules(), [
    { input: INPUT, target: LIVING.id, enabled: false, stop_on_standby: false, low_latency: false },
  ]);
  await until("the screen's rule", () => shown().rule, [LIVING.id, false, false, false]);
  control(`Stop on standby for ${INPUT}`).click();
  await until("the server's rule", () => serverRules(), [{ input: INPUT, target: LIVING.id, enabled: false, low_latency: false }]);
  await until("the screen's rule", () => shown().rule, [LIVING.id, false, true, false]);
  control(`Low latency for ${INPUT}`).click();
  await until("the server's rule, both options on", () => serverRules(), [{ input: INPUT, target: LIVING.id, enabled: false }]);
  await until("the screen's rule", () => shown().rule, [LIVING.id, false, true, true]);

  const alert = screen().shadowRoot.querySelector("chorus-autoplay").shadowRoot.querySelector("li [role=alert]");
  assert.equal(text(alert), "", "nothing was refused");
});

test("the TV upmix set through the screen is read back equal from the server's /api/state", async () => {
  const before = (await zoneOf(LIVING.id)).sound;
  assert.equal(before.tv_upmix, "off");
  control(`TV upmix ambient for ${LIVING.name}`).click();
  await until("the server's upmix", async () => (await zoneOf(LIVING.id)).sound, { ...before, tv_upmix: "ambient" });
  await until("the screen's upmix", () => shown().tv_upmix, "ambient");
  control(`TV upmix off for ${LIVING.name}`).click();
  await until("the server's upmix", async () => (await zoneOf(LIVING.id)).sound, before);
  await until("the screen's upmix", () => shown().tv_upmix, "off");
  assert.equal((await zoneOf(DEN.id)).sound.tv_upmix, "off");
});

test("a change a second client makes appears on the screen with no reload", async () => {
  const element = screen();
  const slider = control(`A/V trim for ${LIVING.name}`);
  await house.command('{"v":2,"t":"av_trim","zone":"living","av_trim_ms":25}');
  await house.command('{"v":2,"t":"sound","zone":"living","tv_upmix":"ambient"}');
  await house.command(JSON.stringify({ v: 2, t: "autoplay", input: INPUT, target: DEN.id, enabled: true, low_latency: false }));
  // The rule now plays the TV in the den: it is still the living room's
  // input (its hub is there), and the screen shows where it plays.
  await until("the screen", () => shown(), { av_trim_ms: 25, tv_upmix: "ambient", rule: [DEN.id, true, true, false] });
  assert.equal(screen(), element, "the page was patched, not loaded again");
  assert.equal(control(`A/V trim for ${LIVING.name}`), slider);
});

test("a command the real server refuses is shown with its field and its words, and Back returns to the rooms", async () => {
  // The screen's controls cannot ask for a trim outside the catalog's range,
  // so the one command here that no control sends is one step past it, sent
  // the way the screen sends its own (the `chorus-command` event the shell
  // listens for): what is held is that the field and the words shown are the
  // real server's.
  screen().dispatchEvent(
    new CustomEvent("chorus-command", {
      detail: { subject: "theater:living", body: '{"v":2,"t":"av_trim","zone":"living","av_trim_ms":201}' },
      bubbles: true,
      composed: true,
    }),
  );
  const alert = screen().shadowRoot.querySelector("[role=alert]");
  await until("the refusal's field", () => alert.getAttribute("data-refusal-field"), "av_trim_ms");
  assert.match(text(alert), /^Refused \(av_trim_ms\): .*-100 to 200/);
  assert.equal((await zoneOf(LIVING.id)).av_trim_ms, 25, "nothing of it was applied");

  getByLabel(app, "Back to rooms").click();
  await until("the rooms", () => queryAllByLabel(app, `Theater for ${LIVING.name}`).length, 1);
  assert.equal(getByLabel(app, "Rooms").localName, "main");
  assert.equal(location.hash, "");
});
