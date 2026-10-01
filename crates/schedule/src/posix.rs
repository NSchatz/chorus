//! The POSIX TZ string: `EST5EDT,M3.2.0,M11.1.0`.
//!
//! It is the footer of a TZif file of version 2 or later, and what a zone does
//! for every instant after the file's last transition (RFC 9636 section 3.3).
//! It is also what a `TZ` environment value may hold directly, which is why
//! [`crate::zone::Zone::from_posix`] accepts one.
//!
//! The grammar, from POSIX.1-2024 XBD section 8.3 as RFC 9636 section 3.3
//! restates and extends it (<https://www.rfc-editor.org/rfc/rfc9636.html>,
//! read 2026-10-01):
//!
//! ```text
//! std offset [dst [offset] [,start[/time],end[/time]]]
//! ```
//!
//! - a name is three or more letters, or `<` three or more of letters,
//!   digits, `+` and `-` `>`;
//! - an offset is `[+-]hh[:mm[:ss]]`, hours 0 to 24, and is WEST of UTC
//!   (`EST5` is UTC-5), the opposite sign of a TZif `utoff`;
//! - the dst offset defaults to one hour east of the std offset;
//! - a date is `Jn` (1 to 365, February 29 never counted), `n` (0 to 365,
//!   counted) or `Mm.w.d` (month, week 1 to 5 where 5 is "the last", weekday
//!   0 Sunday to 6);
//! - a time is `[+-]hh[:mm[:ss]]`, default `02:00:00`, local time in the
//!   offset in effect before the change. Version 3 widens its hours to -167
//!   to 167 (`M3.4.4/26`, `M3.5.0/-1`); this reader accepts that range
//!   whatever the file's version, a strict superset that refuses nothing a
//!   version 2 file can hold.
//!
//! One deliberate refusal: a dst name with no rule. POSIX leaves the rule
//! implementation-defined then (glibc reads a `posixrules` file), every TZif
//! footer zic writes carries one, and a schedule built on a guessed rule is a
//! wrong alarm twice a year. So it is an error, by name.

use std::fmt;

use crate::civil::{
    civil_from_days, days_from_civil, days_in_month, is_leap_year, SECONDS_PER_DAY,
};

/// Why a TZ string was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosixTzError {
    /// The string.
    pub tz: String,
    /// What was wrong, in words.
    pub reason: &'static str,
}

impl fmt::Display for PosixTzError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TZ string {:?}: {}", self.tz, self.reason)
    }
}

impl std::error::Error for PosixTzError {}

/// Which day of the year a rule names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleDate {
    /// `Jn`: day 1 to 365, February 29 never counted.
    Julian1(u16),
    /// `n`: day 0 to 365, February 29 counted.
    Julian0(u16),
    /// `Mm.w.d`: weekday `d` (0 Sunday) of week `w` (5 = last) of month `m`.
    MonthWeekDay {
        /// 1 to 12.
        month: u8,
        /// 1 to 5.
        week: u8,
        /// 0 (Sunday) to 6.
        weekday: u8,
    },
}

impl RuleDate {
    /// The day number this date falls on in `year`.
    pub fn day_in(self, year: i64) -> i64 {
        let jan1 = days_from_civil(year, 1, 1);
        match self {
            RuleDate::Julian1(n) => {
                let n = i64::from(n);
                jan1 + n - 1 + i64::from(is_leap_year(year) && n >= 60)
            }
            RuleDate::Julian0(n) => jan1 + i64::from(n),
            RuleDate::MonthWeekDay {
                month,
                week,
                weekday,
            } => {
                let first = days_from_civil(year, u32::from(month), 1);
                // Sunday-based weekday of the first: day 0 was a Thursday (4).
                let wd_first = (first + 4).rem_euclid(7);
                let mut day =
                    (i64::from(weekday) - wd_first).rem_euclid(7) + 7 * (i64::from(week) - 1);
                let dim = i64::from(days_in_month(year, u32::from(month)));
                while day >= dim {
                    day -= 7;
                }
                first + day
            }
        }
    }
}

/// One end of the daylight-saving period.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rule {
    /// The day.
    pub date: RuleDate,
    /// Seconds after that day's local midnight, in the offset in effect before
    /// the change; -167 h to 167 h.
    pub time_s: i32,
}

