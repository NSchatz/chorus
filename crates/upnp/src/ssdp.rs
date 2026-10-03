//! Discovery (SSDP): what a renderer says on the multicast group and what it
//! answers to a search, as text, and when to say it, as arithmetic.
//!
//! UDA11 section 1. The socket is the server's; this module builds the
//! datagrams, reads an M-SEARCH, and holds the two pieces of timing that are
//! rules rather than plumbing: the announce schedule and the per-source rate
//! limit on search responses. Time is always an argument (milliseconds from
//! a monotonic clock the caller reads), and so is randomness ([`Jitter`]).

use crate::uuid::{sha1, Uuid};
use crate::{Headers, Service, DEVICE_TYPE};
use std::net::IpAddr;

/// The multicast group and port every SSDP message goes to (UDA11 section
/// 1.2.2: HOST "MUST be 239.255.255.250:1900").
pub const MULTICAST: &str = "239.255.255.250:1900";

/// The advertisement lifetime chorus announces, in seconds (UDA11 section
/// 1.2.2: max-age "SHOULD be greater than or equal to 1800").
pub const MAX_AGE_S: u32 = 1800;

/// How many times the initial alive set and the byebye set are sent (UDA11
/// section 1.2.2: "SHOULD send the entire set of discovery messages more
/// than once" and "SHOULD NOT be sent more than three times"). The rule is
/// written for alive; using it for byebye is chorus's inference: a lost
/// byebye leaves a dead renderer in control points' lists until max-age.
pub const SETS: u8 = 2;

/// The longest a search response may be delayed, in seconds (UDA11 section
/// 1.3.2: MX above 5 is treated as 5).
pub const MX_CAP_S: u64 = 5;

/// The `SERVER` (and `USER-AGENT`) value: three product tokens, the second
/// "MUST be UPnP/1.1" (UDA11 section 1.2.2): `<os>/<os version> UPnP/1.1
/// chorus/<chorus version>`.
pub fn server_token(os: &str, os_version: &str, chorus_version: &str) -> String {
    format!("{os}/{os_version} UPnP/1.1 chorus/{chorus_version}")
}

/// What every discovery message of one root device carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Advert {
    /// The device's UUID; the UDN is `uuid:` plus this.
    pub udn: Uuid,
    /// The absolute URL of the device description, reachable from the
    /// network the message goes to (UDA11 section 1.2.2: LOCATION is a
    /// "Single absolute URL").
    pub location: String,
    /// The `SERVER` value ([`server_token`]).
    pub server: String,
    /// `CACHE-CONTROL: max-age`, in seconds.
    pub max_age_s: u32,
    /// `BOOTID.UPNP.ORG` ([`boot_id`]).
    pub boot_id: u32,
    /// `CONFIGID.UPNP.ORG` ([`config_id`]).
    pub config_id: u32,
    /// `SEARCHPORT.UPNP.ORG`, when unicast searches are answered on a port
    /// other than 1900 (UDA11 section 1.2.2: 49152 to 65535).
    pub search_port: Option<u16>,
    /// Whether the device offers the OpenHome services too
    /// ([`Service::offered`]).
    pub openhome: bool,
}

/// The notification types of a root device with no embedded device and
/// `k` services (UDA11 section 1.2.2, tables 1-1 to 1-3: 3 + 2d + k
/// messages), each with its USN: six for the three AV services, eleven with
/// the five OpenHome ones. `NT` of the second row and the USN prefix "MUST
/// match the value of the UDN element in the device description".
pub fn targets(udn: &Uuid, openhome: bool) -> Vec<(String, String)> {
    let uuid = format!("uuid:{udn}");
    let row = |nt: &str| (nt.to_string(), format!("{uuid}::{nt}"));
    let mut rows = vec![
        row("upnp:rootdevice"),
        (uuid.clone(), uuid.clone()),
        row(DEVICE_TYPE),
    ];
    rows.extend(
        Service::offered(openhome)
            .iter()
            .map(|s| row(s.service_type())),
    );
    rows
}

fn boot_config(out: &mut String, advert: &Advert, search_port: bool) {
    out.push_str(&format!("BOOTID.UPNP.ORG: {}\r\n", advert.boot_id));
    out.push_str(&format!("CONFIGID.UPNP.ORG: {}\r\n", advert.config_id));
    if let (true, Some(port)) = (search_port, advert.search_port) {
        out.push_str(&format!("SEARCHPORT.UPNP.ORG: {port}\r\n"));
    }
    out.push_str("\r\n");
}

