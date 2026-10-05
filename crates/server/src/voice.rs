//! The voice path on the server: what it does with a speaker's microphone
//! audio (proposal P8, Option A; K73, I4; `docs/decisions/0166-*` for the
//! wire, `docs/decisions/0169-*` for the intake, `docs/decisions/0172-*` for
//! the wake word, the run and the route).
//!
//! # The rule
//!
//! A `mic_audio` frame is kept only when all of these hold at the moment it
//! arrives, and is dropped and counted otherwise ([`DropReason`]):
//!
//! 1. its session declared the `voice` role;
//! 2. its endpoint is in a room;
//! 3. that room has voice switched on (`voice_enabled`, off by default);
//! 4. the session's own last `mic_state` said the gate is `live` (a session
//!    that has said nothing is muted).
//!
//! The first three are read from the room model by the caller for every
//! frame, so a room switched off drops its next frame, whether or not the
//! endpoint has yet obeyed the `voice_control` that tells it to stop. The
//! fourth is the endpoint's own word about its hardware switch: nothing here
//! or anywhere in the server opens it.
//!
//! # The wake word
//!
//! Every kept sample is run, once, through the wake-word models the server
//! carries (`chorus_wakeword::builtin`), one detector per model per session.
//! That happens in [`Voice::pass`], on the event writer's thread and never on
//! the session's reader, which only appends to the buffer and wakes the
//! writer: an inference never delays a session. A detection is one
//! [`Wake`] (the room and the phrase, nothing else), unless the room has a
//! run open or was reported within [`WAKE_HOLDOFF`] (two microphones of one
//! room hearing one utterance are one event).
//!
//! # The run
//!
//! A run is the only thing that lets microphone audio leave this module, and
//! it is opened by a command ([`Voice::start`]), never by a wake word. It
//! belongs to one room and takes its audio from one session of that room:
//! the one that heard the room's wake word within [`WAKE_GRACE`], else the
//! first with a live gate. From then on that session's kept frames are also
//! queued for the run (at most [`RUN_QUEUE_BYTES`], the oldest falling off),
//! and the one reader that claimed the run with its identifier
//! ([`Voice::claim`]) is handed them ([`Voice::read`]). A run opened after a
//! wake word starts with what the session buffered since the detection, so
//! the first word of the command is not lost; one opened without starts
//! empty.
//!
//! A run ends, and its queue is overwritten and emptied, when it is stopped,
//! at its time limit on the monotonic clock, when its session's gate closes,
//! when its room's voice is switched off, when its session ends, when its
//! reader goes and when another run is opened in its room. Nothing brings an
//! ended run back.
//!
//! # Where the audio is, and where it is not (I4)
//!
//! A kept frame's samples are in this session's buffer (a ring of at most
//! [`BUFFER_SAMPLES`] samples, the oldest falling off), in a detector's
//! state while it is being heard, and, during a run, in that run's queue.
//! All three are memory. The buffer is wiped when the room's voice is
//! switched off, when the gate closes and when the session ends, so a muted
//! or disabled room holds no audio at all. Nothing in this module writes a
//! file or opens a socket, and it has no reference to the router, a slot, a
//! line-in port or the visualizer. The one way out is [`Voice::read`], which
//! the event writer calls for the reader of one open run
//! (`crate::events`, `GET /api/voice-audio`). A status line names counts and
//! never a sample, and never a run's identifier.
//!
//! Control code: it stamps nothing and reads no clock. The monotonic instants
//! a run's limit and a wake word's hold-off are measured with are the
//! caller's (`std::time::Instant`), so no wall clock can move them.
//! `audio-path.conf` records it as excluded.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use chorus_protocol::v2::{MicAudio, VoiceControl, MIC_BYTES_PER_SAMPLE, MIC_SAMPLE_RATE_HZ};
use chorus_wakeword::Detector;

/// The most samples one session's buffer holds: three seconds at the wire's
/// 16 kHz. ASSUMED: enough to carry what was said between a wake word and the
/// run that follows it, not measured.
pub const BUFFER_SAMPLES: usize = MIC_SAMPLE_RATE_HZ as usize * 3;

/// The most bytes one session's buffer holds.
pub const BUFFER_BYTES: usize = BUFFER_SAMPLES * MIC_BYTES_PER_SAMPLE;

/// The most bytes a run holds for its reader: three seconds. A reader slower
/// than the microphone loses the oldest, counted. ASSUMED, not measured.
pub const RUN_QUEUE_BYTES: usize = BUFFER_BYTES;

/// The longest a run lasts when the server is not told otherwise
/// (`--voice-run-limit-ms`). ASSUMED: longer than a spoken command and its
/// transcription take, short enough that a run nobody ends does not listen
/// for long; not measured.
pub const RUN_LIMIT: Duration = Duration::from_secs(30);

/// The longest a run may be configured to last.
pub const RUN_LIMIT_MAX: Duration = Duration::from_secs(120);

/// How long after a room's wake word another detection in that room is the
/// same utterance and no second event. ASSUMED, not measured.
pub const WAKE_HOLDOFF: Duration = Duration::from_secs(2);

/// How long after a room's wake word a run opened there starts at the wake
/// word rather than at the command. ASSUMED, not measured.
pub const WAKE_GRACE: Duration = Duration::from_secs(5);

/// Why a `mic_audio` frame was not kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropReason {
    /// The session's `hello` did not declare the voice role.
    NoVoiceRole,
    /// The endpoint is in no room.
    NoRoom,
    /// The endpoint's room has voice switched off.
    VoiceDisabled,
    /// The session's last `mic_state` said muted, or it has sent none.
    GateMuted,
}

impl DropReason {
    /// Every reason, in the order the counters are kept.
    pub const ALL: [DropReason; 4] = [
        DropReason::NoVoiceRole,
        DropReason::NoRoom,
        DropReason::VoiceDisabled,
        DropReason::GateMuted,
    ];

    /// The reason as a status line names it.
    pub fn name(self) -> &'static str {
        match self {
            DropReason::NoVoiceRole => "no-voice-role",
            DropReason::NoRoom => "no-room",
            DropReason::VoiceDisabled => "voice-disabled",
            DropReason::GateMuted => "gate-muted",
        }
    }

    fn index(self) -> usize {
        match self {
            DropReason::NoVoiceRole => 0,
            DropReason::NoRoom => 1,
            DropReason::VoiceDisabled => 2,
            DropReason::GateMuted => 3,
        }
    }

    /// Why a run whose session's frame was dropped for this reason ended.
    fn ends_a_run_as(self) -> &'static str {
        match self {
            DropReason::GateMuted => "muted",
            DropReason::VoiceDisabled => "voice-disabled",
            DropReason::NoRoom | DropReason::NoVoiceRole => "no-room",
        }
    }
}

