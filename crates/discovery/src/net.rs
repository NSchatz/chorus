//! The sockets: advertising on a link, and browsing one.
//!
//! Everything that can be decided from bytes alone is in [`crate::dnssd`] and
//! is graded against committed packets. What is here is the part that needs a
//! network, and it is deliberately thin, because it is also the part that
//! cannot be graded in a container whose link may not carry multicast at all.
//!
//! # Failing is not the same as finding nothing
//!
//! [`browse`] returns an empty list when the link carried no answer, and an
//! error when the socket could not be opened, could not join the group or
//! could not send. The endpoint treats those differently: nothing found is the
//! case the static fallback exists for, and a socket that would not open is
//! reported by name. Collapsing the two would turn "this container has no
//! multicast" into "there is no server", which is the mistake that makes a
//! fallback look like it worked.

use std::fmt;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::time::{Duration, Instant};

use crate::dnssd::{
    browse_query_bytes, resolve, Advertisement, Service, MDNS_GROUP_V4, MDNS_PORT,
};
use crate::wire::{class, rtype, Message};

/// Largest datagram this module will read. RFC 6762 section 17 allows a
/// message up to the interface MTU, and 9000 covers a jumbo frame.
pub const MAX_DATAGRAM: usize = 9_000;

/// Why a socket operation could not be done.
#[derive(Debug)]
pub struct DiscoveryError {
    /// What was being attempted.
    pub what: String,
    /// What the operating system said.
    pub cause: io::Error,
}

impl fmt::Display for DiscoveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.what, self.cause)
    }
}

impl std::error::Error for DiscoveryError {}

fn failing(what: &str, cause: io::Error) -> DiscoveryError {
    DiscoveryError {
        what: what.to_string(),
        cause,
    }
}

/// The multicast socket a server answers browses on.
pub struct Advertiser {
    socket: UdpSocket,
    advertisements: Vec<Advertisement>,
}

impl Advertiser {
    /// Open the multicast socket and join the group.
    ///
    /// Both halves can fail on a host that does not carry multicast, and both
    /// are reported by name rather than swallowed: a server told to advertise
    /// and unable to is a server that would otherwise look as though it had.
    pub fn open(advertisements: Vec<Advertisement>) -> Result<Advertiser, DiscoveryError> {
        let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, MDNS_PORT))
            .map_err(|e| {
                failing(
                    &format!("binding UDP port {} for multicast DNS", MDNS_PORT),
                    e,
                )
            })?;
        socket
            .join_multicast_v4(&MDNS_GROUP_V4, &Ipv4Addr::UNSPECIFIED)
            .map_err(|e| failing(&format!("joining the multicast group {}", MDNS_GROUP_V4), e))?;
        // RFC 6762 section 11: an mDNS packet is sent with an IP TTL of 255.
        let _ = socket.set_multicast_ttl_v4(255);
        socket
            .set_read_timeout(Some(Duration::from_millis(200)))
            .map_err(|e| failing("setting a read timeout on the multicast socket", e))?;
        Ok(Advertiser {
            socket,
            advertisements,
        })
    }

    /// The service instance names this advertiser answers for.
    pub fn instances(&self) -> Vec<String> {
        self.advertisements
            .iter()
            .map(|a| a.instance_name())
            .collect()
    }

    /// Send every advertisement to the group, unsolicited.
    pub fn announce(&self) -> Result<usize, DiscoveryError> {
        let mut sent = 0;
        for advertisement in &self.advertisements {
            let bytes = advertisement.response_bytes().map_err(|e| {
                failing(
                    "encoding an advertisement",
                    io::Error::new(io::ErrorKind::InvalidData, e.to_string()),
                )
            })?;
            self.socket
                .send_to(&bytes, SocketAddr::from((MDNS_GROUP_V4, MDNS_PORT)))
                .map_err(|e| failing("sending an announcement to the multicast group", e))?;
            sent += 1;
        }
        Ok(sent)
    }

    /// Read whatever has arrived and answer any browse this advertiser knows
    /// about. Returns how many answers it sent.
    ///
    /// Never blocks longer than the socket's read timeout, so the thread that
    /// runs it can notice it is no longer wanted.
    pub fn answer_pending(&self) -> usize {
        let mut answered = 0;
        let mut scratch = vec![0u8; MAX_DATAGRAM];
        loop {
            let (read, from) = match self.socket.recv_from(&mut scratch) {
                Ok(v) => v,
                Err(_) => return answered,
            };
            let message = match crate::wire::decode(&scratch[..read]) {
                Ok(m) => m,
                Err(_) => continue,
            };
            if message.is_response() {
                continue;
            }
            for question in &message.questions {
                for advertisement in &self.advertisements {
                    let service = match crate::wire::Name::parse(&advertisement.service) {
                        Ok(n) => n,
                        Err(_) => continue,
                    };
                    let asks_for_us = question.name == service
                        && (question.qtype == rtype::PTR || question.qtype == rtype::ANY);
                    if !asks_for_us {
                        continue;
                    }
                    let bytes = match advertisement.response_bytes() {
                        Ok(b) => b,
                        Err(_) => continue,
                    };
                    // RFC 6762 section 5.4: a question with the unicast-response
                    // bit set is answered directly to the asker.
                    let to = if question.qclass & class::UNICAST_RESPONSE != 0 {
                        from
                    } else {
                        SocketAddr::from((MDNS_GROUP_V4, MDNS_PORT))
                    };
                    if self.socket.send_to(&bytes, to).is_ok() {
                        answered += 1;
                    }
                }
            }
        }
    }
}

