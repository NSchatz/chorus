//! Protocol v2 end to end: the real `chorus-server` binary and the Linux
//! client's real session path, over loopback, with encryption on.
//!
//! The client side is `chorus_client_linux::session::open` (the function
//! `chorus-client` calls after it connects), the client's own receive path
//! (`receive::handshake`) and `run_session`, with a modelled device in place
//! of ALSA because the test machine has no sound card and the shipped client
//! deliberately has no flag that selects anything but ALSA. Between the two
//! sits a byte-counting TCP proxy, so what was on the wire is checked, not
//! inferred: after the handshake every frame in both directions is a
//! `secure_record`, and the plaintext PCM of a known chunk appears nowhere in
//! the captured bytes.

mod common;

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chorus_audio::MonotonicTimeline;
use chorus_client_linux::config::ClientConfig;
use chorus_client_linux::delaylog::DelayLog;
use chorus_client_linux::receive::{handshake, Received, Receiver};
use chorus_client_linux::run::{header_for, run_session, StopReason};
use chorus_client_linux::session::{self, EndpointIdentity, SessionRefusal};
use chorus_client_linux::sink::{PcmSink, SinkError, SinkWrite};
use chorus_client_linux::{Counters, ZoneWatch};
use chorus_protocol::v2::adoption::PinStore;
use chorus_protocol::v2::noise::{fingerprint, Keypair};
use chorus_protocol::{encode, Message, TimeSync};

const RATE_HZ: u32 = 48_000;
const FRAME_LEN: usize = 4; // stereo pcm_s16le
const CHUNK_BYTES: usize = 960 * FRAME_LEN;
const CHUNKS: usize = 50;

const SECURE_RECORD: u8 = 0x24;
const HANDSHAKE_INIT: u8 = 0x20;
const HANDSHAKE_RESPONSE: u8 = 0x21;
const HANDSHAKE_FINISH: u8 = 0x22;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("chorus-v2-e2e-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    listener.local_addr().unwrap().port()
}

/// Bytes nothing else produces: a linear congruential sequence, so any
/// 64-byte window of it is found only where it was put.
fn known_pcm(len: usize) -> Vec<u8> {
    let mut x: u32 = 0x2545_f491;
    (0..len)
        .map(|_| {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (x >> 24) as u8
        })
        .collect()
}

struct Server {
    child: Child,
    lines: mpsc::Receiver<String>,
    seen: Vec<String>,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if thread::panicking() {
            while let Ok(line) = self.lines.recv_timeout(Duration::from_millis(200)) {
                self.seen.push(line);
            }
            eprintln!("the server said:\n{}", self.seen.join("\n"));
        }
    }
}

impl Server {
    fn start(port: u16, extra: &[&str]) -> Server {
        let mut child = Command::new(env!("CARGO_BIN_EXE_chorus-server"))
            .args([
                "--listen",
                &format!("127.0.0.1:{}", port),
                "--allow-non-realtime",
                "--allow-unlocked-memory",
            ])
            .args(extra)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the server binary runs");
        let (tx, lines) = mpsc::channel();
        for pipe in [
            Box::new(child.stdout.take().unwrap()) as Box<dyn Read + Send>,
            Box::new(child.stderr.take().unwrap()),
        ] {
            let tx = tx.clone();
            thread::spawn(move || {
                for line in BufReader::new(pipe).lines().map_while(Result::ok) {
                    if tx.send(line).is_err() {
                        return;
                    }
                }
            });
        }
        let mut server = Server {
            child,
            lines,
            seen: Vec::new(),
        };
        server.wait_for("listening on=");
        server
    }

    /// Wait for a line containing `what`, and give it back.
    fn wait_for(&mut self, what: &str) -> String {
        if let Some(line) = self.seen.iter().find(|l| l.contains(what)) {
            return line.clone();
        }
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            if let Ok(line) = self.lines.recv_timeout(Duration::from_millis(200)) {
                self.seen.push(line.clone());
                if line.contains(what) {
                    return line;
                }
            }
        }
        panic!(
            "the server never said {:?}; it said:\n{}",
            what,
            self.seen.join("\n")
        );
    }
}

/// A TCP proxy that forwards one connection and keeps every byte it carried,
/// each direction on its own.
struct Proxy {
    port: u16,
    up: Arc<Mutex<Vec<u8>>>,
    down: Arc<Mutex<Vec<u8>>>,
    done: mpsc::Receiver<()>,
}

