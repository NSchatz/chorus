// A room's theater screen (src/theater.js) in the shell, over a scripted
// server: where it is offered (a room with a TV input, by the kind the state
// says, or a theater set) and where it is not; that each control, found by
// its label, sends exactly its documented command and shows the value the
// server answers with, never one of its own; that the trim stays inside the
// catalog's range; and that a room with no sub has no bass management control.

import assert from "node:assert/strict";
import { afterEach, test } from "node:test";

import { AV_TRIM_MS, avTrimCommand, bassManagementCommand, createClient, decibelLiteral, soundCommand } from "../src/api.js";
import "../src/chorus-app.js";
import { addressOf, routeOf } from "../src/routes.js";
import { createStore, roomsOf, tvInputsOf } from "../src/state.js";
import { THEATER_SCREEN, milliseconds, subLevel } from "../src/theater.js";
import { fakeServer, fakeTimers, settle, stateOf, zone } from "./fake-server.js";
import { getByLabel, queryAllByLabel } from "./label-query.js";

let stores = [];

afterEach(() => {
  for (const store of stores) store.stop();
  stores = [];
  document.body.replaceChildren();
  history.replaceState(null, "", "/");
});

const NAME = "Living Room";
const HUB = "hub";
const TV = `${HUB}/tv`;
const TV_NAME = "The television";
const SOUND = { bass: 0, treble: 0, loudness: true, night: false, speech: false, tv_upmix: "off" };
const BASS = { crossover_hz: 80, sub_level_db: 0, sub_polarity: "normal", active: true };
const THEATER_SET = ["FL", "FR", "FC", "LFE", "SL", "SR"].map((role) => ({ endpoint: `speaker-${role.toLowerCase()}`, role }));

// The house: the living room, whose hub offers the TV's optical input and
// whose set is a theater with a sub; and a den with neither, which must not
// change and is offered no theater screen. `living` is what the living room's
// zone says beyond that, and `more` the state's other members.
const house = (serial, living = {}, more = {}) =>
  stateOf(
    serial,
    [
      zone("living", {
        name: NAME,
        endpoints: [HUB],
        bond: THEATER_SET,
        sound: SOUND,
        av_trim_ms: 0,
        bass_management: BASS,
        ...living,
      }),
      zone("den", { sound: SOUND, av_trim_ms: 0, bass_management: { ...BASS, active: false } }),
    ],
    {
      inputs: [TV, "amp/line-1"],
      input_kinds: [
        { input: TV, kind: "optical", tv: true },
        { input: "amp/line-1", kind: "line_in", tv: false },
      ],
      input_labels: [{ input: TV, name: TV_NAME, role: "line-in" }],
      autoplay: [],
      ...more,
    },
  );

const screenOf = (app) => app.shadowRoot.querySelector("chorus-room-theater");
const autoplayOf = (app) => screenOf(app)?.shadowRoot.querySelector("chorus-autoplay") ?? null;

async function rendered(app) {
  await settle();
  await app.updateComplete;
  await screenOf(app)?.updateComplete;
  await autoplayOf(app)?.updateComplete;
  const rooms = app.shadowRoot.querySelector("chorus-rooms");
  await rooms?.updateComplete;
  await Promise.all([...(rooms?.shadowRoot.querySelectorAll("chorus-room-card") ?? [])].map((card) => card.updateComplete));
}

// The app over `server`, opened at a room's theater screen, or at the home.
async function open(server, { room = "living", fetch = server.fetch, home = false } = {}) {
  history.replaceState(null, "", home ? "/app/" : `/app/${addressOf(THEATER_SCREEN, { room })}`);
  const store = createStore(createClient({ fetch, base: server.base, timers: fakeTimers() }));
  stores.push(store);
  const app = document.createElement("chorus-app");
  app.store = store;
  document.body.append(app);
  store.start();
  await rendered(app);
  return app;
}

const text = (node) => node.textContent.replace(/\s+/g, " ").trim();
const figure = (app, field) => text(screenOf(app).shadowRoot.querySelector(`[data-value="${field}"]`));
const alert = (app) => screenOf(app).shadowRoot.querySelector("[role=alert]");
const pressed = (app, label) => getByLabel(app, label).getAttribute("aria-pressed") === "true";

