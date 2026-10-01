//! The source role end to end: a line-in captured on a modelled capture
//! device and sent upstream over the client's real protocol v2 session, with
//! encryption on, to a scripted server peer, while the same session plays a
//! stream down through the real playout path.
//!
//! What is real: `session::open` (the handshake, `hello` with the source role,
//! `capabilities`, the first `source_offer`), `source::spawn` and its loop
//! (signal detection, the codec check, `stream_format`, `audio_chunk`,
//! `stream_end`), the shared session writer, and `receive::handshake` plus
//! `run_session` for the playback beside it (its sync exchange is what
//! publishes the offset the source stamps with).
//!
//! What is modelled: the capture device (below: a scripted signal, released
//! frame by frame by the test, stamped on a clock the test controls, with a
//! fixed capture delay and one scripted overrun), the playback device
//! (`common::ModelledDevice`), and the server (a scripted peer on the real
//! server-side handshake and records; `chorus-server` does not route a shared
//! input yet, which is goals 11 and 17). None of this is timing evidence: the
//! timestamps are checked for being computed as specified (capture instant
//! plus the sync offset), not for how well a real device keeps time.

mod common;

use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chorus_audio::MonotonicTimeline;
use chorus_client_linux::config::{ClientConfig, LineInConfig};
use chorus_client_linux::delaylog::DelayLog;
use chorus_client_linux::receive::handshake;
use chorus_client_linux::run::{header_for, run_session, StopReason};
use chorus_client_linux::session::{self, EndpointIdentity};
use chorus_client_linux::source::{
    self, CaptureError, CaptureRead, CaptureSource, SharedWriter, SignalThresholds, SourceSetup,
    SourceStop,
};
use chorus_client_linux::{Counters, ZoneWatch};
use chorus_protocol::v2::adoption::Verdict;
use chorus_protocol::v2::noise::Keypair;
use chorus_protocol::v2::session::{accept, Identity, SecureReader, SecureWriter, Translation};
use chorus_protocol::v2::{
    roles, ChannelPosition, Codec, Hello, Message, OutputDelay, SourceAction, SourceControl,
    SourceKind, StreamFormat, PROTOCOL_VERSION,
};
use chorus_protocol::{AudioChunk, SampleFormat, StreamEnd, TimeSync};

const RATE_HZ: u32 = 48_000;
const FRAME_LEN: usize = 4; // stereo pcm_s16le
const CHUNK_FRAMES: usize = 960; // 20 ms, LineInConfig's chunk at 48 kHz
const CHUNK_NS: u64 = 20_000_000;

/// The script: 1 s of silence, 3 s of tone, 3 s of silence.
const SILENCE_1: usize = 48_000;
const TONE: usize = 144_000;
const SILENCE_2: usize = 144_000;
const TOTAL: usize = SILENCE_1 + TONE + SILENCE_2;

/// The modelled capture delay (`snd_pcm_delay` on capture): 96 frames, 2 ms.
/// A model value, not a measurement of any device.
const MODEL_DELAY_FRAMES: i64 = 96;
/// The one scripted overrun: at this frame the device loses this many.
const OVERRUN_AT_FRAME: usize = 96_000;
const OVERRUN_LOST_FRAMES: usize = 480;
/// Where the modelled capture clock starts, in ns.
const CAPTURE_BASE_NS: u64 = 5_000_000_000;

/// The server timeline is this endpoint's monotonic timeline plus this.
const SERVER_OFFSET_NS: i64 = 7_000_000_000;
/// How far the playout loop's estimated offset may sit from the true one: an
/// estimate is off by at most half the round trip of the exchange it came
/// from (RFC 5905 section 4), and a loopback round trip on a busy test machine
/// stays well under 100 ms. Stated, not measured. The source's stamps are
/// held to an offset the test publishes, exactly (see the test).
const ESTIMATE_TOLERANCE_NS: i64 = 50_000_000;

const WAIT: Duration = Duration::from_secs(20);
/// The whole test fails past this, whatever it is waiting on.
const TEST_DEADLINE: Duration = Duration::from_secs(180);

