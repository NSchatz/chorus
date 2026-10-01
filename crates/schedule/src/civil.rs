//! Dates, weekdays, days masks and times of day: the proleptic Gregorian
//! calendar as arithmetic on day numbers, with no time zone in it.
//!
//! A **day number** is days since 1970-01-01 (day 0, a Thursday), negative
//! before it. A **local second** is a day number times 86 400 plus the second
//! of the day: civil time written on the same scale as a UTC instant, so that
//! "the UTC instant at which local time reads L" is `L - offset` wherever the
//! offset is constant. Leap seconds do not exist on either scale (POSIX time).
//!
//! The day-number conversions are the well-known era decomposition (400-year
//! cycles of 146 097 days), written here from its description in
//! "chrono-Compatible Low-Level Date Algorithms",
//! <https://howardhinnant.github.io/date_algorithms.html> (read 2026-10-01).

use std::fmt;

/// Seconds in a civil day. Every day has exactly this many on the POSIX scale.
pub const SECONDS_PER_DAY: i64 = 86_400;

/// The widest instant this crate accepts, in seconds either side of the epoch.
///
/// About 34 800 years. Public entry points clamp into it, so that the day and
/// year arithmetic below can never overflow on input read from a file or a
/// network; no schedule a person sets is anywhere near the edge.
pub const INSTANT_LIMIT: i64 = 1 << 40;

/// Clamp an instant into `[-INSTANT_LIMIT, INSTANT_LIMIT]`.
pub fn clamp_instant(t: i64) -> i64 {
    t.clamp(-INSTANT_LIMIT, INSTANT_LIMIT)
}

/// Whether `year` is a Gregorian leap year.
pub fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Days in `month` (1 to 12) of `year`.
pub fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if is_leap_year(year) => 29,
        _ => 28,
    }
}

/// The day number of a proleptic Gregorian date. `month` is 1 to 12 and
/// `day` 1 to 31; a day past the month's end simply continues into the next
/// month, which the TZ rule code relies on for `Jn` dates.
pub fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    // Count from 1 March so the leap day is the last day of the counted year.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400; // [0, 399]
    let m = i64::from(month);
    let mp = if m > 2 { m - 3 } else { m + 9 }; // [0, 11], March = 0
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

/// The proleptic Gregorian date `(year, month, day)` of a day number.
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// A day of the week, Monday first (ISO 8601).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Weekday {
    /// Monday, bit 0 of a [`Days`] mask.
    Monday,
    /// Tuesday, bit 1.
    Tuesday,
    /// Wednesday, bit 2.
    Wednesday,
    /// Thursday, bit 3.
    Thursday,
    /// Friday, bit 4.
    Friday,
    /// Saturday, bit 5.
    Saturday,
    /// Sunday, bit 6.
    Sunday,
}

/// Every weekday, Monday first.
pub const WEEKDAYS: [Weekday; 7] = [
    Weekday::Monday,
    Weekday::Tuesday,
    Weekday::Wednesday,
    Weekday::Thursday,
    Weekday::Friday,
    Weekday::Saturday,
    Weekday::Sunday,
];

impl Weekday {
    /// The weekday of a day number. Day 0, 1970-01-01, was a Thursday.
    pub fn of_day(days: i64) -> Weekday {
        WEEKDAYS[(days + 3).rem_euclid(7) as usize]
    }

    /// 0 for Monday to 6 for Sunday.
    pub fn index(self) -> u8 {
        self as u8
    }

    /// The three-letter lower-case name, `mon` to `sun`: the spelling the
    /// control catalog's day lists use.
    pub fn short_name(self) -> &'static str {
        ["mon", "tue", "wed", "thu", "fri", "sat", "sun"][self as usize]
    }

    /// The weekday a three-letter name names, ignoring ASCII case.
    pub fn from_short_name(name: &str) -> Option<Weekday> {
        WEEKDAYS
            .iter()
            .copied()
            .find(|d| d.short_name().eq_ignore_ascii_case(name))
    }

    /// The day after this one.
    pub fn next(self) -> Weekday {
        WEEKDAYS[(self as usize + 1) % 7]
    }
}

