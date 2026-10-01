//! The v2 messages and their fields.
//!
//! Field order and widths are `docs/protocol.md` ("Version 2"). Every
//! timestamp is nanoseconds from a monotonic source on the device that took
//! it, never wall clock (BRIEF.md guardrail 4).

use crate::message::{AudioChunk, SampleFormat, StreamEnd, TimeSync};
use crate::v2::catalog::{
    ChannelPosition, Codec, Command, Link, Playback, RefusalReason, SourceAction, SourceKind,
    Suite, Type,
};

/// Who a peer is and which roles it takes. The first message inside a
/// session, in both directions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hello {
    /// The protocol version the sender speaks; 2.
    pub protocol_version: u16,
    /// Bits of [`crate::v2::roles`].
    pub roles: u16,
    /// A friendly name, which may be empty (an endpoint is named after adoption).
    pub name: String,
    /// The sender's software or firmware version.
    pub software: String,
}

/// What a player or source endpoint can accept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capabilities {
    /// Bit set of [`Codec::bit`]. PCM is always in it.
    pub codecs: u8,
    /// Bit set of PCM layouts the endpoint can play: bit 0 `pcm_s16le`,
    /// bit 1 `pcm_s24le`, bit 2 `pcm_f32le`.
    pub sample_formats: u8,
    /// Most channels the endpoint plays, 1 to 8.
    pub max_channels: u8,
    /// Sample rates the endpoint plays natively, 1 to 16 of them.
    pub sample_rates_hz: Vec<u32>,
    /// Audio the endpoint can hold ahead of its playout point, in ms.
    pub buffer_ms: u16,
    /// The endpoint's own delay from its playout point to sound, in ns.
    pub intrinsic_latency_ns: u32,
    /// Addressable lights for the colour role; 0 when it has none.
    pub led_count: u16,
    /// Most visualizer bands the endpoint wants; 0 when it takes none.
    pub visualizer_bands: u8,
}

/// The announcement that precedes a stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamFormat {
    /// How the audio is carried.
    pub codec: Codec,
    /// The PCM layout: on the wire for [`Codec::Pcm`], after decoding otherwise.
    pub sample_format: SampleFormat,
    /// Sample rate of the decoded audio.
    pub sample_rate_hz: u32,
    /// One position per channel, in the order the channels are interleaved.
    pub channel_map: Vec<ChannelPosition>,
    /// Frames in one nominal chunk or packet.
    pub frames_per_chunk: u32,
    /// Codec setup: empty for PCM, the 34-byte STREAMINFO body for FLAC, the
    /// Opus ID header for Opus.
    pub codec_config: Vec<u8>,
}

/// One FLAC frame or Opus packet on the server timeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodedChunk {
    /// Per stream, wraps; the same sequence space as `audio_chunk`.
    pub sequence: u32,
    /// Presentation time of the first decoded sample, server timeline.
    pub timestamp_ns: u64,
    /// PCM frames this packet decodes to.
    pub frames: u32,
    /// The codec's bytes, opaque to the protocol.
    pub data: Vec<u8>,
}

/// The server's added output delay for one endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputDelay {
    /// Added to every presentation timestamp before playout, 0 to 5 s.
    pub delay_ns: u64,
}

/// An endpoint's periodic report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Telemetry {
    /// When the report was taken, on the endpoint's monotonic clock.
    pub taken_ns: u64,
    /// Estimated playout error; `i64::MIN` when unknown.
    pub sync_error_ns: i64,
    /// Audio buffered ahead of the playout point, in microseconds.
    pub buffer_fill_us: u32,
    /// Underruns since the session began.
    pub underruns: u32,
    /// Resynchronisations since the session began.
    pub resyncs: u32,
    /// The servo's current rate correction, in parts per billion.
    pub correction_ppb: i32,
    /// How the endpoint reaches the network.
    pub link: Link,
    /// Received signal strength in dBm; `i8::MIN` when unknown or wired.
    pub rssi_dbm: i8,
    /// Board temperature in hundredths of a degree Celsius; `i16::MIN` when unknown.
    pub temperature_centi_c: i16,
}

/// The first frame of a session, sent in the clear by the endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandshakeInit {
    /// The version the initiator speaks.
    pub protocol_version: u16,
    /// The key exchange it asks for.
    pub suite: Suite,
    /// Noise message 1: the initiator's ephemeral public key.
    pub noise: Vec<u8>,
}

/// Noise message 2, sent in the clear by the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandshakeResponse {
    /// Noise message 2, opaque to the frame layer.
    pub noise: Vec<u8>,
}

/// Noise message 3, sent in the clear by the endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandshakeFinish {
    /// Noise message 3, opaque to the frame layer.
    pub noise: Vec<u8>,
}

