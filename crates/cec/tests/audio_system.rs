//! The Audio System role held to the Android CTS checklist for an audio
//! device (`cts/hostsidetests/hdmicec/src/android/hdmicec/cts/audio/`,
//! Apache-2.0, read 2026-10-01), one test per HDMI CTS case where chorus
//! does what the case asks, and a test naming the difference where it
//! deliberately does not (`crates/cec/src/role.rs`, "Deliberate
//! differences"). The pure role is driven with a millisecond clock the test
//! owns; the last tests run the role through [`Driver`] on the fake bus
//! against the four scripted TVs.
//!
//! Nothing here is timing evidence: the clock is the test's, and the fake
//! bus has no bit timing.

use std::sync::Arc;
use std::time::{Duration, Instant};

use chorus_cec::codec::{
    build, opcode, ui, AbortReason, AudioStatus, Message, PhysicalAddress, BROADCAST, TUNER_1, TV,
    UNREGISTERED,
};
use chorus_cec::role::{
    AudioSystem, Config, Effect, MuteRequest, VolumeKey, KEY_RELEASE_MS, REPORT_AFTER_KEY_MS,
    RESPONSE_MS,
};
use chorus_cec::{AdapterError, Driver, FakeBus, FakeTv, TvKind, TvPower, TvPowerState};

const ME: u8 = 5;
const HUB_PA: PhysicalAddress = PhysicalAddress(0x1000);

fn role() -> AudioSystem {
    let mut r = AudioSystem::new(Config::default(), HUB_PA);
    r.start(0);
    r
}

fn to_me(op: u8, operands: &[u8]) -> Message {
    Message::new(TV, ME, op, operands)
}

fn sent(effects: &[Effect]) -> Vec<Message> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::Send(m) => Some(m.clone()),
            _ => None,
        })
        .collect()
}

fn not_sent(effects: &[Effect]) -> Vec<Effect> {
    effects
        .iter()
        .filter(|e| !matches!(e, Effect::Send(_)))
        .cloned()
        .collect()
}

fn samr_on() -> Message {
    build::system_audio_mode_request(TV, ME, Some(PhysicalAddress::TV))
}

fn abort(about: u8, reason: AbortReason) -> Message {
    build::feature_abort(ME, TV, about, reason)
}

// --- HDMI CTS 11.2.15: System Audio Control -----------------------------

#[test]
fn cts_11_2_15_1_system_audio_mode_request_from_the_tv_and_a_tuner_turns_it_on() {
    let mut r = role();
    let out = r.handle(&samr_on(), 1);
    assert_eq!(
        sent(&out),
        vec![build::set_system_audio_mode(ME, BROADCAST, true)]
    );
    assert!(not_sent(&out).contains(&Effect::SystemAudioMode(true)));
    let from_tuner = build::system_audio_mode_request(TUNER_1, ME, Some(PhysicalAddress::TV));
    assert_eq!(
        sent(&r.handle(&from_tuner, 2)),
        vec![build::set_system_audio_mode(ME, BROADCAST, true)],
        "a tuner may ask too, and the broadcast goes out each time"
    );
    assert!(r.system_audio_mode());
}

fn initiating() -> AudioSystem {
    let mut r = AudioSystem::new(
        Config {
            initiate_system_audio: true,
            ..Config::default()
        },
        HUB_PA,
    );
    r.start(0);
    // The TV is seen on.
    r.handle(
        &Message::new(TV, BROADCAST, opcode::ACTIVE_SOURCE, &[0, 0]),
        10,
    );
    r
}

#[test]
fn cts_11_2_15_2_the_hub_initiates_and_broadcasts_after_the_response_time() {
    let mut r = initiating();
    let asked = sent(&r.tick(20));
    assert!(
        asked.contains(&build::set_system_audio_mode(ME, TV, true)),
        "{asked:?}"
    );
    // While it waits, the TV asking the status hears "on" (Android's rule).
    assert_eq!(
        sent(&r.handle(&to_me(opcode::GIVE_SYSTEM_AUDIO_MODE_STATUS, &[]), 30)),
        vec![build::system_audio_mode_status(ME, TV, true)]
    );
    assert!(sent(&r.tick(20 + RESPONSE_MS - 1)).is_empty());
    let out = r.tick(20 + RESPONSE_MS);
    assert_eq!(
        sent(&out),
        vec![build::set_system_audio_mode(ME, BROADCAST, true)]
    );
    assert!(r.system_audio_mode());
    // Not asked again while on.
    assert!(sent(&r.tick(20 + 2 * RESPONSE_MS)).is_empty());
}

