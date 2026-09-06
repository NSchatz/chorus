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
//! - `4`  the audio device could not be opened, failed during the run, or
//!        refused to report its delay to the DAC. The last is its own reported
//!        reason, `delay-refused`, because it is the one signal the sync loop
//!        is entitled to use and there is no substitute for it.
//! - `5`  the client closed the session on a framing error.
//! - `6`  the delay log could not be written.
//! - `7`  this endpoint has no server address at all: discovery returned
//!        nothing, or was not attempted, and no static address was configured.
//!        The message says which of the two it lacked, because those are two
//!        different things to fix.
//!
//! Nothing exits zero while producing no audio, and nothing reports itself as
//! playing while it is not.
//!
//! # Where an endpoint finds its server, in order
//!
//! 1. **The control channel**, where there is one. The server is authoritative
//!    about which group a zone is in and where that group's stream is served,
//!    so a zone that has been grouped elsewhere is told, and this endpoint
//!    moves.
//! 2. **Multicast DNS**, with `--discover`.
//! 3. **The configured static address**, with `--server`.
//!
//! The third is an assertion and not a nicety. Whether multicast reaches a
//! container and crosses a VLAN is an open question in this deployment, and the
//! fallback is what makes an endpoint work either way. What is NOT allowed is
//! silence: an endpoint with neither exits `7` saying so.

use std::io::Write;
use std::net::TcpStream;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chorus_audio::MonotonicTimeline;
use chorus_client_linux::config::{ClientConfig, ClientMode};
use chorus_client_linux::control::{ControlLink, ZoneWatch};
use chorus_client_linux::delaylog::DelayLog;
use chorus_client_linux::receive::{handshake, HandshakeError};
use chorus_client_linux::run::{counter_lines, header_for, run_session, StopReason};
use chorus_client_linux::sink::{AlsaSink, PcmSink};
use chorus_client_linux::Counters;
use chorus_discovery::dnssd::AUDIO_SERVICE;
use chorus_discovery::net::locate;

const EXIT_CONFIG: u8 = 2;
const EXIT_SERVER: u8 = 3;
const EXIT_DEVICE: u8 = 4;
const EXIT_FRAMING: u8 = 5;
const EXIT_LOG: u8 = 6;
const EXIT_NO_SERVER: u8 = 7;

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
        ClientMode::Play => endpoint(&config),
    }
}

