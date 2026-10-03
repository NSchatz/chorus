# Gapless joins through the media player engine, on the host

Date: 2026-10-03
Source: host
Build measured: `b298b2dfe4e854a0c385ff165da7208bb212280a`
Timing evidence: none about speakers. This is a software measurement on the development host:
what a real v2 client session received from a server assembled in process, counted in frames.
The one figure that involves time, where `Boundary` was reported, is given in frames of the
port's own counter and says nothing about when a room hears the join.

What is measured: the media player engine (`crates/server/src/mediaplayer.rs`, ADR 0124) playing
two tracks back to back through one player port, heard by the Linux client's own session code on
a loopback socket. For each pair: the frames expected, the frames received, the largest absolute
sample difference within 2048 frames of the join, and how far past the join the port's
played-frames count stood when the `Boundary` report arrived.

## Command

```
cargo test -p chorus-server --test media_player -- --nocapture --test-threads 1 gapless a_stop_gets
```

run on the commit above, main after the engine's merge (rustc 1.98.1, the dev profile,
x86_64 Linux), under the heavy locks; the lines starting `gapless-join:` and `stall:` are
the figures below. The branch run before the merge gave the same table. The test file is the
method: `crates/server/tests/media_player.rs`.

## Method

The server is 16-bit stereo with 10 ms chunks, at 48 kHz or 44.1 kHz as the pair needs. Track A
is loaded and started, track B queued at once (`QueueNext`), and the session's chunks are
collected until silence follows. "Expected" is the server's own decode of the two files, one
after the other, rounded to 16 bits as the server rounds; for the lossless pairs that is the
uncut original signal. The received frames must contain the expected frames as ONE unbroken run
(no inserted silence, no dropped or repeated frame) with only silence around it, starting on a
chunk. The `Boundary` figure is `played_frames()` read when the report is received, minus the
index of track B's first frame.

- `wav-ramp48`: 120000 frames of a ramp (every frame different), cut at frame 70007, generated.
- `flac-gap44`: `fixtures/decode/flac-gap44-{a,b}.flac`, cut at 22050; started at frame 132 (a
  seek to 3 ms before the start) so the join falls inside a chunk and not on a chunk's edge.
- `mp3-gap44`, `vorbis-gap44`: `fixtures/decode`, one second cut in two; also compared with the
  ORIGINAL signal at the join, at the decode test's bound (0.12 of full scale within 2048 frames).
- `wav-44-then-48`: 30000 frames at 44.1 kHz then 24000 at 48 kHz on a 48 kHz server: the
  resampler is flushed at the join (ADR 0124); expected is track A resampled alone, then B.

## Result

| pair | server rate | frames expected | frames received | max abs difference near the join (16-bit steps) | `Boundary` reported, frames after the join | chunk (frames) |
|---|---|---|---|---|---|---|
| wav-ramp48 | 48000 | 120000 | 120000 | 0 | 73 | 480 |
| flac-gap44 | 44100 | 43968 | 43968 | 0 | 132 | 441 |
| mp3-gap44 | 44100 | 44100 | 44100 | 0 | 441 | 441 |
| vorbis-gap44 | 44100 | 44100 | 44100 | 0 | 441 | 441 |
| wav-44-then-48 | 48000 | 56654 | 56654 | 0 | 466 | 480 |

Against the original signal, within 2048 frames of the join: MP3 0.0883 of full scale, Vorbis
0.0298 (the encoders' own error at a cut; `crates/decode/tests/reference_decodes.rs` measures
the same decodes at 8.8e-2 and 3.0e-2).

In every pair `Boundary` arrived while the count stood inside the chunk that carried the join
(the figure is the distance from the join to the end of its chunk: the audio thread takes whole
chunks). The test asserts at most two chunks. The same figures came out of two runs.

Also printed by the same command, a stalled source (ADR 0124): a `Stop` sent while the player's
thread waited on a server that had stopped sending took effect in 50 ms, and a `Load` sent while
it waited on a server that never answered in 56 ms (45 ms and 54 ms in the branch run). The
fetcher's cancel slice is 100 ms; the test asserts 1 s.

## What this does not show

Nothing about speakers: when a room hears the join is the playout path's, unchanged by this
work. Nothing about a network slower than loopback: a next URI that opens more slowly than the
ring holds (1 s) makes the join late, with the dry ticks counted as underruns. No Opus pair is
played here (the decode tests hold `opus-gap48`).
