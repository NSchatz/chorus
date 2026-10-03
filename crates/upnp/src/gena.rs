//! Eventing (GENA): subscriptions, the callback rule, and event messages.
//!
//! UDA11 section 4. A control point subscribes to a service with a
//! `SUBSCRIBE` naming a callback URL; the renderer then sends `NOTIFY`
//! requests to that URL. That makes a subscription an instruction to open
//! connections to an address the subscriber chose, which is the "CallStranger"
//! weakness (CVE-2020-12695): a device that accepts any callback can be made
//! to send traffic anywhere. [`callback_allowed`] is the answer and is the
//! strictest of three rules at once.
//!
//! Everything here is pure: header fields in, a typed request or an HTTP
//! status out; a table driven by `now_ms`; request text out. Sending is the
//! server's.

use crate::uuid::Uuid;
use crate::xml::escape_text;
use crate::{soap, Headers};
use std::net::{IpAddr, Ipv4Addr};

/// The namespace of the `propertyset` element (UDA11 section 4.3.2).
pub const EVENT_NS: &str = "urn:schemas-upnp-org:event-1-0";

/// How many subscriptions one service of one renderer holds. UDA11 section
/// 4.1.2 lets a publisher refuse with a 5xx status when it "is unable to
/// accept" a subscription; the cap is chorus's own, so a client cannot grow
/// the table without bound.
pub const MAX_SUBSCRIBERS: usize = 16;

/// A `SUBSCRIBE` request, read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Subscribe {
    /// A new subscription (UDA11 section 4.1.2).
    New {
        /// The delivery URLs in the order given, as written between the
        /// angle brackets. Not yet checked: see [`callback_allowed`].
        callbacks: Vec<String>,
        /// The requested duration in seconds, if one was sent.
        timeout_s: Option<u32>,
    },
    /// A renewal (UDA11 section 4.1.3).
    Renew {
        /// The subscription identifier.
        sid: String,
        /// The requested duration in seconds, if one was sent.
        timeout_s: Option<u32>,
    },
}

/// `TIMEOUT: Second-<n>` (UDA11 section 4.1.2). The UPnP 1.0 keyword
/// `infinite` "MUST be silently ignored", and so is anything unreadable: the
/// header is then treated as absent.
fn timeout(headers: &Headers) -> Option<u32> {
    let v = headers.get("TIMEOUT")?;
    let n = v.get(..7).filter(|p| p.eq_ignore_ascii_case("Second-"))?;
    let digits = &v[n.len()..];
    if digits.is_empty() || digits.len() > 9 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// The URLs of a `CALLBACK` value: "One or more URLs each enclosed by angle
/// brackets" (UDA11 section 4.1.2).
fn callbacks(value: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = value;
    while let Some(open) = rest.find('<') {
        let Some(close) = rest[open..].find('>') else {
            break;
        };
        out.push(rest[open + 1..open + close].trim().to_string());
        rest = &rest[open + close + 1..];
    }
    out
}

/// Reads a `SUBSCRIBE`'s header fields, or gives the HTTP status that
/// refuses it (UDA11 sections 4.1.2 and 4.1.3):
///
/// - **400** Incompatible header fields: "An SID header field and one of NT
///   or CALLBACK header fields are present";
/// - **412** Precondition Failed: `CALLBACK` "missing or does not contain a
///   valid HTTP URL", or `NT` is not `upnp:event`, or (renewal) the `SID` is
///   empty. Whether a renewal's SID is known is the table's to say.
pub fn parse_subscribe(headers: &Headers) -> Result<Subscribe, u16> {
    let sid = headers.get("SID");
    let nt = headers.get("NT");
    let callback = headers.get("CALLBACK");
    if sid.is_some() && (nt.is_some() || callback.is_some()) {
        return Err(400);
    }
    if let Some(sid) = sid {
        if sid.is_empty() {
            return Err(412);
        }
        return Ok(Subscribe::Renew {
            sid: sid.to_string(),
            timeout_s: timeout(headers),
        });
    }
    if nt != Some("upnp:event") {
        return Err(412);
    }
    let urls = callbacks(callback.ok_or(412u16)?);
    let has_http = urls.iter().any(|u| {
        u.get(..7)
            .is_some_and(|p| p.eq_ignore_ascii_case("http://"))
    });
    if !has_http {
        return Err(412);
    }
    Ok(Subscribe::New {
        callbacks: urls,
        timeout_s: timeout(headers),
    })
}

/// Reads an `UNSUBSCRIBE`'s header fields into the SID to cancel, or gives
/// the HTTP status that refuses it (UDA11 section 4.1.4): 400 when `SID`
/// comes with `NT` or `CALLBACK`, 412 when there is no `SID`.
pub fn parse_unsubscribe(headers: &Headers) -> Result<String, u16> {
    let sid = headers.get("SID");
    if sid.is_some() && (headers.has("NT") || headers.has("CALLBACK")) {
        return Err(400);
    }
    match sid {
        Some(sid) if !sid.is_empty() => Ok(sid.to_string()),
        _ => Err(412),
    }
}

/// A delivery URL taken apart: an address, a port and a path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallbackUrl {
    /// The host, which is always a literal address here.
    pub addr: IpAddr,
    /// The port; 80 when the URL names none.
    pub port: u16,
    /// The path and query, starting with `/`.
    pub path: String,
}

