//! The packets, byte for byte.
//!
//! Every expected byte below was worked out from OASIS "MQTT Version 3.1.1"
//! (read 2026-10-03) by hand, section by section, and none was produced by
//! running this crate: the encoder is held to the standard, not to itself.

use chorus_mqtt::codec::{
    connack_meaning, decode_inbound, decode_remaining_length, encode_remaining_length, Connect,
    Decoded, EncodeError, Inbound, Publish, Qos, RemainingLength, Will, DISCONNECT,
    MAX_REMAINING_LENGTH, PINGREQ,
};

fn hex(text: &str) -> Vec<u8> {
    text.lines()
        .map(|l| l.split("  ").next().unwrap_or(""))
        .flat_map(str::split_whitespace)
        .map(|b| u8::from_str_radix(b, 16).expect("a hex byte"))
        .collect()
}

/// SPEC 3.1: client id `chorus`, will topic `chorus/v1/server/status`, will
/// message `offline`, user `chorus`, password `pw`, keep alive 60, will QoS 1
/// retained, clean session. Remaining length 10 + 8 + 25 + 9 + 8 + 4 = 64.
const GOLDEN_CONNECT: &str = "\
10 40                         fixed header, remaining length 64
00 04 4D 51 54 54             MQTT
04                            level 4
EE                            flags: user, password, will retain, will QoS 1, will, clean
00 3C                         keep alive 60
00 06 63 68 6F 72 75 73       client id
00 17 63 68 6F 72 75 73 2F 76 31 2F 73 65 72 76 65 72 2F 73 74 61 74 75 73
00 07 6F 66 66 6C 69 6E 65    will message
00 06 63 68 6F 72 75 73       user
00 02 70 77                   password
";

fn golden_connect() -> Connect<'static> {
    Connect {
        client_id: "chorus",
        keep_alive_s: 60,
        will: Some(Will {
            topic: "chorus/v1/server/status",
            message: b"offline",
            qos: Qos::AtLeastOnce,
            retain: true,
        }),
        user: Some("chorus"),
        password: Some(b"pw"),
    }
}

#[test]
fn the_connect_is_the_standards_bytes() {
    assert_eq!(golden_connect().encode().unwrap(), hex(GOLDEN_CONNECT));
}

#[test]
fn the_connect_flags_follow_what_is_present() {
    let flags = |c: Connect| c.encode().unwrap()[9];
    let full = golden_connect();
    assert_eq!(flags(full), 0xEE);
    // An anonymous broker: no user, no password; the will stays.
    let anonymous = Connect {
        user: None,
        password: None,
        ..full
    };
    assert_eq!(flags(anonymous), 0x2E);
    let bytes = anonymous.encode().unwrap();
    // 10 + 8 + 25 + 9 = 52.
    assert_eq!(&bytes[..2], &[0x10, 52]);
    assert_eq!(bytes.len(), 54);
    // A user with no password.
    assert_eq!(
        flags(Connect {
            password: None,
            ..full
        }),
        0xAE
    );
    // A will at QoS 0, not retained; and no will at all: clean session only.
    let plain_will = Will {
        qos: Qos::AtMostOnce,
        retain: false,
        ..full.will.unwrap()
    };
    assert_eq!(
        flags(Connect {
            will: Some(plain_will),
            ..anonymous
        }),
        0x06
    );
    let bare = Connect {
        will: None,
        ..anonymous
    };
    assert_eq!(
        bare.encode().unwrap(),
        hex("10 12 00 04 4D 51 54 54 04 02 00 3C 00 06 63 68 6F 72 75 73")
    );
}

#[test]
fn a_connect_the_standard_forbids_is_not_encoded() {
    let full = golden_connect();
    assert_eq!(
        Connect { user: None, ..full }.encode().unwrap_err(),
        EncodeError::PasswordWithoutUser
    );
    let will = full.will.unwrap();
    for topic in ["chorus/+/status", "chorus/#"] {
        assert_eq!(
            Connect {
                will: Some(Will { topic, ..will }),
                ..full
            }
            .encode()
            .unwrap_err(),
            EncodeError::WildcardInTopic
        );
    }
    assert_eq!(
        Connect {
            will: Some(Will { topic: "", ..will }),
            ..full
        }
        .encode()
        .unwrap_err(),
        EncodeError::Empty {
            field: "will topic"
        }
    );
    assert_eq!(
        Connect {
            user: Some("cho\0rus"),
            ..full
        }
        .encode()
        .unwrap_err(),
        EncodeError::ControlCharacter { field: "user name" }
    );
    let long = "p".repeat(65_536);
    assert_eq!(
        Connect {
            password: Some(long.as_bytes()),
            ..full
        }
        .encode()
        .unwrap_err(),
        EncodeError::FieldTooLong {
            field: "password",
            len: 65_536
        }
    );
}

