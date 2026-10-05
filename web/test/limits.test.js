// A room's volume limit and quiet hours (src/limits.js) in the shell, over a
// scripted server: setting the limit, adding, editing and removing a window
// (up to the catalog's 8) and the switch each send the documented command,
// in the bytes of the catalog's own vectors, and the screen shows what the
// server's state says: the limit in force (`effective_limit`), the volume
// under it and which window the server's clock is inside.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { afterEach, test } from "node:test";

import {
  MAX_QUIET_WINDOWS,
  WEEK,
  createClient,
  limitCommand,
  quietHoursCommand,
  quietHoursEnabledCommand,
} from "../src/api.js";
import "../src/chorus-app.js";
import { LIMITS_SCREEN } from "../src/limits.js";
import { addressOf, routeOf } from "../src/routes.js";
import { createStore, limitsOf } from "../src/state.js";
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

const NAME = "Bed Room";
const WEEKNIGHTS = { days: ["mon", "tue", "wed", "thu", "fri"], start: "22:00", end: "07:00", limit: 0.25 };
const WEEKEND = { days: ["sat", "sun"], start: "13:00", end: "15:00", limit: 0.4 };

// The room as the server says it: its limit, the limit in force, its windows
// (each with `active`) and whether quiet hours are switched on.
const bedroom = (more = {}) =>
  zone("bedroom", { name: NAME, volume: 0.6, limit: 0.8, effective_limit: 0.8, quiet: [], quiet_enabled: true, ...more });
const house = (serial, more = {}) => stateOf(serial, [bedroom(more), zone("den", { limit: 1, effective_limit: 1 })]);

async function rendered(app) {
  await settle();
  await app.updateComplete;
  await app.shadowRoot.querySelector("chorus-room-limits")?.updateComplete;
}

async function open(server, room = "bedroom") {
  history.replaceState(null, "", `/app/${addressOf(LIMITS_SCREEN, { room })}`);
  const store = createStore(createClient({ fetch: server.fetch, base: server.base, timers: fakeTimers() }));
  stores.push(store);
  const app = document.createElement("chorus-app");
  app.store = store;
  document.body.append(app);
  store.start();
  await rendered(app);
  return app;
}

// A scripted server that does what the real one does with the three
// commands, as docs/control-plane.md says it: `limit` and the windows are
// held as sent, a window is active when the test says the clock is inside it
// (`inside(window)`), and the limit in force and the volume follow.
function limitsServer(start = {}, inside = () => false) {
  let room = { limit: 0.8, volume: 0.6, quiet: [], quiet_enabled: true, ...start };
  let serial = 1;
  const said = () => {
    const quiet = room.quiet.map((window) => ({ ...window, active: inside(window) }));
    const caps = room.quiet_enabled ? quiet.filter((window) => window.active).map((window) => window.limit) : [];
    const effective = Math.min(room.limit, ...caps);
    room = { ...room, volume: Math.min(room.volume, effective) };
    return house(serial, { ...room, quiet, effective_limit: effective });
  };
  const server = fakeServer(said());
  server.answer = (body) => {
    const command = JSON.parse(body);
    if (command.t === "limit") room = { ...room, limit: command.limit };
    if (command.t === "quiet_hours") room = { ...room, quiet: command.windows };
    if (command.t === "quiet_hours_enabled") room = { ...room, quiet_enabled: command.enabled };
    serial += 1;
    server.snapshot = said();
    return { status: 200, body: JSON.stringify(server.snapshot) };
  };
  return server;
}

const screenOf = (app) => app.shadowRoot.querySelector("chorus-room-limits");
const text = (node) => node.textContent.replace(/\s+/g, " ").trim();
const value = (app, name) => text(screenOf(app).shadowRoot.querySelector(`[data-value="${name}"]`));
const alert = (app) => screenOf(app).shadowRoot.querySelector("[role=alert]");
const items = (app) => [...screenOf(app).shadowRoot.querySelectorAll("li[data-window]")];
const control = (app, label) => getByLabel(app, `${label} for ${NAME}`);
const pressed = (button) => button.getAttribute("aria-pressed") === "true";
const DAYS = { mon: "Monday", tue: "Tuesday", wed: "Wednesday", thu: "Thursday", fri: "Friday", sat: "Saturday", sun: "Sunday" };

