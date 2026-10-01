//! Line-ins: an endpoint's input (ADR 0066's source role) accepted by the
//! server and played as a stream slot's input.
//!
//! # The path, end to end
//!
//! 1. A session that declared the source role sends `source_offer`
//!    ([`LineIns::offer`]). The input is named `<endpoint>/<input>` (the
//!    catalog's [`InputId`]: the offer's name when it is an identifier, else
//!    `line-<source_id>`) and its signal is queued as an [`InputEvent`] for
//!    the conductor, which hands it to the schedule runtime (the state's
//!    `inputs`, autoplay).
//! 2. When some group plays the input, the runtime asks for a start; the
//!    conductor takes a free [`Port`] for it ([`LineIns::start`]) and sends
//!    `source_control` start, sealed to the input's own session.
//! 3. The endpoint answers with `stream_format`. It must be this server's own
//!    format (rate, channels, sample format, PCM) or the start is refused:
//!    the conductor sends `stop` and logs `line-in refused
//!    reason=format-mismatch` (a conversion is a follow-up).
//! 4. Every upstream `audio_chunk` after that is written into the port's
//!    ring ([`Port::write_pcm`]), on the session's reader thread, as samples
//!    at full scale 1.0. A full ring drops the chunk and counts it.
//! 5. The audio thread drains the ring at each tick into the slot's
//!    resampler and plays it through the latency plan (`crate::slots`).
//! 6. The session ending (or never offering again) is [`InputEvent::Gone`].
//!
//! # What crosses to the audio thread
//!
//! Only the ports. Each is allocated once, at start, with a fixed capacity
//! ([`RING_MS`]), so neither side allocates per chunk on it; the audio
//! thread's side of the lock is held for one copy of what the ring holds into
//! a buffer that was itself sized at start. The lock is a mutex, not a lock-
//! free queue: the reader's critical section is one chunk's conversion
//! (microseconds), short against a 20 ms tick (ASSUMED adequate, not
//! measured; a wait-free ring is a follow-up if the tick measurement asks).
//!
//! No clock is read here: the server timeline the stamps are on is the audio
//! thread's, and the upstream chunks' own capture stamps are not used (the
//! plan is anchored where the audio thread first has enough in hand; see
//! `crate::slots`).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use chorus_audio::StreamFormat as PcmFormat;
use chorus_control::catalog::is_identifier;
use chorus_control::rooms::InputId;
use chorus_protocol::v2::{Codec, SourceOffer, StreamFormat};
use chorus_protocol::SampleFormat;

/// How much upstream audio a port's ring holds, ms. ASSUMED: fifty 20 ms
/// chunks, so a reader that is a second ahead of the audio thread is
/// absorbed rather than dropped.
pub const RING_MS: u64 = 1_000;

/// One endpoint's input, as this server knows it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Offered {
    id: InputId,
    session: u64,
    source_id: u8,
    signal: bool,
    upstream: Upstream,
}

/// Where an input's upstream is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Upstream {
    /// Not asked for.
    Idle,
    /// `source_control` start sent; waiting for its `stream_format`.
    Starting { port: usize },
    /// Its chunks are going into `port`.
    Streaming { port: usize },
    /// Its `stream_format` was refused; nothing it sends is played.
    Refused,
}

impl Upstream {
    fn port(self) -> Option<usize> {
        match self {
            Upstream::Starting { port } | Upstream::Streaming { port } => Some(port),
            Upstream::Idle | Upstream::Refused => None,
        }
    }
}

/// What the conductor is told about the inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputEvent {
    /// A `source_offer`: the input exists, with or without a signal.
    Signal(InputId, bool),
    /// The input's endpoint is gone (its session ended).
    Gone(InputId),
    /// A started input's `stream_format` is not this server's: stop it.
    Refused {
        /// The input.
        input: InputId,
        /// Why, for the log line.
        detail: String,
    },
}

/// A started input as the conductor addresses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Addressed {
    /// The router's id for the input's session.
    pub session: u64,
    /// The endpoint's number for the input.
    pub source_id: u8,
    /// The port its chunks go into.
    pub port: Option<usize>,
}

