//! The committed cross-check vectors, and that they are what this
//! implementation actually produces.
//!
//! These files are what the ESP32-S3 endpoint's C sync core is held to, so a
//! vector that had drifted from the Rust implementation would be holding the
//! second implementation to a third thing. Regenerating has to reproduce every
//! committed file byte for byte, which is exactly the assertion
//! `crates/measure/tests/report_shape.rs` makes about the measurement
//! fixtures, for the same reason.
//!
//! Nothing here writes into `fixtures/`.

use std::fs;

use chorus_sync::crosscheck::{scenario_files, vector_path, vector_text};

#[test]
fn every_committed_scenario_has_a_committed_cross_check_vector() {
    let scenarios = scenario_files();
    assert!(
        scenarios.len() >= 4,
        "expected a representative set of committed scenarios, found {}",
        scenarios.len()
    );
    for scenario in scenarios {
        let vector = vector_path(&scenario);
        assert!(
            vector.exists(),
            "{} has no committed cross-check vector at {}; run `make sync-vectors`",
            scenario.display(),
            vector.display()
        );
    }
}

#[test]
fn regenerating_reproduces_every_committed_vector_byte_for_byte() {
    for scenario in scenario_files() {
        let vector = vector_path(&scenario);
        let committed = fs::read_to_string(&vector)
            .unwrap_or_else(|e| panic!("{} is unreadable: {}", vector.display(), e));
        let produced = vector_text(&scenario);
        assert_eq!(
            committed,
            produced,
            "{} is not what this implementation produces for {}. Either a scenario changed and \
             `make sync-vectors` has not been run, or the sync core changed and the C endpoint \
             is about to disagree with it.",
            vector.display(),
            scenario.display()
        );
    }
}

#[test]
fn a_vector_records_the_selection_and_not_only_the_outcome() {
    // The whole value of these files is that they say which sample the filter
    // chose, so a second implementation that converged by a different route
    // fails. A vector with no exchange block would be a summary, and a summary
    // is what SimResult already is.
    for scenario in scenario_files() {
        let text = vector_text(&scenario);
        let exchanges = text
            .lines()
            .skip_while(|line| !line.starts_with("[exchanges]"))
            .filter(|line| !line.starts_with('#') && !line.starts_with('['))
            .filter(|line| !line.trim().is_empty())
            .count();
        assert!(
            exchanges >= 29,
            "{} records only {} exchanges",
            scenario.display(),
            exchanges
        );
        for line in text
            .lines()
            .skip_while(|line| !line.starts_with("[exchanges]"))
            .filter(|line| !line.starts_with('#') && !line.starts_with('['))
            .filter(|line| !line.trim().is_empty())
        {
            let fields: Vec<&str> = line.split_whitespace().collect();
            assert_eq!(
                fields.len(),
                10,
                "{}: an exchange line has {} fields, not 10: {}",
                scenario.display(),
                fields.len(),
                line
            );
            assert!(
                fields[8] == "fine" || fields[8] == "hard-resync",
                "{}: unknown servo tier {}",
                scenario.display(),
                fields[8]
            );
        }
    }
}

#[test]
fn the_run_that_is_recorded_is_the_run_that_is_graded() {
    // `run` is `run_recorded` with the records dropped. If it ever stops being
    // that, a cross-check vector would describe a run nothing else executes.
    for scenario in scenario_files() {
        let text = fs::read_to_string(&scenario).expect("readable");
        let parsed = chorus_sync::Scenario::parse(&text).expect("parses");
        let plain = chorus_sync::run(&parsed.config).expect("runs");
        let (recorded, records) = chorus_sync::run_recorded(&parsed.config).expect("runs");
        assert_eq!(plain, recorded, "{}", scenario.display());
        assert_eq!(
            records.len() as u32,
            plain.exchanges,
            "{}: one record per exchange",
            scenario.display()
        );
    }
}
