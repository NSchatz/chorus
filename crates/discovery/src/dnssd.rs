//! DNS-based service discovery: what chorus advertises, and what an endpoint
//! makes of what it hears.
//!
//! RFC 6763 section 4.1 gives the shape and this module implements exactly it:
//! a service instance is named `<Instance> . <Service> . <Domain>`, a browse is
//! a PTR query for `<Service>.<Domain>` whose answers are instance names, and
//! resolving one is its SRV record, which "gives the target host and port where
//! the service instance can be reached", plus its TXT record, which section 6
//! says every DNS-SD service MUST have "even if the service has no additional
//! data to store".
//!
//! # What is graded here, and what is not
//!
//! Everything in this module works on BYTES. Given a committed response packet
//! it produces the advertised host and port; given a service type it produces
//! the query packet. `fixtures/discovery/` holds both, and
//! `crates/discovery/tests/dnssd_vectors.rs` holds this code to them, so a
//! second implementation is graded against the packets and not against this
//! code - exactly as `fixtures/protocol/` does for the audio wire.
//!
//! Whether those bytes reach anything is a different question and is not
//! answered here. Multicast through a container and across VLANs is an open
//! question in this deployment, which is why the endpoint has a static fallback
//! and why the live exchange is its own check that refuses by name where it
//! cannot run.

use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crate::wire::{class, flags, rtype, Message, Name, Question, Rdata, Record, WireError};

/// The port multicast DNS runs on (RFC 6762 section 2).
pub const MDNS_PORT: u16 = 5353;

/// The IPv4 link-local multicast address every `.local.` query goes to
/// (RFC 6762 section 3).
pub const MDNS_GROUP_V4: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);

/// Its IPv6 equivalent (RFC 6762 section 3).
pub const MDNS_GROUP_V6: Ipv6Addr = Ipv6Addr::new(0xff02, 0, 0, 0, 0, 0, 0, 0x00fb);

/// The service type the audio stream is advertised as.
pub const AUDIO_SERVICE: &str = "_chorus-audio._tcp.local.";

/// The service type the control channel is advertised as.
pub const CONTROL_SERVICE: &str = "_chorus-ctl._tcp.local.";

/// The TTL every record chorus advertises carries, in seconds.
///
/// RFC 6762 section 10 recommends 120 s for records containing a host name.
pub const RECORD_TTL: u32 = 120;

/// What a resolver made of one advertised instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Service {
    /// The service instance name, in full.
    pub instance: String,
    /// The instance's own label, which is what a person would recognise.
    pub label: String,
    /// The host the SRV record names.
    pub host: String,
    /// The port the SRV record names.
    pub port: u16,
    /// Every address found for that host in the same message, in the order
    /// they appeared.
    pub addresses: Vec<IpAddr>,
    /// The TXT record's key/value pairs, in the order they appeared. A string
    /// with no `=` in it is a key with no value and is carried with an empty
    /// one, per RFC 6763 section 6.4.
    pub txt: Vec<(String, String)>,
}

impl Service {
    /// One TXT value by key.
    pub fn txt_value(&self, key: &str) -> Option<&str> {
        self.txt
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }

    /// The address to connect to: the first address the message carried, with
    /// the SRV target's port.
    ///
    /// `None` where the message named a host and carried no address for it,
    /// which is a partial answer and not an answer.
    pub fn socket_address(&self) -> Option<String> {
        match self.addresses.first()? {
            IpAddr::V4(v4) => Some(format!("{}:{}", v4, self.port)),
            IpAddr::V6(v6) => Some(format!("[{}]:{}", v6, self.port)),
        }
    }
}

/// Why a message could not be read as an advertisement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveError {
    /// What was wrong.
    pub detail: String,
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.detail)
    }
}

impl std::error::Error for ResolveError {}

impl From<WireError> for ResolveError {
    fn from(e: WireError) -> ResolveError {
        ResolveError {
            detail: format!("the packet is not a DNS message: {}", e),
        }
    }
}

/// The query an endpoint sends to browse for a service type.
///
/// One PTR question, the query identifier zero (RFC 6762 section 18.1: "In
/// multicast query messages, the Query Identifier SHOULD be set to zero on
/// transmission"), and the unicast-response bit clear, so an answer goes to the
/// group and every other endpoint on the link gets it too.
pub fn browse_query(service: &str) -> Result<Message, WireError> {
    Ok(Message {
        id: 0,
        flags: 0,
        questions: vec![Question {
            name: Name::parse(service)?,
            qtype: rtype::PTR,
            qclass: class::IN,
        }],
        answers: Vec::new(),
        authorities: Vec::new(),
        additionals: Vec::new(),
    })
}

