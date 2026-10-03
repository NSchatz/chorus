//! The command line.

use std::path::PathBuf;
use std::time::Duration;

/// The usage text.
pub const USAGE: &str = "\
usage: chorus-soloistd --soloist-dir DIR --api-key-file FILE --state-dir DIR --cache-dir DIR [options]

  --soloist-dir DIR        the receiver directory shared with chorus-server
                           (r<i>.lock, r<i>.pcm, r<i>.sock)
  --receivers N            how many receivers the pool has (default 1); the
                           lowest free index below N is claimed
  --receiver I             claim exactly this index instead
  --soloist-bin PATH       the Soloist executable (default /opt/soloist/soloist)
  --api-key-file FILE      the file holding the Soloist API key; read at each
                           start of Soloist, never logged
  --state-dir DIR          Soloist data directories live under it, one per target
  --cache-dir DIR          Soloist cache directories live under it, one per target
  --cache-size MB          Soloist's --cache-size: 0 or at least 100 (default 256)
  --pipewire auto|none     run PipeWire and WirePlumber (auto, the default) or
                           neither (none)
  --pipewire-bin PATH      the PipeWire daemon (default pipewire)
  --wireplumber-bin PATH   the session manager (default wireplumber)
  --pipewire-runtime-dir DIR
                           where the configuration and PipeWire's socket go
                           (default /run/chorus-soloist)
  --wireplumber-config-dir DIR
                           WirePlumber's stock configuration (default
                           /usr/share/wireplumber)
  --ws-timeout-ms MS       how long Soloist has to offer its WebSocket (default 20000)
  --stop-timeout-ms MS     SIGTERM to SIGKILL (default 5000)
  --backoff-min-ms MS      the first retry delay after a failure (default 1000)
  --backoff-max-ms MS      the longest retry delay (default 60000)
  --expiry-check-secs S    how often the build's expiry is looked at (default 86400)
  --health-check           with --soloist-dir (and --pipewire, --pipewire-runtime-dir
                           when they are not the defaults): start nothing, say
                           whether this container's supervisor holds its receiver;
                           exit 0 healthy, 1 unhealthy
  --help, --version";

/// Whether PipeWire is run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipewireMode {
    /// Write the configuration and run PipeWire and WirePlumber.
    Auto,
    /// Run neither: something else writes the FIFO (tests).
    None,
}

/// The configuration a command line gives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// The receiver directory.
    pub soloist_dir: PathBuf,
    /// The pool's size.
    pub receivers: usize,
    /// The index to claim, when named.
    pub receiver: Option<usize>,
    /// The Soloist executable.
    pub soloist_bin: PathBuf,
    /// The API key file.
    pub api_key_file: PathBuf,
    /// The parent of the data directories.
    pub state_dir: PathBuf,
    /// The parent of the cache directories.
    pub cache_dir: PathBuf,
    /// `--cache-size` in MB.
    pub cache_size: u64,
    /// Whether PipeWire is run.
    pub pipewire: PipewireMode,
    /// The PipeWire daemon.
    pub pipewire_bin: PathBuf,
    /// The session manager.
    pub wireplumber_bin: PathBuf,
    /// PipeWire's runtime and configuration directory.
    pub pipewire_runtime_dir: PathBuf,
    /// WirePlumber's stock configuration directory.
    pub wireplumber_config_dir: PathBuf,
    /// How long Soloist has to offer its WebSocket.
    pub ws_timeout: Duration,
    /// SIGTERM to SIGKILL.
    pub stop_timeout: Duration,
    /// The first retry delay.
    pub backoff_min: Duration,
    /// The longest retry delay.
    pub backoff_max: Duration,
    /// How often the build's expiry is looked at.
    pub expiry_check: Duration,
}

/// What a command line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parsed {
    /// Run with this configuration.
    Run(Box<Config>),
    /// Print the usage.
    Help,
    /// Print the version.
    Version,
    /// Probe the supervisor of this container (`--health-check`).
    HealthCheck(Health),
}

/// What `--health-check` looks at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Health {
    /// The receiver directory.
    pub soloist_dir: PathBuf,
    /// The supervisor's runtime directory, where it wrote its index.
    pub pipewire_runtime_dir: PathBuf,
    /// Whether the supervisor runs PipeWire.
    pub pipewire: PipewireMode,
}

/// The default of `--pipewire-runtime-dir`.
const RUNTIME_DIR: &str = "/run/chorus-soloist";

fn pipewire_mode(value: &str) -> Result<PipewireMode, String> {
    match value {
        "auto" => Ok(PipewireMode::Auto),
        "none" => Ok(PipewireMode::None),
        other => Err(format!("--pipewire is auto or none, not {other:?}")),
    }
}

