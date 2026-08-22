//! Committed simulator scenarios.
//!
//! A scenario is a text file under `fixtures/sync/`: the whole configuration
//! plus the two assertions that configuration is committed to hold. Keeping
//! the assertion in the file rather than in a test means adding a scenario is
//! adding a file, and it means the numbers a regression enforces are reviewable
//! in a diff instead of buried in code.

use std::fmt;

use crate::config::{ConfigError, SimConfig};
use crate::jitter::JitterModel;
use crate::servo::ServoConfig;

/// Why a scenario file could not be read.
#[derive(Debug, Clone, PartialEq)]
pub enum ScenarioError {
    /// A line is not `key = value`.
    Malformed {
        /// 1-based line number.
        line: usize,
        /// The offending line.
        text: String,
    },
    /// The same key appears twice.
    DuplicateKey {
        /// 1-based line number of the second appearance.
        line: usize,
        /// The key.
        key: String,
    },
    /// A key the format does not define.
    UnknownKey {
        /// 1-based line number.
        line: usize,
        /// The key.
        key: String,
    },
    /// A key the format requires is absent.
    MissingKey {
        /// The key.
        key: &'static str,
    },
    /// A value that does not parse as the type its key needs.
    BadValue {
        /// The key.
        key: String,
        /// The value as written.
        value: String,
    },
    /// The scenario parsed and describes a configuration the simulator
    /// refuses.
    Invalid(ConfigError),
}

impl fmt::Display for ScenarioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScenarioError::Malformed { line, text } => {
                write!(f, "line {}: {:?} is not `key = value`", line, text)
            }
            ScenarioError::DuplicateKey { line, key } => {
                write!(f, "line {}: key {:?} appears twice", line, key)
            }
            ScenarioError::UnknownKey { line, key } => {
                write!(f, "line {}: unknown key {:?}", line, key)
            }
            ScenarioError::MissingKey { key } => write!(f, "missing key {:?}", key),
            ScenarioError::BadValue { key, value } => {
                write!(f, "{} = {:?} does not parse", key, value)
            }
            ScenarioError::Invalid(inner) => write!(f, "{}", inner),
        }
    }
}

impl std::error::Error for ScenarioError {}

/// A configuration plus what it is committed to hold.
#[derive(Debug, Clone, PartialEq)]
pub struct Scenario {
    /// Human readable name, from the file.
    pub name: String,
    /// The run.
    pub config: SimConfig,
    /// The modelled playout error must be inside the bound by this time.
    pub settle_deadline_ms: u64,
    /// The bound itself, in nanoseconds. 1 ms for every scenario this phase
    /// commits, which is the acceptance criterion the roadmap phase carries.
    pub error_bound_ns: i64,
}

/// Every key a scenario file may set. `error_bound_ns` is the only optional
/// one, and it defaults to the 1 ms this phase is held to.
const KEYS: [&str; 13] = [
    "name",
    "seed",
    "duration_ms",
    "step_ms",
    "sync_interval_ms",
    "server_ppm",
    "client_ppm",
    "initial_offset_ns",
    "base_one_way_delay_us",
    "jitter_model",
    "jitter_scale_us",
    "settle_deadline_ms",
    "error_bound_ns",
];

impl Scenario {
    /// Parse a scenario file.
    ///
    /// Unknown keys are an error rather than a shrug: a typo in a committed
    /// fixture that silently fell back to a default would quietly weaken the
    /// regression it exists to enforce.
    pub fn parse(text: &str) -> Result<Scenario, ScenarioError> {
        let mut pairs: Vec<(String, String)> = Vec::new();

        for (index, raw) in text.lines().enumerate() {
            let line_no = index + 1;
            let line = match raw.find('#') {
                Some(at) => &raw[..at],
                None => raw,
            };
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let split = match line.find('=') {
                Some(at) => at,
                None => {
                    return Err(ScenarioError::Malformed {
                        line: line_no,
                        text: line.to_string(),
                    })
                }
            };
            let key = line[..split].trim().to_string();
            let value = line[split + 1..].trim().to_string();
            if key.is_empty() {
                return Err(ScenarioError::Malformed {
                    line: line_no,
                    text: line.to_string(),
                });
            }
            if !KEYS.contains(&key.as_str()) {
                return Err(ScenarioError::UnknownKey {
                    line: line_no,
                    key,
                });
            }
            if pairs
                .iter()
                .any(|(existing, _)| existing.as_str() == key.as_str())
            {
                return Err(ScenarioError::DuplicateKey {
                    line: line_no,
                    key,
                });
            }
            pairs.push((key, value));
        }

        let jitter_name = required(&pairs, "jitter_model")?.to_string();
        let jitter_scale_us = parse_f64("jitter_scale_us", required(&pairs, "jitter_scale_us")?)?;
        let jitter = JitterModel::from_name(&jitter_name, jitter_scale_us).ok_or(
            ScenarioError::BadValue {
                key: "jitter_model".to_string(),
                value: jitter_name.clone(),
            },
        )?;

        let config = SimConfig {
            seed: parse_u64("seed", required(&pairs, "seed")?)?,
            duration_ms: parse_u64("duration_ms", required(&pairs, "duration_ms")?)?,
            step_ms: parse_u64("step_ms", required(&pairs, "step_ms")?)?,
            sync_interval_ms: parse_u64(
                "sync_interval_ms",
                required(&pairs, "sync_interval_ms")?,
            )?,
            server_ppm: parse_f64("server_ppm", required(&pairs, "server_ppm")?)?,
            client_ppm: parse_f64("client_ppm", required(&pairs, "client_ppm")?)?,
            initial_offset_ns: parse_i64(
                "initial_offset_ns",
                required(&pairs, "initial_offset_ns")?,
            )?,
            base_one_way_delay_us: parse_f64(
                "base_one_way_delay_us",
                required(&pairs, "base_one_way_delay_us")?,
            )?,
            jitter,
            servo: ServoConfig::default(),
        };
        config.validate().map_err(ScenarioError::Invalid)?;

        let error_bound_ns = match optional(&pairs, "error_bound_ns") {
            Some(value) => parse_i64("error_bound_ns", value)?,
            None => 1_000_000,
        };

        Ok(Scenario {
            name: required(&pairs, "name")?.to_string(),
            config,
            settle_deadline_ms: parse_u64(
                "settle_deadline_ms",
                required(&pairs, "settle_deadline_ms")?,
            )?,
            error_bound_ns,
        })
    }

