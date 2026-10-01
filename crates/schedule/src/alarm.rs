//! Alarms: when the next one rings, snooze, and the query the server polls.
//!
//! # Semantics
//!
//! An alarm is a time of day, a days mask and an enabled flag.
//!
//! - **Repeating** (a non-empty mask): it rings at the next wall-clock reading
//!   of its time on one of its days, by the zone's gap and fold rules
//!   ([`crate::zone`]): a 02:30 alarm on the morning clocks skip 02:00 to
//!   03:00 rings at 03:00, and on the morning 01:30 happens twice it rings
//!   once, at the first.
//! - **One shot** (the empty mask): it rings at the next reading of its time
//!   on any day, and [`Alarm::fired`] disables it.
//! - **Snooze**: [`Alarm::snooze`] sets a ring instant `minutes` from the
//!   instant the person pressed it. Snooze is elapsed time, not a wall-clock
//!   reading: nine minutes is nine minutes across a clock change. A pending
//!   snooze rings even if a one-shot alarm was disabled by its firing;
//!   [`Alarm::stop`] and disabling clear it.
//!
//! # Polling
//!
//! The server owns the clock and the thread; it calls [`due_between`] with
//! the instant of its previous poll and the instant now, and rings what comes
//! back. The interval is half open, `(t0, t1]`, so consecutive polls neither
//! miss nor repeat an instant. An alarm is returned at most once per call, at
//! its first ring instant in the interval: a server that was down for a day
//! and polls `(yesterday, now]` gets each missed alarm once, with the instant
//! it should have rung, and decides itself whether a ring that late is still
//! wanted (the server starting with `t0 = now` rings nothing missed).

use crate::civil::{Days, TimeOfDay};
use crate::zone::Zone;

/// The snooze length when the command names none, in minutes. ASSUMED: nine
/// minutes is the common clock-radio and phone default; nothing chorus
/// measured.
pub const DEFAULT_SNOOZE_MIN: u32 = 9;

/// The longest snooze accepted, in minutes (a day). ASSUMED: a bound so a
/// hostile value cannot push an instant to overflow, not a usability claim.
pub const MAX_SNOOZE_MIN: u32 = 24 * 60;

/// An alarm's schedule and its ringing state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alarm {
    /// The wall-clock time it rings at.
    pub time: TimeOfDay,
    /// The days it repeats on; empty for a one-shot alarm.
    pub days: Days,
    /// Whether it rings at its time at all.
    pub enabled: bool,
    /// A pending snooze: the UTC instant it rings again.
    pub snoozed_until: Option<i64>,
}

/// One alarm due in a polled interval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Due<K> {
    /// The caller's key for the alarm (its id).
    pub key: K,
    /// The instant it rings (or should have rung).
    pub at: i64,
}

impl Alarm {
    /// An enabled alarm with no snooze pending.
    pub fn new(time: TimeOfDay, days: Days) -> Alarm {
        Alarm {
            time,
            days,
            enabled: true,
            snoozed_until: None,
        }
    }

    /// Whether this is a one-shot alarm (no days).
    pub fn is_one_shot(&self) -> bool {
        self.days.is_empty()
    }

    /// The first instant after `t` it rings, in `zone`; `None` when it is
    /// disabled with no snooze pending.
    pub fn next_fire_after(&self, zone: &Zone, t: i64) -> Option<i64> {
        let snooze = self.snoozed_until.filter(|&s| s > t);
        let scheduled = if self.enabled {
            let days = if self.is_one_shot() {
                Days::EVERY_DAY
            } else {
                self.days
            };
            zone.next_local_after(t, self.time, days)
        } else {
            None
        };
        match (snooze, scheduled) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// Record that it rang at `at`: a one-shot alarm disables itself, and a
    /// snooze due by `at` is spent.
    pub fn fired(&mut self, at: i64) {
        if self.snoozed_until.is_some_and(|s| s <= at) {
            self.snoozed_until = None;
        } else if self.is_one_shot() {
            self.enabled = false;
        }
    }

    /// Snooze: ring again `minutes` after `now` (clamped to 1 to
    /// [`MAX_SNOOZE_MIN`]).
    pub fn snooze(&mut self, now: i64, minutes: u32) {
        let minutes = minutes.clamp(1, MAX_SNOOZE_MIN);
        self.snoozed_until = Some(now.saturating_add(i64::from(minutes) * 60));
    }

    /// Stop it ringing: any pending snooze is cleared. The schedule is kept.
    pub fn stop(&mut self) {
        self.snoozed_until = None;
    }

    /// Enable or disable it; disabling also clears a pending snooze.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.snoozed_until = None;
        }
    }
}

