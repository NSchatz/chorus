//! The TV path's low-latency stream on a wired player (goal 13, direction 1
//! of `docs/protocol.md` "Low-latency path").
//!
//! # What it is for
//!
//! A TV input played into one wired room cannot ride the slot path: its
//! 180 ms playout latency is lip sync off by an order of magnitude. The
//! server relays the hub's capture to every player of that room as UDP
//! datagrams (`chorus_protocol::v2::lowlat`), each chunk stamped to be heard
//! `L_tv` (20 ms by default) after the TV produced it. This module is the
//! player's end of that: it answers the server's `low_latency_offer` (0x16)
//! with `low_latency_accept` (0x17), receives the datagrams on a thread of
//! its own, repairs a lost chunk from its group's parity, holds what arrived
//! in a small jitter buffer keyed by stamp, and hands the play loop
//! (`crate::run`) the chunk stamped at the instant the next written frame
//! will be heard.
//!
//! # The order of things
//!
//! 1. The session hands every `to_endpoint` offer and every `end` here
//!    (`crate::session`). [`LowLatPlayer::service`], called by the play loop
//!    each iteration, answers it: refused `refused_wireless` on an endpoint
//!    that is not wired or switched the path off (`--low-latency off`; it
//!    never advertised the feature, so a server that offers anyway is
//!    answered by name), `refused_fec` when the FEC shape or the chunk does
//!    not fit this stream, `refused_no_socket` when no UDP socket could be
//!    bound; else a socket is bound, the receiver thread starts, and the
//!    accept carries its port.
//! 2. The receiver takes datagrams from the session's server address only,
//!    opens them (`Opener`: tag, replay window, AEAD) and runs the FEC
//!    (`FecDecoder`): a chunk is handed on the moment it arrives, a rebuilt
//!    one the moment its group allows. A chunk whose header is not the
//!    session's stream (rate, channels, sample format) is rejected; one whose
//!    stamp is already behind the playhead is late and dropped. As the
//!    playhead passes a block of groups, the decoder is told to close them
//!    (`expire_before`), so what they still miss is counted lost.
//! 3. The play loop asks [`LowLatPlayer::render_next`] for the next chunk,
//!    given the server-timeline instant the next written frame will be heard
//!    (`server_now + device delay`). The first call snaps the playhead onto
//!    the stream's stamp grid with silence; each later one plays the chunk
//!    stamped at the playhead, or conceals a missing one: the last written
//!    frame is held and faded linearly to silence over one chunk, and the
//!    first chunk after a gap fades in over the same length, so a loss is
//!    never a hard cut. A playhead that has drifted from the heard instant
//!    by more than [`RESNAP_NS`] is moved back by trimming or padding frames
//!    (counted).
//!
//! # What is not here
//!
//! Volume and the room's sound stay where they are for the slot path: the
//! play loop applies the room's volume to these frames and the sink it was
//! given (the endpoint's DSP chain) processes them. No clock is read here
//! except through the caller: the heard instant is the play loop's, on the
//! server timeline. Nothing here is timing evidence (BRIEF.md 3.1 rule 3).

use std::collections::BTreeMap;
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use chorus_control::transport::Transport;
use chorus_protocol::v2::lowlat::{
    chunk_fits, ChunkInfo, FecDecoder, FecParams, Kind, Opener, MAX_DATAGRAM_LEN,
};
use chorus_protocol::v2::{
    LowLatencyAccept, LowLatencyDirection, LowLatencyOffer, LowLatencyStatus, Message,
};
use chorus_protocol::{SampleFormat, CHUNK_HEADER_LEN};

use crate::config::ClientConfig;
use crate::source::Upstream;
use crate::tvcapture::{decode_sample, encode_sample};

/// How far the playhead may drift from the instant the next written frame
/// is heard before it is moved back, ns. ASSUMED: a fifth of a chunk, well
/// inside BRIEF.md 2.2's lip-sync window, and large enough that a device
/// whose delay report moves by a frame or two never trips it.
pub const RESNAP_NS: u64 = 500_000;

/// The device delay the play loop paces to while the low-latency stream
/// plays, us: the budget's endpoint output path
/// (`chorus_protocol::v2::lowlat::DEFAULTS.endpoint_output_ns`, ASSUMED
/// there: BRIEF.md 5.7's 2 to 10 ms).
pub const LL_DEVICE_TARGET_US: u64 =
    chorus_protocol::v2::lowlat::DEFAULTS.endpoint_output_ns / 1_000;

/// How long the receiver's socket read waits before it looks at its stop
/// flag and the playhead again. ASSUMED: two chunks of the default 2.5 ms.
const RECV_WAIT: Duration = Duration::from_millis(5);

/// The most chunks the jitter buffer holds. ASSUMED: one second of 2.5 ms
/// chunks, far past `L_tv`'s 40 ms ceiling; older entries are late anyway.
const MAX_HELD: usize = 400;