// What the screen shows, read off its controls, in the server's terms.
const shown = (app) => ({
  av_trim_ms: Number(getByLabel(app, `A/V trim for ${NAME}`).value),
  tv_upmix: ["off", "ambient"].filter((word) => pressed(app, `TV upmix ${word} for ${NAME}`)),
  crossover_hz: Number(getByLabel(app, `Crossover for ${NAME}`).value),
  sub_level_db: Number(getByLabel(app, `Sub level for ${NAME}`).value),
  sub_polarity: ["normal", "inverted"].filter((word) => pressed(app, `Sub polarity ${word} for ${NAME}`)),
});
// The TV input's rule as the screen shows it: [target, on, stop on standby, low latency].
const shownRule = (app) => [
  getByLabel(app, `Autoplay target for ${TV_NAME}`).value,
  pressed(app, `Autoplay for ${TV_NAME}`),
  pressed(app, `Stop on standby for ${TV_NAME}`),
  pressed(app, `Low latency for ${TV_NAME}`),
];
const AS_HELD = { av_trim_ms: 0, tv_upmix: ["off"], crossover_hz: 80, sub_level_db: 0, sub_polarity: ["normal"] };

// A person moves a slider to `value` and lets go.
function slide(slider, value) {
  slider.focus();
  slider.value = String(value);
  slider.dispatchEvent(new Event("input", { bubbles: true }));
  slider.dispatchEvent(new Event("change", { bubbles: true }));
}

const cardLinks = (app, name) => queryAllByLabel(app, `Theater for ${name}`);

// --- where it is offered ----------------------------------------------------

test("a room with a TV input, by the kind the state says, is offered the theater screen, and a room without one is not", async () => {
  // No bonded set anywhere: the TV input alone is what offers it.
  const state = house(1, { bond: [], bass_management: { ...BASS, active: false } });
  assert.deepEqual(tvInputsOf(state), [{ input: TV, kind: "optical" }], "the line-in is not a TV's");
  const [living, den] = roomsOf(state);
  assert.deepEqual([living.theater.offered, living.theater.tvInputs], [true, [{ input: TV, kind: "optical" }]]);
  assert.deepEqual([den.theater.offered, den.theater.tvInputs], [false, []]);

  const app = await open(fakeServer(state), { home: true });
  const [link] = cardLinks(app, NAME);
  assert.equal(link.getAttribute("href"), "#/rooms/living/theater");
  assert.deepEqual(cardLinks(app, "den"), [], "the den has no TV input and no theater set");
  assert.equal(queryAllByLabel(app, "Sound for den").length, 1, "its other screens are still linked");

  link.click();
  await rendered(app);
  assert.equal(getByLabel(app, `Theater of ${NAME}`).localName, "main");
  assert.equal(routeOf(location.hash).screen, THEATER_SCREEN);
  assert.equal(getByLabel(app, `A/V trim for ${NAME}`).type, "range");
  // The TV input is listed by the name a person gave it, with what it is.
  const row = autoplayOf(app).shadowRoot.querySelector(`li[data-input="${TV}"]`);
  assert.equal(text(row.querySelector("h3")), TV_NAME);
  assert.equal(text(row.querySelector("[data-kind]")), "Optical");
  assert.equal(autoplayOf(app).shadowRoot.querySelectorAll("li[data-input]").length, 1, "the line-in is not listed");
});

test("the same input said to be a line-in offers nothing: it is the kind that decides", async () => {
  const state = house(1, { bond: [] }, { input_kinds: [{ input: TV, kind: "line_in", tv: false }] });
  assert.equal(roomsOf(state)[0].theater.offered, false);
  const app = await open(fakeServer(state), { home: true });
  assert.deepEqual(cardLinks(app, NAME), []);
  // A server that says no kind at all (before ADR 0194) offers nothing either.
  assert.equal(roomsOf(house(1, { bond: [] }, { input_kinds: undefined }))[0].theater.offered, false);
  // A kind with no `tv` beside it is read by the kind.
  assert.deepEqual(tvInputsOf({ input_kinds: [{ input: TV, kind: "hdmi_arc" }, { input: "a/b", kind: "line_in" }] }), [
    { input: TV, kind: "hdmi_arc" },
  ]);
});

