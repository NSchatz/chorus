//! Encoding and decoding v2 frames.
//!
//! The frame is v1's: a type byte, a big-endian u16 payload length, then the
//! payload. The decoder's checks run in v1's order (`docs/protocol.md`,
//! "Decoder behaviour"), and a type outside the v2 catalog is skipped by its
//! length prefix. One validation routine serves both directions: the encoder
//! refuses exactly what the decoder would reject, so valid encoder output
//! always decodes.

use std::fmt;

use crate::codec::{
    decode_frame as decode_v1_frame, encode_payload as encode_v1_payload,
    DecodeError as V1DecodeError, EncodeError as V1EncodeError, FrameOutcome as V1Outcome,
    HEADER_LEN, MAX_PAYLOAD_LEN,
};
use crate::message::{Message as V1Message, SampleFormat, MAX_CHANNELS};
use crate::message::{MAX_SAMPLE_RATE_HZ, MIN_SAMPLE_RATE_HZ};
use crate::v2::catalog::{
    roles, ChannelPosition, Codec, Command, Link, LowLatencyDirection, LowLatencyStatus, Playback,
    RefusalReason, SourceAction, SourceKind, Suite, Type, FLAC_STREAMINFO_LEN,
    LOW_LATENCY_FEC_DEPTH, LOW_LATENCY_FEC_K, LOW_LATENCY_MAX_CHUNK_FRAMES,
    LOW_LATENCY_MAX_LATENCY_NS, MAGIC, MAX_ARTWORK_LEN, MAX_LONG_TEXT, MAX_OUTPUT_DELAY_NS,
    MAX_RATES, MAX_ROOM_VOLUME_RAMP_MS, MAX_SHORT_TEXT, MAX_VISUALIZER_BANDS,
    OPUS_FRAME_COUNTS_48K, OPUS_HEAD_MIN_LEN, ROOM_VOLUME_FULL, SOUND_CROSSOVER_HZ,
    SOUND_EQ_FREQ_HZ, SOUND_EQ_GAIN_CDB, SOUND_EQ_MAX_FILTERS, SOUND_EQ_Q_MILLI,
    SOUND_SUB_LEVEL_CDB, SOUND_TONE_DB,
};
use crate::v2::catalog::{signal_reason, sound_flags, sound_fold, SOUND_TV_UPMIX_MAX};
use crate::v2::messages::*;

/// What is wrong with a field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// An enumeration byte with no defined meaning.
    Undefined(u64),
    /// A number outside the accepted range.
    OutOfRange(i128),
    /// A length-prefixed field runs past the end of the payload.
    Truncated,
    /// Text that is not UTF-8.
    NotUtf8,
    /// Text or a list longer than its field allows.
    TooLong {
        /// Bytes or entries that were there.
        len: usize,
        /// The most the field allows.
        max: usize,
    },
    /// Fields that contradict each other; the sentence says how.
    Inconsistent(&'static str),
}

/// A field value that is not one the format accepts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError {
    /// The field, as `docs/protocol.md` names it.
    pub field: &'static str,
    /// What is wrong with it.
    pub problem: Problem,
}

impl fmt::Display for FieldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.problem {
            Problem::Undefined(v) => write!(f, "{}: undefined value {}", self.field, v),
            Problem::OutOfRange(v) => {
                write!(f, "{}: {} is outside the accepted range", self.field, v)
            }
            Problem::Truncated => write!(f, "{}: runs past the end of the payload", self.field),
            Problem::NotUtf8 => write!(f, "{}: is not UTF-8", self.field),
            Problem::TooLong { len, max } => {
                write!(
                    f,
                    "{}: {} is longer than the {} allowed",
                    self.field, len, max
                )
            }
            Problem::Inconsistent(why) => write!(f, "{}: {}", self.field, why),
        }
    }
}

fn err<T>(field: &'static str, problem: Problem) -> Result<T, FieldError> {
    Err(FieldError { field, problem })
}

/// Why a frame was rejected. Each rejects one frame and never closes a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// Fewer than 3 bytes remain.
    TruncatedHeader {
        /// Bytes that remained.
        available: usize,
    },
    /// The length field declares more payload than the buffer holds.
    DeclaredLengthExceedsBuffer {
        /// What the length field declared.
        declared: usize,
        /// What was there after the header.
        available: usize,
    },
    /// The payload is shorter than this type's minimum.
    PayloadTooShortForType {
        /// The type, which is in the catalog.
        message_type: Type,
        /// What the length field declared.
        declared: usize,
        /// The minimum for the type.
        minimum: usize,
    },
    /// A field carries a value the format cannot accept.
    InvalidField {
        /// The type, which is in the catalog.
        message_type: Type,
        /// What was wrong.
        error: FieldError,
    },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::TruncatedHeader { available } => write!(
                f,
                "truncated frame header: {} bytes available, {} needed",
                available, HEADER_LEN
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
                "{} declares a {} byte payload, minimum is {}",
                message_type.name(),
                declared,
                minimum
            ),
            DecodeError::InvalidField {
                message_type,
                error,
            } => write!(f, "{}: {}", message_type.name(), error),
        }
    }
}

impl std::error::Error for DecodeError {}

/// Why a message was not encoded. Nothing is emitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodeError {
    /// A field the decoder would reject.
    InvalidField(FieldError),
    /// The payload is longer than a frame can carry.
    PayloadTooLong {
        /// Bytes it would take.
        needed: usize,
    },
    /// A v1 message the v1 encoder refused.
    V1(V1EncodeError),
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EncodeError::InvalidField(e) => write!(f, "{}", e),
            EncodeError::PayloadTooLong { needed } => write!(
                f,
                "payload of {} bytes exceeds the {} byte maximum",
                needed, MAX_PAYLOAD_LEN
            ),
            EncodeError::V1(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for EncodeError {}

/// What the decoder made of one frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// A catalogued message, fully validated.
    Decoded(Message),
    /// A type outside the v2 catalog, stepped over by its length prefix.
    SkippedUnknownType {
        /// The unassigned type byte.
        message_type: u8,
        /// Payload that was skipped unread.
        payload_len: usize,
    },
    /// The frame was refused. Only this frame.
    Rejected(DecodeError),
}