/// The `ssdp:alive` set: one datagram per row of [`targets`], always sent whole (UDA11 section
/// 1.2.2: "refreshing or canceling individual messages is PROHIBITED"). The
/// header set and order are the specification's template; there is no body
/// and the blank line after the last header is part of the message.
pub fn alive_set(advert: &Advert) -> Vec<String> {
    targets(&advert.udn, advert.openhome)
        .into_iter()
        .map(|(nt, usn)| {
            let mut m = String::from("NOTIFY * HTTP/1.1\r\n");
            m.push_str(&format!("HOST: {MULTICAST}\r\n"));
            m.push_str(&format!("CACHE-CONTROL: max-age={}\r\n", advert.max_age_s));
            m.push_str(&format!("LOCATION: {}\r\n", advert.location));
            m.push_str(&format!("NT: {nt}\r\n"));
            m.push_str("NTS: ssdp:alive\r\n");
            m.push_str(&format!("SERVER: {}\r\n", advert.server));
            m.push_str(&format!("USN: {usn}\r\n"));
            boot_config(&mut m, advert, true);
            m
        })
        .collect()
}

/// The `ssdp:byebye` set, one message per alive message, with the same
/// BOOTID the alive messages carried (UDA11 section 1.2.3; no LOCATION,
/// CACHE-CONTROL or SERVER).
pub fn byebye_set(advert: &Advert) -> Vec<String> {
    targets(&advert.udn, advert.openhome)
        .into_iter()
        .map(|(nt, usn)| {
            let mut m = String::from("NOTIFY * HTTP/1.1\r\n");
            m.push_str(&format!("HOST: {MULTICAST}\r\n"));
            m.push_str(&format!("NT: {nt}\r\n"));
            m.push_str("NTS: ssdp:byebye\r\n");
            m.push_str(&format!("USN: {usn}\r\n"));
            boot_config(&mut m, advert, false);
            m
        })
        .collect()
}

/// A search worth answering.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Search {
    /// The search target as sent.
    pub st: String,
    /// The window the responses are spread over, in milliseconds: MX
    /// seconds capped at [`MX_CAP_S`]; one second for a unicast search
    /// without MX.
    pub window_ms: u64,
}

/// Why a datagram is not answered. Every case is a silent drop (UDA11
/// section 1.1.5: malformed messages are discarded without a reply).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchDrop {
    /// Not text, or not an `M-SEARCH * HTTP/1.1` request.
    NotASearch,
    /// `MAN` is missing or is not `"ssdp:discover"`.
    WrongMan,
    /// `ST` is missing or empty.
    NoSt,
    /// A multicast search without a usable `MX` (UDA11 section 1.3.2: "the
    /// device MUST silently discard and ignore the search request").
    NoMx,
}

/// Reads an M-SEARCH (UDA11 section 1.3.2). `multicast` says whether the
/// datagram arrived on the group or was sent to the device's own address:
/// a multicast search needs MX; a unicast one has none and is answered
/// within a second. Header names match in any case. `MAN` is accepted with
/// its required double quotes or without them, and nothing else.
pub fn parse_search(datagram: &[u8], multicast: bool) -> Result<Search, SearchDrop> {
    let text = std::str::from_utf8(datagram).map_err(|_| SearchDrop::NotASearch)?;
    let (start, headers) = Headers::parse(text);
    let mut words = start.split_whitespace();
    if (words.next(), words.next(), words.next(), words.next())
        != (Some("M-SEARCH"), Some("*"), Some("HTTP/1.1"), None)
    {
        return Err(SearchDrop::NotASearch);
    }
    let man = headers.get("MAN").ok_or(SearchDrop::WrongMan)?;
    let unquoted = man
        .strip_prefix('"')
        .and_then(|m| m.strip_suffix('"'))
        .unwrap_or(man);
    if unquoted != "ssdp:discover" {
        return Err(SearchDrop::WrongMan);
    }
    let st = headers
        .get("ST")
        .filter(|s| !s.is_empty())
        .ok_or(SearchDrop::NoSt)?;
    let mx = match headers.get("MX") {
        Some(v) => {
            let digits = v.len() <= 9 && !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit());
            if digits {
                v.parse::<u64>().ok()
            } else {
                None
            }
        }
        None => None,
    };
    let window_ms = match (mx, multicast) {
        (Some(mx), _) => mx.min(MX_CAP_S) * 1000,
        (None, true) => return Err(SearchDrop::NoMx),
        // UDA11 section 1.3.3: a unicast search "SHOULD" be answered "within
        // 1 second".
        (None, false) => 1000,
    };
    Ok(Search {
        st: st.to_string(),
        window_ms,
    })
}

