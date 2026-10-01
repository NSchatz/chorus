//! The TV relay (goal 13, the TV path): a TV input played in low-latency
//! mode goes from its hub to the server and on to every player of the one
//! room that plays it as UDP datagrams (`docs/protocol.md`, "Low-latency
//! path"; ADR 0091 for the wire), never on the slot grid.
//!
//! # The path
//!
//! 1. The conductor decides, each pass, which TV inputs play in low-latency
//!    mode ([`TvPlay`]): a TV's input (`optical` or `hdmi_arc`) started and
//!    streaming, its group one room, the room wired, the input's autoplay
//!    rule (if any) saying `low_latency`, and every player of the room and
//!    the hub advertising `capabilities.features` `low_latency`. Everything
//!    else plays on the slot grid (ADR 0079), unchanged. It hands the list to
//!    [`TvRelay::reconcile`].
//! 2. A new play is offered to the room's players first (`0x16`, direction
//!    `to_endpoint`, a fresh key and tag each), and only when every one of
//!    them accepted (`0x17` with its UDP port) to the hub (direction
//!    `from_endpoint`, this relay's port). The hub switches its upstream to
//!    datagrams at its accept, so the room is never left with a hub sending
//!    to a relay that has nobody to send to. A refusal or an offer not
//!    answered within [`OFFER_TIMEOUT_MS`] ends every stream of the play
//!    (direction `end`) and leaves it on the slot grid until what it is made
//!    of changes (`tv-path refused`).
//! 3. The relay thread ([`TvRelay::run`]) opens each datagram from the hub
//!    (`Opener`: its form, its tag, the replay window, the AEAD), hands it to
//!    the FEC (`FecDecoder`), and restamps every chunk the decoder hands on:
//!    `play_at = capture_stamp + lead`, the lead being
//!    `chorus_control::theater::tv_play_at_lead_ns(L_tv, floor, av_trim_ms)`
//!    (`av-trim-clamped` when the floor clamps it). A chunk whose `play_at`
//!    has already passed is late and never sent. Each restamped chunk goes
//!    through each player's `FecRelay` (the hub's groups are kept, so the
//!    FEC wait is paid once end to end) and `Sealer`, to the player's port.
//!    As the playout point passes a block of groups, `expire_before` closes
//!    them, counting what they still miss as unrecoverable.
//! 4. Volume and sound stay the endpoints': `room_volume` and `sound` reach
//!    them on their sessions as they always do, and they apply both to what
//!    they play from here.
//!
//! # Clocks
//!
//! Only the server's monotonic timeline ([`MonotonicTimeline`]): the stamps
//! the hub sends are on it (the hub maps its capture instants through the
//! sync offset), and the playout point is read from it. Listed in
//! `audio-path.conf`. No wall clock is read here.
//!
//! # Test-only loss
//!
//! `--udp-loss <ppm>,<seed>` ([`Loss`]) drops datagrams on both legs from a
//! seeded generator: each the relay receives from a hub (before it is
//! opened) and each it sends to a player (after it is sealed). It exists so
//! the end-to-end tests can show the FEC repairing what a LAN loses; a
//! deployment never sets it.

use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use chorus_audio::MonotonicTimeline;
use chorus_control::rooms::InputId;
use chorus_control::theater::{tv_play_at_lead_ns, AV_TRIM_CLAMPED};
use chorus_protocol::v2::lowlat::{
    chunk_fits, FecDecoder, FecParams, FecRelay, Header, Opener, Plan, Sealer, KEY_LEN,
    MAX_DATAGRAM_LEN,
};
use chorus_protocol::v2::{
    LowLatencyAccept, LowLatencyDirection, LowLatencyOffer, LowLatencyStatus, Message,
};
use chorus_protocol::SampleFormat;

/// How long an offer waits for its `0x17`, ms. ASSUMED: an endpoint answers
/// inside its session's reader turn (milliseconds); two seconds is a stalled
/// endpoint, not a slow one.
pub const OFFER_TIMEOUT_MS: u64 = 2_000;

/// How long one receive waits before the relay thread looks at its clocks
/// (timeouts, the playout point) again. ASSUMED: under one 2.5 ms chunk.
const RECEIVE_WAIT: Duration = Duration::from_millis(1);

