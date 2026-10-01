//! Sleep timers: "stop in 30 minutes", and the fade that ends it.
//!
//! A sleep timer is a **monotonic** duration from the instant it was set, in
//! nanoseconds on the server's monotonic clock: "in thirty minutes" means
//! thirty minutes of elapsed time, whatever the wall clock does meanwhile, so
//! this is the one schedule here that never touches civil time.
//!
//! # The plan
//!
//! The timer **expires** at `start + minutes`. The music is silent by then:
//! over the [`SLEEP_FADE_S`] seconds before expiry (or from the start, for a
//! timer shorter than that), the room fades linearly in amplitude from the
//! volume it had when the fade began down to 0; at expiry the stream stops and
//! the room's volume is **restored** to that pre-fade volume, so the next
//! thing played starts at the level the person chose rather than at silence.
//!
//! The server samples [`SleepTimer::step`] with the monotonic now and the
//! volume it captured when the fade began (it captures it the first time
//! `step` returns [`SleepStep::Fading`]). Minutes 0 cancels, which is why
//! [`SleepTimer::new`] returns `None` for it.

use crate::ramp::Ramp;

/// The fade-out length, seconds. ASSUMED: thirty seconds; long enough not to
/// wake a sleeper with a cut, short enough to be done when promised. Nothing
/// chorus measured.
pub const SLEEP_FADE_S: u64 = 30;

/// The longest sleep timer, in minutes (a day). ASSUMED: a bound against
/// overflow, not a usability claim.
pub const MAX_SLEEP_MIN: u32 = 24 * 60;

const NS_PER_S: u64 = 1_000_000_000;

/// A running sleep timer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SleepTimer {
    /// When it was set, monotonic nanoseconds.
    pub start_ns: u64,
    /// How long it runs, nanoseconds.
    pub duration_ns: u64,
}

/// What a sleep timer wants done at one monotonic instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SleepStep {
    /// Keep playing; the fade begins in `fade_in_ns`.
    Waiting {
        /// Nanoseconds until the fade begins.
        fade_in_ns: u64,
    },
    /// Fading: apply `volume` (thousandths).
    Fading {
        /// The volume to apply now.
        volume: u16,
    },
    /// Expired: stop the stream and set the room's volume back to `restore`.
    Stop {
        /// The volume the fade began from.
        restore: u16,
    },
}

impl SleepTimer {
    /// A timer of `minutes` from monotonic instant `start_ns`; `None` for 0
    /// (cancel). Minutes above [`MAX_SLEEP_MIN`] are clamped to it.
    pub fn new(start_ns: u64, minutes: u32) -> Option<SleepTimer> {
        (minutes > 0).then(|| SleepTimer {
            start_ns,
            duration_ns: u64::from(minutes.min(MAX_SLEEP_MIN)) * 60 * NS_PER_S,
        })
    }

    /// When it expires, monotonic nanoseconds.
    pub fn expiry_ns(&self) -> u64 {
        self.start_ns.saturating_add(self.duration_ns)
    }

    /// When the fade begins: [`SLEEP_FADE_S`] before expiry, or at the start
    /// if the timer is shorter.
    pub fn fade_start_ns(&self) -> u64 {
        self.expiry_ns()
            .saturating_sub(SLEEP_FADE_S * NS_PER_S)
            .max(self.start_ns)
    }

    /// The fade from `volume_at_fade_start` to 0.
    pub fn fade(&self, volume_at_fade_start: u16) -> Ramp {
        Ramp {
            from: volume_at_fade_start,
            to: 0,
            duration_ns: self.expiry_ns() - self.fade_start_ns(),
        }
    }

    /// What to do at monotonic instant `now_ns`, given the volume the room
    /// had when the fade began (any value while still waiting).
    pub fn step(&self, now_ns: u64, volume_at_fade_start: u16) -> SleepStep {
        if now_ns >= self.expiry_ns() {
            SleepStep::Stop {
                restore: volume_at_fade_start,
            }
        } else if now_ns < self.fade_start_ns() {
            SleepStep::Waiting {
                fade_in_ns: self.fade_start_ns() - now_ns,
            }
        } else {
            SleepStep::Fading {
                volume: self
                    .fade(volume_at_fade_start)
                    .sample(now_ns - self.fade_start_ns()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plan() {
        let t = SleepTimer::new(5 * NS_PER_S, 30).unwrap();
        assert_eq!(t.expiry_ns(), 5 * NS_PER_S + 1800 * NS_PER_S);
        assert_eq!(t.fade_start_ns(), t.expiry_ns() - 30 * NS_PER_S);
        assert_eq!(
            t.step(5 * NS_PER_S, 0),
            SleepStep::Waiting {
                fade_in_ns: 1770 * NS_PER_S
            }
        );
        let f = t.fade_start_ns();
        assert_eq!(t.step(f, 600), SleepStep::Fading { volume: 600 });
        assert_eq!(
            t.step(f + 15 * NS_PER_S, 600),
            SleepStep::Fading { volume: 300 }
        );
        // Truncation toward `from`: one nanosecond before expiry is still 1.
        assert_eq!(
            t.step(t.expiry_ns() - 1, 600),
            SleepStep::Fading { volume: 1 }
        );
        assert_eq!(t.step(t.expiry_ns(), 600), SleepStep::Stop { restore: 600 });
        assert_eq!(SleepTimer::new(0, 0), None);
        assert_eq!(
            SleepTimer::new(0, u32::MAX).unwrap().duration_ns,
            1440 * 60 * NS_PER_S
        );
    }

    #[test]
    fn the_fade_is_monotone_to_silence() {
        let t = SleepTimer::new(0, 1).unwrap();
        let mut last = u16::MAX;
        for k in 0..=3000u64 {
            let now = t.fade_start_ns() + k * 10_000_000;
            let v = match t.step(now, 1000) {
                SleepStep::Fading { volume } => volume,
                SleepStep::Stop { .. } => 0,
                SleepStep::Waiting { .. } => panic!("waiting after the fade began"),
            };
            assert!(v <= last);
            last = v;
        }
        assert_eq!(last, 0);
    }
}
