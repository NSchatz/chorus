//! The sample rate a captured output actually produced (audit A-11).
//!
//! The endpoint rig's AC-3 asks for the rate the ESP32-S3's 24-bit I2S
//! configuration produces to be measured rather than read back: a clock
//! configuration agrees with itself whatever the hardware does. This recovers
//! it from the capture the rig already takes.
//!
//! # How
//!
//! The rig's stream is the chirp of `config/measure.conf`, which repeats every
//! `chirp_period_us` on the server timeline. A device that plays it `e` parts
//! per million fast repeats it every `period / (1 + e)` on the capture clock.
//! Every `rate_hop_us` this finds where the next sweep starts (a one-period
//! reference correlated against the capture, the peak refined by a parabola
//! through its neighbours), unwraps the starts into a count of whole periods,
//! and fits the residual against elapsed time with the free-run fit
//! ([`crate::freerun::fit`]), which already refuses a series too short or too
//! noisy by name. The residual slope is exactly `-e`.
//!
//! # What the figure is relative to
//!
//! The capture interface's own clock. A figure of +20 ppm says the output ran
//! 20 ppm fast against the interface, and says nothing on its own about which
//! of the two crystals is off; the report says so. A servo that inserts or
//! drops frames shows up here too, because it changes the rate at the pins,
//! which is the rate a listener hears.
//!
//! Nothing here writes a baseline or a report: `chorus-measure rate` prints
//! its figures and the bench library hashes them (audit A-5 was a baseline
//! written by the wrong command).

use std::fmt;

use crate::chirp::ChirpSpec;
use crate::config::MeasureConfig;
use crate::freerun::{self, Observation, OffsetSeries, SlopeError, SlopeFit, SlopeSettings};
use crate::wav::Capture;

/// Which captured channel a figure is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    /// The first captured output (the rig wires the ESP32-S3 endpoint here).
    A,
    /// The second captured output.
    B,
}

impl Channel {
    /// The word a report and the command line use.
    pub fn name(self) -> &'static str {
        match self {
            Channel::A => "a",
            Channel::B => "b",
        }
    }
}

/// Everything the estimate reads, all from `config/measure.conf`.
#[derive(Debug, Clone, PartialEq)]
pub struct RateSettings {
    /// The chirp the stream carries.
    pub chirp: ChirpSpec,
    /// How far apart the sweep starts are sampled.
    pub hop_us: f64,
    /// The correlation a sweep start needs to count.
    pub confidence_floor: f64,
    /// The fit's refusal thresholds.
    pub slope: SlopeSettings,
}

impl RateSettings {
    /// The settings `config/measure.conf` declares.
    pub fn from_config(config: &MeasureConfig) -> Result<RateSettings, String> {
        let chirp = ChirpSpec::new(
            config.chirp_start_hz,
            config.chirp_end_hz,
            config.chirp_period_us,
            config.chirp_amplitude_ceiling,
            config.chirp_amplitude_ceiling,
            "config/measure.conf",
        )
        .map_err(|e| e.to_string())?;
        Ok(RateSettings {
            chirp,
            hop_us: config.rate_hop_us,
            confidence_floor: config.confidence_floor,
            slope: SlopeSettings {
                min_points: config.min_resolved_windows,
                min_span_s: config.rate_min_span_s,
                max_half_width_ppm: config.rate_max_half_width_ppm,
            },
        })
    }
}

/// A produced rate, against the capture clock.
#[derive(Debug, Clone, PartialEq)]
pub struct RateEstimate {
    /// Which channel.
    pub channel: Channel,
    /// Parts per million fast (positive) or slow (negative) against the
    /// capture clock.
    pub ppm: f64,
    /// The 95% half-width of that figure.
    pub half_width_ppm: f64,
    /// Sweep starts that cleared the confidence floor and entered the fit.
    pub windows_used: usize,
    /// Sweep starts sampled.
    pub windows_total: usize,
    /// The span the fit covers, in seconds.
    pub span_s: f64,
}