#[test]
fn cts_11_2_15_3_and_18_a_feature_abort_within_the_response_time_means_no_broadcast() {
    let mut r = initiating();
    r.tick(20);
    let refused = Message::new(
        TV,
        ME,
        opcode::FEATURE_ABORT,
        &[opcode::SET_SYSTEM_AUDIO_MODE, AbortReason::Refused.byte()],
    );
    assert!(
        sent(&r.handle(&refused, 500)).is_empty(),
        "a Feature Abort is never answered"
    );
    for t in [20 + RESPONSE_MS, 5_000, 60_000] {
        assert!(
            !sent(&r.tick(t))
                .iter()
                .any(|m| m.opcode == Some(opcode::SET_SYSTEM_AUDIO_MODE)),
            "no Set System Audio Mode at {t} ms"
        );
    }
    assert!(!r.system_audio_mode());
}

#[test]
fn cts_11_2_15_4_status_on_after_the_request() {
    let mut r = role();
    r.handle(&samr_on(), 1);
    assert_eq!(
        sent(&r.handle(&to_me(opcode::GIVE_SYSTEM_AUDIO_MODE_STATUS, &[]), 2)),
        vec![build::system_audio_mode_status(ME, TV, true)]
    );
}

#[test]
fn cts_11_2_15_5_a_request_with_no_operand_turns_it_off() {
    let mut r = role();
    r.handle(&samr_on(), 1);
    let out = r.handle(&to_me(opcode::SYSTEM_AUDIO_MODE_REQUEST, &[]), 2);
    assert_eq!(
        sent(&out),
        vec![build::set_system_audio_mode(ME, BROADCAST, false)]
    );
    assert!(not_sent(&out).contains(&Effect::SystemAudioMode(false)));
}

#[test]
fn cts_11_2_15_6_standby_with_system_audio_on_broadcasts_off_first() {
    let mut r = role();
    r.handle(&samr_on(), 1);
    let out = r.handle(&Message::new(TV, BROADCAST, opcode::STANDBY, &[]), 2);
    assert_eq!(
        sent(&out),
        vec![build::set_system_audio_mode(ME, BROADCAST, false)]
    );
    assert!(out.contains(&Effect::TvPower(TvPowerState::Standby)));
    // A standby sent to the hub by another device does the same.
    let mut r = role();
    r.handle(&samr_on(), 1);
    let out = r.handle(&Message::new(4, ME, opcode::STANDBY, &[]), 2);
    assert_eq!(
        sent(&out),
        vec![build::set_system_audio_mode(ME, BROADCAST, false)]
    );
    assert!(
        !out.iter().any(|e| matches!(e, Effect::TvPower(_))),
        "a playback device's standby to the hub says nothing of the TV"
    );
}

#[test]
fn cts_11_2_15_7_status_off_and_a_set_from_the_tv_is_ignored() {
    let mut r = role();
    let set = Message::new(TV, ME, opcode::SET_SYSTEM_AUDIO_MODE, &[0]);
    assert!(
        r.handle(&set, 1).is_empty(),
        "only an Audio System sends it"
    );
    assert_eq!(
        sent(&r.handle(&to_me(opcode::GIVE_SYSTEM_AUDIO_MODE_STATUS, &[]), 2)),
        vec![build::system_audio_mode_status(ME, TV, false)]
    );
}

#[test]
fn cts_11_2_15_8_mute_is_a_toggle_once_per_press() {
    let mut r = role();
    r.handle(&samr_on(), 1);
    let press = build::user_control_pressed(TV, ME, ui::MUTE);
    assert_eq!(
        r.handle(&press, 10),
        vec![Effect::Mute(MuteRequest::Toggle)]
    );
    // Held: the TV repeats the press; one toggle only.
    assert!(r.handle(&press, 10 + 200).is_empty());
    assert!(r.handle(&press, 10 + 400).is_empty());
    r.handle(&build::user_control_released(TV, ME), 500);
    assert_eq!(
        r.handle(&press, 600),
        vec![Effect::Mute(MuteRequest::Toggle)]
    );
    // Without a release, a press after the safety timeout is a new press.
    assert_eq!(
        r.handle(&press, 600 + KEY_RELEASE_MS + 1),
        vec![Effect::Mute(MuteRequest::Toggle)]
    );
    // Mute Function and Restore Volume Function are set and clear.
    assert_eq!(
        r.handle(
            &build::user_control_pressed(TV, ME, ui::MUTE_FUNCTION),
            5_000
        ),
        vec![Effect::Mute(MuteRequest::On)]
    );
    assert_eq!(
        r.handle(
            &build::user_control_pressed(TV, ME, ui::RESTORE_VOLUME_FUNCTION),
            6_000
        ),
        vec![Effect::Mute(MuteRequest::Off)]
    );
}

