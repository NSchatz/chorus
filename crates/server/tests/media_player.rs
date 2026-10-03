//! Goal 16, the media player engine: a URL goes in at one end and the PCM a
//! real client session receives at the other is the media, sample for sample.
//!
//! The server is assembled in this process as `player_port.rs` does it (the
//! room model, one stream slot, the conductor, the audio thread's
//! `serve_slots`, the client pool), with one difference: the player's thread
//! runs the real driver (`mediaplayer::MediaPlayer`). The media comes from an
//! HTTP server on loopback inside the test, so the engine's fetch policy is
//! built with `allow_loopback: true` here, and only here. The listener is the
//! Linux client's own session code (`common::Player`).
//!
//! What "equal" means: the server sends 16-bit PCM, so the expected samples
//! are a decode (by `chorus-decode`, which `crates/decode/tests` holds to the
//! reference decodes of `fixtures/decode`) put through the server's own
//! rounding to 16 bits. Lossless media at the server's rate is then the
//! file's own samples; everything else is compared with the same decode
//! taken through the same resampler.
//!
//! Timing is asserted in three places, each stated where it is: where
//! `Boundary` is reported, how soon a `Stop` or a `Load` gets through a
//! stalled source, and that a paused position does not move.

mod common;

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::io::{Cursor, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chorus_audio::{MonotonicTimeline, StreamFormat};
use chorus_control::rooms::PlayState;
use chorus_control::zones::{Zone, Zones};
use chorus_decode::{Codec, Decoder, Format, Hint, Resampler, Tags};
use chorus_hostctl::ThreadRegistry;
use chorus_server::clients::ClientPool;
use chorus_server::conductor::{self, Conductor};
use chorus_server::control::ControlState;
use chorus_server::mediaplayer::{
    fetch_policy, Action, Event, MediaInfo, PlayerHandle, PlayerReport, PlayerState, Players,
};
use chorus_server::player;
use chorus_server::playerport::PlayerPort;
use chorus_server::playersessions::{Metadata, PlayRequest, PlayerSessions};
use chorus_server::router::Router;
use chorus_server::serve::ServeParams;
use chorus_server::session::{load_identity, IdentitySource, Offer, SessionContext};
use chorus_server::slots::{serve_slots, SlotCommand, SlotEvent, SlotMedia};
use chorus_server::source::PcmSource;

use common::Player as Listener;

/// 10 ms chunks.
const CHUNK_US: u64 = 10_000;
const LIMIT: Duration = Duration::from_secs(30);

/// How long after the audio thread took the first frame of a joined track
/// `Boundary` may be reported, in chunks, as asserted here: the report is
/// received while the port's count is at most this many chunks past the
/// join. The engine looks at the count every `player::POLL` (5 ms) and a
/// chunk is 10 ms, so the report belongs in the join's own chunk; one more
/// is allowed for a thread that was not scheduled at once.
const BOUNDARY_CHUNKS: u64 = 2;

/// How soon a `Stop`, or a `Load`, sent while the source stalls must have
/// taken effect, as asserted here. The engine's own bound is one
/// `chorus_fetch::CANCEL_SLICE` (100 ms) plus a turn of its loop; the fetch
/// policy's read timeout, which is what a stall would cost without it, is
/// 15 s.
const STALL_BOUND: Duration = Duration::from_secs(1);

type Frame = [i16; 2];

// ----- the media server ------------------------------------------------------

#[derive(Clone)]
enum Kind {
    /// A length, and range requests honoured.
    File,
    /// A length and no ranges: not seekable.
    NoRanges,
    /// No length, the body ends when the connection closes; ICY metadata
    /// every `metaint` bytes carrying `title`, when the request asks for it.
    Live { metaint: usize, title: String },
    /// The head and the first `after` bytes, then nothing, for ever.
    Stall { after: usize },
    /// Nothing at all after the request: not even a head.
    Mute,
}

#[derive(Clone)]
struct Route {
    body: Arc<Vec<u8>>,
    content_type: String,
    kind: Kind,
}

/// An HTTP server on loopback, a thread per connection, serving what the
/// test put in it.
struct MediaServer {
    address: String,
    accepts: Arc<AtomicUsize>,
    routes: Arc<Mutex<HashMap<String, Route>>>,
    open: Arc<AtomicBool>,
}

impl Drop for MediaServer {
    fn drop(&mut self) {
        self.open.store(false, Ordering::SeqCst);
    }
}

impl MediaServer {
    fn start() -> MediaServer {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let accepts = Arc::new(AtomicUsize::new(0));
        let routes: Arc<Mutex<HashMap<String, Route>>> = Arc::default();
        let open = Arc::new(AtomicBool::new(true));
        {
            let (accepts, routes, open) =
                (Arc::clone(&accepts), Arc::clone(&routes), Arc::clone(&open));
            thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(stream) = stream else { return };
                    if !open.load(Ordering::SeqCst) {
                        return;
                    }
                    accepts.fetch_add(1, Ordering::SeqCst);
                    let (routes, open) = (Arc::clone(&routes), Arc::clone(&open));
                    thread::spawn(move || answer(stream, &routes, &open));
                }
            });
        }
        MediaServer {
            address,
            accepts,
            routes,
            open,
        }
    }

    fn serve(&self, path: &str, body: Vec<u8>, content_type: &str, kind: Kind) -> String {
        self.routes.lock().unwrap().insert(
            path.to_string(),
            Route {
                body: Arc::new(body),
                content_type: content_type.to_string(),
                kind,
            },
        );
        self.url(path)
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.address, path)
    }

    fn accepts(&self) -> usize {
        self.accepts.load(Ordering::SeqCst)
    }
}

fn answer(mut stream: TcpStream, routes: &Mutex<HashMap<String, Route>>, open: &AtomicBool) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        match stream.read(&mut byte) {
            Ok(1) => head.push(byte[0]),
            _ => return,
        }
    }
    let head = String::from_utf8_lossy(&head).to_string();
    let path = head.split(' ').nth(1).unwrap_or("/").to_string();
    let header = |name: &str| {
        head.lines().find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.eq_ignore_ascii_case(name).then(|| v.trim().to_string())
        })
    };
    let hold = |stream: TcpStream| {
        while open.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(20));
        }
        drop(stream);
    };
    let route = routes.lock().unwrap().get(&path).cloned();
    let Some(route) = route else {
        let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
        return;
    };
    let len = route.body.len();
    match route.kind {
        Kind::File => {
            let from = header("range")
                .and_then(|r| r.strip_prefix("bytes=").map(str::to_string))
                .and_then(|r| r.split('-').next().and_then(|n| n.parse::<usize>().ok()));
            let head = match from {
                Some(from) if from < len => format!(
                    "HTTP/1.1 206 Partial Content\r\nContent-Type: {}\r\nAccept-Ranges: bytes\r\n\
                     Content-Range: bytes {}-{}/{}\r\nContent-Length: {}\r\n\r\n",
                    route.content_type,
                    from,
                    len - 1,
                    len,
                    len - from
                ),
                _ => format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nAccept-Ranges: bytes\r\n\
                     Content-Length: {}\r\n\r\n",
                    route.content_type, len
                ),
            };
            let from = from.filter(|f| *f < len).unwrap_or(0);
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&route.body[from..]);
        }
        Kind::NoRanges => {
            let _ = stream.write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n",
                    route.content_type, len
                )
                .as_bytes(),
            );
            let _ = stream.write_all(&route.body);
        }
        Kind::Live { metaint, title } => {
            let icy = header("icy-metadata").as_deref() == Some("1");
            let mut head = format!(
                "HTTP/1.0 200 OK\r\nContent-Type: {}\r\nicy-name: Example Radio\r\n",
                route.content_type
            );
            if icy {
                head.push_str(&format!("icy-metaint: {}\r\n", metaint));
            }
            head.push_str("\r\n");
            let _ = stream.write_all(head.as_bytes());
            if !icy {
                let _ = stream.write_all(&route.body);
                return;
            }
            let mut block = format!("StreamTitle='{}';", title).into_bytes();
            let blocks = block.len().div_ceil(16);
            block.resize(blocks * 16, 0);
            for piece in route.body.chunks(metaint) {
                if stream.write_all(piece).is_err() {
                    return;
                }
                if piece.len() == metaint {
                    let _ = stream.write_all(&[blocks as u8]);
                    let _ = stream.write_all(&block);
                }
            }
        }
        Kind::Stall { after } => {
            let _ = stream.write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n",
                    route.content_type, len
                )
                .as_bytes(),
            );
            let _ = stream.write_all(&route.body[..after.min(len)]);
            hold(stream);
        }
        Kind::Mute => hold(stream),
    }
}

