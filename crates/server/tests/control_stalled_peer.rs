//! A control peer that stops draining its receive window is dropped by the
//! event writer, and nobody else is delayed by it (audit finding B-5).
//!
//! AC-11 is about a subscriber that stops reading, and
//! `crates/control/tests/slow_subscriber.rs` grades every clause of it at the
//! application layer: bounded queue, subscriber dropped at the ceiling, what it
//! missed counted and reported, nobody else delayed, the audio path neither
//! blocked nor reordered. All of that holds.
//!
//! This is the layer underneath. A peer that stops draining its TCP receive
//! window - rather than closing - used to block the worker holding its stream
//! inside `write`, until `crates/server/src/control.rs::WRITE_TIMEOUT` gave the
//! slot back. Every stream is now written by ONE thread, the event writer
//! (`crates/server/src/events.rs`), on non-blocking sockets, so the hazard is
//! a different one and so is the demonstration, over real sockets against the
//! real binary, with ONE control worker:
//!
//! 1. A subscriber attaches to `/api/events` and never reads a byte; a second,
//!    well-behaved one attaches and reads everything.
//! 2. Commands are applied, each answered `200` by the one worker (a stream
//!    holds no worker), and EVERY one reaches the well-behaved subscriber
//!    within [`PROMPT`] while the stalled one's socket is full: the writer is
//!    never inside a `write` that waits.
//! 3. The fanout drops the stalled subscriber at its queue's ceiling, and the
//!    writer drops its stream once it has made no write progress for
//!    [`WRITE_TIMEOUT`], counted in the report (`stalled_dropped=1`), leaving
//!    the well-behaved one the only stream held.
//!
//! Every server binds port 0 and is asked where it landed, and every wait polls
//! to a bounded deadline instead of sleeping a fixed time.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

/// Enough zones that one state message is tens of kilobytes, so a handful of
/// them fills a loopback socket's buffers. A message of a few hundred bytes
/// would need thousands of round trips to fill one.
const ZONES: usize = 400;

/// One: an event stream holds no worker, so one is enough for every command.
const WORKERS: usize = 1;

/// How long a state may take to reach a subscriber that is reading. Far longer
/// than the writer takes (it is woken by every change); what this rules out is
/// a write to the stalled peer holding the writer for [`WRITE_TIMEOUT`].
const PROMPT: Duration = Duration::from_secs(2);

/// `crates/server/src/control.rs::WRITE_TIMEOUT`.
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// How long step 4 may take. The write timeout plus room for a loaded machine;
/// a build with no write timeout does not finish this no matter how long it is.
const RECLAIM_DEADLINE: Duration = Duration::from_secs(45);

/// How long any other wait here may take before it is a failure.
const PATIENCE: Duration = Duration::from_secs(30);

struct Server {
    child: Child,
    control: String,
    /// What the child says, line by line. Held for the server's whole life so
    /// the pumps carrying its output never stop draining it.
    lines: mpsc::Receiver<String>,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn pump<R: Read + Send + 'static>(stream: R, tx: mpsc::Sender<String>) {
    std::thread::spawn(move || {
        for line in BufReader::new(stream).lines().map_while(Result::ok) {
            let _ = tx.send(line);
        }
    });
}

