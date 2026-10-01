# 0079: the conductor runs the schedule runtime on the civil and monotonic clocks, the slots play generated chimes and endpoints' line-ins, and a line-in's latency grows on the slots' one grid

- Status: accepted (goal 11, 2026-10-01)
- Decided by: the goal (brief section 15 item 2; K30, K78, K80, K81, K94) through the
  coordinator's goal 11 integration plan, sections (c) and (d), checked against what merged
  (ADR 0071, 0072, 0074, 0075, 0076, 0077); every default below not measured or cited is ASSUMED
- Implemented in: `crates/server/src/conductor.rs` (the clocks, the runtime's entry points, its
  effects, the routing of chimes and line-ins, the latency targets), `crates/server/src/slots.rs`
  (the `Chime` and `LineIn` inputs, a player per line-in port, the commands drained under the
  grid guard), `crates/server/src/linein.rs` (new, on the path: offers, the format check, the
  ports), `crates/server/src/control.rs` (`ControlState::runtime`, `take_applied`, the
  snapshot's slot groups), `crates/server/src/session.rs`, `clients.rs` and `stream.rs` (a
  source session's messages and upstream chunks), `crates/server/src/router.rs` (`push_message`,
  the dedupe on gain and limit), `crates/server/src/config.rs` and `main.rs` (`--tz`,
  `--civil-time-from`, `--schedule-time-scale`, the zone loaded, the chimes rendered),
  `crates/sync/src/latency_grow.rs` (`CubicResampler::with_capacity`, `room`, `held`, `clear`);
  `audio-path.conf`; the contract is `docs/control-plane.md` ("The schedule runtime") and
  `docs/protocol.md` ("Source", the goal 11 clarification); held by
  `crates/server/tests/alarms_sleep_autoplay.rs` (7 tests) and unit tests in `linein.rs` (3),
  `slots.rs` (2 more), `conductor.rs` (2 more) and `config.rs` (1 more)

## Context

ADR 0076 made the alarm, sleep, quiet-hours and autoplay rules a pure module that is called by
nothing; ADR 0077 gave the server stream slots on one grid and a conductor with two named seams
(`Conductor::wait`, `Conductor::pass`) for it; ADR 0071 gave a latency plan and a resampler for a
line-in that nothing calls; ADR 0072 renders chimes nothing plays; ADR 0066 made a Linux endpoint
offer its line-in to a server that ignored it. This change is the wiring: the real binary rings
alarms, fades sleep timers, follows quiet hours and plays line-ins.

## What was read

All 2026-10-01, all chorus's own: the integration plan's sections (c) and (d) and the design
envelope; ADRs 0066, 0071, 0072, 0074, 0075, 0076, 0077; `crates/server/src/*`, its tests,
`crates/schedule/src/*`, `crates/sync/src/latency_grow.rs` and `latency_sim.rs`,
`crates/client-linux/src/{source,session,zone,config,sync}.rs` and `tests/line_in_source.rs`,
`crates/control/src/{zones,rooms,transport}.rs`, `crates/protocol/src/v2/{messages,session}.rs`,
`config/sync.conf`, `audio-path.conf`, `docs/control-plane.md`, `docs/protocol.md`. No outside
source; no number here rests on one.

## Decision

