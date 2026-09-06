//! One stream, one timeline, more than one client.
//!
//! # What "one grouped stream" means here, and what it does not
//!
//! It means exactly this: one PCM source, one chunker, one monotonic timeline,
//! and every connected client handed the same chunks with the same presentation
//! timestamps. Nothing about zones, groups, naming, volume or discovery lives
//! here; that is a later phase's subject. What this is for is the property two
//! endpoints have to share before they can be measured against each other -
//! that the same audio content is due at the same instant at both of them.
//!
//! Serving each client its own [`chorus_audio::Chunker`] on its own timeline,
//! which is what the server did before, gives two clients two different
//! streams that happen to sound alike. They cannot be inaudibly apart, because
//! there is nothing they are both aligned TO.
//!
//! # No back pressure, but a ceiling
//!
//! One endpoint that stops reading must not stop the stream the others are
//! aligned to, so nothing here blocks the chunk emitter on a slow client. That
//! decoupling needs its other half or it is just an unbounded allocation:
//! every subscriber's queue is bounded at [`SUBSCRIBER_QUEUE_LIMIT`], and
//! items dropped for a subscriber at its ceiling are counted in
//! [`Fanout::dropped`] and reported at the end of the stream.
//!
//! # The connection carries both directions
//!
//! A client's time-sync request arrives on the same connection its audio
//! leaves by, and the reply goes back down it, interleaved with the chunks.
//! The framing is self-delimiting, so a mix of types on one connection is what
//! `docs/protocol.md` already provides for.
//!
//! Two stamps make that reply worth having, and both are taken as late as they
//! can be:
//!
//! - `t1` is taken by [`read_requests`] the moment the request is decoded off
//!   the socket, before it is queued for anything.
//! - `t2` is taken by [`write_outbound`] at the moment the reply is encoded,
//!   after it has come off the queue.
//!
//! Stamping both at the same time, or stamping `t2` when the request arrived,
//! would fold the server's own queueing into the network time the client
//! measures, and the client would then correct for a delay that is not there.
//! `t3` is not the server's to fill in: it is the client's receive stamp, on
//! the client's clock, and it stays zero on the wire.
//!
//! Every stamp here is nanoseconds from [`MonotonicTimeline`], which is
//! `Instant` and its own epoch. There is no wall clock anywhere on this path.

use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};

use chorus_audio::MonotonicTimeline;
use chorus_protocol::{decode_frame, encode, FrameOutcome, Message, TimeSync};

/// How far behind one client may fall before the group plays on without it,
/// counted in outbound items.
///
/// A group has no back pressure on purpose: one endpoint that stops reading
/// must not stop the stream the others are aligned to. But a queue with no
/// ceiling is not decoupling, it is an unbounded allocation with a stalled
/// socket on the end of it. At the default stream shape (48 kHz stereo
/// `pcm_s16le` in 20 ms chunks) a subscriber that has stopped draining accrues
/// about 192 kB of audio every second it stays stalled, for as long as the
/// process runs, on a host this server has already asked for 64 MB of locked
/// memory.
///
/// 128 items is 2.56 s of audio at that shape and roughly 500 kB per stalled
/// subscriber. A client that is 2.5 s behind is not going to catch up: the
/// client plays to a fixed playout latency of 180 ms and treats anything past
/// its ceiling as an overflow, so those chunks are already due to be dropped at
/// the far end. What matters is that the drop is BOUNDED and COUNTED here
/// rather than deferred into an allocation, which is what
/// [`Fanout::dropped`] reports.
pub const SUBSCRIBER_QUEUE_LIMIT: usize = 128;

/// Something to put on one client's connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outbound {
    /// An already-encoded frame, broadcast to every subscriber.
    Frame(Arc<Vec<u8>>),
    /// A request that has been received and not yet answered.
    ///
    /// The reply is built at SEND time so that `t2` is a transmit stamp and
    /// not a stamp of when the request reached the queue.
    Answer {
        /// The client's transmit stamp, echoed back untouched.
        t0_ns: u64,
        /// The server's receive stamp, taken when the request was decoded.
        t1_ns: u64,
    },
}

/// Every client currently attached to the stream.
#[derive(Debug, Default)]
pub struct Fanout {
    subscribers: Mutex<Vec<SyncSender<Outbound>>>,
    dropped: AtomicU64,
}

impl Fanout {
    /// A fanout with nobody attached.
    pub fn new() -> Fanout {
        Fanout::default()
    }

