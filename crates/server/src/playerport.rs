//! Player ports: decoded network media handed to the audio thread (goal 16).
//!
//! # What a port is
//!
//! One network media player's audio on its way into a stream slot: a
//! fixed-capacity ring of interleaved frames at THIS server's sample rate and
//! channel count, full scale 1.0. `--players N` makes N of them, allocated
//! once, before the audio thread starts. A group whose source is
//! `player:p<i>` has port `i` as its slot's input
//! (`crate::slots::SlotInput::Player`).
//!
//! # Two sides
//!
//! **The producer** is one ordinary thread (`player-<i>`, `crate::player`):
//! it decodes, resamples to the port's rate, remixes to its channels, and
//! writes. Decoded media is not live, so nothing is ever dropped: [`write`]
//! takes what fits and says how much, and the producer waits for [`room`].
//! The producer also says what the audio thread cannot know: [`flush`]
//! (stop, seek: what is queued is forgotten), [`set_paused`] (silence goes
//! out and nothing is drained) and [`finish`] (no more is coming for now, so
//! running dry is the end and not an underrun).
//!
//! **The audio thread** calls [`play`] once per tick while a slot plays the
//! port: it takes exactly one chunk's frames, oldest first, converts them to
//! the server's sample format and pads with silence whatever the ring could
//! not supply. It never waits for the producer and never allocates.
//!
//! # The counter the producer reads
//!
//! Two counts of frames, both since the last flush, both moved under the
//! ring's lock: [`mark`] is how many the producer has written, [`played_frames`]
//! how many the audio thread has taken. The ring is first in, first out and
//! silence padding is not counted, so **the frame written when `mark()` read
//! N is in a chunk exactly when `played_frames()` exceeds N**. That is how a
//! producer knows its position, and how it knows the moment a second track,
//! written straight after the first with no flush, starts to go out: it reads
//! `mark()` before the second track's first frame and waits for
//! `played_frames()` to pass it. "Played" here means cut into a chunk and
//! stamped; a room hears that chunk one playout latency later
//! (`crate::conductor::heard_latency_ns`).
//!
//! # What crosses to the audio thread
//!
//! The ring behind one mutex, as a line-in's port (`crate::linein`), and two
//! atomics. The audio thread holds the lock for one chunk's conversion; the
//! producer holds it for one copy of at most [`WRITE_SLICE_FRAMES`] frames,
//! however much it was handed. Both are microseconds against a 20 ms tick
//! (ASSUMED adequate, as for the line-in port; not measured).
//!
//! No clock is read here. The stamps a player's chunks go out with are the
//! slots' one grid.
//!
//! [`write`]: PlayerPort::write
//! [`room`]: PlayerPort::room
//! [`flush`]: PlayerPort::flush
//! [`set_paused`]: PlayerPort::set_paused
//! [`finish`]: PlayerPort::finish
//! [`play`]: PlayerPort::play
//! [`mark`]: PlayerPort::mark
//! [`played_frames`]: PlayerPort::played_frames

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

use chorus_audio::StreamFormat;
use chorus_protocol::SampleFormat;

use crate::linein::encode_sample;

/// How much decoded audio a port's ring holds, ms. ASSUMED: one second, the
/// line-in port's size. It is what stands between a producer that is late (an
/// ordinary thread that was not scheduled, a decoder working through a large
/// frame) and an underrun; it is not a network buffer, which is the
/// producer's own to keep. It costs no latency: a stop or a seek flushes it,
/// and a pause takes effect at the next tick whatever it holds. Larger only
/// costs memory, which the server locks (384 kB a port at 48 kHz stereo).
pub const PLAYER_RING_MS: u64 = 1_000;

/// Most frames one hold of the lock copies in on the producer's side. A
/// write of more is done in several holds, so the audio thread never waits
/// behind a copy of a whole ring. ASSUMED: 4096 frames, a few microseconds
/// of copying.
pub const WRITE_SLICE_FRAMES: usize = 4_096;

/// A bounded ring of decoded frames between a player's producer thread and
/// the audio thread. Allocated once.
#[derive(Debug)]
pub struct PlayerPort {
    ring: Mutex<Ring>,
    channels: usize,
    rate_hz: u32,
    capacity: usize,
    paused: AtomicBool,
    underruns: AtomicU64,
}

