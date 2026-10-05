//! The announcement mixes: a room's stream with a clip mixed over it, cut on
//! the audio thread beside the slots (ADR 0174; the arithmetic is
//! `chorus_dsp::duck`, ADR 0173).
//!
//! # What a mix is
//!
//! One more stream on the slots' one grid: a [`Fanout`](crate::stream::Fanout)
//! after the stream slots and the silent one (`crate::router`), fed at every
//! tick with the chunk of its BASE slot (what the announced rooms' group
//! plays, or silence for a group that plays nothing) run through a
//! [`Duck`] together with the frames of one player port, the clip's
//! (`crate::playerport`). The player sessions of the announced rooms are
//! routed to it for as long as the announcement lasts (`crate::conductor`),
//! and everybody else in their group stays on the group's slot, untouched.
//!
//! Chunk `k` of a mix carries the sequence and the timestamp chunk `k` of
//! every slot carries, and while the duck is idle it IS the base slot's
//! chunk, the same bytes. So a session moved onto a mix before its duck
//! starts, and off it after its restore, hears nothing of either move, and a
//! room that is ducked stays on the timeline of the rooms that are not.
//!
//! # Who says what
//!
//! The conductor's thread sends [`MixCommand`]s down the slots' command
//! channel, applied at a chunk boundary like every slot command:
//! `Start` (duck, and mix this port's frames from now on), `Base` (the
//! group moved to another slot), `Finish` (the clip's player said it ended:
//! restore, and take nothing more from the port) and `Cancel` (restore from
//! here; with `fade` a clip still playing fades with the music, and without
//! it the port is let go at once). The audio thread itself ends a clip the
//! moment its port runs dry after its producer said it had finished, so the
//! restore begins on the frame after the clip's last, not a pass later.
//!
//! What crosses back is one [`MixPort`] per mix: whether the duck is idle,
//! and how many commands it has applied, as atomics. The announcer reads
//! them to know when a restore is complete and the rooms may go back.
//!
//! Nothing here reads a clock, waits, or allocates after [`Mix::new`].

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use chorus_dsp::duck::{Duck, DuckParams, DuckState};
use chorus_protocol::SampleFormat;

use crate::linein::{decode_sample, encode_sample};
use crate::playerport::PlayerPort;

/// A change to one mix, applied at the next chunk boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixCommand {
    /// An announcement begins on `mix`: duck `base` (a stream slot, or
    /// `None` for silence) and mix the frames of player port `player` over
    /// it once the duck is full. On a mix that is restoring, the duck goes
    /// down again from where it is.
    Start {
        /// The mix.
        mix: usize,
        /// The slot whose chunks are the music.
        base: Option<usize>,
        /// The player port the clip comes through.
        player: usize,
    },
    /// The music of `mix` is `base` from now on.
    Base {
        /// The mix.
        mix: usize,
        /// The slot whose chunks are the music.
        base: Option<usize>,
    },
    /// The clip of `mix` is over and everything of it has played: the port
    /// is let go, and the music comes back once the duck has been full.
    Finish {
        /// The mix.
        mix: usize,
    },
    /// The announcement on `mix` is called off: the music comes back from
    /// where it is.
    Cancel {
        /// The mix.
        mix: usize,
        /// Whether the clip's port is still this announcement's: a clip
        /// that was playing fades out with the restore. Without it the port
        /// is let go at once (its player failed and may be another clip's
        /// by the next tick).
        fade: bool,
    },
}

impl MixCommand {
    /// The mix it is for.
    pub fn mix(&self) -> usize {
        match self {
            MixCommand::Start { mix, .. }
            | MixCommand::Base { mix, .. }
            | MixCommand::Finish { mix }
            | MixCommand::Cancel { mix, .. } => *mix,
        }
    }
}

/// What the audio thread says about one mix.
#[derive(Debug, Default)]
pub struct MixPort {
    busy: AtomicBool,
    applied: AtomicU64,
}

impl MixPort {
    /// A mix nothing has been said to.
    pub fn new() -> MixPort {
        MixPort::default()
    }

    /// Whether the duck is anywhere but idle, as of the last tick.
    pub fn busy(&self) -> bool {
        self.busy.load(Ordering::SeqCst)
    }

    /// How many commands the audio thread has applied to this mix.
    pub fn applied(&self) -> u64 {
        self.applied.load(Ordering::SeqCst)
    }
}

/// One mix on the audio thread. Everything it needs is allocated by
/// [`Mix::new`].
pub struct Mix {
    port: Arc<MixPort>,
    duck: Option<Duck>,
    base: Option<usize>,
    player: Option<usize>,
    /// The generation of the port the clip's frames were last taken in,
    /// since the last `Start`.
    clip_generation: Option<u64>,
    channels: usize,
    music: Vec<f32>,
    clip: Vec<f32>,
    out: Vec<f32>,
    pcm: Vec<u8>,
}

