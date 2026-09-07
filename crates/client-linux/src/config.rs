//! What a client run is configured with, and which configurations are refused.
//!
//! The three quantities that matter are the minimum, the maximum and the start
//! fill. They are not tuning knobs with independent lives: three relations
//! hold between them, they are checked here at start, and they are written to
//! the delay log so that a run can be graded by someone who did not set them.
//!
//! - The minimum is greater than zero. A minimum of zero would make "inside
//!   the bounds" true of an empty buffer, which is the state an underrun comes
//!   from.
//! - The start fill lies strictly between the two bounds. Starting outside the
//!   band the run is graded against would make the first sample a failure or
//!   make the ceiling unreachable.
//! - The span, divided by the deliberate rate difference the overflow test
//!   applies, is under 600 seconds. Bounds so wide that no run could cross them
//!   would make the crossing behaviour untestable and would leave the delay
//!   assertion carried entirely by the underrun count.

use std::fmt;

use crate::sync::SyncConfig;

/// The deliberate rate difference the overflow verification applies, in parts
/// per million.
///
/// Committed in `config/verification.conf` and repeated here as the default so
/// that the relation below is checked even when the file is not read. The two
/// are asserted equal by the verification suite.
pub const DEFAULT_OVERFLOW_SKEW_PPM: u64 = 2_000;

/// A crossing test has to finish inside one ten-minute run.
pub const MAX_SECONDS_TO_CROSS: u64 = 600;

/// How long `--discover` browses for a server before falling back.
///
/// RFC 6762 section 5.2 has a responder answer a query "immediately" on the
/// link, subject to the randomised 20 to 120 ms delay section 6 requires of a
/// SHARED record, which a DNS-SD service's PTR is. A second is many times that
/// and is short enough that an endpoint whose link carries no multicast at all
/// is playing from its static address a second after it started rather than
/// after a timeout somebody has to wait through.
pub const DEFAULT_DISCOVER_MS: u64 = 1_000;

/// How a client run is configured.
#[derive(Debug, Clone, PartialEq)]
pub struct ClientConfig {
    /// Where the server is.
    pub server: String,
    /// ALSA device name.
    pub device: String,
    /// Smallest buffer occupancy the run is graded against, in microseconds.
    pub min_us: u64,
    /// Largest buffer occupancy the run is graded against, in microseconds.
    pub max_us: u64,
    /// Buffered audio to accumulate before the first frame is written.
    pub start_fill_us: u64,
    /// The device-reported delay the playout loop holds.
    pub device_target_us: u64,
    /// Where the delay log is written.
    pub delay_log: String,
    /// How long to run before exiting cleanly, in seconds. `None` runs until
    /// the stream ends or the connection is lost.
    pub run_seconds: Option<u64>,
    /// The rate difference the overflow verification applies, in ppm, which
    /// the third relation is checked against.
    pub overflow_skew_ppm: u64,
    /// In probe mode, whether a device that reports no delay counts as
    /// unusable.
    ///
    /// It is a real distinction. The ALSA `null` device opens, accepts every
    /// frame instantly and reports a delay of zero forever. It is a perfectly
    /// good device for asking "can this be opened", and it is no use at all
    /// for verifying anything about the delay a device reports, because it
    /// has no ring to report about.
    pub require_pacing: bool,
    /// The sync loop's cadence, window, thresholds and clamp.
    ///
    /// Committed in `config/sync.conf` and passed in from there by the
    /// verification entry points, so a check and the thing it checks cannot
    /// drift apart.
    pub sync: SyncConfig,
    /// Whether `--server` was given at all.
    ///
    /// The distinction matters: a client with no server address and no
    /// discovery has to say which of the two it lacks, and it cannot say that
    /// from a field that has a default in it.
    pub server_configured: bool,
    /// How long to browse for a server by multicast DNS before falling back,
    /// or `None` to browse not at all.
    pub discover_ms: Option<u64>,
    /// The control channel to subscribe to, or `None` for an endpoint that
    /// plays at full scale and is told nothing.
    pub control: Option<String>,
    /// The zone this endpoint plays.
    pub zone: String,
    /// This endpoint's identifier, which is what the zone's membership records.
    pub endpoint: String,
    /// Whether to reconnect and carry on when a session ends unexpectedly.
    ///
    /// Off by default, because every verification this repository already has
    /// grades a SINGLE session and reads its exit code. An endpoint in a house
    /// is started with it on.
    pub rejoin: bool,
    /// Longest to wait between rejoin attempts, in milliseconds.
    pub rejoin_max_ms: u64,
}

