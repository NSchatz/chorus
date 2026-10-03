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
//! # The OpenHome services (goal 17; P6, Option B)
//!
//! With `--upnp-openhome on`, the default, each of those devices also offers
//! OpenHome's Product:2, Volume:2, Info:1, Time:1 and Playlist:1
//! (`chorus_upnp::openhome`; `docs/upnp.md`, "OpenHome";
//! `docs/decisions/0128-openhome-services.md`). A renderer then has
//! two **decks**, of which one at a time drives its player: AVTransport's
//! (the Product source `UpnpAv`) and the Playlist's. An AVTransport
//! `SetAVTransportURI` or `Play` takes the player for the first, a Playlist
//! `Play`, `SeekId` or `SeekIndex` for the second, and the deck that lost it
//! goes STOPPED. The Playlist is held here and walked here: the following
//! track is handed to the player ahead of each boundary, the same next-URI
//! join a control point's `SetNextAVTransportURI` gets, so a list plays
//! through gapless with no control point connected. Its URIs reach the
//! network only through the players' fetch policy, as every renderer URI
//! does (brief section 4.8). No thread is added: the manager applies the
//! player's reports to whichever deck holds the player, and the event thread
//! sends the plain property-set events, the once-a-second Time event among
//! them.
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

use chorus_hostctl::ThreadRegistry;
use chorus_upnp::avtransport::{AvTransport, Effect, TransportState};
use chorus_upnp::description::{self, DeviceInfo, Resource};
use chorus_upnp::gena::{self, CallbackUrl, Cidr, Subscribe, Subscriptions};
use chorus_upnp::lastchange::{self, AVT_NS, RCS_NS};
use chorus_upnp::openhome::info::{Details, Info};
use chorus_upnp::openhome::playlist::{Playlist, ID_ARRAY_HOLD_MS};
use chorus_upnp::openhome::product::{self, Product};
use chorus_upnp::openhome::time::Time;
use chorus_upnp::openhome::volume::Volume;
use crate::targets::{specs_of, Kind, Playing, Spec};
use chorus_upnp::openhome::{Property, Tracker};
use chorus_upnp::rendering::{self, RenderingControl};
use chorus_upnp::soap::{self, Invocation};
use chorus_upnp::ssdp::{self, Advert, Announcer, SearchLimiter, SeededJitter};
use chorus_upnp::uuid::{udn, Target, Uuid, CHORUS_NAMESPACE};
use chorus_upnp::xml::escape_text;
use chorus_upnp::{connmgr, didl, error, Headers, Outputs, Service, UpnpError};

use crate::config::UpnpFlags;
use crate::control::ControlState;
use crate::hostreport::register_ordinary_thread;
use crate::mediaplayer::{Action, Event, MediaInfo, PlayerHandle, PlayerReport, Players};
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

// ----- one renderer -----------------------------------------------------------

struct Renderer {
    spec: Spec,
    udn: Uuid,
    config_id: u32,
    boot_id: u32,
    description: String,
    avt: AvTransport,
    rcs: RenderingControl,
    /// The subscriptions of every service, in [`Service::ALL`]'s order
    /// ([`Service::index`]); those of the OpenHome services stay empty when
    /// they are switched off.
    subs: [Subscriptions; 8],
    /// The OpenHome services' state.
    oh: OpenHome,
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
    service.index()
}

/// Which of a renderer's two transports drives its player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Deck {
    /// AVTransport: a control point pushes one URI and the next (the Product
    /// source `UpnpAv`).
    UpnpAv,
    /// The OpenHome Playlist, which the renderer walks itself.
    Playlist,
}

/// The OpenHome state of one renderer (`chorus_upnp::openhome`).
struct OpenHome {
    product: Product,
    volume: Volume,
    info: Info,
    time: Time,
    playlist: Playlist,
    /// What each service last evented, in [`Service::OPENHOME`]'s order.
    trackers: [Tracker; 5],
    /// The deck that drives the player. Always [`Deck::UpnpAv`] when the
    /// OpenHome services are off.
    deck: Deck,
    /// A track start is awaited: a Load or a Start went to the player, and
    /// its `Started` report is the start of a track for Info and Time.
    fresh: bool,
    /// What the player last opened.
    media: Option<MediaInfo>,
}

/// The Product `Type` of an input of that kind.
fn source_type(kind: Option<chorus_protocol::v2::SourceKind>) -> &'static str {
    use chorus_protocol::v2::SourceKind;
    match kind {
        Some(SourceKind::Optical) => product::TYPE_DIGITAL,
        Some(SourceKind::HdmiArc) => product::TYPE_HDMI,
        Some(SourceKind::LineIn) | None => product::TYPE_ANALOG,
    }
}

