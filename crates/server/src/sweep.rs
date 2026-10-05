//! The measurement sweep's stream: the room-correction sweep, cut on the
//! audio thread beside the slots (ADR 0195).
//!
//! # What it is
//!
//! One more stream on the slots' one grid: a [`Fanout`](crate::stream::Fanout)
//! after the stream slots, the silent one and the announcement mixes
//! (`crate::router`). It plays silence, and, from a [`SweepCommand::Start`]
//! to the end of the program, the program: [`LEAD_MS`] of silence, the sweep
//! the fitter deconvolves with (`chorus_dsp::roomfit::Sweep::recommended` at
//! the stream's rate, the same samples on every channel), and [`TAIL_MS`] of
//! silence. The player sessions of the one room being measured are routed to
//! it for as long as that lasts (`crate::conductor`, told by
//! `crate::measure`); every other session stays where it is and receives
//! none of it, and no group's source changes.
//!
//! Chunk `k` of this stream carries the sequence and the timestamp chunk `k`
//! of every slot carries, so a session moved onto it and back sees a
//! contiguous run of sequences whose content changes, as it does on any
//! move between slots. Nothing is claimed here about when the room hears the
//! sweep relative to anything else: the fitter searches for the response's
//! peak and assumes no latency (`docs/room-correction.md`).
//!
//! # Who says what
//!
//! The conductor's thread sends [`SweepCommand`]s down the slots' command
//! channel, applied at a chunk boundary like every slot command: `Start`
//! (play the program from its first frame) and `Cancel` (stop at this
//! boundary). Each names the play it is about, so a `Cancel` that arrives
//! after its play ended, or after the next one started, does nothing. What
//! crosses back is one [`SweepPort`]: the last play that ran to its end and
//! the last that ended either way, as atomics.
//!
//! The program is rendered once, before the audio thread exists
//! ([`SweepProgram::render`]). Nothing here reads a clock, waits, or
//! allocates after [`SweepPlayer::new`].

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use chorus_dsp::roomfit::Sweep;
use chorus_protocol::SampleFormat;

use crate::linein::encode_sample;

/// The silence the measured room hears before the sweep, ms. ASSUMED: half
/// a second, the length of the fitter's response window
/// (`FitConfig::window_s`, itself ASSUMED from REW's 500 ms), so what the
/// room was playing has that long to ring out before the sweep's first
/// sample, and the room's new volume is in force well before it.
pub const LEAD_MS: u64 = 500;

/// The silence the measured room hears after the sweep, ms. ASSUMED: one
/// second. The fitter refuses a recording shorter than the sweep plus its
/// 0.5 s response window (`too_short`), and `docs/room-correction.md` asks
/// for "at least the sweep plus a second": the room stays silent for that
/// second, so the recording's tail holds the room's response and not music.
pub const TAIL_MS: u64 = 1_000;

/// The sweep alone, as PCM of `format`: every frame carries the sweep's
/// sample on each of `channels` channels.
pub fn sweep_pcm(rate_hz: u32, channels: u16, format: SampleFormat) -> Vec<u8> {
    let width = format.bytes_per_sample();
    let channels = usize::from(channels);
    let signal = Sweep::recommended(rate_hz).signal();
    let mut pcm = vec![0u8; signal.len() * channels * width];
    for (frame, x) in pcm.chunks_exact_mut(channels * width).zip(&signal) {
        for sample in frame.chunks_exact_mut(width) {
            encode_sample(*x, format, sample);
        }
    }
    pcm
}

/// The whole program a play runs through: the lead silence, the sweep, the
/// tail silence, as PCM of the server's format.
#[derive(Clone, PartialEq, Eq)]
pub struct SweepProgram {
    pcm: Arc<[u8]>,
    lead_ms: u64,
    sweep_ms: u64,
    tail_ms: u64,
}

impl fmt::Debug for SweepProgram {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SweepProgram")
            .field("bytes", &self.pcm.len())
            .field("lead_ms", &self.lead_ms)
            .field("sweep_ms", &self.sweep_ms)
            .field("tail_ms", &self.tail_ms)
            .finish()
    }
}

