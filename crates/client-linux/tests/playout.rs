//! The client's decisions, driven against a modelled device.
//!
//! Read `tests/common/mod.rs` first: it says exactly what a modelled device is
//! and is not evidence for. In one line: these tests are evidence about the
//! client's buffering, accounting, drains and exits, and evidence about
//! nothing that happens inside a real sound card.

mod common;

use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;

use chorus_audio::MonotonicTimeline;
use chorus_client_linux::config::ClientConfig;
use chorus_client_linux::delaylog::DelayLog;
use chorus_client_linux::logcheck::{grade, grade_no_rate_change, grade_underruns, parse};
use chorus_client_linux::receive::handshake;
use chorus_client_linux::run::{header_for, run_session, StopReason};
use chorus_client_linux::Counters;

use common::{end_frame, paced_chunks, ModelledDevice};

const CHUNK_US: u64 = 20_000;
const FRAMES_PER_CHUNK: usize = 960;
const RATE_HZ: u32 = 48_000;
const FRAME_LEN: usize = 4;

fn config(tmp: &std::path::Path, run_seconds: Option<u64>) -> ClientConfig {
    ClientConfig {
        server: "unused".to_string(),
        device: "modelled".to_string(),
        min_us: 60_000,
        max_us: 300_000,
        start_fill_us: 120_000,
        device_target_us: 120_000,
        delay_log: tmp.to_string_lossy().into_owned(),
        run_seconds,
        overflow_skew_ppm: 2_000,
        require_pacing: false,
        sync: chorus_client_linux::SyncConfig::default(),
        ..Default::default()
    }
}

fn temp_path(name: &str) -> std::path::PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "chorus-{}-{}-{}.log",
        name,
        std::process::id(),
        std::time::Instant::now().elapsed().as_nanos()
    ));
    dir
}

struct Ran {
    outcome: chorus_client_linux::RunOutcome,
    log_text: String,
    device_underruns: u64,
}

fn run<R: std::io::Read + Send + 'static>(
    name: &str,
    source: R,
    run_seconds: Option<u64>,
    ring_us: u64,
) -> Ran {
    let path = temp_path(name);
    let config = config(&path, run_seconds);
    config.validate().expect("the test configuration is valid");

    let mut source = source;
    let hand = handshake(&mut source, &|| true).expect("the stream starts");
    let mut device = ModelledDevice::new("modelled", RATE_HZ, FRAME_LEN, ring_us);
    let header = header_for(&config, "modelled", &hand.shape);
    let mut log = DelayLog::open(&path, &header).expect("the log opens");
    let counters = Arc::new(Counters::new());
    let outcome = run_session(
        &config,
        source,
        hand,
        &mut device,
        &mut log,
        MonotonicTimeline::new(),
        Arc::clone(&counters),
        // No write half: these tests are about the playout path, and a loop
        // with nobody to exchange with has no offset and corrects nothing,
        // which is exactly the behaviour they were written against.
        None,
        Arc::new(chorus_client_linux::ZoneWatch::new()),
    )
    .expect("the run writes its log");
    drop(log);
    let log_text = std::fs::read_to_string(&path).expect("the log is readable");
    let _ = std::fs::remove_file(&path);
    Ran {
        outcome,
        log_text,
        device_underruns: {
            use chorus_client_linux::sink::PcmSink;
            let _ = device.frames_played();
            device.underruns()
        },
    }
}

#[test]
fn output_is_withheld_until_the_start_fill_and_the_log_names_the_fill_it_started_at() {
    let source = paced_chunks(200, FRAMES_PER_CHUNK, CHUNK_US, 1.0, true, true);
    let ran = run("start-fill", source, Some(3), 400_000);
    let log = parse(&ran.log_text).expect("the log parses");

    let fill = log
        .events
        .iter()
        .find(|e| e.kind == "start-fill")
        .expect("the log names the fill it started at");
    assert_eq!(fill.fields.get("start_fill_us").map(String::as_str), Some("120000"));
    let filled: u64 = fill.fields["filled_us"].parse().unwrap();
    assert!(
        filled >= 120_000,
        "the first write carried {} us, less than the configured fill",
        filled
    );

    // Nothing is graded before that event, and the first graded sample is at
    // or after it.
    let first_graded = log
        .samples
        .iter()
        .find(|s| s.graded)
        .expect("something is graded");
    assert!(first_graded.mono_us >= fill.mono_us);
    for sample in log.samples.iter().filter(|s| s.mono_us < fill.mono_us) {
        assert!(!sample.graded, "a sample before the first write is graded");
    }
}