impl Mix {
    /// A mix of chunks of `frames` frames of `channels` channels at
    /// `rate_hz`, ducking by `chorus_dsp::duck`'s defaults. A rate or a
    /// channel count the duck does not take leaves a mix that only ever
    /// passes its base on.
    pub fn new(
        port: Arc<MixPort>,
        rate_hz: u32,
        channels: usize,
        frames: usize,
        width: usize,
    ) -> Mix {
        let duck = DuckParams::defaults(rate_hz)
            .and_then(|params| Duck::new(&params, channels))
            .ok();
        Mix {
            port,
            duck,
            base: None,
            player: None,
            clip_generation: None,
            channels,
            music: vec![0.0; frames * channels],
            clip: vec![0.0; frames * channels],
            out: vec![0.0; frames * channels],
            pcm: vec![0u8; frames * channels * width],
        }
    }

    /// The slot whose chunks are this mix's music.
    pub fn base(&self) -> Option<usize> {
        self.base
    }

    /// Whether this tick's chunk has to be mixed; otherwise it is the base
    /// slot's own.
    pub fn active(&self) -> bool {
        self.duck
            .as_ref()
            .is_some_and(|d| d.state() != DuckState::Idle)
    }

    /// Apply one command.
    pub fn apply(&mut self, command: MixCommand) {
        match command {
            MixCommand::Start { base, player, .. } => {
                self.base = base;
                self.player = Some(player);
                self.clip_generation = None;
                if let Some(duck) = self.duck.as_mut() {
                    duck.start();
                }
            }
            MixCommand::Base { base, .. } => self.base = base,
            MixCommand::Finish { .. } => {
                self.player = None;
                if let Some(duck) = self.duck.as_mut() {
                    duck.finish();
                }
            }
            MixCommand::Cancel { fade, .. } => {
                if !fade {
                    self.player = None;
                }
                if let Some(duck) = self.duck.as_mut() {
                    duck.cancel();
                }
            }
        }
        self.port.busy.store(self.active(), Ordering::SeqCst);
        self.port.applied.fetch_add(1, Ordering::SeqCst);
    }

    /// One chunk: `music` (the base slot's chunk, as `format`) ducked, with
    /// the next frames of the clip's port mixed over it. Returns the chunk's
    /// bytes. Call only while [`Mix::active`].
    pub fn mix(
        &mut self,
        music: &[u8],
        players: &[Arc<PlayerPort>],
        format: SampleFormat,
    ) -> &[u8] {
        let width = format.bytes_per_sample();
        let Some(duck) = self.duck.as_mut() else {
            self.pcm.copy_from_slice(music);
            return &self.pcm;
        };
        for (x, bytes) in self.music.iter_mut().zip(music.chunks_exact(width)) {
            *x = decode_sample(bytes, format) as f32;
        }
        let (music, out) = (&self.music, &mut self.out);
        let taken_from = &mut self.clip_generation;
        let mut run = |clip: &[f32], last: bool, generation: u64| -> usize {
            if !clip.is_empty() {
                *taken_from = Some(generation);
            }
            // The end is the port's to say only for a clip this mix has
            // played frames of: a port still marked finished from the clip
            // before (its player not yet unloaded) says nothing about this
            // one.
            if last && *taken_from == Some(generation) {
                duck.finish();
            }
            match duck.process(music, clip, out) {
                Ok(mixed) => mixed.clip_frames,
                Err(_) => {
                    out.copy_from_slice(music);
                    0
                }
            }
        };
        match self
            .player
            .and_then(|p| players.get(p))
            .filter(|port| port.channels() == self.channels)
        {
            Some(port) => port.feed(&mut self.clip, run),
            None => {
                run(&[], false, 0);
            }
        }
        for (x, bytes) in self.out.iter().zip(self.pcm.chunks_exact_mut(width)) {
            encode_sample(f64::from(*x), format, bytes);
        }
        self.port.busy.store(
            self.duck
                .as_ref()
                .is_some_and(|d| d.state() != DuckState::Idle),
            Ordering::SeqCst,
        );
        &self.pcm
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S16: SampleFormat = SampleFormat::PcmS16Le;
    const RATE: u32 = 48_000;
    const FRAMES: usize = 960;

    fn chunk(value: i16) -> Vec<u8> {
        std::iter::repeat_n(value.to_le_bytes(), FRAMES * 2)
            .flatten()
            .collect()
    }

    fn samples(bytes: &[u8]) -> Vec<[i16; 2]> {
        bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|f| {
                [
                    i16::from_le_bytes([f[0], f[1]]),
                    i16::from_le_bytes([f[2], f[3]]),
                ]
            })
            .collect()
    }

    /// The whole of one announcement, tick by tick, as the audio thread runs
    /// it: music at 8000 on both channels, a clip of `clip_frames` frames at
    /// 16384 on the left channel alone.
    fn run(clip_frames: usize, ticks: usize) -> Vec<[i16; 2]> {
        let port = Arc::new(MixPort::new());
        let mut mix = Mix::new(Arc::clone(&port), RATE, 2, FRAMES, 2);
        let players = vec![Arc::new(PlayerPort::new(RATE, 2, RATE as usize))];
        let clip: Vec<f32> = (0..clip_frames).flat_map(|_| [0.5f32, 0.0]).collect();
        assert_eq!(players[0].write(&clip), clip_frames);
        players[0].finish();
        mix.apply(MixCommand::Start {
            mix: 0,
            base: Some(0),
            player: 0,
        });
        assert!(port.busy());
        assert_eq!(port.applied(), 1);
        let music = chunk(8_000);
        let mut heard = Vec::new();
        for _ in 0..ticks {
            if mix.active() {
                heard.extend(samples(mix.mix(&music, &players, S16)));
            } else {
                heard.extend(samples(&music));
            }
        }
        assert!(!port.busy(), "the restore completed");
        heard
    }

