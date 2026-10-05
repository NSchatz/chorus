// The live test of grouping (`make web-live`, gate step `web-live`): the
// app's own elements and state layer, in node under happy-dom with no
// browser, against a real chorus-server.
//
// Everything is done through the app's controls, found by their labels, and
// held to the server's own `GET /api/state`: a room dragged onto another
// (Pointer Events) forms a live group, the group slider moves every room and
// keeps their ratio as the server reports it, a room taken out dissolves a
// two-room live group, and forming a saved group fills it. After each change
// the levels the page shows are compared with the state's, room by room and
// group by group: the app has no group volume of its own to disagree.

import assert from "node:assert/strict";
import { after, before, test } from "node:test";

import { createClient } from "../src/api.js";
import "../src/chorus-app.js";
import { createStore } from "../src/state.js";
import { getByLabel, queryAllByLabel } from "../test/label-query.js";
import { startHouse, until } from "./house.js";

// The house: three rooms alone, at volumes with a ratio worth keeping, and
// one saved group no room is in yet.
const ROOMS = ["living", "den", "study"];
const SAVED = { id: "evening", name: "Live Test Evening", zones: ["den", "study"] };

let house;
let store;
let app;

const rooms = () => app.shadowRoot.querySelector("chorus-rooms").shadowRoot;
const groups = () => app.shadowRoot.querySelector("chorus-groups").shadowRoot;
const roomItem = (id) => rooms().querySelector(`li[data-room="${id}"]`);
const groupCards = () => [...groups().querySelectorAll("chorus-group-card")];
const text = (node) => node.textContent.replace(/\s+/g, " ").trim();

// The server's state in the terms the page shows it: each room's group and
// level, and each saved or live group's rooms (as a set: a card lists a
// saved group's own rooms first) and level, in thousandths.
const thousandths = (volume) => Math.round(volume * 1000);
function levelsOf(state) {
  return {
    rooms: Object.fromEntries(state.zones.map((room) => [room.id, String(thousandths(room.volume))])),
    groups: Object.fromEntries(
      state.groups
        .filter((group) => group.kind !== "room")
        .map((group) => [group.id, { rooms: [...group.zones].sort(), volume: String(thousandths(group.volume)) }]),
    ),
  };
}

// The same, read off the rendered page.
function shownLevels() {
  return {
    rooms: Object.fromEntries(
      [...rooms().querySelectorAll("chorus-room-card")].map((card) => [
        card.room.id,
        card.shadowRoot.querySelector("input[type=range]")?.value ?? null,
      ]),
    ),
    groups: Object.fromEntries(
      groupCards()
        .filter((card) => card.shadowRoot.querySelector('li[data-playing="true"]'))
        .map((card) => [
          card.group.id,
          {
            rooms: [...card.shadowRoot.querySelectorAll('li[data-playing="true"]')]
              .map((item) => item.dataset.member)
              .sort(),
            volume: card.shadowRoot.querySelector("input[type=range]")?.value ?? null,
          },
        ]),
    ),
  };
}

// The page shows the server's levels: every room's and every group's.
async function pageMatchesServer(what) {
  const state = await house.state();
  await until(`${what}: the levels the page shows`, shownLevels, levelsOf(state));
  return state;
}

const formed = (state) => state.groups.filter((group) => group.kind !== "room");
const zoneOf = (state, id) => state.zones.find((room) => room.id === id);

// Drag a room's handle onto an element with Pointer Events. happy-dom lays
// nothing out, so the test says what is under the pointer; the app finds its
// drop target from that with its own code.
function drag(roomName, onto) {
  const handle = getByLabel(app, `Move ${roomName}`);
  const real = document.elementFromPoint;
  document.elementFromPoint = () => onto;
  try {
    const at = (type, y) =>
      handle.dispatchEvent(
        new PointerEvent(type, {
          bubbles: true,
          composed: true,
          cancelable: true,
          pointerId: 1,
          pointerType: "touch",
          isPrimary: true,
          button: 0,
          clientX: 20,
          clientY: y,
        }),
      );
    at("pointerdown", 20);
    at("pointermove", 60);
    at("pointermove", 120);
    at("pointerup", 120);
  } finally {
    document.elementFromPoint = real;
  }
}

