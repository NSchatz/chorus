//! The look-ahead limiter: the last stage of the chain, the one that makes
//! "no DSP boost lifts a room above its limit" (K81, I10) true sample by
//! sample.
//!
//! What a brickwall limiter promises is that "the output level never exceeds
//! a set limit", bought with a short delay ("the plug-in introduces a 1
//! millisecond delay in processing, which enables its fast attack"; Steinberg,
//! "Brickwall Limiter",
//! <https://www.steinberg.help/r/groove-agent/6.0/en/halion/topics/effects_reference/brickwalllimiter_r.html>,
//! read 2026-10-01). The shape here is chorus's own:
//!
//! - The signal is delayed by the look-ahead, `L` frames (a pure delay).
//! - Every input frame's required gain is `r = ceiling / peak` when its peak
//!   over the channels exceeds the ceiling, else exactly 1. The last `L + 1`
//!   of them are kept: the one leaving the delay now (`d = 0`) up to the one
//!   that just arrived (`d = L`).
//! - The gain moves by a linear-in-time ramp: from the previous gain `g`, the
//!   next gain is the least of the release candidate `1 - (1 - g) a`, the
//!   required gain `r` of the frame playing now (`d = 0`), and, for every
//!   later kept frame whose `r` is below `g`, `g + (r - g) / (d + 1)`: the
//!   step that would reach `r` exactly when that frame plays. So the frame
//!   playing now is never louder than its required gain allows, and the gain
//!   is on its way down before a peak arrives.
//! - The output is then clamped to `[-ceiling, ceiling]`, which turns "never
//!   exceeds" from a property of exact arithmetic into a property of `f32`.
//!
//! Stereo- (multi-channel-) linked: one gain for every channel, so the image
//! does not move. When no kept frame needs less than unity and the gain is at
//! unity, every term is exactly 1, so input that never exceeds the ceiling
//! comes out as itself delayed, bit for bit (the flat-chain fixture relies on
//! that). The release returns towards unity exponentially, on the distance
//! `1 - g` kept as its own state, and snaps to exactly 1.0 once that distance
//! falls below [`RELEASE_SNAP`] (so a chain that once limited is bit-exact
//! again once it has released).
//!
//! The cost is `L + 1` multiply-adds per frame; at the ASSUMED 2 ms look-ahead
//! that is 97 at 48 kHz.

use crate::delay::Delay;
use crate::DspError;

/// The longest look-ahead: 2 ms at 384 kHz.
pub const MAX_LOOKAHEAD_FRAMES: usize = 768;

/// The chain's look-ahead, microseconds. ASSUMED: 2 ms, twice the 1 ms of the
/// limiter cited above; not measured.
pub const DEFAULT_LOOKAHEAD_US: u32 = 2000;

/// The chain's release time constant, ms. ASSUMED: 100 ms; not measured.
pub const DEFAULT_RELEASE_MS: f64 = 100.0;

/// The distance from unity below which the release lands on exactly 1.0.
/// ASSUMED: 1e-6 (-0.0000087 dB), far below anything audible.
pub const RELEASE_SNAP: f32 = 1e-6;

/// The release coefficient `exp(-1 / (release_s * rate))`, designed in `f64`.
pub fn release_coefficient(release_ms: f64, rate_hz: f64) -> Result<f32, DspError> {
    if !(release_ms.is_finite() && release_ms > 0.0 && rate_hz > 0.0) {
        return Err(DspError::TimeConstant);
    }
    Ok((-1.0 / (release_ms / 1000.0 * rate_hz)).exp() as f32)
}

/// A running, channel-linked look-ahead limiter.
#[derive(Clone, Debug, PartialEq)]
pub struct Limiter {
    lines: Vec<Delay>,
    required: Vec<f32>,
    reciprocal: Vec<f32>,
    newest: usize,
    gain: f32,
    gap: f32,
    release: f32,
    ceiling: f32,
}

impl Limiter {
    /// A limiter over `channels` with a look-ahead of `lookahead_frames`
    /// (refused above [`MAX_LOOKAHEAD_FRAMES`]) and a ceiling of 1.0.
    pub fn new(
        channels: usize,
        lookahead_frames: usize,
        release_ms: f64,
        rate_hz: f64,
    ) -> Result<Limiter, DspError> {
        if lookahead_frames > MAX_LOOKAHEAD_FRAMES {
            return Err(DspError::Lookahead);
        }
        let release = release_coefficient(release_ms, rate_hz)?;
        let mut lines = Vec::with_capacity(channels);
        for _ in 0..channels {
            lines.push(Delay::new(lookahead_frames)?);
        }
        Ok(Limiter {
            lines,
            required: vec![1.0; lookahead_frames + 1],
            reciprocal: (0..=lookahead_frames)
                .map(|d| 1.0f32 / (d as f32 + 1.0))
                .collect(),
            newest: 0,
            gain: 1.0,
            gap: 0.0,
            release,
            ceiling: 1.0,
        })
    }

