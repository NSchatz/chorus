//! Goal 16, the server plumbing: a player port's audio reaches a real client
//! session sample for sample, with no network media and no decoder anywhere.
//!
//! The server is assembled in this process from the pieces `main.rs` wires
//! (the room model, one stream slot, the conductor, the audio thread's
//! `serve_slots`, the client pool and its v2 sessions), so the test holds
//! the producer's side of the port, exactly as a player thread does. The
//! listener is the Linux client's own session code (`common::Player`), on a
//! loopback socket. Every command goes through `ControlState::apply`, which
//! is how a renderer issues `take` in process.
//!
//! The signal is whole 16-bit values written as f32 at full scale 1.0, so
//! what arrives can be compared exactly: a frame is `(v, -v)` with `v` never
//! 0, and silence is `(0, 0)`.
//!
//! What is asserted is never how long something took. Where the outcome of
//! a step depends on scheduling (how much had gone out when a pause or a
//! flush landed), the test reads the port's own counter and requires the
//! client to have received exactly that; a "no gap" claim is made only for
//! audio that was wholly in the ring before it was released, where it holds
//! by construction.

mod common;

use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chorus_audio::{MonotonicTimeline, StreamFormat};
use chorus_control::rooms::{NowPlaying, PlayState};
use chorus_control::zones::{Zone, Zones};
use chorus_hostctl::ThreadRegistry;
use chorus_protocol::v2::Playback;
use chorus_server::clients::ClientPool;
use chorus_server::conductor::{self, Conductor};
use chorus_server::control::ControlState;
use chorus_server::player::{self, PlayerDriver};
use chorus_server::playerport::PlayerPort;
use chorus_server::router::Router;
use chorus_server::serve::ServeParams;
use chorus_server::session::{load_identity, IdentitySource, Offer, SessionContext};
use chorus_server::slots::{serve_slots, SlotCommand, SlotEvent, SlotMedia};
use chorus_server::source::PcmSource;

use common::Player as Listener;

const RATE: u32 = 48_000;
/// 10 ms chunks: 480 frames.
const CHUNK_US: u64 = 10_000;
const CHUNK: usize = 480;
const LIMIT: Duration = Duration::from_secs(30);
/// A value no track has: what was flushed while held, which nobody may hear.
const JUNK: f32 = 32_000.0 / 32_768.0;

/// The server's pieces, running in this process.
struct House {
    address: String,
    state: Arc<ControlState>,
    port: Arc<PlayerPort>,
    keep: Arc<AtomicBool>,
    /// Kept so the audio thread's channels stay open for the run.
    _streams: mpsc::Sender<Box<dyn PcmSource>>,
    _events: mpsc::Receiver<SlotEvent>,
}

impl Drop for House {
    fn drop(&mut self) {
        self.keep.store(false, Ordering::SeqCst);
    }
}

/// One room, `kitchen`, whose endpoint is `endpoint`; one stream slot; one
/// player, `p0`, whose producer side is handed back in place of a thread.
fn house(endpoint: &str) -> House {
    let format = StreamFormat::new(RATE, 2, "pcm_s16le").unwrap();
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
    // One slot for two rooms: the study, second, starts with no source.
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

    // The conductor: what the slot plays, and which slot each session hears.
    {
        let conductor =
            Conductor::new(Arc::clone(&state), Arc::clone(&router), Some(slot_commands))
                .with_players(1);
        let keep = Arc::clone(&keep);
        thread::spawn(move || conductor::run(conductor, keep));
    }
    // The audio thread.
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
    // The client threads and the acceptor that hands them connections.
    let (identity, adoptions) = load_identity(&IdentitySource::Ephemeral, "player-port-test")
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
        ready,
        session,
    );
    // As `main.rs` does: nothing is accepted until every client thread has
    // reported itself (a thread that finds nobody listening for that stops).
    for _ in 0..pool.threads() {
        came_up
            .recv_timeout(LIMIT)
            .expect("every client thread comes up");
    }
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
        keep,
        _streams: streams,
        _events: events,
    }
}

/// One chunk as received: its sequence and its PCM.
type Chunk = (u32, Vec<u8>);

