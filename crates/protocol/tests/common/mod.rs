//! Reading the committed fixtures.
//!
//! Everything here only ever reads. No test in this crate writes a fixture,
//! and `golden_vectors::fixtures_are_not_rewritten_by_the_suite` proves it.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use chorus_protocol::{
    AudioChunk, Message, MessageType, SampleFormat, StreamEnd, TimeSync, RESERVED_LEN,
};

/// Directory holding the protocol golden vectors.
pub fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/protocol")
}

/// Read a fixture file as text, failing loudly if it is missing.
pub fn read_fixture(name: &str) -> String {
    let path = fixture_dir().join(name);
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("committed fixture {} is unreadable: {}", path.display(), e))
}

/// Parse the annotated hex of a `.hex` vector into bytes.
pub fn parse_hex(text: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    for (lineno, raw) in text.lines().enumerate() {
        let line = match raw.find('#') {
            Some(at) => &raw[..at],
            None => raw,
        };
        for token in line.split_whitespace() {
            assert!(
                token.len() % 2 == 0,
                "line {}: hex token {:?} has an odd number of digits",
                lineno + 1,
                token
            );
            let mut at = 0;
            while at < token.len() {
                let pair = &token[at..at + 2];
                let byte = u8::from_str_radix(pair, 16)
                    .unwrap_or_else(|_| panic!("line {}: {:?} is not hex", lineno + 1, pair));
                bytes.push(byte);
                at += 2;
            }
        }
    }
    bytes
}

/// The `key = value` pairs of a `.fields` file.
pub struct Fields {
    values: BTreeMap<String, String>,
    source: String,
}

impl Fields {
    /// Parse a `.fields` file.
    pub fn parse(source: &str, text: &str) -> Fields {
        let mut values = BTreeMap::new();
        for (lineno, raw) in text.lines().enumerate() {
            let line = match raw.find('#') {
                Some(at) => &raw[..at],
                None => raw,
            };
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let (key, value) = line.split_once('=').unwrap_or_else(|| {
                panic!("{} line {}: {:?} is not `key = value`", source, lineno + 1, line)
            });
            let previous = values.insert(key.trim().to_string(), value.trim().to_string());
            assert!(
                previous.is_none(),
                "{} line {}: key {:?} appears twice",
                source,
                lineno + 1,
                key.trim()
            );
        }
        Fields {
            values,
            source: source.to_string(),
        }
    }

    /// The raw value of a key.
    pub fn str(&self, key: &str) -> &str {
        self.values
            .get(key)
            .unwrap_or_else(|| panic!("{}: missing key {:?}", self.source, key))
            .as_str()
    }

    /// A key parsed as an unsigned integer.
    pub fn u64(&self, key: &str) -> u64 {
        let raw = self.str(key);
        raw.parse()
            .unwrap_or_else(|_| panic!("{}: {} = {:?} is not an integer", self.source, key, raw))
    }

    /// A key parsed as unseparated hex bytes.
    pub fn bytes(&self, key: &str) -> Vec<u8> {
        parse_hex(self.str(key))
    }
}

/// Build the message a `.fields` file describes.
pub fn message_from_fields(fields: &Fields) -> Message {
    let name = fields.str("message_type");
    let message_type = MessageType::from_name(name)
        .unwrap_or_else(|| panic!("{} is not a catalogued message type", name));
    match message_type {
        MessageType::TimeSync => Message::TimeSync(TimeSync {
            t0_ns: fields.u64("t0_ns"),
            t1_ns: fields.u64("t1_ns"),
            t2_ns: fields.u64("t2_ns"),
            t3_ns: fields.u64("t3_ns"),
        }),
        MessageType::AudioChunk => {
            let format_name = fields.str("sample_format");
            let sample_format = SampleFormat::from_name(format_name)
                .unwrap_or_else(|| panic!("{} is not a defined sample format", format_name));
            let reserved_bytes = fields.bytes("reserved");
            assert_eq!(
                reserved_bytes.len(),
                RESERVED_LEN,
                "the reserved block is {} bytes",
                RESERVED_LEN
            );
            let mut reserved = [0u8; RESERVED_LEN];
            reserved.copy_from_slice(&reserved_bytes);
            Message::AudioChunk(AudioChunk {
                sequence: fields.u64("sequence") as u32,
                timestamp_ns: fields.u64("timestamp_ns"),
                sample_rate_hz: fields.u64("sample_rate_hz") as u32,
                channels: fields.u64("channels") as u16,
                sample_format,
                reserved,
                audio_data: fields.bytes("audio_data"),
            })
        }
        MessageType::StreamEnd => Message::StreamEnd(StreamEnd {
            final_sequence: fields.u64("final_sequence") as u32,
            end_timestamp_ns: fields.u64("end_timestamp_ns"),
        }),
    }
}

/// One golden vector: the committed bytes and the committed canonical input.
pub struct Vector {
    /// Message type name, which is also the fixture file stem.
    pub name: &'static str,
    /// Bytes from `<name>.hex`.
    pub frame: Vec<u8>,
    /// Message parsed from `<name>.fields`.
    pub message: Message,
}

/// Load the golden vector for one catalogued message type.
pub fn load_vector(message_type: MessageType) -> Vector {
    let name = message_type.name();
    let frame = parse_hex(&read_fixture(&format!("{}.hex", name)));
    let fields_source = format!("{}.fields", name);
    let fields = Fields::parse(&fields_source, &read_fixture(&fields_source));
    let message = message_from_fields(&fields);
    assert_eq!(
        message.message_type(),
        message_type,
        "{} describes the wrong message type",
        fields_source
    );
    Vector {
        name,
        frame,
        message,
    }
}
