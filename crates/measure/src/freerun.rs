//! The free-run drift fit: a series of relative offset observations in, a rate
//! in parts per million out, and a refusal wherever the series cannot carry
//! one.
//!
//! # What is being measured
//!
//! Two clients running with correction DISABLED. Their clocks are independent
//! crystals, so their relative offset walks in a straight line whose slope is
//! the relative rate error. BRIEF.md section 6 expects 20 to 50 ppm per device
//! and about 100 ppm relative worst case; nothing here checks against those
//! numbers, because the roadmap phase is explicit that "the baseline is what
//! the rig MEASURES, not a borrowed ppm range it checks itself against". The
//! numbers are context for a reader, not a threshold.
//!
//! # Why the fit publishes a confidence half-width
//!
//! An ordinary least-squares slope always exists. Over a short window, or under
//! jitter big enough to swamp the drift, it exists and means nothing. So the
//! fit carries the 95% confidence half-width of its own slope, and a run
//! publishes no ppm figure when that half-width is wider than the run declared
//! it would accept. The alternative is a number with no error bar, which is
//! exactly the "sounds synced" evidence BRIEF.md guardrail 3 exists to refuse.
//!
//! # The time base
//!
//! Every observation's timestamp is nanoseconds from a monotonic source, which
//! is the contract `docs/protocol.md` already puts on every timestamp this
//! project moves. The fit derives a rate from elapsed time, so a settable
//! clock anywhere on this path would put a step in the middle of a straight
//! line and be read as drift. `crates/measure/tests/no_settable_wall_clock.rs`
//! is what holds that.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::config::MeasureConfig;

/// The section header the observations follow in a series file.
pub const OBSERVATIONS_SECTION: &str = "[observations]";

/// One client pair's relative offset, observed once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Observation {
    /// When it was taken, in nanoseconds from a monotonic source.
    pub t_ns: i64,
    /// The relative offset between the two clients at that moment, in
    /// nanoseconds.
    pub offset_ns: i64,
}

/// A series of observations, plus what the file that carried them says about
/// itself.
#[derive(Debug, Clone, PartialEq)]
pub struct OffsetSeries {
    /// Where it came from.
    pub path: PathBuf,
    /// What the file calls itself.
    pub label: String,
    /// The rate the file states it was generated at, where it states one. This
    /// is fixture metadata; nothing in the fit reads it.
    pub declared_rate_ppm: Option<f64>,
    /// The jitter level the file states it carries, where it states one.
    pub declared_jitter_us: Option<f64>,
    /// The observations, in file order.
    pub observations: Vec<Observation>,
}

impl OffsetSeries {
    /// How long the series runs, in seconds.
    pub fn span_s(&self) -> f64 {
        match (self.observations.first(), self.observations.last()) {
            (Some(first), Some(last)) => (last.t_ns - first.t_ns) as f64 / 1e9,
            _ => 0.0,
        }
    }
}

/// The settings a free-run run declares before it looks at a series.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlopeSettings {
    /// The fewest observations that can be fitted.
    pub min_points: usize,
    /// The shortest span that can be fitted, in seconds.
    pub min_span_s: f64,
    /// The widest 95% confidence half-width, in ppm, that may be published.
    pub max_half_width_ppm: f64,
}

impl SlopeSettings {
    /// The settings the committed configuration declares.
    pub fn from_config(config: &MeasureConfig) -> SlopeSettings {
        SlopeSettings {
            min_points: config.free_run_min_points,
            min_span_s: config.free_run_min_span_s,
            max_half_width_ppm: config.free_run_max_half_width_ppm,
        }
    }
}

/// A fitted slope, with its own error bar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlopeFit {
    /// The relative rate, in parts per million. Positive means the offset is
    /// growing: the second client's clock runs slow relative to the first.
    pub ppm: f64,
    /// The 95% confidence half-width of that slope, in ppm.
    pub half_width_ppm: f64,
    /// The intercept, in nanoseconds, at the first observation's timestamp.
    pub intercept_ns: f64,
    /// Root mean square of the residuals, in nanoseconds. This is the jitter
    /// the series actually carries, measured rather than declared.
    pub residual_rms_ns: f64,
    /// Observations fitted.
    pub points: usize,
    /// The span they cover, in seconds.
    pub span_s: f64,
}