impl Outcome {
    /// The message, if the frame decoded.
    pub fn message(&self) -> Option<&Message> {
        match self {
            Outcome::Decoded(m) => Some(m),
            _ => None,
        }
    }
}

/// One frame's outcome and how many bytes it consumed.
///
/// `consumed` is zero only when the next frame boundary is unknowable (a
/// truncated header, or a length longer than the buffer): the caller stops
/// and waits for more bytes rather than guessing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    /// What happened.
    pub outcome: Outcome,
    /// Bytes consumed from the front of the buffer.
    pub consumed: usize,
}

/// Encode one v2 message into a whole frame.
pub fn encode(message: &Message) -> Result<Vec<u8>, EncodeError> {
    let payload = encode_payload(message)?;
    if payload.len() > MAX_PAYLOAD_LEN {
        return Err(EncodeError::PayloadTooLong {
            needed: payload.len(),
        });
    }
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.push(message.message_type().to_wire());
    out.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

/// Encode one v2 message's payload, without the frame header.
pub fn encode_payload(message: &Message) -> Result<Vec<u8>, EncodeError> {
    if let Some(v1) = as_v1(message) {
        return encode_v1_payload(&v1).map_err(EncodeError::V1);
    }
    validate(message).map_err(EncodeError::InvalidField)?;
    let mut w = Writer::default();
    match message {
        Message::TimeSync(_) | Message::AudioChunk(_) | Message::StreamEnd(_) => {
            unreachable!("v1 messages are encoded above")
        }
        Message::Hello(m) => {
            w.u16(m.protocol_version);
            w.u16(m.roles);
            w.short_text(&m.name);
            w.short_text(&m.software);
        }
        Message::Capabilities(m) => {
            w.u8(m.codecs);
            w.u8(m.sample_formats);
            w.u8(m.max_channels);
            w.u8(m.sample_rates_hz.len() as u8);
            for rate in &m.sample_rates_hz {
                w.u32(*rate);
            }
            w.u16(m.buffer_ms);
            w.u32(m.intrinsic_latency_ns);
            w.u16(m.led_count);
            w.u8(m.visualizer_bands);
            // Trailing and optional: written only when a bit is set, so a
            // capabilities without features keeps its pre-goal-13 bytes.
            if m.features != 0 {
                w.u8(m.features);
            }
        }
        Message::StreamFormat(m) => {
            w.u8(m.codec.to_wire());
            w.u8(m.sample_format.to_wire());
            w.u32(m.sample_rate_hz);
            w.u8(m.channel_map.len() as u8);
            for position in &m.channel_map {
                w.u8(position.to_wire());
            }
            w.u32(m.frames_per_chunk);
            w.u16(m.codec_config.len() as u16);
            w.bytes(&m.codec_config);
        }
        Message::CodedChunk(m) => {
            w.u32(m.sequence);
            w.u64(m.timestamp_ns);
            w.u32(m.frames);
            w.bytes(&m.data);
        }
        Message::OutputDelay(m) => w.u64(m.delay_ns),
        Message::LowLatencyOffer(m) => {
            w.u8(m.direction.to_wire());
            w.u32(m.stream_tag);
            w.bytes(&m.key);
            w.u16(m.udp_port);
            w.u32(m.chunk_frames);
            w.u8(m.fec_k);
            w.u8(m.fec_depth);
            w.u64(m.latency_ns);
        }
        Message::LowLatencyAccept(m) => {
            w.u32(m.stream_tag);
            w.u8(m.status.to_wire());
            w.u16(m.udp_port);
        }
        Message::Telemetry(m) => {
            w.u64(m.taken_ns);
            w.u64(m.sync_error_ns as u64);
            w.u32(m.buffer_fill_us);
            w.u32(m.underruns);
            w.u32(m.resyncs);
            w.u32(m.correction_ppb as u32);
            w.u8(m.link.to_wire());
            w.u8(m.rssi_dbm as u8);
            w.u16(m.temperature_centi_c as u16);
        }
        Message::HandshakeInit(m) => {
            w.bytes(&MAGIC);
            w.u16(m.protocol_version);
            w.u8(m.suite.to_wire());
            w.bytes(&m.noise);
        }
        Message::HandshakeResponse(m) => w.bytes(&m.noise),
        Message::HandshakeFinish(m) => w.bytes(&m.noise),
        Message::SessionRefused(m) => {
            w.u8(m.reason.to_wire());
            w.long_text(&m.detail);
        }
        Message::SecureRecord(m) => w.bytes(&m.ciphertext),
        Message::Metadata(m) => {
            w.u8(m.playback.to_wire());
            w.u32(m.position_ms);
            w.u32(m.duration_ms);
            w.u64(m.position_at_ns);
            w.u32(m.artwork_id);
            w.long_text(&m.title);
            w.long_text(&m.artist);
            w.long_text(&m.album);
            w.long_text(&m.source);
        }
        Message::Artwork(m) => {
            w.u32(m.artwork_id);
            w.u32(m.total_len);
            w.u32(m.offset);
            w.short_text(&m.mime);
            w.bytes(&m.data);
        }
        Message::ControllerCommand(m) => {
            w.u8(m.command.to_wire());
            w.u16(m.value as u16);
            w.short_text(&m.target);
        }
        Message::ControllerState(m) => {
            w.u8(m.volume);
            w.u8(m.muted as u8);
            w.u8(m.playback.to_wire());
            w.short_text(&m.group);
        }
        Message::VisualizerFrame(m) => {
            w.u64(m.timestamp_ns);
            w.u8(m.beat);
            w.u8(m.peak);
            w.u8(m.bands.len() as u8);
            w.bytes(&m.bands);
        }
        Message::Color(m) => {
            w.u64(m.timestamp_ns);
            w.u8(m.red);
            w.u8(m.green);
            w.u8(m.blue);
            w.u8(m.brightness);
            w.u16(m.transition_ms);
        }
        Message::SourceOffer(m) => {
            w.u8(m.source_id);
            w.u8(m.kind.to_wire());
            w.u8(m.signal as u8);
            w.short_text(&m.name);
            // Goal 13's reason only when there is one, so an offer without
            // keeps its bytes and its vector.
            if m.reason != 0 {
                w.u8(m.reason);
            }
        }
        Message::SourceControl(m) => {
            w.u8(m.source_id);
            w.u8(m.action.to_wire());
            w.u8(m.codec.to_wire());
        }
        Message::RoomVolume(m) => {
            w.u16(m.gain);
            w.u16(m.limit);
            w.u16(m.ramp_ms);
        }
        Message::Sound(m) => {
            w.u8(m.bass_db as u8);
            w.u8(m.treble_db as u8);
            w.u8(m.flags);
            w.u8(m.role);
            w.u8(m.sub_present as u8);
            w.u16(m.crossover_hz);
            w.u16(m.sub_level_cdb as u16);
            w.u8(m.filters.len() as u8);
            for f in &m.filters {
                w.u16(f.freq_hz);
                w.u16(f.gain_cdb as u16);
                w.u16(f.q_milli);
            }
            // The theater block (goal 13) only when it says something, so a
            // goal-12 sound keeps its bytes and its vectors.
            if m.tv_upmix != 0 || m.fold != 0 {
                w.u8(m.tv_upmix);
                w.u8(m.fold);
            }
        }
    }
    Ok(w.out)
}

/// Decode the v2 frame at the front of `buf`.
///
/// Never panics and never reads past the end of `buf`.
pub fn decode_frame(buf: &[u8]) -> Decoded {
    // 1. A whole header?
    if buf.len() < HEADER_LEN {
        return Decoded {
            outcome: Outcome::Rejected(DecodeError::TruncatedHeader {
                available: buf.len(),
            }),
            consumed: 0,
        };
    }
    let type_byte = buf[0];
    let declared = u16::from_be_bytes([buf[1], buf[2]]) as usize;
    let available = buf.len() - HEADER_LEN;
    // 2. Does the declared length fit? Checked before any slice is taken.
    if declared > available {
        return Decoded {
            outcome: Outcome::Rejected(DecodeError::DeclaredLengthExceedsBuffer {
                declared,
                available,
            }),
            consumed: 0,
        };
    }
    let frame_len = HEADER_LEN + declared;
    // 3. Unknown type: step over it.
    let Some(message_type) = Type::from_wire(type_byte) else {
        return Decoded {
            outcome: Outcome::SkippedUnknownType {
                message_type: type_byte,
                payload_len: declared,
            },
            consumed: frame_len,
        };
    };
    // 4. Long enough for its type?
    let minimum = message_type.min_payload_len();
    if declared < minimum {
        return Decoded {
            outcome: Outcome::Rejected(DecodeError::PayloadTooShortForType {
                message_type,
                declared,
                minimum,
            }),
            consumed: frame_len,
        };
    }
    // 5. Field values. The v1 types are v1's own decoder, unchanged.
    let outcome = if message_type.is_v1() {
        match decode_v1_frame(&buf[..frame_len]).outcome {
            V1Outcome::Decoded(m) => Outcome::Decoded(from_v1(m)),
            V1Outcome::Rejected(V1DecodeError::InvalidFieldValue { field, .. }) => {
                Outcome::Rejected(DecodeError::InvalidField {
                    message_type,
                    error: FieldError {
                        field: "v1 field",
                        problem: Problem::Inconsistent(v1_field_sentence(field)),
                    },
                })
            }
            other => unreachable!("checks 1 to 4 already passed for a v1 type: {:?}", other),
        }
    } else {
        let payload = &buf[HEADER_LEN..frame_len];
        match decode_payload(message_type, payload).and_then(|m| validate(&m).map(|()| m)) {
            Ok(m) => Outcome::Decoded(m),
            Err(error) => Outcome::Rejected(DecodeError::InvalidField {
                message_type,
                error,
            }),
        }
    };
    Decoded {
        outcome,
        consumed: frame_len,
    }
}

/// Decode every frame in `buf`, in order, stopping where the next boundary
/// is unknowable. Returns the outcomes and the bytes consumed.
pub fn decode_all(buf: &[u8]) -> (Vec<Outcome>, usize) {
    let mut outcomes = Vec::new();
    let mut at = 0;
    while at < buf.len() {
        let d = decode_frame(&buf[at..]);
        if d.consumed == 0 {
            break;
        }
        outcomes.push(d.outcome);
        at += d.consumed;
    }
    (outcomes, at)
}

fn v1_field_sentence(field: crate::codec::InvalidField) -> &'static str {
    use crate::codec::InvalidField as F;
    match field {
        F::UndefinedSampleFormat(_) => "undefined sample format",
        F::ChannelsOutOfRange(_) => "channel count outside 1 to 8",
        F::SampleRateOutOfRange(_) => "sample rate outside 8000 to 384000 Hz",
        F::AudioDataNotFrameAligned { .. } => "PCM is not a whole number of frames",
        F::EmptyAudioData => "audio chunk carries no samples",
    }
}

