//! AC-8, every way a control message can be refused.
//!
//! "IF a control message names a zone that does not exist, sets a volume
//! outside the range the catalog declares, or is not well-formed JSON THEN THE
//! SYSTEM SHALL reject it with an error naming the offending field, SHALL apply
//! no part of it, and SHALL leave every subscriber's state byte-identical to
//! what it was before the message arrived."
//!
//! Three things are asserted for every refusal below, and the third is the one
//! that is easy to leave out: the state is compared BYTE FOR BYTE against what
//! it was, using the same encoder a subscriber would have been sent, so a
//! change that a field-by-field comparison would have missed cannot pass.

use std::sync::Arc;

use chorus_control::catalog::{decode_command, Command, Volume};
use chorus_control::fanout::ControlFanout;
use chorus_control::zones::{Zone, Zones};

/// The server state every case below starts from: two zones, one of them
/// changed from its defaults so that "unchanged" is a real claim.
fn a_server() -> Zones {
    let mut zones = Zones::new("127.0.0.1:4010");
    zones.add(Zone::new("kitchen")).unwrap();
    zones.add(Zone::new("study")).unwrap();
    zones
        .apply(&Command::Volume {
            zone: "kitchen".to_string(),
            volume: Volume::from_thousandths(375).unwrap(),
        })
        .unwrap();
    zones
        .apply(&Command::Name {
            zone: "study".to_string(),
            name: "The Study".to_string(),
        })
        .unwrap();
    zones
}

/// Offer `text` to a server and require that it is refused, that the refusal
/// names `field`, and that the state a subscriber holds does not move by one
/// byte.
fn refused(text: &str, field: &str) -> String {
    let mut zones = a_server();
    let before = zones.encode_state();
    let before_serial = zones.serial();

    // A real subscriber, on the real fanout, holding the real state.
    let fanout = ControlFanout::new();
    let subscriber = fanout.subscribe();
    fanout.broadcast(Arc::new(before.clone()));
    let held = subscriber.recv().expect("the subscriber has the state");

    let refusal = match decode_command(text) {
        Err(refusal) => refusal,
        Ok(command) => zones
            .apply(&command)
            .expect_err(&format!("'{}' has to be refused", text)),
    };

    assert_eq!(
        refusal.field, field,
        "the refusal of '{}' has to name the offending field",
        text
    );
    assert!(
        !refusal.detail.is_empty(),
        "a refusal with no detail is not actionable"
    );
    assert_eq!(
        zones.encode_state(),
        before,
        "'{}' was refused and moved the state anyway",
        text
    );
    assert_eq!(zones.serial(), before_serial, "and the serial moved");

    // Nothing was fanned out, so what the subscriber holds is what it held.
    fanout.broadcast(Arc::new(zones.encode_state()));
    let now = subscriber.recv().expect("still attached");
    assert_eq!(
        now, held,
        "'{}' was refused and a subscriber's state changed",
        text
    );
    refusal.encode()
}

