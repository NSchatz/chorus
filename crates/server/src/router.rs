//! Which stream each session hears, on the one audio port.
//!
//! # Why routing lives in the session
//!
//! The C endpoint has ONE configured server address and never reads the
//! control plane, so it cannot be told "your group's stream is now over
//! there". Grouping is therefore routed here, inside each session, on the one
//! audio port: every session's outbound queue is attached to exactly one
//! [`Fanout`], and moving a room into another group moves its sessions' queues
//! from one fanout to another between two chunks (goal 11, the stream slots;
//! the ADR for this module says how).
//!
//! A move is seamless only because every fanout is fed on ONE grid: chunk `k`
//! of every slot carries the same sequence and the same presentation
//! timestamp (`crate::slots`). A session moved between ticks sees a
//! contiguous run of sequences whose CONTENT changes, which is what both
//! endpoint kinds already play without a restart (their playout drops only a
//! chunk at or behind the highwater). The grid guard ([`Router::grid`]) is what
//! makes "between ticks" true: the audio thread holds it while it broadcasts
//! one tick to every fanout, and a move takes it, so no session is handed
//! chunk `k` twice or not at all.
//!
//! # Two shapes
//!
//! - **One stream** (`--slots 0`, the default and today's shape): one fanout,
//!   and every session attached to it.
//! - **Slots** (`--slots S`): S fanouts, one per stream slot, and one more that
//!   plays silence to a session whose endpoint is in no room, or whose room's
//!   group has no slot (its source is `none`). The extra one is not counted in
//!   S: it is where nobody is listening to anything.
//!
//! # What it holds per session, and why
//!
//! The session's queue, its endpoint id and roles, the fanout it is on, and
//! the last `room_volume` and `controller_state` it was sent. The last two are
//! what dedupes the pushes (`crate::conductor`): a change is sent to a session
//! only when what it would be told differs from what it was last told.
//!
//! For a session that declared the `visualizer` role (goal 12,
//! `docs/visualizer.md`), also the bands its `capabilities` asked for, the
//! latency after which it plays what it is sent (its room's tier, which the
//! conductor keeps current), and the last colour it was sent. The audio
//! thread hands every frame its slot's analysis makes to
//! [`Router::push_visualizer`], which stamps it for each such session on the
//! slot at the moment that session's room hears it. How many such sessions
//! each fanout carries is kept in an atomic per fanout, so the audio thread
//! asks [`Router::watched`] without taking a lock and analyses only the slots
//! someone is watching.
//!
//! This module reads no clock and stamps nothing. It is on the audio path
//! because every chunk a session receives is decided by it.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError};
use std::sync::{Arc, Mutex, MutexGuard};

use chorus_dsp::visualizer::Frame;
use chorus_protocol::v2::{
    encode, roles, Color, ControllerState, Message, RoomVolume, VisualizerFrame,
};

use crate::stream::{Fanout, Outbound};

/// One session the router is carrying.
#[derive(Debug)]
struct Entry {
    id: u64,
    endpoint: String,
    roles: u16,
    out: SyncSender<Outbound>,
    route: usize,
    room_volume: Option<RoomVolume>,
    controller_state: Option<ControllerState>,
    visualizer_bands: u8,
    heard_latency_ns: u64,
    colour: Option<Color>,
}

impl Entry {
    fn watches(&self) -> bool {
        self.roles & roles::VISUALIZER != 0
    }
}

/// The latency after which a session plays what it is sent, until the
/// conductor says otherwise: the wired tier's playout latency
/// (`crate::conductor::WIRED_GROUP_LATENCY_NS`, `config/sync.conf`).
pub const DEFAULT_HEARD_LATENCY_NS: u64 = 180_000_000;

/// What [`Router::push_visualizer`] did with one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VisualizerPush {
    /// `visualizer_frame` messages queued.
    pub frames: u64,
    /// `color` messages queued.
    pub colours: u64,
    /// Messages a full queue refused (dropped, never retried: the next
    /// frame supersedes this one).
    pub dropped: u64,
}

/// One session as the conductor sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionView {
    /// The router's id for it.
    pub id: u64,
    /// The endpoint's authenticated id.
    pub endpoint: String,
    /// The roles its `hello` declared.
    pub roles: u16,
    /// The fanout it is on.
    pub route: usize,
}

