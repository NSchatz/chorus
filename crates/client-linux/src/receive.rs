//! Turning a byte stream back into chunks, and deciding what to do with the
//! ones that are wrong.
//!
//! # Why this is more than a decoder
//!
//! The transport is TCP, which does not preserve message boundaries. The
//! protocol's length-prefixed framing restores them, and the decoder already
//! refuses to consume anything it cannot locate the end of, so a partial frame
//! at the end of a read is not an error: it is a frame that has not arrived
//! yet.
//!
//! What the decoder cannot see is a **length that does not match the bytes
//! actually delivered for it**. A frame declaring 4000 bytes of payload with
//! 2000 bytes really belonging to it looks, to a decoder, exactly like a frame
//! whose payload happens to contain the next frame's header. Nothing in the
//! bytes says otherwise. So detection lives here, at the layer that knows what
//! this stream is supposed to look like, and the rules are:
//!
//! - The server sends a run of `audio_chunk` frames, then one `stream_end`,
//!   then closes, and answers a `time_sync` request with a `time_sync` reply
//!   at any point in between. That is the whole grammar.
//! - **The stream shape is fixed by the first chunk.** A later chunk that
//!   disagrees about rate, channels or sample format did not come from where
//!   it claims to, and that is a framing error rather than a bad chunk.
//! - **Only the last chunk may be short.** A chunk arriving after a short one
//!   means the short one was not short, it was truncated, so its declared
//!   length did not match the bytes delivered for it. Framing error.
//! - **A frame that fully decoded proves alignment.** Every field was in
//!   range and the payload was the length its type requires, which a
//!   mis-aligned reader gets wrong almost immediately.
//! - **A frame the decoder rejected is one bad frame.** Its header decoded, so
//!   the length prefix that steps over it came from a catalogued type and
//!   alignment survives. Discard it, count it by reason, keep the session
//!   open.
//! - **A frame skipped for an unknown type is alignment lost.** This is the
//!   one place where a stream transport differs from a datagram one, and it is
//!   worth being explicit about. `docs/protocol.md` has a decoder step over an
//!   unassigned type using its length prefix, which is exactly right when the
//!   frame arrived in a datagram whose boundaries the transport preserved. On
//!   TCP nothing corroborates that prefix: if alignment has already been lost,
//!   the "type" is a PCM byte and the "length" is two more of them, and
//!   stepping over them is how a reader stays lost. So this client refuses
//!   rather than steps. The cost is that a newer server's new message type
//!   ends this client's session instead of being ignored; the alternative is
//!   playing mis-framed bytes to a DAC as audio, which is worse and is what
//!   the criterion forbids.
//!
//! A framing error closes the session with a typed error and plays nothing
//! further. It never resynchronises by scanning for something that looks like
//! a header, because that is how mis-framed bytes reach a DAC as noise.

use std::fmt;
use std::io::{self, Read};

use chorus_protocol::{decode_frame, AudioChunk, FrameOutcome, Message, StreamEnd, TimeSync};

/// Largest buffer the reader will accumulate before declaring the stream
/// nonsense: one maximum frame, plus one read.
const MAX_PENDING: usize = 3 + 65_535 + 65_536;

/// The shape of a stream, fixed by its first chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamShape {
    /// Frames per second.
    pub sample_rate_hz: u32,
    /// Channels per frame.
    pub channels: u16,
    /// Sample layout.
    pub sample_format: chorus_protocol::SampleFormat,
    /// Frames the first chunk carried, which is the configured chunk size.
    pub frames_per_chunk: u64,
}