// The windows as the screen shows them, in the command's terms.
const shownWindows = (app) =>
  items(app).map((_, at) => ({
    days: WEEK.filter((day) => pressed(control(app, `${DAYS[day]}, window ${at + 1}`))),
    start: control(app, `Start of window ${at + 1}`).value,
    end: control(app, `End of window ${at + 1}`).value,
    limit: Number(control(app, `Limit of window ${at + 1}`).value) / 1000,
  }));

// A person moves a slider to `to` and lets go of it.
function slide(slider, to) {
  slider.focus();
  slider.value = String(to);
  slider.dispatchEvent(new Event("input", { bubbles: true }));
  slider.dispatchEvent(new Event("change", { bubbles: true }));
  slider.blur();
}

// A person sets a time field.
function setTime(field, to) {
  field.focus();
  field.value = to;
  field.dispatchEvent(new Event("change", { bubbles: true }));
  field.blur();
}

const last = (server) => server.commands.at(-1);

test("the three commands are the catalog's own vectors, and a window's days are written in week order", () => {
  assert.equal(limitCommand("kitchen", 600), fixture("limit.json"));
  assert.equal(
    quietHoursCommand("bedroom", [
      // Given out of order: the catalog refuses days that are not in week order.
      { days: ["fri", "mon", "wed", "tue", "thu"], start: "22:00", end: "07:00", limit: 250 },
      { days: ["sun", "sat"], start: "13:00", end: "15:00", limit: 400 },
    ]),
    fixture("quiet_hours.json"),
  );
  assert.equal(quietHoursCommand("bedroom", []), '{"v":2,"t":"quiet_hours","zone":"bedroom","windows":[]}');
  assert.equal(quietHoursEnabledCommand("bedroom", false), fixture("quiet_hours_enabled.json"));
  assert.equal(MAX_QUIET_WINDOWS, 8);
});

test("a room's limits are read from the catalog's own state, and what it does not carry is null", () => {
  const state = JSON.parse(fixture("state-quiet-disabled.json"));
  assert.deepEqual(limitsOf(state.zones[0]), {
    limit: 800,
    effectiveLimit: 800,
    quietEnabled: false,
    windows: [{ days: ["fri"], start: "22:00", end: "07:00", limit: 200, active: true }],
  });
  assert.deepEqual(limitsOf({ id: "den", limit: "high", quiet: "none" }), {
    limit: null,
    effectiveLimit: null,
    quietEnabled: null,
    windows: [],
  });
});

test("the screen has an address of its own and opens from the room's card", async () => {
  assert.equal(addressOf(LIMITS_SCREEN, { room: "bedroom" }), "#/rooms/bedroom/limits");
  assert.deepEqual(routeOf("#/rooms/bedroom/limits"), {
    screen: LIMITS_SCREEN,
    params: { room: "bedroom" },
    address: "#/rooms/bedroom/limits",
  });
  const server = fakeServer(house(1));
  const store = createStore(createClient({ fetch: server.fetch, base: server.base, timers: fakeTimers() }));
  stores.push(store);
  const app = document.createElement("chorus-app");
  app.store = store;
  document.body.append(app);
  store.start();
  await settle();
  await app.updateComplete;
  const rooms = app.shadowRoot.querySelector("chorus-rooms");
  await rooms.updateComplete;
  await Promise.all([...rooms.shadowRoot.querySelectorAll("chorus-room-card")].map((card) => card.updateComplete));
  const link = getByLabel(app, `Limits for ${NAME}`);
  assert.equal(link.getAttribute("href"), "#/rooms/bedroom/limits");
  link.click();
  await rendered(app);
  assert.equal(location.hash, "#/rooms/bedroom/limits");
  assert.equal(getByLabel(app, `Volume limits of ${NAME}`).localName, "main");
  assert.equal(control(app, "Volume limit").value, "800");
});