#[test]
fn cts_11_2_15_9_report_audio_status_carries_the_rooms_volume_and_mute() {
    let mut r = role();
    r.handle(&samr_on(), 1);
    let ask = to_me(opcode::GIVE_AUDIO_STATUS, &[]);
    let status = |r: &mut AudioSystem| {
        let out = sent(&r.handle(&ask, 2));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].opcode, Some(opcode::REPORT_AUDIO_STATUS));
        assert_eq!(out[0].destination, TV);
        out[0].operands[0]
    };
    r.room_state(0, false, 2);
    assert!([0, 128].contains(&status(&mut r)), "0 %");
    r.room_state(50, false, 2);
    assert!((46..=54).contains(&status(&mut r)), "50 %");
    r.room_state(100, false, 2);
    assert_eq!(status(&mut r), 100, "100 %");
    r.room_state(35, true, 2);
    assert!(status(&mut r) > 127, "muted: bit 7");
    assert_eq!(AudioStatus::from_byte(status(&mut r)).volume, Some(35));
}

#[test]
fn cts_11_2_15_13_and_14_only_lpcm_two_channel_is_ever_described() {
    let mut r = role();
    // LPCM among others: one descriptor, LPCM 2 ch 48 kHz 16 bit.
    for asked in [vec![1u8], vec![2, 1], vec![1, 2, 10, 7]] {
        let out = sent(&r.handle(&to_me(opcode::REQUEST_SHORT_AUDIO_DESCRIPTOR, &asked), 1));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].opcode, Some(opcode::REPORT_SHORT_AUDIO_DESCRIPTOR));
        assert_eq!(out[0].operands, vec![0x09, 0x04, 0x01], "asked {asked:?}");
    }
    // AC-3 (2), E-AC-3 (10, "DDP"), DTS (7), DTS-HD (11) alone: never.
    for asked in [vec![2u8], vec![10], vec![7], vec![11], vec![2, 10, 7, 11]] {
        assert_eq!(
            sent(&r.handle(&to_me(opcode::REQUEST_SHORT_AUDIO_DESCRIPTOR, &asked), 1)),
            vec![abort(
                opcode::REQUEST_SHORT_AUDIO_DESCRIPTOR,
                AbortReason::InvalidOperand
            )],
            "asked {asked:?}"
        );
    }
    // A format ID other than 0 is not a CEA-861 code: LPCM with ID 1 is not LPCM.
    assert_eq!(
        sent(&r.handle(&to_me(opcode::REQUEST_SHORT_AUDIO_DESCRIPTOR, &[0x41]), 1)),
        vec![abort(
            opcode::REQUEST_SHORT_AUDIO_DESCRIPTOR,
            AbortReason::InvalidOperand
        )]
    );
}

#[test]
fn cts_11_2_15_16_system_audio_on_unmutes_a_muted_room() {
    let mut r = role();
    r.room_state(30, true, 0);
    let out = r.handle(&samr_on(), 1);
    assert!(out.contains(&Effect::Mute(MuteRequest::Off)), "{out:?}");
    // An unmuted room is left alone.
    let mut r = role();
    r.room_state(30, false, 0);
    assert!(!r
        .handle(&samr_on(), 1)
        .iter()
        .any(|e| matches!(e, Effect::Mute(_))));
}

#[test]
fn cts_11_2_15_17_differs_system_audio_off_does_not_mute_the_shared_room() {
    let mut r = role();
    r.room_state(30, false, 0);
    r.handle(&samr_on(), 1);
    let out = r.handle(&to_me(opcode::SYSTEM_AUDIO_MODE_REQUEST, &[]), 2);
    assert!(
        !out.iter().any(|e| matches!(e, Effect::Mute(_))),
        "the room plays other sources too: {out:?}"
    );
}

// --- HDMI CTS 11.2.13: remote control pass through -----------------------

#[test]
fn cts_11_2_13_volume_keys_step_once_per_press_and_per_repeat() {
    let mut r = role();
    let up = build::user_control_pressed(TV, ME, ui::VOLUME_UP);
    let down = build::user_control_pressed(TV, ME, ui::VOLUME_DOWN);
    let release = build::user_control_released(TV, ME);
    // -1: press and release.
    assert_eq!(r.handle(&up, 0), vec![Effect::Volume(VolumeKey::Up)]);
    assert!(r.handle(&release, 100).is_empty());
    // -2: press and hold, the TV repeating the press, then release.
    for t in [1_000, 1_200, 1_400] {
        assert_eq!(r.handle(&down, t), vec![Effect::Volume(VolumeKey::Down)]);
    }
    assert!(r.handle(&release, 1_500).is_empty());
    // -3: hold with no release: every repeat still steps.
    for t in [2_000, 2_400, 2_800] {
        assert_eq!(r.handle(&up, t), vec![Effect::Volume(VolumeKey::Up)]);
    }
    // -4: interrupted by another key with no release.
    assert_eq!(
        r.handle(&down, 2_900),
        vec![Effect::Volume(VolumeKey::Down)]
    );
    // Power keys are accepted and change nothing; other keys are refused.
    for k in [
        ui::POWER,
        ui::POWER_TOGGLE_FUNCTION,
        ui::POWER_OFF_FUNCTION,
        ui::POWER_ON_FUNCTION,
    ] {
        assert!(r
            .handle(&build::user_control_pressed(TV, ME, k), 3_000)
            .is_empty());
    }
    let select = build::user_control_pressed(TV, ME, 0x00);
    assert_eq!(
        sent(&r.handle(&select, 3_100)),
        vec![abort(
            opcode::USER_CONTROL_PRESSED,
            AbortReason::InvalidOperand
        )]
    );
}