before(async () => {
  assert.ok(process.env.CHORUS_SERVER_BIN, "CHORUS_SERVER_BIN names a built chorus-server");
  house = await startHouse(ROOMS);
  await house.command('{"v":1,"t":"volume","zone":"living","volume":0.300}');
  await house.command('{"v":1,"t":"volume","zone":"den","volume":0.600}');
  await house.command('{"v":1,"t":"volume","zone":"study","volume":0.500}');
  await house.command(JSON.stringify({ v: 2, t: "group_save", group: SAVED.id, name: SAVED.name, zones: SAVED.zones }));

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

test("the saved group is listed while no room is in it, and no live group is", async () => {
  await until("the group cards", () => groupCards().map((card) => card.group.id), [SAVED.id]);
  const [card] = groupCards();
  await until("the saved group's line", () => text(card.shadowRoot.querySelector("[data-kind]") ?? card), "Saved group, not active");
  assert.equal(card.shadowRoot.querySelector("h2").textContent, SAVED.name);
  assert.deepEqual(
    [...card.shadowRoot.querySelectorAll("li")].map((item) => [item.dataset.member, item.dataset.playing]),
    [["den", "false"], ["study", "false"]],
  );
  const state = await pageMatchesServer("at the start");
  assert.deepEqual(formed(state), []);
  assert.deepEqual(state.saved_groups, [{ ...SAVED, active: false }]);
  await until("the store's status", () => store.view().status, "live");
});

test("joining two rooms through the app forms a live group in /api/state", async () => {
  // The living room, dragged onto the den's card.
  drag("living", roomItem("den").querySelector("chorus-room-card").shadowRoot.querySelector("h2"));
  await until("the live groups on the server", async () => formed(await house.state()).map((group) => group.kind), ["live"]);
  const state = await pageMatchesServer("after the join");
  const [live] = formed(state);
  assert.deepEqual([...live.zones].sort(), ["den", "living"]);
  assert.equal(zoneOf(state, "living").group, live.id);
  assert.equal(zoneOf(state, "den").group, live.id);
  assert.equal(zoneOf(state, "study").group, "study", "the third room was not touched");
  // The page lists it with its rooms, and each room's list says where it is.
  assert.equal(getByLabel(app, "Group for living").value, `group:${live.id}`);
  assert.equal(getByLabel(app, "Group for den").value, `group:${live.id}`);
  assert.equal(getByLabel(app, "Group for study").value, "alone");
  assert.equal(getByLabel(app, "Group volume for living + den").localName, "input");
});

test("a group volume change moves every room and keeps their ratio as the server reports it", async () => {
  const before = await house.state();
  const [live] = formed(before);
  // 0.300 and 0.600: the group volume is their average.
  assert.equal(live.volume, 0.45);
  const slider = getByLabel(app, "Group volume for living + den");
  assert.equal(slider.value, "450");
  slider.focus();
  slider.value = "150";
  slider.dispatchEvent(new Event("input", { bubbles: true }));
  slider.dispatchEvent(new Event("change", { bubbles: true }));
  slider.blur();
  await until("the group volume on the server", async () => formed(await house.state())[0].volume, 0.15);
  const state = await pageMatchesServer("after the group volume change");
  // Every room moved, by the same factor: one to two before, one to two after.
  assert.equal(zoneOf(state, "living").volume, 0.1);
  assert.equal(zoneOf(state, "den").volume, 0.2);
  assert.equal(
    zoneOf(state, "den").volume / zoneOf(state, "living").volume,
    zoneOf(before, "den").volume / zoneOf(before, "living").volume,
  );
  assert.equal(zoneOf(state, "study").volume, 0.5, "a room outside the group kept its volume");
  assert.equal(getByLabel(app, "Group volume for living + den"), slider, "the slider is the element it was");

  // Each room stays adjustable on its own, and the group's figure is then
  // whatever the server says the group's volume is.
  const den = getByLabel(app, "Volume for den");
  den.focus();
  den.value = "400";
  den.dispatchEvent(new Event("input", { bubbles: true }));
  den.dispatchEvent(new Event("change", { bubbles: true }));
  den.blur();
  await until("the den's volume on the server", async () => zoneOf(await house.state(), "den").volume, 0.4);
  const after = await pageMatchesServer("after one room's own change");
  assert.equal(zoneOf(after, "living").volume, 0.1, "the other room of the group stayed");
  assert.equal(slider.value, String(thousandths(formed(after)[0].volume)));
});

test("leaving dissolves a two-room live group", async () => {
  // The path with no drag: the den's own list, set to play alone.
  const list = getByLabel(app, "Group for den");
  list.value = "alone";
  list.dispatchEvent(new Event("change", { bubbles: true }));
  await until("the formed groups on the server", async () => formed(await house.state()), []);
  const state = await pageMatchesServer("after leaving");
  assert.deepEqual(state.zones.map((room) => [room.id, room.group]), ROOMS.map((id) => [id, id]));
  assert.deepEqual(groupCards().map((card) => card.group.id), [SAVED.id], "the live group's card is gone");
  assert.deepEqual(queryAllByLabel(app, "Group volume for living + den"), []);
  assert.equal(getByLabel(app, "Group for den").value, "alone");
  assert.equal(getByLabel(app, "Group for living").value, "alone");
});

test("activating a saved group fills it", async () => {
  getByLabel(app, `Group the rooms of ${SAVED.name}`).click();
  await until("the saved group on the server", async () => (await house.state()).saved_groups, [{ ...SAVED, active: true }]);
  const state = await pageMatchesServer("after activating the saved group");
  const [group] = formed(state);
  assert.equal(group.id, SAVED.id);
  assert.equal(group.kind, "saved");
  assert.deepEqual([...group.zones].sort(), [...SAVED.zones].sort());
  for (const id of SAVED.zones) assert.equal(zoneOf(state, id).group, SAVED.id);
  const [card] = groupCards();
  await until("the saved group's line", () => text(card.shadowRoot.querySelector("[data-kind]")), "Saved group, active");
  assert.deepEqual(queryAllByLabel(app, `Group the rooms of ${SAVED.name}`), []);

  // A third room dragged onto the group's card extends it.
  drag("living", groups().querySelector(`li[data-group="${SAVED.id}"]`));
  await until("the living room's group on the server", async () => zoneOf(await house.state(), "living").group, SAVED.id);
  await pageMatchesServer("after extending the saved group");
});
