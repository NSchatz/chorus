//! Framing, encoding and decoding.
//!
//! One frame is a type byte, a big-endian u16 payload length, then exactly
//! that many payload bytes. The order of the decoder's checks is part of the
//! contract; it is written down in
//! `docs/decisions/0005-decoder-frame-validation.md` and mirrored by the
//! `decode_frame` body below.

use std::fmt;

use crate::message::{
    AudioChunk, Message, MessageType, SampleFormat, TimeSync, CHUNK_HEADER_LEN, MAX_CHANNELS,
    MAX_SAMPLE_RATE_HZ, MIN_SAMPLE_RATE_HZ, RESERVED_LEN, RESERVED_OFFSET,
};

/// Bytes in a frame header: one type byte plus a u16 length.
pub const HEADER_LEN: usize = 3;

/// Largest payload the u16 length field can describe.
pub const MAX_PAYLOAD_LEN: usize = u16::MAX as usize;

/// A field value that arrived in a correctly sized frame and is still not
/// something the format can accept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidField {
    /// `sample_format` is not one of the defined values.
    UndefinedSampleFormat(u8),
    /// `channels` is zero or beyond [`MAX_CHANNELS`].
    ChannelsOutOfRange(u8),
    /// `sample_rate_hz` is outside the accepted band.
    SampleRateOutOfRange(u32),
    /// The PCM byte count is not a whole number of sample frames.
    AudioDataNotFrameAligned {
        /// Bytes of PCM that arrived.
        data_len: usize,
        /// Bytes one frame occupies, from the header's own fields.
        frame_len: usize,
    },
    /// The PCM byte count is zero.
    ///
    /// A decoder never reports this, because a chunk with no samples is below
    /// the audio chunk type's minimum payload and is rejected one check
    /// earlier. It is what an encoder says when it is handed one.
    EmptyAudioData,
}

impl fmt::Display for InvalidField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InvalidField::UndefinedSampleFormat(v) => {
                write!(f, "undefined sample format {}", v)
            }
            InvalidField::ChannelsOutOfRange(v) => write!(
                f,
                "channel count {} outside 1 to {}",
                v, MAX_CHANNELS
            ),
            InvalidField::SampleRateOutOfRange(v) => write!(
                f,
                "sample rate {} Hz outside {} to {}",
                v, MIN_SAMPLE_RATE_HZ, MAX_SAMPLE_RATE_HZ
            ),
            InvalidField::AudioDataNotFrameAligned {
                data_len,
                frame_len,
            } => write!(
                f,
                "{} bytes of PCM is not a whole number of {} byte frames",
                data_len, frame_len
            ),
            InvalidField::EmptyAudioData => write!(f, "audio chunk carries no samples"),
        }
    }
}

/// Why a frame was rejected.
///
/// Every one of these rejects exactly one frame. None of them closes a
/// session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// Fewer bytes remain than a frame header needs.
    TruncatedHeader {
        /// Bytes that remained.
        available: usize,
        /// Bytes a header needs.
        needed: usize,
    },
    /// The length field declares more payload than the buffer holds.
    ///
    /// This is checked before anything slices the payload, which is what makes
    /// "shall not read past the end of the buffer" true.
    DeclaredLengthExceedsBuffer {
        /// What the length field declared.
        declared: usize,
        /// What was actually there after the header.
        available: usize,
    },
    /// The declared payload is shorter than this type's minimum.
    PayloadTooShortForType {
        /// The type byte, which is in the catalog.
        message_type: u8,
        /// What the length field declared.
        declared: usize,
        /// The minimum this type requires.
        minimum: usize,
    },
    /// The frame is the right size and still carries a value the format
    /// cannot accept.
    InvalidFieldValue {
        /// The type byte, which is in the catalog.
        message_type: u8,
        /// What was wrong.
        field: InvalidField,
    },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::TruncatedHeader { available, needed } => write!(
                f,
                "truncated frame header: {} bytes available, {} needed",
                available, needed
            ),
            DecodeError::DeclaredLengthExceedsBuffer {
                declared,
                available,
            } => write!(
                f,
                "frame declares a {} byte payload but only {} bytes are available",
                declared, available
            ),
            DecodeError::PayloadTooShortForType {
                message_type,
                declared,
                minimum,
            } => write!(
                f,
                "message type 0x{:02x} declares a {} byte payload, minimum is {}",
                message_type, declared, minimum
            ),
            DecodeError::InvalidFieldValue {
                message_type,
                field,
            } => write!(f, "message type 0x{:02x}: {}", message_type, field),
        }
    }
}

