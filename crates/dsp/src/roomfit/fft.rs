//! A radix-2 FFT, owned here: the room fit needs one transform size class (powers of two, a few
//! hundred thousand points at most, on the server), and twenty-odd lines of Cooley-Tukey are a
//! cheaper price than a dependency in a crate that has none (CLAUDE.md working agreement 3:
//! build what is small and instructive).
//!
//! Iterative decimation in time: the input is put in bit-reversed order, then log2(n) passes of
//! butterflies combine pairs of half-size transforms. Reference: Cooley and Tukey, "An algorithm
//! for the machine calculation of complex Fourier series", Mathematics of Computation 19(90),
//! 1965, pp. 297-301, https://doi.org/10.1090/S0025-5718-1965-0178586-1 (the bibliographic
//! record, https://research.ibm.com/publications/an-algorithm-for-the-machine-calculation-of-complex-fourier-series,
//! read 2026-10-01; the algorithm is the textbook radix-2 form).
//! The twiddle factors are computed directly from `sin`/`cos` per pass rather than by repeated
//! multiplication, so the rounding error does not accumulate across a pass.
//!
//! Private to `roomfit` on purpose: the visualizer's band analysis may want a different shape
//! (a real-input transform of a fixed small size), and a shared FFT is a decision for whichever
//! change needs it second.

use std::f64::consts::PI;

/// A complex number, just enough of one for the transform and a spectrum.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Complex {
    pub re: f64,
    pub im: f64,
}

impl Complex {
    pub const ZERO: Complex = Complex { re: 0.0, im: 0.0 };

    pub fn new(re: f64, im: f64) -> Complex {
        Complex { re, im }
    }

    pub fn mul(self, o: Complex) -> Complex {
        Complex::new(
            self.re * o.re - self.im * o.im,
            self.re * o.im + self.im * o.re,
        )
    }

    /// |z|^2, the power, without the square root a magnitude would need.
    pub fn norm_sqr(self) -> f64 {
        self.re * self.re + self.im * self.im
    }
}

/// The forward transform in place, X[k] = sum x[n] e^(-2 pi i k n / N). Panics unless the
/// length is a power of two (every caller pads to one with `next_pow2`).
pub fn forward(data: &mut [Complex]) {
    transform(data, -1.0);
}

/// The inverse transform in place, scaled by 1/N so `inverse(forward(x)) == x` to rounding.
pub fn inverse(data: &mut [Complex]) {
    transform(data, 1.0);
    let scale = 1.0 / data.len() as f64;
    for z in data.iter_mut() {
        z.re *= scale;
        z.im *= scale;
    }
}

/// The smallest power of two at least `n` (and at least 1).
pub fn next_pow2(n: usize) -> usize {
    n.max(1).next_power_of_two()
}

fn transform(data: &mut [Complex], sign: f64) {
    let n = data.len();
    assert!(n.is_power_of_two(), "FFT length {n} is not a power of two");
    // Bit-reversal permutation.
    let bits = n.trailing_zeros();
    if bits > 0 {
        for i in 0..n {
            let j = i.reverse_bits() >> (usize::BITS - bits);
            if j > i {
                data.swap(i, j);
            }
        }
    }
    let mut len = 2;
    while len <= n {
        let half = len / 2;
        let step = sign * 2.0 * PI / len as f64;
        for k in 0..half {
            let angle = step * k as f64;
            let w = Complex::new(angle.cos(), angle.sin());
            let mut start = 0;
            while start < n {
                let a = data[start + k];
                let b = data[start + k + half].mul(w);
                data[start + k] = Complex::new(a.re + b.re, a.im + b.im);
                data[start + k + half] = Complex::new(a.re - b.re, a.im - b.im);
                start += len;
            }
        }
        len *= 2;
    }
}

/// The linear convolution of two real sequences through the transform: both zero-padded to a
/// power of two at least `a.len() + b.len() - 1` long, so the circular product does not wrap.
pub fn convolve(a: &[f64], b: &[f64]) -> Vec<f64> {
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }
    let out_len = a.len() + b.len() - 1;
    let n = next_pow2(out_len);
    let mut fa = padded(a, n);
    let mut fb = padded(b, n);
    forward(&mut fa);
    forward(&mut fb);
    for (x, y) in fa.iter_mut().zip(fb.iter()) {
        *x = x.mul(*y);
    }
    inverse(&mut fa);
    fa.iter().take(out_len).map(|z| z.re).collect()
}

/// A real sequence as complex values, zero-padded to `n`.
pub fn padded(x: &[f64], n: usize) -> Vec<Complex> {
    let mut out = vec![Complex::ZERO; n];
    for (o, v) in out.iter_mut().zip(x.iter()) {
        o.re = *v;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The transform against the definition, an O(N^2) DFT, on a length where both are cheap.
    #[test]
    fn matches_the_direct_dft() {
        let n = 64;
        let x: Vec<Complex> = (0..n)
            .map(|i| Complex::new((i as f64 * 0.37).sin() + 0.1 * i as f64, (i as f64).cos()))
            .collect();
        let mut fast = x.clone();
        forward(&mut fast);
        for (k, got) in fast.iter().enumerate() {
            let mut want = Complex::ZERO;
            for (i, v) in x.iter().enumerate() {
                let a = -2.0 * PI * (k * i) as f64 / n as f64;
                let t = v.mul(Complex::new(a.cos(), a.sin()));
                want.re += t.re;
                want.im += t.im;
            }
            assert!((got.re - want.re).abs() < 1e-9 && (got.im - want.im).abs() < 1e-9);
        }
        inverse(&mut fast);
        for (a, b) in fast.iter().zip(x.iter()) {
            assert!((a.re - b.re).abs() < 1e-12 && (a.im - b.im).abs() < 1e-12);
        }
    }

    /// Convolution through the transform against the direct sum.
    #[test]
    fn convolution_matches_the_direct_sum() {
        let a = [1.0, -2.0, 0.5, 3.0, 0.25];
        let b = [0.5, 1.5, -1.0];
        let got = convolve(&a, &b);
        assert_eq!(got.len(), a.len() + b.len() - 1);
        for (n, g) in got.iter().enumerate() {
            let mut want = 0.0;
            for (i, av) in a.iter().enumerate() {
                if n >= i && n - i < b.len() {
                    want += av * b[n - i];
                }
            }
            assert!((g - want).abs() < 1e-12, "sample {n}: {g} vs {want}");
        }
    }
}
