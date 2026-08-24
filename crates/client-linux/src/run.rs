//! One client run, start to finish.
//!
//! # The graded interval
//!
//! The interval the delay assertion is made over, and the only one. It
//! **opens** at the first frame written to the audio device after the start
//! fill is reached, and **closes** at the earliest of: the end of stream
//! arriving, the connection being detected as lost, the client beginning to
//! exit for any other reason, and the end of the run. Samples outside it are
//! written to the log and marked ungraded, and nothing asserts their value.
//! That is what keeps the start fill and the deliberate drain from failing an
//! assertion they were never about.
//!
//! "Earliest" is load-bearing and the code is written to it: the flag goes
//! down where the reason is learned, which for the first three is the moment a
//! stop reason is recorded, NOT the moment the play-out that follows finishes.
//! The client keeps playing what it holds after a stream ends or a server
//! disappears - the interval does not stay open for it, and the log says where
//! it closed and why in a `graded-close` event.
//!
//! # The first write is the whole start fill
//!
//! Not one chunk of it. If output began with a single chunk, the device's
//! reported delay would be one chunk long at the instant the graded interval
//! opened, which is below any sane minimum, and the run would have to be
//! graded around its own start. Writing the accumulated fill in one call puts
//! the reported delay at the start fill immediately, which is inside the
//! bounds by configuration.
//!
//! # Nothing here corrects anything
//!
//! No offset filter, no rate correction, no resampling, no resync. The
//! playout loop holds the device's delay near a configured target by choosing
//! **when** to write, never by changing what it writes or how fast the device
//! consumes it. Two endpoints agreeing is a later phase's subject; this one
//! delivers the path that phase will correct.

use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver as ChannelReceiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use chorus_audio::MonotonicTimeline;
use chorus_protocol::StreamEnd;

use crate::buffer::{frames_to_us, us_to_frames, Accepted, Buffer, Counters, Zone};
use crate::config::ClientConfig;
use crate::delaylog::{DelayLog, LogHeader, LogSummary};
use crate::receive::{
    receive_loop, FramingError, Handshake, ReceiveStop, Received, Receiver, StreamShape,
};
use crate::sink::{PcmSink, SinkError};

/// How often the run writes a sample. Well under the once-per-second the log
/// format requires, so a coarse scheduler cannot make the log non-compliant.
pub const SAMPLE_INTERVAL_US: u64 = 100_000;

/// How long the playout loop sleeps when it has nothing to do.
const IDLE_SLEEP: Duration = Duration::from_millis(2);

/// Why the run ended.
///
/// The end-of-stream case renders `end_timestamp_ns` into its report line and
/// computes nothing from it. `docs/protocol.md` is the normative definition of
/// that field; nothing here restates the relation.
#[derive(Debug)]
pub enum StopReason {
    /// The server signalled the end of the stream in band.
    EndOfStream(StreamEnd),
    /// The connection closed without that signal.
    ConnectionLost {
        /// What the read said, if anything.
        detail: Option<String>,
    },
    /// The client closed the session on a framing error.
    Framing(FramingError),
    /// The configured run length was reached.
    RunLengthReached,
    /// The audio device failed or went away.
    DeviceFailed(SinkError),
    /// The stream never started: no chunk ever arrived.
    NoStream,
}

impl StopReason {
    /// Whether this run should exit zero.
    ///
    /// Only a stream that ended the way it said it would, and a run that ran
    /// as long as it was asked to. Everything else exits non-zero, which is
    /// what stops a lost server from looking like a tidy finish.
    pub fn is_clean(&self) -> bool {
        matches!(
            self,
            StopReason::EndOfStream(_) | StopReason::RunLengthReached
        )
    }

