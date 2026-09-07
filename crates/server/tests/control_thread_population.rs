//! AC-12, the hazard the objective names, graded against `/proc`.
//!
//! "WHEN the server runs with the control plane enabled THE SYSTEM SHALL have
//! created every thread it will ever run before it takes its scheduling report,
//! SHALL account for every one of them in that report, and SHALL exit non-zero
//! rather than serve when it cannot, so a subscriber cannot cause a thread to
//! exist that the host contract never saw."
//!
//! This is `crates/server/tests/thread_shape.rs`'s question asked about the
//! control plane, and it is asked the same way: against the kernel's own list,
//! from outside the process, on the REAL binary, while subscribers come and go.
//! `crates/server/src/main.rs` states why it matters - `std::thread::spawn`
//! inherits the creating thread's scheduling policy and `deploy/run-server.sh`
//! runs this binary with `--ulimit rtprio=20` - and a control plane that grew a
//! thread per subscriber would be putting real-time threads on a host that also
//! runs other things, none of which the report had ever seen.
//!
//! The policies themselves cannot be checked here: this container's granted
//! rtprio ceiling is zero, so every run below is `--allow-non-realtime` and no
//! thread in any of them is real-time. What is checked is the property that
//! holds either way, which is the one a subscriber could break.

use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReportedThread {
    role: String,
    tid: u32,
}

fn pump(stdout: ChildStdout) -> (thread::JoinHandle<()>, mpsc::Receiver<String>) {
    let (tx, rx) = mpsc::channel::<String>();
    let handle = thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                return;
            }
        }
    });
    (handle, rx)
}

