//! A fake Spotify Soloist, for tests. It is not Soloist, contains nothing of
//! Soloist, and was written only from Soloist's public documentation (the
//! command-line and WebSocket API reference pages, read 2026-10-03; the
//! digest and citations are in `docs/soloist.md`). No image and no release
//! carries it: this crate has no binary target, and the program the tests
//! run is an example of `crates/soloistd`.
//!
//! # What is documented behaviour
//!
//! - **The command line**: every documented option in its short and long
//!   form (`-n/--device-name`, `-k/--api-key`, `-D/--data-dir`,
//!   `-C/--cache-dir`, `-z/--cache-size`, `-d/--pipewire-device`,
//!   `-i/--initial-volume`, `-w/--ws`, `-v/--verbose`, `-V/--version`,
//!   `-h/--help`). `--device-name` and `--api-key` are required; a cache
//!   size must be 0 or at least 100.
//! - **Exit codes**: 0 for `--help`, `--version` and a normal shutdown; 1
//!   for a general failure (invalid arguments, missing required options,
//!   "another Spotify Soloist process already using the data directory",
//!   a startup failure); 10 for an expired build.
//! - **The data directory's files**: `soloist.pid`, and with `--ws`
//!   `ws.addr` ("Bind address") and `ws.port` ("Actual listening port"),
//!   written after the server starts and removed on shutdown. Port 0 lets
//!   the operating system choose.
//! - **A stored session**: "Use the same data directory across restarts to
//!   keep the same device identity and stored Spotify Connect session": a
//!   login survives a restart with the same data directory.
//! - **The WebSocket API**: JSON text frames; `auth_state` on connect, and
//!   `playback_state` too when logged in; the three query commands answer
//!   with their event and no `command_result`; every control command needs
//!   a login and answers `command_result`, then the events its effect
//!   causes; a refused message gets `error` on the originating connection
//!   only, the connection stays open, and the unauthenticated case uses the
//!   one documented message, "command requires authentication"; events are
//!   broadcast to every connection.
//! - **The bind failure**: Soloist "starts without the WebSocket API and
//!   logs a warning" (`FAKE_SOLOIST_NO_WS=1`).
//!
//! # What is assumed (the documentation does not state it)
//!
//! - **`--version` output**: whatever `FAKE_SOLOIST_VERSION` holds; the
//!   real format is not documented.
//! - **Signals**: SIGTERM and SIGINT are a normal shutdown (exit 0, files
//!   removed).
//! - **The data-directory lock**: an exclusive `flock` on `soloist.pid`,
//!   which also holds the process id as decimal text.
//! - **`ws.addr` and `ws.port` format**: the value and a line feed.
//! - **The audio**: Soloist plays into PipeWire; this fake has no PipeWire
//!   and writes the FIFO itself, standing in for the pipe-tunnel sink: the
//!   device `chorus-r<i>` is the file `$FAKE_SOLOIST_PIPE_DIR/r<i>.pcm`,
//!   float32 little-endian, 44.1 kHz, stereo, opened read-write and
//!   non-blocking, and a full pipe drops audio, as the sink does. The
//!   signal is [`expected_frame`]: a ramp that names its URI and frame.
//! - **Which events a change causes**, beyond the event's own description:
//!   `play` with a URI gives `track_changed` then `playback_changed`;
//!   `deactivate` pauses a playing session.
//! - **The API key** is not checked. It is written to stderr once, on
//!   purpose: the supervisor's redaction is tested against it.
//!
//! # Test controls (environment, or `KEY=VALUE` lines of the file
//! `FAKE_SOLOIST_CONF`, which win and are read at every start)
//!
//! `FAKE_SOLOIST_VERSION`, `FAKE_SOLOIST_EXPIRED=1` (exit 10),
//! `FAKE_SOLOIST_FAIL=1` (exit 1), `FAKE_SOLOIST_NO_WS=1`,
//! `FAKE_SOLOIST_PIPE_DIR`, `FAKE_SOLOIST_ARGV_LOG` (a file that gets one
//! JSON array of the arguments per start), `FAKE_SOLOIST_COMMAND_LOG` (a
//! file that gets every accepted WebSocket control command, one a line in
//! the order accepted: its name, then its `uri` or `volume`), and `FAKE_SOLOIST_CONTROL`: a
//! Unix socket on which a test plays "the Spotify app", one command a line
//! (`login`, `logout`, `play <uri>`, `pause`, `resume`, `volume <n>`,
//! `drop-ws`, `exit <code>`, and `stall`: from then on a WebSocket `play`
//! is accepted and answered and nothing starts playing), each answered `ok`
//! or `error: <why>`.

#![warn(missing_docs)]

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chorus_control::json::{self, Value};
use chorus_soloist::ws::{self, Decoder, Message, Opcode, Role};
use chorus_soloist::{
    encode_frame, PCM_FRAME_BYTES, PCM_RATE, PID_FILE, WS_ADDR_FILE, WS_PORT_FILE,
};
use chorus_soloistd::sys;

