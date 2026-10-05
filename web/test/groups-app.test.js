// Grouping through the whole app (src/chorus-app.js over a scripted server):
// the drag gesture on Pointer Events, the path with no drag to the same
// commands, and the group slider against the documented K77 rule, with the
// room levels the page shows held to the server's state.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { afterEach, test } from "node:test";

import { createClient } from "../src/api.js";
import "../src/chorus-app.js";
import { DRAG_THRESHOLD, destinationOf } from "../src/drag.js";
import { createStore } from "../src/state.js";
import { afterGroupVolume, fakeServer, fakeTimers, settle, stateOf, zone } from "./fake-server.js";
import { getByLabel, queryAllByLabel } from "./label-query.js";

let stores = [];
const realElementFromPoint = document.elementFromPoint;

afterEach(() => {
  for (const store of stores) store.stop();
  stores = [];
  document.body.replaceChildren();
  document.elementFromPoint = realElementFromPoint;
});

// A house with no group formed and one saved definition: three rooms alone.
const alone = (serial = 1) =>
  stateOf(serial, [zone("kitchen", { name: "The Kitchen" }), zone("den"), zone("study")], {
    groups: ["kitchen", "den", "study"].map((id) => ({ id, kind: "room", zones: [id], volume: 0.5, source: "stream" })),
    saved_groups: [{ id: "downstairs", name: "Downstairs", zones: ["kitchen", "den"], active: false }],
  });

// The same house after the kitchen joined the den: a live group of the two.
const joined = (serial = 2) =>
  stateOf(
    serial,
    [zone("kitchen", { name: "The Kitchen", group: "live-1" }), zone("den", { group: "live-1" }), zone("study")],
    {
      groups: [
        { id: "live-1", kind: "live", zones: ["kitchen", "den"], volume: 0.5, source: "stream" },
        { id: "study", kind: "room", zones: ["study"], volume: 0.5, source: "stream" },
      ],
      saved_groups: [{ id: "downstairs", name: "Downstairs", zones: ["kitchen", "den"], active: false }],
    },
  );

// The catalog's own state vector: a saved group with a room held by a limit.
const rich = () =>
  JSON.parse(readFileSync(new URL("../../fixtures/control/v2/state-rich.json", import.meta.url), "utf8"));

async function mountOver(server) {
  const store = createStore(createClient({ fetch: server.fetch, base: server.base, timers: fakeTimers() }));
  stores.push(store);
  const app = document.createElement("chorus-app");
  app.store = store;
  document.body.append(app);
  store.start();
  await rendered(app);
  return app;
}

async function rendered(app) {
  await settle();
  await app.updateComplete;
  for (const name of ["chorus-groups", "chorus-rooms"]) {
    const screen = app.shadowRoot.querySelector(name);
    await screen.updateComplete;
    await Promise.all([...screen.shadowRoot.querySelectorAll("*")].map((element) => element.updateComplete));
  }
}

const rooms = (app) => app.shadowRoot.querySelector("chorus-rooms").shadowRoot;
const groups = (app) => app.shadowRoot.querySelector("chorus-groups").shadowRoot;
const roomItem = (app, id) => rooms(app).querySelector(`li[data-room="${id}"]`);
const roomCard = (app, id) => roomItem(app, id).querySelector("chorus-room-card");
const groupItem = (app, id) => groups(app).querySelector(`li[data-group="${id}"]`);
const text = (node) => node.textContent.replace(/\s+/g, " ").trim();
const dragStatus = (app) => text(app.shadowRoot.querySelector("[data-drag]"));