impl CallbackUrl {
    /// `addr:port`, with brackets around an IPv6 address: the `HOST` value
    /// and what the server connects to.
    pub fn authority(&self) -> String {
        match self.addr {
            IpAddr::V4(a) => format!("{a}:{}", self.port),
            IpAddr::V6(a) => format!("[{a}]:{}", self.port),
        }
    }
}

/// Why a delivery URL is refused. Each is answered with 412 (UDA11 section
/// 4.1.2: no "valid HTTP URL").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallbackRefusal {
    /// Not an `http://` URL (UDA11 section 4.1.2: each URL "MUST be an HTTP
    /// over TCP URL"), or one with user information or port 0.
    NotHttp,
    /// The host is a name, not a literal IP address.
    NotAnAddress,
    /// The host is a loopback address and loopback is not allowed.
    Loopback,
    /// The host is link-local, multicast, broadcast or unspecified.
    NotUnicast,
    /// The host is in none of the allowed subnets.
    OutsideSubnets,
    /// The host is not the address the `SUBSCRIBE` came from.
    NotTheRequester,
}

/// An address prefix, such as `192.0.2.0/24`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cidr {
    addr: IpAddr,
    prefix: u8,
}

fn canonical(addr: IpAddr) -> IpAddr {
    match addr {
        // ::ffff:a.b.c.d is the IPv4 address a.b.c.d.
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(addr, IpAddr::V4),
        v4 => v4,
    }
}

impl Cidr {
    /// A prefix from an address and a length; `None` when the length is
    /// longer than the address.
    pub fn new(addr: IpAddr, prefix: u8) -> Option<Cidr> {
        let max = if addr.is_ipv4() { 32 } else { 128 };
        (prefix <= max).then_some(Cidr { addr, prefix })
    }

    /// Parses `address/length`; a bare address is a prefix of its full
    /// length.
    pub fn parse(text: &str) -> Option<Cidr> {
        let (addr, prefix) = match text.trim().split_once('/') {
            Some((a, p)) => {
                if p.is_empty() || p.len() > 3 || !p.bytes().all(|b| b.is_ascii_digit()) {
                    return None;
                }
                (a.parse::<IpAddr>().ok()?, Some(p.parse::<u8>().ok()?))
            }
            None => (text.trim().parse::<IpAddr>().ok()?, None),
        };
        let full = if addr.is_ipv4() { 32 } else { 128 };
        Cidr::new(addr, prefix.unwrap_or(full))
    }

    /// Whether the prefix holds the address. Addresses of different families
    /// never match.
    pub fn contains(&self, addr: IpAddr) -> bool {
        let bits = |a: IpAddr| -> (u128, u32) {
            match a {
                IpAddr::V4(v4) => (u128::from(u32::from(v4)), 32),
                IpAddr::V6(v6) => (u128::from(v6), 128),
            }
        };
        let (net, width) = bits(self.addr);
        let (host, host_width) = bits(canonical(addr));
        if width != host_width {
            return false;
        }
        let shift = width - u32::from(self.prefix);
        if shift >= 128 {
            return true;
        }
        (net >> shift) == (host >> shift)
    }
}

/// Takes an `http://` URL with a literal address apart. A host *name* is
/// refused: resolving it is a network act this crate does not perform, the
/// answer could differ between the check and each later delivery, and every
/// control point sends its own address anyway.
pub fn parse_callback_url(url: &str) -> Result<CallbackUrl, CallbackRefusal> {
    let rest = url
        .get(..7)
        .filter(|p| p.eq_ignore_ascii_case("http://"))
        .map(|_| &url[7..])
        .ok_or(CallbackRefusal::NotHttp)?;
    let (authority, path) = match rest.find(['/', '?', '#']) {
        Some(at) => (&rest[..at], &rest[at..]),
        None => (rest, ""),
    };
    if authority.is_empty() || authority.contains('@') {
        return Err(CallbackRefusal::NotHttp);
    }
    if path.chars().any(|c| c.is_control() || c == ' ') {
        return Err(CallbackRefusal::NotHttp);
    }
    let (host, port) = if let Some(v6) = authority.strip_prefix('[') {
        let (host, after) = v6.split_once(']').ok_or(CallbackRefusal::NotAnAddress)?;
        let port = match after.strip_prefix(':') {
            Some(p) => Some(p),
            None if after.is_empty() => None,
            None => return Err(CallbackRefusal::NotAnAddress),
        };
        (
            IpAddr::V6(host.parse().map_err(|_| CallbackRefusal::NotAnAddress)?),
            port,
        )
    } else {
        let (host, port) = match authority.rsplit_once(':') {
            Some((h, p)) => (h, Some(p)),
            None => (authority, None),
        };
        (
            IpAddr::V4(host.parse().map_err(|_| CallbackRefusal::NotAnAddress)?),
            port,
        )
    };
    let port = match port {
        None => 80,
        Some(p) => {
            if p.is_empty() || p.len() > 5 || !p.bytes().all(|b| b.is_ascii_digit()) {
                return Err(CallbackRefusal::NotHttp);
            }
            match p.parse::<u16>() {
                Ok(n) if n != 0 => n,
                _ => return Err(CallbackRefusal::NotHttp),
            }
        }
    };
    let path = if path.starts_with('/') {
        path.split('#').next().unwrap_or("/").to_string()
    } else {
        format!("/{}", path.split('#').next().unwrap_or(""))
    };
    Ok(CallbackUrl {
        addr: canonical(host),
        port,
        path,
    })
}

