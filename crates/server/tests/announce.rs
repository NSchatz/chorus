//! The `announce` command and the server's identity route, on the real
//! binary (goal 18, ADR 0136; the duck, mix and restore: ADR 0173 and
//! ADR 0175; brief section 4.8's "HA's media and TTS URLs from HA's own
//! address").
//!
//! Every test runs the real `chorus-server` with stream slots and a control
//! plane. The music is the configured stream, every sample the same value.
//! The clip is a generated WAV whose LEFT channel is a signal no two
//! neighbouring frames of which are alike and whose RIGHT channel is
//! silence, served by an HTTP server on loopback inside the test, whose
//! address is the server's one `--announce-origin` (`--media-allow-loopback`
//! exists for exactly this and is never set in a deployment). A room is a
//! protocol v2 player session opened through the Linux client's own session
//! code (`common::Player`), and what is graded is the audio it receives:
//! on the right channel the music alone, so its gain frame by frame, and on
//! the left the music and the clip, so which clip frame is in which frame
//! of the stream.
//!
//! The test names are the evidence:
//!
//! - an announcement in a room playing the stream: the answer is the state
//!   with the announcement's number, the group still on the stream and the
//!   room at the announcement's volume CLAMPED to its limit; the room hears
//!   the music go down, every frame of the clip over the ducked music, in
//!   order, and the music come back to its own samples; the state says
//!   `finished`, and the room has the volume it had. A URL from another
//!   origin is refused naming `url` and changes nothing; and an
//!   announcement during another replaces it, which the state says of the
//!   first (`displaced`);
//! - in a group of two rooms, an announcement to one room is heard in that
//!   room alone while the other plays the music untouched on the same
//!   sequences and timestamps, and one to the group is heard in both, the
//!   same bytes at the same sequence;
//! - the timing, measured on the stream's own timeline in frames (a chunk's
//!   sequence times its frames, plus the frame's place in it): from the
//!   clip's first frame to the full duck and from its last to the full
//!   restore, each within ADR 0175's bound, printed as `measured:` lines
//!   (`docs/measurements/2026-10-05-announcement-duck-timing.md`);
//! - a clip that answers 404, and one whose URL redirects out of the
//!   origin, bring the music back at once and say `failed`;
//! - `GET /api/server` answers the committed shape, its `id` survives a
//!   restart with the same identity directory, and a server with no origin
//!   or no player refuses every announcement by name.
//!
//! No wall clock is evidence here: every wait has a generous bound and none
//! of them is graded, and the durations that ARE graded are counts of
//! frames on the stream.

mod common;

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chorus_control::json::{self, Value};
use chorus_protocol::AudioChunk;
use common::{fresh_id, http, Player, RunningServer};

const RATE_HZ: u32 = 48_000;
/// The configured stream: every sample this value.
const SAMPLE: i16 = 0x1234;
/// The music under a clip: `SAMPLE` 20 dB down (`chorus_dsp::duck`).
const DUCKED: i16 = 466;
/// Frames in a chunk of the default stream shape (20 ms at 48 kHz).
const CHUNK_FRAMES: u64 = 960;
/// ADR 0173's defaults at 48 kHz: 200 ms down, 500 ms back.
const DUCK_FRAMES: u64 = 9_600;
const RESTORE_FRAMES: u64 = 24_000;
const LIMIT: Duration = Duration::from_secs(20);

type Frame = [i16; 2];

fn constant_source(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("chorus-ann-{}-{}.pcm", name, std::process::id()));
    let bytes: Vec<u8> = std::iter::repeat_n(SAMPLE.to_le_bytes(), RATE_HZ as usize * 2 * 40)
        .flatten()
        .collect();
    std::fs::write(&path, bytes).unwrap();
    path
}

/// The server serving `kitchen`, with `extra` flags and a throwaway
/// identity.
fn server(source: &Path, extra: &[&str]) -> RunningServer {
    let mut args = vec![
        "--source",
        source.to_str().unwrap(),
        "--slots",
        "2",
        "--zone",
        "kitchen",
    ];
    args.extend_from_slice(extra);
    RunningServer::start(&args)
}

fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + LIMIT;
    while !done() {
        assert!(
            Instant::now() < deadline,
            "{} did not happen within {:?}",
            what,
            LIMIT
        );
        thread::sleep(Duration::from_millis(20));
    }
}

/// Frame `n` of the clip: the left channel a ramp that is never silence
/// and never repeats from one frame to the next, the right channel silent,
/// so the right channel of a mix is the music alone.
fn ramp_frame(n: usize) -> Frame {
    [4_000 + (n % 20_000) as i16, 0]
}

/// What frame `n` of the clip adds to the left channel of a mix: the clip
/// at `1 - 0.1` of its level (ADR 0173's clip gain), give or take the one
/// step two roundings can move it.
fn clip_part(n: usize) -> i16 {
    (f64::from(ramp_frame(n)[0]) * 0.9).round() as i16
}

