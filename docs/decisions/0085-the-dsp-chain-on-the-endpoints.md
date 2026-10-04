# 0085: both endpoints run the DSP chain on every frame they write once a `sound` or a two-way configures it, with the room gain inside the chain, its 2 ms look-ahead added to the device delay, and the output map resolved after it

- Status: accepted (goal 12, 2026-10-01)
- Decided by: the goal (section 16: bass management for bonded sets and the two-way crossover on
  the endpoints; done-when line C, "Bass management and the two-way crossover run end to end on
  fakes"); the coordinator's design envelope for goal 12 (track `chorus-g12/endpoint-dsp`); K69
  and ADR 0063 (the subwoofer's local level and phase knobs); K81 and I10 (no DSP boost lifts a
  room above its limit); the placements below where the envelope left them open
- Implemented in: `firmware/include/chorus/endpoint_dsp.h`, `firmware/src/endpoint_dsp.c` (host
  build `LIB_SRC`, ESP-IDF component sources, `firmware/endpoint-units.conf`),
  `firmware/src/playout.c` and `playout.h`, `firmware/src/session.c`,
  `firmware/src/endpoint_config.c` and `firmware/config/endpoint.conf` (the `two_way*` keys),
  `firmware/main/app_main.c`; `crates/client-linux/src/dsp.rs` (`DspSink`, `TwoWayOutputs`),
  `crates/client-linux/src/control.rs` and `zone.rs` (the gain handed to the chain),
  `crates/client-linux/src/config.rs` (`--two-way`), `crates/client-linux/src/main.rs`; held by
  `firmware/tests/test_endpoint_dsp.c` (`make -C firmware endpoint-dsp`), the end-to-end runs
  `firmware/tests/dsp-session.sh` with `firmware/tests/main_dsp_session.c`
  (`make -C firmware dsp-session`, inside `check`) and
  `crates/server/tests/dsp_end_to_end.rs`, and the client's unit tests
- What the chain does: `docs/dsp.md` (its new section "On the endpoints"); the library: ADR 0082;
  the wire `sound`: ADR 0081

## Context

ADR 0082 gave chorus one chain in two languages; ADR 0081 put the room's sound on the audio wire,
to every endpoint of a room with its own bonded role. Nothing played either yet. Each endpoint
kind already had one place where it changes what a sample is: the C endpoint scales by the
room's volume in its playout writer (`chorus_volume_apply`, ADR 0074), the Linux client scales by
the zone and room gain just before its sink and then remaps at the sink's edge (ADR 0068). The
chain had to go in at that place on both, and three things about it needed deciding: what
happens to the gain the endpoints already apply, how the chain's fixed 2 ms look-ahead is kept
from moving the audio off the sync target, and where an output map sits relative to a chain whose
outputs change with the endpoint's role.

## What was read

All read 2026-10-01: `BRIEF.md` section 3.1; the design envelope; ADRs 0058, 0063, 0068, 0074,
0081, 0082; `docs/dsp.md`, `docs/protocol.md` ("0x39 sound", "The channel map");
`crates/dsp/src/{chain,settings,crossover,biquad}.rs`, `firmware/include/chorus/dsp.h`;
`firmware/src/{playout,session,volume,endpoint_config}.c`, `firmware/main/{app_main,esp_playout}.c`,
`firmware/tests/{main_session.c,session-outage.sh}`; `crates/client-linux/src/{run,sink,zone,
control,outmap,config,main,session}.rs`; `crates/server/tests/{common/mod.rs,sound_on_the_wire.rs,
fifo_source.rs,v2_end_to_end.rs}`. The LR4 properties the tests assert (-6.02 dB at the crossover,
flat sum): D. Bohn, RaneNote 160, <https://www.ranecommercial.com/legacy/note160.html>. No GPL
source was opened.

## Decision

1. **When the chain is in the path.** Once the endpoint knows its stream (rate and channel map)
   and has received a `sound` or has a two-way configured. The server sends `sound` in its
   greeting before any audio (ADR 0081), so in practice the chain runs from a stream's first
   frame. Before that both endpoints are byte for byte what they were (no latency added, the
   integer gain as before), which keeps every existing test and fixture as it was and keeps an
   endpoint of a server that never sends `sound` unchanged. A new stream builds a new chain; each
   `sound` is `set_sound`; a setting the chain refuses keeps what was in force and is counted
   (`dsp_refusals`, `refusals=`). On the C endpoint a chain that cannot be built for a stream
   stays out of the path and the endpoint plays with the integer room gain.
2. **The gain goes into the chain.** The library's order puts the volume after the sound stages
   and before the limiter, whose ceiling is `min(1, limit)`. So while the chain is engaged the
   C writer stops scaling the frames and the Linux `ZoneWatch::apply` stops scaling the PCM;
   both still advance the room's ramp by exactly the frames written, by the same arithmetic, and
   the chain is run over the block at that gain, in sub-blocks of 32 frames (ASSUMED) each at its
   first frame's gain, so a ramp is a staircase of 0.67 ms steps at 48 kHz. The limit handed to
   the chain is the least of the room's limit and the endpoint's own ceiling (`max_volume`,
   `--max-volume`). Silence the writer produces (an insertion, the hold before acquisition, an
   underrun) goes through the chain too, so its 2 ms tail plays out in time instead of leaking
   into the next audio.
3. **The look-ahead is device delay.** Every frame inside the chain is as far from the DAC as a
   frame inside the device's ring, so the C endpoint adds `chorus_dsp_chain_latency_frames` to
   the device delay its loop forms (and to `chorus_playout_fifo`), and shifts the GPIO marker's
   frame by the same count; the Linux `DspSink::delay_frames` is the device's plus the chain's.
   The sync loops then write each frame 96 frames (at 48 kHz) earlier and it is heard at the
   sync target. One accounting on both endpoints. A `sound` never changes the latency (ADR 0082),
   so only engagement does, once.
4. **Outputs.** One chain output (a bonded main, the subwoofer) is played on every device channel
   when nothing names otherwise: both I2S slots on the C endpoint, every channel of a Linux device
   without an output map (ASSUMED: a bonded member's amplifier channels all carry its role). A
   two-way's woofer and tweeter go to the outputs its local configuration names, the rest
   silence. The C endpoint advertises at most two channels, so its subwoofer's feed is the low
   branch of FL + FR; the LFE channel's +10 dB path is exercised by the library's fixtures and by
   a Linux subwoofer on a multichannel stream.
5. **Linux: the chain at the sink's edge, the map after it.** `DspSink` wraps the device (in
   place of `MappedSink`, whose `Remapper` it reuses) and takes stream frames. The chain needs the
   whole stream (a subwoofer sums the mains), so it cannot run after a map has picked a position;
   the map is instead resolved against the chain's OUTPUT positions: the stream's own when the
   endpoint is in no set, `[role]` when it plays one role, re-resolved when a `sound` moves the
   role. So `--output-channels 1 --output 0=LFE` takes a subwoofer's feed from a stereo stream that
   has no LFE channel at all, and a member with `--output 0=FL` plays its high-passed FL. A
   two-way names device outputs itself (`--two-way crossover-hz=<Hz>,woofer=<o>,tweeter=<o>`; the
   device is opened with enough channels to reach both) and is refused beside an output map: a
   driver is not a stream position. Before the chain engages `DspSink` is exactly `MappedSink`,
   or the device itself.
6. **C: the chain in the playout writer.** `chorus_endpoint_dsp_t` (a pure unit, no heap) holds
   the chain, the two-way configuration, the last wire `sound`, the knobs and the stream's
   layout; the playout path owns a pointer to it and every setter runs under the playout lock
   (`chorus_playout_set_dsp`, `_set_stream_layout`, `_set_sound`, `_set_sub_knobs`). The session
   hands it each `stream_format`'s channel map and each `sound`. Samples are the ring's I2S slots
   as left-justified 32-bit values, to `f32` by `2^-31` exactly and back with saturation, so a flat
   engaged chain at unity is the input, delayed. On the board the 72680-byte chain does not fit
   the static DRAM region (the link overflowed it by 38256 bytes), so `app_main` takes it from the
   internal heap once at boot, as `esp_playout.c` takes the jitter buffer; if that fails the
   endpoint logs it and plays without the chain.
7. **The two-way is endpoint configuration.** C: `two_way = off`, `two_way_crossover_hz = 2000`,
   `two_way_woofer_slot = 0`, `two_way_tweeter_slot = 1` in `firmware/config/endpoint.conf`
   (ASSUMED example until goals 24-25 design the drivers; refused by name outside 20 Hz..0.45 x the
   I2S rate, or slots that are not 0 and 1, one each; a board profile cannot set them). The file
   was at 16288 bytes of its readers' 16384-byte buffers, so every reader was raised to 32768
   bytes first (`endpoint_config.c`'s `base[]`, `app_main.c`'s `text[]`, `test_link.c`'s
   `base[]`); it is now 16725 bytes. Linux: `--two-way`, each key defaulting to the same example.
8. **The subwoofer's knobs combine with the catalog.** The knob's level (a cut, -12.0..0 dB) adds
   to the catalog's `sub_level_cdb`, held to the wire's -12..+6 dB, so a knob never lifts the sub
   above what the catalog allows; a phase knob at 90 degrees or more inverts the polarity the
   catalog sets (an exclusive or). The 0..180 degree knob quantised to a polarity is ASSUMED until
   a variable-phase all-pass is designed. The knobs have no ADC binding yet (ADR 0063), so they sit
   at 0 dB and 0 degrees; `chorus_playout_set_sub_knobs` is the seam.

## Line C's evidence

Both endpoint kinds against the real `chorus-server` with a control plane, a known multi-tone
(30, 80, 120, 300, 1000, 2000 and 6000 Hz, 0.1 each, L = R) on the FIFO source path, a bonded 2.1
set (FL, FR, LFE, wired) and a two-way endpoint, every captured frame graded by tone level:

- the mains are the LR4 high branch and the sub the LR4 low branch of FL + FR (+6.02 dB for L = R)
  at 80 Hz, -6.02 dB (mains) at the crossover; `bass_management` 120 Hz is followed;
- the two-way's woofer and tweeter are the LR4 split at 2 kHz, -6.02 dB each there, and sum flat
  (0.00 dB at every tone);
- bass and treble move each tone by the shelves' design; on Linux night and speech change the
  output exactly as the library's own chain does on the same signal;
- under a lowered room limit with +10 dB boosts and the sub at +6 dB, no captured sample exceeds
  the limit, where the same chain at full scale would (the sub peaks at the limit exactly).

`cargo test -p chorus-server --test dsp_end_to_end` (the Linux client, about 12 s) and
`make -C firmware dsp-session` (the C endpoint: the real session, playout path and chain in
`chorus-endpoint-dsp-session` processes whose writer threads stand in for the I2S DMA and
capture every block; 98 checks, about 24 s). Simulation, not timing evidence: levels are graded,
never instants. The C run measures each tone as the median of five 0.1 s windows, because its
sync loop inserts or drops a frame now and then and a single long window straddling one smears
6 kHz by up to 1.6 dB. From 1 kHz up it is the median of twenty-five Hann-weighted 20 ms
pieces instead (2026-10-04): on CI's loaded runner the servo slipped a frame in most of the
five windows, and their median read FL 0.55 dB low at 6 kHz.

## Not chosen

- **The gain applied before the chain, the chain at unity.** It would keep the existing integer
  gain code untouched, but put the night compressor and the limiter after the volume: night mode
  would compress less the quieter the room plays, the opposite of what it is for, and the two
  endpoint kinds would differ from the library's documented order.
- **The chain only while a setting is not flat.** It would save the 2 ms on a flat room, but a
  bass change would then step every endpoint's timing by 2 ms; the latency is fixed from
  engagement on instead.
- **The chain after the output map on Linux.** A map picks positions; a subwoofer needs the sum
  of the mains, and a role that changes with a `bond` would need a different map.
- **The chain object static on the board.** It does not fit the static DRAM region beside the
  rest; one allocation at boot is the jitter buffer's existing pattern.
- **Two-way beside an output map on Linux.** A driver is not a stream position; combining them is
  a configuration with two answers to "what plays on output 0".

## ASSUMED values (not measured)

The 32-frame gain block; one chain output on every device channel without a map; the phase knob's
90-degree polarity threshold and the knob's additive level; the two-way example (2000 Hz, woofer
output or slot 0, tweeter 1) on both endpoints; the 32768-byte configuration buffers (sized with
room, not measured).

## Follow-ups

- A variable-phase all-pass for the subwoofer's phase knob, and the knobs' ADC binding (ADR 0063).
- Driver trims, delays and polarity for the two-way (goals 24-25 design the drivers).
- The resync mute zeroes the chain's input, so up to 2 ms of the chain's tail can play into a
  mute; whether that is audible is a bench question.
- The chain's cost on the ESP32-S3 at 48 kHz stereo is unmeasured: a `docs/measurements/` report
  from the bench before any claim about headroom.
