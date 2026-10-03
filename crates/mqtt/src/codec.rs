//! MQTT 3.1.1 packets for a client that only publishes.
//!
//! Encoders for CONNECT, PUBLISH, PINGREQ and DISCONNECT; a decoder for
//! CONNACK, PUBACK and PINGRESP. Nothing else exists: no SUBSCRIBE, no
//! UNSUBSCRIBE, no QoS 2, no inbound PUBLISH (a client that never subscribes
//! is never sent one, and receiving one is a protocol violation here).
//!
//! Every packet is a fixed header (one byte of type and flags, then the
//! Remaining Length, 1 to 4 bytes), then its body (SPEC 2.1, 2.2). Multi-byte
//! integers are big-endian (SPEC 1.5.2).

use std::fmt;

/// The largest Remaining Length the format carries: four bytes of seven bits
/// (SPEC 2.2.3).
pub const MAX_REMAINING_LENGTH: u32 = 268_435_455;

/// PINGREQ, whole (SPEC 3.12).
pub const PINGREQ: [u8; 2] = [0xC0, 0x00];

/// DISCONNECT, whole (SPEC 3.14).
pub const DISCONNECT: [u8; 2] = [0xE0, 0x00];

/// The protocol level of MQTT 3.1.1 (SPEC 3.1.2.2).
pub const PROTOCOL_LEVEL: u8 = 4;

/// Why a packet was not encoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodeError {
    /// A string or binary field longer than its two-byte length can say
    /// (SPEC 1.5.3).
    FieldTooLong {
        /// Which field.
        field: &'static str,
        /// Its length in bytes.
        len: usize,
    },
    /// A string holding U+0000 [MQTT-1.5.3-2] or another control character
    /// (SPEC 1.5.3 lets a receiver close the connection on those).
    ControlCharacter {
        /// Which field.
        field: &'static str,
    },
    /// A field that must hold at least one character and holds none.
    Empty {
        /// Which field.
        field: &'static str,
    },
    /// A topic name holding a wildcard, `+` or `#` [MQTT-3.3.2-2].
    WildcardInTopic,
    /// A password with no user name [MQTT-3.1.2-22].
    PasswordWithoutUser,
    /// A QoS 1 PUBLISH with packet identifier zero [MQTT-2.3.1-1].
    PacketIdZero,
    /// A body longer than [`MAX_REMAINING_LENGTH`].
    TooLong {
        /// The body's length in bytes.
        len: usize,
    },
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EncodeError::FieldTooLong { field, len } => write!(
                f,
                "the {} is {} bytes and MQTT carries at most 65535",
                field, len
            ),
            EncodeError::ControlCharacter { field } => {
                write!(f, "the {} holds a control character", field)
            }
            EncodeError::Empty { field } => write!(f, "the {} is empty", field),
            EncodeError::WildcardInTopic => {
                write!(f, "a topic name holds '+' or '#', which only a filter may")
            }
            EncodeError::PasswordWithoutUser => {
                write!(f, "a password was given with no user name")
            }
            EncodeError::PacketIdZero => {
                write!(f, "a QoS 1 PUBLISH needs a packet identifier other than 0")
            }
            EncodeError::TooLong { len } => write!(
                f,
                "a packet body of {} bytes is past MQTT's {}",
                len, MAX_REMAINING_LENGTH
            ),
        }
    }
}

impl std::error::Error for EncodeError {}

/// Append `value` as a Remaining Length: base 128, least significant group
/// first, bit 7 set on every byte but the last (SPEC 2.2.3).
pub fn encode_remaining_length(value: u32, out: &mut Vec<u8>) -> Result<(), EncodeError> {
    if value > MAX_REMAINING_LENGTH {
        return Err(EncodeError::TooLong {
            len: value as usize,
        });
    }
    let mut rest = value;
    loop {
        let mut byte = (rest % 128) as u8;
        rest /= 128;
        if rest > 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if rest == 0 {
            return Ok(());
        }
    }
}

/// What the front of a buffer holds, read as a Remaining Length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemainingLength {
    /// The value, and how many bytes it took (1 to 4).
    Value {
        /// The length.
        value: u32,
        /// Bytes consumed.
        used: usize,
    },
    /// The buffer ends inside the field.
    Incomplete,
    /// A fourth byte with bit 7 set: there is no fifth (SPEC 2.2.3).
    Malformed,
}

/// Read a Remaining Length from the front of `bytes`.
pub fn decode_remaining_length(bytes: &[u8]) -> RemainingLength {
    let mut value = 0u32;
    for (index, byte) in bytes.iter().take(4).enumerate() {
        value |= u32::from(byte & 0x7F) << (7 * index);
        if byte & 0x80 == 0 {
            return RemainingLength::Value {
                value,
                used: index + 1,
            };
        }
    }
    if bytes.len() >= 4 {
        RemainingLength::Malformed
    } else {
        RemainingLength::Incomplete
    }
}

