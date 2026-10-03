//! The supervisor proper: one thread that owns every decision, fed by
//! threads that each read one thing.
//!
//! The readers: the socket's acceptor, one reader per server connection,
//! one monitor per child process, two output readers per child process, and
//! the WebSocket client. Each sends a [`Msg`] and decides nothing. The main
//! thread takes messages with a short timeout, so its timers (the retry
//! backoff, SIGTERM to SIGKILL, the daily expiry check, its own SIGTERM)
//! need no thread of their own.

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chorus_control::json;
use chorus_soloist::api;
use chorus_soloist::build::{self, Expiry};
use chorus_soloist::keydir;
use chorus_soloist::protocol::{
    self, BuildReport, FromSupervisor, LineBuffer, ProtocolError, State, StatusReport, ToSupervisor,
};
use chorus_soloist::{
    lock_file_name, node_name, pcm_file_name, socket_file_name, WS_ADDR_FILE, WS_PORT_FILE,
};

use crate::args::{Config, PipewireMode};
use crate::log::Log;
use crate::pipewire::{self, Layout};
use crate::sys;
use crate::wsclient::{self, WsReport};
use crate::{EXIT_DIRECTORY, EXIT_NO_INDEX, EXIT_PIPEWIRE};

/// How long the main thread waits for a message before looking at its
/// timers.
const TICK: Duration = Duration::from_millis(25);

/// How often a child process is looked at for its exit.
const CHILD_POLL: Duration = Duration::from_millis(10);

/// The longest a write to the server may take before the connection is
/// dropped: the server reads promptly or reconnects.
const SERVER_WRITE_TIMEOUT: Duration = Duration::from_secs(2);

/// How long `soloist --version` may take.
const VERSION_TIMEOUT: Duration = Duration::from_secs(10);

/// How long PipeWire has to create its socket.
const PIPEWIRE_START_TIMEOUT: Duration = Duration::from_secs(10);

/// The longest line of a child's output passed through in one piece.
const MAX_OUTPUT_LINE: usize = 16 * 1024;

/// What the reader threads tell the main thread.
enum Msg {
    /// The server connected.
    Connected(UnixStream),
    /// A line from connection `0`.
    Line(u64, String),
    /// Connection `0` ended; with a reason when it was dropped for one.
    Disconnected(u64, Option<String>),
    /// Run `0`'s Soloist exited with this code (`None`: killed by a signal).
    SoloistExited(u64, Option<i32>),
    /// Run `0`'s WebSocket client reports.
    Ws(u64, WsReport),
    /// A PipeWire process of helper generation `0` exited.
    HelperExited(u64, &'static str, Option<i32>),
}

/// A child process with a monitor thread and its output passed through the
/// log.
struct Proc {
    child: Arc<Mutex<Child>>,
    pid: u32,
    /// One message from each output reader when its stream ends.
    drained: Receiver<()>,
}

impl Proc {
    fn spawn(
        mut command: Command,
        label: &'static str,
        log: &Log,
        on_exit: impl FnOnce(Option<i32>) + Send + 'static,
    ) -> io::Result<Proc> {
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn()?;
        let pid = child.id();
        let (done, drained) = mpsc::channel();
        if let Some(out) = child.stdout.take() {
            pass_through(out, label, log.clone(), done.clone());
        }
        if let Some(err) = child.stderr.take() {
            pass_through(err, label, log.clone(), done);
        }
        let child = Arc::new(Mutex::new(child));
        let watched = Arc::clone(&child);
        thread::spawn(move || {
            let code = loop {
                match lock(&watched).try_wait() {
                    Ok(Some(status)) => break status.code(),
                    Ok(None) => {}
                    Err(_) => break None,
                }
                thread::sleep(CHILD_POLL);
            };
            on_exit(code);
        });
        Ok(Proc {
            child,
            pid,
            drained,
        })
    }

    /// SIGTERM, if the process has not been reaped. The lock is what makes
    /// the process id safe to signal: the monitor reaps under the same lock.
    fn terminate(&self) {
        let mut child = lock(&self.child);
        if matches!(child.try_wait(), Ok(None)) {
            let _ = sys::terminate(self.pid);
        }
    }

    /// SIGKILL, if the process has not been reaped.
    fn kill(&self) {
        let mut child = lock(&self.child);
        if matches!(child.try_wait(), Ok(None)) {
            let _ = child.kill();
        }
    }

    fn exited(&self) -> bool {
        !matches!(lock(&self.child).try_wait(), Ok(None))
    }

