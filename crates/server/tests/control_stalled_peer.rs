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
//! is the demonstration, over real sockets against the real binary, with two
//! workers: one an event stream may take, and one kept for commands
//! (`control.rs::stream_slots`).
//!
//! 1. A subscriber attaches to `/api/events` and never reads a byte.
//! 2. Commands are applied until the fanout reports that subscriber DROPPED.
//!    The queue only backs up if the worker is not draining it, so the drop is
//!    itself the evidence that the worker is stuck inside `write`.
//! 3. A second, well-behaved subscriber is refused `503`: the only worker a
//!    stream may have is the stuck one. A command is still served, by the
//!    worker kept for commands.
//! 4. The stuck worker's write times out, its connection is dropped and its
//!    slot comes back, so the second subscriber is served.
//!
//! Without a write timeout step 4 never happens and this test fails at its
//! deadline, which is what it is for.
//!
//! # Scheduling cannot decide it
//!
//! Step 3's refusal holds only while the stuck worker is still inside its
//! write, which ends [`WRITE_TIMEOUT`] after it blocked. The write cannot have
//! blocked before the stalled subscriber attached, so a step 3 that sees the
//! second subscriber served LESS than [`WRITE_TIMEOUT`] after that attach is a
//! real failure, and one that sees it served later has only lost the window to
//! a slow machine. The second is not a verdict: the scenario is run again on a
//! fresh server, a bounded number of times. Every server binds port 0 and is
//! asked where it landed, and every wait polls to a bounded deadline instead
//! of sleeping a fixed time.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Enough zones that one state message is tens of kilobytes, so a handful of
/// them fills a loopback socket's buffers. A message of a few hundred bytes
/// would need thousands of round trips to fill one.
const ZONES: usize = 400;

/// Two: one worker a stream may take, and the one kept for commands.
const WORKERS: usize = 2;

/// `crates/server/src/control.rs::WRITE_TIMEOUT`.
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// How long step 4 may take. The write timeout plus room for a loaded machine;
/// a build with no write timeout does not finish this no matter how long it is.
const RECLAIM_DEADLINE: Duration = Duration::from_secs(45);

/// How long any other wait here may take before it is a failure.
const PATIENCE: Duration = Duration::from_secs(30);

/// How many times the scenario is run before a window lost every time to a
/// slow machine is reported as that.
const ATTEMPTS: usize = 3;

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
        || (answer.contains("503 Service Unavailable")
            && !answer.contains("event-stream control workers"))
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

/// What one run of steps 1 to 3 came to.
enum Premise {
    /// The stalled peer holds the stream worker and the pool is as step 3 says.
    Held {
        server: Server,
        stalled: TcpStream,
        began: Instant,
        dropped: String,
    },
    /// The stuck write may already have timed out, so step 3 decided nothing.
    WindowLost(Duration),
}

fn steps_one_to_three() -> Premise {
    let server = start();
    let address = server.control.clone();

    // 1. A subscriber that never reads a byte of what it asked for, attached on
    //    the control plane's own count before anything is sent.
    let began = Instant::now();
    let stalled = subscribe(&address);
    if let Err(last) = wait_for_report(&address, "subscribers=1 ") {
        panic!("the stalled subscriber never attached: {}", last);
    }

    // 2. Apply commands until the fanout says it dropped that subscriber. The
    //    queue only backs up while the worker is not draining it, so this is
    //    the point at which the worker is known to be inside `write`.
    let mut applied = 0usize;
    let mut dropped = String::new();
    let step_two = Instant::now();
    while step_two.elapsed() < PATIENCE {
        let answer = post(&address);
        assert!(
            answer.contains("200 OK"),
            "a command was not applied while the stalled peer was being filled: {}",
            answer.lines().next().unwrap_or("nothing at all")
        );
        applied += 1;
        if applied.is_multiple_of(8) {
            let text = report(&address);
            if text.contains("dropped_subscribers=1") {
                dropped = text;
                break;
            }
        }
    }
    assert!(
        dropped.contains("dropped_subscribers=1"),
        "the stalled subscriber was never dropped after {} commands over {:?}, so this test never \
         reached the state it is about. The last report was:\n{}",
        applied,
        step_two.elapsed(),
        report(&address)
    );

    // 3. The only worker a stream may have is the stuck one, so a second,
    //    well-behaved subscriber is refused, and the worker kept for commands
    //    still serves one.
    match attach(&address) {
        Ok(_) => {
            let since = began.elapsed();
            assert!(
                since >= WRITE_TIMEOUT,
                "a second subscriber was served {:?} after the stalled one attached, which is \
                 inside the {:?} write timeout: the stalled peer's worker is still inside \
                 `write`, so a stream was given a worker it may not have",
                since,
                WRITE_TIMEOUT
            );
            return Premise::WindowLost(since);
        }
        Err(refused) => assert!(
            refused.contains("503 Service Unavailable") && refused.contains("kept for commands"),
            "the second subscriber was neither served nor refused for want of a stream worker: \
             {}",
            refused
        ),
    }
    let answer = post(&address);
    assert!(
        answer.contains("200 OK"),
        "with the stream worker stuck, a command still has the worker kept for it, and it was \
         answered: {}",
        answer.lines().next().unwrap_or("nothing at all")
    );
    Premise::Held {
        server,
        stalled,
        began,
        dropped,
    }
}

#[test]
fn a_peer_that_stops_reading_gives_its_worker_slot_back() {
    let mut lost = Vec::new();
    let (server, stalled, began, dropped) = loop {
        match steps_one_to_three() {
            Premise::Held {
                server,
                stalled,
                began,
                dropped,
            } => break (server, stalled, began, dropped),
            Premise::WindowLost(after) => {
                lost.push(after);
                assert!(
                    lost.len() < ATTEMPTS,
                    "in {} runs this machine never reached step 3 inside the {:?} write \
                     timeout (it took {:?}), so the premise could not be checked at all",
                    ATTEMPTS,
                    WRITE_TIMEOUT,
                    lost
                );
            }
        }
    };
    let address = server.control.clone();

    // 4. And then the stuck worker's write times out, so the stream slot comes
    //    back without anybody doing anything.
    let deadline = Instant::now() + RECLAIM_DEADLINE;
    let mut last = String::new();
    let mut polite = None;
    while Instant::now() < deadline {
        match attach(&address) {
            Ok(socket) => {
                polite = Some(socket);
                break;
            }
            Err(refused) => last = refused,
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    let took = began.elapsed();
    assert!(
        polite.is_some(),
        "AC-11's fixed worker pool never got the slot back: {:?} after the stalled peer attached, \
         a subscriber is still refused because a peer that stopped reading is still holding a \
         worker inside `write`. The fanout dropped the subscriber ({}), but the WORKER was never \
         reclaimed. The last refusal:\n{}",
        took,
        dropped.lines().last().unwrap_or("").trim(),
        last.lines().last().unwrap_or("")
    );
    println!(
        "the stuck worker's slot came back {:?} after the stalled peer attached, with no operator \
         action{}",
        took,
        if lost.is_empty() {
            String::new()
        } else {
            format!(
                " (step 3's window was lost to scheduling {} time(s) first)",
                lost.len()
            )
        }
    );

    // The stalled peer is still open here, and dropping it is the test's own
    // cleanup rather than anything the server needed.
    drop(stalled);
    drop(polite);
}
