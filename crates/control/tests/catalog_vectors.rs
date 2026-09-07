//! The committed control catalog vectors, both directions.
//!
//! AC-7: "WHEN a control message is encoded THE SYSTEM SHALL produce bytes
//! identical to the committed golden vector for its catalog version, and WHEN a
//! peer announces a catalog version the server does not implement THE SYSTEM
//! SHALL refuse the session naming the version it was offered and the versions
//! it has, and SHALL apply nothing from it."
//!
//! The vectors are DISCOVERED and not declared: this test enumerates
//! `fixtures/control/` and runs every `.fields` file it finds, the way
//! `crates/protocol/tests/golden_vectors.rs` enumerates its own directory. A
//! vector added to the tree is a vector this test runs, with nothing to
//! register - and a message type in the catalog with NO committed vector turns
//! this test red, which is the drift guard.

mod vectors;

use std::collections::BTreeSet;

use chorus_control::catalog::{decode_command, Command, RefusalKind, Volume};
use chorus_control::json;
use chorus_control::zones::{Zone, Zones};

use vectors::{fixture_dir, read_fields, read_json, vector_names, Fields};

/// Every message type the catalog can put on the wire.
///
/// Listed here rather than derived, because the point of the list is to be the
/// thing a new type has to be added to, which is what makes the completeness
/// assertion below bite.
const EVERY_MESSAGE_TYPE: &[&str] = &[
    "hello", "attach", "name", "group", "ungroup", "volume", "mute", "state", "error", "refused",
];

#[test]
fn every_message_type_in_the_catalog_has_a_committed_vector() {
    let mut covered = BTreeSet::new();
    for name in vector_names() {
        let fields = read_fields(&name);
        covered.insert(fields.get("message_type"));
    }
    for message_type in EVERY_MESSAGE_TYPE {
        assert!(
            covered.contains(*message_type),
            "the catalog has a '{}' message and fixtures/control/ has no vector for it; the \
             vectors are the contract a second implementation is held to, so a type with no \
             vector is a type nobody else can implement. Committed vectors cover: {:?}",
            message_type,
            covered
        );
    }
    for message_type in &covered {
        assert!(
            EVERY_MESSAGE_TYPE.contains(&message_type.as_str()),
            "fixtures/control/ carries a vector for '{}', which is not a message type in this \
             catalog",
            message_type
        );
    }
}

#[test]
fn encoding_every_committed_vector_produces_its_committed_bytes() {
    let mut ran = 0;
    for name in vector_names() {
        let fields = read_fields(&name);
        let want = read_json(&name);
        let got = encode_from(&fields);
        assert_eq!(
            got, want,
            "fixtures/control/{}.json is the contract and encoding {}.fields did not produce it",
            name, name
        );
        ran += 1;
    }
    assert!(ran >= 10, "only {} vectors ran", ran);
}

#[test]
fn decoding_every_committed_command_vector_recovers_its_committed_fields() {
    let mut ran = 0;
    for name in vector_names() {
        let fields = read_fields(&name);
        let message_type = fields.get("message_type");
        if matches!(message_type.as_str(), "state" | "error" | "refused") {
            // Those three are what the SERVER sends. They are checked by the
            // encoding direction above and, for the two refusals, by decoding
            // the committed `input` that produces them.
            continue;
        }
        let bytes = read_json(&name);
        let command = decode_command(&bytes)
            .unwrap_or_else(|e| panic!("{}.json does not decode: {}", name, e));
        assert_eq!(
            command,
            command_from(&fields),
            "{}.json decoded to something other than {}.fields",
            name,
            name
        );
        ran += 1;
    }
    assert!(ran >= 7, "only {} command vectors ran", ran);
}

