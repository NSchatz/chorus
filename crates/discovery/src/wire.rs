//! DNS message bytes: names, questions, resource records.
//!
//! Multicast DNS is, in RFC 6762's own words, "Clients performing DNS-like
//! queries for DNS-like resource records by sending DNS-like UDP query and
//! response messages over IP Multicast to UDP port 5353"
//! (`work/specs/.../rfc6762.txt`, section 2). So the wire format here is
//! ordinary DNS wire format, and this module is the encoder and decoder for it.
//!
//! # What it reads, and what it will not
//!
//! It reads name compression, because every real responder emits it and a
//! resolver that cannot follow a pointer cannot resolve anything on a real
//! link. Following a pointer is bounded twice - a pointer may only ever point
//! BACKWARDS, and the total number followed is capped - so a message crafted to
//! make a resolver loop is refused instead.
//!
//! It does not WRITE compression. Every name this module emits is written out
//! in full. That costs bytes in a packet nobody is paying for, and it buys the
//! thing `fixtures/discovery/` needs: one message has exactly one spelling, so
//! a golden vector is a contract and not a coincidence of which name happened
//! to be written first.

use std::fmt;
use std::net::{Ipv4Addr, Ipv6Addr};

/// Record and query types this module names.
pub mod rtype {
    /// A host address.
    pub const A: u16 = 1;
    /// A pointer, which DNS-SD uses for service enumeration.
    pub const PTR: u16 = 12;
    /// Structured key/value data about a service instance.
    pub const TXT: u16 = 16;
    /// An IPv6 host address.
    pub const AAAA: u16 = 28;
    /// The target host and port of a service instance.
    pub const SRV: u16 = 33;
    /// Any type, which is what a browse asks for on a link.
    pub const ANY: u16 = 255;
}

/// The internet class, and the two bits multicast DNS puts in the class field.
pub mod class {
    /// The internet class.
    pub const IN: u16 = 1;
    /// In a QUESTION, "answer me by unicast" (RFC 6762 section 5.4).
    pub const UNICAST_RESPONSE: u16 = 0x8000;
    /// In a RESOURCE RECORD, "this record supersedes what you have cached"
    /// (RFC 6762 section 10.2).
    pub const CACHE_FLUSH: u16 = 0x8000;
    /// The class bits with the top bit removed.
    pub const MASK: u16 = 0x7fff;
}

/// Header flags.
pub mod flags {
    /// This message is a response.
    pub const RESPONSE: u16 = 0x8000;
    /// The responder is authoritative, which every mDNS response is.
    pub const AUTHORITATIVE: u16 = 0x0400;
}

/// Longest a whole encoded name may be, in bytes, including its label lengths
/// and the root label. The DNS limit, and this module holds to it.
pub const MAX_NAME_LEN: usize = 255;

/// Longest one label may be. The DNS limit.
pub const MAX_LABEL_LEN: usize = 63;

/// How many compression pointers a decoder will follow before it decides the
/// message is trying to make it loop.
pub const MAX_POINTERS: usize = 64;

/// A domain name: its labels, without the empty root label.
///
/// Comparison is case-insensitive, which is what DNS says and what a resolver
/// that meets a responder capitalising differently needs.
#[derive(Debug, Clone)]
pub struct Name(Vec<String>);

impl Name {
    /// A name from its dotted form. A trailing dot is optional.
    ///
    /// Refuses a label that is empty or too long, and a name whose encoded
    /// length would exceed the DNS limit, rather than emitting bytes no
    /// responder would accept.
    pub fn parse(text: &str) -> Result<Name, WireError> {
        let trimmed = text.strip_suffix('.').unwrap_or(text);
        if trimmed.is_empty() {
            return Ok(Name(Vec::new()));
        }
        let mut labels = Vec::new();
        let mut encoded = 1usize;
        for label in trimmed.split('.') {
            if label.is_empty() {
                return Err(WireError::new("a name has an empty label in it"));
            }
            if label.len() > MAX_LABEL_LEN {
                return Err(WireError::new(&format!(
                    "the label '{}' is {} bytes and the limit is {}",
                    label,
                    label.len(),
                    MAX_LABEL_LEN
                )));
            }
            encoded += 1 + label.len();
            labels.push(label.to_string());
        }
        if encoded > MAX_NAME_LEN {
            return Err(WireError::new(&format!(
                "'{}' encodes to {} bytes and the limit is {}",
                text, encoded, MAX_NAME_LEN
            )));
        }
        Ok(Name(labels))
    }