/// Byte offset of an `audio_chunk` payload's stamp (`docs/protocol.md` 0x02).
const STAMP_AT: usize = 4;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// What one stream received and played.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LowLatStats {
    /// The stream's tag (0 before any was accepted).
    pub stream_tag: u32,
    /// Datagrams that authenticated and were new.
    pub opened: u64,
    /// Datagrams whose tag did not verify.
    pub auth_failed: u64,
    /// Datagrams already seen or older than the replay window.
    pub replayed: u64,
    /// Chunks the FEC handed on (received or rebuilt).
    pub delivered: u64,
    /// Chunks rebuilt from a parity.
    pub recovered: u64,
    /// Chunks of a closed group that never arrived and could not be rebuilt.
    pub unrecoverable: u64,
    /// Chunks (and datagrams for closed groups) that came after their
    /// playout point: dropped, never played.
    pub late: u64,
    /// Chunks written to the device.
    pub played: u64,
    /// Chunks concealed (a missing chunk held and faded out, or silence).
    pub concealed: u64,
    /// Chunks whose header is not this stream's, or whose FEC fields are not.
    pub rejected: u64,
    /// Chunks written as silence for want of a sync offset.
    pub no_offset: u64,
    /// Times the playhead was moved back onto the heard instant.
    pub resnaps: u64,
    /// TCP chunks discarded while this stream played (the slot path's).
    pub superseded: u64,
}

/// The receiver thread's counts and the jitter buffer, shared with the play
/// loop.
#[derive(Default)]
struct Shared {
    stop: AtomicBool,
    /// The stamp of the next frame to write; 0 until the play loop snapped.
    playhead: AtomicU64,
    /// One chunk's duration, ns (the stamp grid).
    chunk_ns: AtomicU64,
    held: Mutex<BTreeMap<u64, Vec<u8>>>,
    /// The first stamp received, the grid's anchor (0: none yet).
    anchor: AtomicU64,
    opened: AtomicU64,
    auth_failed: AtomicU64,
    replayed: AtomicU64,
    delivered: AtomicU64,
    recovered: AtomicU64,
    unrecoverable: AtomicU64,
    late_fec: AtomicU64,
    late_play: AtomicU64,
    played: AtomicU64,
    concealed: AtomicU64,
    rejected: AtomicU64,
    no_offset: AtomicU64,
    resnaps: AtomicU64,
    superseded: AtomicU64,
}

/// What [`LowLatPlayer::set_tap`] is given: the sequence, stamp (ns) and PCM
/// of every data datagram that opened (on time or not) and of every chunk
/// rebuilt from a parity, and whether it was rebuilt.
pub type Tap = Box<dyn FnMut(u32, u64, &[u8], bool) + Send>;

/// One accepted stream.
struct Stream {
    tag: u32,
    chunk_frames: u32,
    shared: Arc<Shared>,
    join: Option<JoinHandle<()>>,
}

impl Stream {
    fn stop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

/// The stream's shape, from the session's `stream_format` (the play loop's
/// handshake): a datagram's chunk must be this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Shape {
    rate_hz: u32,
    channels: u16,
    format: SampleFormat,
}

/// The play side's output state: the concealment's memory.
struct Output {
    concealer: Concealer,
    shape: Option<Shape>,
}

/// A wired player's low-latency path (see the module documentation).
pub struct LowLatPlayer {
    offers: Mutex<Receiver<LowLatencyOffer>>,
    reply: Mutex<Box<dyn Upstream>>,
    server_ip: IpAddr,
    allowed: bool,
    log: Box<dyn Fn(&str) + Send + Sync>,
    stream: Mutex<Option<Stream>>,
    /// The last stream's counts, kept after it ended for the status line.
    last: Mutex<Option<(u32, Arc<Shared>)>>,
    tap: Arc<Mutex<Option<Tap>>>,
    output: Mutex<Output>,
}

impl LowLatPlayer {
    /// A player's path for one session: `offers` is the session's
    /// `low_latency_player`, `reply` its writer (the accept goes up it),
    /// `server_ip` the session's TCP peer (the only address datagrams are
    /// taken from), `log` where its status lines go.
    pub fn new(
        offers: Receiver<LowLatencyOffer>,
        reply: Box<dyn Upstream>,
        server_ip: IpAddr,
        config: &ClientConfig,
        log: Box<dyn Fn(&str) + Send + Sync>,
    ) -> Arc<LowLatPlayer> {
        Arc::new(LowLatPlayer {
            offers: Mutex::new(offers),
            reply: Mutex::new(reply),
            server_ip,
            allowed: config.transport == Transport::Wired && config.low_latency,
            log,
            stream: Mutex::new(None),
            last: Mutex::new(None),
            tap: Arc::new(Mutex::new(None)),
            output: Mutex::new(Output {
                concealer: Concealer::new(2, SampleFormat::PcmS16Le),
                shape: None,
            }),
        })
    }

    /// The session's stream shape, which a datagram's chunk must match (the
    /// play loop sets it from its handshake before it services offers).
    pub fn set_shape(&self, rate_hz: u32, channels: u16, format: SampleFormat) {
        let mut out = lock(&self.output);
        let shape = Shape {
            rate_hz,
            channels,
            format,
        };
        if out.shape != Some(shape) {
            out.shape = Some(shape);
            out.concealer = Concealer::new(channels, format);
        }
    }

    /// Called on the receiver thread for every chunk the FEC hands on, with
    /// its sequence (the hub's, which the relay keeps), its stamp, its PCM and whether it was rebuilt (the end-to-end tests
    /// read the stamps and the audio through it).
    pub fn set_tap(&self, tap: Tap) {
        *lock(&self.tap) = Some(tap);
    }

