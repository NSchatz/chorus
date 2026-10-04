//! The voice path's two room facts in the control catalog: `voice_enabled`
//! (K73's software gate: a command, a state field, persisted) and `mic_muted`
//! (what the room's microphones report of their hardware gate: read-only
//! state, a fact about now).
//!
//! Nothing here carries audio. What the server does with microphone audio is
//! `crates/server/tests/voice_intake.rs`.

use chorus_control::catalog::{decode_command, Command};
use chorus_control::json::{self, Value};
use chorus_control::persist::{self, load, render, write_file, STATE_FORMAT};
use chorus_control::zones::{Zone, Zones};

fn house() -> Zones {
    let mut zones = Zones::new("127.0.0.1:4010");
    for id in ["kitchen", "bedroom"] {
        zones.add(Zone::new(id)).unwrap();
    }
    zones
}

fn apply(zones: &mut Zones, text: &str) {
    let command = decode_command(text).unwrap_or_else(|e| panic!("{}: {}", text, e));
    zones
        .apply(&command)
        .unwrap_or_else(|e| panic!("{}: {}", text, e));
}

/// A room's `voice_enabled` and `mic_muted`, off the state message.
fn voice(zones: &Zones, id: &str) -> (bool, bool) {
    let state = json::parse(&zones.encode_state()).unwrap();
    let Some(Value::Arr(rooms)) = state.get("zones") else {
        panic!("no zones")
    };
    let room = rooms
        .iter()
        .find(|z| z.get("id").and_then(Value::as_str) == Some(id))
        .unwrap();
    (
        room.get("voice_enabled").and_then(Value::as_bool).unwrap(),
        room.get("mic_muted").and_then(Value::as_bool).unwrap(),
    )
}

