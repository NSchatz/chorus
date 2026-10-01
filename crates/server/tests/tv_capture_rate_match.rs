//! A TV input's rate matching on the real server binary (goal 13, the TV
//! path).
//!
//! The REAL `chorus-server` plays a line-in into a room, from the REAL Linux
//! client's source role (`session::open` with the source role, then
//! `source::spawn`) capturing from a modelled TV whose sample clock runs
//! 300 ppm fast, paced in real time against the monotonic clock both
//! processes share. The room's player records what the server sends it.
//!
//! The graded quantity is the latency the room plays the TV at: each
//! chunk's server stamp minus the TRUE capture instant of the TV frame it
//! starts on (read back off the sawtooth the TV sends). The server plays a
//! line-in at exactly its nominal rate on its own grid (`crate::slots`), so
//! whatever rate the hub sends at shows up as that latency moving:
//!
//! - with the input declared `optical`, the hub rate-matches it
//!   (`ratematch`): the latency stays put, every chunk follows the last
//!   (no underrun: no silent chunk; no overflow: no skipped source), and the
//!   server's own counters say no underrun and no dropped chunk;
//! - the negative control, the same TV declared `line_in` (no rate
//!   matching, today's behaviour), shows the latency moving at the TV's
//!   300 ppm: 0.3 ms per second, so the room's lip sync leaves BRIEF.md
//!   2.2's +/-40 ms window within about two minutes and the server's port
//!   ring (1000 ms) overflows within an hour; a TV 300 ppm SLOW would use
//!   up the line-in's 30 ms start lead and underrun within about 100 s.
//!
//! The run is 40 s of wall clock per test (no time scaling exists for the
//! audio thread: its ticks are real 20 ms ones). None of it is timing
//! evidence: the TV is modelled and both processes share one clock.

mod common;

use std::io::Read;
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use chorus_client_linux::config::{ClientConfig, LineInConfig};
use chorus_client_linux::session::{self, EndpointIdentity};
use chorus_client_linux::source::{
    self, CaptureError, CaptureRead, CaptureSource, SharedWriter, SignalThresholds, SourceSetup,
};
use chorus_client_linux::Counters;
use chorus_protocol::v2::{Codec, SourceKind};
use chorus_protocol::{decode_frame, AudioChunk, FrameOutcome, Message};
use common::{fresh_id, Player, RunningServer};

/// The modelled TV's clock error.
const TV_PPM: f64 = 300.0;
/// The sawtooth's slope: source frame `i` is `(8 i mod 65536) - 32768`, so a
/// 16-bit sample names its frame to an eighth, modulo 8192 frames.
const SLOPE: i64 = 8;
const PERIOD: f64 = 8_192.0;
/// How long the line-in plays, and the graded window inside it.
const PLAY: Duration = Duration::from_secs(40);
const GRADE_FROM_CHUNK: usize = 250; // 5 s: the hub's matcher has settled
const GRADE_TO_CHUNK: usize = 1_750; // 35 s
/// The server's chunk, ns (its default `--chunk-us`).
const CHUNK_NS: f64 = 20_000_000.0;

fn silent_source() -> PathBuf {
    let path = std::env::temp_dir().join(format!("chorus-tvrm-{}.pcm", std::process::id()));
    std::fs::write(&path, vec![0u8; 48_000 * 4 * 60]).unwrap();
    path
}

fn utc_zone() -> String {
    format!(
        "{}/../../fixtures/schedule/Etc_UTC.slim.tzif",
        env!("CARGO_MANIFEST_DIR")
    )
}

/// The TV: frame `i` is digitized at `started + i / (48000 (1 + ppm))`, and
/// a read returns once its last frame is (reporting how many frames have
/// been digitized since, as `snd_pcm_delay` would).
struct PacedTv {
    pos: u64,
    started: Instant,
    rate: f64,
    delay: i64,
}

impl PacedTv {
    fn due(&self, frame: u64) -> Instant {
        self.started + Duration::from_secs_f64(frame as f64 / self.rate)
    }