/// The source spelling of a Spotify receiver (goal 17's Soloist track; the
/// spelling may not exist yet in a given build, and then no group plays it).
const SOLOIST_PREFIX: &str = "soloist:";
const LINE_IN_PREFIX: &str = "line-in:";

/// The Product sources of a target, in order: the Playlist, UPnP AV (not
/// visible, as in the reference: ohPipeline `OpenHome/Av/UpnpAv/UpnpAv.cpp:37`),
/// every input the target's rooms offer (and the one the target plays now,
/// should another room's endpoint offer it), and a `NetAux` source named
/// Spotify while the target's group plays a Spotify receiver.
fn sources_of(
    spec: &Spec,
    kind_of: &dyn Fn(&str) -> Option<chorus_protocol::v2::SourceKind>,
) -> Vec<product::Source> {
    let mut sources = vec![
        product::Source {
            system_name: "Playlist".to_string(),
            name: "Playlist".to_string(),
            kind: product::TYPE_PLAYLIST,
            visible: true,
        },
        product::Source {
            system_name: "UpnpAv".to_string(),
            name: "UPnP AV".to_string(),
            kind: product::TYPE_UPNP_AV,
            visible: false,
        },
    ];
    let mut inputs = spec.inputs.clone();
    if let Some(playing) = spec.source.strip_prefix(LINE_IN_PREFIX) {
        if !inputs.iter().any(|(id, _)| id == playing) {
            inputs.push((playing.to_string(), None));
        }
    }
    for (id, label) in &inputs {
        let short = id.split_once('/').map_or(id.as_str(), |(_, input)| input);
        // The input's own name where no other input of the target has it,
        // else the whole literal.
        let unique = inputs
            .iter()
            .filter(|(other, _)| other.split_once('/').map_or(other.as_str(), |(_, i)| i) == short)
            .count()
            == 1;
        sources.push(product::Source {
            system_name: id.clone(),
            name: label.clone().unwrap_or_else(|| {
                if unique {
                    short.to_string()
                } else {
                    id.clone()
                }
            }),
            kind: source_type(kind_of(id)),
            visible: true,
        });
    }
    if spec.source.starts_with(SOLOIST_PREFIX) {
        sources.push(product::Source {
            system_name: "Spotify".to_string(),
            name: "Spotify".to_string(),
            kind: product::TYPE_NET_AUX,
            visible: true,
        });
    }
    sources
}

/// A DIDL-Lite item for a now-playing record: what Info's `Metadata` says
/// about something that is not a track a control point gave (a line-in, the
/// TV, another protocol's source).
fn didl_of(playing: &Playing) -> String {
    let mut x = String::from(concat!(
        r#"<DIDL-Lite xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/" "#,
        r#"xmlns:dc="http://purl.org/dc/elements/1.1/" "#,
        r#"xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/">"#,
        r#"<item id="0" parentID="0" restricted="1">"#
    ));
    let mut tag = |name: &str, value: &Option<String>| {
        if let Some(v) = value {
            x.push_str(&format!("<{name}>{}</{name}>", escape_text(v)));
        }
    };
    tag("dc:title", &playing.title);
    tag("upnp:artist", &playing.artist);
    tag("upnp:album", &playing.album);
    tag("upnp:albumArtURI", &playing.art_url);
    x.push_str("<upnp:class>object.item.audioItem</upnp:class></item></DIDL-Lite>");
    x
}

