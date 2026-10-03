# 0132: chorus-server reads each Soloist receiver's FIFO on a fixed thread into a port with a fill target, one manager thread runs the pool and takes the room, a receiver is a source no command can name, and the Spotify alarm source ships switched off

- Status: accepted (goal 17, 2026-10-03)
- Decided by: the owner for what is built (P7 Option C; K59, K65, K66, K77, K78, K80, K81);
  the goal (program section 21) inside the coordinator's goal-17 design envelope (section 2.5),
  track `chorus-g17/soloist-server`, for everything else. Every number below that is not cited
  is chorus's own choice and is said to be
- Implemented in: `crates/server/src/soloist.rs` (the manager), `soloistport.rs` (the port),
  `soloistreader.rs` (the readers), `targets.rs` (the targets of a state, moved out of
  `upnp.rs`), and the wiring in `config.rs`, `main.rs`, `conductor.rs`, `control.rs`,
  `slots.rs`; `crates/control` (`Source::Soloist`, the commands `soloist_restart` and
  `playback`, the state's `soloist` member, ten vectors under `fixtures/control/v2`);
  `crates/ctl` (the noun `soloist`); `crates/soloist-fake` (item metadata, a command log, a
  stall control); `crates/server/tests/soloist_receivers.rs`; `docs/soloist.md`

## Context

ADR 0130 gave chorus a supervisor for the owner's Soloist binary, a protocol on a Unix socket, a
FIFO of PCM per receiver and a pure pool. This record is the other side: how `chorus-server`
turns N such receivers into Spotify Connect devices for its rooms and groups, within the
server's own rules (a thread population fixed at start, an audio thread that never waits,
monotonic clocks on the audio path, a catalog whose commands are all vectors).

## What was read

All on 2026-10-03.

- Soloist's documentation under https://developer.spotify.com/documentation/soloist (the
  WebSocket API reference and the command-line reference, through goal 17's research digest,
  which quotes them verbatim) and proposal P7 (`docs/proposals/P7-spotify-soloist.md`).
- Goal 17's PipeWire probe (PipeWire 1.4.2, WirePlumber 0.5.8, the development host; its report
  is the goal's measurement track's): the FIFO's cadence, what an idle sink writes, what a full
  pipe keeps, the quanta lost under load.
- chorus's own code and records: ADRs 0010, 0011, 0012 (the thread inventory), 0050 (the FIFO
  source), 0066 and 0071 (the line-in port), 0075 (catalog v2), 0119 and 0124 (the player
  source and engine), 0125 and 0128 (the renderers), 0129 (stored sources, the Spotify seam),
  0130; `crates/server/src/{upnp,playersessions,playerport,linein,source,slots,conductor,
  control,schedule_runtime}.rs`, `crates/control/src/{rooms,zones,catalog}.rs`,
  `crates/soloist`, `crates/soloistd`, `crates/soloist-fake`.
- No GPL or LGPL source was opened. No Soloist binary or archive was downloaded or run.

## Decision

1. **A receiver's audio crosses to the audio thread through a port of its own, written by a
   thread of its own.** `--soloist-receivers N` makes N `soloist-reader-<i>` threads and one
   `soloist-manager` at start, registered by role, and none after (ADR 0012). The existing
   FIFO source could not be used: it is one per server, read on the audio thread, in the
   server's own format. The reader opens the pipe by the FIFO source's rules, drains it always,
   converts with the media player's remix and resampler, and writes a ring the audio thread
   takes one chunk from without waiting.
2. **The port drops when full, discards when nobody listens, and waits for 120 ms before it
   plays.** It is live audio, so the line-in port's rule (drop and count), not the player
   port's (never drop). A receiver no group plays is drained and discarded, and the ring is
   emptied when a group takes it and when it lets go, so nothing stale is ever heard. The fill
   target is 120 ms: the probe's largest gap between two quanta on a loaded host, 98 ms, plus
   one default chunk. It is chorus's choice from host observations, not a measured optimum, and
   it is one constant. A dry spell under 500 ms (`ASSUMED`) is counted as an underrun; a longer
   one is the music stopping, and the next stream starts on an empty ring.
3. **No rate matching.** The receiver's graph and chorus's timeline run on one host's clock
   (the probe: 44100.07 frames a second). What difference remains becomes a counted drop or
   underrun after hours. A rate-matched resampler, as the line-in has, is the fix if the
   owner's build shows drift; a larger ring is not.
