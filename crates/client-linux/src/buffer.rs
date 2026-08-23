//! The jitter buffer, the counters, and what happens at each bound.
//!
//! # Occupancy
//!
//! Buffer occupancy is the total duration of received audio the client has not
//! yet played: the frames it still holds, **plus** the frames the device has
//! accepted and not yet made audible. The second term is the device-reported
//! delay, which is the audible component and the quantity the delay assertion
//! grades. Both are logged.
//!
//! # What happens at each bound
//!
//! Stated here rather than left to the reader, because "the buffer drifted"
//! and "the buffer hit something" are different events:
//!
//! - **Toward a bound.** The zone the occupancy is in is reported when it
//!   changes. Nothing is done about it: no rate change, no resample, no bound
//!   is moved. Correcting is SYNC-4's subject and this phase asserts that it
//!   does not.
//! - **At the minimum.** Reported. If the buffer then empties, the device
//!   underruns and that is counted from the device's own signal, separately.
//! - **At the maximum.** Reported once per crossing, and whole arriving chunks
//!   are discarded rather than enqueued for as long as occupancy is at or past
//!   it. Discarding at the ceiling is overflow handling and not clock
//!   correction: the timeline and the playback rate are untouched. Growing
//!   instead would put unbounded memory on a host shared with the rest of the
//!   household.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};

use chorus_protocol::AudioChunk;

/// Every counter a run reports.
///
/// Each reason has its own counter on purpose. An overflow discard, a late
/// chunk, a duplicate and a malformed frame are four different things going
/// wrong, and one number that mixed them would hide whichever one mattered.
#[derive(Debug, Default)]
pub struct Counters {
    /// Underruns, from the device's own signal.
    pub underruns: AtomicU64,
    /// Chunks discarded because occupancy was at or past the maximum.
    pub discarded_overflow: AtomicU64,
    /// Chunks discarded because their presentation timestamp was already past.
    pub discarded_late: AtomicU64,
    /// Chunks discarded because their sequence had already been accepted.
    pub discarded_duplicate: AtomicU64,
    /// Frames discarded because the decoder rejected them.
    pub discarded_malformed: AtomicU64,
    /// Frames stepped over because their type is not in the catalog.
    pub skipped_unknown_type: AtomicU64,
    /// Chunks handed to the device.
    pub chunks_played: AtomicU64,
    /// Frames handed to the device.
    pub frames_written: AtomicU64,
    /// Times occupancy crossed into the at-or-past-maximum state.
    pub max_crossings: AtomicU64,
    /// Times occupancy crossed into the at-or-below-minimum state.
    pub min_crossings: AtomicU64,
}

impl Counters {
    /// A fresh set, all zero.
    pub fn new() -> Counters {
        Counters::default()
    }

    /// Read one counter.
    pub fn get(counter: &AtomicU64) -> u64 {
        counter.load(Ordering::Relaxed)
    }
}

/// Where occupancy sits relative to its bounds.
///
/// Reported when it changes, which is what "reports the drift" means: a
/// transition, not a number repeated every sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    /// At or below the configured minimum.
    AtOrBelowMinimum,
    /// Within one eighth of the span of the minimum.
    NearMinimum,
    /// Comfortably between.
    Mid,
    /// Within one eighth of the span of the maximum.
    NearMaximum,
    /// At or past the configured maximum.
    AtOrPastMaximum,
}

impl Zone {
    /// The zone `occupancy_us` falls in, for bounds `min_us` and `max_us`.
    pub fn of(occupancy_us: u64, min_us: u64, max_us: u64) -> Zone {
        let span = max_us.saturating_sub(min_us).max(1);
        let margin = (span / 8).max(1);
        if occupancy_us >= max_us {
            Zone::AtOrPastMaximum
        } else if occupancy_us <= min_us {
            Zone::AtOrBelowMinimum
        } else if occupancy_us >= max_us - margin {
            Zone::NearMaximum
        } else if occupancy_us <= min_us + margin {
            Zone::NearMinimum
        } else {
            Zone::Mid
        }
    }

    /// The zone name as it appears in a report line.
    pub fn name(self) -> &'static str {
        match self {
            Zone::AtOrBelowMinimum => "at-or-below-minimum",
            Zone::NearMinimum => "near-minimum",
            Zone::Mid => "mid",
            Zone::NearMaximum => "near-maximum",
            Zone::AtOrPastMaximum => "at-or-past-maximum",
        }
    }
}

