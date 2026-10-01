# The DSP library

chorus's sound processing (goal 12): `crates/dsp` (package `chorus-dsp`) and its C mirror
`firmware/include/chorus/dsp.h` + `firmware/src/dsp.c`. The same blocks, the same algorithms and
the same state-update order in both languages, held to one set of committed fixtures under
`fixtures/dsp/` (`crates/dsp/tests/shared_fixtures.rs`, `firmware/tests/test_dsp.c`;
`fixtures/README.md` "dsp/"). Pure libraries: no clock, no I/O, and on the C side no heap.

The decisions: `docs/decisions/` "the DSP library". The research behind the phase B values:
`docs/research/research-dsp-phase-b.md` and `.claude/goals/2026-09-chorus-research/research-theater.md`
section 5.

## Numbers

Samples are `f32`. Coefficients are designed in `f64` and rounded to `f32` once (BRIEF section
5.6). Rust never fuses `a * b + c`; the C unit is compiled with `-ffp-contract=off
-fno-fast-math` and writes every float operation with the `f` suffix, so one float operation is
one rounding in both languages. On the host the two chains agree to within 1.5e-8 on every
golden sample (the fixtures allow 1e-6).

A consequence of `f32` coefficients worth knowing: a section whose corner is far below the rate
has its poles close to `z = 1`, and rounding its coefficients moves its low-frequency gain
slightly. The LR4 low-pass at 80 Hz of 48 kHz is 0.005 dB low at 40 Hz (the 2.1 subwoofer
fixture says so and allows for it).

## The blocks

1. **Biquad** (`biquad`, `chorus_dsp_biquad_*`): the RBJ Audio EQ Cookbook's eight designs
   (lowpass, highpass, bandpass with constant 0 dB peak, notch, allpass, peaking, low shelf,
   high shelf), normalised by `a0`, run as Transposed Direct Form II
   (`y = b0 x + z1; z1 = b1 x - a1 y + z2; z2 = b2 x - a2 y`). `magnitude_db(f)` and the complex
   `response(f)` evaluate a design in `f64` (fixtures, room fitting). Changing coefficients keeps
   the state.
2. **LR4 crossover** (`crossover`, `chorus_dsp_lr4_*`): each branch two identical Butterworth
   sections (Q = 1/sqrt 2) at the crossover; the high branch is not inverted. `split` gives
   `(low, high)`; their sum is an all-pass.
3. **Delay** (`delay`, `chorus_dsp_delay_*`): whole frames, at most 4800 (50 ms at 96 kHz), refused
   above. Microseconds become frames by `(us * rate + 500000) / 1000000` in integers. The C line
   runs over caller storage.
4. **Look-ahead limiter** (`limiter`, `chorus_dsp_limiter_*`): channel-linked. The signal is
   delayed by the look-ahead `L`; each frame's required gain is `ceiling / peak` when its peak
   exceeds the ceiling, else 1; the gain is the least of a release candidate, the required gain of
   the frame leaving the delay, and for every later frame needing less than the current gain the
   linear ramp that reaches it as it plays; then the output is clamped to the ceiling. So no
   sample ever exceeds the ceiling, and input that never exceeds it comes out delayed, bit for
   bit. The release runs on the distance below unity as its own state and snaps to exactly 1.0
   below 1e-6, so a chain that has limited becomes bit-exact again.
5. **Night compressor** (`compressor`, `chorus_dsp_compressor_*`): feed-forward, the detector in the
   log domain after the gain computer (Giannoulis, Massberg and Reiss 2012's recommendation), the
   soft-knee static curve, attack and release smoothing of the computed gain with
   `a = exp(-ln 9 / (Fs T))` (T is the 10-90 % time), a makeup gain, linked over all channels.
6. **Loudness compensation** (`loudness`, `chorus_dsp_iso226_*`, `chorus_dsp_loudness_*`): ISO
   226:2003's table and formula give `Lp(f, phon)`. Playing `att` dB below the reference level,
   at `L = 80 - att` phon (held to 20..80), the boost a frequency needs to keep its balance with
   1 kHz is `(Lp(f, L) - Lp(1k, L)) - (Lp(f, 80) - Lp(1k, 80))`: taken at 50 Hz for a low shelf
   at 100 Hz (capped at +12 dB) and at 10 kHz for a high shelf at 8 kHz (capped at +6 dB), never
   negative. The attenuation comes from the room gain, `-20 log10(gain)`, in 0.5 dB steps, so a
   volume ramp redesigns the shelves at most once per step and never resets them. At unity both
   gains are exactly 0 and the stage is off.
7. **Speech enhancement** (`speech`, in the chain): a peaking boost at 2 kHz, Q 0.667, +4 dB on FC
   when the stream has one; with FL and FR, on the mid of a mid/side split
   (`m = (l + r)/2`, `s = (l - r)/2`, `l = m' + s`, `r = m' - s`); on the one channel of a mono
   stream.
8. **Bass management** (in the chain): by the endpoint's role in its bonded set. A main role, with
   a subwoofer in the set, plays its channel through the LR4 high branch at `crossover_hz`. The
   `LFE` role plays the LR4 low branch of the sum of the stream's main channels, plus the
   stream's LFE channel at +10 dB, times the subwoofer level, inverted if set. Every bonded
   endpoint has the room's whole stream, so each computes its own feed.
9. **Two-way** (in the chain, `EndpointDsp`/`chorus_dsp_endpoint_t`): the endpoint's own drivers,
   not the catalog's. Its one input (the role's channel, or unbonded the downmix `(FL + FR)/2`,
   the mono channel, or the mean of the main channels) is split by LR4 at its `crossover_hz` into
   a woofer (output 0) and a tweeter (output 1), each with a trim (cut only, to -24 dB), a delay
   and a polarity.