/// Start the server and read its output until it says it is listening for
/// audio, which is after the scheduling report has been printed.
fn start(extra: &[String]) -> (Server, u32, mpsc::Receiver<String>, Vec<String>) {
    let mut args = vec![
        "--source".to_string(),
        "tone".to_string(),
        "--tone-ms".to_string(),
        "30000".to_string(),
        "--allow-non-realtime".to_string(),
        "--allow-unlocked-memory".to_string(),
    ];
    args.extend(extra.iter().cloned());

    let mut child = Command::new(env!("CARGO_BIN_EXE_chorus-server"))
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the server binary runs");
    let pid = child.id();
    let stdout = child.stdout.take().expect("stdout was piped");
    let server = Server(child);
    let (lines, rest) = pump(stdout);

    let mut startup = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match rest.recv_timeout(Duration::from_millis(500)) {
            Ok(line) => {
                // The AUDIO socket, which is bound after the scheduling report.
                // Matching "listening on=" alone would stop at the control
                // channel's own line, which is printed before any thread
                // exists.
                let listening = line.starts_with("chorus-server: listening on=");
                startup.push(line);
                if listening {
                    return (server, pid, rest, startup);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    drop(lines);
    panic!("the server never reported a bound socket; it said: {:?}", startup);
}

fn reported(lines: &[String]) -> Vec<ReportedThread> {
    lines
        .iter()
        .filter_map(|line| {
            let at = line.find("thread role=")?;
            let rest = &line[at + "thread role=".len()..];
            let (role, rest) = rest.split_once(" tid=")?;
            let tid: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            Some(ReportedThread {
                role: role.to_string(),
                tid: tid.parse().ok()?,
            })
        })
        .collect()
}

fn kernel_threads(pid: u32) -> BTreeSet<u32> {
    std::fs::read_dir(format!("/proc/{}/task", pid))
        .expect("the server process is alive and /proc is mounted")
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().to_string_lossy().parse().ok())
        .collect()
}

/// The control address the server printed, with its ephemeral port resolved.
fn control_address(startup: &[String]) -> String {
    startup
        .iter()
        .find_map(|line| {
            let at = line.find("control listening on=")?;
            let rest = &line[at + "control listening on=".len()..];
            Some(rest.split_whitespace().next()?.to_string())
        })
        .unwrap_or_else(|| panic!("the server never said where its control channel is: {:?}", startup))
}

/// A held-open event-stream subscriber, on a real socket.
///
/// The socket is held and never read again: what matters is that it stays
/// open, because a subscriber that has gone is not the one this test is about.
struct Subscriber(#[allow(dead_code)] TcpStream);

impl Subscriber {
    fn open(address: &str) -> Subscriber {
        let mut socket = TcpStream::connect(address).expect("the control channel is listening");
        socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        write!(
            socket,
            "GET /api/events HTTP/1.1\r\nHost: chorus\r\nConnection: close\r\n\r\n"
        )
        .expect("the request goes up");
        socket.flush().unwrap();
        // Read the opening state, so this subscriber is definitely attached
        // before anything is counted.
        let mut scratch = [0u8; 4_096];
        let read = socket.read(&mut scratch).expect("the state comes back");
        assert!(read > 0, "the subscriber received nothing at all");
        Subscriber(socket)
    }
}

fn a_command(address: &str, body: &str) -> String {
    let mut socket = TcpStream::connect(address).expect("the control channel is listening");
    socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    write!(
        socket,
        "POST /api/command HTTP/1.1\r\nHost: chorus\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
    .expect("the request goes up");
    socket.flush().unwrap();
    let mut response = String::new();
    let _ = socket.read_to_string(&mut response);
    response
}

#[test]
fn the_control_plane_creates_every_thread_it_will_run_before_the_report_is_taken() {
    let audio = free_port();
    let control = free_port();
    let (server, pid, _rest, startup) = start(&[
        "--listen".to_string(),
        format!("127.0.0.1:{}", audio),
        "--max-clients".to_string(),
        "2".to_string(),
        "--control-listen".to_string(),
        format!("127.0.0.1:{}", control),
        "--control-workers".to_string(),
        "4".to_string(),
        "--zone".to_string(),
        "kitchen".to_string(),
        "--zone".to_string(),
        "study".to_string(),
    ]);

    let rows = reported(&startup);
    assert!(!rows.is_empty(), "no thread rows at all: {:?}", startup);
    assert!(
        !rows.iter().any(|r| r.role == "unregistered"),
        "a thread nobody declared is running: {:?}",
        rows
    );
    let roles: BTreeSet<String> = rows.iter().map(|r| r.role.clone()).collect();
    for wanted in ["supervisor", "audio", "acceptor", "control-acceptor"] {
        assert!(roles.contains(wanted), "no {} row in {:?}", wanted, roles);
    }
    for index in 0..4 {
        assert!(
            roles.contains(&format!("control-worker-{}", index)),
            "no control-worker-{} row in {:?}",
            index,
            roles
        );
    }

    // 1 supervisor + 1 audio + 1 acceptor + 2 per client slot + 1 control
    // acceptor + 1 per control worker.
    let expected = 1 + 1 + 1 + 2 * 2 + 1 + 4;
    let before = kernel_threads(pid);
    assert_eq!(
        before.len(),
        expected,
        "the process runs {} threads and the shape says {}: {:?}",
        before.len(),
        expected,
        rows
    );
    let declared: BTreeSet<u32> = rows.iter().map(|r| r.tid).collect();
    assert_eq!(
        declared, before,
        "the report names {:?} and the kernel is running {:?}",
        rows, before
    );

    // Now put the control plane through what a subscriber does: attach, hold
    // an event stream open, issue commands, and leave. Not one of those may
    // change the answer above.
    let address = control_address(&startup);
    let subscribers: Vec<Subscriber> = (0..3).map(|_| Subscriber::open(&address)).collect();
    for zone in ["kitchen", "study"] {
        let response = a_command(
            &address,
            &format!(r#"{{"v":1,"t":"volume","zone":"{}","volume":0.500}}"#, zone),
        );
        assert!(response.contains("200 OK"), "{}", response);
    }
    // A refused command too, which takes a different path through the worker.
    let refused = a_command(&address, r#"{"v":1,"t":"volume","zone":"none","volume":0.500}"#);
    assert!(refused.contains("400 Bad Request"), "{}", refused);

    thread::sleep(Duration::from_millis(300));
    let during = kernel_threads(pid);
    assert_eq!(
        during, before,
        "serving {} control subscribers created a thread the scheduling report never saw",
        subscribers.len()
    );

    drop(subscribers);
    thread::sleep(Duration::from_millis(300));
    let after = kernel_threads(pid);
    assert_eq!(
        after, before,
        "subscribers leaving changed the thread population"
    );

    // And the same again, with the audio path busy, because that is the
    // combination the deployment actually runs.
    let mut client = TcpStream::connect(("127.0.0.1", audio)).expect("the server is listening");
    client.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let mut scratch = vec![0u8; 65_536];
    assert!(client.read(&mut scratch).expect("audio comes down") > 0);
    let _held: Vec<Subscriber> = (0..4).map(|_| Subscriber::open(&address)).collect();
    thread::sleep(Duration::from_millis(300));
    assert_eq!(
        kernel_threads(pid),
        before,
        "a client and four subscribers together created a thread nobody declared"
    );
    drop(server);
}

#[test]
fn a_subscriber_past_the_ceiling_is_refused_by_name_rather_than_served_by_a_new_thread() {
    let audio = free_port();
    let control = free_port();
    let (server, pid, _rest, startup) = start(&[
        "--listen".to_string(),
        format!("127.0.0.1:{}", audio),
        "--max-clients".to_string(),
        "1".to_string(),
        "--control-listen".to_string(),
        format!("127.0.0.1:{}", control),
        "--control-workers".to_string(),
        "2".to_string(),
        "--zone".to_string(),
        "kitchen".to_string(),
    ]);
    let address = control_address(&startup);
    let before = kernel_threads(pid);
    assert_eq!(before.len(), 1 + 1 + 1 + 2 + 1 + 2);

    // Both workers held by event streams that never close.
    let _held: Vec<Subscriber> = (0..2).map(|_| Subscriber::open(&address)).collect();
    thread::sleep(Duration::from_millis(200));

    // The third connection has no worker to go to. It must be answered and
    // closed rather than served by a thread that did not exist at report time.
    let mut third = TcpStream::connect(&address).expect("the listener accepts it");
    third.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    write!(
        third,
        "GET /api/state HTTP/1.1\r\nHost: chorus\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    third.flush().unwrap();
    let mut response = String::new();
    let _ = third.read_to_string(&mut response);
    assert!(
        response.contains("503 Service Unavailable"),
        "a connection past the ceiling has to be refused by name, and this came back: {:?}",
        response
    );
    assert!(
        response.contains("control workers is busy"),
        "the refusal has to say why: {:?}",
        response
    );
    assert_eq!(
        kernel_threads(pid),
        before,
        "refusing a connection created a thread"
    );
    drop(server);
}

#[test]
fn a_control_address_that_cannot_be_bound_stops_the_server_before_it_serves_audio() {
    // AC-9: "IF the control channel cannot bind its configured address, or is
    // denied it THEN THE SYSTEM SHALL exit non-zero with a documented code
    // naming the address and the reason, and SHALL NOT serve audio while
    // reporting itself as controllable."
    //
    // The address is genuinely taken: this test holds it.
    let taken = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let address = taken.local_addr().unwrap().to_string();
    let audio = free_port();

    let output = Command::new(env!("CARGO_BIN_EXE_chorus-server"))
        .args([
            "--listen",
            &format!("127.0.0.1:{}", audio),
            "--source",
            "tone",
            "--allow-non-realtime",
            "--allow-unlocked-memory",
            "--control-listen",
            &address,
            "--zone",
            "kitchen",
        ])
        .output()
        .expect("the server binary runs");

    let code = output.status.code().unwrap_or(-1);
    assert_eq!(code, 8, "the documented exit code for a control refusal");
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(said.contains(&address), "the refusal has to name the address: {}", said);
    assert!(
        said.contains("could not be bound"),
        "the refusal has to name the reason: {}",
        said
    );
    assert!(
        said.contains("will not serve audio while reporting itself as controllable"),
        "{}",
        said
    );
    assert!(
        !said.contains("listening on="),
        "the audio socket must never have been bound: {}",
        said
    );
    assert!(
        said.contains("chunks_sent=0"),
        "and nothing was ever put on it: {}",
        said
    );

    // The audio port is still free, which is the strongest form of "did not
    // serve audio": nothing was ever bound to it.
    std::net::TcpListener::bind(("127.0.0.1", audio))
        .expect("the audio port was never bound by the refused server");
    drop(taken);
}

#[test]
fn a_state_file_this_build_cannot_read_stops_the_server_the_same_way() {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "chorus-unreadable-state-{}-{}.conf",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    ));
    std::fs::write(&path, "format = 1\nserial = 1\n\n[zone kitchen]\nname = K\n").unwrap();
    let audio = free_port();
    let control = free_port();

    let output = Command::new(env!("CARGO_BIN_EXE_chorus-server"))
        .args([
            "--listen",
            &format!("127.0.0.1:{}", audio),
            "--source",
            "tone",
            "--allow-non-realtime",
            "--allow-unlocked-memory",
            "--control-listen",
            &format!("127.0.0.1:{}", control),
            "--state-file",
            &path.display().to_string(),
        ])
        .output()
        .expect("the server binary runs");
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code().unwrap_or(-1), 8);
    let said = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(said.contains("could not be read"), "{}", said);
    assert!(
        said.contains("has no 'group'"),
        "the refusal has to name what was wrong with the file: {}",
        said
    );
    assert!(
        said.contains("Starting with defaults would silently discard"),
        "{}",
        said
    );
}
