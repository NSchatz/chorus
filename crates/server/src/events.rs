//! One thread writes every event stream (audit finding B-5).
//!
//! # Why one writer
//!
//! A `GET /api/events` stream lasts as long as its subscriber stays, and every
//! open page and every endpoint is one. When each stream held a control worker
//! for its whole life, a house with as many subscribers as workers answered
//! every command `503` (the stopgap kept one worker for commands). Here a
//! worker only opens the stream: it writes the headers and the opening state
//! under its own write timeout, subscribes it to the fanout, and hands the
//! socket to this module's one thread, then goes back to the pool. However
//! many subscribers there are, they cost no worker, and the thread population
//! stays the declared one (`crates/server/src/main.rs`).
//!
//! # Two kinds of stream, one writer
//!
//! A `GET /api/controller-events` stream (one `controller_event` per button
//! press the server accepted) is held here too, as one more peer: the same
//! ceiling, the same keepalive, the same stall bound, and no thread. It is
//! fed by a second fanout (`ControlState`'s `presses`) and its messages are
//! written as they are; nothing else about it is different to this module.
//!
//! # A third kind, which has no queue
//!
//! A `GET /api/visualizer?zone=<room>` stream (`crate::lights`,
//! `docs/visualizer.md`) is one more peer under the same ceiling, keepalive
//! and stall bound, and no thread either. It is not fed by a fanout: when it
//! has written its last frame and the rate cap allows another, the writer
//! takes the room's LATEST frame, whatever was made in between. So there is
//! no queue to reach a ceiling and such a subscriber is never dropped for
//! being slow, only for making no write progress at all; what it missed was
//! superseded. The audio thread wakes the writer when a room has a new
//! frame, and the writer wakes itself when a held-back subscriber's cap runs
//! out.
//!
//! # A fourth kind, which is not an event stream
//!
//! A `GET /api/voice-audio?run=<id>` stream (`crate::voice`,
//! `docs/control-plane.md`, "Voice: the wake word, the run and its audio") is
//! the reader of one open voice run: raw 16 kHz mono PCM, no `data:` lines
//! and no keepalive comment, for as long as the run lasts. It is held here
//! for the reason the others are, so a run costs no control worker, and
//! under the same ceiling and stall bound. The writer asks the voice module
//! for the run's next bytes when it has written the last, and closes the
//! stream when the module says the run has ended; a reader that closes or
//! stalls ends its run. The same thread makes the voice module's pass
//! ([`ControlState::voice_pass`]): the wake word is heard here, off every
//! session's reader, and a `voice_wake` is fanned out to the
//! `GET /api/voice-events` streams, which are one more [`Feed::Messages`].
//!
//! # A ceiling, and a stalled peer
//!
//! The writer holds at most `--event-streams` streams (default
//! [`DEFAULT_EVENT_STREAMS`]); one asked for past that is answered `503`,
//! naming the ceiling, by the worker that would have handed it over. Every
//! socket here is non-blocking, so one peer that stops draining its receive
//! window cannot hold this thread inside `write`: its bytes wait in its own
//! pending buffer, which holds at most one state message (the next is taken
//! off its fanout queue only once the last is written, so the fanout's own
//! bounded queue, and its drop of a subscriber at that queue's ceiling, stay
//! the backlog bound: `chorus_control::fanout`). A peer that makes no write
//! progress for [`crate::control::WRITE_TIMEOUT`] is dropped and counted, and
//! the others are written on every pass regardless.
//!
//! Control code: it touches no PCM, stamps nothing, and is never called from
//! the audio thread. Its only clock is the monotonic one, for the keepalive
//! and the stall bound.

use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::control::{ControlState, KEEPALIVE, WRITE_TIMEOUT};
use crate::lights::LightSubscription;
use crate::voice::{RunRead, Voice};

/// Event streams one server holds at once when `--event-streams` is not
/// given. ASSUMED: a house of a few dozen endpoints and pages, with room to
/// spare; not a measured bound. Each costs one socket and at most one state
/// message of buffer, never a thread.
pub const DEFAULT_EVENT_STREAMS: usize = 64;

