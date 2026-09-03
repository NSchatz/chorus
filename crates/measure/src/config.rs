//! The values this rig declares about itself, committed in
//! `config/measure.conf`.
//!
//! Every number a run compares against - the amplitude ceiling that stands in
//! front of a real amplifier, the confidence floor a correlation peak has to
//! clear, the shortest free-run series that can be fitted - lives in one
//! committed file rather than in a default buried in code. The rig reports
//! nothing it cannot also say the threshold for, and a threshold nobody can
//! read is not a declared threshold.
//!
//! Format is the repository's existing one, the same `key = value` with `#`
//! comments that `config/verification.conf` and `fixtures/sync/*.cfg` use, so
//! the shell entry points can read the same file with the `conf` helper in
//! `tools/lib.sh`.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

/// The committed file, relative to the repository root.
pub const CONFIG_FILE: &str = "config/measure.conf";

/// Why the declared configuration could not be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// The file is not there, or could not be read.
    Unreadable {
        /// The path that was tried.
        path: PathBuf,
        /// What the operating system said.
        detail: String,
    },
    /// A line is not `key = value`.
    Malformed {
        /// The path that was read.
        path: PathBuf,
        /// One-based line number.
        line: usize,
        /// What was wrong with it.
        detail: String,
    },
    /// A key this rig needs is absent.
    Missing {
        /// The path that was read.
        path: PathBuf,
        /// The key that is not in it.
        key: String,
    },
    /// A value is present and cannot be read as the number it has to be.
    NotANumber {
        /// The path that was read.
        path: PathBuf,
        /// The key whose value is wrong.
        key: String,
        /// The value as written.
        value: String,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Unreadable { path, detail } => write!(
                f,
                "the declared measurement configuration '{}' could not be read: {}",
                path.display(),
                detail
            ),
            ConfigError::Malformed { path, line, detail } => {
                write!(f, "{} line {}: {}", path.display(), line, detail)
            }
            ConfigError::Missing { path, key } => write!(
                f,
                "{} declares no '{}', and this rig will not supply one of its own",
                path.display(),
                key
            ),
            ConfigError::NotANumber { path, key, value } => write!(
                f,
                "{} gives '{}' as '{}', which is not a number",
                path.display(),
                key,
                value
            ),
        }
    }
}

impl std::error::Error for ConfigError {}

/// The declared configuration, as read.
#[derive(Debug, Clone, PartialEq)]
pub struct MeasureConfig {
    /// Where it was read from.
    pub path: PathBuf,
    /// The highest chirp amplitude, in full-scale units, any run may request.
    ///
    /// This is the guard in front of the only irreversible thing this rig can
    /// do: the chirp leaves through a real amplifier into a real loudspeaker,
    /// and an over-level chirp damages a driver rather than failing a test.
    pub chirp_amplitude_ceiling: f64,
    /// The rate a capture is expected to have been taken at.
    pub capture_sample_rate_hz: u32,
    /// The low edge of the band the chirp sweeps.
    pub chirp_start_hz: f64,
    /// The high edge of the band the chirp sweeps.
    pub chirp_end_hz: f64,
    /// How long one sweep lasts, in microseconds.
    pub chirp_period_us: f64,
    /// Analysis window length, in microseconds.
    pub window_us: f64,
    /// How far one analysis window starts after the previous one, in
    /// microseconds.
    pub hop_us: f64,
    /// The widest lag the correlation searches, in microseconds, either way.
    pub max_lag_us: f64,
    /// The normalised correlation coefficient a window's peak has to reach
    /// before its lag is used.
    pub confidence_floor: f64,
    /// A channel quieter than this, in dBFS, is silent.
    pub silence_floor_dbfs: f64,
    /// The fraction of a channel's energy that has to lie inside the chirp's
    /// band before the rig will accept that a chirp is present.
    pub chirp_band_fraction_floor: f64,
    /// The fewest windows that have to resolve before a run reports a figure.
    pub min_resolved_windows: usize,
    /// The fewest observations a free-run series needs.
    pub free_run_min_points: usize,
    /// The shortest free-run series, in seconds.
    pub free_run_min_span_s: f64,
    /// The widest 95% confidence half-width, in ppm, a published slope may
    /// carry.
    pub free_run_max_half_width_ppm: f64,
}

fn parse(path: &Path, text: &str) -> Result<BTreeMap<String, String>, ConfigError> {
    let mut out = BTreeMap::new();
    for (index, raw) in text.lines().enumerate() {
        let line = match raw.find('#') {
            Some(at) => &raw[..at],
            None => raw,
        }
        .trim();
        if line.is_empty() {
            continue;
        }
        let (key, value) = line.split_once('=').ok_or_else(|| ConfigError::Malformed {
            path: path.to_path_buf(),
            line: index + 1,
            detail: format!("'{}' is not 'key = value'", line),
        })?;
        out.insert(key.trim().to_string(), value.trim().to_string());
    }
    Ok(out)
}

