//! Run the committed latency-growth scenarios and write their report.
//!
//!   cargo run --release -p chorus-sync --bin chorus-sim-latency -- \
//!       --build <40-hex main commit> --note <text> \
//!       [--out docs/measurements/latency-growth-sim.md]
//!
//! Without `--out` the report goes to stdout. `--build` names the commit the
//! report cites as its build (a commit on `main`, `docs/measurements/README.md`)
//! and `--note` says how the binary that ran relates to it; the numbers do not
//! depend on either, and `crates/sync/tests/latency_growth.rs` regenerates the
//! report and compares everything but those two lines.

use std::path::Path;
use std::process::ExitCode;

use chorus_sync::latency_report::{report, ScenarioRuns};
use chorus_sync::latency_sim::{run_latency, LatencyScenario, Mode, SCENARIOS};
use chorus_sync::HouseConfig;

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
                eprintln!("usage: chorus-sim-latency --build <sha> --note <text> [--out <path>]");
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

    let mut loaded = Vec::new();
    for name in SCENARIOS {
        let file = format!("config/sim-latency/{}", name);
        let scenario =
            LatencyScenario::parse(&read(&file)).unwrap_or_else(|e| panic!("{}: {}", file, e));
        let house = HouseConfig::parse(&read(&format!("config/sim-house/{}", scenario.house)))
            .unwrap_or_else(|e| panic!("{}: {}", scenario.house, e));
        let stretch = run_latency(&scenario, &house, Mode::Stretch)
            .unwrap_or_else(|e| panic!("{}: {}", file, e));
        let naive = run_latency(&scenario, &house, Mode::NaiveJump)
            .unwrap_or_else(|e| panic!("{}: {}", file, e));
        loaded.push((scenario, house, stretch, naive));
    }
    let runs: Vec<ScenarioRuns<'_>> = loaded
        .iter()
        .map(|(scenario, house, stretch, naive)| ScenarioRuns {
            scenario,
            house,
            stretch,
            naive,
        })
        .collect();
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
