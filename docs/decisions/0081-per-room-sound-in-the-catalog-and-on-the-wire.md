# 0081: a room's tone, loudness, night mode, speech enhancement, bass management and room-correction EQ are catalog v2 commands, persisted in state-file format 3, and reach every endpoint of the room as protocol v2 `sound` with its own bonded role

- Status: accepted (goal 12, 2026-10-01)
- Decided by: the goal (section 16 items 3 and 4; K30, K31, K87) inside the coordinator's
  goal-12 design envelope, track `chorus-g12/sound-catalog`; every default below not cited is
  ASSUMED
- Implemented in: `crates/control/src/sound.rs` (new: the values, their one spelling, the
  `ROOM_EQ_*` bounds), `crates/control/src/catalog.rs` (the three commands),
  `crates/control/src/zones.rs` (the room model and the state), `crates/control/src/persist.rs`
  (state-file format 3); `crates/protocol/src/v2/` (`Message::Sound`, `Type::Sound`,
  `sound_flags`, `SOUND_*`), `firmware/include/chorus/protocol_v2.h` and
  `firmware/src/protocol_v2.c` (the C codec), `firmware/include/chorus/session.h` and
  `firmware/src/session.c` (`chorus_session_keep_sound`, `chorus_session_last_sound`,
  `on_sound`); `crates/server/src/control.rs` (`sound_of`, `RoomView::sound_for`),
  `router.rs` (`push_sound`), `conductor.rs`, `clients.rs` (the greeting);
  `crates/client-linux/src/zone.rs` (`SoundInbox`, `sound_line`), `control.rs`
  (`ZoneWatch::last_sound`), `session.rs` (`Announced::sound`, `deliver_sound_to`), `run.rs`
  (the `sound` log line), `main.rs`. The contract is `docs/control-plane.md` ("Per-room sound")
  and `docs/protocol.md` ("0x39 sound"), with `fixtures/control/v2/` (7 command vectors, 12
  refusals, `state-rich` extended) and `fixtures/protocol/v2/sound_*` (3 vectors) and
  `fixtures/protocol/v2/rejected/sound_*` (15). Held by `crates/control/tests/catalog_v2.rs`,
  `crates/control/tests/sound_v2.rs`, `crates/protocol/tests/v2_vectors.rs` and `v2_rules.rs`,
  `firmware/tests/test_protocol_v2.c`, `crates/server/tests/sound_on_the_wire.rs` (the real
  binary) and a unit test in `crates/client-linux/src/zone.rs`

## Context

Goal 12 line B: per-room tone, loudness, night mode and speech enhancement in the control
catalog with tests; and the wire half of line C: bass management for a bonded set and the
settings the endpoints' DSP is configured from. The DSP itself (`crates/dsp`,
`firmware/src/dsp.c`) is another track's, and the endpoints apply nothing in this phase: they
decode and keep. The room-correction fitter (track roomfit) needs a set of bounds the catalog
and the wire both hold, so a fit is a valid command by construction.

## What was read

