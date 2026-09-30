//! The analysis entry point: a capture or an offset series in, a report in
//! `docs/measurements/` out.
//!
//! Needs no device and no privilege. That is the whole design: an
//! already-captured recording is a first-class input, so a reader who did not
//! take the capture can reproduce every figure from the committed fixtures.
//!
//! ```text
//! chorus-measure lag <capture.wav> --label <name> --source fixture|hardware [--out <dir>] [--baseline <file>]
//! chorus-measure pair <a.offsets> <b.offsets> --label <name> --source fixture|hardware --series-out <file>
//! chorus-measure free-run <series.offsets> --label <name> --source fixture|hardware [--out <dir>]
//! chorus-measure jitter <series.offsets> --label <name> --mode <power save> --transport <t>
//! chorus-measure rate <capture.wav> [--channel a|b|both] [--rate <hz>]
//! chorus-measure fixtures [--check]
//! ```

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use chorus_audio::MonotonicTimeline;
use chorus_measure::config::MeasureConfig;
use chorus_measure::freerun::{self, SlopeSettings};
use chorus_measure::jitter::{self, JitterError, JitterRun, PowerSaveMode};
use chorus_measure::lag::{self, LagSettings};
use chorus_measure::rate::{self, Channel, RateSettings};
use chorus_measure::report::{self, Baseline, FreeRunRun, LagRun, BASELINE_FILE, MEASUREMENTS_DIR};
use chorus_measure::{fixtures, pair, repository_root, wav};

const USAGE: &str = "\
usage:
  chorus-measure lag <capture.wav> --label <name> --source fixture|hardware
                     [--out <dir>] [--baseline <file>]
  chorus-measure pair <a.offsets> <b.offsets> --label <name> --source fixture|hardware
                      --series-out <file> [--max-gap-ms <ms>] [--out <dir>] [--baseline-out <file>]
  chorus-measure free-run <series.offsets> --label <name> --source fixture|hardware
                          [--out <dir>] [--baseline-out <file>]
  chorus-measure jitter <series.offsets> --label <name> --mode none|min-modem|max-modem
                        --transport wired|wireless [--out <dir>] [--measured]
  chorus-measure rate <capture.wav> [--channel a|b|both] [--rate <hz>]
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
        "pair" => pair_command(&args[1..]),
        "jitter" => jitter_command(&args[1..]),
        "rate" => rate_command(&args[1..]),
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
    let options = Options::parse(args, &["label", "out", "baseline", "rate", "source"])?;
    let capture_path = options
        .positional
        .first()
        .ok_or_else(|| format!("a capture is required\n\n{}", USAGE))?;
    let label = options
        .value("label")
        .ok_or_else(|| format!("--label is required\n\n{}", USAGE))?;
    // Required, with no default, for the reason free-run's is (audit A-5): the
    // saved report's `Source:` line has to say whether the capture came from
    // real line outputs or from a committed fixture, and this rig will not guess.
    let source = run_source(&options)?;

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
    report::require_clean_for_hardware(&build, source).map_err(|e| e.to_string())?;
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
        "cargo run -p chorus-measure --bin chorus-measure -- lag {} --label {} --source {}",
        report::relative_display(Path::new(capture_path), &root),
        label,
        options.value("source").unwrap_or_default()
    );
    let run = LagRun {
        label,
        capture: &capture,
        settings: &settings,
        summary: &summary,
        build: &build,
        baseline: baseline.as_ref(),
        source,
        command: &command,
        analysis_us,
        root: &root,
    };
    let name = format!("rig3-lag-{}.md", slug(label));
    let written = report::write_report(&out, &name, &report::render_lag_report(&run))
        .map_err(|e| e.to_string())?;
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

