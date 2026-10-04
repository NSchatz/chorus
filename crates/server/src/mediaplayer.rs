//! The media player engine: what a player thread does (goal 16).
//!
//! `--players N` makes N ports (`crate::playerport`) and N threads
//! (`crate::player`). This module is the driver those threads run: a
//! [`MediaPlayer`] fetches a URL (`chorus-fetch`), decodes it
//! (`chorus-decode`), takes the decode to the port's channel count and sample
//! rate, and writes it into the port, under [`Action`]s sent through a
//! [`PlayerHandle`] and answering with [`PlayerReport`]s. It opens no UPnP
//! socket and knows nothing of rooms: a renderer (the next track) turns
//! AVTransport's effects into actions and the reports back into AVTransport's
//! reports (the table is in
//! `docs/decisions/0124-the-media-player-engine.md`).
//!
//! # One thread, made at start
//!
//! The fetch and the decode both run on the player's own thread, the one
//! `player::spawn` made before the scheduling report. Nothing here creates a
//! thread, per stream or otherwise. The price is that a read from the network
//! holds the thread, so the fetch is opened with
//! `chorus_fetch::open_cancellable`: a wait on a connected socket looks at
//! this player's commands every `chorus_fetch::CANCEL_SLICE`.
//!
//! # How soon a command takes effect
//!
//! - While the thread is decoding or waiting for room in the ring: within
//!   `player::POLL` plus one decode step.
//! - While the thread waits on a connected source that sends nothing (a
//!   stalled server): `Stop`, `Load`, `Unload` and `SkipToNext` give the wait
//!   up within one `CANCEL_SLICE` (100 ms). The other actions (`Pause`,
//!   `Resume`, `Seek`, `QueueNext`, `ClearNext`) wait for the read to end:
//!   for data, or for the fetch policy's read timeout, after which the track
//!   is `Failed`.
//! - While the thread is inside a TCP connect or a name lookup, which
//!   `std::net` cannot leave early: when that returns, at most the policy's
//!   connect timeout for the connect.
//!
//! What a listener hears after a stop is bounded separately: the port is
//! flushed as the `Stop` is handled, and until then at most what the ring
//! held (`playerport::PLAYER_RING_MS`) can play.
//!
//! # Gapless
//!
//! When the decoder of the current track returns its end and a next track is
//! open, the next track's frames are written straight after the last frame of
//! the current one: no flush, no padding, nothing between. Channel remixing
//! runs before the resampler, so the resampler depends on the source rate
//! alone:
//!
//! - **the same rate**: the same resampler instance carries on, so the join
//!   is continuous through the filter (and at the port's own rate the
//!   resampler passes samples through untouched: the join is sample-exact);
//! - **another rate**: the resampler is flushed (it emits the frames it still
//!   owes, as if silence followed, so the first track keeps its exact frame
//!   count) and a new one is made for the next track. The join is still
//!   gap-free and frame-exact, but it is not continuous through the filter:
//!   each side is band-limited against silence, not against its neighbour.
//!
//! [`Event::Boundary`] is sent when the next track's first frame has been
//! taken by the audio thread (`PlayerPort::played_frames` passes the frame's
//! index), not when it was written. The count is looked at every
//! `player::POLL` while the thread waits for room, and no read of the source
//! is started while that frame is within [`EVENT_HOLD_MS`] of going out, so
//! a source that stalls does not hold the report back. (A report can still
//! be late by the length of a read that was already under way.)
//!
//! Control code: it writes into a port from the far side and touches no
//! chunk and no stamp. `audio-path.conf` records it as excluded.

use std::collections::VecDeque;
use std::io::{self, Read, SeekFrom};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use chorus_decode::{DecodeError, Decoder, Format, Hint, Media, Resampler, Tags};
use chorus_fetch::{FetchError, Policy, Stream};

use crate::player::{self, PlayerDriver};
use crate::playerport::PlayerPort;

/// How much audio is put into the ring, with the port held, before a start,
/// a seek or a skip lets it out, ms. ASSUMED: 250 ms, a quarter of the ring:
/// enough that the first chunks out are whole and a live source that delivers
/// in bursts has something behind it, short enough that a start is not felt
/// as a wait. The end of the media releases the hold sooner.
pub const START_FILL_MS: u64 = 250;

/// How close to audible the frame a report waits for must be for the player
/// to look at the port's count instead of reading more of the source, ms.
/// ASSUMED: 50 ms, a twentieth of the ring: the ring is not drained by more
/// than that for a report's sake.
pub const EVENT_HOLD_MS: u64 = 50;

/// How long a player with nothing to play waits for a command before it looks
/// at whether the run still goes on. ASSUMED: 100 ms.
pub const IDLE_WAIT: Duration = Duration::from_millis(100);

/// What a caller asks of a player. Every action is sent with the caller's
/// epoch (`PlayerHandle::send`), which the reports that follow carry back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Drop whatever is loaded or queued, open and probe this URI now, and
    /// stay silent: [`Event::Opened`] or [`Event::Failed`]. A URI equal to
    /// the queued next's takes over the next's open source without fetching
    /// it again.
    Load {
        /// An `http` or `https` URL.
        uri: String,
        /// The media type the caller was told (a hint, never a decision).
        mime: Option<String>,
    },
    /// (goal 18) [`Action::Load`] for a fetch held to origins: every
    /// connection it makes (the URI, each redirect, a playlist and its
    /// segments) has to be to one of `origins`, or the load fails with the
    /// fetch policy's `refused: origin ...`. What an announcement is loaded
    /// with.
    LoadWithin {
        /// An `http` or `https` URL.
        uri: String,
        /// The media type the caller was told (a hint, never a decision).
        mime: Option<String>,
        /// The origins the fetch may connect to; never empty.
        origins: Vec<chorus_fetch::Origin>,
    },
    /// Play the loaded URI from the start, or from where a `Seek` put it,
    /// opening it again when a `Stop` or its end closed it.
    /// [`Event::Started`] when its first frame is out.
    Start,
    /// Hold the audio where it is.
    Pause,
    /// Carry on from where the audio was held.
    Resume,
    /// Flush the port and return to the start. The URI stays loaded, and so
    /// does a queued next.
    Stop,
    /// Move to `ms` in the loaded media: [`Event::SeekDone`], then
    /// [`Event::Started`] again when it was playing; a source that cannot
    /// seek is [`Event::SeekRefused`] and nothing changes.
    Seek {
        /// The target, in milliseconds from the start.
        ms: u64,
    },
    /// Open and probe this URI ahead, so it follows the current one without
    /// a gap; it replaces a next queued before. [`Event::NextOpened`] or
    /// [`Event::NextFailed`].
    QueueNext {
        /// An `http` or `https` URL.
        uri: String,
        /// The media type the caller was told.
        mime: Option<String>,
    },
    /// Forget the queued next.
    ClearNext,
    /// Flush and start the queued next now: [`Event::Boundary`] when its
    /// first frame is out.
    SkipToNext,
    /// Stop and forget everything: the player is idle again.
    Unload,
}

