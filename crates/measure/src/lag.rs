//! The lag estimator: sliding-window cross-correlation, resolved finer than
//! one capture sample, reduced to a distribution.
//!
//! # The sign convention, declared once and reported with every figure
//!
//! Channel A is the first captured output and channel B the second. The
//! estimator finds the `d` for which `b(t) ~= a(t - d)`. So:
//!
//! > **A POSITIVE lag means the endpoint on channel A leads the endpoint on
//! > channel B by that many microseconds.**
//!
//! A capture with the two channels exchanged therefore reports the same
//! magnitude with the opposite sign, which is asserted rather than asserted
//! about.
//!
//! # How ten microseconds is reached at all
//!
//! One sample at 96 kHz is 10.4 us, so a whole-sample estimator cannot hold
//! "10 us or better": its own quantisation is already coarser than the budget.
//! Two steps get past that.
//!
//! 1. The integer-lag search finds the correlation peak to the nearest sample.
//! 2. The peak is then located on a continuum. The signals are band limited to
//!    the chirp's band, so their cross-correlation is band limited to the same
//!    band and the discrete correlation sequence is a SAMPLED version of a
//!    continuous function. A windowed-sinc (Whittaker-Shannon) reconstruction
//!    of that function is evaluated on a fine grid across the sample either
//!    side of the peak, and a parabola through the best three fine points gives
//!    the vertex.
//!
//! Reconstruction rather than a parabola straight onto the three integer
//! samples is deliberate, and it is the risk this module exists to avoid: a
//! parabola fitted to a correlation peak returns a confident number biased by
//! the shape of the chirp rather than by the delay. The committed fixtures
//! carry delays that are deliberately not a whole number of samples so that a
//! whole-sample estimator, or a badly biased one, fails red.
//!
//! # What is refused rather than reported
//!
//! `documentation/roadmaps/chorus.md` states this phase's fail-safe: "a run
//! that cannot resolve the two outputs says so and writes no number." Four
//! conditions end a run here, each named, and none of them produces a figure:
//! a silent channel, no chirp in the declared band, a correlation peak that
//! never clears the declared confidence floor, and a peak sitting on the edge
//! of the declared search range, where the true peak may be outside it.

use std::f64::consts::PI;
use std::fmt;

use crate::config::MeasureConfig;
use crate::wav::Capture;

/// The convention every figure this module reports is stated under.
pub const SIGN_CONVENTION: &str =
    "a positive lag means the endpoint on channel A (the first captured output) leads the \
     endpoint on channel B (the second) by that many microseconds";

/// How many taps either side the sinc reconstruction uses.
const SINC_TAPS: usize = 16;

/// How finely the reconstruction is evaluated across the sample either side of
/// the integer peak. One step is 1/256 of a sample, which at 96 kHz is 0.041
/// us, and the parabolic vertex taken afterwards is finer still.
const FINE_STEPS: usize = 256;

/// The settings a run declares before it looks at a capture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LagSettings {
    /// Analysis window, in frames.
    pub window_frames: usize,
    /// Distance between the starts of consecutive windows, in frames.
    pub hop_frames: usize,
    /// Widest lag searched, in frames, either way.
    pub max_lag_frames: usize,
    /// The normalised correlation a window's peak has to reach.
    pub confidence_floor: f64,
    /// A channel quieter than this, in dBFS, is silent.
    pub silence_floor_dbfs: f64,
    /// The band the chirp sweeps.
    pub chirp_band_hz: (f64, f64),
    /// The share of a channel's energy that has to lie in that band.
    pub chirp_band_fraction_floor: f64,
    /// The fewest windows that have to resolve.
    pub min_resolved_windows: usize,
}