/// Why no rate is published.
#[derive(Debug, Clone, PartialEq)]
pub enum RateError {
    /// The capture holds less than two periods.
    CaptureTooShort {
        /// Frames in the capture.
        frames: usize,
        /// Frames one sweep start needs.
        required_frames: usize,
    },
    /// The fit refused the series of sweep starts.
    Fit {
        /// Which channel.
        channel: Channel,
        /// Sweep starts that cleared the confidence floor.
        windows_used: usize,
        /// Sweep starts sampled.
        windows_total: usize,
        /// The fit's own refusal.
        error: SlopeError,
    },
}

impl RateError {
    /// A short, stable token naming which refusal this is.
    pub fn condition(&self) -> &'static str {
        match self {
            RateError::CaptureTooShort { .. } => "capture-too-short",
            RateError::Fit { error, .. } => error.condition(),
        }
    }
}

impl fmt::Display for RateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RateError::CaptureTooShort {
                frames,
                required_frames,
            } => write!(
                f,
                "the capture holds {} frames and one sweep start needs {}; no rate is published",
                frames, required_frames
            ),
            RateError::Fit {
                channel,
                windows_used,
                windows_total,
                error,
            } => write!(
                f,
                "channel {}: {} of {} sweep starts cleared the confidence floor, and the fit \
                 over them refused: {}",
                channel.name(),
                windows_used,
                windows_total,
                error
            ),
        }
    }
}

impl std::error::Error for RateError {}

/// Estimate the rate one channel produced.
pub fn estimate(
    capture: &Capture,
    channel: Channel,
    settings: &RateSettings,
) -> Result<RateEstimate, RateError> {
    let fs = f64::from(capture.sample_rate_hz);
    let x = match channel {
        Channel::A => &capture.a,
        Channel::B => &capture.b,
    };
    let period = settings.chirp.period_s() * fs;
    let p = period.round() as usize;
    let required = 2 * p + 2;
    if p < 4 || x.len() < required {
        return Err(RateError::CaptureTooShort {
            frames: x.len(),
            required_frames: required,
        });
    }

    let reference: Vec<f64> = (0..p).map(|k| settings.chirp.at(k as f64 / fs)).collect();
    let ref_energy: f64 = reference.iter().map(|v| v * v).sum();
    let hop = ((settings.hop_us * fs / 1_000_000.0).round() as usize).max(1);

    // Sweep starts, in capture frames, with the whole number of periods since
    // the first one counted by unwrapping consecutive starts.
    let mut starts: Vec<(f64, i64)> = Vec::new();
    let mut total = 0usize;
    let mut s = 0usize;
    while s + 2 * p + 1 < x.len() {
        total += 1;
        if let Some(tau) = sweep_start(&x[s..s + 2 * p + 1], &reference, ref_energy, p, settings) {
            let at = s as f64 + tau;
            let count = match starts.last() {
                None => 0,
                Some(&(previous, n)) => n + ((at - previous) / period).round() as i64,
            };
            starts.push((at, count));
        }
        s += hop;
    }

    let used = starts.len();
    let first = starts.first().map(|&(at, _)| at).unwrap_or(0.0);
    let observations: Vec<Observation> = starts
        .iter()
        .map(|&(at, n)| Observation {
            t_ns: ((at - first) / fs * 1e9).round() as i64,
            offset_ns: ((at - first - n as f64 * period) / fs * 1e9).round() as i64,
        })
        .collect();
    let series = OffsetSeries {
        path: capture.path.clone(),
        label: format!("sweep starts, channel {}", channel.name()),
        declared_rate_ppm: None,
        declared_jitter_us: None,
        observations,
    };
    let fit: SlopeFit =
        freerun::fit(&series, &settings.slope).map_err(|error| RateError::Fit {
            channel,
            windows_used: used,
            windows_total: total,
            error,
        })?;
    Ok(RateEstimate {
        channel,
        // A sweep that arrives early by a growing amount is a device running
        // fast: the residual slope is the negative of the produced rate.
        ppm: -fit.ppm,
        half_width_ppm: fit.half_width_ppm,
        windows_used: used,
        windows_total: total,
        span_s: fit.span_s,
    })
}

