//! The visualizer stream end to end (goal 12, done-when line E; K65;
//! `docs/visualizer.md`).
//!
//! The real `chorus-server` (`--slots 2`) plays a real named pipe
//! (`fifo:<path>`, the development input of `crate::source`). Four
//! endpoints speak protocol v2 through the Linux client's own session code
//! (`chorus_client_linux::session::open`): in the den, one that asks for 16
//! visualizer bands (`ClientConfig::visualizer_bands`, the client's
//! `--visualizer-bands`, which declares the `visualizer` role) and one plain
//! player; in the study (declared wireless) one more that asks for bands;
//! and one that asks for bands but is in no room. Once all four are up, the committed fixture `fixtures/visualizer/01-kick-120bpm.wav`
//! (four kicks 500 ms apart, mono, duplicated into the server's two
//! channels) is written into the pipe.
//!
//! What is checked, from what the endpoints received and nothing else:
//!
//! - where the fixture is on the server timeline: the first non-zero sample
//!   of the audio the den's visualizer endpoint received, by its chunk's
//!   timestamp, and the fixture's every sample after it in order (so no gap
//!   moved a kick);
//! - the frames: exactly one beat per kick, each `visualizer_frame` with a
//!   beat stamped within the stated tolerance of that kick's onset on the
//!   server timeline plus the wired playout latency (when the den hears it),
//!   each carrying the 16 bands asked for, and at least one `color`;
//! - a visualizer endpoint in the study, a wireless room playing the same
//!   stream on the other slot, receives the same beats stamped after the
//!   wireless tier's playout latency instead;
//! - the plain player received audio and not one `visualizer_frame` or
//!   `color`, and neither did the endpoint in no room.
//!
//! The tolerance is the fixture's own (`beat_tolerance_ms`, which the
//! offline analysis meets on the fixture's own hop grid) plus one analysis
//! hop: through the server the hop grid is the stream's, and the fixture can
//! start anywhere in a hop. Nothing here is timing evidence: the stamps are
//! compared with stamps, on one machine, and what is graded is the
//! arithmetic that places a frame on the timeline.

mod common;

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use chorus_client_linux::config::ClientConfig;
use chorus_control::transport::Transport;
use chorus_dsp::visualizer::HOP_MS;
use chorus_protocol::v2::Message as V2Message;
use chorus_protocol::{decode_frame, AudioChunk, FrameOutcome, Message};
use chorus_server::conductor::{heard_latency_ns, WIRED_GROUP_LATENCY_NS};
use common::{fresh_id, Player, RunningServer};

const RATE_HZ: u64 = 48_000;
const BANDS: u8 = 16;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/visualizer")
}

/// A `key = value` from the fixture's parameters.
fn param(key: &str) -> String {
    let text = std::fs::read_to_string(fixtures().join("01-kick-120bpm.params")).unwrap();
    text.lines()
        .map(|l| l.split('#').next().unwrap())
        .filter_map(|l| l.split_once('='))
        .find(|(k, _)| k.trim() == key)
        .map(|(_, v)| v.trim().to_string())
        .unwrap_or_else(|| panic!("01-kick-120bpm.params has no {}", key))
}