/// Whether events may be delivered to `url`, for a `SUBSCRIBE` that came
/// from `requester`. Three rules, all of which must hold:
///
/// 1. **UDA20 section 4.1** (the CallStranger rule): "The subscription
///    request containing a delivery URL not on the same network segment as
///    the fully qualified event subscription URL shall not be accepted."
/// 2. **P6's stricter list**: the delivery address must lie in one of
///    `subnets`, the household subnets the server is configured with (by
///    default the subnets of its own interfaces, which is rule 1's "same
///    network segment"; UDA20 reads that for private networks as the RFC
///    1918 ranges, and a household's own subnets are inside them). The list
///    is checked rather than the RFC 1918 ranges themselves so that a
///    private address on some *other* network is refused too.
/// 3. **The callback host is the requester**: the delivery address must
///    equal the address the `SUBSCRIBE` came from, so a subscriber can only
///    ever point events at itself. Every control point does this already.
///
/// Before those: only `http://`, only a literal address
/// ([`parse_callback_url`]); never a link-local, multicast, broadcast or
/// unspecified address; a loopback address only when `allow_loopback` is set
/// (tests), in which case rule 2 is not applied to it, and rule 3 still is.
pub fn callback_allowed(
    url: &str,
    requester: IpAddr,
    subnets: &[Cidr],
    allow_loopback: bool,
) -> Result<CallbackUrl, CallbackRefusal> {
    let parsed = parse_callback_url(url)?;
    let addr = parsed.addr;
    let not_unicast = match addr {
        IpAddr::V4(a) => {
            a.is_unspecified() || a.is_multicast() || a.is_broadcast() || a.is_link_local()
        }
        // fe80::/10 is link-local.
        IpAddr::V6(a) => {
            a.is_unspecified() || a.is_multicast() || (a.segments()[0] & 0xffc0) == 0xfe80
        }
    };
    if not_unicast {
        return Err(CallbackRefusal::NotUnicast);
    }
    if addr.is_loopback() {
        if !allow_loopback {
            return Err(CallbackRefusal::Loopback);
        }
    } else if !subnets.iter().any(|s| s.contains(addr)) {
        return Err(CallbackRefusal::OutsideSubnets);
    }
    if addr != canonical(requester) {
        return Err(CallbackRefusal::NotTheRequester);
    }
    Ok(parsed)
}

/// Whether an IPv4 address is in the ranges UDA20 section 4.1 lists for
/// private networks: 10/8, 172.16/12 and 192.168/16. Offered for a server
/// that wants to warn when a configured subnet lies outside them; the
/// callback rule itself checks the configured subnets.
pub fn is_private_v4(addr: Ipv4Addr) -> bool {
    let o = addr.octets();
    o[0] == 10 || (o[0] == 172 && (16..32).contains(&o[1])) || (o[0] == 192 && o[1] == 168)
}

/// A subscription identifier from 16 random octets the caller draws: SID
/// "MUST be universally unique. MUST begin with uuid:" (UDA11 section
/// 4.1.2). A version 4 UUID.
pub fn sid_from_random(bytes: [u8; 16]) -> String {
    format!("uuid:{}", Uuid::from_random(bytes))
}

/// How long subscriptions last. The granted duration is the requested one
/// (or the default) held between `min_s` and `max_s`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimeoutPolicy {
    /// What a request without `TIMEOUT` gets.
    pub default_s: u32,
    /// The shortest duration granted.
    pub min_s: u32,
    /// The longest duration granted.
    pub max_s: u32,
}

impl TimeoutPolicy {
    /// Every subscription lasts 1800 s whatever was asked: UDA11 section
    /// 4.1.2 says the actual duration "SHOULD be greater than or equal to
    /// 1800 seconds", and a bounded one keeps a vanished subscriber's entry
    /// from living for ever.
    pub const STANDARD: TimeoutPolicy = TimeoutPolicy {
        default_s: 1800,
        min_s: 1800,
        max_s: 1800,
    };

    fn grant(&self, requested: Option<u32>) -> u32 {
        requested
            .unwrap_or(self.default_s)
            .max(self.min_s)
            .min(self.max_s.max(self.min_s))
    }
}

/// One subscription.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Subscription {
    /// The identifier, `uuid:...`.
    pub sid: String,
    /// The delivery URLs, tried "in order until one succeeds" (UDA11
    /// section 4.1.2).
    pub callbacks: Vec<CallbackUrl>,
    /// When it expires, on the caller's monotonic clock.
    pub expires_ms: u64,
    next_seq: u32,
}

impl Subscription {
    /// The event key of the next message: "0 for initial event message",
    /// then one more per message, and it "MUST wrap from 4294967295 to 1"
    /// (UDA11 section 4.3.2), never back to 0, so a subscriber can always
    /// tell an initial event.
    fn take_seq(&mut self) -> u32 {
        let seq = self.next_seq;
        self.next_seq = if seq == u32::MAX { 1 } else { seq + 1 };
        seq
    }
}

/// One event message to deliver: who to, with which key, and the body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notify {
    /// The subscription it belongs to.
    pub sid: String,
    /// Its event key.
    pub seq: u32,
    /// The subscription's delivery URLs in order.
    pub callbacks: Vec<CallbackUrl>,
}