/// Where the next sweep starts within `segment` (which holds two periods and
/// one frame), refined to a fraction of a frame, or `None` when the best
/// normalised correlation does not clear the floor.
fn sweep_start(
    segment: &[f64],
    reference: &[f64],
    ref_energy: f64,
    p: usize,
    settings: &RateSettings,
) -> Option<f64> {
    // Correlate at lags 0..=p, so a start at lag 0 and one a period later are
    // both seen and the parabola always has a neighbour on each side.
    let score = |tau: usize| -> f64 {
        let window = &segment[tau..tau + p];
        let dot: f64 = window.iter().zip(reference).map(|(a, b)| a * b).sum();
        let energy: f64 = window.iter().map(|v| v * v).sum();
        if energy <= 0.0 || ref_energy <= 0.0 {
            0.0
        } else {
            dot / (energy * ref_energy).sqrt()
        }
    };
    let scores: Vec<f64> = (0..=p).map(score).collect();
    let (best, &peak) = scores
        .iter()
        .enumerate()
        .take(p)
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))?;
    if !(peak >= settings.confidence_floor) {
        return None;
    }
    let left = if best == 0 { scores[p] } else { scores[best - 1] };
    let right = scores[best + 1];
    let denominator = left - 2.0 * peak + right;
    let delta = if denominator.abs() > f64::EPSILON {
        (0.5 * (left - right) / denominator).clamp(-0.5, 0.5)
    } else {
        0.0
    };
    Some(best as f64 + delta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn settings(fs_hop_us: f64) -> RateSettings {
        RateSettings {
            chirp: ChirpSpec::new(1000.0, 8000.0, 20_000.0, 0.25, 1.0, "test").unwrap(),
            hop_us: fs_hop_us,
            confidence_floor: 0.6,
            slope: SlopeSettings {
                min_points: 4,
                min_span_s: 0.5,
                max_half_width_ppm: 5.0,
            },
        }
    }

    fn capture(fs: u32, seconds: f64, ppm_a: f64) -> Capture {
        let chirp = ChirpSpec::new(1000.0, 8000.0, 20_000.0, 0.25, 1.0, "test").unwrap();
        let n = (seconds * f64::from(fs)) as usize;
        let a = (0..n)
            .map(|k| chirp.at(k as f64 / f64::from(fs) * (1.0 + ppm_a * 1e-6) + 0.0037))
            .collect();
        let b = (0..n).map(|k| chirp.at(k as f64 / f64::from(fs))).collect();
        Capture {
            path: PathBuf::from("mem.wav"),
            sample_rate_hz: fs,
            a,
            b,
        }
    }

    #[test]
    fn a_fast_output_reads_fast_and_the_reference_reads_zero() {
        let c = capture(48_000, 2.0, 250.0);
        let s = settings(100_000.0);
        let a = estimate(&c, Channel::A, &s).unwrap();
        let b = estimate(&c, Channel::B, &s).unwrap();
        assert!((a.ppm - 250.0).abs() < 1.0, "{:?}", a);
        assert!(b.ppm.abs() < 1.0, "{:?}", b);
    }

    #[test]
    fn a_slow_output_reads_slow() {
        let c = capture(48_000, 2.0, -80.0);
        let a = estimate(&c, Channel::A, &settings(100_000.0)).unwrap();
        assert!((a.ppm + 80.0).abs() < 1.0, "{:?}", a);
    }

    #[test]
    fn silence_is_refused_rather_than_read_as_a_rate() {
        let mut c = capture(48_000, 1.0, 0.0);
        c.a.iter_mut().for_each(|v| *v = 0.0);
        let err = estimate(&c, Channel::A, &settings(100_000.0)).unwrap_err();
        assert_eq!(err.condition(), "series-too-short", "{}", err);
    }

    #[test]
    fn a_capture_shorter_than_two_periods_is_refused() {
        let c = capture(48_000, 0.03, 0.0);
        let err = estimate(&c, Channel::A, &settings(100_000.0)).unwrap_err();
        assert_eq!(err.condition(), "capture-too-short", "{}", err);
    }
}
