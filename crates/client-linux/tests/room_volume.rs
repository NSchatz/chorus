//! The room's volume on the audio wire, enforced by the Linux client (goal 11,
//! K81, I10; `docs/decisions/0074-room-volume-on-the-audio-wire.md`).
//!
//! Read off the samples a modelled sink ACCEPTED, as `zone_apply.rs` reads the
//! zone gain: what is written at every frame is the least of the ramped
//! `room_volume` gain, its limit, this endpoint's `--max-volume` ceiling and
//! the control plane's zone gain, and a gain never changes how many frames
//! are written.
//!
//! Two halves. The first drives `run_session` with a `ZoneWatch` whose inbox
//! already holds a `room_volume` (the way the session delivers it). The
//! second is end to end over the client's real protocol v2 session, with
//! encryption on, against a scripted server peer (the server's own sending of
//! `room_volume` is the integration track's): `session::open`,
//! `session::deliver_room_volume_to`, `receive::handshake` and `run_session`,
//! with the server sending the committed vector's fields
//! (`fixtures/protocol/v2/room_volume_above_limit`, gain 900 over limit 600).
//!
//! Nothing here is timing evidence: the sinks are modelled, and what is
//! graded is the CONTENT and the COUNT of the frames.

mod common;

use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chorus_audio::MonotonicTimeline;
use chorus_client_linux::config::ClientConfig;
use chorus_client_linux::control::ZoneWatch;
use chorus_client_linux::delaylog::DelayLog;
use chorus_client_linux::receive::{handshake, Handshake, StreamShape};
use chorus_client_linux::run::{fresh_receiver, header_for, run_session};
use chorus_client_linux::session::{self, EndpointIdentity};
use chorus_client_linux::sink::{PcmSink, SinkError, SinkWrite};
use chorus_client_linux::Counters;
use chorus_control::catalog::Volume;
use chorus_protocol::v2::adoption::Verdict;
use chorus_protocol::v2::noise::Keypair;
use chorus_protocol::v2::session::{accept, Identity, SecureReader, SecureWriter, Translation};
use chorus_protocol::v2::{
    ChannelPosition, Codec, Hello, Message, RoomVolume, StreamFormat, PROTOCOL_VERSION,
};
use chorus_protocol::{AudioChunk, SampleFormat, StreamEnd, RESERVED_LEN};

use common::{end_frame, PacedReader};

const RATE_HZ: u32 = 48_000;
const FRAME_LEN: usize = 4;
const FRAMES_PER_CHUNK: usize = 960;
const CHUNKS: u32 = 40;
const SOURCE: i16 = 12_000;

/// A modelled device that keeps every byte it was handed, draining at the
/// nominal rate so the playout loop paces its writes (`zone_apply.rs`'s).
struct RecordingSink {
    accepted: Arc<Mutex<Vec<u8>>>,
    queued: f64,
    played: u64,
    last_tick: Instant,
}

impl RecordingSink {
    fn new() -> RecordingSink {
        RecordingSink {
            accepted: Arc::new(Mutex::new(Vec::new())),
            queued: 0.0,
            played: 0,
            last_tick: Instant::now(),
        }
    }

    fn tape(&self) -> Arc<Mutex<Vec<u8>>> {
        Arc::clone(&self.accepted)
    }

    fn tick(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_tick).as_secs_f64();
        self.last_tick = now;
        let consumed = (elapsed * f64::from(RATE_HZ)).min(self.queued).max(0.0);
        self.queued -= consumed;
        self.played += consumed as u64;
    }
}

impl PcmSink for RecordingSink {
    fn device(&self) -> &str {
        "modelled-recording"
    }

    fn frame_len(&self) -> usize {
        FRAME_LEN
    }

    fn rate_hz(&self) -> u32 {
        RATE_HZ
    }

