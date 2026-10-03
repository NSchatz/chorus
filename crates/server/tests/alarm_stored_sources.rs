//! An alarm whose source is a stored stream URL, on the real binary (goal 17;
//! K80, brief section 4.8's "stored alarm stream URLs").
//!
//! Every test runs the real `chorus-server` with stream slots, a control
//! plane and (where the test says) network media players, its civil clock
//! started thirty schedule seconds before 07:00 and its schedule run ten
//! times faster (`--civil-time-from`, `--schedule-time-scale`), as
//! `alarms_sleep_autoplay.rs` does, so the alarm rings three real seconds
//! after start. The media is served by an HTTP server on loopback inside the
//! test (`--media-allow-loopback`, which exists for exactly this and is never
//! set in a deployment). The room is a protocol v2 player session opened
//! through the Linux client's own session code (`common::Player`).
//!
//! The test names are the evidence:
//!
//! - the alarm fires with `--upnp` OFF and the room hears the stored
//!   stream's own samples (a generated signal no two frames of which are
//!   alike), with the stored source's name as what the room is playing;
//! - the same with the UPnP renderers running (their manager thread, not
//!   the conductor, then takes the players' reports);
//! - a URL that answers 404, a URL the fetch policy refuses (the server's
//!   own control port), no free player and no player at all each ring the
//!   fallback chime with the reason in the log, and the alarm keeps ringing;
//! - a scheme that is not `http` or `https` never gets as far as an alarm:
//!   `source_store` refuses it by name;
//! - an Icecast-style endless stream is stopped by the alarm's stop: the
//!   player is given back and the media server sees its connection closed.
//!
//! Nothing here is timing evidence: what is graded is values, orders and
//! log lines, each waited for with a generous bound and none of them a
//! duration. Wall clock: each test runs about four to eight seconds, most of
//! it the three seconds to 07:00.

mod common;

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chorus_control::json::{self, Value};
use chorus_protocol::v2::RoomVolume;
use chorus_protocol::AudioChunk;
use common::{fresh_id, Player, RunningServer};

const RATE_HZ: u32 = 48_000;
/// The configured stream: every sample this value.
const SAMPLE: i16 = 0x1234;
const LIMIT: Duration = Duration::from_secs(20);

type Frame = [i16; 2];

fn constant_source(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("chorus-ass-{}-{}.pcm", name, std::process::id()));
    let bytes: Vec<u8> = std::iter::repeat_n(SAMPLE.to_le_bytes(), RATE_HZ as usize * 2 * 40)
        .flatten()
        .collect();
    std::fs::write(&path, bytes).unwrap();
    path
}

fn utc_zone() -> String {
    format!(
        "{}/../../fixtures/schedule/Etc_UTC.slim.tzif",
        env!("CARGO_MANIFEST_DIR")
    )
}

