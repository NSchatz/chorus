//! A room's correction can be put back (catalog v2 `room_eq_undo`,
//! `docs/decisions/0200-a-recording-is-fitted-and-not-kept.md`).
//!
//! The rule under test is one step deep: a `room_eq` that carries `filters`
//! is an apply and keeps what stood before it, filters and flag; one
//! `room_eq_undo` puts that back and leaves nothing to undo; a `room_eq` that
//! only switches the correction on or off keeps the undo as it is. The state
//! says whether there is one, and the state file carries it (format 10).

use chorus_control::catalog::decode_command;
use chorus_control::json::{self, Value};
use chorus_control::persist::{load, render, STATE_FORMAT};
use chorus_control::sound::RoomEq;
use chorus_control::zones::{Zone, Zones};

const FIRST: &str = r#"[{"freq_hz":42,"gain_db":-6.00,"q":4.500}]"#;
const SECOND: &str =
    r#"[{"freq_hz":45,"gain_db":-9.32,"q":6.409},{"freq_hz":119,"gain_db":-6.53,"q":5.135}]"#;

fn house() -> Zones {
    let mut zones = Zones::new("127.0.0.1:4010");
    zones.add(Zone::new("living")).unwrap();
    zones.add(Zone::new("kitchen")).unwrap();
    zones
}

fn apply(zones: &mut Zones, text: &str) {
    let command = decode_command(text).unwrap_or_else(|e| panic!("{}: {}", text, e));
    zones
        .apply(&command)
        .unwrap_or_else(|e| panic!("{}: {}", text, e));
}

fn refused(zones: &mut Zones, text: &str) -> String {
    let command = decode_command(text).unwrap_or_else(|e| panic!("{}: {}", text, e));
    zones.apply(&command).unwrap_err().to_string()
}

