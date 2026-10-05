# 0195: the room-correction sweep plays in one room on a stream of its own beside the stream slots, so no source changes and the room's group keeps playing; its level is the room's volume, clamped to the effective limit, and the state says while it plays and how it ended

- Status: accepted, 2026-10-05
- Decided by: the task for the scope ("let a controller ask the server to play the measurement
  sweep (`Sweep::recommended` at the stream's rate) in one room, once, at a bounded level, and
  then give the room back to what it was playing"; "a decision record says how the sweep
  reaches the room"; no timing claim about the sweep's alignment without a measurement); K31
  and K87 for room correction and the phone-microphone measurement; 0083 for the fitter and
  its sweep; 0175 for the shape this follows (a stream of its own on the slots' grid); this
  record for the cheap decisions: where the sweep is cut, how the room is put on it, the
  silences around it, what sets its level, what the room model shows, what is refused and
  what calls a sweep off.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `crates/server/src/sweep.rs` (`SweepProgram`, `SweepPlayer`, `SweepCommand`,
  `SweepPort`, `LEAD_MS`, `TAIL_MS`), `crates/server/src/slots.rs` (`SlotCommand::Sweep`,
  `SlotMedia::sweep`, the sweep step), `crates/server/src/router.rs`
  (`slotted_with_mixes_and_sweep`, `sweep_route`), `crates/server/src/measure.rs`
  (`Measurer::measure`, `route`, `direct`, `settle`), `crates/server/src/conductor.rs` (the
  routing, `direct` and `settle_measurement` in `pass`, `reason=measuring` on the TV path),
  `crates/server/src/control.rs` (`measure_through`, `measure_begin`, `measure_end`),
  `crates/server/src/main.rs` (the program rendered at start), `crates/control/src/catalog.rs`
  (`Command::MeasureSweep`), `crates/control/src/zones.rs` (`measure_check`, `measure_begin`,
  `measure_end`, `measure_watch`, `Measurement`, `MeasurementState`);
  `docs/control-plane.md` ("Room correction: the `measure_sweep` command"),
  `docs/room-correction.md` ("Playing the sweep in a room"), `audio-path.conf`,
  `fixtures/control/v2/` (`measure_sweep*`, `state-measur*`, `error-measure-sweep-*`); held by
  `crates/server/tests/measure_sweep.rs`, `crates/control/tests/measure_v2.rs`,
  `crates/control/tests/catalog_v2.rs` and the unit tests of `sweep.rs`.

## Context

0083 gave chorus a fitter that turns a recording of a sweep played in a room into the room's
`room_eq` filters, and named the sweep the measurement starts from (`Sweep::recommended`).
Nothing could play that sweep. A server generates two kinds of signal already: a chime, which
is the SOURCE of a group and so plays in every room of it, on repeat; and an announcement,
which is a clip from a player mixed over what its rooms play (0175). A measurement needs
neither: it needs one room to play exactly the fitter's samples, once, with nothing mixed in,
and then to go back to what it was doing.

## What was read

All on 2026-10-05, in this repository. In full: `crates/server/src/announce.rs`,
`crates/server/src/conductor.rs`, `crates/control/tests/announce_v2.rs`,
`crates/server/tests/common/mod.rs`, `docs/room-correction.md`. In part:
`crates/dsp/src/roomfit.rs` (`Sweep` and its methods; the other items by name only),
`crates/server/src/slots.rs` (everything up to the line-in player),
`crates/server/src/router.rs` (the shapes, the routes, `move_to`, `push_room_volume`),
`crates/server/src/mixer.rs` (`MixCommand`, `MixPort`, `Mix::new`, `Mix::apply`),
`crates/server/src/control.rs` (the announcement hooks, `commit`, `apply_decoded`, `runtime`,
`snapshot`, `Snapshot`), `crates/server/src/main.rs` (the router, the slot media, the
announcer and the conductor's construction), `crates/server/src/linein.rs` (the two sample
conversions), `crates/server/src/schedule_runtime.rs` (`announcement_over`),
`crates/control/src/catalog.rs` (`Command`, its encoder and the `announce` and `voice_start`
decoders), `crates/control/src/zones.rs` (`Zone`, `Zones`, the announcement functions,
`voice_start_check`, `set_volume`, `runtime_volume`, the end of the state's encoder),
`crates/control/tests/catalog_v2.rs` (the vector harness),
`crates/server/tests/alarms_sleep_autoplay.rs` (the chime alarm test),
`crates/server/tests/common/line_in.rs` (the helpers and `Recorder`),
`docs/control-plane.md` (the v2 command table, "Announcements", "The state message", "The
refusals"), `docs/decisions/0175-announcements-are-mixed-per-room-on-the-slots-grid.md` (its
header, "What was read" and "ASSUMED values"), `audio-path.conf` (the server's entries and
the room-correction exclusion), `tools/conventions/check-adrs.sh` (the rule),
`fixtures/README.md` (the control vectors). No source outside this repository was opened,
and nothing was measured.

## Decision

**How the sweep reaches the room: a stream of its own.** A server with stream slots carries
one more fanout after the slots, the silent one and the announcement mixes: the sweep's
(`Router::sweep_route`). The audio thread cuts it at every tick under the grid guard, on the
tick's own sequence and timestamp: silence, and from a `SweepCommand::Start` the program,
copied chunk by chunk out of a buffer rendered before the audio thread existed. The conductor
routes the player sessions of the measured room to that fanout (`Measurer::route`), tells the
audio thread to start only after the pass has moved them (`Measurer::direct`, the order 0175
uses for a mix), and stops routing them there when the audio thread says the program ran to
its last frame (`SweepPort::completed`), so the next pass moves them back to their group's
slot.

What follows from that:

- **No source changes.** The room's group plays what it played, to everybody else in it, and
  "giving the room back" is one move of its sessions, with nothing to remember and nothing to
  restore but the volume. The state shows the group's source unchanged throughout.
- **One room, whatever group it is in.** A room that shares a slot with others is measured
  alone. The test holds a room of the same group, and a room alone, to their own stream in
  every chunk of the span.
- **The room hears nothing but the program** while it lasts: the stream is not a mix.
- **One sweep at a time.** There is one sweep stream, so a second `measure_sweep` is refused
  (`measuring`) until the first ends. A measurement wants a quiet house; a second stream
  would be a second buffer and a second player for a case nobody has.

**What is played.** `Sweep::recommended(rate).signal()`, each sample written at the stream's
sample format by the conversion the line-in and the mixer use (`linein::encode_sample`;
16-bit: `round(x * 32768)`, clamped) and copied to every channel. The server takes the
samples from the fitter's own function, so they cannot drift apart, and the end-to-end test
compares what a player session received with that function's output, sample for sample.

**The program** is `LEAD_MS` of silence, the sweep, `TAIL_MS` of silence, and the state's
`measurement` carries the three lengths. The silence before lets what the room was playing
ring out and puts the room's `room_volume` in force before the first sample; the silence
after keeps the room quiet while its response to the sweep's end is recorded (the fitter
refuses a recording shorter than the sweep plus its response window).

**The level is the room's volume.** The sweep's samples are fixed at the fitter's half full
scale. The command's optional `volume` sets the room's volume for the sweep through the room
model's one volume path (`Zones::set_volume`), which clamps to the effective limit: the
room's own limit and every active quiet-hours window. Without it the room plays at its own
volume, which that path has already held to the limit. Afterwards the room gets back the
volume it had, unless somebody changed it meanwhile (the rule an announcement's volume
follows). A limit lowered during the sweep reaches the players at the next `room_volume`
like any other, so the bound holds for the whole program, not just at its start.

**The command and the state.** `measure_sweep` names a `zone`, not a target: a measurement is
of one room. Its answer is the state, whose `measurement` member names the sweep: `id`,
`zone`, `state` (`playing`, `finished`, `cancelled`), the `volume` it plays at, the three
lengths, and `reason` when cancelled. It is written once there has been a sweep and stays
until the next one, so a caller that watches the event stream sees its own end; it is never
persisted and never in the v1 shape.

**What is refused**, each by name with nothing changed: an unknown room; `measuring` (this
room or another is playing a sweep); `no-speaker` (no speaker attached to the room, or none
with a player session up: nobody to play it); `muted` (the sweep would be silence, and the
recording would come back `too_quiet` with no hint why); `alarm-ringing`; and
`no-sweep-stream` on a server started without `--slots`, which serves one stream to every
room and cannot play in one alone. The room model decides the first five
(`Zones::measure_check`), so the catalog's vectors pin them; the server adds the live-session
half of `no-speaker` and `no-sweep-stream`.

**What calls a sweep off** (`cancelled`, with the audio thread told to stop at its next chunk
boundary and the room's volume put back): an alarm that starts to ring in the room, because
an alarm must wake; the room's last player session going away; and the audio thread not
reporting the end within the program's length plus `END_GRACE`. The schedule runtime is told
of every end through the hook an announcement's end uses (`announcement_over`), so a room it
holds is never restored to the sweep's volume.

**No timing claim.** A sweep's chunks carry the grid's stamps and play after the room's tier
latency like any stream's. This record states no figure for when the room hears the sweep
relative to the command, to the state, or to any other room, and none exists: the fitter
searches the recording for the response's peak and assumes no latency.

## Considered and not chosen

- **A `sweep` source the room's group plays** (the chime's path: a slot input). It would play
  in every room of the group, so the command would have to regroup the room first and put the
  group back afterwards: two commits to the room model, a slot more than the house was using
  (refused when none is free), and a restore that has to cope with everything a person did in
  between. A stream of its own needs none of that.
- **An announcement mix with a generated clip** (0175's path). A mix ducks the music by 20 dB
  and adds the clip; the room would not receive the fitter's samples, and a mix belongs to a
  player port. Reusing its fanout while bypassing its arithmetic is the stream of its own with
  a misleading name.
- **A player fetching a rendered WAV** (the announcement's interrupting path). It needs
  `--players`, an origin to fetch from and the decoder and resampler in the way of samples
  that must be exact.
- **Rendering on the audio thread.** The sweep is `exp` and `sin` per sample; a buffer made
  once costs memory (1.25 MB at 48 kHz stereo 16-bit) and nothing per tick.
- **Refusing a room whose group plays something**, to keep the house quiet. Whether the other
  rooms' music matters depends on the house; the fitter's `too_noisy` refusal reports it when
  it does.
- **A command to stop a sweep.** It lasts 6.5 s and the room's `volume` and `mute` work
  throughout. Not asked for; a follow-up if the measurement UX wants one.

## ASSUMED values

- `LEAD_MS` = 500: the fitter's response window (itself ASSUMED from REW's 500 ms), as the
  time what the room was playing gets to ring out. Not measured in a room.
- `TAIL_MS` = 1000: `docs/room-correction.md`'s "at least the sweep plus a second".
- `END_GRACE` = 5 s past the program before a sweep the audio thread never reported is called
  off. It only bounds a fault.
- The sweep on every channel of the stream, so a stereo pair or a bonded set is measured
  together: what one `room_eq` per room corrects. Measuring one speaker at a time is not
  built.
- The program's buffer is rendered at the stream's format whatever it is: 6.5 s of frames
  (about 40 MB at 192 kHz, eight channels and 32-bit float, were a server ever configured
  so; 1.25 MB at 48 kHz stereo 16-bit).

## Follow-ups

- Accepting the recording, fitting and applying it (the next task), including what to do
  about a `room_eq` that is already enabled when the room is measured.
- Repeat sweeps and averaging.
- A measurement of where the sweep lands on a room's timeline, if anything ever needs it.
