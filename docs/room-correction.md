# Room correction

How chorus turns a recording of a sweep played in a room into the room's `room_eq` filters
(goal 12, K31). The fitting, from a recording, runs on the server
(`crates/dsp/src/roomfit.rs`); the measurement, a phone's microphone through the control app
(K87), is "The measurement in the app" and "Per-phone limits" below. An endpoint only runs the filters it is sent. The decision record
is `docs/decisions/0083-room-correction-fitting.md`; the sources, with what each says, are
`docs/research/room-correction-sources.md`.

Every value below is either cited (URL, read 2026-10-01 unless its own date is given) or marked
**ASSUMED**: a starting point to confirm on real rooms and real phones, not a measured fact.

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
`docs/decisions/0195-the-measurement-sweep-plays-on-a-stream-of-its-own.md`).

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
- **The room is measured with its correction off**: the sweep plays through the room's tone
  controls and any `room_eq` already set, so a room measured with filters enabled is the
  corrected room. The recording's route refuses such a room by name (`correction_on`, below);
  the tone controls are the person's and stay as they are.

## The recording: uploaded, fitted, applied, undone

The recording half is built too: `POST /api/room-fit` and the catalog's `room_eq_undo`
(`docs/control-plane.md`, "Room correction: a recording, its fit and the undo";
`docs/decisions/0200-a-recording-is-fitted-and-not-kept.md`).

- **What is sent**: one mono recording, a 16-bit PCM WAV at 48 kHz, as the body of
  `POST /api/room-fit?zone=<room>` with `Content-Type: audio/wav`, at most 2 MiB. A recorder at
  another rate resamples first: 48 kHz is the rate every fixture above was made at, and a rate
  no test holds is refused (`unsupported_rate`) rather than fitted.
- **How the route is told which sweep was played**: in its query. `sweep_ms` is the sweep's
  length and `fade_in_ms` its fade-in; the frequencies and the level are always
  `Sweep::recommended`'s (10 Hz to 20 kHz, half full scale), which is what `measure_sweep`
  plays. With neither member the recording is of the sweep `measure_sweep` plays, 5 s with a
  0.1 s fade-in, so an app that used `measure_sweep` says nothing, or passes the
  `sweep_ms` the state's `measurement` gave it. With `sweep_ms` and no `fade_in_ms` there is no
  fade-in: that is the fixtures' sweep, `sweep_ms=1000`. The route builds the `Sweep` the
  fitter deconvolves with from those two numbers at the recording's rate, and nothing is
  guessed from the recording: fixture 01 sent without `sweep_ms=1000` is refused `too_short`,
  because it is shorter than a 5 s sweep and its response.
- **What comes back**: the fit's filters, spelled as `room_eq` spells them
  (`filter_json`), with the RMS deviation before and after; or the fitter's refusal under its
  own name (`too_short`, `clipped`, `too_quiet`, `too_noisy`) and in its own words, which carry
  the numbers. The fit is `fit_recording` with `Target::flat()` and `FitConfig::default()`,
  exactly what the table above was made with: the route adds no step of its own between the
  samples and the fitter.
- **Nothing is applied by the upload.** The person sees the filters and applies them with
  `room_eq`; that apply keeps what stood before it, and `room_eq_undo` puts it back (one step).
- **The room's correction must be off for the measurement.** A room whose `room_eq` is enabled
  with filters is refused `correction_on`: its sweep was the corrected room, and a fit of that
  recording would replace the correction with one for what is left of the room's response. The
  order is: `room_eq` with `enabled` false, `measure_sweep`, record, upload, `room_eq` with the
  new filters. Fitting on top of what is set (composing two filter sets inside 8 filters) is
  not built.
- **The recording is never stored**: it is held in memory for the fit and dropped. No file is
  written and the server's output gets one line of counts per upload.
- **What is not claimed**: that a phone's recording fits as these synthetic ones do. The
  fixtures are the only recordings the route has been given; the first real one is the next
  measurement to report.

## The measurement in the app

The measurement is built in the control app: a room's "Correction" screen
(`web/src/room-correction.js`, `web/src/capture.js`; `docs/app.md`, "A room's correction";
`docs/decisions/0000-the-room-is-recorded-with-an-audio-worklet.md`).

