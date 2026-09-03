//! Every place this rig is supposed to refuse rather than report a number.
//!
//! The roadmap phase's fail-safe is one sentence: "a run that cannot resolve
//! the two outputs says so and writes no number." Each condition below has its
//! own committed input, so a reader who did not take a capture can run every
//! refusal, and each assertion checks three things rather than one: that it
//! refused, that it named which condition it hit, and that nothing was written.
//!
//! Nothing here needs a device or a privilege.

use std::path::{Path, PathBuf};

use chorus_measure::chirp::ChirpSpec;
use chorus_measure::config::{MeasureConfig, CONFIG_FILE};
use chorus_measure::lag::{self, LagSettings};
use chorus_measure::report;
use chorus_measure::{repository_root, wav};

fn config() -> MeasureConfig {
    MeasureConfig::read(&repository_root()).expect("config/measure.conf is committed")
}

fn fixture(name: &str) -> PathBuf {
    repository_root().join("fixtures/measure").join(name)
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("chorus-refusals-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

/// Read a capture and analyse it, returning whichever of the two refusals it
/// hit, as the stable token that names it.
fn analyse_condition(name: &str) -> String {
    let config = config();
    match wav::read_capture(&fixture(name), config.capture_sample_rate_hz) {
        Err(e) => e.condition().to_string(),
        Ok(capture) => {
            let settings = LagSettings::from_config(&config, capture.sample_rate_hz);
            match lag::estimate(&capture, &settings) {
                Err(e) => e.condition().to_string(),
                Ok(summary) => panic!(
                    "{} was expected to refuse and reported a median of {:+.3} us",
                    name, summary.median_us
                ),
            }
        }
    }
}

fn analyse_message(name: &str) -> String {
    let config = config();
    match wav::read_capture(&fixture(name), config.capture_sample_rate_hz) {
        Err(e) => e.to_string(),
        Ok(capture) => {
            let settings = LagSettings::from_config(&config, capture.sample_rate_hz);
            lag::estimate(&capture, &settings)
                .map(|s| panic!("{} reported {:+.3} us", name, s.median_us))
                .unwrap_err()
                .to_string()
        }
    }
}

/// AC-14, all three conditions the criterion names, each against its own
/// committed input, each naming which one it hit.
#[test]
fn a_capture_that_cannot_be_resolved_names_which_condition_it_hit() {
    // A channel is silent.
    assert_eq!(analyse_condition("04-silence.wav"), "silent-channel");
    let said = analyse_message("04-silence.wav");
    assert!(said.contains("is silent"), "{}", said);
    assert!(said.contains("nothing to correlate"), "{}", said);

    // No chirp is present: energy everywhere and none of it a sweep.
    assert_eq!(analyse_condition("05-uncorrelated-noise.wav"), "no-chirp-present");
    let said = analyse_message("05-uncorrelated-noise.wav");
    assert!(said.contains("no chirp is present"), "{}", said);
    assert!(said.contains("Hz band"), "{}", said);

    // The correlation peak falls below the confidence floor: both channels
    // carry a chirp and they cannot be resolved against each other.
    assert_eq!(
        analyse_condition("06-unresolvable-chirps.wav"),
        "below-confidence-floor"
    );
    let said = analyse_message("06-unresolvable-chirps.wav");
    assert!(said.contains("could not be resolved"), "{}", said);
    assert!(said.contains("confidence floor"), "{}", said);
}

/// The three conditions are genuinely distinguished rather than one refusal
/// wearing three messages.
#[test]
fn the_three_unresolvable_conditions_are_told_apart() {
    let conditions = [
        analyse_condition("04-silence.wav"),
        analyse_condition("05-uncorrelated-noise.wav"),
        analyse_condition("06-unresolvable-chirps.wav"),
    ];
    let unique: std::collections::BTreeSet<&String> = conditions.iter().collect();
    assert_eq!(unique.len(), 3, "{:?} collapsed into fewer conditions", conditions);
}

/// AC-14's other half: a refused run reports no lag figure and leaves the
/// measurements directory unchanged.
///
/// The refusal is upstream of the report writer by construction - `estimate`
/// returns an error and there is no summary to render - and this asserts the
/// consequence rather than the construction.
#[test]
fn a_refused_run_reports_no_figure_and_leaves_the_measurements_directory_alone() {
    let dir = repository_root().join(report::MEASUREMENTS_DIR);
    let before: Vec<_> = std::fs::read_dir(&dir)
        .expect("docs/measurements is committed")
        .flatten()
        .map(|e| e.file_name())
        .collect();

    for name in [
        "04-silence.wav",
        "05-uncorrelated-noise.wav",
        "06-unresolvable-chirps.wav",
        "07-wrong-channel-count.wav",
        "08-unsupported-format.wav",
        "09-truncated-body.wav",
    ] {
        // Each one panics if it produced a figure, which is the "reports no lag
        // figure" half.
        let _ = analyse_condition(name);
    }

    let after: Vec<_> = std::fs::read_dir(&dir)
        .expect("docs/measurements is still there")
        .flatten()
        .map(|e| e.file_name())
        .collect();
    assert_eq!(before, after, "a refused run changed docs/measurements/");
}

/// AC-15. A run requesting a chirp amplitude above the declared ceiling refuses
/// to start, names the requested amplitude and the permitted one, and emits no
/// audio.
#[test]
fn an_amplitude_above_the_declared_ceiling_refuses_and_names_both_numbers() {
    let config = config();
    let over = config.chirp_amplitude_ceiling * 3.0;
    let err = ChirpSpec::new(
        config.chirp_start_hz,
        config.chirp_end_hz,
        config.chirp_period_us,
        over,
        config.chirp_amplitude_ceiling,
        CONFIG_FILE,
    )
    .expect_err("an over-level chirp must be refused");
    assert_eq!(err.condition(), "amplitude-above-ceiling");
    let said = err.to_string();
    assert!(said.contains(&over.to_string()), "the requested amplitude is not named: {}", said);
    assert!(
        said.contains(&config.chirp_amplitude_ceiling.to_string()),
        "the permitted amplitude is not named: {}",
        said
    );
    assert!(said.contains(CONFIG_FILE), "where the ceiling is declared is not named: {}", said);
    // "SHALL emit no audio": there is no ChirpSpec, so there is nothing any
    // caller could hand to a device. The refusal is in the constructor rather
    // than beside the device on purpose.
    assert!(said.contains("no audio has been emitted"), "{}", said);
}

/// And the ceiling is a gate rather than a blanket refusal: exactly the
/// declared value is permitted, and a hair over it is not.
#[test]
fn the_declared_ceiling_itself_is_permitted_and_a_hair_over_it_is_not() {
    let config = config();
    assert!(ChirpSpec::new(
        config.chirp_start_hz,
        config.chirp_end_hz,
        config.chirp_period_us,
        config.chirp_amplitude_ceiling,
        config.chirp_amplitude_ceiling,
        CONFIG_FILE,
    )
    .is_ok());
    let just_over = config.chirp_amplitude_ceiling * (1.0 + 1e-9);
    assert!(ChirpSpec::new(
        config.chirp_start_hz,
        config.chirp_end_hz,
        config.chirp_period_us,
        just_over,
        config.chirp_amplitude_ceiling,
        CONFIG_FILE,
    )
    .is_err());
}

/// The declared ceiling is a real limit and not full scale wearing a name.
#[test]
fn the_declared_ceiling_is_well_below_full_scale() {
    let ceiling = config().chirp_amplitude_ceiling;
    assert!(
        ceiling > 0.0 && ceiling <= 0.5,
        "a ceiling of {} full scale is not a guard in front of a loudspeaker",
        ceiling
    );
}

/// AC-16. Every malformed container the criterion names fails at start with a
/// typed error naming what it read and what it required.
#[test]
fn every_malformed_capture_fails_at_start_naming_what_it_read_and_what_it_required() {
    let rate = config().capture_sample_rate_hz;

    let err = wav::read_capture(&fixture("07-wrong-channel-count.wav"), rate)
        .expect_err("a mono capture is not a capture of two line outputs");
    assert_eq!(err.condition(), "wrong-channel-count");
    let said = err.to_string();
    assert!(said.contains("1 channel"), "{}", said);
    assert!(said.contains("2 were required"), "{}", said);

    let err = wav::read_capture(&fixture("08-unsupported-format.wav"), rate)
        .expect_err("a float capture is an unsupported layout");
    assert_eq!(err.condition(), "unsupported-sample-format");
    let said = err.to_string();
    assert!(said.contains("WAVE_FORMAT_IEEE_FLOAT"), "{}", said);
    assert!(said.contains("WAVE_FORMAT_PCM at 16 bits"), "{}", said);

    let err = wav::read_capture(&fixture("09-truncated-body.wav"), rate)
        .expect_err("a truncated capture is not a capture");
    assert_eq!(err.condition(), "truncated-body");
    let said = err.to_string();
    assert!(said.contains("truncated body"), "{}", said);
    assert!(said.contains("declares"), "{}", said);
    assert!(said.contains("are present"), "{}", said);

    // The rate the run declares is part of "a readable two-channel PCM
    // recording AT A DECLARED SAMPLE RATE", so a capture at another rate is
    // refused rather than analysed at the wrong scale, which would put every
    // microsecond out by the ratio.
    let err = wav::read_capture(&fixture("01-chirp-pair-a.wav"), 48_000)
        .expect_err("a capture at another rate is refused");
    assert_eq!(err.condition(), "sample-rate-mismatch");
    let said = err.to_string();
    assert!(said.contains("96000 Hz"), "{}", said);
    assert!(said.contains("48000 Hz"), "{}", said);
}

/// A malformed capture never produces a report, because it never produces a
/// capture to analyse.
#[test]
fn a_malformed_capture_writes_no_report() {
    let dir = scratch("malformed");
    let before = std::fs::read_dir(&dir).unwrap().count();
    for name in [
        "07-wrong-channel-count.wav",
        "08-unsupported-format.wav",
        "09-truncated-body.wav",
    ] {
        assert!(wav::read_capture(&fixture(name), config().capture_sample_rate_hz).is_err());
    }
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), before);
    let _ = std::fs::remove_dir_all(&dir);
}

