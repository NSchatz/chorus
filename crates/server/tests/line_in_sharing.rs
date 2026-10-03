//! Line-in sharing to any group, and a streamer on a line-in as an input,
//! on the real binary (goal 17; ADR 0066's follow-up, K94).
//!
//! Every test runs the real `chorus-server` with stream slots and a control
//! plane; the players and the scripted line-in endpoint are
//! `common::line_in`'s, as `alarms_sleep_autoplay.rs` uses them: a source
//! endpoint that offers `line-1` and streams a pattern whose every sample
//! names the source frame it is.
//!
//! The test names are the evidence:
//!
//! - two formed groups and a saved group hear the same line-in, chunk for
//!   chunk and byte for byte on one grid, from ONE start of the input;
//! - a second group sharing an input the source's own room plays alone, and
//!   leaving it again, moves the port's one latency target (30 ms, 180 ms,
//!   30 ms) with no glitch in the first room: contiguous sequences, no
//!   inserted zero, every step one a resampler held to 500 ppm makes, and
//!   the leave does not stop the input;
//! - an alarm whose line-in another group already plays plays it too: no
//!   `input-busy` fallback;
//! - a line-in labelled as a streamer plays into its endpoint's room when
//!   its signal appears, with no autoplay rule, shows its label as what the
//!   room is playing (`via` `streamer`), is shared to a second group like
//!   any line-in, and leaves that group playing when its own hold runs out.
//!
//! Nothing here is timing evidence: what is graded is values, orders and
//! counts. Byte equality between groups holds whatever the machine does (the
//! groups cut one port's chunks); the continuity test refuses, by name, a
//! run in which its own scripted source could not keep real time
//! ([`SOURCE_LATE_MS`]) and makes it again. Wall clock: about 16 seconds for
//! the file, its tests running side by side.

mod common;

use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

use chorus_control::json::{self, Value};
use chorus_protocol::AudioChunk;
use common::line_in::*;
use common::{fresh_id, RunningServer};

fn plays_line_in(c: &AudioChunk) -> bool {
    !is_silent(c) && !is_stream(c)
}

/// The chunks two listeners both received from sequence `from` on, compared
/// byte for byte; returns how many were compared.
fn same_chunks(a: &[AudioChunk], b: &[AudioChunk], from: u32, what: &str) -> usize {
    let mut same = 0;
    for x in a.iter().filter(|c| c.sequence >= from) {
        if let Some(y) = b.iter().find(|c| c.sequence == x.sequence) {
            assert_eq!(
                x.audio_data, y.audio_data,
                "{}: chunk {} differs",
                what, x.sequence
            );
            assert_eq!(x.timestamp_ns, y.timestamp_ns, "{}: one grid", what);
            same += 1;
        }
    }
    same
}

/// The sequence from which every one of `listeners` plays the line-in.
fn all_playing_from(listeners: &[&[AudioChunk]]) -> u32 {
    listeners
        .iter()
        .map(|chunks| {
            let last_other = chunks.iter().rposition(|c| !plays_line_in(c));
            match last_other {
                Some(i) => chunks[i].sequence.wrapping_add(1),
                None => chunks.first().map_or(0, |c| c.sequence),
            }
        })
        .max()
        .unwrap_or(0)
}

/// What a room shows it is playing: its now-playing title and `via`.
fn shown(state: &str, id: &str) -> Option<(String, String)> {
    let value = json::parse(state).unwrap();
    let Some(Value::Arr(zones)) = value.get("zones") else {
        return None;
    };
    let record = zones
        .iter()
        .find(|z| z.get("id").and_then(Value::as_str) == Some(id))?
        .get("now_playing")?;
    Some((
        record.get("title")?.as_str()?.to_string(),
        record.get("via")?.as_str()?.to_string(),
    ))
}

/// Wait until the server has said `what` `times` times.
fn said_times(server: &mut RunningServer, what: &str, times: usize) {
    wait_for(
        &format!("the server says {:?} {} times", what, times),
        Duration::from_secs(10),
        || {
            server.drain();
            server.seen.iter().filter(|l| l.contains(what)).count() >= times
        },
    );
}