/// A session refused, with the reason named.
///
/// Sent in the clear, so it is information for a log and never a command:
/// a receiver surfaces it and closes, and nothing it holds (a pinned key, an
/// adoption) changes because of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRefused {
    /// Why.
    pub reason: RefusalReason,
    /// A sentence for a person, naming what was refused.
    pub detail: String,
}

/// One encrypted record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecureRecord {
    /// Noise transport ciphertext: whole v2 frames, then the 16-byte tag.
    pub ciphertext: Vec<u8>,
}

/// Now playing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    /// Whether it is playing.
    pub playback: Playback,
    /// Position within the item, in ms, at `position_at_ns`.
    pub position_ms: u32,
    /// Length of the item in ms; 0 when unknown (a live input).
    pub duration_ms: u32,
    /// When `position_ms` was true, on the server timeline.
    pub position_at_ns: u64,
    /// The artwork for this item; 0 when there is none.
    pub artwork_id: u32,
    /// Title; may be empty.
    pub title: String,
    /// Artist; may be empty.
    pub artist: String,
    /// Album; may be empty.
    pub album: String,
    /// The input it arrived on ("UPnP", "Spotify", "Line in"); may be empty.
    pub source: String,
}

/// One piece of an artwork image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artwork {
    /// Which image, as named by `metadata.artwork_id`; never 0.
    pub artwork_id: u32,
    /// Bytes in the whole image, at most 4 MiB.
    pub total_len: u32,
    /// Where this piece starts in the image.
    pub offset: u32,
    /// The image's media type, e.g. `image/jpeg`.
    pub mime: String,
    /// This piece.
    pub data: Vec<u8>,
}

/// A command from a controller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControllerCommand {
    /// What is asked for.
    pub command: Command,
    /// The argument where the command takes one, 0 otherwise.
    pub value: i16,
    /// The room or group for `join`; empty for every other command.
    pub target: String,
}

/// The state a controller shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControllerState {
    /// Volume, 0 to 100.
    pub volume: u8,
    /// Whether muted.
    pub muted: bool,
    /// Whether playing.
    pub playback: Playback,
    /// The room or group the controller's endpoint plays in.
    pub group: String,
}

/// Levels and beat for one instant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisualizerFrame {
    /// When this frame is heard, on the server timeline.
    pub timestamp_ns: u64,
    /// Beat strength, 0 for none.
    pub beat: u8,
    /// Peak level, 0 to 255.
    pub peak: u8,
    /// Band levels, low to high, at most 64.
    pub bands: Vec<u8>,
}

/// A colour for an endpoint's lights.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    /// When the colour applies, on the server timeline.
    pub timestamp_ns: u64,
    /// Red.
    pub red: u8,
    /// Green.
    pub green: u8,
    /// Blue.
    pub blue: u8,
    /// Brightness, 0 (off) to 255.
    pub brightness: u8,
    /// Fade time from the previous colour, in ms.
    pub transition_ms: u16,
}

/// An input an endpoint can share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceOffer {
    /// The endpoint's own number for the input.
    pub source_id: u8,
    /// What kind of input it is.
    pub kind: SourceKind,
    /// Whether a signal is present right now.
    pub signal: bool,
    /// Its name, which may be empty.
    pub name: String,
    /// (goal 13) Why the signal is what it is, a
    /// [`crate::v2::catalog::signal_reason`] value: 0 none given, 1 the TV
    /// went to standby (autoplay stops it at once, without the hold), 2 the
    /// input carries an encoded bitstream chorus does not decode. Written only
    /// when not 0, after `name`; absent decodes as 0.
    pub reason: u8,
}

/// The server starting or stopping a shared input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceControl {
    /// The endpoint's number for the input.
    pub source_id: u8,
    /// Start or stop.
    pub action: SourceAction,
    /// The codec the server wants the input sent in.
    pub codec: Codec,
}

/// A room's gain and limit, from the server to a player.
///
/// The player enforces it: what it plays at every frame is the least of the
/// ramped `gain`, the last `limit` received and its own configured ceiling
/// (`docs/protocol.md`, "0x38 room volume"; brief section 4.8, I10), so a
/// gain above the limit plays at the limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoomVolume {
    /// What to play at, in thousandths of full amplitude, 0 to 1000; already
    /// 0 when the room is muted.
    pub gain: u16,
    /// The room's effective limit, in thousandths, 0 to 1000.
    pub limit: u16,
    /// How long to move from the gain being applied to `gain`, linear in
    /// amplitude, in ms, 0 to 60000; 0 is at once.
    pub ramp_ms: u16,
}

/// One room-correction filter in a `sound`: a peaking EQ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoundFilter {
    /// Centre frequency, Hz, 20 to 1000.
    pub freq_hz: u16,
    /// Gain, hundredths of a dB, -1200 to 300.
    pub gain_cdb: i16,
    /// Q, thousandths, 500 to 10000.
    pub q_milli: u16,
}

