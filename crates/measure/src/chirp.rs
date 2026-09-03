//! The chirp: what it is, and the ceiling that stands in front of it.
//!
//! # Why the amplitude has a refusal in front of it
//!
//! Everything else in this rig is reversible. Fixtures, reports and decision
//! records are files, and a revert undoes them completely. The chirp is not:
//! it leaves an endpoint through a real amplifier into a real loudspeaker, and
//! an over-level chirp - a broadband sweep at full scale, sustained - damages a
//! driver rather than failing a test. So the amplitude a run may request has a
//! ceiling, the ceiling is declared in `config/measure.conf` rather than
//! defaulted in code, and a run that asks for more refuses to start naming both
//! numbers. Nothing is emitted first and checked afterwards.
//!
//! # Why the waveform is defined as a function of continuous time
//!
//! [`ChirpSpec::at`] takes seconds, not a sample index. That is what lets a
//! fixture carry a delay which is deliberately NOT a whole number of capture
//! samples: the delayed channel is the same analytic waveform evaluated at
//! `t - d`, so the ground truth is exact by construction rather than the
//! product of an interpolator whose error would then be indistinguishable from
//! the estimator's. A whole-sample estimator fails those fixtures red, which is
//! the point of them.

use std::f64::consts::PI;
use std::fmt;

/// Why a chirp could not be built.
#[derive(Debug, Clone, PartialEq)]
pub enum ChirpError {
    /// The requested amplitude is above the declared ceiling.
    AmplitudeAboveCeiling {
        /// The amplitude the run asked for, in full-scale units.
        requested: f64,
        /// The amplitude the configuration permits.
        permitted: f64,
        /// Where that ceiling is declared.
        declared_in: String,
    },
    /// The requested amplitude is not a usable level.
    AmplitudeNotPositive {
        /// The amplitude the run asked for.
        requested: f64,
    },
    /// The sweep does not sweep.
    BandNotAscending {
        /// The low edge as given.
        start_hz: f64,
        /// The high edge as given.
        end_hz: f64,
    },
    /// The sweep has no duration.
    PeriodNotPositive {
        /// The period as given, in microseconds.
        period_us: f64,
    },
}

impl ChirpError {
    /// A short, stable token naming which refusal this is.
    pub fn condition(&self) -> &'static str {
        match self {
            ChirpError::AmplitudeAboveCeiling { .. } => "amplitude-above-ceiling",
            ChirpError::AmplitudeNotPositive { .. } => "amplitude-not-positive",
            ChirpError::BandNotAscending { .. } => "band-not-ascending",
            ChirpError::PeriodNotPositive { .. } => "period-not-positive",
        }
    }
}

impl fmt::Display for ChirpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChirpError::AmplitudeAboveCeiling {
                requested,
                permitted,
                declared_in,
            } => write!(
                f,
                "this run requested a chirp amplitude of {} full scale and {} permits at most \
                 {}; the chirp drives a real amplifier and no audio has been emitted",
                requested, declared_in, permitted
            ),
            ChirpError::AmplitudeNotPositive { requested } => write!(
                f,
                "a chirp amplitude of {} full scale would emit nothing to measure",
                requested
            ),
            ChirpError::BandNotAscending { start_hz, end_hz } => write!(
                f,
                "a sweep from {} Hz to {} Hz does not sweep upward, which the correlation \
                 peak's width depends on",
                start_hz, end_hz
            ),
            ChirpError::PeriodNotPositive { period_us } => {
                write!(f, "a sweep period of {} us has no duration", period_us)
            }
        }
    }
}

impl std::error::Error for ChirpError {}

/// A repeating linear frequency sweep.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChirpSpec {
    start_hz: f64,
    end_hz: f64,
    period_s: f64,
    amplitude: f64,
}

/// The fraction of each sweep spent rising into, and falling out of, full
/// level.
///
/// Without it the repeat boundary is a step, which is broadband energy the band
/// limit does not cover and a click a loudspeaker does not enjoy.
const TAPER_FRACTION: f64 = 0.1;

impl ChirpSpec {
    /// Build a chirp, refusing anything the declared ceiling does not permit.
    ///
    /// The ceiling is passed in rather than read here so that this stays a pure
    /// function of its arguments; `config/measure.conf` is where the number
    /// comes from and [`crate::config::MeasureConfig`] is what reads it.
    pub fn new(
        start_hz: f64,
        end_hz: f64,
        period_us: f64,
        amplitude: f64,
        ceiling: f64,
        declared_in: &str,
    ) -> Result<ChirpSpec, ChirpError> {
        if !(amplitude > 0.0) {
            return Err(ChirpError::AmplitudeNotPositive {
                requested: amplitude,
            });
        }
        if !(amplitude <= ceiling) {
            return Err(ChirpError::AmplitudeAboveCeiling {
                requested: amplitude,
                permitted: ceiling,
                declared_in: declared_in.to_string(),
            });
        }
        if !(end_hz > start_hz) || !(start_hz > 0.0) {
            return Err(ChirpError::BandNotAscending { start_hz, end_hz });
        }
        if !(period_us > 0.0) {
            return Err(ChirpError::PeriodNotPositive { period_us });
        }
        Ok(ChirpSpec {
            start_hz,
            end_hz,
            period_s: period_us / 1_000_000.0,
            amplitude,
        })
    }