/// The version printed when `FAKE_SOLOIST_VERSION` is not set.
pub const DEFAULT_VERSION: &str = "Soloist 0.0.0 (chorus fake, no build time)";

/// `O_NONBLOCK` on Linux for x86_64 and aarch64 (the `libc` crate's
/// constant, 0o4000; `crates/server/src/source.rs` cites it).
const O_NONBLOCK: i32 = 0o4000;

/// Frames written to the FIFO in one write: 4096 bytes, which a pipe takes
/// whole or not at all (`PIPE_BUF`).
const CHUNK_FRAMES: u64 = 512;

/// A number made from the URI (FNV-1a), below 4096: only to give two URIs
/// different signals.
fn seed(uri: &str) -> u64 {
    let hash = uri.bytes().fold(0x811C_9DC5u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    });
    u64::from(hash % 4096)
}

/// The frame the fake plays at `frame` (counted from the start of playing
/// `uri`). With `n = frame + seed(uri)`: left is a ramp, `n mod 8192`, and
/// right counts the ramps, `(n / 8192) mod 8192`, each scaled into
/// -0.5..0.5. Every value is a multiple of 1/8192, exact in a float32, so a
/// capture can be compared sample for sample, and a frame names its own
/// index for the first 8192 * 8192 frames (25 minutes): [`frame_of`].
pub fn expected_frame(uri: &str, frame: u64) -> (f32, f32) {
    let n = frame + seed(uri);
    // 0..8191 is exact in an f32.
    let scale = |step: u64| step as f32 / 8192.0 - 0.5;
    (scale(n % 8192), scale((n / 8192) % 8192))
}

/// The index of a frame of `uri`'s signal, if the two samples are one.
pub fn frame_of(uri: &str, left: f32, right: f32) -> Option<u64> {
    let step = |sample: f32| {
        let scaled = (sample + 0.5) * 8192.0;
        // A whole number from 0 to 8191 by the check, so the conversion is exact.
        ((0.0..8192.0).contains(&scaled) && scaled.fract() == 0.0).then_some(scaled as u64)
    };
    (step(right)? * 8192 + step(left)?).checked_sub(seed(uri))
}

/// The bytes the FIFO carries for `frames` frames of `uri` from `first`.
pub fn expected_pcm(uri: &str, first: u64, frames: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(frames * PCM_FRAME_BYTES);
    for frame in first..first + frames as u64 {
        let (left, right) = expected_frame(uri, frame);
        out.extend_from_slice(&encode_frame(left, right));
    }
    out
}

struct Settings(BTreeMap<String, String>);

impl Settings {
    fn read() -> Settings {
        let mut map: BTreeMap<String, String> = std::env::vars()
            .filter(|(k, _)| k.starts_with("FAKE_SOLOIST_"))
            .collect();
        if let Some(text) = map
            .get("FAKE_SOLOIST_CONF")
            .and_then(|path| fs::read_to_string(path).ok())
        {
            for line in text.lines() {
                if let Some((key, value)) = line.split_once('=') {
                    map.insert(key.trim().to_string(), value.trim().to_string());
                }
            }
        }
        Settings(map)
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.0
            .get(key)
            .map(String::as_str)
            .filter(|v| !v.is_empty())
    }

    fn on(&self, key: &str) -> bool {
        self.get(key) == Some("1")
    }
}

#[derive(Default)]
struct Options {
    device_name: Option<String>,
    api_key: Option<String>,
    data_dir: Option<PathBuf>,
    pipewire_device: Option<String>,
    initial_volume: Option<u8>,
    ws: Option<String>,
}

enum Asked {
    Run(Options),
    Version,
    Help,
}

fn parse(arguments: &[String]) -> Result<Asked, String> {
    let mut options = Options::default();
    let mut at = 0;
    while at < arguments.len() {
        let flag = arguments[at].as_str();
        match flag {
            "-V" | "--version" => return Ok(Asked::Version),
            "-h" | "--help" => return Ok(Asked::Help),
            "-v" | "--verbose" => {
                at += 1;
                continue;
            }
            "-p" | "--pair" => return Err("pair mode is not faked".to_string()),
            _ => {}
        }
        let value = arguments
            .get(at + 1)
            .ok_or_else(|| format!("option {flag} needs a value"))?
            .clone();
        match flag {
            "-n" | "--device-name" => options.device_name = Some(value),
            "-k" | "--api-key" => options.api_key = Some(value),
            "-D" | "--data-dir" => options.data_dir = Some(PathBuf::from(value)),
            "-C" | "--cache-dir" => {}
            "-z" | "--cache-size" => match value.parse::<u64>() {
                Ok(size) if size == 0 || size >= 100 => {}
                _ => return Err("cache size must be 0 or at least 100".to_string()),
            },
            "-d" | "--pipewire-device" => options.pipewire_device = Some(value),
            "-i" | "--initial-volume" => match value.parse::<u8>() {
                Ok(volume) if volume <= 100 => options.initial_volume = Some(volume),
                _ => return Err("initial volume must be 0 to 100".to_string()),
            },
            "-w" | "--ws" => options.ws = Some(value),
            "-s" | "--single-track" => return Err("single-track mode is not faked".to_string()),
            other => return Err(format!("unknown option {other}")),
        }
        at += 2;
    }
    Ok(Asked::Run(options))
}