/// Start the server on ports the kernel picks and read back where its control
/// channel landed, once the audio socket is bound (after the scheduling report,
/// so every worker exists).
fn start() -> Server {
    let mut command = Command::new(env!("CARGO_BIN_EXE_chorus-server"));
    command.args([
        "--listen",
        "127.0.0.1:0",
        "--source",
        "tone",
        "--serve-forever",
        "--allow-non-realtime",
        "--allow-unlocked-memory",
        "--ephemeral-identity",
        "--control-listen",
        "127.0.0.1:0",
        "--control-workers",
        &WORKERS.to_string(),
    ]);
    for zone in 0..ZONES {
        command.args(["--zone", &format!("zone-{:03}", zone)]);
    }
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the server binary runs");
    let (tx, lines) = mpsc::channel();
    pump(child.stdout.take().unwrap(), tx.clone());
    pump(child.stderr.take().unwrap(), tx);
    // Owned by the guard from here on, so every way out of this function kills
    // and reaps it unless it is handed back.
    let mut server = Server {
        child,
        control: String::new(),
        lines,
    };
    let mut said = Vec::new();
    let mut control = None;
    let deadline = Instant::now() + PATIENCE;
    while Instant::now() < deadline {
        let Ok(line) = server.lines.recv_timeout(Duration::from_millis(200)) else {
            continue;
        };
        if let Some(at) = line.find("control listening on=") {
            let rest = &line[at + "control listening on=".len()..];
            control = rest.split_whitespace().next().map(str::to_string);
        }
        let ready = line.starts_with("chorus-server: listening on=");
        said.push(line);
        if let (true, Some(control)) = (ready, control.as_ref()) {
            server.control = control.clone();
            return server;
        }
    }
    panic!("the server never said it was listening: {:?}", said);
}

/// One short-lived request, with the whole response read back.
///
/// `None` where the connection could not be made at all, which is not the same
/// as a connection that was answered `503`.
fn request(address: &str, head: &str, body: &str) -> Option<String> {
    let mut socket = TcpStream::connect(address).ok()?;
    socket.set_read_timeout(Some(PATIENCE)).ok()?;
    socket.set_write_timeout(Some(PATIENCE)).ok()?;
    write!(socket, "{}{}", head, body).ok()?;
    socket.flush().ok()?;
    let mut response = Vec::new();
    let mut scratch = [0u8; 16_384];
    loop {
        match socket.read(&mut scratch) {
            Ok(0) | Err(_) => break,
            Ok(read) => response.extend_from_slice(&scratch[..read]),
        }
    }
    Some(String::from_utf8_lossy(&response).to_string())
}

/// The accept loop's refusal: every worker busy for a moment, because a worker
/// hands its slot back just after closing the connection it served. Retried
/// past, never judged.
fn momentarily_full(answer: &str) -> bool {
    answer.is_empty()
        || (answer.contains("503 Service Unavailable") && !answer.contains("event streams is held"))
}

