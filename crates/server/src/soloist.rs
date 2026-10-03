//! The Soloist receiver manager: the server side of Spotify Soloist (goal
//! 17, proposal P7 Option C, `docs/soloist.md`).
//!
//! `--soloist-receivers N` gives the server N receivers, `r0` to `r<N-1>`.
//! Each is another container: PipeWire, one Soloist process and the
//! supervisor `chorus-soloistd`, reached only through the receiver directory
//! (`--soloist-dir`): a FIFO of PCM, read by `crate::soloistreader`, and a
//! Unix socket speaking `chorus_soloist::protocol`, which this module's one
//! thread, `soloist-manager`, connects to. chorus never runs, links or ships
//! Soloist; it talks to the supervisor of a binary the owner installed.
//!
//! What the manager does, all on its one thread:
//!
//! - **Connects** to every `r<i>.sock`, and again whenever a connection
//!   ends. A socket that is not there is not an error: the receiver
//!   containers may start after the server, or not at all.
//! - **Runs the pool** (`chorus_soloist::pool`) over the targets of the
//!   control state (`crate::targets`): every room, saved group and live
//!   group gets a receiver while there are enough, under its own name as the
//!   Spotify Connect device name, and is sent `assign` or `release`.
//! - **Takes the room (K78).** When a target's receiver plays and the
//!   target's group does not play it, every other receiver a room of the
//!   target is hearing is sent `pause` then `deactivate`, and then the
//!   target's rooms are made one group playing the receiver, with the
//!   catalog's own `take`. A receiver that no group plays any more (the
//!   group took another source, an alarm rang, the group dissolved) is sent
//!   `pause`, so the Spotify app shows the truth and nothing plays unheard.
//! - **Says what is playing**: the group's now-playing record, `via`
//!   `spotify`.
//! - **Volume**, both ways, under one of two mappings
//!   ([`VolumeMapping`]), with the room limits clamping either.
//! - **Playback commands** from the catalog (`playback`) and
//!   `soloist_restart`, forwarded.
//! - **The Spotify alarm source**: the conductor's request
//!   ([`Link::play_alarm`]) is checked, played and answered.
//! - **The expiry warning**: the state's `soloist` member, the metrics and
//!   a log line a day.
//!
//! Control code: it touches no PCM and no chunk. Its clocks are the
//! monotonic one for every wait, and the wall clock for one thing only, the
//! build's remaining days (`chorus_soloist::build`); `audio-path.conf`
//! records it as excluded. A Soloist event's `position.timestamp_ms` is
//! Soloist's wall clock and is never read.

use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chorus_control::catalog::Command;
use chorus_control::json::{self, Value};
use chorus_control::rooms::{
    NowPlaying, PlayState, PlaybackAction, SoloistBuild, SoloistReceiver, SoloistState, Source,
};
use chorus_hostctl::ThreadRegistry;
use chorus_soloist::api::{self, Command as Api, Entity, Event, Status};
use chorus_soloist::build::{days_left, Expiry};
use chorus_soloist::pool::{Pool, Target};
use chorus_soloist::protocol::{
    BuildReport, FromSupervisor, LineBuffer, State, StatusReport, ToSupervisor,
};
use chorus_soloist::{receiver_id, receiver_index, socket_file_name};

use crate::control::ControlState;
use crate::hostreport::register_ordinary_thread;
use crate::soloistport::SoloistPort;
use crate::soloistreader::ReaderStats;
use crate::targets::{specs_of, Kind, Spec};

/// The most receivers a server runs. ASSUMED: 32, twice P7's default pool
/// of 16; proposal P11 measures what a host carries.
pub const MAX_RECEIVERS: usize = 32;

/// How long a dissolved, idle live group keeps its receiver, by default.
/// ASSUMED (P7): 60 s.
pub const DEFAULT_GRACE: Duration = Duration::from_secs(60);

/// How long an alarm waits for its receiver to report `playing` before it
/// rings the fallback chime (design 2.5): 10 s.
pub const ALARM_WAIT: Duration = Duration::from_secs(10);

/// The manager's pace: how long it sleeps when nothing arrived.
const POLL: Duration = Duration::from_millis(10);

/// How often a socket that is not there, or refused, is tried again.
const RECONNECT: Duration = Duration::from_millis(250);

/// How long after a `pause` the manager sent a receiver that still reports
/// `playing` is left alone, so a report already on its way does not take the
/// room back. ASSUMED: 3 s.
const PAUSE_SETTLE: Duration = Duration::from_secs(3);

/// How long a refused `take` waits before it is tried again.
const TAKE_RETRY: Duration = Duration::from_secs(2);

/// How long the conductor has to say whether an alarm took its receiver.
const ALARM_VERDICT: Duration = Duration::from_secs(5);

/// How often the expiry warning is logged: once a day.
const WARNING_EVERY: Duration = Duration::from_secs(86_400);

/// Which gain stage a Spotify volume drives (K77; P7 leaves it to the
/// owner's build, so both are built behind one switch).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VolumeMapping {
    /// The default. Soloist's audio is taken to arrive untouched (it is
    /// started at volume 100): a volume the Spotify app sets becomes the
    /// target's volume in chorus, through the catalog's own commands so the
    /// room limits clamp it, and a chorus volume change is sent back with
    /// `set_volume` so the app's slider follows.
    #[default]
    Chorus,
    /// Soloist's volume is taken to act on its audio: it IS the target's
    /// group gain, chorus leaves its own volumes alone, and only clamps: a
    /// volume above the target's limit is set back to the limit.
    Receiver,
}

impl VolumeMapping {
    /// The flag's word.
    pub fn name(self) -> &'static str {
        match self {
            VolumeMapping::Chorus => "chorus",
            VolumeMapping::Receiver => "receiver",
        }
    }

    /// Read the flag's word back.
    pub fn parse(text: &str) -> Option<VolumeMapping> {
        match text {
            "chorus" => Some(VolumeMapping::Chorus),
            "receiver" => Some(VolumeMapping::Receiver),
            _ => None,
        }
    }
}

/// What the manager is started with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// The receiver directory (`--soloist-dir`).
    pub dir: PathBuf,
    /// How many receivers (`--soloist-receivers`).
    pub receivers: usize,
    /// The live groups' grace period (`--soloist-grace`).
    pub grace: Duration,
    /// The volume mapping (`--soloist-volume`).
    pub volume: VolumeMapping,
    /// How long an alarm waits for `playing`.
    pub alarm_wait: Duration,
}