#[test]
fn a_volume_change_the_tv_caused_is_reported_unprompted_with_system_audio_on() {
    let mut r = role();
    r.room_state(40, false, 0);
    // System Audio off: nothing pushed.
    r.handle(&build::user_control_pressed(TV, ME, ui::VOLUME_UP), 10);
    assert!(r.room_state(42, false, 20).is_empty());
    r.handle(&samr_on(), 30);
    r.handle(&build::user_control_pressed(TV, ME, ui::VOLUME_UP), 100);
    let out = r.room_state(44, false, 150);
    assert_eq!(
        sent(&out),
        vec![build::report_audio_status(
            ME,
            TV,
            AudioStatus {
                muted: false,
                volume: Some(44)
            }
        )]
    );
    // The same state again: nothing.
    assert!(r.room_state(44, false, 160).is_empty());
    // A change made elsewhere long after the last key: not pushed.
    assert!(r
        .room_state(20, false, 100 + REPORT_AFTER_KEY_MS + 1)
        .is_empty());
}

// --- System information and the rest --------------------------------------

#[test]
fn power_status_osd_name_physical_address_version_and_vendor_id() {
    let mut r = role();
    assert_eq!(
        sent(&r.handle(&to_me(opcode::GIVE_DEVICE_POWER_STATUS, &[]), 1)),
        vec![Message::new(ME, TV, opcode::REPORT_POWER_STATUS, &[0])]
    );
    assert_eq!(
        sent(&r.handle(&to_me(opcode::GIVE_OSD_NAME, &[]), 1)),
        vec![Message::new(ME, TV, opcode::SET_OSD_NAME, b"chorus")]
    );
    assert_eq!(
        sent(&r.handle(&to_me(opcode::GIVE_PHYSICAL_ADDRESS, &[]), 1)),
        vec![Message::new(
            ME,
            BROADCAST,
            opcode::REPORT_PHYSICAL_ADDRESS,
            &[0x10, 0, 5]
        )]
    );
    assert_eq!(
        sent(&r.handle(&to_me(opcode::GET_CEC_VERSION, &[]), 1)),
        vec![Message::new(ME, TV, opcode::CEC_VERSION, &[5])]
    );
    assert_eq!(
        sent(&r.handle(&to_me(opcode::GIVE_DEVICE_VENDOR_ID, &[]), 1)),
        vec![abort(
            opcode::GIVE_DEVICE_VENDOR_ID,
            AbortReason::UnrecognizedOpcode
        )]
    );
    // With an OUI configured it is broadcast, most significant byte first.
    let mut v = AudioSystem::new(
        Config {
            vendor_id: Some(0x00_12_34),
            ..Config::default()
        },
        HUB_PA,
    );
    assert_eq!(
        sent(&v.handle(&to_me(opcode::GIVE_DEVICE_VENDOR_ID, &[]), 1)),
        vec![Message::new(
            ME,
            BROADCAST,
            opcode::DEVICE_VENDOR_ID,
            &[0x00, 0x12, 0x34]
        )]
    );
    // Abort is refused, as the kernel's core would.
    assert_eq!(
        sent(&r.handle(&to_me(opcode::ABORT, &[]), 1)),
        vec![abort(opcode::ABORT, AbortReason::Refused)]
    );
    // A new physical address (a hot plug) is announced.
    assert_eq!(
        sent(&r.set_physical_address(PhysicalAddress(0x2000))),
        vec![Message::new(
            ME,
            BROADCAST,
            opcode::REPORT_PHYSICAL_ADDRESS,
            &[0x20, 0, 5]
        )]
    );
    assert!(r.set_physical_address(PhysicalAddress(0x2000)).is_empty());
}

