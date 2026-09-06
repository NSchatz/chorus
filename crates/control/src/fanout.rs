//! One state, every subscriber, and a ceiling on how far behind one of them
//! may fall.
//!
//! This follows the shape `crates/server/src/stream.rs` already uses for
//! audio - no back pressure on the thing producing, a bounded queue per
//! subscriber, and every drop counted and reported - and it differs from it in
//! one deliberate way, which is worth stating because the difference looks like
//! an inconsistency until the reason is on the page.
//!
//! **Audio drops the item and keeps the subscriber. Control drops the
//! subscriber.** An endpoint that stopped draining audio for a moment can catch
//! up on the next chunk, and dropping it would end its playback. A control
//! subscriber that is [`CONTROL_QUEUE_LIMIT`] whole state snapshots behind is
//! not behind, it is gone: every one of those snapshots superseded the one
//! before it, so what it would eventually read is a history nobody wants, and
//! meanwhile it is holding a connection slot that a browser which IS reading
//! could have. So it is dropped, what it never received is counted, and the
//! count is reported. `docs/decisions/0017-the-control-fanout.md` records the
//! choice.
//!
//! Neither behaviour can reach the audio path. This fanout holds no lock the
//! audio path takes, allocates nothing the audio path allocates, and the
//! threads that drain it are not the threads that carry audio.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};

/// How many state messages may be queued for one control subscriber before it
/// is dropped.
///
/// Each message is a complete snapshot, so a subscriber holding this many is
/// holding thirty-one snapshots nobody will ever want and one they might. A
/// state message for a house of a dozen zones is a few kilobytes, so the whole
/// ceiling is on the order of a hundred kilobytes per stalled subscriber, and
/// the bound exists so that a browser tab left open on a suspended laptop
/// cannot make the server's memory a function of how long it was suspended.
/// `docs/decisions/0017-the-control-fanout.md` records why 32 rather than 4 or
/// 4096.
pub const CONTROL_QUEUE_LIMIT: usize = 32;

/// Every control subscriber attached right now.
#[derive(Debug, Default)]
pub struct ControlFanout {
    subscribers: Mutex<Vec<SyncSender<Arc<String>>>>,
    dropped_messages: AtomicU64,
    dropped_subscribers: AtomicU64,
}

impl ControlFanout {
    /// A fanout with nobody attached.
    pub fn new() -> ControlFanout {
        ControlFanout::default()
    }

    /// Attach, and receive every state message sent from now on.
    pub fn subscribe(&self) -> Receiver<Arc<String>> {
        let (tx, rx) = mpsc::sync_channel(CONTROL_QUEUE_LIMIT);
        self.lock().push(tx);
        rx
    }

    /// How many subscribers are attached.
    pub fn subscribers(&self) -> usize {
        self.lock().len()
    }

    /// How many messages were never delivered because their subscriber was at
    /// its ceiling or had gone.
    pub fn dropped_messages(&self) -> u64 {
        self.dropped_messages.load(Ordering::Relaxed)
    }

    /// How many subscribers were dropped for being at their ceiling.
    pub fn dropped_subscribers(&self) -> u64 {
        self.dropped_subscribers.load(Ordering::Relaxed)
    }

    /// The line a run prints about what this fanout did.
    pub fn report(&self) -> String {
        format!(
            "control-fanout subscribers={} queue_limit={} dropped_subscribers={} \
             dropped_messages={}",
            self.subscribers(),
            CONTROL_QUEUE_LIMIT,
            self.dropped_subscribers(),
            self.dropped_messages()
        )
    }

    /// Send one state message to every attached subscriber.
    ///
    /// Returns how many subscribers are still attached afterwards. Nothing
    /// here blocks: a subscriber at its ceiling is removed inside this call and
    /// the next subscriber is served immediately, so one stalled socket cannot
    /// delay another's fanout.
    pub fn broadcast(&self, message: Arc<String>) -> usize {
        let mut subscribers = self.lock();
        let mut dropped_here = 0u64;
        let mut messages_lost = 0u64;
        subscribers.retain(|tx| match tx.try_send(Arc::clone(&message)) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                // The queue it is holding is what it never read, plus this one.
                dropped_here += 1;
                messages_lost += CONTROL_QUEUE_LIMIT as u64 + 1;
                false
            }
            Err(TrySendError::Disconnected(_)) => {
                messages_lost += 1;
                false
            }
        });
        if dropped_here > 0 {
            self.dropped_subscribers
                .fetch_add(dropped_here, Ordering::Relaxed);
        }
        if messages_lost > 0 {
            self.dropped_messages
                .fetch_add(messages_lost, Ordering::Relaxed);
        }
        subscribers.len()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<SyncSender<Arc<String>>>> {
        match self.subscribers.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_broadcast_reaches_everybody_attached() {
        let fanout = ControlFanout::new();
        let a = fanout.subscribe();
        let b = fanout.subscribe();
        assert_eq!(fanout.broadcast(Arc::new("one".to_string())), 2);
        assert_eq!(a.recv().unwrap().as_str(), "one");
        assert_eq!(b.recv().unwrap().as_str(), "one");
    }

    #[test]
    fn a_subscriber_that_stops_reading_is_dropped_at_the_ceiling_and_counted() {
        let fanout = ControlFanout::new();
        let reading = fanout.subscribe();
        let _stalled = fanout.subscribe();

        // Fill the stalled one exactly to its ceiling. Everything so far
        // reaches both.
        for _ in 0..CONTROL_QUEUE_LIMIT {
            assert_eq!(fanout.broadcast(Arc::new("s".to_string())), 2);
            assert!(reading.recv().is_ok());
        }
        assert_eq!(fanout.dropped_subscribers(), 0, "not yet: it is AT the ceiling");

        // One more, and it goes.
        assert_eq!(fanout.broadcast(Arc::new("s".to_string())), 1);
        assert_eq!(fanout.dropped_subscribers(), 1);
        assert_eq!(fanout.dropped_messages(), CONTROL_QUEUE_LIMIT as u64 + 1);
        assert!(
            reading.recv().is_ok(),
            "the subscriber that was reading is not delayed and misses nothing"
        );
        assert_eq!(fanout.subscribers(), 1);
    }

    #[test]
    fn a_subscriber_that_disconnected_is_dropped_without_being_counted_as_stalled() {
        let fanout = ControlFanout::new();
        let a = fanout.subscribe();
        let b = fanout.subscribe();
        drop(b);
        assert_eq!(fanout.broadcast(Arc::new("x".to_string())), 1);
        assert_eq!(fanout.dropped_subscribers(), 0, "it left; it did not stall");
        assert_eq!(fanout.dropped_messages(), 1);
        assert!(a.recv().is_ok());
    }
}