    /// SIGTERM, then SIGKILL after `grace`; returns once the process is gone.
    fn stop(&self, grace: Duration) {
        self.terminate();
        let deadline = Instant::now() + grace;
        while !self.exited() {
            if Instant::now() >= deadline {
                self.kill();
            }
            thread::sleep(CHILD_POLL);
        }
        // The last lines the process wrote are still on their way to the
        // log; wait for both streams to end, but not for ever (a process it
        // left behind could hold them open).
        let deadline = Instant::now() + Duration::from_millis(500);
        for _ in 0..2 {
            let left = deadline.saturating_duration_since(Instant::now());
            if self.drained.recv_timeout(left).is_err() {
                break;
            }
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// Copy a child's output to the log, line by line, redacted.
fn pass_through(
    stream: impl Read + Send + 'static,
    label: &'static str,
    log: Log,
    done: Sender<()>,
) {
    thread::spawn(move || {
        let mut reader = BufReader::new(stream);
        let mut line = Vec::new();
        loop {
            line.clear();
            let mut limited = (&mut reader).take(MAX_OUTPUT_LINE as u64);
            match limited.read_until(b'\n', &mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let text = String::from_utf8_lossy(&line);
                    log.line(&format!("{label}: {}", text.trim_end()));
                }
            }
        }
        let _ = done.send(());
    });
}

/// Take the lock of a receiver index: the lowest free one below
/// `receivers`, or exactly `wanted`. The file is returned and must be kept:
/// the lock lasts as long as it is open.
pub fn claim(dir: &Path, receivers: usize, wanted: Option<usize>) -> Result<(usize, File), String> {
    let candidates: Vec<usize> = match wanted {
        Some(index) => vec![index],
        None => (0..receivers).collect(),
    };
    for index in candidates {
        let path = dir.join(lock_file_name(index));
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        match file.try_lock() {
            Ok(()) => return Ok((index, file)),
            Err(fs::TryLockError::WouldBlock) => {}
            Err(fs::TryLockError::Error(e)) => return Err(format!("{}: {e}", path.display())),
        }
    }
    Err(String::new())
}

/// Make the receiver's FIFO, mode 0660, unless a FIFO is already there.
fn ensure_fifo(path: &Path) -> io::Result<()> {
    match fs::metadata(path) {
        Ok(meta) if meta.file_type().is_fifo() => {}
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "exists and is not a FIFO",
            ))
        }
        Err(_) => sys::make_fifo(path, 0o660)?,
    }
    // mkfifo's mode passes through the umask; the documented mode is exact.
    fs::set_permissions(path, fs::Permissions::from_mode(0o660))
}

fn wall_clock() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Run `soloist --version` and read it. A missing executable is
/// `present: false`; anything unreadable is a present build of unknown age.
pub fn read_build(bin: &Path) -> BuildReport {
    let absent = BuildReport {
        present: false,
        version: String::new(),
        build_epoch: None,
        expires_epoch: None,
    };
    if !bin.is_file() {
        return absent;
    }
    let unknown = BuildReport {
        present: true,
        ..absent.clone()
    };
    let spawned = Command::new(bin)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return absent,
        Err(_) => return unknown,
    };
    let deadline = Instant::now() + VERSION_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => thread::sleep(CHILD_POLL),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return unknown;
            }
        }
    }
    // The process has exited, so both pipes end; a version is far smaller
    // than a pipe's capacity, so it never blocked writing either.
    let mut text = String::new();
    for pipe in [
        child.stdout.take().map(|p| Box::new(p) as Box<dyn Read>),
        child.stderr.take().map(|p| Box::new(p) as Box<dyn Read>),
    ]
    .into_iter()
    .flatten()
    {
        let mut bytes = Vec::new();
        let _ = pipe.take(64 * 1024).read_to_end(&mut bytes);
        text.push_str(&String::from_utf8_lossy(&bytes));
        text.push('\n');
    }
    let info = build::parse_version(&text);
    BuildReport {
        present: true,
        expires_epoch: info.expires_epoch(),
        build_epoch: info.build_epoch,
        version: info.version,
    }
}

/// The delay before retry number `failures` (1 for the first): `min`
/// doubled each time, capped at `max`.
pub fn backoff(min: Duration, max: Duration, failures: u32) -> Duration {
    let doublings = failures.saturating_sub(1).min(31);
    min.saturating_mul(1u32 << doublings).min(max)
}

fn human(delay: Duration) -> String {
    if delay >= Duration::from_secs(1) {
        format!("{} s", delay.as_secs())
    } else {
        format!("{} ms", delay.as_millis())
    }
}

struct Assignment {
    target: String,
    name: String,
}

struct Run {
    id: u64,
    process: Proc,
    data_dir: PathBuf,
    /// Tells this run's WebSocket client to end quietly.
    stop: Arc<AtomicBool>,
    /// The connected client's command channel.
    commands: Option<Sender<String>>,
    /// When SIGTERM was sent, if this run is being stopped.
    stopping: Option<Instant>,
    killed: bool,
    /// Why the run is being stopped, when it is a fault.
    fault: Option<String>,
}

