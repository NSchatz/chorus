//! The announcement mixer: duck the music, mix a clip over it, restore.
//!
//! A pure core, as the rest of this crate: it takes the music's frames and
//! the clip's frames and returns the mix. It reads no clock; every length is
//! a count of frames, and the only thing that moves its state is a frame
//! passing through [`Duck::process`]. It runs on the server, before a room's
//! stream is encoded (`docs/decisions/` "the announcement mixer"), so there
//! is no C mirror; its fixtures are `fixtures/dsp/duck/`.
//!
//! # The envelope
//!
//! One integer `level` runs from 0 (the music untouched) to `D * R` (fully
//! ducked), `D` the duck ramp and `R` the restore ramp in frames. A ducking
//! frame adds `R`, a restoring frame takes `D` away, so the way down is
//! exactly `D` frames, the way back exactly `R`, and a ramp turned round in
//! the middle goes on from where it is without a jump. With
//! `p = level / (D * R)`:
//!
//! ```text
//! music gain  g = 1 - (1 - duck_gain) p      (exactly duck_gain at p = 1)
//! clip gain   w = clip_gain                  (holding)
//!             w = clip_gain p                (a cancelled clip, fading)
//! out         = clamp(music g + clip w, -limit, limit)
//! ```
//!
//! `p` is computed in `f64` from the integers and each gain rounded to `f32`
//! once, so the gains are monotone along a ramp. At `level = 0` the frame is
//! copied, not multiplied: the restored music is its input, bit for bit.
//!
//! # The order of things
//!
//! 1. [`Duck::start`]: the next frame is the first of the duck ramp; the
//!    `D`th frame is at the duck gain. No clip frame is taken while the music
//!    is on its way down (the clip starts when the ramp ends, as Android's
//!    audio focus does: `docs/dsp.md`).
//! 2. Holding: each frame takes one clip frame. With no clip frame to take
//!    the music stays ducked under silence, until
//! 3. [`Duck::finish`] (the clip has no more frames than those handed in:
//!    the frame after its last begins the restore) or [`Duck::cancel`] (the
//!    next frame begins the restore, and what is left of the clip fades out
//!    with it; cancelled on the way down, no clip frame was ever played).
//! 4. Restoring: the `R`th frame, and every frame after it, is the music's
//!    own.
//!
//! An event lands on a frame by ending the block there: the caller splits its
//! block at the frame the event belongs to.
//!
//! # The limit
//!
//! `duck_gain + clip_gain <= 1` is checked when the parameters are made, so
//! with a music and a clip inside `-limit..=limit` the sum is inside it too;
//! the clamp takes the last rounding. Frames the mixer does not touch (idle)
//! are not clamped: they are the music's, and the chain's limiter is what
//! holds a room to its ceiling (`docs/dsp.md`, "The chain's order").

use crate::{db_to_gain, DspError, MAX_RATE_HZ, MIN_RATE_HZ};

/// The duck depth in dB when none is given: the music plays 20 dB down under
/// the clip. Cited: the ESPHome mixer speaker's documented example
/// (`decibel_reduction: 20`), `docs/dsp.md`.
pub const DEFAULT_DUCK_DB: f64 = -20.0;

/// The duck ramp, ms. ASSUMED: 200 ms; not measured. The clip waits for it,
/// so it is shorter than the half second of the restore.
pub const DEFAULT_DUCK_RAMP_MS: u32 = 200;

/// The restore ramp, ms. Cited: the Audacity manual's Auto Duck default fade
/// length (0.5 s), `docs/dsp.md`.
pub const DEFAULT_RESTORE_RAMP_MS: u32 = 500;

/// The limit when none is given: full scale.
pub const DEFAULT_LIMIT: f32 = 1.0;

/// The longest ramp: 10 s at 384 kHz.
pub const MAX_RAMP_FRAMES: u32 = 3_840_000;

/// The deepest duck taken, dB (the ESPHome mixer's documented range ends at
/// 50; `docs/dsp.md`).
pub const MIN_DUCK_DB: f64 = -50.0;

/// What configures a [`Duck`]. Lengths are frames; [`DuckParams::from_ms`]
/// makes them from milliseconds and a rate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DuckParams {
    /// The music's gain under the clip, linear, in `0.0..=1.0`.
    pub duck_gain: f32,
    /// The clip's gain, linear, at most `1 - duck_gain`.
    pub clip_gain: f32,
    /// The way down, frames, `1..=MAX_RAMP_FRAMES`.
    pub duck_ramp_frames: u32,
    /// The way back, frames, `1..=MAX_RAMP_FRAMES`.
    pub restore_ramp_frames: u32,
    /// The absolute value no mixed sample exceeds, in `(0, 1]`.
    pub limit: f32,
}