fn frame_ns(frame: usize) -> u64 {
    (frame as u128 * 1_000_000_000 / u128::from(RATE_HZ)) as u64
}

/// The whole script as stereo pcm_s16le. The tone is 441 Hz at -12 dBFS peak
/// on the left and the same tone phase-shifted on the right, so the two
/// channels differ and a swapped or shifted frame shows.
fn script() -> Vec<u8> {
    let mut out = Vec::with_capacity(TOTAL * FRAME_LEN);
    for i in 0..TOTAL {
        let (l, r) = if (SILENCE_1..SILENCE_1 + TONE).contains(&i) {
            let t = (i - SILENCE_1) as f64 / f64::from(RATE_HZ);
            let w = 2.0 * std::f64::consts::PI * 441.0 * t;
            (
                (0.25 * w.sin() * 32_767.0) as i16,
                (0.25 * (w + 1.0).sin() * 32_767.0) as i16,
            )
        } else {
            (0, 0)
        };
        out.extend_from_slice(&l.to_le_bytes());
        out.extend_from_slice(&r.to_le_bytes());
    }
    out
}

/// One read the modelled device served.
#[derive(Debug, Clone, Copy)]
struct Served {
    first_frame: usize,
    captured_at_ns: u64,
}

/// A capture device that plays a script, one released frame at a time.
struct ModelledCapture {
    script: Arc<Vec<u8>>,
    pos: usize,
    released: Arc<(Mutex<usize>, Condvar)>,
    clock: Arc<AtomicU64>,
    served: Arc<Mutex<Vec<Served>>>,
    overran_once: bool,
}

impl CaptureSource for ModelledCapture {
    fn device(&self) -> &str {
        "modelled-line-in"
    }

    fn frame_len(&self) -> usize {
        FRAME_LEN
    }

    fn read(&mut self, pcm: &mut [u8]) -> Result<CaptureRead, CaptureError> {
        let n = pcm.len() / FRAME_LEN;
        let mut overran = false;
        if !self.overran_once && self.pos >= OVERRUN_AT_FRAME {
            // The ring filled while nobody read: these frames are gone, and
            // the device says so the way ALSA does, on the read.
            self.overran_once = true;
            self.pos += OVERRUN_LOST_FRAMES;
            overran = true;
        }
        if self.pos + n > TOTAL {
            return Err(CaptureError::Modelled(
                "the modelled capture's script is finished".to_string(),
            ));
        }
        let (lock, ready) = &*self.released;
        let mut released = lock.lock().unwrap();
        let deadline = Instant::now() + WAIT;
        while *released < self.pos + n {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(CaptureError::Modelled(
                    "the test released no more frames".to_string(),
                ));
            }
            released = ready.wait_timeout(released, left).unwrap().0;
        }
        drop(released);
        pcm.copy_from_slice(&self.script[self.pos * FRAME_LEN..(self.pos + n) * FRAME_LEN]);
        self.served.lock().unwrap().push(Served {
            first_frame: self.pos,
            captured_at_ns: CAPTURE_BASE_NS + frame_ns(self.pos),
        });
        self.pos += n;
        // The read returns when the next frame's digitization is the model
        // delay old: `now` is the capture clock at pos + delay.
        self.clock.store(
            CAPTURE_BASE_NS + frame_ns(self.pos + MODEL_DELAY_FRAMES as usize),
            Ordering::SeqCst,
        );
        Ok(CaptureRead {
            frames: n as u64,
            overran,
        })
    }

    fn delay_frames(&mut self) -> Result<Option<i64>, CaptureError> {
        Ok(Some(MODEL_DELAY_FRAMES))
    }
}

fn release(released: &Arc<(Mutex<usize>, Condvar)>, frames: usize) {
    let (lock, ready) = &**released;
    *lock.lock().unwrap() = frames.min(TOTAL);
    ready.notify_all();
}

