//! The scripted line-in endpoint and the listeners the schedule's
//! end-to-end tests share (`alarms_sleep_autoplay.rs`, `line_in_sharing.rs`).
//!
//! The players are protocol v2 sessions opened through the Linux client's own
//! session code ([`Recorder`]). The line-in ([`LineIn`]) is a scripted source
//! endpoint on the same session code: it declares the source role, offers
//! `line-1`, and on `source_control` start sends `stream_format` and
//! real-time 20 ms chunks of a known pattern, a triangle wave whose every
//! sample names the source frame it is ([`pattern`]), so what a room receives
//! can be read back as the source positions it plays.

use std::io::Read;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use chorus_control::json::{self, Value};
use chorus_protocol::v2::{
    roles, ChannelPosition, Codec, Message as V2Message, RoomVolume, SourceAction, SourceKind,
    SourceOffer, StreamFormat,
};
use chorus_protocol::{
    decode_frame, AudioChunk, FrameOutcome, Message, SampleFormat, StreamEnd, RESERVED_LEN,
};

use super::{Player, RunningServer};

pub const RATE_HZ: u32 = 48_000;
pub const FRAMES: usize = 960;
/// The configured stream: every sample this value, so it is told apart from
/// a chime, a line-in and silence by every byte.
pub const SAMPLE: i16 = 0x1234;
/// Half the line-in pattern's period, in frames.
pub const HALF: u64 = 30_000;

pub fn constant_source(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("chorus-asa-{}-{}.pcm", name, std::process::id()));
    let bytes: Vec<u8> = std::iter::repeat_n(SAMPLE.to_le_bytes(), RATE_HZ as usize * 2 * 40)
        .flatten()
        .collect();
    std::fs::write(&path, bytes).unwrap();
    path
}

pub fn utc_zone() -> String {
    format!(
        "{}/../../fixtures/schedule/Etc_UTC.slim.tzif",
        env!("CARGO_MANIFEST_DIR")
    )
}

/// The server, its clock at `from` (a Monday, 2026-10-05) run `scale` times
/// faster, serving `zones`.
pub fn server(source: &Path, from: &str, scale: &str, zones: &[&str]) -> RunningServer {
    server_with_slots(source, from, scale, zones, "3")
}

