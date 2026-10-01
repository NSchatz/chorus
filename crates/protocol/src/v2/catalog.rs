//! The v2 message catalog: type bytes, enumerations and limits.
//!
//! `docs/protocol.md` ("Version 2") is the normative definition; this module
//! repeats its tables in code and does not extend them.

/// The protocol version this module speaks, as it appears in `handshake_init`
/// and `hello`.
pub const PROTOCOL_VERSION: u16 = 2;

/// The four magic bytes that open every `handshake_init`: ASCII `CHRS`.
pub const MAGIC: [u8; 4] = *b"CHRS";

/// Largest UTF-8 text field carried with a one-byte length (names, ids).
pub const MAX_SHORT_TEXT: usize = 255;

/// Largest UTF-8 text field carried with a two-byte length (metadata).
pub const MAX_LONG_TEXT: usize = 1024;

/// Largest added output delay a server may ask for: 5 s, in nanoseconds.
pub const MAX_OUTPUT_DELAY_NS: u64 = 5_000_000_000;

/// Largest artwork image a sender may announce: 4 MiB.
pub const MAX_ARTWORK_LEN: u32 = 4 * 1024 * 1024;

/// Largest number of visualizer bands in one frame.
pub const MAX_VISUALIZER_BANDS: usize = 64;

/// Largest number of sample rates a `capabilities` message lists.
pub const MAX_RATES: usize = 16;

/// Full scale in a `room_volume` gain or limit: thousandths of full
/// amplitude, the unit the control catalog's volume uses
/// (`docs/control-plane.md`), so a room's volume crosses from the control
/// plane to the audio wire without a conversion.
pub const ROOM_VOLUME_FULL: u16 = 1000;

/// Longest ramp a `room_volume` may ask for: 60 s, in milliseconds. Long
/// enough for an alarm's ramp up or a sleep timer's fade out; anything longer
/// is a server stepping the gain itself.
pub const MAX_ROOM_VOLUME_RAMP_MS: u16 = 60_000;

/// `sound` (0x39): bass and treble, whole dB, inclusive.
pub const SOUND_TONE_DB: (i8, i8) = (-10, 10);

/// `sound`: the crossover between a bonded set's mains and its sub, Hz.
pub const SOUND_CROSSOVER_HZ: (u16, u16) = (40, 200);

/// `sound`: the sub's level trim, hundredths of a dB (-12.00 to +6.00).
pub const SOUND_SUB_LEVEL_CDB: (i16, i16) = (-1200, 600);

/// `sound`: most room-correction filters one message carries. These four
/// `SOUND_EQ_*` bounds are the control catalog's `ROOM_EQ_*`
/// (`crates/control/src/sound.rs`), repeated here because this crate depends
/// on nothing; a test in `crates/server` holds the two equal.
pub const SOUND_EQ_MAX_FILTERS: usize = 8;

/// `sound`: a room-correction filter's centre frequency, Hz.
pub const SOUND_EQ_FREQ_HZ: (u16, u16) = (20, 1000);

/// `sound`: a room-correction filter's gain, hundredths of a dB.
pub const SOUND_EQ_GAIN_CDB: (i16, i16) = (-1200, 300);

/// `sound`: a room-correction filter's Q, thousandths.
pub const SOUND_EQ_Q_MILLI: (u16, u16) = (500, 10_000);

/// `low_latency_offer` (0x16): data chunks per XOR parity group when FEC is
/// on, inclusive. `fec_k` 0 is "no FEC"; 1 would be a copy, not a parity.
/// The upper bound is the design envelope's (goal 13) and keeps a group's
/// received mask in 16 bits.
pub const LOW_LATENCY_FEC_K: (u8, u8) = (2, 16);

/// `low_latency_offer`: the column-interleave depth, inclusive; 1 is none.
/// ASSUMED upper bound (8): the decoder holds three blocks of `k x depth`
/// chunks, and SMPTE ST 2022-1 decoders are reported to cap L x D at 100
/// (`docs/decisions/`, the low-latency path record, cites the source).
pub const LOW_LATENCY_FEC_DEPTH: (u8, u8) = (1, 8);