/// Read the arguments of a `--health-check` (that flag left out).
fn parse_health(arguments: &[&String]) -> Result<Parsed, String> {
    let mut soloist_dir = None;
    let mut health = Health {
        soloist_dir: PathBuf::new(),
        pipewire_runtime_dir: PathBuf::from(RUNTIME_DIR),
        pipewire: PipewireMode::Auto,
    };
    let mut pairs = arguments.chunks(2);
    for pair in &mut pairs {
        let flag = pair[0].as_str();
        let value = pair
            .get(1)
            .ok_or_else(|| format!("{flag} needs a value"))?
            .as_str();
        match flag {
            "--soloist-dir" => soloist_dir = Some(PathBuf::from(value)),
            "--pipewire-runtime-dir" => health.pipewire_runtime_dir = PathBuf::from(value),
            "--pipewire" => health.pipewire = pipewire_mode(value)?,
            other => return Err(format!("--health-check does not take {other:?}")),
        }
    }
    health.soloist_dir =
        soloist_dir.ok_or_else(|| "--health-check needs --soloist-dir".to_string())?;
    Ok(Parsed::HealthCheck(health))
}

fn number(flag: &str, value: &str) -> Result<u64, String> {
    value
        .parse()
        .map_err(|_| format!("{flag} takes a whole number, not {value:?}"))
}

