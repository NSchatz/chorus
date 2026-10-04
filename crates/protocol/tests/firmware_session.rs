//! The firmware update on the wire, end to end against the real C endpoint.
//!
//! The endpoint is `firmware/build/chorus-endpoint-session`, the host build of
//! the session supervisor and the update unit the board runs
//! (`firmware/src/session.c`, `firmware/src/ota.c`), over a two-slot fake flash
//! kept in a file (`firmware/tests/fake_flash.c`, ESP-IDF v6.1's bootloader
//! rule), so the test "reboots" it by starting the binary again on the same
//! file. The server is this test: the Rust session layer and the Rust
//! `firmware_offer` / `firmware_chunk` / `firmware_status` codec, on loopback.
//! So the two implementations of the three messages meet on a real socket,
//! inside real encrypted records, and not only in the shared vectors.
//!
//! It is also the reference for a sender (`docs/protocol.md`, "Firmware
//! update"): one offer, chunks in order inside a window of unacknowledged
//! ones, `firmware_status` as the acknowledgement and the resume point.
//!
//! Loopback only; nothing here touches a device (program section 0.7).

use std::fs::{self, File};
use std::io::{self, BufRead, BufReader};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use chorus_protocol::v2::adoption::Verdict;
use chorus_protocol::v2::noise::Keypair;
use chorus_protocol::v2::session::{accept, Identity, SecureReader, SecureWriter};
use chorus_protocol::v2::{
    features, FirmwareChunk, FirmwareOffer, FirmwareReason, FirmwareState, FirmwareStatus, Message,
    OutputDelay, FIRMWARE_ACK_EVERY, FIRMWARE_WINDOW_CHUNKS,
};

const BOARD: &str = "test-board";
const CHUNK_BYTES: u16 = 1024;
/// How long any one step may take before the test says the endpoint hung.
const STEP: Duration = Duration::from_secs(20);

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository root")
}

/// The C endpoint binary, built (or found fresh) by the firmware makefile,
/// once per test process and under a file lock so two test processes do not
/// run the same make at once.
fn endpoint_binary() -> &'static PathBuf {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY.get_or_init(|| {
        let root = repo_root();
        let build = root.join("firmware/build");
        fs::create_dir_all(&build).expect("firmware/build can be made");
        let binary = build.join("chorus-endpoint-session");
        let lock = File::create(build.join(".chorus-endpoint-session.lock")).expect("a lock file");
        lock.lock().expect("the build lock");
        let made = Command::new("make")
            .arg("-f")
            .arg(root.join("firmware/Makefile"))
            .arg(&binary)
            .current_dir(&root)
            .output()
            .expect("make runs (the endpoint's host build needs make and a C compiler)");
        assert!(
            made.status.success(),
            "the endpoint's host build failed:\n{}\n{}",
            String::from_utf8_lossy(&made.stdout)
                .lines()
                .rev()
                .take(40)
                .collect::<Vec<_>>()
                .join("\n"),
            String::from_utf8_lossy(&made.stderr)
        );
        binary
    })
}

/// One run of the endpoint binary: one boot.
struct Boot {
    child: Child,
    stdout: Arc<Mutex<Vec<String>>>,
}

impl Boot {
    fn lines(&self) -> Vec<String> {
        self.stdout.lock().unwrap().clone()
    }

