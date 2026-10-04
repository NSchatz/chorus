//! Goal 17's inputs in the catalog: stored sources (`source_store`,
//! `source_forget`, the `stored:<id>` spelling that only an alarm carries),
//! input labels (`input_label`, the `streamer` role and the now-playing
//! record the room model writes for it), a line-in in any number of groups,
//! and state-file format 6. The byte-for-byte vectors are under
//! `fixtures/control/v2/` (`source_store`, `source_store-spotify`,
//! `source_forget`, `input_label`, `input_label-clear`, `alarm_set-stored`,
//! `state-inputs` and six refusals) and run in `tests/catalog_v2.rs`.

use chorus_control::catalog::decode_command;
use chorus_control::persist::{load, render, STATE_FORMAT};
use chorus_control::rooms::{
    InputId, InputRole, NowPlaying, PlayState, Source, StoredKind, StoredSource, MAX_DEFINITIONS,
    MAX_STORED_VALUE_LEN,
};
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

fn store(id: &str, kind: &str, value: &str) -> String {
    format!(
        r#"{{"v":2,"t":"source_store","id":"{}","kind":"{}","value":"{}","name":"A name"}}"#,
        id, kind, value
    )
}

fn alarm(source: &str) -> String {
    format!(
        r#"{{"v":2,"t":"alarm_set","alarm":"wake","target":"kitchen","time":"07:00","days":[],"source":"{}","volume":0.300,"ramp_s":30,"duration_min":0,"enabled":true}}"#,
        source
    )
}

const LINE: &str = "endpoint-c/line-1";

fn label(role: &str, name: &str) -> String {
    format!(
        r#"{{"v":2,"t":"input_label","input":"{}","name":"{}","role":"{}"}}"#,
        LINE, name, role
    )
}

fn take(target: &str, source: &str) -> String {
    format!(
        r#"{{"v":2,"t":"take","target":"{}","source":"{}"}}"#,
        target, source
    )
}

