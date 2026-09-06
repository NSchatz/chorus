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
//! # A full pool refuses rather than grows
//!
//! `max_clients` is a ceiling on connections, and it is the same argument the
//! bounded outbound queue in [`crate::stream`] makes: a server that grows a
//! resource per client has no bound on that resource at all. A connection
//! arriving with every slot busy is closed and named in the log, which an
//! operator can see, rather than served by a thread nothing declared.

use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::sync::Arc;
use std::thread;

use chorus_audio::MonotonicTimeline;
use chorus_hostctl::ThreadRegistry;

use crate::hostreport::register_ordinary_thread;
use crate::stream::{read_requests, write_outbound, Outbound};

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
}

/// What the reader half of a slot is handed.
struct ReaderJob {
    source: TcpStream,
    out: SyncSender<Outbound>,
    life: Arc<SlotLife>,
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
    pub fn spawn(
        max_clients: usize,
        timeline: MonotonicTimeline,
        keep: Arc<AtomicBool>,
        registry: Arc<ThreadRegistry>,
        ready: Sender<()>,
    ) -> ClientPool {
        let (free_tx, free) = mpsc::channel::<usize>();
        let mut slots = Vec::with_capacity(max_clients);

        for index in 0..max_clients {
            let life = Arc::new(SlotLife::idle());
            let (to_writer, jobs) = mpsc::sync_channel::<WriterJob>(1);
            {
                let keep = Arc::clone(&keep);
                let registry = Arc::clone(&registry);
                let ready = ready.clone();
                let free_tx = free_tx.clone();
                thread::spawn(move || {
                    register_ordinary_thread(&format!("client-writer-{}", index), &registry);
                    if ready.send(()).is_err() {
                        return;
                    }
                    drop(ready);
                    for job in jobs {
                        let WriterJob {
                            mut sink,
                            inbox,
                            life,
                        } = job;
                        let going = {
                            let keep = Arc::clone(&keep);
                            let life = Arc::clone(&life);
                            move || keep.load(Ordering::SeqCst) && life.live.load(Ordering::SeqCst)
                        };
                        let _ = write_outbound(&mut sink, timeline, &inbox, &going);
                        drop(inbox);
                        drop(sink);
                        if !release(&life, index, &free_tx) {
                            return;
                        }
                    }
                });
            }

            let (to_reader, jobs) = mpsc::sync_channel::<ReaderJob>(1);
            {
                let keep = Arc::clone(&keep);
                let registry = Arc::clone(&registry);
                let ready = ready.clone();
                let free_tx = free_tx.clone();
                thread::spawn(move || {
                    register_ordinary_thread(&format!("client-reader-{}", index), &registry);
                    if ready.send(()).is_err() {
                        return;
                    }
                    drop(ready);
                    for job in jobs {
                        let ReaderJob {
                            mut source,
                            out,
                            life,
                        } = job;
                        let going = {
                            let keep = Arc::clone(&keep);
                            let life = Arc::clone(&life);
                            move || keep.load(Ordering::SeqCst) && life.live.load(Ordering::SeqCst)
                        };
                        read_requests(&mut source, timeline, &out, &going);
                        drop(out);
                        drop(source);
                        if !release(&life, index, &free_tx) {
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
        }
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
    /// `subscribe` is called only once a slot has been secured, so a refused
    /// connection never leaves a subscriber in the fanout with nothing draining
    /// it. `false` means every slot is busy: the sockets are dropped, which
    /// closes the connection, and the caller is expected to say so out loud.
    pub fn attach<F>(&self, sink: TcpStream, source: TcpStream, subscribe: F) -> bool
    where
        F: FnOnce() -> (SyncSender<Outbound>, Receiver<Outbound>),
    {
        let index = match self.free.try_recv() {
            Ok(index) => index,
            Err(_) => return false,
        };
        let slot = &self.slots[index];
        slot.life.finished.store(0, Ordering::SeqCst);
        slot.life.live.store(true, Ordering::SeqCst);
        let (out, inbox) = subscribe();
        let handed_over = slot
            .to_writer
            .send(WriterJob {
                sink,
                inbox,
                life: Arc::clone(&slot.life),
            })
            .is_ok()
            && slot
                .to_reader
                .send(ReaderJob {
                    source,
                    out,
                    life: Arc::clone(&slot.life),
                })
                .is_ok();
        if !handed_over {
            // A slot thread is gone, which can only mean it panicked. The
            // connection is not served and the slot is not returned to the free
            // list: a slot with one half missing would serve the next client
            // in one direction only, and half a connection is worse than none.
            slot.life.live.store(false, Ordering::SeqCst);
        }
        handed_over
    }
}

/// Finish with a connection, and return the slot to the free list once both
/// halves have.
///
/// Returns whether the pool is still there to return it to.
fn release(life: &Arc<SlotLife>, index: usize, free: &Sender<usize>) -> bool {
    // Whichever half finishes first tells the other one to stop: the writer is
    // parked on a queue nothing will fill again, and the reader on a socket
    // that has nothing more to say.
    life.live.store(false, Ordering::SeqCst);
    if life.finished.fetch_add(1, Ordering::SeqCst) == 1 {
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
    use std::sync::mpsc::sync_channel;
    use std::time::{Duration, Instant};

    use chorus_protocol::{decode_frame, encode, FrameOutcome, Message, TimeSync};

    use crate::stream::SUBSCRIBER_QUEUE_LIMIT;

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
        );
        // Every thread reports itself and drops its sender, so this drains
        // exactly when the population is complete.
        let mut count = 0usize;
        while up.recv_timeout(Duration::from_secs(5)).is_ok() {
            count += 1;
        }
        (pool, registry, count)
    }

    /// Hand a connection to the pool, with a real bounded subscriber queue.
    fn attach(pool: &ClientPool, server: TcpStream) -> bool {
        let reader = server.try_clone().expect("a connection splits");
        pool.attach(server, reader, || sync_channel(SUBSCRIBER_QUEUE_LIMIT))
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
        assert_eq!(roles.len(), 6, "every thread registered itself: {:?}", roles);
        for index in 0..3 {
            assert!(roles.contains(&format!("client-writer-{}", index)), "{:?}", roles);
            assert!(roles.contains(&format!("client-reader-{}", index)), "{:?}", roles);
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
        assert!(attach(&pool, server), "the free slot takes the first client");
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
        let (server, mut client) = connected_pair();
        assert!(attach(&pool, server));

        let request = encode(&Message::TimeSync(TimeSync {
            t0_ns: 77,
            t1_ns: 0,
            t2_ns: 0,
            t3_ns: 0,
        }))
        .unwrap();
        client.write_all(&request).expect("the request goes up");
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();

        let mut wire = vec![0u8; 4_096];
        let read = client.read(&mut wire).expect("the reply comes back");
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