/// Why a session was closed by the client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FramingError {
    /// A chunk disagreed with the shape the first chunk established.
    ShapeChanged {
        /// The shape the stream started with.
        expected: StreamShape,
        /// What arrived.
        got_sample_rate_hz: u32,
        /// What arrived.
        got_channels: u16,
        /// What arrived.
        got_sample_format: chorus_protocol::SampleFormat,
    },
    /// A chunk arrived after one that was shorter than a full chunk.
    ChunkAfterShortChunk {
        /// Frames the short chunk carried.
        short_frames: u64,
        /// Frames a full chunk carries.
        full_frames: u64,
        /// Sequence of the chunk that arrived after it.
        sequence: u32,
    },
    /// A frame was stepped over using a length prefix nothing corroborates.
    ///
    /// See the module documentation: on a stream transport this is lost
    /// alignment far more often than it is a newer peer, and the client will
    /// not put the difference on a DAC.
    AlignmentLost {
        /// The type byte that is not in the catalog.
        message_type: u8,
        /// The payload length it declared.
        payload_len: usize,
    },
    /// The peer sent more than a whole maximum frame without a frame boundary
    /// appearing in it.
    NoFrameBoundary {
        /// Bytes held with no complete frame in them.
        pending: usize,
    },
}

impl fmt::Display for FramingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FramingError::ShapeChanged {
                expected,
                got_sample_rate_hz,
                got_channels,
                got_sample_format,
            } => write!(
                f,
                "framing error: the stream is {} Hz, {} channels, {}, and a chunk arrived \
                 claiming {} Hz, {} channels, {}",
                expected.sample_rate_hz,
                expected.channels,
                expected.sample_format.name(),
                got_sample_rate_hz,
                got_channels,
                got_sample_format.name()
            ),
            FramingError::ChunkAfterShortChunk {
                short_frames,
                full_frames,
                sequence,
            } => write!(
                f,
                "framing error: a chunk of {} frames arrived where a full chunk is {} frames, \
                 and sequence {} followed it, so its declared length did not match the bytes \
                 delivered for it",
                short_frames, full_frames, sequence
            ),
            FramingError::AlignmentLost {
                message_type,
                payload_len,
            } => write!(
                f,
                "framing error: a frame of unassigned type 0x{:02x} declaring {} bytes arrived on \
                 a stream transport; nothing corroborates that length, so stepping over it would \
                 risk putting mis-framed bytes on the audio device",
                message_type, payload_len
            ),
            FramingError::NoFrameBoundary { pending } => write!(
                f,
                "framing error: {} bytes held with no complete frame in them",
                pending
            ),
        }
    }
}

impl std::error::Error for FramingError {}

/// What the receiver produced from the bytes it read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Received {
    /// A chunk to offer to the buffer.
    Chunk {
        /// The chunk.
        chunk: AudioChunk,
        /// Frames it carries.
        frames: u64,
    },
    /// The in-band end of the stream.
    End(StreamEnd),
    /// A time-sync reply, on the same connection as the audio.
    ///
    /// It arrives with `t0`, `t1` and `t2` filled in and `t3` still zero: `t3`
    /// is the client's receive stamp and only the client can take it. The
    /// caller stamps it at receipt, which is where this event is delivered.
    TimeSync(TimeSync),
    /// A frame the decoder rejected. One bad frame, session stays open.
    Malformed,
    /// A catalogued message this phase's grammar has no place for.
    ///
    /// It fully decoded, so alignment is proven and the session is fine; it
    /// simply is not part of what this client is here to play. Every type in
    /// the catalog is in the grammar as of SYNC-4, so this is what a type
    /// added to the catalog later arrives as, rather than as lost alignment.
    NotInThisGrammar,
}

/// Why the receive loop stopped.
///
/// The end-of-stream case renders `end_timestamp_ns` into its report line and
/// computes nothing from it. `docs/protocol.md` is the normative definition of
/// that field; nothing here restates the relation.
#[derive(Debug)]
pub enum ReceiveStop {
    /// The in-band end of the stream arrived.
    EndOfStream(StreamEnd),
    /// The connection closed without the in-band end of stream.
    ConnectionLost {
        /// The read error, if the close was an error rather than an orderly
        /// end of file.
        error: Option<io::Error>,
    },
    /// The client closed the session itself, on a framing error.
    Framing(FramingError),
    /// The caller asked the loop to stop, which is how a run-length limit
    /// gets in.
    Stopped,
}