/// Everything a listener received, chunk by chunk.
#[derive(Clone, Default)]
struct Heard(Arc<Mutex<Vec<Chunk>>>);

impl Heard {
    /// Read `listener`'s chunks on a thread of their own until `keep` stops:
    /// a session nobody reads is one whose queue fills.
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

    /// The left channel of every frame received, in order. Checks on the way
    /// that no chunk is missing, every chunk is whole, and every frame is
    /// `(v, -v)`.
    fn left(&self) -> Vec<i16> {
        let chunks = self.0.lock().unwrap();
        let mut out = Vec::with_capacity(chunks.len() * CHUNK);
        for (i, (sequence, data)) in chunks.iter().enumerate() {
            if i > 0 {
                assert_eq!(
                    *sequence,
                    chunks[i - 1].0.wrapping_add(1),
                    "a chunk is missing before sequence {}",
                    sequence
                );
            }
            assert_eq!(data.len(), CHUNK * 4, "chunk {} is not whole", sequence);
            for frame in data.as_chunks::<4>().0 {
                let l = i16::from_le_bytes([frame[0], frame[1]]);
                let r = i16::from_le_bytes([frame[2], frame[3]]);
                assert_eq!(r, -l, "a frame is (v, -v): chunk {}", sequence);
                out.push(l);
            }
        }
        out
    }

    /// The frames received that are not silence.
    fn signal(&self) -> usize {
        self.left().iter().filter(|v| **v != 0).count()
    }

    /// Whole chunks of silence at the end of what was received.
    fn silent_tail(&self) -> usize {
        let left = self.left();
        left.rchunks(CHUNK)
            .take_while(|c| c.len() == CHUNK && c.iter().all(|v| *v == 0))
            .count()
    }

    fn chunks(&self) -> usize {
        self.0.lock().unwrap().len()
    }
}

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

/// `frames` frames of a track whose frame `n` is `(v, -v)`, `v = sign *
/// (base + n)`: whole 16-bit values, never 0, as f32. Each track of a test
/// has values no other has, so where it was received is never ambiguous.
fn track(sign: i32, base: i32, frames: usize) -> (Vec<f32>, Vec<i16>) {
    let values: Vec<i16> = (0..frames as i32)
        .map(|n| i16::try_from(sign * (base + n)).expect("a 16-bit value"))
        .collect();
    let samples = values
        .iter()
        .flat_map(|v| {
            let x = f32::from(*v) / 32_768.0;
            [x, -x]
        })
        .collect();
    (samples, values)
}

/// Write all of `samples`; the ring has room for everything this test writes
/// at once, so nothing waits.
fn write(port: &PlayerPort, samples: &[f32]) {
    assert_eq!(port.write(samples), samples.len() / 2, "the ring had room");
}

fn take(house: &House, target: &str, source: &str) -> Result<String, String> {
    house
        .state
        .apply(&format!(
            r#"{{"v":2,"t":"take","target":"{}","source":"{}"}}"#,
            target, source
        ))
        .map_err(|r| format!("{}: {}", r.field, r.detail))
}

/// Where `needle` (a run of non-silent frames) starts in `left`, requiring
/// it to be there as ONE unbroken run: no gap, no repeat, nothing between.
fn unbroken(left: &[i16], needle: &[i16], what: &str) -> usize {
    assert!(!needle.is_empty(), "{}", what);
    let at = left
        .windows(needle.len())
        .position(|w| w == needle)
        .unwrap_or_else(|| panic!("{}: not received as one unbroken run", what));
    at
}

