# 0173: the announcement mixer is a pure core in crates/dsp that runs on the server before a room's stream is encoded, with no C mirror; it ducks 20 dB in 200 ms, restores in 500 ms and plays the clip at one minus the duck gain

- Status: accepted, 2026-10-05
- Decided by: the task for the scope (a pure library core that ducks, mixes and restores,
  "with ramp times and duck depth as parameters and the output never above the input limit";
  "an ADR for the defaults (duck depth, ramp times)"; a C implementation only if this record
  places the mix on the endpoint); K31 ("announcements/ducking (TTS/notification over playback,
  duck and restore, HA-driven)"); this record for the cheap decisions: where the mix runs, the
  envelope, when the clip starts, what a cancel does, how the limit is held, and the defaults.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `crates/dsp/src/duck.rs` (`Duck`, `DuckParams`, `DuckState`, the `DEFAULT_*`
  constants); `fixtures/dsp/duck/`; held by `crates/dsp/tests/duck_fixtures.rs` and the unit
  tests of `duck.rs`; `docs/dsp.md` ("The announcement mixer"); `firmware/tests/test_dsp.c`
  (its walk of `fixtures/dsp` passes a subdirectory by).

## Context

0136 gave chorus an `announce` command that stops the music for the clip and said the ducking
mixer is a later goal's, which "replaces decision 4's playback". This record is the mixer's
core: the arithmetic that takes a music stream and a clip and gives the mix. It is a library
with nothing calling it; the server's wiring, the command's use of it, and fetching and decoding
a clip are later tasks. A room whose source is a Soloist instance is paused for an announcement
and never reaches this code (0130 decision 10, 0132 decision 13; the owner's answer of
2026-10-04 keeps that).

## What was read

All on 2026-10-05. Documentation pages only: no GPL source and no reciprocally licensed design
was opened (ESPHome and Audacity are read through their published manuals alone).

- ESPHome, "Mixer Speaker": <https://esphome.io/components/speaker/mixer/> (the
  `mixer_speaker.apply_ducking` action: `decibel_reduction` "Must be between 0 and 50",
  `duration` "Defaults to `0s`", the example `decibel_reduction: 20`, `duration: 2.0s`).
- Audacity Manual, "Auto Duck": <https://manual.audacityteam.org/man/auto_duck.html> (duck
  amount -12 dB; fade down and fade up lengths, each "default: 0.5 seconds").
- Android Developers, "Manage audio focus":
  <https://developer.android.com/media/optimize/audio-focus> ("The start of the notification
  playback is synchronized with the end of the ducking ramp"; ducking is not useful for spoken
  content, which pauses instead).
- In this repository: `crates/dsp/src/{lib,chain,limiter,fixture}.rs`,
  `crates/dsp/tests/shared_fixtures.rs`, `firmware/tests/test_dsp.c` (its fixture walk),
  `crates/schedule/src/ramp.rs` (the existing ramp: integer, exact at both ends),
  `docs/dsp.md`, `fixtures/README.md`, `tools/conventions/check-shared-fixtures.sh`,
  `docs/decisions/0082-the-dsp-library.md`, `0136-a-server-identity-and-an-announce-command.md`,
  `docs/soloist.md` ("Announcements pause, they do not duck"),
  `docs/proposals/P7-spotify-soloist.md`, `docs/proposals/P8-voice-path.md`.

## Decision

1. **The mix runs on the server, on a room's stream before it is encoded.** Every endpoint of
   the room then plays one mixed stream on the room's one timeline: the clip is in sync across
   the room for the reason the music is, an endpoint needs no second stream, no second decoder
   and no new message, and both endpoint kinds get announcements at once. The endpoint's chain
   is unchanged, so the room gain and the limiter at the room's limit (K81, I10) still act on
   what is played, clip included. So there is no C mirror, and the core lives in `crates/dsp`
   beside the other two analyses the server runs (`roomfit`, `visualizer`).
2. **The fixtures are `fixtures/dsp/duck/`, Rust-only by declaration.** Both walks of
   `fixtures/dsp` fail on a kind they do not know, so a file the C mirror cannot run does not
   belong among them; a subdirectory keeps the mixer's contract beside the DSP library's and
   out of the shared walk. The Rust walk already skipped directories; the C walk now does too.
   `crates/dsp/tests/duck_fixtures.rs` reads every file there and fails on one it does not
   know. Expected frames and gains are worked by hand from the envelope, never taken from the
   implementation.