struct Player {
    device_name: String,
    logged_in: bool,
    is_active: bool,
    status: &'static str,
    uri: String,
    volume: u8,
    shuffle: bool,
    /// When the present stretch of playing began, and the frame it began at.
    playing_since: Option<(Instant, u64)>,
    /// The next frame to write.
    frame: u64,
    clients: Vec<(u64, TcpStream)>,
    next_client: u64,
    session_file: PathBuf,
    /// `FAKE_SOLOIST_COMMAND_LOG`: where every accepted control command is
    /// written, one a line, in the order it was accepted.
    command_log: Option<PathBuf>,
    /// The test control `stall`: a `play` is accepted and nothing plays.
    stalled: bool,
}

type Shared = Arc<Mutex<Player>>;

fn lock(shared: &Shared) -> MutexGuard<'_, Player> {
    shared.lock().unwrap_or_else(|e| e.into_inner())
}

fn obj(members: Vec<(&str, Value)>) -> Value {
    Value::Obj(
        members
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

fn entity(uri: &str) -> Value {
    let kind = uri.split(':').nth(1).unwrap_or("unknown");
    obj(vec![
        ("uri", Value::text(uri)),
        (
            "entity_type",
            Value::text(if uri.is_empty() { "" } else { kind }),
        ),
        (
            "decorations",
            if uri.is_empty() {
                obj(vec![])
            } else {
                obj(vec![
                    (
                        "identity",
                        obj(vec![("name", Value::text(&format!("Fake {uri}")))]),
                    ),
                    // The documented decorations a now-playing record is
                    // made of, each named after the URI so a test can tell
                    // one item's from another's: two covers (the `large`
                    // one second, so a reader that takes the first is
                    // caught), a parent and two creators.
                    (
                        "visual_identity",
                        obj(vec![(
                            "cover",
                            Value::Arr(
                                ["small", "large"]
                                    .iter()
                                    .map(|size| {
                                        obj(vec![
                                            (
                                                "url",
                                                Value::text(&format!(
                                                    "https://covers.example/{size}/{uri}"
                                                )),
                                            ),
                                            ("size", Value::text(size)),
                                        ])
                                    })
                                    .collect(),
                            ),
                        )]),
                    ),
                    (
                        "parent",
                        obj(vec![(
                            "entity",
                            obj(vec![
                                ("entity_type", Value::text("album")),
                                (
                                    "decorations",
                                    obj(vec![(
                                        "identity",
                                        obj(vec![(
                                            "name",
                                            Value::text(&format!("Fake album of {uri}")),
                                        )]),
                                    )]),
                                ),
                            ]),
                        )]),
                    ),
                    (
                        "creators",
                        Value::Arr(
                            ["Fake artist", "Fake guest"]
                                .iter()
                                .map(|name| {
                                    obj(vec![(
                                        "entity",
                                        obj(vec![
                                            ("entity_type", Value::text("artist")),
                                            (
                                                "decorations",
                                                obj(vec![(
                                                    "identity",
                                                    obj(vec![("name", Value::text(name))]),
                                                )]),
                                            ),
                                        ]),
                                    )])
                                })
                                .collect(),
                        ),
                    ),
                    (
                        "playback",
                        obj(vec![
                            ("duration_ms", Value::int(180_000)),
                            ("content_ratings", Value::Arr(vec![])),
                        ]),
                    ),
                ])
            },
        ),
    ])
}

impl Player {
    /// One accepted control command into the command log: its name, then
    /// its `uri` or `volume` when it has one.
    fn log_command(&self, text: &str) {
        let (Some(path), Ok(value)) = (&self.command_log, json::parse(text)) else {
            return;
        };
        let mut line = value
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if let Some(uri) = value.get("uri").and_then(Value::as_str) {
            line.push(' ');
            line.push_str(uri);
        }
        if let Some(volume) = value.get("volume").and_then(Value::as_num) {
            line.push(' ');
            line.push_str(volume);
        }
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "{line}");
        }
    }

    fn position(&self) -> Value {
        let position_ms = self.frame * 1000 / u64::from(PCM_RATE);
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        obj(vec![
            ("position_ms", Value::Num(position_ms.to_string())),
            ("timestamp_ms", Value::Num(now_ms.to_string())),
            (
                "speed",
                Value::Num(
                    if self.status == "playing" {
                        "1.0"
                    } else {
                        "0.0"
                    }
                    .to_string(),
                ),
            ),
        ])
    }

    fn options(&self) -> Value {
        obj(vec![
            ("shuffle", Value::Bool(self.shuffle)),
            ("repeat", Value::text("off")),
            ("playback_speed", Value::Num("1.0".to_string())),
            ("modes", obj(vec![])),
        ])
    }

    fn auth_state(&self) -> Value {
        obj(vec![
            ("type", Value::text("auth_state")),
            ("logged_in", Value::Bool(self.logged_in)),
            ("is_active", Value::Bool(self.is_active)),
            ("device_name", Value::text(&self.device_name)),
        ])
    }

    fn playback_state(&self) -> Value {
        let action = if self.status == "playing" {
            "pause"
        } else {
            "play"
        };
        obj(vec![
            ("type", Value::text("playback_state")),
            ("status", Value::text(self.status)),
            ("item", entity(&self.uri)),
            ("context", entity("")),
            ("position", self.position()),
            ("volume", Value::int(i64::from(self.volume))),
            ("is_active", Value::Bool(self.is_active)),
            ("options", self.options()),
            (
                "available_actions",
                obj(vec![(action, obj(vec![])), ("seek", obj(vec![]))]),
            ),
        ])
    }

    fn broadcast(&mut self, event: &Value) {
        let frame = ws::encode_frame(true, Opcode::Text, json::write(event).as_bytes(), None);
        self.clients
            .retain_mut(|(_, stream)| stream.write_all(&frame).is_ok());
    }

    fn set_status(&mut self, status: &'static str) {
        self.status = status;
        self.playing_since = (status == "playing").then(|| (Instant::now(), self.frame));
        self.broadcast(&obj(vec![
            ("type", Value::text("playback_changed")),
            ("status", Value::text(status)),
        ]));
    }

    fn set_active(&mut self, active: bool) {
        if self.is_active != active {
            self.is_active = active;
            self.broadcast(&obj(vec![
                ("type", Value::text("device_changed")),
                ("is_active", Value::Bool(active)),
                ("device_name", Value::text(&self.device_name)),
            ]));
        }
    }

    fn play(&mut self, uri: Option<&str>) {
        if let Some(uri) = uri {
            self.uri = uri.to_string();
            self.frame = 0;
            self.broadcast(&obj(vec![
                ("type", Value::text("track_changed")),
                ("item", entity(uri)),
            ]));
        }
        self.set_status("playing");
    }

    fn set_volume(&mut self, volume: u8) {
        self.volume = volume.min(100);
        self.broadcast(&obj(vec![
            ("type", Value::text("volume_changed")),
            ("volume", Value::int(i64::from(self.volume))),
        ]));
    }

    fn seek(&mut self, frame: u64) {
        self.frame = frame;
        if self.playing_since.is_some() {
            self.playing_since = Some((Instant::now(), frame));
        }
        let event = obj(vec![
            ("type", Value::text("position_sync")),
            ("position", self.position()),
        ]);
        self.broadcast(&event);
    }

    fn set_login(&mut self, logged_in: bool) {
        self.logged_in = logged_in;
        if logged_in {
            let _ = fs::write(&self.session_file, "fake session\n");
        } else {
            let _ = fs::remove_file(&self.session_file);
            self.status = "idle";
            self.playing_since = None;
            self.is_active = false;
        }
        let event = self.auth_state();
        self.broadcast(&event);
    }
}

