//! A time zone, and the two conversions scheduling needs: a UTC instant to
//! civil time, and civil time back to the UTC instant it names.
//!
//! # The inverse is not a function, so this module defines it
//!
//! Going from a UTC instant to civil time is always one answer. Going back is
//! not, twice a year in a zone with daylight saving:
//!
//! - **A gap.** When clocks go forward, a span of local times never happens
//!   (02:00 to 02:59:59 in New York on 2026-03-08). A local time in a gap
//!   resolves to **the first valid instant after it**: the transition instant
//!   itself, where the wall clock reads the first time that does exist
//!   (03:00 EDT). So a 02:30 alarm on that morning rings at 03:00, late by the
//!   size of the jump and never skipped. A whole skipped day (Samoa,
//!   2011-12-30) is the same rule: the instant the day was jumped over.
//! - **A fold.** When clocks go back, a span of local times happens twice
//!   (01:00 to 01:59:59 in New York on 2026-11-01). A local time in a fold
//!   resolves to **its first occurrence** (01:30 EDT), and it fires **once**:
//!   [`Zone::next_local_after`] never offers the second occurrence, even when
//!   asked after the first one has passed.
//!
//! [`Zone::resolve`] reports which case a local time is in, with every
//! instant involved; [`Zone::instant_of`] applies the two rules.
//!
//! # How the inverse is computed
//!
//! On the local-second scale of [`crate::civil`], a UTC instant `u` reads local
//! time `u + offset(u)`. Over the UTC span `[L - 2 days, L + 2 days]` the zone
//! is cut into constant-offset segments at its transitions; in a segment
//! `[s, e)` with offset `o`, local time `L` is read exactly at `u = L - o` if
//! `s <= u < e`. Zero solutions is a gap, two is a fold, one is the ordinary
//! case. Two days is enough because no offset this crate accepts is more than
//! 26 hours ([`crate::tzif::UTOFF_MIN`], [`crate::tzif::UTOFF_MAX`]), so every
//! solution lies inside the span. No clock is read: the caller says which
//! instant it is.

use std::fmt;

use crate::civil::{clamp_instant, Civil, Days, TimeOfDay, Weekday, SECONDS_PER_DAY};
use crate::posix::{OffsetAt, PosixTz, PosixTzError};
use crate::tzif::{Tzif, TzifError, UTOFF_MAX, UTOFF_MIN};

/// Why a zone could not be made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZoneError {
    /// The TZif file was refused.
    Tzif(TzifError),
    /// The TZ string was refused.
    Posix(PosixTzError),
    /// A fixed offset outside -89 999 to 93 599 seconds.
    Offset(i32),
}

impl fmt::Display for ZoneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ZoneError::Tzif(e) => e.fmt(f),
            ZoneError::Posix(e) => e.fmt(f),
            ZoneError::Offset(o) => {
                write!(f, "a fixed UTC offset of {o} s is outside -89999 to 93599")
            }
        }
    }
}

impl std::error::Error for ZoneError {}

/// A time zone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Zone {
    /// A fixed offset all year: UTC itself when the offset is 0, and the
    /// fallback when no zoneinfo is configured or readable.
    Fixed {
        /// Seconds east of UTC.
        utc_offset_s: i32,
        /// `UTC`, or the offset spelled as tz does (`+0530`).
        abbreviation: String,
    },
    /// A POSIX TZ string, e.g. a `TZ` value with no file behind it.
    Posix(PosixTz),
    /// A TZif zoneinfo file.
    Tzif(Tzif),
}

/// What instant a local date and time of day names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolved {
    /// The ordinary case: exactly one instant reads that local time.
    Unique(i64),
    /// Clocks went back: two instants read it, `first < second`.
    Fold {
        /// The first occurrence, the one the fold rule uses.
        first: i64,
        /// The second occurrence.
        second: i64,
    },
    /// Clocks went forward over it: no instant reads it.
    Gap {
        /// The transition that jumped over it: the first instant whose local
        /// time is later, which the gap rule uses.
        skipped_to: i64,
    },
}

impl Resolved {
    /// The instant the gap and fold rules pick (module docs).
    pub fn instant(self) -> i64 {
        match self {
            Resolved::Unique(u) => u,
            Resolved::Fold { first, .. } => first,
            Resolved::Gap { skipped_to } => skipped_to,
        }
    }

    /// Every instant at which the wall clock reads the local time (none in a
    /// gap): what a window boundary has to consider.
    pub fn readings(self) -> Vec<i64> {
        match self {
            Resolved::Unique(u) => vec![u],
            Resolved::Fold { first, second } => vec![first, second],
            Resolved::Gap { .. } => Vec::new(),
        }
    }
}

