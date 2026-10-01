//! The TV path end to end (goal 13): a TV played in low-latency mode on the
//! real binaries' code, from a modelled TV on a hub to a theater set.
//!
//! The REAL `chorus-server` (stream slots, the schedule runtime, a control
//! plane, the TV relay `chorus_server::tvrelay`) serves a theater room. Every
//! endpoint is the shipped Linux client's own code: its protocol v2 session
//! (`session::open`), its playout loop (`run_session`, with the time-sync
//! exchange running, so each endpoint's offset onto the server timeline is
//! measured as in a deployment), its low-latency player
//! (`chorus_client_linux::lowlat`) and its sound chain at the sink's edge
//! (`DspSink`), over a modelled device that drains at 48 kHz in real time and
//! keeps every frame it is handed. The hub is one more such endpoint (a
//! player in a room of its own) whose source role (`source::spawn`) captures
//! a modelled TV in real time: L is 1 kHz and R 1.5 kHz, both with 50 Hz,
//! each tone at 0.2 of full scale.
//!
//! What is checked, each with a `summary` line to paste in a report:
//!
//! - (a) loss: `--udp-loss` drops datagrams on both UDP legs (seeded, at
//!   1e-3 and 1e-2); with FEC (k 4) the relay and the players rebuild what
//!   was lost and almost nothing is unrecoverable; with FEC off (the
//!   negative control, `--fec-k 0`) the same loss is chunks lost;
//! - (b) the theater set hears the 2.0 TV map: FL the left, FR the right, FC
//!   (L + R) / sqrt 2 (each tone 3.01 dB under the fronts), SL and SR silent
//!   (`tv_upmix` off, the default), the LFE the bass of both (50 Hz) and none
//!   of the mid tones (bass management);
//! - (c) every chunk a player receives is stamped its capture stamp plus
//!   `L_tv` (20 ms); an A/V trim of +50 ms moves the lead to 70 ms, and one
//!   of -100 ms clamps it at the plan's floor with `av-trim-clamped`;
//! - (d) autoplay on the fake CEC bus: the TV's power-on plays the theater
//!   room in low-latency mode, out of its group; the standby stops it and
//!   restores the room;
//! - (e) a target that is grouped falls back to the slot path, by name.
//!
//! Nothing here is timing evidence (BRIEF.md 3.1 rule 3): every process
//! shares one host clock, the TV and the devices are modelled, and the
//! leads graded are the stamps the relay wrote, not instants anything was
//! heard.

mod common;

use std::f64::consts::PI;
use std::net::{IpAddr, Ipv4Addr, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use chorus_audio::MonotonicTimeline;
use chorus_cec::codec::PhysicalAddress;
use chorus_cec::{FakeBus, FakeTv, TvKind, TvSignal};
use chorus_client_linux::cec::{CecConfig, CecRole};
use chorus_client_linux::config::{ClientConfig, LineInConfig};
use chorus_client_linux::delaylog::DelayLog;
use chorus_client_linux::dsp::DspSink;
use chorus_client_linux::lowlat::LowLatPlayer;
use chorus_client_linux::receive::handshake;
use chorus_client_linux::run::{header_for, run_session};
use chorus_client_linux::session::{self, EndpointIdentity};
use chorus_client_linux::sink::{PcmSink, SinkError, SinkWrite};
use chorus_client_linux::source::{
    self, CaptureError, CaptureRead, CaptureSource, LowLatUplink, SharedWriter, SignalThresholds,
    SourceHandle, SourceSetup,
};
use chorus_client_linux::{Counters, ZoneWatch};
use chorus_control::json::{self, Value};
use chorus_protocol::v2::lowlat::DEFAULTS;
use chorus_protocol::v2::{roles, Codec, SourceKind};
use common::{fresh_id, RunningServer};

const RATE: u32 = 48_000;
/// The TV's tones, Hz: L 1 kHz, R 1.5 kHz, both 50 Hz. Each a whole number
/// of cycles in a [`WINDOW`].
const L_HZ: u32 = 1_000;
const R_HZ: u32 = 1_500;
const BASS_HZ: u32 = 50;
/// Each tone's amplitude, full scale 1.
const AMP: f64 = 0.2;
/// Frames graded per window: 0.5 s.
const WINDOW: usize = 24_000;
/// The chunk on this path, ns (2.5 ms).
const CHUNK_NS: u64 = 2_500_000;
const LOCALHOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

fn silent_source() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "chorus-tvll-{}-{}.pcm",
        std::process::id(),
        fresh_id("s")
    ));
    std::fs::write(&path, vec![0u8; RATE as usize * 4 * 120]).unwrap();
    path
}

