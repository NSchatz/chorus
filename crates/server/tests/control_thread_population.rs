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
//!
//! # Taking workers from a fixed pool to ask about a fixed pool
//!
//! The property above is only visible from a connection, and every connection
//! here is served by one of the control plane's fixed workers. A connection that
//! arrives while all of them are busy is answered `503 Service Unavailable` and
//! closed - correctly, and that refusal is itself graded below - so an assertion
//! judged on whatever came back is an assertion about which thread the kernel
//! ran first.
//!
//! So no assertion here is judged on an answer until the control plane has
//! SERVED it. Where these checks require service they wait for it, boundedly,
//! and where the pool will not come back they fail naming the refusal, the
//! ceiling in force and how many streams were being held. The waiting is
//! synchronised on what the control plane already says about itself - the
//! subscriber count at `GET /api/report` - and on a worker holding a stream
//! whose subscriber has gone it is provoked rather than waited on, because such
//! a worker learns that its peer left only by writing to it. What is never done
//! is the two things that would hide the hazard instead of grading it: the
//! worker ceiling is not raised to buy headroom, and the checks are not
//! serialized.
//!
//! # No check here hands a server a port nobody is holding
//!
//! The other way these checks could be decided by something other than the
//! property they grade is the sockets. They run four at a time in one binary,
//! and one of them deliberately HOLDS a loopback port for its whole run, so a
//! port this process binds, reads and releases in order to pass the number to a
//! server is a port anything in this binary can take in between. The server then
//! exits on a bind it could not make, and the check sees a server that never
//! said it was listening: a red run with nothing to do with AC-12.
//!
//! So no port here is released and then handed over. Every server is started on
//! [`EPHEMERAL`] and asked where it landed, which the server already says of
//! both its sockets; the process that binds a socket is the process that holds
//! it, and there is no interval to lose it in. The one check that needs a port
//! of its own holds it from before its server starts until after it has exited.

mod common;

use std::cell::Cell;
use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// The control worker ceiling the first check runs against, and the client
/// slots beside it.
///
/// `docs/verification-record.md` records this shape for AC-12 and the expected
/// thread population is derived from these two numbers rather than written out,
/// so a run at another ceiling grades the same property.
const DEFAULT_CONTROL_WORKERS: usize = 4;
const MAX_CLIENTS: usize = 2;

/// Event streams the first check holds open while it issues commands.
///
/// Fewer than the workers on purpose here, and more than them later: an event
/// stream is held by the one event writer (`crates/server/src/events.rs`,
/// audit finding B-5), never by a worker, so the count of streams held says
/// nothing about the pool and the population must not move with it.
const HELD_STREAMS: usize = 3;

/// The control plane's own answer to a connection it has no worker for.
const BUSY_STATUS: &str = "503 Service Unavailable";
const BUSY_REASON: &str = "control workers is busy";

/// The longest these checks will wait for the control plane to serve something
/// they require it to serve.
///
/// Far longer than any of it takes when the pool is healthy - a worker hands its
/// slot back the instant it has answered - and short enough that a pool which is
/// not coming back is a failure in seconds rather than a run that never ends.
const SERVICE_DEADLINE: Duration = Duration::from_secs(10);

/// How long a retry listens for an answer the control plane volunteers before
/// sending its request, and how long it then waits for the rest of that answer.
/// See [`volunteered`].
const REFUSAL_PEEK: Duration = Duration::from_millis(25);
const REFUSAL_TAIL: Duration = Duration::from_millis(200);

/// How long a connection that sends nothing is watched for a refusal before it
/// is taken as picked up by a worker. Longer than [`REFUSAL_PEEK`], because a
/// wrong call here makes the pool look fuller than it is.
const SILENT_PEEK: Duration = Duration::from_millis(250);

/// The event writer's own answer to a stream past its ceiling.
const STREAMS_HELD_REASON: &str = "event streams is held";

/// How long to wait for an answer the control plane owes.
const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// How long a retry pauses, and how many retries apart the pool is provoked.
const RETRY_PAUSE: Duration = Duration::from_millis(10);
const PROVOKE_EVERY: usize = 5;

/// How long a thread that was going to be created gets to appear before a
/// population is compared.
///
/// The assertions this precedes are negative - nothing new exists - and a
/// negative asserted the instant after the event is an assertion about the
/// scheduler.
const SETTLE: Duration = Duration::from_millis(300);

/// A loopback address with the port left to the kernel.
///
/// The server resolves it when it binds and prints where it landed, for the
/// audio socket (`crates/server/src/main.rs`, `listener.local_addr()`) and for
/// the control channel (`ControlPlane::address()`) alike, so nothing here has to
/// choose a port on its behalf. See the module note on ports.
const EPHEMERAL: &str = "127.0.0.1:0";

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

/// Carry one of the child's streams into `tx`, line by line, labelled.
///
/// BOTH streams are carried, and into the same channel. The server reports every
/// refusal on stderr (`report` in `crates/server/src/main.rs`) and says it
/// stopped on stdout, so a run that discarded stderr would be a run whose
/// failure could not say why it failed: that is how a server which could not
/// bind a socket reached this file as nothing but "it never said it was
/// listening". The label is what keeps the two apart in a quoted failure, and it
/// is a prefix so that no stderr line can be mistaken for one of the stdout
/// lines these checks read.
fn pump<R: Read + Send + 'static>(
    stream: R,
    tx: mpsc::Sender<String>,
    label: &'static str,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        for line in BufReader::new(stream).lines().map_while(Result::ok) {
            if tx.send(format!("{}{}", label, line)).is_err() {
                return;
            }
        }
    })
}

/// Start the server on kernel-assigned ports and read its output until it says
/// it is listening for audio, which is after the scheduling report has been
/// printed.
///
/// The two sockets are asked for as [`EPHEMERAL`] and read back off the lines
/// the server prints about them, so this process never hands over a port it is
/// not holding. See the module note on ports.
fn start(extra: &[String]) -> (Server, u32, mpsc::Receiver<String>, Vec<String>) {
    let mut args = vec!["--ephemeral-identity".to_string()];
    args.extend(extra.iter().cloned());
    start_with_identity(&args)
}