/// The daylight-saving half of a TZ string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dst {
    /// The abbreviation while it is in effect.
    pub abbreviation: String,
    /// Seconds east of UTC while it is in effect.
    pub utc_offset_s: i32,
    /// When it starts each year.
    pub start: Rule,
    /// When it ends each year.
    pub end: Rule,
}

/// A parsed POSIX TZ string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosixTz {
    /// The standard-time abbreviation.
    pub std_abbreviation: String,
    /// Standard time, seconds east of UTC.
    pub std_utc_offset_s: i32,
    /// Daylight-saving time, if the zone has it.
    pub dst: Option<Dst>,
}

/// The offset facts in effect at one instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OffsetAt<'a> {
    /// Seconds east of UTC.
    pub utc_offset_s: i32,
    /// Whether it is daylight-saving time.
    pub dst: bool,
    /// The abbreviation.
    pub abbreviation: &'a str,
}

struct Cursor<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> Cursor<'a> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }
    fn eat(&mut self, c: u8) -> bool {
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn number(&mut self, max_digits: usize) -> Option<u32> {
        let start = self.i;
        let mut v: u32 = 0;
        while self.i - start < max_digits {
            match self.peek() {
                Some(c) if c.is_ascii_digit() => {
                    v = v * 10 + u32::from(c - b'0');
                    self.i += 1;
                }
                _ => break,
            }
        }
        (self.i > start).then_some(v)
    }
    fn name(&mut self) -> Option<String> {
        let start = self.i;
        if self.eat(b'<') {
            while let Some(c) = self.peek() {
                if c.is_ascii_alphanumeric() || c == b'+' || c == b'-' {
                    self.i += 1;
                } else {
                    break;
                }
            }
            let name = &self.s[start + 1..self.i];
            if !self.eat(b'>') || name.len() < 3 {
                return None;
            }
            return Some(String::from_utf8_lossy(name).into_owned());
        }
        while matches!(self.peek(), Some(c) if c.is_ascii_alphabetic()) {
            self.i += 1;
        }
        (self.i - start >= 3).then(|| String::from_utf8_lossy(&self.s[start..self.i]).into_owned())
    }
    /// `[+-]hh[:mm[:ss]]` as signed seconds, hours at most `max_hours`.
    fn hms(&mut self, max_hours: u32) -> Option<i32> {
        let neg = if self.eat(b'-') {
            true
        } else {
            self.eat(b'+');
            false
        };
        let h = self.number(3)?;
        let mut m = 0;
        let mut s = 0;
        if self.eat(b':') {
            m = self.number(2)?;
            if self.eat(b':') {
                s = self.number(2)?;
            }
        }
        if h > max_hours || m > 59 || s > 59 {
            return None;
        }
        let v = (h * 3600 + m * 60 + s) as i32;
        Some(if neg { -v } else { v })
    }
    fn rule(&mut self) -> Option<Rule> {
        let date = if self.eat(b'J') {
            let n = self.number(3)?;
            (1..=365)
                .contains(&n)
                .then_some(RuleDate::Julian1(n as u16))?
        } else if self.eat(b'M') {
            let month = self.number(2)?;
            let week = self.eat(b'.').then(|| self.number(1)).flatten()?;
            let weekday = self.eat(b'.').then(|| self.number(1)).flatten()?;
            if !(1..=12).contains(&month) || !(1..=5).contains(&week) || weekday > 6 {
                return None;
            }
            RuleDate::MonthWeekDay {
                month: month as u8,
                week: week as u8,
                weekday: weekday as u8,
            }
        } else {
            let n = self.number(3)?;
            (n <= 365).then_some(RuleDate::Julian0(n as u16))?
        };
        let time_s = if self.eat(b'/') { self.hms(167)? } else { 7200 };
        Some(Rule { date, time_s })
    }
}

