//! A Soloist receiver's audio, between its reader thread and the audio
//! thread (goal 17, `docs/soloist.md`).
//!
//! One port per receiver (`--soloist-receivers N`), allocated at start. The
//! reader thread (`crate::soloistreader`) drains the receiver's FIFO always
//! and writes what it converted here; the audio thread takes one chunk a
//! tick for every slot whose input is the receiver
//! (`crate::slots::SlotInput::Soloist`).
//!
//! # The rules
//!
//! - **Live audio: a full ring drops and counts.** The line-in port's rule
//!   (`crate::linein`), not the player port's: Spotify plays in real time
//!   whether or not chorus keeps up, so a writer that waited would only move
//!   the loss into the pipe. What does not fit is dropped and counted.
//! - **Selected or discarded.** A receiver no group plays is drained all the
//!   same (a full pipe would otherwise hold 64 KiB of stale audio for the
//!   next listener, `/cache/tmp` research: the PipeWire probe's section 4a,
//!   recorded in `docs/soloist.md`), and what is read is discarded here and
//!   counted. Selecting and deselecting both empty the ring, so a group that
//!   takes the receiver hears only what arrived after it did.
//! - **A fill target before the first chunk.** The FIFO is written in whole
//!   quanta of 1024 to 2048 frames (23 to 46 ms) in bursts, and on a loaded
//!   host without real-time scheduling the probe saw gaps of up to 98 ms
//!   between them. The port therefore plays nothing until it holds
//!   [`FILL_TARGET_MS`], and starts again from the target after it ran dry.
//! - **Running dry is silence, on time.** The audio thread never waits: a
//!   chunk the ring cannot fill is padded with silence. A dry spell that
//!   ends within [`STREAM_GAP`] was an underrun and is counted; a longer one
//!   was the music stopping (a pause, the end of a queue: an idle sink
//!   writes nothing at all) and is not.
//!
//! # What the lock costs the audio thread
//!
//! One mutex, held for one copy: the reader holds it to copy at most
//! [`WRITE_SLICE_FRAMES`] frames in, the audio thread to copy one chunk
//! out. Neither side blocks or allocates under it. Nothing here reads a
//! clock: the reader tells [`SoloistPort::write`] how long the FIFO was
//! quiet, measured on the monotonic clock.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use chorus_audio::StreamFormat;
use chorus_protocol::SampleFormat;

use crate::linein::encode_sample;

/// How much converted audio a port's ring holds, ms. ASSUMED: one second,
/// the line-in and player ports' size.
pub const RING_MS: u64 = 1_000;

/// How much the ring must hold before the audio thread takes the first
/// chunk, and again after it ran dry, ms. chorus's choice from the PipeWire
/// probe's measured cadence (goal 17, `docs/soloist.md`): the sink writes
/// one quantum of at most 2048 frames (46.4 ms at 44.1 kHz) at a time, and
/// the largest gap between two writes seen on the loaded development host
/// was 98.05 ms. 120 ms is that gap plus one default 20 ms chunk, so a
/// burst that late still finds a whole chunk to play. It is a latency the
/// Spotify app's own playout delay hides, not a sync error: every room of
/// the group plays the same chunk at the same instant.
pub const FILL_TARGET_MS: u64 = 120;

/// A dry spell shorter than this was an underrun; a longer one was the
/// stream stopping. ASSUMED: half a second, five times the largest gap the
/// probe saw.
pub const STREAM_GAP: Duration = Duration::from_millis(500);

/// Most frames one hold of the lock copies in on the reader's side.
pub const WRITE_SLICE_FRAMES: usize = 4_096;

/// What a port has counted since the server started. Never reset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counters {
    /// Frames written into the ring.
    pub written: u64,
    /// Frames the audio thread took.
    pub played: u64,
    /// Frames dropped because the ring was full.
    pub dropped: u64,
    /// Frames discarded because no group played the receiver.
    pub discarded: u64,
    /// Times the ring ran dry in the middle of a stream.
    pub underruns: u64,
    /// Frames of silence played in place of audio the ring did not hold
    /// while it was playing (not counting the fill before a start).
    pub padded: u64,
}