/// A 16-bit stereo WAV file of the signal's first `frames` frames.
fn wav(frames: usize) -> Vec<u8> {
    let data = (frames * 4) as u32;
    let mut out = Vec::with_capacity(44 + frames * 4);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&RATE_HZ.to_le_bytes());
    out.extend_from_slice(&(RATE_HZ * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for n in 0..frames {
        let f = ramp_frame(n);
        out.extend_from_slice(&f[0].to_le_bytes());
        out.extend_from_slice(&f[1].to_le_bytes());
    }
    out
}

// ----- the media server ----------------------------------------------------------

#[derive(Clone)]
enum Route {
    /// A WAV file.
    File(Arc<Vec<u8>>),
    /// A 302 to this URL.
    Redirect(String),
}

/// An HTTP server on loopback serving what the test put in it. A path with
/// no route answers 404. It keeps every path asked for.
struct MediaServer {
    address: String,
    routes: Arc<Mutex<HashMap<String, Route>>>,
    asked: Arc<Mutex<Vec<String>>>,
}

impl MediaServer {
    fn start() -> MediaServer {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let routes: Arc<Mutex<HashMap<String, Route>>> = Arc::default();
        let asked: Arc<Mutex<Vec<String>>> = Arc::default();
        {
            let (routes, asked) = (Arc::clone(&routes), Arc::clone(&asked));
            thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(stream) = stream else { return };
                    let (routes, asked) = (Arc::clone(&routes), Arc::clone(&asked));
                    thread::spawn(move || serve_media(stream, &routes, &asked));
                }
            });
        }
        MediaServer {
            address,
            routes,
            asked,
        }
    }

    fn origin(&self) -> String {
        format!("http://{}", self.address)
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.address, path)
    }

    fn serve(&self, path: &str, route: Route) -> String {
        self.routes.lock().unwrap().insert(path.to_string(), route);
        self.url(path)
    }

    fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }
}

fn serve_media(
    mut stream: TcpStream,
    routes: &Mutex<HashMap<String, Route>>,
    asked: &Mutex<Vec<String>>,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        match stream.read(&mut byte) {
            Ok(1) => head.push(byte[0]),
            _ => return,
        }
    }
    let head = String::from_utf8_lossy(&head).to_string();
    let path = head.split(' ').nth(1).unwrap_or("/").to_string();
    asked.lock().unwrap().push(path.clone());
    let route = routes.lock().unwrap().get(&path).cloned();
    match route {
        None => {
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
        }
        Some(Route::Redirect(to)) => {
            let _ = stream.write_all(
                format!(
                    "HTTP/1.1 302 Found\r\nLocation: {}\r\nContent-Length: 0\r\n\r\n",
                    to
                )
                .as_bytes(),
            );
        }
        Some(Route::File(body)) => {
            let _ = stream.write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: audio/wav\r\nContent-Length: {}\r\n\r\n",
                    body.len()
                )
                .as_bytes(),
            );
            let _ = stream.write_all(&body);
        }
    }
}

// ----- what the room hears -------------------------------------------------------

/// A room's player session, read on a thread of its own.
struct Room {
    chunks: Arc<Mutex<Vec<AudioChunk>>>,
    keep: Arc<AtomicBool>,
}

impl Room {
    fn listen(server: &RunningServer, zone: &str) -> Room {
        let endpoint = fresh_id(&format!("ann-{}", zone));
        server.applied(&format!(
            r#"{{"v":1,"t":"attach","zone":"{}","endpoint":"{}"}}"#,
            zone, endpoint
        ));
        let mut player = Player::connect(&server.audio, &endpoint, 0);
        let chunks: Arc<Mutex<Vec<AudioChunk>>> = Arc::default();
        let keep = Arc::new(AtomicBool::new(true));
        {
            let (chunks, keep) = (Arc::clone(&chunks), Arc::clone(&keep));
            thread::spawn(move || {
                while keep.load(Ordering::SeqCst) {
                    if let Some(chunk) = player.next_chunk(Duration::from_millis(100)) {
                        chunks.lock().unwrap().push(chunk);
                    }
                }
            });
        }
        let room = Room { chunks, keep };
        wait_for("the room hears the configured stream", || {
            room.last().is_some_and(|f| f == [SAMPLE, SAMPLE])
        });
        room
    }