/// One endpoint: subscribe to its zone, find its server, and play until it is
/// told to stop or until nothing is left to try.
///
/// With `--rejoin` this is a loop and a session that ends is a session to start
/// again. Without it there is exactly one session and the exit code is that
/// session's, which is what every verification written before this phase reads.
fn endpoint(config: &ClientConfig) -> ExitCode {
    let timeline = MonotonicTimeline::new();
    let keep = Arc::new(AtomicBool::new(true));
    let watch = Arc::new(ZoneWatch::new());

    // The control channel first, so that the first session already knows its
    // zone's volume, its mute and which group's stream it is meant to be on.
    let link = config.control.as_ref().map(|address| ControlLink {
        address: address.clone(),
        zone: config.zone.clone(),
        endpoint: config.endpoint.clone(),
    });
    let mut following = None;
    if let Some(link) = &link {
        match link.attach() {
            Ok(state) => {
                watch.absorb(&state, &config.zone);
                status(&format!(
                    "control attached={} endpoint={} {}",
                    link.address,
                    config.endpoint,
                    watch.line()
                ));
            }
            Err(e) => {
                // Not fatal. An endpoint whose control channel is not up yet
                // plays at full scale and keeps trying, which is what the
                // follow loop below does.
                report(
                    "the control channel could not be reached",
                    &format!(
                        "{}: {}; this endpoint will play at full scale and keep trying",
                        link.address, e
                    ),
                );
            }
        }
        let link = link.clone();
        let watch = Arc::clone(&watch);
        let keep = Arc::clone(&keep);
        following = Some(std::thread::spawn(move || {
            let go = || keep.load(Ordering::SeqCst);
            link.follow(&watch, &go);
        }));
    }

    let run_limit_us = config.run_seconds.map(|s| s * 1_000_000);
    let mut session = 0u64;
    let mut played_ever = false;
    let mut total_frames = 0u64;
    let mut backoff_ms = 50u64;
    let code = loop {
        session += 1;
        let address = match where_to_play(config, &watch) {
            Ok(address) => address,
            Err(e) => {
                report("this endpoint has nowhere to play from", &e);
                status("stopped reason=no-server-address played=0");
                break ExitCode::from(EXIT_NO_SERVER);
            }
        };
        let outcome = play(config, &address, session, timeline, &watch);
        played_ever |= outcome.played;
        total_frames += outcome.frames_played;
        status(&format!(
            "session n={} server={} played={} frames_played={} total_frames_played={} \
             stop={} {}",
            session,
            address,
            u8::from(outcome.played),
            outcome.frames_played,
            total_frames,
            outcome.reason,
            watch.line()
        ));
        if !config.rejoin {
            break outcome.code;
        }
        if outcome.code == ExitCode::from(EXIT_CONFIG)
            || outcome.code == ExitCode::from(EXIT_LOG)
        {
            // A configuration or a log that cannot be written will not fix
            // itself by being tried again.
            break outcome.code;
        }
        if let Some(limit) = run_limit_us {
            if timeline.now_us() >= limit {
                status(&format!(
                    "stopped reason=run-length-reached sessions={} total_frames_played={} \
                     played={}",
                    session,
                    total_frames,
                    u8::from(played_ever)
                ));
                break if played_ever {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::from(EXIT_SERVER)
                };
            }
        }
        // A backoff, not a spin. The endpoint has nothing else to do and the
        // server may be a second away from coming back, so this stays short and
        // is bounded by --rejoin-max-ms.
        std::thread::sleep(Duration::from_millis(backoff_ms));
        backoff_ms = (backoff_ms * 2).min(config.rejoin_max_ms.max(50));
        if outcome.played {
            backoff_ms = 50;
        }
    };

    keep.store(false, Ordering::SeqCst);
    if let Some(link) = &link {
        let _ = link.leaving();
    }
    if let Some(handle) = following {
        let _ = handle.join();
    }
    code
}

/// Where this endpoint should be playing from, in the order the module
/// documentation gives.
fn where_to_play(config: &ClientConfig, watch: &ZoneWatch) -> Result<String, String> {
    let facts = watch.facts();
    if facts.known && !facts.audio.is_empty() {
        return Ok(facts.audio);
    }
    let window = config.discover_ms.map(Duration::from_millis);
    let static_address = if config.server_configured {
        Some(config.server.as_str())
    } else {
        None
    };
    match locate(AUDIO_SERVICE, window, static_address) {
        Ok(located) => {
            status(&located.line());
            Ok(located.address().to_string())
        }
        Err(e) => Err(e.to_string()),
    }
}

/// What one session did.
struct SessionOutcome {
    code: ExitCode,
    played: bool,
    frames_played: u64,
    reason: String,
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
    let mut sink = match AlsaSink::open(
        &config.device,
        chorus_protocol::SampleFormat::PcmS16Le,
        2,
        48_000,
        config.device_buffer_us() as u32,
    ) {
        Ok(s) => s,
        Err(e) => {
            report("no usable audio device", &e.to_string());
            status(&format!("device-probe device={} usable=0 paces=0", config.device));
            return ExitCode::from(EXIT_DEVICE);
        }
    };

    // Does this device have a ring to report about? A device that accepts
    // frames instantly and reports a delay of zero forever - the ALSA `null`
    // device is exactly that - opens perfectly well and can verify nothing
    // about a reported delay. Saying so here is what lets a verification that
    // needs a real one refuse instead of reporting green.
    let probe_frames = 48_000 / 5; // 200 ms
    let silence = vec![0u8; probe_frames * sink.frame_len()];
    let delay_us = match sink.write(&silence).and_then(|_| sink.delay_frames()) {
        Ok(frames) => frames.max(0) * 1_000_000 / 48_000,
        Err(e) => {
            report("the audio device failed during the probe", &e.to_string());
            status(&format!("device-probe device={} usable=0 paces=0", config.device));
            return ExitCode::from(EXIT_DEVICE);
        }
    };
    let paces = delay_us > 0;
    let _ = sink.drain();