impl Default for ClientConfig {
    fn default() -> Self {
        ClientConfig {
            server: "127.0.0.1:4010".to_string(),
            device: "default".to_string(),
            min_us: 60_000,
            max_us: 300_000,
            start_fill_us: 120_000,
            device_target_us: 120_000,
            delay_log: "chorus-delay.log".to_string(),
            run_seconds: None,
            overflow_skew_ppm: DEFAULT_OVERFLOW_SKEW_PPM,
            require_pacing: false,
            sync: SyncConfig::default(),
            server_configured: false,
            discover_ms: None,
            control: None,
            zone: "default".to_string(),
            endpoint: "endpoint".to_string(),
            rejoin: false,
            rejoin_max_ms: 2_000,
        }
    }
}

/// Why a client configuration was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// The minimum bound is zero.
    MinimumIsZero,
    /// The bounds are the wrong way round, or equal.
    BoundsNotOrdered {
        /// The minimum that was configured.
        min_us: u64,
        /// The maximum that was configured.
        max_us: u64,
    },
    /// The start fill is not strictly between the bounds.
    StartFillOutsideBounds {
        /// The start fill that was configured.
        start_fill_us: u64,
        /// The minimum that was configured.
        min_us: u64,
        /// The maximum that was configured.
        max_us: u64,
    },
    /// The device target is not strictly between the bounds.
    DeviceTargetOutsideBounds {
        /// The target that was configured.
        device_target_us: u64,
        /// The minimum that was configured.
        min_us: u64,
        /// The maximum that was configured.
        max_us: u64,
    },
    /// The fixed playout latency is not strictly between the device target and
    /// the maximum bound.
    PlayoutLatencyOutsideBounds {
        /// The latency that was configured, in microseconds.
        playout_latency_us: u64,
        /// The device delay target it has to sit above.
        device_target_us: u64,
        /// The maximum bound it has to sit below.
        max_us: u64,
    },
    /// The span is so wide that the overflow test could not cross it inside a
    /// ten-minute run.
    SpanNotCrossableInTenMinutes {
        /// Maximum minus minimum, in microseconds.
        span_us: u64,
        /// The rate difference the crossing test applies, in ppm.
        skew_ppm: u64,
        /// Seconds the crossing would take.
        seconds_to_cross: u64,
        /// The bound it has to be under.
        limit_seconds: u64,
    },
    /// The rate difference is zero, so no crossing would ever happen.
    SkewIsZero,
    /// An argument this binary does not have.
    UnknownArgument {
        /// The argument as it was given.
        argument: String,
    },
    /// An argument that needs a value and did not get one.
    MissingValue {
        /// The argument as it was given.
        argument: String,
    },
    /// A value that is not a number.
    NotANumber {
        /// The argument as it was given.
        argument: String,
        /// The value as it was given.
        value: String,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::MinimumIsZero => write!(
                f,
                "the minimum buffer bound is 0 us; a bound of zero makes an empty buffer 'inside \
                 the bounds', which is the state an underrun comes from"
            ),
            ConfigError::BoundsNotOrdered { min_us, max_us } => write!(
                f,
                "the buffer bounds are {} us to {} us; the maximum has to be above the minimum",
                min_us, max_us
            ),
            ConfigError::StartFillOutsideBounds {
                start_fill_us,
                min_us,
                max_us,
            } => write!(
                f,
                "the start fill {} us is not strictly between the bounds {} us and {} us",
                start_fill_us, min_us, max_us
            ),
            ConfigError::DeviceTargetOutsideBounds {
                device_target_us,
                min_us,
                max_us,
            } => write!(
                f,
                "the device delay target {} us is not strictly between the bounds {} us and {} us",
                device_target_us, min_us, max_us
            ),
            ConfigError::PlayoutLatencyOutsideBounds {
                playout_latency_us,
                device_target_us,
                max_us,
            } => write!(
                f,
                "the playout latency {} us is not strictly between the device delay target {} us \
                 and the maximum bound {} us; below the target there is no queue left to hold the \
                 difference, and at or above the maximum the buffer is discarding what the loop \
                 is waiting for",
                playout_latency_us, device_target_us, max_us
            ),
            ConfigError::SpanNotCrossableInTenMinutes {
                span_us,
                skew_ppm,
                seconds_to_cross,
                limit_seconds,
            } => write!(
                f,
                "a span of {} us crossed at {} ppm takes {} s, which is not under the {} s a \
                 ten-minute run allows; bounds no run could cross are bounds nothing is graded \
                 against",
                span_us, skew_ppm, seconds_to_cross, limit_seconds
            ),
            ConfigError::SkewIsZero => write!(
                f,
                "the overflow rate difference is 0 ppm, so no crossing would ever happen"
            ),
            ConfigError::UnknownArgument { argument } => {
                write!(f, "unknown argument '{}'", argument)
            }
            ConfigError::MissingValue { argument } => {
                write!(f, "argument '{}' needs a value", argument)
            }
            ConfigError::NotANumber { argument, value } => {
                write!(f, "argument '{}' got '{}', which is not a number", argument, value)
            }
        }
    }
}