fn utc_zone() -> String {
    format!(
        "{}/../../fixtures/schedule/Etc_UTC.slim.tzif",
        env!("CARGO_MANIFEST_DIR")
    )
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

// --- the modelled TV --------------------------------------------------------

/// The TV: stereo s16 at exactly 48 kHz against the host's monotonic clock
/// (a TV's own drift is ADR 0090's test, not this one), playing its tones
/// while `on`, digital silence otherwise.
struct ToneTv {
    pos: u64,
    started: Instant,
    on: Arc<AtomicBool>,
    /// Frames digitized since the last one a read returned (a late read
    /// says so, as `snd_pcm_delay` would, so the hub's DLL sees the true
    /// capture instants on a loaded host).
    delay: i64,
}

fn tone(hz: u32, n: u64) -> f64 {
    AMP * (2.0 * PI * f64::from(hz) * n as f64 / f64::from(RATE)).sin()
}

impl ToneTv {
    fn due(&self, frame: u64) -> Instant {
        self.started + Duration::from_nanos(frame * 1_000_000_000 / u64::from(RATE))
    }

    fn fill(&mut self, pcm: &mut [u8]) -> CaptureRead {
        let n = (pcm.len() / 4) as u64;
        let on = self.on.load(Ordering::Relaxed);
        for (k, frame) in pcm.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let i = self.pos + k as u64;
            let (l, r) = if on {
                (
                    tone(L_HZ, i) + tone(BASS_HZ, i),
                    tone(R_HZ, i) + tone(BASS_HZ, i),
                )
            } else {
                (0.0, 0.0)
            };
            let l = ((l * 32768.0).round() as i16).to_le_bytes();
            let r = ((r * 32768.0).round() as i16).to_le_bytes();
            *frame = [l[0], l[1], r[0], r[1]];
        }
        self.pos += n;
        let late = Instant::now().saturating_duration_since(self.due(self.pos));
        self.delay = (late.as_secs_f64() * f64::from(RATE)).floor() as i64;
        CaptureRead {
            frames: n,
            overran: false,
        }
    }
}

impl CaptureSource for ToneTv {
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

// --- the modelled device ----------------------------------------------------

/// A device that keeps every frame it is handed and drains at 48 kHz in real
/// time, so the playout loop paces against it as against a DAC.
struct Capture {
    frame_len: usize,
    queued: f64,
    played: u64,
    last: Instant,
    running: bool,
    tape: Arc<Mutex<Vec<i16>>>,
}

impl Capture {
    fn new(tape: Arc<Mutex<Vec<i16>>>) -> Capture {
        Capture {
            frame_len: 4,
            queued: 0.0,
            played: 0,
            last: Instant::now(),
            running: false,
            tape,
        }
    }

    fn tick(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last).as_secs_f64();
        self.last = now;
        if self.running {
            let n = (elapsed * f64::from(RATE)).min(self.queued);
            self.queued -= n;
            self.played += n as u64;
        }
    }
}

impl PcmSink for Capture {
    fn device(&self) -> &str {
        "capture"
    }
    fn frame_len(&self) -> usize {
        self.frame_len
    }
    fn rate_hz(&self) -> u32 {
        RATE
    }
    fn write(&mut self, pcm: &[u8]) -> Result<SinkWrite, SinkError> {
        self.tick();
        self.running = true;
        self.tape.lock().unwrap().extend(
            pcm.as_chunks::<2>()
                .0
                .iter()
                .map(|s| i16::from_le_bytes(*s)),
        );
        let frames = (pcm.len() / self.frame_len) as f64;
        while self.queued + frames > f64::from(RATE) * 0.4 {
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
        Ok(())
    }
    fn frames_played(&mut self) -> Result<u64, SinkError> {
        self.tick();
        Ok(self.played)
    }
}

// --- the endpoints ----------------------------------------------------------

/// What a player's low-latency path received: (sequence, stamp, recovered).
type Received = Arc<Mutex<Vec<(u32, u64, bool)>>>;

/// One Linux endpoint: its device's tape, its low-latency player, and what
/// that player received.
struct Endpoint {
    id: String,
    tape: Arc<Mutex<Vec<i16>>>,
    ll: Arc<Mutex<Option<Arc<LowLatPlayer>>>>,
    received: Received,
    _run: JoinHandle<()>,
}

impl Endpoint {
    fn player(&self) -> Arc<LowLatPlayer> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(p) = self.ll.lock().unwrap().clone() {
                return p;
            }
            assert!(Instant::now() < deadline, "{}: no session", self.id);
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn frames(&self) -> usize {
        self.tape.lock().unwrap().len() / 2
    }

