//! Goal 14, line C: no image installs without an explicit install action
//! (K93, I13), on the real `chorus-server` binary with the real C endpoint.
//!
//! The endpoint is `firmware/build/chorus-endpoint-session`, the host build
//! of the session supervisor and the update unit the board runs, over a
//! two-slot fake flash kept in a file (`--ota-flash`): starting the binary
//! again on the same file is a reboot, with ESP-IDF v6.1's bootloader rule
//! applied first (`firmware/tests/fake_flash.c`). The images are made by the
//! same binary (`--ota-make-image`) and staged with the server's own helper
//! (`chorus-server stage-firmware`), so what the server verifies and sends is
//! what an owner would stage.
//!
//! This test builds the endpoint with the firmware's own Makefile, so it
//! needs what `make firmware-check` needs (make, a C compiler and the pinned
//! ESP-IDF tree for the crypto sources) and fails by name without them.
//!
//! Every wait here is on an event (a line the endpoint printed, a line the
//! server logged, a member of the state message) with a generous deadline;
//! none is a sleep that hopes. Loopback only: nothing here touches a device
//! and nothing sets the owner-at-bench variable (program section 0.7).
//! Nothing here is timing evidence.

mod common;

use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use chorus_control::json::{self, Value};
use common::RunningServer;

/// How long any one step may take before the test says something hung. Far
/// above what a step takes: the gate runs this on a loaded host.
const STEP: Duration = Duration::from_secs(90);

/// The board the test's endpoints claim, and one they do not.
const BOARD: &str = "bench-board";
const OTHER_BOARD: &str = "other-board";

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("this crate lives at crates/<name> under the repository root")
        .to_path_buf()
}

/// The C endpoint binary, built (or found current) by the firmware's own
/// Makefile, once per test process and under a file lock so two test
/// processes do not run the same make at once.
fn endpoint_binary() -> &'static PathBuf {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY.get_or_init(|| {
        let root = repository_root();
        let build = root.join("firmware").join("build");
        fs::create_dir_all(&build).expect("firmware/build can be made");
        let binary = build.join("chorus-endpoint-session");
        let lock = File::create(build.join(".chorus-endpoint-session.lock")).expect("a lock file");
        lock.lock().expect("the build lock");
        let how = "this test runs the real C endpoint, built as `make firmware-check` builds it: \
                   it needs make, a C compiler and the pinned ESP-IDF tree (CHORUS_IDF_V61_DIR)";
        let made = Command::new("make")
            .current_dir(&root)
            .arg("--no-print-directory")
            .arg("-f")
            .arg(root.join("firmware").join("Makefile"))
            .arg(&binary)
            .output();
        match made {
            Ok(out) if out.status.success() => {}
            Ok(out) => panic!(
                "MISSING PREREQUISITE: the C endpoint could not be built. {}.\n{}\n{}",
                how,
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ),
            Err(e) => panic!(
                "MISSING PREREQUISITE: make could not be run ({}). {}",
                e, how
            ),
        }
        assert!(binary.is_file(), "{}", how);
        binary
    })
}

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    listener.local_addr().unwrap().port()
}

/// An id shaped like the firmware's own: `chorus-` and twelve hex digits.
fn speaker_id(test: u64, n: u64) -> String {
    format!(
        "chorus-{:012x}",
        (u64::from(std::process::id()) << 16 | test << 8 | n) & 0xffff_ffff_ffff
    )
}

/// One run of the endpoint binary: one boot.
struct Boot {
    child: Option<Child>,
    stdout: Arc<Mutex<Vec<String>>>,
}

impl Boot {
    fn lines(&self) -> Vec<String> {
        self.stdout.lock().unwrap().clone()
    }

    /// Every `ota state=` line it printed, in order.
    fn states(&self) -> Vec<String> {
        self.lines()
            .into_iter()
            .filter(|l| l.starts_with("ota state="))
            .collect()
    }

    /// Wait for a line starting with `prefix`, and give it back.
    fn wait_for(&self, prefix: &str) -> String {
        let deadline = Instant::now() + STEP;
        loop {
            if let Some(line) = self.lines().into_iter().find(|l| l.starts_with(prefix)) {
                return line;
            }
            assert!(
                Instant::now() < deadline,
                "the endpoint never printed {:?} within {:?}; it printed:\n{}",
                prefix,
                STEP,
                self.lines().join("\n")
            );
            thread::sleep(Duration::from_millis(20));
        }
    }