/// The scripted server: the real server-side handshake and records, a paced
/// PCM stream down to the player, time-sync answers on a timeline
/// `SERVER_OFFSET_NS` ahead of the endpoint's, and every upstream message
/// handed to the test in order.
///
/// Everything it sends goes through one writer thread fed by a channel, so
/// its reader never waits on a lock held by a blocked write, and nothing the
/// test does waits on the socket.
struct ScriptedServer {
    down: Sender<Message>,
    upstream: Receiver<Message>,
    stop_playback: Arc<AtomicBool>,
    port: u16,
    ready: Receiver<()>,
}

impl ScriptedServer {
    fn start(timeline: MonotonicTimeline) -> ScriptedServer {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().unwrap().port();
        let (down, down_rx) = mpsc::channel::<Message>();
        let (upstream_tx, upstream) = mpsc::channel();
        let (ready_tx, ready) = mpsc::channel();
        let stop_playback = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&stop_playback);
        let server_now = move || (timeline.now_ns() as i64 + SERVER_OFFSET_NS) as u64;
        let send = down.clone();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("the endpoint connects");
            let me = Identity {
                id: "scripted-server".to_string(),
                keypair: Keypair::from_secret([7u8; 32]),
            };
            let established = accept(&mut stream, &me, Keypair::from_secret([9u8; 32]), |_, _| {
                Verdict::Adopted
            })
            .expect("the handshake completes");
            let mut writer = SecureWriter::new(stream.try_clone().unwrap(), established.sealer);
            thread::spawn(move || {
                while let Ok(m) = down_rx.recv() {
                    if writer.send(&m).is_err() {
                        return;
                    }
                }
            });

            // Upstream: every message in order, time-sync requests answered.
            let mut reader = SecureReader::new(stream.try_clone().unwrap(), established.opener);
            let answer = send.clone();
            reader.set_handler(Box::new(|_| {}));
            reader.set_translator(Box::new(move |m| {
                if let Message::TimeSync(t) = m {
                    let t1 = server_now();
                    let reply = TimeSync {
                        t0_ns: t.t0_ns,
                        t1_ns: t1,
                        t2_ns: server_now(),
                        t3_ns: 0,
                    };
                    let _ = answer.send(Message::TimeSync(reply));
                }
                let _ = upstream_tx.send(m.clone());
                Ok(Translation::Pass)
            }));
            thread::spawn(move || {
                let mut sink = [0u8; 16 * 1024];
                while matches!(reader.read(&mut sink), Ok(n) if n > 0) {}
            });

