//! The simulator regression.
//!
//! Covers AC1 (the modelled playout error is driven below 1 ms and held there
//! for the rest of the run, across the committed scenarios) and AC5 (the same
//! seed, skew and jitter parameters reproduce the same series exactly).
//!
//! Every scenario under `fixtures/sync/` runs. Adding a scenario is adding a
//! file; there is nothing to register here.

use std::fs;
use std::path::{Path, PathBuf};

use chorus_sync::{run, Scenario};

fn scenario_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sync")
}

fn committed_scenarios() -> Vec<(String, Scenario)> {
    let dir = scenario_dir();
    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{} is unreadable: {}", dir.display(), e))
        .map(|entry| entry.expect("readable directory entry").path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("cfg"))
        .collect();
    paths.sort();

    let mut scenarios = Vec::new();
    for path in paths {
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{} is unreadable: {}", path.display(), e));
        let scenario = Scenario::parse(&text)
            .unwrap_or_else(|e| panic!("{} does not parse: {}", path.display(), e));
        scenarios.push((
            path.file_name()
                .expect("a file name")
                .to_string_lossy()
                .into_owned(),
            scenario,
        ));
    }
    scenarios
}

#[test]
fn the_committed_scenarios_are_there_and_parse() {
    let scenarios = committed_scenarios();
    assert!(
        scenarios.len() >= 4,
        "expected a representative set of committed scenarios, found {}",
        scenarios.len()
    );
    let mut names: Vec<&str> = scenarios.iter().map(|(_, s)| s.name.as_str()).collect();
    names.sort();
    let unique = names.len();
    names.dedup();
    assert_eq!(unique, names.len(), "two scenarios share a name");
}

#[test]
fn every_scenario_drives_the_playout_error_below_its_bound_and_holds_it() {
    for (file, scenario) in committed_scenarios() {
        let result = run(&scenario.config)
            .unwrap_or_else(|e| panic!("{}: the committed configuration is refused: {}", file, e));

        assert_eq!(
            result.samples.len() as u64,
            scenario.config.steps(),
            "{}: one sample per step",
            file
        );

        let bound = scenario.error_bound_ns;
        let deadline = scenario.settle_deadline_ns();
        let settled = result.settle_time_ns(bound).unwrap_or_else(|| {
            panic!(
                "{}: the error never came inside {} ns for the rest of the run (peak {} ns)",
                file,
                bound,
                result.max_abs_error()
            )
        });
        assert!(
            settled <= deadline,
            "{}: settled at {} ns, deadline is {} ns",
            file,
            settled,
            deadline
        );

        let index = result.settle_index(bound).expect("it settled");
        let held = result.max_abs_error_after(index);
        assert!(
            held < bound,
            "{}: after settling the error still reached {} ns, bound is {} ns",
            file,
            held,
            bound
        );
    }
}

#[test]
fn the_hold_covers_the_whole_rest_of_the_run() {
    // "Below 1 ms and hold it for the rest of the run" is not "below 1 ms at
    // the end": check the tail sample by sample rather than trusting the
    // settle index that produced it.
    for (file, scenario) in committed_scenarios() {
        let result = run(&scenario.config).expect("the committed configuration runs");
        let index = result
            .settle_index(scenario.error_bound_ns)
            .unwrap_or_else(|| panic!("{}: never settled", file));
        for sample in &result.samples[index..] {
            assert!(
                sample.error_ns.abs() < scenario.error_bound_ns,
                "{}: {} ns of error at t = {} ns, after settling at index {}",
                file,
                sample.error_ns,
                sample.t_ns,
                index
            );
        }
        assert!(
            result.samples.len() - index > result.samples.len() / 2,
            "{}: settling consumed more than half the run",
            file
        );
    }
}

#[test]
fn the_noiseless_control_is_exact() {
    let scenarios = committed_scenarios();
    let (_, control) = scenarios
        .iter()
        .find(|(_, s)| s.name == "noiseless-control")
        .expect("the noiseless control scenario is committed");
    let result = run(&control.config).expect("it runs");
    assert_eq!(
        result.max_abs_error(),
        0,
        "identical clocks on a silent link should not need a single correction"
    );
    assert_eq!(result.hard_resyncs, 0);
}

#[test]
fn a_scenario_run_twice_produces_the_same_series() {
    for (file, scenario) in committed_scenarios() {
        let first = run(&scenario.config).expect("runs");
        let second = run(&scenario.config).expect("runs again");
        assert_eq!(
            first, second,
            "{}: two runs of one configuration differ",
            file
        );
    }
}

#[test]
fn the_seed_is_what_makes_a_run_reproducible() {
    // If the series were identical whatever the seed, the test above would be
    // asserting nothing at all.
    let scenarios = committed_scenarios();
    let (_, scenario) = scenarios
        .iter()
        .find(|(_, s)| s.name == "wired-loaded")
        .expect("the loaded scenario is committed");

    let baseline = run(&scenario.config).expect("runs");

    let mut reseeded = scenario.config.clone();
    reseeded.seed = scenario.config.seed.wrapping_add(1);
    let other = run(&reseeded).expect("runs");

    assert_ne!(
        baseline.samples, other.samples,
        "a different seed produced an identical jitter stream"
    );
    // A different seed is still the same link, so it still has to converge.
    assert!(other.holds_below(scenario.error_bound_ns, scenario.settle_deadline_ns()));
}

#[test]
fn changing_the_jitter_distribution_changes_the_run() {
    let scenarios = committed_scenarios();
    let (_, scenario) = scenarios
        .iter()
        .find(|(_, s)| s.name == "wired-quiet")
        .expect("the quiet scenario is committed");

    let baseline = run(&scenario.config).expect("runs");

    let mut noisier = scenario.config.clone();
    noisier.jitter = chorus_sync::JitterModel::Uniform { max_us: 400.0 };
    let other = run(&noisier).expect("runs");

    assert_ne!(baseline.samples, other.samples);
    assert!(
        other.holds_below(scenario.error_bound_ns, scenario.settle_deadline_ns()),
        "the servo should absorb a noisier link, peak after settling was {} ns",
        other.max_abs_error_after(other.settle_index(scenario.error_bound_ns).unwrap_or(0))
    );
}