    fn write(&mut self, pcm: &[u8]) -> Result<SinkWrite, SinkError> {
        self.tick();
        self.accepted
            .lock()
            .expect("the tape")
            .extend_from_slice(pcm);
        let frames = (pcm.len() / FRAME_LEN) as u64;
        self.queued += frames as f64;
        Ok(SinkWrite {
            frames_written: frames,
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
        self.tick();
        self.played += self.queued as u64;
        self.queued = 0.0;
        Ok(())
    }

    fn frames_played(&mut self) -> Result<u64, SinkError> {
        self.tick();
        Ok(self.played)
    }
}

fn temp_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "chorus-room-volume-{}-{}-{}.log",
        name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0)
    ))
}

fn a_config(log: &std::path::Path) -> ClientConfig {
    ClientConfig {
        server: "unused".to_string(),
        device: "modelled-recording".to_string(),
        delay_log: log.to_string_lossy().into_owned(),
        run_seconds: Some(4),
        ..Default::default()
    }
}

fn shape() -> StreamShape {
    StreamShape {
        sample_rate_hz: RATE_HZ,
        channels: 2,
        sample_format: SampleFormat::PcmS16Le,
        frames_per_chunk: FRAMES_PER_CHUNK as u64,
    }
}

fn pcm(value: i16) -> Vec<u8> {
    std::iter::repeat_n(value.to_le_bytes(), FRAMES_PER_CHUNK * 2)
        .flatten()
        .collect()
}

fn chunk(sequence: u32) -> AudioChunk {
    AudioChunk {
        sequence,
        timestamp_ns: u64::from(sequence) * 20_000_000,
        sample_rate_hz: RATE_HZ,
        channels: 2,
        sample_format: SampleFormat::PcmS16Le,
        reserved: [0u8; RESERVED_LEN],
        audio_data: pcm(SOURCE),
    }
}

/// `CHUNKS` constant-valued chunks, paced one chunk duration apart, then an
/// end: every accepted sample has one expected value.
fn a_stream() -> (Handshake, PacedReader) {
    let mut parts: Vec<(Duration, Vec<u8>)> = Vec::new();
    for sequence in 0..CHUNKS {
        parts.push((
            Duration::from_millis(20 * u64::from(sequence)),
            chorus_protocol::encode(&chorus_protocol::Message::AudioChunk(chunk(sequence)))
                .expect("the chunk encodes"),
        ));
    }
    parts.push((
        Duration::from_millis(20 * u64::from(CHUNKS)),
        end_frame(CHUNKS - 1, u64::from(CHUNKS) * 20_000_000),
    ));
    let handshake = Handshake {
        receiver: fresh_receiver(),
        shape: shape(),
        buffered: Vec::new(),
    };
    (handshake, PacedReader::new(parts, true))
}

fn samples(bytes: &[u8]) -> Vec<i16> {
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect()
}

/// One run against a recording sink with `watch` in force; every sample the
/// device accepted.
fn accepted_with(watch: Arc<ZoneWatch>) -> Vec<i16> {
    let path = temp_path("apply");
    let config = a_config(&path);
    let (hand, source) = a_stream();
    let mut sink = RecordingSink::new();
    let tape = sink.tape();
    let header = header_for(&config, "modelled-recording", &hand.shape);
    let mut log = DelayLog::open(&path, &header).expect("the log opens");
    run_session(
        &config,
        source,
        hand,
        &mut sink,
        &mut log,
        MonotonicTimeline::new(),
        Arc::new(Counters::new()),
        None,
        watch,
    )
    .expect("the run writes its log");
    drop(log);
    let _ = std::fs::remove_file(&path);
    let bytes = tape.lock().expect("the tape").clone();
    samples(&bytes)
}

fn volume(literal: &str) -> Volume {
    Volume::parse(literal).expect("a volume")
}

/// What the client writes for a sample at a settled gain: the zone gain's
/// own integer scaling (`zone.rs`), truncated toward zero.
fn at(thousandths: i64) -> i16 {
    (i64::from(SOURCE) * thousandths / 1000) as i16
}

fn watch_with(ceiling: &str, message: Option<RoomVolume>) -> Arc<ZoneWatch> {
    let watch = Arc::new(ZoneWatch::with_max_volume(volume(ceiling)));
    if let Some(m) = message {
        watch.room_inbox().deliver(m);
    }
    watch
}