/// Audit A-11: the sample rate a captured output actually produced, against
/// the capture clock, recovered from where each repeat of the chirp starts.
/// Prints one line per channel and writes nothing: no report and no baseline
/// (audit A-5 was a baseline overwritten by the wrong command). The bench
/// library keeps the printed lines with the run's hashed raw data.
fn rate_command(args: &[String]) -> Result<(), String> {
    let options = Options::parse(args, &["channel", "rate"])?;
    let capture_path = options
        .positional
        .first()
        .ok_or_else(|| format!("a capture is required\n\n{}", USAGE))?;
    let root = repository_root();
    let config = MeasureConfig::read(&root).map_err(|e| e.to_string())?;
    let sample_rate = match options.value("rate") {
        Some(text) => text
            .parse::<u32>()
            .map_err(|_| format!("--rate '{}' is not a sample rate", text))?,
        None => config.capture_sample_rate_hz,
    };
    let channels: &[Channel] = match options.value("channel").unwrap_or("both") {
        "a" => &[Channel::A],
        "b" => &[Channel::B],
        "both" => &[Channel::A, Channel::B],
        other => return Err(format!("--channel is a, b or both, not '{}'", other)),
    };
    let capture =
        wav::read_capture(Path::new(capture_path), sample_rate).map_err(|e| e.to_string())?;
    let settings = RateSettings::from_config(&config)?;
    for &channel in channels {
        let estimate = rate::estimate(&capture, channel, &settings).map_err(|e| e.to_string())?;
        println!(
            "chorus-measure: channel {} produced rate {:+.3} ppm (+/-{:.3} ppm, 95%) against \
             the capture clock, over {} of {} sweep starts spanning {:.1} s",
            channel.name(),
            estimate.ppm,
            estimate.half_width_ppm,
            estimate.windows_used,
            estimate.windows_total,
            estimate.span_s
        );
    }
    Ok(())
}

/// The `--source` word of a run, mapped to its report's `Source:` word.
fn run_source(options: &Options) -> Result<&'static str, String> {
    let word = options.value("source").ok_or_else(|| {
        format!(
            "--source is required: 'fixture' for a committed input, 'hardware' for a capture \
             or series taken from real devices. The saved report has to say which it is, and \
             this rig will not guess\n\n{}",
            USAGE
        )
    })?;
    match word {
        "fixture" | "hardware" => Ok(report::report_source(word).unwrap_or("synthetic")),
        other => Err(format!(
            "--source is 'fixture' or 'hardware', not '{}'",
            other
        )),
    }
}