    /// Wait for the process to end; its exit code and every stdout line.
    fn finish(mut self) -> (i32, Vec<String>) {
        let deadline = Instant::now() + STEP;
        loop {
            if let Some(status) = self.child.try_wait().expect("wait") {
                // The reader thread ends at EOF, just after the exit.
                thread::sleep(Duration::from_millis(100));
                return (status.code().unwrap_or(-1), self.lines());
            }
            if Instant::now() > deadline {
                let _ = self.child.kill();
                panic!(
                    "the endpoint did not exit in {:?}; stdout:\n{}",
                    STEP,
                    self.lines().join("\n")
                );
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn stop(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Rig {
    dir: PathBuf,
    listener: TcpListener,
    identity: Identity,
    next_ephemeral: u8,
}

/// A session with the endpoint, from the server's side.
struct Session {
    reader: SecureReader<TcpStream>,
    writer: SecureWriter<TcpStream>,
    /// The socket itself, for its read timeout.
    stream: TcpStream,
    features: u8,
}

impl Rig {
    fn new(name: &str) -> Rig {
        let dir = std::env::temp_dir().join(format!(
            "chorus-firmware-session-{}-{}",
            name,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        listener.set_nonblocking(true).unwrap();
        Rig {
            dir,
            listener,
            identity: Identity {
                id: "firmware-test-server".to_string(),
                keypair: Keypair::from_secret([0x51; 32]),
            },
            next_ephemeral: 1,
        }
    }

    fn flash(&self) -> PathBuf {
        self.dir.join("flash.bin")
    }

    /// Wait until the endpoint has cleared its rollback note, the file the
    /// host build keeps beside the flash as `<flash>.note`.
    fn wait_for_note_cleared(&self) {
        let mut note = self.flash().into_os_string();
        note.push(".note");
        let note = PathBuf::from(note);
        let deadline = Instant::now() + STEP;
        while note.exists() {
            assert!(
                Instant::now() < deadline,
                "the endpoint reported its rollback and never cleared {}",
                note.display()
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    /// An application image of `version` that the endpoint's flash accepts.
    fn image(&self, version: &str, bytes: usize) -> Vec<u8> {
        let path = self.dir.join(format!("image-{}.bin", version));
        let made = Command::new(endpoint_binary())
            .args(["--ota-make-image"])
            .arg(&path)
            .args(["--ota-version", version, "--ota-image-bytes"])
            .arg(bytes.to_string())
            .output()
            .expect("the endpoint binary runs");
        assert!(made.status.success(), "{:?}", made);
        let image = fs::read(&path).unwrap();
        assert_eq!(image[0], 0xE9, "an ESP-IDF application image starts 0xE9");
        image
    }

    /// Start the endpoint: one boot on the rig's flash file.
    fn boot(&self, extra: &[&str]) -> Boot {
        let port = self.listener.local_addr().unwrap().port();
        let mut child = Command::new(endpoint_binary())
            .arg("--server")
            .arg(format!("127.0.0.1:{}", port))
            .args(["--run-seconds", "40", "--first-backoff-ms", "50"])
            .args(["--endpoint-id", "firmware-test-endpoint"])
            .arg("--key")
            .arg(self.dir.join("endpoint.key"))
            .arg("--server-pins")
            .arg(self.dir.join("server-pins"))
            .arg("--ota-flash")
            .arg(self.flash())
            .args(["--ota-version", "1.0.0", "--ota-board", BOARD])
            .args(["--ota-confirm-seconds", "3"])
            .args(extra)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
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
        Boot { child, stdout }
    }

    /// Accept the endpoint's connection and run the handshake.
    fn accept(&mut self) -> Session {
        let deadline = Instant::now() + STEP;
        let mut stream = loop {
            match self.listener.accept() {
                Ok((stream, _)) => break stream,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "the endpoint never connected");
                    thread::sleep(Duration::from_millis(10));
                }
                Err(e) => panic!("accept: {}", e),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream.set_read_timeout(Some(STEP)).unwrap();
        stream.set_nodelay(true).unwrap();
        let ephemeral = Keypair::from_secret([0x60 + self.next_ephemeral; 32]);
        self.next_ephemeral += 1;
        let est = accept(&mut stream, &self.identity, ephemeral, |_, _| {
            Verdict::Known
        })
        .expect("the handshake completes");
        assert_eq!(est.peer_id, "firmware-test-endpoint");
        let reader = SecureReader::new(stream.try_clone().unwrap(), est.opener);
        let writer = SecureWriter::new(stream.try_clone().unwrap(), est.sealer);
        let mut session = Session {
            reader,
            writer,
            stream,
            features: 0,
        };
        // The greeting: hello, capabilities, then the opening firmware_status
        // (read by the caller).
        match session.reader.next_message().expect("hello") {
            Message::Hello(_) => {}
            other => panic!("the first message is hello, got {:?}", other),
        }
        match session.reader.next_message().expect("capabilities") {
            Message::Capabilities(c) => session.features = c.features,
            other => panic!("the second message is capabilities, got {:?}", other),
        }
        session
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

impl Session {
    /// The next `firmware_status`, stepping over telemetry and the rest.
    fn status(&mut self) -> FirmwareStatus {
        loop {
            match self.reader.next_message() {
                Ok(Message::FirmwareStatus(s)) => return s,
                Ok(_) => continue,
                Err(e) => panic!("no firmware_status arrived: {}", e),
            }
        }
    }

    /// Whether the endpoint closed the connection (it rebooted) without
    /// another `firmware_status` first.
    fn closed_without_another_status(&mut self) -> bool {
        loop {
            match self.reader.next_message() {
                Ok(Message::FirmwareStatus(_)) => return false,
                Ok(_) => continue,
                Err(_) => return true,
            }
        }
    }

    /// The reference sender: one offer, then chunks in order, never more
    /// than the window beyond the last acknowledged byte. Returns the status
    /// that ended it (verified or refused) and how many statuses said
    /// `receiving` on the way.
    fn install(&mut self, image: &[u8], transfer: u32, version: &str) -> (FirmwareStatus, usize) {
        self.writer
            .send(&Message::FirmwareOffer(FirmwareOffer {
                transfer,
                size: image.len() as u32,
                sha256: FirmwareOffer::digest_of(image),
                chunk_bytes: CHUNK_BYTES,
                version: version.to_string(),
                board: BOARD.to_string(),
            }))
            .unwrap();
        let first = self.status();
        if first.state != FirmwareState::Receiving {
            return (first, 0);
        }
        let window = (FIRMWARE_WINDOW_CHUNKS * CHUNK_BYTES as u32) as usize;
        let mut acked = first.received as usize;
        let mut sent = acked;
        let mut acks = 1;
        loop {
            while sent < image.len() && sent - acked < window {
                let end = (sent + CHUNK_BYTES as usize).min(image.len());
                self.writer
                    .send(&Message::FirmwareChunk(FirmwareChunk {
                        transfer,
                        offset: sent as u32,
                        data: image[sent..end].to_vec(),
                    }))
                    .unwrap();
                sent = end;
            }
            let status = self.status();
            match status.state {
                FirmwareState::Receiving => {
                    acks += 1;
                    acked = status.received as usize;
                    if status.reason == FirmwareReason::BadOffset {
                        sent = acked;
                    }
                }
                _ => return (status, acks),
            }
        }
    }
}

fn has_line(lines: &[String], wanted: &str) -> bool {
    lines.iter().any(|l| l.starts_with(wanted))
}

#[test]
fn a_good_image_installs_and_confirms_and_a_bad_one_is_rolled_back_over_a_real_session() {
    let mut rig = Rig::new("install");
    let good = rig.image("2.0.0", 150_000);
    let bad = rig.image("3.0.0", 90_000);

    // Boot 1: a freshly flashed endpoint, version 1.0.0 in slot 0.
    let boot = rig.boot(&[]);
    let mut session = rig.accept();
    assert_ne!(
        session.features & features::OTA,
        0,
        "an endpoint with an update unit sets capabilities.features ota"
    );
    let opening = session.status();
    assert_eq!(
        (opening.state, opening.transfer, opening.slot),
        (FirmwareState::Idle, 0, 0)
    );
    assert_eq!(
        (opening.version.as_str(), opening.board.as_str()),
        ("1.0.0", BOARD)
    );
    let before = fs::read(rig.flash()).unwrap();

    // A wrong-board offer is refused by name and writes nothing.
    session
        .writer
        .send(&Message::FirmwareOffer(FirmwareOffer {
            transfer: 5,
            size: good.len() as u32,
            sha256: FirmwareOffer::digest_of(&good),
            chunk_bytes: CHUNK_BYTES,
            version: "2.0.0".to_string(),
            board: "another-board".to_string(),
        }))
        .unwrap();
    let refused = session.status();
    assert_eq!(
        (refused.state, refused.reason, refused.transfer),
        (FirmwareState::Refused, FirmwareReason::WrongBoard, 5)
    );
    assert_eq!(
        fs::read(rig.flash()).unwrap(),
        before,
        "a refused offer leaves the flash byte for byte as it was"
    );

    // A corrupted image (one byte differs from the digest's) is written,
    // fails its digest, and is never selected.
    let mut corrupt = good.clone();
    corrupt[70_000] ^= 0x01;
    session
        .writer
        .send(&Message::FirmwareOffer(FirmwareOffer {
            transfer: 6,
            size: good.len() as u32,
            sha256: FirmwareOffer::digest_of(&good),
            chunk_bytes: CHUNK_BYTES,
            version: "2.0.0".to_string(),
            board: BOARD.to_string(),
        }))
        .unwrap();
    assert_eq!(session.status().state, FirmwareState::Receiving);
    for (i, piece) in corrupt.chunks(CHUNK_BYTES as usize).enumerate() {
        session
            .writer
            .send(&Message::FirmwareChunk(FirmwareChunk {
                transfer: 6,
                offset: (i * CHUNK_BYTES as usize) as u32,
                data: piece.to_vec(),
            }))
            .unwrap();
    }
    let digest = loop {
        let s = session.status();
        if s.state != FirmwareState::Receiving {
            break s;
        }
    };
    assert_eq!(
        (digest.state, digest.reason),
        (FirmwareState::Refused, FirmwareReason::BadDigest),
        "a bad digest is refused before activation"
    );

    // The good image: offered, sent in a window, acknowledged, verified.
    let (verified, acks) = session.install(&good, 7, "2.0.0");
    assert_eq!(
        (verified.state, verified.reason, verified.transfer),
        (FirmwareState::Verified, FirmwareReason::None, 7)
    );
    assert_eq!(verified.received as usize, good.len());
    assert_eq!(verified.image_version, "2.0.0");
    let chunks = good.len().div_ceil(CHUNK_BYTES as usize);
    assert_eq!(
        acks,
        1 + (chunks - 1) / FIRMWARE_ACK_EVERY as usize,
        "one status for the offer and one per {} chunks",
        FIRMWARE_ACK_EVERY
    );
    assert!(
        session.closed_without_another_status(),
        "after verified the endpoint reboots: the connection closes"
    );
    let (code, lines) = boot.finish();
    assert_eq!(code, 0, "{}", lines.join("\n"));
    assert!(has_line(
        &lines,
        "ota boot slot=0 image=valid version=1.0.0"
    ));
    assert!(has_line(
        &lines,
        "ota state=idle transfer=0 received=0 slot=0 version=1.0.0"
    ));
    assert!(has_line(
        &lines,
        "ota state=refused transfer=5 received=0 slot=0 version=1.0.0 reason=wrong_board"
    ));
    assert!(has_line(
        &lines,
        "ota state=refused transfer=6 received=150336 slot=0 version=1.0.0 reason=bad_digest"
    ));
    assert!(has_line(
        &lines,
        "ota state=verified transfer=7 received=150336 slot=0 version=1.0.0 reason=none image=2.0.0"
    ));
    assert!(has_line(&lines, "ota reboot"), "{}", lines.join("\n"));

    // Boot 2: the new image on trial. It confirms once the server's first
    // record opens.
    let boot = rig.boot(&[]);
    let mut session = rig.accept();
    let trial = session.status();
    assert_eq!(
        (
            trial.state,
            trial.transfer,
            trial.slot,
            trial.version.as_str()
        ),
        (FirmwareState::PendingVerify, 7, 1, "2.0.0")
    );
    session
        .writer
        .send(&Message::OutputDelay(OutputDelay { delay_ns: 0 }))
        .unwrap();
    let confirmed = session.status();
    assert_eq!(
        (confirmed.state, confirmed.slot, confirmed.version.as_str()),
        (FirmwareState::Confirmed, 1, "2.0.0"),
        "the server's first record is the self-test passing: confirmed"
    );

    // The bad image: installed like any other.
    let (verified, _) = session.install(&bad, 9, "3.0.0");
    assert_eq!(verified.state, FirmwareState::Verified);
    assert!(session.closed_without_another_status());
    let (code, lines) = boot.finish();
    assert_eq!(code, 0, "{}", lines.join("\n"));
    assert!(has_line(
        &lines,
        "ota boot slot=1 image=pending-verify version=2.0.0"
    ));
    assert!(has_line(
        &lines,
        "ota state=confirmed transfer=7 received=0 slot=1 version=2.0.0"
    ));

    // Boot 3: the bad image never confirms. Its trial (3 s here) ends in the
    // endpoint marking itself invalid and rebooting.
    let boot = rig.boot(&["--ota-never-confirm"]);
    let mut session = rig.accept();
    let trial = session.status();
    assert_eq!(
        (
            trial.state,
            trial.transfer,
            trial.slot,
            trial.version.as_str()
        ),
        (FirmwareState::PendingVerify, 9, 0, "3.0.0")
    );
    // An offer during the trial is refused by name: an image that has not
    // proved itself does not get to replace its only fallback.
    session
        .writer
        .send(&Message::FirmwareOffer(FirmwareOffer {
            transfer: 10,
            size: good.len() as u32,
            sha256: FirmwareOffer::digest_of(&good),
            chunk_bytes: CHUNK_BYTES,
            version: "2.0.0".to_string(),
            board: BOARD.to_string(),
        }))
        .unwrap();
    let not_yet = session.status();
    assert_eq!(
        (not_yet.state, not_yet.reason, not_yet.transfer),
        (FirmwareState::Refused, FirmwareReason::NotConfirmed, 10)
    );
    let giving_up = session.status();
    assert_eq!(
        (giving_up.state, giving_up.reason),
        (FirmwareState::PendingVerify, FirmwareReason::NotConfirmed),
        "at its deadline the image says it did not confirm, and reboots"
    );
    let (code, lines) = boot.finish();
    assert_eq!(code, 0, "{}", lines.join("\n"));
    assert!(has_line(&lines, "ota reboot"), "{}", lines.join("\n"));

    // Boot 4: the bootloader went back to 2.0.0, and the first session says so.
    let boot = rig.boot(&[]);
    let mut session = rig.accept();
    let rolled = session.status();
    assert_eq!(
        (rolled.state, rolled.reason, rolled.transfer, rolled.slot),
        (
            FirmwareState::RolledBack,
            FirmwareReason::NotConfirmed,
            9,
            1
        )
    );
    assert_eq!(
        (rolled.version.as_str(), rolled.image_version.as_str()),
        ("2.0.0", "3.0.0"),
        "rolled back to the previous version, naming the image that was tried"
    );
    let lines = boot.lines();
    assert!(has_line(
        &lines,
        "ota boot slot=1 image=valid version=2.0.0"
    ));
    assert!(has_line(
        &lines,
        "ota state=rolled_back transfer=9 received=0 slot=1 version=2.0.0 reason=not_confirmed image=3.0.0"
    ));
    // The endpoint clears its note once the status is on the wire, a moment
    // after this side has read it. A power cut in between is allowed to
    // report the rollback twice (firmware/src/ota.c, note_clear); this boot
    // is not cut short, so it is stopped only once the note is gone.
    rig.wait_for_note_cleared();
    boot.stop();

    // Boot 5: reported once, the next boot is quiet.
    let boot = rig.boot(&[]);
    let mut session = rig.accept();
    let quiet = session.status();
    assert_eq!(
        (quiet.state, quiet.slot, quiet.version.as_str()),
        (FirmwareState::Idle, 1, "2.0.0")
    );
    boot.stop();
}

#[test]
fn a_transfer_survives_the_session_dropping_and_a_cancel_abandons_it() {
    let mut rig = Rig::new("resume");
    let image = rig.image("2.0.0", 120_000);
    let boot = rig.boot(&[]);
    let mut session = rig.accept();
    assert_eq!(session.status().state, FirmwareState::Idle);

    let offer = FirmwareOffer {
        transfer: 21,
        size: image.len() as u32,
        sha256: FirmwareOffer::digest_of(&image),
        chunk_bytes: CHUNK_BYTES,
        version: "2.0.0".to_string(),
        board: BOARD.to_string(),
    };
    session
        .writer
        .send(&Message::FirmwareOffer(offer.clone()))
        .unwrap();
    assert_eq!(session.status().state, FirmwareState::Receiving);
    // Forty chunks, then the server goes away mid-download.
    for i in 0..40usize {
        session
            .writer
            .send(&Message::FirmwareChunk(FirmwareChunk {
                transfer: 21,
                offset: (i * CHUNK_BYTES as usize) as u32,
                data: image[i * CHUNK_BYTES as usize..(i + 1) * CHUNK_BYTES as usize].to_vec(),
            }))
            .unwrap();
    }
    // Two acknowledgements (chunks 16 and 32) say the forty are being taken.
    assert_eq!(session.status().received, 16 * CHUNK_BYTES as u32);
    assert_eq!(session.status().received, 32 * CHUNK_BYTES as u32);
    drop(session);

    // The endpoint rejoins and says where it is.
    let mut session = rig.accept();
    let resumed = session.status();
    assert_eq!(
        (resumed.state, resumed.transfer, resumed.received),
        (FirmwareState::Receiving, 21, 40 * CHUNK_BYTES as u32),
        "the transfer survives the session: the opening status is the resume point"
    );
    // A different offer is refused while that one is in progress.
    let mut other = offer.clone();
    other.transfer = 22;
    other.sha256[0] ^= 1;
    session.writer.send(&Message::FirmwareOffer(other)).unwrap();
    let busy = session.status();
    assert_eq!(
        (busy.state, busy.reason, busy.transfer),
        (FirmwareState::Refused, FirmwareReason::Busy, 22)
    );
    // The cancel abandons it; nothing was activated.
    session
        .writer
        .send(&Message::FirmwareOffer(FirmwareOffer::cancel()))
        .unwrap();
    let idle = session.status();
    assert_eq!(
        (idle.state, idle.transfer, idle.slot, idle.version.as_str()),
        (FirmwareState::Idle, 0, 0, "1.0.0")
    );
    // And the same offer again now starts from byte 0 and installs.
    let (verified, _) = session.install(&image, 21, "2.0.0");
    assert_eq!(verified.state, FirmwareState::Verified);
    let (code, lines) = boot.finish();
    assert_eq!(code, 0, "{}", lines.join("\n"));
    assert!(has_line(&lines, "ota reboot"), "{}", lines.join("\n"));
}

#[test]
fn an_endpoint_without_an_update_unit_does_not_offer_the_feature() {
    let mut rig = Rig::new("plain");
    let port = rig.listener.local_addr().unwrap().port();
    let mut child = Command::new(endpoint_binary())
        .arg("--server")
        .arg(format!("127.0.0.1:{}", port))
        .args(["--run-seconds", "10", "--first-backoff-ms", "50"])
        .args(["--endpoint-id", "firmware-test-endpoint"])
        .stdout(Stdio::null())
        .spawn()
        .expect("the endpoint binary starts");
    let mut session = rig.accept();
    assert_eq!(
        session.features & features::OTA,
        0,
        "without --ota-flash there is no update unit and no ota feature"
    );
    // An offer anyway is stepped over: no status ever answers it.
    session
        .writer
        .send(&Message::FirmwareOffer(FirmwareOffer {
            transfer: 1,
            size: 4096,
            sha256: [7; 32],
            chunk_bytes: 1024,
            version: "9".to_string(),
            board: BOARD.to_string(),
        }))
        .unwrap();
    let deadline = Instant::now() + Duration::from_millis(1500);
    session
        .stream
        .set_read_timeout(Some(Duration::from_millis(300)))
        .unwrap();
    let mut statuses = 0;
    while Instant::now() < deadline {
        if let Ok(Message::FirmwareStatus(_)) = session.reader.next_message() {
            statuses += 1;
        }
    }
    assert_eq!(statuses, 0);
    let _ = child.kill();
    let _ = child.wait();
}
