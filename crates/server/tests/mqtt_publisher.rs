//! The MQTT publisher against a fake broker (goal 15, line C; P10 Option M2).
//!
//! The real `chorus-server` binary is started with `--mqtt-broker` pointing
//! at a broker that lives in this file: a loopback listener on a port the
//! kernel chose, which reads every packet the server sends, records it, and
//! answers the way MQTT 3.1.1 says a broker does. Nothing here reaches
//! anything but loopback, and no test needs a broker to exist.
//!
//! The fake broker's decoder is its own, written from the standard (OASIS
//! "MQTT Version 3.1.1", read 2026-10-03) and sharing no code with
//! `chorus_mqtt::codec`: a mistake in the encoder is then a packet this file
//! cannot read, not one both sides agree on.
//!
//! What P10 settled, and so what is asserted: retained `online` with a
//! retained `offline` will, retained room and saved-group state that is the
//! control state message's own bytes, events that are not retained, no
//! SUBSCRIBE ever, no topic outside the prefix and none under
//! `homeassistant/`. And around it: off by default, a broker that refuses,
//! dies or goes silent never stops the control plane, and the password is in
//! nothing the server says.
//!
//! Nothing here is graded on how long something took: every wait is for a
//! packet to be in the broker's log, with a generous deadline.

mod common;

use std::collections::{BTreeMap, VecDeque};
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chorus_control::json::{self, Value};
use chorus_protocol::v2::{roles, Command as Button, ControllerCommand, Message};
use common::{Player, RunningServer};

/// How long a test waits for the broker's log to show something. Far longer
/// than it takes; a publisher that is not going to send it fails in this
/// long, not never.
const DEADLINE: Duration = Duration::from_secs(40);

// --- the fake broker ----------------------------------------------------------

/// One thing the broker saw, in the order it saw it.
#[derive(Debug, Clone, PartialEq)]
enum Seen {
    Connect {
        conn: usize,
        flags: u8,
        keep_alive: u16,
        client_id: String,
        will: Option<(String, Vec<u8>, u8, bool)>,
        user: Option<String>,
        password: Option<Vec<u8>>,
        answered: Option<u8>,
    },
    Publish {
        conn: usize,
        topic: String,
        payload: Vec<u8>,
        qos: u8,
        retain: bool,
        dup: bool,
        id: Option<u16>,
    },
    PingReq {
        conn: usize,
    },
    Disconnect {
        conn: usize,
    },
    /// A SUBSCRIBE or an UNSUBSCRIBE: recorded so that a test can prove none
    /// ever arrived.
    Subscribe {
        conn: usize,
        first: u8,
    },
    /// Any other packet type.
    Other {
        conn: usize,
        first: u8,
    },
    /// The connection ended; `clean` after a DISCONNECT. Not clean, with a
    /// will, is the broker publishing the will.
    Closed {
        conn: usize,
        clean: bool,
        will_published: bool,
    },
}

impl Seen {
    fn line(&self) -> String {
        let text = |bytes: &[u8]| {
            let s = String::from_utf8_lossy(bytes);
            if s.chars().count() > 96 {
                format!(
                    "{}... ({} bytes)",
                    s.chars().take(96).collect::<String>(),
                    bytes.len()
                )
            } else {
                s.to_string()
            }
        };
        match self {
            Seen::Connect {
                conn,
                flags,
                keep_alive,
                client_id,
                will,
                user,
                password,
                answered,
            } => format!(
                "conn={} CONNECT flags=0x{:02X} keepalive={} client_id={} will={} user={} \
                 password={} -> {}",
                conn,
                flags,
                keep_alive,
                client_id,
                match will {
                    Some((topic, message, qos, retain)) => format!(
                        "[topic={} message={} qos={} retain={}]",
                        topic,
                        text(message),
                        qos,
                        u8::from(*retain)
                    ),
                    None => "none".to_string(),
                },
                user.as_deref().unwrap_or("-"),
                match password {
                    Some(p) => format!("<{} bytes>", p.len()),
                    None => "-".to_string(),
                },
                match answered {
                    Some(code) => format!("CONNACK {}", code),
                    None => "no answer".to_string(),
                }
            ),
            Seen::Publish {
                conn,
                topic,
                payload,
                qos,
                retain,
                dup,
                id,
            } => format!(
                "conn={} PUBLISH qos={} retain={} dup={} id={} topic={} payload={}",
                conn,
                qos,
                u8::from(*retain),
                u8::from(*dup),
                id.map(|i| i.to_string()).unwrap_or_else(|| "-".to_string()),
                topic,
                if payload.is_empty() {
                    "<empty>".to_string()
                } else {
                    text(payload)
                }
            ),
            Seen::PingReq { conn } => format!("conn={} PINGREQ", conn),
            Seen::Disconnect { conn } => format!("conn={} DISCONNECT", conn),
            Seen::Subscribe { conn, first } => {
                format!("conn={} SUBSCRIBE/UNSUBSCRIBE first=0x{:02X}", conn, first)
            }
            Seen::Other { conn, first } => format!("conn={} OTHER first=0x{:02X}", conn, first),
            Seen::Closed {
                conn,
                clean,
                will_published,
            } => format!(
                "conn={} closed clean={} will_published={}",
                conn,
                u8::from(*clean),
                u8::from(*will_published)
            ),
        }
    }
}

