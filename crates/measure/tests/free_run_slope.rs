//! The free-run drift fit, against committed series with known rates, and the
//! two refusals in front of it.
//!
//! Nothing here needs a device or a privilege. Each series states the rate it
//! was generated at and the jitter it carries in its own `.params` file, so
//! every tolerance below is checked against a number a reader can look up
//! rather than one this test invented.

use chorus_measure::config::MeasureConfig;
use chorus_measure::freerun::{self, OffsetSeries, SlopeSettings};
use chorus_measure::repository_root;

/// The accuracy asked of a noiseless series.
const NOISELESS_TOLERANCE_PPM: f64 = 0.5;

/// The accuracy asked of the series carrying the jitter its fixture states.
const JITTERED_TOLERANCE_PPM: f64 = 5.0;

fn settings() -> SlopeSettings {
    let config = MeasureConfig::read(&repository_root()).expect("config/measure.conf is committed");
    SlopeSettings::from_config(&config)
}

fn series(fixture: &str) -> OffsetSeries {
    let path = repository_root().join("fixtures/measure").join(fixture);
    freerun::read_series(&path)
        .unwrap_or_else(|e| panic!("{} is a committed series: {}", fixture, e))
}

/// The rate a series' own parameter file declares it was generated at.
fn declared_rate_ppm(series: &OffsetSeries) -> f64 {
    series
        .declared_rate_ppm
        .expect("every committed series states the rate it was generated at")
}

/// AC-7, first half. A noiseless series at a known constant relative rate, and
/// a slope within 0.5 ppm of it.
#[test]
fn a_noiseless_series_is_fitted_within_half_a_part_per_million() {
    let series = series("10-free-run-noiseless.offsets");
    let truth = declared_rate_ppm(&series);
    let fit = freerun::fit(&series, &settings()).expect("the noiseless series fits");
    assert!(
        (fit.ppm - truth).abs() < NOISELESS_TOLERANCE_PPM,
        "fitted {:+.4} ppm against a declared {:+.4} ppm, which is {:.4} ppm out of a {:.1} \
         ppm budget",
        fit.ppm,
        truth,
        (fit.ppm - truth).abs(),
        NOISELESS_TOLERANCE_PPM
    );
    // With no noise at all the fit has an exact answer available, and an
    // estimator that cannot be exact where exactness is on offer has a leak in
    // it. This is the same argument fixtures/sync/05-noiseless-control.cfg
    // makes for the simulator.
    assert!(
        fit.residual_rms_ns < 1.0,
        "a noiseless series should leave no residual; it left {:.3} ns RMS",
        fit.residual_rms_ns
    );
}

/// AC-7, second half. The same drift through the jitter its fixture states,
/// and a slope within 5 ppm of it.
#[test]
fn a_jittered_series_is_fitted_within_five_parts_per_million() {
    let series = series("11-free-run-jittered.offsets");
    let truth = declared_rate_ppm(&series);
    let declared_jitter_us = series
        .declared_jitter_us
        .expect("the jittered series states its jitter level");
    assert!(
        declared_jitter_us > 0.0,
        "the jittered fixture states {} us of jitter, which is not jitter",
        declared_jitter_us
    );
    let fit = freerun::fit(&series, &settings()).expect("the jittered series fits");
    assert!(
        (fit.ppm - truth).abs() < JITTERED_TOLERANCE_PPM,
        "fitted {:+.4} ppm against a declared {:+.4} ppm, which is {:.4} ppm out of a {:.1} \
         ppm budget",
        fit.ppm,
        truth,
        (fit.ppm - truth).abs(),
        JITTERED_TOLERANCE_PPM
    );
    // The jitter it states is the jitter it carries. Without this the test
    // above would pass just as well against a fixture that quietly had none,
    // and the 5 ppm tolerance would be measuring nothing.
    let measured_jitter_us = fit.residual_rms_ns / 1000.0;
    assert!(
        (measured_jitter_us / declared_jitter_us - 1.0).abs() < 0.15,
        "the fixture states {:.1} us of jitter and the residuals measure {:.1} us",
        declared_jitter_us,
        measured_jitter_us
    );
}