impl std::error::Error for DecodeError {}

/// Why a message was not encoded.
///
/// An encoder that hits one of these emits nothing at all. A partial or
/// wrapped-around frame is never written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodeError {
    /// The value does not fit the wire field that carries it.
    NotRepresentable {
        /// Field name, as it appears in `docs/protocol.md`.
        field: &'static str,
        /// The value that was handed in.
        value: u64,
        /// The largest value the wire field can carry.
        wire_max: u64,
    },
    /// The value fits its wire field and is still outside the accepted range.
    FieldOutOfRange {
        /// Field name, as it appears in `docs/protocol.md`.
        field: &'static str,
        /// The value that was handed in.
        value: u64,
    },
    /// The payload is longer than the u16 length field can describe.
    PayloadTooLong {
        /// Bytes the payload would occupy.
        needed: usize,
        /// The largest payload a frame can carry.
        max: usize,
    },
    /// A field value the decoder would reject, refused at encode time so that
    /// an encoder can never produce bytes its own decoder refuses.
    InvalidFieldValue(InvalidField),
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EncodeError::NotRepresentable {
                field,
                value,
                wire_max,
            } => write!(
                f,
                "{} = {} does not fit its wire field (max {})",
                field, value, wire_max
            ),
            EncodeError::FieldOutOfRange { field, value } => {
                write!(f, "{} = {} is outside the accepted range", field, value)
            }
            EncodeError::PayloadTooLong { needed, max } => write!(
                f,
                "payload of {} bytes exceeds the {} byte maximum",
                needed, max
            ),
            EncodeError::InvalidFieldValue(field) => write!(f, "{}", field),
        }
    }
}

impl std::error::Error for EncodeError {}

/// What a decoder made of one frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameOutcome {
    /// A catalogued message, fully validated.
    Decoded(Message),
    /// A type byte outside the catalog. The frame was stepped over using its
    /// length prefix and the session carries on.
    SkippedUnknownType {
        /// The unassigned type byte.
        message_type: u8,
        /// Payload the frame declared, which was skipped unread.
        payload_len: usize,
    },
    /// The frame was refused. Only this frame.
    Rejected(DecodeError),
}

impl FrameOutcome {
    /// The message, if this frame decoded.
    pub fn message(&self) -> Option<&Message> {
        match self {
            FrameOutcome::Decoded(m) => Some(m),
            _ => None,
        }
    }

    /// Whether this frame decoded.
    pub fn is_decoded(&self) -> bool {
        matches!(self, FrameOutcome::Decoded(_))
    }

    /// Whether this frame was skipped as an unknown type.
    pub fn is_skipped(&self) -> bool {
        matches!(self, FrameOutcome::SkippedUnknownType { .. })
    }

    /// Whether this frame was rejected.
    pub fn is_rejected(&self) -> bool {
        matches!(self, FrameOutcome::Rejected(_))
    }
}

/// One frame's outcome plus how far the decoder got.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameResult {
    /// What happened to the frame.
    pub outcome: FrameOutcome,
    /// Bytes consumed from the front of the buffer.
    ///
    /// Zero means the next frame boundary is not knowable from what is
    /// present, so a caller must stop rather than guess. It never means the
    /// session is over.
    pub consumed: usize,
}

