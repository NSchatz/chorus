//! Goal 14: firmware in the room model. What a `firmware_install` reaches,
//! what it refuses by name, that accepting it starts nothing but a
//! `requested`, and that nothing about firmware survives a restart.
//!
//! The wire bytes of every command and refusal are the vectors' (`fixtures/
//! control/v2/firmware_*`, `error-firmware-*`, `state-firmware`, read by
//! `catalog_v2.rs`); this file holds the rules the vectors cannot show one
//! at a time.

use chorus_control::catalog::{decode_command, Command};
use chorus_control::firmware::{self, Image, Report, SpeakerFirmware};
use chorus_control::persist::{load, render};
use chorus_control::zones::{Zone, Zones};

const A: &str = "chorus-0123456789ab";
const B: &str = "chorus-ba9876543210";
const C: &str = "chorus-c0ffee000001";

fn image(name: &str, version: &str, board: &str) -> Image {
    Image {
        name: name.to_string(),
        version: version.to_string(),
        board: board.to_string(),
        size: 4096,
        sha256: "00".repeat(32),
        refused: None,
    }
}

fn idle(version: &str, board: &str) -> Report {
    Report {
        state: firmware::IDLE.to_string(),
        reason: firmware::NO_REASON.to_string(),
        transfer: 0,
        received: 0,
        version: version.to_string(),
        board: board.to_string(),
        slot: Some(0),
        image_version: String::new(),
        carried: false,
    }
}

/// Three speakers up: A and B of the brick board at 1.0.0, C of the compact
/// board; an image of each board at 2.0.0 staged.
fn house() -> Zones {
    let mut zones = Zones::new("127.0.0.1:4010");
    zones.add(Zone::new("kitchen")).unwrap();
    for (id, board) in [(A, "brick"), (B, "brick"), (C, "compact")] {
        zones.speaker_adopted(id, "1f0e:2d3c").unwrap();
        zones.speaker_session_up(id, "chorus-endpoint 0.1.0", &["player".to_string()]);
        zones.speaker_now(id, |now| {
            SpeakerFirmware::absorb(&mut now.firmware, &idle("1.0.0", board))
        });
    }
    zones.set_firmware_images(Some(vec![
        image("brick-2", "2.0.0", "brick"),
        image("compact-2", "2.0.0", "compact"),
    ]));
    zones
}

fn state_of(zones: &Zones, id: &str) -> String {
    zones
        .speakers()
        .get(id)
        .and_then(|s| s.now.firmware.as_ref())
        .map(|f| f.state.clone())
        .unwrap_or_default()
}

fn apply(zones: &mut Zones, text: &str) -> Result<(), chorus_control::Refusal> {
    zones.apply(&decode_command(text).unwrap())
}

