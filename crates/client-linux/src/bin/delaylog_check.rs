//! `chorus-delaylog-check`: grade a saved delay log without the client
//! running.
//!
//! ```text
//! chorus-delaylog-check <log> [--min-graded-seconds N] [--require-zero-underruns]
//!                             [--require-no-rate-change] [--rate-tolerance-percent N]
//! ```
//!
//! Prints one line per check and exits non-zero if any of them failed. This is
//! the thing that turns a saved log into evidence: a claim that could only be
//! checked by the process that made it is not one.

use std::process::ExitCode;

use chorus_client_linux::logcheck::{grade, grade_no_rate_change, grade_underruns, parse};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut path: Option<String> = None;
    let mut min_graded_seconds = 0u64;
    let mut require_zero_underruns = false;
    let mut require_no_rate_change = false;
    let mut rate_tolerance_percent = 2u64;

    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--min-graded-seconds" => match it.next().and_then(|v| v.parse().ok()) {
                Some(v) => min_graded_seconds = v,
                None => return usage("--min-graded-seconds needs a number"),
            },
            "--rate-tolerance-percent" => match it.next().and_then(|v| v.parse().ok()) {
                Some(v) => rate_tolerance_percent = v,
                None => return usage("--rate-tolerance-percent needs a number"),
            },
            "--require-zero-underruns" => require_zero_underruns = true,
            "--require-no-rate-change" => require_no_rate_change = true,
            other if other.starts_with("--") => {
                return usage(&format!("unknown argument '{}'", other))
            }
            other => path = Some(other.to_string()),
        }
    }

    let path = match path {
        Some(p) => p,
        None => return usage("no log file given"),
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("chorus-delaylog-check: cannot read {}: {}", path, e);
            return ExitCode::FAILURE;
        }
    };
    let log = match parse(&text) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("chorus-delaylog-check: {}: {}", path, e);
            return ExitCode::FAILURE;
        }
    };

    let mut report = grade(&log, min_graded_seconds);
    if require_zero_underruns {
        grade_underruns(&log, &mut report);
    }
    if require_no_rate_change {
        let nominal = log
            .summary
            .get("nominal_frames")
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0);
        let frames_per_chunk = log
            .config
            .get("frames_per_chunk")
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0);
        let tolerance =
            (nominal * rate_tolerance_percent as i64 / 100).max(frames_per_chunk * 2).max(1);
        grade_no_rate_change(&log, tolerance, &mut report);
    }

    print!("{}", report);
    println!(
        "chorus-delaylog-check: {} checks, {} failed, log={}",
        report.findings.len(),
        report.findings.iter().filter(|f| !f.ok).count(),
        path
    );
    if report.ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn usage(detail: &str) -> ExitCode {
    eprintln!("chorus-delaylog-check: {}", detail);
    eprintln!(
        "usage: chorus-delaylog-check <log> [--min-graded-seconds N] \
         [--require-zero-underruns] [--require-no-rate-change] [--rate-tolerance-percent N]"
    );
    ExitCode::FAILURE
}