    #[test]
    fn a_mix_ducks_plays_the_clip_and_restores_on_the_frame_counts_of_the_defaults() {
        // 200 ms down, a clip of 2000 frames, 500 ms back.
        let heard = run(2_000, 40);
        let (down, back) = (9_600usize, 24_000usize);
        // The way down: the right channel is the music alone.
        assert_eq!(heard[0][1], 7_999, "the first frame is already lower");
        assert!(heard[down - 2][1] > 800);
        assert_eq!(heard[down - 1][1], 800, "20 dB down on frame 9600");
        // The clip: on the left, from the frame after, for exactly its
        // length, at 0.9 of its level over the ducked music.
        let with_clip = (16_384.0f64 * 0.9).round() as i16 + 800;
        for (n, f) in heard.iter().enumerate().skip(down).take(2_000) {
            assert_eq!(f[1], 800, "frame {n}");
            assert!((f[0] - with_clip).abs() <= 1, "frame {n}: {}", f[0]);
        }
        // The frame after its last begins the restore; the music is itself
        // again `back` frames later and bit for bit from then on.
        let over = down + 2_000;
        assert_eq!(heard[over][0], heard[over][1], "no clip frame is left");
        assert!(heard[over + 100][1] > 800 && heard[over + 100][1] < 8_000);
        assert!(heard[over + back - 100][1] < 8_000);
        for f in &heard[over + back - 1..] {
            assert_eq!(*f, [8_000, 8_000]);
        }
    }

    #[test]
    fn a_cancel_restores_from_where_the_music_is_and_a_finish_with_no_clip_too() {
        let port = Arc::new(MixPort::new());
        let mut mix = Mix::new(Arc::clone(&port), RATE, 2, FRAMES, 2);
        let players = vec![Arc::new(PlayerPort::new(RATE, 2, RATE as usize))];
        let music = chunk(8_000);
        mix.apply(MixCommand::Start {
            mix: 0,
            base: None,
            player: 0,
        });
        // Five ticks down (4800 of 9600 frames), then called off.
        for _ in 0..5 {
            mix.mix(&music, &players, S16);
        }
        mix.apply(MixCommand::Cancel {
            mix: 0,
            fade: false,
        });
        let mut ticks = 0;
        while mix.active() {
            let last = samples(mix.mix(&music, &players, S16))[FRAMES - 1][1];
            assert!(last > 4_000, "never lower than where it was: {last}");
            ticks += 1;
            assert!(ticks < 40);
        }
        // Half the depth back takes half the restore: 12000 frames.
        assert_eq!(ticks, 13);
        assert!(!port.busy());

        // A clip that never came: the duck holds until it is told, then
        // restores.
        mix.apply(MixCommand::Start {
            mix: 0,
            base: None,
            player: 0,
        });
        for _ in 0..20 {
            mix.mix(&music, &players, S16);
        }
        assert_eq!(samples(mix.mix(&music, &players, S16))[0], [800, 800]);
        mix.apply(MixCommand::Finish { mix: 0 });
        let mut ticks = 0;
        while mix.active() {
            mix.mix(&music, &players, S16);
            ticks += 1;
            assert!(ticks < 40);
        }
        assert_eq!(ticks, 25, "500 ms of 20 ms chunks");
        assert_eq!(port.applied(), 4);
    }

    #[test]
    fn a_held_port_and_a_late_clip_hold_the_duck_and_lose_no_frame() {
        let port = Arc::new(MixPort::new());
        let mut mix = Mix::new(port, RATE, 2, FRAMES, 2);
        let players = vec![Arc::new(PlayerPort::new(RATE, 2, RATE as usize))];
        let music = chunk(8_000);
        players[0].set_paused(true);
        players[0].write(&[0.5, 0.0, 0.25, 0.0]);
        mix.apply(MixCommand::Start {
            mix: 0,
            base: None,
            player: 0,
        });
        for _ in 0..15 {
            let heard = samples(mix.mix(&music, &players, S16));
            assert!(heard.iter().all(|f| f[0] == f[1]), "nothing of the clip");
        }
        assert_eq!(players[0].queued(), 2);
        players[0].set_paused(false);
        let heard = samples(mix.mix(&music, &players, S16));
        assert_eq!(heard[0], [800 + 14_746, 800]);
        assert_eq!(heard[1], [800 + 7_373, 800]);
        assert_eq!(heard[2], [800, 800], "not finished: the duck holds");
        assert_eq!(players[0].played_frames(), 2);
    }
}
