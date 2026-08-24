//! The message catalog: what chorus puts on the wire, and the field layout of
//! each message.
//!
//! The wire layout is specified in `docs/protocol.md` and the reasoning behind
//! it is in `docs/decisions/0003-wire-protocol-framing.md`. The committed
//! golden vectors under `fixtures/protocol/` are the authority both this
//! implementation and the later C implementation are held to.

/// Payload length of a time sync message: four 64-bit timestamps.
pub const TIME_SYNC_PAYLOAD_LEN: usize = 32;

/// Payload length of a stream end message: the final sequence number and the
/// timestamp one chunk duration past the final chunk.
pub const STREAM_END_PAYLOAD_LEN: usize = 12;

/// Length of the fixed audio chunk header that precedes the PCM bytes.
pub const CHUNK_HEADER_LEN: usize = 32;

/// Opaque bytes reserved inside the audio chunk header.
///
/// No semantics are assigned by this phase. See
/// `docs/decisions/0004-audio-chunk-reserved-bytes.md`.
pub const RESERVED_LEN: usize = 14;

/// Offset of the reserved block inside the audio chunk header.
pub const RESERVED_OFFSET: usize = 18;

/// Largest channel count any endpoint in this system carries.
pub const MAX_CHANNELS: u16 = 8;

/// Lowest sample rate the format accepts.
pub const MIN_SAMPLE_RATE_HZ: u32 = 8_000;

/// Highest sample rate the format accepts.
pub const MAX_SAMPLE_RATE_HZ: u32 = 384_000;

/// A message type that is in the catalog.
///
/// A type byte outside the catalog is not an error: it is a message from a
/// newer peer, and a decoder skips it (BRIEF.md 5.2, forward compatibility).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MessageType {
    /// 0x01, the RFC 5905 section 8 four-timestamp exchange.
    TimeSync,
    /// 0x02, a chunk of PCM on the server timeline.
    AudioChunk,
    /// 0x03, the in-band end of a stream.
    StreamEnd,
}

impl MessageType {
    /// Wire byte for a time sync message.
    pub const TIME_SYNC_BYTE: u8 = 0x01;

    /// Wire byte for an audio chunk message.
    pub const AUDIO_CHUNK_BYTE: u8 = 0x02;

    /// Wire byte for a stream end message.
    pub const STREAM_END_BYTE: u8 = 0x03;

    /// Every type in the catalog, in wire order.
    pub const ALL: [MessageType; 3] = [
        MessageType::TimeSync,
        MessageType::AudioChunk,
        MessageType::StreamEnd,
    ];

    /// The catalogued type for a wire byte, or `None` if it is unassigned.
    pub fn from_wire(byte: u8) -> Option<MessageType> {
        match byte {
            Self::TIME_SYNC_BYTE => Some(MessageType::TimeSync),
            Self::AUDIO_CHUNK_BYTE => Some(MessageType::AudioChunk),
            Self::STREAM_END_BYTE => Some(MessageType::StreamEnd),
            _ => None,
        }
    }

    /// The wire byte for this type.
    pub fn to_wire(self) -> u8 {
        match self {
            MessageType::TimeSync => Self::TIME_SYNC_BYTE,
            MessageType::AudioChunk => Self::AUDIO_CHUNK_BYTE,
            MessageType::StreamEnd => Self::STREAM_END_BYTE,
        }
    }

    /// Short stable name, used by fixtures and diagnostics.
    pub fn name(self) -> &'static str {
        match self {
            MessageType::TimeSync => "time_sync",
            MessageType::AudioChunk => "audio_chunk",
            MessageType::StreamEnd => "stream_end",
        }
    }

    /// The catalogued type with this name, if any.
    pub fn from_name(name: &str) -> Option<MessageType> {
        MessageType::ALL.iter().copied().find(|t| t.name() == name)
    }

    /// Smallest payload a frame of this type can legitimately carry.
    ///
    /// An audio chunk has to carry at least one byte of PCM, which is what
    /// makes a chunk with no samples unrepresentable rather than merely
    /// invalid (`docs/decisions/0005-decoder-frame-validation.md`).
    pub fn min_payload_len(self) -> usize {
        match self {
            MessageType::TimeSync => TIME_SYNC_PAYLOAD_LEN,
            MessageType::AudioChunk => CHUNK_HEADER_LEN + 1,
            MessageType::StreamEnd => STREAM_END_PAYLOAD_LEN,
        }
    }
}

/// How the PCM bytes of an audio chunk are laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SampleFormat {
    /// Signed 16-bit, little-endian.
    PcmS16Le,
    /// Signed 24-bit, little-endian, packed in 3 bytes.
    PcmS24Le,
    /// 32-bit float, little-endian.
    PcmF32Le,
}

impl SampleFormat {
    /// The format for a wire byte, or `None` if the value is undefined.
    pub fn from_wire(byte: u8) -> Option<SampleFormat> {
        match byte {
            1 => Some(SampleFormat::PcmS16Le),
            2 => Some(SampleFormat::PcmS24Le),
            3 => Some(SampleFormat::PcmF32Le),
            _ => None,
        }
    }

    /// The wire byte for this format.
    pub fn to_wire(self) -> u8 {
        match self {
            SampleFormat::PcmS16Le => 1,
            SampleFormat::PcmS24Le => 2,
            SampleFormat::PcmF32Le => 3,
        }
    }