/// How tz spells a numeric abbreviation: `+05`, `-0330`, `+054510`.
fn numeric_abbreviation(off: i32) -> String {
    let sign = if off < 0 { '-' } else { '+' };
    let a = off.unsigned_abs();
    let (h, m, s) = (a / 3600, a / 60 % 60, a % 60);
    match (m, s) {
        (0, 0) => format!("{sign}{h:02}"),
        (_, 0) => format!("{sign}{h:02}{m:02}"),
        _ => format!("{sign}{h:02}{m:02}{s:02}"),
    }
}

/// Whether `name` is a safe zoneinfo name to join under a zoneinfo
/// directory: `Area/City` made of letters, digits, `_`, `-` and `+`, no
/// empty, `.` or `..` component, not absolute. A `TZ` setting is
/// configuration, and a configured `../../etc/shadow` must be a refusal, not
/// a file read; the server checks the name with this before it reads
/// `/usr/share/zoneinfo/<name>`.
pub fn is_safe_zoneinfo_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && name.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'+' | b'.'))
        })
}

impl Zone {
    /// UTC.
    pub fn utc() -> Zone {
        Zone::Fixed {
            utc_offset_s: 0,
            abbreviation: "UTC".to_string(),
        }
    }

    /// A fixed offset, seconds east of UTC.
    pub fn fixed(utc_offset_s: i32) -> Result<Zone, ZoneError> {
        if !(UTOFF_MIN..=UTOFF_MAX).contains(&utc_offset_s) {
            return Err(ZoneError::Offset(utc_offset_s));
        }
        if utc_offset_s == 0 {
            return Ok(Zone::utc());
        }
        Ok(Zone::Fixed {
            utc_offset_s,
            abbreviation: numeric_abbreviation(utc_offset_s),
        })
    }

    /// A zone from a TZif file's bytes.
    pub fn from_tzif(bytes: &[u8]) -> Result<Zone, ZoneError> {
        Tzif::parse(bytes).map(Zone::Tzif).map_err(ZoneError::Tzif)
    }

    /// A zone from a POSIX TZ string (a leading `:` is not part of it).
    pub fn from_posix(tz: &str) -> Result<Zone, ZoneError> {
        let tz = PosixTz::parse(tz).map_err(ZoneError::Posix)?;
        for off in [
            Some(tz.std_utc_offset_s),
            tz.dst.as_ref().map(|d| d.utc_offset_s),
        ]
        .into_iter()
        .flatten()
        {
            if !(UTOFF_MIN..=UTOFF_MAX).contains(&off) {
                return Err(ZoneError::Offset(off));
            }
        }
        Ok(Zone::Posix(tz))
    }

