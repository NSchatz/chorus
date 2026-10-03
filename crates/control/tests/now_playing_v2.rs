//! Goal 16's player sources in the catalog: `player:<id>` as a group's
//! source, the one-group-per-player rule, and the now-playing record the
//! runtime sets (`Zones::set_now_playing`): where it is emitted, how it
//! follows the group's source when groups re-form, and when it is cleared.
//! The byte-for-byte vectors are under `fixtures/control/v2/` (`take-player`,
//! `error-take-player-busy`, `state-playing`) and run in
//! `tests/catalog_v2.rs`.

use chorus_control::catalog::decode_command;
use chorus_control::json::{self, Value};
use chorus_control::persist::{load, render};
use chorus_control::rooms::{NowPlaying, PlayState, Source, MAX_ART_URL_LEN, MAX_NOW_PLAYING_TEXT};
use chorus_control::zones::{Zone, Zones};

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
    for id in ["living", "kitchen", "study"] {
        zones.add(Zone::new(id)).unwrap();
    }
    zones
}

fn take(zones: &mut Zones, target: &str, source: &str) {
    apply(
        zones,
        &format!(
            r#"{{"v":2,"t":"take","target":"{}","source":"{}"}}"#,
            target, source
        ),
    );
}

fn song(state: PlayState) -> NowPlaying {
    NowPlaying {
        title: Some("Morning Light".to_string()),
        artist: Some("The Example Quartet".to_string()),
        album: Some("First Takes".to_string()),
        art_url: Some("http://192.0.2.10:8200/art/42.jpg".to_string()),
        duration_ms: Some(215_000),
        state,
        via: "upnp".to_string(),
    }
}

const SONG: &str = r#"{"title":"Morning Light","artist":"The Example Quartet","album":"First Takes","art_url":"http://192.0.2.10:8200/art/42.jpg","duration_ms":215000,"state":"playing","via":"upnp"}"#;

fn state(zones: &Zones) -> Value {
    json::parse(&zones.encode_state()).unwrap()
}

fn entry(zones: &Zones, list: &str, id: &str) -> Value {
    match state(zones).get(list) {
        Some(Value::Arr(items)) => items
            .iter()
            .find(|g| g.get("id").and_then(Value::as_str) == Some(id))
            .cloned()
            .unwrap_or_else(|| panic!("no '{}' in {}", id, list)),
        _ => panic!("no {} in the state", list),
    }
}

fn member(value: &Value, key: &str) -> Option<String> {
    value.get(key).map(json::write)
}

