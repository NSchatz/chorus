//! Goal 13's room settings in the catalog: the A/V trim (`av_trim`), the TV
//! upmix (`sound`'s `tv_upmix`) and an autoplay rule's TV behaviour
//! (`stop_on_standby`, `low_latency`), through the model, the state message
//! and the state file (format 4), and the relay's lead rule
//! (`theater::tv_play_at_lead_ns`). The byte-for-byte vectors are under
//! `fixtures/control/v2/` (`av_trim*`, `sound-tv-upmix`, `autoplay-tv`,
//! `error-av-trim-*`, `error-sound-tv-upmix`, `state-rich`) and run in
//! `tests/catalog_v2.rs`.

use chorus_control::catalog::{decode_command, Command};
use chorus_control::json::{self, Value};
use chorus_control::persist::{load, render, STATE_FORMAT};
use chorus_control::theater::{tv_play_at_lead_ns, TvUpmix, AV_TRIM_MS};
use chorus_control::zones::{Zone, Zones};

const MS: u64 = 1_000_000;

fn apply(zones: &mut Zones, text: &str) {
    let command = decode_command(text).unwrap_or_else(|e| panic!("{}: {}", text, e));
    zones
        .apply(&command)
        .unwrap_or_else(|e| panic!("{}: {}", text, e));
}

fn refused(zones: &mut Zones, text: &str, field: &str) {
    let before = zones.encode_state();
    let refusal = match decode_command(text) {
        Err(r) => r,
        Ok(command) => zones
            .apply(&command)
            .expect_err(&format!("'{}' has to be refused", text)),
    };
    assert_eq!(refusal.field, field, "{}: {}", text, refusal);
    assert_eq!(zones.encode_state(), before, "'{}' moved the state", text);
}

fn house() -> Zones {
    let mut zones = Zones::new("127.0.0.1:4010");
    zones.add(Zone::new("living")).unwrap();
    zones.add(Zone::new("kitchen")).unwrap();
    zones
}

fn zone_state(zones: &Zones, id: &str) -> Value {
    let state = json::parse(&zones.encode_state()).unwrap();
    match state.get("zones") {
        Some(Value::Arr(items)) => items
            .iter()
            .find(|z| z.get("id").and_then(Value::as_str) == Some(id))
            .cloned()
            .expect("the room is in the state"),
        _ => panic!("no zones"),
    }
}

#[test]
fn a_room_starts_with_no_trim_and_the_upmix_off() {
    let zones = house();
    let z = zones.zone("living").unwrap();
    assert_eq!(z.av_trim_ms, 0);
    assert_eq!(z.sound.tv_upmix, TvUpmix::Off);
    let state = zone_state(&zones, "living");
    assert_eq!(json::write(state.get("av_trim_ms").unwrap()), "0");
    assert_eq!(
        json::write(state.get("sound").unwrap().get("tv_upmix").unwrap()),
        r#""off""#
    );
}

#[test]
fn av_trim_sets_the_room_and_its_bounds_are_the_catalogs() {
    let mut zones = house();
    for ms in [AV_TRIM_MS.0, -1, 0, 1, AV_TRIM_MS.1] {
        apply(
            &mut zones,
            &format!(
                r#"{{"v":2,"t":"av_trim","zone":"living","av_trim_ms":{}}}"#,
                ms
            ),
        );
        assert_eq!(zones.zone("living").unwrap().av_trim_ms, ms);
        assert_eq!(
            json::write(zone_state(&zones, "living").get("av_trim_ms").unwrap()),
            ms.to_string()
        );
    }
    assert_eq!(
        zones.zone("kitchen").unwrap().av_trim_ms,
        0,
        "only its room"
    );
    for bad in [
        r#"{"v":2,"t":"av_trim","zone":"living","av_trim_ms":-101}"#,
        r#"{"v":2,"t":"av_trim","zone":"living","av_trim_ms":201}"#,
        r#"{"v":2,"t":"av_trim","zone":"living","av_trim_ms":1.5}"#,
        r#"{"v":2,"t":"av_trim","zone":"living","av_trim_ms":"10"}"#,
        r#"{"v":2,"t":"av_trim","zone":"living"}"#,
    ] {
        refused(&mut zones, bad, "av_trim_ms");
    }
    refused(
        &mut zones,
        r#"{"v":1,"t":"av_trim","zone":"living","av_trim_ms":10}"#,
        "t",
    );
    refused(
        &mut zones,
        r#"{"v":2,"t":"av_trim","zone":"attic","av_trim_ms":10}"#,
        "zone",
    );
}

