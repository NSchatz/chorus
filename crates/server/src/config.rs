//! What a server run is configured with.
//!
//! Everything the server needs to know is here, and nothing is guessed. In
//! particular a stream format is refused at start, by name, rather than being
//! interpreted under an assumption: bytes read under the wrong format still
//! produce sound, which is exactly what makes guessing dangerous.

use std::fmt;

use chorus_audio::UnsupportedFormat;
use chorus_control::rooms::{CivilTime, ClockTime, DAY_NAMES};
use chorus_control::transport::{Transport, DEFAULT_TRANSPORT};

/// The most stream slots one server serves. ASSUMED: a large house's rooms
/// with room to spare (a slot costs one broadcast per chunk on the audio
/// thread, never a thread); not a measured bound.
pub const MAX_SLOTS: usize = 32;

/// How a server run is configured.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerConfig {
    /// Address to listen on.
    pub listen: String,
    /// Where the PCM comes from: a file path, `fifo:<path>` (or a path that is
    /// a named pipe) for a pipe held open across writers, `tone` for a
    /// generated 440 Hz tone, or `chirp` for the measurement rig's chirp.
    pub source: String,
    /// Where the rig's declared values are read from for `--source chirp`:
    /// the band, the period and the amplitude ceiling.
    pub measure_config: String,
    /// The chirp amplitude, in full-scale units, or `None` for the ceiling
    /// `measure_config` declares. Above the ceiling is refused at start.
    pub chirp_amplitude: Option<f64>,
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
    /// Length of a generated tone or chirp, milliseconds. Zero means endless.
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
    /// The transport each zone was declared with, in the same order.
    ///
    /// From `--zone <id>=<transport>`. A zone declared with no transport is
    /// wired, which is `chorus_control::transport::DEFAULT_TRANSPORT` and is
    /// recorded here explicitly so that no zone's tier is implicit: a reader of
    /// this list never has to know the default to know what a zone is.
    ///
    /// The transport is NOT part of the persisted state and NOT part of the
    /// control catalog. It is declared where the set of zones is declared,
    /// because `docs/control-plane.md` says that set is configured and not
    /// commanded, and which wire a room is on is the same kind of fact.
    pub zone_transports: Vec<(String, Transport)>,
    /// Where each group's audio stream is served, as `group=address`.
    pub group_audio: Vec<(String, String)>,
    /// Whether to advertise this server by multicast DNS.
    pub advertise: bool,
    /// The DNS-SD instance label this server advertises under.
    pub instance: String,
    /// Where the server's long-term key (`server.key`) and its adopted
    /// endpoints (`adopted-endpoints`) live. `None` uses the directory of
    /// `state_file`; with neither, the server refuses to start unless
    /// `ephemeral_identity` is set.
    pub identity_dir: Option<String>,
    /// (goal 14) Where firmware images are staged (`<name>.bin` and
    /// `<name>.manifest`; `crate::firmware`). `None`: nothing is staged and
    /// nothing can be installed. Needs the control plane, whose
    /// `firmware_install` is the only thing that starts a transfer.
    pub firmware_dir: Option<String>,
    /// The id this server presents in the protocol v2 handshake.
    pub server_id: String,
    /// Use a key made for this process alone and keep adoptions in memory:
    /// for tests and throwaway runs, never for a house.
    pub ephemeral_identity: bool,
    /// Stream slots (goal 11): how many groups' streams this one process
    /// cuts at once on its one audio thread, each session routed to its
    /// group's inside its own session. `0` is the one-stream shape every run
    /// before goal 11 had, byte for byte. Needs the control plane (the room
    /// model is what routes), and is not combined with `--group-audio`.
    pub slots: usize,
    /// How many `GET /api/events` streams the event writer holds at once.
    pub event_streams: usize,
    /// Hold the schedule's civil clock at this weekday and time for the run
    /// (`--civil-time mon-23:30`), which decides the quiet hours and when an
    /// alarm rings. For tests and a server with no time source. `None` runs
    /// the civil clock from the host's wall clock (or `--civil-time-from`).
    pub civil_time: Option<CivilTime>,
    /// The time zone file the schedule keeps civil time in (`--tz <path>`, a
    /// TZif file). `None`: `$TZ`, then `/etc/localtime`, else UTC, decided at
    /// start (`main.rs`).
    pub tz: Option<String>,
    /// Run the civil clock from this UTC instant, seconds since the epoch,
    /// plus the schedule time elapsed since start (`--civil-time-from
    /// 2026-10-05T06:59:50Z`). For tests: alarms ring at a civil time the
    /// test chose, on whatever day the test runs.
    pub civil_time_from: Option<i64>,
    /// Run the schedule's durations this many times faster (ramps, fades,
    /// holds, sleep timers, and the civil clock `--civil-time-from` runs).
    /// For tests only; the audio thread's pace is never scaled. 1 is real
    /// time.
    pub schedule_time_scale: u32,
    /// (goal 13) The TV relay's UDP port (`crate::tvrelay`), bound on the
    /// audio listener's address. `None`: the audio port plus one (ASSUMED),
    /// so an ephemeral audio port (`--listen <addr>:0`, the tests') gives an
    /// ephemeral UDP port too; `Some(0)` asks for an ephemeral one.
    pub low_latency_port: Option<u16>,
    /// (goal 13) `L_tv`, the TV relay's stamp lead before the room's A/V trim,
    /// in ns; `None` is `chorus_protocol::v2::lowlat::DEFAULTS.l_tv_ns`. A
    /// value outside the plan's range or below its floor is refused.
    pub tv_latency_ns: Option<u64>,
    /// (goal 13) For tests only (`--test-tv-latency-ms`): `tv_latency_ns`
    /// may leave the plan's 10..40 ms range (up to the offer's 5 s), still
    /// never below the floor. A shared, loaded test host does not keep a
    /// 20 ms deadline; the end-to-end tests grade stamps, maps and counts,
    /// which this does not change.
    pub tv_latency_test: bool,
    /// (goal 13) The low-latency streams' FEC: data chunks per parity (0 for
    /// none, the tests' negative control) and the interleave depth.
    pub fec_k: u8,
    /// See [`ServerConfig::fec_k`].
    pub fec_depth: u8,
    /// (goal 13) For tests only: drop this many datagrams per million, drawn
    /// from a generator seeded with the second value, on BOTH UDP legs (each
    /// datagram the relay receives from a hub and each it sends to a player).
    /// Never set in a deployment; the flag says so in its name's
    /// documentation and in `docs/control-plane.md`.
    pub udp_loss: Option<(u32, u64)>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            listen: "127.0.0.1:4010".to_string(),
            source: "tone".to_string(),
            measure_config: chorus_measure::config::CONFIG_FILE.to_string(),
            chirp_amplitude: None,
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
            zone_transports: Vec::new(),
            group_audio: Vec::new(),
            advertise: false,
            instance: "chorus".to_string(),
            identity_dir: None,
            firmware_dir: None,
            server_id: crate::session::DEFAULT_SERVER_ID.to_string(),
            ephemeral_identity: false,
            slots: 0,
            event_streams: crate::events::DEFAULT_EVENT_STREAMS,
            civil_time: None,
            tz: None,
            civil_time_from: None,
            schedule_time_scale: 1,
            low_latency_port: None,
            tv_latency_ns: None,
            tv_latency_test: false,
            fec_k: chorus_protocol::v2::lowlat::DEFAULTS.fec_k,
            fec_depth: chorus_protocol::v2::lowlat::DEFAULTS.fec_depth,
            udp_loss: None,
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
    /// Stream slots without the room model that routes them.
    SlotsNeedTheControlPlane,
    /// (goal 14) A firmware directory with no control plane to install from.
    FirmwareNeedsTheControlPlane,
    /// Stream slots and the legacy one-process-per-group shape together.
    SlotsWithGroupAudio,
    /// More stream slots than this server serves.
    TooManySlots {
        /// What was asked for.
        slots: usize,
    },
    /// An event-stream ceiling of zero would refuse every stream.
    NoEventStreamsAllowed,
    /// A `--civil-time` that is not `<day>-<HH:MM>`.
    NotACivilTime {
        /// The value as it was given.
        value: String,
    },
    /// A `--civil-time-from` that is not a UTC RFC 3339 instant.
    NotAnInstant {
        /// The value as it was given.
        value: String,
    },
    /// A schedule time scale of 0 or above the ceiling.
    NotATimeScale {
        /// The value as it was given.
        value: u64,
    },
    /// Two ways of setting the civil clock at once.
    TwoCivilClocks,
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
    /// A zone declared with a transport the committed configuration does not
    /// name.
    NotATransport {
        /// The zone that declared it.
        zone: String,
        /// The value as it was read.
        value: String,
        /// Every transport the committed configuration names.
        permitted: String,
    },
    /// (goal 13) A low-latency flag the plan refuses: an `L_tv` out of range
    /// or below the floor, an FEC shape the offer cannot carry, a loss
    /// specification that is not `<ppm>,<seed>`.
    LowLatency {
        /// The argument as it was given.
        argument: String,
        /// Why.
        detail: String,
    },
}

