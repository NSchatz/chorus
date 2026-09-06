//! The shape of the server process, graded against `/proc` rather than against
//! the server's own account of it.
//!
//! `crates/hostctl/src/lib.rs` states the property these tests exist for:
//! "Which threads are real-time is on the record. A report nobody can check is
//! not a safety property, so the report is compared against `/proc/self/task`,
//! which is the kernel's own answer." That comparison is taken once, at
//! startup, and it is only worth anything if the thread population it describes
//! is the one the process actually runs while it serves clients.
//!
//! Two things make that true, and both are checked here from outside the
//! process:
//!
//! 1. **Every thread has a row.** Not a count that happens to match: the tids
//!    the report names are exactly the tids the kernel lists, and no row says
//!    `role=unregistered`.
//! 2. **The thread that asks for a real-time policy is not the thread that
//!    creates the others.** `std::thread::spawn` uses the default
//!    `pthread_attr_t`, whose `inheritsched` is `PTHREAD_INHERIT_SCHED`, so a
//!    thread created by one holding `SCHED_FIFO` is `SCHED_FIFO` too, at the
//!    same priority. `deploy/run-server.sh` runs this binary with `--ulimit
//!    rtprio=20`, so on the deployment target that is the difference between
//!    one real-time thread and every socket handler in the process being one.
//!    The main thread's tid is the process id on Linux, which is what makes
//!    this checkable without any privilege at all: the audio role must not be
//!    on it.
//!
//! The policies themselves cannot be checked here. This container grants an
//! rtprio ceiling of zero, so `--allow-non-realtime` is what these runs use and
//! no thread is real-time in any of them; `tools/host-contract.sh` is the entry
//! point that grades the granted case, and it refuses by name on a host like
//! this one. What is checked here is the property that holds either way.

use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Read};
use std::net::TcpStream;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// A port nothing is listening on, by binding one and letting it go.
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

/// One row of the scheduling report.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ReportedThread {
    role: String,
    tid: u32,
}

/// Start the server, and read its output until it says it is listening.
fn start(port: u16, extra: &[&str]) -> (Server, u32, mpsc::Receiver<String>, Vec<String>) {
    let mut args = vec![
        "--listen".to_string(),
        format!("127.0.0.1:{}", port),
        "--source".to_string(),
        "tone".to_string(),
        "--tone-ms".to_string(),
        "30000".to_string(),
        // Neither of these is what is under test: this container grants no
        // real-time priority and less locked memory than the server wants.
        "--allow-non-realtime".to_string(),
        "--allow-unlocked-memory".to_string(),
    ];
    args.extend(extra.iter().map(|a| a.to_string()));

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
                let listening = line.contains("listening on=");
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
    panic!(
        "the server never reported a bound socket; it said: {:?}",
        startup
    );
}

/// Read a child's stdout on its own thread, because the report goes by before
/// anything connects.
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

/// The rows of the scheduling report, in the order the server printed them.
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

/// The threads the kernel says this process runs, right now.
fn kernel_threads(pid: u32) -> BTreeSet<u32> {
    std::fs::read_dir(format!("/proc/{}/task", pid))
        .expect("the server process is alive and /proc is mounted")
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().to_string_lossy().parse().ok())
        .collect()
}

/// Attach a client and read enough to know it is being served.
fn served(port: u16) -> TcpStream {
    let mut client = TcpStream::connect(("127.0.0.1", port)).expect("the server is listening");
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut scratch = vec![0u8; 65_536];
    let read = client.read(&mut scratch).expect("the client is served audio");
    assert!(read > 0, "the client received nothing, so nothing is attached");
    client
}

#[test]
fn every_thread_the_server_runs_has_a_row_in_its_scheduling_report() {
    let port = free_port();
    let (server, pid, _rest, startup) = start(port, &["--max-clients", "2"]);
    let rows = reported(&startup);
    assert!(
        !rows.is_empty(),
        "the server printed no thread rows at all: {:?}",
        startup
    );
    assert!(
        !rows.iter().any(|r| r.role == "unregistered"),
        "a thread nobody declared is running: {:?}",
        rows
    );

    let client = served(port);
    // Long enough that a thread created for this client would exist.
    thread::sleep(Duration::from_millis(200));

    let running = kernel_threads(pid);
    let declared: BTreeSet<u32> = rows.iter().map(|r| r.tid).collect();
    assert_eq!(
        declared, running,
        "the report names {:?} and the kernel is running {:?} while one client is served. Every \
         thread of this process is created before the report is taken, and none after it, which \
         is what makes one report a description of the whole run",
        rows, running
    );
    // 1 supervisor + 1 audio + 1 acceptor + 2 threads for each of 2 slots.
    assert_eq!(
        running.len(),
        7,
        "the thread population is 3 + 2 * max_clients and nothing else: {:?}",
        rows
    );
    drop(client);
    drop(server);
}

#[test]
fn the_thread_that_asks_for_a_real_time_policy_is_not_the_thread_that_creates_the_others() {
    let port = free_port();
    let (server, pid, _rest, startup) = start(port, &["--max-clients", "1"]);
    let rows = reported(&startup);

    let audio = rows
        .iter()
        .find(|r| r.role == "audio")
        .unwrap_or_else(|| panic!("no thread carries the audio role: {:?}", rows));
    let supervisor = rows
        .iter()
        .find(|r| r.role == "supervisor")
        .unwrap_or_else(|| panic!("no thread carries the supervisor role: {:?}", rows));

    // On Linux the main thread's tid is the process id, and the main thread is
    // the one that creates the acceptor and every client thread.
    assert_eq!(
        supervisor.tid, pid,
        "the supervisor role is the main thread, which is the one that creates the others"
    );
    assert_ne!(
        audio.tid, pid,
        "the audio role, which is the only role that asks the host for a real-time policy, is on \
         the thread that creates every other thread in this process. std::thread::spawn inherits \
         the creating thread's scheduling policy, so under deploy/run-server.sh's --ulimit \
         rtprio=20 that makes every socket handler SCHED_FIFO at priority 20"
    );
    assert!(
        rows.iter().filter(|r| r.role == "audio").count() == 1,
        "exactly one thread does the audio work: {:?}",
        rows
    );
    drop(server);
}

#[test]
fn a_client_past_the_ceiling_is_refused_by_name_rather_than_served_by_a_new_thread() {
    let port = free_port();
    let (server, pid, rest, startup) = start(port, &["--max-clients", "1"]);
    let before = kernel_threads(pid);

    let first = served(port);
    let mut second = TcpStream::connect(("127.0.0.1", port)).expect("the server is listening");
    second
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut scratch = vec![0u8; 1_024];
    assert_eq!(
        second.read(&mut scratch).ok(),
        Some(0),
        "the pool was full, so the second connection has to be closed rather than left waiting \
         for audio that never comes"
    );

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut refusal = None;
    while Instant::now() < deadline && refusal.is_none() {
        match rest.recv_timeout(Duration::from_millis(500)) {
            Ok(line) => {
                if line.contains("client refused") {
                    refusal = Some(line);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    let refusal = refusal.expect("a refused client is said out loud, not dropped silently");
    assert!(
        refusal.contains("reason=no-free-client-slot") && refusal.contains("max_clients=1"),
        "the refusal has to name the reason and the ceiling: {}",
        refusal
    );

    assert_eq!(
        before,
        kernel_threads(pid),
        "neither serving a client nor refusing one creates a thread; the report taken at startup \
         still describes the process: {:?}",
        startup.len()
    );
    drop(first);
    drop(server);
}