fn error(message: &str) -> Value {
    obj(vec![
        ("type", Value::text("error")),
        ("message", Value::text(message)),
    ])
}

fn whole(value: &Value, key: &str, max: u64) -> Result<u64, String> {
    value
        .get(key)
        .and_then(Value::as_num)
        .and_then(|n| n.parse::<u64>().ok())
        .filter(|n| *n <= max)
        .ok_or_else(|| format!("invalid {key}"))
}

fn enabled(value: &Value) -> Result<bool, String> {
    value
        .get("enabled")
        .and_then(Value::as_bool)
        .ok_or_else(|| "invalid enabled".to_string())
}

/// What an accepted control command does, once its result has been sent.
enum Effect {
    Play(Option<String>),
    Pause,
    Skip,
    Seek(u64),
    Volume(u8),
    Shuffle(bool),
    Repeat,
    Queue,
    Active(bool),
}

/// One client message: the answer for the sender alone, and the effect to
/// apply after it. `Err` is the text of an `error` for the sender alone.
fn command(player: &Player, text: &str) -> Result<(Value, Option<Effect>), String> {
    let malformed = || "malformed message".to_string();
    let value = json::parse(text).map_err(|_| malformed())?;
    if value.get("type").and_then(Value::as_str) != Some("command") {
        return Err(malformed());
    }
    let name = value
        .get("command")
        .and_then(Value::as_str)
        .ok_or_else(malformed)?;
    if name == "get_auth_state" {
        return Ok((player.auth_state(), None));
    }
    let known = [
        "get_state",
        "get_queue",
        "play",
        "pause",
        "skip_next",
        "skip_prev",
        "seek",
        "set_volume",
        "set_shuffle",
        "set_repeat_context",
        "set_repeat_track",
        "add_to_queue",
        "activate",
        "deactivate",
    ];
    if !known.contains(&name) {
        return Err(format!("unknown command {name}"));
    }
    if !player.logged_in {
        return Err("command requires authentication".to_string());
    }
    match name {
        "get_state" => return Ok((player.playback_state(), None)),
        "get_queue" => {
            if value.get("limit").is_some() {
                whole(&value, "limit", u64::MAX)?;
            }
            return Ok((queue_changed(), None));
        }
        _ => {}
    }
    // Validated before anything happens: a refused command has no effect
    // and no result.
    let uri = match value.get("uri") {
        None => None,
        Some(Value::Str(uri)) if uri.starts_with("spotify:") => Some(uri.clone()),
        Some(_) => return Err("invalid uri".to_string()),
    };
    let effect = match name {
        "play" => Effect::Play(uri),
        "pause" => Effect::Pause,
        "skip_next" | "skip_prev" => Effect::Skip,
        "seek" => Effect::Seek(whole(&value, "position_ms", u64::MAX / 100_000)?),
        // At most 100 by the bound, so it fits.
        "set_volume" => Effect::Volume(whole(&value, "volume", 100)? as u8),
        "set_shuffle" => Effect::Shuffle(enabled(&value)?),
        "set_repeat_context" | "set_repeat_track" => {
            enabled(&value)?;
            Effect::Repeat
        }
        "add_to_queue" => match &uri {
            Some(uri) if uri.starts_with("spotify:track:") => Effect::Queue,
            _ => return Err("invalid uri".to_string()),
        },
        "activate" => Effect::Active(true),
        _ => Effect::Active(false),
    };
    // "command_result means the command was accepted and dispatched."
    let result = obj(vec![
        ("type", Value::text("command_result")),
        ("command", Value::text(name)),
    ]);
    Ok((result, Some(effect)))
}

