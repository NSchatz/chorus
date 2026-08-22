//! What the decoder does with frames it cannot use.
//!
//! Covers AC3 (unknown message type is skipped, session stays open), AC6 (a
//! frame shorter than its type's minimum is rejected alone and nothing reads
//! past the buffer), and the two binding advisories on this spec: a correctly
//! sized frame carrying an invalid field value, and a length field that
//! disagrees with the buffer it arrived in.
//!
//! The behaviour asserted here is specified in
//! `docs/decisions/0005-decoder-frame-validation.md`.

use chorus_protocol::{
    decode_frame, encode, AudioChunk, DecodeError, FrameOutcome, InvalidField, Message,
    MessageType, SampleFormat, Session, TimeSync, CHUNK_HEADER_LEN, RESERVED_LEN,
};

const UNKNOWN_TYPE: u8 = 0x7F;

fn time_sync() -> Message {
    Message::TimeSync(TimeSync {
        t0_ns: 1_000_000_000,
        t1_ns: 1_000_500_000,
        t2_ns: 1_001_000_000,
        t3_ns: 1_001_600_000,
    })
}

fn audio_chunk() -> Message {
    Message::AudioChunk(AudioChunk {
        sequence: 4096,
        timestamp_ns: 2_000_000_000,
        sample_rate_hz: 48_000,
        channels: 2,
        sample_format: SampleFormat::PcmS16Le,
        reserved: [0u8; RESERVED_LEN],
        audio_data: vec![0x23, 0x01, 0x67, 0x45, 0xAB, 0x89, 0xEF, 0xCD],
    })
}

fn encoded(message: &Message) -> Vec<u8> {
    encode(message).expect("a valid message encodes")
}