struct Connection {
    id: u64,
    stream: UnixStream,
}

struct Helpers {
    generation: u64,
    pipewire: Proc,
    wireplumber: Proc,
}

struct Supervisor {
    config: Config,
    log: Log,
    index: usize,
    tx: Sender<Msg>,
    layout: Option<Layout>,
    helpers: Option<Helpers>,
    helper_generation: u64,
    helper_failures: u32,
    helper_retry_at: Option<Instant>,
    connection: Option<Connection>,
    next_connection: u64,
    build: BuildReport,
    next_expiry_check: Instant,
    assignment: Option<Assignment>,
    generation: u64,
    state: State,
    detail: String,
    /// The last status sent on this connection, so an unchanged one is not
    /// sent again.
    sent_status: Option<StatusReport>,
    run: Option<Run>,
    next_run: u64,
    failures: u32,
    retry_at: Option<Instant>,
}

/// Run the supervisor until SIGTERM or SIGINT; returns the exit code.
pub fn run(config: Config) -> i32 {
    let log = Log::new();
    if let Err(e) = sys::catch_termination() {
        log.line(&format!("the termination signals cannot be caught: {e}"));
    }
    let dir = config.soloist_dir.clone();
    if let Err(e) = fs::create_dir_all(&dir) {
        log.line(&format!("the receiver directory {}: {e}", dir.display()));
        return EXIT_DIRECTORY;
    }
    let (index, _lock) = match claim(&dir, config.receivers, config.receiver) {
        Ok(claimed) => claimed,
        Err(why) if why.is_empty() => {
            log.line(&match config.receiver {
                Some(index) => format!("receiver r{index} is held by another supervisor"),
                None => format!(
                    "all {} receiver indexes are held by other supervisors",
                    config.receivers
                ),
            });
            return EXIT_NO_INDEX;
        }
        Err(why) => {
            log.line(&format!("the receiver lock: {why}"));
            return EXIT_DIRECTORY;
        }
    };
    let fifo = dir.join(pcm_file_name(index));
    if let Err(e) = ensure_fifo(&fifo) {
        log.line(&format!("the FIFO {}: {e}", fifo.display()));
        return EXIT_DIRECTORY;
    }
    let socket_path = dir.join(socket_file_name(index));
    // The lock is held, so a socket file here is a dead supervisor's.
    let _ = fs::remove_file(&socket_path);
    let listener = match UnixListener::bind(&socket_path).and_then(|listener| {
        fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o660))?;
        Ok(listener)
    }) {
        Ok(listener) => listener,
        Err(e) => {
            log.line(&format!("the socket {}: {e}", socket_path.display()));
            return EXIT_DIRECTORY;
        }
    };
    log.line(&format!(
        "receiver r{index} claimed in {} (version {})",
        dir.display(),
        env!("CARGO_PKG_VERSION")
    ));

    let (tx, rx) = mpsc::channel();
    let mut supervisor = Supervisor {
        layout: (config.pipewire == PipewireMode::Auto)
            .then(|| Layout::under(&config.pipewire_runtime_dir)),
        next_expiry_check: Instant::now() + config.expiry_check,
        config,
        log,
        index,
        tx: tx.clone(),
        helpers: None,
        helper_generation: 0,
        helper_failures: 0,
        helper_retry_at: None,
        connection: None,
        next_connection: 0,
        build: BuildReport {
            present: false,
            version: String::new(),
            build_epoch: None,
            expires_epoch: None,
        },
        assignment: None,
        generation: 0,
        state: State::Idle,
        detail: String::new(),
        sent_status: None,
        run: None,
        next_run: 0,
        failures: 0,
        retry_at: None,
    };
    if supervisor.layout.is_some() {
        if let Err(why) = supervisor.start_helpers(&fifo) {
            supervisor
                .log
                .line(&format!("PipeWire could not be started: {why}"));
            let _ = fs::remove_file(&socket_path);
            return EXIT_PIPEWIRE;
        }
    }
    // For `--health-check`: which receiver this container's supervisor holds.
    // Without PipeWire the runtime directory may not be this run's to write
    // (tests), and nothing probes it.
    if let Err(e) =
        crate::health::write_index(&supervisor.config.pipewire_runtime_dir, supervisor.index)
    {
        if supervisor.layout.is_some() {
            supervisor
                .log
                .line(&format!("the index file for --health-check: {e}"));
        }
    }
    supervisor.refresh_build();

    let acceptor = tx.clone();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            if acceptor.send(Msg::Connected(stream)).is_err() {
                break;
            }
        }
    });

    supervisor.serve(&rx, &fifo);
    supervisor.shut_down();
    let _ = fs::remove_file(&socket_path);
    0
}

