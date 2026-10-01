# 0000: the server analyses each watched slot's audio on the audio thread and sends visualizer frames and colours to the slot's visualizer sessions, stamped when their room hears it

- Status: accepted (goal 12, 2026-10-01)
- Decided by: the goal (brief section 16 item 5, done-when line E; K65) through the
  coordinator's goal 12 design envelope ("Phase B tracks", `chorus-g12/visualizer`); every
  default below not measured or cited is ASSUMED
- Implemented in: `crates/dsp/src/visualizer.rs` (new: the analysis), `crates/server/src/slots.rs`
  (an analyser per slot, run after each tick's broadcast), `crates/server/src/router.rs`
  (`watched`, `push_visualizer`, `set_heard_latency`, the per-session bands, heard latency and
  last colour), `crates/server/src/conductor.rs` (`heard_latency_ns`, `with_transports`),
  `crates/server/src/session.rs` and `control.rs` (the endpoint's `visualizer_bands` into its
  `SessionStart`; the session line names it), `crates/server/src/main.rs`,
  `crates/client-linux/src/{config,session,main}.rs` (`--visualizer-bands`, the log lines, the
  front panel's LED on the server timeline); `audio-path.conf`; `fixtures/visualizer/`; the
  contract is `docs/visualizer.md` and `docs/protocol.md` "Visualizer and colour"; held by
  `crates/dsp/tests/visualizer_fixtures.rs` (2 tests over 4 fixtures), unit tests in
  `visualizer.rs` (7), `router.rs` (1), `conductor.rs` (1), the client's `config.rs` (1) and
  `session.rs` (1), and `crates/server/tests/visualizer_stream.rs` (1, the real binary)

## Context

The protocol has carried `visualizer_frame` (0x34) and `color` (0x35) since goal 5, both
endpoint kinds show them on their status LED (ADR 0063, ADR 0067), and nothing sent them. Goal 12
puts the analysis in the DSP library and the stream on the wire; goal 10 left the Linux LED's
server timeline to it.

## What was read

All 2026-10-01: the research `docs/research/research-dsp-phase-b.md` section 2 (and through it
Dixon, "Onset Detection Revisited", DAFx-06, read in full here: sections 2, 2.1 and 2.6; Bello
et al. 2005; IEC 61260-1:2014's preview pages; EBU Tech 3205-E and 3341; Richan and Rouat 2020;
WCAG 2.2 SC 2.3.1; WLED's audio sync documentation page, no WLED source opened); chorus's own
`docs/protocol.md`, ADRs 0063, 0067, 0071, 0074, 0077, 0079, `crates/server/src/*`,
`crates/client-linux/src/{session,main,config,sync,front_panel}.rs`, `crates/controls/src/led.rs`,
`config/sync.conf`, `crates/control/src/transport.rs`. No GPL source.

## Decision

1. **The analysis is a pure library** (`chorus_dsp::visualizer::Analyzer`): 10 ms hops (Dixon's
   100 Hz), a Hamming window of the power of two nearest 46 ms, a frame every 4 hops (25 a
   second). Per frame: the held sample peak, 60 IEC 61260-1 sixth-octave band levels merged by
   power into the bands an endpoint asked for, a beat from Dixon's spectral flux and his first
   two peak-picking conditions on a running normalisation, and now and then a colour (hue from
   the log spectral centroid, blue to red; saturation from the flatness; brightness from the
   level). It reads no clock: a frame's instant is a sample index. `docs/visualizer.md` has
   every value and its source.
2. **The server computes it on the audio thread**, per slot, over the chunk that slot just
   played, after the tick's broadcast and outside the grid guard (a move takes the sessions lock
   and then the guard; the audio thread never waits on the sessions lock while holding the
   guard). Only a slot with a visualizer session is analysed (`Router::watched`, an atomic
   count per fanout, no lock); a slot that gains one starts from scratch. No thread is added, so
   the declared thread population is unchanged.
3. **Each session is sent the frame stamped when its room hears it**: the frame's instant on
   the slots' grid plus the session's heard latency, the playout latency of its room's tier (180
   ms wired, `config/sync.conf`; the wireless policy's 500 ms), which the conductor sets on every
   pass from `--zone <id>=<transport>` (a room declared without one is wired; before the first pass, wired). Its
   bands are its own `visualizer_bands` (at most 60). A `color` goes with a frame that carries
   one and only when it differs from the last that session was sent.
4. **Only the role receives it.** A session without `visualizer` is skipped; a session in no
   room is on the silent fanout, which is never analysed. A run of silent frames sends its first
   and then nothing. A full queue drops the message and counts it, never retries.
5. **The Linux client** takes `--visualizer-bands <n>` (1 to 64; above is refused by name): the
   role, `n` bands in `capabilities`, and a log line per beat and per colour. Its front panel's
   LED now shows frames against the server timeline: the monotonic now plus the offset the
   running session's playout loop publishes (`Counters::offset`, which the source role already
   reads), through one process-wide slot set for the session's life.

## Not chosen

- **A thread of its own for the analysis.** It would add a declared thread and a queue of PCM
  between it and the audio thread; the analysis is two 2048-point FFTs per 20 ms chunk per
  watched slot, small beside the chunk's own encode, so it runs where the PCM already is.
- **50 frames a second** (the research's recommendation, after WLED). A frame arrives up to the
  wireless tier's 500 ms before it is heard and both LEDs hold 16 events; 50 a second would
  overflow them, 25 fills 12.5 (ASSUMED).
- **Dixon's third condition (`g_alpha`)**: marginal by his own account, and on a causally
  normalised function the first onset after silence would hold it up for seconds.
- **The research's `delta` of 0.1**: with it a steady tone's flux ripple can beat; 0.5 keeps the
  tone and sweep fixtures beat-free (both ASSUMED).
- **The research's 20 ms analysis hop**: Dixon's cited 10 ms is used; frames are still 40 ms apart
  but a beat frame is stamped at its onset hop, so a beat's stamp has 10 ms resolution.
- **K-weighted brightness** (the research's recommendation): unweighted for now, a follow-up
  once `crates/dsp`'s biquads are wired in for it.

## ASSUMED values

The frame rate (25 a second), the 60 dB display span, the PPM fall applied to bands, `delta`
0.5, the 3 s running normalisation, the -60 dB flux and deviation floor, the beat scale (64 per
deviation), the colour interval (500 ms) and its change steps (15 degrees, 0.1, 16), the centroid
ends (100 Hz, 8 kHz), unweighted brightness, and the end-to-end tolerance (the fixture's 20 ms
plus one 10 ms hop). Each is marked in the code beside it.

## Consequences

- Done-when line E: `crates/server/tests/visualizer_stream.rs` writes the 120 BPM kick fixture
  into the real server's named pipe; the den's visualizer endpoint receives one beat per kick at
  0, 500, 1000 and 1500 ms of the fixture's start on the server timeline as heard (tolerance 30
  ms), 16 bands each and colours; a visualizer endpoint in a wireless room playing the same
  stream on the other slot receives the same beats stamped 500 ms after instead of 180; the
  plain player and the endpoint in no room receive no 0x34 or 0x35. Not timing evidence: stamps compared with stamps on one host.
- A visualizer session's queue carries up to 25 more items a second than a player's, so its
  128-item ceiling holds 1.7 s of audio rather than 2.56 s.
- Follow-ups: the one-stream shape (`--slots 0`) sends none; K-weighted brightness; a bench
  measurement of an LED flash against the kick a microphone hears; Home Assistant lights from the
  `color` stream.
