//! One schema: the payloads are the control catalog's own bytes.
//!
//! Every committed state vector under `fixtures/control` is cut into its
//! rooms and saved groups, and each piece must be found in the vector byte
//! for byte and read back, with the catalog's own JSON reader, as the entry
//! of that id.

use std::path::PathBuf;

use chorus_control::json::{self, Value};
use chorus_mqtt::payload::{controller_event, retained_of};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/control")
}

fn state_vectors() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for dir in [fixtures(), fixtures().join("v2")] {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if name.starts_with("state") && name.ends_with(".json") {
                let text = std::fs::read_to_string(&path).unwrap();
                out.push((path.display().to_string(), text.trim_end().to_string()));
            }
        }
    }
    out.sort();
    out
}

fn entries(state: &Value, key: &str) -> Vec<Value> {
    match state.get(key) {
        Some(Value::Arr(items)) => items.clone(),
        _ => Vec::new(),
    }
}

#[test]
fn every_state_vector_is_cut_into_its_own_bytes() {
    let vectors = state_vectors();
    assert!(vectors.len() >= 5, "{} state vectors", vectors.len());
    let mut rooms = 0;
    let mut groups = 0;
    for (name, text) in &vectors {
        let whole = json::parse(text).unwrap();
        // The v1 state vectors have no saved groups; a v2 message always has
        // the key. The publisher is only ever given v2.
        let v2 = whole.get("saved_groups").is_some();
        if !v2 {
            assert!(retained_of(text).is_err(), "{}", name);
            continue;
        }
        let cut = retained_of(text).unwrap_or_else(|e| panic!("{}: {}", name, e));
        for (key, pieces) in [("zones", &cut.rooms), ("saved_groups", &cut.groups)] {
            let wanted = entries(&whole, key);
            assert_eq!(pieces.len(), wanted.len(), "{} {}", name, key);
            for ((id, object), entry) in pieces.iter().zip(&wanted) {
                assert!(text.contains(object.as_str()), "{}: {}", name, object);
                assert_eq!(&json::parse(object).unwrap(), entry, "{} {}", name, id);
                assert_eq!(entry.get("id").and_then(Value::as_str), Some(id.as_str()));
                // Canonical bytes: the catalog's writer gives them back.
                assert_eq!(&json::write(entry), object, "{} {}", name, id);
            }
        }
        rooms += cut.rooms.len();
        groups += cut.groups.len();
    }
    assert!(
        rooms >= 4 && groups >= 1,
        "{} rooms, {} groups",
        rooms,
        groups
    );
}