    /// Whether a stream is accepted and running.
    pub fn active(&self) -> bool {
        lock(&self.stream).is_some()
    }

    /// The current stream's counts, or the last one's once it ended.
    pub fn stats(&self) -> LowLatStats {
        let current = lock(&self.stream)
            .as_ref()
            .map(|s| (s.tag, Arc::clone(&s.shared)));
        let pair = current.or_else(|| lock(&self.last).clone());
        match pair {
            Some((tag, shared)) => stats_of(tag, &shared),
            None => LowLatStats::default(),
        }
    }

    /// One status line: every count of [`LowLatPlayer::stats`].
    pub fn status_line(&self) -> String {
        let s = self.stats();
        format!(
            "low-latency stream_tag={} opened={} auth_failed={} replayed={} delivered={} \
             recovered={} unrecoverable={} late={} played={} concealed={} rejected={} \
             no_offset={} resnaps={} superseded={}",
            s.stream_tag,
            s.opened,
            s.auth_failed,
            s.replayed,
            s.delivered,
            s.recovered,
            s.unrecoverable,
            s.late,
            s.played,
            s.concealed,
            s.rejected,
            s.no_offset,
            s.resnaps,
            s.superseded
        )
    }

    /// Frames per chunk of the running stream (0 when none runs).
    pub fn chunk_frames(&self) -> u32 {
        lock(&self.stream).as_ref().map_or(0, |s| s.chunk_frames)
    }

    /// Count TCP chunks the play loop discarded because this stream plays.
    pub fn note_superseded(&self, chunks: u64) {
        if let Some(s) = lock(&self.stream).as_ref() {
            s.shared.superseded.fetch_add(chunks, Ordering::Relaxed);
        }
    }

    /// Answer every offer that arrived (see the module documentation).
    pub fn service(&self) {
        loop {
            let offer = match lock(&self.offers).try_recv() {
                Ok(o) => o,
                Err(_) => return,
            };
            match offer.direction {
                LowLatencyDirection::End => self.end(offer.stream_tag, "end"),
                LowLatencyDirection::ToEndpoint => self.take(&offer),
                // The session routes these to the source role.
                LowLatencyDirection::FromEndpoint => {}
            }
        }
    }

    /// Stop whatever stream runs (the session is ending).
    pub fn stop_all(&self) {
        let tag = lock(&self.stream).as_ref().map(|s| s.tag);
        if let Some(tag) = tag {
            self.end(tag, "session-ended");
        }
    }

    fn end(&self, tag: u32, why: &str) {
        let taken = {
            let mut guard = lock(&self.stream);
            if guard.as_ref().is_some_and(|s| s.tag == tag) {
                guard.take()
            } else {
                None
            }
        };
        if let Some(mut s) = taken {
            s.stop();
            *lock(&self.last) = Some((s.tag, Arc::clone(&s.shared)));
            (self.log)(&format!(
                "low-latency end reason={} {}",
                why,
                self.status_line()
            ));
        }
    }

    fn answer(&self, tag: u32, status: LowLatencyStatus, port: u16) {
        let sent = lock(&self.reply).send(&Message::LowLatencyAccept(LowLatencyAccept {
            stream_tag: tag,
            status,
            udp_port: port,
        }));
        if let Err(e) = sent {
            (self.log)(&format!(
                "low-latency answer-failed stream_tag={} detail={}",
                tag, e
            ));
        }
    }

    fn take(&self, offer: &LowLatencyOffer) {
        let shape = lock(&self.output).shape;
        let refuse = |status: LowLatencyStatus, detail: &str| {
            (self.log)(&format!(
                "low-latency refused stream_tag={} status={} detail={}",
                offer.stream_tag,
                status.name(),
                detail
            ));
            self.answer(offer.stream_tag, status, 0);
        };
        if !self.allowed {
            return refuse(
                LowLatencyStatus::RefusedWireless,
                "this endpoint is not wired or its low-latency path is off",
            );
        }
        let params = match FecParams::new(offer.fec_k, offer.fec_depth) {
            Ok(p) => p,
            Err(e) => return refuse(LowLatencyStatus::RefusedFec, &e.to_string()),
        };
        let Some(shape) = shape else {
            return refuse(
                LowLatencyStatus::RefusedNoSocket,
                "no stream shape yet to check the offer's chunk against",
            );
        };
        if !chunk_fits(offer.chunk_frames, shape.channels, shape.format) {
            return refuse(
                LowLatencyStatus::RefusedFec,
                "the offered chunk does not fit one datagram at this stream's shape",
            );
        }
        let bind: SocketAddr = match self.server_ip {
            IpAddr::V4(_) => (Ipv4Addr::UNSPECIFIED, 0).into(),
            IpAddr::V6(_) => (Ipv6Addr::UNSPECIFIED, 0).into(),
        };
        let socket = match UdpSocket::bind(bind).and_then(|s| {
            s.set_read_timeout(Some(RECV_WAIT))?;
            Ok(s)
        }) {
            Ok(s) => s,
            Err(e) => return refuse(LowLatencyStatus::RefusedNoSocket, &e.to_string()),
        };
        let port = match socket.local_addr() {
            Ok(a) => a.port(),
            Err(e) => return refuse(LowLatencyStatus::RefusedNoSocket, &e.to_string()),
        };
        // An offer with a tag in use replaces it; any other running stream
        // ends too (one stream plays at a time).
        let old = lock(&self.stream).as_ref().map(|s| s.tag);
        if let Some(tag) = old {
            self.end(tag, "replaced");
        }
        let shared = Arc::new(Shared::default());
        let chunk_ns =
            u64::from(offer.chunk_frames) * 1_000_000_000 / u64::from(shape.rate_hz.max(1));
        shared.chunk_ns.store(chunk_ns.max(1), Ordering::Relaxed);
        let join = {
            let shared = Arc::clone(&shared);
            let tap = Arc::clone(&self.tap);
            let opener = Opener::new(offer.key, offer.stream_tag);
            let decoder = FecDecoder::new(params);
            let server_ip = self.server_ip;
            thread::spawn(move || {
                receive(
                    socket, server_ip, opener, decoder, params, shape, &shared, &tap,
                )
            })
        };
        lock(&self.output).concealer.reset();
        *lock(&self.stream) = Some(Stream {
            tag: offer.stream_tag,
            chunk_frames: offer.chunk_frames,
            shared,
            join: Some(join),
        });
        (self.log)(&format!(
            "low-latency start stream_tag={} udp_port={} chunk_frames={} fec_k={} fec_depth={} \
             latency_ns={}",
            offer.stream_tag,
            port,
            offer.chunk_frames,
            offer.fec_k,
            offer.fec_depth,
            offer.latency_ns
        ));
        self.answer(offer.stream_tag, LowLatencyStatus::Accepted, port);
    }