fn room(gain: u16, limit: u16, ramp_ms: u16) -> RoomVolume {
    RoomVolume {
        gain,
        limit,
        ramp_ms,
    }
}

fn state(volume: &str, muted: bool) -> String {
    format!(
        r#"{{"v":1,"t":"state","serial":1,"zones":[{{"id":"kitchen","name":"Kitchen","group":"g","volume":{},"muted":{},"endpoints":["a"],"present":["a"],"audio":"127.0.0.1:4010"}}]}}"#,
        volume, muted
    )
}

fn assert_all(accepted: &[i16], expected: i16, what: &str) {
    assert_eq!(
        accepted.len(),
        CHUNKS as usize * FRAMES_PER_CHUNK * 2,
        "{}: frames in == frames out ({} samples accepted)",
        what,
        accepted.len()
    );
    assert!(
        accepted.iter().all(|s| *s == expected),
        "{}: every sample is {}, first {:?}",
        what,
        expected,
        &accepted[..8.min(accepted.len())]
    );
}

#[test]
fn with_no_room_volume_and_the_default_ceiling_nothing_changes() {
    let accepted = accepted_with(watch_with("1.000", None));
    assert_all(&accepted, SOURCE, "no room_volume, max_volume 1.000");
}

#[test]
fn a_gain_above_the_limit_plays_at_the_limit() {
    let accepted = accepted_with(watch_with("1.000", Some(room(900, 600, 0))));
    assert_all(&accepted, at(600), "gain 900 over limit 600");
    let accepted = accepted_with(watch_with("1.000", Some(room(400, 750, 0))));
    assert_all(&accepted, at(400), "gain 400 under limit 750");
}

#[test]
fn nothing_on_the_wire_raises_the_endpoints_own_ceiling() {
    let accepted = accepted_with(watch_with("0.250", None));
    assert_all(
        &accepted,
        at(250),
        "max_volume 0.250 before any room_volume",
    );
    let accepted = accepted_with(watch_with("0.250", Some(room(1000, 1000, 0))));
    assert_all(
        &accepted,
        at(250),
        "max_volume 0.250 under gain 1000 limit 1000",
    );
}

#[test]
fn the_most_restrictive_of_the_control_plane_and_the_audio_wire_wins() {
    // The control plane's state says 0.300; the wire says 0.800 under 1.000.
    let watch = watch_with("1.000", Some(room(800, 1000, 0)));
    assert!(watch.absorb(&state("0.300", false), "kitchen"));
    assert_all(&accepted_with(watch), at(300), "state 0.300, wire 0.800");
    // The state says 0.900; the wire's limit is 0.400.
    let watch = watch_with("1.000", Some(room(900, 400, 0)));
    assert!(watch.absorb(&state("0.900", false), "kitchen"));
    assert_all(
        &accepted_with(watch),
        at(400),
        "state 0.900, wire limit 0.400",
    );
    // A state that carries the room's effective limit (the v2 state) bounds
    // the gain by it too.
    let watch = watch_with("1.000", None);
    let v2 = state("0.900", false).replace(
        r#""muted":false"#,
        r#""muted":false,"effective_limit":0.200"#,
    );
    assert!(watch.absorb(&v2, "kitchen"));
    assert_eq!(watch.gain(), volume("0.200"));
    assert_all(
        &accepted_with(watch),
        at(200),
        "state 0.900 under effective_limit 0.200",
    );
    // And a mute on either path is silence, with every frame still written.
    let watch = watch_with("1.000", Some(room(1000, 1000, 0)));
    assert!(watch.absorb(&state("0.500", true), "kitchen"));
    assert_all(&accepted_with(watch), 0, "muted on the control plane");
    assert_all(
        &accepted_with(watch_with("1.000", Some(room(0, 1000, 0)))),
        0,
        "muted on the wire (gain 0)",
    );
}

