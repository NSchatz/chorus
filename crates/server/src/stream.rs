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
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use chorus_audio::MonotonicTimeline;
use chorus_protocol::{decode_frame, encode, FrameOutcome, Message, TimeSync};

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
    subscribers: Mutex<Vec<Sender<Outbound>>>,
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
    pub fn subscribe(&self) -> (Sender<Outbound>, Receiver<Outbound>) {
        let (tx, rx) = mpsc::channel();
        self.lock().push(tx.clone());
        (tx, rx)
    }

    /// How many clients are attached.
    pub fn subscribers(&self) -> usize {
        self.lock().len()
    }

    /// Hand `item` to every attached client, dropping the ones that have gone.
    ///
    /// A client that has disconnected is not an error and never stops the
    /// stream: the stream is the thing the others are aligned to, and taking
    /// it down because one endpoint was unplugged would take the group with
    /// it.
    pub fn broadcast(&self, item: Outbound) -> usize {
        let mut subscribers = self.lock();
        subscribers.retain(|tx| tx.send(item.clone()).is_ok());
        subscribers.len()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Sender<Outbound>>> {
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
    out: &Sender<Outbound>,
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
                if out.send(answer).is_err() {
                    return RequestStop::Closed;
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
        let (tx, rx) = mpsc::channel();
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