/// Where the stamp sits in an `audio_chunk` payload (`docs/protocol.md`,
/// 0x02: `sequence` u32 at 0, `timestamp_ns` u64 at 4, big-endian).
const STAMP_AT: std::ops::Range<usize> = 4..12;

/// Send a v2 message on a session (the router's `push_message`): whether it
/// was queued.
pub type SendTo = Box<dyn Fn(u64, &Message) -> bool + Send + Sync>;

/// One TV input the conductor wants on the low-latency path now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TvPlay {
    /// The input.
    pub input: InputId,
    /// The one room that plays it.
    pub room: String,
    /// The hub's session and its TCP peer's address.
    pub hub: (u64, IpAddr),
    /// The room's player sessions and their addresses, by session id.
    pub players: Vec<(u64, IpAddr)>,
    /// The room's A/V trim, ms (positive delays the audio).
    pub trim_ms: i16,
}

impl TvPlay {
    /// What a play is made of, apart from its trim: a play whose sessions
    /// change is a new play (new offers); a trim moves the lead in place.
    fn same_streams(&self, other: &TvPlay) -> bool {
        self.input == other.input
            && self.room == other.room
            && self.hub == other.hub
            && self.players == other.players
    }
}

/// The relay's counts for one play, for its status line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RelayStats {
    /// Datagrams received from the hub (before the test-only loss).
    pub received: u64,
    /// Upstream datagrams the test-only loss dropped.
    pub dropped_up: u64,
    /// Chunks the upstream decoder handed on.
    pub delivered: u64,
    /// Of those, rebuilt from a parity.
    pub recovered: u64,
    /// Chunks of closed groups never received nor rebuilt.
    pub unrecoverable: u64,
    /// Chunks restamped and sent on.
    pub relayed: u64,
    /// Chunks whose play-at had passed when they were ready: never sent.
    pub late: u64,
    /// Datagrams sent to players.
    pub sent: u64,
    /// Downstream datagrams the test-only loss dropped.
    pub dropped_down: u64,
    /// Datagrams that did not open (form, tag, replay, AEAD).
    pub refused: u64,
    /// The age of a chunk when the relay handed it on (relay time less its
    /// capture stamp), us: the largest, and the sum for the mean. What the
    /// hub's capture, the upstream leg and any FEC wait cost; on a test
    /// host, a scheduling figure, not timing evidence.
    pub age_max_us: u64,
    /// See [`RelayStats::age_max_us`].
    pub age_sum_us: u64,
}

impl RelayStats {
    fn line(&self) -> String {
        format!(
            "received={} dropped_up={} delivered={} recovered={} unrecoverable={} relayed={} \
             late={} sent={} dropped_down={} refused={} age_mean_us={} age_max_us={}",
            self.received,
            self.dropped_up,
            self.delivered,
            self.recovered,
            self.unrecoverable,
            self.relayed,
            self.late,
            self.sent,
            self.dropped_down,
            self.refused,
            self.age_sum_us / (self.relayed + self.late).max(1),
            self.age_max_us
        )
    }
}

/// The seeded drop of `--udp-loss` (tests only): splitmix64, so a run is
/// reproducible from its seed and needs no dependency.
#[derive(Debug, Clone)]
pub struct Loss {
    ppm: u32,
    state: u64,
}

impl Loss {
    /// Drop `ppm` per million, from `seed`.
    pub fn new(ppm: u32, seed: u64) -> Loss {
        Loss { ppm, state: seed }
    }

    /// Whether the next datagram is dropped.
    pub fn drop_next(&mut self) -> bool {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z % 1_000_000) < u64::from(self.ppm)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Offers out to the players; the hub not yet asked.
    Players { deadline_ns: u64 },
    /// Every player accepted; the hub's offer is out.
    Hub { deadline_ns: u64 },
    /// The hub accepted: datagrams flow.
    Active,
}

struct Down {
    session: u64,
    tag: u32,
    ip: IpAddr,
    port: Option<u16>,
    sealer: Sealer,
    fec: FecRelay,
}