            // Downstream: the greeting, then a paced PCM stream.
            let _ = send.send(Message::Hello(Hello {
                protocol_version: PROTOCOL_VERSION,
                roles: 0,
                name: "scripted".to_string(),
                software: "line_in_source test".to_string(),
            }));
            let _ = send.send(Message::StreamFormat(StreamFormat {
                codec: Codec::Pcm,
                sample_format: SampleFormat::PcmS16Le,
                sample_rate_hz: RATE_HZ,
                channel_map: vec![ChannelPosition::FrontLeft, ChannelPosition::FrontRight],
                frames_per_chunk: CHUNK_FRAMES as u32,
                codec_config: Vec::new(),
            }));
            let _ = send.send(Message::OutputDelay(OutputDelay { delay_ns: 0 }));
            let _ = ready_tx.send(());
            let origin = server_now();
            let give_up = Instant::now() + 3 * WAIT;
            let mut sequence = 0u32;
            loop {
                let due = origin + u64::from(sequence) * CHUNK_NS;
                if stop.load(Ordering::SeqCst) || Instant::now() > give_up {
                    let _ = send.send(Message::StreamEnd(StreamEnd {
                        final_sequence: sequence.wrapping_sub(1),
                        end_timestamp_ns: due,
                    }));
                    return;
                }
                let now = server_now();
                if now < due {
                    thread::sleep(Duration::from_nanos((due - now).min(5_000_000)));
                    continue;
                }
                let chunk = AudioChunk {
                    sequence,
                    timestamp_ns: due,
                    sample_rate_hz: RATE_HZ,
                    channels: 2,
                    sample_format: SampleFormat::PcmS16Le,
                    reserved: [0u8; 14],
                    audio_data: vec![(sequence % 251) as u8; CHUNK_FRAMES * FRAME_LEN],
                };
                if send.send(Message::AudioChunk(chunk)).is_err() {
                    return;
                }
                sequence += 1;
            }
        });
        ScriptedServer {
            down,
            upstream,
            stop_playback,
            port,
            ready,
        }
    }

    fn control(&self, action: SourceAction, codec: Codec) {
        self.down
            .send(Message::SourceControl(SourceControl {
                source_id: 1,
                action,
                codec,
            }))
            .expect("the control goes down");
    }

    /// The next upstream message that is not a time-sync request or telemetry.
    fn next(&self) -> Message {
        let deadline = Instant::now() + WAIT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.upstream.recv_timeout(left) {
                Ok(Message::TimeSync(_)) | Ok(Message::Telemetry(_)) => continue,
                Ok(m) => return m,
                Err(RecvTimeoutError::Timeout) => panic!("no upstream message within {:?}", WAIT),
                Err(RecvTimeoutError::Disconnected) => panic!("the upstream closed"),
            }
        }
    }

    /// Release the capture a chunk at a time until a message other than an
    /// `audio_chunk` arrives, keeping the chunks. The source role reads the
    /// server's controls between chunks, so a control sent while it waits in
    /// a read takes effect after a chunk more is released; releasing one at
    /// a time is what makes the chunk it takes effect at deterministic enough
    /// to check.
    fn release_until_message(
        &self,
        released: &Arc<(Mutex<usize>, Condvar)>,
        chunks: &mut Vec<AudioChunk>,
    ) -> Message {
        let deadline = Instant::now() + WAIT;
        loop {
            assert!(Instant::now() < deadline, "no message within {:?}", WAIT);
            {
                let (lock, ready) = &**released;
                let mut r = lock.lock().unwrap();
                *r = (*r + CHUNK_FRAMES).min(TOTAL);
                ready.notify_all();
            }
            let until = Instant::now() + Duration::from_millis(20);
            loop {
                let left = until.saturating_duration_since(Instant::now());
                match self.upstream.recv_timeout(left) {
                    Ok(Message::TimeSync(_)) | Ok(Message::Telemetry(_)) => continue,
                    Ok(Message::AudioChunk(c)) => chunks.push(c),
                    Ok(m) => return m,
                    Err(RecvTimeoutError::Timeout) => break,
                    Err(RecvTimeoutError::Disconnected) => panic!("the upstream closed"),
                }
            }
        }
    }

    /// Whatever source-role messages have arrived by now, without waiting.
    fn drain(&self) -> Vec<Message> {
        self.upstream
            .try_iter()
            .filter(|m| !matches!(m, Message::TimeSync(_) | Message::Telemetry(_)))
            .collect()
    }
}

fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {}", what);
        thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn a_line_in_is_offered_started_streamed_bit_for_bit_on_the_server_timeline_and_stopped() {
    // A hard deadline on the whole test: whatever it waits on, it fails
    // rather than holding a test run (and the gate) for ever.
    let finished = Arc::new(AtomicBool::new(false));
    {
        let finished = Arc::clone(&finished);
        thread::spawn(move || {
            let deadline = Instant::now() + TEST_DEADLINE;
            while Instant::now() < deadline {
                if finished.load(Ordering::SeqCst) {
                    return;
                }
                thread::sleep(Duration::from_millis(100));
            }
            eprintln!(
                "line_in_source: the test did not finish within {:?}; failing it",
                TEST_DEADLINE
            );
            std::process::exit(101);
        });
    }
    let timeline = MonotonicTimeline::new();
    let dir = std::env::temp_dir().join(format!("chorus-line-in-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let input = LineInConfig {
        name: "Turntable".to_string(),
        ..LineInConfig::new("modelled-line-in")
    };
    let config = ClientConfig {
        delay_log: dir.join("delay.log").to_string_lossy().into_owned(),
        line_in: Some(input.clone()),
        ..ClientConfig::default()
    };
    config.validate().expect("the configuration is valid");

    let server = ScriptedServer::start(timeline);
    let port = server.port;
    let session_config = config.clone();
    let opening = thread::spawn(move || {
        let stream = TcpStream::connect(("127.0.0.1", port)).expect("the server listens");
        stream
            .set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        let mut me = EndpointIdentity::ephemeral("line-in-endpoint").unwrap();
        session::open(stream, &mut me, &session_config)
            .unwrap_or_else(|e| panic!("the session opens: {}", e))
    });
    let secure = opening.join().expect("the session thread");
    server.ready.recv_timeout(WAIT).expect("the server greeted");

    // 1. hello with the source role beside the player role, capabilities, and
    //    the first offer: no signal yet.
    let hello = match server.next() {
        Message::Hello(h) => h,
        other => panic!("expected hello, got {:?}", other),
    };
    assert_eq!(hello.roles, roles::PLAYER | roles::SOURCE, "{:?}", hello);
    let caps = match server.next() {
        Message::Capabilities(c) => c,
        other => panic!("expected capabilities, got {:?}", other),
    };
    match server.next() {
        Message::SourceOffer(o) => {
            assert_eq!(o.source_id, 1);
            assert_eq!(o.kind, SourceKind::LineIn);
            assert!(!o.signal, "nothing measured yet, so no signal");
            assert_eq!(o.name, "Turntable");
        }
        other => panic!("expected the first source_offer, got {:?}", other),
    }

    // The endpoint side: the source role on the modelled capture, and the
    // playout path on the same session.
    let session::Session {
        reader,
        writer,
        source_control,
        ..
    } = secure;
    let writer = SharedWriter::new(writer);
    let counters = Arc::new(Counters::new());
    // The source role reads its offset from a `Counters` of its own here,
    // which the test publishes to: the offset the playout loop estimates
    // moves by microseconds with every exchange, and holding the stamps to a
    // known offset is what lets them be checked exactly. The binary hands
    // both roles the same `Counters` (`main.rs`), and the playout loop's own
    // estimate is checked against the true offset at the end.
    let source_counters = Arc::new(Counters::new());
    let released = Arc::new((Mutex::new(0usize), Condvar::new()));
    let clock = Arc::new(AtomicU64::new(CAPTURE_BASE_NS));
    let served = Arc::new(Mutex::new(Vec::new()));
    let lines = Arc::new(Mutex::new(Vec::<String>::new()));
    // The session's `source_control`s reach the role through this relay,
    // which counts each one as it lands in the role's inbox. The role reads
    // its inbox between captured chunks, so the test waits for a control to
    // land before releasing the chunk it should take effect after: the
    // control rides the session behind whatever audio the playout's reader
    // has yet to take in, and on a loaded machine that can be longer than the
    // script the role would otherwise read meanwhile (the stop then found
    // the script used up, and the device's failure, not the stop, ended the
    // stream).
    let delivered = Arc::new(AtomicUsize::new(0));
    let controls = {
        let (tx, rx) = mpsc::channel();
        let delivered = Arc::clone(&delivered);
        thread::spawn(move || {
            for control in source_control {
                if tx.send(control).is_err() {
                    return;
                }
                delivered.fetch_add(1, Ordering::SeqCst);
            }
        });
        rx
    };
    let capture = ModelledCapture {
        script: Arc::new(script()),
        pos: 0,
        released: Arc::clone(&released),
        clock: Arc::clone(&clock),
        served: Arc::clone(&served),
        overran_once: false,
    };
    let handle = {
        let clock = Arc::clone(&clock);
        let lines = Arc::clone(&lines);
        source::spawn(
            capture,
            writer.clone(),
            SourceSetup {
                input: input.clone(),
                listed_codecs: caps.codecs,
                clock: Box::new(move || clock.load(Ordering::SeqCst)),
                counters: Arc::clone(&source_counters),
                controls,
                thresholds: SignalThresholds::default(),
                log: Box::new(move |l| lines.lock().unwrap().push(l.to_string())),
                tv_power: None,
            },
        )
    };
    let stats = Arc::clone(&handle.stats);
    let player = {
        let config = config.clone();
        let counters = Arc::clone(&counters);
        let sync_out = writer.clone();
        thread::spawn(move || {
            let mut reader = reader;
            let hand = handshake(&mut reader, &|| true).expect("the playback stream starts");
            let mut device = common::ModelledDevice::new("modelled", RATE_HZ, FRAME_LEN, 400_000);
            let header = header_for(&config, "modelled", &hand.shape);
            let mut log = DelayLog::open(&config.delay_log, &header).expect("the log opens");
            let outcome = run_session(
                &config,
                reader,
                hand,
                &mut device,
                &mut log,
                timeline,
                counters,
                Some(Box::new(sync_out)),
                Arc::new(ZoneWatch::new()),
            )
            .expect("the run writes its log");
            (outcome, device.underruns())
        })
    };

    // 2. A second of silence: no new offer.
    release(&released, SILENCE_1);
    wait_until("the silence to be captured", || {
        stats.frames_captured.load(Ordering::SeqCst) >= SILENCE_1 as u64
    });
    let during_silence = server.drain();
    assert!(
        during_silence.is_empty(),
        "silence changed nothing, and nothing else was sent: {:?}",
        during_silence
    );

    // 3. The tone starts: the input is offered again, with a signal.
    release(&released, SILENCE_1 + 9_600);
    match server.next() {
        Message::SourceOffer(o) => assert!(o.signal, "the tone is a signal: {:?}", o),
        other => panic!("expected an offer with a signal, got {:?}", other),
    }

    // 4. A start in a codec this endpoint cannot send is refused by name, and
    //    a start in PCM is honoured: stream_format first.
    server.control(SourceAction::Start, Codec::Flac);
    server.control(SourceAction::Start, Codec::Pcm);
    wait_until("both starts to reach the role", || {
        delivered.load(Ordering::SeqCst) == 2
    });
    let mut chunks: Vec<AudioChunk> = Vec::new();
    let format = match server.release_until_message(&released, &mut chunks) {
        Message::StreamFormat(f) => f,
        other => panic!("expected stream_format after the start, got {:?}", other),
    };
    assert_eq!(format.codec, Codec::Pcm);
    assert_eq!(format.sample_format, SampleFormat::PcmS16Le);
    assert_eq!(format.sample_rate_hz, RATE_HZ);
    assert_eq!(
        format.channel_map,
        [ChannelPosition::FrontLeft, ChannelPosition::FrontRight]
    );
    assert_eq!(format.frames_per_chunk as usize, CHUNK_FRAMES);
    assert_eq!(stats.refused_starts.load(Ordering::SeqCst), 1);

    // No offset yet: the chunks captured are held, not stamped with a guess.
    for _ in 0..5 {
        // Read the count first: a guard held across `release` would be held
        // while `release` takes the same lock.
        let now = *released.0.lock().unwrap();
        release(&released, now + CHUNK_FRAMES);
    }
    let held_to = *released.0.lock().unwrap() as u64;
    wait_until("five chunks to be captured", || {
        stats.frames_captured.load(Ordering::SeqCst) >= held_to - CHUNK_FRAMES as u64
    });
    thread::sleep(Duration::from_millis(50));
    assert!(
        server.drain().is_empty(),
        "nothing is sent while no offset is known"
    );
    assert_eq!(stats.chunks_sent.load(Ordering::SeqCst), 0);
    source_counters.offset.publish(Some(SERVER_OFFSET_NS));
    assert!(
        lines
            .lock()
            .unwrap()
            .iter()
            .any(|l| l
                .starts_with("source-start-refused source_id=1 codec=flac reason=codec-not-sent")),
        "the refusal is logged by name: {:?}",
        lines.lock().unwrap()
    );

    // 5. The rest of the tone and most of the silence after it: chunks flow,
    //    one overrun happens, and the signal goes.
    assert!(chunks.is_empty(), "nothing before stream_format");
    release(&released, TOTAL - 9_600);
    loop {
        match server.next() {
            Message::AudioChunk(c) => chunks.push(c),
            Message::SourceOffer(o) => {
                assert!(!o.signal, "the next offer withdraws the signal: {:?}", o);
                break;
            }
            other => panic!("expected chunks then an offer, got {:?}", other),
        }
    }

    // 6. Stop: stream_end.
    server.control(SourceAction::Stop, Codec::Pcm);
    wait_until("the stop to reach the role", || {
        delivered.load(Ordering::SeqCst) == 3
    });
    let end = match server.release_until_message(&released, &mut chunks) {
        Message::StreamEnd(e) => e,
        other => panic!("expected chunks then stream_end, got {:?}", other),
    };
    // The role logs the stop AFTER it has sent stream_end (the line says
    // what happened, so it follows it), so the line can trail the message
    // just read by however long a loaded CPU keeps the role's thread off it.
    wait_until(
        "the stop, not the end of the script, to end the stream",
        || {
            lines
                .lock()
                .unwrap()
                .iter()
                .any(|l| l == "source-stopped source_id=1")
        },
    );
    release(&released, TOTAL);

    // The device runs out of script, and the role says so.
    wait_until("the capture to run out of script", || {
        lines
            .lock()
            .unwrap()
            .iter()
            .any(|l| l.starts_with("source-device-failed source_id=1"))
    });
    let (stop, stats) = handle.stop();
    assert!(
        matches!(&stop, SourceStop::DeviceFailed(d) if d.contains("script is finished")),
        "{:?}",
        stop
    );
    thread::sleep(Duration::from_millis(50));
    let after = server.drain();
    assert!(
        !after.iter().any(|m| matches!(m, Message::AudioChunk(_))),
        "nothing was streamed after stream_end"
    );

    // The chunks: in order, bit for bit the captured samples, stamped at the
    // capture instant plus the offset.
    let served = served.lock().unwrap().clone();
    let script = script();
    let first = &chunks[0];
    let r0 = served
        .iter()
        .enumerate()
        .min_by_key(|(_, s)| {
            (s.captured_at_ns as i64 + SERVER_OFFSET_NS - first.timestamp_ns as i64).abs()
        })
        .map(|(i, _)| i)
        .unwrap();
    // The server started the input after the offer, which came with the
    // first full window of tone (frames 48000 to 52800); exactly which
    // chunk the start lands on depends on where the role's reading had got
    // to when the control arrived, so only that bound is fixed.
    assert!(
        served[r0].first_frame >= SILENCE_1 + 4_800,
        "the stream begins after the start, at frame {}",
        served[r0].first_frame
    );
    assert_eq!(
        served[r0].captured_at_ns,
        CAPTURE_BASE_NS + frame_ns(served[r0].first_frame)
    );
    let mut tone_chunks = 0;
    for (j, chunk) in chunks.iter().enumerate() {
        let read = served[r0 + j];
        assert_eq!(chunk.sequence, j as u32, "sequences run on without a gap");
        assert_eq!(chunk.sample_rate_hz, RATE_HZ);
        assert_eq!(chunk.channels, 2);
        assert_eq!(chunk.sample_format, SampleFormat::PcmS16Le);
        let expected =
            &script[read.first_frame * FRAME_LEN..(read.first_frame + CHUNK_FRAMES) * FRAME_LEN];
        assert!(
            chunk.audio_data == expected,
            "chunk {} is not the captured frames {}..{} bit for bit",
            j,
            read.first_frame,
            read.first_frame + CHUNK_FRAMES
        );
        if expected.iter().any(|b| *b != 0) {
            tone_chunks += 1;
        }
        assert_eq!(
            chunk.timestamp_ns as i64,
            read.captured_at_ns as i64 + SERVER_OFFSET_NS,
            "chunk {} is stamped at its capture instant plus the offset, exactly",
            j
        );
    }
    assert!(
        served[r0 + chunks.len() - 1].first_frame > SILENCE_1 + TONE + 96_000,
        "the stream ran on past the withdrawn signal until the stop"
    );
    assert!(
        tone_chunks > 100,
        "the tone went up: {} chunks",
        tone_chunks
    );
    let gap = chunks
        .windows(2)
        .zip(served[r0..].windows(2))
        .find(|(_, s)| s[1].first_frame - s[0].first_frame != CHUNK_FRAMES)
        .expect("the overrun left a gap in the capture");
    let gap_ns = gap.0[1].timestamp_ns as i64 - gap.0[0].timestamp_ns as i64;
    assert!(
        gap_ns == (CHUNK_NS + frame_ns(OVERRUN_LOST_FRAMES)) as i64,
        "the lost frames show as a gap in the timestamps: {} ns",
        gap_ns
    );
    let last = chunks.last().unwrap();
    assert_eq!(end.final_sequence, last.sequence);
    assert_eq!(end.end_timestamp_ns, last.timestamp_ns + CHUNK_NS);

    // Overruns counted and logged, never hidden; nothing dropped for want of
    // an offset; the counts agree with the wire.
    assert_eq!(stats.overruns.load(Ordering::SeqCst), 1);
    assert!(lines
        .lock()
        .unwrap()
        .iter()
        .any(|l| l.starts_with("source-overrun source_id=1") && l.contains("overruns=1")));
    assert_eq!(
        stats.chunks_sent.load(Ordering::SeqCst),
        chunks.len() as u64
    );
    assert_eq!(stats.dropped_no_offset.load(Ordering::SeqCst), 0);
    assert_eq!(stats.starts.load(Ordering::SeqCst), 1);
    assert_eq!(stats.stops.load(Ordering::SeqCst), 1);
    assert_eq!(stats.offers_sent.load(Ordering::SeqCst), 2);

    // The player kept playing through all of it, in the same session. Its
    // sync loop asks for an exchange every 500 ms and takes the reply in at
    // its next tick, discarding one whose round trip was over 100 ms
    // (`crate::sync`), so the second or so the steps above take is one
    // accepted exchange at best unloaded and none on a loaded machine (the
    // flake: "the exchange ran on the shared writer"). The stream is stopped
    // once the loop has published an offset, which it does only from an
    // accepted exchange, not after a fixed time.
    wait_until("the playout loop to accept a time-sync exchange", || {
        counters.offset.get().is_some()
    });
    server.stop_playback.store(true, Ordering::SeqCst);
    let (outcome, underruns) = player.join().expect("the player thread");
    assert!(
        matches!(outcome.stop, StopReason::EndOfStream(_)),
        "{:?}",
        outcome.stop
    );
    assert!(outcome.played_anything);
    assert!(outcome.summary.frames_played > 0);
    assert!(
        outcome.telemetry.accepted > 0,
        "the exchange ran on the shared writer"
    );
    let _ = underruns;
    let estimated = counters
        .offset
        .get()
        .expect("the playout loop published the offset it estimated");
    assert!(
        (estimated - SERVER_OFFSET_NS).abs() <= ESTIMATE_TOLERANCE_NS,
        "the playout loop estimated {} ns against a true {} ns",
        estimated,
        SERVER_OFFSET_NS
    );
    println!(
        "line-in: {} chunks upstream ({} with tone), each stamped capture instant + {} ns \
         exactly; the playout loop's own estimate was {} ns off; {}; playback frames_played={} \
         exchanges_accepted={}",
        chunks.len(),
        tone_chunks,
        SERVER_OFFSET_NS,
        estimated - SERVER_OFFSET_NS,
        stats.line(),
        outcome.summary.frames_played,
        outcome.telemetry.accepted
    );
    let _ = std::fs::remove_dir_all(&dir);
    finished.store(true, Ordering::SeqCst);
}
