//! Per-room sound in catalog v2 (goal 12, done-when line B): `sound`,
//! `bass_management` and `room_eq` change only what they name, are refused by
//! field name outside their ranges, leave every volume path's clamp exactly as
//! it was, are shown in the state, and survive a restart (state-file format
//! 3), while a format 2 file (goal 11's) still loads with the defaults.
//!
//! The wire vectors are `catalog_v2.rs`'s, discovered under
//! `fixtures/control/v2/`; this file holds the room model's rules.

use chorus_control::catalog::{decode_command, Volume};
use chorus_control::json::{self, Value};
use chorus_control::persist::{load, render, STATE_FORMAT};
use chorus_control::sound::{Polarity, SoundSettings};
use chorus_control::zones::{Zone, Zones};
use chorus_control::{ROOM_EQ_FREQ_HZ, ROOM_EQ_GAIN_CDB, ROOM_EQ_MAX_FILTERS, ROOM_EQ_Q_MILLI};

fn apply(zones: &mut Zones, text: &str) {
    let command = decode_command(text).unwrap_or_else(|e| panic!("{}: {}", text, e));
    zones
        .apply(&command)
        .unwrap_or_else(|e| panic!("{}: {}", text, e));
}

/// Offer `text`, require a refusal naming `field`, and the state unchanged.
fn refused(zones: &mut Zones, text: &str, field: &str) -> String {
    let before = zones.encode_state();
    let refusal = match decode_command(text) {
        Err(r) => r,
        Ok(command) => zones
            .apply(&command)
            .expect_err(&format!("'{}' has to be refused", text)),
    };
    assert_eq!(refusal.field, field, "{}: {}", text, refusal);
    assert_eq!(zones.encode_state(), before, "'{}' moved the state", text);
    refusal.detail
}

