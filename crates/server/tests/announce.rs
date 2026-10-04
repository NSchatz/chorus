//! The `announce` command and the server's identity route, on the real
//! binary (goal 18, ADR 0136; brief section 4.8's "HA's media and TTS URLs
//! from HA's own address").
//!
//! Every test runs the real `chorus-server` with stream slots and a control
//! plane. The clip is a generated WAV (a signal no two frames of which are
//! alike, as `alarm_stored_sources.rs` generates its stream) served by an
//! HTTP server on loopback inside the test, whose address is the server's
//! one `--announce-origin` (`--media-allow-loopback` exists for exactly
//! this and is never set in a deployment). The room is a protocol v2 player
//! session opened through the Linux client's own session code
//! (`common::Player`).
//!
//! The test names are the evidence:
//!
//! - an announcement in a room playing the stream: the answer is the state
//!   with the room's group on the player, `via` `announce`, the room at the
//!   announcement's volume CLAMPED to its limit; the room hears the clip's
//!   own samples; and when the clip ends the group plays the stream again
//!   and the room has the volume it had. A URL from another origin is
//!   refused naming `url` and changes nothing; and an announcement during
//!   another replaces it and still restores what played before the first;
//! - a clip that answers 404, and one whose URL redirects out of the
//!   origin, put the room back at once;
//! - `GET /api/server` answers the committed shape, its `id` survives a
//!   restart with the same identity directory, and a server with no origin
//!   or no player refuses every announcement by name.
//!
//! Nothing here is timing evidence: what is graded is values, orders and
//! log lines, each waited for with a generous bound and none of them a
//! duration.

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

