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
//! This module reads no clock and stamps nothing. It is on the audio path
//! because every chunk a session receives is decided by it.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError};
use std::sync::{Arc, Mutex, MutexGuard};

use chorus_protocol::v2::{encode, ControllerState, Message, RoomVolume};

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
}

/// Every session, and the fanouts they are routed to.
#[derive(Debug)]
pub struct Router {
    fanouts: Vec<Arc<Fanout>>,
    slots: usize,
    sessions: Mutex<Vec<Entry>>,
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
        sessions.push(Entry {
            id,
            endpoint: endpoint.to_string(),
            roles,
            out,
            route,
            room_volume: start.room_volume,
            controller_state: start.controller_state.clone(),
        });
        id
    }

    /// Stop carrying a session, which has ended.
    pub fn unregister(&self, id: u64) {
        let mut sessions = lock(&self.sessions);
        if let Some(at) = sessions.iter().position(|e| e.id == id) {
            let entry = sessions.remove(at);
            self.fanouts[entry.route].detach(id);
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