/// The rows a search target matches (UDA11 section 1.3.3): every row of
/// [`targets`] for `ssdp:all`, one for `upnp:rootdevice`, for this device's
/// `uuid:`, for the device type and for each service type.
///
/// Versions: "The response MUST specify the same version as was contained in
/// the search request", and a device that has version N of a service answers
/// a search for a lower version of it, since N is a superset. The AV types
/// exist in version 1 only, so only an exact match answers; Product and
/// Volume are announced in version 2, and a search for
/// `urn:av-openhome-org:service:Product:1` is answered with that very URN in
/// `ST` and in the `USN` ([`Service::matching`]; ohNet does the same, ohN
/// `OpenHome/Net/Device/Upnp/DviProtocolUpnp.cpp:685-702`). A search for a
/// higher version than the one announced matches nothing.
pub fn matches(st: &str, udn: &Uuid, openhome: bool) -> Vec<(String, String)> {
    let all = targets(udn, openhome);
    if st == "ssdp:all" {
        return all;
    }
    if let Some((service, version)) = Service::matching(st) {
        if !Service::offered(openhome).contains(&service) {
            return Vec::new();
        }
        let nt = service.service_type_at(version);
        let usn = format!("uuid:{udn}::{nt}");
        return vec![(nt, usn)];
    }
    all.into_iter()
        .filter(|(nt, _)| {
            if nt.starts_with("uuid:") {
                nt.eq_ignore_ascii_case(st)
            } else {
                nt == st
            }
        })
        .collect()
}

/// The search responses of one device for a search: one unicast datagram
/// per matching row, sent "to the source IP address and port that sent the
/// request" (UDA11 section 1.3.3). `date` is the RFC 1123 date
/// ([`http_date`]; DATE is RECOMMENDED there, so `None` leaves it out).
/// `EXT:` is "REQUIRED for backwards compatibility with UPnP 1.0" and has no
/// value.
pub fn search_responses(search: &Search, advert: &Advert, date: Option<&str>) -> Vec<String> {
    matches(&search.st, &advert.udn, advert.openhome)
        .into_iter()
        .map(|(st, usn)| {
            let mut m = String::from("HTTP/1.1 200 OK\r\n");
            m.push_str(&format!("CACHE-CONTROL: max-age={}\r\n", advert.max_age_s));
            if let Some(date) = date {
                m.push_str(&format!("DATE: {date}\r\n"));
            }
            m.push_str("EXT:\r\n");
            m.push_str(&format!("LOCATION: {}\r\n", advert.location));
            m.push_str(&format!("SERVER: {}\r\n", advert.server));
            m.push_str(&format!("ST: {st}\r\n"));
            m.push_str(&format!("USN: {usn}\r\n"));
            boot_config(&mut m, advert, true);
            m
        })
        .collect()
}

/// A source of jitter: uniform integers below a bound. The server seeds one
/// per process; a test seeds one with a constant and gets the same schedule
/// every run.
pub trait Jitter {
    /// A value in `0..bound`; 0 when `bound` is 0.
    fn below(&mut self, bound: u64) -> u64;
}

/// A small deterministic generator (SplitMix64, the public-domain
/// construction by Sebastiano Vigna: ASSUMED from memory, and nothing here
/// depends on its quality beyond spreading delays).
#[derive(Clone, Debug)]
pub struct SeededJitter(u64);

impl SeededJitter {
    /// A generator from a seed.
    pub fn new(seed: u64) -> SeededJitter {
        SeededJitter(seed)
    }
}

impl Jitter for SeededJitter {
    fn below(&mut self, bound: u64) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        if bound == 0 {
            0
        } else {
            z % bound
        }
    }
}