/// The server, thirty schedule seconds (three real ones) before 07:00 on a
/// Monday, serving `zones`, with `extra` flags.
fn server(source: &Path, zones: &[&str], extra: &[&str]) -> RunningServer {
    let mut args: Vec<String> = [
        "--source",
        source.to_str().unwrap(),
        "--slots",
        "3",
        "--max-clients",
        "6",
        "--tz",
        &utc_zone(),
        "--civil-time-from",
        "2026-10-05T06:59:30Z",
        "--schedule-time-scale",
        "10",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for z in zones {
        args.push("--zone".into());
        args.push(z.to_string());
    }
    args.extend(extra.iter().map(|s| s.to_string()));
    // The renderers' identity rests on a persisted key, so `--upnp` takes
    // an identity directory rather than the throwaway identity.
    let upnp = extra.contains(&"--upnp");
    let dir = std::env::temp_dir().join(fresh_id("chorus-ass-identity"));
    let dir = dir.display().to_string();
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut s = if upnp {
        RunningServer::start_on("127.0.0.1:0", &["--identity-dir", &dir], &refs)
    } else {
        RunningServer::start(&refs)
    };
    s.wait_for("civil tz=");
    s
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

// ----- the generated signal ------------------------------------------------------

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
    /// A file with a length.
    File(Arc<Vec<u8>>, String),
    /// Icecast-style: no length, `audio/L16` at the server's rate, the
    /// generated signal for as long as the peer reads.
    Endless,
}

/// An HTTP server on loopback serving what the test put in it. A path with
/// no route answers 404.
struct MediaServer {
    address: String,
    routes: Arc<Mutex<HashMap<String, Route>>>,
    /// Endless streams whose peer closed (a write failed).
    closed: Arc<AtomicUsize>,
    /// Endless streams that began.
    opened: Arc<AtomicUsize>,
}

impl MediaServer {
    fn start() -> MediaServer {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let routes: Arc<Mutex<HashMap<String, Route>>> = Arc::default();
        let closed = Arc::new(AtomicUsize::new(0));
        let opened = Arc::new(AtomicUsize::new(0));
        {
            let (routes, closed, opened) = (
                Arc::clone(&routes),
                Arc::clone(&closed),
                Arc::clone(&opened),
            );
            thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(stream) = stream else { return };
                    let (routes, closed, opened) = (
                        Arc::clone(&routes),
                        Arc::clone(&closed),
                        Arc::clone(&opened),
                    );
                    thread::spawn(move || serve_media(stream, &routes, &closed, &opened));
                }
            });
        }
        MediaServer {
            address,
            routes,
            closed,
            opened,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.address, path)
    }

    fn serve(&self, path: &str, route: Route) -> String {
        self.routes.lock().unwrap().insert(path.to_string(), route);
        self.url(path)
    }
}

fn serve_media(
    mut stream: TcpStream,
    routes: &Mutex<HashMap<String, Route>>,
    closed: &AtomicUsize,
    opened: &AtomicUsize,
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
    let route = routes.lock().unwrap().get(&path).cloned();
    match route {
        None => {
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
        }
        Some(Route::File(body, content_type)) => {
            let _ = stream.write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n",
                    content_type,
                    body.len()
                )
                .as_bytes(),
            );
            let _ = stream.write_all(&body);
        }
        Some(Route::Endless) => {
            opened.fetch_add(1, Ordering::SeqCst);
            let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
            let ok = stream
                .write_all(
                    format!(
                        "HTTP/1.0 200 OK\r\nContent-Type: audio/L16;rate={};channels=2\r\n\
                         icy-name: Example Radio\r\n\r\n",
                        RATE_HZ
                    )
                    .as_bytes(),
                )
                .is_ok();
            let mut n = 0usize;
            // As fast as the peer takes it: the player reads at the pace it
            // plays, so the socket's own back-pressure paces this.
            while ok {
                let block: Vec<u8> = (n..n + 4_800)
                    .flat_map(|i| {
                        let f = ramp_frame(i);
                        [f[0].to_be_bytes(), f[1].to_be_bytes()].concat()
                    })
                    .collect();
                if stream.write_all(&block).is_err() {
                    break;
                }
                n += 4_800;
            }
            closed.fetch_add(1, Ordering::SeqCst);
        }
    }
}

// ----- what the room hears -------------------------------------------------------

/// A room's player session, read on a thread of its own.
struct Room {
    chunks: Arc<Mutex<Vec<AudioChunk>>>,
    volumes: Arc<Mutex<Vec<RoomVolume>>>,
    keep: Arc<AtomicBool>,
}

