//! Every thread this server will ever use to serve a client, created before
//! the socket is bound.
//!
//! # Why the threads are made in advance
//!
//! The host contract is graded on the scheduling report, and the report is a
//! comparison against `/proc/self/task` taken at one instant. A thread created
//! after that instant is a thread the report never saw, and
//! `crates/hostctl/src/lib.rs` is explicit about what that costs: "A report
//! nobody can check is not a safety property". Worse than unseen: a thread
//! created by `std::thread::spawn` starts under the default `pthread_attr_t`,
//! whose `inheritsched` is `PTHREAD_INHERIT_SCHED`, so it takes the creating
//! thread's scheduling policy and priority. A connection handler spawned from a
//! thread holding `SCHED_FIFO` is itself `SCHED_FIFO`, at the same priority,
//! and `sched(7)` says what that risks: "A nonblocking infinite loop in a
//! thread scheduled under the SCHED_FIFO, SCHED_RR, or SCHED_DEADLINE policy
//! can potentially block all other threads from accessing the CPU forever."
//!
//! So this pool exists to make the process's thread population a FIXED,
//! DECLARED quantity: `2 * max_clients` threads, all created before the report
//! is taken, all registered in the [`ThreadRegistry`] by themselves, and none
//! created afterwards however many clients come and go. A slot is handed a
//! connection, serves it, and goes back to the pool; nothing about a new client
//! changes what the kernel would say if the report were taken again.
//!
//! # What a slot is
//!
//! Two threads, because the two directions of one connection have to be
//! independent: the writer parks on this client's outbound queue and the reader
//! parks on its socket, and neither may wait on the other. They are handed one
//! connection at a time. When both halves are finished the slot returns to the
//! free list and can take the next client.
//!
//! # The session comes first
//!
//! Every connection is a protocol v2 session ([`crate::session`]). The reader
//! runs the handshake, with a timeout, on the slot's own thread, so a slow
//! peer holds its slot and never the acceptor; the writer waits for the
//! session and writes nothing before it, so no frame leaves outside a record.
//! Neither costs a thread.
//!
//! # A full pool refuses rather than grows
//!
//! `max_clients` is a ceiling on connections, and it is the same argument the
//! bounded outbound queue in [`crate::stream`] makes: a server that grows a
//! resource per client has no bound on that resource at all. A connection
//! arriving with every slot busy is closed and named in the log, which an
//! operator can see, rather than served by a thread nothing declared.

use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use chorus_protocol::v2::session::SecureWriter;

use chorus_audio::MonotonicTimeline;
use chorus_hostctl::ThreadRegistry;

use chorus_protocol::v2::Message;

use crate::hostreport::register_ordinary_thread;
use crate::session::{establish, route_controller, Greeting, SessionContext};
use crate::stream::{read_requests_and_upstream, write_outbound, Outbound, SUBSCRIBER_QUEUE_LIMIT};

/// How often a writer waiting for its session looks up to see whether it is
/// still wanted: the same interval the request reader's socket timeout uses.
const GREETING_WAKE: Duration = Duration::from_millis(200);

/// One connection's shared state: whether it is still live, and how many of
/// its two halves have finished with it.
#[derive(Debug)]
struct SlotLife {
    live: AtomicBool,
    finished: AtomicUsize,
}

impl SlotLife {
    fn idle() -> SlotLife {
        SlotLife {
            live: AtomicBool::new(false),
            finished: AtomicUsize::new(0),
        }
    }
}

/// What the writer half of a slot is handed.
struct WriterJob {
    sink: TcpStream,
    inbox: Receiver<Outbound>,
    life: Arc<SlotLife>,
    /// The session, once the reader has it up. Nothing is written on the
    /// connection before it arrives, so no frame leaves outside a record.
    greeting: Receiver<Greeting>,
}

