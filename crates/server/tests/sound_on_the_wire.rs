//! Per-room sound reaches a room's endpoints on the audio wire (goal 12,
//! done-when lines B and the wire half of C; the per-room-sound ADR).
//!
//! The real `chorus-server` (a control plane, the one-stream shape) serves two
//! rooms. Four protocol v2 player sessions, opened through the Linux client's
//! own session code (`common::Player`), are the endpoints: three in the living
//! room, one in the kitchen. Then, one step at a time:
//!
//! - every greeting carries `sound` after `room_volume`, and a room at the
//!   defaults is told exactly the committed `fixtures/protocol/v2/sound_flat`
//!   bytes;
//! - bonding the living room's three as a 2.1 set (FL, FR, LFE) tells each
//!   member its own role and that the set has a sub, and tells the kitchen
//!   nothing;
//! - `sound`, `bass_management` and `room_eq` commands each reach every
//!   member once, and after them the sub is told exactly the committed
//!   `sound_sub_2_1` bytes, which `firmware/tests/test_protocol_v2.c` decodes
//!   and keeps through the C session's store, so the server, the Rust and C
//!   decoders and the C session agree on one set of bytes;
//! - a `room_eq` outside the room-correction bounds is refused naming
//!   `filters`, and sends nothing;
//! - the Linux client's session keeps the last `sound` it received
//!   (`Announced::sound`).
//!
//! Nothing here is timing evidence: what is graded is values and counts.

mod common;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use chorus_control::json::{self, Value};
use chorus_control::{ROOM_EQ_FREQ_HZ, ROOM_EQ_GAIN_CDB, ROOM_EQ_MAX_FILTERS, ROOM_EQ_Q_MILLI};
use chorus_protocol::v2::{
    encode, Message, Sound, SOUND_EQ_FREQ_HZ, SOUND_EQ_GAIN_CDB, SOUND_EQ_MAX_FILTERS,
    SOUND_EQ_Q_MILLI,
};
use common::{Player, RunningServer};

/// A committed v2 vector's frame: whitespace separated hex, `#` comments.
fn committed(stem: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/protocol/v2")
        .join(format!("{}.hex", stem));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} is committed: {}", path.display(), e));
    text.lines()
        .map(|l| l.split('#').next().unwrap_or(""))
        .flat_map(|l| l.split_whitespace())
        .map(|b| u8::from_str_radix(b, 16).expect("a hex byte"))
        .collect()
}

fn sounds(p: &Player) -> Vec<Sound> {
    p.messages
        .lock()
        .unwrap()
        .iter()
        .filter_map(|m| match m {
            Message::Sound(s) => Some(s.clone()),
            _ => None,
        })
        .collect()
}

/// Read every player until each has `want[i]` sounds.
fn until_sounds(players: &mut [&mut Player], want: &[usize], what: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let mut done = true;
        for (p, n) in players.iter_mut().zip(want) {
            if sounds(p).len() < *n {
                done = false;
                let _ = p.next_chunk(Duration::from_millis(20));
            }
        }
        if done {
            return;
        }
        assert!(Instant::now() < deadline, "{}: not within 10 s", what);
    }
}

/// Read every player for a while, so anything sent would have arrived.
fn settle(players: &mut [&mut Player]) {
    let until = Instant::now() + Duration::from_millis(400);
    while Instant::now() < until {
        for p in players.iter_mut() {
            let _ = p.next_chunk(Duration::from_millis(10));
        }
    }
}

fn frame(s: &Sound) -> Vec<u8> {
    encode(&Message::Sound(s.clone())).expect("a valid sound")
}

#[test]
fn the_control_catalog_and_the_wire_hold_the_same_room_correction_bounds() {
    assert_eq!(ROOM_EQ_MAX_FILTERS, SOUND_EQ_MAX_FILTERS);
    assert_eq!(ROOM_EQ_FREQ_HZ, SOUND_EQ_FREQ_HZ);
    assert_eq!(ROOM_EQ_GAIN_CDB, SOUND_EQ_GAIN_CDB);
    assert_eq!(ROOM_EQ_Q_MILLI, SOUND_EQ_Q_MILLI);
}