/// Where a session starts: computed from the room model by the caller when
/// the session comes up, so that what the greeting says and what the router
/// records as sent are the same values.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionStart {
    /// The fanout to attach to.
    pub route: usize,
    /// The `room_volume` the greeting carries, if the session is a player in
    /// a room.
    pub room_volume: Option<RoomVolume>,
    /// The `controller_state` the greeting carries, if the session declared
    /// the controller role and its endpoint is in a room.
    pub controller_state: Option<ControllerState>,
    /// The visualizer bands its `capabilities` asked for (0 to 64).
    pub visualizer_bands: u8,
}

/// Every session, and the fanouts they are routed to.
#[derive(Debug)]
pub struct Router {
    fanouts: Vec<Arc<Fanout>>,
    slots: usize,
    sessions: Mutex<Vec<Entry>>,
    /// Visualizer sessions on each fanout.
    watchers: Vec<AtomicUsize>,
    grid: Mutex<()>,
    next: AtomicU64,
    moves: AtomicU64,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

impl Router {
    /// The one-stream shape: one fanout, every session on it.
    pub fn single(fanout: Arc<Fanout>) -> Router {
        Router::with(vec![fanout], 0)
    }

    /// The slot shape: `slots` fanouts, then the silent one.
    pub fn slotted(slots: usize) -> Router {
        let fanouts = (0..=slots).map(|_| Arc::new(Fanout::new())).collect();
        Router::with(fanouts, slots)
    }

    fn with(fanouts: Vec<Arc<Fanout>>, slots: usize) -> Router {
        Router {
            watchers: fanouts.iter().map(|_| AtomicUsize::new(0)).collect(),
            fanouts,
            slots,
            sessions: Mutex::new(Vec::new()),
            grid: Mutex::new(()),
            next: AtomicU64::new(0),
            moves: AtomicU64::new(0),
        }
    }

    /// How many stream slots this router carries; 0 in the one-stream shape.
    pub fn slots(&self) -> usize {
        self.slots
    }

    /// Every fanout, slot by slot, the silent one last in the slot shape.
    pub fn fanouts(&self) -> &[Arc<Fanout>] {
        &self.fanouts
    }

    /// The fanout a session in no slot is routed to: the silent one in the
    /// slot shape, the only one otherwise.
    pub fn idle(&self) -> usize {
        self.slots
    }

    /// Held by the audio thread while it broadcasts one tick to every fanout,
    /// and by a move, so that a move happens between two ticks.
    pub fn grid(&self) -> MutexGuard<'_, ()> {
        lock(&self.grid)
    }

    /// Carry a session from now on: attach its queue to the fanout `start`
    /// names and record what its greeting told it. Returns its id.
    pub fn register(
        &self,
        endpoint: &str,
        roles: u16,
        out: SyncSender<Outbound>,
        start: &SessionStart,
    ) -> u64 {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let route = start.route.min(self.fanouts.len() - 1);
        let mut sessions = lock(&self.sessions);
        {
            let _between_ticks = self.grid();
            self.fanouts[route].attach(id, out.clone());
        }
        let entry = Entry {
            id,
            endpoint: endpoint.to_string(),
            roles,
            out,
            route,
            room_volume: start.room_volume,
            controller_state: start.controller_state.clone(),
            visualizer_bands: start.visualizer_bands,
            heard_latency_ns: DEFAULT_HEARD_LATENCY_NS,
            colour: None,
        };
        if entry.watches() {
            self.watchers[route].fetch_add(1, Ordering::Relaxed);
        }
        sessions.push(entry);
        id
    }

    /// Stop carrying a session, which has ended.
    pub fn unregister(&self, id: u64) {
        let mut sessions = lock(&self.sessions);
        if let Some(at) = sessions.iter().position(|e| e.id == id) {
            let entry = sessions.remove(at);
            self.fanouts[entry.route].detach(id);
            if entry.watches() {
                self.watchers[entry.route].fetch_sub(1, Ordering::Relaxed);
            }
        }
    }

    /// Every session, as the conductor plans over them.
    pub fn sessions(&self) -> Vec<SessionView> {
        lock(&self.sessions)
            .iter()
            .map(|e| SessionView {
                id: e.id,
                endpoint: e.endpoint.clone(),
                roles: e.roles,
                route: e.route,
            })
            .collect()
    }

    /// How many sessions the router carries.
    pub fn count(&self) -> usize {
        lock(&self.sessions).len()
    }