/// The ring of one receiver's converted audio.
#[derive(Debug)]
pub struct SoloistPort {
    ring: Mutex<Ring>,
    channels: usize,
    rate_hz: u32,
    capacity: usize,
    target: usize,
    selected: AtomicBool,
    written: AtomicU64,
    played: AtomicU64,
    dropped: AtomicU64,
    discarded: AtomicU64,
    underruns: AtomicU64,
    padded: AtomicU64,
}

#[derive(Debug)]
struct Ring {
    /// Interleaved samples, full scale 1.0.
    buf: Vec<f32>,
    /// Index of the oldest sample.
    start: usize,
    /// Samples held.
    len: usize,
    /// Whether the fill target was reached and the audio thread is taking
    /// chunks.
    primed: bool,
    /// The ring ran dry while primed and nothing has been written since.
    ran_dry: bool,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

impl SoloistPort {
    /// A port holding `frames` frames of `channels` channels at `rate_hz`,
    /// which plays once it holds `target` frames.
    pub fn new(rate_hz: u32, channels: usize, frames: usize, target: usize) -> SoloistPort {
        let channels = channels.max(1);
        let capacity = frames.max(1);
        SoloistPort {
            ring: Mutex::new(Ring {
                buf: vec![0.0; capacity * channels],
                start: 0,
                len: 0,
                primed: false,
                ran_dry: false,
            }),
            channels,
            rate_hz,
            capacity,
            target: target.clamp(1, capacity),
            selected: AtomicBool::new(false),
            written: AtomicU64::new(0),
            played: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
            discarded: AtomicU64::new(0),
            underruns: AtomicU64::new(0),
            padded: AtomicU64::new(0),
        }
    }

    /// A port at the server's format: [`RING_MS`] of ring, [`FILL_TARGET_MS`]
    /// of fill target.
    pub fn for_format(format: &StreamFormat) -> SoloistPort {
        let rate = u64::from(format.sample_rate_hz);
        SoloistPort::new(
            format.sample_rate_hz,
            usize::from(format.channels),
            (rate * RING_MS / 1_000) as usize,
            (rate * FILL_TARGET_MS / 1_000) as usize,
        )
    }

    /// The sample rate the reader must write at, Hz.
    pub fn rate_hz(&self) -> u32 {
        self.rate_hz
    }

    /// The channels of one frame the reader must write.
    pub fn channels(&self) -> usize {
        self.channels
    }

    /// How many frames the ring holds when full.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// How many frames the ring must hold before it plays.
    pub fn target(&self) -> usize {
        self.target
    }

    /// How many frames are queued now.
    pub fn queued(&self) -> usize {
        lock(&self.ring).len / self.channels
    }

    /// What the port has counted.
    pub fn counters(&self) -> Counters {
        Counters {
            written: self.written.load(Ordering::Relaxed),
            played: self.played.load(Ordering::Relaxed),
            dropped: self.dropped.load(Ordering::Relaxed),
            discarded: self.discarded.load(Ordering::Relaxed),
            underruns: self.underruns.load(Ordering::Relaxed),
            padded: self.padded.load(Ordering::Relaxed),
        }
    }

    // --- the control side ------------------------------------------------

    /// Say whether a group plays this receiver. A change either way empties
    /// the ring: a group that takes the receiver hears nothing that arrived
    /// before it did, and nothing is kept for a listener that left.
    pub fn select(&self, selected: bool) {
        let mut ring = lock(&self.ring);
        if self.selected.swap(selected, Ordering::SeqCst) != selected {
            ring.start = 0;
            ring.len = 0;
            ring.primed = false;
            ring.ran_dry = false;
        }
    }

    /// Whether a group plays this receiver.
    pub fn is_selected(&self) -> bool {
        self.selected.load(Ordering::SeqCst)
    }

    // --- the reader's side -----------------------------------------------

