//! Weekly windows: "22:00 to 07:00 on school nights", the shape of a room's
//! quiet hours.
//!
//! # Semantics
//!
//! A window is a days mask and a start and end time of day. On each day in
//! the mask, an **instance** of the window opens when the wall clock reads
//! `start` and lasts until it next reads `end`:
//!
//! - `start < end`: the instance is inside that day (`09:00-17:00`).
//! - `end < start`: it crosses midnight and ends the next morning; the mask
//!   names the day it **starts** on, so `fri 22:00-07:00` covers Friday night
//!   into Saturday morning and not Thursday night.
//! - `start == end`: **a full 24 hours** from `start` (decided here: an
//!   instance is never empty, so `00:00-00:00` on `sat,sun` is the whole
//!   weekend). A window that is never active is the empty mask.
//!
//! The window is a statement about wall-clock readings, not elapsed time: it
//! opens and closes when the local clock reads the boundary, so on the night
//! clocks change, `22:00-07:00` is eight or ten hours long and still ends at
//! 07:00. A boundary that a gap skips takes effect at the transition (the
//! clock jumped past it), and in a fold the window follows the clock both
//! times round: an instance ending at 01:30 in a fold ends at the first 01:30
//! and is open again for the repeated hour's 01:00 to 01:30 if the instance
//! covers it. [`WeeklyWindow::contains`] is exactly "does the wall clock's
//! reading fall in an instance", and [`WeeklyWindow::next_boundary_after`] is
//! the first instant that answer changes.
//!
//! Instants are UTC seconds since the Unix epoch; no clock is read.

use crate::civil::{Civil, Days, TimeOfDay, Weekday, SECONDS_PER_DAY};
use crate::zone::Zone;

/// A weekly window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeeklyWindow {
    /// The days an instance starts on.
    pub days: Days,
    /// When an instance opens.
    pub start: TimeOfDay,
    /// When it closes (see the module docs for `end <= start`).
    pub end: TimeOfDay,
}

/// How far ahead [`WeeklyWindow::next_boundary_after`] looks: a week and a
/// bit, enough to see every boundary of a weekly pattern once.
const HORIZON_DAYS: i64 = 9;

impl WeeklyWindow {
    /// A window.
    pub fn new(days: Days, start: TimeOfDay, end: TimeOfDay) -> WeeklyWindow {
        WeeklyWindow { days, start, end }
    }

    /// An instance's length in seconds of wall-clock reading, 60 to 86 400.
    fn length_s(&self) -> i64 {
        let d = (self.end.seconds() - self.start.seconds()).rem_euclid(SECONDS_PER_DAY);
        if d == 0 {
            SECONDS_PER_DAY
        } else {
            d
        }
    }

    /// Whether a wall-clock reading is inside an instance.
    pub fn contains(&self, local: &Civil) -> bool {
        let sod = local.second_of_day();
        let len = self.length_s();
        let start = self.start.seconds();
        // An instance that started today, or one that started yesterday and
        // runs past midnight.
        let today = local.weekday;
        let yesterday = Weekday::of_day(local.day_number() - 1);
        (self.days.contains(today) && sod >= start && sod - start < len)
            || (self.days.contains(yesterday) && sod + SECONDS_PER_DAY - start < len)
    }

    /// Whether the wall clock in `zone` at UTC instant `t` is inside an
    /// instance.
    pub fn contains_at(&self, zone: &Zone, t: i64) -> bool {
        self.contains(&zone.to_civil(t))
    }

    /// The first instant after `t` at which [`WeeklyWindow::contains_at`]
    /// changes value, or `None` if it does not change within the next nine
    /// days (an empty mask, or a window that covers every day all day).
    ///
    /// The answer can change only where the wall clock reads a boundary or
    /// jumps, so the candidates are every reading of `start` and `end` (both
    /// readings in a fold) and every gap resolution and zone transition in
    /// the horizon; the first candidate at which the answer differs from the
    /// answer at `t` is the boundary.
    pub fn next_boundary_after(&self, zone: &Zone, t: i64) -> Option<i64> {
        if self.days.is_empty() {
            return None;
        }
        let now = self.contains_at(zone, t);
        let today = zone.to_civil(t).day_number();
        let mut candidates = zone.transitions_between(t, t + HORIZON_DAYS * SECONDS_PER_DAY);
        for d in today - 1..=today + HORIZON_DAYS {
            for tod in [self.start, self.end] {
                let r = zone.resolve(d, tod.seconds());
                candidates.push(r.instant());
                candidates.extend(r.readings());
            }
        }
        candidates.retain(|&c| c > t);
        candidates.sort_unstable();
        candidates.dedup();
        candidates
            .into_iter()
            .find(|&c| self.contains_at(zone, c) != now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::civil::days_from_civil;

    fn at(y: i64, mo: u32, d: u32, h: i64, mi: i64) -> i64 {
        days_from_civil(y, mo, d) * SECONDS_PER_DAY + h * 3600 + mi * 60
    }

    fn tod(s: &str) -> TimeOfDay {
        TimeOfDay::parse(s).unwrap()
    }

    #[test]
    fn crossing_midnight_belongs_to_the_start_day() {
        let z = Zone::utc();
        // Friday night only. 2026-10-02 is a Friday.
        let w = WeeklyWindow::new(Days::NONE.with(Weekday::Friday), tod("22:00"), tod("07:00"));
        assert!(!w.contains_at(&z, at(2026, 10, 2, 6, 0))); // Friday 06:00: Thursday has no instance
        assert!(!w.contains_at(&z, at(2026, 10, 2, 21, 59)));
        assert!(w.contains_at(&z, at(2026, 10, 2, 22, 0)));
        assert!(w.contains_at(&z, at(2026, 10, 3, 6, 59)));
        assert!(!w.contains_at(&z, at(2026, 10, 3, 7, 0)));
        assert_eq!(
            w.next_boundary_after(&z, at(2026, 10, 1, 0, 0)),
            Some(at(2026, 10, 2, 22, 0))
        );
        assert_eq!(
            w.next_boundary_after(&z, at(2026, 10, 2, 22, 0)),
            Some(at(2026, 10, 3, 7, 0))
        );
    }

    #[test]
    fn equal_start_and_end_is_a_full_day() {
        let z = Zone::utc();
        let w = WeeklyWindow::new(Days::WEEKEND, tod("00:00"), tod("00:00"));
        assert!(w.contains_at(&z, at(2026, 10, 3, 0, 0))); // Saturday
        assert!(w.contains_at(&z, at(2026, 10, 4, 23, 59))); // Sunday
        assert!(!w.contains_at(&z, at(2026, 10, 5, 0, 0))); // Monday
                                                            // Saturday's and Sunday's instances abut: the next change after
                                                            // Saturday noon is Monday 00:00, not Sunday 00:00.
        assert_eq!(
            w.next_boundary_after(&z, at(2026, 10, 3, 12, 0)),
            Some(at(2026, 10, 5, 0, 0))
        );
        let always = WeeklyWindow::new(Days::EVERY_DAY, tod("06:00"), tod("06:00"));
        assert!(always.contains_at(&z, at(2026, 10, 3, 12, 0)));
        assert_eq!(always.next_boundary_after(&z, at(2026, 10, 3, 12, 0)), None);
        let never = WeeklyWindow::new(Days::NONE, tod("06:00"), tod("07:00"));
        assert_eq!(never.next_boundary_after(&z, 0), None);
    }
}
