//! The CEC golden vectors, `fixtures/cec/` (Rust-only by declaration: CEC
//! runs on the Linux hub alone, there is no C implementation;
//! `tools/conventions/check-shared-fixtures.sh`).
//!
//! Every `<name>.hex` decodes to exactly `<name>.fields`, the fields encode
//! back to exactly the bytes, and the validator says of the message what the
//! fields' `validity` says. Then the role is driven to produce each vector it
//! sends, so the bytes the hub puts on the bus are the committed ones.

use std::collections::BTreeMap;
use std::path::PathBuf;

use chorus_cec::codec::{build, opcode, ui, AbortReason, AudioStatus, Message, TV};
use chorus_cec::role::{AudioSystem, Config, Effect};
use chorus_cec::validate::{validate, Validity};
use chorus_cec::PhysicalAddress;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/cec")
}

fn hex(name: &str) -> Vec<u8> {
    let text = std::fs::read_to_string(dir().join(format!("{name}.hex"))).unwrap();
    text.lines()
        .map(|l| l.split('#').next().unwrap())
        .flat_map(|l| l.split_whitespace().map(str::to_string).collect::<Vec<_>>())
        .map(|b| u8::from_str_radix(&b, 16).unwrap())
        .collect()
}

fn fields(name: &str) -> BTreeMap<String, String> {
    let text = std::fs::read_to_string(dir().join(format!("{name}.fields"))).unwrap();
    text.lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

fn names() -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter_map(|n| n.strip_suffix(".hex").map(str::to_string))
        .collect();
    v.sort();
    v
}

fn message_of(f: &BTreeMap<String, String>) -> Message {
    let op = &f["opcode"];
    let operands: Vec<u8> = (0..f["operands"].len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&f["operands"][i..i + 2], 16).unwrap())
        .collect();
    Message {
        initiator: f["initiator"].parse().unwrap(),
        destination: f["destination"].parse().unwrap(),
        opcode: if op.is_empty() {
            None
        } else {
            Some(opcode::from_name(op).unwrap_or_else(|| panic!("unnamed opcode {op}")))
        },
        operands,
    }
}

#[test]
fn every_vector_round_trips_and_validates_as_its_fields_say() {
    let all = names();
    assert!(all.len() >= 30, "the vectors are there: {}", all.len());
    for name in &all {
        // Every .hex has its .fields and nothing else lives here.
        let f = fields(name);
        let bytes = hex(name);
        let m = message_of(&f);
        assert_eq!(Message::decode(&bytes).unwrap(), m, "{name}: decode");
        assert_eq!(m.encode().unwrap(), bytes, "{name}: encode");
        let want = Validity::from_name(&f["validity"]).unwrap();
        assert_eq!(validate(&m), want, "{name}: validity");
    }
    for entry in std::fs::read_dir(dir()).unwrap() {
        let n = entry.unwrap().file_name().into_string().unwrap();
        let stem = n.rsplit_once('.').map(|(s, _)| s).unwrap_or(&n);
        assert!(
            (n.ends_with(".hex") || n.ends_with(".fields")) && all.iter().any(|a| a == stem),
            "{n}: a file the test does not read"
        );
    }
}

fn vector(name: &str) -> Message {
    Message::decode(&hex(name)).unwrap()
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

#[test]
fn the_role_sends_the_committed_bytes() {
    let mut role = AudioSystem::new(Config::default(), PhysicalAddress(0x1000));
    let start = sent(&role.start(0));
    assert_eq!(start[0], vector("report_physical_address"));
    assert_eq!(start[1], vector("give_device_power_status_to_tv"));

    let r = sent(&role.handle(&vector("system_audio_mode_request_on"), 1));
    assert_eq!(r, vec![vector("set_system_audio_mode_on")]);
    let r = sent(&role.handle(
        &Message::new(TV, 5, opcode::GIVE_SYSTEM_AUDIO_MODE_STATUS, &[]),
        2,
    ));
    assert_eq!(r, vec![vector("system_audio_mode_status_on")]);

    let r = sent(&role.handle(&vector("give_audio_status"), 3));
    assert_eq!(r, vec![vector("report_audio_status_unknown")]);
    role.room_state(46, false, 4);
    let r = sent(&role.handle(&vector("give_audio_status"), 5));
    assert_eq!(r, vec![vector("report_audio_status_46")]);
    role.room_state(100, true, 6);
    let r = sent(&role.handle(&vector("give_audio_status"), 7));
    assert_eq!(r, vec![vector("report_audio_status_100_muted")]);

    let r = sent(&role.handle(&vector("request_short_audio_descriptor_lpcm_ac3"), 8));
    assert_eq!(r, vec![vector("report_short_audio_descriptor_lpcm")]);
    let ac3_eac3_dts = Message::new(TV, 5, opcode::REQUEST_SHORT_AUDIO_DESCRIPTOR, &[2, 10, 7]);
    let r = sent(&role.handle(&ac3_eac3_dts, 9));
    assert_eq!(r, vec![vector("feature_abort_sad_invalid_operand")]);

    let ask = |op| Message::new(TV, 5, op, &[]);
    assert_eq!(
        sent(&role.handle(&ask(opcode::GIVE_DEVICE_POWER_STATUS), 10)),
        vec![vector("report_power_status_on")]
    );
    assert_eq!(
        sent(&role.handle(&ask(opcode::GIVE_OSD_NAME), 11)),
        vec![vector("set_osd_name_chorus")]
    );
    assert_eq!(
        sent(&role.handle(&ask(opcode::GET_CEC_VERSION), 12)),
        vec![vector("cec_version_1_4")]
    );
    assert_eq!(
        sent(&role.handle(&ask(opcode::GIVE_DEVICE_VENDOR_ID), 13)),
        vec![vector("feature_abort_vendor_id")]
    );
    let vendor = Message::new(TV, 5, opcode::VENDOR_COMMAND, &[1, 2]);
    assert_eq!(
        sent(&role.handle(&vendor, 14)),
        vec![vector("feature_abort_unrecognized")]
    );
    let r = sent(&role.handle(&vector("system_audio_mode_request_off"), 15));
    assert_eq!(r, vec![vector("set_system_audio_mode_off")]);

    // The vectors the TV sends are what the fake TV's builders send.
    assert_eq!(
        build::user_control_pressed(TV, 5, ui::VOLUME_UP),
        vector("user_control_pressed_volume_up")
    );
    assert_eq!(
        build::user_control_released(TV, 5),
        vector("user_control_released")
    );
    assert_eq!(
        build::feature_abort(
            5,
            TV,
            opcode::VENDOR_COMMAND,
            AbortReason::UnrecognizedOpcode
        ),
        vector("feature_abort_unrecognized")
    );
    assert_eq!(
        AudioStatus::from_byte(vector("report_audio_status_46").operands[0]).volume,
        Some(46)
    );

    // With ARC configured, the start initiates it.
    let mut arc = AudioSystem::new(
        Config {
            arc: true,
            ..Config::default()
        },
        PhysicalAddress(0x1000),
    );
    assert_eq!(sent(&arc.start(0))[2], vector("initiate_arc"));
    // And with System Audio initiation, the hub asks the TV directly.
    let mut init = AudioSystem::new(
        Config {
            initiate_system_audio: true,
            ..Config::default()
        },
        PhysicalAddress(0x1000),
    );
    init.start(0);
    init.handle(&vector("active_source_tv"), 1);
    assert!(sent(&init.tick(2)).contains(&vector("set_system_audio_mode_on_to_tv")));
}
