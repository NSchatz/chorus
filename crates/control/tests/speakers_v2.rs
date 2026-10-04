//! Goal 14's speakers in the catalog: an adopted speaker is listed unnamed,
//! named (`speaker_name`), assigned a room (`speaker_room`) and forgotten
//! (`speaker_forget`), through the model, the state message and the state
//! file (format 5). The byte-for-byte vectors are under `fixtures/control/v2/`
//! (`speaker_*`, `state-speakers`, `error-speaker-*`) and run in
//! `tests/catalog_v2.rs`.
//!
//! What adopts a speaker is the audio session's handshake (the pinned key,
//! `docs/protocol.md` "Adoption"); here the session layer's part is played by
//! the hooks it calls (`Zones::speaker_adopted`, `speaker_session_up`,
//! `speaker_session_down`, `speaker_key_changed`).

use chorus_control::catalog::decode_command;
use chorus_control::json::{self, Value};
use chorus_control::persist::{load, render, STATE_FORMAT};
use chorus_control::speakers::{KeyChange, MAX_SPEAKERS};
use chorus_control::zones::{Zone, Zones};

const A: &str = "chorus-0123456789ab";
const B: &str = "chorus-ba9876543210";
const KEY_A: &str = "1f0e:2d3c:4b5a:6978";
const KEY_B: &str = "8796:a5b4:c3d2:e1f0";

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

fn house() -> Zones {
    let mut zones = Zones::new("127.0.0.1:4010");
    zones.add(Zone::new("living")).unwrap();
    zones.add(Zone::new("kitchen")).unwrap();
    zones
}

fn players() -> Vec<String> {
    vec!["player".to_string()]
}

/// One member of the state message, as its JSON text.
fn member(zones: &Zones, key: &str) -> Option<String> {
    json::parse(&zones.encode_state())
        .unwrap()
        .get(key)
        .map(json::write)
}

fn speaker_state(zones: &Zones, id: &str) -> Value {
    let state = json::parse(&zones.encode_state()).unwrap();
    match state.get("speakers") {
        Some(Value::Arr(items)) => items
            .iter()
            .find(|s| s.get("id").and_then(Value::as_str) == Some(id))
            .cloned()
            .expect("the speaker is in the state"),
        _ => panic!("no speakers in {}", zones.encode_state()),
    }
}

