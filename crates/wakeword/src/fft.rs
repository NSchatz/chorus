// This file is a Rust port of the parts of KISS FFT that the TensorFlow Lite
// micro frontend uses: the 16-bit fixed-point real FFT (kiss_fft.c and
// tools/kiss_fftr.c, built with FIXED_POINT=16). KISS FFT's licence, which
// covers this file:
//
// Copyright (c) 2003-2010, Mark Borgerding
//
// All rights reserved.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
//
//     * Redistributions of source code must retain the above copyright notice,
//       this list of conditions and the following disclaimer.
//     * Redistributions in binary form must reproduce the above copyright
//       notice, this list of conditions and the following disclaimer in the
//       documentation and/or other materials provided with the distribution.
//     * Neither the author nor the names of any contributors may be used to
//       endorse or promote products derived from this software without
//       specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
// AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT OWNER OR CONTRIBUTORS BE
// LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
// CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.

//! The 512-point real FFT of the audio frontend, in 16-bit fixed point.
//!
//! The model was trained on features this exact arithmetic produced (every
//! butterfly divides by its radix and rounds, so the spectrum is scaled and
//! truncated in a particular way), so the port keeps KISS FFT's order of
//! operations, its rounding and its 16-bit wrap on every store. Only what a
//! 512-point transform needs is here: the inner complex FFT has 256 points,
//! which factors into radix-4 stages alone.

/// The real FFT's length in samples.
pub(crate) const FFT_SIZE: usize = 512;
/// The inner complex FFT's length.
const N: usize = FFT_SIZE / 2;
/// The radix of every stage (256 is 4 to the 4th).
const RADIX: usize = 4;

/// A complex number with 16-bit parts.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(crate) struct Complex {
    pub(crate) r: i16,
    pub(crate) i: i16,
}

/// `round(a * b / 2^15)`, stored in 16 bits as C stores it.
#[inline]
fn mul_round(a: i32, b: i32) -> i16 {
    ((a * b + (1 << 14)) >> 15) as i16
}

/// KISS FFT's `C_FIXDIV`: divides by `div`, as a multiply by `32767 / div`.
#[inline]
fn fix_div(c: Complex, div: i32) -> Complex {
    Complex {
        r: mul_round(i32::from(c.r), 32767 / div),
        i: mul_round(i32::from(c.i), 32767 / div),
    }
}

#[inline]
fn mul(a: Complex, b: Complex) -> Complex {
    let (ar, ai, br, bi) = (
        i32::from(a.r),
        i32::from(a.i),
        i32::from(b.r),
        i32::from(b.i),
    );
    Complex {
        r: ((ar * br - ai * bi + (1 << 14)) >> 15) as i16,
        i: ((ar * bi + ai * br + (1 << 14)) >> 15) as i16,
    }
}

#[inline]
fn add(a: Complex, b: Complex) -> Complex {
    Complex {
        r: a.r.wrapping_add(b.r),
        i: a.i.wrapping_add(b.i),
    }
}

#[inline]
fn sub(a: Complex, b: Complex) -> Complex {
    Complex {
        r: a.r.wrapping_sub(b.r),
        i: a.i.wrapping_sub(b.i),
    }
}

fn twiddle(phase: f64) -> Complex {
    Complex {
        r: (0.5 + 32767.0 * phase.cos()).floor() as i16,
        i: (0.5 + 32767.0 * phase.sin()).floor() as i16,
    }
}

/// The twiddle tables and the scratch buffer of one transform.
pub(crate) struct RealFft {
    twiddles: Vec<Complex>,
    super_twiddles: Vec<Complex>,
    tmp: Vec<Complex>,
}

impl RealFft {
    pub(crate) fn new() -> Self {
        let twiddles = (0..N)
            .map(|i| twiddle(-2.0 * std::f64::consts::PI * i as f64 / N as f64))
            .collect();
        let super_twiddles = (0..N / 2)
            .map(|i| twiddle(-std::f64::consts::PI * ((i + 1) as f64 / N as f64 + 0.5)))
            .collect();
        Self {
            twiddles,
            super_twiddles,
            tmp: vec![Complex::default(); N],
        }
    }

