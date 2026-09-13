// F1: a leading licence line swallows every comment below it, down to the first
// code token, so a doc block that the counting stance calls prose is counted at
// zero and the file it is in can leave the ceiling test altogether.
//
// The stance excludes "a leading comment block that opens with an SPDX licence
// identifier or a copyright line" from the prose count, because a licence
// header is an obligation rather than prose an author chose. count() reads that
// block as every comment lexeme before the first code token, however many
// blocks, blank lines and comment forms lie between. So one prepended SPDX line
// removes an arbitrarily large //! documentation block from the count, which is
// the same loophole the stance closes when it makes doc comments prose so that
// relabelling // as /// is not a way past the ceiling.
//
// Both assertions below are the criteria, not a preference:
//
//   criterion 3  "WHEN a file contains nested block comments or doc comments in
//                any of the four forms THE SYSTEM SHALL count every line they
//                occupy as prose"
//   criterion 7  "WHEN a file's counted lines fall below the declared minimum
//                THE SYSTEM SHALL exclude it from the ceiling test" - which is
//                how the swallowed file escapes the gate entirely, since the
//                lines it lost were counted lines

use std::fs;
use std::path::{Path, PathBuf};

use chorus_comment_density::count::{count, Config};
use chorus_comment_density::sweep::{walked_rust_files, Sweep};
use chorus_comment_density::EXIT_OVER_CEILING;

fn config() -> Config {
    Config {
        ceiling_percent: 50,
        warn_percent: 40,
        minimum_counted_lines: 30,
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

#[test]
fn a_licence_line_does_not_swallow_the_doc_block_below_it() {
    let body = format!(
        "// SPDX-License-Identifier: MIT\n\n{}\n{}",
        "//! module documentation, which the stance counts as prose\n".repeat(20),
        "fn f() {}\n".repeat(40)
    );
    let counts = count(&body, &config()).expect("the fixture tokenizes");

    assert_eq!(counts.header_lines, 1, "the header is the licence line and nothing under it");
    assert_eq!(
        counts.prose, 20,
        "every line an inner doc comment occupies is prose, licence header or not"
    );
    assert_eq!(counts.code, 40);
}

#[test]
fn a_licence_line_does_not_hide_a_file_from_the_ceiling() {
    let cfg = config();
    let dir = scratch("licence-swallows-the-block");
    let path = dir.join("narrated.rs");
    fs::write(
        &path,
        format!(
            "// SPDX-License-Identifier: MIT\n\n{}{}",
            "// why\n".repeat(40),
            "fn f() {}\n".repeat(10)
        ),
    )
    .expect("the fixture can be written");

    let sweep = Sweep::run(&dir, walked_rust_files(&dir).expect("the tree can be walked"), &cfg);
    let report = sweep.render(&cfg);

    assert!(
        !report.contains("BELOW-MINIMUM narrated.rs"),
        "fifty counted lines are not below a thirty-line minimum: {report}"
    );
    assert!(
        report.contains("OVER narrated.rs"),
        "forty narration lines under one licence line are over a 50% ceiling: {report}"
    );
    assert_eq!(sweep.verdict(&cfg).exit_code(), EXIT_OVER_CEILING, "{report}");
}
