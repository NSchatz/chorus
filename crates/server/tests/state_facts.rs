//! ADR 0194's three facts in `GET /api/state` of the real binary: the chimes
//! an alarm may name (`chimes`), a sleep timer's time left (`remaining_s`)
//! going down as the schedule runtime counts it and the entry leaving when it
//! ends, and the kind of each offered input (`input_kinds`), with a TV's
//! optical input told apart from a line-in.
//!
//! Real `chorus-server` processes on loopback with the system zone set to the
//! UTC fixture and the civil clock started at a chosen instant
//! (`--civil-time-from`); the schedule's durations run faster
//! (`--schedule-time-scale`), so a two-minute timer is 6 s here. What is
//! asserted is the order and the direction of what the state says, never how
//! long anything took.

mod common;

use std::sync::atomic::Ordering;
use std::time::Duration;

use chorus_control::json::{self, Value};
use chorus_protocol::v2::SourceKind;
use common::fresh_id;
use common::line_in::*;

fn items(state: &str, member: &str) -> Option<Vec<Value>> {
    match json::parse(state).unwrap().get(member) {
        Some(Value::Arr(items)) => Some(items.clone()),
        Some(other) => panic!("{} is {:?}", member, other),
        None => None,
    }
}

fn serial(state: &str) -> u64 {
    let state = json::parse(state).unwrap();
    state
        .get("serial")
        .unwrap()
        .as_num()
        .unwrap()
        .parse()
        .unwrap()
}

/// The bedroom's sleep entry: `minutes` and `remaining_s`; `None` when the
/// state has no such entry.
fn bedroom_timer(state: &str) -> Option<(u64, u64)> {
    let whole = |v: &Value| v.as_num().unwrap().parse::<u64>().unwrap();
    items(state, "sleep")
        .unwrap()
        .iter()
        .find(|s| s.get("target").and_then(Value::as_str) == Some("bedroom"))
        .map(|s| {
            (
                whole(s.get("minutes").unwrap()),
                whole(
                    s.get("remaining_s")
                        .expect("a server with a schedule runtime says remaining_s"),
                ),
            )
        })
}

#[test]
fn a_sleep_timers_remaining_time_goes_down_in_the_state_and_its_entry_leaves_when_it_ends() {
    let source = constant_source("facts-sleep");
    let server = server(&source, "2026-10-05T22:00:00Z", "20", &["bedroom"]);

    // The chimes an alarm may name are the schedule library's, in its order.
    let chimes: Vec<String> = items(&server.state(), "chimes")
        .expect("the state lists the chimes")
        .iter()
        .map(|c| c.as_str().unwrap().to_string())
        .collect();
    let defined: Vec<String> = chorus_schedule::chime::CHIMES
        .iter()
        .map(|c| c.name().to_string())
        .collect();
    assert_eq!(chimes, defined);

    // Two minutes of schedule: 6 s of real time at 20 times. The answer to
    // the command already says all of it is left.
    assert_eq!(bedroom_timer(&server.state()), None);
    server.applied(r#"{"v":2,"t":"sleep","target":"bedroom","minutes":2}"#);
    let mut readings: Vec<(u64, u64)> = Vec::new();
    let mut serials: Vec<u64> = Vec::new();
    wait_for(
        "the timer ends and its entry leaves",
        Duration::from_secs(30),
        || {
            let state = server.state();
            match bedroom_timer(&state) {
                Some((minutes, remaining)) => {
                    assert_eq!(minutes, 2, "minutes stays what was asked for");
                    readings.push((serial(&state), remaining));
                    serials.push(serial(&state));
                    false
                }
                None => true,
            }
        },
    );
    let left: Vec<u64> = readings.iter().map(|(_, r)| *r).collect();
    assert!(left.len() >= 3, "the countdown was read: {:?}", left);
    assert!(
        left[0] <= 120 && left[0] > 60,
        "it starts near all of it: {:?}",
        left
    );
    assert!(
        left.windows(2).all(|w| w[1] <= w[0]),
        "the time left never goes up: {:?}",
        left
    );
    let last = *left.last().unwrap();
    assert!(
        last < left[0] && last <= 60,
        "the time left goes down, into the last minute: {:?}",
        left
    );
    // A state was sent when the whole minutes left changed: the serial moved
    // between a reading above a minute and one within it.
    let above = readings.iter().find(|(_, r)| *r > 60).unwrap().0;
    let within = readings.iter().rev().find(|(_, r)| *r <= 60).unwrap().0;
    assert!(within > above, "serials {:?}", serials);
    // Gone, and it stays gone.
    assert_eq!(bedroom_timer(&server.state()), None);
    assert_eq!(items(&server.state(), "sleep").unwrap().len(), 0);
    let _ = std::fs::remove_file(&source);
}

#[test]
fn every_offered_input_says_its_kind_and_a_tv_input_is_told_apart_from_a_line_in() {
    let source = constant_source("facts-inputs");
    let server = server(&source, "2026-10-05T22:00:00Z", "20", &["den"]);
    assert_eq!(items(&server.state(), "input_kinds"), None);

    let deck = fresh_id("facts-deck");
    let hub = fresh_id("facts-hub");
    let line = LineIn::start(&server.audio, &deck, true);
    let tv = LineIn::start_as(&server.audio, &hub, true, SourceKind::Optical);
    wait_for("both inputs are offered", Duration::from_secs(10), || {
        items(&server.state(), "inputs").is_some_and(|i| i.len() == 2)
    });

    let state = server.state();
    let offered: Vec<String> = items(&state, "inputs")
        .unwrap()
        .iter()
        .map(|i| i.as_str().unwrap().to_string())
        .collect();
    let kinds: Vec<(String, String, bool)> = items(&state, "input_kinds")
        .expect("the state says the kinds")
        .iter()
        .map(|k| {
            (
                k.get("input").unwrap().as_str().unwrap().to_string(),
                k.get("kind").unwrap().as_str().unwrap().to_string(),
                k.get("tv").unwrap().as_bool().unwrap(),
            )
        })
        .collect();
    // One entry for each offered input, in the order of `inputs`.
    assert_eq!(
        kinds.iter().map(|(i, _, _)| i.clone()).collect::<Vec<_>>(),
        offered
    );
    let of = |endpoint: &str| {
        kinds
            .iter()
            .find(|(i, _, _)| *i == format!("{}/line-1", endpoint))
            .unwrap_or_else(|| panic!("{} has no kind: {:?}", endpoint, kinds))
    };
    assert_eq!((of(&deck).1.as_str(), of(&deck).2), ("line_in", false));
    assert_eq!((of(&hub).1.as_str(), of(&hub).2), ("optical", true));

    // The TV's signal goes: its entry goes with it, the line-in's stays.
    tv.signal.store(false, Ordering::SeqCst);
    wait_for("the TV input is withdrawn", Duration::from_secs(10), || {
        items(&server.state(), "inputs").is_some_and(|i| i.len() == 1)
    });
    let kinds = items(&server.state(), "input_kinds").unwrap();
    assert_eq!(kinds.len(), 1);
    assert_eq!(
        kinds[0].get("kind").and_then(Value::as_str),
        Some("line_in")
    );
    drop(line);
    drop(tv);
    let _ = std::fs::remove_file(&source);
}