#[derive(Default)]
struct Shared {
    log: Mutex<Vec<Seen>>,
    /// The retained message of each topic, as a broker would hold it.
    retained: Mutex<BTreeMap<String, Vec<u8>>>,
    /// CONNACK return codes for the next connections, in order; then accept.
    refuse: Mutex<VecDeque<u8>>,
    /// Connections numbered up to this one are answered nothing.
    silent_up_to: AtomicUsize,
    connections: AtomicUsize,
    sockets: Mutex<Vec<TcpStream>>,
}

struct FakeBroker {
    address: String,
    shared: Arc<Shared>,
}

/// Read one packet: its first byte and its body. `None` at the end of the
/// stream or on anything that is not a packet.
fn read_packet(stream: &mut TcpStream) -> Option<(u8, Vec<u8>)> {
    let mut first = [0u8; 1];
    stream.read_exact(&mut first).ok()?;
    // SPEC 2.2.3: base 128, least significant first, at most four bytes.
    let mut length = 0usize;
    let mut shift = 0;
    loop {
        let mut byte = [0u8; 1];
        stream.read_exact(&mut byte).ok()?;
        length |= usize::from(byte[0] & 0x7F) << shift;
        if byte[0] & 0x80 == 0 {
            break;
        }
        shift += 7;
        if shift > 21 {
            return None;
        }
    }
    let mut body = vec![0u8; length];
    stream.read_exact(&mut body).ok()?;
    Some((first[0], body))
}

/// A length-prefixed field off the front of `body` (SPEC 1.5.3).
fn field(body: &mut &[u8]) -> Option<Vec<u8>> {
    if body.len() < 2 {
        return None;
    }
    let length = usize::from(u16::from_be_bytes([body[0], body[1]]));
    if body.len() < 2 + length {
        return None;
    }
    let value = body[2..2 + length].to_vec();
    *body = &body[2 + length..];
    Some(value)
}

fn string(body: &mut &[u8]) -> Option<String> {
    String::from_utf8(field(body)?).ok()
}

/// A CONNECT's body, read as SPEC 3.1 lays it out. `None` if it is not one
/// the standard allows.
fn read_connect(conn: usize, body: &[u8]) -> Option<Seen> {
    let mut rest = body;
    if field(&mut rest)? != b"MQTT" || rest.len() < 4 || rest[0] != 4 {
        return None;
    }
    let flags = rest[1];
    let keep_alive = u16::from_be_bytes([rest[2], rest[3]]);
    rest = &rest[4..];
    // The reserved bit is 0 [MQTT-3.1.2-3]; this client's session is clean.
    if flags & 0x01 != 0 || flags & 0x02 == 0 {
        return None;
    }
    let client_id = string(&mut rest)?;
    let will = if flags & 0x04 != 0 {
        let topic = string(&mut rest)?;
        let message = field(&mut rest)?;
        Some((topic, message, (flags >> 3) & 0x03, flags & 0x20 != 0))
    } else {
        // No will: its QoS and retain bits are 0 [MQTT-3.1.2-11].
        if flags & 0x38 != 0 {
            return None;
        }
        None
    };
    let user = if flags & 0x80 != 0 {
        Some(string(&mut rest)?)
    } else {
        None
    };
    let password = if flags & 0x40 != 0 {
        // A password only with a user name [MQTT-3.1.2-22].
        user.as_ref()?;
        Some(field(&mut rest)?)
    } else {
        None
    };
    if !rest.is_empty() {
        return None;
    }
    Some(Seen::Connect {
        conn,
        flags,
        keep_alive,
        client_id,
        will,
        user,
        password,
        answered: None,
    })
}

/// A PUBLISH, read as SPEC 3.3 lays it out.
fn read_publish(conn: usize, first: u8, body: &[u8]) -> Option<Seen> {
    let qos = (first >> 1) & 0x03;
    let dup = first & 0x08 != 0;
    // QoS 3 is not a level, and DUP is 0 at QoS 0 [MQTT-3.3.1-4],
    // [MQTT-3.3.1-2].
    if qos == 3 || (qos == 0 && dup) {
        return None;
    }
    let mut rest = body;
    let topic = string(&mut rest)?;
    let id = if qos > 0 {
        if rest.len() < 2 {
            return None;
        }
        let id = u16::from_be_bytes([rest[0], rest[1]]);
        rest = &rest[2..];
        // Never 0 [MQTT-2.3.1-1].
        if id == 0 {
            return None;
        }
        Some(id)
    } else {
        None
    };
    // No wildcard in a topic name [MQTT-3.3.2-2], and at least one character.
    if topic.is_empty() || topic.contains(['+', '#', '\0']) {
        return None;
    }
    Some(Seen::Publish {
        conn,
        topic,
        payload: rest.to_vec(),
        qos,
        retain: first & 0x01 != 0,
        dup,
        id,
    })
}