impl Action {
    /// Whether the action gives up a wait on the network (see the module's
    /// "How soon a command takes effect").
    fn interrupts(&self) -> bool {
        matches!(
            self,
            Action::Stop
                | Action::Load { .. }
                | Action::LoadWithin { .. }
                | Action::Unload
                | Action::SkipToNext
        )
    }
}

/// What opening a URI found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaInfo {
    /// The URI as it was given.
    pub uri: String,
    /// How long the media is, when its container says so up front; a live
    /// stream has none.
    pub duration_ms: Option<u64>,
    /// Whether [`Action::Seek`] works: the server honours range requests and
    /// declared a length.
    pub seekable: bool,
    /// The decoder's output shape (the source's rate and channels).
    pub format: Format,
    /// The tags read while opening.
    pub tags: Tags,
    /// The station's name, when the response had ICY headers.
    pub station: Option<String>,
}

/// What a player says happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The loaded URI is open (after a `Load`, and again when a `Start` or a
    /// `Seek` had to open it anew).
    Opened(MediaInfo),
    /// The first frame after a `Start` or a `Seek` has been taken by the
    /// audio thread.
    Started,
    /// The first frame of the next track has been taken by the audio thread:
    /// it is the current track now and the next slot is empty.
    Boundary(MediaInfo),
    /// The last frame was taken and nothing was queued behind it. The player
    /// is stopped with the URI still loaded.
    Ended {
        /// The position the track ended at.
        played_ms: u64,
    },
    /// The loaded URI cannot be fetched or decoded, in the fetcher's or the
    /// decoder's own words: `unsupported: aac`, `refused: loopback address
    /// 127.0.0.1`, `http status 404`. The player is stopped with the URI
    /// still loaded.
    Failed {
        /// Why.
        reason: String,
    },
    /// The queued next URI is open and will follow.
    NextOpened(MediaInfo),
    /// The queued next URI cannot be played; the current track ends with
    /// [`Event::Ended`].
    NextFailed {
        /// Why.
        reason: String,
    },
    /// The stream names a new title: an ICY `StreamTitle`, or the tags of a
    /// new link of a chained Ogg stream.
    Title(String),
    /// A seek landed, here.
    SeekDone {
        /// The position the next frame out has.
        ms: u64,
    },
    /// A seek was not done.
    SeekRefused {
        /// Why: `unsupported: seek on a non-seekable source`.
        reason: String,
    },
}

/// One report of one player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerReport {
    /// Which player (`player::player_id`).
    pub player: usize,
    /// The epoch of the last action the player had taken when this happened.
    pub epoch: u64,
    /// What happened.
    pub event: Event,
}

/// What a player is doing, as [`PlayerHandle::state`] reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerState {
    /// Nothing is loaded: the player is idle and free to be given away.
    Empty,
    /// A URI is loaded and nothing plays.
    Stopped,
    /// Started, and no frame is out yet (opening, filling, seeking).
    Buffering,
    /// Audio is going out.
    Playing,
    /// Held inside the media.
    Paused,
}

impl PlayerState {
    fn from_u8(v: u8) -> PlayerState {
        match v {
            1 => PlayerState::Stopped,
            2 => PlayerState::Buffering,
            3 => PlayerState::Playing,
            4 => PlayerState::Paused,
            _ => PlayerState::Empty,
        }
    }
}

const NO_DURATION: u64 = u64::MAX;

/// What a handle and its player share: counters and the last published
/// position. Nothing here is locked.
#[derive(Debug, Default)]
struct Shared {
    /// Actions sent and not yet taken.
    waiting: AtomicU64,
    /// Interrupting actions sent.
    aborts_sent: AtomicU64,
    /// Interrupting actions taken.
    aborts_taken: AtomicU64,
    state: AtomicU8,
    position_ms: AtomicU64,
    duration_ms: AtomicU64,
    /// The run is over: nothing is worth waiting for.
    stopping: AtomicBool,
}

impl Shared {
    fn abort_pending(&self) -> bool {
        self.aborts_sent.load(Ordering::SeqCst) != self.aborts_taken.load(Ordering::SeqCst)
            || self.stopping.load(Ordering::SeqCst)
    }
}

/// The caller's end of one player: send actions, read where it is. Clone it
/// freely; every clone speaks to the same player.
#[derive(Debug, Clone)]
pub struct PlayerHandle {
    index: usize,
    actions: Sender<(u64, Action)>,
    shared: Arc<Shared>,
}

impl PlayerHandle {
    /// Which player this is.
    pub fn index(&self) -> usize {
        self.index
    }

    /// Send `action` tagged with the caller's `epoch`. Returns `false` when
    /// the player's thread is gone (the run is over).
    pub fn send(&self, epoch: u64, action: Action) -> bool {
        let interrupts = action.interrupts();
        self.shared.waiting.fetch_add(1, Ordering::SeqCst);
        if interrupts {
            self.shared.aborts_sent.fetch_add(1, Ordering::SeqCst);
        }
        if self.actions.send((epoch, action)).is_ok() {
            return true;
        }
        self.shared.waiting.fetch_sub(1, Ordering::SeqCst);
        if interrupts {
            self.shared.aborts_taken.fetch_add(1, Ordering::SeqCst);
        }
        false
    }