#[test]
fn unknown_directed_messages_are_feature_aborted_and_nothing_else_is() {
    let mut r = role();
    // An opcode chorus has no name for, directed: Unrecognized opcode.
    let odd = to_me(0x9a, &[1]);
    assert_eq!(
        sent(&r.handle(&odd, 1)),
        vec![abort(0x9a, AbortReason::UnrecognizedOpcode)]
    );
    // The same broadcast: silence.
    assert!(r
        .handle(&Message::new(TV, BROADCAST, 0x9a, &[1]), 1)
        .is_empty());
    // From Unregistered: silence (CEC 12.2 via Android).
    assert!(r
        .handle(&Message::new(UNREGISTERED, ME, 0x9a, &[1]), 1)
        .is_empty());
    // A Feature Abort is never answered with one.
    let fa = to_me(opcode::FEATURE_ABORT, &[0x9a, 0]);
    assert!(r.handle(&fa, 1).is_empty());
    // Replies and announcements are noted, never aborted.
    for m in [
        to_me(opcode::REPORT_AUDIO_STATUS, &[10]),
        to_me(opcode::CEC_VERSION, &[5]),
        to_me(opcode::SET_OSD_NAME, b"TV"),
        Message::new(TV, BROADCAST, opcode::REPORT_PHYSICAL_ADDRESS, &[0, 0, 0]),
        Message::new(TV, BROADCAST, opcode::DEVICE_VENDOR_ID, &[0, 0, 1]),
        Message::new(4, BROADCAST, opcode::REQUEST_ACTIVE_SOURCE, &[]),
    ] {
        assert!(sent(&r.handle(&m, 1)).is_empty(), "{m}");
    }
    // A message for someone else is not ours, and a poll is nothing.
    assert!(r.handle(&Message::new(TV, 4, 0x9a, &[]), 1).is_empty());
    assert!(r.handle(&Message::poll(TV, ME), 1).is_empty());
    // An operand out of range, directed: Invalid operand.
    assert_eq!(
        sent(&r.handle(&to_me(opcode::SYSTEM_AUDIO_MODE_REQUEST, &[0x01, 0x10]), 1)),
        vec![abort(
            opcode::SYSTEM_AUDIO_MODE_REQUEST,
            AbortReason::InvalidOperand
        )]
    );
    // Too short: dropped.
    assert!(r
        .handle(&to_me(opcode::USER_CONTROL_PRESSED, &[]), 1)
        .is_empty());
    // Our own echo: ignored.
    assert!(r
        .handle(&Message::new(ME, BROADCAST, opcode::STANDBY, &[]), 1)
        .is_empty());
}

#[test]
fn cts_12_2_directly_addressed_messages_received_as_broadcast_are_ignored() {
    let mut r = AudioSystem::new(
        Config {
            arc: true,
            ..Config::default()
        },
        HUB_PA,
    );
    for (op, operands) in [
        (opcode::GIVE_AUDIO_STATUS, vec![]),
        (opcode::GIVE_SYSTEM_AUDIO_MODE_STATUS, vec![]),
        (opcode::REQUEST_SHORT_AUDIO_DESCRIPTOR, vec![1]),
        (opcode::SYSTEM_AUDIO_MODE_REQUEST, vec![0, 0]),
        (opcode::REQUEST_ARC_INITIATION, vec![]),
        (opcode::REQUEST_ARC_TERMINATION, vec![]),
    ] {
        let out = r.handle(&Message::new(TV, BROADCAST, op, &operands), 1);
        assert!(sent(&out).is_empty(), "op 0x{op:02x}: {out:?}");
    }
    assert!(!r.system_audio_mode());
    // And a broadcast-only message received directly is ignored, TV power
    // included.
    let out = r.handle(&to_me(opcode::ACTIVE_SOURCE, &[0, 0]), 2);
    assert!(out.is_empty(), "{out:?}");
    assert_eq!(r.tv_power(), TvPowerState::Unknown);
}

// --- HDMI CTS 11.2.17: ARC -------------------------------------------------

#[test]
fn cts_11_2_17_arc_only_when_configured() {
    // Off (the default, ASSUMED: P2's ARC path is an extractor's optical out).
    let mut r = role();
    for op in [
        opcode::REQUEST_ARC_INITIATION,
        opcode::REQUEST_ARC_TERMINATION,
        opcode::REPORT_ARC_INITIATED,
    ] {
        assert_eq!(
            sent(&r.handle(&to_me(op, &[]), 1)),
            vec![abort(op, AbortReason::UnrecognizedOpcode)]
        );
    }
    // On: -1 initiate at start, -3 on request, -4 terminate on request.
    let mut r = AudioSystem::new(
        Config {
            arc: true,
            ..Config::default()
        },
        HUB_PA,
    );
    assert!(sent(&r.start(0)).contains(&build::initiate_arc(ME, TV)));
    assert_eq!(
        sent(&r.handle(&to_me(opcode::REQUEST_ARC_TERMINATION, &[]), 1)),
        vec![abort(
            opcode::REQUEST_ARC_TERMINATION,
            AbortReason::NotInCorrectMode
        )],
        "not up yet"
    );
    assert_eq!(
        sent(&r.handle(&to_me(opcode::REQUEST_ARC_INITIATION, &[]), 2)),
        vec![build::initiate_arc(ME, TV)]
    );
    assert_eq!(
        r.handle(&to_me(opcode::REPORT_ARC_INITIATED, &[]), 3),
        vec![Effect::Arc(true)]
    );
    assert!(r.arc());
    assert_eq!(
        sent(&r.handle(&to_me(opcode::REQUEST_ARC_TERMINATION, &[]), 4)),
        vec![build::terminate_arc(ME, TV)]
    );
    assert_eq!(
        r.handle(&to_me(opcode::REPORT_ARC_TERMINATED, &[]), 5),
        vec![Effect::Arc(false)]
    );
}

