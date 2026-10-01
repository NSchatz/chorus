//! The hub as the TV's Audio System, end to end on the real server (goal
//! 13; `docs/cec.md`, `crates/cec`, `crates/client-linux/src/cec.rs`).
//!
//! The real `chorus-server` (stream slots, the schedule runtime, a control
//! plane) serves the den. The hub is the real Linux client's code: its
//! protocol v2 session (`session::open`, declaring player, controller and
//! source), its CEC role (`CecRole`, the `chorus-cec` driver and Audio
//! System) on a fake CEC bus, and its source role (`source::spawn`) on a
//! modelled HDMI ARC input that captures digital silence in real time, so
//! anything the TV input offers comes from CEC alone. A scripted Roku-like TV
//! sits on the same bus.
//!
//! What is checked, on the server and on the bus:
//!
//! - the TV turns System Audio Mode on and the hub broadcasts it;
//! - the TV's Volume Up keys change the den's volume on the server, one
//!   `volume_step` each, and stop at the den's limit (K81, I10: the server
//!   clamps, the hub only asks);
//! - Give Audio Status is answered with the den's volume as the server holds
//!   it, and the change the keys caused was pushed to the TV unprompted;
//! - the TV turning on raises the TV input's signal with no audio at all
//!   (autoplay on power), the den's autoplay rule plays the hub's input,
//!   the TV's keys while it plays turn the den down and mute it without
//!   detaching it from the autoplay (ADR 0094), and the TV's standby ends
//!   the signal at once and the den is restored, volume and mute too.
//!
//! Nothing here is timing evidence: the bus is in memory and what is graded
//! is values, orders and counts.

mod common;

use std::io::Read;
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chorus_cec::codec::{build, opcode, ui, PhysicalAddress, AUDIO_SYSTEM, BROADCAST, TV};
use chorus_cec::{FakeBus, FakeTv, TvKind, TvSignal};
use chorus_client_linux::cec::{CecConfig, CecRole, VOLUME_STEP};
use chorus_client_linux::config::{ClientConfig, LineInConfig};
use chorus_client_linux::session::{self, EndpointIdentity};
use chorus_client_linux::source::{
    self, CaptureError, CaptureRead, CaptureSource, SharedWriter, SignalThresholds, SourceSetup,
    Upstream,
};
use chorus_client_linux::Counters;
use chorus_control::json::{self, Value};
use chorus_protocol::v2::{roles, Codec, SourceKind};
use common::{fresh_id, RunningServer};

const RATE_HZ: u32 = 48_000;

fn constant_source() -> PathBuf {
    let path = std::env::temp_dir().join(format!("chorus-cec-tv-{}.pcm", std::process::id()));
    let bytes: Vec<u8> = std::iter::repeat_n(0x1234i16.to_le_bytes(), RATE_HZ as usize * 2 * 40)
        .flatten()
        .collect();
    std::fs::write(&path, bytes).unwrap();
    path
}

/// The den's volume and effective limit (thousandths) and its group's
/// source, off a state message.
fn den(state: &str) -> (u32, u32, String) {
    let value = json::parse(state).unwrap_or_else(|e| panic!("{}: {:?}", state, e));
    let Some(Value::Arr(zones)) = value.get("zones") else {
        panic!("no zones in {}", state)
    };
    let zone = zones
        .iter()
        .find(|z| z.get("id").and_then(Value::as_str) == Some("den"))
        .unwrap();
    let th = |k: &str| -> u32 {
        let text = zone.get(k).and_then(Value::as_num).unwrap();
        (text.parse::<f64>().unwrap() * 1000.0).round() as u32
    };
    let group = zone
        .get("group")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();
    let Some(Value::Arr(groups)) = value.get("groups") else {
        panic!("no groups in {}", state)
    };
    let source = groups
        .iter()
        .find(|g| g.get("id").and_then(Value::as_str) == Some(group.as_str()))
        .and_then(|g| g.get("source").and_then(Value::as_str))
        .unwrap_or("-")
        .to_string();
    (th("volume"), th("effective_limit"), source)
}

