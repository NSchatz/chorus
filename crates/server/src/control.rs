//! The control channel: subscribers attach, commands apply, and the resulting
//! state goes to everybody.
//!
//! # Every thread this needs exists before the scheduling report
//!
//! This is the hazard `crates/server/src/main.rs` names in its own module
//! documentation and the one AC-12 is written about. `std::thread::spawn`
//! inherits the creating thread's scheduling policy, and
//! `deploy/run-server.sh` runs this binary with `--ulimit rtprio=20`, so a
//! control plane that spawned a thread per subscriber would be putting
//! real-time threads on a host that also runs Home Assistant and MQTT, none of
//! which the report the host contract is graded on had ever seen.
//!
//! So the shape here is `crates/server/src/clients.rs`'s shape, for the same
//! reason: a FIXED pool of worker threads, created before the report is taken,
//! each registering itself, and a connection arriving with every worker busy is
//! refused by name rather than served by a thread nobody declared. The whole
//! process is `4 + 2N + M` threads with the control plane on, against `3 + 2N`
//! with it off, and `crates/server/tests/control_thread_population.rs` grades
//! that against `/proc`.
//!
//! # It speaks HTTP, and why
//!
//! One port, three routes that matter: the page, `POST /api/command` carrying
//! one control message, and `GET /api/events`, which is a server-sent event
//! stream carrying one state message per change. A browser can open the last of
//! those with nothing but `EventSource`, and a shell script can open it with a
//! socket and a `GET` line, so THE UI AND THE VERIFICATION SCRIPTS ARE THE SAME
//! SUBSCRIBER. That is deliberate: a check that exercised a second, private
//! subscriber protocol would not be checking the thing the browser uses.
//!
//! The catalog is unchanged by that choice. `docs/control-plane.md` defines the
//! messages, `fixtures/control/` pins their bytes, and HTTP is the envelope
//! they arrive in.
//!
//! # What it is not
//!
//! There is no authentication, no authorisation and no TLS here, and the
//! listen address is configured rather than defaulted to anything reachable.
//! `docs/control-plane.md` says so out loud rather than leaving it to be
//! discovered.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use chorus_control::catalog::{decode_command, Refusal};
use chorus_control::fanout::ControlFanout;
use chorus_control::persist;
use chorus_control::zones::Zones;
use chorus_hostctl::ThreadRegistry;

use crate::hostreport::register_ordinary_thread;

/// Longest request line and header block the control channel will read.
///
/// A control message is a few hundred bytes and the largest legitimate request
/// here is a `POST` of one. This is what stops a peer that opens a connection
/// and never stops typing from being an allocation.
pub const MAX_REQUEST_BYTES: usize = 16 * 1024;

/// How long a worker waits on an idle subscriber queue before it looks up to
/// see whether it is still wanted.
///
/// The same interval `crate::stream` uses, for the same reason: a thread served
/// by a fixed pool has to be able to give its slot back.
const IDLE_WAKE: Duration = Duration::from_millis(200);

/// How often a held-open event stream sends a comment line.
///
/// A server-sent event stream that says nothing looks identical to one whose
/// connection has died, to a proxy and to a browser both. A `:` line is a
/// comment in the event-stream format and is what keeps it visibly alive; it
/// is also what makes a worker notice a peer that has gone, because writing to
/// a closed socket is the only way this end learns.
const KEEPALIVE: Duration = Duration::from_secs(15);