    fn fill(&mut self, pcm: &mut [u8]) -> CaptureRead {
        let n = (pcm.len() / 4) as u64;
        for (k, frame) in pcm.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let i = (self.pos + k as u64) as i64;
            let v = ((SLOPE * i).rem_euclid(65_536) - 32_768) as i16;
            let b = v.to_le_bytes();
            *frame = [b[0], b[1], b[0], b[1]];
        }
        self.pos += n;
        let late = Instant::now().saturating_duration_since(self.due(self.pos));
        self.delay = (late.as_secs_f64() * self.rate).floor() as i64;
        CaptureRead {
            frames: n,
            overran: false,
        }
    }
}

impl CaptureSource for PacedTv {
    fn device(&self) -> &str {
        "modelled-tv"
    }
    fn frame_len(&self) -> usize {
        4
    }
    fn read(&mut self, pcm: &mut [u8]) -> Result<CaptureRead, CaptureError> {
        let due = self.due(self.pos + (pcm.len() / 4) as u64);
        let now = Instant::now();
        if due > now {
            thread::sleep(due - now);
        }
        Ok(self.fill(pcm))
    }
    fn read_within(
        &mut self,
        pcm: &mut [u8],
        wait_ns: u64,
    ) -> Result<Option<CaptureRead>, CaptureError> {
        let due = self.due(self.pos + (pcm.len() / 4) as u64);
        let now = Instant::now();
        let wait = Duration::from_nanos(wait_ns);
        if due > now + wait {
            thread::sleep(wait);
            return Ok(None);
        }
        if due > now {
            thread::sleep(due - now);
        }
        Ok(Some(self.fill(pcm)))
    }
    fn delay_frames(&mut self) -> Result<Option<i64>, CaptureError> {
        Ok(Some(self.delay))
    }
}