    /// A name from labels already checked.
    pub fn from_labels(labels: Vec<String>) -> Name {
        Name(labels)
    }

    /// The labels.
    pub fn labels(&self) -> &[String] {
        &self.0
    }

    /// The dotted form, with the trailing dot that says it is fully qualified.
    pub fn dotted(&self) -> String {
        if self.0.is_empty() {
            return ".".to_string();
        }
        format!("{}.", self.0.join("."))
    }

    /// The first label, which for a service instance name is the instance.
    pub fn first_label(&self) -> Option<&str> {
        self.0.first().map(|s| s.as_str())
    }

    /// Whether this name ends with `suffix`, label by label, case-insensitively.
    pub fn ends_with(&self, suffix: &Name) -> bool {
        if suffix.0.len() > self.0.len() {
            return false;
        }
        let from = self.0.len() - suffix.0.len();
        self.0[from..]
            .iter()
            .zip(suffix.0.iter())
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
    }

    fn encode_into(&self, out: &mut Vec<u8>) -> Result<(), WireError> {
        let start = out.len();
        for label in &self.0 {
            if label.is_empty() || label.len() > MAX_LABEL_LEN {
                return Err(WireError::new("a label is empty or over the length limit"));
            }
            out.push(label.len() as u8);
            out.extend_from_slice(label.as_bytes());
        }
        out.push(0);
        if out.len() - start > MAX_NAME_LEN {
            return Err(WireError::new("a name encodes to more than 255 bytes"));
        }
        Ok(())
    }
}

impl PartialEq for Name {
    fn eq(&self, other: &Name) -> bool {
        self.0.len() == other.0.len()
            && self
                .0
                .iter()
                .zip(other.0.iter())
                .all(|(a, b)| a.eq_ignore_ascii_case(b))
    }
}

impl Eq for Name {}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.dotted())
    }
}

/// What a record carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rdata {
    /// A pointer to another name.
    Ptr(Name),
    /// The target host and port of a service instance.
    Srv {
        /// Lower is preferred.
        priority: u16,
        /// Relative weight among equal priorities.
        weight: u16,
        /// The port the service is on.
        port: u16,
        /// The host it is on.
        target: Name,
    },
    /// Key/value strings, each with its own length byte.
    Txt(Vec<Vec<u8>>),
    /// An IPv4 address.
    A(Ipv4Addr),
    /// An IPv6 address.
    Aaaa(Ipv6Addr),
    /// A type this module does not interpret, kept as its bytes so that a
    /// message carrying one still decodes.
    Other(Vec<u8>),
}

/// One resource record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// The name this record is about.
    pub name: Name,
    /// The record type.
    pub rtype: u16,
    /// The class, including the cache-flush bit where it is set.
    pub class: u16,
    /// How long it may be cached, in seconds.
    pub ttl: u32,
    /// What it carries.
    pub rdata: Rdata,
}

/// One question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    /// The name asked about.
    pub name: Name,
    /// The type asked for.
    pub qtype: u16,
    /// The class asked in, including the unicast-response bit where it is set.
    pub qclass: u16,
}

/// A whole DNS message.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Message {
    /// The query identifier. Zero in a multicast query, per RFC 6762.
    pub id: u16,
    /// The header flags.
    pub flags: u16,
    /// The questions.
    pub questions: Vec<Question>,
    /// The answers.
    pub answers: Vec<Record>,
    /// The authority records.
    pub authorities: Vec<Record>,
    /// The additional records.
    pub additionals: Vec<Record>,
}

