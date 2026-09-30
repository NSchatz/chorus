# 0068: chorus-client drives N device channels through an output map at the sink's edge, and multichannel on the Linux tier is a Pi 5 parallel-lane DAC HAT

- Status: accepted (goal 10, 2026-09-30)
- Decided by: the goal (brief section 14 item 1; K96, K74, K72, P2 Option A); the board itself is
  the owner's input (P4 deferred), so the hardware example is ASSUMED
- Implemented in: `crates/client-linux/src/outmap.rs` (the map, `MappedSink`), additive flags in
  `crates/client-linux/src/config.rs`, the device open in `crates/client-linux/src/main.rs`,
  `max_channels` in `crates/client-linux/src/session.rs`; held by
  `crates/client-linux/tests/multichannel.rs` and
  `crates/server/tests/v2_end_to_end.rs::a_5_1_stream_from_the_real_server_plays_through_an_output_map_onto_8_channels`;
  the hardware design is `docs/hardware/linux-multichannel.md`

## Context

K96 makes Linux endpoints a product tier where they fit: the 2U rack amp (K74), several zones and
a sub or line out on one box, and the theater hub (K72), stereo LPCM now (P2 Option A) and 5.1
later. `chorus-client` opened its ALSA device with the stream's channel count and played the
stream's channels in the stream's order. Protocol v2's `stream_format` already names every
channel's position (`docs/protocol.md`, "The channel map") and says every edge that meets a
transport remaps once, where it meets it; the client did not. And no Raspberry Pi supports I2S
MCLK or TDM (brief section 2), so the hardware half had to say how a Linux box gets more than two
channels.

## What was read

All read 2026-09-30: brief section 14 and K72, K74, K96; `docs/protocol.md` ("The stream format
and the channel map"); `crates/client-linux/src/{sink,run,zone,config,session,main,lib}.rs`,
`crates/client-linux/tests/{zone_apply.rs,common/mod.rs}`, `crates/alsa/src/lib.rs` (the open
signature), `crates/protocol/src/v2/{catalog,negotiate}.rs`, `crates/server/src/session.rs`
(`channel_map`), `crates/server/tests/v2_end_to_end.rs`, `audio-path.conf`. The web sources are
the hardware design's (`docs/hardware/linux-multichannel.md`, section Sources): the Raspberry Pi
I2S white paper RP-009699-WP, the RP1 Peripherals datasheet, HiFiBerry's DAC8x, Studio DAC8x and
ADC8x pages, the alsa-lib PCM plugin and PCM API documentation pages, the kernel's ALSA
configuration and channel-mapping documentation pages, the USB-IF Audio Devices Rev. 2.0 page,
ESI's GIGAPORT eX product and knowledge-base pages, the RK3588 datasheet and the i.MX 8M Plus fact
sheet. No GPL or LGPL source (no kernel source or device-tree source, no alsa-lib source).

## Decision

1. **Where.** The remap is a sink wrapper, `MappedSink`, around the device the client opens. The
   playout loop hands it stream frames after the zone gain; it hands the device the same number
   of device frames. Rate, the reported delay, underruns, drains and the played-out count are the
   device's, passed through. So the buffer, the sync loop, the corrector and every existing test
   are untouched, and a map is invisible upstream. `run.rs` is not changed at all.
2. **What a channel plays.** One stream position; an equal-weight downmix of two or more
   positions (each `1/n`, so correlated full-scale inputs stay within full scale); or silence.
   Outputs not listed are silence. A map whose every output is silence is refused.
3. **Exactness.** One position at 0 dB is a byte copy: a map that only reorders is bit-exact in
   `pcm_s16le`, `pcm_s24le` and `pcm_f32le` (tested with extremes, a NaN payload, a negative zero
   and a subnormal). Anything else is computed in `f64`, rounded to the nearest sample for the
   integer formats and **saturated**, never wrapped; a float output is clamped to [-1.0, 1.0].
   Saturated samples are counted and printed at the session's end
   (`output-map clipped_samples=N`).
4. **Gain bounds: -60 dB to +6 dB, refused outside** (`MIN_GAIN_DB`, `MAX_GAIN_DB`; ASSUMED,
   chosen). +6 dB undoes a two-position downmix's 6.02 dB and no more: more is a volume stage,
   and volume belongs to the zone (limited on the server, brief section 4.8, I10) and the
   amplifier, not to a static trim. Whatever the boost, an output never exceeds digital full
   scale. Below -60 dB an output is meant to be off, and `silence` says so.
5. **Delay: whole frames, 0 to 50 ms, refused above** (`MAX_DELAY_US`; ASSUMED, chosen). Given in
   microseconds and rounded to the nearest frame at the stream's rate (the frames applied are on
   the `output-map` line). 50 ms is 17 m of path at 343 m/s, more than any in-room distance or sub
   trim, and under the client's default minimum buffer bound (60 ms). **A channel with a delay of k
   frames plays exactly k frames later than the sync target**, on purpose; every other channel
   plays at the target. Its first k frames are silence and its last k frames are not played when
   the session ends (writing them would write frames the stream never had).
