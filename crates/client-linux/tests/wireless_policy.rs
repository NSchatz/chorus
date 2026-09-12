//! AC-4 of `chorus#WIFI-7`: "IF a zone is declared wireless THEN THE SYSTEM
//! SHALL apply the wireless buffer policy and SHALL NOT be held to the wired
//! bound."
//!
//! Both halves are decisions the system makes from committed configuration, not
//! measurements, which is why this is a test and not an operator's bench.
//!
//! What none of this says is that a wireless zone HOLDS the 5 ms bound. That is
//! a measured distribution over a real radio, it is AC-2, it is operator graded
//! and `docs/verification-record.md` records it as NOT passed.

use std::path::PathBuf;
use std::process::Command;

use chorus_client_linux::config::{ClientConfig, ConfigError};
use chorus_client_linux::sync::PLAYOUT_LATENCY_US;
use chorus_control::transport::{
    Transport, DEFAULT_TRANSPORT, WIRED_BOUND_US, WIRELESS_BOUND_US, WIRELESS_POLICY,
};

fn args(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| w.to_string()).collect()
}

fn configured(words: &[&str]) -> ClientConfig {
    ClientConfig::from_args(args(words))
        .unwrap_or_else(|e| panic!("{:?} was refused: {}", words, e))
        .0
}

/// Read one `key = value` out of `config/transport.conf`, as a number.
fn conf(key: &str) -> u64 {
    let text = conf_text();
    for line in text.lines() {
        let line = match line.find('#') {
            Some(at) => &line[..at],
            None => line,
        };
        if let Some((k, v)) = line.split_once('=') {
            if k.trim() == key {
                return v
                    .trim()
                    .parse()
                    .unwrap_or_else(|_| panic!("config/transport.conf {} is not a number", key));
            }
        }
    }
    panic!("config/transport.conf has no {}", key);
}

fn conf_word(key: &str) -> String {
    let text = conf_text();
    for line in text.lines() {
        let line = match line.find('#') {
            Some(at) => &line[..at],
            None => line,
        };
        if let Some((k, v)) = line.split_once('=') {
            if k.trim() == key {
                return v.trim().to_string();
            }
        }
    }
    panic!("config/transport.conf has no {}", key);
}

fn conf_text() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../config/transport.conf")
        .canonicalize()
        .expect("config/transport.conf is committed");
    std::fs::read_to_string(path).expect("config/transport.conf is readable")
}

/// The whole of AC-4's first half: a wireless endpoint APPLIES the wireless
/// buffer policy, in every one of the five values that policy declares.
#[test]
fn a_wireless_endpoint_applies_the_wireless_buffer_policy() {
    let wireless = configured(&["--transport", "wireless"]);
    assert_eq!(wireless.transport, Transport::Wireless);
    assert_eq!(wireless.min_us, WIRELESS_POLICY.min_us);
    assert_eq!(wireless.max_us, WIRELESS_POLICY.max_us);
    assert_eq!(wireless.start_fill_us, WIRELESS_POLICY.start_fill_us);
    assert_eq!(wireless.device_target_us, WIRELESS_POLICY.device_target_us);
    assert_eq!(
        wireless.sync.playout_latency_ns / 1_000,
        WIRELESS_POLICY.playout_latency_us
    );

    // And it is a DIFFERENT buffer from the wired one, in every value. A policy
    // that happened to equal the wired numbers would satisfy the assertions
    // above and mean nothing at all.
    let wired = ClientConfig::default();
    assert!(wireless.min_us > wired.min_us, "the floor is deeper");
    assert!(wireless.max_us > wired.max_us, "the ceiling is deeper");
    assert!(
        wireless.start_fill_us > wired.start_fill_us,
        "more is buffered before the first frame"
    );
    assert!(
        wireless.device_target_us > wired.device_target_us,
        "a deeper device queue is held"
    );
    assert!(
        wireless.sync.playout_latency_ns > wired.sync.playout_latency_ns,
        "and content is audible later, which is what a bigger buffer costs"
    );
    assert_eq!(
        wired.sync.playout_latency_ns / 1_000,
        PLAYOUT_LATENCY_US,
        "the wired latency is still the one SYNC-4 fixed"
    );
}