/// Why no ppm figure was published.
#[derive(Debug, Clone, PartialEq)]
pub enum SlopeError {
    /// The series is too short to fit.
    SeriesTooShort {
        /// Observations it holds.
        points: usize,
        /// Observations the run requires.
        required_points: usize,
        /// The span it covers, in seconds.
        span_s: f64,
        /// The span the run requires, in seconds.
        required_span_s: f64,
    },
    /// The series is too noisy for its slope to be bounded.
    SeriesTooNoisy {
        /// The half-width the fit achieved, in ppm.
        half_width_ppm: f64,
        /// The half-width the run declared it would accept.
        permitted_half_width_ppm: f64,
        /// The residual jitter that caused it, in microseconds RMS.
        residual_rms_us: f64,
        /// The slope that was NOT published.
        withheld_ppm: f64,
    },
    /// Every observation shares one timestamp, so there is no time base at all.
    NoTimeBase {
        /// Observations it holds.
        points: usize,
    },
    /// The timestamps do not increase, so they are not from a monotonic source.
    TimestampsNotMonotonic {
        /// The observation, counting from zero, that goes backwards.
        at: usize,
        /// The timestamp before it.
        previous_ns: i64,
        /// The timestamp that went backwards.
        this_ns: i64,
    },
}

impl SlopeError {
    /// A short, stable token naming which refusal this is.
    pub fn condition(&self) -> &'static str {
        match self {
            SlopeError::SeriesTooShort { .. } => "series-too-short",
            SlopeError::SeriesTooNoisy { .. } => "series-too-noisy",
            SlopeError::NoTimeBase { .. } => "no-time-base",
            SlopeError::TimestampsNotMonotonic { .. } => "timestamps-not-monotonic",
        }
    }
}

impl fmt::Display for SlopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SlopeError::SeriesTooShort {
                points,
                required_points,
                span_s,
                required_span_s,
            } => write!(
                f,
                "the free-run series is too short: {} observations over {:.1} s, and this run \
                 requires at least {} observations over at least {:.1} s. No ppm figure is \
                 published",
                points, span_s, required_points, required_span_s
            ),
            SlopeError::SeriesTooNoisy {
                half_width_ppm,
                permitted_half_width_ppm,
                residual_rms_us,
                withheld_ppm,
            } => write!(
                f,
                "the free-run series is too noisy: {:.1} us RMS of residual jitter bounds the \
                 slope only to +/-{:.3} ppm and this run declared +/-{:.3} ppm. The slope of \
                 {:.3} ppm is withheld rather than published",
                residual_rms_us, half_width_ppm, permitted_half_width_ppm, withheld_ppm
            ),
            SlopeError::NoTimeBase { points } => write!(
                f,
                "all {} observations carry the same timestamp, so the series has no time base \
                 to fit a rate against",
                points
            ),
            SlopeError::TimestampsNotMonotonic {
                at,
                previous_ns,
                this_ns,
            } => write!(
                f,
                "observation {} carries timestamp {} ns after {} ns; these timestamps are \
                 required to come from a monotonic source and these do not",
                at, this_ns, previous_ns
            ),
        }
    }
}

impl std::error::Error for SlopeError {}

/// The 97.5th percentile of the standard normal, used as the multiplier on the
/// slope's standard error.
///
/// A normal approximation to Student's t, which the run's declared floor of at
/// least 30 observations is what makes honest. Stated here rather than hidden:
/// at 30 observations the true multiplier is 2.04 rather than 1.96, so this
/// half-width is about 4% optimistic at the very edge of the permitted range
/// and closer than 1% for the hundreds of observations a real run carries.
pub const NORMAL_95: f64 = 1.96;

/// Fit the series, or refuse.
pub fn fit(series: &OffsetSeries, settings: &SlopeSettings) -> Result<SlopeFit, SlopeError> {
    let points = series.observations.len();
    for (at, pair) in series.observations.windows(2).enumerate() {
        if pair[1].t_ns < pair[0].t_ns {
            return Err(SlopeError::TimestampsNotMonotonic {
                at: at + 1,
                previous_ns: pair[0].t_ns,
                this_ns: pair[1].t_ns,
            });
        }
    }
    let span_s = series.span_s();
    if points < settings.min_points || span_s < settings.min_span_s {
        return Err(SlopeError::SeriesTooShort {
            points,
            required_points: settings.min_points,
            span_s,
            required_span_s: settings.min_span_s,
        });
    }

    let t0 = series.observations[0].t_ns;
    let n = points as f64;
    // Ordinary least squares, with time measured from the first observation so
    // the sums stay far away from the end of an f64's precision.
    let xs: Vec<f64> = series
        .observations
        .iter()
        .map(|o| (o.t_ns - t0) as f64)
        .collect();
    let ys: Vec<f64> = series
        .observations
        .iter()
        .map(|o| o.offset_ns as f64)
        .collect();
    let mean_x = xs.iter().sum::<f64>() / n;
    let mean_y = ys.iter().sum::<f64>() / n;
    let mut sxx = 0.0;
    let mut sxy = 0.0;
    for (x, y) in xs.iter().zip(ys.iter()) {
        sxx += (x - mean_x) * (x - mean_x);
        sxy += (x - mean_x) * (y - mean_y);
    }
    if sxx <= 0.0 {
        return Err(SlopeError::NoTimeBase { points });
    }

    let slope = sxy / sxx;
    let intercept = mean_y - slope * mean_x;
    let mut sse = 0.0;
    for (x, y) in xs.iter().zip(ys.iter()) {
        let residual = y - (intercept + slope * x);
        sse += residual * residual;
    }
    let residual_variance = sse / (n - 2.0);
    let residual_rms_ns = (sse / n).sqrt();
    let standard_error = (residual_variance / sxx).sqrt();

    // The slope is nanoseconds of offset per nanosecond of elapsed time, which
    // is dimensionless; a million of those is a part per million.
    let ppm = slope * 1e6;
    let half_width_ppm = NORMAL_95 * standard_error * 1e6;

    if half_width_ppm > settings.max_half_width_ppm {
        return Err(SlopeError::SeriesTooNoisy {
            half_width_ppm,
            permitted_half_width_ppm: settings.max_half_width_ppm,
            residual_rms_us: residual_rms_ns / 1000.0,
            withheld_ppm: ppm,
        });
    }

    Ok(SlopeFit {
        ppm,
        half_width_ppm,
        intercept_ns: intercept,
        residual_rms_ns,
        points,
        span_s,
    })
}