// --- The TV's power ------------------------------------------------------

#[test]
fn tv_on_and_standby_detection() {
    let on = Effect::TvPower(TvPowerState::On);
    let standby = Effect::TvPower(TvPowerState::Standby);
    let cases: Vec<(Message, Effect)> = vec![
        (
            Message::new(TV, ME, opcode::REPORT_POWER_STATUS, &[0]),
            on.clone(),
        ),
        (
            Message::new(TV, ME, opcode::REPORT_POWER_STATUS, &[1]),
            standby.clone(),
        ),
        (
            Message::new(TV, ME, opcode::REPORT_POWER_STATUS, &[2]),
            on.clone(),
        ),
        (
            Message::new(TV, ME, opcode::REPORT_POWER_STATUS, &[3]),
            standby.clone(),
        ),
        (
            Message::new(TV, BROADCAST, opcode::REPORT_POWER_STATUS, &[0]),
            on.clone(),
        ),
        (
            Message::new(4, BROADCAST, opcode::ACTIVE_SOURCE, &[0x20, 0]),
            on.clone(),
        ),
        (
            Message::new(TV, BROADCAST, opcode::ROUTING_CHANGE, &[0x10, 0, 0x20, 0]),
            on.clone(),
        ),
        (
            Message::new(TV, BROADCAST, opcode::SET_STREAM_PATH, &[0x20, 0]),
            on.clone(),
        ),
        (Message::new(4, TV, opcode::IMAGE_VIEW_ON, &[]), on.clone()),
        (Message::new(4, TV, opcode::TEXT_VIEW_ON, &[]), on.clone()),
        (samr_on(), on.clone()),
        (
            Message::new(TV, BROADCAST, opcode::STANDBY, &[]),
            standby.clone(),
        ),
        (Message::new(TV, ME, opcode::STANDBY, &[]), standby.clone()),
        (
            Message::new(4, BROADCAST, opcode::STANDBY, &[]),
            standby.clone(),
        ),
    ];
    for (m, want) in cases {
        let mut r = role();
        // From the opposite state, so the change shows.
        let opposite = if want == on {
            Message::new(TV, BROADCAST, opcode::STANDBY, &[])
        } else {
            Message::new(TV, BROADCAST, opcode::ACTIVE_SOURCE, &[0, 0])
        };
        r.handle(&opposite, 0);
        let out = r.handle(&m, 1);
        assert!(out.contains(&want), "{m}: {out:?}");
        // The same again changes nothing.
        assert!(
            !r.handle(&m, 2)
                .iter()
                .any(|e| matches!(e, Effect::TvPower(_))),
            "{m}"
        );
    }
    // A standby a playback device sends to the hub is not the TV's.
    let mut r = role();
    r.handle(
        &Message::new(TV, BROADCAST, opcode::ACTIVE_SOURCE, &[0, 0]),
        0,
    );
    r.handle(&Message::new(4, ME, opcode::STANDBY, &[]), 1);
    assert_eq!(r.tv_power(), TvPowerState::On);
}

#[test]
fn the_tv_is_polled_for_its_power_at_the_interval() {
    let mut r = AudioSystem::new(
        Config {
            power_poll_ms: 1_000,
            ..Config::default()
        },
        HUB_PA,
    );
    assert!(sent(&r.start(0)).contains(&build::give_device_power_status(ME, TV)));
    assert!(sent(&r.tick(999)).is_empty());
    assert_eq!(
        sent(&r.tick(1_000)),
        vec![build::give_device_power_status(ME, TV)]
    );
    assert!(sent(&r.tick(1_500)).is_empty());
    assert_eq!(
        sent(&r.tick(2_000)),
        vec![build::give_device_power_status(ME, TV)]
    );
}

#[test]
fn a_configuration_the_bus_cannot_carry_is_refused_by_name() {
    let bad = |c: Config| c.check().unwrap_err();
    assert!(bad(Config {
        osd_name: String::new(),
        ..Config::default()
    })
    .contains("1 to 14"));
    assert!(bad(Config {
        osd_name: "a name that is far too long".into(),
        ..Config::default()
    })
    .contains("1 to 14"));
    assert!(bad(Config {
        osd_name: "caf\u{e9}".into(),
        ..Config::default()
    })
    .contains("ASCII"));
    assert!(bad(Config {
        vendor_id: Some(0x0100_0000),
        ..Config::default()
    })
    .contains("24-bit"));
    assert!(Config::default().check().is_ok());
}

