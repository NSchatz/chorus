# 0000: quiet hours are switched off and on per room, by a `quiet_hours_enabled` command and a `quiet_enabled` state field, with the windows kept and the flag persisted in state-file format 7

- Status: accepted, 2026-10-04. Extends 0075 (catalog v2) and 0018 (the persisted zone
  state); the clamp rule of 0075 and the runtime of 0076 and 0079 are unchanged.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `crates/control/src/catalog.rs` (`Command::QuietHoursEnabled`),
  `crates/control/src/zones.rs` (`Zone::quiet_enabled`, `Zone::effective_limit`, the state's
  `quiet_enabled`), `crates/control/src/persist.rs` (state-file format 7),
  `fixtures/control/v2/quiet_hours_enabled.{fields,json}`,
  `fixtures/control/v2/state-quiet-disabled.{fields,json}`, the six `state-*.json` vectors
  with a room in them (one field each), `crates/control/tests/catalog_v2.rs`,
  `crates/control/tests/state_file_v2.rs`, `crates/server/tests/schedule_runtime.rs`,
  `docs/control-plane.md`, `docs/mqtt.md`

## Context

K83 asks for a quiet-hours switch among the Home Assistant entities. The catalog had one
quiet-hours command, `quiet_hours`, which replaces a room's windows; the only way to stop them
capping a room was to send `[]`, which forgets them. A switch that is turned off and on has to
get the same windows back, and its position has to be readable from the state and survive a
restart, or a server restarted inside a window would cap a room somebody had just uncapped.

## What was read

All on 2026-10-04. No GPL source was opened; nothing outside this repository was read.

- chorus's own: 0018, 0074, 0075, 0076, 0079, 0081 (how `room_eq`'s `enabled` is carried),
  `docs/control-plane.md` ("The commands catalog version 2 adds", "The state message", "The
  schedule runtime", "What survives a restart"), `docs/mqtt.md`, K81 and K83 of the program's
  pinned goal file, `crates/control/src/{catalog,zones,persist,rooms}.rs`,
  `crates/server/src/schedule_runtime.rs` and its tests.

## Decision

**The shape is per room.** A room has one flag, `quiet_enabled`, on by default. While it is
off, `Zone::effective_limit` is the room's `limit` and no window enters it. That one function
is what every volume path, the runtime's ramps and the gain-and-limit reports to the endpoints
already read, so nothing else changes: the runtime needs no new case, and "no volume path can
exceed the effective limit" keeps holding with the effective limit redefined.

Why per room and not per window:

- The thing to drive is one switch per room. A per-window flag makes a switch either one
  entity per window (entities that appear and vanish as windows are edited, named by an index
  that shifts when a window is removed) or a client that rewrites every window to flip them
  together, which is a read-modify-write race with anyone editing the windows.
- Windows have no identity in the catalog: `quiet_hours` replaces the list. A per-window flag
  would have to say what replacing the list does to it, and every answer loses a person's
  setting in some order of events. The per-room flag is untouched by `quiet_hours`.
- A window somebody wants off by itself can already be removed and sent again; the per-room
  flag is the case with no spelling today.

**The command is its own, `quiet_hours_enabled` (`zone`, `enabled`), both required.** Not an
optional `enabled` on `quiet_hours`: that command's `windows` would have to become optional
too (a switch must not need to know the windows), which changes the decoding of a command
that already has vectors and callers, for no gain over a second command with two required
fields. A switch's two actions are then two fixed messages. It is a catalog version 2 command;
at version 1 it is refused as any v2-only command is.

**Switching off raises nothing; switching on inside a window clamps at once.** The same rule
as a window ending and a window starting. The room's volume was pulled down when the window
began and is the volume now; turning the cap off lets a person or an alarm go above it, and
does not do so on its own.

**The state: `quiet_enabled`, right after `quiet` in the room's object.** The windows are
written as before, `active` included, and `active` keeps meaning "the clock is inside this
window": it does not go false when the room is switched off. So the `quiet` member is the same
bytes switched off and on, a client can show "would be capped now", and `effective_limit` is
the one field that says what is in force. The room's object grows by one member in every v2
state; the v1 shape is unchanged. The MQTT room payload is that object byte for byte, so it
gains the field and no topic changes.

**Persistence: state-file format 7.** `[zone]` gains `quiet_enabled` (0 or 1), required in a
format 7 file, as every format's own fields are. A format 1 to 6 file loads with every room
enabled, which is what its windows meant when it was written; the next write is format 7.
The flag is something a person set, not a fact about now, so it belongs in the file by 0018's
rule.

## Not chosen

- **A per-window `enabled`.** Above.
- **An optional `enabled` on `quiet_hours`.** Above.
- **Clearing `active` while switched off.** Then the `quiet` bytes would differ between off
  and on, and a client could no longer tell whether switching on would cap the room now.
- **Not persisting the flag** (treating it as a fact about now). A restart would silently
  re-enable a cap, at night, in the room where somebody had turned it off.
- **A global switch for the whole house.** K81's quiet hours are per-room caps; a house-wide
  switch is every room's command sent once each.

## Consequences

- A client built before this that parses the room's object strictly sees one new member.
  The Home Assistant client reads the members it knows by name and is unaffected; its switch
  entity is a later change.
- The control page and `chorusctl` neither show nor send the flag yet.
