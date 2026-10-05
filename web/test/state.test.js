// The state layer (src/state.js) against a scripted server: the snapshot
// first, then every message of the event stream, each replacing the last;
// nothing the app did is held until the server says it.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { RETRY_MS, createClient, volumeCommand } from "../src/api.js";
import { createStore, groupsOf, roomOf, roomsOf } from "../src/state.js";
import { fakeServer, fakeTimers, settle, stateOf, zone } from "./fake-server.js";

// What a room's limits read as from a state that carries none of them.
const NO_LIMITS = { limit: null, effectiveLimit: null, quietEnabled: null, windows: [] };
// What a room's TV path and bass management read as from a state that carries
// none of them: no theater section is offered, and nothing is made up.
// What a room's correction reads as from a state that carries none.
const NO_CORRECTION = { enabled: null, filters: [], undo: false };
const NO_THEATER = {
  offered: false,
  avTrimMs: null,
  tvUpmix: null,
  tvInputs: [],
  set: false,
  surrounds: false,
  bass: { crossoverHz: null, subLevel: null, subPolarity: null, active: false },
};

function storeOf(server, timers = fakeTimers()) {
  return createStore(createClient({ fetch: server.fetch, base: server.base, timers }));
}

test("a room is read as its name, volume in thousandths, mute and bonded set", () => {
  const state = stateOf(
    3,
    [
      zone("living", {
        name: "Living Room",
        volume: 0.375,
        muted: true,
        sound: { bass: 3, treble: -2, loudness: true, night: false, speech: true, tv_upmix: "ambient" },
        bond: [
          { endpoint: "chorus-0123456789ab", role: "FL" },
          { endpoint: "endpoint-b", role: "FR" },
        ],
      }),
      zone("den", { name: "" }),
      { name: "a room with no id is not a room" },
    ],
    { speakers: [{ id: "chorus-0123456789ab", name: "Left of the TV", named: true, room: "living" }] },
  );
  assert.deepEqual(roomsOf(state), [
    {
      id: "living",
      name: "Living Room",
      volume: 375,
      muted: true,
      sound: { bass: 3, treble: -2, loudness: true, night: false, speech: true },
      limits: NO_LIMITS,
      // A pair is not a theater set; the upmix is the server's word.
      theater: { ...NO_THEATER, tvUpmix: "ambient" },
      correction: NO_CORRECTION,
      group: "living",
      bond: [
        { endpoint: "chorus-0123456789ab", name: "Left of the TV", role: "FL" },
        { endpoint: "endpoint-b", name: "endpoint-b", role: "FR" },
      ],
      source: null,
      nowPlaying: null,
    },
    {
      id: "den",
      name: "den",
      volume: 500,
      muted: false,
      // A state that carries no sound for the room: nothing is made up.
      sound: { bass: null, treble: null, loudness: null, night: null, speech: null },
      limits: NO_LIMITS,
      theater: NO_THEATER,
    correction: NO_CORRECTION,
      correction: NO_CORRECTION,
      group: "den",
      bond: [],
      source: null,
      nowPlaying: null,
    },
  ]);
});

test("a member the app cannot read is null, not a made-up value", () => {
  const [room] = roomsOf(stateOf(1, [{ id: "kitchen", volume: "loud" }]));
  assert.deepEqual(room, {
    id: "kitchen",
    name: "kitchen",
    volume: null,
    muted: null,
    sound: { bass: null, treble: null, loudness: null, night: null, speech: null },
    limits: NO_LIMITS,
    theater: NO_THEATER,
    correction: NO_CORRECTION,
    group: "kitchen",
    bond: [],
    source: null,
    nowPlaying: null,
  });
  // A sound whose members are not what the catalog says is read the same way.
  const [odd] = roomsOf(stateOf(1, [zone("den", { sound: { bass: 2.5, treble: "3", loudness: 1, night: true } })]));
  assert.deepEqual(odd.sound, { bass: null, treble: null, loudness: null, night: true, speech: null });
  assert.deepEqual(roomOf(roomsOf(stateOf(1, [zone("den")])), "den").id, "den");
  assert.equal(roomOf(roomsOf(stateOf(1, [zone("den")])), "attic"), null);
  assert.deepEqual(roomsOf(null), []);
  assert.deepEqual(roomsOf({ zones: "none" }), []);
});

