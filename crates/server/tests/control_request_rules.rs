//! What the control channel will read from a peer, and what it will act on.
//!
//! Audit findings B-4, B-5 and B-6 (`docs/audit/2026-09-audit.md`), graded over
//! real sockets against the real binary:
//!
//! - B-4: the 16 KiB bound holds for the WHOLE request. A line with no end is
//!   refused at the bound without the server growing, a body past the bound is
//!   refused unread, and a peer that trickles its request one byte at a time is
//!   cut off at the request deadline instead of holding a worker for ever.
//! - B-6: `POST /api/command` refuses a body not declared `application/json`
//!   (`415`) and a browser `Origin` that is not the server's own (`403`), and
//!   still applies what the control page and the endpoints send.
//! - B-5: event-stream subscribers cannot take the last worker, so with every
//!   worker a stream may have held, a command is still served.
//!
//! Every server here is started on port 0 and asked where it landed, so no
//! check hands a server a port it is not holding.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// A loopback address with the port left to the kernel.
const EPHEMERAL: &str = "127.0.0.1:0";

/// `crates/server/src/control.rs::REQUEST_DEADLINE`.
const REQUEST_DEADLINE: Duration = Duration::from_secs(5);

/// The longest any check here waits for something the server owes it.
const PATIENCE: Duration = Duration::from_secs(30);

const COMMAND: &str = r#"{"v":1,"t":"volume","zone":"kitchen","volume":0.250}"#;

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
    thread::spawn(move || {
        for line in BufReader::new(stream).lines().map_while(Result::ok) {
            let _ = tx.send(line);
        }
    });
}

/// Start the server on ports the kernel picks and read back where its control
/// channel landed, waiting until the audio socket is bound (which is after the
/// scheduling report, so every worker exists).
fn start(extra: &[&str]) -> Server {
    let mut command = Command::new(env!("CARGO_BIN_EXE_chorus-server"));
    command.args([
        "--listen",
        EPHEMERAL,
        "--source",
        "tone",
        "--serve-forever",
        "--allow-non-realtime",
        "--allow-unlocked-memory",
        "--ephemeral-identity",
        "--control-listen",
        EPHEMERAL,
        "--zone",
        "kitchen",
    ]);
    command.args(extra);
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
        if ready {
            if let Some(control) = control {
                server.control = control;
                return server;
            }
        }
    }
    panic!("the server never said where it is listening: {:?}", said);
}

/// Everything the peer is sent back until it closes, the read times out or it
/// is reset, keeping whatever arrived before any of those.
fn read_all(socket: &mut TcpStream) -> String {
    let mut seen = Vec::new();
    let mut scratch = [0u8; 4_096];
    loop {
        match socket.read(&mut scratch) {
            Ok(0) | Err(_) => break,
            Ok(read) => seen.extend_from_slice(&scratch[..read]),
        }
    }
    String::from_utf8_lossy(&seen).to_string()
}

fn exchange(address: &str, request: &str) -> String {
    let mut socket = TcpStream::connect(address).expect("the control channel is listening");
    socket.set_read_timeout(Some(PATIENCE)).unwrap();
    socket.set_write_timeout(Some(PATIENCE)).unwrap();
    let _ = socket.write_all(request.as_bytes());
    read_all(&mut socket)
}

/// The accept loop's own refusal, which a check that is not about the pool
/// retries past: a worker hands its slot back just after it closes the
/// connection it served, so the next connection can arrive a moment early.
fn pool_was_momentarily_full(answer: &str) -> bool {
    answer.is_empty()
        || (answer.contains("503 Service Unavailable") && !answer.contains("event streams is held"))
}