fn number(
    path: &Path,
    values: &BTreeMap<String, String>,
    key: &str,
) -> Result<f64, ConfigError> {
    let raw = values.get(key).ok_or_else(|| ConfigError::Missing {
        path: path.to_path_buf(),
        key: key.to_string(),
    })?;
    raw.parse::<f64>().map_err(|_| ConfigError::NotANumber {
        path: path.to_path_buf(),
        key: key.to_string(),
        value: raw.clone(),
    })
}

impl MeasureConfig {
    /// Read the committed configuration from a repository root.
    pub fn read(root: &Path) -> Result<MeasureConfig, ConfigError> {
        let path = root.join(CONFIG_FILE);
        let text = std::fs::read_to_string(&path).map_err(|e| ConfigError::Unreadable {
            path: path.clone(),
            detail: e.to_string(),
        })?;
        MeasureConfig::parse(&path, &text)
    }

    /// Parse configuration text that is already in hand.
    pub fn parse(path: &Path, text: &str) -> Result<MeasureConfig, ConfigError> {
        let values = parse(path, text)?;
        Ok(MeasureConfig {
            path: path.to_path_buf(),
            chirp_amplitude_ceiling: number(path, &values, "chirp_amplitude_ceiling")?,
            capture_sample_rate_hz: number(path, &values, "capture_sample_rate_hz")? as u32,
            chirp_start_hz: number(path, &values, "chirp_start_hz")?,
            chirp_end_hz: number(path, &values, "chirp_end_hz")?,
            chirp_period_us: number(path, &values, "chirp_period_us")?,
            window_us: number(path, &values, "window_us")?,
            hop_us: number(path, &values, "hop_us")?,
            max_lag_us: number(path, &values, "max_lag_us")?,
            confidence_floor: number(path, &values, "confidence_floor")?,
            silence_floor_dbfs: number(path, &values, "silence_floor_dbfs")?,
            chirp_band_fraction_floor: number(path, &values, "chirp_band_fraction_floor")?,
            min_resolved_windows: number(path, &values, "min_resolved_windows")? as usize,
            free_run_min_points: number(path, &values, "free_run_min_points")? as usize,
            free_run_min_span_s: number(path, &values, "free_run_min_span_s")?,
            free_run_max_half_width_ppm: number(path, &values, "free_run_max_half_width_ppm")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
# a comment
chirp_amplitude_ceiling = 0.25
capture_sample_rate_hz = 96000
chirp_start_hz = 1000
chirp_end_hz = 8000
chirp_period_us = 20000
window_us = 20000
hop_us = 2500
max_lag_us = 1000
confidence_floor = 0.6
silence_floor_dbfs = -80
chirp_band_fraction_floor = 0.5
min_resolved_windows = 4
free_run_min_points = 30
free_run_min_span_s = 60
free_run_max_half_width_ppm = 1.0
";

    #[test]
    fn the_declared_values_parse() {
        let config = MeasureConfig::parse(Path::new("mem"), SAMPLE).unwrap();
        assert_eq!(config.chirp_amplitude_ceiling, 0.25);
        assert_eq!(config.capture_sample_rate_hz, 96_000);
        assert_eq!(config.min_resolved_windows, 4);
    }

    #[test]
    fn a_missing_key_is_named_rather_than_defaulted() {
        let text = SAMPLE.replace("confidence_floor = 0.6\n", "");
        let err = MeasureConfig::parse(Path::new("mem"), &text).unwrap_err();
        assert_eq!(
            err,
            ConfigError::Missing {
                path: PathBuf::from("mem"),
                key: "confidence_floor".to_string(),
            }
        );
        assert!(err.to_string().contains("will not supply one of its own"));
    }

    #[test]
    fn a_value_that_is_not_a_number_is_named() {
        let text = SAMPLE.replace("max_lag_us = 1000", "max_lag_us = soon");
        let err = MeasureConfig::parse(Path::new("mem"), &text).unwrap_err();
        assert!(err.to_string().contains("'soon'"), "{}", err);
    }

    #[test]
    fn the_committed_configuration_is_readable_and_complete() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("this crate lives at crates/<name> under the repository root");
        let config = MeasureConfig::read(root).expect("config/measure.conf is committed");
        assert!(config.chirp_amplitude_ceiling > 0.0);
        assert!(config.chirp_amplitude_ceiling <= 1.0);
        assert!(config.chirp_end_hz > config.chirp_start_hz);
        assert!(config.hop_us > 0.0 && config.hop_us <= config.window_us);
        assert!(config.confidence_floor > 0.0 && config.confidence_floor < 1.0);
    }
}
