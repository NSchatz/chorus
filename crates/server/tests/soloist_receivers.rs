//! Spotify Soloist receivers, end to end, with no Soloist (goal 17, lines A
//! and B; `docs/soloist.md`).
//!
//! What runs here is real except Soloist itself: the real `chorus-server`
//! binary, one real receiver supervisor per receiver (`chorus_soloistd::run`,
//! the whole of `chorus-soloistd`, as this crate's example
//! `server-test-soloistd`, with `--pipewire none`), and under each supervisor
//! the fake Soloist (`crates/soloist-fake`, as the example
//! `server-test-fake-soloist`), which writes the receiver's FIFO itself in
//! place of PipeWire's sink. The test plays the Spotify app through the
//! fake's control socket, and a room's speaker by connecting a player
//! session and reading its chunks. chorus never downloads or runs Soloist;
//! what Soloist does that its documentation does not state is in
//! `docs/soloist.md`'s table of assumptions.
//!
//! **The audio is compared exactly.** The servers here run at 44.1 kHz
//! stereo, the FIFO's own rate, so the reader's resampler passes samples
//! through and the fake's signal (every frame names its URI and its index,
//! in steps of 1/8192, exact in 16 bits) arrives bit for bit. The 48 kHz
//! path is the same reader with the media player's resampler in it, held to
//! that resampler's own output by `soloistreader`'s unit tests.
//!
//! **Waits are bounded and nothing depends on a sleep being long enough.**
//! Every "it happens" is awaited to a deadline ([`LIMIT`]); the fixed sleeps
//! are the ones that show something does NOT happen, each named where it
//! is. A frame the host was too busy to deliver in time is silence (the
//! port pads), never a wrong frame, and the assertions on audio allow
//! silence and a whole dropped write of the fake between two frames, and
//! nothing else.

mod common;

use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chorus_control::json::{self, Value};
use chorus_protocol::AudioChunk;
use chorus_soloist_fake::frame_of;
use common::line_in::Recorder;
use common::{fresh_id, http, RunningServer};

/// How long any one awaited thing may take. Generous: the gate's hosts are
/// shared, and a supervisor starts a process and a WebSocket per assignment.
const LIMIT: Duration = Duration::from_secs(30);

/// The configured stream's sample: odd, so it can never be a frame of the
/// fake's signal (every sample of which is a multiple of 4 as 16 bits).
const STREAM: i16 = 0x1235;

const RATE: u32 = 44_100;

static BENCHES: AtomicU32 = AtomicU32::new(0);