impl ReceiveStop {
    /// Whether this is a clean end.
    ///
    /// Only the in-band signal and a requested stop are clean. A connection
    /// that closed without the signal is the other case, and the two are told
    /// apart by the signal, never by the timing.
    pub fn is_clean(&self) -> bool {
        matches!(
            self,
            ReceiveStop::EndOfStream(_) | ReceiveStop::Stopped
        )
    }
}

impl fmt::Display for ReceiveStop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReceiveStop::EndOfStream(end) => write!(
                f,
                "end of stream: the server signalled it in band after final sequence {}, ending \
                 at {} ns on the server timeline",
                end.final_sequence, end.end_timestamp_ns
            ),
            ReceiveStop::ConnectionLost { error: Some(e) } => write!(
                f,
                "connection to the server was lost without an end-of-stream signal: {}",
                e
            ),
            ReceiveStop::ConnectionLost { error: None } => write!(
                f,
                "the server closed the connection without an end-of-stream signal"
            ),
            ReceiveStop::Framing(e) => write!(f, "{}", e),
            ReceiveStop::Stopped => write!(f, "the configured run length was reached"),
        }
    }
}

/// Decodes a stream transport into chunks, holding the grammar above.
#[derive(Debug, Default)]
pub struct Receiver {
    pending: Vec<u8>,
    shape: Option<StreamShape>,
    saw_short_chunk: Option<u64>,
}

impl Receiver {
    /// A receiver that has seen nothing.
    pub fn new() -> Receiver {
        Receiver::default()
    }

    /// The shape the first chunk established, if one has arrived.
    pub fn shape(&self) -> Option<StreamShape> {
        self.shape
    }

    /// Feed bytes in; take decisions out.
    ///
    /// Returns the sequence of things that happened, in order, or the framing
    /// error that ended the session.
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<Received>, FramingError> {
        self.pending.extend_from_slice(bytes);
        let mut out = Vec::new();
        let mut at = 0usize;
        loop {
            let result = decode_frame(&self.pending[at..]);
            if result.consumed == 0 {
                // The next boundary is not knowable from what is here yet.
                break;
            }
            at += result.consumed;
            match result.outcome {
                FrameOutcome::Decoded(Message::AudioChunk(chunk)) => {
                    out.push(self.accept_chunk(chunk)?);
                }
                FrameOutcome::Decoded(Message::StreamEnd(end)) => {
                    out.push(Received::End(end));
                }
                FrameOutcome::Decoded(Message::TimeSync(reply)) => {
                    out.push(Received::TimeSync(reply));
                }
                FrameOutcome::SkippedUnknownType {
                    message_type,
                    payload_len,
                } => {
                    return Err(FramingError::AlignmentLost {
                        message_type,
                        payload_len,
                    })
                }
                FrameOutcome::Rejected(_) => {
                    out.push(Received::Malformed);
                }
            }
        }
        self.pending.drain(..at);
        if self.pending.len() > MAX_PENDING {
            return Err(FramingError::NoFrameBoundary {
                pending: self.pending.len(),
            });
        }
        Ok(out)
    }

    fn accept_chunk(&mut self, chunk: AudioChunk) -> Result<Received, FramingError> {
        let frame_len = chunk.frame_len().expect("a decoded chunk has channels");
        let frames = (chunk.audio_data.len() / frame_len) as u64;

        if let Some(short_frames) = self.saw_short_chunk {
            let full = self.shape.map(|s| s.frames_per_chunk).unwrap_or(frames);
            return Err(FramingError::ChunkAfterShortChunk {
                short_frames,
                full_frames: full,
                sequence: chunk.sequence,
            });
        }

        match self.shape {
            None => {
                self.shape = Some(StreamShape {
                    sample_rate_hz: chunk.sample_rate_hz,
                    channels: chunk.channels,
                    sample_format: chunk.sample_format,
                    frames_per_chunk: frames,
                });
            }
            Some(shape) => {
                if shape.sample_rate_hz != chunk.sample_rate_hz
                    || shape.channels != chunk.channels
                    || shape.sample_format != chunk.sample_format
                {
                    return Err(FramingError::ShapeChanged {
                        expected: shape,
                        got_sample_rate_hz: chunk.sample_rate_hz,
                        got_channels: chunk.channels,
                        got_sample_format: chunk.sample_format,
                    });
                }
                if frames < shape.frames_per_chunk {
                    // Legitimate only as the final chunk. If another chunk
                    // follows, the next call turns this into a framing error.
                    self.saw_short_chunk = Some(frames);
                } else if frames > shape.frames_per_chunk {
                    return Err(FramingError::ChunkAfterShortChunk {
                        short_frames: shape.frames_per_chunk,
                        full_frames: frames,
                        sequence: chunk.sequence,
                    });
                }
            }
        }

        Ok(Received::Chunk { chunk, frames })
    }
}