test("the screen shows the limit, the limit in force, the volume under it and which window is active, as the server says them", async () => {
  const server = fakeServer(
    house(1, {
      volume: 0.25,
      limit: 0.8,
      effective_limit: 0.25,
      quiet: [
        { ...WEEKNIGHTS, active: true },
        { ...WEEKEND, active: false },
      ],
    }),
  );
  const app = await open(server);
  assert.equal(text(screenOf(app).shadowRoot.querySelector("h2")), `Volume limits of ${NAME}`);
  const limit = control(app, "Volume limit");
  assert.deepEqual([limit.type, limit.min, limit.max, limit.value], ["range", "0", "1000", "800"]);
  assert.equal(value(app, "limit"), "80%");
  // The server's effective limit, not one worked out here: it is under the limit.
  assert.equal(value(app, "effective"), "25%");
  assert.equal(value(app, "volume"), "25%");
  assert.equal(pressed(control(app, "Quiet hours")), true);
  assert.equal(value(app, "enabled"), "On");
  assert.deepEqual(shownWindows(app), [WEEKNIGHTS, WEEKEND]);
  assert.deepEqual(
    items(app).map((item) => [item.hasAttribute("data-active"), text(item.querySelector('[data-value="active"]'))]),
    [
      [true, "Active now"],
      [false, "Not active now"],
    ],
  );
  assert.equal(control(app, "Start of window 1").type, "time");
  assert.equal(text(alert(app)), "");
  assert.deepEqual(server.commands, [], "showing sends nothing");
});

test("a window the clock is inside while quiet hours are off is said to be so, and caps nothing", async () => {
  const state = JSON.parse(fixture("state-quiet-disabled.json"));
  state.zones[0].name = NAME;
  const app = await open(fakeServer(state));
  assert.equal(pressed(control(app, "Quiet hours")), false);
  assert.equal(value(app, "enabled"), "Off");
  assert.equal(value(app, "effective"), "80%");
  assert.equal(text(items(app)[0].querySelector('[data-value="active"]')), "Inside it now, and quiet hours are off");
});

test("setting the limit sends one limit command, and the screen shows the server's answer", async () => {
  const server = limitsServer({ volume: 0.6 });
  const app = await open(server);
  const limit = control(app, "Volume limit");
  // During the drag the figure follows the finger and nothing is sent.
  limit.focus();
  limit.value = "450";
  limit.dispatchEvent(new Event("input", { bubbles: true }));
  await rendered(app);
  assert.equal(value(app, "limit"), "45%");
  assert.deepEqual(server.commands, []);
  limit.dispatchEvent(new Event("change", { bubbles: true }));
  limit.blur();
  await rendered(app);
  assert.deepEqual(server.commands, ['{"v":2,"t":"limit","zone":"bedroom","limit":0.450}']);
  assert.equal(control(app, "Volume limit"), limit, "the control is the element it was");
  assert.equal(limit.value, "450");
  assert.equal(value(app, "effective"), "45%");
  assert.equal(value(app, "volume"), "45%", "the volume the server pulled down to the limit");

  // The server answers with something else: the screen shows the answer.
  server.answer = () => ({ status: 200, body: JSON.stringify(house(9, { limit: 0.7, effective_limit: 0.7 })) });
  slide(limit, 900);
  await rendered(app);
  assert.equal(last(server), '{"v":2,"t":"limit","zone":"bedroom","limit":0.900}');
  assert.equal(limit.value, "700");
  assert.equal(value(app, "limit"), "70%");
});

test("the quiet-hours switch sends quiet_hours_enabled with the opposite of what the server holds, and keeps the windows", async () => {
  const server = limitsServer({ quiet: [WEEKNIGHTS] }, () => true);
  const app = await open(server);
  assert.equal(value(app, "effective"), "25%");
  control(app, "Quiet hours").click();
  await rendered(app);
  assert.deepEqual(server.commands, [fixture("quiet_hours_enabled.json")]);
  assert.equal(pressed(control(app, "Quiet hours")), false);
  assert.equal(value(app, "enabled"), "Off");
  assert.equal(value(app, "effective"), "80%", "off, no window caps the room");
  assert.deepEqual(shownWindows(app), [WEEKNIGHTS], "the windows are kept");
  assert.equal(text(items(app)[0].querySelector('[data-value="active"]')), "Inside it now, and quiet hours are off");

  control(app, "Quiet hours").click();
  await rendered(app);
  assert.equal(last(server), '{"v":2,"t":"quiet_hours_enabled","zone":"bedroom","enabled":true}');
  assert.equal(value(app, "enabled"), "On");
  assert.equal(value(app, "effective"), "25%");
  assert.equal(server.commands.length, 2);
});