3. **One integer level drives both gains.** `level` runs from 0 to `D x R` (`D` the duck ramp,
   `R` the restore ramp, in frames); a ducking frame adds `R`, a restoring frame takes `D`. So
   the way down is exactly `D` frames and the way back exactly `R`, both ends are exact (the
   idea of `crates/schedule`'s ramp), and a ramp turned round in the middle goes on from where
   it is. The music's gain is `1 - (1 - duck_gain) level / (D x R)`, linear in amplitude,
   computed in `f64` and rounded to `f32` once.
4. **At level 0 the frame is copied.** The restored music is its input bit for bit, which a
   multiply by a gain that only rounds to 1 would not promise.
5. **The clip starts when the duck ramp ends**, on frame `D` after the start, as Android's
   audio focus does. A clip mixed in during the ramp would either push the sum above the limit
   or have to fade in and lose the start of a word.
6. **The end of the clip is said, not guessed.** `finish` says the clip has no frames beyond
   those handed in, and the frame after its last begins the restore. Without it, a block with
   no clip frame holds the music ducked under silence: a decoder that is late does not bounce
   the music up and down.
7. **A cancel restores from where the music is, and fades the clip.** On the way down no clip
   frame has played and none does. While the clip plays, its gain follows the level down over
   the restore, so it is not cut.
8. **The limit is held by construction, then clamped.** `duck_gain + clip_gain <= 1` is checked
   when the parameters are made, and the two gains sum to at most 1 on every frame, so inputs
   inside the limit mix to a sum inside it; a clamp takes the last rounding. Idle frames are
   not clamped: the mixer does not touch music it is not mixing. The limiter's own behaviour is
   not this record's.
9. **No clock.** Every length is a frame count; milliseconds exist only in
   `DuckParams::from_ms`, a pure function of a rate. An event lands on a frame by ending the
   block there.
10. **The defaults** (each a constant in `duck.rs`, each in `docs/dsp.md`'s table):
    - duck depth -20 dB: the ESPHome mixer's documented example, and the deeper of the two
      figures read (Audacity's Auto Duck defaults to -12 dB), because the clip is speech;
    - restore ramp 500 ms: Auto Duck's default fade length;
    - clip gain `1 - duck_gain` (0.9 at the default depth), rounded down: computed, the most
      decision 8 leaves;
    - limit 1.0, full scale.

## Considered and not chosen

- **Mixing on the endpoint.** It would let a clip start without the stream's buffer delay, at
  the price of a second stream to every endpoint, a C mixer and a second decoder on the chip,
  and a clip that is in sync only if the second stream is scheduled like the first. Nothing in
  K31 asks for a start faster than the room's buffer.
- **Fixtures directly in `fixtures/dsp/`.** The C walk would fail on them, or would have to
  name a kind it skips, which is the "fixture nobody runs" the directory's rule exists to stop.
- **A new top-level `fixtures/duck/`** as `roomfit` and `visualizer` have. The task asks for
  the files under `fixtures/dsp/`; a subdirectory gives that without weakening the shared walk.
- **An equal-power or logarithmic ramp.** Linear in amplitude is what `room_volume`'s ramp and
  `crates/schedule`'s are, its slope bound is one division, and its ends are exact.
- **A clip gain of 1 with only the clamp to hold the limit.** A clamp that does real work is a
  hard clip, which is audible; the gain bound makes the clamp a rounding guard.
- **Fading the clip in with the duck ramp.** See decision 5.

## ASSUMED values

- The duck ramp, 200 ms. Not measured and not cited: the fades read (0.5 s, and a 2 s example)
  are for a music that fades while the voice is already playing. Here the clip waits for the
  ramp, so the ramp is announcement latency, and 200 ms is chosen as short enough to wait for
  and long enough to be a fade (9600 frames at 48 kHz, a gain step of 0.00009 per frame).
- That -20 dB and 500 ms sound right in a room is unmeasured: they are cited starting points
  until a room is listened to.
- The ramp ceiling, 3 840 000 frames (10 s at 384 kHz), and the depth floor, -50 dB (the end
  of the ESPHome range).