#[test]
fn a_player_source_is_spelled_player_and_an_identifier() {
    let mut zones = house();
    take(&mut zones, "kitchen", "player:p0");
    assert_eq!(zones.source("kitchen"), Source::Player("p0".to_string()));
    assert_eq!(
        member(&entry(&zones, "groups", "kitchen"), "source").as_deref(),
        Some(r#""player:p0""#)
    );
    for bad in ["player:", "player:P0", "player:p 0", "player:a/b"] {
        let detail = refused(
            &mut zones,
            &format!(
                r#"{{"v":2,"t":"take","target":"kitchen","source":"{}"}}"#,
                bad
            ),
            "source",
        );
        assert!(detail.contains("'player:<id>'"), "{}", detail);
    }
}

#[test]
fn a_state_with_no_record_says_nothing_about_what_plays() {
    let mut zones = house();
    let before = zones.encode_state();
    assert!(!before.contains("now_playing"));
    // A player source alone adds nothing to a room: only a record does.
    take(&mut zones, "kitchen", "player:p0");
    let kitchen = entry(&zones, "zones", "kitchen");
    assert_eq!(member(&kitchen, "source"), None);
    assert_eq!(member(&kitchen, "now_playing"), None);
    assert_eq!(
        member(&entry(&zones, "groups", "kitchen"), "now_playing"),
        None
    );
    assert!(
        json::write(&kitchen).ends_with(r#""room_eq":{"enabled":true,"filters":[]}}"#),
        "the room object still ends where it did: {}",
        json::write(&kitchen)
    );
}

#[test]
fn the_record_is_on_the_group_and_on_each_of_its_rooms_and_nowhere_else() {
    let mut zones = house();
    apply(
        &mut zones,
        r#"{"v":2,"t":"join","zone":"living","target":"kitchen"}"#,
    );
    take(&mut zones, "live-1", "player:p0");
    let serial = zones.serial();
    assert_eq!(
        zones.set_now_playing("live-1", Some(song(PlayState::Playing))),
        Ok(true)
    );
    assert_eq!(zones.serial(), serial + 1, "a subscriber is told");
    let group = entry(&zones, "groups", "live-1");
    assert_eq!(member(&group, "now_playing").as_deref(), Some(SONG));
    assert!(
        json::write(&group).ends_with(&format!(
            r#""audio":"127.0.0.1:4010","now_playing":{}}}"#,
            SONG
        )),
        "after audio: {}",
        json::write(&group)
    );
    for room in ["living", "kitchen"] {
        let room = entry(&zones, "zones", room);
        assert_eq!(member(&room, "source").as_deref(), Some(r#""player:p0""#));
        assert_eq!(member(&room, "now_playing").as_deref(), Some(SONG));
        assert!(
            json::write(&room).ends_with(&format!(
                r#""room_eq":{{"enabled":true,"filters":[]}},"source":"player:p0","now_playing":{}}}"#,
                SONG
            )),
            "after room_eq, source then now_playing: {}",
            json::write(&room)
        );
    }
    let study = entry(&zones, "zones", "study");
    assert_eq!(member(&study, "source"), None);
    assert_eq!(member(&study, "now_playing"), None);
    // The v1 shape is untouched.
    assert!(!zones.encode_state_at(1).contains("now_playing"));
}

#[test]
fn saying_the_same_thing_again_moves_nothing() {
    let mut zones = house();
    take(&mut zones, "kitchen", "player:p0");
    assert_eq!(
        zones.set_now_playing("kitchen", Some(song(PlayState::Playing))),
        Ok(true)
    );
    let (serial, bytes) = (zones.serial(), zones.encode_state());
    assert_eq!(
        zones.set_now_playing("kitchen", Some(song(PlayState::Playing))),
        Ok(false)
    );
    assert_eq!((zones.serial(), zones.encode_state()), (serial, bytes));
    // A change of state is a change.
    assert_eq!(
        zones.set_now_playing("kitchen", Some(song(PlayState::Paused))),
        Ok(true)
    );
    assert_eq!(zones.serial(), serial + 1);
    assert_eq!(
        zones.now_playing("kitchen").map(|n| n.state),
        Some(PlayState::Paused)
    );
    // Clearing it, then clearing it again.
    assert_eq!(zones.set_now_playing("kitchen", None), Ok(true));
    assert_eq!(zones.set_now_playing("kitchen", None), Ok(false));
    assert_eq!(zones.serial(), serial + 2);
    assert!(!zones.encode_state().contains("now_playing"));
}

#[test]
fn a_record_is_refused_where_no_player_source_plays() {
    let mut zones = house();
    let before = zones.encode_state();
    let no_group = zones
        .set_now_playing("attic", Some(song(PlayState::Playing)))
        .unwrap_err();
    assert_eq!(no_group.field, "group");
    let not_a_player = zones
        .set_now_playing("kitchen", Some(song(PlayState::Playing)))
        .unwrap_err();
    assert_eq!(not_a_player.field, "group");
    assert!(
        not_a_player.detail.contains("plays 'stream'"),
        "{}",
        not_a_player.detail
    );
    take(&mut zones, "kitchen", "player:p0");
    let before_via = zones.encode_state();
    let via = zones
        .set_now_playing(
            "kitchen",
            Some(NowPlaying {
                via: "Not An Id".to_string(),
                ..song(PlayState::Playing)
            }),
        )
        .unwrap_err();
    assert_eq!(via.field, "via");
    assert_eq!(zones.encode_state(), before_via);
    assert_ne!(before, before_via);
}

#[test]
fn the_record_is_stored_held_to_its_bounds() {
    let mut zones = house();
    take(&mut zones, "kitchen", "player:p0");
    zones
        .set_now_playing(
            "kitchen",
            Some(NowPlaying {
                title: Some(format!("line one\nline two {}", "x".repeat(400))),
                artist: Some("   ".to_string()),
                album: None,
                art_url: Some(format!("http://192.0.2.10/{}", "a".repeat(MAX_ART_URL_LEN))),
                duration_ms: None,
                state: PlayState::Buffering,
                via: "upnp".to_string(),
            }),
        )
        .unwrap();
    let held = zones.now_playing("kitchen").unwrap();
    let title = held.title.as_deref().unwrap();
    assert_eq!(title.len(), MAX_NOW_PLAYING_TEXT);
    assert!(title.starts_with("line one line two x"), "{}", title);
    assert_eq!(held.artist, None);
    assert_eq!(held.art_url, None);
    assert_eq!(
        member(&entry(&zones, "zones", "kitchen"), "now_playing").unwrap(),
        format!(
            r#"{{"title":"{}","artist":null,"album":null,"art_url":null,"duration_ms":null,"state":"buffering","via":"upnp"}}"#,
            title
        )
    );
}

#[test]
fn a_player_plays_in_one_group_and_a_second_claim_is_refused_by_name() {
    let mut zones = house();
    take(&mut zones, "kitchen", "player:p0");
    let detail = refused(
        &mut zones,
        r#"{"v":2,"t":"take","target":"study","source":"player:p0"}"#,
        "source",
    );
    assert!(
        detail.contains("player 'p0' is playing in group 'kitchen'"),
        "{}",
        detail
    );
    // The runtime's hook is held to the same rule.
    let hook = zones
        .set_group_source("study", Source::Player("p0".to_string()))
        .unwrap_err();
    assert_eq!(hook.field, "source");
    assert_eq!(zones.source("study"), Source::Stream);
    // Another player is free, the same group may say it again, and once the
    // kitchen stops, the study may have it.
    take(&mut zones, "study", "player:p1");
    take(&mut zones, "kitchen", "player:p0");
    take(&mut zones, "kitchen", "none");
    take(&mut zones, "study", "player:p0");
    assert_eq!(zones.player_group("p0").as_deref(), Some("study"));
    assert_eq!(zones.player_group("p1"), None);
}

#[test]
fn the_record_follows_the_source_into_a_live_group_and_back_out() {
    let mut zones = house();
    take(&mut zones, "kitchen", "player:p0");
    zones
        .set_now_playing("kitchen", Some(song(PlayState::Playing)))
        .unwrap();
    // The living room joins the kitchen: a live group forms, playing what the
    // kitchen played, with what it was playing.
    apply(
        &mut zones,
        r#"{"v":2,"t":"join","zone":"living","target":"kitchen"}"#,
    );
    assert_eq!(zones.player_group("p0").as_deref(), Some("live-1"));
    assert_eq!(zones.now_playing("kitchen"), None);
    assert_eq!(zones.now_playing("live-1"), Some(&song(PlayState::Playing)));
    for room in ["living", "kitchen"] {
        assert_eq!(
            member(&entry(&zones, "zones", room), "now_playing").as_deref(),
            Some(SONG)
        );
    }
    // The kitchen leaves: the live group dissolves into the living room,
    // still playing, and the kitchen on its own plays the stream.
    apply(
        &mut zones,
        r#"{"v":2,"t":"join","zone":"kitchen","target":"study"}"#,
    );
    assert_eq!(zones.player_group("p0").as_deref(), Some("living"));
    assert_eq!(zones.now_playing("living"), Some(&song(PlayState::Playing)));
    assert_eq!(zones.now_playing("live-1"), None);
    assert_eq!(
        member(&entry(&zones, "zones", "kitchen"), "now_playing"),
        None
    );
}

#[test]
fn take_keeps_the_player_with_the_target_and_the_rooms_pushed_out_play_nothing() {
    // The study joined the kitchen's own group (catalog v1's `group`), which
    // plays a player. Taking the kitchen pushes the study out.
    let mut zones = house();
    apply(
        &mut zones,
        r#"{"v":1,"t":"group","zone":"study","group":"kitchen"}"#,
    );
    // (A `take` of the kitchen would push the study out, so the group is
    // given its source the way the runtime does.)
    zones
        .set_group_source("kitchen", Source::Player("p0".to_string()))
        .unwrap();
    zones
        .set_now_playing("kitchen", Some(song(PlayState::Playing)))
        .unwrap();
    assert_eq!(
        member(&entry(&zones, "zones", "study"), "now_playing").as_deref(),
        Some(SONG)
    );
    apply(&mut zones, r#"{"v":2,"t":"take","target":"kitchen"}"#);
    assert_eq!(zones.zone("study").unwrap().group, "study");
    assert_eq!(zones.player_group("p0").as_deref(), Some("kitchen"));
    assert_eq!(
        zones.now_playing("kitchen"),
        Some(&song(PlayState::Playing))
    );
    assert_eq!(
        zones.source("study"),
        Source::None,
        "one player, one group: the room pushed out is not given a copy"
    );
    assert_eq!(zones.now_playing("study"), None);
}

#[test]
fn take_with_another_source_sends_the_player_with_the_rooms_pushed_out() {
    let mut zones = house();
    apply(
        &mut zones,
        r#"{"v":1,"t":"group","zone":"study","group":"kitchen"}"#,
    );
    // (A `take` of the kitchen would push the study out, so the group is
    // given its source the way the runtime does.)
    zones
        .set_group_source("kitchen", Source::Player("p0".to_string()))
        .unwrap();
    zones
        .set_now_playing("kitchen", Some(song(PlayState::Paused)))
        .unwrap();
    take(&mut zones, "kitchen", "chime:bell");
    assert_eq!(zones.source("kitchen"), Source::Chime("bell".to_string()));
    assert_eq!(zones.now_playing("kitchen"), None);
    assert_eq!(zones.player_group("p0").as_deref(), Some("study"));
    assert_eq!(zones.now_playing("study"), Some(&song(PlayState::Paused)));
}

#[test]
fn the_record_is_cleared_when_the_source_stops_being_a_player_source() {
    let mut zones = house();
    take(&mut zones, "kitchen", "player:p0");
    zones
        .set_now_playing("kitchen", Some(song(PlayState::Playing)))
        .unwrap();
    // Another player source keeps the record's place (the runtime says what
    // the new one plays); anything else clears it.
    take(&mut zones, "kitchen", "player:p1");
    assert!(zones.now_playing("kitchen").is_some());
    take(&mut zones, "kitchen", "stream");
    assert_eq!(zones.now_playing("kitchen"), None);
    take(&mut zones, "kitchen", "player:p0");
    zones
        .set_now_playing("kitchen", Some(song(PlayState::Playing)))
        .unwrap();
    zones.set_group_source("kitchen", Source::None).unwrap();
    assert_eq!(zones.now_playing("kitchen"), None);
    assert!(!zones.encode_state().contains("now_playing"));
}

#[test]
fn the_record_goes_with_a_group_that_is_no_longer_formed() {
    let mut zones = house();
    take(&mut zones, "kitchen", "player:p0");
    zones
        .set_now_playing("kitchen", Some(song(PlayState::Playing)))
        .unwrap();
    // Catalog v1's `group` moves the kitchen into a group of the client's
    // naming: the group 'kitchen' is formed by nobody and what it played goes.
    apply(
        &mut zones,
        r#"{"v":1,"t":"group","zone":"kitchen","group":"party"}"#,
    );
    assert_eq!(zones.player_group("p0"), None);
    assert_eq!(zones.now_playing("kitchen"), None);
    assert!(!zones.encode_state().contains("now_playing"));
}

#[test]
fn nothing_about_what_plays_is_persisted() {
    let mut zones = house();
    let plain = render(&zones);
    take(&mut zones, "kitchen", "player:p0");
    zones
        .set_now_playing("kitchen", Some(song(PlayState::Playing)))
        .unwrap();
    let text = render(&zones);
    assert!(
        !text.contains("Morning Light") && !text.contains("player:"),
        "{}",
        text
    );
    // Apart from the serial, the file is the one a house with no player has.
    let strip = |t: &str| -> String {
        t.lines()
            .filter(|l| !l.starts_with("serial"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(strip(&text), strip(&plain));
    let loaded = load(&text, "127.0.0.1:4010").unwrap();
    assert_eq!(loaded.source("kitchen"), Source::Stream);
    assert_eq!(loaded.now_playing("kitchen"), None);
}