4. **`soloist:r<i>` is a source no command can name.** `Source::Soloist` is exclusive like a
   player (one group at a time, moved by `take`, never copied) and carries a now-playing
   record, `via` `spotify`. The decoder refuses it in `take` and in an alarm by name, because
   a receiver's audio follows the Spotify app; the manager gives a group the source through
   `ControlState::apply_command`, which is `apply` without the decoder.
   `SOURCE_SPELLINGS` is unchanged, so no refusal vector moved.
5. **Take the room is level-triggered and told in order.** While a target's receiver says
   `playing` and the target's group does not play it, the manager pauses and deactivates every
   other receiver a room of the target hears, and then issues the `take`. A group that stops
   playing a receiver gets it paused. The manager's own `pause` is followed by a 3 s settle
   (`ASSUMED`) in which a `playing` report is left alone. A level, not an edge, so a missed
   event (the server was restarted, a supervisor reconnected) is corrected at the next look.
6. **The overlap rule is read literally in both directions.** When a room's own device takes
   the room out of a group, the group's device is paused too, as the design's sentence says
   ("every room of T whose current group plays another receiver"). The rooms left in the group
   go quiet. Not chosen: leaving the group's device playing for the rooms that stay. That would
   be friendlier with two Spotify accounts in one house and is the first thing to revisit on
   the owner's build; it needs the level rule to learn which rooms a group "should" hold.
7. **`playback {target, action}` is a new catalog command.** The catalog had no transport
   command: a UPnP cast is controlled by its control point. It carries pause, resume, next and
   previous to the receiver a group plays and is refused for any other source. One command
   with an action, not four, so the next transport verb is a word and not a message type.
8. **`soloist_restart` and the `soloist` state member follow `firmware_rescan` and
   `firmware`.** The command is agreed by the room model (there are receivers) and carried out
   by `ControlState`; the member is written only by a server that runs receivers, after
   everything else, so no committed state vector moved.
9. **Volume has two mappings behind `--soloist-volume`, default `chorus`.** P7 left the gain
   stage to the owner's build. Under `chorus` a Spotify volume sets the target's volume through
   the catalog (so the limits clamp it) and chorus's volume is sent back with `set_volume`;
   under `receiver` chorus only clamps. One echo rule serves both: a `volume_changed` equal to
   what the manager last sent is not applied. A snapshot's volume is a fact, not a change, and
   is never applied to chorus.
10. **The Spotify alarm source is off unless `--soloist-alarms`.** P7: the Developer Policy's
    alarm clause is the owner's to read first. On, the manager checks the receiver (running,
    not expired or within a day of it, logged in), sends `play`, and answers the conductor when
    `playing` arrives or after 10 s; each failure is a reason the chime rings with.
11. **Generations are the wall clock in nanoseconds at start, then counted.** A supervisor
    outlives a server restart. A counter from 1 would repeat, and a repeated generation makes
    a new assignment equal to the status the supervisor already sent, which it then does not
    send again (found by the restart test, when two servers started in one second).
12. **The flags are inert beside `--soloist-receivers 0`.** They are refused when the flag is
    absent, as `--upnp-*` are, and do nothing beside an explicit zero, so a deployment turns
    the receivers off by changing one number.
13. **Announcements pause a Soloist source; they never duck it** (goal 20's rule, recorded here
    with `Source::is_soloist`). Nothing is built for it yet.

## Not chosen

- One thread polling every FIFO and socket: one slow conversion would starve the other
  receivers' pipes, and the pool's bookkeeping does not belong beside PCM.
- A `PlayerPort` for the receiver: its writer waits for room and never drops, which for live
  audio only moves the loss into the pipe.
- An edge-triggered take (act on `playback_changed` alone): one event missed across a
  reconnect leaves a device playing unheard for ever.
- Waiting for the displaced receiver to confirm `paused` before the take: the manager would
  hold every other receiver's events for a device that may never answer. The room's slot
  changes port at a chunk boundary, so the two signals cannot share a chunk either way (the
  take-the-room test asserts it).

## Consequences

- A server with receivers has `N + 1` more threads and N more one-second rings, whether or not
  a receiver container runs.
- What Soloist really does (the gain stage, the cadence into PipeWire, what a paused device
  answers) is assumed and listed in `docs/soloist.md`; each assumption fails safe.
- `crates/server` has two examples that are test programs. `cargo build --bins`, which every
  image and release build uses, does not build them.