/// `low_latency_offer`: most PCM frames per chunk. A data plaintext (the
/// 32-byte chunk header and the PCM) must leave room for the parity's 8-byte
/// header inside one 1472-byte datagram, so it is at most 1432 bytes and the
/// PCM at most 1400: 700 frames of the narrowest frame the format has (mono
/// 16-bit). Whether a given format fits is [`crate::v2::lowlat::chunk_fits`].
pub const LOW_LATENCY_MAX_CHUNK_FRAMES: u32 = 700;

/// `low_latency_offer`: the largest informational `latency_ns`, the same 5 s
/// bound as `output_delay`.
pub const LOW_LATENCY_MAX_LATENCY_NS: u64 = MAX_OUTPUT_DELAY_NS;

/// The bits of `capabilities`' trailing `features` byte (goal 13). Unlike
/// `sound`'s flags, a bit no version defines is accepted and kept: a feature
/// is something an endpoint CAN do, and a server that does not know one simply
/// never uses it, so refusing the whole `capabilities` would turn a newer
/// endpoint into no endpoint at all.
pub mod features {
    /// The endpoint takes a low-latency UDP stream (`low_latency_offer`).
    pub const LOW_LATENCY: u8 = 1 << 0;
    /// Every defined bit.
    pub const DEFINED: u8 = LOW_LATENCY;
    /// Names, in bit order, for fixtures and diagnostics.
    pub const NAMES: [(u8, &str); 1] = [(LOW_LATENCY, "low_latency")];
}

/// The bits of `sound`'s `flags` byte. A bit outside [`sound_flags::DEFINED`]
/// is rejected (`Problem::Undefined`), not ignored: a later flag is a later
/// version's, and an endpoint that played on without knowing it would sound
/// different from one that knew.
pub mod sound_flags {
    /// Loudness compensation.
    pub const LOUDNESS: u8 = 1 << 0;
    /// Night mode (the compressor).
    pub const NIGHT: u8 = 1 << 1;
    /// Speech enhancement.
    pub const SPEECH: u8 = 1 << 2;
    /// The room-correction filters are applied.
    pub const ROOM_EQ: u8 = 1 << 3;
    /// The sub's polarity is inverted.
    pub const SUB_INVERTED: u8 = 1 << 4;
    /// Every defined bit.
    pub const DEFINED: u8 = LOUDNESS | NIGHT | SPEECH | ROOM_EQ | SUB_INVERTED;
    /// Names, in bit order, for fixtures and diagnostics.
    pub const NAMES: [(u8, &str); 5] = [
        (LOUDNESS, "loudness"),
        (NIGHT, "night"),
        (SPEECH, "speech"),
        (ROOM_EQ, "room_eq"),
        (SUB_INVERTED, "sub_inverted"),
    ];
}

/// `sound`'s `tv_upmix` (goal 13, the optional theater block after the
/// filters): 0 off, 1 ambient. A value past [`SOUND_TV_UPMIX_MAX`] is rejected
/// (`Problem::Undefined`), as an unknown flag is.
pub const SOUND_TV_UPMIX_MAX: u8 = 1;

/// The bits of `sound`'s `fold` byte (goal 13, the theater block): which
/// positions the room's bonded set lacks, so a front member folds the
/// stream's channels for them into its own (ITU-R BS.775-4 Table 2). A bit
/// outside [`sound_fold::DEFINED`] is rejected, as `flags`' are.
pub mod sound_fold {
    /// The set has no centre member.
    pub const CENTRE: u8 = 1 << 0;
    /// The set has no surround pair.
    pub const SURROUND: u8 = 1 << 1;
    /// Every defined bit.
    pub const DEFINED: u8 = CENTRE | SURROUND;
    /// Names, in bit order, for fixtures and diagnostics.
    pub const NAMES: [(u8, &str); 2] = [(CENTRE, "centre"), (SURROUND, "surround")];
}