// --- On the fake bus, against the scripted TVs -----------------------------

fn clock() -> Box<dyn Fn() -> u64 + Send> {
    let t0 = Instant::now();
    Box::new(move || t0.elapsed().as_millis() as u64)
}

fn driver(bus: &FakeBus, config: Config) -> (Driver<chorus_cec::FakeAdapter>, Arc<TvPower>) {
    let power = Arc::new(TvPower::new());
    let d = Driver::start(
        bus.adapter(HUB_PA),
        config,
        Arc::clone(&power),
        clock(),
        Box::new(|_| {}),
    )
    .expect("the driver claims 5");
    (d, power)
}

/// Step the driver until `done` says so, collecting the caller's effects.
fn pump(
    d: &mut Driver<chorus_cec::FakeAdapter>,
    within: Duration,
    mut done: impl FnMut(&[Effect]) -> bool,
) -> Vec<Effect> {
    let deadline = Instant::now() + within;
    let mut all = Vec::new();
    while Instant::now() < deadline {
        all.extend(d.step(Duration::from_millis(10)).expect("a step"));
        if done(&all) {
            return all;
        }
    }
    panic!("timed out; effects so far: {all:?}");
}

const WAIT: Duration = Duration::from_secs(5);

#[test]
fn roku_like_tv_on_volume_status_and_standby() {
    let bus = FakeBus::new();
    let tv = FakeTv::start(&bus, TvKind::RokuLike);
    let (mut d, power) = driver(&bus, Config::default());
    // The hub announced itself and polled; a TV in standby says so.
    assert!(tv
        .expect(0, WAIT, |m| m.opcode
            == Some(opcode::REPORT_PHYSICAL_ADDRESS))
        .is_some());
    pump(&mut d, WAIT, |e| {
        e.contains(&Effect::TvPower(TvPowerState::Standby))
    });
    assert_eq!(power.get(), TvPowerState::Standby);
    tv.power_on(TvKind::RokuLike);
    pump(&mut d, WAIT, |e| {
        e.contains(&Effect::TvPower(TvPowerState::On))
    });
    assert_eq!(power.get(), TvPowerState::On);
    let n = tv.heard().len();
    tv.send(samr_on());
    pump(&mut d, WAIT, |e| e.contains(&Effect::SystemAudioMode(true)));
    assert!(tv
        .expect(n, WAIT, |m| *m
            == build::set_system_audio_mode(ME, BROADCAST, true))
        .is_some());
    tv.press(ui::VOLUME_UP);
    pump(&mut d, WAIT, |e| e.contains(&Effect::Volume(VolumeKey::Up)));
    // The server answers with the room's state: pushed to the TV unprompted.
    let n = tv.heard().len();
    d.room_state(52, false).unwrap();
    let pushed = tv
        .expect(n, WAIT, |m| m.opcode == Some(opcode::REPORT_AUDIO_STATUS))
        .expect("Report Audio Status pushed");
    assert_eq!(pushed.operands, vec![52]);
    let n = tv.heard().len();
    tv.send(to_me(opcode::GIVE_AUDIO_STATUS, &[]));
    pump(&mut d, WAIT, |_| tv.heard().len() > n);
    assert_eq!(
        tv.heard()[n].operands,
        vec![52],
        "Give Audio Status answered"
    );
    tv.standby();
    pump(&mut d, WAIT, |e| {
        e.contains(&Effect::TvPower(TvPowerState::Standby))
    });
    assert!(!d.role().system_audio_mode());
    assert_eq!(power.get(), TvPowerState::Standby);
    assert_eq!(
        d.counters()
            .unacknowledged
            .load(std::sync::atomic::Ordering::Relaxed),
        0
    );
}

#[test]
fn a_feature_abort_everything_tv_never_gets_system_audio_broadcast() {
    let bus = FakeBus::new();
    let tv = FakeTv::start(&bus, TvKind::FeatureAbortEverything);
    let (mut d, power) = driver(
        &bus,
        Config {
            initiate_system_audio: true,
            ..Config::default()
        },
    );
    // Its power poll is refused: the power stays unknown until it shows.
    pump(&mut d, WAIT, |_| {
        tv.heard()
            .iter()
            .any(|m| m.opcode == Some(opcode::GIVE_DEVICE_POWER_STATUS))
    });
    let until = Instant::now() + Duration::from_millis(300);
    while Instant::now() < until {
        d.step(Duration::from_millis(10)).unwrap();
    }
    assert_eq!(power.get(), TvPowerState::Unknown);
    // It is seen on (a playback device's Active Source), the hub asks for
    // System Audio, the TV refuses, and nothing is broadcast.
    tv.send(Message::new(
        TV,
        BROADCAST,
        opcode::ROUTING_CHANGE,
        &[0, 0, 0x10, 0],
    ));
    pump(&mut d, WAIT, |e| {
        e.contains(&Effect::TvPower(TvPowerState::On))
    });
    let until = Instant::now() + Duration::from_millis(2 * RESPONSE_MS);
    while Instant::now() < until {
        d.step(Duration::from_millis(10)).unwrap();
    }
    assert!(tv
        .heard()
        .iter()
        .any(|m| *m == build::set_system_audio_mode(ME, TV, true)));
    assert!(!bus
        .sent()
        .iter()
        .any(|(m, _)| *m == build::set_system_audio_mode(ME, BROADCAST, true)));
    assert!(!d.role().system_audio_mode());
}

