//! What a completed run writes, where it writes it, and that a second run over
//! the same input says the same thing.
//!
//! Every run below writes into a scratch directory rather than into
//! `docs/measurements/`, so the suite can assert "exactly one new file"
//! honestly without a test leaving evidence behind. The committed reports are
//! written by `make measure-fixture-reports`, which is the same code path with
//! the real destination.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use chorus_measure::config::MeasureConfig;
use chorus_measure::freerun::{self, SlopeSettings};
use chorus_measure::lag::{self, LagSettings};
use chorus_measure::report::{
    self, Baseline, BuildIdentity, FreeRunRun, LagRun, BASELINE_FILE, MEASUREMENTS_DIR,
};
use chorus_measure::{fixtures, repository_root, wav};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "chorus-measure-{}-{}-{}",
        name,
        std::process::id(),
        std::time::Instant::now().elapsed().as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

fn listing(dir: &Path) -> BTreeSet<String> {
    std::fs::read_dir(dir)
        .expect("the scratch directory is readable")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect()
}

fn fixture(name: &str) -> PathBuf {
    repository_root().join("fixtures/measure").join(name)
}

/// A build identity that does not move between runs, so the reproducibility
/// assertions are about the figures rather than about the commit.
fn pinned_build() -> BuildIdentity {
    BuildIdentity {
        commit: "0".repeat(40),
        clean: true,
    }
}

struct Analysed {
    capture: wav::Capture,
    settings: LagSettings,
    summary: lag::LagSummary,
}

fn analyse(name: &str) -> Analysed {
    let root = repository_root();
    let config = MeasureConfig::read(&root).expect("config/measure.conf is committed");
    let capture = wav::read_capture(&fixture(name), config.capture_sample_rate_hz)
        .expect("a committed capture");
    let settings = LagSettings::from_config(&config, capture.sample_rate_hz);
    let summary = lag::estimate(&capture, &settings).expect("the reference capture resolves");
    Analysed {
        capture,
        settings,
        summary,
    }
}

fn lag_report(analysed: &Analysed, build: &BuildIdentity, baseline: Option<&Baseline>) -> String {
    let root = repository_root();
    let run = LagRun {
        label: "fixture-reference-capture",
        capture: &analysed.capture,
        settings: &analysed.settings,
        summary: &analysed.summary,
        build,
        baseline,
        command: "make measure-fixture-reports",
        analysis_us: 0,
        root: &root,
    };
    report::render_lag_report(&run)
}