#[test]
fn a_server_that_adopted_nothing_sends_the_state_it_sent_before_goal_14() {
    let zones = house();
    assert_eq!(member(&zones, "speakers"), None);
    assert_eq!(member(&zones, "key_changes"), None);
    assert!(
        zones.encode_state().ends_with(r#""inputs":[]}"#),
        "the state still ends where it did: {}",
        zones.encode_state()
    );
}

#[test]
fn an_adopted_speaker_is_listed_unnamed_and_in_no_room_without_any_command() {
    let mut zones = house();
    let serial = zones.serial();
    assert_eq!(zones.speaker_adopted(A, KEY_A), Ok(true));
    assert!(zones.serial() > serial, "a subscriber is told");
    assert_eq!(
        json::write(&speaker_state(&zones, A)),
        format!(
            r#"{{"id":"{}","name":"Speaker 89ab","named":false,"room":null,"present":false,"software":"","link":"unknown","key":"{}","roles":[]}}"#,
            A, KEY_A
        )
    );
    // Adopted again (its next session): the same record, nothing moves.
    let serial = zones.serial();
    assert_eq!(zones.speaker_adopted(A, KEY_A), Ok(false));
    assert_eq!(zones.serial(), serial);
    // Its session comes up and goes: present, then not, and still in no room.
    assert!(zones.speaker_session_up(A, "chorus-endpoint 0.1.0", &players()));
    let s = speaker_state(&zones, A);
    assert_eq!(s.get("present").and_then(Value::as_bool), Some(true));
    assert_eq!(
        s.get("software").and_then(Value::as_str),
        Some("chorus-endpoint 0.1.0")
    );
    assert_eq!(json::write(s.get("roles").unwrap()), r#"["player"]"#);
    assert!(zones
        .zones()
        .iter()
        .all(|z| z.endpoints.is_empty() && z.present.is_empty()));
    assert!(zones.speaker_session_down(A));
    assert_eq!(
        speaker_state(&zones, A)
            .get("present")
            .and_then(Value::as_bool),
        Some(false)
    );
}

#[test]
fn speaker_name_names_it_and_marks_it_named() {
    let mut zones = house();
    zones.speaker_adopted(A, KEY_A).unwrap();
    apply(
        &mut zones,
        &format!(
            r#"{{"v":2,"t":"speaker_name","speaker":"{}","name":"Kitchen #1"}}"#,
            A
        ),
    );
    let s = zones.speakers().get(A).unwrap();
    assert_eq!((s.name.as_str(), s.named), ("Kitchen #1", true));
    for (bad, field) in [
        (
            r#"{"v":2,"t":"speaker_name","speaker":"chorus-ffffffffffff","name":"Den"}"#
                .to_string(),
            "speaker",
        ),
        (
            format!(
                r#"{{"v":2,"t":"speaker_name","speaker":"{}","name":""}}"#,
                A
            ),
            "name",
        ),
        (
            format!(
                r#"{{"v":2,"t":"speaker_name","speaker":"{}","name":" x"}}"#,
                A
            ),
            "name",
        ),
        (
            format!(r#"{{"v":2,"t":"speaker_name","speaker":"{}"}}"#, A),
            "name",
        ),
        (
            r#"{"v":2,"t":"speaker_name","speaker":"Not An Id","name":"Den"}"#.to_string(),
            "speaker",
        ),
        (
            format!(
                r#"{{"v":1,"t":"speaker_name","speaker":"{}","name":"Den"}}"#,
                A
            ),
            "t",
        ),
    ] {
        refused(&mut zones, &bad, field);
    }
}

#[test]
fn speaker_room_makes_a_session_only_endpoint_a_member_of_the_room() {
    // The firmware has no control client and never sends `attach`: this is
    // the whole of how it gets a room.
    let mut zones = house();
    zones.speaker_adopted(A, KEY_A).unwrap();
    zones.speaker_session_up(A, "chorus-endpoint 0.1.0", &players());
    apply(
        &mut zones,
        &format!(
            r#"{{"v":2,"t":"speaker_room","speaker":"{}","room":"kitchen"}}"#,
            A
        ),
    );
    let kitchen = zones.zone("kitchen").unwrap();
    assert_eq!(kitchen.endpoints, vec![A.to_string()]);
    assert_eq!(kitchen.present, vec![A.to_string()], "its session is up");
    assert_eq!(
        zones.speakers().get(A).unwrap().room.as_deref(),
        Some("kitchen")
    );

    // Its presence is its session's from here on.
    assert!(zones.speaker_session_down(A));
    let kitchen = zones.zone("kitchen").unwrap();
    assert_eq!(kitchen.endpoints, vec![A.to_string()], "still a member");
    assert!(kitchen.present.is_empty(), "and absent");
    assert!(zones.speaker_session_up(A, "chorus-endpoint 0.1.0", &players()));
    assert_eq!(zones.zone("kitchen").unwrap().present, vec![A.to_string()]);
    // A second session (a reconnect racing the old one's end) and the first
    // ending leave it present.
    zones.speaker_session_up(A, "chorus-endpoint 0.1.0", &players());
    zones.speaker_session_down(A);
    assert_eq!(zones.zone("kitchen").unwrap().present, vec![A.to_string()]);

    // Moved: a member of the new room and of no other.
    apply(
        &mut zones,
        &format!(
            r#"{{"v":2,"t":"speaker_room","speaker":"{}","room":"living"}}"#,
            A
        ),
    );
    assert!(zones.zone("kitchen").unwrap().endpoints.is_empty());
    assert!(zones.zone("kitchen").unwrap().present.is_empty());
    assert_eq!(zones.zone("living").unwrap().present, vec![A.to_string()]);

    // And out of every room, said with null.
    apply(
        &mut zones,
        &format!(
            r#"{{"v":2,"t":"speaker_room","speaker":"{}","room":null}}"#,
            A
        ),
    );
    assert!(zones
        .zones()
        .iter()
        .all(|z| z.endpoints.is_empty() && z.present.is_empty()));
    assert_eq!(zones.speakers().get(A).unwrap().room, None);

    let detail = refused(
        &mut zones,
        &format!(
            r#"{{"v":2,"t":"speaker_room","speaker":"{}","room":"attic"}}"#,
            A
        ),
        "room",
    );
    assert!(detail.contains("living, kitchen"), "{}", detail);
    refused(
        &mut zones,
        r#"{"v":2,"t":"speaker_room","speaker":"chorus-ffffffffffff","room":"living"}"#,
        "speaker",
    );
    refused(
        &mut zones,
        &format!(r#"{{"v":2,"t":"speaker_room","speaker":"{}"}}"#, A),
        "room",
    );
}

#[test]
fn an_explicit_speaker_room_wins_over_a_later_attach_to_another_room() {
    let mut zones = house();
    zones.speaker_adopted(A, KEY_A).unwrap();
    // A Linux endpoint attaches itself, as before goal 14: unchanged.
    apply(
        &mut zones,
        &format!(
            r#"{{"v":1,"t":"attach","zone":"living","endpoint":"{}"}}"#,
            A
        ),
    );
    assert_eq!(zones.zone("living").unwrap().present, vec![A.to_string()]);
    assert_eq!(zones.speakers().get(A).unwrap().room, None);
    // Unassigned, its presence is its control client's, as it always was.
    zones.speaker_session_up(A, "chorus-client 0.1.0", &players());
    zones.speaker_session_down(A);
    assert_eq!(zones.zone("living").unwrap().present, vec![A.to_string()]);

    // The owner assigns it the kitchen; its own start-up flag still says
    // living, and its next attach is accepted and lands in the kitchen.
    apply(
        &mut zones,
        &format!(
            r#"{{"v":2,"t":"speaker_room","speaker":"{}","room":"kitchen"}}"#,
            A
        ),
    );
    assert!(zones.zone("living").unwrap().endpoints.is_empty());
    apply(
        &mut zones,
        &format!(
            r#"{{"v":1,"t":"attach","zone":"living","endpoint":"{}"}}"#,
            A
        ),
    );
    assert!(zones.zone("living").unwrap().endpoints.is_empty());
    assert!(zones.zone("living").unwrap().present.is_empty());
    assert_eq!(
        zones.zone("kitchen").unwrap().endpoints,
        vec![A.to_string()]
    );
    assert_eq!(zones.zone("kitchen").unwrap().present, vec![A.to_string()]);

    // With its session up, its control client saying goodbye does not make
    // it absent: the session is what is playing.
    zones.speaker_session_up(A, "chorus-client 0.1.0", &players());
    assert!(!zones.endpoint_left(A));
    assert_eq!(zones.zone("kitchen").unwrap().present, vec![A.to_string()]);
    zones.speaker_session_down(A);
    assert!(zones.zone("kitchen").unwrap().present.is_empty());
}

#[test]
fn a_bonded_speaker_is_not_moved_or_forgotten_out_from_under_its_set() {
    let mut zones = house();
    zones.speaker_adopted(A, KEY_A).unwrap();
    zones.speaker_adopted(B, KEY_B).unwrap();
    for id in [A, B] {
        apply(
            &mut zones,
            &format!(
                r#"{{"v":2,"t":"attach","zone":"living","endpoint":"{}","link":"wired"}}"#,
                id
            ),
        );
    }
    apply(
        &mut zones,
        &format!(
            r#"{{"v":2,"t":"bond","zone":"living","members":[{{"endpoint":"{}","role":"FL"}},{{"endpoint":"{}","role":"FR"}}]}}"#,
            A, B
        ),
    );
    let detail = refused(
        &mut zones,
        &format!(
            r#"{{"v":2,"t":"speaker_room","speaker":"{}","room":"kitchen"}}"#,
            A
        ),
        "speaker",
    );
    assert!(detail.contains("unbond room 'living' first"), "{}", detail);
    refused(
        &mut zones,
        &format!(
            r#"{{"v":2,"t":"speaker_room","speaker":"{}","room":null}}"#,
            A
        ),
        "speaker",
    );
    refused(
        &mut zones,
        &format!(r#"{{"v":2,"t":"speaker_forget","speaker":"{}"}}"#, A),
        "speaker",
    );
    // Assigning it the room its set is in is not a move.
    apply(
        &mut zones,
        &format!(
            r#"{{"v":2,"t":"speaker_room","speaker":"{}","room":"living"}}"#,
            A
        ),
    );
    assert_eq!(zones.zone("living").unwrap().bond.len(), 2);
}

#[test]
fn speaker_forget_removes_the_record_its_membership_and_its_key_change() {
    let mut zones = house();
    zones.speaker_adopted(A, KEY_A).unwrap();
    zones.speaker_adopted(B, KEY_B).unwrap();
    apply(
        &mut zones,
        &format!(
            r#"{{"v":2,"t":"attach","zone":"kitchen","endpoint":"{}","link":"wireless"}}"#,
            A
        ),
    );
    apply(
        &mut zones,
        &format!(
            r#"{{"v":2,"t":"speaker_room","speaker":"{}","room":"kitchen"}}"#,
            A
        ),
    );
    assert!(zones.speaker_key_changed(KeyChange {
        id: A.to_string(),
        pinned: KEY_A.to_string(),
        offered: KEY_B.to_string(),
    }));
    assert_eq!(
        member(&zones, "key_changes").unwrap(),
        format!(
            r#"[{{"id":"{}","pinned":"{}","offered":"{}"}}]"#,
            A, KEY_A, KEY_B
        )
    );
    apply(
        &mut zones,
        &format!(r#"{{"v":2,"t":"speaker_forget","speaker":"{}"}}"#, A),
    );
    assert!(zones.speakers().get(A).is_none());
    assert!(zones.zone("kitchen").unwrap().endpoints.is_empty());
    assert!(zones.zone("kitchen").unwrap().present.is_empty());
    assert!(zones.links().is_empty(), "its link fact goes with it");
    assert_eq!(member(&zones, "key_changes"), None);
    assert!(zones.speakers().get(B).is_some(), "the other is untouched");

    // Its next session is a new adoption: unnamed, in no room.
    assert_eq!(zones.speaker_adopted(A, KEY_B), Ok(true));
    let s = zones.speakers().get(A).unwrap();
    assert_eq!((s.named, s.room.clone()), (false, None));
    assert_eq!(s.now.key, KEY_B);

    let detail = refused(
        &mut zones,
        r#"{"v":2,"t":"speaker_forget","speaker":"chorus-ffffffffffff"}"#,
        "speaker",
    );
    assert!(detail.contains(A) && detail.contains(B), "{}", detail);

    // A key change under an id with no record (it could not be listed) is
    // still something the owner can forget.
    zones.speaker_key_changed(KeyChange {
        id: "unlisted".to_string(),
        pinned: KEY_A.to_string(),
        offered: KEY_B.to_string(),
    });
    apply(
        &mut zones,
        r#"{"v":2,"t":"speaker_forget","speaker":"unlisted"}"#,
    );
    assert_eq!(member(&zones, "key_changes"), None);
}

#[test]
fn the_registry_is_bounded_and_says_so_by_name() {
    let mut zones = house();
    for n in 0..MAX_SPEAKERS {
        assert_eq!(zones.speaker_adopted(&format!("s-{}", n), KEY_A), Ok(true));
    }
    let before = zones.encode_state();
    let refusal = zones.speaker_adopted("one-more", KEY_A).unwrap_err();
    assert_eq!(refusal.name(), "registry-full");
    assert_eq!(
        zones
            .speaker_adopted("Not An Id", KEY_A)
            .unwrap_err()
            .name(),
        "id-not-an-identifier"
    );
    assert_eq!(zones.encode_state(), before, "nothing moved");
}

// --- the state file: format 5 ----------------------------------------------

fn a_house_with_speakers() -> Zones {
    let mut zones = house();
    zones.speaker_adopted(A, KEY_A).unwrap();
    zones.speaker_adopted(B, KEY_B).unwrap();
    apply(
        &mut zones,
        &format!(
            r#"{{"v":2,"t":"speaker_name","speaker":"{}","name":"Kitchen #1"}}"#,
            A
        ),
    );
    apply(
        &mut zones,
        &format!(
            r#"{{"v":2,"t":"speaker_room","speaker":"{}","room":"kitchen"}}"#,
            A
        ),
    );
    zones
}

#[test]
fn everything_goal_14_holds_survives_the_state_file_as_format_5() {
    let mut zones = a_house_with_speakers();
    let text = render(&zones);
    assert_eq!(STATE_FORMAT, 7);
    assert!(text.contains("format = 7\n"));
    assert!(
        text.contains(&format!(
            "[speaker {}]\nname = Kitchen \\#1\nnamed = 1\nroom = kitchen\n",
            A
        )),
        "{}",
        text
    );
    assert!(
        text.contains(&format!(
            "[speaker {}]\nname = Speaker 3210\nnamed = 0\nroom = \n",
            B
        )),
        "{}",
        text
    );
    let back = load(&text, "127.0.0.1:4010").expect("it reads back");
    assert_eq!(render(&back), text, "byte for byte");
    let a = back.speakers().get(A).unwrap();
    assert_eq!(
        (a.name.as_str(), a.named, a.room.as_deref()),
        ("Kitchen #1", true, Some("kitchen"))
    );
    let b = back.speakers().get(B).unwrap();
    assert_eq!(
        (b.name.as_str(), b.named, b.room.as_deref()),
        ("Speaker 3210", false, None)
    );
    assert_eq!(back.zone("kitchen").unwrap().endpoints, vec![A.to_string()]);

    // What is a fact about now is not in the file: a session being up, the
    // software, the roles, the key's fingerprint, a refused key change.
    zones.speaker_session_up(A, "chorus-endpoint 0.1.0", &players());
    zones.speaker_key_changed(KeyChange {
        id: B.to_string(),
        pinned: KEY_B.to_string(),
        offered: KEY_A.to_string(),
    });
    let with_now = render(&zones);
    assert_eq!(
        with_now.replace(
            &format!("serial = {}\n", zones.serial()),
            &format!("serial = {}\n", back.serial())
        ),
        text,
        "only the serial moved"
    );
    let back = load(&with_now, "127.0.0.1:4010").unwrap();
    let a = back.speakers().get(A).unwrap();
    assert!(!a.now.present() && a.now.software.is_empty() && a.now.key.is_empty());
    assert!(back.speakers().key_changes().is_empty());
    assert!(back.zone("kitchen").unwrap().present.is_empty());
}

#[test]
fn a_format_4_file_loads_unchanged_with_no_speaker() {
    // What goal 13's build wrote: format 4, no [speaker] section anywhere.
    let zones = house();
    let five = render(&zones);
    let four = five.replace("format = 7\n", "format = 4\n");
    assert!(!four.contains("\n[speaker "));
    let back = load(&four, "127.0.0.1:4010").expect("format 4 still loads");
    assert!(back.speakers().all().is_empty());
    // The next write is format 5, and it is the same state.
    assert_eq!(render(&back), five);

    // And a format 4 file has no [speaker] section: it was added in 5.
    let with = render(&a_house_with_speakers()).replace("format = 7\n", "format = 4\n");
    let err = load(&with, "x").unwrap_err();
    assert!(
        err.to_string()
            .contains("a format 4 file has no [speaker] section; it was added in format 5"),
        "{}",
        err
    );
}

#[test]
fn a_format_5_file_missing_or_breaking_a_speaker_field_is_refused_not_defaulted() {
    let text = render(&a_house_with_speakers());
    for (from, to, says) in [
        ("name = Kitchen \\#1\n", "", "has no 'name'"),
        ("named = 1\n", "", "has no 'named'"),
        ("room = kitchen\n", "", "has no 'room'"),
        ("named = 1\n", "named = yes\n", "named = 'yes'"),
        (
            "room = kitchen\n",
            "room = attic\n",
            "is assigned a room 'attic' this file does not have",
        ),
        ("name = Kitchen \\#1\n", "name = \n", "is not a name"),
        (
            "[speaker chorus-0123456789ab]",
            "[speaker Not An Id]",
            "is not a speaker identifier",
        ),
        (
            "[speaker chorus-ba9876543210]",
            "[speaker chorus-0123456789ab]",
            "is given twice",
        ),
    ] {
        assert!(text.contains(from), "the test's own premise: {}", from);
        let broken = text.replacen(from, to, 1);
        let err = load(&broken, "x").unwrap_err();
        assert!(err.to_string().contains(says), "{}: {}", says, err);
    }
}