test("a TV input belongs to the room whose endpoint offers it, or the room its rule plays in", () => {
  const rule = { input: TV, target: "den", enabled: true };
  const [living, den] = roomsOf(house(1, { bond: [] }, { autoplay: [rule] }));
  assert.equal(living.theater.offered, true, "its hub is the living room's");
  assert.deepEqual(den.theater.tvInputs, [{ input: TV, kind: "optical" }], "and it plays in the den by its rule");
  // A hub in no room, with no rule: the input is no room's.
  const [alone] = roomsOf(house(1, { bond: [], endpoints: [] }));
  assert.equal(alone.theater.offered, false);
});

test("a theater set is offered the screen with no TV input, and a pair is not", async () => {
  const none = { inputs: [], input_kinds: undefined };
  const pair = [{ endpoint: "a", role: "FL" }, { endpoint: "b", role: "FR" }]; // prettier-ignore
  assert.equal(roomsOf(house(1, { bond: pair }, none))[0].theater.offered, false);
  const [living] = roomsOf(house(1, {}, none));
  assert.deepEqual([living.theater.offered, living.theater.set, living.theater.surrounds], [true, true, true]);

  const app = await open(fakeServer(house(1, {}, none)));
  assert.equal(getByLabel(app, `A/V trim for ${NAME}`).type, "range");
  assert.equal(text(autoplayOf(app).shadowRoot.querySelector("[data-none]")), "This room has no TV input now.");
  assert.deepEqual(queryAllByLabel(app, `Autoplay for ${TV_NAME}`), []);
});

test("a room that is offered none says so at the screen's address, with no control", async () => {
  const app = await open(fakeServer(house(1)), { room: "den" });
  assert.equal(getByLabel(app, "Theater of den").localName, "main");
  assert.match(text(screenOf(app).shadowRoot.querySelector("[data-none]")), /no TV input and no theater set/);
  assert.equal(screenOf(app).shadowRoot.querySelectorAll("input, button").length, 0);
  assert.equal(autoplayOf(app), null);
});

// --- what it shows ----------------------------------------------------------

test("the screen shows the trim, the TV input's rule, the upmix and the sub's settings as the server holds them", async () => {
  const rule = { input: TV, target: "living", enabled: true, stop_on_standby: false };
  const state = house(
    1,
    {
      av_trim_ms: -40,
      sound: { ...SOUND, tv_upmix: "ambient" },
      bass_management: { crossover_hz: 100, sub_level_db: -3.5, sub_polarity: "inverted", active: true },
    },
    { autoplay: [rule] },
  );
  const app = await open(fakeServer(state));
  assert.equal(text(screenOf(app).shadowRoot.querySelector("h2")), `Theater of ${NAME}`);
  assert.deepEqual(shown(app), {
    av_trim_ms: -40,
    tv_upmix: ["ambient"],
    crossover_hz: 100,
    sub_level_db: -3.5,
    sub_polarity: ["inverted"],
  });
  assert.deepEqual(
    ["av_trim_ms", "tv_upmix", "crossover_hz", "sub_level_db", "sub_polarity"].map((field) => figure(app, field)),
    ["-40 ms", "Ambient", "100 Hz", "-3.50 dB", "Inverted"],
  );
  assert.deepEqual(shownRule(app), ["living", true, false, true]);
  assert.equal(text(alert(app)), "");
  assert.deepEqual([milliseconds(0), milliseconds(200), milliseconds(-100)], ["0 ms", "+200 ms", "-100 ms"]);
  assert.deepEqual([subLevel(0), subLevel(600), subLevel(-325)], ["0.00 dB", "+6.00 dB", "-3.25 dB"]);
});

// --- the trim stays inside the catalog's range ------------------------------

