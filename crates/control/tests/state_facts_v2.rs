//! ADR 0000's three facts in the state message, each for a screen that would
//! otherwise hard-code or guess it: the built-in chimes an alarm may name
//! (`chimes`), how long a sleep timer has left (`remaining_s`), and the kind
//! of each offered input (`input_kinds`). Every one is written only when
//! there is something to say, so a model that is told none of it sends the
//! bytes it sent before. The byte-for-byte vector is
//! `fixtures/control/v2/state-facts` and runs in `tests/catalog_v2.rs`.

mod vectors;

use chorus_control::catalog::decode_command;
use chorus_control::json::{self, Value};
use chorus_control::rooms::{InputId, InputKind, Source};
use chorus_control::zones::{Zone, Zones};
use chorus_schedule::chime::{Chime, CHIMES};

use vectors::{read_json_in, v2_dir};

fn apply(zones: &mut Zones, text: &str) {
    let command = decode_command(text).unwrap_or_else(|e| panic!("{}: {}", text, e));
    zones
        .apply(&command)
        .unwrap_or_else(|e| panic!("{}: {}", text, e));
}

fn house() -> Zones {
    let mut zones = Zones::new("127.0.0.1:4010");
    for id in ["kitchen", "bedroom"] {
        zones.add(Zone::new(id)).unwrap();
    }
    zones
}

fn state(zones: &Zones) -> Value {
    json::parse(&zones.encode_state()).unwrap()
}

fn texts_of(value: &Value) -> Vec<String> {
    match value {
        Value::Arr(items) => items
            .iter()
            .map(|v| v.as_str().expect("a string").to_string())
            .collect(),
        other => panic!("not an array: {:?}", other),
    }
}

fn items_of(value: &Value) -> Vec<Value> {
    match value {
        Value::Arr(items) => items.clone(),
        other => panic!("not an array: {:?}", other),
    }
}

/// The names `crates/schedule/src/chime.rs` defines, read from it.
fn defined_chimes() -> Vec<String> {
    CHIMES.iter().map(|c| c.name().to_string()).collect()
}

// --- chimes --------------------------------------------------------------------

#[test]
fn the_state_lists_the_chime_names_the_schedule_library_defines() {
    // A model told nothing says nothing: the bytes every vector before this
    // one pinned.
    let mut zones = house();
    assert!(state(&zones).get("chimes").is_none());
    assert!(!zones.encode_state().contains("chimes"));

    // What chorus-server tells its model at start is the schedule library's
    // own list, in its order: there is no second one to keep.
    let serial = zones.serial();
    zones.set_chimes(chorus_server::schedule_runtime::chime_names());
    assert_eq!(
        zones.serial(),
        serial,
        "a fact about the build moves no serial"
    );
    let listed = texts_of(state(&zones).get("chimes").expect("the state has chimes"));
    assert_eq!(listed, defined_chimes());
    assert!(!listed.is_empty());

    // Each name is one the library renders and one an alarm may carry as
    // `chime:<name>`.
    for name in &listed {
        assert!(Chime::from_name(name).is_some(), "{} is not a chime", name);
        let alarm = format!(
            r#"{{"v":2,"t":"alarm_set","alarm":"wake","target":"kitchen","time":"07:00","days":[],"source":"chime:{}","volume":0.300,"ramp_s":30,"duration_min":0,"enabled":true}}"#,
            name
        );
        apply(&mut zones, &alarm);
        assert_eq!(zones.alarms()[0].source, Source::Chime(name.clone()));
    }

    // The v1 state never carries it.
    assert!(!zones.encode_state_at(1).contains("chimes"));
}

#[test]
fn the_state_vector_carries_exactly_the_chimes_the_schedule_library_defines() {
    let vector = json::parse(&read_json_in(&v2_dir(), "state-facts")).unwrap();
    assert_eq!(texts_of(vector.get("chimes").unwrap()), defined_chimes());
}

// --- sleep ---------------------------------------------------------------------

fn whole(value: &Value) -> i64 {
    value.as_num().expect("a number").parse().expect("whole")
}

fn sleep_entries(zones: &Zones) -> Vec<(String, i64, Option<i64>)> {
    items_of(state(zones).get("sleep").unwrap())
        .iter()
        .map(|s| {
            (
                s.get("target").unwrap().as_str().unwrap().to_string(),
                whole(s.get("minutes").unwrap()),
                s.get("remaining_s").map(whole),
            )
        })
        .collect()
}