#[test]
fn a_tv_without_arc_refuses_it_and_the_hub_carries_on() {
    let bus = FakeBus::new();
    let tv = FakeTv::start(&bus, TvKind::NoArc);
    let (mut d, _) = driver(
        &bus,
        Config {
            arc: true,
            ..Config::default()
        },
    );
    assert!(tv
        .expect(0, WAIT, |m| m.opcode == Some(opcode::INITIATE_ARC))
        .is_some());
    let until = Instant::now() + Duration::from_millis(300);
    while Instant::now() < until {
        d.step(Duration::from_millis(10)).unwrap();
    }
    assert!(!d.role().arc());
    // The volume keys still work.
    tv.send(samr_on());
    tv.press(ui::VOLUME_DOWN);
    pump(&mut d, WAIT, |e| {
        e.contains(&Effect::Volume(VolumeKey::Down))
    });
    // And a Roku-like TV takes ARC.
    let bus = FakeBus::new();
    let _tv = FakeTv::start(&bus, TvKind::RokuLike);
    let (mut d, _) = driver(
        &bus,
        Config {
            arc: true,
            ..Config::default()
        },
    );
    pump(&mut d, WAIT, |e| e.contains(&Effect::Arc(true)));
}

#[test]
fn a_slow_tv_is_still_heard_and_its_silence_is_acceptance() {
    let bus = FakeBus::new();
    let slow = TvKind::Slow(Duration::from_millis(1_200));
    let tv = FakeTv::start(&bus, slow);
    tv.power_on(slow);
    let (mut d, power) = driver(
        &bus,
        Config {
            initiate_system_audio: true,
            ..Config::default()
        },
    );
    // The answer to the start's power poll arrives past the response time
    // and still counts.
    pump(&mut d, WAIT, |e| {
        e.contains(&Effect::TvPower(TvPowerState::On))
    });
    assert_eq!(power.get(), TvPowerState::On);
    // No Feature Abort within 1 s: System Audio Mode is broadcast on.
    pump(&mut d, WAIT, |e| e.contains(&Effect::SystemAudioMode(true)));
    assert!(tv
        .expect(0, WAIT, |m| *m
            == build::set_system_audio_mode(ME, BROADCAST, true))
        .is_some());
}

#[test]
fn a_second_audio_system_cannot_claim_5_and_is_refused_by_name() {
    let bus = FakeBus::new();
    let (_first, _) = driver(&bus, Config::default());
    let e = Driver::start(
        bus.adapter(PhysicalAddress(0x2000)),
        Config::default(),
        Arc::new(TvPower::new()),
        clock(),
        Box::new(|_| {}),
    )
    .unwrap_err();
    assert!(e.to_string().contains("logical address 5"), "{e}");
}

#[test]
fn a_flood_is_reported_lost_and_a_hot_plug_reannounces() {
    let bus = FakeBus::new();
    let tv = FakeTv::start(&bus, TvKind::RokuLike);
    let mut adapter = bus.adapter(HUB_PA);
    let power = Arc::new(TvPower::new());
    // Replug before the start so the event is queued for the first step.
    adapter.replug(PhysicalAddress(0x3000));
    let mut d = Driver::start(adapter, Config::default(), power, clock(), Box::new(|_| {}))
        .expect("claimed");
    for _ in 0..(chorus_cec::fake::QUEUE_DEPTH + 6) {
        tv.send(Message::new(
            TV,
            BROADCAST,
            opcode::REQUEST_ACTIVE_SOURCE,
            &[],
        ));
    }
    d.step(Duration::from_millis(10)).unwrap();
    assert!(d.counters().lost.load(std::sync::atomic::Ordering::Relaxed) >= 6);
    assert_eq!(d.physical_address(), PhysicalAddress(0x3000));
    assert!(tv
        .expect(0, WAIT, |m| *m
            == Message::new(
                ME,
                BROADCAST,
                opcode::REPORT_PHYSICAL_ADDRESS,
                &[0x30, 0, 5]
            ))
        .is_some());
    bus.close();
    assert_eq!(
        d.step(Duration::from_millis(10)).unwrap_err(),
        AdapterError::Closed
    );
}