test("the trim control is the catalog's -100 to 200 ms in whole milliseconds, and asks for nothing outside it", async () => {
  assert.deepEqual(AV_TRIM_MS, { min: -100, max: 200 });
  const server = fakeServer(house(1, { av_trim_ms: 200 }));
  const app = await open(server);
  const slider = getByLabel(app, `A/V trim for ${NAME}`);
  assert.deepEqual([slider.type, slider.min, slider.max, slider.step], ["range", "-100", "200", "1"]);
  assert.equal(slider.getAttribute("aria-valuetext"), "+200 ms");

  // At the late end there is no later to ask for; one earlier is asked for.
  assert.equal(getByLabel(app, `A/V trim 1 ms later for ${NAME}`).disabled, true);
  assert.equal(getByLabel(app, `A/V trim 1 ms earlier for ${NAME}`).disabled, false);
  server.answer = () => ({ status: 200, body: JSON.stringify(house(2, { av_trim_ms: -100 })) });
  getByLabel(app, `A/V trim 1 ms earlier for ${NAME}`).click();
  await rendered(app);
  assert.deepEqual(server.commands, ['{"v":2,"t":"av_trim","zone":"living","av_trim_ms":199}']);
  // The server answered with the early end: no earlier to ask for.
  assert.equal(figure(app, "av_trim_ms"), "-100 ms");
  assert.equal(getByLabel(app, `A/V trim 1 ms earlier for ${NAME}`).disabled, true);
  assert.equal(getByLabel(app, `A/V trim 1 ms later for ${NAME}`).disabled, false);

  // The command itself holds the range, whatever it is handed.
  assert.equal(avTrimCommand("living", 999), '{"v":2,"t":"av_trim","zone":"living","av_trim_ms":200}');
  assert.equal(avTrimCommand("living", -999), '{"v":2,"t":"av_trim","zone":"living","av_trim_ms":-100}');
  assert.equal(avTrimCommand("living", 12.6), '{"v":2,"t":"av_trim","zone":"living","av_trim_ms":13}');
});

// --- each control sends its documented command ------------------------------

// Each control: what a person does to it, the one command that is sent, and
// the living room the scripted server answers with. The answer is never what
// was asked: the server says something else, and the screen shows the answer.
const CONTROLS = [
  {
    label: `A/V trim for ${NAME}`,
    act: (control) => slide(control, -40),
    command: '{"v":2,"t":"av_trim","zone":"living","av_trim_ms":-40}',
    answer: { av_trim_ms: -35 },
    then: { ...AS_HELD, av_trim_ms: -35 },
    figure: ["av_trim_ms", "-35 ms"],
  },
  {
    label: `A/V trim 1 ms later for ${NAME}`,
    act: (control) => control.click(),
    command: '{"v":2,"t":"av_trim","zone":"living","av_trim_ms":1}',
    answer: { av_trim_ms: 7 },
    then: { ...AS_HELD, av_trim_ms: 7 },
    figure: ["av_trim_ms", "+7 ms"],
  },
  {
    label: `TV upmix ambient for ${NAME}`,
    act: (control) => control.click(),
    command: '{"v":2,"t":"sound","zone":"living","tv_upmix":"ambient"}',
    // The server takes the upmix and, as another client just asked, a trim.
    answer: { sound: { ...SOUND, tv_upmix: "ambient" }, av_trim_ms: 12 },
    then: { ...AS_HELD, tv_upmix: ["ambient"], av_trim_ms: 12 },
    figure: ["tv_upmix", "Ambient"],
  },
  {
    label: `Crossover for ${NAME}`,
    act: (control) => slide(control, 120),
    command: '{"v":2,"t":"bass_management","zone":"living","crossover_hz":120}',
    answer: { bass_management: { ...BASS, crossover_hz: 110 } },
    then: { ...AS_HELD, crossover_hz: 110 },
    figure: ["crossover_hz", "110 Hz"],
  },
  {
    label: `Sub level for ${NAME}`,
    act: (control) => slide(control, -3.5),
    command: '{"v":2,"t":"bass_management","zone":"living","sub_level_db":-3.50}',
    answer: { bass_management: { ...BASS, sub_level_db: -3 } },
    then: { ...AS_HELD, sub_level_db: -3 },
    figure: ["sub_level_db", "-3.00 dB"],
  },
  {
    label: `Sub polarity inverted for ${NAME}`,
    act: (control) => control.click(),
    command: '{"v":2,"t":"bass_management","zone":"living","sub_polarity":"inverted"}',
    answer: { bass_management: { ...BASS, sub_polarity: "inverted", crossover_hz: 90 } },
    then: { ...AS_HELD, sub_polarity: ["inverted"], crossover_hz: 90 },
    figure: ["sub_polarity", "Inverted"],
  },
];

