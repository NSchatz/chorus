//! Run the committed low-latency scenarios and write their report.
//!
//!   cargo run --release -p chorus-sync --bin chorus-sim-lowlat -- \
//!       --build <40-hex main commit> --note <text> \
//!       [--out docs/measurements/low-latency-budget-sim.md]
//!
//! Without `--out` the report goes to stdout. `--build` names the commit the
//! report cites as its build (a commit on `main`, `docs/measurements/README.md`)
//! and `--note` says how the binary that ran relates to it; the numbers do not
//! depend on either, and `crates/sync/tests/lowlat_budget.rs` regenerates the
//! report and compares everything but those two lines.

use std::path::Path;
use std::process::ExitCode;

use chorus_sync::lowlat_sim::{report, run_lowlat, LowLatScenario, SCENARIOS};

fn read(relative: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} is unreadable: {}", path.display(), e))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut build = None;
    let mut note = None;
    let mut out = None;
    let mut i = 0;
    while i < args.len() {
        match (args[i].as_str(), args.get(i + 1)) {
            ("--build", Some(v)) => build = Some(v.clone()),
            ("--note", Some(v)) => note = Some(v.clone()),
            ("--out", Some(v)) => out = Some(v.clone()),
            _ => {
                eprintln!("usage: chorus-sim-lowlat --build <sha> --note <text> [--out <path>]");
                return ExitCode::from(2);
            }
        }
        i += 2;
    }
    let build = match build {
        Some(b) if b.len() == 40 && b.chars().all(|c| c.is_ascii_hexdigit()) => b,
        _ => {
            eprintln!("--build needs the 40-hex commit the report cites");
            return ExitCode::from(2);
        }
    };
    let Some(note) = note else {
        eprintln!("--note needs to say how the binary that ran relates to that commit");
        return ExitCode::from(2);
    };
    let mut runs = Vec::new();
    for name in SCENARIOS {
        let file = format!("config/sim-lowlat/{}", name);
        let scenario =
            LowLatScenario::parse(&read(&file)).unwrap_or_else(|e| panic!("{}: {}", file, e));
        let result = run_lowlat(&scenario).unwrap_or_else(|e| panic!("{}: {}", file, e));
        runs.push((scenario, result));
    }
    let text = report(&build, &note, &runs);
    match out {
        Some(path) => {
            std::fs::write(&path, &text).unwrap_or_else(|e| panic!("{}: {}", path, e));
            eprintln!("wrote {} ({} bytes)", path, text.len());
        }
        None => print!("{}", text),
    }
    ExitCode::SUCCESS
}