impl LagSettings {
    /// The settings a capture at `sample_rate_hz` gets from the committed
    /// configuration.
    pub fn from_config(config: &MeasureConfig, sample_rate_hz: u32) -> LagSettings {
        let per_us = f64::from(sample_rate_hz) / 1_000_000.0;
        LagSettings {
            window_frames: (config.window_us * per_us).round() as usize,
            hop_frames: (config.hop_us * per_us).round().max(1.0) as usize,
            max_lag_frames: (config.max_lag_us * per_us).ceil() as usize,
            confidence_floor: config.confidence_floor,
            silence_floor_dbfs: config.silence_floor_dbfs,
            chirp_band_hz: (config.chirp_start_hz, config.chirp_end_hz),
            chirp_band_fraction_floor: config.chirp_band_fraction_floor,
            min_resolved_windows: config.min_resolved_windows,
        }
    }
}

/// One window's answer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowLag {
    /// Which window, counting from zero.
    pub index: usize,
    /// The frame the window's reference channel starts at.
    pub start_frame: usize,
    /// The lag, under [`SIGN_CONVENTION`].
    pub lag_us: f64,
    /// The normalised correlation at the peak.
    pub coefficient: f64,
}

/// What a run reports when it resolves.
#[derive(Debug, Clone, PartialEq)]
pub struct LagSummary {
    /// Windows the capture offered.
    pub windows_total: usize,
    /// Windows whose peak cleared the confidence floor and sat inside the
    /// search range.
    pub windows_used: usize,
    /// The median lag, carrying the sign.
    pub median_us: f64,
    /// The 95th percentile of the ABSOLUTE lag.
    pub p95_abs_us: f64,
    /// The largest absolute lag.
    pub max_abs_us: f64,
    /// The smallest lag any used window reported, signed, for a reader who
    /// wants the spread rather than the summary.
    pub min_us: f64,
    /// The largest lag any used window reported, signed.
    pub max_us: f64,
    /// The weakest correlation among the windows that were used.
    pub min_coefficient: f64,
    /// Every window that was used.
    pub windows: Vec<WindowLag>,
}

impl LagSummary {
    /// Which endpoint leads, in words, under [`SIGN_CONVENTION`].
    pub fn who_leads(&self) -> &'static str {
        if self.median_us > 0.0 {
            "channel A leads channel B"
        } else if self.median_us < 0.0 {
            "channel B leads channel A"
        } else {
            "neither channel leads; the median lag is exactly zero"
        }
    }
}

/// Why a run could not resolve the two outputs.
#[derive(Debug, Clone, PartialEq)]
pub enum LagError {
    /// A channel carries no signal.
    SilentChannel {
        /// Which channel, `"A"` or `"B"`.
        channel: &'static str,
        /// What it measured, in dBFS.
        rms_dbfs: f64,
        /// The floor the run declared.
        floor_dbfs: f64,
    },
    /// Neither channel carries the chirp the run was looking for.
    NoChirpPresent {
        /// Which channel this fraction is for.
        channel: &'static str,
        /// The share of that channel's energy inside the declared band.
        in_band_fraction: f64,
        /// The floor the run declared.
        floor: f64,
        /// The band the run declared, in Hz.
        band_hz: (f64, f64),
    },
    /// The correlation peak never rose above the declared confidence floor.
    BelowConfidenceFloor {
        /// The best normalised correlation any window reached.
        best_coefficient: f64,
        /// The floor the run declared.
        floor: f64,
        /// Windows the capture offered.
        windows_total: usize,
    },
    /// Too few windows resolved to report a distribution.
    TooFewWindowsResolved {
        /// Windows that did resolve.
        resolved: usize,
        /// Windows the run required.
        required: usize,
        /// Windows the capture offered.
        windows_total: usize,
    },
    /// The peak sat on the edge of the declared search range, so the true peak
    /// may be outside it.
    PeakAtSearchEdge {
        /// The edge lag, in microseconds.
        edge_us: f64,
        /// How many windows put their peak there.
        windows: usize,
    },
    /// The capture is not long enough to hold one window plus the search range
    /// either side of it.
    CaptureTooShort {
        /// Frames the capture holds.
        frames: usize,
        /// Frames one window plus both search margins needs.
        required_frames: usize,
    },
}

