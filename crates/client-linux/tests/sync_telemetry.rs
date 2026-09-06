//! What a client publishes about its own timing health.
//!
//! Two of these run the REAL client session - `run_session`, over a real
//! loopback socket, against a scripted server - so what is graded is what a
//! client publishes and not what a unit test could be persuaded to say. The
//! device under it is modelled; `tests/common/mod.rs` says what that is and is
//! not evidence for.

mod common;

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use chorus_audio::MonotonicTimeline;
use chorus_client_linux::config::ClientConfig;
use chorus_client_linux::delaylog::DelayLog;
use chorus_client_linux::receive::handshake;
use chorus_client_linux::run::{header_for, run_session};
use chorus_client_linux::sink::PcmSink;
use chorus_client_linux::sync::{SyncConfig, SyncLoop};
use chorus_client_linux::Counters;
use chorus_protocol::{decode_frame, encode, FrameOutcome, Message, TimeSync};

use common::{chunk_frame, ModelParams, ModelledEndpoint};

const CHUNK_US: u64 = 20_000;
const FRAMES_PER_CHUNK: usize = 960;
const RATE_HZ: u32 = 48_000;
const FRAME_LEN: usize = 4;

// -------------------------------------------------------------------------
// AC-4: the bound is half the round trip of the sample the offset came from.
// -------------------------------------------------------------------------

fn exchange(t0_ns: u64, rtt_ns: u64, forward_share: f64, true_offset_ns: i64) -> TimeSync {
    let forward = (rtt_ns as f64 * forward_share) as u64;
    let back = rtt_ns - forward;
    let turnaround = 1_000u64;
    let t1 = (t0_ns as i64 + true_offset_ns) as u64 + forward;
    TimeSync {
        t0_ns,
        t1_ns: t1,
        t2_ns: t1 + turnaround,
        t3_ns: t0_ns + forward + turnaround + back,
    }
}

#[test]
fn the_bound_is_half_the_round_trip_of_the_sample_the_offset_came_from() {
    // RFC 5905 section 4: "The synchronization distance (LAMBDA) equal to
    // EPSILON + DELTA / 2 represents the maximum error due to all causes", and
    // section 11.1: "lambda = (delta / 2) + epsilon."
    //
    // The sample the offset came from is the one the minimum-round-trip filter
    // selected, which is deliberately neither the newest nor the oldest here.
    let mut loop_ = SyncLoop::new(SyncConfig::default());
    loop_.offer(&exchange(1_000_000_000, 3_000_000, 0.9, 250_000));
    loop_.offer(&exchange(2_000_000_000, 400_000, 0.5, 250_000));
    loop_.offer(&exchange(3_000_000_000, 1_200_000, 0.1, 250_000));

    let telemetry = loop_.telemetry(3_500_000_000);
    assert_eq!(
        telemetry.round_trip_ns,
        Some(400_000),
        "the round trip published has to be the selected sample's"
    );
    assert_eq!(
        telemetry.bound_ns,
        Some(200_000),
        "and the bound is half of it"
    );
    assert_eq!(
        telemetry.bound_ns.unwrap() * 2,
        telemetry.round_trip_ns.unwrap(),
        "half, exactly"
    );
    assert!(telemetry.line().contains("round_trip_ns=400000"));
    assert!(telemetry.line().contains("bound_ns=200000"));

    // The bound really does bound. RFC 5905 section 4 bounds the error of the
    // SAMPLE, so that is what is checked: the exchange the bound was taken
    // from is within its own bound of the truth, and so is the worst-case
    // exchange of that round trip, which is the one with all the asymmetry in
    // one direction.
    let selected = exchange(2_000_000_000, 400_000, 0.5, 250_000);
    assert!(
        (selected.offset_ns() - 250_000).abs() <= telemetry.bound_ns.unwrap() as i64,
        "the selected sample estimated {} ns against a true 250000 ns, bound {} ns",
        selected.offset_ns(),
        telemetry.bound_ns.unwrap()
    );
    for share in [0.0f64, 0.25, 0.5, 0.75, 1.0] {
        let worst = exchange(2_000_000_000, 400_000, share, 250_000);
        assert!(
            (worst.offset_ns() - 250_000).abs() <= 200_000,
            "an exchange of the same 400 us round trip with {} of the delay in one direction \
             estimated {} ns, which is outside its own bound",
            share,
            worst.offset_ns()
        );
    }

    // A newer, worse exchange does not become the bound just by being newer.
    loop_.offer(&exchange(4_000_000_000, 8_000_000, 0.2, 250_000));
    let after = loop_.telemetry(4_500_000_000);
    assert_eq!(after.round_trip_ns, Some(400_000));
    assert_eq!(after.bound_ns, Some(200_000));

    // A better one does.
    loop_.offer(&exchange(5_000_000_000, 100_000, 0.5, 250_000));
    let better = loop_.telemetry(5_500_000_000);
    assert_eq!(better.round_trip_ns, Some(100_000));
    assert_eq!(better.bound_ns, Some(50_000));
}