10. **The chain** (`Chain`, `chorus_dsp_chain_t`): configured from a `SoundSettings` (exactly the
    wire `sound` message's fields: `bass_db`, `treble_db`, `loudness`, `night`, `speech`,
    `room_eq_enabled`, `sub_polarity_inverted`, `role`, `sub_present`, `crossover_hz`,
    `sub_level_cdb`, up to 8 room-EQ filters `{freq_hz, gain_cdb, q_milli}`), the endpoint's
    `EndpointDsp`, the stream's channel map and rate. It processes interleaved frames given the
    room gain and the room's effective limit gain.

## The chain's order

Per frame:

1. room EQ (peaking filters, every channel but LFE, when enabled; a 0 dB filter is skipped);
2. tone: the bass low shelf and the treble high shelf (every channel but LFE);
3. loudness: the two ISO 226 shelves (every channel but LFE);
4. speech;
5. night;
6. bass management and the output's source, by role (role 0 and no two-way: every stream channel
   to its own output);
7. the two-way split;
8. each output's delay (`output_delay_us[o]`, plus the driver's `delay_us` when two-way);
9. times the room gain;
10. the look-ahead limiter at `min(1, limit gain)` (K81, I10: no DSP boost lifts a room above its
    limit).

Outputs: the stream's channel count (role 0, no two-way), 2 (two-way), else 1. The latency is
the limiter's look-ahead whatever the settings (2 ms: 96 frames at 48 kHz), so a setting never
moves the audio in time. A **flat** chain (the default settings, role 0, no two-way, no delays)
runs no filter: each output is its input times the room gain, delayed by the look-ahead, bit for
bit (`fixtures/dsp/chain-flat-*.txt`).

`set_sound` / `chorus_dsp_chain_set_sound` can be called on every `sound` message: a filter whose
gain changes keeps its state; a stage that switches on starts from zero state; a change of the
output layout (role, subwoofer presence, crossover, delays) resets the output side. A refused
setting changes nothing. A new stream (rate or channel map) or new endpoint configuration is a
new chain.

Fixed maximums (both languages): 8 stream channels, 8 outputs, 4800 delay frames per output and
9600 for all outputs together, a 768-frame look-ahead. A `chorus_dsp_chain_t` is 72680 bytes,
one static object; nothing on the audio path allocates.

## Defaults and ASSUMED values

Every value below that is not cited is ASSUMED: chorus's choice, not measured, and marked so in
the code.