impl LagError {
    /// A short, stable token naming which refusal this is, so a script can
    /// grade a run without parsing prose.
    pub fn condition(&self) -> &'static str {
        match self {
            LagError::SilentChannel { .. } => "silent-channel",
            LagError::NoChirpPresent { .. } => "no-chirp-present",
            LagError::BelowConfidenceFloor { .. } => "below-confidence-floor",
            LagError::TooFewWindowsResolved { .. } => "too-few-windows-resolved",
            LagError::PeakAtSearchEdge { .. } => "peak-at-search-edge",
            LagError::CaptureTooShort { .. } => "capture-too-short",
        }
    }
}

impl fmt::Display for LagError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LagError::SilentChannel {
                channel,
                rms_dbfs,
                floor_dbfs,
            } => write!(
                f,
                "channel {} is silent: it measures {:.1} dBFS and this run declared anything \
                 below {:.1} dBFS to be silence, so there is nothing to correlate",
                channel, rms_dbfs, floor_dbfs
            ),
            LagError::NoChirpPresent {
                channel,
                in_band_fraction,
                floor,
                band_hz,
            } => write!(
                f,
                "no chirp is present: channel {} puts {:.3} of its energy in the {:.0} to \
                 {:.0} Hz band this run declared, and at least {:.3} was required",
                channel, in_band_fraction, band_hz.0, band_hz.1, floor
            ),
            LagError::BelowConfidenceFloor {
                best_coefficient,
                floor,
                windows_total,
            } => write!(
                f,
                "the two outputs could not be resolved against each other: the best \
                 correlation peak across {} windows reached {:.3} and this run declared a \
                 confidence floor of {:.3}",
                windows_total, best_coefficient, floor
            ),
            LagError::TooFewWindowsResolved {
                resolved,
                required,
                windows_total,
            } => write!(
                f,
                "only {} of {} windows resolved and this run requires {}; a figure from \
                 fewer is not a distribution",
                resolved, windows_total, required
            ),
            LagError::PeakAtSearchEdge { edge_us, windows } => write!(
                f,
                "{} window(s) put the correlation peak on the edge of the declared search \
                 range of +/-{:.0} us, so the true peak may lie outside it and no lag is \
                 reported",
                windows, edge_us
            ),
            LagError::CaptureTooShort {
                frames,
                required_frames,
            } => write!(
                f,
                "the capture holds {} frames and one analysis window plus the search range \
                 either side of it needs {}",
                frames, required_frames
            ),
        }
    }
}

impl std::error::Error for LagError {}

fn rms(x: &[f64]) -> f64 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|v| v * v).sum::<f64>() / x.len() as f64).sqrt()
}

fn dbfs(rms: f64) -> f64 {
    if rms <= 0.0 {
        f64::NEG_INFINITY
    } else {
        20.0 * rms.log10()
    }
}

/// The share of a signal's energy inside `[lo_hz, hi_hz]`.
///
/// A direct discrete Fourier transform over the bins that band covers, with the
/// twiddles taken from one table of `n` entries so no transcendental function
/// is called inside the loop. No transform library, and none needed: the bins
/// wanted are a small slice of the spectrum, so the direct sum is cheaper than
/// a full transform would be.
pub fn in_band_energy_fraction(x: &[f64], sample_rate_hz: f64, lo_hz: f64, hi_hz: f64) -> f64 {
    let n = x.len();
    if n < 2 {
        return 0.0;
    }
    let mean = x.iter().sum::<f64>() / n as f64;
    let centred: Vec<f64> = x.iter().map(|v| v - mean).collect();
    let total: f64 = centred.iter().map(|v| v * v).sum();
    if total <= 0.0 {
        return 0.0;
    }

    let mut cos_table = Vec::with_capacity(n);
    let mut sin_table = Vec::with_capacity(n);
    for k in 0..n {
        let angle = -2.0 * PI * k as f64 / n as f64;
        cos_table.push(angle.cos());
        sin_table.push(angle.sin());
    }

    let bin_hz = sample_rate_hz / n as f64;
    let lo_bin = (lo_hz / bin_hz).floor().max(1.0) as usize;
    let hi_bin = ((hi_hz / bin_hz).ceil() as usize).min(n / 2 - 1);
    if lo_bin > hi_bin {
        return 0.0;
    }

    let mut in_band = 0.0;
    for bin in lo_bin..=hi_bin {
        let mut re = 0.0;
        let mut im = 0.0;
        let mut index = 0usize;
        for value in &centred {
            re += value * cos_table[index];
            im += value * sin_table[index];
            index += bin;
            if index >= n {
                index -= n;
            }
        }
        // Parseval, with the conjugate bin folded in.
        in_band += 2.0 * (re * re + im * im);
    }
    (in_band / n as f64 / total).min(1.0)
}

fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-12 {
        1.0
    } else {
        (PI * x).sin() / (PI * x)
    }
}

/// The band-limited reconstruction of `r` at the (fractional) index `tau`.
fn reconstruct(r: &[f64], tau: f64) -> f64 {
    let centre = tau.round() as i64;
    let taps = SINC_TAPS as i64;
    let mut acc = 0.0;
    for k in (centre - taps)..=(centre + taps) {
        if k < 0 || k as usize >= r.len() {
            continue;
        }
        let d = tau - k as f64;
        if d.abs() > taps as f64 {
            continue;
        }
        // A Hann window over the tap span, so truncating the sinc does not
        // ring.
        let w = 0.5 * (1.0 + (PI * d / taps as f64).cos());
        acc += r[k as usize] * sinc(d) * w;
    }
    acc
}

/// The vertex of the parabola through three equally spaced points, as an offset
/// in steps from the middle one. Zero when the three do not describe a peak.
fn parabolic_vertex(left: f64, middle: f64, right: f64) -> f64 {
    let denominator = left - 2.0 * middle + right;
    if denominator.abs() < f64::EPSILON {
        return 0.0;
    }
    let offset = 0.5 * (left - right) / denominator;
    if offset.abs() > 1.0 {
        0.0
    } else {
        offset
    }
}

fn percentile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    if sorted.len() == 1 {
        return sorted[0];
    }
    // Linear interpolation between order statistics, so a percentile of a
    // small sample is not silently the maximum.
    let rank = q * (sorted.len() - 1) as f64;
    let low = rank.floor() as usize;
    let high = rank.ceil() as usize;
    if low == high {
        sorted[low]
    } else {
        sorted[low] + (rank - low as f64) * (sorted[high] - sorted[low])
    }
}

/// The median of an already sorted slice.
fn median(sorted: &[f64]) -> f64 {
    percentile(sorted, 0.5)
}

/// One window's lag, or `None` when its peak did not clear the floor, plus
/// whether the peak sat on the search edge.
struct WindowOutcome {
    lag_us: f64,
    coefficient: f64,
    at_edge: bool,
}