    /// Device channel 0 of the last [`WINDOW`] frames, full scale 1.
    fn last_window(&self) -> Vec<f64> {
        let tape = self.tape.lock().unwrap();
        let frames = tape.len() / 2;
        assert!(frames > WINDOW, "{}: {} frames captured", self.id, frames);
        (frames - WINDOW..frames)
            .map(|f| f64::from(tape[f * 2]) / 32768.0)
            .collect()
    }
}

/// The low-latency device target the players pace to here, us. ASSUMED for
/// a loaded test host: the gate runs these beside every other test on a few
/// cores, where a 1 ms wakeup is not kept; the rooms' `L_tv` is raised to
/// [`TEST_L_TV_MS`] to leave room for it.
const TEST_TARGET_US: u64 = 30_000;
/// `--test-tv-latency-ms` (the server's test-only `L_tv` override) for the
/// tests that grade what is heard. On the shared 4-core host these ran on, a
/// chunk reached the relay 30 to 47 ms after its capture (the relay's
/// `age_mean_us`/`age_max_us`, the hub's capture thread waiting to be
/// scheduled; not timing evidence), so the plan's 20 ms (or its 40 ms
/// ceiling) would make most chunks late and grade the host, not the path.
/// 110 ms leaves that room and still lets a -100 ms trim reach the floor.
const TEST_L_TV_MS: &str = "110";

fn client_config(id: &str, delay_dir: &std::path::Path) -> ClientConfig {
    ClientConfig {
        endpoint: id.to_string(),
        low_latency_target_us: TEST_TARGET_US,
        delay_log: delay_dir
            .join(format!("{}.delay.log", id))
            .to_str()
            .unwrap()
            .to_string(),
        ..ClientConfig::default()
    }
}

/// A player: the shipped client's session, playout loop, sync exchange,
/// low-latency player and sound chain, on a modelled device.
fn spawn_player(audio: &str, id: &str, dir: &std::path::Path) -> Endpoint {
    let tape = Arc::new(Mutex::new(Vec::new()));
    let ll = Arc::new(Mutex::new(None));
    let received: Received = Arc::new(Mutex::new(Vec::new()));
    let audio = audio.to_string();
    let config = client_config(id, dir);
    let run = {
        let tape = Arc::clone(&tape);
        let ll = Arc::clone(&ll);
        let received = Arc::clone(&received);
        let id = id.to_string();
        thread::spawn(move || {
            let stream = TcpStream::connect(&audio).expect("the server listens");
            stream
                .set_read_timeout(Some(Duration::from_millis(200)))
                .unwrap();
            let mut me = EndpointIdentity::ephemeral(&id).unwrap();
            let secure = session::open(stream, &mut me, &config)
                .unwrap_or_else(|e| panic!("{}: the session opens: {}", id, e));
            let watch = Arc::new(ZoneWatch::new());
            session::deliver_room_volume_to(&secure.announced, watch.room_inbox());
            session::deliver_sound_to(&secure.announced, watch.sound_inbox());
            let writer = SharedWriter::new(secure.writer);
            let player = LowLatPlayer::new(
                secure.low_latency_player,
                Box::new(writer.clone()),
                LOCALHOST,
                &config,
                Box::new(|_| {}),
            );
            {
                let received = Arc::clone(&received);
                player.set_tap(Box::new(move |seq, stamp, _pcm, recovered| {
                    received.lock().unwrap().push((seq, stamp, recovered));
                }));
            }
            watch.set_low_latency(Arc::clone(&player));
            *ll.lock().unwrap() = Some(player);
            let mut reader = secure.reader;
            let hand = handshake(&mut reader, &|| true).expect("the stream starts");
            let announced = secure.announced.lock().unwrap().clone();
            let stream_map = announced.stream_format.clone().unwrap().channel_map;
            let mut sink = DspSink::new(
                Capture::new(tape),
                None,
                None,
                &stream_map,
                hand.shape.sample_format,
                Arc::clone(&watch),
            )
            .unwrap();
            let header = header_for(&config, "capture", &hand.shape);
            let mut log = DelayLog::open(&config.delay_log, &header).unwrap();
            let _ = run_session(
                &config,
                reader,
                hand,
                &mut sink,
                &mut log,
                MonotonicTimeline::new(),
                Arc::new(Counters::new()),
                Some(Box::new(writer)),
                watch,
            );
        })
    };
    Endpoint {
        id: id.to_string(),
        tape,
        ll,
        received,
        _run: run,
    }
}

/// The hub: a player in a room of its own (its playout loop's exchange is
/// what maps its capture instants onto the server timeline) and the source
/// role on the modelled TV, with or without CEC.
struct Hub {
    lines: Arc<Mutex<Vec<String>>>,
    source: Option<SourceHandle>,
    _run: JoinHandle<()>,
}

impl Hub {
    fn said(&self, what: &str) -> bool {
        self.lines.lock().unwrap().iter().any(|l| l.contains(what))
    }
}

fn spawn_hub(
    audio: &str,
    id: &str,
    dir: &std::path::Path,
    on: Arc<AtomicBool>,
    cec: Option<&CecRole>,
) -> Hub {
    let input = LineInConfig {
        name: "tv".to_string(),
        kind: SourceKind::Optical,
        ..LineInConfig::new("modelled-tv")
    };
    let config = ClientConfig {
        line_in: Some(input.clone()),
        extra_roles: if cec.is_some() { roles::CONTROLLER } else { 0 },
        cec: cec.map(|_| CecConfig::new("fake-cec0")),
        ..client_config(id, dir)
    };
    let stream = TcpStream::connect(audio).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    let mut me = EndpointIdentity::ephemeral(id).unwrap();
    let secure = session::open(stream, &mut me, &config).expect("the hub's session opens");
    let session::Session {
        mut reader,
        writer,
        announced,
        source_control,
        low_latency_source,
        ..
    } = secure;
    if let Some(cec) = cec {
        session::also_hand(&mut reader, &announced, cec.server_messages());
    }
    let writer = SharedWriter::new(writer);
    if let Some(cec) = cec {
        let mut uplink = writer.clone();
        cec.connect(Box::new(move |m| source::Upstream::send(&mut uplink, m)));
    }
    let counters = Arc::new(Counters::new());
    let timeline = MonotonicTimeline::new();
    let lines = Arc::new(Mutex::new(Vec::<String>::new()));
    let source = {
        let lines = Arc::clone(&lines);
        source::spawn(
            ToneTv {
                pos: 0,
                started: Instant::now(),
                on,
                delay: 0,
            },
            writer.clone(),
            SourceSetup {
                input,
                listed_codecs: Codec::Pcm.bit(),
                clock: Box::new(move || timeline.now_ns()),
                counters: Arc::clone(&counters),
                controls: source_control,
                thresholds: SignalThresholds::default(),
                log: Box::new(move |l| lines.lock().unwrap().push(l.to_string())),
                tv_power: cec.map(|c| TvSignal::new(c.tv_power(), true)),
                low_latency: Some(LowLatUplink {
                    offers: low_latency_source,
                    server_ip: LOCALHOST,
                }),
            },
        )
    };
    let run = {
        let config = config.clone();
        thread::spawn(move || {
            let hand = handshake(&mut reader, &|| true).expect("the hub's stream starts");
            let header = header_for(&config, "capture", &hand.shape);
            let mut log = DelayLog::open(&config.delay_log, &header).unwrap();
            let mut sink = Capture::new(Arc::new(Mutex::new(Vec::new())));
            let _ = run_session(
                &config,
                reader,
                hand,
                &mut sink,
                &mut log,
                timeline,
                counters,
                Some(Box::new(writer)),
                Arc::new(ZoneWatch::new()),
            );
        })
    };
    Hub {
        lines,
        source: Some(source),
        _run: run,
    }
}

impl Drop for Hub {
    fn drop(&mut self) {
        if let Some(s) = self.source.take() {
            let (_, stats) = s.stop();
            println!("hub: {}", stats.line());
        }
        if thread::panicking() {
            for l in self.lines.lock().unwrap().iter() {
                eprintln!("hub said: {}", l);
            }
        }
    }
}

// --- grading ----------------------------------------------------------------

/// The level of `hz` in `x`, dB relative to [`AMP`] (one DFT bin).
fn level_db(x: &[f64], hz: u32) -> f64 {
    let (mut re, mut im) = (0.0, 0.0);
    for (n, v) in x.iter().enumerate() {
        let w = 2.0 * PI * f64::from(hz) * n as f64 / f64::from(RATE);
        re += v * w.cos();
        im -= v * w.sin();
    }
    let amplitude = 2.0 * (re * re + im * im).sqrt() / x.len() as f64;
    20.0 * (amplitude / AMP).max(1e-12).log10()
}

fn rms(x: &[f64]) -> f64 {
    (x.iter().map(|v| v * v).sum::<f64>() / x.len() as f64).sqrt()
}

/// The `key=` value on a line.
fn counted(line: &str, key: &str) -> u64 {
    line.split_whitespace()
        .find_map(|w| w.strip_prefix(key))
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| panic!("no {} in {}", key, line))
}