#[derive(Debug)]
struct Ring {
    /// Interleaved samples, full scale 1.0.
    buf: Vec<f32>,
    /// Index of the oldest sample.
    start: usize,
    /// Samples held.
    len: usize,
    /// Moves on every flush.
    generation: u64,
    /// Frames written since the last flush.
    written: u64,
    /// Frames the audio thread has taken since the last flush.
    played: u64,
    /// The producer said no more is coming for now.
    finished: bool,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

impl PlayerPort {
    /// A port holding `frames` frames of `channels` channels at `rate_hz`.
    pub fn new(rate_hz: u32, channels: usize, frames: usize) -> PlayerPort {
        let channels = channels.max(1);
        let capacity = frames.max(1);
        PlayerPort {
            ring: Mutex::new(Ring {
                buf: vec![0.0; capacity * channels],
                start: 0,
                len: 0,
                generation: 0,
                written: 0,
                played: 0,
                finished: false,
            }),
            channels,
            rate_hz,
            capacity,
            paused: AtomicBool::new(false),
            underruns: AtomicU64::new(0),
        }
    }

    /// A port at the server's format, holding [`PLAYER_RING_MS`].
    pub fn for_format(format: &StreamFormat) -> PlayerPort {
        let frames = (u64::from(format.sample_rate_hz) * PLAYER_RING_MS / 1_000) as usize;
        PlayerPort::new(format.sample_rate_hz, usize::from(format.channels), frames)
    }

    /// The sample rate the producer must write at, Hz.
    pub fn rate_hz(&self) -> u32 {
        self.rate_hz
    }

    /// The channels of one frame the producer must write.
    pub fn channels(&self) -> usize {
        self.channels
    }

    /// How many frames the ring holds when full.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    // --- the producer's side ---------------------------------------------

    /// How many frames can be written now.
    pub fn room(&self) -> usize {
        self.capacity - lock(&self.ring).len / self.channels
    }

    /// How many frames are queued: written and not yet taken.
    pub fn queued(&self) -> usize {
        lock(&self.ring).len / self.channels
    }

    /// Write interleaved frames (`samples.len()` a multiple of
    /// [`PlayerPort::channels`]; a trailing part of a frame is ignored), as
    /// many as fit. Returns the frames accepted; the rest is the caller's to
    /// write again once there is room. Nothing is dropped and nothing already
    /// queued is overwritten. A write of at least one frame ends a
    /// [`PlayerPort::finish`].
    pub fn write(&self, samples: &[f32]) -> usize {
        let frames = samples.len() / self.channels;
        let mut done = 0usize;
        while done < frames {
            let mut ring = lock(&self.ring);
            let cap = ring.buf.len();
            let room = (cap - ring.len) / self.channels;
            let n = room.min(frames - done).min(WRITE_SLICE_FRAMES);
            if n == 0 {
                break;
            }
            let from = &samples[done * self.channels..(done + n) * self.channels];
            let at = (ring.start + ring.len) % cap;
            let first = from.len().min(cap - at);
            ring.buf[at..at + first].copy_from_slice(&from[..first]);
            let rest = from.len() - first;
            ring.buf[..rest].copy_from_slice(&from[first..]);
            ring.len += from.len();
            ring.written += n as u64;
            ring.finished = false;
            done += n;
        }
        done
    }

    /// Say that no more is coming for now (the media ended, with nothing
    /// queued behind it). What is queued still plays, the last part of a
    /// chunk padded with silence; running dry after this is the end and is
    /// not counted as an underrun. The next write ends it.
    pub fn finish(&self) {
        lock(&self.ring).finished = true;
    }

    /// Forget everything queued (a stop, a seek) and start counting again:
    /// [`PlayerPort::mark`] and [`PlayerPort::played_frames`] are 0 after it
    /// and the generation moves. Returns how many frames the audio thread had
    /// taken since the previous flush, which is exactly how much of what was
    /// written went out: nothing written before a flush is played after it.
    pub fn flush(&self) -> u64 {
        let mut ring = lock(&self.ring);
        let played = ring.played;
        ring.start = 0;
        ring.len = 0;
        ring.generation += 1;
        ring.written = 0;
        ring.played = 0;
        ring.finished = false;
        played
    }

    /// Frames written since the last flush: the index the next frame written
    /// will have. Read before a track's first frame, it is that track's
    /// boundary in [`PlayerPort::played_frames`].
    pub fn mark(&self) -> u64 {
        lock(&self.ring).written
    }

    /// Frames the audio thread has taken since the last flush. Never above
    /// [`PlayerPort::mark`]; equal to it when everything written has gone
    /// out. Silence the audio thread played instead (paused, running dry,
    /// padding) is not counted.
    pub fn played_frames(&self) -> u64 {
        lock(&self.ring).played
    }

    /// How many times the port was flushed.
    pub fn generation(&self) -> u64 {
        lock(&self.ring).generation
    }

