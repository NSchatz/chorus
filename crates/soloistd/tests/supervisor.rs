//! The real `chorus-soloistd` against the fake Soloist
//! (`crates/soloist-fake`, run as the example `chorus-fake-soloist`), with
//! this test playing chorus-server on the receiver's socket and, through the
//! fake's control socket, the Spotify app.
//!
//! No PipeWire runs here (`--pipewire none`; conventions rule 10): the fake
//! writes the FIFO itself, standing in for the pipe-tunnel sink.
//!
//! Every wait is bounded: a message is awaited until a deadline
//! ([`PATIENCE`], generous because the gate's hosts are shared) and the test
//! fails at the deadline. The only fixed sleeps are the two that show
//! something does NOT happen (no restart loop, no stall), 300 ms and 400 ms.

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::{FileTypeExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chorus_control::json::{self, Value};
use chorus_soloist::api::{self, Command as Api, Event, Status};
use chorus_soloist::protocol::{
    BuildReport, FromSupervisor, LineBuffer, State, StatusReport, ToSupervisor,
};
use chorus_soloist::{PCM_FRAME_BYTES, WS_PORT_FILE};
use chorus_soloist_fake::{expected_pcm, frame_of};

/// How long any one awaited thing may take.
const PATIENCE: Duration = Duration::from_secs(30);

/// The API key the tests use; it must never appear in the supervisor's log
/// or in a message.
const KEY: &str = "not-a-real-key-chorus-test";

static BENCHES: AtomicU32 = AtomicU32::new(0);

fn fake_soloist() -> PathBuf {
    // target/<profile>/deps/<this test> -> target/<profile>/examples/...
    let exe = std::env::current_exe().expect("the test's own path");
    let profile = exe
        .parent()
        .and_then(Path::parent)
        .expect("target/<profile>/deps");
    let fake = profile.join("examples").join("chorus-fake-soloist");
    assert!(
        fake.is_file(),
        "{} is not built; `cargo test` builds the example beside this test",
        fake.display()
    );
    fake
}

/// One scratch directory tree and what every supervisor of a test shares.
struct Bench {
    root: PathBuf,
    conf: PathBuf,
}

impl Bench {
    fn new() -> Bench {
        // Short, because a Unix socket's path is at most 107 bytes.
        let root = std::env::temp_dir().join(format!(
            "csd-{}-{}",
            std::process::id(),
            BENCHES.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("recv")).unwrap();
        fs::write(root.join("key"), format!("{KEY}\n")).unwrap();
        let conf = root.join("fake.conf");
        fs::write(&conf, "").unwrap();
        Bench { root, conf }
    }

    fn dir(&self) -> PathBuf {
        self.root.join("recv")
    }

    fn argv_log(&self) -> PathBuf {
        self.root.join("argv.log")
    }

    fn control(&self) -> PathBuf {
        self.root.join("app.sock")
    }

    /// What the fake does at its next start.
    fn fake(&self, settings: &[(&str, &str)]) {
        let text: String = settings.iter().map(|(k, v)| format!("{k}={v}\n")).collect();
        let temporary = self.root.join("fake.conf.tmp");
        fs::write(&temporary, text).unwrap();
        fs::rename(temporary, &self.conf).unwrap();
    }

    /// Every start of the fake so far, as its argument list.
    fn starts(&self) -> Vec<Vec<String>> {
        let Ok(text) = fs::read_to_string(self.argv_log()) else {
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

    fn supervisor(&self, extra: &[&str]) -> Supervisor {
        let mut command = Command::new(env!("CARGO_BIN_EXE_chorus-soloistd"));
        command
            .arg("--soloist-dir")
            .arg(self.dir())
            .arg("--api-key-file")
            .arg(self.root.join("key"))
            .arg("--state-dir")
            .arg(self.root.join("state"))
            .arg("--cache-dir")
            .arg(self.root.join("cache"))
            .args([
                "--pipewire",
                "none",
                "--backoff-min-ms",
                "40",
                "--backoff-max-ms",
                "160",
            ])
            .args(["--stop-timeout-ms", "3000"]);
        if !extra.contains(&"--soloist-bin") {
            command.arg("--soloist-bin").arg(fake_soloist());
        }
        if !extra.contains(&"--receivers") {
            command.args(["--receivers", "4"]);
        }
        command
            .args(extra)
            .env("FAKE_SOLOIST_CONF", &self.conf)
            .env("FAKE_SOLOIST_ARGV_LOG", self.argv_log())
            .env("FAKE_SOLOIST_CONTROL", self.control())
            .env("FAKE_SOLOIST_PIPE_DIR", self.dir())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let mut child = command.spawn().expect("chorus-soloistd starts");
        let log = Arc::new(Mutex::new(String::new()));
        let sink = Arc::clone(&log);
        let stderr = child.stderr.take().unwrap();
        let reader = thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let mut log = sink.lock().unwrap();
                log.push_str(&line);
                log.push('\n');
            }
        });
        Supervisor {
            child,
            log,
            reader: Some(reader),
        }
    }

    /// Connect to receiver `index`'s socket as the server would.
    fn server(&self, index: usize) -> Server {
        let path = self.dir().join(format!("r{index}.sock"));
        let stream = connect(&path);
        stream
            .set_read_timeout(Some(Duration::from_millis(50)))
            .unwrap();
        Server {
            stream,
            buffer: LineBuffer::new(),
        }
    }

    /// One line to the fake as "the Spotify app".
    fn app(&self, line: &str) {
        let mut stream = connect(&self.control());
        stream.set_read_timeout(Some(PATIENCE)).unwrap();
        stream.write_all(format!("{line}\n").as_bytes()).unwrap();
        let mut answer = String::new();
        BufReader::new(stream).read_line(&mut answer).unwrap();
        assert_eq!(answer, "ok\n", "the fake refused {line:?}");
    }
}

impl Drop for Bench {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn connect(path: &Path) -> UnixStream {
    let deadline = Instant::now() + PATIENCE;
    loop {
        match UnixStream::connect(path) {
            Ok(stream) => return stream,
            Err(e) if Instant::now() >= deadline => panic!("{}: {e}", path.display()),
            Err(_) => thread::sleep(Duration::from_millis(5)),
        }
    }
}

struct Supervisor {
    child: Child,
    log: Arc<Mutex<String>>,
    /// The thread collecting stderr; it ends when the supervisor does.
    reader: Option<thread::JoinHandle<()>>,
}

impl Supervisor {
    fn log(&self) -> String {
        self.log.lock().unwrap().clone()
    }

    fn wait_log(&self, needle: &str) {
        let deadline = Instant::now() + PATIENCE;
        while !self.log().contains(needle) {
            assert!(
                Instant::now() < deadline,
                "the log never said {needle:?}:\n{}",
                self.log()
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    /// SIGTERM, and the exit code.
    fn stop(mut self) -> (Option<i32>, String) {
        chorus_soloistd::sys::terminate(self.child.id()).unwrap();
        let code = self.wait_exit();
        (code, self.log())
    }

    fn wait_exit(&mut self) -> Option<i32> {
        let deadline = Instant::now() + PATIENCE;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                // The whole log: the reader ends when the pipe does.
                if let Some(reader) = self.reader.take() {
                    reader.join().unwrap();
                }
                return status.code();
            }
            assert!(Instant::now() < deadline, "the supervisor did not exit");
            thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = chorus_soloistd::sys::terminate(self.child.id());
            let deadline = Instant::now() + Duration::from_secs(10);
            while matches!(self.child.try_wait(), Ok(None)) && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(5));
            }
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// This test as chorus-server.
struct Server {
    stream: UnixStream,
    buffer: LineBuffer,
}

impl Server {
    fn send(&mut self, message: &ToSupervisor) {
        self.stream
            .write_all(message.encode().unwrap().as_bytes())
            .unwrap();
    }

    fn assign(&mut self, generation: u64, target: &str, name: &str) {
        self.send(&ToSupervisor::Assign {
            generation,
            target: target.into(),
            name: name.into(),
        });
    }

    fn command(&mut self, generation: u64, command: &Api) {
        self.send(&ToSupervisor::Command {
            generation,
            command: command.to_value(),
        });
    }

    /// The next message, or `None` when the supervisor closed the socket.
    fn next(&mut self) -> Option<FromSupervisor> {
        let deadline = Instant::now() + PATIENCE;
        let mut chunk = [0u8; 4096];
        loop {
            if let Some(line) = self.buffer.next_line().unwrap() {
                assert!(!line.contains(KEY), "the API key is in a message: {line}");
                return Some(
                    FromSupervisor::decode(&line).unwrap_or_else(|e| panic!("{line}: {e}")),
                );
            }
            assert!(Instant::now() < deadline, "no message from the supervisor");
            match self.stream.read(&mut chunk) {
                Ok(0) => return None,
                Ok(n) => self.buffer.feed(&chunk[..n]),
                Err(_) => {}
            }
        }
    }

    /// Skip messages until `pick` takes one.
    fn until<T>(&mut self, what: &str, mut pick: impl FnMut(&FromSupervisor) -> Option<T>) -> T {
        let mut seen = Vec::new();
        loop {
            let message = self
                .next()
                .unwrap_or_else(|| panic!("the socket closed before {what}; saw {seen:#?}"));
            if let Some(found) = pick(&message) {
                return found;
            }
            seen.push(message);
            assert!(seen.len() < 500, "500 messages and no {what}: {seen:#?}");
        }
    }

    fn status(&mut self, state: State) -> StatusReport {
        self.until(&format!("status {}", state.as_str()), |m| match m {
            FromSupervisor::Status(status) if status.state == state => Some(status.clone()),
            _ => None,
        })
    }

    /// The next event `pick` takes, with the generation it was relayed under.
    fn event<T>(&mut self, what: &str, mut pick: impl FnMut(&Event) -> Option<T>) -> (u64, T) {
        self.until(what, |m| match m {
            FromSupervisor::Event { generation, event } => {
                let event = api::event_from_value(event).expect("an event");
                pick(&event).map(|found| (*generation, found))
            }
            _ => None,
        })
    }

    /// `hello`, `build`, `status`: what every connection starts with.
    fn greeting(&mut self) -> (usize, BuildReport, StatusReport) {
        let Some(FromSupervisor::Hello {
            v,
            receiver,
            supervisor,
        }) = self.next()
        else {
            panic!("the first message is not hello");
        };
        assert_eq!(v, 1);
        assert_eq!(supervisor, env!("CARGO_PKG_VERSION"));
        let Some(FromSupervisor::Build(build)) = self.next() else {
            panic!("the second message is not build");
        };
        let Some(FromSupervisor::Status(status)) = self.next() else {
            panic!("the third message is not status");
        };
        (receiver, build, status)
    }
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn playing(event: &Event) -> Option<()> {
    (event.status() == Some(&Status::Playing)).then_some(())
}

fn auth(event: &Event) -> Option<bool> {
    match event {
        Event::AuthState { logged_in, .. } => Some(*logged_in),
        _ => None,
    }
}

/// The value after a flag in an argument list.
fn flag<'a>(arguments: &'a [String], name: &str) -> &'a str {
    let at = arguments
        .iter()
        .position(|a| a == name)
        .unwrap_or_else(|| panic!("no {name} in {arguments:?}"));
    &arguments[at + 1]
}

#[test]
fn two_supervisors_claim_the_lowest_free_indexes() {
    let bench = Bench::new();
    let first = bench.supervisor(&["--receivers", "2"]);
    let (index, _, status) = bench.server(0).greeting();
    assert_eq!((index, status.state), (0, State::Idle));
    let second = bench.supervisor(&["--receivers", "2"]);
    let (index, _, _) = bench.server(1).greeting();
    assert_eq!(index, 1);
    // The files of the receiver directory, with their documented modes.
    for i in 0..2 {
        let fifo = fs::metadata(bench.dir().join(format!("r{i}.pcm"))).unwrap();
        assert!(fifo.file_type().is_fifo());
        assert_eq!(fifo.permissions().mode() & 0o777, 0o660);
        let socket = fs::metadata(bench.dir().join(format!("r{i}.sock"))).unwrap();
        assert!(socket.file_type().is_socket());
        assert_eq!(socket.permissions().mode() & 0o777, 0o660);
        assert!(bench.dir().join(format!("r{i}.lock")).is_file());
    }
    // A third has no index left, and a named index that is held is refused.
    let mut third = bench.supervisor(&["--receivers", "2"]);
    assert_eq!(third.wait_exit(), Some(3));
    assert!(
        third.log().contains("all 2 receiver indexes are held"),
        "{}",
        third.log()
    );
    let mut named = bench.supervisor(&["--receivers", "2", "--receiver", "1"]);
    assert_eq!(named.wait_exit(), Some(3));
    // SIGTERM ends a supervisor with 0 and takes its socket away; its index
    // is free for the next one.
    let (code, _) = first.stop();
    assert_eq!(code, Some(0));
    assert!(!bench.dir().join("r0.sock").exists());
    let _again = bench.supervisor(&["--receivers", "2"]);
    assert_eq!(bench.server(0).greeting().0, 0);
    drop(second);
}

#[test]
fn a_connection_starts_with_hello_build_and_status() {
    let bench = Bench::new();
    bench.fake(&[(
        "FAKE_SOLOIST_VERSION",
        "Soloist: 1.3.8.28\\nBuild: 1788933710 (20260909)",
    )]);
    let _supervisor = bench.supervisor(&["--receiver", "2"]);
    let (index, build, status) = bench.server(2).greeting();
    assert_eq!(index, 2);
    assert!(build.present);
    assert!(build.version.starts_with("Soloist: 1.3.8.28"), "{build:?}");
    assert_eq!(build.build_epoch, Some(1_788_933_710));
    assert_eq!(build.expires_epoch, Some(1_788_933_710 + 90 * 86_400));
    assert_eq!(
        status,
        StatusReport {
            state: State::Idle,
            target: String::new(),
            name: String::new(),
            detail: String::new(),
            generation: 0,
        }
    );
}

#[test]
fn assign_starts_soloist_with_the_documented_flags_and_relays_its_events() {
    let bench = Bench::new();
    let supervisor = bench.supervisor(&["--receiver", "1", "--cache-size", "300"]);
    let mut server = bench.server(1);
    server.greeting();
    server.assign(7, "live:den+kitchen", "Kitchen + Den");
    let starting = server.status(State::Starting);
    assert_eq!(
        (
            starting.target.as_str(),
            starting.name.as_str(),
            starting.generation
        ),
        ("live:den+kitchen", "Kitchen + Den", 7)
    );
    // auth_state is what Soloist sends on connect; it arrives after
    // `running` and under the assignment's generation.
    let running = server.status(State::Running);
    assert_eq!(running.generation, 7);
    let (generation, logged_in) = server.event("auth_state", auth);
    assert_eq!((generation, logged_in), (7, false));

    // The argument list the fake saw: exactly the documented flags.
    let starts = bench.starts();
    assert_eq!(starts.len(), 1);
    let state = bench.root.join("state/live-den_kitchen");
    let cache = bench.root.join("cache/live-den_kitchen");
    let expected: Vec<String> = [
        "--device-name",
        "Kitchen + Den",
        "--data-dir",
        state.to_str().unwrap(),
        "--cache-dir",
        cache.to_str().unwrap(),
        "--cache-size",
        "300",
        "--pipewire-device",
        "chorus-r1",
        "--initial-volume",
        "100",
        "--ws",
        "127.0.0.1:0",
        "--api-key",
        KEY,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert_eq!(starts[0], expected);
    assert!(state.join(WS_PORT_FILE).is_file());

    // A command when logged out: Soloist's documented error comes back as
    // an event (the connection stays open).
    server.command(7, &Api::Pause);
    server.event("the error", |e| {
        matches!(e, Event::Error { .. }).then_some(())
    });

    // Stopping the supervisor stops Soloist the normal way: its files go.
    let (code, log) = supervisor.stop();
    assert_eq!(code, Some(0));
    assert!(
        !state.join(WS_PORT_FILE).exists(),
        "ws.port survived:\n{log}"
    );
    assert!(log.contains("fake-soloist: normal shutdown"), "{log}");
    // The key: on the command line the supervisor logged, and in the line
    // the fake wrote to stderr; redacted in both, present in neither.
    assert!(!log.contains(KEY), "the API key is in the log:\n{log}");
    assert!(log.contains("--api-key [redacted]"), "{log}");
    assert!(
        log.contains(
            "soloist: fake-soloist: starting, device_name=Kitchen + Den, api key [redacted]"
        ),
        "{log}"
    );
}

#[test]
fn the_spotify_app_plays_and_the_fifo_carries_the_signal_sample_for_sample() {
    let bench = Bench::new();
    let _supervisor = bench.supervisor(&[]);
    let mut server = bench.server(0);
    server.greeting();
    // The server's reader: read-write and non-blocking, like FifoSource.
    let mut fifo = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(0o4000)
        .open(bench.dir().join("r0.pcm"))
        .unwrap();
    server.assign(1, "room:kitchen", "Kitchen");
    server.status(State::Running);

    bench.app("login");
    let (_, logged_in) = server.event("auth_state after login", |e| auth(e).filter(|l| *l));
    assert!(logged_in);
    let uri = "spotify:playlist:37i9dQZF1DXcBWIGoYBM5M";
    bench.app(&format!("play {uri}"));
    let (_, active) = server.event("device_changed", |e| match e {
        Event::DeviceChanged { is_active, .. } => *is_active,
        _ => None,
    });
    assert!(active);
    let (_, item) = server.event("track_changed", |e| match e {
        Event::TrackChanged { item } => item.clone(),
        _ => None,
    });
    assert_eq!(item.uri, uri);
    assert_eq!(item.entity_type, "playlist");
    server.event("playback_changed playing", playing);

    // A quarter of a second of audio and more, compared with the signal
    // the fake defines for this URI: every sample, bit for bit. The fake
    // writes 512 frames at a time and, like the sink, drops a write the
    // pipe has no room for; a pipe takes such a write whole or not at all,
    // so what arrives is whole writes, each naming the frame it starts at.
    // On a host that stalls this reader for longer than the pipe holds, a
    // write may be missing; none may be wrong, repeated or out of order,
    // and the first is frame 0 (the pipe was open and empty before `play`).
    const WRITE: usize = 512;
    let wanted = 24 * WRITE * PCM_FRAME_BYTES;
    let mut pcm = Vec::new();
    let mut chunk = [0u8; 8192];
    let deadline = Instant::now() + PATIENCE;
    while pcm.len() < wanted {
        assert!(
            Instant::now() < deadline,
            "only {} bytes in the FIFO",
            pcm.len()
        );
        match fifo.read(&mut chunk) {
            Ok(n) => pcm.extend_from_slice(&chunk[..n]),
            Err(_) => thread::sleep(Duration::from_millis(2)),
        }
    }
    assert!(
        pcm.len().is_multiple_of(WRITE * PCM_FRAME_BYTES),
        "{}",
        pcm.len()
    );
    let mut next = 0u64;
    for (i, write) in pcm.chunks(WRITE * PCM_FRAME_BYTES).enumerate() {
        let head: [u8; PCM_FRAME_BYTES] = write[..PCM_FRAME_BYTES].try_into().unwrap();
        let (left, right) = chorus_soloist::decode_frame(head);
        let first = frame_of(uri, left, right)
            .unwrap_or_else(|| panic!("write {i} does not start with a frame of the signal"));
        assert!(
            first.is_multiple_of(WRITE as u64) && first >= next,
            "write {i} starts at frame {first}, after {next}"
        );
        assert!(
            i > 0 || first == 0,
            "the first write starts at frame {first}"
        );
        assert!(
            write == expected_pcm(uri, first, WRITE),
            "write {i} (frame {first}) differs from the signal"
        );
        next = first + WRITE as u64;
    }
    // And it is not another URI's signal.
    assert_ne!(
        pcm[..WRITE * PCM_FRAME_BYTES],
        expected_pcm("spotify:track:other", 0, WRITE)[..]
    );

    // A command from chorus: pause reaches Soloist, its result and the
    // change come back.
    server.command(1, &Api::Pause);
    let (_, command) = server.event("command_result", |e| match e {
        Event::CommandResult { command } => Some(command.clone()),
        _ => None,
    });
    assert_eq!(command, "pause");
    server.event("playback_changed paused", |e| {
        (e.status() == Some(&Status::Paused)).then_some(())
    });
    // get_state answers with the snapshot: paused, the item, volume 100
    // (the supervisor starts Soloist with --initial-volume 100).
    server.command(1, &Api::GetState);
    let (_, state) = server.event("playback_state", |e| match e {
        Event::PlaybackState(state) => Some(state.clone()),
        _ => None,
    });
    assert_eq!(state.status, Some(Status::Paused));
    assert_eq!(state.item.as_ref().map(|i| i.uri.as_str()), Some(uri));
    assert_eq!(state.volume, Some(100));
    // A command for another generation is dropped, and said so.
    server.command(99, &Api::Play { uri: None });
    let dropped = server.until("the dropped command's status", |m| match m {
        FromSupervisor::Status(s) if s.detail.starts_with("command dropped") => Some(s.clone()),
        _ => None,
    });
    assert_eq!(dropped.state, State::Running);
}

#[test]
fn a_second_assign_restarts_soloist_under_another_directory_and_name_and_release_stops_it() {
    let bench = Bench::new();
    let _supervisor = bench.supervisor(&[]);
    let mut server = bench.server(0);
    server.greeting();
    server.assign(1, "room:kitchen", "Kitchen");
    server.status(State::Running);
    let kitchen = bench.root.join("state/room-kitchen");
    assert!(kitchen.join(WS_PORT_FILE).is_file());
    bench.app("login");
    server.event("login", |e| auth(e).filter(|l| *l));

    // The same target and name again: nothing restarts, the generation moves.
    server.assign(2, "room:kitchen", "Kitchen");
    let same = server.status(State::Running);
    assert_eq!(same.generation, 2);
    assert_eq!(bench.starts().len(), 1);

    server.assign(3, "group:downstairs", "Downstairs");
    let running = server.until("running as Downstairs", |m| match m {
        FromSupervisor::Status(s) if s.state == State::Running && s.generation == 3 => {
            Some(s.clone())
        }
        _ => None,
    });
    assert_eq!(
        (running.target.as_str(), running.name.as_str()),
        ("group:downstairs", "Downstairs")
    );
    let starts = bench.starts();
    assert_eq!(starts.len(), 2);
    let downstairs = bench.root.join("state/group-downstairs");
    assert_eq!(flag(&starts[1], "--device-name"), "Downstairs");
    assert_eq!(flag(&starts[1], "--data-dir"), downstairs.to_str().unwrap());
    assert_eq!(
        flag(&starts[1], "--cache-dir"),
        bench.root.join("cache/group-downstairs").to_str().unwrap()
    );
    // The first Soloist was stopped the normal way before the second began.
    assert!(!kitchen.join(WS_PORT_FILE).exists());
    assert!(downstairs.join(WS_PORT_FILE).is_file());
    // The new target's Soloist is a new device: not logged in, and its
    // events carry the new generation.
    let (generation, logged_in) = server.event("the new auth_state", auth);
    assert_eq!((generation, logged_in), (3, false));

    // A rename is the same target under another name: a restart, the same
    // directory.
    server.assign(4, "group:downstairs", "Ground floor");
    server.until("running as Ground floor", |m| match m {
        FromSupervisor::Status(s) if s.state == State::Running && s.name == "Ground floor" => {
            Some(())
        }
        _ => None,
    });
    let starts = bench.starts();
    assert_eq!(starts.len(), 3);
    assert_eq!(flag(&starts[2], "--data-dir"), downstairs.to_str().unwrap());

    server.send(&ToSupervisor::Release { generation: 5 });
    let idle = server.status(State::Idle);
    assert_eq!(
        idle,
        StatusReport {
            state: State::Idle,
            target: String::new(),
            name: String::new(),
            detail: String::new(),
            generation: 5,
        }
    );
    assert!(!downstairs.join(WS_PORT_FILE).exists());
    // Back to the kitchen: the same directory as before, so the stored
    // session is still there and Soloist comes up logged in.
    server.assign(6, "room:kitchen", "Kitchen");
    let (generation, logged_in) = server.event("the kept session", auth);
    assert_eq!((generation, logged_in), (6, true));
    // A target that has no directory name is refused and nothing changes.
    server.assign(7, "room:../etc", "Nope");
    let refused = server.until("the refusal", |m| match m {
        FromSupervisor::Status(s) if s.detail.starts_with("assign refused") => Some(s.clone()),
        _ => None,
    });
    assert_eq!(
        (refused.target.as_str(), refused.generation),
        ("room:kitchen", 6)
    );
}

#[test]
fn exit_10_is_expired_with_no_restart_loop_and_restart_recovers_after_an_update() {
    let bench = Bench::new();
    let old = format!("Soloist 1.0.0, build {}", now_epoch() - 91 * 86_400);
    bench.fake(&[
        ("FAKE_SOLOIST_EXPIRED", "1"),
        ("FAKE_SOLOIST_VERSION", &old),
    ]);
    let supervisor = bench.supervisor(&[]);
    let mut server = bench.server(0);
    let (_, build, _) = server.greeting();
    assert_eq!(build.version, old);
    supervisor.wait_log("warning: Soloist build expired");
    server.assign(1, "room:kitchen", "Kitchen");
    let expired = server.status(State::Expired);
    assert!(expired.detail.contains("exited 10"), "{expired:?}");
    // No restart loop: with a 40 ms backoff a loop would have started the
    // fake several times in this window. It started once.
    thread::sleep(Duration::from_millis(300));
    assert_eq!(bench.starts().len(), 1);
    // Assigning the same target again (a server that reconnected) does not
    // start it either.
    server.assign(2, "room:kitchen", "Kitchen");
    assert_eq!(server.status(State::Expired).generation, 2);
    assert_eq!(bench.starts().len(), 1);

    // The owner replaces the binary; `chorusctl soloist restart`.
    let new = format!("Soloist 1.1.0, build {}", now_epoch() - 86_400);
    bench.fake(&[("FAKE_SOLOIST_VERSION", &new)]);
    server.send(&ToSupervisor::Restart);
    let build = server.until("the new build", |m| match m {
        FromSupervisor::Build(build) => Some(build.clone()),
        _ => None,
    });
    assert_eq!(build.version, new);
    assert_eq!(
        build.expires_epoch,
        build.build_epoch.map(|b| b + 90 * 86_400)
    );
    let running = server.status(State::Running);
    assert_eq!(
        (running.target.as_str(), running.generation),
        ("room:kitchen", 2)
    );
    assert_eq!(bench.starts().len(), 2);
}

#[test]
fn exit_1_is_failed_and_retried_with_a_capped_backoff() {
    let bench = Bench::new();
    bench.fake(&[("FAKE_SOLOIST_FAIL", "1")]);
    let _supervisor = bench.supervisor(&[]);
    let mut server = bench.server(0);
    server.greeting();
    let begun = Instant::now();
    server.assign(1, "room:kitchen", "Kitchen");
    // 40 ms, doubled each time, capped at 160 ms.
    for (attempt, delay) in [(1, 40), (2, 80), (3, 160), (4, 160)] {
        let failed = server.until(&format!("failure {attempt}"), |m| match m {
            FromSupervisor::Status(s)
                if s.state == State::Failed
                    && s.detail.contains(&format!("(attempt {attempt})")) =>
            {
                Some(s.clone())
            }
            _ => None,
        });
        assert_eq!(
            failed.detail,
            format!("Soloist exited 1; retry in {delay} ms (attempt {attempt})")
        );
        // (At least: on a slow host the next retry may already have begun.)
        assert!(bench.starts().len() >= attempt);
    }
    // Four starts took at least the three delays between them.
    assert!(begun.elapsed() >= Duration::from_millis(40 + 80 + 160));
    // Whatever was wrong is put right: the next retry succeeds, unasked.
    bench.fake(&[]);
    let running = server.status(State::Running);
    assert_eq!(running.detail, "");
    // A release while failing stops the retries.
    bench.fake(&[("FAKE_SOLOIST_FAIL", "1")]);
    server.assign(2, "room:den", "Den");
    server.status(State::Failed);
    server.send(&ToSupervisor::Release { generation: 3 });
    server.status(State::Idle);
    let starts = bench.starts().len();
    thread::sleep(Duration::from_millis(300));
    assert_eq!(bench.starts().len(), starts);
}

#[test]
fn a_missing_ws_port_is_a_fault() {
    let bench = Bench::new();
    bench.fake(&[("FAKE_SOLOIST_NO_WS", "1")]);
    let supervisor = bench.supervisor(&["--ws-timeout-ms", "300"]);
    let mut server = bench.server(0);
    server.greeting();
    server.assign(1, "room:kitchen", "Kitchen");
    server.status(State::Starting);
    let failed = server.status(State::Failed);
    assert!(
        failed
            .detail
            .starts_with("no WebSocket within 300 ms (no ws.port in the data directory)"),
        "{failed:?}"
    );
    // The Soloist that never offered its API was stopped, not left running.
    supervisor.wait_log("fake-soloist: normal shutdown");
    supervisor.wait_log("the WebSocket API could not bind");
}

#[test]
fn a_build_ten_days_from_expiry_is_warned_about() {
    let bench = Bench::new();
    // Built 80 days ago less an hour: 10 whole days and an hour are left.
    let built = now_epoch() - 80 * 86_400 + 3_600;
    bench.fake(&[(
        "FAKE_SOLOIST_VERSION",
        &format!("soloist 1.3.7.292 (build {built}, x86_64, ...)"),
    )]);
    let supervisor = bench.supervisor(&[]);
    let (_, build, _) = bench.server(0).greeting();
    assert_eq!(build.build_epoch, Some(built));
    assert_eq!(build.expires_epoch, Some(built + 90 * 86_400));
    supervisor.wait_log("warning: Soloist build expires in 10 days");
}

#[test]
fn an_unparseable_version_is_unknown_and_never_a_warning() {
    let bench = Bench::new();
    bench.fake(&[("FAKE_SOLOIST_VERSION", "soloist: unrecognized build stamp")]);
    let supervisor = bench.supervisor(&[]);
    let mut server = bench.server(0);
    let (_, build, _) = server.greeting();
    assert_eq!(
        build,
        BuildReport {
            present: true,
            version: "soloist: unrecognized build stamp".into(),
            build_epoch: None,
            expires_epoch: None,
        }
    );
    // Unknown is not expired: Soloist is started as usual.
    server.assign(1, "room:kitchen", "Kitchen");
    server.status(State::Running);
    let (_, log) = supervisor.stop();
    assert!(log.contains("expiry unknown"), "{log}");
    assert!(!log.contains("warning:"), "{log}");
}

#[test]
fn no_binary_is_reported_and_found_when_it_appears() {
    let bench = Bench::new();
    let bin = bench.root.join("soloist");
    let _supervisor = bench.supervisor(&["--soloist-bin", bin.to_str().unwrap()]);
    let mut server = bench.server(0);
    let (_, build, _) = server.greeting();
    assert_eq!(
        build,
        BuildReport {
            present: false,
            version: String::new(),
            build_epoch: None,
            expires_epoch: None,
        }
    );
    // A command with nothing running is dropped, and said so.
    server.command(0, &Api::Pause);
    let dropped = server.until("the dropped command", |m| match m {
        FromSupervisor::Status(s) if s.detail.starts_with("command dropped") => Some(s.clone()),
        _ => None,
    });
    assert_eq!(dropped.state, State::Idle);
    server.assign(1, "room:kitchen", "Kitchen");
    let missing = server.status(State::NoBinary);
    assert!(
        missing.detail.contains("no Soloist executable"),
        "{missing:?}"
    );
    // The owner mounts the binary (here: a link to the fake).
    std::os::unix::fs::symlink(fake_soloist(), &bin).unwrap();
    let build = server.until("the build", |m| match m {
        FromSupervisor::Build(build) if build.present => Some(build.clone()),
        _ => None,
    });
    assert!(build.present);
    server.status(State::Running);
}

#[test]
fn the_server_may_drop_and_reconnect_and_a_new_connection_replaces_the_old() {
    let bench = Bench::new();
    let _supervisor = bench.supervisor(&[]);
    let mut server = bench.server(0);
    server.greeting();
    server.assign(4, "room:kitchen", "Kitchen");
    server.status(State::Running);
    bench.app("login");
    server.event("login", |e| auth(e).filter(|l| *l));
    drop(server);

    // Soloist carries on; the new connection is told everything again, and
    // gets a fresh auth_state without asking.
    let mut again = bench.server(0);
    let (index, build, status) = again.greeting();
    assert_eq!(index, 0);
    assert!(build.present);
    assert_eq!(
        (status.state, status.target.as_str(), status.generation),
        (State::Running, "room:kitchen", 4)
    );
    let (generation, logged_in) = again.event("auth_state", auth);
    assert_eq!((generation, logged_in), (4, true));
    assert_eq!(bench.starts().len(), 1);

    // A second connection while the first is open: the first is closed.
    let mut newer = bench.server(0);
    newer.greeting();
    while again.next().is_some() {}
    newer.command(4, &Api::GetAuthState);
    newer.event("auth_state on the new connection", auth);

    // Soloist's WebSocket dropping is not a restart either: the supervisor
    // connects again.
    bench.app("drop-ws");
    let lost = newer.status(State::Starting);
    assert!(lost.detail.contains("the WebSocket was lost"), "{lost:?}");
    newer.status(State::Running);
    assert_eq!(bench.starts().len(), 1);
    // A line that is not the protocol is skipped, not fatal.
    newer
        .stream
        .write_all(b"not json\n{\"t\":\"dance\"}\n")
        .unwrap();
    newer.command(4, &Api::GetAuthState);
    newer.event("auth_state after the bad lines", auth);
}

#[test]
fn an_absent_fifo_reader_does_not_stall_the_supervisor_or_soloist() {
    let bench = Bench::new();
    let _supervisor = bench.supervisor(&[]);
    let mut server = bench.server(0);
    server.greeting();
    server.assign(1, "room:kitchen", "Kitchen");
    server.status(State::Running);
    bench.app("login");
    bench.app("play spotify:track:6rqhFgbbKwnb9MLmUQDhG6");
    server.event("playing", playing);
    // Nobody reads r0.pcm. A pipe holds 64 KiB, 186 ms of this format; in
    // 400 ms the fake has filled it and is dropping, as the sink would.
    thread::sleep(Duration::from_millis(400));
    // Everything still answers: the app, a command, an event.
    bench.app("volume 42");
    let (_, volume) = server.event("volume_changed", |e| match e {
        Event::VolumeChanged { volume } => *volume,
        _ => None,
    });
    assert_eq!(volume, 42);
    server.command(1, &Api::Pause);
    server.event("paused", |e| {
        (e.status() == Some(&Status::Paused)).then_some(())
    });
    // A late reader finds the pipe full of the oldest audio (why the server
    // drains always): exactly the pipe's capacity from frame 0.
    let mut fifo = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(0o4000)
        .open(bench.dir().join("r0.pcm"))
        .unwrap();
    let mut stale = Vec::new();
    let mut chunk = [0u8; 8192];
    while let Ok(n) = fifo.read(&mut chunk) {
        stale.extend_from_slice(&chunk[..n]);
    }
    assert!(
        stale.len() >= 4096 && stale.len().is_multiple_of(PCM_FRAME_BYTES),
        "{}",
        stale.len()
    );
    let frames = stale.len() / PCM_FRAME_BYTES;
    assert_eq!(
        stale,
        expected_pcm("spotify:track:6rqhFgbbKwnb9MLmUQDhG6", 0, frames)
    );
    // And a server that stops reading its socket does not stall it: the
    // connection is replaceable at any time.
    let mut fresh = bench.server(0);
    assert_eq!(fresh.greeting().2.state, State::Running);
}

#[test]
fn usage_errors_exit_2() {
    let bench = Bench::new();
    let mut bad = bench.supervisor(&["--cache-size", "50"]);
    assert_eq!(bad.wait_exit(), Some(2));
    assert!(bad
        .log()
        .contains("--cache-size is 0 (no limit) or at least 100"));
    let out = Command::new(env!("CARGO_BIN_EXE_chorus-soloistd"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("--soloist-dir DIR"));
}
