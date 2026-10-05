# An announcement's duck and restore, in frames of the stream a room receives

Date: 2026-10-05
Source: host
Build measured: `0b32c33fa3a942705b49777b2f5fae90ce7d0feb`
Build note: the commit above is the commit of `main` this report's pull request is made on. What
ran is that commit with the pull request's change to `crates/server` and `crates/control` on
top (the announcement mixes, ADR 0174); nothing else differs from it.
Timing evidence: none about speakers. This is a software measurement on the development host:
what a real protocol v2 player session received from the real `chorus-server` binary, counted in
frames of the stream. No wall clock is read to make any figure here, and nothing says when a
room hears a frame (that is the stream's playout latency, which this change does not touch).

What is measured: the two bounds of ADR 0174 decision 11, for an announcement mixed over a room
that plays the configured stream.

1. From the clip's first frame to the full duck: zero frames (the music is fully ducked no later
   than the frame before the clip's first).
2. From the clip's last frame to the full restore: at most the restore ramp and one chunk,
   24 000 + 960 frames at 48 kHz.

## Command

```
cargo test -p chorus-server --test announce the_duck_is_full -- --nocapture
```

run three times (rustc 1.98.1, the dev profile, x86_64 Linux), under the heavy locks. The test
file is the method: `crates/server/tests/announce.rs`
(`the_duck_is_full_before_the_clip_and_the_music_is_back_within_the_bound_in_frames`, and
`heard`, which reads an announcement out of a room's frames and fails on any frame that is not
what the envelope says).

## Method

The server runs with two stream slots, one player and one room, 48 kHz stereo `pcm_s16le` in
20 ms chunks (960 frames). The music is the configured stream, every sample 4660. Each clip is a
generated 48 kHz stereo WAV served on loopback: its left channel a ramp, its right channel
silence. So in what the room receives the RIGHT channel is the music alone (its gain, frame by
frame) and the LEFT channel minus the right is the clip (which clip frame is in which frame of
the stream).

A frame's place on the stream's timeline is its chunk's sequence times 960 plus its place in the
chunk. Five clips are announced one after another, of 4801, 12 345, 24 000, 30 007 and 48 959
frames, so that they end at different places in a chunk. For each, from the frames the room
received:

- `duck first`: the first frame whose sample is below 4660;
- `duck full`: the first frame whose sample is 466 (4660 at -20 dB);
- `clip first`, `clip last`: the first and last frames whose left channel is not their right;
- `restored`: the first frame after the clip whose sample is 4660 again.

Every frame between is checked: the music never rises on the way down and never falls on the way
back, it is 466 under every clip frame, every clip frame is there in order with nothing between
(left minus right is 0.9 of the clip's sample, give or take one step), and no clip frame is
heard during either ramp.

## Result

The same in all three runs, for every clip:

| clip, frames | duck first to duck full, frames | duck full to clip first, frames | clip last to restored, frames |
|---:|---:|---:|---:|
| 4 801 | 9 598 | 2 | 23 998 |
| 12 345 | 9 598 | 2 | 23 998 |
| 24 000 | 9 598 | 2 | 23 998 |
| 30 007 | 9 598 | 2 | 23 998 |
| 48 959 | 9 598 | 2 | 23 998 |

Against the bounds:

| bound (ADR 0174, decision 11) | bound, frames | measured, frames | at 48 kHz |
|---|---:|---:|---:|
| the clip's first frame to the full duck | 0 (never after) | -2 (the full duck is 2 frames BEFORE) | -0.04 ms |
| the clip's last frame to the full restore | 24 960 | 23 998 | 499.96 ms |

## Reading the figures

The figures are read off 16-bit samples, which reach a level a frame or two before the gain
does. The envelope (ADR 0173) is exact: the way down is 9600 frames and the way back 24 000.

- **9598, not 9600.** On the first frame of the ramp the gain is `1 - 0.9 / 9600` and 4660 times
  that rounds to 4660, so the first frame that READS lower is the second; and frame 9599 of the
  ramp already rounds to 466. 9599 - 2 + 1 = 9598.
- **2, not 1.** The clip's first frame is frame 9601 after the start, the frame after the gain
  reaches the duck gain on frame 9600 (ADR 0173 decision 5); the sample read 466 one frame
  before that.
- **23 998, not 24 000.** The restore begins on the frame after the clip's last, and 4660 times
  the gain rounds to 4660 from frame 23 998 of it on.

So the restore began on the frame after the clip's last in every one of the 15 announcements:
none used the chunk the bound allows for a producer that says it finished late.

## What this does not show

- Nothing about a speaker: no endpoint played these frames, and no clock was compared.
- How long after the `announce` command the duck begins (one conductor pass) or how long a clip
  takes to arrive (its fetch): neither is a frame count, and neither is bounded by ADR 0174. In
  these runs the clip was in its port before the duck was full, every time (a loopback fetch).
- A clip that arrives AFTER the duck is full: the music is then held down until it does. The
  unit test `a_held_port_and_a_late_clip_hold_the_duck_and_lose_no_frame` in
  `crates/server/src/mixer.rs` holds that; it is not measured here.
- Any rate but 48 kHz, any chunk but 20 ms, any format but 16-bit.