    /// Hold the port (`true`) or let it play (`false`). While held the audio
    /// thread plays silence, takes nothing and counts no underrun, from its
    /// next tick. A producer holds the port while it fills it (buffering) so
    /// the first chunk out is a whole one.
    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::SeqCst);
    }

    /// Whether the port is held.
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    /// Ticks the audio thread wanted a chunk and the ring held less than one,
    /// while the port was not held, something had been written since the last
    /// flush and the producer had not said [`PlayerPort::finish`]: the
    /// producer fell behind. Each played silence for what was missing. Never
    /// reset.
    pub fn underruns(&self) -> u64 {
        self.underruns.load(Ordering::Relaxed)
    }

    // --- the audio thread's side -----------------------------------------

    /// One chunk for the audio thread: up to `frames` frames, oldest first,
    /// as little-endian `format` into `out`, and silence for the rest of
    /// `out`. Returns the frames taken. A held port gives silence and takes
    /// nothing. Allocates nothing and waits for nothing but the ring's lock.
    pub fn play(&self, frames: usize, format: SampleFormat, out: &mut [u8]) -> usize {
        if self.paused.load(Ordering::SeqCst) {
            out.fill(0);
            return 0;
        }
        let width = format.bytes_per_sample();
        let wanted = frames.min(out.len() / (width * self.channels));
        let mut ring = lock(&self.ring);
        let cap = ring.buf.len();
        let take = (ring.len / self.channels).min(wanted);
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
        ring.played += take as u64;
        let starved = take < wanted && ring.written > 0 && !ring.finished;
        drop(ring);
        out[to..].fill(0);
        if starved {
            self.underruns.fetch_add(1, Ordering::Relaxed);
        }
        take
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S16: SampleFormat = SampleFormat::PcmS16Le;

    /// Frame `n` of a stereo test signal: `n + 1` left, its negation right,
    /// as s16 at full scale 1.0 (exact in f32).
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
    fn a_write_takes_what_fits_and_drops_nothing() {
        let port = PlayerPort::new(48_000, 2, 10);
        assert_eq!((port.room(), port.capacity(), port.channels()), (10, 10, 2));
        assert_eq!(port.write(&signal(0, 6)), 6);
        assert_eq!(port.write(&signal(6, 6)), 4, "four more fit");
        assert_eq!((port.room(), port.queued(), port.mark()), (0, 10, 10));
        assert_eq!(port.write(&signal(10, 1)), 0, "full: nothing accepted");
        let mut out = [0u8; 4 * 4];
        assert_eq!(port.play(4, S16, &mut out), 4);
        assert_eq!(left(&out), [1, 2, 3, 4]);
        // The two frames refused are written again and follow in order,
        // across the ring's wrap.
        assert_eq!(port.write(&signal(10, 2)), 2);
        let mut rest = [0u8; 8 * 4];
        assert_eq!(port.play(8, S16, &mut rest), 8);
        assert_eq!(left(&rest), [5, 6, 7, 8, 9, 10, 11, 12]);
        assert_eq!(port.played_frames(), 12);
        assert_eq!(port.mark(), 12);
        // Both channels came through: the right is the left negated.
        assert_eq!(i16::from_le_bytes([rest[2], rest[3]]), -5);
    }

    #[test]
    fn a_trailing_part_of_a_frame_is_not_written() {
        let port = PlayerPort::new(48_000, 2, 10);
        assert_eq!(port.write(&[0.5, 0.5, 0.25]), 1);
        assert_eq!(port.mark(), 1);
    }

    #[test]
    fn a_write_larger_than_one_slice_arrives_whole_and_in_order() {
        let frames = WRITE_SLICE_FRAMES * 2 + 17;
        let port = PlayerPort::new(48_000, 2, frames + 5);
        assert_eq!(port.write(&signal(0, frames)), frames);
        let mut out = vec![0u8; frames * 4];
        assert_eq!(port.play(frames, S16, &mut out), frames);
        let heard = left(&out);
        assert!(heard
            .iter()
            .enumerate()
            .all(|(n, v)| *v == ((n as u64 % 30_000) + 1) as i16));
    }

    #[test]
    fn a_short_ring_plays_what_it_has_padded_with_silence() {
        let port = PlayerPort::new(48_000, 2, 100);
        port.write(&signal(0, 3));
        port.finish();
        let mut out = [0xffu8; 8 * 4];
        assert_eq!(port.play(8, S16, &mut out), 3);
        assert_eq!(left(&out), [1, 2, 3, 0, 0, 0, 0, 0]);
        assert_eq!(port.played_frames(), 3, "the padding is not counted");
        assert_eq!(port.underruns(), 0, "the producer said it had finished");
        // Dry after the end: silence, and still no underrun.
        assert_eq!(port.play(8, S16, &mut out), 0);
        assert!(out.iter().all(|b| *b == 0));
        assert_eq!(port.underruns(), 0);
    }

    #[test]
    fn running_dry_in_the_middle_is_an_underrun_and_loses_nothing() {
        let port = PlayerPort::new(48_000, 2, 100);
        let mut out = [0u8; 4 * 4];
        // Nothing written yet: silence, and not an underrun.
        assert_eq!(port.play(4, S16, &mut out), 0);
        assert_eq!(port.underruns(), 0);
        port.write(&signal(0, 6));
        assert_eq!(port.play(4, S16, &mut out), 4);
        assert_eq!(port.underruns(), 0);
        assert_eq!(port.play(4, S16, &mut out), 2);
        assert_eq!(left(&out), [5, 6, 0, 0]);
        assert_eq!(port.underruns(), 1);
        assert_eq!(port.play(4, S16, &mut out), 0);
        assert_eq!(port.underruns(), 2);
        // The producer catches up: the next frame written is the next played.
        port.write(&signal(6, 4));
        assert_eq!(port.play(4, S16, &mut out), 4);
        assert_eq!(left(&out), [7, 8, 9, 10]);
        assert_eq!(port.underruns(), 2);
        assert_eq!((port.mark(), port.played_frames()), (10, 10));
    }

    #[test]
    fn a_held_port_plays_silence_and_takes_nothing() {
        let port = PlayerPort::new(48_000, 2, 100);
        port.write(&signal(0, 8));
        let mut out = [0xffu8; 4 * 4];
        assert_eq!(port.play(4, S16, &mut out), 4);
        port.set_paused(true);
        assert!(port.is_paused());
        for _ in 0..3 {
            assert_eq!(port.play(4, S16, &mut out), 0);
            assert!(out.iter().all(|b| *b == 0));
        }
        assert_eq!((port.played_frames(), port.queued()), (4, 4));
        assert_eq!(port.underruns(), 0);
        port.set_paused(false);
        assert_eq!(port.play(4, S16, &mut out), 4);
        assert_eq!(left(&out), [5, 6, 7, 8], "nothing was lost to the hold");
    }

    #[test]
    fn a_flush_forgets_what_is_queued_and_starts_the_counts_again() {
        let port = PlayerPort::new(48_000, 2, 100);
        port.write(&signal(0, 10));
        let mut out = [0u8; 4 * 4];
        port.play(4, S16, &mut out);
        assert_eq!(port.generation(), 0);
        assert_eq!(port.flush(), 4, "four of the ten went out");
        assert_eq!(
            (
                port.generation(),
                port.mark(),
                port.played_frames(),
                port.queued()
            ),
            (1, 0, 0, 0)
        );
        assert_eq!(port.play(4, S16, &mut out), 0);
        assert!(out.iter().all(|b| *b == 0), "nothing of the old audio");
        assert_eq!(
            port.underruns(),
            0,
            "empty after a flush is not an underrun"
        );
        port.write(&signal(100, 4));
        assert_eq!(port.play(4, S16, &mut out), 4);
        assert_eq!(left(&out), [101, 102, 103, 104]);
    }

    #[test]
    fn a_second_track_written_straight_after_the_first_starts_at_its_mark() {
        let port = PlayerPort::new(48_000, 2, 100);
        port.write(&signal(0, 7));
        let boundary = port.mark();
        port.write(&signal(1_000, 9));
        assert_eq!(boundary, 7);
        let mut heard = Vec::new();
        let mut crossed_at = None;
        let mut out = [0u8; 4 * 4];
        for tick in 0..4 {
            port.play(4, S16, &mut out);
            heard.extend(left(&out));
            if crossed_at.is_none() && port.played_frames() > boundary {
                crossed_at = Some(tick);
            }
        }
        // No gap and no overlap: frame 7 of what was played is the second
        // track's first, and it went out in the tick the count passed 7.
        assert_eq!(&heard[..7], &[1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(
            &heard[7..16],
            &[1001, 1002, 1003, 1004, 1005, 1006, 1007, 1008, 1009]
        );
        assert_eq!(crossed_at, Some(1));
        assert_eq!(port.played_frames(), 16);
    }

    #[test]
    fn every_sample_format_carries_the_samples_exactly() {
        for (format, width) in [
            (SampleFormat::PcmS16Le, 2usize),
            (SampleFormat::PcmS24Le, 3),
            (SampleFormat::PcmF32Le, 4),
        ] {
            let port = PlayerPort::new(44_100, 1, 8);
            assert_eq!(port.rate_hz(), 44_100);
            port.write(&[0.25, -0.5]);
            let mut out = vec![0u8; 2 * width];
            assert_eq!(port.play(2, format, &mut out), 2);
            assert_eq!(crate::linein::decode_sample(&out[..width], format), 0.25);
            assert_eq!(crate::linein::decode_sample(&out[width..], format), -0.5);
        }
    }

    #[test]
    fn a_port_at_the_servers_format_holds_one_second() {
        let format = StreamFormat::new(48_000, 2, "pcm_s16le").unwrap();
        let port = PlayerPort::for_format(&format);
        assert_eq!(
            (port.capacity(), port.channels(), port.rate_hz()),
            (48_000, 2, 48_000)
        );
    }
}