/// Browse the link for a service type for `window`, and report every instance
/// that answered.
///
/// The query carries the unicast-response bit, so a responder answers this
/// socket directly. That is what lets an endpoint browse without binding port
/// 5353, which it cannot do on a host already running a responder of its own.
pub fn browse(service: &str, window: Duration) -> Result<Vec<Service>, DiscoveryError> {
    let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0))
        .map_err(|e| failing("binding a UDP socket to browse from", e))?;
    let _ = socket.set_multicast_ttl_v4(255);
    socket
        .set_read_timeout(Some(Duration::from_millis(50)))
        .map_err(|e| failing("setting a read timeout on the browse socket", e))?;

    let mut query = crate::wire::decode(&browse_query_bytes(service).map_err(|e| {
        failing(
            "encoding the browse query",
            io::Error::new(io::ErrorKind::InvalidData, e.to_string()),
        )
    })?)
    .map_err(|e| {
        failing(
            "reading back the browse query",
            io::Error::new(io::ErrorKind::InvalidData, e.to_string()),
        )
    })?;
    for question in &mut query.questions {
        question.qclass |= class::UNICAST_RESPONSE;
    }
    let bytes = encode_or(failing_bytes, &query)?;
    socket
        .send_to(&bytes, SocketAddr::from((MDNS_GROUP_V4, MDNS_PORT)))
        .map_err(|e| failing("sending the browse query to the multicast group", e))?;

    let deadline = Instant::now() + window;
    let mut services: Vec<Service> = Vec::new();
    let mut scratch = vec![0u8; MAX_DATAGRAM];
    while Instant::now() < deadline {
        let read = match socket.recv_from(&mut scratch) {
            Ok((read, _)) => read,
            Err(_) => continue,
        };
        if let Ok(found) = resolve(&scratch[..read], service) {
            for one in found {
                if !services.iter().any(|s| s.instance == one.instance) {
                    services.push(one);
                }
            }
        }
    }
    Ok(services)
}

fn failing_bytes(e: crate::wire::WireError) -> DiscoveryError {
    failing(
        "encoding the browse query",
        io::Error::new(io::ErrorKind::InvalidData, e.to_string()),
    )
}

fn encode_or(
    on_error: fn(crate::wire::WireError) -> DiscoveryError,
    message: &Message,
) -> Result<Vec<u8>, DiscoveryError> {
    crate::wire::encode(message).map_err(on_error)
}

/// Where an endpoint decided its server is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Located {
    /// Discovery answered.
    Discovered {
        /// The address to connect to.
        address: String,
        /// The instance that answered.
        instance: String,
    },
    /// Discovery answered nothing, and the configured static address is used.
    Fallback {
        /// The address to connect to.
        address: String,
        /// Why the fallback was taken, in words a log can carry.
        because: String,
    },
    /// Discovery was not attempted, and the configured static address is used.
    Configured {
        /// The address to connect to.
        address: String,
    },
}

impl Located {
    /// The address to connect to.
    pub fn address(&self) -> &str {
        match self {
            Located::Discovered { address, .. }
            | Located::Fallback { address, .. }
            | Located::Configured { address } => address,
        }
    }

