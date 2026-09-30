//! What a simulator run is configured with, and which configurations are
//! refused.
//!
//! The simulator answers a question about a model. Handing it a configuration
//! the model does not describe, and getting a number back anyway, would be
//! worse than getting an error: the number would look like evidence. So every
//! parameter has a documented range (see
//! `docs/decisions/0006-sync-simulator-and-servo.md`) and anything outside it
//! is refused before a run starts.

use std::fmt;

use crate::jitter::JitterModel;
use crate::servo::ServoConfig;

/// Largest clock error the linear clock model will accept, in ppm.
///
/// Real crystals are 20 to 50 ppm. 1000 is 20 times the worst realistic pair,
/// and past it a constant-rate model stops describing a crystal.
pub const MAX_SKEW_PPM: f64 = 1_000.0;

/// Largest network delay or jitter scale accepted, in microseconds.
pub const MAX_DELAY_US: f64 = 100_000.0;

/// Largest crystal wander accepted, in ppm per square root of a second.
///
/// The random walk's standard deviation after `t` seconds is this times
/// `sqrt(t)`. At 0.1 a crystal wanders about 2.4 ppm in ten minutes, several
/// times what a temperature-stable room does to an uncompensated crystal
/// (ASSUMED; `docs/decisions/0006-sync-simulator-and-servo.md`). Past 1 the
/// model describes a failing oscillator, not a drifting one.
pub const MAX_WANDER_PPM_PER_SQRT_S: f64 = 1.0;

/// Largest number of steps a single run may take.
///
/// A guard against a configuration that would occupy CI for hours.
pub const MAX_STEPS: u64 = 10_000_000;

/// Fixed server turnaround between receiving a time sync request and replying,
/// in nanoseconds.
pub const SERVER_TURNAROUND_NS: f64 = 50_000.0;

/// Why a configuration was refused.
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigError {
    /// A run of no length has no playout-error series to report.
    ZeroDuration,
    /// The sampling step is zero, or longer than the run itself.
    InvalidStep {
        /// The step that was asked for, in milliseconds.
        step_ms: u64,
        /// The run length it was asked for, in milliseconds.
        duration_ms: u64,
    },
    /// Time sync exchanges cannot happen never.
    ZeroSyncInterval,
    /// A clock skew outside the modelled range.
    SkewOutOfRange {
        /// Which clock.
        clock: &'static str,
        /// The value asked for, in ppm.
        ppm: f64,
    },
    /// A network delay or jitter scale outside the modelled range.
    DelayOutOfRange {
        /// Which parameter.
        parameter: &'static str,
        /// The value asked for, in microseconds.
        value_us: f64,
    },
    /// The forward and return base delays do not both land in the modelled
    /// range once the asymmetry is applied.
    AsymmetryOutOfRange {
        /// The asymmetry asked for, in microseconds.
        value_us: f64,
    },
    /// A burst model parameter outside its range.
    InvalidJitterParameter {
        /// Which parameter.
        parameter: &'static str,
        /// The value asked for.
        value: f64,
    },
    /// A crystal wander rate that is negative, not finite, or above
    /// [`MAX_WANDER_PPM_PER_SQRT_S`].
    WanderOutOfRange {
        /// The value asked for, in ppm per square root of a second.
        value: f64,
    },
    /// A servo gain or limit that is not a usable number.
    InvalidServoParameter {
        /// Which parameter.
        parameter: &'static str,
        /// The value asked for.
        value: f64,
    },
    /// The run would take more steps than [`MAX_STEPS`].
    TooManySteps {
        /// Steps the configuration would take.
        steps: u64,
        /// The limit.
        max: u64,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::ZeroDuration => write!(f, "a run must be at least one step long"),
            ConfigError::InvalidStep {
                step_ms,
                duration_ms,
            } => write!(
                f,
                "step of {} ms does not fit a run of {} ms",
                step_ms, duration_ms
            ),
            ConfigError::ZeroSyncInterval => {
                write!(f, "the time sync interval must be at least 1 ms")
            }
            ConfigError::SkewOutOfRange { clock, ppm } => write!(
                f,
                "{} skew of {} ppm is outside +/- {} ppm",
                clock, ppm, MAX_SKEW_PPM
            ),
            ConfigError::DelayOutOfRange {
                parameter,
                value_us,
            } => write!(
                f,
                "{} of {} us is outside 0 to {} us",
                parameter, value_us, MAX_DELAY_US
            ),
            ConfigError::AsymmetryOutOfRange { value_us } => write!(
                f,
                "path asymmetry of {} us puts the return base delay outside 0 to {} us",
                value_us, MAX_DELAY_US
            ),
            ConfigError::InvalidJitterParameter { parameter, value } => {
                write!(f, "jitter {} of {} is outside its range", parameter, value)
            }
            ConfigError::WanderOutOfRange { value } => write!(
                f,
                "crystal wander of {} ppm/sqrt(s) is outside 0 to {}",
                value, MAX_WANDER_PPM_PER_SQRT_S
            ),
            ConfigError::InvalidServoParameter { parameter, value } => {
                write!(f, "servo {} of {} is not usable", parameter, value)
            }
            ConfigError::TooManySteps { steps, max } => {
                write!(f, "{} steps exceeds the {} step limit", steps, max)
            }
        }
    }
}

