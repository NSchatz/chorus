//! The gap and fold rules, on real zones, for alarms and windows.
//!
//! Gap: a local time clocks skip resolves to the transition instant (the
//! first valid instant after it). Fold: a local time that happens twice
//! resolves to its first occurrence, and an alarm there rings once.
//! Transition instants are the ones `tests/tzif_fixtures.rs` holds to the tz
//! rules and to glibc.

use std::path::PathBuf;

use chorus_schedule::civil::days_from_civil;
use chorus_schedule::{due_between, Alarm, Days, Resolved, TimeOfDay, Weekday, WeeklyWindow, Zone};

const H: i64 = 3600;

fn zone(name: &str) -> Zone {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../fixtures/schedule/{name}.fat.tzif"));
    Zone::from_tzif(&std::fs::read(&p).unwrap()).unwrap()
}

fn utc(y: i64, mo: u32, d: u32, h: i64, mi: i64) -> i64 {
    days_from_civil(y, mo, d) * 86_400 + h * H + mi * 60
}

fn day(y: i64, mo: u32, d: u32) -> i64 {
    days_from_civil(y, mo, d)
}

fn tod(s: &str) -> TimeOfDay {
    TimeOfDay::parse(s).unwrap()
}

#[test]
fn resolve_reports_each_case() {
    let ny = zone("America_New_York");
    assert_eq!(
        ny.resolve(day(2026, 3, 8), H + 1800),
        Resolved::Unique(utc(2026, 3, 8, 6, 30))
    );
    assert_eq!(
        ny.resolve(day(2026, 3, 8), 2 * H + 1800),
        Resolved::Gap {
            skipped_to: utc(2026, 3, 8, 7, 0)
        }
    );
    // 02:00 itself is in the gap (the clock reads 01:59:59 then 03:00:00).
    assert_eq!(
        ny.resolve(day(2026, 3, 8), 2 * H),
        Resolved::Gap {
            skipped_to: utc(2026, 3, 8, 7, 0)
        }
    );
    assert_eq!(
        ny.resolve(day(2026, 3, 8), 3 * H),
        Resolved::Unique(utc(2026, 3, 8, 7, 0))
    );
    assert_eq!(
        ny.resolve(day(2026, 11, 1), H + 1800),
        Resolved::Fold {
            first: utc(2026, 11, 1, 5, 30),
            second: utc(2026, 11, 1, 6, 30)
        }
    );
    // 02:00 after the fold happens once (01:59:59 EST is followed by 02:00 EST).
    assert_eq!(
        ny.resolve(day(2026, 11, 1), 2 * H),
        Resolved::Unique(utc(2026, 11, 1, 7, 0))
    );

    let berlin = zone("Europe_Berlin");
    assert_eq!(
        berlin.resolve(day(2026, 3, 29), 2 * H + 1800),
        Resolved::Gap {
            skipped_to: utc(2026, 3, 29, 1, 0)
        }
    );
    assert_eq!(
        berlin.resolve(day(2026, 10, 25), 2 * H + 1800),
        Resolved::Fold {
            first: utc(2026, 10, 25, 0, 30),
            second: utc(2026, 10, 25, 1, 30)
        }
    );

    // Lord Howe: half-hour changes. 02:00 to 02:29 skipped in October; 01:30
    // to 01:59 repeated in April.
    let lh = zone("Australia_Lord_Howe");
    assert_eq!(
        lh.resolve(day(2026, 10, 4), 2 * H + 15 * 60),
        Resolved::Gap {
            skipped_to: utc(2026, 10, 3, 15, 30)
        }
    );
    assert_eq!(
        lh.resolve(day(2026, 4, 5), H + 45 * 60),
        Resolved::Fold {
            first: utc(2026, 4, 4, 14, 45),
            second: utc(2026, 4, 4, 15, 15)
        }
    );

    // Version 3 rules: Jerusalem's "26:00 Thursday" is 02:00 Friday; Nuuk's
    // "-1:00 Sunday" is 23:00 Saturday.
    let jerusalem = zone("Asia_Jerusalem");
    assert_eq!(
        jerusalem.resolve(day(2026, 3, 27), 2 * H + 1800),
        Resolved::Gap {
            skipped_to: utc(2026, 3, 27, 0, 0)
        }
    );
    let nuuk = zone("America_Nuuk");
    assert_eq!(
        nuuk.resolve(day(2026, 3, 28), 23 * H + 1800),
        Resolved::Gap {
            skipped_to: utc(2026, 3, 29, 1, 0)
        }
    );
}