fn as_v1(message: &Message) -> Option<V1Message> {
    match message {
        Message::TimeSync(m) => Some(V1Message::TimeSync(*m)),
        Message::AudioChunk(m) => Some(V1Message::AudioChunk(m.clone())),
        Message::StreamEnd(m) => Some(V1Message::StreamEnd(*m)),
        _ => None,
    }
}

fn from_v1(message: V1Message) -> Message {
    match message {
        V1Message::TimeSync(m) => Message::TimeSync(m),
        V1Message::AudioChunk(m) => Message::AudioChunk(m),
        V1Message::StreamEnd(m) => Message::StreamEnd(m),
    }
}

/// Parse a payload that check 4 found long enough. Structure only: lengths
/// are bounds-checked here and values are judged by [`validate`].
fn decode_payload(message_type: Type, payload: &[u8]) -> Result<Message, FieldError> {
    let mut r = Reader {
        buf: payload,
        at: 0,
    };
    let m = match message_type {
        Type::TimeSync | Type::AudioChunk | Type::StreamEnd => {
            unreachable!("v1 types are decoded by the v1 decoder")
        }
        Type::Hello => Message::Hello(Hello {
            protocol_version: r.u16("protocol_version")?,
            roles: r.u16("roles")?,
            name: r.short_text("name")?,
            software: r.short_text("software")?,
        }),
        Type::Capabilities => {
            let codecs = r.u8("codecs")?;
            let sample_formats = r.u8("sample_formats")?;
            let max_channels = r.u8("max_channels")?;
            let count = r.u8("rate_count")? as usize;
            let mut sample_rates_hz = Vec::with_capacity(count);
            for _ in 0..count {
                sample_rates_hz.push(r.u32("sample_rates_hz")?);
            }
            Message::Capabilities(Capabilities {
                codecs,
                sample_formats,
                max_channels,
                sample_rates_hz,
                buffer_ms: r.u16("buffer_ms")?,
                intrinsic_latency_ns: r.u32("intrinsic_latency_ns")?,
                led_count: r.u16("led_count")?,
                visualizer_bands: r.u8("visualizer_bands")?,
                // Absent (a pre-goal-13 endpoint) reads as no features.
                features: r.optional_u8(),
            })
        }
        Type::StreamFormat => {
            let codec_byte = r.u8("codec")?;
            let codec = Codec::from_wire(codec_byte).ok_or(FieldError {
                field: "codec",
                problem: Problem::Undefined(codec_byte as u64),
            })?;
            let format_byte = r.u8("sample_format")?;
            let sample_format = SampleFormat::from_wire(format_byte).ok_or(FieldError {
                field: "sample_format",
                problem: Problem::Undefined(format_byte as u64),
            })?;
            let sample_rate_hz = r.u32("sample_rate_hz")?;
            let channels = r.u8("channels")? as usize;
            let mut channel_map = Vec::with_capacity(channels);
            for _ in 0..channels {
                let b = r.u8("channel_map")?;
                channel_map.push(ChannelPosition::from_wire(b).ok_or(FieldError {
                    field: "channel_map",
                    problem: Problem::Undefined(b as u64),
                })?);
            }
            let frames_per_chunk = r.u32("frames_per_chunk")?;
            let config_len = r.u16("codec_config_len")? as usize;
            let codec_config = r.take("codec_config", config_len)?.to_vec();
            Message::StreamFormat(StreamFormat {
                codec,
                sample_format,
                sample_rate_hz,
                channel_map,
                frames_per_chunk,
                codec_config,
            })
        }
        Type::CodedChunk => Message::CodedChunk(CodedChunk {
            sequence: r.u32("sequence")?,
            timestamp_ns: r.u64("timestamp_ns")?,
            frames: r.u32("frames")?,
            data: r.rest().to_vec(),
        }),
        Type::OutputDelay => Message::OutputDelay(OutputDelay {
            delay_ns: r.u64("delay_ns")?,
        }),
        Type::Telemetry => Message::Telemetry(Telemetry {
            taken_ns: r.u64("taken_ns")?,
            sync_error_ns: r.u64("sync_error_ns")? as i64,
            buffer_fill_us: r.u32("buffer_fill_us")?,
            underruns: r.u32("underruns")?,
            resyncs: r.u32("resyncs")?,
            correction_ppb: r.u32("correction_ppb")? as i32,
            link: {
                let b = r.u8("link")?;
                Link::from_wire(b).ok_or(FieldError {
                    field: "link",
                    problem: Problem::Undefined(b as u64),
                })?
            },
            rssi_dbm: r.u8("rssi_dbm")? as i8,
            temperature_centi_c: r.u16("temperature_centi_c")? as i16,
        }),
        Type::LowLatencyOffer => {
            let b = r.u8("direction")?;
            let direction = LowLatencyDirection::from_wire(b).ok_or(FieldError {
                field: "direction",
                problem: Problem::Undefined(b as u64),
            })?;
            let stream_tag = r.u32("stream_tag")?;
            let mut key = [0u8; 32];
            key.copy_from_slice(r.take("key", 32)?);
            Message::LowLatencyOffer(LowLatencyOffer {
                direction,
                stream_tag,
                key,
                udp_port: r.u16("udp_port")?,
                chunk_frames: r.u32("chunk_frames")?,
                fec_k: r.u8("fec_k")?,
                fec_depth: r.u8("fec_depth")?,
                latency_ns: r.u64("latency_ns")?,
            })
        }
        Type::LowLatencyAccept => {
            let stream_tag = r.u32("stream_tag")?;
            let b = r.u8("status")?;
            Message::LowLatencyAccept(LowLatencyAccept {
                stream_tag,
                status: LowLatencyStatus::from_wire(b).ok_or(FieldError {
                    field: "status",
                    problem: Problem::Undefined(b as u64),
                })?,
                udp_port: r.u16("udp_port")?,
            })
        }
        Type::HandshakeInit => {
            let magic = r.take("magic", 4)?;
            if magic != MAGIC {
                return err("magic", Problem::Inconsistent("is not ASCII CHRS"));
            }
            let protocol_version = r.u16("protocol_version")?;
            let suite_byte = r.u8("suite")?;
            let suite = Suite::from_wire(suite_byte).ok_or(FieldError {
                field: "suite",
                problem: Problem::Undefined(suite_byte as u64),
            })?;
            Message::HandshakeInit(HandshakeInit {
                protocol_version,
                suite,
                noise: r.rest().to_vec(),
            })
        }
        Type::HandshakeResponse => Message::HandshakeResponse(HandshakeResponse {
            noise: r.rest().to_vec(),
        }),
        Type::HandshakeFinish => Message::HandshakeFinish(HandshakeFinish {
            noise: r.rest().to_vec(),
        }),
        Type::SessionRefused => {
            let b = r.u8("reason")?;
            Message::SessionRefused(SessionRefused {
                reason: RefusalReason::from_wire(b).ok_or(FieldError {
                    field: "reason",
                    problem: Problem::Undefined(b as u64),
                })?,
                detail: r.long_text("detail")?,
            })
        }
        Type::SecureRecord => Message::SecureRecord(SecureRecord {
            ciphertext: r.rest().to_vec(),
        }),
        Type::Metadata => {
            let b = r.u8("playback")?;
            Message::Metadata(Metadata {
                playback: Playback::from_wire(b).ok_or(FieldError {
                    field: "playback",
                    problem: Problem::Undefined(b as u64),
                })?,
                position_ms: r.u32("position_ms")?,
                duration_ms: r.u32("duration_ms")?,
                position_at_ns: r.u64("position_at_ns")?,
                artwork_id: r.u32("artwork_id")?,
                title: r.long_text("title")?,
                artist: r.long_text("artist")?,
                album: r.long_text("album")?,
                source: r.long_text("source")?,
            })
        }
        Type::Artwork => Message::Artwork(Artwork {
            artwork_id: r.u32("artwork_id")?,
            total_len: r.u32("total_len")?,
            offset: r.u32("offset")?,
            mime: r.short_text("mime")?,
            data: r.rest().to_vec(),
        }),
        Type::ControllerCommand => {
            let b = r.u8("command")?;
            Message::ControllerCommand(ControllerCommand {
                command: Command::from_wire(b).ok_or(FieldError {
                    field: "command",
                    problem: Problem::Undefined(b as u64),
                })?,
                value: r.u16("value")? as i16,
                target: r.short_text("target")?,
            })
        }
        Type::ControllerState => {
            let volume = r.u8("volume")?;
            let muted = r.bool("muted")?;
            let b = r.u8("playback")?;
            Message::ControllerState(ControllerState {
                volume,
                muted,
                playback: Playback::from_wire(b).ok_or(FieldError {
                    field: "playback",
                    problem: Problem::Undefined(b as u64),
                })?,
                group: r.short_text("group")?,
            })
        }
        Type::VisualizerFrame => {
            let timestamp_ns = r.u64("timestamp_ns")?;
            let beat = r.u8("beat")?;
            let peak = r.u8("peak")?;
            let count = r.u8("band_count")? as usize;
            Message::VisualizerFrame(VisualizerFrame {
                timestamp_ns,
                beat,
                peak,
                bands: r.take("bands", count)?.to_vec(),
            })
        }
        Type::Color => Message::Color(Color {
            timestamp_ns: r.u64("timestamp_ns")?,
            red: r.u8("red")?,
            green: r.u8("green")?,
            blue: r.u8("blue")?,
            brightness: r.u8("brightness")?,
            transition_ms: r.u16("transition_ms")?,
        }),
        Type::SourceOffer => {
            let source_id = r.u8("source_id")?;
            let b = r.u8("kind")?;
            Message::SourceOffer(SourceOffer {
                source_id,
                kind: SourceKind::from_wire(b).ok_or(FieldError {
                    field: "kind",
                    problem: Problem::Undefined(b as u64),
                })?,
                signal: r.bool("signal")?,
                name: r.short_text("name")?,
                reason: if r.remaining() > 0 {
                    r.u8("reason")?
                } else {
                    0
                },
            })
        }
        Type::SourceControl => {
            let source_id = r.u8("source_id")?;
            let a = r.u8("action")?;
            let c = r.u8("codec")?;
            Message::SourceControl(SourceControl {
                source_id,
                action: SourceAction::from_wire(a).ok_or(FieldError {
                    field: "action",
                    problem: Problem::Undefined(a as u64),
                })?,
                codec: Codec::from_wire(c).ok_or(FieldError {
                    field: "codec",
                    problem: Problem::Undefined(c as u64),
                })?,
            })
        }
        Type::RoomVolume => Message::RoomVolume(RoomVolume {
            gain: r.u16("gain")?,
            limit: r.u16("limit")?,
            ramp_ms: r.u16("ramp_ms")?,
        }),
        Type::Sound => {
            let bass_db = r.u8("bass_db")? as i8;
            let treble_db = r.u8("treble_db")? as i8;
            let flags = r.u8("flags")?;
            let role = r.u8("role")?;
            let sub_present = r.bool("sub_present")?;
            let crossover_hz = r.u16("crossover_hz")?;
            let sub_level_cdb = r.u16("sub_level_cdb")? as i16;
            let count = r.u8("eq_count")?;
            // Checked before the filters are read, so a count past the
            // bound is that field out of range (as the C decoder, which has
            // room for eight, says it) and never a read of nine.
            if usize::from(count) > SOUND_EQ_MAX_FILTERS {
                return err("eq_count", Problem::OutOfRange(count as i128));
            }
            let mut filters = Vec::with_capacity(usize::from(count));
            for _ in 0..count {
                filters.push(SoundFilter {
                    freq_hz: r.u16("freq_hz")?,
                    gain_cdb: r.u16("gain_cdb")? as i16,
                    q_milli: r.u16("q_milli")?,
                });
            }
            // The theater block (goal 13): each field read when its byte is
            // there, 0 when it is not (a goal-12 server's sound).
            let tv_upmix = if r.remaining() > 0 {
                r.u8("tv_upmix")?
            } else {
                0
            };
            let fold = if r.remaining() > 0 { r.u8("fold")? } else { 0 };
            Message::Sound(Sound {
                bass_db,
                treble_db,
                flags,
                role,
                sub_present,
                crossover_hz,
                sub_level_cdb,
                filters,
                tv_upmix,
                fold,
            })
        }
    };
    // Bytes past the last known field are a later version's fields: ignored.
    Ok(m)
}

