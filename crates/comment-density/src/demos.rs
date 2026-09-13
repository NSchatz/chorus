// The committed counterexamples, and the requirement that each one still
// produces what it demonstrates.
//
// A check that has never been seen to fail is a formality rather than evidence.
// The one that matters most here is comment-like-text-in-strings: it requires
// the string-heavy file to report prose=0, so a counter that went back to
// matching quote characters is caught by the demonstration rather than by
// somebody noticing a file was gutted.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use crate::count::Config;
use crate::sweep::{walked_rust_files, Sweep, DEMONSTRATIONS};

pub struct Demonstration {
    pub name: &'static str,
    pub shape: &'static str,
    pub wanted: i32,
    pub must_name: &'static [&'static str],
}

pub const TREES: &[Demonstration] = &[
    Demonstration {
        name: "a-file-over-the-ceiling",
        shape: "a file whose own comments put it over the ceiling",
        wanted: crate::EXIT_OVER_CEILING,
        must_name: &["OVER narrated.rs", "is over the"],
    },
    Demonstration {
        name: "comment-like-text-in-strings",
        shape: "comment-like text inside strings, raw strings, byte strings and characters",
        wanted: crate::EXIT_OVER_CEILING,
        must_name: &["FILE strings.rs prose=0 code=", "OVER narrated.rs"],
    },
    Demonstration {
        name: "a-file-that-cannot-be-tokenized",
        shape: "a file whose tokens run off the end of it",
        wanted: crate::EXIT_UNTOKENIZABLE,
        must_name: &["UNTOKENIZABLE unterminated.rs byte "],
    },
    Demonstration {
        name: "a-file-that-cannot-be-read",
        shape: "a tracked .rs path that cannot be opened",
        wanted: crate::EXIT_UNREADABLE,
        must_name: &["UNREADABLE gone.rs"],
    },
    Demonstration {
        name: "an-empty-sweep",
        shape: "a tree in which the category matches nothing at all",
        wanted: crate::EXIT_EMPTY_SWEEP,
        must_name: &["STOPPED LOOKING"],
    },
    Demonstration {
        name: "a-tree-that-passes",
        shape: "a documented file in the warn band beside a file under the minimum",
        wanted: crate::EXIT_OK,
        must_name: &["FILE documented.rs", "BELOW-MINIMUM tiny.rs", "WARN documented.rs"],
    },
];

pub fn scan(root: &Path, cfg: &Config) -> (String, Vec<String>) {
    let mut out = String::new();
    let mut failures = Vec::new();
    let dir = root.join(DEMONSTRATIONS);

    for demonstration in TREES {
        let tree = dir.join(demonstration.name);
        if !tree.is_dir() {
            failures.push(format!(
                "{}: no such tree under {DEMONSTRATIONS}",
                demonstration.name
            ));
            continue;
        }
        let paths = match walked_rust_files(&tree) {
            Ok(paths) => paths,
            Err(error) => {
                failures.push(format!("{}: could not be walked ({error})", demonstration.name));
                continue;
            }
        };
        let sweep = Sweep::run(&tree, paths, cfg);
        let verdict = sweep.verdict(cfg);
        let code = verdict.exit_code();
        let report = sweep.render(cfg);

        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "--- {} ({}, exit {}, wanted {})",
            demonstration.name, demonstration.shape, code, demonstration.wanted
        );
        for line in report.lines() {
            let _ = writeln!(out, "    {line}");
        }

        let mut held = true;
        if code != demonstration.wanted {
            failures.push(format!(
                "{} exited {} and {} was wanted; a demonstration that does not produce what it \
                 demonstrates is not a demonstration",
                demonstration.name, code, demonstration.wanted
            ));
            held = false;
        }
        for wanted in demonstration.must_name {
            if !report.contains(wanted) {
                failures.push(format!(
                    "{} did not name {:?}, so the report no longer shows what the tree \
                     demonstrates",
                    demonstration.name, wanted
                ));
                held = false;
            }
        }
        if held {
            let _ = writeln!(out, "pass {} produced what it demonstrates", demonstration.name);
        }
    }

    // A tree added to the directory and never run here would make this
    // meta-check quietly incomplete, which is the exact failure it exists to
    // prevent. Derived from the directory rather than restated.
    let _ = writeln!(out);
    let _ = writeln!(out, "--- every committed demonstration is exercised above");
    match fs::read_dir(&dir) {
        Err(error) => failures.push(format!("{DEMONSTRATIONS} could not be listed ({error})")),
        Ok(entries) => {
            let mut found: Vec<String> = Vec::new();
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    found.push(entry.file_name().to_string_lossy().to_string());
                }
            }
            found.sort();
            let missed: Vec<&String> = found
                .iter()
                .filter(|name| !TREES.iter().any(|tree| tree.name == name.as_str()))
                .collect();
            if missed.is_empty() {
                let _ = writeln!(
                    out,
                    "pass all {} committed demonstrations are run above",
                    found.len()
                );
            } else {
                for name in missed {
                    failures.push(format!(
                        "{name} is committed under {DEMONSTRATIONS} and is not run here"
                    ));
                }
            }
        }
    }
    (out, failures)
}