impl Room {
    fn listen(server: &RunningServer, zone: &str) -> Room {
        let endpoint = fresh_id(&format!("ass-{}", zone));
        server.applied(&format!(
            r#"{{"v":1,"t":"attach","zone":"{}","endpoint":"{}"}}"#,
            zone, endpoint
        ));
        let mut player = Player::connect(&server.audio, &endpoint, 0);
        let chunks: Arc<Mutex<Vec<AudioChunk>>> = Arc::default();
        let volumes: Arc<Mutex<Vec<RoomVolume>>> = Arc::default();
        let keep = Arc::new(AtomicBool::new(true));
        {
            let (chunks, volumes, keep) =
                (Arc::clone(&chunks), Arc::clone(&volumes), Arc::clone(&keep));
            thread::spawn(move || {
                while keep.load(Ordering::SeqCst) {
                    if let Some(chunk) = player.next_chunk(Duration::from_millis(100)) {
                        chunks.lock().unwrap().push(chunk);
                    }
                    *volumes.lock().unwrap() = player.room_volumes();
                }
            });
        }
        let room = Room {
            chunks,
            volumes,
            keep,
        };
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

    fn gains(&self) -> Vec<u16> {
        self.volumes
            .lock()
            .unwrap()
            .iter()
            .map(|r| r.gain)
            .collect()
    }
}

impl Drop for Room {
    fn drop(&mut self) {
        self.keep.store(false, Ordering::SeqCst);
    }
}

// ----- the state -----------------------------------------------------------------

fn parsed(state: &str) -> Value {
    json::parse(state).unwrap_or_else(|e| panic!("{}: {:?}", state, e))
}

fn list<'a>(state: &'a Value, key: &str) -> &'a [Value] {
    match state.get(key) {
        Some(Value::Arr(items)) => items,
        _ => &[],
    }
}

