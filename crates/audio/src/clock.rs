//! The one timeline the server stamps on, and the one the client measures on.
//!
//! # Why this type exists at all
//!
//! `std::time::Instant` is already monotonic on Linux, and that is exactly the
//! property this project's fourth guardrail is about. What `Instant` will not
//! do is hand out a number, and the wire format needs one: `docs/protocol.md`
//! says every timestamp is "nanoseconds from a monotonic source on the device
//! that took it, never wall clock". So this is an `Instant` captured once, plus
//! a subtraction.
//!
//! # Why the epoch is process start and not something more meaningful
//!
//! It cannot be anything more meaningful without a settable clock. The two
//! endpoints' epochs are unrelated by design - `docs/protocol.md` says so, and
//! the whole time-sync exchange exists because of it - so nothing is lost by
//! making the server's epoch its own start. A number with a shared meaning
//! would have to come from `CLOCK_REALTIME`, which is the thing the guardrail
//! forbids, and which `timens7` confirms is the one clock a container's time
//! namespace does not protect from being stepped.
//!
//! # What this deliberately is not
//!
//! It is not a clock that can be corrected, disciplined, offset or resynced.
//! That is SYNC-4's subject. Here, time only goes forward at the rate the
//! kernel says.

use std::time::Instant;

/// A monotonic timeline with its own epoch.
///
/// Cheap to clone; a clone shares the same epoch, which is what makes it "one
/// timeline" across the threads of one process.
#[derive(Debug, Clone, Copy)]
pub struct MonotonicTimeline {
    epoch: Instant,
}

impl Default for MonotonicTimeline {
    fn default() -> Self {
        MonotonicTimeline::new()
    }
}

impl MonotonicTimeline {
    /// Start a timeline now.
    pub fn new() -> MonotonicTimeline {
        MonotonicTimeline {
            epoch: Instant::now(),
        }
    }

    /// Nanoseconds since this timeline's epoch.
    ///
    /// Saturates rather than wrapping at the far end of a `u64`, which is
    /// roughly 584 years after the epoch and therefore not a real case; the
    /// saturation is here so that no arithmetic in the audio path can wrap
    /// silently.
    pub fn now_ns(&self) -> u64 {
        let elapsed = self.epoch.elapsed().as_nanos();
        if elapsed > u128::from(u64::MAX) {
            u64::MAX
        } else {
            elapsed as u64
        }
    }

    /// Microseconds since this timeline's epoch.
    pub fn now_us(&self) -> u64 {
        self.now_ns() / 1_000
    }

    /// The `Instant` this timeline started at, for callers that want to sleep
    /// until an absolute point on it rather than read it.
    pub fn epoch(&self) -> Instant {
        self.epoch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_timeline_never_goes_backwards() {
        let t = MonotonicTimeline::new();
        let mut last = 0u64;
        for _ in 0..1000 {
            let now = t.now_ns();
            assert!(now >= last, "{} < {}", now, last);
            last = now;
        }
    }

    #[test]
    fn a_clone_shares_the_epoch() {
        let a = MonotonicTimeline::new();
        let b = a;
        assert_eq!(a.epoch(), b.epoch());
    }
}