impl SweepProgram {
    /// The program at `rate_hz`, `channels` channels of `format`.
    pub fn render(rate_hz: u32, channels: u16, format: SampleFormat) -> SweepProgram {
        let frame_len = usize::from(channels) * format.bytes_per_sample();
        let rate = u64::from(rate_hz);
        let sweep = sweep_pcm(rate_hz, channels, format);
        let lead = (LEAD_MS * rate / 1_000) as usize * frame_len;
        let tail = (TAIL_MS * rate / 1_000) as usize * frame_len;
        let sweep_frames = (sweep.len() / frame_len.max(1)) as u64;
        let mut pcm = vec![0u8; lead + sweep.len() + tail];
        pcm[lead..lead + sweep.len()].copy_from_slice(&sweep);
        SweepProgram {
            pcm: Arc::from(pcm),
            lead_ms: LEAD_MS,
            sweep_ms: (sweep_frames * 1_000).div_ceil(rate.max(1)),
            tail_ms: TAIL_MS,
        }
    }

    /// The program's bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.pcm
    }

    /// The lead silence, the sweep and the tail silence, ms.
    pub fn lengths_ms(&self) -> (u64, u64, u64) {
        (self.lead_ms, self.sweep_ms, self.tail_ms)
    }

    /// The whole program, ms.
    pub fn total_ms(&self) -> u64 {
        self.lead_ms + self.sweep_ms + self.tail_ms
    }
}

/// A change to the sweep's stream, applied at the next chunk boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SweepCommand {
    /// Play the program from its first frame, as play `id` (never 0).
    Start {
        /// The play.
        id: u64,
    },
    /// Stop play `id` at this boundary; nothing when another play, or none,
    /// is playing.
    Cancel {
        /// The play.
        id: u64,
    },
}

/// What the audio thread says about the sweep's stream.
#[derive(Debug, Default)]
pub struct SweepPort {
    completed: AtomicU64,
    ended: AtomicU64,
}

impl SweepPort {
    /// A stream nothing has been played on.
    pub fn new() -> SweepPort {
        SweepPort::default()
    }

    /// The last play whose program ran to its last frame; 0 before any did.
    pub fn completed(&self) -> u64 {
        self.completed.load(Ordering::SeqCst)
    }

    /// The last play that ended, either way; 0 before any did.
    pub fn ended(&self) -> u64 {
        self.ended.load(Ordering::SeqCst)
    }
}

/// The sweep's stream on the audio thread. Everything it needs is allocated
/// by [`SweepPlayer::new`].
pub struct SweepPlayer {
    program: SweepProgram,
    port: Arc<SweepPort>,
    /// The play and the byte of the program its next chunk starts at.
    playing: Option<(u64, usize)>,
    out: Vec<u8>,
}

impl SweepPlayer {
    /// A player of `program` in chunks of `bytes_per_chunk` bytes.
    pub fn new(program: SweepProgram, port: Arc<SweepPort>, bytes_per_chunk: usize) -> SweepPlayer {
        SweepPlayer {
            program,
            port,
            playing: None,
            out: vec![0u8; bytes_per_chunk],
        }
    }

    /// Whether this tick's chunk is the program's; otherwise it is silence.
    pub fn active(&self) -> bool {
        self.playing.is_some()
    }

    /// Apply one command.
    pub fn apply(&mut self, command: SweepCommand) {
        match command {
            SweepCommand::Start { id } => {
                if let Some((old, _)) = self.playing {
                    self.port.ended.store(old, Ordering::SeqCst);
                }
                self.playing = Some((id, 0));
            }
            SweepCommand::Cancel { id } => {
                if self.playing.is_some_and(|(playing, _)| playing == id) {
                    self.playing = None;
                    self.port.ended.store(id, Ordering::SeqCst);
                }
            }
        }
    }

