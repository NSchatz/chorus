//! Second-order sections: the RBJ Audio EQ Cookbook designs and their
//! processing.
//!
//! The designs are the cookbook's, as the W3C published it ("Audio EQ
//! Cookbook", W3C Working Group Note 8 June 2021,
//! <https://www.w3.org/TR/audio-eq-cookbook/>, read 2026-10-01), written here
//! from the formulas: `A = 10^(dBgain/40)`, `w0 = 2 pi f0 / Fs`,
//! `alpha = sin(w0) / (2 Q)`, and for each type its `b0 b1 b2 a0 a1 a2`, all
//! normalised by `a0` so `a0` is 1 and is not stored. The cookbook's own
//! statements are what the fixtures hold the designs to: the peaking filter's
//! gain at `f0` is `dBgain`; a shelf's `f0` is its midpoint, where the gain is
//! `dBgain/2`; the LPF and HPF analogue prototypes `1/(s^2 + s/Q + 1)` and
//! `s^2/(s^2 + s/Q + 1)` have magnitude `Q` at `f0`; the 0 dB band-pass peaks at
//! 0 dB; the notch has a zero at `f0`; the all-pass has unit magnitude.
//!
//! Processing is Transposed Direct Form II in `f32`, in this order (the C
//! mirror does the same, so the two round identically):
//!
//! ```text
//! y  = b0*x + z1
//! z1 = (b1*x - a1*y) + z2
//! z2 = b2*x - a2*y
//! ```

use crate::DspError;

/// The eight cookbook designs. The band-pass is the "constant 0 dB peak gain"
/// variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `H(s) = 1 / (s^2 + s/Q + 1)`.
    Lowpass,
    /// `H(s) = s^2 / (s^2 + s/Q + 1)`.
    Highpass,
    /// `H(s) = (s/Q) / (s^2 + s/Q + 1)`: 0 dB at `f0`.
    Bandpass,
    /// `H(s) = (s^2 + 1) / (s^2 + s/Q + 1)`.
    Notch,
    /// `H(s) = (s^2 - s/Q + 1) / (s^2 + s/Q + 1)`.
    Allpass,
    /// The peaking EQ: `dBgain` at `f0`.
    Peaking,
    /// The low shelf: `dBgain` towards DC, `dBgain/2` at `f0`.
    LowShelf,
    /// The high shelf: `dBgain` towards Nyquist, `dBgain/2` at `f0`.
    HighShelf,
}

impl Kind {
    /// The fixture name of a design (`lowpass`, `highpass`, `bandpass`,
    /// `notch`, `allpass`, `peaking`, `lowshelf`, `highshelf`).
    pub fn from_name(name: &str) -> Option<Kind> {
        Some(match name {
            "lowpass" => Kind::Lowpass,
            "highpass" => Kind::Highpass,
            "bandpass" => Kind::Bandpass,
            "notch" => Kind::Notch,
            "allpass" => Kind::Allpass,
            "peaking" => Kind::Peaking,
            "lowshelf" => Kind::LowShelf,
            "highshelf" => Kind::HighShelf,
            _ => return None,
        })
    }
}

/// A designed section, in `f64`, normalised so `a0 = 1`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Coefficients {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

impl Coefficients {
    /// The section that passes its input unchanged.
    pub const IDENTITY: Coefficients = Coefficients {
        b0: 1.0,
        b1: 0.0,
        b2: 0.0,
        a1: 0.0,
        a2: 0.0,
    };