    fn frames(&self) -> Vec<Frame> {
        self.chunks
            .lock()
            .unwrap()
            .iter()
            .flat_map(|c| {
                c.audio_data
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|f| {
                        [
                            i16::from_le_bytes([f[0], f[1]]),
                            i16::from_le_bytes([f[2], f[3]]),
                        ]
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    fn last(&self) -> Option<Frame> {
        self.frames().last().copied()
    }

    /// Every frame heard so far with its place on the stream's timeline:
    /// the chunk's sequence times a chunk's frames, plus its place in the
    /// chunk. Chunks are in the order they were sent.
    fn timeline(&self) -> Vec<(u64, Frame)> {
        self.chunks
            .lock()
            .unwrap()
            .iter()
            .flat_map(|c| {
                let first = u64::from(c.sequence) * CHUNK_FRAMES;
                c.audio_data
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .enumerate()
                    .map(|(n, f)| {
                        (
                            first + n as u64,
                            [
                                i16::from_le_bytes([f[0], f[1]]),
                                i16::from_le_bytes([f[2], f[3]]),
                            ],
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Every chunk's sequence and timestamp, in the order received.
    fn stamps(&self) -> Vec<(u32, u64)> {
        self.chunks
            .lock()
            .unwrap()
            .iter()
            .map(|c| (c.sequence, c.timestamp_ns))
            .collect()
    }

    /// How many frames of a clip were heard so far: the frames whose left
    /// channel is not their right.
    fn clip_frames(&self) -> usize {
        self.frames().iter().filter(|f| f[0] != f[1]).count()
    }
}

/// One announcement as a room heard it, on the stream's timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Heard {
    /// The first frame the music is lower in.
    duck_first: u64,
    /// The first frame the music is fully ducked in.
    duck_full: u64,
    /// The clip's first and last frames.
    clip_first: u64,
    clip_last: u64,
    /// The first frame after the clip the music is its own sample in again.
    restored: u64,
}

/// Read one announcement of a clip of `clip` frames out of what a room
/// heard from timeline frame `from` on, checking every frame on the way:
/// before it the music's own samples; then the music alone going down,
/// never up, to the ducked level; then, with the music held there, every
/// frame of the clip in order with nothing between them; then the music
/// alone coming back, never down; and after it the music's own samples to
/// the end of what was heard.
fn heard(timeline: &[(u64, Frame)], from: u64, clip: usize) -> Heard {
    /// The frames in order, refusing a gap between two of them.
    struct Cursor<I: Iterator<Item = (u64, Frame)>> {
        frames: I,
        at: Option<u64>,
    }
    impl<I: Iterator<Item = (u64, Frame)>> Cursor<I> {
        fn next(&mut self, what: &str) -> (u64, Frame) {
            let (at, frame) = self
                .frames
                .next()
                .unwrap_or_else(|| panic!("the stream ended {}", what));
            if let Some(before) = self.at {
                assert_eq!(at, before + 1, "a gap in the stream {}", what);
            }
            self.at = Some(at);
            (at, frame)
        }
    }
    let music = [SAMPLE, SAMPLE];
    let mut stream = Cursor {
        frames: timeline.iter().copied().filter(|(at, _)| *at >= from),
        at: None,
    };
    // Before: the music.
    let (duck_first, mut level) = loop {
        let (at, frame) = stream.next("before the duck");
        if frame != music {
            assert_eq!(frame[0], frame[1], "no clip frame on the way down");
            break (at, frame[1]);
        }
    };
    // Down: the music alone, never rising, to the ducked level.
    let mut duck_full = duck_first;
    while level != DUCKED {
        let (at, frame) = stream.next("on the way down");
        assert_eq!(
            frame[0], frame[1],
            "no clip frame at {} on the way down",
            at
        );
        assert!(
            frame[1] <= level && frame[1] >= DUCKED,
            "frame {}: {:?}",
            at,
            frame
        );
        level = frame[1];
        duck_full = at;
    }
    // Held, until the clip's first frame.
    let clip_first = loop {
        let (at, frame) = stream.next("before the clip");
        assert_eq!(frame[1], DUCKED, "the music is held down at frame {}", at);
        if frame[0] != frame[1] {
            assert!(
                (frame[0] - DUCKED - clip_part(0)).abs() <= 1,
                "the clip's first frame at {}: {:?}",
                at,
                frame
            );
            break at;
        }
    };
    // The clip, every frame of it, over the ducked music.
    let mut clip_last = clip_first;
    for n in 1..clip {
        let (at, frame) = stream.next("in the clip");
        assert_eq!(
            frame[1], DUCKED,
            "the music under clip frame {} at {}",
            n, at
        );
        assert!(
            (frame[0] - DUCKED - clip_part(n)).abs() <= 1,
            "clip frame {} at {}: {:?}",
            n,
            at,
            frame
        );
        clip_last = at;
    }
    // Back: the music alone, never falling, to its own samples.
    let mut level = DUCKED;
    let restored = loop {
        let (at, frame) = stream.next("on the way back");
        assert_eq!(frame[0], frame[1], "no clip frame at {} after its last", at);
        assert!(
            frame[1] >= level && frame[1] <= SAMPLE,
            "frame {}: {:?}",
            at,
            frame
        );
        level = frame[1];
        if frame == music {
            break at;
        }
    };
    for (at, frame) in stream.frames {
        assert_eq!(frame, music, "the music is itself again at frame {}", at);
    }
    Heard {
        duck_first,
        duck_full,
        clip_first,
        clip_last,
        restored,
    }
}

impl Drop for Room {
    fn drop(&mut self) {
        self.keep.store(false, Ordering::SeqCst);
    }
}

// ----- the state -----------------------------------------------------------------

fn parsed(text: &str) -> Value {
    json::parse(text).unwrap_or_else(|e| panic!("{}: {:?}", text, e))
}

fn list<'a>(state: &'a Value, key: &str) -> &'a [Value] {
    match state.get(key) {
        Some(Value::Arr(items)) => items,
        _ => &[],
    }
}

/// What a group plays in `state`, and the `via` of its now-playing record.
fn playing_in(state: &str, group: &str) -> (String, Option<String>) {
    let state = parsed(state);
    let group = list(&state, "groups")
        .iter()
        .find(|g| g.get("id").and_then(Value::as_str) == Some(group))
        .unwrap_or_else(|| panic!("no group {}", group))
        .clone();
    (
        group
            .get("source")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        group
            .get("now_playing")
            .and_then(|r| r.get("via"))
            .and_then(Value::as_str)
            .map(str::to_string),
    )
}

/// A room's volume in `state`, as the three-digit decimal it is written.
fn volume_in(state: &str, zone: &str) -> String {
    let state = parsed(state);
    let room = list(&state, "zones")
        .iter()
        .find(|z| z.get("id").and_then(Value::as_str) == Some(zone))
        .unwrap_or_else(|| panic!("no room {}", zone))
        .clone();
    match room.get("volume") {
        Some(Value::Num(digits)) => digits.clone(),
        other => panic!("room {} has the volume {:?}", zone, other),
    }
}

/// The state with its serial and its list of announcements taken out, for
/// "the house is as it was".
fn without_serial(state: &str) -> String {
    match parsed(state) {
        Value::Obj(members) => json::write(&Value::Obj(
            members
                .into_iter()
                .filter(|(k, _)| k != "serial" && k != "announcements")
                .collect(),
        )),
        other => panic!("the state is {:?}", other),
    }
}

/// The number an `announce` command's answer gives its announcement.
fn number_in(answer: &str) -> i64 {
    match parsed(answer).get("announcement") {
        Some(Value::Num(digits)) => digits.parse().expect("a number"),
        other => panic!("the answer's announcement is {:?}: {}", other, answer),
    }
}

/// How the state says announcement `id` stands: its `state`, its rooms and
/// its `reason`; `None` when the state does not list it.
fn announcement_in(state: &str, id: i64) -> Option<(String, Vec<String>, Option<String>)> {
    let state = parsed(state);
    let listed = list(&state, "announcements")
        .iter()
        .find(|a| matches!(a.get("id"), Some(Value::Num(n)) if n.parse() == Ok(id)))?
        .clone();
    let rooms = match listed.get("rooms") {
        Some(Value::Arr(rooms)) => rooms
            .iter()
            .filter_map(|r| r.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    };
    Some((
        listed.get("state").and_then(Value::as_str)?.to_string(),
        rooms,
        listed
            .get("reason")
            .and_then(Value::as_str)
            .map(str::to_string),
    ))
}

/// Wait until the state says announcement `id` is over; how it ended and
/// why.
fn over(server: &RunningServer, id: i64) -> (String, Option<String>) {
    let mut ended = None;
    wait_for(
        &format!("announcement {} is over", id),
        || match announcement_in(&server.state(), id) {
            Some((state, _, reason)) if state != "playing" => {
                ended = Some((state, reason));
                true
            }
            _ => false,
        },
    );
    ended.unwrap()
}

/// Wait until a room has heard the music's own samples for half a second
/// on end: whatever an announcement did is over and in `Room::timeline`.
fn settled(room: &Room) {
    wait_for("the room hears the music alone again", || {
        let frames = room.frames();
        frames.len() > 24_000
            && frames[frames.len() - 24_000..]
                .iter()
                .all(|f| *f == [SAMPLE, SAMPLE])
    });
}

/// The timeline frame a room has heard up to now.
fn now_at(room: &Room) -> u64 {
    room.timeline().last().map_or(0, |(at, _)| *at)
}

fn announce(target: &str, url: &str, volume: Option<&str>) -> String {
    match volume {
        Some(v) => format!(
            r#"{{"v":2,"t":"announce","target":"{}","url":"{}","volume":{}}}"#,
            target, url, v
        ),
        None => format!(
            r#"{{"v":2,"t":"announce","target":"{}","url":"{}"}}"#,
            target, url
        ),
    }
}

/// A command that must be refused with a 400 naming `field`; its detail.
fn refused(server: &RunningServer, body: &str, field: &str) -> String {
    let before = server.state();
    let (status, answer) = server.command(body);
    assert!(status.contains("400"), "{}: {} {}", body, status, answer);
    let refusal = parsed(&answer);
    assert_eq!(
        refusal.get("t").and_then(Value::as_str),
        Some("error"),
        "{}",
        answer
    );
    assert_eq!(
        refusal.get("field").and_then(Value::as_str),
        Some(field),
        "{}",
        answer
    );
    assert_eq!(server.state(), before, "a refusal changes no state");
    refusal
        .get("detail")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn server_info(server: &RunningServer) -> String {
    let (status, body) = http(
        &server.control,
        "GET /api/server HTTP/1.1\r\nHost: chorus\r\nConnection: close\r\n\r\n",
    );
    assert!(status.contains("200"), "{} {}", status, body);
    body
}

fn id_of(info: &str) -> String {
    parsed(info)
        .get("id")
        .and_then(Value::as_str)
        .expect("an id")
        .to_string()
}

// ----- the tests -----------------------------------------------------------------

fn mixing_server(source: &Path, origin: &str, zones: &[&str]) -> RunningServer {
    let mut args = vec!["--source", source.to_str().unwrap(), "--slots", "2"];
    for zone in zones {
        args.extend_from_slice(&["--zone", zone]);
    }
    args.extend_from_slice(&[
        "--players",
        "1",
        "--media-allow-loopback",
        "--announce-origin",
        origin,
    ]);
    RunningServer::start(&args)
}

#[test]
fn an_announcement_ducks_the_music_mixes_the_clip_over_it_and_brings_the_music_back() {
    let media = MediaServer::start();
    // Lengths that are no whole number of chunks, so a clip ends inside one.
    let (long, short) = (2 * 48_000 + 123, 24_000 + 7);
    let clip = media.serve("/api/tts_proxy/clip.wav", Route::File(Arc::new(wav(long))));
    let brief = media.serve(
        "/api/tts_proxy/short.wav",
        Route::File(Arc::new(wav(short))),
    );
    let elsewhere = MediaServer::start();
    let off_list = elsewhere.serve("/clip.wav", Route::File(Arc::new(wav(24_000))));

    let source = constant_source("plays");
    let origin = media.origin();
    let mut server = mixing_server(&source, &origin, &["kitchen"]);
    let room = Room::listen(&server, "kitchen");
    server.applied(r#"{"v":2,"t":"volume","zone":"kitchen","volume":0.250}"#);
    server.applied(r#"{"v":2,"t":"limit","zone":"kitchen","limit":0.400}"#);
    assert_eq!(playing_in(&server.state(), "kitchen").0, "stream");
    assert!(
        server_info(&server).contains(&format!(r#""announce_origins":["{}"]"#, origin)),
        "{}",
        server_info(&server)
    );

    // A URL from an origin that is not on the list: refused naming `url`,
    // nothing changed, nothing fetched.
    let detail = refused(
        &server,
        &announce("kitchen", &off_list, Some("0.300")),
        "url",
    );
    assert!(
        detail.contains(&elsewhere.origin()) && detail.contains(&origin),
        "the refusal names the origin offered and the ones allowed: {}",
        detail
    );
    assert!(elsewhere.asked().is_empty(), "{:?}", elsewhere.asked());

    // The announcement, at a volume above the room's limit: the answer is
    // the state with the announcement's number, the group still playing
    // what it played, and the volume clamped to the limit.
    let before = server.state();
    let from = now_at(&room);
    let answer = server.applied(&announce("kitchen", &clip, Some("0.900")));
    let first = number_in(&answer);
    assert_eq!(first, 1, "{}", answer);
    assert_eq!(
        playing_in(&answer, "kitchen"),
        ("stream".to_string(), None),
        "no group's source changes: {}",
        answer
    );
    assert_eq!(
        volume_in(&answer, "kitchen"),
        "0.400",
        "clamped to the limit"
    );
    assert_eq!(
        announcement_in(&answer, first),
        Some(("playing".to_string(), vec!["kitchen".to_string()], None)),
        "{}",
        answer
    );
    server.wait_for_all(&[
        "announce owner=announce:1 id=1",
        "target=kitchen",
        "plays=player:p0",
        "previous=stream",
        "rooms=kitchen",
    ]);

    // The caller waits on the state: `finished`, the volume the room had,
    // and the house as it was.
    assert_eq!(over(&server, first), ("finished".to_string(), None));
    let after = server.state();
    assert_eq!(playing_in(&after, "kitchen"), ("stream".to_string(), None));
    assert_eq!(volume_in(&after, "kitchen"), "0.250", "the volume it had");
    assert_eq!(
        without_serial(&after),
        without_serial(&before),
        "the house is as it was before the announcement"
    );
    server.wait_for_all(&[
        "announce owner=announce:1 id=1 ended",
        "restored=stream",
        "outcome=finished",
    ]);

    // What the room heard: the music down, every frame of the clip over
    // the ducked music, the music back, and its own samples after.
    settled(&room);
    server.wait_for("announce owner=announce:1 id=1 music-restored");
    let one = heard(&room.timeline(), from, long);
    assert_eq!(one.clip_last - one.clip_first + 1, long as u64);
    assert!(one.duck_full < one.clip_first, "{:?}", one);
    assert!(one.restored > one.clip_last, "{:?}", one);

    // The player was given back: a second announcement, with no volume,
    // plays on it and leaves the room's volume alone throughout.
    let from = now_at(&room);
    let answer = server.applied(&announce("kitchen", &brief, None));
    let second = number_in(&answer);
    assert_eq!(second, 2);
    assert_eq!(volume_in(&answer, "kitchen"), "0.250");
    assert_eq!(over(&server, second).0, "finished");
    assert_eq!(volume_in(&server.state(), "kitchen"), "0.250");
    settled(&room);
    server.wait_for("announce owner=announce:2 id=2 music-restored");
    let two = heard(&room.timeline(), from, short);
    assert_eq!(two.clip_last - two.clip_first + 1, short as u64);

    // An announcement during another, in the same room: it replaces the
    // one playing on the same mix and player, the state says so of the
    // first, and what comes back at the end is the music and the volume
    // the room had before the first.
    let answer = server.applied(&announce("kitchen", &clip, Some("0.350")));
    let third = number_in(&answer);
    wait_for("the room hears the third clip", || {
        room.clip_frames() > long + short
    });
    let answer = server.applied(&announce("kitchen", &brief, Some("0.300")));
    let fourth = number_in(&answer);
    assert_eq!((third, fourth), (3, 4));
    assert_eq!(volume_in(&answer, "kitchen"), "0.300");
    assert_eq!(
        announcement_in(&answer, third).map(|(state, _, reason)| (state, reason)),
        Some((
            "displaced".to_string(),
            Some("replaced by announcement 4".to_string())
        )),
        "{}",
        answer
    );
    server.wait_for_all(&["announce owner=announce:3 id=4", "replaces=the-one-playing"]);
    assert_eq!(over(&server, fourth).0, "finished");
    assert_eq!(volume_in(&server.state(), "kitchen"), "0.250");
    settled(&room);
    assert_eq!(
        without_serial(&server.state()),
        without_serial(&before),
        "the house is as it was before any announcement"
    );
    let _ = std::fs::remove_file(&source);
}

#[test]
fn one_room_of_a_group_is_ducked_alone_and_the_group_is_ducked_together() {
    let media = MediaServer::start();
    let frames = 48_000 + 321;
    let clip = media.serve(
        "/api/tts_proxy/clip.wav",
        Route::File(Arc::new(wav(frames))),
    );
    let source = constant_source("rooms");
    let origin = media.origin();
    let mut server = mixing_server(&source, &origin, &["kitchen", "living"]);
    server.applied(
        r#"{"v":2,"t":"group_save","group":"downstairs","name":"Downstairs","zones":["kitchen","living"]}"#,
    );
    server.applied(r#"{"v":2,"t":"take","target":"downstairs","source":"stream"}"#);
    let kitchen = Room::listen(&server, "kitchen");
    let living = Room::listen(&server, "living");
    assert_eq!(playing_in(&server.state(), "downstairs").0, "stream");
    let before = server.state();

    // To one room of the playing group: that room alone.
    let from = now_at(&kitchen).max(now_at(&living));
    let answer = server.applied(&announce("kitchen", &clip, None));
    let id = number_in(&answer);
    assert_eq!(
        announcement_in(&answer, id).map(|(_, rooms, _)| rooms),
        Some(vec!["kitchen".to_string()])
    );
    assert_eq!(
        playing_in(&answer, "downstairs").0,
        "stream",
        "the group plays on"
    );
    assert_eq!(over(&server, id).0, "finished");
    settled(&kitchen);
    settled(&living);
    server.wait_for(&format!("id={} music-restored", id));
    let in_kitchen = heard(&kitchen.timeline(), from, frames);
    assert_eq!(
        in_kitchen.clip_last - in_kitchen.clip_first + 1,
        frames as u64
    );
    // The other room: the music's own samples in every frame, on chunks
    // with no gap, while the kitchen was ducked.
    let other: Vec<(u64, Frame)> = living
        .timeline()
        .into_iter()
        .filter(|(at, _)| *at >= from)
        .collect();
    assert!(
        other.first().unwrap().0 <= in_kitchen.duck_first
            && other.last().unwrap().0 >= in_kitchen.restored,
        "the living room was heard over the whole announcement"
    );
    for pair in other.windows(2) {
        assert_eq!(pair[1].0, pair[0].0 + 1, "no gap in the living room");
    }
    for (at, frame) in &other {
        assert_eq!(
            *frame,
            [SAMPLE, SAMPLE],
            "the living room is unducked at {}",
            at
        );
    }
    // In sync: a sequence carries the same timestamp in both rooms, the
    // ducked one and the one that is not.
    let stamped: HashMap<u32, u64> = living.stamps().into_iter().collect();
    let mut shared = 0;
    for (sequence, timestamp) in kitchen.stamps() {
        if let Some(theirs) = stamped.get(&sequence) {
            assert_eq!(*theirs, timestamp, "sequence {}", sequence);
            shared += 1;
        }
    }
    assert!(shared > 100, "the two rooms share a timeline: {}", shared);

    // To the group: every room of it, the same frames at the same place on
    // the timeline.
    let from = now_at(&kitchen).max(now_at(&living));
    let answer = server.applied(&announce("downstairs", &clip, None));
    let id = number_in(&answer);
    assert_eq!(
        announcement_in(&answer, id).map(|(_, rooms, _)| rooms),
        Some(vec!["kitchen".to_string(), "living".to_string()])
    );
    assert_eq!(over(&server, id).0, "finished");
    settled(&kitchen);
    settled(&living);
    server.wait_for(&format!("id={} music-restored", id));
    let in_kitchen = heard(&kitchen.timeline(), from, frames);
    let in_living = heard(&living.timeline(), from, frames);
    assert_eq!(in_kitchen, in_living, "both rooms, frame for frame");
    assert_eq!(
        without_serial(&server.state()),
        without_serial(&before),
        "the house is as it was"
    );
    let _ = std::fs::remove_file(&source);
}

#[test]
fn the_duck_is_full_before_the_clip_and_the_music_is_back_within_the_bound_in_frames() {
    let media = MediaServer::start();
    let source = constant_source("timing");
    let origin = media.origin();
    let server = mixing_server(&source, &origin, &["kitchen"]);
    let room = Room::listen(&server, "kitchen");
    // Clips of lengths that end at different places in a chunk.
    let lengths = [4_801usize, 12_345, 24_000, 30_007, 48_959];
    println!(
        "measured: clip_frames duck_ramp_frames clip_first_minus_full_duck \
         restored_minus_clip_last"
    );
    for (n, frames) in lengths.iter().enumerate() {
        let url = media.serve(
            &format!("/api/tts_proxy/timing-{}.wav", n),
            Route::File(Arc::new(wav(*frames))),
        );
        let from = now_at(&room);
        let answer = server.applied(&announce("kitchen", &url, None));
        assert_eq!(over(&server, number_in(&answer)).0, "finished");
        settled(&room);
        let h = heard(&room.timeline(), from, *frames);
        assert_eq!(h.clip_last - h.clip_first + 1, *frames as u64);
        // The way down, on the stream: ADR 0173's 9600 frames, less the
        // frames at each end whose 16-bit sample already is the level it
        // is heading for.
        let ramp = h.duck_full - h.duck_first + 1;
        assert!(
            ramp <= DUCK_FRAMES && ramp + 4 >= DUCK_FRAMES,
            "the duck ramp is {} frames",
            ramp
        );
        // ADR 0175, bound 1: the music is fully ducked no later than the
        // clip's first frame, so from that frame to the full duck is no
        // frames at all.
        assert!(
            h.duck_full < h.clip_first,
            "the full duck at {} is after the clip's first frame at {}",
            h.duck_full,
            h.clip_first
        );
        // ADR 0175, bound 2: from the clip's last frame, the music is its
        // own samples again within the restore ramp and one chunk.
        let back = h.restored - h.clip_last;
        assert!(
            back <= RESTORE_FRAMES + CHUNK_FRAMES,
            "the restore took {} frames",
            back
        );
        println!(
            "measured: {} {} {} {}",
            frames,
            ramp,
            h.clip_first - h.duck_full,
            back
        );
    }
    let _ = std::fs::remove_file(&source);
}

#[test]
fn a_clip_that_cannot_be_fetched_brings_the_music_back_at_once_and_says_failed() {
    let media = MediaServer::start();
    let elsewhere = MediaServer::start();
    let away = elsewhere.serve("/clip.wav", Route::File(Arc::new(wav(24_000))));
    let leaves = media.serve("/api/tts_proxy/moved.wav", Route::Redirect(away));
    let missing = media.url("/api/tts_proxy/missing.wav");

    let source = constant_source("fails");
    let origin = media.origin();
    let mut server = mixing_server(&source, &origin, &["kitchen"]);
    let room = Room::listen(&server, "kitchen");
    server.applied(r#"{"v":2,"t":"volume","zone":"kitchen","volume":0.250}"#);
    let before = server.state();
    let from = now_at(&room);

    // A clip that is not there: the command is accepted (the fetch happens
    // on the player's thread), and the failure puts everything back.
    let answer = server.applied(&announce("kitchen", &missing, Some("0.600")));
    let first = number_in(&answer);
    assert_eq!(playing_in(&answer, "kitchen").0, "stream");
    assert_eq!(volume_in(&answer, "kitchen"), "0.600");
    assert_eq!(
        over(&server, first),
        ("failed".to_string(), Some("http status 404".to_string()))
    );
    server.wait_for_all(&[
        "announce owner=announce:1 id=1 ended",
        "restored=stream",
        "outcome=failed",
        "failure=\"http status 404\"",
    ]);
    assert_eq!(without_serial(&server.state()), without_serial(&before));

    // A clip whose URL redirects out of the origin: the fetch is held to
    // the origin, so nothing is fetched from where the redirect points.
    let answer = server.applied(&announce("kitchen", &leaves, Some("0.600")));
    let second = number_in(&answer);
    let (how, why) = over(&server, second);
    assert_eq!(how, "failed");
    assert!(
        why.as_deref()
            .is_some_and(|w| w.starts_with("refused: origin ")),
        "{:?}",
        why
    );
    server.wait_for_all(&[
        "id=2 ended",
        "restored=stream",
        "failure=\"refused: origin ",
    ]);
    assert_eq!(without_serial(&server.state()), without_serial(&before));
    assert!(elsewhere.asked().is_empty(), "{:?}", elsewhere.asked());

    // The room heard no frame of any clip, and the music is its own
    // samples again: a duck that had begun came straight back.
    settled(&room);
    assert_eq!(room.clip_frames(), 0, "the room heard no clip");
    assert!(
        room.timeline()
            .iter()
            .filter(|(at, _)| *at >= from)
            .all(|(_, f)| f[0] == f[1] && f[1] >= DUCKED && f[1] <= SAMPLE),
        "nothing but the music, at or under its own level"
    );
    let _ = std::fs::remove_file(&source);
}

#[test]
fn a_server_says_who_it_is_and_refuses_by_name_what_it_cannot_announce() {
    let source = constant_source("identity");
    let url = "http://ha.example:8123/api/tts_proxy/abc.mp3";

    // An origin and no player: the identity route lists the origin, and an
    // announcement is refused for want of a player, after the target.
    let with_origin = server(&source, &["--announce-origin", "http://HA.example:8123/"]);
    let info = server_info(&with_origin);
    let id = id_of(&info);
    assert!(
        id.len() == 30
            && id.starts_with("chorus-server-")
            && id["chorus-server-".len()..]
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "{}",
        id
    );
    assert!(chorus_control::catalog::is_server_id(&id));
    assert_eq!(
        info,
        format!(
            r#"{{"v":2,"t":"server","id":"{}","software":"chorus-server {}","catalogs":[1,2],"announce_origins":["http://ha.example:8123"]}}"#,
            id,
            env!("CARGO_PKG_VERSION")
        )
    );
    let detail = refused(&with_origin, &announce("kitchen", url, None), "t");
    assert!(detail.starts_with("no-players:"), "{}", detail);
    refused(&with_origin, &announce("garage", url, None), "target");
    let detail = refused(
        &with_origin,
        &announce("kitchen", "http://ha.example/abc.mp3", None),
        "url",
    );
    assert!(detail.contains("http://ha.example:8123"), "{}", detail);
    refused(
        &with_origin,
        &announce("kitchen", "file:///etc/hostname", None),
        "url",
    );
    refused(
        &with_origin,
        &announce("kitchen", url, Some("1.500")),
        "volume",
    );
    drop(with_origin);

    // No origin: every announcement is refused naming `url`, and the list
    // is empty, not absent. The id is the key's: the same after a restart
    // with the same identity directory.
    let dir = std::env::temp_dir().join(fresh_id("chorus-ann-identity"));
    let dir = dir.display().to_string();
    let args = [
        "--source",
        source.to_str().unwrap(),
        "--slots",
        "2",
        "--zone",
        "kitchen",
        "--players",
        "1",
    ];
    let first = RunningServer::start_on("127.0.0.1:0", &["--identity-dir", &dir], &args);
    let info = server_info(&first);
    assert!(info.ends_with(r#""announce_origins":[]}"#), "{}", info);
    let detail = refused(&first, &announce("kitchen", url, None), "url");
    assert!(detail.starts_with("no-announce-origin:"), "{}", detail);
    let kept = id_of(&info);
    drop(first);
    let second = RunningServer::start_on("127.0.0.1:0", &["--identity-dir", &dir], &args);
    assert_eq!(
        id_of(&server_info(&second)),
        kept,
        "the id survives a restart"
    );
    assert_ne!(kept, id, "another key is another server");
    drop(second);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&source);
}
