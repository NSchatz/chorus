// Every refusal path, and the exit code each one carries.
//
// The gate run over the landed tree cannot grade any of this: a compliant tree
// exits zero whether or not the counter reads a raw string correctly, and it
// has no unreadable file in it to be refused about. So each path gets a tree
// built for it here, and two of them are driven through the compiled binary so
// the exit code is the one a shell would see.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use chorus_comment_density::count::Config;
use chorus_comment_density::record;
use chorus_comment_density::sweep::{walked_rust_files, Sweep, Verdict};
use chorus_comment_density::{
    EXIT_CODES, EXIT_EMPTY_SWEEP, EXIT_OK, EXIT_OVER_CEILING, EXIT_UNREADABLE, EXIT_UNTOKENIZABLE,
};

fn config(ceiling: u32, warn: u32, minimum: usize) -> Config {
    Config {
        ceiling_percent: ceiling,
        warn_percent: warn,
        minimum_counted_lines: minimum,
        directive_prefixes: Vec::new(),
        generated_markers: Vec::new(),
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the crate sits two levels under the repository root")
        .join("target/comment-density-scratch")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("a scratch tree can be made under target/");
    dir
}

fn write(dir: &Path, name: &str, body: &str) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("the parent directory can be made");
    }
    fs::write(path, body).expect("the fixture can be written");
}

fn sweep_of(dir: &Path, cfg: &Config) -> Sweep {
    Sweep::run(dir, walked_rust_files(dir).expect("the tree can be walked"), cfg)
}

fn narrated(lines: usize) -> String {
    "// why\n".repeat(lines)
}

#[test]
fn an_untokenizable_file_is_named_with_its_byte_offset_and_is_not_a_zero() {
    let cfg = config(50, 40, 30);
    let dir = scratch("untokenizable");
    write(&dir, "broken.rs", "fn f() {}\n/* this block never closes\n");
    let sweep = sweep_of(&dir, &cfg);
    let report = sweep.render(&cfg);

    assert!(report.contains("UNTOKENIZABLE broken.rs byte 10:"), "{report}");
    assert!(!report.contains("FILE broken.rs"), "it is not reported as measured: {report}");
    assert_eq!(sweep.verdict(&cfg), Verdict::Untokenizable(vec!["broken.rs".to_string()]));
    assert_eq!(sweep.verdict(&cfg).exit_code(), EXIT_UNTOKENIZABLE);
}

#[test]
fn an_unterminated_raw_string_is_untokenizable_too() {
    let cfg = config(50, 40, 30);
    let dir = scratch("unterminated-raw");
    write(&dir, "broken.rs", "fn f() { let s = r#\"never closed;\n }\n");
    let sweep = sweep_of(&dir, &cfg);
    assert_eq!(sweep.verdict(&cfg).exit_code(), EXIT_UNTOKENIZABLE);
    assert!(sweep.render(&cfg).contains("unterminated raw string literal"));
}

#[cfg(unix)]
#[test]
fn a_file_that_cannot_be_read_is_named_and_never_skipped() {
    let cfg = config(50, 40, 30);
    let dir = scratch("unreadable");
    write(&dir, "fine.rs", "fn f() {}\n");
    std::os::unix::fs::symlink("no-such-target", dir.join("gone.rs"))
        .expect("a dangling symlink can be made");

    let paths = walked_rust_files(&dir).expect("the tree can be walked");
    assert!(paths.contains(&"gone.rs".to_string()), "a dangling .rs is swept, not skipped");

    let sweep = sweep_of(&dir, &cfg);
    assert!(sweep.render(&cfg).contains("UNREADABLE gone.rs"));
    assert_eq!(sweep.verdict(&cfg), Verdict::Unreadable(vec!["gone.rs".to_string()]));
    assert_eq!(sweep.verdict(&cfg).exit_code(), EXIT_UNREADABLE);
}

#[test]
fn a_sweep_that_matched_nothing_refuses_rather_than_reporting_a_compliant_tree() {
    let cfg = config(50, 40, 30);
    let dir = scratch("empty-sweep");
    write(&dir, "README.md", "no rust here\n");
    let sweep = sweep_of(&dir, &cfg);
    assert_eq!(sweep.verdict(&cfg), Verdict::EmptySweep);
    assert_eq!(sweep.verdict(&cfg).exit_code(), EXIT_EMPTY_SWEEP);
    assert!(sweep.render(&cfg).contains("STOPPED LOOKING"));
}

#[test]
fn a_file_below_the_minimum_leaves_the_ceiling_and_stays_in_the_report() {
    let cfg = config(50, 40, 30);
    let dir = scratch("below-minimum");
    write(&dir, "tiny.rs", "// why\n// why\n// why\nfn f() {}\n");
    let sweep = sweep_of(&dir, &cfg);
    let report = sweep.render(&cfg);

    assert!(report.contains("FILE tiny.rs prose=3 code=1 ratio=75.0%"), "{report}");
    assert!(report.contains("BELOW-MINIMUM tiny.rs counted=4"), "{report}");
    assert!(
        report.contains("below the 30-line minimum and excluded from the ceiling test: 1 file(s)"),
        "{report}"
    );
    assert_eq!(sweep.verdict(&cfg), Verdict::Clean, "75% is not judged below the minimum");
}

