//! Goal 18's `announce` in the catalog and the room model: which messages
//! decode, what the model refuses (an unknown target, a URL that is not from
//! a configured origin, a room an alarm is ringing in), and what starting an
//! announcement changes and hands back for its end to restore. The
//! byte-for-byte vectors are under `fixtures/control/v2/` (`announce`,
//! `announce-volume`, `server`, `server-no-origin` and five refusals) and run
//! in `tests/catalog_v2.rs`; the real server is
//! `crates/server/tests/announce.rs`.

use chorus_control::catalog::{decode_command, decode_message, Command, Volume};
use chorus_control::rooms::{Origin, Source};
use chorus_control::zones::{Announcement, AnnouncementState, Zone, Zones};

const URL: &str = "http://ha.example:8123/api/tts_proxy/abc.mp3";

fn v(thousandths: i64) -> Volume {
    Volume::from_thousandths(thousandths).unwrap()
}

fn p(id: &str) -> Source {
    Source::Player(id.to_string())
}

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

fn announce(target: &str, url: &str) -> String {
    format!(
        r#"{{"v":2,"t":"announce","target":"{}","url":"{}"}}"#,
        target, url
    )
}

/// Three rooms, and the home automation's origin.
fn house() -> Zones {
    let mut zones = Zones::new("127.0.0.1:4010");
    for id in ["living", "kitchen", "study"] {
        zones.add(Zone::new(id)).unwrap();
    }
    zones.set_announce_origins(vec![Origin::parse("http://ha.example:8123").unwrap()]);
    zones
}

