//! The UPnP AV media renderers: the sockets, the threads and the glue between
//! `chorus-upnp`'s pure core, the room model and the media players (goal 16;
//! P6, Option A; `docs/upnp.md` is the guide and
//! `docs/decisions/0125-the-upnp-av-media-renderers.md` the record).
//!
//! # What it is
//!
//! Off unless `--upnp` is given. When on, every room, every saved group
//! (active or not) and every live group of the control state is a UPnP AV
//! media renderer: a root device of type `MediaRenderer:1` with AVTransport:1,
//! RenderingControl:1 and ConnectionManager:1, described and controlled on
//! one HTTP port (`--upnp-listen`) and announced over SSDP. A control point
//! that plays a URL on one makes its rooms play it (K78, "take the room"),
//! through one of the server's network media players (`--players`).
//!
//! # The threads, fixed and made at start
//!
//! `4 + W` ordinary threads, created with the rest of the population before
//! the scheduling report and never after, `W` being `--upnp-workers`:
//!
//! - `upnp-ssdp` owns the discovery socket: alive when a renderer appears,
//!   again on the core's announce schedule, byebye when it vanishes and when
//!   the run stops, and the answers to M-SEARCH, each delayed inside its MX
//!   window and limited per source address;
//! - `upnp-acceptor` and `upnp-worker-<i>` serve the HTTP port: one request
//!   per connection, the request bounded in size and time, a connection
//!   arriving with every worker busy turned away with 503. An action never
//!   does network I/O: it changes the AVTransport state machine, sends the
//!   player a command and returns;
//! - `upnp-events` sends the GENA NOTIFY messages, the only outbound
//!   connections this module makes, each under a connect and an I/O timeout;
//! - `upnp-manager` follows the control state's fanout (targets appear,
//!   vanish and are renamed; volumes change) and the players' reports
//!   (opened, started, the gapless boundary, ended, failed), and gives back
//!   players nobody uses. It is its own thread, not part of `upnp-events`,
//!   because that one may be waiting on a subscriber that does not answer,
//!   and a track boundary must not wait behind it.
//!
//! # What is shared
//!
//! One table of renderers behind one mutex ([`Renderers`]). Whoever holds it
//! does no network I/O: the workers read a whole request before taking it and
//! write the response after letting go, the event thread takes what is due
//! and delivers with the lock released.
//!
//! Control code: it touches no PCM and is never called from the audio thread.
//! Its timers are monotonic. The wall clock is read for two labels the
//! specifications ask for, the `DATE` header and the `BOOTID.UPNP.ORG`
//! number, and for nothing else; `audio-path.conf` records the unit as
//! excluded.

use std::collections::{BTreeMap, VecDeque};
use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chorus_control::json::{self, Value};
use chorus_hostctl::ThreadRegistry;
use chorus_upnp::avtransport::{AvTransport, Effect, TransportState};
use chorus_upnp::description::{self, DeviceInfo, Resource};
use chorus_upnp::gena::{self, CallbackUrl, Cidr, Subscribe, Subscriptions};
use chorus_upnp::lastchange::{self, AVT_NS, RCS_NS};
use chorus_upnp::rendering::{self, RenderingControl};
use chorus_upnp::soap::{self, Invocation};
use chorus_upnp::ssdp::{self, Advert, Announcer, SearchLimiter, SeededJitter};
use chorus_upnp::uuid::{udn, Target, Uuid, CHORUS_NAMESPACE};
use chorus_upnp::{connmgr, didl, error, Headers, Outputs, Service, UpnpError};

use crate::config::UpnpFlags;
use crate::control::ControlState;
use crate::hostreport::register_ordinary_thread;
use crate::mediaplayer::{Action, Event, PlayerHandle, PlayerReport, Players};
use crate::player::player_id;
use crate::playersessions::{Metadata, PlayRequest, PlayerSessions};

/// The renderers' HTTP port when `--upnp-listen` is not given. The next in
/// chorus's own family after the audio port 4010 and the control port 4020,
/// and below Linux's default range for ephemeral ports (32768 to 60999 on the
/// development host, `/proc/sys/net/ipv4/ip_local_port_range`, read
/// 2026-10-03), so an outbound connection of some other program is not handed
/// it first. The host firewall is opened for this port by the owner.
pub const DEFAULT_HTTP_PORT: u16 = 4030;

/// The HTTP workers when `--upnp-workers` is not given. ASSUMED: 4; a control
/// point makes short requests one after another, and a request is answered
/// without waiting on anything.
pub const DEFAULT_WORKERS: usize = 4;

/// The most HTTP workers `--upnp-workers` takes.
pub const MAX_WORKERS: usize = 16;

/// The SSDP port (UDA11 section 1.2: 239.255.255.250:1900).
pub const SSDP_PORT: u16 = 1900;

/// The multicast TTL of discovery messages (UDA11 section 1.2.2: "SHOULD
/// default to 2").
const SSDP_TTL: u32 = 2;

/// The largest request head read, bytes.
const MAX_HEAD_BYTES: usize = 16 * 1024;

/// The largest request body read, bytes (a SOAP request carrying two
/// DIDL-Lite documents is a few kilobytes). Larger is answered 413.
const MAX_BODY_BYTES: usize = 64 * 1024;

/// How long a whole request may take to arrive.
const REQUEST_DEADLINE: Duration = Duration::from_secs(5);

/// How long one write of a response may take.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);

/// How long connecting to a subscriber may take. UDA11 section 4.3.2 allows
/// a subscriber 30 seconds; chorus gives up sooner (the specification's
/// "SHOULD abandon sending this message"), because one thread delivers to
/// every subscriber and a dead one must cost the others little.
const NOTIFY_CONNECT_TIMEOUT: Duration = Duration::from_secs(1);

/// How long writing one NOTIFY, and reading its status line, may take.
const NOTIFY_IO_TIMEOUT: Duration = Duration::from_secs(2);

/// After a delivery fails, the subscriber's messages are abandoned for this
/// long without trying (its subscription is kept, UDA11 section 4.3.2), ms.
const SUSPECT_FOR_MS: u64 = 30_000;

/// Event messages queued for one subscriber at most; past it the oldest is
/// abandoned.
const NOTIFY_QUEUE: usize = 64;

/// How long a renderer that is STOPPED keeps the player it loaded, ms. A
/// control point often sends Stop, SetAVTransportURI and Play one after
/// another, and a player that was given back would have to be unloaded
/// first; after this long with nothing played the player goes back to the
/// pool, and a later Play loads the URI again. ASSUMED: 30 s.
const STOPPED_HOLD_MS: u64 = 30_000;

/// How often the manager and the event thread look up when nothing woke
/// them.
const POLL: Duration = Duration::from_millis(20);

/// How long the supervisor waits for the byebye messages when the run ends.
pub const FAREWELL_WAIT: Duration = Duration::from_secs(2);

/// Discovery datagrams waiting to be sent at most; past it a search is not
/// answered.
const OUTBOX_LIMIT: usize = 8192;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn unix_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn http_date() -> String {
    ssdp::http_date(unix_s())
}

/// The IPv4 subnets this host is directly attached to, from the kernel's
/// routing table (`/proc/net/route`: the routes with no gateway), without
/// the loopback interface and the link-local range: the default for
/// `--upnp-callback-subnet`. `std` cannot list interfaces, and this file is
/// what Linux offers without `unsafe`. Empty when it cannot be read, which
/// refuses every callback that is not on loopback until the owner names a
/// subnet.
pub fn on_link_subnets() -> Vec<Cidr> {
    let Ok(table) = std::fs::read_to_string("/proc/net/route") else {
        return Vec::new();
    };
    subnets_of_route_table(&table)
}

fn subnets_of_route_table(table: &str) -> Vec<Cidr> {
    let mut out: Vec<Cidr> = Vec::new();
    for line in table.lines().skip(1) {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 8 || f[0] == "lo" {
            continue;
        }
        let hex = |s: &str| u32::from_str_radix(s, 16).ok();
        let (Some(dest), Some(gateway), Some(mask)) = (hex(f[1]), hex(f[2]), hex(f[7])) else {
            continue;
        };
        if gateway != 0 || mask == 0 {
            continue;
        }
        // The file prints each address as the number its four octets make in
        // the host's own byte order.
        let addr = Ipv4Addr::from(dest.to_ne_bytes());
        let prefix = u32::from_be_bytes(mask.to_ne_bytes()).leading_ones() as u8;
        if addr.is_link_local() || addr.is_loopback() {
            continue;
        }
        if let Some(cidr) = Cidr::new(IpAddr::V4(addr), prefix) {
            if !out.contains(&cidr) {
                out.push(cidr);
            }
        }
    }
    out
}

/// Whether an event callback may point at a loopback address: only when the
/// renderers' own HTTP listener is on loopback, which is how tests and a
/// developer's machine run them. A listener on any other address (every
/// deployment) refuses loopback callbacks, as P6's rule says: a subscriber
/// must never be able to aim this server's requests at the server's own
/// host.
pub fn loopback_callbacks_allowed(listener: &SocketAddr) -> bool {
    listener.ip().is_loopback()
}

/// The sockets of the renderers, bound before any thread exists so that a
/// port somebody else holds is a refusal to start, by name.
pub struct Sockets {
    listener: TcpListener,
    http: SocketAddr,
    ssdp: UdpSocket,
    ssdp_port: u16,
    group: SocketAddrV4,
}