impl Message {
    /// Every record in the message, whichever section it is in.
    pub fn records(&self) -> impl Iterator<Item = &Record> {
        self.answers
            .iter()
            .chain(self.authorities.iter())
            .chain(self.additionals.iter())
    }

    /// Whether this is a response.
    pub fn is_response(&self) -> bool {
        self.flags & flags::RESPONSE != 0
    }
}

/// Why some bytes are not a DNS message this module accepts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireError {
    /// What was wrong.
    pub detail: String,
}

impl WireError {
    fn new(detail: &str) -> WireError {
        WireError {
            detail: detail.to_string(),
        }
    }
}

impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.detail)
    }
}

impl std::error::Error for WireError {}

/// Encode a message, with every name written out in full.
pub fn encode(message: &Message) -> Result<Vec<u8>, WireError> {
    let mut out = Vec::with_capacity(512);
    out.extend_from_slice(&message.id.to_be_bytes());
    out.extend_from_slice(&message.flags.to_be_bytes());
    out.extend_from_slice(&(message.questions.len() as u16).to_be_bytes());
    out.extend_from_slice(&(message.answers.len() as u16).to_be_bytes());
    out.extend_from_slice(&(message.authorities.len() as u16).to_be_bytes());
    out.extend_from_slice(&(message.additionals.len() as u16).to_be_bytes());
    for question in &message.questions {
        question.name.encode_into(&mut out)?;
        out.extend_from_slice(&question.qtype.to_be_bytes());
        out.extend_from_slice(&question.qclass.to_be_bytes());
    }
    for record in message
        .answers
        .iter()
        .chain(message.authorities.iter())
        .chain(message.additionals.iter())
    {
        encode_record(record, &mut out)?;
    }
    Ok(out)
}

fn encode_record(record: &Record, out: &mut Vec<u8>) -> Result<(), WireError> {
    record.name.encode_into(out)?;
    out.extend_from_slice(&record.rtype.to_be_bytes());
    out.extend_from_slice(&record.class.to_be_bytes());
    out.extend_from_slice(&record.ttl.to_be_bytes());
    let mut rdata = Vec::new();
    match &record.rdata {
        Rdata::Ptr(name) => name.encode_into(&mut rdata)?,
        Rdata::Srv {
            priority,
            weight,
            port,
            target,
        } => {
            rdata.extend_from_slice(&priority.to_be_bytes());
            rdata.extend_from_slice(&weight.to_be_bytes());
            rdata.extend_from_slice(&port.to_be_bytes());
            target.encode_into(&mut rdata)?;
        }
        Rdata::Txt(strings) => {
            for string in strings {
                if string.len() > 255 {
                    return Err(WireError::new(
                        "a TXT string is over the 255 bytes one length byte can count",
                    ));
                }
                rdata.push(string.len() as u8);
                rdata.extend_from_slice(string);
            }
            if rdata.is_empty() {
                // RFC 6763 section 6.1: an empty TXT record is one zero byte,
                // not zero bytes.
                rdata.push(0);
            }
        }
        Rdata::A(address) => rdata.extend_from_slice(&address.octets()),
        Rdata::Aaaa(address) => rdata.extend_from_slice(&address.octets()),
        Rdata::Other(bytes) => rdata.extend_from_slice(bytes),
    }
    if rdata.len() > u16::MAX as usize {
        return Err(WireError::new("a record's data is over 65535 bytes"));
    }
    out.extend_from_slice(&(rdata.len() as u16).to_be_bytes());
    out.extend_from_slice(&rdata);
    Ok(())
}

