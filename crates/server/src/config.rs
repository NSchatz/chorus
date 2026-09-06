//! What a server run is configured with.
//!
//! Everything the server needs to know is here, and nothing is guessed. In
//! particular a stream format is refused at start, by name, rather than being
//! interpreted under an assumption: bytes read under the wrong format still
//! produce sound, which is exactly what makes guessing dangerous.

use std::fmt;

use chorus_audio::UnsupportedFormat;

/// How a server run is configured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    /// Address to listen on.
    pub listen: String,
    /// Where the PCM comes from: a file path, or `tone` for a generated one.
    pub source: String,
    /// Frames per second.
    pub sample_rate_hz: u32,
    /// Channels per frame.
    pub channels: u32,
    /// Sample layout, by protocol name.
    pub sample_format: String,
    /// Chunk duration, microseconds.
    pub chunk_us: u64,
    /// How much faster than real time chunks are emitted, in parts per
    /// million. Zero is the ordinary case; the overflow verification sets it
    /// deliberately so that a client's buffer climbs to its ceiling inside a
    /// bounded run.
    pub rate_skew_ppm: u64,
    /// Length of a generated tone, milliseconds. Zero means endless.
    pub tone_ms: u64,
    /// Real-time priority to ask for, clamped to the granted ceiling.
    pub rt_priority: u32,
    /// CPU-time bound applied to every real-time thread, microseconds.
    pub rttime_us: u64,
    /// Start without a real-time policy when the host grants none.
    pub allow_non_realtime: bool,
    /// Whether to attempt to lock audio-path memory.
    pub lock_memory: bool,
    /// How much locked memory the server wants, bytes.
    pub memlock_wanted_bytes: u64,
    /// Start unlocked when the host denies locking.
    pub allow_unlocked_memory: bool,
    /// Serve one client and exit, rather than serving one client after
    /// another.
    pub once: bool,
    /// How many clients may be attached at once.
    ///
    /// A ceiling rather than a target. Two threads exist for each of these
    /// from the moment the process starts, so this number is also what fixes
    /// the process's thread population: a client is served by a thread that
    /// already existed when the scheduling report was taken, or it is refused.
    pub max_clients: usize,
    /// Where the control channel listens, or `None` for a server with no
    /// control plane at all.
    ///
    /// Opt-in rather than defaulted, and
    /// `docs/decisions/0016-the-control-catalog.md` records why: a fixed
    /// default port would be bound by several of this repository's own
    /// verification runs at once, and `deploy/` is not this phase's to change.
    pub control_listen: Option<String>,
    /// How many control connections may be served at once.
    ///
    /// A ceiling, and the same argument `max_clients` makes: one thread exists
    /// for each of these from the moment the process starts, so this number is
    /// part of what fixes the thread population.
    pub control_workers: usize,
    /// Where the zone state is persisted, or `None` for a run that keeps
    /// nothing across a restart.
    pub state_file: Option<String>,
    /// The zones this server has, in the order they were configured.
    ///
    /// Read only when there is no persisted state to load; see
    /// `docs/decisions/0018-the-persisted-zone-state.md`.
    pub zones: Vec<String>,
    /// Where each group's audio stream is served, as `group=address`.
    pub group_audio: Vec<(String, String)>,
    /// Whether to advertise this server by multicast DNS.
    pub advertise: bool,
    /// The DNS-SD instance label this server advertises under.
    pub instance: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            listen: "127.0.0.1:4010".to_string(),
            source: "tone".to_string(),
            sample_rate_hz: 48_000,
            channels: 2,
            sample_format: "pcm_s16le".to_string(),
            chunk_us: 20_000,
            rate_skew_ppm: 0,
            tone_ms: 0,
            rt_priority: 20,
            rttime_us: 200_000,
            allow_non_realtime: false,
            lock_memory: true,
            memlock_wanted_bytes: 64 * 1024 * 1024,
            allow_unlocked_memory: false,
            once: true,
            max_clients: 4,
            control_listen: None,
            control_workers: 8,
            state_file: None,
            zones: Vec::new(),
            group_audio: Vec::new(),
            advertise: false,
            instance: "chorus".to_string(),
        }
    }
}

/// Why a server configuration was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerConfigError {
    /// The stream format is not one this server carries.
    Format(UnsupportedFormat),
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
    /// A CPU-time bound of zero would kill a real-time thread immediately.
    RtTimeIsZero,
    /// A ceiling of zero clients would refuse every connection.
    NoClientsAllowed,
    /// A control channel with no worker would refuse every connection.
    NoControlWorkersAllowed,
    /// A `--group-audio` argument that is not `group=address`.
    NotAGroupAddress {
        /// The value as it was given.
        value: String,
    },
    /// A zone identifier the catalog does not allow.
    NotAZone {
        /// The value as it was given.
        value: String,
    },
}