// One pointer, as a browser reports it: happy-dom lays nothing out, so what
// is "under" the pointer is said by the test, and found by the app's own
// code (drag.js) through document.elementFromPoint.
function pointer(app) {
  let under = null;
  document.elementFromPoint = () => under;
  const fire = (type, target, x, y, more = {}) => {
    const event = new PointerEvent(type, {
      bubbles: true,
      composed: true,
      cancelable: true,
      pointerId: 7,
      pointerType: "touch",
      isPrimary: true,
      button: 0,
      clientX: x,
      clientY: y,
      ...more,
    });
    target.dispatchEvent(event);
    return event;
  };
  let handle = null;
  return {
    // Press the handle of a room's card.
    down(roomName) {
      handle = getByLabel(app, `Move ${roomName}`);
      fire("pointerdown", handle, 10, 10);
    },
    // Move to (x, y), where `element` is what lies under the pointer.
    move(element, x = 10, y = 10 + DRAG_THRESHOLD * 4) {
      under = element;
      return fire("pointermove", handle, x, y);
    },
    up(element, x = 10, y = 10 + DRAG_THRESHOLD * 4) {
      under = element;
      return fire("pointerup", handle, x, y);
    },
    cancel() {
      return fire("pointercancel", handle, 0, 0);
    },
    handle: () => handle,
  };
}

test("a room dragged onto another room joins it, and the page shows the group the server formed", async () => {
  const server = fakeServer(alone());
  server.answer = () => ({ status: 200, body: JSON.stringify(joined()) });
  const app = await mountOver(server);
  const finger = pointer(app);
  // The target is the innermost thing under the finger: the den's heading.
  const target = roomCard(app, "den").shadowRoot.querySelector("h2");

  finger.down("The Kitchen");
  assert.equal(dragStatus(app), "", "a press is not a drag yet");
  finger.move(target, 10, 10 + DRAG_THRESHOLD - 1);
  await rendered(app);
  assert.equal(dragStatus(app), "", "nor is a move shorter than the threshold");
  const moved = finger.move(target);
  await rendered(app);
  assert.equal(moved.defaultPrevented, true, "a drag's move is the app's");
  assert.equal(dragStatus(app), "Moving The Kitchen. Drop it on a room or a group.");
  assert.equal(roomItem(app, "kitchen").hasAttribute("data-moving"), true);
  assert.equal(roomItem(app, "den").hasAttribute("data-over"), true, "the room under the finger is marked");
  assert.deepEqual(server.commands, [], "nothing is asked for before the drop");

  finger.up(target);
  // The click a browser sends right after the release is not a press of the handle.
  finger.handle().click();
  assert.notEqual(roomCard(app, "kitchen").shadowRoot.activeElement, getByLabel(app, "Group for The Kitchen"));
  await rendered(app);
  assert.deepEqual(server.commands, ['{"v":2,"t":"join","zone":"kitchen","target":"den"}']);
  assert.equal(dragStatus(app), "");
  assert.equal(roomItem(app, "den").hasAttribute("data-over"), false);
  // The live group is on the page, with its rooms, because the server said so.
  assert.deepEqual(
    [...groupItem(app, "live-1").querySelector("chorus-group-card").shadowRoot.querySelectorAll("li")].map(
      (item) => item.dataset.member,
    ),
    ["kitchen", "den"],
  );
  assert.equal(getByLabel(app, "Group for The Kitchen").value, "group:live-1");
});

test("a room dragged onto a group joins that group, saved or live", async () => {
  const server = fakeServer(joined());
  const app = await mountOver(server);
  const finger = pointer(app);

  finger.down("study");
  finger.move(groupItem(app, "live-1").querySelector("chorus-group-card").shadowRoot.querySelector("h2"));
  await rendered(app);
  assert.equal(groupItem(app, "live-1").hasAttribute("data-over"), true);
  finger.up(groupItem(app, "live-1").querySelector("chorus-group-card").shadowRoot.querySelector("h2"));
  await rendered(app);

  finger.down("study");
  finger.move(groupItem(app, "downstairs"));
  finger.up(groupItem(app, "downstairs"));
  await rendered(app);
  assert.deepEqual(server.commands, [
    '{"v":2,"t":"join","zone":"study","target":"live-1"}',
    '{"v":2,"t":"join","zone":"study","target":"downstairs"}',
  ]);
});

test("a grouped room dragged to the way out leaves its group", async () => {
  const server = fakeServer(joined());
  const app = await mountOver(server);
  const finger = pointer(app);
  const out = groups(app).querySelector('[data-drop="alone"]');
  assert.equal(out.hidden, true);

  finger.down("den");
  finger.move(out);
  await rendered(app);
  assert.equal(out.hidden, false, "the way out is offered while a grouped room is dragged");
  assert.equal(out.hasAttribute("data-over"), true);
  finger.up(out);
  await rendered(app);
  assert.deepEqual(server.commands, ['{"v":2,"t":"take","target":"den"}']);
  assert.equal(out.hidden, true);
});

