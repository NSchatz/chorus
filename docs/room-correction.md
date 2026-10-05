# Room correction

How chorus turns a recording of a sweep played in a room into the room's `room_eq` filters
(goal 12, K31). The measurement itself, a phone's microphone through the control app, is goal
22's (K87); this is the fitting, from a recording, and it runs on the server
(`crates/dsp/src/roomfit.rs`). An endpoint only runs the filters it is sent. The decision record
is `docs/decisions/0083-room-correction-fitting.md`; the sources, with what each says, are
`docs/research/room-correction-sources.md`.

Every value below is either cited (URL, read 2026-10-01) or marked **ASSUMED**: a starting point
for goal 22 to confirm on real rooms and real phones, not a measured fact.

## The bounds

The fitter only ever outputs filters inside the `room_eq` catalog bounds, the same numbers the
catalog refuses outside of and the wire carries (`docs/protocol.md`, `sound`):

| Field | Bound | Quantum |
|---|---|---|
| filters | at most 8 | |
| `freq_hz` | 20 to 1000 Hz | 1 Hz |
| `gain_db` | -12.00 to +3.00 dB | 0.01 dB |
| `q` | 0.5 to 10.0 | 0.001 |

It works on those quanta directly (whole Hz, centi-dB, milli-Q), so its output needs no rounding,
and it models each filter with the crate's own RBJ peaking design and `magnitude_db` evaluator
(the W3C Audio EQ Cookbook, https://www.w3.org/TR/audio-eq-cookbook/), so what it predicts is
what the endpoints run. Beyond the per-filter bounds it holds three rules of its own:

- **The combined boost** of all filters is at most `max_boost_db` (+3.00 dB, ASSUMED equal to
  the catalog's per-filter maximum) at every frequency, checked on the fit grid, on a 24 per
  octave grid from 10 Hz to 20 kHz and at every boosting filter's centre (REW's "Overall Max
  Boost", https://www.roomeqwizard.com/help/help_en-GB/html/eqwindow.html).
- **No boost below 100 Hz** (`min_boost_hz`, ASSUMED). An Android device's unprocessed
  microphone source need only be within +-20 dB of its midband from 5 to 100 Hz (Android 11
  CDD section 5.11, https://source.android.com/docs/compatibility/11/android-11-cdd), and iOS
  publishes nothing, so a dip below 100 Hz may be the phone's, and boosting it adds excursion.
- **A boost rings out within 0.5 s.** REW limits a boost's Q so its "60dB decay time" does not
  "exceed approximately 500 ms" (the same page). An RBJ peaking boost's poles have
  Q_pole = Q A, A = 10^(gain/40); a resonance decays 60 dB in ln(1000) Q_pole / (pi f0); so
  Q <= 0.2274 f0 / A. Cuts have Q_pole = Q / A < Q and need no such rule.

## The pipeline

1. **The sweep** (Farina, "Simultaneous measurement of impulse response and distortion with a
   swept-sine technique", AES 108th Convention, 2000, preprint 5093,
   https://angelofarina.it/Public/Papers/134-AES00.PDF, section 4):
   x(t) = A sin[K (e^(t/L) - 1)], K = w1 T / ln(w2/w1), L = T / ln(w2/w1). Its instantaneous
   frequency is f1 e^(t/L): equal time, and equal energy, per octave. Farina 2007 ("Advancements
   in impulse response measurements by sine sweeps", AES 122nd Convention, paper 7121,
   https://angelofarina.it/Public/Papers/226-AES122.pdf) recommends a short fade-in, no fade-out,
   and stopping at the last zero crossing; `Sweep::signal` does exactly that.
   `Sweep::recommended` is the sweep goal 22 starts from: 10 Hz to 20 kHz, 5 s, half full scale,
   0.1 s fade-in (ASSUMED values, from the research's recommendation: 10 Hz puts the fade-in an
   octave below the fit band; 5 s keeps an ASSUMED 100 ppm speaker-to-phone clock mismatch under
   a millisecond and the second harmonic 0.46 s ahead of the linear response).
2. **Refusals** before any arithmetic, each by name, so the measurement UX can say what to do:

   | Name | When | Default (ASSUMED) |
   |---|---|---|
   | `too_short` | fewer samples than the sweep plus the response window | sweep + 0.5 s |
   | `clipped` | a run of consecutive samples at or above the clip level | 3 samples at 0.999 FS |
   | `too_quiet` | the recording's peak below a floor | -50 dBFS |
   | `too_noisy` | the deconvolved response's peak stands too little above the noise before it | 40 dB |
   | `bad_config` | a configuration outside the bounds, or a sweep not covering the band | |

   The noise is measured in the deconvolved response between the second harmonic's arrival
   (L ln 2 before the peak) and the window, where only noise lands. 40 dB is where the fit
   began to degrade in a simulation of fixture 01's room under rising noise (seed 120001): at
   44.7 dB its cuts were within 0.25 dB of the noiseless fit's; at 39.7 dB they were 0.6 dB
   shallower and a spurious +1.3 dB boost appeared at 217 Hz; at 29.7 dB the cuts were half
   their depth. Fixture 07, at -16 dBFS of noise, stands 36.0 dB above it and is refused.
   Simulation, not a measurement.
3. **Deconvolution** (Farina 2000, section 3): the recording convolved, linearly (by the FFT,
   zero-padded so nothing wraps), with the inverse filter: the sweep reversed in time with an
   envelope "to reduce the level by 6 dB/octave, starting from 0 dB and ending to
   -6 log2(w2/w1)" (section 6), scaled so the sweep through it has unit gain in the band. The
   linear response peaks one sweep length after the sweep starts (any latency only moves it
   later; the peak is searched for, never assumed), and the N-th harmonic arrives L ln N before
   it, so a window from 5 ms before the peak (ASSUMED) to 0.5 s after (ASSUMED; REW's modal
   analysis length is 500 ms by default, the same page) holds the linear response alone. The
   window fades in over its 5 ms and out over its last quarter (ASSUMED). The FFT is a radix-2
   Cooley-Tukey of chorus's own (`roomfit/fft.rs`; Cooley and Tukey, Math. Comp. 19(90), 1965,
   https://doi.org/10.1090/S0025-5718-1965-0178586-1).
4. **Magnitude and smoothing**: the window's spectrum, zero-padded to at least 2^16 points
   (0.73 Hz bins at 48 kHz, ASSUMED), power-averaged over 1/12 octave (ASSUMED; REW offers 1/1
   to 1/48, https://www.roomeqwizard.com/help/help_en-GB/html/graph.html) around each point of a
   48 per octave log grid (ASSUMED), with a rectangular window in log frequency
   (Hatziantoniou and Mourjopoulos, "Generalized fractional-octave smoothing of audio and
   acoustic responses", JAES 48(4), 2000, https://aes2.org/publications/elibrary-page/?id=12070,
   abstract read; the rectangular window is ASSUMED). Power, not dB, is averaged, so a null weighs
   what its energy weighs.
5. **The level and the target**: the level is the median of the smoothed response from 300 Hz
   to 3 kHz (ASSUMED: above the modal region, inside the band a phone records within +-10 dB,
   the CDD's 100 Hz to 7 kHz). The target is flat at that level (ASSUMED; REW, Audyssey and Dirac
   default to flat, research 1d), or any curve through (Hz, dB) points (`Target::new`), e.g. a
   house curve with a low-frequency rise.
6. **The fit band**: from 20 Hz up to the modal region's top, 300 Hz (ASSUMED). The Schroeder
   frequency, f_s = 2000 sqrt(T60 / V) (Masovic, "Room Acoustics", TU Berlin lecture notes,
   arXiv:2111.01900, section 3.3, eq. 3.94 with the rounded constant), is where a room's
   response stops being a few separate modes that dominate every seat and becomes a dense
   statistical sum that differs a metre away. Typical living rooms: V = 40 m^3, T60 = 0.4 s
   gives 200 Hz; V = 100 m^3, T60 = 0.5 s gives 141 Hz. 300 Hz covers them with margin. The low
   edge rises to the speaker's -3 dB point (the first grid point within 3 dB of the level,
   ASSUMED), and below the target in the half octave above that edge the fit leaves the
   response alone (ASSUMED half an octave): no correction below a speaker's own roll-off (REW's
   rule, and Audyssey's "Low Frequency EQ Limit", research 1d).
7. **Nulls are left alone**: any dip that reaches 6 dB below the target (ASSUMED) is a null, a
   cancellation boost cannot fill because the boost cancels as well. Over the whole dip the fit
   pays for any change, boost or cut, and gains nothing.
8. **The greedy fit**: up to 8 times, take the largest weighted deviation in the band (a dip's
   error weighs half a peak's, ASSUMED, so cuts come first; points left alone weigh only their
   change), start a filter there with the opposite gain and a Q from the deviation's half-height
   width (the cookbook's Q for a bandwidth of N octaves, sqrt(2^N) / (2^N - 1)), refine it by
   coordinate descent on its three quanta, then refine every filter together twice. Every move
   is checked against the bounds and the three rules above before it is taken, so the fit never
   holds a filter outside them. It stops when no weighted deviation exceeds 1 dB (ASSUMED), when
   a new filter's gain would be under 0.5 dB (ASSUMED), when a round improves the cost by less
   than 1% (ASSUMED), or at 8 filters.

The result converts to the catalog's command with `room_eq_command_json(zone, &filters)`:
`{"v":2,"t":"room_eq","zone":"...","filters":[{"freq_hz":45,"gain_db":-9.32,"q":6.409},...],
"enabled":true}`.

## The fixtures and what the tests hold

`fixtures/roomfit/` (Rust-only by declaration, `docs/conventions.md` rule 9) holds synthetic
recordings: 1 s sweeps (10 Hz to 20 kHz, half full scale) through a speaker roll-off, known modes
and nulls (RBJ peaking filters: near its resonance a mode is a second-order resonance), a level
offset and seeded noise, at 48 kHz, 16-bit mono WAV, each beside a `.params` file holding every
value it was made from. `crates/dsp/tests/roomfit.rs` regenerates each byte for byte and
`make roomfit-fixtures` is how a parameter change is made. The 1 s sweep keeps a recording at
154 KB; goal 22's recommended 5 s sweep is held to the same Farina properties in its own test.

| Fixture | Room | Fit (defaults, flat target) | RMS deviation outside the nulls |
|---|---|---|---|
| 01 | modes 45 Hz +10 dB Q 5, 118 Hz +7 Q 4; null 72 Hz -18 Q 6; -12 dB; noise -70 dBFS | 45 Hz -9.32 dB Q 6.409, 119 Hz -6.53 Q 5.135 | 3.21 -> 0.37 dB |
| 02 | modes 38 +8 Q 7, 95 +6 Q 3, 210 +9 Q 6; null 150 -15 Q 8; -20 dB; noise -66 | 38 -7.23 Q 7.965, 94 -5.94 Q 3.41, 152 +1.37 Q 2.42, 211 -8.70 Q 6.366 | 3.38 -> 0.27 dB |
| 03 | modes 55 +16 Q 8, 170 +5 Q 2.5; -6 dB; noise -72 | 55 -12.00 Q 6.625, 55 -3.06 Q 9.82, 170 -4.97 Q 2.497 | 3.74 -> 0.28 dB |
| 04 | 01 at -66 dB | refused `too_quiet` (peak -65.2 dBFS) | |
| 05 | 01 at +6 dB | refused `clipped` (115 samples at full scale) | |
| 06 | 01 cut off at 0.75 s | refused `too_short` | |
| 07 | 01 under -16 dBFS of noise | refused `too_noisy` (36.0 dB) | |

These are synthetic rooms: the numbers say the fitter does what it is meant to on rooms whose
truth is known, not how well it corrects a real room. The tests assert: (a) at most 8 filters,
each inside the bounds and the boost rules; (b) a cut within 1/12 octave of every mode, the
correction there within 2.5 dB of the mode's height (or at least the 12 dB bound's worth for a
mode beyond it); (c) the RMS deviation outside the points left alone falls by at least 4 times
(the fixtures hold about 10), both on the fit's own smoothed estimate and against the room's true
response, no point ends more than 3 dB above the target, and no peak ends more than 0.5 dB above
where it was; (d) at every null, and everywhere from 10 Hz to 20 kHz, the combined correction is
at most +3.00 dB; (e) each degenerate recording is refused by its name. And the model is what the
endpoints run: each recording played through the fitted filters as the chain runs them (the
RBJ design rounded once to f32, Transposed Direct Form II in f32) and measured again matches the
fit's prediction (an RMS difference of 0.12 to 0.13 dB on the fixtures; under 0.25 dB asserted). Two worked examples hold
the method to Farina's paper: the sweep's measured instantaneous frequency equals
f1 e^((t/T) ln(f2/f1)) within 0.5%, and the sweep through its inverse filter is a single impulse
one sweep length late, peaking at the band's share of the spectrum ((f2 - f1) / (fs/2), within
5%), flat within 1 dB from 20 Hz to 10 kHz and with nothing above -60 dB of the peak more than
30 ms from it.

## Playing the sweep in a room

The playback half of the measurement is built: the control catalog's `measure_sweep` command
(`docs/control-plane.md`, "Room correction: the `measure_sweep` command";
`docs/decisions/0000-the-measurement-sweep-plays-on-a-stream-of-its-own.md`).

- **What is played** is `Sweep::recommended` at the stream's own rate, the samples
  `Sweep::signal` returns, written at the stream's sample format (16-bit: `round(x * 32768)`)
  and copied to every channel of the stream. The server renders it once, at start
  (`crates/server/src/sweep.rs`), so the sweep a room plays and the sweep the fitter
  deconvolves with are one function's output. Every channel carrying it is ASSUMED: it measures
  the room's speakers together, which is what `room_eq` corrects (one filter set per room).
- **Where it is played**: in one room. The sweep has a stream of its own beside the stream
  slots; the measured room's players are routed to it and back, so no source changes and the
  rest of the room's group keeps playing. That music is a noise source for the measurement
  when it is audible at the microphone: the fitter's `too_noisy` refusal is what catches it,
  and pausing the rest of the house first is the person's choice, not the command's.
- **Around it**: 0.5 s of silence before (ASSUMED: the fitter's response window, so what the
  room was playing has that long to ring out) and 1 s after (ASSUMED: the "sweep plus a
  second" below, so the recording's tail holds the room's response and not music). The state's
  `measurement` carries the three lengths (`lead_ms`, `sweep_ms`, `tail_ms`): record from the
  command's answer for at least their sum.
- **How loud**: the sweep's samples are half full scale, and the room's volume decides the
  rest. The command's optional `volume` sets it for the sweep, clamped to the room's effective
  limit (its own limit and any active quiet-hours window), and the room gets its own volume
  back afterwards. A recording that comes out `too_quiet` or `clipped` is measured again at
  another `volume`.
- **What is not claimed**: when the room hears the sweep. The sweep's chunks are stamped on
  the slots' grid like any stream's and play after the room's tier latency, and the fitter
  searches the recording for the response's peak rather than assuming where it is. No number
  for the sweep's alignment exists, and none is stated, until one is measured.
- **Open for the recording's task**: the sweep plays through the room's tone controls and any
  `room_eq` already set, so a room measured with filters enabled is the corrected room. Either
  the fit is applied on top of what is set, or the filters are switched off for the
  measurement (`room_eq` with `enabled` false); that choice belongs with accepting the
  recording.

## For goal 22 (the measurement)

- Ask the browser for `{echoCancellation: false, noiseSuppression: false, autoGainControl:
  false, channelCount: 1}` and record what `getSettings()` returns (W3C Media Capture and
  Streams, https://www.w3.org/TR/mediacapture-streams/; on WebKit, `echoCancellation: false` is
  the switch that turns the processing off, WebKit bug 179411).
- Play `Sweep::recommended(48_000)` (`measure_sweep`, above: one sweep per command), record at
  least the sweep plus a second, and repeat it two or three times; averaging the deconvolved
  responses aligned on their peaks is a follow-up.
- An uncalibrated phone's low end is the largest unknown: a microphone roll-off is smooth and
  cannot make a narrow mode, but a mode riding on it reads lower than it is. Detrending
  against a heavily (1 octave) smoothed copy before fitting, and per-model calibration (Sonos's
  Trueplay calibrates every iOS model, https://tech-blog.sonos.com/posts/trueplay-spectral-correction/)
  are follow-ups that need real phones.
- Averaging several seats, and the speaker-to-phone clock mismatch (Farina 2007 lists
  counter-skewing the response when the clocks differ), are follow-ups too.
