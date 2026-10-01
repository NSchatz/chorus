//! Limits and quiet hours hold on the server and on the endpoint, for every
//! volume path (goal 11; K81, I10, brief section 4.8; ADR 0074's "What the
//! server must send").
//!
//! The real `chorus-server` (`--slots 2`, a fixed `--civil-time` so a
//! quiet-hours window can be active) serves two rooms. In the kitchen: the
//! REAL Linux client (`session::open`, `deliver_room_volume_to`,
//! `receive::handshake`, `run_session` on a modelled recording device, as
//! `v2_end_to_end.rs` runs it) and a second session that also declares the
//! controller role. In the study: one more player. Every volume path is then
//! driven, one step at a time: `volume` above the limit, `volume_step`,
//! a `take` that changes no volume (nothing is sent), `group_volume`,
//! `group_volume_step`, `limit`, `mute` and unmute, `quiet_hours` evaluated
//! at the civil time (`Zones::set_civil_time`), and the controller role's
//! `controller_command`. After each step:
//!
//! - every affected player received exactly one new `room_volume` (an
//!   unaffected one none), whose gain is the room's volume (0 when muted) and
//!   never above its limit, which is the room's effective limit in the state;
//! - the Linux client's applied gain settles at min(gain, limit), never above
//!   the limit;
//! - the controller session received `controller_state` for a change made on
//!   the page (the ADR 0067 follow-up).
//!
//! And every sample the Linux client wrote is at most the source scaled by
//! the highest limit the kitchen ever had. The kitchen's sequence is the
//! committed fixture `fixtures/volume/room-volume-sequence.hex`, which
//! `firmware/tests/test_volume.c` feeds through the C endpoint's decoder and
//! volume path: both endpoint kinds, one sequence. `CHORUS_WRITE_FIXTURES=1`
//! rewrites it from a run; otherwise the run must reproduce it byte for byte.
//!
//! Nothing here is timing evidence: the device is modelled, and what is
//! graded is values and counts.

mod common;