/// How long a write to a control peer may block before that peer is dropped.
///
/// [`chorus_control::fanout::ControlFanout`] answers the slow subscriber at the
/// APPLICATION layer: every subscriber's queue is bounded, and one that stops
/// consuming is removed at the ceiling with what it missed counted. That is the
/// whole of the criterion and it holds. It is not the whole of the hazard.
///
/// A peer that stops draining its TCP receive window - rather than closing -
/// blocks its worker inside `write` for as long as the kernel is willing to
/// wait, which is indefinitely. The fanout still drops the subscriber, but the
/// WORKER never gets back to `recv_timeout`, so its slot in the fixed pool is
/// never returned; enough such peers and the accept loop answers everyone
/// `503`, which is a fixed thread pool being denied to the people entitled to
/// it. The pool is fixed on purpose (`crates/server/src/main.rs`: every thread
/// exists before the scheduling report), so a stuck slot cannot be replaced by
/// growing the pool and has to be reclaimed instead.
///
/// This bounds it. A write that cannot make progress in this long is an error,
/// the connection is dropped and the slot comes back. Deliberately far longer
/// than any legitimate write here needs - a state message is a few hundred
/// bytes and the largest this server can build is tens of kilobytes - so a
/// merely slow peer is not cut off, and shorter than [`KEEPALIVE`] so a stuck
/// stream is reclaimed before the next comment line would have been due.
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// Why the control channel could not start.
#[derive(Debug)]
pub enum ControlRefused {
    /// The configured address could not be bound, or was denied.
    Bind {
        /// The address that was asked for.
        address: String,
        /// What the operating system said.
        cause: io::Error,
    },
    /// The persisted state file could not be read, or is not this format.
    State {
        /// The file that was read.
        path: String,
        /// What was wrong with it.
        detail: String,
    },
    /// The zones named on the command line are not a set of zones.
    Zones {
        /// What was wrong.
        detail: String,
    },
}

impl std::fmt::Display for ControlRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ControlRefused::Bind { address, cause } => write!(
                f,
                "the control channel address {} could not be bound: {}. This server will not \
                 serve audio while reporting itself as controllable, so it is stopping instead",
                address, cause
            ),
            ControlRefused::State { path, detail } => write!(
                f,
                "the persisted zone state at {} could not be read: {}. Starting with defaults \
                 would silently discard whatever a person had already set, so it is stopping \
                 instead",
                path, detail
            ),
            ControlRefused::Zones { detail } => write!(f, "{}", detail),
        }
    }
}

impl std::error::Error for ControlRefused {}

/// Everything the control plane holds, shared by every worker.
pub struct ControlState {
    zones: Mutex<Zones>,
    fanout: Arc<ControlFanout>,
    state_file: Option<PathBuf>,
    /// Commands applied since the process started.
    applied: AtomicU64,
    /// Commands refused since the process started.
    refused: AtomicU64,
    /// Connections turned away because every worker was busy.
    turned_away: AtomicU64,
    /// What the page is served with, so the UI is one artifact and not three.
    ui: Ui,
}

/// The files the browser is served: the page, what it loads, and the document
/// every region of the page links to. The document is served rather than only
/// committed, because a link to an explanation that answers 404 in a browser is
/// a link to nothing.
struct Ui {
    html: &'static str,
    css: &'static str,
    js: &'static str,
    doc: &'static str,
}

/// The policy the browser is handed with the page and with every asset the page
/// loads.
///
/// Tight enough to be worth having: nothing loads from anywhere but this origin,
/// there is no inline script or inline style for anything to be smuggled into,
/// and the page cannot be framed. Wide enough that the page still works: the
/// stylesheet and the script are served from here, and `connect-src` is what the
/// state request and the event stream travel on. A policy that silenced the page
/// would be a failure and not a pass, which is why the check that grades this
/// asserts the page still renders its zones and still updates under it.
///
/// It says nothing about what `POST /api/command` accepts. A Content-Security-
/// Policy constrains what a BROWSER may load and connect to; the control
/// listener's acceptance rules are `docs/control-plane.md`'s, and this listener
/// has no authentication either before or after this header, which is a fact
/// about the deployment and not something a response header can change.
const CONTENT_SECURITY_POLICY: &str = "default-src 'none'; script-src 'self'; style-src 'self'; \
     connect-src 'self'; img-src 'self' data:; base-uri 'none'; form-action 'none'; \
     frame-ancestors 'none'";

impl ControlState {
    /// Build the shared state.
    pub fn new(zones: Zones, state_file: Option<PathBuf>) -> ControlState {
        ControlState {
            zones: Mutex::new(zones),
            fanout: Arc::new(ControlFanout::new()),
            state_file,
            applied: AtomicU64::new(0),
            refused: AtomicU64::new(0),
            turned_away: AtomicU64::new(0),
            ui: Ui {
                html: include_str!("ui/index.html"),
                css: include_str!("ui/chorus.css"),
                js: include_str!("ui/chorus.js"),
                doc: include_str!("../../../docs/control-page.md"),
            },
        }
    }

    /// The fanout, for a caller that wants to report on it.
    pub fn fanout(&self) -> &Arc<ControlFanout> {
        &self.fanout
    }