    /// One radix-4 butterfly stage over `out`, whose four quarters hold the sub-transforms.
    fn butterfly4(twiddles: &[Complex], out: &mut [Complex], fstride: usize) {
        let m = out.len() / RADIX;
        for k in 0..m {
            let a = fix_div(out[k], 4);
            let b = fix_div(out[k + m], 4);
            let c = fix_div(out[k + 2 * m], 4);
            let d = fix_div(out[k + 3 * m], 4);
            let s0 = mul(b, twiddles[k * fstride]);
            let s1 = mul(c, twiddles[2 * k * fstride]);
            let s2 = mul(d, twiddles[3 * k * fstride]);
            let s5 = sub(a, s1);
            let a = add(a, s1);
            let s3 = add(s0, s2);
            let s4 = sub(s0, s2);
            out[k + 2 * m] = sub(a, s3);
            out[k] = add(a, s3);
            out[k + m] = Complex {
                r: s5.r.wrapping_add(s4.i),
                i: s5.i.wrapping_sub(s4.r),
            };
            out[k + 3 * m] = Complex {
                r: s5.r.wrapping_sub(s4.i),
                i: s5.i.wrapping_add(s4.r),
            };
        }
    }

    /// KISS FFT's recursive decimation: `out` receives the transform of every `fstride`-th
    /// input pair starting at pair `start`.
    fn work(
        twiddles: &[Complex],
        out: &mut [Complex],
        time: &[i16; FFT_SIZE],
        start: usize,
        fstride: usize,
    ) {
        let m = out.len() / RADIX;
        if m == 1 {
            for (j, o) in out.iter_mut().enumerate() {
                let at = start + j * fstride;
                *o = Complex {
                    r: time[2 * at],
                    i: time[2 * at + 1],
                };
            }
        } else {
            for (j, part) in out.chunks_exact_mut(m).enumerate() {
                Self::work(twiddles, part, time, start + j * fstride, fstride * RADIX);
            }
        }
        Self::butterfly4(twiddles, out, fstride);
    }

    /// Transforms 512 real samples into bins 0 to 256 (`freq` has 257 entries).
    pub(crate) fn forward(&mut self, time: &[i16; FFT_SIZE], freq: &mut [Complex; N + 1]) {
        // The complex FFT of the samples taken in pairs: evens as real parts, odds as imaginary.
        Self::work(&self.twiddles, &mut self.tmp, time, 0, 1);
        let tmp = &self.tmp;
        let dc = fix_div(tmp[0], 2);
        freq[0] = Complex {
            r: dc.r.wrapping_add(dc.i),
            i: 0,
        };
        freq[N] = Complex {
            r: dc.r.wrapping_sub(dc.i),
            i: 0,
        };
        for k in 1..=N / 2 {
            let fpk = fix_div(tmp[k], 2);
            let fpnk = fix_div(
                Complex {
                    r: tmp[N - k].r,
                    i: tmp[N - k].i.wrapping_neg(),
                },
                2,
            );
            let f1k = add(fpk, fpnk);
            let f2k = sub(fpk, fpnk);
            let tw = mul(f2k, self.super_twiddles[k - 1]);
            // The halvings are done on the 32-bit sums, as C's integer promotion does.
            let (f1r, f1i, twr, twi) = (
                i32::from(f1k.r),
                i32::from(f1k.i),
                i32::from(tw.r),
                i32::from(tw.i),
            );
            freq[k] = Complex {
                r: ((f1r + twr) >> 1) as i16,
                i: ((f1i + twi) >> 1) as i16,
            };
            freq[N - k] = Complex {
                r: ((f1r - twr) >> 1) as i16,
                i: ((twi - f1i) >> 1) as i16,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tone in bin 16 lands in bin 16 and nowhere else, at the scale the fixed-point
    /// transform gives it (each of the four radix-4 stages and the real step divide).
    #[test]
    fn a_tone_lands_in_its_bin() {
        let mut time = [0i16; FFT_SIZE];
        for (n, s) in time.iter_mut().enumerate() {
            *s = (16000.0 * (2.0 * std::f64::consts::PI * 16.0 * n as f64 / FFT_SIZE as f64).cos())
                .round() as i16;
        }
        let mut freq = [Complex::default(); N + 1];
        RealFft::new().forward(&time, &mut freq);
        let power = |c: Complex| i64::from(c.r).pow(2) + i64::from(c.i).pow(2);
        let peak = power(freq[16]);
        // A full-scale cosine of amplitude A gives A/2 in its bin after the 1/512 scaling.
        assert!(
            (7000i64.pow(2)..9000i64.pow(2)).contains(&peak),
            "{:?}",
            freq[16]
        );
        for (k, c) in freq.iter().enumerate() {
            if k != 16 {
                assert!(power(*c) < 64, "bin {k}: {c:?}");
            }
        }
    }
}
