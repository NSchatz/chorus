//! The monotonic-clock guard for the measurement harness's timing path.
//!
//! `CLAUDE.md` guardrail 4 and the roadmap phase both want the same thing here:
//! this rig derives a rate in parts per million from elapsed time, and a clock
//! a human or an NTP daemon can step would put a jump in the middle of a
//! straight line and be read as drift. So every unit of this crate is
//! enumerated in the repository's committed `audio-path.conf`, and the suite
//! fails if any listed one reads a settable wall clock.
//!
//! The machinery is `crates/audio-path`, deliberately rather than a second
//! scanner of this crate's own: two scanners of the same shape drift apart, and
//! the point of that crate is that there is one list and one grader. What this
//! file adds is the assertions specific to the harness, including the red
//! demonstration this criterion asks for and the one that keeps the single
//! permitted exception from quietly widening.

use std::path::{Path, PathBuf};

use chorus_audio_path::list::{AudioPathList, Excluded, LIST_FILE};
use chorus_audio_path::scan::{scan, SETTABLE_CLOCK_NAMES};
use chorus_measure::repository_root;

/// The one unit of this crate permitted to read a settable clock, and the only
/// thing it is permitted to do with it.
const PERMITTED_READER: &str = "crates/measure/src/report.rs";

fn committed_list() -> (PathBuf, AudioPathList) {
    let root = repository_root();
    let text = std::fs::read_to_string(root.join(LIST_FILE))
        .unwrap_or_else(|e| panic!("{} is committed and readable: {}", LIST_FILE, e));
    let list = AudioPathList::parse(&text)
        .unwrap_or_else(|e| panic!("{} does not parse: {}", LIST_FILE, e));
    (root, list)
}

/// Every `.rs` file under this crate's `src/`, as the list would name it.
fn every_unit_of_this_crate(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    collect(&root.join("crates/measure/src"), root, &mut out);
    out.sort();
    out
}