1. **The clocks are read in the conductor, once per wake, and nowhere else.** `Clocks::now`
   returns the monotonic instant (`Instant`, the conductor's own origin) and the civil one
   (`SystemTime`, UTC seconds). Civil time decides only when (an alarm's minute, a quiet window);
   every duration is monotonic. `conductor.rs` stays excluded from the audio path, its reason
   rewritten; what crosses to the audio thread is a slot's input and a latency target, never an
   instant. The time zone is loaded at start, before any thread: `--tz <path>`, else `$TZ` (a
   zoneinfo name passing `is_safe_zoneinfo_name`, read under `/usr/share/zoneinfo`, an absolute
   path, or a POSIX TZ string), else `/etc/localtime`, else UTC; reported on a `civil` status
   line; a named file that cannot be read or is not a zone exits 2. A `$TZ` that is none of
   those exits 2 too: an alarm kept in a zone nobody chose rings at the wrong hour.
2. **`Conductor::wait`** waits until the earlier of a poke, `IDLE` and the runtime's next tick;
   **`Conductor::pass`** first runs the runtime: every line-in event (`on_input_signal`,
   `on_input_gone`, a refused format), every person's command applied since the last pass
   (`ControlState` records each one `apply` and `controller` commit; the runtime's own changes go
   through `ControlState::runtime` and are never recorded), then `tick` when due (at least once a
   second of schedule time, and at `next_deadline_ns`). `ControlState::runtime` applies the
   change to a copy, plans the slots, persists on `Persist`, publishes when the serial moved. A
   change of the runtime's that needs a slot more is not refused (an alarm must not be lost): the
   newest such group gets source `none`, logged `outcome=no-free-slot`.
3. **The effects.** `Log` is printed; `SetSource` needs nothing (the slots are routed from the
   room model); `SourceControl` takes or frees a line-in port and sends `source_control` (PCM) in
   the input's own session; a `RoomVolume` with a `ramp_ms` (a ramp step) is sent to every player
   of the room with that `ramp_ms` when its gain and limit are what the model says now, and the
   at-once values (`ramp_ms` 0) are sent by the pass from the model as before. The router dedupes
   on gain and limit only, so a step and the value it reaches are one message.
   **Deviation from ADR 0076's limit ordering:** the runtime's held-back limit (gain first with
   the old limit over `LIMIT_PULL_MS`, the limit later) is not forwarded; a limit or a quiet
   window pulling a room down goes out at once, gain and limit in one message, which is ADR
   0077's behaviour and the committed `fixtures/volume/room-volume-sequence.hex`. Both endpoint
   kinds enforce `min(gain, limit)` at every frame, so the only difference is a click-free pull
   versus an immediate one; keeping one rule for every path was preferred.
4. **Test clocks.** `--civil-time <day>-<HH:MM>` (ADR 0077's flag) now holds the runtime's civil
   clock at that weekday and time of the week of Monday 2024-01-01 in the loaded zone (ASSUMED
   reference week; the quiet-hours result is the one the flag gave before). `--civil-time-from
   <YYYY-MM-DDTHH:MM:SSZ>` runs it from that UTC instant; `--schedule-time-scale <n>` (1 to 60,
   ASSUMED ceiling) runs the schedule's durations and that civil clock n times faster and divides
   a ramp step's `ramp_ms` by n so an endpoint's ramp keeps up. The audio thread is never
   scaled. The two civil flags together are refused (exit 2).
5. **Chimes.** Every chime of `chorus_schedule::chime::CHIMES` is rendered at the server's format
   at start, before the audio thread and the scheduling report (`slot-media` status line), and
   played by index: `SlotInput::Chime(i)` repeats it with a `CHIME_GAP_MS` = 2000 ms (ASSUMED)
   gap, from its first frame whenever it becomes a slot's input. No allocation on the audio
   thread beyond the chunk encoding every slot already does.
6. **Line-ins from endpoints** (`crate::linein`, on the path). A session that declared the source
   role and sends `source_offer` offers `<endpoint>/<name>` (the name when it is a catalog
   identifier, else `line-<source_id>`, ASSUMED). The conductor starts an input by taking one of
   S ports (one input feeds at most one group, so S is enough) and sends `start`; the input's
   `stream_format` must be the server's (PCM, rate, channels, sample format) or it is stopped and
   logged `line-in refused reason=format-mismatch`; its chunks are written, converted to full
   scale 1.0, into the port's ring (`RING_MS` = 1000 ms, ASSUMED; a chunk that does not fit is
   dropped whole and counted). The session ending is `on_input_gone`. The port is a mutex the
   reader holds for one chunk's conversion and the audio thread for one copy; ASSUMED adequate
   against a 20 ms tick, not measured.
7. **A line-in plays on the slots' one grid** (deviation from the plan, which stamped line-in
   chunks with the plan's play-at instants). Every chunk keeps the grid's sequence and timestamp,
   so a session moving into or out of a line-in's group sees no timestamp jump; the latency is in
   which source frames each chunk carries. The plan (`LatencyPlan`, `CubicResampler`, ADR 0071)
   starts at `LOCAL_LATENCY_NS` = 30 ms (ADR 0071's L_local, ASSUMED there) once the port holds
   `LINE_IN_START_CHUNKS` = 2 chunks (ASSUMED: one of slack against arrival jitter), anchored
   there rather than on the endpoint's capture stamps, which are not read: the margin to the grid
   is then true by construction whatever the endpoint's offset estimate. Its first chunk, two
   frames short (the lookahead), holds its first frame for those two, so nothing is
   zero-inserted. The resampler is allocated at start for `LINE_IN_HOLD_MS` = 2000 ms (ASSUMED,
   above the start fill plus the largest growth) and fed only as much as it has room for. A tick
   whose source has not arrived plays silence and the plan does not move on (it is a clone that
   is asked first): a late upstream costs a gap and a chunk of latency, never a dropped or
   repeated source frame, and is counted (`underruns=`).
8. **The latency target** is L_local while the group is exactly the source endpoint's own room,
   else the group's tier: `WIRED_GROUP_LATENCY_NS` = 180 ms (`config/sync.conf`
   `playout_latency_us`, held equal by a test) or the wireless policy's 500 ms when any room of
   the group is declared wireless. The conductor sends `SlotCommand::LatencyTarget` on a change;
   the plan grows or shrinks to it with ADR 0071's bounded raised-cosine stretch. A line-in's
   player belongs to its PORT, not its slot, so a group that moves to another slot (a room joining
   forms a new live group) keeps its plan.
9. **No glitch from a move.** The audio thread drains the slot commands both before reading the
   stream and again under the grid guard just before it broadcasts, and the conductor sends the
   inputs a slot starts playing before it moves sessions and the ones it stops after. A session
   moved (under the same guard) between two slots therefore hears, at every tick, a slot that
   plays its group's input. The guard is held for the line-in render (one resample per port per
   tick, shared by every slot playing it) and the chunk encode of chimes and line-ins.
10. **The one-stream shape** (`--slots 0`) runs the runtime too (quiet hours, ramps, sleep,
    alarms on the room model) but has no slot to play a chime or a line-in in; source messages
    are ignored there as before.

## Every value ASSUMED here

`CHIME_GAP_MS` 2000; `LOCAL_LATENCY_NS` 30 ms (from ADR 0071); `LINE_IN_START_CHUNKS` 2;
`LINE_IN_HOLD_MS` 2000; `RING_MS` 1000; the input name `line-<source_id>`; `MAX_TIME_SCALE` 60;
the `--civil-time` reference week of 2024-01-01; a mutex port being adequate; an underrun waiting
rather than skipping. From ADR 0076, unchanged and also ASSUMED there: the 1 s tick and step, the
2 s end fade, 60 s late fire, 30 s autoplay hold, the bell fallback, the 30 s sleep fade.

## Tests (no device, conventions rule 10)

`crates/server/tests/alarms_sleep_autoplay.rs` (7), each on the real binary with `--slots 3`,
the committed UTC TZif fixture and `--civil-time-from`: a chime alarm rings at its civil minute,
the player's chunks after the switch are the rendered bell byte for byte, its `room_volume` gains
rise monotonically from 0 to 0.600 in ramped steps, and `alarm_stop` fades to 0 and restores the
volume and the stream; a line-in alarm starts the scripted line-in, ramps, and the player's
chunks read back as the source positions of a known triangle pattern with contiguous sequences
and no zero; a sleep timer fades monotonically to 0 in steps, sets the source to none (silence)
and restores 0.500 in the state and on the wire; quiet hours starting at 07:01 pull the study
down to the cap and hold a kitchen alarm's rise at it, on the server's state, on every
`room_volume` after the window starts, and in the REAL Linux client's applied gain
(`run_session`); line-in autoplay starts on the signal, plays the pattern, and stops after the
hold, restoring the stream; the real Linux client's source role (`source::spawn` on a modelled
capture paced in real time) is autoplayed, played and stopped when it falls silent; and K94 on
the real binary: the den, alone, plays its own line-in at a constant stamp offset, the kitchen
joins, and over the next 7 s the den's chunks keep contiguous sequences, a monotone stamp offset
grown (53 frames, 1.104 ms, in the run this record was written from; the plan's raised cosine 7 s into its 10 s ramp predicts 1.106 ms; the test asks for 10 to 120 frames)
and no zero sample, while the kitchen plays the same line-in. None of this is timing evidence
(BRIEF.md section 3.1 rule 3): values, orders and counts.

## Not chosen

- **Stamping a line-in's chunks with the plan's play-at instants:** every move into or out of a
  line-in's group would be a timestamp jump on both endpoint kinds.
- **Anchoring the plan on the endpoint's capture stamps:** the margin to the grid would depend on
  the endpoint's offset estimate; anchored at the start fill it is true by construction.
- **A player per slot:** a room joining forms a new group, which may take another slot, and the
  line-in would restart at L_local.
- **Refusing a runtime change at a full slot table:** an alarm would be lost to it.
- **Forwarding the runtime's held-back limit:** two messages for one limit change and a fixture
  rewritten, for a difference both endpoints already make harmless.

## Follow-ups

- An endpoint mode that plays a local line-in at L_local: today both endpoint kinds add their
  fixed playout latency (180 ms wired) on top of every stamp, so a line-in is heard at that plus
  the stamp offset, and the growth adds to it. Until then K94 holds for the offset on the wire.
- Drift between the capture clock and the server timeline: a port that fills (capture faster)
  drops a chunk, one that starves waits a chunk; a fill-level servo using the same plan at a tiny
  rate would absorb it.
- A format conversion for a line-in that is not the server's format, and a wire message for a
  refused start (ADR 0066's goal 17 follow-up).
- Measure the audio thread's tick with S line-ins and chimes on the target host (a
  `docs/measurements/` report, `Source: host`), including the grid guard's hold time.
- The house soak (`tools/house-soak-run.sh`, a parallel track) with alarms, sleep and a line-in.
- Whether turning a ringing alarm's room up should end it (ADR 0076's follow-up, unchanged).