impl Sockets {
    /// Bind the HTTP listener and the discovery socket `flags` name.
    ///
    /// The discovery socket binds the unspecified address, which is what
    /// receives multicast, unless the HTTP address is a loopback one (tests):
    /// then it binds that address and the outside network never reaches it.
    /// `std` binds without `SO_REUSEADDR`, so on a host where another program
    /// holds UDP port 1900 this fails, and says so (`docs/upnp.md`, "A host
    /// that already runs an SSDP program").
    pub fn bind(flags: &UpnpFlags) -> Result<Sockets, String> {
        let listener = TcpListener::bind(&flags.listen)
            .map_err(|e| format!("--upnp-listen {}: {}", flags.listen, e))?;
        let http = listener
            .local_addr()
            .map_err(|e| format!("--upnp-listen {}: {}", flags.listen, e))?;
        let group = match &flags.ssdp_group {
            None => SocketAddrV4::new(Ipv4Addr::new(239, 255, 255, 250), SSDP_PORT),
            Some(text) => text
                .parse()
                .map_err(|_| format!("--upnp-ssdp-group {}: not an IPv4 address:port", text))?,
        };
        let bind_ip = match http.ip() {
            IpAddr::V4(ip) if ip.is_loopback() => ip,
            _ => Ipv4Addr::UNSPECIFIED,
        };
        let ssdp = UdpSocket::bind(SocketAddrV4::new(bind_ip, flags.ssdp_port)).map_err(|e| {
            format!(
                "the SSDP socket, UDP port {}: {} (std binds without SO_REUSEADDR: if another \
                 SSDP program on this host holds the port, see docs/upnp.md)",
                flags.ssdp_port, e
            )
        })?;
        let ssdp_port = ssdp.local_addr().map_or(flags.ssdp_port, |a| a.port());
        if group.ip().is_multicast() {
            let interface = match http.ip() {
                IpAddr::V4(ip) if !ip.is_unspecified() => ip,
                _ => Ipv4Addr::UNSPECIFIED,
            };
            ssdp.join_multicast_v4(group.ip(), &interface)
                .map_err(|e| format!("joining the SSDP group {}: {}", group.ip(), e))?;
            let _ = ssdp.set_multicast_ttl_v4(SSDP_TTL);
        }
        Ok(Sockets {
            listener,
            http,
            ssdp,
            ssdp_port,
            group,
        })
    }

    /// The HTTP port that was bound: one more of the server's own listeners
    /// for the players' fetch policy.
    pub fn http_port(&self) -> u16 {
        self.http.port()
    }
}

// ----- the control state, read ------------------------------------------------

/// What a target is.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Kind {
    Room,
    Saved,
    Live,
}

/// One target as the control state has it now.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Spec {
    /// `room:<id>`, `group:<id>` or `live:<members>`: the name the UDN is
    /// made of, and the owner name in the player pool.
    key: String,
    kind: Kind,
    /// The friendlyName.
    name: String,
    /// The member rooms.
    rooms: Vec<String>,
    /// What a `take` names to make this target's rooms play together.
    take: String,
    /// The formed group to set the group volume of, when the target's rooms
    /// are one formed group now (a saved group that is not formed has none).
    group: Option<String>,
    /// Volume in thousandths and mute, as the control state holds them.
    volume: u16,
    mute: bool,
}

fn thousandths(v: Option<&Value>) -> u16 {
    v.and_then(Value::as_num)
        .and_then(|n| n.parse::<f64>().ok())
        .map_or(0, |f| (f * 1000.0).round().clamp(0.0, 1000.0) as u16)
}

fn strings(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Arr(items)) => items
            .iter()
            .filter_map(|i| i.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

fn items<'a>(state: &'a Value, key: &str) -> &'a [Value] {
    match state.get(key) {
        Some(Value::Arr(items)) => items,
        _ => &[],
    }
}

/// The state's serial and its targets: one per room, per saved group and per
/// live group.
fn specs_of(state: &str) -> Option<(i64, Vec<Spec>)> {
    let state = json::parse(state).ok()?;
    let serial = state
        .get("serial")
        .and_then(Value::as_num)
        .and_then(|n| n.parse().ok())?;
    let text = |v: &Value, key: &str| v.get(key).and_then(Value::as_str).map(str::to_string);
    // id -> (name, volume, muted)
    let mut rooms: BTreeMap<String, (String, u16, bool)> = BTreeMap::new();
    let mut specs = Vec::new();
    for zone in items(&state, "zones") {
        let id = text(zone, "id")?;
        let name = text(zone, "name").unwrap_or_else(|| id.clone());
        let volume = thousandths(zone.get("volume"));
        let mute = zone.get("muted").and_then(Value::as_bool).unwrap_or(false);
        rooms.insert(id.clone(), (name.clone(), volume, mute));
        specs.push(Spec {
            key: Target::Room(&id).name(),
            kind: Kind::Room,
            name,
            rooms: vec![id.clone()],
            take: id,
            group: None,
            volume,
            mute,
        });
    }
    let all_muted = |members: &[String]| {
        !members.is_empty()
            && members
                .iter()
                .all(|m| rooms.get(m).is_some_and(|(_, _, muted)| *muted))
    };
    // id -> (kind, members, volume) of every formed group.
    let mut formed: BTreeMap<String, (String, Vec<String>, u16)> = BTreeMap::new();
    for group in items(&state, "groups") {
        let id = text(group, "id")?;
        formed.insert(
            id,
            (
                text(group, "kind").unwrap_or_default(),
                strings(group.get("zones")),
                thousandths(group.get("volume")),
            ),
        );
    }
    for saved in items(&state, "saved_groups") {
        let id = text(saved, "id")?;
        let members = strings(saved.get("zones"));
        let (group, volume, mute) = match formed.get(&id) {
            Some((_, in_it, volume)) => (Some(id.clone()), *volume, all_muted(in_it)),
            None => {
                // Not formed: the average of its rooms, as the group volume
                // would be (rounded half up).
                let sum: u32 = members
                    .iter()
                    .filter_map(|m| rooms.get(m).map(|(_, v, _)| u32::from(*v)))
                    .sum();
                let n = members.len().max(1) as u32;
                (None, ((2 * sum + n) / (2 * n)) as u16, all_muted(&members))
            }
        };
        specs.push(Spec {
            key: Target::Group(&id).name(),
            kind: Kind::Saved,
            name: text(saved, "name").unwrap_or_else(|| id.clone()),
            rooms: members,
            take: id,
            group,
            volume,
            mute,
        });
    }
    for (id, (kind, members, volume)) in &formed {
        if kind != "live" {
            continue;
        }
        let mut sorted: Vec<&str> = members.iter().map(String::as_str).collect();
        sorted.sort_unstable();
        sorted.dedup();
        // A live group is called by its rooms: their display names in the
        // order of their ids, joined by " + ".
        let name = sorted
            .iter()
            .map(|m| rooms.get(*m).map_or(*m, |(name, _, _)| name.as_str()))
            .collect::<Vec<_>>()
            .join(" + ");
        specs.push(Spec {
            key: Target::Live(&sorted).name(),
            kind: Kind::Live,
            name,
            rooms: members.clone(),
            take: id.clone(),
            group: Some(id.clone()),
            volume: *volume,
            mute: all_muted(members),
        });
    }
    Some((serial, specs))
}

// ----- one renderer -----------------------------------------------------------

struct Renderer {
    spec: Spec,
    udn: Uuid,
    config_id: u32,
    boot_id: u32,
    description: String,
    avt: AvTransport,
    rcs: RenderingControl,
    /// The subscriptions of AVTransport, RenderingControl and
    /// ConnectionManager, in [`Service::ALL`]'s order.
    subs: [Subscriptions; 3],
    /// The player this renderer holds.
    player: Option<usize>,
    /// Whether the player has the current URI loaded.
    loaded: bool,
    /// The upper half of the epochs this renderer's actions carry: a new one
    /// per player taken, so a report of another holder of the same player is
    /// never taken for this one's.
    base: u64,
    /// When the renderer last loaded or stopped, for [`STOPPED_HOLD_MS`].
    idle_since_ms: u64,
    /// The next URIs handed to the player, with their metadata, newest last.
    queued: Vec<(String, String)>,
}

fn service_index(service: Service) -> usize {
    match service {
        Service::AvTransport => 0,
        Service::RenderingControl => 1,
        Service::ConnectionManager => 2,
    }
}

fn model_name(kind: &Kind) -> &'static str {
    match kind {
        Kind::Room => "chorus room",
        Kind::Saved => "chorus saved group",
        Kind::Live => "chorus live group",
    }
}

/// What the control point's DIDL-Lite says about `uri`: the hints for the
/// now-playing record, and the media type of its `res`.
fn hints_of(uri: &str, metadata: &str) -> (Metadata, Option<String>) {
    let Some(parsed) = didl::parse(metadata) else {
        return (Metadata::default(), None);
    };
    let res = parsed.resource_for(uri);
    let mime = res
        .and_then(|r| r.protocol_info.as_ref())
        .map(|p| p.content_format.trim().to_string())
        .filter(|m| !m.is_empty() && m != "*");
    (
        Metadata {
            title: parsed.title.clone(),
            artist: parsed.artist.clone(),
            album: parsed.album.clone(),
            art_url: parsed.album_art_uri.clone(),
            duration_ms: res.and_then(|r| r.duration_ms),
        },
        mime,
    )
}

/// One event message waiting to be delivered.
struct Job {
    seq: u32,
    callbacks: Vec<CallbackUrl>,
    body: Arc<String>,
    /// The device and service it belongs to, to drop the subscription when
    /// the subscriber says it has forgotten it.
    udn: String,
    service: Service,
}

#[derive(Default)]
struct SubQueue {
    jobs: VecDeque<Job>,
    /// Until when deliveries are not tried (a delivery failed).
    suspect_until_ms: Option<u64>,
}

#[derive(Default)]
struct Inner {
    /// By the UDN's text.
    renderers: BTreeMap<String, Renderer>,
    /// The BOOTID each UDN last used in this process.
    last_boot: BTreeMap<String, u32>,
    /// The serial of the control state the table was last made from.
    serial: i64,
    /// By SID.
    queues: BTreeMap<String, SubQueue>,
}

/// What the renderers are configured with.
pub struct Settings {
    /// The HTTP listener's address as bound.
    pub http: SocketAddr,
    /// Where event callbacks may point (`--upnp-callback-subnet`, else the
    /// host's own subnets).
    pub subnets: Vec<Cidr>,
    /// The stable identity the UDNs are derived from: the fingerprint of the
    /// server's persisted public key.
    pub server_id: String,
}

/// The renderers of a server: the table, and everything the threads share.
pub struct Renderers {
    settings: Settings,
    state: Arc<ControlState>,
    players: Arc<Players>,
    sessions: Arc<PlayerSessions>,
    inner: Mutex<Inner>,
    started: Instant,
    next_base: AtomicU64,
    server: String,
    wake_events: SyncSender<()>,
    log: Box<dyn Fn(&str) + Send + Sync>,
}