/// How often the writer looks up when nothing woke it: to notice a stopped
/// run, and to send keepalives.
const IDLE_WAKE: Duration = Duration::from_millis(200);

/// How soon the writer tries again while a peer has bytes it could not take.
const RETRY: Duration = Duration::from_millis(20);

/// What a stream is fed by.
enum Feed {
    /// A fanout's bounded queue (states, or controller events), written at
    /// catalog version `version`.
    Messages {
        inbox: Receiver<Arc<String>>,
        version: i64,
    },
    /// A room's latest visualizer frame, under the rate cap.
    Light(LightSubscription),
    /// One open voice run's audio, for its one reader.
    Mic {
        voice: Arc<Voice>,
        /// The run's identifier, as its reader offered it.
        run: String,
    },
}

/// The most of a run's audio taken for its reader at once: a quarter of a
/// second. What a reader has not taken waits in the run's own bounded queue.
const MIC_CHUNK_BYTES: usize = 8_000;

/// One stream the writer holds.
struct Peer {
    socket: TcpStream,
    feed: Feed,
    pending: Vec<u8>,
    written: usize,
    progress: Instant,
    last_write: Instant,
}

/// What a worker hands the writer.
pub struct HandOver {
    socket: TcpStream,
    feed: Feed,
}

/// The event streams the writer holds, and the way a stream reaches it.
pub struct EventStreams {
    ceiling: usize,
    held: AtomicUsize,
    stalled: AtomicU64,
    handoff: SyncSender<HandOver>,
    taken: Mutex<Option<Receiver<HandOver>>>,
    wake: SyncSender<()>,
    woken: Mutex<Option<Receiver<()>>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

impl EventStreams {
    /// At most `ceiling` streams.
    pub fn new(ceiling: usize) -> EventStreams {
        let (handoff, taken) = mpsc::sync_channel(ceiling.max(1));
        let (wake, woken) = mpsc::sync_channel(1);
        EventStreams {
            ceiling,
            held: AtomicUsize::new(0),
            stalled: AtomicU64::new(0),
            handoff,
            taken: Mutex::new(Some(taken)),
            wake,
            woken: Mutex::new(Some(woken)),
        }
    }

    /// The ceiling in force.
    pub fn ceiling(&self) -> usize {
        self.ceiling
    }

    /// Streams held now, counting one being opened.
    pub fn held(&self) -> usize {
        self.held.load(Ordering::SeqCst)
    }

    /// Tell the writer there is something to write.
    pub fn wake(&self) {
        let _ = self.wake.try_send(());
    }

    /// The wake, for a thread that holds no `ControlState` (the audio
    /// thread, through `crate::lights`): a `try_send` on it never blocks.
    pub fn waker(&self) -> SyncSender<()> {
        self.wake.clone()
    }

    /// Claim one of the ceiling's streams, or `None` when every one is held.
    pub fn claim(&self) -> Option<Claim<'_>> {
        self.held
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |held| {
                (held < self.ceiling).then_some(held + 1)
            })
            .ok()
            .map(|_| Claim {
                streams: self,
                handed: false,
            })
    }

    /// `events held=H ceiling=K stalled_dropped=D`, for the report.
    pub fn report(&self) -> String {
        format!(
            "events held={} ceiling={} stalled_dropped={}",
            self.held(),
            self.ceiling,
            self.stalled.load(Ordering::Relaxed)
        )
    }
}

/// One claimed stream: handed to the writer, or given back when dropped.
pub struct Claim<'a> {
    streams: &'a EventStreams,
    handed: bool,
}

