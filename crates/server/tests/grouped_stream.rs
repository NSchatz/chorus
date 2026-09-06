//! Two clients, one stream, one timeline - and the exchange sharing the
//! connection with the audio.
//!
//! These run anywhere: no audio device, no privilege. The bytes go over real
//! loopback TCP sockets and are decoded with the client's own receiver, so a
//! framing decision is tested once rather than modelled twice.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use chorus_audio::{MonotonicTimeline, StreamFormat};
use chorus_client_linux::receive::{Received, Receiver};
use chorus_protocol::{encode, Message, TimeSync};
use chorus_server::serve::{serve_stream, ServeParams};
use chorus_server::stream::{read_requests, write_outbound, Fanout, FanoutSink};

const CHUNK_US: u64 = 20_000;
const CHUNK_NS: u64 = CHUNK_US * 1_000;

fn params() -> ServeParams {
    ServeParams {
        format: StreamFormat::new(48_000, 2, "pcm_s16le").unwrap(),
        chunk_us: CHUNK_US,
        // Faster than real time, so a test that wants a few hundred chunks
        // does not take a few hundred chunk durations.
        rate_skew_ppm: 500_000,
    }
}

/// What one attached client received.
struct Attached {
    chunks: Vec<(u32, u64, Vec<u8>)>,
    replies: Vec<TimeSync>,
}

/// A server with one stream, one timeline, and a fanout anything can attach
/// to.
struct GroupedServer {
    address: String,
    fanout: Arc<Fanout>,
    timeline: MonotonicTimeline,
    keep: Arc<AtomicBool>,
    ready: mpsc::Receiver<usize>,
}

impl GroupedServer {
    /// Bind, and accept connections into the fanout until told to stop.
    ///
    /// No chunk is produced until [`GroupedServer::produce`] is called, so a
    /// test can attach every client it wants BEFORE the stream starts and know
    /// they are all on it.
    fn start() -> GroupedServer {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let address = listener.local_addr().unwrap().to_string();
        let fanout = Arc::new(Fanout::new());
        // A timeline whose origin is emphatically not zero. A timeline created
        // and immediately used has an origin of a few microseconds, and an
        // arithmetic bug that ignores the origin is then nearly invisible.
        let timeline = MonotonicTimeline::new();
        while timeline.now_ns() < 5 * CHUNK_NS {
            thread::sleep(Duration::from_millis(1));
        }
        let keep = Arc::new(AtomicBool::new(true));
        let (ready_tx, ready) = mpsc::channel();

        {
            let fanout = Arc::clone(&fanout);
            let keep = Arc::clone(&keep);
            thread::spawn(move || {
                while let Ok((stream, _)) = listener.accept() {
                    let _ = stream.set_nodelay(true);
                    let reader = stream.try_clone().expect("a connection splits");
                    let _ = reader.set_read_timeout(Some(Duration::from_millis(20)));
                    let (tx, rx) = fanout.subscribe();
                    let attached = fanout.subscribers();
                    {
                        let keep = Arc::clone(&keep);
                        let mut sink = stream;
                        thread::spawn(move || {
                            let go = move || keep.load(Ordering::SeqCst);
                            let _ = write_outbound(&mut sink, timeline, &rx, &go);
                        });
                    }
                    {
                        let keep = Arc::clone(&keep);
                        let mut reader = reader;
                        thread::spawn(move || {
                            let go = move || keep.load(Ordering::SeqCst);
                            read_requests(&mut reader, timeline, &tx, &go);
                        });
                    }
                    if ready_tx.send(attached).is_err() {
                        return;
                    }
                }
            });
        }

        GroupedServer {
            address,
            fanout,
            timeline,
            keep,
            ready,
        }
    }

    /// Wait until `n` clients are attached.
    fn wait_for(&self, n: usize) {
        while self.fanout.subscribers() < n {
            self.ready
                .recv_timeout(Duration::from_secs(5))
                .expect("a client attaches");
        }
    }

    /// Cut `chunks` chunks from one source, on one timeline, and fan them out.
    fn produce(&self, chunks: usize) -> thread::JoinHandle<()> {
        let fanout = Arc::clone(&self.fanout);
        let timeline = self.timeline;
        let keep = Arc::clone(&self.keep);
        let params = params();
        let total = params.format.frames_in(CHUNK_US).unwrap() * chunks * params.format.frame_len();
        thread::spawn(move || {
            let mut left = total;
            let mut fed = 0usize;
            // A ramp, so a dropped, repeated or reordered byte shows up in the
            // concatenation rather than hiding in silence.
            let mut read = move |buf: &mut [u8]| -> std::io::Result<usize> {
                let n = buf.len().min(left);
                for (i, b) in buf[..n].iter_mut().enumerate() {
                    *b = ((fed + i) % 251) as u8;
                }
                fed += n;
                left -= n;
                Ok(n)
            };
            let mut sink = FanoutSink::new(fanout);
            let go = move || keep.load(Ordering::SeqCst);
            let _ = serve_stream(params, timeline, &mut read, &mut sink, &go);
        })
    }

