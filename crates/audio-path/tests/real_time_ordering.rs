//! The CPU-time-bound ordering check, run against this repository.
//!
//! `sched(7)` says an unyielding real-time thread can starve every other
//! thread on the machine, and names `RLIMIT_RTTIME` as the bound on that. A
//! bound applied after the policy leaves the window open; a bound applied
//! before it never opens one. That ordering held in this tree before anything
//! checked it, and `docs/verification-record.md` claimed a check that did not
//! exist. This file is that check.
//!
//! It needs no sound device and no elevated privilege: it reads source text.
//! So it runs in the ordinary suite, on every change, like the audio-path
//! checks beside it.
//!
//! The three demonstrations at the bottom are committed rather than described,
//! for the reason `docs/decisions/0011` already gives about the audio-path
//! ones: a branch proves a check is not vacuous once, for whoever was told the
//! branch name, and a committed test proves it on every run for everyone.

use std::path::{Path, PathBuf};

use chorus_audio_path::realtime::{scan_real_time, AcquisitionList, SITE_FILE};
use chorus_audio_path::scan::repository_root;

/// The two acquisitions the invariant in `docs/verification-record.md` is
/// about: the library entry point every server thread goes through, and the
/// spin binary that takes a policy directly.
const THE_TWO_REAL_ACQUISITIONS: &[&str] = &[
    "crates/server/src/hostreport.rs",
    "crates/server/src/bin/chorus-rt-spin.rs",
];

fn committed_list() -> (PathBuf, AcquisitionList) {
    let root = repository_root();
    let text = std::fs::read_to_string(root.join(SITE_FILE))
        .unwrap_or_else(|e| panic!("{} is committed and readable: {}", SITE_FILE, e));
    let list = AcquisitionList::parse(&text)
        .unwrap_or_else(|e| panic!("{} does not parse: {}", SITE_FILE, e));
    (root, list)
}

#[test]
fn the_committed_tree_applies_the_cpu_time_bound_before_every_real_time_acquisition() {
    let (root, list) = committed_list();
    let finding = scan_real_time(&root, &list);
    assert!(finding.ok(), "{}", finding);

    let report = finding.to_string();
    println!("{}", report.trim_end());
    assert!(
        report.contains(&format!(
            "{} real-time acquisition sites examined",
            finding.sites_examined()
        )),
        "the report has to say how many sites it looked at, so that a green run is a statement \
         about a number and not about nothing:\n{}",
        report
    );
    assert!(
        finding.sites_examined() >= THE_TWO_REAL_ACQUISITIONS.len(),
        "the check examined {} sites, which is fewer than the acquisitions this tree is known to \
         have:\n{}",
        finding.sites_examined(),
        finding
    );
}

#[test]
fn both_real_acquisitions_are_among_the_sites_the_check_examined() {
    let (root, list) = committed_list();
    let finding = scan_real_time(&root, &list);
    for unit in THE_TWO_REAL_ACQUISITIONS {
        assert!(
            finding.holding.iter().any(|s| s.unit == *unit),
            "{} takes a real-time policy and the check did not grade it as holding:\n{}",
            unit,
            finding
        );
    }
}

/// The first demonstration: a real acquisition that asks for the scheduling
/// policy before it applies the CPU-time bound turns the check red, and the
/// report names the site and its line.
///
/// The mutation is a swap of two statements in the spin binary, which is a
/// real acquisition rather than an invented one, so what goes red is the thing
/// the invariant is about.
#[test]
fn taking_the_policy_before_the_bound_turns_it_red_and_names_the_site() {
    let (root, list) = committed_list();
    let scratch = scratch_copy(&root, "policy-before-bound");
    let victim = "crates/server/src/bin/chorus-rt-spin.rs";

    let path = scratch.join(victim);
    let source = std::fs::read_to_string(&path).unwrap();
    let bound_statement = "let applied = match bound_real_time_cpu_time(rttime_us) {";
    let policy_statement = "let grant = match take_real_time_policy(rt_priority) {";
    assert!(source.contains(bound_statement) && source.contains(policy_statement));
    // Swap the two through a placeholder, so the acquisition ends up above the
    // bound and both bindings are still declared before they are used.
    let swapped = source
        .replace(bound_statement, "\u{0}PLACEHOLDER\u{0}")
        .replace(policy_statement, bound_statement)
        .replace("\u{0}PLACEHOLDER\u{0}", policy_statement);
    std::fs::write(&path, &swapped).unwrap();

    let finding = scan_real_time(&scratch, &list);
    let _ = std::fs::remove_dir_all(&scratch);

    assert!(
        !finding.ok(),
        "the check stayed green with a real-time policy taken before the CPU-time bound:\n{}",
        finding
    );
    let offender = finding
        .unbounded
        .iter()
        .find(|s| s.unit == victim)
        .unwrap_or_else(|| panic!("{} was not reported as unbounded:\n{}", victim, finding));
    assert!(offender.line > 0, "the finding carries a line number");
    assert_eq!(
        offender.bounded_at, None,
        "no bound precedes the acquisition in that function any more"
    );
    let bound_after = offender
        .bound_after_at
        .expect("the bound is still there, below the acquisition now");
    assert!(
        bound_after > offender.line,
        "the swap put the bound at line {}, which is not after the acquisition at {}",
        bound_after,
        offender.line
    );
    let report = finding.to_string();
    assert!(
        report.contains(&format!("{}:{}", victim, offender.line)),
        "the report names the offending site and its line:\n{}",
        report
    );
}