/// Read from `source` until the stream ends, the connection is lost, the
/// client closes the session, or the caller asks it to stop.
///
/// `on_received` sees everything, in order. A read timeout is not a lost
/// connection: it is how the loop gets a chance to notice `keep_going`.
pub fn receive_loop<R: Read>(
    source: &mut R,
    receiver: &mut Receiver,
    keep_going: &dyn Fn() -> bool,
    on_received: &mut dyn FnMut(Received),
) -> ReceiveStop {
    let mut scratch = vec![0u8; 65_536];
    loop {
        if !keep_going() {
            return ReceiveStop::Stopped;
        }
        let n = match source.read(&mut scratch) {
            Ok(0) => return ReceiveStop::ConnectionLost { error: None },
            Ok(n) => n,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(ref e)
                if e.kind() == io::ErrorKind::WouldBlock
                    || e.kind() == io::ErrorKind::TimedOut =>
            {
                continue
            }
            Err(e) => return ReceiveStop::ConnectionLost { error: Some(e) },
        };
        let events = match receiver.push(&scratch[..n]) {
            Ok(v) => v,
            Err(e) => return ReceiveStop::Framing(e),
        };
        for event in events {
            if let Received::End(end) = event {
                return ReceiveStop::EndOfStream(end);
            }
            on_received(event);
        }
    }
}

/// What reading up to the first chunk produced.
///
/// The client cannot open an audio device until it knows what the stream is,
/// and it only knows that once a chunk has arrived and told it. So the first
/// chunk is read before the device is opened, and everything read alongside it
/// is carried forward rather than thrown away.
#[derive(Debug)]
pub struct Handshake {
    /// The shape the first chunk established.
    pub shape: StreamShape,
    /// The receiver, with its framing state, to carry into the run.
    pub receiver: Receiver,
    /// Everything decoded while waiting, in order, including the first chunk.
    pub buffered: Vec<Received>,
}

/// Why the first chunk never arrived.
#[derive(Debug)]
pub enum HandshakeError {
    /// The connection closed before any chunk arrived.
    ConnectionLost {
        /// What the read said, if anything.
        error: Option<io::Error>,
    },
    /// The stream was mis-framed from the start.
    Framing(FramingError),
    /// The stream ended before it began.
    EndOfStreamBeforeAnyChunk,
}

impl fmt::Display for HandshakeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HandshakeError::ConnectionLost { error: Some(e) } => write!(
                f,
                "the connection closed before any audio chunk arrived: {}",
                e
            ),
            HandshakeError::ConnectionLost { error: None } => {
                write!(f, "the connection closed before any audio chunk arrived")
            }
            HandshakeError::Framing(e) => write!(f, "{}", e),
            HandshakeError::EndOfStreamBeforeAnyChunk => {
                write!(f, "the server ended the stream before sending any audio")
            }
        }
    }
}

impl std::error::Error for HandshakeError {}