/// Every offered input, the ports their upstream goes into, and what the
/// conductor has yet to hear.
pub struct LineIns {
    format: PcmFormat,
    ports: Vec<Arc<Port>>,
    inner: Mutex<Inner>,
    wake: Box<dyn Fn() + Send + Sync>,
}

#[derive(Default)]
struct Inner {
    inputs: Vec<Offered>,
    events: Vec<InputEvent>,
}

impl std::fmt::Debug for LineIns {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LineIns")
            .field("ports", &self.ports.len())
            .finish_non_exhaustive()
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// The input name an offer is known by: its own name when that is an
/// identifier, else `line-<source_id>` (ASSUMED spelling).
pub fn input_name(offer: &SourceOffer) -> String {
    if is_identifier(&offer.name) {
        offer.name.clone()
    } else {
        format!("line-{}", offer.source_id)
    }
}

impl LineIns {
    /// `ports` ports (at most that many inputs stream at once: one input
    /// feeds at most one slot, so the slot count is enough), each holding
    /// [`RING_MS`] of `format`; `wake` pokes the conductor.
    pub fn new(format: PcmFormat, ports: usize, wake: Box<dyn Fn() + Send + Sync>) -> LineIns {
        let frames = (u64::from(format.sample_rate_hz) * RING_MS / 1_000) as usize;
        LineIns {
            format,
            ports: (0..ports)
                .map(|_| Arc::new(Port::new(frames, usize::from(format.channels))))
                .collect(),
            inner: Mutex::new(Inner::default()),
            wake,
        }
    }

    /// The ports, for the audio thread.
    pub fn ports(&self) -> &[Arc<Port>] {
        &self.ports
    }

    fn event(&self, inner: &mut Inner, event: InputEvent) {
        inner.events.push(event);
        (self.wake)();
    }

    // --- the session readers' side --------------------------------------

    /// A `source_offer` from `endpoint`'s session `session`.
    pub fn offer(&self, endpoint: &str, session: u64, offer: &SourceOffer) {
        let Some(id) = InputId::parse(&format!("{}/{}", endpoint, input_name(offer))) else {
            return;
        };
        let mut inner = lock(&self.inner);
        let fresh = match inner.inputs.iter_mut().find(|i| i.id == id) {
            Some(known) => {
                let changed = known.signal != offer.signal || known.session != session;
                known.signal = offer.signal;
                known.session = session;
                known.source_id = offer.source_id;
                changed
            }
            None => {
                inner.inputs.push(Offered {
                    id: id.clone(),
                    session,
                    source_id: offer.source_id,
                    signal: offer.signal,
                    upstream: Upstream::Idle,
                });
                true
            }
        };
        if fresh {
            self.event(&mut inner, InputEvent::Signal(id, offer.signal));
        }
    }

    /// A `stream_format` from session `session`: its started input's format.
    pub fn stream_format(&self, session: u64, format: &StreamFormat) {
        let mut inner = lock(&self.inner);
        let Some(i) = inner
            .inputs
            .iter()
            .position(|i| i.session == session && matches!(i.upstream, Upstream::Starting { .. }))
        else {
            return;
        };
        match self.mismatch(format) {
            None => {
                let port = inner.inputs[i].upstream.port().unwrap_or(0);
                inner.inputs[i].upstream = Upstream::Streaming { port };
            }
            Some(detail) => {
                inner.inputs[i].upstream = Upstream::Refused;
                let input = inner.inputs[i].id.clone();
                self.event(&mut inner, InputEvent::Refused { input, detail });
            }
        }
    }

    fn mismatch(&self, format: &StreamFormat) -> Option<String> {
        let ours = &self.format;
        if format.codec != Codec::Pcm
            || format.sample_rate_hz != ours.sample_rate_hz
            || format.channel_map.len() != usize::from(ours.channels)
            || format.sample_format != ours.sample_format
        {
            return Some(format!(
                "offered codec={} rate_hz={} channels={} sample_format={}; this server streams \
                 codec=pcm rate_hz={} channels={} sample_format={}",
                format.codec.name(),
                format.sample_rate_hz,
                format.channel_map.len(),
                format.sample_format.name(),
                ours.sample_rate_hz,
                ours.channels,
                ours.sample_format.name()
            ));
        }
        None
    }

