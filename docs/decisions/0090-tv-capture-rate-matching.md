# 0090: a TV input on the Linux hub is rate matched onto the server timeline by a DLL on its period stamps and an adaptive-resampling ratio loop, and refused by name when it is not PCM, has no lock, or runs outside +/-1500 ppm

- Status: accepted (goal 13, 2026-10-01)
- Decided by: the goal 13 envelope, section 2 (Capture); research `capture.md` (goal 13) and its
  proposals 6.1 to 6.5; BRIEF.md section 3.1 (monotonic clocks in the audio path, measurement
  rule); the placements below where the envelope left them open
- Implemented in: `crates/client-linux/src/ratematch.rs` (the DLL, the ratio loop, the matcher),
  `crates/client-linux/src/tvcapture.rs` (the IEC 61937 scan, the lock and range refusals, the
  chunking), `crates/client-linux/src/source.rs` (`run_tv`, `is_tv`, `tv_signal`,
  `CaptureSource::read_within` and `channel_status`, the TV counters and status line),
  `crates/client-linux/src/main.rs` (the status line at a session's end),
  `crates/sync/src/latency_grow.rs` (`ChunkPlan::constant_rate`), `audio-path.conf`; held by
  `crates/client-linux/tests/tv_capture.rs` (on `tests/common/tv.rs`, the modelled TV),
  `crates/server/tests/tv_capture_rate_match.rs` (the real server binary), and the unit tests in
  `ratematch.rs`, `tvcapture.rs` and `latency_grow.rs`

## Context

A TV's optical or ARC output is clocked by the TV: the receiver on the hub recovers its clock
from the stream, so the hub captures at the TV's rate. IEC 60958-3 lets a consumer source be off
by +/-1000 ppm (Level II, section 7.2.1 of IS/IEC 60958-3:2003,
<https://law.resource.org/pub/in/bis/S04/is.iec.60958.3.2003.pdf>, read 2026-10-01). ADR 0066
sends a line-in upstream as captured, stamped at the capture instant; the server plays it at
its own nominal rate (ADR 0071's plan), so a TV 300 ppm fast moves the room's latency by 18 ms
a minute and a TV 300 ppm slow uses up the 30 ms start lead in about 100 s. The envelope makes
the hub's output the timeline's: exactly the nominal rate, stamped `t0 + n / rate`. A TV can
also send an encoded bitstream (IEC 61937) that must never play as PCM, and goes silent or
stalls when it is switched off.

## What was read

All read 2026-10-01, clean-room (BRIEF.md 3.1 rule 1, `docs/clean-room.md`): no ALSA, kernel
`sound/`, PulseAudio, PipeWire, JACK, zita-ajbridge or alsa_in source was opened. Papers: F.
Adriaensen, "Using a DLL to filter time" (LAC 2005,
<https://kokkinizita.linuxaudio.org/papers/usingdll.pdf>), and "Controlling adaptive
resampling" (LAC 2012, <https://kokkinizita.linuxaudio.org/papers/adapt-resamp.pdf>).
Standards: IEC 61937-1:2021 preview (iTeh, the IEC's sample: burst preamble Pa = 0xF872, Pb =
0x4E1F in subframes 1 and 2 of one frame, time slots 12 to 27), IS/IEC 60958-3:2003. Datasheets:
TI DIR9001 (SLES198A: +/-1500 ppm accept window, the ERROR and AUDIO pins, Table 12's unlocked
behaviour), Cirrus CS8416 (DS578F5 section 10.2: bit 1 is not set by every encoder). Through
research `capture.md`, which cites each with its section. Repository: ADRs 0066, 0071, the
envelope, `crates/sync/src/latency_grow.rs`, `crates/server/src/{linein,slots,conductor}.rs`.

## Decision

1. **Which inputs.** `optical` and `hdmi_arc` (`source::is_tv`) run `source::run_tv`; `line_in`
   keeps ADR 0066's loop unchanged (see "Not chosen" for why it is not rate matched here).
2. **Capture.** Periods of 240 frames (5 ms at 48 kHz, research 6.1) with an ALSA ring of 20 ms;
   each read waits at most 20 ms (`CaptureSource::read_within`, on `AlsaCapture` a 1 ms poll of
   `snd_pcm_avail` against an `Instant` deadline), so a stalled receiver never blocks the loop.
3. **The DLL** (Adriaensen 2005, section 3 and 4): one update per period with the instant the
   period's last frame was digitized, the read's return less the device's reported delay. B =
   1 Hz at 200 periods/s (`w = 0.0314, b = 0.0444, c = 0.000987`, research 6.2). The rate
   estimate is the loop's period state `e2`, not the paper's `(t1 - t0) / P`, which carries the
   proportional term and passes the wakeup jitter through (unit test: +/-100 us jitter gives the
   700 ppm source within 100 ppm). The DLL runs on the SERVER timeline (the hub's monotonic clock
   plus the published sync offset), so the output is nominal on the timeline the rooms play, not
   on the hub's crystal; an offset step larger than 10 ms is a relock.
4. **The ratio loop** (Adriaensen 2012, sections 3.2 to 3.4): error `e = x* - R` in source
   frames, `x*` the source position whose DLL-mapped capture instant equals the next output
   frame's stamp and `R` the resampler's fractional position (so the fractional delay is in the
   error, section 3.2); a second-order low-pass at 20x the bandwidth; the 2005 loop's
   coefficients, scaled by the frames per update (derivation in `ratematch.rs`); 0.5 Hz for the
   first 4 s, then 0.05 Hz; the ratio and the integrator clamped to +/-1500 ppm. The start is
   Adriaensen's one-off skip made exact: the resampler starts at the fractional source position
   captured 2 ms before the newest period, so the initial error is zero, and the integrator
   starts at the DLL's estimate after a 0.5 s warm-up.
5. **The resampler**: `chorus_sync`'s Catmull-Rom `CubicResampler`, through a new public
   `ChunkPlan::constant_rate(source_start, step, frames)` (a private `Positions` variant beside
   the latency plan's segment; the existing plan, its tests and its numbers are unchanged).
6. **Stamps**: output frame `n` of a lock is stamped `S0 + n * 1e9 / rate` on the server
   timeline; chunks of the input's chunk size (960 frames, 20 ms, as today) take the stamp of
   their first frame. A relock (overrun, ring over- or underflow, a timing jump) restarts the
   output at a fresh `S0`; a partial chunk is dropped and counted, never sent with a wrong stamp.
7. **Refusals** (`tvcapture::Refusal`), each logged `source-tv-refused reason=<name>` with the
   owner's advice, counted, and offered as `signal = false` (`source-offer ... signal=0
   reason=<name>`); nothing captured is forwarded while one stands:
   - `non-pcm`: a frame whose left and right top 16 bits are Pa and Pb, or the receiver's
     non-audio bit (`CaptureSource::channel_status`; ALSA exposes none today), mutes from the
     period it is in; it lifts after 250 ms with neither (research 6.5, ASSUMED). The level
     detector is not consulted for it.
   - `no-lock`: no frames for 100 ms, or a read error (a device that is gone still ends the role,
     as ADR 0066's does); lifted when the matcher runs again after its warm-up.
   - `rate-out-of-range`: the ratio loop at its clamp for 2 s; the matcher then measures afresh
     and the refusal lifts at the first warm-up whose estimate is inside the clamp.
8. **The offered signal** is `tv_signal(wanted, refusal)`: wanted is the level detector's verdict
   (ADR 0066), or on a hub with CEC what ADR 0087's `chorus_cec::TvSignal` makes of it and the
   TV's power (the TV on offers the input before any audio, a standby ends it at once); a
   refusal withholds it either way, since nothing would be forwarded to play.
9. **The seam for the low-latency path** (envelope direction 2, built by the integration track):
   `TvFrontEnd::set_chunk_frames` switches the chunk size from the next chunk (120 frames,
   2.5 ms, when `0x16` direction 2 is offered), and every chunk leaves `run_tv` through
   `Upstream::send` with its stamp, so a UDP sender is one more `Upstream`.

## Evidence

Simulation, not timing evidence (BRIEF.md 3.1 rule 3): the TV, its jitter (wakeups late by up to
100 us, uniform) and its drift are model inputs (`tests/common/tv.rs`).

- `tests/tv_capture.rs` (10 tests, the real source loop on 10 modelled minutes): +200, -200,
  +1400 and -1400 ppm sources drifting slowly (up to 6 ppm over the run) keep every chunk's
  stamp within one frame (20.8 us, `SETTLED_BOUND_FRAMES`) of the true capture instant of the
  frame it starts on, after 60 s: measured 10.7 to 11.0 us at worst, a mean of 10.0 to 10.2 us
  (half a frame: the model reports the device delay in whole frames, as ALSA does) and under
  1 us of variation; stamps exactly 20 ms apart, no relock, no ring over- or underflow. A 1 kHz
  tone comes out at -0.000017 ppm (+1400 ppm TV) and +0.000025 ppm (-200 ppm drifting) of
  nominal, fitted over 540 s. A +2000 ppm TV is refused `rate-out-of-range` at 2.5 s and taken
  back 0.47 s after it returns to +100 ppm; an IEC 61937 stream is refused `non-pcm` on the
  period of its first burst and nothing of it is forwarded; a set non-audio bit is refused with
  PCM samples; a stall is `no-lock` 100 ms after the last period and taken back 0.5 s after the
  clock returns.
- `crates/server/tests/tv_capture_rate_match.rs` (2 tests, 40 s of wall clock each, the real
  `chorus-server` playing the real client's source role from a +300 ppm modelled TV paced in real
  time): declared `optical`, the room's latency moved +0.005 ms over 30 s (+0.2 ppm), the server
  reported `underruns=0 chunks_dropped=0`; the negative control, the same TV declared `line_in`
  (today's path), moved +8.997 ms (+299.9 ppm): at that slope lip sync leaves +/-40 ms in about
  133 s and the 1000 ms port ring overflows in under an hour. The latency is graded net of whole
  chunks of late delivery, which a loaded host can cause and rate matching cannot (the server
  and the hub run without real-time priority in the gate).

## Not chosen

- **Rate matching `line_in` too.** Not clearly right yet: an analogue input is digitized by the
  hub's own converter, whose crystal error against the server timeline is unmeasured, it has no
  IEC 61937 or lock question, and its tests hold the capture-instant stamps exactly. The same matcher would serve
  it unchanged (`is_tv` is the one switch); a follow-up for a bench measurement.
- **Feed-forward of the DLL's rate into the ratio.** The DLL at 1 Hz passes the wakeup jitter
  into its estimate; driving the ratio with it modulates the output's frequency at 1 Hz, which
  is what Adriaensen 2012 section 2 warns against. The estimate seeds the integrator once.
- **Sending silence while refused.** The envelope says the hub forwards nothing; the stream's
  stamps then carry the gap, and the receiving end's gap handling (the relay's crossfade) is the
  integration track's.
- **The 96-bit sync code** (four zero words, Pa, Pb; WM8804 and CS8416). Pa/Pb in one frame is the
  envelope's rule; requiring the zero stuffing too would lower false positives on PCM and miss a
  burst with no stuffing before it. A follow-up if a bench shows false mutes.

## Deviations from the envelope

- The envelope's tests name +/-900 ppm; the track's brief asked +/-1400 ppm, which is tested (with
  +/-200 ppm), inside the +/-1500 ppm clamp.
- The DLL runs on the server timeline (hub clock plus the published offset), not the hub's alone:
  "exactly the nominal rate on the timeline" needs the server's.
- The rate estimate is the DLL's `e2` (decision 3).
- The non-PCM mute is not ramped: nothing is sent while muted, so there is nothing to ramp on the
  hub; the room's side of the gap is the integration track's.

## ASSUMED values (not measured)

`PERIOD_FRAMES` 240; `DLL_BANDWIDTH_HZ` 1.0; `STARTUP_BANDWIDTH_HZ` 0.5; `SEND_DELAY_NS` 2 ms;
`WARMUP_PERIODS` 100 (0.5 s); `RELOCK_ERROR_NS` 10 ms; `RING_FRAMES` 2400 (50 ms);
`NON_PCM_CLEAN_HOLD_MS` 250; `NO_FRAMES_MS` 100; `CLAMP_HOLD_MS` 2000; `TV_CAPTURE_BUFFER_US`
20000; `TV_READ_WAIT_MS` 20; the TV's nominal rate (48 kHz, the configured `--line-in-rate-hz`;
the owner's TVs are unknown: Needs item "The three TVs: model, eARC port, optical out and audio
menu"). Cited, not assumed: the 0.05 Hz loop, the 20x low-pass and the 4 s start-up (Adriaensen
2012), the loop coefficients (Adriaensen 2005), +/-1500 ppm (DIR9001), Pa/Pb (IEC 61937-1).

## Follow-ups

- Integration track: switch the chunk size to 120 frames and send over UDP when the server offers
  direction 2 (`TvFrontEnd::set_chunk_frames`, an `Upstream` for the datagrams); the relay plays
  the stamps, so the gap a refusal leaves is crossfaded there.
- Bench (S8): the receiver's behaviour with the TV off (stall or free-running zeros), the ALSA
  controls a Digi+ I/O or DIR9001 board exposes (`amixer -c <card> contents`) so
  `channel_status` can read the non-audio bit, the hub's real wakeup jitter at 240-frame periods,
  and the TVs' measured rate error (`source-tv ppm=`).
- A capture format of S32_LE (research 6.1) beside S16 and S24, and reopening at 44.1 or 32 kHz
  when a TV changes rate class.
- Rate matching `line_in` once its converter's drift is measured (Not chosen).