fn window_lag(
    a: &[f64],
    b: &[f64],
    start: usize,
    settings: &LagSettings,
    sample_rate_hz: f64,
) -> WindowOutcome {
    let w = settings.window_frames;
    let l = settings.max_lag_frames as i64;

    let reference = &a[start..start + w];
    let reference_mean = reference.iter().sum::<f64>() / w as f64;
    let reference_centred: Vec<f64> = reference.iter().map(|v| v - reference_mean).collect();
    let reference_norm = reference_centred.iter().map(|v| v * v).sum::<f64>().sqrt();

    // The raw correlation, which is what the sub-sample reconstruction runs on:
    // it is a sampled band-limited function of the lag, and the NORMALISED
    // coefficient is not, because its divisor moves with the lag.
    let mut raw = vec![0.0f64; (2 * l + 1) as usize];
    let mut best_coefficient = f64::NEG_INFINITY;
    let mut best_index = 0usize;
    for (index, slot) in raw.iter_mut().enumerate() {
        let k = index as i64 - l;
        let from = (start as i64 + k) as usize;
        let other = &b[from..from + w];
        let other_mean = other.iter().sum::<f64>() / w as f64;
        let mut dot = 0.0;
        let mut other_sq = 0.0;
        for (x, y) in reference_centred.iter().zip(other.iter()) {
            let y = y - other_mean;
            dot += x * y;
            other_sq += y * y;
        }
        *slot = dot;
        let denominator = reference_norm * other_sq.sqrt();
        let coefficient = if denominator > 0.0 {
            dot / denominator
        } else {
            0.0
        };
        if coefficient > best_coefficient {
            best_coefficient = coefficient;
            best_index = index;
        }
    }

    let at_edge = best_index == 0 || best_index == raw.len() - 1;
    if at_edge {
        return WindowOutcome {
            lag_us: (best_index as i64 - l) as f64 * 1_000_000.0 / sample_rate_hz,
            coefficient: best_coefficient,
            at_edge: true,
        };
    }

    // Step two: the peak on a continuum, from the band-limited reconstruction
    // of the raw correlation across the sample either side of it.
    let mut best_tau = best_index as f64;
    let mut best_value = f64::NEG_INFINITY;
    let mut fine = Vec::with_capacity(2 * FINE_STEPS + 1);
    for step in 0..=(2 * FINE_STEPS) {
        let tau = best_index as f64 - 1.0 + step as f64 / FINE_STEPS as f64;
        let value = reconstruct(&raw, tau);
        fine.push(value);
        if value > best_value {
            best_value = value;
            best_tau = tau;
        }
    }
    // And the vertex of a parabola through the best three fine points, so the
    // answer is not quantised at the fine grid either.
    let at = fine
        .iter()
        .enumerate()
        .max_by(|x, y| x.1.partial_cmp(y.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, _)| i)
        .unwrap_or(FINE_STEPS);
    if at > 0 && at < fine.len() - 1 {
        let offset = parabolic_vertex(fine[at - 1], fine[at], fine[at + 1]);
        best_tau += offset / FINE_STEPS as f64;
    }

    let lag_frames = best_tau - l as f64;
    WindowOutcome {
        lag_us: lag_frames * 1_000_000.0 / sample_rate_hz,
        coefficient: best_coefficient,
        at_edge: false,
    }
}