/// What the reader half of a slot is handed.
struct ReaderJob {
    source: TcpStream,
    out: SyncSender<Outbound>,
    life: Arc<SlotLife>,
    greeting: SyncSender<Greeting>,
    peer: SocketAddr,
}

/// One client's worth of capacity: two threads that exist whether or not a
/// client is attached.
struct Slot {
    to_writer: SyncSender<WriterJob>,
    to_reader: SyncSender<ReaderJob>,
    life: Arc<SlotLife>,
}

/// Every client thread this process runs.
///
/// Built once, before the scheduling report and before the listening socket is
/// bound. It is not `Sync` on purpose: the free list is a receiver, so exactly
/// one thread hands connections out, which is the accepting thread.
pub struct ClientPool {
    slots: Vec<Slot>,
    free: Receiver<usize>,
    max_clients: usize,
    busy: Arc<AtomicUsize>,
}

/// How many slots are serving a connection right now, readable from any
/// thread (the pool itself belongs to the acceptor).
#[derive(Debug, Clone)]
pub struct BusySlots(Arc<AtomicUsize>);

impl BusySlots {
    /// Slots whose two halves have not both finished with their connection.
    pub fn count(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }

    /// Wait until every slot has finished, or until `limit` has passed.
    /// Returns whether they all finished.
    ///
    /// A run that is stopping sets the pool's `keep` to false first: every
    /// writer then writes what is already queued for it (the end of the
    /// stream included) and stops, and every reader stops within its read
    /// timeout. Waiting here is what lets that queued end reach a client
    /// before the process exits under it.
    pub fn wait_idle(&self, limit: Duration) -> bool {
        let deadline = std::time::Instant::now() + limit;
        while self.count() > 0 {
            if std::time::Instant::now() >= deadline {
                return false;
            }
            thread::sleep(Duration::from_millis(10));
        }
        true
    }
}

