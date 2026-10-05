//! The measurement sweep on the real binary (ADR 0000; the playback half of
//! the phone-microphone measurement, `docs/room-correction.md`).
//!
//! Every test runs the real `chorus-server` with a control plane and sends
//! `measure_sweep` over `POST /api/command`. The players are protocol v2
//! sessions opened through the Linux client's own session code
//! (`common::line_in::Recorder`), and the configured stream is a constant
//! (`SAMPLE` in every sample), so a chunk of the stream, a chunk of silence
//! and a chunk of the sweep are told apart by every byte.
//!
//! The test names are the evidence: the room being measured, which shares a
//! group with another room, receives the sweep the fitter deconvolves with
//! (`chorus_dsp::roomfit::Sweep::recommended` at the stream's rate), sample
//! for sample, between the silences the program puts around it, and no
//! other room receives any of it; the state says a sweep is playing while it
//! is; afterwards the room hears its group's stream again at the volume it
//! had; the refusals are by name; and the level is never above the room's
//! effective limit. Nothing here is timing evidence: what is graded is
//! values, orders and counts.

mod common;

use std::time::Duration;

use chorus_control::json::{self, Value};
use chorus_dsp::roomfit::Sweep;
use chorus_protocol::AudioChunk;
use common::line_in::*;
use common::{fresh_id, RunningServer};

/// The 20 ms chunks of the program at 48 kHz: the lead silence, the sweep,
/// the tail silence.
const LEAD_CHUNKS: usize = 25;
const SWEEP_CHUNKS: usize = 250;
const TAIL_CHUNKS: usize = 50;

/// The sweep as the fitter makes it, at the stream's format: 16-bit, the
/// same sample on both channels.
fn the_fitters_sweep() -> Vec<u8> {
    Sweep::recommended(RATE_HZ)
        .signal()
        .iter()
        .flat_map(|x| {
            let s = (x * 32_768.0).round().clamp(-32_768.0, 32_767.0) as i16;
            [s.to_le_bytes(), s.to_le_bytes()].concat()
        })
        .collect()
}

/// The state's `measurement`: its number, room, state, volume in
/// thousandths and reason.
fn measurement(state: &str) -> Option<(i64, String, String, u32, Option<String>)> {
    let value = json::parse(state).unwrap_or_else(|e| panic!("{}: {:?}", state, e));
    let m = value.get("measurement")?;
    let text = |k: &str| m.get(k).and_then(Value::as_str).map(str::to_string);
    let num = |k: &str| {
        m.get(k)
            .and_then(Value::as_num)
            .unwrap()
            .parse::<f64>()
            .unwrap()
    };
    Some((
        num("id") as i64,
        text("zone").unwrap(),
        text("state").unwrap(),
        (num("volume") * 1000.0).round() as u32,
        text("reason"),
    ))
}

fn attach(server: &RunningServer, zone: &str, endpoint: &str) {
    server.applied(&format!(
        r#"{{"v":1,"t":"attach","zone":"{}","endpoint":"{}"}}"#,
        zone, endpoint
    ));
}

/// A command that must be refused: the field and the detail of its `error`.
fn refused(server: &RunningServer, body: &str) -> (String, String) {
    let (status, answer) = server.command(body);
    assert!(!status.contains("200"), "{} was applied: {}", body, answer);
    let value = json::parse(&answer).unwrap_or_else(|e| panic!("{}: {:?}", answer, e));
    assert_eq!(value.get("t").and_then(Value::as_str), Some("error"));
    let text = |k: &str| value.get(k).and_then(Value::as_str).unwrap().to_string();
    (text("field"), text("detail"))
}

fn between(chunks: &[AudioChunk], first: u32, last: u32) -> Vec<&AudioChunk> {
    chunks
        .iter()
        .filter(|c| c.sequence >= first && c.sequence <= last)
        .collect()
}