#[test]
fn a_zone_that_does_not_exist_is_refused_and_the_zones_that_do_are_named() {
    let encoded = refused(r#"{"v":1,"t":"mute","zone":"bathroom","muted":true}"#, "zone");
    assert!(encoded.contains("there is no zone 'bathroom'"), "{}", encoded);
    assert!(encoded.contains("kitchen, study"), "{}", encoded);
    // Every command that names a zone, not only one of them.
    refused(r#"{"v":1,"t":"name","zone":"bathroom","name":"Bathroom"}"#, "zone");
    refused(r#"{"v":1,"t":"group","zone":"bathroom","group":"downstairs"}"#, "zone");
    refused(r#"{"v":1,"t":"ungroup","zone":"bathroom"}"#, "zone");
    refused(r#"{"v":1,"t":"volume","zone":"bathroom","volume":0.500}"#, "zone");
    refused(r#"{"v":1,"t":"attach","zone":"bathroom","endpoint":"endpoint-a"}"#, "zone");
}

#[test]
fn a_volume_outside_the_declared_range_is_refused_at_every_edge() {
    for text in [
        r#"{"v":1,"t":"volume","zone":"kitchen","volume":1.001}"#,
        r#"{"v":1,"t":"volume","zone":"kitchen","volume":2.000}"#,
        r#"{"v":1,"t":"volume","zone":"kitchen","volume":-0.001}"#,
        r#"{"v":1,"t":"volume","zone":"kitchen","volume":100}"#,
        // More precision than the catalog declares is refused rather than
        // rounded: a value that came back different from the value that went
        // in is what the golden vectors exist to prevent.
        r#"{"v":1,"t":"volume","zone":"kitchen","volume":0.5001}"#,
        // An exponent is a second spelling of a number and the catalog has
        // exactly one.
        r#"{"v":1,"t":"volume","zone":"kitchen","volume":5e-1}"#,
        // A string is not a number.
        r#"{"v":1,"t":"volume","zone":"kitchen","volume":"0.500"}"#,
    ] {
        let encoded = refused(text, "volume");
        assert!(encoded.contains(r#""t":"error""#), "{}", encoded);
    }
}

#[test]
fn a_volume_at_each_end_of_the_declared_range_is_accepted() {
    // The refusals above are only meaningful if the range is not empty.
    let mut zones = a_server();
    for thousandths in [0i64, 1, 500, 999, 1_000] {
        let text = format!(
            r#"{{"v":1,"t":"volume","zone":"kitchen","volume":{}}}"#,
            Volume::from_thousandths(thousandths).unwrap().literal()
        );
        let command = decode_command(&text).unwrap_or_else(|e| panic!("{}: {}", text, e));
        zones.apply(&command).unwrap_or_else(|e| panic!("{}: {}", text, e));
        assert_eq!(
            zones.zone("kitchen").unwrap().volume.thousandths(),
            thousandths as u32
        );
    }
}

#[test]
fn a_message_that_is_not_well_formed_json_is_refused_and_names_no_field() {
    for text in [
        "",
        "   ",
        "not json at all",
        "{",
        r#"{"v":1,"t":"mute","zone":"kitchen",}"#,
        r#"{"v":1,"t":"mute","zone":"kitchen","muted":true}}"#,
        r#"{'v':1}"#,
        r#"{"v":1,"t":"mute","zone":"kitchen","muted":true}{"v":1,"t":"hello"}"#,
        // A duplicate field has two meanings and no way to choose.
        r#"{"v":1,"t":"mute","zone":"kitchen","zone":"study","muted":true}"#,
        // Not an object.
        r#"[{"v":1,"t":"hello"}]"#,
        r#""hello""#,
    ] {
        let encoded = refused(text, "");
        assert!(
            encoded.contains("not well-formed JSON") || encoded.contains("is an"),
            "{} -> {}",
            text,
            encoded
        );
    }
}

#[test]
fn a_message_whose_shape_is_wrong_is_refused_naming_the_field() {
    // A field the command does not declare.
    refused(r#"{"v":1,"t":"ungroup","zone":"kitchen","volume":0.500}"#, "volume");
    // A field the command requires and does not have.
    refused(r#"{"v":1,"t":"volume","zone":"kitchen"}"#, "volume");
    refused(r#"{"v":1,"t":"mute","zone":"kitchen"}"#, "muted");
    // A field of the wrong type.
    refused(r#"{"v":1,"t":"mute","zone":"kitchen","muted":1}"#, "muted");
    refused(r#"{"v":1,"t":"mute","zone":5,"muted":true}"#, "zone");
    // An identifier that is not one.
    refused(r#"{"v":1,"t":"mute","zone":"Kitchen Zone","muted":true}"#, "zone");
    refused(r#"{"v":1,"t":"mute","zone":"","muted":true}"#, "zone");
    // A name that would not survive the persisted state file.
    refused(r#"{"v":1,"t":"name","zone":"kitchen","name":"two\nlines"}"#, "name");
    refused(r#"{"v":1,"t":"name","zone":"kitchen","name":" padded "}"#, "name");
    refused(r#"{"v":1,"t":"name","zone":"kitchen","name":""}"#, "name");
    // A type this catalog does not have.
    refused(r#"{"v":1,"t":"reboot","zone":"kitchen"}"#, "t");
    refused(r#"{"v":1}"#, "t");
}

#[test]
fn a_refused_message_does_not_stop_the_next_one_being_applied() {
    // The session survives a rejection: that is what makes "one message" the
    // cost of a bad message.
    let mut zones = a_server();
    assert!(decode_command(r#"{"v":1,"t":"volume","zone":"kitchen","volume":9.000}"#).is_err());
    let command = decode_command(r#"{"v":1,"t":"volume","zone":"kitchen","volume":0.250}"#).unwrap();
    zones.apply(&command).unwrap();
    assert_eq!(zones.zone("kitchen").unwrap().volume.thousandths(), 250);
}