impl PosixTz {
    /// Parse a TZ string (without the leading `:` a `TZ` value may carry).
    pub fn parse(tz: &str) -> Result<PosixTz, PosixTzError> {
        let err = |reason| PosixTzError {
            tz: tz.to_string(),
            reason,
        };
        let mut c = Cursor {
            s: tz.as_bytes(),
            i: 0,
        };
        let std_abbreviation = c.name().ok_or_else(|| err("no standard-time name"))?;
        let std_west = c.hms(24).ok_or_else(|| err("no standard-time offset"))?;
        let std_utc_offset_s = -std_west;
        if c.peek().is_none() {
            return Ok(PosixTz {
                std_abbreviation,
                std_utc_offset_s,
                dst: None,
            });
        }
        let dst_abbreviation = c.name().ok_or_else(|| err("a bad daylight-saving name"))?;
        let dst_utc_offset_s = if matches!(c.peek(), Some(b'+' | b'-' | b'0'..=b'9')) {
            -c.hms(24)
                .ok_or_else(|| err("a bad daylight-saving offset"))?
        } else {
            std_utc_offset_s + 3600
        };
        if !c.eat(b',') {
            return Err(err(
                "a daylight-saving name with no rule; the rule would be a guess",
            ));
        }
        let start = c.rule().ok_or_else(|| err("a bad start rule"))?;
        if !c.eat(b',') {
            return Err(err("no end rule"));
        }
        let end = c.rule().ok_or_else(|| err("a bad end rule"))?;
        if c.peek().is_some() {
            return Err(err("trailing characters"));
        }
        Ok(PosixTz {
            std_abbreviation,
            std_utc_offset_s,
            dst: Some(Dst {
                abbreviation: dst_abbreviation,
                utc_offset_s: dst_utc_offset_s,
                start,
                end,
            }),
        })
    }

    /// The year's two changes as UTC instants, each with whether daylight
    /// saving is in effect after it: `[(start, true), (end, false)]`.
    fn changes_in(dst: &Dst, std_off: i32, year: i64) -> [(i64, bool); 2] {
        let at = |rule: &Rule, before_off: i32| {
            rule.date.day_in(year) * SECONDS_PER_DAY + i64::from(rule.time_s)
                - i64::from(before_off)
        };
        [
            (at(&dst.start, std_off), true),
            (at(&dst.end, dst.utc_offset_s), false),
        ]
    }

    /// Every change of the years around `t`, sorted by instant; at equal
    /// instants the end sorts before the start, so a zone that is on daylight
    /// saving all year (`EST5EDT,0/0,J365/25`, RFC 9636 section 3.3.1) stays on
    /// it across the year boundary.
    fn changes_around(dst: &Dst, std_off: i32, t: i64, years_after: i64) -> Vec<(i64, bool)> {
        let (year, _, _) = civil_from_days(t.div_euclid(SECONDS_PER_DAY));
        let mut v: Vec<(i64, bool)> = (year - 1..=year + years_after)
            .flat_map(|y| Self::changes_in(dst, std_off, y))
            .collect();
        v.sort_by_key(|&(at, dst_after)| (at, dst_after));
        v
    }

    fn is_dst_at(&self, t: i64) -> bool {
        let Some(dst) = &self.dst else {
            return false;
        };
        Self::changes_around(dst, self.std_utc_offset_s, t, 1)
            .iter()
            .take_while(|(at, _)| *at <= t)
            .last()
            .map(|&(_, d)| d)
            .unwrap_or(false)
    }

