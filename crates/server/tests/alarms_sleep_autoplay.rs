//! Alarms, sleep timers, quiet hours and line-in autoplay on the real binary
//! (goal 11; K30, K78, K80, K81, K94; ADR 0076 and the ADR that wires it).
//!
//! Every test runs the real `chorus-server` with stream slots and a control
//! plane, its civil clock started at a UTC instant the test chose
//! (`--civil-time-from`) in the committed UTC zone fixture (`--tz`), and the
//! schedule's durations run faster (`--schedule-time-scale`), so an alarm
//! rings seconds after start whatever day the test runs on. The audio thread
//! is never scaled: chunks are real-time 20 ms ones.
//!
//! The players are protocol v2 sessions opened through the Linux client's own
//! session code (`common::Player`), and in the quiet-hours test the REAL Linux
//! client's playout (`run_session` on a modelled recording device, as
//! `limits_hold_for_every_volume_path.rs` runs it). The line-in is a scripted
//! source endpoint on the same session code: it declares the source role,
//! offers `line-1`, and on `source_control` start sends `stream_format` and
//! real-time 20 ms chunks of a known pattern, a triangle wave whose every
//! sample names the source frame it is (`pattern`), so what a room receives
//! can be read back as the source positions it plays.
//!
//! The test names are the evidence: an alarm with a chime fires at its civil
//! time and ramps; an alarm with a line-in plays the line-in and ramps; a
//! sleep timer fades, stops and restores; quiet hours pull a room down and cap
//! an alarm's ramp on the server, on the wire and in the Linux client; line-in
//! autoplay plays and stops after its hold, from the scripted source and from
//! the REAL Linux client's source role (`source::spawn` on a modelled capture
//! device paced in real time, its own signal detection deciding when the
//! input is offered and withdrawn); and a second room joining a
//! line-in's group grows its latency without a glitch in the playing room
//! (K94 on the real binary). Nothing here is timing evidence: what is graded
//! is values, orders and counts.

mod common;

use std::io::Read;
use std::net::{Shutdown, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use chorus_audio::MonotonicTimeline;
use chorus_client_linux::config::{ClientConfig, LineInConfig};
use chorus_client_linux::control::ZoneWatch;
use chorus_client_linux::delaylog::DelayLog;
use chorus_client_linux::receive::handshake;
use chorus_client_linux::run::{header_for, run_session};
use chorus_client_linux::session::{self, EndpointIdentity};
use chorus_client_linux::sink::{PcmSink, SinkError, SinkWrite};
use chorus_client_linux::source::{
    self, CaptureError, CaptureRead, CaptureSource, SharedWriter, SignalThresholds, SourceSetup,
};
use chorus_client_linux::Counters;
use chorus_control::json::{self, Value};
use chorus_protocol::v2::{
    roles, ChannelPosition, Codec, Message as V2Message, RoomVolume, SourceAction, SourceKind,
    SourceOffer, StreamFormat,
};
use chorus_protocol::{
    decode_frame, AudioChunk, FrameOutcome, Message, SampleFormat, StreamEnd, RESERVED_LEN,
};
use common::{fresh_id, Player, RunningServer};

const RATE_HZ: u32 = 48_000;
const FRAMES: usize = 960;
/// The configured stream: every sample this value, so it is told apart from
/// a chime, a line-in and silence by every byte.
const SAMPLE: i16 = 0x1234;
/// Half the line-in pattern's period, in frames.
const HALF: u64 = 30_000;

fn constant_source(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("chorus-asa-{}-{}.pcm", name, std::process::id()));
    let bytes: Vec<u8> = std::iter::repeat_n(SAMPLE.to_le_bytes(), RATE_HZ as usize * 2 * 40)
        .flatten()
        .collect();
    std::fs::write(&path, bytes).unwrap();
    path
}

fn utc_zone() -> String {
    format!(
        "{}/../../fixtures/schedule/Etc_UTC.slim.tzif",
        env!("CARGO_MANIFEST_DIR")
    )
}