impl fmt::Display for ServerConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServerConfigError::Format(e) => write!(f, "{}", e),
            ServerConfigError::UnknownArgument { argument } => {
                write!(f, "unknown argument '{}'", argument)
            }
            ServerConfigError::MissingValue { argument } => {
                write!(f, "argument '{}' needs a value", argument)
            }
            ServerConfigError::NotANumber { argument, value } => write!(
                f,
                "argument '{}' got '{}', which is not a number",
                argument, value
            ),
            ServerConfigError::RtTimeIsZero => write!(
                f,
                "a CPU-time bound of 0 us would terminate a real-time thread the instant it \
                 started"
            ),
            ServerConfigError::NoClientsAllowed => write!(
                f,
                "a ceiling of 0 clients would bind a socket and refuse every connection that \
                 arrived on it"
            ),
            ServerConfigError::NoControlWorkersAllowed => write!(
                f,
                "a control channel with 0 workers would bind a socket and refuse every \
                 connection that arrived on it"
            ),
            ServerConfigError::NotAGroupAddress { value } => write!(
                f,
                "'{}' is not 'group=address'; --group-audio says where one group's audio stream \
                 is served, for example --group-audio downstairs=127.0.0.1:4011",
                value
            ),
            ServerConfigError::NotAZone { value } => write!(
                f,
                "'{}' is not a zone identifier; the control catalog declares 1 to {} characters \
                 of lower-case letters, digits and hyphens",
                value,
                chorus_control::catalog::MAX_IDENTIFIER_LEN
            ),
        }
    }
}

impl std::error::Error for ServerConfigError {}

impl From<UnsupportedFormat> for ServerConfigError {
    fn from(e: UnsupportedFormat) -> ServerConfigError {
        ServerConfigError::Format(e)
    }
}

impl ServerConfig {
    /// Parse a command line, leaving anything unrecognised as an error rather
    /// than a default.
    pub fn from_args<I: IntoIterator<Item = String>>(
        args: I,
    ) -> Result<ServerConfig, ServerConfigError> {
        let mut config = ServerConfig::default();
        let mut it = args.into_iter();
        while let Some(arg) = it.next() {
            let mut value = || -> Result<String, ServerConfigError> {
                it.next().ok_or_else(|| ServerConfigError::MissingValue {
                    argument: arg.clone(),
                })
            };
            match arg.as_str() {
                "--listen" => config.listen = value()?,
                "--source" => config.source = value()?,
                "--format" => config.sample_format = value()?,
                "--rate" => config.sample_rate_hz = number(&arg, &value()?)? as u32,
                "--channels" => config.channels = number(&arg, &value()?)? as u32,
                "--chunk-us" => config.chunk_us = number(&arg, &value()?)?,
                "--rate-skew-ppm" => config.rate_skew_ppm = number(&arg, &value()?)?,
                "--tone-ms" => config.tone_ms = number(&arg, &value()?)?,
                "--rt-priority" => config.rt_priority = number(&arg, &value()?)? as u32,
                "--rttime-us" => config.rttime_us = number(&arg, &value()?)?,
                "--allow-non-realtime" => config.allow_non_realtime = true,
                "--no-lock-memory" => config.lock_memory = false,
                "--memlock-wanted-bytes" => config.memlock_wanted_bytes = number(&arg, &value()?)?,
                "--allow-unlocked-memory" => config.allow_unlocked_memory = true,
                "--serve-forever" => config.once = false,
                "--max-clients" => config.max_clients = number(&arg, &value()?)? as usize,
                "--control-listen" => config.control_listen = Some(value()?),
                "--control-workers" => config.control_workers = number(&arg, &value()?)? as usize,
                "--state-file" => config.state_file = Some(value()?),
                "--zone" => {
                    let id = value()?;
                    if !chorus_control::catalog::is_identifier(&id) {
                        return Err(ServerConfigError::NotAZone { value: id });
                    }
                    config.zones.push(id);
                }
                "--group-audio" => {
                    let pair = value()?;
                    let (group, address) = pair
                        .split_once('=')
                        .ok_or_else(|| ServerConfigError::NotAGroupAddress {
                            value: pair.clone(),
                        })?;
                    if !chorus_control::catalog::is_identifier(group) || address.is_empty() {
                        return Err(ServerConfigError::NotAGroupAddress {
                            value: pair.clone(),
                        });
                    }
                    config
                        .group_audio
                        .push((group.to_string(), address.to_string()));
                }
                "--advertise" => config.advertise = true,
                "--instance" => config.instance = value()?,
                other => {
                    return Err(ServerConfigError::UnknownArgument {
                        argument: other.to_string(),
                    })
                }
            }
        }
        if config.rttime_us == 0 {
            return Err(ServerConfigError::RtTimeIsZero);
        }
        if config.max_clients == 0 {
            return Err(ServerConfigError::NoClientsAllowed);
        }
        if config.control_listen.is_some() && config.control_workers == 0 {
            return Err(ServerConfigError::NoControlWorkersAllowed);
        }
        Ok(config)
    }
}