/// The fixture's samples (mono 16-bit, 48 kHz).
fn kick() -> Vec<i16> {
    let bytes = std::fs::read(fixtures().join("01-kick-120bpm.wav")).unwrap();
    assert_eq!(&bytes[36..40], b"data", "a canonical WAV");
    assert_eq!(
        u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
        48_000
    );
    bytes[44..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect()
}

/// A session's chunks and v2 messages, read on a thread of its own.
struct Recorder {
    chunks: Arc<Mutex<Vec<AudioChunk>>>,
    messages: Arc<Mutex<Vec<V2Message>>>,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl Recorder {
    fn start(player: Player) -> Recorder {
        let (session, messages) = player.split();
        let mut reader = session.reader;
        let chunks = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let join = {
            let chunks = Arc::clone(&chunks);
            let stop = Arc::clone(&stop);
            thread::spawn(move || {
                let mut pending: Vec<u8> = Vec::new();
                let mut scratch = vec![0u8; 65_536];
                while !stop.load(Ordering::SeqCst) {
                    match reader.read(&mut scratch) {
                        Ok(0) => return,
                        Ok(n) => pending.extend_from_slice(&scratch[..n]),
                        Err(_) => continue,
                    }
                    let mut at = 0usize;
                    while at < pending.len() {
                        let d = decode_frame(&pending[at..]);
                        if d.consumed == 0 {
                            break;
                        }
                        at += d.consumed;
                        if let FrameOutcome::Decoded(Message::AudioChunk(c)) = d.outcome {
                            chunks.lock().unwrap().push(c);
                        }
                    }
                    pending.drain(..at);
                }
            })
        };
        Recorder {
            chunks,
            messages,
            stop,
            join: Some(join),
        }
    }

    fn finish(mut self) -> (Vec<AudioChunk>, Vec<V2Message>) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
        let chunks = self.chunks.lock().unwrap().clone();
        let messages = self.messages.lock().unwrap().clone();
        (chunks, messages)
    }
}

fn visualizer_messages(messages: &[V2Message]) -> usize {
    messages
        .iter()
        .filter(|m| matches!(m, V2Message::VisualizerFrame(_) | V2Message::Color(_)))
        .count()
}

#[test]
fn beats_from_fixture_audio_reach_a_visualizer_endpoint_on_the_server_timeline() {
    let dir = std::env::temp_dir().join(format!("chorus-visualizer-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let fifo = dir.join("pcm.fifo");
    assert!(Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("mkfifo runs")
        .success());
    let mut server = RunningServer::start(&[
        "--source",
        &format!("fifo:{}", fifo.display()),
        "--slots",
        "2",
        "--max-clients",
        "4",
        "--zone",
        "den",
        "--zone",
        "study=wireless",
    ]);
    let watcher = fresh_id("visualizer-watcher");
    let plain = fresh_id("visualizer-plain");
    let roomless = fresh_id("visualizer-roomless");
    let far = fresh_id("visualizer-far");
    for (zone, endpoint) in [("den", &watcher), ("den", &plain), ("study", &far)] {
        server.applied(&format!(
            r#"{{"v":1,"t":"attach","zone":"{}","endpoint":"{}"}}"#,
            zone, endpoint
        ));
    }
    let asks = ClientConfig {
        visualizer_bands: BANDS,
        ..ClientConfig::default()
    };
    let watching = Recorder::start(Player::connect_with(&server.audio, &watcher, &asks));
    let line = server.wait_for_all(&["client session", &format!("id={} ", watcher)]);
    assert!(
        line.contains("visualizer_bands=16") && line.contains("roles=9"),
        "the server saw the role and the bands: {line}"
    );
    let playing = Recorder::start(Player::connect(&server.audio, &plain, 0));
    server.wait_for_all(&["client session", &format!("id={} ", plain)]);
    let nowhere = Recorder::start(Player::connect_with(&server.audio, &roomless, &asks));
    server.wait_for_all(&["client session", &format!("id={} ", roomless)]);
    let farther = Recorder::start(Player::connect_with(&server.audio, &far, &asks));
    server.wait_for_all(&["client session", &format!("id={} ", far)]);
    // Let the conductor route all three and the slot play silence a while.
    thread::sleep(Duration::from_millis(600));

    // The fixture into the pipe, in the server's two channels.
    let samples = kick();
    let stereo: Vec<u8> = samples
        .iter()
        .flat_map(|s| {
            let b = s.to_le_bytes();
            [b[0], b[1], b[0], b[1]]
        })
        .collect();
    {
        let mut pipe = OpenOptions::new().write(true).open(&fifo).unwrap();
        pipe.write_all(&stereo).unwrap();
    }
    let duration_ms = samples.len() as u64 * 1_000 / RATE_HZ;
    thread::sleep(Duration::from_millis(duration_ms + 1_500));
    let (chunks, messages) = watching.finish();
    let (plain_chunks, plain_messages) = playing.finish();
    let (_, nowhere_messages) = nowhere.finish();
    let (_, far_messages) = farther.finish();

    // Where the fixture is on the server timeline: its first non-zero
    // sample, then every sample after it in order.
    let left: Vec<(u64, i16)> = chunks
        .iter()
        .flat_map(|c| {
            c.audio_data
                .as_chunks::<4>()
                .0
                .iter()
                .enumerate()
                .map(move |(i, f)| {
                    (
                        c.timestamp_ns + i as u64 * 1_000_000_000 / RATE_HZ,
                        i16::from_le_bytes([f[0], f[1]]),
                    )
                })
        })
        .collect();
    let first = left
        .iter()
        .position(|(_, s)| *s != 0)
        .expect("the fixture reached the den");
    let anchor_ns = left[first].0;
    let heard: Vec<i16> = left[first..]
        .iter()
        .map(|(_, s)| *s)
        .take(samples.len())
        .collect();
    assert_eq!(heard.len(), samples.len(), "the whole fixture arrived");
    assert!(
        heard == samples,
        "the fixture arrived in order, with no gap inside it"
    );

    // The beats, against the kicks, when the den hears them.
    let tolerance_ms: f64 = param("beat_tolerance_ms").parse::<f64>().unwrap() + f64::from(HOP_MS);
    let kicks: Vec<f64> = param("expect_beats_ms")
        .split_whitespace()
        .map(|v| v.parse().unwrap())
        .collect();
    let latency = WIRED_GROUP_LATENCY_NS as u64;
    let beats_heard_after = |messages: &[V2Message], latency: u64| -> Vec<(f64, u8, usize)> {
        messages
            .iter()
            .filter_map(|m| match m {
                V2Message::VisualizerFrame(v) if v.beat > 0 => Some((
                    (v.timestamp_ns as i64 - (anchor_ns + latency) as i64) as f64 / 1e6,
                    v.beat,
                    v.bands.len(),
                )),
                _ => None,
            })
            .collect()
    };
    let beats = beats_heard_after(&messages, latency);
    let frames = messages
        .iter()
        .filter(|m| matches!(m, V2Message::VisualizerFrame(_)))
        .count();
    let colours = messages
        .iter()
        .filter(|m| matches!(m, V2Message::Color(_)))
        .count();
    for (at, strength, bands) in &beats {
        println!(
            "visualizer e2e: beat {strength} at {at:+.1} ms from the fixture's start, as heard, \
             with {bands} bands"
        );
    }
    assert_eq!(beats.len(), kicks.len(), "one beat per kick: {beats:?}");
    for ((at, strength, bands), kick) in beats.iter().zip(&kicks) {
        assert!(
            (at - kick).abs() <= tolerance_ms,
            "the beat at {at:.1} ms is not within {tolerance_ms} ms of the kick at {kick} ms"
        );
        assert!(
            *strength >= 128,
            "the beat at {at:.1} ms is only {strength}"
        );
        assert_eq!(*bands, usize::from(BANDS), "the bands asked for");
    }
    assert!(colours >= 1, "a colour was sent");
    // The wireless study plays the same stream on the other slot, and hears
    // it after the wireless tier's latency: so are its frames stamped.
    let wireless = heard_latency_ns(Transport::Wireless);
    let far_beats = beats_heard_after(&far_messages, wireless);
    assert_eq!(far_beats.len(), kicks.len(), "the study: {far_beats:?}");
    for ((at, _, _), kick) in far_beats.iter().zip(&kicks) {
        assert!(
            (at - kick).abs() <= tolerance_ms,
            "the study's beat at {at:.1} ms (after {} ms) is not near the kick at {kick} ms",
            wireless / 1_000_000
        );
    }
    assert!(
        !plain_chunks.is_empty(),
        "the plain player was served audio"
    );
    assert_eq!(
        visualizer_messages(&plain_messages),
        0,
        "the player without the visualizer role was sent no frame"
    );
    assert_eq!(
        visualizer_messages(&nowhere_messages),
        0,
        "the endpoint in no room was sent no frame"
    );
    println!(
        "visualizer e2e: {} kicks, {} beats within {} ms on the server timeline (wired latency \
         {} ms); {} frames and {} colours to the visualizer endpoint; 0 to the plain player \
         ({} chunks) and 0 to the endpoint in no room; the wireless study's {} beats within \
         the same tolerance after {} ms",
        kicks.len(),
        beats.len(),
        tolerance_ms,
        latency / 1_000_000,
        frames,
        colours,
        plain_chunks.len(),
        far_beats.len(),
        wireless / 1_000_000
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// --- the HTTP stream (`GET /api/visualizer?zone=<room>`) ----------------------
//
// The same fixture through the same server, read by subscribers of the HTTP
// control plane that hold no audio-wire session: `docs/visualizer.md`, "The
// HTTP stream". Nothing above this line was changed for it.

use std::net::TcpStream;
use std::time::Instant;

use chorus_control::json::{self, Value};
use chorus_server::lights::MIN_FRAME_INTERVAL;

/// One frame an HTTP subscriber received, and when its bytes were read, ns
/// on the test's own monotonic clock.
#[derive(Debug, Clone)]
struct Lit {
    arrived_ns: i128,
    zone: String,
    timestamp_ns: u64,
    lead_ms: i64,
    peak: u8,
    beat: u8,
    rgb: (u8, u8, u8),
    brightness: u8,
}

/// A `GET /api/visualizer?zone=<room>` subscriber on a raw socket (what a
/// script or a Home Assistant client is), read on a thread of its own.
struct LightStream {
    frames: Arc<Mutex<Vec<Lit>>>,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

fn light_request(control: &str, target: &str) -> (TcpStream, String) {
    let mut socket = TcpStream::connect(control).expect("the control plane listens");
    write!(
        socket,
        "GET {} HTTP/1.1\r\nHost: chorus\r\nConnection: close\r\n\r\n",
        target
    )
    .unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut head = String::new();
    let mut byte = [0u8; 1];
    // Byte by byte as far as the opening comment line or the end of a
    // refusal, so nothing of the stream itself is read here.
    while !head.ends_with("\r\n\r\n: visualizer\n\n") {
        match socket.read(&mut byte) {
            Ok(1) => head.push(byte[0] as char),
            _ => break,
        }
    }
    (socket, head)
}

impl LightStream {
    fn open(control: &str, zone: &str, epoch: Instant) -> LightStream {
        let (mut socket, head) = light_request(control, &format!("/api/visualizer?zone={}", zone));
        assert!(
            head.starts_with("HTTP/1.1 200 OK") && head.contains("text/event-stream"),
            "{zone}: {head}"
        );
        socket
            .set_read_timeout(Some(Duration::from_millis(50)))
            .unwrap();
        let frames = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let join = {
            let frames = Arc::clone(&frames);
            let stop = Arc::clone(&stop);
            thread::spawn(move || {
                let mut buffered = String::new();
                let mut scratch = [0u8; 4_096];
                while !stop.load(Ordering::SeqCst) {
                    let n = match socket.read(&mut scratch) {
                        Ok(0) => return,
                        Ok(n) => n,
                        Err(_) => continue,
                    };
                    let arrived_ns = epoch.elapsed().as_nanos() as i128;
                    buffered.push_str(&String::from_utf8_lossy(&scratch[..n]));
                    while let Some((block, rest)) = buffered.split_once("\n\n") {
                        let block = block.to_string();
                        buffered = rest.to_string();
                        let Some(data) = block.strip_prefix("data: ") else {
                            assert!(block.starts_with(':'), "not a comment: {block:?}");
                            continue;
                        };
                        let message = json::parse(data).expect("a JSON message");
                        let int = |key: &str| -> i128 {
                            message
                                .get(key)
                                .and_then(Value::as_num)
                                .unwrap_or_else(|| panic!("{data} has no {key}"))
                                .parse()
                                .unwrap()
                        };
                        assert_eq!(
                            (int("v"), message.get("t").and_then(Value::as_str)),
                            (2, Some("visualizer"))
                        );
                        assert_eq!(int("transition_ms") % 500, 0, "{data}");
                        frames.lock().unwrap().push(Lit {
                            arrived_ns,
                            zone: message
                                .get("zone")
                                .and_then(Value::as_str)
                                .unwrap()
                                .to_string(),
                            timestamp_ns: int("timestamp_ns") as u64,
                            lead_ms: int("lead_ms") as i64,
                            peak: int("peak") as u8,
                            beat: int("beat") as u8,
                            rgb: (int("red") as u8, int("green") as u8, int("blue") as u8),
                            brightness: int("brightness") as u8,
                        });
                    }
                }
            })
        };
        LightStream {
            frames,
            stop,
            join: Some(join),
        }
    }

    fn finish(mut self) -> Vec<Lit> {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(j) = self.join.take() {
            j.join().expect("the subscriber read what it was sent");
        }
        let frames = self.frames.lock().unwrap().clone();
        frames
    }
}

/// A plain player's chunks with the instant each was read, ns on the test's
/// clock: where the fixture is on the server timeline, and how the server
/// timeline sits against the test's clock.
fn timed_chunks(
    player: Player,
    epoch: Instant,
    stop: Arc<AtomicBool>,
) -> JoinHandle<Vec<(i128, AudioChunk)>> {
    let (session, _messages) = player.split();
    let mut reader = session.reader;
    thread::spawn(move || {
        let _writer = session.writer;
        let mut chunks = Vec::new();
        let mut pending: Vec<u8> = Vec::new();
        let mut scratch = vec![0u8; 65_536];
        while !stop.load(Ordering::SeqCst) {
            match reader.read(&mut scratch) {
                Ok(0) => break,
                Ok(n) => pending.extend_from_slice(&scratch[..n]),
                Err(_) => continue,
            }
            let arrived_ns = epoch.elapsed().as_nanos() as i128;
            let mut at = 0usize;
            while at < pending.len() {
                let d = decode_frame(&pending[at..]);
                if d.consumed == 0 {
                    break;
                }
                at += d.consumed;
                if let FrameOutcome::Decoded(Message::AudioChunk(c)) = d.outcome {
                    chunks.push((arrived_ns, c));
                }
            }
            pending.drain(..at);
        }
        chunks
    })
}

#[test]
fn an_http_subscriber_reads_a_rooms_beats_and_colour_under_the_rate_cap() {
    let dir = std::env::temp_dir().join(format!("chorus-visualizer-http-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let fifo = dir.join("pcm.fifo");
    assert!(Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("mkfifo runs")
        .success());
    // Two slots and three rooms: the den (wired) and the study (wireless)
    // play the stream, and the attic, the room past the last slot, plays
    // nothing (its group starts with source `none`).
    let mut server = RunningServer::start(&[
        "--source",
        &format!("fifo:{}", fifo.display()),
        "--slots",
        "2",
        "--max-clients",
        "2",
        "--zone",
        "den",
        "--zone",
        "study=wireless",
        "--zone",
        "attic",
    ]);
    let idle = server.wait_for("source=none reason=every-slot-in-use");
    assert!(
        idle.contains("group=attic"),
        "the attic plays nothing: {idle}"
    );

    // A request that names no room, or one this server does not have, is
    // refused and holds nothing.
    let (_, unnamed) = light_request(&server.control, "/api/visualizer");
    assert!(unnamed.starts_with("HTTP/1.1 400"), "{unnamed}");
    let (_, unknown) = light_request(&server.control, "/api/visualizer?zone=cellar");
    assert!(unknown.starts_with("HTTP/1.1 404"), "{unknown}");

    // The only audio-wire session is a plain player in the den, which
    // declares no visualizer role: it is how this test knows where the
    // fixture is on the server timeline. The subscribers are HTTP alone.
    let plain = fresh_id("visualizer-http-plain");
    server.applied(&format!(
        r#"{{"v":1,"t":"attach","zone":"den","endpoint":"{}"}}"#,
        plain
    ));
    let epoch = Instant::now();
    let stop = Arc::new(AtomicBool::new(false));
    let playing = Player::connect(&server.audio, &plain, 0);
    server.wait_for_all(&["client session", &format!("id={} ", plain)]);
    let hearing = timed_chunks(playing, epoch, Arc::clone(&stop));
    let den = LightStream::open(&server.control, "den", epoch);
    let study = LightStream::open(&server.control, "study", epoch);
    let attic = LightStream::open(&server.control, "attic", epoch);
    let report = common::http(
        &server.control,
        "GET /api/report HTTP/1.1\r\nHost: chorus\r\nConnection: close\r\n\r\n",
    )
    .1;
    assert!(report.contains("light_subscribers=3"), "{report}");
    // Let the conductor place the rooms and the slots play silence a while.
    thread::sleep(Duration::from_millis(600));

    let samples = kick();
    let stereo: Vec<u8> = samples
        .iter()
        .flat_map(|s| {
            let b = s.to_le_bytes();
            [b[0], b[1], b[0], b[1]]
        })
        .collect();
    {
        let mut pipe = OpenOptions::new().write(true).open(&fifo).unwrap();
        pipe.write_all(&stereo).unwrap();
    }
    let duration_ms = samples.len() as u64 * 1_000 / RATE_HZ;
    thread::sleep(Duration::from_millis(duration_ms + 1_500));
    let report = common::http(
        &server.control,
        "GET /api/report HTTP/1.1\r\nHost: chorus\r\nConnection: close\r\n\r\n",
    )
    .1;
    let den = den.finish();
    let study = study.finish();
    let attic = attic.finish();
    stop.store(true, Ordering::SeqCst);
    let chunks = hearing.join().unwrap();

    // Where the fixture is on the server timeline, as the test above finds
    // it: the first non-zero sample the den's player was sent.
    let anchor_ns = chunks
        .iter()
        .find_map(|(_, c)| {
            c.audio_data
                .as_chunks::<4>()
                .0
                .iter()
                .position(|f| i16::from_le_bytes([f[0], f[1]]) != 0)
                .map(|i| c.timestamp_ns + i as u64 * 1_000_000_000 / RATE_HZ)
        })
        .expect("the fixture reached the den");

    let tolerance_ms: f64 = param("beat_tolerance_ms").parse::<f64>().unwrap() + f64::from(HOP_MS);
    let kicks: Vec<f64> = param("expect_beats_ms")
        .split_whitespace()
        .map(|v| v.parse().unwrap())
        .collect();
    let wired = WIRED_GROUP_LATENCY_NS as u64;
    let wireless = heard_latency_ns(Transport::Wireless);
    let cap_ms = MIN_FRAME_INTERVAL.as_millis() as i128;
    assert_eq!(
        cap_ms, 100,
        "the documented cap: at most 10 frames a second"
    );

    for (room, frames, latency) in [("den", &den, wired), ("study", &study, wireless)] {
        assert!(frames.iter().all(|f| f.zone == room), "{room}: {frames:?}");
        // A beat for each kick, stamped when this room hears the kick, and a
        // colour on every frame that carries one.
        let beats: Vec<&Lit> = frames.iter().filter(|f| f.beat > 0).collect();
        assert_eq!(
            beats.len(),
            kicks.len(),
            "{room}: one beat per kick: {beats:?}"
        );
        for (beat, kick) in beats.iter().zip(&kicks) {
            let at = (beat.timestamp_ns as i64 - (anchor_ns + latency) as i64) as f64 / 1e6;
            println!(
                "visualizer http: {room} beat {} at {at:+.1} ms from the fixture's start, as \
                 heard; lead {} ms; peak {}; colour {:?} at brightness {}",
                beat.beat, beat.lead_ms, beat.peak, beat.rgb, beat.brightness
            );
            assert!(
                (at - kick).abs() <= tolerance_ms,
                "{room}: the beat at {at:.1} ms is not within {tolerance_ms} ms of the kick at \
                 {kick} ms"
            );
            assert!(
                beat.beat >= 128,
                "{room}: the beat at {at:.1} ms is only {}",
                beat.beat
            );
            assert!(
                beat.brightness > 0 && beat.rgb != (0, 0, 0),
                "{room}: the beat at {at:.1} ms carries no colour: {beat:?}"
            );
        }
        // Silence sends its first frame and then nothing: the frames stop
        // within a second of the fixture's end, and this stream was read
        // for 1.5 s after it.
        // Silence sends its first frame and then nothing. The stream was
        // opened onto a slot playing silence at least 600 ms before the
        // fixture, six frames' worth under the cap: one frame came of it,
        // the first, and it shows nothing.
        let before: Vec<&Lit> = frames
            .iter()
            .filter(|f| (f.timestamp_ns as i64) < (anchor_ns + latency) as i64 - 100_000_000)
            .collect();
        assert_eq!(before.len(), 1, "{room}: frames of the silence: {before:?}");
        assert_eq!(
            (before[0].peak, before[0].beat),
            (0, 0),
            "{room}: {before:?}"
        );
        let last = frames.last().unwrap();

        // The rate. By the server's own account, the moment it wrote a
        // frame is the frame's stamp less its lead (whole ms, rounded down),
        // and two of those are never closer than the cap.
        let wrote: Vec<i128> = frames
            .iter()
            .map(|f| i128::from(f.timestamp_ns) / 1_000_000 - i128::from(f.lead_ms))
            .collect();
        let closest = wrote.windows(2).map(|w| w[1] - w[0]).min().unwrap();
        assert!(
            closest >= cap_ms - 1,
            "{room}: two frames written {closest} ms apart, under the {cap_ms} ms cap"
        );
        // And as observed here: over the whole stream, the frames that
        // arrived are no more than the cap allows in the time they took to
        // arrive (one frame's interval of slack for when this thread read).
        let span_ms = (last.arrived_ns - frames[0].arrived_ns) / 1_000_000;
        let most = (span_ms + cap_ms) / cap_ms + 1;
        assert!(
            frames.len() as i128 <= most,
            "{room}: {} frames arrived in {span_ms} ms, more than {cap_ms} ms apart allows",
            frames.len()
        );
        // The busiest second observed, for the record.
        let busiest = frames
            .iter()
            .map(|f| {
                frames
                    .iter()
                    .filter(|g| {
                        g.arrived_ns >= f.arrived_ns && g.arrived_ns < f.arrived_ns + 1_000_000_000
                    })
                    .count()
            })
            .max()
            .unwrap();
        assert!(
            busiest <= 11,
            "{room}: {busiest} frames arrived within one second"
        );

        // The lead, measured: the server never sends a chunk before the
        // instant it is stamped with, so the least (arrival - stamp) over
        // the den's chunks is the test clock's offset from the server
        // timeline, to within loopback's delay. With it, when the room
        // hears a frame on the test's clock, less when the frame arrived,
        // is what `lead_ms` said, to within that delay and a millisecond.
        let offset_ns = chunks
            .iter()
            .map(|(arrived, c)| arrived - i128::from(c.timestamp_ns))
            .min()
            .unwrap();
        let mut errors_ms: Vec<f64> = frames
            .iter()
            .map(|f| {
                let left_ns = i128::from(f.timestamp_ns) + offset_ns - f.arrived_ns;
                left_ns as f64 / 1e6 - f.lead_ms as f64
            })
            .collect();
        errors_ms.sort_by(|a, b| a.total_cmp(b));
        let median = errors_ms[errors_ms.len() / 2];
        let leads: Vec<i64> = frames.iter().map(|f| f.lead_ms).collect();
        println!(
            "visualizer http: {room}: {} frames over {span_ms} ms, closest written {closest} ms \
             apart, at most {busiest} in one second; lead_ms {} to {} (beats: {:?}); measured \
             time to hearing less lead_ms: {:.1} / {:.1} / {:.1} ms (least / median / most)",
            frames.len(),
            leads.iter().min().unwrap(),
            leads.iter().max().unwrap(),
            beats.iter().map(|b| b.lead_ms).collect::<Vec<_>>(),
            errors_ms[0],
            median,
            errors_ms[errors_ms.len() - 1]
        );
        assert!(
            median.abs() <= 20.0,
            "{room}: lead_ms is {median:.1} ms (median) from the time to hearing measured here"
        );
    }
    // The same kicks, 320 ms later for the wireless room: its stamps are
    // its own tier's.
    let first = |frames: &[Lit]| frames.iter().find(|f| f.beat > 0).unwrap().timestamp_ns;
    assert_eq!(first(&study) - first(&den), wireless - wired);

    assert!(
        attic.is_empty(),
        "the room playing nothing was sent {} frames: {attic:?}",
        attic.len()
    );
    assert!(report.contains("light_subscribers=3"), "{report}");
    println!("visualizer http: {}", report.trim());
    let _ = std::fs::remove_dir_all(&dir);
}