#[test]
fn announce_is_a_v2_command_with_an_optional_volume() {
    assert_eq!(
        decode_command(&announce("kitchen", URL)).unwrap(),
        Command::Announce {
            target: "kitchen".to_string(),
            url: URL.to_string(),
            volume: None,
        }
    );
    let with_volume = format!(
        r#"{{"v":2,"t":"announce","target":"kitchen","url":"{}","volume":0.300}}"#,
        URL
    );
    let command = decode_command(&with_volume).unwrap();
    assert_eq!(
        command,
        Command::Announce {
            target: "kitchen".to_string(),
            url: URL.to_string(),
            volume: Some(v(300)),
        }
    );
    assert_eq!(command.encode(), with_volume);
    // Not a command of catalog version 1, and no field it does not declare.
    let at_v1 = decode_message(&announce("kitchen", URL).replace(r#""v":2"#, r#""v":1"#))
        .expect_err("a v2-only command at v1");
    assert_eq!(at_v1.field, "t", "{}", at_v1);
    let extra = format!(
        r#"{{"v":2,"t":"announce","target":"kitchen","url":"{}","duck":true}}"#,
        URL
    );
    assert_eq!(decode_command(&extra).unwrap_err().field, "duck");
}

#[test]
fn the_decoder_holds_the_url_to_the_shape_a_stored_url_has() {
    let mut zones = house();
    for bad in [
        "file:///etc/hostname",
        "ftp://ha.example:8123/a.mp3",
        "http://",
        "http://ha.example:8123/a b.mp3",
        "HTTP://ha.example:8123/a.mp3",
    ] {
        refused(&mut zones, &announce("kitchen", bad), "url");
    }
    let long = format!("http://ha.example:8123/{}", "a".repeat(2048));
    let detail = refused(&mut zones, &announce("kitchen", &long), "url");
    assert!(detail.contains("2048"), "{}", detail);
    refused(
        &mut zones,
        r#"{"v":2,"t":"announce","target":"kitchen","url":7}"#,
        "url",
    );
    refused(&mut zones, &announce("Kitchen!", URL), "target");
}

#[test]
fn only_a_url_from_a_configured_origin_is_announced() {
    let mut zones = house();
    apply(&mut zones, &announce("kitchen", URL));
    // The check changes nothing but the serial, as every applied command.
    assert_eq!(zones.source("kitchen"), Source::Stream);
    for elsewhere in [
        "http://ha.example/api/tts_proxy/abc.mp3",
        "https://ha.example:8123/api/tts_proxy/abc.mp3",
        "http://media.example:8123/abc.mp3",
        "http://ha.example.media.example:8123/abc.mp3",
        "http://ha.example:8123@media.example/abc.mp3",
        "http://ha.example:81234/abc.mp3",
    ] {
        let detail = refused(&mut zones, &announce("kitchen", elsewhere), "url");
        assert!(detail.contains("http://ha.example:8123"), "{}", detail);
    }
    // The target is looked at first.
    refused(&mut zones, &announce("garage", URL), "target");
    refused(
        &mut zones,
        &announce("garage", "http://media.example/a.mp3"),
        "target",
    );
    // No origin configured: every announcement is refused, by name.
    zones.set_announce_origins(Vec::new());
    let detail = refused(&mut zones, &announce("kitchen", URL), "url");
    assert!(detail.starts_with("no-announce-origin:"), "{}", detail);
}

#[test]
fn starting_an_announcement_hands_back_what_its_end_restores() {
    let mut zones = house();
    apply(
        &mut zones,
        r#"{"v":2,"t":"volume","zone":"kitchen","volume":0.250}"#,
    );
    apply(
        &mut zones,
        r#"{"v":2,"t":"limit","zone":"kitchen","limit":0.400}"#,
    );
    apply(
        &mut zones,
        r#"{"v":2,"t":"take","target":"kitchen","source":"chime:bell"}"#,
    );
    // A volume above the room's limit is clamped, like every volume path.
    let announced = zones
        .announce_begin("kitchen", p("p0"), Some(v(900)))
        .unwrap();
    assert_eq!(announced.group, "kitchen");
    assert_eq!(announced.previous, Source::Chime("bell".to_string()));
    assert_eq!(announced.volumes, [("kitchen".to_string(), v(250), v(400))]);
    assert_eq!(zones.source("kitchen"), p("p0"));
    assert_eq!(zones.zone("kitchen").unwrap().volume, v(400));
    assert_eq!(zones.announce_group("kitchen").as_deref(), Some("kitchen"));

    // With no volume the rooms keep theirs and nothing is handed back.
    let quiet = zones.announce_begin("study", p("p1"), None).unwrap();
    assert!(quiet.volumes.is_empty());
    assert_eq!(quiet.previous, Source::Stream);

    // A player another group plays is refused, with nothing changed.
    let before = zones.encode_state();
    let refusal = zones
        .announce_begin("living", p("p0"), Some(v(100)))
        .unwrap_err();
    assert_eq!(refusal.field, "source", "{}", refusal);
    assert_eq!(zones.encode_state(), before);
    assert!(zones.announce_begin("garage", p("p2"), None).is_err());
    assert_eq!(zones.encode_state(), before);
}

#[test]
fn a_room_plays_in_the_group_it_is_in_and_a_saved_group_is_taken_first() {
    let mut zones = house();
    apply(
        &mut zones,
        r#"{"v":2,"t":"group_save","group":"downstairs","name":"Downstairs","zones":["living","kitchen"]}"#,
    );
    // The saved group is not active: its rooms are each alone.
    assert_eq!(zones.announce_group("downstairs"), None);
    let announced = zones
        .announce_begin("downstairs", p("p0"), Some(v(200)))
        .unwrap();
    assert_eq!(announced.group, "downstairs");
    for room in ["living", "kitchen"] {
        assert_eq!(zones.zone(room).unwrap().group, "downstairs");
        assert_eq!(zones.zone(room).unwrap().volume, v(200));
    }
    assert_eq!(zones.zone("study").unwrap().group, "study");
    assert_eq!(announced.volumes.len(), 2);
    assert_eq!(zones.source("downstairs"), p("p0"));

    // A room target in a multi-room group plays in that group: nobody is
    // regrouped, and the whole group hears it.
    assert_eq!(
        zones.announce_group("kitchen").as_deref(),
        Some("downstairs")
    );
    let again = zones.announce_begin("kitchen", p("p0"), None).unwrap();
    assert_eq!(again.group, "downstairs");
    assert_eq!(again.previous, p("p0"), "it replaced the one playing");
    assert_eq!(zones.zone("living").unwrap().group, "downstairs");
}

#[test]
fn an_announcement_does_not_interrupt_a_ringing_alarm() {
    let mut zones = house();
    apply(
        &mut zones,
        r#"{"v":2,"t":"join","zone":"living","target":"kitchen"}"#,
    );
    apply(
        &mut zones,
        r#"{"v":2,"t":"alarm_set","alarm":"wake","target":"kitchen","time":"07:00","days":[],"source":"chime:bell","volume":0.500,"ramp_s":0,"duration_min":5,"enabled":true}"#,
    );
    // Not ringing: the announcement is allowed.
    apply(&mut zones, &announce("kitchen", URL));
    zones.set_alarm_ringing("wake", true).unwrap();
    // Ringing: refused for the room, and for the room that shares its group.
    for target in ["kitchen", "living"] {
        let detail = refused(&mut zones, &announce(target, URL), "target");
        assert!(
            detail.contains("alarm 'wake' is ringing"),
            "{}: {}",
            target,
            detail
        );
    }
    // A room the alarm does not ring in is announced to.
    apply(&mut zones, &announce("study", URL));
    zones.set_alarm_ringing("wake", false).unwrap();
    apply(&mut zones, &announce("living", URL));
}

#[test]
fn an_announcement_mixed_over_a_room_changes_no_source_and_only_that_rooms_volume() {
    let mut zones = house();
    apply(
        &mut zones,
        r#"{"v":2,"t":"group_save","group":"downstairs","name":"Downstairs","zones":["living","kitchen"]}"#,
    );
    // A saved group that is not active: the rooms it lists, and no group
    // to play in yet.
    assert_eq!(
        zones.announce_over_rooms("downstairs"),
        ["living".to_string(), "kitchen".to_string()]
    );
    assert_eq!(zones.announce_source("downstairs"), None);
    apply(
        &mut zones,
        r#"{"v":2,"t":"take","target":"downstairs","source":"chime:bell"}"#,
    );
    apply(
        &mut zones,
        r#"{"v":2,"t":"volume","zone":"kitchen","volume":0.250}"#,
    );
    let living_before = zones.zone("living").unwrap().volume;

    // One room of the playing group: that room alone, at the clip's
    // volume, and the group plays what it played.
    assert_eq!(
        zones.announce_over_rooms("kitchen"),
        ["kitchen".to_string()]
    );
    assert_eq!(
        zones.announce_source("kitchen"),
        Some(Source::Chime("bell".to_string()))
    );
    let one = zones.announce_over_begin("kitchen", Some(v(400))).unwrap();
    assert_eq!(one.group, "downstairs");
    assert_eq!(one.previous, Source::Chime("bell".to_string()));
    assert_eq!(one.rooms, ["kitchen".to_string()]);
    assert_eq!(one.volumes, [("kitchen".to_string(), v(250), v(400))]);
    assert_eq!(
        zones.source("downstairs"),
        Source::Chime("bell".to_string())
    );
    assert_eq!(zones.zone("living").unwrap().volume, living_before);
    assert_eq!(zones.zone("living").unwrap().group, "downstairs");

    // The group: every room of it, and still no source changes.
    let all = zones.announce_over_begin("downstairs", None).unwrap();
    assert_eq!(all.rooms.len(), 2);
    assert!(all.volumes.is_empty());
    assert_eq!(
        zones.source("downstairs"),
        Source::Chime("bell".to_string())
    );

    // An unknown target is refused with nothing changed.
    let before = zones.encode_state();
    assert_eq!(
        zones.announce_over_begin("garage", None).unwrap_err().field,
        "target"
    );
    assert_eq!(zones.encode_state(), before);
    assert!(zones.announce_over_rooms("garage").is_empty());
}

#[test]
fn a_mixed_announcement_is_watched_for_an_alarm_a_regrouping_and_its_groups_source() {
    let mut zones = house();
    apply(
        &mut zones,
        r#"{"v":2,"t":"join","zone":"living","target":"kitchen"}"#,
    );
    let group = zones.zone("kitchen").unwrap().group.clone();
    let rooms = ["kitchen".to_string(), "living".to_string()];
    let watch = zones.announce_watch(&rooms, &group);
    assert_eq!(watch.here, rooms);
    assert_eq!(watch.ringing, None);
    assert_eq!(watch.source, Source::Stream);

    // A room that leaves the group is no longer one of its rooms.
    apply(
        &mut zones,
        r#"{"v":2,"t":"join","zone":"living","target":"study"}"#,
    );
    assert!(!zones
        .announce_watch(&rooms, &group)
        .here
        .contains(&"living".to_string()));
    let group = zones.zone("kitchen").unwrap().group.clone();
    assert_eq!(
        zones.announce_watch(&rooms, &group).here,
        ["kitchen".to_string()]
    );

    // An alarm that rings in one of its rooms is named.
    apply(
        &mut zones,
        r#"{"v":2,"t":"alarm_set","alarm":"wake","target":"kitchen","time":"07:00","days":[],"source":"chime:bell","volume":0.500,"ramp_s":0,"duration_min":5,"enabled":true}"#,
    );
    assert_eq!(zones.announce_watch(&rooms, &group).ringing, None);
    zones.set_alarm_ringing("wake", true).unwrap();
    assert_eq!(
        zones.announce_watch(&rooms, &group).ringing.as_deref(),
        Some("wake")
    );
}

#[test]
fn the_state_lists_announcements_only_while_there_is_one_to_name() {
    let mut zones = house();
    let bare = zones.encode_state();
    assert!(!bare.contains("announcements"), "{}", bare);
    let playing = Announcement {
        id: 3,
        target: "kitchen".to_string(),
        rooms: vec!["kitchen".to_string()],
        state: AnnouncementState::Playing,
        reason: None,
    };
    assert!(zones.set_announcements(vec![playing.clone()]));
    assert!(
        !zones.set_announcements(vec![playing.clone()]),
        "no change, no new state"
    );
    let state = zones.encode_state();
    assert!(
        state.ends_with(
            r#","announcements":[{"id":3,"target":"kitchen","rooms":["kitchen"],"state":"playing"}]}"#
        ),
        "{}",
        state
    );
    let failed = Announcement {
        state: AnnouncementState::Failed,
        reason: Some("http status 404".to_string()),
        ..playing
    };
    assert!(zones.set_announcements(vec![failed]));
    assert!(
        zones.encode_state().ends_with(
            r#"{"id":3,"target":"kitchen","rooms":["kitchen"],"state":"failed","reason":"http status 404"}]}"#
        ),
        "{}",
        zones.encode_state()
    );
    for (state, word) in [
        (AnnouncementState::Finished, "finished"),
        (AnnouncementState::Displaced, "displaced"),
    ] {
        assert_eq!(state.name(), word);
    }
    // The v1 shape of the state never carries the list.
    assert!(!zones.encode_state_at(1).contains("announcements"));
    assert!(zones.set_announcements(Vec::new()));
    assert!(!zones.encode_state().contains("announcements"));
}
