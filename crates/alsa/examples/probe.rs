//! Probe one ALSA playback device and say what it reports.
//!
//! `cargo run --example probe -p chorus-alsa -- <device>`
//!
//! This is the smallest thing that answers "can this machine play audio at
//! all, and does the device report a delay", which is the prerequisite every
//! device-dependent verification in `tools/` checks before it starts.

use std::process::ExitCode;

use chorus_alsa::{Format, Pcm};

fn main() -> ExitCode {
    let device = std::env::args().nth(1).unwrap_or_else(|| "null".to_string());

    if let Err(e) = chorus_alsa::runtime_available() {
        eprintln!("probe: {}", e);
        return ExitCode::FAILURE;
    }

    let mut pcm = match Pcm::open(&device, Format::S16Le, 2, 48_000, 340_000) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("probe: {}", e);
            return ExitCode::FAILURE;
        }
    };

    println!("probe: opened {}", pcm.device());
    let frame = pcm.frame_len();
    let chunk = vec![0u8; frame * 960];
    for i in 0..25 {
        match pcm.write(&chunk) {
            Ok(report) => {
                let delay = pcm.delay_frames().unwrap_or(-1);
                println!(
                    "probe: write {} frames={} underran={} delay_frames={} delay_us={}",
                    i,
                    report.frames_written,
                    report.underran,
                    delay,
                    delay * 1_000_000 / i64::from(pcm.rate_hz())
                );
            }
            Err(e) => {
                eprintln!("probe: write failed: {}", e);
                return ExitCode::FAILURE;
            }
        }
    }
    if let Err(e) = pcm.drain() {
        eprintln!("probe: drain failed: {}", e);
        return ExitCode::FAILURE;
    }
    println!("probe: ok");
    ExitCode::SUCCESS
}