#[test]
fn tv_upmix_is_a_partial_sound_field() {
    let mut zones = house();
    apply(
        &mut zones,
        r#"{"v":2,"t":"sound","zone":"living","bass":2}"#,
    );
    apply(
        &mut zones,
        r#"{"v":2,"t":"sound","zone":"living","tv_upmix":"ambient"}"#,
    );
    let s = zones.zone("living").unwrap().sound;
    assert_eq!((s.bass, s.tv_upmix), (2, TvUpmix::Ambient), "bass kept");
    apply(
        &mut zones,
        r#"{"v":2,"t":"sound","zone":"living","night":true}"#,
    );
    assert_eq!(
        zones.zone("living").unwrap().sound.tv_upmix,
        TvUpmix::Ambient
    );
    refused(
        &mut zones,
        r#"{"v":2,"t":"sound","zone":"living","tv_upmix":"wide"}"#,
        "tv_upmix",
    );
    refused(
        &mut zones,
        r#"{"v":2,"t":"sound","zone":"living","tv_upmix":true}"#,
        "tv_upmix",
    );
}

#[test]
fn an_autoplay_rules_tv_fields_default_to_true_and_are_written_only_when_false() {
    let plain = decode_command(
        r#"{"v":2,"t":"autoplay","input":"hub/tv","target":"living","enabled":true}"#,
    )
    .unwrap();
    match &plain {
        Command::Autoplay(rule) => assert!(rule.stop_on_standby && rule.low_latency),
        other => panic!("{:?}", other),
    }
    assert!(!plain.encode().contains("stop_on_standby"));
    let held = decode_command(
        r#"{"v":2,"t":"autoplay","input":"hub/tv","target":"living","enabled":true,"stop_on_standby":false}"#,
    )
    .unwrap();
    match &held {
        Command::Autoplay(rule) => assert!(!rule.stop_on_standby && rule.low_latency),
        other => panic!("{:?}", other),
    }
    assert!(decode_command(
        r#"{"v":2,"t":"autoplay","input":"hub/tv","target":"living","enabled":true,"stop_on_standby":1}"#,
    )
    .is_err());
}

#[test]
fn everything_goal_13_holds_survives_the_state_file_as_format_4() {
    let mut zones = house();
    for text in [
        r#"{"v":2,"t":"av_trim","zone":"living","av_trim_ms":-35}"#,
        r#"{"v":2,"t":"sound","zone":"living","tv_upmix":"ambient"}"#,
        r#"{"v":2,"t":"autoplay","input":"hub/tv","target":"living","enabled":true,"stop_on_standby":false,"low_latency":false}"#,
    ] {
        apply(&mut zones, text);
    }
    let text = render(&zones);
    // Format 5 (goal 14) still holds every format 4 field, in its place.
    assert_eq!(STATE_FORMAT, 6);
    assert!(text.contains("format = 6\n"));
    assert!(
        text.contains("tv_upmix = ambient\nav_trim_ms = -35\n"),
        "{}",
        text
    );
    assert!(
        text.contains("stop_on_standby = 0\nlow_latency = 0\n"),
        "{}",
        text
    );
    let back = load(&text, "127.0.0.1:4010").expect("it reads back");
    assert_eq!(render(&back), text, "byte for byte");
    let z = back.zone("living").unwrap();
    assert_eq!((z.av_trim_ms, z.sound.tv_upmix), (-35, TvUpmix::Ambient));
    let rule = &back.autoplay_rules()[0];
    assert!(!rule.stop_on_standby && !rule.low_latency);
}