/// The leads a player saw, ns: each received chunk's stamp minus the
/// capture stamp of its sequence, on the hub's grid (the hub stamps chunk
/// `n` of a lock `S0 + n x 2.5 ms`, ADR 0090) as the relay said it: from its
/// first chunk (`tv-relay first-chunk`) and each move of the grid a hub's
/// relock makes (`tv-relay capture-grid`), the latest at or before each
/// sequence.
fn leads(received: &[(u32, u64, bool)], server: &RunningServer) -> Vec<i64> {
    let grids: Vec<(i64, i64)> = server
        .seen
        .iter()
        .filter(|l| l.contains("tv-relay first-chunk") || l.contains("tv-relay capture-grid"))
        .map(|l| {
            (
                counted(l, "sequence=") as i64,
                counted(l, "capture_ns=") as i64,
            )
        })
        .collect();
    received
        .iter()
        .map(|&(seq, stamp, _)| {
            let seq = i64::from(seq);
            let (seq0, cap0) = grids
                .iter()
                .rev()
                .find(|(s, _)| *s <= seq)
                .copied()
                .unwrap_or(grids[0]);
            stamp as i64 - (cap0 + (seq - seq0) * CHUNK_NS as i64)
        })
        .collect()
}

/// A server for these tests: three slots, the schedule in real time, the
/// rooms named, and `extra`.
fn server(extra: &[&str]) -> (RunningServer, PathBuf) {
    let source = silent_source();
    let mut args = vec![
        "--source",
        source.to_str().unwrap(),
        "--slots",
        "3",
        "--max-clients",
        "10",
        "--zone",
        "theater",
        "--zone",
        "den",
        "--zone",
        "hubroom",
    ];
    let tz = utc_zone();
    args.extend([
        "--tz",
        tz.as_str(),
        "--civil-time-from",
        "2026-10-05T20:00:00Z",
        "--schedule-time-scale",
        "1",
    ]);
    args.extend(extra);
    let mut s = RunningServer::start(&args);
    s.wait_for("low-latency listening port=");
    (s, source)
}