#[test]
fn an_alarm_in_a_gap_rings_at_the_transition() {
    let ny = zone("America_New_York");
    let alarm = Alarm::new(tod("02:30"), Days::EVERY_DAY);
    let sat_noon = utc(2026, 3, 7, 17, 0);
    let at = alarm.next_fire_after(&ny, sat_noon).unwrap();
    assert_eq!(at, utc(2026, 3, 8, 7, 0));
    assert_eq!(
        ny.to_civil(at).to_string(),
        "2026-03-08 03:00:00 EDT (-04:00)"
    );
    // And the next morning is an ordinary 02:30 EDT.
    assert_eq!(alarm.next_fire_after(&ny, at), Some(utc(2026, 3, 9, 6, 30)));
}

#[test]
fn an_alarm_in_a_fold_rings_once_at_the_first() {
    let ny = zone("America_New_York");
    let alarm = Alarm::new(tod("01:30"), Days::EVERY_DAY);
    let first = alarm
        .next_fire_after(&ny, utc(2026, 10, 31, 17, 0))
        .unwrap();
    assert_eq!(first, utc(2026, 11, 1, 5, 30));
    assert_eq!(ny.to_civil(first).abbreviation, "EDT");
    // Asked after the first, or even between the two readings, it does not
    // offer the second 01:30 (06:30 UTC): the next ring is Monday's.
    let monday = utc(2026, 11, 2, 6, 30);
    assert_eq!(alarm.next_fire_after(&ny, first), Some(monday));
    assert_eq!(
        alarm.next_fire_after(&ny, utc(2026, 11, 1, 6, 0)),
        Some(monday)
    );
    // The poll over the whole night returns it once.
    let alarms = [("a", alarm)];
    let due = due_between(
        &alarms,
        &ny,
        utc(2026, 10, 31, 17, 0),
        utc(2026, 11, 1, 12, 0),
    );
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].at, first);
}

#[test]
fn a_skipped_day_still_rings() {
    // Samoa skipped Friday 2011-12-30 entirely (Thursday 23:59:59 -10 was
    // followed by Saturday 00:00:00 +14). A Friday alarm rings at the jump.
    let apia = zone("Pacific_Apia");
    let fri = Days::NONE.with(Weekday::Friday);
    let alarm = Alarm::new(tod("07:00"), fri);
    let at = alarm
        .next_fire_after(&apia, utc(2011, 12, 29, 0, 0))
        .unwrap();
    assert_eq!(at, utc(2011, 12, 30, 10, 0));
    assert_eq!(
        apia.to_civil(at).to_string(),
        "2011-12-31 00:00:00 +14 (+14:00)"
    );
    // A Saturday alarm that morning is unaffected.
    let sat = Alarm::new(tod("07:00"), Days::NONE.with(Weekday::Saturday));
    assert_eq!(
        sat.next_fire_after(&apia, at),
        Some(utc(2011, 12, 30, 17, 0))
    );
}

#[test]
fn a_repeating_alarm_tracks_the_wall_clock_across_both_changes() {
    let berlin = zone("Europe_Berlin");
    let alarm = Alarm::new(tod("07:00"), Days::EVERY_DAY);
    let mut t = utc(2026, 3, 20, 0, 0);
    let mut rings = 0;
    while t < utc(2026, 11, 10, 0, 0) {
        let at = alarm.next_fire_after(&berlin, t).unwrap();
        let c = berlin.to_civil(at);
        assert_eq!((c.hour, c.minute, c.second), (7, 0, 0), "{c}");
        assert!(at - t <= 86_400 + H, "a ring skipped after {t}");
        t = at;
        rings += 1;
    }
    assert!(rings > 230);
}

