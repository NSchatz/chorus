//! The wireless tier on the server: which zone is on which transport, what a
//! group holding one is held to, and the refusal of a message that tries to
//! change it.
//!
//! Three criteria of `chorus#WIFI-7`:
//!
//!   AC-6.  a zone declared with no transport is wired, and the server reports
//!          for EVERY zone it serves the transport in force and the bound that
//!          transport is held to, so that no zone's tier is implicit.
//!   AC-7.  a group containing at least one wireless zone is held to the
//!          wireless buffer policy and the wireless bound, and the report names
//!          the zone whose declaration set it.
//!   AC-15. a control message that asks to change a zone's transport is
//!          refused, says why, and leaves every subscriber's state
//!          byte-identical.
//!
//! Every one of them is graded against the REAL `chorus-server` binary reading
//! its own command line and its own state file, not against a struct a test
//! built: a rule that holds in a library and not in the process is not a rule
//! the system has.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use chorus_control::catalog::{decode_command, TRANSPORT_IS_CONFIGURED};
use chorus_control::persist;
use chorus_control::transport::{
    Transport, ZoneTransports, WIRED_BOUND_US, WIRELESS_BOUND_US, WIRELESS_POLICY,
};
use chorus_control::zones::{Zone, Zones};

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

fn scratch(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "chorus-wireless-zones-{}-{}.state",
        std::process::id(),
        name
    ));
    let _ = std::fs::remove_file(&path);
    path
}