test("a cancelled drag issues no command", async () => {
  // The kitchen is in a live group here, so any destination a cancelled drag
  // wrongly carried (a room, a group, the way out) would be a command.
  const server = fakeServer(joined());
  const app = await mountOver(server);
  const finger = pointer(app);
  const den = roomItem(app, "study");

  // The browser takes the pointer away (a scroll, a second finger, a call).
  finger.down("The Kitchen");
  finger.move(den);
  await rendered(app);
  assert.equal(dragStatus(app), "Moving The Kitchen. Drop it on a room or a group.");
  finger.cancel();
  await rendered(app);
  assert.equal(dragStatus(app), "");
  assert.equal(den.hasAttribute("data-over"), false);
  // The pointer's later events belong to no drag.
  finger.up(den);

  // Escape during a drag.
  finger.down("The Kitchen");
  finger.move(den);
  document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  finger.up(den);
  await rendered(app);
  assert.equal(dragStatus(app), "");

  // Released over nothing that takes a room, and over the room itself.
  finger.down("The Kitchen");
  finger.move(app);
  finger.up(app);
  finger.down("The Kitchen");
  finger.move(roomItem(app, "kitchen"));
  finger.up(roomItem(app, "kitchen"));
  await rendered(app);

  // A second pointer and a secondary button start nothing.
  const handle = getByLabel(app, "Move The Kitchen");
  for (const more of [{ isPrimary: false }, { button: 2 }]) {
    handle.dispatchEvent(
      new PointerEvent("pointerdown", { bubbles: true, composed: true, pointerId: 9, clientX: 0, clientY: 0, ...more }),
    );
    handle.dispatchEvent(
      new PointerEvent("pointermove", { bubbles: true, composed: true, pointerId: 9, clientX: 0, clientY: 90 }),
    );
  }
  await rendered(app);
  assert.equal(dragStatus(app), "");
  assert.deepEqual(server.commands, []);
});

test("a press on the handle that is not dragged is a click, and goes to the room's list", async () => {
  const server = fakeServer(alone());
  const app = await mountOver(server);
  const finger = pointer(app);
  finger.down("den");
  finger.up(roomItem(app, "kitchen"), 10, 10);
  finger.handle().click();
  await rendered(app);
  assert.deepEqual(server.commands, [], "a press with no travel moved nothing");
  const list = getByLabel(app, "Group for den");
  assert.equal(roomCard(app, "den").shadowRoot.activeElement, list, "the list has the focus");
  assert.equal(list.localName, "select");
});

test("a drop target is found from the innermost element, out through shadow roots", async () => {
  const app = await mountOver(fakeServer(joined()));
  const slider = getByLabel(app, "Volume for den");
  assert.deepEqual(destinationOf(slider), { kind: "room", id: "den" });
  assert.deepEqual(destinationOf(getByLabel(app, "Group volume for The Kitchen + den")), { kind: "group", id: "live-1" });
  assert.deepEqual(destinationOf(groups(app).querySelector('[data-drop="alone"]')), { kind: "alone" });
  assert.equal(destinationOf(app.shadowRoot.querySelector("h1")), null);
  assert.equal(destinationOf(null), null);
});

