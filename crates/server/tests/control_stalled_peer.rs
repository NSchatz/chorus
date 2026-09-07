//! A control peer that stops draining its receive window must not keep a
//! worker slot forever.
//!
//! AC-11 is about a subscriber that stops reading, and
//! `crates/control/tests/slow_subscriber.rs` grades every clause of it at the
//! application layer: bounded queue, subscriber dropped at the ceiling, what it
//! missed counted and reported, nobody else delayed, the audio path neither
//! blocked nor reordered. All of that holds.
//!
//! This is the layer underneath, and it is not the same question. A peer that
//! stops draining its TCP receive window - rather than closing - blocks its
//! worker inside `write`. The fanout still drops the SUBSCRIBER at the ceiling,
//! but the WORKER is stuck in a system call and never reaches `recv_timeout`,
//! so its slot in the fixed pool is never returned. The pool is fixed on
//! purpose (`crates/server/src/main.rs`: every thread this process will ever
//! run exists before the scheduling report), so a slot that is never returned
//! cannot be replaced by growing the pool, and enough such peers make the
//! accept loop answer `503 Service Unavailable` to everybody.
//!
//! `crates/server/src/control.rs::WRITE_TIMEOUT` is what bounds it. This test
//! is the demonstration, over real sockets against the real binary:
//!
//! 1. A subscriber attaches to `/api/events` and never reads a byte.
//! 2. Commands are applied until the fanout reports that subscriber DROPPED.
//!    The queue only backs up if the worker is not draining it, so the drop is
//!    itself the evidence that the worker is stuck inside `write`.
//! 3. A second, well-behaved subscriber takes the other worker, so every slot
//!    in the pool is now occupied and a fresh connection is answered `503`.
//! 4. The stuck worker's write times out, its connection is dropped and its
//!    slot comes back, so the same request is answered.
//!
//! Without a write timeout step 4 never happens and this test hangs at its
//! deadline and fails, which is what it is for.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Enough zones that one state message is tens of kilobytes, so a handful of
/// them fills a loopback socket's buffers. A message of a few hundred bytes
/// would need thousands of round trips to fill one.
const ZONES: usize = 400;

/// Two, so that the pool can be filled by one stuck peer and one well-behaved
/// subscriber. Any larger and the test would have to stall more peers to say
/// the same thing.
const WORKERS: usize = 2;

/// The write timeout in `control.rs` is five seconds; this is that plus room
/// for a loaded machine. A build with no write timeout does not finish this no
/// matter how long the deadline is.
const RECLAIM_DEADLINE: Duration = Duration::from_secs(45);

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

fn start(audio: u16, control: u16, log: &PathBuf) -> Server {
    let mut command = Command::new(env!("CARGO_BIN_EXE_chorus-server"));
    command.args([
        "--listen",
        &format!("127.0.0.1:{}", audio),
        "--source",
        "tone",
        "--serve-forever",
        "--allow-non-realtime",
        "--allow-unlocked-memory",
        "--control-listen",
        &format!("127.0.0.1:{}", control),
        "--control-workers",
        &WORKERS.to_string(),
    ]);
    for zone in 0..ZONES {
        command.args(["--zone", &format!("zone-{:03}", zone)]);
    }
    Server(
        command
            .stdout(Stdio::from(
                std::fs::File::create(log).expect("a log file"),
            ))
            .stderr(Stdio::null())
            .spawn()
            .expect("the server binary runs"),
    )
}

/// One short-lived request, with the whole response read back.
///
/// `None` where the connection could not be made at all, which is not the same
/// as a connection that was answered `503`.
fn request(address: &str, head: &str, body: &str) -> Option<String> {
    let mut socket = TcpStream::connect(address).ok()?;
    socket.set_read_timeout(Some(Duration::from_secs(10))).ok()?;
    socket.set_write_timeout(Some(Duration::from_secs(10))).ok()?;
    write!(socket, "{}{}", head, body).ok()?;
    socket.flush().ok()?;
    let mut response = String::new();
    let _ = socket.read_to_string(&mut response);
    Some(response)
}

