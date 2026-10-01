# 0000: control catalog v2 adds rooms, bonded sets, saved and live groups, take-the-room, Sonos-style group volume, clamped limits and quiet hours, and the alarm, sleep and autoplay configuration, beside v1 and byte-compatible with it

- Status: accepted (goal 11, 2026-10-01)
- Decided by: the goal (brief section 15 items 1 and 2; K54, K59, K77, K78, K81, K91, I10, I19)
  inside the coordinator's goal-11 design envelope; the defaults marked ASSUMED below are the
  goal's, not measured
- Implemented in: `crates/control/src/rooms.rs` (the v2 values), `crates/control/src/catalog.rs`
  (the commands, the per-version decoder), `crates/control/src/zones.rs` (the room model, the
  clamp, the runtime hooks, the v1 and v2 state renderers), `crates/control/src/persist.rs`
  (state-file format 2, B-9), `crates/server/src/control.rs` (`?v=1`, answers at the peer's
  version), `crates/server/src/main.rs` (the declared transports reach the model); the contract is
  `docs/control-plane.md` with `fixtures/control/v2/`; held by
  `crates/control/tests/catalog_v2.rs` and `crates/control/tests/state_file_v2.rs`, and by every
  v1 test and vector unchanged

## Context

Goal 11 needs rooms, bonded sets (stereo pairs, subs, theater surrounds), saved and live groups,
"take the room", group volume, per-room limits and quiet hours, and the configuration alarms,
sleep timers and autoplay need, all as control-catalog changes a phone app can drive. Catalog v1
(ADR 0016) has zones, free-named groups, volume and mute, and its decoder refuses a field it does
not declare, so every one of these is a new catalog version. K91 says a Wi-Fi endpoint is never
part of a bonded set, and goal 2's audit found the state file renamed without an `fsync` (B-9).

## What was read

