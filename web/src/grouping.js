// Moving a room between groups, as pure logic: where a room can go, where it
// is, and the one command that takes it there. The drag gesture (drag.js),
// the room's "Plays with" list (room-card.js) and a group's "Remove" button
// (group-card.js) all end here, so they cannot issue different commands for
// the same move.
//
// A destination is { kind, id }:
//   { kind: "room", id }    onto another room: play with it
//   { kind: "group", id }   onto a saved or live group: play in it
//   { kind: "alone" }       out of the group, to play alone
//
// The rooms and groups are the state layer's (state.js). Nothing here decides
// what a group is or who is in it: that is read from the server's state, and
// the command's effect is the server's too.

import { joinCommand, takeCommand } from "./api.js";

// The saved or live group the room plays in now, or null when it is alone.
export function groupOfRoom(room, groups) {
  return groups.find((group) => group.id === room.group && group.rooms.some((member) => member.id === room.id)) ?? null;
}

// The control message that moves `room` to `destination`, or null when there
// is nothing to do: a room dropped on itself, on a room it already plays
// with, on the group it is in, or sent alone when it is alone.
export function moveCommand(room, destination, groups) {
  if (!room || !destination) return null;
  const current = groupOfRoom(room, groups);
  if (destination.kind === "alone") {
    // `take` on the room itself: it plays in the group named for it, and the
    // live group it left dissolves on the server when one room remains.
    return current ? takeCommand(room.id) : null;
  }
  if (typeof destination.id !== "string" || !destination.id) return null;
  if (destination.kind === "group") {
    return current && current.id === destination.id ? null : joinCommand(room.id, destination.id);
  }
  if (destination.kind === "room") {
    if (destination.id === room.id) return null;
    if (current && current.rooms.some((member) => member.id === destination.id)) return null;
    return joinCommand(room.id, destination.id);
  }
  return null;
}

// A destination as one string, for an <option>'s value, and back.
export const placeValue = (destination) =>
  destination.kind === "alone" ? "alone" : `${destination.kind}:${destination.id}`;

export function placeOfValue(value) {
  if (value === "alone") return { kind: "alone" };
  const colon = String(value).indexOf(":");
  if (colon < 1) return null;
  const kind = value.slice(0, colon);
  const id = value.slice(colon + 1);
  return (kind === "room" || kind === "group") && id ? { kind, id } : null;
}

// Where the room is now, as an option's value: its group, or alone.
export function placeOf(room, groups) {
  const current = groupOfRoom(room, groups);
  return current ? placeValue({ kind: "group", id: current.id }) : "alone";
}

// Every place the room's list offers, in order: alone, each saved and live
// group, then each other room that is alone (a room in a group is reached
// through its group). Each is { value, label }.
export function placesFor(room, rooms, groups) {
  return [
    { value: "alone", label: "Alone" },
    ...groups.map((group) => ({ value: placeValue({ kind: "group", id: group.id }), label: group.name })),
    ...rooms
      .filter((other) => other.id !== room.id && !groupOfRoom(other, groups))
      .map((other) => ({ value: placeValue({ kind: "room", id: other.id }), label: `With ${other.name}` })),
  ];
}