/// What an HTTP worker answers with.
struct Reply {
    status: u16,
    reason: &'static str,
    headers: Vec<(&'static str, String)>,
    body: String,
    /// The answer to a HEAD: the headers a GET would have, and no body.
    head_only: bool,
}

impl Reply {
    fn empty(status: u16, reason: &'static str) -> Reply {
        Reply {
            status,
            reason,
            headers: Vec::new(),
            body: String::new(),
            head_only: false,
        }
    }

    fn xml(status: u16, reason: &'static str, body: String) -> Reply {
        Reply {
            status,
            reason,
            headers: vec![("CONTENT-TYPE", soap::CONTENT_TYPE.to_string())],
            body,
            head_only: false,
        }
    }
}

fn reason_of(status: u16) -> &'static str {
    match status {
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        412 => "Precondition Failed",
        413 => "Content Too Large",
        415 => "Unsupported Media Type",
        431 => "Request Header Fields Too Large",
        501 => "Not Implemented",
        503 => "Service Unavailable",
        _ => "Internal Server Error",
    }
}

impl Renderers {
    fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    fn allow_loopback(&self) -> bool {
        loopback_callbacks_allowed(&self.settings.http)
    }

    fn wake_events(&self) {
        let _ = self.wake_events.try_send(());
    }

    fn handle_of(&self, r: &Renderer) -> Option<PlayerHandle> {
        r.player.and_then(|i| self.players.handle(i)).cloned()
    }

    fn tag(r: &Renderer) -> u64 {
        (r.base << 32) | (r.avt.epoch() & 0xffff_ffff)
    }

    /// Unload and give back the renderer's player, ending its session.
    fn unload(&self, r: &mut Renderer) {
        let Some(index) = r.player.take() else {
            return;
        };
        r.loaded = false;
        r.queued.clear();
        if self.sessions.in_session(&r.spec.key) {
            // Ends the session: the group's source becomes `none`, the
            // player is unloaded and given back.
            self.sessions.stop(&r.spec.key);
            return;
        }
        if self.players.owner_of(index).as_deref() == Some(r.spec.key.as_str()) {
            if let Some(handle) = self.players.handle(index) {
                handle.send(u64::MAX, Action::Unload);
            }
            self.players.release(index);
        }
    }

    // ----- the target table -----

    /// Bring the table to `state`: a target that is new gets a renderer, one
    /// that is gone loses it, one whose name changed is described anew (the
    /// discovery thread says byebye and alive when it sees the new BOOTID),
    /// and every renderer is told its real volume and mute. A state older
    /// than the one the table was made from is ignored.
    fn sync(&self, inner: &mut Inner, state: &str) {
        let Some((serial, specs)) = specs_of(state) else {
            (self.log)("upnp state not followed reason=\"the control state did not parse\"");
            return;
        };
        if serial < inner.serial {
            return;
        }
        inner.serial = serial;
        let now = self.now_ms();
        let mut seen: Vec<String> = Vec::with_capacity(specs.len());
        for spec in specs {
            let target = match spec.kind {
                Kind::Room => udn(
                    &CHORUS_NAMESPACE,
                    &self.settings.server_id,
                    &Target::Room(&spec.take),
                ),
                Kind::Saved => udn(
                    &CHORUS_NAMESPACE,
                    &self.settings.server_id,
                    &Target::Group(&spec.take),
                ),
                Kind::Live => {
                    let members: Vec<&str> = spec.rooms.iter().map(String::as_str).collect();
                    udn(
                        &CHORUS_NAMESPACE,
                        &self.settings.server_id,
                        &Target::Live(&members),
                    )
                }
            };
            let key = target.to_string();
            seen.push(key.clone());
            let info = DeviceInfo {
                udn: target,
                friendly_name: spec.name.clone(),
                model_name: model_name(&spec.kind).to_string(),
                model_number: env!("CARGO_PKG_VERSION").to_string(),
            };
            let config_id = description::device_config_id(&info);
            if let Some(r) = inner.renderers.get_mut(&key) {
                if r.config_id != config_id {
                    // A rename: the description changed, so the device goes
                    // (byebye) and comes back (alive) with a new CONFIGID and
                    // a larger BOOTID, and its subscriptions end (UDA11
                    // sections 1.2.2 and 4.1.1). What it plays is untouched.
                    r.boot_id = ssdp::boot_id(unix_s(), Some(r.boot_id));
                    inner.last_boot.insert(key.clone(), r.boot_id);
                    r.config_id = config_id;
                    r.description = description::device_description(&info, config_id);
                    for subs in &mut r.subs {
                        subs.clear();
                    }
                    (self.log)(&format!(
                        "upnp renderer renamed target={} udn={} name=\"{}\" bootid={} configid={}",
                        spec.key, key, spec.name, r.boot_id, config_id
                    ));
                }
                r.rcs.report(spec.volume, spec.mute);
                r.spec = spec;
                continue;
            }
            let boot_id = ssdp::boot_id(unix_s(), inner.last_boot.get(&key).copied());
            inner.last_boot.insert(key.clone(), boot_id);
            (self.log)(&format!(
                "upnp renderer appeared target={} udn={} name=\"{}\" bootid={} configid={}",
                spec.key, key, spec.name, boot_id, config_id
            ));
            inner.renderers.insert(
                key,
                Renderer {
                    udn: target,
                    config_id,
                    boot_id,
                    description: description::device_description(&info, config_id),
                    avt: AvTransport::new(),
                    rcs: RenderingControl::new(spec.volume, spec.mute),
                    subs: [
                        Subscriptions::standard(),
                        Subscriptions::standard(),
                        Subscriptions::standard(),
                    ],
                    player: None,
                    loaded: false,
                    base: 0,
                    idle_since_ms: now,
                    queued: Vec::new(),
                    spec,
                },
            );
        }
        let gone: Vec<String> = inner
            .renderers
            .keys()
            .filter(|k| !seen.contains(k))
            .cloned()
            .collect();
        for key in gone {
            if let Some(mut r) = inner.renderers.remove(&key) {
                // A target that is gone stops what it was playing: there is
                // no renderer left to control it with.
                self.unload(&mut r);
                (self.log)(&format!(
                    "upnp renderer vanished target={} udn={}",
                    r.spec.key, key
                ));
            }
        }
        self.wake_events();
    }

    /// What the discovery thread announces: each renderer's UDN, BOOTID and
    /// CONFIGID.
    fn announced(&self) -> Vec<(Uuid, u32, u32)> {
        lock(&self.inner)
            .renderers
            .values()
            .map(|r| (r.udn, r.boot_id, r.config_id))
            .collect()
    }

    // ----- control -----

    fn apply_command(&self, command: &str) -> Result<String, UpnpError> {
        self.state.apply(command).map_err(|refusal| {
            (self.log)(&format!(
                "upnp command refused field={} detail=\"{}\"",
                refusal.field, refusal.detail
            ));
            error::ACTION_FAILED
        })
    }

    /// Set the target's volume through the control plane's own commands, so
    /// every room's limit clamps it (I10, K81): `volume` for a room,
    /// `group_volume` for a group that is formed, and for a saved group that
    /// is not formed `volume` on each of its rooms. Returns the state after.
    fn set_volume(&self, spec: &Spec, thousandths: u16) -> Result<String, UpnpError> {
        let volume = format!("{}.{:03}", thousandths / 1000, thousandths % 1000);
        match (&spec.kind, &spec.group) {
            (Kind::Room, _) => self.apply_command(&format!(
                r#"{{"v":1,"t":"volume","zone":"{}","volume":{}}}"#,
                spec.take, volume
            )),
            (_, Some(group)) => self.apply_command(&format!(
                r#"{{"v":2,"t":"group_volume","group":"{}","volume":{}}}"#,
                group, volume
            )),
            (_, None) => {
                let mut state = Err(error::ACTION_FAILED);
                for room in &spec.rooms {
                    state = Ok(self.apply_command(&format!(
                        r#"{{"v":1,"t":"volume","zone":"{}","volume":{}}}"#,
                        room, volume
                    ))?);
                }
                state
            }
        }
    }

    /// Mute or unmute the target: the catalog has no group mute, so a group
    /// target mutes every one of its rooms.
    fn set_mute(&self, spec: &Spec, mute: bool) -> Result<String, UpnpError> {
        let mut state = Err(error::ACTION_FAILED);
        for room in &spec.rooms {
            state = Ok(self.apply_command(&format!(
                r#"{{"v":1,"t":"mute","zone":"{}","muted":{}}}"#,
                room, mute
            ))?);
        }
        state
    }

    /// Start the renderer's current URI on its player: load it when the
    /// player does not have it, then make the target's rooms play the player
    /// (the `take`, K78) unless they already do.
    fn start(
        &self,
        r: &mut Renderer,
        handle: &PlayerHandle,
        index: usize,
    ) -> Result<(), UpnpError> {
        let tag = Self::tag(r);
        let (uri, metadata) = {
            let (u, m) = r.avt.current();
            (u.to_string(), m.to_string())
        };
        let (hints, mime) = hints_of(&uri, &metadata);
        if !r.loaded {
            handle.send(
                tag,
                Action::Load {
                    uri: uri.clone(),
                    mime: mime.clone(),
                },
            );
            r.loaded = true;
            let (next, next_metadata) = {
                let (u, m) = r.avt.next_queued();
                (u.to_string(), m.to_string())
            };
            if !next.is_empty() {
                let (_, next_mime) = hints_of(&next, &next_metadata);
                r.queued.push((next.clone(), next_metadata));
                handle.send(
                    tag,
                    Action::QueueNext {
                        uri: next,
                        mime: next_mime,
                    },
                );
            }
        }
        if self.sessions.in_session(&r.spec.key)
            && self.state.player_group(&player_id(index)).is_some()
        {
            // The rooms already play this player (a new URI while playing).
            handle.send(tag, Action::Start);
            self.sessions.set_metadata(&r.spec.key, hints);
            return Ok(());
        }
        let request = PlayRequest {
            owner: r.spec.key.clone(),
            target: r.spec.take.clone(),
            uri,
            mime,
            via: "upnp".to_string(),
            epoch: tag,
            metadata: hints,
        };
        if let Err(reason) = self.sessions.start_loaded(&request) {
            (self.log)(&format!(
                "upnp play refused target={} reason=\"{}\"",
                r.spec.key, reason
            ));
            let epoch = r.avt.epoch();
            r.avt.failed(epoch, &reason);
            return Err(error::ACTION_FAILED);
        }
        Ok(())
    }