/// The answer to an alarm's request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlarmAnswer {
    /// The receiver plays: the alarm's group should play `source`.
    Started {
        /// The alarm.
        alarm: String,
        /// The receiver's index.
        receiver: usize,
        /// `soloist:r<i>`.
        source: Source,
    },
    /// It did not start: ring the chime.
    Failed {
        /// The alarm.
        alarm: String,
        /// `soloist-unavailable`, `soloist-logged-out`, `soloist-expired`
        /// or `soloist-timeout`.
        reason: &'static str,
        /// The reason in words.
        detail: String,
    },
}

#[derive(Debug)]
enum Request {
    Alarm {
        alarm: String,
        target: String,
        uri: String,
    },
    AlarmTaken {
        receiver: usize,
        took: bool,
    },
    Restart,
    Playback {
        receiver: usize,
        action: PlaybackAction,
    },
}

/// What the metrics endpoint shows of the manager.
#[derive(Debug, Clone, Default)]
struct View {
    connected: Vec<bool>,
    expires_epoch: Option<u64>,
    expired: bool,
}

/// What the rest of the server shares with the manager: the conductor's
/// alarm requests and their answers, the control plane's commands, and the
/// numbers the metrics endpoint prints. Plain data behind mutexes; nothing
/// here is called from the audio thread.
pub struct Link {
    requests: Mutex<Vec<Request>>,
    answers: Mutex<Vec<AlarmAnswer>>,
    view: Mutex<View>,
    ports: Vec<Arc<SoloistPort>>,
    readers: Vec<Arc<ReaderStats>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn wall_clock() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl Link {
    /// A link over the receivers' ports and their readers' counters.
    pub fn new(ports: Vec<Arc<SoloistPort>>, readers: Vec<Arc<ReaderStats>>) -> Link {
        let receivers = ports.len();
        Link {
            requests: Mutex::new(Vec::new()),
            answers: Mutex::new(Vec::new()),
            view: Mutex::new(View {
                connected: vec![false; receivers],
                ..View::default()
            }),
            ports,
            readers,
        }
    }

    /// How many receivers the server runs.
    pub fn receivers(&self) -> usize {
        self.ports.len()
    }

    /// The receivers' ports, for whoever routes the slots.
    pub fn ports(&self) -> &[Arc<SoloistPort>] {
        &self.ports
    }

    /// The state member a server shows before any supervisor has
    /// connected: every receiver `absent`.
    pub fn initial_state(&self) -> SoloistState {
        SoloistState {
            receivers: (0..self.receivers())
                .map(|i| SoloistReceiver {
                    id: receiver_id(i),
                    state: "absent".to_string(),
                    target: String::new(),
                    name: String::new(),
                })
                .collect(),
            ..SoloistState::default()
        }
    }

    /// (the conductor) An alarm whose source is a stored Spotify URI fired:
    /// play `uri` on `target`'s receiver. The answer comes through
    /// [`Link::take_answers`], and the conductor is woken for it.
    pub fn play_alarm(&self, alarm: &str, target: &str, uri: &str) {
        lock(&self.requests).push(Request::Alarm {
            alarm: alarm.to_string(),
            target: target.to_string(),
            uri: uri.to_string(),
        });
    }

    /// (the conductor) Whether the alarm's group took the receiver that
    /// [`AlarmAnswer::Started`] named. When it did not (a person chose
    /// something else meanwhile) the receiver is paused.
    pub fn alarm_taken(&self, receiver: usize, took: bool) {
        lock(&self.requests).push(Request::AlarmTaken { receiver, took });
    }

    /// (the conductor) The answers so far.
    pub fn take_answers(&self) -> Vec<AlarmAnswer> {
        std::mem::take(&mut *lock(&self.answers))
    }

    /// (the control plane) `soloist_restart` was applied.
    pub fn restart(&self) {
        lock(&self.requests).push(Request::Restart);
    }

    /// (the control plane) A `playback` command for the group playing
    /// receiver `receiver` was applied.
    pub fn playback(&self, receiver: usize, action: PlaybackAction) {
        lock(&self.requests).push(Request::Playback { receiver, action });
    }

    /// The Prometheus text of the receivers (`GET /metrics`): the build's
    /// seconds to expiry, each receiver's connection, and each reader's and
    /// port's counters.
    pub fn metrics(&self) -> String {
        let view = lock(&self.view).clone();
        let mut out = String::new();
        let family = |name: &str, kind: &str, help: &str, out: &mut String| {
            out.push_str(&format!("# HELP {} {}\n# TYPE {} {}\n", name, help, name, kind));
        };
        family(
            "chorus_soloist_build_expires_seconds",
            "gauge",
            "Seconds until the Soloist build expires, negative once it has; no sample while \
             the build time is unknown.",
            &mut out,
        );
        if let Some(expires) = view.expires_epoch {
            let left = i128::from(expires) - i128::from(wall_clock());
            out.push_str(&format!("chorus_soloist_build_expires_seconds {}\n", left));
        }
        family(
            "chorus_soloist_build_expired",
            "gauge",
            "1 when a receiver's Soloist exited as expired (exit code 10) or its build time \
             has passed, else 0.",
            &mut out,
        );
        out.push_str(&format!(
            "chorus_soloist_build_expired {}\n",
            u8::from(view.expired)
        ));
        family(
            "chorus_soloist_receiver_connected",
            "gauge",
            "1 while the server holds a connection to the receiver's supervisor.",
            &mut out,
        );
        for (i, up) in view.connected.iter().enumerate() {
            out.push_str(&format!(
                "chorus_soloist_receiver_connected{{receiver=\"{}\"}} {}\n",
                receiver_id(i),
                u8::from(*up)
            ));
        }
        type Read = fn(&Link, usize) -> u64;
        let counters: [(&str, &str, Read); 7] = [
            (
                "chorus_soloist_frames_read_total",
                "Frames read from the receiver's FIFO, played or not.",
                |l, i| l.readers[i].frames_read.load(Ordering::Relaxed),
            ),
            (
                "chorus_soloist_frames_written_total",
                "Converted frames written into the receiver's port.",
                |l, i| l.ports[i].counters().written,
            ),
            (
                "chorus_soloist_frames_played_total",
                "Frames the audio thread took from the receiver's port.",
                |l, i| l.ports[i].counters().played,
            ),
            (
                "chorus_soloist_frames_dropped_total",
                "Frames dropped because the receiver's port was full.",
                |l, i| l.ports[i].counters().dropped,
            ),
            (
                "chorus_soloist_frames_discarded_total",
                "Frames discarded because no group played the receiver.",
                |l, i| l.ports[i].counters().discarded,
            ),
            (
                "chorus_soloist_underruns_total",
                "Times the receiver's port ran dry in the middle of a stream.",
                |l, i| l.ports[i].counters().underruns,
            ),
            (
                "chorus_soloist_frames_padded_total",
                "Frames of silence played in place of audio the receiver's port did not hold.",
                |l, i| l.ports[i].counters().padded,
            ),
        ];
        for (name, help, read) in counters {
            family(name, "counter", help, &mut out);
            for i in 0..self.receivers() {
                out.push_str(&format!(
                    "{}{{receiver=\"{}\"}} {}\n",
                    name,
                    receiver_id(i),
                    read(self, i)
                ));
            }
        }
        out
    }
}

// ----- the control state, read ------------------------------------------------

/// What the manager needs of a state beyond its targets: which group each
/// room is in and what each formed group plays.
#[derive(Debug, Default)]
struct Facts {
    specs: Vec<Spec>,
    room_group: BTreeMap<String, String>,
    group_source: BTreeMap<String, String>,
}

impl Facts {
    fn of(state: &str) -> Option<(i64, Facts)> {
        let (serial, specs) = specs_of(state)?;
        let value = json::parse(state).ok()?;
        let text = |v: &Value, key: &str| v.get(key).and_then(Value::as_str).map(str::to_string);
        let items = |key: &str| match value.get(key) {
            Some(Value::Arr(items)) => items.as_slice(),
            _ => &[],
        };
        let mut facts = Facts {
            specs,
            ..Facts::default()
        };
        for zone in items("zones") {
            let id = text(zone, "id")?;
            let group = text(zone, "group").unwrap_or_else(|| id.clone());
            facts.room_group.insert(id, group);
        }
        for group in items("groups") {
            let id = text(group, "id")?;
            facts
                .group_source
                .insert(id, text(group, "source").unwrap_or_default());
        }
        Some((serial, facts))
    }

