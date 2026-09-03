//! The three assertions about what the lag estimator resolves, all graded
//! against committed fixtures with ground truth in their parameter files.
//!
//! Accuracy and quantisation are two different questions and each gets its own
//! test here. A coarse estimator that happened to land close would satisfy the
//! first alone; an estimator whose output was fine-grained and wrong would
//! satisfy the second alone. Neither is the other.
//!
//! Nothing here needs a device or a privilege. The captures are files in the
//! tree and the numbers they hold are stated in the `.params` beside them, so a
//! reader who did not take a capture can re-run every assertion below.

use chorus_measure::config::MeasureConfig;
use chorus_measure::lag::{self, LagSettings, LagSummary};
use chorus_measure::{repository_root, wav};

/// The budget the roadmap phase names: "a resolution of 10 us or better".
const BUDGET_US: f64 = 10.0;

/// The delay `01-chirp-pair-a.params` declares, in microseconds.
const DELAY_A_US: f64 = 254.0;

/// The delay `02-chirp-pair-b.params` declares. Exactly 10 us more than A, and
/// under one sample at 96 kHz.
const DELAY_B_US: f64 = 264.0;

fn analyse(fixture: &str) -> LagSummary {
    let root = repository_root();
    let config = MeasureConfig::read(&root).expect("config/measure.conf is committed");
    let path = root.join("fixtures/measure").join(fixture);
    let capture = wav::read_capture(&path, config.capture_sample_rate_hz)
        .unwrap_or_else(|e| panic!("{} is a committed capture: {}", fixture, e));
    let settings = LagSettings::from_config(&config, capture.sample_rate_hz);
    lag::estimate(&capture, &settings)
        .unwrap_or_else(|e| panic!("{} should resolve: {}", fixture, e))
}

/// AC-4. A capture whose second channel is the first delayed by a known amount
/// that is NOT a whole number of capture samples, and every one of the three
/// reported figures inside 10 us of it.
///
/// 254.0 us at 96 kHz is 24.384 samples. One sample is 10.4 us, so an estimator
/// that reported whole samples would be quantised more coarsely than the whole
/// budget before it made a single error.
#[test]
fn every_reported_figure_is_within_ten_microseconds_of_the_known_delay() {
    let summary = analyse("01-chirp-pair-a.wav");
    for (name, value) in [
        ("median", summary.median_us),
        ("p95", summary.p95_abs_us),
        ("maximum", summary.max_abs_us),
    ] {
        assert!(
            (value - DELAY_A_US).abs() < BUDGET_US,
            "the {} lag is {:.3} us and the fixture declares {:.1} us, which is {:.3} us out \
             of a {:.0} us budget",
            name,
            value,
            DELAY_A_US,
            (value - DELAY_A_US).abs(),
            BUDGET_US
        );
    }
    assert!(
        summary.windows_used >= 8,
        "a distribution needs windows; {} were used",
        summary.windows_used
    );
    assert_eq!(
        summary.windows_used, summary.windows_total,
        "every window of a clean capture should resolve"
    );
}

/// The same assertion said the other way round: the delay is deliberately not a
/// whole number of samples, so an estimator that only reported whole samples
/// could not pass the test above.
///
/// Committed as its own assertion because it is the property the fixture exists
/// for, and a reader should not have to work it out from a decimal.
#[test]
fn the_fixture_delay_is_deliberately_not_a_whole_number_of_capture_samples() {
    let root = repository_root();
    let config = MeasureConfig::read(&root).unwrap();
    let in_samples = DELAY_A_US * f64::from(config.capture_sample_rate_hz) / 1_000_000.0;
    let distance_to_a_whole_sample = (in_samples - in_samples.round()).abs();
    assert!(
        distance_to_a_whole_sample > 0.2,
        "{} us is {} samples, only {} from a whole one",
        DELAY_A_US,
        in_samples,
        distance_to_a_whole_sample
    );
    let sample_us = 1_000_000.0 / f64::from(config.capture_sample_rate_hz);
    assert!(
        sample_us > BUDGET_US,
        "one capture sample is {:.2} us, which would already be inside the {:.0} us budget; \
         this fixture set stops proving anything the day the capture rate rises that far",
        sample_us,
        BUDGET_US
    );
}