    /// The state message as it stands.
    pub fn encoded_state(&self) -> String {
        self.locked().encode_state()
    }

    /// The line a run prints about what the control plane did.
    pub fn report(&self) -> String {
        format!(
            "control applied={} refused={} turned_away={} {}",
            self.applied.load(Ordering::Relaxed),
            self.refused.load(Ordering::Relaxed),
            self.turned_away.load(Ordering::Relaxed),
            self.fanout.report()
        )
    }

    fn locked(&self) -> std::sync::MutexGuard<'_, Zones> {
        match self.zones.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Apply one control message, persist the result and fan it out.
    ///
    /// The state is persisted BEFORE it is fanned out, so that a subscriber
    /// which has been told a change happened cannot be told something the disk
    /// would contradict after a restart.
    fn apply(&self, text: &str) -> Result<String, Refusal> {
        let command = decode_command(text)?;
        let (state, persist_error) = {
            let mut zones = self.locked();
            zones.apply(&command)?;
            let state = zones.encode_state();
            let mut persist_error = None;
            if let Some(path) = &self.state_file {
                if let Err(e) = persist::write_file(path, &zones) {
                    persist_error = Some(e.to_string());
                }
            }
            (state, persist_error)
        };
        if let Some(detail) = persist_error {
            // Reported and not swallowed, and not a reason to refuse the
            // command either: the change IS in force in this process, and
            // saying it was refused would be a lie in the other direction.
            eprintln!(
                "chorus-server: the zone state could not be persisted: {}. The change is in \
                 force in this process and will NOT survive a restart",
                detail
            );
        }
        self.applied.fetch_add(1, Ordering::Relaxed);
        self.fanout.broadcast(Arc::new(state.clone()));
        Ok(state)
    }

    /// Mark an endpoint as gone and fan out the result.
    fn endpoint_left(&self, endpoint: &str) {
        let state = {
            let mut zones = self.locked();
            if !zones.endpoint_left(endpoint) {
                return;
            }
            if let Some(path) = &self.state_file {
                let _ = persist::write_file(path, &zones);
            }
            zones.encode_state()
        };
        self.fanout.broadcast(Arc::new(state));
    }
}

/// One control worker's slot.
struct Slot {
    to_worker: SyncSender<TcpStream>,
}

/// The control channel's fixed thread pool and the listener it serves.
pub struct ControlPlane {
    listener: TcpListener,
    address: String,
    state: Arc<ControlState>,
    slots: Vec<Slot>,
    free: Receiver<usize>,
}

impl ControlPlane {
    /// Bind the configured address.
    ///
    /// Done before any thread is created and before the audio listener is
    /// bound, so a server that cannot be controlled never reaches the point of
    /// serving audio.
    pub fn bind(address: &str, state: Arc<ControlState>) -> Result<ControlPlane, ControlRefused> {
        let listener = TcpListener::bind(address).map_err(|cause| ControlRefused::Bind {
            address: address.to_string(),
            cause,
        })?;
        let bound = listener
            .local_addr()
            .map(|a| a.to_string())
            .unwrap_or_else(|_| address.to_string());
        Ok(ControlPlane {
            listener,
            address: bound,
            state,
            slots: Vec::new(),
            free: mpsc::channel().1,
        })
    }

    /// The address actually bound, which is the configured one with any
    /// ephemeral port resolved.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// The shared state.
    pub fn state(&self) -> &Arc<ControlState> {
        &self.state
    }

    /// Create every worker thread this plane will ever run.
    ///
    /// Each registers itself and sends one unit down `ready`, exactly as
    /// [`crate::clients::ClientPool`] does, so the caller can wait for the
    /// whole population before taking a scheduling report of it.
    pub fn spawn_workers(
        &mut self,
        workers: usize,
        keep: Arc<AtomicBool>,
        registry: Arc<ThreadRegistry>,
        ready: Sender<()>,
    ) {
        let (free_tx, free) = mpsc::channel::<usize>();
        for index in 0..workers {
            let (to_worker, jobs) = mpsc::sync_channel::<TcpStream>(1);
            let keep = Arc::clone(&keep);
            let registry = Arc::clone(&registry);
            let state = Arc::clone(&self.state);
            let ready = ready.clone();
            let returning = free_tx.clone();
            thread::spawn(move || {
                register_ordinary_thread(&format!("control-worker-{}", index), &registry);
                if ready.send(()).is_err() {
                    return;
                }
                drop(ready);
                for connection in jobs {
                    serve_connection(connection, &state, &keep);
                    if returning.send(index).is_err() {
                        return;
                    }
                }
            });
            self.slots.push(Slot { to_worker });
            if free_tx.send(index).is_err() {
                break;
            }
        }
        self.free = free;
    }