    fn spec(&self, key: &str) -> Option<&Spec> {
        self.specs.iter().find(|s| s.key == key)
    }

    /// The formed group that plays receiver `index`, if one does.
    fn played_by(&self, index: usize) -> Option<&str> {
        let source = format!("soloist:{}", receiver_id(index));
        self.group_source
            .iter()
            .find(|(_, s)| **s == source)
            .map(|(g, _)| g.as_str())
    }

    /// The receivers, other than `index`, the rooms of `spec` are hearing
    /// now.
    fn heard_in(&self, spec: &Spec, index: usize) -> Vec<usize> {
        let mut heard = Vec::new();
        for room in &spec.rooms {
            let other = self
                .room_group
                .get(room)
                .and_then(|g| self.group_source.get(g))
                .and_then(|s| s.strip_prefix("soloist:"))
                .and_then(receiver_index);
            if let Some(other) = other {
                if other != index && !heard.contains(&other) {
                    heard.push(other);
                }
            }
        }
        heard
    }
}

// ----- one receiver -----------------------------------------------------------

struct Connection {
    stream: UnixStream,
    lines: LineBuffer,
}

struct AlarmWait {
    alarm: String,
    deadline: Instant,
    started: bool,
}

struct Receiver {
    index: usize,
    socket: PathBuf,
    connection: Option<Connection>,
    next_connect: Instant,
    greeted: bool,
    build: Option<BuildReport>,
    status: Option<StatusReport>,
    /// The generation of the last `assign` or `release` sent on this
    /// connection.
    generation: u64,
    /// What was last sent on this connection: `Some(None)` is a release.
    sent: Option<Option<(String, String)>>,
    asked_auth: bool,
    logged_in: bool,
    active: bool,
    playing: Option<Status>,
    volume: Option<u8>,
    /// The volume the manager last sent and has not seen echoed.
    sent_volume: Option<u8>,
    item: Option<Entity>,
    /// Until when a `playing` report is left alone after the manager's own
    /// `pause`.
    settle_until: Option<Instant>,
    take_retry: Option<Instant>,
    alarm: Option<AlarmWait>,
    /// Whether a group played this receiver at the last look.
    was_played: bool,
    /// The group and record last written as now playing.
    shown: Option<(String, NowPlaying)>,
}

impl Receiver {
    fn new(index: usize, dir: &Path, now: Instant) -> Receiver {
        Receiver {
            index,
            socket: dir.join(socket_file_name(index)),
            connection: None,
            next_connect: now,
            greeted: false,
            build: None,
            status: None,
            generation: 0,
            sent: None,
            asked_auth: false,
            logged_in: false,
            active: false,
            playing: None,
            volume: None,
            sent_volume: None,
            item: None,
            settle_until: None,
            take_retry: None,
            alarm: None,
            was_played: false,
            shown: None,
        }
    }

    fn id(&self) -> String {
        receiver_id(self.index)
    }

    /// Forget what Soloist said: it stopped, or it is another target's now.
    fn forget_session(&mut self) {
        self.asked_auth = false;
        self.logged_in = false;
        self.active = false;
        self.playing = None;
        self.volume = None;
        self.sent_volume = None;
        self.item = None;
    }

    fn drop_connection(&mut self, now: Instant, why: &str) {
        if self.connection.take().is_some() {
            println!(
                "chorus-server: soloist receiver={} disconnected reason=\"{}\"",
                self.id(),
                why
            );
        }
        self.greeted = false;
        self.sent = None;
        self.status = None;
        self.forget_session();
        self.next_connect = now + RECONNECT;
    }

    fn running(&self) -> bool {
        self.connection.is_some()
            && self
                .status
                .as_ref()
                .is_some_and(|s| s.state == State::Running && s.generation == self.generation)
    }

    fn is_playing(&self) -> bool {
        self.playing.as_ref().is_some_and(Status::is_playing)
    }

    fn send(&mut self, message: &ToSupervisor, now: Instant) -> bool {
        let Some(connection) = self.connection.as_mut() else {
            return false;
        };
        let sent = match message.encode() {
            Ok(line) => connection.stream.write_all(line.as_bytes()),
            Err(_) => return false,
        };
        match sent {
            Ok(()) => true,
            Err(e) => {
                self.drop_connection(now, &e.to_string());
                false
            }
        }
    }

