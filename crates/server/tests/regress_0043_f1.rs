//! REGRESSION for S0043-chorus-product-6, refuter finding F1.
//!
//! A zone name that the control catalog ACCEPTS does not survive the persisted
//! state file. `crates/control/src/catalog.rs::is_display_name` allows `#` in a
//! name, `crates/control/src/persist.rs::render` writes the name unescaped as
//! `name = <name>`, and `persist::load` does
//! `raw.split('#').next()` on every line, so everything from the first `#` on
//! that line is discarded as a comment.
//!
//! Two consequences, graded separately below:
//!
//! 1. `Kitchen #1` comes back from a restart as `Kitchen`. The spec's AC-3 says
//!    "Zone names, group membership, volume and mute as they stood before the
//!    kill are what the endpoints come back to; a restart that returns them to
//!    defaults fails this."
//! 2. `#1` renders as `name = #1`, which reads back as an EMPTY name, which
//!    `load` then refuses - so the replacement server exits instead of serving
//!    and AC-3's "return all of them to playback with no operator action"
//!    cannot happen at all.
//!
//! The first two tests are the library round trip. The third runs the real
//! `chorus-server` binary, applies the name over a real socket, kills it with
//! SIGKILL and starts a new process on the same state file - exactly what
//! `tools/restart-storm-run.sh` does, with a name that has a `#` in it.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use chorus_control::catalog::decode_command;
use chorus_control::persist;
use chorus_control::zones::{Zone, Zones};

/// Apply `name` to a zone through the real catalog, render the state file the
/// server would write, and read it back the way the server does at start.
fn round_trip(name: &str) -> Result<String, String> {
    let mut zones = Zones::new("127.0.0.1:4010");
    zones.add(Zone::new("kitchen")).expect("a zone");
    let text = format!(r#"{{"v":1,"t":"name","zone":"kitchen","name":"{}"}}"#, name);
    let command = decode_command(&text)
        .unwrap_or_else(|e| panic!("the catalog refuses '{}': {}", name, e));
    zones.apply(&command).expect("the zone exists");
    assert_eq!(zones.zones()[0].name, name, "the name was applied in memory");

    let written = persist::render(&zones);
    match persist::load(&written, "127.0.0.1:4010") {
        Ok(reloaded) => Ok(reloaded.zones()[0].name.clone()),
        Err(e) => Err(format!("{}\n--- the file this build wrote ---\n{}", e, written)),
    }
}

#[test]
fn a_zone_name_with_a_hash_in_it_survives_the_state_file() {
    let name = "Kitchen #1";
    match round_trip(name) {
        Ok(back) => assert_eq!(
            back, name,
            "the name '{}' did not survive a write and a read of the state file; it came back as \
             '{}'",
            name, back
        ),
        Err(e) => panic!("the state file this build wrote could not be read back: {}", e),
    }
}

#[test]
fn a_zone_name_that_begins_with_a_hash_does_not_make_the_state_file_unreadable() {
    let name = "#1";
    match round_trip(name) {
        Ok(back) => assert_eq!(back, name, "'{}' came back as '{}'", name, back),
        Err(e) => panic!(
            "a name the catalog ACCEPTED makes the state file this build wrote unreadable, so the \
             server that replaces a killed one refuses to start: {}",
            e
        ),
    }
}

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

fn start(audio: u16, control: u16, state: &PathBuf) -> Server {
    Server(
        Command::new(env!("CARGO_BIN_EXE_chorus-server"))
            .args([
                "--listen",
                &format!("127.0.0.1:{}", audio),
                "--source",
                "tone",
                "--serve-forever",
                "--allow-non-realtime",
                "--allow-unlocked-memory",
                "--control-listen",
                &format!("127.0.0.1:{}", control),
                "--state-file",
                state.to_str().unwrap(),
                "--zone",
                "kitchen",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("the server binary runs"),
    )
}

fn wait_for_control(address: &str) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if TcpStream::connect(address).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(200));
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
    request(
        address,
        "GET /api/state HTTP/1.1\r\nHost: chorus\r\nConnection: close\r\n\r\n",
        "",
    )
}

/// The whole of AC-3's second half, with a `#` in the name: a real server, a
/// real command over a real socket, a SIGKILL, and a new process on the same
/// state file.
fn a_restart_gives_the_name_back(name: &str) {
    let audio = free_port();
    let control = free_port();
    let address = format!("127.0.0.1:{}", control);
    let mut state = std::env::temp_dir();
    state.push(format!(
        "chorus-regress-0043-f1-{}-{}.state",
        std::process::id(),
        name.len()
    ));
    let _ = std::fs::remove_file(&state);

    let first = start(audio, control, &state);
    assert!(wait_for_control(&address), "the first server never came up");

    let body = format!(r#"{{"v":1,"t":"name","zone":"kitchen","name":"{}"}}"#, name);
    let applied = post(&address, &body);
    assert!(
        applied.contains("200 OK"),
        "the control channel refused the name, so there is nothing to persist: {}",
        applied
    );
    let before = state_of(&address);
    assert!(
        before.contains(&format!(r#""name":"{}""#, name)),
        "the running server does not hold the name it accepted: {}",
        before
    );

    // SIGKILL, exactly as tools/restart-storm-run.sh does, then a NEW process on
    // the same state file.
    drop(first);
    std::thread::sleep(Duration::from_secs(1));

    let written = std::fs::read_to_string(&state).expect("the state file was written");
    let _second = start(audio, control, &state);
    let came_up = wait_for_control(&address);
    let after = if came_up { state_of(&address) } else { String::new() };
    let _ = std::fs::remove_file(&state);
    assert!(
        came_up,
        "AC-3: the replacement server never came up at all, so no endpoint can come back to \
         playback. The state file it was handed reads:\n{}",
        written
    );
    assert!(
        after.contains(&format!(r#""name":"{}""#, name)),
        "AC-3: the zone did not come back to the name it had before the kill.\nbefore: \
         {}\nafter:  {}\nstate file:\n{}",
        before.lines().last().unwrap_or(""),
        after.lines().last().unwrap_or(""),
        written
    );
}

#[test]
fn a_restart_gives_back_a_zone_name_with_a_hash_in_it() {
    a_restart_gives_the_name_back("Kitchen #1");
}

#[test]
fn a_restart_gives_back_a_zone_name_that_begins_with_a_hash() {
    a_restart_gives_the_name_back("#1");
}