    /// The next PCM to write, for the frame heard at `heard_at_ns` on the
    /// server timeline (`None`: no sync offset yet, so a chunk of silence).
    /// Empty when no stream runs.
    pub fn render_next(&self, heard_at_ns: Option<u64>) -> Vec<u8> {
        let (shared, chunk_frames) = match lock(&self.stream).as_ref() {
            Some(s) => (Arc::clone(&s.shared), s.chunk_frames),
            None => return Vec::new(),
        };
        let mut out = lock(&self.output);
        let Some(shape) = out.shape else {
            return Vec::new();
        };
        let frame_len = usize::from(shape.channels) * shape.format.bytes_per_sample();
        let chunk_bytes = chunk_frames as usize * frame_len;
        let chunk_ns = shared.chunk_ns.load(Ordering::Relaxed);
        let rate = u64::from(shape.rate_hz.max(1));
        let ns_to_frames = |ns: u64| (u128::from(ns) * u128::from(rate) / 1_000_000_000) as usize;
        let Some(heard) = heard_at_ns else {
            shared.no_offset.fetch_add(1, Ordering::Relaxed);
            return out.concealer.render(None, chunk_frames as usize);
        };
        let mut playhead = shared.playhead.load(Ordering::Relaxed);
        let mut pcm = Vec::with_capacity(chunk_bytes * 2);
        if playhead == 0 {
            // Snap onto the stream's grid: the first grid point at or after
            // the heard instant, the frames before it silence.
            let anchor = shared.anchor.load(Ordering::Relaxed);
            if anchor == 0 {
                return out.concealer.render(None, chunk_frames as usize);
            }
            let g = grid_at_or_after(anchor, chunk_ns, heard);
            pcm.extend(out.concealer.render(None, ns_to_frames(g - heard)));
            playhead = g;
            shared.playhead.store(playhead, Ordering::Relaxed);
        } else {
            let err = i128::from(heard) - i128::from(playhead);
            if err.unsigned_abs() > u128::from(RESNAP_NS) {
                shared.resnaps.fetch_add(1, Ordering::Relaxed);
                if err > 0 {
                    // Behind: what is stamped at the playhead would be heard
                    // late. Skip whole chunks, then trim the head of the next.
                    let mut behind = err as u64;
                    while behind >= chunk_ns {
                        playhead += chunk_ns;
                        behind -= chunk_ns;
                    }
                    shared.playhead.store(playhead, Ordering::Relaxed);
                    let trim = ns_to_frames(behind) * frame_len;
                    let mut chunk = self.chunk_at(&shared, &mut out, playhead, chunk_frames);
                    chunk.drain(..trim.min(chunk.len()));
                    playhead += chunk_ns;
                    shared.playhead.store(playhead, Ordering::Relaxed);
                    return chunk;
                }
                // Ahead: hold silence until the playhead is heard.
                pcm.extend(out.concealer.render(None, ns_to_frames((-err) as u64)));
            }
        }
        pcm.extend(self.chunk_at(&shared, &mut out, playhead, chunk_frames));
        shared
            .playhead
            .store(playhead + chunk_ns, Ordering::Relaxed);
        pcm
    }

    /// The chunk stamped at `playhead`, rendered (or concealed), and every
    /// held chunk older than it dropped as late.
    fn chunk_at(
        &self,
        shared: &Shared,
        out: &mut Output,
        playhead: u64,
        chunk_frames: u32,
    ) -> Vec<u8> {
        let chunk_ns = shared.chunk_ns.load(Ordering::Relaxed);
        let half = chunk_ns / 2;
        let found = {
            let mut held = lock(&shared.held);
            let lo = playhead.saturating_sub(half);
            let stale: Vec<u64> = held.range(..lo).map(|(k, _)| *k).collect();
            for k in &stale {
                held.remove(k);
            }
            shared
                .late_play
                .fetch_add(stale.len() as u64, Ordering::Relaxed);
            let key = held
                .range(lo..playhead.saturating_add(half))
                .next()
                .map(|(k, _)| *k);
            key.and_then(|k| held.remove(&k))
        };
        match found {
            Some(pcm) => {
                shared.played.fetch_add(1, Ordering::Relaxed);
                out.concealer.render(Some(&pcm), chunk_frames as usize)
            }
            None => {
                shared.concealed.fetch_add(1, Ordering::Relaxed);
                out.concealer.render(None, chunk_frames as usize)
            }
        }
    }
}

