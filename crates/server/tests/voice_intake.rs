//! The microphone intake on the real binary (proposal P8, Option A; K73, I4;
//! `crates/server/src/voice.rs`).
//!
//! A scripted voice endpoint (a protocol v2 session through the Linux
//! client's own session code, declaring the `voice` role) sends `mic_state`
//! and `mic_audio` to a real `chorus-server` with stream slots, a control
//! plane, a state file and an identity directory. Every `mic_audio` frame it
//! sends carries one marker, the ASCII bytes `MICMARK!` over and over, so the
//! microphone's samples can be looked for, byte for byte, in everything else
//! the server puts out.
//!
//! The test names are the evidence:
//!
//! - frames from a room with voice switched off are dropped and counted;
//!   frames while the gate is reported muted are dropped and counted; with
//!   voice on and the gate live they reach the buffer; and the server tells
//!   the endpoint to stop the uplink (`voice_control`, `uplink` false) when
//!   voice is switched off, and drops what the endpoint sends after that;
//! - I4: while the microphone's audio flows, the state lists no microphone
//!   among its inputs and sources, no command selects it as a source, and
//!   the marker is in no audio chunk of any room (the microphone's own room
//!   included), no v2 message any session received, no visualizer message
//!   or HTTP visualizer stream, no state or event stream, no log line and no
//!   file the server wrote;
//! - voice is off in every room by default and survives a restart, as a
//!   room's other settings do, and a microphone's gate does not.
//!
//! The counts are read off the server's own status lines, which a `mic_state`
//! closes in order (the gate line is written by the reader thread that took
//! the frames before it), so nothing here waits on a guess. Nothing here is
//! timing evidence. Wall clock: about 3 seconds for the file.

mod common;

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use chorus_client_linux::config::ClientConfig;
use chorus_control::json::{self, Value};
use chorus_protocol::v2::session::SecureWriter;
use chorus_protocol::v2::{
    mic_format, roles, Message as V2Message, MicAudio, MicGate, MicState, VoiceControl,
};
use common::line_in::{constant_source, is_silent, is_stream, wait_for, Recorder};
use common::{fresh_id, Player, RunningServer};

/// What every microphone frame of this file carries.
const MARKER: &[u8] = b"MICMARK!";

/// One nominal 20 ms frame: 320 samples, the marker 80 times over.
fn mic_frame(sequence: u32) -> V2Message {
    V2Message::MicAudio(MicAudio {
        format: mic_format::PCM_S16LE_16K_MONO,
        sequence,
        timestamp_ns: u64::from(sequence) * 20_000_000,
        data: MARKER.repeat(80),
    })
}

fn has_marker(bytes: &[u8]) -> bool {
    bytes.windows(MARKER.len()).any(|w| w == MARKER)
}