    fn stop(&self) {
        self.keep.store(false, Ordering::SeqCst);
    }
}

/// Connect, read for `for_at_least` chunks or until `deadline`, and decode
/// everything with the client's own receiver.
fn attach_and_collect(
    address: &str,
    want_chunks: usize,
    requests: usize,
    deadline: Duration,
) -> (TcpStream, Attached) {
    let stream = TcpStream::connect(address).expect("the server is listening");
    stream
        .set_read_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    let mut writer = stream.try_clone().unwrap();
    let mut reader = stream.try_clone().unwrap();

    let handle = thread::spawn(move || {
        let mut receiver = Receiver::new();
        let mut out = Attached {
            chunks: Vec::new(),
            replies: Vec::new(),
        };
        let mut scratch = vec![0u8; 65_536];
        let started = Instant::now();
        while started.elapsed() < deadline {
            if out.chunks.len() >= want_chunks && out.replies.len() >= requests {
                break;
            }
            let n = match reader.read(&mut scratch) {
                Ok(0) => break,
                Ok(n) => n,
                Err(_) => continue,
            };
            for event in receiver.push(&scratch[..n]).expect("the stream is framed") {
                match event {
                    Received::Chunk { chunk, .. } => out.chunks.push((
                        chunk.sequence,
                        chunk.timestamp_ns,
                        chunk.audio_data.clone(),
                    )),
                    Received::TimeSync(reply) => out.replies.push(reply),
                    other => panic!("unexpected event {:?}", other),
                }
            }
        }
        out
    });

    // The requests, if any, go up the same connection the audio comes down.
    for i in 0..requests {
        let request = TimeSync {
            t0_ns: 1_000 + i as u64,
            t1_ns: 0,
            t2_ns: 0,
            t3_ns: 0,
        };
        writer
            .write_all(&encode(&Message::TimeSync(request)).unwrap())
            .expect("the request goes up the connection");
        writer.flush().unwrap();
        thread::sleep(Duration::from_millis(5));
    }

    let collected = handle.join().expect("the reader thread finishes");
    (stream, collected)
}

// -------------------------------------------------------------------------
// AC-8: two clients, one timeline, contiguous sequences.
// -------------------------------------------------------------------------

#[test]
fn two_clients_at_once_get_one_timeline_and_a_contiguous_run_each() {
    let server = GroupedServer::start();
    let address = server.address.clone();

    let a_address = address.clone();
    let a = thread::spawn(move || attach_and_collect(&a_address, 120, 0, Duration::from_secs(10)));
    let b_address = address.clone();
    let b = thread::spawn(move || attach_and_collect(&b_address, 120, 0, Duration::from_secs(10)));

    server.wait_for(2);
    let producer = server.produce(200);

    let (_a_stream, first) = a.join().expect("client a finishes");
    let (_b_stream, second) = b.join().expect("client b finishes");
    server.stop();
    let _ = producer.join();

    assert!(
        first.chunks.len() >= 120 && second.chunks.len() >= 120,
        "both clients have to be served: {} and {}",
        first.chunks.len(),
        second.chunks.len()
    );

    // Each one's sequence numbers are a contiguous run.
    for (name, attached) in [("a", &first), ("b", &second)] {
        for pair in attached.chunks.windows(2) {
            assert_eq!(
                pair[1].0,
                pair[0].0 + 1,
                "client {} saw a gap between sequence {} and {}",
                name,
                pair[0].0,
                pair[1].0
            );
        }
    }

    // ONE timeline: the same presentation timestamp for the same content, and
    // the same content for the same sequence.
    let mut compared = 0usize;
    for (sequence, timestamp_ns, audio) in &first.chunks {
        if let Some((_, other_ts, other_audio)) =
            second.chunks.iter().find(|(s, _, _)| s == sequence)
        {
            assert_eq!(
                timestamp_ns, other_ts,
                "sequence {} is due at {} ns for one client and {} ns for the other",
                sequence, timestamp_ns, other_ts
            );
            assert_eq!(audio, other_audio, "sequence {} carried different audio", sequence);
            compared += 1;
        }
    }
    assert!(
        compared >= 100,
        "only {} sequences were seen by both clients, which is not two clients on one stream",
        compared
    );

    // And it is a timeline rather than a counter: the spacing is exactly one
    // chunk duration, start to start, whatever the pacing did.
    for pair in first.chunks.windows(2) {
        assert_eq!(
            pair[1].1 - pair[0].1,
            CHUNK_NS,
            "the timestamps are not one chunk apart"
        );
    }
    // The origin is the server's own monotonic reading when the stream
    // started, not zero, so an arithmetic bug that ignores it is visible.
    assert!(
        first.chunks[0].1 >= 5 * CHUNK_NS,
        "the stream's origin is {} ns, which no chunk count could be told apart from",
        first.chunks[0].1
    );
}