/// [`start`] with the identity flags left to `extra`: for `--upnp`, which
/// needs a persisted identity (`--identity-dir`) and refuses a throwaway one.
fn start_with_identity(extra: &[String]) -> (Server, u32, mpsc::Receiver<String>, Vec<String>) {
    let mut args = vec![
        "--source".to_string(),
        "tone".to_string(),
        "--tone-ms".to_string(),
        "30000".to_string(),
        "--allow-non-realtime".to_string(),
        "--allow-unlocked-memory".to_string(),
        "--listen".to_string(),
        EPHEMERAL.to_string(),
        "--control-listen".to_string(),
        EPHEMERAL.to_string(),
    ];
    args.extend(extra.iter().cloned());

    let mut child = Command::new(env!("CARGO_BIN_EXE_chorus-server"))
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the server binary runs");
    let pid = child.id();
    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");
    let server = Server(child);
    let (tx, rest) = mpsc::channel::<String>();
    let on_stdout = pump(stdout, tx.clone(), "");
    let on_stderr = pump(stderr, tx, "stderr: ");

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
            // Both pipes are closed, which is this server gone. Whatever it said
            // on its way out is in `startup`, stderr included.
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    drop(on_stdout);
    drop(on_stderr);
    panic!(
        "the server never reported a bound socket; it said: {:?}",
        startup
    );
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
        .unwrap_or_else(|| {
            panic!(
                "the server never said where its control channel is: {:?}",
                startup
            )
        })
}

/// The audio address the server printed, with its ephemeral port resolved.
///
/// The same line `start` waits for, read for the address rather than the fact of
/// it: `main.rs` prints `local_addr()` here, so this is where the kernel put the
/// socket and not what was asked for.
fn audio_address(startup: &[String]) -> String {
    startup
        .iter()
        .find_map(|line| {
            let at = line.find("chorus-server: listening on=")?;
            let rest = &line[at + "chorus-server: listening on=".len()..];
            Some(rest.split_whitespace().next()?.to_string())
        })
        .unwrap_or_else(|| {
            panic!(
                "the server never said where its audio socket is: {:?}",
                startup
            )
        })
}

/// The ceiling the first check's control plane is started with.
///
/// `CHORUS_DETERMINISM_WORKERS` is how `tools/control-determinism.sh` starves it
/// on purpose. Set below what this check attaches and commands, there is no
/// scheduling in which the control plane can serve it, so the check has to go
/// red saying so; a repeated run in which that starved run went green would be a
/// repeated run of checks that had stopped needing the pool at all.
fn control_workers() -> usize {
    knob(
        "CHORUS_DETERMINISM_WORKERS",
        DEFAULT_CONTROL_WORKERS,
        "control worker count",
    )
}

/// The event-stream ceiling the first check's control plane is started with:
/// the server's own default unless `CHORUS_DETERMINISM_STREAMS` starves it,
/// which is the other way `tools/control-determinism.sh` makes these checks
/// unservable on purpose (fewer streams than [`HELD_STREAMS`]).
fn event_streams() -> Option<usize> {
    std::env::var("CHORUS_DETERMINISM_STREAMS")
        .ok()
        .map(|_| knob("CHORUS_DETERMINISM_STREAMS", 0, "event-stream ceiling"))
}

fn knob(name: &str, default: usize, what: &str) -> usize {
    match std::env::var(name) {
        Err(_) => default,
        Ok(value) => match value.trim().parse::<usize>() {
            Ok(n) if n > 0 => n,
            _ => panic!(
                "{} is {:?}, which is not a {}. It starves the control plane on purpose and has \
                 to be a whole number above zero",
                name, value, what
            ),
        },
    }
}

/// What one attempt at a request got.
enum Attempt<T> {
    /// The control plane served it. Whatever it answered - applied, refused by
    /// the catalog, anything - is an answer this check is entitled to judge.
    Served(T),
    /// It was not served, verbatim, because a failure has to name what came
    /// back rather than summarise it.
    NotServed(String),
}

fn get_request(path: &str) -> String {
    format!(
        "GET {} HTTP/1.1\r\nHost: chorus\r\nConnection: close\r\n\r\n",
        path
    )
}