/// `source_offer`'s optional `reason` (goal 13): why an input's signal is
/// what it is. A value past [`signal_reason::MAX`] is rejected (`Undefined`).
pub mod signal_reason {
    /// No reason given (every offer before goal 13).
    pub const NONE: u8 = 0;
    /// The TV went to standby (CEC): a TV autoplay stops at once, no hold.
    pub const STANDBY: u8 = 1;
    /// The input carries an encoded (IEC 61937) bitstream, not PCM.
    pub const NON_PCM: u8 = 2;
    /// The highest defined.
    pub const MAX: u8 = NON_PCM;
    /// Names, for fixtures and diagnostics.
    pub const NAMES: [(u8, &str); 3] = [(NONE, "none"), (STANDBY, "standby"), (NON_PCM, "non_pcm")];
}

/// A message type in the v2 catalog.
///
/// Types 0x01 to 0x03 are the v1 messages, carried into v2 unchanged
/// byte for byte; their golden vectors are the ones at the top of
/// `fixtures/protocol/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Type {
    /// 0x01, the RFC 5905 section 8 four-timestamp exchange (v1, unchanged).
    TimeSync,
    /// 0x02, a chunk of PCM on the server timeline (v1, unchanged).
    AudioChunk,
    /// 0x03, the in-band end of a stream (v1, unchanged).
    StreamEnd,
    /// 0x10, who a peer is and which roles it takes.
    Hello,
    /// 0x11, what a player or source endpoint can accept.
    Capabilities,
    /// 0x12, the announcement that precedes a stream: codec, format, channel map.
    StreamFormat,
    /// 0x13, one FLAC or Opus packet on the server timeline.
    CodedChunk,
    /// 0x14, the server's added output delay for one endpoint.
    OutputDelay,
    /// 0x15, an endpoint's periodic health and sync report.
    Telemetry,
    /// 0x16, the server offering (or ending) a low-latency UDP stream.
    LowLatencyOffer,
    /// 0x17, an endpoint's answer to a `low_latency_offer`.
    LowLatencyAccept,
    /// 0x20, the first frame of a session: magic, version, suite, Noise message 1.
    HandshakeInit,
    /// 0x21, Noise message 2.
    HandshakeResponse,
    /// 0x22, Noise message 3.
    HandshakeFinish,
    /// 0x23, a session refused, with the reason named.
    SessionRefused,
    /// 0x24, one encrypted record carrying whole v2 frames.
    SecureRecord,
    /// 0x30, now playing (metadata role).
    Metadata,
    /// 0x31, one piece of an artwork image (metadata role).
    Artwork,
    /// 0x32, a command from a controller (controller role).
    ControllerCommand,
    /// 0x33, the state a controller shows (controller role).
    ControllerState,
    /// 0x34, levels and beat for one instant (visualizer role).
    VisualizerFrame,
    /// 0x35, a colour for an endpoint's lights (visualizer role).
    Color,
    /// 0x36, an input an endpoint can share (source role).
    SourceOffer,
    /// 0x37, the server starting or stopping a shared input (source role).
    SourceControl,
    /// 0x38, the room's gain and limit for a player (player role).
    RoomVolume,
    /// 0x39, the room's sound settings for one player (player role).
    Sound,
}

impl Type {
    /// Every type in the catalog, in wire order.
    pub const ALL: [Type; 26] = [
        Type::TimeSync,
        Type::AudioChunk,
        Type::StreamEnd,
        Type::Hello,
        Type::Capabilities,
        Type::StreamFormat,
        Type::CodedChunk,
        Type::OutputDelay,
        Type::Telemetry,
        Type::LowLatencyOffer,
        Type::LowLatencyAccept,
        Type::HandshakeInit,
        Type::HandshakeResponse,
        Type::HandshakeFinish,
        Type::SessionRefused,
        Type::SecureRecord,
        Type::Metadata,
        Type::Artwork,
        Type::ControllerCommand,
        Type::ControllerState,
        Type::VisualizerFrame,
        Type::Color,
        Type::SourceOffer,
        Type::SourceControl,
        Type::RoomVolume,
        Type::Sound,
    ];

