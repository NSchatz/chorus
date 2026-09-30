//! Reading the committed v2 vectors under `fixtures/protocol/v2/`.

use std::fs;
use std::path::PathBuf;

use chorus_protocol::v2::noise::Keypair;
use chorus_protocol::v2::*;
use chorus_protocol::SampleFormat;

use super::{fixture_dir, parse_hex, Fields};

/// The v2 vector directory.
pub fn v2_dir() -> PathBuf {
    fixture_dir().join("v2")
}

/// One v2 vector.
pub struct V2Vector {
    /// The file stem.
    pub stem: String,
    /// The committed frame.
    pub frame: Vec<u8>,
    /// The committed fields.
    pub fields: Fields,
    /// The message the fields describe.
    pub message: Message,
}

/// Every `<stem>.hex` in the v2 directory (not its subdirectories), sorted.
pub fn v2_stems() -> Vec<String> {
    let mut stems: Vec<String> = fs::read_dir(v2_dir())
        .expect("fixtures/protocol/v2 exists")
        .map(|e| e.expect("a readable entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "hex"))
        .map(|p| p.file_stem().unwrap().to_string_lossy().into_owned())
        .collect();
    stems.sort();
    stems
}

/// Load one v2 vector by stem.
pub fn load_v2(stem: &str) -> V2Vector {
    let dir = v2_dir();
    let frame = parse_hex(&fs::read_to_string(dir.join(format!("{}.hex", stem))).expect("hex"));
    let source = format!("v2/{}.fields", stem);
    let fields = Fields::parse(
        &source,
        &fs::read_to_string(dir.join(format!("{}.fields", stem))).expect("fields"),
    );
    let message = v2_message_from_fields(&fields);
    V2Vector {
        stem: stem.to_string(),
        frame,
        fields,
        message,
    }
}

fn words(f: &Fields, key: &str) -> Vec<String> {
    f.str(key).split_whitespace().map(str::to_string).collect()
}

fn int(f: &Fields, key: &str) -> i64 {
    f.str(key)
        .parse()
        .unwrap_or_else(|_| panic!("{} is not an integer", key))
}

fn named<T>(what: &str, v: Option<T>) -> T {
    v.unwrap_or_else(|| panic!("{} is not a defined name", what))
}

/// A test key pair from a `.fields` secret.
pub fn keypair(f: &Fields, key: &str) -> Keypair {
    let bytes = f.bytes(key);
    let mut k = [0u8; 32];
    k.copy_from_slice(&bytes);
    Keypair::from_secret(k)
}

/// Build the message a v2 `.fields` file describes.
pub fn v2_message_from_fields(f: &Fields) -> Message {
    let t = named("message_type", Type::from_name(f.str("message_type")));
    match t {
        Type::Hello => Message::Hello(Hello {
            protocol_version: f.u64("protocol_version") as u16,
            roles: words(f, "roles").iter().fold(0, |acc, name| {
                acc | named(
                    name,
                    roles::NAMES
                        .iter()
                        .find(|(_, n)| n == name)
                        .map(|(b, _)| *b),
                )
            }),
            name: f.str("name").to_string(),
            software: f.str("software").to_string(),
        }),
        Type::Capabilities => Message::Capabilities(Capabilities {
            codecs: words(f, "codecs")
                .iter()
                .fold(0, |a, n| a | named(n, Codec::from_name(n)).bit()),
            sample_formats: words(f, "sample_formats").iter().fold(0, |a, n| {
                a | 1 << (named(n, SampleFormat::from_name(n)).to_wire() - 1)
            }),
            max_channels: f.u64("max_channels") as u8,
            sample_rates_hz: words(f, "sample_rates_hz")
                .iter()
                .map(|r| r.parse().unwrap())
                .collect(),
            buffer_ms: f.u64("buffer_ms") as u16,
            intrinsic_latency_ns: f.u64("intrinsic_latency_ns") as u32,
            led_count: f.u64("led_count") as u16,
            visualizer_bands: f.u64("visualizer_bands") as u8,
        }),
        Type::StreamFormat => Message::StreamFormat(StreamFormat {
            codec: named("codec", Codec::from_name(f.str("codec"))),
            sample_format: named(
                "sample_format",
                SampleFormat::from_name(f.str("sample_format")),
            ),
            sample_rate_hz: f.u64("sample_rate_hz") as u32,
            channel_map: words(f, "channel_map")
                .iter()
                .map(|p| named(p, ChannelPosition::from_name(p)))
                .collect(),
            frames_per_chunk: f.u64("frames_per_chunk") as u32,
            codec_config: parse_hex(f.str("codec_config")),
        }),
        Type::CodedChunk => Message::CodedChunk(CodedChunk {
            sequence: f.u64("sequence") as u32,
            timestamp_ns: f.u64("timestamp_ns"),
            frames: f.u64("frames") as u32,
            data: f.bytes("data"),
        }),
        Type::OutputDelay => Message::OutputDelay(OutputDelay {
            delay_ns: f.u64("delay_ns"),
        }),
        Type::Telemetry => Message::Telemetry(Telemetry {
            taken_ns: f.u64("taken_ns"),
            sync_error_ns: int(f, "sync_error_ns"),
            buffer_fill_us: f.u64("buffer_fill_us") as u32,
            underruns: f.u64("underruns") as u32,
            resyncs: f.u64("resyncs") as u32,
            correction_ppb: int(f, "correction_ppb") as i32,
            link: named("link", Link::from_name(f.str("link"))),
            rssi_dbm: int(f, "rssi_dbm") as i8,
            temperature_centi_c: int(f, "temperature_centi_c") as i16,
        }),
        Type::HandshakeInit => Message::HandshakeInit(HandshakeInit {
            protocol_version: f.u64("protocol_version") as u16,
            suite: named("suite", Suite::from_name(f.str("suite"))),
            noise: f.bytes("noise"),
        }),
        Type::HandshakeResponse => Message::HandshakeResponse(HandshakeResponse {
            noise: f.bytes("noise"),
        }),
        Type::HandshakeFinish => Message::HandshakeFinish(HandshakeFinish {
            noise: f.bytes("noise"),
        }),
        Type::SessionRefused => Message::SessionRefused(SessionRefused {
            reason: named("reason", RefusalReason::from_name(f.str("reason"))),
            detail: f.str("detail").to_string(),
        }),
        Type::SecureRecord => Message::SecureRecord(SecureRecord {
            ciphertext: f.bytes("ciphertext"),
        }),
        Type::Metadata => Message::Metadata(Metadata {
            playback: named("playback", Playback::from_name(f.str("playback"))),
            position_ms: f.u64("position_ms") as u32,
            duration_ms: f.u64("duration_ms") as u32,
            position_at_ns: f.u64("position_at_ns"),
            artwork_id: f.u64("artwork_id") as u32,
            title: f.str("title").to_string(),
            artist: f.str("artist").to_string(),
            album: f.str("album").to_string(),
            source: f.str("source").to_string(),
        }),
        Type::Artwork => Message::Artwork(Artwork {
            artwork_id: f.u64("artwork_id") as u32,
            total_len: f.u64("total_len") as u32,
            offset: f.u64("offset") as u32,
            mime: f.str("mime").to_string(),
            data: f.bytes("data"),
        }),
        Type::ControllerCommand => Message::ControllerCommand(ControllerCommand {
            command: named("command", Command::from_name(f.str("command"))),
            value: int(f, "value") as i16,
            target: f.str("target").to_string(),
        }),
        Type::ControllerState => Message::ControllerState(ControllerState {
            volume: f.u64("volume") as u8,
            muted: f.u64("muted") == 1,
            playback: named("playback", Playback::from_name(f.str("playback"))),
            group: f.str("group").to_string(),
        }),
        Type::VisualizerFrame => Message::VisualizerFrame(VisualizerFrame {
            timestamp_ns: f.u64("timestamp_ns"),
            beat: f.u64("beat") as u8,
            peak: f.u64("peak") as u8,
            bands: words(f, "bands")
                .iter()
                .map(|b| b.parse().unwrap())
                .collect(),
        }),
        Type::Color => Message::Color(Color {
            timestamp_ns: f.u64("timestamp_ns"),
            red: f.u64("red") as u8,
            green: f.u64("green") as u8,
            blue: f.u64("blue") as u8,
            brightness: f.u64("brightness") as u8,
            transition_ms: f.u64("transition_ms") as u16,
        }),
        Type::SourceOffer => Message::SourceOffer(SourceOffer {
            source_id: f.u64("source_id") as u8,
            kind: named("kind", SourceKind::from_name(f.str("kind"))),
            signal: f.u64("signal") == 1,
            name: f.str("name").to_string(),
        }),
        Type::SourceControl => Message::SourceControl(SourceControl {
            source_id: f.u64("source_id") as u8,
            action: named("action", SourceAction::from_name(f.str("action"))),
            codec: named("codec", Codec::from_name(f.str("codec"))),
        }),
        Type::TimeSync | Type::AudioChunk | Type::StreamEnd => {
            panic!("v1 messages are vectored at the top of fixtures/protocol/")
        }
    }
}
