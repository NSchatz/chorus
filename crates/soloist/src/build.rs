//! When a Soloist build expires: the `soloist --version` parser and the
//! arithmetic on its answer.
//!
//! A Soloist build stops working 90 days after it was built ("exits with
//! code `10` when the build has expired"; Soloist's command-line reference
//! and downloads page, read 2026-10-03, cited in `docs/soloist.md`). The
//! format of `--version` is NOT documented: the page says only that the
//! output "includes the Spotify Soloist version, build timestamp, build
//! identifiers, platform, and architecture". The shapes users have
//! transcribed in issue reports (LEADs, not the literal output) all carry
//! either a 10-digit Unix time or a calendar date, so [`parse_version`]
//! looks for exactly those and nothing else:
//!
//! 1. a run of exactly 10 decimal digits, not touching a letter or another
//!    digit, that is a time from 2020 to 2099: the build's epoch seconds;
//! 2. else a date, `YYYYMMDD` or `YYYY-MM-DD`, with the same boundaries and
//!    a valid month and day: midnight UTC of that day, the earliest the
//!    build can have been made, so a warning is never late.
//!
//! Anything else is "unknown". Unknown is never "expired": the supervisor
//! then reports nulls and warns about nothing, and Soloist's own exit code
//! 10 remains the authority.
//!
//! No function here reads a clock: "now" is an argument (wall-clock Unix
//! seconds, the only clock a calendar deadline can be compared with; this is
//! not the audio path).

/// Days a build works for after it was built.
pub const LIFETIME_DAYS: u64 = 90;

/// Days before expiry from which a warning is raised.
pub const WARNING_DAYS: i64 = 14;

/// Seconds in a day.
pub const SECONDS_PER_DAY: u64 = 86_400;

/// The longest version text kept, in characters.
pub const MAX_VERSION_CHARS: usize = crate::protocol::MAX_VERSION_CHARS;

/// 2020-01-01T00:00:00Z: no Soloist build is older (the product was
/// announced in 2026).
const EPOCH_MIN: u64 = 1_577_836_800;

/// 2100-01-01T00:00:00Z.
const EPOCH_MAX: u64 = 4_102_444_800;

/// What `soloist --version` said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildInfo {
    /// The first non-empty line of the output, trimmed, at most
    /// [`MAX_VERSION_CHARS`] characters.
    pub version: String,
    /// The build's time in Unix seconds, if the output named one.
    pub build_epoch: Option<u64>,
}

impl BuildInfo {
    /// When this build expires, in Unix seconds, if its time is known.
    pub fn expires_epoch(&self) -> Option<u64> {
        self.build_epoch.map(expires_epoch)
    }
}

/// Days since 1970-01-01 of a proleptic Gregorian date. The arithmetic is
/// the "days from civil" algorithm of Howard Hinnant's date paper
/// (https://howardhinnant.github.io/date_algorithms.html, public domain,
/// read 2026-10-03), for years from 1970 on.
fn days_from_civil(year: u64, month: u64, day: u64) -> u64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn days_in_month(year: u64, month: u64) -> u64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => {
            29
        }
        _ => 28,
    }
}

/// Midnight UTC of a date, if it is a real date from 2020 to 2099.
fn date_epoch(year: u64, month: u64, day: u64) -> Option<u64> {
    if !(2020..2100).contains(&year) || !(1..=12).contains(&month) {
        return None;
    }
    if day == 0 || day > days_in_month(year, month) {
        return None;
    }
    Some(days_from_civil(year, month, day) * SECONDS_PER_DAY)
}

fn digits(bytes: &[u8]) -> Option<u64> {
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return None;
    }
    std::str::from_utf8(bytes).ok()?.parse().ok()
}

/// Read the output of `soloist --version`.
pub fn parse_version(output: &str) -> BuildInfo {
    let version: String = output
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_VERSION_CHARS)
        .collect();
    let bytes = output.as_bytes();
    // A token is a maximal run of ASCII letters, digits and hyphens: "not
    // touching a letter or another digit" is then "the whole token", except
    // for the hyphenated date, which is one token by this definition too.
    let is_token = |b: u8| b.is_ascii_alphanumeric() || b == b'-';
    let mut tokens = Vec::new();
    let mut start = None;
    for (i, b) in bytes.iter().enumerate() {
        match (is_token(*b), start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                tokens.push(&bytes[s..i]);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        tokens.push(&bytes[s..]);
    }
    let epoch = tokens.iter().find_map(|t| {
        let n = if t.len() == 10 {
            digits(t)?
        } else {
            return None;
        };
        (EPOCH_MIN..EPOCH_MAX).contains(&n).then_some(n)
    });
    let date = || {
        tokens.iter().find_map(|t| match t.len() {
            8 => date_epoch(digits(&t[..4])?, digits(&t[4..6])?, digits(&t[6..])?),
            10 if t[4] == b'-' && t[7] == b'-' => {
                date_epoch(digits(&t[..4])?, digits(&t[5..7])?, digits(&t[8..])?)
            }
            _ => None,
        })
    };
    BuildInfo {
        version,
        build_epoch: epoch.or_else(date),
    }
}