/// What Info's `Details` says about what the player opened. `BitRate` is the
/// decoded stream's for a lossless source (rate x channels x bits) and 0 for
/// a lossy one: the decoder does not report a compressed bit rate.
fn details_of(info: &MediaInfo) -> Details {
    let bits = u32::from(info.format.bits.unwrap_or(0));
    Details {
        duration_s: info
            .duration_ms
            .map_or(0, |ms| (ms / 1000).min(u64::from(u32::MAX)) as u32),
        bit_rate: info
            .format
            .rate
            .saturating_mul(u32::from(info.format.channels))
            .saturating_mul(bits),
        bit_depth: bits,
        sample_rate: info.format.rate,
        lossless: info.format.bits.is_some(),
        codec_name: info.format.codec.name().to_string(),
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
    /// Whether the OpenHome services are offered (`--upnp-openhome`).
    pub openhome: bool,
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
    /// What kind an offered input (`<endpoint>/<input>`) is.
    input_kind: InputKinds,
    log: Box<dyn Fn(&str) + Send + Sync>,
}

/// Says what kind an offered input, named `<endpoint>/<input>`, is; `None`
/// when it is not offered or nobody knows.
pub type InputKinds = Box<dyn Fn(&str) -> Option<chorus_protocol::v2::SourceKind> + Send + Sync>;

/// What the renderers ask of the rest of the server.
pub struct Hooks {
    /// What kind each offered input is, for the OpenHome source list.
    pub input_kind: InputKinds,
    /// Where the renderers' log lines go.
    pub log: Box<dyn Fn(&str) + Send + Sync>,
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

    /// The transport that drives the renderer's player now.
    fn deck(r: &Renderer) -> &AvTransport {
        match r.oh.deck {
            Deck::UpnpAv => &r.avt,
            Deck::Playlist => r.oh.playlist.deck(),
        }
    }

    fn tag(r: &Renderer) -> u64 {
        (r.base << 32) | (Self::deck(r).epoch() & 0xffff_ffff)
    }

    /// Tell the driving deck that its play could not start.
    fn fail(r: &mut Renderer, reason: &str) {
        let epoch = Self::deck(r).epoch();
        match r.oh.deck {
            Deck::UpnpAv => {
                r.avt.failed(epoch, reason);
            }
            Deck::Playlist => {
                r.oh.playlist.failed(epoch, reason);
            }
        }
    }

    /// Stop the driving deck's state machine without touching the player
    /// (the player is gone, or about to be told by the caller).
    fn halt(r: &mut Renderer) -> Vec<Effect> {
        r.oh.time.stopped();
        match r.oh.deck {
            Deck::UpnpAv => r.avt.stop().unwrap_or_default(),
            Deck::Playlist => r.oh.playlist.halt(),
        }
    }

    /// Give the player to the other deck. The deck that had it goes STOPPED
    /// (ohPipeline: the source that is deactivated stops and events it,
    /// `OpenHome/Av/Playlist/SourcePlaylist.cpp:244-252`,
    /// `OpenHome/Av/UpnpAv/UpnpAv.cpp:100-107`), what it was playing stops,
    /// and the reports still on their way from it are dropped by a new epoch
    /// base. The list, and AVTransport's URIs, are kept.
    fn switch_deck(&self, r: &mut Renderer, to: Deck) {
        if r.oh.deck == to {
            return;
        }
        let engaged = !matches!(
            Self::deck(r).state(),
            TransportState::Stopped | TransportState::NoMediaPresent
        );
        if engaged {
            if let Some(handle) = self.handle_of(r) {
                handle.send(Self::tag(r), Action::Stop);
            }
            self.sessions.suspend(&r.spec.key);
        }
        match r.oh.deck {
            Deck::UpnpAv => {
                let _ = r.avt.stop();
            }
            Deck::Playlist => {
                r.oh.playlist.deactivate();
            }
        }
        r.oh.deck = to;
        if to == Deck::Playlist {
            r.oh.playlist.activate();
        }
        r.oh.fresh = false;
        r.oh.time.stopped();
        r.loaded = false;
        r.queued.clear();
        if r.player.is_some() {
            r.base = self.next_base.fetch_add(1, Ordering::SeqCst) & 0x7fff_ffff;
        }
        r.idle_since_ms = self.now_ms();
        (self.log)(&format!(
            "upnp renderer source target={} source={}",
            r.spec.key,
            match to {
                Deck::UpnpAv => "UpnpAv",
                Deck::Playlist => "Playlist",
            }
        ));
    }

    /// Whether the target's rooms play something that is not this renderer's
    /// own player: a line-in, the TV, a Spotify receiver, another holder's
    /// player.
    fn plays_elsewhere(r: &Renderer) -> bool {
        let source = r.spec.source.as_str();
        if matches!(source, "" | "stream" | "none") {
            return false;
        }
        match (source.strip_prefix("player:"), r.player) {
            (Some(id), Some(index)) => id != player_id(index),
            _ => true,
        }
    }

    /// Bring the OpenHome state in line with the target as the control state
    /// has it: the volume and its limit, the name, the source list, which
    /// source is current, and what Info says of a source that is not a track
    /// of this renderer's.
    fn refresh_openhome(&self, r: &mut Renderer) {
        r.oh.volume.report(r.spec.volume, r.spec.mute, r.spec.limit);
        r.oh.product.set_room(&r.spec.name);
        r.oh.product
            .set_sources(sources_of(&r.spec, &*self.input_kind));
        let source = r.spec.source.as_str();
        let by_input = source
            .strip_prefix(LINE_IN_PREFIX)
            .and_then(|input| r.oh.product.index_of_system_name(input));
        let by_receiver = source
            .starts_with(SOLOIST_PREFIX)
            .then(|| r.oh.product.index_of_kind(product::TYPE_NET_AUX))
            .flatten();
        let index = by_input.or(by_receiver).unwrap_or(match r.oh.deck {
            Deck::Playlist => 0,
            Deck::UpnpAv => 1,
        });
        r.oh.product.set_index(index);
        if Self::plays_elsewhere(r) {
            let metadata = r.spec.playing.as_ref().map_or_else(String::new, didl_of);
            if r.oh.info.follow(source, &metadata) {
                r.oh.time
                    .track(r.spec.playing.as_ref().and_then(|p| p.duration_ms));
            }
        }
    }

    /// A track of the driving deck starts: Info and Time count it.
    fn track_started(r: &mut Renderer) {
        let (uri, metadata) = {
            let (u, m) = Self::deck(r).current();
            (u.to_string(), m.to_string())
        };
        let duration = Self::deck(r).duration_ms();
        r.oh.info.track(&uri, &metadata);
        if let Some(media) = r.oh.media.as_ref().filter(|m| m.uri == uri) {
            r.oh.info.details(details_of(media));
        }
        r.oh.time.track(duration);
        r.oh.fresh = false;
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
                openhome: self.settings.openhome,
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
                self.refresh_openhome(r);
                continue;
            }
            let boot_id = ssdp::boot_id(unix_s(), inner.last_boot.get(&key).copied());
            inner.last_boot.insert(key.clone(), boot_id);
            (self.log)(&format!(
                "upnp renderer appeared target={} udn={} name=\"{}\" bootid={} configid={}",
                spec.key, key, spec.name, boot_id, config_id
            ));
            let mut renderer = Renderer {
                udn: target,
                config_id,
                boot_id,
                description: description::device_description(&info, config_id),
                avt: AvTransport::new(),
                rcs: RenderingControl::new(spec.volume, spec.mute),
                subs: std::array::from_fn(|_| Subscriptions::standard()),
                oh: OpenHome {
                    product: Product::new(
                        &spec.name,
                        model_name(&spec.kind),
                        sources_of(&spec, &*self.input_kind),
                    ),
                    volume: Volume::new(spec.volume, spec.mute, spec.limit),
                    info: Info::new(),
                    time: Time::new(),
                    playlist: Playlist::new(),
                    trackers: std::array::from_fn(|_| Tracker::new()),
                    // The Playlist is the source a new OpenHome device
                    // starts on; without the services there is one deck.
                    deck: if self.settings.openhome {
                        Deck::Playlist
                    } else {
                        Deck::UpnpAv
                    },
                    fresh: false,
                    media: None,
                },
                player: None,
                loaded: false,
                base: 0,
                idle_since_ms: now,
                queued: Vec::new(),
                spec,
            };
            self.refresh_openhome(&mut renderer);
            inner.renderers.insert(key, renderer);
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
            let (u, m) = Self::deck(r).current();
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
                let (u, m) = Self::deck(r).next_queued();
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
            Self::fail(r, &reason);
            return Err(error::ACTION_FAILED);
        }
        Ok(())
    }

    /// Turn the effects of an input to the driving deck (AVTransport's or the
    /// Playlist's) into player actions and control commands.
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
                        r.oh.fresh = true;
                        r.idle_since_ms = self.now_ms();
                        handle.send(tag, Action::Load { uri, mime });
                    }
                    Effect::Start => {
                        r.oh.fresh = true;
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
                        r.oh.time.stopped();
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
        if Self::deck(r).state() == TransportState::NoMediaPresent {
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
                if r.oh.deck == Deck::UpnpAv {
                    if let Some(handle) = self.handle_of(r) {
                        r.avt.position(handle.position_ms());
                    }
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
                if needs_player {
                    // The AVTransport takeover: a URI or a Play makes UPnP AV
                    // the device's source and stops the Playlist (ohPipeline
                    // `OpenHome/Av/UpnpAv/UpnpAv.cpp:128-130`, `:162`).
                    self.switch_deck(r, Deck::UpnpAv);
                    r.oh.product.set_standby(false);
                }
                let (out, effects) = r.avt.invoke(invocation)?;
                if r.oh.deck == Deck::UpnpAv {
                    self.apply_effects(r, effects)?;
                }
                // While the Playlist has the player, AVTransport's other
                // actions change its own variables and nothing that plays
                // (`UpnpAv.cpp:183-266`: `if (IsActive())`).
                self.refresh_openhome(r);
                out
            }
            Service::Playlist => {
                let r = inner.renderers.get_mut(key).ok_or(error::ACTION_FAILED)?;
                if r.oh.deck == Deck::Playlist {
                    if let Some(handle) = self.handle_of(r) {
                        r.oh.playlist.position(handle.position_ms());
                    }
                }
                let seed = crate::session::random_32().map_or(1, |b| {
                    u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
                });
                let name = invocation.action.name;
                let takes_over = Playlist::activates(name);
                // A trial on a copy, as the source it would be.
                let mut trial = r.oh.playlist.clone();
                trial.activate();
                let (_, wanted) = trial.invoke(invocation, seed)?;
                let needs_player = (takes_over || r.oh.deck == Deck::Playlist)
                    && wanted
                        .iter()
                        .any(|e| matches!(e, Effect::Load { .. } | Effect::Start));
                if needs_player && !self.take_player(r) {
                    (self.log)(&format!(
                        "upnp action refused target={} action=Playlist.{} reason=\"no free player: \
                         all {} are in use\"",
                        r.spec.key,
                        name,
                        self.players.len()
                    ));
                    return Err(error::ACTION_FAILED);
                }
                if takes_over {
                    // Play, SeekId and SeekIndex make the Playlist the
                    // device's source and stop UPnP AV (ohPipeline
                    // `OpenHome/Av/Playlist/SourcePlaylist.cpp:273-276`,
                    // `:394-396`).
                    self.switch_deck(r, Deck::Playlist);
                    r.oh.product.set_standby(false);
                }
                let (out, effects) = r.oh.playlist.invoke(invocation, seed)?;
                if r.oh.deck == Deck::Playlist {
                    self.apply_effects(r, effects)?;
                }
                self.refresh_openhome(r);
                out
            }
            Service::Volume => {
                let r = inner.renderers.get_mut(key).ok_or(error::ACTION_FAILED)?;
                let (out, effects) = r.oh.volume.invoke(invocation)?;
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
                // The same volume as RenderingControl's, through the same
                // commands: the real value, clamp included, before the
                // action answers, and both services event it.
                if let Some(state) = state {
                    self.sync(&mut inner, &state);
                }
                out
            }
            Service::Product => {
                let r = inner.renderers.get_mut(key).ok_or(error::ACTION_FAILED)?;
                let (out, effects) = r.oh.product.invoke(invocation)?;
                let mut state = None;
                for effect in effects {
                    match effect {
                        product::Effect::Select { index } => {
                            state = self.select_source(r, index)?;
                        }
                        product::Effect::Standby { on } => {
                            if on {
                                // Standby stops what this renderer plays and
                                // nothing else: chorus has no power state to
                                // enter (the decision record says why the
                                // flag exists at all).
                                let effects = Self::halt(r);
                                let _ = self.apply_effects(r, effects);
                            }
                            r.oh.product.set_standby(on);
                        }
                    }
                }
                match state {
                    Some(state) => self.sync(&mut inner, &state),
                    None => {
                        if let Some(r) = inner.renderers.get_mut(key) {
                            self.refresh_openhome(r);
                        }
                    }
                }
                out
            }
            Service::Info => {
                let r = inner.renderers.get(key).ok_or(error::ACTION_FAILED)?;
                return r.oh.info.invoke(invocation);
            }
            Service::Time => {
                let r = inner.renderers.get_mut(key).ok_or(error::ACTION_FAILED)?;
                self.read_position(r);
                return r.oh.time.invoke(invocation);
            }
        };
        drop(inner);
        self.wake_events();
        Ok(done)
    }

    /// Make the Product source at `index` the current one. A line-in or a
    /// TV input is selected with the room model's own `take`, so the
    /// target's rooms play it (K78) and the state after is returned for the
    /// caller to follow; the Playlist and UPnP AV are this renderer's decks,
    /// and selecting one stops what the rooms played from elsewhere, as the
    /// reference deactivates the source it leaves (ohPipeline
    /// `OpenHome/Av/Product.cpp:429-462`). Nothing starts to play by being
    /// selected. Leaves standby.
    fn select_source(&self, r: &mut Renderer, index: usize) -> Result<Option<String>, UpnpError> {
        let source =
            r.oh.product
                .sources()
                .get(index)
                .cloned()
                .ok_or(error::OH_PRODUCT_SOURCE_NOT_FOUND)?;
        r.oh.product.set_standby(false);
        let target = r.spec.take.clone();
        let take = |source: &str| {
            self.apply_command(&format!(
                r#"{{"v":2,"t":"take","target":"{}","source":"{}"}}"#,
                target, source
            ))
        };
        let state = match source.kind {
            product::TYPE_PLAYLIST | product::TYPE_UPNP_AV => {
                let elsewhere = Self::plays_elsewhere(r);
                self.switch_deck(
                    r,
                    if source.kind == product::TYPE_PLAYLIST {
                        Deck::Playlist
                    } else {
                        Deck::UpnpAv
                    },
                );
                if elsewhere {
                    Some(take("none")?)
                } else {
                    None
                }
            }
            // Listed only while it plays: selecting it changes nothing.
            product::TYPE_NET_AUX => None,
            _ => {
                let state = take(&format!("{LINE_IN_PREFIX}{}", source.system_name))?;
                // The rooms left this renderer's player: its deck stops now,
                // not when the manager next looks.
                let effects = Self::halt(r);
                let _ = self.apply_effects(r, effects);
                Some(state)
            }
        };
        (self.log)(&format!(
            "upnp source selected target={} source=\"{}\" type={}",
            r.spec.key, source.system_name, source.kind
        ));
        Ok(state)
    }

    /// Give Time the played position, while the driving deck plays.
    fn read_position(&self, r: &mut Renderer) {
        if Self::deck(r).state() == TransportState::Playing {
            if let Some(handle) = self.handle_of(r) {
                r.oh.time.position(handle.position_ms());
            }
        }
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
        // What the player opened, for Info's details.
        if let Event::Opened(info) | Event::Boundary(info) = &report.event {
            r.oh.media = Some(info.clone());
        }
        match r.oh.deck {
            Deck::UpnpAv => self.report_to_avtransport(r, epoch, &report.event),
            Deck::Playlist => self.report_to_playlist(r, epoch, &report.event),
        }
        // Info and Time follow the player, whichever deck drives it.
        match &report.event {
            Event::Opened(info) => {
                if !r.oh.fresh && Self::deck(r).current().0 == info.uri {
                    r.oh.info.details(details_of(info));
                }
                let duration = Self::deck(r).duration_ms();
                r.oh.time.duration(duration);
            }
            Event::Started if r.oh.fresh => Self::track_started(r),
            Event::Boundary(_) => Self::track_started(r),
            Event::Title(title) => r.oh.info.metatext(title),
            Event::Ended { .. } | Event::Failed { .. } => r.oh.time.stopped(),
            _ => {}
        }
        self.refresh_openhome(r);
        drop(inner);
        self.wake_events();
    }

    /// One player report for a renderer whose AVTransport drives the player.
    fn report_to_avtransport(&self, r: &mut Renderer, epoch: u64, event: &Event) {
        match event {
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
    }

    /// One player report for a renderer whose Playlist drives the player.
    /// The Playlist decides what follows by itself: at a boundary it hands
    /// the player the track after the one that just became audible, with no
    /// control point in the loop.
    fn report_to_playlist(&self, r: &mut Renderer, epoch: u64, event: &Event) {
        match event {
            Event::Opened(info) => {
                r.oh.playlist
                    .media_opened(epoch, info.duration_ms, info.seekable);
            }
            Event::Started => {
                r.oh.playlist.playing(epoch);
            }
            Event::Boundary(info) => {
                let effects =
                    r.oh.playlist
                        .track_boundary(epoch, &info.uri, info.duration_ms, info.seekable);
                let (uri, metadata) = r.oh.playlist.deck().current();
                let (hints, _) = hints_of(uri, metadata);
                self.sessions.set_metadata(&r.spec.key, hints);
                let _ = self.apply_effects(r, effects);
            }
            Event::NextOpened(_) => {}
            Event::NextFailed { reason } => {
                (self.log)(&format!(
                    "upnp playlist track failed target={} reason=\"{}\"",
                    r.spec.key, reason
                ));
                r.oh.playlist.next_failed(epoch, reason);
            }
            Event::Ended { .. } => {
                let effects = r.oh.playlist.ended(epoch);
                let _ = self.apply_effects(r, effects);
            }
            Event::Failed { reason } => {
                (self.log)(&format!(
                    "upnp media failed target={} reason=\"{}\"",
                    r.spec.key, reason
                ));
                r.oh.playlist.failed(epoch, reason);
            }
            Event::SeekRefused { .. } => {
                r.oh.playlist.playing(epoch);
            }
            Event::Title(_) | Event::SeekDone { .. } => {}
        }
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
                    Self::deck(r).state(),
                    TransportState::Stopped | TransportState::NoMediaPresent
                ) {
                    let _ = Self::halt(r);
                    changed = true;
                    (self.log)(&format!(
                        "upnp renderer stopped target={} reason=\"its rooms play something else\"",
                        r.spec.key
                    ));
                }
                continue;
            }
            let idle = matches!(
                Self::deck(r).state(),
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

    /// The evented variables of an OpenHome service with their values now.
    fn evented(r: &Renderer, service: Service) -> Vec<Property> {
        match service {
            Service::Product => r.oh.product.evented(),
            Service::Volume => r.oh.volume.evented(),
            Service::Info => r.oh.info.evented(),
            Service::Time => r.oh.time.evented(),
            Service::Playlist => r.oh.playlist.evented(),
            _ => Vec::new(),
        }
    }

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
            // OpenHome: every evented variable, one property each, no
            // LastChange (ohNet `OpenHome/Net/Device/DviSubscription.cpp`
            // lines 219-258, 393-407).
            _ => {
                let vars = Self::evented(r, service);
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
        if service == Service::Time {
            self.read_position(r);
        }
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
            if !self.settings.openhome {
                continue;
            }
            // The OpenHome services: each variable that changed since the
            // service last evented, in one message per service, to every
            // subscriber that has had its initial event. Time's position is
            // read only while somebody subscribes to Time: it changes once a
            // second while playing, and that is the event's cadence.
            if !r.subs[service_index(Service::Time)].is_empty() {
                self.read_position(r);
            }
            for (i, service) in Service::OPENHOME.into_iter().enumerate() {
                let current = Self::evented(r, service);
                let moderated: &[&str] = if service == Service::Playlist {
                    &["IdArray"]
                } else {
                    &[]
                };
                let changed = r.oh.trackers[i].poll(&current, now, moderated, ID_ARRAY_HOLD_MS);
                if changed.is_empty() {
                    r.subs[service_index(service)].expire(now);
                    continue;
                }
                let notifies = r.subs[service_index(service)].event(now);
                if notifies.is_empty() {
                    continue;
                }
                let pairs: Vec<(&str, &str)> =
                    changed.iter().map(|(n, v)| (*n, v.as_str())).collect();
                let body = Arc::new(gena::propertyset(&pairs));
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
            let held = r.oh.trackers.iter().filter_map(Tracker::due_ms).min();
            for at in [r.avt.events().due_ms(), r.rcs.events().due_ms(), held]
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
            openhome: self.shared.settings.openhome,
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
        // A request that named a lower version of the service than the one
        // announced is answered in the version it named.
        let version = soap::requested_version(service, &action).unwrap_or(service.version());
        match result {
            Ok(out) => {
                let mut reply = Reply::xml(
                    200,
                    "OK",
                    soap::build_response_at(service, version, &action.action, &out),
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
        // The OpenHome services exist only when they are switched on.
        if let Resource::Scpd(service) | Resource::Control(service) | Resource::Event(service) =
            resource
        {
            if !Service::offered(self.settings.openhome).contains(&service) {
                return not_found();
            }
        }
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
    /// player sessions over its players (goal 17: an alarm's stored stream
    /// URL plays through the same one). `reports` is the players' one report
    /// stream (`Players::take_reports`). `hooks` is what the renderers ask
    /// of the rest of the server.
    pub fn new(
        sockets: Sockets,
        flags: &UpnpFlags,
        server_id: String,
        state: Arc<ControlState>,
        sessions: Arc<PlayerSessions>,
        reports: Receiver<PlayerReport>,
        hooks: Hooks,
    ) -> Upnp {
        let Hooks { input_kind, log } = hooks;
        let players = Arc::clone(sessions.players());
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
                openhome: flags.openhome,
            },
            state,
            players,
            sessions,
            inner: Mutex::new(Inner::default()),
            started: Instant::now(),
            next_base: AtomicU64::new(1),
            server: server_token(),
            wake_events,
            input_kind,
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
             callback_subnets={} loopback_callbacks={} identity={} openhome={}",
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
            s.server_id,
            if s.openhome { "on" } else { "off" }
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
    fn a_target_knows_its_limit_its_source_and_the_inputs_its_rooms_offer() {
        let state = concat!(
            r#"{"v":2,"t":"state","serial":9,"zones":["#,
            r#"{"id":"kitchen","name":"Kitchen","group":"live-1","volume":0.400,"muted":false,"#,
            r#""endpoints":["amp","sub"],"limit":0.800,"effective_limit":0.600},"#,
            r#"{"id":"den","name":"Den","group":"live-1","volume":0.600,"muted":false,"#,
            r#""endpoints":["hub"],"limit":1.000,"effective_limit":1.000},"#,
            r#"{"id":"study","name":"study","group":"study","volume":1.000,"muted":false,"#,
            r#""endpoints":[]}],"#,
            r#""groups":[{"id":"live-1","kind":"live","zones":["kitchen","den"],"volume":0.500,"#,
            r#""source":"line-in:hub/tv","audio":"127.0.0.1:4010","now_playing":{"title":"TV","#,
            r#""artist":null,"album":null,"art_url":null,"duration_ms":null,"state":"playing","#,
            r#""via":"streamer"}},"#,
            r#"{"id":"study","kind":"room","zones":["study"],"volume":1.000,"source":"stream","#,
            r#""audio":"127.0.0.1:4010"}],"saved_groups":[],"#,
            r#""inputs":["amp/line-1",{"id":"hub/tv","name":"Television"},"other/line-9"]}"#
        );
        let (_, specs) = specs_of(state).unwrap();
        let by = |key: &str| specs.iter().find(|s| s.key == key).unwrap();
        // A room: its effective limit, its own endpoints' inputs, and what
        // its group plays.
        let kitchen = by("room:kitchen");
        assert_eq!(kitchen.limit, 600);
        assert_eq!(kitchen.inputs, [("amp/line-1".to_string(), None)]);
        assert_eq!(kitchen.source, "line-in:hub/tv");
        assert_eq!(
            kitchen.playing.as_ref().and_then(|p| p.title.as_deref()),
            Some("TV")
        );
        // A room with no limit in the state is not limited; an input of an
        // endpoint in no room of the target is not the target's.
        let study = by("room:study");
        assert_eq!((study.limit, study.inputs.len()), (1000, 0));
        assert_eq!((study.source.as_str(), &study.playing), ("stream", &None));
        // A group: the average of its rooms' limits, the inputs of all its
        // rooms, an input's label where the state gives one.
        let live = by("live:den+kitchen");
        assert_eq!(live.limit, 800);
        assert_eq!(
            live.inputs,
            [
                ("amp/line-1".to_string(), None),
                ("hub/tv".to_string(), Some("Television".to_string()))
            ]
        );
    }

    #[test]
    fn the_sources_are_the_playlist_upnp_av_the_inputs_by_kind_and_spotify_while_it_plays() {
        use chorus_protocol::v2::SourceKind;
        let spec = |source: &str, inputs: &[(&str, Option<&str>)]| Spec {
            key: "room:kitchen".into(),
            kind: Kind::Room,
            name: "Kitchen".into(),
            rooms: vec!["kitchen".into()],
            take: "kitchen".into(),
            group: None,
            volume: 400,
            mute: false,
            limit: 1000,
            source: source.into(),
            playing: None,
            inputs: inputs
                .iter()
                .map(|(id, label)| (id.to_string(), label.map(str::to_string)))
                .collect(),
        };
        let kinds = |id: &str| match id {
            "amp/line-1" => Some(SourceKind::LineIn),
            "hub/optical" => Some(SourceKind::Optical),
            "hub/arc" => Some(SourceKind::HdmiArc),
            _ => None,
        };
        let listed = |spec: &Spec| -> Vec<(String, String, &'static str, bool)> {
            sources_of(spec, &kinds)
                .into_iter()
                .map(|s| (s.system_name, s.name, s.kind, s.visible))
                .collect()
        };
        let row = |system: &str, name: &str, kind: &'static str, visible: bool| {
            (system.to_string(), name.to_string(), kind, visible)
        };
        let fixed = [
            row("Playlist", "Playlist", "Playlist", true),
            row("UpnpAv", "UPnP AV", "UpnpAv", false),
        ];
        assert_eq!(listed(&spec("stream", &[])), fixed);
        let inputs = [
            ("amp/line-1", None),
            ("hub/optical", Some("Television")),
            ("hub/arc", None),
        ];
        let mut expected = fixed.to_vec();
        expected.extend([
            row("amp/line-1", "line-1", "Analog", true),
            row("hub/optical", "Television", "Digital", true),
            row("hub/arc", "arc", "Hdmi", true),
        ]);
        assert_eq!(listed(&spec("stream", &inputs)), expected);
        // A Spotify receiver playing in the target's group is one more
        // source, a NetAux called Spotify, and only then. The spelling is
        // goal 17's Soloist track's; nothing else here depends on it.
        let mut with_spotify = expected.clone();
        with_spotify.push(row("Spotify", "Spotify", "NetAux", true));
        assert_eq!(listed(&spec("soloist:r0", &inputs)), with_spotify);
        assert_eq!(listed(&spec("player:p0", &inputs)), expected);
        // An input the target plays that another room's endpoint offers is
        // listed too (kind unknown: analogue), and two inputs of one name
        // are told apart by their endpoints.
        let twins = [("amp/line-1", None), ("den-amp/line-1", None)];
        let named: Vec<String> = sources_of(&spec("line-in:far/aux", &twins), &kinds)
            .into_iter()
            .skip(2)
            .map(|s| format!("{}={}:{}", s.system_name, s.name, s.kind))
            .collect();
        assert_eq!(
            named,
            [
                "amp/line-1=amp/line-1:Analog",
                "den-amp/line-1=den-amp/line-1:Analog",
                "far/aux=aux:Analog"
            ]
        );
    }

    #[test]
    fn a_now_playing_record_becomes_a_didl_item_for_info() {
        let item = didl_of(&Playing {
            title: Some("A & B".into()),
            artist: Some("C".into()),
            album: None,
            art_url: None,
            duration_ms: Some(1000),
        });
        let read = didl::parse(&item).expect("DIDL-Lite");
        assert_eq!(read.title.as_deref(), Some("A & B"));
        assert_eq!(read.artist.as_deref(), Some("C"));
        assert_eq!(read.album, None);
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
