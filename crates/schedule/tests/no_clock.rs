//! This crate reads no clock of any kind, and `audio-path.conf` accounts for
//! every unit of it.
//!
//! Civil time is allowed for scheduling (K30), but it arrives as an argument:
//! the server reads its clock, this crate does arithmetic. So the rule here is
//! stricter than the audio path's: no settable clock (the vocabulary is
//! `crates/audio-path`'s own, not a second list that could drift) and no
//! monotonic one either.

use std::path::{Path, PathBuf};

use chorus_audio_path::list::{AudioPathList, LIST_FILE};
use chorus_audio_path::scan::settable_clock_in;

/// The monotonic spellings, beside the audio path's settable ones.
const MONOTONIC: &[&str] = &[
    "Instant::now",
    "Instant",
    "CLOCK_MONOTONIC",
    "clock_gettime",
    "std::time",
];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn units(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let p = entry.path();
        if p.is_dir() {
            units(&p, out);
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push(p);
        }
    }
}

fn every_unit() -> Vec<PathBuf> {
    let mut out = Vec::new();
    units(&root().join("crates/schedule/src"), &mut out);
    out.sort();
    out
}

/// The clock a line of code reads, if any.
fn clock_in(line: &str) -> Option<&'static str> {
    let code = line.split("//").next().unwrap_or("");
    settable_clock_in(code).or_else(|| MONOTONIC.iter().find(|m| code.contains(*m)).copied())
}

#[test]
fn the_check_sees_each_kind_of_clock() {
    // The red demonstrations: each spelling a unit could read a clock with.
    for line in [
        "let now = std::time::SystemTime::now();",
        "use std::time::{SystemTime as Wall};",
        "let t = Instant::now();",
        "use std::time::Instant;",
        "clock_gettime(CLOCK_MONOTONIC, &mut ts);",
    ] {
        assert!(clock_in(line).is_some(), "{line}");
    }
    assert_eq!(
        clock_in("let at = zone.instant_of(day, sod); // not Instant::now"),
        None
    );
}

#[test]
fn no_unit_reads_a_clock() {
    let all = every_unit();
    assert!(
        all.len() >= 10,
        "found {} units; looking in the wrong place",
        all.len()
    );
    for unit in all {
        let text = std::fs::read_to_string(&unit).unwrap();
        for (n, line) in text.lines().enumerate() {
            assert_eq!(
                clock_in(line),
                None,
                "{}:{}: {}",
                unit.display(),
                n + 1,
                line
            );
        }
    }
}

#[test]
fn the_audio_path_list_accounts_for_every_unit() {
    let text = std::fs::read_to_string(root().join(LIST_FILE)).unwrap();
    let list = AudioPathList::parse(&text).unwrap();
    for unit in every_unit() {
        let rel = unit
            .strip_prefix(root())
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        assert!(
            list.accounts_for(&rel),
            "{rel} is neither listed nor excluded in {LIST_FILE}"
        );
    }
}
