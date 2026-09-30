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

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
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
    let command =
        decode_command(&text).unwrap_or_else(|e| panic!("the catalog refuses '{}': {}", name, e));
    zones.apply(&command).expect("the zone exists");
    assert_eq!(
        zones.zones()[0].name,
        name,
        "the name was applied in memory"
    );

    let written = persist::render(&zones);
    match persist::load(&written, "127.0.0.1:4010") {
        Ok(reloaded) => Ok(reloaded.zones()[0].name.clone()),
        Err(e) => Err(format!(
            "{}\n--- the file this build wrote ---\n{}",
            e, written
        )),
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
        Err(e) => panic!(
            "the state file this build wrote could not be read back: {}",
            e
        ),
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

struct Server {
    child: Child,
    /// The control channel's address, as the server reported binding it.
    control: String,
    /// What the child says, line by line. Held for the server's whole life so
    /// the pumps carrying its output never stop draining it.
    lines: mpsc::Receiver<String>,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// How long a server gets to say it is listening, and how long an answer the
/// control channel owes may take. Generous on purpose: this file grades what
/// the state file holds, and a slow machine is not a finding about it.
const PATIENCE: Duration = Duration::from_secs(30);

fn pump<R: Read + Send + 'static>(stream: R, tx: mpsc::Sender<String>) {
    std::thread::spawn(move || {
        for line in BufReader::new(stream).lines().map_while(Result::ok) {
            let _ = tx.send(line);
        }
    });
}

/// Start a server on ports the kernel picks, and read back where its control
/// channel landed.
///
/// Nothing here chooses a port and hands it over: a port bound, released and
/// passed on is a port anything else on the machine can take in between, and
/// a replacement server started on the port its predecessor just left races
/// that port's release. The server resolves port 0 when it binds and prints
/// the result, so the process holding the socket is the one that picked it.
/// Waiting for the audio socket's line, which comes after the scheduling
/// report, means every control worker exists by the time this returns.
fn start(state: &Path) -> Server {
    let mut child = Command::new(env!("CARGO_BIN_EXE_chorus-server"))
        .args([
            "--listen",
            "127.0.0.1:0",
            "--source",
            "tone",
            "--serve-forever",
            "--allow-non-realtime",
            "--allow-unlocked-memory",
            "--ephemeral-identity",
            "--control-listen",
            "127.0.0.1:0",
            "--state-file",
            state.to_str().unwrap(),
            "--zone",
            "kitchen",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the server binary runs");
    let (tx, lines) = mpsc::channel();
    pump(child.stdout.take().unwrap(), tx.clone());
    pump(child.stderr.take().unwrap(), tx);
    // Owned by the guard from here on, so every way out of this function kills
    // and reaps it unless it is handed back.
    let mut server = Server {
        child,
        control: String::new(),
        lines,
    };
    let mut said = Vec::new();
    let mut control = None;
    let deadline = Instant::now() + PATIENCE;
    while Instant::now() < deadline {
        match server.lines.recv_timeout(Duration::from_millis(200)) {
            Ok(line) => {
                if let Some(at) = line.find("control listening on=") {
                    let rest = &line[at + "control listening on=".len()..];
                    control = rest.split_whitespace().next().map(str::to_string);
                }
                let ready = line.starts_with("chorus-server: listening on=");
                said.push(line);
                if let (true, Some(control)) = (ready, control.as_ref()) {
                    server.control = control.clone();
                    return server;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            // Both pipes closed: the server has gone, and what it said on the
            // way out is in `said`.
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    drop(server);
    panic!(
        "the server never said it was listening (for the replacement server, that is AC-3's \
         failure: no endpoint can come back to playback); it said:\n{}\nThe state file it was handed \
         reads:\n{}",
        said.join("\n"),
        std::fs::read_to_string(state).unwrap_or_else(|e| format!("(unreadable: {})", e))
    );
}

/// One request and the whole answer, or a failure that says how long it waited
/// and what had arrived. An answer cut short by a timeout is never passed on as
/// if it were the whole of one.
fn request(address: &str, head: &str, body: &str) -> String {
    let began = Instant::now();
    let mut socket = TcpStream::connect(address).expect("the control channel is listening");
    socket.set_read_timeout(Some(PATIENCE)).unwrap();
    write!(socket, "{}{}", head, body).expect("the request goes up");
    socket.flush().unwrap();
    let mut response = Vec::new();
    if let Err(e) = socket.read_to_end(&mut response) {
        panic!(
            "no complete answer from the control channel after {:?} ({}); what had arrived: {:?}",
            began.elapsed(),
            e,
            String::from_utf8_lossy(&response)
        );
    }
    String::from_utf8_lossy(&response).to_string()
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
    let mut state = std::env::temp_dir();
    state.push(format!(
        "chorus-regress-0043-f1-{}-{}.state",
        std::process::id(),
        name.len()
    ));
    let _ = std::fs::remove_file(&state);

    let first = start(&state);

    let body = format!(r#"{{"v":1,"t":"name","zone":"kitchen","name":"{}"}}"#, name);
    let applied = post(&first.control, &body);
    assert!(
        applied.contains("200 OK"),
        "the control channel refused the name, so there is nothing to persist: {}",
        applied
    );
    let before = state_of(&first.control);
    assert!(
        before.contains(&format!(r#""name":"{}""#, name)),
        "the running server does not hold the name it accepted: {}",
        before
    );

    // SIGKILL, exactly as tools/restart-storm-run.sh does (the drop waits for
    // the process to be gone), then a NEW process on the same state file. It
    // binds ports of its own, so nothing waits for the old ones to be released.
    drop(first);

    let written = std::fs::read_to_string(&state).expect("the state file was written");
    let second = start(&state);
    let after = state_of(&second.control);
    let _ = std::fs::remove_file(&state);
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