#[test]
fn a_sleep_timer_says_how_long_it_has_left_where_a_runtime_counts_it() {
    const SLEEP: &str = r#"{"v":2,"t":"sleep","target":"bedroom","minutes":30}"#;

    // Nothing counts this model's timers: the entry is what it always was,
    // and a count handed to it is not kept.
    let mut bare = house();
    apply(&mut bare, SLEEP);
    assert_eq!(sleep_entries(&bare), [("bedroom".to_string(), 30, None)]);
    assert!(!bare.sleep_remaining("bedroom", 10));
    assert_eq!(sleep_entries(&bare), [("bedroom".to_string(), 30, None)]);

    // A runtime counts this one's: all of it is left when it is asked for.
    let mut zones = house();
    zones.set_sleep_counted(true);
    apply(&mut zones, SLEEP);
    assert_eq!(
        sleep_entries(&zones),
        [("bedroom".to_string(), 30, Some(1800))]
    );

    // The count is kept to the second, and a state goes out (the serial
    // moves) only when the whole minutes left change.
    let serial = zones.serial();
    assert!(!zones.sleep_remaining("bedroom", 1799));
    assert!(!zones.sleep_remaining("bedroom", 1741));
    assert_eq!(zones.serial(), serial);
    assert_eq!(
        sleep_entries(&zones),
        [("bedroom".to_string(), 30, Some(1741))]
    );
    assert!(zones.sleep_remaining("bedroom", 1740));
    assert_eq!(zones.serial(), serial + 1);
    assert!(!zones.sleep_remaining("bedroom", 1740));
    assert_eq!(
        sleep_entries(&zones),
        [("bedroom".to_string(), 30, Some(1740))]
    );
    // `minutes` is still what was asked for.
    assert_eq!(zones.sleep_timers()[0].minutes, 30);

    // Asking again starts over; a target with no timer has no count.
    apply(
        &mut zones,
        r#"{"v":2,"t":"sleep","target":"bedroom","minutes":5}"#,
    );
    assert_eq!(
        sleep_entries(&zones),
        [("bedroom".to_string(), 5, Some(300))]
    );
    assert!(!zones.sleep_remaining("kitchen", 60));

    // The entry leaves when the runtime says the timer ran out.
    assert!(zones.sleep_expired("bedroom"));
    assert!(sleep_entries(&zones).is_empty());
    assert!(!zones.sleep_remaining("bedroom", 1));

    // The v1 state has no sleep timers at all.
    assert!(!zones.encode_state_at(1).contains("remaining_s"));
}

// --- input kinds ---------------------------------------------------------------

fn input(text: &str) -> InputId {
    InputId::parse(text).unwrap()
}

fn kinds(zones: &Zones) -> Vec<(String, String, bool)> {
    match state(zones).get("input_kinds") {
        None => Vec::new(),
        Some(list) => items_of(list)
            .iter()
            .map(|k| {
                (
                    k.get("input").unwrap().as_str().unwrap().to_string(),
                    k.get("kind").unwrap().as_str().unwrap().to_string(),
                    k.get("tv").unwrap().as_bool().unwrap(),
                )
            })
            .collect(),
    }
}

#[test]
fn every_offered_input_says_its_kind_and_a_tv_input_is_told_apart_from_a_line_in() {
    let mut zones = house();
    // The server says the kind, then offers the input. Saying the kind of
    // an input not offered yet changes nothing a subscriber sees.
    let serial = zones.serial();
    assert!(!zones.set_input_kind(&input("hub/tv"), InputKind::Optical));
    assert_eq!(zones.serial(), serial);
    assert!(!zones.encode_state().contains("input_kinds"));
    assert!(zones.offer_input(input("hub/tv")));
    for (id, kind) in [
        ("bar/arc", InputKind::HdmiArc),
        ("deck/line-1", InputKind::LineIn),
    ] {
        zones.set_input_kind(&input(id), kind);
        zones.offer_input(input(id));
    }

    // One entry for each entry of `inputs`, in its order, each with its kind.
    let offered = texts_of(state(&zones).get("inputs").unwrap());
    assert_eq!(offered, ["bar/arc", "deck/line-1", "hub/tv"]);
    let said = kinds(&zones);
    assert_eq!(
        said.iter().map(|(i, _, _)| i.clone()).collect::<Vec<_>>(),
        offered
    );
    let row = |i: &str, k: &str, tv: bool| (i.to_string(), k.to_string(), tv);
    assert_eq!(
        said,
        [
            row("bar/arc", "hdmi_arc", true),
            row("deck/line-1", "line_in", false),
            row("hub/tv", "optical", true),
        ]
    );
    // Optical and HDMI ARC are the TV inputs; a line-in is not one.
    for kind in InputKind::ALL {
        assert_eq!(
            kind.is_tv(),
            matches!(kind, InputKind::Optical | InputKind::HdmiArc)
        );
        assert_eq!(InputKind::from_name(kind.name()), Some(kind));
    }

    // A kind that changes while the input is offered is a change of state.
    let serial = zones.serial();
    assert!(zones.set_input_kind(&input("deck/line-1"), InputKind::Optical));
    assert_eq!(zones.serial(), serial + 1);
    assert!(!zones.set_input_kind(&input("deck/line-1"), InputKind::Optical));
    assert_eq!(kinds(&zones)[1], row("deck/line-1", "optical", true));

    // An input that goes takes its entry with it, and one offered with no
    // kind said (a model no server feeds) has none: `inputs` still lists it.
    assert!(zones.withdraw_input(&input("hub/tv")));
    assert_eq!(kinds(&zones).len(), 2);
    assert_eq!(zones.input_kind(&input("hub/tv")), None);
    zones.offer_input(input("hub/tv"));
    assert_eq!(texts_of(state(&zones).get("inputs").unwrap()).len(), 3);
    assert_eq!(kinds(&zones).len(), 2);

    // The v1 state never carries it.
    assert!(!zones.encode_state_at(1).contains("input_kinds"));
}