impl std::error::Error for ConfigError {}

/// One simulator run, completely described.
///
/// Every field is data. Nothing here reads a clock, so two runs of the same
/// configuration are the same run.
#[derive(Debug, Clone, PartialEq)]
pub struct SimConfig {
    /// Seed for the jitter stream.
    pub seed: u64,
    /// How long the modelled run lasts, in milliseconds of true time.
    pub duration_ms: u64,
    /// How often the playout error is sampled, in milliseconds.
    pub step_ms: u64,
    /// How often the client runs a time sync exchange, in milliseconds.
    pub sync_interval_ms: u64,
    /// Server crystal error, in ppm.
    pub server_ppm: f64,
    /// Client crystal error, in ppm.
    pub client_ppm: f64,
    /// Offset between the two monotonic epochs at the start of the run.
    ///
    /// Any value: two monotonic clocks have no shared origin, so this is what
    /// the client has to discover.
    pub initial_offset_ns: i64,
    /// One-way network delay before any jitter, in microseconds.
    pub base_one_way_delay_us: f64,
    /// Return base delay minus forward base delay, in microseconds.
    ///
    /// Zero is a symmetric path, where the NTP offset estimate is unbiased by
    /// construction. Anything else biases it by half this value, and no
    /// filter can see that: the bias is in the minimum round trip too.
    pub path_asymmetry_us: f64,
    /// How delay above the base is distributed. Both directions draw from the
    /// same model, forward first.
    pub jitter: JitterModel,
    /// Standard deviation of the client crystal's random walk, in ppm per
    /// square root of a second. Zero is a constant-rate crystal.
    pub client_wander_ppm_per_sqrt_s: f64,
    /// Gains and limits of the correction law.
    pub servo: ServoConfig,
}

impl Default for SimConfig {
    fn default() -> SimConfig {
        SimConfig {
            seed: 1,
            duration_ms: 60_000,
            step_ms: 10,
            sync_interval_ms: 1_000,
            server_ppm: 0.0,
            client_ppm: 40.0,
            initial_offset_ns: 12_345_000,
            base_one_way_delay_us: 120.0,
            path_asymmetry_us: 0.0,
            jitter: JitterModel::Uniform { max_us: 60.0 },
            client_wander_ppm_per_sqrt_s: 0.0,
            servo: ServoConfig::default(),
        }
    }
}

impl SimConfig {
    /// Steps this configuration would take.
    pub fn steps(&self) -> u64 {
        if self.step_ms == 0 {
            return 0;
        }
        self.duration_ms / self.step_ms
    }

    /// Check every parameter against its documented range.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.duration_ms == 0 {
            return Err(ConfigError::ZeroDuration);
        }
        if self.step_ms == 0 || self.step_ms > self.duration_ms {
            return Err(ConfigError::InvalidStep {
                step_ms: self.step_ms,
                duration_ms: self.duration_ms,
            });
        }
        if self.sync_interval_ms == 0 {
            return Err(ConfigError::ZeroSyncInterval);
        }
        check_skew("server", self.server_ppm)?;
        check_skew("client", self.client_ppm)?;
        check_delay("base one way delay", self.base_one_way_delay_us)?;
        check_asymmetry(self.base_one_way_delay_us, self.path_asymmetry_us)?;
        check_delay("jitter scale", self.jitter.scale_us())?;
        check_jitter(&self.jitter)?;
        let wander = self.client_wander_ppm_per_sqrt_s;
        if !wander.is_finite() || !(0.0..=MAX_WANDER_PPM_PER_SQRT_S).contains(&wander) {
            return Err(ConfigError::WanderOutOfRange { value: wander });
        }
        check_servo(&self.servo)?;

        let steps = self.steps();
        if steps == 0 {
            return Err(ConfigError::ZeroDuration);
        }
        if steps > MAX_STEPS {
            return Err(ConfigError::TooManySteps {
                steps,
                max: MAX_STEPS,
            });
        }
        Ok(())
    }
}