/// A set of weekdays, one bit each, Monday at bit 0.
///
/// The empty set is meaningful: for an alarm it means "one shot" (the next
/// occurrence on any day, then disabled), and for a window it means "never".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Days(u8);

impl Days {
    /// No day.
    pub const NONE: Days = Days(0);
    /// Every day.
    pub const EVERY_DAY: Days = Days(0x7f);
    /// Monday to Friday.
    pub const WEEKDAYS: Days = Days(0x1f);
    /// Saturday and Sunday.
    pub const WEEKEND: Days = Days(0x60);

    /// The set a 7-bit mask names, or `None` if bit 7 is set.
    pub fn from_bits(bits: u8) -> Option<Days> {
        (bits & 0x80 == 0).then_some(Days(bits))
    }

    /// The 7-bit mask.
    pub fn bits(self) -> u8 {
        self.0
    }

    /// The set of the named days (`mon` to `sun`), or the first name that is
    /// not one. A repeated name is not an error; it is the same day.
    pub fn from_names<'a, I: IntoIterator<Item = &'a str>>(names: I) -> Result<Days, &'a str> {
        let mut bits = 0u8;
        for name in names {
            let day = Weekday::from_short_name(name).ok_or(name)?;
            bits |= 1 << day.index();
        }
        Ok(Days(bits))
    }

    /// The names in the set, Monday first.
    pub fn names(self) -> Vec<&'static str> {
        WEEKDAYS
            .iter()
            .filter(|d| self.contains(**d))
            .map(|d| d.short_name())
            .collect()
    }

    /// This set with `day` added.
    pub fn with(self, day: Weekday) -> Days {
        Days(self.0 | 1 << day.index())
    }

    /// Whether `day` is in the set.
    pub fn contains(self, day: Weekday) -> bool {
        self.0 & (1 << day.index()) != 0
    }

    /// Whether the set is empty.
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// A time of day to the minute, `00:00` to `23:59`: what a person sets an
/// alarm or a window boundary to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimeOfDay(u16);

/// Why a time of day did not parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeOfDayError(pub String);

impl fmt::Display for TimeOfDayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "not a time of day HH:MM (00:00 to 23:59): {:?}", self.0)
    }
}

impl std::error::Error for TimeOfDayError {}

impl TimeOfDay {
    /// Midnight, `00:00`.
    pub const MIDNIGHT: TimeOfDay = TimeOfDay(0);

    /// `hour:minute`, or `None` outside 00:00 to 23:59.
    pub fn new(hour: u8, minute: u8) -> Option<TimeOfDay> {
        (hour < 24 && minute < 60).then(|| TimeOfDay(u16::from(hour) * 60 + u16::from(minute)))
    }

    /// Parse exactly `HH:MM`, two digits each: the one spelling the control
    /// catalog uses, so `7:30` and `07:30:00` are refused rather than guessed.
    pub fn parse(s: &str) -> Result<TimeOfDay, TimeOfDayError> {
        let err = || TimeOfDayError(s.to_string());
        let b = s.as_bytes();
        if b.len() != 5 || b[2] != b':' || ![0, 1, 3, 4].iter().all(|&i| b[i].is_ascii_digit()) {
            return Err(err());
        }
        let hour = (b[0] - b'0') * 10 + (b[1] - b'0');
        let minute = (b[3] - b'0') * 10 + (b[4] - b'0');
        TimeOfDay::new(hour, minute).ok_or_else(err)
    }

    /// The hour, 0 to 23.
    pub fn hour(self) -> u8 {
        (self.0 / 60) as u8
    }

    /// The minute, 0 to 59.
    pub fn minute(self) -> u8 {
        (self.0 % 60) as u8
    }

    /// Minutes since midnight, 0 to 1439.
    pub fn minutes(self) -> u16 {
        self.0
    }

    /// Seconds since midnight.
    pub fn seconds(self) -> i64 {
        i64::from(self.0) * 60
    }
}

impl fmt::Display for TimeOfDay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}:{:02}", self.hour(), self.minute())
    }
}