/// When each of `count` search responses goes out, as delays in
/// milliseconds from the search's arrival, ascending: each "a random period
/// of time between 0 seconds and the number of seconds specified in the MX
/// field", multi-part responses "spread at random intervals" over the same
/// window (UDA11 section 1.3.3). A tenth of the window is kept back so the
/// last datagram still arrives inside it.
pub fn response_delays(count: usize, window_ms: u64, jitter: &mut dyn Jitter) -> Vec<u64> {
    let usable = window_ms - window_ms / 10;
    let mut delays: Vec<u64> = (0..count).map(|_| jitter.below(usable)).collect();
    delays.sort_unstable();
    delays
}

/// The announce schedule of one root device (UDA11 section 1.2.2), as a pure
/// state machine: the caller asks [`Announcer::due_ms`] when to wake, calls
/// [`Announcer::poll`] then, and sends the whole alive set when it says so.
///
/// - Before the initial set, "wait a random interval (e.g. between 0 and
///   100milliseconds)".
/// - The initial set is sent [`SETS`] times, "with some delay between sets
///   e.g. a few hundred milliseconds": 200 to 300 ms here.
/// - Then the set is re-sent "at a randomly-distributed interval of less
///   than one-half of the advertisement expiration time": between one third
///   and 29/60 of max-age, which is 600 to 870 s for 1800 s, so that even
///   one lost refresh is followed by another before the advertisement
///   expires.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Announcer {
    max_age_ms: u64,
    initial_left: u8,
    due_ms: u64,
}

impl Announcer {
    /// A schedule for a device that appears at `now_ms`.
    pub fn new(now_ms: u64, max_age_s: u32, jitter: &mut dyn Jitter) -> Announcer {
        Announcer {
            max_age_ms: u64::from(max_age_s) * 1000,
            initial_left: SETS,
            due_ms: now_ms + jitter.below(100),
        }
    }

    /// When the next alive set is due.
    pub fn due_ms(&self) -> u64 {
        self.due_ms
    }

    /// `true` when the alive set is to be sent now; the schedule then moves
    /// to the following set. `false` when nothing is due yet.
    pub fn poll(&mut self, now_ms: u64, jitter: &mut dyn Jitter) -> bool {
        if now_ms < self.due_ms {
            return false;
        }
        self.initial_left = self.initial_left.saturating_sub(1);
        self.due_ms = now_ms
            + if self.initial_left > 0 {
                200 + jitter.below(100)
            } else {
                let low = self.max_age_ms / 3;
                let high = self.max_age_ms * 29 / 60;
                low + jitter.below(high - low)
            };
        true
    }
}

/// A per-source limit on answered searches, so the renderer set cannot be
/// used to amplify traffic at an address (one `ssdp:all` search draws six
/// datagrams per renderer, eleven with the OpenHome services: 96 to 176 from
/// a sixteen-room house, to whatever source
/// address the query claimed). UDA11 does not ask for this; it is chorus's
/// own rule (the limits are chorus's choice). A search over the limit is
/// dropped, which the specification's "silently discard" covers.
#[derive(Clone, Debug)]
pub struct SearchLimiter {
    per_window: u32,
    window_ms: u64,
    max_sources: usize,
    sources: Vec<(IpAddr, u64, u32)>,
}

impl SearchLimiter {
    /// A limiter that answers at most `per_window` searches per source in
    /// any `window_ms`, tracking at most `max_sources` sources at once.
    pub fn new(per_window: u32, window_ms: u64, max_sources: usize) -> SearchLimiter {
        SearchLimiter {
            per_window,
            window_ms,
            max_sources,
            sources: Vec::new(),
        }
    }

    /// The default: 8 searches a second per source, 256 sources. A control
    /// point's discovery burst is three or four searches.
    pub fn standard() -> SearchLimiter {
        SearchLimiter::new(8, 1000, 256)
    }

    /// Whether a search from `source` at `now_ms` is answered; counts it if
    /// so. When the table is full of sources still inside their windows, a
    /// new source is refused: the state stays bounded and the failure is on
    /// the side of saying less.
    pub fn allow(&mut self, source: IpAddr, now_ms: u64) -> bool {
        let window = self.window_ms;
        self.sources
            .retain(|(_, start, _)| now_ms.saturating_sub(*start) < window);
        if let Some(entry) = self.sources.iter_mut().find(|(a, _, _)| *a == source) {
            if entry.2 >= self.per_window {
                return false;
            }
            entry.2 += 1;
            return true;
        }
        if self.sources.len() >= self.max_sources || self.per_window == 0 {
            return false;
        }
        self.sources.push((source, now_ms, 1));
        true
    }
}

