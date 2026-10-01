# 0093: a TV played into one wired room goes hub to server to players as UDP datagrams, offered to the players before the hub, restamped capture plus the room's trimmed L_tv on one relay thread, played at its stamps with a one-chunk fade over a gap, and every other case stays on the slot grid by name

- Status: accepted (goal 13, 2026-10-01)
- Decided by: the goal-13 design envelope, section 5 (Integration), track `chorus-g13/tv-path`;
  the follow-ups ADRs 0087, 0088, 0090 and 0091 handed this track; BRIEF.md 5.7 ("UDP + simple
  XOR parity FEC, wired only, stereo first") and section 3.1 (monotonic clocks in the audio
  path, the measurement rule); the placements below where the envelope left them open
- Implemented in: `crates/server/src/tvrelay.rs` (new: `TvRelay`, `TvPlay`, `RelaySetup`,
  `Loss`), `conductor.rs` (`tv_path`, `with_tv_relay`), `config.rs` (`--low-latency-port`,
  `--tv-latency-ms`, `--test-tv-latency-ms`, `--fec-k`, `--fec-depth`, `--udp-loss`,
  `low_latency_plan`), `main.rs` (the socket and the `tv-relay` thread), `router.rs`
  (`set_link`, `SessionView.features`/`peer`), `session.rs` (`Greeting.features`, 0x17 to the
  relay), `clients.rs`, `linein.rs` (`tv_source`), `control.rs` (`RoomView.av_trim_ms`,
  `SlotGroup.low_latency`); `crates/client-linux/src/lowlat.rs` (new: `LowLatPlayer`, the
  jitter buffer and the concealer), `run.rs` (the low-latency branch of the playout loop),
  `source.rs` (`LowLatUplink`, the hub's datagram upstream), `session.rs` (the feature bit,
  the 0x16 taps), `config.rs` (`--low-latency`, `--low-latency-target-us`), `control.rs`
  (`ZoneWatch::set_low_latency`), `sink.rs`/`dsp.rs`/`outmap.rs`
  (`PcmSink::fixed_latency_frames`), `main.rs`; `audio-path.conf`; the workspace
  `Cargo.toml` (third-party crates optimised in debug); held by
  `crates/server/tests/tv_low_latency.rs` (3 end-to-end tests on the real server binary),
  `tvrelay.rs` (4 unit tests), `crates/client-linux/src/lowlat.rs` (4) and `session.rs` (1)
  unit tests, `control_thread_population.rs` (the relay thread). The contract is
  `docs/protocol.md` ("What the server and a Linux endpoint do"), `docs/control-plane.md`
  ("The TV relay") and `docs/linux-endpoint.md` ("The TV path's low-latency stream")

## Context

Goal 13's four tracks merged the parts of the TV path: the wire and its FEC (ADR 0091), the
hub's rate-matched capture (ADR 0090), CEC on the hub (ADR 0087), the theater maps, the A/V trim
and TV autoplay (ADR 0088). Nothing yet carried a TV's sound over the wire: the server had no UDP
socket, the client did not take an offer, and the hub still sent 20 ms chunks up its session. This
record is that integration and what its end-to-end tests found.

## What was read

Main at this record's merge includes #92 (ADR 0092: an autoplay is kept through the hub's CEC
volume and mute, `stereo_downmix` is set, the `chorus-udp-loss` probe and bench session S8); this
change was merged onto it and its tests run there.

All 2026-10-01: the envelope and the tracks' follow-ups; ADRs 0004, 0066, 0071, 0079, 0087,
0088, 0090, 0091; `docs/protocol.md` "Low-latency path"; the code each "Implemented in" names and
the end-to-end tests it copies its harness from (`tv_capture_rate_match.rs`, `cec_tv.rs`,
`dsp_end_to_end.rs`). No GPL source was opened; nothing outside this repository was consulted
for a number used here (every new number is ASSUMED, below).

## Decision

1. **When.** The conductor's pass plays a TV input in low-latency mode when the input is a TV's
   (`optical`, `hdmi_arc`) and streaming, its autoplay rule (if any) says `low_latency`, its
   group is one room, the room is wired, the server's chunk fits one datagram, and the hub and
   every player of the room advertise `low_latency`. Otherwise the slot path (ADR 0079) plays it,
   unchanged. Each change of mode is one line: `tv-path mode=low-latency input= room=` or
   `tv-path mode=slot input= group= reason=` (`rule`, `grouped`, `wireless`,
   `chunk-does-not-fit`, `no-player`, `player-not-capable`, `hub-not-capable`, `refused`).
2. **Players first, then the hub.** The relay offers each player (`0x16 to_endpoint`, a fresh
   key from `/dev/urandom` and a process-unique tag each) and offers the hub (`from_endpoint`,
   its own port) only when every player accepted, so a hub never switches its upstream to
   datagrams nobody can play. A refusal, or an offer unanswered for 2 s, ends every stream of
   the play (`0x16 end`) and keeps it on the slot until its rooms, sessions or hub change.
