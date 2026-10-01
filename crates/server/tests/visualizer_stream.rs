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