test("a window is written as a draft that sends nothing, and adding it sends the room's windows with it at the end", async () => {
  const server = limitsServer({ quiet: [WEEKNIGHTS] }, (window) => window.start === "13:00");
  const app = await open(server);
  // The draft a person starts from: every night.
  assert.deepEqual(
    WEEK.map((day) => pressed(control(app, `${DAYS[day]}, the new window`))),
    [true, true, true, true, true, true, true],
  );
  for (const day of ["mon", "tue", "wed", "thu", "fri"]) control(app, `${DAYS[day]}, the new window`).click();
  setTime(control(app, "Start of the new window"), "13:00");
  setTime(control(app, "End of the new window"), "15:00");
  const limit = control(app, "Limit of the new window");
  limit.value = "400";
  limit.dispatchEvent(new Event("input", { bubbles: true }));
  await rendered(app);
  assert.equal(value(app, "draft-limit"), "40%");
  assert.deepEqual(server.commands, [], "nothing of a draft is sent");
  assert.equal(items(app).length, 1);

  control(app, "Add window").click();
  await rendered(app);
  assert.deepEqual(server.commands, [fixture("quiet_hours.json")], "one command: both windows, the new one last");
  assert.deepEqual(shownWindows(app), [WEEKNIGHTS, WEEKEND]);
  // The server's clock is inside the new window: it is active and its cap is in force.
  assert.equal(text(items(app)[1].querySelector('[data-value="active"]')), "Active now");
  assert.equal(value(app, "effective"), "40%");
  assert.equal(value(app, "volume"), "40%");
});

test("a draft with no day cannot be added", async () => {
  const server = limitsServer();
  const app = await open(server);
  for (const day of WEEK) control(app, `${DAYS[day]}, the new window`).click();
  await rendered(app);
  assert.equal(control(app, "Add window").disabled, true);
  control(app, "Add window").click();
  await rendered(app);
  assert.deepEqual(server.commands, []);
  // Days picked out of order are still a week in order.
  control(app, "Sunday, the new window").click();
  control(app, "Monday, the new window").click();
  await rendered(app);
  control(app, "Add window").click();
  await rendered(app);
  assert.deepEqual(JSON.parse(last(server)).windows[0].days, ["mon", "sun"]);
});

// Each edit of a window: what a person does, and the one command that is
// sent, which is every window of the room with that one change.
const EDITS = [
  {
    what: "switching a day off",
    act: (app) => control(app, "Friday, window 1").click(),
    windows: [{ ...WEEKNIGHTS, days: ["mon", "tue", "wed", "thu"] }, WEEKEND],
  },
  {
    what: "switching a day on, written in week order",
    act: (app) => control(app, "Wednesday, window 2").click(),
    windows: [WEEKNIGHTS, { ...WEEKEND, days: ["wed", "sat", "sun"] }],
  },
  {
    what: "its start",
    act: (app) => setTime(control(app, "Start of window 1"), "21:30"),
    windows: [{ ...WEEKNIGHTS, start: "21:30" }, WEEKEND],
  },
  {
    what: "its end",
    act: (app) => setTime(control(app, "End of window 2"), "16:45"),
    windows: [WEEKNIGHTS, { ...WEEKEND, end: "16:45" }],
  },
  {
    what: "its limit",
    act: (app) => slide(control(app, "Limit of window 2"), 150),
    windows: [WEEKNIGHTS, { ...WEEKEND, limit: 0.15 }],
  },
  {
    what: "removing it",
    act: (app) => control(app, "Remove window 1").click(),
    windows: [WEEKEND],
  },
];