/// A request served by a worker, retried past the moment described at
/// [`pool_was_momentarily_full`] but never past anything else.
fn served(address: &str, request: &str) -> String {
    let started = Instant::now();
    loop {
        let answer = exchange(address, request);
        if !pool_was_momentarily_full(&answer) || started.elapsed() >= PATIENCE {
            return answer;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn post(address: &str, headers: &str, body: &str) -> String {
    served(
        address,
        &format!(
            "POST /api/command HTTP/1.1\r\nHost: {}\r\n{}Content-Length: {}\r\n\
             Connection: close\r\n\r\n{}",
            address,
            headers,
            body.len(),
            body
        ),
    )
}

fn state_of(address: &str) -> String {
    served(
        address,
        &format!(
            "GET /api/state HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            address
        ),
    )
}

fn status_line(answer: &str) -> &str {
    answer.lines().next().unwrap_or("nothing at all")
}

/// The server's peak resident set, from the kernel.
fn peak_rss_kib(server: &Server) -> u64 {
    let status = std::fs::read_to_string(format!("/proc/{}/status", server.child.id()))
        .expect("the server is alive and /proc is mounted");
    status
        .lines()
        .find_map(|line| {
            let rest = line.strip_prefix("VmHWM:")?;
            rest.split_whitespace().next()?.parse().ok()
        })
        .expect("a VmHWM line")
}

#[test]
fn a_line_with_no_end_is_refused_at_the_bound_without_the_server_growing() {
    let server = start(&[]);
    // Settle the baseline: the first requests are what fault the workers'
    // buffers in, and they are not what this check is about.
    assert!(state_of(&server.control).contains("200 OK"));
    let before = peak_rss_kib(&server);

    // Far more than loopback socket buffers can hold between them, so a server
    // that stops reading at the bound stops the sender well short of it.
    const SENT: usize = 64 * 1024 * 1024;
    let mut socket = TcpStream::connect(&server.control).unwrap();
    socket.set_read_timeout(Some(PATIENCE)).unwrap();
    let mut writer = socket.try_clone().unwrap();
    writer
        .set_write_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let sender = thread::spawn(move || {
        let mut sent = 0usize;
        if writer.write_all(b"GET /").is_err() {
            return sent;
        }
        let chunk = vec![b'a'; 64 * 1024];
        while sent < SENT {
            match writer.write(&chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => sent += n,
            }
        }
        sent
    });
    let answer = read_all(&mut socket);
    drop(socket);
    let sent = sender.join().unwrap();
    assert!(
        answer.starts_with("HTTP/1.1 431 Request Header Fields Too Large"),
        "a request line with no end has to be refused at the bound; after {} bytes the server \
         answered: {}",
        sent,
        status_line(&answer)
    );
    assert!(answer.contains("16384 bytes"), "{}", answer);
    assert!(
        sent < SENT,
        "the server kept reading all {} bytes instead of stopping at the bound",
        SENT
    );

    let after = peak_rss_kib(&server);
    assert!(
        after.saturating_sub(before) < 2 * 1024,
        "the server's peak resident set grew from {} KiB to {} KiB reading a line it should have \
         cut at 16 KiB",
        before,
        after
    );
    // And the worker came back.
    assert!(state_of(&server.control).contains("200 OK"));
}

#[test]
fn a_header_block_past_the_bound_and_a_body_past_it_are_refused() {
    let server = start(&[]);
    let mut head = format!("GET /api/state HTTP/1.1\r\nHost: {}\r\n", server.control);
    while head.len() <= 16 * 1024 {
        head.push_str("X-Padding: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\r\n");
    }
    head.push_str("\r\n");
    let answer = served(&server.control, &head);
    assert!(
        answer.starts_with("HTTP/1.1 431 "),
        "{}",
        status_line(&answer)
    );

    let answer = served(
        &server.control,
        &format!(
            "POST /api/command HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\n\
             Content-Length: 20000\r\nConnection: close\r\n\r\n{}",
            server.control, COMMAND
        ),
    );
    assert!(
        answer.starts_with("HTTP/1.1 413 "),
        "a body that would take the request past the bound is refused before it is read: {}",
        status_line(&answer)
    );
}

#[test]
fn a_request_that_trickles_in_is_cut_off_at_the_deadline() {
    let server = start(&[]);
    let mut socket = TcpStream::connect(&server.control).unwrap();
    socket.set_read_timeout(Some(PATIENCE)).unwrap();
    let mut writer = socket.try_clone().unwrap();
    let began = Instant::now();
    let (stop_tx, stop) = mpsc::channel::<()>();
    let trickle = thread::spawn(move || {
        let _ = writer.write_all(b"GET /api/state HTTP/1.1\r\nX-Slow: ");
        // One byte at a time, each well inside any per-read timeout, for far
        // longer than the request deadline.
        let mut sent = 0usize;
        while stop.recv_timeout(Duration::from_millis(100)).is_err() {
            if writer.write_all(b"a").is_err() {
                break;
            }
            sent += 1;
            if sent > 600 {
                break;
            }
        }
    });
    let answer = read_all(&mut socket);
    let took = began.elapsed();
    let _ = stop_tx.send(());
    trickle.join().unwrap();
    assert!(
        answer.starts_with("HTTP/1.1 408 Request Timeout"),
        "a peer trickling one byte every 100 ms has to be cut off at the request deadline; after \
         {:?} the server answered: {}",
        took,
        status_line(&answer)
    );
    assert!(
        took >= REQUEST_DEADLINE,
        "cut off after {:?}, before the {:?} deadline",
        took,
        REQUEST_DEADLINE
    );
    // The worker it held is back.
    assert!(state_of(&server.control).contains("200 OK"));
}

#[test]
fn a_command_not_declared_json_is_refused_and_changes_nothing() {
    let server = start(&[]);
    let before = state_of(&server.control);
    for headers in ["Content-Type: text/plain\r\n", ""] {
        let answer = post(&server.control, headers, COMMAND);
        assert!(
            answer.starts_with("HTTP/1.1 415 Unsupported Media Type"),
            "a command sent with {:?} was answered: {}",
            headers,
            status_line(&answer)
        );
    }
    assert_eq!(
        state_of(&server.control),
        before,
        "a refused command changed the state"
    );
}

#[test]
fn a_command_from_another_origin_is_refused_and_changes_nothing() {
    let server = start(&[]);
    let before = state_of(&server.control);
    for origin in ["http://elsewhere.example", "null", "http://127.0.0.1:1"] {
        let answer = post(
            &server.control,
            &format!("Content-Type: application/json\r\nOrigin: {}\r\n", origin),
            COMMAND,
        );
        assert!(
            answer.starts_with("HTTP/1.1 403 Forbidden"),
            "a command from Origin {} was answered: {}",
            origin,
            status_line(&answer)
        );
    }
    assert_eq!(
        state_of(&server.control),
        before,
        "a refused command changed the state"
    );
}

#[test]
fn what_the_page_and_the_endpoints_send_is_still_applied() {
    let server = start(&[]);
    // The control page: same origin, and `fetch` adds a charset to nothing
    // here but a client may, so the parameter is allowed.
    let page = post(
        &server.control,
        &format!(
            "Content-Type: application/json; charset=utf-8\r\nOrigin: http://{}\r\n",
            server.control
        ),
        COMMAND,
    );
    assert!(page.contains("200 OK"), "{}", status_line(&page));
    assert!(page.contains(r#""volume":0.250"#), "{}", page);
    // An endpoint (`crates/client-linux/src/control.rs`): no Origin at all.
    let endpoint = post(
        &server.control,
        "Content-Type: application/json\r\n",
        r#"{"v":1,"t":"volume","zone":"kitchen","volume":0.500}"#,
    );
    assert!(endpoint.contains("200 OK"), "{}", status_line(&endpoint));
    assert!(endpoint.contains(r#""volume":0.500"#), "{}", endpoint);
}

/// Attach an event-stream subscriber and return the socket once its stream is
/// served, or what came back instead.
fn attach(address: &str) -> Result<TcpStream, String> {
    let started = Instant::now();
    loop {
        let mut socket = TcpStream::connect(address).expect("the control channel is listening");
        socket.set_read_timeout(Some(PATIENCE)).unwrap();
        let _ = write!(
            socket,
            "GET /api/events HTTP/1.1\r\nHost: {}\r\nAccept: text/event-stream\r\n\r\n",
            address
        );
        let mut opening = String::new();
        let mut scratch = [0u8; 4_096];
        loop {
            match socket.read(&mut scratch) {
                Ok(0) | Err(_) => break,
                Ok(read) => {
                    opening.push_str(&String::from_utf8_lossy(&scratch[..read]));
                    // A refusal is read to its end, which is close by: the
                    // connection is being closed.
                    if opening.contains("\r\n\r\ndata: ") {
                        break;
                    }
                }
            }
        }
        if opening.starts_with("HTTP/1.1 200 OK") && opening.contains("\r\n\r\ndata: ") {
            return Ok(socket);
        }
        if !pool_was_momentarily_full(&opening) || started.elapsed() >= PATIENCE {
            return Err(opening);
        }
        thread::sleep(Duration::from_millis(20));
    }
}

/// Audit finding B-5, closed: an event stream is held by the one event writer
/// (`crates/server/src/events.rs`), not by a worker, so a server with ONE
/// control worker holds every stream its ceiling allows and still serves a
/// command beside them; past the ceiling a stream is refused by name.
#[test]
fn subscribers_cost_no_worker_and_past_their_ceiling_are_refused_by_name() {
    const WORKERS: usize = 1;
    const STREAMS: usize = 3;
    let server = start(&[
        "--control-workers",
        &WORKERS.to_string(),
        "--event-streams",
        &STREAMS.to_string(),
    ]);
    let held: Vec<TcpStream> = (0..STREAMS)
        .map(|n| {
            attach(&server.control)
                .unwrap_or_else(|e| panic!("subscriber {} was not served: {}", n, e))
        })
        .collect();
    let refused = match attach(&server.control) {
        Ok(_) => panic!(
            "a subscriber past the ceiling of {} event streams was served",
            STREAMS
        ),
        Err(answer) => answer,
    };
    assert!(
        refused.starts_with("HTTP/1.1 503 Service Unavailable")
            && refused.contains("every one of this server's 3 event streams is held"),
        "{}",
        refused
    );
    let answer = post(
        &server.control,
        "Content-Type: application/json\r\n",
        COMMAND,
    );
    assert!(
        answer.contains("200 OK"),
        "with {} subscribers held and {} control worker a command was answered: {}",
        held.len(),
        WORKERS,
        status_line(&answer)
    );
    drop(held);
}
