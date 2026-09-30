//! Run the committed houses and write the house simulation report.
//!
//!   cargo run --release -p chorus-sync --bin chorus-sim-house -- \
//!       --build <40-hex main commit> [--out docs/measurements/sim-house-8-rooms.md]
//!
//! Without `--out` the report goes to stdout. `--build` names the commit the
//! binary was built from; the report is a measurement report and has to say
//! which build it measured (`docs/measurements/README.md`).

use std::path::Path;
use std::process::ExitCode;

use chorus_sync::house_report::report;
use chorus_sync::{run_house, HouseConfig};

fn load(name: &str) -> HouseConfig {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../config/sim-house")
        .join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} is unreadable: {}", path.display(), e));
    HouseConfig::parse(&text).unwrap_or_else(|e| panic!("{}: {}", path.display(), e))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut build = None;
    let mut out = None;
    let mut i = 0;
    while i < args.len() {
        match (args[i].as_str(), args.get(i + 1)) {
            ("--build", Some(v)) => build = Some(v.clone()),
            ("--out", Some(v)) => out = Some(v.clone()),
            _ => {
                eprintln!("usage: chorus-sim-house --build <sha> [--out <path>]");
                return ExitCode::from(2);
            }
        }
        i += 2;
    }
    let build = match build {
        Some(b) if b.len() == 40 && b.chars().all(|c| c.is_ascii_hexdigit()) => b,
        _ => {
            eprintln!("--build needs the 40-hex commit this binary was built from");
            return ExitCode::from(2);
        }
    };

    let switched = load("8-rooms-switched.house");
    let routed = load("8-rooms-routed.house");
    let switched_result = run_house(&switched).expect("the switched house runs");
    let routed_result = run_house(&routed).expect("the routed house runs");
    let text = report(
        &build,
        (&switched, &switched_result),
        (&routed, &routed_result),
    );
    match out {
        Some(path) => {
            std::fs::write(&path, &text).unwrap_or_else(|e| panic!("{}: {}", path, e));
            eprintln!("wrote {} ({} bytes)", path, text.len());
        }
        None => print!("{}", text),
    }
    ExitCode::SUCCESS
}