for (const control of CONTROLS) {
  test(`"${control.label}" sends one command with only its field, and shows what the server answers`, async () => {
    const server = fakeServer(house(1));
    server.answer = () => ({ status: 200, body: JSON.stringify(house(2, control.answer)) });
    // The server holds its answer back: until it comes, nothing has changed.
    let release;
    const held = new Promise((resolve) => (release = resolve));
    const app = await open(server, {
      fetch: async (url, options) => {
        if (String(url).endsWith("api/command")) await held;
        return server.fetch(url, options);
      },
    });
    assert.deepEqual(shown(app), AS_HELD);

    const element = getByLabel(app, control.label);
    control.act(element);
    await rendered(app);
    assert.deepEqual(server.commands, [], "the answer has not come");
    element.blur();
    await rendered(app);
    assert.deepEqual(shown(app), AS_HELD, "no optimistic value: the screen is as the server still holds it");

    release();
    await rendered(app);
    assert.deepEqual(server.commands, [control.command], "exactly one command, with only the changed field");
    assert.deepEqual(shown(app), control.then, "the screen shows the server's answer, not what was asked");
    assert.equal(figure(app, control.figure[0]), control.figure[1]);
    assert.equal(getByLabel(app, control.label), element, "the control is the element it was");
    assert.equal(text(alert(app)), "");
  });
}

test("the commands are the catalog's bytes: its own vectors, and a level with two places", () => {
  assert.equal(soundCommand("living", { tv_upmix: "ambient" }), '{"v":2,"t":"sound","zone":"living","tv_upmix":"ambient"}');
  assert.equal(soundCommand("living", { bass: 1, tv_upmix: "off" }), '{"v":2,"t":"sound","zone":"living","bass":1,"tv_upmix":"off"}');
  assert.equal(bassManagementCommand("living", { crossover_hz: 80 }), '{"v":2,"t":"bass_management","zone":"living","crossover_hz":80}');
  assert.equal(
    bassManagementCommand("living", { crossover_hz: 100, sub_level_db: -350, sub_polarity: "inverted" }),
    '{"v":2,"t":"bass_management","zone":"living","crossover_hz":100,"sub_level_db":-3.50,"sub_polarity":"inverted"}',
  );
  assert.equal(bassManagementCommand("living"), '{"v":2,"t":"bass_management","zone":"living"}');
  assert.deepEqual([0, 600, -1200, -5, 125, 9999, -9999].map(decibelLiteral), ["0.00", "6.00", "-12.00", "-0.05", "1.25", "6.00", "-12.00"]);
  assert.equal(bassManagementCommand("living", { crossover_hz: 20 }), '{"v":2,"t":"bass_management","zone":"living","crossover_hz":40}');
});

test("the bass sliders are the catalog's ranges", async () => {
  const app = await open(fakeServer(house(1)));
  const crossover = getByLabel(app, `Crossover for ${NAME}`);
  assert.deepEqual([crossover.min, crossover.max, crossover.step], ["40", "200", "1"]);
  const level = getByLabel(app, `Sub level for ${NAME}`);
  assert.deepEqual([level.min, level.max, level.step], ["-12", "6", "0.5"]);
});

// --- TV autoplay: one command for the TV input, with the fields set ---------