/// The second demonstration: an acquisition in a unit the committed list does
/// not name is reported as unaccounted for, rather than the invariant being
/// reported as holding.
///
/// The smuggled unit applies the bound first, so the ONLY thing wrong with it
/// is that nobody wrote it down. A check that graded only the sites it was
/// told about would call this tree green.
#[test]
fn an_acquisition_in_an_unlisted_unit_is_reported_as_unaccounted_for() {
    let (root, list) = committed_list();
    let scratch = scratch_copy(&root, "unlisted-acquisition");
    let smuggled = "crates/audio/src/smuggled_real_time.rs";

    std::fs::write(
        scratch.join(smuggled),
        concat!(
            "//! Introduced by the real-time demonstration: an acquisition in a unit\n",
            "//! the committed list does not name. It bounds itself first, so the only\n",
            "//! thing wrong with it is that nobody wrote it down.\n",
            "pub fn take_the_contract() {\n",
            "    let _ = chorus_hostctl::bound_real_time_cpu_time(200_000);\n",
            "    let _ = chorus_hostctl::take_real_time_policy(20);\n",
            "}\n",
        ),
    )
    .unwrap();

    let finding = scan_real_time(&scratch, &list);
    let _ = std::fs::remove_dir_all(&scratch);

    assert!(
        !finding.ok(),
        "the check reported the invariant as holding with an acquisition nobody accounted \
         for:\n{}",
        finding
    );
    assert!(
        finding.unaccounted.iter().any(|s| s.unit == smuggled),
        "{} was not named as unaccounted for:\n{}",
        smuggled,
        finding
    );
    assert!(
        finding.unbounded.is_empty(),
        "the smuggled site bounds itself first; the finding against it is the accounting one \
         alone:\n{}",
        finding
    );
    assert!(
        finding.to_string().contains(smuggled),
        "the report names the site:\n{}",
        finding
    );
}

/// The third demonstration: the names this check looks for, occurring only in
/// a line comment, a doc comment and a string literal, produce no finding.
///
/// This repository quotes the very names it searches for - in doc comments, in
/// error messages, and in this check's own constants - so a scanner that read
/// prose as code would fire on its own documentation. The committed tree
/// passing at all is the standing evidence; this makes it deliberate.
#[test]
fn the_names_occurring_only_in_prose_produce_no_finding() {
    let (root, list) = committed_list();
    let before = scan_real_time(&root, &list);
    assert!(before.ok(), "{}", before);

    let scratch = scratch_copy(&root, "prose-only");
    let listed = scratch.join("crates/server/src/hostreport.rs");
    let mut source = std::fs::read_to_string(&listed).unwrap();
    source.push_str(concat!(
        "\n",
        "/// Introduced by the real-time demonstration. A doc comment naming\n",
        "/// take_real_time_policy(20) with no bound anywhere near it.\n",
        "pub fn prose_only() -> &'static str {\n",
        "    // A line comment naming take_real_time_policy(20), same as above.\n",
        "    let quoted = \"take_real_time_policy(20)\";\n",
        "    let raw = r#\"take_real_time_policy(20)\"#;\n",
        "    let _ = raw;\n",
        "    quoted\n",
        "}\n",
    ));
    std::fs::write(&listed, source).unwrap();

    let after = scan_real_time(&scratch, &list);
    let _ = std::fs::remove_dir_all(&scratch);

    assert!(
        after.ok(),
        "the check fired on names that occur only in prose:\n{}",
        after
    );
    assert_eq!(
        after.sites_examined(),
        before.sites_examined(),
        "a doc comment, a line comment and two string literals added a site:\n{}",
        after
    );
}

/// Copy the source tree into a scratch directory so a demonstration can mutate
/// it without touching the working tree.
fn scratch_copy(root: &Path, name: &str) -> PathBuf {
    let mut scratch = std::env::temp_dir();
    scratch.push(format!("chorus-real-time-{}-{}", name, std::process::id()));
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