    /// The wire byte.
    pub fn to_wire(self) -> u8 {
        match self {
            Type::TimeSync => 0x01,
            Type::AudioChunk => 0x02,
            Type::StreamEnd => 0x03,
            Type::Hello => 0x10,
            Type::Capabilities => 0x11,
            Type::StreamFormat => 0x12,
            Type::CodedChunk => 0x13,
            Type::OutputDelay => 0x14,
            Type::Telemetry => 0x15,
            Type::LowLatencyOffer => 0x16,
            Type::LowLatencyAccept => 0x17,
            Type::HandshakeInit => 0x20,
            Type::HandshakeResponse => 0x21,
            Type::HandshakeFinish => 0x22,
            Type::SessionRefused => 0x23,
            Type::SecureRecord => 0x24,
            Type::Metadata => 0x30,
            Type::Artwork => 0x31,
            Type::ControllerCommand => 0x32,
            Type::ControllerState => 0x33,
            Type::VisualizerFrame => 0x34,
            Type::Color => 0x35,
            Type::SourceOffer => 0x36,
            Type::SourceControl => 0x37,
            Type::RoomVolume => 0x38,
            Type::Sound => 0x39,
        }
    }

    /// The catalogued type for a wire byte, or `None` when it is unassigned.
    pub fn from_wire(byte: u8) -> Option<Type> {
        Type::ALL.iter().copied().find(|t| t.to_wire() == byte)
    }