impl Notify {
    /// The whole `NOTIFY` request for one of the delivery URLs.
    pub fn request(&self, callback: &CallbackUrl, body: &str) -> String {
        build_notify(callback, &self.sid, self.seq, body)
    }
}

/// Why the table did not take a request; `status()` is the HTTP answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableRefusal {
    /// The service already holds its cap of subscriptions.
    Full,
    /// The SID "does not correspond to a known, un-expired subscription"
    /// (UDA11 section 4.1.3).
    UnknownSid,
}

impl TableRefusal {
    /// 503 for a full table (UDA11 section 4.1.2: a 5xx status when the
    /// publisher cannot accept), 412 for an unknown SID.
    pub fn status(self) -> u16 {
        match self {
            TableRefusal::Full => 503,
            TableRefusal::UnknownSid => 412,
        }
    }
}

/// The subscriptions of one service of one renderer. Expiry is driven by the
/// `now_ms` each call is given (a monotonic clock's reading); nothing here
/// reads a clock.
#[derive(Clone, Debug)]
pub struct Subscriptions {
    cap: usize,
    policy: TimeoutPolicy,
    subs: Vec<Subscription>,
}

impl Subscriptions {
    /// An empty table holding at most `cap` subscriptions.
    pub fn new(cap: usize, policy: TimeoutPolicy) -> Subscriptions {
        Subscriptions {
            cap,
            policy,
            subs: Vec::new(),
        }
    }

    /// An empty table with [`MAX_SUBSCRIBERS`] and the standard durations.
    pub fn standard() -> Subscriptions {
        Subscriptions::new(MAX_SUBSCRIBERS, TimeoutPolicy::STANDARD)
    }

    /// Drops every subscription that has expired by `now_ms` and returns
    /// their SIDs.
    pub fn expire(&mut self, now_ms: u64) -> Vec<String> {
        let (dead, live): (Vec<_>, Vec<_>) =
            self.subs.drain(..).partition(|s| s.expires_ms <= now_ms);
        self.subs = live;
        dead.into_iter().map(|s| s.sid).collect()
    }

    /// Accepts a new subscription with an SID the caller made
    /// ([`sid_from_random`]) and delivery URLs it has checked
    /// ([`callback_allowed`]); returns the granted duration in seconds. The
    /// caller answers 200, flushes the answer, and only then sends the
    /// initial event ([`Subscriptions::initial`]): "The device MUST insure
    /// that the control point has received the response to the subscription
    /// request before sending the initial event message" (UDA11 section
    /// 4.1.2).
    pub fn subscribe(
        &mut self,
        sid: String,
        callbacks: Vec<CallbackUrl>,
        requested_s: Option<u32>,
        now_ms: u64,
    ) -> Result<u32, TableRefusal> {
        self.expire(now_ms);
        if self.subs.len() >= self.cap {
            return Err(TableRefusal::Full);
        }
        let granted = self.policy.grant(requested_s);
        self.subs.push(Subscription {
            sid,
            callbacks,
            expires_ms: now_ms + u64::from(granted) * 1000,
            next_seq: 0,
        });
        Ok(granted)
    }

    /// Renews a subscription (UDA11 section 4.1.3): the SID and the event
    /// key carry on, no initial event is sent, and the duration restarts
    /// from now. Returns the granted duration in seconds.
    pub fn renew(
        &mut self,
        sid: &str,
        requested_s: Option<u32>,
        now_ms: u64,
    ) -> Result<u32, TableRefusal> {
        self.expire(now_ms);
        let granted = self.policy.grant(requested_s);
        let sub = self
            .subs
            .iter_mut()
            .find(|s| s.sid == sid)
            .ok_or(TableRefusal::UnknownSid)?;
        sub.expires_ms = now_ms + u64::from(granted) * 1000;
        Ok(granted)
    }

    /// Cancels a subscription (UDA11 section 4.1.4).
    pub fn unsubscribe(&mut self, sid: &str, now_ms: u64) -> Result<(), TableRefusal> {
        self.expire(now_ms);
        let at = self
            .subs
            .iter()
            .position(|s| s.sid == sid)
            .ok_or(TableRefusal::UnknownSid)?;
        self.subs.remove(at);
        Ok(())
    }

    /// Drops every subscription: a renderer that goes away (byebye) or
    /// re-announces with a new BOOTID has none left (UDA11 section 4.1.1:
    /// subscribers assume theirs cancelled when the BOOTID changes).
    pub fn clear(&mut self) {
        self.subs.clear();
    }

    /// How many subscriptions are held (expired ones included until the next
    /// call that expires them).
    pub fn len(&self) -> usize {
        self.subs.len()
    }

    /// Whether no subscription is held.
    pub fn is_empty(&self) -> bool {
        self.subs.is_empty()
    }

    /// When the earliest subscription expires, for the caller's timer.
    pub fn next_expiry_ms(&self) -> Option<u64> {
        self.subs.iter().map(|s| s.expires_ms).min()
    }

    /// The initial event message of a subscription just accepted: key 0
    /// (UDA11 section 4.3.2). `None` when the SID is not in the table, or
    /// when its initial event was already taken.
    pub fn initial(&mut self, sid: &str) -> Option<Notify> {
        let sub = self.subs.iter_mut().find(|s| s.sid == sid)?;
        if sub.next_seq != 0 {
            return None;
        }
        Some(Notify {
            sid: sub.sid.clone(),
            seq: sub.take_seq(),
            callbacks: sub.callbacks.clone(),
        })
    }