// The living room's TV rule as the server holds it, and each control of it:
// the one `autoplay` it sends, which carries the whole rule with the one change.
const RULE = { input: TV, target: "living", enabled: false };
const RULE_CONTROLS = [
  {
    label: `Autoplay for ${TV_NAME}`,
    held: RULE,
    command: `{"v":2,"t":"autoplay","input":"${TV}","target":"living","enabled":true}`,
    answer: { ...RULE, enabled: true },
    then: ["living", true, true, true],
  },
  {
    label: `Stop on standby for ${TV_NAME}`,
    held: { ...RULE, enabled: true },
    command: `{"v":2,"t":"autoplay","input":"${TV}","target":"living","enabled":true,"stop_on_standby":false}`,
    answer: { ...RULE, enabled: true, stop_on_standby: false },
    then: ["living", true, false, true],
  },
  {
    label: `Low latency for ${TV_NAME}`,
    held: { ...RULE, enabled: true, stop_on_standby: false },
    command: `{"v":2,"t":"autoplay","input":"${TV}","target":"living","enabled":true,"stop_on_standby":false,"low_latency":false}`,
    answer: { ...RULE, enabled: true, stop_on_standby: false, low_latency: false },
    then: ["living", true, false, false],
  },
  {
    // Back on: the field is left out, which is how the catalog writes "on".
    label: `Stop on standby for ${TV_NAME}`,
    held: { ...RULE, enabled: true, stop_on_standby: false, low_latency: false },
    command: `{"v":2,"t":"autoplay","input":"${TV}","target":"living","enabled":true,"low_latency":false}`,
    answer: { ...RULE, enabled: true, low_latency: false },
    then: ["living", true, true, false],
  },
  {
    // The switch keeps both options as the rule has them.
    label: `Autoplay for ${TV_NAME}`,
    held: { ...RULE, enabled: true, stop_on_standby: false, low_latency: false },
    command: `{"v":2,"t":"autoplay","input":"${TV}","target":"living","enabled":false,"stop_on_standby":false,"low_latency":false}`,
    answer: { ...RULE, enabled: false, stop_on_standby: false, low_latency: false },
    then: ["living", false, false, false],
  },
];

for (const control of RULE_CONTROLS) {
  test(`"${control.label}" on ${JSON.stringify(control.held)} sends one autoplay for the TV input with the fields set`, async () => {
    const server = fakeServer(house(1, {}, { autoplay: [control.held] }));
    server.answer = () => ({ status: 200, body: JSON.stringify(house(2, {}, { autoplay: [control.answer] })) });
    const app = await open(server);
    const before = shownRule(app);
    getByLabel(app, control.label).click();
    await rendered(app);
    assert.deepEqual(server.commands, [control.command], "exactly one autoplay command, for the TV input");
    assert.notDeepEqual(shownRule(app), before);
    assert.deepEqual(shownRule(app), control.then, "the screen shows the server's rule");
  });
}

test("a TV input with no rule is switched on for its room in one command, and an option makes the rule switched off", async () => {
  const server = fakeServer(house(1));
  const app = await open(server);
  assert.deepEqual(shownRule(app), ["", false, true, true], "no rule: both options at the catalog's defaults");
  const toggle = getByLabel(app, `Autoplay for ${TV_NAME}`);
  assert.equal(toggle.disabled, false);
  assert.match(text(autoplayOf(app).shadowRoot.querySelector('[data-value="enabled"]')), new RegExp(`plays it in ${NAME}$`));

  // The server does not take it: the screen still shows no rule.
  toggle.click();
  await rendered(app);
  assert.deepEqual(server.commands, [`{"v":2,"t":"autoplay","input":"${TV}","target":"living","enabled":true}`]);
  assert.deepEqual(shownRule(app), ["", false, true, true], "nothing is shown that the server did not say");

  getByLabel(app, `Low latency for ${TV_NAME}`).click();
  await rendered(app);
  assert.equal(server.commands[1], `{"v":2,"t":"autoplay","input":"${TV}","target":"living","enabled":false,"low_latency":false}`);
});

test("a rule that plays the TV elsewhere keeps its target through the switch and the options, and the target can be changed", async () => {
  const held = { input: TV, target: "den", enabled: true, low_latency: false };
  const server = fakeServer(house(1, {}, { autoplay: [held] }));
  server.answer = () => ({ status: 200, body: JSON.stringify(house(1, {}, { autoplay: [held] })) });
  const app = await open(server);
  assert.deepEqual(shownRule(app), ["den", true, true, false]);
  getByLabel(app, `Stop on standby for ${TV_NAME}`).click();
  await rendered(app);
  const picker = getByLabel(app, `Autoplay target for ${TV_NAME}`);
  picker.value = "living";
  picker.dispatchEvent(new Event("change", { bubbles: true }));
  await rendered(app);
  assert.deepEqual(server.commands, [
    `{"v":2,"t":"autoplay","input":"${TV}","target":"den","enabled":true,"stop_on_standby":false,"low_latency":false}`,
    `{"v":2,"t":"autoplay","input":"${TV}","target":"living","enabled":true,"low_latency":false}`,
  ]);
});

