//! Audit A-4: two clients' free-run series, paired on the server timeline, and
//! the free-run fit over the pair, against the committed client fixtures.
//!
//! `fixtures/measure/16-free-run-client-a` walks at +20.0 ppm against the
//! server and `17-free-run-client-b` at -17.5 ppm, ticking a quarter second
//! later, so the pair has to be interpolated and its slope is +37.5 ppm. Both
//! are generated from their `.params`; neither is a measurement.

use chorus_measure::config::MeasureConfig;
use chorus_measure::freerun::{self, SlopeSettings};
use chorus_measure::pair;
use chorus_measure::report::{self, BuildIdentity};
use chorus_measure::repository_root;

fn client(name: &str) -> pair::ClientSeries {
    let path = repository_root().join("fixtures/measure").join(name);
    pair::read_client_series(&path).unwrap_or_else(|e| panic!("{}: {}", name, e))
}

#[test]
fn two_client_fixtures_pair_into_their_relative_rate() {
    let a = client("16-free-run-client-a.offsets");
    let b = client("17-free-run-client-b.offsets");
    let pairs = pair::pair(&a, &b, 5_000_000_000).expect("the fixtures pair");
    // Every A tick but the first and the last falls inside B's span.
    assert_eq!(pairs.len(), 1200);

    let text = pair::render_paired("fixture-pair", &a, &b, &pairs);
    let series = freerun::parse_series(std::path::Path::new("paired"), &text).unwrap();
    let config = MeasureConfig::read(&repository_root()).unwrap();
    let fit = freerun::fit(&series, &SlopeSettings::from_config(&config)).expect("a slope");
    let declared = a.series.declared_rate_ppm.unwrap() - b.series.declared_rate_ppm.unwrap();
    assert!((declared - 37.5).abs() < 1e-9);
    assert!(
        (fit.ppm - declared).abs() <= fit.half_width_ppm.max(0.05),
        "fit {:+.4} ppm (+/-{:.4}) against the declared {:+.4}",
        fit.ppm,
        fit.half_width_ppm,
        declared
    );
    // The paired series says what it is, for the fit and for a reader.
    assert!(text.contains("correction = disabled"));
    assert!(text.contains("client_a = free-run-client-a"));
}

#[test]
fn a_relative_series_is_not_a_client_series() {
    // 10-free-run-noiseless is already a RELATIVE series with no time base; a
    // pair of it with anything is refused by name rather than differenced.
    let relative = client("10-free-run-noiseless.offsets");
    let b = client("17-free-run-client-b.offsets");
    let err = pair::pair(&relative, &b, 5_000_000_000).unwrap_err();
    assert_eq!(err.condition(), "not-free-run");
}

#[test]
fn a_hardware_report_from_a_dirty_tree_is_refused() {
    let dirty = BuildIdentity {
        commit: "0".repeat(40),
        clean: false,
        dirty_paths: vec!["M config/sync.conf".to_string()],
    };
    let err = report::require_clean_for_hardware(&dirty, "hardware").unwrap_err();
    assert_eq!(err.condition(), "hardware-run-on-dirty-tree");
    assert!(report::require_clean_for_hardware(&dirty, "synthetic").is_ok());
    assert_eq!(report::report_source("fixture"), Some("synthetic"));
    assert_eq!(report::report_source("hardware"), Some("hardware"));
    assert_eq!(report::report_source("guess"), None);
}