    /// One event message per live subscription, each with its own next key.
    /// A subscription whose initial event has not been taken yet is skipped:
    /// its initial event will carry the current values, and key 0 must come
    /// first. The key advances whether or not delivery later succeeds (UDA11
    /// section 4.3.2: an undeliverable message is abandoned and the
    /// subscription kept; the gap in keys is how a subscriber notices).
    pub fn event(&mut self, now_ms: u64) -> Vec<Notify> {
        self.expire(now_ms);
        self.subs
            .iter_mut()
            .filter(|s| s.next_seq != 0)
            .map(|s| Notify {
                sid: s.sid.clone(),
                seq: s.take_seq(),
                callbacks: s.callbacks.clone(),
            })
            .collect()
    }
}

/// The body of an event message (UDA11 section 4.3.2): one `e:property` per
/// variable, the variable's element unqualified, its value escaped once.
/// For AVTransport and RenderingControl the one property is `LastChange`
/// and its value is an `Event` document ([`crate::lastchange::event_xml`]).
pub fn propertyset(properties: &[(&str, &str)]) -> String {
    let mut x = format!("<?xml version=\"1.0\"?>\n<e:propertyset xmlns:e=\"{EVENT_NS}\">");
    for (name, value) in properties {
        x.push_str(&format!(
            "<e:property><{name}>{}</{name}></e:property>",
            escape_text(value)
        ));
    }
    x.push_str("</e:propertyset>");
    x
}

/// A whole `NOTIFY` request (UDA11 section 4.3.2), header set and order as
/// the specification's template; `CONTENT-LENGTH` is the body's length in
/// bytes.
pub fn build_notify(callback: &CallbackUrl, sid: &str, seq: u32, body: &str) -> String {
    format!(
        "NOTIFY {} HTTP/1.1\r\nHOST: {}\r\nCONTENT-TYPE: {}\r\nNT: upnp:event\r\nNTS: upnp:propchange\r\nSID: {sid}\r\nSEQ: {seq}\r\nCONTENT-LENGTH: {}\r\n\r\n{body}",
        callback.path,
        callback.authority(),
        soap::CONTENT_TYPE,
        body.len()
    )
}