/// Estimate the inter-device lag across a capture.
pub fn estimate(capture: &Capture, settings: &LagSettings) -> Result<LagSummary, LagError> {
    let sample_rate_hz = f64::from(capture.sample_rate_hz);
    let frames = capture.frames();
    let required = settings.window_frames + 2 * settings.max_lag_frames;
    if frames < required {
        return Err(LagError::CaptureTooShort {
            frames,
            required_frames: required,
        });
    }

    // Refusal one: a silent channel. Checked before anything is correlated,
    // because correlating against silence produces a number.
    for (name, channel) in [("A", &capture.a), ("B", &capture.b)] {
        let level = dbfs(rms(channel));
        if level < settings.silence_floor_dbfs {
            return Err(LagError::SilentChannel {
                channel: name,
                rms_dbfs: level,
                floor_dbfs: settings.silence_floor_dbfs,
            });
        }
    }

    // Refusal two: no chirp in the declared band. A capture with energy but no
    // sweep is a different fault from a capture whose sweeps do not agree, and
    // an operator is owed the difference.
    let probe = settings.window_frames.min(frames);
    for (name, channel) in [("A", &capture.a), ("B", &capture.b)] {
        let fraction = in_band_energy_fraction(
            &channel[..probe],
            sample_rate_hz,
            settings.chirp_band_hz.0,
            settings.chirp_band_hz.1,
        );
        if fraction < settings.chirp_band_fraction_floor {
            return Err(LagError::NoChirpPresent {
                channel: name,
                in_band_fraction: fraction,
                floor: settings.chirp_band_fraction_floor,
                band_hz: settings.chirp_band_hz,
            });
        }
    }

    let mut used: Vec<WindowLag> = Vec::new();
    let mut best_coefficient = f64::NEG_INFINITY;
    let mut edge_windows = 0usize;
    let mut windows_total = 0usize;
    let last_start = frames - settings.window_frames - settings.max_lag_frames;
    let mut start = settings.max_lag_frames;
    while start <= last_start {
        let outcome = window_lag(&capture.a, &capture.b, start, settings, sample_rate_hz);
        if outcome.coefficient > best_coefficient {
            best_coefficient = outcome.coefficient;
        }
        // A peak at the edge only means "the answer may be outside the search
        // range" when it is a peak at all. A weak best-of-a-bad-lot that
        // happens to fall on the boundary is an ordinary below-the-floor
        // window, and reporting it as an edge would hide the real fault.
        if outcome.at_edge && outcome.coefficient >= settings.confidence_floor {
            edge_windows += 1;
        } else if !outcome.at_edge && outcome.coefficient >= settings.confidence_floor {
            used.push(WindowLag {
                index: windows_total,
                start_frame: start,
                lag_us: outcome.lag_us,
                coefficient: outcome.coefficient,
            });
        }
        windows_total += 1;
        start += settings.hop_frames;
    }

    // Refusal three: the peak never cleared the floor anywhere.
    if used.is_empty() && edge_windows == 0 {
        return Err(LagError::BelowConfidenceFloor {
            best_coefficient,
            floor: settings.confidence_floor,
            windows_total,
        });
    }
    // Refusal four: the peak sat on the edge of the declared search range. A
    // single edge window is enough to refuse: the range is a claim about where
    // the answer can be, and a peak on the boundary says that claim may be
    // wrong.
    if edge_windows > 0 {
        return Err(LagError::PeakAtSearchEdge {
            edge_us: settings.max_lag_frames as f64 * 1_000_000.0 / sample_rate_hz,
            windows: edge_windows,
        });
    }
    if used.len() < settings.min_resolved_windows {
        return Err(LagError::TooFewWindowsResolved {
            resolved: used.len(),
            required: settings.min_resolved_windows,
            windows_total,
        });
    }

    let mut signed: Vec<f64> = used.iter().map(|w| w.lag_us).collect();
    signed.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    let mut magnitudes: Vec<f64> = used.iter().map(|w| w.lag_us.abs()).collect();
    magnitudes.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));

    Ok(LagSummary {
        windows_total,
        windows_used: used.len(),
        median_us: median(&signed),
        p95_abs_us: percentile(&magnitudes, 0.95),
        max_abs_us: *magnitudes.last().expect("used is not empty"),
        min_us: signed[0],
        max_us: signed[signed.len() - 1],
        min_coefficient: used
            .iter()
            .map(|w| w.coefficient)
            .fold(f64::INFINITY, f64::min),
        windows: used,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chirp::ChirpSpec;
    use std::path::PathBuf;

    fn settings() -> LagSettings {
        LagSettings {
            window_frames: 1920,
            hop_frames: 240,
            max_lag_frames: 96,
            confidence_floor: 0.6,
            silence_floor_dbfs: -80.0,
            chirp_band_hz: (1000.0, 8000.0),
            chirp_band_fraction_floor: 0.5,
            min_resolved_windows: 4,
        }
    }

    fn a_capture(delay_us: f64, frames: usize) -> Capture {
        let chirp = ChirpSpec::new(1000.0, 8000.0, 20_000.0, 0.25, 0.25, "test").unwrap();
        let rate = 96_000.0;
        let delay_s = delay_us / 1_000_000.0;
        let mut a = Vec::with_capacity(frames);
        let mut b = Vec::with_capacity(frames);
        for n in 0..frames {
            let t = n as f64 / rate;
            a.push(chirp.at(t));
            b.push(chirp.at(t - delay_s));
        }
        Capture {
            path: PathBuf::from("mem"),
            sample_rate_hz: 96_000,
            a,
            b,
        }
    }

    #[test]
    fn a_whole_sample_delay_is_recovered() {
        let capture = a_capture(10.416_666_666_666_666, 9600);
        let summary = estimate(&capture, &settings()).unwrap();
        assert!(
            (summary.median_us - 10.4166).abs() < 0.5,
            "{:?}",
            summary.median_us
        );
    }

    #[test]
    fn a_sub_sample_delay_is_recovered_far_inside_ten_microseconds() {
        let capture = a_capture(254.0, 9600);
        let summary = estimate(&capture, &settings()).unwrap();
        assert!(
            (summary.median_us - 254.0).abs() < 1.0,
            "median was {}",
            summary.median_us
        );
    }

    #[test]
    fn the_sign_says_which_channel_leads() {
        let leading = estimate(&a_capture(254.0, 9600), &settings()).unwrap();
        assert!(leading.median_us > 0.0);
        assert_eq!(leading.who_leads(), "channel A leads channel B");
        let trailing = estimate(&a_capture(-254.0, 9600), &settings()).unwrap();
        assert!(trailing.median_us < 0.0);
        assert_eq!(trailing.who_leads(), "channel B leads channel A");
    }

    #[test]
    fn a_silent_channel_is_refused_before_anything_is_correlated() {
        let mut capture = a_capture(254.0, 9600);
        capture.b.iter_mut().for_each(|v| *v = 0.0);
        let err = estimate(&capture, &settings()).unwrap_err();
        assert_eq!(err.condition(), "silent-channel");
        assert!(err.to_string().contains("channel B"), "{}", err);
    }

    #[test]
    fn a_delay_just_outside_the_declared_search_range_is_refused_not_reported() {
        // 1005 us against a declared range of 1000 us. The correlation is still
        // strong at the boundary, so the argmax lands on it. Reporting 1000 us
        // there would be a confident wrong number, which is the whole reason
        // this refusal exists.
        let capture = a_capture(1005.0, 19200);
        let err = estimate(&capture, &settings()).unwrap_err();
        assert_eq!(err.condition(), "peak-at-search-edge");
        assert!(err.to_string().contains("no lag is reported"), "{}", err);
    }

    #[test]
    fn a_delay_far_outside_the_declared_search_range_is_refused_too() {
        // 2000 us: the true peak is nowhere in the search range and the
        // correlation inside it never clears the floor. A different refusal
        // from the one above, and the right one - the capture does not look
        // like two copies of one chirp at any lag this run looked at.
        let capture = a_capture(2000.0, 19200);
        let err = estimate(&capture, &settings()).unwrap_err();
        assert_eq!(err.condition(), "below-confidence-floor");
    }

    #[test]
    fn a_capture_shorter_than_one_window_is_refused() {
        let capture = a_capture(254.0, 100);
        let err = estimate(&capture, &settings()).unwrap_err();
        assert_eq!(err.condition(), "capture-too-short");
    }

    #[test]
    fn a_full_band_noise_capture_carries_no_chirp() {
        let mut rng = crate::rng::Rng::new(5);
        let frames = 9600;
        let capture = Capture {
            path: PathBuf::from("mem"),
            sample_rate_hz: 96_000,
            a: (0..frames).map(|_| rng.next_symmetric() * 0.25).collect(),
            b: (0..frames).map(|_| rng.next_symmetric() * 0.25).collect(),
        };
        let err = estimate(&capture, &settings()).unwrap_err();
        assert_eq!(err.condition(), "no-chirp-present");
    }

    #[test]
    fn a_real_sweep_puts_nearly_all_its_energy_in_its_own_band() {
        let capture = a_capture(0.0, 9600);
        let fraction = in_band_energy_fraction(&capture.a[..1920], 96_000.0, 1000.0, 8000.0);
        assert!(fraction > 0.95, "{}", fraction);
    }

    #[test]
    fn a_percentile_of_a_small_sample_is_not_silently_the_maximum() {
        let sorted: Vec<f64> = (0..32).map(f64::from).collect();
        let p95 = percentile(&sorted, 0.95);
        assert!(p95 < 31.0, "{}", p95);
        assert!(p95 > 29.0, "{}", p95);
        assert_eq!(median(&sorted), 15.5);
    }

    #[test]
    fn the_estimator_is_not_quantised_to_whole_samples() {
        // A whole-sample estimator would report the same number for both of
        // these, since 10 us is under one 96 kHz sample.
        let one = estimate(&a_capture(254.0, 9600), &settings())
            .unwrap()
            .median_us;
        let other = estimate(&a_capture(264.0, 9600), &settings())
            .unwrap()
            .median_us;
        assert!((other - one - 10.0).abs() < 1.0, "{} vs {}", one, other);
    }
}
