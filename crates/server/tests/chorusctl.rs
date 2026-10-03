//! chorusctl against the real `chorus-server` binary: at least one verb of
//! each of its seven nouns (rooms, groups, volume, inputs, sources, endpoints,
//! updates),
//! through chorusctl's own entry point, with every answer checked against the
//! server's state.
//!
//! The server is on loopback with kernel-assigned ports and a throwaway
//! identity; one Linux-client session is opened so there is a real adopted
//! speaker to name and place. Nothing here is timing evidence.

mod common;

use std::time::{Duration, Instant};

use chorus_control::json::{self, Value};
use common::{Player, RunningServer};

/// Run chorusctl against `server` with these arguments.
fn ctl(server: &RunningServer, args: &[&str]) -> chorus_ctl::Outcome {
    let mut all = vec!["--server".to_string(), server.control.clone()];
    all.extend(args.iter().map(|a| a.to_string()));
    chorus_ctl::run_with(&all, None)
}

/// A run that must exit 0: its stdout.
fn ok(server: &RunningServer, args: &[&str]) -> String {
    let outcome = ctl(server, args);
    assert_eq!(
        outcome.code, 0,
        "chorusctl {:?} exited {}: {}",
        args, outcome.code, outcome.stderr
    );
    assert_eq!(outcome.stderr, "", "chorusctl {:?}", args);
    outcome.stdout
}

/// A run the server must refuse: exit 3, with the server's words.
fn refused(server: &RunningServer, args: &[&str]) -> String {
    let outcome = ctl(server, args);
    assert_eq!(
        outcome.code, 3,
        "chorusctl {:?} exited {}: {}{}",
        args, outcome.code, outcome.stdout, outcome.stderr
    );
    assert_eq!(outcome.stdout, "", "chorusctl {:?}", args);
    outcome.stderr
}

fn parsed(text: &str) -> Value {
    json::parse(text.trim_end()).unwrap_or_else(|e| panic!("{:?} in {}", e, text))
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("no string '{}' in {:?}", key, value))
}