/// AC-18. An unwritable destination exits non-zero naming the path and the
/// reason, and leaves behind no partial or empty report.
#[test]
fn an_unwritable_destination_is_refused_naming_the_path_and_the_reason() {
    // Case one: the directory is not there. Deterministic on every machine.
    let missing = std::env::temp_dir().join(format!(
        "chorus-measurements-absent-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&missing);
    let err = report::write_report(&missing, "rig3-lag-x.md", "body")
        .expect_err("a missing destination must be refused");
    assert_eq!(err.condition(), "destination-missing");
    let said = err.to_string();
    assert!(said.contains(&missing.display().to_string()), "{}", said);
    assert!(!missing.exists(), "the writer created its own destination");

    // Case two: the destination exists and is not a directory. ENOTDIR is
    // returned for every user including root, so this case is deterministic
    // where a permission bit would not be.
    let dir = scratch("not-a-directory");
    let a_file = dir.join("this-is-a-file");
    std::fs::write(&a_file, "not a directory").unwrap();
    let err = report::write_report(&a_file, "rig3-lag-x.md", "body")
        .expect_err("a destination that is a file must be refused");
    assert_eq!(err.condition(), "destination-not-a-directory");
    assert!(err.to_string().contains("is not a directory"));

    let err = report::write_report(&a_file.join("under-a-file"), "rig3-lag-x.md", "body")
        .expect_err("a destination under a file must be refused");
    assert_eq!(err.condition(), "destination-missing");
    assert!(err.to_string().contains("under-a-file"));

    // Case three: the directory exists and cannot be written to. On a machine
    // running as root the mode bits do not deny anything, so rather than skip
    // the case - which would be a green for a check that did not run - the
    // permission is PROBED and the assertion follows the probe: denied means
    // refuse, permitted means succeed. Either way something is asserted.
    let denied = dir.join("denied");
    std::fs::create_dir(&denied).unwrap();
    let mut mode = std::fs::metadata(&denied).unwrap().permissions();
    set_read_only(&mut mode);
    std::fs::set_permissions(&denied, mode).unwrap();
    let really_denied = std::fs::write(denied.join(".probe"), "x").is_err();
    let outcome = report::write_report(&denied, "rig3-lag-x.md", "body");
    if really_denied {
        let err = outcome.expect_err("a denied write must be refused");
        assert_eq!(err.condition(), "destination-not-writable");
        let said = err.to_string();
        assert!(said.contains("rig3-lag-x.md"), "{}", said);
        assert!(said.contains("Nothing partial has been left behind"), "{}", said);
    } else {
        // This process can write there whatever the mode bits say, so the
        // honest assertion is the opposite one: the writer must not invent a
        // refusal it has no reason for.
        outcome.expect("this process can write here, so the write must succeed");
    }
    let mut mode = std::fs::metadata(&denied).unwrap().permissions();
    set_writable(&mut mode);
    let _ = std::fs::set_permissions(&denied, mode);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A refused write leaves nothing behind at all, not even an empty file.
#[test]
fn a_refused_write_leaves_no_partial_and_no_empty_report() {
    let dir = scratch("no-partial");
    let denied = dir.join("denied");
    std::fs::create_dir(&denied).unwrap();
    let mut mode = std::fs::metadata(&denied).unwrap().permissions();
    set_read_only(&mut mode);
    std::fs::set_permissions(&denied, mode).unwrap();

    if std::fs::write(denied.join(".probe"), "x").is_err() {
        let _ = report::write_report(&denied, "rig3-lag-x.md", "a body that never landed");
        let mut mode = std::fs::metadata(&denied).unwrap().permissions();
        set_writable(&mut mode);
        std::fs::set_permissions(&denied, mode).unwrap();
        let left: Vec<String> = std::fs::read_dir(&denied)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            left.is_empty(),
            "a refused write left {:?} behind, and a partial report reads as evidence",
            left
        );
    } else {
        // Running as a user the mode bits do not constrain. The property still
        // holds and is asserted the only way it can be here: a successful write
        // leaves exactly the report and no temporary file beside it.
        let mut mode = std::fs::metadata(&denied).unwrap().permissions();
        set_writable(&mut mode);
        std::fs::set_permissions(&denied, mode).unwrap();
        let _ = std::fs::remove_file(denied.join(".probe"));
        report::write_report(&denied, "rig3-lag-x.md", "a body").unwrap();
        let left: Vec<String> = std::fs::read_dir(&denied)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left, vec!["rig3-lag-x.md".to_string()]);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
fn set_read_only(permissions: &mut std::fs::Permissions) {
    use std::os::unix::fs::PermissionsExt;
    permissions.set_mode(0o555);
}

#[cfg(unix)]
fn set_writable(permissions: &mut std::fs::Permissions) {
    use std::os::unix::fs::PermissionsExt;
    permissions.set_mode(0o755);
}

#[cfg(not(unix))]
fn set_read_only(permissions: &mut std::fs::Permissions) {
    permissions.set_readonly(true);
}

#[cfg(not(unix))]
fn set_writable(permissions: &mut std::fs::Permissions) {
    permissions.set_readonly(false);
}

/// The degenerate captures are real files in the tree, not something a test
/// built and threw away, so every refusal above is one a reader can reproduce.
#[test]
fn every_degenerate_input_the_criteria_name_is_committed() {
    for name in [
        "04-silence.wav",
        "05-uncorrelated-noise.wav",
        "06-unresolvable-chirps.wav",
        "07-wrong-channel-count.wav",
        "08-unsupported-format.wav",
        "09-truncated-body.wav",
        "12-free-run-too-short.offsets",
        "13-free-run-too-noisy.offsets",
    ] {
        let path = fixture(name);
        assert!(path.exists(), "{} is not committed", path.display());
        let params = path.with_extension("params");
        assert!(
            Path::new(&params).exists(),
            "{} has no committed parameters beside it",
            path.display()
        );
    }
}