fn filters(zones: &mut Zones, filters: &str, enabled: Option<bool>) {
    let enabled = match enabled {
        Some(e) => format!(r#","enabled":{}"#, e),
        None => String::new(),
    };
    apply(
        zones,
        &format!(
            r#"{{"v":2,"t":"room_eq","zone":"living","filters":{}{}}}"#,
            filters, enabled
        ),
    );
}

const UNDO: &str = r#"{"v":2,"t":"room_eq_undo","zone":"living"}"#;

/// The room's `room_eq` as the state message carries it, encoded.
fn in_the_state(zones: &Zones, zone: &str) -> String {
    let state = json::parse(&zones.encode_state_at(2)).unwrap();
    let Some(Value::Arr(rooms)) = state.get("zones") else {
        panic!("the state has no zones");
    };
    let room = rooms
        .iter()
        .find(|z| z.get("id").and_then(Value::as_str) == Some(zone))
        .unwrap();
    json::write(room.get("room_eq").unwrap())
}

fn eq(zones: &Zones) -> RoomEq {
    zones.zone("living").unwrap().room_eq.clone()
}

#[test]
fn an_undo_after_an_apply_over_an_earlier_correction_restores_its_filters_and_its_flag() {
    for earlier_enabled in [true, false] {
        let mut zones = house();
        filters(&mut zones, FIRST, Some(earlier_enabled));
        let earlier = eq(&zones);
        assert_eq!(earlier.enabled, earlier_enabled);
        assert_eq!(earlier.filters.len(), 1);

        filters(&mut zones, SECOND, Some(true));
        assert_eq!(eq(&zones).filters.len(), 2);
        assert!(in_the_state(&zones, "living").ends_with(r#","undo":true}"#));

        apply(&mut zones, UNDO);
        assert_eq!(eq(&zones), earlier, "exactly the earlier correction");
        // One step: the undo is spent, and the state stops offering it.
        assert!(!in_the_state(&zones, "living").contains("undo"));
        assert!(refused(&mut zones, UNDO).contains("nothing-to-undo"));
        assert_eq!(eq(&zones), earlier, "a refused undo changes nothing");
    }
}

#[test]
fn an_undo_after_a_first_apply_leaves_no_correction() {
    let mut zones = house();
    let untouched = eq(&zones);
    assert_eq!(untouched, RoomEq::default());
    assert!(!in_the_state(&zones, "living").contains("undo"));
    assert!(refused(&mut zones, UNDO).contains("nothing-to-undo"));

    filters(&mut zones, SECOND, Some(true));
    assert!(in_the_state(&zones, "living").contains(r#""undo":true"#));
    // The room beside it was never corrected and says what it always said.
    assert_eq!(
        in_the_state(&zones, "kitchen"),
        r#"{"enabled":true,"filters":[]}"#
    );

    apply(&mut zones, UNDO);
    assert_eq!(eq(&zones), untouched);
    assert_eq!(
        in_the_state(&zones, "living"),
        r#"{"enabled":true,"filters":[]}"#
    );
}

#[test]
fn switching_the_correction_off_and_on_is_not_an_apply_and_keeps_the_undo() {
    let mut zones = house();
    filters(&mut zones, FIRST, None);
    let earlier = eq(&zones);
    filters(&mut zones, SECOND, None);
    apply(
        &mut zones,
        r#"{"v":2,"t":"room_eq","zone":"living","enabled":false}"#,
    );
    apply(
        &mut zones,
        r#"{"v":2,"t":"room_eq","zone":"living","enabled":true}"#,
    );
    assert!(in_the_state(&zones, "living").contains(r#""undo":true"#));
    apply(&mut zones, UNDO);
    assert_eq!(eq(&zones), earlier);
}

#[test]
fn an_undo_is_a_v2_command_for_a_room_this_server_has() {
    let mut zones = house();
    assert!(decode_command(r#"{"v":1,"t":"room_eq_undo","zone":"living"}"#).is_err());
    assert!(decode_command(r#"{"v":2,"t":"room_eq_undo"}"#).is_err());
    assert!(decode_command(r#"{"v":2,"t":"room_eq_undo","zone":"living","depth":2}"#).is_err());
    assert!(refused(&mut zones, r#"{"v":2,"t":"room_eq_undo","zone":"attic"}"#).contains("attic"));
}

#[test]
fn the_undo_survives_a_restart_and_an_older_file_has_none() {
    assert_eq!(STATE_FORMAT, 10);
    let mut zones = house();
    filters(&mut zones, FIRST, Some(false));
    filters(&mut zones, SECOND, Some(true));
    let saved = render(&zones);
    assert!(saved.contains("format = 10\n"), "{}", saved);
    assert!(saved.contains("room_eq_undo = 0\n"), "{}", saved);
    assert!(
        saved.contains("room_eq_undo_filters = 42 -6.00 4.500\n"),
        "{}",
        saved
    );
    assert!(saved.contains("room_eq_undo = none\nroom_eq_undo_filters = \n"));

    let mut back = load(&saved, "127.0.0.1:4010").expect("format 10 loads");
    assert_eq!(render(&back), saved);
    assert_eq!(
        back.zone("living").unwrap().room_eq_undo,
        zones.zone("living").unwrap().room_eq_undo
    );
    apply(&mut back, UNDO);
    let restored = back.zone("living").unwrap();
    assert!(!restored.room_eq.enabled);
    assert_eq!(restored.room_eq.filters.len(), 1);
    assert!(restored.room_eq_undo.is_none());

    // What the build before this one wrote: format 9, neither field.
    let nine: String = saved
        .replace("format = 10\n", "format = 9\n")
        .lines()
        .filter(|l| !l.starts_with("room_eq_undo"))
        .map(|l| format!("{}\n", l))
        .collect();
    let old = load(&nine, "127.0.0.1:4010").expect("format 9 still loads");
    assert!(old.zones().iter().all(|z| z.room_eq_undo.is_none()));
    assert_eq!(old.zone("living").unwrap().room_eq, eq(&zones));
    assert!(render(&old).contains("format = 10\n"));

    // A format 10 file says it for every room, and only in the shape it has.
    let missing = saved.replacen("room_eq_undo = none\n", "", 1);
    assert!(load(&missing, "x")
        .unwrap_err()
        .to_string()
        .contains("room_eq_undo"));
    for (from, to) in [
        ("room_eq_undo = 0\n", "room_eq_undo = yes\n"),
        (
            "room_eq_undo = none\nroom_eq_undo_filters = \n",
            "room_eq_undo = none\nroom_eq_undo_filters = 42 -6.00 4.500\n",
        ),
        (
            "room_eq_undo_filters = 42 -6.00 4.500\n",
            "room_eq_undo_filters = 42 6.00 4.500\n",
        ),
    ] {
        assert!(saved.contains(from), "{}", from);
        assert!(load(&saved.replacen(from, to, 1), "x").is_err(), "{}", to);
    }
}