/// Why a series file could not be read.
#[derive(Debug, Clone, PartialEq)]
pub enum SeriesError {
    /// The file could not be opened or read.
    Unreadable {
        /// The path that was tried.
        path: PathBuf,
        /// What the operating system said.
        detail: String,
    },
    /// A line is not what the format allows there.
    Malformed {
        /// The path that was read.
        path: PathBuf,
        /// One-based line number.
        line: usize,
        /// What was wrong with it.
        detail: String,
    },
}

impl fmt::Display for SeriesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SeriesError::Unreadable { path, detail } => write!(
                f,
                "the free-run series '{}' could not be read: {}",
                path.display(),
                detail
            ),
            SeriesError::Malformed { path, line, detail } => {
                write!(f, "{} line {}: {}", path.display(), line, detail)
            }
        }
    }
}

impl std::error::Error for SeriesError {}

/// Read a committed series file.
///
/// The format is the repository's `key = value` with `#` comments, then an
/// `[observations]` header, then one `t_ns offset_ns` pair per line. It needs
/// nothing more than a whitespace splitter to read, which is what lets a
/// second-language implementation grade the same fixture.
pub fn read_series(path: &Path) -> Result<OffsetSeries, SeriesError> {
    let text = std::fs::read_to_string(path).map_err(|e| SeriesError::Unreadable {
        path: path.to_path_buf(),
        detail: e.to_string(),
    })?;
    parse_series(path, &text)
}