/// When a build made at `build_epoch` expires: [`LIFETIME_DAYS`] later.
pub fn expires_epoch(build_epoch: u64) -> u64 {
    build_epoch.saturating_add(LIFETIME_DAYS * SECONDS_PER_DAY)
}

/// Whole days left before `expires_epoch`, rounded down: 0 on the last day,
/// negative once expired.
pub fn days_left(expires_epoch: u64, now_epoch: u64) -> i64 {
    let day = SECONDS_PER_DAY as i64;
    let left = i64::try_from(expires_epoch).unwrap_or(i64::MAX)
        - i64::try_from(now_epoch).unwrap_or(i64::MAX);
    left.div_euclid(day)
}

/// Where a build stands against its expiry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expiry {
    /// The build's time is not known. Not a warning, not an expiry.
    Unknown,
    /// More than [`WARNING_DAYS`] left.
    Fine {
        /// Whole days left.
        days_left: i64,
    },
    /// [`WARNING_DAYS`] or fewer left, and not yet expired.
    Warning {
        /// Whole days left; 0 on the last day.
        days_left: i64,
    },
    /// Past its expiry.
    Expired,
}

impl Expiry {
    /// Where a build stands at `now_epoch`.
    pub fn at(expires_epoch: Option<u64>, now_epoch: u64) -> Expiry {
        let Some(expires) = expires_epoch else {
            return Expiry::Unknown;
        };
        if now_epoch >= expires {
            return Expiry::Expired;
        }
        let days_left = days_left(expires, now_epoch);
        if days_left <= WARNING_DAYS {
            Expiry::Warning { days_left }
        } else {
            Expiry::Fine { days_left }
        }
    }

    /// The whole days left, when known and not yet expired.
    pub fn days_left(self) -> Option<i64> {
        match self {
            Expiry::Fine { days_left } | Expiry::Warning { days_left } => Some(days_left),
            Expiry::Unknown | Expiry::Expired => None,
        }
    }