impl ClientPool {
    /// Create every client thread, and park each one waiting for a connection.
    ///
    /// Each thread registers itself in `registry` and then sends one unit down
    /// `ready`, so the caller can wait until the process's whole thread
    /// population exists before it takes a scheduling report of it. The sender
    /// is dropped as soon as that unit is sent, so a caller that reads `ready`
    /// to exhaustion learns both how many threads came up and that no more are
    /// coming.
    ///
    /// The threads inherit the scheduling policy of the thread that calls this,
    /// which is why the caller is the one that must not be holding a real-time
    /// one.
    ///
    /// `session` is what every slot's reader runs the protocol v2 handshake
    /// with before it serves a request (see [`crate::session`]).
    pub fn spawn(
        max_clients: usize,
        timeline: MonotonicTimeline,
        keep: Arc<AtomicBool>,
        registry: Arc<ThreadRegistry>,
        ready: Sender<()>,
        session: Arc<SessionContext>,
    ) -> ClientPool {
        let (free_tx, free) = mpsc::channel::<usize>();
        let mut slots = Vec::with_capacity(max_clients);
        let busy = Arc::new(AtomicUsize::new(0));

        for index in 0..max_clients {
            let life = Arc::new(SlotLife::idle());
            let (to_writer, jobs) = mpsc::sync_channel::<WriterJob>(1);
            {
                let keep = Arc::clone(&keep);
                let registry = Arc::clone(&registry);
                let ready = ready.clone();
                let free_tx = free_tx.clone();
                let busy = Arc::clone(&busy);
                thread::spawn(move || {
                    register_ordinary_thread(&format!("client-writer-{}", index), &registry);
                    if ready.send(()).is_err() {
                        return;
                    }
                    drop(ready);
                    for job in jobs {
                        let WriterJob {
                            sink,
                            inbox,
                            life,
                            greeting,
                        } = job;
                        let going = {
                            let keep = Arc::clone(&keep);
                            let life = Arc::clone(&life);
                            move || keep.load(Ordering::SeqCst) && life.live.load(Ordering::SeqCst)
                        };
                        // Wait for the reader's session, or for the slot to
                        // be released because the handshake failed or the
                        // run stopped.
                        let session = loop {
                            match greeting.recv_timeout(GREETING_WAKE) {
                                Ok(g) => break Some(g),
                                Err(RecvTimeoutError::Timeout) if going() => continue,
                                Err(_) => break None,
                            }
                        };
                        if let Some(Greeting {
                            sealer, messages, ..
                        }) = session
                        {
                            let mut secure = SecureWriter::new(sink, sealer);
                            if messages.iter().try_for_each(|m| secure.send(m)).is_ok() {
                                let _ = write_outbound(&mut secure, timeline, &inbox, &going);
                            }
                        } else {
                            drop(sink);
                        }
                        drop(inbox);
                        if !release(&life, index, &free_tx, &busy) {
                            return;
                        }
                    }
                });
            }

            let (to_reader, jobs) = mpsc::sync_channel::<ReaderJob>(1);
            {
                let session = Arc::clone(&session);
                let keep = Arc::clone(&keep);
                let registry = Arc::clone(&registry);
                let ready = ready.clone();
                let free_tx = free_tx.clone();
                let busy = Arc::clone(&busy);
                thread::spawn(move || {
                    register_ordinary_thread(&format!("client-reader-{}", index), &registry);
                    if ready.send(()).is_err() {
                        return;
                    }
                    drop(ready);
                    for job in jobs {
                        let ReaderJob {
                            source,
                            out,
                            life,
                            greeting,
                            peer,
                        } = job;
                        let going = {
                            let keep = Arc::clone(&keep);
                            let life = Arc::clone(&life);
                            move || keep.load(Ordering::SeqCst) && life.live.load(Ordering::SeqCst)
                        };
                        // The handshake runs here, on this slot's own thread,
                        // so a slow peer never stalls the acceptor.
                        if let Some((mut reader, mut hello)) = establish(&source, peer, &session) {
                            // Where it starts and what its greeting says,
                            // from the room model (crate::router): the
                            // room_volume is sealed before the first chunk,
                            // which the router only starts handing this
                            // session's queue once it is registered.
                            // (goal 14) The speaker is listed and marked
                            // present first, so its start is its room's.
                            session.session_up(&hello);
                            let start = session.start_for(&hello);
                            if let Some(volume) = start.room_volume {
                                hello.messages.push(Message::RoomVolume(volume));
                            }
                            // Then the room's sound (goal 12), before any
                            // audio too.
                            if let Some(sound) = &start.sound {
                                hello.messages.push(Message::Sound(sound.clone()));
                            }
                            if let Some(state) = &start.controller_state {
                                hello.messages.push(Message::ControllerState(state.clone()));
                            }
                            let id = session.router.register(
                                &hello.endpoint_id,
                                hello.roles,
                                out.clone(),
                                &start,
                            );
                            session.router.set_link(id, hello.features, peer.ip());
                            if let Some(control) = &session.control {
                                // Anything committed between the start and
                                // the registration reaches it this way.
                                control.wake_conductor();
                            }
                            route_controller(&mut reader, &session, &hello, id);
                            let hello_id = hello.endpoint_id.clone();
                            if greeting.send(hello).is_ok() {
                                (session.on_session)();
                                // A line-in's upstream chunks, into its port.
                                let line_ins = session.line_ins.clone();
                                let mut upstream = |m: &chorus_protocol::Message| {
                                    if let (
                                        Some(line_ins),
                                        chorus_protocol::Message::AudioChunk(c),
                                    ) = (&line_ins, m)
                                    {
                                        line_ins.chunk(id, &c.audio_data);
                                    }
                                };
                                // (goal 14) Every time the reader looks up
                                // from its socket (a frame, or its read
                                // timeout) it tops up the firmware transfer
                                // travelling in this session, if there is
                                // one: the sender is this thread, not a new
                                // one (crate::firmware).
                                let looking = || {
                                    session.pump_firmware(id);
                                    going()
                                };
                                read_requests_and_upstream(
                                    &mut reader,
                                    timeline,
                                    &out,
                                    &looking,
                                    &mut upstream,
                                );
                            }
                            if let Some(line_ins) = &session.line_ins {
                                line_ins.session_ended(id);
                            }
                            session.router.unregister(id);
                            session.session_down(&hello_id, id);
                            // The TV relay's streams of this session end on
                            // the conductor's next pass (`crate::tvrelay`).
                            if let Some(control) = &session.control {
                                control.wake_conductor();
                            }
                        }
                        drop(greeting);
                        drop(out);
                        drop(source);
                        if !release(&life, index, &free_tx, &busy) {
                            return;
                        }
                    }
                });
            }

            slots.push(Slot {
                to_writer,
                to_reader,
                life,
            });
            // Every slot starts free. The free list is what `attach` draws
            // from, so a slot that is not on it cannot be handed a client.
            if free_tx.send(index).is_err() {
                break;
            }
        }

        ClientPool {
            slots,
            free,
            max_clients,
            busy,
        }
    }