/// A speaker with a microphone, adopted, in `room`, with a session up.
fn voice_speaker(zones: &mut Zones, id: &str, room: &str) {
    zones.speaker_adopted(id, "aa:bb").unwrap();
    apply(
        zones,
        &format!(r#"{{"v":2,"t":"speaker_room","speaker":"{id}","room":"{room}"}}"#),
    );
    zones.speaker_session_up(id, "test 1", &["player".to_string(), "voice".to_string()]);
}

#[test]
fn voice_enabled_is_a_v2_command_that_encodes_and_decodes_to_itself() {
    let text = r#"{"v":2,"t":"voice_enabled","zone":"kitchen","enabled":true}"#;
    let command = decode_command(text).unwrap();
    assert_eq!(
        command,
        Command::VoiceEnabled {
            zone: "kitchen".to_string(),
            enabled: true
        }
    );
    assert_eq!(command.type_name(), "voice_enabled");
    assert_eq!(command.encode(), text);
}

#[test]
fn voice_enabled_is_refused_by_name_when_it_is_malformed() {
    // No `enabled`.
    let e = decode_command(r#"{"v":2,"t":"voice_enabled","zone":"kitchen"}"#).unwrap_err();
    assert_eq!(e.field, "enabled", "{}", e);
    // Not a boolean: there is no third state.
    let e = decode_command(r#"{"v":2,"t":"voice_enabled","zone":"kitchen","enabled":"on"}"#)
        .unwrap_err();
    assert_eq!(e.field, "enabled", "{}", e);
    // A field the command does not declare (nothing here names a source).
    let e = decode_command(
        r#"{"v":2,"t":"voice_enabled","zone":"kitchen","enabled":true,"source":"mic"}"#,
    )
    .unwrap_err();
    assert_eq!(e.field, "source", "{}", e);
    // Catalog version 1 has no such command.
    let e = decode_command(r#"{"v":1,"t":"voice_enabled","zone":"kitchen","enabled":true}"#)
        .unwrap_err();
    assert_eq!(e.field, "t", "{}", e);
    // A room that does not exist.
    let mut zones = house();
    let command =
        decode_command(r#"{"v":2,"t":"voice_enabled","zone":"attic","enabled":true}"#).unwrap();
    let e = zones.apply(&command).unwrap_err();
    assert_eq!(e.field, "zone", "{}", e);
}

#[test]
fn voice_is_off_in_every_room_until_switched_on_and_the_state_says_so() {
    let mut zones = house();
    for room in ["kitchen", "bedroom"] {
        assert!(!zones.zone(room).unwrap().voice_enabled);
        assert_eq!(voice(&zones, room), (false, true), "{}", room);
    }
    let before = zones.serial();
    apply(
        &mut zones,
        r#"{"v":2,"t":"voice_enabled","zone":"kitchen","enabled":true}"#,
    );
    assert!(zones.serial() > before, "a change the subscribers are told");
    assert!(voice(&zones, "kitchen").0);
    assert!(!voice(&zones, "bedroom").0, "one room, not the house");
    apply(
        &mut zones,
        r#"{"v":2,"t":"voice_enabled","zone":"kitchen","enabled":false}"#,
    );
    assert!(!voice(&zones, "kitchen").0);
    // The v1 state shape is what it was: no voice field.
    assert!(!zones.encode_state_at(1).contains("voice"));
    assert!(!zones.encode_state_at(1).contains("mic"));
}

#[test]
fn mic_muted_follows_the_speakers_own_gate_and_no_command_sets_it() {
    let mut zones = house();
    voice_speaker(&mut zones, "kitchen-mic", "kitchen");
    // A microphone that has said nothing is muted.
    assert_eq!(voice(&zones, "kitchen"), (false, true));
    let before = zones.serial();
    assert!(zones.speaker_mic_gate("kitchen-mic", true));
    assert!(zones.serial() > before);
    assert_eq!(voice(&zones, "kitchen"), (false, false), "live");
    assert_eq!(voice(&zones, "bedroom"), (false, true), "another room");
    // Saying the same again changes nothing and tells nobody.
    let before = zones.serial();
    assert!(!zones.speaker_mic_gate("kitchen-mic", true));
    assert_eq!(zones.serial(), before);
    // The switch closes it.
    assert!(zones.speaker_mic_gate("kitchen-mic", false));
    assert!(voice(&zones, "kitchen").1);
    // A live gate does not outlast its session.
    zones.speaker_mic_gate("kitchen-mic", true);
    assert!(zones.speaker_session_down("kitchen-mic"));
    assert!(voice(&zones, "kitchen").1, "gone is muted");
    zones.speaker_session_up("kitchen-mic", "test 1", &["voice".to_string()]);
    assert!(
        voice(&zones, "kitchen").1,
        "and back is muted until it says"
    );
    // An id the server does not list has no gate.
    assert!(!zones.speaker_mic_gate("nobody", true));
    // Switching voice on unmutes nothing: the hardware state is its own.
    apply(
        &mut zones,
        r#"{"v":2,"t":"voice_enabled","zone":"kitchen","enabled":true}"#,
    );
    assert_eq!(voice(&zones, "kitchen"), (true, true));
    // And no command names `mic_muted`: the catalog has none to set it.
    for text in [
        r#"{"v":2,"t":"mic_muted","zone":"kitchen","muted":false}"#,
        r#"{"v":2,"t":"mic_mute","zone":"kitchen","muted":false}"#,
        r#"{"v":2,"t":"voice_enabled","zone":"kitchen","enabled":true,"mic_muted":false}"#,
    ] {
        assert!(decode_command(text).is_err(), "{}", text);
    }
}

#[test]
fn a_room_is_live_when_any_of_its_present_microphones_is() {
    let mut zones = house();
    voice_speaker(&mut zones, "mic-a", "kitchen");
    voice_speaker(&mut zones, "mic-b", "kitchen");
    zones.speaker_mic_gate("mic-b", true);
    assert!(!voice(&zones, "kitchen").1);
    zones.speaker_mic_gate("mic-b", false);
    assert!(voice(&zones, "kitchen").1);
}

#[test]
fn voice_enabled_survives_a_restart_and_the_gate_does_not() {
    let directory = std::env::temp_dir().join(format!("chorus-state-voice-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("zones.state");
    let mut zones = house();
    voice_speaker(&mut zones, "kitchen-mic", "kitchen");
    zones.speaker_mic_gate("kitchen-mic", true);
    apply(
        &mut zones,
        r#"{"v":2,"t":"voice_enabled","zone":"kitchen","enabled":true}"#,
    );
    write_file(&path, &zones).expect("it writes");
    let saved = std::fs::read_to_string(&path).unwrap();
    assert_eq!(STATE_FORMAT, 8);
    assert!(saved.contains("format = 8\n"), "{}", saved);
    assert_eq!(saved.matches("voice_enabled = 1\n").count(), 1, "{}", saved);
    assert_eq!(saved.matches("voice_enabled = 0\n").count(), 1, "{}", saved);
    assert!(
        !saved.contains("mic_") && !saved.contains("gate") && !saved.contains("live"),
        "the gate is a fact about now: {}",
        saved
    );

    let back = persist::read_file(&path, "127.0.0.1:4010")
        .unwrap()
        .expect("it is there");
    std::fs::remove_dir_all(&directory).unwrap();
    assert_eq!(render(&back), saved, "the same file, byte for byte");
    assert!(back.zone("kitchen").unwrap().voice_enabled, "still on");
    assert!(!back.zone("bedroom").unwrap().voice_enabled, "the default");
    assert_eq!(voice(&back, "kitchen"), (true, true), "muted until it says");
}

#[test]
fn a_format_7_file_loads_with_voice_off_everywhere_and_is_written_back_as_format_8() {
    let mut zones = house();
    apply(
        &mut zones,
        r#"{"v":2,"t":"voice_enabled","zone":"kitchen","enabled":true}"#,
    );
    let eight = render(&house());
    // What the build before this one wrote: format 7, no `voice_enabled`.
    let seven = eight
        .replace("format = 8\n", "format = 7\n")
        .replace("voice_enabled = 0\n", "");
    assert!(seven.contains("format = 7\n") && !seven.contains("voice_enabled ="));
    let back = load(&seven, "127.0.0.1:4010").expect("format 7 still loads");
    assert!(back.zones().iter().all(|z| !z.voice_enabled));
    assert_eq!(render(&back), eight, "the next write is format 8");
    // A format 8 file says it for every room: a missing field is refused,
    // not defaulted.
    let missing = render(&zones).replacen("voice_enabled = 1\n", "", 1);
    let error = load(&missing, "x").unwrap_err();
    assert!(error.to_string().contains("voice_enabled"), "{}", error);
    // And a value that is not 0 or 1 is refused, not read as off.
    let wrong = render(&zones).replacen("voice_enabled = 1\n", "voice_enabled = yes\n", 1);
    assert!(load(&wrong, "x").is_err());
}