fn example(name: &str) -> PathBuf {
    // target/<profile>/deps/<this test> -> target/<profile>/examples/<name>
    let exe = std::env::current_exe().expect("the test's own path");
    let profile = exe
        .parent()
        .and_then(Path::parent)
        .expect("target/<profile>/deps");
    let path = profile.join("examples").join(name);
    assert!(
        path.is_file(),
        "{} is not built; `cargo test` (or `cargo test --examples`) builds it beside this test",
        path.display()
    );
    path
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

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

// ----- the receivers -------------------------------------------------------------

/// One scratch tree: the receiver directory and what the supervisors keep.
struct Bench {
    root: PathBuf,
}

/// A running supervisor. Dropped, it is stopped the normal way (SIGTERM),
/// so it stops its Soloist too.
struct Supervisor {
    child: Child,
}

impl Supervisor {
    fn stop(&mut self) {
        let _ = chorus_soloistd::sys::terminate(self.child.id());
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
                _ => break,
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Bench {
    fn new() -> Bench {
        // Short, because a Unix socket's path is at most 107 bytes.
        let root = std::env::temp_dir().join(format!(
            "css-{}-{}",
            std::process::id(),
            BENCHES.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("recv")).unwrap();
        fs::write(root.join("key"), "not-a-real-key-chorus-test\n").unwrap();
        let bytes: Vec<u8> = std::iter::repeat_n(STREAM.to_le_bytes(), RATE as usize * 2 * 5)
            .flatten()
            .collect();
        fs::write(root.join("stream.pcm"), bytes).unwrap();
        Bench { root }
    }

    fn dir(&self) -> PathBuf {
        self.root.join("recv")
    }

    fn conf(&self, index: usize) -> PathBuf {
        self.root.join(format!("fake{index}.conf"))
    }

    /// What receiver `index`'s fake does at its next start
    /// (`FAKE_SOLOIST_CONF`, read at every start).
    fn fake(&self, index: usize, settings: &[(&str, &str)]) {
        let text: String = settings.iter().map(|(k, v)| format!("{k}={v}\n")).collect();
        let temporary = self.root.join(format!("fake{index}.conf.tmp"));
        fs::write(&temporary, text).unwrap();
        fs::rename(temporary, self.conf(index)).unwrap();
    }

    /// The supervisor of receiver `index`, of `receivers`.
    fn supervisor(&self, index: usize, receivers: usize) -> Supervisor {
        if !self.conf(index).exists() {
            self.fake(index, &[]);
        }
        let child = Command::new(example("server-test-soloistd"))
            .arg("--soloist-dir")
            .arg(self.dir())
            .arg("--api-key-file")
            .arg(self.root.join("key"))
            .arg("--state-dir")
            .arg(self.root.join("state"))
            .arg("--cache-dir")
            .arg(self.root.join("cache"))
            .arg("--soloist-bin")
            .arg(example("server-test-fake-soloist"))
            .args(["--receivers", &receivers.to_string()])
            .args(["--receiver", &index.to_string()])
            .args(["--pipewire", "none"])
            .args(["--backoff-min-ms", "40", "--backoff-max-ms", "160"])
            .args(["--stop-timeout-ms", "3000"])
            .env("FAKE_SOLOIST_CONF", self.conf(index))
            .env(
                "FAKE_SOLOIST_ARGV_LOG",
                self.root.join(format!("argv{index}.log")),
            )
            .env(
                "FAKE_SOLOIST_COMMAND_LOG",
                self.root.join(format!("commands{index}.log")),
            )
            .env(
                "FAKE_SOLOIST_CONTROL",
                self.root.join(format!("app{index}.sock")),
            )
            .env("FAKE_SOLOIST_PIPE_DIR", self.dir())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("the supervisor starts");
        Supervisor { child }
    }

    fn supervisors(&self, receivers: usize) -> Vec<Supervisor> {
        (0..receivers)
            .map(|i| self.supervisor(i, receivers))
            .collect()
    }

    /// One line to receiver `index`'s fake as "the Spotify app": its answer.
    fn app_says(&self, index: usize, line: &str) -> Option<String> {
        let mut stream = UnixStream::connect(self.root.join(format!("app{index}.sock"))).ok()?;
        stream.set_read_timeout(Some(LIMIT)).ok()?;
        stream.write_all(format!("{line}\n").as_bytes()).ok()?;
        let mut answer = String::new();
        BufReader::new(stream).read_line(&mut answer).ok()?;
        Some(answer)
    }

    /// The Spotify app does `line` on receiver `index`. Waits for the fake
    /// to be there (it is started when the receiver is assigned).
    fn app(&self, index: usize, line: &str) {
        let mut last = None;
        wait_for(&format!("the fake of r{index} takes {line:?}"), || {
            last = self.app_says(index, line);
            last.as_deref() == Some("ok\n")
        });
    }

    /// Every WebSocket control command receiver `index`'s fake accepted, in
    /// order.
    fn commands(&self, index: usize) -> Vec<String> {
        fs::read_to_string(self.root.join(format!("commands{index}.log")))
            .map(|t| t.lines().map(str::to_string).collect())
            .unwrap_or_default()
    }

    /// Every start of receiver `index`'s fake, as its argument list.
    fn starts(&self, index: usize) -> Vec<Vec<String>> {
        let Ok(text) = fs::read_to_string(self.root.join(format!("argv{index}.log"))) else {
            return Vec::new();
        };
        text.lines()
            .map(|line| match json::parse(line).unwrap() {
                Value::Arr(items) => items
                    .iter()
                    .map(|v| v.as_str().unwrap().to_string())
                    .collect(),
                other => panic!("{other:?}"),
            })
            .collect()
    }

    /// The value of `flag` at the last start of receiver `index`'s fake.
    fn started_with(&self, index: usize, flag: &str) -> Option<String> {
        let starts = self.starts(index);
        let last = starts.last()?;
        let at = last.iter().position(|a| a == flag)?;
        last.get(at + 1).cloned()
    }

    /// The server: 44.1 kHz stereo, `zones`, `receivers` receivers, `extra`.
    fn server(&self, zones: &[&str], receivers: usize, extra: &[&str]) -> RunningServer {
        let dir = self.dir();
        let stream = self.root.join("stream.pcm");
        let tz = format!(
            "{}/../../fixtures/schedule/Etc_UTC.slim.tzif",
            env!("CARGO_MANIFEST_DIR")
        );
        let receivers = receivers.to_string();
        let mut args: Vec<String> = [
            "--source",
            stream.to_str().unwrap(),
            "--rate",
            "44100",
            // The TV path's latency plan has a floor that depends on the
            // chunk's frame count; at 44.1 kHz it is just above the
            // default 25 ms. No TV plays here.
            "--tv-latency-ms",
            "40",
            "--slots",
            "4",
            "--max-clients",
            "6",
            "--tz",
            &tz,
            "--soloist-dir",
            dir.to_str().unwrap(),
            "--soloist-receivers",
            &receivers,
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        for zone in zones {
            args.push("--zone".into());
            args.push(zone.to_string());
        }
        args.extend(extra.iter().map(|s| s.to_string()));
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        RunningServer::start(&refs)
    }
}

impl Drop for Bench {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

// ----- the state -------------------------------------------------------------------

fn parsed(text: &str) -> Value {
    json::parse(text).unwrap_or_else(|e| panic!("{text}: {e:?}"))
}

fn list<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    match value.get(key) {
        Some(Value::Arr(items)) => items,
        _ => &[],
    }
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// Receiver `id` as the state lists it: its state, target and name.
fn receiver(server: &RunningServer, id: &str) -> (String, String, String) {
    let state = parsed(&server.state());
    let soloist = state.get("soloist").cloned().unwrap_or(Value::Null);
    list(&soloist, "receivers")
        .iter()
        .find(|r| text(r, "id") == id)
        .map(|r| (text(r, "state"), text(r, "target"), text(r, "name")))
        .unwrap_or_default()
}

/// The state's `soloist` member.
fn soloist(server: &RunningServer) -> Value {
    parsed(&server.state())
        .get("soloist")
        .cloned()
        .unwrap_or(Value::Null)
}

/// The group a room is in and what that group plays.
fn playing_in(server: &RunningServer, room: &str) -> (String, String) {
    let state = parsed(&server.state());
    let group = list(&state, "zones")
        .iter()
        .find(|z| text(z, "id") == room)
        .map(|z| text(z, "group"))
        .unwrap_or_default();
    let source = list(&state, "groups")
        .iter()
        .find(|g| text(g, "id") == group)
        .map(|g| text(g, "source"))
        .unwrap_or_default();
    (group, source)
}

/// A formed group's now-playing record.
fn now_playing(server: &RunningServer, group: &str) -> Option<Value> {
    let state = parsed(&server.state());
    list(&state, "groups")
        .iter()
        .find(|g| text(g, "id") == group)
        .and_then(|g| g.get("now_playing").cloned())
}

fn volume_of(server: &RunningServer, room: &str) -> String {
    let state = parsed(&server.state());
    list(&state, "zones")
        .iter()
        .find(|z| text(z, "id") == room)
        .and_then(|z| z.get("volume").and_then(Value::as_num).map(str::to_string))
        .unwrap_or_default()
}

fn ringing(server: &RunningServer, alarm: &str) -> bool {
    list(&parsed(&server.state()), "alarms")
        .iter()
        .find(|a| text(a, "alarm") == alarm)
        .and_then(|a| a.get("ringing").and_then(Value::as_bool))
        .unwrap_or(false)
}

fn metrics(server: &RunningServer) -> String {
    http(
        &server.control,
        "GET /metrics HTTP/1.1\r\nHost: chorus\r\nConnection: close\r\n\r\n",
    )
    .1
}

fn metric(server: &RunningServer, name: &str) -> Option<i64> {
    metrics(server)
        .lines()
        .find(|l| l.starts_with(name) && !l.starts_with('#'))
        .and_then(|l| l.rsplit(' ').next()?.parse().ok())
}

fn ctl(server: &RunningServer, args: &[&str]) -> chorus_ctl::Outcome {
    let mut all = vec!["--server".to_string(), server.control.clone()];
    all.extend(args.iter().map(|a| a.to_string()));
    chorus_ctl::run_with(&all, None)
}

fn ctl_ok(server: &RunningServer, args: &[&str]) -> String {
    let outcome = ctl(server, args);
    assert_eq!(
        outcome.code, 0,
        "chorusctl {:?} exited {}: {}",
        args, outcome.code, outcome.stderr
    );
    outcome.stdout
}

/// Wait until every one of `ids` runs Soloist for its target.
fn running(server: &RunningServer, ids: &[&str]) {
    wait_for(&format!("{ids:?} are running"), || {
        ids.iter().all(|id| receiver(server, id).0 == "running")
    });
}

/// Attach a speaker to `room` and listen as it.
fn listen(server: &RunningServer, room: &str) -> Recorder {
    let endpoint = fresh_id(&format!("css-{room}"));
    server.applied(&format!(
        r#"{{"v":1,"t":"attach","zone":"{room}","endpoint":"{endpoint}"}}"#
    ));
    Recorder::player(&server.audio, &endpoint)
}

// ----- what a room hears -----------------------------------------------------------

type Frame = [i16; 2];

fn frames(chunk: &AudioChunk) -> Vec<Frame> {
    chunk
        .audio_data
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

/// The index of a frame of `uri`'s signal, if it is one.
fn index_in(uri: &str, frame: Frame) -> Option<u64> {
    frame_of(
        uri,
        f32::from(frame[0]) / 32_768.0,
        f32::from(frame[1]) / 32_768.0,
    )
}

/// What a frame is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Heard {
    Silence,
    Stream,
    /// A frame of the fake's signal: its position on the signal's own
    /// counter (the frame index plus a small offset the URI chooses).
    Signal(u64),
    Other(Frame),
}

/// A frame of the fake's signal carries a ramp in its left sample and the
/// count of ramps in its right, each a multiple of 4 as 16 bits
/// (`chorus_soloist_fake::expected_frame`); anything else is not one.
fn classify(frame: Frame) -> Heard {
    if frame == [0, 0] {
        return Heard::Silence;
    }
    if frame == [STREAM, STREAM] {
        return Heard::Stream;
    }
    let step = |sample: i16| (sample % 4 == 0).then(|| (i64::from(sample) / 4 + 4096) as u64);
    match (step(frame[0]), step(frame[1])) {
        (Some(left), Some(right)) => Heard::Signal(right * 8192 + left),
        _ => Heard::Other(frame),
    }
}

/// How often what `chunks` hold goes from one signal to another, and
/// whether it ever does so inside a chunk. Every URI's signal starts near
/// zero on the signal's counter when it is played and counts up, so within
/// one signal the counter only rises, and a fall is another signal
/// starting. Panics on a frame that is neither signal, silence nor the
/// configured stream.
fn switches(chunks: &[AudioChunk], what: &str) -> usize {
    let mut last: Option<u64> = None;
    let mut switches = 0;
    for chunk in chunks {
        for (at, frame) in frames(chunk).into_iter().enumerate() {
            let now = match classify(frame) {
                Heard::Signal(n) => n,
                Heard::Silence | Heard::Stream => continue,
                Heard::Other(frame) => {
                    panic!("{what}: chunk {} holds {:?}", chunk.sequence, frame)
                }
            };
            if last.is_some_and(|before| now < before) {
                switches += 1;
                let first_signal = frames(chunk)
                    .iter()
                    .position(|f| matches!(classify(*f), Heard::Signal(_)));
                assert_eq!(
                    first_signal,
                    Some(at),
                    "{what}: chunk {} mixes two signals",
                    chunk.sequence
                );
            }
            last = Some(now);
        }
    }
    switches
}

/// Every frame of `uri`'s signal a room heard from chunk `from` on: its
/// indices, in the order heard. Panics on a frame that is neither the
/// signal, silence nor the configured stream.
fn signal_heard(room: &Recorder, from: usize, uri: &str) -> Vec<u64> {
    let mut indices = Vec::new();
    for chunk in room.chunks().iter().skip(from) {
        for frame in frames(chunk) {
            match (classify(frame), index_in(uri, frame)) {
                (Heard::Signal(_), Some(index)) => indices.push(index),
                (Heard::Silence | Heard::Stream, _) => {}
                _ => panic!(
                    "chunk {} holds {:?}, which is not a frame of {}",
                    chunk.sequence, frame, uri
                ),
            }
        }
    }
    indices
}

/// The signal arrived sample for sample: every frame heard is a frame of
/// the URI's signal (that is `signal_heard`), in order, each the next index
/// after the one before. The one thing allowed between two frames is what
/// the fake itself does when its pipe is full, as PipeWire's sink does: a
/// whole write of 512 frames dropped. Returns how many frames were heard.
fn assert_sample_for_sample(indices: &[u64], what: &str) -> usize {
    assert!(
        !indices.is_empty(),
        "{what}: nothing of the signal was heard"
    );
    for pair in indices.windows(2) {
        let step = pair[1].wrapping_sub(pair[0]);
        assert!(
            step == 1 || (pair[1] > pair[0] && (step - 1) % 512 == 0),
            "{what}: frame {} was followed by frame {}",
            pair[0],
            pair[1]
        );
    }
    indices.len()
}

/// The frames of the track played last: what `signal_heard` gives after the
/// last place its index falls. (A track just played starts at frame 0; what
/// came before it in the same room is another play.)
fn latest_play(indices: &[u64]) -> &[u64] {
    let start = indices
        .windows(2)
        .rposition(|pair| pair[1] < pair[0])
        .map_or(0, |at| at + 1);
    &indices[start..]
}

/// Wait until `room` has heard `frames` frames of the play of `uri` that
/// started after chunk `from`, from its very first frame: Soloist was
/// restarted in between, so the track starts at frame 0. (The few frames of
/// the old play that were still on their way when Soloist died may come
/// first, as after any underrun; they are that play's, in order.)
fn hears_from_the_start(room: &Recorder, from: usize, uri: &str, frames: usize, what: &str) {
    let fresh = |all: &[u64]| {
        let play = latest_play(all);
        (play.first() == Some(&0) && play.len() >= frames).then(|| play.to_vec())
    };
    wait_for(what, || fresh(&signal_heard(room, from, uri)).is_some());
    let play = fresh(&signal_heard(room, from, uri)).unwrap();
    assert_sample_for_sample(&play, what);
}

/// Wait until `room` has heard `frames` frames of `uri` from chunk `from`.
fn hears(room: &Recorder, from: usize, uri: &str, frames: usize, what: &str) -> Vec<u64> {
    wait_for(what, || signal_heard(room, from, uri).len() >= frames);
    signal_heard(room, from, uri)
}

// ----- the tests -------------------------------------------------------------------

const TRACK_A: &str = "spotify:track:6rqhFgbbKwnb9MLmUQDhG6";
const TRACK_B: &str = "spotify:track:2JRo0gjbX4GrCqBYdRohoo";
const TRACK_C: &str = "spotify:track:4aawyAB9vmqN3uQ7FjRGTy";
const PLAYLIST: &str = "spotify:playlist:37i9dQZF1DXcBWIGoYBM5M";

/// Line A's assignment rules: every room and saved group has a receiver
/// under its own name; a live group gets one when it forms, under its
/// rooms' names joined, loses it after the grace period once it dissolves,
/// and gets the same Soloist data directory when it forms again; a renamed
/// room is assigned again under the new name; and a target the pool has no
/// receiver for is listed.
#[test]
fn receivers_follow_the_rooms_the_saved_groups_and_the_live_groups_under_their_names() {
    let bench = Bench::new();
    let server = bench.server(
        &["kitchen", "den", "study", "hall"],
        6,
        &["--soloist-grace", "1"],
    );
    let _supervisors = bench.supervisors(6);
    running(&server, &["r0", "r1", "r2", "r3"]);
    for (id, room) in [
        ("r0", "kitchen"),
        ("r1", "den"),
        ("r2", "study"),
        ("r3", "hall"),
    ] {
        assert_eq!(
            receiver(&server, id),
            ("running".into(), format!("room:{room}"), room.to_string())
        );
    }
    for (index, room) in ["kitchen", "den", "study", "hall"].iter().enumerate() {
        assert_eq!(
            bench.started_with(index, "--device-name").as_deref(),
            Some(*room),
            "the Spotify Connect device is called what the room is"
        );
    }
    assert_eq!(receiver(&server, "r4").0, "idle", "nothing to host yet");

    // A saved group: the next receiver, under the group's name.
    server.applied(
        r#"{"v":2,"t":"group_save","group":"downstairs","name":"Downstairs","zones":["kitchen","den"]}"#,
    );
    running(&server, &["r4"]);
    assert_eq!(
        receiver(&server, "r4"),
        (
            "running".into(),
            "group:downstairs".into(),
            "Downstairs".into()
        )
    );
    assert_eq!(
        bench.started_with(4, "--device-name").as_deref(),
        Some("Downstairs")
    );

    // A live group forms: a receiver under its rooms' names joined.
    server.applied(r#"{"v":2,"t":"join","zone":"study","target":"hall"}"#);
    running(&server, &["r5"]);
    assert_eq!(
        receiver(&server, "r5"),
        (
            "running".into(),
            "live:hall+study".into(),
            "hall + study".into()
        )
    );
    let data_dir = bench.started_with(5, "--data-dir").unwrap();
    assert!(data_dir.ends_with("/live-hall_study"), "{data_dir}");
    assert_eq!(bench.starts(5).len(), 1);

    // It dissolves: after the grace period (1 s here) the receiver is free.
    server.applied(r#"{"v":2,"t":"take","target":"study"}"#);
    wait_for("the dissolved group's receiver is released", || {
        receiver(&server, "r5") == ("idle".into(), String::new(), String::new())
    });

    // It forms again: a receiver again, and the same data directory, so the
    // same Spotify Connect identity and stored session.
    server.applied(r#"{"v":2,"t":"join","zone":"study","target":"hall"}"#);
    running(&server, &["r5"]);
    assert_eq!(receiver(&server, "r5").1, "live:hall+study");
    assert_eq!(bench.starts(5).len(), 2);
    assert_eq!(bench.started_with(5, "--data-dir").unwrap(), data_dir);

    // A renamed room is assigned again under the new name, on its receiver.
    server.applied(r#"{"v":1,"t":"name","zone":"den","name":"Den Two"}"#);
    wait_for("the den's receiver runs under the new name", || {
        receiver(&server, "r1") == ("running".into(), "room:den".into(), "Den Two".into())
            && bench.started_with(1, "--device-name").as_deref() == Some("Den Two")
    });

    // One target more than receivers: it has none, and the state says so.
    server.applied(r#"{"v":2,"t":"join","zone":"kitchen","target":"den"}"#);
    wait_for(
        "the newest live group is listed as having no receiver",
        || {
            list(&soloist(&server), "exhausted")
                .iter()
                .any(|t| t.as_str() == Some("live:den+kitchen"))
        },
    );
    assert!(
        ctl_ok(&server, &["soloist", "status"]).contains("no receiver left for: live:den+kitchen")
    );
}

/// Line B's first half: the Spotify app plays on a room's device, the room
/// plays the receiver, hears the signal sample for sample, and the state
/// says what is playing, via `spotify`.
#[test]
fn the_spotify_app_playing_on_a_rooms_receiver_is_heard_in_the_room_sample_for_sample() {
    let bench = Bench::new();
    let server = bench.server(&["kitchen"], 1, &[]);
    let _supervisors = bench.supervisors(1);
    let kitchen = listen(&server, "kitchen");
    running(&server, &["r0"]);
    assert_eq!(playing_in(&server, "kitchen").1, "stream");
    bench.app(0, "login");
    let from = kitchen.count();
    bench.app(0, &format!("play {TRACK_A}"));
    wait_for("the kitchen plays its receiver", || {
        playing_in(&server, "kitchen") == ("kitchen".into(), "soloist:r0".into())
    });
    // Two seconds of it, exactly.
    let heard = hears(
        &kitchen,
        from,
        TRACK_A,
        2 * RATE as usize,
        "the kitchen hears two seconds of the track",
    );
    let frames = assert_sample_for_sample(&heard, "the kitchen");
    assert!(frames >= 2 * RATE as usize);

    // What is playing: the item's names, the large cover, via spotify.
    wait_for("the state says what is playing", || {
        now_playing(&server, "kitchen").is_some_and(|r| text(&r, "state") == "playing")
    });
    let record = now_playing(&server, "kitchen").unwrap();
    assert_eq!(text(&record, "title"), format!("Fake {TRACK_A}"));
    assert_eq!(text(&record, "artist"), "Fake artist, Fake guest");
    assert_eq!(text(&record, "album"), format!("Fake album of {TRACK_A}"));
    assert_eq!(
        text(&record, "art_url"),
        format!("https://covers.example/large/{TRACK_A}")
    );
    assert_eq!(text(&record, "via"), "spotify");
    assert_eq!(
        record.get("duration_ms").and_then(Value::as_num),
        Some("180000")
    );

    // The app pauses: the record says paused, and the room hears silence,
    // not the tail again.
    bench.app(0, "pause");
    wait_for("the state says paused", || {
        now_playing(&server, "kitchen").is_some_and(|r| text(&r, "state") == "paused")
    });

    // The reader's counters are in the metrics endpoint.
    let played = metric(
        &server,
        "chorus_soloist_frames_played_total{receiver=\"r0\"}",
    )
    .unwrap();
    assert!(played >= 2 * i64::from(RATE), "{played}");
    assert!(
        metric(&server, "chorus_soloist_frames_read_total{receiver=\"r0\"}").unwrap() >= played
    );
    assert_eq!(
        metric(
            &server,
            "chorus_soloist_frames_dropped_total{receiver=\"r0\"}"
        ),
        Some(0)
    );
    assert_eq!(
        metric(
            &server,
            "chorus_soloist_receiver_connected{receiver=\"r0\"}"
        ),
        Some(1)
    );
}

/// Take the room (K78), both ways. Playing on a saved group's device moves
/// its rooms into the group and they all hear the group's signal; a member
/// room whose own receiver was playing has that receiver paused, then
/// deactivated, BEFORE the take, and no chunk it hears mixes the two
/// signals. Then playing on a room's own device while the room is in the
/// group takes the room out.
#[test]
fn playing_on_a_groups_receiver_takes_its_rooms_and_playing_on_a_rooms_takes_it_out() {
    let bench = Bench::new();
    let mut server = bench.server(&["kitchen", "den"], 3, &[]);
    let _supervisors = bench.supervisors(3);
    server.applied(
        r#"{"v":2,"t":"group_save","group":"downstairs","name":"Downstairs","zones":["kitchen","den"]}"#,
    );
    let kitchen = listen(&server, "kitchen");
    let den = listen(&server, "den");
    running(&server, &["r0", "r1", "r2"]);
    assert_eq!(receiver(&server, "r2").1, "group:downstairs");

    // The kitchen plays its own device, for a second and a half.
    bench.app(0, "login");
    bench.app(0, &format!("play {TRACK_A}"));
    wait_for("the kitchen plays its receiver", || {
        playing_in(&server, "kitchen").1 == "soloist:r0"
    });
    hears(
        &kitchen,
        0,
        TRACK_A,
        RATE as usize * 3 / 2,
        "the kitchen hears track A",
    );

    // The app plays on the group's device.
    bench.app(2, "login");
    let den_from = den.count();
    bench.app(2, &format!("play {TRACK_B}"));
    wait_for("both rooms play the group's receiver", || {
        ["kitchen", "den"]
            .iter()
            .all(|r| playing_in(&server, r) == ("downstairs".into(), "soloist:r2".into()))
    });
    // The displaced receiver was paused, then deactivated, and before the
    // take: the order of the fake's own command log, and of the server's.
    wait_for("the kitchen's receiver was told", || {
        bench.commands(0).len() >= 2
    });
    assert_eq!(bench.commands(0), ["pause", "deactivate"]);
    server.wait_for("soloist take target=downstairs source=soloist:r2 outcome=ok");
    let line_of = |what: &str| {
        server
            .seen
            .iter()
            .position(|l| l.contains(what))
            .unwrap_or_else(|| panic!("the server never said {what:?}"))
    };
    assert!(
        line_of("soloist receiver=r0 displaced by=r2 target=group:downstairs: pause, deactivate")
            < line_of("soloist take target=downstairs source=soloist:r2 outcome=ok"),
        "the displaced receiver is told before the take"
    );
    // Both rooms hear the group's signal, sample for sample.
    let in_den = hears(
        &den,
        den_from,
        TRACK_B,
        RATE as usize,
        "the den hears a second of track B",
    );
    assert_sample_for_sample(&in_den, "the den");
    wait_for("the kitchen hears the group's signal too", || {
        switches(&kitchen.chunks(), "the kitchen") == 1
    });
    // The kitchen heard A and then B: one switch, at a chunk boundary, so
    // no chunk mixes the two signals; and what it switched to started near
    // zero, as a track just played does (A had played for over a second).
    assert_eq!(
        switches(&kitchen.chunks(), "the kitchen"),
        1,
        "the kitchen went from its own receiver to the group's, once"
    );
    assert_eq!(
        switches(&den.chunks(), "the den"),
        0,
        "the den heard only B"
    );
    assert!(
        in_den[0] < 2 * u64::from(RATE),
        "the den's first frame is frame {} of the track",
        in_den[0]
    );
    // One grid: where both rooms have a chunk of the group's signal, it is
    // the same chunk.
    let in_kitchen = kitchen.chunks();
    wait_for("both rooms have heard the group for a while", || {
        den.count() >= den_from + 80 && kitchen.count() >= den_from + 80
    });
    let same = den
        .chunks()
        .iter()
        .skip(den_from)
        .filter(|c| {
            frames(c)
                .iter()
                .any(|f| matches!(classify(*f), Heard::Signal(_)))
        })
        .filter(|c| {
            in_kitchen
                .iter()
                .find(|k| k.sequence == c.sequence)
                .is_some_and(|k| {
                    assert_eq!(k.audio_data, c.audio_data, "chunk {}", c.sequence);
                    true
                })
        })
        .count();
    assert!(same >= 20, "only {same} chunks in common");

    // The other way: the den's own device plays while the den is in the
    // group. The den leaves the group and plays its own receiver; the
    // group's receiver, which the den was hearing, is paused first.
    bench.app(1, "login");
    let den_from = den.count();
    bench.app(1, &format!("play {TRACK_C}"));
    wait_for("the den plays its own receiver, out of the group", || {
        playing_in(&server, "den") == ("den".into(), "soloist:r1".into())
    });
    assert_eq!(playing_in(&server, "kitchen").0, "downstairs");
    wait_for("the group's receiver was told", || {
        bench.commands(2).len() >= 2
    });
    assert_eq!(bench.commands(2)[..2], ["pause", "deactivate"]);
    wait_for("the den hears track C", || {
        switches(&den.chunks(), "the den") == 1
    });
    let _ = den_from;
    // B, then C: one switch, at a chunk boundary.
    assert_eq!(switches(&den.chunks(), "the den"), 1);
    assert_eq!(
        switches(&kitchen.chunks(), "the kitchen"),
        1,
        "the kitchen was left alone"
    );
}

/// A group that takes another source makes the manager pause its receiver
/// (and deactivate it, since it was the active device), so the Spotify app
/// shows the truth and nothing plays unheard.
#[test]
fn a_group_that_takes_another_source_has_its_receiver_paused() {
    let bench = Bench::new();
    let mut server = bench.server(&["kitchen"], 1, &[]);
    let _supervisors = bench.supervisors(1);
    running(&server, &["r0"]);
    bench.app(0, "login");
    bench.app(0, &format!("play {TRACK_A}"));
    wait_for("the kitchen plays its receiver", || {
        playing_in(&server, "kitchen").1 == "soloist:r0"
    });
    assert!(bench.commands(0).is_empty(), "nothing was sent to play it");
    // A chime, as an alarm would ring it.
    server.applied(r#"{"v":2,"t":"take","target":"kitchen","source":"chime:bell"}"#);
    wait_for("the receiver was paused and deactivated", || {
        bench.commands(0) == ["pause", "deactivate"]
    });
    server.wait_for("soloist receiver=r0 no group plays it any more: pause, deactivate");
    server.wait_for("soloist receiver=r0 playback=paused");
    // And it stays that way: the room is not taken back. (A fixed wait,
    // longer than the manager's 3 s settle, to show nothing happens.)
    thread::sleep(Duration::from_millis(3_500));
    assert_eq!(playing_in(&server, "kitchen").1, "chime:bell");
    assert_eq!(bench.commands(0), ["pause", "deactivate"]);
    // A client cannot give the receiver back by name.
    let (status, answer) =
        server.command(r#"{"v":2,"t":"take","target":"kitchen","source":"soloist:r0"}"#);
    assert!(status.contains("422") || status.contains("400"), "{status}");
    assert!(answer.contains("a command cannot name one"), "{answer}");
    // The app can: playing again takes the room again.
    bench.app(0, "resume");
    wait_for("the kitchen plays its receiver again", || {
        playing_in(&server, "kitchen").1 == "soloist:r0"
    });
}

/// Volume, the default mapping: a volume from the Spotify app becomes the
/// room's volume, clamped by the room's limit, and the app's slider is set
/// to what the limit left; a chorus volume change reaches the receiver;
/// and neither comes back as an echo.
#[test]
fn a_volume_from_the_app_is_clamped_by_the_room_limit_and_a_chorus_volume_reaches_the_app() {
    let bench = Bench::new();
    let server = bench.server(&["kitchen"], 1, &[]);
    let _supervisors = bench.supervisors(1);
    running(&server, &["r0"]);
    server.applied(r#"{"v":2,"t":"limit","zone":"kitchen","limit":0.500}"#);
    bench.app(0, "login");
    // Soloist starts at 100 and the room is at its limit: the app's slider
    // is brought to the room's volume.
    wait_for("the receiver is told the room's volume", || {
        bench.commands(0) == ["set_volume 50"]
    });
    // The app turns it up past the limit.
    bench.app(0, "volume 80");
    wait_for("the app's slider is set back to the limit", || {
        bench.commands(0) == ["set_volume 50", "set_volume 50"]
    });
    assert_eq!(volume_of(&server, "kitchen"), "0.500");
    // The app turns it down: the room follows, and nothing is sent back.
    bench.app(0, "volume 20");
    wait_for("the room follows the app", || {
        volume_of(&server, "kitchen") == "0.200"
    });
    // chorus changes the volume: the receiver is told, once.
    server.applied(r#"{"v":1,"t":"volume","zone":"kitchen","volume":0.300}"#);
    wait_for("the receiver is told chorus's volume", || {
        bench.commands(0).last().map(String::as_str) == Some("set_volume 30")
    });
    // No echo loop: nothing more is sent and the volume stays. (A fixed
    // wait, to show nothing happens.)
    thread::sleep(Duration::from_millis(600));
    assert_eq!(
        bench.commands(0),
        ["set_volume 50", "set_volume 50", "set_volume 30"]
    );
    assert_eq!(volume_of(&server, "kitchen"), "0.300");
}

/// Volume, the other mapping (`--soloist-volume receiver`): Soloist's own
/// volume is the gain, chorus leaves its volumes alone and only clamps.
#[test]
fn under_the_receiver_mapping_chorus_only_clamps_the_apps_volume_to_the_limit() {
    let bench = Bench::new();
    let server = bench.server(&["kitchen"], 1, &["--soloist-volume", "receiver"]);
    let _supervisors = bench.supervisors(1);
    running(&server, &["r0"]);
    server.applied(r#"{"v":1,"t":"volume","zone":"kitchen","volume":0.400}"#);
    server.applied(r#"{"v":2,"t":"limit","zone":"kitchen","limit":0.600}"#);
    bench.app(0, "login");
    // Soloist starts at 100, above the limit: clamped.
    wait_for("the receiver is clamped to the limit", || {
        bench.commands(0) == ["set_volume 60"]
    });
    bench.app(0, "volume 30");
    bench.app(0, "volume 90");
    wait_for("a volume above the limit is clamped again", || {
        bench.commands(0) == ["set_volume 60", "set_volume 60"]
    });
    // chorus's own volume was never touched, and a chorus change is not
    // sent to the receiver. (A fixed wait, to show nothing happens.)
    server.applied(r#"{"v":1,"t":"volume","zone":"kitchen","volume":0.350}"#);
    thread::sleep(Duration::from_millis(500));
    assert_eq!(volume_of(&server, "kitchen"), "0.350");
    assert_eq!(bench.commands(0), ["set_volume 60", "set_volume 60"]);
}

/// Playback commands from chorus reach the receiver: `chorusctl soloist
/// pause` and `next`, and the catalog's `playback` for `resume` and
/// `previous`; a room that plays something else is refused by name.
#[test]
fn pause_and_next_from_chorusctl_and_the_catalog_reach_the_receiver() {
    let bench = Bench::new();
    let server = bench.server(&["kitchen", "den"], 2, &[]);
    let _supervisors = bench.supervisors(2);
    running(&server, &["r0", "r1"]);
    bench.app(0, "login");
    bench.app(0, &format!("play {TRACK_A}"));
    wait_for("the kitchen plays its receiver", || {
        playing_in(&server, "kitchen").1 == "soloist:r0"
    });
    ctl_ok(&server, &["soloist", "pause", "kitchen"]);
    wait_for("pause reached Soloist", || bench.commands(0) == ["pause"]);
    wait_for("the state says paused", || {
        now_playing(&server, "kitchen").is_some_and(|r| text(&r, "state") == "paused")
    });
    server.applied(r#"{"v":2,"t":"playback","target":"kitchen","action":"resume"}"#);
    ctl_ok(&server, &["soloist", "next", "kitchen"]);
    server.applied(r#"{"v":2,"t":"playback","target":"kitchen","action":"previous"}"#);
    wait_for("resume, next and previous reached Soloist", || {
        bench.commands(0) == ["pause", "play", "skip_next", "skip_prev"]
    });
    // The den plays the stream: there is no receiver to tell.
    let refused = ctl(&server, &["soloist", "pause", "den"]);
    assert_eq!(refused.code, 3, "{}", refused.stdout);
    assert!(
        refused.stderr.contains("which is not a Spotify receiver"),
        "{}",
        refused.stderr
    );
    assert!(bench.commands(1).is_empty());
}

// ----- the Spotify alarm source ------------------------------------------------------

/// A server twelve real seconds (two schedule minutes at 10x) before 07:00
/// on a Monday: time for the receiver to be assigned and logged in before
/// the alarm fires.
fn alarm_server(bench: &Bench, extra: &[&str]) -> RunningServer {
    let mut args = vec![
        "--civil-time-from",
        "2026-10-05T06:58:00Z",
        "--schedule-time-scale",
        "10",
    ];
    args.extend_from_slice(extra);
    let mut server = bench.server(&["kitchen"], 1, &args);
    server.wait_for("civil tz=");
    server
}

/// The stored Spotify source and an alarm at 07:00 that plays it.
fn set_alarm(server: &RunningServer) {
    server.applied(&format!(
        r#"{{"v":2,"t":"source_store","id":"wake-list","kind":"spotify","value":"{PLAYLIST}","name":"Wake up"}}"#
    ));
    server.applied(
        r#"{"v":2,"t":"alarm_set","alarm":"wake","target":"kitchen","time":"07:00","days":[],"source":"stored:wake-list","volume":0.500,"ramp_s":20,"duration_min":5,"enabled":true}"#,
    );
}

fn rings_the_chime(server: &mut RunningServer, reason: &str) -> String {
    wait_for("the alarm rings", || ringing(server, "wake"));
    let line = server.wait_for_all(&[
        "schedule alarm=wake fallback=chime",
        &format!("reason={reason}"),
    ]);
    wait_for("the kitchen plays the bell", || {
        playing_in(server, "kitchen").1 == "chime:bell"
    });
    line
}

/// The Spotify alarm source ships switched off (P7: the Developer Policy's
/// alarm clause is the owner's to read first): without `--soloist-alarms`
/// an alarm with a stored Spotify URI rings the chime, reason `soloist-off`,
/// and the receiver is sent nothing, though it is running and logged in.
#[test]
fn the_spotify_alarm_source_is_off_by_default_and_rings_the_chime() {
    let bench = Bench::new();
    let mut server = alarm_server(&bench, &[]);
    let _supervisors = bench.supervisors(1);
    set_alarm(&server);
    running(&server, &["r0"]);
    bench.app(0, "login");
    rings_the_chime(&mut server, "soloist-off");
    assert!(bench.commands(0).iter().all(|c| !c.starts_with("play")));
}

/// With `--soloist-alarms`: the alarm sends `play` with the stored URI to
/// its room's receiver, the room plays the receiver and hears that URI's
/// signal, and stopping the alarm pauses the receiver.
#[test]
fn with_soloist_alarms_an_alarm_plays_its_stored_uri_and_the_room_hears_it() {
    let bench = Bench::new();
    let mut server = alarm_server(&bench, &["--soloist-alarms"]);
    let _supervisors = bench.supervisors(1);
    let kitchen = listen(&server, "kitchen");
    set_alarm(&server);
    running(&server, &["r0"]);
    bench.app(0, "login");
    wait_for("the alarm rings", || ringing(&server, "wake"));
    server.wait_for(&format!(
        "soloist receiver=r0 alarm=wake play uri={PLAYLIST}"
    ));
    wait_for("the kitchen plays the receiver", || {
        playing_in(&server, "kitchen").1 == "soloist:r0"
    });
    server.wait_for("schedule alarm=wake started stored=wake-list plays=soloist:r0");
    assert!(
        bench.commands(0).contains(&format!("play {PLAYLIST}")),
        "{:?}",
        bench.commands(0)
    );
    let heard = hears(
        &kitchen,
        0,
        PLAYLIST,
        RATE as usize,
        "the kitchen hears a second of the playlist",
    );
    assert_sample_for_sample(&heard, "the kitchen");
    assert!(ringing(&server, "wake"));
    assert!(
        !server.seen.iter().any(|l| l.contains("fallback=chime")),
        "no fallback"
    );
    // Stopped: the room is restored and the receiver paused.
    server.applied(r#"{"v":2,"t":"alarm_stop","alarm":"wake"}"#);
    wait_for("the room is restored", || {
        playing_in(&server, "kitchen").1 == "stream" && !ringing(&server, "wake")
    });
    wait_for("the receiver is paused", || {
        bench.commands(0).iter().any(|c| c == "pause")
    });
}

/// A receiver nobody is logged in to: the chime, reason `soloist-logged-out`.
#[test]
fn an_alarm_on_a_logged_out_receiver_rings_the_chime() {
    let bench = Bench::new();
    let mut server = alarm_server(&bench, &["--soloist-alarms"]);
    let _supervisors = bench.supervisors(1);
    set_alarm(&server);
    running(&server, &["r0"]);
    let line = rings_the_chime(&mut server, "soloist-logged-out");
    assert!(line.contains("no Spotify account is logged in"), "{line}");
    assert!(bench.commands(0).is_empty());
}

/// An expired build (Soloist exits with code 10): the chime, reason
/// `soloist-expired`.
#[test]
fn an_alarm_on_an_expired_build_rings_the_chime() {
    let bench = Bench::new();
    bench.fake(0, &[("FAKE_SOLOIST_EXPIRED", "1")]);
    let mut server = alarm_server(&bench, &["--soloist-alarms"]);
    let _supervisors = bench.supervisors(1);
    set_alarm(&server);
    wait_for("the receiver is expired", || {
        receiver(&server, "r0").0 == "expired"
    });
    rings_the_chime(&mut server, "soloist-expired");
}

/// A receiver that accepts `play` and never reports `playing`: after the
/// 10 s bound the chime, reason `soloist-timeout`.
#[test]
fn an_alarm_whose_receiver_never_reports_playing_rings_the_chime_after_the_bound() {
    let bench = Bench::new();
    let mut server = alarm_server(&bench, &["--soloist-alarms"]);
    let _supervisors = bench.supervisors(1);
    set_alarm(&server);
    running(&server, &["r0"]);
    bench.app(0, "login");
    bench.app(0, "stall");
    wait_for("the alarm rings", || ringing(&server, "wake"));
    server.wait_for("soloist receiver=r0 alarm=wake play uri=");
    let asked = Instant::now();
    assert_ne!(playing_in(&server, "kitchen").1, "chime:bell", "not yet");
    let line = rings_the_chime(&mut server, "soloist-timeout");
    assert!(line.contains("no 'playing' within 10 s"), "{line}");
    let waited = asked.elapsed();
    assert!(
        waited >= Duration::from_secs(8) && waited < Duration::from_secs(25),
        "the fallback came after {waited:?}"
    );
    assert!(bench.commands(0).contains(&format!("play {PLAYLIST}")));
}

// ----- the expiry warning --------------------------------------------------------------

/// A `--version` line whose build is `days` days and one hour short of
/// expiry: "expires in <days> days".
fn version_expiring_in(days: u64) -> String {
    let build = now_epoch() + days * 86_400 + 3_600 - 90 * 86_400;
    format!("Soloist 9.9.9, build {build}, Linux/test")
}

/// A build ten days from expiry is warned about in the state, in
/// `chorusctl soloist status` and in the metrics; an expired build (exit
/// code 10) shows as expired; and after the owner "updates" the binary,
/// `chorusctl soloist restart` brings the receiver back.
#[test]
fn a_build_near_expiry_warns_an_expired_one_says_so_and_restart_after_an_update_recovers() {
    let bench = Bench::new();
    bench.fake(0, &[("FAKE_SOLOIST_VERSION", &version_expiring_in(10))]);
    let mut server = bench.server(&["kitchen"], 1, &[]);
    let _supervisors = bench.supervisors(1);
    running(&server, &["r0"]);
    wait_for("the state carries the warning", || {
        text(&soloist(&server), "warning") == "Soloist build expires in 10 days"
    });
    let member = soloist(&server);
    let build = member.get("build").unwrap();
    assert!(text(build, "version").starts_with("Soloist 9.9.9, build "));
    assert_eq!(
        build.get("expires_in_days").and_then(Value::as_num),
        Some("10")
    );
    server.wait_for("soloist warning: Soloist build expires in 10 days");
    let status = ctl_ok(&server, &["soloist", "status"]);
    assert!(
        status.starts_with("warning: Soloist build expires in 10 days\n"),
        "{status}"
    );
    assert!(status.contains("(expires in 10 days)"), "{status}");
    assert!(
        status.contains("r0") && status.contains("room:kitchen"),
        "{status}"
    );
    let left = metric(&server, "chorus_soloist_build_expires_seconds").unwrap();
    assert!(
        (10 * 86_400..=10 * 86_400 + 3_600).contains(&left),
        "{left} s"
    );
    assert_eq!(metric(&server, "chorus_soloist_build_expired"), Some(0));

    // The build expires: Soloist exits with code 10 at its next start.
    let old = now_epoch() - 100 * 86_400;
    bench.fake(
        0,
        &[
            (
                "FAKE_SOLOIST_VERSION",
                &format!("Soloist 9.9.9, build {old}, Linux/test"),
            ),
            ("FAKE_SOLOIST_EXPIRED", "1"),
        ],
    );
    ctl_ok(&server, &["soloist", "restart"]);
    wait_for("the receiver is expired", || {
        receiver(&server, "r0").0 == "expired"
            && text(&soloist(&server), "warning") == "Soloist build expired"
    });
    let status = ctl_ok(&server, &["soloist", "status"]);
    assert!(
        status.starts_with("warning: Soloist build expired\n"),
        "{status}"
    );
    assert!(status.contains("expired"), "{status}");
    assert_eq!(metric(&server, "chorus_soloist_build_expired"), Some(1));
    assert!(metric(&server, "chorus_soloist_build_expires_seconds").unwrap() < 0);
    // No restart loop: the fake was started once more, and not again. (A
    // fixed wait, to show nothing happens.)
    let starts = bench.starts(0).len();
    thread::sleep(Duration::from_millis(500));
    assert_eq!(
        bench.starts(0).len(),
        starts,
        "an expired build is not retried"
    );

    // The owner installs a new build and runs `chorusctl soloist restart`.
    bench.fake(0, &[("FAKE_SOLOIST_VERSION", &version_expiring_in(89))]);
    ctl_ok(&server, &["soloist", "restart"]);
    wait_for("the receiver runs again, with no warning", || {
        receiver(&server, "r0").0 == "running" && soloist(&server).get("warning").is_none()
    });
    assert_eq!(metric(&server, "chorus_soloist_build_expired"), Some(0));
    let status = ctl_ok(&server, &["soloist", "status"]);
    assert!(status.starts_with("build: Soloist 9.9.9"), "{status}");
}

// ----- restarts ------------------------------------------------------------------------

/// Soloist killed mid-play, then the supervisor stopped and started again
/// mid-play: each time the server is connected again, the receiver is
/// assigned again, and when the app plays the room hears it again.
#[test]
fn soloist_or_its_supervisor_restarted_mid_play_is_reconnected_and_the_room_plays_again() {
    let bench = Bench::new();
    let mut server = bench.server(&["kitchen"], 1, &[]);
    let mut supervisors = bench.supervisors(1);
    let kitchen = listen(&server, "kitchen");
    running(&server, &["r0"]);
    bench.app(0, "login");
    bench.app(0, &format!("play {TRACK_A}"));
    let heard = hears(
        &kitchen,
        0,
        TRACK_A,
        RATE as usize / 2,
        "the kitchen hears A",
    );
    assert_sample_for_sample(&heard, "the kitchen, before");

    // Soloist dies (a crash: exit code 1). The supervisor starts it again;
    // the login is in its data directory.
    let _ = bench.app_says(0, "exit 1");
    wait_for("Soloist was started again", || bench.starts(0).len() >= 2);
    running(&server, &["r0"]);
    server.wait_for("soloist receiver=r0 logged-in");
    let from = kitchen.count();
    bench.app(0, &format!("play {TRACK_B}"));
    hears_from_the_start(
        &kitchen,
        from,
        TRACK_B,
        RATE as usize / 2,
        "the kitchen hears B after Soloist was restarted",
    );
    assert_eq!(playing_in(&server, "kitchen").1, "soloist:r0");

    // The supervisor is stopped (a container restart) and started again.
    supervisors[0].stop();
    server.wait_for("soloist receiver=r0 disconnected");
    wait_for("the state says the receiver is absent", || {
        receiver(&server, "r0").0 == "absent"
    });
    assert_eq!(
        metric(
            &server,
            "chorus_soloist_receiver_connected{receiver=\"r0\"}"
        ),
        Some(0)
    );
    supervisors[0] = bench.supervisor(0, 1);
    running(&server, &["r0"]);
    assert_eq!(receiver(&server, "r0").1, "room:kitchen");
    let from = kitchen.count();
    bench.app(0, &format!("play {TRACK_C}"));
    hears_from_the_start(
        &kitchen,
        from,
        TRACK_C,
        RATE as usize / 2,
        "the kitchen hears C after the supervisor was restarted",
    );
}

/// The server restarted: a room and a saved group have their receivers
/// from the start, the receivers are assigned again to the same targets,
/// and Soloist is not restarted for it, once or in a loop.
#[test]
fn a_restarted_server_assigns_its_receivers_again_without_restarting_soloist() {
    let bench = Bench::new();
    let state_file = bench.root.join("state.json");
    let extra = ["--state-file", state_file.to_str().unwrap()];
    let _supervisors = bench.supervisors(3);
    {
        let server = bench.server(&["kitchen", "den"], 3, &extra);
        server.applied(
            r#"{"v":2,"t":"group_save","group":"downstairs","name":"Downstairs","zones":["kitchen","den"]}"#,
        );
        running(&server, &["r0", "r1", "r2"]);
        bench.app(2, "login");
    }
    for index in 0..3 {
        assert_eq!(bench.starts(index).len(), 1);
    }
    // A new server on the same state: the saved group is there at start.
    let mut server = bench.server(&["kitchen", "den"], 3, &extra);
    running(&server, &["r0", "r1", "r2"]);
    assert_eq!(
        receiver(&server, "r2"),
        (
            "running".into(),
            "group:downstairs".into(),
            "Downstairs".into()
        )
    );
    assert_eq!(receiver(&server, "r0").1, "room:kitchen");
    assert_eq!(receiver(&server, "r1").1, "room:den");
    // It learns the login it was not connected for.
    server.wait_for("soloist receiver=r2 logged-in");
    // Soloist was not restarted. (A fixed wait, to show nothing happens.)
    thread::sleep(Duration::from_millis(700));
    for index in 0..3 {
        assert_eq!(
            bench.starts(index).len(),
            1,
            "r{index}'s Soloist carried on through the server's restart"
        );
    }
    server.drain();
    let assigns = server
        .seen
        .iter()
        .filter(|l| l.contains("soloist receiver=r2 assign"))
        .count();
    assert_eq!(assigns, 1, "one assign, not a loop");
}

// ----- the thread population -----------------------------------------------------------

fn thread_roles(server: &mut RunningServer) -> Vec<String> {
    // The report is taken before the audio socket is bound, and the server
    // was waited for until it said it listens there: every row is in.
    server.drain();
    server
        .seen
        .iter()
        .filter_map(|line| {
            let at = line.find("thread role=")?;
            let rest = &line[at + "thread role=".len()..];
            Some(rest.split_once(" tid=")?.0.to_string())
        })
        .collect()
}

fn kernel_threads(pid: u32) -> usize {
    fs::read_dir(format!("/proc/{pid}/task")).unwrap().count()
}

/// With `--soloist-receivers R` the server has exactly `R + 1` more threads,
/// by role, from the start, with no receiver container running, and the
/// same number after receivers came, were assigned and played. Without the
/// flag (or with 0) there is no such thread, no `soloist` member in the
/// state, and the receiver directory's FIFO is not opened: what was written
/// into it is still there.
#[test]
fn the_receivers_threads_are_fixed_at_start_and_absent_without_the_flag() {
    let bench = Bench::new();
    let mut with = bench.server(&["kitchen", "den"], 2, &[]);
    let roles = thread_roles(&mut with);
    let ours: BTreeSet<&str> = roles
        .iter()
        .map(String::as_str)
        .filter(|r| r.starts_with("soloist"))
        .collect();
    assert_eq!(
        ours,
        BTreeSet::from(["soloist-manager", "soloist-reader-0", "soloist-reader-1"]),
        "{roles:?}"
    );
    assert!(!roles.iter().any(|r| r == "unregistered"), "{roles:?}");
    let before = kernel_threads(with.pid());
    assert_eq!(before, roles.len(), "every thread is in the report");
    let _supervisors = bench.supervisors(2);
    running(&with, &["r0", "r1"]);
    bench.app(0, "login");
    bench.app(0, &format!("play {TRACK_A}"));
    wait_for("the kitchen plays its receiver", || {
        playing_in(&with, "kitchen").1 == "soloist:r0"
    });
    with.applied(r#"{"v":2,"t":"join","zone":"den","target":"kitchen"}"#);
    thread::sleep(Duration::from_millis(300));
    assert_eq!(
        kernel_threads(with.pid()),
        before,
        "receivers, assignments and playing made no thread"
    );
    drop(with);

    // Without: a directory holding one FIFO with bytes in it.
    let quiet = Bench::new();
    let fifo = quiet.dir().join("r0.pcm");
    chorus_soloistd::sys::make_fifo(&fifo, 0o660).unwrap();
    let mut pipe = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(0o4000)
        .open(&fifo)
        .unwrap();
    pipe.write_all(&[7u8; 4096]).unwrap();
    let stream = quiet.root.join("stream.pcm");
    let mut without = RunningServer::start(&[
        "--source",
        stream.to_str().unwrap(),
        "--rate",
        "44100",
        "--tv-latency-ms",
        "40",
        "--slots",
        "2",
        "--zone",
        "kitchen",
        "--soloist-receivers",
        "0",
        "--soloist-dir",
        quiet.dir().to_str().unwrap(),
    ]);
    let roles = thread_roles(&mut without);
    assert!(!roles.iter().any(|r| r.contains("soloist")), "{roles:?}");
    assert_eq!(kernel_threads(without.pid()), roles.len());
    let state = without.state();
    assert!(!state.contains("soloist"), "{state}");
    assert!(!metrics(&without).contains("chorus_soloist"), "no metric");
    let (status, answer) = without.command(r#"{"v":2,"t":"soloist_restart"}"#);
    assert!(!status.contains("200"), "{status}");
    assert!(answer.contains("no-receivers"), "{answer}");
    without.drain();
    assert!(
        !without.seen.iter().any(|l| l.contains("soloist")),
        "the server says nothing of receivers"
    );
    // (A fixed wait, to show nothing reads the FIFO.)
    thread::sleep(Duration::from_millis(300));
    let mut back = [0u8; 8192];
    let n = pipe.read(&mut back).unwrap();
    assert_eq!(
        (n, &back[..n]),
        (4096, &[7u8; 4096][..]),
        "nothing drained it"
    );
    let entries: Vec<String> = fs::read_dir(quiet.dir())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(entries, ["r0.pcm"], "nothing was made in the directory");
}
