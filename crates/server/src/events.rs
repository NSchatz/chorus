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

use std::io::{self, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::control::{ControlState, KEEPALIVE, WRITE_TIMEOUT};

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

/// One stream the writer holds.
struct Peer {
    socket: TcpStream,
    inbox: Receiver<Arc<String>>,
    version: i64,
    pending: Vec<u8>,
    written: usize,
    progress: Instant,
    last_write: Instant,
}

/// What a worker hands the writer.
pub struct HandOver {
    socket: TcpStream,
    inbox: Receiver<Arc<String>>,
    version: i64,
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
    /// catalog version it is written at.
    pub fn hand_over(mut self, socket: TcpStream, inbox: Receiver<Arc<String>>, version: i64) {
        let sent = self
            .streams
            .handoff
            .try_send(HandOver {
                socket,
                inbox,
                version,
            })
            .is_ok();
        if sent {
            self.handed = true;
            self.streams.wake();
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
    while keep.load(Ordering::SeqCst) {
        let waiting = peers.iter().any(|p| p.written < p.pending.len());
        match woken.recv_timeout(if waiting { RETRY } else { IDLE_WAKE }) {
            Ok(()) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        while let Ok(handed) = arrivals.try_recv() {
            let now = Instant::now();
            if handed.socket.set_nonblocking(true).is_err() {
                streams.held.fetch_sub(1, Ordering::SeqCst);
                continue;
            }
            peers.push(Peer {
                socket: handed.socket,
                inbox: handed.inbox,
                version: handed.version,
                pending: Vec::new(),
                written: 0,
                progress: now,
                last_write: now,
            });
        }
        let now = Instant::now();
        let before = peers.len();
        peers.retain_mut(|peer| match serve(peer, &state, now) {
            Served::Keep => true,
            Served::Gone => false,
            Served::Stalled => {
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
    if peer.written == peer.pending.len() {
        peer.pending.clear();
        peer.written = 0;
        match peer.inbox.try_recv() {
            Ok(message) => {
                // The fanout carries the build's own (v2) state. A v1
                // subscriber is written the v1 rendering of the state as it
                // stands when the change reaches it: still one complete
                // snapshot per change, never an older one.
                let text = if peer.version == chorus_control::CATALOG_VERSION {
                    message.to_string()
                } else {
                    state.encoded_state_at(peer.version)
                };
                peer.pending
                    .extend_from_slice(format!("data: {}\n\n", text).as_bytes());
                peer.progress = now;
            }
            Err(TryRecvError::Empty) => {
                if now.duration_since(peer.last_write) >= KEEPALIVE {
                    peer.pending.extend_from_slice(b": keepalive\n\n");
                    peer.progress = now;
                }
            }
            // The fanout dropped this subscriber at its queue's ceiling.
            Err(TryRecvError::Disconnected) => return Served::Gone,
        }
    }
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