/// The value rules of `docs/protocol.md`, applied the same way before
/// encoding and after decoding.
pub fn validate(message: &Message) -> Result<(), FieldError> {
    match message {
        Message::TimeSync(_) | Message::AudioChunk(_) | Message::StreamEnd(_) => Ok(()),
        Message::Hello(m) => {
            if m.protocol_version == 0 {
                return err("protocol_version", Problem::OutOfRange(0));
            }
            if m.roles & !roles::DEFINED != 0 {
                return err("roles", Problem::Undefined(m.roles as u64));
            }
            short_text_ok("name", &m.name)?;
            short_text_ok("software", &m.software)
        }
        Message::Capabilities(m) => {
            if m.codecs & Codec::Pcm.bit() == 0 {
                return err("codecs", Problem::Inconsistent("PCM is mandatory"));
            }
            let defined_codecs = Codec::ALL.iter().fold(0u8, |a, c| a | c.bit());
            if m.codecs & !defined_codecs != 0 {
                return err("codecs", Problem::Undefined(m.codecs as u64));
            }
            if m.sample_formats == 0 || m.sample_formats & !0b111 != 0 {
                return err(
                    "sample_formats",
                    Problem::Undefined(m.sample_formats as u64),
                );
            }
            if m.max_channels == 0 || m.max_channels as u16 > MAX_CHANNELS {
                return err("max_channels", Problem::OutOfRange(m.max_channels as i128));
            }
            if m.sample_rates_hz.is_empty() || m.sample_rates_hz.len() > MAX_RATES {
                return err(
                    "sample_rates_hz",
                    Problem::TooLong {
                        len: m.sample_rates_hz.len(),
                        max: MAX_RATES,
                    },
                );
            }
            for rate in &m.sample_rates_hz {
                rate_ok("sample_rates_hz", *rate)?;
            }
            if m.visualizer_bands as usize > MAX_VISUALIZER_BANDS {
                return err(
                    "visualizer_bands",
                    Problem::OutOfRange(m.visualizer_bands as i128),
                );
            }
            Ok(())
        }
        Message::StreamFormat(m) => {
            rate_ok("sample_rate_hz", m.sample_rate_hz)?;
            let n = m.channel_map.len();
            if n == 0 || n as u16 > MAX_CHANNELS {
                return err("channels", Problem::OutOfRange(n as i128));
            }
            if n > 1 && m.channel_map.contains(&ChannelPosition::Mono) {
                return err(
                    "channel_map",
                    Problem::Inconsistent("MONO is only for a one-channel stream"),
                );
            }
            for (i, p) in m.channel_map.iter().enumerate() {
                if m.channel_map[..i].contains(p) {
                    return err(
                        "channel_map",
                        Problem::Inconsistent("a position appears twice"),
                    );
                }
            }
            if m.frames_per_chunk == 0 {
                return err("frames_per_chunk", Problem::OutOfRange(0));
            }
            match m.codec {
                Codec::Pcm => {
                    if !m.codec_config.is_empty() {
                        return err(
                            "codec_config",
                            Problem::Inconsistent("PCM carries no codec setup"),
                        );
                    }
                }
                Codec::Flac => {
                    if m.codec_config.len() != FLAC_STREAMINFO_LEN {
                        return err(
                            "codec_config",
                            Problem::Inconsistent("FLAC setup is the 34-byte STREAMINFO body"),
                        );
                    }
                    if m.sample_format == SampleFormat::PcmF32Le {
                        return err(
                            "sample_format",
                            Problem::Inconsistent("FLAC decodes to integer PCM"),
                        );
                    }
                }
                Codec::Opus => {
                    if m.sample_rate_hz != 48_000 {
                        return err(
                            "sample_rate_hz",
                            Problem::Inconsistent("Opus streams decode at 48000 Hz"),
                        );
                    }
                    if !OPUS_FRAME_COUNTS_48K.contains(&m.frames_per_chunk) {
                        return err(
                            "frames_per_chunk",
                            Problem::Inconsistent("not an Opus frame duration"),
                        );
                    }
                    if m.codec_config.len() < OPUS_HEAD_MIN_LEN
                        || &m.codec_config[..8] != b"OpusHead"
                    {
                        return err(
                            "codec_config",
                            Problem::Inconsistent("Opus setup is an OpusHead ID header"),
                        );
                    }
                    if m.codec_config[9] as usize != n {
                        return err(
                            "codec_config",
                            Problem::Inconsistent(
                                "OpusHead channel count differs from the channel map",
                            ),
                        );
                    }
                }
            }
            Ok(())
        }
        Message::CodedChunk(m) => {
            if m.frames == 0 {
                return err("frames", Problem::OutOfRange(0));
            }
            if m.data.is_empty() {
                return err(
                    "data",
                    Problem::Inconsistent("a packet carries at least one byte"),
                );
            }
            Ok(())
        }
        Message::OutputDelay(m) => {
            if m.delay_ns > MAX_OUTPUT_DELAY_NS {
                return err("delay_ns", Problem::OutOfRange(m.delay_ns as i128));
            }
            Ok(())
        }
        Message::Telemetry(_) => Ok(()),
        Message::LowLatencyOffer(m) => validate_low_latency_offer(m),
        Message::LowLatencyAccept(m) => {
            if m.stream_tag == 0 {
                return err("stream_tag", Problem::OutOfRange(0));
            }
            if m.status != LowLatencyStatus::Accepted && m.udp_port != 0 {
                return err("udp_port", Problem::Inconsistent("a refusal names no port"));
            }
            Ok(())
        }
        Message::HandshakeInit(m) => {
            if m.noise.len() < 32 {
                return err("noise", Problem::Truncated);
            }
            Ok(())
        }
        Message::HandshakeResponse(m) => {
            if m.noise.len() < Type::HandshakeResponse.min_payload_len() {
                return err("noise", Problem::Truncated);
            }
            Ok(())
        }
        Message::HandshakeFinish(m) => {
            if m.noise.len() < Type::HandshakeFinish.min_payload_len() {
                return err("noise", Problem::Truncated);
            }
            Ok(())
        }
        Message::SessionRefused(m) => long_text_ok("detail", &m.detail),
        Message::SecureRecord(m) => {
            if m.ciphertext.len() < Type::SecureRecord.min_payload_len() {
                return err("ciphertext", Problem::Truncated);
            }
            Ok(())
        }
        Message::Metadata(m) => {
            long_text_ok("title", &m.title)?;
            long_text_ok("artist", &m.artist)?;
            long_text_ok("album", &m.album)?;
            long_text_ok("source", &m.source)
        }
        Message::Artwork(m) => {
            if m.artwork_id == 0 {
                return err("artwork_id", Problem::OutOfRange(0));
            }
            if m.total_len == 0 || m.total_len > MAX_ARTWORK_LEN {
                return err("total_len", Problem::OutOfRange(m.total_len as i128));
            }
            if m.mime.is_empty() || !m.mime.is_ascii() {
                return err(
                    "mime",
                    Problem::Inconsistent("a media type is non-empty ASCII"),
                );
            }
            short_text_ok("mime", &m.mime)?;
            if m.data.is_empty() {
                return err(
                    "data",
                    Problem::Inconsistent("a piece carries at least one byte"),
                );
            }
            if m.offset as u64 + m.data.len() as u64 > m.total_len as u64 {
                return err(
                    "offset",
                    Problem::Inconsistent("the piece ends past total_len"),
                );
            }
            Ok(())
        }
        Message::ControllerCommand(m) => {
            let v = m.value as i128;
            let ok = match m.command {
                Command::VolumeSet => (0..=100).contains(&v),
                Command::VolumeStep => (-100..=100).contains(&v),
                Command::MuteSet => (0..=1).contains(&v),
                _ => v == 0,
            };
            if !ok {
                return err("value", Problem::OutOfRange(v));
            }
            if (m.command == Command::Join) == m.target.is_empty() {
                return err(
                    "target",
                    Problem::Inconsistent("names a room or group for join, and only for join"),
                );
            }
            short_text_ok("target", &m.target)
        }
        Message::ControllerState(m) => {
            if m.volume > 100 {
                return err("volume", Problem::OutOfRange(m.volume as i128));
            }
            short_text_ok("group", &m.group)
        }
        Message::VisualizerFrame(m) => {
            if m.bands.len() > MAX_VISUALIZER_BANDS {
                return err(
                    "bands",
                    Problem::TooLong {
                        len: m.bands.len(),
                        max: MAX_VISUALIZER_BANDS,
                    },
                );
            }
            Ok(())
        }
        Message::Color(_) => Ok(()),
        Message::SourceOffer(m) => {
            short_text_ok("name", &m.name)?;
            if m.reason > signal_reason::MAX {
                return err("reason", Problem::Undefined(m.reason as u64));
            }
            Ok(())
        }
        Message::SourceControl(_) => Ok(()),
        Message::RoomVolume(m) => {
            // Each field on its own: a gain above the limit is a valid
            // message (the player plays it at the limit), a gain or a limit
            // above full scale is not one.
            if m.gain > ROOM_VOLUME_FULL {
                return err("gain", Problem::OutOfRange(m.gain as i128));
            }
            if m.limit > ROOM_VOLUME_FULL {
                return err("limit", Problem::OutOfRange(m.limit as i128));
            }
            if m.ramp_ms > MAX_ROOM_VOLUME_RAMP_MS {
                return err("ramp_ms", Problem::OutOfRange(m.ramp_ms as i128));
            }
            Ok(())
        }
        Message::Sound(m) => validate_sound(m),
    }
}