#[test]
fn chorusctl_drives_the_real_server_through_all_seven_nouns() {
    let firmware = std::env::temp_dir().join(common::fresh_id("chorusctl-firmware"));
    std::fs::create_dir_all(&firmware).expect("a firmware directory");
    let server = RunningServer::start(&[
        "--source",
        "tone",
        "--serve-forever",
        "--zone",
        "kitchen",
        "--zone",
        "study",
        "--zone",
        "den",
        "--firmware-dir",
        firmware.to_str().expect("a UTF-8 path"),
    ]);

    // rooms: list, name, show; --json is the server's own zones.
    let listed = ok(&server, &["rooms", "list"]);
    assert!(listed.starts_with("ROOM "), "{}", listed);
    for room in ["kitchen", "study", "den"] {
        assert!(listed.contains(&format!("\n{} ", room)), "{}", listed);
    }
    let named = ok(&server, &["rooms", "name", "kitchen", "The Kitchen"]);
    assert!(named.contains("\nname: The Kitchen\n"), "{}", named);
    let zones = ok(&server, &["rooms", "list", "--json"]);
    assert!(
        server.state().contains(zones.trim_end()),
        "rooms list --json is not the zones of the server's state:\n{}",
        zones
    );
    let shown = parsed(&ok(&server, &["rooms", "show", "kitchen", "--json"]));
    assert_eq!(text(&shown, "name"), "The Kitchen");
    assert_eq!(ctl(&server, &["rooms", "show", "attic"]).code, 4);

    // volume: limit, set (clamped by the server, not by chorusctl), step,
    // mute, get.
    let limited = ok(&server, &["volume", "limit", "kitchen", "0.600"]);
    assert!(limited.contains("limit=0.600"), "{}", limited);
    let set = ok(&server, &["volume", "set", "kitchen", "0.900"]);
    assert_eq!(
        set,
        "kitchen volume=0.600 muted=no limit=0.600 effective_limit=0.600\n"
    );
    let stepped = ok(&server, &["volume", "step", "kitchen", "-50"]);
    assert!(stepped.starts_with("kitchen volume=0.550 "), "{}", stepped);
    let muted = ok(&server, &["volume", "mute", "kitchen"]);
    assert!(muted.contains(" muted=yes "), "{}", muted);
    ok(&server, &["volume", "unmute", "kitchen"]);
    let got = parsed(&ok(&server, &["volume", "get", "kitchen", "--json"]));
    assert_eq!(got.get("volume").and_then(Value::as_num), Some("0.550"));
    assert_eq!(got.get("muted").and_then(Value::as_bool), Some(false));
    let no_room = refused(&server, &["volume", "set", "attic", "0.5"]);
    assert!(no_room.contains("attic"), "{}", no_room);

    // groups: join, list, save, take, leave, delete.
    ok(&server, &["groups", "join", "study", "kitchen"]);
    let state = parsed(&server.state());
    let group_of = |state: &Value, room: &str| -> String {
        match state.get("zones") {
            Some(Value::Arr(zones)) => zones
                .iter()
                .find(|z| text(z, "id") == room)
                .map(|z| text(z, "group").to_string())
                .expect("the room is in the state"),
            _ => panic!("no zones"),
        }
    };
    assert_eq!(group_of(&state, "study"), group_of(&state, "kitchen"));
    let live = group_of(&state, "kitchen");
    let group_volume = ok(&server, &["volume", "get", "--group", &live]);
    assert!(
        group_volume.starts_with(&format!("{} volume=", live)),
        "{}",
        group_volume
    );
    ok(
        &server,
        &["groups", "save", "pair", "The Pair", "kitchen", "study"],
    );
    let groups = parsed(&ok(&server, &["groups", "list", "--json"]));
    let saved = match groups.get("saved_groups") {
        Some(Value::Arr(saved)) => saved.clone(),
        other => panic!("no saved_groups: {:?}", other),
    };
    assert_eq!(saved.len(), 1);
    assert_eq!(text(&saved[0], "name"), "The Pair");
    ok(&server, &["groups", "take", "pair", "--source", "stream"]);
    let state = parsed(&server.state());
    assert_eq!(group_of(&state, "kitchen"), "pair");
    assert_eq!(group_of(&state, "study"), "pair");
    ok(&server, &["groups", "leave", "study"]);
    assert_eq!(group_of(&parsed(&server.state()), "study"), "study");
    let deleted = ok(&server, &["groups", "delete", "pair"]);
    assert!(!deleted.contains("SAVED"), "{}", deleted);

    // inputs: list (no endpoint offers one here), and select, which the
    // server takes for an input not offered yet (the group waits for it).
    assert_eq!(ok(&server, &["inputs", "list"]), "no inputs\n");
    assert_eq!(ok(&server, &["inputs", "list", "--json"]), "[]\n");
    let selected = ok(&server, &["inputs", "select", "nobody/line-1", "den"]);
    assert!(
        selected.contains("line-in:nobody/line-1  den\n"),
        "{}",
        selected
    );
    let groups = parsed(&ok(&server, &["groups", "list", "--json"]));
    let den = match groups.get("groups") {
        Some(Value::Arr(groups)) => groups
            .iter()
            .find(|g| text(g, "id") == "den")
            .cloned()
            .expect("den's group"),
        other => panic!("no groups: {:?}", other),
    };
    assert_eq!(text(&den, "source"), "line-in:nobody/line-1");
    let no_target = refused(&server, &["inputs", "select", "nobody/line-1", "attic"]);
    assert!(no_target.contains("attic"), "{}", no_target);
    ok(&server, &["groups", "take", "den", "--source", "stream"]);
    // (goal 17) A label on an input, and its removal.
    assert_eq!(ok(&server, &["inputs", "labels"]), "no labelled inputs\n");
    let labelled = ok(
        &server,
        &[
            "inputs",
            "label",
            "nobody/line-1",
            "streamer",
            "Den streamer",
        ],
    );
    assert_eq!(
        labelled,
        "INPUT          ROLE      NAME\nnobody/line-1  streamer  Den streamer\n"
    );
    assert_eq!(
        ok(&server, &["inputs", "labels", "--json"]),
        "[{\"input\":\"nobody/line-1\",\"name\":\"Den streamer\",\"role\":\"streamer\"}]\n"
    );
    assert_eq!(
        ok(&server, &["inputs", "unlabel", "nobody/line-1"]),
        "no labelled inputs\n"
    );

    // sources (goal 17): store, list, the alarm-only rule, forget.
    assert_eq!(ok(&server, &["sources", "list"]), "no stored sources\n");
    let stored = ok(
        &server,
        &[
            "sources",
            "store",
            "morning-radio",
            "url",
            "Morning radio",
            "https://radio.example/stream.mp3",
        ],
    );
    assert!(
        stored.contains("morning-radio  url   Morning radio  https://radio.example/stream.mp3\n"),
        "{}",
        stored
    );
    let not_a_take = refused(
        &server,
        &["groups", "take", "den", "--source", "stored:morning-radio"],
    );
    assert!(not_a_take.contains("only an alarm plays"), "{}", not_a_take);
    assert_eq!(
        ok(&server, &["sources", "forget", "morning-radio"]),
        "no stored sources\n"
    );
    let unknown = refused(&server, &["sources", "forget", "morning-radio"]);
    assert!(unknown.contains("no stored source"), "{}", unknown);

    // endpoints: a real session is adopted; list, show, name, room, forget.
    let id = common::fresh_id("chorusctl-speaker");
    let player = Player::connect(&server.audio, &id, 0);
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let shown = ctl(&server, &["endpoints", "show", &id, "--json"]);
        if shown.code == 0 && parsed(&shown.stdout).get("speaker") != Some(&Value::Null) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the speaker was never adopted: exit {} {}{}",
            shown.code,
            shown.stdout,
            shown.stderr
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let named = ok(&server, &["endpoints", "name", &id, "Den left"]);
    assert!(named.contains("\nname: Den left\n"), "{}", named);
    let placed = ok(&server, &["endpoints", "room", &id, "den"]);
    assert!(placed.contains("\nroom: den\n"), "{}", placed);
    let listed = ok(&server, &["endpoints", "list"]);
    assert!(listed.starts_with("SPEAKER "), "{}", listed);
    assert!(
        listed.contains(&format!("\n{}  Den left  den ", id)),
        "{}",
        listed
    );
    let shown = parsed(&ok(&server, &["endpoints", "show", &id, "--json"]));
    let speaker = shown.get("speaker").expect("a speaker");
    assert_eq!(text(speaker, "name"), "Den left");
    assert_eq!(text(speaker, "room"), "den");
    assert_eq!(speaker.get("present").and_then(Value::as_bool), Some(true));
    let unplaced = ok(&server, &["endpoints", "room", &id, "--none"]);
    assert!(unplaced.contains("\nroom: -\n"), "{}", unplaced);
    let nobody = refused(&server, &["endpoints", "forget", "chorus-ffffffffffff"]);
    assert!(nobody.contains("chorus-ffffffffffff"), "{}", nobody);
    drop(player);

    // updates: rescan and list against an empty firmware directory, status,
    // and install and cancel, which go to the server and are refused by it
    // (there is no such image and no install to cancel).
    assert_eq!(ok(&server, &["updates", "rescan"]), "no staged images\n");
    assert_eq!(ok(&server, &["updates", "list", "--json"]), "[]\n");
    ok(&server, &["updates", "status"]);
    let no_image = refused(&server, &["updates", "install", &id, "brick-9-9-9"]);
    assert!(no_image.contains("brick-9-9-9"), "{}", no_image);
    let no_image = refused(
        &server,
        &["updates", "install", "--all", "brick-9-9-9", "--json"],
    );
    let error = parsed(&no_image);
    assert_eq!(text(&error, "error"), "refused");
    assert_eq!(error.get("exit").and_then(Value::as_num), Some("3"));
    assert_eq!(error.get("status").and_then(Value::as_num), Some("400"));
    refused(&server, &["updates", "cancel", &id]);

    // And the unreachable code, against the port the server's control plane
    // held, once the server is gone.
    let control = server.control.clone();
    drop(server);
    let gone = chorus_ctl::run_with(
        &[
            "--server".to_string(),
            control,
            "rooms".to_string(),
            "list".to_string(),
        ],
        None,
    );
    assert_eq!(gone.code, 2, "{}{}", gone.stdout, gone.stderr);
    let _ = std::fs::remove_dir_all(&firmware);
}
