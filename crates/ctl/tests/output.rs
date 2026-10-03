//! What chorusctl prints, from the committed state vectors.
//!
//! The fake server answers `GET /api/state` with a state vector's bytes; the
//! listing a person reads is compared whole, and a `--json` output is held to
//! being the server's own JSON: a part that is one value of the state message
//! must be a substring of the vector, digit for digit.

mod support;

use support::{ctl, vector, Fake};

/// chorusctl's stdout for `args`, against a server whose state is `state`.
fn printed(state: &str, args: &[&str]) -> String {
    let body = vector(state);
    let fake = Fake::answering(&[("200 OK", &body)]);
    let outcome = ctl(&fake.address, args);
    assert_eq!(outcome.code, 0, "{:?}: {}", args, outcome.stderr);
    assert_eq!(outcome.stderr, "", "{:?}", args);
    fake.requests();
    outcome.stdout
}

/// A `--json` output that is one value cut out of the state: one line, and a
/// substring of the vector's bytes.
fn cut_out_of(state: &str, args: &[&str]) -> String {
    let mut all = args.to_vec();
    all.push("--json");
    let out = printed(state, &all);
    let line = out.strip_suffix('\n').expect("one trailing newline");
    assert!(
        !line.contains('\n'),
        "{:?} printed more than one line",
        args
    );
    assert!(
        vector(state).contains(line),
        "{:?} --json is not a part of fixtures/control/{}.json as the server wrote it:\n{}",
        args,
        state,
        line
    );
    line.to_string()
}

#[test]
fn rooms_list_and_show_from_the_rich_state() {
    assert_eq!(
        printed("v2/state-rich", &["rooms", "list"]),
        "\
ROOM     NAME         GROUP       VOLUME  MUTED  LIMIT  PRESENT
living   Living Room  downstairs  0.857   no     1.000  3/3
kitchen  kitchen      downstairs  0.343   no     0.400  0/0
study    study        live-1      1.000   no     1.000  1/1
bedroom  bedroom      live-1      0.200   no     0.200  1/1
"
    );
    let shown = printed("v2/state-rich", &["rooms", "show", "bedroom"]);
    assert!(shown.starts_with("id: bedroom\nname: bedroom\ngroup: live-1\nvolume: 0.200\n"));
    assert!(shown.contains("\ntransport: wireless\nlimit: 1.000\neffective_limit: 0.200\n"));
    assert!(shown.contains("\nendpoints: endpoint-d\n"));
}