#[test]
fn an_unknown_catalog_version_refuses_the_session_and_applies_nothing() {
    let fields = read_fields("refused-unknown-version");
    let refusal = decode_command(&fields.get("input")).expect_err("this version is not implemented");
    assert!(
        refusal.ends_the_session(),
        "an unknown catalog version refuses the SESSION, not one message"
    );
    assert!(matches!(
        refusal.kind,
        RefusalKind::UnknownVersion { offered: Some(9) }
    ));
    assert_eq!(refusal.encode(), read_json("refused-unknown-version"));

    // "SHALL apply nothing from it": the message carried a well-formed command
    // beside its version, and no part of it reached the state.
    let zones = two_zones();
    let before = zones.encode_state();
    let refusal = decode_command(r#"{"v":9,"t":"mute","zone":"kitchen","muted":true}"#)
        .expect_err("the version is refused before the command is read");
    assert!(refusal.ends_the_session());
    assert!(!zones.zone("kitchen").unwrap().muted);
    assert_eq!(zones.encode_state(), before);
    // And the refusal names both sides, which is the criterion's own wording.
    assert!(refusal.detail.contains("version 9 was offered"), "{}", refusal);
    assert!(refusal.detail.contains("implements 1"), "{}", refusal);
}

#[test]
fn a_vector_that_is_changed_by_one_byte_is_caught() {
    // The assertion above is only worth having if it can fail. Take the
    // committed volume vector, move it by the smallest step the catalog has,
    // and require the comparison to notice.
    let committed = read_json("volume");
    let nudged = Command::Volume {
        zone: "kitchen".to_string(),
        volume: Volume::from_thousandths(501).unwrap(),
    }
    .encode();
    assert_ne!(nudged, committed);
    assert!(committed.contains("0.500") && nudged.contains("0.501"));
}

fn two_zones() -> Zones {
    let mut zones = Zones::new("127.0.0.1:4010");
    zones.add(Zone::new("kitchen")).unwrap();
    zones.add(Zone::new("study")).unwrap();
    zones
}

/// Build the message a `.fields` file describes and encode it.
fn encode_from(fields: &Fields) -> String {
    match fields.get("message_type").as_str() {
        "state" => {
            let mut zones = Zones::new(&fields.get("default_audio"));
            for (key, value) in fields.pairs() {
                if let Some(group) = key.strip_prefix("group_audio.") {
                    zones.set_group_audio(group, value);
                }
            }
            let mut index = 0usize;
            while fields.has(&format!("zone.{}.id", index)) {
                let at = |k: &str| fields.get(&format!("zone.{}.{}", index, k));
                let mut zone = Zone::new(&at("id"));
                zone.name = at("name");
                zone.group = at("group");
                zone.volume = Volume::parse(&at("volume")).expect("a volume in the vector");
                zone.muted = at("muted") == "1";
                zone.endpoints = list(&at("endpoints"));
                zone.present = list(&at("present"));
                zones.add(zone).expect("a zone in the vector");
                index += 1;
            }
            zones.set_serial(fields.get("serial").parse().expect("a serial"));
            zones.encode_state()
        }
        "error" | "refused" => {
            let input = fields.get("input");
            let refusal = match decode_command(&input) {
                Err(refusal) => refusal,
                Ok(command) => {
                    // A message that decodes but that the state refuses.
                    let mut zones = Zones::new("127.0.0.1:4010");
                    for id in list(&fields.get("zones")) {
                        zones.add(Zone::new(&id)).expect("a zone in the vector");
                    }
                    zones
                        .apply(&command)
                        .expect_err("this vector's input is refused")
                }
            };
            assert_eq!(refusal.field, fields.get("field"), "the field it names");
            assert_eq!(
                u8::from(refusal.ends_the_session()).to_string(),
                fields.get("ends_the_session"),
                "whether it ends the session"
            );
            refusal.encode()
        }
        _ => command_from(fields).encode(),
    }
}

fn command_from(fields: &Fields) -> Command {
    let message_type = fields.get("message_type");
    if message_type == "hello" {
        return Command::Hello;
    }
    let zone = fields.get("zone");
    match message_type.as_str() {
        "hello" => Command::Hello,
        "attach" => Command::Attach {
            zone,
            endpoint: fields.get("endpoint"),
        },
        "name" => Command::Name {
            zone,
            name: fields.get("name"),
        },
        "group" => Command::Group {
            zone,
            group: fields.get("group"),
        },
        "ungroup" => Command::Ungroup { zone },
        "volume" => Command::Volume {
            zone,
            volume: Volume::parse(&fields.get("volume")).expect("a volume in the vector"),
        },
        "mute" => Command::Mute {
            zone,
            muted: fields.get("muted") == "1",
        },
        other => panic!("{} is not a command in this catalog", other),
    }
}

fn list(text: &str) -> Vec<String> {
    text.split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

#[test]
fn every_committed_vector_is_json_this_readers_own_reader_accepts() {
    // A vector nobody else can parse is not a contract. This is the cheapest
    // possible check that the committed bytes are JSON at all, and it runs on
    // the state and refusal vectors too, which the decode test above skips.
    for name in vector_names() {
        let bytes = read_json(&name);
        json::parse(&bytes)
            .unwrap_or_else(|e| panic!("fixtures/control/{}.json is not JSON: {}", name, e));
    }
    assert!(fixture_dir().is_dir());
}