    fn command(&mut self, command: &Api, now: Instant) -> bool {
        let generation = self.generation;
        self.send(
            &ToSupervisor::Command {
                generation,
                command: command.to_value(),
            },
            now,
        )
    }

    /// `pause`, and `deactivate` when `deactivate` says so, in that order.
    fn pause(&mut self, deactivate: bool, now: Instant) {
        self.command(&Api::Pause, now);
        if deactivate {
            self.command(&Api::Deactivate, now);
        }
        self.settle_until = Some(now + PAUSE_SETTLE);
    }

    /// Everything the supervisor has sent, as messages.
    fn read(&mut self, now: Instant) -> Vec<FromSupervisor> {
        let mut messages = Vec::new();
        let Some(connection) = self.connection.as_mut() else {
            return messages;
        };
        let mut chunk = [0u8; 8192];
        let mut ended = None;
        loop {
            match connection.stream.read(&mut chunk) {
                Ok(0) => {
                    ended = Some("the supervisor closed the connection".to_string());
                    break;
                }
                Ok(n) => connection.lines.feed(&chunk[..n]),
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => {
                    ended = Some(e.to_string());
                    break;
                }
            }
        }
        loop {
            match connection.lines.next_line() {
                Ok(Some(line)) => {
                    // A kind this build does not know is skipped, as the
                    // protocol says; so is a line that is not a message.
                    if let Ok(message) = FromSupervisor::decode(&line) {
                        messages.push(message);
                    }
                }
                Ok(None) => break,
                Err(_) => {
                    ended = Some("a line longer than the protocol allows".to_string());
                    break;
                }
            }
        }
        if let Some(why) = ended {
            self.drop_connection(now, &why);
        }
        messages
    }
}

// ----- the manager ------------------------------------------------------------

struct Manager {
    settings: Settings,
    state: Arc<ControlState>,
    link: Arc<Link>,
    receivers: Vec<Receiver>,
    pool: Pool,
    origin: Instant,
    facts: Facts,
    serial: i64,
    exhausted: Vec<String>,
    pool_deadline: Option<Duration>,
    generation: u64,
    shown: Option<SoloistState>,
    warned: Option<(String, Instant)>,
    /// Set when something the pool or the state member depends on moved.
    dirty: bool,
}

fn record_of(item: Option<&Entity>, status: Option<&Status>) -> Option<NowPlaying> {
    let item = item.filter(|i| !i.is_empty())?;
    let state = match status {
        Some(Status::Playing) => PlayState::Playing,
        Some(Status::Buffering) => PlayState::Buffering,
        _ => PlayState::Paused,
    };
    Some(
        NowPlaying {
            title: item.name.clone(),
            artist: item.artist(),
            album: item.album().map(str::to_string),
            art_url: item.cover_url().map(str::to_string),
            duration_ms: item.duration_ms,
            state,
            via: "spotify".to_string(),
        }
        .bounded(),
    )
}

impl Manager {
    fn new(settings: Settings, state: Arc<ControlState>, link: Arc<Link>) -> Manager {
        let now = Instant::now();
        let receivers = (0..settings.receivers)
            .map(|i| Receiver::new(i, &settings.dir, now))
            .collect();
        Manager {
            pool: Pool::new(settings.receivers, settings.grace),
            settings,
            state,
            link,
            receivers,
            origin: now,
            facts: Facts::default(),
            serial: i64::MIN,
            exhausted: Vec::new(),
            pool_deadline: None,
            // Above any generation an earlier run of this server sent, so a
            // supervisor that outlived a restart never takes a new target's
            // events for an old one's.
            generation: wall_clock().saturating_mul(1_000),
            shown: None,
            warned: None,
            dirty: true,
        }
    }

    fn apply(&self, text: &str) -> Result<String, String> {
        self.state
            .apply(text)
            .map_err(|refusal| format!("{}: {}", refusal.field, refusal.detail))
    }

    // --- connections ---

    fn connect(&mut self, now: Instant) {
        for rx in &mut self.receivers {
            if rx.connection.is_some() || now < rx.next_connect {
                continue;
            }
            rx.next_connect = now + RECONNECT;
            let Ok(stream) = UnixStream::connect(&rx.socket) else {
                continue;
            };
            if stream.set_nonblocking(true).is_err() {
                continue;
            }
            println!("chorus-server: soloist receiver={} connected", rx.id());
            rx.connection = Some(Connection {
                stream,
                lines: LineBuffer::new(),
            });
            rx.greeted = false;
            rx.sent = None;
            self.dirty = true;
        }
    }

    // --- the state ---

    fn sync(&mut self, state: &str) {
        let Some((serial, facts)) = Facts::of(state) else {
            return;
        };
        if serial < self.serial {
            return;
        }
        self.serial = serial;
        self.facts = facts;
        self.dirty = true;
    }

    /// Run the pool over the targets, and send every connected receiver the
    /// `assign` or `release` it has not been sent.
    fn assign(&mut self, now: Instant) {
        let targets: Vec<Target> = self
            .facts
            .specs
            .iter()
            .map(|s| Target::new(&s.key, &s.name))
            .collect();
        let busy: Vec<bool> = self.receivers.iter().map(Receiver::is_playing).collect();
        let update = self
            .pool
            .update(&targets, &busy, now.duration_since(self.origin));
        self.exhausted = update.exhausted;
        self.pool_deadline = update.next_deadline;
        let wanted = self.pool.assignments();
        for (rx, wanted) in self.receivers.iter_mut().zip(wanted) {
            if rx.connection.is_none() || !rx.greeted {
                continue;
            }
            let wanted = wanted.map(|a| (a.key, a.name));
            if rx.sent.as_ref() == Some(&wanted) {
                continue;
            }
            self.generation += 1;
            rx.generation = self.generation;
            rx.forget_session();
            rx.alarm = None;
            let message = match &wanted {
                Some((key, name)) => {
                    println!(
                        "chorus-server: soloist receiver={} assign target={} name=\"{}\" \
                         generation={}",
                        rx.id(),
                        key,
                        name,
                        rx.generation
                    );
                    ToSupervisor::Assign {
                        generation: rx.generation,
                        target: key.clone(),
                        name: name.clone(),
                    }
                }
                None => {
                    println!(
                        "chorus-server: soloist receiver={} release generation={}",
                        rx.id(),
                        rx.generation
                    );
                    ToSupervisor::Release {
                        generation: rx.generation,
                    }
                }
            };
            if rx.send(&message, now) {
                rx.sent = Some(wanted);
            }
        }
    }

