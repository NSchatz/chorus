# 0082: the DSP library is one algorithm in two languages, crates/dsp and firmware/src/dsp.c, held to shared fixtures whose expected values come from cited worked examples, and the endpoint chain ends in a look-ahead limiter at the room's limit

- Status: accepted (goal 12, 2026-10-01)
- Decided by: the goal (section 16 items 1 and 2: "biquads (RBJ), LR4 crossovers, delay, a
  look-ahead limiter, loudness compensation, a night-mode compressor, speech enhancement, in a new
  `crates/dsp` and a C mirror, float32, with `fixtures/dsp` shared between them"; "every filter
  and curve checked against published worked examples (cited)"); the coordinator's design
  envelope for goal 12 (track `chorus-g12/dsp-core`, blocks 1-10); K30, K81 and I10 (no DSP boost
  lifts a room above its limit); the semantics below where the envelope left them open
- Implemented in: `crates/dsp` (`chorus-dsp`: `biquad.rs`, `crossover.rs`, `delay.rs`,
  `limiter.rs`, `compressor.rs`, `loudness.rs`, `speech.rs`, `settings.rs`, `chain.rs`,
  `fixture.rs`, `examples/chain_samples.rs`); `firmware/include/chorus/dsp.h`,
  `firmware/src/dsp.c` (in the host build's `LIB_SRC` and the ESP-IDF component's sources);
  `fixtures/dsp/` and `tools/dsp-fixtures/generate.py`; held by
  `crates/dsp/tests/shared_fixtures.rs`, the crate's unit tests and `firmware/tests/test_dsp.c`
  (`make -C firmware dsp`, inside `check`); the units listed in `audio-path.conf` and
  `firmware/endpoint-units.conf`
- What each block does, the chain order, every citation and every ASSUMED default: `docs/dsp.md`

## Context

Goal 12 brings per-room sound (tone, loudness, night, speech), bass management for bonded sets,
the two-way crossover, room correction and the visualizer. The endpoints run the per-endpoint
chain (the C firmware and the Linux client), the server runs the analyses. A filter that
differs between the C endpoint and the Linux endpoint is a room whose speakers do not match, so
the chain has to be one algorithm in two languages, the way the protocol and the sync core are
(BRIEF section 7, conventions rule 9). And the goal asks for more than agreement between the two:
every filter and curve checked against a published worked example, so both cannot be wrong the
same way.

## What was read

All read 2026-10-01: the W3C "Audio EQ Cookbook" note (https://www.w3.org/TR/audio-eq-cookbook/);
ITU-R BS.1770-5 Annex 1, Tables 1 and 2
(https://www.itu.int/dms_pubrec/itu-r/rec/bs/R-REC-BS.1770-5-202311-I!!PDF-E.pdf); pyloudnorm's
`meter.py`, `iirfilter.py` and `LICENSE` (MIT, https://github.com/csteinmetz1/pyloudnorm), read for
its K-weighting parameters, nothing copied; RaneNote 160
(https://www.ranecommercial.com/legacy/note160.html) and Linkwitz Lab's "Active Filters"
(https://www.linkwitzlab.com/filters.htm); J. O. Smith III, "Delay Lines"
(https://ccrma.stanford.edu/~jos/pasp/Delay_Lines.html); Steinberg's "Brickwall Limiter" page;
MathWorks' `compressor` reference
(https://www.mathworks.com/help/audio/ref/compressor-system-object.html), for Giannoulis,
Massberg and Reiss 2012's static curve (the paper's PDF host returned an empty page) and the
10-90 % time constants; the ISO 226:2003 table and formula as reproduced at
https://www.dsprelated.com/showcode/174.php (the standard is sold); ATSC A/52:2018 section 3
(https://www.atsc.org/wp-content/uploads/2021/04/A52-2018.pdf); Wikipedia "Bass management";
SVS's crossover page (the THX 80 Hz); J. T. Geiger et al., "Dialogue enhancement of stereo
sound", EUSIPCO 2015; `docs/research/research-dsp-phase-b.md` (the goal's research, committed
here); `research-theater.md` section 5; and in this repository `docs/protocol.md` (the channel
map, the rate and channel bounds), `firmware/src/volume.c` and `firmware/tests/harness.h`,
`fixture_text.h` (idiom), `crates/schedule` (crate shape), `docs/conventions.md`. No GPL source
was opened; no DSP library's source was read.

## Decision

### One algorithm, two languages, one rounding

The crate has no dependency, no clock and no I/O (`#![forbid(unsafe_code)]`). The C unit has no
ESP-IDF header and no heap. Samples are `f32`, designs `f64` rounded to `f32` once (BRIEF section
5.6). Both run the same operations in the same order: the biquad is Transposed Direct Form II
written identically, every C float operation carries the `f` suffix so nothing is promoted to
double, and `-ffp-contract=off -fno-fast-math` keeps a multiply-add two roundings as Rust's is.
On the host the two chains agree to 1.5e-8 on every golden sample.

### The fixtures say where each number comes from

`fixtures/dsp/` has 39 files of eleven kinds, read by both sides; a kind either reader does not
know fails. Every block has at least one fixture whose expected values are printed in, or a
property stated by, a cited source: the ITU K-weighting coefficients reproduced by the high
shelf and high-pass designs at pyloudnorm's parameters (within 1.1e-4 and 2.9e-5; and the
designed stage 1 within 0.0022 dB of the printed filter's response); the cookbook's magnitudes at
f0, DC and Nyquist for all eight designs; LR4's -6.02 dB, in-phase branches and flat sum (and
the running split's 0.5 amplitude at the crossover); `y(n) = x(n - M)`; the limiter's "never
exceeds"; the Giannoulis curve and the 10-90 % times; the ISO 226 table rows and formula. Where
a value has to be computed from a source (a curve at an input, a contour level, a designed
response), `tools/dsp-fixtures/generate.py` computes it in Python from the source's formula,
independently of both implementations. The chain fixtures add analytic checks (a flat chain is
its input times the gain, bit for bit; a centred tone through speech enhancement, a 40 Hz tone
through a main and a subwoofer, a 2 kHz tone through the two-way split and their flat sum, all
at the amplitudes the cited responses give) and, where the chain is too involved for a closed
form, golden samples from the Rust chain, which say so: they hold C to Rust, they are not a
worked example.

### The chain

Order: room EQ, tone, loudness, speech, night, bass management (by role), the two-way split,
per-output delay, the room gain, the look-ahead limiter at `min(1, limit gain)`. The limiter is
the guarantee for K81 and I10: whatever the settings boost, no output sample exceeds the room's
limit. Its look-ahead is the chain's latency whatever the settings (2 ms), so a setting never
moves the audio in time and a flat chain is still its input delayed and scaled, bit for bit.
Gains change without resetting filter state; a layout change (role, subwoofer, crossover,
delays) resets the output side; a refused setting changes nothing.

### Fixed maximums

8 channels, 8 outputs, 4800 delay frames per output (50 ms at 96 kHz), 9600 for all of a
chain's outputs together, a 768-frame look-ahead (2 ms at 384 kHz). The Rust chain refuses what
the C chain cannot hold, so the two refuse the same configurations. A `chorus_dsp_chain_t` is
72680 bytes, one static object on the endpoint.

### Refinements of the envelope (owned by this track)

- The limiter's ramp: a linear-in-time approach to every pending frame's required gain plus a
  final clamp, and the release kept as the distance below unity with a snap to exactly 1.0
  (without it, `1 - (1 - g) a` stalls one ulp below 1 in `f32` and a chain that once limited is
  never bit-exact again; a unit test found it).
- The two-way drivers' delays add to the endpoint's per-output delays.
- A main role the stream lacks plays silence, except a mono stream, which plays on every main.
- Speech's gain is +4 dB (Geiger et al.'s 3.8 dB centre gain), per the goal's research; the
  envelope's placeholder was open.
- `SoundSettings::default()` is flat (loudness off); the catalog's own defaults (loudness on)
  are the catalog's to send.
- The room-EQ bounds are exported by `chorus-dsp` (`settings::ROOM_EQ_*`) because
  `SoundSettings::validate` needs them; `crates/control`'s `ROOM_EQ_*` are the same numbers.

### Every ASSUMED default

The table in `docs/dsp.md` "Defaults and ASSUMED values": the tone shelves (100 Hz, 8 kHz, Q
1/sqrt 2), the corner limit (0.45 x the rate), loudness (80 phon reference, 50 Hz and 10 kHz
evaluation, 100 Hz and 8 kHz shelves, +12 and +6 dB caps, 0.5 dB steps), speech's centre and Q
(2 kHz, 0.667), the night curve (-24 dBFS, 3:1, 12 dB knee, +6 dB makeup) and times (10 ms,
500 ms), the limiter (2 ms look-ahead, 100 ms release, 1e-6 snap), the unbonded two-way input,
the two-way example (2 kHz) and the driver trim range (-24..0 dB).

## Not chosen

- **Golden outputs for every fixture.** A fixture whose expected values are one
  implementation's output proves only that the other agrees; the goal asks for cited worked
  examples, so goldens appear only where a closed form is impractical, and they say so.
- **Fixed-point or Q-format coefficients for the ESP32-S3.** The S3 has a single-precision FPU;
  `f32` keeps one algorithm for both endpoints. The cost (the LR4 low-pass at 80 Hz of 48 kHz is
  0.005 dB low at 40 Hz from coefficient rounding) is documented and allowed for.
- **A 12 dB/oct high-pass on mains (the THX scheme).** It relies on a main speaker's own
  2nd-order roll-off at the crossover, which a generic endpoint does not have; LR4 on both sides
  is the one choice whose electrical sum is provably flat.
- **The Dolby "Film Standard" curve for night mode now.** It is multi-segment around a dialogue
  reference PCM does not carry; the Giannoulis soft-knee curve is what the envelope names.
  Recorded as a follow-up.
- **Bypassing the limiter in a flat chain.** It would change the latency when a setting changes;
  the limiter at unity is already bit-exact.

## Follow-ups

- `chorus-g12/endpoint-dsp` wires the chain: on the C endpoint (a static `chorus_dsp_chain_t`,
  the playout path's float conversion around it, latency of `chorus_dsp_chain_latency_frames`)
  and in the Linux client; both kinds must add the same latency.
- The catalog track's `ROOM_EQ_*` and `chorus-dsp`'s are the same numbers; the integration may
  make one re-export the other.
- Night mode on the Dolby "Film Standard" curve (research section 4b), if listening says the
  single knee is not enough.
- The chain runs the per-channel stages on every stream channel; an endpoint that plays one
  channel of 5.1 could run them on that channel only (the night detector still needs all).