#[test]
fn install_all_reaches_the_boards_present_speakers_and_marks_them_requested_only() {
    let mut zones = house();
    let (_, targets) = zones.firmware_targets(None, "brick-2", false).unwrap();
    assert_eq!(
        targets,
        vec![A.to_string(), B.to_string()],
        "of the image's board"
    );
    zones.speaker_session_down(B);
    let (_, targets) = zones.firmware_targets(None, "brick-2", false).unwrap();
    assert_eq!(targets, vec![A.to_string()], "present ones only");

    let serial = zones.serial();
    apply(
        &mut zones,
        r#"{"v":2,"t":"firmware_install","all":true,"image":"brick-2"}"#,
    )
    .unwrap();
    assert_eq!(zones.serial(), serial + 1);
    assert_eq!(state_of(&zones, A), firmware::REQUESTED);
    assert_eq!(state_of(&zones, B), firmware::IDLE, "absent: not reached");
    assert_eq!(
        state_of(&zones, C),
        firmware::IDLE,
        "another board: not reached"
    );
    let install = zones
        .speakers()
        .get(A)
        .unwrap()
        .now
        .firmware
        .clone()
        .unwrap()
        .install
        .unwrap();
    assert_eq!(
        (install.image.as_str(), install.version.as_str()),
        ("brick-2", "2.0.0")
    );
    assert_eq!(
        install.transfer, 0,
        "the server assigns the transfer, not the model"
    );

    // A second install while one is in hand is busy; a cancel ends it.
    let refusal = apply(
        &mut zones,
        &format!(
            r#"{{"v":2,"t":"firmware_install","speaker":"{}","image":"brick-2"}}"#,
            A
        ),
    )
    .unwrap_err();
    assert!(refusal.detail.starts_with("busy: "), "{}", refusal.detail);
    apply(
        &mut zones,
        &format!(r#"{{"v":2,"t":"firmware_cancel","speaker":"{}"}}"#, A),
    )
    .unwrap();
    assert_eq!(state_of(&zones, A), firmware::CANCELLED);
    // A cancelled install is an outcome, not one in progress: install again.
    apply(
        &mut zones,
        &format!(
            r#"{{"v":2,"t":"firmware_install","speaker":"{}","image":"brick-2"}}"#,
            A
        ),
    )
    .unwrap();
    assert_eq!(state_of(&zones, A), firmware::REQUESTED);
}

#[test]
fn a_speaker_that_never_reported_takes_no_updates_and_force_reinstalls() {
    let mut zones = house();
    zones
        .speaker_adopted("chorus-000000000099", "aaaa")
        .unwrap();
    zones.speaker_session_up("chorus-000000000099", "linux", &["player".to_string()]);
    let refusal = zones
        .firmware_targets(Some("chorus-000000000099"), "brick-2", false)
        .unwrap_err();
    assert!(
        refusal.detail.starts_with("not-updatable: "),
        "{}",
        refusal.detail
    );

    zones.set_firmware_images(Some(vec![image("brick-1", "1.0.0", "brick")]));
    let refusal = zones
        .firmware_targets(Some(A), "brick-1", false)
        .unwrap_err();
    assert!(
        refusal.detail.starts_with("already-running: "),
        "{}",
        refusal.detail
    );
    let (_, targets) = zones.firmware_targets(Some(A), "brick-1", true).unwrap();
    assert_eq!(targets, vec![A.to_string()]);
    let (_, targets) = zones.firmware_targets(None, "brick-1", true).unwrap();
    assert_eq!(
        targets,
        vec![A.to_string(), B.to_string()],
        "all with force"
    );
}

#[test]
fn update_available_follows_the_staged_images_and_a_rescan_moves_the_serial_only_on_change() {
    let mut zones = house();
    let available = |zones: &Zones, id: &str| {
        let state = zones.encode_state();
        let speakers = chorus_control::json::parse(&state).unwrap();
        let list = match speakers.get("speakers") {
            Some(chorus_control::json::Value::Arr(items)) => items.clone(),
            _ => panic!("{}", state),
        };
        list.iter()
            .find(|s| s.get("id").and_then(|v| v.as_str()) == Some(id))
            .and_then(|s| s.get("firmware").cloned())
            .and_then(|f| f.get("update_available").and_then(|v| v.as_bool()))
            .unwrap()
    };
    assert!(available(&zones, A));
    let serial = zones.serial();
    assert!(!zones.set_firmware_images(Some(vec![
        image("brick-2", "2.0.0", "brick"),
        image("compact-2", "2.0.0", "compact"),
    ])));
    assert_eq!(zones.serial(), serial, "the same images again is no change");
    let mut refused = image("brick-2", "2.0.0", "brick");
    refused.refused = Some("digest-mismatch".to_string());
    assert!(zones.set_firmware_images(Some(vec![refused])));
    assert!(!available(&zones, A), "a refused image is no update");
    assert_eq!(zones.serial(), serial + 1);
}

#[test]
fn nothing_about_firmware_survives_a_restart() {
    let mut zones = house();
    apply(
        &mut zones,
        &format!(
            r#"{{"v":2,"t":"firmware_install","speaker":"{}","image":"brick-2"}}"#,
            A
        ),
    )
    .unwrap();
    let text = render(&zones);
    assert!(
        !text.contains("firmware") && !text.contains("brick-2"),
        "{}",
        text
    );
    let back = load(&text, "127.0.0.1:4010").unwrap();
    assert_eq!(
        back.firmware_images(),
        None,
        "the directory is read again at start"
    );
    for id in [A, B, C] {
        let speaker = back
            .speakers()
            .get(id)
            .expect("the record itself is persisted");
        assert_eq!(
            speaker.now.firmware, None,
            "an install in progress is not resumed"
        );
    }
    // And the record's persisted half is the bytes it was without firmware.
    let mut plain = Zones::new("127.0.0.1:4010");
    plain.add(Zone::new("kitchen")).unwrap();
    for id in [A, B, C] {
        plain.speaker_adopted(id, "1f0e:2d3c").unwrap();
    }
    assert_eq!(
        render(&back)
            .lines()
            .filter(|l| !l.starts_with("serial"))
            .collect::<Vec<_>>(),
        render(&plain)
            .lines()
            .filter(|l| !l.starts_with("serial"))
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_firmware_command_that_is_refused_applies_nothing() {
    let mut zones = house();
    let before = zones.encode_state();
    for text in [
        r#"{"v":2,"t":"firmware_install","all":true,"image":"none-such"}"#,
        r#"{"v":2,"t":"firmware_install","speaker":"chorus-ffffffffffff","image":"brick-2"}"#,
        r#"{"v":2,"t":"firmware_cancel","speaker":"chorus-0123456789ab"}"#,
    ] {
        assert!(apply(&mut zones, text).is_err(), "{}", text);
        assert_eq!(zones.encode_state(), before, "{}", text);
    }
    assert!(matches!(
        decode_command(r#"{"v":2,"t":"firmware_rescan"}"#).unwrap(),
        Command::FirmwareRescan
    ));
}