#[test]
fn a_rate_matched_run_keeps_the_reported_delay_inside_the_bounds_and_reports_no_underruns() {
    let source = paced_chunks(1_000, FRAMES_PER_CHUNK, CHUNK_US, 1.0, true, true);
    let ran = run("steady", source, Some(4), 400_000);
    let log = parse(&ran.log_text).expect("the log parses");

    let mut report = grade(&log, 2);
    grade_underruns(&log, &mut report);
    grade_no_rate_change(&log, 4_800, &mut report);
    assert!(report.ok(), "{}\n{}", report, ran.log_text);
    assert_eq!(ran.outcome.summary.underruns, 0);
    assert_eq!(ran.device_underruns, 0, "the device never ran dry either");
}

#[test]
fn a_source_faster_than_the_sink_crosses_the_maximum_and_the_crossing_is_reported() {
    // Three times faster than real time. The shipped configuration is sized
    // so that the deliberate 2000 ppm difference crosses the span inside a
    // ten-minute run; here the difference is made far larger so that the same
    // behaviour is exercised in seconds and cannot be lost to a loaded
    // machine. What is being asserted is what happens AT the ceiling, which
    // does not depend on how fast it was reached.
    let source = paced_chunks(4_000, FRAMES_PER_CHUNK, CHUNK_US, 0.3, true, true);
    let ran = run("overflow", source, Some(4), 400_000);
    let log = parse(&ran.log_text).expect("the log parses");

    let peak = log.samples.iter().map(|s| s.occupancy_us).max().unwrap_or(0);
    let crossing = log
        .events
        .iter()
        .find(|e| e.kind == "bound-crossing")
        .unwrap_or_else(|| {
            panic!(
                "the crossing at the maximum was never reported; peak occupancy was {} us of a \
                 300000 us maximum, discarded_overflow={}, zones={:?}",
                peak,
                ran.outcome.summary.discarded_overflow,
                log.events
                    .iter()
                    .filter(|e| e.kind == "zone")
                    .map(|e| e.fields["zone"].clone())
                    .collect::<Vec<_>>()
            )
        });
    assert_eq!(crossing.fields.get("bound").map(String::as_str), Some("maximum"));
    assert!(ran.outcome.summary.discarded_overflow > 0);

    // The overflow counter is its own, and moving it moves nothing else.
    assert_eq!(ran.outcome.summary.underruns, 0);
    assert_eq!(ran.outcome.summary.discarded_late, 0);
    assert_eq!(ran.outcome.summary.discarded_duplicate, 0);
    assert_eq!(ran.outcome.summary.discarded_malformed, 0);

    // Occupancy stays under the maximum plus one chunk for the whole run.
    let cap = 300_000 + CHUNK_US;
    for sample in &log.samples {
        assert!(
            sample.occupancy_us <= cap,
            "occupancy reached {} us, past {} us",
            sample.occupancy_us,
            cap
        );
    }

    // Playback continued, and at the device's nominal rate: no resampling and
    // no rate change happened in response.
    let mut report = grade(&log, 0);
    grade_no_rate_change(&log, 4_800, &mut report);
    let rate = report
        .findings
        .iter()
        .find(|f| f.check == "no-rate-change")
        .unwrap();
    assert!(rate.ok, "{}", report);
}