fn post(address: &str) -> Option<String> {
    let body = r#"{"v":1,"t":"volume","zone":"zone-000","volume":0.500}"#;
    let head = format!(
        "POST /api/command HTTP/1.1\r\nHost: chorus\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    request(address, &head, body)
}

fn report(address: &str) -> Option<String> {
    request(
        address,
        "GET /api/report HTTP/1.1\r\nHost: chorus\r\nConnection: close\r\n\r\n",
        "",
    )
}

fn wait_for_control(address: &str) -> bool {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if TcpStream::connect(address).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

/// Open an event stream. `drain` says whether anything is ever read from it.
fn subscribe(address: &str) -> TcpStream {
    let mut socket = TcpStream::connect(address).expect("the control channel is listening");
    socket.set_write_timeout(Some(Duration::from_secs(10))).unwrap();
    write!(
        socket,
        "GET /api/events HTTP/1.1\r\nHost: chorus\r\nAccept: text/event-stream\r\n\r\n"
    )
    .expect("the request goes up");
    socket.flush().unwrap();
    socket
}

#[test]
fn a_peer_that_stops_reading_gives_its_worker_slot_back() {
    let audio = free_port();
    let control = free_port();
    let address = format!("127.0.0.1:{}", control);
    let mut log = std::env::temp_dir();
    log.push(format!("chorus-stalled-peer-{}.log", std::process::id()));
    let _server = start(audio, control, &log);
    assert!(wait_for_control(&address), "the server never came up");

    // 1. A subscriber that never reads a byte of what it asked for.
    let stalled = subscribe(&address);
    std::thread::sleep(Duration::from_millis(300));

    // 2. Apply commands until the fanout says it dropped that subscriber. The
    //    queue only backs up while the worker is not draining it, so this is
    //    the point at which the worker is known to be inside `write`.
    let mut applied = 0usize;
    let mut dropped = String::new();
    for _ in 0..2_000 {
        if post(&address).is_none() {
            break;
        }
        applied += 1;
        if applied % 8 == 0 {
            if let Some(text) = report(&address) {
                if text.contains("dropped_subscribers=1") {
                    dropped = text;
                    break;
                }
            }
        }
    }
    assert!(
        dropped.contains("dropped_subscribers=1"),
        "the stalled subscriber was never dropped after {} commands, so this test never reached \
         the state it is about. The last report was:\n{}",
        applied,
        report(&address).unwrap_or_default()
    );

    // 3. The other worker goes to a subscriber that behaves, so every slot in
    //    the pool is occupied. A fresh connection now has nowhere to go.
    let mut polite = subscribe(&address);
    let mut opening = [0u8; 64];
    polite
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let read = polite.read(&mut opening).expect("the polite subscriber is served");
    assert!(read > 0, "the second subscriber was not served at all");

    let busy = post(&address).unwrap_or_default();
    assert!(
        busy.contains("503 Service Unavailable"),
        "the premise of this test is that both workers are occupied - one by the stalled peer and \
         one by the polite subscriber - and a fresh connection is turned away. It was answered:\n{}",
        busy.lines().next().unwrap_or("nothing at all")
    );

    // 4. And then the stuck worker's write times out, so the slot comes back
    //    without anybody doing anything.
    let deadline = Instant::now() + RECLAIM_DEADLINE;
    let began = Instant::now();
    let mut answered = String::new();
    while Instant::now() < deadline {
        let attempt = post(&address).unwrap_or_default();
        if attempt.contains("200 OK") {
            answered = attempt;
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    let took = began.elapsed();
    assert!(
        answered.contains("200 OK"),
        "AC-11's fixed worker pool never got the slot back: {:?} after the pool filled, a command \
         is still answered 503 because a peer that stopped reading is still holding a worker \
         inside `write`. The fanout dropped the subscriber ({}), but the WORKER was never \
         reclaimed.",
        took,
        dropped.lines().last().unwrap_or("").trim()
    );
    println!(
        "the stuck worker's slot came back {:?} after the pool filled, with no operator action",
        took
    );

    // The stalled peer is still open here, and dropping it is the test's own
    // cleanup rather than anything the server needed.
    drop(stalled);
    drop(polite);
    let _ = std::fs::remove_file(&log);
}