impl std::error::Error for ConfigError {}

impl ClientConfig {
    /// Check the three relations that keep the bounds meaningful.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.min_us == 0 {
            return Err(ConfigError::MinimumIsZero);
        }
        if self.max_us <= self.min_us {
            return Err(ConfigError::BoundsNotOrdered {
                min_us: self.min_us,
                max_us: self.max_us,
            });
        }
        if self.start_fill_us <= self.min_us || self.start_fill_us >= self.max_us {
            return Err(ConfigError::StartFillOutsideBounds {
                start_fill_us: self.start_fill_us,
                min_us: self.min_us,
                max_us: self.max_us,
            });
        }
        if self.device_target_us <= self.min_us || self.device_target_us >= self.max_us {
            return Err(ConfigError::DeviceTargetOutsideBounds {
                device_target_us: self.device_target_us,
                min_us: self.min_us,
                max_us: self.max_us,
            });
        }
        let playout_latency_us = self.sync.playout_latency_ns / 1_000;
        if playout_latency_us <= self.device_target_us || playout_latency_us >= self.max_us {
            return Err(ConfigError::PlayoutLatencyOutsideBounds {
                playout_latency_us,
                device_target_us: self.device_target_us,
                max_us: self.max_us,
            });
        }
        if self.overflow_skew_ppm == 0 {
            return Err(ConfigError::SkewIsZero);
        }
        let seconds = self.seconds_to_cross();
        if seconds >= MAX_SECONDS_TO_CROSS {
            return Err(ConfigError::SpanNotCrossableInTenMinutes {
                span_us: self.span_us(),
                skew_ppm: self.overflow_skew_ppm,
                seconds_to_cross: seconds,
                limit_seconds: MAX_SECONDS_TO_CROSS,
            });
        }
        Ok(())
    }

    /// Maximum minus minimum, in microseconds.
    pub fn span_us(&self) -> u64 {
        self.max_us.saturating_sub(self.min_us)
    }

    /// How long the overflow test takes to drive occupancy across the span,
    /// in seconds.
    ///
    /// A source running `skew_ppm` faster than the sink adds `skew_ppm`
    /// microseconds of audio per second of real time, so the span divided by
    /// that is the time to cross it.
    pub fn seconds_to_cross(&self) -> u64 {
        if self.overflow_skew_ppm == 0 {
            return u64::MAX;
        }
        self.span_us().div_ceil(self.overflow_skew_ppm)
    }

    /// The device ring to ask ALSA for: above the configured maximum, so that
    /// the ring is never what caps the reported delay.
    pub fn device_buffer_us(&self) -> u64 {
        self.max_us + self.max_us / 8 + 20_000
    }

    /// Parse a command line, leaving anything unrecognised as an error rather
    /// than a default.
    pub fn from_args<I: IntoIterator<Item = String>>(
        args: I,
    ) -> Result<(ClientConfig, ClientMode), ConfigError> {
        let mut config = ClientConfig::default();
        let mut mode = ClientMode::Play;
        let mut it = args.into_iter().peekable();
        while let Some(arg) = it.next() {
            let mut value = || -> Result<String, ConfigError> {
                it.next().ok_or_else(|| ConfigError::MissingValue {
                    argument: arg.clone(),
                })
            };
            match arg.as_str() {
                "--probe-device" => mode = ClientMode::ProbeDevice,
                "--require-pacing" => config.require_pacing = true,
                "--server" => {
                    config.server = value()?;
                    config.server_configured = true;
                }
                "--no-server" => {
                    // The only way to say "I have no static address" on a
                    // command line where one has a default. It exists so that
                    // the discovery-and-fallback behaviour can be exercised
                    // without editing the default out of the source.
                    config.server_configured = false;
                }
                "--discover" => config.discover_ms = Some(DEFAULT_DISCOVER_MS),
                "--discover-ms" => config.discover_ms = Some(number(&arg, &value()?)?),
                "--control" => config.control = Some(value()?),
                "--zone" => config.zone = value()?,
                "--endpoint" => config.endpoint = value()?,
                "--rejoin" => config.rejoin = true,
                "--rejoin-max-ms" => config.rejoin_max_ms = number(&arg, &value()?)?,
                "--device" => config.device = value()?,
                "--delay-log" => config.delay_log = value()?,
                "--min-us" => config.min_us = number(&arg, &value()?)?,
                "--max-us" => config.max_us = number(&arg, &value()?)?,
                "--start-fill-us" => config.start_fill_us = number(&arg, &value()?)?,
                "--device-target-us" => config.device_target_us = number(&arg, &value()?)?,
                "--overflow-skew-ppm" => config.overflow_skew_ppm = number(&arg, &value()?)?,
                "--run-seconds" => config.run_seconds = Some(number(&arg, &value()?)?),
                "--sync-interval-ms" => config.sync.interval_ms = number(&arg, &value()?)?,
                "--filter-window" => {
                    config.sync.filter_window = number(&arg, &value()?)? as usize
                }
                "--smoothing-alpha" => config.sync.smoothing_alpha = decimal(&arg, &value()?)?,
                "--hard-resync-threshold-us" => {
                    config.sync.hard_resync_threshold_ns =
                        number(&arg, &value()?)? as f64 * 1_000.0
                }
                "--max-correction-ppm" => {
                    config.sync.max_correction_ppm = decimal(&arg, &value()?)?
                }
                "--staleness-limit-ms" => {
                    config.sync.staleness_limit_ns = number(&arg, &value()?)? * 1_000_000
                }
                "--max-rtt-us" => config.sync.max_rtt_ns = number(&arg, &value()?)? * 1_000,
                "--playout-latency-us" => {
                    config.sync.playout_latency_ns = number(&arg, &value()?)? * 1_000
                }
                "--mute-us" => config.sync.mute_ns = number(&arg, &value()?)? * 1_000,
                other => {
                    return Err(ConfigError::UnknownArgument {
                        argument: other.to_string(),
                    })
                }
            }
        }
        Ok((config, mode))
    }
}

