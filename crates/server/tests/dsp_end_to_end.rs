//! Bass management and the two-way crossover, end to end on fakes, on the
//! Linux endpoint (goal 12, done-when line C; the C endpoint's half is
//! `firmware/tests/dsp-session.sh`).
//!
//! The real `chorus-server` with a control plane plays a known multi-tone
//! through the FIFO source path (`--source fifo:`, ADR 0050) into two rooms.
//! Four endpoints are the shipped client's own pieces: the v2 session
//! (`session::open`), the room's volume and sound delivered to a
//! [`ZoneWatch`], the handshake, the playout loop (`run_session`) and the
//! endpoint's sound chain at the sink's edge ([`DspSink`]), over a capture
//! device that keeps every frame it is handed. Three are a bonded 2.1 set in
//! the living room (FL, FR, LFE, wired); the fourth, in the study, is a
//! two-way (`--two-way crossover-hz=2000,woofer=0,tweeter=1`). Then, a step
//! at a time, with each step graded on the frames captured after it:
//!
//! 1. the mains carry the LR4 high branch and the sub the LR4 low branch of
//!    the summed mains at 80 Hz, each tone at the level the design predicts
//!    and -6.02 dB (mains) at the crossover itself; the two-way's woofer and
//!    tweeter are the LR4 split at 2 kHz and sum flat;
//! 2. a `bass_management` crossover of 120 Hz is followed;
//! 3. `sound` bass and treble move each tone by the shelves' design;
//! 4. night and then speech change the captured output exactly as the
//!    library's own chain does on the same signal;
//! 5. with the room's limit lowered under a +10 dB boost, no captured sample
//!    exceeds the limit, where the same chain without its limiter would.
//!
//! The predictions are `crates/dsp`'s own responses and chain, which the
//! shared fixtures (`fixtures/dsp`) hold to the cited designs; the LR4
//! crossover's -6.02 dB and flat sum (RaneNote 160,
//! <https://www.ranecommercial.com/legacy/note160.html>, read 2026-10-01) are
//! asserted directly as well. Nothing here is timing evidence: levels are
//! graded, never instants.

mod common;

use std::f64::consts::PI;
use std::fs::OpenOptions;
use std::io::Write;
use std::net::TcpStream;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use chorus_audio::MonotonicTimeline;
use chorus_client_linux::config::ClientConfig;
use chorus_client_linux::delaylog::DelayLog;
use chorus_client_linux::dsp::{DspSink, TwoWayOutputs};
use chorus_client_linux::receive::handshake;
use chorus_client_linux::run::{header_for, run_session};
use chorus_client_linux::session::{self, EndpointIdentity};
use chorus_client_linux::sink::{PcmSink, SinkError, SinkWrite};
use chorus_client_linux::{Counters, ZoneWatch};
use chorus_dsp::biquad::{Coefficients, Kind};
use chorus_dsp::chain::{BASS_HZ, TONE_Q, TREBLE_HZ};
use chorus_dsp::crossover::{db_of, Lr4Design};
use chorus_dsp::{Chain, EndpointDsp, SoundSettings};
use common::RunningServer;

const RATE: u32 = 48_000;
/// The test tones, Hz: every one a whole number of cycles in a
/// [`WINDOW`]-frame window, so each is read off one DFT bin with no leakage.
const TONES: [u32; 7] = [30, 80, 120, 300, 1000, 2000, 6000];
/// Each tone's amplitude in both channels (L = R), full scale 1.
const AMP: f64 = 0.1;
/// Frames graded per window: 0.5 s.
const WINDOW: usize = 24_000;

/// The source's sample `n`: every tone, in phase, at [`AMP`].
fn source(n: u64) -> f64 {
    TONES
        .iter()
        .map(|&f| AMP * (2.0 * PI * f64::from(f) * n as f64 / f64::from(RATE)).sin())
        .sum()
}

/// One stereo `pcm_s16le` frame of the source.
fn source_s16(n: u64) -> i16 {
    (source(n) * 32768.0).round() as i16
}

/// The tone's amplitude in `x` relative to [`AMP`], dB.
fn level_db(x: &[f64], f: u32) -> f64 {
    let (mut re, mut im) = (0.0, 0.0);
    for (n, v) in x.iter().enumerate() {
        let w = 2.0 * PI * f64::from(f) * n as f64 / f64::from(RATE);
        re += v * w.cos();
        im -= v * w.sin();
    }
    let amplitude = 2.0 * (re * re + im * im).sqrt() / x.len() as f64;
    20.0 * (amplitude / AMP).log10()
}