    /// How many moves between fanouts have been made, over the run.
    pub fn moves(&self) -> u64 {
        self.moves.load(Ordering::Relaxed)
    }

    /// How many sessions are on each fanout, slot by slot.
    pub fn routed(&self) -> Vec<usize> {
        let sessions = lock(&self.sessions);
        (0..self.fanouts.len())
            .map(|f| sessions.iter().filter(|e| e.route == f).count())
            .collect()
    }

    /// Move a session to fanout `route`, between two ticks. Returns whether it
    /// moved.
    pub fn move_to(&self, id: u64, route: usize) -> bool {
        if route >= self.fanouts.len() {
            return false;
        }
        let mut sessions = lock(&self.sessions);
        let Some(entry) = sessions.iter_mut().find(|e| e.id == id) else {
            return false;
        };
        if entry.route == route {
            return false;
        }
        {
            let _between_ticks = self.grid();
            self.fanouts[entry.route].detach(id);
            self.fanouts[route].attach(id, entry.out.clone());
        }
        if entry.watches() {
            self.watchers[entry.route].fetch_sub(1, Ordering::Relaxed);
            self.watchers[route].fetch_add(1, Ordering::Relaxed);
        }
        entry.route = route;
        self.moves.fetch_add(1, Ordering::Relaxed);
        true
    }

    /// Send a session `room_volume` unless its gain and limit are what the
    /// session was last sent (a ramp step and the at-once value it reaches
    /// are one change, not two). Returns `Some(true)` when sent,
    /// `Some(false)` when it was a repeat, and `None` when the session's
    /// queue is full (it is left as it was, so the next pass tries again) or
    /// the session is gone.
    pub fn push_room_volume(&self, id: u64, message: RoomVolume) -> Option<bool> {
        let mut sessions = lock(&self.sessions);
        let entry = sessions.iter_mut().find(|e| e.id == id)?;
        if entry
            .room_volume
            .is_some_and(|m| m.gain == message.gain && m.limit == message.limit)
        {
            return Some(false);
        }
        send(&entry.out, &Message::RoomVolume(message))?;
        entry.room_volume = Some(message);
        Some(true)
    }

    /// Send a session any other v2 message (a line-in's `source_control`).
    /// `false` when its queue is full or it is gone.
    pub fn push_message(&self, id: u64, message: &Message) -> bool {
        let sessions = lock(&self.sessions);
        match sessions.iter().find(|e| e.id == id) {
            Some(entry) => send(&entry.out, message).is_some(),
            None => false,
        }
    }

    /// Send a session `controller_state` unless it is what the session was
    /// last sent, or always when `answer` (it answers the session's own
    /// command, which is owed a reply whatever it changed). Same return as
    /// [`Router::push_room_volume`].
    pub fn push_controller_state(
        &self,
        id: u64,
        state: &ControllerState,
        answer: bool,
    ) -> Option<bool> {
        let mut sessions = lock(&self.sessions);
        let entry = sessions.iter_mut().find(|e| e.id == id)?;
        if !answer && entry.controller_state.as_ref() == Some(state) {
            return Some(false);
        }
        send(&entry.out, &Message::ControllerState(state.clone()))?;
        entry.controller_state = Some(state.clone());
        Some(true)
    }
}

impl Router {
    /// Whether any visualizer session is on fanout `route`, without a lock:
    /// the audio thread analyses only the slots someone watches.
    pub fn watched(&self, route: usize) -> bool {
        self.watchers
            .get(route)
            .is_some_and(|w| w.load(Ordering::Relaxed) > 0)
    }

    /// The latency after which session `id` plays what it is sent, ns: its
    /// room's tier (`crate::conductor`). Returns whether it changed.
    pub fn set_heard_latency(&self, id: u64, latency_ns: u64) -> bool {
        let mut sessions = lock(&self.sessions);
        match sessions.iter_mut().find(|e| e.id == id) {
            Some(entry) if entry.heard_latency_ns != latency_ns => {
                entry.heard_latency_ns = latency_ns;
                true
            }
            _ => false,
        }
    }