    /// The amplitude this chirp was built at, in full-scale units.
    pub fn amplitude(&self) -> f64 {
        self.amplitude
    }

    /// The band it sweeps, in Hz.
    pub fn band_hz(&self) -> (f64, f64) {
        (self.start_hz, self.end_hz)
    }

    /// How long one sweep lasts, in seconds.
    pub fn period_s(&self) -> f64 {
        self.period_s
    }

    /// The waveform at `t` seconds, for any real `t`, including a negative one.
    ///
    /// Continuous time is the whole point: see the module documentation.
    pub fn at(&self, t: f64) -> f64 {
        let u = t.rem_euclid(self.period_s);
        let x = u / self.period_s;
        let sweep = (self.end_hz - self.start_hz) / self.period_s;
        let phase = 2.0 * PI * (self.start_hz * u + 0.5 * sweep * u * u);
        self.amplitude * taper(x) * phase.sin()
    }
}

/// A raised-cosine rise and fall, so the repeat boundary is not a step.
fn taper(x: f64) -> f64 {
    if x < TAPER_FRACTION {
        0.5 * (1.0 - (PI * x / TAPER_FRACTION).cos())
    } else if x > 1.0 - TAPER_FRACTION {
        0.5 * (1.0 - (PI * (1.0 - x) / TAPER_FRACTION).cos())
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_chirp() -> ChirpSpec {
        ChirpSpec::new(1000.0, 8000.0, 20_000.0, 0.25, 0.25, "config/measure.conf").unwrap()
    }

    #[test]
    fn an_amplitude_above_the_ceiling_is_refused_naming_both_numbers() {
        let err =
            ChirpSpec::new(1000.0, 8000.0, 20_000.0, 0.9, 0.25, "config/measure.conf").unwrap_err();
        assert_eq!(err.condition(), "amplitude-above-ceiling");
        let said = err.to_string();
        assert!(said.contains("0.9"), "{}", said);
        assert!(said.contains("0.25"), "{}", said);
        assert!(said.contains("no audio has been emitted"), "{}", said);
    }

    #[test]
    fn the_ceiling_itself_is_permitted() {
        assert!(ChirpSpec::new(1000.0, 8000.0, 20_000.0, 0.25, 0.25, "c").is_ok());
    }

    #[test]
    fn a_hair_above_the_ceiling_is_not() {
        let err = ChirpSpec::new(1000.0, 8000.0, 20_000.0, 0.250_000_1, 0.25, "c").unwrap_err();
        assert_eq!(err.condition(), "amplitude-above-ceiling");
    }

    #[test]
    fn a_nan_amplitude_is_refused_rather_than_compared_away() {
        // Every comparison against NaN is false, so a ceiling check written as
        // `if amplitude > ceiling { refuse }` would let this through.
        let err = ChirpSpec::new(1000.0, 8000.0, 20_000.0, f64::NAN, 0.25, "c").unwrap_err();
        assert_eq!(err.condition(), "amplitude-not-positive");
    }

    #[test]
    fn the_waveform_never_exceeds_its_amplitude() {
        let chirp = a_chirp();
        for n in 0..200_000 {
            let t = n as f64 / 96_000.0;
            assert!(chirp.at(t).abs() <= 0.25 + 1e-12);
        }
    }

    #[test]
    fn the_waveform_repeats_at_its_period() {
        let chirp = a_chirp();
        for n in 0..1000 {
            let t = n as f64 * 1e-5;
            let a = chirp.at(t);
            let b = chirp.at(t + chirp.period_s());
            assert!((a - b).abs() < 1e-12, "{} != {} at t={}", a, b, t);
        }
    }

    #[test]
    fn a_negative_time_is_the_same_waveform_run_backwards_into_the_previous_sweep() {
        let chirp = a_chirp();
        let a = chirp.at(-0.000_3);
        let b = chirp.at(chirp.period_s() - 0.000_3);
        assert!((a - b).abs() < 1e-12, "{} != {}", a, b);
    }

    #[test]
    fn the_taper_takes_the_repeat_boundary_to_zero() {
        let chirp = a_chirp();
        assert!(chirp.at(0.0).abs() < 1e-12);
        assert!(chirp.at(chirp.period_s() - 1e-9).abs() < 1e-6);
    }
}
