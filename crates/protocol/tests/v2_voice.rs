//! The voice role (`docs/protocol.md`, "The voice role"): `mic_audio`,
//! `mic_state` and `voice_control` against their committed vectors, the role
//! bit that gates them, and what a peer that does not know them does.
//!
//! The messages here are written out as literals, not built from the
//! `.fields` files (`v2_vectors.rs` does that for every vector), so the
//! committed bytes are held to the layout twice.

mod common;

use chorus_protocol::v2::{
    decode_all, decode_frame, decode_frame_for_roles, encode, mic_format, roles, validate,
    DecodeError, EncodeError, Hello, Message, MicAudio, MicGate, MicState, Outcome, Problem,
    SourceKind, Type, VoiceControl, MIC_BYTES_PER_SAMPLE, MIC_MAX_SAMPLES, MIC_SAMPLE_RATE_HZ,
};
use chorus_protocol::{FrameOutcome, MessageType};

use common::load_vector;
use common::v2::load_v2;

fn mic_audio(samples: &[i16]) -> MicAudio {
    MicAudio {
        format: mic_format::PCM_S16LE_16K_MONO,
        sequence: 7,
        timestamp_ns: 3_000_000_000,
        data: samples.iter().flat_map(|s| s.to_le_bytes()).collect(),
    }
}

/// The voice messages, each with the stem of its committed vector.
fn voice_messages() -> Vec<(&'static str, Message)> {
    let control = |uplink, listening| Message::VoiceControl(VoiceControl { uplink, listening });
    vec![
        (
            "mic_audio",
            Message::MicAudio(mic_audio(&[0, 1, -1, 12345, i16::MIN, i16::MAX])),
        ),
        (
            "mic_state_muted",
            Message::MicState(MicState {
                gate: MicGate::Muted,
            }),
        ),
        (
            "mic_state_live",
            Message::MicState(MicState {
                gate: MicGate::Live,
            }),
        ),
        ("voice_control_off", control(false, false)),
        ("voice_control_uplink", control(true, false)),
        ("voice_control_listening", control(true, true)),
    ]
}

#[test]
fn each_voice_message_encodes_to_and_decodes_from_its_committed_vector() {
    for (stem, message) in voice_messages() {
        let v = load_v2(stem);
        assert_eq!(v.message, message, "{}.fields describes the message", stem);
        assert_eq!(
            encode(&message).unwrap(),
            v.frame,
            "{}: the encoder produces the committed bytes",
            stem
        );
        let d = decode_frame(&v.frame);
        assert_eq!(d.consumed, v.frame.len(), "{}: one whole frame", stem);
        assert_eq!(d.outcome, Outcome::Decoded(message), "{}", stem);
    }
    // The three types sit after `sound`, in the role block of the catalog.
    assert_eq!(Type::MicAudio.to_wire(), 0x3A);
    assert_eq!(Type::MicState.to_wire(), 0x3B);
    assert_eq!(Type::VoiceControl.to_wire(), 0x3C);
    for t in [Type::MicAudio, Type::MicState, Type::VoiceControl] {
        assert_eq!(t.role(), Some(roles::VOICE), "{}", t.name());
        assert!(!t.is_plaintext(), "{} travels in a record", t.name());
    }
}

#[test]
fn a_hello_declares_the_voice_role_as_bit_5() {
    let v = load_v2("hello_voice");
    let hello = Hello {
        protocol_version: 2,
        roles: roles::PLAYER | roles::CONTROLLER | roles::VISUALIZER | roles::VOICE,
        name: "Study".to_string(),
        software: "chorus-client 0.2.0".to_string(),
    };
    assert_eq!(roles::VOICE, 1 << 5);
    assert_eq!(hello.roles, 0x002D);
    assert_eq!(encode(&Message::Hello(hello.clone())).unwrap(), v.frame);
    assert_eq!(
        decode_frame(&v.frame).outcome,
        Outcome::Decoded(Message::Hello(hello))
    );
    // The bit after it is still no role.
    let mut frame = v.frame.clone();
    frame[6] |= 0x40;
    match decode_frame(&frame).outcome {
        Outcome::Rejected(DecodeError::InvalidField { error, .. }) => {
            assert_eq!(error.field, "roles");
            assert!(matches!(error.problem, Problem::Undefined(_)));
        }
        other => panic!("bit 6 of roles must be rejected, got {:?}", other),
    }
}