#[test]
fn a_player_ports_audio_reaches_a_client_sample_for_sample() {
    let endpoint = common::fresh_id("kitchen-speaker");
    let house = house(&endpoint);
    let port = Arc::clone(&house.port);
    let listener = Listener::connect(&house.address, &endpoint, 0);
    let heard = Heard::from(listener, Arc::clone(&house.keep));
    wait("the session's first chunks", || heard.chunks() >= 3);

    // The kitchen takes the player. Until something is written it is silence.
    take(&house, "kitchen", "player:p0").expect("the take is applied");
    assert_eq!(house.state.player_group("p0").as_deref(), Some("kitchen"));
    let mut expected: Vec<i16> = Vec::new();

    // --- start: 30 000 frames, in the ring before any of it is released ---
    let (a0, a0_values) = track(1, 1, 30_000);
    port.set_paused(true);
    write(&port, &a0);
    port.set_paused(false);
    expected.extend(&a0_values);
    wait("the first audio", || heard.signal() >= 2_000);

    // --- pause: silence, nothing drained; more is written meanwhile -------
    port.set_paused(true);
    let (a1, a1_values) = track(-1, 1, 5_000);
    write(&port, &a1);
    expected.extend(&a1_values);
    wait("silence while held", || heard.silent_tail() >= 10);
    let at_pause = port.played_frames();
    // How much had gone out when the hold landed is the scheduler's; that
    // the hold landed inside what was written is not (625 ms of audio).
    assert!(
        (2_000..35_000).contains(&at_pause),
        "held with {} played",
        at_pause
    );
    assert_eq!(
        heard.signal() as u64,
        at_pause,
        "the client has exactly what the audio thread took, and no more arrives while held"
    );
    assert_eq!(
        port.queued() as u64,
        35_000 - at_pause,
        "nothing was lost to the hold"
    );
    let silent_before_resume = heard.silent_tail();
    assert!(silent_before_resume >= 10);
    // --- resume: the next frame out is the next frame written -------------
    port.set_paused(false);
    wait("everything written so far", || {
        port.played_frames() == 35_000
    });

    // --- underrun: the ring runs dry mid-stream; silence, counted ---------
    let before = port.underruns();
    wait("an underrun is counted", || port.underruns() > before);
    wait("silence while dry", || heard.silent_tail() >= 3);
    assert_eq!(heard.signal(), 35_000, "silence, not a repeat");
    // The producer catches up (one write of less than a slice lands whole).
    let (a2, a2_values) = track(-1, 10_001, 2_000);
    write(&port, &a2);
    expected.extend(&a2_values);
    wait("the audio after the underrun", || {
        port.played_frames() == 37_000
    });

    // --- flush in mid-play: what had not gone out never does --------------
    let (a3, a3_values) = track(-1, 20_001, 6_000);
    port.set_paused(true);
    write(&port, &a3);
    port.set_paused(false);
    wait("some of it", || heard.signal() >= 37_000 + CHUNK);
    let went_out = port.flush();
    assert!((37_000 + CHUNK as u64..=43_000).contains(&went_out));
    expected.extend(&a3_values[..(went_out - 37_000) as usize]);
    assert_eq!(
        (port.mark(), port.played_frames(), port.queued()),
        (0, 0, 0)
    );
    // --- flush while held: none of it is ever heard -----------------------
    port.set_paused(true);
    let junk = [JUNK, -JUNK].repeat(3_000);
    write(&port, &junk);
    assert_eq!(port.flush(), 0);
    port.set_paused(false);
    wait("silence after the flushes", || heard.silent_tail() >= 5);
    assert_eq!(
        heard.signal() as u64,
        went_out,
        "exactly what went out before the flush, and nothing of the old audio after it"
    );

    // --- two tracks back to back, the second ending in part of a chunk ----
    let (t1, t1_values) = track(-1, 27_001, 4_137);
    let (t2, t2_values) = track(1, 1, 5_000);
    let underruns_before = port.underruns();
    port.set_paused(true);
    write(&port, &t1);
    let boundary = port.mark();
    write(&port, &t2);
    port.finish();
    port.set_paused(false);
    assert_eq!(boundary, 4_137);
    let total = boundary + 5_000;
    wait("both tracks", || port.played_frames() == total);
    assert_eq!(port.mark(), total, "everything written has gone out");
    wait("the end of the media", || heard.silent_tail() >= 5);
    assert_eq!(
        port.underruns(),
        underruns_before,
        "running dry after finish() is the end, not an underrun"
    );
    expected.extend(&t1_values);
    expected.extend(&t2_values);

    // --- the whole run, sample for sample ---------------------------------
    house.keep.store(false, Ordering::SeqCst);
    let left = heard.left();
    let signal: Vec<i16> = left.iter().copied().filter(|v| *v != 0).collect();
    assert_eq!(signal.len(), expected.len(), "frames received");
    assert!(
        signal == expected,
        "the frames received are the frames written, in order, each once; first difference at {:?}",
        signal.iter().zip(&expected).position(|(a, b)| a != b)
    );
    assert!(!left.contains(&32_000), "the flushed audio was heard");

    // Start: audio that was in the ring when it was released comes out with
    // no gap, from the first frame of a chunk.
    let first = unbroken(
        &left,
        &expected[..at_pause as usize],
        "the start, up to the hold",
    );
    assert_eq!(left[first], a0_values[0]);
    assert_eq!(first % CHUNK, 0, "the first frame out starts a chunk");
    // The hold: only silence between the last frame before it and the first
    // after it, at least the ten chunks seen, and what follows is unbroken.
    let resumed = unbroken(
        &left,
        &expected[at_pause as usize..35_000],
        "after the hold",
    );
    let held = &left[first + at_pause as usize..resumed];
    assert!(held.iter().all(|v| *v == 0), "silence while held");
    assert!(
        held.len() >= silent_before_resume * CHUNK - CHUNK,
        "{}",
        held.len()
    );
    assert_eq!(resumed % CHUNK, 0, "a resume starts a chunk");
    // The underrun's catch-up and the flushed track's played part, each
    // unbroken.
    unbroken(&left, &a2_values, "after the underrun");
    unbroken(
        &left,
        &a3_values[..(went_out - 37_000) as usize],
        "before the flush",
    );
    // Gapless: the two tracks are one unbroken run, no gap and no overlap,
    // and the second track's first frame is frame `boundary` of what the
    // audio thread took since the flush: the count the producer watches.
    let both: Vec<i16> = t1_values.iter().chain(&t2_values).copied().collect();
    let tracks = unbroken(&left, &both, "the two tracks");
    assert_eq!(tracks % CHUNK, 0);
    assert_eq!(left[tracks + boundary as usize], t2_values[0]);
    assert_eq!(
        left[tracks + boundary as usize - 1],
        *t1_values.last().unwrap()
    );
    // The end: 9137 frames are 19 chunks and 17 frames; the last chunk is
    // those 17 frames and silence, played, not held back.
    let end = tracks + both.len();
    assert_eq!(both.len() % CHUNK, 17);
    assert_eq!(end % CHUNK, 17);
    assert!(left[end..end + CHUNK - 17].iter().all(|v| *v == 0));
    assert_eq!(*left[..end].last().unwrap(), *t2_values.last().unwrap());
}