/// AC-4's second half: it is NOT held to the wired bound.
#[test]
fn a_wireless_zone_is_not_held_to_the_wired_bound() {
    let wireless = configured(&["--transport", "wireless"]);
    assert_eq!(wireless.transport.bound_us(), WIRELESS_BOUND_US);
    assert_ne!(
        wireless.transport.bound_us(),
        WIRED_BOUND_US,
        "a wireless zone held to the wired bound is the whole thing this criterion forbids"
    );
    assert!(
        WIRELESS_BOUND_US > WIRED_BOUND_US,
        "the wireless bound is the looser one"
    );
}

/// A zone that declares nothing is wired, and is unchanged by any of this.
#[test]
fn a_zone_that_declares_no_transport_is_wired_and_unchanged() {
    let plain = configured(&[]);
    assert_eq!(plain.transport, DEFAULT_TRANSPORT);
    assert_eq!(plain.transport, Transport::Wired);
    assert_eq!(plain.transport.bound_us(), WIRED_BOUND_US);

    let default = ClientConfig::default();
    assert_eq!(plain.min_us, default.min_us);
    assert_eq!(plain.max_us, default.max_us);
    assert_eq!(plain.start_fill_us, default.start_fill_us);
    assert_eq!(plain.device_target_us, default.device_target_us);
    assert_eq!(plain.sync.playout_latency_ns, default.sync.playout_latency_ns);
    plain.validate().expect("the wired defaults are still valid");
}

/// The policy is a configuration the SHIPPED CLIENT will start with.
///
/// This is the assertion that makes the five numbers a set rather than five
/// independent picks: it runs the client's own `validate`, which enforces the
/// relations between them, so a policy the client would refuse to start with is
/// a policy this repository cannot hold a zone to.
#[test]
fn the_wireless_policy_is_a_configuration_the_client_can_actually_run() {
    let wireless = configured(&["--transport", "wireless"]);
    wireless
        .validate()
        .expect("the committed wireless policy has to be a configuration the client accepts");

    // The relations, spelled out, so a later edit that breaks one is told which.
    assert!(WIRELESS_POLICY.min_us > 0);
    assert!(WIRELESS_POLICY.max_us > WIRELESS_POLICY.min_us);
    assert!(
        WIRELESS_POLICY.start_fill_us > WIRELESS_POLICY.min_us
            && WIRELESS_POLICY.start_fill_us < WIRELESS_POLICY.max_us
    );
    assert!(
        WIRELESS_POLICY.device_target_us > WIRELESS_POLICY.min_us
            && WIRELESS_POLICY.device_target_us < WIRELESS_POLICY.max_us
    );
    assert!(
        WIRELESS_POLICY.playout_latency_us > WIRELESS_POLICY.device_target_us
            && WIRELESS_POLICY.playout_latency_us < WIRELESS_POLICY.max_us,
        "the declared latency has to be one an endpoint in the group can apply"
    );
}

/// AC-8. An endpoint in a group held to the wireless policy that cannot apply
/// the declared latency STOPS, and reports both latencies.
#[test]
fn an_endpoint_that_cannot_apply_the_declared_latency_refuses_naming_both() {
    // Bounds that cannot hold the declared latency at all.
    let squeezed = configured(&["--transport", "wireless", "--max-us", "300000"]);
    let err = squeezed
        .validate()
        .expect_err("500 ms of latency does not fit under a 300 ms ceiling");
    match &err {
        ConfigError::WirelessPlayoutLatencyNotApplied {
            declared_us,
            applied_us,
            max_us,
            ..
        } => {
            assert_eq!(*declared_us, WIRELESS_POLICY.playout_latency_us);
            assert_eq!(
                *applied_us, PLAYOUT_LATENCY_US,
                "the latency it could apply is the wired one, which is exactly what it refuses \
                 to play at"
            );
            assert_eq!(*max_us, 300_000);
        }
        other => panic!("the wrong refusal: {:?}", other),
    }
    let said = err.to_string();
    assert!(said.contains("500000"), "{}", said);
    assert!(said.contains("180000"), "{}", said);
    assert!(said.contains("stops rather than playing"), "{}", said);

    // And an endpoint told to play at the WIRED latency inside a wireless group
    // is the same refusal, which is the case the criterion names by name.
    let wired_latency = configured(&[
        "--transport",
        "wireless",
        "--playout-latency-us",
        "180000",
    ]);
    let err = wired_latency
        .validate()
        .expect_err("playing at the wired latency in a wireless group is the forbidden case");
    match err {
        ConfigError::WirelessPlayoutLatencyNotApplied {
            declared_us,
            applied_us,
            ..
        } => {
            assert_eq!(declared_us, WIRELESS_POLICY.playout_latency_us);
            assert_eq!(applied_us, 180_000);
        }
        other => panic!("the wrong refusal: {:?}", other),
    }
}