#[test]
fn the_sweep_plays_in_one_room_of_a_group_and_the_room_gets_its_stream_and_volume_back() {
    let source = constant_source("measure-sweep");
    let server = server(
        &source,
        "2026-10-05T12:00:00Z",
        "1",
        &["kitchen", "study", "hall"],
    );
    let (k, s, h) = (fresh_id("ms-k"), fresh_id("ms-s"), fresh_id("ms-h"));
    attach(&server, "kitchen", &k);
    attach(&server, "study", &s);
    attach(&server, "hall", &h);
    // The study plays with the kitchen: one group, one slot, one stream.
    server.applied(r#"{"v":2,"t":"join","zone":"study","target":"kitchen"}"#);
    server.applied(r#"{"v":1,"t":"volume","zone":"kitchen","volume":0.400}"#);
    let kitchen = Recorder::player(&server.audio, &k);
    let study = Recorder::player(&server.audio, &s);
    let hall = Recorder::player(&server.audio, &h);
    for (room, hears) in [("kitchen", &kitchen), ("study", &study), ("hall", &hall)] {
        hears.until_hearing(
            &format!("the stream in the {}", room),
            Duration::from_secs(5),
            is_stream,
        );
    }
    assert_eq!(measurement(&server.state()), None, "no sweep yet");
    let (_, _, playing_before) = room(&server.state(), "kitchen");
    assert_eq!(playing_before, "stream");

    // The command: its answer is the state, which names the sweep.
    let answer = server.applied(r#"{"v":2,"t":"measure_sweep","zone":"kitchen","volume":0.300}"#);
    assert_eq!(
        measurement(&answer),
        Some((1, "kitchen".to_string(), "playing".to_string(), 300, None))
    );
    assert!(
        answer.contains(r#""lead_ms":500,"sweep_ms":5000,"tail_ms":1000"#),
        "{}",
        answer
    );
    // While it plays the state says so, the room is at the sweep's volume,
    // and its group still plays what it played: no source changed.
    let during = server.state();
    assert_eq!(measurement(&during).unwrap().2, "playing");
    assert_eq!(room(&during, "kitchen"), (300, 1000, "stream".to_string()));
    assert_eq!(room(&during, "study").0, 1000, "the other room's volume");

    wait_for("the sweep finishes", Duration::from_secs(20), || {
        measurement(&server.state()).is_some_and(|m| m.2 == "finished")
    });
    kitchen.until_hearing("the stream again", Duration::from_secs(5), is_stream);
    // The room is back: its earlier source and its earlier volume.
    let after = server.state();
    assert_eq!(
        measurement(&after),
        Some((1, "kitchen".to_string(), "finished".to_string(), 300, None))
    );
    assert_eq!(room(&after, "kitchen"), (400, 1000, "stream".to_string()));
    wait_for("the restored gain is sent", Duration::from_secs(5), || {
        kitchen.room_volumes().last().is_some_and(|r| r.gain == 400)
    });
    let gains: Vec<u16> = kitchen.room_volumes().iter().map(|r| r.gain).collect();
    assert!(
        gains.contains(&300),
        "the sweep's volume was sent: {:?}",
        gains
    );
    assert!(
        study.room_volumes().iter().all(|r| r.gain == 1000),
        "the other room's volume never moved: {:?}",
        study.room_volumes()
    );

    // What the kitchen received: the stream, then the program, then the
    // stream, on one contiguous run of sequences.
    let chunks = kitchen.chunks();
    for w in chunks.windows(2) {
        assert_eq!(w[1].sequence, w[0].sequence.wrapping_add(1), "contiguous");
    }
    let first = chunks.iter().position(|c| !is_stream(c)).unwrap();
    let last = chunks.iter().rposition(|c| !is_stream(c)).unwrap();
    let program = &chunks[first..=last];
    assert!(
        program.iter().all(|c| !is_stream(c)),
        "the room hears nothing of its group while the program lasts"
    );
    // The silence before the sweep: the program's lead, after however many
    // silent chunks passed between the move and the start.
    let lead = program.iter().take_while(|c| is_silent(c)).count();
    assert!(
        (LEAD_CHUNKS..LEAD_CHUNKS + 25).contains(&lead),
        "{} silent chunks before the sweep",
        lead
    );
    // The sweep: the fitter's own samples, every one.
    let sweep = the_fitters_sweep();
    assert_eq!(sweep.len(), SWEEP_CHUNKS * FRAMES * 4);
    let heard: Vec<u8> = program[lead..lead + SWEEP_CHUNKS]
        .iter()
        .flat_map(|c| c.audio_data.clone())
        .collect();
    assert!(
        heard == sweep,
        "the room received the fitter's sweep, sample for sample"
    );
    // The silence after it, until the room is back on its group's stream.
    let tail = &program[lead + SWEEP_CHUNKS..];
    assert!(
        tail.len() >= TAIL_CHUNKS,
        "{} chunks after the sweep",
        tail.len()
    );
    assert!(tail.iter().all(is_silent), "silence after the sweep");

    // No other room received any of it: the room that shares the kitchen's
    // group and the room alone both heard the stream in every chunk of the
    // same span of the grid.
    let (from, to) = (program[0].sequence, program[program.len() - 1].sequence);
    for (name, hears) in [("study", &study), ("hall", &hall)] {
        let all = hears.chunks();
        let theirs = between(&all, from, to);
        assert_eq!(theirs.len(), program.len(), "the {} missed no chunk", name);
        assert!(
            theirs.iter().all(|c| is_stream(c)),
            "the {} heard only its stream while the kitchen was measured",
            name
        );
    }
    let _ = std::fs::remove_file(&source);
}

#[test]
fn a_sweep_is_refused_by_name_for_an_unknown_room_a_room_measuring_and_a_room_nobody_can_play_in() {
    let source = constant_source("measure-refusals");
    let server = server(
        &source,
        "2026-10-05T12:00:00Z",
        "1",
        &["kitchen", "study", "attic"],
    );
    let (k, s) = (fresh_id("mr-k"), fresh_id("mr-s"));
    attach(&server, "kitchen", &k);
    attach(&server, "study", &s);
    let kitchen = Recorder::player(&server.audio, &k);
    kitchen.until_hearing("the stream", Duration::from_secs(5), is_stream);
    let before = server.state();

    // A room the server does not have.
    let (field, detail) = refused(&server, r#"{"v":2,"t":"measure_sweep","zone":"garage"}"#);
    assert_eq!(field, "zone");
    assert!(detail.contains("there is no zone 'garage'"), "{}", detail);
    // A room the server cannot play in: no speaker attached to it.
    let (field, detail) = refused(&server, r#"{"v":2,"t":"measure_sweep","zone":"attic"}"#);
    assert_eq!(field, "zone");
    assert!(detail.starts_with("no-speaker:"), "{}", detail);
    // And one whose speaker is attached and has no session up.
    let (field, detail) = refused(&server, r#"{"v":2,"t":"measure_sweep","zone":"study"}"#);
    assert_eq!(field, "zone");
    assert!(
        detail.starts_with("no-speaker:") && detail.contains("player session"),
        "{}",
        detail
    );
    // A muted room, where the sweep would be silence.
    server.applied(r#"{"v":1,"t":"mute","zone":"kitchen","muted":true}"#);
    let (field, detail) = refused(&server, r#"{"v":2,"t":"measure_sweep","zone":"kitchen"}"#);
    assert_eq!(field, "zone");
    assert!(detail.starts_with("muted:"), "{}", detail);
    server.applied(r#"{"v":1,"t":"mute","zone":"kitchen","muted":false}"#);
    // A volume the catalog does not declare, and not a command of version 1.
    let (field, _) = refused(
        &server,
        r#"{"v":2,"t":"measure_sweep","zone":"kitchen","volume":1.500}"#,
    );
    assert_eq!(field, "volume");
    let (field, _) = refused(&server, r#"{"v":1,"t":"measure_sweep","zone":"kitchen"}"#);
    assert_eq!(field, "t");
    // Nothing of any of them was applied: no sweep, and the room as it was.
    assert_eq!(measurement(&server.state()), None);
    assert_eq!(room(&server.state(), "kitchen"), room(&before, "kitchen"));

    // A room already measuring, and any room while one is.
    server.applied(r#"{"v":2,"t":"measure_sweep","zone":"kitchen"}"#);
    let (field, detail) = refused(&server, r#"{"v":2,"t":"measure_sweep","zone":"kitchen"}"#);
    assert_eq!(field, "zone");
    assert!(
        detail.starts_with("measuring: room 'kitchen' is playing measurement sweep 1 already"),
        "{}",
        detail
    );
    let (field, detail) = refused(&server, r#"{"v":2,"t":"measure_sweep","zone":"study"}"#);
    assert_eq!(field, "zone");
    assert!(
        detail.starts_with("measuring: room 'kitchen' is playing measurement sweep 1 and"),
        "{}",
        detail
    );
    assert_eq!(measurement(&server.state()).unwrap().2, "playing");
    drop(kitchen);
    drop(server);

    // A server that serves one stream to every room (no `--slots`) has no
    // stream to play a sweep on in one room alone.
    let one_stream =
        RunningServer::start(&["--source", source.to_str().unwrap(), "--zone", "kitchen"]);
    let speaker = fresh_id("mr-one");
    attach(&one_stream, "kitchen", &speaker);
    let (field, detail) = refused(
        &one_stream,
        r#"{"v":2,"t":"measure_sweep","zone":"kitchen"}"#,
    );
    assert_eq!(field, "t");
    assert!(detail.starts_with("no-sweep-stream:"), "{}", detail);
    let (field, _) = refused(
        &one_stream,
        r#"{"v":2,"t":"measure_sweep","zone":"garage"}"#,
    );
    assert_eq!(field, "zone", "the room is looked at first");
    let _ = std::fs::remove_file(&source);
}

#[test]
fn the_sweeps_level_is_bounded_by_the_rooms_effective_limit() {
    let source = constant_source("measure-limit");
    // Monday 22:30 UTC: inside the quiet window set below.
    let server = server(&source, "2026-10-05T22:30:00Z", "1", &["kitchen"]);
    let speaker = fresh_id("ml-k");
    attach(&server, "kitchen", &speaker);
    server.applied(r#"{"v":2,"t":"limit","zone":"kitchen","limit":0.500}"#);
    server.applied(
        r#"{"v":2,"t":"quiet_hours","zone":"kitchen","windows":[{"days":["mon"],"start":"22:00","end":"07:00","limit":0.200}]}"#,
    );
    server.applied(r#"{"v":1,"t":"volume","zone":"kitchen","volume":0.150}"#);
    wait_for(
        "the quiet window is in force",
        Duration::from_secs(10),
        || room(&server.state(), "kitchen") == (150, 200, "stream".to_string()),
    );
    let kitchen = Recorder::player(&server.audio, &speaker);
    kitchen.until_hearing("the stream", Duration::from_secs(5), is_stream);
    let told_before = kitchen.room_volumes().len();

    // Asked for 0.900: the room plays the sweep at its effective limit, the
    // quiet window's 0.200, which is below its own limit of 0.500.
    let answer = server.applied(r#"{"v":2,"t":"measure_sweep","zone":"kitchen","volume":0.900}"#);
    assert_eq!(measurement(&answer).unwrap().3, 200, "{}", answer);
    assert_eq!(room(&answer, "kitchen"), (200, 200, "stream".to_string()));
    wait_for("the sweep's volume is sent", Duration::from_secs(5), || {
        kitchen.room_volumes().last().is_some_and(|r| r.gain == 200)
    });
    wait_for("the sweep finishes", Duration::from_secs(20), || {
        measurement(&server.state()).is_some_and(|m| m.2 == "finished")
    });
    // The room has its own volume back, and nothing its players were told
    // from the command on was above the limit.
    assert_eq!(
        room(&server.state(), "kitchen"),
        (150, 200, "stream".to_string())
    );
    wait_for("the restored gain is sent", Duration::from_secs(5), || {
        kitchen.room_volumes().last().is_some_and(|r| r.gain == 150)
    });
    let told = kitchen.room_volumes();
    assert!(told.len() > told_before);
    for r in &told {
        assert!(
            r.limit == 200 && r.gain <= 200,
            "never above the effective limit: {:?}",
            told
        );
    }
    let _ = std::fs::remove_file(&source);
}