/// The bytes of that query.
pub fn browse_query_bytes(service: &str) -> Result<Vec<u8>, WireError> {
    crate::wire::encode(&browse_query(service)?)
}

/// What a server advertises about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Advertisement {
    /// The instance label, which becomes the first label of the instance name.
    pub instance: String,
    /// The service type, for example [`AUDIO_SERVICE`].
    pub service: String,
    /// The host name the SRV record targets, for example `chorus.local.`.
    pub host: String,
    /// The port the service is on.
    pub port: u16,
    /// The addresses the host has.
    pub addresses: Vec<IpAddr>,
    /// The TXT record's key/value pairs, in the order they are to be written.
    pub txt: Vec<(String, String)>,
}

impl Advertisement {
    /// The full service instance name, `<Instance>.<Service>.<Domain>`.
    pub fn instance_name(&self) -> String {
        format!("{}.{}", self.instance, self.service)
    }

    /// The response message that announces it: the PTR in the answer section,
    /// and the SRV, TXT and address records in the additional section, which is
    /// what RFC 6763 section 12 calls the additional-record set.
    pub fn response(&self) -> Result<Message, WireError> {
        let service = Name::parse(&self.service)?;
        let instance = Name::parse(&self.instance_name())?;
        let host = Name::parse(&self.host)?;
        let mut additionals = vec![
            Record {
                name: instance.clone(),
                rtype: rtype::SRV,
                class: class::IN | class::CACHE_FLUSH,
                ttl: RECORD_TTL,
                rdata: Rdata::Srv {
                    // RFC 6763 section 5: one instance is described by exactly
                    // one SRV record, and "in this common case the priority and
                    // weight fields of the SRV record SHOULD both be set to
                    // zero".
                    priority: 0,
                    weight: 0,
                    port: self.port,
                    target: host.clone(),
                },
            },
            Record {
                name: instance.clone(),
                rtype: rtype::TXT,
                class: class::IN | class::CACHE_FLUSH,
                ttl: RECORD_TTL,
                rdata: Rdata::Txt(
                    self.txt
                        .iter()
                        .map(|(k, v)| {
                            if v.is_empty() {
                                k.as_bytes().to_vec()
                            } else {
                                format!("{}={}", k, v).into_bytes()
                            }
                        })
                        .collect(),
                ),
            },
        ];
        for address in &self.addresses {
            additionals.push(Record {
                name: host.clone(),
                rtype: match address {
                    IpAddr::V4(_) => rtype::A,
                    IpAddr::V6(_) => rtype::AAAA,
                },
                class: class::IN | class::CACHE_FLUSH,
                ttl: RECORD_TTL,
                rdata: match address {
                    IpAddr::V4(v4) => Rdata::A(*v4),
                    IpAddr::V6(v6) => Rdata::Aaaa(*v6),
                },
            });
        }
        Ok(Message {
            id: 0,
            flags: flags::RESPONSE | flags::AUTHORITATIVE,
            questions: Vec::new(),
            answers: vec![Record {
                name: service,
                rtype: rtype::PTR,
                class: class::IN,
                ttl: RECORD_TTL,
                rdata: Rdata::Ptr(instance),
            }],
            authorities: Vec::new(),
            additionals,
        })
    }

    /// The bytes of that response.
    pub fn response_bytes(&self) -> Result<Vec<u8>, WireError> {
        crate::wire::encode(&self.response()?)
    }
}