/// Two rooms; the living room has a wired 2.1 set.
fn house() -> Zones {
    let mut zones = Zones::new("127.0.0.1:4010");
    zones.add(Zone::new("living")).unwrap();
    zones.add(Zone::new("kitchen")).unwrap();
    for e in ["endpoint-a", "endpoint-b", "endpoint-c"] {
        apply(
            &mut zones,
            &format!(
                r#"{{"v":2,"t":"attach","zone":"living","endpoint":"{}","link":"wired"}}"#,
                e
            ),
        );
    }
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
fn a_room_starts_flat_with_loudness_on_and_bass_management_at_80_hz() {
    let zones = house();
    let z = zones.zone("kitchen").unwrap();
    assert_eq!(z.sound, SoundSettings::default());
    assert_eq!((z.sound.bass, z.sound.treble), (0, 0));
    assert!(z.sound.loudness && !z.sound.night && !z.sound.speech);
    assert_eq!(z.bass.crossover_hz, 80);
    assert_eq!(z.bass.sub_level_cdb, 0);
    assert_eq!(z.bass.sub_polarity, Polarity::Normal);
    assert!(z.room_eq.enabled && z.room_eq.filters.is_empty());
    assert_eq!(
        json::write(zone_state(&zones, "kitchen").get("sound").unwrap()),
        // Goal 13 added `tv_upmix` (its own tests: tests/theater_v2.rs).
        r#"{"bass":0,"treble":0,"loudness":true,"night":false,"speech":false,"tv_upmix":"off"}"#
    );
}

#[test]
fn a_partial_update_changes_only_the_fields_it_carries() {
    let mut zones = house();
    apply(
        &mut zones,
        r#"{"v":2,"t":"sound","zone":"living","bass":4,"treble":-3}"#,
    );
    apply(
        &mut zones,
        r#"{"v":2,"t":"sound","zone":"living","night":true}"#,
    );
    let s = zones.zone("living").unwrap().sound;
    assert_eq!(
        (s.bass, s.treble, s.loudness, s.night, s.speech),
        (4, -3, true, true, false)
    );
    apply(
        &mut zones,
        r#"{"v":2,"t":"bass_management","zone":"living","sub_polarity":"inverted"}"#,
    );
    apply(
        &mut zones,
        r#"{"v":2,"t":"bass_management","zone":"living","sub_level_db":-2.5}"#,
    );
    let b = zones.zone("living").unwrap().bass;
    assert_eq!(
        (b.crossover_hz, b.sub_level_cdb, b.sub_polarity),
        (80, -250, Polarity::Inverted)
    );
    apply(
        &mut zones,
        r#"{"v":2,"t":"room_eq","zone":"living","filters":[{"freq_hz":55,"gain_db":-4.25,"q":3.000}]}"#,
    );
    apply(
        &mut zones,
        r#"{"v":2,"t":"room_eq","zone":"living","enabled":false}"#,
    );
    let eq = &zones.zone("living").unwrap().room_eq;
    assert_eq!(eq.filters.len(), 1, "disabling keeps the filters");
    assert!(!eq.enabled);
    apply(
        &mut zones,
        r#"{"v":2,"t":"room_eq","zone":"living","filters":[]}"#,
    );
    assert!(
        zones.zone("living").unwrap().room_eq.filters.is_empty(),
        "[] clears"
    );
    // The other room is untouched by all of it.
    assert_eq!(
        zones.zone("kitchen").unwrap().sound,
        SoundSettings::default()
    );
}

#[test]
fn every_out_of_range_value_is_refused_by_its_field_and_applies_nothing() {
    let mut zones = house();
    for (text, field) in [
        (r#"{"v":2,"t":"sound","zone":"living","bass":-11}"#, "bass"),
        (
            r#"{"v":2,"t":"sound","zone":"living","treble":11}"#,
            "treble",
        ),
        (
            r#"{"v":2,"t":"sound","zone":"living","night":"yes"}"#,
            "night",
        ),
        (
            r#"{"v":2,"t":"sound","zone":"living","speech":0}"#,
            "speech",
        ),
        (
            r#"{"v":2,"t":"sound","zone":"living","volume":0.5}"#,
            "volume",
        ),
        (r#"{"v":2,"t":"sound","zone":"nowhere","bass":1}"#, "zone"),
        (
            r#"{"v":1,"t":"room_eq","zone":"living","enabled":true}"#,
            "t",
        ),
        (
            r#"{"v":2,"t":"bass_management","zone":"living","crossover_hz":201}"#,
            "crossover_hz",
        ),
        (
            r#"{"v":2,"t":"bass_management","zone":"living","sub_level_db":-12.01}"#,
            "sub_level_db",
        ),
        (
            r#"{"v":2,"t":"bass_management","zone":"living","sub_level_db":1.005}"#,
            "sub_level_db",
        ),
        (
            r#"{"v":2,"t":"room_eq","zone":"living","filters":[{"freq_hz":19,"gain_db":-1.00,"q":1.000}]}"#,
            "filters",
        ),
        (
            r#"{"v":2,"t":"room_eq","zone":"living","filters":[{"freq_hz":100,"gain_db":-12.01,"q":1.000}]}"#,
            "filters",
        ),
        (
            r#"{"v":2,"t":"room_eq","zone":"living","filters":[{"freq_hz":100,"gain_db":-1.00,"q":10.001}]}"#,
            "filters",
        ),
        (
            r#"{"v":2,"t":"room_eq","zone":"living","filters":[{"freq_hz":100,"gain_db":-1.00}]}"#,
            "filters",
        ),
        (
            r#"{"v":2,"t":"room_eq","zone":"living","enabled":1}"#,
            "enabled",
        ),
    ] {
        refused(&mut zones, text, field);
    }
}

#[test]
fn the_room_correction_bounds_are_exported_and_are_what_the_catalog_accepts() {
    assert_eq!(ROOM_EQ_MAX_FILTERS, 8);
    assert_eq!(ROOM_EQ_FREQ_HZ, (20, 1000));
    assert_eq!(ROOM_EQ_GAIN_CDB, (-1200, 300));
    assert_eq!(ROOM_EQ_Q_MILLI, (500, 10_000));
    let mut zones = house();
    let edges: Vec<String> = (0..ROOM_EQ_MAX_FILTERS)
        .map(|i| {
            if i % 2 == 0 {
                r#"{"freq_hz":20,"gain_db":-12.00,"q":0.500}"#.to_string()
            } else {
                r#"{"freq_hz":1000,"gain_db":3.00,"q":10.000}"#.to_string()
            }
        })
        .collect();
    apply(
        &mut zones,
        &format!(
            r#"{{"v":2,"t":"room_eq","zone":"living","filters":[{}]}}"#,
            edges.join(",")
        ),
    );
    assert_eq!(zones.zone("living").unwrap().room_eq.filters.len(), 8);
}

#[test]
fn bass_management_is_active_exactly_when_the_set_has_a_sub() {
    let mut zones = house();
    let active = |zones: &Zones| {
        zone_state(zones, "living")
            .get("bass_management")
            .and_then(|b| b.get("active"))
            .and_then(Value::as_bool)
            .unwrap()
    };
    assert!(!active(&zones), "no set");
    apply(
        &mut zones,
        r#"{"v":2,"t":"bond","zone":"living","members":[{"endpoint":"endpoint-a","role":"FL"},{"endpoint":"endpoint-b","role":"FR"}]}"#,
    );
    assert!(!active(&zones), "a stereo pair has no sub");
    apply(
        &mut zones,
        r#"{"v":2,"t":"bond","zone":"living","members":[{"endpoint":"endpoint-a","role":"FL"},{"endpoint":"endpoint-b","role":"FR"},{"endpoint":"endpoint-c","role":"LFE"}]}"#,
    );
    assert!(active(&zones), "2.1");
    apply(&mut zones, r#"{"v":2,"t":"unbond","zone":"living"}"#);
    assert!(!active(&zones));
}

#[test]
fn sound_moves_no_volume_and_every_volume_path_keeps_its_clamp() {
    let mut zones = house();
    apply(
        &mut zones,
        r#"{"v":2,"t":"limit","zone":"living","limit":0.400}"#,
    );
    apply(
        &mut zones,
        r#"{"v":1,"t":"volume","zone":"living","volume":0.900}"#,
    );
    assert_eq!(
        zones.zone("living").unwrap().volume,
        Volume::from_thousandths(400).unwrap()
    );
    // A boost in tone is not a volume path: volume, limit and effective limit
    // stay exactly where the clamp put them (the endpoint's limiter holds the
    // DSP's own boosts under the limit, ADR 0074 and the DSP envelope).
    for text in [
        r#"{"v":2,"t":"sound","zone":"living","bass":10,"treble":10,"loudness":true}"#,
        r#"{"v":2,"t":"bass_management","zone":"living","sub_level_db":6.00}"#,
        r#"{"v":2,"t":"room_eq","zone":"living","filters":[{"freq_hz":100,"gain_db":3.00,"q":1.000}]}"#,
    ] {
        apply(&mut zones, text);
        let z = zones.zone("living").unwrap();
        assert_eq!(z.volume.literal(), "0.400", "{}", text);
        assert_eq!(z.limit.literal(), "0.400", "{}", text);
        assert_eq!(z.effective_limit().literal(), "0.400", "{}", text);
    }
    apply(
        &mut zones,
        r#"{"v":2,"t":"volume_step","zone":"living","step":500}"#,
    );
    assert_eq!(zones.zone("living").unwrap().volume.literal(), "0.400");
}

#[test]
fn everything_sound_holds_survives_the_state_file_as_format_3() {
    let mut zones = house();
    for text in [
        r#"{"v":2,"t":"sound","zone":"living","bass":-4,"treble":2,"loudness":false,"night":true,"speech":true}"#,
        r#"{"v":2,"t":"bass_management","zone":"living","crossover_hz":120,"sub_level_db":-1.75,"sub_polarity":"inverted"}"#,
        r#"{"v":2,"t":"room_eq","zone":"living","filters":[{"freq_hz":42,"gain_db":-6.00,"q":4.500},{"freq_hz":120,"gain_db":-3.25,"q":2.000}],"enabled":false}"#,
    ] {
        apply(&mut zones, text);
    }
    let text = render(&zones);
    // Format 5 (goal 14) still holds every format 3 field, in its place.
    assert_eq!(STATE_FORMAT, 6);
    assert!(text.contains("format = 6\n"), "{}", text);
    assert!(text.contains(
        "bass = -4\ntreble = 2\nloudness = 0\nnight = 1\nspeech = 1\ncrossover_hz = 120\n\
         sub_level_db = -1.75\nsub_polarity = inverted\nroom_eq = 0\n\
         room_eq_filters = 42 -6.00 4.500; 120 -3.25 2.000\n"
    ));
    let back = load(&text, "127.0.0.1:4010").expect("it reads back");
    assert_eq!(render(&back), text, "byte for byte");
    let z = back.zone("living").unwrap();
    assert_eq!(z.sound, zones.zone("living").unwrap().sound);
    assert_eq!(z.bass, zones.zone("living").unwrap().bass);
    assert_eq!(z.room_eq, zones.zone("living").unwrap().room_eq);
}

/// A file as goal 11's build wrote it (format 2), verbatim.
const FORMAT_2: &str = "# chorus zone state, written by chorus-server.\n\
format = 2\n\
serial = 5\n\
\n\
[zone living]\n\
name = Living\n\
group = living\n\
volume = 0.500\n\
muted = 0\n\
endpoints = endpoint-a,endpoint-b\n\
limit = 0.800\n\
quiet =\n\
bond = FL:endpoint-a,FR:endpoint-b\n\
\n\
[endpoint endpoint-a]\n\
link = wired\n\
\n\
[endpoint endpoint-b]\n\
link = wired\n";

#[test]
fn a_format_2_file_loads_unchanged_with_the_sound_defaults() {
    let zones = load(FORMAT_2, "127.0.0.1:4010").expect("format 2 still loads");
    let z = zones.zone("living").unwrap();
    assert_eq!(z.volume.literal(), "0.500");
    assert_eq!(z.limit.literal(), "0.800");
    assert_eq!(z.bond.len(), 2);
    assert_eq!(z.sound, SoundSettings::default());
    assert_eq!(z.bass.crossover_hz, 80);
    assert!(z.room_eq.filters.is_empty() && z.room_eq.enabled);
    let again = render(&zones);
    // The next write is the current format (5 since goal 14).
    assert!(again.contains("format = 6\n"));
    assert_eq!(render(&load(&again, "x").unwrap()), again);
}

#[test]
fn a_format_3_zone_missing_or_breaking_a_sound_field_is_refused_not_defaulted() {
    let text = render(&house());
    let missing = text.replacen("speech = 0\n", "", 1);
    let err = load(&missing, "x").unwrap_err();
    assert!(err.to_string().contains("has no 'speech'"), "{}", err);
    let boost = text.replacen(
        "room_eq_filters = \n",
        "room_eq_filters = 100 4.00 1.000\n",
        1,
    );
    let err = load(&boost, "x").unwrap_err();
    assert!(err.to_string().contains("gain_db"), "{}", err);
    let bass = text.replacen("bass = 0\n", "bass = 11\n", 1);
    let err = load(&bass, "x").unwrap_err();
    assert!(err.to_string().contains("bass = '11'"), "{}", err);
}