use std::net::{Shutdown, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chorus_audio::MonotonicTimeline;
use chorus_client_linux::config::ClientConfig;
use chorus_client_linux::control::ZoneWatch;
use chorus_client_linux::delaylog::DelayLog;
use chorus_client_linux::receive::handshake;
use chorus_client_linux::run::{header_for, run_session};
use chorus_client_linux::session::{self, EndpointIdentity};
use chorus_client_linux::sink::{PcmSink, SinkError, SinkWrite};
use chorus_client_linux::Counters;
use chorus_control::catalog::Volume;
use chorus_control::json::{self, Value};
use chorus_protocol::v2::{
    encode, roles, Command as Button, ControllerCommand, Message, RoomVolume,
};
use common::{Player, RunningServer};

const RATE_HZ: u32 = 48_000;
const SOURCE: i16 = 12_000;

/// The modelled device: drains at the nominal rate, keeps every byte.
struct RecordingSink {
    tape: Arc<Mutex<Vec<u8>>>,
    queued: f64,
    played: u64,
    last: Instant,
}

impl RecordingSink {
    fn tick(&mut self) {
        let now = Instant::now();
        let n = (now.duration_since(self.last).as_secs_f64() * f64::from(RATE_HZ)).min(self.queued);
        self.last = now;
        self.queued -= n;
        self.played += n as u64;
    }
}

impl PcmSink for RecordingSink {
    fn device(&self) -> &str {
        "modelled-recording"
    }
    fn frame_len(&self) -> usize {
        4
    }
    fn rate_hz(&self) -> u32 {
        RATE_HZ
    }
    fn write(&mut self, pcm: &[u8]) -> Result<SinkWrite, SinkError> {
        self.tick();
        self.tape.lock().unwrap().extend_from_slice(pcm);
        let frames = (pcm.len() / 4) as f64;
        while self.queued + frames > f64::from(RATE_HZ) * 0.4 {
            thread::sleep(Duration::from_millis(2));
            self.tick();
        }
        self.queued += frames;
        Ok(SinkWrite {
            frames_written: frames as u64,
            underran: false,
        })
    }
    fn delay_frames(&mut self) -> Result<i64, SinkError> {
        self.tick();
        Ok(self.queued as i64)
    }
    fn in_xrun(&mut self) -> Result<bool, SinkError> {
        Ok(false)
    }
    fn drain(&mut self) -> Result<(), SinkError> {
        self.played += self.queued as u64;
        self.queued = 0.0;
        Ok(())
    }
    fn frames_played(&mut self) -> Result<u64, SinkError> {
        self.tick();
        Ok(self.played)
    }
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/volume/room-volume-sequence.hex")
}

fn hex_bytes(text: &str) -> Vec<u8> {
    text.lines()
        .map(|l| l.split('#').next().unwrap_or(""))
        .flat_map(|l| l.split_whitespace())
        .map(|h| u8::from_str_radix(h, 16).expect("hex"))
        .collect()
}

/// One room's gain, mute and effective limit, read off a state message.
fn room(state: &str, id: &str) -> (Volume, bool, Volume) {
    let value = json::parse(state).unwrap_or_else(|e| panic!("{}: {:?}", state, e));
    let Some(Value::Arr(zones)) = value.get("zones") else {
        panic!("no zones in {}", state)
    };
    let zone = zones
        .iter()
        .find(|z| z.get("id").and_then(Value::as_str) == Some(id))
        .unwrap_or_else(|| panic!("no zone {} in {}", id, state));
    let num = |k: &str| Volume::parse(zone.get(k).and_then(Value::as_num).unwrap()).unwrap();
    let muted = zone.get("muted").and_then(Value::as_bool).unwrap();
    (num("volume"), muted, num("effective_limit"))
}

/// What a room's players must be told, from the state.
fn told(state: &str, id: &str) -> RoomVolume {
    let (volume, muted, limit) = room(state, id);
    RoomVolume {
        gain: if muted {
            0
        } else {
            volume.thousandths() as u16
        },
        limit: limit.thousandths() as u16,
        ramp_ms: 0,
    }
}

/// The Linux client's captured messages, shared with its playout thread.
type Captured = Arc<Mutex<Vec<RoomVolume>>>;

fn wait_for(what: &str, limit: Duration, done: impl Fn() -> bool) {
    let deadline = Instant::now() + limit;
    while !done() {
        assert!(
            Instant::now() < deadline,
            "{} did not happen within {:?}",
            what,
            limit
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn every_volume_path_reaches_every_affected_player_clamped_on_server_and_linux_client() {
    // Ten seconds of a constant sample, so every written sample has one
    // expected value per gain.
    let source = std::env::temp_dir().join(format!("chorus-every-path-{}.pcm", std::process::id()));
    let bytes: Vec<u8> = std::iter::repeat_n(SOURCE.to_le_bytes(), RATE_HZ as usize * 2 * 10)
        .flatten()
        .collect();
    std::fs::write(&source, bytes).unwrap();
    let server = RunningServer::start(&[
        "--source",
        source.to_str().unwrap(),
        "--slots",
        "2",
        "--max-clients",
        "4",
        "--civil-time",
        "mon-23:30",
        "--zone",
        "kitchen",
        "--zone",
        "study",
    ]);
    let linux = common::fresh_id("every-path-linux");
    let button = common::fresh_id("every-path-button");
    let study = common::fresh_id("every-path-study");
    for (zone, endpoint) in [("kitchen", &linux), ("kitchen", &button), ("study", &study)] {
        server.applied(&format!(
            r#"{{"v":1,"t":"attach","zone":"{}","endpoint":"{}"}}"#,
            zone, endpoint
        ));
    }
    // The kitchen starts limited, below full scale, at half volume.
    server.applied(r#"{"v":2,"t":"limit","zone":"kitchen","limit":0.800}"#);
    server.applied(r#"{"v":1,"t":"volume","zone":"kitchen","volume":0.500}"#);
    let state = server.applied(r#"{"v":1,"t":"volume","zone":"study","volume":0.400}"#);
    let highest_kitchen_limit = room(&state, "kitchen").2;

    // The real Linux client in the kitchen.
    let captured: Captured = Arc::new(Mutex::new(Vec::new()));
    let watch = Arc::new(ZoneWatch::new());
    let tape = Arc::new(Mutex::new(Vec::new()));
    let stream = TcpStream::connect(&server.audio).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    let hang_up = stream.try_clone().unwrap();
    let log = std::env::temp_dir().join(format!("chorus-every-path-{}.log", std::process::id()));
    let config = ClientConfig {
        delay_log: log.to_str().unwrap().to_string(),
        ..ClientConfig::default()
    };
    let mut me = EndpointIdentity::ephemeral(&linux).unwrap();
    let mut secure = session::open(stream, &mut me, &config).expect("the session opens");
    session::deliver_room_volume_to(&secure.announced, watch.room_inbox());
    {
        let captured = Arc::clone(&captured);
        session::also_hand(
            &mut secure.reader,
            &secure.announced,
            Box::new(move |m| {
                if let Message::RoomVolume(r) = m {
                    captured.lock().unwrap().push(*r);
                }
            }),
        );
    }
    let player = {
        let watch = Arc::clone(&watch);
        let tape = Arc::clone(&tape);
        let mut reader = secure.reader;
        let config = config.clone();
        thread::spawn(move || {
            let hand = handshake(&mut reader, &|| true).expect("the stream starts");
            let mut sink = RecordingSink {
                tape,
                queued: 0.0,
                played: 0,
                last: Instant::now(),
            };
            let header = header_for(&config, "modelled-recording", &hand.shape);
            let mut log = DelayLog::open(&config.delay_log, &header).expect("the log opens");
            run_session(
                &config,
                reader,
                hand,
                &mut sink,
                &mut log,
                MonotonicTimeline::new(),
                Arc::new(Counters::new()),
                None,
                watch,
            )
            .expect("the run writes its log")
        })
    };
    // The kitchen's controller and the study's player.
    let mut controller = Player::connect(&server.audio, &button, roles::CONTROLLER);
    let mut in_study = Player::connect(&server.audio, &study, 0);
    controller.until("the controller's greeting", Duration::from_secs(5), |p| {
        !p.room_volumes().is_empty()
    });
    in_study.until("the study's greeting", Duration::from_secs(5), |p| {
        !p.room_volumes().is_empty()
    });
    wait_for(
        "the Linux client's greeting",
        Duration::from_secs(5),
        || !captured.lock().unwrap().is_empty() && watch.room().messages() >= 1,
    );
    assert_eq!(
        captured.lock().unwrap()[0],
        told(&state, "kitchen"),
        "the greeting"
    );
    assert_eq!(in_study.room_volumes()[0], told(&state, "study"));

    // Every path, one at a time: (what, the command, does it change the
    // kitchen, does it change the study).
    let steps: Vec<(&str, String, bool, bool)> = vec![
        ("volume above the limit", r#"{"v":1,"t":"volume","zone":"kitchen","volume":0.950}"#.into(), true, false),
        ("volume_step", r#"{"v":2,"t":"volume_step","zone":"kitchen","step":-100}"#.into(), true, false),
        ("group_save", r#"{"v":2,"t":"group_save","group":"downstairs","name":"Downstairs","zones":["kitchen","study"]}"#.into(), false, false),
        ("take, which changes no volume", r#"{"v":2,"t":"take","target":"downstairs"}"#.into(), false, false),
        ("group_volume", r#"{"v":2,"t":"group_volume","group":"downstairs","volume":0.800}"#.into(), true, true),
        ("group_volume_step", r#"{"v":2,"t":"group_volume_step","group":"downstairs","step":-200}"#.into(), true, true),
        ("limit below the volume", r#"{"v":2,"t":"limit","zone":"kitchen","limit":0.300}"#.into(), true, false),
        ("mute", r#"{"v":1,"t":"mute","zone":"kitchen","muted":true}"#.into(), true, false),
        ("unmute", r#"{"v":1,"t":"mute","zone":"kitchen","muted":false}"#.into(), true, false),
        ("quiet hours, active at the civil time", r#"{"v":2,"t":"quiet_hours","zone":"kitchen","windows":[{"days":["mon"],"start":"22:00","end":"07:00","limit":0.200}]}"#.into(), true, false),
    ];
    let mut fixture = String::from(
        "# Every room_volume the real chorus-server sent the kitchen's player while\n\
         # crates/server/tests/limits_hold_for_every_volume_path.rs drove every volume path, one\n\
         # frame (type 0x38, 6-byte payload: gain, limit, ramp_ms) per line. Read by that test and\n\
         # by firmware/tests/test_volume.c, which feeds it through the C endpoint's volume path.\n",
    );
    let mut sequence = captured.lock().unwrap().clone();
    fixture.push_str(&format!("# the greeting\n{}\n", hex_line(&sequence[0])));
    let mut lines = Vec::new();
    for (what, command, kitchen, studied) in &steps {
        let k0 = captured.lock().unwrap().len();
        let c0 = controller.room_volumes().len();
        let s0 = in_study.room_volumes().len();
        let states0 = controller_states(&controller);
        let state_now = server.applied(command);
        let want_k = told(&state_now, "kitchen");
        let want_s = told(&state_now, "study");
        if *kitchen {
            wait_for(what, Duration::from_secs(5), || {
                captured.lock().unwrap().len() > k0
            });
            controller.until(what, Duration::from_secs(5), |p| {
                p.room_volumes().len() > c0
            });
        }
        if *studied {
            in_study.until(what, Duration::from_secs(5), |p| {
                p.room_volumes().len() > s0
            });
        }
        // Anything else would have arrived by now: the conductor is woken by
        // the commit the command's answer came after.
        let _ = controller.next_chunk(Duration::from_millis(150));
        let _ = in_study.next_chunk(Duration::from_millis(50));
        let k = captured.lock().unwrap().clone();
        let expect_k = usize::from(*kitchen);
        assert_eq!(
            k.len() - k0,
            expect_k,
            "{}: room_volumes to the Linux client",
            what
        );
        assert_eq!(
            controller.room_volumes().len() - c0,
            expect_k,
            "{}: to the kitchen's other player",
            what
        );
        assert_eq!(
            in_study.room_volumes().len() - s0,
            usize::from(*studied),
            "{}: to the study",
            what
        );
        if *kitchen {
            let got = *k.last().unwrap();
            assert_eq!(got, want_k, "{}: the kitchen is told the state", what);
            assert!(got.gain <= got.limit, "{}: gain above the limit", what);
            assert_eq!(*controller.room_volumes().last().unwrap(), got);
            lines.push(format!("# {}\n{}\n", what, hex_line(&got)));
            sequence.push(got);
            // The Linux client's applied gain settles at min(gain, limit).
            let messages = k.len() as u64;
            wait_for(what, Duration::from_secs(5), || {
                watch.room().messages() >= messages
            });
            let applied = watch.room().settled(watch.gain());
            assert_eq!(
                applied.thousandths(),
                u32::from(got.gain.min(got.limit)),
                "{}: the Linux client's applied gain",
                what
            );
            assert!(
                applied <= room(&state_now, "kitchen").2,
                "{}: above the effective limit",
                what
            );
        }
        if *studied {
            let got = *in_study.room_volumes().last().unwrap();
            assert_eq!(got, want_s, "{}: the study is told the state", what);
            assert!(got.gain <= got.limit);
        }
        // A change made on the page reaches the kitchen's controller as
        // controller_state when it changes what a controller shows.
        if *kitchen && *what != "quiet hours, active at the civil time" {
            controller.until(what, Duration::from_secs(5), |p| {
                controller_states(p) > states0
            });
        }
    }

    // The controller role's own path: a button turning the kitchen down.
    let k0 = captured.lock().unwrap().len();
    let before = told(&server.state(), "kitchen");
    controller
        .session
        .writer
        .send(&Message::ControllerCommand(ControllerCommand {
            command: Button::VolumeStep,
            value: -5,
            target: String::new(),
        }))
        .unwrap();
    wait_for("controller_command", Duration::from_secs(5), || {
        captured.lock().unwrap().len() > k0
    });
    let state_now = server.state();
    let got = *captured.lock().unwrap().last().unwrap();
    assert_eq!(got, told(&state_now, "kitchen"), "controller_command");
    assert!(
        got.gain < before.gain && got.gain <= got.limit,
        "{:?} after {:?}",
        got,
        before
    );
    lines.push(format!(
        "# controller_command volume_step -5 points\n{}\n",
        hex_line(&got)
    ));
    sequence.push(got);
    let messages = sequence.len() as u64;
    wait_for("controller_command", Duration::from_secs(5), || {
        watch.room().messages() >= messages
    });
    assert_eq!(
        watch.room().settled(watch.gain()).thousandths(),
        u32::from(got.gain.min(got.limit))
    );

    // Every message every player received held gain <= limit.
    for (who, list) in [
        ("linux", captured.lock().unwrap().clone()),
        ("controller", controller.room_volumes()),
        ("study", in_study.room_volumes()),
    ] {
        assert!(
            list.iter().all(|r| r.gain <= r.limit && r.ramp_ms == 0),
            "{}: {:?}",
            who,
            list
        );
    }
    assert_eq!(
        *captured.lock().unwrap(),
        sequence,
        "nothing was sent the kitchen that this test did not ask for"
    );

    // The Linux client's written samples: never above the source scaled by
    // the highest limit the kitchen had.
    // At least half a second of audio before the hang-up: the steps above
    // take wall time that depends on the host and the build (with the
    // optimised crypto of the tv-path ADR 0094 they finish in about 1.2 s),
    // and the count below is graded on what was played, not on how long the
    // steps took.
    let deadline = Instant::now() + Duration::from_secs(10);
    while tape.lock().unwrap().len() <= 2 * 48_000 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    let _ = hang_up.shutdown(Shutdown::Both);
    let _ = player.join();
    let written = tape.lock().unwrap().clone();
    let ceiling = i64::from(SOURCE) * i64::from(highest_kitchen_limit.thousandths()) / 1000;
    let samples: Vec<i16> = written
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| i16::from_le_bytes(*c))
        .collect();
    assert!(
        samples.len() > 48_000,
        "the client played ({} samples)",
        samples.len()
    );
    assert!(
        samples.iter().all(|s| i64::from(*s).abs() <= ceiling),
        "a sample above {} was written",
        ceiling
    );

    // The committed fixture is this run's kitchen sequence.
    for line in lines {
        fixture.push_str(&line);
    }
    if std::env::var("CHORUS_WRITE_FIXTURES").as_deref() == Ok("1") {
        std::fs::create_dir_all(fixture_path().parent().unwrap()).unwrap();
        std::fs::write(fixture_path(), &fixture).unwrap();
    }
    let committed = std::fs::read_to_string(fixture_path()).expect("the committed fixture");
    let frames: Vec<u8> = sequence
        .iter()
        .flat_map(|r| encode(&Message::RoomVolume(*r)).unwrap())
        .collect();
    assert_eq!(
        hex_bytes(&committed),
        frames,
        "fixtures/volume/room-volume-sequence.hex is this run's sequence"
    );
    println!(
        "every volume path: {} room_volume frames to the kitchen (each gain <= limit, Linux client \
         applied min(gain, limit) at each), {} to the study, {} samples written, none above {}",
        sequence.len(),
        in_study.room_volumes().len(),
        samples.len(),
        ceiling
    );
    let _ = std::fs::remove_file(&source);
    let _ = std::fs::remove_file(&log);
}

fn controller_states(p: &Player) -> usize {
    p.messages
        .lock()
        .unwrap()
        .iter()
        .filter(|m| matches!(m, Message::ControllerState(_)))
        .count()
}

fn hex_line(r: &RoomVolume) -> String {
    encode(&Message::RoomVolume(*r))
        .unwrap()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<Vec<_>>()
        .join(" ")
}

/// A session that is up in the one-stream shape (`--slots 0`) is told its
/// room's `room_volume` before its first chunk, as in the slot shape.
#[test]
fn in_the_one_stream_shape_the_greeting_carries_room_volume_before_the_first_chunk() {
    let server = RunningServer::start(&["--source", "tone", "--serve-forever", "--zone", "den"]);
    let id = common::fresh_id("one-stream");
    server.applied(&format!(
        r#"{{"v":1,"t":"attach","zone":"den","endpoint":"{}"}}"#,
        id
    ));
    let state = server.applied(r#"{"v":2,"t":"limit","zone":"den","limit":0.600}"#);
    let mut p = Player::connect(&server.audio, &id, 0);
    let first = p.next_chunk(Duration::from_secs(5)).expect("a chunk");
    assert_eq!(
        p.room_volumes(),
        vec![told(&state, "den")],
        "before chunk {}",
        first.sequence
    );
    let at = p
        .messages
        .lock()
        .unwrap()
        .iter()
        .position(|m| matches!(m, Message::RoomVolume(_)))
        .unwrap();
    assert!(
        matches!(p.messages.lock().unwrap()[at - 1], Message::OutputDelay(_)),
        "room_volume follows the offer's greeting: {:?}",
        p.messages.lock().unwrap()
    );
}