fn number(argument: &str, value: &str) -> Result<u64, ConfigError> {
    value.parse().map_err(|_| ConfigError::NotANumber {
        argument: argument.to_string(),
        value: value.to_string(),
    })
}

/// A finite decimal. `NaN` and the infinities parse as `f64` and are not
/// numbers this configuration can mean anything with, so they are refused
/// here rather than turned into a servo that never corrects.
fn decimal(argument: &str, value: &str) -> Result<f64, ConfigError> {
    match value.parse::<f64>() {
        Ok(v) if v.is_finite() => Ok(v),
        _ => Err(ConfigError::NotANumber {
            argument: argument.to_string(),
            value: value.to_string(),
        }),
    }
}

/// What this invocation is for.
///
/// There is deliberately no mode that plays into nothing: the only two are
/// "play through the configured device" and "say whether the configured device
/// can be opened at all", and the second plays nothing and claims nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientMode {
    /// Connect, buffer and play.
    Play,
    /// Open the configured device, report, close, exit.
    ProbeDevice,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_committed_defaults_satisfy_all_three_relations() {
        let c = ClientConfig::default();
        c.validate().expect("the shipped defaults are valid");
        assert!(c.min_us > 0);
        assert!(c.start_fill_us > c.min_us && c.start_fill_us < c.max_us);
        assert!(c.seconds_to_cross() < MAX_SECONDS_TO_CROSS);
        // 240000 us of span at 2000 ppm is 120 s.
        assert_eq!(c.span_us(), 240_000);
        assert_eq!(c.seconds_to_cross(), 120);
    }

    #[test]
    fn a_zero_minimum_is_refused() {
        let c = ClientConfig {
            min_us: 0,
            ..Default::default()
        };
        assert_eq!(c.validate(), Err(ConfigError::MinimumIsZero));
    }

    #[test]
    fn a_start_fill_on_a_bound_is_refused_because_strictly_between_means_strictly() {
        for fill in [60_000u64, 300_000] {
            let c = ClientConfig {
                start_fill_us: fill,
                device_target_us: 120_000,
                ..Default::default()
            };
            assert!(matches!(
                c.validate(),
                Err(ConfigError::StartFillOutsideBounds { .. })
            ));
        }
    }

    #[test]
    fn bounds_no_run_could_cross_are_refused() {
        let c = ClientConfig {
            min_us: 60_000,
            max_us: 60_000_000,
            start_fill_us: 120_000,
            device_target_us: 120_000,
            ..Default::default()
        };
        match c.validate() {
            Err(ConfigError::SpanNotCrossableInTenMinutes {
                seconds_to_cross, ..
            }) => assert!(seconds_to_cross >= 600),
            other => panic!("expected a span refusal, got {:?}", other),
        }
    }

    #[test]
    fn an_unknown_argument_is_an_error_and_not_a_default() {
        let err = ClientConfig::from_args(["--sink".to_string(), "none".to_string()]).unwrap_err();
        assert_eq!(
            err,
            ConfigError::UnknownArgument {
                argument: "--sink".to_string()
            }
        );
    }

    #[test]
    fn there_is_no_argument_that_selects_a_sink() {
        // The only two modes are play and probe. A run either plays through
        // the configured ALSA device or exits; nothing on the command line can
        // substitute something else for it.
        let (_, mode) = ClientConfig::from_args(Vec::<String>::new()).unwrap();
        assert_eq!(mode, ClientMode::Play);
        let (_, mode) = ClientConfig::from_args(["--probe-device".to_string()]).unwrap();
        assert_eq!(mode, ClientMode::ProbeDevice);
        for candidate in ["--sink", "--null-sink", "--dry-run", "--no-audio", "--fake"] {
            assert!(
                ClientConfig::from_args([candidate.to_string()]).is_err(),
                "{} must not be accepted",
                candidate
            );
        }
    }

    #[test]
    fn the_device_ring_asked_for_is_above_the_configured_maximum() {
        let c = ClientConfig::default();
        assert!(c.device_buffer_us() > c.max_us);
    }
}