impl std::fmt::Debug for LowLatPlayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LowLatPlayer")
            .field("server_ip", &self.server_ip)
            .field("allowed", &self.allowed)
            .field("stats", &self.stats())
            .finish_non_exhaustive()
    }
}

impl Drop for LowLatPlayer {
    fn drop(&mut self) {
        if let Some(mut s) = lock(&self.stream).take() {
            s.stop();
        }
    }
}

fn stats_of(tag: u32, s: &Shared) -> LowLatStats {
    let g = |a: &AtomicU64| a.load(Ordering::Relaxed);
    LowLatStats {
        stream_tag: tag,
        opened: g(&s.opened),
        auth_failed: g(&s.auth_failed),
        replayed: g(&s.replayed),
        delivered: g(&s.delivered),
        recovered: g(&s.recovered),
        unrecoverable: g(&s.unrecoverable),
        late: g(&s.late_fec) + g(&s.late_play),
        played: g(&s.played),
        concealed: g(&s.concealed),
        rejected: g(&s.rejected),
        no_offset: g(&s.no_offset),
        resnaps: g(&s.resnaps),
        superseded: g(&s.superseded),
    }
}

/// The first point of the grid `anchor + n x step` (n of either sign) at or
/// after `t`.
pub fn grid_at_or_after(anchor: u64, step: u64, t: u64) -> u64 {
    let step = step.max(1);
    if t <= anchor {
        anchor - ((anchor - t) / step) * step
    } else {
        let n = (t - anchor).div_ceil(step);
        anchor + n * step
    }
}

/// The receiver thread (see the module documentation, step 2).
#[allow(clippy::too_many_arguments)]
fn receive(
    socket: UdpSocket,
    server_ip: IpAddr,
    mut opener: Opener,
    mut decoder: FecDecoder,
    params: FecParams,
    shape: Shape,
    shared: &Shared,
    tap: &Mutex<Option<Tap>>,
) {
    let mut buf = vec![0u8; MAX_DATAGRAM_LEN + 1];
    // The grid's anchor in chunk numbers: (chunk index, stamp) of the first
    // chunk handed on.
    let mut base: Option<(u64, u64)> = None;
    let mut rejected_shape = 0u64;
    let mut late_play = 0u64;
    while !shared.stop.load(Ordering::SeqCst) {
        let got = socket.recv_from(&mut buf);
        let (n, from) = match got {
            Ok(v) => v,
            Err(e)
                if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut =>
            {
                expire(&mut decoder, params, base, shared);
                publish(&opener, &decoder, shared, rejected_shape);
                continue;
            }
            Err(_) => {
                thread::sleep(RECV_WAIT);
                continue;
            }
        };
        // Datagrams from anyone but the session's server are not ours.
        if from.ip() != server_ip {
            continue;
        }
        let Ok(opened) = opener.open(&buf[..n]) else {
            publish(&opener, &decoder, shared, rejected_shape);
            continue;
        };
        let chunk_ns = shared.chunk_ns.load(Ordering::Relaxed);
        // Every data datagram that opened is seen by the tap and moves the
        // stream's mapping, whether or not its group is still open: a chunk
        // too late to play is still a stamp received (and a lead that drops
        // below what this endpoint can play still shows on the tap).
        if opened.kind == Kind::Data && header_matches(&opened.plaintext, shape) {
            if let Some(info) = ChunkInfo::from_payload(&opened.plaintext) {
                let p = &opened.plaintext;
                let mut b = [0u8; 8];
                b.copy_from_slice(&p[STAMP_AT..STAMP_AT + 8]);
                let stamp = u64::from_be_bytes(b);
                let index = params.chunk_index(info.group, info.group_index);
                if base.is_none() {
                    shared.anchor.store(stamp.max(1), Ordering::Relaxed);
                }
                // The newest chunk's (index, stamp) maps chunk numbers onto
                // the timeline for `expire`: the newest, not the first, so a
                // hub's relock (its stamps restart on a fresh grid, ADR 0090)
                // or a moved lead moves the mapping with it.
                if base.is_none_or(|(i, _)| index >= i) {
                    base = Some((index, stamp));
                }
                let mut q = [0u8; 4];
                q.copy_from_slice(&p[..4]);
                if let Some(t) = lock(tap).as_mut() {
                    t(u32::from_be_bytes(q), stamp, &p[CHUNK_HEADER_LEN..], false);
                }
            }
        }
        let got = decoder.push(opened.kind, &opened.plaintext);
        for d in got {
            if !header_matches(&d.payload, shape) {
                rejected_shape += 1;
                continue;
            }
            let mut b = [0u8; 8];
            b.copy_from_slice(&d.payload[STAMP_AT..STAMP_AT + 8]);
            let stamp = u64::from_be_bytes(b);
            let pcm = &d.payload[CHUNK_HEADER_LEN..];
            if d.recovered {
                // A rebuilt chunk was never a datagram: the tap sees it here.
                if base.is_none_or(|(i, _)| d.chunk_index >= i) {
                    base = Some((d.chunk_index, stamp));
                }
                let mut q = [0u8; 4];
                q.copy_from_slice(&d.payload[..4]);
                if let Some(t) = lock(tap).as_mut() {
                    t(u32::from_be_bytes(q), stamp, pcm, true);
                }
            }
            let playhead = shared.playhead.load(Ordering::Relaxed);
            if playhead != 0 && stamp.saturating_add(chunk_ns / 2) < playhead {
                late_play += 1;
                continue;
            }
            let mut held = lock(&shared.held);
            held.insert(stamp, pcm.to_vec());
            while held.len() > MAX_HELD {
                let Some(first) = held.keys().next().copied() else {
                    break;
                };
                held.remove(&first);
                late_play += 1;
            }
        }
        if late_play > 0 {
            shared.late_play.fetch_add(late_play, Ordering::Relaxed);
            late_play = 0;
        }
        expire(&mut decoder, params, base, shared);
        publish(&opener, &decoder, shared, rejected_shape);
    }
}

