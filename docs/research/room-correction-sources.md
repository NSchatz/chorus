# Room correction sources: the sweep, the Schroeder frequency, smoothing, auto-EQ bounds and phone microphones

Research for goal 12 (DSP), track `chorus-g12/roomfit`, 2026-10-01, by a research agent with the
coordinator's questions; what `docs/room-correction.md` and
`docs/decisions/0083-room-correction-fitting.md` rest on. This is section 1, the room-fit
recommendations and the room-fit sources of the goal-12 phase B research notes (which also cover
the visualizer, bass management, speech and night mode for the other tracks); paths under
`/cache/tmp/chorus-g12/` are working files outside the repository. Every URL was read on
2026-10-01. LEAD marks material that is not a citation.

## 1. Room correction fitting (track `roomfit`)

Context (design envelope, `/cache/tmp/chorus-g12/design.md`, "Phase B tracks" and `room_eq`):
the fitter emits at most 8 peaking filters, `freq_hz` 20..1000, `gain_db` -12.0..+3.0, `q`
0.5..10.0; the measurement is an uncalibrated phone microphone in a browser (K87).

### 1a. Exponential sine sweep (Farina)

Primary sources:

- A. Farina, "Simultaneous measurement of impulse response and distortion with a swept-sine
  technique", AES 108th Convention, Paris, 2000 February 19-22, preprint 5093.
  https://angelofarina.it/Public/Papers/134-AES00.PDF (read 2026-10-01).
- A. Farina, "Advancements in impulse response measurements by sine sweeps", AES 122nd
  Convention, Vienna, 2007 May 5-8, paper 7121.
  https://angelofarina.it/Public/Papers/226-AES122.pdf (read 2026-10-01).
- A. Farina, "Non-linear convolution: a new approach for the auralization of distorting
  systems", AES 110th Convention, Amsterdam, 2001 May 12-15.
  https://angelofarina.it/Public/Papers/154-AES110.PDF (read 2026-10-01; context only).

The sweep (Farina 2000, section 2): start angular frequency w1, end w2, duration T,

    x(t) = sin( K * (exp(t / L) - 1) ),   K = T * w1 / ln(w2 / w1),   L = T / ln(w2 / w1)

so the instantaneous angular frequency is w1 * exp(t / L) (equal time per octave). In samples,
with f1, f2 in Hz and rate fs: w = 2 pi f, t = n / fs.

The inverse filter (Farina 2000, section 4): "time-reversing the excitation signal, and then
applying to it an amplitude envelope to reduce the level by 6 dB/octave, starting from 0 dB and
ending to -6 log2(w2/w1)". Written as a formula (our transcription, equivalent to the quote):

    f(t) = x(T - t) * exp(-t / L),   0 <= t <= T

since exp(-T / L) = w1 / w2, i.e. 20 log10(w1/w2) = -6.02 log2(w2/w1) dB at the end. Farina 2007
(section "the ESS method") states the same in spectral terms: the sweep's spectrum is pink,
"falling down by -3 dB/octave", and the inverse filter's amplitude increases "by +3 dB/octave",
so x convolved with f is flat over [f1, f2]. Normalisation is free: scale f so the magnitude of
FFT(x conv f) is 1 (0 dB) at a mid-band frequency (e.g. 1 kHz). ASSUMED convention.