impl Supervisor {
    fn serve(&mut self, rx: &Receiver<Msg>, fifo: &Path) {
        while !sys::termination_requested() {
            match rx.recv_timeout(TICK) {
                Ok(msg) => self.handle(msg),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            self.tick(fifo);
        }
        self.log.line("told to stop");
    }

    fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::Connected(stream) => self.on_connected(stream),
            Msg::Line(id, line) => {
                if self.connection.as_ref().is_some_and(|c| c.id == id) {
                    self.on_line(&line);
                }
            }
            Msg::Disconnected(id, why) => {
                if self.connection.as_ref().is_some_and(|c| c.id == id) {
                    self.log.line(&match why {
                        Some(why) => format!("the server's connection was dropped: {why}"),
                        None => "the server disconnected".to_string(),
                    });
                    self.drop_connection();
                }
            }
            Msg::SoloistExited(id, code) => self.on_soloist_exited(id, code),
            Msg::Ws(id, report) => self.on_ws(id, report),
            Msg::HelperExited(generation, name, code) => {
                self.on_helper_exited(generation, name, code);
            }
        }
    }

    // ---- the server's connection -------------------------------------

    fn on_connected(&mut self, stream: UnixStream) {
        if self.connection.is_some() {
            self.log
                .line("a new server connection replaces the old one");
            self.drop_connection();
        }
        let id = self.next_connection;
        self.next_connection += 1;
        let reader = match stream.try_clone() {
            Ok(reader) => reader,
            Err(e) => {
                return self
                    .log
                    .line(&format!("a connection could not be read: {e}"))
            }
        };
        let _ = stream.set_write_timeout(Some(SERVER_WRITE_TIMEOUT));
        let tx = self.tx.clone();
        thread::spawn(move || read_connection(id, reader, &tx));
        self.connection = Some(Connection { id, stream });
        self.sent_status = None;
        self.log.line("the server connected");
        self.send(&FromSupervisor::Hello {
            v: protocol::VERSION,
            receiver: self.index,
            supervisor: env!("CARGO_PKG_VERSION").to_string(),
        });
        self.send(&FromSupervisor::Build(self.build.clone()));
        self.send_status();
        // A server that connects to a running Soloist has missed its
        // auth_state; ask for it again on its behalf.
        self.tell_soloist(&api::Command::GetAuthState.to_json());
    }

    fn drop_connection(&mut self) {
        if let Some(connection) = self.connection.take() {
            let _ = connection.stream.shutdown(std::net::Shutdown::Both);
        }
        self.sent_status = None;
    }

    fn send(&mut self, message: &FromSupervisor) {
        let Some(connection) = &mut self.connection else {
            return;
        };
        let line = match message.encode() {
            Ok(line) => self.log.clean(&line),
            Err(e) => return self.log.line(&format!("a message was not sent: {e}")),
        };
        if let Err(e) = connection.stream.write_all(line.as_bytes()) {
            self.log.line(&format!(
                "the server is not reading ({e}); connection dropped"
            ));
            self.drop_connection();
        }
    }

    fn status(&self, detail: &str) -> StatusReport {
        let (target, name) = self
            .assignment
            .as_ref()
            .map(|a| (a.target.clone(), a.name.clone()))
            .unwrap_or_default();
        StatusReport {
            state: self.state,
            target,
            name,
            detail: detail.to_string(),
            generation: self.generation,
        }
    }

    /// Send the status if it differs from the last one sent.
    fn send_status(&mut self) {
        let status = self.status(&self.detail);
        if self.sent_status.as_ref() != Some(&status) {
            self.send(&FromSupervisor::Status(status.clone()));
            if self.connection.is_some() {
                self.sent_status = Some(status);
            }
        }
    }

    /// Send the present status once with a passing remark as its detail.
    fn remark(&mut self, detail: &str) {
        self.log.line(detail);
        let status = self.status(detail);
        self.send(&FromSupervisor::Status(status));
        self.sent_status = None;
    }

    fn set_state(&mut self, state: State, detail: &str) {
        if self.state != state || self.detail != detail {
            self.log.line(&format!(
                "{}{}{}",
                state.as_str(),
                if detail.is_empty() { "" } else { ": " },
                detail
            ));
        }
        self.state = state;
        self.detail = detail.to_string();
        self.send_status();
    }

    fn on_line(&mut self, line: &str) {
        match ToSupervisor::decode(line) {
            Ok(ToSupervisor::Assign {
                generation,
                target,
                name,
            }) => self.on_assign(generation, target, name),
            Ok(ToSupervisor::Release { generation }) => self.on_release(generation),
            Ok(ToSupervisor::Command {
                generation,
                command,
            }) => {
                if generation != self.generation {
                    self.remark(&format!(
                        "command dropped: it names generation {generation}, this is {}",
                        self.generation
                    ));
                } else if !self.tell_soloist(&json::write(&command)) {
                    self.remark("command dropped: Soloist is not running");
                }
            }
            Ok(ToSupervisor::Restart) => self.on_restart(),
            Err(ProtocolError::UnknownKind(kind)) => {
                self.log
                    .line(&format!("a message of unknown kind {kind:?} was skipped"));
            }
            Err(e) => self
                .log
                .line(&format!("a line from the server was skipped: {e}")),
        }
    }

    /// Hand a JSON text to Soloist; false when it is not connected.
    fn tell_soloist(&self, text: &str) -> bool {
        let Some(run) = &self.run else { return false };
        if self.state != State::Running || run.stopping.is_some() {
            return false;
        }
        run.commands
            .as_ref()
            .is_some_and(|commands| commands.send(text.to_string()).is_ok())
    }

    // ---- assignments --------------------------------------------------

    fn on_assign(&mut self, generation: u64, target: String, name: String) {
        if let Err(e) = keydir::dir_name(&target) {
            return self.remark(&format!("assign refused: target {target:?}: {e}"));
        }
        if name.is_empty() || name.chars().any(char::is_control) {
            return self
                .remark("assign refused: the device name is empty or has a control character");
        }
        let same = self
            .assignment
            .as_ref()
            .is_some_and(|a| a.target == target && a.name == name);
        self.generation = generation;
        if same {
            // The same target under the same name: Soloist carries on, and
            // so does an expiry or a retry schedule (only `restart` clears
            // those; a server that reconnects and assigns again must not
            // turn "expired" into a restart loop).
            return self.send_status();
        }
        self.log.line(&format!(
            "assigned {target} as {name:?} (generation {generation})"
        ));
        self.assignment = Some(Assignment { target, name });
        self.failures = 0;
        self.retry_at = None;
        self.restart_soloist();
    }

    fn on_release(&mut self, generation: u64) {
        self.generation = generation;
        self.assignment = None;
        self.failures = 0;
        self.retry_at = None;
        if self.run.is_some() {
            self.stop_run(None);
            self.send_status();
        } else {
            self.set_state(State::Idle, "");
        }
    }

    fn on_restart(&mut self) {
        self.log.line("restart: reading the build again");
        self.refresh_build();
        self.failures = 0;
        self.retry_at = None;
        if self.assignment.is_some() {
            self.restart_soloist();
        } else {
            self.set_state(State::Idle, "");
        }
    }

    /// Stop the running Soloist if there is one, then start one for the
    /// present assignment.
    fn restart_soloist(&mut self) {
        if self.run.is_some() {
            self.stop_run(None);
            self.set_state(State::Starting, "stopping the previous Soloist");
        } else {
            self.start_soloist();
        }
    }

    fn stop_run(&mut self, fault: Option<String>) {
        if let Some(run) = &mut self.run {
            if run.stopping.is_none() {
                run.stop.store(true, Ordering::SeqCst);
                run.commands = None;
                run.process.terminate();
                run.stopping = Some(Instant::now());
            }
            if run.fault.is_none() {
                run.fault = fault;
            }
        }
    }

    fn fail(&mut self, state: State, why: &str) {
        self.failures += 1;
        let delay = backoff(
            self.config.backoff_min,
            self.config.backoff_max,
            self.failures,
        );
        self.retry_at = Some(Instant::now() + delay);
        self.set_state(
            state,
            &format!(
                "{why}; retry in {} (attempt {})",
                human(delay),
                self.failures
            ),
        );
    }

    fn start_soloist(&mut self) {
        let Some(assignment) = &self.assignment else {
            return self.set_state(State::Idle, "");
        };
        let (target, name) = (assignment.target.clone(), assignment.name.clone());
        self.retry_at = None;
        if self.layout.is_some() && self.helpers.is_none() {
            return self.fail(State::Failed, "PipeWire is not running");
        }
        let bin = self.config.soloist_bin.clone();
        if !bin.is_file() || !self.build.present {
            // The binary went, or has just appeared: say which build it is
            // before anything else.
            self.refresh_build();
        }
        if !bin.is_file() {
            return self.fail(
                State::NoBinary,
                &format!("no Soloist executable at {}", bin.display()),
            );
        }
        // The key is read at each start, so a replaced key file needs no
        // restart of the supervisor.
        let key = match fs::read_to_string(&self.config.api_key_file) {
            Ok(text) if !text.trim().is_empty() => text.trim().to_string(),
            Ok(_) => return self.fail(State::Failed, "the API key file is empty"),
            Err(e) => {
                return self.fail(
                    State::Failed,
                    &format!("the API key file cannot be read ({})", e.kind()),
                )
            }
        };
        self.log.set_secret(&key);
        // Checked when assigned; a key that has no directory name never
        // gets here.
        let Ok(dir_name) = keydir::dir_name(&target) else {
            return self.fail(State::Failed, "the target has no directory name");
        };
        let data_dir = self.config.state_dir.join(&dir_name);
        let cache_dir = self.config.cache_dir.join(&dir_name);
        for dir in [&data_dir, &cache_dir] {
            if let Err(e) = fs::create_dir_all(dir) {
                return self.fail(State::Failed, &format!("{}: {e}", dir.display()));
            }
        }
        // A port file left by a Soloist that was killed would be read as
        // this one's.
        for stale in [WS_PORT_FILE, WS_ADDR_FILE] {
            let _ = fs::remove_file(data_dir.join(stale));
        }
        let arguments: Vec<String> = vec![
            "--device-name".into(),
            name,
            "--data-dir".into(),
            data_dir.display().to_string(),
            "--cache-dir".into(),
            cache_dir.display().to_string(),
            "--cache-size".into(),
            self.config.cache_size.to_string(),
            "--pipewire-device".into(),
            node_name(self.index),
            "--initial-volume".into(),
            "100".into(),
            "--ws".into(),
            "127.0.0.1:0".into(),
            "--api-key".into(),
            key,
        ];
        // Logged through the redacting log: the key's value never appears.
        self.log.line(&format!(
            "starting {} {}",
            bin.display(),
            arguments.join(" ")
        ));
        let mut command = Command::new(&bin);
        command.args(&arguments);
        if let Some(layout) = &self.layout {
            command.envs(layout.environment(&self.config.wireplumber_config_dir));
        }
        let id = self.next_run;
        self.next_run += 1;
        let tx = self.tx.clone();
        let process = match Proc::spawn(command, "soloist", &self.log, move |code| {
            let _ = tx.send(Msg::SoloistExited(id, code));
        }) {
            Ok(process) => process,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return self.fail(State::NoBinary, "the Soloist executable vanished")
            }
            Err(e) => {
                return self.fail(State::Failed, &format!("Soloist could not be started: {e}"))
            }
        };
        let mut run = Run {
            id,
            process,
            data_dir,
            stop: Arc::new(AtomicBool::new(false)),
            commands: None,
            stopping: None,
            killed: false,
            fault: None,
        };
        self.connect_ws(&mut run);
        self.run = Some(run);
        self.set_state(State::Starting, "");
    }

    fn connect_ws(&self, run: &mut Run) {
        let tx = self.tx.clone();
        let id = run.id;
        let client = wsclient::spawn(
            run.data_dir.clone(),
            self.config.ws_timeout,
            Arc::clone(&run.stop),
            move |report| {
                let _ = tx.send(Msg::Ws(id, report));
            },
        );
        run.commands = Some(client.commands);
    }

    fn on_ws(&mut self, id: u64, report: WsReport) {
        if !self
            .run
            .as_ref()
            .is_some_and(|run| run.id == id && run.stopping.is_none())
        {
            return;
        }
        match report {
            WsReport::Up => {
                self.failures = 0;
                self.set_state(State::Running, "");
            }
            WsReport::Event(text) => {
                // Redacted by value before it is parsed, relayed or logged.
                let text = self.log.clean(&text);
                match json::parse(&text) {
                    Ok(event @ json::Value::Obj(_)) => self.send(&FromSupervisor::Event {
                        generation: self.generation,
                        event,
                    }),
                    Ok(_) => self
                        .log
                        .line("a frame from Soloist that is not a JSON object was dropped"),
                    Err(e) => self.log.line(&format!(
                        "a frame from Soloist that is not JSON was dropped: {e}"
                    )),
                }
            }
            WsReport::Fault(why) => self.stop_run(Some(why)),
            WsReport::Lost(why) => {
                // Soloist is still running: connect again, under the same
                // bound, and only then call it a fault.
                let Some(mut run) = self.run.take() else {
                    return;
                };
                self.connect_ws(&mut run);
                self.run = Some(run);
                self.set_state(
                    State::Starting,
                    &format!("the WebSocket was lost ({why}); reconnecting"),
                );
            }
        }
    }

    fn on_soloist_exited(&mut self, id: u64, code: Option<i32>) {
        if !self.run.as_ref().is_some_and(|run| run.id == id) {
            return;
        }
        let Some(run) = self.run.take() else { return };
        run.stop.store(true, Ordering::SeqCst);
        let how = match code {
            Some(code) => format!("Soloist exited {code}"),
            None => "Soloist was killed by a signal".to_string(),
        };
        self.log.line(&how);
        if let Some(fault) = run.fault {
            return self.fail(State::Failed, &fault);
        }
        if run.stopping.is_some() {
            // Stopped on purpose: for a release, or to start the next one.
            return if self.assignment.is_some() {
                self.start_soloist();
            } else {
                self.set_state(State::Idle, "");
            };
        }
        match code {
            // "Spotify Soloist build expired. Install a newer build before
            // starting Spotify Soloist again." No retry until `restart`.
            Some(10) => {
                self.retry_at = None;
                self.set_state(
                    State::Expired,
                    "Soloist exited 10: the build has expired; replace the binary, then restart",
                );
            }
            _ => self.fail(State::Failed, &how),
        }
    }

    // ---- the build and its expiry -------------------------------------

    fn refresh_build(&mut self) {
        let build = read_build(&self.config.soloist_bin);
        if build != self.build {
            self.log.line(&match (build.present, build.expires_epoch) {
                (false, _) => format!(
                    "no Soloist executable at {}",
                    self.config.soloist_bin.display()
                ),
                (true, Some(expires)) => format!(
                    "Soloist build: {:?}, expires at Unix time {expires}",
                    build.version
                ),
                (true, None) => format!(
                    "Soloist build: {:?}, build time not recognised, expiry unknown",
                    build.version
                ),
            });
            self.build = build;
            self.send(&FromSupervisor::Build(self.build.clone()));
        }
        self.warn_of_expiry();
    }

    /// The supervisor's own warning: one line at every look at the build
    /// (at start, on `restart`, and at every expiry check) while it stands.
    fn warn_of_expiry(&self) {
        if let Some(text) = Expiry::at(self.build.expires_epoch, wall_clock()).warning() {
            self.log.line(&format!("warning: {text}"));
        }
    }

    // ---- PipeWire -----------------------------------------------------

    fn start_helpers(&mut self, fifo: &Path) -> Result<(), String> {
        let Some(layout) = self.layout.clone() else {
            return Ok(());
        };
        layout
            .write(fifo, &node_name(self.index))
            .map_err(|e| format!("the configuration under {}: {e}", layout.runtime.display()))?;
        let _ = fs::remove_file(layout.socket());
        let environment = layout.environment(&self.config.wireplumber_config_dir);
        self.helper_generation += 1;
        let generation = self.helper_generation;
        let spawn = |bin: &Path, name: &'static str, arguments: &[&str], this: &Supervisor| {
            let mut command = Command::new(bin);
            command.args(arguments).envs(environment.clone());
            let tx = this.tx.clone();
            Proc::spawn(command, name, &this.log, move |code| {
                let _ = tx.send(Msg::HelperExited(generation, name, code));
            })
            .map_err(|e| format!("{}: {e}", bin.display()))
        };
        let pipewire = spawn(&self.config.pipewire_bin, "pipewire", &[], self)?;
        let deadline = Instant::now() + PIPEWIRE_START_TIMEOUT;
        while !layout.socket().exists() {
            if pipewire.exited() {
                return Err("pipewire exited before creating its socket".to_string());
            }
            if Instant::now() >= deadline {
                pipewire.stop(Duration::from_secs(1));
                return Err(format!(
                    "pipewire created no socket within {} s",
                    PIPEWIRE_START_TIMEOUT.as_secs()
                ));
            }
            thread::sleep(CHILD_POLL);
        }
        let wireplumber = match spawn(
            &self.config.wireplumber_bin,
            "wireplumber",
            &["-p", pipewire::PROFILE],
            self,
        ) {
            Ok(wireplumber) => wireplumber,
            Err(why) => {
                pipewire.stop(Duration::from_secs(1));
                return Err(why);
            }
        };
        self.log.line(&format!(
            "PipeWire and WirePlumber started; sink {} writes {}",
            node_name(self.index),
            fifo.display()
        ));
        self.helpers = Some(Helpers {
            generation,
            pipewire,
            wireplumber,
        });
        Ok(())
    }

    fn on_helper_exited(&mut self, generation: u64, name: &str, code: Option<i32>) {
        let Some(helpers) = &self.helpers else { return };
        if helpers.generation != generation {
            return;
        }
        self.log.line(&format!(
            "{name} exited ({code:?}); PipeWire will be started again"
        ));
        if let Some(helpers) = self.helpers.take() {
            helpers.wireplumber.stop(Duration::from_secs(1));
            helpers.pipewire.stop(Duration::from_secs(1));
        }
        self.helper_failures += 1;
        self.helper_retry_at = Some(
            Instant::now()
                + backoff(
                    self.config.backoff_min,
                    self.config.backoff_max,
                    self.helper_failures,
                ),
        );
        // Soloist's sink is gone; it is started again once PipeWire is.
        if self.run.is_some() {
            self.stop_run(Some(format!("{name} exited")));
        }
    }

    // ---- timers -------------------------------------------------------

    fn tick(&mut self, fifo: &Path) {
        let now = Instant::now();
        if let Some(run) = &mut self.run {
            if let Some(since) = run.stopping {
                if !run.killed && now.duration_since(since) >= self.config.stop_timeout {
                    self.log.line("Soloist ignored SIGTERM; killing it");
                    run.process.kill();
                    run.killed = true;
                }
            }
        }
        if self.helper_retry_at.is_some_and(|at| now >= at) {
            self.helper_retry_at = None;
            match self.start_helpers(fifo) {
                Ok(()) => self.helper_failures = 0,
                Err(why) => {
                    self.log
                        .line(&format!("PipeWire could not be started: {why}"));
                    self.helper_failures += 1;
                    self.helper_retry_at = Some(
                        now + backoff(
                            self.config.backoff_min,
                            self.config.backoff_max,
                            self.helper_failures,
                        ),
                    );
                }
            }
        }
        if self.run.is_none() && self.retry_at.is_some_and(|at| now >= at) {
            self.retry_at = None;
            if self.assignment.is_some() {
                self.start_soloist();
            }
        }
        if now >= self.next_expiry_check {
            self.next_expiry_check = now + self.config.expiry_check;
            self.refresh_build();
        }
    }

    fn shut_down(&mut self) {
        if let Some(run) = self.run.take() {
            run.stop.store(true, Ordering::SeqCst);
            run.process.stop(self.config.stop_timeout);
        }
        if let Some(helpers) = self.helpers.take() {
            helpers.wireplumber.stop(Duration::from_secs(2));
            helpers.pipewire.stop(Duration::from_secs(2));
        }
        self.drop_connection();
    }
}