test("the room's list is the path with no drag: it says where the room is and issues the same commands", async () => {
  const server = fakeServer(alone());
  server.answer = () => ({ status: 200, body: JSON.stringify(joined()) });
  const app = await mountOver(server);
  const list = getByLabel(app, "Group for The Kitchen");
  assert.equal(list.value, "alone");
  assert.deepEqual(
    [...list.options].map((option) => [option.value, text(option)]),
    [
      ["alone", "Alone"],
      ["group:downstairs", "Downstairs"],
      ["room:den", "With den"],
      ["room:study", "With study"],
    ],
  );

  // Join the den: the same bytes the drop sent.
  list.value = "room:den";
  list.dispatchEvent(new Event("change", { bubbles: true }));
  assert.equal(list.value, "alone", "the list waits for the server: no optimistic value");
  await rendered(app);
  assert.deepEqual(server.commands, ['{"v":2,"t":"join","zone":"kitchen","target":"den"}']);
  assert.equal(list.value, "group:live-1", "and then says what the server said");
  assert.equal(getByLabel(app, "Group for The Kitchen"), list, "the same element");

  // Join a saved group, then leave.
  server.answer = () => ({ status: 200, body: JSON.stringify(joined(3)) });
  const study = getByLabel(app, "Group for study");
  study.value = "group:downstairs";
  study.dispatchEvent(new Event("change", { bubbles: true }));
  list.value = "alone";
  list.dispatchEvent(new Event("change", { bubbles: true }));
  await rendered(app);
  assert.deepEqual(server.commands.slice(1), [
    '{"v":2,"t":"join","zone":"study","target":"downstairs"}',
    '{"v":2,"t":"take","target":"kitchen"}',
  ]);
});

test("a group's buttons are the same path: form a saved group, take a room out", async () => {
  const server = fakeServer(joined());
  const app = await mountOver(server);
  getByLabel(app, "Group the rooms of Downstairs").click();
  getByLabel(app, "Remove den from The Kitchen + den").click();
  await rendered(app);
  assert.deepEqual(server.commands, ['{"v":2,"t":"take","target":"downstairs"}', '{"v":2,"t":"take","target":"den"}']);
});

test("a refused move is shown on the room in the server's words, and a refused group command on the group", async () => {
  const server = fakeServer(joined());
  const refusal = { v: 2, t: "error", field: "target", detail: "'den' cannot join now" };
  server.answer = () => ({ status: 400, body: JSON.stringify(refusal) });
  const app = await mountOver(server);
  const list = getByLabel(app, "Group for study");
  list.value = "group:live-1";
  list.dispatchEvent(new Event("change", { bubbles: true }));
  getByLabel(app, "Group the rooms of Downstairs").click();
  await rendered(app);
  assert.equal(text(roomCard(app, "study").shadowRoot.querySelector("[role=alert]")), "Refused: 'den' cannot join now");
  assert.equal(list.value, "alone", "the list still says where the server has the room");
  assert.equal(
    text(groupItem(app, "downstairs").querySelector("chorus-group-card").shadowRoot.querySelector("[role=alert]")),
    "Refused: 'den' cannot join now",
  );
});

// What the page shows of each room's level, and what the state says, in the
// same terms: the slider's value in thousandths.
const shownLevels = (app, ids) =>
  Object.fromEntries(ids.map((id) => [id, roomCard(app, id).shadowRoot.querySelector("input[type=range]").value]));
const stateLevels = (state, ids) =>
  Object.fromEntries(ids.map((id) => [id, String(Math.round(state.zones.find((room) => room.id === id).volume * 1000))]));