/// Parse series text already in hand.
pub fn parse_series(path: &Path, text: &str) -> Result<OffsetSeries, SeriesError> {
    let mut series = OffsetSeries {
        path: path.to_path_buf(),
        label: String::new(),
        declared_rate_ppm: None,
        declared_jitter_us: None,
        observations: Vec::new(),
    };
    let mut in_observations = false;
    for (index, raw) in text.lines().enumerate() {
        let line = match raw.find('#') {
            Some(at) => &raw[..at],
            None => raw,
        }
        .trim();
        if line.is_empty() {
            continue;
        }
        if line == OBSERVATIONS_SECTION {
            in_observations = true;
            continue;
        }
        if !in_observations {
            let (key, value) = line.split_once('=').ok_or_else(|| SeriesError::Malformed {
                path: path.to_path_buf(),
                line: index + 1,
                detail: format!("'{}' is not 'key = value' and no {} header has been seen", line, OBSERVATIONS_SECTION),
            })?;
            let key = key.trim();
            let value = value.trim();
            match key {
                "label" => series.label = value.to_string(),
                "declared_rate_ppm" => series.declared_rate_ppm = value.parse().ok(),
                "declared_jitter_us" => series.declared_jitter_us = value.parse().ok(),
                _ => {}
            }
            continue;
        }
        let mut fields = line.split_whitespace();
        let (Some(t), Some(offset), None) = (fields.next(), fields.next(), fields.next()) else {
            return Err(SeriesError::Malformed {
                path: path.to_path_buf(),
                line: index + 1,
                detail: format!("'{}' is not exactly two whitespace separated integers", line),
            });
        };
        let t_ns = t.parse::<i64>().map_err(|_| SeriesError::Malformed {
            path: path.to_path_buf(),
            line: index + 1,
            detail: format!("'{}' is not a nanosecond timestamp", t),
        })?;
        let offset_ns = offset.parse::<i64>().map_err(|_| SeriesError::Malformed {
            path: path.to_path_buf(),
            line: index + 1,
            detail: format!("'{}' is not a nanosecond offset", offset),
        })?;
        series.observations.push(Observation { t_ns, offset_ns });
    }
    Ok(series)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> SlopeSettings {
        SlopeSettings {
            min_points: 30,
            min_span_s: 60.0,
            max_half_width_ppm: 1.0,
        }
    }

    fn a_series(points: usize, interval_s: f64, ppm: f64, jitter_ns: f64, seed: u64) -> OffsetSeries {
        let mut rng = crate::rng::Rng::new(seed);
        let observations = (0..points)
            .map(|n| {
                let t_ns = (n as f64 * interval_s * 1e9).round() as i64;
                let ideal = t_ns as f64 * ppm / 1e6;
                let noise = if jitter_ns > 0.0 {
                    rng.next_normal() * jitter_ns
                } else {
                    0.0
                };
                Observation {
                    t_ns,
                    offset_ns: (ideal + noise).round() as i64,
                }
            })
            .collect();
        OffsetSeries {
            path: PathBuf::from("mem"),
            label: "test".to_string(),
            declared_rate_ppm: Some(ppm),
            declared_jitter_us: Some(jitter_ns / 1000.0),
            observations,
        }
    }

    #[test]
    fn a_noiseless_series_recovers_its_rate() {
        let fit = fit(&a_series(601, 1.0, 37.5, 0.0, 1), &settings()).unwrap();
        assert!((fit.ppm - 37.5).abs() < 0.5, "{}", fit.ppm);
        assert_eq!(fit.points, 601);
    }

    #[test]
    fn a_jittered_series_recovers_its_rate_and_says_how_well() {
        let fit = fit(&a_series(601, 1.0, 37.5, 20_000.0, 2), &settings()).unwrap();
        assert!((fit.ppm - 37.5).abs() < 5.0, "{}", fit.ppm);
        assert!(fit.half_width_ppm < 1.0, "{}", fit.half_width_ppm);
        assert!(fit.residual_rms_ns > 10_000.0, "{}", fit.residual_rms_ns);
    }

    #[test]
    fn a_short_series_publishes_nothing_and_names_the_condition() {
        let err = fit(&a_series(21, 1.0, 37.5, 0.0, 3), &settings()).unwrap_err();
        assert_eq!(err.condition(), "series-too-short");
        let said = err.to_string();
        assert!(said.contains("No ppm figure is published"), "{}", said);
    }

    #[test]
    fn a_noisy_series_publishes_nothing_and_names_the_condition() {
        let err = fit(&a_series(121, 1.0, 37.5, 3_000_000.0, 4), &settings()).unwrap_err();
        assert_eq!(err.condition(), "series-too-noisy");
        let said = err.to_string();
        assert!(said.contains("withheld rather than published"), "{}", said);
    }

    #[test]
    fn a_series_whose_timestamps_go_backwards_is_refused() {
        let mut series = a_series(601, 1.0, 37.5, 0.0, 5);
        series.observations[300].t_ns -= 5_000_000_000;
        let err = fit(&series, &settings()).unwrap_err();
        assert_eq!(err.condition(), "timestamps-not-monotonic");
    }

    #[test]
    fn a_series_with_one_timestamp_has_no_time_base() {
        let mut series = a_series(601, 1.0, 37.5, 0.0, 6);
        series.observations.iter_mut().for_each(|o| o.t_ns = 0);
        // Span is zero, so the length check fires first; that is the honest
        // order, and the no-time-base refusal covers a series that passes the
        // length check with a degenerate spread.
        let err = fit(&series, &settings()).unwrap_err();
        assert_eq!(err.condition(), "series-too-short");
    }

    #[test]
    fn the_series_format_round_trips_through_the_reader() {
        let text = "# a comment\n\
                    label = example\n\
                    declared_rate_ppm = 37.5\n\
                    declared_jitter_us = 20\n\
                    [observations]\n\
                    0 0\n\
                    1000000000 37\n\
                    2000000000 75\n";
        let series = parse_series(Path::new("mem"), text).unwrap();
        assert_eq!(series.label, "example");
        assert_eq!(series.declared_rate_ppm, Some(37.5));
        assert_eq!(series.declared_jitter_us, Some(20.0));
        assert_eq!(series.observations.len(), 3);
        assert_eq!(series.observations[2].offset_ns, 75);
        assert!((series.span_s() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn an_observation_line_that_is_not_two_integers_is_refused_by_line_number() {
        let text = "[observations]\n0 0\n1000000000 thirty-seven\n";
        let err = parse_series(Path::new("mem"), text).unwrap_err();
        assert!(err.to_string().contains("line 3"), "{}", err);
    }
}
