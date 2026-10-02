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
pub mod lowlat;
pub mod messages;
pub mod negotiate;
pub mod noise;
pub mod session;

pub use catalog::{
    features, roles, signal_reason, sound_flags, sound_fold, ChannelPosition, Codec, Command,
    FirmwareReason, FirmwareState, Link, LowLatencyDirection, LowLatencyStatus, Playback,
    RefusalReason, SourceAction, SourceKind, Suite, Type, FIRMWARE_ACK_EVERY,
    FIRMWARE_MAX_CHUNK_BYTES, FIRMWARE_MAX_SIZE, FIRMWARE_MAX_TEXT, FIRMWARE_SLOT_UNKNOWN,
    FIRMWARE_WINDOW_CHUNKS, LOW_LATENCY_FEC_DEPTH, LOW_LATENCY_FEC_K, LOW_LATENCY_MAX_CHUNK_FRAMES,
    LOW_LATENCY_MAX_LATENCY_NS, MAGIC, MAX_ROOM_VOLUME_RAMP_MS, PROTOCOL_VERSION, ROOM_VOLUME_FULL,
    SOUND_CROSSOVER_HZ, SOUND_EQ_FREQ_HZ, SOUND_EQ_GAIN_CDB, SOUND_EQ_MAX_FILTERS,
    SOUND_EQ_Q_MILLI, SOUND_SUB_LEVEL_CDB, SOUND_TONE_DB,
};
pub use codec::{
    decode_all, decode_frame, encode, encode_payload, validate, DecodeError, Decoded, EncodeError,
    FieldError, Outcome, Problem,
};
pub use messages::*;
