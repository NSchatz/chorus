//! REGRESSION ARTIFACT for S0051-chorus-wifi-7, impl-gate ordinal 1, finding F1.
//!
//! This file documents a defect. It is written by the refuter and is not a fix.
//!
//! AC-6: "WHEN a zone is declared with no transport THE SYSTEM SHALL treat it as
//! wired and SHALL report, FOR EVERY ZONE IT SERVES, the transport in force and
//! the bound that transport is held to, so that no zone's tier is implicit".
//!
//! `crates/server/src/main.rs` prints one tier line per entry of
//! `config.zones`, which is the `--zone` command line. The zones the server
//! actually SERVES come from `chorus_server::control::initial_state`, which
//! ignores `configured` entirely whenever a state file loads:
//!
//! ```text
//!     let from_file = loaded.is_some();
//!     let mut zones = match loaded {
//!         Some(zones) => zones,
//!         None => { ... for id in configured { ... } ... }
//!     };
//! ```
//!
//! So a server restarted against its own persisted state - the documented
//! restart path, `docs/decisions/0018-the-persisted-zone-state.md` - serves
//! zones for which no tier line is ever printed. Every one of those zones has an
//! implicit tier, which is the exact thing AC-6's closing clause forbids.

use std::io::{BufRead, BufReader};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use chorus_control::catalog::decode_command;
use chorus_control::persist;
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
        "chorus-regress-0051-f1-{}-{}.state",
        std::process::id(),
        name
    ));
    let _ = std::fs::remove_file(&path);
    path
}

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

/// Write a state file the real renderer produced, holding three zones, two of
/// them grouped.
fn a_persisted_house(path: &PathBuf) {
    let mut zones = Zones::new("127.0.0.1:4010");
    zones.add(Zone::new("kitchen")).unwrap();
    zones.add(Zone::new("bedroom")).unwrap();
    zones.add(Zone::new("study")).unwrap();
    for zone in ["kitchen", "bedroom"] {
        zones
            .apply(
                &decode_command(&format!(
                    r#"{{"v":1,"t":"group","zone":"{}","group":"downstairs"}}"#,
                    zone
                ))
                .unwrap(),
            )
            .unwrap();
    }
    std::fs::write(path, persist::render(&zones)).expect("the state file is written");
}

/// F1. A server restarted against its persisted state serves three zones and
/// reports the tier of none of them.
#[test]
fn every_zone_the_server_serves_is_reported_with_its_tier_after_a_restart() {
    let state = scratch("restart");
    a_persisted_house(&state);

    // The documented restart: the persisted state is the authority, so the
    // operator does not repeat the zone list on the command line.
    let (_server, mut out, address) = start(&[], Some(&state));
    let lines = lines_until_listening(&mut out);
    assert!(wait_for_control(&address), "the server never came up");
    let said = lines.join("\n");

    assert!(
        said.contains("zones=3") && said.contains("state=reloaded"),
        "the server has to actually be serving the three persisted zones for this to be about \
         AC-6: {}",
        said
    );

    let reported = lines.iter().filter(|l| l.contains("zone id=")).count();
    assert_eq!(
        reported, 3,
        "AC-6: the server serves 3 zones and reported the tier of {}. Every zone it serves has \
         to carry its transport and its bound, so that no zone's tier is implicit. What it \
         actually said was:\n{}",
        reported, said
    );

    let _ = std::fs::remove_file(&state);
}

/// F1, the sharper shape: the command line names ONE of the persisted zones, so
/// the tier lines that are printed are real but incomplete, and a reader cannot
/// tell the difference.
#[test]
fn a_partial_zone_declaration_does_not_leave_two_thirds_of_the_house_untiered() {
    let state = scratch("partial");
    a_persisted_house(&state);

    let (_server, mut out, address) = start(&["bedroom=wireless"], Some(&state));
    let lines = lines_until_listening(&mut out);
    assert!(wait_for_control(&address), "the server never came up");
    let said = lines.join("\n");

    assert!(
        said.contains("zones=3") && said.contains("state=reloaded"),
        "the server has to be serving the three persisted zones: {}",
        said
    );

    for zone in ["kitchen", "bedroom", "study"] {
        assert!(
            said.contains(&format!("zone id={} transport=", zone)),
            "AC-6: '{}' is served and its tier was never reported, so it is implicit. What the \
             server said was:\n{}",
            zone,
            said
        );
    }

    let _ = std::fs::remove_file(&state);
}
