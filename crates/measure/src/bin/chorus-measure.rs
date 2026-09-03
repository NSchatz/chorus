//! The analysis entry point: a capture or an offset series in, a report in
//! `docs/measurements/` out.
//!
//! Needs no device and no privilege. That is the whole design: an
//! already-captured recording is a first-class input, so a reader who did not
//! take the capture can reproduce every figure from the committed fixtures.
//!
//! ```text
//! chorus-measure lag <capture.wav> --label <name> [--out <dir>] [--baseline <file>]
//! chorus-measure free-run <series.offsets> --label <name> [--out <dir>] [--source fixture|hardware]
//! chorus-measure fixtures [--check]
//! ```

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use chorus_audio::MonotonicTimeline;
use chorus_measure::config::MeasureConfig;
use chorus_measure::freerun::{self, SlopeSettings};
use chorus_measure::lag::{self, LagSettings};
use chorus_measure::report::{
    self, Baseline, FreeRunRun, LagRun, BASELINE_FILE, MEASUREMENTS_DIR,
};
use chorus_measure::{fixtures, repository_root, wav};

const USAGE: &str = "\
usage:
  chorus-measure lag <capture.wav> --label <name> [--out <dir>] [--baseline <file>]
  chorus-measure free-run <series.offsets> --label <name> [--out <dir>]
                          [--source fixture|hardware] [--baseline-out <file>]
  chorus-measure fixtures [--check]

Every threshold this rig compares against is declared in config/measure.conf.
A run that cannot resolve the two outputs exits non-zero naming which condition
it hit and writes no report.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first() else {
        eprintln!("{}", USAGE);
        return ExitCode::from(2);
    };
    let result = match command.as_str() {
        "lag" => lag_command(&args[1..]),
        "free-run" => free_run_command(&args[1..]),
        "fixtures" => fixtures_command(&args[1..]),
        "--help" | "-h" | "help" => {
            println!("{}", USAGE);
            return ExitCode::SUCCESS;
        }
        other => Err(format!("'{}' is not a command\n\n{}", other, USAGE)),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("chorus-measure: {}", message);
            ExitCode::from(1)
        }
    }
}

/// A hand-rolled option reader. No argument-parsing crate, for the reason
/// `docs/decisions/0002-repository-layout-and-ci.md` gives.
struct Options {
    positional: Vec<String>,
    named: Vec<(String, String)>,
    flags: Vec<String>,
}

impl Options {
    fn parse(args: &[String], takes_value: &[&str]) -> Result<Options, String> {
        let mut options = Options {
            positional: Vec::new(),
            named: Vec::new(),
            flags: Vec::new(),
        };
        let mut at = 0;
        while at < args.len() {
            let arg = &args[at];
            if let Some(name) = arg.strip_prefix("--") {
                if takes_value.contains(&name) {
                    let value = args
                        .get(at + 1)
                        .ok_or_else(|| format!("--{} needs a value", name))?;
                    options.named.push((name.to_string(), value.clone()));
                    at += 2;
                    continue;
                }
                options.flags.push(name.to_string());
                at += 1;
                continue;
            }
            options.positional.push(arg.clone());
            at += 1;
        }
        Ok(options)
    }

    fn value(&self, name: &str) -> Option<&str> {
        self.named
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    fn flag(&self, name: &str) -> bool {
        self.flags.iter().any(|f| f == name)
    }
}

fn slug(label: &str) -> String {
    label
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

fn lag_command(args: &[String]) -> Result<(), String> {
    let options = Options::parse(args, &["label", "out", "baseline", "rate"])?;
    let capture_path = options
        .positional
        .first()
        .ok_or_else(|| format!("a capture is required\n\n{}", USAGE))?;
    let label = options
        .value("label")
        .ok_or_else(|| format!("--label is required\n\n{}", USAGE))?;

    let root = repository_root();
    let config = MeasureConfig::read(&root).map_err(|e| e.to_string())?;
    let rate = match options.value("rate") {
        Some(text) => text
            .parse::<u32>()
            .map_err(|_| format!("--rate '{}' is not a sample rate", text))?,
        None => config.capture_sample_rate_hz,
    };
    let out = options
        .value("out")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join(MEASUREMENTS_DIR));

    let capture = wav::read_capture(Path::new(capture_path), rate).map_err(|e| e.to_string())?;
    let settings = LagSettings::from_config(&config, capture.sample_rate_hz);

    let timeline = MonotonicTimeline::new();
    let summary = lag::estimate(&capture, &settings).map_err(|e| e.to_string())?;
    let analysis_us = timeline.now_us();

    let build = report::build_identity(&root).map_err(|e| e.to_string())?;
    let baseline_path = options
        .value("baseline")
        .map(PathBuf::from)
        .unwrap_or_else(|| out.join(BASELINE_FILE));
    let baseline = if baseline_path.exists() {
        Some(Baseline::read(&baseline_path).map_err(|e| e.to_string())?)
    } else {
        None
    };

    let command = format!(
        "cargo run -p chorus-measure --bin chorus-measure -- lag {} --label {}",
        report::relative_display(Path::new(capture_path), &root),
        label
    );
    let run = LagRun {
        label,
        capture: &capture,
        settings: &settings,
        summary: &summary,
        build: &build,
        baseline: baseline.as_ref(),
        command: &command,
        analysis_us,
        root: &root,
    };
    let name = format!("rig3-lag-{}.md", slug(label));
    let written =
        report::write_report(&out, &name, &report::render_lag_report(&run)).map_err(|e| e.to_string())?;
    println!(
        "chorus-measure: median {:+.3} us, p95 {:.3} us, max {:.3} us over {} of {} windows",
        summary.median_us,
        summary.p95_abs_us,
        summary.max_abs_us,
        summary.windows_used,
        summary.windows_total
    );
    println!("chorus-measure: wrote {}", written.display());
    Ok(())
}

fn free_run_command(args: &[String]) -> Result<(), String> {
    let options = Options::parse(args, &["label", "out", "source", "baseline-out"])?;
    let series_path = options
        .positional
        .first()
        .ok_or_else(|| format!("a series is required\n\n{}", USAGE))?;
    let label = options
        .value("label")
        .ok_or_else(|| format!("--label is required\n\n{}", USAGE))?;
    let source = options.value("source").unwrap_or("fixture").to_string();
    if source != "fixture" && source != "hardware" {
        return Err(format!(
            "--source is 'fixture' or 'hardware', not '{}'. A baseline fitted from a committed \
             fixture bounds the estimator and says nothing about any real crystal, and the \
             recorded artifact has to say which it is",
            source
        ));
    }

    let root = repository_root();
    let config = MeasureConfig::read(&root).map_err(|e| e.to_string())?;
    let settings = SlopeSettings::from_config(&config);
    let out = options
        .value("out")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join(MEASUREMENTS_DIR));