    /// What the player is doing.
    pub fn state(&self) -> PlayerState {
        PlayerState::from_u8(self.shared.state.load(Ordering::SeqCst))
    }

    /// The position in the current track, ms: the frames the audio thread has
    /// taken since the track's first frame, plus where a seek started it.
    /// Frozen while paused, 0 when stopped. Published by the player's thread
    /// each time round its loop, so it lags by at most one `player::POLL`
    /// while the thread is not waiting on the network.
    pub fn position_ms(&self) -> u64 {
        self.shared.position_ms.load(Ordering::SeqCst)
    }

    /// The current track's duration, when it is known.
    pub fn duration_ms(&self) -> Option<u64> {
        let d = self.shared.duration_ms.load(Ordering::SeqCst);
        (d != NO_DURATION).then_some(d)
    }

    /// Whether nothing is loaded and nothing is on its way.
    pub fn is_idle(&self) -> bool {
        self.state() == PlayerState::Empty && self.shared.waiting.load(Ordering::SeqCst) == 0
    }
}

/// The fetch policy of a server whose listeners are on `listener_ports`:
/// loopback is refused unless `allow_loopback` (tests, development), every
/// port in `listener_ports` is refused on this machine's own addresses, and
/// the timeouts, the redirect bound and the CA bundle are the fetcher's
/// defaults (brief section 4.8, `docs/streams.md`). A port of 0 names
/// nothing and is left out. The embedding code builds it once, before the
/// player threads start; a later listener (the renderer's HTTP port) is one
/// more entry in `listener_ports`.
pub fn fetch_policy(listener_ports: &[u16], allow_loopback: bool) -> Policy {
    Policy {
        allow_loopback,
        denied_ports_on_self: listener_ports
            .iter()
            .copied()
            .filter(|port| *port != 0)
            .collect(),
        ..Policy::default()
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// The server's players: their handles, the one stream of their reports, and
/// who holds which. Plain data behind a mutex; it runs no thread.
#[derive(Debug)]
pub struct Players {
    handles: Vec<PlayerHandle>,
    owners: Mutex<Vec<Option<String>>>,
    reports: Mutex<Option<Receiver<PlayerReport>>>,
}

impl Players {
    /// `count` players fetching under `policy`, and the drivers to hand
    /// `player::spawn` (one per port, in order). Made before the threads
    /// start, so the thread population is what `--players` says and nothing
    /// else.
    pub fn new(count: usize, policy: Policy) -> (Players, Vec<Box<dyn PlayerDriver>>) {
        let policy = Arc::new(policy);
        let (report_tx, report_rx) = mpsc::channel();
        let mut handles = Vec::with_capacity(count);
        let mut drivers: Vec<Box<dyn PlayerDriver>> = Vec::with_capacity(count);
        for index in 0..count {
            let (actions, inbox) = mpsc::channel();
            let shared = Arc::new(Shared::default());
            shared.duration_ms.store(NO_DURATION, Ordering::SeqCst);
            handles.push(PlayerHandle {
                index,
                actions,
                shared: Arc::clone(&shared),
            });
            drivers.push(Box::new(MediaPlayer {
                index,
                inbox,
                reports: report_tx.clone(),
                shared,
                policy: Arc::clone(&policy),
            }));
        }
        (
            Players {
                handles,
                owners: Mutex::new(vec![None; count]),
                reports: Mutex::new(Some(report_rx)),
            },
            drivers,
        )
    }

    /// How many players there are.
    pub fn len(&self) -> usize {
        self.handles.len()
    }

    /// Whether there is no player.
    pub fn is_empty(&self) -> bool {
        self.handles.is_empty()
    }

    /// Player `index`'s handle.
    pub fn handle(&self, index: usize) -> Option<&PlayerHandle> {
        self.handles.get(index)
    }

    /// The reports of every player, in the order they were made. There is
    /// one receiver; the first caller gets it.
    pub fn take_reports(&self) -> Option<Receiver<PlayerReport>> {
        lock(&self.reports).take()
    }

    /// A player for `owner`: the one it already holds, else the lowest idle
    /// one nobody holds (idle: nothing loaded). `None` when every player is
    /// held or busy.
    pub fn acquire(&self, owner: &str) -> Option<usize> {
        let mut owners = lock(&self.owners);
        if let Some(held) = owners.iter().position(|o| o.as_deref() == Some(owner)) {
            return Some(held);
        }
        let free =
            (0..owners.len()).find(|i| owners[*i].is_none() && self.handles[*i].is_idle())?;
        owners[free] = Some(owner.to_string());
        Some(free)
    }

    /// Give player `index` back. Its holder unloads it first; a player that
    /// still has something loaded is not handed out again until it is idle.
    pub fn release(&self, index: usize) {
        if let Some(slot) = lock(&self.owners).get_mut(index) {
            *slot = None;
        }
    }

    /// Who holds player `index`.
    pub fn owner_of(&self, index: usize) -> Option<String> {
        lock(&self.owners).get(index).cloned().flatten()
    }

    /// The player `owner` holds.
    pub fn held_by(&self, owner: &str) -> Option<usize> {
        lock(&self.owners)
            .iter()
            .position(|o| o.as_deref() == Some(owner))
    }
}

/// The driver of one player thread. Made by [`Players::new`].
pub struct MediaPlayer {
    index: usize,
    inbox: Receiver<(u64, Action)>,
    reports: Sender<PlayerReport>,
    shared: Arc<Shared>,
    policy: Arc<Policy>,
}

impl PlayerDriver for MediaPlayer {
    fn run(&mut self, port: Arc<PlayerPort>, keep: &AtomicBool) {
        let mut engine = Engine::new(self, port, keep);
        while keep.load(Ordering::SeqCst) {
            let mut taken = false;
            while let Ok((epoch, action)) = engine.me.inbox.try_recv() {
                engine.handle(epoch, action);
                taken = true;
            }
            if taken {
                // Before any wait for room: what the handle reads follows an
                // action at once.
                engine.publish();
            }
            let busy = engine.playing && engine.step();
            engine.publish();
            if busy {
                continue;
            }
            let wait = if engine.playing {
                player::POLL
            } else {
                IDLE_WAIT
            };
            match engine.me.inbox.recv_timeout(wait) {
                Ok((epoch, action)) => {
                    engine.handle(epoch, action);
                    engine.publish();
                }
                Err(RecvTimeoutError::Timeout) => {}
                // Every handle is gone: nothing more will be asked. What
                // plays, plays on until the run stops.
                Err(RecvTimeoutError::Disconnected) => thread::sleep(wait),
            }
        }
        self.shared.stopping.store(true, Ordering::SeqCst);
    }
}

/// A URI and what the caller said it is.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Request {
    uri: String,
    mime: Option<String>,
    /// (goal 18) The origins the fetch is held to; empty for no such rule.
    origins: Vec<chorus_fetch::Origin>,
}

/// An open source: the decoder, and where the fetch leaves the latest ICY
/// title for the engine to find.
struct Source {
    decoder: Decoder,
    icy_title: Option<Arc<Mutex<Option<String>>>>,
}

/// A loaded or queued URI, open or not.
struct Track {
    req: Request,
    src: Option<Source>,
    info: Option<MediaInfo>,
    /// Opening it failed and was reported; it is not tried again.
    failed: bool,
}

impl Track {
    fn new(req: Request) -> Track {
        Track {
            req,
            src: None,
            info: None,
            failed: false,
        }
    }
}

/// A join that was written and is not audible yet.
struct Join {
    /// The index, in frames written since the last flush, of the next
    /// track's first frame.
    mark: u64,
    req: Request,
    info: MediaInfo,
}

/// The track a listener hears now. While a join is written and not yet
/// audible this is the track before it, and `Engine::current` the one after.
#[derive(Clone)]
struct Heard {
    req: Request,
    info: Option<MediaInfo>,
}

enum Drain {
    Ended,
    Failed(String),
}

/// A fetch stream as the decoder's media.
struct NetMedia {
    stream: Stream,
    icy_title: Option<Arc<Mutex<Option<String>>>>,
}

impl Read for NetMedia {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.stream.read(buf)?;
        if let Some(slot) = &self.icy_title {
            if let Some(title) = self.stream.stream_title() {
                let mut held = lock(slot);
                if held.as_deref() != Some(title.as_str()) {
                    *held = Some(title);
                }
            }
        }
        Ok(n)
    }
}