fn command_request(body: &str) -> String {
    format!(
        "POST /api/command HTTP/1.1\r\nHost: chorus\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
}

fn volume_body(zone: &str, volume: &str) -> String {
    format!(
        r#"{{"v":1,"t":"volume","zone":"{}","volume":{}}}"#,
        zone, volume
    )
}

fn one_line(text: &str) -> String {
    text.replace(['\r', '\n'], " ").trim().to_string()
}

/// Anything the control plane says before it has been asked.
///
/// The accept loop answers a connection it has no worker for and closes it
/// WITHOUT reading the request. A request written into that connection is unread
/// data at close, which the kernel answers with a reset, and the reset discards
/// the very answer that said why - which is how a busy pool reaches a check as a
/// broken pipe instead of as the refusal it is. Listening first is what keeps
/// the refusal readable, so a failure can name it.
///
/// `None` means nothing was volunteered in the window, which is what a
/// connection a worker has picked up looks like: that worker is waiting for the
/// request.
fn volunteered(socket: &mut TcpStream, window: Duration) -> Option<String> {
    let _ = socket.set_read_timeout(Some(window));
    let mut scratch = [0u8; 4_096];
    let mut seen = match socket.read(&mut scratch) {
        Ok(0) => {
            let _ = socket.set_read_timeout(Some(READ_TIMEOUT));
            return Some("the control channel closed the connection without answering".to_string());
        }
        Ok(read) => String::from_utf8_lossy(&scratch[..read]).to_string(),
        Err(_) => {
            let _ = socket.set_read_timeout(Some(READ_TIMEOUT));
            return None;
        }
    };
    // An answer arrives in as many pieces as the network felt like, and a
    // refusal quoted as far as its first packet would be a refusal quoted
    // without the sentence that says why. Read to the end of it, which is close
    // by: this connection is being closed.
    let _ = socket.set_read_timeout(Some(REFUSAL_TAIL));
    loop {
        match socket.read(&mut scratch) {
            Ok(0) => break,
            Ok(read) => seen.push_str(&String::from_utf8_lossy(&scratch[..read])),
            Err(_) => break,
        }
    }
    let _ = socket.set_read_timeout(Some(READ_TIMEOUT));
    Some(seen)
}

/// One request on a fresh connection, and what came back.
fn exchange(address: &str, request: &str, peek: bool) -> Attempt<String> {
    let mut socket = match TcpStream::connect(address) {
        Ok(socket) => socket,
        Err(cause) => {
            return Attempt::NotServed(format!(
                "the control channel did not accept a connection: {}",
                cause
            ))
        }
    };
    let _ = socket.set_read_timeout(Some(READ_TIMEOUT));
    if peek {
        if let Some(early) = volunteered(&mut socket, REFUSAL_PEEK) {
            return Attempt::NotServed(early);
        }
    }
    if let Err(cause) = write!(socket, "{}", request).and_then(|()| socket.flush()) {
        return Attempt::NotServed(format!(
            "the control channel closed the connection before the request could go up ({}), \
             which is what it does to a connection it has no worker for",
            cause
        ));
    }
    let mut answer = String::new();
    let read = socket.read_to_string(&mut answer);
    if answer.contains(BUSY_STATUS) {
        return Attempt::NotServed(answer);
    }
    if answer.is_empty() {
        return Attempt::NotServed(match read {
            Err(cause) => format!("the control channel gave nothing back: {}", cause),
            Ok(_) => "the control channel closed the connection without answering".to_string(),
        });
    }
    Attempt::Served(answer)
}

/// Attach one event-stream subscriber, and hand back the socket only if the
/// attachment was SERVED.
///
/// A refused attachment answers bytes too, and bytes coming back is exactly what
/// it would take to count one as attached. What is required here is the served
/// event stream: the status line, the content type, and the opening state every
/// subscriber is sent before any change.
fn attach(address: &str, peek: bool) -> Attempt<TcpStream> {
    let mut socket = match TcpStream::connect(address) {
        Ok(socket) => socket,
        Err(cause) => {
            return Attempt::NotServed(format!(
                "the control channel did not accept a connection: {}",
                cause
            ))
        }
    };
    let _ = socket.set_read_timeout(Some(READ_TIMEOUT));
    if peek {
        if let Some(early) = volunteered(&mut socket, REFUSAL_PEEK) {
            return Attempt::NotServed(early);
        }
    }
    if let Err(cause) =
        write!(socket, "{}", get_request("/api/events")).and_then(|()| socket.flush())
    {
        return Attempt::NotServed(format!(
            "the control channel closed the connection before the request could go up ({}), \
             which is what it does to a connection it has no worker for",
            cause
        ));
    }
    let mut opening = String::new();
    let mut scratch = [0u8; 4_096];
    loop {
        match socket.read(&mut scratch) {
            Ok(0) => break,
            Ok(read) => {
                opening.push_str(&String::from_utf8_lossy(&scratch[..read]));
                if opening.contains(BUSY_STATUS) || opening.contains("\r\n\r\ndata: ") {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    if opening.contains(BUSY_STATUS) {
        return Attempt::NotServed(opening);
    }
    if !opening.starts_with("HTTP/1.1 200 OK")
        || !opening.contains("text/event-stream")
        || !opening.contains("\r\n\r\ndata: ")
    {
        return Attempt::NotServed(format!(
            "an attachment was answered with '{}', which is not a served event stream",
            one_line(&opening)
        ));
    }
    Attempt::Served(socket)
}

/// The control plane these checks are talking to, and what they are holding on
/// it.
struct Plane {
    address: String,
    /// The ceiling this plane was started with. Named in every failure, because
    /// "every worker is busy" means a different thing at two workers and at
    /// forty.
    workers: usize,
    /// The event-stream ceiling, named in every failure for the same reason;
    /// `None` is the server's default.
    streams: Option<usize>,
    /// A zone whose volume can be set to provoke the pool.
    zone: String,
    /// Event streams these checks are holding open right now.
    held: Cell<usize>,
}

impl Plane {
    fn new(address: String, workers: usize, streams: Option<usize>, zone: &str) -> Plane {
        Plane {
            address,
            workers,
            streams,
            zone: zone.to_string(),
            held: Cell::new(0),
        }
    }

    /// Establish that the control plane serves this, and hand back what it
    /// answered.
    ///
    /// The wait is bounded and the failure is loud: no attempt count is
    /// unlimited, nothing here blocks for ever, and a pool that never comes back
    /// is a red check naming the refusal rather than a green one.
    fn serve<T>(&self, what: &str, mut attempt: impl FnMut(bool) -> Attempt<T>) -> T {
        let started = Instant::now();
        let mut refusals: Vec<String> = Vec::new();
        let mut tries = 0usize;
        loop {
            tries += 1;
            // The first attempt asks straight out; a retry listens first, so
            // that the refusal it is retrying past is readable.
            match attempt(tries > 1) {
                Attempt::Served(answer) => return answer,
                Attempt::NotServed(refusal) => {
                    let refusal = one_line(&refusal);
                    if !refusals.contains(&refusal) {
                        refusals.push(refusal);
                    }
                }
            }
            if started.elapsed() >= SERVICE_DEADLINE {
                panic!(
                    "the control plane refused to serve {}, and this check requires it to be \
                     served.\n  attempts:                 {} over {:?}\n  \
                     control workers in force: {}\n  event streams in force:   {}\n  \
                     attachments held:         {}\n  what came back:\n{}",
                    what,
                    tries,
                    started.elapsed(),
                    self.workers,
                    self.streams
                        .map(|n| n.to_string())
                        .unwrap_or_else(|| "the default".to_string()),
                    self.held.get(),
                    refusals
                        .iter()
                        .map(|refusal| format!("    {}\n", refusal))
                        .collect::<String>()
                );
            }
            if tries.is_multiple_of(PROVOKE_EVERY) {
                self.provoke();
            }
            thread::sleep(RETRY_PAUSE);
        }
    }

    /// One command, served.
    fn command(&self, body: &str) -> String {
        let request = command_request(body);
        self.serve(&format!("the command {}", body), |peek| {
            exchange(&self.address, &request, peek)
        })
    }

    /// One event-stream subscriber, attached and held open.
    fn subscriber(&self) -> Subscriber<'_> {
        let socket = self.serve("an event-stream attachment", |peek| {
            attach(&self.address, peek)
        });
        self.held.set(self.held.get() + 1);
        Subscriber {
            socket,
            plane: self,
        }
    }

    /// Make a worker that is holding a stream whose subscriber has gone notice
    /// that it has.
    ///
    /// Such a worker is parked waiting for something to send, and writing is the
    /// only way this end learns the peer left. A command that IS APPLIED is
    /// fanned out to every subscriber, so it is what gives those slots back
    /// inside a check rather than at the next keepalive. Best effort by
    /// definition: if this was refused too then the pool had nothing free, and
    /// the caller is going to try again anyway.
    fn provoke(&self) {
        let _ = exchange(
            &self.address,
            &command_request(&volume_body(&self.zone, "0.500")),
            false,
        );
    }

    /// How many event-stream subscribers the control plane says it has.
    ///
    /// Its own count, at `GET /api/report`. A subscriber is in it from the
    /// moment its stream is served until the worker holding that stream is done
    /// with it, which makes it the thing to synchronise on: it moves when the
    /// pool moves, and it is already part of the shipped control plane.
    fn attached(&self) -> usize {
        let request = get_request("/api/report");
        let answer = self.serve("a report of what is attached", |peek| {
            exchange(&self.address, &request, peek)
        });
        let at = answer
            .find("subscribers=")
            .unwrap_or_else(|| panic!("no subscriber count in the report: {}", one_line(&answer)));
        let rest = &answer[at + "subscribers=".len()..];
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        digits
            .parse()
            .unwrap_or_else(|_| panic!("no subscriber count in the report: {}", one_line(&answer)))
    }

    /// Wait until the control plane says exactly this many subscribers are
    /// attached, provoking the pool so a worker holding a departed one gives its
    /// slot back.
    fn wait_until_attached(&self, wanted: usize) {
        let started = Instant::now();
        loop {
            let seen = self.attached();
            if seen == wanted {
                return;
            }
            if started.elapsed() >= SERVICE_DEADLINE {
                // One fact, one spelling. `serve` above names the ceiling and
                // the attachments held in exactly these words and
                // `tools/control-determinism.sh` requires a starved run to say
                // them, so a failure that reached the deadline here rather than
                // there has to be readable by the same reader.
                panic!(
                    "the control plane says {} event-stream subscribers are attached after {:?}, \
                     and this check needs {}.\n  \
                     control workers in force: {}\n  attachments held:         {}",
                    seen,
                    started.elapsed(),
                    wanted,
                    self.workers,
                    self.held.get()
                );
            }
            self.provoke();
            thread::sleep(RETRY_PAUSE);
        }
    }
}

/// A held-open event-stream subscriber, on a real socket, established as served.
///
/// The socket is held and never read again: what matters is that it stays open,
/// because a subscriber that has gone is not the one this test is about.
struct Subscriber<'a> {
    #[allow(dead_code)]
    socket: TcpStream,
    plane: &'a Plane,
}

impl Drop for Subscriber<'_> {
    fn drop(&mut self) {
        self.plane.held.set(self.plane.held.get() - 1);
    }
}

/// What the control plane answers a connection arriving past the ceiling.
///
/// Listens before asking, for [`volunteered`]'s reason: this connection is
/// expected to be turned away without its request ever being read, and writing
/// into it first is what would cost the answer.
fn answer_past_the_ceiling(address: &str) -> String {
    let mut socket = TcpStream::connect(address).expect("the listener accepts it");
    let _ = socket.set_read_timeout(Some(READ_TIMEOUT));
    if let Some(answer) = volunteered(&mut socket, READ_TIMEOUT) {
        return answer;
    }
    // Nothing was volunteered, so a worker has this connection after all. Ask,
    // and let the assertion say what came back.
    write!(socket, "{}", get_request("/api/state")).expect("the request goes up");
    socket.flush().unwrap();
    let mut answer = String::new();
    let _ = socket.read_to_string(&mut answer);
    answer
}

/// The population with the control plane on: 1 supervisor + 1 audio + 1
/// acceptor + 2 per client slot + 1 control acceptor + 1 per control worker +
/// the event writer + the conductor. Not a function of `--slots`, nor of how
/// many endpoints or subscribers there are. The TV relay (with `--slots`) and
/// the MQTT publisher (with `--mqtt-broker`) are one more each, and the
/// players (`--players P`, goal 16) are P more, added where a test turns them
/// on.
fn population(max_clients: usize, workers: usize) -> usize {
    1 + 1 + 1 + 2 * max_clients + 1 + workers + 1 + 1
}

/// Every row the report must carry, by role.
fn assert_roles(rows: &[ReportedThread], workers: usize, startup: &[String]) {
    assert!(!rows.is_empty(), "no thread rows at all: {:?}", startup);
    assert!(
        !rows.iter().any(|r| r.role == "unregistered"),
        "a thread nobody declared is running: {:?}",
        rows
    );
    let roles: BTreeSet<String> = rows.iter().map(|r| r.role.clone()).collect();
    for wanted in [
        "supervisor",
        "audio",
        "acceptor",
        "control-acceptor",
        "event-writer",
        "conductor",
    ] {
        assert!(roles.contains(wanted), "no {} row in {:?}", wanted, roles);
    }
    for index in 0..workers {
        assert!(
            roles.contains(&format!("control-worker-{}", index)),
            "no control-worker-{} row in {:?}",
            index,
            roles
        );
    }
}

#[test]
fn the_control_plane_creates_every_thread_it_will_run_before_the_report_is_taken() {
    let workers = control_workers();
    let streams = event_streams();
    let mut extra = vec![
        "--max-clients".to_string(),
        MAX_CLIENTS.to_string(),
        "--control-workers".to_string(),
        workers.to_string(),
        "--zone".to_string(),
        "kitchen".to_string(),
        "--zone".to_string(),
        "study".to_string(),
    ];
    if let Some(n) = streams {
        extra.extend(["--event-streams".to_string(), n.to_string()]);
    }
    let (server, pid, _rest, startup) = start(&extra);

    let rows = reported(&startup);
    assert_roles(&rows, workers, &startup);
    let expected = population(MAX_CLIENTS, workers);
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
    let plane = Plane::new(control_address(&startup), workers, streams, "kitchen");
    let subscribers: Vec<Subscriber> = (0..HELD_STREAMS).map(|_| plane.subscriber()).collect();
    // Every one of them is attached, on the control plane's own count, before
    // anything is asserted about what holding them did.
    plane.wait_until_attached(subscribers.len());
    for zone in ["kitchen", "study"] {
        let response = plane.command(&volume_body(zone, "0.500"));
        assert!(response.contains("200 OK"), "{}", response);
    }
    // A refused command too, which takes a different path through the worker.
    let refused = plane.command(&volume_body("none", "0.500"));
    assert!(refused.contains("400 Bad Request"), "{}", refused);

    thread::sleep(SETTLE);
    let during = kernel_threads(pid);
    assert_eq!(
        during,
        before,
        "serving {} control subscribers created a thread the scheduling report never saw",
        subscribers.len()
    );

    drop(subscribers);
    // Gone from the control plane's own count, which is the same moment the
    // event writer is done with each of them.
    plane.wait_until_attached(0);
    thread::sleep(SETTLE);
    let after = kernel_threads(pid);
    assert_eq!(
        after, before,
        "subscribers leaving changed the thread population"
    );

    // And the same again, with the audio path busy, because that is the
    // combination the deployment actually runs. More streams than there are
    // workers this time: a stream holds no worker (B-5), so a command is
    // still served beside them all.
    let mut client = common::v2_client(audio_address(&startup).as_str(), READ_TIMEOUT);
    let mut scratch = vec![0u8; 65_536];
    assert!(client.reader.read(&mut scratch).expect("audio comes down") > 0);
    let held: Vec<Subscriber> = (0..workers + 1).map(|_| plane.subscriber()).collect();
    let response = plane.command(&volume_body("kitchen", "0.250"));
    assert!(response.contains("200 OK"), "{}", response);
    thread::sleep(SETTLE);
    assert_eq!(
        kernel_threads(pid),
        before,
        "a client, {} subscribers and a command together created a thread nobody declared",
        held.len()
    );
    drop(held);
    drop(server);
}

#[test]
fn the_population_does_not_depend_on_the_number_of_stream_slots() {
    let mut counts = Vec::new();
    for slots in [1usize, 8] {
        let (server, pid, _rest, startup) = start(&[
            "--max-clients".to_string(),
            MAX_CLIENTS.to_string(),
            "--control-workers".to_string(),
            "2".to_string(),
            "--slots".to_string(),
            slots.to_string(),
            "--zone".to_string(),
            "kitchen".to_string(),
            "--zone".to_string(),
            "study".to_string(),
        ]);
        let rows = reported(&startup);
        assert_roles(&rows, 2, &startup);
        let threads = kernel_threads(pid);
        let declared: BTreeSet<u32> = rows.iter().map(|r| r.tid).collect();
        assert_eq!(declared, threads, "--slots {}: {:?}", slots, rows);
        // With slots the line-ins exist, and with them the TV relay's one
        // thread (goal 13, `chorus_server::tvrelay`): still not a function
        // of S.
        assert!(
            rows.iter().any(|r| r.role == "tv-relay"),
            "no tv-relay row in {:?}",
            rows
        );
        assert_eq!(
            threads.len(),
            population(MAX_CLIENTS, 2) + 1,
            "--slots {} runs {} threads: {:?}",
            slots,
            threads.len(),
            rows
        );
        // A session on a slot, and the population again.
        let mut client = common::v2_client(audio_address(&startup).as_str(), READ_TIMEOUT);
        let mut scratch = vec![0u8; 65_536];
        assert!(client.reader.read(&mut scratch).expect("audio comes down") > 0);
        thread::sleep(SETTLE);
        assert_eq!(
            kernel_threads(pid),
            threads,
            "--slots {}: a session made a thread",
            slots
        );
        counts.push(threads.len());
        drop(server);
    }
    assert_eq!(counts[0], counts[1], "S = 1 and S = 8 run the same threads");
    println!(
        "population with --slots 1 and --slots 8: {} threads each (6 + 2N + M + 1, N={}, M=2)",
        counts[0], MAX_CLIENTS
    );
}

/// (goal 15) The MQTT publisher is one more declared, ordinary thread with
/// `--mqtt-broker`, and does not exist without it: `6 + 2N + M + 1` against
/// `6 + 2N + M`. The broker here is a loopback listener that never accepts
/// and never answers, which is the hardest case for the rule: the publisher
/// is stuck waiting on it, and that must cost no thread and nothing else.
#[test]
fn the_mqtt_publisher_is_one_declared_thread_with_its_flag_and_none_without() {
    let stalled_broker = std::net::TcpListener::bind(EPHEMERAL).expect("a loopback port");
    let broker = stalled_broker.local_addr().unwrap().to_string();
    let shape = |mqtt: bool| {
        let mut extra = vec![
            "--max-clients".to_string(),
            MAX_CLIENTS.to_string(),
            "--control-workers".to_string(),
            "2".to_string(),
            "--zone".to_string(),
            "kitchen".to_string(),
        ];
        if mqtt {
            extra.extend(["--mqtt-broker".to_string(), broker.clone()]);
        }
        let (server, pid, _rest, startup) = start(&extra);
        let rows = reported(&startup);
        assert_roles(&rows, 2, &startup);
        let threads = kernel_threads(pid);
        let declared: BTreeSet<u32> = rows.iter().map(|r| r.tid).collect();
        assert_eq!(declared, threads, "mqtt={}: {:?}", mqtt, rows);
        let publishers = rows.iter().filter(|r| r.role == "mqtt-publisher").count();
        assert_eq!(publishers, usize::from(mqtt), "mqtt={}: {:?}", mqtt, rows);
        assert_eq!(
            threads.len(),
            population(MAX_CLIENTS, 2) + usize::from(mqtt),
            "mqtt={} runs {} threads: {:?}",
            mqtt,
            threads.len(),
            rows
        );
        // A command, a session and a broker that is not answering: the
        // population again.
        let plane = Plane::new(control_address(&startup), 2, None, "kitchen");
        let response = plane.command(&volume_body("kitchen", "0.500"));
        assert!(response.contains("200 OK"), "{}", response);
        let mut client = common::v2_client(audio_address(&startup).as_str(), READ_TIMEOUT);
        let mut scratch = vec![0u8; 65_536];
        assert!(client.reader.read(&mut scratch).expect("audio comes down") > 0);
        thread::sleep(SETTLE);
        assert_eq!(
            kernel_threads(pid),
            threads,
            "mqtt={}: a command, a session or the broker made a thread",
            mqtt
        );
        drop(server);
        threads.len()
    };
    let (off, on) = (shape(false), shape(true));
    assert_eq!(on, off + 1);
    println!(
        "population without --mqtt-broker: {} threads (6 + 2N + M, N={}, M=2); with it: {}",
        off, MAX_CLIENTS, on
    );
}

/// (goal 16) `--upnp` is `4 + W` more declared, ordinary threads (`upnp-ssdp`,
/// `upnp-acceptor`, `upnp-events`, `upnp-manager` and one `upnp-worker-<i>`
/// per `--upnp-workers` W), created with the rest, and none without the flag.
/// A control point that searches, reads a description, calls an action and
/// subscribes makes no thread: the event to its listener goes out on
/// `upnp-events`, which exists already.
#[test]
fn the_upnp_renderers_are_declared_threads_with_their_flag_and_none_without() {
    const WORKERS: usize = 3;
    let dir = std::env::temp_dir().join(format!("chorus-upnp-threads-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a directory for the identity");
    let notify = std::net::UdpSocket::bind(EPHEMERAL).expect("a loopback port");
    let group = notify.local_addr().unwrap().to_string();
    let shape = |upnp: bool| {
        let mut extra: Vec<String> = [
            "--max-clients",
            &MAX_CLIENTS.to_string(),
            "--control-workers",
            "2",
            "--slots",
            "2",
            "--players",
            "1",
            "--zone",
            "kitchen",
            "--zone",
            "study",
            "--identity-dir",
            &dir.display().to_string(),
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        if upnp {
            extra.extend(
                [
                    "--upnp",
                    "--upnp-listen",
                    "127.0.0.1:0",
                    "--upnp-workers",
                    &WORKERS.to_string(),
                    "--upnp-ssdp-port",
                    "0",
                    "--upnp-ssdp-group",
                    &group,
                ]
                .iter()
                .map(|s| s.to_string()),
            );
        }
        let (server, pid, _rest, startup) = start_with_identity(&extra);
        let rows = reported(&startup);
        assert_roles(&rows, 2, &startup);
        let threads = kernel_threads(pid);
        let declared: BTreeSet<u32> = rows.iter().map(|r| r.tid).collect();
        assert_eq!(declared, threads, "upnp={}: {:?}", upnp, rows);
        let roles: Vec<&str> = rows
            .iter()
            .map(|r| r.role.as_str())
            .filter(|r| r.starts_with("upnp-"))
            .collect();
        if upnp {
            let mut wanted = vec!["upnp-acceptor", "upnp-events", "upnp-manager", "upnp-ssdp"];
            let workers: Vec<String> = (0..WORKERS).map(|i| format!("upnp-worker-{i}")).collect();
            wanted.extend(workers.iter().map(String::as_str));
            let mut got = roles.clone();
            got.sort_unstable();
            wanted.sort_unstable();
            assert_eq!(got, wanted);
        } else {
            assert!(roles.is_empty(), "{:?}", roles);
        }
        // 1 for the TV relay (`--slots`), 1 for the player.
        assert_eq!(
            threads.len(),
            population(MAX_CLIENTS, 2) + 2 + if upnp { 4 + WORKERS } else { 0 },
            "upnp={} runs {} threads: {:?}",
            upnp,
            threads.len(),
            rows
        );
        if upnp {
            // A control point's whole first contact: a description, an
            // action and a subscription whose initial event is delivered.
            let line = startup
                .iter()
                .find(|l| l.contains("upnp renderers listening on="))
                .expect("the renderers' line");
            let http = line
                .split("listening on=")
                .nth(1)
                .and_then(|r| r.split_whitespace().next())
                .unwrap()
                .to_string();
            // The kitchen's UDN, made the way the server makes it: from the
            // fingerprint of its key and the target's name.
            let key = startup
                .iter()
                .find_map(|l| l.split(" key=").nth(1))
                .and_then(|r| r.split_whitespace().next())
                .expect("the identity line");
            let udn = chorus_upnp::uuid::udn(
                &chorus_upnp::uuid::CHORUS_NAMESPACE,
                key,
                &chorus_upnp::uuid::Target::Room("kitchen"),
            );
            let ask = |request: String| {
                let mut socket = TcpStream::connect(&http).expect("the renderers listen");
                let _ = socket.set_read_timeout(Some(READ_TIMEOUT));
                socket.write_all(request.as_bytes()).unwrap();
                let mut answer = String::new();
                let _ = socket.read_to_string(&mut answer);
                answer
            };
            // The manager makes the renderers once it runs: ask until the
            // kitchen's is there.
            let deadline = Instant::now() + SERVICE_DEADLINE;
            let description = loop {
                let answer = ask(format!(
                    "GET /upnp/{udn}/desc.xml HTTP/1.1\r\nHOST: x\r\n\r\n"
                ));
                if answer.contains("200 OK") || Instant::now() >= deadline {
                    break answer;
                }
                thread::sleep(RETRY_PAUSE);
            };
            assert!(description.contains("MediaRenderer:1"), "{}", description);
            let listener = std::net::TcpListener::bind(EPHEMERAL).unwrap();
            let subscribed = ask(format!(
                "SUBSCRIBE /upnp/{udn}/rcs/event HTTP/1.1\r\nHOST: x\r\nCALLBACK: <http://{}/cb>\r\nNT: upnp:event\r\n\r\n",
                listener.local_addr().unwrap()
            ));
            assert!(subscribed.contains("200 OK"), "{}", subscribed);
            let (mut event, _) = listener.accept().expect("the initial event arrives");
            let mut scratch = [0u8; 16];
            assert!(event.read(&mut scratch).expect("a NOTIFY") > 0);
        }
        thread::sleep(SETTLE);
        assert_eq!(
            kernel_threads(pid),
            threads,
            "upnp={}: a control point made a thread",
            upnp
        );
        drop(server);
        threads.len()
    };
    let (off, on) = (shape(false), shape(true));
    assert_eq!(on, off + 4 + WORKERS);
    println!(
        "population without --upnp: {} threads; with it and --upnp-workers {}: {} (4 + W more)",
        off, WORKERS, on
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// (goal 17) `--soloist-receivers R` is `R + 1` more declared, ordinary
/// threads, `soloist-reader-0` to `soloist-reader-<R-1>` and
/// `soloist-manager`, created with the rest whether or not any receiver
/// container runs (none does here: the receiver directory is empty), and
/// none without the flag: `6 + 2N + M + 1 + R + 1` against `6 + 2N + M + 1`.
/// A session hearing a room makes no thread. What receivers that come,
/// are assigned and play do to the population (nothing) is
/// `soloist_receivers.rs`'s, which runs real supervisors.
#[test]
fn the_soloist_receivers_are_declared_threads_with_their_flag_and_none_without() {
    let dir = std::env::temp_dir().join(common::fresh_id("chorus-population-soloist"));
    std::fs::create_dir_all(&dir).unwrap();
    let shape = |receivers: usize| {
        let mut extra = vec![
            "--max-clients".to_string(),
            MAX_CLIENTS.to_string(),
            "--control-workers".to_string(),
            "2".to_string(),
            "--slots".to_string(),
            "2".to_string(),
            "--zone".to_string(),
            "kitchen".to_string(),
        ];
        if receivers > 0 {
            extra.extend([
                "--soloist-dir".to_string(),
                dir.display().to_string(),
                "--soloist-receivers".to_string(),
                receivers.to_string(),
            ]);
        }
        let (server, pid, _rest, startup) = start(&extra);
        let rows = reported(&startup);
        assert_roles(&rows, 2, &startup);
        let threads = kernel_threads(pid);
        let declared: BTreeSet<u32> = rows.iter().map(|r| r.tid).collect();
        assert_eq!(declared, threads, "receivers={}: {:?}", receivers, rows);
        let named: BTreeSet<String> = rows
            .iter()
            .filter(|r| r.role.starts_with("soloist"))
            .map(|r| r.role.clone())
            .collect();
        let mut wanted: BTreeSet<String> = (0..receivers)
            .map(|i| format!("soloist-reader-{}", i))
            .collect();
        if receivers > 0 {
            wanted.insert("soloist-manager".to_string());
        }
        assert_eq!(named, wanted, "receivers={}: {:?}", receivers, rows);
        assert_eq!(
            threads.len(),
            population(MAX_CLIENTS, 2) + 1 + wanted.len(),
            "receivers={} runs {} threads: {:?}",
            receivers,
            threads.len(),
            rows
        );
        assert_eq!(
            startup
                .iter()
                .any(|l| l.contains("chorus-server: soloist ")),
            receivers > 0,
            "the receivers line is said only with the flag: {:?}",
            startup
        );
        let mut client = common::v2_client(audio_address(&startup).as_str(), READ_TIMEOUT);
        let mut scratch = vec![0u8; 65_536];
        assert!(client.reader.read(&mut scratch).expect("audio comes down") > 0);
        thread::sleep(SETTLE);
        assert_eq!(
            kernel_threads(pid),
            threads,
            "receivers={}: waiting for receivers that are not there made a thread",
            receivers
        );
        drop(server);
        threads.len()
    };
    let (off, on) = (shape(0), shape(3));
    assert_eq!(on, off + 4);
    assert_eq!(
        std::fs::read_dir(&dir).unwrap().count(),
        0,
        "the server made nothing in the receiver directory"
    );
    println!(
        "population without --soloist-receivers: {} threads (6 + 2N + M + 1, N={}, M=2); with \
         --soloist-receivers 3: {}",
        off, MAX_CLIENTS, on
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// (goal 16) `--players P` is P more declared, ordinary threads, `player-0`
/// to `player-<P-1>`, created with the rest whether or not anything plays,
/// and none without the flag: `6 + 2N + M + 1 + P` against `6 + 2N + M + 1`
/// (the one is the TV relay, which `--slots` brings and `--players` needs).
/// A group taking a player, and a session hearing it, make no thread: a
/// thread per stream is what the contract forbids.
#[test]
fn the_players_are_declared_threads_with_their_flag_and_none_without() {
    let shape = |players: usize| {
        let mut extra = vec![
            "--max-clients".to_string(),
            MAX_CLIENTS.to_string(),
            "--control-workers".to_string(),
            "2".to_string(),
            "--slots".to_string(),
            "2".to_string(),
            "--zone".to_string(),
            "kitchen".to_string(),
            "--zone".to_string(),
            "study".to_string(),
        ];
        if players > 0 {
            extra.extend(["--players".to_string(), players.to_string()]);
        }
        let (server, pid, _rest, startup) = start(&extra);
        let rows = reported(&startup);
        assert_roles(&rows, 2, &startup);
        let threads = kernel_threads(pid);
        let declared: BTreeSet<u32> = rows.iter().map(|r| r.tid).collect();
        assert_eq!(declared, threads, "players={}: {:?}", players, rows);
        let named: BTreeSet<String> = rows
            .iter()
            .filter(|r| r.role.starts_with("player"))
            .map(|r| r.role.clone())
            .collect();
        let wanted: BTreeSet<String> = (0..players).map(|i| format!("player-{}", i)).collect();
        assert_eq!(named, wanted, "players={}: {:?}", players, rows);
        assert_eq!(
            threads.len(),
            population(MAX_CLIENTS, 2) + 1 + players,
            "players={} runs {} threads: {:?}",
            players,
            threads.len(),
            rows
        );
        assert_eq!(
            startup
                .iter()
                .any(|l| l.contains("chorus-server: players ")),
            players > 0,
            "the players line is said only with the flag: {:?}",
            startup
        );
        // Each room takes a player (refused by name where there is none), a
        // session hears one, and the population again.
        let plane = Plane::new(control_address(&startup), 2, None, "kitchen");
        for (room, player) in [("kitchen", "p0"), ("study", "p1")] {
            let response = plane.command(&format!(
                r#"{{"v":2,"t":"take","target":"{}","source":"player:{}"}}"#,
                room, player
            ));
            if players > 0 {
                assert!(response.contains("200 OK"), "{}", response);
                assert!(
                    response.contains(&format!(r#""source":"player:{}""#, player)),
                    "{}",
                    response
                );
            } else {
                assert!(response.contains("400 Bad Request"), "{}", response);
                assert!(
                    response.contains(&format!("there is no player '{}' on this server", player)),
                    "{}",
                    response
                );
            }
        }
        let mut client = common::v2_client(audio_address(&startup).as_str(), READ_TIMEOUT);
        let mut scratch = vec![0u8; 65_536];
        assert!(client.reader.read(&mut scratch).expect("audio comes down") > 0);
        thread::sleep(SETTLE);
        assert_eq!(
            kernel_threads(pid),
            threads,
            "players={}: a take or a session made a thread",
            players
        );
        drop(server);
        threads.len()
    };
    let (off, on) = (shape(0), shape(3));
    assert_eq!(on, off + 3);
    println!(
        "population without --players: {} threads (6 + 2N + M + 1, N={}, M=2); with --players \
         3: {}",
        off, MAX_CLIENTS, on
    );
}

#[test]
fn a_subscriber_past_the_ceiling_is_refused_by_name_rather_than_served_by_a_new_thread() {
    let (server, pid, _rest, startup) = start(&[
        "--max-clients".to_string(),
        "1".to_string(),
        "--control-workers".to_string(),
        "1".to_string(),
        "--event-streams".to_string(),
        "2".to_string(),
        "--zone".to_string(),
        "kitchen".to_string(),
    ]);
    let plane = Plane::new(control_address(&startup), 1, Some(2), "kitchen");
    let before = kernel_threads(pid);
    assert_eq!(before.len(), population(1, 1));

    // Both of the event writer's streams held, on a server with ONE control
    // worker: a stream costs no worker, so a command is still served.
    let held: Vec<Subscriber> = (0..2).map(|_| plane.subscriber()).collect();
    let response = plane.command(&volume_body("kitchen", "0.300"));
    assert!(response.contains("200 OK"), "{}", response);

    // The third stream is past the ceiling: answered by name and closed,
    // rather than held by a thread that did not exist at report time.
    // Asked until a worker answers it: with ONE worker, the connection can
    // arrive a moment before that worker has handed its slot back from the
    // command above, which is the pool's refusal and not the one graded here.
    let body = plane.serve("the event writer's answer past its ceiling", |_| {
        let (status, body) = common::http(plane.address.as_str(), &get_request("/api/events"));
        if body.contains(BUSY_REASON) {
            Attempt::NotServed(body)
        } else {
            Attempt::Served(format!("{} {}", status, body))
        }
    });
    assert!(body.contains(BUSY_STATUS), "{}", body);
    assert!(
        body.contains(STREAMS_HELD_REASON) && body.contains("2 event streams"),
        "the refusal has to say why: {}",
        body
    );

    // And the worker pool's own ceiling: the one worker held by a connection
    // that has not sent its request, so the next connection has no worker.
    let silent = plane.serve("a connection a worker picks up", |_| {
        let mut socket = match TcpStream::connect(plane.address.as_str()) {
            Ok(socket) => socket,
            Err(cause) => return Attempt::NotServed(cause.to_string()),
        };
        match volunteered(&mut socket, SILENT_PEEK) {
            Some(refusal) => Attempt::NotServed(refusal),
            None => Attempt::Served(socket),
        }
    });
    let response = answer_past_the_ceiling(plane.address.as_str());
    assert!(
        response.contains(BUSY_STATUS),
        "a connection past the ceiling has to be refused by name, and this came back: {:?}",
        response
    );
    assert!(
        response.contains(BUSY_REASON),
        "the refusal has to say why: {:?}",
        response
    );
    assert_eq!(
        kernel_threads(pid),
        before,
        "refusing a stream and a connection created a thread"
    );
    drop(silent);
    drop(held);
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
    let taken = std::net::TcpListener::bind(EPHEMERAL).expect("a loopback port");
    let address = taken.local_addr().unwrap().to_string();
    // And the audio address is one this test holds too, from before the server
    // starts until after it has exited. That is the race-free form of "nothing
    // was ever bound to it": a port released and looked at again afterwards
    // could have been taken by anything in between, and the checks in this
    // binary that start servers are exactly such an anything. Held, it cannot
    // have been bound by this server, and a server that had got as far as its
    // audio listener would have failed ON IT and said so - naming it, with the
    // transport exit code instead of the control one. The assertions below are
    // what would catch that.
    let audio = std::net::TcpListener::bind(EPHEMERAL).expect("a loopback port");
    let audio_address = audio.local_addr().unwrap().to_string();

    let output = Command::new(env!("CARGO_BIN_EXE_chorus-server"))
        .args([
            "--listen",
            &audio_address,
            "--source",
            "tone",
            "--allow-non-realtime",
            "--allow-unlocked-memory",
            "--ephemeral-identity",
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
    assert!(
        said.contains(&address),
        "the refusal has to name the address: {}",
        said
    );
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
    // The audio socket was never even reached, which is the strongest form of
    // "did not serve audio". This check holds that address, so a server that had
    // tried to bind it would have been refused and would have reported that
    // refusal by name - and it says nothing about it at all.
    assert!(
        !said.contains("the listen address could not be bound"),
        "the server stopped at the control channel, before the audio socket: {}",
        said
    );
    assert!(
        !said.contains(&audio_address),
        "the audio address was never reached, so nothing should name it: {}",
        said
    );

    drop(audio);
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
    std::fs::write(
        &path,
        "format = 1\nserial = 1\n\n[zone kitchen]\nname = K\n",
    )
    .unwrap();

    // Both addresses are left to the kernel, and neither is ever bound: the
    // state file is read before the control channel is bound, which is the
    // ordering this check is about.
    let output = Command::new(env!("CARGO_BIN_EXE_chorus-server"))
        .args([
            "--listen",
            EPHEMERAL,
            "--source",
            "tone",
            "--allow-non-realtime",
            "--allow-unlocked-memory",
            "--ephemeral-identity",
            "--control-listen",
            EPHEMERAL,
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