test("the house's autoplay screen still shows no TV option", async () => {
  const server = fakeServer(house(1, {}, { autoplay: [{ ...RULE, stop_on_standby: false }] }));
  history.replaceState(null, "", "/app/#/autoplay");
  const store = createStore(createClient({ fetch: server.fetch, base: server.base, timers: fakeTimers() }));
  stores.push(store);
  const app = document.createElement("chorus-app");
  app.store = store;
  document.body.append(app);
  store.start();
  await settle();
  await app.updateComplete;
  await app.shadowRoot.querySelector("chorus-autoplay").updateComplete;
  assert.equal(getByLabel(app, `Autoplay for ${TV_NAME}`).localName, "button");
  assert.deepEqual(queryAllByLabel(app, `Stop on standby for ${TV_NAME}`), []);
  assert.deepEqual(queryAllByLabel(app, `Low latency for ${TV_NAME}`), []);
  // And an input with no rule there still waits for a target.
  assert.equal(getByLabel(app, "Autoplay for amp/line-1").disabled, true);
});

// --- a room with no sub ------------------------------------------------------

test("a room with no sub shows no bass management control, and says why", async () => {
  const noSub = THEATER_SET.filter((member) => member.role !== "LFE");
  const server = fakeServer(house(1, { bond: noSub, bass_management: { ...BASS, active: false } }));
  const app = await open(server);
  // The rest of the section is there.
  assert.equal(getByLabel(app, `A/V trim for ${NAME}`).type, "range");
  assert.equal(getByLabel(app, `TV upmix ambient for ${NAME}`).localName, "button");
  for (const label of ["Crossover", "Sub level", "Sub polarity", "Sub polarity normal", "Sub polarity inverted"]) {
    assert.deepEqual(queryAllByLabel(app, `${label} for ${NAME}`), [], `no "${label}" control`);
  }
  const root = screenOf(app).shadowRoot;
  assert.equal(root.querySelectorAll('[data-field="crossover_hz"], [data-field="sub_level_db"], [data-of="sub_polarity"]').length, 0);
  assert.match(text(root.querySelector("[data-no-sub]")), /no sub/);

  // The sub is bonded (another client's doing): the controls appear.
  server.send(house(2));
  await rendered(app);
  assert.equal(getByLabel(app, `Crossover for ${NAME}`).value, "80");
  assert.equal(root.querySelector("[data-no-sub]"), null);
});

test("a set with no surround speakers is told so beside the upmix, which it can still set", async () => {
  const front = THEATER_SET.filter((member) => ["FL", "FR", "FC"].includes(member.role));
  const server = fakeServer(house(1, { bond: front, bass_management: { ...BASS, active: false } }));
  const app = await open(server);
  assert.match(text(screenOf(app).shadowRoot), /no surround speakers now/);
  assert.equal(getByLabel(app, `TV upmix ambient for ${NAME}`).disabled, false);
});

// --- refusals, other clients, missing values ---------------------------------

test("a refusal is shown with the field the server named and its detail, and the control returns to the server's value", async () => {
  const server = fakeServer(house(1, { av_trim_ms: 20 }));
  const app = await open(server);
  const detail = "the field 'av_trim_ms' is 201 and the catalog declares a whole number from -100 to 200";
  server.answer = () => ({ status: 400, body: JSON.stringify({ v: 2, t: "error", field: "av_trim_ms", detail }) });
  const trim = getByLabel(app, `A/V trim for ${NAME}`);
  slide(trim, 150);
  await rendered(app);
  assert.deepEqual(server.commands, ['{"v":2,"t":"av_trim","zone":"living","av_trim_ms":150}']);
  assert.equal(text(alert(app)), `Refused (av_trim_ms): ${detail}`);
  assert.equal(alert(app).getAttribute("data-refusal-field"), "av_trim_ms");
  assert.equal(trim.value, "20");
  assert.equal(figure(app, "av_trim_ms"), "+20 ms");

  // The next command clears it.
  server.answer = () => ({ status: 200, body: JSON.stringify(house(2, { av_trim_ms: 21 })) });
  getByLabel(app, `A/V trim 1 ms later for ${NAME}`).click();
  await rendered(app);
  assert.equal(text(alert(app)), "");
  assert.equal(figure(app, "av_trim_ms"), "+21 ms");
});