/// What a room's group plays, and the title and `via` of its now-playing
/// record.
fn playing(server: &RunningServer, zone: &str) -> (String, Option<String>, Option<String>) {
    let state = parsed(&server.state());
    let group = list(&state, "groups")
        .iter()
        .find(|g| g.get("id").and_then(Value::as_str) == Some(zone))
        .unwrap_or_else(|| panic!("no group {}", zone))
        .clone();
    let text = |v: Option<&Value>, k: &str| {
        v.and_then(|v| v.get(k))
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    let record = group.get("now_playing");
    (
        text(Some(&group), "source").unwrap_or_default(),
        text(record, "title"),
        text(record, "via"),
    )
}

fn ringing(server: &RunningServer, alarm: &str) -> bool {
    list(&parsed(&server.state()), "alarms")
        .iter()
        .find(|a| a.get("alarm").and_then(Value::as_str) == Some(alarm))
        .and_then(|a| a.get("ringing").and_then(Value::as_bool))
        .unwrap_or(false)
}

fn store_url(server: &RunningServer, id: &str, url: &str) {
    server.applied(&format!(
        r#"{{"v":2,"t":"source_store","id":"{}","kind":"url","value":"{}","name":"Morning radio"}}"#,
        id, url
    ));
}

/// An alarm at 07:00, once, ramping over twenty schedule seconds (two real
/// ones) to 0.500 and ringing for five schedule minutes.
fn alarm(server: &RunningServer, id: &str, target: &str, source: &str) {
    server.applied(&format!(
        r#"{{"v":2,"t":"alarm_set","alarm":"{}","target":"{}","time":"07:00","days":[],"source":"{}","volume":0.500,"ramp_s":20,"duration_min":5,"enabled":true}}"#,
        id, target, source
    ));
}

/// The bell as the server renders it, for "the room hears the chime".
fn bell() -> Vec<Frame> {
    chorus_schedule::render(
        chorus_schedule::Chime::Bell,
        RATE_HZ,
        2,
        chorus_schedule::PcmFormat::S16Le,
    )
    .unwrap()
    .as_chunks::<4>()
    .0
    .iter()
    .map(|f| {
        [
            i16::from_le_bytes([f[0], f[1]]),
            i16::from_le_bytes([f[2], f[3]]),
        ]
    })
    .collect()
}

/// The room rings the fallback chime: its group plays `chime:bell`, the
/// alarm rings, and what it hears after the configured stream is the bell.
fn assert_rings_the_bell(server: &RunningServer, room: &Room, zone: &str, alarm: &str) {
    wait_for("the group plays the bell", || {
        playing(server, zone).0 == "chime:bell"
    });
    assert!(ringing(server, alarm), "the alarm still rings");
    let bell = bell();
    // The bell's first frames may be zero (a chime starts at a zero
    // crossing), so find it by its first loud frames.
    let loud: Vec<Frame> = bell.iter().copied().filter(|f| *f != [0, 0]).collect();
    wait_for("a second of the bell is heard", || {
        room.signal().len() >= RATE_HZ as usize / 2
    });
    let heard = room.signal();
    let n = 4_000.min(heard.len()).min(loud.len());
    assert_eq!(
        heard[..n],
        loud[..n],
        "what the room hears is the bell, frame for frame"
    );
}

// ----- the tests -----------------------------------------------------------------

/// The core of the goal's line: with `--upnp` OFF, an alarm with a stored
/// stream URL fires, the room hears that stream's samples, the ramp runs,
/// the room shows the stored source's name, and `alarm_stop` restores the
/// room and gives the player back.
fn a_stored_url_alarm_plays_its_stream(extra: &[&str], prefix: &str) {
    let source = constant_source(&format!("plays-{}", prefix));
    let media = MediaServer::start();
    // Twenty seconds of the signal: longer than the test listens.
    let url = media.serve(
        "/radio.wav",
        Route::File(Arc::new(wav(RATE_HZ as usize * 20)), "audio/wav".into()),
    );
    let mut flags = vec!["--players", "1", "--media-allow-loopback"];
    flags.extend_from_slice(extra);
    let mut server = server(&source, &["kitchen"], &flags);
    server.applied(r#"{"v":1,"t":"volume","zone":"kitchen","volume":0.400}"#);
    let kitchen = Room::listen(&server, "kitchen");
    store_url(&server, "radio", &url);
    alarm(&server, "wake", "kitchen", "stored:radio");

    wait_for("the alarm rings", || ringing(&server, "wake"));
    wait_for("the kitchen plays the player", || {
        playing(&server, "kitchen").0 == "player:p0"
    });
    server.wait_for(&format!(
        "{} player p0 plays for alarm:wake on kitchen via alarm",
        prefix
    ));
    server.wait_for("schedule alarm=wake started stored=radio plays=player:p0");
    wait_for("the room shows the stored source's name", || {
        let (_, title, via) = playing(&server, "kitchen");
        title.as_deref() == Some("Morning radio") && via.as_deref() == Some("alarm")
    });
    // Two seconds of the stream, sample for sample from its first frame.
    wait_for("two seconds of the stored stream", || {
        kitchen.signal().len() >= RATE_HZ as usize * 2
    });
    let heard = kitchen.signal();
    for (n, frame) in heard.iter().enumerate() {
        assert_eq!(*frame, ramp_frame(n), "frame {} of the stored stream", n);
    }
    // The ramp: from silence up to the alarm's volume, in steps.
    wait_for("the ramp reaches 0.500", || {
        kitchen.gains().last() == Some(&500)
    });
    let gains = kitchen.gains();
    let zero = gains.iter().position(|g| *g == 0).expect("a silent start");
    assert!(
        gains[zero..].windows(2).all(|w| w[1] >= w[0]),
        "the rise is monotone: {:?}",
        gains
    );
    assert!(gains[zero..].len() >= 8, "a ramp is stepped: {:?}", gains);

    // alarm_stop: the room as it was, and the player given back.
    server.applied(r#"{"v":2,"t":"alarm_stop","alarm":"wake"}"#);
    wait_for("the room is restored", || {
        let (source, title, _) = playing(&server, "kitchen");
        source == "stream" && title.is_none() && !ringing(&server, "wake")
    });
    server.wait_for(&format!("{} player p0 released for alarm:wake", prefix));
    wait_for("the configured stream again", || {
        kitchen.last() == Some([SAMPLE, SAMPLE])
    });
    wait_for("the restored gain is sent", || {
        kitchen.gains().last() == Some(&400)
    });
    server.drain();
    assert!(
        !server.seen.iter().any(|l| l.contains("fallback=chime")),
        "no fallback: the stream played"
    );
    let _ = std::fs::remove_file(&source);
}

#[test]
fn an_alarm_with_a_stored_url_fires_with_upnp_off_and_the_room_hears_that_stream() {
    a_stored_url_alarm_plays_its_stream(&[], "media");
}

#[test]
fn an_alarm_with_a_stored_url_plays_with_the_upnp_renderers_running_too() {
    // No datagram leaves the machine: the discovery socket's group is a
    // socket of this test's, as `upnp_control_point.rs` does.
    let notify = UdpSocket::bind("127.0.0.1:0").unwrap();
    let group = notify.local_addr().unwrap().to_string();
    a_stored_url_alarm_plays_its_stream(
        &[
            "--upnp",
            "--upnp-listen",
            "127.0.0.1:0",
            "--upnp-ssdp-port",
            "0",
            "--upnp-ssdp-group",
            &group,
        ],
        "upnp",
    );
}

#[test]
fn a_stored_url_that_answers_404_rings_the_chime_with_the_reason() {
    let source = constant_source("404");
    let media = MediaServer::start();
    let mut server = server(
        &source,
        &["kitchen"],
        &["--players", "1", "--media-allow-loopback"],
    );
    let kitchen = Room::listen(&server, "kitchen");
    store_url(&server, "radio", &media.url("/gone.mp3"));
    alarm(&server, "wake", "kitchen", "stored:radio");
    wait_for("the alarm rings", || ringing(&server, "wake"));
    let line = server.wait_for("schedule alarm=wake fallback=chime reason=stream-failed");
    assert!(
        line.contains("wanted=stored:radio plays=chime:bell")
            && line.contains("detail=\"http status 404\""),
        "{}",
        line
    );
    assert_rings_the_bell(&server, &kitchen, "kitchen", "wake");
    // The player was given back: nothing holds it.
    server.wait_for("media player p0 failed: http status 404");
    let _ = std::fs::remove_file(&source);
}

#[test]
fn a_stored_url_the_fetch_policy_refuses_rings_the_chime_and_a_refused_scheme_is_never_stored() {
    let source = constant_source("refused");
    let mut server = server(
        &source,
        &["kitchen"],
        &["--players", "1", "--media-allow-loopback"],
    );
    let kitchen = Room::listen(&server, "kitchen");
    // A scheme that is not http or https is refused at the door, by name:
    // it never reaches an alarm.
    for value in ["file:///etc/hostname", "ftp://radio.example/stream.mp3"] {
        let (status, answer) = server.command(&format!(
            r#"{{"v":2,"t":"source_store","id":"bad","kind":"url","value":"{}","name":"Bad"}}"#,
            value
        ));
        assert!(
            status.contains("400") || status.contains("422"),
            "{}",
            status
        );
        assert!(
            answer.contains(r#""field":"value""#) && answer.contains("'http://' or 'https://'"),
            "{}",
            answer
        );
    }
    assert!(!server.state().contains("stored_sources"));
    // A URL the fetch policy refuses when it is played: this server's own
    // control port, refused even where loopback is allowed (brief 4.8).
    store_url(
        &server,
        "inward",
        &format!("http://{}/api/state", server.control),
    );
    alarm(&server, "wake", "kitchen", "stored:inward");
    wait_for("the alarm rings", || ringing(&server, "wake"));
    let line = server.wait_for("schedule alarm=wake fallback=chime reason=url-refused");
    assert!(line.contains("detail=\"refused: "), "{}", line);
    assert_rings_the_bell(&server, &kitchen, "kitchen", "wake");
    let _ = std::fs::remove_file(&source);
}

#[test]
fn no_free_player_rings_the_chime_and_the_other_alarm_keeps_the_player() {
    let source = constant_source("busy");
    let media = MediaServer::start();
    let url = media.serve("/live", Route::Endless);
    let mut server = server(
        &source,
        &["kitchen", "bedroom"],
        &["--players", "1", "--media-allow-loopback"],
    );
    let kitchen = Room::listen(&server, "kitchen");
    let bedroom = Room::listen(&server, "bedroom");
    store_url(&server, "radio", &url);
    // Two alarms at the same minute, one player: alarms fire in id order.
    alarm(&server, "a-kitchen", "kitchen", "stored:radio");
    alarm(&server, "b-bedroom", "bedroom", "stored:radio");
    wait_for("both alarms ring", || {
        ringing(&server, "a-kitchen") && ringing(&server, "b-bedroom")
    });
    let line = server.wait_for("schedule alarm=b-bedroom fallback=chime reason=no-free-player");
    assert!(
        line.contains("detail=\"players: no free player: all 1 are in use\""),
        "{}",
        line
    );
    assert_rings_the_bell(&server, &bedroom, "bedroom", "b-bedroom");
    wait_for("the kitchen plays the player", || {
        playing(&server, "kitchen").0 == "player:p0"
    });
    wait_for("the kitchen hears the stream", || {
        kitchen.signal().len() >= RATE_HZ as usize
    });
    for (n, frame) in kitchen.signal().iter().enumerate() {
        assert_eq!(*frame, ramp_frame(n), "frame {} of the stream", n);
    }
    let _ = std::fs::remove_file(&source);
}

#[test]
fn a_server_with_no_players_rings_the_chime() {
    let source = constant_source("none");
    let mut server = server(&source, &["kitchen"], &[]);
    let kitchen = Room::listen(&server, "kitchen");
    store_url(&server, "radio", "https://radio.example/stream.mp3");
    alarm(&server, "wake", "kitchen", "stored:radio");
    wait_for("the alarm rings", || ringing(&server, "wake"));
    let line = server.wait_for("schedule alarm=wake fallback=chime reason=no-players");
    assert!(
        line.contains("this server was started without --players"),
        "{}",
        line
    );
    assert_rings_the_bell(&server, &kitchen, "kitchen", "wake");
    let _ = std::fs::remove_file(&source);
}

#[test]
fn a_stored_spotify_source_rings_the_chime_because_soloist_alarms_are_off() {
    let source = constant_source("spotify");
    let mut server = server(
        &source,
        &["kitchen"],
        &["--players", "1", "--media-allow-loopback"],
    );
    let kitchen = Room::listen(&server, "kitchen");
    server.applied(
        r#"{"v":2,"t":"source_store","id":"wake-list","kind":"spotify","value":"spotify:playlist:37i9dQZF1DXexample0000","name":"Wake up"}"#,
    );
    alarm(&server, "wake", "kitchen", "stored:wake-list");
    wait_for("the alarm rings", || ringing(&server, "wake"));
    server.wait_for(
        "schedule alarm=wake fallback=chime reason=soloist-off wanted=stored:wake-list \
         plays=chime:bell",
    );
    assert_rings_the_bell(&server, &kitchen, "kitchen", "wake");
    let _ = std::fs::remove_file(&source);
}

#[test]
fn an_icecast_style_endless_stream_is_stopped_by_the_alarms_stop() {
    let source = constant_source("endless");
    let media = MediaServer::start();
    let url = media.serve("/live", Route::Endless);
    let mut server = server(
        &source,
        &["kitchen"],
        &["--players", "1", "--media-allow-loopback"],
    );
    let kitchen = Room::listen(&server, "kitchen");
    store_url(&server, "radio", &url);
    alarm(&server, "wake", "kitchen", "stored:radio");
    wait_for("the alarm rings", || ringing(&server, "wake"));
    wait_for("the kitchen plays the player", || {
        playing(&server, "kitchen").0 == "player:p0"
    });
    wait_for("two seconds of the endless stream", || {
        kitchen.signal().len() >= RATE_HZ as usize * 2
    });
    for (n, frame) in kitchen.signal().iter().enumerate() {
        assert_eq!(*frame, ramp_frame(n), "frame {} of the endless stream", n);
    }
    assert_eq!(media.opened.load(Ordering::SeqCst), 1);
    assert_eq!(
        media.closed.load(Ordering::SeqCst),
        0,
        "the stream is still being read"
    );
    server.applied(r#"{"v":2,"t":"alarm_stop","alarm":"wake"}"#);
    wait_for("the room is restored", || {
        playing(&server, "kitchen").0 == "stream" && !ringing(&server, "wake")
    });
    server.wait_for("media player p0 released for alarm:wake");
    wait_for("the media server sees its stream closed", || {
        media.closed.load(Ordering::SeqCst) == 1
    });
    wait_for("the configured stream again", || {
        kitchen.last() == Some([SAMPLE, SAMPLE])
    });
    assert_eq!(
        media.opened.load(Ordering::SeqCst),
        1,
        "nothing fetched it again"
    );
    let _ = std::fs::remove_file(&source);
}
