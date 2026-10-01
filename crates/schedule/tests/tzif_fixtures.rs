//! The TZif reader against the committed zoneinfo files in `fixtures/schedule`.
//!
//! Three independent holds on the same answer:
//!
//! 1. **glibc's own reader.** `fixtures/schedule/zdump.txt` is `zdump -v`
//!    over the same tzdata: the second before and the second of every
//!    transition in 2026 and 2027 (and 2100, far past the fat files' table, so
//!    only the footer can answer). Every line must convert identically, from
//!    the fat file and from the slim one.
//! 2. **The tz source rules, by hand.** The transition instants below are
//!    computed from the rule lines of the tz database (`tzdata.zi` 2026c):
//!    `R u 2007 ma - Mar Su>=8 2 1 D` / `N Su>=1 2 0 S` (United States),
//!    `R E 1981 ma - Mar lastSu 1u 1 S` / `1996 ma - O lastSu 1u 0 -` (EU),
//!    `R LH 2008 ma - Ap Su>=1 2 0 -` / `O Su>=1 2 0:30 -` (Lord Howe), read
//!    2026-10-01 from the host's tzdata package (IANA tz, public domain).
//! 3. **Fat and slim agree.** Hourly over 2020 to 2045 the two builds of each
//!    zone, one answering from its table and the other from its footer, must
//!    give the same civil time.

use std::path::PathBuf;

use chorus_schedule::civil::days_from_civil;
use chorus_schedule::tzif::{Tzif, TzifError};
use chorus_schedule::{Civil, Zone};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/schedule")
}

fn bytes(name: &str) -> Vec<u8> {
    std::fs::read(fixtures().join(name)).unwrap_or_else(|e| panic!("fixtures/schedule/{name}: {e}"))
}