#[test]
fn a_file_over_the_ceiling_fails_and_a_file_in_the_warn_band_does_not() {
    let cfg = config(50, 40, 30);
    let dir = scratch("ceiling");
    write(&dir, "over.rs", &format!("{}{}", narrated(21), "fn f() {}\n".repeat(19)));
    write(&dir, "warned.rs", &format!("{}{}", narrated(18), "fn f() {}\n".repeat(22)));
    let sweep = sweep_of(&dir, &cfg);
    let report = sweep.render(&cfg);

    assert!(report.contains("OVER over.rs ratio=52.5% is over the 50% ceiling"), "{report}");
    assert!(report.contains("WARN warned.rs ratio=45.0% is in the warn band"), "{report}");
    assert_eq!(sweep.verdict(&cfg), Verdict::OverCeiling(vec!["over.rs".to_string()]));
    assert_eq!(sweep.verdict(&cfg).exit_code(), EXIT_OVER_CEILING);
}

#[test]
fn a_tree_the_ceiling_holds_over_exits_zero() {
    let cfg = config(50, 40, 30);
    let dir = scratch("clean");
    write(&dir, "fine.rs", &format!("{}{}", narrated(5), "fn f() {}\n".repeat(35)));
    let sweep = sweep_of(&dir, &cfg);
    assert_eq!(sweep.verdict(&cfg), Verdict::Clean);
    assert_eq!(sweep.verdict(&cfg).exit_code(), EXIT_OK);
}

#[test]
fn every_exit_code_is_distinct_and_carries_what_it_means() {
    let mut seen = BTreeSet::new();
    for (code, meaning) in EXIT_CODES {
        assert!(seen.insert(*code), "exit code {code} is used for two failure modes");
        assert!(!meaning.is_empty(), "exit code {code} says nothing about what it means");
        assert!(*code >= 0 && *code != 1, "1 is what a panic uses; {code} must not be it");
    }
    assert_eq!(seen.len(), EXIT_CODES.len());
    for verdict in [
        Verdict::Clean,
        Verdict::OverCeiling(Vec::new()),
        Verdict::Untokenizable(Vec::new()),
        Verdict::Unreadable(Vec::new()),
        Verdict::EmptySweep,
    ] {
        let code = verdict.exit_code();
        assert!(
            EXIT_CODES.iter().any(|(listed, _)| *listed == code),
            "{verdict:?} exits {code}, which the documented table does not carry"
        );
    }
}

// --- the record and the gate have to agree -----------------------------------

fn two_file_sweep(name: &str) -> (PathBuf, Sweep, Config) {
    let cfg = config(50, 40, 30);
    let dir = scratch(name);
    write(&dir, "a.rs", &format!("{}{}", narrated(5), "fn f() {}\n".repeat(35)));
    write(&dir, "b.rs", "fn g() {}\n");
    let sweep = sweep_of(&dir, &cfg);
    (dir, sweep, cfg)
}

fn record_of(thresholds: &str, rows: &str) -> String {
    format!(
        "# a record\n\n{thresholds}\n\n{}\n\n\
         | file | prose before | code before | ratio before | prose after | code after | ratio \
         after |\n|---|---|---|---|---|---|---|\n{rows}\n{}\n",
        record::BEGIN,
        record::END
    )
}

const AGREES: &str = "- ceiling: 50%\n- warn band: 40%\n- minimum counted lines: 30";
const GOOD_ROWS: &str = "| a.rs | 5 | 35 | 12.5% | 5 | 35 | 12.5% |\n\
                         | b.rs | 0 | 1 | 0.0% | 0 | 1 | 0.0% |";

#[test]
fn a_record_that_matches_the_tree_and_the_gate_raises_nothing() {
    let (_dir, sweep, cfg) = two_file_sweep("record-agrees");
    let parsed = record::parse(&record_of(AGREES, GOOD_ROWS));
    assert_eq!(record::check(&parsed, &sweep, &cfg), Vec::<String>::new());
}

#[test]
fn a_record_that_declares_no_ceiling_is_a_disagreement() {
    let (_dir, sweep, cfg) = two_file_sweep("record-no-ceiling");
    let parsed = record::parse(&record_of("- warn band: 40%\n- minimum counted lines: 30", GOOD_ROWS));
    let found = record::check(&parsed, &sweep, &cfg);
    assert!(found.iter().any(|line| line.contains("declares no ceiling")), "{found:?}");
}

#[test]
fn a_record_that_declares_a_value_the_gate_does_not_enforce_is_a_disagreement() {
    let (_dir, sweep, cfg) = two_file_sweep("record-wrong-values");
    let parsed = record::parse(&record_of(
        "- ceiling: 44%\n- warn band: 33%\n- minimum counted lines: 22",
        GOOD_ROWS,
    ));
    let found = record::check(&parsed, &sweep, &cfg);
    assert!(found.iter().any(|line| line.contains("44% ceiling and the gate enforces 50%")));
    assert!(found.iter().any(|line| line.contains("33% warn band and the gate enforces 40%")));
    assert!(found.iter().any(|line| line.contains("22-line minimum and the gate enforces 30")));
}