/// Close every group of the blocks the playhead has passed.
fn expire(decoder: &mut FecDecoder, params: FecParams, base: Option<(u64, u64)>, s: &Shared) {
    let playhead = s.playhead.load(Ordering::Relaxed);
    let chunk_ns = s.chunk_ns.load(Ordering::Relaxed).max(1);
    let Some((index, stamp)) = base else {
        return;
    };
    if playhead <= stamp {
        return;
    }
    let passed = index + (playhead - stamp) / chunk_ns;
    if let Some((group, _)) = params.locate(passed) {
        let depth = u32::from(params.depth());
        decoder.expire_before((group / depth) * depth);
    }
}

fn publish(opener: &Opener, decoder: &FecDecoder, s: &Shared, rejected_shape: u64) {
    let o = opener.stats();
    let f = decoder.stats();
    let set = |a: &AtomicU64, v: u64| a.store(v, Ordering::Relaxed);
    set(&s.opened, o.opened);
    set(&s.auth_failed, o.auth_failed);
    set(&s.replayed, o.replayed);
    set(&s.delivered, f.delivered);
    set(&s.recovered, f.recovered);
    set(&s.unrecoverable, f.unrecoverable);
    set(&s.late_fec, f.late);
    set(&s.rejected, f.rejected + rejected_shape);
}

/// Whether a chunk payload's header is the session's stream: its rate
/// (offset 12, u32 big-endian), channels (16) and sample format (17), and a
/// whole number of frames after the header.
fn header_matches(payload: &[u8], shape: Shape) -> bool {
    if payload.len() < CHUNK_HEADER_LEN {
        return false;
    }
    let mut r = [0u8; 4];
    r.copy_from_slice(&payload[12..16]);
    let frame_len = usize::from(shape.channels) * shape.format.bytes_per_sample();
    u32::from_be_bytes(r) == shape.rate_hz
        && u16::from(payload[16]) == shape.channels
        && SampleFormat::from_wire(payload[17]) == Some(shape.format)
        && frame_len > 0
        && (payload.len() - CHUNK_HEADER_LEN).is_multiple_of(frame_len)
}

/// The concealment (see the module documentation, step 3): a chunk passes
/// through, faded in when it follows a gap; a missing chunk holds the last
/// frame written and fades it linearly to silence over the chunk, then
/// silence. ASSUMED fade length: one chunk (2.5 ms at the defaults), short
/// enough that the gap is heard as a gap rather than a smear, long enough
/// that no step is a click (a full-scale held sample falls by 1/120 of full
/// scale per frame at 120 frames).
pub struct Concealer {
    channels: usize,
    format: SampleFormat,
    last: Vec<f64>,
    gap: bool,
}

impl Concealer {
    /// A concealer for frames of `channels` samples of `format`; it starts
    /// as after a gap, so the first chunk fades in.
    pub fn new(channels: u16, format: SampleFormat) -> Concealer {
        Concealer {
            channels: usize::from(channels.max(1)),
            format,
            last: vec![0.0; usize::from(channels.max(1))],
            gap: true,
        }
    }

    /// Forget the last frame (a new stream): as after a gap.
    pub fn reset(&mut self) {
        self.last.iter_mut().for_each(|v| *v = 0.0);
        self.gap = true;
    }