#[test]
fn a_stalled_feed_underruns_the_device_and_the_counter_moves_without_widening_a_bound() {
    // Twenty chunks, then a long silence, then twenty more: the buffer runs
    // dry in the gap.
    let mut parts = Vec::new();
    for i in 0..20u32 {
        parts.push((
            Duration::from_micros(u64::from(i) * CHUNK_US),
            common::chunk_frame(i, u64::from(i) * CHUNK_US * 1_000, FRAMES_PER_CHUNK),
        ));
    }
    for i in 20..60u32 {
        parts.push((
            Duration::from_millis(1_500) + Duration::from_micros(u64::from(i - 20) * CHUNK_US),
            common::chunk_frame(i, u64::from(i) * CHUNK_US * 1_000, FRAMES_PER_CHUNK),
        ));
    }
    parts.push((
        Duration::from_millis(1_500) + Duration::from_micros(40 * CHUNK_US),
        end_frame(59, 60 * CHUNK_US * 1_000),
    ));
    let source = common::PacedReader::new(parts, true);

    let ran = run("underrun", source, Some(4), 400_000);
    let log = parse(&ran.log_text).expect("the log parses");

    assert!(
        ran.outcome.summary.underruns >= 1,
        "the stall drained the device but no underrun was counted: {}",
        ran.log_text
    );
    assert!(
        log.events.iter().any(|e| e.kind == "underrun"),
        "the underrun is reported in the log"
    );
    // The bounds in the log are the configured ones, unchanged.
    assert_eq!(log.config.get("min_us").map(String::as_str), Some("60000"));
    assert_eq!(log.config.get("max_us").map(String::as_str), Some("300000"));
    // Playback continued after the underrun.
    assert!(ran.outcome.played_anything);
    assert!(matches!(
        ran.outcome.stop,
        StopReason::EndOfStream(_) | StopReason::RunLengthReached
    ));
}

#[test]
fn a_clean_end_of_stream_plays_the_short_final_chunk_out_and_exits_clean() {
    let mut parts = Vec::new();
    for i in 0..30u32 {
        parts.push((
            Duration::from_micros(u64::from(i) * CHUNK_US),
            common::chunk_frame(i, u64::from(i) * CHUNK_US * 1_000, FRAMES_PER_CHUNK),
        ));
    }
    // A short final chunk, then the in-band signal.
    parts.push((
        Duration::from_micros(30 * CHUNK_US),
        common::chunk_frame(30, 30 * CHUNK_US * 1_000, 100),
    ));
    parts.push((
        Duration::from_micros(30 * CHUNK_US),
        end_frame(30, 31 * CHUNK_US * 1_000),
    ));
    let source = common::PacedReader::new(parts, true);

    let ran = run("clean-end", source, None, 400_000);
    match &ran.outcome.stop {
        StopReason::EndOfStream(end) => {
            assert_eq!(end.final_sequence, 30);
        }
        other => panic!("expected a clean end, got {:?}", other),
    }
    assert!(ran.outcome.stop.is_clean());
    assert!(ran.outcome.played_anything);
    // The whole stream reached the device, short final chunk included.
    assert_eq!(
        ran.outcome.summary.frames_written,
        30 * FRAMES_PER_CHUNK as u64 + 100
    );
    assert_eq!(ran.outcome.summary.underruns, 0, "a drain is not an underrun");

    // The graded interval closed when the end of stream arrived, not when the
    // play-out that followed it finished.
    let log = parse(&ran.log_text).expect("the log parses");
    let close = log
        .events
        .iter()
        .find(|e| e.kind == "graded-close")
        .expect("the log says where the graded interval closed");
    assert_eq!(
        close.fields.get("reason").map(String::as_str),
        Some("end-of-stream")
    );
    for sample in log.samples.iter().filter(|s| s.mono_us > close.mono_us) {
        assert!(
            !sample.graded,
            "a sample at {} us, after the end of stream at {} us, is still marked graded",
            sample.mono_us, close.mono_us
        );
    }
}

#[test]
fn a_connection_lost_without_the_signal_plays_out_what_is_held_and_exits_dirty() {
    let mut parts = Vec::new();
    for i in 0..30u32 {
        parts.push((
            Duration::from_micros(u64::from(i) * CHUNK_US),
            common::chunk_frame(i, u64::from(i) * CHUNK_US * 1_000, FRAMES_PER_CHUNK),
        ));
    }
    // No end-of-stream frame: the reader simply closes.
    let source = common::PacedReader::new(parts, true);

    let ran = run("lost", source, None, 400_000);
    match &ran.outcome.stop {
        StopReason::ConnectionLost { .. } => {}
        other => panic!("expected a lost connection, got {:?}", other),
    }
    assert!(!ran.outcome.stop.is_clean(), "a lost server is not a clean end");
    assert!(ran.outcome.played_anything, "what was held is played out");
    assert_eq!(
        ran.outcome.summary.frames_written,
        30 * FRAMES_PER_CHUNK as u64,
        "everything held reached the device"
    );
    assert_eq!(
        ran.outcome.summary.underruns, 0,
        "the deliberate play-out is not counted as underruns"
    );
}