#[test]
fn a_take_naming_a_player_this_server_does_not_run_is_refused_by_name() {
    let house = house("nobody");
    let before = house.state.encoded_state();
    let refused = take(&house, "kitchen", "player:p1").unwrap_err();
    assert!(
        refused.starts_with("source: there is no player 'p1' on this server; its players are p0"),
        "{}",
        refused
    );
    assert_eq!(house.state.encoded_state(), before, "nothing was applied");
    // A server with no player says so.
    house.state.set_players(0);
    let none = take(&house, "kitchen", "player:p0").unwrap_err();
    assert!(none.contains("started without --players"), "{}", none);
    house.state.set_players(1);
    // One player, one group: the study taking it is refused naming the
    // kitchen. (The study has no slot here; the refusal is the model's, and
    // comes first.)
    take(&house, "kitchen", "player:p0").unwrap();
    let busy = take(&house, "study", "player:p0").unwrap_err();
    assert!(
        busy.starts_with("source: player 'p0' is playing in group 'kitchen'"),
        "{}",
        busy
    );
}

#[test]
fn what_is_playing_reaches_the_state_and_a_paused_room_shows_paused() {
    let house = house("kitchen-panel");
    let subscriber = house.state.fanout().subscribe();
    let song = |state| NowPlaying {
        title: Some("Morning Light".to_string()),
        artist: Some("The Example Quartet".to_string()),
        album: None,
        art_url: None,
        duration_ms: Some(215_000),
        state,
        via: "upnp".to_string(),
    };
    // Not before the group plays a player source.
    let early = house
        .state
        .set_now_playing("kitchen", Some(song(PlayState::Playing)))
        .unwrap_err();
    assert_eq!(early.field, "group");
    take(&house, "kitchen", "player:p0").unwrap();
    let controller = |house: &House| {
        house
            .state
            .snapshot()
            .rooms
            .iter()
            .find(|r| r.id == "kitchen")
            .unwrap()
            .controller_state
            .playback
    };
    assert_eq!(controller(&house), Playback::Playing);
    while subscriber.try_recv().is_ok() {}

    assert_eq!(
        house
            .state
            .set_now_playing("kitchen", Some(song(PlayState::Playing))),
        Ok(true)
    );
    let told = subscriber.try_recv().expect("every subscriber is told");
    assert!(
        told.contains(r#""source":"player:p0","now_playing":{"title":"Morning Light","artist":"The Example Quartet","album":null,"art_url":null,"duration_ms":215000,"state":"playing","via":"upnp"}"#),
        "{}",
        told
    );
    assert_eq!(told.as_str(), house.state.encoded_state());
    // The same again: nobody is told anything.
    assert_eq!(
        house
            .state
            .set_now_playing("kitchen", Some(song(PlayState::Playing))),
        Ok(false)
    );
    assert!(subscriber.try_recv().is_err());

    assert_eq!(
        house
            .state
            .set_now_playing("kitchen", Some(song(PlayState::Paused))),
        Ok(true)
    );
    assert_eq!(controller(&house), Playback::Paused);
    assert_eq!(
        house.state.now_playing("kitchen").map(|n| n.state),
        Some(PlayState::Paused)
    );
    assert_eq!(
        house
            .state
            .set_now_playing("kitchen", Some(song(PlayState::Buffering))),
        Ok(true)
    );
    assert_eq!(controller(&house), Playback::Playing);
    // Stopping (the source is no longer a player) clears it.
    take(&house, "kitchen", "none").unwrap();
    assert_eq!(house.state.now_playing("kitchen"), None);
    assert_eq!(house.state.player_group("p0"), None);
    assert!(!house.state.encoded_state().contains("now_playing"));
}

/// A driver handed to `player::spawn` is what a player's thread runs: this
/// one writes a known signal into its port, as a decode loop will.
struct Scripted(Vec<f32>);

impl PlayerDriver for Scripted {
    fn run(&mut self, port: Arc<PlayerPort>, keep: &AtomicBool) {
        port.set_paused(true);
        player::write_all(&port, &self.0, keep, &|| false);
        port.finish();
        port.set_paused(false);
        while keep.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(5));
        }
    }
}

