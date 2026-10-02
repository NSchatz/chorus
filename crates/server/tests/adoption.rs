//! Goal 14, line D: a new speaker is adopted without anybody doing anything,
//! then named and assigned a room through the control API, on the real
//! `chorus-server` binary with the real C endpoint binaries and a Linux
//! client.
//!
//! The C endpoints are the firmware's own session code built for the host
//! (`firmware/build/chorus-endpoint-session` and
//! `firmware/build/chorus-endpoint-dsp-session`, the programs
//! `firmware/tests/session-outage.sh` and `firmware/tests/dsp-session.sh`
//! drive): they have no control client and never send `attach`, exactly as
//! the board. This test builds them with the firmware's own Makefile, so it
//! needs what `make firmware-check` needs (make, a C compiler and the pinned
//! ESP-IDF tree for the crypto sources) and fails by name without them.
//!
//! Loopback only; nothing here is timing evidence.

mod common;

use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use chorus_client_linux::config::ClientConfig;
use chorus_client_linux::session::{self, EndpointIdentity, SessionRefusal};
use chorus_control::json::{self, Value};
use chorus_protocol::AudioChunk;
use common::{Player, RunningServer};

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("this crate lives at crates/<name> under the repository root")
        .to_path_buf()
}

/// Build the two host programs of the C endpoint with the firmware's own
/// Makefile (a no-op when they are current) and say where they are.
fn c_endpoints() -> (PathBuf, PathBuf) {
    let root = repository_root();
    let build = root.join("firmware").join("build");
    let session = build.join("chorus-endpoint-session");
    let dsp_session = build.join("chorus-endpoint-dsp-session");
    let made = Command::new("make")
        .current_dir(&root)
        .arg("--no-print-directory")
        .arg("-f")
        .arg(root.join("firmware").join("Makefile"))
        .arg(&session)
        .arg(&dsp_session)
        .output();
    let how = "this test runs the real C endpoint, built as `make firmware-check` builds it: it \
               needs make, a C compiler and the pinned ESP-IDF tree (CHORUS_IDF_V61_DIR)";
    match made {
        Ok(out) if out.status.success() => {}
        Ok(out) => panic!(
            "MISSING PREREQUISITE: the C endpoint could not be built. {}.\n{}\n{}",
            how,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
        Err(e) => panic!(
            "MISSING PREREQUISITE: make could not be run ({}). {}",
            e, how
        ),
    }
    assert!(session.is_file() && dsp_session.is_file(), "{}", how);
    (session, dsp_session)
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("chorus-adoption-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    listener.local_addr().unwrap().port()
}

/// An id shaped like the firmware's own: `chorus-` and twelve hex digits.
fn speaker_id(n: u64) -> String {
    format!(
        "chorus-{:012x}",
        (u64::from(std::process::id()) << 16 | n) & 0xffff_ffff_ffff
    )
}

/// A C endpoint that is killed if the test ends before it does.
struct Endpoint(Option<Child>);

impl Endpoint {
    fn spawn(program: &Path, args: &[&str]) -> Endpoint {
        Endpoint(Some(
            Command::new(program)
                .args(args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap_or_else(|e| panic!("{} runs: {}", program.display(), e)),
        ))
    }

    /// Wait for it to run its time and give back what it printed.
    fn summary(mut self) -> String {
        let out = self
            .0
            .take()
            .expect("it is running")
            .wait_with_output()
            .expect("the endpoint ran");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.status.success(), "the C endpoint failed:\n{}", text);
        text
    }
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// One `key=value` field of a C endpoint's summary, the last one printed.
fn field(summary: &str, key: &str) -> u64 {
    summary
        .split_whitespace()
        .rev()
        .find_map(|word| word.strip_prefix(&format!("{}=", key)))
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| panic!("the summary has no {}=<number>:\n{}", key, summary))
}

fn state(server: &RunningServer) -> Value {
    let text = server.state();
    json::parse(&text).unwrap_or_else(|e| panic!("the state is JSON ({}): {}", e, text))
}

fn listed(state: &Value, member: &str, id: &str) -> Option<Value> {
    match state.get(member) {
        Some(Value::Arr(items)) => items
            .iter()
            .find(|s| s.get("id").and_then(Value::as_str) == Some(id))
            .cloned(),
        _ => None,
    }
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("no text '{}' in {}", key, json::write(value)))
        .to_string()
}

fn flag(value: &Value, key: &str) -> bool {
    value
        .get(key)
        .and_then(Value::as_bool)
        .unwrap_or_else(|| panic!("no flag '{}' in {}", key, json::write(value)))
}

/// Poll the state until `done` holds of it, or panic naming `what`.
fn until(server: &RunningServer, what: &str, done: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let now = state(server);
        if done(&now) {
            return now;
        }
        assert!(
            Instant::now() < deadline,
            "{}: not within 15 s; the state is {}",
            what,
            json::write(&now)
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn zone_lists(state: &Value, zone: &str, list: &str, id: &str) -> bool {
    listed(state, "zones", zone)
        .and_then(|z| z.get(list).map(json::write))
        .is_some_and(|members| members.contains(&format!("\"{}\"", id)))
}

fn silent(chunk: &AudioChunk) -> bool {
    chunk.audio_data.iter().all(|b| *b == 0)
}

fn pin_file(identity: &Path) -> String {
    std::fs::read_to_string(identity.join("adopted-endpoints")).unwrap_or_default()
}

/// The whole of line D on the real binaries.
///
/// What is NOT rebuilt here is the trust rule adoption stands on: a key that
/// changes under an adopted id is refused and never re-pinned. That is goal
/// 5's, tested where it lives and cited, not copied:
/// `an_endpoint_whose_key_changed_is_refused_and_surfaced` in
/// `crates/protocol/tests/v2_vectors.rs` (the pin store, the refusal's bytes
/// and the endpoint's view of it) and
/// `an_endpoint_whose_key_changed_is_refused_and_surfaced_by_the_server` in
/// `crates/server/tests/v2_end_to_end.rs` (the real server: the refusal, the
/// log line and the pin file left byte for byte). This test adds what goal
/// 14 adds on top: the refusal is also SURFACED in the control state
/// (`key_changes`), and `speaker_forget` is the owner's one way past it.
///
/// In order:
///
/// 1. A real server (two rooms, stream slots, a state file and an identity
///    directory) and three speakers with fresh identities: two C endpoints
///    and a Linux client. Each appears in `speakers`, adopted and unnamed,
///    in no room and present, with no command sent by anybody.
/// 2. `speaker_name` and `speaker_room` through `POST /api/command`; the
///    state shows the names and the room, and the room lists the speakers
///    as members and present. The Linux client, in the same room, hears
///    silence before the assignment and the room's stream after it.
/// 3. The room's volume is set; the C endpoint's playout path applies it
///    (its own summary), the Linux client is sent it.
/// 4. The server is killed and started again on the same files: names and
///    rooms are back (state-file format 5), and the C endpoint, which
///    rejoined by itself under the key it was adopted with, is present in
///    its room again.
/// 5. Another key under the Linux speaker's id is refused, the pin file is
///    untouched, and the state says so in `key_changes`. `speaker_forget`
///    removes the record and the pin, and the next session under that id is
///    adopted afresh: unnamed, in no room, pinned to the new key.
#[test]
fn a_new_speaker_is_adopted_named_and_assigned_a_room() {
    let (session_bin, dsp_session_bin) = c_endpoints();
    let dir = scratch("line-d");
    let identity = dir.join("identity");
    let state_file = dir.join("zones.state");
    let listen = format!("127.0.0.1:{}", free_port());
    let identity_flags = ["--identity-dir", identity.to_str().unwrap()];
    let flags = [
        "--source",
        "tone",
        "--serve-forever",
        "--slots",
        "2",
        "--zone",
        "kitchen",
        "--zone",
        "study",
        "--state-file",
        state_file.to_str().unwrap(),
    ];
    let mut server = RunningServer::start_on(&listen, &identity_flags, &flags);

    // --- 1. three new speakers, adopted with no command ---------------------
    let wired = speaker_id(1); // the C endpoint, with a key it keeps in a file
    let amped = speaker_id(2); // the C endpoint with its playout path
    let linux = speaker_id(3); // the Linux client
    let wired_run = Endpoint::spawn(
        &session_bin,
        &[
            "--server",
            &listen,
            "--endpoint-id",
            &wired,
            "--run-seconds",
            "22",
            "--key",
            dir.join("wired.key").to_str().unwrap(),
            "--server-pins",
            dir.join("wired.pins").to_str().unwrap(),
            "--first-backoff-ms",
            "100",
            "--max-backoff-ms",
            "400",
        ],
    );
    let amped_run = Endpoint::spawn(
        &dsp_session_bin,
        &[
            "--server",
            &listen,
            "--endpoint-id",
            &amped,
            "--run-seconds",
            "8",
            "--capture",
            dir.join("amped.raw").to_str().unwrap(),
        ],
    );
    let linux_home = dir.join("linux-identity");
    std::fs::create_dir_all(&linux_home).unwrap();
    let mut linux_identity = EndpointIdentity::load(&linux_home, &linux).unwrap();
    let first_linux_key = linux_identity.fingerprint();
    let mut linux_player =
        Player::connect_as(&server.audio, &mut linux_identity, &ClientConfig::default());

    let adopted = until(&server, "all three speakers are listed and present", |s| {
        [&wired, &amped, &linux]
            .iter()
            .all(|id| listed(s, "speakers", id).is_some_and(|v| flag(&v, "present")))
    });
    for id in [&wired, &amped, &linux] {
        let s = listed(&adopted, "speakers", id).unwrap();
        assert!(!flag(&s, "named"), "{} is unnamed", id);
        assert_eq!(
            text(&s, "name"),
            format!("Speaker {}", &id[id.len() - 4..]),
            "until it is named it is called by the tail of its id"
        );
        assert_eq!(s.get("room"), Some(&Value::Null), "{} is in no room", id);
        assert_eq!(json::write(s.get("roles").unwrap()), r#"["player"]"#);
        assert!(!text(&s, "software").is_empty());
        assert!(pin_file(&identity).contains(&format!(" {}\n", id)));
        server.wait_for(&format!("endpoint adopted id={}", id));
        server.wait_for(&format!("speaker listed id={}", id));
    }
    assert_eq!(
        text(&listed(&adopted, "speakers", &linux).unwrap(), "key"),
        first_linux_key,
        "the key shown is the one that was pinned"
    );
    let report = common::http(
        &server.control,
        "GET /api/report HTTP/1.1\r\nHost: chorus\r\nConnection: close\r\n\r\n",
    )
    .1;
    assert!(
        report.contains("control applied=0 refused=0"),
        "nobody sent a command: {}",
        report
    );
    // In no room it hears the silent slot, as before goal 14.
    let before = linux_player
        .next_chunk(Duration::from_secs(5))
        .expect("a chunk arrives");
    assert!(silent(&before), "a speaker in no room hears silence");

    // --- 2. named and assigned through POST /api/command ---------------------
    for (id, name) in [
        (&wired, "Kitchen shelf"),
        (&amped, "Kitchen counter"),
        (&linux, "Kitchen radio"),
    ] {
        server.applied(&format!(
            r#"{{"v":2,"t":"speaker_name","speaker":"{}","name":"{}"}}"#,
            id, name
        ));
        let answer = server.applied(&format!(
            r#"{{"v":2,"t":"speaker_room","speaker":"{}","room":"kitchen"}}"#,
            id
        ));
        let now = json::parse(&answer).unwrap();
        let s = listed(&now, "speakers", id).unwrap();
        assert_eq!(text(&s, "name"), name);
        assert!(flag(&s, "named") && flag(&s, "present"));
        assert_eq!(text(&s, "room"), "kitchen");
        // The session-only endpoint is a member of the room, and present,
        // with no `attach` from anybody.
        assert!(zone_lists(&now, "kitchen", "endpoints", id));
        assert!(zone_lists(&now, "kitchen", "present", id));
    }
    let (status, refusal) = server.command(&format!(
        r#"{{"v":2,"t":"speaker_room","speaker":"{}","room":"attic"}}"#,
        wired
    ));
    assert!(status.contains("400"), "{} {}", status, refusal);
    assert!(refusal.contains(r#""field":"room""#), "{}", refusal);
    let (status, refusal) = server
        .command(r#"{"v":2,"t":"speaker_name","speaker":"chorus-ffffffffffff","name":"Nobody"}"#);
    assert!(status.contains("400"), "{} {}", status, refusal);
    assert!(refusal.contains(r#""field":"speaker""#), "{}", refusal);

    // The room's stream reaches its new member: the Linux client's chunks
    // turn from the silent slot's to the tone's, in the same session.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let chunk = linux_player
            .next_chunk(Duration::from_secs(5))
            .expect("a chunk arrives");
        if !silent(&chunk) {
            break;
        }
        assert!(Instant::now() < deadline, "the room's stream never came");
    }

    // --- 3. the room's volume is the speakers' -------------------------------
    server.applied(r#"{"v":1,"t":"volume","zone":"kitchen","volume":0.250}"#);
    linux_player.until(
        "the Linux speaker is sent the room's volume",
        Duration::from_secs(10),
        |p| p.room_volumes().last().is_some_and(|v| v.gain == 250),
    );
    let amped_summary = amped_run.summary();
    println!("{}", amped_summary.trim_end());
    assert!(
        field(&amped_summary, "chunks") > 0,
        "the C endpoint played the room's stream"
    );
    assert!(
        field(&amped_summary, "room_volume_messages") >= 1,
        "a session in no room is told no volume: this one was in the room"
    );
    assert_eq!(
        field(&amped_summary, "applied_volume"),
        250,
        "the C endpoint's playout path follows the room's volume"
    );
    until(&server, "the C endpoint that ended is absent", |s| {
        listed(s, "speakers", &amped).is_some_and(|v| !flag(&v, "present"))
            && !zone_lists(s, "kitchen", "present", &amped)
            && zone_lists(s, "kitchen", "endpoints", &amped)
    });

    // --- 4. a restart keeps names and rooms (format 5) -----------------------
    drop(linux_player);
    drop(server);
    let saved = std::fs::read_to_string(&state_file).unwrap();
    assert!(saved.contains("format = 5\n"), "{}", saved);
    assert!(
        saved.contains(&format!(
            "[speaker {}]\nname = Kitchen shelf\nnamed = 1\nroom = kitchen\n",
            wired
        )),
        "{}",
        saved
    );
    let mut server = RunningServer::start_on(&listen, &identity_flags, &flags);
    let back = state(&server);
    for (id, name) in [
        (&wired, "Kitchen shelf"),
        (&amped, "Kitchen counter"),
        (&linux, "Kitchen radio"),
    ] {
        let s = listed(&back, "speakers", id).unwrap();
        assert_eq!(text(&s, "name"), name);
        assert!(flag(&s, "named"));
        assert_eq!(text(&s, "room"), "kitchen");
        assert!(zone_lists(&back, "kitchen", "endpoints", id));
    }
    // The C endpoint rejoins by itself, under the key it was adopted with:
    // known, not adopted again, and present in its room again.
    until(&server, "the C endpoint rejoined its room", |s| {
        listed(s, "speakers", &wired).is_some_and(|v| flag(&v, "present"))
            && zone_lists(s, "kitchen", "present", &wired)
    });
    let rejoined =
        server.wait_for_all(&["client session", &format!("id={} ", wired), "verdict=known"]);
    println!("{}", rejoined);

    // --- 5. a changed key is surfaced; forget is the way past it --------------
    let pins_before = pin_file(&identity);
    let replaced_home = dir.join("linux-identity-replaced");
    std::fs::create_dir_all(&replaced_home).unwrap();
    let mut replaced = EndpointIdentity::load(&replaced_home, &linux).unwrap();
    let second_linux_key = replaced.fingerprint();
    assert_ne!(first_linux_key, second_linux_key);
    let stream = TcpStream::connect(&server.audio).unwrap();
    match session::open(stream, &mut replaced, &ClientConfig::default()) {
        Err(SessionRefusal::RefusedByServer { reason, .. }) => assert_eq!(reason, "key_changed"),
        Err(other) => panic!("expected the server's key_changed refusal, got {:?}", other),
        Ok(_) => panic!("a changed key was admitted"),
    }
    let surfaced = until(&server, "the key change is in the state", |s| {
        listed(s, "key_changes", &linux).is_some()
    });
    let change = listed(&surfaced, "key_changes", &linux).unwrap();
    assert_eq!(text(&change, "pinned"), first_linux_key);
    assert_eq!(text(&change, "offered"), second_linux_key);
    assert_eq!(pin_file(&identity), pins_before, "never re-pinned");
    let s = listed(&surfaced, "speakers", &linux).unwrap();
    assert_eq!(text(&s, "key"), first_linux_key);
    assert!(
        !flag(&s, "present"),
        "the refused session is not a presence"
    );

    let answer = server.applied(&format!(
        r#"{{"v":2,"t":"speaker_forget","speaker":"{}"}}"#,
        linux
    ));
    let forgotten = json::parse(&answer).unwrap();
    assert!(listed(&forgotten, "speakers", &linux).is_none());
    assert!(listed(&forgotten, "key_changes", &linux).is_none());
    assert!(!zone_lists(&forgotten, "kitchen", "endpoints", &linux));
    assert!(!pin_file(&identity).contains(&format!(" {}\n", linux)));
    assert!(pin_file(&identity).contains(&format!(" {}\n", wired)));

    let again = Player::connect_as(&server.audio, &mut replaced, &ClientConfig::default());
    let afresh = until(&server, "the forgotten speaker is adopted afresh", |s| {
        listed(s, "speakers", &linux).is_some_and(|v| flag(&v, "present"))
    });
    let s = listed(&afresh, "speakers", &linux).unwrap();
    assert!(!flag(&s, "named"));
    assert_eq!(
        text(&s, "name"),
        format!("Speaker {}", &linux[linux.len() - 4..])
    );
    assert_eq!(s.get("room"), Some(&Value::Null));
    assert_eq!(text(&s, "key"), second_linux_key);
    assert!(pin_file(&identity).contains(&format!(" {}\n", linux)));
    drop(again);

    // The C endpoint ran its time across the restart: it played before and
    // after it, and nobody told it to rejoin.
    let wired_summary = wired_run.summary();
    println!("{}", wired_summary.trim_end());
    assert!(field(&wired_summary, "chunks") > 0);
    assert!(field(&wired_summary, "rejoins") >= 1);
    assert!(field(&wired_summary, "connections_that_played") >= 2);
    assert_eq!(field(&wired_summary, "refusals"), 0);
    let s = until(&server, "the C endpoint that ended is absent", |s| {
        listed(s, "speakers", &wired).is_some_and(|v| !flag(&v, "present"))
    });
    println!(
        "speakers at the end: {}",
        json::write(s.get("speakers").unwrap())
    );
    drop(server);
    let _ = std::fs::remove_dir_all(&dir);
}