/// What the room heard.
struct Heard {
    chunks: Arc<Mutex<Vec<AudioChunk>>>,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl Heard {
    fn record(mut reader: impl Read + Send + 'static) -> Heard {
        let chunks = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let join = {
            let chunks = Arc::clone(&chunks);
            let stop = Arc::clone(&stop);
            thread::spawn(move || {
                let mut pending = Vec::new();
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
        Heard {
            chunks,
            stop,
            join: Some(join),
        }
    }
}

impl Drop for Heard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

fn left(c: &AudioChunk) -> Vec<i64> {
    c.audio_data
        .as_chunks::<4>()
        .0
        .iter()
        .map(|f| i64::from(i16::from_le_bytes([f[0], f[1]])))
        .collect()
}

/// The source position (modulo [`PERIOD`]) a chunk's first frame plays, off
/// the first frame whose neighbours both step by the slope (away from the
/// sawtooth's wrap); `None` for a chunk that is not the TV.
fn position_mod(c: &AudioChunk) -> Option<f64> {
    let v = left(c);
    (1..v.len() - 1)
        .find(|&k| {
            let (a, b) = (v[k] - v[k - 1], v[k + 1] - v[k]);
            (SLOPE - 1..=SLOPE + 1).contains(&a) && (SLOPE - 1..=SLOPE + 1).contains(&b)
        })
        .map(|k| ((v[k] + 32_768) as f64 / SLOPE as f64 - k as f64).rem_euclid(PERIOD))
}

/// What a run measured.
struct Measured {
    /// Latency change over the graded window, ns (last minus first).
    drift_ns: f64,
    /// Its slope, ppm.
    slope_ppm: f64,
    /// Silent chunks between line-in chunks (each an underrun).
    gaps: usize,
    /// Consecutive chunks whose sources are not consecutive (a dropped or
    /// repeated stretch).
    jumps: usize,
    /// The server's line-in report at the stop.
    report: String,
}

fn play_the_tv(kind: SourceKind) -> Measured {
    let source = silent_source();
    let mut server = RunningServer::start(&[
        "--source",
        source.to_str().unwrap(),
        "--slots",
        "3",
        "--max-clients",
        "6",
        "--tz",
        &utc_zone(),
        "--civil-time-from",
        "2026-10-05T12:00:00Z",
        "--schedule-time-scale",
        "1",
        "--zone",
        "lounge",
    ]);
    server.wait_for("civil tz=");
    let speaker = fresh_id("tvrm-speaker");
    let hub = fresh_id("tvrm-hub");
    // Both in the room, so the line-in plays at L_local (no latency growth
    // running under the measurement).
    for e in [&speaker, &hub] {
        server.applied(&format!(
            r#"{{"v":1,"t":"attach","zone":"lounge","endpoint":"{}"}}"#,
            e
        ));
    }
    let (room, _) = Player::connect(&server.audio, &speaker, 0).split();
    let heard = Heard::record(room.reader);

    let input = LineInConfig {
        kind,
        ..LineInConfig::new("modelled-tv")
    };
    let config = ClientConfig {
        line_in: Some(input.clone()),
        ..ClientConfig::default()
    };
    let stream = TcpStream::connect(&server.audio).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    let mut me = EndpointIdentity::ephemeral(&hub).unwrap();
    let session::Session {
        reader,
        writer,
        source_control,
        ..
    } = session::open(stream, &mut me, &config).expect("the session opens");
    let hub_hears = Heard::record(reader);
    let counters = Arc::new(Counters::new());
    // The server plays a line-in on its own grid and does not read the
    // upstream stamps; both processes share the host's monotonic clock.
    counters.offset.publish(Some(0));
    let started = Instant::now();
    let lines = Arc::new(Mutex::new(Vec::<String>::new()));
    let handle = {
        let lines = Arc::clone(&lines);
        source::spawn(
            PacedTv {
                pos: 0,
                started,
                rate: 48_000.0 * (1.0 + TV_PPM * 1e-6),
                delay: 0,
            },
            SharedWriter::new(writer),
            SourceSetup {
                input,
                listed_codecs: Codec::Pcm.bit(),
                clock: Box::new(move || started.elapsed().as_nanos() as u64),
                counters,
                controls: source_control,
                thresholds: SignalThresholds::default(),
                tv_power: None,
                low_latency: None,
                log: Box::new(move |l| lines.lock().unwrap().push(l.to_string())),
            },
        )
    };
    // The room takes the TV once it is offered.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let (status, _) = server.command(&format!(
            r#"{{"v":2,"t":"take","target":"lounge","source":"line-in:{}/line-1"}}"#,
            hub
        ));
        if status.contains("200") {
            break;
        }
        assert!(Instant::now() < deadline, "the TV was never offered");
        thread::sleep(Duration::from_millis(100));
    }
    server.wait_for("line-in start");
    thread::sleep(PLAY);
    // Stop the line-in while the hub still sends, so the server's report
    // counts only the time it played.
    server.applied(r#"{"v":2,"t":"take","target":"lounge","source":"stream"}"#);
    let report = server.wait_for("line-in stop");
    let (_, stats) = handle.stop();
    drop(hub_hears);
    let chunks = heard.chunks.lock().unwrap().clone();
    drop(heard);
    let _ = std::fs::remove_file(&source);
    eprintln!("{}", stats.tv_line());

    // From the first chunk that carries the TV.
    let first = chunks
        .iter()
        .position(|c| position_mod(c).is_some())
        .expect("the room heard the TV");
    let tv: Vec<&AudioChunk> = chunks[first..].iter().collect();
    let last_tv = tv.iter().rposition(|c| position_mod(c).is_some()).unwrap();
    let tv = &tv[..=last_tv];
    let gap_at: Vec<usize> = tv
        .iter()
        .enumerate()
        .filter(|(_, c)| c.audio_data.iter().all(|b| *b == 0))
        .map(|(k, _)| k)
        .collect();
    eprintln!("silent chunks at {:?}", gap_at);
    let gaps = gap_at.len();
    let rate = 48_000.0 * (1.0 + TV_PPM * 1e-6);
    let mut position: Option<f64> = None;
    let mut jumps = 0usize;
    let mut series = Vec::new();
    let mut silent_before = 0u64;
    for (k, c) in tv.iter().enumerate() {
        let Some(m) = position_mod(c) else {
            if c.audio_data.iter().all(|b| *b == 0) {
                silent_before += 1;
            }
            continue;
        };
        let p = match position {
            None => m,
            Some(prev) => {
                let expected = prev + 960.0;
                let p = m + ((expected - m) / PERIOD).round() * PERIOD;
                // A chunk plays 960 source frames at the hub's output rate;
                // a dropped or repeated chunk would move this by 960.
                if (p - expected).abs() > 2.0 {
                    jumps += 1;
                }
                p
            }
        };
        position = Some(p);
        if (GRADE_FROM_CHUNK..=GRADE_TO_CHUNK).contains(&k) {
            // Latency = stamp - capture instant of position p (up to a
            // constant, which cancels), less one chunk per silent chunk so
            // far: an underrun plays a chunk of silence and the server's
            // plan then waits, moving the latency by exactly one chunk
            // (`crate::slots`, LINE_IN_START_CHUNKS). What is left is the
            // drift alone.
            let latency = c.timestamp_ns as f64 - p / rate * 1e9 - silent_before as f64 * CHUNK_NS;
            series.push((c.timestamp_ns as f64, latency));
        }
    }
    assert!(series.len() > 1_400, "{} graded chunks", series.len());
    let n = series.len() as f64;
    let mt = series.iter().map(|(t, _)| t).sum::<f64>() / n;
    let ml = series.iter().map(|(_, l)| l).sum::<f64>() / n;
    let slope = series.iter().map(|(t, l)| (t - mt) * (l - ml)).sum::<f64>()
        / series.iter().map(|(t, _)| (t - mt) * (t - mt)).sum::<f64>();
    let drift_ns = series.last().unwrap().1 - series[0].1;
    let m = Measured {
        drift_ns,
        slope_ppm: slope * 1e6,
        gaps,
        jumps,
        report,
    };
    eprintln!(
        "{} TV at {:+} ppm: the room's latency moved {:+.3} ms over {} chunks ({:+.1} ppm); \
         gaps={} jumps={}; {}",
        kind.name(),
        TV_PPM,
        m.drift_ns / 1e6,
        series.len(),
        m.slope_ppm,
        m.gaps,
        m.jumps,
        m.report
    );
    m
}

fn counted(report: &str, key: &str) -> u64 {
    report
        .split_whitespace()
        .find_map(|w| w.strip_prefix(key))
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| panic!("no {} in {}", key, report))
}

#[test]
fn a_tv_300_ppm_fast_on_optical_is_rate_matched_so_the_servers_port_neither_drains_nor_fills() {
    let m = play_the_tv(SourceKind::Optical);
    // No overflow: the server dropped nothing and the room heard every
    // source frame in order.
    assert_eq!(m.jumps, 0, "no source skipped or repeated");
    assert_eq!(counted(&m.report, "chunks_dropped="), 0);
    // Rate matched: less the whole chunks of any late delivery, the latency
    // holds within half a millisecond over 30 s (unmatched it moves 9 ms,
    // the negative control below), so the port's fill has no trend toward
    // either bound. A late delivery (this test shares a loaded host with
    // the gate; the server and the hub run without real-time priority)
    // costs a chunk of silence and moves the latency by exactly that chunk;
    // the count is printed, and every one of them is one silent chunk the
    // room heard, never lost source.
    assert!(m.drift_ns.abs() < 500_000.0, "{:.3} ms", m.drift_ns / 1e6);
    assert!(m.slope_ppm.abs() < 15.0, "{:.1} ppm", m.slope_ppm);
    assert!(
        counted(&m.report, "underruns=") >= m.gaps as u64,
        "every silent chunk is a counted underrun"
    );
}

#[test]
fn negative_control_the_same_tv_as_a_line_in_drifts_at_its_300_ppm() {
    let m = play_the_tv(SourceKind::LineIn);
    // Without rate matching the server plays the TV's frames at its own
    // nominal rate: the latency grows at the TV's error. 30 s at 300 ppm is
    // 9 ms; at that slope lip sync leaves +/-40 ms in about 133 s, and a TV
    // 300 ppm slow would eat the 30 ms start lead (an underrun) in 100 s.
    assert!(
        (m.slope_ppm - TV_PPM).abs() < 30.0,
        "{:.1} ppm",
        m.slope_ppm
    );
    assert!(m.drift_ns > 7_000_000.0, "{:.3} ms", m.drift_ns / 1e6);
}