#[test]
fn a_rooms_sound_reaches_its_endpoints_with_their_roles_and_a_bad_room_eq_is_refused() {
    let server = RunningServer::start(&[
        "--source",
        "tone",
        "--serve-forever",
        "--max-clients",
        "4",
        "--zone",
        "living",
        "--zone",
        "kitchen",
    ]);
    let fl = common::fresh_id("sound-fl");
    let fr = common::fresh_id("sound-fr");
    let sub = common::fresh_id("sound-sub");
    let other = common::fresh_id("sound-kitchen");
    for (zone, endpoint) in [
        ("living", &fl),
        ("living", &fr),
        ("living", &sub),
        ("kitchen", &other),
    ] {
        server.applied(&format!(
            r#"{{"v":2,"t":"attach","zone":"{}","endpoint":"{}","link":"wired"}}"#,
            zone, endpoint
        ));
    }
    let mut p_fl = Player::connect(&server.audio, &fl, 0);
    let mut p_fr = Player::connect(&server.audio, &fr, 0);
    let mut p_sub = Player::connect(&server.audio, &sub, 0);
    let mut p_kitchen = Player::connect(&server.audio, &other, 0);

    // The greeting: room_volume, then sound, before any audio; at the
    // defaults that is the committed sound_flat, byte for byte.
    until_sounds(
        &mut [&mut p_fl, &mut p_fr, &mut p_sub, &mut p_kitchen],
        &[1, 1, 1, 1],
        "the greeting's sound",
    );
    for p in [&p_fl, &p_fr, &p_sub, &p_kitchen] {
        let messages = p.messages.lock().unwrap().clone();
        let rv = messages
            .iter()
            .position(|m| matches!(m, Message::RoomVolume(_)))
            .expect("room_volume in the greeting");
        let sd = messages
            .iter()
            .position(|m| matches!(m, Message::Sound(_)))
            .expect("sound in the greeting");
        assert!(rv < sd, "room_volume, then sound: {:?}", messages);
        assert_eq!(frame(&sounds(p)[0]), committed("sound_flat"));
    }

    // A 2.1 set: each member is told its own role and that there is a sub.
    server.applied(&format!(
        r#"{{"v":2,"t":"bond","zone":"living","members":[{{"endpoint":"{}","role":"FL"}},{{"endpoint":"{}","role":"FR"}},{{"endpoint":"{}","role":"LFE"}}]}}"#,
        fl, fr, sub
    ));
    until_sounds(
        &mut [&mut p_fl, &mut p_fr, &mut p_sub],
        &[2, 2, 2],
        "the bond",
    );
    for (p, role) in [(&p_fl, 1u8), (&p_fr, 2), (&p_sub, 4)] {
        let last = sounds(p).last().cloned().unwrap();
        assert_eq!((last.role, last.sub_present), (role, true), "{:?}", last);
    }

    // Tone, bass management and room EQ, one command each: every member is
    // told once per command, the kitchen never.
    for (n, command) in [
        r#"{"v":2,"t":"sound","zone":"living","bass":3,"treble":-2}"#,
        r#"{"v":2,"t":"bass_management","zone":"living","crossover_hz":100,"sub_level_db":-3.50,"sub_polarity":"inverted"}"#,
        r#"{"v":2,"t":"room_eq","zone":"living","filters":[{"freq_hz":42,"gain_db":-6.00,"q":4.500}]}"#,
    ]
    .iter()
    .enumerate()
    {
        server.applied(command);
        let want = 3 + n;
        until_sounds(
            &mut [&mut p_fl, &mut p_fr, &mut p_sub],
            &[want, want, want],
            command,
        );
    }
    settle(&mut [&mut p_fl, &mut p_fr, &mut p_sub, &mut p_kitchen]);
    assert_eq!(sounds(&p_fl).len(), 5, "one per change, no repeats");
    assert_eq!(sounds(&p_sub).len(), 5);
    assert_eq!(
        sounds(&p_kitchen).len(),
        1,
        "another room's change is not the kitchen's"
    );
    // The sub's last sound is the committed 2.1 vector, byte for byte.
    assert_eq!(
        frame(sounds(&p_sub).last().unwrap()),
        committed("sound_sub_2_1")
    );
    let fl_last = sounds(&p_fl).last().cloned().unwrap();
    assert_eq!(fl_last.role, 1);
    // Goal 13: a front member of a set with no centre and no surrounds is
    // also told to fold them (sound_fold, ITU-R BS.775-4); the sub is not.
    assert_eq!(fl_last.fold, 3);
    assert_eq!(
        Sound {
            role: 4,
            fold: 0,
            ..fl_last
        },
        sounds(&p_sub).last().cloned().unwrap(),
        "the members differ only in their role and the front member's fold"
    );

    // A room_eq outside the room-correction bounds: refused by field, and
    // nothing is sent.
    let (status, answer) = server.command(
        r#"{"v":2,"t":"room_eq","zone":"living","filters":[{"freq_hz":120,"gain_db":6.00,"q":2.000}]}"#,
    );
    assert!(!status.contains("200"), "{} {}", status, answer);
    let refusal = json::parse(&answer).expect("a refusal is JSON");
    assert_eq!(
        refusal.get("field").and_then(Value::as_str),
        Some("filters"),
        "{}",
        answer
    );
    let (status, answer) = server.command(r#"{"v":2,"t":"sound","zone":"living","bass":11}"#);
    assert!(
        !status.contains("200") && answer.contains(r#""field":"bass""#),
        "{}",
        answer
    );
    settle(&mut [&mut p_fl, &mut p_fr, &mut p_sub, &mut p_kitchen]);
    assert_eq!(sounds(&p_sub).len(), 5, "a refusal sends nothing");

    // Unbonding: every former member is told it is in no set.
    server.applied(r#"{"v":2,"t":"unbond","zone":"living"}"#);
    until_sounds(
        &mut [&mut p_fl, &mut p_fr, &mut p_sub],
        &[6, 6, 6],
        "the unbond",
    );
    for p in [&p_fl, &p_fr, &p_sub] {
        let last = sounds(p).last().cloned().unwrap();
        assert_eq!((last.role, last.sub_present), (0, false));
    }

    // The Linux client's session keeps the last one it received.
    for p in [&p_fl, &p_sub, &p_kitchen] {
        let announced = p.session.announced.lock().unwrap().clone();
        assert_eq!(announced.sound.as_ref(), sounds(p).last());
        assert_eq!(announced.sounds as usize, sounds(p).len());
    }
}