    /// Turn the effects of an AVTransport input into player actions and
    /// control commands.
    fn apply_effects(&self, r: &mut Renderer, effects: Vec<Effect>) -> Result<(), UpnpError> {
        let mut outcome = Ok(());
        if let (Some(index), Some(handle)) = (r.player, self.handle_of(r)) {
            for effect in effects {
                let tag = Self::tag(r);
                match effect {
                    Effect::Load { uri, metadata } => {
                        let (_, mime) = hints_of(&uri, &metadata);
                        r.queued.clear();
                        r.loaded = true;
                        r.idle_since_ms = self.now_ms();
                        handle.send(tag, Action::Load { uri, mime });
                    }
                    Effect::Start => {
                        if let Err(e) = self.start(r, &handle, index) {
                            outcome = Err(e);
                            break;
                        }
                    }
                    Effect::Pause => {
                        self.sessions.set_paused(&r.spec.key, tag, true);
                    }
                    Effect::Resume => {
                        self.sessions.set_paused(&r.spec.key, tag, false);
                    }
                    Effect::Stop => {
                        handle.send(tag, Action::Stop);
                        self.sessions.suspend(&r.spec.key);
                        r.idle_since_ms = self.now_ms();
                    }
                    Effect::SeekTo { ms } => {
                        handle.send(tag, Action::Seek { ms });
                    }
                    Effect::QueueNext { uri, metadata } => {
                        if r.loaded {
                            let (_, mime) = hints_of(&uri, &metadata);
                            r.queued.push((uri.clone(), metadata));
                            if r.queued.len() > 4 {
                                r.queued.remove(0);
                            }
                            handle.send(tag, Action::QueueNext { uri, mime });
                        }
                    }
                    Effect::ClearNext => {
                        handle.send(tag, Action::ClearNext);
                    }
                    Effect::SkipToNext => {
                        handle.send(tag, Action::SkipToNext);
                    }
                }
            }
        }
        if r.avt.state() == TransportState::NoMediaPresent {
            self.unload(r);
        }
        outcome
    }

    /// Give the renderer a player if it has none. `false` when every player
    /// is in use.
    fn take_player(&self, r: &mut Renderer) -> bool {
        if r.player.is_some() {
            return true;
        }
        let Some(index) = self.players.acquire(&r.spec.key) else {
            return false;
        };
        r.player = Some(index);
        r.loaded = false;
        r.queued.clear();
        r.base = self.next_base.fetch_add(1, Ordering::SeqCst) & 0x7fff_ffff;
        r.idle_since_ms = self.now_ms();
        true
    }

    /// Perform one validated action. Never waits on the network.
    fn act(
        &self,
        key: &str,
        service: Service,
        invocation: &Invocation,
    ) -> Result<Outputs, UpnpError> {
        let mut inner = lock(&self.inner);
        let done = match service {
            Service::ConnectionManager => {
                if !inner.renderers.contains_key(key) {
                    return Err(error::ACTION_FAILED);
                }
                return connmgr::invoke(invocation);
            }
            Service::RenderingControl => {
                let r = inner.renderers.get_mut(key).ok_or(error::ACTION_FAILED)?;
                let (out, effects) = r.rcs.invoke(invocation)?;
                let spec = r.spec.clone();
                let mut state = None;
                for effect in effects {
                    state = Some(match effect {
                        rendering::Effect::SetVolume { thousandths } => {
                            self.set_volume(&spec, thousandths)?
                        }
                        rendering::Effect::SetMute { mute } => self.set_mute(&spec, mute)?,
                    });
                }
                // The real values, clamp included, before the action
                // answers: GetVolume and the event show what holds.
                if let Some(state) = state {
                    self.sync(&mut inner, &state);
                }
                out
            }
            Service::AvTransport => {
                let r = inner.renderers.get_mut(key).ok_or(error::ACTION_FAILED)?;
                if let Some(handle) = self.handle_of(r) {
                    r.avt.position(handle.position_ms());
                }
                // A trial on a copy says whether the action is accepted and
                // whether it needs a player, before anything changes.
                let mut trial = r.avt.clone();
                let (_, wanted) = trial.invoke(invocation)?;
                let needs_player = wanted
                    .iter()
                    .any(|e| matches!(e, Effect::Load { .. } | Effect::Start));
                if needs_player && !self.take_player(r) {
                    // UDA11 table 3-3, 501 Action Failed: "current state of
                    // service prevents invoking that action". AVTransport's
                    // own 705 is a device's hold switch and 715 is about the
                    // content, so neither says "no player is free".
                    (self.log)(&format!(
                        "upnp action refused target={} action={} reason=\"no free player: all {} \
                         are in use\"",
                        r.spec.key,
                        invocation.action.name,
                        self.players.len()
                    ));
                    return Err(error::ACTION_FAILED);
                }
                let (out, effects) = r.avt.invoke(invocation)?;
                self.apply_effects(r, effects)?;
                out
            }
        };
        drop(inner);
        self.wake_events();
        Ok(done)
    }

    // ----- the players' reports -----

    /// Take one report of a player: the sessions keep the now-playing record
    /// by it, and the renderer that holds the player moves its AVTransport.
    fn on_report(&self, report: &PlayerReport) {
        let mut inner = lock(&self.inner);
        let Some(r) = inner.renderers.values_mut().find(|r| {
            r.player == Some(report.player) && report.epoch >> 32 == r.base && r.base != 0
        }) else {
            // A player no renderer holds any more (its target vanished).
            self.sessions.on_report(report);
            return;
        };
        if matches!(report.event, Event::Ended { .. } | Event::Failed { .. }) {
            // The renderer's own end of a track: its rooms stop playing the
            // player (the session ends) and the renderer keeps the player,
            // loaded, for [`STOPPED_HOLD_MS`]: a control point that answers
            // STOPPED with the next SetAVTransportURI and Play finds it
            // ready. (Left to the sessions, the player would be unloaded and
            // given back, and could not be taken again until it had
            // unloaded.)
            self.sessions.suspend(&r.spec.key);
            r.idle_since_ms = self.now_ms();
        }
        self.sessions.on_report(report);
        let epoch = report.epoch & 0xffff_ffff;
        match &report.event {
            Event::Opened(info) => {
                r.avt.media_opened(epoch, info.duration_ms, info.seekable);
            }
            Event::Started => {
                r.avt.playing(epoch);
            }
            Event::Boundary(info) => {
                // The player says which track became audible. Almost always
                // it is the queued next URI. When a SetNextAVTransportURI (or
                // a clear) arrived after the join was written and before it
                // was heard, AVTransport's next URI is already another: the
                // joined one is put back for the handover and the newer one
                // queued again behind it, all in one change, so one event
                // says what is true.
                let (next, next_metadata) = {
                    let (u, m) = r.avt.next_queued();
                    (u.to_string(), m.to_string())
                };
                if next != info.uri {
                    let joined = r
                        .queued
                        .iter()
                        .rev()
                        .find(|(uri, _)| *uri == info.uri)
                        .map_or_else(String::new, |(_, metadata)| metadata.clone());
                    let _ = r.avt.set_next_av_transport_uri(&info.uri, &joined);
                    r.avt.track_boundary(epoch, info.duration_ms, info.seekable);
                    if !next.is_empty() {
                        let _ = r.avt.set_next_av_transport_uri(&next, &next_metadata);
                    }
                } else {
                    r.avt.track_boundary(epoch, info.duration_ms, info.seekable);
                }
                let (uri, metadata) = r.avt.current();
                let (hints, _) = hints_of(uri, metadata);
                self.sessions.set_metadata(&r.spec.key, hints);
            }
            Event::NextOpened(_) => {}
            Event::NextFailed { reason } => {
                r.avt.next_failed(epoch, reason);
            }
            Event::Ended { .. } => {
                // Effects only for a next URI that was good and did not join
                // in time: it is loaded and started as a new play.
                let effects = r.avt.ended(epoch);
                let _ = self.apply_effects(r, effects);
            }
            Event::Failed { reason } => {
                (self.log)(&format!(
                    "upnp media failed target={} reason=\"{}\"",
                    r.spec.key, reason
                ));
                r.avt.failed(epoch, reason);
            }
            Event::SeekRefused { .. } => {
                // The player carries on where it was.
                r.avt.playing(epoch);
            }
            Event::Title(_) | Event::SeekDone { .. } => {}
        }
        drop(inner);
        self.wake_events();
    }

    /// What the manager does every turn: players whose group plays something
    /// else are given back (the sessions' rule) and their renderers go
    /// STOPPED; a renderer that has been STOPPED for [`STOPPED_HOLD_MS`]
    /// gives its player back.
    fn housekeeping(&self) {
        let mut inner = lock(&self.inner);
        self.sessions.reconcile();
        let now = self.now_ms();
        let mut changed = false;
        for r in inner.renderers.values_mut() {
            let Some(index) = r.player else { continue };
            if self.players.owner_of(index).as_deref() != Some(r.spec.key.as_str()) {
                // Somebody else changed what this renderer's rooms play (the
                // app, an alarm, another renderer's take): its player was
                // given back.
                r.player = None;
                r.loaded = false;
                r.queued.clear();
                if !matches!(
                    r.avt.state(),
                    TransportState::Stopped | TransportState::NoMediaPresent
                ) {
                    let _ = r.avt.stop();
                    changed = true;
                    (self.log)(&format!(
                        "upnp renderer stopped target={} reason=\"its rooms play something else\"",
                        r.spec.key
                    ));
                }
                continue;
            }
            let idle = matches!(
                r.avt.state(),
                TransportState::Stopped | TransportState::NoMediaPresent
            ) && !self.sessions.in_session(&r.spec.key);
            if idle && now.saturating_sub(r.idle_since_ms) >= STOPPED_HOLD_MS {
                self.unload(r);
            }
        }
        drop(inner);
        if changed {
            self.wake_events();
        }
    }

    // ----- eventing -----

