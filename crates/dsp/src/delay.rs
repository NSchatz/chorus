//! Whole-sample delay lines.
//!
//! "Let the input signal be denoted x(n), n=0,1,2,..., and let the delay-line
//! length be M samples. Then the output signal y(n) is specified by the
//! relation y(n) = x(n-M)" (J. O. Smith III, "Physical Audio Signal
//! Processing", Delay Lines, <https://ccrma.stanford.edu/~jos/pasp/Delay_Lines.html>,
//! read 2026-10-01), with the line starting full of zeros. A delay is a
//! whole number of samples (BRIEF section 5.6 asks for per-output delay to
//! align drivers and rooms; a fraction of a sample at 48 kHz is 21 us, far
//! inside what a driver alignment needs) with a fixed maximum, so the C mirror
//! holds it in fixed storage and never allocates on the audio path.

use crate::DspError;

/// The longest delay one line holds: 50 ms at 96 kHz (the design envelope's
/// "at least 50 ms at 96 kHz"; 100 ms at 48 kHz).
pub const MAX_FRAMES: usize = 4800;

/// Microseconds to whole frames at `rate_hz`, rounded to nearest (halves up),
/// in integers so both languages agree exactly.
pub fn frames_for_us(delay_us: u32, rate_hz: u32) -> u64 {
    (delay_us as u64 * rate_hz as u64 + 500_000) / 1_000_000
}

/// One channel's delay line.
#[derive(Clone, Debug, PartialEq)]
pub struct Delay {
    line: Vec<f32>,
    pos: usize,
}

impl Delay {
    /// A line of `frames` samples, refused above [`MAX_FRAMES`]. Zero frames
    /// passes samples through.
    pub fn new(frames: usize) -> Result<Delay, DspError> {
        if frames > MAX_FRAMES {
            return Err(DspError::DelayTooLong);
        }
        Ok(Delay {
            line: vec![0.0; frames],
            pos: 0,
        })
    }

    /// The delay in frames.
    pub fn frames(&self) -> usize {
        self.line.len()
    }

    /// Clears the line to zeros.
    pub fn reset(&mut self) {
        self.line.iter_mut().for_each(|s| *s = 0.0);
        self.pos = 0;
    }

    /// One sample in, the sample from `frames` ago out.
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        if self.line.is_empty() {
            return x;
        }
        let y = self.line[self.pos];
        self.line[self.pos] = x;
        self.pos += 1;
        if self.pos == self.line.len() {
            self.pos = 0;
        }
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delays_by_whole_frames_and_refuses_above_the_maximum() {
        let mut d = Delay::new(3).unwrap();
        let out: Vec<f32> = (1..=6).map(|x| d.process(x as f32)).collect();
        assert_eq!(out, [0.0, 0.0, 0.0, 1.0, 2.0, 3.0]);
        assert!(Delay::new(MAX_FRAMES).is_ok());
        assert_eq!(Delay::new(MAX_FRAMES + 1), Err(DspError::DelayTooLong));
        assert_eq!(frames_for_us(1000, 48000), 48);
        assert_eq!(frames_for_us(10, 48000), 0); // 0.48 rounds down
        assert_eq!(frames_for_us(11, 48000), 1); // 0.528 rounds up
    }
}