/// Frame `n` of the generated signal: never silence, never the configured
/// stream's sample, and no two alike.
fn ramp_frame(n: usize) -> Frame {
    [1 + (n % 30_011) as i16, -(1 + (n / 30_011) as i16)]
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

    /// The frames of the generated signal heard so far: everything that is
    /// neither the configured stream nor silence.
    fn signal(&self) -> Vec<Frame> {
        self.frames()
            .into_iter()
            .filter(|f| *f != [SAMPLE, SAMPLE] && *f != [0, 0])
            .collect()
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

/// The state with its serial taken out, for "nothing changed".
fn without_serial(state: &str) -> String {
    match parsed(state) {
        Value::Obj(members) => json::write(&Value::Obj(
            members.into_iter().filter(|(k, _)| k != "serial").collect(),
        )),
        other => panic!("the state is {:?}", other),
    }
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

#[test]
fn an_announcement_plays_in_the_room_then_what_it_played_and_its_volume_come_back() {
    let media = MediaServer::start();
    // Four seconds of the signal: long enough that the room is seen playing
    // it, whatever the host is doing.
    let clip = media.serve(
        "/api/tts_proxy/clip.wav",
        Route::File(Arc::new(wav(4 * 48_000))),
    );
    let short = media.serve(
        "/api/tts_proxy/short.wav",
        Route::File(Arc::new(wav(24_000))),
    );
    let elsewhere = MediaServer::start();
    let off_list = elsewhere.serve("/clip.wav", Route::File(Arc::new(wav(24_000))));

    let source = constant_source("plays");
    let origin = media.origin();
    let mut server = server(
        &source,
        &[
            "--players",
            "1",
            "--media-allow-loopback",
            "--announce-origin",
            &origin,
        ],
    );
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
    // the state with the group on the player, `via` `announce`, and the
    // volume clamped to the limit.
    let before = server.state();
    let answer = server.applied(&announce("kitchen", &clip, Some("0.900")));
    assert_eq!(
        playing_in(&answer, "kitchen"),
        ("player:p0".to_string(), Some("announce".to_string())),
        "{}",
        answer
    );
    assert_eq!(
        volume_in(&answer, "kitchen"),
        "0.400",
        "clamped to the limit"
    );
    server.wait_for_all(&[
        "announce owner=announce:1",
        "target=kitchen",
        "plays=player:p0",
        "previous=stream",
    ]);

    // The room hears the clip's own samples, from its first frame, in order.
    wait_for("the room hears the clip", || room.signal().len() >= 4_800);
    let heard = room.signal();
    for (n, frame) in heard.iter().take(4_800).enumerate() {
        assert_eq!(*frame, ramp_frame(n), "frame {} of the clip", n);
    }

    // When the clip ends: the stream again, the volume it had, no record.
    wait_for("the group plays the stream again", || {
        playing_in(&server.state(), "kitchen").0 == "stream"
    });
    let after = server.state();
    assert_eq!(playing_in(&after, "kitchen"), ("stream".to_string(), None));
    assert_eq!(volume_in(&after, "kitchen"), "0.250", "the volume it had");
    assert_eq!(
        without_serial(&after),
        without_serial(&before),
        "the house is as it was before the announcement"
    );
    server.wait_for_all(&["announce owner=announce:1 ended", "restored=stream"]);
    wait_for("the room hears the configured stream again", || {
        room.last().is_some_and(|f| f == [SAMPLE, SAMPLE])
    });

    // The player was given back: a second announcement, with no volume,
    // plays on it and leaves the room's volume alone throughout.
    let answer = server.applied(&announce("kitchen", &short, None));
    assert_eq!(playing_in(&answer, "kitchen").0, "player:p0", "{}", answer);
    assert_eq!(volume_in(&answer, "kitchen"), "0.250");
    wait_for("the second clip ends", || {
        playing_in(&server.state(), "kitchen").0 == "stream"
    });
    assert_eq!(volume_in(&server.state(), "kitchen"), "0.250");
    server.wait_for_all(&["announce owner=announce:2 ended", "restored=stream"]);

    // An announcement during another, in the same group: it replaces the
    // one playing on the same player, and what comes back at the end is
    // what the room played, and the volume it had, before the first.
    server.applied(&announce("kitchen", &clip, Some("0.350")));
    let answer = server.applied(&announce("kitchen", &short, Some("0.300")));
    assert_eq!(playing_in(&answer, "kitchen").0, "player:p0", "{}", answer);
    assert_eq!(volume_in(&answer, "kitchen"), "0.300");
    server.wait_for_all(&["announce owner=announce:3", "replaces=the-one-playing"]);
    wait_for("the replacing clip ends", || {
        playing_in(&server.state(), "kitchen").0 == "stream"
    });
    assert_eq!(volume_in(&server.state(), "kitchen"), "0.250");
    server.wait_for_all(&["announce owner=announce:3 ended", "restored=stream"]);
    assert_eq!(
        without_serial(&server.state()),
        without_serial(&before),
        "the house is as it was before any announcement"
    );
    let _ = std::fs::remove_file(&source);
}

#[test]
fn a_clip_that_cannot_be_fetched_puts_the_room_back_at_once() {
    let media = MediaServer::start();
    let elsewhere = MediaServer::start();
    let away = elsewhere.serve("/clip.wav", Route::File(Arc::new(wav(24_000))));
    let leaves = media.serve("/api/tts_proxy/moved.wav", Route::Redirect(away));
    let missing = media.url("/api/tts_proxy/missing.wav");

    let source = constant_source("fails");
    let origin = media.origin();
    let mut server = server(
        &source,
        &[
            "--players",
            "1",
            "--media-allow-loopback",
            "--announce-origin",
            &origin,
        ],
    );
    let room = Room::listen(&server, "kitchen");
    server.applied(r#"{"v":2,"t":"volume","zone":"kitchen","volume":0.250}"#);
    let before = server.state();

    // A clip that is not there: the command is accepted (the fetch happens
    // on the player's thread), and the failure puts everything back.
    let answer = server.applied(&announce("kitchen", &missing, Some("0.600")));
    assert_eq!(playing_in(&answer, "kitchen").0, "player:p0");
    assert_eq!(volume_in(&answer, "kitchen"), "0.600");
    server.wait_for_all(&[
        "announce owner=announce:1 ended",
        "restored=stream",
        "failure=\"http status 404\"",
    ]);
    assert_eq!(without_serial(&server.state()), without_serial(&before));

    // A clip whose URL redirects out of the origin: the fetch is held to
    // the origin, so nothing is fetched from where the redirect points.
    server.applied(&announce("kitchen", &leaves, Some("0.600")));
    server.wait_for_all(&[
        "announce owner=announce:2 ended",
        "restored=stream",
        "failure=\"refused: origin ",
    ]);
    assert_eq!(without_serial(&server.state()), without_serial(&before));
    assert!(elsewhere.asked().is_empty(), "{:?}", elsewhere.asked());
    assert!(
        room.signal().is_empty(),
        "the room heard nothing but the stream"
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