for (const edit of EDITS) {
  test(`editing a window (${edit.what}) sends one quiet_hours command with every window, and the screen shows the answer`, async () => {
    const server = limitsServer({ quiet: [WEEKNIGHTS, WEEKEND] });
    const app = await open(server);
    edit.act(app);
    await rendered(app);
    assert.equal(server.commands.length, 1, "exactly one command");
    const sent = JSON.parse(server.commands[0]);
    assert.deepEqual([sent.v, sent.t, sent.zone], [2, "quiet_hours", "bedroom"]);
    assert.deepEqual(sent.windows, edit.windows);
    assert.match(server.commands[0], /"limit":\d\.\d{3}\}/, "a limit is written as the catalog writes a volume");
    assert.doesNotMatch(server.commands[0], /active/, "what the server says of a window is not sent back");
    assert.deepEqual(shownWindows(app), edit.windows);
    assert.equal(text(alert(app)), "");
  });
}

test("a change made before the server has answered the last is made to what that one asked for, and does not undo it", async () => {
  const server = limitsServer({ quiet: [WEEKNIGHTS] });
  // The server holds its answers back.
  let release;
  const held = new Promise((resolve) => (release = resolve));
  const fetch = async (url, options) => {
    if (String(url).endsWith("api/command")) await held;
    return server.fetch(url, options);
  };
  history.replaceState(null, "", `/app/${addressOf(LIMITS_SCREEN, { room: "bedroom" })}`);
  const store = createStore(createClient({ fetch, base: server.base, timers: fakeTimers() }));
  stores.push(store);
  const app = document.createElement("chorus-app");
  app.store = store;
  document.body.append(app);
  store.start();
  await rendered(app);

  // Three taps, one after another, with no answer yet.
  control(app, "Friday, window 1").click();
  control(app, "Saturday, window 1").click();
  control(app, "Remove window 1").click();
  await rendered(app);
  assert.deepEqual(shownWindows(app), [WEEKNIGHTS], "no optimistic value: the screen is as the server still holds it");
  release();
  await rendered(app);
  assert.deepEqual(
    server.commands.map((command) => JSON.parse(command).windows.map((window) => window.days)),
    [[["mon", "tue", "wed", "thu"]], [["mon", "tue", "wed", "thu", "sat"]], []],
    "each command carries the changes before it",
  );
  assert.equal(items(app).length, 0);

  // Every command answered: the next change starts from the server's state.
  control(app, "Add window").click();
  await rendered(app);
  assert.equal(JSON.parse(last(server)).windows.length, 1);
});

test("removing the last window sends an empty list, and the screen says the room has none", async () => {
  const server = limitsServer({ quiet: [WEEKEND] });
  const app = await open(server);
  control(app, "Remove window 1").click();
  await rendered(app);
  assert.deepEqual(server.commands, ['{"v":2,"t":"quiet_hours","zone":"bedroom","windows":[]}']);
  assert.equal(items(app).length, 0);
  assert.equal(text(screenOf(app).shadowRoot.querySelector("[data-none]")), "This room has no quiet-hours window.");
});

test("windows are added up to the catalog's 8, and a ninth is not offered until one is removed", async () => {
  const server = limitsServer();
  const app = await open(server);
  for (let count = 1; count <= MAX_QUIET_WINDOWS; count += 1) {
    setTime(control(app, "End of the new window"), `0${count}:00`);
    await rendered(app);
    control(app, "Add window").click();
    await rendered(app);
    assert.equal(items(app).length, count);
    assert.equal(JSON.parse(last(server)).windows.length, count);
  }
  assert.equal(server.commands.length, 8);
  assert.deepEqual(shownWindows(app).map((window) => window.end), ["01:00", "02:00", "03:00", "04:00", "05:00", "06:00", "07:00", "08:00"]); // prettier-ignore
  assert.deepEqual(queryAllByLabel(app, `Add window for ${NAME}`), [], "no ninth window is offered");
  assert.match(text(screenOf(app).shadowRoot.querySelector("[data-full]")), /at most 8 windows/);

  control(app, "Remove window 8").click();
  await rendered(app);
  assert.equal(items(app).length, 7);
  assert.equal(control(app, "Add window").disabled, false);
});

test("a time field that is cleared sends nothing and takes the server's value again", async () => {
  const server = limitsServer({ quiet: [WEEKEND] });
  const app = await open(server);
  const start = control(app, "Start of window 1");
  setTime(start, "");
  await rendered(app);
  assert.deepEqual(server.commands, []);
  assert.equal(start.value, "13:00");
});

