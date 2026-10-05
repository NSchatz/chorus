//! The `measure_sweep` command in the catalog and the room model (ADR 0000):
//! which messages decode, what the model refuses by name (an unknown room, a
//! room measuring, a room no speaker is attached to, a muted room, a room an
//! alarm rings in), what starting a sweep changes and what its end puts
//! back, and how the state names it. The byte-for-byte vectors are under
//! `fixtures/control/v2/` (`measure_sweep`, `measure_sweep-volume`,
//! `state-measuring`, `state-measured` and six refusals) and run in
//! `tests/catalog_v2.rs`; the real server is
//! `crates/server/tests/measure_sweep.rs`.

use chorus_control::catalog::{decode_command, decode_message, Command, Volume};
use chorus_control::rooms::{CivilTime, ClockTime, Source};
use chorus_control::zones::{MeasurementState, Zone, Zones};

/// The lengths a server plays: the lead silence, the sweep, the tail.
const LENGTHS: (u64, u64, u64) = (500, 5_000, 1_000);

fn v(thousandths: i64) -> Volume {
    Volume::from_thousandths(thousandths).unwrap()
}

fn apply(zones: &mut Zones, text: &str) {
    let command = decode_command(text).unwrap_or_else(|e| panic!("{}: {}", text, e));
    zones
        .apply(&command)
        .unwrap_or_else(|e| panic!("{}: {}", text, e));
}

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

