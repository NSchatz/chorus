// Moving a room (src/grouping.js): the one command each move is, and the
// moves that are none. The drag, the room's list and a group's "Remove"
// button all go through this.

import assert from "node:assert/strict";
import { test } from "node:test";

import { groupOfRoom, moveCommand, placeOf, placeOfValue, placeValue, placesFor } from "../src/grouping.js";

// A house: kitchen and den alone; study and bedroom in a live group; a saved
// group of kitchen and living that only living plays in now.
const room = (id, group = id) => ({ id, name: id, group });
const rooms = [room("kitchen"), room("den"), room("study", "live-1"), room("bedroom", "live-1"), room("living", "downstairs")];
const member = (id) => ({ id, name: id });
const groups = [
  {
    id: "downstairs",
    name: "Downstairs",
    kind: "saved",
    active: false,
    defined: [member("kitchen"), member("living")],
    rooms: [member("living")],
    volume: 500,
  },
  {
    id: "live-1",
    name: "study + bedroom",
    kind: "live",
    active: null,
    defined: null,
    rooms: [member("study"), member("bedroom")],
    volume: 500,
  },
];
const [kitchen, den, study, , living] = rooms;

test("a room dropped on a room that is alone joins it", () => {
  assert.equal(moveCommand(kitchen, { kind: "room", id: "den" }, groups), '{"v":2,"t":"join","zone":"kitchen","target":"den"}');
});

test("a room dropped on a group, or on a room of a group, joins that group", () => {
  assert.equal(
    moveCommand(kitchen, { kind: "group", id: "live-1" }, groups),
    '{"v":2,"t":"join","zone":"kitchen","target":"live-1"}',
  );
  assert.equal(
    moveCommand(kitchen, { kind: "room", id: "study" }, groups),
    '{"v":2,"t":"join","zone":"kitchen","target":"study"}',
  );
  // A saved group is a target whether or not it is formed.
  assert.equal(
    moveCommand(den, { kind: "group", id: "downstairs" }, groups),
    '{"v":2,"t":"join","zone":"den","target":"downstairs"}',
  );
  // And a room in one group moves to another.
  assert.equal(
    moveCommand(study, { kind: "group", id: "downstairs" }, groups),
    '{"v":2,"t":"join","zone":"study","target":"downstairs"}',
  );
});

test("a room sent to play alone is taken out of its group", () => {
  assert.equal(moveCommand(study, { kind: "alone" }, groups), '{"v":2,"t":"take","target":"study"}');
  assert.equal(moveCommand(living, { kind: "alone" }, groups), '{"v":2,"t":"take","target":"living"}');
});

test("a move to where the room already is issues no command", () => {
  assert.equal(moveCommand(kitchen, { kind: "room", id: "kitchen" }, groups), null, "onto itself");
  assert.equal(moveCommand(kitchen, { kind: "alone" }, groups), null, "alone when alone");
  assert.equal(moveCommand(study, { kind: "group", id: "live-1" }, groups), null, "onto its own group");
  assert.equal(moveCommand(study, { kind: "room", id: "bedroom" }, groups), null, "onto a room it plays with");
  assert.equal(moveCommand(study, null, groups), null, "no destination");
  assert.equal(moveCommand(null, { kind: "alone" }, groups), null, "no such room");
  assert.equal(moveCommand(kitchen, { kind: "group" }, groups), null, "a group with no id");
});

test("where a room is and where it can go are read from the server's groups", () => {
  assert.equal(groupOfRoom(kitchen, groups), null);
  assert.equal(groupOfRoom(study, groups).id, "live-1");
  assert.equal(placeOf(kitchen, groups), "alone");
  assert.equal(placeOf(study, groups), "group:live-1");
  assert.deepEqual(placesFor(kitchen, rooms, groups), [
    { value: "alone", label: "Alone" },
    { value: "group:downstairs", label: "Downstairs" },
    { value: "group:live-1", label: "study + bedroom" },
    { value: "room:den", label: "With den" },
  ]);
  // An id with a colon in it survives the round trip.
  for (const destination of [{ kind: "alone" }, { kind: "group", id: "live-1" }, { kind: "room", id: "a:b" }]) {
    assert.deepEqual(placeOfValue(placeValue(destination)), destination);
  }
  assert.equal(placeOfValue("nonsense"), null);
  assert.equal(placeOfValue("speaker:x"), null);
});
