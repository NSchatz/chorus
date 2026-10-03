//! The fetch policy: which addresses chorus will connect to when it is handed
//! a URL (brief section 4.8; proposal P6, "Fetching").
//!
//! `SetAVTransportURI` and a stored alarm stream make the server fetch a URL
//! somebody else chose. Without a rule, that is a way to make the server talk
//! to its own loopback services, to link-local metadata endpoints or to its
//! own control port (server-side request forgery). The rule here is a pure
//! function over the address the name RESOLVED to, and the caller connects to
//! exactly that address, so a name that resolves differently the second time
//! (DNS rebinding) gains nothing.
//!
//! Private ranges (RFC 1918, IPv6 unique local) are allowed on purpose: UPnP
//! control points serve media from the household's own network.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::time::Duration;

use crate::error::FetchError;

/// What a fetch may do and how long it may take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    /// Whether loopback addresses may be fetched. False in production; true
    /// in tests, whose servers live on loopback.
    pub allow_loopback: bool,
    /// Ports that are refused on any address of this machine: the server's
    /// own listeners (control, audio, the renderer's HTTP port).
    pub denied_ports_on_self: Vec<u16>,
    /// How many redirects one request may follow.
    pub max_redirects: u8,
    /// The bound on one TCP connect.
    pub connect_timeout: Duration,
    /// The bound on one read (and one write) on the socket, and on receiving
    /// a whole response head.
    pub read_timeout: Duration,
    /// The most bytes a response's status line and headers may take.
    pub max_header_bytes: usize,
    /// The PEM bundle of trusted roots for https. `None`: the file named by
    /// `SSL_CERT_FILE`, else `/etc/ssl/certs/ca-certificates.crt`.
    pub ca_bundle: Option<PathBuf>,
}

impl Default for Policy {
    /// The production policy with no ports of its own named. Every number is
    /// ASSUMED (not derived from a measurement): 5 redirects, 10 s to connect,
    /// 15 s per read, 32 KiB of headers.
    fn default() -> Self {
        Policy {
            allow_loopback: false,
            denied_ports_on_self: Vec::new(),
            max_redirects: 5,
            connect_timeout: Duration::from_secs(10),
            read_timeout: Duration::from_secs(15),
            max_header_bytes: 32 * 1024,
            ca_bundle: None,
        }
    }
}

/// An IPv4-mapped IPv6 address (`::ffff:a.b.c.d`, RFC 4291 section 2.5.5.2)
/// as the IPv4 address it reaches; anything else unchanged.
pub fn canonical(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => IpAddr::V6(v6),
        },
        v4 => v4,
    }
}