fn queue_changed() -> Value {
    obj(vec![
        ("type", Value::text("queue_changed")),
        ("previous", Value::Arr(vec![])),
        ("upcoming", Value::Arr(vec![])),
    ])
}

fn apply(player: &mut Player, effect: Effect) {
    match effect {
        // "command_result means the command was accepted and dispatched. It
        // does not guarantee that playback state has already changed": a
        // stalled fake accepts the play and never plays.
        Effect::Play(_) if player.stalled => {}
        Effect::Play(uri) => player.play(uri.as_deref()),
        Effect::Pause => player.set_status("paused"),
        Effect::Skip => player.seek(0),
        Effect::Seek(position_ms) => player.seek(position_ms * u64::from(PCM_RATE) / 1000),
        Effect::Volume(volume) => player.set_volume(volume),
        Effect::Shuffle(on) => {
            player.shuffle = on;
            let event = obj(vec![
                ("type", Value::text("options_changed")),
                ("options", player.options()),
            ]);
            player.broadcast(&event);
        }
        Effect::Repeat => {
            let event = obj(vec![
                ("type", Value::text("options_changed")),
                ("options", player.options()),
            ]);
            player.broadcast(&event);
        }
        Effect::Queue => player.broadcast(&queue_changed()),
        Effect::Active(active) => {
            if !active && player.status == "playing" {
                player.set_status("paused");
            }
            player.set_active(active);
        }
    }
}

fn text_frame(value: &Value) -> Vec<u8> {
    ws::encode_frame(true, Opcode::Text, json::write(value).as_bytes(), None)
}

/// One WebSocket connection: the handshake, the greeting, then commands.
fn serve_client(mut stream: TcpStream, shared: &Shared) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut head = Vec::new();
    let mut chunk = [0u8; 2048];
    let end = loop {
        if let Some(end) = ws::head_end(&head) {
            break end;
        }
        match stream.read(&mut chunk) {
            Ok(n) if n > 0 && head.len() < ws::MAX_HANDSHAKE => head.extend_from_slice(&chunk[..n]),
            _ => return,
        }
    };
    let Ok(key) = ws::read_handshake_request(&head[..end]) else {
        let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n");
        return;
    };
    if stream
        .write_all(ws::handshake_response(&key).as_bytes())
        .is_err()
    {
        return;
    }
    let _ = stream.set_read_timeout(None);
    let id = {
        let mut player = lock(shared);
        let Ok(clone) = stream.try_clone() else {
            return;
        };
        let id = player.next_client;
        player.next_client += 1;
        // "When the server accepts a connection, Spotify Soloist sends
        // auth_state. If a user is already logged in, it also sends
        // playback_state."
        let mut greeting = text_frame(&player.auth_state());
        if player.logged_in {
            greeting.extend(text_frame(&player.playback_state()));
        }
        if stream.write_all(&greeting).is_err() {
            return;
        }
        player.clients.push((id, clone));
        id
    };
    let mut decoder = Decoder::new(Role::Server, ws::DEFAULT_MAX_MESSAGE);
    decoder.feed(&head[end..]);
    'connection: loop {
        loop {
            let message = match decoder.next_message() {
                Ok(Some(message)) => message,
                Ok(None) => break,
                Err(_) => break 'connection,
            };
            let mut player = lock(shared);
            match message {
                Message::Text(text) => match command(&player, &text) {
                    Ok((answer, effect)) => {
                        if stream.write_all(&text_frame(&answer)).is_err() {
                            break 'connection;
                        }
                        if let Some(effect) = effect {
                            player.log_command(&text);
                            apply(&mut player, effect);
                        }
                    }
                    // "The connection stays open after an error."
                    Err(why) => {
                        if stream.write_all(&text_frame(&error(&why))).is_err() {
                            break 'connection;
                        }
                    }
                },
                Message::Binary(_) => {
                    let _ = stream.write_all(&text_frame(&error("malformed message")));
                }
                Message::Ping(data) => {
                    let _ = stream.write_all(&ws::encode_frame(true, Opcode::Pong, &data, None));
                }
                Message::Pong(_) => {}
                Message::Close { .. } => {
                    let _ = stream.write_all(&ws::encode_frame(
                        true,
                        Opcode::Close,
                        &[0x03, 0xE8],
                        None,
                    ));
                    break 'connection;
                }
            }
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => decoder.feed(&chunk[..n]),
        }
    }
    lock(shared).clients.retain(|(client, _)| *client != id);
    let _ = stream.shutdown(std::net::Shutdown::Both);
}