    /// Short stable name, used by fixtures and diagnostics.
    pub fn name(self) -> &'static str {
        match self {
            Type::TimeSync => "time_sync",
            Type::AudioChunk => "audio_chunk",
            Type::StreamEnd => "stream_end",
            Type::Hello => "hello",
            Type::Capabilities => "capabilities",
            Type::StreamFormat => "stream_format",
            Type::CodedChunk => "coded_chunk",
            Type::OutputDelay => "output_delay",
            Type::Telemetry => "telemetry",
            Type::LowLatencyOffer => "low_latency_offer",
            Type::LowLatencyAccept => "low_latency_accept",
            Type::HandshakeInit => "handshake_init",
            Type::HandshakeResponse => "handshake_response",
            Type::HandshakeFinish => "handshake_finish",
            Type::SessionRefused => "session_refused",
            Type::SecureRecord => "secure_record",
            Type::Metadata => "metadata",
            Type::Artwork => "artwork",
            Type::ControllerCommand => "controller_command",
            Type::ControllerState => "controller_state",
            Type::VisualizerFrame => "visualizer_frame",
            Type::Color => "color",
            Type::SourceOffer => "source_offer",
            Type::SourceControl => "source_control",
            Type::RoomVolume => "room_volume",
            Type::Sound => "sound",
        }
    }

    /// The catalogued type with this name, if any.
    pub fn from_name(name: &str) -> Option<Type> {
        Type::ALL.iter().copied().find(|t| t.name() == name)
    }

    /// Whether this type was already in the v1 catalog (bytes unchanged).
    pub fn is_v1(self) -> bool {
        matches!(self, Type::TimeSync | Type::AudioChunk | Type::StreamEnd)
    }

    /// Whether this type travels in the clear, outside any `secure_record`.
    ///
    /// Only the handshake, the refusal and the record envelope itself do.
    /// Every other v2 message is carried inside a `secure_record`, and a
    /// session layer refuses one that arrives in the clear.
    pub fn is_plaintext(self) -> bool {
        matches!(
            self,
            Type::HandshakeInit
                | Type::HandshakeResponse
                | Type::HandshakeFinish
                | Type::SessionRefused
                | Type::SecureRecord
        )
    }

    /// Smallest payload a frame of this type can legitimately carry.
    pub fn min_payload_len(self) -> usize {
        match self {
            Type::TimeSync => crate::message::TIME_SYNC_PAYLOAD_LEN,
            Type::AudioChunk => crate::message::CHUNK_HEADER_LEN + 1,
            Type::StreamEnd => crate::message::STREAM_END_PAYLOAD_LEN,
            // version, roles, name length, software length
            Type::Hello => 6,
            // codecs, formats, channels, rate count, one rate, buffer, latency, leds, bands
            Type::Capabilities => 1 + 1 + 1 + 1 + 4 + 2 + 4 + 2 + 1,
            // codec, format, rate, channels, one map entry, frames per chunk, config length
            Type::StreamFormat => 1 + 1 + 4 + 1 + 1 + 4 + 2,
            // sequence, timestamp, frames, one data byte
            Type::CodedChunk => 4 + 8 + 4 + 1,
            Type::OutputDelay => 8,
            Type::Telemetry => 8 + 8 + 4 + 4 + 4 + 4 + 1 + 1 + 2,
            // direction, stream_tag, key, udp_port, chunk_frames, fec_k,
            // fec_depth, latency_ns: all fixed
            Type::LowLatencyOffer => 1 + 4 + 32 + 2 + 4 + 1 + 1 + 8,
            // stream_tag, status, udp_port
            Type::LowLatencyAccept => 4 + 1 + 2,
            // magic, version, suite, the 32-byte ephemeral key
            Type::HandshakeInit => 4 + 2 + 1 + 32,
            // e (32), encrypted s (48), an encrypted payload tag (16)
            Type::HandshakeResponse => 32 + 48 + 16,
            // encrypted s (48), an encrypted payload tag (16)
            Type::HandshakeFinish => 48 + 16,
            // reason, detail length
            Type::SessionRefused => 1 + 2,
            // an AEAD tag and at least a 3-byte frame header
            Type::SecureRecord => 16 + 3,
            // playback, position, duration, position_at, artwork, four lengths
            Type::Metadata => 1 + 4 + 4 + 8 + 4 + 2 * 4,
            // id, total, offset, mime length, one data byte
            Type::Artwork => 4 + 4 + 4 + 1 + 1,
            // command, value, target length
            Type::ControllerCommand => 1 + 2 + 1,
            // volume, muted, playback, group length
            Type::ControllerState => 1 + 1 + 1 + 1,
            // timestamp, beat, peak, band count
            Type::VisualizerFrame => 8 + 1 + 1 + 1,
            // timestamp, r, g, b, brightness, transition
            Type::Color => 8 + 4 + 2,
            // id, kind, signal, name length
            Type::SourceOffer => 1 + 1 + 1 + 1,
            // id, action, codec
            Type::SourceControl => 1 + 1 + 1,
            // gain, limit, ramp
            Type::RoomVolume => 2 + 2 + 2,
            // bass, treble, flags, role, sub_present, crossover, sub level,
            // eq_count (no filter)
            Type::Sound => 1 + 1 + 1 + 1 + 1 + 2 + 2 + 1,
        }
    }
}