    /// Wait for the process to end by itself (it rebooted); its exit code
    /// and every stdout line.
    fn finish(mut self) -> (i32, Vec<String>) {
        let mut child = self.child.take().expect("it is running");
        let deadline = Instant::now() + STEP;
        loop {
            if let Some(status) = child.try_wait().expect("wait") {
                // The reader thread ends at end of file, just after the exit.
                let settle = Instant::now() + Duration::from_secs(5);
                while Instant::now() < settle
                    && !self.lines().iter().any(|l| l.contains("run_seconds="))
                {
                    thread::sleep(Duration::from_millis(20));
                }
                return (status.code().unwrap_or(-1), self.lines());
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!(
                    "the endpoint did not exit within {:?}; it printed:\n{}",
                    STEP,
                    self.lines().join("\n")
                );
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    /// Send the process a signal by name (`STOP`, `CONT`).
    fn signal(&self, name: &str) {
        let pid = self.child.as_ref().expect("it is running").id();
        let sent = Command::new("kill")
            .arg(format!("-{}", name))
            .arg(pid.to_string())
            .status()
            .expect("kill runs");
        assert!(sent.success(), "kill -{} {}", name, pid);
    }
}

impl Drop for Boot {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
            if thread::panicking() {
                eprintln!("an endpoint printed:\n{}", self.lines().join("\n"));
            }
        }
    }
}

/// One speaker: its id, its flash file, its key and its pins.
struct Speaker {
    id: String,
    dir: PathBuf,
    board: String,
}

impl Speaker {
    fn flash(&self) -> PathBuf {
        self.dir.join("flash.bin")
    }

    fn flash_bytes(&self) -> Vec<u8> {
        fs::read(self.flash()).expect("the flash file")
    }

    /// Start the endpoint: one boot on this speaker's flash file.
    fn boot(&self, server: &str, extra: &[&str]) -> Boot {
        let mut child = Command::new(endpoint_binary())
            .args(["--server", server])
            .args(["--run-seconds", "600"])
            .args(["--first-backoff-ms", "50", "--max-backoff-ms", "400"])
            .args(["--endpoint-id", &self.id])
            .arg("--key")
            .arg(self.dir.join("endpoint.key"))
            .arg("--server-pins")
            .arg(self.dir.join("server-pins"))
            .arg("--ota-flash")
            .arg(self.flash())
            .args(["--ota-version", "1.0.0", "--ota-board", &self.board])
            .args(extra)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the endpoint binary starts");
        let stdout = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&stdout);
        let pipe = child.stdout.take().unwrap();
        thread::spawn(move || {
            for line in BufReader::new(pipe).lines().map_while(Result::ok) {
                sink.lock().unwrap().push(line);
            }
        });
        Boot {
            child: Some(child),
            stdout,
        }
    }
}

/// A scratch directory, a firmware directory, and a server's files in it.
struct Rig {
    dir: PathBuf,
    listen: String,
    test: u64,
}