// ----- the server's pieces ---------------------------------------------------

struct House {
    address: String,
    state: Arc<ControlState>,
    port: Arc<PlayerPort>,
    players: Arc<Players>,
    reports: mpsc::Receiver<PlayerReport>,
    keep: Arc<AtomicBool>,
    chunk: usize,
    _streams: mpsc::Sender<Box<dyn PcmSource>>,
    _events: mpsc::Receiver<SlotEvent>,
}

impl Drop for House {
    fn drop(&mut self) {
        self.keep.store(false, Ordering::SeqCst);
    }
}

/// One room, `kitchen`, whose endpoint is `endpoint`, and a `study` with no
/// slot; one stream slot at `rate` Hz, 16-bit stereo; one player, `p0`,
/// whose thread runs the media player engine.
fn house(rate: u32, endpoint: &str, allow_loopback: bool) -> House {
    let format = StreamFormat::new(rate, 2, "pcm_s16le").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let timeline = MonotonicTimeline::new();
    let router = Arc::new(Router::slotted(1));
    let keep = Arc::new(AtomicBool::new(true));
    let registry = Arc::new(ThreadRegistry::new());
    let (ready, came_up) = mpsc::channel::<()>();

    let mut zones = Zones::new(&address);
    let mut kitchen = Zone::new("kitchen");
    kitchen.endpoints.push(endpoint.to_string());
    zones.add(kitchen).unwrap();
    zones.add(Zone::new("study")).unwrap();
    let state = Arc::new(ControlState::new(zones, None));
    assert_eq!(state.serve_on_slots(1), ["study"]);
    state.set_players(1);

    let port = Arc::new(PlayerPort::for_format(&format));
    let media = SlotMedia {
        players: vec![Arc::clone(&port)],
        ..SlotMedia::default()
    };
    let (slot_commands, slot_inbox) = mpsc::sync_channel::<SlotCommand>(8);
    let (streams, stream_jobs) = mpsc::channel::<Box<dyn PcmSource>>();
    let (slot_events, events) = mpsc::channel::<SlotEvent>();
    {
        let conductor =
            Conductor::new(Arc::clone(&state), Arc::clone(&router), Some(slot_commands))
                .with_players(1);
        let keep = Arc::clone(&keep);
        thread::spawn(move || conductor::run(conductor, keep));
    }
    {
        let router = Arc::clone(&router);
        let keep = Arc::clone(&keep);
        let params = ServeParams {
            format,
            chunk_us: CHUNK_US,
            rate_skew_ppm: 0,
        };
        thread::spawn(move || {
            serve_slots(
                params,
                timeline,
                &router,
                &media,
                &slot_inbox,
                &stream_jobs,
                &slot_events,
                &keep,
            )
            .expect("the slots are served until the run stops");
        });
    }
    let (identity, adoptions) = load_identity(&IdentitySource::Ephemeral, "media-player-test")
        .expect("an ephemeral identity");
    let session = Arc::new(SessionContext {
        identity,
        adoptions: Arc::new(adoptions),
        offer: Offer::new(&format, CHUNK_US).expect("an offer"),
        log: Box::new(|_| {}),
        on_session: Box::new(|| {}),
        hellos: Default::default(),
        telemetry: Default::default(),
        control: Some(Arc::clone(&state)),
        router: Arc::clone(&router),
        line_ins: None,
        tv_relay: None,
        firmware: None,
    });
    let pool = ClientPool::spawn(
        2,
        timeline,
        Arc::clone(&keep),
        Arc::clone(&registry),
        ready.clone(),
        session,
    );
    // The player thread, as `main.rs` makes it: the engine is its driver,
    // and the fetch policy names this server's own listener.
    let own_port = address.rsplit(':').next().unwrap().parse::<u16>().unwrap();
    let (players, drivers) = Players::new(1, fetch_policy(&[own_port], allow_loopback));
    let reports = players.take_reports().expect("the reports");
    let threads = player::spawn(
        std::slice::from_ref(&port),
        drivers,
        &keep,
        &registry,
        &ready,
    );
    drop(ready);
    for _ in 0..pool.threads() + threads {
        came_up.recv_timeout(LIMIT).expect("every thread comes up");
    }
    assert!(registry.snapshot().iter().any(|t| t.role == "player-0"));
    {
        let keep = Arc::clone(&keep);
        thread::spawn(move || {
            while keep.load(Ordering::SeqCst) {
                let Ok((stream, peer)) = listener.accept() else {
                    return;
                };
                let _ = stream.set_nodelay(true);
                let reader = stream.try_clone().expect("a second handle");
                let _ = reader.set_read_timeout(Some(Duration::from_millis(200)));
                assert!(pool.attach(stream, reader, peer), "a free client slot");
            }
        });
    }
    House {
        address,
        state,
        port,
        players: Arc::new(players),
        reports,
        keep,
        chunk: (u64::from(rate) * CHUNK_US / 1_000_000) as usize,
        _streams: streams,
        _events: events,
    }
}

/// One chunk as received: its sequence and its PCM.
type Chunk = (u32, Vec<u8>);

/// Everything a listener received, frame by frame.
#[derive(Clone, Default)]
struct Heard(Arc<Mutex<Vec<Chunk>>>);

impl Heard {
    fn from(mut listener: Listener, keep: Arc<AtomicBool>) -> Heard {
        let heard = Heard::default();
        let into = heard.clone();
        thread::spawn(move || {
            while keep.load(Ordering::SeqCst) {
                if let Some(chunk) = listener.next_chunk(Duration::from_millis(100)) {
                    into.0
                        .lock()
                        .unwrap()
                        .push((chunk.sequence, chunk.audio_data));
                }
            }
        });
        heard
    }

    /// Every frame received, in order; checks that no chunk is missing.
    fn frames(&self) -> Vec<Frame> {
        let chunks = self.0.lock().unwrap();
        let mut out = Vec::new();
        for (i, (sequence, data)) in chunks.iter().enumerate() {
            if i > 0 {
                assert_eq!(
                    *sequence,
                    chunks[i - 1].0.wrapping_add(1),
                    "a chunk is missing before sequence {}",
                    sequence
                );
            }
            for f in data.as_chunks::<4>().0 {
                out.push([
                    i16::from_le_bytes([f[0], f[1]]),
                    i16::from_le_bytes([f[2], f[3]]),
                ]);
            }
        }
        out
    }

    fn chunks(&self) -> usize {
        self.0.lock().unwrap().len()
    }
}

const SILENCE: Frame = [0, 0];

fn wait(what: &str, done: impl Fn() -> bool) {
    let deadline = Instant::now() + LIMIT;
    while !done() {
        assert!(
            Instant::now() < deadline,
            "{}: not within {:?}",
            what,
            LIMIT
        );
        thread::sleep(Duration::from_millis(2));
    }
}

/// A house with a listener in the kitchen, the kitchen playing `player:p0`,
/// and a media server.
struct Rig {
    house: House,
    media: MediaServer,
    heard: Heard,
    handle: PlayerHandle,
    epoch: Cell<u64>,
    seen: RefCell<Vec<Event>>,
}

impl Rig {
    fn new(rate: u32) -> Rig {
        Rig::with_policy(rate, true)
    }