    // --- what the supervisors say ---

    fn handle(&mut self, index: usize, message: FromSupervisor, now: Instant) {
        match message {
            FromSupervisor::Hello { receiver, v, .. } => {
                let rx = &mut self.receivers[index];
                if receiver != index {
                    let why = format!(
                        "the supervisor on {} says it is receiver {}",
                        rx.socket.display(),
                        receiver
                    );
                    rx.drop_connection(now, &why);
                    return;
                }
                println!(
                    "chorus-server: soloist receiver={} hello protocol={}",
                    rx.id(),
                    v
                );
                rx.greeted = true;
                self.dirty = true;
            }
            FromSupervisor::Build(build) => {
                self.receivers[index].build = Some(build);
                self.dirty = true;
            }
            FromSupervisor::Status(status) => {
                let rx = &mut self.receivers[index];
                if status.detail.starts_with("command dropped")
                    || status.detail.starts_with("assign refused")
                {
                    println!(
                        "chorus-server: soloist receiver={} remark=\"{}\"",
                        rx.id(),
                        status.detail
                    );
                    return;
                }
                let running = status.state == State::Running && status.generation == rx.generation;
                rx.status = Some(status);
                if !running {
                    rx.forget_session();
                } else if !rx.asked_auth {
                    // Events sent while this server was not connected, or
                    // under a generation it did not know, were not kept.
                    rx.asked_auth = true;
                    rx.command(&Api::GetAuthState, now);
                }
                self.dirty = true;
            }
            FromSupervisor::Event { generation, event } => {
                if generation != self.receivers[index].generation {
                    return;
                }
                if let Ok(event) = api::event_from_value(&event) {
                    self.event(index, event, now);
                }
            }
        }
    }

    fn event(&mut self, index: usize, event: Event, now: Instant) {
        let rx = &mut self.receivers[index];
        match event {
            Event::AuthState {
                logged_in,
                is_active,
                ..
            } => {
                let was = rx.logged_in;
                rx.logged_in = logged_in;
                rx.active = is_active.unwrap_or(false);
                if logged_in && !was {
                    println!("chorus-server: soloist receiver={} logged-in", rx.id());
                    rx.command(&Api::GetState, now);
                } else if !logged_in {
                    rx.playing = None;
                    rx.item = None;
                    rx.volume = None;
                }
                self.dirty = true;
            }
            Event::PlaybackState(snapshot) => {
                if let Some(active) = snapshot.is_active {
                    rx.active = active;
                }
                if snapshot.item.is_some() {
                    rx.item = snapshot.item.clone();
                }
                if let Some(status) = snapshot.status.clone() {
                    self.status(index, status);
                }
                if let Some(volume) = snapshot.volume {
                    self.volume_from_receiver(index, volume, now);
                }
            }
            Event::TrackChanged { item } => rx.item = item,
            Event::PlaybackChanged {
                status: Some(status),
            } => self.status(index, status),
            Event::VolumeChanged {
                volume: Some(volume),
            } => self.volume_from_receiver(index, volume, now),
            Event::DeviceChanged { is_active, .. } => rx.active = is_active.unwrap_or(rx.active),
            Event::CommandResult { command } => {
                // A `play` on a receiver that was already playing may change
                // no status: ask, so an alarm waiting for `playing` sees it.
                if command == "play" && rx.alarm.as_ref().is_some_and(|a| !a.started) {
                    rx.command(&Api::GetState, now);
                }
            }
            Event::Error { message } => println!(
                "chorus-server: soloist receiver={} error=\"{}\"",
                rx.id(),
                message.escape_debug()
            ),
            _ => {}
        }
    }

    fn status(&mut self, index: usize, status: Status) {
        let rx = &mut self.receivers[index];
        if !status.is_playing() {
            rx.settle_until = None;
        }
        if rx.playing.as_ref() != Some(&status) {
            println!(
                "chorus-server: soloist receiver={} playback={}",
                rx.id(),
                status.as_str()
            );
            // The pool keeps a busy receiver for a dissolved group.
            self.dirty = true;
        }
        rx.playing = Some(status);
    }

    // --- volume ---

    fn set_target_volume(&self, spec: &Spec, thousandths: u16) -> Result<(), String> {
        let volume = format!("{}.{:03}", thousandths / 1000, thousandths % 1000);
        match (&spec.kind, &spec.group) {
            (Kind::Room, _) => self.apply(&format!(
                r#"{{"v":1,"t":"volume","zone":"{}","volume":{}}}"#,
                spec.take, volume
            ))?,
            (_, Some(group)) => self.apply(&format!(
                r#"{{"v":2,"t":"group_volume","group":"{}","volume":{}}}"#,
                group, volume
            ))?,
            (_, None) => {
                for room in &spec.rooms {
                    self.apply(&format!(
                        r#"{{"v":1,"t":"volume","zone":"{}","volume":{}}}"#,
                        room, volume
                    ))?;
                }
                String::new()
            }
        };
        Ok(())
    }

    fn spec_of(&self, index: usize) -> Option<Spec> {
        let key = self.pool.target_of(index)?;
        self.facts.spec(key).cloned()
    }

    /// The Spotify app (or Soloist itself) changed the receiver's volume.
    fn volume_from_receiver(&mut self, index: usize, volume: u8, now: Instant) {
        let volume = volume.min(100);
        let rx = &mut self.receivers[index];
        rx.volume = Some(volume);
        if rx.sent_volume == Some(volume) {
            // The echo of what this manager set: not a person's change.
            rx.sent_volume = None;
            return;
        }
        let Some(spec) = self.spec_of(index) else {
            return;
        };
        match self.settings.volume {
            VolumeMapping::Chorus => {
                let id = receiver_id(index);
                match self.set_target_volume(&spec, u16::from(volume) * 10) {
                    Ok(()) => println!(
                        "chorus-server: soloist receiver={} volume={} applied target={}",
                        id, volume, spec.key
                    ),
                    Err(why) => println!(
                        "chorus-server: soloist receiver={} volume={} refused=\"{}\"",
                        id, volume, why
                    ),
                }
                // What the limits made of it comes back with the state and
                // is sent to the receiver by `follow_volume`.
            }
            VolumeMapping::Receiver => {
                let limit = (spec.limit / 10).min(100) as u8;
                if volume > limit {
                    let rx = &mut self.receivers[index];
                    println!(
                        "chorus-server: soloist receiver={} volume={} clamped={} target={}",
                        rx.id(),
                        volume,
                        limit,
                        spec.key
                    );
                    rx.sent_volume = Some(limit);
                    rx.command(&Api::SetVolume { volume: limit }, now);
                }
            }
        }
    }