    /// How many worker threads this plane created.
    pub fn threads(&self) -> usize {
        self.slots.len()
    }

    /// Run the accept loop. Called on a thread the caller created before its
    /// scheduling report; this function creates none.
    pub fn accept_loop(self, keep: Arc<AtomicBool>) {
        let ControlPlane {
            listener,
            state,
            slots,
            free,
            ..
        } = self;
        while keep.load(Ordering::SeqCst) {
            let (connection, peer) = match listener.accept() {
                Ok(v) => v,
                Err(_) => return,
            };
            let _ = connection.set_nodelay(true);
            let index = match free.try_recv() {
                Ok(index) => index,
                Err(_) => {
                    state.turned_away.fetch_add(1, Ordering::Relaxed);
                    refuse_busy(connection, peer, slots.len());
                    continue;
                }
            };
            if slots[index].to_worker.send(connection).is_err() {
                return;
            }
        }
    }
}

/// Turn a connection away because every worker is busy.
///
/// The same argument `crates/server/src/clients.rs` makes about audio clients:
/// a server that grows a thread per connection has no bound on threads at all,
/// so the connection is answered honestly and closed.
fn refuse_busy(mut connection: TcpStream, peer: SocketAddr, workers: usize) {
    let body = format!(
        "{{\"v\":1,\"t\":\"error\",\"field\":\"\",\"detail\":\"every one of this server's {} \
         control workers is busy; try again\"}}",
        workers
    );
    let _ = write!(
        connection,
        "HTTP/1.1 503 Service Unavailable\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = connection.flush();
    eprintln!(
        "chorus-server: control connection refused peer={} reason=no-free-control-worker \
         workers={}",
        peer, workers
    );
}

/// One HTTP request, and the response to it.
struct Request {
    method: String,
    path: String,
    body: String,
}

fn read_request(reader: &mut BufReader<TcpStream>) -> Option<Request> {
    let mut line = String::new();
    if reader.read_line(&mut line).ok()? == 0 {
        return None;
    }
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();
    let mut length = 0usize;
    let mut read = line.len();
    loop {
        let mut header = String::new();
        let n = reader.read_line(&mut header).ok()?;
        read += n;
        if n == 0 || read > MAX_REQUEST_BYTES {
            return None;
        }
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().ok()?;
            }
        }
    }
    if length > MAX_REQUEST_BYTES {
        return None;
    }
    let mut body = vec![0u8; length];
    if length > 0 {
        reader.read_exact(&mut body).ok()?;
    }
    Some(Request {
        method,
        path,
        body: String::from_utf8_lossy(&body).to_string(),
    })
}

fn respond(connection: &mut TcpStream, status: &str, content_type: &str, body: &str) {
    respond_with(connection, status, content_type, body, "");
}

/// The same response, carrying the Content-Security-Policy. Everything a
/// BROWSER is handed goes through this one: the page, its stylesheet, its
/// script and the document its regions link to.
fn respond_to_browser(connection: &mut TcpStream, status: &str, content_type: &str, body: &str) {
    respond_with(
        connection,
        status,
        content_type,
        body,
        CONTENT_SECURITY_POLICY,
    );
}

fn respond_with(
    connection: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &str,
    policy: &str,
) {
    let policy_header = if policy.is_empty() {
        String::new()
    } else {
        format!("Content-Security-Policy: {}\r\n", policy)
    };
    let _ = write!(
        connection,
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\n{}Connection: close\r\n\r\n{}",
        status,
        content_type,
        body.len(),
        policy_header,
        body
    );
    let _ = connection.flush();
}

