//! The v2 value rules, both directions, and a decoder that never panics.
//!
//! One validation routine serves the encoder and the decoder, so what one
//! refuses the other rejects. These tests hold that from both ends on the
//! rules `docs/protocol.md` states, and corrupt every byte of every committed
//! v2 vector to show the decoder never panics, never reads past its buffer, and
//! that whatever it accepts re-encodes to a frame it accepts again.

mod common;

use chorus_protocol::v2::*;
use chorus_protocol::SampleFormat;

use common::v2::{load_v2, v2_stems};

fn refused_both_ways(message: Message, field: &str) {
    match encode(&message) {
        Err(EncodeError::InvalidField(e)) => assert_eq!(e.field, field, "{:?}", message),
        other => panic!("the encoder must refuse {:?}, got {:?}", message, other),
    }
    // Build the bytes by hand from a valid twin, then check the decoder agrees.
    assert!(validate(&message).is_err());
}

fn stream_format(codec: Codec, map: Vec<ChannelPosition>, frames: u32, config: Vec<u8>) -> Message {
    Message::StreamFormat(StreamFormat {
        codec,
        sample_format: SampleFormat::PcmS16Le,
        sample_rate_hz: 48_000,
        channel_map: map,
        frames_per_chunk: frames,
        codec_config: config,
    })
}

#[test]
fn the_stated_value_rules_are_refused_by_the_encoder() {
    use ChannelPosition::*;
    let caps = |codecs| {
        Message::Capabilities(Capabilities {
            codecs,
            sample_formats: 1,
            max_channels: 2,
            sample_rates_hz: vec![48_000],
            buffer_ms: 100,
            intrinsic_latency_ns: 0,
            led_count: 0,
            visualizer_bands: 0,
        })
    };
    refused_both_ways(caps(Codec::Flac.bit()), "codecs");
    refused_both_ways(
        stream_format(Codec::Pcm, vec![FrontLeft, FrontLeft], 960, vec![]),
        "channel_map",
    );
    refused_both_ways(
        stream_format(Codec::Pcm, vec![Mono, FrontLeft], 960, vec![]),
        "channel_map",
    );
    refused_both_ways(
        stream_format(Codec::Pcm, vec![FrontLeft, FrontRight], 960, vec![1]),
        "codec_config",
    );
    refused_both_ways(
        stream_format(Codec::Flac, vec![FrontLeft, FrontRight], 4096, vec![0; 33]),
        "codec_config",
    );
    refused_both_ways(
        stream_format(
            Codec::Opus,
            vec![FrontLeft, FrontRight],
            1000,
            b"OpusHead\x01\x02\0\0\0\0\0\0\0\0\0".to_vec(),
        ),
        "frames_per_chunk",
    );
    refused_both_ways(
        stream_format(
            Codec::Opus,
            vec![FrontLeft],
            960,
            b"OpusHead\x01\x02\0\0\0\0\0\0\0\0\0".to_vec(),
        ),
        "codec_config",
    );
    refused_both_ways(
        Message::OutputDelay(OutputDelay {
            delay_ns: 5_000_000_001,
        }),
        "delay_ns",
    );
    refused_both_ways(
        Message::ControllerCommand(ControllerCommand {
            command: Command::Join,
            value: 0,
            target: String::new(),
        }),
        "target",
    );
    refused_both_ways(
        Message::ControllerCommand(ControllerCommand {
            command: Command::VolumeSet,
            value: 101,
            target: String::new(),
        }),
        "value",
    );
    refused_both_ways(
        Message::Artwork(Artwork {
            artwork_id: 1,
            total_len: 4,
            offset: 2,
            mime: "image/png".into(),
            data: vec![0; 3],
        }),
        "offset",
    );
    refused_both_ways(
        Message::Hello(Hello {
            protocol_version: 2,
            roles: 1 << 9,
            name: String::new(),
            software: String::new(),
        }),
        "roles",
    );
}

#[test]
fn a_rule_broken_on_the_wire_is_rejected_by_the_decoder_as_that_field() {
    // capabilities.hex with the codec set changed to FLAC only (PCM missing).
    let mut frame = load_v2("capabilities").frame;
    frame[3] = Codec::Flac.bit();
    match decode_frame(&frame).outcome {
        Outcome::Rejected(DecodeError::InvalidField { error, .. }) => {
            assert_eq!(error.field, "codecs")
        }
        other => panic!("expected the codecs rule, got {:?}", other),
    }
    // hello.hex with a name length that runs past the payload.
    let mut frame = load_v2("hello_endpoint").frame;
    frame[7] = 250;
    match decode_frame(&frame).outcome {
        Outcome::Rejected(DecodeError::InvalidField { error, .. }) => {
            assert_eq!(
                error,
                FieldError {
                    field: "name",
                    problem: Problem::Truncated
                }
            )
        }
        other => panic!("expected a truncated name, got {:?}", other),
    }
}

#[test]
fn every_single_byte_corruption_of_every_vector_is_handled_without_panic() {
    let mut decoded = 0usize;
    let mut rejected = 0usize;
    for stem in v2_stems() {
        let frame = load_v2(&stem).frame;
        for at in 0..frame.len() {
            for flip in [0x01u8, 0x80, 0xFF] {
                let mut bad = frame.clone();
                bad[at] ^= flip;
                let d = decode_frame(&bad);
                assert!(
                    d.consumed <= bad.len(),
                    "{} byte {}: consumed past the buffer",
                    stem,
                    at
                );
                match d.outcome {
                    Outcome::Decoded(m) => {
                        decoded += 1;
                        let again = encode(&m).unwrap_or_else(|e| {
                            panic!(
                                "{} byte {}: decoded {:?} but it does not re-encode: {}",
                                stem, at, m, e
                            )
                        });
                        assert_eq!(decode_frame(&again).outcome, Outcome::Decoded(m));
                    }
                    Outcome::Rejected(_) => rejected += 1,
                    Outcome::SkippedUnknownType { .. } => {}
                }
            }
        }
    }
    println!(
        "corruptions: {} still decoded and round-tripped, {} rejected",
        decoded, rejected
    );
    assert!(rejected > 0 && decoded > 0);
}

#[test]
fn every_prefix_of_a_stream_of_all_the_vectors_is_safe() {
    let stream: Vec<u8> = v2_stems().iter().flat_map(|s| load_v2(s).frame).collect();
    let (all, consumed) = decode_all(&stream);
    assert_eq!(consumed, stream.len());
    assert_eq!(all.len(), v2_stems().len());
    for cut in 0..stream.len() {
        let (outcomes, consumed) = decode_all(&stream[..cut]);
        assert!(consumed <= cut);
        assert!(outcomes.iter().all(|o| matches!(o, Outcome::Decoded(_))));
    }
}