/// Decode a message.
pub fn decode(bytes: &[u8]) -> Result<Message, WireError> {
    if bytes.len() < 12 {
        return Err(WireError::new(
            "fewer than the 12 bytes a DNS header occupies",
        ));
    }
    let id = u16::from_be_bytes([bytes[0], bytes[1]]);
    let header_flags = u16::from_be_bytes([bytes[2], bytes[3]]);
    let counts = [
        u16::from_be_bytes([bytes[4], bytes[5]]) as usize,
        u16::from_be_bytes([bytes[6], bytes[7]]) as usize,
        u16::from_be_bytes([bytes[8], bytes[9]]) as usize,
        u16::from_be_bytes([bytes[10], bytes[11]]) as usize,
    ];
    let mut at = 12usize;
    let mut questions = Vec::with_capacity(counts[0]);
    for _ in 0..counts[0] {
        let name = read_name(bytes, &mut at)?;
        let qtype = read_u16(bytes, &mut at)?;
        let qclass = read_u16(bytes, &mut at)?;
        questions.push(Question {
            name,
            qtype,
            qclass,
        });
    }
    let mut sections: Vec<Vec<Record>> = Vec::with_capacity(3);
    for count in &counts[1..] {
        let mut records = Vec::with_capacity(*count);
        for _ in 0..*count {
            records.push(read_record(bytes, &mut at)?);
        }
        sections.push(records);
    }
    let mut sections = sections.into_iter();
    Ok(Message {
        id,
        flags: header_flags,
        questions,
        answers: sections.next().unwrap_or_default(),
        authorities: sections.next().unwrap_or_default(),
        additionals: sections.next().unwrap_or_default(),
    })
}

fn read_u16(bytes: &[u8], at: &mut usize) -> Result<u16, WireError> {
    if *at + 2 > bytes.len() {
        return Err(WireError::new("the message ended inside a 16-bit field"));
    }
    let value = u16::from_be_bytes([bytes[*at], bytes[*at + 1]]);
    *at += 2;
    Ok(value)
}

fn read_u32(bytes: &[u8], at: &mut usize) -> Result<u32, WireError> {
    if *at + 4 > bytes.len() {
        return Err(WireError::new("the message ended inside a 32-bit field"));
    }
    let value = u32::from_be_bytes([bytes[*at], bytes[*at + 1], bytes[*at + 2], bytes[*at + 3]]);
    *at += 4;
    Ok(value)
}

/// Read a name, following compression pointers.
///
/// `at` is left just past the name AS IT APPEARS HERE, which for a compressed
/// name is just past its pointer and not just past whatever it pointed at.
fn read_name(bytes: &[u8], at: &mut usize) -> Result<Name, WireError> {
    let mut labels = Vec::new();
    let mut cursor = *at;
    let mut followed = 0usize;
    let mut jumped = false;
    let mut encoded = 1usize;
    loop {
        let length = *bytes.get(cursor).ok_or_else(|| {
            WireError::new("the message ended where a name's label length was expected")
        })?;
        match length & 0xC0 {
            0 => {
                cursor += 1;
                if length == 0 {
                    if !jumped {
                        *at = cursor;
                    }
                    return Ok(Name(labels));
                }
                let end = cursor + length as usize;
                let label = bytes.get(cursor..end).ok_or_else(|| {
                    WireError::new("the message ended inside a name's label")
                })?;
                encoded += 1 + label.len();
                if encoded > MAX_NAME_LEN {
                    return Err(WireError::new("a name is over the 255-byte limit"));
                }
                labels.push(String::from_utf8_lossy(label).to_string());
                cursor = end;
            }
            0xC0 => {
                let second = *bytes.get(cursor + 1).ok_or_else(|| {
                    WireError::new("a compression pointer is one byte short")
                })?;
                let target =
                    (((length & 0x3F) as usize) << 8) | second as usize;
                if !jumped {
                    *at = cursor + 2;
                    jumped = true;
                }
                // A pointer may only point backwards. That single rule is what
                // makes following one terminate; the counter below is the
                // belt to its braces.
                if target >= cursor {
                    return Err(WireError::new(
                        "a compression pointer points forwards or at itself, which cannot \
                         terminate",
                    ));
                }
                followed += 1;
                if followed > MAX_POINTERS {
                    return Err(WireError::new(
                        "a name follows more compression pointers than this decoder will",
                    ));
                }
                cursor = target;
            }
            _ => {
                return Err(WireError::new(
                    "a label length has reserved bits set, so this is not a name",
                ))
            }
        }
    }
}