    status(&format!(
        "device-probe device={} usable=1 paces={} probe_delay_us={} frame_len={}",
        config.device,
        u8::from(paces),
        delay_us,
        sink.frame_len()
    ));
    if config.require_pacing && !paces {
        report(
            "the audio device reports no delay",
            &format!(
                "'{}' accepted 200 ms of audio and still reports a delay of {} us, so it has no \
                 ring to report about and cannot verify anything about a reported delay",
                config.device, delay_us
            ),
        );
        return ExitCode::from(EXIT_DEVICE);
    }
    ExitCode::SUCCESS
}

fn refused(code: u8, reason: &str) -> SessionOutcome {
    SessionOutcome {
        code: ExitCode::from(code),
        played: false,
        frames_played: 0,
        reason: reason.to_string(),
    }
}

fn play(
    config: &ClientConfig,
    server: &str,
    session: u64,
    timeline: MonotonicTimeline,
    watch: &Arc<ZoneWatch>,
) -> SessionOutcome {
    // The first session writes the configured log; a rejoin writes its own
    // beside it, so a run that rejoined leaves one record per session rather
    // than one record with the earlier ones written over.
    let delay_log = if session <= 1 {
        config.delay_log.clone()
    } else {
        format!("{}.session{}", config.delay_log, session)
    };
    status(&format!(
        "starting server={} device={} min_us={} max_us={} start_fill_us={} \
         device_target_us={} delay_log={} session={} zone={}",
        server,
        config.device,
        config.min_us,
        config.max_us,
        config.start_fill_us,
        config.device_target_us,
        delay_log,
        session,
        config.zone
    ));

    let stream = match TcpStream::connect(server) {
        Ok(s) => s,
        Err(e) => {
            report(
                "the server could not be reached at start",
                &format!("{}: {}", server, e),
            );
            status("stopped reason=server-unreachable played=0");
            return refused(EXIT_SERVER, "server-unreachable");
        }
    };
    if let Err(e) = stream.set_read_timeout(Some(Duration::from_millis(200))) {
        report("the connection could not be configured", &e.to_string());
        return refused(EXIT_SERVER, "connection-unconfigurable");
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
            return refused(code, "no-stream");
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
            return refused(EXIT_DEVICE, "device-unusable");
        }
    };

    let header = header_for(config, &config.device, &hand.shape);
    let mut log = match DelayLog::open(&delay_log, &header) {
        Ok(l) => l,
        Err(e) => {
            report(
                "the delay log could not be written",
                &format!("{}: {}", delay_log, e),
            );
            return refused(EXIT_LOG, "delay-log-unwritable");
        }
    };

    // The exchange goes back up the connection the audio came down, which is
    // the criterion's own wording and is also the only way the round trip it
    // measures is the round trip the audio takes.
    let sync_out: Option<Box<dyn std::io::Write>> = match stream.try_clone() {
        Ok(w) => Some(Box::new(w)),
        Err(e) => {
            report(
                "the connection could not be shared with the sync loop",
                &format!("{}; this run will have no offset and will say so", e),
            );
            None
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
        sync_out,
        Arc::clone(watch),
    ) {
        Ok(o) => o,
        Err(e) => {
            report("the delay log could not be written", &e.to_string());
            return refused(EXIT_LOG, "delay-log-unwritable");
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
    status(&format!("sync {}", outcome.telemetry.line()));
    status(&format!(
        "sync-frames inserted_frames={} dropped_frames={}",
        outcome.inserted_frames, outcome.dropped_frames
    ));
    status(&format!(
        "stopped reason={} played={} delay_log={}",
        outcome.stop.name(),
        u8::from(outcome.played_anything),
        delay_log
    ));
    status(&outcome.stop.describe());

    let code = match outcome.stop {
        StopReason::EndOfStream(_)
        | StopReason::RunLengthReached
        | StopReason::ZoneMoved { .. } => 0,
        StopReason::ConnectionLost { .. } | StopReason::NoStream => EXIT_SERVER,
        StopReason::Framing(_) => EXIT_FRAMING,
        StopReason::DeviceFailed(_) | StopReason::DelayRefused(_) => EXIT_DEVICE,
    };
    SessionOutcome {
        code: if code == 0 {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(code)
        },
        played: outcome.played_anything,
        frames_played: outcome.summary.frames_played,
        reason: outcome.stop.name().to_string(),
    }
}