    /// Designs one cookbook section. `gain_db` is read only by the peaking
    /// and shelving designs. Refused: a rate that is not finite and positive,
    /// `f0` outside `(0, rate/2)`, a `q` that is not finite and positive, a
    /// gain that is not finite.
    pub fn design(
        kind: Kind,
        rate_hz: f64,
        f0_hz: f64,
        q: f64,
        gain_db: f64,
    ) -> Result<Coefficients, DspError> {
        if !(rate_hz.is_finite() && rate_hz > 0.0) {
            return Err(DspError::Rate);
        }
        if !(f0_hz.is_finite() && f0_hz > 0.0 && f0_hz < rate_hz / 2.0) {
            return Err(DspError::Frequency);
        }
        if !(q.is_finite() && q > 0.0) {
            return Err(DspError::Q);
        }
        if !gain_db.is_finite() {
            return Err(DspError::Gain);
        }
        let a = 10f64.powf(gain_db / 40.0);
        let w0 = 2.0 * core::f64::consts::PI * f0_hz / rate_hz;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);
        let (b0, b1, b2, a0, a1, a2) = match kind {
            Kind::Lowpass => (
                (1.0 - cos_w0) / 2.0,
                1.0 - cos_w0,
                (1.0 - cos_w0) / 2.0,
                1.0 + alpha,
                -2.0 * cos_w0,
                1.0 - alpha,
            ),
            Kind::Highpass => (
                (1.0 + cos_w0) / 2.0,
                -(1.0 + cos_w0),
                (1.0 + cos_w0) / 2.0,
                1.0 + alpha,
                -2.0 * cos_w0,
                1.0 - alpha,
            ),
            Kind::Bandpass => (alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * cos_w0, 1.0 - alpha),
            Kind::Notch => (
                1.0,
                -2.0 * cos_w0,
                1.0,
                1.0 + alpha,
                -2.0 * cos_w0,
                1.0 - alpha,
            ),
            Kind::Allpass => (
                1.0 - alpha,
                -2.0 * cos_w0,
                1.0 + alpha,
                1.0 + alpha,
                -2.0 * cos_w0,
                1.0 - alpha,
            ),
            Kind::Peaking => (
                1.0 + alpha * a,
                -2.0 * cos_w0,
                1.0 - alpha * a,
                1.0 + alpha / a,
                -2.0 * cos_w0,
                1.0 - alpha / a,
            ),
            Kind::LowShelf => {
                let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha),
                    2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0),
                    a * ((a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha),
                    (a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha,
                    -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0),
                    (a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha,
                )
            }
            Kind::HighShelf => {
                let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha),
                    -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0),
                    a * ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha),
                    (a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha,
                    2.0 * ((a - 1.0) - (a + 1.0) * cos_w0),
                    (a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha,
                )
            }
        };
        Ok(Coefficients {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        })
    }

    /// The complex response `H(e^{jw})` at `freq_hz`, as `(re, im)`, with
    /// `z^-1 = cos w - j sin w`.
    pub fn response(&self, freq_hz: f64, rate_hz: f64) -> (f64, f64) {
        let w = 2.0 * core::f64::consts::PI * freq_hz / rate_hz;
        let (c1, s1) = (w.cos(), -w.sin());
        let (c2, s2) = ((2.0 * w).cos(), -(2.0 * w).sin());
        let num = (
            self.b0 + self.b1 * c1 + self.b2 * c2,
            self.b1 * s1 + self.b2 * s2,
        );
        let den = (
            1.0 + self.a1 * c1 + self.a2 * c2,
            self.a1 * s1 + self.a2 * s2,
        );
        complex_div(num, den)
    }

    /// `20 log10 |H(e^{jw})|` at `freq_hz`: the evaluator the fixtures and the
    /// room-correction fit use.
    pub fn magnitude_db(&self, freq_hz: f64, rate_hz: f64) -> f64 {
        let (re, im) = self.response(freq_hz, rate_hz);
        10.0 * (re * re + im * im).log10()
    }
}

/// `a * b` for complex numbers as `(re, im)`.
pub fn complex_mul(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0)
}

/// `a / b` for complex numbers as `(re, im)`.
pub fn complex_div(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    let d = b.0 * b.0 + b.1 * b.1;
    ((a.0 * b.0 + a.1 * b.1) / d, (a.1 * b.0 - a.0 * b.1) / d)
}