    /// The settle deadline in nanoseconds, which is what [`SimResult`] speaks.
    ///
    /// [`SimResult`]: crate::sim::SimResult
    pub fn settle_deadline_ns(&self) -> u64 {
        self.settle_deadline_ms.saturating_mul(1_000_000)
    }
}

fn optional<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find(|(existing, _)| existing.as_str() == key)
        .map(|(_, value)| value.as_str())
}

fn required<'a>(pairs: &'a [(String, String)], key: &'static str) -> Result<&'a str, ScenarioError> {
    optional(pairs, key).ok_or(ScenarioError::MissingKey { key })
}

fn parse_u64(key: &str, value: &str) -> Result<u64, ScenarioError> {
    value.parse::<u64>().map_err(|_| ScenarioError::BadValue {
        key: key.to_string(),
        value: value.to_string(),
    })
}

fn parse_i64(key: &str, value: &str) -> Result<i64, ScenarioError> {
    value.parse::<i64>().map_err(|_| ScenarioError::BadValue {
        key: key.to_string(),
        value: value.to_string(),
    })
}

fn parse_f64(key: &str, value: &str) -> Result<f64, ScenarioError> {
    let parsed = value.parse::<f64>().map_err(|_| ScenarioError::BadValue {
        key: key.to_string(),
        value: value.to_string(),
    })?;
    if !parsed.is_finite() {
        return Err(ScenarioError::BadValue {
            key: key.to_string(),
            value: value.to_string(),
        });
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::{Scenario, ScenarioError};
    use crate::jitter::JitterModel;

    const SAMPLE: &str = "\
# a comment
name = example
seed = 7
duration_ms = 30000
step_ms = 10
sync_interval_ms = 1000
server_ppm = -12.5
client_ppm = 38.0
initial_offset_ns = -8000000
base_one_way_delay_us = 200.0
jitter_model = exponential
jitter_scale_us = 150.0
settle_deadline_ms = 15000
";

    #[test]
    fn a_scenario_round_trips_from_text() {
        let scenario = Scenario::parse(SAMPLE).expect("the sample parses");
        assert_eq!(scenario.name, "example");
        assert_eq!(scenario.config.seed, 7);
        assert_eq!(scenario.config.duration_ms, 30_000);
        assert_eq!(scenario.config.server_ppm, -12.5);
        assert_eq!(scenario.config.initial_offset_ns, -8_000_000);
        assert_eq!(
            scenario.config.jitter,
            JitterModel::Exponential { mean_us: 150.0 }
        );
        assert_eq!(scenario.error_bound_ns, 1_000_000, "the default bound");
        assert_eq!(scenario.settle_deadline_ns(), 15_000_000_000);
    }

    #[test]
    fn a_missing_key_is_an_error() {
        let text = SAMPLE.replace("seed = 7\n", "");
        assert_eq!(
            Scenario::parse(&text),
            Err(ScenarioError::MissingKey { key: "seed" })
        );
    }

    #[test]
    fn an_unknown_key_is_an_error() {
        let text = format!("{}jitter_kind = uniform\n", SAMPLE);
        match Scenario::parse(&text) {
            Err(ScenarioError::UnknownKey { key, .. }) => assert_eq!(key, "jitter_kind"),
            other => panic!("expected an unknown key error, got {:?}", other),
        }
    }

    #[test]
    fn a_duplicate_key_is_an_error() {
        let text = format!("{}seed = 9\n", SAMPLE);
        match Scenario::parse(&text) {
            Err(ScenarioError::DuplicateKey { key, .. }) => assert_eq!(key, "seed"),
            other => panic!("expected a duplicate key error, got {:?}", other),
        }
    }

    #[test]
    fn a_value_that_does_not_parse_is_an_error() {
        let text = SAMPLE.replace("seed = 7", "seed = seven");
        match Scenario::parse(&text) {
            Err(ScenarioError::BadValue { key, value }) => {
                assert_eq!(key, "seed");
                assert_eq!(value, "seven");
            }
            other => panic!("expected a bad value error, got {:?}", other),
        }
    }

    #[test]
    fn an_unmodellable_configuration_is_refused_at_parse_time() {
        let text = SAMPLE.replace("client_ppm = 38.0", "client_ppm = 9000.0");
        match Scenario::parse(&text) {
            Err(ScenarioError::Invalid(_)) => {}
            other => panic!("expected an invalid configuration, got {:?}", other),
        }
    }

    #[test]
    fn an_unknown_jitter_model_is_an_error() {
        let text = SAMPLE.replace("jitter_model = exponential", "jitter_model = gaussian");
        match Scenario::parse(&text) {
            Err(ScenarioError::BadValue { key, value }) => {
                assert_eq!(key, "jitter_model");
                assert_eq!(value, "gaussian");
            }
            other => panic!("expected a bad value error, got {:?}", other),
        }
    }
}