impl Media for NetMedia {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.stream.seek(pos)
    }
    fn is_seekable(&self) -> bool {
        self.stream.opened().seekable
    }
    fn byte_len(&self) -> Option<u64> {
        self.stream.opened().byte_len
    }
}

/// A decode error in the words the caller is shown: a fetch error that rode
/// through the decoder inside an `io::Error` comes out as the fetch error it
/// is (`refused: ...`, `unsupported: hls: ...`, `io: ...`).
fn reason(e: DecodeError) -> String {
    match e {
        DecodeError::Io(io) => FetchError::from(io).to_string(),
        other => other.to_string(),
    }
}

/// The extension of the last path segment of `url`, lower case.
fn extension_of(url: &str) -> Option<String> {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let name = path.rsplit('/').next()?;
    let (stem, ext) = name.rsplit_once('.')?;
    (!stem.is_empty()
        && !ext.is_empty()
        && ext.len() <= 5
        && ext.bytes().all(|b| b.is_ascii_alphanumeric()))
    .then(|| ext.to_ascii_lowercase())
}

/// The rate and channel count of an `audio/L16` media type (RFC 3551 section
/// 4.5.11 and RFC 2586, ASSUMED from memory: `rate` is required, `channels`
/// defaults to 1).
fn l16_of(mime: &str) -> Option<(u32, u16)> {
    let mut parts = mime.split(';');
    if !parts.next()?.trim().eq_ignore_ascii_case("audio/l16") {
        return None;
    }
    let mut rate = None;
    let mut channels = 1u16;
    for part in parts {
        let (key, value) = part.split_once('=')?;
        let value = value.trim().trim_matches('"');
        match key.trim().to_ascii_lowercase().as_str() {
            "rate" => rate = value.parse().ok(),
            "channels" => channels = value.parse().ok()?,
            _ => {}
        }
    }
    Some((rate?, channels))
}

/// A media type that says nothing about what the bytes are.
fn is_generic(mime: &str) -> bool {
    let kind = mime.split(';').next().unwrap_or(mime).trim();
    kind.is_empty()
        || kind.eq_ignore_ascii_case("application/octet-stream")
        || kind.eq_ignore_ascii_case("binary/octet-stream")
}

struct Engine<'a> {
    me: &'a MediaPlayer,
    port: Arc<PlayerPort>,
    keep: &'a AtomicBool,
    rate: u32,
    channels: u16,
    /// The epoch of the last action taken.
    epoch: u64,
    /// The track being decoded.
    current: Option<Track>,
    next: Option<Track>,
    heard: Option<Heard>,
    joins: VecDeque<Join>,
    playing: bool,
    paused: bool,
    /// The port is held while the ring fills.
    filling: bool,
    /// `Started` is owed once a frame is out.
    started_owed: bool,
    drain: Option<Drain>,
    finished: bool,
    /// The resampler and the source rate it was made for.
    resampler: Option<(u32, Resampler)>,
    /// Frames produced when the resampler was made, and source frames it has
    /// taken since.
    resampler_origin: u64,
    resampler_in: u64,
    /// Output frames produced since the last flush of the port: written, or
    /// waiting in `pending`.
    produced: u64,
    pending: Vec<f32>,
    pending_at: usize,
    decoded: Vec<f32>,
    remixed: Vec<f32>,
    /// The index of the audible track's first frame, and where in the track
    /// that frame is.
    base_mark: u64,
    offset_ms: u64,
    duration_ms: Option<u64>,
    title: Option<String>,
}