    /// The offset in effect at UTC instant `t`.
    pub fn offset_at(&self, t: i64) -> OffsetAt<'_> {
        match &self.dst {
            Some(dst) if self.is_dst_at(t) => OffsetAt {
                utc_offset_s: dst.utc_offset_s,
                dst: true,
                abbreviation: &dst.abbreviation,
            },
            _ => OffsetAt {
                utc_offset_s: self.std_utc_offset_s,
                dst: false,
                abbreviation: &self.std_abbreviation,
            },
        }
    }

    /// The first instant after `t` at which the offset in effect changes, or
    /// `None` if it never does (no daylight saving, or all of it).
    pub fn next_transition_after(&self, t: i64) -> Option<i64> {
        let dst = self.dst.as_ref()?;
        Self::changes_around(dst, self.std_utc_offset_s, t, 2)
            .into_iter()
            .map(|(at, _)| at)
            .filter(|&at| at > t)
            .find(|&at| self.is_dst_at(at) != self.is_dst_at(at - 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(y: i64, mo: u32, d: u32, h: i64, mi: i64) -> i64 {
        days_from_civil(y, mo, d) * SECONDS_PER_DAY + h * 3600 + mi * 60
    }

    #[test]
    fn new_york() {
        let tz = PosixTz::parse("EST5EDT,M3.2.0,M11.1.0").unwrap();
        assert_eq!(tz.std_utc_offset_s, -18_000);
        assert_eq!(tz.dst.as_ref().unwrap().utc_offset_s, -14_400);
        // 2026-03-08 02:00 EST is 07:00 UTC; 2026-11-01 02:00 EDT is 06:00 UTC.
        let start = utc(2026, 3, 8, 7, 0);
        let end = utc(2026, 11, 1, 6, 0);
        assert!(!tz.offset_at(start - 1).dst);
        assert!(tz.offset_at(start).dst);
        assert!(tz.offset_at(end - 1).dst);
        assert!(!tz.offset_at(end).dst);
        assert_eq!(tz.next_transition_after(utc(2026, 1, 1, 0, 0)), Some(start));
        assert_eq!(tz.next_transition_after(start), Some(end));
    }

    #[test]
    fn southern_hemisphere_and_half_hours() {
        let tz = PosixTz::parse("<+1030>-10:30<+11>-11,M10.1.0,M4.1.0").unwrap();
        assert_eq!(tz.std_abbreviation, "+1030");
        assert_eq!(tz.std_utc_offset_s, 37_800);
        // January is summer: daylight saving.
        assert!(tz.offset_at(utc(2026, 1, 15, 0, 0)).dst);
        assert!(!tz.offset_at(utc(2026, 7, 15, 0, 0)).dst);
    }

    #[test]
    fn version_3_hours() {
        let jerusalem = PosixTz::parse("IST-2IDT,M3.4.4/26,M10.5.0").unwrap();
        assert_eq!(jerusalem.dst.as_ref().unwrap().start.time_s, 26 * 3600);
        let nuuk = PosixTz::parse("<-02>2<-01>,M3.5.0/-1,M10.5.0/0").unwrap();
        assert_eq!(nuuk.dst.as_ref().unwrap().start.time_s, -3600);
        // All year daylight saving: never a transition.
        let always = PosixTz::parse("EST5EDT,0/0,J365/25").unwrap();
        for t in [
            utc(2026, 1, 1, 4, 59),
            utc(2026, 1, 1, 5, 0),
            utc(2026, 7, 1, 0, 0),
            utc(2026, 12, 31, 23, 0),
        ] {
            assert!(always.offset_at(t).dst);
        }
        assert_eq!(always.next_transition_after(utc(2026, 6, 1, 0, 0)), None);
    }

    #[test]
    fn julian_days() {
        // J60 is 1 March in every year; 59 (zero-based) is 29 February in a
        // leap year.
        assert_eq!(
            RuleDate::Julian1(60).day_in(2028),
            days_from_civil(2028, 3, 1)
        );
        assert_eq!(
            RuleDate::Julian1(60).day_in(2027),
            days_from_civil(2027, 3, 1)
        );
        assert_eq!(
            RuleDate::Julian0(59).day_in(2028),
            days_from_civil(2028, 2, 29)
        );
        // M2.5.0: the last Sunday of February 2026 is the 22nd.
        let last = RuleDate::MonthWeekDay {
            month: 2,
            week: 5,
            weekday: 0,
        };
        assert_eq!(last.day_in(2026), days_from_civil(2026, 2, 22));
    }

    #[test]
    fn refusals() {
        for bad in [
            "",
            "E5",
            "EST",
            "EST25",
            "EST5EDT",
            "EST5EDT,M3.2.0",
            "EST5EDT,M13.2.0,M11.1.0",
            "EST5EDT,M3.6.0,M11.1.0",
            "EST5EDT,M3.2.7,M11.1.0",
            "EST5EDT,J0,J365",
            "EST5EDT,M3.2.0/168,M11.1.0",
            "EST5EDT,M3.2.0,M11.1.0x",
            "<AB>5",
        ] {
            assert!(PosixTz::parse(bad).is_err(), "{bad}");
        }
        assert!(PosixTz::parse("UTC0").unwrap().dst.is_none());
    }
}