/// What became of one `mic_audio` frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intake {
    /// Its samples are in the session's buffer.
    Buffered,
    /// It was dropped, and counted under this reason.
    Dropped(DropReason),
}

/// The room a frame's endpoint is in, as the caller read it off the room
/// model when the frame arrived.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoomVoice<'a> {
    /// The room's id.
    pub room: &'a str,
    /// Whether the room has voice switched on.
    pub enabled: bool,
}

/// A wake word heard in a room: what the control plane publishes. It has no
/// audio and no run in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wake {
    /// The room whose microphone heard it.
    pub room: String,
    /// The phrase, as the model's manifest spells it.
    pub phrase: String,
}

/// What one [`Voice::pass`] found.
#[derive(Debug, Default)]
pub struct Pass {
    /// The wake words heard since the last pass, in order.
    pub wakes: Vec<Wake>,
    /// The status lines to print: counts and names, never a sample and never
    /// a run's identifier.
    pub lines: Vec<String>,
    /// When the next open run reaches its limit, if any is open.
    pub due: Option<Instant>,
}

/// Why a reader was not given a run ([`Voice::claim`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimRefused {
    /// No open run has that identifier: there is none, or it is another's.
    /// One answer for both, so a wrong guess learns nothing.
    NoRun,
    /// The run has a reader already.
    Taken,
}

/// What a run's reader is handed ([`Voice::read`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunRead {
    /// The next bytes: whole 16-bit little-endian samples at 16 kHz mono.
    Audio(Vec<u8>),
    /// Nothing yet; the run is open.
    Nothing,
    /// The run has ended. Nothing more will come.
    Ended,
}

struct Entry {
    session: u64,
    endpoint: String,
    voice_role: bool,
    gate_live: bool,
    told: VoiceControl,
    buffer: VecDeque<u8>,
    last: Option<Intake>,
    buffered_frames: u64,
    dropped_frames: u64,
    /// The room its last kept frame was in.
    room: Option<String>,
    /// How many bytes at the end of the buffer no detector has heard yet.
    unheard: usize,
    /// Moves whenever what the detectors heard no longer leads up to what
    /// comes next (a wipe, a run): they start again.
    generation: u64,
    /// How many bytes at the end of the buffer came after the session's last
    /// wake word; `None` when it heard none since the last wipe.
    since_wake: Option<usize>,
}

impl Entry {
    /// Overwrite what the buffer holds, then empty it: a muted or disabled
    /// room keeps no audio.
    fn wipe(&mut self) {
        for byte in self.buffer.iter_mut() {
            *byte = 0;
        }
        self.buffer.clear();
        self.unheard = 0;
        self.since_wake = None;
        self.generation += 1;
    }

    fn counts(&self) -> String {
        format!(
            "buffered_frames={} dropped_frames={}",
            self.buffered_frames, self.dropped_frames
        )
    }
}

/// One open run.
struct Run {
    id: String,
    room: String,
    session: u64,
    endpoint: String,
    deadline: Instant,
    queue: VecDeque<u8>,
    claimed: bool,
    served_bytes: u64,
    lost_bytes: u64,
}

/// The last wake word reported in a room.
struct Heard {
    room: String,
    session: u64,
    at: Instant,
}

#[derive(Default)]
struct Inner {
    entries: Vec<Entry>,
    runs: Vec<Run>,
    heard: Vec<Heard>,
    /// Status lines waiting for the next pass to print them.
    notes: Vec<String>,
}

impl Inner {
    /// End every run `which` picks: its queue is overwritten and emptied,
    /// its room's detectors start again, and a line says why. Whether any
    /// ended.
    fn end_runs(&mut self, which: impl Fn(&Run) -> bool, reason: &str) -> bool {
        let mut any = false;
        let mut at = 0;
        while at < self.runs.len() {
            if !which(&self.runs[at]) {
                at += 1;
                continue;
            }
            let mut run = self.runs.remove(at);
            for byte in run.queue.iter_mut() {
                *byte = 0;
            }
            run.queue.clear();
            for entry in self
                .entries
                .iter_mut()
                .filter(|e| e.room.as_deref() == Some(run.room.as_str()))
            {
                // What was said during the run was the run's, not a wake word.
                entry.unheard = 0;
                entry.since_wake = None;
                entry.generation += 1;
            }
            self.heard.retain(|h| h.room != run.room);
            self.notes.push(format!(
                "voice run room={} id={} ended reason={} read={} served_bytes={} lost_bytes={}",
                run.room,
                run.endpoint,
                reason,
                u8::from(run.claimed),
                run.served_bytes,
                run.lost_bytes
            ));
            any = true;
        }
        any
    }
}

/// One session's detectors, held by the pass alone.
struct Listener {
    session: u64,
    generation: u64,
    /// Samples fed since the detectors last started.
    fed: u64,
    detectors: Vec<Detector>,
}

/// What a pass wakes when something it or another thread should look at
/// changed.
struct Hooks {
    writer: SyncSender<()>,
    conductor: SyncSender<()>,
}

/// Every session's microphone state and buffer, the open runs, and the
/// counters.
pub struct Voice {
    inner: Mutex<Inner>,
    /// Taken by [`Voice::pass`] alone, and never while `inner` is held for
    /// longer than a copy: an inference blocks no session.
    listeners: Mutex<Vec<Listener>>,
    dropped: [AtomicU64; 4],
    buffered: AtomicU64,
    wakes: AtomicU64,
    runs: AtomicU64,
    limit: Duration,
    hooks: OnceLock<Hooks>,
}

impl Default for Voice {
    fn default() -> Voice {
        Voice::new()
    }
}

impl std::fmt::Debug for Voice {
    // Counts only: a buffer, a queue and a run's identifier are never printed.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = self.locked();
        f.debug_struct("Voice")
            .field("sessions", &inner.entries.len())
            .field("open_runs", &inner.runs.len())
            .field("buffered_frames", &self.buffered_frames())
            .finish_non_exhaustive()
    }
}

const OFF: VoiceControl = VoiceControl {
    uplink: false,
    listening: false,
};