impl Claim<'_> {
    /// Give the writer an opened stream: its socket, its fanout queue and the
    /// catalog version it is written at. A stream whose messages are not
    /// states (`GET /api/controller-events`) is handed over at the build's
    /// own version, at which a message is written as it is.
    pub fn hand_over(self, socket: TcpStream, inbox: Receiver<Arc<String>>, version: i64) {
        self.hand(socket, Feed::Messages { inbox, version });
    }

    /// Give the writer an opened visualizer stream (`GET /api/visualizer`):
    /// its socket and its subscription to one room's latest frame.
    pub fn hand_over_light(self, socket: TcpStream, subscription: LightSubscription) {
        self.hand(socket, Feed::Light(subscription));
    }

    /// Give the writer the reader of one open voice run
    /// (`GET /api/voice-audio`): its socket, and the run it claimed.
    pub fn hand_over_mic(self, socket: TcpStream, voice: Arc<Voice>, run: String) {
        self.hand(socket, Feed::Mic { voice, run });
    }

    fn hand(mut self, socket: TcpStream, feed: Feed) {
        match self.streams.handoff.try_send(HandOver { socket, feed }) {
            Ok(()) => {
                self.handed = true;
                self.streams.wake();
            }
            // Never taken by the writer: a run whose reader this was has
            // none, and ends.
            Err(mpsc::TrySendError::Full(lost)) | Err(mpsc::TrySendError::Disconnected(lost)) => {
                if let Feed::Mic { voice, run } = &lost.feed {
                    voice.reader_gone(run, "reader-gone");
                }
            }
        }
    }
}

