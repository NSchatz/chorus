//! S0031-chorus-sync-4, refuter finding F6: every thread the server runs is one
//! its scheduling report counted.
//!
//! `crates/hostctl/src/lib.rs` states the safety property in its own words:
//! "Which threads are real-time is on the record. A report nobody can check is
//! not a safety property, so the report is compared against
//! `/proc/self/task`, which is the kernel's own answer." `crates/server/src/main.rs`
//! runs `scheduling_report` ONCE, at startup.
//!
//! Before this phase that described the whole process: the old accept-and-serve
//! loop ran on the main thread and spawned nothing. The first cut of the fanout
//! server spawned an acceptor, a producer, and two threads per attached client,
//! all from the thread that held the real-time policy, and registered none of
//! them. `std::thread::spawn` uses default `pthread_attr_t`, whose
//! `inheritsched` is `PTHREAD_INHERIT_SCHED`, so each of them takes the creating
//! thread's scheduling policy and priority. `deploy/run-server.sh` runs the
//! container with `--ulimit rtprio=20` and `--rt-priority 20`, so on the
//! deployment target those threads were `SCHED_FIFO` at priority 20 and none of
//! them appeared in the report the host contract is graded on.
//!
//! This is the refuter's demonstration of that, carried into the tree unchanged
//! in its body so it goes on guarding the behaviour rather than the moment it
//! was found. It is in two halves:
//!
//! - `a_spawned_thread_inherits_the_policy_of_the_thread_that_spawned_it`
//!   establishes the mechanism, using `SCHED_BATCH`, which any unprivileged
//!   thread may set. It passes on any Linux and is here so the second half is
//!   read for what it is.
//! - `every_thread_the_server_runs_is_one_its_scheduling_report_counted`
//!   is the finding: it runs the real `chorus-server` binary with a client
//!   attached and compares the thread count the report published against
//!   `/proc/<pid>/task`. It passed at `origin/main`, failed on the tree this
//!   finding was raised against, and passes again now that the process creates
//!   every thread it will ever run before it takes the report and none after.
//!
//! This container grants an rtprio ceiling of 0, so the second half runs with
//! `--allow-non-realtime`: what it can show here is whether the report saw the
//! threads, which is the half that is a property of the code rather than of
//! the host.

use std::io::{BufRead, BufReader, Read};
use std::net::TcpStream;
use std::os::raw::c_int;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

#[repr(C)]
struct SchedParam {
    sched_priority: c_int,
}

extern "C" {
    fn sched_setscheduler(pid: c_int, policy: c_int, param: *const SchedParam) -> c_int;
    fn sched_getscheduler(pid: c_int) -> c_int;
}

/// `SCHED_BATCH`. Settable by any thread, no privilege and no rtprio ceiling
/// needed, which is what makes the mechanism demonstrable in this container.
const SCHED_BATCH: c_int = 3;

#[test]
fn a_spawned_thread_inherits_the_policy_of_the_thread_that_spawned_it() {
    let parent = thread::spawn(|| {
        let param = SchedParam { sched_priority: 0 };
        let rc = unsafe { sched_setscheduler(0, SCHED_BATCH, &param) };
        assert_eq!(rc, 0, "SCHED_BATCH needs no privilege and should be settable");
        let mine = unsafe { sched_getscheduler(0) };
        assert_eq!(mine, SCHED_BATCH, "this thread did not take the policy");

        let child = thread::spawn(|| unsafe { sched_getscheduler(0) });
        child.join().expect("the child thread finishes")
    });
    let child_policy = parent.join().expect("the parent thread finishes");
    assert_eq!(
        child_policy, SCHED_BATCH,
        "std::thread::spawn is documented here as inheriting the creating thread's scheduling \
         policy; it reported {} instead. If this ever stops being true the finding this file \
         carries has to be re-derived",
        child_policy
    );
}

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

/// Threads the kernel says this pid runs, right now.
fn kernel_threads(pid: u32) -> usize {
    std::fs::read_dir(format!("/proc/{}/task", pid))
        .expect("the server process is alive and /proc is mounted")
        .filter_map(|e| e.ok())
        .count()
}

#[test]
fn every_thread_the_server_runs_is_one_its_scheduling_report_counted() {
    let port = free_port();
    let mut child = Command::new(env!("CARGO_BIN_EXE_chorus-server"))
        .args([
            "--listen",
            &format!("127.0.0.1:{}", port),
            "--source",
            "tone",
            "--tone-ms",
            "30000",
            // This container grants no real-time priority and less locked
            // memory than the server wants. Neither is what is under test: what
            // is under test is whether the report saw the threads at all.
            "--allow-non-realtime",
            "--allow-unlocked-memory",
            "--serve-forever",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the server binary runs");
    let pid = child.id();
    let stdout = child.stdout.take().expect("stdout was piped");
    let server = Server(child);

    // The report line goes by before the socket is bound, so it is read on its
    // own thread and handed over.
    let (tx, rx) = mpsc::channel::<String>();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                return;
            }
        }
    });

    let mut declared: Option<usize> = None;
    let mut listening = false;
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline && !(declared.is_some() && listening) {
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(line) => {
                if let Some(at) = line.find("scheduling-report threads=") {
                    let rest = &line[at + "scheduling-report threads=".len()..];
                    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                    declared = digits.parse().ok();
                }
                if line.contains("listening on=") {
                    listening = true;
                }
            }
            Err(_) => continue,
        }
    }
    let declared = declared.expect("the server prints its scheduling report before it listens");
    assert!(listening, "the server never reported a bound socket");

    // One client, which is what `deploy/run-server.sh` exists to serve. Two is
    // what AC-8 asks for, and each one costs another pair.
    let mut client = TcpStream::connect(("127.0.0.1", port)).expect("the server is listening");
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut scratch = vec![0u8; 65_536];
    let read = client.read(&mut scratch).expect("the client is served audio");
    assert!(read > 0, "the client received nothing, so nothing is attached");
    // Give the per-client threads a moment to exist.
    thread::sleep(Duration::from_millis(200));

    let running = kernel_threads(pid);
    let names: Vec<String> = std::fs::read_dir(format!("/proc/{}/task", pid))
        .map(|d| {
            d.filter_map(|e| e.ok())
                .map(|e| {
                    let tid = e.file_name().to_string_lossy().into_owned();
                    let comm = std::fs::read_to_string(format!("/proc/{}/task/{}/comm", pid, tid))
                        .unwrap_or_default();
                    format!("{} ({})", tid, comm.trim())
                })
                .collect()
        })
        .unwrap_or_default();
    drop(server);

    assert_eq!(
        running, declared,
        "the scheduling report the host contract is graded on counted {} thread(s), and the \
         process is running {} while it serves one client: {:?}. Every one of the extra threads \
         is spawned by the thread that called take_contract_for_this_thread, and \
         std::thread::spawn inherits that thread's scheduling policy (asserted in the test above), \
         so under deploy/run-server.sh's --ulimit rtprio=20 they are SCHED_FIFO at priority 20 and \
         none of them is in the report. scheduling_report() is never run again, so the server's \
         own EXIT_UNDECLARED_THREAD check and tools/host-contract.sh cannot see them",
        declared, running, names
    );
}
