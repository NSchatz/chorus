//! The wake word, the voice run and the run-scoped microphone route on the
//! real binary (proposal P8, Option A; `crates/server/src/voice.rs`;
//! `docs/control-plane.md`, "Voice: the wake word, the run and its audio").
//!
//! A scripted voice speaker (a protocol v2 session through the Linux
//! client's own session code, declaring the `voice` role) sends `mic_state`
//! and `mic_audio` to a real `chorus-server`, and the test is the home
//! automation: it reads `GET /api/voice-events`, sends `voice_start` and
//! `voice_stop`, and reads `GET /api/voice-audio`.
//!
//! The test names are the evidence:
//!
//! - the wake fixture (`fixtures/wakeword/okay-nabu.wav`) sent as microphone
//!   audio is one `voice_wake` for its room; a `voice_start` answers with a
//!   run identifier; the route then streams what the speaker sends;
//! - the route refuses with no run open, a wrong identifier, a second reader
//!   and a caller from another address; the identifier is in no event
//!   stream, no state, no report and no log line; the run ends when stopped,
//!   at its limit, on mute and when voice is switched off;
//! - a `voice_start` with no wake word before it (start conversation) opens
//!   a run, and one in a muted or a switched-off room is refused by name.
//!
//! The loopback interface gives a test one source address, so "a caller from
//! another address" is shown from the other side: a server started with
//! another address as its integration's refuses this one, with the right
//! identifier of an open, unread run in hand.
//!
//! The limit is the server's own monotonic clock's
//! (`std::time::Instant`); what this file measures of it, with its own, is
//! that the run was still open before the limit and ended by it within
//! scheduling slack. The exact boundary is `voice.rs`'s unit test, which
//! hands the instants in. Nothing here is timing evidence about audio. Wall
//! clock: about 3 seconds for the file.

mod common;

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use chorus_client_linux::config::ClientConfig;
use chorus_control::json::{self, Value};
use chorus_protocol::v2::session::SecureWriter;
use chorus_protocol::v2::{
    mic_format, roles, Message as V2Message, MicAudio, MicGate, MicState, VoiceControl,
};
use common::line_in::{constant_source, wait_for, Recorder};
use common::{fresh_id, Player, RunningServer};

/// The address the test reads the route from.
const HERE: &str = "127.0.0.1";

/// An address that is not this test's (TEST-NET-1, RFC 5737).
const ELSEWHERE: &str = "192.0.2.7";

/// "Okay Nabu", 2 s of 16 kHz mono 16-bit PCM, without its WAV header.
fn wake_fixture() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/wakeword/okay-nabu.wav");
    let wav = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {}", path.display(), e));
    assert_eq!(&wav[..4], b"RIFF");
    assert_eq!(&wav[36..40], b"data", "a 44-byte header");
    wav[44..].to_vec()
}

/// `frames` nominal 20 ms frames a wake-word model has nothing to say about,
/// each sample naming where it is in the whole: what the route delivers can
/// be held to it byte for byte.
fn speech(tag: u8, frames: usize) -> Vec<u8> {
    (0..frames * 320)
        .flat_map(|n| [tag, (n % 251) as u8])
        .collect()
}