#[test]
fn a_driver_on_a_player_thread_is_heard_through_the_same_path() {
    let endpoint = common::fresh_id("kitchen-speaker");
    let house = house(&endpoint);
    let (samples, values) = track(1, 100, 3 * CHUNK + 41);
    let registry = Arc::new(ThreadRegistry::new());
    let (ready, came_up) = mpsc::channel::<()>();
    let threads = player::spawn(
        std::slice::from_ref(&house.port),
        vec![Box::new(Scripted(samples))],
        &house.keep,
        &registry,
        &ready,
    );
    drop(ready);
    assert_eq!((threads, came_up.iter().count()), (1, 1));
    assert_eq!(registry.snapshot()[0].role, "player-0");

    let listener = Listener::connect(&house.address, &endpoint, 0);
    let heard = Heard::from(listener, Arc::clone(&house.keep));
    take(&house, "kitchen", "player:p0").unwrap();
    wait("the driver's audio", || {
        house.port.played_frames() == values.len() as u64 && heard.silent_tail() >= 3
    });
    house.keep.store(false, Ordering::SeqCst);
    let left = heard.left();
    let at = unbroken(&left, &values, "the driver's signal");
    assert_eq!(at % CHUNK, 0);
    assert_eq!(left.iter().filter(|v| **v != 0).count(), values.len());
    assert_eq!(house.port.underruns(), 0);
}