/// A room's sound settings, from the server to one player
/// (`docs/protocol.md`, "0x39 sound"): the control catalog's `sound`,
/// `bass_management` and `room_eq` for the player's room, plus where this
/// player sits in the room's bonded set. Every player of a set receives the
/// room's whole stream, so bass management is the player's own work, from
/// `role` and `sub_present`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sound {
    /// Low-shelf gain, whole dB, -10 to 10.
    pub bass_db: i8,
    /// High-shelf gain, whole dB, -10 to 10.
    pub treble_db: i8,
    /// [`crate::v2::catalog::sound_flags`] bits; any other bit is rejected.
    pub flags: u8,
    /// The player's channel position in the room's bonded set
    /// ([`crate::v2::ChannelPosition`]'s numbers), 0 when it is in no set.
    pub role: u8,
    /// Whether the room's set has an LFE member (bass management on).
    pub sub_present: bool,
    /// The crossover, Hz, 40 to 200.
    pub crossover_hz: u16,
    /// The sub's level trim, hundredths of a dB, -1200 to 600.
    pub sub_level_cdb: i16,
    /// The room-correction filters, at most 8.
    pub filters: Vec<SoundFilter>,
    /// Goal 13, the theater block: what a surround member plays from a
    /// stream with no surround channel, 0 off or 1 ambient. With `fold`, the
    /// block is on the wire only when one of them is not 0, so a goal-12
    /// `sound` is byte for byte what it was; absent decodes as 0.
    pub tv_upmix: u8,
    /// Goal 13, the theater block: [`crate::v2::catalog::sound_fold`] bits,
    /// what the room's set lacks; any other bit is rejected.
    pub fold: u8,
}

/// Any message in the v2 catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// 0x01.
    TimeSync(TimeSync),
    /// 0x02.
    AudioChunk(AudioChunk),
    /// 0x03.
    StreamEnd(StreamEnd),
    /// 0x10.
    Hello(Hello),
    /// 0x11.
    Capabilities(Capabilities),
    /// 0x12.
    StreamFormat(StreamFormat),
    /// 0x13.
    CodedChunk(CodedChunk),
    /// 0x14.
    OutputDelay(OutputDelay),
    /// 0x15.
    Telemetry(Telemetry),
    /// 0x20.
    HandshakeInit(HandshakeInit),
    /// 0x21.
    HandshakeResponse(HandshakeResponse),
    /// 0x22.
    HandshakeFinish(HandshakeFinish),
    /// 0x23.
    SessionRefused(SessionRefused),
    /// 0x24.
    SecureRecord(SecureRecord),
    /// 0x30.
    Metadata(Metadata),
    /// 0x31.
    Artwork(Artwork),
    /// 0x32.
    ControllerCommand(ControllerCommand),
    /// 0x33.
    ControllerState(ControllerState),
    /// 0x34.
    VisualizerFrame(VisualizerFrame),
    /// 0x35.
    Color(Color),
    /// 0x36.
    SourceOffer(SourceOffer),
    /// 0x37.
    SourceControl(SourceControl),
    /// 0x38.
    RoomVolume(RoomVolume),
    /// 0x39.
    Sound(Sound),
}

impl Message {
    /// Which catalogued type this message is.
    pub fn message_type(&self) -> Type {
        match self {
            Message::TimeSync(_) => Type::TimeSync,
            Message::AudioChunk(_) => Type::AudioChunk,
            Message::StreamEnd(_) => Type::StreamEnd,
            Message::Hello(_) => Type::Hello,
            Message::Capabilities(_) => Type::Capabilities,
            Message::StreamFormat(_) => Type::StreamFormat,
            Message::CodedChunk(_) => Type::CodedChunk,
            Message::OutputDelay(_) => Type::OutputDelay,
            Message::Telemetry(_) => Type::Telemetry,
            Message::HandshakeInit(_) => Type::HandshakeInit,
            Message::HandshakeResponse(_) => Type::HandshakeResponse,
            Message::HandshakeFinish(_) => Type::HandshakeFinish,
            Message::SessionRefused(_) => Type::SessionRefused,
            Message::SecureRecord(_) => Type::SecureRecord,
            Message::Metadata(_) => Type::Metadata,
            Message::Artwork(_) => Type::Artwork,
            Message::ControllerCommand(_) => Type::ControllerCommand,
            Message::ControllerState(_) => Type::ControllerState,
            Message::VisualizerFrame(_) => Type::VisualizerFrame,
            Message::Color(_) => Type::Color,
            Message::SourceOffer(_) => Type::SourceOffer,
            Message::SourceControl(_) => Type::SourceControl,
            Message::RoomVolume(_) => Type::RoomVolume,
            Message::Sound(_) => Type::Sound,
        }
    }
}