    /// Under the `chorus` mapping: a receiver whose volume is not its
    /// target's is told the target's, once.
    fn follow_volume(&mut self, now: Instant) {
        if self.settings.volume != VolumeMapping::Chorus {
            return;
        }
        for index in 0..self.receivers.len() {
            let rx = &self.receivers[index];
            if !rx.running() || !rx.logged_in {
                continue;
            }
            let (Some(have), Some(spec)) = (rx.volume, self.spec_of(index)) else {
                continue;
            };
            let want = ((u32::from(spec.volume) + 5) / 10).min(100) as u8;
            if want == have || rx.sent_volume == Some(want) {
                continue;
            }
            let rx = &mut self.receivers[index];
            rx.sent_volume = Some(want);
            rx.command(&Api::SetVolume { volume: want }, now);
        }
    }

    // --- take the room ---

    fn take_rooms(&mut self, now: Instant) {
        for index in 0..self.receivers.len() {
            let rx = &self.receivers[index];
            let played = self.facts.played_by(index).is_some();
            let held = rx.alarm.is_some();
            // A group stopped playing this receiver, whoever made it: the
            // Spotify app is told, and nothing plays unheard.
            let settling = rx.settle_until.is_some_and(|t| now < t);
            if rx.was_played && !played && !held && !settling && rx.running() {
                let deactivate = rx.active;
                println!(
                    "chorus-server: soloist receiver={} no group plays it any more: pause{}",
                    rx.id(),
                    if deactivate { ", deactivate" } else { "" }
                );
                self.receivers[index].pause(deactivate, now);
            }
            self.receivers[index].was_played = played;
            let rx = &self.receivers[index];
            if !rx.running() || !rx.is_playing() || held {
                continue;
            }
            if rx.settle_until.is_some_and(|t| now < t) || rx.take_retry.is_some_and(|t| now < t) {
                continue;
            }
            let Some(spec) = self.spec_of(index) else {
                continue;
            };
            let source = Source::Soloist(receiver_id(index));
            if self.facts.group_source.get(&spec.take) == Some(&source.literal()) {
                continue;
            }
            // Never two Spotify sources in one room: every other receiver a
            // room of this target hears is paused and deactivated BEFORE
            // the take (P7, the overlap clause).
            for other in self.facts.heard_in(&spec, index) {
                println!(
                    "chorus-server: soloist receiver={} displaced by={} target={}: pause, \
                     deactivate",
                    receiver_id(other),
                    receiver_id(index),
                    spec.key
                );
                self.receivers[other].pause(true, now);
            }
            let taken = self.state.apply_command(Command::Take {
                target: spec.take.clone(),
                source: Some(source.clone()),
            });
            match taken {
                Ok(state) => {
                    println!(
                        "chorus-server: soloist take target={} source={} outcome=ok",
                        spec.take,
                        source.literal()
                    );
                    self.receivers[index].take_retry = None;
                    self.receivers[index].was_played = true;
                    self.sync(&state);
                }
                Err(refusal) => {
                    println!(
                        "chorus-server: soloist take target={} source={} outcome=refused \
                         detail=\"{}: {}\"",
                        spec.take,
                        source.literal(),
                        refusal.field,
                        refusal.detail
                    );
                    self.receivers[index].take_retry = Some(now + TAKE_RETRY);
                }
            }
        }
    }

    // --- what is playing ---

    fn show_playing(&mut self) {
        for index in 0..self.receivers.len() {
            let Some(group) = self.facts.played_by(index).map(str::to_string) else {
                self.receivers[index].shown = None;
                continue;
            };
            let rx = &self.receivers[index];
            let Some(record) = record_of(rx.item.as_ref(), rx.playing.as_ref()) else {
                continue;
            };
            let wanted = (group, record);
            if rx.shown.as_ref() == Some(&wanted) {
                continue;
            }
            if self
                .state
                .set_now_playing(&wanted.0, Some(wanted.1.clone()))
                .is_ok()
            {
                self.receivers[index].shown = Some(wanted);
            }
        }
    }

    // --- requests from the rest of the server ---

    fn receiver_for(&self, target: &str) -> Option<usize> {
        let spec = self
            .facts
            .specs
            .iter()
            .find(|s| s.take == target && s.kind != Kind::Live)
            .or_else(|| self.facts.specs.iter().find(|s| s.take == target))?;
        self.pool.receiver_of(&spec.key)
    }

    fn expired(&self, rx: &Receiver) -> bool {
        rx.status.as_ref().is_some_and(|s| s.state == State::Expired)
            || rx.build.as_ref().is_some_and(|b| {
                matches!(
                    Expiry::at(b.expires_epoch, wall_clock()),
                    Expiry::Expired | Expiry::Warning { days_left: 0 }
                )
            })
    }

    fn answer(&self, answer: AlarmAnswer) {
        lock(&self.link.answers).push(answer);
        self.state.wake_conductor();
    }

    fn start_alarm(&mut self, alarm: String, target: &str, uri: &str, now: Instant) {
        let failed = |reason: &'static str, detail: String| AlarmAnswer::Failed {
            alarm: alarm.clone(),
            reason,
            detail,
        };
        let Some(index) = self.receiver_for(target) else {
            self.answer(failed(
                "soloist-unavailable",
                format!("'{}' has no receiver", target),
            ));
            return;
        };
        let rx = &self.receivers[index];
        let id = rx.id();
        let refusal = if self.expired(rx) {
            Some(("soloist-expired", format!("{}: the Soloist build expired", id)))
        } else if !rx.running() {
            let state = rx
                .status
                .as_ref()
                .map_or("absent", |s| s.state.as_str())
                .to_string();
            Some((
                "soloist-unavailable",
                format!("{}: the receiver is {}", id, state),
            ))
        } else if !rx.logged_in {
            Some((
                "soloist-logged-out",
                format!("{}: no Spotify account is logged in to this device", id),
            ))
        } else {
            None
        };
        if let Some((reason, detail)) = refusal {
            self.answer(failed(reason, detail));
            return;
        }
        println!(
            "chorus-server: soloist receiver={} alarm={} play uri={}",
            id, alarm, uri
        );
        let wait = self.settings.alarm_wait;
        let rx = &mut self.receivers[index];
        // Forgotten, so only a report made after this `play` counts.
        rx.playing = None;
        rx.settle_until = None;
        rx.alarm = Some(AlarmWait {
            alarm,
            deadline: now + wait,
            started: false,
        });
        rx.command(
            &Api::Play {
                uri: Some(uri.to_string()),
            },
            now,
        );
    }