test("a refused autoplay is shown on the TV input, not as the screen's", async () => {
  const server = fakeServer(house(1, {}, { autoplay: [RULE] }));
  const app = await open(server);
  server.answer = () => ({ status: 400, body: JSON.stringify({ v: 2, t: "error", field: "input", detail: "no such input" }) });
  getByLabel(app, `Autoplay for ${TV_NAME}`).click();
  await rendered(app);
  assert.equal(text(autoplayOf(app).shadowRoot.querySelector("li [role=alert]")), "Refused: no such input");
  assert.equal(text(alert(app)), "");
});

test("a change another client makes appears on the screen, except under a slider a person has hold of", async () => {
  const server = fakeServer(house(1));
  const app = await open(server);
  const trim = getByLabel(app, `A/V trim for ${NAME}`);
  server.send(
    house(
      2,
      { av_trim_ms: 60, sound: { ...SOUND, tv_upmix: "ambient" }, bass_management: { ...BASS, crossover_hz: 95, sub_level_db: 1.5, sub_polarity: "inverted" } },
      { autoplay: [{ ...RULE, enabled: true, low_latency: false }] },
    ),
  ); // prettier-ignore
  await rendered(app);
  assert.deepEqual(shown(app), { av_trim_ms: 60, tv_upmix: ["ambient"], crossover_hz: 95, sub_level_db: 1.5, sub_polarity: ["inverted"] });
  assert.deepEqual(shownRule(app), ["living", true, true, false]);
  assert.equal(getByLabel(app, `A/V trim for ${NAME}`), trim, "the screen was patched, not painted again");

  // A person is dragging the trim: the figure follows the finger, and an
  // update moves the crossover and leaves the trim's slider where the finger is.
  trim.focus();
  trim.value = "-30";
  trim.dispatchEvent(new Event("input", { bubbles: true }));
  await rendered(app);
  assert.equal(figure(app, "av_trim_ms"), "-30 ms");
  server.send(house(3, { av_trim_ms: 80, bass_management: { ...BASS, crossover_hz: 60 } }));
  await rendered(app);
  assert.equal(trim.value, "-30");
  assert.equal(getByLabel(app, `Crossover for ${NAME}`).value, "60");
  trim.blur();
  await rendered(app);
  assert.equal(trim.value, "80");
  assert.equal(figure(app, "av_trim_ms"), "+80 ms");
  assert.deepEqual(server.commands, [], "following the server sends nothing");
});

test("a setting the state does not carry is said to be unavailable, not shown as a default", async () => {
  const server = fakeServer(house(1, { av_trim_ms: undefined, sound: { bass: 0 }, bass_management: { active: true, crossover_hz: 70 } }));
  const app = await open(server);
  assert.deepEqual(queryAllByLabel(app, `A/V trim for ${NAME}`), []);
  assert.equal(figure(app, "av_trim_ms"), "Unavailable");
  assert.equal(getByLabel(app, `A/V trim 1 ms later for ${NAME}`).disabled, true);
  assert.equal(figure(app, "tv_upmix"), "Unavailable");
  assert.equal(getByLabel(app, `TV upmix ambient for ${NAME}`).disabled, true);
  assert.equal(getByLabel(app, `Crossover for ${NAME}`).value, "70");
  assert.equal(figure(app, "sub_level_db"), "Unavailable");
  assert.equal(figure(app, "sub_polarity"), "Unavailable");
});

test("before the first state the screen says it is reading, and a room the server does not have is said so", async () => {
  const server = fakeServer(null);
  const app = await open(server, { room: "attic" });
  const words = () => text(screenOf(app).shadowRoot.querySelector("[data-missing]"));
  assert.equal(words(), "Reading this server's rooms.");
  assert.equal(getByLabel(app, "Theater of attic").localName, "main");
  server.send(house(1));
  await rendered(app);
  assert.equal(words(), 'This server has no room "attic".');
  assert.equal(getByLabel(app, "Back to rooms").localName, "a");
});

test("its styles name tokens, never a literal colour or length", () => {
  for (const name of ["chorus-room-theater", "chorus-autoplay"]) {
    const sheet = customElements.get(name).styles.cssText;
    assert.match(sheet, /min-height: var\(--control-size\)/);
    assert.doesNotMatch(sheet, /#[0-9a-f]{3,8}\b|\b\d+(px|rem|em)\b/i);
  }
});