    /// The offset in effect at UTC instant `t`.
    pub fn offset_at(&self, t: i64) -> OffsetAt<'_> {
        let t = clamp_instant(t);
        match self {
            Zone::Fixed {
                utc_offset_s,
                abbreviation,
            } => OffsetAt {
                utc_offset_s: *utc_offset_s,
                dst: false,
                abbreviation,
            },
            Zone::Posix(p) => p.offset_at(t),
            Zone::Tzif(z) => z.offset_at(t),
        }
    }

    /// Civil time at UTC instant `t` (seconds since the Unix epoch).
    pub fn to_civil(&self, t: i64) -> Civil {
        let t = clamp_instant(t);
        let o = self.offset_at(t);
        Civil::from_local_second(
            t + i64::from(o.utc_offset_s),
            o.utc_offset_s,
            o.abbreviation,
            o.dst,
        )
    }

    /// The first instant after `t` at which the zone's offset, DST flag or
    /// abbreviation may change, or `None` if it never does.
    pub fn next_transition_after(&self, t: i64) -> Option<i64> {
        let t = clamp_instant(t);
        match self {
            Zone::Fixed { .. } => None,
            Zone::Posix(p) => p.next_transition_after(t),
            Zone::Tzif(z) => z.next_transition_after(t),
        }
    }

    /// Every transition in `(a, b]`, ascending.
    pub fn transitions_between(&self, a: i64, b: i64) -> Vec<i64> {
        let mut out = Vec::new();
        let mut cur = a;
        while let Some(next) = self.next_transition_after(cur) {
            if next > b || next <= cur {
                break;
            }
            out.push(next);
            cur = next;
        }
        out
    }

    /// Which instants read local date `day` (a day number) at
    /// `second_of_day` (0 to 86 399).
    pub fn resolve(&self, day: i64, second_of_day: i64) -> Resolved {
        let local = clamp_instant(
            day.saturating_mul(SECONDS_PER_DAY)
                .saturating_add(second_of_day),
        );
        let lo = local - 2 * SECONDS_PER_DAY;
        let hi = local + 2 * SECONDS_PER_DAY;
        // Constant-offset segments covering [lo, hi): (start, offset).
        let mut starts = vec![lo];
        starts.extend(self.transitions_between(lo, hi));
        let offsets: Vec<i64> = starts
            .iter()
            .map(|&s| i64::from(self.offset_at(s).utc_offset_s))
            .collect();
        let mut solutions = Vec::new();
        for (i, (&s, &o)) in starts.iter().zip(&offsets).enumerate() {
            let e = starts.get(i + 1).copied().unwrap_or(hi);
            let u = local - o;
            if s <= u && u < e {
                solutions.push(u);
            }
        }
        match solutions[..] {
            [u] => Resolved::Unique(u),
            [first, .., second] => Resolved::Fold { first, second },
            [] => {
                // A gap: the transition T whose jump covers the local time,
                // T + before <= local < T + after.
                let skipped_to = (1..starts.len())
                    .find(|&i| {
                        starts[i] + offsets[i - 1] <= local && local < starts[i] + offsets[i]
                    })
                    .map(|i| starts[i])
                    // Unreachable for a consistent zone (local time is
                    // increasing, so a value nobody reads is inside a jump);
                    // the plain conversion is the least surprising answer.
                    .unwrap_or(local - offsets[0]);
                Resolved::Gap { skipped_to }
            }
        }
    }

    /// The instant local date `day` at `second_of_day` names, by the gap and
    /// fold rules (module docs).
    pub fn instant_of(&self, day: i64, second_of_day: i64) -> i64 {
        self.resolve(day, second_of_day).instant()
    }

    /// The first instant strictly after `t` at which the wall clock reads
    /// `time` on one of `days`, by the gap and fold rules; `None` if `days`
    /// is empty.
    ///
    /// The weekday is the weekday of the local date being read, so a Monday
    /// 02:30 that a gap skips still fires (at the transition), and a fold's
    /// second reading never fires.
    pub fn next_local_after(&self, t: i64, time: TimeOfDay, days: Days) -> Option<i64> {
        if days.is_empty() {
            return None;
        }
        let t = clamp_instant(t);
        let today = self.to_civil(t).day_number();
        // From yesterday (an offset change can put yesterday's reading after
        // t) through two weeks (a skipped day cannot hide a weekday longer
        // than that); the earliest qualifying instant wins.
        (today - 1..=today + 15)
            .filter(|&d| days.contains(Weekday::of_day(d)))
            .map(|d| self.instant_of(d, time.seconds()))
            .filter(|&u| u > t)
            .min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::civil::days_from_civil;

    #[test]
    fn fixed_offsets() {
        let z = Zone::fixed(19_800).unwrap();
        let c = z.to_civil(0);
        assert_eq!(
            (c.hour, c.minute, c.abbreviation.as_str()),
            (5, 30, "+0530")
        );
        assert_eq!(Zone::fixed(-3600).unwrap().to_civil(0).abbreviation, "-01");
        assert!(Zone::fixed(100_000).is_err());
        assert_eq!(Zone::fixed(0).unwrap(), Zone::utc());
        let d = days_from_civil(2026, 10, 1);
        assert_eq!(z.resolve(d, 0), Resolved::Unique(d * 86_400 - 19_800));
    }

    #[test]
    fn utc_next_local() {
        let z = Zone::utc();
        let t = days_from_civil(2026, 10, 1) * 86_400 + 7 * 3600; // Thursday 07:00
        let seven = TimeOfDay::new(7, 0).unwrap();
        // Strictly after: the 07:00 that is now is not next.
        assert_eq!(
            z.next_local_after(t, seven, Days::EVERY_DAY),
            Some(t + 86_400)
        );
        assert_eq!(z.next_local_after(t - 1, seven, Days::EVERY_DAY), Some(t));
        // Next Monday is 2026-10-05.
        let mon = Days::NONE.with(Weekday::Monday);
        assert_eq!(z.next_local_after(t, seven, mon), Some(t + 4 * 86_400));
        assert_eq!(z.next_local_after(t, seven, Days::NONE), None);
    }

    #[test]
    fn zoneinfo_names() {
        for ok in [
            "Europe/Berlin",
            "America/Argentina/Buenos_Aires",
            "Etc/GMT+5",
            "UTC",
            "posix/Europe/Berlin",
        ] {
            assert!(is_safe_zoneinfo_name(ok), "{ok}");
        }
        for bad in [
            "",
            "/etc/passwd",
            "../x",
            "Europe/../../x",
            "a//b",
            "a/./b",
            "Europe/Berlin\n",
            "a b",
        ] {
            assert!(!is_safe_zoneinfo_name(bad), "{bad:?}");
        }
    }
}
