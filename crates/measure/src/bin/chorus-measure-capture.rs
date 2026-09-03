//! The device-backed entry point: emit the chirp, record the two line outputs,
//! and refuse by name where there is no capture device.
//!
//! This is the only binary in the repository that both plays and records, and
//! it is the one place the amplitude ceiling matters, because it is the one
//! place a chirp reaches an amplifier. Two properties it holds without
//! exception:
//!
//! 1. **The amplitude is checked before anything is opened.** A run that asks
//!    for more than `config/measure.conf` permits exits naming both numbers and
//!    has emitted nothing, because it never reached a device at all.
//! 2. **An absent capture device is a non-zero exit naming the prerequisite.**
//!    Never a skip, never a green. `tools/lib.sh` carries the reason this
//!    repository is emphatic about that.
//!
//! ```text
//! chorus-measure-capture --probe-capture-device [--capture-device <name>]
//! chorus-measure-capture --out <file.wav> [--seconds <n>] [--amplitude <a>]
//!                        [--capture-device <name>] [--playback-device <name>]
//! ```

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use chorus_measure::capture;
use chorus_measure::chirp::ChirpSpec;
use chorus_measure::config::{MeasureConfig, CONFIG_FILE};
use chorus_measure::repository_root;

const USAGE: &str = "\
usage:
  chorus-measure-capture --probe-capture-device [--capture-device <name>]
  chorus-measure-capture --out <file.wav> [--seconds <n>] [--amplitude <a>]
                         [--capture-device <name>] [--playback-device <name>]

The chirp amplitude ceiling is declared in config/measure.conf. A run above it
refuses to start, names both numbers, and emits no audio.";

/// The exit code a missing prerequisite gets, matching the one `tools/lib.sh`
/// uses so a shell entry point and this binary agree.
const EXIT_MISSING_PREREQUISITE: u8 = 3;