    /// One chunk of the program: its next bytes, and silence for whatever
    /// the program lacks at its end. The play is over once its last byte is
    /// in a chunk. Call only while [`SweepPlayer::active`].
    pub fn play(&mut self) -> &[u8] {
        let Some((id, at)) = self.playing else {
            self.out.fill(0);
            return &self.out;
        };
        let bytes = self.program.bytes();
        let n = (bytes.len() - at).min(self.out.len());
        self.out[..n].copy_from_slice(&bytes[at..at + n]);
        self.out[n..].fill(0);
        if at + n >= bytes.len() {
            self.playing = None;
            self.port.completed.store(id, Ordering::SeqCst);
            self.port.ended.store(id, Ordering::SeqCst);
        } else {
            self.playing = Some((id, at + n));
        }
        &self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s16(bytes: &[u8]) -> Vec<i16> {
        bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| i16::from_le_bytes(*b))
            .collect()
    }

    #[test]
    fn the_sweeps_pcm_is_the_fitters_sweep_on_every_channel() {
        let signal = Sweep::recommended(48_000).signal();
        let pcm = s16(&sweep_pcm(48_000, 2, SampleFormat::PcmS16Le));
        assert_eq!(pcm.len(), signal.len() * 2);
        for (frame, x) in pcm.as_chunks::<2>().0.iter().zip(&signal) {
            let expected = (x * 32_768.0).round().clamp(-32_768.0, 32_767.0) as i16;
            assert_eq!(*frame, [expected, expected]);
        }
        // Half full scale, as the fitter's sweep is.
        let peak = pcm.iter().map(|s| s.unsigned_abs()).max().unwrap();
        assert!((16_300..=16_384).contains(&peak), "{}", peak);
    }

    #[test]
    fn the_program_is_the_lead_the_sweep_and_the_tail() {
        let program = SweepProgram::render(48_000, 2, SampleFormat::PcmS16Le);
        assert_eq!(program.lengths_ms(), (500, 5_000, 1_000));
        assert_eq!(program.total_ms(), 6_500);
        let sweep = sweep_pcm(48_000, 2, SampleFormat::PcmS16Le);
        let lead = 24_000 * 4;
        let bytes = program.bytes();
        assert_eq!(bytes.len(), lead + sweep.len() + 48_000 * 4);
        assert!(bytes[..lead].iter().all(|b| *b == 0));
        assert_eq!(&bytes[lead..lead + sweep.len()], &sweep[..]);
        assert!(bytes[lead + sweep.len()..].iter().all(|b| *b == 0));
    }

    #[test]
    fn a_play_runs_through_the_program_once_and_says_it_completed() {
        let program = SweepProgram::render(8_000, 1, SampleFormat::PcmS16Le);
        let port = Arc::new(SweepPort::new());
        // A chunk the program is not a whole number of: 300 bytes.
        let mut player = SweepPlayer::new(program.clone(), Arc::clone(&port), 300);
        assert!(!player.active());
        player.apply(SweepCommand::Start { id: 1 });
        let mut heard = Vec::new();
        let mut chunks = 0;
        while player.active() {
            heard.extend_from_slice(player.play());
            chunks += 1;
            assert!(chunks < 10_000);
        }
        let len = program.bytes().len();
        assert_eq!(chunks, len.div_ceil(300));
        assert_eq!(&heard[..len], program.bytes());
        assert!(heard[len..].iter().all(|b| *b == 0), "padded with silence");
        assert_eq!((port.completed(), port.ended()), (1, 1));
    }

    #[test]
    fn a_cancel_stops_its_own_play_and_no_other() {
        let program = SweepProgram::render(8_000, 1, SampleFormat::PcmS16Le);
        let port = Arc::new(SweepPort::new());
        let mut player = SweepPlayer::new(program, Arc::clone(&port), 320);
        player.apply(SweepCommand::Start { id: 1 });
        let _ = player.play();
        player.apply(SweepCommand::Cancel { id: 2 });
        assert!(player.active(), "another play's cancel does nothing");
        player.apply(SweepCommand::Cancel { id: 1 });
        assert!(!player.active());
        assert_eq!((port.completed(), port.ended()), (0, 1));
        // A late cancel of a play that is over does nothing to the next.
        player.apply(SweepCommand::Start { id: 2 });
        player.apply(SweepCommand::Cancel { id: 1 });
        assert!(player.active());
        // A start over a play still running ends that one.
        player.apply(SweepCommand::Start { id: 3 });
        assert_eq!((port.completed(), port.ended()), (0, 2));
    }
}