/// The response to an accepted `SUBSCRIBE` or renewal (UDA11 sections 4.1.2
/// and 4.1.3): `SID`, the actual `TIMEOUT`, and `CONTENT-LENGTH: 0`. `date`
/// is an RFC 1123 date ([`crate::ssdp::http_date`]) or `None` to leave DATE
/// out.
pub fn subscribe_response(sid: &str, timeout_s: u32, server: &str, date: Option<&str>) -> String {
    let mut r = String::from("HTTP/1.1 200 OK\r\n");
    if let Some(date) = date {
        r.push_str(&format!("DATE: {date}\r\n"));
    }
    r.push_str(&format!(
        "SERVER: {server}\r\nSID: {sid}\r\nCONTENT-LENGTH: 0\r\nTIMEOUT: Second-{timeout_s}\r\n\r\n"
    ));
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv6Addr;

    fn h(pairs: &[(&str, &str)]) -> Headers {
        Headers::from_pairs(pairs.iter().copied())
    }

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn subscribe_new_renew_and_the_refusals() {
        assert_eq!(
            parse_subscribe(&h(&[
                ("CALLBACK", "<http://192.0.2.50:49152/cb>"),
                ("NT", "upnp:event"),
                ("TIMEOUT", "Second-300"),
            ])),
            Ok(Subscribe::New {
                callbacks: vec!["http://192.0.2.50:49152/cb".into()],
                timeout_s: Some(300),
            })
        );
        // Several URLs, header names in any case, no TIMEOUT.
        assert_eq!(
            parse_subscribe(&h(&[
                ("callback", "<http://192.0.2.50/a> <http://192.0.2.50/b>"),
                ("nt", "upnp:event"),
            ])),
            Ok(Subscribe::New {
                callbacks: vec!["http://192.0.2.50/a".into(), "http://192.0.2.50/b".into()],
                timeout_s: None,
            })
        );
        assert_eq!(
            parse_subscribe(&h(&[("SID", "uuid:abc"), ("TIMEOUT", "Second-1800")])),
            Ok(Subscribe::Renew {
                sid: "uuid:abc".into(),
                timeout_s: Some(1800),
            })
        );
        // 400: SID mixed with NT or CALLBACK.
        assert_eq!(
            parse_subscribe(&h(&[("SID", "uuid:abc"), ("NT", "upnp:event")])),
            Err(400)
        );
        assert_eq!(
            parse_subscribe(&h(&[
                ("SID", "uuid:abc"),
                ("CALLBACK", "<http://192.0.2.50/>")
            ])),
            Err(400)
        );
        // 412: nothing, CALLBACK missing, NT missing or wrong, no http URL.
        assert_eq!(parse_subscribe(&h(&[])), Err(412));
        assert_eq!(parse_subscribe(&h(&[("NT", "upnp:event")])), Err(412));
        assert_eq!(
            parse_subscribe(&h(&[("CALLBACK", "<http://192.0.2.50/>")])),
            Err(412)
        );
        assert_eq!(
            parse_subscribe(&h(&[
                ("CALLBACK", "<http://192.0.2.50/>"),
                ("NT", "upnp:other")
            ])),
            Err(412)
        );
        for bad in [
            "",
            "http://192.0.2.50/",
            "<>",
            "<https://192.0.2.50/>",
            "<ftp://192.0.2.50/>",
            "<http://192.0.2.50/",
        ] {
            assert_eq!(
                parse_subscribe(&h(&[("CALLBACK", bad), ("NT", "upnp:event")])),
                Err(412),
                "{bad}"
            );
        }
        assert_eq!(parse_subscribe(&h(&[("SID", "")])), Err(412));
    }

    #[test]
    fn timeout_values_are_read_or_ignored() {
        let t = |v: &str| timeout(&h(&[("TIMEOUT", v)]));
        assert_eq!(t("Second-1800"), Some(1800));
        assert_eq!(t("second-5"), Some(5));
        assert_eq!(t("Second-infinite"), None);
        assert_eq!(t("infinite"), None);
        assert_eq!(t("Second-"), None);
        assert_eq!(t("Second--5"), None);
        assert_eq!(t("Second-99999999999"), None);
        assert_eq!(t("1800"), None);
        assert_eq!(timeout(&h(&[])), None);
    }

    #[test]
    fn unsubscribe_and_its_refusals() {
        assert_eq!(
            parse_unsubscribe(&h(&[("SID", "uuid:abc")])),
            Ok("uuid:abc".into())
        );
        assert_eq!(parse_unsubscribe(&h(&[])), Err(412));
        assert_eq!(parse_unsubscribe(&h(&[("SID", "")])), Err(412));
        assert_eq!(
            parse_unsubscribe(&h(&[("SID", "uuid:abc"), ("NT", "upnp:event")])),
            Err(400)
        );
        assert_eq!(
            parse_unsubscribe(&h(&[
                ("SID", "uuid:abc"),
                ("CALLBACK", "<http://192.0.2.1/>")
            ])),
            Err(400)
        );
    }

    #[test]
    fn callback_urls_are_taken_apart() {
        let u = parse_callback_url("http://192.0.2.50:49152/cb?x=1").unwrap();
        assert_eq!(
            (u.addr, u.port, u.path.as_str()),
            (ip("192.0.2.50"), 49152, "/cb?x=1")
        );
        assert_eq!(u.authority(), "192.0.2.50:49152");
        let u = parse_callback_url("HTTP://192.0.2.50").unwrap();
        assert_eq!((u.port, u.path.as_str()), (80, "/"));
        let u = parse_callback_url("http://192.0.2.50?q#f").unwrap();
        assert_eq!(u.path, "/?q");
        let u = parse_callback_url("http://[2001:db8::1]:8080/cb").unwrap();
        assert_eq!(u.addr, ip("2001:db8::1"));
        assert_eq!(u.authority(), "[2001:db8::1]:8080");
        let u = parse_callback_url("http://[::ffff:192.0.2.50]/").unwrap();
        assert_eq!(u.addr, ip("192.0.2.50"));
        for (bad, why) in [
            ("https://192.0.2.50/", CallbackRefusal::NotHttp),
            ("ftp://192.0.2.50/", CallbackRefusal::NotHttp),
            ("192.0.2.50/cb", CallbackRefusal::NotHttp),
            ("", CallbackRefusal::NotHttp),
            ("http://", CallbackRefusal::NotHttp),
            ("http://user@192.0.2.50/", CallbackRefusal::NotHttp),
            ("http://192.0.2.50:0/", CallbackRefusal::NotHttp),
            ("http://192.0.2.50:99999/", CallbackRefusal::NotHttp),
            ("http://192.0.2.50:x/", CallbackRefusal::NotHttp),
            ("http://192.0.2.50/a b", CallbackRefusal::NotHttp),
            ("http://192.0.2.50/a\r\nX: y", CallbackRefusal::NotHttp),
            ("http://phone.example/cb", CallbackRefusal::NotAnAddress),
            ("http://localhost/cb", CallbackRefusal::NotAnAddress),
            ("http://192.0.2/cb", CallbackRefusal::NotAnAddress),
            ("http://[2001:db8::1/cb", CallbackRefusal::NotAnAddress),
            ("http://[2001:db8::1]x/cb", CallbackRefusal::NotAnAddress),
            ("http://[nope]/cb", CallbackRefusal::NotAnAddress),
        ] {
            assert_eq!(parse_callback_url(bad), Err(why), "{bad}");
        }
    }

    #[test]
    fn prefixes_hold_their_addresses() {
        let c = Cidr::parse("192.0.2.0/24").unwrap();
        assert!(c.contains(ip("192.0.2.1")) && c.contains(ip("192.0.2.255")));
        assert!(!c.contains(ip("192.0.3.1")));
        assert!(!c.contains(ip("2001:db8::1")));
        assert!(c.contains(ip("::ffff:192.0.2.9")));
        let all = Cidr::parse("0.0.0.0/0").unwrap();
        assert!(all.contains(ip("203.0.113.9")));
        let one = Cidr::parse("198.51.100.7").unwrap();
        assert!(one.contains(ip("198.51.100.7")) && !one.contains(ip("198.51.100.8")));
        let v6 = Cidr::parse("2001:db8::/32").unwrap();
        assert!(v6.contains(ip("2001:db8:1::5")) && !v6.contains(ip("2001:db9::1")));
        assert!(Cidr::parse("::/0").unwrap().contains(ip("2001:db8::1")));
        for bad in [
            "",
            "192.0.2.0/33",
            "192.0.2.0/",
            "192.0.2.0/-1",
            "nope/8",
            "2001:db8::/129",
            "192.0.2.0/1234",
        ] {
            assert_eq!(Cidr::parse(bad), None, "{bad}");
        }
        assert_eq!(Cidr::new(ip("192.0.2.0"), 33), None);
    }

    #[test]
    fn the_callback_rule_refuses_strangers() {
        let subnets = [Cidr::parse("192.0.2.0/24").unwrap()];
        let me = ip("192.0.2.50");
        let ok = callback_allowed("http://192.0.2.50:49152/cb", me, &subnets, false).unwrap();
        assert_eq!(ok.port, 49152);
        let refused = |url: &str, from: IpAddr, loopback: bool| {
            callback_allowed(url, from, &subnets, loopback).unwrap_err()
        };
        // CallStranger: an address off the household subnets, even when the
        // request claims to come from it.
        assert_eq!(
            refused("http://203.0.113.9/cb", ip("203.0.113.9"), false),
            CallbackRefusal::OutsideSubnets
        );
        // On the subnet, but not the requester: a subscriber cannot aim
        // events at a neighbour.
        assert_eq!(
            refused("http://192.0.2.51/cb", me, false),
            CallbackRefusal::NotTheRequester
        );
        // Loopback, unless tests allow it; and then still only the requester.
        assert_eq!(
            refused("http://127.0.0.1:8000/cb", ip("127.0.0.1"), false),
            CallbackRefusal::Loopback
        );
        assert!(callback_allowed("http://127.0.0.1:8000/cb", ip("127.0.0.1"), &[], true).is_ok());
        assert!(callback_allowed("http://[::1]:8000/cb", ip("::1"), &[], true).is_ok());
        assert_eq!(
            refused("http://127.0.0.1:8000/cb", me, true),
            CallbackRefusal::NotTheRequester
        );
        // Allowing loopback does not open the subnets.
        assert_eq!(
            refused("http://203.0.113.9/cb", ip("203.0.113.9"), true),
            CallbackRefusal::OutsideSubnets
        );
        // Never link-local, multicast, broadcast or unspecified.
        for url in [
            "http://169.254.1.1/cb",
            "http://239.255.255.250:1900/cb",
            "http://255.255.255.255/cb",
            "http://0.0.0.0/cb",
            "http://[fe80::1]/cb",
            "http://[ff02::c]/cb",
            "http://[::]/cb",
        ] {
            let all = [
                Cidr::parse("0.0.0.0/0").unwrap(),
                Cidr::parse("::/0").unwrap(),
            ];
            let host = parse_callback_url(url).unwrap().addr;
            assert_eq!(
                callback_allowed(url, host, &all, true),
                Err(CallbackRefusal::NotUnicast),
                "{url}"
            );
        }
        // Not http, or a name.
        assert_eq!(
            refused("https://192.0.2.50/cb", me, false),
            CallbackRefusal::NotHttp
        );
        assert_eq!(
            refused("http://phone.example/cb", me, false),
            CallbackRefusal::NotAnAddress
        );
        // An IPv4-mapped requester is the IPv4 requester.
        assert!(callback_allowed(
            "http://192.0.2.50/cb",
            IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0xffff, 0xc000, 0x0232)),
            &subnets,
            false
        )
        .is_ok());
        // An empty subnet list allows nothing off loopback.
        assert_eq!(
            callback_allowed("http://192.0.2.50/cb", me, &[], false),
            Err(CallbackRefusal::OutsideSubnets)
        );
    }

    #[test]
    fn the_private_ranges_are_uda_2_0s() {
        let p = |n: u32| is_private_v4(Ipv4Addr::from(n));
        assert!(p(0x0A00_0001) && p(0x0AFF_FFFF));
        assert!(p(0xAC10_0001) && p(0xAC1F_FFFF));
        assert!(!p(0xAC0F_FFFF) && !p(0xAC20_0000));
        assert!(p(0xC0A8_0001) && !p(0xC0A9_0001));
        assert!(!p(0xC000_0201) && !p(0x0B00_0001));
    }

    #[test]
    fn sids_are_uuids() {
        assert_eq!(
            sid_from_random([0xab; 16]),
            "uuid:abababab-abab-4bab-abab-abababababab"
        );
    }

    fn cb() -> Vec<CallbackUrl> {
        vec![parse_callback_url("http://192.0.2.50:49152/cb").unwrap()]
    }

    #[test]
    fn the_table_grants_caps_renews_and_expires() {
        let policy = TimeoutPolicy {
            default_s: 300,
            min_s: 2,
            max_s: 600,
        };
        let mut t = Subscriptions::new(2, policy);
        assert!(t.is_empty());
        assert_eq!(t.subscribe("uuid:a".into(), cb(), None, 1000), Ok(300));
        assert_eq!(t.subscribe("uuid:b".into(), cb(), Some(1), 1000), Ok(2));
        assert_eq!(
            t.subscribe("uuid:c".into(), cb(), Some(9999), 1000),
            Err(TableRefusal::Full)
        );
        assert_eq!(TableRefusal::Full.status(), 503);
        assert_eq!(TableRefusal::UnknownSid.status(), 412);
        assert_eq!(t.len(), 2);
        assert_eq!(t.next_expiry_ms(), Some(3000));
        // b expires at 3000; the slot is free again.
        assert_eq!(t.expire(2999), Vec::<String>::new());
        assert_eq!(
            t.subscribe("uuid:c".into(), cb(), Some(9999), 3000),
            Ok(600)
        );
        assert_eq!(t.renew("uuid:b", None, 3000), Err(TableRefusal::UnknownSid));
        // A renewal restarts the duration from now.
        assert_eq!(t.renew("uuid:a", Some(10), 200_000), Ok(10));
        assert_eq!(t.next_expiry_ms(), Some(210_000));
        assert_eq!(t.unsubscribe("uuid:a", 200_000), Ok(()));
        assert_eq!(
            t.unsubscribe("uuid:a", 200_000),
            Err(TableRefusal::UnknownSid)
        );
        assert_eq!(t.expire(u64::MAX), ["uuid:c"]);
        assert!(t.is_empty());
        // The standard policy grants 1800 whatever is asked.
        let mut s = Subscriptions::standard();
        assert_eq!(s.subscribe("uuid:x".into(), cb(), Some(5), 0), Ok(1800));
        assert_eq!(s.renew("uuid:x", Some(999_999), 0), Ok(1800));
        s.clear();
        assert_eq!(s.len(), 0);
    }

    #[test]
    fn keys_start_at_0_go_up_by_1_and_wrap_to_1() {
        let mut t = Subscriptions::standard();
        t.subscribe("uuid:a".into(), cb(), None, 0).unwrap();
        // Before the initial event is taken, no later event is made for it.
        assert!(t.event(0).is_empty());
        let first = t.initial("uuid:a").unwrap();
        assert_eq!(first.seq, 0);
        assert!(t.initial("uuid:a").is_none(), "one initial event");
        assert!(t.initial("uuid:zzz").is_none());
        for expected in 1..=50u32 {
            let n = t.event(0);
            assert_eq!(n.len(), 1);
            assert_eq!(n[0].seq, expected);
        }
        // A second subscriber has its own key; a renewal keeps the first's.
        t.subscribe("uuid:b".into(), cb(), None, 0).unwrap();
        assert_eq!(t.initial("uuid:b").unwrap().seq, 0);
        t.renew("uuid:a", None, 5).unwrap();
        let n = t.event(5);
        assert_eq!(
            n.iter()
                .map(|n| (n.sid.as_str(), n.seq))
                .collect::<Vec<_>>(),
            [("uuid:a", 51), ("uuid:b", 1)]
        );
        // The wrap: 4294967295 is followed by 1, never 0.
        let mut s = Subscription {
            sid: "uuid:w".into(),
            callbacks: cb(),
            expires_ms: 0,
            next_seq: u32::MAX - 1,
        };
        assert_eq!(s.take_seq(), 4_294_967_294);
        assert_eq!(s.take_seq(), 4_294_967_295);
        assert_eq!(s.take_seq(), 1);
        assert_eq!(s.take_seq(), 2);
        // An expired subscription gets no event.
        assert!(t.event(u64::MAX).is_empty());
    }

    #[test]
    fn a_notify_is_the_specifications_request() {
        let body = propertyset(&[("LastChange", "<Event a=\"1\"/>")]);
        assert_eq!(
            body,
            "<?xml version=\"1.0\"?>\n<e:propertyset xmlns:e=\"urn:schemas-upnp-org:event-1-0\"><e:property><LastChange>&lt;Event a=\"1\"/&gt;</LastChange></e:property></e:propertyset>"
        );
        let n = Notify {
            sid: "uuid:abc".into(),
            seq: 7,
            callbacks: cb(),
        };
        let r = n.request(&n.callbacks[0], &body);
        let (head, sent_body) = r.split_once("\r\n\r\n").unwrap();
        let (start, headers) = Headers::parse(head);
        assert_eq!(start, "NOTIFY /cb HTTP/1.1");
        assert_eq!(headers.get("HOST"), Some("192.0.2.50:49152"));
        assert_eq!(
            headers.get("CONTENT-TYPE"),
            Some("text/xml; charset=\"utf-8\"")
        );
        assert_eq!(headers.get("NT"), Some("upnp:event"));
        assert_eq!(headers.get("NTS"), Some("upnp:propchange"));
        assert_eq!(headers.get("SID"), Some("uuid:abc"));
        assert_eq!(headers.get("SEQ"), Some("7"));
        assert_eq!(
            headers.get("CONTENT-LENGTH"),
            Some(body.len().to_string().as_str())
        );
        assert_eq!(sent_body, body);
        // The length is bytes, not characters.
        let utf = build_notify(&n.callbacks[0], "uuid:abc", 0, "\u{e9}");
        assert!(utf.contains("CONTENT-LENGTH: 2\r\n"));
    }

    #[test]
    fn the_subscribe_response_carries_sid_and_timeout() {
        let r = subscribe_response(
            "uuid:abc",
            1800,
            "Linux/6.12 UPnP/1.1 chorus/0.1.0",
            Some("Fri, 02 Oct 2026 09:40:00 GMT"),
        );
        assert_eq!(
            r,
            "HTTP/1.1 200 OK\r\nDATE: Fri, 02 Oct 2026 09:40:00 GMT\r\nSERVER: Linux/6.12 UPnP/1.1 chorus/0.1.0\r\nSID: uuid:abc\r\nCONTENT-LENGTH: 0\r\nTIMEOUT: Second-1800\r\n\r\n"
        );
        assert!(!subscribe_response("uuid:abc", 1, "s", None).contains("DATE"));
    }
}