6. **Nothing missing is guessed.** A position the stream lacks is silence on that output and an
   `output-map-missing` line names the output, the position and the stream's map; a downmix keeps
   its configured weight. Stream positions no output reads are an `output-map-unused` line. The
   one fallback is a `MONO` stream feeding outputs that read `FL`, `FR` or `FC`, reported on an
   `output-map-fallback` line.
7. **Advertised channels.** With a map, `capabilities.max_channels` is the number of distinct
   positions it reads (a stream with more would carry channels this endpoint discards); without
   one it stays 8.
8. **Configuration.** `--output-channels N` (1 to 8, the device's count, which ALSA is opened
   with) and repeated `--output <index>=<source>[,gain-db=<dB>][,delay-us=<us>]`, sources as
   protocol names (`FL`, `FL+FR`, `silence`, case-insensitive). Command-line flags rather than a
   map file: a map is at most eight short lines, the endpoint's unit file already carries its
   command line (the package track), and a flag is refused at start with the rest of the
   configuration (exit 2). Every refusal names its fix. No map: today's behaviour, byte for byte.
9. **Hardware: a Raspberry Pi 5 with a parallel-lane 8-channel DAC HAT (the HiFiBerry DAC8x
   class) for the rack amp and the theater hub; fallback, a class-compliant USB Audio Class 2
   interface.** One card is one ALSA device on one clock (RP1's audio PLL). The rack amp runs one
   `chorus-client` per zone, each on its own channel subset of the card through ALSA's `dshare`;
   the theater hub runs one client with one map over the card. Reasons, examples and sources are
   in `docs/hardware/linux-multichannel.md`. The board is ASSUMED until the owner answers P4.

## Not chosen

- **Remapping in the playout loop, or before the buffer.** It would change the frame the corrector
  shapes and the bytes the zone gain scales, and put the channel count into the sync accounting.
  At the edge it is one wrapper and nothing else moves.
- **A per-output delay applied by writing fewer frames on other channels or by moving the sync
  target.** Channels of one device share one frame clock; the only way to delay one of them is a
  delay line, and the target stays the group's.
- **Unbounded gain, or a clamp instead of a refusal.** A configured value outside the bounds is a
  mistake to name, not to round.
- **Downmix as a plain sum.** Correlated full-scale channels would clip; the equal weight is the
  predictable level, and the +6 dB bound exists to undo it where wanted.
- **A map file.** Reasonable for more channels than the protocol carries; at eight it is one more
  file to deploy and validate.
- **One client per card for the rack amp.** Zones are served separate streams; one client would
  need one stream carrying every zone.
- **Hardware options B and C as the primary.** A UAC2 box adds a clock domain and a cable in the
  chassis; a TDM SBC (RK3588, i.MX 8M Plus) is a board design for more than eight channels, MCLK
  codecs or TDM amplifier ICs, none of which K72 or K74 needs today.

## Follow-ups

- Hardware: whether several `dshare` clients' reported delays track the card as a sole client's
  does is a measurement (goal 26's bench, `docs/measurements/`), LEAD until then.
- The rack's zone and channel count (P13, goal 26) and the theater hub's 5.1 layout (goal 13)
  replace the ASSUMED examples.
- A stream at another rate than the card's `dshare` rate is refused at open; whether the server
  serves each Linux zone at the card's rate or the client resamples is goal 11's or 26's call.