/// The policy's decision about one resolved address.
///
/// `is_own` says whether an address is one of this machine's; it is asked only
/// when `denied_ports_on_self` names the port. Loopback addresses always count
/// as this machine's.
///
/// The rules, in order, each with the name its refusal carries:
///
/// | Refusal | Addresses | Source |
/// |---|---|---|
/// | `unspecified address` | `0.0.0.0/8`, `::` | RFC 1122 section 3.2.1.3, RFC 4291 section 2.5.2 |
/// | `loopback address` | `127.0.0.0/8`, `::1` (unless `allow_loopback`) | RFC 1122, RFC 4291 section 2.5.3 |
/// | `link-local address` | `169.254.0.0/16`, `fe80::/10` | RFC 3927, RFC 4291 section 2.5.6 |
/// | `multicast address` | `224.0.0.0/4`, `ff00::/8` | RFC 5771, RFC 4291 section 2.7 |
/// | `broadcast address` | `255.255.255.255` | RFC 919 |
/// | `the server's own port` | an own address at a denied port | brief 4.8, P6 |
///
/// An IPv4-mapped IPv6 address is judged as the IPv4 address it reaches.
/// (RFC section numbers read 2026-10-03 at <https://www.rfc-editor.org/rfc/>.)
pub fn check_address(
    addr: SocketAddr,
    policy: &Policy,
    is_own: &dyn Fn(IpAddr) -> bool,
) -> Result<(), FetchError> {
    let ip = canonical(addr.ip());
    let refuse = |rule: &str| Err(FetchError::Refused(format!("{rule} {ip}")));
    let loopback = match ip {
        IpAddr::V4(v4) => {
            if v4.octets()[0] == 0 {
                return refuse("unspecified address");
            }
            if v4 == Ipv4Addr::BROADCAST {
                return refuse("broadcast address");
            }
            if v4.is_link_local() {
                return refuse("link-local address");
            }
            if v4.is_multicast() {
                return refuse("multicast address");
            }
            v4.is_loopback()
        }
        IpAddr::V6(v6) => {
            if v6 == Ipv6Addr::UNSPECIFIED {
                return refuse("unspecified address");
            }
            if (v6.segments()[0] & 0xffc0) == 0xfe80 {
                return refuse("link-local address");
            }
            if v6.is_multicast() {
                return refuse("multicast address");
            }
            v6.is_loopback()
        }
    };
    if loopback && !policy.allow_loopback {
        return refuse("loopback address");
    }
    if policy.denied_ports_on_self.contains(&addr.port()) && (loopback || is_own(ip)) {
        return Err(FetchError::Refused(format!(
            "the server's own port {} at {ip}",
            addr.port()
        )));
    }
    Ok(())
}

