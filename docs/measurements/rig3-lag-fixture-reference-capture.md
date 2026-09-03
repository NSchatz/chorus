# Inter-device lag: fixture-reference-capture

Date: 2026-09-03
Build measured: `60b639b98586ed9e1d0bf0c195870a2bf95a1172`
Tree at that commit: carried 2 uncommitted path(s) when this run was taken: ?? docs/measurements/free-run-baseline.conf, ?? docs/measurements/rig3-free-run-noiseless-fixture.md
Reproduce with: `cargo run -p chorus-measure --bin chorus-measure -- lag fixtures/measure/01-chirp-pair-a.wav --label fixture-reference-capture`

## Sign convention

a positive lag means the endpoint on channel A (the first captured output) leads the endpoint on channel B (the second) by that many microseconds.

## Figures

| figure | value |
|---|---|
| capture | `fixtures/measure/01-chirp-pair-a.wav` |
| capture sample rate | 96000 Hz |
| capture length | 9600 frames, 100.0 ms |
| analysis windows | 32 offered, 32 used |
| median lag | +253.981 us |
| p95 lag (absolute) | 254.026 us |
| maximum lag (absolute) | 254.039 us |
| spread across windows | +253.902 us to +254.039 us |
| weakest correlation used | 0.9877 |
| which output leads | channel A leads channel B |

## The free-run baseline this run cites

Baseline: +37.5000 ppm (+/-0.0000 ppm), measured from fixture, established by `docs/measurements/rig3-free-run-noiseless-fixture.md` at commit `60b639b98586ed9e1d0bf0c195870a2bf95a1172`.

That baseline was fitted from a committed fixture, not from two clients running with correction disabled. It bounds this rig's estimator against a known rate. It is NOT a statement about any real crystal, and no claim about hardware drift rests on it.

## What the run declared before it looked at the capture

| setting | value |
|---|---|
| analysis window | 1920 frames, 20.0 ms |
| window hop | 240 frames, 2.50 ms |
| lag search range | +/-96 frames, +/-1000 us |
| confidence floor | 0.600 normalised correlation |
| silence floor | -80.0 dBFS |
| chirp band | 1000 to 8000 Hz, at least 0.50 of channel energy |
| windows required | 4 |

## Method

Sliding-window cross-correlation of the two captured channels, with the peak located on a continuum by a windowed-sinc reconstruction of the correlation and a parabolic vertex on the reconstruction. `docs/decisions/0013-the-measurement-rig.md` records why, and the accuracy that has been demonstrated against fixtures with known ground truth.

Analysis time on a monotonic clock: 371.0 ms. That figure is a diagnostic and nothing derived from the capture depends on it.

## What this report does not establish

A capture taken from a file establishes what the estimator does with that file. Whether the two ENDPOINTS were that far apart is a question about a capture taken through real line outputs into one interface, and only a report over such a capture answers it. `docs/verification-record.md` records which criteria of this phase are operator graded for exactly that reason.