impl<'a> Engine<'a> {
    fn new(me: &'a MediaPlayer, port: Arc<PlayerPort>, keep: &'a AtomicBool) -> Engine<'a> {
        let rate = port.rate_hz();
        let channels = u16::try_from(port.channels()).unwrap_or(u16::MAX);
        Engine {
            me,
            port,
            keep,
            rate,
            channels,
            epoch: 0,
            current: None,
            next: None,
            heard: None,
            joins: VecDeque::new(),
            playing: false,
            paused: false,
            filling: false,
            started_owed: false,
            drain: None,
            finished: false,
            resampler: None,
            resampler_origin: 0,
            resampler_in: 0,
            produced: 0,
            pending: Vec::new(),
            pending_at: 0,
            decoded: Vec::new(),
            remixed: Vec::new(),
            base_mark: 0,
            offset_ms: 0,
            duration_ms: None,
            title: None,
        }
    }

    fn report(&self, event: Event) {
        // What the handle reads is never behind what a report says.
        self.publish();
        let _ = self.me.reports.send(PlayerReport {
            player: self.me.index,
            epoch: self.epoch,
            event,
        });
    }

    fn aborted(&self) -> bool {
        self.me.shared.abort_pending()
    }

    // ----- opening --------------------------------------------------------

    fn open(&self, req: &Request) -> Result<(Source, MediaInfo), String> {
        let shared = Arc::clone(&self.me.shared);
        let cancel: chorus_fetch::Cancel = Arc::new(move || shared.abort_pending());
        let stream = if req.origins.is_empty() {
            chorus_fetch::open_cancellable(&req.uri, &self.me.policy, cancel)
        } else {
            // (goal 18) The players' policy, and this fetch's origins on top.
            let held = Policy {
                origins: req.origins.clone(),
                ..(*self.me.policy).clone()
            };
            chorus_fetch::open_cancellable(&req.uri, &held, cancel)
        }
        .map_err(|e| e.to_string())?;
        let opened = stream.opened().clone();
        // The response's own type, unless it says nothing; then the caller's.
        let mime = opened
            .content_type
            .clone()
            .filter(|t| !is_generic(t))
            .or_else(|| req.mime.clone())
            .or_else(|| opened.content_type.clone());
        let icy_title = opened
            .icy
            .as_ref()
            .and_then(|icy| icy.metaint)
            .map(|_| Arc::new(Mutex::new(None)));
        let media = Box::new(NetMedia {
            stream,
            icy_title: icy_title.clone(),
        });
        let decoder = match mime.as_deref().and_then(l16_of) {
            Some((rate, channels)) => Decoder::open_l16(media, rate, channels),
            None => Decoder::open(
                media,
                &Hint {
                    mime,
                    extension: extension_of(&opened.final_url),
                },
            ),
        }
        .map_err(reason)?;
        let format = decoder.format().clone();
        check_shape(&format)?;
        let info = MediaInfo {
            uri: req.uri.clone(),
            // A body with no declared length is a live stream: whatever a
            // header inside it claims, how long it goes on is not known.
            duration_ms: format
                .frames
                .filter(|_| opened.byte_len.is_some())
                .map(|frames| frames * 1_000 / u64::from(format.rate)),
            seekable: opened.seekable,
            format,
            tags: decoder.tags().clone(),
            station: opened.icy.and_then(|icy| icy.name),
        };
        Ok((Source { decoder, icy_title }, info))
    }

    /// Opens the current track. On failure reports `Failed`, unless the open
    /// was given up for an action that is waiting.
    fn open_current(&mut self) -> bool {
        let Some(req) = self.current.as_ref().map(|t| t.req.clone()) else {
            return false;
        };
        match self.open(&req) {
            Ok((src, info)) => {
                self.title = info.tags.title.clone();
                self.duration_ms = info.duration_ms;
                self.heard = Some(Heard {
                    req,
                    info: Some(info.clone()),
                });
                if let Some(track) = self.current.as_mut() {
                    track.src = Some(src);
                    track.info = Some(info.clone());
                }
                self.report(Event::Opened(info));
                true
            }
            Err(reason) => {
                if !self.aborted() {
                    self.report(Event::Failed { reason });
                }
                false
            }
        }
    }

    /// Opens the queued next track, when it is not open and has not failed.
    /// Returns whether it is open now.
    fn open_next(&mut self) -> bool {
        let Some(req) = self.next.as_ref().map(|t| t.req.clone()) else {
            return false;
        };
        match self.next.as_ref() {
            Some(t) if t.failed => return false,
            Some(t) if t.src.is_some() => return true,
            _ => {}
        }
        let opened = self.open(&req);
        let aborted = self.aborted();
        let Some(track) = self.next.as_mut() else {
            return false;
        };
        match opened {
            Ok((src, info)) => {
                track.src = Some(src);
                track.info = Some(info.clone());
                self.report(Event::NextOpened(info));
                true
            }
            Err(reason) => {
                // Given up for a waiting action: it is tried again when it
                // is next needed. Otherwise it failed, once.
                if !aborted {
                    track.failed = true;
                    self.report(Event::NextFailed { reason });
                }
                false
            }
        }
    }

    // ----- actions --------------------------------------------------------

    fn handle(&mut self, epoch: u64, action: Action) {
        let shared = &self.me.shared;
        shared.waiting.fetch_sub(1, Ordering::SeqCst);
        if action.interrupts() {
            shared.aborts_taken.fetch_add(1, Ordering::SeqCst);
        }
        self.epoch = epoch;
        match action {
            Action::Load { uri, mime } => self.load(Request {
                uri,
                mime,
                origins: Vec::new(),
            }),
            Action::LoadWithin { uri, mime, origins } => self.load(Request { uri, mime, origins }),
            Action::Start => {
                if !self.playing && self.current.is_some() {
                    self.begin();
                }
            }
            Action::Pause => {
                if self.playing {
                    self.paused = true;
                    self.port.set_paused(true);
                }
            }
            Action::Resume => {
                if self.playing {
                    self.paused = false;
                    if !self.filling {
                        self.port.set_paused(false);
                    }
                }
            }
            Action::Stop => {
                self.revert();
                self.halt();
            }
            Action::Seek { ms } => self.seek(ms),
            Action::QueueNext { uri, mime } => {
                self.next = Some(Track::new(Request {
                    uri,
                    mime,
                    origins: Vec::new(),
                }));
                self.open_next();
            }
            Action::ClearNext => self.next = None,
            Action::SkipToNext => self.skip(),
            Action::Unload => {
                self.halt();
                self.joins.clear();
                self.current = None;
                self.next = None;
                self.heard = None;
                self.duration_ms = None;
                self.title = None;
            }
        }
    }

    /// Stops the audio and forgets what was being converted. The loaded URI
    /// stays; its source is closed, so a start fetches it again.
    fn halt(&mut self) {
        self.port.flush();
        self.port.set_paused(false);
        self.playing = false;
        self.paused = false;
        self.filling = false;
        self.started_owed = false;
        self.drain = None;
        self.offset_ms = 0;
        self.reset_conversion();
        if let Some(track) = self.current.as_mut() {
            track.src = None;
        }
    }

    /// The state of a port just flushed: nothing produced, no resampler.
    fn reset_conversion(&mut self) {
        self.resampler = None;
        self.resampler_origin = 0;
        self.resampler_in = 0;
        self.produced = 0;
        self.pending.clear();
        self.pending_at = 0;
        self.base_mark = 0;
        self.finished = false;
    }

    /// When a join is written and not audible yet, what is about to be
    /// flushed belongs to two tracks. The caller's view is the audible one:
    /// make it the current track again (closed, to be fetched anew) and put
    /// the joined track back as the next, unless a later next was queued.
    fn revert(&mut self) {
        let Some(first) = self.joins.pop_front() else {
            return;
        };
        self.joins.clear();
        if self.next.is_none() {
            let mut track = Track::new(first.req);
            track.info = Some(first.info);
            self.next = Some(track);
        }
        if let Some(heard) = self.heard.clone() {
            self.duration_ms = heard.info.as_ref().and_then(|i| i.duration_ms);
            self.title = heard.info.as_ref().and_then(|i| i.tags.title.clone());
            let mut track = Track::new(heard.req);
            track.info = heard.info;
            self.current = Some(track);
        }
    }

    fn load(&mut self, req: Request) {
        // AVTransport's `ended` hands a queued next back as Load + Start when
        // it was not joined: the open source is taken over, not fetched again.
        let promoted = match self.next.take() {
            Some(track) if track.req.uri == req.uri && track.src.is_some() => Some(track),
            _ => None,
        };
        self.halt();
        self.joins.clear();
        self.next = None;
        self.duration_ms = None;
        self.title = None;
        match promoted {
            Some(mut track) => {
                track.req = req.clone();
                let info = track.info.clone();
                self.current = Some(track);
                self.heard = Some(Heard {
                    req,
                    info: info.clone(),
                });
                if let Some(info) = info {
                    self.title = info.tags.title.clone();
                    self.duration_ms = info.duration_ms;
                    self.report(Event::Opened(info));
                }
            }
            None => {
                self.heard = Some(Heard {
                    req: req.clone(),
                    info: None,
                });
                self.current = Some(Track::new(req));
                self.open_current();
            }
        }
    }

    /// Starts the current track: from the start, or from where a seek left
    /// its decoder.
    fn begin(&mut self) {
        let open = self.current.as_ref().is_some_and(|t| t.src.is_some());
        if !open && !self.open_current() {
            return;
        }
        self.port.flush();
        self.reset_conversion();
        self.port.set_paused(true);
        self.filling = true;
        self.playing = true;
        self.paused = false;
        self.started_owed = true;
        self.drain = None;
    }

    fn seek(&mut self, ms: u64) {
        let refuse = |engine: &Engine, reason: &str| {
            engine.report(Event::SeekRefused {
                reason: reason.to_string(),
            })
        };
        if self.current.is_none() {
            return refuse(self, "nothing is loaded");
        }
        // The audible track decides whether a seek can be done.
        if let Some(Heard {
            info: Some(info), ..
        }) = &self.heard
        {
            if !info.seekable {
                return refuse(self, "unsupported: seek on a non-seekable source");
            }
        }
        self.revert();
        let open = self.current.as_ref().is_some_and(|t| t.src.is_some());
        if !open && !self.open_current() {
            if self.playing {
                self.halt();
            }
            return;
        }
        let Some(track) = self.current.as_mut() else {
            return;
        };
        if !track.info.as_ref().is_some_and(|i| i.seekable) {
            return refuse(self, "unsupported: seek on a non-seekable source");
        }
        let Some(src) = track.src.as_mut() else {
            return;
        };
        let rate = u64::from(src.decoder.format().rate);
        match src.decoder.seek(ms.saturating_mul(rate) / 1_000) {
            Ok(frame) => {
                self.port.flush();
                self.reset_conversion();
                self.drain = None;
                self.offset_ms = frame * 1_000 / rate;
                if self.playing {
                    self.port.set_paused(true);
                    self.filling = true;
                    self.started_owed = true;
                }
                self.report(Event::SeekDone { ms: self.offset_ms });
            }
            Err(e) => {
                let why = reason(e);
                if !self.aborted() {
                    refuse(self, &why);
                }
            }
        }
    }

    fn skip(&mut self) {
        self.revert();
        if !self.open_next() {
            return;
        }
        let Some(track) = self.next.take() else {
            return;
        };
        let Some(info) = track.info.clone() else {
            return;
        };
        let paused = self.playing && self.paused;
        self.halt();
        self.title = info.tags.title.clone();
        self.joins.push_back(Join {
            mark: 0,
            req: track.req.clone(),
            info,
        });
        self.current = Some(track);
        self.port.set_paused(true);
        self.filling = true;
        self.playing = true;
        self.paused = paused;
    }

    // ----- playing --------------------------------------------------------

    /// One piece of work while playing. Returns `false` when there was
    /// nothing to do but wait.
    fn step(&mut self) -> bool {
        self.events();
        if self
            .next
            .as_ref()
            .is_some_and(|t| t.src.is_none() && !t.failed)
        {
            self.open_next();
            return true;
        }
        if self.pending_at < self.pending.len() {
            return self.write();
        }
        self.pending.clear();
        self.pending_at = 0;
        if self.event_is_imminent() {
            return false;
        }
        if matches!(self.drain, Some(Drain::Ended))
            && self.next.as_ref().is_some_and(|t| t.src.is_some())
        {
            // A next arrived while the last frames were going out: it joins
            // them, late or not.
            self.join();
            return true;
        }
        if self.drain.is_some() {
            if !self.finished {
                self.port.finish();
                self.finished = true;
                self.release_fill();
            }
            if self.port.played_frames() >= self.produced {
                self.end();
                return true;
            }
            return false;
        }
        self.decode();
        true
    }

    /// The index the audio thread's count must pass for the next report.
    fn due(&self) -> Option<u64> {
        if self.started_owed {
            return Some(0);
        }
        self.joins.front().map(|j| j.mark)
    }

    /// Whether the frame the next report waits for is in the ring and will
    /// be taken within [`EVENT_HOLD_MS`]. No read of the source is started
    /// then: a read can block for as long as the source likes, and `Started`
    /// and `Boundary` are owed when the frame goes out, not when the read
    /// comes back.
    fn event_is_imminent(&self) -> bool {
        let Some(mark) = self.due() else {
            return false;
        };
        if self.filling || self.paused {
            return false;
        }
        let played = self.port.played_frames();
        let ahead = mark.saturating_sub(played);
        ahead < u64::from(self.rate) * EVENT_HOLD_MS / 1_000 && (self.port.queued() as u64) > ahead
    }

    /// Reports what became audible.
    fn events(&mut self) {
        let played = self.port.played_frames();
        if self.started_owed && played > 0 {
            self.started_owed = false;
            self.report(Event::Started);
        }
        while self.joins.front().is_some_and(|j| played > j.mark) {
            let Some(join) = self.joins.pop_front() else {
                break;
            };
            self.base_mark = join.mark;
            self.offset_ms = 0;
            self.duration_ms = join.info.duration_ms;
            self.heard = Some(Heard {
                req: join.req,
                info: Some(join.info.clone()),
            });
            self.report(Event::Boundary(join.info));
        }
    }

    fn release_fill(&mut self) {
        if self.filling {
            self.filling = false;
            self.port.set_paused(self.paused);
        }
    }

    fn write(&mut self) -> bool {
        let channels = usize::from(self.channels);
        let rest = &self.pending[self.pending_at..];
        if self.filling {
            // The port is held: take what fits and never wait for room that
            // only a release can make.
            let n = self.port.write(rest);
            self.pending_at += n * channels;
            let target = (u64::from(self.rate) * START_FILL_MS / 1_000) as usize;
            if n == 0 || self.port.queued() >= target.min(self.port.capacity()) {
                self.release_fill();
            }
            return true;
        }
        let shared = &self.me.shared;
        let port = &self.port;
        let due = self.due();
        let n = player::write_all(port, rest, self.keep, &|| {
            // Asked every `player::POLL` while there is no room: the
            // position the handle reads keeps moving meanwhile.
            let played = port.played_frames();
            shared
                .position_ms
                .store(self.position_at(played), Ordering::SeqCst);
            shared.waiting.load(Ordering::SeqCst) > 0 || due.is_some_and(|mark| played > mark)
        });
        self.pending_at += n * channels;
        true
    }

    fn decode(&mut self) {
        let Some(src) = self.current.as_mut().and_then(|t| t.src.as_mut()) else {
            // A wait on its source was given up for an action that then did
            // not replace it.
            self.fail("the source was closed".to_string());
            return;
        };
        self.decoded.clear();
        match src.decoder.read(&mut self.decoded) {
            Ok(0) => self.end_of_track(),
            Ok(_) => {
                let (rate, channels) = (src.decoder.format().rate, src.decoder.format().channels);
                let title = match &src.icy_title {
                    Some(slot) => lock(slot).clone(),
                    None => src.decoder.tags().title.clone(),
                };
                if title.is_some() && title != self.title {
                    self.title = title.clone();
                    if let Some(title) = title {
                        self.report(Event::Title(title));
                    }
                }
                self.convert(rate, channels);
            }
            Err(e) => {
                let why = reason(e);
                if let Some(track) = self.current.as_mut() {
                    track.src = None;
                }
                if !self.aborted() {
                    self.fail(why);
                }
            }
        }
    }

    /// Takes `self.decoded` (the source's rate and channels) to the port's
    /// and appends it to `self.pending`.
    fn convert(&mut self, rate: u32, channels: u16) {
        if let Err(why) = check_rate(rate, channels) {
            self.fail(why);
            return;
        }
        self.remixed.clear();
        chorus_decode::remix(&self.decoded, channels, self.channels, &mut self.remixed);
        if self.resampler.as_ref().map(|(from, _)| *from) != Some(rate) {
            self.flush_resampler();
            self.resampler = Some((rate, Resampler::new(rate, self.rate, self.channels)));
            self.resampler_origin = self.produced;
            self.resampler_in = 0;
        }
        let Some((_, resampler)) = self.resampler.as_mut() else {
            return;
        };
        let wide = usize::from(self.channels);
        let before = self.pending.len();
        resampler.process(&self.remixed, &mut self.pending);
        self.produced += ((self.pending.len() - before) / wide) as u64;
        self.resampler_in += (self.remixed.len() / wide) as u64;
    }

    /// Emits what the resampler still owes and drops it.
    fn flush_resampler(&mut self) {
        if let Some((_, mut resampler)) = self.resampler.take() {
            let before = self.pending.len();
            resampler.flush(&mut self.pending);
            self.produced += ((self.pending.len() - before) / usize::from(self.channels)) as u64;
        }
        self.resampler_in = 0;
    }

    fn end_of_track(&mut self) {
        if self.next.as_ref().is_some_and(|t| t.src.is_some()) {
            self.join();
            return;
        }
        // The source stays open while the last frames go out, so a seek
        // back into the track needs no second fetch.
        self.flush_resampler();
        self.drain = Some(Drain::Ended);
    }

    /// The next track becomes the one being decoded; its first frame goes
    /// straight after the last frame of the one before.
    fn join(&mut self) {
        let Some(track) = self.next.take() else {
            return;
        };
        let (Some(info), Some(src)) = (track.info.clone(), track.src.as_ref()) else {
            return;
        };
        let rate = src.decoder.format().rate;
        let mark = match &self.resampler {
            // The same resampler carries on: output frame k is the input at
            // k * from / to, so the first output frame at or after input
            // frame n is ceil(n * to / from).
            Some((from, _)) if *from == rate => {
                let (n, to, from) = (
                    u128::from(self.resampler_in),
                    u128::from(self.rate),
                    u128::from(*from),
                );
                self.resampler_origin + (n * to).div_ceil(from) as u64
            }
            _ => {
                self.flush_resampler();
                self.produced
            }
        };
        self.joins.push_back(Join {
            mark,
            req: track.req.clone(),
            info: info.clone(),
        });
        self.title = info.tags.title;
        self.current = Some(track);
        self.drain = None;
        self.finished = false;
    }

    /// The current track cannot go on: what is queued plays out, then
    /// `Failed`.
    fn fail(&mut self, why: String) {
        self.flush_resampler();
        if let Some(track) = self.current.as_mut() {
            track.src = None;
        }
        self.drain = Some(Drain::Failed(why));
    }

    /// Everything written has gone out.
    fn end(&mut self) {
        self.events();
        let played_ms = self.position();
        let outcome = self.drain.take();
        // A start that never had a frame (an empty file) is still a start.
        self.started_owed = false;
        self.joins.clear();
        self.halt();
        match outcome {
            Some(Drain::Failed(reason)) => self.report(Event::Failed { reason }),
            _ => self.report(Event::Ended { played_ms }),
        }
    }

    // ----- what the handle reads -------------------------------------------

    fn position(&self) -> u64 {
        self.position_at(self.port.played_frames())
    }

    /// The position when the audio thread has taken `played` frames.
    fn position_at(&self, played: u64) -> u64 {
        if !self.playing {
            return 0;
        }
        let frames = played.saturating_sub(self.base_mark);
        let ms = self.offset_ms + frames * 1_000 / u64::from(self.rate);
        self.duration_ms.map_or(ms, |d| ms.min(d))
    }

    fn publish(&self) {
        let state = if self.current.is_none() {
            PlayerState::Empty
        } else if !self.playing {
            PlayerState::Stopped
        } else if self.paused {
            PlayerState::Paused
        } else if self.started_owed || self.filling {
            PlayerState::Buffering
        } else {
            PlayerState::Playing
        };
        let shared = &self.me.shared;
        shared.position_ms.store(self.position(), Ordering::SeqCst);
        shared
            .duration_ms
            .store(self.duration_ms.unwrap_or(NO_DURATION), Ordering::SeqCst);
        shared.state.store(state as u8, Ordering::SeqCst);
    }
}

