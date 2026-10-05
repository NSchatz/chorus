// The rooms read from the server's state message (src/rooms.js), with the
// request stubbed: no server and no browser.

import assert from "node:assert/strict";
import { test } from "node:test";

import { loadRooms, roomsOf } from "../src/rooms.js";

test("a room is its id and its name, and a room without a name shows its id", () => {
  const state = { v: 2, t: "state", zones: [{ id: "kitchen", name: "The Kitchen" }, { id: "den", name: "" }, { id: "patio" }] };
  assert.deepEqual(roomsOf(state), [
    { id: "kitchen", name: "The Kitchen" },
    { id: "den", name: "den" },
    { id: "patio", name: "patio" },
  ]);
});

test("a message without rooms, or one that is not a state message, gives none", () => {
  assert.deepEqual(roomsOf({ zones: [] }), []);
  assert.deepEqual(roomsOf({}), []);
  assert.deepEqual(roomsOf(null), []);
  assert.deepEqual(roomsOf({ zones: [null, { name: "no id" }] }), []);
});

test("the rooms are read from api/state under the server's root", async () => {
  const asked = [];
  const rooms = await loadRooms(async (url) => {
    asked.push(url);
    return { ok: true, json: async () => ({ zones: [{ id: "den", name: "Den" }] }) };
  });
  assert.deepEqual(asked, ["../api/state"]);
  assert.deepEqual(rooms, [{ id: "den", name: "Den" }]);
});

test("a server that refuses or cannot be reached gives no rooms", async () => {
  assert.deepEqual(await loadRooms(async () => ({ ok: false, json: async () => ({}) })), []);
  assert.deepEqual(
    await loadRooms(async () => {
      throw new TypeError("fetch failed");
    }),
    [],
  );
});