#[test]
fn two_formed_groups_and_a_saved_group_hear_the_same_line_in_sample_for_sample() {
    let source = constant_source("shared");
    // Four rooms, each its own group at start: four slots.
    let mut server = server_with_slots(
        &source,
        "2026-10-05T12:00:00Z",
        "1",
        &["kitchen", "study", "den", "hall"],
        "4",
    );
    let amp = fresh_id("asa-sh-amp");
    let speakers: Vec<(&str, String)> = ["study", "den", "hall"]
        .iter()
        .map(|z| (*z, fresh_id(&format!("asa-sh-{}", z))))
        .collect();
    server.applied(&format!(
        r#"{{"v":1,"t":"attach","zone":"kitchen","endpoint":"{}"}}"#,
        amp
    ));
    for (zone, endpoint) in &speakers {
        server.applied(&format!(
            r#"{{"v":1,"t":"attach","zone":"{}","endpoint":"{}"}}"#,
            zone, endpoint
        ));
    }
    server
        .applied(r#"{"v":2,"t":"group_save","group":"pair","name":"Pair","zones":["den","hall"]}"#);
    // The line-in's endpoint is the kitchen's player as well as its source.
    let line = LineIn::start(&server.audio, &amp, true);
    let rooms: Vec<Recorder> = speakers
        .iter()
        .map(|(_, endpoint)| Recorder::player(&server.audio, endpoint))
        .collect();
    wait_for("the input is offered", Duration::from_secs(5), || {
        server.state().contains(&format!("{}/line-1", amp))
    });
    // Two formed groups (each a room of its own) and a saved group.
    for target in ["kitchen", "study", "pair"] {
        server.applied(&format!(
            r#"{{"v":2,"t":"take","target":"{}","source":"line-in:{}/line-1"}}"#,
            target, amp
        ));
    }
    let wanted = format!("line-in:{}/line-1", amp);
    for zone in ["kitchen", "study", "den", "hall"] {
        assert_eq!(room(&server.state(), zone).2, wanted, "{}", zone);
    }
    server.wait_for("listeners=3");
    let all = |f: &dyn Fn(&Recorder) -> bool| f(&line.hears) && rooms.iter().all(f);
    wait_for(
        "every room plays the line-in",
        Duration::from_secs(10),
        || all(&|r| r.chunks().last().is_some_and(plays_line_in)),
    );
    thread::sleep(Duration::from_secs(3));
    let kitchen = line.hears.chunks();
    let others: Vec<Vec<AudioChunk>> = rooms.iter().map(|r| r.chunks()).collect();
    let mut listeners: Vec<&[AudioChunk]> = vec![&kitchen];
    listeners.extend(others.iter().map(|c| c.as_slice()));
    let from = all_playing_from(&listeners);
    for ((zone, _), chunks) in speakers.iter().zip(&others) {
        let same = same_chunks(&kitchen, chunks, from, zone);
        assert!(
            same >= 100,
            "{}: only {} chunks in common with the kitchen",
            zone,
            same
        );
    }
    // And what they all hear is the line-in itself: the pattern's source
    // positions, read back off the kitchen's chunks.
    let read_back = kitchen
        .iter()
        .filter(|c| c.sequence >= from && plays_line_in(c))
        .filter_map(|c| position(c, None))
        .count();
    assert!(read_back > 50, "{} positions read back", read_back);
    assert_eq!(
        line.starts.load(Ordering::SeqCst),
        1,
        "one start serves every group"
    );
    assert_eq!(line.stops.load(Ordering::SeqCst), 0);
    let _ = std::fs::remove_file(&source);
}

/// How late the scripted source's own 20 ms pacing may wake before a run
/// that grades continuity is void: past this the SOURCE dropped out of real
/// time (a machine busy with something else), and an underrun the server
/// then plays as silence is the harness's, not a glitch of the share.
const SOURCE_LATE_MS: u64 = 15;

#[test]
fn a_second_group_sharing_a_line_in_and_leaving_it_moves_the_latency_without_a_glitch() {
    // The scripted source must keep real time for a continuity claim to
    // mean anything. A run in which it could not is refused by name and
    // made again, three times at most; a run in which it could is graded
    // in full, whatever it shows.
    for attempt in 1..=3 {
        match share_and_leave() {
            Ok(()) => return,
            Err(late_ms) => println!(
                "attempt {}: REFUSED: the scripted line-in woke {} ms late (more than {} ms): \
                 this machine did not run the source in real time, so continuity was not graded",
                attempt, late_ms, SOURCE_LATE_MS
            ),
        }
    }
    panic!("three runs in a row could not keep the scripted line-in in real time");
}

/// One run of the share and the leave; `Err` with the source's lateness when
/// the source did not keep real time.
fn share_and_leave() -> Result<(), u64> {
    let source = constant_source("share-leave");
    let mut server = server(&source, "2026-10-05T12:00:00Z", "1", &["den", "kitchen"]);
    let amp = fresh_id("asa-sl-amp");
    let speaker = fresh_id("asa-sl-kitchen");
    for (zone, endpoint) in [("den", &amp), ("kitchen", &speaker)] {
        server.applied(&format!(
            r#"{{"v":1,"t":"attach","zone":"{}","endpoint":"{}"}}"#,
            zone, endpoint
        ));
    }
    let line = LineIn::start(&server.audio, &amp, true);
    let kitchen = Recorder::player(&server.audio, &speaker);
    wait_for("the input is offered", Duration::from_secs(5), || {
        server.state().contains(&format!("{}/line-1", amp))
    });
    // Alone in its own room: the local latency.
    server.applied(&format!(
        r#"{{"v":2,"t":"take","target":"den","source":"line-in:{}/line-1"}}"#,
        amp
    ));
    server.wait_for_all(&["listeners=1", "target_ms=30"]);
    wait_for("the den plays the line-in", Duration::from_secs(5), || {
        line.hears.chunks().last().is_some_and(plays_line_in)
    });
    thread::sleep(Duration::from_secs(2));
    // A second GROUP takes the same input: one target for the port, the
    // largest any listener needs.
    let shared_at = line.hears.count();
    server.applied(&format!(
        r#"{{"v":2,"t":"take","target":"kitchen","source":"line-in:{}/line-1"}}"#,
        amp
    ));
    server.wait_for_all(&["listeners=2", "target_ms=180"]);
    wait_for(
        "the kitchen plays the line-in",
        Duration::from_secs(5),
        || kitchen.chunks().last().is_some_and(plays_line_in),
    );
    thread::sleep(Duration::from_secs(7));
    // It leaves: the den is the sole listener in its own room again.
    let left_at = line.hears.count();
    server.applied(r#"{"v":2,"t":"take","target":"kitchen","source":"stream"}"#);
    said_times(&mut server, "target_ms=30", 2);
    kitchen.until_hearing("the stream again", Duration::from_secs(5), is_stream);
    thread::sleep(Duration::from_secs(3));

    let late_ms = line.late_ms.load(Ordering::SeqCst);
    if late_ms > SOURCE_LATE_MS {
        let _ = std::fs::remove_file(&source);
        return Err(late_ms);
    }
    let chunks = line.hears.chunks();
    // Contiguous sequences, no inserted zero, and every step one a resampler
    // held to 500 ppm makes, in either direction: no glitch at the share,
    // through it, at the leave or after it.
    let positions = positions_within(&chunks, "the den through the share and the leave", true);
    let offsets: Vec<(u32, f64)> = positions
        .iter()
        .map(|(seq, pos)| (*seq, f64::from(*seq) * FRAMES as f64 - pos))
        .collect();
    let (shared_seq, left_seq) = (chunks[shared_at].sequence, chunks[left_at].sequence);
    let at = |pick: &dyn Fn(u32) -> bool| -> Vec<f64> {
        offsets
            .iter()
            .filter(|(s, _)| pick(*s))
            .map(|(_, o)| *o)
            .collect()
    };
    let alone = at(&|s| s < shared_seq);
    let shared = at(&|s| s >= shared_seq && s < left_seq);
    let after = at(&|s| s >= left_seq);
    let spread = alone.iter().cloned().fold(f64::MIN, f64::max)
        - alone.iter().cloned().fold(f64::MAX, f64::min);
    assert!(
        spread <= 1.5,
        "alone, the offset holds: spread {} frames",
        spread
    );
    let grown = shared.last().unwrap() - alone.last().unwrap();
    assert!(
        (10.0..=120.0).contains(&grown),
        "the offset grew by {} frames in 7 s of sharing",
        grown
    );
    // The leave asks for the local latency again. ADR 0071's plan holds a
    // target set while a transition runs until that transition ends (the
    // growth to 180 ms takes minutes at 500 ppm), so what is graded here is
    // that the leave itself moves nothing abruptly: the offset goes on as
    // the running transition takes it, a few frames in three seconds.
    assert!(
        after.len() > 50,
        "{} positions after the leave",
        after.len()
    );
    let moved = after.last().unwrap() - shared.last().unwrap();
    assert!(
        (-1.5..=120.0).contains(&moved),
        "the offset moved by {} frames in the 3 s after the leave",
        moved
    );
    // While it shared, the second group heard the den's own chunks.
    let during: Vec<AudioChunk> = kitchen
        .chunks()
        .into_iter()
        .filter(|c| plays_line_in(c) && c.sequence < left_seq)
        .collect();
    let first = during.first().map_or(shared_seq, |c| c.sequence);
    assert!(same_chunks(&during, &chunks, first, "the kitchen while sharing") >= 100);
    assert_eq!(line.starts.load(Ordering::SeqCst), 1, "never restarted");
    assert_eq!(
        line.stops.load(Ordering::SeqCst),
        0,
        "a listener leaving does not stop the input"
    );
    println!(
        "line-in sharing on the real binary: {} den chunks contiguous, offset constant within \
         {:.1} frames alone, grown by {:.1} frames 7 s into the share and moved by {:.1} frames \
         in the 3 s after the leave, no zero sample, source at most {} ms late",
        chunks.len(),
        spread,
        grown,
        moved,
        late_ms
    );
    let _ = std::fs::remove_file(&source);
    Ok(())
}

#[test]
fn an_alarm_whose_line_in_another_group_already_plays_plays_it_too() {
    let source = constant_source("alarm-shared");
    let mut server = server(
        &source,
        "2026-10-05T06:59:30Z",
        "10",
        &["kitchen", "lounge"],
    );
    let amp = fresh_id("asa-as-amp");
    let (k, l) = (fresh_id("asa-as-kitchen"), fresh_id("asa-as-lounge"));
    for (zone, endpoint) in [("kitchen", &k), ("lounge", &l)] {
        server.applied(&format!(
            r#"{{"v":1,"t":"attach","zone":"{}","endpoint":"{}"}}"#,
            zone, endpoint
        ));
    }
    let kitchen = Recorder::player(&server.audio, &k);
    let lounge = Recorder::player(&server.audio, &l);
    let line = LineIn::start(&server.audio, &amp, true);
    wait_for("the input is offered", Duration::from_secs(5), || {
        server.state().contains(&format!("{}/line-1", amp))
    });
    // A person already plays the input in the lounge.
    server.applied(&format!(
        r#"{{"v":2,"t":"take","target":"lounge","source":"line-in:{}/line-1"}}"#,
        amp
    ));
    server.applied(&format!(
        r#"{{"v":2,"t":"alarm_set","alarm":"radio","target":"kitchen","time":"07:00","days":[],"source":"line-in:{}/line-1","volume":0.500,"ramp_s":20,"duration_min":5,"enabled":true}}"#,
        amp
    ));
    wait_for("the alarm rings", Duration::from_secs(10), || {
        ringing(&server.state(), "radio")
    });
    let wanted = format!("line-in:{}/line-1", amp);
    assert_eq!(room(&server.state(), "kitchen").2, wanted, "no fallback");
    assert_eq!(room(&server.state(), "lounge").2, wanted);
    server.wait_for("listeners=2");
    wait_for(
        "the kitchen plays the line-in",
        Duration::from_secs(5),
        || kitchen.chunks().last().is_some_and(plays_line_in),
    );
    wait_for("the ramp reaches 0.500", Duration::from_secs(10), || {
        kitchen.room_volumes().last().is_some_and(|r| r.gain == 500)
    });
    thread::sleep(Duration::from_secs(2));
    let (heard, other) = (kitchen.chunks(), lounge.chunks());
    let from = all_playing_from(&[&heard[..], &other[..]]);
    assert!(same_chunks(&heard, &other, from, "the kitchen and the lounge") >= 80);
    server.drain();
    assert!(
        !server
            .seen
            .iter()
            .any(|l| l.contains("input-busy") || l.contains("fallback=chime")),
        "the alarm played its own source"
    );
    assert_eq!(line.starts.load(Ordering::SeqCst), 1);
    // The alarm ends: the kitchen is restored, the lounge keeps the input.
    server.applied(r#"{"v":2,"t":"alarm_stop","alarm":"radio"}"#);
    kitchen.until_hearing("the stream again", Duration::from_secs(10), is_stream);
    assert_eq!(room(&server.state(), "lounge").2, wanted);
    assert!(lounge.chunks().last().is_some_and(plays_line_in));
    assert_eq!(line.stops.load(Ordering::SeqCst), 0);
    let _ = std::fs::remove_file(&source);
}

#[test]
fn a_streamer_on_a_line_in_plays_into_its_room_shows_its_label_and_is_shared() {
    let source = constant_source("streamer");
    // The hold after the signal goes is 30 schedule seconds: 3 real ones.
    let server = server(&source, "2026-10-05T12:00:00Z", "10", &["kitchen", "study"]);
    let amp = fresh_id("asa-st-amp");
    let speaker = fresh_id("asa-st-study");
    for (zone, endpoint) in [("kitchen", &amp), ("study", &speaker)] {
        server.applied(&format!(
            r#"{{"v":1,"t":"attach","zone":"{}","endpoint":"{}"}}"#,
            zone, endpoint
        ));
    }
    // The streamer is wired to the kitchen endpoint's line-in, and silent.
    let line = LineIn::start(&server.audio, &amp, false);
    let study = Recorder::player(&server.audio, &speaker);
    let labelled = server.applied(&format!(
        r#"{{"v":2,"t":"input_label","input":"{}/line-1","name":"Kitchen streamer","role":"streamer"}}"#,
        amp
    ));
    assert!(
        labelled.contains(&format!(
            r#""input_labels":[{{"input":"{}/line-1","name":"Kitchen streamer","role":"streamer"}}]"#,
            amp
        )),
        "{}",
        labelled
    );
    line.hears
        .until_hearing("the stream", Duration::from_secs(5), is_stream);
    assert_eq!(room(&server.state(), "kitchen").2, "stream");

    // Its signal appears: the kitchen plays it, with no autoplay rule, and
    // shows the label.
    line.signal.store(true, Ordering::SeqCst);
    let wanted = format!("line-in:{}/line-1", amp);
    wait_for(
        "the kitchen plays the streamer",
        Duration::from_secs(5),
        || room(&server.state(), "kitchen").2 == wanted,
    );
    assert_eq!(
        shown(&server.state(), "kitchen"),
        Some(("Kitchen streamer".to_string(), "streamer".to_string()))
    );
    assert_eq!(shown(&server.state(), "study"), None);
    wait_for("the kitchen hears it", Duration::from_secs(5), || {
        line.hears.chunks().last().is_some_and(plays_line_in)
    });

    // Shared to a second group like any line-in, label and all.
    server.applied(&format!(
        r#"{{"v":2,"t":"take","target":"study","source":"{}"}}"#,
        wanted
    ));
    assert_eq!(
        shown(&server.state(), "study"),
        Some(("Kitchen streamer".to_string(), "streamer".to_string()))
    );
    wait_for("the study hears it", Duration::from_secs(5), || {
        study.chunks().last().is_some_and(plays_line_in)
    });
    thread::sleep(Duration::from_secs(2));
    let (a, b) = (line.hears.chunks(), study.chunks());
    let from = all_playing_from(&[&a[..], &b[..]]);
    assert!(same_chunks(&a, &b, from, "the kitchen and the study") >= 80);
    assert_eq!(line.starts.load(Ordering::SeqCst), 1);

    // The signal goes: after the hold the kitchen is what it was, and the
    // study, which a person chose, keeps the input.
    line.signal.store(false, Ordering::SeqCst);
    wait_for("the kitchen is restored", Duration::from_secs(10), || {
        room(&server.state(), "kitchen").2 == "stream"
    });
    assert_eq!(shown(&server.state(), "kitchen"), None);
    assert_eq!(room(&server.state(), "study").2, wanted);
    assert_eq!(line.stops.load(Ordering::SeqCst), 0);
    line.hears
        .until_hearing("the stream again", Duration::from_secs(5), is_stream);
    let _ = std::fs::remove_file(&source);
}
