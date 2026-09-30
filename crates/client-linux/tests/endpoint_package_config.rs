//! The Linux endpoint package's shipped configuration is one this client accepts.
//!
//! `deploy/endpoint/client.conf` becomes `/etc/chorus/client.conf`, and
//! `chorus-client.service` runs `chorus-client` with the unit's own arguments
//! followed by `CHORUS_CLIENT_ARGS` split at whitespace. A shipped file the
//! parser refuses is a speaker that exits 2 on first boot, and
//! `RestartPreventExitStatus=2` then leaves it stopped, so the file is held to
//! the parser here, with the real-time line it documents switched on as well.

use std::path::PathBuf;

use chorus_client_linux::config::ClientConfig;

fn repo_file(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {}", path.display(), e))
}

/// The arguments the unit passes before `$CHORUS_CLIENT_ARGS`, read from its
/// `ExecStart=` line.
fn unit_arguments() -> Vec<String> {
    let unit = repo_file("deploy/endpoint/chorus-client.service");
    let exec = unit
        .lines()
        .find_map(|l| l.strip_prefix("ExecStart="))
        .expect("the unit has an ExecStart= line");
    let mut words: Vec<String> = exec.split_whitespace().map(str::to_string).collect();
    assert_eq!(words.remove(0), "/usr/bin/chorus-client");
    assert_eq!(
        words.pop().as_deref(),
        Some("$CHORUS_CLIENT_ARGS"),
        "the configuration's arguments come last, split at whitespace"
    );
    words
}

fn shipped_arguments() -> Vec<String> {
    let conf = repo_file("deploy/endpoint/client.conf");
    let line = conf
        .lines()
        .find_map(|l| l.strip_prefix("CHORUS_CLIENT_ARGS="))
        .expect("client.conf sets CHORUS_CLIENT_ARGS");
    assert!(
        !line.starts_with('"'),
        "the value is unquoted, one line (systemd.exec(5), EnvironmentFile=)"
    );
    line.split_whitespace().map(str::to_string).collect()
}

#[test]
fn the_shipped_configuration_is_accepted_and_runs_without_real_time() {
    let mut args = unit_arguments();
    args.extend(shipped_arguments());
    let (config, _) = ClientConfig::from_args(args).expect("the shipped arguments parse");
    config.validate().expect("the shipped arguments validate");
    assert!(config.rejoin, "the unit rejoins");
    assert_eq!(
        config.identity_dir.as_deref(),
        Some("/var/lib/chorus-client")
    );
    assert!(
        config.no_delay_log,
        "an installed endpoint keeps no growing delay log"
    );
    assert_eq!(
        config.rt_priority, None,
        "real-time playout is off by default"
    );
}

#[test]
fn the_documented_real_time_line_is_accepted_and_fits_the_units_limits() {
    let conf = repo_file("deploy/endpoint/client.conf");
    let documented = conf
        .lines()
        .map(|l| l.trim_start_matches('#').trim())
        .find(|l| l.starts_with("--rt-priority"))
        .expect("client.conf documents the real-time arguments");
    let mut args = unit_arguments();
    args.extend(shipped_arguments());
    args.extend(documented.split_whitespace().map(str::to_string));
    let (config, _) = ClientConfig::from_args(args).expect("the documented line parses");
    config.validate().expect("the documented line validates");

    // The client clamps its priority to LimitRTPRIO= and can only lower
    // RLIMIT_RTTIME below the unit's LimitRTTIME=, so the documented values
    // must sit at or under the unit's.
    let unit = repo_file("deploy/endpoint/chorus-client.service");
    let value = |key: &str| {
        unit.lines()
            .find_map(|l| l.strip_prefix(key))
            .unwrap_or_else(|| panic!("the unit sets {}", key))
            .to_string()
    };
    let rtprio: u32 = value("LimitRTPRIO=").parse().unwrap();
    let rttime_ms: u64 = value("LimitRTTIME=")
        .strip_suffix("ms")
        .expect("LimitRTTIME= is written in ms")
        .parse()
        .unwrap();
    assert!(config.rt_priority.unwrap() <= rtprio);
    assert!(config.rttime_us <= rttime_ms * 1_000);
}