/// One section running in `f32`: the coefficients rounded once, and the two
/// state words of Transposed Direct Form II.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Default for Biquad {
    fn default() -> Self {
        Biquad::new(&Coefficients::IDENTITY)
    }
}

impl Biquad {
    /// A section with zero state.
    pub fn new(c: &Coefficients) -> Biquad {
        let mut q = Biquad {
            b0: 0.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            z1: 0.0,
            z2: 0.0,
        };
        q.set(c);
        q
    }

    /// Replaces the coefficients and keeps the state, so a gain change on a
    /// running filter does not restart it.
    pub fn set(&mut self, c: &Coefficients) {
        self.b0 = c.b0 as f32;
        self.b1 = c.b1 as f32;
        self.b2 = c.b2 as f32;
        self.a1 = c.a1 as f32;
        self.a2 = c.a2 as f32;
    }

    /// Clears the state.
    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    /// The rounded coefficients `[b0, b1, b2, a1, a2]`.
    pub fn coefficients(&self) -> [f32; 5] {
        [self.b0, self.b1, self.b2, self.a1, self.a2]
    }

    /// One sample through the section.
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refusals_name_the_parameter() {
        let ok = Coefficients::design(Kind::Peaking, 48000.0, 1000.0, 1.0, 6.0);
        assert!(ok.is_ok());
        assert_eq!(
            Coefficients::design(Kind::Peaking, 48000.0, 24000.0, 1.0, 6.0),
            Err(DspError::Frequency)
        );
        assert_eq!(
            Coefficients::design(Kind::Peaking, 0.0, 1000.0, 1.0, 6.0),
            Err(DspError::Rate)
        );
        assert_eq!(
            Coefficients::design(Kind::Peaking, 48000.0, 1000.0, 0.0, 6.0),
            Err(DspError::Q)
        );
        assert_eq!(
            Coefficients::design(Kind::Peaking, 48000.0, 1000.0, 1.0, f64::NAN),
            Err(DspError::Gain)
        );
    }

    #[test]
    fn a_boost_then_the_same_cut_is_a_wire() {
        // The cookbook's own remark on its peaking Q: "a boost of N dB
        // followed by a cut of N dB for identical Q and f0/Fs results in a
        // precisely flat unity gain filter".
        let up = Coefficients::design(Kind::Peaking, 48000.0, 300.0, 2.0, 9.0).unwrap();
        let down = Coefficients::design(Kind::Peaking, 48000.0, 300.0, 2.0, -9.0).unwrap();
        for f in [20.0, 100.0, 300.0, 1000.0, 15000.0] {
            let sum = up.magnitude_db(f, 48000.0) + down.magnitude_db(f, 48000.0);
            assert!(sum.abs() < 1e-9, "{f} Hz: {sum}");
        }
    }

    #[test]
    fn the_identity_passes_samples_unchanged() {
        let mut q = Biquad::default();
        for x in [0.5f32, -0.25, 1.0, 0.0, 1e-7] {
            assert_eq!(q.process(x).to_bits(), x.to_bits());
        }
    }

    #[test]
    fn an_impulse_response_is_the_difference_equation() {
        let c = Coefficients::design(Kind::Lowpass, 48000.0, 1000.0, 0.7, 0.0).unwrap();
        let mut q = Biquad::new(&c);
        let [b0, b1, b2, a1, a2] = q.coefficients();
        // Direct Form I in f64 as the independent reference.
        let (mut x1, mut x2, mut y1, mut y2) = (0f64, 0f64, 0f64, 0f64);
        for n in 0..64 {
            let x = if n == 0 { 1.0 } else { 0.0 };
            let y =
                b0 as f64 * x + b1 as f64 * x1 + b2 as f64 * x2 - a1 as f64 * y1 - a2 as f64 * y2;
            let got = q.process(x as f32) as f64;
            assert!((got - y).abs() < 1e-6, "sample {n}: {got} vs {y}");
            (x2, x1, y2, y1) = (x1, x, y1, y);
        }
    }
}