impl Proxy {
    fn start(server_port: u16) -> Proxy {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().unwrap().port();
        let up = Arc::new(Mutex::new(Vec::new()));
        let down = Arc::new(Mutex::new(Vec::new()));
        let (done_tx, done) = mpsc::channel();
        {
            let up = Arc::clone(&up);
            let down = Arc::clone(&down);
            thread::spawn(move || {
                let (client, _) = listener.accept().expect("the client connects");
                let server =
                    TcpStream::connect(("127.0.0.1", server_port)).expect("the server listens");
                let pump = |mut from: TcpStream, mut to: TcpStream, keep: Arc<Mutex<Vec<u8>>>| {
                    thread::spawn(move || {
                        let mut buf = vec![0u8; 65_536];
                        loop {
                            match from.read(&mut buf) {
                                Ok(0) | Err(_) => break,
                                Ok(n) => {
                                    keep.lock().unwrap().extend_from_slice(&buf[..n]);
                                    if to.write_all(&buf[..n]).is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                        let _ = to.shutdown(Shutdown::Write);
                    })
                };
                let a = pump(client.try_clone().unwrap(), server.try_clone().unwrap(), up);
                let b = pump(server, client, down);
                let _ = a.join();
                let _ = b.join();
                let _ = done_tx.send(());
            });
        }
        Proxy {
            port,
            up,
            down,
            done,
        }
    }
}

/// Split captured bytes into frame type bytes, insisting they end on a frame
/// boundary.
fn frame_types(bytes: &[u8]) -> Vec<u8> {
    let mut types = Vec::new();
    let mut at = 0usize;
    while at < bytes.len() {
        assert!(
            at + 3 <= bytes.len(),
            "the capture ends inside a frame header"
        );
        let len = u16::from_be_bytes([bytes[at + 1], bytes[at + 2]]) as usize;
        assert!(
            at + 3 + len <= bytes.len(),
            "the capture ends inside a frame"
        );
        types.push(bytes[at]);
        at += 3 + len;
    }
    types
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// A device modelled closely enough for `run_session`: a ring that drains at
/// the nominal rate against the monotonic clock and reports what it holds as
/// its delay. Nothing here is evidence about ALSA; it is what lets the
/// client's real session and playout run on a machine with no sound card.
struct ModelledDevice {
    queued: f64,
    played: u64,
    last: Instant,
    running: bool,
    frame_len: usize,
    tape: Arc<Mutex<Vec<u8>>>,
}

impl ModelledDevice {
    fn new() -> ModelledDevice {
        ModelledDevice::with_frame_len(FRAME_LEN)
    }

    /// A device of any channel count and format: `frame_len` bytes a frame.
    /// It keeps every byte it accepts on its tape.
    fn with_frame_len(frame_len: usize) -> ModelledDevice {
        ModelledDevice {
            queued: 0.0,
            played: 0,
            last: Instant::now(),
            running: false,
            frame_len,
            tape: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn tick(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last).as_secs_f64();
        self.last = now;
        if self.running {
            let n = (elapsed * f64::from(RATE_HZ)).min(self.queued);
            self.queued -= n;
            self.played += n as u64;
        }
    }
}

impl PcmSink for ModelledDevice {
    fn device(&self) -> &str {
        "modelled"
    }
    fn frame_len(&self) -> usize {
        self.frame_len
    }
    fn rate_hz(&self) -> u32 {
        RATE_HZ
    }
    fn write(&mut self, pcm: &[u8]) -> Result<SinkWrite, SinkError> {
        self.tick();
        self.running = true;
        self.tape.lock().unwrap().extend_from_slice(pcm);
        let frames = (pcm.len() / self.frame_len) as f64;
        // A ring of 400 ms: a write that would overfill it blocks until it
        // has room, as a blocking ALSA write does.
        let ring = f64::from(RATE_HZ) * 0.4;
        while self.queued + frames > ring {
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
        while self.queued > 0.5 {
            thread::sleep(Duration::from_millis(2));
            self.tick();
        }
        self.running = false;
        Ok(())
    }
    fn frames_played(&mut self) -> Result<u64, SinkError> {
        self.tick();
        Ok(self.played)
    }
}

/// Whatever the client's receive path reads, kept as well, so the test can
/// decode exactly the plaintext frames the client was handed.
struct Tee<R> {
    inner: R,
    kept: Arc<Mutex<Vec<u8>>>,
}

impl<R: Read> Read for Tee<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.kept.lock().unwrap().extend_from_slice(&buf[..n]);
        Ok(n)
    }
}

#[test]
fn encryption_on_the_real_server_and_the_linux_client_stream_pcm_end_to_end_as_secure_records() {
    let dir = scratch("stream");
    let pcm = known_pcm(CHUNKS * CHUNK_BYTES);
    let source = dir.join("known.pcm");
    std::fs::write(&source, &pcm).unwrap();

    let port = free_port();
    let mut server = Server::start(
        port,
        &[
            "--source",
            source.to_str().unwrap(),
            "--identity-dir",
            dir.join("identity").to_str().unwrap(),
        ],
    );
    let proxy = Proxy::start(port);

    // The client, exactly as chorus-client opens its session.
    let endpoint_id = common::fresh_id("e2e-endpoint");
    let mut me = EndpointIdentity::ephemeral(&endpoint_id).unwrap();
    let endpoint_key = me.fingerprint();
    let config = ClientConfig {
        delay_log: dir.join("delay.log").to_str().unwrap().to_string(),
        ..ClientConfig::default()
    };
    config
        .validate()
        .expect("the default client configuration is valid");
    let stream = TcpStream::connect(("127.0.0.1", proxy.port)).expect("the proxy listens");
    stream
        .set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    let secure = session::open(stream, &mut me, &config)
        .unwrap_or_else(|e| panic!("the session opens: {}", e));
    assert!(secure.pinned_now, "a first session pins the server's key");

    let kept = Arc::new(Mutex::new(Vec::new()));
    let mut reader = Tee {
        inner: secure.reader,
        kept: Arc::clone(&kept),
    };
    let hand = handshake(&mut reader, &|| true).expect("the stream starts");
    let announced = secure.announced.lock().unwrap().clone();
    session::check_announcement(&announced, &hand.shape)
        .expect("the first chunk is what stream_format announced");
    let announced_format = announced.stream_format.clone().unwrap();
    assert_eq!(announced_format.frames_per_chunk, 960);
    assert_eq!(
        announced_format
            .channel_map
            .iter()
            .map(|p| p.name())
            .collect::<Vec<_>>(),
        ["FL", "FR"]
    );
    assert_eq!(announced.output_delay_ns, Some(0));

    let mut device = ModelledDevice::new();
    let header = header_for(&config, "modelled", &hand.shape);
    let mut log = DelayLog::open(&config.delay_log, &header).expect("the log opens");
    let counters = Arc::new(Counters::new());
    let outcome = run_session(
        &config,
        reader,
        hand,
        &mut device,
        &mut log,
        MonotonicTimeline::new(),
        Arc::clone(&counters),
        Some(Box::new(secure.writer)),
        Arc::new(ZoneWatch::new()),
    )
    .expect("the run writes its log");

    let plain = kept.lock().unwrap().clone();
    let plain_types = frame_types(&plain);
    eprintln!(
        "plaintext frames the client read: {} (last {:02x?}); stop {:?}",
        plain_types.len(),
        &plain_types[plain_types.len().saturating_sub(4)..],
        outcome.stop
    );
    // A clean end: the in-band stream_end, and every chunk in order.
    let end = match &outcome.stop {
        StopReason::EndOfStream(end) => *end,
        other => panic!("the run did not end on the in-band signal: {:?}", other),
    };
    let mut receiver = Receiver::new();
    let events = receiver
        .push(&kept.lock().unwrap())
        .expect("the plaintext the client read is well framed");
    let chunks: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Received::Chunk { chunk, .. } => Some(chunk.clone()),
            _ => None,
        })
        .collect();
    let replies = events
        .iter()
        .filter(|e| matches!(e, Received::TimeSync(_)))
        .count();
    assert_eq!(chunks.len(), CHUNKS, "every chunk arrived");
    for (i, c) in chunks.iter().enumerate() {
        assert_eq!(c.sequence, chunks[0].sequence + i as u32, "in order");
        assert_eq!(
            c.audio_data,
            &pcm[i * CHUNK_BYTES..(i + 1) * CHUNK_BYTES],
            "chunk {} carries the source's bytes",
            i
        );
    }
    assert_eq!(end.final_sequence, chunks[CHUNKS - 1].sequence);
    assert!(
        events.iter().any(|e| matches!(e, Received::End(_))),
        "the stream_end came in band"
    );
    assert!(
        outcome.played_anything,
        "the client's playout path played the stream"
    );

    // The server adopted this endpoint and said so.
    let adopted = server.wait_for("endpoint adopted id=");
    assert!(
        adopted.contains(&format!(
            "endpoint adopted id={} key={}",
            endpoint_id, endpoint_key
        )),
        "{}",
        adopted
    );
    let pins = std::fs::read_to_string(dir.join("identity").join("adopted-endpoints")).unwrap();
    assert!(pins.contains(&format!(" {}\n", endpoint_id)), "{}", pins);

    // The wire: after the handshake, only records, both ways.
    proxy
        .done
        .recv_timeout(Duration::from_secs(10))
        .expect("the connection closes after the stream ends");
    let up = proxy.up.lock().unwrap().clone();
    let down = proxy.down.lock().unwrap().clone();
    let up_types = frame_types(&up);
    let down_types = frame_types(&down);
    assert_eq!(&up_types[..2], &[HANDSHAKE_INIT, HANDSHAKE_FINISH]);
    assert_eq!(down_types[0], HANDSHAKE_RESPONSE);
    let up_records = up_types[2..]
        .iter()
        .filter(|t| **t == SECURE_RECORD)
        .count();
    let down_records = down_types[1..]
        .iter()
        .filter(|t| **t == SECURE_RECORD)
        .count();
    assert_eq!(
        up_records,
        up_types.len() - 2,
        "every frame the client sent after the handshake was a secure_record: {:02x?}",
        up_types
    );
    assert_eq!(
        down_records,
        down_types.len() - 1,
        "every frame the server sent after the handshake was a secure_record: {:02x?}",
        down_types
    );
    // hello and capabilities, then the time-sync requests.
    assert!(up_records >= 2, "{:02x?}", up_types);
    // hello, stream_format, output_delay, every chunk, the end, the replies.
    assert!(down_records > 3 + CHUNKS, "{:02x?}", down_types);

    // No plaintext: a known chunk's PCM, whole or in part, is nowhere on the
    // wire, and neither is any v1 frame header of the stream.
    for i in [0usize, 10, CHUNKS - 1] {
        let chunk = &pcm[i * CHUNK_BYTES..(i + 1) * CHUNK_BYTES];
        assert!(!contains(&down, chunk), "chunk {} crossed in the clear", i);
        assert!(
            !contains(&down, &chunk[..64]) && !contains(&up, &chunk[..64]),
            "64 bytes of chunk {}'s PCM crossed in the clear",
            i
        );
    }
    let first_frame = encode(&Message::AudioChunk(chunks[0].clone())).unwrap();
    assert!(!contains(&down, &first_frame[..35]));

    println!(
        "v2 end to end: {} chunks in order, {} replies, stream_end final_sequence={}; wire up {} \
         bytes ({} records after 2 handshake frames), down {} bytes ({} records after 1); \
         endpoint {} adopted with key {}",
        chunks.len(),
        replies,
        end.final_sequence,
        up.len(),
        up_records,
        down.len(),
        down_records,
        endpoint_id,
        endpoint_key
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Goal 10 line A, end to end: the real server serves a 5.1 PCM stream and
/// the Linux client's real session and playout path play it through an output
/// map onto a modelled 8-channel device in another order.
#[test]
fn a_5_1_stream_from_the_real_server_plays_through_an_output_map_onto_8_channels() {
    use chorus_client_linux::outmap::{MappedSink, OutputMap};

    const CHANNELS: usize = 6;
    const FRAME: usize = CHANNELS * 2; // pcm_s16le
    const FRAMES: usize = 960;
    let dir = scratch("multichannel");
    let pcm = known_pcm(CHUNKS * FRAMES * FRAME);
    let source = dir.join("known-5-1.pcm");
    std::fs::write(&source, &pcm).unwrap();

    let port = free_port();
    let _server = Server::start(
        port,
        &[
            "--source",
            source.to_str().unwrap(),
            "--channels",
            "6",
            "--identity-dir",
            dir.join("identity").to_str().unwrap(),
        ],
    );

    // The device's order: ALSA surround51 (FL FR RL RR FC LFE), then a stereo
    // downmix and one unmapped output.
    let outputs: Vec<String> = ["0=FL", "1=FR", "2=BL", "3=BR", "4=FC", "5=LFE", "6=FL+FR"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let map = OutputMap::from_args(Some("8"), &outputs).unwrap().unwrap();
    let mut me = EndpointIdentity::ephemeral(&common::fresh_id("e2e-multichannel")).unwrap();
    let config = ClientConfig {
        delay_log: dir.join("delay.log").to_str().unwrap().to_string(),
        output_map: Some(map.clone()),
        ..ClientConfig::default()
    };
    config.validate().unwrap();
    assert_eq!(session::capabilities(&config).max_channels, 6);
    let stream = TcpStream::connect(("127.0.0.1", port)).expect("the server listens");
    stream
        .set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    let secure = session::open(stream, &mut me, &config)
        .unwrap_or_else(|e| panic!("the session opens: {}", e));
    let mut reader = secure.reader;
    let hand = handshake(&mut reader, &|| true).expect("the stream starts");
    let announced = secure.announced.lock().unwrap().clone();
    session::check_announcement(&announced, &hand.shape).expect("the announcement holds");
    let stream_map = announced.stream_format.clone().unwrap().channel_map;
    assert_eq!(
        stream_map.iter().map(|p| p.name()).collect::<Vec<_>>(),
        ["FL", "FR", "FC", "LFE", "BL", "BR"]
    );

    let device = ModelledDevice::with_frame_len(8 * 2);
    let tape = Arc::clone(&device.tape);
    let (mut sink, resolved) =
        MappedSink::new(device, &map, &stream_map, hand.shape.sample_format).unwrap();
    let header = header_for(&config, "modelled-8", &hand.shape);
    let mut log = DelayLog::open(&config.delay_log, &header).expect("the log opens");
    // No time-sync exchange (`None`): the loop then corrects nothing, so every
    // device frame is exactly one source frame and the tape can be compared
    // with the source byte for byte.
    let _writer = secure.writer;
    let outcome = run_session(
        &config,
        reader,
        hand,
        &mut sink,
        &mut log,
        MonotonicTimeline::new(),
        Arc::new(Counters::new()),
        None,
        Arc::new(ZoneWatch::new()),
    )
    .expect("the run writes its log");
    assert!(
        matches!(outcome.stop, StopReason::EndOfStream(_)),
        "{:?}",
        outcome.stop
    );

    let tape = tape.lock().unwrap().clone();
    assert_eq!(
        tape.len(),
        CHUNKS * FRAMES * 16,
        "every source frame, as one device frame"
    );
    // device channel <- stream channel: FL FR BL BR FC LFE.
    let order = [0usize, 1, 4, 5, 2, 3];
    for (f, (src, dev)) in pcm
        .as_chunks::<FRAME>()
        .0
        .iter()
        .zip(tape.as_chunks::<16>().0)
        .enumerate()
    {
        for (d, s) in order.iter().enumerate() {
            assert_eq!(
                &dev[d * 2..d * 2 + 2],
                &src[s * 2..s * 2 + 2],
                "frame {} device channel {}",
                f,
                d
            );
        }
        let l = i16::from_le_bytes([src[0], src[1]]) as f64;
        let r = i16::from_le_bytes([src[2], src[3]]) as f64;
        let mix = i16::from_le_bytes([dev[12], dev[13]]) as f64;
        assert_eq!(mix, ((l + r) / 2.0).round(), "frame {} downmix", f);
        assert_eq!(&dev[14..16], &[0, 0], "frame {} unmapped output", f);
    }
    println!(
        "5.1 end to end: {} frames from the real server onto 8 channels; {}",
        tape.len() / 16,
        resolved.report().join(" | ")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_endpoint_whose_key_changed_is_refused_and_surfaced_by_the_server() {
    let dir = scratch("key-changed");
    let identity = dir.join("identity");
    std::fs::create_dir_all(&identity).unwrap();
    let endpoint_id = "kitchen-endpoint";
    let first = Keypair::from_secret([0x41; 32]);
    let second = Keypair::from_secret([0x42; 32]);
    let mut pins = PinStore::new();
    assert!(pins.check(endpoint_id, &first.public).admits());
    let pin_path = identity.join("adopted-endpoints");
    std::fs::write(&pin_path, pins.to_text()).unwrap();
    let before = std::fs::read(&pin_path).unwrap();

    let port = free_port();
    let mut server = Server::start(
        port,
        &[
            "--source",
            "tone",
            "--identity-dir",
            identity.to_str().unwrap(),
        ],
    );

    let mut me = EndpointIdentity::from_secret(endpoint_id, [0x42; 32]).unwrap();
    let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let refusal = match session::open(stream, &mut me, &ClientConfig::default()) {
        Err(e) => e,
        Ok(_) => panic!("a changed key was admitted"),
    };
    match &refusal {
        SessionRefusal::RefusedByServer { reason, detail } => {
            assert_eq!(*reason, "key_changed");
            assert!(detail.contains(endpoint_id), "{}", detail);
        }
        other => panic!("expected the server's key_changed refusal, got {:?}", other),
    }
    assert_eq!(refusal.reason(), "session-refused-key-changed");

    let line = server.wait_for("endpoint key changed");
    let expected = format!(
        "endpoint key changed id={} pinned={} offered={}; refused",
        endpoint_id,
        fingerprint(&first.public),
        fingerprint(&second.public)
    );
    assert!(line.contains(&expected), "{}\nwanted {}", line, expected);
    assert_eq!(
        std::fs::read(&pin_path).unwrap(),
        before,
        "the pin file is unchanged: a changed key is never re-pinned"
    );
    println!("client: {}", refusal);
    println!("server: {}", line);
    drop(server);
    let _ = std::fs::remove_dir_all(&dir);
}

fn fixture_frame(stem: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/protocol/v2")
        .join(format!("{}.hex", stem));
    let text = std::fs::read_to_string(&path).expect("the committed vector");
    text.lines()
        .map(|l| l.split('#').next().unwrap_or(""))
        .flat_map(|l| l.split_whitespace().map(str::to_string).collect::<Vec<_>>())
        .map(|h| u8::from_str_radix(&h, 16).expect("hex"))
        .collect()
}

#[test]
fn a_v1_client_is_refused_with_the_committed_frame_and_the_server_names_protocol_v1() {
    let port = free_port();
    let mut server = Server::start(port, &["--source", "tone", "--ephemeral-identity"]);
    let mut v1 = TcpStream::connect(("127.0.0.1", port)).unwrap();
    v1.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let request = encode(&Message::TimeSync(TimeSync {
        t0_ns: 1,
        t1_ns: 0,
        t2_ns: 0,
        t3_ns: 0,
    }))
    .unwrap();
    v1.write_all(&request).unwrap();
    let mut answer = Vec::new();
    v1.read_to_end(&mut answer)
        .expect("the server answers and closes");
    assert_eq!(
        answer,
        fixture_frame("session_refused_v1_peer"),
        "exactly fixtures/protocol/v2/session_refused_v1_peer.hex, then the close"
    );
    let line = server.wait_for("reason=protocol-v1");
    assert!(line.contains("client refused peer="), "{}", line);
    assert!(line.contains("protocol v1"), "{}", line);
    println!("v1 peer got {} bytes; server: {}", answer.len(), line);
}

#[test]
fn a_server_started_with_no_identity_is_refused_by_name() {
    let out = Command::new(env!("CARGO_BIN_EXE_chorus-server"))
        .args([
            "--listen",
            "127.0.0.1:0",
            "--allow-non-realtime",
            "--allow-unlocked-memory",
        ])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(2), "{}{}", stdout, stderr);
    assert!(stderr.contains("--identity-dir"), "{}", stderr);
    assert!(stdout.contains("stopped reason=no-identity"), "{}", stdout);
}

#[test]
fn a_slow_handshake_holds_a_slot_not_the_acceptor() {
    // One silent peer that never sends handshake_init, then a real session:
    // the second is served while the first is still being waited on.
    let port = free_port();
    let mut server = Server::start(
        port,
        &[
            "--source",
            "tone",
            "--tone-ms",
            "200",
            "--ephemeral-identity",
            "--max-clients",
            "2",
            // Keep serving after the stream, so the silent peer's handshake
            // timeout is still ahead when the stream ends.
            "--serve-forever",
        ],
    );
    let silent = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let started = Instant::now();
    let mut client = common::v2_client(("127.0.0.1", port), Duration::from_secs(5));
    assert!(
        started.elapsed() < Duration::from_millis(1_500),
        "the session waited on the silent peer's handshake: {:?}",
        started.elapsed()
    );
    assert!(common::drain(&mut client) > 0);
    server.wait_for("reason=handshake-timeout");
    drop(silent);
}