    /// A handle on how many slots are serving, for the thread that decides
    /// when the process may exit.
    pub fn busy(&self) -> BusySlots {
        BusySlots(Arc::clone(&self.busy))
    }

    /// How many threads this pool created.
    ///
    /// Two per slot, fixed for the life of the process. The caller counts on
    /// this to know when its thread population is complete.
    pub fn threads(&self) -> usize {
        self.slots.len() * 2
    }

    /// The ceiling on connections served at once.
    pub fn max_clients(&self) -> usize {
        self.max_clients
    }

    /// Hand one connection to a free slot, or refuse it.
    ///
    /// The connection's outbound queue is made here, bounded at
    /// [`SUBSCRIBER_QUEUE_LIMIT`], and attached to a stream only once its
    /// session is up (`crate::router`, from the slot's reader), so neither a
    /// refused connection nor a failed handshake ever leaves a subscriber in a
    /// fanout with nothing draining it. `false` means every slot is busy: the
    /// sockets are dropped, which closes the connection, and the caller is
    /// expected to say so out loud.
    pub fn attach(&self, sink: TcpStream, source: TcpStream, peer: SocketAddr) -> bool {
        let index = match self.free.try_recv() {
            Ok(index) => index,
            Err(_) => return false,
        };
        let slot = &self.slots[index];
        self.busy.fetch_add(1, Ordering::SeqCst);
        slot.life.finished.store(0, Ordering::SeqCst);
        slot.life.live.store(true, Ordering::SeqCst);
        let (out, inbox) = mpsc::sync_channel::<Outbound>(SUBSCRIBER_QUEUE_LIMIT);
        let (greeting_tx, greeting_rx) = mpsc::sync_channel::<Greeting>(1);
        let handed_over = slot
            .to_writer
            .send(WriterJob {
                sink,
                inbox,
                life: Arc::clone(&slot.life),
                greeting: greeting_rx,
            })
            .is_ok()
            && slot
                .to_reader
                .send(ReaderJob {
                    source,
                    out,
                    life: Arc::clone(&slot.life),
                    greeting: greeting_tx,
                    peer,
                })
                .is_ok();
        if !handed_over {
            // A slot thread is gone, which can only mean it panicked. The
            // connection is not served and the slot is not returned to the free
            // list: a slot with one half missing would serve the next client
            // in one direction only, and half a connection is worse than none.
            slot.life.live.store(false, Ordering::SeqCst);
            self.busy.fetch_sub(1, Ordering::SeqCst);
        }
        handed_over
    }
}