    /// An upstream `audio_chunk`'s PCM from session `session`. Returns
    /// whether it went into a port.
    pub fn chunk(&self, session: u64, pcm: &[u8]) -> bool {
        let port = {
            let inner = lock(&self.inner);
            inner.inputs.iter().find_map(|i| match i.upstream {
                Upstream::Streaming { port } if i.session == session => Some(port),
                _ => None,
            })
        };
        match port.and_then(|p| self.ports.get(p)) {
            Some(port) => port.write_pcm(pcm, self.format.sample_format),
            None => false,
        }
    }

    /// Session `session` ended: every input it offered is gone.
    pub fn session_ended(&self, session: u64) {
        let mut inner = lock(&self.inner);
        let gone: Vec<InputId> = inner
            .inputs
            .iter()
            .filter(|i| i.session == session)
            .map(|i| i.id.clone())
            .collect();
        inner.inputs.retain(|i| i.session != session);
        for id in gone {
            self.event(&mut inner, InputEvent::Gone(id));
        }
    }

    // --- the conductor's side --------------------------------------------

    /// Everything that happened since the last call, in order.
    pub fn take_events(&self) -> Vec<InputEvent> {
        std::mem::take(&mut lock(&self.inner).events)
    }

    /// Start `input`: give it a free port (emptied) and say where to send
    /// `source_control`. `None` when the input is not offered or every port
    /// is taken.
    pub fn start(&self, input: &InputId) -> Option<Addressed> {
        let mut inner = lock(&self.inner);
        let taken: Vec<usize> = inner
            .inputs
            .iter()
            .filter_map(|i| i.upstream.port())
            .collect();
        let i = inner.inputs.iter().position(|i| i.id == *input)?;
        let port = match inner.inputs[i].upstream.port() {
            Some(p) => p,
            None => (0..self.ports.len()).find(|p| !taken.contains(p))?,
        };
        self.ports[port].reset();
        inner.inputs[i].upstream = Upstream::Starting { port };
        Some(Addressed {
            session: inner.inputs[i].session,
            source_id: inner.inputs[i].source_id,
            port: Some(port),
        })
    }

    /// Stop `input`: free its port and say where to send `source_control`
    /// stop. `None` when it is not offered.
    pub fn stop(&self, input: &InputId) -> Option<Addressed> {
        let mut inner = lock(&self.inner);
        let i = inner.inputs.iter().position(|i| i.id == *input)?;
        inner.inputs[i].upstream = Upstream::Idle;
        Some(Addressed {
            session: inner.inputs[i].session,
            source_id: inner.inputs[i].source_id,
            port: None,
        })
    }

    /// The port `input`'s upstream goes into, while it is started.
    pub fn port_of(&self, input: &InputId) -> Option<usize> {
        lock(&self.inner)
            .inputs
            .iter()
            .find(|i| i.id == *input)
            .and_then(|i| i.upstream.port())
    }

    /// `line-ins offered=... streaming=... dropped=...` for a status line.
    pub fn report(&self) -> String {
        let inner = lock(&self.inner);
        let streaming = inner
            .inputs
            .iter()
            .filter(|i| matches!(i.upstream, Upstream::Streaming { .. }))
            .count();
        let dropped: u64 = self.ports.iter().map(|p| p.dropped()).sum();
        let accepted: u64 = self.ports.iter().map(|p| p.accepted()).sum();
        format!(
            "line-ins offered={} streaming={} ports={} chunks_accepted={} chunks_dropped={}",
            inner.inputs.len(),
            streaming,
            self.ports.len(),
            accepted,
            dropped
        )
    }
}

/// A bounded ring of upstream samples (interleaved, full scale 1.0) between a
/// session's reader and the audio thread. Allocated once.
#[derive(Debug)]
pub struct Port {
    ring: Mutex<Ring>,
    channels: usize,
    dropped: AtomicU64,
    accepted: AtomicU64,
}

#[derive(Debug)]
struct Ring {
    buf: Vec<f64>,
    start: usize,
    len: usize,
    /// Moves on every reset, so the audio thread knows a new stream began.
    generation: u64,
}

impl Port {
    /// A port holding `frames` frames of `channels` channels.
    pub fn new(frames: usize, channels: usize) -> Port {
        Port {
            ring: Mutex::new(Ring {
                buf: vec![0.0; frames.max(1) * channels.max(1)],
                start: 0,
                len: 0,
                generation: 0,
            }),
            channels: channels.max(1),
            dropped: AtomicU64::new(0),
            accepted: AtomicU64::new(0),
        }
    }