test("the store holds the snapshot, then each event-stream update in turn", async () => {
  const server = fakeServer(stateOf(5, [zone("kitchen", { volume: 0.25 })]));
  const store = storeOf(server);
  const views = [];
  store.subscribe((view) => views.push({ status: view.status, serial: view.state?.serial ?? null, rooms: view.rooms }));
  assert.deepEqual(views, [{ status: "connecting", serial: null, rooms: [] }]);

  store.start();
  await settle();
  // The snapshot: something to show before the stream has said anything.
  assert.equal(views.at(-1).serial, 5);
  assert.equal(views.at(-1).rooms[0].volume, 250);

  server.send(stateOf(5, [zone("kitchen", { volume: 0.25 })]));
  await settle();
  assert.equal(views.at(-1).status, "live");

  // Another client changes the room: the update arrives and replaces the state.
  server.send(stateOf(6, [zone("kitchen", { volume: 0.75, muted: true })]));
  await settle();
  assert.equal(views.at(-1).serial, 6);
  assert.deepEqual(
    { volume: views.at(-1).rooms[0].volume, muted: views.at(-1).rooms[0].muted },
    { volume: 750, muted: true },
  );

  // A room added to the server's state is a room of the store.
  server.send(stateOf(7, [zone("kitchen"), zone("den")]));
  await settle();
  assert.deepEqual(views.at(-1).rooms.map((room) => room.id), ["kitchen", "den"]);
  store.stop();
});

test("a snapshot that answers after the stream has delivered is not applied", async () => {
  const server = fakeServer(stateOf(2, [zone("kitchen")]));
  const inner = server.fetch;
  let release;
  const held = new Promise((resolve) => (release = resolve));
  server.fetch = async (url, options) => {
    if (String(url).endsWith("api/state")) await held;
    return inner(url, options);
  };
  const store = storeOf(server);
  store.start();
  await settle();
  server.send(stateOf(9, [zone("kitchen", { volume: 1 })]));
  await settle();
  assert.equal(store.view().state.serial, 9);
  release();
  await settle();
  assert.equal(store.view().state.serial, 9);
  assert.equal(store.view().rooms[0].volume, 1000);
  store.stop();
});

test("with no snapshot the event stream alone fills the store", async () => {
  const server = fakeServer(null);
  const store = storeOf(server);
  store.start();
  await settle();
  assert.equal(store.view().state, null);
  server.send(stateOf(1, [zone("den")]));
  await settle();
  assert.deepEqual(store.view().rooms.map((room) => room.id), ["den"]);
  assert.equal(store.view().status, "live");
  store.stop();
});

test("a dropped stream leaves the last state and says it is lost, then recovers", async () => {
  const server = fakeServer(stateOf(1, [zone("kitchen")]));
  const timers = fakeTimers();
  const store = storeOf(server, timers);
  store.start();
  await settle();
  server.send(stateOf(1, [zone("kitchen")]));
  await settle();
  server.drop();
  await settle();
  assert.equal(store.view().status, "lost");
  assert.equal(store.view().rooms.length, 1);

  timers.fire(RETRY_MS);
  await settle();
  server.send(stateOf(4, [zone("kitchen", { muted: true })]));
  await settle();
  assert.equal(store.view().status, "live");
  assert.equal(store.view().rooms[0].muted, true);
  store.stop();
});