/// Whether `ip` is an address of this machine: a UDP socket can be bound to
/// it. Needs no interface enumeration (which std does not offer and libc would
/// need `unsafe` for). Where the kernel lets a socket bind to a foreign address
/// (`ip_nonlocal_bind`), every address looks like ours and the denied ports
/// are refused everywhere: the rule fails closed.
pub fn is_own_address(ip: IpAddr) -> bool {
    UdpSocket::bind(SocketAddr::new(ip, 0)).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(allow_loopback: bool, denied: &[u16]) -> Policy {
        Policy {
            allow_loopback,
            denied_ports_on_self: denied.to_vec(),
            ..Policy::default()
        }
    }

    fn verdict(ip: &str, port: u16, policy: &Policy, own: &[&str]) -> Result<(), String> {
        let own: Vec<IpAddr> = own.iter().map(|a| a.parse().unwrap()).collect();
        let addr = SocketAddr::new(ip.parse().unwrap(), port);
        match check_address(addr, policy, &|ip| own.contains(&ip)) {
            Ok(()) => Ok(()),
            Err(FetchError::Refused(rule)) => Err(rule),
            Err(other) => panic!("{ip}: {other:?}"),
        }
    }

    fn refused(ip: &str, policy: &Policy) -> String {
        verdict(ip, 80, policy, &[]).expect_err(ip)
    }

    #[test]
    fn public_and_private_addresses_are_allowed() {
        let p = policy(false, &[4020]);
        for ip in [
            "192.0.2.1",
            "198.51.100.200",
            "203.0.113.9",
            "2001:db8::1",
            "::ffff:192.0.2.1",
            // Unique local (RFC 4193): the IPv6 private range.
            "fd00:db8::1",
            "fec0::1",
        ] {
            assert_eq!(verdict(ip, 80, &p, &[]), Ok(()), "{ip}");
        }
        // The RFC 1918 ranges, built from octets so no private address is written here.
        for octets in [
            [10, 1, 2, 3],
            [172, 16, 0, 1],
            [172, 31, 255, 254],
            [192, 84 * 2, 1, 10],
        ] {
            let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::from(octets)), 8200);
            assert!(check_address(addr, &p, &|_| false).is_ok(), "{octets:?}");
        }
    }

    #[test]
    fn loopback_is_refused_unless_allowed() {
        let p = policy(false, &[]);
        for ip in [
            "127.0.0.1",
            "127.255.255.254",
            "127.1.2.3",
            "::1",
            "::ffff:127.0.0.1",
        ] {
            assert!(refused(ip, &p).starts_with("loopback address"), "{ip}");
        }
        assert_eq!(
            refused("::ffff:127.0.0.1", &p),
            "loopback address 127.0.0.1"
        );
        let p = policy(true, &[]);
        for ip in ["127.0.0.1", "127.9.9.9", "::1", "::ffff:127.0.0.1"] {
            assert_eq!(verdict(ip, 80, &p, &[]), Ok(()), "{ip}");
        }
    }

    #[test]
    fn link_local_is_refused_even_with_loopback_allowed() {
        for p in [policy(false, &[]), policy(true, &[])] {
            for ip in [
                "169.254.0.1",
                "169.254.169.254",
                "169.254.255.255",
                "fe80::1",
                "febf:ffff::1",
                "::ffff:169.254.169.254",
            ] {
                assert!(refused(ip, &p).starts_with("link-local address"), "{ip}");
            }
            // The neighbours of the two ranges are not link-local.
            for ip in ["169.253.255.255", "169.255.0.0", "fe7f::1", "fec0::1"] {
                assert_eq!(verdict(ip, 80, &p, &[]), Ok(()), "{ip}");
            }
        }
    }

    #[test]
    fn unspecified_multicast_and_broadcast_are_refused() {
        for p in [policy(false, &[]), policy(true, &[])] {
            for ip in ["0.0.0.0", "0.1.2.3", "::", "::ffff:0.0.0.0"] {
                assert!(refused(ip, &p).starts_with("unspecified address"), "{ip}");
            }
            for ip in [
                "224.0.0.1",
                "239.255.255.250",
                "ff02::1",
                "ff05::c",
                "::ffff:239.255.255.250",
            ] {
                assert!(refused(ip, &p).starts_with("multicast address"), "{ip}");
            }
            for ip in ["255.255.255.255", "::ffff:255.255.255.255"] {
                assert!(refused(ip, &p).starts_with("broadcast address"), "{ip}");
            }
            assert_eq!(verdict("223.255.255.255", 80, &p, &[]), Ok(()));
        }
    }

    #[test]
    fn the_servers_own_ports_are_refused_on_its_own_addresses_only() {
        let p = policy(true, &[4010, 4020]);
        let own = ["192.0.2.10", "2001:db8::10"];
        for ip in [
            "192.0.2.10",
            "2001:db8::10",
            "::ffff:192.0.2.10",
            "127.0.0.1",
            "::1",
        ] {
            let rule = verdict(ip, 4020, &p, &own).expect_err(ip);
            assert!(rule.starts_with("the server's own port 4020 at "), "{rule}");
            assert_eq!(verdict(ip, 8000, &p, &own), Ok(()), "{ip}: another port");
        }
        // The same port on another machine is somebody else's service.
        assert_eq!(verdict("192.0.2.11", 4020, &p, &own), Ok(()));
        // In production loopback is refused before the port is looked at.
        let p = policy(false, &[4020]);
        assert_eq!(
            verdict("127.0.0.1", 4020, &p, &own),
            Err("loopback address 127.0.0.1".to_string())
        );
        assert!(verdict("192.0.2.10", 4020, &p, &own).is_err());
    }

    #[test]
    fn the_own_address_probe_is_not_asked_without_denied_ports() {
        let p = policy(true, &[]);
        let addr = SocketAddr::new("192.0.2.10".parse().unwrap(), 80);
        assert!(check_address(addr, &p, &|_| panic!("asked")).is_ok());
    }

    #[test]
    fn the_bind_probe_knows_loopback_and_not_a_documentation_address() {
        assert!(is_own_address("127.0.0.1".parse().unwrap()));
        assert!(!is_own_address("192.0.2.1".parse().unwrap()));
    }

    #[test]
    fn the_default_policy_is_the_production_one() {
        let p = Policy::default();
        assert!(!p.allow_loopback);
        assert_eq!(p.max_redirects, 5);
        assert!(p.ca_bundle.is_none());
    }
}