Deconvolution: h(t) = y(t) conv f(t), computed aperiodically (linear, not circular), so the
distortion products land before the linear response instead of folding into its tail (Farina
2007: "to implement the convolution aperiodically, for avoiding that the resulting impulse
response folds back"). The linear IR peak sits at a delay equal to the sweep length T after the
start of the recording-to-playback alignment (Farina 2007: "with a delay equal to the length of
the test signal").

Harmonic separation: the N-th harmonic's response packs at an earlier time. From Farina 2000's
instantaneous frequency w1 exp(t/L), the N-th harmonic N w1 exp(t/L) = w1 exp((t + L ln N)/L),
so it arrives

    dt_N = L * ln(N) = T * ln(N) / ln(w2 / w1)

before the linear peak (derived here from the paper's K and L; Farina 2000 describes the
"distortion peaks at very precise anticipatory times" and the log sweep's "delay ... increasing,
for example, of 1s each octave"). Worked value: T = 10 s, 20 Hz to 20 kHz, L = 10 / ln(1000) =
1.4476 s, dt_2 = 1.003 s, dt_3 = 1.590 s. So a linear-IR window must start less than dt_2 before
the peak (a few ms of pre-window is ample) and its length bounds the lowest resolvable frequency.

Pitfalls the 2007 paper documents (section "pre-ringing"):

- fade-in and fade-out cause sinc-like pre-ringing; its example is "fs=44100 Hz, sweep from
  22 Hz to 22050 Hz, 15s long, 0.1s fade-in, no fade-out"; it recommends sweeping up to Nyquist
  and cutting at "the latest zero-crossing before its abrupt termination" instead of a fade-out.
  For a phone through a browser we do not need above about 16 kHz, but the rule holds: no
  fade-out, a short fade-in (0.1 s at 22 Hz start), and stop at a zero crossing.
- frequency-domain "exact" deconvolution uses Kirkeby regularisation
  C(f) = Conj(H(f)) / (Conj(H(f)) H(f) + eps(f)) with eps small inside the swept band and large
  outside (equation 4).
- clock mismatch between the playback and recording devices skews the IR; Farina 2007 lists
  "counter-skewing of the measured impulse response when the playback and recording digital
  clocks are mismatched". Our case (speaker DAC on the chorus clock, phone ADC on its own) is
  exactly that. Mitigation: keep the sweep short (a few seconds) so a 100 ppm mismatch is under
  1 ms over the sweep, and look for the IR peak instead of assuming its sample index. The 100 ppm
  figure is ASSUMED, not cited.

Recommended sweep for chorus (ASSUMED values except where cited): fs 48 kHz, f1 = 10 Hz (an
octave below the fit band so the 0.1 s fade-in sits outside it), f2 = 20 kHz, T = 5 s, fade-in
0.1 s, no fade-out, stop on a zero crossing; 1 s of silence recorded after (room tail); repeat
2 to 3 times and average the deconvolved IRs aligned on their peaks. With T = 5 s, f1 = 10 Hz,
f2 = 20 kHz: L = 0.6578 s, dt_2 = 0.456 s, so a linear-IR window of up to about 0.4 s before the
next sweep is clean; a 0.5 s analysis window (REW's default "Analysis Length", below) still
fits after the peak because the harmonics are before it.

### 1b. Schroeder frequency: why correct only the modal region

Primary source: D. Masovic, "Room Acoustics (lecture notes)", TU Berlin 2018, updated
2021-11-04, arXiv:2111.01900, section 3.3, equations 3.93 and 3.94.
https://arxiv.org/pdf/2111.01900 (read 2026-10-01).

- Eq. 3.94: f_Schroed ~ 2100 sqrt(T60 / V) (T60 in s, V in m^3), from requiring three modes per
  modal bandwidth ("the value of 3 is chosen quite arbitrarily"). The commonly quoted constant
  is 2000 (same derivation, rounded): f_s = 2000 sqrt(T60 / V).
- Meaning (quote): it "indicates roughly the low-frequency range in which resonant behaviour of
  a room can be pronounced and perceivable", and "Above the Schroeder frequency the overlap of a
  large number of modes results in a much smoother frequency response of the room", "not a clear
  limit between the two regimes".

Domestic values (computed here with the 2000 constant): V = 40 m^3, T60 = 0.4 s gives 200 Hz;
V = 60 m^3, T60 = 0.5 s gives 183 Hz; V = 100 m^3, T60 = 0.5 s gives 141 Hz. Above f_s the
response varies with position statistically, so a single-point phone measurement there would
"correct" position-specific comb filtering that is different a metre away; below it the modes are
few, high-Q, and dominate the bass at every seat, so cuts there are the useful correction. This is
also where the tools in 1d concentrate (REW "less than 200 Hz or so").

### 1c. Fractional-octave smoothing

Primary sources:

- P. D. Hatziantoniou and J. N. Mourjopoulos, "Generalized fractional-octave smoothing of audio
  and acoustic responses", JAES 48(4), 259-280, 2000 (plus Addendum, JAES 48(10), 2000).
  https://aes2.org/publications/elibrary-page/?id=12070 (abstract page read 2026-10-01; the paper
  itself is paywalled and was not read).
- REW help, "Graph menu" smoothing options. https://www.roomeqwizard.com/help/help_en-GB/html/graph.html
  (read 2026-10-01): choices 1/1, 1/2, 1/3, 1/6, 1/12, 1/24, 1/48 octave; "Var" smoothing is
  "1/48 octave below 100 Hz, 1/3 octave above 10 kHz and varies between 1/48 and 1/3 octave
  from 100 Hz to 10 kHz, reaching 1/6 octave at 1 kHz"; "Psychoacoustic" is "1/3 octave below
  100Hz, 1/6 octave above 1 kHz and varies from 1/3 octave to 1/6 octave between 100 Hz and
  1 kHz".
- REW help, EQ window. https://www.roomeqwizard.com/help/help_en-GB/html/eqwindow.html (read
  2026-10-01): "It is best to apply the 'variable' smoothing to the response before running the
  target match."
- Sonos, "Trueplay Spectral Correction" (tech blog). https://tech-blog.sonos.com/posts/trueplay-spectral-correction/
  (read 2026-10-01): "less smoothing at lower frequencies ... more smoothing at higher
  frequencies" (no fractions given).

Method (power smoothing, the simple case of the Hatziantoniou framework): for each output
frequency f on a log grid and a smoothing width of w octaves (w = 1/6 for 1/6 octave), average
|H|^2 over the bins in [f 2^(-w/2), f 2^(w/2)], then convert to dB. Use a rectangular
window in log frequency (ASSUMED; the paper also gives Hann-shaped windows). Recommendation for
the fitter: REW-style variable smoothing (1/48 octave below 100 Hz, rising to 1/6 at 1 kHz),
which keeps modal peaks sharp where they are fitted. For an uncalibrated phone a fixed 1/12
octave over 20..1000 Hz is a simpler, testable alternative (ASSUMED).

A test can check smoothing against a property: power smoothing of a flat response is flat, and
power smoothing of a single-bin spike of energy E in a uniform-bin spectrum spreads it over the
window (energy conserved within the window). Both are properties, not printed numbers.

### 1d. Bounds used by established auto-EQ tools

| Tool | What its public docs state | Source (read 2026-10-01) |
|---|---|---|
| REW auto-EQ | separate "Individual Max Boost" and "Overall Max Boost", both may be 0 (cut only); "boost filters are subject to Q limits to avoid inadvertently creating artificial resonances": boost Q may not make the filter's "60dB decay time to exceed approximately 500 ms"; max Q 5.0 above 200 Hz, or (option "Vary max Q above 200 Hz") from 10.0 at 200 Hz to 3.0 at or above 10 kHz; match range typically "less than 200 Hz or so"; no filters "below the frequency at which the measurement first exceeds the target or above the frequency at which the measurement last drops below the target to prevent trying to boost a response beyond its natural roll-offs"; variable smoothing before matching; modal analysis length "500 ms by default" | https://www.roomeqwizard.com/help/help_en-GB/html/eqwindow.html |
| Sonos Trueplay | "a parametric equalizer ... has sixteen filters" (IIR); per-product in-room target; per-iOS-model microphone calibration curves ("We measure every new iOS device and create a Trueplay calibration curve for it"); test tone period "about a third of a second", played "for 45 seconds" (mono) while the user moves through "more than 150 places"; smoothing frequency-dependent | https://tech-blog.sonos.com/posts/trueplay-spectral-correction/ ; https://support.sonos.com/en-us/article/tune-your-sonos-speakers-with-trueplay |
| Audyssey MultEQ-X | "MultEQ filters often provide up to 9dB of boost" (with a speaker-damage warning); detects each speaker's LF roll-off and does not EQ below it ("Low Frequency EQ Limit"); default target "Reference" = flat with "High Frequency Roll-off 1" (rooms under 2500 cu. ft.) and "Midrange Compensation" on | https://audyssey.com/MultEQ-X%20User%20Guide%201.1.pdf (pages 25-29) |
| Genelec GLM 3 AutoCal | room response equaliser = LF shelving (1-2), HF shelving (3-4) and "Parametric notch filters (5-20)", count product-dependent (82xx: 5-11) | https://assets.ctfassets.net/4zjnzn055a4v/7dglsmyMvsRKB46QDSSvsw/9765e695838d2ba01dd4823f43a75b89/GLM_3_System_Operating_Manual.pdf (pages 61-62) |
| Genelec AutoCal2 | support article "Why does GLM AutoCal2 use filters with positive gain?" exists; LEAD only (HTTP 403, not read); a third-party summary says positive-gain filters have "monitor model specific restrictions to the gain, Q-value and frequency range" | https://support.genelec.com/hc/en-us/articles/4404366109458 (LEAD) |
| Dirac Live | default target flat; range "curtains" default to the detected speaker range; 22 Hz to 20 kHz and 80 Hz crossover in the Bass Control example | LEAD: https://docs.minidsp.com/product-manuals/shd/dirac-live/filter-design.html (403, search snippet only) and https://www.arcam.co.uk/ugc/tor/AV40/Dirac%20Live%20Bass%20Control/Bass%20Control%20in%20Live_07Apr20.pdf (not read) |

The common pattern: cut-dominant, boosts small and broad, no EQ below the speaker's own roll-off,
narrow filters allowed only at low frequencies, a modest filter count (8 to 16).

Derived boost-Q rule (REW's 500 ms criterion turned into a formula; our derivation, not REW's
text): a resonance of centre f0 and quality Q decays with T60 = ln(1000) Q / (pi f0) =
2.199 Q / f0. An RBJ peaking boost has poles at Q_pole = Q * A (cookbook: a0 = 1 + alpha / A,
so the pole damping is alpha / A), A = 10^(gain_dB / 40). Requiring T60 <= 0.5 s gives

    Q_boost_max(f0, gain_dB) = 0.2274 * f0 / A

e.g. at +3 dB (A = 1.189): 20 Hz gives 3.8, 40 Hz gives 7.7, 52 Hz and above gives the catalog's
10.0. Cuts have Q_pole = Q / A < Q, so cuts need no such rule.

### 1e. Phone microphones through a browser

- W3C Media Capture and Streams, Candidate Recommendation Draft 2025-10-09,
  https://www.w3.org/TR/mediacapture-streams/ (read 2026-10-01): `echoCancellation`,
  `autoGainControl` ("There are cases where it is not needed and it is desirable to turn it off
  so that the audio is not altered"), `noiseSuppression` (same wording), `sampleRate`,
  `sampleSize`, `channelCount`, `latency` are constrainable properties.
- WebKit bug 179411 "getUserMedia echoCancellation constraint has no affect", RESOLVED FIXED
  2019-11-19, https://bugs.webkit.org/show_bug.cgi?id=179411 (read 2026-10-01): the fix note says
  "when setting echoCancellation to false, we both disable AGC and echo cancellation"; the
  original report saw an apparent low-pass near 12 kHz with processing on.
- WebKit bug 204444 "Add support for ... autoGainControl", status NEW, last modified 2026-04-06,
  https://bugs.webkit.org/show_bug.cgi?id=204444 (read 2026-10-01): Safari (and so every iOS
  browser, all WebKit) has no separate `autoGainControl` constraint; `echoCancellation: false`
  is the switch that turns processing off.
- MDN `MediaTrackSettings.autoGainControl`, "Limited availability ... not Baseline",
  https://developer.mozilla.org/en-US/docs/Web/API/MediaTrackSettings/autoGainControl (read
  2026-10-01).
- Android CDD (Android 11), section 5.11 "Capture for Unprocessed",
  https://source.android.com/docs/compatibility/11/android-11-cdd (read 2026-10-01): a device
  that offers `AudioSource.UNPROCESSED` MUST be "flat ... ±10dB from 100 Hz to 7000 Hz", "±20 dB
  from 5 Hz to 100 Hz compared to the mid-frequency range", "±30 dB from 7000 Hz to 22 KHz", 94 dB
  SPL at 1 kHz gives "-36 dB Full Scale" for float samples, SNR >= 60 dB, THD < 1% at 90 dB SPL,
  and no AGC, high-pass or echo cancellation. That is the best case for an app; whether a browser
  page gets the UNPROCESSED source is not stated by the CDD (not verified).
- Sonos (above) calibrates every iOS model's microphone; NIOSH (Kardous and Shaw, JASA 2014)
  found only some iOS apps within ±2 dB of a reference for A-weighted levels, and calibrated
  external microphones within ±1 dB (follow-up, JASA 140(4) EL327, 2016);
  https://www.cdc.gov/niosh/bulletin/2014/sound-app.html (search summary only; LEAD).

Consequences for chorus (K87, no calibration file): request
`{echoCancellation: false, noiseSuppression: false, autoGainControl: false, channelCount: 1}`
and read back `getSettings()`; record the returned values in the measurement. Even then the
microphone's magnitude below 100 Hz is only bounded to ±20 dB relative to midband (Android CDD),
and on iOS it is uncharacterised. A smooth microphone roll-off cannot create a narrow modal peak,
so the fit should (a) detrend: subtract a heavily smoothed (1 octave) version of the measured
response before looking for peaks, or equivalently compare against the median level, and (b)
cut peaks, never fill dips with boost below 100 Hz. Boosts (max +3 dB) only at 100 Hz and above.
These are design inferences from the CDD tolerances, ASSUMED until measured on real phones.

### 1f. Worked examples a test can check

- RBJ peaking EQ (W3C Working Group Note "Audio EQ Cookbook", 2021-06-08,
  https://www.w3.org/TR/audio-eq-cookbook/, read 2026-10-01): b0 = 1 + alpha A,
  b1 = -2 cos w0, b2 = 1 - alpha A, a0 = 1 + alpha / A, a1 = -2 cos w0, a2 = 1 - alpha / A,
  A = 10^(dBgain/40); BW is between "midpoint (dBgain/2) gain frequencies for peaking EQ";
  1/Q = 2 sinh(ln(2)/2 BW w0 / sin w0). Testable properties: |H(f0)| = dBgain exactly; the
  magnitude at the two midpoint frequencies (f0 2^(-BW/2), f0 2^(BW/2)) is close to dBgain / 2
  (exact in the analog prototype; the digital BW formula's w0/sin w0 term is a warping
  approximation, so assert with a tolerance, e.g. 0.1 dB below fs/10, ASSUMED); a cut of -g dB and a boost of
  +g dB at the same f0 and Q multiply to exactly 0 dB at every frequency (the cookbook's
  peaking EQ is gain-symmetric: swapping A for 1/A swaps numerator and denominator). Audyssey
  states the same symmetry for its "Parametric Peaking Filter" (MultEQ-X guide, page 28).
- Ground-truth synthetic rooms (the design's `fixtures/roomfit/`): build a room as a cascade of
  known peaking filters inside the catalog bounds plus a smooth microphone tilt; the exact
  inverse (same f0 and Q, negated gain) is a valid answer, so the fitter's residual can be
  asserted against it. This is a self-consistent property, not a published number.
- No openly readable published "measured room -> fitted PEQ table" example was found in the time
  budget; REW, Dirac and Audyssey document parameters, not reference fits.

Recommendation for 1 (also in the final section):

- Fit range: default 30 Hz to 300 Hz (above the domestic Schroeder frequencies of 140 to 200 Hz
  with margin), never below the measured response's own LF roll-off (REW and Audyssey rule);
  the catalog's 20..1000 Hz stays the hard bound. 300 Hz is ASSUMED from 1b's arithmetic.
- Gain: cut to -12 dB (catalog), boost to +3 dB only at 100 Hz and above, and the summed boost
  of all filters at any frequency <= +3 dB (REW's "Overall Max Boost" idea).
- Q: cuts 0.5..10 (catalog; REW allows up to 10 at 200 Hz); boosts Q <= min(10, 0.2274 f0 / A).
- Count: at most 8 (catalog); stop early when the improvement of an extra filter is < 0.5 dB RMS
  (ASSUMED).
- Smoothing: REW-style variable smoothing (1/48 oct below 100 Hz to 1/6 at 1 kHz), or a fixed 1/12
  octave; then detrend with a 1-octave smoothed copy for peak picking.
- Target: flat at the median of the smoothed response over the fit band (cut peaks down to it).
- Quality criterion (ASSUMED thresholds a test asserts on the synthetic rooms): every filter is
  inside the catalog bounds; over the fit band on a 1/24-octave log grid, the RMS of (smoothed
  corrected response minus target) is at most half of the uncorrected RMS, the largest positive
  deviation after correction is <= 3 dB, and no grid point that was above the target before correction ends
  more than 0.5 dB above its uncorrected value (a cut never makes a peak worse).

## Recommendations for the roomfit track

Each value with its basis. ASSUMED marks a value with no direct citation.

Track `roomfit`:

| Item | Value | Basis |
|---|---|---|
| Sweep | x(t) = sin(K (exp(t/L) - 1)), K = T w1 / ln(w2/w1), L = T / ln(w2/w1) | Farina 2000, section 2 |
| Inverse filter | f(t) = x(T - t) exp(-t / L), then normalise to 0 dB at 1 kHz | Farina 2000, section 4 (envelope -6 dB/oct, 0 to -6 log2(w2/w1)); normalisation ASSUMED |
| Deconvolution | linear (aperiodic) convolution; IR peak at the sweep length; harmonic N at L ln N earlier | Farina 2000; Farina 2007 |
| Sweep parameters | 48 kHz, 10 Hz to 20 kHz, T = 5 s, 0.1 s fade-in, no fade-out, end on a zero crossing, 1 s tail, 2 to 3 repeats aligned on the peak | fade rule Farina 2007; numbers ASSUMED |
| Fit range | 30 Hz to 300 Hz default, never below the measured LF roll-off; hard bound 20..1000 Hz | Schroeder 140-200 Hz for domestic rooms (Masovic eq. 3.94, computed); REW "less than 200 Hz or so" and roll-off rule; Audyssey LF limit; 300 Hz ASSUMED |
| Smoothing | variable: 1/48 oct below 100 Hz, to 1/6 oct at 1 kHz (REW "Var"); or fixed 1/12 oct | REW graph help; REW EQ help ("apply the 'variable' smoothing") |
| Detrend | subtract a 1-octave-smoothed copy before peak picking | ASSUMED, motivated by the Android CDD ±20 dB below 100 Hz |
| Target | flat at the median of the smoothed response over the fit band | ASSUMED (REW "Target Level"; Dirac default flat) |
| Max cut | -12 dB per filter | catalog bound; tools are cut-dominant (REW, GLM notch filters) |
| Max boost | +3 dB per filter and +3 dB summed, only at >= 100 Hz | catalog bound; REW individual/overall max boost; Audyssey "up to 9dB" warning; 100 Hz from Android CDD |
| Q | cuts 0.5..10; boosts <= min(10, 0.2274 f0 / A), A = 10^(g/40) | catalog; REW max Q 10 at 200 Hz and the 500 ms decay rule (formula derived here) |
| Count | <= 8, stop when the next filter improves RMS by < 0.5 dB | catalog (Sonos uses 16, GLM up to 16); stop rule ASSUMED |
| Quality test | filters in bounds; fit-band RMS error (1/24-oct grid) <= 50% of uncorrected; max positive deviation <= 3 dB; no above-target point worsens by > 0.5 dB | ASSUMED thresholds |
| Peaking property tests | magnitude at f0 = gain; half gain at the BW edges; +g and -g at the same f0, Q cancel exactly | W3C Audio EQ Cookbook |
| Browser capture | getUserMedia with echoCancellation, noiseSuppression, autoGainControl all false, channelCount 1; record getSettings() | W3C Media Capture; WebKit bugs 179411 (echoCancellation false also disables AGC on Safari) and 204444 (no autoGainControl on WebKit) |

## What was read (the room-fit sources)


All on 2026-10-01. "Read" means the text was fetched and the cited passage seen; "summary" means
only a search-engine or fetch summary was seen; "LEAD" items are not citations.

1. Farina 2000, AES 108th preprint 5093: https://angelofarina.it/Public/Papers/134-AES00.PDF (read)
2. Farina 2001, AES 110th: https://angelofarina.it/Public/Papers/154-AES110.PDF (read, context)
3. Farina 2007, AES 122nd paper 7121: https://angelofarina.it/Public/Papers/226-AES122.pdf (read)
4. Masovic, Room Acoustics lecture notes, arXiv:2111.01900: https://arxiv.org/pdf/2111.01900 (read, section 3.3)
5. Hatziantoniou and Mourjopoulos 2000, JAES abstract page: https://aes2.org/publications/elibrary-page/?id=12070 (summary only; paper paywalled)
6. REW help, EQ window: https://www.roomeqwizard.com/help/help_en-GB/html/eqwindow.html (read)
7. REW help, graph smoothing: https://www.roomeqwizard.com/help/help_en-GB/html/graph.html (read)
8. Sonos tech blog, Trueplay Spectral Correction: https://tech-blog.sonos.com/posts/trueplay-spectral-correction/ (read)
9. Sonos support, Trueplay: https://support.sonos.com/en-us/article/tune-your-sonos-speakers-with-trueplay (summary)
10. Audyssey MultEQ-X User Guide 1.1: https://audyssey.com/MultEQ-X%20User%20Guide%201.1.pdf (read, pages 25-29)
11. Genelec GLM 3 System Operating Manual: https://assets.ctfassets.net/4zjnzn055a4v/7dglsmyMvsRKB46QDSSvsw/9765e695838d2ba01dd4823f43a75b89/GLM_3_System_Operating_Manual.pdf (read, pages 26-27, 60-62)
12. Genelec support, AutoCal2 positive gain: https://support.genelec.com/hc/en-us/articles/4404366109458 (HTTP 403; LEAD)
13. miniDSP Dirac Live filter design page: https://docs.minidsp.com/product-manuals/shd/dirac-live/filter-design.html (HTTP 403; LEAD)
14. W3C Media Capture and Streams CRD 2025-10-09: https://www.w3.org/TR/mediacapture-streams/ (read)
15. WebKit bug 179411: https://bugs.webkit.org/show_bug.cgi?id=179411 (read)
16. WebKit bug 204444: https://bugs.webkit.org/show_bug.cgi?id=204444 (read)
17. MDN MediaTrackSettings.autoGainControl: https://developer.mozilla.org/en-US/docs/Web/API/MediaTrackSettings/autoGainControl (read)
18. Android 11 CDD section 5.11: https://source.android.com/docs/compatibility/11/android-11-cdd (read)
19. NIOSH smartphone app studies: https://www.cdc.gov/niosh/bulletin/2014/sound-app.html (summary; LEAD)
20. W3C Audio EQ Cookbook Note 2021-06-08: https://www.w3.org/TR/audio-eq-cookbook/ (read)

Not opened (clean-room, K33): no GPL or other reciprocal-licence source file was opened. WLED
(EUPL) and its MoonModules fork were consulted through their documentation pages only; a GitHub
link to MoonModules source seen in a docs page was not followed. The CRAN "SII" package was not
opened.