    /// Send `frame`, which describes the audio fanout `route` carried at
    /// `at_ns` on the server timeline, to every visualizer session on that
    /// fanout: a `visualizer_frame` with the bands it asked for, stamped
    /// `at_ns` plus its heard latency (when its room hears that audio,
    /// docs/protocol.md 0x34), and the frame's colour as a `color` with the
    /// same stamp when it differs from the last one that session was sent.
    /// A session without the role is sent nothing. `bands` is scratch.
    ///
    /// Called by the audio thread after a tick's broadcast, outside the grid
    /// guard (a move takes this lock and then the guard, so the audio thread
    /// never holds the guard while it waits here).
    pub fn push_visualizer(
        &self,
        route: usize,
        at_ns: u64,
        frame: &Frame,
        bands: &mut Vec<u8>,
    ) -> VisualizerPush {
        let mut done = VisualizerPush::default();
        let mut sessions = lock(&self.sessions);
        for entry in sessions
            .iter_mut()
            .filter(|e| e.route == route && e.watches())
        {
            let heard_ns = at_ns.saturating_add(entry.heard_latency_ns);
            frame.bands(usize::from(entry.visualizer_bands), bands);
            let message = Message::VisualizerFrame(VisualizerFrame {
                timestamp_ns: heard_ns,
                beat: frame.beat,
                peak: frame.peak,
                bands: bands.clone(),
            });
            match send(&entry.out, &message) {
                Some(()) => done.frames += 1,
                None => done.dropped += 1,
            }
            let Some(c) = frame.colour else {
                continue;
            };
            let colour = Color {
                timestamp_ns: heard_ns,
                red: c.red,
                green: c.green,
                blue: c.blue,
                brightness: c.brightness,
                transition_ms: c.transition_ms,
            };
            let same = entry.colour.is_some_and(|last| {
                (last.red, last.green, last.blue, last.brightness)
                    == (colour.red, colour.green, colour.blue, colour.brightness)
            });
            if same {
                continue;
            }
            match send(&entry.out, &Message::Color(colour)) {
                Some(()) => {
                    entry.colour = Some(colour);
                    done.colours += 1;
                }
                None => done.dropped += 1,
            }
        }
        done
    }
}