/// A device that keeps every frame it is handed and drains at the stream's
/// rate, so the playout loop paces against it as against a DAC.
struct Capture {
    frame_len: usize,
    queued: f64,
    played: u64,
    last: Instant,
    running: bool,
    tape: Arc<Mutex<Vec<i16>>>,
}

impl Capture {
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

/// One endpoint: its capture (two device channels, interleaved) and its run.
struct Endpoint {
    tape: Arc<Mutex<Vec<i16>>>,
    run: JoinHandle<()>,
}

impl Endpoint {
    /// Frames captured so far (two channels each).
    fn frames(&self) -> usize {
        self.tape.lock().unwrap().len() / 2
    }

    /// Wait until `n` frames are captured.
    fn until(&self, n: usize, what: &str) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while self.frames() < n {
            assert!(
                Instant::now() < deadline,
                "{}: {} frames not captured",
                what,
                n
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    /// One device channel of `WINDOW` frames from frame `from`, full scale 1.
    fn channel(&self, from: usize, channel: usize) -> Vec<f64> {
        self.until(from + WINDOW, "a window");
        let tape = self.tape.lock().unwrap();
        (from..from + WINDOW)
            .map(|f| f64::from(tape[f * 2 + channel]) / 32768.0)
            .collect()
    }
}

fn spawn_endpoint(audio: &str, id: &str, two_way: Option<TwoWayOutputs>, dir: &Path) -> Endpoint {
    let tape = Arc::new(Mutex::new(Vec::new()));
    let audio = audio.to_string();
    let id = id.to_string();
    let log_path = dir.join(format!("{}.delay.log", id));
    let kept = Arc::clone(&tape);
    let run = thread::spawn(move || {
        let config = ClientConfig {
            delay_log: log_path.to_str().unwrap().to_string(),
            two_way,
            ..ClientConfig::default()
        };
        config.validate().unwrap();
        let stream = TcpStream::connect(&audio).expect("the server listens");
        stream
            .set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        let mut me = EndpointIdentity::ephemeral(&id).unwrap();
        let secure = session::open(stream, &mut me, &config)
            .unwrap_or_else(|e| panic!("{}: the session opens: {}", id, e));
        // As the shipped client does (main.rs): the room's volume and sound
        // reach the watch before the stream is read.
        let watch = Arc::new(ZoneWatch::new());
        session::deliver_room_volume_to(&secure.announced, watch.room_inbox());
        session::deliver_sound_to(&secure.announced, watch.sound_inbox());
        let mut reader = secure.reader;
        let hand = handshake(&mut reader, &|| true).expect("the stream starts");
        let announced = secure.announced.lock().unwrap().clone();
        let stream_map = announced.stream_format.clone().unwrap().channel_map;
        let device = Capture {
            frame_len: 4,
            queued: 0.0,
            played: 0,
            last: Instant::now(),
            running: false,
            tape: kept,
        };
        let mut sink = DspSink::new(
            device,
            None,
            two_way.as_ref(),
            &stream_map,
            hand.shape.sample_format,
            Arc::clone(&watch),
        )
        .unwrap();
        assert!(
            sink.chain().is_some(),
            "{}: the greeting's sound engaged the chain before the first frame: {:?}",
            id,
            sink.report()
        );
        let header = header_for(&config, "capture", &hand.shape);
        let mut log = DelayLog::open(&config.delay_log, &header).unwrap();
        let _writer = secure.writer;
        // No time-sync exchange: no correction, so every captured frame is
        // one source frame through the chain. The run ends when the server
        // goes away at the end of the test.
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
    });
    Endpoint { tape, run }
}

/// The library's chain on the source signal with `settings` (a role in the
/// 2.1 set), at `gain` under `limit`: the levels of its output over the last
/// window of `seconds` of it, and its peak there.
fn predicted(settings: &SoundSettings, seconds: usize, gain: f32, limit: f32) -> (Vec<f64>, f64) {
    let mut chain = Chain::new(settings, &EndpointDsp::default(), &[1, 2], RATE).unwrap();
    let frames = seconds * RATE as usize;
    let input: Vec<f32> = (0..frames as u64)
        .flat_map(|n| {
            let v = f32::from(source_s16(n)) / 32768.0;
            [v, v]
        })
        .collect();
    let mut out = vec![0.0f32; frames * chain.out_channels()];
    chain.process(&input, &mut out, gain, limit).unwrap();
    let k = chain.out_channels();
    let tail: Vec<f64> = (frames - WINDOW..frames)
        .map(|f| f64::from(out[f * k]))
        .collect();
    let peak = tail.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    (TONES.iter().map(|&f| level_db(&tail, f)).collect(), peak)
}

fn assert_levels(what: &str, got: &[f64], want: &[f64], tolerance_db: f64) {
    for ((f, g), w) in TONES.iter().zip(got).zip(want) {
        assert!(
            (g - w).abs() <= tolerance_db,
            "{}: {} Hz at {:.3} dB, predicted {:.3} dB (tolerance {} dB)",
            what,
            f,
            g,
            w,
            tolerance_db
        );
    }
}

fn levels(x: &[f64]) -> Vec<f64> {
    TONES.iter().map(|&f| level_db(x, f)).collect()
}

/// The LR4 branches at `crossover_hz`, dB, at every tone.
fn lr4(crossover_hz: f64) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let d = Lr4Design::new(f64::from(RATE), crossover_hz).unwrap();
    let r = f64::from(RATE);
    let low = TONES
        .iter()
        .map(|&f| db_of(d.response_low(f64::from(f), r)))
        .collect();
    let high = TONES
        .iter()
        .map(|&f| db_of(d.response_high(f64::from(f), r)))
        .collect();
    let sum = TONES
        .iter()
        .map(|&f| db_of(d.response_sum(f64::from(f), r)))
        .collect();
    (low, high, sum)
}

fn tone_index(f: u32) -> usize {
    TONES.iter().position(|&t| t == f).unwrap()
}

#[test]
fn bass_management_and_the_two_way_run_end_to_end_on_the_linux_endpoint() {
    let dir = std::env::temp_dir().join(format!("chorus-dsp-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let fifo = dir.join("pcm.fifo");
    assert!(Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap()
        .success());
    let server = RunningServer::start(&[
        "--source",
        &format!("fifo:{}", fifo.display()),
        "--serve-forever",
        "--max-clients",
        "4",
        "--zone",
        "living",
        "--zone",
        "study",
    ]);

    // The player: the multi-tone, forever, paced by the server's reads.
    let playing = Arc::new(AtomicBool::new(true));
    let player = {
        let playing = Arc::clone(&playing);
        let fifo = fifo.clone();
        thread::spawn(move || {
            let mut pipe = OpenOptions::new().write(true).open(&fifo).unwrap();
            let mut n = 0u64;
            while playing.load(Ordering::SeqCst) {
                let mut chunk = Vec::with_capacity(4800 * 4);
                for _ in 0..4800 {
                    let s = source_s16(n).to_le_bytes();
                    chunk.extend_from_slice(&[s[0], s[1], s[0], s[1]]);
                    n += 1;
                }
                if pipe.write_all(&chunk).is_err() {
                    return;
                }
            }
        })
    };

    let ids: Vec<String> = ["dsp-fl", "dsp-fr", "dsp-sub", "dsp-two-way"]
        .iter()
        .map(|s| common::fresh_id(s))
        .collect();
    for (zone, id) in [
        ("living", &ids[0]),
        ("living", &ids[1]),
        ("living", &ids[2]),
        ("study", &ids[3]),
    ] {
        server.applied(&format!(
            r#"{{"v":2,"t":"attach","zone":"{}","endpoint":"{}","link":"wired"}}"#,
            zone, id
        ));
    }
    server.applied(&format!(
        r#"{{"v":2,"t":"bond","zone":"living","members":[{{"endpoint":"{}","role":"FL"}},{{"endpoint":"{}","role":"FR"}},{{"endpoint":"{}","role":"LFE"}}]}}"#,
        ids[0], ids[1], ids[2]
    ));
    // Loudness compensation is the catalog's default; at full volume it is
    // off anyway, and it is turned off so the levels below are the
    // crossover's alone.
    for zone in ["living", "study"] {
        server.applied(&format!(
            r#"{{"v":2,"t":"sound","zone":"{}","loudness":false}}"#,
            zone
        ));
    }
    let two_way = TwoWayOutputs::parse("crossover-hz=2000,woofer=0,tweeter=1").unwrap();
    let fl = spawn_endpoint(&server.audio, &ids[0], None, &dir);
    let fr = spawn_endpoint(&server.audio, &ids[1], None, &dir);
    let sub = spawn_endpoint(&server.audio, &ids[2], None, &dir);
    let tw = spawn_endpoint(&server.audio, &ids[3], Some(two_way), &dir);
    let settle = RATE as usize / 2;

    // 1. The 2.1 set at 80 Hz, and the two-way at 2 kHz.
    for e in [&fl, &fr, &sub, &tw] {
        e.until(settle, "the first half second");
    }
    let (low, high, _) = lr4(80.0);
    let from = fl.frames().max(settle);
    for (name, e) in [("FL", &fl), ("FR", &fr)] {
        for ch in 0..2 {
            let got = levels(&e.channel(from, ch));
            assert_levels(
                &format!("{} main channel {} at 80 Hz", name, ch),
                &got,
                &high,
                0.1,
            );
            assert!(
                (got[tone_index(80)] + 6.02).abs() < 0.1,
                "{}: -6.02 dB at the crossover, got {:.3}",
                name,
                got[tone_index(80)]
            );
        }
    }
    // The sub plays the low branch of FL + FR: L = R, so +6.02 dB.
    let sub_want: Vec<f64> = low.iter().map(|d| d + 20.0 * 2f64.log10()).collect();
    let sub_got = levels(&sub.channel(sub.frames().max(settle), 0));
    let sub_deep: Vec<usize> = (0..TONES.len()).filter(|&i| sub_want[i] > -60.0).collect();
    for &i in &sub_deep {
        assert!(
            (sub_got[i] - sub_want[i]).abs() < 0.1,
            "sub: {} Hz at {:.3} dB, predicted {:.3}",
            TONES[i],
            sub_got[i],
            sub_want[i]
        );
    }
    assert!(
        (sub_got[tone_index(80)] - (20.0 * 2f64.log10() - 6.02)).abs() < 0.1,
        "sub: -6.02 dB of the summed mains at the crossover"
    );
    assert!(
        sub_got[tone_index(6000)] < -60.0,
        "the sub carries no treble"
    );
    let (tw_low, tw_high, tw_sum) = lr4(2000.0);
    let at = tw.frames().max(settle);
    let woofer = tw.channel(at, 0);
    let tweeter = tw.channel(at, 1);
    let summed: Vec<f64> = woofer.iter().zip(&tweeter).map(|(w, t)| w + t).collect();
    let (wl, tl, sl) = (levels(&woofer), levels(&tweeter), levels(&summed));
    for i in 0..TONES.len() {
        if tw_low[i] > -60.0 {
            assert!((wl[i] - tw_low[i]).abs() < 0.1, "woofer {} Hz", TONES[i]);
        }
        if tw_high[i] > -60.0 {
            assert!((tl[i] - tw_high[i]).abs() < 0.1, "tweeter {} Hz", TONES[i]);
        }
        assert!(
            sl[i].abs() < 0.1 && tw_sum[i].abs() < 0.01,
            "the two-way sums flat: {} Hz at {:.3} dB",
            TONES[i],
            sl[i]
        );
    }
    let x = tone_index(2000);
    assert!((wl[x] + 6.02).abs() < 0.1 && (tl[x] + 6.02).abs() < 0.1);
    println!(
        "2.1 at 80 Hz: FL {:?} sub {:?}; two-way woofer {:?} tweeter {:?} sum {:?}",
        rounded(&levels(&fl.channel(from, 0))),
        rounded(&sub_got),
        rounded(&wl),
        rounded(&tl),
        rounded(&sl)
    );

    // Each later step: apply a command, then grade a window that starts
    // `after` frames past the frames already captured.
    let step = |command: &str, e: &Endpoint, after: usize| -> usize {
        let mark = e.frames();
        server.applied(command);
        mark + after
    };

    // 2. The crossover moves to 120 Hz.
    let from = step(
        r#"{"v":2,"t":"bass_management","zone":"living","crossover_hz":120}"#,
        &fl,
        RATE as usize / 2,
    );
    let (low, high, _) = lr4(120.0);
    let got = levels(&fl.channel(from, 0));
    assert_levels("FL at 120 Hz", &got, &high, 0.1);
    assert!((got[tone_index(120)] + 6.02).abs() < 0.1);
    let sub_from = sub.frames().max(from);
    let sub_got = levels(&sub.channel(sub_from, 0));
    let i = tone_index(120);
    assert!(
        (sub_got[i] - (low[i] + 20.0 * 2f64.log10())).abs() < 0.1,
        "the sub follows the crossover"
    );

    // 3. Bass +6 dB and treble -4 dB: the main moves by the shelves.
    let from = step(
        r#"{"v":2,"t":"sound","zone":"living","bass":6,"treble":-4}"#,
        &fl,
        RATE as usize / 2,
    );
    let r = f64::from(RATE);
    let bass = Coefficients::design(Kind::LowShelf, r, BASS_HZ, TONE_Q, 6.0).unwrap();
    let treble = Coefficients::design(Kind::HighShelf, r, TREBLE_HZ, TONE_Q, -4.0).unwrap();
    let want: Vec<f64> = TONES
        .iter()
        .zip(&high)
        .map(|(&f, h)| {
            h + bass.magnitude_db(f64::from(f), r) + treble.magnitude_db(f64::from(f), r)
        })
        .collect();
    assert_levels(
        "FL with bass +6, treble -4",
        &levels(&fl.channel(from, 0)),
        &want,
        0.1,
    );

    // 4. Night, then speech: as the library's chain does on this signal.
    let mut settings = SoundSettings {
        role: 1,
        sub_present: true,
        crossover_hz: 120,
        bass_db: 6,
        treble_db: -4,
        night: true,
        ..SoundSettings::default()
    };
    let from = step(
        r#"{"v":2,"t":"sound","zone":"living","night":true}"#,
        &fl,
        3 * RATE as usize,
    );
    let (want, _) = predicted(&settings, 6, 1.0, 1.0);
    assert_levels("FL at night", &levels(&fl.channel(from, 0)), &want, 0.2);
    settings.night = false;
    settings.speech = true;
    let from = step(
        r#"{"v":2,"t":"sound","zone":"living","night":false,"speech":true}"#,
        &fl,
        RATE as usize / 2,
    );
    let (want, _) = predicted(&settings, 2, 1.0, 1.0);
    let got = levels(&fl.channel(from, 0));
    assert_levels("FL with speech", &got, &want, 0.1);
    assert!(
        got[tone_index(2000)] > high[tone_index(2000)] + 3.0,
        "speech lifts 2 kHz"
    );

    // 5. +10 dB of bass and treble and the sub at +6 dB, under a limit of
    // 0.5: the room plays at its limit, and the limiter holds every sample
    // at or under it, where the same chain without the limit would not.
    server.applied(r#"{"v":2,"t":"sound","zone":"living","speech":false,"bass":10,"treble":10}"#);
    server.applied(r#"{"v":2,"t":"bass_management","zone":"living","sub_level_db":6.00}"#);
    let from = step(
        r#"{"v":2,"t":"limit","zone":"living","limit":0.5}"#,
        &fl,
        RATE as usize,
    );
    let sub_settings = SoundSettings {
        role: 4,
        speech: false,
        bass_db: 10,
        treble_db: 10,
        sub_level_cdb: 600,
        ..settings
    };
    let (_, unlimited) = predicted(&sub_settings, 2, 0.5, 2.0);
    assert!(
        unlimited > 0.6,
        "the boosted sub would exceed the limit: {:.3}",
        unlimited
    );
    for (name, e) in [("FL", &fl), ("FR", &fr), ("sub", &sub)] {
        let start = e.frames().max(from);
        for ch in 0..2 {
            let x = e.channel(start, ch);
            let peak = x.iter().fold(0.0f64, |m, v| m.max(v.abs()));
            assert!(
                peak <= 0.5,
                "{} channel {}: a sample at {:.5} of full scale, above the limit 0.5",
                name,
                ch,
                peak
            );
            if name == "sub" && ch == 0 {
                assert!(peak > 0.45, "the room plays up to its limit: {:.4}", peak);
                println!(
                    "limit 0.5, bass and treble +10, sub +6: sub peak {:.4}; without the room limit {:.4}",
                    peak, unlimited
                );
            }
        }
    }

    playing.store(false, Ordering::SeqCst);
    drop(server);
    for e in [fl, fr, sub, tw] {
        let _ = e.run.join();
    }
    let _ = player.join();
    let _ = std::fs::remove_dir_all(&dir);
}

fn rounded(v: &[f64]) -> Vec<f64> {
    v.iter().map(|x| (x * 100.0).round() / 100.0).collect()
}