/// The two delivery levels this client uses (SPEC 4.3). QoS 2 does not exist
/// here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Qos {
    /// QoS 0: sent once, never acknowledged.
    AtMostOnce,
    /// QoS 1: acknowledged by a PUBACK carrying the same packet identifier.
    AtLeastOnce,
}

impl Qos {
    fn bits(self) -> u8 {
        match self {
            Qos::AtMostOnce => 0,
            Qos::AtLeastOnce => 1,
        }
    }
}

/// A text that may travel as an MQTT "UTF-8 encoded string" (SPEC 1.5.3): a
/// Rust `&str` is already well-formed UTF-8 with no surrogates
/// [MQTT-1.5.3-1]; this adds no U+0000 [MQTT-1.5.3-2], none of the control
/// characters a receiver may close the connection on, and the length bound.
fn check_string(field: &'static str, text: &str) -> Result<(), EncodeError> {
    if text.len() > usize::from(u16::MAX) {
        return Err(EncodeError::FieldTooLong {
            field,
            len: text.len(),
        });
    }
    if text.chars().any(char::is_control) {
        return Err(EncodeError::ControlCharacter { field });
    }
    Ok(())
}

fn push_field(field: &'static str, bytes: &[u8], out: &mut Vec<u8>) -> Result<(), EncodeError> {
    let len = u16::try_from(bytes.len()).map_err(|_| EncodeError::FieldTooLong {
        field,
        len: bytes.len(),
    })?;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

/// A topic name a PUBLISH or a will may carry: at least one character
/// [MQTT-4.7.3-1], no wildcard [MQTT-3.3.2-2], and a legal string.
fn check_topic(field: &'static str, topic: &str) -> Result<(), EncodeError> {
    if topic.is_empty() {
        return Err(EncodeError::Empty { field });
    }
    check_string(field, topic)?;
    if topic.contains(['+', '#']) {
        return Err(EncodeError::WildcardInTopic);
    }
    Ok(())
}

fn framed(first: u8, body: Vec<u8>) -> Result<Vec<u8>, EncodeError> {
    let len = u32::try_from(body.len())
        .ok()
        .filter(|l| *l <= MAX_REMAINING_LENGTH)
        .ok_or(EncodeError::TooLong { len: body.len() })?;
    let mut packet = Vec::with_capacity(body.len() + 5);
    packet.push(first);
    encode_remaining_length(len, &mut packet)?;
    packet.extend_from_slice(&body);
    Ok(packet)
}

/// The message the broker publishes for a client that went away without a
/// DISCONNECT (SPEC 3.1.2.5).
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Will<'a> {
    /// Where it is published.
    pub topic: &'a str,
    /// What is published.
    pub message: &'a [u8],
    /// At which level.
    pub qos: Qos,
    /// Whether the broker keeps it for later subscribers.
    pub retain: bool,
}

/// A CONNECT (SPEC 3.1). Always a clean session: this client keeps no session
/// state and republishes what it holds after every connect (SPEC 3.1.2.4).
///
/// No `Debug`: a value of this type holds the password.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Connect<'a> {
    /// The client identifier, always present and first in the payload
    /// [MQTT-3.1.3-3].
    pub client_id: &'a str,
    /// Keep Alive in seconds; 0 turns the mechanism off (SPEC 3.1.2.10).
    pub keep_alive_s: u16,
    /// The will, if any.
    pub will: Option<Will<'a>>,
    /// The user name, if any.
    pub user: Option<&'a str>,
    /// The password, if any; binary data, and only with a user name
    /// [MQTT-3.1.2-22].
    pub password: Option<&'a [u8]>,
}

impl Connect<'_> {
    /// The packet's bytes.
    pub fn encode(&self) -> Result<Vec<u8>, EncodeError> {
        check_string("client id", self.client_id)?;
        if self.password.is_some() && self.user.is_none() {
            return Err(EncodeError::PasswordWithoutUser);
        }
        // Clean Session, bit 1. Bit 0 is reserved and 0 [MQTT-3.1.2-3].
        let mut flags = 0x02u8;
        if let Some(will) = &self.will {
            check_topic("will topic", will.topic)?;
            flags |= 0x04 | (will.qos.bits() << 3);
            if will.retain {
                flags |= 0x20;
            }
        }
        if let Some(user) = self.user {
            check_string("user name", user)?;
            flags |= 0x80;
        }
        if self.password.is_some() {
            flags |= 0x40;
        }
        let mut body = Vec::new();
        // The variable header: the protocol name, the level, the flags and
        // the keep alive (SPEC 3.1.2).
        push_field("protocol name", b"MQTT", &mut body)?;
        body.push(PROTOCOL_LEVEL);
        body.push(flags);
        body.extend_from_slice(&self.keep_alive_s.to_be_bytes());
        // The payload, in the one order the format allows [MQTT-3.1.3-1].
        push_field("client id", self.client_id.as_bytes(), &mut body)?;
        if let Some(will) = &self.will {
            push_field("will topic", will.topic.as_bytes(), &mut body)?;
            push_field("will message", will.message, &mut body)?;
        }
        if let Some(user) = self.user {
            push_field("user name", user.as_bytes(), &mut body)?;
        }
        if let Some(password) = self.password {
            push_field("password", password, &mut body)?;
        }
        framed(0x10, body)
    }
}

