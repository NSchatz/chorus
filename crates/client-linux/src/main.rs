//! `chorus-client`: connect, buffer, play, and leave a record.
//!
//! # Exit codes, which are the contract a script grades
//!
//! - `0`  the stream ended with the in-band end-of-stream signal, or the
//!        configured run length was reached. Both are the run finishing the
//!        way it said it would.
//! - `2`  the configuration was refused.
//! - `3`  the server could not be reached at start, or the connection was lost
//!        during the run with no end-of-stream signal. The report says which.
//! - `4`  the audio device could not be opened, or failed during the run.
//! - `5`  the client closed the session on a framing error.
//! - `6`  the delay log could not be written.
//!
//! Nothing exits zero while producing no audio, and nothing reports itself as
//! playing while it is not.

use std::io::Write;
use std::net::TcpStream;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chorus_audio::MonotonicTimeline;
use chorus_client_linux::config::{ClientConfig, ClientMode};
use chorus_client_linux::delaylog::DelayLog;
use chorus_client_linux::receive::{handshake, HandshakeError};
use chorus_client_linux::run::{counter_lines, header_for, run_session, StopReason};
use chorus_client_linux::sink::{AlsaSink, PcmSink};
use chorus_client_linux::Counters;

const EXIT_CONFIG: u8 = 2;
const EXIT_SERVER: u8 = 3;
const EXIT_DEVICE: u8 = 4;
const EXIT_FRAMING: u8 = 5;
const EXIT_LOG: u8 = 6;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (config, mode) = match ClientConfig::from_args(args) {
        Ok(v) => v,
        Err(e) => {
            report("configuration refused", &e.to_string());
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    if let Err(e) = config.validate() {
        report("configuration refused", &e.to_string());
        return ExitCode::from(EXIT_CONFIG);
    }

    match mode {
        ClientMode::ProbeDevice => probe_device(&config),
        ClientMode::Play => play(&config),
    }
}

fn report(what: &str, detail: &str) {
    let mut err = std::io::stderr();
    let _ = writeln!(err, "chorus-client: {}: {}", what, detail);
}

fn status(line: &str) {
    println!("chorus-client: {}", line);
}

/// Open the configured device, say what happened, close it, exit.
///
/// This plays nothing and claims nothing. It exists so that a verification
/// needing an audio device can say "there is no usable audio device here" and
/// stop, rather than reporting itself green having played silence.
fn probe_device(config: &ClientConfig) -> ExitCode {
    if let Err(e) = chorus_alsa::runtime_available() {
        report("no usable audio device", &e.to_string());
        status(&format!("device-probe device={} usable=0", config.device));
        return ExitCode::from(EXIT_DEVICE);
    }
    match AlsaSink::open(
        &config.device,
        chorus_protocol::SampleFormat::PcmS16Le,
        2,
        48_000,
        config.device_buffer_us() as u32,
    ) {
        Ok(sink) => {
            status(&format!(
                "device-probe device={} usable=1 rate_hz=48000 frame_len={}",
                config.device,
                sink.frame_len()
            ));
            ExitCode::SUCCESS
        }
        Err(e) => {
            report("no usable audio device", &e.to_string());
            status(&format!("device-probe device={} usable=0", config.device));
            ExitCode::from(EXIT_DEVICE)
        }
    }
}

fn play(config: &ClientConfig) -> ExitCode {
    let timeline = MonotonicTimeline::new();
    status(&format!(
        "starting server={} device={} min_us={} max_us={} start_fill_us={} \
         device_target_us={} delay_log={}",
        config.server,
        config.device,
        config.min_us,
        config.max_us,
        config.start_fill_us,
        config.device_target_us,
        config.delay_log
    ));

    let stream = match TcpStream::connect(&config.server) {
        Ok(s) => s,
        Err(e) => {
            report(
                "the server could not be reached at start",
                &format!("{}: {}", config.server, e),
            );
            status("stopped reason=server-unreachable played=0");
            return ExitCode::from(EXIT_SERVER);
        }
    };
    if let Err(e) = stream.set_read_timeout(Some(Duration::from_millis(200))) {
        report("the connection could not be configured", &e.to_string());
        return ExitCode::from(EXIT_SERVER);
    }
    let _ = stream.set_nodelay(true);
    let mut stream = stream;

    // The device cannot be opened until the stream says what it is, so the
    // first chunk is read first and carried forward.
    let keep = Arc::new(AtomicBool::new(true));
    let go = {
        let keep = Arc::clone(&keep);
        move || keep.load(Ordering::SeqCst)
    };
    let hand = match handshake(&mut stream, &go) {
        Ok(h) => h,
        Err(e) => {
            let code = match e {
                HandshakeError::Framing(_) => EXIT_FRAMING,
                _ => EXIT_SERVER,
            };
            report("the stream never started", &e.to_string());
            status("stopped reason=no-stream played=0");
            return ExitCode::from(code);
        }
    };
    status(&format!(
        "stream rate_hz={} channels={} sample_format={} frames_per_chunk={}",
        hand.shape.sample_rate_hz,
        hand.shape.channels,
        hand.shape.sample_format.name(),
        hand.shape.frames_per_chunk
    ));

    let mut sink = match AlsaSink::open(
        &config.device,
        hand.shape.sample_format,
        hand.shape.channels,
        hand.shape.sample_rate_hz,
        config.device_buffer_us() as u32,
    ) {
        Ok(s) => s,
        Err(e) => {
            report(
                "the configured audio device could not be opened",
                &format!("{}: {}", config.device, e),
            );
            status(&format!(
                "stopped reason=device-unusable device={} played=0",
                config.device
            ));
            return ExitCode::from(EXIT_DEVICE);
        }
    };

    let header = header_for(config, &config.device, &hand.shape);
    let mut log = match DelayLog::open(&config.delay_log, &header) {
        Ok(l) => l,
        Err(e) => {
            report(
                "the delay log could not be written",
                &format!("{}: {}", config.delay_log, e),
            );
            return ExitCode::from(EXIT_LOG);
        }
    };

    let counters = Arc::new(Counters::new());
    let outcome = match run_session(
        config,
        stream,
        hand,
        &mut sink,
        &mut log,
        timeline,
        Arc::clone(&counters),
    ) {
        Ok(o) => o,
        Err(e) => {
            report("the delay log could not be written", &e.to_string());
            return ExitCode::from(EXIT_LOG);
        }
    };

    for line in counter_lines(&counters) {
        status(&line);
    }
    status(&format!(
        "summary graded_span_us={} graded_samples={} delay_min_us={} delay_max_us={} \
         margin_to_min_us={} margin_to_max_us={} frames_played={} nominal_frames={}",
        outcome.summary.graded_span_us,
        outcome.summary.graded_samples,
        outcome.summary.delay_min_us,
        outcome.summary.delay_max_us,
        outcome.summary.margin_to_min_us,
        outcome.summary.margin_to_max_us,
        outcome.summary.frames_played,
        outcome.summary.nominal_frames
    ));
    status(&format!(
        "stopped reason={} played={} delay_log={}",
        outcome.stop.name(),
        u8::from(outcome.played_anything),
        config.delay_log
    ));
    status(&outcome.stop.describe());

    match outcome.stop {
        StopReason::EndOfStream(_) | StopReason::RunLengthReached => ExitCode::SUCCESS,
        StopReason::ConnectionLost { .. } | StopReason::NoStream => ExitCode::from(EXIT_SERVER),
        StopReason::Framing(_) => ExitCode::from(EXIT_FRAMING),
        StopReason::DeviceFailed(_) => ExitCode::from(EXIT_DEVICE),
    }
}