#[test]
fn a_client_that_joins_late_is_on_the_same_timeline_from_where_it_joined() {
    let server = GroupedServer::start();
    let address = server.address.clone();

    let early_address = address.clone();
    let early =
        thread::spawn(move || attach_and_collect(&early_address, 200, 0, Duration::from_secs(10)));
    server.wait_for(1);
    let producer = server.produce(400);

    thread::sleep(Duration::from_millis(300));
    let (_late_stream, late) = attach_and_collect(&address, 40, 0, Duration::from_secs(10));
    let (_early_stream, first) = early.join().expect("the early client finishes");
    server.stop();
    let _ = producer.join();

    assert!(!late.chunks.is_empty(), "the late client got nothing");
    assert!(
        late.chunks[0].0 > 0,
        "the late client started at sequence 0, so it was given a stream of its own"
    );
    for pair in late.chunks.windows(2) {
        assert_eq!(pair[1].0, pair[0].0 + 1, "the late client's run has a gap");
    }
    let mut compared = 0;
    for (sequence, timestamp_ns, _) in &late.chunks {
        if let Some((_, other_ts, _)) = first.chunks.iter().find(|(s, _, _)| s == sequence) {
            assert_eq!(timestamp_ns, other_ts);
            compared += 1;
        }
    }
    assert!(compared > 0, "the two clients shared no content at all");
}

// -------------------------------------------------------------------------
// AC-9: the exchange is answered on the connection carrying the audio.
// -------------------------------------------------------------------------

#[test]
fn a_time_sync_is_answered_on_the_audio_connection_while_audio_keeps_flowing() {
    let server = GroupedServer::start();
    let address = server.address.clone();

    let client = thread::spawn(move || attach_and_collect(&address, 150, 6, Duration::from_secs(10)));
    server.wait_for(1);
    let producer = server.produce(400);
    let (_stream, attached) = client.join().expect("the client finishes");
    let before = server.timeline.now_ns();
    server.stop();
    let _ = producer.join();

    assert_eq!(attached.replies.len(), 6, "every request has to be answered");
    assert!(
        attached.chunks.len() >= 150,
        "audio has to keep flowing while the exchange happens: {} chunks",
        attached.chunks.len()
    );

    let mut last_t1 = 0u64;
    for (i, reply) in attached.replies.iter().enumerate() {
        // The client's own transmit stamp, echoed untouched.
        assert_eq!(reply.t0_ns, 1_000 + i as u64);
        // The two the server took, both nanoseconds from its monotonic
        // timeline: nonzero, ordered, inside the run, and never going
        // backwards between exchanges.
        assert!(reply.t1_ns > 0, "t1 is unset on reply {}", i);
        assert!(reply.t2_ns >= reply.t1_ns, "the server transmitted before it received");
        assert!(reply.t2_ns <= before, "a stamp from after the run ended");
        assert!(reply.t1_ns >= last_t1, "the server's stamps went backwards");
        last_t1 = reply.t1_ns;
        // t3 is the client's receive stamp on the client's clock. The server
        // cannot know it and does not invent one.
        assert_eq!(reply.t3_ns, 0);
    }

    // Completed by the client, the exchange is four monotonic-nanosecond
    // timestamps and yields a round trip and an offset.
    let t3 = server.timeline.now_ns();
    let completed = TimeSync {
        t3_ns: t3,
        ..attached.replies[0]
    };
    assert!(completed.rtt_ns() > 0 || completed.t2_ns == completed.t1_ns);
    for stamp in [completed.t0_ns, completed.t1_ns, completed.t2_ns, completed.t3_ns] {
        assert!(stamp > 0, "every timestamp of the exchange has to be set");
    }

    // The audio and the exchange really did share one connection: the chunks
    // are still a contiguous run with the replies interleaved among them.
    for pair in attached.chunks.windows(2) {
        assert_eq!(pair[1].0, pair[0].0 + 1);
    }
}

#[test]
fn two_clients_exchange_independently_on_their_own_connections() {
    let server = GroupedServer::start();
    let address = server.address.clone();

    let a_address = address.clone();
    let a = thread::spawn(move || attach_and_collect(&a_address, 100, 4, Duration::from_secs(10)));
    let b_address = address.clone();
    let b = thread::spawn(move || attach_and_collect(&b_address, 100, 4, Duration::from_secs(10)));
    server.wait_for(2);
    let producer = server.produce(400);

    let (_a_stream, first) = a.join().expect("client a finishes");
    let (_b_stream, second) = b.join().expect("client b finishes");
    server.stop();
    let _ = producer.join();

    assert_eq!(first.replies.len(), 4);
    assert_eq!(second.replies.len(), 4);
    assert!(first.chunks.len() >= 100 && second.chunks.len() >= 100);
    // Both sets of stamps come off the same server timeline, which is what
    // makes two clients' offsets comparable at all.
    let a_max = first.replies.iter().map(|r| r.t2_ns).max().unwrap();
    let b_max = second.replies.iter().map(|r| r.t2_ns).max().unwrap();
    let a_min = first.replies.iter().map(|r| r.t1_ns).min().unwrap();
    let b_min = second.replies.iter().map(|r| r.t1_ns).min().unwrap();
    assert!(
        a_max >= b_min && b_max >= a_min,
        "the two clients' server stamps do not overlap in time, so they are not one timeline"
    );
}
