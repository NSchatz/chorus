//! What the encoder does with a value it cannot put on the wire.
//!
//! Covers AC7: a field value outside the range the wire format can represent
//! is rejected, and nothing truncated or wrapped around is emitted.

use chorus_protocol::{
    decode_frame, encode, AudioChunk, EncodeError, FrameOutcome, InvalidField, Message,
    SampleFormat, MAX_PAYLOAD_LEN, RESERVED_LEN,
};

fn chunk_with(channels: u16, sample_rate_hz: u32, audio_data: Vec<u8>) -> Message {
    Message::AudioChunk(AudioChunk {
        sequence: 1,
        timestamp_ns: 2_000_000_000,
        sample_rate_hz,
        channels,
        sample_format: SampleFormat::PcmS16Le,
        reserved: [0u8; RESERVED_LEN],
        audio_data,
    })
}

#[test]
fn a_channel_count_the_wire_field_cannot_carry_is_refused() {
    // 512 channels does not fit the u8 the wire carries. Truncating it would
    // put 0 on the wire, which is exactly the wrapped-around encoding this
    // criterion forbids.
    let message = chunk_with(512, 48_000, vec![0u8; 2048]);
    let result = encode(&message);
    assert_eq!(
        result,
        Err(EncodeError::NotRepresentable {
            field: "channels",
            value: 512,
            wire_max: 255,
        })
    );
    assert!(result.is_err(), "nothing at all is emitted");
}

#[test]
fn a_payload_longer_than_the_length_field_can_describe_is_refused() {
    // The u16 length field tops out at 65535. A payload of 70032 bytes would
    // wrap to 4496 and produce a frame that decodes as garbage.
    let audio_data = vec![0u8; 70_000];
    let needed = 32 + audio_data.len();
    assert!(needed > MAX_PAYLOAD_LEN);

    let message = chunk_with(2, 48_000, audio_data);
    assert_eq!(
        encode(&message),
        Err(EncodeError::PayloadTooLong {
            needed,
            max: MAX_PAYLOAD_LEN,
        })
    );
}

#[test]
fn a_payload_at_the_maximum_is_still_encodable() {
    // The boundary the previous test steps over: the largest frame this
    // framing allows, 65535 bytes end to end.
    let audio_data = vec![0u8; MAX_PAYLOAD_LEN - 32 - 3];
    assert_eq!(audio_data.len() % 4, 0, "still frame aligned");
    let message = chunk_with(2, 48_000, audio_data);
    let frame = encode(&message).expect("a payload inside the limit encodes");
    assert_eq!(frame.len(), MAX_PAYLOAD_LEN);

    let result = decode_frame(&frame);
    assert!(result.outcome.is_decoded(), "and it decodes back");
}

#[test]
fn field_values_the_decoder_would_reject_are_refused_at_encode_time() {
    // An encoder must not be able to produce bytes its own decoder refuses.
    let cases: Vec<(&str, Message, EncodeError)> = vec![
        (
            "zero channels",
            chunk_with(0, 48_000, vec![0u8; 4]),
            EncodeError::FieldOutOfRange {
                field: "channels",
                value: 0,
            },
        ),
        (
            "more channels than any endpoint carries",
            chunk_with(9, 48_000, vec![0u8; 36]),
            EncodeError::FieldOutOfRange {
                field: "channels",
                value: 9,
            },
        ),
        (
            "implausible sample rate",
            chunk_with(2, 100, vec![0u8; 4]),
            EncodeError::FieldOutOfRange {
                field: "sample_rate_hz",
                value: 100,
            },
        ),
        (
            "sample rate above the accepted band",
            chunk_with(2, 1_000_000, vec![0u8; 4]),
            EncodeError::FieldOutOfRange {
                field: "sample_rate_hz",
                value: 1_000_000,
            },
        ),
        (
            "no samples at all",
            chunk_with(2, 48_000, Vec::new()),
            EncodeError::InvalidFieldValue(InvalidField::EmptyAudioData),
        ),
        (
            "pcm that is not a whole number of frames",
            chunk_with(2, 48_000, vec![0u8; 5]),
            EncodeError::InvalidFieldValue(InvalidField::AudioDataNotFrameAligned {
                data_len: 5,
                frame_len: 4,
            }),
        ),
    ];

    for (name, message, expected) in cases {
        assert_eq!(encode(&message), Err(expected), "case: {}", name);
    }
}

#[test]
fn whatever_the_encoder_emits_the_decoder_accepts() {
    let messages = vec![
        chunk_with(1, 8_000, vec![0u8; 2]),
        chunk_with(2, 48_000, vec![0xA5; 4096]),
        chunk_with(6, 96_000, vec![0x5A; 96]),
        chunk_with(8, 384_000, vec![0x11; 160]),
    ];
    for message in messages {
        let frame = encode(&message).expect("a valid chunk encodes");
        let result = decode_frame(&frame);
        assert_eq!(result.consumed, frame.len());
        match result.outcome {
            FrameOutcome::Decoded(decoded) => assert_eq!(decoded, message),
            other => panic!("encoder output must decode, got {:?}", other),
        }
    }
}