/// The fit publishes its own error bar, and the error bar is honest: the
/// noiseless series bounds its slope far more tightly than the jittered one.
#[test]
fn the_fit_publishes_a_confidence_half_width_that_tracks_the_noise() {
    let clean = freerun::fit(&series("10-free-run-noiseless.offsets"), &settings()).unwrap();
    let noisy = freerun::fit(&series("11-free-run-jittered.offsets"), &settings()).unwrap();
    assert!(
        clean.half_width_ppm < noisy.half_width_ppm,
        "a noiseless series should bound its slope more tightly than a jittered one: \
         {:.6} against {:.6}",
        clean.half_width_ppm,
        noisy.half_width_ppm
    );
    assert!(
        noisy.half_width_ppm > 0.0,
        "a jittered series has a real uncertainty and should report one"
    );
    // And the truth is inside the bar it published, which is what a confidence
    // interval is for.
    let truth = declared_rate_ppm(&series("11-free-run-jittered.offsets"));
    assert!(
        (noisy.ppm - truth).abs() <= noisy.half_width_ppm * 3.0,
        "the declared rate {:+.4} ppm is outside three times the published half-width of \
         {:.6} ppm around {:+.4} ppm",
        truth,
        noisy.half_width_ppm,
        noisy.ppm
    );
}

/// AC-17, first condition. A series too short to fit publishes nothing and says
/// which of the two conditions it hit.
#[test]
fn a_series_too_short_publishes_no_figure_and_names_that_condition() {
    let series = series("12-free-run-too-short.offsets");
    let settings = settings();
    let err = freerun::fit(&series, &settings)
        .expect_err("a series this short must not publish a ppm figure");
    assert_eq!(err.condition(), "series-too-short");
    let said = err.to_string();
    assert!(said.contains("too short"), "{}", said);
    assert!(said.contains("No ppm figure is published"), "{}", said);
    // The refusal names both halves of what it required, so an operator knows
    // whether to run longer or observe more often.
    assert!(
        said.contains(&settings.min_points.to_string()),
        "the refusal should name the {} observations it required: {}",
        settings.min_points,
        said
    );
}

/// AC-17, second condition. A series long enough to fit and too noisy to bound
/// publishes nothing, says which condition it hit, and says what it withheld.
#[test]
fn a_series_too_noisy_publishes_no_figure_and_names_that_condition() {
    let series = series("13-free-run-too-noisy.offsets");
    let settings = settings();
    // Long enough that the length check is not what fires. Without this the
    // test could pass for the wrong reason and the two conditions would be one.
    assert!(series.observations.len() >= settings.min_points);
    assert!(series.span_s() >= settings.min_span_s);

    let err = freerun::fit(&series, &settings)
        .expect_err("a series this noisy must not publish a ppm figure");
    assert_eq!(err.condition(), "series-too-noisy");
    let said = err.to_string();
    assert!(said.contains("too noisy"), "{}", said);
    assert!(said.contains("withheld rather than published"), "{}", said);
}

/// The two conditions are genuinely two: neither fixture trips the other's
/// check.
#[test]
fn the_two_refusals_are_told_apart_rather_than_collapsed() {
    let short = freerun::fit(&series("12-free-run-too-short.offsets"), &settings()).unwrap_err();
    let noisy = freerun::fit(&series("13-free-run-too-noisy.offsets"), &settings()).unwrap_err();
    assert_ne!(short.condition(), noisy.condition());
}

/// A slope always exists. That is exactly why the refusals are the interesting
/// part: with the confidence bound removed, the too-noisy series would publish
/// a number.
#[test]
fn the_too_noisy_series_would_publish_a_number_if_the_bound_were_removed() {
    let series = series("13-free-run-too-noisy.offsets");
    let mut permissive = settings();
    permissive.max_half_width_ppm = f64::INFINITY;
    let fit = freerun::fit(&series, &permissive).expect("an unbounded fit always succeeds");
    let truth = declared_rate_ppm(&series);
    assert!(
        (fit.ppm - truth).abs() > 1.0,
        "the point of this fixture is that the unbounded answer is wrong by more than the \
         bound would have allowed; it came out {:+.4} ppm against {:+.4} ppm",
        fit.ppm,
        truth
    );
    assert!(
        fit.half_width_ppm > settings().max_half_width_ppm,
        "and that the half-width says so: {:.4} ppm against a permitted {:.4} ppm",
        fit.half_width_ppm,
        settings().max_half_width_ppm
    );
}