    /// Attach, and receive everything broadcast from now on.
    ///
    /// A client that attaches mid-stream gets the chunks from where it
    /// attached. Its sequence numbers are a contiguous run from its own first
    /// chunk; they do not start at zero, because the stream did not.
    ///
    /// The queue is bounded at [`SUBSCRIBER_QUEUE_LIMIT`]; see there for why a
    /// group needs a ceiling even though it must not have back pressure.
    pub fn subscribe(&self) -> (SyncSender<Outbound>, Receiver<Outbound>) {
        let (tx, rx) = mpsc::sync_channel(SUBSCRIBER_QUEUE_LIMIT);
        self.lock().push(tx.clone());
        (tx, rx)
    }

    /// How many clients are attached.
    pub fn subscribers(&self) -> usize {
        self.lock().len()
    }

    /// How many items have been dropped for subscribers that were at their
    /// ceiling, over the life of this fanout.
    ///
    /// This is the report half of the bound. A drop that is not counted is
    /// indistinguishable from a stream that was never sent, and an operator
    /// reading a run needs to be able to tell those apart.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Hand `item` to every attached client, dropping the ones that have gone
    /// and the items of the ones that are too far behind.
    ///
    /// A client that has disconnected is not an error and never stops the
    /// stream: the stream is the thing the others are aligned to, and taking
    /// it down because one endpoint was unplugged would take the group with
    /// it. A client that is still connected but has stopped draining is the
    /// same argument one step on: the group plays past it, this item is
    /// dropped for it alone and counted in [`Fanout::dropped`], and it stays
    /// attached so that it can catch up if it starts reading again.
    pub fn broadcast(&self, item: Outbound) -> usize {
        let mut subscribers = self.lock();
        let mut dropped = 0u64;
        subscribers.retain(|tx| match tx.try_send(item.clone()) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                dropped += 1;
                true
            }
            Err(TrySendError::Disconnected(_)) => false,
        });
        if dropped > 0 {
            self.dropped.fetch_add(dropped, Ordering::Relaxed);
        }
        subscribers.len()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<SyncSender<Outbound>>> {
        match self.subscribers.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

/// The fanout, wearing the [`Write`] the chunk emitter expects.
///
/// The emitter writes one whole encoded frame per call, so one call is one
/// message and no framing knowledge is needed here.
#[derive(Debug, Clone)]
pub struct FanoutSink(Arc<Fanout>);

impl FanoutSink {
    /// A sink that broadcasts to `fanout`.
    pub fn new(fanout: Arc<Fanout>) -> FanoutSink {
        FanoutSink(fanout)
    }
}

impl Write for FanoutSink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.broadcast(Outbound::Frame(Arc::new(buf.to_vec())));
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Why the request reader stopped.
#[derive(Debug)]
pub enum RequestStop {
    /// The client closed the connection, or the caller asked for a stop.
    Closed,
    /// The read failed.
    Failed(io::Error),
    /// The client sent bytes that are not a frame this server can locate the
    /// end of.
    Unframed {
        /// Bytes held with no complete frame in them.
        pending: usize,
    },
}

/// Largest run of bytes the reader will hold without a frame boundary in it.
const MAX_PENDING: usize = 3 + 65_535 + 65_536;

/// Read one client's requests until it goes away.
///
/// `t1` is taken here, the moment a request is decoded, which is the earliest
/// point the server can honestly say it received one.
pub fn read_requests<R: Read>(
    source: &mut R,
    timeline: MonotonicTimeline,
    out: &SyncSender<Outbound>,
    keep_going: &dyn Fn() -> bool,
) -> RequestStop {
    let mut pending: Vec<u8> = Vec::new();
    let mut scratch = vec![0u8; 4_096];
    loop {
        if !keep_going() {
            return RequestStop::Closed;
        }
        let n = match source.read(&mut scratch) {
            Ok(0) => return RequestStop::Closed,
            Ok(n) => n,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(ref e)
                if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut =>
            {
                continue
            }
            Err(e) => return RequestStop::Failed(e),
        };
        pending.extend_from_slice(&scratch[..n]);
        let mut at = 0usize;
        loop {
            let result = decode_frame(&pending[at..]);
            if result.consumed == 0 {
                break;
            }
            at += result.consumed;
            if let FrameOutcome::Decoded(Message::TimeSync(request)) = result.outcome {
                let answer = Outbound::Answer {
                    t0_ns: request.t0_ns,
                    t1_ns: timeline.now_ns(),
                };
                match out.try_send(answer) {
                    Ok(()) => {}
                    // The queue is bounded, so this client is already
                    // SUBSCRIBER_QUEUE_LIMIT items behind on its own socket.
                    // Blocking here would stall this reader on a client that
                    // is not reading; the exchange is simply not answered, and
                    // an unanswered exchange is one the client discards and
                    // asks again. What must not happen is an answer queued
                    // behind seconds of stale audio and stamped with a `t2`
                    // from before that wait.
                    Err(TrySendError::Full(_)) => {}
                    Err(TrySendError::Disconnected(_)) => return RequestStop::Closed,
                }
            }
        }
        pending.drain(..at);
        if pending.len() > MAX_PENDING {
            return RequestStop::Unframed {
                pending: pending.len(),
            };
        }
    }
}

/// Write one client's connection until it goes away.
///
/// `t2` is taken here, at the moment the reply is built, which is the latest
/// point the server can honestly say it transmitted one.
pub fn write_outbound<W: Write>(
    sink: &mut W,
    timeline: MonotonicTimeline,
    inbox: &Receiver<Outbound>,
) -> io::Result<u64> {
    let mut written = 0u64;
    for item in inbox {
        match item {
            Outbound::Frame(bytes) => {
                sink.write_all(&bytes)?;
                written += 1;
            }
            Outbound::Answer { t0_ns, t1_ns } => {
                let reply = TimeSync {
                    t0_ns,
                    t1_ns,
                    t2_ns: timeline.now_ns(),
                    // The client's receive stamp, on the client's clock. The
                    // server cannot know it and will not invent it.
                    t3_ns: 0,
                };
                let frame = encode(&Message::TimeSync(reply))
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
                sink.write_all(&frame)?;
                written += 1;
            }
        }
        sink.flush()?;
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_broadcast_reaches_every_attached_client_and_nobody_else() {
        let fanout = Fanout::new();
        let (_a_tx, a) = fanout.subscribe();
        let (_b_tx, b) = fanout.subscribe();
        assert_eq!(fanout.subscribers(), 2);
        let frame = Outbound::Frame(Arc::new(vec![1, 2, 3]));
        assert_eq!(fanout.broadcast(frame.clone()), 2);
        assert_eq!(a.recv().unwrap(), frame);
        assert_eq!(b.recv().unwrap(), frame);
    }

    #[test]
    fn a_subscriber_that_stops_draining_is_bounded_and_its_drops_are_counted() {
        let fanout = Fanout::new();
        let (_reader_tx, reading) = fanout.subscribe();
        let (_stalled_tx, stalled) = fanout.subscribe();
        let frame = Outbound::Frame(Arc::new(vec![0u8; 3_840]));

        // Twice the ceiling, so the stalled subscriber is well past it and the
        // one that is reading is never behind.
        let broadcasts = SUBSCRIBER_QUEUE_LIMIT * 2;
        for _ in 0..broadcasts {
            assert_eq!(fanout.broadcast(frame.clone()), 2, "nobody is disconnected");
            assert!(reading.recv().is_ok(), "the draining client keeps up");
        }

        // The stalled one holds exactly its ceiling and not one item more.
        let mut held = 0usize;
        while stalled.try_recv().is_ok() {
            held += 1;
        }
        assert_eq!(
            held, SUBSCRIBER_QUEUE_LIMIT,
            "a stalled subscriber must not accumulate past its ceiling"
        );
        assert_eq!(
            fanout.dropped() as usize,
            broadcasts - SUBSCRIBER_QUEUE_LIMIT,
            "every dropped item is counted, so a drop is never silent"
        );
    }

    #[test]
    fn a_client_that_has_gone_is_dropped_and_the_stream_carries_on() {
        let fanout = Fanout::new();
        let (_a_tx, a) = fanout.subscribe();
        let (b_tx, b) = fanout.subscribe();
        drop(b);
        drop(b_tx);
        assert_eq!(fanout.broadcast(Outbound::Frame(Arc::new(vec![7]))), 1);
        assert_eq!(fanout.subscribers(), 1);
        assert!(a.recv().is_ok());
    }

    #[test]
    fn a_request_is_answered_with_the_clients_own_stamp_echoed_and_two_server_ones() {
        let timeline = MonotonicTimeline::new();
        let request = encode(&Message::TimeSync(TimeSync {
            t0_ns: 4_242,
            t1_ns: 0,
            t2_ns: 0,
            t3_ns: 0,
        }))
        .unwrap();
        let (tx, rx) = mpsc::sync_channel(SUBSCRIBER_QUEUE_LIMIT);
        let mut source = std::io::Cursor::new(request);
        read_requests(&mut source, timeline, &tx, &|| true);
        drop(tx);

        let mut wire = Vec::new();
        write_outbound(&mut wire, timeline, &rx).unwrap();

        let decoded = decode_frame(&wire);
        match decoded.outcome {
            FrameOutcome::Decoded(Message::TimeSync(reply)) => {
                assert_eq!(reply.t0_ns, 4_242, "the client's own stamp comes back");
                assert!(reply.t1_ns > 0);
                assert!(reply.t2_ns >= reply.t1_ns, "received before transmitted");
                assert!(reply.t2_ns <= timeline.now_ns());
                assert_eq!(reply.t3_ns, 0, "only the client can stamp t3");
            }
            other => panic!("expected a time sync reply, got {:?}", other),
        }
    }
}