#[test]
fn a_format_3_file_loads_unchanged_with_the_goal_13_defaults() {
    // What goal 12's build wrote: format 3, no TV fields anywhere.
    let mut zones = house();
    apply(
        &mut zones,
        r#"{"v":2,"t":"autoplay","input":"hub/tv","target":"living","enabled":true}"#,
    );
    let four = render(&zones);
    let three = four
        .replace("format = 6\n", "format = 3\n")
        .replace("tv_upmix = off\n", "")
        .replace("av_trim_ms = 0\n", "")
        .replace("stop_on_standby = 1\n", "")
        .replace("low_latency = 1\n", "");
    assert!(!three.contains("tv_upmix") && !three.contains("stop_on_standby"));
    let back = load(&three, "127.0.0.1:4010").expect("format 3 still loads");
    let z = back.zone("living").unwrap();
    assert_eq!((z.av_trim_ms, z.sound.tv_upmix), (0, TvUpmix::Off));
    let rule = &back.autoplay_rules()[0];
    assert!(rule.stop_on_standby && rule.low_latency);
    // The next write is the current format, and it is the same state.
    assert_eq!(render(&back), four);
}

#[test]
fn a_format_4_file_missing_or_breaking_a_tv_field_is_refused_not_defaulted() {
    let mut zones = house();
    apply(
        &mut zones,
        r#"{"v":2,"t":"autoplay","input":"hub/tv","target":"living","enabled":true}"#,
    );
    let text = render(&zones);
    for (from, to, says) in [
        ("tv_upmix = off\n", "", "has no 'tv_upmix'"),
        ("av_trim_ms = 0\n", "", "has no 'av_trim_ms'"),
        ("tv_upmix = off\n", "tv_upmix = wide\n", "not a TV upmix"),
        (
            "av_trim_ms = 0\n",
            "av_trim_ms = 201\n",
            "av_trim_ms = '201'",
        ),
        ("av_trim_ms = 0\n", "av_trim_ms = +5\n", "av_trim_ms = '+5'"),
        ("stop_on_standby = 1\n", "", "has no 'stop_on_standby'"),
        (
            "low_latency = 1\n",
            "low_latency = 2\n",
            "low_latency = '2'",
        ),
    ] {
        let broken = text.replacen(from, to, 1);
        let err = load(&broken, "x").unwrap_err();
        assert!(err.to_string().contains(says), "{}: {}", says, err);
    }
}

#[test]
fn the_relay_lead_is_the_tv_latency_moved_by_the_trim_and_clamped_at_the_floor() {
    // The design envelope's example L_tv (30 ms, ASSUMED) and a 10 ms floor.
    let (l_tv, floor) = (30 * MS, 10 * MS);
    assert_eq!(tv_play_at_lead_ns(l_tv, floor, 0), (30 * MS, false));
    assert_eq!(
        tv_play_at_lead_ns(l_tv, floor, AV_TRIM_MS.1),
        (230 * MS, false)
    );
    assert_eq!(tv_play_at_lead_ns(l_tv, floor, -20), (10 * MS, false));
    assert_eq!(tv_play_at_lead_ns(l_tv, floor, -21), (10 * MS, true));
    assert_eq!(
        tv_play_at_lead_ns(l_tv, floor, AV_TRIM_MS.0),
        (10 * MS, true)
    );
    // Every trim the catalog takes: monotonic in the trim, never below the
    // floor, and clamped exactly when L_tv + trim < floor.
    let mut last = 0;
    for ms in AV_TRIM_MS.0..=AV_TRIM_MS.1 {
        let (lead, clamped) = tv_play_at_lead_ns(l_tv, floor, ms);
        assert!(lead >= floor && lead >= last);
        assert_eq!(
            clamped,
            (l_tv as i64 + i64::from(ms) * MS as i64) < floor as i64
        );
        last = lead;
    }
}