#[test]
fn the_bound_tracks_the_window_over_a_modelled_run() {
    let mut endpoint = ModelledEndpoint::new(ModelParams::default(), SyncConfig::default());
    let result = endpoint.run(5 * 60_000_000_000);
    let telemetry = result.telemetry;
    let rtt = telemetry
        .round_trip_ns
        .expect("thousands of exchanges were accepted");
    assert_eq!(telemetry.bound_ns, Some(rtt / 2));
    // The modelled quiet segment is 120 us of base delay each way plus at most
    // 60 us of queuing, so the least queued exchange in the window is close to
    // 240 us and its half is close to 120 us. A bound far outside that would
    // mean the filter is not selecting what it says it selects.
    assert!(
        (240_000..=300_000).contains(&rtt),
        "the selected round trip was {} ns against a modelled 240 us floor",
        rtt
    );
    assert!(!telemetry.stale);
    assert!(telemetry.offset_ns.is_some());
}

// -------------------------------------------------------------------------
// AC-14: no exchange yet means no offset and no bound, and never a zero.
// -------------------------------------------------------------------------

#[test]
fn a_client_that_has_run_no_exchange_publishes_no_offset_and_no_bound() {
    let loop_ = SyncLoop::new(SyncConfig::default());
    let telemetry = loop_.telemetry(12_345_678);
    assert_eq!(telemetry.offset_ns, None);
    assert_eq!(telemetry.round_trip_ns, None);
    assert_eq!(telemetry.bound_ns, None);
    assert_eq!(telemetry.age_ns, None);
    assert_eq!(telemetry.accepted, 0);
    let line = telemetry.line();
    for field in ["offset_ns=none", "round_trip_ns=none", "bound_ns=none"] {
        assert!(line.contains(field), "{} is missing from {}", field, line);
    }
    for wrong in ["offset_ns=0", "round_trip_ns=0", "bound_ns=0"] {
        assert!(
            !line.contains(wrong),
            "a client with no offset published {} in {}",
            wrong,
            line
        );
    }
}

#[test]
fn exchanges_that_were_all_discarded_still_count_as_no_exchange() {
    // "No exchange has SUCCEEDED" and "no exchange has been tried" are
    // different states of the world and the same answer: nothing was learned,
    // so there is nothing to publish.
    let mut loop_ = SyncLoop::new(SyncConfig::default());
    for i in 1..=8u64 {
        loop_.offer(&TimeSync {
            t0_ns: i * 1_000_000_000,
            t1_ns: 9_000_000_000,
            t2_ns: 9_900_000_000,
            t3_ns: i * 1_000_000_000 + 200_000,
        });
    }
    let telemetry = loop_.telemetry(9_000_000_000);
    assert_eq!(telemetry.discarded, 8);
    assert_eq!(telemetry.accepted, 0);
    assert_eq!(telemetry.offset_ns, None);
    assert_eq!(telemetry.bound_ns, None);
    assert!(telemetry.line().contains("bound_ns=none"));
}

// -------------------------------------------------------------------------
// The real client session, over a real socket, against a scripted server.
// -------------------------------------------------------------------------

/// What the scripted server does about a time-sync request.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Answering {
    /// Answer every request, with the four timestamps of the exchange.
    Yes,
    /// Never answer one. Audio keeps flowing.
    No,
}