struct Relay {
    play: TvPlay,
    lead_ns: u64,
    phase: Phase,
    up_tag: u32,
    up_key: [u8; KEY_LEN],
    opener: Opener,
    decoder: FecDecoder,
    downs: Vec<Down>,
    /// The newest chunk handed on: its number in the stream and its capture
    /// stamp. Stamps are on a grid of one chunk from there.
    base: Option<(u64, u64)>,
    said_first: bool,
    stats: RelayStats,
}

#[derive(Default)]
struct Inner {
    relays: Vec<Relay>,
    /// Plays refused or timed out, not offered again while they are what the
    /// conductor wants.
    refused: Vec<TvPlay>,
}

/// The server's TV relay: one UDP socket, the plays on it, and their thread.
pub struct TvRelay {
    socket: UdpSocket,
    port: u16,
    plan: Plan,
    fec: FecParams,
    shape: (u32, u16, SampleFormat),
    timeline: MonotonicTimeline,
    send: SendTo,
    wake: Box<dyn Fn() + Send + Sync>,
    say: Box<dyn Fn(&str) + Send + Sync>,
    key: Box<dyn Fn() -> Option<[u8; KEY_LEN]> + Send + Sync>,
    loss: Mutex<Option<(Loss, Loss)>>,
    next_tag: AtomicU32,
    inner: Mutex<Inner>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// What [`TvRelay::new`] is built from.
pub struct RelaySetup {
    /// The bound socket.
    pub socket: UdpSocket,
    /// The plan (chunk, FEC, `L_tv`, the floor).
    pub plan: Plan,
    /// This server's stream: rate, channels, sample format. A TV input
    /// plays only in this format (`crate::linein` refuses any other).
    pub shape: (u32, u16, SampleFormat),
    /// The server timeline.
    pub timeline: MonotonicTimeline,
    /// Send a v2 message on a session (the router's `push_message`).
    pub send: SendTo,
    /// Wake the conductor (a play refused, timed out or ended).
    pub wake: Box<dyn Fn() + Send + Sync>,
    /// Where status lines go.
    pub say: Box<dyn Fn(&str) + Send + Sync>,
    /// A fresh random key per offer (`/dev/urandom`).
    pub key: Box<dyn Fn() -> Option<[u8; KEY_LEN]> + Send + Sync>,
    /// `--udp-loss` (tests only): ppm and seed.
    pub loss: Option<(u32, u64)>,
}

impl TvRelay {
    /// A relay on `setup.socket`. The plan's FEC shape was checked when the
    /// configuration was parsed.
    pub fn new(setup: RelaySetup) -> std::io::Result<TvRelay> {
        let port = setup.socket.local_addr()?.port();
        setup.socket.set_read_timeout(Some(RECEIVE_WAIT))?;
        let fec = setup
            .plan
            .fec()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string()))?;
        Ok(TvRelay {
            socket: setup.socket,
            port,
            plan: setup.plan,
            fec,
            shape: setup.shape,
            timeline: setup.timeline,
            send: setup.send,
            wake: setup.wake,
            say: setup.say,
            key: setup.key,
            // The two legs draw from two generators, so the loss on one does
            // not depend on how much traffic the other carried.
            loss: Mutex::new(
                setup
                    .loss
                    .map(|(ppm, seed)| (Loss::new(ppm, seed), Loss::new(ppm, seed ^ 0x5DEE_CE66))),
            ),
            next_tag: AtomicU32::new(1),
            inner: Mutex::new(Inner::default()),
        })
    }

    /// The UDP port the hubs send to.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The plan this relay runs.
    pub fn plan(&self) -> Plan {
        self.plan
    }

    /// Whether a TV in this server's format fits one datagram per chunk.
    pub fn fits(&self) -> bool {
        chunk_fits(self.plan.chunk_frames, self.shape.1, self.shape.2)
    }

    /// Whether `input` plays on the low-latency path now (its hub accepted).
    pub fn active(&self, input: &InputId) -> bool {
        lock(&self.inner)
            .relays
            .iter()
            .any(|r| r.play.input == *input && r.phase == Phase::Active)
    }

    /// Whether `play` was refused or timed out and is not offered again.
    pub fn refused(&self, play: &TvPlay) -> bool {
        lock(&self.inner)
            .refused
            .iter()
            .any(|r| r.same_streams(play))
    }

    fn tag(&self) -> u32 {
        loop {
            let t = self.next_tag.fetch_add(1, Ordering::Relaxed);
            if t != 0 {
                return t;
            }
        }
    }

    fn offer(&self, direction: LowLatencyDirection, tag: u32, key: [u8; KEY_LEN]) -> Message {
        Message::LowLatencyOffer(LowLatencyOffer {
            direction,
            stream_tag: tag,
            key,
            udp_port: if direction == LowLatencyDirection::FromEndpoint {
                self.port
            } else {
                0
            },
            chunk_frames: self.plan.chunk_frames,
            fec_k: self.plan.fec_k,
            fec_depth: self.plan.fec_depth,
            latency_ns: self.plan.l_tv_ns,
        })
    }

    fn end_message(tag: u32) -> Message {
        Message::LowLatencyOffer(LowLatencyOffer {
            direction: LowLatencyDirection::End,
            stream_tag: tag,
            key: [0u8; KEY_LEN],
            udp_port: 0,
            chunk_frames: 0,
            fec_k: 0,
            fec_depth: 0,
            latency_ns: 0,
        })
    }

    fn lead(&self, play: &TvPlay) -> (u64, bool) {
        tv_play_at_lead_ns(self.plan.l_tv_ns, self.plan.floor_ns(), play.trim_ms)
    }

    fn say_lead(&self, play: &TvPlay, lead_ns: u64, clamped: bool) {
        if clamped {
            (self.say)(&format!(
                "{} input={} room={} trim_ms={} l_tv_ns={} floor_ns={} lead_ns={}",
                AV_TRIM_CLAMPED,
                play.input.literal(),
                play.room,
                play.trim_ms,
                self.plan.l_tv_ns,
                self.plan.floor_ns(),
                lead_ns
            ));
        }
    }

    /// Make the plays on the low-latency path what the conductor wants:
    /// plays no longer wanted (or whose sessions changed) end, a changed trim
    /// moves the lead, and a new play is offered to its players. Returns the
    /// inputs that just stopped playing on this path.
    pub fn reconcile(&self, wanted: &[TvPlay]) -> Vec<InputId> {
        let mut inner = lock(&self.inner);
        // A refusal is remembered only while it is what is wanted.
        inner
            .refused
            .retain(|r| wanted.iter().any(|w| w.same_streams(r)));
        let mut ended = Vec::new();
        let mut keep = Vec::new();
        for mut relay in std::mem::take(&mut inner.relays) {
            match wanted.iter().find(|w| w.same_streams(&relay.play)) {
                Some(w) => {
                    if w.trim_ms != relay.play.trim_ms {
                        relay.play.trim_ms = w.trim_ms;
                        let (lead_ns, clamped) = self.lead(w);
                        relay.lead_ns = lead_ns;
                        (self.say)(&format!(
                            "tv-relay lead input={} room={} trim_ms={} lead_ns={} clamped={}",
                            w.input.literal(),
                            w.room,
                            w.trim_ms,
                            lead_ns,
                            u8::from(clamped)
                        ));
                        self.say_lead(w, lead_ns, clamped);
                    }
                    keep.push(relay);
                }
                None => {
                    self.end(&relay, "not-wanted");
                    ended.push(relay.play.input.clone());
                }
            }
        }
        inner.relays = keep;
        for w in wanted {
            let known = inner.relays.iter().any(|r| r.play.same_streams(w))
                || inner.refused.iter().any(|r| r.same_streams(w));
            if known {
                continue;
            }
            match self.start(w) {
                Some(relay) => inner.relays.push(relay),
                None => inner.refused.push(w.clone()),
            }
        }
        ended
    }

    fn start(&self, play: &TvPlay) -> Option<Relay> {
        let (lead_ns, clamped) = self.lead(play);
        let deadline_ns = self.timeline.now_ns() + OFFER_TIMEOUT_MS * 1_000_000;
        let mut downs = Vec::new();
        for &(session, ip) in &play.players {
            let key = (self.key)()?;
            let tag = self.tag();
            let sealer = Sealer::new(key, tag).ok()?;
            downs.push(Down {
                session,
                tag,
                ip,
                port: None,
                sealer,
                fec: FecRelay::new(self.fec),
            });
            let sent = (self.send)(
                session,
                &self.offer(LowLatencyDirection::ToEndpoint, tag, key),
            );
            if !sent {
                (self.say)(&format!(
                    "tv-path refused input={} room={} session={} reason=offer-not-sent",
                    play.input.literal(),
                    play.room,
                    session
                ));
                for d in &downs {
                    (self.send)(d.session, &TvRelay::end_message(d.tag));
                }
                return None;
            }
        }
        // The hub's key and tag are fixed now; its offer waits for the
        // players.
        let up_key = (self.key)()?;
        let up_tag = self.tag();
        (self.say)(&format!(
            "tv-relay offer input={} room={} players={} hub_session={} chunk_frames={} fec_k={} \
             fec_depth={} l_tv_ns={} trim_ms={} lead_ns={} port={}",
            play.input.literal(),
            play.room,
            play.players.len(),
            play.hub.0,
            self.plan.chunk_frames,
            self.plan.fec_k,
            self.plan.fec_depth,
            self.plan.l_tv_ns,
            play.trim_ms,
            lead_ns,
            self.port
        ));
        self.say_lead(play, lead_ns, clamped);
        Some(Relay {
            play: play.clone(),
            lead_ns,
            phase: Phase::Players { deadline_ns },
            up_tag,
            up_key,
            opener: Opener::new(up_key, up_tag),
            decoder: FecDecoder::new(self.fec),
            downs,
            base: None,
            said_first: false,
            stats: RelayStats::default(),
        })
    }

    /// End every stream of `relay` (direction `end`) and say what it did.
    fn end(&self, relay: &Relay, reason: &str) {
        for d in &relay.downs {
            (self.send)(d.session, &TvRelay::end_message(d.tag));
        }
        // The hub was offered only once every player accepted.
        if !matches!(relay.phase, Phase::Players { .. }) {
            (self.send)(relay.play.hub.0, &TvRelay::end_message(relay.up_tag));
        }
        let mut stats = relay.stats;
        let f = relay.decoder.stats();
        stats.delivered = f.delivered;
        stats.recovered = f.recovered;
        stats.unrecoverable = f.unrecoverable;
        (self.say)(&format!(
            "tv-relay end input={} room={} reason={} {}",
            relay.play.input.literal(),
            relay.play.room,
            reason,
            stats.line()
        ));
    }

    /// A session's `0x17`.
    pub fn accept(&self, session: u64, accept: &LowLatencyAccept) {
        let mut inner = lock(&self.inner);
        let deadline_ns = self.timeline.now_ns() + OFFER_TIMEOUT_MS * 1_000_000;
        let Some(at) = inner.relays.iter().position(|r| {
            (r.play.hub.0 == session && r.up_tag == accept.stream_tag)
                || r.downs
                    .iter()
                    .any(|d| d.session == session && d.tag == accept.stream_tag)
        }) else {
            return;
        };
        if accept.status != LowLatencyStatus::Accepted {
            let relay = inner.relays.remove(at);
            (self.say)(&format!(
                "tv-path refused input={} room={} session={} status={}",
                relay.play.input.literal(),
                relay.play.room,
                session,
                accept.status.name()
            ));
            self.end(&relay, "refused");
            inner.refused.push(relay.play.clone());
            drop(inner);
            (self.wake)();
            return;
        }
        let relay = &mut inner.relays[at];
        if relay.play.hub.0 == session && relay.up_tag == accept.stream_tag {
            if matches!(relay.phase, Phase::Hub { .. }) {
                relay.phase = Phase::Active;
                (self.say)(&format!(
                    "tv-relay active input={} room={} stream_tag={} players={} lead_ns={}",
                    relay.play.input.literal(),
                    relay.play.room,
                    relay.up_tag,
                    relay.downs.len(),
                    relay.lead_ns
                ));
            }
            return;
        }
        if let Some(d) = relay
            .downs
            .iter_mut()
            .find(|d| d.session == session && d.tag == accept.stream_tag)
        {
            d.port = Some(accept.udp_port);
        }
        if matches!(relay.phase, Phase::Players { .. })
            && relay.downs.iter().all(|d| d.port.is_some())
        {
            // Every player is listening: now the hub may switch.
            let key = relay.up_key;
            let message = self.offer(LowLatencyDirection::FromEndpoint, relay.up_tag, key);
            relay.phase = Phase::Hub { deadline_ns };
            let hub = relay.play.hub.0;
            if !(self.send)(hub, &message) {
                (self.say)(&format!(
                    "tv-path refused input={} room={} session={} reason=offer-not-sent",
                    relay.play.input.literal(),
                    relay.play.room,
                    hub
                ));
            }
        }
    }

    /// The relay thread: until `keep` goes false, receive, open, decode,
    /// restamp and send on; between datagrams, close what the playout point
    /// passed and end offers that timed out.
    pub fn run(&self, keep: &AtomicBool) {
        let mut buf = vec![0u8; MAX_DATAGRAM_LEN + 1];
        while keep.load(Ordering::SeqCst) {
            match self.socket.recv_from(&mut buf) {
                Ok((n, from)) => self.datagram(&buf[..n], from),
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(_) => std::thread::sleep(RECEIVE_WAIT),
            }
            self.housekeeping();
        }
    }

    fn housekeeping(&self) {
        let now = self.timeline.now_ns();
        let mut inner = lock(&self.inner);
        let mut timed_out = Vec::new();
        inner.relays.retain(|r| match r.phase {
            Phase::Players { deadline_ns } | Phase::Hub { deadline_ns } if now > deadline_ns => {
                (self.say)(&format!(
                    "tv-path refused input={} room={} reason=offer-timeout timeout_ms={}",
                    r.play.input.literal(),
                    r.play.room,
                    OFFER_TIMEOUT_MS
                ));
                self.end(r, "offer-timeout");
                timed_out.push(r.play.clone());
                false
            }
            _ => true,
        });
        // Not offered again while it is what the conductor wants.
        let any_timed_out = !timed_out.is_empty();
        inner.refused.extend(timed_out);
        let block = self.fec.block_len();
        let depth = u64::from(self.fec.depth());
        let chunk_ns = self.plan.chunk_ns().max(1);
        for r in inner.relays.iter_mut() {
            // Close every block whose last chunk's play-at has passed.
            if let Some((n0, s0)) = r.base {
                let horizon = now.saturating_sub(r.lead_ns);
                if horizon > s0 {
                    let n = n0 + (horizon - s0) / chunk_ns;
                    let first_open = (n / block) * depth;
                    if let Ok(g) = u32::try_from(first_open) {
                        r.decoder.expire_before(g);
                    }
                }
            }
        }
        if any_timed_out {
            drop(inner);
            (self.wake)();
        }
    }

    fn datagram(&self, bytes: &[u8], from: SocketAddr) {
        let Ok(header) = Header::parse(bytes) else {
            return;
        };
        let mut inner = lock(&self.inner);
        let Some(relay) = inner.relays.iter_mut().find(|r| {
            r.phase == Phase::Active && r.up_tag == header.stream_tag && r.play.hub.1 == from.ip()
        }) else {
            return;
        };
        relay.stats.received += 1;
        {
            let mut loss = lock(&self.loss);
            if let Some((up, _)) = loss.as_mut() {
                if up.drop_next() {
                    relay.stats.dropped_up += 1;
                    return;
                }
            }
        }
        let opened = match relay.opener.open(bytes) {
            Ok(o) => o,
            Err(_) => {
                relay.stats.refused += 1;
                return;
            }
        };
        let delivered = relay.decoder.push(opened.kind, &opened.plaintext);
        let now = self.timeline.now_ns();
        for mut d in delivered {
            if d.payload.len() < STAMP_AT.end {
                continue;
            }
            let mut stamp = [0u8; 8];
            stamp.copy_from_slice(&d.payload[STAMP_AT]);
            let capture_ns = u64::from_be_bytes(stamp);
            // The newest chunk maps chunk numbers onto the timeline (for
            // `expire_before`): a hub's relock restarts its stamps on a fresh
            // grid (ADR 0090), and the mapping moves with it, said once per
            // move (`capture-grid`).
            if let Some((i, c)) = relay.base {
                let on_grid = i128::from(c)
                    + (i128::from(d.chunk_index) - i128::from(i))
                        * i128::from(self.plan.chunk_ns());
                if on_grid != i128::from(capture_ns) {
                    let mut seq = [0u8; 4];
                    seq.copy_from_slice(&d.payload[0..4]);
                    (self.say)(&format!(
                        "tv-relay capture-grid input={} chunk={} sequence={} capture_ns={} \
                         moved_ns={}",
                        relay.play.input.literal(),
                        d.chunk_index,
                        u32::from_be_bytes(seq),
                        capture_ns,
                        i128::from(capture_ns) - on_grid
                    ));
                }
            }
            if relay.base.is_none_or(|(i, _)| d.chunk_index >= i) {
                relay.base = Some((d.chunk_index, capture_ns));
            }
            let play_at = capture_ns.saturating_add(relay.lead_ns);
            let age_us = now.saturating_sub(capture_ns) / 1_000;
            relay.stats.age_max_us = relay.stats.age_max_us.max(age_us);
            relay.stats.age_sum_us += age_us;
            if play_at <= now {
                relay.stats.late += 1;
                continue;
            }
            d.payload[STAMP_AT].copy_from_slice(&play_at.to_be_bytes());
            if !relay.said_first {
                relay.said_first = true;
                let mut seq = [0u8; 4];
                seq.copy_from_slice(&d.payload[0..4]);
                (self.say)(&format!(
                    "tv-relay first-chunk input={} chunk={} sequence={} capture_ns={} \
                     play_at_ns={} lead_ns={} now_ns={} recovered={}",
                    relay.play.input.literal(),
                    d.chunk_index,
                    u32::from_be_bytes(seq),
                    capture_ns,
                    play_at,
                    relay.lead_ns,
                    now,
                    u8::from(d.recovered)
                ));
            }
            relay.stats.relayed += 1;
            for down in relay.downs.iter_mut() {
                let Some(port) = down.port else {
                    continue;
                };
                let Ok(datagrams) = down.fec.push(&d.payload) else {
                    continue;
                };
                for dg in datagrams {
                    let Ok(sealed) = down.sealer.seal(&dg) else {
                        continue;
                    };
                    {
                        let mut loss = lock(&self.loss);
                        if let Some((_, leg)) = loss.as_mut() {
                            if leg.drop_next() {
                                relay.stats.dropped_down += 1;
                                continue;
                            }
                        }
                    }
                    if self
                        .socket
                        .send_to(&sealed, SocketAddr::new(down.ip, port))
                        .is_ok()
                    {
                        relay.stats.sent += 1;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;
    use std::sync::Arc;

    use chorus_protocol::v2::lowlat::DEFAULTS;

    type Sent = Arc<Mutex<Vec<(u64, LowLatencyOffer)>>>;

    fn relay() -> (TvRelay, Sent, Arc<Mutex<Vec<String>>>) {
        let sent: Sent = Arc::new(Mutex::new(Vec::new()));
        let said = Arc::new(Mutex::new(Vec::new()));
        let s = Arc::clone(&sent);
        let l = Arc::clone(&said);
        let relay = TvRelay::new(RelaySetup {
            socket: UdpSocket::bind("127.0.0.1:0").unwrap(),
            plan: DEFAULTS,
            shape: (48_000, 2, SampleFormat::PcmS16Le),
            timeline: MonotonicTimeline::new(),
            send: Box::new(move |id, m| {
                if let Message::LowLatencyOffer(o) = m {
                    s.lock().unwrap().push((id, o.clone()));
                }
                true
            }),
            wake: Box::new(|| {}),
            say: Box::new(move |line| l.lock().unwrap().push(line.to_string())),
            key: Box::new(|| Some([7u8; KEY_LEN])),
            loss: None,
        })
        .unwrap();
        (relay, sent, said)
    }

    fn play(trim_ms: i16) -> TvPlay {
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        TvPlay {
            input: InputId::parse("hub/tv").unwrap(),
            room: "theater".to_string(),
            hub: (1, ip),
            players: vec![(2, ip), (3, ip)],
            trim_ms,
        }
    }

    fn accept(tag: u32, status: LowLatencyStatus, port: u16) -> LowLatencyAccept {
        LowLatencyAccept {
            stream_tag: tag,
            status,
            udp_port: port,
        }
    }

    #[test]
    fn the_players_are_offered_first_and_the_hub_only_once_every_player_accepted() {
        let (relay, sent, said) = relay();
        assert!(relay.reconcile(&[play(0)]).is_empty());
        let offers = sent.lock().unwrap().clone();
        assert_eq!(offers.len(), 2, "one offer per player, none to the hub yet");
        assert!(offers
            .iter()
            .all(|(_, o)| o.direction == LowLatencyDirection::ToEndpoint && o.udp_port == 0));
        assert_eq!(offers[0].1.fec_k, DEFAULTS.fec_k);
        relay.accept(
            2,
            &accept(offers[0].1.stream_tag, LowLatencyStatus::Accepted, 5000),
        );
        assert_eq!(
            sent.lock().unwrap().len(),
            2,
            "one player still owes its answer"
        );
        relay.accept(
            3,
            &accept(offers[1].1.stream_tag, LowLatencyStatus::Accepted, 5001),
        );
        let hub = sent.lock().unwrap()[2].clone();
        assert_eq!(hub.0, 1);
        assert_eq!(hub.1.direction, LowLatencyDirection::FromEndpoint);
        assert_eq!(hub.1.udp_port, relay.port());
        assert!(!relay.active(&play(0).input));
        relay.accept(1, &accept(hub.1.stream_tag, LowLatencyStatus::Accepted, 0));
        assert!(relay.active(&play(0).input));
        assert!(said
            .lock()
            .unwrap()
            .iter()
            .any(|l| l.contains("tv-relay active")));
        // Not wanted any more: every stream of it is ended.
        assert_eq!(relay.reconcile(&[]), vec![play(0).input]);
        let ends: Vec<u64> = sent
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, o)| o.direction == LowLatencyDirection::End)
            .map(|(id, _)| *id)
            .collect();
        assert_eq!(ends, vec![2, 3, 1]);
    }

    #[test]
    fn a_refusal_ends_the_play_and_it_is_not_offered_again_while_it_is_what_is_wanted() {
        let (relay, sent, said) = relay();
        relay.reconcile(&[play(0)]);
        let tag = sent.lock().unwrap()[0].1.stream_tag;
        relay.accept(2, &accept(tag, LowLatencyStatus::RefusedWireless, 0));
        assert!(relay.refused(&play(0)));
        assert!(said
            .lock()
            .unwrap()
            .iter()
            .any(|l| l.contains("tv-path refused") && l.contains("status=refused_wireless")));
        let before = sent.lock().unwrap().len();
        relay.reconcile(&[play(0)]);
        assert_eq!(sent.lock().unwrap().len(), before, "no second offer");
        // Once it is not wanted, a later want is a fresh attempt.
        relay.reconcile(&[]);
        relay.reconcile(&[play(0)]);
        assert!(sent.lock().unwrap().len() > before);
    }

    #[test]
    fn a_trim_moves_the_lead_in_place_and_the_floor_clamps_it_by_name() {
        let (relay, sent, said) = relay();
        relay.reconcile(&[play(0)]);
        let offers = sent.lock().unwrap().len();
        relay.reconcile(&[play(50)]);
        assert_eq!(
            sent.lock().unwrap().len(),
            offers,
            "a trim is not a new play"
        );
        assert!(said
            .lock()
            .unwrap()
            .iter()
            .any(|l| l.contains("tv-relay lead") && l.contains("lead_ns=70000000")));
        relay.reconcile(&[play(-100)]);
        let floor = DEFAULTS.floor_ns();
        assert!(said
            .lock()
            .unwrap()
            .iter()
            .any(|l| l.starts_with(AV_TRIM_CLAMPED) && l.contains(&format!("lead_ns={}", floor))));
    }

    #[test]
    fn the_test_loss_is_seeded_and_near_its_rate() {
        let mut a = Loss::new(10_000, 42);
        let mut b = Loss::new(10_000, 42);
        let draws: Vec<bool> = (0..200_000).map(|_| a.drop_next()).collect();
        assert!(draws.iter().all(|&d| d == b.drop_next()), "reproducible");
        let n = draws.iter().filter(|&&d| d).count();
        assert!((1_700..2_300).contains(&n), "{} of 200000 at 1e-2", n);
        assert!(!(0..10_000).any(|_| Loss::new(0, 1).drop_next()));
    }
}
