//! Stream slots on the real binary (goal 11): every group's stream cut on one
//! grid by one process, each session routed inside its own session on the one
//! audio port, and a change that needs one slot more refused by name.
//!
//! The server runs `--slots 3` with four rooms, so the fourth room's group
//! starts with its source set to `none` (it says so); the endpoints are
//! protocol v2 sessions opened through the Linux client's own session code
//! (`common::Player`), each with an endpoint id a room names. What is graded
//! is content and sequence numbers, never timing.

mod common;

use std::time::Duration;

use chorus_protocol::AudioChunk;
use common::{Player, RunningServer};

/// A sample value nothing else produces, so "the stream" and "silence" are
/// told apart by every byte.
const SAMPLE: i16 = 0x1234;

fn constant_source(name: &str) -> std::path::PathBuf {
    let path =
        std::env::temp_dir().join(format!("chorus-slots-{}-{}.pcm", name, std::process::id()));
    // Ten seconds of stereo pcm_s16le, every sample the same.
    let bytes: Vec<u8> = std::iter::repeat_n(SAMPLE.to_le_bytes(), 48_000 * 2 * 10)
        .flatten()
        .collect();
    std::fs::write(&path, bytes).unwrap();
    path
}

fn plays_the_stream(c: &AudioChunk) -> bool {
    c.audio_data
        .chunks(2)
        .all(|s| i16::from_le_bytes([s[0], s[1]]) == SAMPLE)
}

fn silent(c: &AudioChunk) -> bool {
    c.audio_data.iter().all(|b| *b == 0)
}

fn serial(state: &str) -> String {
    let at = state.find("\"serial\":").expect("a serial");
    state[at..].split(',').next().unwrap().to_string()
}

fn chunks(player: &mut Player, n: usize) -> Vec<AudioChunk> {
    (0..n)
        .map(|_| {
            player
                .next_chunk(Duration::from_secs(5))
                .expect("a chunk arrives")
        })
        .collect()
}

#[test]
fn two_rooms_hear_their_own_groups_on_one_grid_and_a_take_moves_a_session_in_session() {
    let source = constant_source("grid");
    let mut server = RunningServer::start(&[
        "--source",
        source.to_str().unwrap(),
        "--slots",
        "3",
        "--zone",
        "a",
        "--zone",
        "b",
        "--zone",
        "c",
        "--zone",
        "d",
    ]);
    let note = server.wait_for("slots group=d source=none reason=every-slot-in-use");
    println!("{}", note);
    let assigned = server.wait_for("slots count=3 assigned=");
    assert!(assigned.contains("assigned=0=a,1=b,2=c"), "{}", assigned);

    let pa = common::fresh_id("slots-a");
    let pd = common::fresh_id("slots-d");
    server.applied(&format!(
        r#"{{"v":1,"t":"attach","zone":"a","endpoint":"{}"}}"#,
        pa
    ));
    server.applied(&format!(
        r#"{{"v":1,"t":"attach","zone":"d","endpoint":"{}"}}"#,
        pd
    ));
    let mut a = Player::connect(&server.audio, &pa, 0);
    let mut d = Player::connect(&server.audio, &pd, 0);

    // Different content, one grid: the same sequence carries the same
    // presentation timestamp in both sessions.
    let from_a = chunks(&mut a, 20);
    let from_d = chunks(&mut d, 20);
    assert!(
        from_a.iter().all(plays_the_stream),
        "room a's group plays the stream"
    );
    assert!(
        from_d.iter().all(silent),
        "room d's group has source none: silence"
    );
    let shared: Vec<(u32, u64, u64)> = from_a
        .iter()
        .filter_map(|x| {
            from_d
                .iter()
                .find(|y| y.sequence == x.sequence)
                .map(|y| (x.sequence, x.timestamp_ns, y.timestamp_ns))
        })
        .collect();
    assert!(!shared.is_empty(), "the two sessions overlap in time");
    for (sequence, ta, td) in &shared {
        assert_eq!(
            ta, td,
            "sequence {} is stamped the same in both slots",
            sequence
        );
    }

    // Take the room (K78): a saved group of a and d, taken, plays the stream
    // on a slot; d's session moves to it in-session, with no restart.
    server.applied(r#"{"v":2,"t":"group_save","group":"ad","name":"A and D","zones":["a","d"]}"#);
    let before_take = d.next_chunk(Duration::from_secs(5)).unwrap();
    server.applied(r#"{"v":2,"t":"take","target":"ad"}"#);
    let mut run = vec![before_take];
    loop {
        let c = d
            .next_chunk(Duration::from_secs(5))
            .expect("d keeps receiving");
        let done = plays_the_stream(&c);
        run.push(c);
        if done || run.len() > 200 {
            break;
        }
    }
    run.extend(chunks(&mut d, 10));
    assert!(
        run.last().map(plays_the_stream).unwrap_or(false),
        "after the take d hears the stream"
    );
    for w in run.windows(2) {
        assert_eq!(
            w[1].sequence,
            w[0].sequence.wrapping_add(1),
            "d's sequences stay contiguous across the move"
        );
        assert_eq!(w[1].timestamp_ns - w[0].timestamp_ns, 20_000_000);
    }
    let moved_at = run.iter().position(plays_the_stream).unwrap();
    println!(
        "take: d moved from silence to the stream at sequence {} with no gap ({} chunks graded)",
        run[moved_at].sequence,
        run.len()
    );
    let after = chunks(&mut a, 10);
    assert!(after.iter().all(plays_the_stream));
    for w in after.windows(2) {
        assert_eq!(w[1].sequence, w[0].sequence.wrapping_add(1));
    }

    // Groups b, c and ad now hold all three slots. Ungrouping d would give
    // it a group of its own playing the stream: a fourth slot, refused by
    // name, with nothing applied.
    let state_before = server.state();
    let (status, refusal) = server.command(r#"{"v":1,"t":"ungroup","zone":"d"}"#);
    assert!(status.contains("400"), "{} {}", status, refusal);
    assert!(
        refusal.contains("every one of this server's 3 stream slots is in use")
            && refusal.contains("--slots 4")
            && refusal.contains(r#""field":"target""#),
        "{}",
        refusal
    );
    let state_after = server.state();
    assert_eq!(
        serial(&state_after),
        serial(&state_before),
        "the state did not move"
    );
    assert_eq!(state_after, state_before);
    let still = chunks(&mut d, 5);
    assert!(
        still.iter().all(plays_the_stream),
        "d still hears its group"
    );
    println!("refused past the ceiling: {}", refusal);
    let _ = std::fs::remove_file(&source);
}
