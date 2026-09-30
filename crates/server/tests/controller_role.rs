//! The controller role end to end (K65; brief section 13 item 2).
//!
//! `fixtures/controls/<class>.hex` holds the `controller_command` frames the
//! endpoint's C controls produce for a scripted run of each class's buttons
//! (`firmware/tests/test_controls.c` holds its output to those bytes). Here
//! the same bytes are decoded by `chorus-protocol` and applied to a room
//! through the server's controller translation, so a press on the endpoint
//! and the room's state on the server are checked against one file.
//! `visualizer-sequence.*` is the status LED's fixture; this side decodes
//! every frame of it and checks the `.led` moments cover the whole sequence.

use std::path::PathBuf;

use chorus_control::{Zone, Zones};
use chorus_protocol::v2::{self, Message, Outcome};
use chorus_server::controller::{translate, ControllerAction, TransportRequest};

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/controls")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn parse_hex(text: &str) -> Vec<u8> {
    text.lines()
        .map(|l| l.split('#').next().unwrap_or(""))
        .flat_map(|l| l.split_whitespace())
        .map(|b| u8::from_str_radix(b, 16).expect("a hex byte"))
        .collect()
}

fn decode_all(bytes: &[u8]) -> Vec<Message> {
    let mut at = 0;
    let mut out = Vec::new();
    while at < bytes.len() {
        let d = v2::decode_frame(&bytes[at..]);
        match d.outcome {
            Outcome::Decoded(m) => out.push(m),
            other => panic!("frame at {at} does not decode: {other:?}"),
        }
        assert!(d.consumed > 0);
        at += d.consumed;
    }
    out
}

fn house() -> Zones {
    let mut zones = Zones::new("127.0.0.1:4000");
    zones.add(Zone::new("kitchen")).unwrap();
    zones.add(Zone::new("rack")).unwrap();
    zones
}

/// Apply every frame of a class's fixture to `zone`; return the transport
/// requests, in order.
fn press_through(zones: &mut Zones, zone: &str, file: &str) -> Vec<TransportRequest> {
    let mut transport = Vec::new();
    for message in decode_all(&parse_hex(&fixture(file))) {
        let Message::ControllerCommand(command) = message else {
            panic!("{file} holds a {message:?}, not a controller_command");
        };
        match translate(zones, zone, &command).expect("a controller command translates") {
            ControllerAction::Apply(change) => zones.apply(&change).expect("the change applies"),
            ControllerAction::Transport(request) => transport.push(request),
        }
    }
    transport
}

#[test]
fn compact_buttons_change_the_room_on_the_server() {
    let mut zones = house();
    let transport = press_through(&mut zones, "kitchen", "compact.hex");
    let kitchen = zones.zone("kitchen").unwrap();
    // Full scale, +5 clamps at full, three -5 steps: 1.000 - 0.150.
    assert_eq!(kitchen.volume.literal(), "0.850");
    // Joined "downstairs" on a long press, then left: back in its own group.
    assert_eq!(kitchen.group, "kitchen");
    assert_eq!(
        transport,
        vec![
            TransportRequest::Toggle,
            TransportRequest::Next,
            TransportRequest::Previous
        ]
    );
}

#[test]
fn the_join_in_the_compact_fixture_groups_the_room() {
    // Only the frames up to and including the join, to see the join land.
    let bytes = parse_hex(&fixture("compact.hex"));
    let messages = decode_all(&bytes);
    let mut zones = house();
    for message in messages.iter().take(8) {
        let Message::ControllerCommand(command) = message else {
            unreachable!()
        };
        if let ControllerAction::Apply(change) = translate(&zones, "kitchen", command).unwrap() {
            zones.apply(&change).unwrap();
        }
    }
    assert_eq!(zones.zone("kitchen").unwrap().group, "downstairs");
}

#[test]
fn streaming_amp_front_buttons_change_the_room_on_the_server() {
    let mut zones = house();
    let transport = press_through(&mut zones, "rack", "streaming-amp.hex");
    assert_eq!(zones.zone("rack").unwrap().volume.literal(), "1.000");
    assert_eq!(
        transport,
        vec![
            TransportRequest::Toggle,
            TransportRequest::Next,
            TransportRequest::Previous
        ]
    );
}

#[test]
fn a_controller_is_not_a_bypass() {
    let zones = house();
    let volume = |value| {
        let command = v2::ControllerCommand {
            command: v2::Command::VolumeStep,
            value,
            target: String::new(),
        };
        match translate(&zones, "kitchen", &command).unwrap() {
            ControllerAction::Apply(chorus_control::Command::Volume { volume, .. }) => {
                volume.literal()
            }
            other => panic!("{other:?}"),
        }
    };
    assert_eq!(volume(100), "1.000", "a step past full scale is clamped");
    assert_eq!(volume(-100), "0.000", "a step past silence is clamped");
    let unknown = v2::ControllerCommand {
        command: v2::Command::Toggle,
        value: 0,
        target: String::new(),
    };
    assert!(
        translate(&zones, "attic", &unknown).is_err(),
        "an unknown zone is refused"
    );
    let bad_join = v2::ControllerCommand {
        command: v2::Command::Join,
        value: 0,
        target: "Not A Group!".to_string(),
    };
    assert!(translate(&zones, "kitchen", &bad_join).is_err());
}

#[test]
fn the_visualizer_fixture_decodes_and_its_moments_cover_it() {
    let messages = decode_all(&parse_hex(&fixture("visualizer-sequence.hex")));
    let mut stamps = Vec::new();
    for m in &messages {
        match m {
            Message::VisualizerFrame(f) => stamps.push(f.timestamp_ns),
            Message::Color(c) => stamps.push(c.timestamp_ns),
            other => panic!("the LED sequence holds {other:?}"),
        }
    }
    assert_eq!(messages.len(), 7);
    let moments: Vec<u64> = fixture("visualizer-sequence.led")
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter(|l| !l.is_empty())
        .map(|l| {
            let words: Vec<&str> = l.split_whitespace().collect();
            assert_eq!(words.len(), 6, "a .led line has six words: {l}");
            for w in &words[2..] {
                let v: u32 = w.parse().expect("a colour or brightness");
                assert!(v <= 255);
            }
            words[0].parse().expect("a server timestamp")
        })
        .collect();
    assert!(moments.windows(2).all(|w| w[0] < w[1]), "moments ascend");
    assert!(moments[0] < *stamps.iter().min().unwrap(), "one moment before the stream");
    assert!(
        *moments.last().unwrap() > *stamps.iter().max().unwrap(),
        "the last moment is after every frame"
    );
}