/// A transport this build does not have is refused rather than defaulted.
#[test]
fn a_transport_the_committed_configuration_does_not_name_is_refused() {
    for word in ["wifi", "Wireless", "", "copper"] {
        let err = ClientConfig::from_args(args(&["--transport", word]))
            .expect_err("only a word the committed configuration names is a transport");
        match &err {
            ConfigError::NotATransport { value, permitted } => {
                assert_eq!(value, word);
                assert_eq!(permitted, "wired, wireless");
            }
            other => panic!("'{}' was not refused as a transport: {:?}", word, other),
        }
    }
}

/// The committed file and the compiled constants are the same numbers.
///
/// The same arrangement `config/sync.conf` has with
/// `crates/client-linux/src/sync.rs`, and it is here for the same reason: the
/// shipped binaries carry the constants and the committed file is what a person
/// reads, so a check and the thing it checks cannot drift apart.
#[test]
fn the_committed_transport_configuration_and_the_compiled_one_agree() {
    assert_eq!(conf("wired_bound_us"), WIRED_BOUND_US);
    assert_eq!(conf("wireless_bound_us"), WIRELESS_BOUND_US);
    assert_eq!(conf("wireless_min_us"), WIRELESS_POLICY.min_us);
    assert_eq!(conf("wireless_max_us"), WIRELESS_POLICY.max_us);
    assert_eq!(conf("wireless_start_fill_us"), WIRELESS_POLICY.start_fill_us);
    assert_eq!(
        conf("wireless_device_target_us"),
        WIRELESS_POLICY.device_target_us
    );
    assert_eq!(
        conf("wireless_playout_latency_us"),
        WIRELESS_POLICY.playout_latency_us
    );
    assert_eq!(conf_word("transports"), Transport::permitted().replace(", ", " "));
    assert_eq!(conf_word("default_transport"), DEFAULT_TRANSPORT.name());
}

/// The SHIPPED BINARY takes the tier, applies the policy and starts.
///
/// A rule that holds in a library and not in the process is not a rule the
/// system has. This runs the real `chorus-client` with a device that does not
/// exist: the configuration has to be ACCEPTED (the run gets as far as the
/// device) rather than refused, and the wireless run's exit is the device's and
/// not the configuration's.
#[test]
fn the_shipped_client_accepts_the_wireless_tier() {
    let output = Command::new(env!("CARGO_BIN_EXE_chorus-client"))
        .args([
            "--transport",
            "wireless",
            "--device",
            "chorus-no-such-device",
            "--probe-device",
        ])
        .output()
        .expect("the client binary runs");
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !said.contains("configuration refused"),
        "the shipped client refused the committed wireless policy: {}",
        said
    );
    assert_ne!(
        output.status.code(),
        Some(2),
        "exit 2 is the configuration refusal, and the wireless policy is not one: {}",
        said
    );

    // And the same binary REFUSES the case AC-8 names, exits non-zero, and says
    // it played nothing.
    let refused = Command::new(env!("CARGO_BIN_EXE_chorus-client"))
        .args([
            "--transport",
            "wireless",
            "--playout-latency-us",
            "180000",
            "--device",
            "chorus-no-such-device",
        ])
        .output()
        .expect("the client binary runs");
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&refused.stdout),
        String::from_utf8_lossy(&refused.stderr)
    );
    assert_eq!(refused.status.code(), Some(2), "{}", said);
    assert!(said.contains("500000") && said.contains("180000"), "{}", said);
    assert!(said.contains("played=0"), "{}", said);
}