    fn with_policy(rate: u32, allow_loopback: bool) -> Rig {
        let endpoint = common::fresh_id("kitchen-speaker");
        let house = house(rate, &endpoint, allow_loopback);
        let listener = Listener::connect(&house.address, &endpoint, 0);
        let heard = Heard::from(listener, Arc::clone(&house.keep));
        wait("the session's first chunks", || heard.chunks() >= 3);
        house
            .state
            .apply(r#"{"v":2,"t":"take","target":"kitchen","source":"player:p0"}"#)
            .expect("the kitchen takes the player");
        let handle = house.players.handle(0).unwrap().clone();
        Rig {
            house,
            media: MediaServer::start(),
            heard,
            handle,
            epoch: Cell::new(1),
            seen: RefCell::new(Vec::new()),
        }
    }

    fn send(&self, action: Action) {
        assert!(self.handle.send(self.epoch.get(), action));
    }

    fn load(&self, uri: &str) -> MediaInfo {
        self.send(Action::Load {
            uri: uri.to_string(),
            mime: None,
        });
        match self.next() {
            Event::Opened(info) => info,
            other => panic!("Opened expected, got {:?}", other),
        }
    }

    fn queue(&self, uri: &str) {
        self.send(Action::QueueNext {
            uri: uri.to_string(),
            mime: None,
        });
    }

    /// The next report; it carries the epoch of the actions sent.
    fn next(&self) -> Event {
        let report = self
            .house
            .reports
            .recv_timeout(LIMIT)
            .unwrap_or_else(|_| panic!("no report within {:?}; seen {:?}", LIMIT, self.seen));
        assert_eq!(report.player, 0);
        assert_eq!(report.epoch, self.epoch.get(), "{:?}", report);
        self.seen.borrow_mut().push(report.event.clone());
        report.event
    }

    /// The next report `wanted` holds of, passing over the others.
    fn until(&self, what: &str, wanted: impl Fn(&Event) -> bool) -> Event {
        loop {
            let event = self.next();
            assert!(
                !matches!(event, Event::Failed { .. }) || wanted(&event),
                "{}: {:?}",
                what,
                event
            );
            if wanted(&event) {
                return event;
            }
        }
    }

    fn ended(&self) -> u64 {
        match self.until("Ended", |e| matches!(e, Event::Ended { .. })) {
            Event::Ended { played_ms } => played_ms,
            _ => unreachable!(),
        }
    }

    /// How many of the reports seen so far `wanted` holds of, after a pause
    /// in which anything late would have arrived.
    fn count(&self, wanted: impl Fn(&Event) -> bool) -> usize {
        thread::sleep(Duration::from_millis(60));
        while let Ok(report) = self.house.reports.try_recv() {
            self.seen.borrow_mut().push(report.event);
        }
        self.seen.borrow().iter().filter(|e| wanted(e)).count()
    }

    /// Waits until `expected` has been received as one unbroken run followed
    /// by silence, and returns everything received and where the run starts.
    fn run_of(&self, what: &str, expected: &[Frame]) -> (Vec<Frame>, usize) {
        let quiet = 3 * self.house.chunk;
        let deadline = Instant::now() + LIMIT;
        loop {
            let frames = self.heard.frames();
            let tail_is_quiet = frames.len() > quiet
                && frames[frames.len() - quiet..].iter().all(|f| *f == SILENCE);
            if tail_is_quiet {
                if let Some(at) = find(&frames, expected) {
                    return (frames, at);
                }
            }
            if Instant::now() >= deadline {
                let signal: Vec<Frame> = frames.iter().copied().filter(|f| *f != SILENCE).collect();
                let wanted: Vec<Frame> =
                    expected.iter().copied().filter(|f| *f != SILENCE).collect();
                panic!(
                    "{}: not received as one unbroken run; {} frames that are not silence against {} expected, first difference at {:?}",
                    what,
                    signal.len(),
                    wanted.len(),
                    signal.iter().zip(&wanted).position(|(a, b)| a != b)
                );
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    /// `run_of`, and nothing else was heard: everything outside the run is
    /// silence, and the run starts a chunk.
    fn only(&self, what: &str, expected: &[Frame]) -> usize {
        let (frames, at) = self.run_of(what, expected);
        assert!(
            frames[..at].iter().all(|f| *f == SILENCE)
                && frames[at + expected.len()..].iter().all(|f| *f == SILENCE),
            "{}: something else was heard too",
            what
        );
        assert_eq!(
            at % self.house.chunk,
            0,
            "{}: the first frame starts a chunk",
            what
        );
        at
    }
}

fn find(hay: &[Frame], needle: &[Frame]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    // The run starts where silence ends, or (media that begins with a zero
    // frame) a little before; only those places are tried.
    let first = needle.iter().position(|f| *f != SILENCE)?;
    let mut at = 0;
    while at + needle.len() <= hay.len() {
        let found = hay[at + first..].iter().position(|f| *f == needle[first])? + at;
        if found + needle.len() <= hay.len() && hay[found..found + needle.len()] == *needle {
            return Some(found);
        }
        at = found + 1;
    }
    None
}

// ----- media -----------------------------------------------------------------

fn fixture_path(name: &str) -> String {
    format!(
        "{}/../../fixtures/decode/{name}",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(fixture_path(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn fields(name: &str) -> BTreeMap<String, String> {
    let text = std::fs::read_to_string(fixture_path(&format!("{name}.fields"))).unwrap();
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| l.split_once(" = "))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// The whole decode of `bytes` by the server's decoders.
fn decode(bytes: &[u8], extension: &str) -> (Format, Tags, Vec<f32>) {
    let hint = Hint {
        mime: None,
        extension: Some(extension.to_string()),
    };
    let mut decoder = Decoder::open(Box::new(Cursor::new(bytes.to_vec())), &hint).expect("decodes");
    let format = decoder.format().clone();
    let mut pcm = Vec::new();
    while decoder.read(&mut pcm).expect("decodes") > 0 {}
    (format, decoder.tags().clone(), pcm)
}

/// Interleaved stereo f32 as the frames the server sends: its own rounding
/// to 16 bits (`linein::encode_sample`).
fn s16(pcm: &[f32]) -> Vec<Frame> {
    let one = |x: f32| {
        let mut b = [0u8; 2];
        chorus_server::linein::encode_sample(
            f64::from(x),
            chorus_protocol::SampleFormat::PcmS16Le,
            &mut b,
        );
        i16::from_le_bytes(b)
    };
    pcm.as_chunks::<2>()
        .0
        .iter()
        .map(|f| [one(f[0]), one(f[1])])
        .collect()
}

/// `pcm` (stereo, `from` Hz) through the server's resampler to `to` Hz, in
/// one piece and flushed.
fn resampled(pcm: &[f32], from: u32, to: u32) -> Vec<f32> {
    let mut resampler = Resampler::new(from, to, 2);
    let mut out = Vec::new();
    resampler.process(pcm, &mut out);
    resampler.flush(&mut out);
    out
}

/// Frame `n` of the generated signal: a ramp with a law per sample, never
/// silence, and no two frames alike.
fn ramp_frame(n: usize) -> Frame {
    [1 + (n % 30_011) as i16, -(1 + (n / 30_011) as i16)]
}

fn ramp(from: usize, frames: usize) -> Vec<Frame> {
    (from..from + frames).map(ramp_frame).collect()
}

/// A 16-bit stereo WAV file of `frames`.
fn wav(rate: u32, frames: &[Frame]) -> Vec<u8> {
    let data = (frames.len() * 4) as u32;
    let mut out = Vec::with_capacity(44 + frames.len() * 4);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for f in frames {
        out.extend_from_slice(&f[0].to_le_bytes());
        out.extend_from_slice(&f[1].to_le_bytes());
    }
    out
}

fn as_f32(frames: &[Frame]) -> Vec<f32> {
    frames
        .iter()
        .flat_map(|f| [f32::from(f[0]) / 32_768.0, f32::from(f[1]) / 32_768.0])
        .collect()
}

// ----- one file of each format -------------------------------------------------

/// Load, Start, and the kitchen's session receives the media and nothing
/// else; `Opened` has the duration and the tags; the position advances and
/// ends at the duration; `Ended` comes once.
fn plays_exactly(name: &str, content_type: &str, rate: u32, codec: Codec) {
    let about = fields(name);
    let file = &about["file"];
    let extension = file.rsplit('.').next().unwrap();
    let bytes = fixture(file);
    let (format, _, pcm) = decode(&bytes, extension);
    assert_eq!(format.rate, rate, "{name}: played at the server's own rate");
    let frames: u64 = about["frames"].parse().unwrap();
    let expected = s16(&pcm);
    assert_eq!(expected.len() as u64, frames);
    let duration = frames * 1_000 / u64::from(rate);

    let rig = Rig::new(rate);
    let url = rig
        .media
        .serve(&format!("/music/{file}"), bytes, content_type, Kind::File);
    let info = rig.load(&url);
    assert_eq!(info.uri, url);
    assert_eq!(info.duration_ms, Some(duration), "{name}");
    assert_eq!((info.format.codec, info.format.rate), (codec, rate));
    assert!(info.seekable);
    assert_eq!(
        info.tags,
        Tags {
            title: about.get("title").cloned(),
            artist: about.get("artist").cloned(),
            album: about.get("album").cloned(),
        },
        "{name}"
    );
    assert_eq!(rig.handle.state(), PlayerState::Stopped);
    assert_eq!(rig.handle.duration_ms(), Some(duration));
    // Loaded is silent: nothing was written.
    assert_eq!(rig.house.port.mark(), 0);

    rig.send(Action::Start);
    let mut positions = Vec::new();
    let mut started = false;
    let played_ms = loop {
        match rig.house.reports.recv_timeout(Duration::from_millis(2)) {
            Ok(report) => match report.event {
                Event::Started => started = true,
                Event::Ended { played_ms } => break played_ms,
                other => panic!("{name}: {:?}", other),
            },
            Err(_) => positions.push(rig.handle.position_ms()),
        }
    };
    assert!(started, "{name}: Started comes before Ended");
    assert_eq!(played_ms, duration, "{name}: the position at the end");
    // The position only moves forward, up to the duration (the last look
    // may already be the stopped player's 0).
    if positions.last() == Some(&0) {
        positions.pop();
    }
    assert!(
        positions.windows(2).all(|w| w[0] <= w[1]),
        "{name}: {:?}",
        positions
    );
    assert!(positions.iter().all(|p| *p <= duration), "{name}");
    assert!(
        positions.iter().any(|p| *p > 0),
        "{name}: the position moved"
    );

    rig.only(name, &expected);
    assert_eq!(
        rig.count(|e| matches!(e, Event::Ended { .. })),
        0,
        "Ended came once"
    );
    assert_eq!(rig.handle.state(), PlayerState::Stopped);
    assert_eq!(rig.handle.position_ms(), 0);
    assert_eq!(rig.house.port.underruns(), 0, "{name}");

    // A lossy decode is also held to the fixture's reference decode, at the
    // decode test's own bound (ISO/IEC 11172-4 full accuracy: 2^-14 at any
    // sample) widened by the half step of the 16 bits it was sent in.
    if about.get("match").map(String::as_str) == Some("iso-11172-4-full-accuracy") {
        let reference = fixture(&format!("{name}.ref"));
        let worst = reference
            .as_chunks::<3>()
            .0
            .iter()
            .zip(expected.iter().flatten())
            .map(|(b, got)| {
                let want = f64::from(i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) / 8_388_608.0;
                (want - f64::from(*got) / 32_768.0).abs()
            })
            .fold(0.0, f64::max);
        assert!(
            worst <= 1.0 / 16_384.0 + 1.0 / 65_536.0,
            "{name}: {worst:e}"
        );
    }
}

#[test]
fn a_wav_file_plays_sample_for_sample() {
    plays_exactly("wav-tone44-s16", "audio/wav", 44_100, Codec::Pcm);
}

#[test]
fn a_flac_file_plays_sample_for_sample() {
    plays_exactly("flac-tone44-s16", "audio/flac", 44_100, Codec::Flac);
}

#[test]
fn an_alac_file_plays_sample_for_sample() {
    plays_exactly(
        "alac-tone44-s16-moov-first",
        "audio/mp4",
        44_100,
        Codec::Alac,
    );
}

#[test]
fn an_mp3_file_plays_as_its_reference_decode() {
    plays_exactly("mp3-tone44", "audio/mpeg", 44_100, Codec::Mp3);
}

#[test]
fn an_ogg_vorbis_file_plays_as_its_reference_decode() {
    plays_exactly("vorbis-tone44", "audio/ogg", 44_100, Codec::Vorbis);
}

#[test]
fn an_ogg_opus_file_plays_as_its_reference_decode() {
    plays_exactly("opus-tone44", "audio/ogg", 48_000, Codec::Opus);
}

/// The magnitude of `signal` at `hz`, per sample (a tone of amplitude `a`
/// gives `a / 2`).
fn bin(signal: &[f64], rate: f64, hz: f64) -> f64 {
    let (mut re, mut im) = (0.0f64, 0.0f64);
    for (n, x) in signal.iter().enumerate() {
        let w = 2.0 * std::f64::consts::PI * hz * n as f64 / rate;
        re += x * w.cos();
        im += x * w.sin();
    }
    (re * re + im * im).sqrt() / signal.len() as f64
}

#[test]
fn material_at_44100_on_a_48000_server_plays_at_the_right_speed_and_pitch() {
    let bytes = fixture("wav-tone44-s16.wav");
    let (_, _, pcm) = decode(&bytes, "wav");
    let expected = s16(&resampled(&pcm, 44_100, 48_000));
    // 13230 frames are 0.3 s: 14400 frames at 48 kHz, exactly.
    assert_eq!(expected.len(), 14_400);

    let rig = Rig::new(48_000);
    let url = rig.media.serve("/tone.wav", bytes, "audio/wav", Kind::File);
    let info = rig.load(&url);
    assert_eq!((info.format.rate, info.duration_ms), (44_100, Some(300)));
    rig.send(Action::Start);
    assert_eq!(rig.ended(), 300, "the position at the end is the duration");
    // The same decode through the same resampler, however the engine cut
    // its input into pieces: equal sample for sample, so the duration is
    // exact (and so within one chunk).
    let at = rig.only("the resampled tone", &expected);
    let (frames, _) = rig.run_of("the resampled tone", &expected);
    // The left channel's 1 kHz tone (an eighth of full scale in the fixture:
    // four tones, their mean, at half scale)
    // is still at 1 kHz: not at 1088 Hz, where playing 44.1 kHz samples at
    // 48 kHz would put it, nor at 919 Hz.
    let left: Vec<f64> = frames[at..at + expected.len()]
        .iter()
        .map(|f| f64::from(f[0]) / 32_768.0)
        .collect();
    let at_1k = bin(&left, 48_000.0, 1_000.0);
    assert!((at_1k - 0.0625).abs() < 0.005, "{at_1k}");
    for wrong in [1_000.0 * 48_000.0 / 44_100.0, 1_000.0 * 44_100.0 / 48_000.0] {
        let there = bin(&left, 48_000.0, wrong);
        assert!(there < at_1k / 20.0, "{wrong} Hz: {there} against {at_1k}");
    }
}

// ----- gapless -----------------------------------------------------------------

/// What one gapless join measured, for `docs/measurements/gapless-join-host.md`.
struct Joined {
    expected: usize,
    received: usize,
    /// The largest absolute sample difference, in 16-bit steps, between what
    /// was received and what was expected, within 2048 frames of the join.
    worst: i32,
    /// How far past the join the port's count was when `Boundary` arrived.
    after: u64,
}

/// Plays `a` then `b` gaplessly and holds what was received to `expected`,
/// in which `b`'s first frame is frame `join`. `start` runs between the load
/// and the start.
fn joins(
    pair: &str,
    rig: &Rig,
    a: &str,
    b: &str,
    expected: &[Frame],
    join: usize,
    start: impl Fn(&Rig),
) -> Joined {
    let first = rig.load(a);
    start(rig);
    rig.send(Action::Start);
    rig.queue(b);
    let next = match rig.until("NextOpened", |e| matches!(e, Event::NextOpened(_))) {
        Event::NextOpened(info) => info,
        _ => unreachable!(),
    };
    assert_eq!(next.uri, b);
    let boundary = rig.until("Boundary", |e| matches!(e, Event::Boundary(_)));
    // `Boundary` is reported once the first frame of the next track is in a
    // chunk, and within BOUNDARY_CHUNKS chunks of it.
    let played = rig.house.port.played_frames();
    assert_eq!(boundary, Event::Boundary(next.clone()), "{pair}");
    assert!(
        played > join as u64,
        "{pair}: reported before the join was out"
    );
    let after = played - join as u64;
    assert!(
        after <= BOUNDARY_CHUNKS * rig.house.chunk as u64,
        "{pair}: Boundary came {after} frames after the join"
    );
    // The duration the handle shows is the second track's from here.
    wait("the duration of the second track", || {
        rig.handle.duration_ms() == next.duration_ms
    });
    assert_ne!(first.uri, next.uri);
    let played_ms = rig.ended();
    assert_eq!(
        Some(played_ms),
        next.duration_ms,
        "{pair}: the position at the end"
    );
    assert_eq!(
        rig.seen
            .borrow()
            .iter()
            .filter(|e| matches!(e, Event::Started | Event::Boundary(_) | Event::Ended { .. }))
            .count(),
        3,
        "{pair}: one Started, one Boundary, one Ended: {:?}",
        rig.seen
    );

    let at = rig.only(pair, expected);
    let (frames, _) = rig.run_of(pair, expected);
    let received = frames.iter().filter(|f| **f != SILENCE).count();
    let window = join.saturating_sub(2_048)..(join + 2_048).min(expected.len());
    let worst = window
        .clone()
        .flat_map(|n| {
            let (got, want) = (frames[at + n], expected[n]);
            [0, 1].map(|c| (i32::from(got[c]) - i32::from(want[c])).abs())
        })
        .max()
        .unwrap_or(0);
    assert_eq!(rig.house.port.underruns(), 0, "{pair}");
    Joined {
        expected: expected.iter().filter(|f| **f != SILENCE).count(),
        received,
        worst,
        after,
    }
}

fn said(pair: &str, total: usize, j: &Joined, chunk: usize, note: &str) {
    println!(
        "gapless-join: pair={pair} frames_expected={total} frames_received_not_silent={} of {} \
         max_abs_diff_s16_within_2048_frames_of_join={} boundary_reported_frames_after_join={} \
         chunk_frames={chunk} {note}",
        j.received, j.expected, j.worst, j.after
    );
}

#[test]
fn gapless_two_wav_files_cut_from_one_signal_join_sample_for_sample() {
    // 2.5 s of the ramp, cut at a frame that is no multiple of a chunk.
    let (total, cut) = (120_000usize, 70_007usize);
    let signal = ramp(0, total);
    assert_ne!(cut % 480, 0);
    let rig = Rig::new(48_000);
    let a = rig.media.serve(
        "/a.wav",
        wav(48_000, &signal[..cut]),
        "audio/wav",
        Kind::File,
    );
    let b = rig.media.serve(
        "/b.wav",
        wav(48_000, &signal[cut..]),
        "audio/wav",
        Kind::File,
    );
    let j = joins("wav-ramp48", &rig, &a, &b, &signal, cut, |_| {});
    assert_eq!(
        (j.received, j.worst),
        (total, 0),
        "the uncut signal, exactly"
    );
    said(
        "wav-ramp48",
        total,
        &j,
        rig.house.chunk,
        "(generated; cut at frame 70007)",
    );
}

#[test]
fn gapless_the_flac_pair_joins_sample_for_sample() {
    let about = fields("flac-gap44-a");
    let (total, cut): (usize, usize) = (
        about["gap_total_frames"].parse().unwrap(),
        about["gap_cut_frame"].parse().unwrap(),
    );
    let (bytes_a, bytes_b) = (fixture("flac-gap44-a.flac"), fixture("flac-gap44-b.flac"));
    let mut whole = decode(&bytes_a, "flac").2;
    whole.extend(decode(&bytes_b, "flac").2);
    let original = s16(&whole);
    assert_eq!(original.len(), total);
    // The committed pair is cut at 22050, which is 50 whole chunks at 44.1
    // kHz. Starting 3 ms in (frame 132) puts the join inside a chunk.
    let skip = 3 * 44_100 / 1_000;
    assert_ne!((cut - skip) % 441, 0);
    let rig = Rig::new(44_100);
    let a = rig
        .media
        .serve("/a.flac", bytes_a, "audio/flac", Kind::File);
    let b = rig
        .media
        .serve("/b.flac", bytes_b, "audio/flac", Kind::File);
    let j = joins(
        "flac-gap44",
        &rig,
        &a,
        &b,
        &original[skip..],
        cut - skip,
        |rig| {
            rig.send(Action::Seek { ms: 3 });
            assert_eq!(
                rig.next(),
                Event::SeekDone { ms: 2 },
                "frame 132 is 2.99 ms"
            );
        },
    );
    assert_eq!(
        (j.received, j.worst),
        (j.expected, 0),
        "the uncut signal, exactly"
    );
    said(
        "flac-gap44",
        total - skip,
        &j,
        rig.house.chunk,
        "(fixtures/decode; started at frame 132 so the join is inside a chunk)",
    );
}

/// The gap signal of tools/decode-fixtures/generate.py, from its formula, as
/// `crates/decode/tests/reference_decodes.rs` has it.
fn gap_signal(rate: u32, frames: usize) -> Vec<f32> {
    const LEFT: [f64; 3] = [440.5, 997.3, 3001.7];
    const RIGHT: [f64; 3] = [311.3, 1499.1, 5003.9];
    let mut out = Vec::with_capacity(frames * 2);
    for i in 0..frames {
        let t = i as f64 / f64::from(rate);
        for tones in [LEFT, RIGHT] {
            let s: f64 = tones
                .iter()
                .map(|f| (2.0 * std::f64::consts::PI * f * t).sin())
                .sum::<f64>()
                / 3.0;
            out.push(((s * 0.5 * 32767.0).round() / 32768.0) as f32);
        }
    }
    out
}

/// A lossy pair: the frames received are exactly the two decodes one after
/// the other (so the frame count is the original's and nothing is inserted,
/// dropped or repeated), and the join is as close to the ORIGINAL signal as
/// the decode test demands (`JOIN_MAX` 0.12 within 2048 frames of the join).
fn lossy_pair_joins(pair: &str, extension: &str, content_type: &str) {
    let (bytes_a, bytes_b) = (
        fixture(&format!("{pair}-a.{extension}")),
        fixture(&format!("{pair}-b.{extension}")),
    );
    let mut whole = decode(&bytes_a, extension).2;
    let cut = whole.len() / 2;
    whole.extend(decode(&bytes_b, extension).2);
    let expected = s16(&whole);
    assert_eq!(
        (expected.len(), cut),
        (44_100, 22_050),
        "one second, cut in two"
    );
    let rig = Rig::new(44_100);
    let a = rig.media.serve(
        &format!("/a.{extension}"),
        bytes_a,
        content_type,
        Kind::File,
    );
    let b = rig.media.serve(
        &format!("/b.{extension}"),
        bytes_b,
        content_type,
        Kind::File,
    );
    let j = joins(pair, &rig, &a, &b, &expected, cut, |_| {});
    assert_eq!(
        (j.received, j.worst),
        (j.expected, 0),
        "{pair}: the two decodes, joined"
    );
    let (frames, at) = rig.run_of(pair, &expected);
    let original = gap_signal(44_100, 44_100);
    let worst = (cut - 2_048..cut + 2_048)
        .flat_map(|n| {
            [0, 1].map(|c| {
                (f64::from(frames[at + n][c]) / 32_768.0 - f64::from(original[2 * n + c])).abs()
            })
        })
        .fold(0.0, f64::max);
    assert!(
        worst < 0.12,
        "{pair}: {worst} from the original at the join"
    );
    said(
        pair,
        expected.len(),
        &j,
        rig.house.chunk,
        &format!("(fixtures/decode; against the two decodes; max_abs_diff_from_original_within_2048_frames_of_join={worst:.4} of full scale)"),
    );
}

#[test]
fn gapless_the_mp3_pair_joins_with_the_exact_frame_count() {
    lossy_pair_joins("mp3-gap44", "mp3", "audio/mpeg");
}

#[test]
fn gapless_the_vorbis_pair_joins_with_the_exact_frame_count() {
    lossy_pair_joins("vorbis-gap44", "ogg", "audio/ogg");
}

#[test]
fn gapless_across_a_rate_change_keeps_every_frame() {
    // 44.1 kHz then 48 kHz on a 48 kHz server: the resampler is flushed at
    // the join, so the first track has exactly ceil(n * 48000 / 44100)
    // frames and the second starts on the next frame, untouched.
    let first = ramp(0, 30_000);
    let second = ramp(2_000_000, 24_000);
    let mut expected = s16(&resampled(&as_f32(&first), 44_100, 48_000));
    let join = expected.len();
    assert_eq!(join, (30_000usize * 48_000).div_ceil(44_100));
    expected.extend(&second);
    let rig = Rig::new(48_000);
    let a = rig
        .media
        .serve("/a.wav", wav(44_100, &first), "audio/wav", Kind::File);
    let b = rig
        .media
        .serve("/b.wav", wav(48_000, &second), "audio/wav", Kind::File);
    let j = joins("wav-44-then-48", &rig, &a, &b, &expected, join, |_| {});
    assert_eq!((j.received, j.worst), (j.expected, 0));
    said(
        "wav-44-then-48",
        expected.len(),
        &j,
        rig.house.chunk,
        "(generated; against the first track resampled alone and flushed, then the second)",
    );
}

// ----- the next slot -----------------------------------------------------------

/// Three short tracks no frame of which is in another.
fn three(rig: &Rig) -> ([String; 3], [Vec<Frame>; 3]) {
    let tracks = [
        ramp(0, 14_407),
        ramp(1_000_000, 12_011),
        ramp(2_000_000, 9_613),
    ];
    let urls = ["/one.wav", "/two.wav", "/three.wav"].map(|p| p.to_string());
    let urls = [0, 1, 2].map(|i| {
        rig.media
            .serve(&urls[i], wav(48_000, &tracks[i]), "audio/wav", Kind::File)
    });
    (urls, tracks)
}

#[test]
fn a_next_queued_again_before_the_join_replaces_the_first() {
    let rig = Rig::new(48_000);
    let (urls, tracks) = three(&rig);
    rig.load(&urls[0]);
    rig.queue(&urls[1]);
    assert!(matches!(rig.next(), Event::NextOpened(i) if i.uri == urls[1]));
    rig.queue(&urls[2]);
    assert!(matches!(rig.next(), Event::NextOpened(i) if i.uri == urls[2]));
    rig.send(Action::Start);
    assert!(matches!(
        rig.until("Boundary", |e| matches!(e, Event::Boundary(_))),
        Event::Boundary(i) if i.uri == urls[2]
    ));
    rig.ended();
    let expected: Vec<Frame> = tracks[0].iter().chain(&tracks[2]).copied().collect();
    rig.only("the first track then the third", &expected);
}

#[test]
fn a_cleared_next_does_not_follow() {
    let rig = Rig::new(48_000);
    let (urls, tracks) = three(&rig);
    rig.load(&urls[0]);
    rig.queue(&urls[1]);
    assert!(matches!(rig.next(), Event::NextOpened(_)));
    rig.send(Action::ClearNext);
    rig.send(Action::Start);
    rig.ended();
    rig.only("the first track alone", &tracks[0]);
    assert_eq!(rig.count(|e| matches!(e, Event::Boundary(_))), 0);
}

#[test]
fn a_next_that_is_not_found_fails_by_name_and_the_current_track_ends_normally() {
    let rig = Rig::new(48_000);
    let (urls, tracks) = three(&rig);
    rig.load(&urls[0]);
    rig.send(Action::Start);
    rig.queue(&rig.media.url("/missing.flac"));
    assert_eq!(
        rig.until("NextFailed", |e| matches!(e, Event::NextFailed { .. })),
        Event::NextFailed {
            reason: "http status 404".to_string()
        }
    );
    rig.ended();
    rig.only("the first track alone", &tracks[0]);
    assert_eq!(rig.count(|e| matches!(e, Event::Ended { .. })), 1);
}

#[test]
fn a_skip_flushes_and_starts_the_next_at_once() {
    let rig = Rig::new(48_000);
    let long = ramp(0, 144_000);
    let next = ramp(1_000_000, 12_011);
    let a = rig
        .media
        .serve("/long.wav", wav(48_000, &long), "audio/wav", Kind::File);
    let b = rig
        .media
        .serve("/next.wav", wav(48_000, &next), "audio/wav", Kind::File);
    rig.load(&a);
    rig.queue(&b);
    assert!(matches!(rig.next(), Event::NextOpened(_)));
    rig.send(Action::Start);
    assert_eq!(rig.next(), Event::Started);
    rig.send(Action::SkipToNext);
    assert!(matches!(rig.next(), Event::Boundary(i) if i.uri == b));
    rig.ended();
    let (frames, at) = rig.run_of("the next track, whole", &next);
    // Before it: the start of the long track, unbroken, and then silence.
    let before: Vec<Frame> = frames[..at]
        .iter()
        .copied()
        .filter(|f| *f != SILENCE)
        .collect();
    assert!(!before.is_empty() && before.len() < long.len());
    assert_eq!(
        before,
        long[..before.len()],
        "what went out before the skip"
    );
    assert!(frames[at + next.len()..].iter().all(|f| *f == SILENCE));
}

// ----- pause, stop, seek ---------------------------------------------------------

#[test]
fn a_pause_is_silence_with_the_position_frozen_and_a_resume_loses_nothing() {
    let rig = Rig::new(48_000);
    let track = ramp(0, 72_000);
    let url = rig
        .media
        .serve("/a.wav", wav(48_000, &track), "audio/wav", Kind::File);
    rig.load(&url);
    rig.send(Action::Start);
    assert_eq!(rig.next(), Event::Started);
    wait("some of it", || rig.handle.position_ms() >= 100);
    rig.send(Action::Pause);
    wait("paused", || rig.handle.state() == PlayerState::Paused);
    let quiet = 10 * rig.house.chunk;
    wait("silence while paused", || {
        let frames = rig.heard.frames();
        frames[frames.len() - quiet..].iter().all(|f| *f == SILENCE)
    });
    // Frozen: the same position 100 ms later, and it is where the audio
    // stopped (what the client has, in ms).
    let held = rig.handle.position_ms();
    thread::sleep(Duration::from_millis(100));
    assert_eq!(rig.handle.position_ms(), held);
    let out = rig.heard.frames().iter().filter(|f| **f != SILENCE).count() as u64;
    assert_eq!(held, out * 1_000 / 48_000);
    assert!((100..1_500).contains(&held));
    rig.send(Action::Resume);
    assert_eq!(rig.ended(), 1_500);
    // Nothing lost and nothing twice: without the silence it is the track.
    let (frames, _) = rig.run_of("the part after the pause", &track[out as usize..]);
    let signal: Vec<Frame> = frames.iter().copied().filter(|f| *f != SILENCE).collect();
    assert!(
        signal == track,
        "{} frames against {}",
        signal.len(),
        track.len()
    );
    let first = find(&frames, &track[..out as usize]).expect("the part before the pause");
    let rest = find(&frames, &track[out as usize..]).expect("the part after it");
    assert!(frames[first + out as usize..rest]
        .iter()
        .all(|f| *f == SILENCE));
    assert!(rest - (first + out as usize) >= quiet);
}

#[test]
fn a_stop_returns_to_the_start_and_a_start_plays_it_again_from_there() {
    let rig = Rig::new(48_000);
    let track = ramp(0, 60_000);
    let url = rig
        .media
        .serve("/a.wav", wav(48_000, &track), "audio/wav", Kind::File);
    rig.load(&url);
    // One open is two requests: the decoder looks at the first bytes and
    // goes back to the start, which the fetcher does with a range request.
    let one_open = rig.media.accepts();
    rig.send(Action::Start);
    assert_eq!(rig.next(), Event::Started);
    wait("some of it", || rig.handle.position_ms() >= 100);
    rig.send(Action::Stop);
    wait("stopped", || rig.handle.state() == PlayerState::Stopped);
    assert_eq!(rig.handle.position_ms(), 0);
    assert_eq!(rig.house.port.queued(), 0, "the port was flushed");
    rig.send(Action::Start);
    // The URI stayed loaded; it is fetched again.
    assert!(matches!(rig.next(), Event::Opened(i) if i.uri == url));
    assert_eq!(rig.next(), Event::Started);
    assert_eq!(rig.ended(), 1_250);
    assert_eq!(rig.media.accepts(), 2 * one_open);
    let (frames, at) = rig.run_of("the whole track, the second time", &track);
    let before: Vec<Frame> = frames[..at]
        .iter()
        .copied()
        .filter(|f| *f != SILENCE)
        .collect();
    assert!(!before.is_empty() && before.len() < track.len());
    assert_eq!(before, track[..before.len()], "the first, stopped, play");
}

#[test]
fn a_seek_on_a_server_with_ranges_lands_on_the_frame_asked_for() {
    // Tolerance: none. `Decoder::seek` returns the frame asked for (it
    // decodes forward from the packet before it), so the first frame out
    // after a seek to T ms is frame T * rate / 1000 of the media.
    let rig = Rig::new(44_100);
    let bytes = fixture("flac-gap44-a.flac");
    let whole = s16(&decode(&bytes, "flac").2);
    let url = rig.media.serve("/a.flac", bytes, "audio/flac", Kind::File);
    rig.load(&url);
    rig.send(Action::Start);
    assert_eq!(rig.next(), Event::Started);
    rig.send(Action::Seek { ms: 250 });
    assert_eq!(rig.next(), Event::SeekDone { ms: 250 });
    assert_eq!(rig.next(), Event::Started, "audible again from there");
    assert_eq!(rig.ended(), 500);
    let target = 250 * 44_100 / 1_000;
    let (frames, at) = rig.run_of("from the target to the end", &whole[target..]);
    assert_eq!(at % rig.house.chunk, 0);
    // What came before the seek is the start of the media and no frame of
    // it is the frame before the target followed by the target.
    let before: Vec<Frame> = frames[..at]
        .iter()
        .copied()
        .filter(|f| *f != SILENCE)
        .collect();
    let start: Vec<Frame> = whole
        .iter()
        .copied()
        .filter(|f| *f != SILENCE)
        .take(before.len())
        .collect();
    assert!(!before.is_empty() && before == start);
    assert!(frames[at + whole.len() - target..]
        .iter()
        .all(|f| *f == SILENCE));

    // A generated WAV, sought while stopped: the start plays from there.
    let track = ramp(0, 48_000);
    let rig = Rig::new(48_000);
    let url = rig
        .media
        .serve("/b.wav", wav(48_000, &track), "audio/wav", Kind::File);
    rig.load(&url);
    rig.send(Action::Seek { ms: 700 });
    assert_eq!(rig.next(), Event::SeekDone { ms: 700 });
    rig.send(Action::Start);
    assert_eq!(rig.next(), Event::Started);
    assert_eq!(rig.ended(), 1_000);
    rig.only("from 700 ms", &track[33_600..]);
}

#[test]
fn a_seek_on_a_source_without_ranges_is_refused_by_name_and_changes_nothing() {
    let rig = Rig::new(48_000);
    let track = ramp(0, 48_000);
    let url = rig
        .media
        .serve("/a.wav", wav(48_000, &track), "audio/wav", Kind::NoRanges);
    let info = rig.load(&url);
    assert!(!info.seekable);
    rig.send(Action::Start);
    assert_eq!(rig.next(), Event::Started);
    rig.send(Action::Seek { ms: 500 });
    assert_eq!(
        rig.next(),
        Event::SeekRefused {
            reason: "unsupported: seek on a non-seekable source".to_string()
        }
    );
    rig.ended();
    rig.only("the whole track, unbroken", &track);
}

// ----- refusals ------------------------------------------------------------------

#[test]
fn aac_fails_by_name_and_never_plays_a_sample() {
    let rig = Rig::new(48_000);
    for (file, content_type) in [
        ("aac-adts.aac", "audio/aac"),
        ("aac-in-mp4.m4a", "audio/mp4"),
    ] {
        let url = rig
            .media
            .serve(&format!("/{file}"), fixture(file), content_type, Kind::File);
        rig.send(Action::Load {
            uri: url,
            mime: None,
        });
        let failed = Event::Failed {
            reason: "unsupported: aac".to_string(),
        };
        assert_eq!(rig.next(), failed, "{file}");
        // A start tries again and fails the same way.
        rig.send(Action::Start);
        assert_eq!(rig.next(), failed, "{file}");
        assert_eq!(rig.handle.state(), PlayerState::Stopped);
    }
    assert_eq!(rig.house.port.mark(), 0, "nothing was written");
    thread::sleep(Duration::from_millis(100));
    assert!(rig.heard.frames().iter().all(|f| *f == SILENCE));
}

#[test]
fn a_loopback_url_is_refused_by_name_and_no_connection_is_made() {
    // The production policy: `allow_loopback` false.
    let rig = Rig::with_policy(48_000, false);
    let url = rig.media.serve(
        "/a.wav",
        wav(48_000, &ramp(0, 4_800)),
        "audio/wav",
        Kind::File,
    );
    rig.send(Action::Load {
        uri: url,
        mime: None,
    });
    assert_eq!(
        rig.next(),
        Event::Failed {
            reason: "refused: loopback address 127.0.0.1".to_string()
        }
    );
    // The server's own audio listener, by any name for this machine.
    rig.send(Action::Load {
        uri: format!("http://{}/", rig.house.address),
        mime: None,
    });
    assert!(matches!(rig.next(), Event::Failed { reason } if reason.starts_with("refused: ")));
    assert_eq!(rig.media.accepts(), 0, "no connection was made");
    // Neither http nor https.
    rig.send(Action::Load {
        uri: "file:///etc/hostname".to_string(),
        mime: None,
    });
    assert!(matches!(rig.next(), Event::Failed { .. }));
}

#[test]
fn a_policy_that_allows_loopback_still_refuses_the_servers_own_port() {
    let rig = Rig::new(48_000);
    rig.send(Action::Load {
        uri: format!("http://{}/state", rig.house.address),
        mime: None,
    });
    let port = rig.house.address.rsplit(':').next().unwrap();
    assert_eq!(
        rig.next(),
        Event::Failed {
            reason: format!("refused: the server's own port {port} at 127.0.0.1")
        }
    );
}

// ----- a source that stalls ------------------------------------------------------

#[test]
fn a_stop_gets_through_a_stalled_source_within_the_bound() {
    let rig = Rig::new(48_000);
    let track = ramp(0, 144_000);
    let bytes = wav(48_000, &track);
    // 0.4 s of audio arrives, then nothing.
    let sent = 19_200usize;
    let url = rig.media.serve(
        "/stalls.wav",
        bytes,
        "audio/wav",
        Kind::Stall {
            after: 44 + sent * 4,
        },
    );
    rig.load(&url);
    rig.send(Action::Start);
    assert_eq!(rig.next(), Event::Started);
    // Everything that arrived went out, and the ring ran dry: the player's
    // thread is waiting on the socket now. The dry ticks are counted.
    wait("the ring runs dry", || {
        rig.house.port.queued() == 0 && rig.house.port.underruns() > 0
    });
    assert!(rig.house.port.played_frames() <= sent as u64);
    let asked = Instant::now();
    rig.send(Action::Stop);
    wait("stopped", || rig.handle.state() == PlayerState::Stopped);
    let took = asked.elapsed();
    assert!(took <= STALL_BOUND, "the stop took {:?}", took);
    println!("stall: stop through a stalled read took {:?}", took);
    assert_eq!(
        rig.count(|e| matches!(e, Event::Failed { .. } | Event::Ended { .. })),
        0,
        "{:?}",
        rig.seen
    );

    // A server that accepts and never answers: a new Load does not wait for
    // the read timeout either.
    let mute = rig
        .media
        .serve("/mute.wav", Vec::new(), "audio/wav", Kind::Mute);
    let good = rig.media.serve(
        "/good.wav",
        wav(48_000, &ramp(0, 4_800)),
        "audio/wav",
        Kind::File,
    );
    let accepted = rig.media.accepts();
    rig.send(Action::Load {
        uri: mute,
        mime: None,
    });
    wait("the connection to the mute server", || {
        rig.media.accepts() > accepted
    });
    thread::sleep(Duration::from_millis(150));
    let asked = Instant::now();
    rig.send(Action::Load {
        uri: good.clone(),
        mime: None,
    });
    assert!(matches!(rig.next(), Event::Opened(i) if i.uri == good));
    let took = asked.elapsed();
    assert!(took <= STALL_BOUND, "the load took {:?}", took);
    println!(
        "stall: load through a server that never answers took {:?}",
        took
    );
}

// ----- a live stream -------------------------------------------------------------

#[test]
fn a_live_stream_with_icy_metadata_plays_until_it_closes_and_reports_its_title() {
    let bytes = fixture("mp3-tone44.mp3");
    let expected = s16(&decode(&bytes, "mp3").2);
    let rig = Rig::new(44_100);
    let url = rig.media.serve(
        "/radio",
        bytes,
        "audio/mpeg",
        Kind::Live {
            metaint: 1_024,
            title: "Evening Programme".to_string(),
        },
    );
    let info = rig.load(&url);
    assert_eq!(info.duration_ms, None, "a stream has no length");
    assert!(!info.seekable);
    assert_eq!(info.station.as_deref(), Some("Example Radio"));
    assert_eq!(rig.handle.duration_ms(), None);
    rig.send(Action::Start);
    assert_eq!(
        rig.until("Title", |e| matches!(e, Event::Title(_))),
        Event::Title("Evening Programme".to_string())
    );
    // It ends when the server closes, at the last frame: the metadata was
    // taken out of the bytes, so the decode is the file's own.
    assert_eq!(rig.ended(), 300);
    rig.only("the stream", &expected);
    assert_eq!(
        rig.count(|e| matches!(e, Event::Title(_))),
        1,
        "the title was said once"
    );
}

// ----- sessions ------------------------------------------------------------------

fn sessions(rig: &Rig) -> (PlayerSessions, Arc<Mutex<Vec<String>>>) {
    let log: Arc<Mutex<Vec<String>>> = Arc::default();
    let into = Arc::clone(&log);
    let sessions = PlayerSessions::new(
        Arc::clone(&rig.house.state),
        Arc::clone(&rig.house.players),
        Box::new(move |line| into.lock().unwrap().push(line.to_string())),
    );
    (sessions, log)
}

fn request(owner: &str, target: &str, uri: &str) -> PlayRequest {
    PlayRequest {
        owner: owner.to_string(),
        target: target.to_string(),
        uri: uri.to_string(),
        mime: None,
        via: "upnp".to_string(),
        epoch: 7,
        metadata: Metadata::default(),
    }
}

/// Pumps the sessions until a report `wanted` holds of.
fn pump(sessions: &PlayerSessions, rig: &Rig, what: &str, wanted: impl Fn(&Event) -> bool) {
    let deadline = Instant::now() + LIMIT;
    loop {
        if let Some(report) = sessions.pump(&rig.house.reports, Duration::from_millis(20)) {
            if wanted(&report.event) {
                return;
            }
        }
        assert!(
            Instant::now() < deadline,
            "{}: not within {:?}",
            what,
            LIMIT
        );
    }
}

#[test]
fn a_session_takes_a_player_shows_what_plays_and_gives_it_back_at_the_end() {
    let rig = Rig::new(44_100);
    // The rig's own take is undone: the session issues it.
    rig.house
        .state
        .apply(r#"{"v":2,"t":"take","target":"kitchen","source":"none"}"#)
        .unwrap();
    let state = &rig.house.state;
    let (sessions, log) = sessions(&rig);
    let bytes = fixture("flac-tone44-s16.flac");
    let expected = s16(&decode(&bytes, "flac").2);
    let url = rig.media.serve("/a.flac", bytes, "audio/flac", Kind::File);

    // An unknown target is refused by the model's own words and holds no player.
    let refused = sessions
        .play(&request("room:attic", "attic", &url))
        .unwrap_err();
    assert!(refused.starts_with("target: "), "{refused}");
    assert_eq!(rig.house.players.owner_of(0), None);

    assert_eq!(
        sessions.play(&request("room:kitchen", "kitchen", &url)),
        Ok(0)
    );
    assert_eq!(
        rig.house.players.owner_of(0).as_deref(),
        Some("room:kitchen")
    );
    assert_eq!(sessions.player_of("room:kitchen"), Some(0));
    assert_eq!(state.player_group("p0").as_deref(), Some("kitchen"));
    let shown = state.now_playing("kitchen").expect("a record at once");
    assert_eq!(
        (shown.state, shown.via.as_str()),
        (PlayState::Buffering, "upnp")
    );

    pump(&sessions, &rig, "Opened", |e| matches!(e, Event::Opened(_)));
    let shown = state.now_playing("kitchen").unwrap();
    assert_eq!(
        shown.title.as_deref(),
        Some("chorus fixture flac-tone44-s16")
    );
    assert_eq!(shown.artist.as_deref(), Some("the generator"));
    assert_eq!(shown.album.as_deref(), Some("fixtures/decode"));
    assert_eq!(shown.duration_ms, Some(300));
    pump(&sessions, &rig, "Started", |e| matches!(e, Event::Started));
    assert_eq!(
        state.now_playing("kitchen").unwrap().state,
        PlayState::Playing
    );
    assert!(state.encoded_state().contains(
        r#""source":"player:p0","now_playing":{"title":"chorus fixture flac-tone44-s16""#
    ));

    pump(&sessions, &rig, "Ended", |e| {
        matches!(e, Event::Ended { .. })
    });
    assert_eq!(state.player_group("p0"), None, "the group's source is none");
    assert_eq!(state.now_playing("kitchen"), None);
    assert!(!state.encoded_state().contains("now_playing"));
    assert_eq!(
        rig.house.players.owner_of(0),
        None,
        "the player was given back"
    );
    wait("the player is idle", || rig.handle.is_idle());
    rig.only("the file", &expected);

    // A failure: named, kept, logged, and the player is given back.
    let aac = rig
        .media
        .serve("/a.aac", fixture("aac-adts.aac"), "audio/aac", Kind::File);
    assert_eq!(
        sessions.play(&request("room:kitchen", "kitchen", &aac)),
        Ok(0)
    );
    pump(&sessions, &rig, "Failed", |e| {
        matches!(e, Event::Failed { .. })
    });
    assert_eq!(
        sessions.last_failure(0).as_deref(),
        Some("unsupported: aac")
    );
    assert_eq!(state.player_group("p0"), None);
    assert_eq!(rig.house.players.owner_of(0), None);
    assert!(log
        .lock()
        .unwrap()
        .contains(&"player p0 failed: unsupported: aac".to_string()));
}

#[test]
fn a_busy_player_cannot_be_stolen_and_is_released_when_its_group_plays_something_else() {
    let rig = Rig::new(48_000);
    rig.house
        .state
        .apply(r#"{"v":2,"t":"take","target":"kitchen","source":"none"}"#)
        .unwrap();
    let state = &rig.house.state;
    let (sessions, _log) = sessions(&rig);
    let track = ramp(0, 240_000);
    let url = rig
        .media
        .serve("/long.wav", wav(48_000, &track), "audio/wav", Kind::File);
    assert_eq!(
        sessions.play(&request("room:kitchen", "kitchen", &url)),
        Ok(0)
    );
    pump(&sessions, &rig, "Started", |e| matches!(e, Event::Started));

    // The study asks for a player: there is one, and it is the kitchen's.
    let before = state.encoded_state();
    assert_eq!(
        sessions.play(&request("room:study", "study", &url)),
        Err("players: no free player: all 1 are in use".to_string())
    );
    // And the model refuses a take of the busy player by another group.
    let stolen = state
        .apply(r#"{"v":2,"t":"take","target":"study","source":"player:p0"}"#)
        .unwrap_err();
    assert!(
        stolen
            .detail
            .starts_with("player 'p0' is playing in group 'kitchen'"),
        "{}",
        stolen.detail
    );
    assert_eq!(state.encoded_state(), before, "nothing changed");
    assert_eq!(
        rig.house.players.owner_of(0).as_deref(),
        Some("room:kitchen")
    );

    // A pause is shown.
    assert!(sessions.set_paused("room:kitchen", 7, true));
    assert_eq!(
        state.now_playing("kitchen").unwrap().state,
        PlayState::Paused
    );
    assert!(sessions.set_paused("room:kitchen", 7, false));
    assert_eq!(
        state.now_playing("kitchen").unwrap().state,
        PlayState::Playing
    );

    // Somebody else changes the kitchen's source: the player stops and is
    // given back at the next pump, though no report said anything.
    state
        .apply(r#"{"v":2,"t":"take","target":"kitchen","source":"none"}"#)
        .unwrap();
    assert_eq!(
        sessions.pump(&rig.house.reports, Duration::from_millis(20)),
        None
    );
    assert_eq!(rig.house.players.owner_of(0), None);
    wait("the player is unloaded", || rig.handle.is_idle());
    assert_eq!(rig.house.port.queued(), 0, "the port was flushed");
    assert_eq!(state.now_playing("kitchen"), None);
    // The player is free again, for anyone.
    assert_eq!(rig.house.players.acquire("room:study"), Some(0));
}