    /// `frames` frames: `pcm` (a chunk received) or, with `None`, the held
    /// frame fading out.
    pub fn render(&mut self, pcm: Option<&[u8]>, frames: usize) -> Vec<u8> {
        let width = self.format.bytes_per_sample();
        let mut out = Vec::with_capacity(frames * self.channels * width);
        match pcm {
            Some(pcm) => {
                let n = pcm.len() / (width * self.channels);
                for f in 0..n {
                    let gain = if self.gap {
                        (f + 1) as f64 / n as f64
                    } else {
                        1.0
                    };
                    for c in 0..self.channels {
                        let at = (f * self.channels + c) * width;
                        let x = decode_sample(self.format, &pcm[at..at + width]);
                        if self.gap {
                            encode_sample(self.format, x * gain, &mut out);
                        } else {
                            out.extend_from_slice(&pcm[at..at + width]);
                        }
                        if f + 1 == n {
                            self.last[c] = x;
                        }
                    }
                }
                self.gap = false;
            }
            None => {
                for f in 0..frames {
                    let gain = 1.0 - (f + 1) as f64 / frames as f64;
                    for c in 0..self.channels {
                        encode_sample(self.format, self.last[c] * gain, &mut out);
                    }
                }
                if frames > 0 {
                    self.last.iter_mut().for_each(|v| *v = 0.0);
                    self.gap = true;
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chorus_protocol::v2::lowlat::{FecEncoder, Sealer};
    use chorus_protocol::AudioChunk;
    use chorus_protocol::RESERVED_LEN;
    use std::sync::mpsc;
    use std::time::Instant;

    fn s16(pcm: &[u8]) -> Vec<i16> {
        pcm.as_chunks::<2>()
            .0
            .iter()
            .map(|b| i16::from_le_bytes(*b))
            .collect()
    }

    fn dc(v: i16, frames: usize) -> Vec<u8> {
        (0..frames * 2).flat_map(|_| v.to_le_bytes()).collect()
    }

    #[test]
    fn a_missing_chunk_fades_out_and_the_next_fades_in_with_no_step_past_the_fade() {
        let mut c = Concealer::new(2, SampleFormat::PcmS16Le);
        let mut tape = Vec::new();
        tape.extend(s16(&c.render(Some(&dc(20_000, 120)), 120)));
        tape.extend(s16(&c.render(Some(&dc(20_000, 120)), 120)));
        tape.extend(s16(&c.render(None, 120)));
        tape.extend(s16(&c.render(None, 120)));
        tape.extend(s16(&c.render(Some(&dc(20_000, 120)), 120)));
        // A full chunk of the held frame falls to silence in 120 steps: no
        // step is larger than 20000 / 120 (+1 for rounding).
        let left: Vec<i32> = tape.iter().step_by(2).map(|v| i32::from(*v)).collect();
        let worst = left.windows(2).map(|w| (w[1] - w[0]).abs()).max().unwrap();
        assert!(worst <= 20_000 / 120 + 1, "a step of {}", worst);
        // The second missing chunk is silence, and the end of the first.
        assert_eq!(left[2 * 120 + 119], 0);
        assert!(left[3 * 120..4 * 120].iter().all(|v| *v == 0));
        // The chunk after the gap reaches full level by its end.
        assert_eq!(left[5 * 120 - 1], 20_000);
    }

    #[test]
    fn the_grid_point_at_or_after_an_instant() {
        assert_eq!(grid_at_or_after(1_000, 250, 1_000), 1_000);
        assert_eq!(grid_at_or_after(1_000, 250, 1_001), 1_250);
        assert_eq!(grid_at_or_after(1_000, 250, 600), 750);
        assert_eq!(grid_at_or_after(1_000, 250, 500), 500);
    }

    /// An upstream that keeps what it is sent.
    struct Kept(Arc<Mutex<Vec<Message>>>);

    impl Upstream for Kept {
        fn send(&mut self, message: &Message) -> io::Result<()> {
            self.0.lock().unwrap().push(message.clone());
            Ok(())
        }
    }

    fn player(
        config: &ClientConfig,
    ) -> (
        Arc<LowLatPlayer>,
        mpsc::Sender<LowLatencyOffer>,
        Arc<Mutex<Vec<Message>>>,
    ) {
        let (tx, rx) = mpsc::channel();
        let kept = Arc::new(Mutex::new(Vec::new()));
        let p = LowLatPlayer::new(
            rx,
            Box::new(Kept(Arc::clone(&kept))),
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            config,
            Box::new(|_| {}),
        );
        p.set_shape(48_000, 2, SampleFormat::PcmS16Le);
        (p, tx, kept)
    }

    fn offer(tag: u32, k: u8, frames: u32) -> LowLatencyOffer {
        LowLatencyOffer {
            direction: LowLatencyDirection::ToEndpoint,
            stream_tag: tag,
            key: [7u8; 32],
            udp_port: 0,
            chunk_frames: frames,
            fec_k: k,
            fec_depth: 1,
            latency_ns: 20_000_000,
        }
    }

    fn accepts(kept: &Mutex<Vec<Message>>) -> Vec<LowLatencyAccept> {
        kept.lock()
            .unwrap()
            .iter()
            .filter_map(|m| match m {
                Message::LowLatencyAccept(a) => Some(*a),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_wireless_endpoint_a_bad_fec_and_an_oversized_chunk_are_refused_by_status() {
        let wireless = ClientConfig {
            transport: Transport::Wireless,
            ..ClientConfig::default()
        };
        let (p, tx, kept) = player(&wireless);
        tx.send(offer(5, 4, 120)).unwrap();
        p.service();
        assert_eq!(accepts(&kept)[0].status, LowLatencyStatus::RefusedWireless);
        assert!(!p.active());

        let (p, tx, kept) = player(&ClientConfig::default());
        tx.send(offer(6, 1, 120)).unwrap();
        // 700 frames of stereo s16 is 2800 bytes: past one datagram.
        tx.send(offer(7, 4, 700)).unwrap();
        p.service();
        let a = accepts(&kept);
        assert_eq!(a[0].status, LowLatencyStatus::RefusedFec);
        assert_eq!(a[1].status, LowLatencyStatus::RefusedFec);
        assert!(a.iter().all(|a| a.udp_port == 0));
        assert!(!p.active());
    }

    #[test]
    fn a_loopback_stream_with_one_datagram_lost_is_recovered_played_and_ended() {
        let (p, tx, kept) = player(&ClientConfig::default());
        let tapped = Arc::new(Mutex::new(Vec::new()));
        {
            let tapped = Arc::clone(&tapped);
            p.set_tap(Box::new(move |_seq, stamp, pcm, rec| {
                tapped.lock().unwrap().push((stamp, pcm.len(), rec))
            }));
        }
        tx.send(offer(9, 4, 120)).unwrap();
        p.service();
        let a = accepts(&kept);
        assert_eq!(a[0].status, LowLatencyStatus::Accepted);
        assert!(a[0].udp_port != 0 && p.active());
        let to: SocketAddr = (Ipv4Addr::LOCALHOST, a[0].udp_port).into();
        let sock = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let mut sealer = Sealer::new([7u8; 32], 9).unwrap();
        let mut enc = FecEncoder::new(FecParams::new(4, 1).unwrap());
        let base = 1_000_000_000u64;
        let mut sent = 0;
        for n in 0..8u64 {
            let chunk = AudioChunk {
                sequence: n as u32,
                timestamp_ns: base + n * 2_500_000,
                sample_rate_hz: 48_000,
                channels: 2,
                sample_format: SampleFormat::PcmS16Le,
                reserved: [0u8; RESERVED_LEN],
                audio_data: dc(1_000 + n as i16, 120),
            };
            for d in enc.push(&chunk).unwrap() {
                let bytes = sealer.seal(&d).unwrap();
                sent += 1;
                // The second datagram (chunk 1's data) is lost.
                if sent == 2 {
                    continue;
                }
                sock.send_to(&bytes, to).unwrap();
            }
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while p.stats().delivered < 8 {
            assert!(Instant::now() < deadline, "{}", p.status_line());
            thread::sleep(Duration::from_millis(5));
        }
        let s = p.stats();
        assert_eq!((s.delivered, s.recovered, s.unrecoverable), (8, 1, 0));
        assert_eq!(tapped.lock().unwrap().iter().filter(|t| t.2).count(), 1);
        // Heard exactly at the first stamp: the chunks play in order, the
        // first faded in, the rest whole.
        let first = p.render_next(Some(base));
        assert_eq!(s16(&first).len(), 240);
        let second = p.render_next(Some(base + 2_500_000));
        assert!(
            s16(&second).iter().all(|v| *v == 1_001),
            "the rebuilt chunk"
        );
        // A chunk heard far later: everything before the playhead is late.
        let _ = p.render_next(Some(base + 7 * 2_500_000));
        assert!(p.stats().resnaps >= 1);
        assert!(p.stats().played >= 3);
        // The end stops the receiver and keeps the counts.
        tx.send(LowLatencyOffer {
            direction: LowLatencyDirection::End,
            stream_tag: 9,
            key: [0u8; 32],
            udp_port: 0,
            chunk_frames: 0,
            fec_k: 0,
            fec_depth: 0,
            latency_ns: 0,
        })
        .unwrap();
        p.service();
        assert!(!p.active());
        assert_eq!(p.stats().stream_tag, 9);
        assert!(p.status_line().starts_with("low-latency stream_tag=9 "));
    }

    #[test]
    fn a_chunk_arriving_after_the_playhead_passed_it_is_late_and_never_played() {
        let (p, tx, kept) = player(&ClientConfig::default());
        tx.send(offer(3, 0, 120)).unwrap();
        p.service();
        let port = accepts(&kept)[0].udp_port;
        let to: SocketAddr = (Ipv4Addr::LOCALHOST, port).into();
        let sock = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let mut sealer = Sealer::new([7u8; 32], 3).unwrap();
        let mut enc = FecEncoder::new(FecParams::none());
        let send = |n: u64, sealer: &mut Sealer, enc: &mut FecEncoder| {
            let chunk = AudioChunk {
                sequence: n as u32,
                timestamp_ns: 5_000_000_000 + n * 2_500_000,
                sample_rate_hz: 48_000,
                channels: 2,
                sample_format: SampleFormat::PcmS16Le,
                reserved: [0u8; RESERVED_LEN],
                audio_data: dc(500, 120),
            };
            for d in enc.push(&chunk).unwrap() {
                sock.send_to(&sealer.seal(&d).unwrap(), to).unwrap();
            }
        };
        send(0, &mut sealer, &mut enc);
        let deadline = Instant::now() + Duration::from_secs(5);
        while p.stats().delivered < 1 {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        // Play chunks 0..4 (1..4 concealed: not sent yet).
        for n in 0..5u64 {
            let _ = p.render_next(Some(5_000_000_000 + n * 2_500_000));
        }
        // Chunks 1 and 2 now arrive: their stamps are behind the playhead
        // (the decoder has closed their groups as the playhead passed).
        send(1, &mut sealer, &mut enc);
        send(2, &mut sealer, &mut enc);
        while p.stats().late < 2 {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        let s = p.stats();
        assert_eq!(s.late, 2, "{}", p.status_line());
        assert_eq!(s.played, 1);
        assert_eq!(s.concealed, 4);
    }
}