/// Serve `chunks` chunks of audio on a loopback socket, optionally answering
/// time-sync requests, and give back the address to connect to.
fn scripted_server(chunks: u32, answering: Answering, stop: Arc<AtomicBool>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let address = listener.local_addr().unwrap().to_string();
    thread::spawn(move || {
        let (stream, _) = listener.accept().expect("the client connects");
        stream
            .set_read_timeout(Some(Duration::from_millis(2)))
            .unwrap();
        let mut reader = stream.try_clone().unwrap();
        let mut writer = stream;
        let timeline = MonotonicTimeline::new();
        let origin_ns = timeline.now_ns();
        let mut pending: Vec<u8> = Vec::new();
        let mut scratch = [0u8; 4_096];
        let mut sent = 0u32;

        while sent < chunks && !stop.load(Ordering::SeqCst) {
            // Pace the chunks against the server's own monotonic timeline.
            let due_ns = origin_ns + u64::from(sent) * CHUNK_US * 1_000;
            if timeline.now_ns() >= due_ns {
                let frame = chunk_frame(sent, due_ns, FRAMES_PER_CHUNK);
                if writer.write_all(&frame).is_err() {
                    return;
                }
                let _ = writer.flush();
                sent += 1;
                continue;
            }

            match reader.read(&mut scratch) {
                Ok(0) => return,
                Ok(n) => pending.extend_from_slice(&scratch[..n]),
                Err(_) => {
                    thread::sleep(Duration::from_millis(1));
                    continue;
                }
            }
            let mut at = 0usize;
            loop {
                let result = decode_frame(&pending[at..]);
                if result.consumed == 0 {
                    break;
                }
                at += result.consumed;
                if let FrameOutcome::Decoded(Message::TimeSync(request)) = result.outcome {
                    if answering == Answering::No {
                        continue;
                    }
                    let t1_ns = timeline.now_ns();
                    let reply = TimeSync {
                        t0_ns: request.t0_ns,
                        t1_ns,
                        t2_ns: timeline.now_ns(),
                        t3_ns: 0,
                    };
                    let frame = encode(&Message::TimeSync(reply)).unwrap();
                    if writer.write_all(&frame).is_err() {
                        return;
                    }
                    let _ = writer.flush();
                }
            }
            pending.drain(..at);
        }
    });
    address
}

fn client_config(log: &std::path::Path, run_seconds: u64, sync: SyncConfig) -> ClientConfig {
    ClientConfig {
        server: "unused".to_string(),
        device: "modelled".to_string(),
        min_us: 60_000,
        max_us: 300_000,
        start_fill_us: 120_000,
        device_target_us: 120_000,
        delay_log: log.to_string_lossy().into_owned(),
        run_seconds: Some(run_seconds),
        overflow_skew_ppm: 2_000,
        require_pacing: false,
        sync,
    }
}

fn temp_path(name: &str) -> std::path::PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!("chorus-{}-{}.log", name, std::process::id()));
    dir
}

/// Run the real client session against the scripted server, on a device that
/// behaves and the configuration the client ships with.
fn run_against(answering: Answering, name: &str) -> (chorus_client_linux::RunOutcome, String) {
    let device = common::ModelledDevice::new("modelled", RATE_HZ, FRAME_LEN, 400_000);
    run_against_device(answering, name, SyncConfig::default(), device)
}

/// The same, on a device and a sync configuration the caller chooses.
fn run_against_device(
    answering: Answering,
    name: &str,
    sync: SyncConfig,
    mut device: common::ModelledDevice,
) -> (chorus_client_linux::RunOutcome, String) {
    let stop = Arc::new(AtomicBool::new(false));
    let address = scripted_server(2_000, answering, Arc::clone(&stop));
    let mut stream = TcpStream::connect(&address).expect("the scripted server is listening");
    stream
        .set_read_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    let sync_out: Box<dyn Write> = Box::new(stream.try_clone().unwrap());

    let hand = handshake(&mut stream, &|| true).expect("the stream starts");
    let path = temp_path(name);
    let config = client_config(&path, 4, sync);
    config.validate().expect("the test configuration is valid");
    let header = header_for(&config, device.device(), &hand.shape);
    let mut log = DelayLog::open(&path, &header).expect("the log opens");
    let counters = Arc::new(Counters::new());
    let outcome = run_session(
        &config,
        stream,
        hand,
        &mut device,
        &mut log,
        MonotonicTimeline::new(),
        counters,
        Some(sync_out),
    )
    .expect("the run writes its log");
    drop(log);
    stop.store(true, Ordering::SeqCst);
    let text = std::fs::read_to_string(&path).expect("the log is readable");
    let _ = std::fs::remove_file(&path);
    (outcome, text)
}

