//! The Linkwitz-Riley 4th-order (LR4) crossover.
//!
//! "Cascading two 2nd-order Butterworth filters creates a LR-4 design"; at the
//! crossover each output is down 6 dB (".707 x .707 = .5"); "the low pass and
//! high pass outputs are everywhere in phase"; and "the summed response is
//! perfectly flat", an all-pass (D. Bohn, "Linkwitz-Riley Crossovers: A
//! Primer", RaneNote 160, <https://www.ranecommercial.com/legacy/note160.html>,
//! read 2026-10-01; the -6 dB point also at
//! <https://www.linkwitzlab.com/filters.htm>, read 2026-10-01). Here each
//! branch is two identical cookbook sections (`biquad`, Q = 1/sqrt 2) at the
//! crossover frequency, and the high branch is not inverted: the analogue
//! branches are `1/B(s)^2` and `s^4/B(s)^2` with `B(s) = s^2 + sqrt2 s + 1`,
//! whose ratio `s^4` is real and positive on the `jw` axis, and whose sum
//! `(s^2 - sqrt2 s + 1)/(s^2 + sqrt2 s + 1)` is an all-pass. The bilinear
//! transform keeps both facts, which the shared fixtures check.

use crate::biquad::{complex_mul, Biquad, Coefficients, Kind};
use crate::DspError;

/// The Butterworth Q of each section: `1/sqrt 2`.
pub const BUTTERWORTH_Q: f64 = core::f64::consts::FRAC_1_SQRT_2;

/// The two designed sections of an LR4 split (each used twice).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lr4Design {
    pub low: Coefficients,
    pub high: Coefficients,
}

impl Lr4Design {
    /// Designs the split at `crossover_hz`.
    pub fn new(rate_hz: f64, crossover_hz: f64) -> Result<Lr4Design, DspError> {
        Ok(Lr4Design {
            low: Coefficients::design(Kind::Lowpass, rate_hz, crossover_hz, BUTTERWORTH_Q, 0.0)?,
            high: Coefficients::design(Kind::Highpass, rate_hz, crossover_hz, BUTTERWORTH_Q, 0.0)?,
        })
    }

    /// The low branch's complex response (the section's, squared).
    pub fn response_low(&self, freq_hz: f64, rate_hz: f64) -> (f64, f64) {
        let h = self.low.response(freq_hz, rate_hz);
        complex_mul(h, h)
    }

    /// The high branch's complex response (the section's, squared).
    pub fn response_high(&self, freq_hz: f64, rate_hz: f64) -> (f64, f64) {
        let h = self.high.response(freq_hz, rate_hz);
        complex_mul(h, h)
    }

    /// The summed response, low + high.
    pub fn response_sum(&self, freq_hz: f64, rate_hz: f64) -> (f64, f64) {
        let l = self.response_low(freq_hz, rate_hz);
        let h = self.response_high(freq_hz, rate_hz);
        (l.0 + h.0, l.1 + h.1)
    }
}

/// `20 log10 |h|` of a complex response.
pub fn db_of(h: (f64, f64)) -> f64 {
    10.0 * (h.0 * h.0 + h.1 * h.1).log10()
}

/// A running LR4 split of one channel.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Lr4 {
    low: [Biquad; 2],
    high: [Biquad; 2],
}

impl Lr4 {
    /// A split with zero state.
    pub fn new(design: &Lr4Design) -> Lr4 {
        let mut s = Lr4::default();
        s.set(design);
        s
    }

    /// Replaces the coefficients and keeps the state.
    pub fn set(&mut self, design: &Lr4Design) {
        for q in &mut self.low {
            q.set(&design.low);
        }
        for q in &mut self.high {
            q.set(&design.high);
        }
    }

    /// Clears the state.
    pub fn reset(&mut self) {
        for q in self.low.iter_mut().chain(self.high.iter_mut()) {
            q.reset();
        }
    }

    /// One sample in, `(low, high)` out.
    #[inline]
    pub fn split(&mut self, x: f32) -> (f32, f32) {
        (self.low(x), self.high(x))
    }

    /// The low branch alone (the subwoofer feed).
    #[inline]
    pub fn low(&mut self, x: f32) -> f32 {
        let v = self.low[0].process(x);
        self.low[1].process(v)
    }

    /// The high branch alone (a main in a set with a subwoofer).
    #[inline]
    pub fn high(&mut self, x: f32) -> f32 {
        let v = self.high[0].process(x);
        self.high[1].process(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branches_are_in_phase_and_sum_flat() {
        let d = Lr4Design::new(48000.0, 80.0).unwrap();
        for f in [10.0, 40.0, 80.0, 160.0, 1000.0, 20000.0] {
            let l = d.response_low(f, 48000.0);
            let h = d.response_high(f, 48000.0);
            // in phase: the cross product of the two phasors is zero and the
            // dot product is positive.
            let cross = l.0 * h.1 - l.1 * h.0;
            let dot = l.0 * h.0 + l.1 * h.1;
            assert!(cross.abs() < 1e-9 && dot >= 0.0, "{f} Hz");
            assert!(db_of(d.response_sum(f, 48000.0)).abs() < 1e-9, "{f} Hz");
        }
        assert!((db_of(d.response_low(80.0, 48000.0)) + 6.0206).abs() < 1e-3);
    }
}