- **What the browser is asked for**: `{echoCancellation: false, noiseSuppression: false,
  autoGainControl: false, channelCount: 1}` (W3C Media Capture and Streams,
  https://www.w3.org/TR/mediacapture-streams/, read 2026-10-05). What the track's
  `getSettings()` then says is shown on the screen, each of the three as off, as kept on
  (flagged) or as not reported. The specification calls a setting "a target value that complies
  with constraints", which "may differ from measured performance at times": the screen shows
  what the browser says, not what its audio path did.
- **How it is recorded**: uncompressed, by an AudioWorklet in an audio context asked for at
  48 kHz, first channel only; where the browser gives another rate the page resamples to 48 kHz
  before the upload. The file is the route's: 16-bit mono WAV, `round(x * 32768)` held to the
  16-bit range.
- **The order**: the room's correction is switched off if it was on, the recording starts,
  `measure_sweep` is sent, and the recording ends one second (ASSUMED) after the state's
  `measurement` says `finished`; a sweep that is `cancelled`, or whose end is not heard of
  within its own length and five seconds (ASSUMED), is given up with nothing uploaded. The
  recording goes to `POST /api/room-fit` with no sweep in the query, which is the sweep
  `measure_sweep` plays, and the correction is switched back on if it was.
- **What is shown**: the fit's filters with the fitter's two RMS figures, called what they are
  (a prediction from one recording), and nothing is applied until "Apply" (`room_eq` with the
  filters). A refused recording is shown under the fitter's name and in its words, with what to
  do: `too_short` (keep the screen open until the sweep ends), `clipped` (turn the room down or
  move away), `too_quiet` (turn the room up, move closer, uncover the microphone), `too_noisy`
  (quiet the room or turn it up).
- **The correction afterwards**: the screen lists the room's filters as the server holds them,
  switches them off and on (`room_eq` with `enabled`) and undoes the last apply
  (`room_eq_undo`).
- **Where the recording goes**: to this server's route and nowhere else. The page writes it to
  no storage and keeps no sample once the route has answered; the microphone is stopped when
  the recording is taken, when a step fails and when the screen is left
  (`web/test/room-correction.test.js`).
- **What is tested, and what is not**: `web/live/room-correction.live.js` runs the screen
  against the real server, its sweep and its fitter, with the fixtures above in the
  microphone's place (the capture seam): fixtures 01 to 03 end with the table's filters in
  `GET /api/state`, 04 to 07 in their refusals. **No real phone has been measured**, and no
  real room.

## Per-phone limits

**No real phone has been measured with this.** Everything below is what a specification, a
vendor's documentation, a bug tracker's page or MDN's compatibility data says (each read
2026-10-05), or is marked **ASSUMED** or **not known**. It is the list of what the first real
measurements have to settle, not a description of how any phone behaves. No browser's source was
read.

**The three constraints.**