/// Whether two identifiers are the same, in time that depends on their
/// lengths alone.
fn same_id(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

impl Voice {
    /// No session, nothing buffered, nothing dropped, no run; a run lasts at
    /// most [`RUN_LIMIT`].
    pub fn new() -> Voice {
        Voice::with_run_limit(RUN_LIMIT)
    }

    /// [`Voice::new`], with runs that last at most `limit`.
    pub fn with_run_limit(limit: Duration) -> Voice {
        Voice {
            inner: Mutex::new(Inner::default()),
            listeners: Mutex::new(Vec::new()),
            dropped: Default::default(),
            buffered: AtomicU64::new(0),
            wakes: AtomicU64::new(0),
            runs: AtomicU64::new(0),
            limit,
            hooks: OnceLock::new(),
        }
    }

    /// Wake `writer` (the thread that calls [`Voice::pass`]) when there is
    /// audio to hear or a run changed, and `conductor` when what a room's
    /// endpoints are told (`listening`) changed. Each is a bounded
    /// `try_send` that never blocks. Called once, at start.
    pub fn connect(&self, writer: SyncSender<()>, conductor: SyncSender<()>) {
        let _ = self.hooks.set(Hooks { writer, conductor });
    }

    fn poke_writer(&self) {
        if let Some(hooks) = self.hooks.get() {
            let _ = hooks.writer.try_send(());
        }
    }

    /// A run opened or ended: the writer has a deadline or a reader to look
    /// at, and the conductor a `listening` to send.
    fn poke(&self) {
        if let Some(hooks) = self.hooks.get() {
            let _ = hooks.writer.try_send(());
            let _ = hooks.conductor.try_send(());
        }
    }

    fn locked(&self) -> MutexGuard<'_, Inner> {
        match self.inner.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// The longest a run lasts.
    pub fn run_limit(&self) -> Duration {
        self.limit
    }

    /// A session is up. Its gate is muted and its uplink off until it and
    /// the server say otherwise (`docs/protocol.md`, "The voice role").
    pub fn session_up(&self, session: u64, endpoint: &str, voice_role: bool) {
        let mut inner = self.locked();
        inner.entries.retain(|e| e.session != session);
        inner.entries.push(Entry {
            session,
            endpoint: endpoint.to_string(),
            voice_role,
            gate_live: false,
            told: OFF,
            buffer: VecDeque::new(),
            last: None,
            buffered_frames: 0,
            dropped_frames: 0,
            room: None,
            unheard: 0,
            generation: 0,
            since_wake: None,
        });
    }

    /// A session has ended: its buffer is wiped, a run it fed ends, and it
    /// is forgotten. Returns whether another session of the same endpoint
    /// still reports a live gate (what the room model is then told of the
    /// endpoint), and the status line when the session had sent any
    /// microphone audio.
    pub fn session_down(&self, session: u64) -> (bool, Option<String>) {
        let mut inner = self.locked();
        let Some(at) = inner.entries.iter().position(|e| e.session == session) else {
            return (false, None);
        };
        let mut entry = inner.entries.remove(at);
        entry.wipe();
        let ended = inner.end_runs(|r| r.session == session, "session-ended");
        inner.heard.retain(|h| h.session != session);
        let still_live = inner
            .entries
            .iter()
            .any(|e| e.endpoint == entry.endpoint && e.gate_live);
        let line = (entry.buffered_frames + entry.dropped_frames > 0)
            .then(|| format!("voice mic id={} ended {}", entry.endpoint, entry.counts()));
        drop(inner);
        if ended {
            self.poke();
        }
        (still_live, line)
    }

    /// The session's endpoint said its gate is `live` or muted (`mic_state`).
    /// Closing it wipes the buffer and ends a run the session fed. `None`
    /// for a session that did not declare the voice role (its word is not
    /// taken) or is not known; else whether the gate changed, and the status
    /// line when it did.
    pub fn gate(&self, session: u64, live: bool) -> Option<(bool, Option<String>)> {
        let mut inner = self.locked();
        let entry = inner
            .entries
            .iter_mut()
            .find(|e| e.session == session && e.voice_role)?;
        if entry.gate_live == live {
            return Some((false, None));
        }
        entry.gate_live = live;
        if !live {
            entry.wipe();
        }
        let line = format!(
            "voice mic id={} gate={} {}",
            entry.endpoint,
            if live { "live" } else { "muted" },
            entry.counts()
        );
        let ended = !live && inner.end_runs(|r| r.session == session, "muted");
        drop(inner);
        if ended {
            self.poke();
        }
        Some((true, Some(line)))
    }

    /// One `mic_audio` frame from `session`, whose endpoint is in `room`
    /// (`None`: in no room) as the room model says now. Returns what became
    /// of it, and a status line when that differs from what became of the
    /// session's frame before (so a stream of frames is one line, not one a
    /// frame). The line carries counts, never samples.
    ///
    /// A kept frame is also queued for the run its session feeds, if there
    /// is one; a dropped frame ends that run.
    pub fn audio(
        &self,
        session: u64,
        room: Option<RoomVoice<'_>>,
        frame: &MicAudio,
    ) -> (Intake, Option<String>) {
        let mut guard = self.locked();
        let inner = &mut *guard;
        let Some(entry) = inner.entries.iter_mut().find(|e| e.session == session) else {
            // A session this module was never told of declared nothing.
            self.dropped[DropReason::NoVoiceRole.index()].fetch_add(1, Ordering::Relaxed);
            return (Intake::Dropped(DropReason::NoVoiceRole), None);
        };
        let intake = match room {
            _ if !entry.voice_role => Intake::Dropped(DropReason::NoVoiceRole),
            None => Intake::Dropped(DropReason::NoRoom),
            Some(r) if !r.enabled => Intake::Dropped(DropReason::VoiceDisabled),
            Some(_) if !entry.gate_live => Intake::Dropped(DropReason::GateMuted),
            Some(_) => Intake::Buffered,
        };
        let mut ended = false;
        match intake {
            Intake::Buffered => {
                // The decoder holds a frame to whole samples and to at most
                // MIC_MAX_SAMPLES of them, far below the bound.
                let data = &frame.data[..frame.data.len().min(BUFFER_BYTES)];
                let over = (entry.buffer.len() + data.len()).saturating_sub(BUFFER_BYTES);
                entry.buffer.drain(..over);
                entry.buffer.extend(data.iter().copied());
                entry.buffered_frames += 1;
                entry.unheard = (entry.unheard + data.len()).min(BUFFER_BYTES);
                if let Some(since) = entry.since_wake.as_mut() {
                    *since = (*since + data.len()).min(BUFFER_BYTES);
                }
                let now_in = room.map(|r| r.room);
                if entry.room.as_deref() != now_in {
                    entry.room = now_in.map(str::to_string);
                }
                self.buffered.fetch_add(1, Ordering::Relaxed);
                if let Some(run) = inner.runs.iter_mut().find(|r| r.session == session) {
                    let over = (run.queue.len() + data.len()).saturating_sub(RUN_QUEUE_BYTES);
                    for byte in run.queue.iter_mut().take(over) {
                        *byte = 0;
                    }
                    run.queue.drain(..over);
                    run.lost_bytes += over as u64;
                    run.queue.extend(data.iter().copied());
                }
            }
            Intake::Dropped(reason) => {
                // What was kept while the room was listening does not
                // outlast it.
                entry.wipe();
                entry.dropped_frames += 1;
                self.dropped[reason.index()].fetch_add(1, Ordering::Relaxed);
            }
        }
        let line = (entry.last != Some(intake)).then(|| {
            let room = room.map_or("-", |r| r.room);
            match intake {
                Intake::Buffered => format!(
                    "voice mic id={} room={} intake=buffering {}",
                    entry.endpoint,
                    room,
                    entry.counts()
                ),
                Intake::Dropped(reason) => format!(
                    "voice mic id={} room={} intake=dropping reason={} {}",
                    entry.endpoint,
                    room,
                    reason.name(),
                    entry.counts()
                ),
            }
        });
        entry.last = Some(intake);
        if let Intake::Dropped(reason) = intake {
            ended = inner.end_runs(|r| r.session == session, reason.ends_a_run_as());
        }
        drop(guard);
        if ended {
            self.poke();
        } else if intake == Intake::Buffered {
            self.poke_writer();
        }
        (intake, line)
    }

    /// What `session` was last told (`voice_control`); off and not listening
    /// until told otherwise. `None` for a session not known here.
    pub fn told(&self, session: u64) -> Option<VoiceControl> {
        self.locked()
            .entries
            .iter()
            .find(|e| e.session == session)
            .map(|e| e.told)
    }

    /// The server has sent `session` this `voice_control`. Turning the uplink
    /// off wipes the buffer and ends a run the session fed. Returns the
    /// status line, or `None` for a session not known here.
    pub fn tell(&self, session: u64, control: VoiceControl) -> Option<String> {
        let mut inner = self.locked();
        let entry = inner.entries.iter_mut().find(|e| e.session == session)?;
        entry.told = control;
        if !control.uplink {
            entry.wipe();
        }
        let line = format!(
            "voice control id={} uplink={} listening={} {}",
            entry.endpoint,
            u8::from(control.uplink),
            u8::from(control.listening),
            entry.counts()
        );
        let ended = !control.uplink && inner.end_runs(|r| r.session == session, "voice-disabled");
        drop(inner);
        if ended {
            self.poke();
        }
        Some(line)
    }

    /// A copy of what `endpoint`'s buffers hold, oldest first: whole 16-bit
    /// little-endian samples at 16 kHz mono.
    pub fn buffered(&self, endpoint: &str) -> Vec<u8> {
        self.locked()
            .entries
            .iter()
            .filter(|e| e.endpoint == endpoint)
            .flat_map(|e| e.buffer.iter().copied())
            .collect()
    }

    /// [`Voice::buffered`], and the buffers are left wiped.
    pub fn take(&self, endpoint: &str) -> Vec<u8> {
        let mut out = Vec::new();
        for entry in self
            .locked()
            .entries
            .iter_mut()
            .filter(|e| e.endpoint == endpoint)
        {
            out.extend(entry.buffer.iter().copied());
            entry.wipe();
        }
        out
    }

    /// Frames dropped for `reason` since this server started.
    pub fn dropped(&self, reason: DropReason) -> u64 {
        self.dropped[reason.index()].load(Ordering::Relaxed)
    }

    /// Frames dropped for any reason since this server started.
    pub fn dropped_frames(&self) -> u64 {
        DropReason::ALL.iter().map(|r| self.dropped(*r)).sum()
    }

    /// Frames kept since this server started.
    pub fn buffered_frames(&self) -> u64 {
        self.buffered.load(Ordering::Relaxed)
    }

    /// Wake words reported since this server started.
    pub fn wakes(&self) -> u64 {
        self.wakes.load(Ordering::Relaxed)
    }

    /// Runs opened since this server started.
    pub fn runs_opened(&self) -> u64 {
        self.runs.load(Ordering::Relaxed)
    }
}

/// The wake word and the run.
impl Voice {
    /// Hear what has been kept since the last pass, end the runs past their
    /// limit, and hand back what there is to say. `now` is the caller's
    /// monotonic clock.
    ///
    /// Called from one thread (the event writer). The sessions' lock is held
    /// only to copy the unheard samples out and to record what was found;
    /// the models run with it released.
    pub fn pass(&self, now: Instant) -> Pass {
        // 1. Runs past their limit, and the samples nobody has heard.
        let mut unheard: Vec<(u64, u64, Vec<u8>)> = Vec::new();
        let ended = {
            let mut guard = self.locked();
            let inner = &mut *guard;
            let ended = inner.end_runs(|r| now >= r.deadline, "limit");
            for entry in inner.entries.iter_mut() {
                if entry.unheard == 0 || !entry.voice_role {
                    continue;
                }
                let in_a_run = entry
                    .room
                    .as_deref()
                    .is_some_and(|room| inner.runs.iter().any(|r| r.room == room));
                let count = std::mem::take(&mut entry.unheard).min(entry.buffer.len());
                if in_a_run {
                    // The room is being listened to already: this is the
                    // run's, and no wake word.
                    continue;
                }
                let from = entry.buffer.len() - count;
                unheard.push((
                    entry.session,
                    entry.generation,
                    entry.buffer.iter().skip(from).copied().collect(),
                ));
            }
            ended
        };
        if ended {
            self.poke();
        }
        // 2. The models, with no session waiting on them.
        let mut found: Vec<(u64, u64, String, u64)> = Vec::new();
        let mut lines = Vec::new();
        {
            let mut listeners = match self.listeners.lock() {
                Ok(g) => g,
                Err(poisoned) => poisoned.into_inner(),
            };
            for (session, generation, mut bytes) in unheard {
                let at = match listeners.iter().position(|l| l.session == session) {
                    Some(at) => at,
                    None => {
                        let mut detectors = Vec::new();
                        for builtin in chorus_wakeword::builtin() {
                            match Detector::from_builtin(builtin) {
                                Ok(detector) => detectors.push(detector),
                                Err(e) => lines.push(format!(
                                    "voice wake model={} refused: {}",
                                    builtin.name, e
                                )),
                            }
                        }
                        listeners.push(Listener {
                            session,
                            generation,
                            fed: 0,
                            detectors,
                        });
                        listeners.len() - 1
                    }
                };
                let listener = &mut listeners[at];
                if listener.generation != generation {
                    for detector in listener.detectors.iter_mut() {
                        detector.reset();
                    }
                    listener.generation = generation;
                    listener.fed = 0;
                }
                let samples: Vec<i16> = bytes
                    .as_chunks::<MIC_BYTES_PER_SAMPLE>()
                    .0
                    .iter()
                    .map(|pair| i16::from_le_bytes(*pair))
                    .collect();
                for byte in bytes.iter_mut() {
                    *byte = 0;
                }
                listener.fed += samples.len() as u64;
                for detector in listener.detectors.iter_mut() {
                    for detection in detector.process(&samples) {
                        found.push((
                            session,
                            generation,
                            detection.phrase,
                            listener.fed.saturating_sub(detection.at_sample),
                        ));
                    }
                }
            }
        }
        // 3. What was found, against the rooms as they are now.
        let mut guard = self.locked();
        let inner = &mut *guard;
        let known: Vec<u64> = inner.entries.iter().map(|e| e.session).collect();
        let mut wakes = Vec::new();
        for (session, generation, phrase, samples_after) in found {
            let Some(entry) = inner
                .entries
                .iter_mut()
                .find(|e| e.session == session && e.generation == generation)
            else {
                continue;
            };
            let Some(room) = entry.room.clone() else {
                continue;
            };
            if inner.runs.iter().any(|r| r.room == room) {
                continue;
            }
            if inner
                .heard
                .iter()
                .any(|h| h.room == room && now.saturating_duration_since(h.at) < WAKE_HOLDOFF)
            {
                continue;
            }
            let after = samples_after as usize * MIC_BYTES_PER_SAMPLE + entry.unheard;
            entry.since_wake = Some(after.min(entry.buffer.len()));
            inner.heard.retain(|h| h.room != room);
            inner.heard.push(Heard {
                room: room.clone(),
                session,
                at: now,
            });
            self.wakes.fetch_add(1, Ordering::Relaxed);
            inner.notes.push(format!(
                "voice wake room={} id={} phrase=\"{}\"",
                room, entry.endpoint, phrase
            ));
            wakes.push(Wake { room, phrase });
        }
        lines.append(&mut inner.notes);
        let due = inner.runs.iter().map(|r| r.deadline).min();
        drop(guard);
        // A session that is gone takes its detectors with it.
        if let Ok(mut listeners) = self.listeners.lock() {
            listeners.retain(|l| known.contains(&l.session));
        }
        Pass { wakes, lines, due }
    }

    /// Open a run in `room`, whose endpoints are `endpoints`, under the
    /// identifier `id` (the caller's, from the system's random source),
    /// ending the room's open run if it has one. `None` when no voice
    /// session of the room has a live gate: there is nothing to listen to.
    /// Otherwise how long the run may last, counted from `now`.
    pub fn start(
        &self,
        room: &str,
        endpoints: &[String],
        id: String,
        now: Instant,
    ) -> Option<Duration> {
        let mut guard = self.locked();
        let inner = &mut *guard;
        let live = |e: &Entry| e.voice_role && e.gate_live && endpoints.contains(&e.endpoint);
        let heard = inner
            .heard
            .iter()
            .find(|h| h.room == room && now.saturating_duration_since(h.at) <= WAKE_GRACE)
            .map(|h| h.session);
        let after_wake = heard
            .and_then(|session| inner.entries.iter().position(|e| e.session == session))
            .filter(|at| live(&inner.entries[*at]) && inner.entries[*at].since_wake.is_some());
        let at = after_wake.or_else(|| inner.entries.iter().position(live))?;
        // Read before the room's open run is ended, which forgets the wake
        // word with it.
        let since = after_wake.map_or(0, |at| heard_bytes(&inner.entries[at]));
        inner.end_runs(|r| r.room == room, "superseded");
        let entry = &mut inner.entries[at];
        let mut queue = VecDeque::new();
        queue.extend(
            entry
                .buffer
                .iter()
                .skip(entry.buffer.len() - since)
                .copied(),
        );
        entry.since_wake = None;
        entry.unheard = 0;
        entry.generation += 1;
        entry.room = Some(room.to_string());
        let (session, endpoint) = (entry.session, entry.endpoint.clone());
        inner.heard.retain(|h| h.room != room);
        inner.notes.push(format!(
            "voice run room={} id={} started limit_ms={} after_wake={}",
            room,
            endpoint,
            self.limit.as_millis(),
            u8::from(after_wake.is_some())
        ));
        inner.runs.push(Run {
            id,
            room: room.to_string(),
            session,
            endpoint,
            deadline: now + self.limit,
            queue,
            claimed: false,
            served_bytes: 0,
            lost_bytes: 0,
        });
        self.runs.fetch_add(1, Ordering::Relaxed);
        drop(guard);
        self.poke();
        Some(self.limit)
    }

    /// End `room`'s run, if it has one. Whether it had.
    pub fn stop(&self, room: &str) -> bool {
        let ended = self.locked().end_runs(|r| r.room == room, "stopped");
        if ended {
            self.poke();
        }
        ended
    }

    /// Whether `room` has a run open: what its voice endpoints are told as
    /// `voice_control.listening`.
    pub fn listening(&self, room: &str) -> bool {
        self.locked().runs.iter().any(|r| r.room == room)
    }

    /// Become the one reader of the open run whose identifier is `offered`.
    /// A run past its limit is ended first, so it is no run.
    pub fn claim(&self, offered: &str, now: Instant) -> Result<(), ClaimRefused> {
        let mut inner = self.locked();
        let ended = inner.end_runs(|r| now >= r.deadline, "limit");
        // Every open run is compared, whichever matches.
        let mut found = None;
        for (at, run) in inner.runs.iter().enumerate() {
            if same_id(&run.id, offered) {
                found = Some(at);
            }
        }
        let outcome = match found {
            None => Err(ClaimRefused::NoRun),
            Some(at) if inner.runs[at].claimed => Err(ClaimRefused::Taken),
            Some(at) => {
                inner.runs[at].claimed = true;
                Ok(())
            }
        };
        drop(inner);
        if ended {
            self.poke();
        }
        outcome
    }

    /// Whether the run `id` is open at `now`. A run past its limit is ended
    /// here, so its reader writes nothing it was handed before the limit
    /// and had not yet sent.
    pub fn is_open(&self, id: &str, now: Instant) -> bool {
        let mut inner = self.locked();
        let ended = inner.end_runs(|r| same_id(&r.id, id) && now >= r.deadline, "limit");
        let open = inner.runs.iter().any(|r| same_id(&r.id, id));
        drop(inner);
        if ended {
            self.poke();
        }
        open
    }

    /// The next bytes of the run `id`, at most `most`, for its reader. A run
    /// past its limit is ended here rather than read, so no sample leaves
    /// after the limit whenever the next pass comes.
    pub fn read(&self, id: &str, most: usize, now: Instant) -> RunRead {
        let mut inner = self.locked();
        let Some(at) = inner.runs.iter().position(|r| same_id(&r.id, id)) else {
            return RunRead::Ended;
        };
        if now >= inner.runs[at].deadline {
            inner.end_runs(|r| same_id(&r.id, id), "limit");
            drop(inner);
            self.poke();
            return RunRead::Ended;
        }
        let run = &mut inner.runs[at];
        // Whole samples only.
        let count = run.queue.len().min(most) / MIC_BYTES_PER_SAMPLE * MIC_BYTES_PER_SAMPLE;
        if count == 0 {
            return RunRead::Nothing;
        }
        let mut out = Vec::with_capacity(count);
        for byte in run.queue.iter_mut().take(count) {
            out.push(std::mem::take(byte));
        }
        run.queue.drain(..count);
        run.served_bytes += count as u64;
        RunRead::Audio(out)
    }

    /// The reader of run `id` is gone (it closed, or made no progress): the
    /// run ends. `why` is the reason the line gives.
    pub fn reader_gone(&self, id: &str, why: &str) {
        let ended = self.locked().end_runs(|r| same_id(&r.id, id), why);
        if ended {
            self.poke();
        }
    }
}

/// How many bytes at the end of `entry`'s buffer came after its wake word.
fn heard_bytes(entry: &Entry) -> usize {
    entry.since_wake.unwrap_or(0).min(entry.buffer.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chorus_protocol::v2::mic_format;

    fn frame(sequence: u32, byte: u8, samples: usize) -> MicAudio {
        MicAudio {
            format: mic_format::PCM_S16LE_16K_MONO,
            sequence,
            timestamp_ns: 0,
            data: vec![byte; samples * MIC_BYTES_PER_SAMPLE],
        }
    }

    const ON: Option<RoomVoice<'static>> = Some(RoomVoice {
        room: "kitchen",
        enabled: true,
    });
    const DISABLED: Option<RoomVoice<'static>> = Some(RoomVoice {
        room: "kitchen",
        enabled: false,
    });

    #[test]
    fn a_frame_is_kept_only_with_the_role_a_room_voice_on_and_a_live_gate() {
        let voice = Voice::new();
        voice.session_up(1, "mic", true);
        voice.session_up(2, "plain", false);
        // Nothing said yet: muted.
        assert_eq!(
            voice.audio(1, ON, &frame(0, 7, 320)).0,
            Intake::Dropped(DropReason::GateMuted)
        );
        assert_eq!(voice.gate(1, true).map(|g| g.0), Some(true));
        assert_eq!(
            voice.audio(1, DISABLED, &frame(1, 7, 320)).0,
            Intake::Dropped(DropReason::VoiceDisabled)
        );
        assert_eq!(
            voice.audio(1, None, &frame(2, 7, 320)).0,
            Intake::Dropped(DropReason::NoRoom)
        );
        assert!(voice.buffered("mic").is_empty());
        assert_eq!(voice.audio(1, ON, &frame(3, 7, 320)).0, Intake::Buffered);
        assert_eq!(voice.buffered("mic"), vec![7u8; 640]);
        // A session without the role is never listened to, and its word
        // about a gate is not taken.
        assert_eq!(voice.gate(2, true), None);
        assert_eq!(
            voice.audio(2, ON, &frame(0, 9, 320)).0,
            Intake::Dropped(DropReason::NoVoiceRole)
        );
        assert_eq!(
            voice.audio(99, ON, &frame(0, 9, 320)).0,
            Intake::Dropped(DropReason::NoVoiceRole)
        );
        assert!(voice.buffered("plain").is_empty());
        assert_eq!(voice.buffered_frames(), 1);
        assert_eq!(voice.dropped(DropReason::GateMuted), 1);
        assert_eq!(voice.dropped(DropReason::VoiceDisabled), 1);
        assert_eq!(voice.dropped(DropReason::NoRoom), 1);
        assert_eq!(voice.dropped(DropReason::NoVoiceRole), 2);
        assert_eq!(voice.dropped_frames(), 5);
    }

    #[test]
    fn the_buffer_is_bounded_and_keeps_the_newest() {
        let voice = Voice::new();
        voice.session_up(1, "mic", true);
        voice.gate(1, true);
        // Four seconds of 100 ms frames, each filled with its own number.
        for n in 0..40u32 {
            assert_eq!(
                voice.audio(1, ON, &frame(n, n as u8, 1600)).0,
                Intake::Buffered
            );
        }
        let held = voice.buffered("mic");
        assert_eq!(held.len(), BUFFER_BYTES);
        assert_eq!(held.first(), Some(&10), "the oldest second fell off");
        assert_eq!(held.last(), Some(&39));
    }

    #[test]
    fn muting_disabling_and_ending_each_wipe_the_buffer() {
        let voice = Voice::new();
        voice.session_up(1, "mic", true);
        voice.gate(1, true);
        voice.audio(1, ON, &frame(0, 7, 320));
        assert!(!voice.buffered("mic").is_empty());
        voice.gate(1, false);
        assert!(voice.buffered("mic").is_empty(), "muted");

        voice.gate(1, true);
        voice.audio(1, ON, &frame(0, 7, 320));
        voice.audio(1, DISABLED, &frame(1, 7, 320));
        assert!(
            voice.buffered("mic").is_empty(),
            "a frame of a disabled room"
        );

        voice.audio(1, ON, &frame(0, 7, 320));
        voice.tell(
            1,
            VoiceControl {
                uplink: false,
                listening: false,
            },
        );
        assert!(voice.buffered("mic").is_empty(), "told to stop");

        voice.audio(1, ON, &frame(0, 7, 320));
        assert_eq!(voice.take("mic").len(), 640);
        assert!(voice.buffered("mic").is_empty(), "taken");

        voice.audio(1, ON, &frame(0, 7, 320));
        let (still_live, line) = voice.session_down(1);
        assert!(!still_live);
        assert!(line
            .unwrap()
            .contains("ended buffered_frames=5 dropped_frames=1"));
        assert!(voice.buffered("mic").is_empty(), "ended");
        assert_eq!(voice.told(1), None);
    }

    #[test]
    fn a_status_line_is_one_per_change_and_names_counts_not_samples() {
        let voice = Voice::new();
        voice.session_up(1, "mic", true);
        let (_, line) = voice.audio(1, ON, &frame(0, 0x4d, 320));
        assert_eq!(
            line.as_deref(),
            Some(
                "voice mic id=mic room=kitchen intake=dropping reason=gate-muted \
                 buffered_frames=0 dropped_frames=1"
            )
        );
        assert_eq!(voice.audio(1, ON, &frame(1, 0x4d, 320)).1, None);
        let (_, line) = voice.gate(1, true).unwrap();
        assert_eq!(
            line.as_deref(),
            Some("voice mic id=mic gate=live buffered_frames=0 dropped_frames=2")
        );
        let (_, line) = voice.audio(1, ON, &frame(2, 0x4d, 320));
        assert_eq!(
            line.as_deref(),
            Some(
                "voice mic id=mic room=kitchen intake=buffering buffered_frames=1 \
                 dropped_frames=2"
            )
        );
        assert_eq!(voice.audio(1, ON, &frame(3, 0x4d, 320)).1, None);
        assert!(
            !format!("{:?}", voice).contains("77"),
            "no sample is printed"
        );
    }

    #[test]
    fn a_reconnecting_endpoints_gate_is_its_live_sessions() {
        let voice = Voice::new();
        voice.session_up(1, "mic", true);
        voice.session_up(2, "mic", true);
        voice.gate(1, true);
        voice.gate(2, true);
        assert!(voice.session_down(1).0, "the other session is still live");
        assert!(!voice.session_down(2).0);
    }

    /// The wake fixture: "Okay Nabu", 2 s at 16 kHz mono, behind a 44-byte
    /// WAV header (`fixtures/wakeword/README.md`).
    fn wake_fixture() -> Vec<u8> {
        include_bytes!("../../../fixtures/wakeword/okay-nabu.wav")[44..].to_vec()
    }

    fn feed(voice: &Voice, session: u64, pcm: &[u8]) {
        for (n, data) in pcm.chunks(640).enumerate() {
            let frame = MicAudio {
                format: mic_format::PCM_S16LE_16K_MONO,
                sequence: n as u32,
                timestamp_ns: 0,
                data: data.to_vec(),
            };
            assert_eq!(voice.audio(session, ON, &frame).0, Intake::Buffered);
        }
    }

    fn kitchen() -> Vec<String> {
        vec!["mic".to_string()]
    }

    #[test]
    fn the_wake_fixture_is_one_wake_for_its_room_and_two_microphones_are_one_event() {
        let voice = Voice::new();
        voice.session_up(1, "mic", true);
        voice.session_up(2, "mic-2", true);
        voice.gate(1, true);
        voice.gate(2, true);
        let now = Instant::now();
        feed(&voice, 1, &wake_fixture());
        feed(&voice, 2, &wake_fixture());
        let pass = voice.pass(now);
        assert_eq!(
            pass.wakes,
            vec![Wake {
                room: "kitchen".to_string(),
                phrase: "Okay Nabu".to_string()
            }],
            "one event, though both microphones heard it"
        );
        assert!(pass
            .lines
            .iter()
            .any(|l| l == "voice wake room=kitchen id=mic phrase=\"Okay Nabu\""));
        assert_eq!(voice.wakes(), 1);
        // Nothing new: nothing heard.
        assert!(voice.pass(now).wakes.is_empty());
        // Silence is no wake word.
        feed(&voice, 1, &vec![0u8; 64_000]);
        assert!(voice.pass(now + Duration::from_secs(10)).wakes.is_empty());
    }

    #[test]
    fn a_run_serves_its_sessions_frames_to_one_reader_and_ends_when_stopped() {
        let voice = Voice::new();
        voice.session_up(1, "mic", true);
        let now = Instant::now();
        assert_eq!(
            voice.start("kitchen", &kitchen(), "a".repeat(32), now),
            None,
            "no live gate: nothing to listen to"
        );
        voice.gate(1, true);
        // Said before the run, with no wake word: not the run's.
        voice.audio(1, ON, &frame(0, 9, 320));
        assert_eq!(
            voice.start("kitchen", &kitchen(), "a".repeat(32), now),
            Some(RUN_LIMIT)
        );
        assert!(voice.listening("kitchen"));
        assert!(!voice.listening("study"));
        assert_eq!(
            voice.claim(&"b".repeat(32), now),
            Err(ClaimRefused::NoRun),
            "a wrong identifier"
        );
        assert_eq!(voice.claim("", now), Err(ClaimRefused::NoRun));
        assert_eq!(voice.claim(&"a".repeat(32), now), Ok(()));
        assert_eq!(
            voice.claim(&"a".repeat(32), now),
            Err(ClaimRefused::Taken),
            "a second reader"
        );
        assert_eq!(voice.read(&"a".repeat(32), 4096, now), RunRead::Nothing);
        voice.audio(1, ON, &frame(1, 7, 320));
        voice.audio(1, ON, &frame(2, 8, 320));
        assert_eq!(
            voice.read(&"a".repeat(32), 700, now),
            RunRead::Audio([vec![7u8; 640], vec![8u8; 60]].concat()),
            "in order, whole samples, at most what was asked"
        );
        assert_eq!(
            voice.read(&"a".repeat(32), 4096, now),
            RunRead::Audio(vec![8u8; 580])
        );
        assert!(voice.stop("kitchen"));
        assert!(!voice.stop("kitchen"), "no run to stop");
        assert!(!voice.listening("kitchen"));
        assert_eq!(voice.read(&"a".repeat(32), 4096, now), RunRead::Ended);
        assert_eq!(voice.claim(&"a".repeat(32), now), Err(ClaimRefused::NoRun));
        let lines = voice.pass(now).lines;
        assert!(
            lines
                .iter()
                .any(|l| l == "voice run room=kitchen id=mic started limit_ms=30000 after_wake=0"),
            "{lines:?}"
        );
        assert!(
            lines.iter().any(|l| l
                == "voice run room=kitchen id=mic ended reason=stopped read=1 \
                    served_bytes=1280 lost_bytes=0"),
            "{lines:?}"
        );
        assert!(
            !lines.iter().any(|l| l.contains("aaaa")),
            "no line names a run's identifier: {lines:?}"
        );
        assert!(!format!("{:?}", voice).contains("aaaa"));
    }

    #[test]
    fn a_run_ends_at_its_limit_on_mute_on_voice_off_and_with_its_session() {
        let limit = Duration::from_millis(500);
        let open = |voice: &Voice, now: Instant| {
            assert_eq!(
                voice.start("kitchen", &kitchen(), "c".repeat(32), now),
                Some(limit)
            );
            voice.audio(1, ON, &frame(0, 7, 320));
        };
        let reason = |voice: &Voice, now: Instant| -> String {
            let lines = voice.pass(now).lines;
            let line = lines
                .iter()
                .rev()
                .find(|l| l.contains(" ended reason="))
                .unwrap_or_else(|| panic!("no run ended: {lines:?}"));
            line.split("reason=")
                .nth(1)
                .unwrap()
                .split(' ')
                .next()
                .unwrap()
                .to_string()
        };
        let voice = Voice::with_run_limit(limit);
        voice.session_up(1, "mic", true);
        voice.gate(1, true);
        let now = Instant::now();

        // The limit, on the clock the caller hands in: open one instant
        // short of it, ended at it, whether a pass or a read comes first.
        open(&voice, now);
        let nearly = now + limit - Duration::from_nanos(1);
        assert!(voice.pass(nearly).due == Some(now + limit));
        assert!(voice.listening("kitchen"));
        assert!(voice.is_open(&"c".repeat(32), nearly));
        assert_eq!(
            voice.read(&"c".repeat(32), 4096, now + limit),
            RunRead::Ended,
            "nothing is read at the limit"
        );
        assert!(
            !voice.is_open(&"c".repeat(32), nearly),
            "and it stays ended"
        );
        assert_eq!(reason(&voice, now + limit), "limit");
        open(&voice, now);
        assert!(voice.pass(now + limit).due.is_none());
        assert!(!voice.listening("kitchen"));
        open(&voice, now);
        assert_eq!(
            voice.claim(&"c".repeat(32), now + limit),
            Err(ClaimRefused::NoRun),
            "a run past its limit is no run"
        );

        // Mute.
        open(&voice, now);
        voice.gate(1, false);
        assert!(!voice.listening("kitchen"));
        assert_eq!(reason(&voice, now), "muted");
        assert_eq!(voice.read(&"c".repeat(32), 4096, now), RunRead::Ended);
        voice.gate(1, true);

        // Voice switched off: the endpoint is told to stop, or its next
        // frame arrives in a disabled room, whichever is first.
        open(&voice, now);
        voice.tell(1, OFF);
        assert_eq!(reason(&voice, now), "voice-disabled");
        open(&voice, now);
        voice.audio(1, DISABLED, &frame(1, 7, 320));
        assert_eq!(reason(&voice, now), "voice-disabled");

        // A second run in the room replaces the first.
        open(&voice, now);
        assert_eq!(
            voice.start("kitchen", &kitchen(), "d".repeat(32), now),
            Some(limit)
        );
        assert_eq!(reason(&voice, now), "superseded");
        assert_eq!(voice.claim(&"c".repeat(32), now), Err(ClaimRefused::NoRun));
        assert_eq!(voice.claim(&"d".repeat(32), now), Ok(()));

        // Its reader goes, and its session ends.
        voice.reader_gone(&"d".repeat(32), "reader-gone");
        assert_eq!(reason(&voice, now), "reader-gone");
        open(&voice, now);
        voice.session_down(1);
        assert_eq!(reason(&voice, now), "session-ended");
        assert!(!voice.listening("kitchen"));
    }

    #[test]
    fn a_run_opened_after_a_wake_word_starts_at_the_wake_word_and_not_before() {
        let voice = Voice::new();
        voice.session_up(1, "mic", true);
        voice.gate(1, true);
        let now = Instant::now();
        feed(&voice, 1, &wake_fixture());
        assert_eq!(voice.pass(now).wakes.len(), 1);
        // The command, said before the start command arrives.
        voice.audio(1, ON, &frame(0, 0x21, 320));
        voice.audio(1, ON, &frame(1, 0x22, 320));
        voice.start("kitchen", &kitchen(), "e".repeat(32), now);
        voice.claim(&"e".repeat(32), now).unwrap();
        voice.audio(1, ON, &frame(2, 0x23, 320));
        let RunRead::Audio(read) = voice.read(&"e".repeat(32), usize::MAX, now) else {
            panic!("the run has audio")
        };
        let said = [vec![0x21u8; 640], vec![0x22u8; 640], vec![0x23u8; 640]].concat();
        assert!(read.ends_with(&said), "the command is whole");
        let before = &read[..read.len() - said.len()];
        let fixture = wake_fixture();
        assert!(
            fixture.ends_with(before) && before.len() < fixture.len() / 2,
            "and what precedes it is the end of the recording, after the phrase: {} bytes",
            before.len()
        );
        assert!(voice
            .pass(now)
            .lines
            .iter()
            .any(|l| l.ends_with("after_wake=1")));
        // During the run the room reports no wake word.
        feed(&voice, 1, &wake_fixture());
        assert!(voice.pass(now + Duration::from_secs(3)).wakes.is_empty());
        voice.stop("kitchen");
        // And the run's audio is not heard as one afterwards.
        assert!(voice.pass(now + Duration::from_secs(4)).wakes.is_empty());
        // A run opened long after the wake word starts at the command.
        feed(&voice, 1, &wake_fixture());
        let later = now + Duration::from_secs(20);
        assert_eq!(voice.pass(later).wakes.len(), 1);
        voice.start(
            "kitchen",
            &kitchen(),
            "f".repeat(32),
            later + WAKE_GRACE * 2,
        );
        assert_eq!(
            voice.read(&"f".repeat(32), usize::MAX, later + WAKE_GRACE * 2),
            RunRead::Nothing
        );
    }
}