impl Shared {
    fn record(&self, seen: Seen) {
        self.log.lock().unwrap().push(seen);
    }

    fn retain(&self, topic: &str, payload: &[u8]) {
        let mut retained = self.retained.lock().unwrap();
        if payload.is_empty() {
            // A retained message with no payload clears the topic
            // [MQTT-3.3.1-10].
            retained.remove(topic);
        } else {
            retained.insert(topic.to_string(), payload.to_vec());
        }
    }

    /// Serve one connection until it ends.
    fn serve(&self, conn: usize, mut stream: TcpStream) {
        let silent = || conn <= self.silent_up_to.load(Ordering::SeqCst);
        let mut will = None;
        let mut clean = false;
        let mut connected = false;
        while let Some((first, body)) = read_packet(&mut stream) {
            match first >> 4 {
                1 if !connected => {
                    let Some(mut seen) = read_connect(conn, &body) else {
                        self.record(Seen::Other { conn, first });
                        break;
                    };
                    let code = self.refuse.lock().unwrap().pop_front().unwrap_or(0);
                    let answer = !silent();
                    if let Seen::Connect {
                        answered,
                        will: asked,
                        ..
                    } = &mut seen
                    {
                        *answered = answer.then_some(code);
                        if code == 0 {
                            will = asked.clone();
                        }
                    }
                    self.record(seen);
                    if answer {
                        let _ = stream.write_all(&[0x20, 0x02, 0x00, code]);
                    }
                    if code != 0 {
                        // A refusing broker closes [MQTT-3.2.2-5]; nothing
                        // was connected, so there is no will to publish.
                        let _ = stream.shutdown(Shutdown::Both);
                        self.record(Seen::Closed {
                            conn,
                            clean: false,
                            will_published: false,
                        });
                        return;
                    }
                    connected = true;
                }
                3 if connected => {
                    let Some(seen) = read_publish(conn, first, &body) else {
                        self.record(Seen::Other { conn, first });
                        break;
                    };
                    if let Seen::Publish {
                        topic,
                        payload,
                        retain,
                        id,
                        ..
                    } = &seen
                    {
                        if *retain {
                            self.retain(topic, payload);
                        }
                        if let (Some(id), false) = (id, silent()) {
                            let id = id.to_be_bytes();
                            let _ = stream.write_all(&[0x40, 0x02, id[0], id[1]]);
                        }
                    }
                    self.record(seen);
                }
                12 if connected && first == 0xC0 && body.is_empty() => {
                    self.record(Seen::PingReq { conn });
                    if !silent() {
                        let _ = stream.write_all(&[0xD0, 0x00]);
                    }
                }
                14 if connected && first == 0xE0 && body.is_empty() => {
                    self.record(Seen::Disconnect { conn });
                    clean = true;
                    break;
                }
                8 | 10 => {
                    self.record(Seen::Subscribe { conn, first });
                    break;
                }
                _ => {
                    self.record(Seen::Other { conn, first });
                    break;
                }
            }
        }
        let _ = stream.shutdown(Shutdown::Both);
        // A connection that ended with no DISCONNECT has its will published
        // [MQTT-3.1.2-8]; a DISCONNECT discards it [MQTT-3.14.4-3].
        let mut will_published = false;
        if !clean {
            if let Some((topic, message, _, retain)) = will {
                if retain {
                    self.retain(&topic, &message);
                }
                will_published = true;
            }
        }
        self.record(Seen::Closed {
            conn,
            clean,
            will_published,
        });
    }
}