/// `CONFIGID.UPNP.ORG` for a configuration: a 24-bit value (UDA11 section
/// 1.2.2 leaves 0 to 16777215 to vendors) taken from the first three octets
/// of the SHA-1 of the bytes that describe it. Two messages with the same
/// value must mean the same description (UDA11 section 1.2.2: "If any part
/// of the configuration changes, the CONFIGID.UPNP.ORG field value MUST be
/// changed"); a hash gives that without state, is the same after a restart,
/// and collides for two different descriptions of one device once in 2^24.
pub fn config_id(description: &[u8]) -> u32 {
    let h = sha1(description);
    (u32::from(h[0]) << 16) | (u32::from(h[1]) << 8) | u32::from(h[2])
}

/// `BOOTID.UPNP.ORG` for a device that (re)appears: it "MUST be increased
/// each time a device (re)joins the network and sends an initial announce"
/// and is a non-negative 31-bit integer (UDA11 section 1.2.2, which suggests
/// seconds since 1970). `unix_s` is the wall clock used as a label only;
/// `last` is the value this UDN last used in this process, if any, so a live
/// group that re-forms within the same second still goes up.
pub fn boot_id(unix_s: u64, last: Option<u32>) -> u32 {
    const MASK: u64 = 0x7FFF_FFFF;
    let now = unix_s & MASK;
    let floor = last.map_or(0, |l| (u64::from(l) + 1) & MASK);
    now.max(floor) as u32
}