#[test]
fn mic_audio_from_a_peer_without_the_voice_role_is_refused() {
    let audio = load_v2("mic_audio");
    let state = load_v2("mic_state_live");
    let telemetry = load_v2("telemetry");
    // What hello_endpoint declares: a player that takes metadata, no voice.
    let without = match load_v2("hello_endpoint").message {
        Message::Hello(h) => h.roles,
        other => panic!("hello_endpoint is a hello, got {:?}", other),
    };
    assert_eq!(without & roles::VOICE, 0);
    let with = match load_v2("hello_voice").message {
        Message::Hello(h) => h.roles,
        other => panic!("hello_voice is a hello, got {:?}", other),
    };

    for v in [&audio, &state] {
        let mut stream = v.frame.clone();
        stream.extend_from_slice(&telemetry.frame);
        // Without the role: refused, by name, and nothing of it is handed on.
        let d = decode_frame_for_roles(&stream, without);
        assert_eq!(
            d.outcome,
            Outcome::Rejected(DecodeError::RoleNotDeclared {
                message_type: v.message.message_type(),
                role: roles::VOICE,
            }),
            "{}",
            v.stem
        );
        assert!(d.outcome.message().is_none());
        // It costs that one frame: the next one is found and decoded.
        assert_eq!(d.consumed, v.frame.len(), "{}", v.stem);
        let next = decode_frame_for_roles(&stream[d.consumed..], without);
        assert_eq!(next.outcome, Outcome::Decoded(telemetry.message.clone()));
        // With the role the same bytes are the message.
        let d = decode_frame_for_roles(&stream, with);
        assert_eq!(d.outcome, Outcome::Decoded(v.message.clone()), "{}", v.stem);
    }
    let refusal = DecodeError::RoleNotDeclared {
        message_type: Type::MicAudio,
        role: roles::VOICE,
    };
    assert_eq!(
        refusal.to_string(),
        "mic_audio refused: the session did not declare the voice role"
    );

    // A player and source that is not a voice endpoint cannot pass mic audio
    // off under the roles it has, and mic audio is not a kind of source.
    let d = decode_frame_for_roles(&audio.frame, roles::PLAYER | roles::SOURCE);
    assert!(matches!(
        d.outcome,
        Outcome::Rejected(DecodeError::RoleNotDeclared { .. })
    ));
    assert!(SourceKind::ALL
        .iter()
        .all(|k| !k.name().contains("mic") && !k.name().contains("voice")));
    assert_eq!(SourceKind::from_name("microphone"), None);

    // The endpoint's side of the same rule: voice_control on a session that
    // declared no voice role is stepped over.
    let control = load_v2("voice_control_listening");
    assert!(matches!(
        decode_frame_for_roles(&control.frame, without).outcome,
        Outcome::Rejected(DecodeError::RoleNotDeclared {
            message_type: Type::VoiceControl,
            ..
        })
    ));
    assert_eq!(
        decode_frame_for_roles(&control.frame, with).outcome,
        Outcome::Decoded(control.message.clone())
    );
    // A malformed frame is reported as that, whatever the roles.
    let half = [0x3A, 0x00, 0x03, 0x01, 0x00, 0x00];
    assert!(matches!(
        decode_frame_for_roles(&half, with).outcome,
        Outcome::Rejected(DecodeError::PayloadTooShortForType { .. })
    ));
    // Messages of no role pass on any session, as before.
    assert_eq!(
        decode_frame_for_roles(&telemetry.frame, 0).outcome,
        Outcome::Decoded(telemetry.message.clone())
    );
}

#[test]
fn a_peer_that_does_not_know_the_voice_messages_skips_them() {
    // The v1 decoder at the crate root is such a peer: its catalog ends at
    // 0x03. Each voice vector, then a frame it does know.
    let time_sync = load_vector(MessageType::TimeSync);
    for (stem, _) in voice_messages() {
        let v = load_v2(stem);
        let mut stream = v.frame.clone();
        stream.extend_from_slice(&time_sync.frame);
        let d = chorus_protocol::decode_frame(&stream);
        match d.outcome {
            FrameOutcome::SkippedUnknownType {
                message_type,
                payload_len,
            } => {
                assert_eq!(message_type, v.frame[0], "{}", stem);
                assert_eq!(payload_len, v.frame.len() - 3, "{}", stem);
            }
            other => panic!("{}: an old peer must skip it, got {:?}", stem, other),
        }
        assert_eq!(d.consumed, v.frame.len(), "{}: stepped over whole", stem);
        let next = chorus_protocol::decode_frame(&stream[d.consumed..]);
        assert_eq!(
            next.outcome,
            FrameOutcome::Decoded(time_sync.message.clone()),
            "{}: the frame after it still decodes",
            stem
        );
    }
    // And this decoder treats the type after the last one it knows the same
    // way, between two voice frames: skipped, not fatal.
    let mut stream = load_v2("mic_state_live").frame;
    stream.extend_from_slice(&[0x3D, 0x00, 0x02, 0xAA, 0xBB]);
    stream.extend_from_slice(&load_v2("voice_control_uplink").frame);
    let (outcomes, consumed) = decode_all(&stream);
    assert_eq!(consumed, stream.len());
    assert_eq!(outcomes.len(), 3);
    assert_eq!(
        outcomes[1],
        Outcome::SkippedUnknownType {
            message_type: 0x3D,
            payload_len: 2
        }
    );
    assert_eq!(
        outcomes[2],
        Outcome::Decoded(load_v2("voice_control_uplink").message)
    );
}