    fn initial_body(r: &Renderer, service: Service) -> String {
        match service {
            Service::AvTransport => gena::propertyset(&[(
                "LastChange",
                &lastchange::event_xml(AVT_NS, &r.avt.evented()),
            )]),
            Service::RenderingControl => gena::propertyset(&[(
                "LastChange",
                &lastchange::event_xml(RCS_NS, &r.rcs.evented()),
            )]),
            Service::ConnectionManager => {
                let vars = connmgr::evented();
                let pairs: Vec<(&str, &str)> = vars.iter().map(|(n, v)| (*n, v.as_str())).collect();
                gena::propertyset(&pairs)
            }
        }
    }

    fn push_job(queues: &mut BTreeMap<String, SubQueue>, sid: String, job: Job, now: u64) {
        let queue = queues.entry(sid).or_default();
        if queue.suspect_until_ms.is_some_and(|until| now < until) {
            // Abandoned (UDA11 section 4.3.2); its key was used all the same.
            return;
        }
        if queue.jobs.len() >= NOTIFY_QUEUE {
            queue.jobs.pop_front();
        }
        queue.jobs.push_back(job);
    }

    /// Queue the initial event of a subscription whose response was written.
    fn queue_initial(&self, key: &str, service: Service, sid: &str) {
        let mut guard = lock(&self.inner);
        let inner = &mut *guard;
        let now = self.now_ms();
        let Some(r) = inner.renderers.get_mut(key) else {
            return;
        };
        let body = Arc::new(Self::initial_body(r, service));
        if let Some(notify) = r.subs[service_index(service)].initial(sid) {
            Self::push_job(
                &mut inner.queues,
                notify.sid,
                Job {
                    seq: notify.seq,
                    callbacks: notify.callbacks,
                    body,
                    udn: key.to_string(),
                    service,
                },
                now,
            );
        }
        drop(guard);
        self.wake_events();
    }

    /// Move every LastChange whose moderation period has passed into the
    /// subscribers' queues, and take what is ready to deliver: the
    /// subscribers that answered last time first.
    fn take_due(&self) -> Vec<(String, Job)> {
        let mut guard = lock(&self.inner);
        let inner = &mut *guard;
        let now = self.now_ms();
        for (key, r) in inner.renderers.iter_mut() {
            for (service, namespace) in [
                (Service::AvTransport, AVT_NS),
                (Service::RenderingControl, RCS_NS),
            ] {
                let moderator = match service {
                    Service::AvTransport => r.avt.events(),
                    _ => r.rcs.events(),
                };
                let Some(changes) = moderator.take(now) else {
                    continue;
                };
                let notifies = r.subs[service_index(service)].event(now);
                if notifies.is_empty() {
                    continue;
                }
                let body = Arc::new(gena::propertyset(&[(
                    "LastChange",
                    &lastchange::event_xml(namespace, &changes),
                )]));
                for notify in notifies {
                    Self::push_job(
                        &mut inner.queues,
                        notify.sid,
                        Job {
                            seq: notify.seq,
                            callbacks: notify.callbacks,
                            body: Arc::clone(&body),
                            udn: key.clone(),
                            service,
                        },
                        now,
                    );
                }
            }
            // ConnectionManager's variables never change; its table still
            // expires.
            r.subs[service_index(Service::ConnectionManager)].expire(now);
        }
        let mut healthy = Vec::new();
        let mut suspects = Vec::new();
        inner.queues.retain(|sid, queue| {
            let was_suspect = queue.suspect_until_ms.is_some();
            if queue.suspect_until_ms.is_some_and(|until| now < until) {
                return true;
            }
            queue.suspect_until_ms = None;
            let out = if was_suspect {
                &mut suspects
            } else {
                &mut healthy
            };
            for job in queue.jobs.drain(..) {
                out.push((sid.clone(), job));
            }
            // An empty queue of a subscriber in good standing is forgotten
            // until its next event.
            false
        });
        healthy.append(&mut suspects);
        healthy
    }

    /// When the event thread should next look, at the latest.
    fn next_event_due(&self) -> Duration {
        let mut inner = lock(&self.inner);
        let now = self.now_ms();
        let mut due: Option<u64> = None;
        for r in inner.renderers.values_mut() {
            for at in [r.avt.events().due_ms(), r.rcs.events().due_ms()]
                .into_iter()
                .flatten()
            {
                due = Some(due.map_or(at, |d| d.min(at)));
            }
        }
        match due {
            Some(at) => Duration::from_millis(at.saturating_sub(now).clamp(1, 200)),
            None => Duration::from_millis(200),
        }
    }

    fn delivery_failed(&self, sid: &str, forgotten: Option<(&str, Service)>) {
        let mut inner = lock(&self.inner);
        let now = self.now_ms();
        if let Some((key, service)) = forgotten {
            // 412: the subscriber does not know the SID any more.
            if let Some(r) = inner.renderers.get_mut(key) {
                let _ = r.subs[service_index(service)].unsubscribe(sid, now);
            }
            inner.queues.remove(sid);
            return;
        }
        let queue = inner.queues.entry(sid.to_string()).or_default();
        queue.jobs.clear();
        queue.suspect_until_ms = Some(now + SUSPECT_FOR_MS);
    }
}

// ----- the event thread ---------------------------------------------------------

enum Delivery {
    Done,
    Forgotten,
    Failed(String),
}

/// One NOTIFY to the first delivery URL that takes it (UDA11 section 4.1.2:
/// the URLs are tried "in order until one succeeds").
fn deliver(sid: &str, job: &Job) -> Delivery {
    let mut last = String::from("no delivery URL");
    for callback in &job.callbacks {
        let address = SocketAddr::new(callback.addr, callback.port);
        let mut stream = match TcpStream::connect_timeout(&address, NOTIFY_CONNECT_TIMEOUT) {
            Ok(s) => s,
            Err(e) => {
                last = format!("{}: {:?}", address, e.kind());
                continue;
            }
        };
        let _ = stream.set_write_timeout(Some(NOTIFY_IO_TIMEOUT));
        let _ = stream.set_read_timeout(Some(NOTIFY_IO_TIMEOUT));
        let _ = stream.set_nodelay(true);
        let request = gena::build_notify(callback, sid, job.seq, &job.body);
        if let Err(e) = stream.write_all(request.as_bytes()) {
            last = format!("{}: {:?}", address, e.kind());
            continue;
        }
        // The status line is all that is read of the answer.
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while head.len() < 64 && !head.ends_with(b"\n") {
            match stream.read(&mut byte) {
                Ok(1) => head.push(byte[0]),
                _ => break,
            }
        }
        let status = String::from_utf8_lossy(&head)
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse::<u16>().ok());
        match status {
            Some(200..=299) => return Delivery::Done,
            Some(412) => return Delivery::Forgotten,
            Some(other) => last = format!("{}: status {}", address, other),
            None => last = format!("{}: no answer", address),
        }
    }
    Delivery::Failed(last)
}

fn run_events(shared: &Renderers, wake: &Receiver<()>, keep: &AtomicBool) {
    while keep.load(Ordering::SeqCst) {
        let jobs = shared.take_due();
        let mut failed: Vec<String> = Vec::new();
        for (sid, job) in &jobs {
            if failed.contains(sid) {
                continue;
            }
            match deliver(sid, job) {
                Delivery::Done => {}
                Delivery::Forgotten => {
                    failed.push(sid.clone());
                    shared.delivery_failed(sid, Some((&job.udn, job.service)));
                    (shared.log)(&format!(
                        "upnp subscription dropped sid={} reason=\"the subscriber answered 412\"",
                        sid
                    ));
                }
                Delivery::Failed(why) => {
                    failed.push(sid.clone());
                    shared.delivery_failed(sid, None);
                    (shared.log)(&format!(
                        "upnp event abandoned sid={} seq={} reason=\"{}\" retry_ms={}",
                        sid, job.seq, why, SUSPECT_FOR_MS
                    ));
                }
            }
        }
        let wait = shared.next_event_due().min(if jobs.is_empty() {
            Duration::from_millis(200)
        } else {
            Duration::from_millis(1)
        });
        match wake.recv_timeout(wait) {
            Ok(()) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => thread::sleep(POLL),
        }
    }
}

// ----- the manager thread -------------------------------------------------------

fn run_manager(shared: &Renderers, reports: &Receiver<PlayerReport>, keep: &AtomicBool) {
    // Subscribed before the state is read, so a change between the two is in
    // the queue rather than lost.
    let mut inbox = shared.state.fanout().subscribe();
    shared.sync(&mut lock(&shared.inner), &shared.state.encoded_state());
    while keep.load(Ordering::SeqCst) {
        match reports.recv_timeout(POLL) {
            Ok(report) => {
                shared.on_report(&report);
                while let Ok(more) = reports.try_recv() {
                    shared.on_report(&more);
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => thread::sleep(POLL),
        }
        let mut latest = None;
        let mut dropped = false;
        loop {
            match inbox.try_recv() {
                Ok(state) => latest = Some(state),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    dropped = true;
                    break;
                }
            }
        }
        if dropped {
            // The fanout dropped this subscriber at its queue's ceiling:
            // attach again, then read the state as it stands.
            inbox = shared.state.fanout().subscribe();
            shared.sync(&mut lock(&shared.inner), &shared.state.encoded_state());
        } else if let Some(state) = latest {
            shared.sync(&mut lock(&shared.inner), &state);
        }
        shared.housekeeping();
    }
}

// ----- the discovery thread -----------------------------------------------------

struct Known {
    udn: Uuid,
    boot_id: u32,
    config_id: u32,
    announcer: Announcer,
}

struct Discovery<'a> {
    shared: &'a Renderers,
    socket: UdpSocket,
    port: u16,
    group: SocketAddrV4,
}