/// Whether the den is muted, off a state message.
fn den_muted(state: &str) -> bool {
    let value = json::parse(state).unwrap_or_else(|e| panic!("{}: {:?}", state, e));
    let Some(Value::Arr(zones)) = value.get("zones") else {
        panic!("no zones in {}", state)
    };
    zones
        .iter()
        .find(|z| z.get("id").and_then(Value::as_str) == Some("den"))
        .and_then(|z| z.get("muted").and_then(Value::as_bool))
        .unwrap()
}

fn wait_for(what: &str, limit: Duration, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + limit;
    while !done() {
        assert!(
            Instant::now() < deadline,
            "{} did not happen within {:?}",
            what,
            limit
        );
        thread::sleep(Duration::from_millis(20));
    }
}

/// The hub's HDMI ARC input, modelled: digital silence in real time.
struct SilentTv {
    pos: u64,
    started: Instant,
}

impl CaptureSource for SilentTv {
    fn device(&self) -> &str {
        "modelled-hdmi-arc"
    }
    fn frame_len(&self) -> usize {
        4
    }
    fn read(&mut self, pcm: &mut [u8]) -> Result<CaptureRead, CaptureError> {
        let n = (pcm.len() / 4) as u64;
        let due = self.started + Duration::from_nanos((self.pos + n) * 1_000_000_000 / 48_000);
        let now = Instant::now();
        if due > now {
            thread::sleep(due - now);
        }
        pcm.fill(0);
        self.pos += n;
        Ok(CaptureRead {
            frames: n,
            overran: false,
        })
    }
    fn delay_frames(&mut self) -> Result<Option<i64>, CaptureError> {
        Ok(Some(0))
    }
}