All read 2026-10-01: `BRIEF.md` sections 2.2 and 3.1; the goal program's section 15 and K54,
K59, K77, K78, K81, K91, I10; the goal-11 design envelope; `docs/control-plane.md`;
`docs/protocol.md` ("The channel map"); ADRs 0016, 0017, 0018, 0019, 0024 and 0026;
`crates/control/src/*.rs` and its tests; `crates/server/src/{control,controller,main}.rs`;
`crates/client-linux/src/control.rs`; `crates/server/src/ui/chorus.js`;
`tools/{refusals,control-plane-run,control-determinism,restart-storm-run}.sh`. Sonos's public
Control API documentation: "Volume" (<https://docs.sonos.com/docs/volume>) and
"groupVolume: setVolume" (<https://docs.sonos.com/reference/groupvolume-setvolume-groupid>),
both read 2026-10-01. No GPL source.

## Decision

1. **Versions.** `CATALOG_VERSION = 2`, `IMPLEMENTED_VERSIONS = [1, 2]`. A command is WRITTEN at
   the lowest version that declares it (`Command::min_version`), so every v1 command encodes to
   its v1 vector byte for byte and a v1 server reads what a v2 client sends. A v1 command is
   accepted at v1 or v2; a v2-only command or field at v1 is rejected as unknown at that version
   (one message, the session stays open). An `error` carries the version of the message it
   answers (`Refusal::at`), or 1 where none could be read. The state message is v2 everywhere;
   `GET /api/state?v=1` and `GET /api/events?v=1` serve the v1 renderer, which is the one that
   produces `fixtures/control/state.json`. A v2 state's `zones` start with v1's fields in v1's
   order, which is what keeps the Linux client's subscriber and the control page working
   unchanged. `POST /api/command` answers with the v2 state, the same bytes every subscriber is
   sent.
2. **The one v1 vector a version bump touches.** `fixtures/control/refused-unknown-version.json`
   says `"implemented":[1]`, which a [1, 2] build cannot say truthfully. The vector is kept byte
   for byte and is reproduced by `decode_command_with(input, [1])`, a decoder holding the set its
   own `.fields` names (`implemented = 1`); the [1, 2] build's refusal is pinned beside it in
   `fixtures/control/v2/`. `catalog_vectors.rs` gained that one call and a `..` in one pattern;
   `tools/refusals.sh` greps for `[1,2]`. No v1 vector changed.
3. **Rooms (zone on the wire).** A room gains `limit`, quiet-hours windows (`days`, `start`,
   `end`, `limit`; past midnight a window belongs to the days it starts on; `start == end` is
   refused as ambiguous; at most 8, ASSUMED), and a bonded set. The state shows `volume` as
   clamped, `limit` and `effective_limit`, each window's `active`, and the room's declared
   `transport`.
4. **The clamp rule.** Effective limit = `min(limit, every active window's cap)`. Every volume
   path (`volume`, `volume_step`, `group_volume`, `group_volume_step`, the controller role's
   commands via `crates/server/src/controller.rs`, and the runtime hooks `runtime_volume` and
   `start_ramp`) goes through one setter that clamps, never refuses. Lowering a limit or a window
   becoming active pulls the volume and a running ramp's target down; a window ending raises
   nothing. The model takes which windows are active as INPUT (`Zones::set_civil_time` with a
   weekday and `HH:MM`, or `Zones::set_active_quiet`); it reads no clock.
5. **Bonded sets and the Wi-Fi refusal (K91).** Roles are `docs/protocol.md`'s channel positions
   `FL FR FC LFE BL BR SL SR`. Valid layouts: `FL FR`, plus optional `LFE`, plus optional `FC`,
   and a surround pair `SL SR` or `BL BR` only with `FC` (so 2.0, 2.1, 3.0, 3.1, 5.0, 5.1). The
   envelope's "theater FL FR FC with optional LFE and SL SR or BL BR" is read as allowing the
   front three without surrounds (a soundbar-style LCR), recorded here as the interpretation. One
   endpoint per role; members are endpoints of that room and of no other room's set. An endpoint
   whose link is not `wired` (wireless or unknown) is refused (field `members`, the detail names
   the endpoint, its role and its link), a room declared `--zone <id>=wireless` cannot hold a
   bond (field `zone`), and an endpoint in a set that later reports a non-wired link has that
   `attach` refused (field `link`) until the set is dissolved, so the invariant cannot be broken
   after the fact. Links are what endpoints say on `attach` (new optional `link`) and are
   persisted.
6. **Groups.** A formed group is `room` (one room in its own group), `saved` or `live`. Saved
   groups (`group_save`, `group_delete`) are persisted definitions of two or more rooms with an
   id no room has, listed always with `active` (K59). `join` (a command this track added beside
   the envelope's list: an assigned live-group id needs a command that forms one) puts a room in
   the group of a room or group; joining a room that is alone forms `live-<n>`, the smallest free
   `n`. A live group that `join` or `take` leaves with one room dissolves into that room's own
   group, carrying its source. v1's `group` and `ungroup` keep v1's behaviour exactly (a room
   alone in a client-named group stays there), so no v1 test moved.
7. **Take the room (K78).** Every target room leaves its group for the target's (a room's own,
   a saved group's id, or a formed group); leftovers keep playing; a dissolved live group's last
   room keeps the source; rooms that had joined the target room's own group leave it with its
   source. An optional `source` sets what the target plays.
8. **Group volume (K77), Sonos's definition.** Sonos: group volume is the average of the players'
   volumes, and setting it "proportionally adjusts the volume of each player so that the average
   corresponds to the desired group volume level" (docs.sonos.com/docs/volume, read 2026-10-01).
   chorus: `G` = average, rounded half up; `G'` scales every room by `G'/G` rounded half up, each
   clamped to its effective limit; from `G = 0` every room is set to `G'`; a step is
   `G' = clamp(G + step, 0, 1000)`. Not followed from Sonos, deliberately: a clamped room's
   shortfall is not redistributed (the clamp is per room and never a surprise raise of another);
   group volume never unmutes (Sonos's `setVolume` does, per its reference page; chorus's mute is
   per room and orthogonal); and the scale is from the volumes as they stand rather than from a
   snapshot, so a group driven to 0 loses its balance (the envelope's "from 0 sets all").
9. **Alarms, sleep and autoplay: configuration and validation only.** `alarm_set` (target a room
   or a saved group, `HH:MM`, days with empty meaning once, a source, volume, `ramp_s` 0 to 600
   and `duration_min` 0 to 720 with 0 meaning until stopped, both bounds ASSUMED),
   `alarm_delete`, `alarm_stop`, `sleep` (a room or a formed group, 0 to 720 minutes ASSUMED, 0
   cancels; not persisted, it goes when its group does), `autoplay` (input `<endpoint>/<input>`,
   target a room or a saved group). At most 32 each of saved groups, alarms and autoplay rules
   (ASSUMED, a bound on the state message). The runtime drives the model through documented hooks
   (`set_civil_time`, `set_active_quiet`, `set_group_source`, `start_ramp`, `runtime_volume`,
   `stop_ramp`, `set_alarm_ringing`, `sleep_expired`, `offer_input`, `withdraw_input`), each
   bumping the serial when it changes something. Sources are spelled `stream`, `none`,
   `chime:<name>`, `line-in:<endpoint>/<input>`; existence is the runtime's.
10. **Persistence, state-file format 2.** Adds `limit`, `quiet` and `bond` to `[zone]`, and
    `[endpoint]`, `[saved-group]`, `[alarm]`, `[autoplay]` sections; every format 2 field is
    required. A format 1 file loads unchanged with the v2 defaults and is written back as format
    2. Facts about now (presence, active windows, ringing, ramps, sources, sleep timers, offered
    inputs) are not persisted. A hand-edited bond is held to the `bond` command's rules at load.
    B-9: the temporary is `fsync`ed before the rename and the directory after; the
    write-then-readback invariant (ADR 0018) is unchanged.
11. **Whole or nothing.** `Zones::apply` now applies to a copy and installs it on success, which
    makes "a refused command leaves the state byte-identical" true by construction for the
    multi-room commands.

## Not chosen

- **A v2 that changes v1's shapes** (renaming `zone` to `room`, nesting groups): it would break
  every v1 vector and the Linux client for a spelling.
- **Writing every command at v2**: every v1 vector would change its first byte for nothing, and
  v1 servers could not read v2 clients' v1 commands.
- **Dissolving every one-room live group, v1's included**: `group` into a fresh name would undo
  itself at once, changing v1's behaviour and its vectors.
- **Silently dropping a bond when a member reports a radio**: it breaks K91's invariant quietly;
  the refusal names it instead.
- **Redistributing a clamped room's shortfall** to reach Sonos's exact average: it raises rooms
  nobody touched, which is the opposite of what a limit is for.

## Follow-ups (the integration track)

Send `room_volume` (gain, effective limit) per session; drive `set_civil_time` from
`crates/schedule` once a minute and at every window boundary; fire alarms, ramps and sleep
fades through the hooks; set sources and offered inputs from line-in and chimes; give the
control page UI for the v2 fields.