/// Read a server connection's lines and hand them to the main thread.
fn read_connection(id: u64, mut stream: UnixStream, tx: &Sender<Msg>) {
    let mut buffer = LineBuffer::new();
    let mut chunk = [0u8; 4096];
    let why = 'connection: loop {
        loop {
            match buffer.next_line() {
                Ok(Some(line)) => {
                    if tx.send(Msg::Line(id, line)).is_err() {
                        return;
                    }
                }
                Ok(None) => break,
                Err(e) => break 'connection Some(e.to_string()),
            }
        }
        match stream.read(&mut chunk) {
            Ok(0) => break None,
            Ok(n) => buffer.feed(&chunk[..n]),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => break None,
        }
    };
    let _ = stream.shutdown(std::net::Shutdown::Both);
    let _ = tx.send(Msg::Disconnected(id, why));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("chorus-soloistd-sup-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_backoff_doubles_to_its_cap() {
        let (min, max) = (Duration::from_secs(1), Duration::from_secs(60));
        let delays: Vec<u64> = (1..=9).map(|n| backoff(min, max, n).as_secs()).collect();
        assert_eq!(delays, vec![1, 2, 4, 8, 16, 32, 60, 60, 60]);
        assert_eq!(backoff(min, max, 0), min);
        assert_eq!(backoff(min, max, u32::MAX), max);
        assert_eq!(human(Duration::from_millis(250)), "250 ms");
        assert_eq!(human(Duration::from_secs(4)), "4 s");
    }

    #[test]
    fn the_lowest_free_index_is_claimed_and_a_held_one_is_not() {
        let dir = scratch("claim");
        let (first, _a) = claim(&dir, 3, None).unwrap();
        let (second, _b) = claim(&dir, 3, None).unwrap();
        assert_eq!((first, second), (0, 1));
        assert_eq!(claim(&dir, 3, Some(1)).unwrap_err(), "");
        let (third, c) = claim(&dir, 3, Some(2)).unwrap();
        assert_eq!(third, 2);
        assert_eq!(claim(&dir, 3, None).unwrap_err(), "");
        // A released index is free again.
        drop(c);
        assert_eq!(claim(&dir, 3, None).unwrap().0, 2);
        assert!(dir.join("r0.lock").is_file());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_fifo_is_made_with_mode_0660_and_a_plain_file_is_refused() {
        let dir = scratch("fifo");
        let path = dir.join("r0.pcm");
        ensure_fifo(&path).unwrap();
        let meta = fs::metadata(&path).unwrap();
        assert!(meta.file_type().is_fifo());
        assert_eq!(meta.permissions().mode() & 0o777, 0o660);
        ensure_fifo(&path).unwrap();
        let plain = dir.join("r1.pcm");
        fs::write(&plain, b"").unwrap();
        assert!(ensure_fifo(&plain).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_missing_binary_is_not_present() {
        let build = read_build(Path::new("/nonexistent/soloist"));
        assert!(!build.present);
        assert_eq!((build.build_epoch, build.expires_epoch), (None, None));
    }
}