/// Read until the first chunk says what this stream is.
pub fn handshake<R: Read>(
    source: &mut R,
    keep_going: &dyn Fn() -> bool,
) -> Result<Handshake, HandshakeError> {
    let mut receiver = Receiver::new();
    let mut buffered = Vec::new();
    let mut scratch = vec![0u8; 65_536];
    loop {
        if !keep_going() {
            return Err(HandshakeError::ConnectionLost { error: None });
        }
        let n = match source.read(&mut scratch) {
            Ok(0) => return Err(HandshakeError::ConnectionLost { error: None }),
            Ok(n) => n,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(ref e)
                if e.kind() == io::ErrorKind::WouldBlock
                    || e.kind() == io::ErrorKind::TimedOut =>
            {
                continue
            }
            Err(e) => return Err(HandshakeError::ConnectionLost { error: Some(e) }),
        };
        let events = receiver.push(&scratch[..n]).map_err(HandshakeError::Framing)?;
        for event in events {
            if matches!(event, Received::End(_)) && receiver.shape().is_none() {
                return Err(HandshakeError::EndOfStreamBeforeAnyChunk);
            }
            buffered.push(event);
        }
        if let Some(shape) = receiver.shape() {
            return Ok(Handshake {
                shape,
                receiver,
                buffered,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chorus_protocol::{encode, Message, SampleFormat, RESERVED_LEN};

    fn chunk_bytes(sequence: u32, timestamp_ns: u64, frames: usize) -> Vec<u8> {
        encode(&Message::AudioChunk(AudioChunk {
            sequence,
            timestamp_ns,
            sample_rate_hz: 48_000,
            channels: 2,
            sample_format: SampleFormat::PcmS16Le,
            reserved: [0u8; RESERVED_LEN],
            audio_data: vec![7u8; frames * 4],
        }))
        .unwrap()
    }

    #[test]
    fn a_frame_split_across_reads_is_not_an_error() {
        let bytes = chunk_bytes(0, 0, 960);
        let mut r = Receiver::new();
        assert!(r.push(&bytes[..10]).unwrap().is_empty());
        assert!(r.push(&bytes[10..100]).unwrap().is_empty());
        let out = r.push(&bytes[100..]).unwrap();
        assert_eq!(out.len(), 1);
        assert!(matches!(out[0], Received::Chunk { frames: 960, .. }));
    }

    #[test]
    fn the_first_chunk_fixes_the_shape_and_a_disagreeing_one_is_a_framing_error() {
        let mut r = Receiver::new();
        r.push(&chunk_bytes(0, 0, 960)).unwrap();
        let odd = encode(&Message::AudioChunk(AudioChunk {
            sequence: 1,
            timestamp_ns: 20_000_000,
            sample_rate_hz: 44_100,
            channels: 2,
            sample_format: SampleFormat::PcmS16Le,
            reserved: [0u8; RESERVED_LEN],
            audio_data: vec![0u8; 960 * 4],
        }))
        .unwrap();
        assert!(matches!(
            r.push(&odd),
            Err(FramingError::ShapeChanged { .. })
        ));
    }

    #[test]
    fn a_short_chunk_is_fine_and_a_chunk_after_it_is_not() {
        let mut r = Receiver::new();
        r.push(&chunk_bytes(0, 0, 960)).unwrap();
        let out = r.push(&chunk_bytes(1, 20_000_000, 100)).unwrap();
        assert_eq!(out.len(), 1, "a short chunk is the final chunk and is kept");
        match r.push(&chunk_bytes(2, 40_000_000, 960)) {
            Err(FramingError::ChunkAfterShortChunk {
                short_frames,
                full_frames,
                sequence,
            }) => {
                assert_eq!(short_frames, 100);
                assert_eq!(full_frames, 960);
                assert_eq!(sequence, 2);
            }
            other => panic!("expected a framing error, got {:?}", other),
        }
    }

    #[test]
    fn a_rejected_frame_is_one_bad_frame_and_the_session_survives() {
        let mut r = Receiver::new();
        // An audio chunk frame whose PCM is not a whole number of frames.
        let mut bad = chunk_bytes(0, 0, 960);
        bad[2] -= 1;
        let len = bad.len();
        bad.truncate(len - 1);
        let mut stream = bad;
        stream.extend_from_slice(&chunk_bytes(1, 20_000_000, 960));
        let out = r.push(&stream).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], Received::Malformed);
        assert!(matches!(out[1], Received::Chunk { .. }));
    }

    #[test]
    fn a_frame_stepped_over_by_an_uncorroborated_length_is_alignment_lost() {
        let mut r = Receiver::new();
        r.push(&chunk_bytes(0, 0, 960)).unwrap();
        // Type 0x7F is unassigned. A decoder steps over it by design, which is
        // right on a transport that preserved the frame's boundaries and wrong
        // on one that did not.
        match r.push(&[0x7F, 0x00, 0x02, 0xAA, 0xBB]) {
            Err(FramingError::AlignmentLost {
                message_type,
                payload_len,
            }) => {
                assert_eq!(message_type, 0x7F);
                assert_eq!(payload_len, 2);
            }
            other => panic!("expected alignment lost, got {:?}", other),
        }
    }

    #[test]
    fn a_time_sync_reply_interleaved_with_audio_reaches_the_caller() {
        // The exchange shares the connection with the audio, which is the
        // whole point of AC-9: the reply arrives between two chunks, both
        // chunks still decode, and the reply is handed up rather than dropped.
        let mut r = Receiver::new();
        let mut stream = chunk_bytes(0, 0, 960);
        stream.extend_from_slice(
            &encode(&Message::TimeSync(chorus_protocol::TimeSync {
                t0_ns: 1,
                t1_ns: 2,
                t2_ns: 3,
                t3_ns: 0,
            }))
            .unwrap(),
        );
        stream.extend_from_slice(&chunk_bytes(1, 20_000_000, 960));
        let out = r.push(&stream).unwrap();
        assert_eq!(out.len(), 3);
        match out[1] {
            Received::TimeSync(reply) => {
                assert_eq!(reply.t0_ns, 1);
                assert_eq!(reply.t1_ns, 2);
                assert_eq!(reply.t2_ns, 3);
                assert_eq!(reply.t3_ns, 0, "only the client can stamp t3");
            }
            ref other => panic!("expected a time sync reply, got {:?}", other),
        }
        assert!(matches!(out[2], Received::Chunk { .. }));
    }

    #[test]
    fn the_end_of_stream_arrives_as_data_and_not_as_a_close() {
        let mut r = Receiver::new();
        let mut stream = chunk_bytes(0, 0, 960);
        stream.extend_from_slice(
            &encode(&Message::StreamEnd(StreamEnd {
                final_sequence: 0,
                end_timestamp_ns: 20_000_000,
            }))
            .unwrap(),
        );
        let out = r.push(&stream).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(
            out[1],
            Received::End(StreamEnd {
                final_sequence: 0,
                end_timestamp_ns: 20_000_000
            })
        );
    }

    #[test]
    fn a_truncated_chunk_followed_by_the_next_one_is_caught_rather_than_played() {
        // The injection AC-17 names: declare a full chunk, deliver less than
        // that, and let the next frame's bytes fill the gap.
        let full = chunk_bytes(0, 0, 960);
        let next = chunk_bytes(1, 20_000_000, 960);
        let mut stream = Vec::new();
        stream.extend_from_slice(&full);
        // Header declares a full chunk; only half the PCM belongs to it.
        stream.extend_from_slice(&next[..3]);
        stream.extend_from_slice(&next[3..3 + 32 + 480 * 4]);
        stream.extend_from_slice(&chunk_bytes(2, 40_000_000, 960));

        let mut r = Receiver::new();
        let mut played = 0u64;
        let mut error = None;
        match r.push(&stream) {
            Ok(events) => {
                for e in events {
                    if let Received::Chunk { frames, .. } = e {
                        played += frames;
                    }
                }
            }
            Err(e) => error = Some(e),
        }
        assert!(
            error.is_some() || played <= 960 + 960,
            "mis-framed bytes must not turn into extra audio"
        );
        // The stream cannot be reconciled, and the client says so rather than
        // playing the middle of a header as samples.
        assert!(error.is_some(), "expected a framing error, played {}", played);
    }
}