    /// Write interleaved frames at the port's rate and channel count. `quiet`
    /// is how long the FIFO had been silent before these frames arrived, on
    /// the reader's monotonic clock. Returns the frames kept. While no group
    /// plays the receiver everything is discarded and counted; what a full
    /// ring cannot take is dropped and counted.
    pub fn write(&self, samples: &[f32], quiet: Duration) -> usize {
        let frames = samples.len() / self.channels;
        let mut done = 0usize;
        while done < frames {
            let mut ring = lock(&self.ring);
            // Read under the lock, so a `select` cannot fall between the
            // check and the copy.
            if !self.selected.load(Ordering::SeqCst) {
                drop(ring);
                self.discarded
                    .fetch_add((frames - done) as u64, Ordering::Relaxed);
                return done;
            }
            if ring.ran_dry {
                ring.ran_dry = false;
                if quiet < STREAM_GAP {
                    self.underruns.fetch_add(1, Ordering::Relaxed);
                }
            }
            let cap = ring.buf.len();
            let room = (cap - ring.len) / self.channels;
            let n = room.min(frames - done).min(WRITE_SLICE_FRAMES);
            if n == 0 {
                drop(ring);
                self.dropped
                    .fetch_add((frames - done) as u64, Ordering::Relaxed);
                break;
            }
            let from = &samples[done * self.channels..(done + n) * self.channels];
            let at = (ring.start + ring.len) % cap;
            let first = from.len().min(cap - at);
            ring.buf[at..at + first].copy_from_slice(&from[..first]);
            let rest = from.len() - first;
            ring.buf[..rest].copy_from_slice(&from[first..]);
            ring.len += from.len();
            done += n;
        }
        self.written.fetch_add(done as u64, Ordering::Relaxed);
        done
    }

    // --- the audio thread's side -----------------------------------------