#[test]
fn tv_keys_move_the_rooms_volume_within_its_limit_and_tv_power_plays_and_stops_the_tv() {
    let pcm = constant_source();
    let tz = format!(
        "{}/../../fixtures/schedule/Etc_UTC.slim.tzif",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut server = RunningServer::start(&[
        "--source",
        pcm.to_str().unwrap(),
        "--slots",
        "3",
        "--max-clients",
        "6",
        "--tz",
        &tz,
        "--civil-time-from",
        "2026-10-05T20:00:00Z",
        // The autoplay hold (30 s of schedule) runs in 1.5 s.
        "--schedule-time-scale",
        "20",
        "--zone",
        "den",
    ]);
    server.wait_for("civil tz=");
    let hub = fresh_id("cec-hub");
    server.applied(&format!(
        r#"{{"v":1,"t":"attach","zone":"den","endpoint":"{}"}}"#,
        hub
    ));
    server.applied(r#"{"v":1,"t":"volume","zone":"den","volume":0.500}"#);
    server.applied(r#"{"v":2,"t":"limit","zone":"den","limit":0.560}"#);
    assert_eq!(den(&server.state()), (500, 560, "stream".to_string()));

    // The bus, the TV and the hub's CEC role.
    let bus = FakeBus::new();
    let tv = FakeTv::start(&bus, TvKind::RokuLike);
    let lines = Arc::new(Mutex::new(Vec::<String>::new()));
    let log: Arc<dyn Fn(&str) + Send + Sync> = {
        let lines = Arc::clone(&lines);
        Arc::new(move |l: &str| lines.lock().unwrap().push(l.to_string()))
    };
    let said = |what: &str| lines.lock().unwrap().iter().any(|l| l.contains(what));
    let t0 = Instant::now();
    let opener = bus.clone();
    let cec = CecRole::start(
        move || Ok(opener.adapter(PhysicalAddress(0x1000))),
        &CecConfig::new("fake-cec0"),
        Arc::new(move || t0.elapsed().as_millis() as u64),
        Arc::clone(&log),
    );
    wait_for("the hub claims address 5", Duration::from_secs(5), || {
        said("cec claimed logical_address=5")
    });

    // The hub's session: player, controller (CEC adds it) and source.
    let input = LineInConfig {
        name: "TV".to_string(),
        kind: SourceKind::HdmiArc,
        ..LineInConfig::new("modelled-hdmi-arc")
    };
    let config = ClientConfig {
        endpoint: hub.clone(),
        zone: "den".to_string(),
        line_in: Some(input.clone()),
        extra_roles: roles::CONTROLLER,
        cec: Some(CecConfig::new("fake-cec0")),
        ..ClientConfig::default()
    };
    let stream = TcpStream::connect(&server.audio).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    let mut me = EndpointIdentity::ephemeral(&hub).unwrap();
    let secure = session::open(stream, &mut me, &config).expect("the session opens");
    let session::Session {
        mut reader,
        writer,
        announced,
        source_control,
        ..
    } = secure;
    session::also_hand(&mut reader, &announced, cec.server_messages());
    let writer = SharedWriter::new(writer);
    {
        let mut uplink = writer.clone();
        cec.connect(Box::new(move |m| uplink.send(m)));
    }
    let session_line = server.wait_for_all(&["client session", &hub]);
    let reading = Arc::new(AtomicBool::new(true));
    let reader_thread = {
        let reading = Arc::clone(&reading);
        thread::spawn(move || {
            let mut sink = vec![0u8; 64 * 1024];
            while reading.load(Ordering::SeqCst) {
                match reader.read(&mut sink) {
                    Ok(0) => return,
                    Ok(_) => {}
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            || e.kind() == std::io::ErrorKind::TimedOut => {}
                    Err(_) => return,
                }
            }
        })
    };
    let counters = Arc::new(Counters::new());
    counters.offset.publish(Some(0));
    let source = {
        let lines = Arc::clone(&lines);
        let started = Instant::now();
        source::spawn(
            SilentTv { pos: 0, started },
            writer.clone(),
            SourceSetup {
                input,
                listed_codecs: Codec::Pcm.bit(),
                clock: Box::new(move || started.elapsed().as_nanos() as u64),
                counters,
                controls: source_control,
                thresholds: SignalThresholds::default(),
                log: Box::new(move |l| lines.lock().unwrap().push(l.to_string())),
                tv_power: Some(TvSignal::new(cec.tv_power(), true)),
            },
        )
    };
    println!("server: {session_line}");

    // The TV turns System Audio Mode on; the hub broadcasts it.
    tv.send(build::system_audio_mode_request(
        TV,
        AUDIO_SYSTEM,
        Some(PhysicalAddress::TV),
    ));
    assert!(tv
        .expect(0, Duration::from_secs(5), |m| *m
            == build::set_system_audio_mode(AUDIO_SYSTEM, BROADCAST, true))
        .is_some());

    // Volume Up, five presses: 0.500 + 5 x 0.020 asks for 0.600, and the
    // den's limit holds it at 0.560.
    let heard_before = tv.heard().len();
    for _ in 0..5 {
        tv.press(ui::VOLUME_UP);
        thread::sleep(Duration::from_millis(150));
    }
    wait_for(
        "the hub sends five commands",
        Duration::from_secs(5),
        || cec.counters().commands_sent.load(Ordering::Relaxed) == 5,
    );
    let line = server.wait_for(&format!("command=volume_step value={}", VOLUME_STEP));
    assert!(
        line.contains("zone=den") && line.contains("applied"),
        "{line}"
    );
    wait_for("the den reaches its limit", Duration::from_secs(5), || {
        den(&server.state()).0 == 560
    });
    // One more press: still the limit.
    tv.press(ui::VOLUME_UP);
    wait_for("the sixth command", Duration::from_secs(5), || {
        cec.counters().commands_sent.load(Ordering::Relaxed) == 6
    });
    thread::sleep(Duration::from_millis(300));
    assert_eq!(den(&server.state()).0, 560, "never above the limit");

    // The change the keys caused reached the TV unprompted, and Give Audio
    // Status reports the server's 56.
    let pushed = tv
        .expect(heard_before, Duration::from_secs(5), |m| {
            m.opcode == Some(opcode::REPORT_AUDIO_STATUS) && m.operands == [56]
        })
        .expect("Report Audio Status pushed after the keys");
    println!("pushed: {pushed}");
    let n = tv.heard().len();
    tv.send(build::user_control_released(TV, AUDIO_SYSTEM));
    tv.send(chorus_cec::Message::new(
        TV,
        AUDIO_SYSTEM,
        opcode::GIVE_AUDIO_STATUS,
        &[],
    ));
    let answer = tv
        .expect(n, Duration::from_secs(5), |m| {
            m.opcode == Some(opcode::REPORT_AUDIO_STATUS)
        })
        .expect("Give Audio Status answered");
    assert_eq!(answer.operands, vec![56], "volume 56, not muted");

    // A TV asking for System Audio Mode is on: the TV input was offered,
    // and with no autoplay rule yet nothing played it. Its standby ends the
    // offer at once.
    assert!(said("source-offer source_id=1 signal=1 reason=tv-on"));
    assert_eq!(den(&server.state()).2, "stream");
    tv.standby();
    wait_for("the first standby", Duration::from_secs(2), || {
        said("source-offer source_id=1 signal=0 reason=standby")
    });
    lines.lock().unwrap().clear();

    // The den's autoplay rule for the TV input. The keys above came before
    // it; the keys below come while the autoplay plays, which must not
    // detach the den from it (a level is not a source change, ADR 0094).
    server.applied(&format!(
        r#"{{"v":2,"t":"autoplay","input":"{}/line-1","target":"den","enabled":true}}"#,
        hub
    ));

    // The TV turns on: with no audio at all, the TV input is offered, the
    // den's autoplay takes it and the hub is told to start.
    tv.power_on(TvKind::RokuLike);
    wait_for(
        "the hub offers the TV input",
        Duration::from_secs(5),
        || said("source-offer source_id=1 signal=1 reason=tv-on"),
    );
    wait_for("the hub is told to start", Duration::from_secs(10), || {
        said("source-started")
    });
    wait_for("the den plays the TV", Duration::from_secs(5), || {
        den(&server.state()).2 == format!("line-in:{}/line-1", hub)
    });

    assert_eq!(den(&server.state()).0, 560);
    assert!(!den_muted(&server.state()));

    // While the TV plays: Volume Down twice and Mute, from the TV's remote
    // through the hub. The den follows (0.520, muted) and stays the
    // autoplay's.
    let sent = cec.counters().commands_sent.load(Ordering::Relaxed);
    for key in [ui::VOLUME_DOWN, ui::VOLUME_DOWN] {
        tv.press(key);
        thread::sleep(Duration::from_millis(150));
    }
    tv.send(build::user_control_released(TV, AUDIO_SYSTEM));
    wait_for("the den turned down", Duration::from_secs(5), || {
        den(&server.state()).0 == 520
    });
    tv.press(ui::MUTE);
    tv.send(build::user_control_released(TV, AUDIO_SYSTEM));
    wait_for("the den muted", Duration::from_secs(5), || {
        den_muted(&server.state())
    });
    assert_eq!(
        cec.counters().commands_sent.load(Ordering::Relaxed),
        sent + 3,
        "two volume steps and a mute"
    );
    assert_eq!(
        den(&server.state()).2,
        format!("line-in:{}/line-1", hub),
        "the den still plays the TV"
    );

    // Standby: the signal ends at once, the den's autoplay stops the TV and
    // restores the stream, at the volume and mute the den had before the TV
    // took it (0.560, not muted): it was still the autoplay's to restore.
    tv.standby();
    wait_for(
        "the signal ends with the standby",
        Duration::from_secs(2),
        || said("source-offer source_id=1 signal=0 reason=standby"),
    );
    assert!(said("cec tv-power state=standby"));
    wait_for("the hub is told to stop", Duration::from_secs(10), || {
        said("source-stopped")
    });
    wait_for("the den is restored", Duration::from_secs(5), || {
        den(&server.state()).2 == "stream"
    });
    assert_eq!(den(&server.state()).0, 560, "the volume before the TV");
    assert!(!den_muted(&server.state()), "the mute before the TV");
    // The server's log, read in order up to the autoplay's stop: nothing
    // detached the den on the way.
    server.wait_for(&format!(
        "schedule autoplay input={}/line-1 stopped reason=",
        hub
    ));
    assert!(
        !server.seen.iter().any(|l| l.contains("detached")),
        "the TV's keys did not detach the den from its autoplay"
    );
    assert!(!said("cec transmit"), "every CEC transmit was acknowledged");

    let (stop, stats) = source.stop();
    assert_eq!(stop.name(), "stopped");
    assert_eq!(stats.starts.load(Ordering::SeqCst), 1);
    cec.disconnect();
    cec.stop();
    reading.store(false, Ordering::SeqCst);
    let _ = reader_thread.join();
    for l in lines.lock().unwrap().iter() {
        println!("hub: {l}");
    }
    for l in server
        .seen
        .iter()
        .filter(|l| l.contains("controller") || l.contains("autoplay"))
    {
        println!("server: {l}");
    }
    let _ = std::fs::remove_file(&pcm);
}