impl Drop for Claim<'_> {
    fn drop(&mut self) {
        if !self.handed {
            self.streams.held.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

/// The writer's loop: run on the `event-writer` thread, created with the rest
/// of the population before the scheduling report. Returns when `keep` says
/// stop.
pub fn run_writer(state: Arc<ControlState>, keep: Arc<AtomicBool>) {
    let streams = state.events();
    let (Some(arrivals), Some(woken)) = (lock(&streams.taken).take(), lock(&streams.woken).take())
    else {
        return;
    };
    let mut peers: Vec<Peer> = Vec::new();
    // When the next open voice run reaches its limit: the writer looks up
    // then, so a run ends at its limit and not at the next idle wake.
    let mut voice_due: Option<Instant> = None;
    while keep.load(Ordering::SeqCst) {
        let waiting = peers.iter().any(|p| p.written < p.pending.len());
        // A visualizer subscriber the rate cap is holding back is looked at
        // again the moment its cap runs out, not at the next wake.
        let now = Instant::now();
        let capped = peers
            .iter()
            .filter_map(|p| match &p.feed {
                Feed::Light(subscription) => subscription.wait(now),
                Feed::Messages { .. } | Feed::Mic { .. } => None,
            })
            .chain(voice_due.map(|due| due.saturating_duration_since(now)))
            .min();
        let rest = if waiting { RETRY } else { IDLE_WAKE };
        match woken.recv_timeout(capped.map_or(rest, |c| c.min(rest))) {
            Ok(()) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        while let Ok(handed) = arrivals.try_recv() {
            let now = Instant::now();
            if handed.socket.set_nonblocking(true).is_err() {
                streams.held.fetch_sub(1, Ordering::SeqCst);
                if let Feed::Mic { voice, run } = &handed.feed {
                    voice.reader_gone(run, "reader-gone");
                }
                continue;
            }
            peers.push(Peer {
                socket: handed.socket,
                feed: handed.feed,
                pending: Vec::new(),
                written: 0,
                progress: now,
                last_write: now,
            });
        }
        // The voice path's pass, before the streams are written: a wake word
        // heard now is on its subscribers' queues for this pass, and a run
        // past its limit has ended before its reader is served.
        let now = Instant::now();
        voice_due = state.voice_pass(now);
        let before = peers.len();
        peers.retain_mut(|peer| match serve(peer, &state, now) {
            Served::Keep => true,
            Served::Gone => {
                // A run whose reader went has nobody to listen for.
                if let Feed::Mic { voice, run } = &peer.feed {
                    voice.reader_gone(run, "reader-gone");
                }
                false
            }
            Served::Stalled => {
                if let Feed::Mic { voice, run } = &peer.feed {
                    voice.reader_gone(run, "reader-stalled");
                }
                streams.stalled.fetch_add(1, Ordering::Relaxed);
                eprintln!(
                    "chorus-server: event stream dropped reason=no-write-progress waited_ms={}",
                    WRITE_TIMEOUT.as_millis()
                );
                false
            }
        });
        let gone = before - peers.len();
        if gone > 0 {
            streams.held.fetch_sub(gone, Ordering::SeqCst);
        }
    }
}

enum Served {
    Keep,
    Gone,
    Stalled,
}

/// One pass over one peer: take its next message when it has written the
/// last, a keepalive when it has been quiet, and write what it will take.
fn serve(peer: &mut Peer, state: &ControlState, now: Instant) -> Served {
    if let Feed::Mic { voice, run } = &peer.feed {
        // A run that has ended (stopped, muted, at its limit) is written
        // nothing more, not even what its reader had been handed and had
        // not yet taken: that is overwritten and the stream is closed.
        if !voice.is_open(run, now) {
            peer.pending.fill(0);
            return Served::Gone;
        }
        // The reader sends nothing after its request, so the only thing a
        // read can say is that it has gone. Asked on every pass: a run with
        // nothing to write would otherwise not learn it.
        let mut scratch = [0u8; 64];
        match peer.socket.read(&mut scratch) {
            Ok(0) => return Served::Gone,
            Ok(_) => {}
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) => {}
            Err(_) => return Served::Gone,
        }
    }
    if peer.written == peer.pending.len() {
        // What was written held microphone audio when this is a run's
        // reader: it is overwritten before it is let go.
        if let Feed::Mic { .. } = &peer.feed {
            peer.pending.fill(0);
        }
        peer.pending.clear();
        peer.written = 0;
        if let Feed::Mic { voice, run } = &peer.feed {
            // Raw bytes, no event framing and no keepalive: the run's next
            // audio, or the end of the stream with the run.
            match voice.read(run, MIC_CHUNK_BYTES, now) {
                RunRead::Audio(bytes) => {
                    peer.pending = bytes;
                    peer.progress = now;
                }
                RunRead::Nothing => {}
                RunRead::Ended => return Served::Gone,
            }
            return write_pending(peer, now);
        }
        // The next message, if there is one to write now.
        let next = match &mut peer.feed {
            Feed::Messages { inbox, version } => match inbox.try_recv() {
                // The fanout carries the build's own (v2) state. A v1
                // subscriber is written the v1 rendering of the state as it
                // stands when the change reaches it: still one complete
                // snapshot per change, never an older one.
                Ok(message) => Some(format!(
                    "data: {}\n\n",
                    if *version == chorus_control::CATALOG_VERSION {
                        message.to_string()
                    } else {
                        state.encoded_state_at(*version)
                    }
                )),
                Err(TryRecvError::Empty) => None,
                // The fanout dropped this subscriber at its queue's ceiling.
                Err(TryRecvError::Disconnected) => return Served::Gone,
            },
            // The room's latest frame, when the cap allows one and there is
            // one newer than the last this subscriber was sent.
            // The clock is read here, not taken from the pass: the frame's
            // lead and its cap are both reckoned at this instant, and a pass
            // that served other peers first does not overstate the lead.
            Feed::Light(subscription) => subscription.next(Instant::now()),
            // Handled above.
            Feed::Mic { .. } => None,
        };
        match next {
            Some(text) => {
                peer.pending.extend_from_slice(text.as_bytes());
                peer.progress = now;
            }
            None => {
                if now.duration_since(peer.last_write) >= KEEPALIVE {
                    peer.pending.extend_from_slice(b": keepalive\n\n");
                    peer.progress = now;
                }
            }
        }
    }
    write_pending(peer, now)
}

/// Write what `peer` has pending, as far as its socket takes it.
fn write_pending(peer: &mut Peer, now: Instant) -> Served {
    while peer.written < peer.pending.len() {
        match peer.socket.write(&peer.pending[peer.written..]) {
            Ok(0) => return Served::Gone,
            Ok(n) => {
                peer.written += n;
                peer.progress = now;
                peer.last_write = now;
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return Served::Gone,
        }
    }
    if peer.written < peer.pending.len() && now.duration_since(peer.progress) >= WRITE_TIMEOUT {
        return Served::Stalled;
    }
    Served::Keep
}
