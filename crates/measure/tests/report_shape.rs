//! What a completed run writes, where it writes it, and that a second run over
//! the same input says the same thing.
//!
//! Every run below writes into a scratch directory rather than into
//! `docs/measurements/`, so the suite can assert "exactly one new file"
//! honestly without a test leaving evidence behind. The committed reports are
//! written by `make measure-fixture-reports`, which is the same code path with
//! the real destination.

use std::collections::BTreeSet;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use chorus_measure::config::MeasureConfig;
use chorus_measure::freerun::{self, SlopeSettings};
use chorus_measure::jitter::{self, JitterRun, PowerSaveMode};
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
        dirty_paths: Vec::new(),
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
        dirty_paths: vec![
            "M crates/measure/src/lag.rs".to_string(),
            "?? docs/measurements/rig3-free-run-noiseless-fixture.md".to_string(),
        ],
    };
    let body = lag_report(&analysed, &dirty, None);
    assert!(
        body.contains("carried 2 uncommitted path(s)"),
        "a dirty tree has to be visible in the report: {}",
        body
    );
    // And it names them. "Dirty" on its own cannot tell a reader whether the
    // code that produced the number was modified or whether a previous report
    // in the same batch was simply not committed yet, and those are very
    // different things to know about a measurement.
    assert!(body.contains("M crates/measure/src/lag.rs"), "{}", body);
    assert!(!body.contains("Tree at that commit: clean"));
}