fn scratch(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("chorus-voice-run-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The server on `dir`, serving `kitchen` and `study`, with `extra` flags.
fn server(dir: &Path, source: &Path, extra: &[&str]) -> RunningServer {
    let identity = dir.join("identity");
    let mut flags = vec![
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
    ];
    flags.extend_from_slice(extra);
    RunningServer::start_on(
        "127.0.0.1:0",
        &["--identity-dir", identity.to_str().unwrap()],
        &flags,
    )
}

/// A raw `GET` on the control plane, read to its end on a thread: the head,
/// the body as bytes, and whether the server has closed it.
struct Stream {
    bytes: Arc<Mutex<Vec<u8>>>,
    closed: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl Stream {
    fn open(control: &str, target: &str) -> Stream {
        let mut socket = TcpStream::connect(control).expect("the control plane listens");
        write!(
            socket,
            "GET {} HTTP/1.1\r\nHost: chorus\r\nConnection: close\r\n\r\n",
            target
        )
        .unwrap();
        socket
            .set_read_timeout(Some(Duration::from_millis(20)))
            .unwrap();
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let closed = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let join = {
            let (bytes, closed, stop) =
                (Arc::clone(&bytes), Arc::clone(&closed), Arc::clone(&stop));
            thread::spawn(move || {
                let mut scratch = [0u8; 8_192];
                while !stop.load(Ordering::SeqCst) {
                    match socket.read(&mut scratch) {
                        Ok(0) => break,
                        Ok(n) => bytes.lock().unwrap().extend_from_slice(&scratch[..n]),
                        Err(e)
                            if matches!(
                                e.kind(),
                                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                            ) => {}
                        Err(_) => break,
                    }
                }
                closed.store(true, Ordering::SeqCst);
            })
        };
        Stream {
            bytes,
            closed,
            stop,
            join: Some(join),
        }
    }

    fn all(&self) -> Vec<u8> {
        self.bytes.lock().unwrap().clone()
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.all()).to_string()
    }

    /// The response's head, once it has arrived whole.
    fn head(&self) -> String {
        wait_for("the response's head", Duration::from_secs(5), || {
            self.all().windows(4).any(|w| w == b"\r\n\r\n")
        });
        let all = self.all();
        let end = all.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        String::from_utf8_lossy(&all[..end]).to_string()
    }

    /// Everything after the head.
    fn body(&self) -> Vec<u8> {
        let all = self.all();
        match all.windows(4).position(|w| w == b"\r\n\r\n") {
            Some(end) => all[end + 4..].to_vec(),
            None => Vec::new(),
        }
    }

    fn closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    fn until_closed(&self, what: &str) {
        wait_for(what, Duration::from_secs(5), || self.closed());
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

/// The scripted voice speaker.
struct Mic {
    id: String,
    writer: SecureWriter<TcpStream>,
    hears: Recorder,
    sequence: u32,
}

impl Mic {
    /// A voice speaker of `room`, up, with voice switched on there and its
    /// gate live.
    fn listening_in(server: &mut RunningServer, room: &str, stem: &str) -> Mic {
        let id = fresh_id(stem);
        server.applied(&format!(
            r#"{{"v":1,"t":"attach","zone":"{}","endpoint":"{}"}}"#,
            room, id
        ));
        server.applied(&format!(
            r#"{{"v":2,"t":"voice_enabled","zone":"{}","enabled":true}}"#,
            room
        ));
        let config = ClientConfig {
            extra_roles: roles::VOICE,
            ..ClientConfig::default()
        };
        let (session, messages) = Player::connect_with(&server.audio, &id, &config).split();
        let mut mic = Mic {
            id,
            writer: session.writer,
            hears: Recorder::reading(session.reader, messages),
            sequence: 0,
        };
        server.wait_for_all(&["client session", &format!("id={} ", mic.id)]);
        mic.gate(MicGate::Live);
        let control = server.control.clone();
        wait_for("the room shows a live gate", Duration::from_secs(5), || {
            !muted(&common::http(&control, STATE).1, room)
        });
        wait_for(
            "the speaker is asked for its uplink",
            Duration::from_secs(5),
            || mic.told().last().is_some_and(|c| c.uplink),
        );
        mic
    }

    fn gate(&mut self, gate: MicGate) {
        self.writer
            .send(&V2Message::MicState(MicState { gate }))
            .expect("mic_state is sent");
    }

    /// Send `pcm` as nominal 20 ms `mic_audio` frames.
    fn say(&mut self, pcm: &[u8]) {
        for data in pcm.chunks(640) {
            self.writer
                .send(&V2Message::MicAudio(MicAudio {
                    format: mic_format::PCM_S16LE_16K_MONO,
                    sequence: self.sequence,
                    timestamp_ns: u64::from(self.sequence) * 20_000_000,
                    data: data.to_vec(),
                }))
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

    /// Wait until the speaker's last word from the server says the room is
    /// (or is not) being listened to.
    fn until_listening(&self, listening: bool) {
        wait_for(
            if listening {
                "the speaker is told its room is listened to"
            } else {
                "the speaker is told its room is no longer listened to"
            },
            Duration::from_secs(5),
            || self.told().last().is_some_and(|c| c.listening == listening),
        );
    }
}

const STATE: &str = "GET /api/state HTTP/1.1\r\nHost: chorus\r\nConnection: close\r\n\r\n";

/// A room's `mic_muted`, off a state message.
fn muted(state: &str, id: &str) -> bool {
    let value = json::parse(state).unwrap_or_else(|e| panic!("{}: {:?}", state, e));
    let Some(Value::Arr(zones)) = value.get("zones") else {
        panic!("no zones in {}", state)
    };
    zones
        .iter()
        .find(|z| z.get("id").and_then(Value::as_str) == Some(id))
        .and_then(|z| z.get("mic_muted"))
        .and_then(Value::as_bool)
        .unwrap_or_else(|| panic!("no mic_muted for {} in {}", id, state))
}

/// `voice_start` in `room`, which must open a run: its identifier and limit.
fn start(server: &RunningServer, room: &str) -> (String, i64) {
    let answer = server.applied(&format!(r#"{{"v":2,"t":"voice_start","zone":"{}"}}"#, room));
    let value = json::parse(&answer).unwrap_or_else(|e| panic!("{}: {:?}", answer, e));
    assert_eq!(
        value.get("t").and_then(Value::as_str),
        Some("voice_run"),
        "the answer is the run, not a state: {answer}"
    );
    assert_eq!(value.get("zone").and_then(Value::as_str), Some(room));
    let run = value
        .get("run")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("no run in {}", answer))
        .to_string();
    assert!(
        run.len() == 32 && run.bytes().all(|b| b.is_ascii_hexdigit()),
        "32 hexadecimal digits: {run}"
    );
    let limit: i64 = value
        .get("limit_ms")
        .and_then(Value::as_num)
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("no limit_ms in {}", answer));
    (run, limit)
}

/// A request the route must refuse: the status line and the body.
fn refused(server: &RunningServer, target: &str) -> (String, String) {
    let (status, body) = common::http(
        &server.control,
        &format!(
            "GET {} HTTP/1.1\r\nHost: chorus\r\nConnection: close\r\n\r\n",
            target
        ),
    );
    assert!(!status.contains("200"), "{target} was served: {status}");
    assert!(
        !body.contains("\u{0}") && body.len() < 400,
        "a refusal is words: {body:?}"
    );
    (status, body)
}

/// How many of the server's lines so far contain every one of `what`.
fn said(server: &mut RunningServer, what: &[&str]) -> usize {
    server.drain();
    server
        .seen
        .iter()
        .filter(|l| what.iter().all(|w| l.contains(w)))
        .count()
}

/// Wait until the server has said `count` lines containing every one of
/// `what`, and give the last back.
fn until_said(server: &mut RunningServer, what: &[&str], count: usize) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    while said(server, what) < count {
        assert!(
            Instant::now() < deadline,
            "the server never said {:?} {} times; it said:\n{}",
            what,
            count,
            server.seen.join("\n")
        );
        thread::sleep(Duration::from_millis(10));
    }
    server
        .seen
        .iter()
        .filter(|l| what.iter().all(|w| l.contains(w)))
        .nth(count - 1)
        .unwrap()
        .clone()
}

fn count_of(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

#[test]
fn a_wake_word_is_one_event_a_start_answers_with_a_run_and_the_route_streams_it() {
    let dir = scratch("run");
    let source = constant_source("voice-run");
    let mut server = server(&dir, &source, &["--voice-integration", HERE]);
    let state = server.state();
    assert!(
        state.ends_with(r#","wake_words":[{"id":"okay_nabu","phrase":"Okay Nabu"}]}"#),
        "the state lists the server's wake-word models: {state}"
    );
    let events = Stream::open(&server.control, "/api/events");
    let wakes = Stream::open(&server.control, "/api/voice-events");
    let presses = Stream::open(&server.control, "/api/controller-events");
    assert!(wakes.head().starts_with("HTTP/1.1 200"), "{}", wakes.head());
    wait_for("the opening comment line", Duration::from_secs(5), || {
        !wakes.body().is_empty()
    });
    assert_eq!(
        String::from_utf8_lossy(&wakes.body()),
        ": voice events\n\n",
        "it opens with a comment line and no event"
    );
    let mut mic = Mic::listening_in(&mut server, "kitchen", "voice-run-ep");
    let mut runs: Vec<String> = Vec::new();

    // --- no run is open: the route refuses, whatever is offered --------------
    let (status, body) = refused(&server, &format!("/api/voice-audio?run={}", "0".repeat(32)));
    assert!(status.contains("404"), "{status}");
    assert!(body.contains("no-voice-run"), "{body}");
    let (status, body) = refused(&server, "/api/voice-audio");
    assert!(status.contains("400"), "{status}");
    assert!(body.contains("no-run-named"), "{body}");

    // --- the wake fixture is one wake event for its room ---------------------
    let fixture = wake_fixture();
    mic.say(&fixture);
    let line = until_said(&mut server, &["voice wake room=kitchen"], 1);
    assert!(
        line.contains(&format!("id={} ", mic.id)) && line.contains(r#"phrase="Okay Nabu""#),
        "{line}"
    );
    let wake = r#"data: {"v":2,"t":"voice_wake","zone":"kitchen","phrase":"Okay Nabu"}"#;
    wait_for("the wake event", Duration::from_secs(5), || {
        wakes.text().contains(wake)
    });
    assert!(
        !mic.told().iter().any(|c| c.listening),
        "a wake word opens no run: {:?}",
        mic.told()
    );

    // --- a start command answers with a run, and the route streams it --------
    // The start of the command, said before the home automation has answered
    // the wake word.
    let early = speech(0xA1, 5);
    mic.say(&early);
    let (run, limit) = start(&server, "kitchen");
    runs.push(run.clone());
    assert_eq!(limit, 30_000, "the default limit");
    mic.until_listening(true);
    let line = until_said(&mut server, &["voice run room=kitchen", "started"], 1);
    assert!(line.contains("after_wake=1"), "{line}");
    let audio = Stream::open(&server.control, &format!("/api/voice-audio?run={}", run));
    let head = audio.head();
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    assert!(
        head.contains("Content-Type: application/octet-stream")
            && head.contains("X-Chorus-Audio-Format: pcm_s16le; rate=16000; channels=1"),
        "{head}"
    );
    let rest = speech(0xA2, 20);
    mic.say(&rest);
    let command = [early.clone(), rest.clone()].concat();
    wait_for("the command's audio", Duration::from_secs(5), || {
        audio.body().ends_with(&command)
    });
    // What came before the command is the end of the recording, after the
    // phrase, and nothing else: the route carries what the speaker sent, in
    // order, from the wake word on.
    let body = audio.body();
    let sent = [fixture.clone(), command.clone()].concat();
    assert!(sent.ends_with(&body), "{} bytes", body.len());
    assert!(
        body.len() - command.len() < fixture.len() / 2,
        "from the wake word, not from before it: {} bytes of the recording",
        body.len() - command.len()
    );
    assert_eq!(body.len() % 2, 0, "whole samples");

    // --- a wrong identifier and a second reader are refused ------------------
    let mut wrong = run.clone().into_bytes();
    wrong[31] = if wrong[31] == b'0' { b'1' } else { b'0' };
    let wrong = String::from_utf8(wrong).unwrap();
    let (status, body) = refused(&server, &format!("/api/voice-audio?run={}", wrong));
    assert!(status.contains("404"), "{status}");
    assert!(body.contains("no-voice-run"), "{body}");
    let (status, body) = refused(&server, &format!("/api/voice-audio?run={}", run));
    assert!(status.contains("409"), "{status}");
    assert!(body.contains("voice-run-taken"), "{body}");
    // Neither took anything from the reader the run has.
    let more = speech(0xA3, 5);
    mic.say(&more);
    wait_for("the run goes on", Duration::from_secs(5), || {
        audio.body().ends_with(&more)
    });
    assert!(!audio.closed());
    until_said(&mut server, &["voice route refused reason=no-voice-run"], 2);
    until_said(
        &mut server,
        &["voice route refused reason=voice-run-taken"],
        1,
    );

    // --- the stop command ends it --------------------------------------------
    let answer = server.applied(r#"{"v":2,"t":"voice_stop","zone":"kitchen"}"#);
    assert!(answer.starts_with(r#"{"v":2,"t":"state""#), "{answer}");
    audio.until_closed("the stream ends with the run");
    let line = until_said(&mut server, &["voice run room=kitchen", "ended"], 1);
    assert!(
        line.contains("reason=stopped") && line.contains("read=1"),
        "{line}"
    );
    mic.until_listening(false);
    let (status, _) = refused(&server, &format!("/api/voice-audio?run={}", run));
    assert!(status.contains("404"), "an ended run is no run: {status}");
    // Stopping a room with no run is not an error; a room that is not is.
    server.applied(r#"{"v":2,"t":"voice_stop","zone":"kitchen"}"#);
    let (status, answer) = server.command(r#"{"v":2,"t":"voice_stop","zone":"attic"}"#);
    assert!(
        status.contains("400") && answer.contains(r#""field":"zone""#),
        "{answer}"
    );

    // --- start conversation: a run with no wake word before it ---------------
    let (run, _) = start(&server, "kitchen");
    runs.push(run.clone());
    assert_ne!(runs[0], runs[1], "an identifier is made new for every run");
    let line = until_said(&mut server, &["voice run room=kitchen", "started"], 2);
    assert!(line.contains("after_wake=0"), "{line}");
    let audio = Stream::open(&server.control, &format!("/api/voice-audio?run={}", run));
    assert!(audio.head().starts_with("HTTP/1.1 200"));
    mic.until_listening(true);
    let reply = speech(0xB1, 15);
    mic.say(&reply);
    wait_for("the reply's audio", Duration::from_secs(5), || {
        audio.body().len() >= reply.len()
    });
    assert_eq!(
        audio.body(),
        reply,
        "exactly what was said after the start, and nothing from before it"
    );

    // --- mute ends the run, and a muted room refuses a start by name ---------
    mic.gate(MicGate::Muted);
    audio.until_closed("the stream ends when the microphone is muted");
    until_said(
        &mut server,
        &["voice run room=kitchen", "ended reason=muted"],
        1,
    );
    mic.until_listening(false);
    wait_for(
        "the room shows the gate closed",
        Duration::from_secs(5),
        || muted(&server.state(), "kitchen"),
    );
    let (status, answer) = server.command(r#"{"v":2,"t":"voice_start","zone":"kitchen"}"#);
    assert!(status.contains("400"), "{status} {answer}");
    assert!(
        answer.contains(r#""field":"zone""#) && answer.contains("mic-muted: "),
        "{answer}"
    );
    assert!(!answer.contains("\"run\""), "{answer}");
    // A room with no microphone at all is the same answer.
    server.applied(r#"{"v":2,"t":"voice_enabled","zone":"study","enabled":true}"#);
    let (_, answer) = server.command(r#"{"v":2,"t":"voice_start","zone":"study"}"#);
    assert!(answer.contains("mic-muted: "), "{answer}");

    // --- voice switched off ends the run, and refuses a start by name --------
    mic.gate(MicGate::Live);
    wait_for("the room shows a live gate", Duration::from_secs(5), || {
        !muted(&server.state(), "kitchen")
    });
    let (run, _) = start(&server, "kitchen");
    runs.push(run.clone());
    let audio = Stream::open(&server.control, &format!("/api/voice-audio?run={}", run));
    assert!(audio.head().starts_with("HTTP/1.1 200"));
    let last = speech(0xC1, 5);
    mic.say(&last);
    wait_for("the run's audio", Duration::from_secs(5), || {
        audio.body() == last
    });
    server.applied(r#"{"v":2,"t":"voice_enabled","zone":"kitchen","enabled":false}"#);
    audio.until_closed("the stream ends when voice is switched off");
    until_said(
        &mut server,
        &["voice run room=kitchen", "ended reason=voice-disabled"],
        1,
    );
    let (status, answer) = server.command(r#"{"v":2,"t":"voice_start","zone":"kitchen"}"#);
    assert!(status.contains("400"), "{status} {answer}");
    assert!(
        answer.contains(r#""field":"zone""#) && answer.contains("voice-disabled: "),
        "{answer}"
    );
    let (_, answer) = server.command(r#"{"v":2,"t":"voice_start","zone":"attic"}"#);
    assert!(answer.contains(r#""field":"zone""#), "{answer}");
    // The commands are catalog version 2's.
    let (_, answer) = server.command(r#"{"v":1,"t":"voice_start","zone":"kitchen"}"#);
    assert!(answer.contains(r#""field":"t""#), "{answer}");

    // --- one wake event in all, and the identifiers are nowhere --------------
    assert_eq!(
        count_of(&wakes.text(), "voice_wake"),
        1,
        "one wake event: {}",
        wakes.text()
    );
    assert!(
        !events.text().contains("voice_wake") && !presses.text().contains("voice_wake"),
        "and on its own stream alone"
    );
    assert_eq!(runs.len(), 3);
    server.drain();
    let report = common::http(
        &server.control,
        "GET /api/report HTTP/1.1\r\nHost: chorus\r\nConnection: close\r\n\r\n",
    )
    .1;
    let metrics = common::http(
        &server.control,
        "GET /metrics HTTP/1.1\r\nHost: chorus\r\nConnection: close\r\n\r\n",
    )
    .1;
    assert!(events.text().len() > 2_000, "the event stream said a lot");
    for run in &runs {
        for (what, text) in [
            ("the event stream", events.text()),
            ("the voice event stream", wakes.text()),
            ("the controller event stream", presses.text()),
            ("the state", server.state()),
            ("the v1 state", {
                common::http(
                    &server.control,
                    "GET /api/state?v=1 HTTP/1.1\r\nHost: chorus\r\nConnection: close\r\n\r\n",
                )
                .1
            }),
            ("the report", report.clone()),
            ("the metrics", metrics.clone()),
            ("the log", server.seen.join("\n")),
        ] {
            assert!(!text.contains(run.as_str()), "{what} names a run");
            assert!(
                !text.contains("\"run\"") && !text.contains("voice_run"),
                "{what} carries a run"
            );
        }
    }
    // And no microphone sample is on an event stream.
    for stream in [&events, &wakes, &presses] {
        let bytes = stream.all();
        for tag in [0xA1u8, 0xA2, 0xB1, 0xC1] {
            assert!(
                !bytes
                    .windows(8)
                    .any(|w| w.iter().step_by(2).all(|b| *b == tag)),
                "an event stream carries microphone audio"
            );
        }
    }
    drop(mic);
    drop(server);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&source);
}

#[test]
fn a_run_ends_at_its_time_limit() {
    let dir = scratch("limit");
    let source = constant_source("voice-run-limit");
    const LIMIT_MS: u64 = 1_200;
    let mut server = server(
        &dir,
        &source,
        &["--voice-integration", HERE, "--voice-run-limit-ms", "1200"],
    );
    let mut mic = Mic::listening_in(&mut server, "kitchen", "voice-limit-ep");
    // Taken before the command is sent, so the run cannot have started
    // earlier than this.
    let asked = Instant::now();
    let (run, limit) = start(&server, "kitchen");
    assert_eq!(limit, LIMIT_MS as i64, "the answer says how long it has");
    let audio = Stream::open(&server.control, &format!("/api/voice-audio?run={}", run));
    assert!(audio.head().starts_with("HTTP/1.1 200"));
    mic.until_listening(true);
    // The speaker talks on, past the limit.
    let mut sent = Vec::new();
    while !audio.closed() {
        assert!(
            asked.elapsed() < Duration::from_secs(10),
            "the run outlived its limit"
        );
        let frame = speech(0xD1, 1);
        mic.say(&frame);
        sent.extend_from_slice(&frame);
        thread::sleep(Duration::from_millis(20));
    }
    let ended = asked.elapsed();
    assert!(
        ended >= Duration::from_millis(LIMIT_MS),
        "open until its limit: it ended after {ended:?}"
    );
    assert!(
        ended < Duration::from_millis(LIMIT_MS) + Duration::from_secs(3),
        "and not held past it: it ended after {ended:?}"
    );
    let line = until_said(&mut server, &["voice run room=kitchen", "ended"], 1);
    assert!(
        line.contains("reason=limit") && line.contains("read=1"),
        "{line}"
    );
    mic.until_listening(false);
    let body = audio.body();
    assert!(
        body.len() > 10_000 && sent.starts_with(&body),
        "what was said until then was served, in order: {} of {} bytes",
        body.len(),
        sent.len()
    );
    // Past the limit there is no run, though the speaker is still talking.
    mic.say(&speech(0xD2, 3));
    let (status, body) = refused(&server, &format!("/api/voice-audio?run={}", run));
    assert!(
        status.contains("404") && body.contains("no-voice-run"),
        "{status} {body}"
    );
    drop(mic);
    drop(server);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&source);
}

#[test]
fn the_route_is_served_to_the_registered_address_and_to_no_other() {
    // A server whose integration is somewhere else: this test's address is
    // "another address", and it holds the right identifier of an open run
    // nobody is reading.
    let dir = scratch("address");
    let source = constant_source("voice-run-address");
    let mut server = server(&dir, &source, &["--voice-integration", ELSEWHERE]);
    server.wait_for(&format!("voice integration address={}", ELSEWHERE));
    let mut mic = Mic::listening_in(&mut server, "kitchen", "voice-address-ep");
    let (run, _) = start(&server, "kitchen");
    mic.until_listening(true);
    mic.say(&speech(0xE1, 5));
    let (status, body) = refused(&server, &format!("/api/voice-audio?run={}", run));
    assert!(status.contains("403"), "{status}");
    assert!(body.contains("not-the-voice-integration"), "{body}");
    let line = until_said(&mut server, &["voice route refused"], 1);
    assert!(
        line.contains("reason=not-the-voice-integration") && line.contains("peer=127.0.0.1"),
        "{line}"
    );
    // The answer is the same with no run named and with a wrong one: who is
    // asking is settled first.
    for target in ["/api/voice-audio", "/api/voice-audio?run=nothing"] {
        let (status, _) = refused(&server, target);
        assert!(status.contains("403"), "{target}: {status}");
    }
    // The refused caller took nothing: the run is open and unread still.
    assert!(mic.told().last().is_some_and(|c| c.listening));
    server.applied(r#"{"v":2,"t":"voice_stop","zone":"kitchen"}"#);
    let line = until_said(&mut server, &["voice run room=kitchen", "ended"], 1);
    assert!(
        line.contains("reason=stopped")
            && line.contains("read=0")
            && line.contains("served_bytes=0"),
        "{line}"
    );
    server.drain();
    assert!(
        !server.seen.join("\n").contains(&run),
        "the log names a run"
    );
    drop(mic);
    drop(server);

    // A server told of no integration opens no run at all: nothing could
    // read it.
    let mut server = self::server(&dir, &source, &[]);
    let mic = Mic::listening_in(&mut server, "kitchen", "voice-nobody-ep");
    let (status, answer) = server.command(r#"{"v":2,"t":"voice_start","zone":"kitchen"}"#);
    assert!(status.contains("400"), "{status} {answer}");
    assert!(
        answer.contains(r#""field":"t""#) && answer.contains("no-voice-integration: "),
        "{answer}"
    );
    assert!(!mic.told().iter().any(|c| c.listening));
    let (status, _) = refused(&server, &format!("/api/voice-audio?run={}", run));
    assert!(status.contains("403"), "{status}");
    drop(mic);
    drop(server);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&source);
}