    let series = freerun::read_series(Path::new(series_path)).map_err(|e| e.to_string())?;
    let fit = freerun::fit(&series, &settings).map_err(|e| e.to_string())?;
    let build = report::build_identity(&root).map_err(|e| e.to_string())?;

    let report_name = format!("rig3-free-run-{}.md", slug(label));
    let baseline = Baseline {
        ppm: fit.ppm,
        half_width_ppm: fit.half_width_ppm,
        source,
        series: report::relative_display(Path::new(series_path), &root),
        established_by: format!("{}/{}", MEASUREMENTS_DIR, report_name),
        established_at_commit: build.commit.clone(),
    };
    let command = format!(
        "cargo run -p chorus-measure --bin chorus-measure -- free-run {} --label {}",
        report::relative_display(Path::new(series_path), &root),
        label
    );
    let run = FreeRunRun {
        label,
        series: &series,
        settings: &settings,
        fit: &fit,
        build: &build,
        baseline: &baseline,
        command: &command,
        root: &root,
    };
    let written = report::write_report(&out, &report_name, &report::render_free_run_report(&run))
        .map_err(|e| e.to_string())?;

    let baseline_out = options
        .value("baseline-out")
        .map(PathBuf::from)
        .unwrap_or_else(|| out.join(BASELINE_FILE));
    let baseline_dir = baseline_out
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let baseline_name = baseline_out
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| BASELINE_FILE.to_string());
    report::write_report(&baseline_dir, &baseline_name, &baseline.render())
        .map_err(|e| e.to_string())?;

    println!(
        "chorus-measure: {:+.4} ppm (+/-{:.4} ppm) over {} observations spanning {:.1} s",
        fit.ppm, fit.half_width_ppm, fit.points, fit.span_s
    );
    println!("chorus-measure: wrote {}", written.display());
    println!(
        "chorus-measure: recorded the free-run baseline in {}",
        baseline_out.display()
    );
    Ok(())
}

fn fixtures_command(args: &[String]) -> Result<(), String> {
    let options = Options::parse(args, &["dir"])?;
    let root = repository_root();
    let dir = options
        .value("dir")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join(fixtures::FIXTURES_DIR));
    let check = options.flag("check");

    let all = fixtures::read_all(&dir).map_err(|e| e.to_string())?;
    let mut differing = Vec::new();
    for params in &all {
        let bytes = fixtures::generate(params).map_err(|e| e.to_string())?;
        let output = params.output_path();
        if check {
            match std::fs::read(&output) {
                Ok(committed) if committed == bytes => {
                    println!("pass {} is byte-identical to its parameters", output.display())
                }
                Ok(committed) => {
                    println!(
                        "FAIL {} is {} bytes and its parameters generate {}",
                        output.display(),
                        committed.len(),
                        bytes.len()
                    );
                    differing.push(output);
                }
                Err(e) => {
                    println!("FAIL {} could not be read: {}", output.display(), e);
                    differing.push(output);
                }
            }
        } else {
            std::fs::write(&output, &bytes)
                .map_err(|e| format!("{} could not be written: {}", output.display(), e))?;
            println!("wrote {} ({} bytes)", output.display(), bytes.len());
        }
    }
    if !differing.is_empty() {
        return Err(format!(
            "{} committed fixture(s) do not match their parameters",
            differing.len()
        ));
    }
    Ok(())
}