/// Read every instance of `service` a response packet advertises.
///
/// An instance with no SRV record is not an instance: it has no host and no
/// port, so there is nothing to connect to, and it is left out rather than
/// returned with a guess in it. An instance whose SRV target has no address in
/// the same message is returned - the host name is real information - and
/// [`Service::socket_address`] is what says there is nothing to dial.
pub fn resolve(packet: &[u8], service: &str) -> Result<Vec<Service>, ResolveError> {
    let message = crate::wire::decode(packet)?;
    if !message.is_response() {
        return Err(ResolveError {
            detail: "the packet is a query and not a response".to_string(),
        });
    }
    let wanted = Name::parse(service).map_err(ResolveError::from)?;
    let mut found = Vec::new();
    for record in message.records() {
        let target = match (&record.rdata, record.rtype) {
            (Rdata::Ptr(target), rtype::PTR) if record.name == wanted => target,
            _ => continue,
        };
        if found.iter().any(|n: &Name| n == target) {
            continue;
        }
        found.push(target.clone());
    }

    let mut services = Vec::new();
    for instance in found {
        let srv = message.records().find(|r| {
            r.rtype == rtype::SRV && r.name == instance
        });
        let (port, host) = match srv.map(|r| &r.rdata) {
            Some(Rdata::Srv { port, target, .. }) => (*port, target.clone()),
            _ => continue,
        };
        let txt = message
            .records()
            .find(|r| r.rtype == rtype::TXT && r.name == instance)
            .map(|r| match &r.rdata {
                Rdata::Txt(strings) => strings
                    .iter()
                    .map(|s| {
                        let text = String::from_utf8_lossy(s).to_string();
                        match text.split_once('=') {
                            Some((k, v)) => (k.to_string(), v.to_string()),
                            None => (text, String::new()),
                        }
                    })
                    .collect(),
                _ => Vec::new(),
            })
            .unwrap_or_default();
        let mut addresses = Vec::new();
        for record in message.records() {
            if record.name != host {
                continue;
            }
            match &record.rdata {
                Rdata::A(v4) => addresses.push(IpAddr::V4(*v4)),
                Rdata::Aaaa(v6) => addresses.push(IpAddr::V6(*v6)),
                _ => {}
            }
        }
        services.push(Service {
            instance: instance.dotted(),
            label: instance.first_label().unwrap_or_default().to_string(),
            host: host.dotted(),
            port,
            addresses,
            txt,
        });
    }
    Ok(services)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn an_advertisement() -> Advertisement {
        Advertisement {
            instance: "chorus".to_string(),
            service: AUDIO_SERVICE.to_string(),
            host: "chorus.local.".to_string(),
            port: 4010,
            addresses: vec![IpAddr::V4(Ipv4Addr::new(192, 168, 1, 40))],
            txt: vec![
                ("v".to_string(), "1".to_string()),
                ("group".to_string(), "downstairs".to_string()),
            ],
        }
    }

    #[test]
    fn what_is_advertised_is_what_is_resolved() {
        let advertisement = an_advertisement();
        let packet = advertisement.response_bytes().unwrap();
        let services = resolve(&packet, AUDIO_SERVICE).unwrap();
        assert_eq!(services.len(), 1);
        let service = &services[0];
        assert_eq!(service.instance, "chorus._chorus-audio._tcp.local.");
        assert_eq!(service.label, "chorus");
        assert_eq!(service.host, "chorus.local.");
        assert_eq!(service.port, 4010);
        assert_eq!(service.socket_address().as_deref(), Some("192.168.1.40:4010"));
        assert_eq!(service.txt_value("group"), Some("downstairs"));
    }

    #[test]
    fn an_instance_of_a_different_service_is_not_returned() {
        let packet = an_advertisement().response_bytes().unwrap();
        assert!(resolve(&packet, CONTROL_SERVICE).unwrap().is_empty());
    }

    #[test]
    fn a_query_is_not_an_answer() {
        let bytes = browse_query_bytes(AUDIO_SERVICE).unwrap();
        let err = resolve(&bytes, AUDIO_SERVICE).unwrap_err();
        assert!(err.detail.contains("query and not a response"), "{}", err);
    }

    #[test]
    fn an_instance_with_no_srv_record_is_left_out_rather_than_guessed() {
        let mut message = an_advertisement().response().unwrap();
        message.additionals.retain(|r| r.rtype != rtype::SRV);
        let packet = crate::wire::encode(&message).unwrap();
        assert!(resolve(&packet, AUDIO_SERVICE).unwrap().is_empty());
    }

    #[test]
    fn a_txt_string_with_no_equals_is_a_key_with_no_value() {
        let mut advertisement = an_advertisement();
        advertisement.txt = vec![("bare".to_string(), String::new())];
        let packet = advertisement.response_bytes().unwrap();
        let services = resolve(&packet, AUDIO_SERVICE).unwrap();
        assert_eq!(services[0].txt, vec![("bare".to_string(), String::new())]);
    }
}