impl FakeBroker {
    fn start() -> FakeBroker {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let address = listener.local_addr().unwrap().to_string();
        let shared = Arc::new(Shared::default());
        {
            let shared = Arc::clone(&shared);
            thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(stream) = stream else { return };
                    let conn = shared.connections.fetch_add(1, Ordering::SeqCst) + 1;
                    if let Ok(copy) = stream.try_clone() {
                        shared.sockets.lock().unwrap().push(copy);
                    }
                    let shared = Arc::clone(&shared);
                    thread::spawn(move || shared.serve(conn, stream));
                }
            });
        }
        FakeBroker { address, shared }
    }

    fn log(&self) -> Vec<Seen> {
        self.shared.log.lock().unwrap().clone()
    }

    fn retained(&self) -> BTreeMap<String, String> {
        self.shared
            .retained
            .lock()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), String::from_utf8_lossy(v).to_string()))
            .collect()
    }

    /// Answer the next connections' CONNECT with these return codes.
    fn refuse_next(&self, codes: &[u8]) {
        self.shared.refuse.lock().unwrap().extend(codes);
    }

    /// Close every connection there is, as a broker that died would.
    fn drop_connections(&self) {
        for socket in self.shared.sockets.lock().unwrap().drain(..) {
            let _ = socket.shutdown(Shutdown::Both);
        }
    }

    /// Stop answering on every connection there is now; later ones are
    /// answered.
    fn go_silent(&self) {
        self.shared.silent_up_to.store(
            self.shared.connections.load(Ordering::SeqCst),
            Ordering::SeqCst,
        );
    }

    /// The log as lines, numbered.
    fn lines(&self) -> String {
        self.log()
            .iter()
            .enumerate()
            .map(|(i, seen)| format!("#{:02} {}", i, seen.line()))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Wait until `done` holds of the log, or panic naming `what` with the
    /// whole log.
    fn wait(&self, what: &str, done: impl Fn(&[Seen]) -> bool) -> Vec<Seen> {
        let deadline = Instant::now() + DEADLINE;
        loop {
            let log = self.log();
            if done(&log) {
                return log;
            }
            assert!(
                Instant::now() < deadline,
                "the broker never saw {}; it saw:\n{}",
                what,
                self.lines()
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    /// Wait until the retained messages are exactly what `state` asks for
    /// under `prefix`, with the status `online`.
    fn wait_retained_is(&self, what: &str, prefix: &str, server: &RunningServer) {
        let deadline = Instant::now() + DEADLINE;
        loop {
            let wanted = wanted(prefix, &server.state());
            let held = self.retained();
            if held == wanted {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "{}: the broker holds\n{:#?}\nand the state asks for\n{:#?}\nlog:\n{}",
                what,
                held,
                wanted,
                self.lines()
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
}

// --- what the state asks for, worked out here and not by the publisher --------

/// Every retained topic `state` (the body of `GET /api/state`) asks for under
/// `prefix`: `online`, each room and each saved group. Each payload is the
/// entry written by the control catalog's own JSON writer, and must also be
/// found in the state message as it is: the catalog's bytes, not a
/// re-encoding that merely means the same.
fn wanted(prefix: &str, state: &str) -> BTreeMap<String, String> {
    let whole = json::parse(state).expect("the state is JSON");
    let mut out = BTreeMap::new();
    out.insert(format!("{}/server/status", prefix), "online".to_string());
    for (key, level) in [("zones", "rooms"), ("saved_groups", "groups")] {
        let Some(Value::Arr(entries)) = whole.get(key) else {
            panic!("the state has no {}: {}", key, state);
        };
        for entry in entries {
            let id = entry.get("id").and_then(Value::as_str).expect("an id");
            let bytes = json::write(entry);
            assert!(state.contains(&bytes), "{} is not in {}", bytes, state);
            out.insert(format!("{}/{}/{}/state", prefix, level, id), bytes);
        }
    }
    out
}

fn publishes(log: &[Seen]) -> Vec<(String, String, bool)> {
    log.iter()
        .filter_map(|seen| match seen {
            Seen::Publish {
                topic,
                payload,
                retain,
                ..
            } => Some((
                topic.clone(),
                String::from_utf8_lossy(payload).to_string(),
                *retain,
            )),
            _ => None,
        })
        .collect()
}

fn connects(log: &[Seen]) -> usize {
    log.iter()
        .filter(|s| matches!(s, Seen::Connect { .. }))
        .count()
}

/// What P10 settled as a property of every packet the broker ever saw:
/// nothing but CONNECT, PUBLISH, PINGREQ and DISCONNECT, never a SUBSCRIBE,
/// every topic under the prefix and none under `homeassistant/`, every
/// PUBLISH QoS 1 and never a duplicate.
fn assert_read_only_and_under(prefix: &str, log: &[Seen]) {
    let under = format!("{}/", prefix);
    for seen in log {
        match seen {
            Seen::Subscribe { .. } => panic!("the publisher subscribed: {}", seen.line()),
            Seen::Other { .. } => panic!("a packet P10 does not have: {}", seen.line()),
            Seen::Publish {
                topic, qos, dup, ..
            } => {
                assert!(topic.starts_with(&under), "{}", seen.line());
                assert!(!topic.starts_with("homeassistant/"), "{}", seen.line());
                assert!(
                    topic.ends_with("/state")
                        || topic.ends_with("/status")
                        || topic.ends_with("/event"),
                    "a topic P10 does not have: {}",
                    seen.line()
                );
                assert_eq!((*qos, *dup), (1, false), "{}", seen.line());
            }
            Seen::Connect { will, .. } => {
                let (topic, ..) = will.as_ref().expect("a will");
                assert!(topic.starts_with(&under), "{}", seen.line());
            }
            Seen::PingReq { .. } | Seen::Disconnect { .. } | Seen::Closed { .. } => {}
        }
    }
}

fn password_file(password: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "chorus-mqtt-test-{}-{}.password",
        std::process::id(),
        common::fresh_id("f")
    ));
    std::fs::write(&path, format!("{}\n", password)).unwrap();
    path
}

const HOUSE: [&str; 7] = [
    "--source",
    "tone",
    "--serve-forever",
    "--zone",
    "kitchen",
    "--zone",
    "study",
];

fn start(extra: &[&str]) -> RunningServer {
    RunningServer::start(&[&HOUSE[..], extra].concat())
}

fn socket_count(pid: u32) -> usize {
    std::fs::read_dir(format!("/proc/{}/fd", pid))
        .expect("the server is alive and /proc is mounted")
        .filter_map(Result::ok)
        .filter_map(|e| std::fs::read_link(e.path()).ok())
        .filter(|target| target.to_string_lossy().starts_with("socket:"))
        .count()
}

/// Wait for the server to hold exactly `wanted` sockets: a control request
/// the test itself just made is a socket for a moment after it was answered.
fn wait_for_sockets(pid: u32, wanted: usize, what: &str) {
    let deadline = Instant::now() + DEADLINE;
    loop {
        let held = socket_count(pid);
        if held == wanted {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{}: {} sockets, wanted {}",
            what,
            held,
            wanted
        );
        thread::sleep(Duration::from_millis(20));
    }
}

// --- the tests ----------------------------------------------------------------

#[test]
fn mqtt_publishes_what_p10_settled_against_a_fake_broker() {
    let broker = FakeBroker::start();
    // Obviously not a real credential, and made here.
    let password = format!("not-a-real-password-{}", std::process::id());
    let secret = password_file(&password);
    let mut server = start(&[
        "--mqtt-broker",
        &broker.address,
        "--mqtt-user",
        "chorus",
        "--mqtt-password-file",
        secret.to_str().unwrap(),
    ]);
    let prefix = "chorus/v1";
    let status = "chorus/v1/server/status";

    // The CONNECT: a retained `offline` will on the status topic, the user,
    // and the password out of the file with its line ending removed.
    broker.wait_retained_is("the first state", prefix, &server);
    let log = broker.log();
    assert_eq!(
        log[0],
        Seen::Connect {
            conn: 1,
            // user, password, will retain, will QoS 1, will, clean session.
            flags: 0xEE,
            keep_alive: 60,
            client_id: "chorus".to_string(),
            will: Some((status.to_string(), b"offline".to_vec(), 1, true)),
            user: Some("chorus".to_string()),
            password: Some(password.clone().into_bytes()),
            answered: Some(0),
        }
    );
    // The first PUBLISH is the retained `online`, and then one retained
    // state per room, each the room's object from GET /api/state.
    let first = publishes(&log);
    assert_eq!(first[0], (status.to_string(), "online".to_string(), true));
    let state = server.state();
    let asked = wanted(prefix, &state);
    assert_eq!(asked.len(), 3, "online and two rooms: {:?}", asked);
    for room in ["kitchen", "study"] {
        let topic = format!("chorus/v1/rooms/{}/state", room);
        let payload = &asked[&topic];
        assert!(payload.starts_with(&format!(r#"{{"id":"{}","#, room)));
        assert!(first.contains(&(topic, payload.clone(), true)));
    }
    assert_eq!(first.len(), 3, "{:?}", first);

    // A saved group gets its own retained topic, the group's object.
    server.applied(
        r#"{"v":2,"t":"group_save","group":"downstairs","name":"Downstairs","zones":["kitchen","study"]}"#,
    );
    broker.wait_retained_is("the saved group", prefix, &server);
    let group = &broker.retained()["chorus/v1/groups/downstairs/state"];
    assert!(group.starts_with(r#"{"id":"downstairs","name":"Downstairs","zones":["#));
    assert!(server.state().contains(group.as_str()));

    // A volume change republishes that room and no other. The mute of the
    // study that follows is the marker: between the two commands' publishes
    // there is nothing, so the kitchen's change published the kitchen alone.
    let mark = broker.log().len();
    server.applied(r#"{"v":1,"t":"volume","zone":"kitchen","volume":0.250}"#);
    broker.wait("the kitchen's new volume", |log| {
        publishes(&log[mark..]).iter().any(|(t, p, _)| {
            t == "chorus/v1/rooms/kitchen/state" && p.contains(r#""volume":0.250"#)
        })
    });
    server.applied(r#"{"v":1,"t":"mute","zone":"study","muted":true}"#);
    let log = broker.wait("the study muted", |log| {
        publishes(&log[mark..])
            .iter()
            .any(|(t, p, _)| t == "chorus/v1/rooms/study/state" && p.contains(r#""muted":true"#))
    });
    let since: Vec<String> = publishes(&log[mark..]).into_iter().map(|p| p.0).collect();
    assert_eq!(
        since,
        [
            "chorus/v1/rooms/kitchen/state",
            "chorus/v1/rooms/study/state"
        ],
        "one change, one room"
    );
    broker.wait_retained_is("after the two changes", prefix, &server);

    // Deleting the saved group clears its retained topic: a retained PUBLISH
    // with no payload.
    let mark = broker.log().len();
    server.applied(r#"{"v":2,"t":"group_delete","group":"downstairs"}"#);
    let log = broker.wait("the group's topic cleared", |log| {
        publishes(&log[mark..]).contains(&(
            "chorus/v1/groups/downstairs/state".to_string(),
            String::new(),
            true,
        ))
    });
    assert!(
        !publishes(&log[mark..])
            .iter()
            .any(|(t, _, _)| t.contains("/rooms/")),
        "deleting a saved group republished a room:\n{}",
        broker.lines()
    );
    broker.wait_retained_is("after the delete", prefix, &server);
    assert!(!broker
        .retained()
        .contains_key("chorus/v1/groups/downstairs/state"));

    // A controller command from an endpoint (a button on a speaker) is one
    // event that is NOT retained, on that endpoint's topic.
    let button = common::fresh_id("mqtt-button");
    server.applied(&format!(
        r#"{{"v":1,"t":"attach","zone":"kitchen","endpoint":"{}"}}"#,
        button
    ));
    let mut controller = Player::connect(&server.audio, &button, roles::CONTROLLER);
    controller.until("the controller's greeting", Duration::from_secs(10), |p| {
        !p.room_volumes().is_empty()
    });
    let mark = broker.log().len();
    controller
        .session
        .writer
        .send(&Message::ControllerCommand(ControllerCommand {
            command: Button::VolumeStep,
            value: -5,
            target: String::new(),
        }))
        .unwrap();
    let event_topic = format!("chorus/v1/speakers/{}/event", button);
    let log = broker.wait("the controller event", |log| {
        publishes(&log[mark..])
            .iter()
            .any(|(t, _, _)| *t == event_topic)
    });
    let (_, event, retained) = publishes(&log[mark..])
        .into_iter()
        .find(|(t, _, _)| *t == event_topic)
        .unwrap();
    assert!(!retained, "an event must not be retained");
    assert_eq!(
        event,
        format!(
            r#"{{"endpoint":"{}","zone":"kitchen","command":"volume_step","value":-5,"target":"","outcome":"applied"}}"#,
            button
        )
    );
    // The step changed the kitchen, so the kitchen's state follows it.
    broker.wait("the kitchen after the step", |log| {
        publishes(&log[mark..]).iter().any(|(t, p, _)| {
            t == "chorus/v1/rooms/kitchen/state" && p.contains(r#""volume":0.200"#)
        })
    });
    // A transport command changes no room; it is still an event.
    let mark = broker.log().len();
    controller
        .session
        .writer
        .send(&Message::ControllerCommand(ControllerCommand {
            command: Button::Toggle,
            value: 0,
            target: String::new(),
        }))
        .unwrap();
    let log = broker.wait("the toggle event", |log| {
        publishes(&log[mark..])
            .iter()
            .any(|(t, _, _)| *t == event_topic)
    });
    let toggles: Vec<_> = publishes(&log[mark..])
        .into_iter()
        .filter(|(t, _, _)| *t == event_topic)
        .collect();
    assert_eq!(toggles.len(), 1);
    assert!(toggles[0]
        .1
        .contains(r#""command":"toggle","value":0,"target":"","outcome":"waits-for-an-input""#));
    assert!(!toggles[0].2);
    broker.wait_retained_is("at the end", prefix, &server);
    // No event was ever retained: the broker holds the status and the rooms.
    assert!(broker.retained().keys().all(|t| !t.contains("/speakers/")));

    // Everything the broker ever saw, held to what P10 settled.
    let log = broker.log();
    assert_read_only_and_under(prefix, &log);
    assert_eq!(connects(&log), 1, "one connection for the whole run");
    assert!(
        !log.iter().any(|s| matches!(s, Seen::Subscribe { .. })),
        "a SUBSCRIBE arrived"
    );
    assert!(publishes(&log)
        .iter()
        .all(|(t, _, _)| !t.starts_with("homeassistant/") && t.starts_with("chorus/v1/")));

    // The password is in nothing the server said, and not on its command
    // line.
    drop(controller);
    thread::sleep(Duration::from_millis(200));
    server.drain();
    assert!(
        !server.seen.iter().any(|line| line.contains(&password)),
        "the server printed the password"
    );
    let said = server.wait_for("mqtt publisher broker=");
    assert!(said.contains("user=chorus password=set"), "{}", said);
    let cmdline = std::fs::read(format!("/proc/{}/cmdline", server.pid())).unwrap();
    assert!(!String::from_utf8_lossy(&cmdline).contains(&password));

    println!(
        "the fake broker's log ({} packets and closes):\n{}",
        log.len(),
        broker.lines()
    );
    println!("retained at the end:");
    for (topic, payload) in broker.retained() {
        println!("  {} = {}", topic, payload);
    }
    let _ = std::fs::remove_file(secret);
}

#[test]
fn mqtt_is_off_by_default_no_thread_no_connection_no_line() {
    // Without the flag: no publisher thread in the scheduling report, not a
    // word about MQTT, and no socket beyond the two listeners.
    let mut off = start(&[]);
    off.wait_for("chorus-server: listening on=");
    off.drain();
    assert!(
        !off.seen.iter().any(|l| l.to_lowercase().contains("mqtt")),
        "{:?}",
        off.seen
    );
    let off_sockets = socket_count(off.pid());

    // With it, the same server holds exactly one socket more, and says so.
    let broker = FakeBroker::start();
    let mut on = start(&["--mqtt-broker", &broker.address]);
    broker.wait_retained_is("the state", "chorus/v1", &on);
    on.wait_for("thread role=mqtt-publisher");
    on.wait_for("mqtt connected broker=");
    wait_for_sockets(on.pid(), off_sockets + 1, "the publisher is one connection");
    // And the server with it off never came near this broker: one
    // connection, the other server's.
    thread::sleep(Duration::from_millis(300));
    wait_for_sockets(off.pid(), off_sockets, "the server with MQTT off");
    assert_eq!(connects(&broker.log()), 1);
    // An anonymous broker: the will stays, no user and no password.
    match &broker.log()[0] {
        Seen::Connect {
            flags,
            user,
            password,
            ..
        } => assert_eq!((*flags, user, password), (0x2E, &None, &None)),
        other => panic!("{:?}", other),
    }
    assert_read_only_and_under("chorus/v1", &broker.log());
}

#[test]
fn a_broker_that_refuses_then_dies_never_stops_the_control_plane_and_everything_is_republished() {
    let broker = FakeBroker::start();
    // Not authorized, then bad user name or password, then accepted.
    broker.refuse_next(&[5, 4]);
    let prefix = "house/audio";
    let mut server = start(&[
        "--mqtt-broker",
        &broker.address,
        "--mqtt-prefix",
        prefix,
        "--mqtt-client-id",
        "chorusTest2",
    ]);
    // While it is being refused the control plane answers as it always does.
    broker.wait("the first refused CONNECT", |log| connects(log) >= 1);
    let state = server.applied(r#"{"v":1,"t":"volume","zone":"kitchen","volume":0.300}"#);
    assert!(state.contains(r#""volume":0.300"#));
    assert_eq!(server.state(), state);
    // It tries again, is refused again, tries again and is accepted; what it
    // then publishes is the state as it stands, the change made meanwhile
    // included.
    broker.wait_retained_is("after two refusals", prefix, &server);
    let log = broker.log();
    let answers: Vec<Option<u8>> = log
        .iter()
        .filter_map(|s| match s {
            Seen::Connect { answered, .. } => Some(*answered),
            _ => None,
        })
        .collect();
    assert_eq!(answers, [Some(5), Some(4), Some(0)]);
    assert!(
        !log.iter()
            .any(|s| matches!(s, Seen::Publish { conn, .. } if *conn < 3)),
        "something was published on a refused connection:\n{}",
        broker.lines()
    );
    server.wait_for_all(&["mqtt not-connected", "connack=5", "not authorized"]);
    server.wait_for_all(&["mqtt not-connected", "connack=4"]);
    assert!(broker.retained()["house/audio/rooms/kitchen/state"].contains(r#""volume":0.300"#));

    // The broker dies. Its will says `offline`; the control plane goes on.
    let mark = broker.log().len();
    broker.drop_connections();
    broker.wait("the connection gone", |log| {
        log[mark..].iter().any(|s| {
            matches!(
                s,
                Seen::Closed {
                    clean: false,
                    will_published: true,
                    ..
                }
            )
        })
    });
    assert_eq!(broker.retained()["house/audio/server/status"], "offline");
    let state = server.applied(r#"{"v":1,"t":"mute","zone":"study","muted":true}"#);
    assert_eq!(server.state(), state);
    server.wait_for("mqtt disconnected");

    // The publisher comes back by itself and republishes EVERYTHING: the
    // status, and every room whether or not it changed while it was away.
    let log = broker.wait("everything republished", |log| {
        let again = publishes(&log[mark..]);
        ["server/status", "rooms/kitchen/state", "rooms/study/state"]
            .iter()
            .all(|t| again.iter().any(|(topic, _, _)| topic.ends_with(t)))
    });
    let again = publishes(&log[mark..]);
    assert_eq!(
        again[0],
        (
            "house/audio/server/status".to_string(),
            "online".to_string(),
            true
        ),
        "online first"
    );
    broker.wait_retained_is("after the reconnect", prefix, &server);
    assert!(broker.retained()["house/audio/rooms/study/state"].contains(r#""muted":true"#));
    let log = broker.log();
    assert_eq!(connects(&log), 4);
    assert!(log.iter().all(|s| match s {
        Seen::Connect { client_id, .. } => client_id == "chorusTest2",
        _ => true,
    }));
    assert_read_only_and_under(prefix, &log);
    println!("the fake broker's log:\n{}", broker.lines());
}

#[test]
fn a_broker_that_is_not_there_does_not_stop_the_server() {
    // A loopback port nothing listens on: bound, read, and closed again.
    let nobody = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().to_string()
    };
    let mut server = start(&["--mqtt-broker", &nobody]);
    // The server came up (start returns once it listens for audio), says the
    // broker is not there, and serves commands and audio.
    server.wait_for_all(&["mqtt not-connected", "retry_ms=1000"]);
    let state = server.applied(r#"{"v":1,"t":"volume","zone":"kitchen","volume":0.400}"#);
    assert_eq!(server.state(), state);
    let mut client = common::v2_client(server.audio.as_str(), Duration::from_secs(5));
    let mut scratch = vec![0u8; 4096];
    assert!(client.reader.read(&mut scratch).expect("audio comes down") > 0);
    // It keeps trying, each wait twice the last.
    server.wait_for_all(&["mqtt not-connected", "retry_ms=2000"]);
    let state = server.applied(r#"{"v":1,"t":"volume","zone":"kitchen","volume":0.500}"#);
    assert_eq!(server.state(), state);
}

#[test]
fn the_keep_alive_is_a_pingreq_and_a_silent_broker_is_left_and_come_back_to() {
    let broker = FakeBroker::start();
    let server = start(&["--mqtt-broker", &broker.address, "--mqtt-keepalive-s", "2"]);
    broker.wait_retained_is("the state", "chorus/v1", &server);
    match &broker.log()[0] {
        Seen::Connect { keep_alive, .. } => assert_eq!(*keep_alive, 2),
        other => panic!("{:?}", other),
    }
    // With nothing to publish the connection is kept alive by PINGREQ.
    broker.wait("two PINGREQs", |log| {
        log.iter()
            .filter(|s| matches!(s, Seen::PingReq { .. }))
            .count()
            >= 2
    });
    // The broker stops answering. The publisher gives the connection up
    // (the will says `offline`) and connects again; this broker answers the
    // new connection, so everything is published again.
    let mark = broker.log().len();
    broker.go_silent();
    let log = broker.wait("a new connection after the silence", |log| {
        connects(&log[mark..]) >= 1
    });
    assert!(log[mark..]
        .iter()
        .any(|s| matches!(s, Seen::Closed { clean: false, .. })));
    broker.wait("the state again", |log| {
        publishes(&log[mark..])
            .iter()
            .filter(|(t, _, retain)| *retain && t.contains("/rooms/"))
            .count()
            >= 2
    });
    broker.wait_retained_is("after the silence", "chorus/v1", &server);
    assert_read_only_and_under("chorus/v1", &broker.log());
    // The control plane was there throughout.
    let state = server.applied(r#"{"v":1,"t":"volume","zone":"kitchen","volume":0.100}"#);
    assert_eq!(server.state(), state);
}

#[test]
fn a_clean_stop_says_offline_and_then_disconnects() {
    let broker = FakeBroker::start();
    // No --serve-forever: the server serves one stream and exits by itself.
    let mut server = RunningServer::start(&[
        "--source",
        "tone",
        "--tone-ms",
        "400",
        "--zone",
        "kitchen",
        "--mqtt-broker",
        &broker.address,
    ]);
    broker.wait("online", |log| {
        publishes(log).contains(&(
            "chorus/v1/server/status".to_string(),
            "online".to_string(),
            true,
        ))
    });
    let mut client = common::v2_client(server.audio.as_str(), Duration::from_secs(5));
    common::drain(&mut client);
    assert_eq!(server.exited(DEADLINE), Some(true), "the run ends cleanly");
    // The last things the broker saw: a retained `offline`, acknowledged,
    // then DISCONNECT, then the connection closed with the will discarded.
    let log = broker.wait("the connection closed", |log| {
        log.iter().any(|s| matches!(s, Seen::Closed { .. }))
    });
    let tail: Vec<String> = log[log.len() - 3..].iter().map(Seen::line).collect();
    assert!(
        tail[0].contains("retain=1")
            && tail[0].contains("topic=chorus/v1/server/status payload=offline"),
        "{:?}",
        tail
    );
    assert_eq!(tail[1], "conn=1 DISCONNECT");
    assert_eq!(tail[2], "conn=1 closed clean=1 will_published=0");
    assert_eq!(broker.retained()["chorus/v1/server/status"], "offline");
    server.wait_for_all(&["mqtt stopped", "offline_acknowledged=1"]);
    assert_read_only_and_under("chorus/v1", &log);
}

#[test]
fn a_password_file_that_cannot_be_read_is_a_configuration_refused_by_name() {
    let run = |file: &str| {
        std::process::Command::new(env!("CARGO_BIN_EXE_chorus-server"))
            .args([
                "--control-listen",
                "127.0.0.1:0",
                "--ephemeral-identity",
                "--mqtt-broker",
                "127.0.0.1:1",
                "--mqtt-user",
                "chorus",
                "--mqtt-password-file",
                file,
            ])
            .output()
            .expect("the server binary runs")
    };
    let missing = std::env::temp_dir().join(format!("chorus-mqtt-none-{}", std::process::id()));
    let out = run(missing.to_str().unwrap());
    assert_eq!(out.status.code(), Some(2));
    let said = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        said.contains("configuration refused") && said.contains("--mqtt-password-file"),
        "{}",
        said
    );
    // An empty file is a mistake to name, not an empty password to send.
    let empty = missing.with_extension("empty");
    std::fs::write(&empty, "\n").unwrap();
    let out = run(empty.to_str().unwrap());
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("holds no password"));
    let _ = std::fs::remove_file(empty);
}