    /// The short machine-readable name, so a script can tell the two drains
    /// apart without matching on prose.
    pub fn name(&self) -> &'static str {
        match self {
            StopReason::EndOfStream(_) => "end-of-stream",
            StopReason::ConnectionLost { .. } => "connection-lost",
            StopReason::Framing(_) => "framing-error",
            StopReason::RunLengthReached => "run-length-reached",
            StopReason::DeviceFailed(_) => "device-failed",
            StopReason::NoStream => "no-stream",
        }
    }

    /// One line for the final report.
    pub fn describe(&self) -> String {
        match self {
            StopReason::EndOfStream(end) => format!(
                "the stream ended: the server sent the in-band end-of-stream signal after final \
                 sequence {}, ending at {} ns on the server timeline",
                end.final_sequence, end.end_timestamp_ns
            ),
            StopReason::ConnectionLost { detail: Some(d) } => format!(
                "the connection to the server was lost during the run, with no end-of-stream \
                 signal: {}",
                d
            ),
            StopReason::ConnectionLost { detail: None } => {
                "the connection to the server was lost during the run, with no end-of-stream \
                 signal: the server closed it"
                    .to_string()
            }
            StopReason::Framing(e) => format!("the client closed the session: {}", e),
            StopReason::RunLengthReached => "the configured run length was reached".to_string(),
            StopReason::DeviceFailed(e) => format!("the audio device failed: {}", e),
            StopReason::NoStream => "the connection carried no audio chunk at all".to_string(),
        }
    }
}

/// What one run produced.
#[derive(Debug)]
pub struct RunOutcome {
    /// Why it ended.
    pub stop: StopReason,
    /// The numbers the log's summary line carries.
    pub summary: LogSummary,
    /// Whether any audio was played.
    pub played_anything: bool,
}

/// A log line the receiving thread wants written.
struct PendingEvent {
    mono_us: u64,
    kind: &'static str,
    detail: String,
}

type StopSlot = Arc<Mutex<Option<StopReason>>>;

fn record_stop(slot: &StopSlot, reason: StopReason) {
    let mut guard = match slot.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    if guard.is_none() {
        *guard = Some(reason);
    }
}

/// Run one session against a connected source and an open sink.
///
/// `handshake` carries the framing state and everything already decoded while
/// the stream's shape was being learned, so nothing read before the device was
/// opened is lost.
pub fn run_session<R: Read + Send + 'static, S: PcmSink>(
    config: &ClientConfig,
    mut source: R,
    handshake: Handshake,
    sink: &mut S,
    log: &mut DelayLog,
    timeline: MonotonicTimeline,
    counters: Arc<Counters>,
) -> Result<RunOutcome, std::io::Error> {
    let rate_hz = sink.rate_hz();
    let buffer = Arc::new(Buffer::new(config.min_us, config.max_us, rate_hz));
    let keep_going = Arc::new(AtomicBool::new(true));
    let (events_tx, events_rx) = mpsc::channel::<PendingEvent>();
    let stop_slot: StopSlot = Arc::new(Mutex::new(None));

    let Handshake {
        receiver, buffered, ..
    } = handshake;

    let receiver_handle = {
        let buffer = Arc::clone(&buffer);
        let counters = Arc::clone(&counters);
        let keep_going = Arc::clone(&keep_going);
        let stop_slot = Arc::clone(&stop_slot);
        let events_tx = events_tx.clone();
        let min_us = config.min_us;
        let max_us = config.max_us;
        thread::spawn(move || {
            let mut receiver = receiver;
            let mut absorb = make_absorber(
                Arc::clone(&buffer),
                Arc::clone(&counters),
                events_tx,
                timeline,
                min_us,
                max_us,
            );
            let mut ended = None;
            for event in buffered {
                if let Received::End(end) = event {
                    ended = Some(end);
                    break;
                }
                absorb(event);
            }
            let stop = match ended {
                Some(end) => ReceiveStop::EndOfStream(end),
                None => {
                    let keep = Arc::clone(&keep_going);
                    let go = move || keep.load(Ordering::SeqCst);
                    receive_loop(&mut source, &mut receiver, &go, &mut absorb)
                }
            };
            record_stop(
                &stop_slot,
                match stop {
                    ReceiveStop::EndOfStream(end) => StopReason::EndOfStream(end),
                    ReceiveStop::ConnectionLost { error } => StopReason::ConnectionLost {
                        detail: error.map(|e| e.to_string()),
                    },
                    ReceiveStop::Framing(e) => StopReason::Framing(e),
                    ReceiveStop::Stopped => StopReason::RunLengthReached,
                },
            );
            buffer.close_input();
        })
    };

    let outcome = play(
        config,
        sink,
        log,
        timeline,
        &buffer,
        &counters,
        &keep_going,
        &events_rx,
        &stop_slot,
    );

    keep_going.store(false, Ordering::SeqCst);
    let _ = receiver_handle.join();
    outcome
}