/// Every alarm that rings in `(t0, t1]`, each once, at its first ring
/// instant there, sorted by instant (ties in input order).
pub fn due_between<K: Clone>(alarms: &[(K, Alarm)], zone: &Zone, t0: i64, t1: i64) -> Vec<Due<K>> {
    let mut due: Vec<Due<K>> = alarms
        .iter()
        .filter_map(|(key, alarm)| {
            let at = alarm.next_fire_after(zone, t0)?;
            (at <= t1).then(|| Due {
                key: key.clone(),
                at,
            })
        })
        .collect();
    due.sort_by_key(|d| d.at);
    due
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::civil::{days_from_civil, Weekday};

    const DAY: i64 = 86_400;

    fn thursday_0600() -> i64 {
        days_from_civil(2026, 10, 1) * DAY + 6 * 3600
    }

    #[test]
    fn one_shot_rings_once() {
        let z = Zone::utc();
        let mut a = Alarm::new(TimeOfDay::new(7, 0).unwrap(), Days::NONE);
        let t = thursday_0600();
        let at = a.next_fire_after(&z, t).unwrap();
        assert_eq!(at, t + 3600);
        a.fired(at);
        assert!(!a.enabled);
        assert_eq!(a.next_fire_after(&z, at), None);
    }

    #[test]
    fn snooze_rings_even_after_a_one_shot_fired() {
        let z = Zone::utc();
        let mut a = Alarm::new(TimeOfDay::new(7, 0).unwrap(), Days::NONE);
        let at = thursday_0600() + 3600;
        a.fired(at);
        a.snooze(at + 30, DEFAULT_SNOOZE_MIN);
        assert_eq!(a.next_fire_after(&z, at + 30), Some(at + 30 + 540));
        a.fired(at + 30 + 540);
        assert_eq!(a.snoozed_until, None);
        assert_eq!(a.next_fire_after(&z, at + 600), None);
        // Stop clears a snooze.
        let mut b = Alarm::new(TimeOfDay::new(7, 0).unwrap(), Days::EVERY_DAY);
        b.snooze(at, 5);
        b.stop();
        assert_eq!(b.next_fire_after(&z, at), Some(at + DAY));
        b.snooze(at, 0);
        assert_eq!(b.snoozed_until, Some(at + 60));
        b.set_enabled(false);
        assert_eq!(b.next_fire_after(&z, at), None);
    }

    #[test]
    fn due_between_is_half_open_and_once_per_alarm() {
        let z = Zone::utc();
        let t = thursday_0600();
        let alarms = vec![
            (
                "weekdays",
                Alarm::new(TimeOfDay::new(7, 0).unwrap(), Days::WEEKDAYS),
            ),
            (
                "sat",
                Alarm::new(
                    TimeOfDay::new(7, 0).unwrap(),
                    Days::NONE.with(Weekday::Saturday),
                ),
            ),
            (
                "early",
                Alarm::new(TimeOfDay::new(6, 30).unwrap(), Days::EVERY_DAY),
            ),
        ];
        let due = due_between(&alarms, &z, t, t + 3600);
        assert_eq!(
            due,
            vec![
                Due {
                    key: "early",
                    at: t + 1800
                },
                Due {
                    key: "weekdays",
                    at: t + 3600
                }
            ]
        );
        // The next poll starts at the previous one's end: nothing repeats.
        assert!(due_between(&alarms, &z, t + 3600, t + 7200).is_empty());
        // A week-long gap returns each alarm once, at its first instant.
        let late = due_between(&alarms, &z, t, t + 7 * DAY);
        assert_eq!(late.len(), 3);
        assert_eq!(
            late[2],
            Due {
                key: "sat",
                at: t + 2 * DAY + 3600
            }
        );
    }
}