/// Read the arguments (the program name left out).
pub fn parse(arguments: &[String]) -> Result<Parsed, String> {
    if arguments.iter().any(|a| a == "--health-check") {
        let rest: Vec<&String> = arguments
            .iter()
            .filter(|a| *a != "--health-check")
            .collect();
        return parse_health(&rest);
    }
    let mut soloist_dir = None;
    let mut api_key_file = None;
    let mut state_dir = None;
    let mut cache_dir = None;
    let mut config = Config {
        soloist_dir: PathBuf::new(),
        receivers: 1,
        receiver: None,
        soloist_bin: PathBuf::from("/opt/soloist/soloist"),
        api_key_file: PathBuf::new(),
        state_dir: PathBuf::new(),
        cache_dir: PathBuf::new(),
        cache_size: 256,
        pipewire: PipewireMode::Auto,
        pipewire_bin: PathBuf::from("pipewire"),
        wireplumber_bin: PathBuf::from("wireplumber"),
        pipewire_runtime_dir: PathBuf::from(RUNTIME_DIR),
        wireplumber_config_dir: PathBuf::from("/usr/share/wireplumber"),
        ws_timeout: Duration::from_millis(20_000),
        stop_timeout: Duration::from_millis(5_000),
        backoff_min: Duration::from_millis(1_000),
        backoff_max: Duration::from_millis(60_000),
        expiry_check: Duration::from_secs(86_400),
    };
    let mut at = 0;
    while at < arguments.len() {
        let flag = arguments[at].as_str();
        match flag {
            "--help" | "-h" => return Ok(Parsed::Help),
            "--version" | "-V" => return Ok(Parsed::Version),
            _ => {}
        }
        let value = arguments
            .get(at + 1)
            .ok_or_else(|| format!("{flag} needs a value"))?
            .as_str();
        let millis = |v: &str| number(flag, v).map(Duration::from_millis);
        match flag {
            "--soloist-dir" => soloist_dir = Some(PathBuf::from(value)),
            "--receivers" => config.receivers = number(flag, value)? as usize,
            "--receiver" => config.receiver = Some(number(flag, value)? as usize),
            "--soloist-bin" => config.soloist_bin = PathBuf::from(value),
            "--api-key-file" => api_key_file = Some(PathBuf::from(value)),
            "--state-dir" => state_dir = Some(PathBuf::from(value)),
            "--cache-dir" => cache_dir = Some(PathBuf::from(value)),
            "--cache-size" => config.cache_size = number(flag, value)?,
            "--pipewire" => config.pipewire = pipewire_mode(value)?,
            "--pipewire-bin" => config.pipewire_bin = PathBuf::from(value),
            "--wireplumber-bin" => config.wireplumber_bin = PathBuf::from(value),
            "--pipewire-runtime-dir" => config.pipewire_runtime_dir = PathBuf::from(value),
            "--wireplumber-config-dir" => config.wireplumber_config_dir = PathBuf::from(value),
            "--ws-timeout-ms" => config.ws_timeout = millis(value)?,
            "--stop-timeout-ms" => config.stop_timeout = millis(value)?,
            "--backoff-min-ms" => config.backoff_min = millis(value)?,
            "--backoff-max-ms" => config.backoff_max = millis(value)?,
            "--expiry-check-secs" => {
                config.expiry_check = Duration::from_secs(number(flag, value)?.max(1));
            }
            other => return Err(format!("unknown argument {other:?}")),
        }
        at += 2;
    }
    let need =
        |value: Option<PathBuf>, flag: &str| value.ok_or_else(|| format!("{flag} is required"));
    config.soloist_dir = need(soloist_dir, "--soloist-dir")?;
    config.api_key_file = need(api_key_file, "--api-key-file")?;
    config.state_dir = need(state_dir, "--state-dir")?;
    config.cache_dir = need(cache_dir, "--cache-dir")?;
    if config.receivers == 0 {
        return Err("--receivers must be at least 1".to_string());
    }
    if let Some(index) = config.receiver {
        if index >= config.receivers {
            return Err(format!(
                "--receiver {index} is not below --receivers {}",
                config.receivers
            ));
        }
    }
    // Soloist's own rule: "Values must be `0` or at least `100`."
    if config.cache_size != 0 && config.cache_size < 100 {
        return Err("--cache-size is 0 (no limit) or at least 100".to_string());
    }
    if config.backoff_min.is_zero() || config.backoff_max < config.backoff_min {
        return Err("--backoff-min-ms must be above 0 and at most --backoff-max-ms".to_string());
    }
    Ok(Parsed::Run(Box::new(config)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_string).collect()
    }

    const NEEDED: &str = "--soloist-dir /run/s --api-key-file /k --state-dir /st --cache-dir /c";

    #[test]
    fn the_defaults() {
        let Parsed::Run(config) = parse(&args(NEEDED)).unwrap() else {
            panic!("not a run");
        };
        assert_eq!(config.receivers, 1);
        assert_eq!(config.receiver, None);
        assert_eq!(config.soloist_bin, PathBuf::from("/opt/soloist/soloist"));
        assert_eq!(config.cache_size, 256);
        assert_eq!(config.pipewire, PipewireMode::Auto);
        assert_eq!(config.ws_timeout, Duration::from_secs(20));
        assert_eq!(config.stop_timeout, Duration::from_secs(5));
        assert_eq!(config.expiry_check, Duration::from_secs(86_400));
    }

    #[test]
    fn every_flag_is_read() {
        let line = format!(
            "{NEEDED} --receivers 16 --receiver 3 --soloist-bin /x/soloist --cache-size 0 \
             --pipewire none --pipewire-bin /p --wireplumber-bin /w --pipewire-runtime-dir /r \
             --wireplumber-config-dir /wc --ws-timeout-ms 5 --stop-timeout-ms 6 \
             --backoff-min-ms 7 --backoff-max-ms 8 --expiry-check-secs 9"
        );
        let Parsed::Run(config) = parse(&args(&line)).unwrap() else {
            panic!("not a run");
        };
        assert_eq!((config.receivers, config.receiver), (16, Some(3)));
        assert_eq!(config.soloist_dir, PathBuf::from("/run/s"));
        assert_eq!(config.cache_size, 0);
        assert_eq!(config.pipewire, PipewireMode::None);
        assert_eq!(config.backoff_max, Duration::from_millis(8));
        assert_eq!(config.expiry_check, Duration::from_secs(9));
    }

    #[test]
    fn what_is_refused() {
        for (line, why) in [
            ("--state-dir /st", "--soloist-dir is required"),
            (&format!("{NEEDED} --receivers 0"), "at least 1"),
            (&format!("{NEEDED} --receivers 2 --receiver 2"), "not below"),
            (&format!("{NEEDED} --cache-size 50"), "at least 100"),
            (&format!("{NEEDED} --pipewire maybe"), "auto or none"),
            (&format!("{NEEDED} --receivers many"), "whole number"),
            (&format!("{NEEDED} --frobnicate 1"), "unknown argument"),
            (&format!("{NEEDED} --receivers"), "needs a value"),
            (&format!("{NEEDED} --backoff-min-ms 0"), "above 0"),
        ] {
            let error = parse(&args(line)).unwrap_err();
            assert!(error.contains(why), "{line}: {error}");
        }
        for (line, why) in [
            ("--health-check", "needs --soloist-dir"),
            ("--health-check --soloist-dir", "needs a value"),
            (
                "--health-check --soloist-dir /s --receivers 2",
                "does not take",
            ),
            (
                "--health-check --soloist-dir /s --pipewire maybe",
                "auto or none",
            ),
        ] {
            let error = parse(&args(line)).unwrap_err();
            assert!(error.contains(why), "{line}: {error}");
        }
        assert_eq!(
            parse(&args("--soloist-dir /s --health-check")),
            Ok(Parsed::HealthCheck(Health {
                soloist_dir: PathBuf::from("/s"),
                pipewire_runtime_dir: PathBuf::from("/run/chorus-soloist"),
                pipewire: PipewireMode::Auto,
            }))
        );
        assert_eq!(
            parse(&args(
                "--health-check --pipewire none --pipewire-runtime-dir /r --soloist-dir /s"
            )),
            Ok(Parsed::HealthCheck(Health {
                soloist_dir: PathBuf::from("/s"),
                pipewire_runtime_dir: PathBuf::from("/r"),
                pipewire: PipewireMode::None,
            }))
        );
        assert_eq!(parse(&args("--help")), Ok(Parsed::Help));
        assert_eq!(parse(&args("--version")), Ok(Parsed::Version));
    }
}