/// What the buffer did with an arriving chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accepted {
    /// Queued for playout.
    Queued,
    /// Discarded because occupancy is at or past the maximum.
    DiscardedOverflow,
    /// Discarded because its presentation timestamp is already past.
    DiscardedLate,
    /// Discarded because its sequence has already been accepted.
    DiscardedDuplicate,
}

/// What one offered chunk did, and whether it took the buffer over its
/// ceiling.
///
/// The crossing is a property of the call rather than of the outcome, because
/// either outcome can be the one that crosses: the enqueue that fills the
/// buffer to the brim crosses it, and so does the first arrival after that.
/// Reporting it here, once, is what makes "once per crossing" true rather
/// than "once per chunk while over".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Offer {
    /// What happened to the chunk.
    pub accepted: Accepted,
    /// Whether this call moved occupancy from below the maximum to at or past
    /// it.
    pub crossed_maximum: bool,
}

/// A chunk waiting to be played.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Queued {
    /// The chunk itself.
    pub chunk: AudioChunk,
    /// Frames it carries.
    pub frames: u64,
}

#[derive(Debug)]
struct Inner {
    queue: VecDeque<Queued>,
    queued_frames: u64,
    device_delay_frames: i64,
    /// Highest sequence accepted so far, and whether anything has been.
    highwater: Option<u32>,
    /// Presentation timestamp one past the last chunk handed to the device.
    playout_ts_ns: Option<u64>,
    /// Whether occupancy is currently at or past the maximum.
    at_or_past_max: bool,
    /// The last zone reported.
    zone: Zone,
    /// No more input is coming.
    input_closed: bool,
}

/// The buffer between the receiver and the playout loop.
pub struct Buffer {
    inner: Mutex<Inner>,
    changed: Condvar,
    min_frames: u64,
    max_frames: u64,
    rate_hz: u32,
}