/// Terms closes the graded interval at the EARLIEST of end of stream received,
/// the connection detected as lost, the client beginning to exit for any other
/// reason, and the end of the run. The play-out that follows a lost connection
/// is outside it, however long it takes - so the flag has to go down when the
/// connection is detected as lost, and not when the last held chunk has been
/// written.
#[test]
fn the_graded_interval_closes_when_the_connection_is_lost_not_when_the_play_out_ends() {
    let mut parts = Vec::new();
    for i in 0..30u32 {
        parts.push((
            Duration::from_micros(u64::from(i) * CHUNK_US),
            common::chunk_frame(i, u64::from(i) * CHUNK_US * 1_000, FRAMES_PER_CHUNK),
        ));
    }
    // No end-of-stream frame: the reader simply closes, and what is held is
    // played out afterwards.
    let source = common::PacedReader::new(parts, true);

    let ran = run("graded-close", source, None, 400_000);
    let log = parse(&ran.log_text).expect("the log parses");

    let close = log
        .events
        .iter()
        .find(|e| e.kind == "graded-close")
        .unwrap_or_else(|| panic!("the log never says where the graded interval closed:\n{}", ran.log_text));
    assert_eq!(
        close.fields.get("reason").map(String::as_str),
        Some("connection-lost"),
        "the interval closed for the wrong reason: {:?}",
        close
    );

    let after: Vec<_> = log
        .samples
        .iter()
        .filter(|s| s.mono_us > close.mono_us)
        .collect();
    assert!(
        !after.is_empty(),
        "the run recorded nothing after the connection was lost, so this asserts nothing:\n{}",
        ran.log_text
    );
    for sample in &after {
        assert!(
            !sample.graded,
            "a sample at {} us, after the connection was detected as lost at {} us, is still \
             marked graded:\n{}",
            sample.mono_us, close.mono_us, ran.log_text
        );
    }

    // And the play-out that followed the close was real: everything held
    // reached the device after the interval had already closed.
    assert_eq!(
        ran.outcome.summary.frames_written,
        30 * FRAMES_PER_CHUNK as u64,
        "everything held reached the device"
    );
}

#[test]
fn a_duplicate_sequence_and_a_malformed_frame_are_discarded_by_reason_and_the_session_survives() {
    let mut parts = Vec::new();
    for i in 0..10u32 {
        parts.push((
            Duration::from_micros(u64::from(i) * CHUNK_US),
            common::chunk_frame(i, u64::from(i) * CHUNK_US * 1_000, FRAMES_PER_CHUNK),
        ));
    }
    // A duplicate of sequence 5, and a chunk frame whose declared payload is
    // one byte short of a whole frame.
    parts.push((
        Duration::from_micros(10 * CHUNK_US),
        common::chunk_frame(5, 5 * CHUNK_US * 1_000, FRAMES_PER_CHUNK),
    ));
    let mut malformed = common::chunk_frame(11, 11 * CHUNK_US * 1_000, FRAMES_PER_CHUNK);
    let declared = u16::from_be_bytes([malformed[1], malformed[2]]) - 1;
    malformed[1..3].copy_from_slice(&declared.to_be_bytes());
    malformed.pop();
    parts.push((Duration::from_micros(10 * CHUNK_US), malformed));
    for i in 12..30u32 {
        parts.push((
            Duration::from_micros(u64::from(i) * CHUNK_US),
            common::chunk_frame(i, u64::from(i) * CHUNK_US * 1_000, FRAMES_PER_CHUNK),
        ));
    }
    parts.push((
        Duration::from_micros(30 * CHUNK_US),
        end_frame(29, 30 * CHUNK_US * 1_000),
    ));
    let source = common::PacedReader::new(parts, true);

    let ran = run("discards", source, None, 400_000);
    assert_eq!(
        ran.outcome.summary.discarded_duplicate, 1,
        "the duplicate sequence is counted as a duplicate"
    );
    assert_eq!(
        ran.outcome.summary.discarded_malformed, 1,
        "the malformed frame is counted as malformed"
    );
    assert_eq!(
        ran.outcome.summary.underruns, 0,
        "neither discard moved the underrun counter"
    );
    assert!(
        ran.outcome.stop.is_clean(),
        "the session survived both and ended on the in-band signal: {:?}",
        ran.outcome.stop
    );
}

