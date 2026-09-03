# Free-run drift: noiseless-fixture

Date: 2026-09-03
Build measured: `60b639b98586ed9e1d0bf0c195870a2bf95a1172`
Tree at that commit: clean
Reproduce with: `cargo run -p chorus-measure --bin chorus-measure -- free-run fixtures/measure/10-free-run-noiseless.offsets --label noiseless-fixture`

## Figures

| figure | value |
|---|---|
| series | `fixtures/measure/10-free-run-noiseless.offsets` |
| observations | 601 |
| span | 600.0 s |
| relative rate | +37.5000 ppm |
| 95% confidence half-width | +/-0.0000 ppm |
| residual jitter | 0.0 us RMS |
| offset at the first observation | +0 ns |

## What the run declared before it looked at the series

| setting | value |
|---|---|
| observations required | 30 |
| span required | 60 s |
| widest publishable half-width | +/-1.000 ppm |
| rate the fixture states it was generated at | +37.5000 ppm |
| jitter the fixture states it carries | 0.0 us |

## Recorded as the baseline

This slope has been recorded in `docs/measurements/free-run-baseline.conf` as the free-run baseline later runs cite, with `source = fixture`.

`source = fixture` is the honest label here. This series was generated from committed parameters at a known rate; the fit recovering that rate bounds the ESTIMATOR and says nothing about any real crystal. A hardware baseline needs two clients running with correction disabled, which is operator graded in `docs/verification-record.md`.

## Method

Ordinary least squares of relative offset against elapsed time, with the slope's own 95% confidence half-width from the residuals. A run publishes no ppm figure when that half-width is wider than the run declared, or when the series is shorter than the run declared. `docs/decisions/0013-the-measurement-rig.md` records both thresholds and why they are where they are.