/// `low_latency_offer`'s rules, field by field in wire order
/// (`docs/protocol.md`, "0x16 low latency offer"). Rejected, never clamped.
fn validate_low_latency_offer(m: &LowLatencyOffer) -> Result<(), FieldError> {
    if m.stream_tag == 0 {
        return err("stream_tag", Problem::OutOfRange(0));
    }
    if m.direction == LowLatencyDirection::End {
        // An end names its stream and nothing else.
        let rest_zero = m.key == [0u8; 32]
            && m.udp_port == 0
            && m.chunk_frames == 0
            && m.fec_k == 0
            && m.fec_depth == 0
            && m.latency_ns == 0;
        if !rest_zero {
            return err(
                "direction",
                Problem::Inconsistent("an end carries only its stream_tag; the rest is zero"),
            );
        }
        return Ok(());
    }
    if m.key == [0u8; 32] {
        return err(
            "key",
            Problem::Inconsistent("a stream key is never all zero"),
        );
    }
    let port_needed = m.direction == LowLatencyDirection::FromEndpoint;
    if port_needed == (m.udp_port == 0) {
        return err(
            "udp_port",
            Problem::Inconsistent(
                "names the server's port for a stream from the endpoint, and only then",
            ),
        );
    }
    if m.chunk_frames == 0 || m.chunk_frames > LOW_LATENCY_MAX_CHUNK_FRAMES {
        return err("chunk_frames", Problem::OutOfRange(m.chunk_frames as i128));
    }
    if m.fec_k != 0 && (m.fec_k < LOW_LATENCY_FEC_K.0 || m.fec_k > LOW_LATENCY_FEC_K.1) {
        return err("fec_k", Problem::OutOfRange(m.fec_k as i128));
    }
    if m.fec_depth < LOW_LATENCY_FEC_DEPTH.0 || m.fec_depth > LOW_LATENCY_FEC_DEPTH.1 {
        return err("fec_depth", Problem::OutOfRange(m.fec_depth as i128));
    }
    if m.fec_k == 0 && m.fec_depth != 1 {
        return err(
            "fec_depth",
            Problem::Inconsistent("a stream without FEC has no interleave"),
        );
    }
    if m.latency_ns > LOW_LATENCY_MAX_LATENCY_NS {
        return err("latency_ns", Problem::OutOfRange(m.latency_ns as i128));
    }
    Ok(())
}