fn refused_as(message: Message, field: &str) -> Problem {
    match encode(&message) {
        Err(EncodeError::InvalidField(e)) => {
            assert_eq!(e.field, field, "{:?}", message);
            assert_eq!(validate(&message).unwrap_err().field, field);
            e.problem
        }
        other => panic!("{:?} must be refused, got {:?}", message, other),
    }
}

#[test]
fn the_voice_messages_are_held_to_their_rules_and_their_edges_are_accepted() {
    assert_eq!(MIC_SAMPLE_RATE_HZ, 16_000);
    assert_eq!(MIC_BYTES_PER_SAMPLE, 2);
    // 100 ms at 16 kHz.
    assert_eq!(MIC_MAX_SAMPLES as u32 * 10, MIC_SAMPLE_RATE_HZ);

    // The longest chunk is accepted and round-trips; one sample more is not.
    let longest = Message::MicAudio(mic_audio(&vec![-2; MIC_MAX_SAMPLES]));
    let frame = encode(&longest).unwrap();
    assert_eq!(frame.len(), 3 + 13 + MIC_MAX_SAMPLES * MIC_BYTES_PER_SAMPLE);
    assert_eq!(decode_frame(&frame).outcome, Outcome::Decoded(longest));
    let one_sample = Message::MicAudio(mic_audio(&[-2]));
    let frame = encode(&one_sample).unwrap();
    assert_eq!(frame.len(), 3 + Type::MicAudio.min_payload_len());
    assert_eq!(decode_frame(&frame).outcome, Outcome::Decoded(one_sample));

    let too_long = Message::MicAudio(mic_audio(&vec![0; MIC_MAX_SAMPLES + 1]));
    assert_eq!(
        refused_as(too_long, "data"),
        Problem::TooLong {
            len: (MIC_MAX_SAMPLES + 1) * MIC_BYTES_PER_SAMPLE,
            max: MIC_MAX_SAMPLES * MIC_BYTES_PER_SAMPLE,
        }
    );
    assert!(matches!(
        refused_as(Message::MicAudio(mic_audio(&[])), "data"),
        Problem::Inconsistent(_)
    ));
    let mut odd = mic_audio(&[1, 2]);
    odd.data.pop();
    assert!(matches!(
        refused_as(Message::MicAudio(odd), "data"),
        Problem::Inconsistent(_)
    ));
    for format in [0, mic_format::MAX + 1, 255] {
        let mut m = mic_audio(&[1]);
        m.format = format;
        assert_eq!(
            refused_as(Message::MicAudio(m), "format"),
            Problem::Undefined(format as u64)
        );
    }

    // On the wire: a gate or a bool no version defines is that field.
    let rejected_field = |frame: &[u8]| match decode_frame(frame) {
        d if d.consumed != frame.len() => panic!("{:02x?}: not consumed whole", frame),
        d => match d.outcome {
            Outcome::Rejected(DecodeError::InvalidField { error, .. }) => {
                assert!(matches!(error.problem, Problem::Undefined(_)));
                error.field
            }
            other => panic!("{:02x?} must be rejected, got {:?}", frame, other),
        },
    };
    assert_eq!(rejected_field(&[0x3B, 0x00, 0x01, 0x02]), "gate");
    assert_eq!(rejected_field(&[0x3C, 0x00, 0x02, 0x02, 0x00]), "uplink");
    assert_eq!(rejected_field(&[0x3C, 0x00, 0x02, 0x01, 0x02]), "listening");
    // Shorter than the type's fields: one frame, by its length.
    for short in [&[0x3B, 0x00, 0x00][..], &[0x3C, 0x00, 0x01, 0x01][..]] {
        let d = decode_frame(short);
        assert_eq!(d.consumed, short.len());
        assert!(matches!(
            d.outcome,
            Outcome::Rejected(DecodeError::PayloadTooShortForType { .. })
        ));
    }
    // A field a later version adds after the known ones is ignored.
    let d = decode_frame(&[0x3C, 0x00, 0x03, 0x01, 0x01, 0x7F]);
    assert_eq!(
        d.outcome,
        Outcome::Decoded(Message::VoiceControl(VoiceControl {
            uplink: true,
            listening: true
        }))
    );
    let d = decode_frame(&[0x3B, 0x00, 0x02, 0x01, 0x7F]);
    assert_eq!(
        d.outcome,
        Outcome::Decoded(Message::MicState(MicState {
            gate: MicGate::Live
        }))
    );
}