/// AC-3 and AC-9. A completed run writes exactly one new file under the
/// measurements directory, and that file names the build it measured, whether
/// that tree was clean, the capture rate, the window count and all three lag
/// figures.
#[test]
fn a_completed_run_writes_one_report_naming_the_build_and_every_figure() {
    let dir = scratch("one-report");
    let before = listing(&dir);
    let analysed = analyse("01-chirp-pair-a.wav");
    let build = pinned_build();
    let body = lag_report(&analysed, &build, None);
    let written = report::write_report(&dir, "rig3-lag-fixture-reference-capture.md", &body)
        .expect("the scratch directory is writable");

    let after = listing(&dir);
    let new: Vec<&String> = after.difference(&before).collect();
    assert_eq!(new.len(), 1, "exactly one new file, and these appeared: {:?}", new);
    assert_eq!(new[0], "rig3-lag-fixture-reference-capture.md");

    let saved = std::fs::read_to_string(&written).expect("the report is readable");
    assert!(saved.contains(&build.commit), "the report names no commit");
    assert!(saved.contains("Tree at that commit: clean"), "{}", saved);
    assert!(saved.contains("96000 Hz"), "the capture rate is missing");
    assert!(
        saved.contains(&format!(
            "{} offered, {} used",
            analysed.summary.windows_total, analysed.summary.windows_used
        )),
        "the window count is missing"
    );
    assert!(saved.contains("median lag"), "the median is missing");
    assert!(saved.contains("p95 lag"), "the p95 is missing");
    assert!(saved.contains("maximum lag"), "the maximum is missing");
    assert!(
        saved.contains(&format!("{:+.3} us", analysed.summary.median_us)),
        "the median's value is missing"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The same criterion's other half: a dirty tree is reported as a dirty tree.
/// A report that always said "clean" would pass the assertion above.
#[test]
fn a_report_says_when_the_tree_it_measured_was_not_clean() {
    let analysed = analyse("01-chirp-pair-a.wav");
    let dirty = BuildIdentity {
        commit: "a".repeat(40),
        clean: false,
    };
    let body = lag_report(&analysed, &dirty, None);
    assert!(
        body.contains("carried uncommitted changes"),
        "a dirty tree has to be visible in the report: {}",
        body
    );
    assert!(!body.contains("Tree at that commit: clean"));
}

/// AC-8. A free-run analysis records its slope as the baseline in a committed
/// artifact, and a later run names both that value and the report that
/// established it.
#[test]
fn a_free_run_records_a_baseline_and_a_later_run_cites_it_and_its_report() {
    let dir = scratch("baseline");
    let root = repository_root();
    let config = MeasureConfig::read(&root).unwrap();
    let settings = SlopeSettings::from_config(&config);
    let series = freerun::read_series(&fixture("10-free-run-noiseless.offsets")).unwrap();
    let fit = freerun::fit(&series, &settings).unwrap();
    let build = pinned_build();

    let report_name = "rig3-free-run-noiseless-fixture.md";
    let baseline = Baseline {
        ppm: fit.ppm,
        half_width_ppm: fit.half_width_ppm,
        source: "fixture".to_string(),
        series: "fixtures/measure/10-free-run-noiseless.offsets".to_string(),
        established_by: format!("{}/{}", MEASUREMENTS_DIR, report_name),
        established_at_commit: build.commit.clone(),
    };
    let free_run = FreeRunRun {
        label: "noiseless-fixture",
        series: &series,
        settings: &settings,
        fit: &fit,
        build: &build,
        baseline: &baseline,
        command: "make measure-fixture-reports",
        root: &root,
    };
    report::write_report(&dir, report_name, &report::render_free_run_report(&free_run)).unwrap();
    report::write_report(&dir, BASELINE_FILE, &baseline.render()).unwrap();

    // The artifact is on disk and reads back as what was recorded.
    let recorded = Baseline::read(&dir.join(BASELINE_FILE)).expect("the baseline was recorded");
    assert!((recorded.ppm - fit.ppm).abs() < 1e-4);
    assert_eq!(recorded.established_by, baseline.established_by);

    // And a LATER run names both the value and the report that established it.
    let analysed = analyse("01-chirp-pair-a.wav");
    let later = lag_report(&analysed, &build, Some(&recorded));
    assert!(
        later.contains(&format!("{:+.4} ppm", recorded.ppm)),
        "the later report does not name the baseline value: {}",
        later
    );
    assert!(
        later.contains(&recorded.established_by),
        "the later report does not name the report that established the baseline: {}",
        later
    );
    // A fixture-derived baseline has to say so, so nobody reads it as a
    // statement about a real crystal.
    assert!(later.contains("NOT a statement about any real crystal"), "{}", later);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A run with no baseline recorded says so rather than quietly omitting the
/// section, because a missing citation is a gap in the record and not a pass.
#[test]
fn a_run_with_no_recorded_baseline_says_that_rather_than_omitting_it() {
    let analysed = analyse("01-chirp-pair-a.wav");
    let body = lag_report(&analysed, &pinned_build(), None);
    assert!(body.contains("No free-run baseline has been recorded"), "{}", body);
    assert!(body.contains("not a passing result"), "{}", body);
}

/// AC-10, first half. Every committed fixture input regenerates from its
/// committed parameters byte for byte.
#[test]
fn every_committed_fixture_regenerates_from_its_parameters_byte_for_byte() {
    let dir = repository_root().join(fixtures::FIXTURES_DIR);
    let all = fixtures::read_all(&dir).expect("the fixture parameters are committed");
    assert!(
        all.len() >= 13,
        "the fixture set is smaller than the phase needs: {} files",
        all.len()
    );
    for params in &all {
        let generated = fixtures::generate(params)
            .unwrap_or_else(|e| panic!("{} could not be generated: {}", params.path.display(), e));
        let committed = std::fs::read(params.output_path()).unwrap_or_else(|e| {
            panic!(
                "{} is committed beside its parameters: {}",
                params.output_path().display(),
                e
            )
        });
        assert_eq!(
            committed.len(),
            generated.len(),
            "{} is {} bytes and its parameters generate {}",
            params.output_path().display(),
            committed.len(),
            generated.len()
        );
        assert!(
            committed == generated,
            "{} differs from what its parameters generate; run `make measure-fixtures` if the \
             parameters were the thing that changed",
            params.output_path().display()
        );
    }
}

/// AC-10, second half. A second analysis run over the same input reports
/// identical numbers.
#[test]
fn a_second_run_over_the_same_input_reports_identical_numbers() {
    let once = analyse("01-chirp-pair-a.wav");
    let twice = analyse("01-chirp-pair-a.wav");
    assert_eq!(once.summary, twice.summary);

    let build = pinned_build();
    let first = report::lag_figures(&LagRun {
        label: "fixture-reference-capture",
        capture: &once.capture,
        settings: &once.settings,
        summary: &once.summary,
        build: &build,
        baseline: None,
        command: "make measure-fixture-reports",
        analysis_us: 0,
        root: &repository_root(),
    });
    let second = report::lag_figures(&LagRun {
        label: "fixture-reference-capture",
        capture: &twice.capture,
        settings: &twice.settings,
        summary: &twice.summary,
        build: &build,
        // A different analysis time, to prove the figures do not carry one.
        baseline: None,
        command: "make measure-fixture-reports",
        analysis_us: 999_999,
        root: &repository_root(),
    });
    assert_eq!(
        first, second,
        "the figures a report carries have to be identical on a second run"
    );

    let series = freerun::read_series(&fixture("10-free-run-noiseless.offsets")).unwrap();
    let settings = SlopeSettings::from_config(&MeasureConfig::read(&repository_root()).unwrap());
    assert_eq!(
        freerun::fit(&series, &settings).unwrap(),
        freerun::fit(&series, &settings).unwrap()
    );
}

/// The committed reports really are in `docs/measurements/`, they really do
/// name a build, and the baseline the lag report cites really is the one the
/// free-run report established.
///
/// This is the assertion about the tree rather than about a scratch directory:
/// the phase's third criterion is that a completed run's report is SAVED there,
/// and a suite that only ever wrote into a temporary directory would never have
/// checked that anything was.
#[test]
fn the_committed_reports_are_in_the_measurements_directory_and_agree_with_each_other() {
    let root = repository_root();
    let dir = root.join(MEASUREMENTS_DIR);
    let lag_report = dir.join("rig3-lag-fixture-reference-capture.md");
    let free_run_report = dir.join("rig3-free-run-noiseless-fixture.md");
    let baseline_path = dir.join(BASELINE_FILE);

    let lag_text = std::fs::read_to_string(&lag_report)
        .unwrap_or_else(|e| panic!("{} is committed: {}", lag_report.display(), e));
    let free_run_text = std::fs::read_to_string(&free_run_report)
        .unwrap_or_else(|e| panic!("{} is committed: {}", free_run_report.display(), e));
    let baseline = Baseline::read(&baseline_path)
        .unwrap_or_else(|e| panic!("{} is committed: {}", baseline_path.display(), e));

    // Each names a build, as a forty character hexadecimal commit.
    for (name, text) in [("lag", &lag_text), ("free-run", &free_run_text)] {
        let line = text
            .lines()
            .find(|l| l.starts_with("Build measured: "))
            .unwrap_or_else(|| panic!("the {} report names no build", name));
        let commit: String = line.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        assert!(
            commit.len() >= 40,
            "the {} report's build line is not a commit: {}",
            name,
            line
        );
    }

    // The baseline artifact points at the report that established it, that
    // report exists, and the lag report cites the same value.
    assert_eq!(
        baseline.established_by,
        format!("{}/rig3-free-run-noiseless-fixture.md", MEASUREMENTS_DIR)
    );
    assert!(root.join(&baseline.established_by).exists());
    assert!(
        lag_text.contains(&format!("{:+.4} ppm", baseline.ppm)),
        "the committed lag report does not cite the committed baseline of {:+.4} ppm",
        baseline.ppm
    );
    assert!(
        lag_text.contains(&baseline.established_by),
        "the committed lag report does not name the report that established the baseline"
    );
    // And the honest label survived into the committed artifact.
    assert_eq!(baseline.source, "fixture");
    assert!(free_run_text.contains("`source = fixture` is the honest label here"));
}

/// The figures in the committed lag report are the figures the estimator
/// produces from the committed capture today.
///
/// This is what makes the saved report reproducible by a reader who did not run
/// it: not a promise in prose, an assertion that re-running the analysis
/// reproduces the table.
#[test]
fn the_committed_report_carries_the_figures_the_committed_capture_produces() {
    let analysed = analyse("01-chirp-pair-a.wav");
    let text = std::fs::read_to_string(
        repository_root()
            .join(MEASUREMENTS_DIR)
            .join("rig3-lag-fixture-reference-capture.md"),
    )
    .expect("the committed lag report");
    for (name, rendered) in [
        ("median", format!("{:+.3} us", analysed.summary.median_us)),
        ("p95", format!("{:.3} us", analysed.summary.p95_abs_us)),
        ("maximum", format!("{:.3} us", analysed.summary.max_abs_us)),
    ] {
        assert!(
            text.contains(&rendered),
            "the committed report does not carry the {} this capture produces ({}); re-run \
             `make measure-fixture-reports`",
            name,
            rendered
        );
    }
}