fn collect(dir: &Path, root: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, root, out);
        } else if path.extension().map(|e| e == "rs").unwrap_or(false) {
            out.push(
                path.strip_prefix(root)
                    .expect("every unit is under the repository root")
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
}

/// The list accounts for every unit of this crate, one way or the other.
///
/// A list you can shrink is not a check: without this, moving a clock read into
/// a new file and listing nothing would leave the suite green.
#[test]
fn the_committed_list_accounts_for_every_unit_of_this_crate() {
    let (root, list) = committed_list();
    let units = every_unit_of_this_crate(&root);
    assert!(
        units.len() >= 11,
        "the harness is bigger than {} units; this test is looking in the wrong place",
        units.len()
    );
    for unit in &units {
        assert!(
            list.accounts_for(unit),
            "{} is neither listed in {} nor excluded there with a reason",
            unit,
            LIST_FILE
        );
    }
}

/// AC-11. No unit of the harness's timing path reads a settable wall clock.
#[test]
fn no_unit_of_the_harness_timing_path_reads_a_settable_wall_clock() {
    let (root, list) = committed_list();
    let finding = scan(&root, &list);
    assert!(
        finding.clock_reads.is_empty(),
        "a unit on the timestamp path reads a settable clock:\n{}",
        finding
    );
    assert!(
        finding.missing.is_empty() && finding.absent.is_empty(),
        "the list has a hole in it or names a file that is not there:\n{}",
        finding
    );
}

/// The demonstration the criterion asks for: a settable-clock read smuggled
/// into a listed unit of this crate turns the suite red.
///
/// Run against a copy of the tree with the read introduced, so it is committed
/// evidence a later reader can re-run rather than a branch name they have to be
/// told about.
#[test]
fn one_settable_clock_read_in_a_harness_unit_turns_the_check_red() {
    for victim in [
        // The unit the whole guardrail is about: a rate derived from elapsed
        // time.
        "crates/measure/src/freerun.rs",
        // And the estimator, where a clock read would be even less obviously
        // wrong, because nothing there looks like a clock at all.
        "crates/measure/src/lag.rs",
    ] {
        let (root, list) = committed_list();
        let scratch = scratch_copy(&root, victim);
        let target = scratch.join(victim);
        let mut source = std::fs::read_to_string(&target).unwrap();
        source.push_str(
            "\n\
             /// Introduced by the measurement harness's clock demonstration. This is\n\
             /// exactly the read the check exists to catch: a rate derived from a clock\n\
             /// that can be stepped.\n\
             pub fn demonstration_wall_clock_seconds() -> u64 {\n\
             \x20   std::time::SystemTime::now()\n\
             \x20       .duration_since(std::time::UNIX_EPOCH)\n\
             \x20       .map(|d| d.as_secs())\n\
             \x20       .unwrap_or(0)\n\
             }\n",
        );
        std::fs::write(&target, source).unwrap();

        let finding = scan(&scratch, &list);
        assert!(
            !finding.ok(),
            "the check stayed green with a settable clock read in {}",
            victim
        );
        assert!(
            finding.clock_reads.iter().any(|r| r.unit == victim),
            "{} did not appear in the report:\n{}",
            victim,
            finding
        );
        let _ = std::fs::remove_dir_all(&scratch);
    }
}

/// The single permitted exception is the report's human-readable date, it is
/// recorded with its reason, and it is the ONLY unit of this crate that reads a
/// settable clock.
///
/// Without this assertion the exclusion could widen one file at a time and
/// every test above would stay green, because an excluded unit is not scanned.
#[test]
fn the_reports_date_is_the_only_settable_clock_read_in_this_crate() {
    let (root, list) = committed_list();

    // It is excluded, and the exclusion carries a reason that says what it is.
    let excluded: Vec<&Excluded> = list
        .excluded
        .iter()
        .filter(|e| e.unit.starts_with("crates/measure/"))
        .collect();
    assert_eq!(
        excluded.len(),
        1,
        "this crate should have exactly one exclusion and has {:?}",
        excluded.iter().map(|e| &e.unit).collect::<Vec<_>>()
    );
    assert_eq!(excluded[0].unit, PERMITTED_READER);
    assert!(
        excluded[0].reason.contains("date"),
        "the exclusion's reason should say it is the date: '{}'",
        excluded[0].reason
    );

    // And now the part the exclusion cannot answer for itself: scan every unit
    // of this crate INCLUDING the excluded one, and require that the excluded
    // one is where all the clock reads are.
    let everything = AudioPathList {
        on_path: every_unit_of_this_crate(&root),
        excluded: Vec::new(),
    };
    let finding = scan(&root, &everything);
    assert!(
        !finding.clock_reads.is_empty(),
        "no unit of this crate reads a settable clock at all, so either the report stopped \
         dating itself or this scan is looking at nothing"
    );
    for read in &finding.clock_reads {
        assert_eq!(
            read.unit, PERMITTED_READER,
            "{} reads a settable clock at line {} ({}), and only {} may:\n    {}",
            read.unit, read.line, read.name, PERMITTED_READER, read.text
        );
    }
}

/// The names the scanner knows include the ones this crate could plausibly
/// reach for, so the check above is not green because it is looking for the
/// wrong words.
#[test]
fn the_scanner_knows_the_names_this_crate_could_have_used() {
    for name in ["SystemTime::now", "UNIX_EPOCH", "CLOCK_REALTIME"] {
        assert!(
            SETTABLE_CLOCK_NAMES.contains(&name),
            "the scanner does not look for {}",
            name
        );
    }
}

/// Where the harness DOES take elapsed time, it takes it from this
/// repository's monotonic timeline rather than from anything of its own.
#[test]
fn elapsed_time_in_the_harness_comes_from_the_monotonic_timeline() {
    let root = repository_root();
    let mut users = Vec::new();
    for unit in every_unit_of_this_crate(&root) {
        let source = std::fs::read_to_string(root.join(&unit)).unwrap();
        if source.contains("MonotonicTimeline::new()") {
            users.push(unit);
        }
    }
    assert!(
        users.len() >= 2,
        "the harness takes elapsed time in the capture path and in the analysis entry point, \
         and only {:?} use the monotonic timeline",
        users
    );

    // The timeline itself only goes forward, which is the property everything
    // above rests on.
    let timeline = chorus_audio::MonotonicTimeline::new();
    let mut last = 0u64;
    for _ in 0..10_000 {
        let now = timeline.now_ns();
        assert!(now >= last, "{} < {}", now, last);
        last = now;
    }
}

/// Copy the source tree into a scratch directory so a demonstration can mutate
/// it without touching the working tree.
fn scratch_copy(root: &Path, name: &str) -> PathBuf {
    let mut scratch = std::env::temp_dir();
    scratch.push(format!(
        "chorus-measure-clock-{}-{}",
        name.replace('/', "-"),
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    copy_tree(&root.join("crates"), &scratch.join("crates"));
    scratch
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let path = entry.path();
        let target = to.join(entry.file_name());
        if path.is_dir() {
            if entry.file_name() == "target" {
                continue;
            }
            copy_tree(&path, &target);
        } else {
            std::fs::copy(&path, &target).unwrap();
        }
    }
}
