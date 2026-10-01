# 0077: one server serves every group's stream on stream slots cut on one grid, routes each session to its group inside the session, pushes room_volume and controller_state from one conductor, and writes every event stream from one thread

- Status: accepted (goal 11, 2026-10-01)
- Decided by: the goal (brief section 4.8 and section 15; K78, K81, I10; audit finding B-5; the
  ADR 0067 and ADR 0074 follow-ups) inside the coordinator's goal-11 design envelope and the
  integration plan's PR 1; every default below not measured or cited is ASSUMED
- Implemented in: `crates/server/src/slots.rs` (the slots, one grid, the inputs),
  `crates/server/src/router.rs` (sessions, fanouts, moves, deduped pushes),
  `crates/server/src/slot_table.rs` (plan before commit), `crates/server/src/conductor.rs` (the
  change-driven pushes), `crates/server/src/events.rs` (the event writer),
  `crates/server/src/control.rs` (the room model and its slot table under one lock, the snapshot,
  `set_civil_time`), `crates/server/src/clients.rs` and `session.rs` (registration and the
  greeting), `crates/server/src/stream.rs` (fanout ids), `crates/server/src/config.rs` and
  `main.rs` (`--slots`, `--event-streams`, `--civil-time`, the population); the contract is
  `docs/control-plane.md` ("Stream slots", "Event streams and the event writer", "The thread
  population") and `docs/protocol.md` ("0x38 room volume"); held by
  `crates/server/tests/stream_slots.rs`, `limits_hold_for_every_volume_path.rs`,
  `control_thread_population.rs`, `control_stalled_peer.rs`, `control_request_rules.rs`,
  `fixtures/volume/room-volume-sequence.hex` and `firmware/tests/test_volume.c`

## Context

Catalog v2 (ADR 0075) gave the server a room model with groups, take-the-room, group volume,
limits and quiet hours; ADR 0074 put the room's gain and limit on the audio wire and made both
endpoint kinds enforce them, and left the SENDING to this track. Until now one server process
served one stream, and grouping relied on the Linux client moving to another process's address
(`--group-audio`). The C endpoint has ONE configured server address and never reads the control
plane, so that cannot group it. The event streams were the other open item: each held a control
worker for its whole life (the B-5 stopgap kept one worker for commands).

## What was read

All 2026-10-01, all chorus's own: BRIEF.md section 4.8 and section 15; the goal-11 design
envelope and the integration plan (a planner's draft written before phase A merged; every API it
assumed was checked against main, and where it differed this record follows main); ADRs 0014,
0016 to 0019, 0063, 0067, 0071, 0072, 0074, 0075; `crates/server/src/*`, its tests,
`crates/control/src/{zones,rooms,catalog,fanout,json}.rs`, `crates/client-linux/src/{session,
zone,control}.rs` and `tests/room_volume.rs`, `firmware/src/playout.c`, `firmware/include/
chorus/volume.h`, `firmware/tests/test_volume.c`, `tools/control-determinism.sh`,
`tools/conventions/check-shared-fixtures.sh`, `audio-path.conf`, `docs/control-plane.md`,
`docs/protocol.md`. No external source; no number here rests on one.

## Decision

1. **Stream slots.** `--slots S` (1 to 32, ASSUMED ceiling; default 0) makes one process cut S
   streams on its ONE real-time audio thread, in lock-step on one grid: tick `k` emits chunk `k`
   of every slot with the same sequence and the same presentation timestamp, `origin + k * chunk
   duration` on the one monotonic timeline (the relation the chunker stamps). A slot is a fanout
   and an input; one more fanout plays silence to a session in no slot. `--slots 0` is the
   one-stream shape, unchanged: the same audio thread loop (`serve_stream`), `stream_end`,
   `--serve-forever` and exit codes. `--slots` needs `--control-listen` (the room model routes)
   and is refused with `--group-audio` (exit 2, both named); `--group-audio` stays as the legacy
   shape for Linux clients and is not extended.
2. **Inputs, switched at a chunk boundary.** What a slot plays is a `SlotInput`, changed by a
   `SlotCommand` on a bounded channel the audio thread drains with `try_recv` at the top of each
   tick, so the conductor never blocks it. This change carries `Silence` and `Stream` (the
   configured `--source`, read ONCE per tick however many slots play it, and only when one does).
   A group whose source is a chime or a line-in plays silence until those inputs exist. In the
   slot shape a source that ends is reopened by the supervisor (its slots play silence between),
   and no slot sends `stream_end`: a slot does not end, its input changes.
3. **Routing inside the session.** Every session's outbound queue is made when its connection is
   accepted and attached to a fanout only once its session is up (`Router::register`, from the
   slot's reader after `establish`), so neither a refused connection nor a failed handshake
   leaves a subscriber behind. A session hears the slot of the group its endpoint's room is in
   (the room whose `present` names the endpoint, else whose `endpoints` do: the rule the
   controller role already used); in no room, or in a group with no slot, it hears silence. A
   move detaches the queue from one fanout and attaches it to another under the GRID GUARD, a
   mutex the audio thread holds while it broadcasts one tick to every fanout, so a moved session
   gets each tick exactly once: its sequences stay contiguous and only the content changes, which
   both endpoint kinds already play (their playout drops only a chunk at or behind the
   highwater). Every group's `audio` in the state is the one listen address, so Linux clients do
   not move either. The guard is held for S + 1 non-blocking `try_send`s per tick; a move holds it
   for one detach and one attach.
4. **Plan before commit, a named refusal at the ceiling.** A group NEEDS a slot when it is formed
   and its source is not `none`. The room model and its slot table share one lock; every change
   (a command, a button) is applied to a copy, the table is planned over the copy (sticky: a
   group keeps its slot; slots freed first, then new groups take the lowest free one), and both
   are installed only when the plan fits. One that needs a slot more is refused `400`, field
   `target`, naming the ceiling, the groups holding it and `--slots S+1`, with nothing applied or
   persisted. At start, groups past the ceiling are given source `none` (no slot, silence) and
   each is reported (`slots group=<id> source=none reason=every-slot-in-use`) rather than the
   server refusing to start a house whose state file it was handed. The deployment's answer is
   S = the number of rooms, which can never run out (`deploy/run-server.sh` takes it as
   `CHORUS_SLOTS`, opt-in, because what an installed house hears is the owner's deploy).
5. **`room_volume`, per player session.** With a control plane, a session of an endpoint a room
   names gets, in its greeting after `stream_format` and `output_delay` and before its first
   chunk, the room's `room_volume`: gain = its volume (0 muted) and never above the limit,
   limit = its effective limit, `ramp_ms` 0. After that, whenever the room's gain or effective
   limit changes, wherever the change was made, every player session of that room is sent the
   new values, at once (`ramp_ms` 0, ASSUMED: a person's change applies as made, no de-click
   ramp), and only when it differs from what that session was last sent. A room in a group gets
   its own room's values. A server with no control plane sends none (it has no room), and an
   endpoint in no room is sent none (it plays at its own ceiling, ADR 0074).
6. **`controller_state`, on changes made anywhere.** A controller session's greeting carries its
   room's `controller_state`; after that the conductor pushes it whenever what a controller shows
   (volume points, mute, group) changes, from the page, another endpoint or a button. An answer to
   the session's own `controller_command` is still always sent (through the router, so the
   conductor's dedupe counts it): the ADR 0067 follow-up.
7. **The conductor**, a thread created before the scheduling report, does all of 2, 3, 5 and 6.
   It wakes on a poke (bounded at one: a waiting poke covers every change before it) sent after
   every commit and every session registration, or every 200 ms; it reads the room model once
   per pass and pushes only what changed; a push a full queue refused (that session is 128 items
   behind) is owed and the pass runs again 50 ms later (ASSUMED). **It reads no clock in this
   change.** `--civil-time <day>-<HH:MM>` holds the civil time quiet hours are evaluated at fixed
   for the run (`ControlState::set_civil_time`, the catalog's own hook), for tests and a server
   with no time source.
8. **One event writer (B-5).** A worker answering `GET /api/events` writes the headers and the
   opening state under its own 5 s write timeout, subscribes the stream to the control fanout,
   and hands the socket to the event writer, one thread holding every stream on non-blocking
   sockets; the worker goes back to the pool. The writer holds at most `--event-streams` (default
   64, ASSUMED); past it a stream is answered `503`, `every one of this server's K event streams
   is held`. Each peer has at most one state message pending: the next is taken off its fanout
   queue only once the last is written, so the fanout's own bound (32, ADR 0017) and its drop of
   a subscriber at that bound stay the backlog bound. A peer with no write progress for 5 s (the
   old `WRITE_TIMEOUT`) is dropped and counted (`stalled_dropped`), and every other peer is
   written on every pass regardless. The stopgap (`stream_slots`, a worker kept for commands) is
   deleted.
9. **The population.** With the control plane on, `6 + 2N + M` threads in either shape (the
   event writer and the conductor join `4 + 2N + M`), plus one for `--advertise`; without it
   `3 + 2N`, unchanged. None depends on S, on endpoints or on subscribers;
   `control_thread_population.rs` grades it against `/proc` with `--slots 1` and `--slots 8`.
   `tools/control-determinism.sh` now starves the event-stream ceiling (1 and 2, below the three
   streams the checks hold), since workers no longer hold streams.
10. **On the path.** `slots.rs` and `router.rs` are listed in `audio-path.conf` (they decide what
    every chunk is and who gets it; neither reads any clock but the one timeline);
    `conductor.rs`, `events.rs` and `slot_table.rs` are excluded with reasons.

## How the schedule runtime plugs in (the next goal 11 track)

The time-driven work (alarms, sleep, the quiet-hours clock, ramps) is the pure
`crates/server/src/schedule_runtime.rs` (ADR 0076: `Runtime::tick` returning `Effect`s,
`next_deadline_ns`, `on_command_applied`, `on_input_signal`, `on_input_gone`), written in
parallel; this change neither creates nor edits it. It joins the conductor at two named seams, so the
change there is small:

- `Conductor::wait` is the loop's wake: today a poke or 200 ms; there, the earlier of that and
  `next_deadline_ns`. The thread loop then reads the civil and monotonic clocks ONCE per
  wake and hands them to `tick` (conductor.rs moves from "reads no clock" to "reads them once per
  tick, for scheduling only"; it stays excluded from the audio path).
- `Conductor::pass` is the effect application: today it applies the room model to the sessions;
  there it first applies `tick`'s effects (and calls `on_command_applied` for a change a person
  made, which the poke now carries) through `ControlState`'s runtime hooks
  (`set_civil_time`, the catalog's `set_group_source`, `start_ramp`, `runtime_volume`,
  `set_alarm_ringing`, `sleep_expired`, each of which commits, plans the slots and pokes the
  conductor again), sends any `SlotCommand` an effect names (a chime or a line-in input: the enum
  grows), and then does what it does now. A ramp step is a `room_volume` with a `ramp_ms` the
  router sends exactly as it sends today's; ADR 0074's rule for ramps longer than 60 s applies.
  `--civil-time` then becomes the runtime's fixed-clock test input (or is replaced by its
  `--civil-time-from`).

## Not chosen

- **One process per group** (the `--group-audio` shape) for every endpoint: the C endpoint has one
  server address and cannot be told to move.
- **A chunker per slot on its own thread**: S threads, a population that depends on S, and S
  timelines that would have to be kept in lock-step instead of being one by construction.
- **Moves done on the audio thread** (a command per session): the same guarantee as the grid
  guard, with the session table copied onto the real-time thread.
- **Refusing to start** with more needing groups than slots: a restart with a persisted state
  file would then fail on what a person had set; source `none`, reported, is visible and
  recoverable.
- **Removing the direct `controller_state` answer** in favour of the conductor's push: a command
  that changes nothing (volume up at the limit) is still owed an answer.

## Follow-ups (the integration track)

- The schedule runtime (above), chimes and line-ins as slot inputs, the line-in source handler,
  autoplay, the latency-growth plan wired into the line-in input (ADR 0071).
- Measure the audio thread's tick at S = 32 on the target host (a `docs/measurements/` report,
  `Source: host`): this change claims no timing, and S broadcasts per tick are not yet measured.
- The FIFO source waits up to half a chunk inside a tick when its writer is idle (`source.rs`);
  in the slot shape that delays every slot's tick by as much, absorbed by the pacing. A
  `read_now` bounded by the tick's remainder would remove it.
- The house soak (`tools/house-soak-run.sh`) with `--slots 8`.
