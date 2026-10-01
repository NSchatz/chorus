//! The night-mode compressor: feed-forward, the level detected in the log
//! domain after the gain computer, a soft knee, attack and release smoothing
//! and a makeup gain, linked across channels (one gain for all, so the image
//! does not move).
//!
//! The gain computer is Giannoulis, Massberg and Reiss, "Digital Dynamic Range
//! Compressor Design: A Tutorial and Analysis", JAES 60(6), 2012
//! (<https://secure.aes.org/forum/pubs/journal/?ID=174>), whose recommendation
//! is the feed-forward design "with the detector placed in the log domain
//! after the gain computer". The paper's PDF could not be fetched on
//! 2026-10-01; its static characteristic, as MathWorks' compressor (which
//! cites it) prints it
//! (<https://www.mathworks.com/help/audio/ref/compressor-system-object.html>,
//! read 2026-10-01), for threshold `T`, ratio `R` and knee width `W`, all dB:
//!
//! ```text
//! y = x                                     2(x - T) < -W
//! y = x + (1/R - 1) (x - T + W/2)^2 / (2W)  2|x - T| <= W
//! y = T + (x - T)/R                         2(x - T) >  W
//! ```
//!
//! The smoothing is the same page's: the computed gain `gc = y - x` (dB, never
//! positive) is followed by
//! `gs[n] = aA gs[n-1] + (1 - aA) gc[n]` when `gc[n] <= gs[n-1]` (attack: the
//! gain is falling) and with `aR` otherwise, `a = exp(-ln 9 / (Fs T))`, which
//! makes `T` the 10 % to 90 % time of a step (a fixture checks that). The
//! level detector is the frame's peak over the channels.

use crate::DspError;

/// The night mode's settings. ASSUMED, every one (chorus's own choice, not
/// measured): threshold -24 dBFS, ratio 3:1, a 12 dB knee, 10 ms attack,
/// 500 ms release (the ASSUMED pair of docs/research/research-dsp-phase-b.md; the Dolby Metadata
/// Guide's profiles give no times) and +6 dB makeup, so full scale comes down
/// 10 dB and material below the knee comes up 6 dB.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompressorParams {
    pub threshold_db: f64,
    pub ratio: f64,
    pub knee_db: f64,
    pub attack_ms: f64,
    pub release_ms: f64,
    pub makeup_db: f64,
}

/// See [`CompressorParams`].
pub const NIGHT: CompressorParams = CompressorParams {
    threshold_db: -24.0,
    ratio: 3.0,
    knee_db: 12.0,
    attack_ms: 10.0,
    release_ms: 500.0,
    makeup_db: 6.0,
};

/// The static characteristic `y(x)` in dB (see the module), in `f64`.
pub fn static_curve_db(x_db: f64, threshold_db: f64, ratio: f64, knee_db: f64) -> f64 {
    let over = x_db - threshold_db;
    if 2.0 * over < -knee_db {
        x_db
    } else if knee_db > 0.0 && 2.0 * over.abs() <= knee_db {
        let t = over + knee_db / 2.0;
        x_db + (1.0 / ratio - 1.0) * t * t / (2.0 * knee_db)
    } else {
        threshold_db + over / ratio
    }
}

/// The same characteristic in `f32`, as the running compressor evaluates it
/// per frame (the ESP32-S3's FPU is single precision).
fn static_curve_db_f32(x_db: f32, threshold_db: f32, ratio: f32, knee_db: f32) -> f32 {
    let over = x_db - threshold_db;
    if 2.0 * over < -knee_db {
        x_db
    } else if knee_db > 0.0 && 2.0 * over.abs() <= knee_db {
        let t = over + knee_db / 2.0;
        x_db + (1.0 / ratio - 1.0) * t * t / (2.0 * knee_db)
    } else {
        threshold_db + over / ratio
    }
}

/// The smoothing coefficient `exp(-ln 9 / (Fs T))` for a 10-90 % time of
/// `time_ms`.
pub fn ballistics_coefficient(time_ms: f64, rate_hz: f64) -> Result<f32, DspError> {
    if !(time_ms.is_finite() && time_ms > 0.0 && rate_hz > 0.0) {
        return Err(DspError::TimeConstant);
    }
    Ok((-(9f64.ln()) / (rate_hz * time_ms / 1000.0)).exp() as f32)
}