#[test]
fn a_real_client_run_publishes_the_offset_its_round_trip_and_half_of_it() {
    let (outcome, log) = run_against(Answering::Yes, "telemetry-answered");
    let telemetry = outcome.telemetry;

    assert!(
        telemetry.accepted >= 2,
        "four seconds at two exchanges a second should accept several, got {}",
        telemetry.accepted
    );
    let offset = telemetry
        .offset_ns
        .expect("the server answered, so there is an offset");
    let rtt = telemetry.round_trip_ns.expect("and a round trip");
    let bound = telemetry.bound_ns.expect("and a bound");
    assert_eq!(bound, rtt / 2, "the bound is half the round trip");
    assert!(
        rtt < 100_000_000,
        "a loopback round trip of {} ns is not a loopback round trip",
        rtt
    );
    // Both clocks are this process's own `Instant` epochs, minutes apart at
    // most, so the offset is a real number rather than a placeholder.
    assert!(offset.abs() < 60_000_000_000);

    // And the run wrote it down, so a reader who was not here can check.
    assert!(
        log.contains("kind=sync "),
        "the delay log carries no sync telemetry"
    );
    assert!(log.contains("bound_ns="));
    assert!(
        log.contains("kind=sync-exchange accepted=1"),
        "the log does not record an accepted exchange"
    );
    assert!(outcome.played_anything);
}

#[test]
fn a_real_client_whose_server_never_answers_publishes_no_offset_and_corrects_nothing() {
    let (outcome, log) = run_against(Answering::No, "telemetry-unanswered");
    let telemetry = outcome.telemetry;

    assert_eq!(telemetry.accepted, 0, "nothing was ever answered");
    assert_eq!(telemetry.offset_ns, None);
    assert_eq!(telemetry.round_trip_ns, None);
    assert_eq!(telemetry.bound_ns, None);
    assert_eq!(
        outcome.inserted_frames, 0,
        "no correction at all was applied"
    );
    assert_eq!(outcome.dropped_frames, 0);
    assert_eq!(telemetry.correction_ppm, 0.0);
    assert_eq!(telemetry.hard_resyncs, 0);

    assert!(
        outcome.played_anything,
        "audio still played; the client is not blocked on an answer"
    );
    assert!(
        log.contains("offset_ns=none") && log.contains("bound_ns=none"),
        "the log has to say `none` and not a zero"
    );
    assert!(
        !log.contains("kind=correction"),
        "a client with no offset applied a correction"
    );
}

#[test]
fn a_real_client_whose_device_reports_no_delay_still_publishes_a_stale_offset() {
    // AC-15's third conjunct through the shipped session rather than the loop
    // alone. The device reports a delay of zero for ever, which is what the
    // ALSA `null` device does and what `make verify-null-device` runs the
    // shipped binaries against: nothing may be corrected against it, and the
    // offset's age has to be published for what it is anyway.
    //
    // The staleness limit is shortened to 200 ms because a session that runs
    // for four seconds cannot outlive the shipped 10 s one. Nothing else about
    // the run differs from the two above.
    let mut device = common::ModelledDevice::new("modelled-null", RATE_HZ, FRAME_LEN, 400_000);
    device.reports_zero_delay();
    let sync = SyncConfig {
        staleness_limit_ns: 200_000_000,
        ..SyncConfig::default()
    };
    let (outcome, log) = run_against_device(Answering::Yes, "telemetry-stale-null", sync, device);
    let telemetry = outcome.telemetry;

    assert!(
        telemetry.accepted >= 2,
        "the server answered, so there is an offset for the run to age: {}",
        telemetry.line()
    );
    assert!(
        outcome.played_anything,
        "audio kept playing while the offset went stale"
    );
    assert_eq!(
        outcome.inserted_frames, 0,
        "a delay of zero is not a signal, so nothing is corrected against it"
    );
    assert_eq!(outcome.dropped_frames, 0);
    assert!(
        !log.contains("kind=correction"),
        "the run corrected against a device that reported no delay"
    );
    assert!(
        log.contains("kind=sync-no-device-delay"),
        "the log does not record the device's zero"
    );

    // The published line, which is what a consumer keys on.
    assert!(
        log.lines()
            .any(|line| line.contains("kind=sync ") && line.contains("stale=1")),
        "no published telemetry line reports the offset as stale:\n{}",
        log.lines()
            .filter(|line| line.contains("kind=sync "))
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        log.contains("kind=sync-stale"),
        "the log's own stale event never fired, so the run's event stream and its published \
         lines disagree about the same fact"
    );
}