/// The RFC 1123 date HTTP uses for `DATE` ("rfc1123-date" as defined in RFC
/// 2616, UDA11 section 1.3.3), for a time in seconds since 1970: for example
/// `Sat, 03 Oct 2026 09:40:00 GMT`. A wall-clock label; nothing is timed by
/// it.
pub fn http_date(unix_s: u64) -> String {
    const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let days = unix_s / 86_400;
    let secs = unix_s % 86_400;
    // Days since 1970-01-01 to a civil date: count from 0000-03-01 in
    // 400-year eras so the leap day is the last day of a year.
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    format!(
        "{}, {:02} {} {} {:02}:{:02}:{:02} GMT",
        DAYS[(days % 7) as usize],
        day,
        MONTHS[(month - 1) as usize],
        year,
        secs / 3600,
        (secs / 60) % 60,
        secs % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn advert() -> Advert {
        Advert {
            udn: Uuid::parse("3b8fa6e6-bb30-5005-b768-3e87f0af9a9a").unwrap(),
            location: "http://192.0.2.10:49200/upnp/3b8fa6e6-bb30-5005-b768-3e87f0af9a9a/desc.xml"
                .into(),
            server: server_token("Linux", "6.12", "0.1.0"),
            max_age_s: MAX_AGE_S,
            boot_id: 1_790_934_000,
            config_id: 1234,
            search_port: None,
            openhome: false,
        }
    }

    #[test]
    fn the_server_token_has_three_products_and_names_upnp_1_1() {
        let s = server_token("Linux", "6.12", "0.1.0");
        assert_eq!(s, "Linux/6.12 UPnP/1.1 chorus/0.1.0");
        assert_eq!(s.split(' ').count(), 3);
    }

    #[test]
    fn the_six_rows_are_the_specifications() {
        let u = "uuid:3b8fa6e6-bb30-5005-b768-3e87f0af9a9a";
        let t = targets(&advert().udn, false);
        let nts: Vec<&str> = t.iter().map(|(nt, _)| nt.as_str()).collect();
        assert_eq!(
            nts,
            [
                "upnp:rootdevice",
                u,
                "urn:schemas-upnp-org:device:MediaRenderer:1",
                "urn:schemas-upnp-org:service:AVTransport:1",
                "urn:schemas-upnp-org:service:RenderingControl:1",
                "urn:schemas-upnp-org:service:ConnectionManager:1",
            ]
        );
        for (i, (nt, usn)) in t.iter().enumerate() {
            if i == 1 {
                assert_eq!(usn, u);
            } else {
                assert_eq!(usn, &format!("{u}::{nt}"));
            }
        }
    }

    #[test]
    fn alive_and_byebye_carry_their_required_headers_and_end_in_a_blank_line() {
        let a = advert();
        let alive = alive_set(&a);
        let bye = byebye_set(&a);
        assert_eq!((alive.len(), bye.len()), (6, 6));
        for m in &alive {
            let (start, h) = Headers::parse(m);
            assert_eq!(start, "NOTIFY * HTTP/1.1");
            assert_eq!(h.get("HOST"), Some("239.255.255.250:1900"));
            assert_eq!(h.get("CACHE-CONTROL"), Some("max-age=1800"));
            assert_eq!(h.get("NTS"), Some("ssdp:alive"));
            assert_eq!(h.get("LOCATION"), Some(a.location.as_str()));
            assert_eq!(h.get("BOOTID.UPNP.ORG"), Some("1790934000"));
            assert_eq!(h.get("CONFIGID.UPNP.ORG"), Some("1234"));
            assert!(h.has("SERVER") && h.has("NT") && h.has("USN"));
            assert!(!h.has("SEARCHPORT.UPNP.ORG"));
            assert!(m.ends_with("\r\n\r\n"));
            assert!(m.len() < 1400, "one datagram");
        }
        for m in &bye {
            let (_, h) = Headers::parse(m);
            assert_eq!(h.get("NTS"), Some("ssdp:byebye"));
            assert_eq!(h.get("BOOTID.UPNP.ORG"), Some("1790934000"));
            assert!(!h.has("LOCATION") && !h.has("CACHE-CONTROL") && !h.has("SERVER"));
            assert!(m.ends_with("\r\n\r\n"));
        }
        let with_port = Advert {
            search_port: Some(49_201),
            openhome: false,
            ..a
        };
        for m in alive_set(&with_port) {
            assert!(m.contains("SEARCHPORT.UPNP.ORG: 49201\r\n\r\n"));
        }
        for m in byebye_set(&with_port) {
            assert!(!m.contains("SEARCHPORT"));
        }
    }

    #[test]
    fn with_openhome_there_are_eleven_rows_and_a_lower_version_is_echoed() {
        let a = Advert {
            openhome: true,
            ..advert()
        };
        assert_eq!(alive_set(&a).len(), 11);
        assert_eq!(byebye_set(&a).len(), 11);
        assert_eq!(&targets(&a.udn, true)[..6], &targets(&a.udn, false)[..]);
        let answer = |st: &str| {
            search_responses(
                &Search {
                    st: st.into(),
                    window_ms: 1000,
                },
                &a,
                None,
            )
        };
        assert_eq!(answer("ssdp:all").len(), 11);
        for s in Service::ALL {
            assert_eq!(answer(s.service_type()).len(), 1, "{s:?}");
        }
        // Product is announced in version 2; a search for version 1 gets one
        // answer that says version 1, in ST and in USN.
        let v1 = "urn:av-openhome-org:service:Product:1";
        let r = answer(v1);
        assert_eq!(r.len(), 1);
        let (_, h) = Headers::parse(&r[0]);
        assert_eq!(h.get("ST"), Some(v1));
        assert_eq!(h.get("USN"), Some(format!("uuid:{}::{v1}", a.udn).as_str()));
        let (_, h2) = Headers::parse(&answer("urn:av-openhome-org:service:Product:2")[0]);
        assert_eq!(h2.get("ST"), Some("urn:av-openhome-org:service:Product:2"));
        // A higher version, and a service chorus leaves out, get nothing.
        for none in [
            "urn:av-openhome-org:service:Product:3",
            "urn:av-openhome-org:service:Playlist:2",
            "urn:av-openhome-org:service:Radio:1",
            "urn:av-openhome-org:service:Transport:1",
            "urn:av-openhome-org:device:Source:1",
        ] {
            assert!(answer(none).is_empty(), "{none}");
        }
    }

    fn search(extra: &str) -> Vec<u8> {
        format!("M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\n{extra}\r\n").into_bytes()
    }

    #[test]
    fn a_search_is_read_and_the_drop_rules_hold() {
        let ok = search("MAN: \"ssdp:discover\"\r\nMX: 2\r\nST: ssdp:all\r\n");
        assert_eq!(
            parse_search(&ok, true),
            Ok(Search {
                st: "ssdp:all".into(),
                window_ms: 2000
            })
        );
        // Header names in any case, the MAN quotes missing.
        let sloppy = search("man: ssdp:discover\r\nmx: 1\r\nst: upnp:rootdevice\r\n");
        assert_eq!(parse_search(&sloppy, true).unwrap().window_ms, 1000);
        // MX above 5 is 5.
        let long = search("MAN: \"ssdp:discover\"\r\nMX: 120\r\nST: ssdp:all\r\n");
        assert_eq!(parse_search(&long, true).unwrap().window_ms, 5000);
        // No MX: dropped on multicast, a second on unicast.
        let no_mx = search("MAN: \"ssdp:discover\"\r\nST: ssdp:all\r\n");
        assert_eq!(parse_search(&no_mx, true), Err(SearchDrop::NoMx));
        assert_eq!(parse_search(&no_mx, false).unwrap().window_ms, 1000);
        let bad_mx = search("MAN: \"ssdp:discover\"\r\nMX: soon\r\nST: ssdp:all\r\n");
        assert_eq!(parse_search(&bad_mx, true), Err(SearchDrop::NoMx));
        // MAN wrong or missing.
        let wrong = search("MAN: \"ssdp:other\"\r\nMX: 1\r\nST: ssdp:all\r\n");
        assert_eq!(parse_search(&wrong, true), Err(SearchDrop::WrongMan));
        let none = search("MX: 1\r\nST: ssdp:all\r\n");
        assert_eq!(parse_search(&none, true), Err(SearchDrop::WrongMan));
        // ST missing.
        let no_st = search("MAN: \"ssdp:discover\"\r\nMX: 1\r\n");
        assert_eq!(parse_search(&no_st, true), Err(SearchDrop::NoSt));
        // Not a search at all.
        for other in [
            &b"NOTIFY * HTTP/1.1\r\n\r\n"[..],
            b"M-SEARCH / HTTP/1.1\r\nMAN: \"ssdp:discover\"\r\nMX: 1\r\nST: ssdp:all\r\n\r\n",
            b"M-SEARCH * HTTP/1.0\r\n\r\n",
            b"\xff\xfe",
            b"",
        ] {
            assert_eq!(parse_search(other, true), Err(SearchDrop::NotASearch));
        }
    }

    #[test]
    fn each_search_target_gets_its_responses() {
        let a = advert();
        let count = |st: &str| {
            search_responses(
                &Search {
                    st: st.into(),
                    window_ms: 1000,
                },
                &a,
                None,
            )
            .len()
        };
        assert_eq!(count("ssdp:all"), 6);
        assert_eq!(count("upnp:rootdevice"), 1);
        assert_eq!(count("uuid:3b8fa6e6-bb30-5005-b768-3e87f0af9a9a"), 1);
        assert_eq!(count("uuid:3B8FA6E6-BB30-5005-B768-3E87F0AF9A9A"), 1);
        assert_eq!(count("uuid:00000000-0000-0000-0000-000000000000"), 0);
        assert_eq!(count("urn:schemas-upnp-org:device:MediaRenderer:1"), 1);
        assert_eq!(count("urn:schemas-upnp-org:device:MediaRenderer:2"), 0);
        assert_eq!(count("urn:schemas-upnp-org:device:MediaServer:1"), 0);
        for s in Service::AV {
            assert_eq!(count(s.service_type()), 1);
        }
        // The OpenHome services are not offered by this device.
        for s in Service::OPENHOME {
            assert_eq!(count(s.service_type()), 0);
        }
        assert_eq!(count("urn:schemas-upnp-org:service:AVTransport:2"), 0);
        assert_eq!(count(""), 0);

        let r = search_responses(
            &Search {
                st: "urn:schemas-upnp-org:device:MediaRenderer:1".into(),
                window_ms: 1000,
            },
            &a,
            Some("Sat, 03 Oct 2026 09:40:00 GMT"),
        );
        let (start, h) = Headers::parse(&r[0]);
        assert_eq!(start, "HTTP/1.1 200 OK");
        assert_eq!(h.get("EXT"), Some(""));
        assert_eq!(h.get("DATE"), Some("Sat, 03 Oct 2026 09:40:00 GMT"));
        assert_eq!(
            h.get("ST"),
            Some("urn:schemas-upnp-org:device:MediaRenderer:1")
        );
        assert_eq!(
            h.get("USN"),
            Some("uuid:3b8fa6e6-bb30-5005-b768-3e87f0af9a9a::urn:schemas-upnp-org:device:MediaRenderer:1")
        );
        assert!(r[0].ends_with("\r\n\r\n"));
    }

    #[test]
    fn responses_are_spread_inside_the_window() {
        let mut j = SeededJitter::new(7);
        let d = response_delays(96, 3000, &mut j);
        assert_eq!(d.len(), 96);
        assert!(d.windows(2).all(|w| w[0] <= w[1]));
        assert!(*d.last().unwrap() < 2700);
        assert!(d.iter().any(|x| *x > 1000), "spread, not bunched at 0");
        assert_eq!(response_delays(3, 0, &mut j), [0, 0, 0]);
        // The same seed gives the same schedule.
        let a = response_delays(8, 1000, &mut SeededJitter::new(1));
        let b = response_delays(8, 1000, &mut SeededJitter::new(1));
        assert_eq!(a, b);
    }

    #[test]
    fn the_initial_set_repeats_and_refreshes_stay_under_half_of_max_age() {
        let mut j = SeededJitter::new(42);
        let mut a = Announcer::new(10_000, 1800, &mut j);
        // The first set after a 0 to 100 ms jitter.
        let first = a.due_ms();
        assert!((10_000..10_100).contains(&first));
        assert!(!a.poll(first - 1, &mut j) || first == 10_000);
        assert!(a.poll(first, &mut j));
        // The repeat a few hundred milliseconds later.
        let second = a.due_ms();
        assert!((first + 200..first + 300).contains(&second));
        assert!(!a.poll(second - 1, &mut j));
        assert!(a.poll(second, &mut j));
        // Then refreshes at 600 to 870 s, each under half of max-age.
        let mut now = second;
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..200 {
            let due = a.due_ms();
            let gap = due - now;
            assert!((600_000..870_000).contains(&gap), "{gap}");
            assert!(gap < 900_000);
            seen.insert(gap);
            assert!(!a.poll(due - 1, &mut j));
            assert!(a.poll(due, &mut j));
            now = due;
        }
        assert!(seen.len() > 100, "the interval is random");
        // A late poll schedules from when it ran, never in the past.
        assert!(a.poll(now + 5_000_000, &mut j));
        assert!(a.due_ms() > now + 5_000_000);
    }

    #[test]
    fn searches_are_rate_limited_per_source() {
        let a = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1));
        let b = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 2));
        let c = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 3));
        let mut l = SearchLimiter::new(3, 1000, 2);
        assert!(l.allow(a, 0) && l.allow(a, 10) && l.allow(a, 20));
        assert!(!l.allow(a, 30), "the fourth in a window is dropped");
        assert!(l.allow(b, 30), "another source has its own count");
        assert!(!l.allow(c, 40), "a full table refuses a new source");
        // The window passes: the source is answered again and the table has
        // room.
        assert!(l.allow(a, 1000));
        assert!(l.allow(c, 1031));
        assert!(!SearchLimiter::new(0, 1000, 4).allow(a, 0));
        let mut s = SearchLimiter::standard();
        let answered = (0..100).filter(|i| s.allow(a, *i)).count();
        assert_eq!(answered, 8);
    }

    #[test]
    fn config_id_is_24_bits_of_the_description() {
        // SHA-1("abc") begins a9 99 3e (RFC 3174 section 7.3).
        assert_eq!(config_id(b"abc"), 0x00a9_993e);
        assert!(config_id(b"anything") <= 0x00ff_ffff);
        assert_ne!(config_id(b"<root>a</root>"), config_id(b"<root>b</root>"));
    }

    #[test]
    fn boot_id_only_goes_up_and_fits_31_bits() {
        assert_eq!(boot_id(1_790_934_000, None), 1_790_934_000);
        assert_eq!(boot_id(1_790_934_000, Some(5)), 1_790_934_000);
        assert_eq!(boot_id(1_790_934_000, Some(1_790_934_000)), 1_790_934_001);
        assert_eq!(boot_id(1_790_934_000, Some(1_790_934_007)), 1_790_934_008);
        assert!(boot_id(u64::MAX, None) <= 0x7FFF_FFFF);
    }

    /// The dates were computed with Python's `email.utils.formatdate` on
    /// 2026-10-03, not with this crate.
    #[test]
    fn http_dates_are_rfc_1123() {
        assert_eq!(http_date(0), "Thu, 01 Jan 1970 00:00:00 GMT");
        assert_eq!(http_date(951_782_400), "Tue, 29 Feb 2000 00:00:00 GMT");
        assert_eq!(http_date(1_790_934_000), "Fri, 02 Oct 2026 09:40:00 GMT");
        assert_eq!(http_date(4_102_444_799), "Thu, 31 Dec 2099 23:59:59 GMT");
    }
}