fn sweep(zone: &str) -> String {
    format!(r#"{{"v":2,"t":"measure_sweep","zone":"{}"}}"#, zone)
}

/// Three rooms; the kitchen and the living room each have a speaker.
fn house() -> Zones {
    let mut zones = Zones::new("127.0.0.1:4010");
    for id in ["living", "kitchen", "attic"] {
        zones.add(Zone::new(id)).unwrap();
    }
    apply(
        &mut zones,
        r#"{"v":1,"t":"attach","zone":"kitchen","endpoint":"endpoint-a"}"#,
    );
    apply(
        &mut zones,
        r#"{"v":1,"t":"attach","zone":"living","endpoint":"endpoint-b"}"#,
    );
    zones
}

#[test]
fn measure_sweep_is_a_v2_command_with_an_optional_volume() {
    assert_eq!(
        decode_command(&sweep("kitchen")).unwrap(),
        Command::MeasureSweep {
            zone: "kitchen".to_string(),
            volume: None,
        }
    );
    let with_volume = r#"{"v":2,"t":"measure_sweep","zone":"kitchen","volume":0.300}"#;
    let command = decode_command(with_volume).unwrap();
    assert_eq!(
        command,
        Command::MeasureSweep {
            zone: "kitchen".to_string(),
            volume: Some(v(300)),
        }
    );
    assert_eq!(command.encode(), with_volume);
    assert_eq!(command.type_name(), "measure_sweep");
    // Not a command of catalog version 1, and no field it does not declare.
    let at_v1 = decode_message(&sweep("kitchen").replace(r#""v":2"#, r#""v":1"#))
        .expect_err("a v2-only command at v1");
    assert_eq!(at_v1.field, "t", "{}", at_v1);
    let extra = r#"{"v":2,"t":"measure_sweep","zone":"kitchen","repeat":3}"#;
    assert_eq!(decode_command(extra).unwrap_err().field, "repeat");
    let loud = r#"{"v":2,"t":"measure_sweep","zone":"kitchen","volume":1.001}"#;
    assert_eq!(decode_command(loud).unwrap_err().field, "volume");
}

#[test]
fn the_room_model_refuses_a_sweep_by_name_and_changes_nothing() {
    let mut zones = house();
    // The check changes nothing but the serial, as every applied command.
    apply(&mut zones, &sweep("kitchen"));
    assert_eq!(zones.measurement(), None);

    let detail = refused(&mut zones, &sweep("garage"), "zone");
    assert!(detail.contains("there is no zone 'garage'"), "{}", detail);
    let detail = refused(&mut zones, &sweep("attic"), "zone");
    assert!(detail.starts_with("no-speaker:"), "{}", detail);

    apply(
        &mut zones,
        r#"{"v":1,"t":"mute","zone":"kitchen","muted":true}"#,
    );
    let detail = refused(&mut zones, &sweep("kitchen"), "zone");
    assert!(detail.starts_with("muted:"), "{}", detail);
    apply(
        &mut zones,
        r#"{"v":1,"t":"mute","zone":"kitchen","muted":false}"#,
    );

    // An alarm ringing in the room, or in a room that shares its group.
    apply(
        &mut zones,
        r#"{"v":2,"t":"join","zone":"living","target":"kitchen"}"#,
    );
    apply(
        &mut zones,
        r#"{"v":2,"t":"alarm_set","alarm":"wake","target":"kitchen","time":"07:00","days":[],"source":"chime:bell","volume":0.500,"ramp_s":0,"duration_min":5,"enabled":true}"#,
    );
    assert_eq!(zones.measure_watch("kitchen"), None);
    zones.set_alarm_ringing("wake", true).unwrap();
    for room in ["kitchen", "living"] {
        let detail = refused(&mut zones, &sweep(room), "zone");
        assert!(
            detail.starts_with("alarm-ringing: alarm 'wake'"),
            "{}",
            detail
        );
        assert_eq!(zones.measure_watch(room).as_deref(), Some("wake"));
    }
    zones.set_alarm_ringing("wake", false).unwrap();
    apply(&mut zones, &sweep("living"));

    // A room that is measuring, and any room while one is.
    zones.measure_begin("kitchen", None, LENGTHS).unwrap();
    let detail = refused(&mut zones, &sweep("kitchen"), "zone");
    assert!(
        detail.starts_with("measuring: room 'kitchen' is playing measurement sweep 1 already"),
        "{}",
        detail
    );
    let detail = refused(&mut zones, &sweep("living"), "zone");
    assert!(
        detail.starts_with("measuring: room 'kitchen' is playing measurement sweep 1 and"),
        "{}",
        detail
    );
    let before = zones.encode_state();
    assert_eq!(
        zones
            .measure_begin("living", None, LENGTHS)
            .unwrap_err()
            .field,
        "zone"
    );
    assert_eq!(zones.encode_state(), before);
}

#[test]
fn a_sweeps_volume_is_clamped_to_the_effective_limit_and_put_back_afterwards() {
    let mut zones = house();
    apply(
        &mut zones,
        r#"{"v":2,"t":"take","target":"kitchen","source":"chime:bell"}"#,
    );
    apply(
        &mut zones,
        r#"{"v":1,"t":"volume","zone":"kitchen","volume":0.250}"#,
    );
    apply(
        &mut zones,
        r#"{"v":2,"t":"limit","zone":"kitchen","limit":0.400}"#,
    );
    // Asked for more than the room's limit: clamped, like every volume path.
    let first = zones
        .measure_begin("kitchen", Some(v(900)), LENGTHS)
        .unwrap();
    assert_eq!((first.id, first.before, first.set), (1, v(250), v(400)));
    assert_eq!(zones.zone("kitchen").unwrap().volume, v(400));
    let playing = zones.measurement().unwrap().clone();
    assert_eq!(playing.state, MeasurementState::Playing);
    assert_eq!((playing.zone.as_str(), playing.volume), ("kitchen", v(400)));
    // No source and no group changed.
    assert_eq!(zones.source("kitchen"), Source::Chime("bell".to_string()));
    assert_eq!(zones.zone("kitchen").unwrap().group, "kitchen");

    // The end puts the volume back, once; a sweep that is over ends no more.
    assert!(!zones.measure_end(7, MeasurementState::Finished, None, v(250), v(400)));
    assert!(zones.measure_end(1, MeasurementState::Finished, None, first.before, first.set));
    assert_eq!(zones.zone("kitchen").unwrap().volume, v(250));
    assert_eq!(
        zones.measurement().unwrap().state,
        MeasurementState::Finished
    );
    assert!(!zones.measure_end(1, MeasurementState::Finished, None, first.before, first.set));

    // Inside a quiet window the cap is the window's, below the room's limit.
    apply(
        &mut zones,
        r#"{"v":2,"t":"quiet_hours","zone":"kitchen","windows":[{"days":["mon"],"start":"22:00","end":"07:00","limit":0.100}]}"#,
    );
    zones.set_civil_time(Some(CivilTime {
        weekday: 0,
        time: ClockTime::parse("23:00").unwrap(),
    }));
    assert_eq!(zones.zone("kitchen").unwrap().volume, v(100));
    let second = zones
        .measure_begin("kitchen", Some(v(1000)), LENGTHS)
        .unwrap();
    assert_eq!((second.id, second.before, second.set), (2, v(100), v(100)));
    // With no volume the room keeps its own and the sweep plays at that.
    assert!(zones.measure_end(
        2,
        MeasurementState::Cancelled,
        Some("alarm 'wake' rings in the room".to_string()),
        second.before,
        second.set
    ));
    let third = zones.measure_begin("kitchen", None, LENGTHS).unwrap();
    assert_eq!((third.before, third.set), (v(100), v(100)));
    assert_eq!(zones.measurement().unwrap().volume, v(100));

    // A volume somebody changed while the sweep played is left alone.
    let mut zones = house();
    let lent = zones
        .measure_begin("kitchen", Some(v(300)), LENGTHS)
        .unwrap();
    apply(
        &mut zones,
        r#"{"v":1,"t":"volume","zone":"kitchen","volume":0.200}"#,
    );
    assert!(zones.measure_end(1, MeasurementState::Finished, None, lent.before, lent.set));
    assert_eq!(zones.zone("kitchen").unwrap().volume, v(200));
}

#[test]
fn the_state_names_the_sweep_from_its_start_and_says_how_it_ended() {
    let mut zones = house();
    assert!(!zones.encode_state().contains("measurement"));
    let m = zones
        .measure_begin("kitchen", Some(v(300)), LENGTHS)
        .unwrap();
    assert!(
        zones.encode_state().ends_with(
            r#","measurement":{"id":1,"zone":"kitchen","state":"playing","volume":0.300,"lead_ms":500,"sweep_ms":5000,"tail_ms":1000}}"#
        ),
        "{}",
        zones.encode_state()
    );
    assert!(zones.measure_end(
        m.id,
        MeasurementState::Cancelled,
        Some("alarm 'wake' rings in the room".to_string()),
        m.before,
        m.set
    ));
    assert!(
        zones.encode_state().ends_with(
            r#""state":"cancelled","volume":0.300,"lead_ms":500,"sweep_ms":5000,"tail_ms":1000,"reason":"alarm 'wake' rings in the room"}}"#
        ),
        "{}",
        zones.encode_state()
    );
    assert_eq!(MeasurementState::Finished.name(), "finished");
    // The v1 shape of the state never carries it.
    assert!(!zones.encode_state_at(1).contains("measurement"));
}