/// Defines a one-byte enumeration with wire values and stable names.
macro_rules! wire_enum {
    (
        $(#[$meta:meta])*
        $name:ident { $( $(#[$vmeta:meta])* $variant:ident = $value:expr, $text:expr; )+ }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $( $(#[$vmeta])* $variant, )+
        }

        impl $name {
            /// Every value, in wire order.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            /// The wire byte.
            pub fn to_wire(self) -> u8 {
                match self { $( $name::$variant => $value, )+ }
            }

            /// The value for a wire byte, or `None` when it is undefined.
            pub fn from_wire(byte: u8) -> Option<$name> {
                match byte { $( $value => Some($name::$variant), )+ _ => None }
            }

            /// Short stable name, used by fixtures and diagnostics.
            pub fn name(self) -> &'static str {
                match self { $( $name::$variant => $text, )+ }
            }

            /// The value with this name, if any.
            pub fn from_name(name: &str) -> Option<$name> {
                $name::ALL.iter().copied().find(|v| v.name() == name)
            }
        }
    };
}

wire_enum! {
    /// How the audio of a stream is carried.
    Codec {
        /// Uncompressed PCM in `audio_chunk` frames.
        Pcm = 1, "pcm";
        /// FLAC (RFC 9639), one frame per `coded_chunk`.
        Flac = 2, "flac";
        /// Opus (RFC 6716), one packet per `coded_chunk`.
        Opus = 3, "opus";
    }
}

impl Codec {
    /// This codec's bit in a `capabilities` codec set.
    pub fn bit(self) -> u8 {
        1 << (self.to_wire() - 1)
    }
}

wire_enum! {
    /// A speaker position in a channel map.
    ///
    /// Positions 1 to 18 follow the bit order of `WAVEFORMATEXTENSIBLE`'s
    /// `dwChannelMask` (position `p` is bit `p - 1`), so a WAV file's mask
    /// maps onto them without a table. Position 0 is a mono channel that has
    /// no place in a surround layout. Which transport orders its channels how
    /// (WAV, ALSA, HDMI) is in `docs/protocol.md`, "The channel map".
    ChannelPosition {
        /// A single channel with no surround position.
        Mono = 0, "MONO";
        /// Front left.
        FrontLeft = 1, "FL";
        /// Front right.
        FrontRight = 2, "FR";
        /// Front centre.
        FrontCenter = 3, "FC";
        /// Low-frequency effects.
        LowFrequency = 4, "LFE";
        /// Back (rear) left.
        BackLeft = 5, "BL";
        /// Back (rear) right.
        BackRight = 6, "BR";
        /// Front left of centre.
        FrontLeftOfCenter = 7, "FLC";
        /// Front right of centre.
        FrontRightOfCenter = 8, "FRC";
        /// Back centre.
        BackCenter = 9, "BC";
        /// Side left.
        SideLeft = 10, "SL";
        /// Side right.
        SideRight = 11, "SR";
        /// Top centre.
        TopCenter = 12, "TC";
        /// Top front left.
        TopFrontLeft = 13, "TFL";
        /// Top front centre.
        TopFrontCenter = 14, "TFC";
        /// Top front right.
        TopFrontRight = 15, "TFR";
        /// Top back left.
        TopBackLeft = 16, "TBL";
        /// Top back centre.
        TopBackCenter = 17, "TBC";
        /// Top back right.
        TopBackRight = 18, "TBR";
    }
}

wire_enum! {
    /// Why a session was refused.
    RefusalReason {
        /// The peer does not speak protocol v2 (a v1 peer, or another version).
        ProtocolVersion = 1, "protocol_version";
        /// The endpoint's long-term key is not the one pinned at its adoption.
        KeyChanged = 2, "key_changed";
        /// The handshake failed: a bad message, a failed decryption, a timeout.
        HandshakeFailed = 3, "handshake_failed";
        /// The peer asked for a cipher suite this side does not offer.
        UnsupportedSuite = 4, "unsupported_suite";
        /// The server has no free slot for another endpoint.
        ServerFull = 5, "server_full";
        /// The owner removed this endpoint; it is not re-adopted automatically.
        NotAdopted = 6, "not_adopted";
    }
}

wire_enum! {
    /// The key exchange and ciphers a session uses.
    Suite {
        /// `Noise_XX_25519_ChaChaPoly_SHA256`.
        NoiseXx25519ChaChaPolySha256 = 1, "Noise_XX_25519_ChaChaPoly_SHA256";
    }
}

wire_enum! {
    /// Whether something is playing.
    Playback {
        /// Nothing is playing.
        Stopped = 0, "stopped";
        /// Playing.
        Playing = 1, "playing";
        /// Paused, resumable.
        Paused = 2, "paused";
    }
}

wire_enum! {
    /// What a controller asks for.
    Command {
        /// Start or resume playback.
        Play = 1, "play";
        /// Pause playback.
        Pause = 2, "pause";
        /// Play if paused, pause if playing.
        Toggle = 3, "toggle";
        /// Next item of the input, where the input has one.
        Next = 4, "next";
        /// Previous item of the input, where the input has one.
        Previous = 5, "previous";
        /// Set the volume; `value` is 0 to 100.
        VolumeSet = 6, "volume_set";
        /// Change the volume; `value` is -100 to 100.
        VolumeStep = 7, "volume_step";
        /// Mute (`value` 1) or unmute (`value` 0).
        MuteSet = 8, "mute_set";
        /// Join the room or group named in `target`.
        Join = 9, "join";
        /// Leave the current group and play alone.
        Leave = 10, "leave";
    }
}

wire_enum! {
    /// The kind of input a source endpoint offers.
    ///
    /// There is no microphone kind: a speaker microphone feeds only the
    /// voice path and is never a shareable source (brief section 4.8, I4).
    SourceKind {
        /// An analogue line input.
        LineIn = 1, "line_in";
        /// An optical (TOSLINK) S/PDIF input.
        Optical = 2, "optical";
        /// HDMI ARC or eARC from a TV.
        HdmiArc = 3, "hdmi_arc";
    }
}

wire_enum! {
    /// What the server asks a source endpoint to do.
    SourceAction {
        /// Start sending the input upstream.
        Start = 1, "start";
        /// Stop sending it.
        Stop = 2, "stop";
    }
}

wire_enum! {
    /// Which way a low-latency stream flows (`low_latency_offer.direction`).
    LowLatencyDirection {
        /// Ends the stream named by `stream_tag`; every other field is zero.
        End = 0, "end";
        /// The server sends audio to this endpoint (a player of the room).
        ToEndpoint = 1, "to_endpoint";
        /// This endpoint sends its source to the server (the TV hub).
        FromEndpoint = 2, "from_endpoint";
    }
}

wire_enum! {
    /// An endpoint's answer to a `low_latency_offer`.
    LowLatencyStatus {
        /// Accepted: datagrams may flow.
        Accepted = 0, "accepted";
        /// Refused: the endpoint is on a wireless link (BRIEF.md 5.7: wired only).
        RefusedWireless = 1, "refused_wireless";
        /// Refused: the endpoint could not open a UDP socket.
        RefusedNoSocket = 2, "refused_no_socket";
        /// Refused: the endpoint cannot run the offered FEC (k or depth).
        RefusedFec = 3, "refused_fec";
    }
}

wire_enum! {
    /// How an endpoint reaches the network.
    Link {
        /// Not reported.
        Unknown = 0, "unknown";
        /// Ethernet.
        Wired = 1, "wired";
        /// Wi-Fi.
        Wireless = 2, "wireless";
    }
}

/// The roles a peer takes, as bits of `hello.roles`.
///
/// The player role is the base every speaker takes; the other four are the
/// roles of decision K65.
pub mod roles {
    /// Plays a stream (a speaker).
    pub const PLAYER: u16 = 1 << 0;
    /// Receives now playing and artwork.
    pub const METADATA: u16 = 1 << 1;
    /// Sends controller commands and shows controller state.
    pub const CONTROLLER: u16 = 1 << 2;
    /// Receives visualizer frames and colours.
    pub const VISUALIZER: u16 = 1 << 3;
    /// Offers inputs and streams them upstream.
    pub const SOURCE: u16 = 1 << 4;
    /// Every defined role bit.
    pub const DEFINED: u16 = PLAYER | METADATA | CONTROLLER | VISUALIZER | SOURCE;
    /// Names, in bit order, for fixtures and diagnostics.
    pub const NAMES: [(u16, &str); 5] = [
        (PLAYER, "player"),
        (METADATA, "metadata"),
        (CONTROLLER, "controller"),
        (VISUALIZER, "visualizer"),
        (SOURCE, "source"),
    ];
}

/// Opus frame durations RFC 6716 allows, as frame counts at 48 kHz
/// (2.5, 5, 10, 20, 40 and 60 ms).
pub const OPUS_FRAME_COUNTS_48K: [u32; 6] = [120, 240, 480, 960, 1920, 2880];

/// Length of a FLAC STREAMINFO block's body (RFC 9639 section 8.2).
pub const FLAC_STREAMINFO_LEN: usize = 34;

/// Smallest Opus ID header (RFC 7845 section 5.1), mapping family 0.
pub const OPUS_HEAD_MIN_LEN: usize = 19;