#[test]
fn a_row_whose_two_code_counts_differ_is_named() {
    let (_dir, sweep, cfg) = two_file_sweep("record-code-moved");
    let rows = "| a.rs | 5 | 34 | 12.8% | 5 | 35 | 12.5% |\n| b.rs | 0 | 1 | 0.0% | 0 | 1 | 0.0% |";
    let found = record::check(&record::parse(&record_of(AGREES, rows)), &sweep, &cfg);
    assert!(
        found.iter().any(|line| line.contains("a.rs") && line.contains("moved a code token")),
        "{found:?}"
    );
}

#[test]
fn a_row_naming_a_path_the_gate_does_not_measure_is_named() {
    let (_dir, sweep, cfg) = two_file_sweep("record-unknown-path");
    let rows = format!("{GOOD_ROWS}\n| gone.rs | 1 | 1 | 50.0% | 1 | 1 | 50.0% |");
    let found = record::check(&record::parse(&record_of(AGREES, &rows)), &sweep, &cfg);
    assert!(
        found.iter().any(|line| line.contains("gone.rs") && line.contains("not a tracked .rs file")),
        "{found:?}"
    );
}

#[test]
fn a_measured_file_the_record_forgot_is_named() {
    let (_dir, sweep, cfg) = two_file_sweep("record-missing-row");
    let rows = "| a.rs | 5 | 35 | 12.5% | 5 | 35 | 12.5% |";
    let found = record::check(&record::parse(&record_of(AGREES, rows)), &sweep, &cfg);
    assert!(found.iter().any(|line| line.contains("b.rs") && line.contains("no row for it")));
}

#[test]
fn a_row_that_no_longer_matches_the_tree_is_named() {
    let (_dir, sweep, cfg) = two_file_sweep("record-stale-after");
    let rows = "| a.rs | 5 | 35 | 12.5% | 9 | 35 | 20.4% |\n| b.rs | 0 | 1 | 0.0% | 0 | 1 | 0.0% |";
    let found = record::check(&record::parse(&record_of(AGREES, rows)), &sweep, &cfg);
    assert!(
        found.iter().any(|line| line.contains("a.rs") && line.contains("the tree measures")),
        "{found:?}"
    );
}

#[test]
fn the_generated_region_is_rewritten_with_the_before_column_carried_over() {
    let (_dir, sweep, cfg) = two_file_sweep("record-rewrite");
    let stale = "| a.rs | 40 | 35 | 53.3% | 40 | 35 | 53.3% |\n\
                 | b.rs | 0 | 1 | 0.0% | 0 | 1 | 0.0% |";
    let rewritten = record::render(&record_of(AGREES, stale), &sweep, &cfg, false)
        .expect("the region is there to rewrite");
    assert!(rewritten.contains("| a.rs | 40 | 35 | 53.3% | 5 | 35 | 12.5% |"), "{rewritten}");
    assert_eq!(record::check(&record::parse(&rewritten), &sweep, &cfg), Vec::<String>::new());
}

// --- and the same codes, out of the compiled binary --------------------------

fn git_tree(name: &str) -> PathBuf {
    let dir = scratch(name);
    write(&dir, "tools/comment-density-directives.txt", "# empty on purpose\n");
    write(&dir, "tools/comment-density-generated-markers.txt", "# empty on purpose\n");
    let status = Command::new("git")
        .arg("-C")
        .arg(&dir)
        .args(["init", "-q"])
        .status()
        .expect("git runs; the gate lists what is tracked with it");
    assert!(status.success(), "git init failed in the scratch tree");
    dir
}

fn add_and_run(dir: &Path) -> std::process::Output {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["add", "-A"])
        .status()
        .expect("git add runs");
    assert!(status.success(), "git add failed in the scratch tree");
    Command::new(env!("CARGO_BIN_EXE_chorus-comment-density"))
        .args(["gate", "--root"])
        .arg(dir)
        .output()
        .expect("the gate binary runs")
}

#[test]
fn the_binary_refuses_a_repository_whose_sweep_matched_nothing() {
    let dir = git_tree("binary-empty");
    let output = add_and_run(&dir);
    assert_eq!(output.status.code(), Some(EXIT_EMPTY_SWEEP));
    let said = String::from_utf8_lossy(&output.stdout);
    assert!(said.contains("STOPPED LOOKING"), "{said}");
    assert!(!said.lines().any(|line| line.starts_with("pass ")), "nothing reads as satisfied");
}

#[test]
fn the_binary_exits_over_ceiling_on_a_tracked_file_over_the_ceiling() {
    let dir = git_tree("binary-over");
    write(&dir, "narrated.rs", &narrated(40));
    let output = add_and_run(&dir);
    assert_eq!(output.status.code(), Some(EXIT_OVER_CEILING));
    let said = String::from_utf8_lossy(&output.stdout);
    assert!(said.contains("OVER narrated.rs"), "{said}");
}
