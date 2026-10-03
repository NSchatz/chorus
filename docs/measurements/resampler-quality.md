# The decoders' resampler: passband, images, aliases and sweep error

Date: 2026-10-03
Source: synthetic
Build measured: `123ffa330bb7cb134e431b580f8e344f66a2a88e`
Timing evidence: none. Nothing here is a timing claim: these are figures of arithmetic on
generated signals, the same on any machine that computes IEEE floating point (the signals come
from `sin`, whose last bit may differ between C libraries, which moves no figure at the
precision printed here; `ASSUMED`, not shown on a second machine).

What is measured: `chorus_decode::Resampler` (`crates/decode/src/resample.rs`), the windowed-sinc
polyphase resampler that takes a decode to a stream's rate (goal 16; ADR 0122). 96 zero crossings
of the sinc on each side at the lower of the two rates, a Kaiser window designed for 110 dB, the
stopband edge at the lower rate's Nyquist frequency, one row of taps per output phase (160 rows
for 44.1 to 48 kHz), `f32` taps, `f64` accumulation.

## Command

```
cargo test -p chorus-decode --test resampler_quality -- --nocapture
```

run on the commit above (rustc 1.98.1, the dev profile, x86_64 Linux), under the heavy locks. The
test file is the method: `crates/decode/tests/resampler_quality.rs`.

## Method

Every signal is mono, 0.25 s at the input rate, amplitude 0.5, fed in chunks of 1153 frames and
flushed. The first and last `2 * delay_frames() + 16` output frames are left out, because there
the filter's window reaches past the signal's ends.

- **Passband ripple**: 12 tones logarithmically spaced from 20 Hz to 0.45 of the lower rate. For
  each, the amplitude of the output's component at that frequency is fitted by least squares (a
  sine and a cosine); the figure is the largest `|20 log10(out / in)|`.
- **Spurious**: for the same 12 tones, the rms of everything left after the fitted tone is
  removed, relative to the tone's rms: images, aliases and arithmetic noise together. The worst
  tone is reported.
- **Stopband** (rate going down): 8 tones between the output's Nyquist frequency and the
  input's, which must not come out at all (whatever comes out is an alias); the output's rms
  relative to the input tone's rms, worst tone.
- **Image** (rate going up): 8 tones between `from - to/2` and `from/2`, whose first image
  `from - f` lies below the output's Nyquist frequency; the amplitude fitted at the image
  frequency (jointly with the tone, so the tone does not leak into the figure), relative to the
  input amplitude, worst tone.
- **Sweep error**: a logarithmic sweep from 20 Hz to 0.45 of the lower rate, resampled, against
  the same sweep computed directly at the output rate. The resampler's output is aligned in time
  with its input, so the two are compared sample by sample; the figure is the error's rms
  relative to the sweep's rms.
- **Gain at 20 kHz**: one tone at 20 kHz, where both rates can carry it.

## Result

| from | to | passband ripple (dB) | to (Hz) | spurious (dB) | stopband (dB) | image (dB) | sweep error (dB) | gain at 20 kHz (dB) |
|---|---|---|---|---|---|---|---|---|
| 44100 | 48000 | 0.00002 | 19845 | -126.0 | n/a | -110.500 | -131.9 | -0.000 |
| 96000 | 48000 | 0.00002 | 21600 | -150.0 | -114.675 | n/a | -129.1 | -0.000 |
| 48000 | 44100 | 0.00001 | 19845 | -127.6 | -111.671 | n/a | -130.3 | -0.000 |
| 8000 | 48000 | 0.00002 | 3600 | -127.0 | n/a | -119.438 | -129.1 | n/a |
| 44100 | 47999 (interpolated rows) | 0.00001 | 19845 | -124.7 | n/a | -110.505 | -131.7 | not printed |

The raw lines, as the test printed them:

```
resampler 8000 -> 48000: passband ripple 0.00002 dB (to 3600 Hz); spurious -127.0 dB; stopband n/a dB; image -119.438 dB; sweep error -129.1 dB; gain at 20 kHz n/a dB
resampler 48000 -> 44100: passband ripple 0.00001 dB (to 19845 Hz); spurious -127.6 dB; stopband -111.671 dB; image n/a dB; sweep error -130.3 dB; gain at 20 kHz -0.000 dB
resampler 44100 -> 48000: passband ripple 0.00002 dB (to 19845 Hz); spurious -126.0 dB; stopband n/a dB; image -110.500 dB; sweep error -131.9 dB; gain at 20 kHz -0.000 dB
resampler 96000 -> 48000: passband ripple 0.00002 dB (to 21600 Hz); spurious -150.0 dB; stopband -114.675 dB; image n/a dB; sweep error -129.1 dB; gain at 20 kHz -0.000 dB
resampler 44100 -> 47999 (interpolated rows): passband ripple 0.00001 dB; spurious -124.7 dB; image -110.505 dB; sweep error -131.7 dB
```

## The bound the tests hold

The same test file fails outside these bounds, on every `cargo test` (so in `make tier-fast`):
passband ripple at most 0.01 dB up to 0.45 of the lower rate; spurious at most -100 dB; stopband
and image at most -100 dB; sweep error at most -100 dB (-90 dB for spurious and sweep error on the
interpolated ratio). The measured figures are 10 dB or more inside every bound.

A control in the same file shows the measurement can fail: resampling by taking the nearest
input frame measures above -40 dB of spurious content.

## Limits

- Four ratios and one interpolated ratio, not every pair between 8 and 192 kHz. The filter is
  one design scaled by the ratio, so other pairs are expected to behave alike; that is an
  expectation, not a measurement.
- Tones are sampled at 8 to 12 frequencies per band, not swept densely: a narrow defect between
  them would be missed by the tone figures (the sweep covers the passband continuously, but only
  as one rms figure).
- The stopband and image figures sit at the Kaiser window's design attenuation (110 dB); the
  spurious figure for passband tones is lower because their images and aliases fall deeper in
  the stopband.
- Mono only. Channels are filtered independently by the same taps
  (`resample::tests::channels_stay_separate`).
- Not a listening test, and no claim about a device.