/// The largest clip gain a duck gain leaves room for: `1 - duck_gain`,
/// rounded down so the two never sum above 1.
pub fn clip_gain_for(duck_gain: f32) -> f32 {
    let c = (1.0 - f64::from(duck_gain)) as f32;
    if c > 0.0 && f64::from(duck_gain) + f64::from(c) > 1.0 {
        f32::from_bits(c.to_bits() - 1)
    } else {
        c.max(0.0)
    }
}

/// Milliseconds to frames at `rate_hz`, rounded to nearest in integers, at
/// least one frame.
pub fn ms_to_frames(ms: u32, rate_hz: u32) -> u64 {
    ((u64::from(ms) * u64::from(rate_hz) + 500) / 1000).max(1)
}

impl DuckParams {
    /// Parameters from a depth in dB (0 or below, down to [`MIN_DUCK_DB`])
    /// and ramp lengths in milliseconds at `rate_hz`; the clip gain is
    /// [`clip_gain_for`] the depth and the limit [`DEFAULT_LIMIT`].
    pub fn from_ms(
        duck_db: f64,
        duck_ramp_ms: u32,
        restore_ramp_ms: u32,
        rate_hz: u32,
    ) -> Result<DuckParams, DspError> {
        if !(MIN_RATE_HZ..=MAX_RATE_HZ).contains(&rate_hz) {
            return Err(DspError::Rate);
        }
        if !(duck_db.is_finite() && (MIN_DUCK_DB..=0.0).contains(&duck_db)) {
            return Err(DspError::Gain);
        }
        let frames = |ms: u32| -> Result<u32, DspError> {
            let n = ms_to_frames(ms, rate_hz);
            if n > u64::from(MAX_RAMP_FRAMES) {
                return Err(DspError::TimeConstant);
            }
            Ok(n as u32)
        };
        let duck_gain = db_to_gain(duck_db) as f32;
        let p = DuckParams {
            duck_gain,
            clip_gain: clip_gain_for(duck_gain),
            duck_ramp_frames: frames(duck_ramp_ms)?,
            restore_ramp_frames: frames(restore_ramp_ms)?,
            limit: DEFAULT_LIMIT,
        };
        p.validate()?;
        Ok(p)
    }

    /// The defaults at `rate_hz`: [`DEFAULT_DUCK_DB`],
    /// [`DEFAULT_DUCK_RAMP_MS`], [`DEFAULT_RESTORE_RAMP_MS`].
    pub fn defaults(rate_hz: u32) -> Result<DuckParams, DspError> {
        DuckParams::from_ms(
            DEFAULT_DUCK_DB,
            DEFAULT_DUCK_RAMP_MS,
            DEFAULT_RESTORE_RAMP_MS,
            rate_hz,
        )
    }

    /// Refuses a gain outside its range (`Gain`), gains that sum above 1 or a
    /// limit outside `(0, 1]` (`Gain`), and a ramp of no frames or longer than
    /// [`MAX_RAMP_FRAMES`] (`TimeConstant`).
    pub fn validate(&self) -> Result<(), DspError> {
        let unit = |g: f32| g.is_finite() && (0.0..=1.0).contains(&g);
        if !(unit(self.duck_gain) && unit(self.clip_gain)) {
            return Err(DspError::Gain);
        }
        if f64::from(self.duck_gain) + f64::from(self.clip_gain) > 1.0 {
            return Err(DspError::Gain);
        }
        if !(self.limit.is_finite() && self.limit > 0.0 && self.limit <= 1.0) {
            return Err(DspError::Gain);
        }
        for n in [self.duck_ramp_frames, self.restore_ramp_frames] {
            if n == 0 || n > MAX_RAMP_FRAMES {
                return Err(DspError::TimeConstant);
            }
        }
        Ok(())
    }

    /// The most the music's gain moves from one frame to the next: the depth
    /// over the shorter ramp.
    pub fn max_gain_step(&self) -> f64 {
        let depth = 1.0 - f64::from(self.duck_gain);
        depth / f64::from(self.duck_ramp_frames.min(self.restore_ramp_frames))
    }
}

/// Where a [`Duck`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DuckState {
    /// The music passes untouched.
    Idle,
    /// The music is on its way down; no clip frame is taken.
    Ducking,
    /// The music is at the duck gain; each frame takes a clip frame.
    Holding,
    /// The music is on its way back.
    Restoring,
}