/// A request served by a worker, retried past [`momentarily_full`] only.
fn served(address: &str, head: &str, body: &str) -> String {
    let started = Instant::now();
    loop {
        let answer = request(address, head, body).unwrap_or_default();
        if !momentarily_full(&answer) || started.elapsed() >= PATIENCE {
            return answer;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn post(address: &str) -> String {
    let body = r#"{"v":1,"t":"volume","zone":"zone-000","volume":0.500}"#;
    let head = format!(
        "POST /api/command HTTP/1.1\r\nHost: chorus\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    served(address, &head, body)
}

fn report(address: &str) -> String {
    served(
        address,
        "GET /api/report HTTP/1.1\r\nHost: chorus\r\nConnection: close\r\n\r\n",
        "",
    )
}

/// Open an event stream and never read from it.
fn subscribe(address: &str) -> TcpStream {
    let mut socket = TcpStream::connect(address).expect("the control channel is listening");
    socket.set_write_timeout(Some(PATIENCE)).unwrap();
    write!(
        socket,
        "GET /api/events HTTP/1.1\r\nHost: chorus\r\nAccept: text/event-stream\r\n\r\n"
    )
    .expect("the request goes up");
    socket.flush().unwrap();
    socket
}

/// Open an event stream and read its opening: the socket if the stream was
/// served, and the answer if it was refused. Retried past [`momentarily_full`].
fn attach(address: &str) -> Result<TcpStream, String> {
    let started = Instant::now();
    loop {
        let mut socket = subscribe(address);
        socket.set_read_timeout(Some(PATIENCE)).unwrap();
        let mut opening = Vec::new();
        let mut scratch = [0u8; 4_096];
        loop {
            match socket.read(&mut scratch) {
                Ok(0) | Err(_) => break,
                Ok(read) => {
                    opening.extend_from_slice(&scratch[..read]);
                    if opening.windows(10).any(|w| w == b"\r\n\r\ndata: ") {
                        break;
                    }
                }
            }
        }
        let opening = String::from_utf8_lossy(&opening).to_string();
        if opening.starts_with("HTTP/1.1 200 OK") && opening.contains("\r\n\r\ndata: ") {
            return Ok(socket);
        }
        if !momentarily_full(&opening) || started.elapsed() >= PATIENCE {
            return Err(opening);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Poll the report until it contains `wanted`, or give up at [`PATIENCE`].
fn wait_for_report(address: &str, wanted: &str) -> Result<String, String> {
    let started = Instant::now();
    loop {
        let text = report(address);
        if text.contains(wanted) {
            return Ok(text);
        }
        if started.elapsed() >= PATIENCE {
            return Err(text);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_peer_that_stops_reading_is_dropped_by_the_event_writer_and_delays_nobody() {
    let server = start();
    let address = server.control.clone();

    // 1. The stalled subscriber, attached on the control plane's own count
    //    before anything is sent, and a reading one beside it.
    let stalled = subscribe(&address);
    if let Err(last) = wait_for_report(&address, "subscribers=1 ") {
        panic!("the stalled subscriber never attached: {}", last);
    }
    let mut reading = attach(&address).expect("the reading subscriber is served");
    reading
        .set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    let seen = Arc::new(AtomicUsize::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let reader = {
        let seen = Arc::clone(&seen);
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            let mut tail: Vec<u8> = Vec::new();
            let mut scratch = [0u8; 65_536];
            while !stop.load(Ordering::SeqCst) {
                match reading.read(&mut scratch) {
                    Ok(0) => break,
                    Ok(n) => {
                        tail.extend_from_slice(&scratch[..n]);
                        let mut at = 0;
                        while let Some(i) = find(&tail[at..], b"\n\ndata: ") {
                            seen.fetch_add(1, Ordering::SeqCst);
                            at += i + 1;
                        }
                        let keep = tail.len().saturating_sub(8).max(at);
                        tail.drain(..keep);
                    }
                    Err(_) => {}
                }
            }
        })
    };

    // 2. Commands, each served and each reaching the reading subscriber
    //    promptly, until the writer has dropped the stalled stream.
    let began = Instant::now();
    let mut applied = 0usize;
    let mut slowest = Duration::ZERO;
    let mut report_text = String::new();
    while began.elapsed() < RECLAIM_DEADLINE {
        let answer = post(&address);
        assert!(
            answer.contains("200 OK"),
            "a command was not served while a stalled stream was held: {}",
            answer.lines().next().unwrap_or("nothing at all")
        );
        applied += 1;
        // `attach` read the opening up to its `data: `; each later state is
        // counted when its own `data: ` arrives after the last one's end.
        let sent = Instant::now();
        while seen.load(Ordering::SeqCst) < applied {
            assert!(
                sent.elapsed() < PROMPT,
                "command {} did not reach the reading subscriber within {:?} ({} of {} \
                 arrived): the stalled peer delayed it",
                applied,
                PROMPT,
                seen.load(Ordering::SeqCst),
                applied
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        slowest = slowest.max(sent.elapsed());
        report_text = report(&address);
        if report_text.contains("stalled_dropped=1") {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    // 3. Dropped, counted, and the reading one the only stream held.
    assert!(
        report_text.contains("stalled_dropped=1"),
        "the event writer never dropped the stalled stream within {:?} ({} commands): {}",
        RECLAIM_DEADLINE,
        applied,
        report_text
    );
    assert!(
        began.elapsed() >= WRITE_TIMEOUT - Duration::from_millis(500),
        "dropped before the stall bound could have run out: {:?}",
        began.elapsed()
    );
    let held = wait_for_report(&address, "events held=1 ")
        .unwrap_or_else(|last| panic!("the reading stream is not the only one held: {}", last));
    println!(
        "stalled stream dropped by the event writer {:?} after it attached, {} commands each \
         reaching the reading subscriber within {:?}; {}",
        began.elapsed(),
        applied,
        slowest,
        held.lines().last().unwrap_or("").trim()
    );
    stop.store(true, Ordering::SeqCst);
    let _ = reader.join();
    drop(stalled);
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}
