//! `chorus-client`: connect, buffer, play, and leave a record.
//!
//! # Exit codes, which are the contract a script grades
//!
//! - `0`  the stream ended with the in-band end-of-stream signal, or the
//!   configured run length was reached. Both are the run finishing the
//!   way it said it would.
//! - `2`  the configuration was refused.
//! - `3`  the server could not be reached at start, or the connection was lost
//!   during the run with no end-of-stream signal. The report says which.
//! - `4`  the audio device could not be opened, failed during the run, or
//!   refused to report its delay to the DAC. The last is its own reported
//!   reason, `delay-refused`, because it is the one signal the sync loop
//!   is entitled to use and there is no substitute for it.
//! - `5`  the client closed the session on a framing error.
//! - `6`  the delay log could not be written.
//! - `7`  this endpoint has no server address at all: discovery returned
//!   nothing, or was not attempted, and no static address was configured.
//!   The message says which of the two it lacked, because those are two
//!   different things to fix.
//!
//! A protocol v2 session that does not open (the server may speak v1, it
//! refused this endpoint's key, or its own key changed) exits `3` with the
//! refusal named on the `stopped` line; an identity that cannot be loaded, or
//! a playing run given no `--identity-dir` and no `--ephemeral-identity`,
//! exits `2`.
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
use chorus_client_linux::outmap::{self, MappedSink};
use chorus_client_linux::receive::{handshake, HandshakeError};
use chorus_client_linux::run::{counter_lines, header_for, run_session, StopReason};
use chorus_client_linux::session::{self, EndpointIdentity};
use chorus_client_linux::sink::{AlsaSink, PcmSink};
use chorus_client_linux::source::{
    self, AlsaCapture, SharedWriter, SignalThresholds, SourceHandle,
};
use chorus_client_linux::Counters;
use chorus_control::transport::Transport;
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
        // AC-8: an endpoint that cannot apply the playout latency its group is
        // held to STOPS, and says so on the line every verification reads, so
        // that "and plays nothing" is visible rather than inferred from an exit
        // code.
        if matches!(
            e,
            chorus_client_linux::config::ConfigError::WirelessPlayoutLatencyNotApplied { .. }
        ) {
            status("stopped reason=wireless-playout-latency-not-applied played=0 frames_played=0");
        }
        return ExitCode::from(EXIT_CONFIG);
    }

    match mode {
        ClientMode::ProbeDevice => probe_device(&config),
        ClientMode::ProbeLineIn => probe_line_in(&config),
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

    // Who this endpoint is: loaded once, so every session (and every rejoin)
    // presents the same key under the same id, and the server's pin is
    // checked against the same store.
    let mut identity = match load_identity(config) {
        Ok(i) => i,
        Err(e) => {
            report("configuration refused", &e);
            status("stopped reason=identity-refused played=0");
            keep.store(false, Ordering::SeqCst);
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    status(&format!(
        "identity id={} key={} store={}",
        config.endpoint_id(),
        identity.fingerprint(),
        config.identity_dir.as_deref().unwrap_or("ephemeral")
    ));

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
        let outcome = play(config, &address, session, timeline, &watch, &mut identity);
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
        if outcome.code == ExitCode::from(EXIT_CONFIG) || outcome.code == ExitCode::from(EXIT_LOG) {
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

/// This endpoint's protocol v2 identity, from `--identity-dir`, or made for
/// this process alone with `--ephemeral-identity`, or refused by name.
fn load_identity(config: &ClientConfig) -> Result<EndpointIdentity, String> {
    let id = config.endpoint_id();
    if config.ephemeral_identity {
        return EndpointIdentity::ephemeral(id);
    }
    match &config.identity_dir {
        Some(dir) => EndpointIdentity::load(std::path::Path::new(dir), id),
        None => Err(
            "this endpoint has no identity to present: pass --identity-dir <dir> so its key and \
             its server's pin survive a restart (the server pins this endpoint's key to its id), \
             or --ephemeral-identity for a throwaway run"
                .to_string(),
        ),
    }
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
        outmap::device_channels(config.output_map.as_ref(), 2),
        48_000,
        config.device_buffer_us() as u32,
    ) {
        Ok(s) => s,
        Err(e) => {
            report("no usable audio device", &e.to_string());
            status(&format!(
                "device-probe device={} usable=0 paces=0",
                config.device
            ));
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
            status(&format!(
                "device-probe device={} usable=0 paces=0",
                config.device
            ));
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

/// Open the configured line-in, capture 200 ms of it, say what the device
/// reported, exit. Plays nothing and sends nothing; it is how an operator (and
/// `make verify-alsa-null`, on ALSA's `null` capture device) checks a capture
/// device opens, delivers frames and reports its delay, without a server.
fn probe_line_in(config: &ClientConfig) -> ExitCode {
    use chorus_client_linux::source::{CaptureSource, SignalDetector};
    let Some(input) = config.line_in.as_ref() else {
        report("configuration refused", "--probe-line-in needs --line-in");
        return ExitCode::from(EXIT_CONFIG);
    };
    let unusable = |detail: &str| {
        report("no usable line-in", detail);
        status(&format!("line-in-probe device={} usable=0", input.device));
        ExitCode::from(EXIT_DEVICE)
    };
    let mut capture = match AlsaCapture::open(input) {
        Ok(c) => c,
        Err(e) => return unusable(&e.to_string()),
    };
    let frames = (input.rate_hz / 5) as usize; // 200 ms
    let mut pcm = vec![0u8; frames * capture.frame_len()];
    let read = match capture.read(&mut pcm) {
        Ok(r) => r,
        Err(e) => return unusable(&e.to_string()),
    };
    let delay = match capture.delay_frames() {
        Ok(Some(d)) => d.to_string(),
        Ok(None) => "overrun".to_string(),
        Err(e) => return unusable(&e.to_string()),
    };
    let mut detector = SignalDetector::new(
        SignalThresholds::default(),
        input.rate_hz,
        input.channels,
        input.sample_format,
    );
    detector.push(&pcm);
    status(&format!(
        "line-in-probe device={} usable=1 frames_read={} overran={} delay_frames={} signal={} \
         frame_len={}",
        input.device,
        read.frames,
        u8::from(read.overran),
        delay,
        u8::from(detector.present()),
        capture.frame_len()
    ));
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
    identity: &mut EndpointIdentity,
) -> SessionOutcome {
    // The first session writes the configured log; a rejoin writes its own
    // beside it, so a run that rejoined leaves one record per session rather
    // than one record with the earlier ones written over.
    let delay_log = if session <= 1 {
        config.delay_log.clone()
    } else {
        format!("{}.session{}", config.delay_log, session)
    };
    // The tier rides on the line that already says what this run is configured
    // with, so one grep answers "what is this endpoint held to" for a wired and
    // a wireless run alike.
    //
    // The power-save fields carry the SAME KEYS the ESP32 endpoint publishes
    // (`firmware/src/telemetry.c`), so one grep reaches both endpoints rather
    // than one of them. The ANSWER differs and is meant to: this endpoint
    // declares no mode and reads none back, so it says `unknown` on a wireless
    // link and `not-applicable` on a wired one, which is what the firmware line
    // says about a link with no radio in it. Turning Linux modem power save off
    // needs privilege and a different authority than the one this phase cites,
    // which is the ESP-IDF default; a Linux endpoint in a wireless zone gets the
    // deeper buffer and is honest about not knowing what its own radio is doing.
    let wifi_ps = if config.transport == Transport::Wireless {
        "unknown"
    } else {
        "not-applicable"
    };
    status(&format!(
        "starting server={} device={} min_us={} max_us={} start_fill_us={} \
         device_target_us={} delay_log={} session={} zone={} transport={} bound_us={} \
         playout_latency_us={} wifi_ps_declared={} wifi_ps_in_force={}",
        server,
        config.device,
        config.min_us,
        config.max_us,
        config.start_fill_us,
        config.device_target_us,
        delay_log,
        session,
        config.zone,
        config.transport,
        config.transport.bound_us(),
        config.sync.playout_latency_ns / 1_000,
        wifi_ps,
        wifi_ps
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

    // Protocol v2: the encrypted session, this endpoint's hello and
    // capabilities, and the server's key checked against its pin. Nothing is
    // read as audio before this succeeds.
    let secure = match session::open(stream, identity, config) {
        Ok(s) => s,
        Err(e) => {
            report("the session was refused", &e.to_string());
            status(&format!("stopped reason={} played=0", e.reason()));
            return refused(EXIT_SERVER, &e.reason());
        }
    };
    status(&format!(
        "session server_id={} server_key={} pinned_now={}",
        secure.server_id,
        secure.server_key,
        u8::from(secure.pinned_now)
    ));
    let session::Session {
        reader: mut stream,
        writer,
        announced,
        source_control,
        ..
    } = secure;
    // One writer for the session, shared by the time-sync exchange and the
    // source role; records never interleave inside a frame.
    let writer = SharedWriter::new(writer);
    let counters = Arc::new(Counters::new());
    // The source role runs for the whole session, beside the playout, and is
    // stopped (with `stream_end` for a started input) whichever way the
    // session ends.
    let _source = SourceGuard(config.line_in.as_ref().and_then(|input| {
        start_source(config, input, &writer, &counters, timeline, source_control)
    }));

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
    // The first chunk has to be what the server announced.
    let announcement = match announced.lock() {
        Ok(g) => g.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    };
    if let Err(e) = session::check_announcement(&announcement, &hand.shape) {
        report("the session was closed", &e);
        status("stopped reason=framing-announcement-mismatch played=0");
        return refused(EXIT_FRAMING, "framing-announcement-mismatch");
    }
    status(&format!(
        "stream-format announced codec={} output_delay_ns={}",
        announcement
            .stream_format
            .as_ref()
            .map(|f| f.codec.name())
            .unwrap_or("pcm"),
        announcement.output_delay_ns.unwrap_or(0)
    ));
    status(&format!(
        "stream rate_hz={} channels={} sample_format={} frames_per_chunk={}",
        hand.shape.sample_rate_hz,
        hand.shape.channels,
        hand.shape.sample_format.name(),
        hand.shape.frames_per_chunk
    ));

    // With an output map the device is opened with the MAP's channel count
    // and the stream is remapped at its edge (`outmap.rs`); without one, the
    // stream's own channels, as always.
    let device_channels = outmap::device_channels(config.output_map.as_ref(), hand.shape.channels);
    let alsa = match AlsaSink::open(
        &config.device,
        hand.shape.sample_format,
        device_channels,
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

    let mut clipped = None;
    let mut sink: Box<dyn PcmSink> = match &config.output_map {
        None => Box::new(alsa),
        Some(map) => {
            let stream_map = announcement
                .stream_format
                .as_ref()
                .map(|f| f.channel_map.clone())
                .unwrap_or_default();
            match MappedSink::new(alsa, map, &stream_map, hand.shape.sample_format) {
                Ok((mapped, resolved)) => {
                    // What each device channel plays, and anything the stream
                    // lacks, said before the first frame: nothing is silenced
                    // or substituted without a line saying so.
                    for line in resolved.report() {
                        status(line);
                    }
                    clipped = Some(mapped.clip_counter());
                    Box::new(mapped)
                }
                Err(e) => {
                    report("the output map does not fit the device", &e.to_string());
                    status(&format!(
                        "stopped reason=device-unusable device={} played=0",
                        config.device
                    ));
                    return refused(EXIT_DEVICE, "device-unusable");
                }
            }
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
    // measures is the round trip the audio takes. It is sealed like
    // everything else in the session, and nothing else writes on it.
    let sync_out: Option<Box<dyn std::io::Write>> = Some(Box::new(writer.clone()));

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
    if let Some(clipped) = &clipped {
        status(&format!(
            "output-map clipped_samples={}",
            clipped.load(Ordering::Relaxed)
        ));
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

/// Stops the source role when a session ends, however it ends, and says what
/// it did.
struct SourceGuard(Option<SourceHandle>);

impl Drop for SourceGuard {
    fn drop(&mut self) {
        if let Some(handle) = self.0.take() {
            let (stop, stats) = handle.stop();
            status(&stats.line());
            status(&format!("source stopped reason={}", stop.name()));
        }
    }
}

/// Open the configured line-in and run the source role on it. A capture
/// device that cannot be opened is reported by name and the endpoint goes on
/// playing: its first offer already said there is no signal, which stays true.
fn start_source(
    config: &ClientConfig,
    input: &chorus_client_linux::config::LineInConfig,
    writer: &SharedWriter<TcpStream>,
    counters: &Arc<Counters>,
    timeline: MonotonicTimeline,
    controls: std::sync::mpsc::Receiver<chorus_protocol::v2::SourceControl>,
) -> Option<SourceHandle> {
    let capture = match AlsaCapture::open(input) {
        Ok(c) => c,
        Err(e) => {
            report(
                "the configured line-in could not be opened",
                &format!("{}: {}", input.device, e),
            );
            status(&format!(
                "source unusable device={} source_id={}",
                input.device, input.source_id
            ));
            return None;
        }
    };
    status(&format!(
        "source offered source_id={} kind={} name=\"{}\" device={} rate_hz={} channels={} \
         sample_format={}",
        input.source_id,
        input.kind.name(),
        input.name,
        input.device,
        input.rate_hz,
        input.channels,
        input.sample_format.name()
    ));
    Some(source::spawn(
        capture,
        writer.clone(),
        source::SourceSetup {
            input: input.clone(),
            listed_codecs: session::capabilities(config).codecs,
            clock: Box::new(move || timeline.now_ns()),
            counters: Arc::clone(counters),
            controls,
            thresholds: SignalThresholds::default(),
            log: Box::new(|line| status(line)),
        },
    ))
}