/// `sound`'s rules, field by field in wire order (`docs/protocol.md`, "0x39
/// sound"). Rejected, never clamped, as `room_volume`'s are.
fn validate_sound(m: &Sound) -> Result<(), FieldError> {
    let within = |field: &'static str, v: i128, (lo, hi): (i128, i128)| {
        if v < lo || v > hi {
            err(field, Problem::OutOfRange(v))
        } else {
            Ok(())
        }
    };
    let tone = (SOUND_TONE_DB.0 as i128, SOUND_TONE_DB.1 as i128);
    within("bass_db", m.bass_db as i128, tone)?;
    within("treble_db", m.treble_db as i128, tone)?;
    if m.flags & !sound_flags::DEFINED != 0 {
        return err("flags", Problem::Undefined(m.flags as u64));
    }
    if ChannelPosition::from_wire(m.role).is_none() {
        return err("role", Problem::Undefined(m.role as u64));
    }
    within(
        "crossover_hz",
        m.crossover_hz as i128,
        (SOUND_CROSSOVER_HZ.0 as i128, SOUND_CROSSOVER_HZ.1 as i128),
    )?;
    within(
        "sub_level_cdb",
        m.sub_level_cdb as i128,
        (SOUND_SUB_LEVEL_CDB.0 as i128, SOUND_SUB_LEVEL_CDB.1 as i128),
    )?;
    if m.filters.len() > SOUND_EQ_MAX_FILTERS {
        return err("eq_count", Problem::OutOfRange(m.filters.len() as i128));
    }
    for f in &m.filters {
        within(
            "freq_hz",
            f.freq_hz as i128,
            (SOUND_EQ_FREQ_HZ.0 as i128, SOUND_EQ_FREQ_HZ.1 as i128),
        )?;
        within(
            "gain_cdb",
            f.gain_cdb as i128,
            (SOUND_EQ_GAIN_CDB.0 as i128, SOUND_EQ_GAIN_CDB.1 as i128),
        )?;
        within(
            "q_milli",
            f.q_milli as i128,
            (SOUND_EQ_Q_MILLI.0 as i128, SOUND_EQ_Q_MILLI.1 as i128),
        )?;
    }
    if m.tv_upmix > SOUND_TV_UPMIX_MAX {
        return err("tv_upmix", Problem::Undefined(m.tv_upmix as u64));
    }
    if m.fold & !sound_fold::DEFINED != 0 {
        return err("fold", Problem::Undefined(m.fold as u64));
    }
    Ok(())
}

