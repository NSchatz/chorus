# Research: DSP phase B (room fit, visualizer, bass management, speech and night)

Goal 12 (DSP), research for the phase B tracks `roomfit`, `visualizer` and `endpoint-dsp`.
Read 2026-10-01. Every number carries its source URL and the date read. Forum or blog material
is marked LEAD and is not a citation. ASSUMED marks values with no direct citation.

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

## 2. Visualizer (track `visualizer`)

Wire context (`docs/protocol.md`): `0x34 visualizer_frame` = timestamp_ns, `beat` (u8 strength,
0 none), `peak` (u8 0..255), band count 0..64 (at most the endpoint's `visualizer_bands`),
`bands` (u8 levels, low to high). `0x35 color` = timestamp_ns, R, G, B, brightness 0..255,
`transition_ms` u16.

### 2a. Onset and beat detection

Primary sources:

- J. P. Bello, L. Daudet, S. Abdallah, C. Duxbury, M. Davies, M. B. Sandler, "A Tutorial on Onset
  Detection in Music Signals", IEEE Trans. Speech and Audio Processing 13(5), Sept. 2005,
  1035-1047. https://hajim.rochester.edu/ece/sites/zduan/teaching/ece472/reading/Bello_2005.pdf
  (read 2026-10-01).
- S. Dixon, "Onset Detection Revisited", Proc. DAFx-06, Montreal, 2006.
  https://www.dafx.de/paper-archive/2006/papers/p_133.pdf (read 2026-10-01).

Method (both papers): reduce the audio to a detection function at a low rate (Dixon: "usually
have a low sampling rate (e.g. 100Hz)"), then peak-pick against an adaptive threshold.

Spectral flux (Dixon 2006, section 2.1, the simplest of his top performers):

    SF(n) = sum over bins k of H(|X(n,k)| - |X(n-1,k)|),   H(x) = (x + |x|) / 2

"Empirical tests favoured the use of the L1-norm here over the L2-norm ... and the linear
magnitude over the logarithmic". STFT: "window size N = 2048 (46 ms at a sampling rate of
r = 44100 Hz) and hop size h = 441 (10 ms, or 78.5% overlap)", "using a Hamming window",
"calculated at a frame rate of 100 Hz". Result on 106054 piano
onsets: SF precision 0.958, recall 0.969, F 0.964, mean absolute error 8.8 ms (Table 2); "spectral
flux has the advantage of being the simplest and fastest algorithm".

Peak picking (Dixon 2006, section 2.6): normalise f(n) to mean 0, standard deviation 1; frame n
is an onset if (1) f(n) >= f(k) for n - w <= k <= n + w; (2) f(n) >= (sum of f(k) for
k = n - m w .. n + w) / (m w + w + 1) + delta; (3) f(n) >= g_a(n - 1) where
g_a(n) = max(f(n), a g_a(n - 1) + (1 - a) f(n)); with "w = 3" and "m = 3". delta and a were
tuned per data set (not printed as fixed values), so they are ASSUMED here: delta = 0.1 after
normalisation (ASSUMED), a = 0.9 (ASSUMED). The look-ahead is w hops = 30 ms, which is fine
because the server analyses audio well before it is heard (frames are stamped with the time the
audio is heard).

Bello 2005 (section IV) gives the equivalent median form: the threshold is delta + lambda times
the moving median of |d| over a window of about 100 ms ("set to the longest time interval on
which the global dynamics are not expected to evolve (around 100 ms)", lambda "set to 1").
Evaluation tolerance in both: an onset matches within 50 ms.

Streaming normalisation: the "mean 0, std 1" step needs a running estimate; use exponential
moving mean and variance over about 3 s (ASSUMED, matched to the EBU short-term window, 2e).
`beat` byte: 0 when no onset; otherwise min(255, round(64 * z)) where z is the normalised flux
above threshold (ASSUMED scale). A tempo tracker (Scheirer 1998 comb filters, cited in Bello
section II as six-band elliptic filter bank plus comb resonators) is not needed for LED beat
flashes and is deferred; Scheirer's paper itself was not read.

Fixture check a test can make: an impulse train (clicks) at 120 BPM over silence or pink noise
must give onsets every 500 ms within the papers' 50 ms tolerance; a steady sine gives none
after the first frame.

### 2b. Frame rates

| System | Documented rate | Source (read 2026-10-01) |
|---|---|---|
| WLED (MoonModules) audio sync | "one packet every 20 milliseconds (approx)", 16 GEQ channels as u8, all sample data "[0...255]" | https://mm.kno.wled.ge/soundreactive/sync/ (docs page only; WLED code not opened) |
| Philips Hue Entertainment | quoted "From Hue Docs": "a streaming rate of 50-60Hz is used"; "The bridge sends maximum at 25 Hz messages over ZigBee. Thus, the (fastest) effect rate should be 2 - 3 times slower than this 25 Hz, i.e < 12.5 Hz" | LEAD: https://pkg.go.dev/github.com/rschio/huestream (BSD-2-Clause package docs quoting the official docs); https://iotech.blog/posts/philips-hue-entertainment-api/ (LEAD); the official page https://developers.meethue.com/develop/hue-entertainment/hue-entertainment-api/ needs a login (not read) |
| Nanoleaf external control | stream "no faster than 10Hz" | LEAD: search snippet of https://nanoleaf.atlassian.net/wiki/spaces/nlapid/pages/2789310530/ (the fetched page did not include section 5.7) |
| EBU Mode loudness meter | "The update rate for 'live meters' shall be at least 10 Hz" (momentary and short-term) | https://tech.ebu.ch/docs/tech/tech3341.pdf (V4, November 2023) |
| Photosensitivity | WCAG 2.2 SC 2.3.1: nothing "flashes more than three times in any one second period" unless below the general and red flash thresholds | https://www.w3.org/WAI/WCAG22/Understanding/three-flashes-or-below-threshold.html |

Recommendation: `visualizer_frame` at 50 Hz (20 ms hop, WLED's rate and the Hue stream rate;
it also gives the onset detector a 20 ms hop, coarser than Dixon's 10 ms but inside the 50 ms
tolerance). `color` at most 10 Hz with `transition_ms` = the frame interval (100 ms), so smart
lights fade instead of step (Nanoleaf's 10 Hz cap; Hue's < 12.5 Hz effect rate). Brightness
changes larger than 10% of full scale limited to 3 per second (WCAG 2.3.1 used as a precedent
for room lighting; ASSUMED transfer from screens to lamps).

### 2c. Bands and level mapping

- IEC 61260-1:2014 (preview pages read 2026-10-01,
  https://cdn.standards.iteh.ai/samples/13383/3c4ae3e762b540cc8111744cb8f0ae8e/IEC-61260-1-2014.pdf),
  clause 5.2.1 formula (1): octave ratio G = 10^(3/10) (1.99526); 5.3: fr = 1000 Hz exactly;
  5.4.1 formula (2): odd b, fm = fr G^(x/b); 5.4.2 formula (3): even b,
  fm = fr G^((2x+1)/(2b)). Clause 3.11: a fractional-octave band's upper/lower edge ratio is
  G^(1/b), so the edges are fm G^(-1/(2b)) and fm G^(1/(2b)) (derived from 3.11 and the
  geometric-mean centre; clause 5.6 itself is not in the preview). 5.4 Note 1: narrow bands "can
  be combined to approximate the band level" of a wider band.
- Third-octave set for the audio range: b = 3, x = -17..13 gives 31 bands, fm = 19.95 Hz to
  19.95 kHz (nominal 20 Hz to 20 kHz).

Recommendation: compute the 31 third-octave band powers from an FFT (2048 points at 48 kHz =
23.4 Hz bins; bands below about 100 Hz span few bins, so use a 4096-point FFT for the band
analysis or accept coarse low bands; ASSUMED), then for an endpoint wanting n bands (n <= 31)
merge contiguous third-octave bands by summing power, splitting the 31 as evenly as possible
low to high (IEC Note 1). For n > 31 (wire allows 64) use b = 6 (even-b formula) over the same
range (60 bands). Default n = 16 (WLED's 16 GEQ channels as precedent).

Level mapping (ASSUMED, deterministic so fixtures can assert it): band level L in dBFS (power,
relative to a full-scale sine's power in that band = 0 dBFS); byte = clamp(round(255 (L + 60) / 60),
0, 255), i.e. a 60 dB display range. No automatic gain on the server (WLED applies AGC on the
device because its input level is unknown; the server knows full scale). Optional later: a
slow AGC over the short-term window.

### 2d. Colour

- E. Richan and J. Rouat, "A proposal and evaluation of new timbre visualisation methods for
  audio sample browsers", Personal and Ubiquitous Computing (2020), arXiv:2011.15096,
  https://arxiv.org/pdf/2011.15096 (read 2026-10-01), section 4: "The spectral centroid
  (measuring timbral brightness) is mapped to a gradient from blue to red and spectral flatness
  (measuring tonality) is then mapped to the color's saturation." It also reports prior work
  preferring "associating the spectral centroid with color lightness". Their finding: "shape
  significantly improves task performance, while color and texture have little effect", so the
  colour mapping is an aesthetic choice with precedent, not a perceptual law.

Recommendation (ASSUMED parameters on cited precedent): spectral centroid c (Hz) of the frame,
smoothed with the 400 ms momentary window (2e); position p = log2(c / 100) / log2(8000 / 100)
clamped to 0..1 (100 Hz to 8 kHz); hue = 240 deg (blue) at p = 0 to 0 deg (red) at p = 1
(Richan and Rouat's blue-to-red); saturation = 1 - spectral flatness (geometric mean over
arithmetic mean of the power spectrum, clamped 0..1; tonal = saturated); brightness byte = the
momentary level mapped as in 2c. Convert HSV to RGB with the standard formula. Test: a 100 Hz
sine gives blue, an 8 kHz sine gives red, white noise gives low saturation.

### 2e. Level ballistics

- EBU Tech 3341 V4 (Nov 2023), https://tech.ebu.ch/docs/tech/tech3341.pdf (read 2026-10-01):
  "The Momentary Loudness uses a sliding rectangular time window of length 0.4 s. The
  measurement is not gated." Short-term: "sliding rectangular time window of length 3 s";
  "Further slowdown of the attack or release ... shall not be employed in 'EBU Mode'"; it notes
  BS.1771-1 "prescribes a 1st order IIR filter with a time-constant of 0.4 s" for momentary.
  K-weighting per ITU-R BS.1770 (already a dsp-core fixture).
- EBU Tech 3205-E, "The EBU standard peak-programme meter", 2nd edition November 1979 (legacy,
  superseded by R128), https://tech.ebu.ch/docs/tech/tech3205.pdf (read 2026-10-01): "The
  integration time in normal mode shall be 10 ±2 ms"; return time: from +12 to -12 (24 dB) "in
  2.8 ±0.3 s in the normal mode" (about 8.6 dB/s), "approximately constant". This is the IEC
  60268-10 Type IIb (EBU) meter; IEC 60268-10 itself was not read.

Recommendation: `peak` = sample peak (max |x| over all channels) in the 20 ms frame, in dBFS,
held with an EBU PPM-like linear fall of 24 dB per 2.8 s (8.57 dB/s) and instant attack, mapped to
a byte by the 2c rule. Bands: per-band power averaged over the frame, then the same fall-back
ballistics per band (ASSUMED transfer of PPM return time to bands). Brightness for `color`:
K-weighted momentary loudness (400 ms rectangular window), mapped -60..0 LUFS to 0..255
(ASSUMED range).

## 3. Bass management (track `endpoint-dsp`; dsp-core block 8)

Sources (all read 2026-10-01):

- ATSC A/52:2018 "Digital Audio Compression (AC-3, E-AC-3)", 2018-01-25,
  https://www.atsc.org/wp-content/uploads/2021/04/A52-2018.pdf, definitions (section 3):
  "low frequency effects (lfe) channel - An optional single channel of limited (<120 Hz)
  bandwidth, which is intended to be reproduced at a level +10 dB with respect to the fbw
  channels." Downmix section: "An ideal downmix would have the lfe channel reproduce at an
  acoustic level of +10 dB with respect to the left and right channels."
- Dolby Laboratories, "5.1-Channel Production Guidelines", Issue 1 (S00/12957),
  https://www.associationdesmixeurs.fr/wp-content/uploads/2015/10/Dolby-5.1-Channel-Production-Guidelines.pdf
  (a Dolby document hosted by a third party), section 3: consumer decoders add the LFE and the
  bass-managed channels; "The five main channels are then high-pass filtered at either a fixed
  frequency of 80 Hz or a selectable frequency of 80, 100, or 120 Hz. The summation of the LFE
  and any other channels is low-pass filtered at the same frequency"; "for DVD and other consumer
  applications, a crossover frequency of 80 Hz is required"; "The LFE channel is calibrated such
  that each 1/3 octave band between 20 and 120 Hz is 10 dB higher than the equivalent 1/3 octave
  bands for any of the full-range speakers"; LFE "intended for reproduction at +10 dB SPL (with
  respect to the main channels within the same 3-120 Hz passband)". The guide gives no
  crossover slopes.
- THX: thx.com pages state THX requires an 80 Hz crossover in every THX Certified AVR and
  recommends setting speakers to "Small" (search summary of https://www.thx.com/blog/faq_category/buying-setup-guides/
  and https://www.thx.com/faq/ ; the FAQ returned HTTP 403, so LEAD). THX's slope specification
  is not public.
- B. Florian and C. Miller, "Bass Management Woes: Trouble on the Slopes", Secrets of Home
  Theater and High Fidelity, 2007-11-30,
  https://hometheaterhifi.com/editorial/bass-management-woes-trouble-on-the-slopes/ (secondary,
  editorial): THX satellites have a 2nd-order acoustic roll-off at 80 Hz, the processor adds a
  2nd-order (12 dB/oct) electrical high-pass, and "The subwoofer signal gets a 4th order roll-off
  at the same 80 Hz and Presto!: A perfect 4th order Linkwitz/Riley crossover"; with full-range
  (non-THX) mains the 2nd-order electrical HPF leaves excess output around 40-60 Hz, and the
  authors want "4th order for all others" offered as an option.
- Audyssey MultEQ-X User Guide 1.1 (above, page 25): "A default 2nd order high-pass filter is
  selected because it often works ideally with the AVR's built-in bass manager to create an ideal
  'Linkwitz-Riley' crossover network between the satellite and subwoofer." This is a vendor
  statement of the same rationale: the 12 dB/oct electrical HPF relies on a second acoustic
  2nd-order roll-off.

Answers:

- Crossover: 80 Hz default (Dolby guide "required" for consumer; THX's AVR requirement, LEAD).
  Selectable 80/100/120 Hz in Dolby's description; the catalog's 40..200 Hz range is wider and
  fine.
- LFE gain: +10 dB in-band (A/52 definition; Dolby guide). In a float chain with a 0 dBFS
  ceiling, +10 dB on a full-scale LFE clips: the look-ahead limiter catches it, and A/52 itself
  warns "Care should be taken to assure that loudspeakers are not overdriven by the full scale
  low frequency content" (downmix section).
- Slopes: the THX scheme is 12 dB/oct electrical HPF on mains plus a 24 dB/oct LPF on the sub,
  designed so that the main speaker's own 2nd-order roll-off makes the acoustic sum LR4
  (Florian and Miller; Audyssey). This is not a THX primary text (not public).
- LR4 on both: accepted when the mains are flat well below the crossover (Florian and Miller
  argue for it with full-range mains), and it is the only choice whose electrical sum is
  provably flat (allpass) without knowing the main drivers. chorus endpoints are generic, so
  the design's LR4 on both is the right default; a later option "HPF order 2" for sealed mains
  with a known 2nd-order roll-off at the crossover can be added (ASSUMED, not needed now).
  Fixture property (already in the design): LR4 low and high are each -6.02 dB at the crossover
  and sum flat.

## 4. Speech enhancement and night mode

### 4a. Dialogue / speech enhancement

- J. T. Geiger, P. Grosche, Y. Lacouture Parodi, "Dialogue enhancement of stereo sound", EUSIPCO
  2015, Nice, pp. 874-878, https://www.eurasip.org/Proceedings/Eusipco/Eusipco2015/papers/1570096395.pdf
  (read 2026-10-01). Centre extraction per frequency bin: Ce = alpha (L + R), Le = L - Ce,
  Re = R - Ce with alpha chosen so Le and Re are orthogonal (eq. 4); Wiener gain
  G = P(Ce) / (P(Ce) + P(Le - Re)) (eq. 7); voice activity from the centre's share of spectral
  flux (eq. 9); output C' = p Ce + q V G Ce (eq. 10) with p = q = 1 in the evaluation (speech
  components boosted up to +6 dB, others kept). Parameters: "sine windows with length of 64 ms and
  50% overlap", smoothing factor 0.8, VAD attack 0.7 and release 0.98, a 43-band ERB filter
  bank. The simple baseline it compares against: "simple center extraction and gain, in which
  the center is amplified (by 3.8 dB) with respect to the left and right channels"; the proposed
  method was judged significantly clearer than both the original and that baseline (13
  listeners).
- A. Craciun, C. Uhle, T. Backstrom, "An evaluation of stereo speech enhancement methods for
  different audio-visual scenarios", EUSIPCO 2015,
  https://www.eurasip.org/Proceedings/Eusipco/Eusipco2015/papers/1570095143.pdf (read 2026-10-01;
  context: a 9 dB background attenuation hidden reference).
- Band importance: speech intelligibility weighting (ANSI S3.5-1997 SII) is concentrated around
  1 to 2.5 kHz (LEAD: search summary of a ScienceDirect abstract on the Mandarin SII importance
  function, https://www.sciencedirect.com/science/article/abs/pii/S0167639316301935 , and S. E. Yoho
  et al., "Speech-material and talker effects in speech band importance", JASA 143(3),
  1417-1426, 2018, https://u.osu.edu/splab/files/2019/08/Speech-material-and-talker-effects-in-speech-band-importance.pdf ,
  which discusses the ANSI functions but whose peak values were not extracted). ANSI S3.5
  itself was not read.

Recommendation: the design's block 7 (peaking boost on FC, on mid for stereo, on the channel for
mono) is the "simple centre gain" family that Geiger et al. use as their baseline; it is cheap and
safe for an ESP32-S3. Values: centre 2000 Hz, Q 0.667 (a 2-octave band, 1 to 4 kHz at the
half-gain points: for N octaves Q = sqrt(2^N) / (2^N - 1)), gain +4 dB (rounded from Geiger et
al.'s 3.8 dB centre gain). The frequency and Q are ASSUMED from the band-importance LEADs; the
gain has the paper behind it. The Wiener/VAD method is the upgrade path (server-side or Linux
client), not phase B.

### 4b. Night mode compression

- Dolby Laboratories, "Dolby Metadata Guide", Issue 2, 2003,
  http://www.aesnashville.org/PDFs/Technical/Dolby/Dolby%20Metadata.pdf (Dolby document hosted by
  the AES Nashville section; read 2026-10-01), "Dynamic Range Control Profiles" (pages 9-10).
  Each profile is centred on the dialogue level (dialnorm); Line mode reproduces dialogue at
  "-31 dBFS Leq(A)"; RF mode raises the programme 11 dB (dialogue at -20 dBFS). Profiles (input
  levels in dB, as printed):

| Profile | Max boost | Boost range (ratio) | Null band | Early cut (ratio) | Cut (ratio) |
|---|---|---|---|---|---|
| Film Light | 6 dB below -53 | -53 to -41 (2:1) | 20 dB, -41 to -21 | -26 to -11 (2:1) | -11 to +4 (20:1) |
| Film Standard | 6 dB below -43 | -43 to -31 (2:1) | 5 dB, -31 to -26 | -26 to -16 (2:1) | -16 to +4 (20:1) |
| Music Light | 12 dB below -65 | -65 to -41 (2:1) | 20 dB, -41 to -21 | none | -21 to +9 (2:1) |
| Music Standard | 12 dB below -55 | -55 to -31 (2:1) | 5 dB, -31 to -26 | -26 to -16 (2:1) | -16 to +4 (20:1) |
| Speech | 15 dB below -50 | -50 to -31 (5:1) | 5 dB, -31 to -26 | -26 to -16 (2:1) | -16 to +4 (20:1) |

  (Film Light's early cut "-26 to -11" overlaps its null band "-41 to -21" as printed; treat as
  a typo in the source and do not use Film Light without checking.) The guide gives no attack or
  release times.
- ATSC A/52:2018 (above) carries the `dynrng` and `compr` words that decoders apply; the
  profiles' gain curves are an encoder-side choice (Dolby guide).

Recommendation: night mode = Dolby "Film Standard" static curve on PCM, using the -31 dBFS Line
mode dialogue reference as the null band centre because PCM carries no dialnorm (ASSUMED that
dialogue sits there): gain 0 in -31..-26 dBFS; 2:1 cut from -26 to -16 dBFS (output rises 5 dB
over that 10 dB); 20:1 above -16 dBFS; 2:1 boost from -31 down to -43 dBFS, capped at +6 dB below
-43 dBFS. The level detector is the design's Giannoulis feed-forward log-domain detector with soft
knees at each breakpoint (knee width ASSUMED 4 dB), stereo-linked. Time constants are not in the
Dolby guide: ASSUMED attack 10 ms, release 500 ms pending Giannoulis et al.'s examples. Since the
static curve boosts quiet passages, the look-ahead limiter at the room's ceiling still bounds the
output (K81, I10).

Static-curve fixture values (hard knees, levels as the detector's dBFS), computed here from the
Film Standard row:
- input -50 dBFS: below -43, so +6 dB (cap).
- input -40 dBFS: in -43..-31 at 2:1 toward -31: output = -31 + (-40 + 31)/2 = -35.5, gain +4.5 dB.
- input -28 dBFS: null band, gain 0.
- input -20 dBFS: early cut 2:1 from -26: output = -26 + 6/2 = -23, gain -3 dB.
- input 0 dBFS: early cut gives -21 at -16; then 20:1: output = -21 + 16/20 = -20.2, gain -20.2 dB.

## Recommendations for the tracks

Each value with its basis. ASSUMED marks a value with no direct citation; code comments and
docs must say ASSUMED for those (design envelope rule).

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

Track `visualizer`:

| Item | Value | Basis |
|---|---|---|
| Frame rate (`visualizer_frame`) | 50 Hz (20 ms) | WLED sync "one packet every 20 milliseconds"; Hue stream 50-60 Hz (LEAD) |
| `color` rate | <= 10 Hz, `transition_ms` = 100 | Nanoleaf <= 10 Hz (LEAD); Hue effect rate < 12.5 Hz (LEAD) |
| Flash limit | brightness swings > 10% of full scale at most 3 per second | WCAG 2.2 SC 2.3.1 (screen rule applied to lamps, ASSUMED transfer) |
| Onset function | spectral flux, L1, linear magnitude, half-wave rectified | Dixon 2006 section 2.1; Bello 2005 eq. 5 |
| STFT | 2048-point Hamming at 48 kHz, hop 960 (20 ms) | Dixon 2006 uses 2048 / 441 at 44.1 kHz (10 ms); 20 ms hop ASSUMED (inside the 50 ms tolerance) |
| Peak picking | w = 3, m = 3, delta = 0.1, a = 0.9 on a running-normalised flux (3 s EMA) | w, m: Dixon 2006; delta, a, 3 s ASSUMED |
| `beat` byte | 0 or min(255, round(64 z)) | ASSUMED |
| Bands | 31 third-octave bands, fm = 1000 * 10^(3x/30), x = -17..13; merged by power to n (default 16); b = 6 for n > 31 | IEC 61260-1:2014 5.2-5.4 and 5.4 Note 1; 16 from WLED |
| Level byte | clamp(round(255 (dBFS + 60) / 60), 0, 255) | ASSUMED (WLED uses 0..255 with AGC) |
| `peak` | sample peak per frame, instant attack, fall 24 dB in 2.8 s (8.57 dB/s) | EBU Tech 3205-E return time |
| Brightness | K-weighted momentary loudness, 400 ms rectangular, -60..0 LUFS to 0..255 | EBU Tech 3341 (0.4 s); range ASSUMED |
| Colour | hue 240 to 0 deg by log spectral centroid 100 Hz..8 kHz; saturation = 1 - spectral flatness | Richan and Rouat 2020 (centroid blue to red, flatness to saturation); ranges ASSUMED |

Track `endpoint-dsp` (and dsp-core defaults):

| Item | Value | Basis |
|---|---|---|
| Crossover | 80 Hz default | Dolby 5.1 Production Guidelines ("80 Hz is required" for consumer); THX AVR requirement (LEAD) |
| LFE gain | +10 dB in-band | ATSC A/52:2018 definitions; Dolby guidelines |
| Slopes | LR4 high on mains, LR4 low on sub (both 24 dB/oct) | flat electrical sum; Florian and Miller 2007 favour 4th order for full-range mains; THX's 12/24 scheme assumes a 2nd-order acoustic roll-off at 80 Hz (Florian and Miller; Audyssey MultEQ-X guide p. 25) |
| Speech | peaking 2000 Hz, Q 0.667 (1 to 4 kHz), +4 dB on FC / mid / mono | gain: Geiger et al. EUSIPCO 2015 centre gain 3.8 dB baseline; frequency and Q ASSUMED from SII band-importance LEADs |
| Night | Dolby Film Standard static curve around -31 dBFS: +6 dB max boost below -43, 2:1 boost -43..-31, null -31..-26, 2:1 cut -26..-16, 20:1 above -16 | Dolby Metadata Guide Issue 2, pp. 9-10; -31 dBFS from its Line mode; reference choice for PCM ASSUMED |
| Night timing | attack 10 ms, release 500 ms, knee 4 dB | ASSUMED (Dolby guide gives none) |

## What was read

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
21. Bello et al. 2005, onset tutorial: https://hajim.rochester.edu/ece/sites/zduan/teaching/ece472/reading/Bello_2005.pdf (read)
22. Dixon 2006, Onset Detection Revisited: https://www.dafx.de/paper-archive/2006/papers/p_133.pdf (read)
23. WLED MoonModules audio sync docs: https://mm.kno.wled.ge/soundreactive/sync/ (read; documentation page only, no WLED source opened)
24. WLED audio reactive docs: https://kno.wled.ge/advanced/audio-reactive/ (read; no numbers)
25. Hue Entertainment official page: https://developers.meethue.com/develop/hue-entertainment/hue-entertainment-api/ (login wall; not read)
26. huestream Go package docs (BSD-2-Clause) quoting Hue docs: https://pkg.go.dev/github.com/rschio/huestream (read; LEAD)
27. iotech blog on Hue Entertainment: https://iotech.blog/posts/philips-hue-entertainment-api/ (read; LEAD)
28. Nanoleaf Light Panels Open API: https://nanoleaf.atlassian.net/wiki/spaces/nlapid/pages/2789310530/Nanoleaf+Light+Panels+Open+API+Documentation (read partially; the 10 Hz figure is from a search summary, LEAD)
29. IEC 61260-1:2014 preview: https://cdn.standards.iteh.ai/samples/13383/3c4ae3e762b540cc8111744cb8f0ae8e/IEC-61260-1-2014.pdf (read, clauses 3, 5.2-5.5)
30. EBU Tech 3341 V4: https://tech.ebu.ch/docs/tech/tech3341.pdf (read)
31. EBU Tech 3205-E: https://tech.ebu.ch/docs/tech/tech3205.pdf (read)
32. WCAG 2.2 Understanding SC 2.3.1: https://www.w3.org/WAI/WCAG22/Understanding/three-flashes-or-below-threshold.html (read)
33. Richan and Rouat, arXiv:2011.15096: https://arxiv.org/pdf/2011.15096 (read)
34. ATSC A/52:2018: https://www.atsc.org/wp-content/uploads/2021/04/A52-2018.pdf (read, definitions and downmix)
35. Dolby 5.1-Channel Production Guidelines: https://www.associationdesmixeurs.fr/wp-content/uploads/2015/10/Dolby-5.1-Channel-Production-Guidelines.pdf (read)
36. Dolby Metadata Guide Issue 2: http://www.aesnashville.org/PDFs/Technical/Dolby/Dolby%20Metadata.pdf (read, pages 7-10)
37. Florian and Miller 2007, Secrets: https://hometheaterhifi.com/editorial/bass-management-woes-trouble-on-the-slopes/ (read; secondary)
38. THX FAQ and setup guides: https://www.thx.com/faq/ (HTTP 403), https://www.thx.com/blog/faq_category/buying-setup-guides/ (summary; LEAD)
39. Geiger, Grosche, Lacouture Parodi, EUSIPCO 2015: https://www.eurasip.org/Proceedings/Eusipco/Eusipco2015/papers/1570096395.pdf (read)
40. Craciun, Uhle, Backstrom, EUSIPCO 2015: https://www.eurasip.org/Proceedings/Eusipco/Eusipco2015/papers/1570095143.pdf (read, context)
41. Yoho et al. 2018, JASA: https://u.osu.edu/splab/files/2019/08/Speech-material-and-talker-effects-in-speech-band-importance.pdf (read, introduction only)
42. Mandarin SII importance abstract: https://www.sciencedirect.com/science/article/abs/pii/S0167639316301935 (summary; LEAD)
43. Existing repo research: `.claude/goals/2026-09-chorus-research/research-theater.md` section 5.2-5.3 (read; its bass management sources were Wikipedia and snippets, superseded here by A/52 and Dolby)

Not opened (clean-room, K33): no GPL or other reciprocal-licence source file was opened. WLED
(EUPL) and its MoonModules fork were consulted through their documentation pages only; a GitHub
link to MoonModules source seen in a docs page was not followed. The CRAN "SII" package was not
opened.