fn zone(name: &str) -> Zone {
    Zone::from_tzif(&bytes(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

const ZONES: [&str; 7] = [
    "America_New_York",
    "Europe_Berlin",
    "Australia_Lord_Howe",
    "Asia_Jerusalem",
    "America_Nuuk",
    "Pacific_Apia",
    "Etc_UTC",
];

fn utc(y: i64, mo: u32, d: u32, h: i64, mi: i64, s: i64) -> i64 {
    days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + s
}

fn month(name: &str) -> u32 {
    1 + [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ]
    .iter()
    .position(|m| *m == name)
    .unwrap_or_else(|| panic!("month {name}")) as u32
}

/// `Sun Mar  8 06:59:59 2026` as (year, month, day, h, m, s).
fn zdump_time(words: &[&str]) -> (i64, u32, u32, i64, i64, i64) {
    let hms: Vec<i64> = words[3].split(':').map(|x| x.parse().unwrap()).collect();
    (
        words[4].parse().unwrap(),
        month(words[1]),
        words[2].parse().unwrap(),
        hms[0],
        hms[1],
        hms[2],
    )
}

#[test]
fn every_fixture_parses_with_its_version() {
    for z in ZONES {
        for build in ["fat", "slim"] {
            let t = Tzif::parse(&bytes(&format!("{z}.{build}.tzif")))
                .unwrap_or_else(|e| panic!("{z} {build}: {e}"));
            let want = if z == "Asia_Jerusalem" || z == "America_Nuuk" {
                3
            } else {
                2
            };
            assert_eq!(t.version, want, "{z} {build}");
            assert!(t.footer.is_some(), "{z} {build} has a footer");
        }
    }
}

#[test]
fn glibc_zdump_agrees_at_every_listed_second() {
    let text = String::from_utf8(bytes("zdump.txt")).unwrap();
    let mut checked = 0;
    for line in text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
    {
        let words: Vec<&str> = line.split_whitespace().collect();
        // zone, 5 words of UT time, "UT", "=", 5 words of local time, abbr, isdst=, gmtoff=
        assert_eq!(words.len(), 16, "{line}");
        let (y, mo, d, h, mi, s) = zdump_time(&words[1..6]);
        let t = utc(y, mo, d, h, mi, s);
        let (ly, lmo, ld, lh, lmi, ls) = zdump_time(&words[8..13]);
        let abbr = words[13];
        let dst = words[14] == "isdst=1";
        let gmtoff: i32 = words[15].trim_start_matches("gmtoff=").parse().unwrap();
        let file = words[0].replace('/', "_");
        for build in ["fat", "slim"] {
            let c: Civil = zone(&format!("{file}.{build}.tzif")).to_civil(t);
            let got = (
                c.year,
                u32::from(c.month),
                u32::from(c.day),
                i64::from(c.hour),
                i64::from(c.minute),
                i64::from(c.second),
            );
            assert_eq!(got, (ly, lmo, ld, lh, lmi, ls), "{build}: {line}");
            assert_eq!(
                (c.abbreviation.as_str(), c.dst, c.utc_offset_s),
                (abbr, dst, gmtoff),
                "{build}: {line}"
            );
            checked += 1;
        }
    }
    assert!(checked >= 120, "only {checked} conversions checked");
}

/// (zone, UTC instant of the transition, local before, local after), local
/// as `YYYY-MM-DD HH:MM:SS ABBR (+hh:mm)`. Instants from the tz rule lines in
/// the module docs.
fn transitions_2026_2027() -> Vec<(&'static str, i64, &'static str, &'static str)> {
    vec![
        // United States: second Sunday of March 02:00 EST, first Sunday of
        // November 02:00 EDT.
        (
            "America_New_York",
            utc(2026, 3, 8, 7, 0, 0),
            "2026-03-08 01:59:59 EST (-05:00)",
            "2026-03-08 03:00:00 EDT (-04:00)",
        ),
        (
            "America_New_York",
            utc(2026, 11, 1, 6, 0, 0),
            "2026-11-01 01:59:59 EDT (-04:00)",
            "2026-11-01 01:00:00 EST (-05:00)",
        ),
        (
            "America_New_York",
            utc(2027, 3, 14, 7, 0, 0),
            "2027-03-14 01:59:59 EST (-05:00)",
            "2027-03-14 03:00:00 EDT (-04:00)",
        ),
        (
            "America_New_York",
            utc(2027, 11, 7, 6, 0, 0),
            "2027-11-07 01:59:59 EDT (-04:00)",
            "2027-11-07 01:00:00 EST (-05:00)",
        ),
        // EU: last Sunday of March and of October, 01:00 UTC.
        (
            "Europe_Berlin",
            utc(2026, 3, 29, 1, 0, 0),
            "2026-03-29 01:59:59 CET (+01:00)",
            "2026-03-29 03:00:00 CEST (+02:00)",
        ),
        (
            "Europe_Berlin",
            utc(2026, 10, 25, 1, 0, 0),
            "2026-10-25 02:59:59 CEST (+02:00)",
            "2026-10-25 02:00:00 CET (+01:00)",
        ),
        (
            "Europe_Berlin",
            utc(2027, 3, 28, 1, 0, 0),
            "2027-03-28 01:59:59 CET (+01:00)",
            "2027-03-28 03:00:00 CEST (+02:00)",
        ),
        (
            "Europe_Berlin",
            utc(2027, 10, 31, 1, 0, 0),
            "2027-10-31 02:59:59 CEST (+02:00)",
            "2027-10-31 02:00:00 CET (+01:00)",
        ),
        // Lord Howe: half an hour, first Sunday of April and of October, 02:00.
        (
            "Australia_Lord_Howe",
            utc(2026, 4, 4, 15, 0, 0),
            "2026-04-05 01:59:59 +11 (+11:00)",
            "2026-04-05 01:30:00 +1030 (+10:30)",
        ),
        (
            "Australia_Lord_Howe",
            utc(2026, 10, 3, 15, 30, 0),
            "2026-10-04 01:59:59 +1030 (+10:30)",
            "2026-10-04 02:30:00 +11 (+11:00)",
        ),
        (
            "Australia_Lord_Howe",
            utc(2027, 4, 3, 15, 0, 0),
            "2027-04-04 01:59:59 +11 (+11:00)",
            "2027-04-04 01:30:00 +1030 (+10:30)",
        ),
        (
            "Australia_Lord_Howe",
            utc(2027, 10, 2, 15, 30, 0),
            "2027-10-03 01:59:59 +1030 (+10:30)",
            "2027-10-03 02:30:00 +11 (+11:00)",
        ),
    ]
}

#[test]
fn transitions_from_the_tz_rules() {
    for (z, t, before, after) in transitions_2026_2027() {
        for build in ["fat", "slim"] {
            let zone = zone(&format!("{z}.{build}.tzif"));
            assert_eq!(zone.to_civil(t - 1).to_string(), before, "{z} {build}");
            assert_eq!(zone.to_civil(t).to_string(), after, "{z} {build}");
            assert_eq!(
                zone.next_transition_after(t - 3 * 86_400),
                Some(t),
                "{z} {build}"
            );
        }
    }
}

#[test]
fn fat_and_slim_agree_hourly_for_twenty_five_years() {
    for z in ZONES {
        let fat = zone(&format!("{z}.fat.tzif"));
        let slim = zone(&format!("{z}.slim.tzif"));
        let mut t = utc(2020, 1, 1, 0, 0, 0);
        while t < utc(2045, 1, 1, 0, 0, 0) {
            assert_eq!(fat.to_civil(t), slim.to_civil(t), "{z} at {t}");
            t += 3600;
        }
    }
}

#[test]
fn the_footer_as_a_posix_zone_agrees_past_the_table() {
    // A TZ value with no file behind it is the footer alone.
    let ny = zone("America_New_York.fat.tzif");
    let posix = Zone::from_posix("EST5EDT,M3.2.0,M11.1.0").unwrap();
    let mut t = utc(2026, 1, 1, 0, 0, 0);
    while t < utc(2040, 1, 1, 0, 0, 0) {
        assert_eq!(ny.to_civil(t), posix.to_civil(t), "at {t}");
        t += 1800;
    }
}

/// The version 1 data block of a version 2 file, alone, as a version 1 file.
fn as_version_1(v2: &[u8]) -> Vec<u8> {
    let n = |i: usize| {
        u32::from_be_bytes([
            v2[20 + 4 * i],
            v2[21 + 4 * i],
            v2[22 + 4 * i],
            v2[23 + 4 * i],
        ]) as usize
    };
    let (isut, isstd, leap, time, typ, chr) = (n(0), n(1), n(2), n(3), n(4), n(5));
    let len = 44 + time * 5 + typ * 6 + chr + leap * 8 + isstd + isut;
    let mut v1 = v2[..len].to_vec();
    v1[4] = 0;
    v1
}

#[test]
fn version_1_files_are_read() {
    for z in ["America_New_York", "Europe_Berlin", "Australia_Lord_Howe"] {
        let fat = bytes(&format!("{z}.fat.tzif"));
        let v1 = Tzif::parse(&as_version_1(&fat)).unwrap();
        assert_eq!(v1.version, 1);
        assert!(v1.footer.is_none());
        let v1 = Zone::Tzif(v1);
        let v2 = zone(&format!("{z}.fat.tzif"));
        // The 32-bit table runs to 2037, so 2026 and 2027 are inside it.
        for (zz, t, _, _) in transitions_2026_2027().into_iter().filter(|r| r.0 == z) {
            let _ = zz;
            for dt in [-1, 0, 1, 3600] {
                assert_eq!(v1.to_civil(t + dt), v2.to_civil(t + dt));
            }
        }
    }
}

#[test]
fn a_damaged_file_is_an_error_never_a_panic() {
    let fat = bytes("Europe_Berlin.fat.tzif");
    for len in 0..fat.len() {
        assert!(
            Tzif::parse(&fat[..len]).is_err(),
            "a {len}-byte prefix parsed"
        );
    }
    // Flip every byte in turn: whatever it parses as, it must not panic, and
    // a parse that succeeds must still answer without panicking.
    for i in 0..fat.len() {
        let mut b = fat.clone();
        b[i] ^= 0xa5;
        if let Ok(t) = Tzif::parse(&b) {
            let z = Zone::Tzif(t);
            let _ = z.to_civil(utc(2026, 10, 25, 1, 0, 0));
            let _ = z.next_transition_after(0);
        }
    }
    assert_eq!(Tzif::parse(b"TZiX"), Err(TzifError::NotTzif));
    let mut v = fat.clone();
    v[4] = b'9';
    assert_eq!(Tzif::parse(&v), Err(TzifError::Version(b'9')));
    let mut trailing = fat.clone();
    trailing.push(b'x');
    assert!(Tzif::parse(&trailing).is_err());
}

/// A minimal version 2 file: one type, no transitions, the given leap count
/// and footer.
fn minimal_v2(leapcnt: u32, footer: &str) -> Vec<u8> {
    let header = |time_size: usize, out: &mut Vec<u8>| {
        out.extend_from_slice(b"TZif2");
        out.extend_from_slice(&[0; 15]);
        for c in [0u32, 0, leapcnt, 0, 1, 4] {
            out.extend_from_slice(&c.to_be_bytes());
        }
        out.extend_from_slice(&3600i32.to_be_bytes());
        out.extend_from_slice(&[0, 0]);
        out.extend_from_slice(b"ABC\0");
        for _ in 0..leapcnt {
            out.extend_from_slice(&vec![0u8; time_size + 4]);
        }
    };
    let mut out = Vec::new();
    header(4, &mut out);
    header(8, &mut out);
    out.push(b'\n');
    out.extend_from_slice(footer.as_bytes());
    out.push(b'\n');
    out
}

#[test]
fn a_file_with_no_transition() {
    // An empty footer: type 0 throughout.
    let z = Zone::from_tzif(&minimal_v2(0, "")).unwrap();
    let c = z.to_civil(0);
    assert_eq!((c.hour, c.abbreviation.as_str()), (1, "ABC"));
    assert_eq!(z.next_transition_after(0), None);
    // A footer with rules: the footer throughout (RFC 9636 section 3.3).
    let z = Zone::from_tzif(&minimal_v2(0, "CET-1CEST,M3.5.0,M10.5.0/3")).unwrap();
    assert_eq!(z.to_civil(utc(2026, 7, 1, 0, 0, 0)).abbreviation, "CEST");
    assert_eq!(
        z.next_transition_after(utc(2026, 1, 1, 0, 0, 0)),
        Some(utc(2026, 3, 29, 1, 0, 0))
    );
}

#[test]
fn leap_second_files_are_refused_by_name() {
    assert_eq!(
        Zone::from_tzif(&minimal_v2(1, "")),
        Err(chorus_schedule::ZoneError::Tzif(TzifError::LeapSeconds))
    );
    assert!(TzifError::LeapSeconds.to_string().contains("posix"));
    assert!(matches!(
        Tzif::parse(&minimal_v2(0, "EST5EDT")),
        Err(TzifError::Footer(_))
    ));
}