/// AC-5. Two captures whose true delays differ by exactly 10 us, and medians
/// that differ.
///
/// Ten microseconds is 0.96 of a sample at 96 kHz, so a whole-sample estimator
/// reports the same median for both. The assertion the criterion asks for is
/// that they differ; the tighter one below says the difference is the right
/// size, which is what makes "not quantised more coarsely than 10 us" mean
/// something rather than "the two numbers are not bit-identical".
#[test]
fn ten_microseconds_of_true_difference_moves_the_reported_median() {
    let a = analyse("01-chirp-pair-a.wav");
    let b = analyse("02-chirp-pair-b.wav");
    assert!(
        a.median_us != b.median_us,
        "both captures reported {:.6} us, so the figure is quantised more coarsely than the \
         10 us that separates them",
        a.median_us
    );
    let difference = b.median_us - a.median_us;
    let truth = DELAY_B_US - DELAY_A_US;
    assert!(
        (difference - truth).abs() < 1.0,
        "the medians differ by {:.3} us and the fixtures differ by {:.1} us",
        difference,
        truth
    );
}

/// AC-6. The sign convention, and a capture with the channels exchanged
/// reporting the same magnitude with the opposite sign.
#[test]
fn exchanging_the_channels_flips_the_sign_and_keeps_the_magnitude() {
    let plain = analyse("01-chirp-pair-a.wav");
    let exchanged = analyse("03-chirp-pair-exchanged.wav");

    assert!(
        plain.median_us > 0.0,
        "the fixture delays channel B, so channel A leads and the median is positive; it is \
         {:.3} us",
        plain.median_us
    );
    assert!(
        exchanged.median_us < 0.0,
        "with the channels exchanged the median should be negative; it is {:.3} us",
        exchanged.median_us
    );
    assert!(
        (plain.median_us.abs() - exchanged.median_us.abs()).abs() < 1.0,
        "the magnitudes should match: {:.3} us against {:.3} us",
        plain.median_us,
        exchanged.median_us
    );
}

/// The convention is not only obeyed, it is stated, and the statement is
/// carried into the report a reader gets.
#[test]
fn the_report_says_which_output_leads_under_a_convention_it_declares() {
    let plain = analyse("01-chirp-pair-a.wav");
    let exchanged = analyse("03-chirp-pair-exchanged.wav");
    assert_eq!(plain.who_leads(), "channel A leads channel B");
    assert_eq!(exchanged.who_leads(), "channel B leads channel A");
    assert!(
        lag::SIGN_CONVENTION.contains("positive lag")
            && lag::SIGN_CONVENTION.contains("channel A")
            && lag::SIGN_CONVENTION.contains("leads"),
        "the declared convention has to say which way round it is: '{}'",
        lag::SIGN_CONVENTION
    );
}

/// The estimator is demonstrably finer than one capture sample, measured rather
/// than asserted: the two fixtures' medians are separated by well under a
/// sample and the estimator resolves them.
#[test]
fn the_reported_figure_is_resolved_more_finely_than_one_capture_sample() {
    let root = repository_root();
    let config = MeasureConfig::read(&root).unwrap();
    let sample_us = 1_000_000.0 / f64::from(config.capture_sample_rate_hz);
    let a = analyse("01-chirp-pair-a.wav");
    let b = analyse("02-chirp-pair-b.wav");
    let separation = (b.median_us - a.median_us).abs();
    assert!(
        separation < sample_us,
        "the fixtures are {:.3} us apart and one sample is {:.3} us; this test is only \
         meaningful while the separation is the smaller",
        separation,
        sample_us
    );
    let error = ((b.median_us - a.median_us) - (DELAY_B_US - DELAY_A_US)).abs();
    assert!(
        error < sample_us / 2.0,
        "resolving a {:.3} us separation to {:.3} us is not finer than the {:.3} us sample it \
         sits inside",
        separation,
        error,
        sample_us
    );
}

/// The distribution is a distribution: p95 and the maximum are taken from the
/// window population and are not the median wearing two other names.
#[test]
fn the_three_figures_are_taken_from_the_window_population() {
    let summary = analyse("01-chirp-pair-a.wav");
    assert!(summary.windows.len() == summary.windows_used);
    assert!(
        summary.max_abs_us >= summary.p95_abs_us,
        "{} < {}",
        summary.max_abs_us,
        summary.p95_abs_us
    );
    let largest = summary
        .windows
        .iter()
        .map(|w| w.lag_us.abs())
        .fold(f64::NEG_INFINITY, f64::max);
    assert_eq!(summary.max_abs_us, largest);
    assert!(
        summary.min_us < summary.max_us,
        "a real capture's windows do not all agree exactly: {} to {}",
        summary.min_us,
        summary.max_us
    );
    assert!(
        summary.min_coefficient > 0.9,
        "a clean fixture should correlate strongly; the weakest window was {}",
        summary.min_coefficient
    );
}