    /// Empty it for a new stream.
    pub fn reset(&self) {
        let mut ring = lock(&self.ring);
        ring.start = 0;
        ring.len = 0;
        ring.generation += 1;
    }

    /// Chunks written whole.
    pub fn accepted(&self) -> u64 {
        self.accepted.load(Ordering::Relaxed)
    }

    /// Chunks dropped because the ring was full.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Write one chunk's little-endian PCM, converted to full scale 1.0.
    /// A chunk that does not fit whole is dropped and counted: the ring never
    /// holds part of a chunk, so what plays is never a splice inside one.
    pub fn write_pcm(&self, pcm: &[u8], format: SampleFormat) -> bool {
        let width = format.bytes_per_sample();
        let samples = pcm.len() / width;
        let samples = samples - samples % self.channels;
        let mut ring = lock(&self.ring);
        let cap = ring.buf.len();
        if ring.len + samples > cap {
            drop(ring);
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        let mut at = (ring.start + ring.len) % cap;
        for s in pcm.chunks_exact(width).take(samples) {
            ring.buf[at] = decode_sample(s, format);
            at = (at + 1) % cap;
        }
        ring.len += samples;
        drop(ring);
        self.accepted.fetch_add(1, Ordering::Relaxed);
        true
    }

    /// Hand at most `max_frames` whole frames, oldest first, to `take` (in
    /// at most two slices), and forget them. Returns the frames handed and
    /// the ring's generation. Allocates nothing.
    pub fn drain(&self, max_frames: usize, mut take: impl FnMut(&[f64])) -> (usize, u64) {
        let mut ring = lock(&self.ring);
        let cap = ring.buf.len();
        let frames = (ring.len / self.channels).min(max_frames);
        let samples = frames * self.channels;
        let first = samples.min(cap - ring.start);
        if first > 0 {
            take(&ring.buf[ring.start..ring.start + first]);
        }
        if samples > first {
            take(&ring.buf[..samples - first]);
        }
        ring.start = (ring.start + samples) % cap;
        ring.len -= samples;
        (frames, ring.generation)
    }

    /// The ring's generation, without draining.
    pub fn generation(&self) -> u64 {
        lock(&self.ring).generation
    }
}

/// One little-endian sample as full scale 1.0.
pub fn decode_sample(bytes: &[u8], format: SampleFormat) -> f64 {
    match format {
        SampleFormat::PcmS16Le => f64::from(i16::from_le_bytes([bytes[0], bytes[1]])) / 32_768.0,
        SampleFormat::PcmS24Le => {
            let v = i32::from_le_bytes([0, bytes[0], bytes[1], bytes[2]]) >> 8;
            f64::from(v) / 8_388_608.0
        }
        SampleFormat::PcmF32Le => {
            f64::from(f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        }
    }
}

/// One sample at full scale 1.0 into `out` as little-endian `format`,
/// clamped to the format's range.
pub fn encode_sample(x: f64, format: SampleFormat, out: &mut [u8]) {
    match format {
        SampleFormat::PcmS16Le => {
            let v = (x * 32_768.0).round().clamp(-32_768.0, 32_767.0) as i16;
            out[..2].copy_from_slice(&v.to_le_bytes());
        }
        SampleFormat::PcmS24Le => {
            let v = (x * 8_388_608.0).round().clamp(-8_388_608.0, 8_388_607.0) as i32;
            out[..3].copy_from_slice(&v.to_le_bytes()[..3]);
        }
        SampleFormat::PcmF32Le => {
            out[..4].copy_from_slice(&(x.clamp(-1.0, 1.0) as f32).to_le_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_port_keeps_whole_chunks_in_order_and_drops_what_does_not_fit() {
        let port = Port::new(4, 2);
        let chunk =
            |v: i16| -> Vec<u8> { std::iter::repeat_n(v.to_le_bytes(), 4).flatten().collect() };
        assert!(port.write_pcm(&chunk(1000), SampleFormat::PcmS16Le));
        assert!(port.write_pcm(&chunk(2000), SampleFormat::PcmS16Le));
        assert!(
            !port.write_pcm(&chunk(3000), SampleFormat::PcmS16Le),
            "the ring holds 4 frames"
        );
        assert_eq!(port.dropped(), 1);
        let mut got = Vec::new();
        let (frames, _) = port.drain(3, |s| got.extend_from_slice(s));
        assert_eq!(frames, 3);
        assert_eq!(got.len(), 6);
        assert!((got[0] - 1000.0 / 32_768.0).abs() < 1e-12);
        assert!((got[5] - 2000.0 / 32_768.0).abs() < 1e-12);
        // Wraps around.
        assert!(port.write_pcm(&chunk(3000), SampleFormat::PcmS16Le));
        let mut rest = Vec::new();
        let (frames, _) = port.drain(10, |s| rest.extend_from_slice(s));
        assert_eq!(frames, 3);
        assert!((rest[5] - 3000.0 / 32_768.0).abs() < 1e-12);
    }

    #[test]
    fn samples_round_trip_in_every_format() {
        for (format, x) in [
            (SampleFormat::PcmS16Le, 0.25),
            (SampleFormat::PcmS24Le, -0.5),
            (SampleFormat::PcmF32Le, 0.125),
        ] {
            let mut b = [0u8; 4];
            encode_sample(x, format, &mut b);
            assert_eq!(decode_sample(&b, format), x, "{:?}", format);
        }
    }

    #[test]
    fn an_offer_is_named_signalled_started_and_gone() {
        let format = PcmFormat::new(48_000, 2, "pcm_s16le").unwrap();
        let lineins = LineIns::new(format, 1, Box::new(|| {}));
        let offer = SourceOffer {
            source_id: 1,
            kind: chorus_protocol::v2::SourceKind::LineIn,
            signal: true,
            name: String::new(),
        };
        lineins.offer("amp", 7, &offer);
        let id = InputId::parse("amp/line-1").unwrap();
        assert_eq!(
            lineins.take_events(),
            vec![InputEvent::Signal(id.clone(), true)]
        );
        lineins.offer("amp", 7, &offer);
        assert!(lineins.take_events().is_empty(), "no change, no event");
        let at = lineins.start(&id).unwrap();
        assert_eq!((at.session, at.source_id, at.port), (7, 1, Some(0)));
        assert!(!lineins.chunk(7, &[0; 8]), "not before its stream_format");
        let wrong = StreamFormat {
            codec: Codec::Pcm,
            sample_format: SampleFormat::PcmS16Le,
            sample_rate_hz: 44_100,
            channel_map: vec![
                chorus_protocol::v2::ChannelPosition::FrontLeft,
                chorus_protocol::v2::ChannelPosition::FrontRight,
            ],
            frames_per_chunk: 882,
            codec_config: Vec::new(),
        };
        lineins.stream_format(7, &wrong);
        assert!(matches!(
            lineins.take_events().as_slice(),
            [InputEvent::Refused { .. }]
        ));
        lineins.start(&id).unwrap();
        let right = StreamFormat {
            sample_rate_hz: 48_000,
            frames_per_chunk: 960,
            ..wrong
        };
        lineins.stream_format(7, &right);
        assert!(lineins.chunk(7, &[0; 8]));
        lineins.session_ended(7);
        assert_eq!(lineins.take_events(), vec![InputEvent::Gone(id.clone())]);
        assert!(lineins.port_of(&id).is_none());
    }
}