All read 2026-10-01: `BRIEF.md` section 3.1; the goal program's section 16; the goal-12 design
envelope; ADRs 0074 and 0075 (the precedents this follows); `docs/control-plane.md`,
`docs/protocol.md`; `crates/control/src/*.rs` and its tests; `crates/protocol/src/v2/*.rs` and
its tests; `firmware/src/protocol_v2.c`, `firmware/src/session.c`,
`firmware/tests/test_protocol_v2.c`; `crates/server/src/{control,router,conductor,clients}.rs`
and `crates/server/tests/limits_hold_for_every_volume_path.rs`;
[`.claude/goals/2026-09-chorus-research/research-theater.md`](https://github.com/NSchatz/chorus/blob/535ed2816b5735153ac81c52a235024aa806f2b5/.claude/goals/2026-09-chorus-research/research-theater.md) section 5.2;
`crates/client-linux/src/{zone,control,session,run,main}.rs`. External, for the defaults:
Sonos's support article "Adjust the bass, treble, balance, and loudness"
(<https://support.sonos.com/en-us/article/adjust-the-bass-treble-balance-and-loudness>), Sonos's
Control API overview (<https://docs.sonos.com/docs/control>), and a Sonos community thread on
the Amp's defaults
(<https://en.community.sonos.com/components-and-architectural-228996/sonos-amp-default-settings-loudness-and-sub-6821628>),
all read 2026-10-01. No GPL source.

## Decision

1. **Three catalog v2 commands, each a partial update.** `sound` `{zone, bass?, treble?,
   loudness?, night?, speech?}`, `bass_management` `{zone, crossover_hz?, sub_level_db?,
   sub_polarity?}`, `room_eq` `{zone, filters?, enabled?}`. An absent field keeps what the room
   had; `filters: []` clears; disabling keeps the filters (a person compares with and without a
   fit). A command with only `zone` changes nothing and answers with the state, like every other
   command that changes nothing. Written at `"v":2` (they are v2-only); at `"v":1` each is
   refused as not a command of that version. The catalog version does not move: v2 is the
   current catalog and grows by commands, as goal 11 grew it.
2. **Ranges and defaults.**

   | value | range | default | source |
   |---|---|---|---|
   | `bass`, `treble` | whole dB, -10 to 10 | 0 | ASSUMED, 1 dB a step. Sonos's apps show -10 to +10, but neither its support article nor its developer documentation prints the range |
   | `loudness` | bool | true | ASSUMED as Sonos's default: Sonos's own documentation does not state it; a community answer (above) says current players default to on |
   | `night`, `speech` | bool | false | ASSUMED (the envelope) |
   | `crossover_hz` | 40 to 200 | 80 | 80 Hz is the THX/SMPTE crossover ([`.claude/goals/2026-09-chorus-research/research-theater.md`](https://github.com/NSchatz/chorus/blob/535ed2816b5735153ac81c52a235024aa806f2b5/.claude/goals/2026-09-chorus-research/research-theater.md) section 5.2 and its source [B1]); the range is ASSUMED |
   | `sub_level_db` | -12.00 to 6.00, 0.01 | 0.00 | ASSUMED |
   | `sub_polarity` | normal, inverted | normal | |
   | `room_eq.filters` | at most 8; 20 to 1000 Hz, -12.00 to +3.00 dB (0.01), Q 0.500 to 10.000 (0.001) | none | ASSUMED (the envelope): cut-heavy, below 1 kHz |
   | `room_eq.enabled` | bool | true | ASSUMED: flat either way with no filters, and a fit just made is meant to be heard |

   Out of range is refused, never clamped, naming the field (`bass`, `crossover_hz`,
   `sub_level_db`, `sub_polarity`, ...); a filter outside the bounds is refused naming
   `filters`, the detail naming the filter's index and its field, as `quiet_hours` does for
   `windows`.
3. **Decimals as integers.** A gain is hundredths of a dB and a Q thousandths, held as integers
   and written with exactly two and three places (`-3.50`, `4.500`); read back with at most
   that many, never rounded (`0.7071` is refused), no exponent, no negative zero. These are the
   wire's own units, so the server forwards a room's values without converting anything, and no
   binary floating point appears in the catalog, the state file or the vectors (ADR 0016's rule
   for volume).
4. **The room-correction bounds are one set of numbers.** `ROOM_EQ_MAX_FILTERS`,
   `ROOM_EQ_FREQ_HZ`, `ROOM_EQ_GAIN_CDB`, `ROOM_EQ_Q_MILLI` are exported by `chorus-control`;
   the protocol crate (which depends on nothing) repeats them as `SOUND_EQ_*` and a server test
   holds the two equal; the C codec has `CHORUS_V2_SOUND_EQ_*`. The DSP track's
   `CHORUS_DSP_ROOM_EQ_*` mirror is that track's, and the integration track holds it equal to
   these (follow-up).
5. **Not a volume path.** None of the three moves `volume`, `limit` or `effective_limit`; a test
   sets every boost to its maximum in a limited room and every volume path's clamp is exactly
   as before. That a DSP boost cannot lift a room above its limit is the endpoint chain's
   look-ahead limiter at `min(1, effective limit)` (the DSP envelope), not the catalog's.
6. **The state.** Each v2 zone gains, after `ramp`, `sound` `{bass, treble, loudness, night,
   speech}`, `bass_management` `{crossover_hz, sub_level_db, sub_polarity, active}` (`active` =
   the bonded set has an `LFE` member) and `room_eq` `{enabled, filters}`. Every change is
   fanned out as the state message like any command, which is what "pushed as events" is.
   `fixtures/control/v2/state-rich.json` is regenerated: its `.fields` gained four commands
   (`sound`, `bass_management`, two `room_eq`), so the serial moved from 24 to 28, and every
   zone carries the three new objects; nothing else in it changed. No v1 vector changed (v1's
   state renderer is untouched).
7. **State-file format 3.** Ten required `[zone]` keys: `bass`, `treble`, `loudness`, `night`,
   `speech`, `crossover_hz`, `sub_level_db`, `sub_polarity`, `room_eq` (enabled, 0 or 1),
   `room_eq_filters` (`freq_hz gain_db q` per filter, `; ` between). A format 1 or format 2
   file loads unchanged with the sound defaults (a test loads a verbatim format 2 file) and is
   written back as format 3; a format 3 file missing a key, or holding a value out of range, is
   refused naming it, never defaulted. Two existing tests named the current format by number
   and were updated, not retired: `state_file_v2.rs`'s "written back as format 2" now says the
   current format (`STATE_FORMAT`), and `persist.rs`'s "a format this build does not
   understand" test now offers format 4.
8. **The wire: `0x39 sound`**, server to player, in a record, exactly the envelope's layout
   (big-endian; `docs/protocol.md`, "0x39 sound"). Every field is checked in wire order and
   rejected out of range, never clamped. Two refinements this track owns: a flag bit outside the
   five defined, and a `role` that is no channel position (above 18), are `undefined` rather than
   `out_of_range`, as every other enumeration in v2 is, so the rejection vectors' `problem` now
   names `out_of_range` or `undefined` and both test suites accept either (the three
   `room_volume` rejection vectors are unchanged); and an `eq_count` above 8 is rejected before
   any filter is read, naming `eq_count`, in both decoders (the C one has room for eight).
   `sub_present` other than 0 or 1 is rejected as `undefined` like every v2 bool; it has a test
   on the wire and no rejection vector, because the Rust message holds a `bool`.
9. **What the server sends, and when.** The conductor, like `room_volume`: in the greeting
   right after `room_volume` and before any audio; and to every player of the room whenever what
   that player would be told changes (the room's sound, bass management, room EQ, or its bonded
   set, which changes each member's `role` and `sub_present`), deduped against what the session
   was last sent. `role` is the endpoint's position in the room's set (`Role::position`, the
   channel map's numbers), 0 when in none; `sub_present` is the set having an `LFE` member.
   Every member receives the room's whole stream, so each does its own bass management from
   these two; the server renders nothing per endpoint.
10. **Endpoints, phase A: decode and keep.** The C session keeps the last `sound` in its run
    result (`chorus_session_keep_sound`, read with `chorus_session_last_sound`), logs an
    `event=sound` line with its fields, and hands it to an optional `on_sound` callback, which is
    the seam the endpoint DSP track configures the chain through. The Linux client keeps it in
    the session (`Announced::sound`, `sounds`) and in `ZoneWatch` (`last_sound`, held for the
    process like the room's gain), and its playout loop writes a `sound` line to the delay log
    when a new one arrives. Neither applies anything yet: a player that has never received one,
    and every player in this phase, plays flat. Kept across a new stream and a new session.

## Not chosen

- **A catalog v3.** v2 has no shipped peer that would break, the new commands are v2-only, and a
  version bump would rewrite every v2 vector's first byte for nothing.
- **Floating-point dB and Q in the catalog.** Two languages would disagree about spelling
  `0.1`; integers in the wire's units do not.
- **Clamping a filter into the bounds.** A fit that came back different from what was sent is
  the defect a refusal names; the fitter is required to emit only filters inside the bounds.
- **One message per setting on the wire.** Three messages would arrive and apply separately,
  and a player would spend frames configured from a mix of old and new; one message is one
  reconfiguration.
- **Server-side bass management (a per-endpoint render).** Every member already receives the
  whole stream (the DSP envelope), and rendering per endpoint would break the one-stream fanout.
- **Optional format 3 keys** (defaulting a missing one): the state file's rule since ADR 0018 is
  that a missing field is somebody's mistake, not a default.

## Follow-ups (the integration and endpoint-dsp tracks)

- Endpoint DSP: configure the chain from `chorus_session_last_sound` / `on_sound` on the C
  endpoint and from `ZoneWatch::last_sound` on the Linux client; the chain's `SoundSettings` is
  exactly this message's fields.
- Hold `CHORUS_DSP_ROOM_EQ_*` (the DSP track's) equal to `CHORUS_V2_SOUND_EQ_*` and
  `ROOM_EQ_*`, in a test.
- The control page has no UI for the three commands yet.
- Replace the ASSUMED tone range and loudness default with a citation if Sonos's own
  documentation ever prints them.