    /// The line to show the owner, if there is something to say: "Soloist
    /// build expires in N days" from [`WARNING_DAYS`] before, "Soloist build
    /// expired" after.
    pub fn warning(self) -> Option<String> {
        match self {
            Expiry::Warning { days_left: 1 } => Some("Soloist build expires in 1 day".to_string()),
            Expiry::Warning { days_left } => {
                Some(format!("Soloist build expires in {days_left} days"))
            }
            Expiry::Expired => Some("Soloist build expired".to_string()),
            Expiry::Unknown | Expiry::Fine { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = SECONDS_PER_DAY;

    #[test]
    fn dates_are_days_since_1970() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        // 2020-01-01 and 2100-01-01 are the bounds above.
        assert_eq!(days_from_civil(2020, 1, 1) * DAY, EPOCH_MIN);
        assert_eq!(days_from_civil(2100, 1, 1) * DAY, EPOCH_MAX);
        assert_eq!(date_epoch(2024, 2, 29), Some(1_709_164_800));
        assert_eq!(date_epoch(2026, 2, 29), None);
        assert_eq!(date_epoch(2026, 13, 1), None);
        assert_eq!(date_epoch(2026, 4, 31), None);
        assert_eq!(date_epoch(2026, 0, 10), None);
        assert_eq!(date_epoch(1999, 1, 1), None);
    }

    /// The two epochs users transcribed with their dates (issues #8 and
    /// #10): each falls on the date stated beside it.
    #[test]
    fn the_transcribed_epochs_fall_on_their_stated_dates() {
        let sept4 = date_epoch(2026, 9, 4).unwrap();
        assert!((sept4..sept4 + DAY).contains(&1_788_523_318));
        let sept9 = date_epoch(2026, 9, 9).unwrap();
        assert!((sept9..sept9 + DAY).contains(&1_788_933_710));
    }

    #[test]
    fn every_transcribed_shape_is_read() {
        // Issue #1.
        let info = parse_version("soloist 1.3.7.292 (build 1786982514, x86_64, ...)\n");
        assert_eq!(
            info.version,
            "soloist 1.3.7.292 (build 1786982514, x86_64, ...)"
        );
        assert_eq!(info.build_epoch, Some(1_786_982_514));
        // Issue #8: an epoch and a date; the epoch wins (it is the finer).
        let info = parse_version(
            "Soloist: 1.3.8.9\nBuild: 1788523318\nBuild date: 2026-09-04\nGit revision: g5c3a2053ac\n",
        );
        assert_eq!(info.version, "Soloist: 1.3.8.9");
        assert_eq!(info.build_epoch, Some(1_788_523_318));
        // Issue #10.
        let info = parse_version(
            "Soloist: 1.3.8.28\nBuild: 1788933710 (20260909)\nPlatform: linux/aarch64\n",
        );
        assert_eq!(info.build_epoch, Some(1_788_933_710));
        // Issue #13: a date only.
        let info = parse_version("Soloist 1.3.8.96, build 20260930, Linux/aarch64");
        assert_eq!(
            info.version,
            "Soloist 1.3.8.96, build 20260930, Linux/aarch64"
        );
        assert_eq!(info.build_epoch, date_epoch(2026, 9, 30));
        // The hyphenated date alone.
        let info = parse_version("Soloist: 1.3.8.9\r\nBuild date: 2026-09-04\r\n");
        assert_eq!(info.build_epoch, date_epoch(2026, 9, 4));
        assert_eq!(info.version, "Soloist: 1.3.8.9");
    }

    #[test]
    fn garbage_is_unknown_never_expired() {
        for text in [
            "",
            "\n\n",
            "soloist",
            "Soloist 1.3.8.96",
            "error: unknown option",
            // Too few and too many digits, and digits touching letters.
            "build 178898251",
            "build 17889825140",
            "rev g1788523318ab",
            "rev 1788523318ab",
            "x20260930",
            "202609301",
            // Out of range: 2001 and 2286 as epochs, 1999 and an impossible date.
            "build 1000000000",
            "build 9999999999",
            "build 19990101",
            "build 20261341",
            "build 2026-13-41",
            "build 2026-9-4",
            "2026/09/04",
            "\u{0}\u{1}\u{fffd}",
        ] {
            let info = parse_version(text);
            assert_eq!(info.build_epoch, None, "{text:?}");
            assert_eq!(info.expires_epoch(), None);
            assert_eq!(Expiry::at(info.expires_epoch(), u64::MAX), Expiry::Unknown);
            assert_eq!(Expiry::at(info.expires_epoch(), u64::MAX).warning(), None);
        }
    }

    #[test]
    fn the_version_text_is_one_bounded_clean_line() {
        let info = parse_version(&format!(
            "\n  \u{1b}[1mSoloist\u{7} {}\nmore",
            "x".repeat(500)
        ));
        assert!(info.version.starts_with("[1mSoloist x"));
        assert_eq!(info.version.chars().count(), MAX_VERSION_CHARS);
    }

    #[test]
    fn a_build_lives_90_days() {
        assert_eq!(expires_epoch(1_788_523_318), 1_788_523_318 + 90 * DAY);
        assert_eq!(expires_epoch(u64::MAX), u64::MAX);
    }

    #[test]
    fn days_left_rounds_down() {
        let expires = 100 * DAY;
        assert_eq!(days_left(expires, 0), 100);
        assert_eq!(days_left(expires, 1), 99);
        assert_eq!(days_left(expires, 99 * DAY), 1);
        assert_eq!(days_left(expires, 99 * DAY + 1), 0);
        assert_eq!(days_left(expires, expires), 0);
        assert_eq!(days_left(expires, expires + 1), -1);
        assert_eq!(days_left(expires, expires + DAY), -1);
        assert_eq!(days_left(expires, expires + DAY + 1), -2);
    }

    #[test]
    fn the_warning_starts_14_days_before() {
        let built = 1_788_523_318;
        let expires = expires_epoch(built);
        let at = |now: u64| Expiry::at(Some(expires), now);
        assert_eq!(at(built), Expiry::Fine { days_left: 90 });
        assert_eq!(at(built).warning(), None);
        // One second before the 14-day mark's day ends: 15 whole days left.
        assert_eq!(at(expires - 15 * DAY), Expiry::Fine { days_left: 15 });
        assert_eq!(
            at(expires - 15 * DAY + 1),
            Expiry::Warning { days_left: 14 }
        );
        assert_eq!(
            at(expires - 15 * DAY + 1).warning().as_deref(),
            Some("Soloist build expires in 14 days")
        );
        assert_eq!(
            at(expires - 10 * DAY).warning().as_deref(),
            Some("Soloist build expires in 10 days")
        );
        assert_eq!(
            at(expires - DAY).warning().as_deref(),
            Some("Soloist build expires in 1 day")
        );
        assert_eq!(at(expires - 1), Expiry::Warning { days_left: 0 });
        assert_eq!(
            at(expires - 1).warning().as_deref(),
            Some("Soloist build expires in 0 days")
        );
        assert_eq!(at(expires), Expiry::Expired);
        assert_eq!(at(expires + 400 * DAY), Expiry::Expired);
        assert_eq!(
            at(expires).warning().as_deref(),
            Some("Soloist build expired")
        );
        assert_eq!(at(expires).days_left(), None);
        assert_eq!(at(built).days_left(), Some(90));
    }
}
