//! Protocol v2: the catalog, the codec, the key exchange and the session.
//!
//! `docs/protocol.md` ("Version 2") is the contract and the golden vectors
//! under `fixtures/protocol/v2/` hold this implementation to it. v2 keeps v1's
//! frame and v1's three messages byte for byte, adds the session and the
//! roles, and carries every message after the handshake inside an encrypted
//! `secure_record`. The v1 catalog at the crate root stays as it is for the
//! endpoint until it moves to v2 (goal 6).

pub mod adoption;
pub mod catalog;
pub mod codec;
pub mod messages;
pub mod negotiate;
pub mod noise;
pub mod session;

pub use catalog::{
    roles, ChannelPosition, Codec, Command, Link, Playback, RefusalReason, SourceAction,
    SourceKind, Suite, Type, MAGIC, MAX_ROOM_VOLUME_RAMP_MS, PROTOCOL_VERSION, ROOM_VOLUME_FULL,
};
pub use codec::{
    decode_all, decode_frame, encode, encode_payload, validate, DecodeError, Decoded, EncodeError,
    FieldError, Outcome, Problem,
};
pub use messages::*;