/// Start the real server with the given zone declarations, and hand back its
/// stdout so the tier lines can be read off it.
fn start(zone_args: &[&str], state: Option<&PathBuf>) -> (Server, BufReader<ChildStdout>, String) {
    let audio = free_port();
    let control = free_port();
    let mut args = vec![
        "--listen".to_string(),
        format!("127.0.0.1:{}", audio),
        "--source".to_string(),
        "tone".to_string(),
        "--serve-forever".to_string(),
        "--allow-non-realtime".to_string(),
        "--allow-unlocked-memory".to_string(),
        "--control-listen".to_string(),
        format!("127.0.0.1:{}", control),
    ];
    if let Some(path) = state {
        args.push("--state-file".to_string());
        args.push(path.to_str().unwrap().to_string());
    }
    for word in zone_args {
        args.push("--zone".to_string());
        args.push(word.to_string());
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_chorus-server"))
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the server binary runs");
    let out = BufReader::new(child.stdout.take().expect("stdout is piped"));
    (Server(child), out, format!("127.0.0.1:{}", control))
}

/// Read the server's stdout until it says the control channel is listening, and
/// hand back everything it said up to and including that line.
fn lines_until_listening(out: &mut BufReader<ChildStdout>) -> Vec<String> {
    let mut lines = Vec::new();
    for line in out.lines() {
        let line = line.expect("the server's stdout is readable");
        let done = line.contains("control listening on=");
        lines.push(line);
        if done {
            break;
        }
        if lines.len() > 200 {
            break;
        }
    }
    lines
}

fn wait_for_control(address: &str) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if TcpStream::connect(address).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

fn request(address: &str, head: &str, body: &str) -> String {
    let mut socket = TcpStream::connect(address).expect("the control channel is listening");
    socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    write!(socket, "{}{}", head, body).expect("the request goes up");
    socket.flush().unwrap();
    let mut response = String::new();
    let _ = socket.read_to_string(&mut response);
    response
}

fn post(address: &str, body: &str) -> String {
    let head = format!(
        "POST /api/command HTTP/1.1\r\nHost: chorus\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    request(address, &head, body)
}

fn state_of(address: &str) -> String {
    let response = request(
        address,
        "GET /api/state HTTP/1.1\r\nHost: chorus\r\nConnection: close\r\n\r\n",
        "",
    );
    response
        .rsplit_once("\r\n\r\n")
        .map(|(_, body)| body.to_string())
        .unwrap_or(response)
}

/// AC-6. Every zone the server serves is reported with its transport and its
/// bound, and a zone that declared no transport is wired.
#[test]
fn every_zone_is_reported_with_its_transport_and_its_bound() {
    let (_server, mut out, address) = start(&["kitchen", "bedroom=wireless", "study=wired"], None);
    let lines = lines_until_listening(&mut out);
    assert!(wait_for_control(&address), "the server never came up");
    let said = lines.join("\n");

    assert!(
        said.contains(&format!(
            "zone id=kitchen transport=wired bound_us={}",
            WIRED_BOUND_US
        )),
        "a zone declaring no transport has to be reported as wired: {}",
        said
    );
    assert!(
        said.contains(&format!(
            "zone id=bedroom transport=wireless bound_us={}",
            WIRELESS_BOUND_US
        )),
        "{}",
        said
    );
    assert!(
        said.contains(&format!(
            "zone id=study transport=wired bound_us={}",
            WIRED_BOUND_US
        )),
        "{}",
        said
    );

    // EVERY zone, not some of them: three declared, three reported, and no
    // zone's tier left to a reader's knowledge of the default.
    let reported = lines.iter().filter(|l| l.contains("zone id=")).count();
    assert_eq!(reported, 3, "one line per zone, and these were said: {}", said);
}

/// AC-7. A group holding one wireless zone is held to the wireless policy and
/// the wireless bound, and the report names the zone that did it.
#[test]
fn a_group_holding_a_wireless_zone_is_held_to_the_wireless_policy() {
    // A state file in which kitchen and bedroom are in ONE group, written by
    // the same renderer the server writes with, so the server reloads a real
    // grouping rather than being told one on a command line.
    let state = scratch("group");
    let mut zones = Zones::new("127.0.0.1:4010");
    zones.add(Zone::new("kitchen")).unwrap();
    zones.add(Zone::new("bedroom")).unwrap();
    zones.add(Zone::new("study")).unwrap();
    for zone in ["kitchen", "bedroom"] {
        zones
            .apply(&decode_command(&format!(
                r#"{{"v":1,"t":"group","zone":"{}","group":"downstairs"}}"#,
                zone
            ))
            .unwrap())
            .unwrap();
    }
    std::fs::write(&state, persist::render(&zones)).expect("the state file is written");

    let (_server, mut out, address) = start(
        &["kitchen", "bedroom=wireless", "study"],
        Some(&state),
    );
    let lines = lines_until_listening(&mut out);
    assert!(wait_for_control(&address), "the server never came up");
    let said = lines.join("\n");

    let downstairs = lines
        .iter()
        .find(|l| l.contains("group id=downstairs"))
        .unwrap_or_else(|| panic!("no tier was reported for the group: {}", said));
    assert!(
        downstairs.contains("transport=wireless"),
        "one wireless zone makes the whole group wireless: {}",
        downstairs
    );
    assert!(
        downstairs.contains(&format!("bound_us={}", WIRELESS_BOUND_US)),
        "and the group is held to the wireless bound: {}",
        downstairs
    );
    assert!(
        downstairs.contains("set_by=bedroom"),
        "and the report names the zone whose declaration set it: {}",
        downstairs
    );
    assert!(
        downstairs.contains(&format!(
            "playout_latency_us={}",
            WIRELESS_POLICY.playout_latency_us
        )) && downstairs.contains(&format!("min_us={}", WIRELESS_POLICY.min_us))
            && downstairs.contains(&format!("max_us={}", WIRELESS_POLICY.max_us)),
        "and the buffer policy the group is held to: {}",
        downstairs
    );

    // The WIRED zone that shares the group is held to the same policy, because
    // a group is the unit a stream is served to and it cannot be half wired.
    // What says so is that the group's tier is reported once, for the group,
    // and there is no second line holding kitchen to the wired bound.
    assert!(
        !said.contains("group id=kitchen"),
        "kitchen is in downstairs and has no group of its own: {}",
        said
    );

    // The group with no wireless zone in it stays wired and names nobody.
    let study = lines
        .iter()
        .find(|l| l.contains("group id=study"))
        .unwrap_or_else(|| panic!("no tier was reported for the wired group: {}", said));
    assert!(study.contains("transport=wired"), "{}", study);
    assert!(study.contains(&format!("bound_us={}", WIRED_BOUND_US)), "{}", study);
    assert!(study.contains("set_by=none"), "{}", study);
    assert!(study.contains("policy=wired"), "{}", study);

    let _ = std::fs::remove_file(&state);
}

/// AC-15. A control message that tries to change a transport is refused, says
/// why, and leaves every subscriber's state byte-identical.
#[test]
fn a_message_that_tries_to_change_a_transport_is_refused_and_changes_nothing() {
    let (_server, mut out, address) = start(&["kitchen", "bedroom=wireless"], None);
    let _ = lines_until_listening(&mut out);
    assert!(wait_for_control(&address), "the server never came up");

    let before = state_of(&address);
    assert!(before.contains("\"zones\""), "the state reads back: {}", before);

    // Three spellings of the same wish: a field on a command that has one, a
    // field on a command that does not, and a command type of its own.
    let attempts = [
        r#"{"v":1,"t":"group","zone":"kitchen","group":"downstairs","transport":"wireless"}"#,
        r#"{"v":1,"t":"mute","zone":"kitchen","muted":true,"transport":"wired"}"#,
        r#"{"v":1,"t":"transport","zone":"kitchen","transport":"wireless"}"#,
    ];
    for attempt in attempts {
        let answer = post(&address, attempt);
        assert!(
            answer.contains("400 Bad Request"),
            "'{}' was not refused: {}",
            attempt,
            answer
        );
        assert!(
            answer.contains("CONFIGURED and not commanded"),
            "the refusal has to say WHY, and where a transport is declared: {}",
            answer
        );
        assert!(
            answer.contains("config/transport.conf"),
            "and where the tiers are committed: {}",
            answer
        );
    }

    let after = state_of(&address);
    assert_eq!(
        before, after,
        "a refused message leaves every subscriber's state byte-identical"
    );

    // The `mute` attempt above carries a well-formed mute. Nothing of it was
    // applied, which is what "refuse the message" rather than "ignore the extra
    // field" means.
    assert!(
        !after.contains(r#""muted":true"#),
        "part of a refused message was applied: {}",
        after
    );
}

/// AC-5's library half, beside `tools/refusals.sh` which runs the binary: a
/// transport the committed configuration does not name is refused at start,
/// naming the zone, the value and the permitted transports.
#[test]
fn a_transport_the_committed_configuration_does_not_name_stops_the_server() {
    let output = Command::new(env!("CARGO_BIN_EXE_chorus-server"))
        .args([
            "--listen",
            "127.0.0.1:0",
            "--allow-non-realtime",
            "--allow-unlocked-memory",
            "--zone",
            "bedroom=wifi",
        ])
        .output()
        .expect("the server binary runs");
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_ne!(output.status.code(), Some(0), "{}", said);
    assert!(said.contains("bedroom"), "the zone is named: {}", said);
    assert!(said.contains("wifi"), "the value it read is named: {}", said);
    assert!(
        said.contains("wired, wireless"),
        "the permitted transports are named: {}",
        said
    );
    assert!(
        !said.contains("listening on="),
        "no audio was served: {}",
        said
    );
    assert!(
        !said.contains("control listening on="),
        "and no control state: {}",
        said
    );
}

/// The tier arithmetic, over the zone state the server actually holds.
///
/// Beside the process checks rather than instead of them: this is what says the
/// rule is "any wireless zone in the group" rather than "the first zone" or
/// "the zone the stream is named for".
#[test]
fn any_wireless_zone_in_a_group_makes_the_group_wireless_whatever_its_position() {
    for wireless_at in 0..3 {
        let ids = ["kitchen", "bedroom", "study"];
        let zones: Vec<Zone> = ids
            .iter()
            .map(|id| {
                let mut zone = Zone::new(id);
                zone.group = "downstairs".to_string();
                zone
            })
            .collect();
        let declared: Vec<(String, Transport)> = ids
            .iter()
            .enumerate()
            .map(|(at, id)| {
                (
                    id.to_string(),
                    if at == wireless_at {
                        Transport::Wireless
                    } else {
                        Transport::Wired
                    },
                )
            })
            .collect();
        let tier = ZoneTransports::new(&declared).group_tier(&zones, "downstairs");
        assert_eq!(tier.transport, Transport::Wireless);
        assert_eq!(tier.bound_us, WIRELESS_BOUND_US);
        assert_eq!(tier.set_by.as_deref(), Some(ids[wireless_at]));
    }

    // And with none of them wireless the group is wired and names nobody.
    let zones: Vec<Zone> = ["kitchen", "bedroom"]
        .iter()
        .map(|id| {
            let mut zone = Zone::new(id);
            zone.group = "downstairs".to_string();
            zone
        })
        .collect();
    let tier = ZoneTransports::new(&[
        ("kitchen".to_string(), Transport::Wired),
        ("bedroom".to_string(), Transport::Wired),
    ])
    .group_tier(&zones, "downstairs");
    assert_eq!(tier.transport, Transport::Wired);
    assert_eq!(tier.set_by, None);
}

/// The refusal text is one constant, so the server, the tests and a reader all
/// quote the same sentence.
#[test]
fn the_refusal_says_where_a_transport_is_declared() {
    assert!(TRANSPORT_IS_CONFIGURED.contains("CONFIGURED and not commanded"));
    assert!(TRANSPORT_IS_CONFIGURED.contains("--zone <id>=<transport>"));
    assert!(TRANSPORT_IS_CONFIGURED.contains("config/transport.conf"));
}