impl Buffer {
    /// A buffer bounded by `min_us` and `max_us` at `rate_hz`.
    pub fn new(min_us: u64, max_us: u64, rate_hz: u32) -> Buffer {
        Buffer {
            inner: Mutex::new(Inner {
                queue: VecDeque::new(),
                queued_frames: 0,
                device_delay_frames: 0,
                highwater: None,
                playout_ts_ns: None,
                at_or_past_max: false,
                zone: Zone::AtOrBelowMinimum,
                input_closed: false,
            }),
            changed: Condvar::new(),
            min_frames: us_to_frames(min_us, rate_hz),
            max_frames: us_to_frames(max_us, rate_hz),
            rate_hz,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        match self.inner.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Frames of occupancy: what is queued plus what the device holds.
    pub fn occupancy_frames(&self) -> u64 {
        let inner = self.lock();
        occupancy(&inner)
    }

    /// Occupancy in microseconds.
    pub fn occupancy_us(&self) -> u64 {
        frames_to_us(self.occupancy_frames(), self.rate_hz)
    }

    /// Frames waiting in the queue, not counting what the device holds.
    pub fn queued_frames(&self) -> u64 {
        self.lock().queued_frames
    }

    /// Tell the buffer what the device most recently reported.
    pub fn set_device_delay_frames(&self, frames: i64) {
        let mut inner = self.lock();
        inner.device_delay_frames = frames;
    }

    /// Offer a chunk to the buffer, applying the receive-side rules.
    ///
    /// The order is deliberate: a duplicate is a duplicate whatever the buffer
    /// level is, a late chunk is late whatever the buffer level is, and only a
    /// chunk that would otherwise have been played is discarded for overflow.
    pub fn offer(&self, chunk: AudioChunk, frames: u64) -> Offer {
        let mut inner = self.lock();

        if let Some(highwater) = inner.highwater {
            // Sequences increase by one and wrap, and the transport preserves
            // order, so anything not ahead of the highwater has been seen.
            if chunk.sequence.wrapping_sub(highwater) == 0
                || chunk.sequence.wrapping_sub(highwater) > u32::MAX / 2
            {
                return Offer {
                    accepted: Accepted::DiscardedDuplicate,
                    crossed_maximum: false,
                };
            }
        }
        if let Some(playout_ts) = inner.playout_ts_ns {
            if chunk.timestamp_ns < playout_ts {
                return Offer {
                    accepted: Accepted::DiscardedLate,
                    crossed_maximum: false,
                };
            }
        }

        let was_at_or_past = inner.at_or_past_max;
        let before = occupancy(&inner);
        if before >= self.max_frames {
            inner.at_or_past_max = true;
            return Offer {
                accepted: Accepted::DiscardedOverflow,
                crossed_maximum: !was_at_or_past,
            };
        }

        inner.highwater = Some(chunk.sequence);
        inner.queued_frames += frames;
        inner.queue.push_back(Queued { chunk, frames });
        let after = occupancy(&inner);
        let crossed = if after >= self.max_frames {
            inner.at_or_past_max = true;
            !was_at_or_past
        } else {
            false
        };
        drop(inner);
        self.changed.notify_all();
        Offer {
            accepted: Accepted::Queued,
            crossed_maximum: crossed,
        }
    }

    /// Take the chunk at the front, if there is one.
    pub fn pop(&self) -> Option<Queued> {
        let mut inner = self.lock();
        let queued = inner.queue.pop_front()?;
        inner.queued_frames -= queued.frames;
        if occupancy(&inner) < self.max_frames {
            inner.at_or_past_max = false;
        }
        Some(queued)
    }

    /// Record that a chunk has been handed to the device.
    ///
    /// The playout timestamp is what makes a later chunk "already in the
    /// past": it is one chunk past the last chunk that went to the device, on
    /// the server timeline, which is the only timeline both ends share.
    pub fn note_played(&self, chunk: &AudioChunk, frames: u64) {
        let mut inner = self.lock();
        let duration_ns = frames * 1_000_000_000 / u64::from(self.rate_hz);
        inner.playout_ts_ns = Some(chunk.timestamp_ns.saturating_add(duration_ns));
    }

    /// The zone occupancy is in now, and whether it changed since the last
    /// call.
    pub fn zone_transition(&self, min_us: u64, max_us: u64) -> (Zone, bool) {
        let mut inner = self.lock();
        let occupancy_us = frames_to_us(occupancy(&inner), self.rate_hz);
        let zone = Zone::of(occupancy_us, min_us, max_us);
        let changed = zone != inner.zone;
        inner.zone = zone;
        (zone, changed)
    }

    /// Say that no more chunks are coming.
    pub fn close_input(&self) {
        let mut inner = self.lock();
        inner.input_closed = true;
        drop(inner);
        self.changed.notify_all();
    }

    /// Whether the input has closed.
    pub fn input_closed(&self) -> bool {
        self.lock().input_closed
    }

    /// Wait until the queue holds `frames`, the input closes, or `timeout`
    /// elapses. Returns the frames queued when it returned.
    pub fn wait_for_queued(&self, frames: u64, timeout: std::time::Duration) -> u64 {
        let inner = self.lock();
        let (inner, _) = self
            .changed
            .wait_timeout_while(inner, timeout, |i| {
                i.queued_frames < frames && !i.input_closed
            })
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        inner.queued_frames
    }

    /// The minimum bound in frames.
    pub fn min_frames(&self) -> u64 {
        self.min_frames
    }

    /// The maximum bound in frames.
    pub fn max_frames(&self) -> u64 {
        self.max_frames
    }
}

fn occupancy(inner: &Inner) -> u64 {
    inner.queued_frames + inner.device_delay_frames.max(0) as u64
}

/// Frames in `us` microseconds at `rate_hz`.
pub fn us_to_frames(us: u64, rate_hz: u32) -> u64 {
    us * u64::from(rate_hz) / 1_000_000
}

/// Microseconds `frames` frames occupy at `rate_hz`.
pub fn frames_to_us(frames: u64, rate_hz: u32) -> u64 {
    frames * 1_000_000 / u64::from(rate_hz)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chorus_protocol::{SampleFormat, RESERVED_LEN};

    fn chunk(sequence: u32, timestamp_ns: u64, frames: usize) -> AudioChunk {
        AudioChunk {
            sequence,
            timestamp_ns,
            sample_rate_hz: 48_000,
            channels: 2,
            sample_format: SampleFormat::PcmS16Le,
            reserved: [0u8; RESERVED_LEN],
            audio_data: vec![0u8; frames * 4],
        }
    }

    fn offer(b: &Buffer, sequence: u32, timestamp_ns: u64) -> Accepted {
        b.offer(chunk(sequence, timestamp_ns, 960), 960).accepted
    }

    #[test]
    fn occupancy_counts_what_the_device_holds_as_well_as_the_queue() {
        let b = Buffer::new(60_000, 300_000, 48_000);
        offer(&b, 0, 0);
        assert_eq!(b.occupancy_us(), 20_000);
        b.set_device_delay_frames(4_800);
        assert_eq!(b.occupancy_us(), 120_000);
    }

    #[test]
    fn a_chunk_is_discarded_at_the_maximum_and_the_crossing_is_reported_once() {
        let b = Buffer::new(60_000, 100_000, 48_000);
        // 100 ms of maximum is 4800 frames; five 20 ms chunks fill it.
        let mut crossings = 0;
        for i in 0..5 {
            let offered = b.offer(chunk(i, u64::from(i) * 20_000_000, 960), 960);
            assert_eq!(offered.accepted, Accepted::Queued);
            crossings += u32::from(offered.crossed_maximum);
        }
        assert_eq!(crossings, 1, "the enqueue that filled it is the crossing");

        let first = b.offer(chunk(5, 100_000_000, 960), 960);
        assert_eq!(first.accepted, Accepted::DiscardedOverflow);
        assert!(!first.crossed_maximum, "the crossing was already reported");
        let second = b.offer(chunk(6, 120_000_000, 960), 960);
        assert_eq!(second.accepted, Accepted::DiscardedOverflow);
        assert!(!second.crossed_maximum);
        // Never more than the maximum plus one chunk is held.
        assert!(b.occupancy_frames() <= b.max_frames() + 960);
    }

    #[test]
    fn a_crossing_is_reported_again_after_the_buffer_comes_back_down() {
        let b = Buffer::new(60_000, 100_000, 48_000);
        let mut crossings = 0;
        for i in 0..6 {
            crossings +=
                u32::from(b.offer(chunk(i, u64::from(i) * 20_000_000, 960), 960).crossed_maximum);
        }
        assert_eq!(crossings, 1);
        b.pop();
        b.pop();
        assert_eq!(offer(&b, 6, 120_000_000), Accepted::Queued);
        for i in 7..9 {
            crossings +=
                u32::from(b.offer(chunk(i, u64::from(i) * 20_000_000, 960), 960).crossed_maximum);
        }
        assert_eq!(crossings, 2, "coming back down and going up again crosses twice");
    }

    #[test]
    fn a_sequence_already_accepted_is_a_duplicate() {
        let b = Buffer::new(60_000, 300_000, 48_000);
        offer(&b, 7, 0);
        assert_eq!(offer(&b, 7, 20_000_000), Accepted::DiscardedDuplicate);
        assert_eq!(offer(&b, 6, 20_000_000), Accepted::DiscardedDuplicate);
        assert_eq!(offer(&b, 8, 20_000_000), Accepted::Queued);
    }

    #[test]
    fn duplicate_detection_survives_the_sequence_wrapping() {
        let b = Buffer::new(60_000, 300_000, 48_000);
        offer(&b, u32::MAX, 0);
        assert_eq!(offer(&b, 0, 20_000_000), Accepted::Queued);
        assert_eq!(offer(&b, u32::MAX, 40_000_000), Accepted::DiscardedDuplicate);
    }

    #[test]
    fn a_timestamp_already_past_the_playout_point_is_late() {
        let b = Buffer::new(60_000, 300_000, 48_000);
        offer(&b, 0, 100_000_000);
        let queued = b.pop().unwrap();
        b.note_played(&queued.chunk, 960);
        // The playout point is now 120 ms on the server timeline.
        assert_eq!(offer(&b, 1, 119_000_000), Accepted::DiscardedLate);
        assert_eq!(offer(&b, 2, 120_000_000), Accepted::Queued);
    }

    #[test]
    fn zones_report_a_transition_and_not_every_sample() {
        let b = Buffer::new(60_000, 300_000, 48_000);
        let (zone, changed) = b.zone_transition(60_000, 300_000);
        assert_eq!(zone, Zone::AtOrBelowMinimum);
        assert!(!changed, "the buffer starts empty and starts in that zone");
        b.set_device_delay_frames(us_to_frames(150_000, 48_000) as i64);
        let (zone, changed) = b.zone_transition(60_000, 300_000);
        assert_eq!(zone, Zone::Mid);
        assert!(changed);
        let (_, changed) = b.zone_transition(60_000, 300_000);
        assert!(!changed, "the same zone twice is not a transition");
    }

    #[test]
    fn the_zone_boundaries_are_where_they_are_said_to_be() {
        assert_eq!(Zone::of(60_000, 60_000, 300_000), Zone::AtOrBelowMinimum);
        assert_eq!(Zone::of(60_001, 60_000, 300_000), Zone::NearMinimum);
        assert_eq!(Zone::of(90_000, 60_000, 300_000), Zone::NearMinimum);
        assert_eq!(Zone::of(150_000, 60_000, 300_000), Zone::Mid);
        assert_eq!(Zone::of(270_000, 60_000, 300_000), Zone::NearMaximum);
        assert_eq!(Zone::of(300_000, 60_000, 300_000), Zone::AtOrPastMaximum);
        assert_eq!(Zone::of(999_000, 60_000, 300_000), Zone::AtOrPastMaximum);
    }
}