    /// One line for a status report.
    pub fn line(&self) -> String {
        match self {
            Located::Discovered { address, instance } => format!(
                "server-located how=mdns address={} instance={}",
                address, instance
            ),
            Located::Fallback { address, because } => format!(
                "server-located how=static-fallback address={} because={}",
                address, because
            ),
            Located::Configured { address } => {
                format!("server-located how=configured address={}", address)
            }
        }
    }
}

/// Why an endpoint has no server address at all.
#[derive(Debug)]
pub struct NoServer {
    /// Whether discovery was attempted.
    pub browsed: bool,
    /// What discovery did, if it was attempted.
    pub discovery: Option<String>,
}

impl fmt::Display for NoServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.browsed, &self.discovery) {
            (true, Some(what)) => write!(
                f,
                "this endpoint has no server address: discovery was attempted and {}, and no \
                 static address was configured. It needs one of the two: pass --server \
                 <host:port>, or put a chorus server advertising {} on this link",
                what,
                crate::dnssd::AUDIO_SERVICE
            ),
            _ => write!(
                f,
                "this endpoint has no server address: discovery was not attempted and no static \
                 address was configured. It needs one of the two: pass --server <host:port>, or \
                 pass --discover and put a chorus server advertising {} on this link",
                crate::dnssd::AUDIO_SERVICE
            ),
        }
    }
}

impl std::error::Error for NoServer {}

/// Decide where the server is: discover one, or fall back to the configured
/// static address, or say which of the two is missing.
pub fn locate(
    service: &str,
    discover: Option<Duration>,
    static_address: Option<&str>,
) -> Result<Located, NoServer> {
    let mut what_discovery_did = None;
    if let Some(window) = discover {
        match browse(service, window) {
            Ok(found) => {
                if let Some(address) = found
                    .iter()
                    .find_map(|s| s.socket_address().map(|a| (a, s.instance.clone())))
                {
                    return Ok(Located::Discovered {
                        address: address.0,
                        instance: address.1,
                    });
                }
                what_discovery_did = Some(format!(
                    "returned nothing in {} ms",
                    window.as_millis()
                ));
            }
            Err(e) => {
                what_discovery_did = Some(format!("could not run at all ({})", e));
            }
        }
    }
    match (static_address, what_discovery_did) {
        (Some(address), Some(because)) => Ok(Located::Fallback {
            address: address.to_string(),
            because,
        }),
        (Some(address), None) => Ok(Located::Configured {
            address: address.to_string(),
        }),
        (None, discovery) => Err(NoServer {
            browsed: discover.is_some(),
            discovery,
        }),
    }
}

/// The addresses a host has that are worth advertising.
///
/// Loopback is included on purpose: it is the address every verification in
/// this repository actually uses, and an advertisement that left it out would
/// be an advertisement no local check could follow.
pub fn advertisable_addresses(listen: &str) -> Vec<IpAddr> {
    match listen.parse::<SocketAddr>() {
        Ok(SocketAddr::V4(v4)) if !v4.ip().is_unspecified() => vec![IpAddr::V4(*v4.ip())],
        Ok(SocketAddr::V6(v6)) if !v6.ip().is_unspecified() => vec![IpAddr::V6(*v6.ip())],
        _ => vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_discovery_and_no_static_address_names_both() {
        let err = locate(crate::dnssd::AUDIO_SERVICE, None, None).unwrap_err();
        let text = err.to_string();
        assert!(text.contains("no server address"), "{}", text);
        assert!(text.contains("--server"), "{}", text);
        assert!(text.contains("--discover"), "{}", text);
    }

    #[test]
    fn a_configured_address_with_no_discovery_is_used_as_it_stands() {
        let located = locate(crate::dnssd::AUDIO_SERVICE, None, Some("127.0.0.1:4010")).unwrap();
        assert_eq!(located.address(), "127.0.0.1:4010");
        assert!(located.line().contains("how=configured"), "{}", located.line());
    }

    #[test]
    fn the_address_advertised_for_a_wildcard_listen_is_the_loopback_one() {
        assert_eq!(
            advertisable_addresses("0.0.0.0:4010"),
            vec![IpAddr::V4(Ipv4Addr::LOCALHOST)]
        );
        assert_eq!(
            advertisable_addresses("192.168.1.40:4010"),
            vec![IpAddr::V4(Ipv4Addr::new(192, 168, 1, 40))]
        );
    }
}