#[test]
fn a_chunk_stamped_in_the_past_is_discarded_as_late_and_does_not_displace_the_chunk_that_was_due() {
    let mut parts = Vec::new();
    for i in 0..20u32 {
        parts.push((
            Duration::from_micros(u64::from(i) * CHUNK_US),
            common::chunk_frame(i, u64::from(i) * CHUNK_US * 1_000, FRAMES_PER_CHUNK),
        ));
    }
    // Sequence 20, stamped back at the very start of the stream.
    parts.push((
        Duration::from_millis(500),
        common::chunk_frame(20, 0, FRAMES_PER_CHUNK),
    ));
    for i in 21..40u32 {
        parts.push((
            Duration::from_millis(500) + Duration::from_micros(u64::from(i - 20) * CHUNK_US),
            common::chunk_frame(i, u64::from(i) * CHUNK_US * 1_000, FRAMES_PER_CHUNK),
        ));
    }
    parts.push((
        Duration::from_millis(500) + Duration::from_micros(20 * CHUNK_US),
        end_frame(39, 40 * CHUNK_US * 1_000),
    ));
    let source = common::PacedReader::new(parts, true);

    let ran = run("late", source, None, 400_000);
    assert_eq!(
        ran.outcome.summary.discarded_late, 1,
        "the chunk stamped in the past is counted as late"
    );
    assert_eq!(
        ran.outcome.summary.frames_written,
        39 * FRAMES_PER_CHUNK as u64,
        "the late chunk was not played in place of the chunk that was due"
    );
}

#[test]
fn a_device_that_goes_away_mid_run_stops_the_run_and_does_not_claim_playback() {
    let path = temp_path("device-loss");
    let config = config(&path, Some(4));
    let mut source = paced_chunks(1_000, FRAMES_PER_CHUNK, CHUNK_US, 1.0, true, true);
    let hand = handshake(&mut source, &|| true).expect("the stream starts");
    let mut device = ModelledDevice::new("modelled", RATE_HZ, FRAME_LEN, 400_000);
    let switch = device.failure_switch();
    let header = header_for(&config, "modelled", &hand.shape);
    let mut log = DelayLog::open(&path, &header).expect("the log opens");
    let counters = Arc::new(Counters::new());

    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(600));
        *switch.lock().unwrap() = Some(std::time::Instant::now());
    });

    let outcome = run_session(
        &config,
        source,
        hand,
        &mut device,
        &mut log,
        MonotonicTimeline::new(),
        counters,
        None,
        Arc::new(chorus_client_linux::ZoneWatch::new()),
    )
    .expect("the run writes its log");
    drop(log);
    let _ = std::fs::remove_file(&path);

    // A device that has gone refuses the first thing the playout loop asks it,
    // which is how far it is from its DAC. That is its own stop reason and it
    // names the device; see `StopReason::DelayRefused`.
    match &outcome.stop {
        StopReason::DelayRefused(refused) => {
            assert_eq!(refused.device, "modelled");
            assert!(refused.cause.to_string().contains("removed"), "{:?}", refused);
        }
        StopReason::DeviceFailed(e) => {
            assert!(e.to_string().contains("removed"), "{}", e);
        }
        other => panic!("expected a device failure, got {:?}", other),
    }
    assert!(!outcome.stop.is_clean(), "a dead device is not a clean end");
}

#[test]
fn a_stream_that_never_starts_is_not_a_run() {
    // A connection that closes with nothing on it.
    let mut source = Cursor::new(Vec::<u8>::new());
    let err = handshake(&mut source, &|| true).unwrap_err();
    assert!(err.to_string().contains("before any audio chunk arrived"));
}
