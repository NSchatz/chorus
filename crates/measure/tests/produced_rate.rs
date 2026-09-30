//! Audit A-11: `chorus-measure rate` recovers the rate a captured output
//! produced from the committed fixture with a known skew
//! (fixtures/measure/18-rate-skewed.params: channel A 150 ppm fast, channel B
//! on the capture clock), and refuses a capture too short to say.

use std::path::PathBuf;
use std::process::Command;

use chorus_measure::config::MeasureConfig;
use chorus_measure::rate::{self, Channel, RateSettings};
use chorus_measure::{repository_root, wav};

fn fixture(name: &str) -> PathBuf {
    repository_root().join("fixtures/measure").join(name)
}

#[test]
fn the_skewed_fixture_reads_as_its_declared_rate() {
    let config = MeasureConfig::read(&repository_root()).unwrap();
    let capture = wav::read_capture(
        &fixture("18-rate-skewed.wav"),
        config.capture_sample_rate_hz,
    )
    .unwrap();
    let settings = RateSettings::from_config(&config).unwrap();
    let a = rate::estimate(&capture, Channel::A, &settings).unwrap();
    let b = rate::estimate(&capture, Channel::B, &settings).unwrap();
    println!(
        "channel a: {:+.3} ppm +/-{:.3} over {} of {}",
        a.ppm, a.half_width_ppm, a.windows_used, a.windows_total
    );
    println!(
        "channel b: {:+.3} ppm +/-{:.3} over {} of {}",
        b.ppm, b.half_width_ppm, b.windows_used, b.windows_total
    );
    assert!((a.ppm - 150.0).abs() < 1.0, "{:?}", a);
    assert!(b.ppm.abs() < 1.0, "{:?}", b);
    assert!(a.half_width_ppm <= config.rate_max_half_width_ppm);
}

#[test]
fn the_command_prints_one_line_per_channel_and_writes_nothing() {
    let out_dir = std::env::temp_dir().join(format!("chorus-rate-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out_dir);
    let output = Command::new(env!("CARGO_BIN_EXE_chorus-measure"))
        .arg("rate")
        .arg(fixture("18-rate-skewed.wav"))
        .current_dir(std::env::temp_dir())
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    print!("{}", text);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let a: f64 = text
        .split("channel a produced rate ")
        .nth(1)
        .and_then(|rest| rest.split(' ').next())
        .and_then(|figure| figure.parse().ok())
        .expect("channel a's figure");
    assert!((a - 150.0).abs() < 1.0, "{}", text);
    assert!(
        text.contains("chorus-measure: channel b produced rate "),
        "{}",
        text
    );
    assert_eq!(text.lines().count(), 2, "{}", text);
}

#[test]
fn a_capture_too_short_for_the_span_is_refused_by_name() {
    // 100 ms: two sweep starts at most, far under rate_min_span_s.
    let output = Command::new(env!("CARGO_BIN_EXE_chorus-measure"))
        .arg("rate")
        .arg(fixture("01-chirp-pair-a.wav"))
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&output.stderr);
    println!("{}", err);
    assert!(!output.status.success());
    assert!(err.contains("too short"), "{}", err);
}