/// Encode one message into a complete frame.
///
/// Returns an error and emits nothing when the message cannot be represented,
/// or when it carries a value the decoder would refuse.
pub fn encode(message: &Message) -> Result<Vec<u8>, EncodeError> {
    let payload = encode_payload(message)?;
    if payload.len() > MAX_PAYLOAD_LEN {
        return Err(EncodeError::PayloadTooLong {
            needed: payload.len(),
            max: MAX_PAYLOAD_LEN,
        });
    }
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.push(message.message_type().to_wire());
    out.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

/// Encode just the payload of a message, without the frame header.
pub fn encode_payload(message: &Message) -> Result<Vec<u8>, EncodeError> {
    match message {
        Message::TimeSync(ts) => Ok(encode_time_sync(ts)),
        Message::AudioChunk(chunk) => encode_audio_chunk(chunk),
    }
}

fn encode_time_sync(ts: &TimeSync) -> Vec<u8> {
    let mut out = Vec::with_capacity(32);
    out.extend_from_slice(&ts.t0_ns.to_be_bytes());
    out.extend_from_slice(&ts.t1_ns.to_be_bytes());
    out.extend_from_slice(&ts.t2_ns.to_be_bytes());
    out.extend_from_slice(&ts.t3_ns.to_be_bytes());
    out
}

fn encode_audio_chunk(chunk: &AudioChunk) -> Result<Vec<u8>, EncodeError> {
    if chunk.channels > u8::MAX as u16 {
        return Err(EncodeError::NotRepresentable {
            field: "channels",
            value: chunk.channels as u64,
            wire_max: u8::MAX as u64,
        });
    }
    if chunk.channels == 0 || chunk.channels > MAX_CHANNELS {
        return Err(EncodeError::FieldOutOfRange {
            field: "channels",
            value: chunk.channels as u64,
        });
    }
    if chunk.sample_rate_hz < MIN_SAMPLE_RATE_HZ || chunk.sample_rate_hz > MAX_SAMPLE_RATE_HZ {
        return Err(EncodeError::FieldOutOfRange {
            field: "sample_rate_hz",
            value: chunk.sample_rate_hz as u64,
        });
    }
    if chunk.audio_data.is_empty() {
        return Err(EncodeError::InvalidFieldValue(InvalidField::EmptyAudioData));
    }
    let frame_len = chunk.channels as usize * chunk.sample_format.bytes_per_sample();
    if chunk.audio_data.len() % frame_len != 0 {
        return Err(EncodeError::InvalidFieldValue(
            InvalidField::AudioDataNotFrameAligned {
                data_len: chunk.audio_data.len(),
                frame_len,
            },
        ));
    }
    let total = CHUNK_HEADER_LEN + chunk.audio_data.len();
    if total > MAX_PAYLOAD_LEN {
        return Err(EncodeError::PayloadTooLong {
            needed: total,
            max: MAX_PAYLOAD_LEN,
        });
    }

    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&chunk.sequence.to_be_bytes());
    out.extend_from_slice(&chunk.timestamp_ns.to_be_bytes());
    out.extend_from_slice(&chunk.sample_rate_hz.to_be_bytes());
    out.push(chunk.channels as u8);
    out.push(chunk.sample_format.to_wire());
    out.extend_from_slice(&chunk.reserved);
    out.extend_from_slice(&chunk.audio_data);
    Ok(out)
}

/// Decode the frame at the front of `buf`.
///
/// Never panics and never reads past the end of `buf`, whatever the length
/// field claims.
pub fn decode_frame(buf: &[u8]) -> FrameResult {
    // 1. Is there a whole header?
    if buf.len() < HEADER_LEN {
        return FrameResult {
            outcome: FrameOutcome::Rejected(DecodeError::TruncatedHeader {
                available: buf.len(),
                needed: HEADER_LEN,
            }),
            consumed: 0,
        };
    }

    let type_byte = buf[0];
    let declared = u16::from_be_bytes([buf[1], buf[2]]) as usize;
    let available = buf.len() - HEADER_LEN;

    // 2. Does the declared length fit in what is actually here? This runs
    //    before any slice of the payload is taken.
    if declared > available {
        return FrameResult {
            outcome: FrameOutcome::Rejected(DecodeError::DeclaredLengthExceedsBuffer {
                declared,
                available,
            }),
            consumed: 0,
        };
    }

    let frame_len = HEADER_LEN + declared;
    let payload = &buf[HEADER_LEN..frame_len];

    // 3. Unknown type: step over it, do not fail the session.
    let message_type = match MessageType::from_wire(type_byte) {
        Some(t) => t,
        None => {
            return FrameResult {
                outcome: FrameOutcome::SkippedUnknownType {
                    message_type: type_byte,
                    payload_len: declared,
                },
                consumed: frame_len,
            };
        }
    };

    // 4. Long enough for what it claims to be?
    let minimum = message_type.min_payload_len();
    if declared < minimum {
        return FrameResult {
            outcome: FrameOutcome::Rejected(DecodeError::PayloadTooShortForType {
                message_type: type_byte,
                declared,
                minimum,
            }),
            consumed: frame_len,
        };
    }

    // 5. Field values.
    let outcome = match decode_payload(message_type, payload) {
        Ok(message) => FrameOutcome::Decoded(message),
        Err(field) => FrameOutcome::Rejected(DecodeError::InvalidFieldValue {
            message_type: type_byte,
            field,
        }),
    };
    FrameResult {
        outcome,
        consumed: frame_len,
    }
}