#[test]
fn rooms_list_says_what_each_room_is_playing_when_some_room_does() {
    assert_eq!(
        printed("v2/state-playing", &["rooms", "list"]),
        "\
ROOM     NAME     GROUP       VOLUME  MUTED  LIMIT  PRESENT  PLAYING
living   living   downstairs  1.000   no     1.000  0/0      Morning Light - The Example Quartet
kitchen  kitchen  downstairs  1.000   no     1.000  0/0      Morning Light - The Example Quartet
study    study    study       1.000   no     1.000  0/0      Evening news (paused)
bedroom  bedroom  bedroom     1.000   no     1.000  0/0      -
hall     hall     hall        1.000   no     1.000  0/0      -
"
    );
    let shown = printed("v2/state-playing", &["rooms", "show", "study"]);
    assert!(
        shown.ends_with(
            "source: player:p1\nnow_playing: {\"title\":\"Evening news\",\"artist\":null,\
             \"album\":null,\"art_url\":null,\"duration_ms\":null,\"state\":\"paused\",\
             \"via\":\"upnp\"}\n"
        ),
        "{}",
        shown
    );
    // A room's JSON is still the server's own bytes, what plays included.
    let one = cut_out_of("v2/state-playing", &["rooms", "show", "living"]);
    assert!(one.contains(r#""source":"player:p0","now_playing":{"title":"Morning Light","#));
    let groups = printed("v2/state-playing", &["groups", "list"]);
    assert!(groups.contains("player:p2"), "{}", groups);
}

#[test]
fn rooms_json_is_the_servers_zones() {
    let all = cut_out_of("v2/state-rich", &["rooms", "list"]);
    assert!(all.starts_with(r#"[{"id":"living","name":"Living Room","#));
    assert!(all.ends_with("]"));
    let one = cut_out_of("v2/state-rich", &["rooms", "show", "kitchen"]);
    assert!(
        one.starts_with(r#"{"id":"kitchen","name":"kitchen","group":"downstairs","volume":0.343,"#)
    );
    // The sub's level keeps its two decimals and the volume its three: the
    // digits are the server's, never a float printed again.
    assert!(one.contains(r#""sub_level_db":0.00"#) && one.contains(r#""limit":0.400"#));
}

#[test]
fn a_v1_state_lists_too() {
    assert_eq!(
        printed("state", &["rooms", "list"]),
        "\
ROOM     NAME     GROUP       VOLUME  MUTED  LIMIT  PRESENT
kitchen  Kitchen  downstairs  0.375   no     -      1/2
study    Study    study       1.000   yes    -      0/0
"
    );
    assert_eq!(
        printed("state", &["volume", "get", "study"]),
        "study volume=1.000 muted=yes\n"
    );
}

#[test]
fn groups_list_from_the_rich_state() {
    assert_eq!(
        printed("v2/state-rich", &["groups", "list"]),
        "\
GROUP       KIND   VOLUME  SOURCE                     ROOMS
downstairs  saved  0.600   line-in:endpoint-c/line-1  living,kitchen
live-1      live   0.600   stream                     study,bedroom

SAVED       NAME        ACTIVE  ROOMS
downstairs  Downstairs  yes     living,kitchen
"
    );
    assert_eq!(
        printed("v2/state-rich", &["groups", "list", "--json"]),
        concat!(
            r#"{"groups":[{"id":"downstairs","kind":"saved","zones":["living","kitchen"],"#,
            r#""volume":0.600,"source":"line-in:endpoint-c/line-1","audio":"127.0.0.1:4011"},"#,
            r#"{"id":"live-1","kind":"live","zones":["study","bedroom"],"volume":0.600,"#,
            r#""source":"stream","audio":"127.0.0.1:4010"}],"saved_groups":[{"id":"downstairs","#,
            r#""name":"Downstairs","zones":["living","kitchen"],"active":true}]}"#,
            "\n"
        )
    );
}

#[test]
fn volume_get_of_a_room_and_of_a_group() {
    assert_eq!(
        printed("v2/state-rich", &["volume", "get", "kitchen"]),
        "kitchen volume=0.343 muted=no limit=0.400 effective_limit=0.400\n"
    );
    assert_eq!(
        printed("v2/state-rich", &["volume", "get", "--group", "downstairs"]),
        "downstairs volume=0.600 rooms=living,kitchen\n"
    );
    assert_eq!(
        cut_out_of("v2/state-rich", &["volume", "get", "--group", "live-1"]),
        r#"{"id":"live-1","kind":"live","zones":["study","bedroom"],"volume":0.600,"source":"stream","audio":"127.0.0.1:4010"}"#
    );
    assert_eq!(
        cut_out_of("v2/state-rich", &["volume", "get", "kitchen"]),
        cut_out_of("v2/state-rich", &["rooms", "show", "kitchen"])
    );
}

#[test]
fn inputs_list_from_the_rich_state() {
    assert_eq!(
        printed("v2/state-rich", &["inputs", "list"]),
        "INPUT              PLAYING IN\nendpoint-c/line-1  downstairs\n"
    );
    assert_eq!(
        cut_out_of("v2/state-rich", &["inputs", "list"]),
        r#"["endpoint-c/line-1"]"#
    );
}

#[test]
fn endpoints_list_and_show_from_the_speakers_state() {
    assert_eq!(
        printed("v2/state-speakers", &["endpoints", "list"]),
        "\
SPEAKER              NAME          ROOM     PRESENT  LINK     SOFTWARE               ROLES
chorus-0123456789ab  Kitchen left  kitchen  yes      unknown  chorus-endpoint 0.1.0  player
chorus-ba9876543210  Speaker 3210  -        no       unknown  -                      -

ENDPOINT             LINK     ROOMS
chorus-0123456789ab  unknown  kitchen

KEY CHANGED          PINNED               OFFERED
chorus-ba9876543210  8796:a5b4:c3d2:e1f0  0a1b:2c3d:4e5f:6071
"
    );
    assert_eq!(
        printed(
            "v2/state-speakers",
            &["endpoints", "show", "chorus-0123456789ab"]
        ),
        "\
id: chorus-0123456789ab
name: Kitchen left
named: yes
room: kitchen
present: yes
software: chorus-endpoint 0.1.0
link: unknown
key: 1f0e:2d3c:4b5a:6978
roles: player
"
    );
    assert_eq!(
        printed(
            "v2/state-speakers",
            &["endpoints", "show", "chorus-ba9876543210", "--json"]
        ),
        concat!(
            r#"{"endpoint":null,"speaker":{"id":"chorus-ba9876543210","name":"Speaker 3210","#,
            r#""named":false,"room":null,"present":false,"software":"","link":"unknown","#,
            r#""key":"8796:a5b4:c3d2:e1f0","roles":[]}}"#,
            "\n"
        )
    );
}

#[test]
fn endpoints_of_a_state_with_no_adopted_speaker() {
    assert_eq!(
        printed("v2/state-rich", &["endpoints", "list"]),
        "\
ENDPOINT    LINK      ROOMS
endpoint-a  wired     living
endpoint-b  wired     living
endpoint-c  wired     living
endpoint-d  wireless  bedroom
endpoint-e  unknown   study
"
    );
    assert_eq!(
        printed("v2/state-rich", &["endpoints", "show", "endpoint-d"]),
        "id: endpoint-d\nlink: wireless\nrooms: bedroom\n"
    );
    // The server leaves `speakers` and `key_changes` out when they are empty;
    // the --json shape has them always.
    let listed = printed("v2/state-rich", &["endpoints", "list", "--json"]);
    assert!(listed.starts_with(r#"{"endpoints":[{"id":"endpoint-a","link":"wired"},"#));
    assert!(listed.ends_with("],\"speakers\":[],\"key_changes\":[]}\n"));
}

#[test]
fn updates_list_and_status_from_the_firmware_state() {
    assert_eq!(
        printed("v2/state-firmware", &["updates", "list"]),
        "\
IMAGE           VERSION  BOARD            SIZE     VERDICT   REASON
brick-2-0-0     2.0.0    brick-s3-wired   1536000  verified  -
brick-tampered  2.0.0    brick-s3-wired   1536000  refused   digest-mismatch
compact-2-0-0   2.0.0    compact-s3-wifi  1536000  verified  -
brick-1-0-0     1.0.0    brick-s3-wired   1536000  verified  -
"
    );
    assert_eq!(
        printed("v2/state-firmware", &["updates", "status"]),
        "\
SPEAKER              NAME          RUNS   BOARD           SLOT  STATE        REASON         UPDATE  INSTALLING   RECEIVED
chorus-0123456789ab  Speaker 89ab  1.0.0  brick-s3-wired  0     requested    none           2.0.0   brick-2-0-0  0/1536000
chorus-ba9876543210  Speaker 3210  1.0.0  brick-s3-wired  1     rolled_back  not_confirmed  3.0.0   -            0/0
"
    );
    let images = cut_out_of("v2/state-firmware", &["updates", "list"]);
    assert!(images.starts_with(r#"[{"name":"brick-2-0-0","version":"2.0.0","#));
    let speakers = cut_out_of("v2/state-firmware", &["updates", "status"]);
    assert!(
        speakers.contains(r#""firmware":{"version":"1.0.0","board":"brick-s3-wired","slot":0,"#)
    );
}

#[test]
fn an_empty_state_says_so_and_its_json_is_empty_arrays() {
    for (args, text, json) in [
        (vec!["rooms", "list"], "no rooms\n", "[]\n"),
        (
            vec!["groups", "list"],
            "no groups\n",
            "{\"groups\":[],\"saved_groups\":[]}\n",
        ),
        (vec!["inputs", "list"], "no inputs\n", "[]\n"),
        (
            vec!["endpoints", "list"],
            "no speakers or endpoints\n",
            "{\"endpoints\":[],\"speakers\":[],\"key_changes\":[]}\n",
        ),
        (vec!["updates", "list"], "no staged images\n", "[]\n"),
        (
            vec!["updates", "status"],
            "no speaker reports its firmware\n",
            "[]\n",
        ),
    ] {
        assert_eq!(printed("v2/state-empty", &args), text, "{:?}", args);
        let mut with_json = args.clone();
        with_json.push("--json");
        assert_eq!(printed("v2/state-empty", &with_json), json, "{:?}", args);
    }
}

#[test]
fn a_mutating_verb_prints_what_it_changed_and_with_json_the_whole_answer() {
    let state = vector("v2/state-rich");
    let fake = Fake::answering(&[("200 OK", &state), ("200 OK", &state)]);
    let shown = ctl(&fake.address, &["volume", "set", "kitchen", "0.343"]);
    assert_eq!(
        (shown.code, shown.stdout.as_str()),
        (
            0,
            "kitchen volume=0.343 muted=no limit=0.400 effective_limit=0.400\n"
        )
    );
    let whole = ctl(
        &fake.address,
        &["volume", "set", "kitchen", "0.343", "--json"],
    );
    assert_eq!((whole.code, whole.stdout), (0, format!("{}\n", state)));
    fake.requests();
}

#[test]
fn a_mutating_verb_whose_subject_is_gone_from_the_answer_says_ok() {
    // `endpoints name` shows the speaker; this answer has no such speaker, as
    // the state after a concurrent `forget` would not.
    let state = vector("v2/state-empty");
    let fake = Fake::answering(&[("200 OK", &state)]);
    let outcome = ctl(
        &fake.address,
        &["endpoints", "name", "chorus-0123456789ab", "Den"],
    );
    assert_eq!((outcome.code, outcome.stdout.as_str()), (0, "ok\n"));
    fake.requests();
}