/// Civil time in a zone at one instant: what a wall clock there reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Civil {
    /// Proleptic Gregorian year.
    pub year: i64,
    /// 1 to 12.
    pub month: u8,
    /// 1 to 31.
    pub day: u8,
    /// The day of the week.
    pub weekday: Weekday,
    /// 0 to 23.
    pub hour: u8,
    /// 0 to 59.
    pub minute: u8,
    /// 0 to 59 (POSIX time has no leap second).
    pub second: u8,
    /// Seconds east of UTC in effect at the instant.
    pub utc_offset_s: i32,
    /// The zone's abbreviation for the offset (`EST`, `CEST`, `+1030`).
    pub abbreviation: String,
    /// Whether the offset is daylight-saving time.
    pub dst: bool,
}

impl Civil {
    /// Civil time for the local second `local` (see the module docs) with the
    /// offset facts that produced it.
    pub fn from_local_second(
        local: i64,
        utc_offset_s: i32,
        abbreviation: &str,
        dst: bool,
    ) -> Civil {
        let days = local.div_euclid(SECONDS_PER_DAY);
        let sod = local.rem_euclid(SECONDS_PER_DAY);
        let (year, month, day) = civil_from_days(days);
        Civil {
            year,
            month: month as u8,
            day: day as u8,
            weekday: Weekday::of_day(days),
            hour: (sod / 3600) as u8,
            minute: (sod / 60 % 60) as u8,
            second: (sod % 60) as u8,
            utc_offset_s,
            abbreviation: abbreviation.to_string(),
            dst,
        }
    }

    /// The day number of this date.
    pub fn day_number(&self) -> i64 {
        days_from_civil(self.year, u32::from(self.month), u32::from(self.day))
    }

    /// Seconds since this day's midnight.
    pub fn second_of_day(&self) -> i64 {
        i64::from(self.hour) * 3600 + i64::from(self.minute) * 60 + i64::from(self.second)
    }

    /// The local second this civil time reads.
    pub fn local_second(&self) -> i64 {
        self.day_number() * SECONDS_PER_DAY + self.second_of_day()
    }
}

impl fmt::Display for Civil {
    /// `2026-03-08 03:00:00 EDT (-04:00)`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let off = self.utc_offset_s;
        let sign = if off < 0 { '-' } else { '+' };
        let a = off.unsigned_abs();
        write!(
            f,
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02} {} ({}{:02}:{:02})",
            self.year,
            self.month,
            self.day,
            self.hour,
            self.minute,
            self.second,
            self.abbreviation,
            sign,
            a / 3600,
            a / 60 % 60
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_numbers_round_trip_over_a_wide_range() {
        for days in (-1_000_000..1_000_000).step_by(997) {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days);
            assert!(d >= 1 && d <= days_in_month(y, m));
        }
    }

    #[test]
    fn known_dates() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        // 2026-10-01 is a Thursday.
        assert_eq!(
            Weekday::of_day(days_from_civil(2026, 10, 1)),
            Weekday::Thursday
        );
        assert_eq!(Weekday::of_day(0), Weekday::Thursday);
        assert_eq!(Weekday::of_day(-4), Weekday::Sunday);
        assert!(is_leap_year(2000) && !is_leap_year(1900) && is_leap_year(2028));
    }

    #[test]
    fn times_of_day_parse_one_spelling() {
        assert_eq!(
            TimeOfDay::parse("07:30").unwrap(),
            TimeOfDay::new(7, 30).unwrap()
        );
        assert_eq!(TimeOfDay::parse("23:59").unwrap().to_string(), "23:59");
        for bad in [
            "7:30", "24:00", "12:60", "07:30:00", "07-30", "", "ab:cd", "+1:30",
        ] {
            assert!(TimeOfDay::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn days_masks() {
        let d = Days::from_names(["mon", "Fri", "fri"]).unwrap();
        assert_eq!(d.bits(), 0b1_0001);
        assert_eq!(d.names(), vec!["mon", "fri"]);
        assert_eq!(Days::from_names(["mon", "xyz"]), Err("xyz"));
        assert!(Days::from_bits(0x80).is_none());
        assert!(Days::WEEKEND.contains(Weekday::Sunday));
        assert!(!Days::WEEKDAYS.contains(Weekday::Saturday));
        assert_eq!(Weekday::Sunday.next(), Weekday::Monday);
    }
}