#[test]
fn the_rich_vector_gives_the_rooms_and_saved_groups_it_holds() {
    let text = std::fs::read_to_string(fixtures().join("v2/state-rich.json")).unwrap();
    let cut = retained_of(text.trim_end()).unwrap();
    let ids = |v: &[(String, String)]| v.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>();
    assert_eq!(ids(&cut.rooms)[..3], ["living", "kitchen", "study"]);
    assert!(cut.rooms[0].1.starts_with(
        r#"{"id":"living","name":"Living Room","group":"downstairs","volume":0.857,"#
    ));
    assert!(cut.rooms[0].1.ends_with(
        r#""room_eq":{"enabled":true,"filters":[{"freq_hz":42,"gain_db":-6.00,"q":4.500}]}}"#
    ));
    assert!(!cut.groups.is_empty());
    for (id, object) in &cut.groups {
        assert!(
            object.starts_with(&format!(r#"{{"id":"{}","name":"#, id)),
            "{}",
            object
        );
        assert!(object.ends_with('}'));
    }
}

/// (goal 16) A room whose group plays a player and has a now-playing record
/// carries `source` and `now_playing` in its payload, because the payload is
/// the room's object; a room with no record has neither. Nothing in this
/// crate changed for it.
#[test]
fn a_room_playing_a_player_carries_its_source_and_what_it_plays() {
    let text = std::fs::read_to_string(fixtures().join("v2/state-playing.json")).unwrap();
    let cut = retained_of(text.trim_end()).unwrap();
    let room = |id: &str| &cut.rooms.iter().find(|(r, _)| r == id).unwrap().1;
    let song = concat!(
        r#""source":"player:p0","now_playing":{"title":"Morning Light","artist":"The Example Quartet","#,
        r#""album":"First Takes","art_url":"http://192.0.2.10:8200/art/42.jpg","#,
        r#""duration_ms":215000,"state":"playing","via":"upnp"}}"#
    );
    for id in ["living", "kitchen"] {
        assert!(room(id).ends_with(song), "{}", room(id));
    }
    assert!(room("study").ends_with(concat!(
        r#""source":"player:p1","now_playing":{"title":"Evening news","artist":null,"#,
        r#""album":null,"art_url":null,"duration_ms":null,"state":"paused","via":"upnp"}}"#
    )));
    for id in ["bedroom", "hall"] {
        assert!(!room(id).contains("now_playing"), "{}", room(id));
        assert!(!room(id).contains(r#""source""#), "{}", room(id));
        assert!(room(id).ends_with(r#""room_eq":{"enabled":true,"filters":[]}}"#));
    }
    // The saved group's payload is its definition, as before.
    assert_eq!(
        cut.groups,
        [(
            "downstairs".to_string(),
            r#"{"id":"downstairs","name":"Downstairs","zones":["living","kitchen"],"active":true}"#
                .to_string()
        )]
    );
    // And docs/mqtt.md shows the study's payload as it is.
    let doc = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/mqtt.md"),
    )
    .unwrap();
    assert!(
        doc.contains(&format!("```json\n{}\n```", room("study"))),
        "docs/mqtt.md does not show {}",
        room("study")
    );
}

#[test]
fn a_string_holding_brackets_quotes_and_commas_does_not_move_a_cut() {
    let state = concat!(
        r#"{"v":2,"t":"state","zones":[{"id":"a","name":"x]}\",[{\\"},"#,
        r#" {"id":"b","name":"é, \"q\""}],"saved_groups":[ ],"tail":[1,2]}"#
    );
    let cut = retained_of(state).unwrap();
    assert_eq!(cut.rooms.len(), 2);
    assert_eq!(
        cut.rooms[0],
        (
            "a".to_string(),
            r#"{"id":"a","name":"x]}\",[{\\"}"#.to_string()
        )
    );
    assert_eq!(cut.rooms[1].1, r#"{"id":"b","name":"é, \"q\""}"#);
    assert!(cut.groups.is_empty());
}

#[test]
fn a_text_that_is_not_a_state_message_is_an_error_and_never_a_panic() {
    let whole = r#"{"v":2,"zones":[{"id":"a","name":"x"}],"saved_groups":[{"id":"g"}]}"#;
    assert!(retained_of(whole).is_ok());
    for cut in 0..whole.len() {
        if whole.is_char_boundary(cut) {
            let _ = retained_of(&whole[..cut]);
        }
    }
    for bad in [
        "",
        "[]",
        r#"{"zones":{},"saved_groups":[]}"#,
        r#"{"zones":[{"name":"no id"}],"saved_groups":[]}"#,
        r#"{"zones":[{"id":7}],"saved_groups":[]}"#,
        r#"{"zones":[1 2],"saved_groups":[]}"#,
        r#"{"zones":[]}"#,
    ] {
        assert!(retained_of(bad).is_err(), "{:?}", bad);
    }
}

#[test]
fn a_controller_event_is_one_canonical_object() {
    assert_eq!(
        controller_event("endpoint-a", "kitchen", "volume_step", -5, "", "applied"),
        r#"{"endpoint":"endpoint-a","zone":"kitchen","command":"volume_step","value":-5,"target":"","outcome":"applied"}"#
    );
    let odd = controller_event("a\"b", "z", "join", 0, "down\nstairs", "applied");
    let read = json::parse(&odd).unwrap();
    assert_eq!(read.get("endpoint").and_then(Value::as_str), Some("a\"b"));
    assert_eq!(
        read.get("target").and_then(Value::as_str),
        Some("down\nstairs")
    );
    assert!(!odd.contains('\n'));
}

#[test]
fn the_examples_in_docs_mqtt_md_are_the_fixtures_bytes() {
    let doc = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/mqtt.md"),
    )
    .unwrap();
    let text = std::fs::read_to_string(fixtures().join("v2/state-rich.json")).unwrap();
    let cut = retained_of(text.trim_end()).unwrap();
    let kitchen = &cut.rooms.iter().find(|(id, _)| id == "kitchen").unwrap().1;
    let downstairs = &cut
        .groups
        .iter()
        .find(|(id, _)| id == "downstairs")
        .unwrap()
        .1;
    for example in [kitchen, downstairs] {
        assert!(
            doc.contains(&format!("```json\n{}\n```", example)),
            "docs/mqtt.md does not show {}",
            example
        );
    }
    let event = controller_event("endpoint-a", "kitchen", "volume_step", -5, "", "applied");
    assert!(
        doc.contains(&format!("```json\n{}\n```", event)),
        "{}",
        event
    );
}