#[test]
fn a_fade_is_linear_monotone_ends_on_its_target_and_writes_every_frame() {
    // From unity to silence over 200 ms (9600 frames) of an 800 ms stream.
    let accepted = accepted_with(watch_with("1.000", Some(room(0, 1000, 200))));
    assert_eq!(
        accepted.len(),
        CHUNKS as usize * FRAMES_PER_CHUNK * 2,
        "frames in == frames out under a ramp"
    );
    let left: Vec<i16> = accepted.iter().step_by(2).copied().collect();
    assert_eq!(
        left[0], SOURCE,
        "the first frame plays at the gain it started from"
    );
    assert!(
        left.windows(2).all(|w| w[1] <= w[0]),
        "the fade never rises"
    );
    let ramp_frames = 200 * RATE_HZ as usize / 1000;
    // Frame k of N plays at trunc(SOURCE * g_k / 65536), g_k = 65536 - 65536 k / N.
    for (k, s) in left.iter().enumerate().take(ramp_frames) {
        let g = 65_536 - (65_536u64 * k as u64 / ramp_frames as u64) as i64;
        assert_eq!(
            i64::from(*s),
            i64::from(SOURCE) * g / 65_536,
            "frame {} of the ramp",
            k
        );
    }
    assert!(
        left[ramp_frames..].iter().all(|s| *s == 0),
        "silence from the end of the ramp on"
    );
    // And up: from silence to 0.500 over 100 ms, under a limit of 0.400.
    let watch = watch_with("1.000", Some(room(0, 1000, 0)));
    let path = temp_path("up");
    let config = a_config(&path);
    let (hand, source) = a_stream();
    let mut sink = RecordingSink::new();
    let tape = sink.tape();
    let header = header_for(&config, "modelled-recording", &hand.shape);
    let mut log = DelayLog::open(&path, &header).expect("the log opens");
    {
        // The mute is taken by the priming write; the ramp up arrives after.
        let watch = Arc::clone(&watch);
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(250));
            watch.room_inbox().deliver(room(500, 400, 100));
        });
    }
    run_session(
        &config,
        source,
        hand,
        &mut sink,
        &mut log,
        MonotonicTimeline::new(),
        Arc::new(Counters::new()),
        None,
        Arc::clone(&watch),
    )
    .expect("the run writes its log");
    drop(log);
    let _ = std::fs::remove_file(&path);
    let accepted = samples(&tape.lock().unwrap());
    assert_eq!(accepted.len(), CHUNKS as usize * FRAMES_PER_CHUNK * 2);
    let left: Vec<i16> = accepted.iter().step_by(2).copied().collect();
    assert!(
        left.windows(2).all(|w| w[1] >= w[0]),
        "the ramp up never falls"
    );
    assert_eq!(left[0], 0, "it starts muted");
    assert_eq!(
        *left.last().unwrap(),
        at(400),
        "it ends at the limit, never at the gain above it"
    );
    assert!(
        left.iter().all(|s| *s <= at(400)),
        "and never passes the limit"
    );
    assert_eq!(watch.room().messages(), 2, "both messages were taken");
}

/// A scripted server on the real server-side handshake and records: hello,
/// stream_format, the committed vector's room_volume, then a paced PCM stream
/// and its end.
fn scripted_server(message: RoomVolume) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().unwrap().port();
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
        // Upstream is read and dropped (hello, capabilities).
        let mut reader = SecureReader::new(stream.try_clone().unwrap(), established.opener);
        reader.set_handler(Box::new(|_| {}));
        reader.set_translator(Box::new(|_| Ok(Translation::Pass)));
        thread::spawn(move || {
            let mut sink = [0u8; 16 * 1024];
            while matches!(std::io::Read::read(&mut reader, &mut sink), Ok(n) if n > 0) {}
        });
        let send = |w: &mut SecureWriter<TcpStream>, m: Message| w.send(&m).is_ok();
        let _ = send(
            &mut writer,
            Message::Hello(Hello {
                protocol_version: PROTOCOL_VERSION,
                roles: 0,
                name: "scripted".to_string(),
                software: "room_volume test".to_string(),
            }),
        );
        let _ = send(
            &mut writer,
            Message::StreamFormat(StreamFormat {
                codec: Codec::Pcm,
                sample_format: SampleFormat::PcmS16Le,
                sample_rate_hz: RATE_HZ,
                channel_map: vec![ChannelPosition::FrontLeft, ChannelPosition::FrontRight],
                frames_per_chunk: FRAMES_PER_CHUNK as u32,
                codec_config: Vec::new(),
            }),
        );
        // Before the first audio, as ADR 0074 asks of the server.
        let _ = send(&mut writer, Message::RoomVolume(message));
        let start = Instant::now();
        for sequence in 0..CHUNKS {
            let due = start + Duration::from_millis(20 * u64::from(sequence));
            if let Some(wait) = due.checked_duration_since(Instant::now()) {
                thread::sleep(wait);
            }
            if !send(&mut writer, Message::AudioChunk(chunk(sequence))) {
                return;
            }
        }
        let _ = send(
            &mut writer,
            Message::StreamEnd(StreamEnd {
                final_sequence: CHUNKS - 1,
                end_timestamp_ns: u64::from(CHUNKS) * 20_000_000,
            }),
        );
        thread::sleep(Duration::from_secs(2));
    });
    port
}