/// A PUBLISH (SPEC 3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Publish<'a> {
    /// The topic name.
    pub topic: &'a str,
    /// The payload; empty with `retain` clears the topic's retained message
    /// [MQTT-3.3.1-10].
    pub payload: &'a [u8],
    /// The delivery level.
    pub qos: Qos,
    /// Whether the broker keeps it for later subscribers.
    pub retain: bool,
    /// The packet identifier: not zero at QoS 1 [MQTT-2.3.1-1], not sent at
    /// QoS 0 [MQTT-2.3.1-5].
    pub packet_id: u16,
}

impl Publish<'_> {
    /// The packet's bytes. DUP is never set: a clean-session client never
    /// sends a packet again (SPEC 4.4, [MQTT-3.1.2-6]).
    pub fn encode(&self) -> Result<Vec<u8>, EncodeError> {
        check_topic("topic", self.topic)?;
        if self.qos == Qos::AtLeastOnce && self.packet_id == 0 {
            return Err(EncodeError::PacketIdZero);
        }
        let first = 0x30 | (self.qos.bits() << 1) | u8::from(self.retain);
        let mut body = Vec::with_capacity(self.topic.len() + self.payload.len() + 4);
        push_field("topic", self.topic.as_bytes(), &mut body)?;
        if self.qos == Qos::AtLeastOnce {
            body.extend_from_slice(&self.packet_id.to_be_bytes());
        }
        body.extend_from_slice(self.payload);
        framed(first, body)
    }
}

/// A packet the broker sends a publish-only client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inbound {
    /// CONNACK (SPEC 3.2): whether the broker says it holds a session, and
    /// the return code (0 is accepted).
    ConnAck {
        /// The Session Present flag.
        session_present: bool,
        /// The return code (SPEC 3.2.2.3).
        code: u8,
    },
    /// PUBACK (SPEC 3.4) for the PUBLISH with this packet identifier.
    PubAck {
        /// The identifier acknowledged.
        packet_id: u16,
    },
    /// PINGRESP (SPEC 3.13).
    PingResp,
}

/// What the front of a buffer of broker bytes holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decoded {
    /// One whole packet, and how many bytes it took.
    Packet {
        /// The packet.
        packet: Inbound,
        /// Bytes consumed.
        used: usize,
    },
    /// The buffer ends inside a packet: read more.
    Incomplete,
    /// Not something a broker may send this client; the connection is to be
    /// closed [MQTT-4.8.0-1].
    Violation(&'static str),
}

/// Read one packet from the front of `bytes`.
///
/// Anything but a CONNACK, a PUBACK or a PINGRESP with exactly the flags and
/// length the format gives it is a violation, including every packet type
/// that only a subscriber is sent.
pub fn decode_inbound(bytes: &[u8]) -> Decoded {
    let Some(first) = bytes.first() else {
        return Decoded::Incomplete;
    };
    let expected_len: u32 = match first {
        0x20 | 0x40 => 2,
        0xD0 => 0,
        _ => return Decoded::Violation("a packet a publish-only client is never sent"),
    };
    let (len, used) = match decode_remaining_length(&bytes[1..]) {
        RemainingLength::Value { value, used } => (value, used),
        RemainingLength::Incomplete => return Decoded::Incomplete,
        RemainingLength::Malformed => return Decoded::Violation("a malformed remaining length"),
    };
    if len != expected_len {
        return Decoded::Violation("a packet of the wrong length for its type");
    }
    let body = &bytes[1 + used..];
    if body.len() < len as usize {
        return Decoded::Incomplete;
    }
    let used = 1 + used + len as usize;
    let packet = match first {
        0x20 => {
            // Bits 7 to 1 of the acknowledge flags are reserved and 0
            // (SPEC 3.2.2.1).
            if body[0] & 0xFE != 0 {
                return Decoded::Violation("a CONNACK with reserved flags set");
            }
            Inbound::ConnAck {
                session_present: body[0] & 0x01 != 0,
                code: body[1],
            }
        }
        0x40 => Inbound::PubAck {
            packet_id: u16::from_be_bytes([body[0], body[1]]),
        },
        _ => Inbound::PingResp,
    };
    Decoded::Packet { packet, used }
}

/// What a CONNACK return code means, in the standard's words (SPEC 3.2.2.3,
/// Table 3.1).
pub fn connack_meaning(code: u8) -> &'static str {
    match code {
        0 => "connection accepted",
        1 => "unacceptable protocol version",
        2 => "identifier rejected",
        3 => "server unavailable",
        4 => "bad user name or password",
        5 => "not authorized",
        _ => "a reserved return code",
    }
}