impl fmt::Display for ServerConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServerConfigError::Format(e) => write!(f, "{}", e),
            ServerConfigError::LowLatency { argument, detail } => {
                write!(f, "argument '{}' refused: {}", argument, detail)
            }
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
            ServerConfigError::SlotsNeedTheControlPlane => write!(
                f,
                "--slots needs --control-listen: the room model is what routes each session to \
                 its group's stream, and a server with no control plane has none"
            ),
            ServerConfigError::FirmwareNeedsTheControlPlane => write!(
                f,
                "--firmware-dir needs --control-listen: an image is installed only by the \
                 control plane's firmware_install command, and a server with no control plane \
                 has no way to say it"
            ),
            ServerConfigError::SlotsWithGroupAudio => write!(
                f,
                "--slots and --group-audio are two ways to serve groups and are not combined: \
                 --slots serves every group from this process on its one audio port, \
                 --group-audio points a group at another process (the legacy shape, for Linux \
                 clients only)"
            ),
            ServerConfigError::TooManySlots { slots } => write!(
                f,
                "--slots {} is more than the {} stream slots one server serves",
                slots, MAX_SLOTS
            ),
            ServerConfigError::NoEventStreamsAllowed => write!(
                f,
                "an event-stream ceiling of 0 would refuse every GET /api/events"
            ),
            ServerConfigError::NotACivilTime { value } => write!(
                f,
                "'{}' is not a civil time; --civil-time takes <day>-<HH:MM>, the day one of {}, \
                 for example mon-23:30",
                value,
                DAY_NAMES.join(" ")
            ),
            ServerConfigError::NotAnInstant { value } => write!(
                f,
                "'{}' is not an instant; --civil-time-from takes a UTC RFC 3339 instant, \
                 YYYY-MM-DDTHH:MM:SSZ, for example 2026-10-05T06:59:50Z",
                value
            ),
            ServerConfigError::NotATimeScale { value } => write!(
                f,
                "--schedule-time-scale {} is not 1 to {}",
                value, MAX_TIME_SCALE
            ),
            ServerConfigError::TwoCivilClocks => write!(
                f,
                "--civil-time holds the civil clock fixed and --civil-time-from runs it from an \
                 instant; give one of them"
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
            ServerConfigError::NotATransport {
                zone,
                value,
                permitted,
            } => write!(
                f,
                "the zone '{}' is declared with the transport '{}', and the transports the \
                 committed configuration names are {}. config/transport.conf declares them and \
                 what each is held to; nothing is served, because a zone whose tier nobody chose \
                 would be a zone held to a bound nobody chose",
                zone, value, permitted
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
                "--measure-config" => config.measure_config = value()?,
                "--chirp-amplitude" => {
                    let text = value()?;
                    let amplitude =
                        text.parse::<f64>()
                            .map_err(|_| ServerConfigError::NotANumber {
                                argument: arg.clone(),
                                value: text.clone(),
                            })?;
                    config.chirp_amplitude = Some(amplitude);
                }
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
                "--identity-dir" => config.identity_dir = Some(value()?),
                "--firmware-dir" => config.firmware_dir = Some(value()?),
                "--server-id" => config.server_id = value()?,
                "--ephemeral-identity" => config.ephemeral_identity = true,
                // `--zone <id>` or `--zone <id>=<transport>`. One declaration
                // site, because a zone's transport is a fact about the zone and
                // `docs/control-plane.md` puts the set of zones on this command
                // line. `=` is not an identifier character, so a bare `--zone
                // kitchen` is unambiguous and unchanged.
                "--zone" => {
                    let declaration = value()?;
                    let (id, transport) = match declaration.split_once('=') {
                        Some((id, word)) => {
                            let transport = Transport::parse(word).ok_or_else(|| {
                                ServerConfigError::NotATransport {
                                    zone: id.to_string(),
                                    value: word.to_string(),
                                    permitted: Transport::permitted(),
                                }
                            })?;
                            (id.to_string(), transport)
                        }
                        None => (declaration.clone(), DEFAULT_TRANSPORT),
                    };
                    if !chorus_control::catalog::is_identifier(&id) {
                        return Err(ServerConfigError::NotAZone { value: id });
                    }
                    config.zones.push(id.clone());
                    config.zone_transports.push((id, transport));
                }
                "--group-audio" => {
                    let pair = value()?;
                    let (group, address) = pair.split_once('=').ok_or_else(|| {
                        ServerConfigError::NotAGroupAddress {
                            value: pair.clone(),
                        }
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
                "--slots" => config.slots = number(&arg, &value()?)? as usize,
                "--event-streams" => config.event_streams = number(&arg, &value()?)? as usize,
                "--civil-time" => {
                    let text = value()?;
                    config.civil_time =
                        Some(civil_time(&text).ok_or(ServerConfigError::NotACivilTime {
                            value: text.clone(),
                        })?);
                }
                "--tz" => config.tz = Some(value()?),
                "--civil-time-from" => {
                    let text = value()?;
                    config.civil_time_from =
                        Some(utc_instant(&text).ok_or(ServerConfigError::NotAnInstant {
                            value: text.clone(),
                        })?);
                }
                "--schedule-time-scale" => {
                    let n = number(&arg, &value()?)?;
                    if n == 0 || n > u64::from(MAX_TIME_SCALE) {
                        return Err(ServerConfigError::NotATimeScale { value: n });
                    }
                    config.schedule_time_scale = n as u32;
                }
                "--low-latency-port" => {
                    let n = number(&arg, &value()?)?;
                    config.low_latency_port =
                        Some(u16::try_from(n).map_err(|_| ServerConfigError::LowLatency {
                            argument: arg.clone(),
                            detail: format!("{} is not a UDP port", n),
                        })?);
                }
                "--tv-latency-ms" => {
                    config.tv_latency_ns = Some(number(&arg, &value()?)?.saturating_mul(1_000_000))
                }
                "--test-tv-latency-ms" => {
                    config.tv_latency_ns = Some(number(&arg, &value()?)?.saturating_mul(1_000_000));
                    config.tv_latency_test = true;
                }
                "--fec-k" => {
                    config.fec_k = u8::try_from(number(&arg, &value()?)?).unwrap_or(u8::MAX)
                }
                "--fec-depth" => {
                    config.fec_depth = u8::try_from(number(&arg, &value()?)?).unwrap_or(u8::MAX)
                }
                "--udp-loss" => {
                    let text = value()?;
                    let parsed = text.split_once(',').and_then(|(ppm, seed)| {
                        Some((ppm.parse::<u32>().ok()?, seed.parse::<u64>().ok()?))
                    });
                    match parsed {
                        Some((ppm, seed)) if ppm <= 1_000_000 => {
                            config.udp_loss = Some((ppm, seed))
                        }
                        _ => {
                            return Err(ServerConfigError::LowLatency {
                                argument: arg.clone(),
                                detail: format!(
                                    "'{}' is not <ppm>,<seed> with ppm at most 1000000",
                                    text
                                ),
                            })
                        }
                    }
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
        if config.slots > MAX_SLOTS {
            return Err(ServerConfigError::TooManySlots {
                slots: config.slots,
            });
        }
        if config.slots > 0 && config.control_listen.is_none() {
            return Err(ServerConfigError::SlotsNeedTheControlPlane);
        }
        if config.firmware_dir.is_some() && config.control_listen.is_none() {
            return Err(ServerConfigError::FirmwareNeedsTheControlPlane);
        }
        if config.slots > 0 && !config.group_audio.is_empty() {
            return Err(ServerConfigError::SlotsWithGroupAudio);
        }
        if config.civil_time.is_some() && config.civil_time_from.is_some() {
            return Err(ServerConfigError::TwoCivilClocks);
        }
        if config.event_streams == 0 {
            return Err(ServerConfigError::NoEventStreamsAllowed);
        }
        Ok(config)
    }
}

impl ServerConfig {
    /// The low-latency plan these flags make must be one the offer can carry
    /// and the floor rule allows (ADR 0091): refused, never raised. Checked
    /// where the TV relay is built, after the stream format was accepted (a
    /// rate this server cannot carry is refused as that first).
    pub fn check_low_latency(&self) -> Result<(), ServerConfigError> {
        let plan = self.low_latency_plan();
        if let Err(e) = plan.fec() {
            return Err(ServerConfigError::LowLatency {
                argument: "--fec-k/--fec-depth".to_string(),
                detail: e.to_string(),
            });
        }
        if let Err(e) = plan.check_latency(plan.l_tv_ns) {
            return Err(ServerConfigError::LowLatency {
                argument: "--tv-latency-ms".to_string(),
                detail: e.to_string(),
            });
        }
        Ok(())
    }

    /// The low-latency plan this configuration runs (goal 13): the protocol's
    /// defaults with this server's rate, `L_tv` and FEC shape.
    pub fn low_latency_plan(&self) -> chorus_protocol::v2::lowlat::Plan {
        let mut plan = chorus_protocol::v2::lowlat::DEFAULTS;
        plan.sample_rate_hz = self.sample_rate_hz;
        plan.fec_k = self.fec_k;
        plan.fec_depth = self.fec_depth;
        if let Some(l) = self.tv_latency_ns {
            plan.l_tv_ns = l;
        }
        if self.tv_latency_test {
            plan.l_tv_range_ns = (0, chorus_protocol::v2::LOW_LATENCY_MAX_LATENCY_NS);
        }
        plan
    }
}

/// The largest `--schedule-time-scale`. ASSUMED: a minute of schedule a
/// second, enough to run a 30 s ramp in half a second.
pub const MAX_TIME_SCALE: u32 = 60;

/// `YYYY-MM-DDTHH:MM:SSZ` (RFC 3339, UTC, whole seconds) as seconds since
/// the Unix epoch.
pub fn utc_instant(text: &str) -> Option<i64> {
    let b = text.as_bytes();
    if b.len() != 20
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || b[19] != b'Z'
    {
        return None;
    }
    let field = |from: usize, to: usize| -> Option<i64> {
        let digits = &text[from..to];
        digits
            .bytes()
            .all(|c| c.is_ascii_digit())
            .then(|| digits.parse().ok())?
    };
    let (year, month, day) = (field(0, 4)?, field(5, 7)? as u32, field(8, 10)? as u32);
    let (hour, minute, second) = (field(11, 13)?, field(14, 16)?, field(17, 19)?);
    if !(1..=12).contains(&month)
        || day == 0
        || day > chorus_schedule::civil::days_in_month(year, month)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    Some(
        chorus_schedule::civil::days_from_civil(year, month, day) * 86_400
            + hour * 3_600
            + minute * 60
            + second,
    )
}

/// `<day>-<HH:MM>`: a weekday as the catalog spells it and a time of day.
fn civil_time(text: &str) -> Option<CivilTime> {
    let (day, time) = text.split_once('-')?;
    let weekday = DAY_NAMES.iter().position(|d| *d == day)? as u8;
    Some(CivilTime {
        weekday,
        time: ClockTime::parse(time)?,
    })
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
        let c =
            ServerConfig::from_args(["--control-listen".to_string(), "127.0.0.1:0".to_string()])
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
        let err = ServerConfig::from_args(["--zone".to_string(), "Kitchen Zone".to_string()])
            .unwrap_err();
        assert!(
            matches!(err, ServerConfigError::NotAZone { .. }),
            "{:?}",
            err
        );
        let err = ServerConfig::from_args(["--group-audio".to_string(), "downstairs".to_string()])
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
    fn a_zone_declares_its_transport_where_it_is_declared_and_wired_is_the_default() {
        let c = ServerConfig::from_args([
            "--zone".to_string(),
            "kitchen".to_string(),
            "--zone".to_string(),
            "bedroom=wireless".to_string(),
            "--zone".to_string(),
            "study=wired".to_string(),
        ])
        .unwrap();
        assert_eq!(
            c.zones,
            vec![
                "kitchen".to_string(),
                "bedroom".to_string(),
                "study".to_string()
            ]
        );
        assert_eq!(
            c.zone_transports,
            vec![
                ("kitchen".to_string(), Transport::Wired),
                ("bedroom".to_string(), Transport::Wireless),
                ("study".to_string(), Transport::Wired),
            ],
            "a zone declaring no transport is wired, and it is recorded rather than inferred"
        );
    }

    #[test]
    fn a_transport_the_committed_configuration_does_not_name_is_refused_at_start() {
        for word in ["wifi", "Wireless", "", "wired-ish"] {
            let err = ServerConfig::from_args(["--zone".to_string(), format!("bedroom={}", word)])
                .unwrap_err();
            match &err {
                ServerConfigError::NotATransport {
                    zone,
                    value,
                    permitted,
                } => {
                    assert_eq!(zone, "bedroom");
                    assert_eq!(value, word);
                    assert_eq!(permitted, "wired, wireless");
                }
                other => panic!("'{}' was not refused as a transport: {:?}", word, other),
            }
            let said = err.to_string();
            assert!(said.contains("bedroom"), "{}", said);
            assert!(said.contains("wired, wireless"), "{}", said);
        }
        // And the zone identifier is still checked on the other side of the `=`.
        let err = ServerConfig::from_args(["--zone".to_string(), "Bed Room=wireless".to_string()])
            .unwrap_err();
        assert!(
            matches!(err, ServerConfigError::NotAZone { .. }),
            "{:?}",
            err
        );
    }

    #[test]
    fn slots_need_the_control_plane_and_are_not_combined_with_group_audio() {
        let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            ServerConfig::default().slots,
            0,
            "the one-stream shape is the default"
        );
        assert_eq!(
            ServerConfig::from_args(args(&["--slots", "2"])).unwrap_err(),
            ServerConfigError::SlotsNeedTheControlPlane
        );
        assert_eq!(
            ServerConfig::from_args(args(&["--firmware-dir", "/srv/firmware"])).unwrap_err(),
            ServerConfigError::FirmwareNeedsTheControlPlane
        );
        let c = ServerConfig::from_args(args(&[
            "--firmware-dir",
            "/srv/firmware",
            "--control-listen",
            "127.0.0.1:0",
        ]))
        .unwrap();
        assert_eq!(c.firmware_dir.as_deref(), Some("/srv/firmware"));
        assert_eq!(ServerConfig::default().firmware_dir, None);
        assert_eq!(
            ServerConfig::from_args(args(&[
                "--slots",
                "2",
                "--control-listen",
                "127.0.0.1:0",
                "--group-audio",
                "g=127.0.0.1:4011"
            ]))
            .unwrap_err(),
            ServerConfigError::SlotsWithGroupAudio
        );
        assert_eq!(
            ServerConfig::from_args(args(&["--slots", "33", "--control-listen", "127.0.0.1:0"]))
                .unwrap_err(),
            ServerConfigError::TooManySlots { slots: 33 }
        );
        let c = ServerConfig::from_args(args(&["--slots", "8", "--control-listen", "127.0.0.1:0"]))
            .unwrap();
        assert_eq!(c.slots, 8);
        assert_eq!(c.event_streams, 64);
        assert_eq!(
            ServerConfig::from_args(args(&["--event-streams", "0"])).unwrap_err(),
            ServerConfigError::NoEventStreamsAllowed
        );
        let c = ServerConfig::from_args(args(&["--civil-time", "sun-07:05"])).unwrap();
        let t = c.civil_time.unwrap();
        assert_eq!((t.weekday, t.time.literal()), (6, "07:05".to_string()));
        for bad in ["sunday-07:05", "sun 07:05", "sun-25:00"] {
            assert!(
                matches!(
                    ServerConfig::from_args(args(&["--civil-time", bad])),
                    Err(ServerConfigError::NotACivilTime { .. })
                ),
                "{}",
                bad
            );
        }
    }

    #[test]
    fn the_client_ceiling_is_what_fixes_the_thread_population() {
        let c = ServerConfig::default();
        assert_eq!(c.max_clients, 4, "docs/decisions/0014 records why 4");
        let c = ServerConfig::from_args(["--max-clients".to_string(), "2".to_string()]).unwrap();
        assert_eq!(c.max_clients, 2);
    }

    #[test]
    fn the_schedule_test_clocks_are_parsed_and_refused_by_name() {
        let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let c = ServerConfig::from_args(args(&[
            "--civil-time-from",
            "2026-10-05T06:59:50Z",
            "--schedule-time-scale",
            "10",
            "--tz",
            "/x",
        ]))
        .unwrap();
        // 2026-10-05 is 20731 days after 1970-01-01.
        assert_eq!(
            c.civil_time_from,
            Some(20_731 * 86_400 + 6 * 3_600 + 59 * 60 + 50)
        );
        assert_eq!(c.schedule_time_scale, 10);
        assert_eq!(c.tz.as_deref(), Some("/x"));
        assert_eq!(ServerConfig::default().schedule_time_scale, 1);
        for bad in [
            "2026-10-05 06:59:50Z",
            "2026-13-05T06:59:50Z",
            "2026-02-30T00:00:00Z",
            "x",
        ] {
            assert!(
                matches!(
                    ServerConfig::from_args(args(&["--civil-time-from", bad])),
                    Err(ServerConfigError::NotAnInstant { .. })
                ),
                "{}",
                bad
            );
        }
        for bad in ["0", "61"] {
            assert!(matches!(
                ServerConfig::from_args(args(&["--schedule-time-scale", bad])),
                Err(ServerConfigError::NotATimeScale { .. })
            ));
        }
        assert_eq!(
            ServerConfig::from_args(args(&[
                "--civil-time",
                "mon-07:00",
                "--civil-time-from",
                "2026-10-05T06:59:50Z"
            ]))
            .unwrap_err(),
            ServerConfigError::TwoCivilClocks
        );
    }
}