/// One line from "the Spotify app".
fn control(shared: &Shared, line: &str) -> Result<(), String> {
    let mut words = line.split_whitespace();
    let verb = words.next().unwrap_or("");
    let argument = words.next();
    let mut player = lock(shared);
    let needs_login = |player: &Player| {
        if player.logged_in {
            Ok(())
        } else {
            Err("not logged in".to_string())
        }
    };
    match verb {
        "login" => player.set_login(true),
        "logout" => player.set_login(false),
        "play" => {
            needs_login(&player)?;
            let uri = argument.ok_or("play needs a URI")?;
            // Selecting the device in the app makes it the active one.
            player.set_active(true);
            player.play(Some(uri));
        }
        "resume" => {
            needs_login(&player)?;
            player.play(None);
        }
        "pause" => {
            needs_login(&player)?;
            player.set_status("paused");
        }
        "volume" => {
            needs_login(&player)?;
            let volume = argument
                .and_then(|v| v.parse::<u8>().ok())
                .ok_or("volume needs 0 to 100")?;
            player.set_volume(volume);
        }
        "stall" => player.stalled = true,
        "drop-ws" => {
            for (_, stream) in player.clients.drain(..) {
                let _ = stream.shutdown(std::net::Shutdown::Both);
            }
        }
        "exit" => {
            let code = argument.and_then(|c| c.parse().ok()).unwrap_or(1);
            // A crash: nothing is cleaned up.
            std::process::exit(code);
        }
        other => return Err(format!("unknown control {other:?}")),
    }
    Ok(())
}

fn serve_control(listener: UnixListener, shared: Shared) {
    for stream in listener.incoming().flatten() {
        let shared = Arc::clone(&shared);
        thread::spawn(move || {
            let Ok(mut writer) = stream.try_clone() else {
                return;
            };
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                let answer = match control(&shared, &line) {
                    Ok(()) => "ok\n".to_string(),
                    Err(why) => format!("error: {why}\n"),
                };
                if writer.write_all(answer.as_bytes()).is_err() {
                    break;
                }
            }
        });
    }
}

/// The FIFO standing in for the pipe-tunnel sink of `device`.
fn open_pipe(settings: &Settings, device: Option<&str>) -> Option<File> {
    let dir = settings.get("FAKE_SOLOIST_PIPE_DIR")?;
    let device = device?;
    let name = format!("{}.pcm", device.strip_prefix("chorus-").unwrap_or(device));
    // Read-write and non-blocking, as the sink opens it: the open never
    // waits for a reader and a write never blocks.
    OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(O_NONBLOCK)
        .open(Path::new(dir).join(name))
        .ok()
}

/// Write the frames that are due, in real time by the monotonic clock. A
/// chunk the pipe has no room for is dropped, as the sink drops it.
fn play_due(shared: &Shared, pipe: &mut File) {
    let mut player = lock(shared);
    let Some((since, start)) = player.playing_since else {
        return;
    };
    let elapsed = since.elapsed().as_micros() as u64;
    let due = start + elapsed * u64::from(PCM_RATE) / 1_000_000;
    while player.frame + CHUNK_FRAMES <= due {
        let bytes = expected_pcm(&player.uri, player.frame, CHUNK_FRAMES as usize);
        let _ = pipe.write(&bytes);
        player.frame += CHUNK_FRAMES;
    }
}

fn fail(why: &str) -> i32 {
    eprintln!("fake-soloist: error: {why}");
    1
}