| | iOS Safari (WebKit) | Android Chrome |
|---|---|---|
| `echoCancellation: false` | The constraint is supported since Safari 11 (MDN browser-compat-data, `api/MediaStreamTrack.json`, `echoCancellation_constraint`, with Safari on iOS mirroring Safari; https://raw.githubusercontent.com/mdn/browser-compat-data/main/api/MediaStreamTrack.json). A WebKit engineer on the bug that fixed it: "When setting echoCancellation to false, we both disable AGC and echo cancellation" (https://bugs.webkit.org/show_bug.cgi?id=179411, comment 19, 2019; the bug is resolved fixed). A later comment there: "It might be that some filters are still active." | Supported since Chrome 59, Chrome Android mirroring Chrome (the same file). What Chrome's capture path on Android then does is **not known**: no Chromium document that says it was found. |
| `noiseSuppression: false` | The same file records the constraint as not supported in Safari (`noiseSuppression_constraint`, `version_added: false`). Whether WebKit's `echoCancellation: false` also turns noise suppression off is **not known**: the bug's comment names AGC and echo cancellation only. | Supported since Chrome 67 (the same file). The path behind it is **not known**, as above. |
| `autoGainControl: false` | Recorded as not supported in Safari (`autoGainControl_constraint`, `version_added: false`); the request for it is still open (https://bugs.webkit.org/show_bug.cgi?id=204444, "Add support for ... autoGainControl", status NEW). Per bug 179411's comment 19, `echoCancellation: false` is the switch that disables it. | Supported since Chrome 67 (the same file). |
| `channelCount: 1` | Recorded as not supported in Safari (`channelCount_constraint`, `version_added: false`). The app records the first channel whatever it is given. | Supported since Chrome 59. |

What follows for the screen: on an iPhone the browser is expected to report nothing, or `true`,
for `noiseSuppression` and `autoGainControl` whatever it did (**ASSUMED** from the rows above,
not seen), so the screen's flag there says what the browser reported and is not evidence of
processing; and on every phone a reported `false` is the browser's word, a "target value", not a
measurement of the path. Every iOS browser is WebKit outside the regions where Apple grants
another engine (App Store Review Guidelines 2.5.6, "Apps that browse the web must use the
appropriate WebKit framework and WebKit JavaScript", with an entitlement for an alternative
engine in the EU and Japan; https://developer.apple.com/app-store/review/guidelines/), so the
iOS column is Chrome's and Firefox's on an iPhone too.

**Sample rate.** The route takes 48 kHz only. The app asks for an audio context at 48 kHz (the
constructor's `sampleRate` option: Chrome 74, Firefox 61, Safari 14.1, with the phones'
browsers mirroring them; MDN browser-compat-data, `api/AudioContext.json`,
`options_sampleRate_parameter`), which per the Web Audio API sets the context's rate ("If
contextOptions.sampleRate is specified, set the sampleRate of context to this value",
https://www.w3.org/TR/webaudio/). At what rate a phone's microphone runs, whether Safari
reports `sampleRate` in `getSettings()`, and how each browser resamples a microphone into a
context at another rate are **not known**: no source read says. The screen shows the rate the
browser reports for the microphone and the rate it recorded at. Where the context is not at
48 kHz the page resamples (`resample`, `web/src/capture.js`); the fit of a resampled recording
has not been compared with the fit of the same recording at 48 kHz.

**The uncalibrated low end.** A phone's microphone has no calibration here, and the fit band
is 20 to 300 Hz. For Android the only bound found is the compatibility definition's, and it
binds only a device that offers the unprocessed source to apps ("If device implementations
intent to support unprocessed audio source and make it available to third-party apps"): such a
device "MUST exhibit approximately flat amplitude-versus-frequency characteristics in the
mid-frequency range: specifically ±10dB from 100 Hz to 7000 Hz", "MUST exhibit amplitude
levels in the low frequency range: specifically from ±20 dB from 5 Hz to 100 Hz compared to the
mid-frequency range", and "MUST not have any other signal processing (e.g. Automatic Gain
Control, High Pass Filter, or Echo cancellation) in the path other than a level multiplier"
(Android 11 CDD, section 5.11, https://source.android.com/docs/compatibility/11/android-11-cdd).
Whether Chrome records from that source is **not known**. For iOS nothing was found that
states a microphone's response. Measurement software handles this with a file of the
microphone's own response that is subtracted from what it measured (Room EQ Wizard,
https://www.roomeqwizard.com/help/help_en-GB/html/calfiles.html); the app has none. What the
fitter does about it is unchanged and is above: no boost below 100 Hz, and a level taken from
300 Hz to 3 kHz. A microphone's roll-off is smooth and cannot make a narrow mode, but a mode
riding on it reads lower than it is, so a cut below 100 Hz may be shallower than the room
needs. The screen says so in its guidance, and leaves the result to the person's ears, with the
switch and the undo beside it.

**The secure context.** `navigator.mediaDevices` exists only in a secure context (W3C Media
Capture and Streams: `[SameObject, SecureContext] readonly attribute MediaDevices
mediaDevices`), and MDN says the same of the AudioWorklet ("available only in secure contexts
(HTTPS), in some or all supporting browsers",
https://developer.mozilla.org/en-US/docs/Web/API/AudioWorklet). MDN's table of what is one
lists `http://localhost` as secure and `http://example.com` as not, and names an `https`
scheme or a loopback host among what makes an origin trustworthy
(https://developer.mozilla.org/en-US/docs/Web/Security/Defenses/Secure_Contexts): the server's
own plain `http` address on the house network is none of those, so the screen can measure only
where the app is served over HTTPS, which is how the household's reverse proxy serves it
(`docs/app.md`, "Where it is served"). Opened over plain HTTP the screen says so and asks for
nothing. The AudioWorklet is recorded as available from Chrome 66 and Safari 14.1, the phones'
browsers mirroring them (MDN browser-compat-data, `api/AudioWorklet.json`); a browser without
it is told that it cannot measure.

**Also not known**, and the phone check's to find (`docs/app.md`, "Phone check"): whether an
installed app on iOS is given the microphone and runs the worklet; whether a phone keeps
recording when its screen locks during the 6.5 s program (the guidance says to keep the screen
open); and how loud a sweep at a room's usual volume is at a phone's microphone, that is,
whether `clipped` or `too_quiet` is the common first answer.

## Follow-ups for the measurement

Not built, each needing real phones or real rooms first:

- Repeating the sweep two or three times and averaging the deconvolved responses aligned on
  their peaks. One sweep is one recording and one fit.
- A level for the sweep on the screen (`measure_sweep`'s `volume`).
- Detrending against a heavily (1 octave) smoothed copy before fitting, and per-model
  calibration (Sonos's Trueplay calibrates every iOS model,
  https://tech-blog.sonos.com/posts/trueplay-spectral-correction/, read 2026-10-01).
- Averaging several seats, and the speaker-to-phone clock mismatch (Farina 2007 lists
  counter-skewing the response when the clocks differ).