/// One v2 message onto a session's queue, never blocking: a session that is
/// not reading is 128 items behind already, and the next pass tries again.
fn send(out: &SyncSender<Outbound>, message: &Message) -> Option<()> {
    let frame = encode(message).ok()?;
    match out.try_send(Outbound::Frame(Arc::new(frame))) {
        Ok(()) => Some(()),
        Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    use crate::stream::SUBSCRIBER_QUEUE_LIMIT;

    fn frame(byte: u8) -> Outbound {
        Outbound::Frame(Arc::new(vec![byte]))
    }

    #[test]
    fn a_session_moved_between_ticks_hears_each_tick_exactly_once() {
        let router = Router::slotted(2);
        let (out, inbox) = mpsc::sync_channel(SUBSCRIBER_QUEUE_LIMIT);
        let id = router.register("e", 0, out, &SessionStart::default());
        // Tick 0 on both slots, the move, tick 1 on both slots.
        for tick in 0..2u8 {
            {
                let _g = router.grid();
                router.fanouts()[0].broadcast(frame(tick * 10));
                router.fanouts()[1].broadcast(frame(tick * 10 + 1));
            }
            if tick == 0 {
                assert!(router.move_to(id, 1));
            }
        }
        let got: Vec<Outbound> = inbox.try_iter().collect();
        assert_eq!(
            got,
            vec![frame(0), frame(11)],
            "slot 0's tick 0, slot 1's tick 1"
        );
        assert_eq!(router.routed(), vec![0, 1, 0]);
        assert_eq!(router.moves(), 1);
        router.unregister(id);
        assert_eq!(router.count(), 0);
        assert_eq!(router.fanouts()[1].subscribers(), 0);
    }

    fn visualizer_messages(inbox: &mpsc::Receiver<Outbound>) -> Vec<Message> {
        inbox
            .try_iter()
            .filter_map(|o| match o {
                Outbound::Frame(bytes) => match chorus_protocol::v2::decode_frame(&bytes).outcome {
                    chorus_protocol::v2::Outcome::Decoded(
                        m @ (Message::VisualizerFrame(_) | Message::Color(_)),
                    ) => Some(m),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }

    fn analysed(colour: Option<chorus_dsp::visualizer::Colour>) -> Frame {
        let mut band_db = [chorus_dsp::visualizer::FLOOR_DB as f32; chorus_dsp::visualizer::BANDS];
        band_db[0] = 0.0;
        Frame {
            at_sample: 0,
            peak: 200,
            beat: 150,
            band_db,
            colour,
        }
    }

    #[test]
    fn visualizer_frames_reach_only_the_watching_sessions_of_their_slot_stamped_when_heard() {
        let router = Router::slotted(2);
        let start = |route: usize, bands: u8| SessionStart {
            route,
            visualizer_bands: bands,
            ..SessionStart::default()
        };
        let session = |roles: u16, s: &SessionStart| {
            let (out, inbox) = mpsc::sync_channel(SUBSCRIBER_QUEUE_LIMIT);
            (router.register("e", roles, out, s), inbox)
        };
        let (wide, wide_in) = session(roles::PLAYER | roles::VISUALIZER, &start(0, 16));
        let (_, narrow_in) = session(roles::VISUALIZER, &start(0, 4));
        let (_, plain_in) = session(roles::PLAYER, &start(0, 16));
        let (_, other_in) = session(roles::PLAYER | roles::VISUALIZER, &start(1, 16));
        assert!(router.watched(0) && router.watched(1) && !router.watched(2));
        assert!(router.set_heard_latency(wide, 500_000_000));
        assert!(!router.set_heard_latency(wide, 500_000_000), "unchanged");

        let colour = chorus_dsp::visualizer::Colour {
            red: 0,
            green: 0,
            blue: 255,
            brightness: 100,
            transition_ms: 500,
        };
        let mut scratch = Vec::new();
        let done = router.push_visualizer(0, 1_000, &analysed(Some(colour)), &mut scratch);
        assert_eq!(
            done,
            VisualizerPush {
                frames: 2,
                colours: 2,
                dropped: 0
            }
        );
        // The same colour again is not sent again.
        let again = router.push_visualizer(0, 2_000, &analysed(Some(colour)), &mut scratch);
        assert_eq!((again.frames, again.colours), (2, 0));

        let wide_got = visualizer_messages(&wide_in);
        match &wide_got[..] {
            [Message::VisualizerFrame(f), Message::Color(c), Message::VisualizerFrame(g)] => {
                assert_eq!(f.timestamp_ns, 1_000 + 500_000_000, "its heard latency");
                assert_eq!((f.beat, f.peak, f.bands.len()), (150, 200, 16));
                assert_eq!(f.bands[0], 255);
                assert_eq!(c.timestamp_ns, f.timestamp_ns);
                assert_eq!((c.blue, c.brightness, c.transition_ms), (255, 100, 500));
                assert_eq!(g.timestamp_ns, 2_000 + 500_000_000);
            }
            other => panic!("{:?}", other),
        }
        match &visualizer_messages(&narrow_in)[..] {
            [Message::VisualizerFrame(f), Message::Color(_), Message::VisualizerFrame(_)] => {
                assert_eq!(f.timestamp_ns, 1_000 + DEFAULT_HEARD_LATENCY_NS);
                assert_eq!(f.bands.len(), 4, "the bands it asked for");
            }
            other => panic!("{:?}", other),
        }
        assert!(
            visualizer_messages(&plain_in).is_empty(),
            "no role, nothing"
        );
        assert!(visualizer_messages(&other_in).is_empty(), "another slot");

        // A move carries the watcher count; leaving drops it.
        assert!(router.move_to(wide, 2));
        assert!(router.watched(2));
        router.unregister(wide);
        assert!(!router.watched(2));
    }

    #[test]
    fn a_push_is_sent_once_per_change_and_a_full_queue_is_retried() {
        let router = Router::single(Arc::new(Fanout::new()));
        let (out, inbox) = mpsc::sync_channel(1);
        let first = RoomVolume {
            gain: 500,
            limit: 800,
            ramp_ms: 0,
        };
        let start = SessionStart {
            room_volume: Some(first),
            ..SessionStart::default()
        };
        let id = router.register("e", 0, out, &start);
        assert_eq!(
            router.push_room_volume(id, first),
            Some(false),
            "the greeting said it"
        );
        let next = RoomVolume { gain: 400, ..first };
        assert_eq!(router.push_room_volume(id, next), Some(true));
        assert_eq!(router.push_room_volume(id, next), Some(false));
        let third = RoomVolume { gain: 300, ..first };
        assert_eq!(
            router.push_room_volume(id, third),
            None,
            "the queue is full"
        );
        assert!(inbox.try_recv().is_ok());
        assert_eq!(router.push_room_volume(id, third), Some(true), "retried");
    }
}