    /// One chunk for the audio thread: up to `frames` frames, oldest first,
    /// as little-endian `format` into `out`, and silence for the rest of
    /// `out`. Returns the frames taken. Silence, and nothing taken, while no
    /// group plays the receiver and until the ring holds its fill target.
    /// Allocates nothing and waits for nothing but the ring's lock.
    pub fn play(&self, frames: usize, format: SampleFormat, out: &mut [u8]) -> usize {
        let width = format.bytes_per_sample();
        let wanted = frames.min(out.len() / (width * self.channels));
        let mut ring = lock(&self.ring);
        if !self.selected.load(Ordering::SeqCst) {
            drop(ring);
            out.fill(0);
            return 0;
        }
        let held = ring.len / self.channels;
        if !ring.primed {
            if held < self.target {
                drop(ring);
                out.fill(0);
                return 0;
            }
            ring.primed = true;
        }
        let cap = ring.buf.len();
        let take = held.min(wanted);
        let samples = take * self.channels;
        let mut from = ring.start;
        let mut to = 0usize;
        for _ in 0..samples {
            encode_sample(f64::from(ring.buf[from]), format, &mut out[to..to + width]);
            to += width;
            from += 1;
            if from == cap {
                from = 0;
            }
        }
        ring.start = from;
        ring.len -= samples;
        if take < wanted {
            // Dry: fill again to the target before the next chunk, so a
            // late burst is not played a few frames at a time.
            ring.primed = false;
            ring.ran_dry = true;
        }
        drop(ring);
        out[to..].fill(0);
        self.played.fetch_add(take as u64, Ordering::Relaxed);
        if take < wanted {
            self.padded
                .fetch_add((wanted - take) as u64, Ordering::Relaxed);
        }
        take
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S16: SampleFormat = SampleFormat::PcmS16Le;
    const NOW: Duration = Duration::ZERO;

    fn signal(from: u64, frames: usize) -> Vec<f32> {
        (from..from + frames as u64)
            .flat_map(|n| {
                let v = ((n % 30_000) + 1) as f32 / 32_768.0;
                [v, -v]
            })
            .collect()
    }

    fn left(out: &[u8]) -> Vec<i16> {
        out.as_chunks::<4>()
            .0
            .iter()
            .map(|f| i16::from_le_bytes([f[0], f[1]]))
            .collect()
    }

    #[test]
    fn nothing_is_kept_and_nothing_plays_while_no_group_plays_the_receiver() {
        let port = SoloistPort::new(44_100, 2, 100, 4);
        assert_eq!(port.write(&signal(0, 10), NOW), 0);
        assert_eq!(port.queued(), 0);
        let mut out = [1u8; 16];
        assert_eq!(port.play(4, S16, &mut out), 0);
        assert_eq!(out, [0u8; 16]);
        let c = port.counters();
        assert_eq!((c.discarded, c.written, c.played, c.padded), (10, 0, 0, 0));
    }

    #[test]
    fn it_plays_from_the_fill_target_and_not_before() {
        let port = SoloistPort::new(44_100, 2, 100, 8);
        port.select(true);
        assert_eq!(port.write(&signal(0, 6), NOW), 6);
        let mut out = [1u8; 16];
        assert_eq!(port.play(4, S16, &mut out), 0, "6 frames is under 8");
        assert_eq!(out, [0u8; 16]);
        assert_eq!(port.counters().padded, 0, "filling is not padding");
        assert_eq!(port.write(&signal(6, 2), NOW), 2);
        assert_eq!(port.play(4, S16, &mut out), 4);
        assert_eq!(left(&out), [1, 2, 3, 4]);
        assert_eq!(port.play(4, S16, &mut out), 4);
        assert_eq!(left(&out), [5, 6, 7, 8]);
    }

    #[test]
    fn a_full_ring_drops_what_does_not_fit_and_counts_it() {
        let port = SoloistPort::new(44_100, 2, 10, 2);
        port.select(true);
        assert_eq!(port.write(&signal(0, 14), NOW), 10);
        let c = port.counters();
        assert_eq!((c.written, c.dropped), (10, 4));
        let mut out = [0u8; 40];
        assert_eq!(port.play(10, S16, &mut out), 10);
        assert_eq!(left(&out), [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    }

    #[test]
    fn running_dry_pads_silence_and_fills_to_the_target_again() {
        let port = SoloistPort::new(44_100, 2, 100, 4);
        port.select(true);
        port.write(&signal(0, 6), NOW);
        let mut out = [1u8; 16];
        assert_eq!(port.play(4, S16, &mut out), 4);
        assert_eq!(port.play(4, S16, &mut out), 2, "two frames were left");
        assert_eq!(left(&out), [5, 6, 0, 0]);
        assert_eq!(port.counters().padded, 2);
        // The next burst is short of the target: silence until it is met.
        port.write(&signal(6, 3), Duration::from_millis(60));
        assert_eq!(port.play(4, S16, &mut out), 0);
        assert_eq!(port.counters().underruns, 1, "a 60 ms gap is an underrun");
        port.write(&signal(9, 1), NOW);
        assert_eq!(port.play(4, S16, &mut out), 4);
        assert_eq!(left(&out), [7, 8, 9, 10]);
    }

    #[test]
    fn a_stream_that_stopped_and_started_again_is_not_an_underrun() {
        let port = SoloistPort::new(44_100, 2, 100, 2);
        port.select(true);
        port.write(&signal(0, 2), NOW);
        let mut out = [0u8; 16];
        assert_eq!(port.play(4, S16, &mut out), 2);
        port.write(&signal(2, 4), Duration::from_secs(3));
        assert_eq!(port.counters().underruns, 0);
        assert_eq!(port.play(4, S16, &mut out), 4);
    }

    #[test]
    fn selecting_and_deselecting_both_empty_the_ring() {
        let port = SoloistPort::new(44_100, 2, 100, 2);
        port.select(true);
        port.write(&signal(0, 8), NOW);
        port.select(true);
        assert_eq!(port.queued(), 8, "selecting again changes nothing");
        port.select(false);
        assert_eq!(port.queued(), 0);
        port.select(true);
        let mut out = [1u8; 16];
        assert_eq!(port.play(4, S16, &mut out), 0, "nothing stale");
        port.write(&signal(100, 4), NOW);
        assert_eq!(port.play(4, S16, &mut out), 4);
        assert_eq!(left(&out), [101, 102, 103, 104]);
    }

    #[test]
    fn a_port_at_the_servers_format_holds_a_second_and_starts_at_120_ms() {
        let format = StreamFormat {
            sample_rate_hz: 48_000,
            channels: 2,
            sample_format: SampleFormat::PcmS16Le,
        };
        let port = SoloistPort::for_format(&format);
        assert_eq!(
            (port.capacity(), port.target(), port.channels(), port.rate_hz()),
            (48_000, 5_760, 2, 48_000)
        );
    }
}
