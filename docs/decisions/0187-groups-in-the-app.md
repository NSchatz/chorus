# 0187: the app lists every saved and live group, moves a room with a Pointer Events drag or the room's own list, and shows the server's group volume without working one out

- Status: accepted, 2026-10-05.
- Decided by: the owner for what is asked (K59: a saved group is always visible; K77: the
  group slider scales every room relatively and each room stays adjustable; K78: take the
  room; proposal P5, `docs/proposals/P5-app-stack.md`: drag-to-group is built on Pointer
  Events). Which command each move is, the path with no drag, where the gesture lives and
  when the group slider sends are this record's: cheap to reverse, each in one module.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `web/src/state.js` (`groupsOf`), `web/src/api.js` (`joinCommand`,
  `takeCommand`, `groupVolumeCommand`), `web/src/grouping.js`, `web/src/drag.js`,
  `web/src/groups.js`, `web/src/group-card.js`, `web/src/room-card.js`, `web/src/rooms.js`,
  `web/src/chorus-app.js`; `web/test/` (`grouping.test.js`, `group-card.test.js`,
  `groups-app.test.js` and the K77 model in `fake-server.js`); `web/live/groups.live.js` and
  `web/live/house.js`.

## Context

The control plane has had saved and live groups, `join`, `take` and Sonos-style group volume
since catalog version 2 (`docs/control-plane.md`). The app showed rooms only. Record 0185 set
the rule every screen follows: the server owns the state, the app shows its last state
message and keeps no value of its own.

## Decisions

1. **The groups a person sees are read from the state message and nothing else.**
   `groupsOf` lists every entry of `saved_groups` (active or not, K59), then every formed
   group of kind `live`. A room alone in the group named for it is a room, not a group. A
   live group has no name on the server, so it is called by its rooms' names.

2. **A move is a room and a destination, and one function turns it into its command.**
   `grouping.js` `moveCommand`: onto a room or a group is `join` with that target; out of a
   group is `take` on the room itself, which puts it in the group named for it and lets the
   server dissolve a live group left with one room. v1's `ungroup` is not used because it
   dissolves nothing (`docs/control-plane.md`, "The state a server holds"). A move to where
   the room already is issues no command. The drag, the room's list and a group's "Remove"
   button all go through it, so they cannot disagree.

3. **The drag is Pointer Events, with no HTML drag and drop.** `drag.js`: a press on a room's
   handle becomes a drag after 8 CSS pixels of travel; the handle captures the pointer and
   has `touch-action: none`, so a finger on it drags instead of scrolling; the target is
   found with `elementFromPoint` through shadow roots, and is the nearest element marked
   `data-drop`. `pointercancel`, a lost capture, Escape and a release over no target issue no
   command. The gesture lives in `chorus-app`, the one element above both the rooms and the
   groups. During a drag the room and the target under the pointer are marked and a status
   line says what is moving; no floating copy of the card follows the pointer, because that
   needs a position written per frame and the page's policy allows no inline style.

4. **The path with no drag is a list on each room's card** ("Plays with": alone, each saved
   and live group, each other room that is alone). It is a native `select`, so a keyboard, a
   switch and a screen reader have it for nothing; it says where the server has the room and
   goes back to that the moment a choice is made, following only when the state comes back.
   Pressing the handle without dragging it moves the focus to the list. A group's card also
   has "Remove" for each room and, for a saved group that is not active, "Group these
   rooms" (`take` on the saved group).

5. **The app works out no group volume.** A group's figure is `groups[].volume` of the state
   message. The slider sends `group_volume` with the figure asked for, and the server scales,
   clamps and reports; the group's card and the room cards then show what it reported. It
   sends when the slider is let go, not while it moves: the server scales from the volumes as
   they stand, so a run of commands in one gesture would round the balance away, and passing
   through zero would lose it.

6. **Refusals are shown where the command came from.** The app keeps the server's words by
   id; a room id and a group id never collide (`docs/control-plane.md`: a saved group's id is
   one no room has, and `live-<n>` names no room).

## Consequences

- Creating, renaming and deleting a saved group's definition (`group_save`, `group_delete`)
  are not in the app yet: they need a form of their own and are left for a later change.
- A drag shows its target but no copy of the card under the finger (decision 3).
- `make web-live` now runs two files; `web/live/house.js` holds what they share.
- The app has no dependency it did not have: the stack of record 0181 is unchanged.

## What was read

- `docs/control-plane.md` (group kinds, saved and live groups, dissolving; the commands
  catalog version 2 adds; "Take the room (K78)"; "Group volume (K77, Sonos-style)"; the state
  message).
- `docs/proposals/P5-app-stack.md` ("Drag-to-group on phones and tablets") and
  `docs/decisions/0185-the-apps-state-layer-and-its-live-test.md`.
- `fixtures/control/v2/` (`join.json`, `take.json`, `group_volume.json`, `group_save.json`,
  `state-rich.json`).
- No source outside this repository was opened.