fn number(argument: &str, value: &str) -> Result<u64, ServerConfigError> {
    value.parse().map_err(|_| ServerConfigError::NotANumber {
        argument: argument.to_string(),
        value: value.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_argument_is_an_error_and_not_a_default() {
        let err =
            ServerConfig::from_args(["--pretend".to_string(), "yes".to_string()]).unwrap_err();
        assert_eq!(
            err,
            ServerConfigError::UnknownArgument {
                argument: "--pretend".to_string()
            }
        );
    }

    #[test]
    fn the_defaults_are_the_documented_ones() {
        let c = ServerConfig::default();
        assert_eq!(c.chunk_us, 20_000);
        assert_eq!(c.sample_rate_hz, 48_000);
        assert_eq!(c.channels, 2);
        assert_eq!(c.sample_format, "pcm_s16le");
        assert_eq!(c.rate_skew_ppm, 0);
        assert!(c.lock_memory);
        assert!(!c.allow_non_realtime);
        assert!(!c.allow_unlocked_memory);
    }

    #[test]
    fn a_zero_cpu_time_bound_is_refused() {
        let err =
            ServerConfig::from_args(["--rttime-us".to_string(), "0".to_string()]).unwrap_err();
        assert_eq!(err, ServerConfigError::RtTimeIsZero);
    }

    #[test]
    fn a_client_ceiling_of_zero_is_refused_rather_than_bound_to_a_socket() {
        let err =
            ServerConfig::from_args(["--max-clients".to_string(), "0".to_string()]).unwrap_err();
        assert_eq!(err, ServerConfigError::NoClientsAllowed);
    }

    #[test]
    fn the_control_plane_is_off_unless_an_address_is_given_for_it() {
        assert_eq!(ServerConfig::default().control_listen, None);
        let c = ServerConfig::from_args([
            "--control-listen".to_string(),
            "127.0.0.1:0".to_string(),
        ])
        .unwrap();
        assert_eq!(c.control_listen.as_deref(), Some("127.0.0.1:0"));
        assert_eq!(c.control_workers, 8, "docs/decisions/0016 records why 8");
    }

    #[test]
    fn a_control_channel_with_no_worker_is_refused_rather_than_bound() {
        let err = ServerConfig::from_args([
            "--control-listen".to_string(),
            "127.0.0.1:0".to_string(),
            "--control-workers".to_string(),
            "0".to_string(),
        ])
        .unwrap_err();
        assert_eq!(err, ServerConfigError::NoControlWorkersAllowed);
        // With no control channel the number is not consulted at all, so it is
        // not a reason to refuse a configuration that never uses it.
        assert!(
            ServerConfig::from_args(["--control-workers".to_string(), "0".to_string()]).is_ok()
        );
    }

    #[test]
    fn a_zone_or_a_group_address_that_is_not_one_is_refused_at_start() {
        let err =
            ServerConfig::from_args(["--zone".to_string(), "Kitchen Zone".to_string()]).unwrap_err();
        assert!(matches!(err, ServerConfigError::NotAZone { .. }), "{:?}", err);
        let err = ServerConfig::from_args([
            "--group-audio".to_string(),
            "downstairs".to_string(),
        ])
        .unwrap_err();
        assert!(
            matches!(err, ServerConfigError::NotAGroupAddress { .. }),
            "{:?}",
            err
        );
        let c = ServerConfig::from_args([
            "--zone".to_string(),
            "kitchen".to_string(),
            "--zone".to_string(),
            "study".to_string(),
            "--group-audio".to_string(),
            "downstairs=127.0.0.1:4011".to_string(),
        ])
        .unwrap();
        assert_eq!(c.zones, vec!["kitchen".to_string(), "study".to_string()]);
        assert_eq!(
            c.group_audio,
            vec![("downstairs".to_string(), "127.0.0.1:4011".to_string())]
        );
    }

    #[test]
    fn the_client_ceiling_is_what_fixes_the_thread_population() {
        let c = ServerConfig::default();
        assert_eq!(c.max_clients, 4, "docs/decisions/0014 records why 4");
        let c = ServerConfig::from_args(["--max-clients".to_string(), "2".to_string()]).unwrap();
        assert_eq!(c.max_clients, 2);
    }
}