/// What one [`Duck::process`] call did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mixed {
    /// Frames written (the music's frame count).
    pub frames: usize,
    /// Clip frames taken from the front of the clip slice.
    pub clip_frames: usize,
}

/// A running duck, mix and restore over interleaved frames of `channels`.
#[derive(Clone, Debug, PartialEq)]
pub struct Duck {
    params: DuckParams,
    channels: usize,
    state: DuckState,
    /// 0 (untouched) to `full` (fully ducked).
    level: u64,
    full: u64,
    /// The clip has no frames beyond those handed in.
    finishing: bool,
    /// The restore fades what is left of a cancelled clip.
    fading: bool,
}

impl Duck {
    /// A mixer for streams of `channels` (1 to 8, the chain's bound); refuses
    /// what [`DuckParams::validate`] refuses.
    pub fn new(params: &DuckParams, channels: usize) -> Result<Duck, DspError> {
        params.validate()?;
        if !(1..=8).contains(&channels) {
            return Err(DspError::ChannelMap);
        }
        Ok(Duck {
            params: *params,
            channels,
            state: DuckState::Idle,
            level: 0,
            full: u64::from(params.duck_ramp_frames) * u64::from(params.restore_ramp_frames),
            finishing: false,
            fading: false,
        })
    }

    /// Its parameters.
    pub fn params(&self) -> &DuckParams {
        &self.params
    }

    /// Where it is.
    pub fn state(&self) -> DuckState {
        self.state
    }

    /// An announcement begins: the next frame is the first of the duck ramp
    /// (from wherever a restore had got to). Nothing while one is under way.
    pub fn start(&mut self) {
        if matches!(self.state, DuckState::Idle | DuckState::Restoring) {
            self.finishing = false;
            self.fading = false;
            self.state = if self.level == self.full {
                DuckState::Holding
            } else {
                DuckState::Ducking
            };
        }
    }

    /// The clip has no frames beyond those handed to `process`: the frame
    /// after its last begins the restore.
    pub fn finish(&mut self) {
        if matches!(self.state, DuckState::Ducking | DuckState::Holding) {
            self.finishing = true;
        }
    }

    /// The announcement is called off: the next frame begins the restore from
    /// the gain the music is at, and a clip that was playing fades out with
    /// it.
    pub fn cancel(&mut self) {
        match self.state {
            DuckState::Ducking => {
                self.fading = false;
                self.state = DuckState::Restoring;
            }
            DuckState::Holding => {
                self.fading = true;
                self.state = DuckState::Restoring;
            }
            DuckState::Idle | DuckState::Restoring => {}
        }
    }

    fn gains(&self) -> (f32, f32) {
        if self.level == self.full {
            return (self.params.duck_gain, self.params.clip_gain);
        }
        let p = self.level as f64 / self.full as f64;
        let g = 1.0 - (1.0 - f64::from(self.params.duck_gain)) * p;
        (g as f32, (f64::from(self.params.clip_gain) * p) as f32)
    }

    fn clamp(&self, v: f32) -> f32 {
        let l = self.params.limit;
        if v > l {
            l
        } else if v < -l {
            -l
        } else {
            v
        }
    }