#[test]
fn a_publish_is_the_standards_bytes_at_every_flag_this_client_sends() {
    let publish = |qos, retain, packet_id| Publish {
        topic: "a/b",
        payload: b"hi",
        qos,
        retain,
        packet_id,
    };
    // SPEC 3.3.1: 0x30 | qos << 1 | retain. Topic `a/b` is 00 03 61 2F 62.
    assert_eq!(
        publish(Qos::AtMostOnce, false, 0).encode().unwrap(),
        hex("30 07 00 03 61 2F 62 68 69")
    );
    assert_eq!(
        publish(Qos::AtMostOnce, true, 0).encode().unwrap(),
        hex("31 07 00 03 61 2F 62 68 69")
    );
    // QoS 1 carries the packet identifier after the topic (SPEC 3.3.2.2).
    assert_eq!(
        publish(Qos::AtLeastOnce, false, 0x1234).encode().unwrap(),
        hex("32 09 00 03 61 2F 62 12 34 68 69")
    );
    assert_eq!(
        publish(Qos::AtLeastOnce, true, 1).encode().unwrap(),
        hex("33 09 00 03 61 2F 62 00 01 68 69")
    );
    // The packet identifier of a QoS 0 PUBLISH is not sent, whatever it holds.
    assert_eq!(
        publish(Qos::AtMostOnce, false, 77).encode().unwrap(),
        publish(Qos::AtMostOnce, false, 0).encode().unwrap()
    );
}

#[test]
fn the_retained_status_and_the_retained_clear_are_the_standards_bytes() {
    // `chorus/v1/server/status` is 23 bytes: 2 + 23 + 2 + 6 = 33 = 0x21.
    let online = Publish {
        topic: "chorus/v1/server/status",
        payload: b"online",
        qos: Qos::AtLeastOnce,
        retain: true,
        packet_id: 1,
    };
    assert_eq!(
        online.encode().unwrap(),
        hex(
            "33 21 00 17 63 68 6F 72 75 73 2F 76 31 2F 73 65 72 76 65 72 2F 73 74 61 74 75 73 \
             00 01 6F 6E 6C 69 6E 65"
        )
    );
    // A retained PUBLISH with no payload clears the topic [MQTT-3.3.1-10].
    let clear = Publish {
        topic: "a/b",
        payload: b"",
        qos: Qos::AtLeastOnce,
        retain: true,
        packet_id: 2,
    };
    assert_eq!(clear.encode().unwrap(), hex("33 07 00 03 61 2F 62 00 02"));
}

#[test]
fn a_publish_the_standard_forbids_is_not_encoded() {
    let ok = Publish {
        topic: "a/b",
        payload: b"",
        qos: Qos::AtLeastOnce,
        retain: false,
        packet_id: 1,
    };
    assert_eq!(
        Publish { packet_id: 0, ..ok }.encode().unwrap_err(),
        EncodeError::PacketIdZero
    );
    for topic in ["a/+", "#", "a/#/b"] {
        assert_eq!(
            Publish { topic, ..ok }.encode().unwrap_err(),
            EncodeError::WildcardInTopic
        );
    }
    assert_eq!(
        Publish { topic: "", ..ok }.encode().unwrap_err(),
        EncodeError::Empty { field: "topic" }
    );
    assert_eq!(
        Publish {
            topic: "a/\0",
            ..ok
        }
        .encode()
        .unwrap_err(),
        EncodeError::ControlCharacter { field: "topic" }
    );
}

#[test]
fn a_long_publish_has_a_two_byte_remaining_length() {
    // 2 + 3 + 2 + 314 = 321, the standard's own worked example: C1 02.
    let payload = vec![b'x'; 314];
    let bytes = Publish {
        topic: "a/b",
        payload: &payload,
        qos: Qos::AtLeastOnce,
        retain: true,
        packet_id: 9,
    }
    .encode()
    .unwrap();
    assert_eq!(&bytes[..3], &[0x33, 0xC1, 0x02]);
    assert_eq!(bytes.len(), 3 + 321);
}

#[test]
fn the_constant_packets_are_the_standards() {
    assert_eq!(PINGREQ, [0xC0, 0x00]);
    assert_eq!(DISCONNECT, [0xE0, 0x00]);
}