/// Audit A-4: two clients' free-run series in, the relative series out, then
/// the free-run fit and the baseline over it exactly as `free-run` does.
fn pair_command(args: &[String]) -> Result<(), String> {
    let options = Options::parse(
        args,
        &[
            "label",
            "source",
            "series-out",
            "max-gap-ms",
            "out",
            "baseline-out",
        ],
    )?;
    let (Some(a_path), Some(b_path)) = (options.positional.first(), options.positional.get(1))
    else {
        return Err(format!("two client series are required\n\n{}", USAGE));
    };
    let label = options
        .value("label")
        .ok_or_else(|| format!("--label is required\n\n{}", USAGE))?;
    run_source(&options)?;
    let series_out = options
        .value("series-out")
        .ok_or_else(|| format!("--series-out is required\n\n{}", USAGE))?;
    // Five seconds is ten sync ticks at the committed 500 ms cadence: a choice
    // about what counts as a missing stretch, not a measured value.
    let max_gap_ms: i64 = match options.value("max-gap-ms") {
        Some(text) => text
            .parse()
            .map_err(|_| format!("--max-gap-ms '{}' is not a number of milliseconds", text))?,
        None => 5_000,
    };
    let a = pair::read_client_series(Path::new(a_path)).map_err(|e| e.to_string())?;
    let b = pair::read_client_series(Path::new(b_path)).map_err(|e| e.to_string())?;
    let pairs = pair::pair(&a, &b, max_gap_ms * 1_000_000).map_err(|e| e.to_string())?;
    let text = pair::render_paired(label, &a, &b, &pairs);
    std::fs::write(series_out, text)
        .map_err(|e| format!("{} could not be written: {}", series_out, e))?;
    println!(
        "chorus-measure: paired {} observations of {} with {} into {}",
        pairs.len(),
        a.series.label,
        b.series.label,
        series_out
    );
    let mut fit_args = vec![
        series_out.to_string(),
        "--label".to_string(),
        label.to_string(),
        "--source".to_string(),
        options.value("source").unwrap_or_default().to_string(),
    ];
    for key in ["out", "baseline-out"] {
        if let Some(value) = options.value(key) {
            fit_args.push(format!("--{}", key));
            fit_args.push(value.to_string());
        }
    }
    free_run_command(&fit_args)
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
    // Required, with no default. A default of `fixture` recorded a hardware
    // run that forgot the flag as a fixture, and a default of `hardware` would
    // call a fixture a measurement; the run has to say which it is.
    let source = options
        .value("source")
        .ok_or_else(|| {
            format!(
                "--source is required: 'fixture' for a committed series, 'hardware' for two \
                 real clients. The recorded baseline has to say which it is, and this rig will \
                 not guess\n\n{}",
                USAGE
            )
        })?
        .to_string();
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

    let baseline_out = options
        .value("baseline-out")
        .map(PathBuf::from)
        .unwrap_or_else(|| out.join(BASELINE_FILE));
    // Before anything is written: a fixture run never replaces a baseline
    // fitted from hardware, which other reports cite as a measurement.
    Baseline::check_replaceable(&baseline_out, &source).map_err(|e| e.to_string())?;

    let series = freerun::read_series(Path::new(series_path)).map_err(|e| e.to_string())?;
    let fit = freerun::fit(&series, &settings).map_err(|e| e.to_string())?;
    let build = report::build_identity(&root).map_err(|e| e.to_string())?;
    report::require_clean_for_hardware(&build, report::report_source(&source).unwrap_or(""))
        .map_err(|e| e.to_string())?;

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
        "cargo run -p chorus-measure --bin chorus-measure -- free-run {} --label {} --source {}",
        report::relative_display(Path::new(series_path), &root),
        label,
        baseline.source
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

/// `chorus#WIFI-7`'s characterization, host side: a series in, one report per
/// power-save mode into `docs/measurements/`.
///
/// The mode is REQUIRED and has no default. A jitter figure taken with modem
/// sleep disabled and one taken with the platform default left in place are
/// measurements of different systems, and a report that did not say which was in
/// force would be a number nobody could use.
fn jitter_command(args: &[String]) -> Result<(), String> {
    let options = Options::parse(args, &["label", "out", "mode", "transport"])?;
    let series_path = options
        .positional
        .first()
        .ok_or_else(|| format!("a series is required\n\n{}", USAGE))?;
    let label = options
        .value("label")
        .ok_or_else(|| format!("--label is required\n\n{}", USAGE))?;

    let mode = match options.value("mode") {
        Some(word) => PowerSaveMode::parse(word).ok_or_else(|| {
            JitterError::ModeNotKnown {
                offered: word.to_string(),
                permitted: PowerSaveMode::permitted(),
            }
            .to_string()
        })?,
        None => {
            return Err(JitterError::ModeNotKnown {
                offered: String::new(),
                permitted: PowerSaveMode::permitted(),
            }
            .to_string())
        }
    };
    let transport = options.value("transport").unwrap_or("wireless").to_string();
    if !jitter::TRANSPORTS.contains(&transport.as_str()) {
        return Err(JitterError::TransportNotKnown {
            offered: transport,
            permitted: jitter::TRANSPORTS.join(", "),
        }
        .to_string());
    }

    let root = repository_root();
    let out = options
        .value("out")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join(MEASUREMENTS_DIR));

    let series = freerun::read_series(Path::new(series_path)).map_err(|e| e.to_string())?;
    let summary = jitter::analyse(&series).map_err(|e| e.to_string())?;
    let build = report::build_identity(&root).map_err(|e| e.to_string())?;
    if options.flag("measured") {
        report::require_clean_for_hardware(&build, "hardware").map_err(|e| e.to_string())?;
    }

    // A series is a fixture unless the run says it was taken over a real link.
    // The default is the cautious one: a report that wrongly says "fixture" is
    // an under-claim, and one that wrongly says "measured" is evidence that is
    // not there.
    let from_fixture = !options.flag("measured");
    let command = format!(
        "cargo run -p chorus-measure --bin chorus-measure -- jitter {} --label {} --mode {} \
         --transport {}",
        report::relative_display(Path::new(series_path), &root),
        label,
        mode.name(),
        transport
    );
    let run = JitterRun {
        label,
        series: &series,
        summary: &summary,
        mode,
        transport: &transport,
        from_fixture,
        build: &build,
        command: &command,
        root: &root,
    };
    let name = format!("rig3-jitter-{}.md", slug(label));
    let written = report::write_report(&out, &name, &jitter::render_jitter_report(&run))
        .map_err(|e| e.to_string())?;
    println!(
        "chorus-measure: power save {}, transport {}: median {:.1} us, p95 {:.1} us, max {:.1} \
         us, peak to peak {:.1} us over {} observations",
        mode.name(),
        transport,
        summary.median_us,
        summary.p95_us,
        summary.max_us,
        summary.peak_to_peak_us,
        summary.observations
    );
    println!("chorus-measure: wrote {}", written.display());
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
                    println!(
                        "pass {} is byte-identical to its parameters",
                        output.display()
                    )
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