impl Discovery<'_> {
    /// The address a peer reaches this server's HTTP port at: the listener's
    /// own when it names one, else the local address the route to the peer
    /// leaves from (a connected UDP socket's, which sends nothing).
    fn host_for(&self, peer: SocketAddr) -> Option<IpAddr> {
        let ip = self.shared.settings.http.ip();
        if !ip.is_unspecified() {
            return Some(ip);
        }
        let probe = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
        probe.connect(peer).ok()?;
        probe.local_addr().ok().map(|a| a.ip())
    }

    fn advert(&self, known: &Known, peer: SocketAddr) -> Option<Advert> {
        let host = SocketAddr::new(self.host_for(peer)?, self.shared.settings.http.port());
        Some(Advert {
            udn: known.udn,
            location: format!(
                "http://{}{}",
                host,
                description::description_path(&known.udn)
            ),
            server: self.shared.server.clone(),
            max_age_s: ssdp::MAX_AGE_S,
            boot_id: known.boot_id,
            config_id: known.config_id,
            // UDA11 section 1.2.2: a device that answers unicast searches on
            // a port other than 1900 says which, in 49152 to 65535.
            search_port: (self.port != SSDP_PORT && self.port >= 49152).then_some(self.port),
        })
    }

    fn notify(&self, messages: &[String]) {
        for message in messages {
            let _ = self.socket.send_to(message.as_bytes(), self.group);
        }
    }

    fn byebye(&self, known: &Known) {
        if let Some(advert) = self.advert(known, SocketAddr::V4(self.group)) {
            let set = ssdp::byebye_set(&advert);
            for _ in 0..ssdp::SETS {
                self.notify(&set);
            }
        }
    }
}

/// Whether a search datagram was written for the multicast group: its HOST
/// names the group (UDA11 section 1.3.2). `std` does not say which address a
/// datagram arrived on, and the HOST header is how the sender says which form
/// of search it made: a multicast one must carry MX, a unicast one has none.
fn is_multicast_search(datagram: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(datagram) else {
        return false;
    };
    let (_, headers) = Headers::parse(text);
    headers
        .get("HOST")
        .and_then(|h| h.rsplit_once(':').map_or(Some(h), |(host, _)| Some(host)))
        .and_then(|host| host.parse::<Ipv4Addr>().ok())
        .is_none_or(|ip| ip.is_multicast())
}

fn run_ssdp(d: &Discovery<'_>, keep: &AtomicBool, seed: u64) {
    let mut jitter = SeededJitter::new(seed);
    let mut limiter = SearchLimiter::standard();
    let mut known: BTreeMap<String, Known> = BTreeMap::new();
    let mut outbox: Vec<(u64, SocketAddr, String)> = Vec::new();
    let mut buffer = [0u8; 2048];
    let group = SocketAddr::V4(d.group);
    while keep.load(Ordering::SeqCst) {
        let now = d.shared.now_ms();
        // Follow the table: byebye for what is gone or was renamed, a new
        // announce schedule for what is new.
        let table = d.shared.announced();
        known.retain(|key, k| {
            let still = table
                .iter()
                .any(|(udn, boot, _)| udn.to_string() == *key && *boot == k.boot_id);
            if !still {
                // Nothing more is said for it but byebye: search responses
                // still waiting for their moment are dropped.
                let usn = format!("uuid:{}", key);
                outbox.retain(|(_, _, message)| !message.contains(&usn));
                d.byebye(k);
            }
            still
        });
        for (udn, boot_id, config_id) in table {
            known.entry(udn.to_string()).or_insert_with(|| Known {
                udn,
                boot_id,
                config_id,
                announcer: Announcer::new(now, ssdp::MAX_AGE_S, &mut jitter),
            });
        }
        for k in known.values_mut() {
            if k.announcer.poll(now, &mut jitter) {
                if let Some(advert) = d.advert(k, group) {
                    d.notify(&ssdp::alive_set(&advert));
                }
            }
        }
        // What is due of the search responses.
        outbox.retain(|(due, peer, message)| {
            if *due > now {
                return true;
            }
            let _ = d.socket.send_to(message.as_bytes(), peer);
            false
        });
        let next = outbox
            .iter()
            .map(|(due, _, _)| *due)
            .chain(known.values().map(|k| k.announcer.due_ms()))
            .min()
            .map_or(50, |due| due.saturating_sub(now).clamp(1, 50));
        let _ = d.socket.set_read_timeout(Some(Duration::from_millis(next)));
        let (len, peer) = match d.socket.recv_from(&mut buffer) {
            Ok(v) => v,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                ) =>
            {
                continue
            }
            Err(_) => {
                thread::sleep(POLL);
                continue;
            }
        };
        let datagram = &buffer[..len];
        let Ok(search) = ssdp::parse_search(datagram, is_multicast_search(datagram)) else {
            continue;
        };
        let now = d.shared.now_ms();
        if !limiter.allow(peer.ip(), now) {
            continue;
        }
        let date = http_date();
        let mut responses = Vec::new();
        for k in known.values() {
            if let Some(advert) = d.advert(k, peer) {
                responses.extend(ssdp::search_responses(&search, &advert, Some(&date)));
            }
        }
        if outbox.len() + responses.len() > OUTBOX_LIMIT {
            continue;
        }
        let delays = ssdp::response_delays(responses.len(), search.window_ms, &mut jitter);
        for (message, delay) in responses.into_iter().zip(delays) {
            outbox.push((now + delay, peer, message));
        }
    }
    // The run is over: every renderer says byebye.
    for k in known.values() {
        d.byebye(k);
    }
    (d.shared.log)(&format!("upnp stopped byebye_renderers={}", known.len()));
}

// ----- the HTTP workers ---------------------------------------------------------

struct Request {
    method: String,
    path: String,
    /// The request was HTTP/1.0: the answer says so too.
    http10: bool,
    headers: Headers,
    body: Vec<u8>,
}

/// Read up to `want` more bytes into `into` before `deadline`.
fn read_some(stream: &mut TcpStream, into: &mut Vec<u8>, deadline: Instant) -> Result<(), u16> {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return Err(408);
    }
    let _ = stream.set_read_timeout(Some(left));
    let mut scratch = [0u8; 4096];
    match stream.read(&mut scratch) {
        Ok(0) => Err(400),
        Ok(n) => {
            into.extend_from_slice(&scratch[..n]);
            Ok(())
        }
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
            ) =>
        {
            Err(408)
        }
        Err(e) if e.kind() == io::ErrorKind::Interrupted => Ok(()),
        Err(_) => Err(400),
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// A chunked body (RFC 9112 section 7.1) decoded from `raw`, which starts at
/// the first chunk-size line; more is read as needed.
fn read_chunked(
    stream: &mut TcpStream,
    mut raw: Vec<u8>,
    deadline: Instant,
) -> Result<Vec<u8>, u16> {
    let mut body = Vec::new();
    let mut at = 0usize;
    loop {
        let line_end = loop {
            if let Some(end) = find(&raw[at..], b"\r\n") {
                break at + end;
            }
            if raw.len() - at > 256 {
                return Err(400);
            }
            read_some(stream, &mut raw, deadline)?;
        };
        let line = std::str::from_utf8(&raw[at..line_end]).map_err(|_| 400u16)?;
        let size = line.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size, 16).map_err(|_| 400u16)?;
        at = line_end + 2;
        if size == 0 {
            // Trailer fields, if any, are not wanted.
            return Ok(body);
        }
        if body.len() + size > MAX_BODY_BYTES {
            return Err(413);
        }
        while raw.len() < at + size + 2 {
            read_some(stream, &mut raw, deadline)?;
        }
        body.extend_from_slice(&raw[at..at + size]);
        at += size + 2;
    }
}

/// One request: the head, then a body by `Content-Length` or chunked. `Err`
/// is the status that refuses it.
fn read_request(stream: &mut TcpStream) -> Result<Request, u16> {
    let deadline = Instant::now() + REQUEST_DEADLINE;
    let mut raw = Vec::new();
    let head_end = loop {
        if let Some(end) = find(&raw, b"\r\n\r\n") {
            break end;
        }
        if raw.len() > MAX_HEAD_BYTES {
            return Err(431);
        }
        read_some(stream, &mut raw, deadline)?;
    };
    let head = std::str::from_utf8(&raw[..head_end]).map_err(|_| 400u16)?;
    let (start, headers) = Headers::parse(head);
    let mut words = start.split_whitespace();
    let (Some(method), Some(path), Some(version)) = (words.next(), words.next(), words.next())
    else {
        return Err(400);
    };
    if !version.starts_with("HTTP/1.") {
        return Err(400);
    }
    let (method, path, http10) = (method.to_string(), path.to_string(), version == "HTTP/1.0");
    let rest = raw[head_end + 4..].to_vec();
    if headers
        .get("EXPECT")
        .is_some_and(|e| e.eq_ignore_ascii_case("100-continue"))
    {
        let _ = stream.write_all(b"HTTP/1.1 100 Continue\r\n\r\n");
    }
    let chunked = headers
        .get("TRANSFER-ENCODING")
        .is_some_and(|t| t.to_ascii_lowercase().contains("chunked"));
    let body = if chunked {
        read_chunked(stream, rest, deadline)?
    } else {
        let length = match headers.get("CONTENT-LENGTH") {
            None => 0,
            Some(text) => text.trim().parse::<usize>().map_err(|_| 400u16)?,
        };
        if length > MAX_BODY_BYTES {
            return Err(413);
        }
        let mut body = rest;
        while body.len() < length {
            read_some(stream, &mut body, deadline)?;
        }
        body.truncate(length);
        body
    };
    Ok(Request {
        method,
        path,
        http10,
        headers,
        body,
    })
}

fn write_reply(
    stream: &mut TcpStream,
    http10: bool,
    server: &str,
    reply: &Reply,
) -> io::Result<()> {
    let mut head = format!(
        "HTTP/1.{} {} {}\r\n",
        if http10 { 0 } else { 1 },
        reply.status,
        reply.reason
    );
    for (name, value) in &reply.headers {
        head.push_str(&format!("{}: {}\r\n", name, value));
    }
    head.push_str(&format!(
        "CONTENT-LENGTH: {}\r\nDATE: {}\r\nSERVER: {}\r\nCONNECTION: close\r\n\r\n",
        reply.body.len(),
        http_date(),
        server
    ));
    let _ = stream.set_write_timeout(Some(RESPONSE_TIMEOUT));
    stream.write_all(head.as_bytes())?;
    if !reply.head_only {
        stream.write_all(reply.body.as_bytes())?;
    }
    stream.flush()
}

/// What to do once the response is written: the initial event of a new
/// subscription (UDA11 section 4.1.2: only after the subscriber has the
/// response).
type After = Option<(String, Service, String)>;