/// The server, its clock at `from` (a Monday, 2026-10-05) run `scale` times
/// faster, serving `zones`.
fn server(source: &Path, from: &str, scale: &str, zones: &[&str]) -> RunningServer {
    let mut args: Vec<String> = [
        "--source",
        source.to_str().unwrap(),
        "--slots",
        "3",
        "--max-clients",
        "6",
        "--tz",
        &utc_zone(),
        "--civil-time-from",
        from,
        "--schedule-time-scale",
        scale,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for z in zones {
        args.push("--zone".into());
        args.push(z.to_string());
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut s = RunningServer::start(&refs);
    s.wait_for("civil tz=");
    s
}

fn samples(c: &AudioChunk) -> Vec<i16> {
    c.audio_data
        .as_chunks::<2>()
        .0
        .iter()
        .map(|s| i16::from_le_bytes(*s))
        .collect()
}

fn is_stream(c: &AudioChunk) -> bool {
    samples(c).iter().all(|s| *s == SAMPLE)
}

fn is_silent(c: &AudioChunk) -> bool {
    c.audio_data.iter().all(|b| *b == 0)
}

/// Source frame `i` of the line-in: a triangle from 2000 to 32000 and back,
/// so every sample is nonzero and, away from the two corners, names its frame.
fn pattern(i: u64) -> i16 {
    let m = i % (2 * HALF);
    let t = if m < HALF { m } else { 2 * HALF - m };
    (2_000 + t) as i16
}

/// The source position a chunk's first frame plays, read off its first
/// sample of channel 0 and, for the slope, its fifth frame's (a line-in's
/// first chunk holds its first frame for two more); `None` near a corner,
/// where the slope is ambiguous. `near` is where it is expected, to pick the
/// right period.
fn position(c: &AudioChunk, near: Option<f64>) -> Option<f64> {
    let s = samples(c);
    let (v0, v1) = (f64::from(s[0]), f64::from(s[8]));
    if !(2_010.0..=31_990.0).contains(&v0) {
        return None;
    }
    let cand = if v1 > v0 {
        v0 - 2_000.0
    } else {
        (2 * HALF) as f64 - (v0 - 2_000.0)
    };
    let period = (2 * HALF) as f64;
    Some(match near {
        Some(e) => cand + ((e - cand) / period).round() * period,
        None => cand,
    })
}

/// A room's volume and effective limit (thousandths) and its group's
/// source, off a state message.
fn room(state: &str, id: &str) -> (u32, u32, String) {
    let value = json::parse(state).unwrap_or_else(|e| panic!("{}: {:?}", state, e));
    let Some(Value::Arr(zones)) = value.get("zones") else {
        panic!("no zones in {}", state)
    };
    let zone = zones
        .iter()
        .find(|z| z.get("id").and_then(Value::as_str) == Some(id))
        .unwrap_or_else(|| panic!("no zone {} in {}", id, state));
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

fn ringing(state: &str, alarm: &str) -> bool {
    let value = json::parse(state).unwrap();
    let Some(Value::Arr(alarms)) = value.get("alarms") else {
        return false;
    };
    alarms
        .iter()
        .find(|a| a.get("alarm").and_then(Value::as_str) == Some(alarm))
        .and_then(|a| a.get("ringing").and_then(Value::as_bool))
        .unwrap_or(false)
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

/// Read a session's chunks into a list until stopped, keeping its v2
/// messages too.
struct Recorder {
    chunks: Arc<Mutex<Vec<AudioChunk>>>,
    messages: Arc<Mutex<Vec<V2Message>>>,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl Recorder {
    fn player(audio: &str, endpoint: &str) -> Recorder {
        let player = Player::connect(audio, endpoint, 0);
        let (session, messages) = player.split();
        Recorder::reading(session.reader, messages)
    }

    fn reading(
        mut reader: chorus_protocol::v2::session::SecureReader<TcpStream>,
        messages: Arc<Mutex<Vec<V2Message>>>,
    ) -> Recorder {
        let chunks = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let join = {
            let chunks = Arc::clone(&chunks);
            let stop = Arc::clone(&stop);
            thread::spawn(move || {
                let mut pending: Vec<u8> = Vec::new();
                let mut scratch = vec![0u8; 65_536];
                while !stop.load(Ordering::SeqCst) {
                    match reader.read(&mut scratch) {
                        Ok(0) => return,
                        Ok(n) => pending.extend_from_slice(&scratch[..n]),
                        Err(_) => continue,
                    }
                    let mut at = 0usize;
                    while at < pending.len() {
                        let d = decode_frame(&pending[at..]);
                        if d.consumed == 0 {
                            break;
                        }
                        at += d.consumed;
                        if let FrameOutcome::Decoded(Message::AudioChunk(c)) = d.outcome {
                            chunks.lock().unwrap().push(c);
                        }
                    }
                    pending.drain(..at);
                }
            })
        };
        Recorder {
            chunks,
            messages,
            stop,
            join: Some(join),
        }
    }

    fn chunks(&self) -> Vec<AudioChunk> {
        self.chunks.lock().unwrap().clone()
    }

    fn count(&self) -> usize {
        self.chunks.lock().unwrap().len()
    }

    fn room_volumes(&self) -> Vec<RoomVolume> {
        self.messages
            .lock()
            .unwrap()
            .iter()
            .filter_map(|m| match m {
                V2Message::RoomVolume(r) => Some(*r),
                _ => None,
            })
            .collect()
    }

    /// Wait until the last chunk received satisfies `f`.
    fn until_hearing(&self, what: &str, limit: Duration, f: impl Fn(&AudioChunk) -> bool) {
        wait_for(what, limit, || {
            self.chunks.lock().unwrap().last().is_some_and(&f)
        });
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

/// A scripted line-in endpoint: a player that also declares the source role,
/// offers `line-1`, and streams [`pattern`] upstream in real time while
/// started.
struct LineIn {
    /// Its player half.
    hears: Recorder,
    signal: Arc<AtomicBool>,
    starts: Arc<AtomicU64>,
    stops: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl LineIn {
    fn start(audio: &str, endpoint: &str, signal: bool) -> LineIn {
        let player = Player::connect(audio, endpoint, roles::SOURCE);
        let (session, messages) = player.split();
        let mut writer = session.writer;
        let controls = session.source_control;
        let hears = Recorder::reading(session.reader, messages);
        let signal = Arc::new(AtomicBool::new(signal));
        let starts = Arc::new(AtomicU64::new(0));
        let stops = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let join = {
            let (signal, starts, stops, stop) = (
                Arc::clone(&signal),
                Arc::clone(&starts),
                Arc::clone(&stops),
                Arc::clone(&stop),
            );
            thread::spawn(move || {
                let offer = |s: bool| {
                    V2Message::SourceOffer(SourceOffer {
                        source_id: 1,
                        kind: SourceKind::LineIn,
                        signal: s,
                        name: "line-1".to_string(),
                        reason: 0,
                    })
                };
                let mut offered = signal.load(Ordering::SeqCst);
                if writer.send(&offer(offered)).is_err() {
                    return;
                }
                let mut streaming = false;
                let mut frame = 0u64;
                let mut sequence = 0u32;
                let mut next = Instant::now();
                while !stop.load(Ordering::SeqCst) {
                    let now_signal = signal.load(Ordering::SeqCst);
                    if now_signal != offered {
                        offered = now_signal;
                        if writer.send(&offer(offered)).is_err() {
                            return;
                        }
                    }
                    while let Ok(control) = controls.try_recv() {
                        match control.action {
                            SourceAction::Start => {
                                let format = V2Message::StreamFormat(StreamFormat {
                                    codec: Codec::Pcm,
                                    sample_format: SampleFormat::PcmS16Le,
                                    sample_rate_hz: RATE_HZ,
                                    channel_map: vec![
                                        ChannelPosition::FrontLeft,
                                        ChannelPosition::FrontRight,
                                    ],
                                    frames_per_chunk: FRAMES as u32,
                                    codec_config: Vec::new(),
                                });
                                if writer.send(&format).is_err() {
                                    return;
                                }
                                streaming = true;
                                frame = 0;
                                sequence = 0;
                                starts.fetch_add(1, Ordering::SeqCst);
                            }
                            SourceAction::Stop => {
                                if streaming {
                                    let _ = writer.send(&V2Message::StreamEnd(StreamEnd {
                                        final_sequence: sequence.wrapping_sub(1),
                                        end_timestamp_ns: 0,
                                    }));
                                }
                                streaming = false;
                                stops.fetch_add(1, Ordering::SeqCst);
                            }
                        }
                    }
                    if streaming {
                        let pcm: Vec<u8> = (frame..frame + FRAMES as u64)
                            .flat_map(|i| {
                                let b = pattern(i).to_le_bytes();
                                [b[0], b[1], b[0], b[1]]
                            })
                            .collect();
                        let chunk = AudioChunk {
                            sequence,
                            timestamp_ns: frame * 1_000_000_000 / u64::from(RATE_HZ),
                            sample_rate_hz: RATE_HZ,
                            channels: 2,
                            sample_format: SampleFormat::PcmS16Le,
                            reserved: [0u8; RESERVED_LEN],
                            audio_data: pcm,
                        };
                        if writer.send(&V2Message::AudioChunk(chunk)).is_err() {
                            return;
                        }
                        frame += FRAMES as u64;
                        sequence = sequence.wrapping_add(1);
                    }
                    next += Duration::from_millis(20);
                    let now = Instant::now();
                    if next > now {
                        thread::sleep(next - now);
                    }
                }
            })
        };
        LineIn {
            hears,
            signal,
            starts,
            stops,
            stop,
            join: Some(join),
        }
    }
}

impl Drop for LineIn {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

/// The gains of `rvs` from the first 0 on, which an alarm's fire sends.
fn rise_from_zero(rvs: &[RoomVolume], from: usize) -> Vec<RoomVolume> {
    let at = rvs[from..]
        .iter()
        .position(|r| r.gain == 0)
        .map(|i| i + from)
        .unwrap_or_else(|| panic!("no gain 0 after the fire: {:?}", rvs));
    rvs[at..].to_vec()
}

fn assert_rises_in_steps(rise: &[RoomVolume], target: u16, what: &str) {
    let gains: Vec<u16> = rise.iter().map(|r| r.gain).collect();
    let top = gains
        .iter()
        .position(|g| *g == target)
        .unwrap_or_else(|| panic!("{}: never reached {}: {:?}", what, target, gains));
    let up = &rise[..=top];
    assert!(
        up.windows(2).all(|w| w[1].gain >= w[0].gain),
        "{}: the rise is not monotone: {:?}",
        what,
        gains
    );
    assert!(
        up.len() >= 8,
        "{}: a ramp is stepped, not a jump: {:?}",
        what,
        gains
    );
    assert!(
        up[1..].iter().all(|r| r.ramp_ms > 0),
        "{}: each step is ramped: {:?}",
        what,
        up
    );
    assert!(up.iter().all(|r| r.gain <= r.limit), "{}: {:?}", what, up);
}

/// The source positions `chunks` play, from the first that carries the
/// line-in: each chunk's sequence and position (corners skipped), checked
/// for contiguous sequences, no silent or zero sample, and steps a resampler
/// held to 500 ppm can make.
fn line_in_positions(chunks: &[AudioChunk], what: &str) -> Vec<(u32, f64)> {
    let first = chunks
        .iter()
        .position(|c| !is_silent(c) && !is_stream(c))
        .unwrap_or_else(|| panic!("{}: the line-in never played", what));
    let played = &chunks[first..];
    let mut out: Vec<(u32, f64)> = Vec::new();
    for (k, c) in played.iter().enumerate() {
        if k > 0 {
            assert_eq!(
                c.sequence,
                played[k - 1].sequence.wrapping_add(1),
                "{}: sequences are contiguous",
                what
            );
        }
        assert!(
            samples(c).iter().all(|s| *s != 0),
            "{}: chunk {} has an inserted zero (an underrun or a splice)",
            what,
            c.sequence
        );
        let near = out
            .last()
            .map(|(seq, pos)| pos + f64::from(c.sequence - seq) * FRAMES as f64);
        if let Some(pos) = position(c, near) {
            if let Some((seq, prev)) = out.last() {
                let per = (pos - prev) / f64::from(c.sequence - seq);
                assert!(
                    (FRAMES as f64 * (1.0 - 6e-4) - 1.5..=FRAMES as f64 + 1.5).contains(&per),
                    "{}: chunk {} plays {} source frames per chunk",
                    what,
                    c.sequence,
                    per
                );
            }
            out.push((c.sequence, pos));
        }
    }
    out
}

// --- the tests ----------------------------------------------------------------

#[test]
fn an_alarm_with_a_chime_fires_at_its_civil_time_ramps_up_and_alarm_stop_restores() {
    let source = constant_source("chime");
    // 07:00 civil is 30 s of schedule time, 3 s of real time, after start.
    let server = server(&source, "2026-10-05T06:59:30Z", "10", &["kitchen"]);
    let speaker = fresh_id("asa-chime");
    server.applied(&format!(
        r#"{{"v":1,"t":"attach","zone":"kitchen","endpoint":"{}"}}"#,
        speaker
    ));
    server.applied(r#"{"v":1,"t":"volume","zone":"kitchen","volume":0.400}"#);
    let kitchen = Recorder::player(&server.audio, &speaker);
    kitchen.until_hearing("the stream", Duration::from_secs(5), is_stream);
    server.applied(
        r#"{"v":2,"t":"alarm_set","alarm":"wake","target":"kitchen","time":"07:00","days":[],"source":"chime:bell","volume":0.600,"ramp_s":20,"duration_min":5,"enabled":true}"#,
    );
    let before = kitchen.room_volumes().len();
    wait_for("the alarm rings", Duration::from_secs(10), || {
        ringing(&server.state(), "wake")
    });
    let (_, _, playing) = room(&server.state(), "kitchen");
    assert_eq!(playing, "chime:bell");
    // 20 s of ramp is 2 s of real time.
    wait_for("the ramp reaches 0.600", Duration::from_secs(10), || {
        kitchen.room_volumes().last().is_some_and(|r| r.gain == 600)
    });
    let rise = rise_from_zero(&kitchen.room_volumes(), before);
    assert_rises_in_steps(&rise, 600, "the chime alarm");

    // The player received the chime's PCM, rendered by chorus_schedule at the
    // server's format, from its first frame.
    let chime = chorus_schedule::render(
        chorus_schedule::Chime::Bell,
        RATE_HZ,
        2,
        chorus_schedule::PcmFormat::S16Le,
    )
    .unwrap();
    wait_for("a second of the chime", Duration::from_secs(5), || {
        let c = kitchen.chunks();
        c.iter()
            .rposition(is_stream)
            .is_some_and(|i| c.len() > i + 60)
    });
    let chunks = kitchen.chunks();
    let after_stream = chunks.iter().rposition(is_stream).unwrap() + 1;
    let heard: Vec<u8> = chunks[after_stream..after_stream + 50]
        .iter()
        .flat_map(|c| c.audio_data.clone())
        .collect();
    assert_eq!(
        heard,
        chime[..heard.len()],
        "the chunks after the switch are the bell, byte for byte"
    );
    for w in chunks.windows(2) {
        assert_eq!(w[1].sequence, w[0].sequence.wrapping_add(1), "contiguous");
    }

    // alarm_stop: a fade to 0, then the room as it was.
    let stopped_at = kitchen.room_volumes().len();
    server.applied(r#"{"v":2,"t":"alarm_stop","alarm":"wake"}"#);
    wait_for("the room is restored", Duration::from_secs(10), || {
        let (v, _, s) = room(&server.state(), "kitchen");
        v == 400 && s == "stream" && !ringing(&server.state(), "wake")
    });
    wait_for("the restored gain is sent", Duration::from_secs(5), || {
        kitchen.room_volumes().last().is_some_and(|r| r.gain == 400)
    });
    let after: Vec<u16> = kitchen.room_volumes()[stopped_at..]
        .iter()
        .map(|r| r.gain)
        .collect();
    let zero = after
        .iter()
        .position(|g| *g == 0)
        .unwrap_or_else(|| panic!("alarm_stop fades to 0 before the restore: {:?}", after));
    assert!(
        after[..=zero].windows(2).all(|w| w[1] <= w[0]),
        "the end fade falls: {:?}",
        after
    );
    kitchen.until_hearing("the stream again", Duration::from_secs(5), is_stream);
    let _ = std::fs::remove_file(&source);
}

#[test]
fn an_alarm_with_a_line_in_plays_the_line_in_and_ramps() {
    let source = constant_source("alarm-line-in");
    let server = server(&source, "2026-10-05T06:59:30Z", "10", &["kitchen"]);
    let speaker = fresh_id("asa-li-speaker");
    let amp = fresh_id("asa-li-amp");
    server.applied(&format!(
        r#"{{"v":1,"t":"attach","zone":"kitchen","endpoint":"{}"}}"#,
        speaker
    ));
    let kitchen = Recorder::player(&server.audio, &speaker);
    let line = LineIn::start(&server.audio, &amp, true);
    wait_for("the input is offered", Duration::from_secs(5), || {
        server.state().contains(&format!("{}/line-1", amp))
    });
    server.applied(&format!(
        r#"{{"v":2,"t":"alarm_set","alarm":"radio","target":"kitchen","time":"07:00","days":[],"source":"line-in:{}/line-1","volume":0.500,"ramp_s":20,"duration_min":5,"enabled":true}}"#,
        amp
    ));
    let before = kitchen.room_volumes().len();
    wait_for("the alarm rings", Duration::from_secs(10), || {
        ringing(&server.state(), "radio")
    });
    assert_eq!(
        room(&server.state(), "kitchen").2,
        format!("line-in:{}/line-1", amp),
        "no fallback: the line-in was offered"
    );
    wait_for("the line-in is started", Duration::from_secs(5), || {
        line.starts.load(Ordering::SeqCst) == 1
    });
    wait_for("the ramp reaches 0.500", Duration::from_secs(10), || {
        kitchen.room_volumes().last().is_some_and(|r| r.gain == 500)
    });
    assert_rises_in_steps(
        &rise_from_zero(&kitchen.room_volumes(), before),
        500,
        "the line-in alarm",
    );
    // Two seconds of the line-in.
    let n = kitchen.count();
    wait_for(
        "two seconds of the line-in",
        Duration::from_secs(10),
        || kitchen.count() > n + 100,
    );
    let positions = line_in_positions(&kitchen.chunks(), "the line-in alarm");
    assert!(positions.len() > 50, "{} positions", positions.len());
    server.applied(r#"{"v":2,"t":"alarm_stop","alarm":"radio"}"#);
    wait_for("the line-in is stopped", Duration::from_secs(10), || {
        line.stops.load(Ordering::SeqCst) == 1
    });
    kitchen.until_hearing("the stream again", Duration::from_secs(10), is_stream);
    let _ = std::fs::remove_file(&source);
}

#[test]
fn a_sleep_timer_fades_to_zero_then_stops_and_restores_the_volume() {
    let source = constant_source("sleep");
    let server = server(&source, "2026-10-05T22:00:00Z", "20", &["bedroom"]);
    let speaker = fresh_id("asa-sleep");
    server.applied(&format!(
        r#"{{"v":1,"t":"attach","zone":"bedroom","endpoint":"{}"}}"#,
        speaker
    ));
    server.applied(r#"{"v":1,"t":"volume","zone":"bedroom","volume":0.500}"#);
    let bedroom = Recorder::player(&server.audio, &speaker);
    bedroom.until_hearing("the stream", Duration::from_secs(5), is_stream);
    let before = bedroom.room_volumes().len();
    // One minute: the fade starts 30 s in and ends at the minute, 1.5 s and
    // 3 s of real time at 20 times.
    server.applied(r#"{"v":2,"t":"sleep","target":"bedroom","minutes":1}"#);
    wait_for(
        "the sleep timer stops the room",
        Duration::from_secs(10),
        || room(&server.state(), "bedroom").2 == "none",
    );
    let (volume, _, _) = room(&server.state(), "bedroom");
    assert_eq!(volume, 500, "the volume is restored in the state");
    wait_for("the restored gain is sent", Duration::from_secs(5), || {
        bedroom.room_volumes().last().is_some_and(|r| r.gain == 500)
    });
    let gains: Vec<u16> = bedroom.room_volumes()[before..]
        .iter()
        .map(|r| r.gain)
        .collect();
    let zero = gains
        .iter()
        .position(|g| *g == 0)
        .unwrap_or_else(|| panic!("the fade reaches 0: {:?}", gains));
    assert!(zero >= 8, "the fade is stepped: {:?}", gains);
    assert!(
        gains[..=zero].windows(2).all(|w| w[1] <= w[0]),
        "the fade falls monotonically: {:?}",
        gains
    );
    assert_eq!(gains.last(), Some(&500), "{:?}", gains);
    // Stopped: silence from here on.
    let n = bedroom.count();
    wait_for(
        "half a second after the stop",
        Duration::from_secs(5),
        || bedroom.count() > n + 25,
    );
    assert!(
        bedroom.chunks()[n + 5..].iter().all(is_silent),
        "a stopped room plays silence"
    );
    let _ = std::fs::remove_file(&source);
}

/// The modelled device the Linux client writes to: drains at the nominal
/// rate and keeps nothing.
struct NullSink {
    queued: f64,
    played: u64,
    last: Instant,
}

impl NullSink {
    fn tick(&mut self) {
        let now = Instant::now();
        let n = (now.duration_since(self.last).as_secs_f64() * f64::from(RATE_HZ)).min(self.queued);
        self.last = now;
        self.queued -= n;
        self.played += n as u64;
    }
}

impl PcmSink for NullSink {
    fn device(&self) -> &str {
        "modelled-null"
    }
    fn frame_len(&self) -> usize {
        4
    }
    fn rate_hz(&self) -> u32 {
        RATE_HZ
    }
    fn write(&mut self, pcm: &[u8]) -> Result<SinkWrite, SinkError> {
        self.tick();
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

#[test]
fn quiet_hours_starting_on_schedule_pull_a_room_down_and_hold_an_alarm_ramp_at_the_cap() {
    let source = constant_source("quiet");
    // 07:00 is 3 s of real time in, 07:01 is 6 s.
    let mut server = server(&source, "2026-10-05T06:59:00Z", "20", &["kitchen", "study"]);
    let linux = fresh_id("asa-quiet-linux");
    let study = fresh_id("asa-quiet-study");
    for (zone, endpoint) in [("kitchen", &linux), ("study", &study)] {
        server.applied(&format!(
            r#"{{"v":1,"t":"attach","zone":"{}","endpoint":"{}"}}"#,
            zone, endpoint
        ));
    }
    server.applied(r#"{"v":1,"t":"volume","zone":"study","volume":0.600}"#);
    for zone in ["kitchen", "study"] {
        server.applied(&format!(
            r#"{{"v":2,"t":"quiet_hours","zone":"{}","windows":[{{"days":["mon"],"start":"07:01","end":"08:00","limit":0.300}}]}}"#,
            zone
        ));
    }
    // The alarm rises from 0 to 0.800 over 120 s (6 s real), so the window
    // starts half way up.
    server.applied(
        r#"{"v":2,"t":"alarm_set","alarm":"early","target":"kitchen","time":"07:00","days":["mon"],"source":"chime:triad","volume":0.800,"ramp_s":120,"duration_min":30,"enabled":true}"#,
    );

    // The real Linux client in the kitchen.
    let captured: Arc<Mutex<Vec<RoomVolume>>> = Arc::new(Mutex::new(Vec::new()));
    let watch = Arc::new(ZoneWatch::new());
    let stream = TcpStream::connect(&server.audio).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    let hang_up = stream.try_clone().unwrap();
    let log = std::env::temp_dir().join(format!("chorus-asa-quiet-{}.log", std::process::id()));
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
                if let V2Message::RoomVolume(r) = m {
                    captured.lock().unwrap().push(*r);
                }
            }),
        );
    }
    let player = {
        let watch = Arc::clone(&watch);
        let reader = secure.reader;
        let config = config.clone();
        thread::spawn(move || {
            let mut reader = reader;
            let hand = handshake(&mut reader, &|| true).expect("the stream starts");
            let mut sink = NullSink {
                queued: 0.0,
                played: 0,
                last: Instant::now(),
            };
            let header = header_for(&config, "modelled-null", &hand.shape);
            let mut log = DelayLog::open(&config.delay_log, &header).expect("the log opens");
            let _ = run_session(
                &config,
                reader,
                hand,
                &mut sink,
                &mut log,
                MonotonicTimeline::new(),
                Arc::new(Counters::new()),
                None,
                watch,
            );
        })
    };
    let in_study = Recorder::player(&server.audio, &study);

    // The alarm rings, then the window starts on schedule.
    wait_for("the alarm rings", Duration::from_secs(10), || {
        ringing(&server.state(), "early")
    });
    wait_for("the quiet window starts", Duration::from_secs(10), || {
        room(&server.state(), "kitchen").1 == 300
    });
    server.wait_for("schedule quiet-hours zone=kitchen effective_limit=0.300");
    let at_start = captured.lock().unwrap().len();
    // The study, at 0.600, is pulled down to the cap.
    wait_for("the study is pulled down", Duration::from_secs(5), || {
        in_study
            .room_volumes()
            .last()
            .is_some_and(|r| r.gain == 300 && r.limit == 300)
    });
    assert_eq!(room(&server.state(), "study").0, 300);
    // The rest of the ramp (to 07:02, 3 s more) runs under the cap.
    thread::sleep(Duration::from_secs(4));
    let state = server.state();
    let (volume, limit, _) = room(&state, "kitchen");
    assert_eq!(limit, 300, "{}", state);
    assert_eq!(volume, 300, "the ramp is held at the cap: {}", state);
    let all = captured.lock().unwrap().clone();
    assert!(
        all.iter().all(|r| r.gain <= r.limit),
        "no room_volume above its limit: {:?}",
        all
    );
    let since: Vec<RoomVolume> = all[at_start..].to_vec();
    assert!(
        since.iter().all(|r| r.gain <= 300 && r.limit == 300),
        "after the window starts nothing above the cap goes out: {:?}",
        since
    );
    assert!(
        all[..at_start].iter().any(|r| r.gain > 0 && r.gain < 300),
        "the ramp was rising before the window: {:?}",
        all
    );
    // And the Linux client applies min(gain, limit): the cap. (Two messages
    // in one chunk are taken as the later one, so its count can trail.)
    wait_for(
        "the Linux client applies the cap",
        Duration::from_secs(5),
        || watch.room().settled(watch.gain()).thousandths() == 300,
    );
    assert!(
        watch.room().messages() >= 2,
        "the client took the ramp's messages"
    );
    let _ = hang_up.shutdown(Shutdown::Both);
    let _ = player.join();
    let _ = std::fs::remove_file(&source);
    let _ = std::fs::remove_file(&log);
}

#[test]
fn line_in_autoplay_plays_to_its_target_and_stops_and_restores_after_the_hold() {
    let source = constant_source("autoplay");
    let server = server(&source, "2026-10-05T12:00:00Z", "20", &["kitchen"]);
    let speaker = fresh_id("asa-auto-speaker");
    let amp = fresh_id("asa-auto-amp");
    server.applied(&format!(
        r#"{{"v":1,"t":"attach","zone":"kitchen","endpoint":"{}"}}"#,
        speaker
    ));
    server.applied(&format!(
        r#"{{"v":2,"t":"autoplay","input":"{}/line-1","target":"kitchen","enabled":true}}"#,
        amp
    ));
    let kitchen = Recorder::player(&server.audio, &speaker);
    kitchen.until_hearing("the stream", Duration::from_secs(5), is_stream);
    let line = LineIn::start(&server.audio, &amp, false);
    thread::sleep(Duration::from_millis(500));
    assert_eq!(line.starts.load(Ordering::SeqCst), 0, "no signal, no start");
    assert_eq!(room(&server.state(), "kitchen").2, "stream");

    // Signal on: the target plays the line-in.
    line.signal.store(true, Ordering::SeqCst);
    wait_for("the line-in is started", Duration::from_secs(5), || {
        line.starts.load(Ordering::SeqCst) == 1
    });
    assert_eq!(
        room(&server.state(), "kitchen").2,
        format!("line-in:{}/line-1", amp)
    );
    let n = kitchen.count();
    wait_for(
        "two seconds of the line-in",
        Duration::from_secs(10),
        || kitchen.count() > n + 100,
    );
    let positions = line_in_positions(&kitchen.chunks(), "autoplay");
    assert!(positions.len() > 50);

    // Signal off: held 30 s (1.5 s real), then stopped and restored.
    line.signal.store(false, Ordering::SeqCst);
    let off = Instant::now();
    wait_for("the line-in is stopped", Duration::from_secs(10), || {
        line.stops.load(Ordering::SeqCst) == 1
    });
    assert!(
        off.elapsed() >= Duration::from_millis(1_200),
        "the hold was kept: stopped after {:?}",
        off.elapsed()
    );
    wait_for("the room is restored", Duration::from_secs(5), || {
        room(&server.state(), "kitchen").2 == "stream"
    });
    kitchen.until_hearing("the stream again", Duration::from_secs(5), is_stream);
    let _ = std::fs::remove_file(&source);
}

#[test]
fn a_second_room_joining_a_line_ins_group_grows_its_latency_without_a_glitch() {
    let source = constant_source("k94");
    let mut server = server(&source, "2026-10-05T12:00:00Z", "1", &["den", "kitchen"]);
    let amp = fresh_id("asa-k94-amp");
    let speaker = fresh_id("asa-k94-kitchen");
    for (zone, endpoint) in [("den", &amp), ("kitchen", &speaker)] {
        server.applied(&format!(
            r#"{{"v":1,"t":"attach","zone":"{}","endpoint":"{}"}}"#,
            zone, endpoint
        ));
    }
    // The line-in's endpoint is the den's player as well as its source.
    let line = LineIn::start(&server.audio, &amp, true);
    let kitchen = Recorder::player(&server.audio, &speaker);
    wait_for("the input is offered", Duration::from_secs(5), || {
        server.state().contains(&format!("{}/line-1", amp))
    });
    server.applied(&format!(
        r#"{{"v":2,"t":"take","target":"den","source":"line-in:{}/line-1"}}"#,
        amp
    ));
    server.wait_for("target_ms=30");
    // Two seconds alone, at L_local.
    wait_for("the den plays the line-in", Duration::from_secs(5), || {
        line.hears
            .chunks()
            .last()
            .is_some_and(|c| !is_silent(c) && !is_stream(c))
    });
    thread::sleep(Duration::from_secs(2));
    let joined_at = line.hears.count();
    server.applied(r#"{"v":2,"t":"join","zone":"kitchen","target":"den"}"#);
    server.wait_for("target_ms=180");
    // Seven seconds of the growth (ADR 0071: a 10 s raised-cosine ramp up to
    // 500 ppm; 7 s in, the offset has grown by about 1.1 ms).
    thread::sleep(Duration::from_secs(7));
    let chunks = line.hears.chunks();
    let positions = line_in_positions(&chunks, "the den through the join");
    // The stamp offset in frames: where the chunk is on the grid minus the
    // source frame it plays (the line-in's own capture instants are its frame
    // indices at 48 kHz), up to a constant.
    let offsets: Vec<(u32, f64)> = positions
        .iter()
        .map(|(seq, pos)| (*seq, f64::from(*seq) * FRAMES as f64 - pos))
        .collect();
    for w in offsets.windows(2) {
        assert!(
            w[1].1 >= w[0].1 - 1.0,
            "the stamp offset never falls: {:?} then {:?}",
            w[0],
            w[1]
        );
    }
    let join_seq = chunks[joined_at].sequence;
    let before: Vec<f64> = offsets
        .iter()
        .filter(|(s, _)| *s < join_seq)
        .map(|(_, o)| *o)
        .collect();
    let spread = before.iter().cloned().fold(f64::MIN, f64::max)
        - before.iter().cloned().fold(f64::MAX, f64::min);
    assert!(
        spread <= 1.5,
        "alone, at L_local, the offset holds: spread {} frames",
        spread
    );
    let grown = offsets.last().unwrap().1 - before.last().unwrap();
    assert!(
        (10.0..=120.0).contains(&grown),
        "the offset grew by {} frames in 7 s of the transition",
        grown
    );
    // The joining room hears the same line-in.
    assert!(
        kitchen
            .chunks()
            .last()
            .is_some_and(|c| !is_silent(c) && !is_stream(c)),
        "the kitchen plays the line-in too"
    );
    println!(
        "K94 on the real binary: {} den chunks contiguous, {} positions read back, offset \
         constant within {:.1} frames alone and grown by {:.1} frames ({:.3} ms) 7 s into the \
         transition, no zero sample",
        chunks.len(),
        positions.len(),
        spread,
        grown,
        grown / 48.0
    );
    let _ = std::fs::remove_file(&source);
}

/// The Linux client's line-in, modelled: [`pattern`] for `loud_frames`, then
/// digital silence, delivered in real time (a read returns once its last
/// frame is due), with no capture delay.
struct PacedCapture {
    pos: u64,
    loud_frames: u64,
    started: Instant,
}

impl CaptureSource for PacedCapture {
    fn device(&self) -> &str {
        "modelled-line-in"
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
        for (k, frame) in pcm.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let i = self.pos + k as u64;
            let v = if i < self.loud_frames { pattern(i) } else { 0 };
            let b = v.to_le_bytes();
            *frame = [b[0], b[1], b[0], b[1]];
        }
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
fn the_real_linux_clients_line_in_is_autoplayed_to_its_target_and_stopped_when_it_falls_silent() {
    let source = constant_source("real-line-in");
    let server = server(&source, "2026-10-05T12:00:00Z", "20", &["kitchen"]);
    let speaker = fresh_id("asa-real-speaker");
    let amp = fresh_id("asa-real-amp");
    server.applied(&format!(
        r#"{{"v":1,"t":"attach","zone":"kitchen","endpoint":"{}"}}"#,
        speaker
    ));
    // The client offers its input as `line-1`: the name it was configured
    // with is not an identifier, so the server names it by its source id.
    server.applied(&format!(
        r#"{{"v":2,"t":"autoplay","input":"{}/line-1","target":"kitchen","enabled":true}}"#,
        amp
    ));
    let kitchen = Recorder::player(&server.audio, &speaker);
    kitchen.until_hearing("the stream", Duration::from_secs(5), is_stream);

    // The real client: its session (hello with player | source, the first
    // offer) and its source role (signal detection, the start check,
    // stream_format, the chunks, stream_end) on the modelled line-in.
    let input = LineInConfig {
        name: "Turntable".to_string(),
        ..LineInConfig::new("modelled-line-in")
    };
    let config = ClientConfig {
        line_in: Some(input.clone()),
        ..ClientConfig::default()
    };
    let stream = TcpStream::connect(&server.audio).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    let mut me = EndpointIdentity::ephemeral(&amp).unwrap();
    let secure = session::open(stream, &mut me, &config).expect("the session opens");
    let session::Session {
        reader,
        writer,
        source_control,
        ..
    } = secure;
    let hears = Recorder::reading(reader, Arc::new(Mutex::new(Vec::new())));
    let counters = Arc::new(Counters::new());
    // The stamps the client sends are its capture instants through the
    // offset its playout loop publishes; this server plays a line-in on its
    // own grid and does not read them, so any published offset will do.
    counters.offset.publish(Some(0));
    let lines = Arc::new(Mutex::new(Vec::<String>::new()));
    let started = Instant::now();
    let handle = {
        let lines = Arc::clone(&lines);
        source::spawn(
            PacedCapture {
                pos: 0,
                loud_frames: 48_000 * 4,
                started,
            },
            SharedWriter::new(writer),
            SourceSetup {
                input,
                listed_codecs: Codec::Pcm.bit(),
                clock: Box::new(move || started.elapsed().as_nanos() as u64),
                counters,
                controls: source_control,
                thresholds: SignalThresholds::default(),
                log: Box::new(move |l| lines.lock().unwrap().push(l.to_string())),
            },
        )
    };
    let said = |what: &str| lines.lock().unwrap().iter().any(|l| l.contains(what));
    wait_for(
        "the client starts its input",
        Duration::from_secs(10),
        || said("source-started"),
    );
    assert_eq!(
        room(&server.state(), "kitchen").2,
        format!("line-in:{}/line-1", amp)
    );
    wait_for(
        "two seconds of the line-in",
        Duration::from_secs(10),
        || {
            let c = kitchen.chunks();
            c.iter()
                .position(|c| !is_silent(c) && !is_stream(c))
                .is_some_and(|i| c.len() > i + 100)
        },
    );
    let chunks = kitchen.chunks();
    let first = chunks
        .iter()
        .position(|c| !is_silent(c) && !is_stream(c))
        .unwrap();
    let positions = line_in_positions(&chunks[..first + 100], "the real client's line-in");
    assert!(positions.len() > 50);
    // Silence from 4 s: the client withdraws the signal 2 s later, the
    // server holds 30 s of schedule (1.5 s), then stops it and restores.
    wait_for(
        "the client is told to stop",
        Duration::from_secs(20),
        || said("source-stopped"),
    );
    assert!(
        said("source-offer source_id=1 signal=0"),
        "{:?}",
        lines.lock().unwrap()
    );
    wait_for("the room is restored", Duration::from_secs(5), || {
        room(&server.state(), "kitchen").2 == "stream"
    });
    kitchen.until_hearing("the stream again", Duration::from_secs(5), is_stream);
    let (stop, stats) = handle.stop();
    assert_eq!(stop.name(), "stopped", "{:?}", lines.lock().unwrap());
    assert_eq!(stats.starts.load(Ordering::SeqCst), 1);
    drop(hears);
    let _ = std::fs::remove_file(&source);
}