test("a command changes the store only through what the server answers", async () => {
  const server = fakeServer(stateOf(1, [zone("kitchen", { volume: 0.5 })]));
  const store = storeOf(server);
  store.start();
  await settle();

  // Refused: nothing changes, and the refusal comes back to the caller.
  server.answer = () => ({
    status: 400,
    body: '{"v":1,"t":"error","field":"zone","detail":"there is no zone \'attic\'"}',
  });
  assert.deepEqual(await store.command(volumeCommand("attic", 100)), { ok: false, refusal: "there is no zone 'attic'", field: "zone" });
  assert.equal(store.view().state.serial, 1);

  // Accepted, and clamped by the server: the store holds the server's volume,
  // not the one that was asked for.
  server.answer = () => ({ status: 200, body: JSON.stringify(stateOf(2, [zone("kitchen", { volume: 0.6 })])) });
  const result = await store.command(volumeCommand("kitchen", 900));
  assert.equal(result.ok, true);
  assert.equal(store.view().rooms[0].volume, 600);

  // An answer older than what the stream has delivered moves nothing back.
  server.send(stateOf(8, [zone("kitchen", { volume: 0.1 })]));
  await settle();
  server.answer = () => ({ status: 200, body: JSON.stringify(stateOf(7, [zone("kitchen", { volume: 0.9 })])) });
  await store.command(volumeCommand("kitchen", 900));
  assert.equal(store.view().rooms[0].volume, 100);
  store.stop();
});

// The catalog's own state vector: a saved group that is active and a live one.
const rich = () =>
  JSON.parse(readFileSync(new URL("../../fixtures/control/v2/state-rich.json", import.meta.url), "utf8"));

test("the groups are every saved group, then every live group, each with its rooms and the server's group volume", () => {
  assert.deepEqual(groupsOf(rich()), [
    {
      id: "downstairs",
      name: "Downstairs",
      kind: "saved",
      active: true,
      defined: [
        { id: "living", name: "Living Room" },
        { id: "kitchen", name: "kitchen" },
      ],
      rooms: [
        { id: "living", name: "Living Room" },
        { id: "kitchen", name: "kitchen" },
      ],
      volume: 600,      source: "line-in:endpoint-c/line-1",
      nowPlaying: null,
    },
    {
      id: "live-1",
      name: "study + bedroom",
      kind: "live",
      active: null,
      defined: null,
      rooms: [
        { id: "study", name: "study" },
        { id: "bedroom", name: "bedroom" },
      ],
      volume: 600,      source: "stream",
      nowPlaying: null,
    },
  ]);
  assert.deepEqual(roomsOf(rich()).map((room) => room.group), ["downstairs", "downstairs", "live-1", "live-1"]);
});

test("a saved group no room is in is listed all the same, with no rooms playing and no volume (K59)", () => {
  const state = stateOf(1, [zone("kitchen"), zone("den")], {
    groups: [
      { id: "kitchen", kind: "room", zones: ["kitchen"], volume: 0.5, source: "stream" },
      { id: "den", kind: "room", zones: ["den"], volume: 0.5, source: "stream" },
    ],
    saved_groups: [{ id: "downstairs", name: "Downstairs", zones: ["kitchen", "den"], active: false }],
  });
  assert.deepEqual(groupsOf(state), [
    {
      id: "downstairs",
      name: "Downstairs",
      kind: "saved",
      active: false,
      defined: [
        { id: "kitchen", name: "kitchen" },
        { id: "den", name: "den" },
      ],
      rooms: [],
      volume: null,      source: null,
      nowPlaying: null,
    },
  ]);
  // A room alone in the group named for it is a room, not a group.
  assert.deepEqual(groupsOf(stateOf(1, [zone("kitchen")], { groups: state.groups })), []);
  assert.deepEqual(groupsOf(null), []);
});

test("the store's view carries the groups of the state it holds", async () => {
  const server = fakeServer(rich());
  const store = storeOf(server);
  store.start();
  await settle();
  assert.deepEqual(store.view().groups.map((group) => group.id), ["downstairs", "live-1"]);
  store.stop();
});