/// Assemble a frame whose length prefix agrees with its payload.
fn frame(type_byte: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = vec![type_byte];
    out.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

/// Assemble a frame whose length prefix is a lie.
fn frame_claiming(type_byte: u8, declared: u16, payload: &[u8]) -> Vec<u8> {
    let mut out = vec![type_byte];
    out.extend_from_slice(&declared.to_be_bytes());
    out.extend_from_slice(payload);
    out
}

/// An audio chunk payload assembled field by field, so a test can put a value
/// on the wire that the encoder would refuse to write.
fn audio_payload(channels: u8, sample_format: u8, sample_rate_hz: u32, pcm: &[u8]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(CHUNK_HEADER_LEN + pcm.len());
    payload.extend_from_slice(&7u32.to_be_bytes());
    payload.extend_from_slice(&123_456_789u64.to_be_bytes());
    payload.extend_from_slice(&sample_rate_hz.to_be_bytes());
    payload.push(channels);
    payload.push(sample_format);
    payload.extend_from_slice(&[0u8; RESERVED_LEN]);
    payload.extend_from_slice(pcm);
    assert_eq!(payload.len(), CHUNK_HEADER_LEN + pcm.len());
    payload
}

// --- AC3: an unrecognised message type ---------------------------------

#[test]
fn unknown_message_type_is_skipped_and_the_session_stays_open() {
    let mut buf = Vec::new();
    buf.extend_from_slice(&encoded(&time_sync()));
    buf.extend_from_slice(&frame(UNKNOWN_TYPE, &[0xDE, 0xAD, 0xBE, 0xEF, 0x00]));
    buf.extend_from_slice(&encoded(&audio_chunk()));

    let mut session = Session::new();
    let outcomes = session.decode_buffer(&buf);

    assert_eq!(outcomes.len(), 3, "three frames were presented");
    assert_eq!(outcomes[0].message(), Some(&time_sync()));
    assert_eq!(
        outcomes[1],
        FrameOutcome::SkippedUnknownType {
            message_type: UNKNOWN_TYPE,
            payload_len: 5,
        }
    );
    assert_eq!(
        outcomes[2].message(),
        Some(&audio_chunk()),
        "the frame after the unknown one still decodes"
    );
    assert!(session.is_open(), "an unknown type never closes a session");
    assert_eq!(session.frames_decoded(), 2);
    assert_eq!(session.frames_skipped(), 1);
    assert_eq!(session.frames_rejected(), 0);
}

#[test]
fn an_unknown_type_is_stepped_over_by_exactly_its_declared_length() {
    // A large unknown payload that happens to contain bytes which would look
    // like valid frames if the decoder resynchronised by scanning.
    let mut noise = Vec::new();
    noise.extend_from_slice(&encoded(&time_sync()));
    noise.extend_from_slice(&encoded(&time_sync()));

    let mut buf = frame(0xA5, &noise);
    buf.extend_from_slice(&encoded(&time_sync()));

    let mut session = Session::new();
    let outcomes = session.decode_buffer(&buf);

    assert_eq!(outcomes.len(), 2, "the noise inside the skipped frame is not parsed");
    assert!(outcomes[0].is_skipped());
    assert_eq!(outcomes[1].message(), Some(&time_sync()));
    assert!(session.is_open());
}

#[test]
fn an_unknown_type_with_an_empty_payload_is_skipped() {
    let buf = frame(0xFF, &[]);
    let mut session = Session::new();
    let outcomes = session.decode_buffer(&buf);
    assert_eq!(
        outcomes,
        vec![FrameOutcome::SkippedUnknownType {
            message_type: 0xFF,
            payload_len: 0,
        }]
    );
    assert!(session.is_open());
}

// --- AC6: a frame shorter than its declared type needs ------------------

#[test]
fn frame_shorter_than_its_type_minimum_is_rejected_alone() {
    // A time sync frame carrying 16 bytes where the type needs 32.
    let short = frame(MessageType::TIME_SYNC_BYTE, &[0u8; 16]);
    let mut buf = short.clone();
    buf.extend_from_slice(&encoded(&audio_chunk()));

    let mut session = Session::new();
    let outcomes = session.decode_buffer(&buf);

    assert_eq!(outcomes.len(), 2);
    assert_eq!(
        outcomes[0],
        FrameOutcome::Rejected(DecodeError::PayloadTooShortForType {
            message_type: MessageType::TIME_SYNC_BYTE,
            declared: 16,
            minimum: 32,
        })
    );
    assert_eq!(
        outcomes[1].message(),
        Some(&audio_chunk()),
        "only the short frame was rejected"
    );
    assert!(session.is_open());
    assert_eq!(session.frames_rejected(), 1);
    assert_eq!(session.frames_decoded(), 1);
}

#[test]
fn an_audio_chunk_with_a_header_and_no_samples_is_too_short_for_its_type() {
    // Exactly the 32 byte chunk header, no PCM. The minimum is 33, so this is
    // rejected on length rather than on a field value: a chunk with no samples
    // is unrepresentable, not merely invalid (decisions/0005).
    let payload = audio_payload(2, SampleFormat::PcmS16Le.to_wire(), 48_000, &[]);
    assert_eq!(payload.len(), CHUNK_HEADER_LEN);
    let mut buf = frame(MessageType::AUDIO_CHUNK_BYTE, &payload);
    buf.extend_from_slice(&encoded(&time_sync()));

    let mut session = Session::new();
    let outcomes = session.decode_buffer(&buf);

    assert_eq!(
        outcomes[0],
        FrameOutcome::Rejected(DecodeError::PayloadTooShortForType {
            message_type: MessageType::AUDIO_CHUNK_BYTE,
            declared: CHUNK_HEADER_LEN,
            minimum: CHUNK_HEADER_LEN + 1,
        })
    );
    assert_eq!(outcomes[1].message(), Some(&time_sync()));
    assert!(session.is_open());
}

// --- The length field against the buffer it arrived in ------------------

#[test]
fn declared_length_beyond_the_buffer_is_rejected_without_reading_past_it() {
    // The counterexample this check exists for: a valid type byte, a length
    // field claiming 1000 bytes of payload, and 4 bytes actually present. The
    // total is past no fixed minimum test that would catch it, and slicing the
    // claimed payload would be an out of bounds read.
    let buf = frame_claiming(MessageType::TIME_SYNC_BYTE, 1000, &[0xAA, 0xBB, 0xCC, 0xDD]);

    let mut session = Session::new();
    let outcomes = session.decode_buffer(&buf);

    assert_eq!(
        outcomes,
        vec![FrameOutcome::Rejected(
            DecodeError::DeclaredLengthExceedsBuffer {
                declared: 1000,
                available: 4,
            }
        )]
    );
    assert!(
        session.is_open(),
        "a lying length field rejects a frame, it does not end a session"
    );
    assert_eq!(session.frames_rejected(), 1);
}

#[test]
fn a_lying_length_field_costs_only_the_buffer_it_arrived_in() {
    let mut session = Session::new();

    let liar = frame_claiming(MessageType::AUDIO_CHUNK_BYTE, 65_535, &[0x00; 8]);
    let outcomes = session.decode_buffer(&liar);
    assert!(outcomes[0].is_rejected());
    assert!(session.is_open());

    // The next buffer decodes normally: the session was never damaged.
    let outcomes = session.decode_buffer(&encoded(&time_sync()));
    assert_eq!(outcomes[0].message(), Some(&time_sync()));
    assert_eq!(session.frames_decoded(), 1);
    assert_eq!(session.frames_rejected(), 1);
}

#[test]
fn a_truncated_frame_header_is_rejected() {
    for len in 1..3usize {
        let buf = vec![MessageType::TIME_SYNC_BYTE; len];
        let mut session = Session::new();
        let outcomes = session.decode_buffer(&buf);
        assert_eq!(
            outcomes,
            vec![FrameOutcome::Rejected(DecodeError::TruncatedHeader {
                available: len,
                needed: 3,
            })],
            "a {} byte buffer is not a frame header",
            len
        );
        assert!(session.is_open());
    }
}

#[test]
fn every_prefix_of_a_valid_stream_is_safe() {
    // The strongest form of "shall not read past the end of the buffer it was
    // given": truncate a good stream at every possible point and require that
    // nothing panics and the session survives all of them.
    let mut stream = Vec::new();
    stream.extend_from_slice(&encoded(&time_sync()));
    stream.extend_from_slice(&frame(UNKNOWN_TYPE, &[1, 2, 3]));
    stream.extend_from_slice(&encoded(&audio_chunk()));

    for cut in 0..=stream.len() {
        let mut session = Session::new();
        let outcomes = session.decode_buffer(&stream[..cut]);
        assert!(
            session.is_open(),
            "session closed on a {} byte prefix",
            cut
        );
        // Every complete frame in the prefix still decodes or skips normally.
        let rejected = outcomes.iter().filter(|o| o.is_rejected()).count();
        assert!(
            rejected <= 1,
            "a truncated stream rejects at most its last, incomplete frame (prefix {})",
            cut
        );
    }
}

#[test]
fn an_empty_buffer_produces_nothing_and_keeps_the_session_open() {
    let mut session = Session::new();
    let outcomes = session.decode_buffer(&[]);
    assert!(outcomes.is_empty());
    assert!(session.is_open());
}

// --- A correctly sized frame carrying a value the format cannot accept ---

#[test]
fn invalid_field_values_are_rejected_frame_by_frame() {
    let cases: Vec<(&str, Vec<u8>, InvalidField)> = vec![
        (
            "undefined sample format",
            audio_payload(2, 0x09, 48_000, &[0, 0, 0, 0]),
            InvalidField::UndefinedSampleFormat(0x09),
        ),
        (
            "reserved sample format zero",
            audio_payload(2, 0x00, 48_000, &[0, 0, 0, 0]),
            InvalidField::UndefinedSampleFormat(0x00),
        ),
        (
            "zero channels",
            audio_payload(0, SampleFormat::PcmS16Le.to_wire(), 48_000, &[0, 0, 0, 0]),
            InvalidField::ChannelsOutOfRange(0),
        ),
        (
            "too many channels",
            audio_payload(9, SampleFormat::PcmS16Le.to_wire(), 48_000, &[0, 0, 0, 0]),
            InvalidField::ChannelsOutOfRange(9),
        ),
        (
            "implausible sample rate",
            audio_payload(2, SampleFormat::PcmS16Le.to_wire(), 100, &[0, 0, 0, 0]),
            InvalidField::SampleRateOutOfRange(100),
        ),
        (
            "pcm that is not a whole number of frames",
            audio_payload(2, SampleFormat::PcmS16Le.to_wire(), 48_000, &[0, 0, 0, 0, 0]),
            InvalidField::AudioDataNotFrameAligned {
                data_len: 5,
                frame_len: 4,
            },
        ),
    ];

    for (name, payload, expected) in cases {
        let mut buf = frame(MessageType::AUDIO_CHUNK_BYTE, &payload);
        buf.extend_from_slice(&encoded(&time_sync()));

        let mut session = Session::new();
        let outcomes = session.decode_buffer(&buf);

        assert_eq!(
            outcomes[0],
            FrameOutcome::Rejected(DecodeError::InvalidFieldValue {
                message_type: MessageType::AUDIO_CHUNK_BYTE,
                field: expected,
            }),
            "case: {}",
            name
        );
        assert_eq!(
            outcomes[1].message(),
            Some(&time_sync()),
            "case {}: the following frame still decodes",
            name
        );
        assert!(session.is_open(), "case {}: the session stays open", name);
        assert_eq!(session.frames_rejected(), 1, "case: {}", name);
        assert_eq!(session.frames_decoded(), 1, "case: {}", name);
    }
}

#[test]
fn an_invalid_field_value_is_not_treated_as_an_unknown_type() {
    // Both keep the session open, and they are still different things: one is
    // a newer peer, the other is a peer that disagrees with the format. The
    // counters have to tell them apart.
    let payload = audio_payload(2, 0x09, 48_000, &[0, 0, 0, 0]);
    let buf = frame(MessageType::AUDIO_CHUNK_BYTE, &payload);

    let mut session = Session::new();
    let outcomes = session.decode_buffer(&buf);

    assert!(outcomes[0].is_rejected());
    assert!(!outcomes[0].is_skipped());
    assert_eq!(session.frames_skipped(), 0);
    assert_eq!(session.frames_rejected(), 1);
}

// --- Forward compatibility ---------------------------------------------

#[test]
fn reserved_bytes_are_preserved_and_never_rejected() {
    // A future sender assigns meaning to the reserved block. A decoder built
    // today must not reject that frame, and must hand the bytes through.
    let mut payload = audio_payload(2, SampleFormat::PcmS16Le.to_wire(), 48_000, &[1, 2, 3, 4]);
    let marker: [u8; RESERVED_LEN] = [
        0xF0, 0x0D, 0xBE, 0xEF, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A,
    ];
    payload[18..32].copy_from_slice(&marker);

    let buf = frame(MessageType::AUDIO_CHUNK_BYTE, &payload);
    let result = decode_frame(&buf);

    match result.outcome {
        FrameOutcome::Decoded(Message::AudioChunk(chunk)) => {
            assert_eq!(chunk.reserved, marker, "reserved bytes pass through intact");
        }
        other => panic!("a set reserved block must still decode, got {:?}", other),
    }
}

#[test]
fn trailing_bytes_after_a_known_payload_are_ignored() {
    // A future version adds a field to time sync. A decoder built today reads
    // the fields it knows and ignores the rest rather than rejecting.
    let mut payload = vec![0u8; 32];
    payload[0..8].copy_from_slice(&1_000_000_000u64.to_be_bytes());
    payload[8..16].copy_from_slice(&1_000_500_000u64.to_be_bytes());
    payload[16..24].copy_from_slice(&1_001_000_000u64.to_be_bytes());
    payload[24..32].copy_from_slice(&1_001_600_000u64.to_be_bytes());
    payload.extend_from_slice(&[0xAB; 8]);

    let mut buf = frame(MessageType::TIME_SYNC_BYTE, &payload);
    buf.extend_from_slice(&encoded(&audio_chunk()));

    let mut session = Session::new();
    let outcomes = session.decode_buffer(&buf);

    assert_eq!(outcomes.len(), 2);
    assert_eq!(
        outcomes[0].message(),
        Some(&time_sync()),
        "the fields this version knows are recovered"
    );
    assert_eq!(
        outcomes[1].message(),
        Some(&audio_chunk()),
        "the next frame is found using the declared length, not the known one"
    );
}

// --- A mixed buffer -----------------------------------------------------

#[test]
fn a_session_survives_a_buffer_holding_every_outcome() {
    let mut buf = Vec::new();
    buf.extend_from_slice(&encoded(&time_sync()));
    buf.extend_from_slice(&frame(UNKNOWN_TYPE, &[9, 9, 9]));
    buf.extend_from_slice(&frame(MessageType::TIME_SYNC_BYTE, &[0u8; 8]));
    buf.extend_from_slice(&frame(
        MessageType::AUDIO_CHUNK_BYTE,
        &audio_payload(2, 0x09, 48_000, &[0, 0, 0, 0]),
    ));
    buf.extend_from_slice(&encoded(&audio_chunk()));

    let mut session = Session::new();
    let outcomes = session.decode_buffer(&buf);

    assert_eq!(outcomes.len(), 5);
    assert!(outcomes[0].is_decoded());
    assert!(outcomes[1].is_skipped());
    assert!(outcomes[2].is_rejected());
    assert!(outcomes[3].is_rejected());
    assert!(outcomes[4].is_decoded());
    assert!(session.is_open());
    assert_eq!(session.frames_decoded(), 2);
    assert_eq!(session.frames_skipped(), 1);
    assert_eq!(session.frames_rejected(), 2);
}

#[test]
fn a_session_is_only_ever_closed_by_its_owner() {
    let mut session = Session::new();
    session.decode_buffer(&frame_claiming(MessageType::TIME_SYNC_BYTE, 4096, &[0; 2]));
    assert!(session.is_open());
    session.close();
    assert!(!session.is_open());
}
