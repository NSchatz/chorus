//! The two checks, run against this repository.
//!
//! These are the assertions the phase's clock guardrail rests on, so they run
//! in the ordinary suite, on every change, with no environment at all.

use std::path::Path;

use chorus_audio_path::list::{AudioPathList, LIST_FILE};
use chorus_audio_path::scan::{repository_root, scan};

fn committed_list() -> (std::path::PathBuf, AudioPathList) {
    let root = repository_root();
    let text = std::fs::read_to_string(root.join(LIST_FILE))
        .unwrap_or_else(|e| panic!("{} is committed and readable: {}", LIST_FILE, e));
    let list = AudioPathList::parse(&text)
        .unwrap_or_else(|e| panic!("{} does not parse: {}", LIST_FILE, e));
    (root, list)
}

#[test]
fn the_committed_list_names_something() {
    let (_, list) = committed_list();
    assert!(
        list.on_path.len() >= 10,
        "the audio path is bigger than {} units",
        list.on_path.len()
    );
    assert!(
        list.excluded
            .iter()
            .any(|e| e.unit == "crates/client-linux/src/delaylog.rs"),
        "the delay-log writer is the exclusion this phase names, and it has to be recorded"
    );
}

#[test]
fn no_unit_on_the_audio_path_reads_a_settable_wall_clock() {
    let (root, list) = committed_list();
    let finding = scan(&root, &list);
    assert!(
        finding.clock_reads.is_empty(),
        "a unit on the audio or timestamp path reads a settable clock:\n{}",
        finding
    );
}

#[test]
fn the_list_accounts_for_every_unit_the_listed_ones_reach() {
    let (root, list) = committed_list();
    let finding = scan(&root, &list);
    assert!(
        finding.missing.is_empty(),
        "the list has a hole in it:\n{}",
        finding
    );
    assert!(
        finding.absent.is_empty(),
        "the list names a file that is not in the tree:\n{}",
        finding
    );
}

#[test]
fn the_committed_tree_passes_both_checks() {
    let (root, list) = committed_list();
    let finding = scan(&root, &list);
    assert!(finding.ok(), "{}", finding);
    assert!(finding.units_reached >= 10);
}

/// The first demonstration the criterion asks for: one settable clock read in
/// a listed unit turns the check red.
///
/// Run against a copy of the tree with the read introduced, rather than
/// against a branch, so it is committed evidence a later reader can re-run
/// instead of a branch name they have to be told about.
#[test]
fn introducing_one_settable_clock_read_into_a_listed_unit_turns_it_red() {
    let (root, list) = committed_list();
    let scratch = scratch_copy(&root, "clock-read");

    let victim = scratch.join("crates/audio/src/chunker.rs");
    let mut source = std::fs::read_to_string(&victim).unwrap();
    source.push_str(
        "\n\
         /// Introduced by the audio-path demonstration. This is the read the\n\
         /// check exists to catch.\n\
         pub fn demonstration_wall_clock() -> u64 {\n\
         \x20   std::time::SystemTime::now()\n\
         \x20       .duration_since(std::time::UNIX_EPOCH)\n\
         \x20       .map(|d| d.as_secs())\n\
         \x20       .unwrap_or(0)\n\
         }\n",
    );
    std::fs::write(&victim, source).unwrap();

    let finding = scan(&scratch, &list);
    assert!(
        !finding.ok(),
        "the check stayed green with a settable clock read on the path"
    );
    assert!(
        finding
            .clock_reads
            .iter()
            .any(|r| r.unit == "crates/audio/src/chunker.rs"),
        "{}",
        finding
    );
    let _ = std::fs::remove_dir_all(&scratch);
}

/// The second demonstration: a first-party unit added under a listed one, and
/// neither listed nor excluded, turns the check red.
#[test]
fn adding_an_unlisted_unit_under_a_listed_one_turns_it_red() {
    let (root, list) = committed_list();
    let scratch = scratch_copy(&root, "unlisted-unit");

    std::fs::write(
        scratch.join("crates/audio/src/smuggled.rs"),
        "//! Introduced by the audio-path demonstration: a unit under a listed\n\
         //! one that the list does not account for.\n\
         pub fn now() -> u64 {\n\
         \x20   std::time::SystemTime::now()\n\
         \x20       .duration_since(std::time::UNIX_EPOCH)\n\
         \x20       .map(|d| d.as_secs())\n\
         \x20       .unwrap_or(0)\n\
         }\n",
    )
    .unwrap();
    let lib = scratch.join("crates/audio/src/lib.rs");
    let mut source = std::fs::read_to_string(&lib).unwrap();
    source.push_str("\npub mod smuggled;\n");
    std::fs::write(&lib, source).unwrap();

    let finding = scan(&scratch, &list);
    assert!(
        !finding.ok(),
        "the check stayed green with an unaccounted-for unit under a listed one"
    );
    assert!(
        finding
            .missing
            .iter()
            .any(|m| m.unit == "crates/audio/src/smuggled.rs"),
        "{}",
        finding
    );
    let _ = std::fs::remove_dir_all(&scratch);
}

/// Copy the source tree into a scratch directory so a demonstration can mutate
/// it without touching the working tree.
fn scratch_copy(root: &Path, name: &str) -> std::path::PathBuf {
    let mut scratch = std::env::temp_dir();
    scratch.push(format!("chorus-audio-path-{}-{}", name, std::process::id()));
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