/// Whether a status line ends its own words with these two counts (the
/// server appends its scheduling and memory state to most lines).
fn counts(line: &str, buffered: u64, dropped: u64) -> bool {
    let wanted = format!("buffered_frames={} dropped_frames={}", buffered, dropped);
    line.ends_with(&wanted) || line.contains(&format!("{} scheduling=", wanted))
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("chorus-voice-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The server on `dir`'s state file and identity, serving `kitchen` and
/// `study` on two slots.
fn server(dir: &Path, source: &Path) -> RunningServer {
    let identity = dir.join("identity");
    let state_file = dir.join("zones.state");
    RunningServer::start_on(
        "127.0.0.1:0",
        &["--identity-dir", identity.to_str().unwrap()],
        &[
            "--source",
            source.to_str().unwrap(),
            "--slots",
            "2",
            "--max-clients",
            "6",
            "--zone",
            "kitchen",
            "--zone",
            "study",
            "--state-file",
            state_file.to_str().unwrap(),
        ],
    )
}

/// A room's `voice_enabled` and `mic_muted`, off a state message.
fn voice(state: &str, id: &str) -> (bool, bool) {
    let value = json::parse(state).unwrap_or_else(|e| panic!("{}: {:?}", state, e));
    let Some(Value::Arr(zones)) = value.get("zones") else {
        panic!("no zones in {}", state)
    };
    let zone = zones
        .iter()
        .find(|z| z.get("id").and_then(Value::as_str) == Some(id))
        .unwrap_or_else(|| panic!("no zone {} in {}", id, state));
    (
        zone.get("voice_enabled")
            .and_then(Value::as_bool)
            .unwrap_or_else(|| panic!("no voice_enabled in {}", state)),
        zone.get("mic_muted")
            .and_then(Value::as_bool)
            .unwrap_or_else(|| panic!("no mic_muted in {}", state)),
    )
}

/// Everything a raw `GET` stream on the control plane says, kept as bytes.
struct Tap {
    bytes: Arc<Mutex<Vec<u8>>>,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl Tap {
    fn open(control: &str, target: &str) -> Tap {
        let mut socket = TcpStream::connect(control).expect("the control plane listens");
        write!(
            socket,
            "GET {} HTTP/1.1\r\nHost: chorus\r\nConnection: close\r\n\r\n",
            target
        )
        .unwrap();
        socket
            .set_read_timeout(Some(Duration::from_millis(50)))
            .unwrap();
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let join = {
            let bytes = Arc::clone(&bytes);
            let stop = Arc::clone(&stop);
            thread::spawn(move || {
                let mut scratch = [0u8; 8_192];
                while !stop.load(Ordering::SeqCst) {
                    match socket.read(&mut scratch) {
                        Ok(0) => return,
                        Ok(n) => bytes.lock().unwrap().extend_from_slice(&scratch[..n]),
                        Err(_) => continue,
                    }
                }
            })
        };
        Tap {
            bytes,
            stop,
            join: Some(join),
        }
    }

    fn bytes(&self) -> Vec<u8> {
        self.bytes.lock().unwrap().clone()
    }
}

impl Drop for Tap {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

/// The scripted voice endpoint: a player that also declares the voice role
/// (and asks for visualizer bands, so the visualizer stream reaches it too).
struct Mic {
    id: String,
    writer: SecureWriter<TcpStream>,
    hears: Recorder,
    sequence: u32,
}

impl Mic {
    fn connect(audio: &str, id: &str) -> Mic {
        let config = ClientConfig {
            extra_roles: roles::VOICE,
            visualizer_bands: 16,
            ..ClientConfig::default()
        };
        let (session, messages) = Player::connect_with(audio, id, &config).split();
        Mic {
            id: id.to_string(),
            writer: session.writer,
            hears: Recorder::reading(session.reader, messages),
            sequence: 0,
        }
    }

    fn gate(&mut self, gate: MicGate) {
        self.writer
            .send(&V2Message::MicState(MicState { gate }))
            .expect("mic_state is sent");
    }

    fn frames(&mut self, count: usize) {
        for _ in 0..count {
            self.writer
                .send(&mic_frame(self.sequence))
                .expect("mic_audio is sent");
            self.sequence = self.sequence.wrapping_add(1);
        }
    }

    /// Every `voice_control` the server has sent it.
    fn told(&self) -> Vec<VoiceControl> {
        self.hears
            .messages
            .lock()
            .unwrap()
            .iter()
            .filter_map(|m| match m {
                V2Message::VoiceControl(c) => Some(*c),
                _ => None,
            })
            .collect()
    }
}

/// Every file under `dir`, with what it holds.
fn files_under(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in std::fs::read_dir(&next).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if let Ok(bytes) = std::fs::read(&path) {
                out.push((path, bytes));
            }
        }
    }
    out
}

#[test]
fn mic_audio_is_kept_only_with_voice_on_and_a_live_gate_and_never_leaves_the_voice_path() {
    let dir = scratch("intake");
    assert!(
        has_marker(&MARKER.repeat(80)),
        "the search finds a frame's own bytes"
    );
    let source = constant_source("voice-intake");
    let mut server = server(&dir, &source);
    // No endpoint id says "mic": the state is searched for that word below.
    let satellite = fresh_id("voice-ep");
    let beside = fresh_id("voice-beside");
    let far = fresh_id("voice-far");
    for (zone, endpoint) in [
        ("kitchen", &satellite),
        ("kitchen", &beside),
        ("study", &far),
    ] {
        server.applied(&format!(
            r#"{{"v":1,"t":"attach","zone":"{}","endpoint":"{}"}}"#,
            zone, endpoint
        ));
    }
    let events = Tap::open(&server.control, "/api/events");
    let lights = Tap::open(&server.control, "/api/visualizer?zone=kitchen");
    let mut mic = Mic::connect(&server.audio, &satellite);
    let line = server.wait_for_all(&["client session", &format!("id={} ", satellite)]);
    assert!(
        line.contains("roles=41"),
        "player, visualizer and voice: {line}"
    );
    let beside_hears = Recorder::player(&server.audio, &beside);
    let far_hears = Recorder::player(&server.audio, &far);
    let id = format!("id={} ", mic.id);

    // --- voice is off by default: the room's frames are dropped and counted --
    let state = server.state();
    assert_eq!(voice(&state, "kitchen"), (false, true), "off, and muted");
    assert_eq!(voice(&state, "study"), (false, true));
    assert!(
        state.contains(r#""roles":["player","visualizer","voice"]"#),
        "{state}"
    );
    mic.frames(5);
    let line = server.wait_for_all(&["voice mic", &id, "intake=dropping"]);
    assert!(
        line.contains("room=kitchen") && line.contains("reason=voice-disabled"),
        "{line}"
    );
    // The gate opens: the room shows it, and its frames are dropped still.
    mic.gate(MicGate::Live);
    let line = server.wait_for_all(&["voice mic", &id, "gate=live"]);
    assert!(counts(&line, 0, 5), "five dropped, none kept: {line}");
    wait_for("the room shows a live gate", Duration::from_secs(5), || {
        voice(&server.state(), "kitchen") == (false, false)
    });
    assert!(
        mic.told().is_empty(),
        "nothing asked of a room with voice off"
    );
    mic.frames(3);

    // --- voice on, the gate muted: dropped and counted -----------------------
    mic.gate(MicGate::Muted);
    let line = server.wait_for_all(&["voice mic", &id, "gate=muted"]);
    assert!(
        counts(&line, 0, 8),
        "the three more were dropped too: {line}"
    );
    server.applied(r#"{"v":2,"t":"voice_enabled","zone":"kitchen","enabled":true}"#);
    wait_for(
        "the endpoint is asked for its uplink",
        Duration::from_secs(5),
        || {
            mic.told().last()
                == Some(&VoiceControl {
                    uplink: true,
                    listening: false,
                })
        },
    );
    assert_eq!(
        voice(&server.state(), "kitchen"),
        (true, true),
        "switching voice on opens no gate"
    );
    assert_eq!(voice(&server.state(), "study"), (false, true));
    mic.frames(4);
    let line = server.wait_for_all(&["voice mic", &id, "reason=gate-muted"]);
    assert!(line.contains("room=kitchen"), "{line}");

    // --- voice on and the gate live: the frames reach the buffer -------------
    mic.gate(MicGate::Live);
    let line = server.wait_for_all(&["voice mic", &id, "gate=live", "dropped_frames=12"]);
    assert!(counts(&line, 0, 12), "{line}");
    mic.frames(6);
    let line = server.wait_for_all(&["voice mic", &id, "intake=buffering"]);
    assert!(line.contains("room=kitchen"), "{line}");

    // --- I4, with the microphone's audio flowing -----------------------------
    // Every room is playing, the microphone's own included.
    for (what, recorder) in [
        ("the satellite", &mic.hears),
        ("its neighbour", &beside_hears),
        ("the study", &far_hears),
    ] {
        recorder.until_hearing(what, Duration::from_secs(10), is_stream);
    }
    let state = server.state();
    assert_eq!(voice(&state, "kitchen"), (true, false));
    assert!(
        state.contains(r#""inputs":[]"#),
        "no input is offered: {state}"
    );
    let lower = state.to_lowercase();
    assert!(
        !lower.contains("microphone") && !lower.contains("mic:") && !lower.contains("/mic"),
        "the state names no microphone: {state}"
    );
    let sources_of = |state: &str| -> Vec<String> {
        let value = json::parse(state).unwrap();
        let Some(Value::Arr(groups)) = value.get("groups") else {
            panic!("no groups in {}", state)
        };
        groups
            .iter()
            .map(|g| g.get("source").and_then(Value::as_str).unwrap().to_string())
            .collect()
    };
    let before = sources_of(&state);
    assert!(before.iter().all(|s| s == "stream"), "{before:?}");
    // No spelling of a microphone is a source...
    for source in [
        "mic".to_string(),
        "microphone".to_string(),
        "voice".to_string(),
        format!("mic:{}", satellite),
        format!("voice:{}", satellite),
        format!("microphone:{}/mic", satellite),
        format!("mic_audio:{}", satellite),
    ] {
        for target in ["kitchen", "study"] {
            let body = format!(
                r#"{{"v":2,"t":"take","target":"{}","source":"{}"}}"#,
                target, source
            );
            let (status, answer) = server.command(&body);
            assert!(!status.contains("200"), "{body} was applied: {answer}");
            assert!(answer.contains(r#""field":"source""#), "{body}: {answer}");
        }
    }
    // ...no input role is one...
    let (status, answer) = server.command(&format!(
        r#"{{"v":2,"t":"input_label","input":"{}/mic","name":"Mic","role":"microphone"}}"#,
        satellite
    ));
    assert!(!status.contains("200"), "{answer}");
    // ...and the one spelling that parses, a line-in of the voice endpoint,
    // names an input the endpoint never offered: it plays nothing of the
    // microphone (the marker search below covers what the study heard).
    let _ = server.command(&format!(
        r#"{{"v":2,"t":"take","target":"study","source":"line-in:{}/mic"}}"#,
        satellite
    ));
    mic.frames(6);
    thread::sleep(Duration::from_millis(600));
    assert!(server.state().contains(r#""inputs":[]"#));
    server.applied(r#"{"v":2,"t":"take","target":"study","source":"stream"}"#);

    // --- the count of what reached the buffer, closed by the gate ------------
    mic.gate(MicGate::Muted);
    let line = server.wait_for_all(&["voice mic", &id, "gate=muted", "buffered_frames=12"]);
    assert!(counts(&line, 12, 12), "twelve kept, twelve dropped: {line}");
    wait_for(
        "the room shows the gate closed",
        Duration::from_secs(5),
        || voice(&server.state(), "kitchen") == (true, true),
    );

    // --- voice switched off: the endpoint is told to stop --------------------
    mic.gate(MicGate::Live);
    server.wait_for_all(&["voice mic", &id, "gate=live", "buffered_frames=12"]);
    server.applied(r#"{"v":2,"t":"voice_enabled","zone":"kitchen","enabled":false}"#);
    wait_for(
        "the endpoint is told to stop",
        Duration::from_secs(5),
        || {
            mic.told()
                == [
                    VoiceControl {
                        uplink: true,
                        listening: false,
                    },
                    VoiceControl {
                        uplink: false,
                        listening: false,
                    },
                ]
        },
    );
    let line = server.wait_for_all(&["voice control", &id, "uplink=0"]);
    assert!(counts(&line, 12, 12), "{line}");
    // An endpoint that sends on anyway is not listened to.
    mic.frames(3);
    mic.gate(MicGate::Muted);
    let line = server.wait_for_all(&["voice mic", &id, "gate=muted", "dropped_frames=15"]);
    assert!(
        counts(&line, 12, 15),
        "the three after the stop were dropped: {line}"
    );
    assert!(
        server.seen.iter().any(|l| l.contains("voice mic")
            && l.contains(&id)
            && l.contains("intake=dropping reason=voice-disabled buffered_frames=12")),
        "and said so by name: {:?}",
        server.seen
    );
    assert_eq!(voice(&server.state(), "kitchen"), (false, true));

    // --- I4: the marker is in nothing else the server put out ----------------
    thread::sleep(Duration::from_millis(300));
    let mut carried = 0usize;
    for (what, recorder) in [
        ("the satellite", &mic.hears),
        ("its neighbour", &beside_hears),
        ("the study", &far_hears),
    ] {
        let chunks = recorder.chunks();
        assert!(
            chunks.iter().filter(|c| !is_silent(c)).count() > 20,
            "{what} was playing all along"
        );
        for chunk in &chunks {
            assert!(
                !has_marker(&chunk.audio_data),
                "{what}: chunk {} carries the microphone",
                chunk.sequence
            );
        }
        carried += chunks.len();
        let messages = recorder.messages.lock().unwrap();
        for message in messages.iter() {
            assert!(
                !matches!(message, V2Message::MicAudio(_) | V2Message::AudioChunk(_)),
                "{what} was sent {:?}",
                message.message_type()
            );
            assert!(
                !has_marker(format!("{:?}", message).as_bytes()),
                "{what}: a {:?} carries the microphone",
                message.message_type()
            );
        }
    }
    assert!(carried > 150, "{carried} chunks were searched");
    let visualized = mic
        .hears
        .messages
        .lock()
        .unwrap()
        .iter()
        .filter(|m| matches!(m, V2Message::VisualizerFrame(_) | V2Message::Color(_)))
        .count();
    assert!(visualized > 0, "the visualizer stream was running");
    for (what, tap) in [("the event stream", &events), ("the light stream", &lights)] {
        let bytes = tap.bytes();
        assert!(
            bytes.starts_with(b"HTTP/1.1 200"),
            "{what}: {}",
            String::from_utf8_lossy(&bytes[..bytes.len().min(80)])
        );
        assert!(bytes.len() > 200, "{what} said something");
        assert!(!has_marker(&bytes), "{what} carries the microphone");
    }
    let said = events.bytes();
    let said = String::from_utf8_lossy(&said);
    assert!(
        said.contains(r#""voice_enabled":true"#) && said.contains(r#""mic_muted":false"#),
        "the event stream carried the two facts, and nothing of the audio"
    );
    for body in [
        server.state(),
        common::http(
            &server.control,
            "GET /metrics HTTP/1.1\r\nHost: chorus\r\nConnection: close\r\n\r\n",
        )
        .1,
    ] {
        assert!(!has_marker(body.as_bytes()));
    }
    server.drain();
    for line in &server.seen {
        assert!(
            !has_marker(line.as_bytes()),
            "a log line carries it: {line}"
        );
        // What is said of the microphone is words and counts.
        if line.contains("voice mic") || line.contains("voice control") {
            let text = line.split_once("voice ").unwrap().1;
            for token in text.split_whitespace().skip(1) {
                let known = [
                    "id=",
                    "room=",
                    "intake=",
                    "reason=",
                    "gate=",
                    "uplink=",
                    "listening=",
                    "buffered_frames=",
                    "dropped_frames=",
                    "ended",
                    // What the server appends to a status line.
                    "scheduling=",
                    "rtprio_",
                    "memory=",
                    "memlock_",
                ];
                assert!(
                    known.iter().any(|k| token.starts_with(k)),
                    "{token:?} in {line:?}"
                );
            }
        }
    }
    let files = files_under(&dir);
    assert!(
        files.iter().any(|(p, _)| p.ends_with("zones.state")),
        "the state file was written: {files:?}"
    );
    for (path, bytes) in &files {
        assert!(
            !has_marker(bytes),
            "{} carries the microphone",
            path.display()
        );
    }
    // The session ends: its counts are the last word.
    let Mic { writer, hears, .. } = mic;
    drop(hears);
    drop(writer);
    let line = server.wait_for_all(&["voice mic", &id, "ended"]);
    assert!(counts(&line, 12, 15), "{line}");
    drop(events);
    drop(lights);
    drop(server);
    for (path, bytes) in files_under(&dir) {
        assert!(
            !has_marker(&bytes),
            "{} carries the microphone",
            path.display()
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&source);
}

#[test]
fn mic_audio_from_a_session_without_the_voice_role_or_in_no_room_is_dropped() {
    let dir = scratch("refused");
    let source = constant_source("voice-refused");
    let mut server = server(&dir, &source);
    let plain = fresh_id("voice-plain");
    let roomless = fresh_id("voice-roomless");
    server.applied(&format!(
        r#"{{"v":1,"t":"attach","zone":"kitchen","endpoint":"{}"}}"#,
        plain
    ));
    server.applied(r#"{"v":2,"t":"voice_enabled","zone":"kitchen","enabled":true}"#);
    // A player in a room with voice on, which never declared the voice role.
    let (session, messages) = Player::connect(&server.audio, &plain, 0).split();
    let mut writer = session.writer;
    let _hears = Recorder::reading(session.reader, messages);
    server.wait_for_all(&["client session", &format!("id={} ", plain)]);
    writer
        .send(&V2Message::MicState(MicState {
            gate: MicGate::Live,
        }))
        .unwrap();
    server.wait_for_all(&[
        "voice mic",
        &format!("id={} ", plain),
        "mic_state refused reason=no-voice-role",
    ]);
    writer.send(&mic_frame(0)).unwrap();
    let line = server.wait_for_all(&["voice mic", &format!("id={} ", plain), "intake=dropping"]);
    assert!(line.contains("reason=no-voice-role"), "{line}");
    assert_eq!(
        voice(&server.state(), "kitchen"),
        (true, true),
        "its word about a gate is not the room's"
    );
    // A voice endpoint in no room: there is no room to have voice on.
    let mut mic = Mic::connect(&server.audio, &roomless);
    server.wait_for_all(&["client session", &format!("id={} ", roomless)]);
    mic.gate(MicGate::Live);
    mic.frames(2);
    let line = server.wait_for_all(&["voice mic", &format!("id={} ", roomless), "intake=dropping"]);
    assert!(
        line.contains("room=- ") && line.contains("reason=no-room"),
        "{line}"
    );
    assert!(mic.told().is_empty());
    drop(mic);
    drop(server);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&source);
}

#[test]
fn voice_is_off_in_every_room_by_default_and_survives_a_restart() {
    let dir = scratch("restart");
    let source = constant_source("voice-restart");
    let satellite = fresh_id("voice-kept");
    {
        let mut server = server(&dir, &source);
        for room in ["kitchen", "study"] {
            assert_eq!(voice(&server.state(), room), (false, true), "{room}");
        }
        server.applied(&format!(
            r#"{{"v":1,"t":"attach","zone":"kitchen","endpoint":"{}"}}"#,
            satellite
        ));
        server.applied(r#"{"v":2,"t":"voice_enabled","zone":"kitchen","enabled":true}"#);
        let mut mic = Mic::connect(&server.audio, &satellite);
        mic.gate(MicGate::Live);
        server.wait_for_all(&["voice mic", "gate=live"]);
        wait_for("voice on and the gate live", Duration::from_secs(5), || {
            voice(&server.state(), "kitchen") == (true, false)
        });
        let saved = std::fs::read_to_string(dir.join("zones.state")).unwrap();
        assert!(saved.contains("format = 10\n"), "{saved}");
        assert_eq!(saved.matches("voice_enabled = 1\n").count(), 1, "{saved}");
        assert_eq!(saved.matches("voice_enabled = 0\n").count(), 1, "{saved}");
    }
    // The server that replaces it reads the file once, at start.
    let server = server(&dir, &source);
    assert_eq!(
        voice(&server.state(), "kitchen"),
        (true, true),
        "still on, and muted until a microphone says otherwise"
    );
    assert_eq!(
        voice(&server.state(), "study"),
        (false, true),
        "the default"
    );
    server.applied(r#"{"v":2,"t":"voice_enabled","zone":"kitchen","enabled":false}"#);
    assert_eq!(voice(&server.state(), "kitchen"), (false, true));
    drop(server);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&source);
}