/// Enough uncommitted paths to overflow the list, and the report says how many
/// it did not show rather than silently truncating.
#[test]
fn a_report_that_cannot_list_every_uncommitted_path_says_how_many_it_left_out() {
    let dirty = BuildIdentity {
        commit: "a".repeat(40),
        clean: false,
        dirty_paths: (0..30).map(|n| format!("M file-{}.rs", n)).collect(),
    };
    let state = dirty.tree_state();
    assert!(state.contains("carried 30 uncommitted path(s)"), "{}", state);
    assert!(state.contains("and 18 more"), "{}", state);
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

// -------------------------------------------------------------------------
// chorus#WIFI-7: the wireless characterization's report shape (AC-12) and the
// refusal when a report cannot be written (AC-18).
// -------------------------------------------------------------------------

/// The committed series for one power-save mode, and its label.
fn jitter_series(mode: PowerSaveMode) -> (PathBuf, &'static str) {
    match mode {
        PowerSaveMode::None => (
            fixture("14-wireless-jitter-ps-none.offsets"),
            "wireless-ps-none",
        ),
        PowerSaveMode::MinModem => (
            fixture("15-wireless-jitter-ps-min-modem.offsets"),
            "wireless-ps-min-modem",
        ),
        PowerSaveMode::MaxModem => panic!("no series is committed for the maximum modem mode"),
    }
}

fn jitter_report(mode: PowerSaveMode, build: &BuildIdentity) -> String {
    let root = repository_root();
    let (path, label) = jitter_series(mode);
    let series = freerun::read_series(&path).expect("a committed series");
    let summary = jitter::analyse(&series).expect("a committed series is a distribution");
    let run = JitterRun {
        label,
        series: &series,
        summary: &summary,
        mode,
        transport: "wireless",
        from_fixture: true,
        build,
        command: "make measure-fixture-reports",
        root: &root,
    };
    jitter::render_jitter_report(&run)
}

/// AC-12. One report per power-save mode, each naming the mode that was in
/// force, the transport and the build it measured.
#[test]
fn a_jitter_run_writes_one_report_per_power_save_mode_naming_the_mode_the_transport_and_the_build()
{
    let dir = scratch("jitter-per-mode");
    let before = listing(&dir);
    let build = pinned_build();

    for (mode, name) in [
        (PowerSaveMode::None, "rig3-jitter-wireless-ps-none.md"),
        (
            PowerSaveMode::MinModem,
            "rig3-jitter-wireless-ps-min-modem.md",
        ),
    ] {
        let body = jitter_report(mode, &build);
        report::write_report(&dir, name, &body).expect("the scratch directory is writable");
        assert!(
            body.contains(&format!("Power save mode in force: **{}**", mode.name())),
            "the report does not name the mode that was in force: {}",
            body
        );
        assert!(
            body.contains(mode.platform_name()),
            "and it names the platform's own spelling too: {}",
            body
        );
        assert!(
            body.contains("Transport: **wireless**"),
            "the report does not name the transport: {}",
            body
        );
        assert!(
            body.contains(&build.commit),
            "the report does not name the build it measured: {}",
            body
        );
        // And it says, loudly, what a modelled series is not.
        assert!(
            body.contains("NOT A MEASUREMENT"),
            "a report over a committed fixture has to say so: {}",
            body
        );
    }

    let after = listing(&dir);
    let new: Vec<&String> = after.difference(&before).collect();
    assert_eq!(
        new.len(),
        2,
        "one report per mode, and these appeared: {:?}",
        new
    );

    // The two reports are different numbers, so "one per mode" is a difference
    // and not two copies of one run.
    let none = std::fs::read_to_string(dir.join("rig3-jitter-wireless-ps-none.md")).unwrap();
    let default =
        std::fs::read_to_string(dir.join("rig3-jitter-wireless-ps-min-modem.md")).unwrap();
    assert_ne!(none, default);
    let _ = std::fs::remove_dir_all(&dir);
}

/// AC-12's second half: a report whose mode is not known is REFUSED, and
/// nothing is written.
#[test]
fn a_report_whose_power_save_mode_is_not_known_is_refused_and_nothing_is_written() {
    let dir = scratch("jitter-no-mode");
    let (series, _) = jitter_series(PowerSaveMode::None);

    for mode in [None, Some("unknown"), Some(""), Some("WIFI_PS_NONE")] {
        let mut args = vec![
            "jitter".to_string(),
            series.to_string_lossy().into_owned(),
            "--label".to_string(),
            "refused".to_string(),
            "--transport".to_string(),
            "wireless".to_string(),
            "--out".to_string(),
            dir.to_string_lossy().into_owned(),
        ];
        if let Some(word) = mode {
            args.push("--mode".to_string());
            args.push(word.to_string());
        }
        let output = Command::new(env!("CARGO_BIN_EXE_chorus-measure"))
            .args(&args)
            .output()
            .expect("the measure binary runs");
        let said = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_ne!(
            output.status.code(),
            Some(0),
            "a mode of {:?} was accepted: {}",
            mode,
            said
        );
        assert!(
            said.contains("NO REPORT IS WRITTEN"),
            "the refusal has to say nothing was written: {}",
            said
        );
        assert!(
            said.contains("none, min-modem, max-modem"),
            "and name the modes a report may carry: {}",
            said
        );
        assert!(
            listing(&dir).is_empty(),
            "a refused run left something behind: {:?}",
            listing(&dir)
        );
    }

    // A transport nobody committed is refused the same way.
    let output = Command::new(env!("CARGO_BIN_EXE_chorus-measure"))
        .args([
            "jitter",
            series.to_str().unwrap(),
            "--label",
            "refused",
            "--mode",
            "none",
            "--transport",
            "wifi",
            "--out",
            dir.to_str().unwrap(),
        ])
        .output()
        .expect("the measure binary runs");
    let said = String::from_utf8_lossy(&output.stderr).into_owned();
    assert_ne!(output.status.code(), Some(0), "{}", said);
    assert!(said.contains("wired, wireless"), "{}", said);
    assert!(listing(&dir).is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

/// AC-18. A report that cannot be written names the path and the reason, exits
/// non-zero, and leaves no partial file behind.
#[test]
fn a_report_that_cannot_be_written_names_the_path_and_leaves_nothing_partial() {
    // The destination is not there at all.
    let missing = std::env::temp_dir().join(format!(
        "chorus-no-such-measurements-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&missing);
    let err = report::write_report(&missing, "x.md", "body").unwrap_err();
    assert_eq!(err.condition(), "destination-missing");
    assert!(err.to_string().contains(&missing.display().to_string()));
    assert!(!missing.exists(), "the refusal created the destination");

    // The destination exists and is not a directory.
    let file = std::env::temp_dir().join(format!("chorus-not-a-dir-{}", std::process::id()));
    std::fs::write(&file, "not a directory").unwrap();
    let err = report::write_report(&file, "x.md", "body").unwrap_err();
    assert_eq!(err.condition(), "destination-not-a-directory");
    assert!(err.to_string().contains(&file.display().to_string()));
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "not a directory",
        "the refusal wrote over the thing it refused"
    );
    let _ = std::fs::remove_file(&file);

    // The destination is a directory that cannot be written to. This is the one
    // that matters for "no partial file": the body goes to a temporary beside
    // the destination and is renamed into place, so a denied write has a window
    // in which a partial file could exist.
    let locked = scratch("jitter-read-only");
    let permissions = std::fs::Permissions::from_mode(0o555);
    std::fs::set_permissions(&locked, permissions).unwrap();
    let err = report::write_report(&locked, "rig3-jitter-denied.md", "body")
        .expect_err("a read-only directory cannot hold a report");
    assert_eq!(err.condition(), "destination-not-writable");
    assert!(
        err.to_string().contains("rig3-jitter-denied.md"),
        "the refusal has to name the path: {}",
        err
    );
    assert!(
        err.to_string().contains("Nothing partial has been left behind"),
        "{}",
        err
    );
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        listing(&locked).is_empty(),
        "a denied write left something behind: {:?}",
        listing(&locked)
    );
    let _ = std::fs::remove_dir_all(&locked);

    // And the SHIPPED BINARY does the same, exiting non-zero and naming the
    // destination it could not write into.
    let (series, _) = jitter_series(PowerSaveMode::None);
    let output = Command::new(env!("CARGO_BIN_EXE_chorus-measure"))
        .args([
            "jitter",
            series.to_str().unwrap(),
            "--label",
            "denied",
            "--mode",
            "none",
            "--transport",
            "wireless",
            "--out",
            missing.to_str().unwrap(),
        ])
        .output()
        .expect("the measure binary runs");
    let said = String::from_utf8_lossy(&output.stderr).into_owned();
    assert_ne!(output.status.code(), Some(0), "{}", said);
    assert!(
        said.contains(&missing.display().to_string()),
        "the refusal has to name the path: {}",
        said
    );
    assert!(!missing.exists());
}

/// A second analysis over the same series reports identical numbers.
#[test]
fn a_second_jitter_run_over_the_same_series_reports_identical_numbers() {
    for mode in [PowerSaveMode::None, PowerSaveMode::MinModem] {
        let (path, _) = jitter_series(mode);
        let series = freerun::read_series(&path).unwrap();
        assert_eq!(
            jitter::analyse(&series).unwrap(),
            jitter::analyse(&series).unwrap()
        );
    }
    let build = pinned_build();
    let once = jitter_report(PowerSaveMode::None, &build);
    let twice = jitter_report(PowerSaveMode::None, &build);
    assert_eq!(once, twice);
}

/// The committed jitter reports really are in `docs/measurements/`, one per
/// mode, and each carries the figures its committed series produces today.
#[test]
fn the_committed_jitter_reports_are_in_the_measurements_directory_and_are_reproducible() {
    let dir = repository_root().join(MEASUREMENTS_DIR);
    for (mode, name) in [
        (PowerSaveMode::None, "rig3-jitter-wireless-ps-none.md"),
        (
            PowerSaveMode::MinModem,
            "rig3-jitter-wireless-ps-min-modem.md",
        ),
    ] {
        let path = dir.join(name);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{} is committed: {}", path.display(), e));

        // It names a build, as a forty character hexadecimal commit.
        let line = text
            .lines()
            .find(|l| l.starts_with("Build measured: "))
            .unwrap_or_else(|| panic!("{} names no build", name));
        let commit: String = line.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        assert!(commit.len() >= 40, "{} is not a commit: {}", name, line);

        assert!(
            text.contains(&format!("Power save mode in force: **{}**", mode.name())),
            "{} does not name the mode that was in force",
            name
        );
        assert!(text.contains("Transport: **wireless**"), "{}", name);

        // And the figures are the ones the committed series produces today,
        // which is what makes the saved report reproducible by a reader who did
        // not run it.
        let (series_path, _) = jitter_series(mode);
        let series = freerun::read_series(&series_path).unwrap();
        let summary = jitter::analyse(&series).unwrap();
        for (what, rendered) in [
            ("median", format!("{:.1} us", summary.median_us)),
            ("p95", format!("{:.1} us", summary.p95_us)),
            ("maximum", format!("{:.1} us", summary.max_us)),
            ("peak to peak", format!("{:.1} us", summary.peak_to_peak_us)),
        ] {
            assert!(
                text.contains(&rendered),
                "{} does not carry the {} its series produces ({}); re-run `make \
                 measure-fixture-reports`",
                name,
                what,
                rendered
            );
        }
    }
}

/// The two committed series are materially different, so a check over them can
/// see a difference at all.
///
/// Without this the pair could quietly become two copies of one shape and every
/// assertion above would still pass, which would make the whole "one report per
/// mode" arrangement decorative.
#[test]
fn the_two_committed_series_differ_the_way_the_two_modes_do() {
    let disabled = {
        let (path, _) = jitter_series(PowerSaveMode::None);
        jitter::analyse(&freerun::read_series(&path).unwrap()).unwrap()
    };
    let default = {
        let (path, _) = jitter_series(PowerSaveMode::MinModem);
        jitter::analyse(&freerun::read_series(&path).unwrap()).unwrap()
    };
    assert!(
        default.median_us > disabled.median_us * 10.0,
        "the platform default's series is not materially noisier than the disabled one: {:.1} us \
         against {:.1} us",
        default.median_us,
        disabled.median_us
    );
    assert!(
        default.p95_us > disabled.p95_us * 10.0,
        "{:.1} us against {:.1} us",
        default.p95_us,
        disabled.p95_us
    );
}

/// The floor a distribution is computed from, and the words a report may name,
/// agree with the committed configuration.
#[test]
fn the_jitter_analysis_agrees_with_the_committed_configuration() {
    let config = MeasureConfig::read(&repository_root()).unwrap();
    assert_eq!(
        jitter::MIN_OBSERVATIONS,
        config.free_run_min_points,
        "the floor under a distribution and the floor under a slope fit are the same number for \
         the same reason, and they have drifted apart"
    );

    let text = std::fs::read_to_string(repository_root().join("config/transport.conf"))
        .expect("config/transport.conf is committed");
    let declared: Vec<String> = text
        .lines()
        .filter_map(|line| {
            let line = match line.find('#') {
                Some(at) => &line[..at],
                None => line,
            };
            line.split_once('=').and_then(|(k, v)| {
                (k.trim() == "transports").then(|| v.trim().to_string())
            })
        })
        .collect();
    assert_eq!(declared.len(), 1, "config/transport.conf names transports once");
    let declared: Vec<&str> = declared[0].split_whitespace().collect();
    assert_eq!(
        declared, jitter::TRANSPORTS,
        "the transports a report may name and the ones config/transport.conf commits have \
         drifted apart"
    );
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
