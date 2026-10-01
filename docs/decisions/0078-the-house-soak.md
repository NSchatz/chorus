# 0078: the house soak runs one --slots 8 server and ten real endpoints on ALSA null under a seeded command load, grades what a house must keep for an hour, and writes a host report that says it is not timing evidence

- Status: accepted (goal 11, 2026-10-01)
- Decided by: the goal (goal 11 item 5 and line E; the integration plan's "Soak" paragraph)
  inside the coordinator's goal-11 design envelope; every bound below not cited is ASSUMED
- Implemented in: `tools/house-soak-run.sh` (`make verify-house-soak`),
  `tools/house-soak/load.py` (setup, the state recorder, the command load),
  `tools/house-soak/report.py` (the grade and the report); documented in `docs/house-soak.md`;
  its refusal held by `tools/unrun-checks-are-visibly-unrun.sh`; the client fix in
  `crates/client-linux/src/control.rs`, held by its test
  `an_attach_turned_away_is_tried_again_while_the_stream_is_held`

## Context

Goal 11's line E asks for "a software soak of at least one hour at 8 rooms ... committed,
labelled, with its duration". ADR 0077 made one server serve every group from stream slots and
named the house soak as its follow-up. `tools/soak-run.sh` is a different thing (AC-4: three days
on real endpoints with the capture rig) and keeps refusing here, unchanged.

## What was read

All 2026-10-01, all chorus's own: the goal program's goal 11 section and line E; the goal-11
design envelope and integration plan; ADRs 0050, 0074, 0075 and 0077; `tools/alsa-null-run.sh`,
`tools/soak-run.sh`, `tools/restart-storm-run.sh`, `tools/control-plane-run.sh`, `tools/lib.sh`,
`tools/unrun-checks-are-visibly-unrun.sh`; `tools/conventions/check-measurements.sh` and
`check-provenance.sh`; `docs/control-plane.md` ("The thread population", "Stream slots", "Event
streams and the event writer"); `docs/measurements/sim-house-8-rooms.md` and the measurements
README; `fixtures/control/v2/*.json`; `crates/client-linux/src/{config,delaylog,run,control}.rs`,
`crates/control/src/zones.rs` (`change`, `bond`), `crates/server/src/config.rs`, and the raw
files of this change's short runs. No external
source; no number here rests on one.

## Decision

1. **Real processes, ALSA `null`, loopback.** One `chorus-server --slots 8 --control-listen
   --serve-forever --source tone` (the generated tone never ends, so one source covers the run) and
   ten `chorus-client` processes on ALSA `null`, through the same rootless-libasound loader as the
   gate's ALSA null runs (`use_rootless_alsa`). The client has no fake sink by design
   (`config.rs` refuses `--sink` and its kin), so `null` is how a client runs without a device.
2. **The house.** Eight rooms, ASSUMED names (the simulated house of `sim-house-8-rooms.md`; the
   owner's list is a Needs item); `bathroom` declared wireless with its endpoint on `--transport
   wireless`; `living` and `kitchen` each with two endpoints bonded FL/FR. The Linux client
   attaches at catalog v1 without a link, so the harness attaches the four bonded endpoints first
   with `link: wired` (a v1 attach leaves a declared link as it is) and then bonds them; it also
   asks for a bond in the wireless room and requires the refusal naming the room (K91).
3. **A seeded, table-driven load.** One command every 2 s (ASSUMED) for the window, drawn by
   weight from sixteen kinds (every volume path, mute, join, take with and without a source,
   ungroup, saved groups, limit, quiet hours, alarms and sleep), each built from the state the last
   answer carried. A command the server answers as not a command is counted as unknown and the
   load goes on: the alarms and sleep runtime is wired in parallel, and the table must not break
   on a build without it. Every applied change in what a room hears (its group or its group's
   source) is logged as a source switch.
4. **Criteria.** The server's thread count off `/proc` unchanged and equal to the documented
   `6 + 2N + M`; every endpoint's unchanged; RSS growth at most 8 MiB per process after a 30 s
   warm-up (both ASSUMED); `GET /api/report`'s fanout drops, stalled streams and turned-away
   connections all zero and the harness's own subscriber never cut off; every state's volume at
   or below its effective limit and every `room_volume` an endpoint logged with gain at or below
   its limit; no underrun or hard resync in the window that is not within 5 s (ASSUMED) of a
   logged switch of the endpoint's own room; every endpoint present at the end on the one session
   it opened and stopped by its run length; every command answered 200, 400 or 426; and the
   window's duration measured on `CLOCK_MONOTONIC` with wall-clock start and end.
5. **The report.** Written by `report.py` from the raw files alone, so a kept run directory
   regrades: `Source: host`, the build, date, duration, host facts (kernel, CPUs, cgroup quota),
   every parameter, every criterion with its bound and verdict, and "host software soak on ALSA
   null, not a hardware measurement and not timing evidence". Raw files stay in the run directory
   and are listed with sha256 and size; none is committed (the delay logs are megabytes an hour).
6. **The build it names.** Rule 11 wants a commit in main's history and a squash merge leaves
   branch commits out of it, so the harness names HEAD by default and accepts another commit only
   when `crates/`, the manifests, the toolchain file and `config/` are identical to it (`git diff
   --quiet`), recording that it checked (so a branch that changes only tools or docs can name
   the main commit it was cut from).
7. **Bounded.** The script re-runs itself under `timeout` at the soak plus 600 s, the server under
   the same bound, every endpoint under `--run-seconds`.
8. **Start-up is not the window.** Ten endpoints started in one instant are a small restart
   storm (AC-3's subject): the first short run had 45 control connections turned away while all
   8 workers were busy and three first audio sessions fail (`Resource temporarily unavailable`)
   and rejoin. The control counters and session counts are graded over the window, the
   difference between snapshots at its two ends, and what happened before it is reported beside
   them.
9. **The client retries a turned-away attach (a fix the soak found).** In that same start-up,
   two endpoints' `attach` was answered `503` and never sent again: the Linux client attached
   once per event stream and ignored the answer (`let _ = self.attach()`), so while the stream
   held those endpoints were in no room, heard silence in the slot shape and were never sent a
   `room_volume`. `ControlLink::follow` now retries a failed attach every `RETRY_INTERVAL`
   (500 ms) while it holds the stream, until one is applied. Held by a unit test against a
   control channel that answers the first attach `503`.
10. **Not in the gate.** An hour is too long; a smoke of half a minute would add ten debug
   processes to every gate on a shared host for little the short run does not show. `make
   verify` holds its refusal without a device instead. The hour is run by the coordinator under
   the heavy lock and committed as `docs/measurements/house-soak-8-rooms.md`.

## The short run in this change, and why it is not committed

The harness's test is a 120 s run of the whole thing (the PR carries its summary; its raw files and
its generated report are kept under the goal's report directory). Its report is NOT committed,
because rule 11 wants `Build measured:` to be a commit in main's history and no such commit holds
the binaries it ran: this change also fixes the Linux client (decision 9), and a squash merge
leaves the branch's commits out of main. The one-hour run, made on main after this merges, names
the merge commit and is the report goal 11's line E points at
(`docs/measurements/house-soak-8-rooms.md`).

## Not chosen

- **Extending `tools/soak-run.sh`**: it is AC-4's three-day run and its refusal is quoted in the
  verification record; a software mode there would blur what it refuses.
- **A FIFO source fed by the script**: the tone runs for ever on the server's own timeline with no
  writer to stall (ADR 0077's follow-up notes a FIFO's idle writer delays every slot's tick).
- **Protocol sessions opened by a test harness** (`common::Player`) instead of the client binary:
  the soak is of the shipped binaries.
- **Grading underruns as zero outright**: a source switch can legitimately cost a start fill;
  attributing them keeps the criterion honest without a false red.

## Follow-ups

- The coordinator's one-hour run and its report (`docs/measurements/house-soak-8-rooms.md`).
- The start-up storm: with 8 control workers, ten endpoints arriving together have connections
  turned away and first sessions failing with `EAGAIN` before they rejoin. The rejoin works (AC-3);
  why a first session's handshake times out under that load is for the server's owners to look at.
- The audio thread does not count its tick overruns; once it does (ADR 0077's S = 32
  measurement), the soak grades them.
- When the line-in source handler lands, one endpoint with a line-in on an ALSA file plugin
  input, as the integration plan sketches.
- The three-day bench run on real endpoints is the owner's (`docs/house-soak.md`).