    /// Mixes `music` (whole interleaved frames) into `out` (the same size),
    /// taking clip frames from the front of `clip` (whole frames of the same
    /// channel count; the clip's next frames, any number of them) as the
    /// state asks. Returns how many of each it used; the caller hands the
    /// clip's remainder to the next call.
    pub fn process(
        &mut self,
        music: &[f32],
        clip: &[f32],
        out: &mut [f32],
    ) -> Result<Mixed, DspError> {
        let n = self.channels;
        if !music.len().is_multiple_of(n)
            || !clip.len().is_multiple_of(n)
            || out.len() != music.len()
        {
            return Err(DspError::Buffer);
        }
        let (d, r) = (
            u64::from(self.params.duck_ramp_frames),
            u64::from(self.params.restore_ramp_frames),
        );
        let mut clips = clip.chunks_exact(n);
        let total = clip.len() / n;
        let mut taken = 0;
        for (x, y) in music.chunks_exact(n).zip(out.chunks_exact_mut(n)) {
            // Holding with the clip played out: this frame begins the restore.
            if self.state == DuckState::Holding && self.finishing && taken == total {
                self.fading = false;
                self.state = DuckState::Restoring;
            }
            let voice = match self.state {
                DuckState::Idle => {
                    y.copy_from_slice(x);
                    continue;
                }
                DuckState::Ducking => {
                    self.level = (self.level + r).min(self.full);
                    if self.level == self.full {
                        self.state = DuckState::Holding;
                    }
                    None
                }
                DuckState::Holding => clips.next(),
                DuckState::Restoring => {
                    self.level = self.level.saturating_sub(d);
                    if self.level == 0 {
                        self.state = DuckState::Idle;
                        self.finishing = false;
                        self.fading = false;
                        y.copy_from_slice(x);
                        continue;
                    }
                    if self.fading {
                        clips.next()
                    } else {
                        None
                    }
                }
            };
            let (g, w) = self.gains();
            match voice {
                Some(c) => {
                    taken += 1;
                    for ((o, &m), &v) in y.iter_mut().zip(x).zip(c) {
                        *o = self.clamp(m * g + v * w);
                    }
                }
                None => {
                    for (o, &m) in y.iter_mut().zip(x) {
                        *o = self.clamp(m * g);
                    }
                }
            }
        }
        Ok(Mixed {
            frames: music.len() / n,
            clip_frames: taken,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> DuckParams {
        DuckParams {
            duck_gain: 0.25,
            clip_gain: 0.75,
            duck_ramp_frames: 8,
            restore_ramp_frames: 16,
            limit: 1.0,
        }
    }

    /// The music's gain per frame: a mono run over a music of ones.
    fn gains_of(d: &mut Duck, frames: usize, clip: &[f32]) -> (Vec<f32>, usize) {
        let music = vec![1f32; frames];
        let mut out = vec![0f32; frames];
        let m = d.process(&music, clip, &mut out).unwrap();
        (out, m.clip_frames)
    }

    #[test]
    fn the_defaults_and_their_frames() {
        let p = DuckParams::defaults(48_000).unwrap();
        assert_eq!(p.duck_ramp_frames, 9600);
        assert_eq!(p.restore_ramp_frames, 24_000);
        assert_eq!(p.duck_gain, 0.1);
        assert!(f64::from(p.duck_gain) + f64::from(p.clip_gain) <= 1.0);
        assert!((f64::from(p.clip_gain) - 0.9).abs() < 1e-7);
        assert_eq!(p.limit, 1.0);
        assert_eq!(ms_to_frames(200, 44_100), 8820);
        assert_eq!(ms_to_frames(0, 48_000), 1);
    }

    #[test]
    fn refusals() {
        assert_eq!(DuckParams::defaults(7999), Err(DspError::Rate));
        assert_eq!(
            DuckParams::from_ms(1.0, 200, 500, 48_000),
            Err(DspError::Gain)
        );
        assert_eq!(
            DuckParams::from_ms(-51.0, 200, 500, 48_000),
            Err(DspError::Gain)
        );
        assert_eq!(
            DuckParams::from_ms(-20.0, 100_000, 500, 48_000),
            Err(DspError::TimeConstant)
        );
        let over = DuckParams {
            clip_gain: 0.8,
            ..params()
        };
        assert_eq!(over.validate(), Err(DspError::Gain));
        let none = DuckParams {
            duck_ramp_frames: 0,
            ..params()
        };
        assert_eq!(none.validate(), Err(DspError::TimeConstant));
        let limit = DuckParams {
            limit: 1.5,
            ..params()
        };
        assert_eq!(limit.validate(), Err(DspError::Gain));
        assert_eq!(Duck::new(&params(), 0).err(), Some(DspError::ChannelMap));
        let mut d = Duck::new(&params(), 2).unwrap();
        let mut out = [0f32; 3];
        assert_eq!(d.process(&[0.0; 3], &[], &mut out), Err(DspError::Buffer));
        let mut out = [0f32; 4];
        assert_eq!(
            d.process(&[0.0; 4], &[0.0], &mut out),
            Err(DspError::Buffer)
        );
    }

    #[test]
    fn a_clip_gain_never_sums_above_one() {
        for bits in (0..=0x3f80_0000u32).step_by(4099) {
            let g = f32::from_bits(bits);
            let c = clip_gain_for(g);
            assert!(f64::from(g) + f64::from(c) <= 1.0, "{g} + {c}");
            assert!((0.0..=1.0).contains(&c));
        }
        assert_eq!(clip_gain_for(1.0), 0.0);
        assert_eq!(clip_gain_for(0.0), 1.0);
    }

    #[test]
    fn idle_is_the_music_bit_for_bit_and_takes_no_clip() {
        let mut d = Duck::new(&params(), 1).unwrap();
        // Above the limit, a negative zero, a subnormal: copied, not computed.
        let music = [1.5f32, -0.0, 1e-40, -3.0, f32::MIN_POSITIVE];
        let mut out = [0f32; 5];
        let m = d.process(&music, &[0.5; 5], &mut out).unwrap();
        assert_eq!(m.clip_frames, 0);
        for (a, b) in music.iter().zip(&out) {
            assert_eq!(a.to_bits(), b.to_bits());
        }
        assert_eq!(d.state(), DuckState::Idle);
    }

    #[test]
    fn down_in_d_frames_hold_and_back_in_r_frames() {
        let mut d = Duck::new(&params(), 1).unwrap();
        d.start();
        d.finish();
        let clip = [0f32; 4];
        let (g, taken) = gains_of(&mut d, 40, &clip);
        assert_eq!(taken, 4);
        // Frame n of the way down is at 1 - 0.75 (n + 1) / 8.
        for (n, v) in g.iter().enumerate().take(8) {
            assert_eq!(*v, 1.0 - 0.75 * (n as f32 + 1.0) / 8.0);
        }
        assert_eq!(&g[7..12], &[0.25; 5]);
        // The clip's four frames are 8..12; the restore is frames 12..28.
        for k in 0..16 {
            assert_eq!(g[12 + k], 0.25 + 0.75 * (k as f32 + 1.0) / 16.0);
        }
        assert_eq!(&g[27..], &[1.0; 13]);
        assert_eq!(d.state(), DuckState::Idle);
    }

    #[test]
    fn holding_without_clip_frames_stays_ducked_until_finish() {
        let mut d = Duck::new(&params(), 1).unwrap();
        d.start();
        let (g, taken) = gains_of(&mut d, 30, &[]);
        assert_eq!(taken, 0);
        assert_eq!(g[29], 0.25);
        assert_eq!(d.state(), DuckState::Holding);
        d.finish();
        let (g, _) = gains_of(&mut d, 16, &[]);
        assert!(g[0] > 0.25);
        assert_eq!(g[15], 1.0);
        assert_eq!(d.state(), DuckState::Idle);
    }

    #[test]
    fn a_cancel_on_the_way_down_turns_round_without_a_jump() {
        let mut d = Duck::new(&params(), 1).unwrap();
        d.start();
        let (down, _) = gains_of(&mut d, 4, &[1.0; 4]);
        assert_eq!(down[3], 1.0 - 0.75 * 4.0 / 8.0);
        d.cancel();
        let (up, taken) = gains_of(&mut d, 16, &[1.0; 16]);
        assert_eq!(taken, 0, "no clip frame was ever played");
        // Half way down, so half the restore: 8 frames, each depth / 16.
        assert!((up[0] - down[3] - 0.75 / 16.0).abs() < 1e-7);
        assert_eq!(&up[7..], &[1.0; 9]);
        assert_eq!(d.state(), DuckState::Idle);
    }

    #[test]
    fn a_cancelled_clip_fades_with_the_restore() {
        let mut d = Duck::new(&params(), 1).unwrap();
        d.start();
        let music = [0f32; 12];
        let mut out = [0f32; 12];
        let m = d.process(&music, &[1.0; 100], &mut out).unwrap();
        assert_eq!(m.clip_frames, 4);
        assert_eq!(&out[8..], &[0.75; 4]);
        d.cancel();
        let mut out = [9f32; 20];
        let m = d.process(&[0.0; 20], &[1.0; 96], &mut out).unwrap();
        assert_eq!(m.clip_frames, 15);
        for (k, v) in out.iter().enumerate().take(15) {
            assert_eq!(*v, 0.75 * (15 - k) as f32 / 16.0);
        }
        assert_eq!(&out[15..], &[0.0; 5]);
    }

    #[test]
    fn a_start_during_the_restore_ducks_again_from_there() {
        let mut d = Duck::new(&params(), 1).unwrap();
        d.start();
        d.finish();
        gains_of(&mut d, 8, &[]);
        let (up, _) = gains_of(&mut d, 8, &[]);
        assert_eq!(d.state(), DuckState::Restoring);
        d.start();
        let (down, _) = gains_of(&mut d, 8, &[]);
        assert!((up[7] - down[0] - 0.75 / 8.0).abs() < 1e-7);
        assert_eq!(down[3], 0.25);
        assert_eq!(d.state(), DuckState::Holding);
    }

    #[test]
    fn the_limit_holds_for_inputs_inside_it() {
        let p = DuckParams {
            limit: 0.5,
            clip_gain: clip_gain_for(0.25),
            ..params()
        };
        let mut d = Duck::new(&p, 1).unwrap();
        d.start();
        let mut out = [0f32; 64];
        d.process(&[0.5; 64], &[0.5; 64], &mut out).unwrap();
        assert!(out.iter().all(|v| v.abs() <= 0.5));
        assert_eq!(out[20], 0.5);
    }
}