fn check_rate(rate: u32, channels: u16) -> Result<(), String> {
    if !(Resampler::MIN_RATE..=Resampler::MAX_RATE).contains(&rate) {
        return Err(format!("unsupported: sample rate {rate} Hz"));
    }
    if channels == 0 {
        return Err("unsupported: media with no audio channel".to_string());
    }
    Ok(())
}

fn check_shape(format: &Format) -> Result<(), String> {
    check_rate(format.rate, format.channels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_extension_is_the_last_segments_and_ignores_the_query() {
        assert_eq!(
            extension_of("http://media.example/a/b.FLAC?x=1.mp3").as_deref(),
            Some("flac")
        );
        assert_eq!(extension_of("http://media.example/stream"), None);
        assert_eq!(extension_of("http://media.example/v1.2/"), None);
        assert_eq!(extension_of("http://media.example/.hidden"), None);
    }

    #[test]
    fn an_l16_type_names_its_rate_and_channels() {
        assert_eq!(l16_of("audio/L16;rate=44100;channels=2"), Some((44_100, 2)));
        assert_eq!(l16_of("audio/l16; rate=8000"), Some((8_000, 1)));
        assert_eq!(l16_of("audio/L16"), None, "a rate is required");
        assert_eq!(l16_of("audio/flac;rate=44100"), None);
    }

    #[test]
    fn a_generic_type_gives_way_to_the_callers() {
        assert!(is_generic("application/octet-stream"));
        assert!(is_generic(""));
        assert!(!is_generic("audio/mpeg"));
    }

    #[test]
    fn the_policy_names_the_listeners_and_refuses_loopback() {
        let policy = fetch_policy(&[4010, 0, 8080], false);
        assert!(!policy.allow_loopback);
        assert_eq!(policy.denied_ports_on_self, [4010, 8080]);
        assert_eq!(policy.ca_bundle, None);
    }

    #[test]
    fn the_pool_hands_out_idle_players_and_takes_them_back() {
        let (players, drivers) = Players::new(2, fetch_policy(&[], false));
        assert_eq!((players.len(), drivers.len()), (2, 2));
        assert_eq!(players.acquire("room:kitchen"), Some(0));
        assert_eq!(players.acquire("room:kitchen"), Some(0), "the one it holds");
        assert_eq!(players.acquire("room:study"), Some(1));
        assert_eq!(players.acquire("group:all"), None, "every player is held");
        assert_eq!(players.owner_of(1).as_deref(), Some("room:study"));
        assert_eq!(players.held_by("room:study"), Some(1));
        players.release(0);
        assert_eq!(players.owner_of(0), None);
        assert_eq!(players.acquire("group:all"), Some(0));
        assert!(players.take_reports().is_some());
        assert!(players.take_reports().is_none(), "there is one receiver");
        assert!(players.handle(2).is_none());
    }

    #[test]
    fn a_player_with_an_action_on_its_way_is_not_idle() {
        let (players, _drivers) = Players::new(1, fetch_policy(&[], false));
        let handle = players.handle(0).unwrap().clone();
        assert!(handle.is_idle());
        assert!(handle.send(
            1,
            Action::Load {
                uri: "http://media.example/a.flac".to_string(),
                mime: None
            }
        ));
        assert!(!handle.is_idle(), "nothing took the action yet");
        assert_eq!(players.acquire("room:kitchen"), None);
    }
}