impl Renderers {
    fn control(&self, key: &str, service: Service, request: &Request) -> Reply {
        if !request
            .headers
            .get("CONTENT-TYPE")
            .is_some_and(soap::is_xml_content_type)
        {
            return Reply::empty(415, reason_of(415));
        }
        let Ok(body) = std::str::from_utf8(&request.body) else {
            return Reply::empty(400, reason_of(400));
        };
        let Ok(action) = soap::parse_request(request.headers.get("SOAPACTION"), body) else {
            return Reply::empty(400, reason_of(400));
        };
        let result = soap::validate(service, &action)
            .and_then(|invocation| self.act(key, service, &invocation));
        match result {
            Ok(out) => {
                let mut reply = Reply::xml(
                    200,
                    "OK",
                    soap::build_response(service, &action.action, &out),
                );
                reply.headers.push(("EXT", String::new()));
                reply
            }
            Err(e) => {
                let mut reply = Reply::xml(500, "Internal Server Error", soap::build_fault(&e));
                reply.headers.push(("EXT", String::new()));
                reply
            }
        }
    }

    fn subscribe(
        &self,
        key: &str,
        service: Service,
        request: &Request,
        peer: IpAddr,
    ) -> (Reply, After) {
        let refuse = |status: u16| (Reply::empty(status, reason_of(status)), None);
        let parsed = match gena::parse_subscribe(&request.headers) {
            Ok(p) => p,
            Err(status) => return refuse(status),
        };
        let now = self.now_ms();
        let granted = |sid: &str, seconds: u32| {
            let mut reply = Reply::empty(200, "OK");
            reply.headers.push(("SID", sid.to_string()));
            reply
                .headers
                .push(("TIMEOUT", format!("Second-{}", seconds)));
            reply
        };
        match parsed {
            Subscribe::Renew { sid, timeout_s } => {
                let mut inner = lock(&self.inner);
                let Some(r) = inner.renderers.get_mut(key) else {
                    return refuse(404);
                };
                match r.subs[service_index(service)].renew(&sid, timeout_s, now) {
                    Ok(seconds) => (granted(&sid, seconds), None),
                    Err(refusal) => refuse(refusal.status()),
                }
            }
            Subscribe::New {
                callbacks,
                timeout_s,
            } => {
                // Every delivery URL must pass: one that points anywhere but
                // at the subscriber itself, inside the household's subnets,
                // refuses the subscription (the CallStranger rule and P6's
                // stricter list, `gena::callback_allowed`).
                let mut allowed = Vec::with_capacity(callbacks.len());
                for url in &callbacks {
                    match gena::callback_allowed(
                        url,
                        peer,
                        &self.settings.subnets,
                        self.allow_loopback(),
                    ) {
                        Ok(callback) => allowed.push(callback),
                        Err(why) => {
                            (self.log)(&format!(
                                "upnp subscription refused peer={} reason={:?}",
                                peer, why
                            ));
                            return refuse(412);
                        }
                    }
                }
                let Ok(random) = crate::session::random_32() else {
                    return refuse(503);
                };
                let mut bytes = [0u8; 16];
                bytes.copy_from_slice(&random[..16]);
                let sid = gena::sid_from_random(bytes);
                let mut inner = lock(&self.inner);
                let Some(r) = inner.renderers.get_mut(key) else {
                    return refuse(404);
                };
                match r.subs[service_index(service)].subscribe(sid.clone(), allowed, timeout_s, now)
                {
                    Ok(seconds) => (
                        granted(&sid, seconds),
                        Some((key.to_string(), service, sid)),
                    ),
                    Err(refusal) => refuse(refusal.status()),
                }
            }
        }
    }

    fn unsubscribe(&self, key: &str, service: Service, request: &Request) -> Reply {
        let sid = match gena::parse_unsubscribe(&request.headers) {
            Ok(sid) => sid,
            Err(status) => return Reply::empty(status, reason_of(status)),
        };
        let mut inner = lock(&self.inner);
        let now = self.now_ms();
        let Some(r) = inner.renderers.get_mut(key) else {
            return Reply::empty(404, reason_of(404));
        };
        match r.subs[service_index(service)].unsubscribe(&sid, now) {
            Ok(()) => {
                inner.queues.remove(&sid);
                Reply::empty(200, "OK")
            }
            Err(refusal) => Reply::empty(refusal.status(), reason_of(refusal.status())),
        }
    }

    fn route(&self, request: &Request, peer: IpAddr) -> (Reply, After) {
        let not_found = || (Reply::empty(404, reason_of(404)), None);
        let Some((udn, resource)) = description::route(&request.path) else {
            return not_found();
        };
        let key = udn.to_string();
        let (description, config_id) = match lock(&self.inner).renderers.get(&key) {
            Some(r) => (r.description.clone(), r.config_id),
            // Unknown, or vanished: the device is not there.
            None => return not_found(),
        };
        let method = request.method.as_str();
        let wrong_method = |allow: &str| {
            let mut reply = Reply::empty(405, reason_of(405));
            reply.headers.push(("ALLOW", allow.to_string()));
            (reply, None)
        };
        let document = |body: String| {
            let mut reply = Reply::xml(200, "OK", body);
            reply.head_only = method == "HEAD";
            (reply, None)
        };
        match resource {
            Resource::Description if matches!(method, "GET" | "HEAD") => document(description),
            Resource::Scpd(service) if matches!(method, "GET" | "HEAD") => {
                document(description::scpd(service, config_id))
            }
            Resource::Description | Resource::Scpd(_) => wrong_method("GET, HEAD"),
            Resource::Control(service) => match method {
                "POST" => (self.control(&key, service, request), None),
                // M-POST (the SOAP HTTP extension framework) is not offered.
                _ => wrong_method("POST"),
            },
            Resource::Event(service) => match method {
                "SUBSCRIBE" => self.subscribe(&key, service, request, peer),
                "UNSUBSCRIBE" => (self.unsubscribe(&key, service, request), None),
                _ => wrong_method("SUBSCRIBE, UNSUBSCRIBE"),
            },
        }
    }

    fn serve(&self, mut stream: TcpStream) {
        let peer = match stream.peer_addr() {
            Ok(p) => p.ip(),
            Err(_) => return,
        };
        let (http10, reply, after) = match read_request(&mut stream) {
            Ok(request) => {
                let (reply, after) = self.route(&request, peer);
                (request.http10, reply, after)
            }
            Err(status) => (false, Reply::empty(status, reason_of(status)), None),
        };
        let written = write_reply(&mut stream, http10, &self.server, &reply);
        let _ = stream.shutdown(std::net::Shutdown::Write);
        if let (Ok(()), Some((key, service, sid))) = (written, after) {
            self.queue_initial(&key, service, &sid);
        }
    }
}

fn refuse_busy(mut stream: TcpStream, server: &str) {
    let _ = write_reply(
        &mut stream,
        false,
        server,
        &Reply::empty(503, reason_of(503)),
    );
}

// ----- assembly -----------------------------------------------------------------

/// The supervisor's end of a clean stop: dropped when the run ends, it tells
/// the threads to stop and gives the discovery thread [`FAREWELL_WAIT`] to
/// say byebye for every renderer before the process exits.
pub struct Farewell {
    keep: Arc<AtomicBool>,
    done: Receiver<()>,
}

impl Drop for Farewell {
    fn drop(&mut self) {
        self.keep.store(false, Ordering::SeqCst);
        let _ = self.done.recv_timeout(FAREWELL_WAIT);
    }
}

/// The `SERVER` value: `Linux/<kernel release> UPnP/1.1 chorus/<version>`.
fn server_token() -> String {
    let release = std::fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
    let version: String = release
        .trim()
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    ssdp::server_token(
        "Linux",
        if version.is_empty() { "0" } else { &version },
        env!("CARGO_PKG_VERSION"),
    )
}

/// The renderers, ready to have their threads made.
pub struct Upnp {
    shared: Arc<Renderers>,
    sockets: Sockets,
    workers: usize,
    reports: Receiver<PlayerReport>,
    wake: Receiver<()>,
}

impl Upnp {
    /// Assemble the renderers over the sockets already bound.
    ///
    /// `subnets` is what `--upnp-callback-subnet` gave, already parsed; empty
    /// takes the host's own ([`on_link_subnets`]). `server_id` is the stable
    /// identity the UDNs rest on. `sessions` is the server's one table of
    /// player sessions (goal 17: an alarm's stored stream URL plays through
    /// the same one). `reports` is the players' one report stream
    /// (`Players::take_reports`).
    pub fn new(
        sockets: Sockets,
        flags: &UpnpFlags,
        server_id: String,
        state: Arc<ControlState>,
        players: Arc<Players>,
        sessions: Arc<PlayerSessions>,
        reports: Receiver<PlayerReport>,
        log: Box<dyn Fn(&str) + Send + Sync>,
    ) -> Upnp {
        let mut subnets: Vec<Cidr> = flags
            .callback_subnets
            .iter()
            .filter_map(|s| Cidr::parse(s))
            .collect();
        if subnets.is_empty() {
            subnets = on_link_subnets();
        }
        let (wake_events, wake) = mpsc::sync_channel(1);
        let session_log: Arc<dyn Fn(&str) + Send + Sync> = Arc::from(log);
        let shared = Arc::new(Renderers {
            settings: Settings {
                http: sockets.http,
                subnets,
                server_id,
            },
            state,
            players,
            sessions,
            inner: Mutex::new(Inner::default()),
            started: Instant::now(),
            next_base: AtomicU64::new(1),
            server: server_token(),
            wake_events,
            log: Box::new(move |line: &str| session_log(line)),
        });
        Upnp {
            shared,
            sockets,
            workers: flags.workers,
            reports,
            wake,
        }
    }

    /// The line the server prints about the renderers at start.
    pub fn describe(&self) -> String {
        let s = &self.shared.settings;
        format!(
            "upnp renderers listening on={} workers={} ssdp_port={} ssdp_group={} \
             callback_subnets={} loopback_callbacks={} identity={}",
            s.http,
            self.workers,
            self.sockets.ssdp_port,
            self.sockets.group,
            if s.subnets.is_empty() {
                "none".to_string()
            } else {
                s.subnets
                    .iter()
                    .map(|c| format!("{:?}", c))
                    .collect::<Vec<_>>()
                    .join(",")
            },
            if self.shared.allow_loopback() {
                "allowed-because-the-listener-is-loopback"
            } else {
                "refused"
            },
            s.server_id
        )
    }