/// Serve one connection, whatever it turns out to be, and return the worker's
/// thread to the pool.
fn serve_connection(connection: TcpStream, state: &Arc<ControlState>, keep: &Arc<AtomicBool>) {
    let _ = connection.set_read_timeout(Some(Duration::from_secs(10)));
    // Both directions are bounded, and for the same reason: this worker's slot
    // in the fixed pool has to come back. See WRITE_TIMEOUT.
    let _ = connection.set_write_timeout(Some(WRITE_TIMEOUT));
    let reader_socket = match connection.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut reader = BufReader::new(reader_socket);
    let mut connection = connection;
    let request = match read_request(&mut reader) {
        Some(request) => request,
        None => {
            respond(
                &mut connection,
                "400 Bad Request",
                "application/json",
                "{\"v\":1,\"t\":\"error\",\"field\":\"\",\"detail\":\"this is not an HTTP request \
                 this server can read\"}",
            );
            return;
        }
    };
    let path = request.path.split('?').next().unwrap_or("/").to_string();
    match (request.method.as_str(), path.as_str()) {
        ("GET", "/") | ("GET", "/index.html") => respond_to_browser(
            &mut connection,
            "200 OK",
            "text/html; charset=utf-8",
            state.ui.html,
        ),
        ("GET", "/chorus.css") => respond_to_browser(
            &mut connection,
            "200 OK",
            "text/css; charset=utf-8",
            state.ui.css,
        ),
        ("GET", "/chorus.js") => respond_to_browser(
            &mut connection,
            "200 OK",
            "text/javascript; charset=utf-8",
            state.ui.js,
        ),
        // What every region of the page links to. The paragraphs explaining what
        // a figure counts live here rather than on the surface, and a link to an
        // explanation has to answer with the explanation.
        ("GET", "/docs/control-page.md") => respond_to_browser(
            &mut connection,
            "200 OK",
            "text/plain; charset=utf-8",
            state.ui.doc,
        ),
        ("GET", "/api/state") => respond(
            &mut connection,
            "200 OK",
            "application/json",
            &state.encoded_state(),
        ),
        // The bound's report half, over the wire. AC-11 asks that what a
        // dropped subscriber lost is counted AND reported, and a count that
        // only appears on the server's stdout at end of stream is not
        // reportable to anything that is running.
        ("GET", "/api/report") => respond(
            &mut connection,
            "200 OK",
            "text/plain; charset=utf-8",
            &format!("{}\n", state.report()),
        ),
        ("GET", "/api/events") => serve_events(connection, state, keep),
        ("POST", "/api/command") => {
            match state.apply(request.body.trim()) {
                Ok(applied) => respond(&mut connection, "200 OK", "application/json", &applied),
                Err(refusal) => {
                    state.refused.fetch_add(1, Ordering::Relaxed);
                    let status = if refusal.ends_the_session() {
                        // The version is not one this build implements, so the
                        // session is refused rather than the message.
                        "426 Upgrade Required"
                    } else {
                        "400 Bad Request"
                    };
                    respond(
                        &mut connection,
                        status,
                        "application/json",
                        &refusal.encode(),
                    );
                }
            }
        }
        ("POST", "/api/leaving") => {
            let endpoint = request.body.trim().to_string();
            state.endpoint_left(&endpoint);
            respond(&mut connection, "200 OK", "application/json", "{}");
        }
        _ => respond(
            &mut connection,
            "404 Not Found",
            "application/json",
            "{\"v\":1,\"t\":\"error\",\"field\":\"\",\"detail\":\"no such route\"}",
        ),
    }
}