fn make_absorber(
    buffer: Arc<Buffer>,
    counters: Arc<Counters>,
    events: Sender<PendingEvent>,
    timeline: MonotonicTimeline,
    min_us: u64,
    max_us: u64,
) -> impl FnMut(Received) {
    move |received: Received| match received {
        Received::Chunk { chunk, frames } => {
            let offered = buffer.offer(chunk, frames);
            match offered.accepted {
                Accepted::Queued => {}
                Accepted::DiscardedOverflow => {
                    counters.discarded_overflow.fetch_add(1, Ordering::Relaxed);
                }
                Accepted::DiscardedLate => {
                    counters.discarded_late.fetch_add(1, Ordering::Relaxed);
                }
                Accepted::DiscardedDuplicate => {
                    counters.discarded_duplicate.fetch_add(1, Ordering::Relaxed);
                }
            }
            if offered.crossed_maximum {
                counters.max_crossings.fetch_add(1, Ordering::Relaxed);
                let _ = events.send(PendingEvent {
                    mono_us: timeline.now_us(),
                    kind: "bound-crossing",
                    detail: format!(
                        "bound=maximum occupancy_us={} max_us={} discarded_overflow={}",
                        buffer.occupancy_us(),
                        max_us,
                        Counters::get(&counters.discarded_overflow)
                    ),
                });
            }
            let (zone, changed) = buffer.zone_transition(min_us, max_us);
            if changed {
                if zone == Zone::AtOrBelowMinimum {
                    counters.min_crossings.fetch_add(1, Ordering::Relaxed);
                }
                let _ = events.send(PendingEvent {
                    mono_us: timeline.now_us(),
                    kind: "zone",
                    detail: format!(
                        "zone={} occupancy_us={} min_us={} max_us={}",
                        zone.name(),
                        buffer.occupancy_us(),
                        min_us,
                        max_us
                    ),
                });
            }
        }
        Received::Malformed => {
            counters.discarded_malformed.fetch_add(1, Ordering::Relaxed);
        }
        Received::NotInThisGrammar => {
            counters.skipped_unknown_type.fetch_add(1, Ordering::Relaxed);
        }
        Received::End(_) => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn play<S: PcmSink>(
    config: &ClientConfig,
    sink: &mut S,
    log: &mut DelayLog,
    timeline: MonotonicTimeline,
    buffer: &Arc<Buffer>,
    counters: &Arc<Counters>,
    keep_going: &Arc<AtomicBool>,
    events: &ChannelReceiver<PendingEvent>,
    stop_slot: &StopSlot,
) -> Result<RunOutcome, std::io::Error> {
    let rate_hz = sink.rate_hz();
    let start_fill_frames = us_to_frames(config.start_fill_us, rate_hz);
    let target_frames = us_to_frames(config.device_target_us, rate_hz) as i64;
    let run_limit_us = config.run_seconds.map(|s| s * 1_000_000);

    let mut graded = false;
    let mut next_sample_us = 0u64;
    let mut delay_min_us = i64::MAX;
    let mut delay_max_us = i64::MIN;
    let mut graded_samples = 0u64;
    let mut first_graded_us = 0u64;
    let mut last_graded_us = 0u64;
    let mut first_write_us = 0u64;
    let mut played_anything = false;
    let mut device_error: Option<SinkError> = None;
    let mut chunk_frames = 0u64;

    macro_rules! sample_if_due {
        ($delay_us:expr, $graded:expr) => {{
            let now = timeline.now_us();
            if now >= next_sample_us {
                next_sample_us = now + SAMPLE_INTERVAL_US;
                let occupancy_us = buffer.occupancy_us();
                let delay_us: i64 = $delay_us;
                let is_graded: bool = $graded;
                log.sample(now, delay_us, occupancy_us, is_graded)?;
                if is_graded {
                    graded_samples += 1;
                    if graded_samples == 1 {
                        first_graded_us = now;
                    }
                    last_graded_us = now;
                    delay_min_us = delay_min_us.min(delay_us);
                    delay_max_us = delay_max_us.max(delay_us);
                }
            }
        }};
    }

    macro_rules! drain_events {
        () => {
            while let Ok(event) = events.try_recv() {
                log.event(event.mono_us, event.kind, &event.detail)?;
            }
        };
    }

    // Phase 1: withhold output until the buffer holds the start fill.
    loop {
        drain_events!();
        if buffer.queued_frames() >= start_fill_frames || buffer.input_closed() {
            break;
        }
        if let Some(limit) = run_limit_us {
            if timeline.now_us() >= limit {
                break;
            }
        }
        sample_if_due!(0, false);
        buffer.wait_for_queued(start_fill_frames, Duration::from_millis(5));
    }

    // Phase 2: the priming write. One call, the whole fill, so the graded
    // interval opens with the reported delay already inside the bounds.
    let mut primed: Vec<u8> = Vec::new();
    let mut primed_frames = 0u64;
    let mut last_primed = None;
    while primed_frames < start_fill_frames {
        match buffer.pop() {
            Some(queued) => {
                if chunk_frames == 0 {
                    chunk_frames = queued.frames;
                }
                primed.extend_from_slice(&queued.chunk.audio_data);
                primed_frames += queued.frames;
                last_primed = Some((queued.chunk, queued.frames));
            }
            None => break,
        }
    }

    if primed_frames > 0 {
        log.event(
            timeline.now_us(),
            "start-fill",
            &format!(
                "start_fill_us={} filled_us={} filled_frames={}",
                config.start_fill_us,
                frames_to_us(primed_frames, rate_hz),
                primed_frames
            ),
        )?;
        match sink.write(&primed) {
            Ok(report) => {
                counters
                    .frames_written
                    .fetch_add(report.frames_written, Ordering::Relaxed);
                counters.chunks_played.fetch_add(1, Ordering::Relaxed);
                if report.underran {
                    counters.underruns.fetch_add(1, Ordering::Relaxed);
                }
                played_anything = true;
                graded = true;
                first_write_us = timeline.now_us();
                next_sample_us = 0;
                if let Some((chunk, frames)) = &last_primed {
                    buffer.note_played(chunk, *frames);
                }
                let delay = sink.delay_frames().unwrap_or(0);
                buffer.set_device_delay_frames(delay);
                sample_if_due!(frames_to_us(delay.max(0) as u64, rate_hz) as i64, true);
            }
            Err(e) => device_error = Some(e),
        }
    }

    // Phase 3: hold the device's delay near its target by choosing when to
    // write. Nothing here changes the rate or the samples.
    while device_error.is_none() {
        drain_events!();

        if let Some(limit) = run_limit_us {
            if timeline.now_us() >= limit {
                keep_going.store(false, Ordering::SeqCst);
                record_stop(stop_slot, StopReason::RunLengthReached);
                if graded {
                    // The end of the run is the fourth of the four things that
                    // close the interval. The flag is not lowered here because
                    // nothing reads it again - the loop breaks, and phase 4
                    // takes no graded sample - so the event is what records
                    // where the interval closed and why.
                    log.event(
                        timeline.now_us(),
                        "graded-close",
                        "reason=run-length-reached graded=0",
                    )?;
                }
                break;
            }
        }
        // The graded interval closes at the EARLIEST of the four things Terms
        // names, and three of them arrive here as a recorded stop reason: the
        // end of stream, the connection detected as lost, and the client
        // beginning to exit for any other reason. What follows is the play-out
        // of what is already held, which is outside the interval however long
        // it takes - so the flag goes down here, at the reason, and not at the
        // start of the drain loop below.
        let stopped_because = match stop_slot.lock() {
            Ok(g) => g.as_ref().map(StopReason::name),
            Err(p) => p.into_inner().as_ref().map(StopReason::name),
        };
        if let Some(reason) = stopped_because {
            if graded {
                graded = false;
                log.event(
                    timeline.now_us(),
                    "graded-close",
                    &format!("reason={} graded=0", reason),
                )?;
            }
            if buffer.queued_frames() == 0 {
                break;
            }
        }

        let delay = match sink.delay_frames() {
            Ok(d) => d,
            Err(e) => {
                device_error = Some(e);
                break;
            }
        };
        buffer.set_device_delay_frames(delay);
        let delay_us = frames_to_us(delay.max(0) as u64, rate_hz) as i64;

        match sink.in_xrun() {
            Ok(true) => {
                let n = counters.underruns.fetch_add(1, Ordering::Relaxed) + 1;
                log.event(
                    timeline.now_us(),
                    "underrun",
                    &format!("source=device-state underruns={}", n),
                )?;
            }
            Ok(false) => {}
            Err(e) => {
                device_error = Some(e);
                break;
            }
        }

        if delay + chunk_frames.max(1) as i64 <= target_frames {
            match buffer.pop() {
                Some(queued) => {
                    if chunk_frames == 0 {
                        chunk_frames = queued.frames;
                    }
                    match sink.write(&queued.chunk.audio_data) {
                        Ok(report) => {
                            counters
                                .frames_written
                                .fetch_add(report.frames_written, Ordering::Relaxed);
                            counters.chunks_played.fetch_add(1, Ordering::Relaxed);
                            played_anything = true;
                            if report.underran {
                                let n = counters.underruns.fetch_add(1, Ordering::Relaxed) + 1;
                                log.event(
                                    timeline.now_us(),
                                    "underrun",
                                    &format!("source=write underruns={}", n),
                                )?;
                            }
                            buffer.note_played(&queued.chunk, queued.frames);
                        }
                        Err(e) => {
                            device_error = Some(e);
                            break;
                        }
                    }
                }
                None => {
                    if buffer.input_closed() {
                        break;
                    }
                    thread::sleep(IDLE_SLEEP);
                }
            }
        } else {
            thread::sleep(IDLE_SLEEP);
        }

        sample_if_due!(delay_us, graded);
    }

    // Phase 4: the deliberate drain. The graded interval is closed, so what
    // the delay does from here is recorded and asserted by nothing. Whatever
    // the client already holds is played out; asking the device to drain,
    // rather than letting its ring run dry, is what keeps this from counting
    // as underruns.
    graded = false;
    let _ = graded;
    keep_going.store(false, Ordering::SeqCst);
    if device_error.is_none() && played_anything {
        log.event(timeline.now_us(), "drain-begin", "graded=0")?;
        while let Some(queued) = buffer.pop() {
            match sink.write(&queued.chunk.audio_data) {
                Ok(report) => {
                    counters
                        .frames_written
                        .fetch_add(report.frames_written, Ordering::Relaxed);
                    counters.chunks_played.fetch_add(1, Ordering::Relaxed);
                }
                Err(e) => {
                    device_error = Some(e);
                    break;
                }
            }
            let delay = sink.delay_frames().unwrap_or(0);
            buffer.set_device_delay_frames(delay);
            sample_if_due!(frames_to_us(delay.max(0) as u64, rate_hz) as i64, false);
        }
        if device_error.is_none() {
            if let Err(e) = sink.drain() {
                device_error = Some(e);
            }
        }
    }
    drain_events!();
    {
        // One last ungraded sample, so the file ends where the run ended.
        let now = timeline.now_us();
        log.sample(now, 0, buffer.occupancy_us(), false)?;
    }

    let frames_played = sink.frames_played().unwrap_or(0);
    let end_us = timeline.now_us();
    let nominal_frames = if played_anything {
        us_to_frames(end_us.saturating_sub(first_write_us), rate_hz)
    } else {
        0
    };

    let stop = match device_error {
        Some(e) => StopReason::DeviceFailed(e),
        None => {
            let mut guard = match stop_slot.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            guard.take().unwrap_or(StopReason::NoStream)
        }
    };

    let summary = LogSummary {
        graded_span_us: last_graded_us.saturating_sub(first_graded_us),
        graded_samples,
        delay_min_us: if graded_samples > 0 { delay_min_us } else { 0 },
        delay_max_us: if graded_samples > 0 { delay_max_us } else { 0 },
        margin_to_min_us: if graded_samples > 0 {
            delay_min_us - config.min_us as i64
        } else {
            0
        },
        margin_to_max_us: if graded_samples > 0 {
            config.max_us as i64 - delay_max_us
        } else {
            0
        },
        underruns: Counters::get(&counters.underruns),
        discarded_overflow: Counters::get(&counters.discarded_overflow),
        discarded_late: Counters::get(&counters.discarded_late),
        discarded_duplicate: Counters::get(&counters.discarded_duplicate),
        discarded_malformed: Counters::get(&counters.discarded_malformed),
        frames_written: Counters::get(&counters.frames_written),
        frames_played,
        nominal_frames,
    };
    log.summary(&summary)?;
    log.flush()?;

    Ok(RunOutcome {
        stop,
        summary,
        played_anything,
    })
}

/// The header a run writes at the top of its delay log.
pub fn header_for(config: &ClientConfig, device: &str, shape: &StreamShape) -> LogHeader {
    LogHeader {
        min_us: config.min_us,
        max_us: config.max_us,
        start_fill_us: config.start_fill_us,
        device_target_us: config.device_target_us,
        device: device.to_string(),
        rate_hz: shape.sample_rate_hz,
        channels: shape.channels,
        sample_format: shape.sample_format.name().to_string(),
        frames_per_chunk: shape.frames_per_chunk,
        overflow_skew_ppm: config.overflow_skew_ppm,
    }
}

/// A counter snapshot, for the final report.
pub fn counter_lines(counters: &Counters) -> Vec<String> {
    vec![
        format!("underruns={}", Counters::get(&counters.underruns)),
        format!(
            "discarded_overflow={}",
            Counters::get(&counters.discarded_overflow)
        ),
        format!("discarded_late={}", Counters::get(&counters.discarded_late)),
        format!(
            "discarded_duplicate={}",
            Counters::get(&counters.discarded_duplicate)
        ),
        format!(
            "discarded_malformed={}",
            Counters::get(&counters.discarded_malformed)
        ),
        format!(
            "skipped_unknown_type={}",
            Counters::get(&counters.skipped_unknown_type)
        ),
        format!("chunks_played={}", Counters::get(&counters.chunks_played)),
        format!("frames_written={}", Counters::get(&counters.frames_written)),
        format!("max_crossings={}", Counters::get(&counters.max_crossings)),
        format!("min_crossings={}", Counters::get(&counters.min_crossings)),
    ]
}

/// A receiver with no bytes yet, for callers that build a handshake by hand.
pub fn fresh_receiver() -> Receiver {
    Receiver::new()
}