    /// The look-ahead in frames: the limiter's latency.
    pub fn lookahead(&self) -> usize {
        self.required.len() - 1
    }

    /// Sets the ceiling (linear, the largest magnitude an output sample may
    /// have). Negative or NaN is taken as 0.
    pub fn set_ceiling(&mut self, ceiling: f32) {
        self.ceiling = if ceiling > 0.0 { ceiling } else { 0.0 };
    }

    /// The ceiling in force.
    pub fn ceiling(&self) -> f32 {
        self.ceiling
    }

    /// The gain applied to the frame that left the delay last.
    pub fn gain(&self) -> f32 {
        self.gain
    }

    /// Clears the delay and the gain history.
    pub fn reset(&mut self) {
        self.lines.iter_mut().for_each(Delay::reset);
        self.required.iter_mut().for_each(|r| *r = 1.0);
        self.newest = 0;
        self.gain = 1.0;
        self.gap = 0.0;
    }

    /// One frame (one sample per channel), limited in place: the frame
    /// written back is the one that entered `lookahead` frames ago.
    pub fn process_frame(&mut self, frame: &mut [f32]) {
        let ceiling = self.ceiling;
        let mut peak = 0.0f32;
        for &x in frame.iter() {
            let a = x.abs();
            if a > peak {
                peak = a;
            }
        }
        let r = if peak > ceiling { ceiling / peak } else { 1.0 };
        let len = self.required.len();
        self.newest += 1;
        if self.newest == len {
            self.newest = 0;
        }
        self.required[self.newest] = r;

        let previous = self.gain;
        // The release moves the distance below unity, kept as its own state:
        // `1 - (1 - g) a` recomputed from `g` would round back to `g` once
        // the step is under half an ulp, and the gain would stall below 1.
        let mut gap = self.gap * self.release;
        if gap < RELEASE_SNAP {
            gap = 0.0;
        }
        let release = 1.0 - gap;
        let mut g = release;
        // The oldest kept frame (d = 0) sits just after the newest: the gain
        // never exceeds its required gain. Every later frame that needs less
        // than the previous gain pulls the gain down on its ramp.
        let mut idx = self.newest + 1;
        if idx == len {
            idx = 0;
        }
        if self.required[idx] < g {
            g = self.required[idx];
        }
        for d in 1..len {
            idx += 1;
            if idx == len {
                idx = 0;
            }
            let r = self.required[idx];
            if r < previous {
                let term = previous + (r - previous) * self.reciprocal[d];
                if term < g {
                    g = term;
                }
            }
        }
        self.gain = g;
        self.gap = if g < release { 1.0 - g } else { gap };

        for (x, line) in frame.iter_mut().zip(self.lines.iter_mut()) {
            let mut y = line.process(*x) * g;
            if y > ceiling {
                y = ceiling;
            } else if y < -ceiling {
                y = -ceiling;
            } else if y.is_nan() {
                y = 0.0;
            }
            *x = y;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_input_is_the_input_delayed_bit_for_bit() {
        let mut l = Limiter::new(2, 96, 100.0, 48000.0).unwrap();
        l.set_ceiling(0.5);
        let input: Vec<f32> = (0..2000).map(|n| 0.5 * ((n as f32) * 0.01).sin()).collect();
        let mut out = Vec::new();
        for &x in &input {
            let mut f = [x, -x];
            l.process_frame(&mut f);
            out.push(f);
        }
        for n in 96..2000 {
            assert_eq!(out[n][0].to_bits(), input[n - 96].to_bits());
            assert_eq!(out[n][1].to_bits(), (-input[n - 96]).to_bits());
        }
    }

    #[test]
    fn loud_input_never_exceeds_the_ceiling_and_releases_to_unity() {
        let mut l = Limiter::new(1, 96, 10.0, 48000.0).unwrap();
        l.set_ceiling(0.25);
        for n in 0..48000 {
            let x = if n < 4800 {
                3.0 * ((n as f32) * 0.05).sin()
            } else {
                0.01
            };
            let mut f = [x];
            l.process_frame(&mut f);
            assert!(f[0].abs() <= 0.25, "frame {n}: {}", f[0]);
        }
        assert_eq!(l.gain(), 1.0);
    }

    #[test]
    fn the_lookahead_has_a_fixed_maximum() {
        assert!(Limiter::new(8, MAX_LOOKAHEAD_FRAMES, 100.0, 384000.0).is_ok());
        assert_eq!(
            Limiter::new(8, MAX_LOOKAHEAD_FRAMES + 1, 100.0, 384000.0),
            Err(DspError::Lookahead)
        );
    }
}
