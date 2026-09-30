//! The stream every graded run plays and the analyser that grades it are held
//! to each other here, so the pairing cannot drift apart again.
//!
//! Audit A-7 found the SYNC-4 hour run streaming the 440 Hz tone while the lag
//! analyser refuses anything without most of its energy in the chirp's band:
//! the one run that grades the phase could only ever end in
//! `no-chirp-present`. Nothing failed, because nothing fed what the server
//! streams into what the rig analyses. This does, with no device: PCM as
//! `chorus-server --source chirp` produces it, played by two modelled
//! endpoints a known number of frames apart, captured into a two-channel WAV
//! and read back through the same parser and estimator `chorus-measure lag`
//! uses.
//!
//! What this does NOT model is the analogue half: two DACs, two amplifiers and
//! an interface resampling to the capture rate. That is the hardware run's
//! job, and this is the precondition for it being able to report anything.

use std::path::Path;

use chorus_audio::StreamFormat;
use chorus_measure::config::{MeasureConfig, CONFIG_FILE};
use chorus_measure::lag::{self, LagSettings};
use chorus_measure::repository_root;
use chorus_measure::wav::{self, WAVE_FORMAT_PCM};
use chorus_server::source::{self, PcmSource};

/// The rate the stream and the modelled capture run at. The server's default
/// stream rate; the committed `config/verification.conf` runs at it too.
const RATE_HZ: u32 = 48_000;

/// How far endpoint B plays behind endpoint A: 12 frames, 250 us at 48 kHz.
const OFFSET_FRAMES: usize = 12;

/// Pull `frames` frames of 16-bit stereo out of a source, and keep channel 0.
fn first_channel(source: &mut dyn PcmSource, frames: usize) -> Vec<i16> {
    let mut raw = vec![0u8; frames * 4];
    let mut filled = 0;
    while filled < raw.len() {
        let n = source.read(&mut raw[filled..]).unwrap();
        assert!(n > 0, "the source ended after {} bytes", filled);
        filled += n;
    }
    (0..raw.len())
        .step_by(4)
        .map(|at| i16::from_le_bytes([raw[at], raw[at + 1]]))
        .collect()
}

/// Two endpoints playing `stream`, B `offset` frames behind A, captured on
/// the L and R inputs of one interface, and analysed.
fn capture_and_analyse(stream: &[i16], offset: usize) -> Result<lag::LagSummary, lag::LagError> {
    let config = MeasureConfig::read(&repository_root()).expect("config/measure.conf is committed");
    let mut interleaved = Vec::with_capacity(2 * stream.len());
    for n in offset..stream.len() {
        interleaved.push(stream[n]);
        interleaved.push(stream[n - offset]);
    }
    let bytes = wav::write_wav(&interleaved, 2, RATE_HZ, 16, WAVE_FORMAT_PCM);
    let capture = wav::parse_capture(Path::new("modelled-endpoints.wav"), &bytes, RATE_HZ)
        .expect("a two-channel 16-bit capture at the declared rate parses");
    lag::estimate(&capture, &LagSettings::from_config(&config, RATE_HZ))
}

fn format() -> StreamFormat {
    StreamFormat::new(RATE_HZ, 2, "pcm_s16le").unwrap()
}

#[test]
fn the_servers_chirp_is_accepted_by_the_lag_analyser_and_the_offset_is_recovered() {
    let chirp = source::rig_chirp(&repository_root().join(CONFIG_FILE), None)
        .expect("the committed configuration builds a chirp");
    let mut stream = source::open("chirp", format(), 20_000, 0, Some(&chirp)).unwrap();
    let pcm = first_channel(stream.as_mut(), RATE_HZ as usize);

    let summary = capture_and_analyse(&pcm, OFFSET_FRAMES)
        .unwrap_or_else(|e| panic!("the server's chirp was refused: {} ({})", e, e.condition()));

    // B plays behind A, so under lag::SIGN_CONVENTION A leads and the lag is
    // positive.
    let want_us = OFFSET_FRAMES as f64 * 1_000_000.0 / f64::from(RATE_HZ);
    assert!(
        (summary.median_us - want_us).abs() < 1.0,
        "median {:.3} us, the endpoints were {:.3} us apart",
        summary.median_us,
        want_us
    );
    assert!(
        (summary.max_abs_us - want_us).abs() < 1.0,
        "max {:.3} us, the endpoints were {:.3} us apart",
        summary.max_abs_us,
        want_us
    );
    assert!(summary.windows_used >= 4, "{:?}", summary.windows_used);
}

#[test]
fn the_tone_is_refused_as_no_chirp_which_is_why_graded_runs_stream_the_chirp() {
    // The failure A-7 found, pinned: if a graded run is ever pointed back at the
    // tone, this is what its capture would say.
    let mut stream = source::open("tone", format(), 20_000, 0, None).unwrap();
    let pcm = first_channel(stream.as_mut(), RATE_HZ as usize);
    let err = capture_and_analyse(&pcm, OFFSET_FRAMES).unwrap_err();
    assert_eq!(err.condition(), "no-chirp-present", "{}", err);
}

#[test]
fn every_graded_run_streams_the_chirp() {
    // The three entry points whose captures go through the lag analyser. A
    // script that started the server with any other source would be graded
    // by an analyser that refuses it.
    let root = repository_root();
    for script in [
        "tools/sync-hour-run.sh",
        "tools/endpoint-rig-run.sh",
        "tools/wireless-characterization-run.sh",
        "tools/measure/capture-run.sh",
    ] {
        let text = std::fs::read_to_string(root.join(script)).unwrap();
        // The `--source` of each `chorus-server` command, which runs from the
        // line naming the binary to the first line without a continuation.
        let mut sources: Vec<&str> = Vec::new();
        let mut in_server = false;
        for line in text.lines() {
            let line = line.trim();
            if line.contains("/chorus-server\"") {
                in_server = true;
            }
            if in_server {
                if let Some(rest) = line.strip_prefix("--source ") {
                    sources.push(rest.trim_end_matches('\\').trim());
                }
                if !line.ends_with('\\') {
                    in_server = false;
                }
            }
        }
        assert_eq!(
            sources,
            vec!["chirp"],
            "{} starts chorus-server with {:?}",
            script,
            sources
        );
    }
}

#[test]
fn the_server_refuses_an_over_level_chirp_before_it_binds_anything() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_chorus-server"))
        .args([
            "--source",
            "chirp",
            "--chirp-amplitude",
            "0.9",
            "--listen",
            "127.0.0.1:0",
            "--measure-config",
        ])
        .arg(repository_root().join(CONFIG_FILE))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{}{}", stdout, stderr);
    assert!(stdout.contains("reason=chirp-refused"), "{}", stdout);
    assert!(stderr.contains("0.9"), "{}", stderr);
    assert!(!stdout.contains("listening"), "{}", stdout);
}