/// Hold one server-sent event stream open, writing a state message per change.
///
/// The first thing written is the state as it stands, so a subscriber is never
/// waiting for a change to learn what is true now.
fn serve_events(mut connection: TcpStream, state: &Arc<ControlState>, keep: &Arc<AtomicBool>) {
    let inbox = state.fanout.subscribe();
    let opening = state.encoded_state();
    if write!(
        connection,
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\n\
         Connection: close\r\n\r\ndata: {}\n\n",
        opening
    )
    .is_err()
        || connection.flush().is_err()
    {
        return;
    }
    let mut since_keepalive = Duration::ZERO;
    while keep.load(Ordering::SeqCst) {
        match inbox.recv_timeout(IDLE_WAKE) {
            Ok(message) => {
                since_keepalive = Duration::ZERO;
                if write!(connection, "data: {}\n\n", message).is_err()
                    || connection.flush().is_err()
                {
                    return;
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                since_keepalive += IDLE_WAKE;
                if since_keepalive >= KEEPALIVE {
                    since_keepalive = Duration::ZERO;
                    if write!(connection, ": keepalive\n\n").is_err()
                        || connection.flush().is_err()
                    {
                        return;
                    }
                }
            }
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// Build the zone state a run starts from: the persisted file where there is
/// one, and the configured zones otherwise.
///
/// The two are not merged. A state file is the whole answer or it is not
/// consulted, because merging would mean deciding whether a zone in the file
/// and not on the command line had been deleted or had never been configured,
/// and there is no way to tell those apart.
/// `docs/decisions/0018-the-persisted-zone-state.md` records that.
pub fn initial_state(
    state_file: Option<&str>,
    configured: &[String],
    group_audio: &[(String, String)],
    default_audio: &str,
) -> Result<(Zones, PathBuf, bool), ControlRefused> {
    let path = PathBuf::from(state_file.unwrap_or("chorus-zones.state"));
    let mut loaded = None;
    if state_file.is_some() {
        match persist::read_file(&path, default_audio) {
            Ok(zones) => loaded = zones,
            Err(e) => {
                return Err(ControlRefused::State {
                    path: path.display().to_string(),
                    detail: e.to_string(),
                })
            }
        }
    }
    let from_file = loaded.is_some();
    let mut zones = match loaded {
        Some(zones) => zones,
        None => {
            let mut zones = Zones::new(default_audio);
            for id in configured {
                zones
                    .add(chorus_control::zones::Zone::new(id))
                    .map_err(|e| ControlRefused::Zones {
                        detail: format!("the zone '{}' was refused: {}", id, e),
                    })?;
            }
            zones
        }
    };
    for (group, address) in group_audio {
        zones.set_group_audio(group, address);
    }
    Ok((zones, path, from_file))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chorus_control::zones::Zone;

    #[test]
    fn a_command_that_is_applied_reaches_every_subscriber() {
        let mut zones = Zones::new("127.0.0.1:4010");
        zones.add(Zone::new("kitchen")).unwrap();
        let state = Arc::new(ControlState::new(zones, None));
        let a = state.fanout.subscribe();
        let b = state.fanout.subscribe();
        let applied = state
            .apply(r#"{"v":1,"t":"volume","zone":"kitchen","volume":0.250}"#)
            .expect("it applies");
        assert!(applied.contains(r#""volume":0.250"#), "{}", applied);
        assert_eq!(a.recv().unwrap().as_str(), applied);
        assert_eq!(
            b.recv().unwrap().as_str(),
            applied,
            "including the subscriber that did not send the command"
        );
    }

    #[test]
    fn a_command_that_is_refused_reaches_nobody() {
        let mut zones = Zones::new("127.0.0.1:4010");
        zones.add(Zone::new("kitchen")).unwrap();
        let state = Arc::new(ControlState::new(zones, None));
        let subscriber = state.fanout.subscribe();
        let before = state.encoded_state();
        assert!(state
            .apply(r#"{"v":1,"t":"volume","zone":"kitchen","volume":9.000}"#)
            .is_err());
        assert_eq!(state.encoded_state(), before);
        assert!(
            subscriber.try_recv().is_err(),
            "a refused command must fan nothing out"
        );
    }

    #[test]
    fn a_state_file_that_cannot_be_read_stops_the_run_rather_than_starting_from_defaults() {
        let mut path = std::env::temp_dir();
        path.push(format!("chorus-bad-state-{}.conf", std::process::id()));
        std::fs::write(&path, "format = 99\n").unwrap();
        let err = initial_state(
            Some(&path.display().to_string()),
            &[],
            &[],
            "127.0.0.1:4010",
        )
        .expect_err("a state file this build does not understand is refused");
        assert!(err.to_string().contains("could not be read"), "{}", err);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_state_file_that_is_not_there_yet_is_not_an_error() {
        let mut path = std::env::temp_dir();
        path.push(format!("chorus-absent-state-{}.conf", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let (zones, _, from_file) = initial_state(
            Some(&path.display().to_string()),
            &["kitchen".to_string()],
            &[],
            "127.0.0.1:4010",
        )
        .expect("a first run has no state file");
        assert!(!from_file);
        assert!(zones.zone("kitchen").is_some());
    }
}