/// Finish with a connection, and return the slot to the free list once both
/// halves have.
///
/// Returns whether the pool is still there to return it to.
fn release(life: &Arc<SlotLife>, index: usize, free: &Sender<usize>, busy: &AtomicUsize) -> bool {
    // Whichever half finishes first tells the other one to stop: the writer is
    // parked on a queue nothing will fill again, and the reader on a socket
    // that has nothing more to say.
    life.live.store(false, Ordering::SeqCst);
    if life.finished.fetch_add(1, Ordering::SeqCst) == 1 {
        busy.fetch_sub(1, Ordering::SeqCst);
        free.send(index).is_ok()
    } else {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::{Duration, Instant};

    use chorus_audio::StreamFormat;
    use chorus_protocol::v2::noise::Keypair;
    use chorus_protocol::v2::session::{connect, Identity, SecureReader};
    use chorus_protocol::v2::{roles, Capabilities, Codec, Hello, Message as V2Message};
    use chorus_protocol::{decode_frame, encode, FrameOutcome, Message, TimeSync};

    use crate::session::Offer;

    fn context() -> Arc<SessionContext> {
        let offer = Offer::new(&StreamFormat::new(48_000, 2, "pcm_s16le").unwrap(), 20_000)
            .expect("stereo has a channel map");
        Arc::new(SessionContext::quiet(
            Identity {
                id: "test-server".to_string(),
                keypair: Keypair::from_secret([11; 32]),
            },
            offer,
        ))
    }

    /// The endpoint's side of a session over `stream`: the handshake, then
    /// `hello` and `capabilities`, as the Linux client sends them.
    fn v2_endpoint(stream: TcpStream) -> (SecureReader<TcpStream>, SecureWriter<TcpStream>) {
        let mut handshake = stream.try_clone().unwrap();
        let me = Identity {
            id: "test-endpoint".to_string(),
            keypair: Keypair::from_secret([12; 32]),
        };
        let session = connect(
            &mut handshake,
            &me,
            Keypair::from_secret([13; 32]),
            |_, _| chorus_protocol::v2::adoption::Verdict::Adopted,
        )
        .expect("the session comes up");
        let mut writer = SecureWriter::new(stream.try_clone().unwrap(), session.sealer);
        writer
            .send(&V2Message::Hello(Hello {
                protocol_version: 2,
                roles: roles::PLAYER,
                name: String::new(),
                software: "test".to_string(),
            }))
            .unwrap();
        writer
            .send(&V2Message::Capabilities(Capabilities {
                codecs: Codec::Pcm.bit(),
                sample_formats: 0b111,
                max_channels: 8,
                sample_rates_hz: vec![48_000],
                buffer_ms: 300,
                intrinsic_latency_ns: 0,
                led_count: 0,
                visualizer_bands: 0,
                features: 0,
            }))
            .unwrap();
        (SecureReader::new(stream, session.opener), writer)
    }

    /// A connected pair of loopback sockets: what the acceptor would have.
    fn connected_pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let address = listener.local_addr().unwrap();
        let client = TcpStream::connect(address).expect("the listener is up");
        let (server, _) = listener.accept().expect("the connection arrives");
        server
            .set_read_timeout(Some(Duration::from_millis(20)))
            .unwrap();
        (server, client)
    }

    fn a_pool(max_clients: usize) -> (ClientPool, Arc<ThreadRegistry>, usize) {
        let registry = Arc::new(ThreadRegistry::new());
        let (ready, up) = mpsc::channel::<()>();
        let pool = ClientPool::spawn(
            max_clients,
            MonotonicTimeline::new(),
            Arc::new(AtomicBool::new(true)),
            Arc::clone(&registry),
            ready,
            context(),
        );
        // Every thread reports itself and drops its sender, so this drains
        // exactly when the population is complete.
        let mut count = 0usize;
        while up.recv_timeout(Duration::from_secs(5)).is_ok() {
            count += 1;
        }
        (pool, registry, count)
    }

    /// Hand a connection to the pool.
    fn attach(pool: &ClientPool, server: TcpStream) -> bool {
        let reader = server.try_clone().expect("a connection splits");
        let peer = server.peer_addr().expect("a connected peer");
        pool.attach(server, reader, peer)
    }

    fn wait_until<F: Fn() -> bool>(what: &str, f: F) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if f() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("timed out waiting for {}", what);
    }

    #[test]
    fn every_thread_the_pool_will_ever_run_exists_and_is_registered_before_it_serves_anyone() {
        let (pool, registry, up) = a_pool(3);
        assert_eq!(pool.threads(), 6, "two threads per slot, and no more");
        assert_eq!(up, 6, "every thread reported itself before it was needed");
        let roles: Vec<String> = registry.snapshot().iter().map(|t| t.role.clone()).collect();
        assert_eq!(
            roles.len(),
            6,
            "every thread registered itself: {:?}",
            roles
        );
        for index in 0..3 {
            assert!(
                roles.contains(&format!("client-writer-{}", index)),
                "{:?}",
                roles
            );
            assert!(
                roles.contains(&format!("client-reader-{}", index)),
                "{:?}",
                roles
            );
        }
        assert!(
            registry.snapshot().iter().all(|t| !t.wants_real_time),
            "a socket is not the audio path, and none of these threads asks to be real-time"
        );
    }

    #[test]
    fn serving_a_client_creates_no_thread_and_neither_does_refusing_one() {
        let (pool, registry, _up) = a_pool(1);
        let before = registry.snapshot().len();

        let (server, client) = connected_pair();
        assert!(attach(&pool, server), "the one free slot takes the client");
        let (second, mut refused) = connected_pair();
        assert!(
            !attach(&pool, second),
            "the pool is full, so the second connection is refused rather than served"
        );
        assert_eq!(
            registry.snapshot().len(),
            before,
            "the thread population is fixed: a client that is served and a client that is refused \
             both cost zero new threads, which is what makes one scheduling report describe the \
             whole run. crates/server/tests/regress_0031_f6.rs asserts the same thing about the \
             whole process, against /proc"
        );

        // The refused connection was closed rather than left hanging.
        refused
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut scratch = [0u8; 16];
        assert_eq!(
            refused.read(&mut scratch).ok(),
            Some(0),
            "a refused client is disconnected, not left waiting for audio that never comes"
        );
        drop(client);
    }

    #[test]
    fn a_slot_comes_back_when_its_client_goes_away_and_serves_the_next_one() {
        let (pool, _registry, _up) = a_pool(1);

        let (server, client) = connected_pair();
        assert!(
            attach(&pool, server),
            "the free slot takes the first client"
        );
        drop(client);

        // Both halves have to finish before the slot is reusable, which is the
        // thing that could deadlock: the reader sees the close, and the writer
        // is parked on a queue nobody will fill.
        wait_until("the slot to come back", || {
            let (server, client) = connected_pair();
            let taken = attach(&pool, server);
            if taken {
                drop(client);
            }
            taken
        });
    }

    #[test]
    fn an_attached_client_is_answered_on_the_connection_it_asked_on() {
        let (pool, _registry, _up) = a_pool(1);
        let (server, client) = connected_pair();
        assert!(attach(&pool, server));
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let (mut reader, mut writer) = v2_endpoint(client);

        let request = encode(&Message::TimeSync(TimeSync {
            t0_ns: 77,
            t1_ns: 0,
            t2_ns: 0,
            t3_ns: 0,
        }))
        .unwrap();
        writer.write_all(&request).expect("the request goes up");

        let mut wire = vec![0u8; 4_096];
        let read = reader.read(&mut wire).expect("the reply comes back");
        match decode_frame(&wire[..read]).outcome {
            FrameOutcome::Decoded(Message::TimeSync(reply)) => {
                assert_eq!(reply.t0_ns, 77, "the client's own stamp comes back");
                assert!(reply.t1_ns > 0 && reply.t2_ns >= reply.t1_ns);
                assert_eq!(reply.t3_ns, 0, "only the client can stamp t3");
            }
            other => panic!("expected a time sync reply, got {:?}", other),
        }
    }
}