/// [`server`] with `slots` stream slots.
pub fn server_with_slots(
    source: &Path,
    from: &str,
    scale: &str,
    zones: &[&str],
    slots: &str,
) -> RunningServer {
    let mut args: Vec<String> = [
        "--source",
        source.to_str().unwrap(),
        "--slots",
        slots,
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

pub fn samples(c: &AudioChunk) -> Vec<i16> {
    c.audio_data
        .as_chunks::<2>()
        .0
        .iter()
        .map(|s| i16::from_le_bytes(*s))
        .collect()
}

pub fn is_stream(c: &AudioChunk) -> bool {
    samples(c).iter().all(|s| *s == SAMPLE)
}

pub fn is_silent(c: &AudioChunk) -> bool {
    c.audio_data.iter().all(|b| *b == 0)
}

/// Source frame `i` of the line-in: a triangle from 2000 to 32000 and back,
/// so every sample is nonzero and, away from the two corners, names its frame.
pub fn pattern(i: u64) -> i16 {
    let m = i % (2 * HALF);
    let t = if m < HALF { m } else { 2 * HALF - m };
    (2_000 + t) as i16
}

/// The source position a chunk's first frame plays, read off its first
/// sample of channel 0 and, for the slope, its fifth frame's (a line-in's
/// first chunk holds its first frame for two more); `None` near a corner,
/// where the slope is ambiguous. `near` is where it is expected, to pick the
/// right period.
pub fn position(c: &AudioChunk, near: Option<f64>) -> Option<f64> {
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
pub fn room(state: &str, id: &str) -> (u32, u32, String) {
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

pub fn ringing(state: &str, alarm: &str) -> bool {
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

pub fn wait_for(what: &str, limit: Duration, mut done: impl FnMut() -> bool) {
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
pub struct Recorder {
    pub chunks: Arc<Mutex<Vec<AudioChunk>>>,
    pub messages: Arc<Mutex<Vec<V2Message>>>,
    pub stop: Arc<AtomicBool>,
    pub join: Option<JoinHandle<()>>,
}

impl Recorder {
    pub fn player(audio: &str, endpoint: &str) -> Recorder {
        let player = Player::connect(audio, endpoint, 0);
        let (session, messages) = player.split();
        Recorder::reading(session.reader, messages)
    }

    pub fn reading(
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

    pub fn chunks(&self) -> Vec<AudioChunk> {
        self.chunks.lock().unwrap().clone()
    }

    pub fn count(&self) -> usize {
        self.chunks.lock().unwrap().len()
    }

    pub fn room_volumes(&self) -> Vec<RoomVolume> {
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
    pub fn until_hearing(&self, what: &str, limit: Duration, f: impl Fn(&AudioChunk) -> bool) {
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
pub struct LineIn {
    /// Its player half.
    pub hears: Recorder,
    pub signal: Arc<AtomicBool>,
    pub starts: Arc<AtomicU64>,
    pub stops: Arc<AtomicU64>,
    /// The latest this endpoint's own 20 ms pacing ever woke, in ms: a
    /// machine too busy to run the scripted source in real time shows here,
    /// and a test that grades continuity asks before it does.
    pub late_ms: Arc<AtomicU64>,
    pub stop: Arc<AtomicBool>,
    pub join: Option<JoinHandle<()>>,
}

impl LineIn {
    pub fn start(audio: &str, endpoint: &str, signal: bool) -> LineIn {
        let player = Player::connect(audio, endpoint, roles::SOURCE);
        let (session, messages) = player.split();
        let mut writer = session.writer;
        let controls = session.source_control;
        let hears = Recorder::reading(session.reader, messages);
        let signal = Arc::new(AtomicBool::new(signal));
        let starts = Arc::new(AtomicU64::new(0));
        let stops = Arc::new(AtomicU64::new(0));
        let late_ms = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let join = {
            let (signal, starts, stops, stop, late_ms) = (
                Arc::clone(&signal),
                Arc::clone(&starts),
                Arc::clone(&stops),
                Arc::clone(&stop),
                Arc::clone(&late_ms),
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
                    } else {
                        late_ms.fetch_max((now - next).as_millis() as u64, Ordering::SeqCst);
                    }
                }
            })
        };
        LineIn {
            hears,
            signal,
            starts,
            stops,
            late_ms,
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
pub fn rise_from_zero(rvs: &[RoomVolume], from: usize) -> Vec<RoomVolume> {
    let at = rvs[from..]
        .iter()
        .position(|r| r.gain == 0)
        .map(|i| i + from)
        .unwrap_or_else(|| panic!("no gain 0 after the fire: {:?}", rvs));
    rvs[at..].to_vec()
}

pub fn assert_rises_in_steps(rise: &[RoomVolume], target: u16, what: &str) {
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
pub fn line_in_positions(chunks: &[AudioChunk], what: &str) -> Vec<(u32, f64)> {
    positions_within(chunks, what, false)
}

/// [`line_in_positions`], and with `shrinking` the steps a plan that is
/// coming back down makes too (more than a chunk of source per chunk, by at
/// most the same 500 ppm).
pub fn positions_within(chunks: &[AudioChunk], what: &str, shrinking: bool) -> Vec<(u32, f64)> {
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
                let most = if shrinking {
                    FRAMES as f64 * (1.0 + 6e-4) + 1.5
                } else {
                    FRAMES as f64 + 1.5
                };
                assert!(
                    (FRAMES as f64 * (1.0 - 6e-4) - 1.5..=most).contains(&per),
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