    /// Create every thread the renderers will ever run: `upnp-ssdp`,
    /// `upnp-acceptor`, `upnp-worker-<i>` for each worker, `upnp-events` and
    /// `upnp-manager`. Each registers itself as an ordinary thread and sends
    /// one unit down `ready`, as every thread of the population does.
    /// Returns how many threads were created and the supervisor's end of a
    /// clean stop.
    pub fn spawn(
        self,
        keep: &Arc<AtomicBool>,
        registry: &Arc<ThreadRegistry>,
        ready: &Sender<()>,
    ) -> (usize, Farewell) {
        let Upnp {
            shared,
            sockets,
            workers,
            reports,
            wake,
        } = self;
        let Sockets {
            listener,
            ssdp,
            ssdp_port,
            group,
            ..
        } = sockets;
        let start = |role: String, body: Box<dyn FnOnce() + Send>| {
            let registry = Arc::clone(registry);
            let ready = ready.clone();
            thread::spawn(move || {
                register_ordinary_thread(&role, &registry);
                if ready.send(()).is_err() {
                    return;
                }
                drop(ready);
                body();
            });
        };
        let (said, done) = mpsc::channel();
        {
            let (shared, keep) = (Arc::clone(&shared), Arc::clone(keep));
            let seed = crate::session::random_32()
                .map(|b| u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
                .unwrap_or(1);
            start(
                "upnp-ssdp".to_string(),
                Box::new(move || {
                    let discovery = Discovery {
                        shared: &shared,
                        socket: ssdp,
                        port: ssdp_port,
                        group,
                    };
                    run_ssdp(&discovery, &keep, seed);
                    let _ = said.send(());
                }),
            );
        }
        let (free_tx, free) = mpsc::channel::<usize>();
        let mut to_workers: Vec<SyncSender<TcpStream>> = Vec::with_capacity(workers);
        for index in 0..workers {
            let (to_worker, jobs) = mpsc::sync_channel::<TcpStream>(1);
            let shared = Arc::clone(&shared);
            let returning = free_tx.clone();
            start(
                format!("upnp-worker-{}", index),
                Box::new(move || {
                    for connection in jobs {
                        shared.serve(connection);
                        if returning.send(index).is_err() {
                            return;
                        }
                    }
                }),
            );
            to_workers.push(to_worker);
            let _ = free_tx.send(index);
        }
        {
            let (shared, keep) = (Arc::clone(&shared), Arc::clone(keep));
            start(
                "upnp-acceptor".to_string(),
                Box::new(move || {
                    while keep.load(Ordering::SeqCst) {
                        let Ok((connection, _)) = listener.accept() else {
                            return;
                        };
                        let _ = connection.set_nodelay(true);
                        match free.try_recv() {
                            Ok(index) => {
                                if to_workers[index].send(connection).is_err() {
                                    return;
                                }
                            }
                            // Every worker is busy: answered honestly and
                            // closed, never queued behind a thread that does
                            // not exist.
                            Err(_) => refuse_busy(connection, &shared.server),
                        }
                    }
                }),
            );
        }
        {
            let (shared, keep) = (Arc::clone(&shared), Arc::clone(keep));
            start(
                "upnp-events".to_string(),
                Box::new(move || run_events(&shared, &wake, &keep)),
            );
        }
        {
            let (shared, keep) = (Arc::clone(&shared), Arc::clone(keep));
            start(
                "upnp-manager".to_string(),
                Box::new(move || run_manager(&shared, &reports, &keep)),
            );
        }
        (
            workers + 4,
            Farewell {
                keep: Arc::clone(keep),
                done,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_callback_subnets_are_the_on_link_routes_without_loopback_and_link_local() {
        let table =
            "Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\t\tMTU\tWindow\tIRTT\n\
                     eth0\t00000000\t010200C0\t0003\t0\t0\t0\t00000000\t0\t0\t0\n\
                     eth0\t000200C0\t00000000\t0001\t0\t0\t0\t00FFFFFF\t0\t0\t0\n\
                     eth1\t0000FEA9\t00000000\t0001\t0\t0\t0\t0000FFFF\t0\t0\t0\n\
                     lo\t0000007F\t00000000\t0001\t0\t0\t0\t000000FF\t0\t0\t0\n\
                     br0\t006433C6\t00000000\t0001\t0\t0\t0\t80FFFFFF\t0\t0\t0\n";
        if cfg!(target_endian = "little") {
            assert_eq!(
                subnets_of_route_table(table),
                vec![
                    Cidr::parse("192.0.2.0/24").unwrap(),
                    Cidr::parse("198.51.100.0/25").unwrap()
                ]
            );
        }
        assert!(subnets_of_route_table("").is_empty());
    }

    #[test]
    fn a_loopback_callback_is_allowed_only_when_the_listener_is_on_loopback() {
        let requester: IpAddr = "127.0.0.1".parse().unwrap();
        let url = "http://127.0.0.1:49200/cb";
        let subnets = [Cidr::parse("192.0.2.0/24").unwrap()];
        // The tests' shape: the listener is on loopback.
        let on_loopback: SocketAddr = "127.0.0.1:4030".parse().unwrap();
        assert!(gena::callback_allowed(
            url,
            requester,
            &subnets,
            loopback_callbacks_allowed(&on_loopback)
        )
        .is_ok());
        // Every deployment: the listener is on the LAN, or on every address.
        for listener in ["192.0.2.10:4030", "0.0.0.0:4030"] {
            let listener: SocketAddr = listener.parse().unwrap();
            assert_eq!(
                gena::callback_allowed(
                    url,
                    requester,
                    &subnets,
                    loopback_callbacks_allowed(&listener)
                ),
                Err(gena::CallbackRefusal::Loopback),
                "{listener}"
            );
        }
        // A subscriber inside the subnets is taken either way, and one
        // outside them refused either way.
        let lan: IpAddr = "192.0.2.50".parse().unwrap();
        assert!(gena::callback_allowed("http://192.0.2.50:8080/cb", lan, &subnets, false).is_ok());
        let far: IpAddr = "198.51.100.7".parse().unwrap();
        assert_eq!(
            gena::callback_allowed("http://198.51.100.7:8080/cb", far, &subnets, true),
            Err(gena::CallbackRefusal::OutsideSubnets)
        );
    }

    #[test]
    fn a_search_is_multicast_by_its_host_header() {
        assert!(is_multicast_search(
            b"M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\n\r\n"
        ));
        assert!(!is_multicast_search(
            b"M-SEARCH * HTTP/1.1\r\nHOST: 192.0.2.7:1900\r\n\r\n"
        ));
        // No HOST at all is held to the stricter, multicast rule.
        assert!(is_multicast_search(b"M-SEARCH * HTTP/1.1\r\n\r\n"));
    }

    const STATE: &str = concat!(
        r#"{"v":2,"t":"state","serial":7,"zones":["#,
        r#"{"id":"kitchen","name":"Kitchen","group":"live-1","volume":0.400,"muted":false},"#,
        r#"{"id":"den","name":"Den","group":"live-1","volume":0.600,"muted":true},"#,
        r#"{"id":"study","name":"study","group":"study","volume":1.000,"muted":false}],"#,
        r#""groups":[{"id":"live-1","kind":"live","zones":["kitchen","den"],"volume":0.500,"#,
        r#""source":"stream","audio":"127.0.0.1:4010"},"#,
        r#"{"id":"study","kind":"room","zones":["study"],"volume":1.000,"source":"stream","#,
        r#""audio":"127.0.0.1:4010"}],"#,
        r#""saved_groups":[{"id":"upstairs","name":"Upstairs","zones":["den","study"],"#,
        r#""active":false}]}"#
    );

    #[test]
    fn the_targets_are_every_room_every_saved_group_and_every_live_group() {
        let (serial, specs) = specs_of(STATE).unwrap();
        assert_eq!(serial, 7);
        let keys: Vec<&str> = specs.iter().map(|s| s.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "room:kitchen",
                "room:den",
                "room:study",
                "group:upstairs",
                "live:den+kitchen"
            ]
        );
        let by = |key: &str| specs.iter().find(|s| s.key == key).unwrap();
        assert_eq!((by("room:den").volume, by("room:den").mute), (600, true));
        // A saved group that is not formed: the average of its rooms, no
        // formed group to set a group volume on, muted only when all are.
        let saved = by("group:upstairs");
        assert_eq!(
            (saved.name.as_str(), saved.volume, saved.mute, &saved.group),
            ("Upstairs", 800, false, &None)
        );
        // A live group is called by its rooms, in the order of their ids,
        // and taken by its current id.
        let live = by("live:den+kitchen");
        assert_eq!(
            (live.name.as_str(), live.take.as_str(), live.volume),
            ("Den + Kitchen", "live-1", 500)
        );
        assert_eq!(live.group.as_deref(), Some("live-1"));
    }

    #[test]
    fn metadata_gives_the_now_playing_hints_and_the_media_type() {
        let didl = concat!(
            r#"<DIDL-Lite xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/" "#,
            r#"xmlns:dc="http://purl.org/dc/elements/1.1/" "#,
            r#"xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/"><item id="1" parentID="0" "#,
            r#"restricted="1"><dc:title>Low Tide</dc:title>"#,
            r#"<upnp:artist>The Harbour Lights</upnp:artist><upnp:album>Salt</upnp:album>"#,
            r#"<res protocolInfo="http-get:*:audio/L16;rate=44100;channels=2:*" "#,
            r#"duration="0:00:03">http://192.0.2.9/a</res></item></DIDL-Lite>"#
        );
        let (hints, mime) = hints_of("http://192.0.2.9/a", didl);
        assert_eq!(hints.title.as_deref(), Some("Low Tide"));
        assert_eq!(hints.artist.as_deref(), Some("The Harbour Lights"));
        assert_eq!(hints.album.as_deref(), Some("Salt"));
        assert_eq!(hints.duration_ms, Some(3000));
        assert_eq!(mime.as_deref(), Some("audio/L16;rate=44100;channels=2"));
        assert_eq!(
            hints_of("http://192.0.2.9/a", "garbage"),
            (Metadata::default(), None)
        );
    }
}