fn rate_ok(field: &'static str, rate: u32) -> Result<(), FieldError> {
    if !(MIN_SAMPLE_RATE_HZ..=MAX_SAMPLE_RATE_HZ).contains(&rate) {
        return err(field, Problem::OutOfRange(rate as i128));
    }
    Ok(())
}

fn short_text_ok(field: &'static str, text: &str) -> Result<(), FieldError> {
    if text.len() > MAX_SHORT_TEXT {
        return err(
            field,
            Problem::TooLong {
                len: text.len(),
                max: MAX_SHORT_TEXT,
            },
        );
    }
    Ok(())
}

fn long_text_ok(field: &'static str, text: &str) -> Result<(), FieldError> {
    if text.len() > MAX_LONG_TEXT {
        return err(
            field,
            Problem::TooLong {
                len: text.len(),
                max: MAX_LONG_TEXT,
            },
        );
    }
    Ok(())
}

#[derive(Default)]
struct Writer {
    out: Vec<u8>,
}

impl Writer {
    fn u8(&mut self, v: u8) {
        self.out.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.out.extend_from_slice(&v.to_be_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.out.extend_from_slice(&v.to_be_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.out.extend_from_slice(&v.to_be_bytes());
    }
    fn bytes(&mut self, v: &[u8]) {
        self.out.extend_from_slice(v);
    }
    // Lengths were checked by `validate` before anything was written.
    fn short_text(&mut self, v: &str) {
        self.u8(v.len() as u8);
        self.bytes(v.as_bytes());
    }
    fn long_text(&mut self, v: &str) {
        self.u16(v.len() as u16);
        self.bytes(v.as_bytes());
    }
}

struct Reader<'a> {
    buf: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, field: &'static str, n: usize) -> Result<&'a [u8], FieldError> {
        if self.buf.len() - self.at < n {
            return err(field, Problem::Truncated);
        }
        let s = &self.buf[self.at..self.at + n];
        self.at += n;
        Ok(s)
    }
    fn u8(&mut self, field: &'static str) -> Result<u8, FieldError> {
        Ok(self.take(field, 1)?[0])
    }
    fn bool(&mut self, field: &'static str) -> Result<bool, FieldError> {
        match self.u8(field)? {
            0 => Ok(false),
            1 => Ok(true),
            v => err(field, Problem::Undefined(v as u64)),
        }
    }
    fn u16(&mut self, field: &'static str) -> Result<u16, FieldError> {
        let b = self.take(field, 2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }
    fn u32(&mut self, field: &'static str) -> Result<u32, FieldError> {
        let b = self.take(field, 4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn u64(&mut self, field: &'static str) -> Result<u64, FieldError> {
        let mut a = [0u8; 8];
        a.copy_from_slice(self.take(field, 8)?);
        Ok(u64::from_be_bytes(a))
    }
    fn text(&mut self, field: &'static str, n: usize) -> Result<String, FieldError> {
        let b = self.take(field, n)?;
        String::from_utf8(b.to_vec()).map_err(|_| FieldError {
            field,
            problem: Problem::NotUtf8,
        })
    }
    fn short_text(&mut self, field: &'static str) -> Result<String, FieldError> {
        let n = self.u8(field)? as usize;
        self.text(field, n)
    }
    fn long_text(&mut self, field: &'static str) -> Result<String, FieldError> {
        let n = self.u16(field)? as usize;
        if n > MAX_LONG_TEXT {
            return err(
                field,
                Problem::TooLong {
                    len: n,
                    max: MAX_LONG_TEXT,
                },
            );
        }
        self.text(field, n)
    }
    /// A trailing optional byte: 0 when the payload ends here.
    fn optional_u8(&mut self) -> u8 {
        match self.buf.get(self.at) {
            Some(&b) => {
                self.at += 1;
                b
            }
            None => 0,
        }
    }
    /// Bytes not yet read: an optional trailing field is read only when its
    /// byte is there (`sound`'s theater block).
    fn remaining(&self) -> usize {
        self.buf.len() - self.at
    }
    fn rest(&mut self) -> &'a [u8] {
        let s = &self.buf[self.at..];
        self.at = self.buf.len();
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_v1_type_decodes_through_the_v2_decoder_unchanged() {
        let ts = crate::message::TimeSync {
            t0_ns: 1,
            t1_ns: 2,
            t2_ns: 3,
            t3_ns: 4,
        };
        let frame = crate::codec::encode(&V1Message::TimeSync(ts)).unwrap();
        assert_eq!(encode(&Message::TimeSync(ts)).unwrap(), frame);
        assert_eq!(
            decode_frame(&frame).outcome,
            Outcome::Decoded(Message::TimeSync(ts))
        );
    }

    #[test]
    fn an_unassigned_type_is_skipped_by_its_length() {
        let d = decode_frame(&[0x7F, 0x00, 0x02, 0xAA, 0xBB, 0x10]);
        assert_eq!(d.consumed, 5);
        assert!(matches!(
            d.outcome,
            Outcome::SkippedUnknownType {
                message_type: 0x7F,
                payload_len: 2
            }
        ));
    }

    #[test]
    fn a_short_text_that_runs_past_the_payload_is_rejected_not_read() {
        // hello: version 2, roles 1, name length 200 with only 1 byte there.
        let frame = [0x10, 0x00, 0x06, 0x00, 0x02, 0x00, 0x01, 200, 0x41];
        let d = decode_frame(&frame);
        assert_eq!(d.consumed, frame.len());
        match d.outcome {
            Outcome::Rejected(DecodeError::InvalidField { error, .. }) => {
                assert_eq!(
                    error,
                    FieldError {
                        field: "name",
                        problem: Problem::Truncated
                    }
                )
            }
            other => panic!("expected a rejection, got {:?}", other),
        }
    }
}