#[test]
fn a_window_follows_the_wall_clock_across_a_change() {
    let ny = zone("America_New_York");
    let quiet = WeeklyWindow::new(Days::EVERY_DAY, tod("22:00"), tod("07:00"));
    // Saturday 2026-10-31 22:00 EDT = Sunday 02:00 UTC; ends Sunday 07:00
    // EST = 12:00 UTC: ten hours of elapsed time, as the clock reads it.
    let opens = utc(2026, 11, 1, 2, 0);
    assert_eq!(
        quiet.next_boundary_after(&ny, utc(2026, 10, 31, 20, 0)),
        Some(opens)
    );
    assert_eq!(
        quiet.next_boundary_after(&ny, opens),
        Some(utc(2026, 11, 1, 12, 0))
    );
    assert!(quiet.contains_at(&ny, utc(2026, 11, 1, 6, 30)));
    // A boundary in a gap: a window that ends at 02:30 on the spring-forward
    // night ends at the jump.
    let late = WeeklyWindow::new(
        Days::NONE.with(Weekday::Saturday),
        tod("23:00"),
        tod("02:30"),
    );
    assert_eq!(
        late.next_boundary_after(&ny, utc(2026, 3, 8, 4, 30)),
        Some(utc(2026, 3, 8, 7, 0))
    );
    assert!(late.contains_at(&ny, utc(2026, 3, 8, 6, 59)));
    assert!(!late.contains_at(&ny, utc(2026, 3, 8, 7, 0)));
}

#[test]
fn a_window_in_a_fold_follows_the_clock_both_times_round() {
    let ny = zone("America_New_York");
    // Sunday 00:00 to 01:30. On 2026-11-01 the clock reads 01:00 to 01:59
    // twice, so the window closes at the first 01:30 (05:30 UTC), opens
    // again when the clock falls back to 01:00 (06:00 UTC) and closes at the
    // second 01:30 (06:30 UTC).
    let w = WeeklyWindow::new(Days::NONE.with(Weekday::Sunday), tod("00:00"), tod("01:30"));
    let mut t = utc(2026, 10, 31, 12, 0);
    let mut boundaries = Vec::new();
    for _ in 0..4 {
        t = w.next_boundary_after(&ny, t).unwrap();
        boundaries.push(t);
    }
    assert_eq!(
        boundaries,
        vec![
            utc(2026, 11, 1, 4, 0),
            utc(2026, 11, 1, 5, 30),
            utc(2026, 11, 1, 6, 0),
            utc(2026, 11, 1, 6, 30)
        ]
    );
}

#[test]
fn boundaries_are_exactly_where_contains_changes() {
    // Property: walking minute by minute through a month with two changes in
    // three zones, contains_at changes value exactly at the instants
    // next_boundary_after reports, and nowhere else.
    let windows = [
        WeeklyWindow::new(Days::EVERY_DAY, tod("22:00"), tod("07:00")),
        WeeklyWindow::new(Days::WEEKDAYS, tod("01:15"), tod("02:45")),
        WeeklyWindow::new(Days::WEEKEND, tod("02:00"), tod("02:00")),
    ];
    for (name, from) in [
        ("America_New_York", utc(2026, 10, 28, 0, 0)),
        ("Europe_Berlin", utc(2026, 3, 26, 0, 0)),
        ("Australia_Lord_Howe", utc(2026, 4, 2, 0, 0)),
    ] {
        let z = zone(name);
        for w in &windows {
            let end = from + 7 * 86_400;
            // Every change, found the slow way: every boundary and every
            // transition here is on a whole minute.
            let mut changes = Vec::new();
            let mut prev = w.contains_at(&z, from);
            let mut t = from + 60;
            while t <= end {
                let now = w.contains_at(&z, t);
                if now != prev {
                    changes.push(t);
                }
                prev = now;
                t += 60;
            }
            let mut predicted = Vec::new();
            let mut t = from;
            while let Some(b) = w.next_boundary_after(&z, t).filter(|&b| b <= end) {
                predicted.push(b);
                t = b;
            }
            assert_eq!(predicted, changes, "{name} {w:?}");
            assert!(changes.len() >= 2, "{name} {w:?}: {changes:?}");
        }
    }
}