    fn alarms(&mut self, now: Instant) {
        for index in 0..self.receivers.len() {
            let rx = &mut self.receivers[index];
            let Some(wait) = rx.alarm.as_mut() else {
                continue;
            };
            if !wait.started && rx.playing.as_ref().is_some_and(Status::is_playing) {
                wait.started = true;
                wait.deadline = now + ALARM_VERDICT;
                let alarm = wait.alarm.clone();
                self.answer(AlarmAnswer::Started {
                    alarm,
                    receiver: index,
                    source: Source::Soloist(receiver_id(index)),
                });
                continue;
            }
            if now < wait.deadline {
                continue;
            }
            let (alarm, started) = (wait.alarm.clone(), wait.started);
            rx.alarm = None;
            if !started {
                let id = rx.id();
                rx.pause(false, now);
                self.answer(AlarmAnswer::Failed {
                    alarm,
                    reason: "soloist-timeout",
                    detail: format!(
                        "{}: no 'playing' within {} s of 'play'",
                        id,
                        self.settings.alarm_wait.as_secs()
                    ),
                });
            }
        }
    }

    fn requests(&mut self, now: Instant) {
        let requests = std::mem::take(&mut *lock(&self.link.requests));
        for request in requests {
            match request {
                Request::Alarm { alarm, target, uri } => {
                    self.start_alarm(alarm, &target, &uri, now)
                }
                Request::AlarmTaken { receiver, took } => {
                    if let Some(rx) = self.receivers.get_mut(receiver) {
                        rx.alarm = None;
                        rx.was_played = took;
                        if !took {
                            rx.pause(false, now);
                        }
                    }
                }
                Request::Restart => {
                    println!("chorus-server: soloist restart: every receiver is told");
                    for rx in &mut self.receivers {
                        rx.send(&ToSupervisor::Restart, now);
                    }
                }
                Request::Playback { receiver, action } => {
                    let Some(rx) = self.receivers.get_mut(receiver) else {
                        continue;
                    };
                    let command = match action {
                        PlaybackAction::Pause => Api::Pause,
                        PlaybackAction::Resume => Api::Play { uri: None },
                        PlaybackAction::Next => Api::SkipNext,
                        PlaybackAction::Previous => Api::SkipPrev,
                    };
                    println!(
                        "chorus-server: soloist receiver={} playback command={}",
                        rx.id(),
                        command.name()
                    );
                    rx.command(&command, now);
                }
            }
        }
    }

    // --- the state member, the metrics and the warning ---

    fn publish(&mut self, now: Instant) {
        let wall = wall_clock();
        let builds: Vec<&BuildReport> = self
            .receivers
            .iter()
            .filter_map(|rx| rx.build.as_ref())
            .filter(|b| b.present)
            .collect();
        // Every receiver mounts the one binary; if they ever differ, the
        // build that expires first is the one to warn about.
        let build = builds
            .iter()
            .filter(|b| b.expires_epoch.is_some())
            .min_by_key(|b| b.expires_epoch)
            .or(builds.first())
            .copied();
        let exited_expired = self
            .receivers
            .iter()
            .any(|rx| rx.status.as_ref().is_some_and(|s| s.state == State::Expired));
        let expiry = if exited_expired {
            Expiry::Expired
        } else {
            Expiry::at(build.and_then(|b| b.expires_epoch), wall)
        };
        let assignments = self.pool.assignments();
        let shown = SoloistState {
            receivers: self
                .receivers
                .iter()
                .zip(&assignments)
                .map(|(rx, assigned)| SoloistReceiver {
                    id: rx.id(),
                    state: match (&rx.connection, &rx.status) {
                        (Some(_), Some(status)) => status.state.as_str().to_string(),
                        (Some(_), None) => "starting".to_string(),
                        (None, _) => "absent".to_string(),
                    },
                    target: assigned.as_ref().map_or(String::new(), |a| a.key.clone()),
                    name: assigned.as_ref().map_or(String::new(), |a| a.name.clone()),
                })
                .collect(),
            build: build.map(|b| SoloistBuild {
                version: b.version.clone(),
                expires_in_days: b.expires_epoch.map(|e| days_left(e, wall)),
            }),
            warning: expiry.warning(),
            exhausted: self.exhausted.clone(),
        };
        {
            let mut view = lock(&self.link.view);
            view.connected = self
                .receivers
                .iter()
                .map(|rx| rx.connection.is_some())
                .collect();
            view.expires_epoch = build.and_then(|b| b.expires_epoch);
            view.expired = expiry == Expiry::Expired;
        }
        if let Some(warning) = &shown.warning {
            let due = match &self.warned {
                Some((last, at)) => last != warning || now.duration_since(*at) >= WARNING_EVERY,
                None => true,
            };
            if due {
                println!("chorus-server: soloist warning: {}", warning);
                self.warned = Some((warning.clone(), now));
            }
        } else {
            self.warned = None;
        }
        if self.shown.as_ref() != Some(&shown) {
            self.state.set_soloist(Some(shown.clone()));
            self.shown = Some(shown);
        }
    }

    // --- the loop ---

    fn run(&mut self, keep: &AtomicBool) {
        // Subscribed before the state is read, so a change between the two
        // is in the queue rather than lost.
        let mut inbox = self.state.fanout().subscribe();
        let first = self.state.encoded_state();
        self.sync(&first);
        let mut published = Instant::now();
        while keep.load(Ordering::SeqCst) {
            let now = Instant::now();
            self.connect(now);
            let mut heard = false;
            for index in 0..self.receivers.len() {
                for message in self.receivers[index].read(now) {
                    heard = true;
                    self.handle(index, message, now);
                }
                if self.receivers[index].connection.is_none() {
                    self.dirty = true;
                }
            }
            let mut latest = None;
            let mut dropped = false;
            loop {
                match inbox.try_recv() {
                    Ok(state) => latest = Some(state),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        dropped = true;
                        break;
                    }
                }
            }
            if dropped {
                inbox = self.state.fanout().subscribe();
                let state = self.state.encoded_state();
                self.sync(&state);
            } else if let Some(state) = latest {
                self.sync(&state);
            }
            let deadline = self
                .pool_deadline
                .is_some_and(|d| now.duration_since(self.origin) >= d);
            if self.dirty || deadline {
                self.assign(now);
            }
            self.requests(now);
            self.take_rooms(now);
            self.alarms(now);
            self.show_playing();
            self.follow_volume(now);
            // The days left move with the wall clock even when nothing else
            // does: looked at once a minute.
            if self.dirty || deadline || now.duration_since(published) >= Duration::from_secs(60) {
                self.publish(now);
                published = now;
            }
            self.dirty = false;
            if !heard {
                thread::sleep(POLL);
            }
        }
    }
}