3. **The relay** is one thread (`tv-relay`, created with the rest of the population, so the
   documented count with `--slots` is `6 + 2N + M + 1`) on one UDP socket beside the audio
   listener. Per hub datagram: the hub's address and tag, `Opener`, `FecDecoder`; per chunk
   handed on: `play_at = capture_stamp + lead`, `lead =
   chorus_control::theater::tv_play_at_lead_ns(L_tv, floor, av_trim_ms)` (`av-trim-clamped`
   when the floor holds it), a chunk whose `play_at` has passed is never sent, then each
   player's `FecRelay` and `Sealer`. `expire_before` closes each block whose play-at passed,
   reckoned from the newest chunk's (number, stamp), so a hub's relock (a fresh stamp grid, ADR
   0090) moves the reckoning with it; the relay says each such move (`tv-relay
   capture-grid`). A trim moves the lead in place (`tv-relay lead`). Monotonic server timeline
   only; listed in `audio-path.conf`.
4. **The player** (`LowLatPlayer`) takes `to_endpoint` only when wired and on, answers with a
   UDP port, keeps datagrams from the server's address alone, and holds the FEC's chunks by
   stamp. The playout loop, while a stream plays, discards the slot path's chunks, skips the
   servo's corrections (the exchange and the offset go on), lets the device drain to a small
   target (`--low-latency-target-us`, 4 ms, above the sink's own fixed latency, the sound
   chain's 2 ms look-ahead: `PcmSink::fixed_latency_frames`) and writes the chunk stamped at the
   instant the next frame is heard (the delay read's instant plus the delay, on the server
   timeline); a missing chunk holds the last frame and fades it to silence over one chunk, the
   next fades in over one chunk (never a hard cut); a late chunk is dropped and counted; a play
   position more than 0.5 ms off is moved back. Volume and sound apply as on the slot path.
5. **The hub** answers `from_endpoint`, cuts its rate-matched output into the offer's
   `chunk_frames` (`TvFrontEnd::set_chunk_frames`) and sends each chunk as datagrams
   (`FecEncoder`, `Sealer`) to the server's port instead of up the session; `end` returns it to
   20 ms chunks on the session. The CEC standby reason (ADR 0087's follow-up) was already on
   main from the capture track; nothing to add.
6. **Test-only levers**, each named so: `--udp-loss <ppm>,<seed>` drops datagrams from a seeded
   splitmix64 on both legs (each received from a hub, each sent to a player);
   `--test-tv-latency-ms` lets `L_tv` leave 10..40 ms (never below the floor); `--fec-k 0` is
   the negative control.
7. **The budget's capture term is the hub as built, and `L_tv` is 25 ms** (the coordinator's
   binding note; ADR 0091 assumed 2.0 ms of capture buffering, two 1 ms periods). The hub
   (ADR 0090) reads 240-frame (5 ms) periods and sends a frame once it is 2 ms
   (`SEND_DELAY_NS`) old, so a chunk's last frame leaves up to one period plus 2 ms after it
   was digitized: `DEFAULTS.capture_ns` is 7 ms, the floor 24.417 ms, and `DEFAULTS.l_tv_ns`
   25 ms (the interleaved scenario 35 ms over its 31.917 ms floor). The other choice, a capture
   period at or under the chunk in low-latency mode, was not taken: with the 2 ms send delay
   kept, even 1 ms periods give a floor of 20.417 ms, over 20; it would need the period and
   the send delay changed together, a re-derivation of the DLL and ratio-loop constants and
   ADR 0090's 10 tests on another period rate, for 4.6 ms. Lip sync stays inside BRIEF.md
   2.2's [-40, +15]: -(1 + 0.0625 + 25) = -26.06 ms at the defaults (-36.06 ms interleaved);
   the game-mode TV scenario was outside before (-86.06 ms) and is outside now (-91.06 ms),
   as the report says. `docs/measurements/low-latency-budget-sim.md` is regenerated
   (simulation, not timing evidence); `crates/sync/tests/lowlat_budget.rs` pins the new lip
   sync. Bench S8.4 measures the capture term.
8. **Third-party crates are optimised in debug and test builds** (`[profile.dev.package."*"]
   opt-level = 2`). Unoptimised, the RustCrypto AEAD could not seal and open the end-to-end
   tests' 3000 datagrams a second on the shared host: the relay fell behind until it was
   discarding late chunks, which pinned the chunks' age at the relay to `L_tv` itself
   (measured: mean 38.7 ms at `L_tv` 40, 107 ms at 110). Optimised: 8.5 to 18.7 ms. chorus's own
   crates keep the debug level.

## Evidence

Test runs on a shared 4-core host at a load average of 11 to 21 (other jobs), the real
`chorus-server` binary, every endpoint the shipped client's code on modelled devices. Not timing
evidence (BRIEF.md 3.1 rule 3): every process shares one clock, the TV and the DACs are modelled.

- `tv_low_latency.rs`, a theater set (FL FR FC LFE SL SR) on `L_tv` 110 ms, 1e-3 loss on both
  legs: FL plays the left (1 kHz at 0.00 dB, the right's 1.5 kHz at -81.4 dB), FR the right
  (-0.01 dB; the left at -78.1 dB), FC both at -3.01 and -3.02 dB ((L + R) / sqrt 2), the LFE
  50 Hz at +4.8 dB and the mids under -86 dB, SL and SR digital silence (`tv_upmix` off); every
  one of 1666 chunks received was stamped capture + 110 ms; a +50 ms trim made it 160 ms and a
  -100 ms trim clamped at the floor 24.416667 ms with `av-trim-clamped`; before the trims the six
  players played 9824 chunks, rebuilt 10, lost none; joining the den's group put it back on the
  slot (`reason=grouped`) and the players back on TCP. The three tests run one at a time: run
  together on the shared host (load average above 30 at the time) the hub's capture thread was
  starved into ring underflows and nothing reached the relay in time.
- The same, two players, 6 s each: 1e-3 with FEC: 12 datagrams dropped, 3 rebuilt at the relay and 9 at the players, 0 lost;
  1e-2 with FEC: 100 dropped, 24 rebuilt at the relay and 58 at the players, 2 lost (two
  losses in one group); 1e-2 without
  FEC (the control): 86 dropped, 26 chunks lost at the relay and 112 at the players (the
  relay's included, once per player).
- CEC on the fake bus (the configured default `L_tv`, 25 ms): the theater, in the den's group, is
  taken out of it and played in low-latency mode when the TV powers on; every chunk received is
  stamped capture + 25 ms; the standby stops the autoplay (`stopped reason=standby`), ends the relay and
  restores the theater.

## ASSUMED values

The relay's offer timeout 2 s; its receive wait 1 ms; the UDP port, the audio port + 1 when
`--low-latency-port` is not given (ephemeral with an ephemeral audio port); a TV input with no
autoplay rule played by a command is wanted in low-latency mode (the rule's default; `take` has
no field of its own); the player's device target 4 ms (the budget's endpoint output path), its
top-up wait a quarter of the target in 1..5 ms, its fade one chunk (2.5 ms), its re-snap 0.5 ms,
its receive wait 5 ms, its jitter buffer cap 400 chunks; the tests' 30 ms device target and 110
ms `L_tv` (a loaded shared host, see Evidence). The TV-dependent ones wait on the Needs item "The
three TVs: model, eARC port, optical out and audio menu".

## Deviations from the envelope

- The envelope's "min fill sized by the sim's budget" is not a fill: the stamps already carry the
  lead, so the player plays each chunk at its stamp and holds nothing before it.
- The slot that still carries the room's group hears nothing from the hub while the relay plays
  (its port underruns and counts it); the players ignore the slot's chunks for that time.
- ADR 0091's defaults change: the capture term 2 to 7 ms and `L_tv` 20 to 25 ms (decision 7).
  On the test host a chunk reached the relay 8.5 ms after its capture stamp on average in the
  least loaded run, consistent with the new term (not timing evidence).

## Not chosen

- **Offering the hub and the players at once**: a hub accepting first switches to datagrams the
  relay cannot yet send anywhere; the room hears silence until the slowest player answers.
- **A thread per play, or per player**: one relay thread is the population's one more thread,
  whatever is played; a play is a few hundred datagrams a second.
- **The servo correcting the low-latency stream**: its error is formed from a 120 ms buffer's
  stamps; the low-latency stream is placed by its own stamps and re-snapped instead.
- **Raising the plan's `L_tv` range for the tests**: the range is the product's; a test-only
  flag says what it is.

## Follow-ups

- Firmware: the UDP socket, the session wiring of 0x16/0x17 and a long-lived PSA key per stream
  (ADR 0091; the C layer and its vectors are in). Until then the C endpoint never advertises
  `low_latency` and a room with one plays the TV on the slot path (`player-not-capable`).
- Bench S8: the hub's capture-to-send latency against the floor's 7 ms term, the ALSA period a
  real DAC needs to hold the 4 ms target, the relay and the legs on the owner's LAN.
- A `low_latency` field on `take` (a catalog change with vectors), if a command should choose.
- 5.1 at s24 needs 60-frame chunks (`chunk-does-not-fit` today); the TV path is stereo.
- The relay's counts in the control plane's telemetry, beside the log lines.

## Revisit when

Bench S8 measures the hub's capture latency or a real DAC's period, the firmware takes the
stream, or a 5.1 TV input is built (Option B).
