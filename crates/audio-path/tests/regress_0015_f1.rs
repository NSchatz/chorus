//! Regress artifact written by the impl gate of `S0015-chorus-sound-2`,
//! finding F1. It is not a fix; it documents a hole in the completeness half
//! of the audio-path check, so that the fix has something to turn green.
//!
//! AC-23 of that spec: "WHEN the repository's test suite runs THE SYSTEM SHALL
//! fail ... IF that list omits a first-party unit which the listed units
//! depend on and which is not recorded in the same file as an exclusion with a
//! reason."
//!
//! `crates/audio-path/src/scan.rs` finds a module declaration with
//!
//! ```text
//! line.strip_prefix("mod ").or_else(|| line.strip_prefix("pub mod "))
//! ```
//!
//! so it sees `mod x;` and `pub mod x;` and nothing else. A module declared
//! with any restricted visibility - `pub(crate) mod x;`, `pub(super) mod x;`,
//! `pub(in path) mod x;`, all ordinary Rust - is invisible to it: the unit is
//! neither followed (so it is never scanned for a settable clock) nor reported
//! as missing from the list. The list can therefore be made incomplete, and a
//! settable wall-clock read can be put on the audio path, by two words, with
//! the suite staying green.
//!
//! The committed demonstration
//! `tests/audio_path.rs::adding_an_unlisted_unit_under_a_listed_one_turns_it_red`
//! uses `pub mod`, which is the one spelling the scanner does see, so it does
//! not cover this.
//!
//! Both cases below are run against a scratch copy of the tree, exactly as the
//! committed demonstrations are, so nothing here touches the working tree.

use std::path::{Path, PathBuf};

use chorus_audio_path::list::{AudioPathList, LIST_FILE};
use chorus_audio_path::scan::{repository_root, scan};

/// The body of the smuggled unit: a first-party unit on the audio path that
/// reads the settable wall clock, which is precisely what AC-23 exists to make
/// impossible.
const SMUGGLED: &str = "\
pub fn wall_clock_seconds() -> u64 {
\x20   std::time::SystemTime::now()
\x20       .duration_since(std::time::UNIX_EPOCH)
\x20       .map(|d| d.as_secs())
\x20       .unwrap_or(0)
}
";

fn committed_list() -> (PathBuf, AudioPathList) {
    let root = repository_root();
    let text = std::fs::read_to_string(root.join(LIST_FILE))
        .unwrap_or_else(|e| panic!("{} is committed and readable: {}", LIST_FILE, e));
    let list = AudioPathList::parse(&text)
        .unwrap_or_else(|e| panic!("{} does not parse: {}", LIST_FILE, e));
    (root, list)
}

/// Copy the source tree into a scratch directory, declare a new unit under a
/// listed one with `declaration`, and return what the scan made of it.
fn scan_with_smuggled_unit(name: &str, declaration: &str) -> chorus_audio_path::scan::Finding {
    let (root, list) = committed_list();
    let scratch = scratch_copy(&root, name);

    std::fs::write(scratch.join("crates/audio/src/smuggled.rs"), SMUGGLED).unwrap();
    let lib = scratch.join("crates/audio/src/lib.rs");
    let mut source = std::fs::read_to_string(&lib).unwrap();
    source.push_str(&format!("\n{}\n", declaration));
    std::fs::write(&lib, source).unwrap();

    let finding = scan(&scratch, &list);
    let _ = std::fs::remove_dir_all(&scratch);
    finding
}

/// The control. `pub mod` is the spelling the committed demonstration uses and
/// the scanner does see, so this passes today; it is here so that a failure of
/// the case below cannot be mistaken for the scratch copy being broken.
#[test]
fn regress_0015_f1_control_a_pub_mod_declaration_is_caught() {
    let finding = scan_with_smuggled_unit("control-pub-mod", "pub mod smuggled;");
    assert!(
        !finding.ok(),
        "the control case regressed: `pub mod` no longer turns the check red"
    );
    assert!(
        finding
            .missing
            .iter()
            .any(|m| m.unit == "crates/audio/src/smuggled.rs"),
        "{}",
        finding
    );
}

/// The finding. A unit declared `pub(crate) mod` under a listed unit is
/// neither listed, nor excluded with a reason, nor reported - and the settable
/// wall-clock read inside it is never scanned for either.
#[test]
fn regress_0015_f1_a_restricted_visibility_module_is_missed_by_the_completeness_check() {
    let finding = scan_with_smuggled_unit("pub-crate-mod", "pub(crate) mod smuggled;");

    assert!(
        finding
            .missing
            .iter()
            .any(|m| m.unit == "crates/audio/src/smuggled.rs"),
        "AC-23: a first-party unit that a listed unit depends on, and that the list neither \
         names nor excludes with a reason, has to be reported. `pub(crate) mod smuggled;` under \
         the listed crates/audio/src/lib.rs was not:\n{}",
        finding
    );
    assert!(
        !finding.ok(),
        "AC-23: the suite has to fail on an incomplete list. It stayed green:\n{}",
        finding
    );
}

/// The same hole with `pub(super) mod`, to show it is the visibility syntax and
/// not one particular spelling.
#[test]
fn regress_0015_f1_pub_super_is_missed_the_same_way() {
    let finding = scan_with_smuggled_unit("pub-super-mod", "pub(super) mod smuggled;");
    assert!(
        !finding.ok(),
        "AC-23: `pub(super) mod smuggled;` under a listed unit left the check green:\n{}",
        finding
    );
}

fn scratch_copy(root: &Path, name: &str) -> PathBuf {
    let mut scratch = std::env::temp_dir();
    scratch.push(format!("chorus-regress-0015-f1-{}-{}", name, std::process::id()));
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