/// The exit code an over-level request gets. Distinct from the one above on
/// purpose: "you asked for something unsafe" is a different fault from "this
/// machine has no capture device", and a script grading a run should not have
/// to read prose to tell them apart.
const EXIT_REFUSED: u8 = 4;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut named: Vec<(String, String)> = Vec::new();
    let mut flags: Vec<String> = Vec::new();
    let takes_value = [
        "capture-device",
        "playback-device",
        "out",
        "seconds",
        "amplitude",
    ];
    let mut at = 0;
    while at < args.len() {
        let Some(name) = args[at].strip_prefix("--") else {
            eprintln!("chorus-measure-capture: '{}' is not an option\n\n{}", args[at], USAGE);
            return ExitCode::from(2);
        };
        if takes_value.contains(&name) {
            let Some(value) = args.get(at + 1) else {
                eprintln!("chorus-measure-capture: --{} needs a value", name);
                return ExitCode::from(2);
            };
            named.push((name.to_string(), value.clone()));
            at += 2;
        } else {
            flags.push(name.to_string());
            at += 1;
        }
    }
    let value = |name: &str| -> Option<String> {
        named
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, v)| v.clone())
    };
    let flag = |name: &str| flags.iter().any(|f| f == name);

    if flag("help") || flag("h") {
        println!("{}", USAGE);
        return ExitCode::SUCCESS;
    }

    let root = repository_root();
    let config = match MeasureConfig::read(&root) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("chorus-measure-capture: {}", e);
            return ExitCode::from(2);
        }
    };
    let capture_device =
        value("capture-device").unwrap_or_else(|| default_device("CHORUS_CAPTURE_DEVICE"));

    // --- refusal one: the amplitude, before anything is opened ---------------
    //
    // Deliberately ahead of the device probe. An operator who asks for an
    // unsafe level on a machine that happens to have no capture device has to
    // be told about the level: it is the request that is dangerous, and it will
    // be dangerous on the machine that DOES have the device.
    let requested = match value("amplitude") {
        Some(text) => match text.parse::<f64>() {
            Ok(a) => a,
            Err(_) => {
                eprintln!(
                    "chorus-measure-capture: --amplitude '{}' is not a number",
                    text
                );
                return ExitCode::from(2);
            }
        },
        None => config.chirp_amplitude_ceiling,
    };
    let chirp = match ChirpSpec::new(
        config.chirp_start_hz,
        config.chirp_end_hz,
        config.chirp_period_us,
        requested,
        config.chirp_amplitude_ceiling,
        CONFIG_FILE,
    ) {
        Ok(chirp) => chirp,
        Err(e) => {
            eprintln!("REFUSED");
            eprintln!("  reason:       {}", e);
            eprintln!("  requested:    {} full scale", requested);
            eprintln!(
                "  permitted:    {} full scale, declared in {}",
                config.chirp_amplitude_ceiling, CONFIG_FILE
            );
            eprintln!("  emitted:      nothing. No device has been opened.");
            return ExitCode::from(EXIT_REFUSED);
        }
    };

    // --- refusal two: the capture device -------------------------------------
    let probe = capture::probe_capture_device(&capture_device, config.capture_sample_rate_hz);
    println!("{}", probe);
    if !probe.usable {
        eprintln!("MISSING PREREQUISITE");
        eprintln!(
            "  criterion:    two endpoint line outputs captured together, cross-correlated into \
             median, p95 and maximum inter-device lag"
        );
        eprintln!(
            "  prerequisite: an ALSA capture device that opens two channels at {} Hz; '{}' does \
             not ({})",
            config.capture_sample_rate_hz, probe.device, probe.detail
        );
        eprintln!(
            "  how to get it: connect both endpoints' line outputs to the L and R inputs of one \
             audio interface and point CHORUS_CAPTURE_DEVICE at it"
        );
        eprintln!("  this check is NOT passed, NOT skipped-green and NOT satisfied.");
        return ExitCode::from(EXIT_MISSING_PREREQUISITE);
    }

    if flag("probe-capture-device") {
        println!("chorus-measure-capture: the capture device is usable");
        return ExitCode::SUCCESS;
    }

    let Some(out) = value("out") else {
        eprintln!("chorus-measure-capture: --out is required for a recording\n\n{}", USAGE);
        return ExitCode::from(2);
    };
    let seconds: f64 = match value("seconds") {
        Some(text) => match text.parse() {
            Ok(s) => s,
            Err(_) => {
                eprintln!("chorus-measure-capture: --seconds '{}' is not a number", text);
                return ExitCode::from(2);
            }
        },
        None => 10.0,
    };
    let frames = (seconds * f64::from(config.capture_sample_rate_hz)) as usize;
    let playback_device =
        value("playback-device").unwrap_or_else(|| default_device("CHORUS_CLIENT_DEVICE"));

    match run(
        &playback_device,
        &capture_device,
        config.capture_sample_rate_hz,
        frames,
        &chirp,
        Path::new(&out),
    ) {
        Ok(path) => {
            println!("chorus-measure-capture: wrote {}", path.display());
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("chorus-measure-capture: {}", message);
            ExitCode::from(1)
        }
    }
}

fn default_device(variable: &str) -> String {
    std::env::var(variable).unwrap_or_else(|_| "default".to_string())
}

fn run(
    playback_device: &str,
    capture_device: &str,
    rate_hz: u32,
    frames: usize,
    chirp: &ChirpSpec,
    out: &Path,
) -> Result<PathBuf, String> {
    // The emitter runs first and drains, so the recording that follows carries
    // the chirp rather than racing it. Two endpoints playing the same grouped
    // stream is what a real run measures; this single-device path is the
    // loopback rehearsal an operator uses to check the interface before
    // trusting a number from it.
    let emitted_us = capture::emit(playback_device, rate_hz, frames, chirp)
        .map_err(|e| format!("the chirp could not be played: {}", e))?;
    let recorded = capture::record(capture_device, rate_hz, frames)
        .map_err(|e| format!("the capture failed: {}", e))?;
    std::fs::write(out, &recorded.wav)
        .map_err(|e| format!("{} could not be written: {}", out.display(), e))?;
    println!(
        "chorus-measure-capture: emitted for {} us, recorded {} frames in {} us, overran={}",
        emitted_us,
        recorded.frames,
        recorded.elapsed_us,
        u8::from(recorded.overran)
    );
    Ok(out.to_path_buf())
}