#[test]
fn a_stored_source_is_stored_replaced_listed_and_forgotten() {
    let mut zones = house();
    assert!(!zones.encode_state().contains("stored_sources"));
    apply(
        &mut zones,
        &store("radio", "url", "https://radio.example/a.mp3"),
    );
    apply(
        &mut zones,
        &store("wake-list", "spotify", "spotify:playlist:abcDEF123"),
    );
    // Storing under an id again replaces it.
    apply(&mut zones, &store("radio", "url", "http://radio.example/b"));
    assert_eq!(
        zones.stored_sources(),
        &[
            StoredSource {
                id: "radio".into(),
                kind: StoredKind::Url,
                value: "http://radio.example/b".into(),
                name: "A name".into(),
            },
            StoredSource {
                id: "wake-list".into(),
                kind: StoredKind::Spotify,
                value: "spotify:playlist:abcDEF123".into(),
                name: "A name".into(),
            },
        ]
    );
    assert!(zones.encode_state().contains(
        r#""stored_sources":[{"id":"radio","kind":"url","value":"http://radio.example/b","name":"A name"},{"id":"wake-list","kind":"spotify","value":"spotify:playlist:abcDEF123","name":"A name"}]"#
    ));
    apply(&mut zones, r#"{"v":2,"t":"source_forget","id":"radio"}"#);
    assert!(zones.stored_source("radio").is_none());
    let detail = refused(
        &mut zones,
        r#"{"v":2,"t":"source_forget","id":"radio"}"#,
        "id",
    );
    assert!(
        detail.contains("the stored sources are wake-list"),
        "{}",
        detail
    );
}

#[test]
fn a_stored_value_is_held_to_its_kind() {
    let mut zones = house();
    for (kind, value) in [
        ("url", "ftp://radio.example/a"),
        ("url", "file:///etc/hostname"),
        ("url", "radio.example/a"),
        ("url", "http://"),
        ("url", "https:///path-and-no-host"),
        ("url", "https://radio.example/a b"),
        ("spotify", "https://radio.example/a"),
        ("spotify", "spotify:artist:abc"),
        ("spotify", "spotify:playlist:"),
        ("spotify", "spotify:playlist:abc:def"),
        ("spotify", "spotify:playlist:abc-def"),
    ] {
        refused(&mut zones, &store("x", kind, value), "value");
    }
    refused(
        &mut zones,
        &store("x", "podcast", "https://a.example/"),
        "kind",
    );
    refused(
        &mut zones,
        &store("Not An Id", "url", "https://a.example/"),
        "id",
    );
    // The bound on a value's length.
    let long = format!("https://radio.example/{}", "a".repeat(MAX_STORED_VALUE_LEN));
    let detail = refused(&mut zones, &store("x", "url", &long), "value");
    assert!(detail.contains("at most 2048"), "{}", detail);
    let fits = format!(
        "https://radio.example/{}",
        "a".repeat(MAX_STORED_VALUE_LEN - "https://radio.example/".len())
    );
    apply(&mut zones, &store("x", "url", &fits));
    // Every kind of Spotify item the catalog names.
    for item in ["track", "album", "playlist", "episode"] {
        apply(
            &mut zones,
            &store(
                "y",
                "spotify",
                &format!("spotify:{}:4uLU6hMCjMI75M1A2tKUQC", item),
            ),
        );
    }
}

#[test]
fn at_most_32_stored_sources_and_32_labels_are_held() {
    let mut zones = house();
    for n in 0..MAX_DEFINITIONS {
        apply(
            &mut zones,
            &store(&format!("s{}", n), "url", "https://radio.example/a"),
        );
        apply(
            &mut zones,
            &format!(
                r#"{{"v":2,"t":"input_label","input":"amp/line-{}","name":"L","role":"line-in"}}"#,
                n
            ),
        );
    }
    let detail = refused(
        &mut zones,
        &store("one-more", "url", "https://radio.example/a"),
        "",
    );
    assert!(detail.contains("at most 32"), "{}", detail);
    refused(
        &mut zones,
        r#"{"v":2,"t":"input_label","input":"amp/line-99","name":"L","role":"line-in"}"#,
        "",
    );
    // Replacing one is always allowed.
    apply(&mut zones, &store("s0", "url", "https://radio.example/b"));
    apply(
        &mut zones,
        r#"{"v":2,"t":"input_label","input":"amp/line-0","name":"M","role":"streamer"}"#,
    );
}

#[test]
fn stored_is_an_alarms_source_and_never_a_groups() {
    let mut zones = house();
    apply(
        &mut zones,
        &store("radio", "url", "https://radio.example/a.mp3"),
    );
    // An alarm carries it, and only one that exists.
    apply(&mut zones, &alarm("stored:radio"));
    assert_eq!(zones.alarms()[0].source, Source::Stored("radio".into()));
    let detail = refused(&mut zones, &alarm("stored:nothing"), "source");
    assert!(
        detail.contains("the stored sources are radio"),
        "{}",
        detail
    );
    // A take does not, by name; nor does the runtime's own hook.
    let detail = refused(&mut zones, &take("kitchen", "stored:radio"), "source");
    assert!(detail.contains("only an alarm plays"), "{}", detail);
    let before = zones.encode_state();
    let refusal = zones
        .set_group_source("kitchen", Source::Stored("radio".into()))
        .expect_err("a group never plays a stored source");
    assert_eq!(refusal.field, "source");
    assert_eq!(zones.encode_state(), before);
    assert_eq!(zones.source("kitchen"), Source::Stream);
    // A take's refusal of a misspelt source does not offer the spelling.
    let detail = refused(&mut zones, &take("kitchen", "radio"), "source");
    assert!(!detail.contains("stored"), "{}", detail);
    let detail = refused(&mut zones, &alarm("radio"), "source");
    assert!(detail.contains("'stored:<id>'"), "{}", detail);
    // It is not forgotten from under the alarm.
    let detail = refused(
        &mut zones,
        r#"{"v":2,"t":"source_forget","id":"radio"}"#,
        "id",
    );
    assert!(detail.contains("alarm 'wake'"), "{}", detail);
    apply(&mut zones, &alarm("chime:bell"));
    apply(&mut zones, r#"{"v":2,"t":"source_forget","id":"radio"}"#);
}

#[test]
fn a_line_in_plays_in_any_number_of_groups() {
    let mut zones = house();
    let source = format!("line-in:{}", LINE);
    apply(&mut zones, &take("kitchen", &source));
    apply(&mut zones, &take("study", &source));
    apply(
        &mut zones,
        r#"{"v":2,"t":"group_save","group":"pair","name":"Pair","zones":["living","study"]}"#,
    );
    apply(&mut zones, &take("pair", &source));
    let line_in = Source::LineIn(InputId::parse(LINE).unwrap());
    assert_eq!(zones.source("kitchen"), line_in);
    assert_eq!(zones.source("pair"), line_in);
    // One of them choosing something else leaves the other playing it.
    apply(&mut zones, &take("pair", "stream"));
    assert_eq!(zones.source("kitchen"), line_in);
    assert_eq!(zones.source("pair"), Source::Stream);
}

#[test]
fn a_streamer_label_is_shown_by_every_group_playing_the_input() {
    let mut zones = house();
    let source = format!("line-in:{}", LINE);
    apply(&mut zones, &take("kitchen", &source));
    assert!(zones.now_playing("kitchen").is_none(), "no label yet");
    // Labelled while a group plays it: shown at once.
    apply(&mut zones, &label("streamer", "Kitchen streamer"));
    let record = zones.now_playing("kitchen").expect("the label is shown");
    assert_eq!(
        *record,
        NowPlaying {
            title: Some("Kitchen streamer".into()),
            artist: None,
            album: None,
            art_url: None,
            duration_ms: None,
            state: PlayState::Playing,
            via: "streamer".into(),
        }
    );
    // A second group that takes it shows it too; a renamed label is
    // renamed everywhere.
    apply(&mut zones, &take("study", &source));
    apply(&mut zones, &label("streamer", "The streamer"));
    for group in ["kitchen", "study"] {
        assert_eq!(
            zones.now_playing(group).and_then(|r| r.title.clone()),
            Some("The streamer".to_string()),
            "{}",
            group
        );
    }
    assert!(zones.now_playing("living").is_none());
    // A group that plays something else shows nothing, whatever it is: the
    // record does not outlive the line-in, even under a player source.
    apply(&mut zones, &take("study", "player:p0"));
    assert!(zones.now_playing("study").is_none());
    apply(&mut zones, &take("study", &source));
    apply(&mut zones, &take("study", "chime:bell"));
    assert!(zones.now_playing("study").is_none());
    // No runtime may write a record over a line-in's.
    let refusal = zones
        .set_now_playing(
            "kitchen",
            Some(NowPlaying {
                title: Some("Something else".into()),
                artist: None,
                album: None,
                art_url: None,
                duration_ms: None,
                state: PlayState::Playing,
                via: "upnp".into(),
            }),
        )
        .expect_err("a line-in's record is the model's own");
    assert_eq!(refusal.field, "group");
    // The role `line-in` is a name and nothing more; an empty name with it
    // removes the label.
    apply(&mut zones, &label("line-in", "Turntable"));
    assert!(zones.now_playing("kitchen").is_none());
    assert_eq!(
        zones
            .input_label(&InputId::parse(LINE).unwrap())
            .map(|l| l.role),
        Some(InputRole::LineIn)
    );
    apply(&mut zones, &label("line-in", ""));
    assert!(zones.input_labels().is_empty());
    assert!(!zones.encode_state().contains("input_labels"));
    // A streamer has a name.
    refused(&mut zones, &label("streamer", ""), "name");
    refused(&mut zones, &label("microphone", "Mic"), "role");
}

#[test]
fn a_streamers_room_is_the_one_its_endpoint_is_in() {
    let mut zones = house();
    assert_eq!(zones.room_of_endpoint("endpoint-c"), None);
    apply(
        &mut zones,
        r#"{"v":1,"t":"attach","zone":"kitchen","endpoint":"endpoint-c"}"#,
    );
    assert_eq!(zones.room_of_endpoint("endpoint-c"), Some("kitchen"));
}

// --- the state file: format 6 ------------------------------------------------

fn configured() -> Zones {
    let mut zones = house();
    // A URL with everything a state file's own syntax could trip on: a
    // query with `=` and `&`, a percent escape and a fragment's `#`.
    apply(
        &mut zones,
        &store("radio", "url", "https://radio.example/live?x=1&y=%20#top"),
    );
    apply(
        &mut zones,
        &store("wake-list", "spotify", "spotify:playlist:abcDEF123"),
    );
    apply(&mut zones, &alarm("stored:radio"));
    apply(&mut zones, &label("streamer", "Kitchen #1 streamer"));
    apply(&mut zones, &take("kitchen", &format!("line-in:{}", LINE)));
    zones
}

#[test]
fn stored_sources_and_labels_come_back_from_the_state_file_byte_for_byte() {
    let zones = configured();
    let text = render(&zones);
    assert_eq!(STATE_FORMAT, 7);
    assert!(text.contains("format = 7\n"));
    assert!(
        text.contains(
            "[stored-source radio]\nkind = url\nvalue = https://radio.example/live?x=1&y=%20\\#top\nname = A name\n"
        ),
        "{}",
        text
    );
    assert!(
        text.contains(
            "[input-label endpoint-c/line-1]\nname = Kitchen \\#1 streamer\nrole = streamer\n"
        ),
        "{}",
        text
    );
    assert!(text.contains("source = stored:radio\n"), "{}", text);
    let back = load(&text, "127.0.0.1:4010").expect("it reads back");
    assert_eq!(render(&back), text, "and renders back to the same file");
    assert_eq!(back.stored_sources(), zones.stored_sources());
    assert_eq!(back.input_labels(), zones.input_labels());
    assert_eq!(
        back.stored_source("radio").unwrap().value,
        "https://radio.example/live?x=1&y=%20#top"
    );
    // What a group plays is a fact about now: it is not in the file, so the
    // label's record is not either.
    assert!(back.now_playing("kitchen").is_none());
    assert_eq!(back.source("kitchen"), Source::Stream);
}

#[test]
fn a_format_5_file_loads_unchanged_and_has_neither_section() {
    let mut plain = house();
    apply(&mut plain, &alarm("chime:bell"));
    let six = render(&plain);
    let five = six.replace("format = 7\n", "format = 5\n");
    let back = load(&five, "127.0.0.1:4010").expect("a format 5 file loads");
    assert!(back.stored_sources().is_empty() && back.input_labels().is_empty());
    assert_eq!(render(&back), six, "the next write is format 6");
    // A format 6 section in a file that says it is format 5 is refused by
    // name, not guessed at.
    let mixed = render(&configured()).replace("format = 7\n", "format = 5\n");
    let error = load(&mixed, "127.0.0.1:4010").expect_err("format 5 has no such section");
    assert!(
        error.to_string().contains("it was added in format 6"),
        "{}",
        error
    );
}

#[test]
fn a_hand_edited_state_file_is_held_to_the_catalogs_rules() {
    let text = render(&configured());
    for (from, to, says) in [
        (
            "value = https://radio.example/live?x=1&y=%20\\#top\n",
            "value = file:///etc/hostname\n",
            "'http://' or 'https://'",
        ),
        (
            "kind = url\n",
            "kind = podcast\n",
            "not a kind of stored source",
        ),
        (
            "role = streamer\n",
            "role = microphone\n",
            "not an input's role",
        ),
        (
            "[stored-source radio]\n",
            "[stored-source other]\n",
            "which is not a stored source in this file",
        ),
    ] {
        assert!(text.contains(from), "{} is in {}", from, text);
        let edited = text.replacen(from, to, 1);
        let error = load(&edited, "127.0.0.1:4010").expect_err("it is refused");
        assert!(error.to_string().contains(says), "{}: {}", to, error);
    }
}