test("the group slider follows the K77 rule as the server applies it: every room scaled, each clamped, nothing worked out here", async () => {
  // The catalog's vector: living 0.857 and kitchen 0.343 (limit 0.400) in the
  // saved group, group volume 0.600. The scripted server answers a
  // group_volume with the documented rule (fake-server.js).
  const server = fakeServer(rich());
  server.answer = (body) => {
    const command = JSON.parse(body);
    assert.equal(command.t, "group_volume");
    server.snapshot = afterGroupVolume(server.snapshot, command.group, Math.round(command.volume * 1000));
    return { status: 200, body: JSON.stringify(server.snapshot) };
  };
  const app = await mountOver(server);
  const ids = ["living", "kitchen", "study", "bedroom"];
  const slider = getByLabel(app, "Group volume for Downstairs");
  const figure = () =>
    text(groupItem(app, "downstairs").querySelector("chorus-group-card").shadowRoot.querySelector("[data-volume]"));
  assert.equal(slider.value, "600");
  assert.deepEqual(shownLevels(app, ids), { living: "857", kitchen: "343", study: "1000", bedroom: "200" });

  const set = async (thousandths) => {
    slider.focus();
    slider.value = String(thousandths);
    slider.dispatchEvent(new Event("input", { bubbles: true }));
    slider.dispatchEvent(new Event("change", { bubbles: true }));
    slider.blur();
    await rendered(app);
  };

  // Down to 0.300: both rooms halve, and their balance is kept (857:343).
  await set(300);
  assert.deepEqual(server.commands, ['{"v":2,"t":"group_volume","group":"downstairs","volume":0.300}']);
  assert.deepEqual(shownLevels(app, ids), { living: "429", kitchen: "172", study: "1000", bedroom: "200" });
  assert.deepEqual(shownLevels(app, ids), stateLevels(server.snapshot, ids), "the page shows the server's levels");
  assert.equal(slider.value, "301", "the group volume shown is the server's average, not the figure asked for");
  assert.equal(figure(), "30%");

  // Up to 0.900: the kitchen stops at its limit and the living room at full
  // scale, the shortfall is not shared out, and the average ends below what
  // was asked. The page says what the server says: 0.700, not 0.900.
  await set(900);
  assert.deepEqual(shownLevels(app, ids), { living: "1000", kitchen: "400", study: "1000", bedroom: "200" });
  assert.deepEqual(shownLevels(app, ids), stateLevels(server.snapshot, ids));
  assert.equal(slider.value, "700");
  assert.equal(figure(), "70%");

  // The other group's rooms were never touched, and its slider is its own.
  assert.equal(getByLabel(app, "Group volume for study + bedroom").value, "600");

  // Each room stays adjustable on its own: the room's slider sends `volume`.
  server.answer = () => ({ status: 200, body: JSON.stringify(server.snapshot) });
  const kitchen = getByLabel(app, "Volume for kitchen");
  kitchen.value = "250";
  kitchen.dispatchEvent(new Event("change", { bubbles: true }));
  await rendered(app);
  assert.equal(server.commands.at(-1), '{"v":1,"t":"volume","zone":"kitchen","volume":0.250}');
});

test("from a group volume of zero every room is set to the volume asked for, as the server says", async () => {
  const silent = rich();
  for (const room of silent.zones) if (room.group === "live-1") room.volume = 0;
  silent.groups[1].volume = 0;
  const server = fakeServer(silent);
  server.answer = (body) => {
    const command = JSON.parse(body);
    server.snapshot = afterGroupVolume(server.snapshot, command.group, Math.round(command.volume * 1000));
    return { status: 200, body: JSON.stringify(server.snapshot) };
  };
  const app = await mountOver(server);
  const slider = getByLabel(app, "Group volume for study + bedroom");
  assert.equal(slider.value, "0");
  slider.value = "150";
  slider.dispatchEvent(new Event("change", { bubbles: true }));
  await rendered(app);
  const ids = ["study", "bedroom"];
  assert.deepEqual(shownLevels(app, ids), { study: "150", bedroom: "150" });
  assert.deepEqual(shownLevels(app, ids), stateLevels(server.snapshot, ids));
  assert.equal(slider.value, "150");
});

test("the app has no group volume of its own: a group's level changes only when the server's state does", async () => {
  const server = fakeServer(rich());
  // A server that takes the command and changes nothing (every room held).
  server.answer = () => ({ status: 200, body: JSON.stringify({ ...rich(), serial: 32 }) });
  const app = await mountOver(server);
  const slider = getByLabel(app, "Group volume for Downstairs");
  slider.value = "100";
  slider.dispatchEvent(new Event("change", { bubbles: true }));
  await rendered(app);
  assert.equal(slider.value, "600");
  assert.deepEqual(shownLevels(app, ["living", "kitchen"]), { living: "857", kitchen: "343" });
  // Another client changes one room: the group's figure is whatever the
  // server then says it is, here a value no average of the page's would give.
  const odd = rich();
  odd.serial = 40;
  odd.zones[1].volume = 0.1;
  odd.groups[0].volume = 0.999;
  server.send(odd);
  await rendered(app);
  assert.equal(slider.value, "999");
  assert.deepEqual(queryAllByLabel(app, "Group volume for Downstairs"), [slider]);
});