fn check_skew(clock: &'static str, ppm: f64) -> Result<(), ConfigError> {
    if !ppm.is_finite() || ppm.abs() > MAX_SKEW_PPM {
        return Err(ConfigError::SkewOutOfRange { clock, ppm });
    }
    Ok(())
}

fn check_delay(parameter: &'static str, value_us: f64) -> Result<(), ConfigError> {
    if !value_us.is_finite() || value_us < 0.0 || value_us > MAX_DELAY_US {
        return Err(ConfigError::DelayOutOfRange {
            parameter,
            value_us,
        });
    }
    Ok(())
}

fn check_asymmetry(base_us: f64, asymmetry_us: f64) -> Result<(), ConfigError> {
    let return_us = base_us + asymmetry_us;
    if !asymmetry_us.is_finite() || !(0.0..=MAX_DELAY_US).contains(&return_us) {
        return Err(ConfigError::AsymmetryOutOfRange {
            value_us: asymmetry_us,
        });
    }
    Ok(())
}

fn check_jitter(jitter: &JitterModel) -> Result<(), ConfigError> {
    if let JitterModel::Burst {
        enter_prob,
        exit_prob,
        burst_scale_us,
        burst_shape,
        ..
    } = *jitter
    {
        // A probability of exactly 0 or 1 is a legitimate degenerate model
        // (never bursts, or never leaves one), except that a burst which can
        // never end is not a link, so exit must be above zero.
        if !enter_prob.is_finite() || !(0.0..=1.0).contains(&enter_prob) {
            return Err(ConfigError::InvalidJitterParameter {
                parameter: "burst_enter_prob",
                value: enter_prob,
            });
        }
        if !exit_prob.is_finite() || exit_prob <= 0.0 || exit_prob > 1.0 {
            return Err(ConfigError::InvalidJitterParameter {
                parameter: "burst_exit_prob",
                value: exit_prob,
            });
        }
        if !burst_scale_us.is_finite() || burst_scale_us <= 0.0 || burst_scale_us > MAX_DELAY_US {
            return Err(ConfigError::InvalidJitterParameter {
                parameter: "burst_scale_us",
                value: burst_scale_us,
            });
        }
        // Below 1 the uncapped mean is infinite and the model is only its cap.
        if !burst_shape.is_finite() || burst_shape <= 1.0 || burst_shape > 100.0 {
            return Err(ConfigError::InvalidJitterParameter {
                parameter: "burst_shape",
                value: burst_shape,
            });
        }
    }
    Ok(())
}

fn check_servo(servo: &ServoConfig) -> Result<(), ConfigError> {
    // Zero is a legitimate gain: a servo with both gains at zero is the
    // control case that proves a regression is asserting something.
    let non_negative = [("kp", servo.kp), ("ki", servo.ki)];
    for (parameter, value) in non_negative {
        if !value.is_finite() || value < 0.0 {
            return Err(ConfigError::InvalidServoParameter { parameter, value });
        }
    }
    let positive = [
        ("max_correction_ppm", servo.max_correction_ppm),
        ("hard_resync_threshold_ns", servo.hard_resync_threshold_ns),
    ];
    for (parameter, value) in positive {
        if !value.is_finite() || value <= 0.0 {
            return Err(ConfigError::InvalidServoParameter { parameter, value });
        }
    }
    if !servo.smoothing_alpha.is_finite()
        || servo.smoothing_alpha <= 0.0
        || servo.smoothing_alpha > 1.0
    {
        return Err(ConfigError::InvalidServoParameter {
            parameter: "smoothing_alpha",
            value: servo.smoothing_alpha,
        });
    }
    if servo.filter_window == 0 {
        return Err(ConfigError::InvalidServoParameter {
            parameter: "filter_window",
            value: 0.0,
        });
    }
    Ok(())
}