/// Run the fake with the command line's arguments (the program name left
/// out); returns the exit code.
pub fn run(arguments: Vec<String>) -> i32 {
    let settings = Settings::read();
    let options = match parse(&arguments) {
        Ok(Asked::Run(options)) => options,
        Ok(Asked::Version) => {
            // `\n` in the setting is a line break: the conf file is one
            // setting a line.
            let version = settings
                .get("FAKE_SOLOIST_VERSION")
                .unwrap_or(DEFAULT_VERSION);
            println!("{}", version.replace("\\n", "\n"));
            return 0;
        }
        Ok(Asked::Help) => {
            println!("usage: soloist -n NAME -k KEY [options] (a fake for tests)");
            return 0;
        }
        Err(why) => return fail(&why),
    };
    if let Some(path) = settings.get("FAKE_SOLOIST_ARGV_LOG") {
        let line = json::write(&Value::Arr(
            arguments.iter().map(|a| Value::text(a)).collect(),
        ));
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "{line}");
        }
    }
    let (Some(device_name), Some(api_key)) = (options.device_name, options.api_key) else {
        return fail("missing required options: --device-name and --api-key");
    };
    // On purpose: the supervisor must take this out of its log.
    eprintln!("fake-soloist: starting, device_name={device_name}, api key {api_key}");
    if settings.on("FAKE_SOLOIST_EXPIRED") {
        eprintln!("fake-soloist: this build has expired");
        return 10;
    }
    if settings.on("FAKE_SOLOIST_FAIL") {
        return fail("startup failure");
    }
    let Some(data_dir) = options.data_dir else {
        return fail("the fake needs --data-dir");
    };
    if fs::create_dir_all(&data_dir).is_err() {
        return fail("unwritable data directory");
    }
    let pid_path = data_dir.join(PID_FILE);
    let Ok(mut pid_file) = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&pid_path)
    else {
        return fail("unwritable data directory");
    };
    if pid_file.try_lock().is_err() {
        return fail("another Spotify Soloist process is already using the data directory");
    }
    let _ = pid_file.set_len(0);
    let _ = writeln!(pid_file, "{}", std::process::id());

    let session_file = data_dir.join("fake-session");
    let shared: Shared = Arc::new(Mutex::new(Player {
        device_name: device_name.clone(),
        logged_in: session_file.exists(),
        is_active: false,
        status: "idle",
        uri: String::new(),
        volume: options.initial_volume.unwrap_or(50),
        shuffle: false,
        playing_since: None,
        frame: 0,
        clients: Vec::new(),
        next_client: 0,
        session_file,
        command_log: settings.get("FAKE_SOLOIST_COMMAND_LOG").map(PathBuf::from),
        stalled: false,
    }));
    if let Err(e) = sys::catch_termination() {
        return fail(&format!("signals: {e}"));
    }

    let mut ws_files = Vec::new();
    match (&options.ws, settings.on("FAKE_SOLOIST_NO_WS")) {
        (Some(_), true) => {
            eprintln!(
                "fake-soloist: warning: the WebSocket API could not bind; starting without it"
            );
        }
        (Some(bind), false) => {
            let listener = match bind.parse::<SocketAddr>().map(TcpListener::bind) {
                Ok(Ok(listener)) => listener,
                _ => return fail("invalid --ws address"),
            };
            let Ok(local) = listener.local_addr() else {
                return fail("startup failure");
            };
            for (name, value) in [
                (WS_ADDR_FILE, local.ip().to_string()),
                (WS_PORT_FILE, local.port().to_string()),
            ] {
                // Written whole and then renamed, so a reader never sees
                // half a port number.
                let path = data_dir.join(name);
                let temporary = data_dir.join(format!("{name}.tmp"));
                if fs::write(&temporary, format!("{value}\n")).is_err()
                    || fs::rename(&temporary, &path).is_err()
                {
                    return fail("unwritable data directory");
                }
                ws_files.push(path);
            }
            let for_clients = Arc::clone(&shared);
            thread::spawn(move || {
                for stream in listener.incoming().flatten() {
                    let shared = Arc::clone(&for_clients);
                    thread::spawn(move || serve_client(stream, &shared));
                }
            });
        }
        (None, _) => {}
    }
    if let Some(path) = settings.get("FAKE_SOLOIST_CONTROL") {
        let _ = fs::remove_file(path);
        match UnixListener::bind(path) {
            Ok(listener) => {
                let for_control = Arc::clone(&shared);
                thread::spawn(move || serve_control(listener, for_control));
            }
            Err(e) => return fail(&format!("the control socket {path}: {e}")),
        }
    }
    let mut pipe = open_pipe(&settings, options.pipewire_device.as_deref());
    eprintln!("fake-soloist: ready, device_name={device_name}");

    while !sys::termination_requested() {
        if let Some(pipe) = &mut pipe {
            play_due(&shared, pipe);
        }
        thread::sleep(Duration::from_millis(2));
    }
    // "On shutdown, Spotify Soloist removes ws.addr and ws.port."
    for path in ws_files {
        let _ = fs::remove_file(path);
    }
    let _ = fs::remove_file(&pid_path);
    if let Some(path) = settings.get("FAKE_SOLOIST_CONTROL") {
        let _ = fs::remove_file(path);
    }
    eprintln!("fake-soloist: normal shutdown");
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_signal_names_its_uri_and_frame() {
        let a = "spotify:track:aaaa";
        let b = "spotify:track:bbbb";
        assert_ne!(expected_frame(a, 0), expected_frame(b, 0));
        let (left, right) = expected_frame(a, 5);
        assert!((-0.5..0.5).contains(&left) && (-0.5..0.5).contains(&right));
        // A ramp: one step of 1/8192 a frame, wrapping every 8192 frames.
        let step = expected_frame(a, 6).0 - left;
        assert!(step == 1.0 / 8192.0 || step == 1.0 / 8192.0 - 1.0);
        // A frame names its index, far past one ramp.
        for frame in [0u64, 1, 511, 512, 8191, 8192, 8193, 100_000, 44_100 * 600] {
            let (left, right) = expected_frame(a, frame);
            assert_eq!(frame_of(a, left, right), Some(frame), "{frame}");
        }
        assert_eq!(frame_of(a, 0.3, 0.0), None);
        assert_eq!(frame_of(a, 1.0, 0.0), None);
        let bytes = expected_pcm(a, 5, 3);
        assert_eq!(bytes.len(), 3 * PCM_FRAME_BYTES);
        assert_eq!(bytes[..4], left.to_le_bytes());
        assert_eq!(bytes[4..8], right.to_le_bytes());
    }

    fn player(logged_in: bool) -> Player {
        Player {
            device_name: "Kitchen".into(),
            logged_in,
            is_active: false,
            status: "idle",
            uri: String::new(),
            volume: 100,
            shuffle: false,
            playing_since: None,
            frame: 0,
            clients: Vec::new(),
            next_client: 0,
            session_file: PathBuf::from("/nonexistent/fake-session"),
            command_log: None,
            stalled: false,
        }
    }

    /// The fake's events read with the model the server will use.
    #[test]
    fn its_events_parse_with_the_api_model() {
        use chorus_soloist::api::{self, Event, Status};
        let mut p = player(true);
        p.uri = "spotify:track:abc".into();
        p.status = "playing";
        let text = json::write(&p.playback_state());
        let Event::PlaybackState(state) = api::parse_event(&text).unwrap() else {
            panic!("{text}");
        };
        assert_eq!(state.status, Some(Status::Playing));
        let item = state.item.unwrap();
        assert_eq!(item.uri, "spotify:track:abc");
        assert_eq!(item.entity_type, "track");
        assert_eq!(item.duration_ms, Some(180_000));
        assert_eq!(state.volume, Some(100));
        assert_eq!(
            api::parse_event(&json::write(&p.auth_state())).unwrap(),
            Event::AuthState {
                logged_in: true,
                is_active: Some(false),
                device_name: Some("Kitchen".into())
            }
        );
    }

    #[test]
    fn commands_are_refused_as_documented() {
        let out = player(false);
        // Only get_auth_state works without a login.
        assert!(command(&out, r#"{"type":"command","command":"get_auth_state"}"#).is_ok());
        for name in ["get_state", "get_queue", "play", "pause", "activate"] {
            assert_eq!(
                command(&out, &format!(r#"{{"type":"command","command":"{name}"}}"#)).err(),
                Some("command requires authentication".to_string()),
                "{name}"
            );
        }
        let p = player(true);
        for bad in [
            "not json",
            r#"{"type":"event","command":"pause"}"#,
            r#"{"type":"command"}"#,
            r#"{"type":"command","command":"dance"}"#,
            r#"{"type":"command","command":"seek"}"#,
            r#"{"type":"command","command":"set_volume","volume":101}"#,
            r#"{"type":"command","command":"set_shuffle","enabled":"yes"}"#,
            r#"{"type":"command","command":"play","uri":"https://example.invalid/x"}"#,
            r#"{"type":"command","command":"add_to_queue","uri":"spotify:album:x"}"#,
            r#"{"type":"command","command":"add_to_queue"}"#,
        ] {
            assert!(command(&p, bad).is_err(), "{bad}");
        }
        // Every documented example is accepted (the fixtures are the
        // reference page's own).
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/soloist");
        let mut seen = 0;
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if name.starts_with("command-") {
                let text = fs::read_to_string(&path).unwrap();
                assert!(command(&p, &text).is_ok(), "{name}");
                seen += 1;
            }
        }
        assert!(seen >= 8);
    }

    #[test]
    fn every_documented_option_is_accepted_in_both_forms() {
        let line = |text: &str| -> Vec<String> { text.split(' ').map(str::to_string).collect() };
        for text in [
            "-n Kitchen -k KEY -D /d -C /c -z 256 -d chorus-r0 -i 100 -w 127.0.0.1:0 -v",
            "--device-name Kitchen --api-key KEY --data-dir /d --cache-dir /c --cache-size 0 \
             --pipewire-device chorus-r0 --initial-volume 0 --ws 127.0.0.1:0 --verbose",
        ] {
            let Ok(Asked::Run(options)) = parse(&line(text)) else {
                panic!("{text}");
            };
            assert_eq!(options.device_name.as_deref(), Some("Kitchen"));
            assert_eq!(options.api_key.as_deref(), Some("KEY"));
            assert_eq!(options.pipewire_device.as_deref(), Some("chorus-r0"));
            assert_eq!(options.ws.as_deref(), Some("127.0.0.1:0"));
        }
        assert!(matches!(parse(&line("-V")), Ok(Asked::Version)));
        assert!(matches!(parse(&line("--help")), Ok(Asked::Help)));
        for bad in [
            "-z 50",
            "-i 101",
            "--frobnicate 1",
            "-n",
            "-p",
            "-s spotify:track:x",
        ] {
            assert!(parse(&line(bad)).is_err(), "{bad}");
        }
    }
}