/// Decode a payload that check 4 has already found long enough for its type.
///
/// Private on purpose: every index below is safe only because the caller
/// checked the minimum first, so this is not a surface anyone outside this
/// module gets to hand a short slice to.
///
/// A payload longer than the fields this version knows about is accepted and
/// the excess ignored, which is how a decoder built today survives a field
/// added tomorrow. Re-encoding such a message produces the canonical, shorter
/// form.
fn decode_payload(message_type: MessageType, payload: &[u8]) -> Result<Message, InvalidField> {
    debug_assert!(payload.len() >= message_type.min_payload_len());
    match message_type {
        MessageType::TimeSync => Ok(Message::TimeSync(TimeSync {
            t0_ns: read_u64_be(payload, 0),
            t1_ns: read_u64_be(payload, 8),
            t2_ns: read_u64_be(payload, 16),
            t3_ns: read_u64_be(payload, 24),
        })),
        MessageType::AudioChunk => {
            let sequence = read_u32_be(payload, 0);
            let timestamp_ns = read_u64_be(payload, 4);
            let sample_rate_hz = read_u32_be(payload, 12);
            let channels_byte = payload[16];
            let format_byte = payload[17];

            let sample_format = match SampleFormat::from_wire(format_byte) {
                Some(f) => f,
                None => return Err(InvalidField::UndefinedSampleFormat(format_byte)),
            };
            if channels_byte == 0 || channels_byte as u16 > MAX_CHANNELS {
                return Err(InvalidField::ChannelsOutOfRange(channels_byte));
            }
            if sample_rate_hz < MIN_SAMPLE_RATE_HZ || sample_rate_hz > MAX_SAMPLE_RATE_HZ {
                return Err(InvalidField::SampleRateOutOfRange(sample_rate_hz));
            }

            let mut reserved = [0u8; RESERVED_LEN];
            reserved.copy_from_slice(&payload[RESERVED_OFFSET..RESERVED_OFFSET + RESERVED_LEN]);

            let audio_data = &payload[CHUNK_HEADER_LEN..];
            if audio_data.is_empty() {
                return Err(InvalidField::EmptyAudioData);
            }
            let frame_len = channels_byte as usize * sample_format.bytes_per_sample();
            if audio_data.len() % frame_len != 0 {
                return Err(InvalidField::AudioDataNotFrameAligned {
                    data_len: audio_data.len(),
                    frame_len,
                });
            }

            Ok(Message::AudioChunk(AudioChunk {
                sequence,
                timestamp_ns,
                sample_rate_hz,
                channels: channels_byte as u16,
                sample_format,
                reserved,
                audio_data: audio_data.to_vec(),
            }))
        }
    }
}

fn read_u64_be(src: &[u8], at: usize) -> u64 {
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&src[at..at + 8]);
    u64::from_be_bytes(bytes)
}

fn read_u32_be(src: &[u8], at: usize) -> u32 {
    let mut bytes = [0u8; 4];
    bytes.copy_from_slice(&src[at..at + 4]);
    u32::from_be_bytes(bytes)
}
