//! The chorus wire protocol.
//!
//! A pure library: no socket, no clock, no audio device. It turns messages
//! into bytes and bytes back into messages, and it is the half of the protocol
//! that the later C implementation is held to byte for byte through the golden
//! vectors under `fixtures/protocol/`.
//!
//! - Framing and the message catalog: `docs/protocol.md`.
//! - Why the frame looks like this: `docs/decisions/0003-wire-protocol-framing.md`.
//! - What a decoder does with a frame it cannot accept:
//!   `docs/decisions/0005-decoder-frame-validation.md`.
//!
//! ```
//! use chorus_protocol::{encode, Message, Session, TimeSync};
//!
//! let message = Message::TimeSync(TimeSync {
//!     t0_ns: 1_000_000_000,
//!     t1_ns: 1_000_500_000,
//!     t2_ns: 1_001_000_000,
//!     t3_ns: 1_001_600_000,
//! });
//! let frame = encode(&message).expect("a valid message encodes");
//!
//! let mut session = Session::new();
//! let outcomes = session.decode_buffer(&frame);
//! assert_eq!(outcomes[0].message(), Some(&message));
//! assert!(session.is_open());
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod codec;
pub mod message;
pub mod session;

pub use codec::{
    decode_frame, encode, encode_payload, DecodeError, EncodeError, FrameOutcome, FrameResult,
    InvalidField, HEADER_LEN, MAX_PAYLOAD_LEN,
};
pub use message::{
    AudioChunk, Message, MessageType, SampleFormat, TimeSync, CHUNK_HEADER_LEN, MAX_CHANNELS,
    MAX_SAMPLE_RATE_HZ, MIN_SAMPLE_RATE_HZ, RESERVED_LEN, RESERVED_OFFSET, TIME_SYNC_PAYLOAD_LEN,
};
pub use session::Session;
