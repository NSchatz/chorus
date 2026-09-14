# Wireless jitter: wireless-ps-none, power save none

Date: 2026-09-12
Build measured: `9233f2fa9cb5494babd0561017c984ff74149d8c`
Tree at that commit: carried 22 uncommitted path(s) when this run was taken: M audio-path.conf, M crates/client-linux/src/config.rs, M crates/client-linux/src/main.rs, M crates/control/src/catalog.rs, M crates/control/src/lib.rs, M crates/measure/src/bin/chorus-measure.rs, M crates/measure/src/lib.rs, M crates/server/src/config.rs, M crates/server/src/main.rs, M docs/measurements/free-run-baseline.conf, M docs/measurements/rig3-free-run-noiseless-fixture.md, M docs/measurements/rig3-lag-fixture-reference-capture.md, and 10 more
Power save mode in force: **none** (`WIFI_PS_NONE`), which is not the platform default
Transport: **wireless**
Reproduce with: `cargo run -p chorus-measure --bin chorus-measure -- jitter fixtures/measure/14-wireless-jitter-ps-none.offsets --label wireless-ps-none --mode none --transport wireless`

## Figures

| figure | value |
|---|---|
| series | `fixtures/measure/14-wireless-jitter-ps-none.offsets` |
| power save mode in force | none (WIFI_PS_NONE) |
| transport | wireless |
| observations | 1200 over 599.5 s |
| centre of the series | +4.9 us |
| median deviation | 523.2 us |
| p95 deviation | 1546.0 us |
| maximum deviation | 2457.8 us |
| peak to peak | 4884.0 us |
| RMS deviation | 798.8 us |

## What the series says about itself

The series states it carries 800.0 us of jitter.

It states a relative rate of +0.0000 ppm, which the figures above do not use: every deviation is taken from the series' own median, so a steady drift moves the centre and not the spread.

## Method

Every figure is the deviation of an observation from the series' OWN median, in microseconds, with the percentile taken by nearest rank so that every figure printed is an observation that was actually taken. A deviation from zero would be a claim that zero is where the offsets should be, which is a statement about a servo and not about jitter.

Nothing here corrects, disciplines or tunes anything. `config/sync.conf` holds the servo constants SYNC-4 fixed and this phase moved none of them: BRIEF.md section 9 is explicit that the answer to Wi-Fi jitter is a bigger buffer or a wired zone, never servo aggression.

## What this report does not establish

**THIS SERIES IS A COMMITTED FIXTURE AND NOT A MEASUREMENT.** It was generated from committed parameters at a stated jitter level, so what the figures above establish is what this analysis does with that file, and nothing whatever about any radio, any access point or any room. No claim about Wi-Fi rests on it.

A measured series needs an ESP32-S3 endpoint on a real wireless link, a second endpoint in another room and the RIG-3 capture rig, which is what `tools/wireless-characterization-run.sh` refuses by name without. The criteria that would be answered by such a run are operator graded, and `docs/verification-record.md` records them as NOT passed.