test("a refusal is shown with the field the server named and its words, and the control returns to the server's value", async () => {
  const server = limitsServer({ quiet: [WEEKNIGHTS] });
  const app = await open(server);
  const refusal = JSON.parse(fixture("error-quiet-window-ambiguous.json"));
  server.answer = () => ({ status: 400, body: JSON.stringify(refusal) });
  setTime(control(app, "End of window 1"), "22:00");
  await rendered(app);
  assert.equal(JSON.parse(last(server)).windows[0].end, "22:00");
  assert.equal(text(alert(app)), `Refused (windows): ${refusal.detail}`);
  assert.equal(alert(app).getAttribute("data-refusal-field"), "windows");
  assert.equal(control(app, "End of window 1").value, "07:00");

  // A window with no day left is the server's to refuse too.
  const none = "a quiet-hours window names at least one day";
  server.answer = () => ({ status: 400, body: JSON.stringify({ v: 2, t: "error", field: "windows", detail: none }) });
  server.send(house(2, { quiet: [{ ...WEEKNIGHTS, days: ["fri"], active: false }] }));
  await rendered(app);
  control(app, "Friday, window 1").click();
  await rendered(app);
  assert.deepEqual(JSON.parse(last(server)).windows[0].days, []);
  assert.equal(text(alert(app)), `Refused (windows): ${none}`);
  assert.equal(pressed(control(app, "Friday, window 1")), true);
});

test("a change another client makes appears on the screen, except under a control a person has hold of", async () => {
  const server = fakeServer(house(1, { quiet: [{ ...WEEKNIGHTS, active: false }] }));
  const app = await open(server);
  const limit = control(app, "Volume limit");
  const item = items(app)[0];
  limit.focus();
  limit.value = "300";
  limit.dispatchEvent(new Event("input", { bubbles: true }));
  server.send(
    house(2, {
      limit: 0.5,
      effective_limit: 0.2,
      volume: 0.2,
      quiet: [{ ...WEEKNIGHTS, start: "20:00", limit: 0.2, active: true }],
      quiet_enabled: true,
    }),
  );
  await rendered(app);
  assert.equal(limit.value, "300", "the slider stays under the finger");
  assert.equal(control(app, "Start of window 1").value, "20:00");
  assert.equal(control(app, "Limit of window 1").value, "200");
  assert.equal(value(app, "effective"), "20%");
  assert.equal(text(item.querySelector('[data-value="active"]')), "Active now");
  assert.equal(items(app)[0], item, "the screen was patched, not painted again");
  limit.blur();
  await rendered(app);
  assert.equal(limit.value, "500");
  assert.equal(value(app, "limit"), "50%");
  assert.deepEqual(server.commands, [], "following the server sends nothing");
});

test("before the first state the screen says it is reading, and a room the server does not have is said so", async () => {
  const server = fakeServer(null);
  const app = await open(server, "attic");
  const words = () => text(screenOf(app).shadowRoot.querySelector("[data-missing]"));
  assert.equal(words(), "Reading this server's rooms.");
  server.send(house(1));
  await rendered(app);
  assert.equal(words(), 'This server has no room "attic".');
  assert.equal(getByLabel(app, "Back to rooms").localName, "a");
});

test("a state that carries no limits is said to be unavailable, not shown as a default", async () => {
  const app = await open(fakeServer(stateOf(1, [zone("bedroom", { name: NAME })])));
  assert.deepEqual(queryAllByLabel(app, `Volume limit for ${NAME}`), []);
  assert.equal(value(app, "limit"), "Unavailable");
  assert.equal(value(app, "effective"), "Unavailable");
  assert.equal(control(app, "Quiet hours").disabled, true);
  assert.equal(value(app, "enabled"), "Unavailable");
});

test("its styles name tokens, never a literal colour or length", () => {
  const sheet = customElements.get("chorus-room-limits").styles.cssText;
  assert.match(sheet, /min-height: var\(--control-size\)/);
  assert.doesNotMatch(sheet, /#[0-9a-f]{3,8}\b|\b\d+(px|rem|em)\b/i);
});