/// Create the `soloist-manager` thread. It registers itself as an ordinary
/// thread and sends one unit down `ready` before it connects to anything,
/// as every thread of the population does. Returns how many threads were
/// created: one.
pub fn spawn(
    settings: Settings,
    state: Arc<ControlState>,
    link: Arc<Link>,
    keep: &Arc<AtomicBool>,
    registry: &Arc<ThreadRegistry>,
    ready: &Sender<()>,
) -> usize {
    let keep = Arc::clone(keep);
    let registry = Arc::clone(registry);
    let ready = ready.clone();
    thread::spawn(move || {
        register_ordinary_thread("soloist-manager", &registry);
        if ready.send(()).is_err() {
            return;
        }
        drop(ready);
        Manager::new(settings, state, link).run(&keep);
    });
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATE: &str = concat!(
        r#"{"v":2,"t":"state","serial":7,"zones":["#,
        r#"{"id":"kitchen","name":"Kitchen","group":"downstairs","volume":0.400,"muted":false},"#,
        r#"{"id":"den","name":"Den","group":"downstairs","volume":0.600,"muted":false},"#,
        r#"{"id":"study","name":"Study","group":"study","volume":1.000,"muted":false}],"#,
        r#""groups":[{"id":"downstairs","kind":"saved","zones":["kitchen","den"],"volume":0.500,"#,
        r#""source":"soloist:r3"},"#,
        r#"{"id":"study","kind":"room","zones":["study"],"volume":1.000,"source":"soloist:r2"}],"#,
        r#""saved_groups":[{"id":"downstairs","name":"Downstairs","zones":["kitchen","den"]}]}"#
    );

    #[test]
    fn the_facts_say_which_group_plays_a_receiver_and_what_a_targets_rooms_hear() {
        let (serial, facts) = Facts::of(STATE).unwrap();
        assert_eq!(serial, 7);
        assert_eq!(facts.played_by(3), Some("downstairs"));
        assert_eq!(facts.played_by(2), Some("study"));
        assert_eq!(facts.played_by(0), None);
        let kitchen = facts.spec("room:kitchen").unwrap();
        assert_eq!(facts.heard_in(kitchen, 0), [3], "its group's receiver");
        let saved = facts.spec("group:downstairs").unwrap();
        assert!(facts.heard_in(saved, 3).is_empty(), "its own is not another");
        assert_eq!(facts.heard_in(saved, 1), [3]);
    }

    #[test]
    fn a_record_is_the_items_names_and_the_large_cover_via_spotify() {
        let event = api::parse_event(concat!(
            r#"{"type":"track_changed","item":{"uri":"spotify:track:a","entity_type":"track","#,
            r#""decorations":{"identity":{"name":"My Song"},"visual_identity":{"cover":["#,
            r#"{"url":"https://i.example/s.jpg","size":"small"},"#,
            r#"{"url":"https://i.example/l.jpg","size":"large"}]},"#,
            r#""parent":{"entity":{"decorations":{"identity":{"name":"Album Name"}}}},"#,
            r#""creators":[{"entity":{"decorations":{"identity":{"name":"One"}}}},"#,
            r#"{"entity":{"decorations":{"identity":{"name":"Two"}}}}],"#,
            r#""playback":{"duration_ms":210000}}}}"#
        ))
        .unwrap();
        let Event::TrackChanged { item } = event else {
            panic!("not a track change");
        };
        let record = record_of(item.as_ref(), Some(&Status::Playing)).unwrap();
        assert_eq!(record.title.as_deref(), Some("My Song"));
        assert_eq!(record.artist.as_deref(), Some("One, Two"));
        assert_eq!(record.album.as_deref(), Some("Album Name"));
        assert_eq!(record.art_url.as_deref(), Some("https://i.example/l.jpg"));
        assert_eq!(record.duration_ms, Some(210_000));
        assert_eq!((record.state, record.via.as_str()), (PlayState::Playing, "spotify"));
        let paused = record_of(item.as_ref(), Some(&Status::Paused)).unwrap();
        assert_eq!(paused.state, PlayState::Paused);
        assert_eq!(record_of(None, Some(&Status::Playing)), None);
    }

    #[test]
    fn the_volume_mappings_are_two_words() {
        assert_eq!(VolumeMapping::parse("chorus"), Some(VolumeMapping::Chorus));
        assert_eq!(VolumeMapping::parse("receiver"), Some(VolumeMapping::Receiver));
        assert_eq!(VolumeMapping::parse("both"), None);
        assert_eq!(VolumeMapping::default().name(), "chorus");
    }

    #[test]
    fn the_metrics_name_every_receiver_and_the_seconds_to_expiry() {
        let ports = vec![Arc::new(SoloistPort::new(44_100, 2, 10, 2))];
        let link = Link::new(ports, vec![Arc::new(ReaderStats::default())]);
        let text = link.metrics();
        assert!(text.contains("# TYPE chorus_soloist_build_expires_seconds gauge\n"));
        assert!(!text.contains("\nchorus_soloist_build_expires_seconds "), "unknown: no sample");
        assert!(text.contains("chorus_soloist_receiver_connected{receiver=\"r0\"} 0\n"));
        assert!(text.contains("chorus_soloist_underruns_total{receiver=\"r0\"} 0\n"));
        lock(&link.view).expires_epoch = Some(wall_clock() + 1_000);
        let text = link.metrics();
        let line = text
            .lines()
            .find(|l| l.starts_with("chorus_soloist_build_expires_seconds "))
            .unwrap();
        let left: i64 = line.rsplit(' ').next().unwrap().parse().unwrap();
        assert!((990..=1_000).contains(&left), "{}", line);
    }
}