/// `ln(10) / 20` in `f32`: dB to a natural exponent.
pub const LN10_OVER_20: f32 = 0.115_129_25;

/// The level detector's floor, so silence is a finite level.
pub const FLOOR: f32 = 1e-10;

/// A running, channel-linked compressor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Compressor {
    threshold_db: f32,
    ratio: f32,
    knee_db: f32,
    makeup_db: f32,
    attack: f32,
    release: f32,
    smoothed_db: f32,
}

impl Compressor {
    /// A compressor at rest (no gain reduction yet).
    pub fn new(p: &CompressorParams, rate_hz: f64) -> Result<Compressor, DspError> {
        if !(p.ratio.is_finite() && p.ratio >= 1.0) {
            return Err(DspError::Gain);
        }
        if !(p.threshold_db.is_finite()
            && p.knee_db.is_finite()
            && p.knee_db >= 0.0
            && p.makeup_db.is_finite())
        {
            return Err(DspError::Gain);
        }
        Ok(Compressor {
            threshold_db: p.threshold_db as f32,
            ratio: p.ratio as f32,
            knee_db: p.knee_db as f32,
            makeup_db: p.makeup_db as f32,
            attack: ballistics_coefficient(p.attack_ms, rate_hz)?,
            release: ballistics_coefficient(p.release_ms, rate_hz)?,
            smoothed_db: 0.0,
        })
    }

    /// Clears the smoothing state.
    pub fn reset(&mut self) {
        self.smoothed_db = 0.0;
    }

    /// The smoothed gain reduction, dB (0 or negative), before makeup.
    pub fn smoothed_db(&self) -> f32 {
        self.smoothed_db
    }

    /// One step of the attack/release smoothing towards a computed gain
    /// `gc_db`; returns the smoothed gain.
    #[inline]
    pub fn smooth(&mut self, gc_db: f32) -> f32 {
        let a = if gc_db <= self.smoothed_db {
            self.attack
        } else {
            self.release
        };
        self.smoothed_db = a * self.smoothed_db + (1.0 - a) * gc_db;
        self.smoothed_db
    }

    /// One frame through the compressor, in place.
    pub fn process_frame(&mut self, frame: &mut [f32]) {
        let mut peak = FLOOR;
        for &x in frame.iter() {
            let a = x.abs();
            if a > peak {
                peak = a;
            }
        }
        let x_db = 20.0 * peak.log10();
        let y_db = static_curve_db_f32(x_db, self.threshold_db, self.ratio, self.knee_db);
        let gs = self.smooth(y_db - x_db);
        let g = ((gs + self.makeup_db) * LN10_OVER_20).exp();
        for x in frame.iter_mut() {
            *x *= g;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_static_curve_is_continuous_at_both_knee_edges() {
        let (t, r, w) = (-20.0, 4.0, 10.0);
        for edge in [t - w / 2.0, t + w / 2.0] {
            let below = static_curve_db(edge - 1e-9, t, r, w);
            let above = static_curve_db(edge + 1e-9, t, r, w);
            assert!((below - above).abs() < 1e-6, "{edge}");
        }
        // hard knee
        assert_eq!(static_curve_db(-30.0, -20.0, 4.0, 0.0), -30.0);
        assert_eq!(static_curve_db(0.0, -20.0, 4.0, 0.0), -15.0);
    }

    #[test]
    fn the_attack_time_is_the_ten_to_ninety_percent_time() {
        let mut c = Compressor::new(
            &CompressorParams {
                attack_ms: 10.0,
                ..NIGHT
            },
            48000.0,
        )
        .unwrap();
        let (mut t10, mut t90) = (None, None);
        for n in 0..48000 {
            let g = c.smooth(-10.0);
            if t10.is_none() && g <= -1.0 {
                t10 = Some(n);
            }
            if t90.is_none() && g <= -9.0 {
                t90 = Some(n);
                break;
            }
        }
        let rise = t90.unwrap() - t10.unwrap();
        assert!((rise as i64 - 480).abs() <= 1, "{rise}");
    }
}