    /// Bytes one sample of one channel occupies.
    pub fn bytes_per_sample(self) -> usize {
        match self {
            SampleFormat::PcmS16Le => 2,
            SampleFormat::PcmS24Le => 3,
            SampleFormat::PcmF32Le => 4,
        }
    }

    /// Short stable name, used by fixtures and diagnostics.
    pub fn name(self) -> &'static str {
        match self {
            SampleFormat::PcmS16Le => "pcm_s16le",
            SampleFormat::PcmS24Le => "pcm_s24le",
            SampleFormat::PcmF32Le => "pcm_f32le",
        }
    }

    /// The format with this name, if any.
    pub fn from_name(name: &str) -> Option<SampleFormat> {
        [
            SampleFormat::PcmS16Le,
            SampleFormat::PcmS24Le,
            SampleFormat::PcmF32Le,
        ]
        .iter()
        .copied()
        .find(|f| f.name() == name)
    }
}

/// The four timestamps of one RFC 5905 section 8 exchange.
///
/// Every one of them is nanoseconds from a monotonic source on the device that
/// took it, never wall clock (BRIEF.md guardrail 4). The two clocks have
/// unrelated epochs, which is the whole reason this exchange exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeSync {
    /// Client transmit, on the client clock.
    pub t0_ns: u64,
    /// Server receive, on the server clock.
    pub t1_ns: u64,
    /// Server transmit, on the server clock.
    pub t2_ns: u64,
    /// Client receive, on the client clock.
    pub t3_ns: u64,
}

impl TimeSync {
    /// Round trip time of the exchange, in nanoseconds.
    ///
    /// `(t3 - t0) - (t2 - t1)`, computed so that it cannot underflow on
    /// timestamps that disagree; a nonsensical exchange yields 0 rather than
    /// wrapping.
    pub fn rtt_ns(&self) -> u64 {
        let elapsed_client = self.t3_ns.saturating_sub(self.t0_ns);
        let elapsed_server = self.t2_ns.saturating_sub(self.t1_ns);
        elapsed_client.saturating_sub(elapsed_server)
    }

    /// Estimated offset of the server clock from the client clock, in
    /// nanoseconds: `((t1 - t0) + (t2 - t3)) / 2`.
    pub fn offset_ns(&self) -> i64 {
        let a = self.t1_ns as i128 - self.t0_ns as i128;
        let b = self.t2_ns as i128 - self.t3_ns as i128;
        ((a + b) / 2) as i64
    }
}

/// One chunk of PCM on the server timeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioChunk {
    /// Monotonically increasing per stream; wraps.
    pub sequence: u32,
    /// When the first sample is due, on the server timeline, in nanoseconds
    /// from a monotonic source.
    pub timestamp_ns: u64,
    /// Sample rate of the PCM in this chunk.
    pub sample_rate_hz: u32,
    /// Channel count. One byte on the wire; wider here so that a value the
    /// wire cannot carry is rejected by the encoder rather than truncated.
    pub channels: u16,
    /// Layout of the PCM bytes.
    pub sample_format: SampleFormat,
    /// Opaque reserved bytes. No semantics this phase; a decoder passes
    /// whatever arrived through untouched.
    pub reserved: [u8; RESERVED_LEN],
    /// The PCM itself, in the announced format's byte order.
    pub audio_data: Vec<u8>,
}

impl AudioChunk {
    /// Bytes one frame (one sample on every channel) occupies.
    ///
    /// Returns `None` when the channel count is zero, where the question has
    /// no answer.
    pub fn frame_len(&self) -> Option<usize> {
        if self.channels == 0 {
            return None;
        }
        Some(self.channels as usize * self.sample_format.bytes_per_sample())
    }
}

/// The end of a stream, sent in band after the final chunk.
///
/// # Why this is a message and not a closed socket
///
/// A transport close and a transport that broke look identical to the peer:
/// both are a read returning zero. Telling them apart by timing is guesswork,
/// and guessing wrong means either counting a clean end as a failure or
/// treating a dead server as a tidy finish. So the end of a stream is data on
/// the connection, sent before the close, and a close without it means the
/// other thing.
///
/// An older decoder that does not know type 0x03 skips it and stays open,
/// which is the forward-compatibility rule `docs/protocol.md` already states.
/// Adding it changes no committed golden vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamEnd {
    /// Sequence number of the final chunk of the stream.
    ///
    /// A receiver that has not seen this sequence knows it is missing audio
    /// that was sent, which is a different thing from the stream being over.
    pub final_sequence: u32,
    /// One configured chunk duration past the presentation timestamp of the
    /// final chunk, on the server timeline.
    ///
    /// `docs/protocol.md` is the normative definition of this field; this
    /// comment repeats its relation and does not extend it. The duration added
    /// is the configured one, never the final chunk's own, so this is the
    /// instant the stream stops being audible only when the final chunk is
    /// full. Only the last chunk of a stream may be short, and when it is,
    /// this instant is a little past the point the audio stops.
    pub end_timestamp_ns: u64,
}

/// Any message in the catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// A time sync exchange.
    TimeSync(TimeSync),
    /// A chunk of PCM.
    AudioChunk(AudioChunk),
    /// The in-band end of a stream.
    StreamEnd(StreamEnd),
}

impl Message {
    /// Which catalogued type this message is.
    pub fn message_type(&self) -> MessageType {
        match self {
            Message::TimeSync(_) => MessageType::TimeSync,
            Message::AudioChunk(_) => MessageType::AudioChunk,
            Message::StreamEnd(_) => MessageType::StreamEnd,
        }
    }
}