impl Rig {
    fn new(name: &str, test: u64) -> Rig {
        let dir = std::env::temp_dir().join(format!(
            "chorus-firmware-install-{}-{}",
            name,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("firmware")).expect("a scratch directory");
        Rig {
            dir,
            listen: format!("127.0.0.1:{}", free_port()),
            test,
        }
    }

    fn firmware_dir(&self) -> PathBuf {
        self.dir.join("firmware")
    }

    /// The server, on this rig's address, identity, state file and firmware
    /// directory: starting it again is a restart.
    fn server(&self) -> RunningServer {
        let identity = self.dir.join("identity");
        let state_file = self.dir.join("zones.state");
        let firmware = self.firmware_dir();
        let mut server = RunningServer::start_on(
            &self.listen,
            &["--identity-dir", identity.to_str().unwrap()],
            &[
                "--source",
                "tone",
                "--serve-forever",
                "--max-clients",
                "6",
                "--state-file",
                state_file.to_str().unwrap(),
                "--firmware-dir",
                firmware.to_str().unwrap(),
            ],
        );
        said(&mut server, &["firmware dir=", "images="]);
        server
    }

    fn speaker(&self, n: u64, board: &str) -> Speaker {
        let id = speaker_id(self.test, n);
        let dir = self.dir.join(&id);
        fs::create_dir_all(&dir).unwrap();
        Speaker {
            id,
            dir,
            board: board.to_string(),
        }
    }

    /// Make an image of `version` the endpoint's flash accepts, and stage it
    /// as `name` for `board` with the server's own helper. Returns its bytes.
    fn stage(&self, name: &str, version: &str, board: &str, bytes: usize) -> Vec<u8> {
        let built = self.dir.join(format!("build-{}.bin", name));
        let made = Command::new(endpoint_binary())
            .arg("--ota-make-image")
            .arg(&built)
            .args(["--ota-version", version, "--ota-image-bytes"])
            .arg(bytes.to_string())
            .output()
            .expect("the endpoint binary runs");
        assert!(made.status.success(), "{:?}", made);
        let staged = Command::new(env!("CARGO_BIN_EXE_chorus-server"))
            .arg("stage-firmware")
            .arg("--image")
            .arg(&built)
            .args(["--board", board, "--name", name])
            .arg("--firmware-dir")
            .arg(self.firmware_dir())
            .output()
            .expect("the server binary runs");
        assert!(
            staged.status.success(),
            "staging failed:\n{}\n{}",
            String::from_utf8_lossy(&staged.stdout),
            String::from_utf8_lossy(&staged.stderr)
        );
        let image = fs::read(self.firmware_dir().join(format!("{}.bin", name))).unwrap();
        assert_eq!(image, fs::read(&built).unwrap(), "staged byte for byte");
        image
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        if !thread::panicking() {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
}

/// Wait (generously) for a server log line holding every one of `what`.
fn said(server: &mut RunningServer, what: &[&str]) -> String {
    let deadline = Instant::now() + STEP;
    loop {
        server.drain();
        if let Some(line) = server
            .seen
            .iter()
            .find(|l| what.iter().all(|w| l.contains(w)))
        {
            return line.clone();
        }
        assert!(
            Instant::now() < deadline,
            "the server never said {:?} within {:?}; it said:\n{}",
            what,
            STEP,
            server.seen.join("\n")
        );
        thread::sleep(Duration::from_millis(20));
    }
}

/// How many `firmware_offer`s the server says it has sent so far: one log
/// line per offer, written where the offer is queued.
fn offers(server: &mut RunningServer) -> usize {
    server.drain();
    server
        .seen
        .iter()
        .filter(|l| l.contains("firmware offer "))
        .count()
}

fn state(server: &RunningServer) -> Value {
    let text = server.state();
    json::parse(&text).unwrap_or_else(|e| panic!("the state is JSON ({}): {}", e, text))
}

fn listed(state: &Value, member: &str, key: &str, id: &str) -> Option<Value> {
    match state.get(member) {
        Some(Value::Arr(items)) => items
            .iter()
            .find(|s| s.get(key).and_then(Value::as_str) == Some(id))
            .cloned(),
        _ => None,
    }
}

/// A speaker's `firmware` object in the state, once it has one.
fn firmware_of(state: &Value, id: &str) -> Option<Value> {
    listed(state, "speakers", "id", id).and_then(|s| s.get("firmware").cloned())
}

fn image_of(state: &Value, name: &str) -> Option<Value> {
    state
        .get("firmware")
        .and_then(|f| listed(f, "images", "name", name))
}

fn text(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::Str(s)) => s.clone(),
        Some(Value::Null) => "null".to_string(),
        Some(Value::Num(n)) => n.clone(),
        other => panic!("no text '{}' in {}: {:?}", key, json::write(value), other),
    }
}

fn flag(value: &Value, key: &str) -> bool {
    value
        .get(key)
        .and_then(Value::as_bool)
        .unwrap_or_else(|| panic!("no flag '{}' in {}", key, json::write(value)))
}

/// Poll the state until `done` holds of it, or panic naming `what`.
fn until(server: &RunningServer, what: &str, done: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + STEP;
    loop {
        let now = state(server);
        if done(&now) {
            return now;
        }
        assert!(
            Instant::now() < deadline,
            "{}: not within {:?}; the state is {}",
            what,
            STEP,
            json::write(&now)
        );
        thread::sleep(Duration::from_millis(50));
    }
}

/// Wait until speaker `id` is present and has said what it runs.
fn reported(server: &RunningServer, id: &str) -> Value {
    let now = until(
        server,
        "the speaker is present and reported its firmware",
        |s| {
            listed(s, "speakers", "id", id).is_some_and(|v| flag(&v, "present"))
                && firmware_of(s, id).is_some()
        },
    );
    firmware_of(&now, id).unwrap()
}

fn install(id: &str, image: &str) -> String {
    format!(
        r#"{{"v":2,"t":"firmware_install","speaker":"{}","image":"{}"}}"#,
        id, image
    )
}

/// A command that must be refused: the refusal's body.
fn refused(server: &RunningServer, body: &str, field: &str, name: &str) -> String {
    let (status, answer) = server.command(body);
    assert!(
        status.contains("400"),
        "{} was answered {} {}",
        body,
        status,
        answer
    );
    assert!(
        answer.contains(&format!(r#""field":"{}""#, field)),
        "{} names another field: {}",
        body,
        answer
    );
    assert!(
        answer.contains(&format!(r#""detail":"{}: "#, name)),
        "{} is not refused as {}: {}",
        body,
        name,
        answer
    );
    answer
}

/// THE test for the goal's line: "No image installs without an explicit
/// install action".
///
/// In order:
///
/// 1. A real server with a staged, verified, newer image, and a real C
///    endpoint running version 1.0.0, adopted and connected. The state says
///    `update_available` for it and lists the image `verified`.
/// 2. The graded window. Events, not a sleep: the state is read twenty
///    times, a `firmware_rescan` is applied, the server is KILLED AND
///    STARTED AGAIN on the same files, and the endpoint reconnects by
///    itself. Across all of it the state says `update_available` and `idle`,
///    neither server process logged one `firmware_offer`, the endpoint
///    printed no state but `idle`, and its flash file is byte for byte what
///    it was.
/// 3. `firmware_install` is posted. The image is written and verified (the
///    endpoint's own lines), the endpoint reboots (the binary exits; it is
///    started again on the same flash file), boots the new slot on trial,
///    confirms, and the state shows the new version, `confirmed`, and no
///    update available. The server sent exactly one offer.
#[test]
fn nothing_installs_until_the_explicit_install_action() {
    let rig = Rig::new("line-c", 1);
    let image = rig.stage("good", "2.0.0", BOARD, 150_000);
    let speaker = rig.speaker(1, BOARD);
    let mut server = rig.server();
    said(
        &mut server,
        &["firmware image name=good", "verdict=verified"],
    );

    // --- 1. staged, verified, newer; the speaker adopted and connected -------
    let boot = speaker.boot(&rig.listen, &[]);
    boot.wait_for("ota boot slot=0 image=valid version=1.0.0");
    let fw = reported(&server, &speaker.id);
    assert_eq!(text(&fw, "version"), "1.0.0");
    assert_eq!(text(&fw, "board"), BOARD);
    assert_eq!(text(&fw, "slot"), "0");
    assert_eq!(text(&fw, "state"), "idle");
    assert!(
        flag(&fw, "update_available"),
        "a newer verified image is staged"
    );
    let now = state(&server);
    let listed_image = image_of(&now, "good").expect("the staged image is listed");
    assert_eq!(text(&listed_image, "verdict"), "verified");
    assert_eq!(text(&listed_image, "version"), "2.0.0");
    assert_eq!(text(&listed_image, "size"), image.len().to_string());
    said(&mut server, &["endpoint adopted", &speaker.id]);
    let flash_before = speaker.flash_bytes();

    // --- 2. the graded window: update available, and nothing installs -------
    let quiet = |server: &RunningServer| {
        let fw = firmware_of(&state(server), &speaker.id).expect("its firmware is known");
        assert!(flag(&fw, "update_available"));
        assert_eq!(text(&fw, "state"), "idle");
        assert_eq!(text(&fw, "version"), "1.0.0");
        assert_eq!(text(&fw, "image"), "null", "no install is in hand");
    };
    for _ in 0..20 {
        quiet(&server);
    }
    // Looking at the directory again is not an install either.
    server.applied(r#"{"v":2,"t":"firmware_rescan"}"#);
    quiet(&server);
    assert_eq!(
        offers(&mut server),
        0,
        "the first server process sent no offer"
    );
    assert_eq!(speaker.flash_bytes(), flash_before);

    // A server restart, and the endpoint reconnecting by itself.
    drop(server);
    let mut server = rig.server();
    said(
        &mut server,
        &["firmware image name=good", "verdict=verified"],
    );
    reported(&server, &speaker.id);
    said(
        &mut server,
        &["client session", &speaker.id, "verdict=known"],
    );
    for _ in 0..20 {
        quiet(&server);
    }
    assert_eq!(
        offers(&mut server),
        0,
        "the second server process sent no offer"
    );
    let states = boot.states();
    assert!(
        !states.is_empty() && states.iter().all(|l| l.starts_with("ota state=idle ")),
        "the endpoint was offered nothing: {:?}",
        states
    );
    assert_eq!(
        speaker.flash_bytes(),
        flash_before,
        "staging, a rescan, a server restart and a reconnect wrote no byte of the flash"
    );
    let report = common::http(
        &server.control,
        "GET /api/report HTTP/1.1\r\nHost: chorus\r\nConnection: close\r\n\r\n",
    )
    .1;
    assert!(
        report.contains("control applied=0 refused=0"),
        "nobody has sent this server process a command: {}",
        report
    );

    // --- 3. the explicit install action ---------------------------------------
    let answer = server.applied(&install(&speaker.id, "good"));
    let asked = firmware_of(&json::parse(&answer).unwrap(), &speaker.id).unwrap();
    assert_eq!(text(&asked, "state"), "requested");
    assert_eq!(text(&asked, "image"), "good");
    assert_eq!(text(&asked, "image_version"), "2.0.0");
    assert_eq!(text(&asked, "size"), image.len().to_string());
    let offer = said(&mut server, &["firmware offer ", &speaker.id]);
    assert!(
        offer.contains("image=good") && offer.contains("version=\"2.0.0\""),
        "{}",
        offer
    );

    // Written and verified: the endpoint says so and reboots.
    let (code, lines) = boot.finish();
    assert_eq!(code, 0, "{}", lines.join("\n"));
    let verified = lines
        .iter()
        .find(|l| l.starts_with("ota state=verified "))
        .unwrap_or_else(|| panic!("never verified:\n{}", lines.join("\n")));
    assert!(
        verified.contains(&format!("received={} ", image.len()))
            && verified.contains("image=2.0.0"),
        "{}",
        verified
    );
    assert!(
        lines.iter().any(|l| l == "ota reboot"),
        "{}",
        lines.join("\n")
    );
    said(&mut server, &["firmware verified ", &speaker.id]);
    let written = speaker.flash_bytes();
    assert_ne!(written, flash_before, "the install wrote the flash");
    assert!(
        written.windows(image.len()).any(|w| w == &image[..]),
        "the staged image is in the flash, byte for byte"
    );
    until(
        &server,
        "the state says verified, then the speaker is gone",
        |s| {
            firmware_of(s, &speaker.id).is_some_and(|f| text(&f, "state") == "verified")
                && listed(s, "speakers", "id", &speaker.id).is_some_and(|v| !flag(&v, "present"))
        },
    );

    // Rebooted on the same flash file: the new slot on trial, then confirmed.
    let boot = speaker.boot(&rig.listen, &[]);
    boot.wait_for("ota boot slot=1 image=pending-verify version=2.0.0");
    boot.wait_for("ota state=confirmed ");
    let done = until(&server, "the state shows the new version, confirmed", |s| {
        firmware_of(s, &speaker.id).is_some_and(|f| text(&f, "state") == "confirmed")
    });
    let fw = firmware_of(&done, &speaker.id).unwrap();
    assert_eq!(
        text(&fw, "version"),
        "2.0.0",
        "the state shows the new version"
    );
    assert_eq!(text(&fw, "slot"), "1");
    assert_eq!(text(&fw, "image"), "good");
    assert!(
        !flag(&fw, "update_available"),
        "it runs the staged version now"
    );
    assert_eq!(offers(&mut server), 1, "one install action, one offer");
    println!("firmware at the end: {}", json::write(&fw));
    drop(boot);
    drop(server);
}

/// A staged image whose bytes are not the ones its manifest's digest was
/// taken of is refused at staging time, listed `refused`, and never offered:
/// refused before activation, before a byte of it leaves the server. And an
/// image that was verified and then changed on disk is refused at the
/// install action itself. (What the ENDPOINT does with a bad digest, should
/// one ever reach it, is `chorus-g14/ota-core`'s:
/// `a_good_image_installs_and_confirms_and_a_bad_one_is_rolled_back_over_a_real_session`
/// in `crates/protocol/tests/firmware_session.rs` and the fake-flash suite.)
#[test]
fn a_bad_digest_is_refused_before_activation_and_never_offered() {
    let rig = Rig::new("bad-digest", 2);
    rig.stage("tampered", "2.0.0", BOARD, 90_000);
    let later = rig.stage("later", "3.0.0", BOARD, 90_000);
    // One byte of the staged file changes after its manifest was written.
    let path = rig.firmware_dir().join("tampered.bin");
    let mut bytes = fs::read(&path).unwrap();
    bytes[70_000] ^= 0x01;
    fs::write(&path, &bytes).unwrap();

    let speaker = rig.speaker(1, BOARD);
    let mut server = rig.server();
    said(
        &mut server,
        &[
            "firmware image name=tampered",
            "verdict=refused",
            "reason=digest-mismatch",
        ],
    );
    let boot = speaker.boot(&rig.listen, &[]);
    reported(&server, &speaker.id);
    let now = state(&server);
    let tampered = image_of(&now, "tampered").unwrap();
    assert_eq!(text(&tampered, "verdict"), "refused");
    assert_eq!(text(&tampered, "reason"), "digest-mismatch");
    let flash_before = speaker.flash_bytes();

    refused(
        &server,
        &install(&speaker.id, "tampered"),
        "image",
        "image-not-verified",
    );
    refused(
        &server,
        r#"{"v":2,"t":"firmware_install","all":true,"image":"tampered"}"#,
        "image",
        "image-not-verified",
    );

    // Verified at the scan, changed since: refused at the install action.
    assert_eq!(
        text(&image_of(&now, "later").unwrap(), "verdict"),
        "verified"
    );
    let path = rig.firmware_dir().join("later.bin");
    let mut changed = later.clone();
    changed[70_000] ^= 0x01;
    fs::write(&path, &changed).unwrap();
    let answer = refused(
        &server,
        &install(&speaker.id, "later"),
        "image",
        "image-not-verified",
    );
    assert!(answer.contains("changed on disk"), "{}", answer);
    // A rescan sees it for what it is now.
    let rescanned = json::parse(&server.applied(r#"{"v":2,"t":"firmware_rescan"}"#)).unwrap();
    assert_eq!(
        text(&image_of(&rescanned, "later").unwrap(), "reason"),
        "digest-mismatch"
    );
    let fw = firmware_of(&rescanned, &speaker.id).unwrap();
    assert!(!flag(&fw, "update_available"), "no verified image is left");
    assert_eq!(text(&fw, "state"), "idle");

    assert_eq!(offers(&mut server), 0, "a refused image is never offered");
    assert!(boot
        .states()
        .iter()
        .all(|l| l.starts_with("ota state=idle ")));
    assert_eq!(speaker.flash_bytes(), flash_before);
}

/// An image that never confirms is rolled back by the bootloader, and the
/// state says `rolled_back`, naming the image that was tried and the version
/// that runs again.
#[test]
fn an_image_that_never_confirms_is_rolled_back_and_the_state_says_so() {
    let rig = Rig::new("rollback", 3);
    rig.stage("bad", "3.0.0", BOARD, 90_000);
    let speaker = rig.speaker(1, BOARD);
    let mut server = rig.server();
    let boot = speaker.boot(&rig.listen, &[]);
    reported(&server, &speaker.id);

    server.applied(&install(&speaker.id, "bad"));
    let (code, lines) = boot.finish();
    assert_eq!(code, 0, "{}", lines.join("\n"));
    assert!(lines.iter().any(|l| l.starts_with("ota state=verified ")));

    // The bad image's boot: on trial, it never confirms; at its deadline it
    // marks itself invalid and reboots.
    let trial = speaker.boot(
        &rig.listen,
        &["--ota-never-confirm", "--ota-confirm-seconds", "3"],
    );
    trial.wait_for("ota boot slot=1 image=pending-verify version=3.0.0");
    until(&server, "the state shows the image on trial", |s| {
        firmware_of(s, &speaker.id).is_some_and(|f| {
            text(&f, "state") == "pending_verify" && text(&f, "version") == "3.0.0"
        })
    });
    // An install during the trial is busy.
    refused(&server, &install(&speaker.id, "bad"), "speaker", "busy");
    let (code, lines) = trial.finish();
    assert_eq!(code, 0, "{}", lines.join("\n"));
    assert!(
        lines.iter().any(|l| l == "ota reboot"),
        "{}",
        lines.join("\n")
    );

    // The bootloader went back, and the first session says so.
    let back = speaker.boot(&rig.listen, &[]);
    back.wait_for("ota boot slot=0 image=valid version=1.0.0");
    back.wait_for("ota state=rolled_back ");
    let done = until(&server, "the state says rolled_back", |s| {
        firmware_of(s, &speaker.id).is_some_and(|f| text(&f, "state") == "rolled_back")
    });
    let fw = firmware_of(&done, &speaker.id).unwrap();
    assert_eq!(text(&fw, "reason"), "not_confirmed");
    assert_eq!(
        text(&fw, "version"),
        "1.0.0",
        "the previous version runs again"
    );
    assert_eq!(text(&fw, "slot"), "0");
    assert_eq!(text(&fw, "image"), "bad", "the image that was tried");
    assert_eq!(text(&fw, "image_version"), "3.0.0");
    assert!(flag(&fw, "update_available"), "information, as before");
    said(&mut server, &["firmware rolled_back ", &speaker.id]);
    assert_eq!(
        offers(&mut server),
        1,
        "the rollback is not retried by anything"
    );
    println!("firmware after the rollback: {}", json::write(&fw));
}

/// `firmware_install` with `all` reaches every present speaker of the
/// image's board that does not run its version already, and no other.
#[test]
fn firmware_install_with_all_reaches_every_speaker_of_the_board_and_no_other() {
    let rig = Rig::new("all", 4);
    let image = rig.stage("good", "2.0.0", BOARD, 90_000);
    let first = rig.speaker(1, BOARD);
    let second = rig.speaker(2, BOARD);
    let other = rig.speaker(3, OTHER_BOARD);
    let mut server = rig.server();
    let first_boot = first.boot(&rig.listen, &[]);
    let second_boot = second.boot(&rig.listen, &[]);
    let other_boot = other.boot(&rig.listen, &[]);
    for speaker in [&first, &second, &other] {
        reported(&server, &speaker.id);
    }
    let now = state(&server);
    assert!(flag(
        &firmware_of(&now, &first.id).unwrap(),
        "update_available"
    ));
    assert!(
        !flag(&firmware_of(&now, &other.id).unwrap(), "update_available"),
        "an image for another board is not an update for this one"
    );
    let other_flash = other.flash_bytes();

    let answer = server.applied(r#"{"v":2,"t":"firmware_install","all":true,"image":"good"}"#);
    let asked = json::parse(&answer).unwrap();
    for speaker in [&first, &second] {
        let fw = firmware_of(&asked, &speaker.id).unwrap();
        assert_eq!(text(&fw, "image"), "good", "{}", speaker.id);
    }
    assert_eq!(
        text(&firmware_of(&asked, &other.id).unwrap(), "state"),
        "idle"
    );

    for (speaker, boot) in [(&first, first_boot), (&second, second_boot)] {
        let (code, lines) = boot.finish();
        assert_eq!(code, 0, "{}", lines.join("\n"));
        assert!(lines.iter().any(|l| l.starts_with("ota state=verified ")));
        assert!(speaker
            .flash_bytes()
            .windows(image.len())
            .any(|w| w == &image[..]));
    }
    let boots: Vec<Boot> = [&first, &second]
        .iter()
        .map(|s| s.boot(&rig.listen, &[]))
        .collect();
    let done = until(&server, "both speakers of the board confirmed", |s| {
        [&first, &second].iter().all(|speaker| {
            firmware_of(s, &speaker.id)
                .is_some_and(|f| text(&f, "state") == "confirmed" && text(&f, "version") == "2.0.0")
        })
    });
    assert_eq!(offers(&mut server), 2, "one offer per speaker of the board");
    let untouched = firmware_of(&done, &other.id).unwrap();
    assert_eq!(text(&untouched, "version"), "1.0.0");
    assert_eq!(text(&untouched, "state"), "idle");
    assert!(other_boot
        .states()
        .iter()
        .all(|l| l.starts_with("ota state=idle ")));
    assert_eq!(other.flash_bytes(), other_flash);
    // Everybody runs it now: `all` has nobody left to reach.
    refused(
        &server,
        r#"{"v":2,"t":"firmware_install","all":true,"image":"good"}"#,
        "all",
        "nothing-to-install",
    );
    drop(boots);
}

/// An image built for another board is refused by the server, by name, and
/// so is every other install that must not start: each offers nothing.
#[test]
fn a_wrong_board_image_and_every_other_refusal_is_named_and_offers_nothing() {
    let rig = Rig::new("refusals", 5);
    rig.stage("for-another", "2.0.0", OTHER_BOARD, 90_000);
    rig.stage("same", "1.0.0", BOARD, 90_000);
    let speaker = rig.speaker(1, BOARD);
    let gone = rig.speaker(2, BOARD);
    let mut server = rig.server();
    let boot = speaker.boot(&rig.listen, &[]);
    reported(&server, &speaker.id);
    // A second speaker that was here and left.
    let gone_boot = gone.boot(&rig.listen, &[]);
    reported(&server, &gone.id);
    drop(gone_boot);
    until(&server, "the speaker that left is absent", |s| {
        listed(s, "speakers", "id", &gone.id).is_some_and(|v| !flag(&v, "present"))
    });
    let flash_before = speaker.flash_bytes();

    refused(
        &server,
        &install(&speaker.id, "for-another"),
        "speaker",
        "wrong-board",
    );
    refused(
        &server,
        &install(&speaker.id, "same"),
        "speaker",
        "already-running",
    );
    refused(
        &server,
        &install(&gone.id, "same"),
        "speaker",
        "speaker-absent",
    );
    refused(
        &server,
        &install(&speaker.id, "nothing-staged"),
        "image",
        "unknown-image",
    );
    let (status, answer) = server.command(&install("chorus-ffffffffffff", "same"));
    assert!(
        status.contains("400") && answer.contains(r#""field":"speaker""#),
        "{}",
        answer
    );
    refused(
        &server,
        &format!(
            r#"{{"v":2,"t":"firmware_cancel","speaker":"{}"}}"#,
            speaker.id
        ),
        "speaker",
        "nothing-to-cancel",
    );
    refused(
        &server,
        r#"{"v":2,"t":"firmware_install","all":true,"image":"same"}"#,
        "all",
        "nothing-to-install",
    );
    // A v2 command at version 1 is not a command of that version.
    let (status, answer) =
        server.command(&install(&speaker.id, "same").replace("\"v\":2", "\"v\":1"));
    assert!(status.contains("400"), "{} {}", status, answer);

    assert_eq!(offers(&mut server), 0, "a refusal offers nothing");
    assert!(boot
        .states()
        .iter()
        .all(|l| l.starts_with("ota state=idle ")));
    assert_eq!(speaker.flash_bytes(), flash_before);

    // The same version again is an install when the owner says `force`.
    let answer = server.applied(&format!(
        r#"{{"v":2,"t":"firmware_install","speaker":"{}","image":"same","force":true}}"#,
        speaker.id
    ));
    let fw = firmware_of(&json::parse(&answer).unwrap(), &speaker.id).unwrap();
    assert_eq!(text(&fw, "state"), "requested");
    let (code, lines) = boot.finish();
    assert_eq!(code, 0, "{}", lines.join("\n"));
    assert!(lines.iter().any(|l| l.starts_with("ota state=verified ")));
    assert_eq!(offers(&mut server), 1);
}

/// An install in progress when the server stops is NOT resumed after the
/// restart: the new server process sends no offer, a speaker found holding
/// part of the old transfer is told to abandon it and shown `interrupted`,
/// and installing takes a new `firmware_install`.
///
/// The endpoint is held still (SIGSTOP) while the install is commanded and
/// the server is killed, so nothing can complete the transfer: the install
/// is in progress, by construction, when the server stops. Whether the offer
/// itself reached the endpoint's socket before the kill depends on how much
/// of the session's audio was queued ahead of it, so both cases are graded
/// by the same rule (nothing resumes, nothing activates) and the one that
/// happened is printed; the orphan-cancel path is also graded alone, in
/// `firmware::tests::a_transfer_ends_with_its_session_and_an_orphan_is_cancelled_not_resumed`.
#[test]
fn an_install_in_progress_when_the_server_stops_is_not_resumed_after_restart() {
    let rig = Rig::new("not-resumed", 6);
    rig.stage("good", "2.0.0", BOARD, 600_000);
    let speaker = rig.speaker(1, BOARD);
    let mut server = rig.server();
    let boot = speaker.boot(&rig.listen, &[]);
    reported(&server, &speaker.id);

    boot.signal("STOP");
    let answer = server.applied(&install(&speaker.id, "good"));
    let fw = firmware_of(&json::parse(&answer).unwrap(), &speaker.id).unwrap();
    assert_eq!(text(&fw, "state"), "requested");
    said(&mut server, &["firmware offer ", &speaker.id]);
    // The server stops with the install in progress.
    drop(server);

    let mut server = rig.server();
    boot.signal("CONT");
    // The endpoint takes whatever reached it before the server went away,
    // finds the connection gone, and rejoins the new server.
    reported(&server, &speaker.id);
    let took_the_offer = boot
        .lines()
        .iter()
        .any(|l| l.starts_with("ota state=receiving "));
    if took_the_offer {
        // It rejoined mid-transfer: told to abandon it, shown interrupted.
        said(
            &mut server,
            &["firmware cancel ", &speaker.id, "reason=not-resumed"],
        );
        let now = until(&server, "the state says interrupted", |s| {
            firmware_of(s, &speaker.id).is_some_and(|f| text(&f, "state") == "interrupted")
        });
        let fw = firmware_of(&now, &speaker.id).unwrap();
        assert_eq!(text(&fw, "reason"), "not_resumed");
    }
    // Either way: idle again on the endpoint, and nothing in progress here.
    let deadline = Instant::now() + STEP;
    while boot
        .states()
        .last()
        .is_none_or(|l| !l.starts_with("ota state=idle "))
    {
        assert!(
            Instant::now() < deadline,
            "the endpoint never came back to idle: {:?}",
            boot.states()
        );
        thread::sleep(Duration::from_millis(20));
    }
    for _ in 0..20 {
        let fw = firmware_of(&state(&server), &speaker.id).unwrap();
        let shown = text(&fw, "state");
        assert!(
            shown == "interrupted" || shown == "idle",
            "nothing is in progress after the restart: {}",
            json::write(&fw)
        );
        assert_eq!(text(&fw, "version"), "1.0.0");
        assert!(flag(&fw, "update_available"));
    }
    assert_eq!(
        offers(&mut server),
        0,
        "the restarted server sent no offer: nothing was resumed"
    );
    assert!(
        !boot
            .lines()
            .iter()
            .any(|l| l.starts_with("ota state=verified ") || l == "ota reboot"),
        "nothing was activated: {:?}",
        boot.states()
    );
    // (Read again: the line may have reached this process's pipe reader
    // only after the check above.)
    let took_the_offer = took_the_offer
        || boot
            .lines()
            .iter()
            .any(|l| l.starts_with("ota state=receiving "));
    println!(
        "the endpoint {} the offer before the server stopped",
        if took_the_offer {
            "had taken"
        } else {
            "had not taken"
        }
    );
    // The flash still boots what it booted.
    drop(boot);
    let again = speaker.boot(&rig.listen, &[]);
    again.wait_for("ota boot slot=0 image=valid version=1.0.0");
    drop(again);

    // And the install the owner asks for now is a new one, and completes.
    let boot = speaker.boot(&rig.listen, &[]);
    until(&server, "the speaker is back", |s| {
        listed(s, "speakers", "id", &speaker.id).is_some_and(|v| flag(&v, "present"))
    });
    boot.wait_for("ota state=idle ");
    // Its opening status has reached the server once it is listed as taking
    // updates again; a refusal until then is `speaker-absent`, by name.
    let deadline = Instant::now() + STEP;
    loop {
        let (status, answer) = server.command(&install(&speaker.id, "good"));
        if status.contains("200") {
            break;
        }
        assert!(
            answer.contains("speaker-absent") && Instant::now() < deadline,
            "{} {}",
            status,
            answer
        );
        thread::sleep(Duration::from_millis(50));
    }
    let (code, lines) = boot.finish();
    assert_eq!(code, 0, "{}", lines.join("\n"));
    assert!(lines.iter().any(|l| l.starts_with("ota state=verified ")));
    assert_eq!(offers(&mut server), 1);
}