#[test]
fn the_remaining_length_is_the_standards_at_every_boundary() {
    // SPEC 2.2.3, Table 2.4, and the two worked examples (64 and 321).
    let cases: &[(u32, &[u8])] = &[
        (0, &[0x00]),
        (64, &[0x40]),
        (127, &[0x7F]),
        (128, &[0x80, 0x01]),
        (321, &[0xC1, 0x02]),
        (16_383, &[0xFF, 0x7F]),
        (16_384, &[0x80, 0x80, 0x01]),
        (2_097_151, &[0xFF, 0xFF, 0x7F]),
        (2_097_152, &[0x80, 0x80, 0x80, 0x01]),
        (268_435_455, &[0xFF, 0xFF, 0xFF, 0x7F]),
    ];
    for (value, bytes) in cases {
        let mut out = Vec::new();
        encode_remaining_length(*value, &mut out).unwrap();
        assert_eq!(&out, bytes, "{}", value);
        assert_eq!(
            decode_remaining_length(bytes),
            RemainingLength::Value {
                value: *value,
                used: bytes.len()
            },
            "{}",
            value
        );
        // Bytes after the field are not part of it.
        let mut longer = bytes.to_vec();
        longer.push(0xFF);
        assert_eq!(
            decode_remaining_length(&longer),
            RemainingLength::Value {
                value: *value,
                used: bytes.len()
            }
        );
    }
    assert_eq!(MAX_REMAINING_LENGTH, 268_435_455);
    assert!(encode_remaining_length(MAX_REMAINING_LENGTH + 1, &mut Vec::new()).is_err());
    assert_eq!(
        decode_remaining_length(&[0x80, 0x80, 0x80, 0x80, 0x01]),
        RemainingLength::Malformed
    );
    for short in [&[][..], &[0x80], &[0xFF, 0xFF], &[0x80, 0x80, 0x80]] {
        assert_eq!(decode_remaining_length(short), RemainingLength::Incomplete);
    }
}

#[test]
fn the_three_inbound_packets_decode() {
    assert_eq!(
        decode_inbound(&[0x20, 0x02, 0x00, 0x00]),
        Decoded::Packet {
            packet: Inbound::ConnAck {
                session_present: false,
                code: 0
            },
            used: 4
        }
    );
    for code in 1..=5u8 {
        assert_eq!(
            decode_inbound(&[0x20, 0x02, 0x00, code]),
            Decoded::Packet {
                packet: Inbound::ConnAck {
                    session_present: false,
                    code
                },
                used: 4
            }
        );
    }
    assert_eq!(
        decode_inbound(&[0x20, 0x02, 0x01, 0x00]),
        Decoded::Packet {
            packet: Inbound::ConnAck {
                session_present: true,
                code: 0
            },
            used: 4
        }
    );
    // Two packets in one read: the first is taken, the rest is left.
    assert_eq!(
        decode_inbound(&[0x40, 0x02, 0x12, 0x34, 0xD0, 0x00]),
        Decoded::Packet {
            packet: Inbound::PubAck { packet_id: 0x1234 },
            used: 4
        }
    );
    assert_eq!(
        decode_inbound(&[0xD0, 0x00]),
        Decoded::Packet {
            packet: Inbound::PingResp,
            used: 2
        }
    );
}

#[test]
fn a_packet_cut_short_asks_for_more_at_every_length() {
    for whole in [
        &[0x20u8, 0x02, 0x00, 0x00][..],
        &[0x40, 0x02, 0x00, 0x07],
        &[0xD0, 0x00],
    ] {
        for cut in 0..whole.len() {
            assert_eq!(
                decode_inbound(&whole[..cut]),
                Decoded::Incomplete,
                "{:02X?} cut at {}",
                whole,
                cut
            );
        }
    }
}

#[test]
fn anything_else_from_the_broker_is_a_violation() {
    let violation = |bytes: &[u8]| matches!(decode_inbound(bytes), Decoded::Violation(_));
    // An inbound PUBLISH (this client never subscribed), a SUBACK, a PUBREC,
    // an UNSUBACK, the reserved types, and a client-only packet echoed back.
    for first in [
        0x30u8, 0x31, 0x32, 0x90, 0x50, 0xB0, 0x00, 0xF0, 0x10, 0xC0, 0xE0,
    ] {
        assert!(violation(&[first, 0x00]), "{:02X}", first);
    }
    // Reserved flag bits set on a packet that has none [MQTT-2.2.2-2].
    assert!(violation(&[0x21, 0x02, 0x00, 0x00]));
    assert!(violation(&[0x42, 0x02, 0x00, 0x01]));
    assert!(violation(&[0xD1, 0x00]));
    // The wrong length for the type.
    assert!(violation(&[0x20, 0x03, 0x00, 0x00, 0x00]));
    assert!(violation(&[0x40, 0x00]));
    assert!(violation(&[0xD0, 0x01, 0x00]));
    assert!(violation(&[0x20, 0x80, 0x80, 0x80, 0x80, 0x01]));
    // Reserved bits in the CONNACK's flags byte (SPEC 3.2.2.1).
    assert!(violation(&[0x20, 0x02, 0x02, 0x00]));
}

#[test]
fn the_return_codes_are_named_in_the_standards_words() {
    assert_eq!(connack_meaning(0), "connection accepted");
    assert_eq!(connack_meaning(4), "bad user name or password");
    assert_eq!(connack_meaning(5), "not authorized");
    assert_eq!(connack_meaning(6), "a reserved return code");
}