fn read_record(bytes: &[u8], at: &mut usize) -> Result<Record, WireError> {
    let name = read_name(bytes, at)?;
    let rtype = read_u16(bytes, at)?;
    let record_class = read_u16(bytes, at)?;
    let ttl = read_u32(bytes, at)?;
    let length = read_u16(bytes, at)? as usize;
    let end = *at + length;
    if end > bytes.len() {
        return Err(WireError::new(
            "a record declares more data than the message holds",
        ));
    }
    let body = &bytes[*at..end];
    let rdata = match rtype {
        rtype::PTR => {
            let mut inner = *at;
            let target = read_name(bytes, &mut inner)?;
            Rdata::Ptr(target)
        }
        rtype::SRV => {
            if length < 6 {
                return Err(WireError::new("an SRV record is shorter than its fixed part"));
            }
            let mut inner = *at;
            let priority = read_u16(bytes, &mut inner)?;
            let weight = read_u16(bytes, &mut inner)?;
            let port = read_u16(bytes, &mut inner)?;
            let target = read_name(bytes, &mut inner)?;
            Rdata::Srv {
                priority,
                weight,
                port,
                target,
            }
        }
        rtype::TXT => {
            let mut strings = Vec::new();
            let mut inner = 0usize;
            while inner < body.len() {
                let len = body[inner] as usize;
                inner += 1;
                let stop = inner + len;
                if stop > body.len() {
                    return Err(WireError::new(
                        "a TXT string declares more bytes than the record holds",
                    ));
                }
                if len > 0 {
                    strings.push(body[inner..stop].to_vec());
                }
                inner = stop;
            }
            Rdata::Txt(strings)
        }
        rtype::A => {
            if length != 4 {
                return Err(WireError::new("an A record is not four bytes"));
            }
            Rdata::A(Ipv4Addr::new(body[0], body[1], body[2], body[3]))
        }
        rtype::AAAA => {
            if length != 16 {
                return Err(WireError::new("an AAAA record is not sixteen bytes"));
            }
            let mut octets = [0u8; 16];
            octets.copy_from_slice(body);
            Rdata::Aaaa(Ipv6Addr::from(octets))
        }
        _ => Rdata::Other(body.to_vec()),
    };
    *at = end;
    Ok(Record {
        name,
        rtype,
        class: record_class,
        ttl,
        rdata,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_survives_the_round_trip() {
        let message = Message {
            id: 0,
            flags: flags::RESPONSE | flags::AUTHORITATIVE,
            questions: vec![Question {
                name: Name::parse("_chorus-audio._tcp.local.").unwrap(),
                qtype: rtype::PTR,
                qclass: class::IN,
            }],
            answers: vec![Record {
                name: Name::parse("_chorus-audio._tcp.local.").unwrap(),
                rtype: rtype::PTR,
                class: class::IN,
                ttl: 120,
                rdata: Rdata::Ptr(Name::parse("hifi._chorus-audio._tcp.local.").unwrap()),
            }],
            authorities: Vec::new(),
            additionals: vec![
                Record {
                    name: Name::parse("hifi._chorus-audio._tcp.local.").unwrap(),
                    rtype: rtype::SRV,
                    class: class::IN | class::CACHE_FLUSH,
                    ttl: 120,
                    rdata: Rdata::Srv {
                        priority: 0,
                        weight: 0,
                        port: 4010,
                        target: Name::parse("hifi.local.").unwrap(),
                    },
                },
                Record {
                    name: Name::parse("hifi._chorus-audio._tcp.local.").unwrap(),
                    rtype: rtype::TXT,
                    class: class::IN | class::CACHE_FLUSH,
                    ttl: 120,
                    rdata: Rdata::Txt(vec![b"v=1".to_vec()]),
                },
                Record {
                    name: Name::parse("hifi.local.").unwrap(),
                    rtype: rtype::A,
                    class: class::IN | class::CACHE_FLUSH,
                    ttl: 120,
                    rdata: Rdata::A(Ipv4Addr::new(127, 0, 0, 1)),
                },
            ],
        };
        let bytes = encode(&message).unwrap();
        assert_eq!(decode(&bytes).unwrap(), message);
    }

    #[test]
    fn a_compression_pointer_is_followed() {
        // The name at offset 12 is written out; the second one points at it.
        let mut bytes = vec![0, 0, 0x84, 0, 0, 0, 0, 2, 0, 0, 0, 0];
        let full = Name::parse("hifi.local.").unwrap();
        let mut name = Vec::new();
        full.encode_into(&mut name).unwrap();
        let target_at = bytes.len();
        // Answer 1: hifi.local. A 127.0.0.1
        bytes.extend_from_slice(&name);
        bytes.extend_from_slice(&rtype::A.to_be_bytes());
        bytes.extend_from_slice(&class::IN.to_be_bytes());
        bytes.extend_from_slice(&120u32.to_be_bytes());
        bytes.extend_from_slice(&4u16.to_be_bytes());
        bytes.extend_from_slice(&[127, 0, 0, 1]);
        // Answer 2: the same name, as a pointer.
        bytes.push(0xC0 | (target_at >> 8) as u8);
        bytes.push((target_at & 0xFF) as u8);
        bytes.extend_from_slice(&rtype::A.to_be_bytes());
        bytes.extend_from_slice(&class::IN.to_be_bytes());
        bytes.extend_from_slice(&120u32.to_be_bytes());
        bytes.extend_from_slice(&4u16.to_be_bytes());
        bytes.extend_from_slice(&[127, 0, 0, 2]);

        let message = decode(&bytes).unwrap();
        assert_eq!(message.answers.len(), 2);
        assert_eq!(message.answers[0].name, full);
        assert_eq!(message.answers[1].name, full);
    }

    #[test]
    fn a_pointer_that_cannot_terminate_is_refused_rather_than_followed() {
        // A pointer at offset 12 pointing at itself.
        let mut bytes = vec![0, 0, 0x84, 0, 0, 0, 0, 1, 0, 0, 0, 0];
        bytes.push(0xC0);
        bytes.push(12);
        let err = decode(&bytes).unwrap_err();
        assert!(err.detail.contains("cannot"), "{}", err);
        // And one pointing forwards.
        let mut bytes = vec![0, 0, 0x84, 0, 0, 0, 0, 1, 0, 0, 0, 0];
        bytes.push(0xC0);
        bytes.push(40);
        bytes.resize(60, 0);
        assert!(decode(&bytes).is_err());
    }

    #[test]
    fn a_name_compares_without_regard_to_case() {
        assert_eq!(
            Name::parse("HIFI._Chorus-Audio._TCP.Local.").unwrap(),
            Name::parse("hifi._chorus-audio._tcp.local.").unwrap()
        );
        assert!(Name::parse("hifi._chorus-audio._tcp.local.")
            .unwrap()
            .ends_with(&Name::parse("_CHORUS-AUDIO._TCP.LOCAL.").unwrap()));
    }

    #[test]
    fn a_truncated_message_is_refused_at_every_place_it_can_be_cut() {
        let message = Message {
            id: 0,
            flags: flags::RESPONSE,
            questions: Vec::new(),
            answers: vec![Record {
                name: Name::parse("hifi.local.").unwrap(),
                rtype: rtype::A,
                class: class::IN,
                ttl: 120,
                rdata: Rdata::A(Ipv4Addr::new(127, 0, 0, 1)),
            }],
            authorities: Vec::new(),
            additionals: Vec::new(),
        };
        let bytes = encode(&message).unwrap();
        for cut in 0..bytes.len() {
            assert!(
                decode(&bytes[..cut]).is_err(),
                "{} bytes of a {}-byte message decoded",
                cut,
                bytes.len()
            );
        }
        assert!(decode(&bytes).is_ok());
    }
}