| What | Value | Basis |
|---|---|---|
| Settings default | flat: everything off, role 0, crossover 80 Hz | the design envelope (the catalog's own defaults, e.g. loudness on, are the catalog's) |
| Crossover default | 80 Hz | cited (SVS, THX standard) |
| LFE into the subwoofer | +10 dB | cited (ATSC A/52:2018) |
| Bass shelf | 100 Hz, Q 1/sqrt 2, 1 dB per step | ASSUMED |
| Treble shelf | 8 kHz, Q 1/sqrt 2, 1 dB per step | ASSUMED |
| Corner limit | every corner held to 0.45 x the rate | ASSUMED |
| Loudness reference | 80 phon at unity volume | ASSUMED |
| Loudness evaluation | 50 Hz (low), 10 kHz (high) | ASSUMED |
| Loudness shelves | 100 Hz and 8 kHz, Q 1/sqrt 2 | ASSUMED |
| Loudness caps | +12 dB low, +6 dB high | ASSUMED |
| Loudness step | 0.5 dB | ASSUMED |
| Speech centre and Q | 2 kHz, Q 0.667 (1 to 4 kHz) | ASSUMED (the voice band "about 1-4 kHz" is a snippet) |
| Speech gain | +4 dB | cited (Geiger et al. 2015's 3.8 dB, rounded) |
| Night curve | threshold -24 dBFS, 3:1, 12 dB knee, +6 dB makeup | ASSUMED (the Dolby "Film Standard" profile in the research is a follow-up) |
| Night times | attack 10 ms, release 500 ms | ASSUMED (the research's pair; Dolby gives none) |
| Limiter look-ahead | 2 ms | ASSUMED |
| Limiter release | 100 ms time constant | ASSUMED |
| Limiter release snap | 1e-6 below unity | ASSUMED |
| Unbonded two-way input | `(FL + FR)/2`, else the mono channel, else the mean of the mains | ASSUMED |
| A role the stream lacks | silence (a mono stream plays on every main role) | ASSUMED |
| Two-way example | 2 kHz | ASSUMED (until goals 24-25 design the drivers) |
| Driver trim range | -24..0 dB | ASSUMED |

## Citations

All read 2026-10-01.

- RBJ Audio EQ Cookbook, W3C Working Group Note, 8 June 2021:
  <https://www.w3.org/TR/audio-eq-cookbook/>. The designs, and the stated properties the
  `rbj-*.txt` fixtures check.
- ITU-R BS.1770-5 (11/2023), Annex 1 Tables 1 and 2, the K-weighting filter's coefficients at
  48 kHz: <https://www.itu.int/dms_pubrec/itu-r/rec/bs/R-REC-BS.1770-5-202311-I!!PDF-E.pdf>.
- pyloudnorm (MIT licence, <https://github.com/csteinmetz1/pyloudnorm>), `meter.py` and
  `iirfilter.py`: the cookbook parameters that reproduce the K-weighting (high shelf 1500 Hz,
  +4 dB, Q 1/sqrt 2; high-pass 38 Hz, Q 0.5). Read, not copied.
- D. Bohn, "Linkwitz-Riley Crossovers: A Primer", RaneNote 160:
  <https://www.ranecommercial.com/legacy/note160.html>; and Linkwitz Lab, "Active Filters":
  <https://www.linkwitzlab.com/filters.htm>.
- J. O. Smith III, "Physical Audio Signal Processing", Delay Lines:
  <https://ccrma.stanford.edu/~jos/pasp/Delay_Lines.html>.
- Steinberg, "Brickwall Limiter":
  <https://www.steinberg.help/r/groove-agent/6.0/en/halion/topics/effects_reference/brickwalllimiter_r.html>
  ("the output level never exceeds a set limit").
- D. Giannoulis, M. Massberg, J. D. Reiss, "Digital Dynamic Range Compressor Design: A Tutorial
  and Analysis", JAES 60(6), 2012: <https://secure.aes.org/forum/pubs/journal/?ID=174> (the PDF
  could not be fetched); its static curve and the 10-90 % time constants as MathWorks prints
  them, citing it: <https://www.mathworks.com/help/audio/ref/compressor-system-object.html>.
- ISO 226:2003, "Acoustics: Normal equal-loudness-level contours":
  <https://www.iso.org/standard/34222.html> (sold); its parameter table and formula as reproduced
  at <https://www.dsprelated.com/showcode/174.php>.
- ATSC A/52:2018, section 3 (the LFE channel "is intended to be reproduced at a level +10 dB
  with respect to the fbw channels"): <https://www.atsc.org/wp-content/uploads/2021/04/A52-2018.pdf>;
  also <https://en.wikipedia.org/wiki/Bass_management>.
- SVS, "Tips for Setting the Crossover Frequency of a Subwoofer" ("The most common crossover
  frequency recommended (and the THX standard) is 80 Hz"):
  <https://www.svsound.com/blogs/subwoofer-setup-and-tuning/tips-for-setting-the-proper-crossover-frequency-for-a-subwoofer>.
- J. T. Geiger, P. Grosche, Y. Lacouture Parodi, "Dialogue enhancement of stereo sound", EUSIPCO
  2015 (the "simple center extraction and gain" baseline, "amplified (by 3.8 dB)"):
  <https://www.eurasip.org/Proceedings/Eusipco/Eusipco2015/papers/1570096395.pdf>.

No GPL source was opened. The code is written here from the formulas above.

## Regenerating the fixtures

`python3 tools/dsp-fixtures/generate.py` (by hand, never by the gate) rewrites `fixtures/dsp/`:
every expected value from the cited numbers and formulas, computed in Python independently of
both implementations, then the chain fixtures' `samples.<o>` lines from the Rust chain
(`cargo run -p chorus-dsp --example chain_samples -- <fixture>`).