/// The committed vector's fields, read as the protocol tests read them.
fn committed_above_limit() -> RoomVolume {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/protocol/v2/room_volume_above_limit.fields");
    let text = std::fs::read_to_string(&path).expect("the committed vector");
    let field = |key: &str| -> u16 {
        text.lines()
            .filter_map(|l| l.split('#').next())
            .filter_map(|l| l.split_once('='))
            .find(|(k, _)| k.trim() == key)
            .and_then(|(_, v)| v.trim().parse().ok())
            .unwrap_or_else(|| panic!("{} in {}", key, path.display()))
    };
    RoomVolume {
        gain: field("gain"),
        limit: field("limit"),
        ramp_ms: field("ramp_ms"),
    }
}

#[test]
fn over_the_real_encrypted_session_a_room_volume_above_its_limit_plays_at_the_limit() {
    let finished = Arc::new(AtomicBool::new(false));
    {
        let finished = Arc::clone(&finished);
        thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(60);
            while Instant::now() < deadline {
                if finished.load(Ordering::SeqCst) {
                    return;
                }
                thread::sleep(Duration::from_millis(100));
            }
            eprintln!("room_volume: the session test did not finish within 60 s; failing it");
            std::process::exit(101);
        });
    }
    let message = committed_above_limit();
    assert!(
        message.gain > message.limit,
        "the vector is a gain above its limit"
    );
    let port = scripted_server(message);

    let path = temp_path("session");
    let config = a_config(&path);
    let stream = TcpStream::connect(("127.0.0.1", port)).expect("the server listens");
    stream
        .set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    let mut me = EndpointIdentity::ephemeral("room-volume-endpoint").unwrap();
    let secure = session::open(stream, &mut me, &config)
        .unwrap_or_else(|e| panic!("the session opens: {}", e));
    let watch = Arc::new(ZoneWatch::new());
    session::deliver_room_volume_to(&secure.announced, watch.room_inbox());
    let announced = Arc::clone(&secure.announced);
    let mut reader = secure.reader;
    let (done_tx, done_rx) = mpsc::channel();
    let player = {
        let watch = Arc::clone(&watch);
        thread::spawn(move || {
            let hand = handshake(&mut reader, &|| true).expect("the stream starts");
            let mut sink = RecordingSink::new();
            let tape = sink.tape();
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
            .expect("the run writes its log");
            let _ = done_tx.send(());
            let bytes = tape.lock().unwrap().clone();
            samples(&bytes)
        })
    };
    done_rx
        .recv_timeout(Duration::from_secs(30))
        .expect("the run ends");
    let accepted = player.join().expect("the player");
    let _ = std::fs::remove_file(&path);
    finished.store(true, Ordering::SeqCst);

    assert_eq!(
        announced.lock().unwrap().room_volumes,
        1,
        "the session received the one room_volume"
    );
    assert_eq!(watch.room_inbox().received(), 1, "and delivered it");
    assert_eq!(watch.room().limit(), volume("0.600"));
    assert_all(
        &accepted,
        at(i64::from(message.limit)),
        "gain 900 over limit 600, over the encrypted session",
    );
}