fn attach_and_bond(server: &RunningServer, players: &[(&str, &str)], hub: &str) {
    for (id, _) in players {
        server.applied(&format!(
            r#"{{"v":2,"t":"attach","zone":"theater","endpoint":"{}","link":"wired"}}"#,
            id
        ));
    }
    server.applied(&format!(
        r#"{{"v":2,"t":"attach","zone":"hubroom","endpoint":"{}","link":"wired"}}"#,
        hub
    ));
    let members: Vec<String> = players
        .iter()
        .map(|(id, role)| format!(r#"{{"endpoint":"{}","role":"{}"}}"#, id, role))
        .collect();
    server.applied(&format!(
        r#"{{"v":2,"t":"bond","zone":"theater","members":[{}]}}"#,
        members.join(",")
    ));
    server.applied(r#"{"v":2,"t":"sound","zone":"theater","loudness":false}"#);
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("chorus-tvll-{}", fresh_id(name)));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Every player in low-latency mode, with at least `played` chunks played
/// and `received` received.
fn until_low_latency(players: &[&Endpoint], played: u64, received: usize) {
    let ready = |p: &&Endpoint| {
        let s = p.player().stats();
        p.player().active() && s.played >= played && p.received.lock().unwrap().len() >= received
    };
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline && !players.iter().all(ready) {
        thread::sleep(Duration::from_millis(50));
    }
    if !players.iter().all(ready) {
        for p in players {
            eprintln!("{}: {}", p.id, p.player().status_line());
        }
    }
    wait_for(
        "every player plays the low-latency stream",
        Duration::from_secs(20),
        || players.iter().all(ready),
    );
}

// --- the tests --------------------------------------------------------------

#[test]
fn a_theater_set_plays_the_tv_in_low_latency_mode_with_its_maps_its_lead_and_its_trim() {
    let dir = scratch("theater");
    let (mut server, source) =
        server(&["--udp-loss", "1000,7", "--test-tv-latency-ms", TEST_L_TV_MS]);
    let roles = ["FL", "FR", "FC", "LFE", "SL", "SR"];
    let ids: Vec<String> = roles
        .iter()
        .map(|r| fresh_id(&format!("tv-{}", r.to_lowercase())))
        .collect();
    let hub_id = fresh_id("tv-hub");
    let pairs: Vec<(&str, &str)> = ids
        .iter()
        .zip(roles)
        .map(|(i, r)| (i.as_str(), r))
        .collect();
    attach_and_bond(&server, &pairs, &hub_id);
    let players: Vec<Endpoint> = ids
        .iter()
        .map(|id| spawn_player(&server.audio, id, &dir))
        .collect();
    // The rule first: autoplay starts on the signal's edge.
    server.applied(&format!(
        r#"{{"v":2,"t":"autoplay","input":"{}/tv","target":"theater","enabled":true}}"#,
        hub_id
    ));
    let on = Arc::new(AtomicBool::new(true));
    let hub = spawn_hub(&server.audio, &hub_id, &dir, Arc::clone(&on), None);

    // The TV's signal starts the autoplay; the room is one wired room whose
    // every player and hub can take the path.
    let mode = server.wait_for("tv-path mode=low-latency");
    assert!(mode.contains("room=theater"), "{mode}");
    let active = server.wait_for("tv-relay active");
    assert!(active.contains("players=6"), "{active}");
    let first = server.wait_for("tv-relay first-chunk");
    let all: Vec<&Endpoint> = players.iter().collect();
    until_low_latency(&all, 400, 400);
    assert!(hub.said("source-low-latency start"), "the hub switched");
    thread::sleep(Duration::from_secs(3));

    // (b) The maps, on the last half second each device played.
    let window: Vec<Vec<f64>> = players.iter().map(|p| p.last_window()).collect();
    let fl = &window[0];
    let reference = level_db(fl, L_HZ);
    let rel = |x: &[f64], hz: u32| level_db(x, hz) - reference;
    let mut map_lines = Vec::new();
    for (role, x) in roles.iter().zip(&window) {
        map_lines.push(format!(
            "{} 1kHz={:+.2}dB 1.5kHz={:+.2}dB 50Hz={:+.2}dB rms={:.5}",
            role,
            rel(x, L_HZ),
            rel(x, R_HZ),
            rel(x, BASS_HZ),
            rms(x)
        ));
    }
    for l in &map_lines {
        println!("map {}", l);
    }
    assert!(
        reference > -12.0,
        "FL plays the left at {:.2} dB",
        reference
    );
    assert!(rel(fl, R_HZ) < -40.0, "FL has none of the right");
    let fr = &window[1];
    assert!(rel(fr, R_HZ).abs() < 0.5, "FR plays the right");
    assert!(rel(fr, L_HZ) < -40.0, "FR has none of the left");
    let fc = &window[2];
    for hz in [L_HZ, R_HZ] {
        let d = rel(fc, hz) + 3.0103;
        assert!(
            d.abs() < 0.5,
            "FC plays (L + R) / sqrt 2: {} Hz off by {:.2} dB",
            hz,
            d
        );
    }
    let lfe = &window[3];
    assert!(rel(lfe, BASS_HZ) > -12.0, "the sub plays the bass");
    assert!(
        rel(lfe, L_HZ) < -30.0 && rel(lfe, R_HZ) < -30.0,
        "and no mid"
    );
    for x in &window[4..6] {
        assert!(rms(x) < 1e-3, "a surround is silent with tv_upmix off");
    }

    // (c) The lead: every chunk each player received is its capture stamp
    // plus L_tv.
    let at_trim = |p: &Endpoint| p.received.lock().unwrap().len();
    server.drain();
    let before: Vec<i64> = leads(&players[0].received.lock().unwrap(), &server);
    assert!(before.len() > 1_000);
    let l_tv: i64 = TEST_L_TV_MS.parse::<i64>().unwrap() * 1_000_000;
    assert!(
        before.iter().all(|&l| l == l_tv),
        "leads {:?}",
        &before[..5]
    );
    let mark = at_trim(&players[0]);
    // (a) at 1e-3 on both legs, before any trim: what the players rebuilt
    // and lost, and the sequence gaps one of them saw.
    let stats: Vec<_> = players.iter().map(|p| p.player().stats()).collect();
    let recovered: u64 = stats.iter().map(|s| s.recovered).sum();
    let unrecoverable: u64 = stats.iter().map(|s| s.unrecoverable).sum();
    let late: u64 = stats.iter().map(|s| s.late).sum();
    let played: u64 = stats.iter().map(|s| s.played).sum();
    let missing = {
        let r = players[0].received.lock().unwrap();
        let seqs: std::collections::BTreeSet<u32> = r.iter().map(|x| x.0).collect();
        let span = seqs.last().unwrap() - seqs.first().unwrap() + 1;
        span as usize - seqs.len()
    };
    for p in &players {
        println!("{}: {}", p.id, p.player().status_line());
    }
    // Once trimmed to the floor (19.4 ms) the test's 30 ms device target
    // cannot be met: every chunk is late from then on, by design of the
    // test, and nothing below grades what is heard after that trim.
    server.applied(r#"{"v":2,"t":"av_trim","zone":"theater","av_trim_ms":50}"#);
    server.wait_for("tv-relay lead input=");
    let mut plus = 0i64;
    wait_for(
        "a chunk at the trimmed lead",
        Duration::from_secs(10),
        || {
            server.drain();
            let r = players[0].received.lock().unwrap();
            plus = leads(&r[mark..], &server).last().copied().unwrap_or(0);
            plus == l_tv + 50_000_000
        },
    );
    let mark2 = at_trim(&players[0]);
    // Every lead seen between the two trims is the old one or the new one.
    let between: Vec<i64> = leads(&players[0].received.lock().unwrap()[mark..mark2], &server);
    assert!(between.iter().all(|&l| l == l_tv || l == l_tv + 50_000_000));
    server.applied(r#"{"v":2,"t":"av_trim","zone":"theater","av_trim_ms":-100}"#);
    let clamped = server.wait_for("av-trim-clamped");
    // The floor's lead is below what the test's device target can play, so
    // these chunks are late at the players; the tap sees every datagram
    // received all the same.
    let mut floor = 0i64;
    wait_for(
        "a chunk at the floor's lead",
        Duration::from_secs(10),
        || {
            server.drain();
            let r = players[0].received.lock().unwrap();
            floor = leads(&r[mark2..], &server).last().copied().unwrap_or(0);
            floor as u64 == DEFAULTS.floor_ns()
        },
    );
    assert!(
        clamped.contains(&format!("lead_ns={}", DEFAULTS.floor_ns())),
        "{clamped}"
    );

    // (e) The theater joins the den's group: a group plays the TV on the
    // slot path, by name, and the players leave the low-latency stream.
    server.applied(r#"{"v":2,"t":"join","zone":"den","target":"theater"}"#);
    let slot = server.wait_for("tv-path mode=slot");
    assert!(slot.contains("reason=grouped"), "{slot}");
    let end = server.wait_for("tv-relay end");
    wait_for(
        "the players leave the stream",
        Duration::from_secs(5),
        || players.iter().all(|p| !p.player().active()),
    );
    let tcp_from = players[0].frames();
    wait_for("the slot path plays on", Duration::from_secs(10), || {
        players[0].frames() > tcp_from + RATE as usize
    });

    println!("relay: {}", first);
    println!("relay: {}", end);
    println!(
        "summary (b) maps: {} (FL reference {:+.2} dB)",
        map_lines.join("; "),
        reference
    );
    println!(
        "summary (c) lead: L_tv {} ms on {} chunks; +50 ms trim -> {} ms; -100 ms trim -> {} ms \
         (floor {} ms, av-trim-clamped)",
        l_tv as f64 / 1e6,
        before.len(),
        plus as f64 / 1e6,
        floor as f64 / 1e6,
        DEFAULTS.floor_ns() as f64 / 1e6,
    );
    println!(
        "summary (a) 1e-3 both legs, 6 players, before the trims: players played={} \
         recovered={} unrecoverable={} late={}, sequences missing at FL {}; relay over the run \
         recovered={} unrecoverable={}",
        played,
        recovered,
        unrecoverable,
        late,
        missing,
        counted(&end, "recovered="),
        counted(&end, "unrecoverable="),
    );
    assert!(unrecoverable <= played / 100, "FEC kept the loss under 1%");
    println!("summary (e) grouped: {}", slot);
    drop(hub);
    drop(server);
    let _ = std::fs::remove_file(&source);
}

/// One loss run: two players (FL, FR), `secs` seconds of the TV, and the
/// relay's and the players' counts.
fn loss_run(ppm: u32, fec_k: u8, secs: u64) -> (String, u64, u64, u64, u64) {
    let dir = scratch("loss");
    let loss = format!("{},{}", ppm, 11 + u64::from(fec_k));
    let k = fec_k.to_string();
    let (mut server, source) = server(&[
        "--udp-loss",
        &loss,
        "--fec-k",
        &k,
        "--test-tv-latency-ms",
        TEST_L_TV_MS,
    ]);
    let ids = [fresh_id("loss-fl"), fresh_id("loss-fr")];
    let hub_id = fresh_id("loss-hub");
    attach_and_bond(
        &server,
        &[(ids[0].as_str(), "FL"), (ids[1].as_str(), "FR")],
        &hub_id,
    );
    let players: Vec<Endpoint> = ids
        .iter()
        .map(|id| spawn_player(&server.audio, id, &dir))
        .collect();
    server.applied(&format!(
        r#"{{"v":2,"t":"autoplay","input":"{}/tv","target":"theater","enabled":true}}"#,
        hub_id
    ));
    let hub = spawn_hub(
        &server.audio,
        &hub_id,
        &dir,
        Arc::new(AtomicBool::new(true)),
        None,
    );
    server.wait_for("tv-relay active");
    let all: Vec<&Endpoint> = players.iter().collect();
    until_low_latency(&all, 400, 400);
    thread::sleep(Duration::from_secs(secs));
    // End it, so the relay says its counts.
    server.applied(r#"{"v":2,"t":"take","target":"theater","source":"stream"}"#);
    let end = server.wait_for("tv-relay end");
    let stats: Vec<_> = players.iter().map(|p| p.player().stats()).collect();
    let recovered = stats.iter().map(|s| s.recovered).sum();
    let unrecoverable = stats.iter().map(|s| s.unrecoverable).sum();
    let delivered = stats.iter().map(|s| s.delivered).sum();
    for p in &players {
        println!("{}: {}", p.id, p.player().status_line());
    }
    drop(hub);
    drop(server);
    let _ = std::fs::remove_file(&source);
    (end, recovered, unrecoverable, delivered, stats.len() as u64)
}

#[test]
fn loss_on_both_legs_is_repaired_by_the_fec_and_is_lost_without_it() {
    let mut lines = Vec::new();
    let mut by_case = Vec::new();
    for (ppm, k) in [(1_000u32, 4u8), (10_000, 4), (10_000, 0)] {
        let (end, recovered, unrecoverable, delivered, _) = loss_run(ppm, k, 6);
        let up_rec = counted(&end, "recovered=");
        let up_lost = counted(&end, "unrecoverable=");
        let dropped = counted(&end, "dropped_up=") + counted(&end, "dropped_down=");
        let line = format!(
            "summary (a) loss={:.0e} fec_k={}: datagrams dropped={} upstream recovered={} \
             unrecoverable={}; players delivered={} recovered={} unrecoverable={}",
            f64::from(ppm) / 1e6,
            k,
            dropped,
            up_rec,
            up_lost,
            delivered,
            recovered,
            unrecoverable
        );
        println!("{}", line);
        lines.push(line);
        by_case.push((ppm, k, dropped, up_rec + recovered, up_lost + unrecoverable));
    }
    let (_, _, dropped_on, rec_on, lost_on) = by_case[1];
    let (_, _, dropped_off, rec_off, lost_off) = by_case[2];
    assert!(dropped_on > 0 && dropped_off > 0, "loss was injected");
    assert!(rec_on > 0, "the FEC rebuilt chunks at 1e-2");
    assert_eq!(rec_off, 0, "nothing is rebuilt without FEC");
    assert!(lost_off > 0, "without FEC the loss is chunks lost");
    assert!(
        lost_on * 5 < lost_off,
        "FEC leaves {} lost against {} without",
        lost_on,
        lost_off
    );
    let (_, _, _, rec_low, lost_low) = by_case[0];
    assert!(rec_low > 0 || lost_low == 0);
    drop(lines);
}

fn group_of(state: &str, zone: &str) -> (String, String) {
    let value = json::parse(state).unwrap_or_else(|e| panic!("{}: {:?}", state, e));
    let Some(Value::Arr(zones)) = value.get("zones") else {
        panic!("no zones in {}", state)
    };
    let group = zones
        .iter()
        .find(|z| z.get("id").and_then(Value::as_str) == Some(zone))
        .and_then(|z| z.get("group").and_then(Value::as_str))
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
    (group, source)
}

#[test]
fn tv_power_on_the_cec_bus_plays_the_theater_in_low_latency_mode_and_standby_restores_it() {
    let dir = scratch("cec");
    let (mut server, source) = server(&[]);
    let ids = [fresh_id("cec-fl"), fresh_id("cec-fr")];
    let hub_id = fresh_id("cec-hub");
    attach_and_bond(
        &server,
        &[(ids[0].as_str(), "FL"), (ids[1].as_str(), "FR")],
        &hub_id,
    );
    // The theater starts in the den's group.
    server.applied(r#"{"v":2,"t":"join","zone":"theater","target":"den"}"#);
    let grouped = group_of(&server.state(), "theater");
    assert_eq!(grouped.0, group_of(&server.state(), "den").0);

    let bus = FakeBus::new();
    let tv = FakeTv::start(&bus, TvKind::RokuLike);
    let cec_lines = Arc::new(Mutex::new(Vec::<String>::new()));
    let log: Arc<dyn Fn(&str) + Send + Sync> = {
        let l = Arc::clone(&cec_lines);
        Arc::new(move |s: &str| l.lock().unwrap().push(s.to_string()))
    };
    let t0 = Instant::now();
    let opener = bus.clone();
    let cec = CecRole::start(
        move || Ok(opener.adapter(PhysicalAddress(0x1000))),
        &CecConfig::new("fake-cec0"),
        Arc::new(move || t0.elapsed().as_millis() as u64),
        log,
    );
    wait_for("the hub claims address 5", Duration::from_secs(5), || {
        cec_lines
            .lock()
            .unwrap()
            .iter()
            .any(|l| l.contains("cec claimed logical_address=5"))
    });
    let players: Vec<Endpoint> = ids
        .iter()
        .map(|id| spawn_player(&server.audio, id, &dir))
        .collect();
    server.applied(&format!(
        r#"{{"v":2,"t":"autoplay","input":"{}/tv","target":"theater","enabled":true}}"#,
        hub_id
    ));
    let on = Arc::new(AtomicBool::new(false));
    let hub = spawn_hub(&server.audio, &hub_id, &dir, Arc::clone(&on), Some(&cec));
    thread::sleep(Duration::from_millis(500));
    assert!(!server.seen.iter().any(|l| l.contains("tv-relay offer")));

    // The TV turns on (and its picture's sound with it).
    on.store(true, Ordering::SeqCst);
    tv.power_on(TvKind::RokuLike);
    wait_for("the hub offers the TV", Duration::from_secs(5), || {
        hub.said("source-offer source_id=1 signal=1")
    });
    let mode = server.wait_for("tv-path mode=low-latency");
    assert!(mode.contains("room=theater"), "{mode}");
    server.wait_for("tv-relay active");
    let all: Vec<&Endpoint> = players.iter().collect();
    // The default L_tv here (20 ms): what is checked is the mode and the
    // stamps, not what a loaded host manages to play in time (with the
    // test's 30 ms device target most of it is late, by design; the chunks
    // received before the play position exists are stamped all the same).
    until_low_latency(&all, 0, 20);
    server.wait_for("tv-relay first-chunk");
    server.drain();
    let lead = leads(&players[0].received.lock().unwrap(), &server);
    assert!(
        lead.iter().all(|&l| l == DEFAULTS.l_tv_ns as i64),
        "the default lead: {:?}",
        &lead[..5]
    );
    let playing = group_of(&server.state(), "theater");
    assert_ne!(
        playing.0,
        group_of(&server.state(), "den").0,
        "out of its group"
    );
    assert_eq!(playing.1, format!("line-in:{}/tv", hub_id));

    // Standby: the signal ends at once, the autoplay stops, the relay ends
    // and the room is restored.
    tv.standby();
    on.store(false, Ordering::SeqCst);
    wait_for("the standby is offered", Duration::from_secs(5), || {
        hub.said("signal=0 reason=standby")
    });
    let stopped = server.wait_for("stopped reason=standby");
    let end = server.wait_for("tv-relay end");
    wait_for(
        "the players leave the stream",
        Duration::from_secs(5),
        || players.iter().all(|p| !p.player().active()),
    );
    wait_for("the theater is restored", Duration::from_secs(5), || {
        group_of(&server.state(), "theater").1 != format!("line-in:{}/tv", hub_id)
    });
    println!(
        "summary (c) default lead: {} chunks received at L_tv {} ms",
        lead.len(),
        DEFAULTS.l_tv_ns as f64 / 1e6
    );
    println!("summary (d) cec: {} | {} | {}", mode, stopped, end);
    cec.disconnect();
    cec.stop();
    drop(hub);
    drop(server);
    let _ = std::fs::remove_file(&source);
}
