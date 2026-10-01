//! The source role: an input on this endpoint (a line-in, optical or HDMI
//! ARC) captured and sent upstream to the server (K65, `docs/protocol.md`
//! "Source").
//!
//! # What happens, in order
//!
//! 1. `hello` carries the source role beside the player role, and right after
//!    `capabilities` the endpoint sends a `source_offer` with no signal
//!    (`crate::session`): nothing has been measured yet.
//! 2. The capture device is read continuously, one chunk at a time, whether
//!    or not the server wants the input, because presence is measured on it.
//!    [`SignalDetector`] turns the level over a window into "a signal is
//!    present", with hysteresis, and every change is a new `source_offer`.
//! 3. On `source_control start` the codec is checked ([`check_start`]): it has
//!    to be one the endpoint listed in `capabilities` and one this endpoint
//!    can send an input in, which on this client is PCM alone (it encodes
//!    nothing). Any other request is refused by name on the endpoint's log and
//!    nothing is sent; the input stays offered.
//! 4. An accepted start sends `stream_format` (PCM, the capture's shape, a
//!    channel map), then one `audio_chunk` per captured chunk, from the next
//!    chunk captured.
//! 5. On `stop`, `stream_end`.
//!
//! # The timestamp on a captured chunk
//!
//! A chunk's `timestamp_ns` is the instant its first frame was digitized,
//! mapped onto the server timeline through this endpoint's sync offset. Right
//! after a read of `n` frames returns at `now` (this endpoint's monotonic
//! clock: the same `MonotonicTimeline` the sync exchange stamps `t0` and `t3`
//! on), the device's capture delay `d` says the NEXT frame was digitized `d`
//! frames ago (`snd_pcm_delay` on a capture stream, alsa-lib's definition in
//! `chorus_alsa::Pcm::capture_delay_frames`), so the first of the `n` was
//! digitized at `now - (d + n) / rate`. The server-timeline stamp is that
//! plus the offset the playout loop published (`crate::sync::PublishedOffset`,
//! `server = client + offset`). No wall clock is read anywhere here.
//!
//! Chunks captured after a start and before any offset is known are held (at
//! most [`MAX_HELD_MS`] of them, the oldest dropped and counted) and stamped
//! when one is: a timestamp is never invented. An overrun loses frames on the
//! device; it is counted and logged, the sequence carries on, and the next
//! chunk's timestamp (from its own capture instant) shows the gap.
//!
//! # What is modelled and what is not
//!
//! The shipped client reads exactly one implementation of [`CaptureSource`],
//! [`AlsaCapture`], and no flag selects anything else, for the same reason
//! `crate::sink` gives: an input that reports itself as captured while
//! producing nothing is the failure to make impossible. The modelled capture
//! device the tests drive lives under `tests/`.

use std::collections::VecDeque;
use std::fmt;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, TryLockError};
use std::thread::{self, JoinHandle};

use chorus_alsa::{AlsaError, Format, Pcm};
use chorus_protocol::v2::session::SecureWriter;
use chorus_protocol::v2::{
    ChannelPosition, Codec, Message, SourceAction, SourceControl, SourceOffer, StreamFormat,
};
use chorus_protocol::{AudioChunk, SampleFormat, StreamEnd, RESERVED_LEN};

use crate::buffer::Counters;
use crate::config::LineInConfig;

/// The capture ring asked of the device, in microseconds: ten 20 ms chunks.
/// `ASSUMED`: enough that a reader scheduled a little late does not overrun,
/// not measured on a line-in.
pub const CAPTURE_BUFFER_US: u32 = 200_000;

/// The level window signal presence is judged over. `ASSUMED`.
pub const SIGNAL_WINDOW_MS: u32 = 100;

/// A window at or above this RMS level means a signal is present. `ASSUMED`
/// (from memory, not measured: a quiet passage of music at consumer line level
/// sits well above it, and an unconnected input's noise floor well below).
pub const SIGNAL_ON_DBFS: f64 = -50.0;

/// Below this RMS level a window counts as silent. The gap to
/// [`SIGNAL_ON_DBFS`] is the hysteresis. `ASSUMED`.
pub const SIGNAL_OFF_DBFS: f64 = -60.0;

/// How long the input has to stay silent before the signal is said to be
/// gone, so a pause between tracks is not a withdrawn source. `ASSUMED`.
pub const SIGNAL_OFF_HOLD_MS: u32 = 2_000;

/// Most audio held after a start while no sync offset is known yet. `ASSUMED`.
pub const MAX_HELD_MS: u32 = 1_000;

/// What one read from a capture device delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CaptureRead {
    /// Frames delivered.
    pub frames: u64,
    /// Whether the device signalled an overrun during this read: its own
    /// signal (`-EPIPE` from `snd_pcm_readi`), never an inference.
    pub overran: bool,
}

/// Why a capture device could not be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureError {
    /// The ALSA capture device failed.
    Alsa(AlsaError),
    /// A modelled device failed. Only reachable from the test suite.
    Modelled(String),
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CaptureError::Alsa(e) => write!(f, "{}", e),
            CaptureError::Modelled(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for CaptureError {}

impl From<AlsaError> for CaptureError {
    fn from(e: AlsaError) -> CaptureError {
        CaptureError::Alsa(e)
    }
}

/// Something that captures interleaved PCM and can say how long ago the next
/// frame was digitized.
pub trait CaptureSource: Send {
    /// The device name, for reports and errors.
    fn device(&self) -> &str;

    /// Bytes one frame occupies.
    fn frame_len(&self) -> usize;

    /// Fill `pcm` (whole frames), blocking until it is full.
    fn read(&mut self, pcm: &mut [u8]) -> Result<CaptureRead, CaptureError>;

    /// How long ago, in frames, the next frame a read returns was digitized;
    /// `None` while the device is in its overrun state.
    fn delay_frames(&mut self) -> Result<Option<i64>, CaptureError>;
}

/// The ALSA capture device the shipped client reads a line-in from.
pub struct AlsaCapture {
    pcm: Pcm,
}

impl AlsaCapture {
    /// Open the configured input's capture device in its configured shape.
    pub fn open(input: &LineInConfig) -> Result<AlsaCapture, CaptureError> {
        let format = match input.sample_format {
            SampleFormat::PcmS16Le => Format::S16Le,
            SampleFormat::PcmS24Le => Format::S24Packed3Le,
            SampleFormat::PcmF32Le => Format::F32Le,
        };
        let pcm = Pcm::open_capture(
            &input.device,
            format,
            input.channels,
            input.rate_hz,
            CAPTURE_BUFFER_US,
        )?;
        Ok(AlsaCapture { pcm })
    }
}

impl CaptureSource for AlsaCapture {
    fn device(&self) -> &str {
        self.pcm.device()
    }

    fn frame_len(&self) -> usize {
        self.pcm.frame_len()
    }

    fn read(&mut self, pcm: &mut [u8]) -> Result<CaptureRead, CaptureError> {
        let report = self.pcm.read(pcm)?;
        Ok(CaptureRead {
            frames: report.frames_read,
            overran: report.overran,
        })
    }

    fn delay_frames(&mut self) -> Result<Option<i64>, CaptureError> {
        Ok(self.pcm.capture_delay_frames()?)
    }
}

/// The thresholds signal presence is judged by.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SignalThresholds {
    /// The level window, in milliseconds.
    pub window_ms: u32,
    /// At or above: present.
    pub on_dbfs: f64,
    /// Below: silent.
    pub off_dbfs: f64,
    /// Silent this long: gone.
    pub off_hold_ms: u32,
}

impl Default for SignalThresholds {
    fn default() -> SignalThresholds {
        SignalThresholds {
            window_ms: SIGNAL_WINDOW_MS,
            on_dbfs: SIGNAL_ON_DBFS,
            off_dbfs: SIGNAL_OFF_DBFS,
            off_hold_ms: SIGNAL_OFF_HOLD_MS,
        }
    }
}

/// Signal presence from the level over a window, with hysteresis: one window
/// at or above the on level makes it present; it goes only after the level
/// has stayed below the off level for the hold time. A window between the two
/// levels changes nothing and restarts the hold.
#[derive(Debug, Clone)]
pub struct SignalDetector {
    thresholds: SignalThresholds,
    format: SampleFormat,
    window_frames: u64,
    hold_frames: u64,
    channels: usize,
    sum_sq: f64,
    samples: u64,
    frames: u64,
    quiet_frames: u64,
    present: bool,
}

impl SignalDetector {
    /// A detector for PCM of this shape, starting at "no signal".
    pub fn new(
        thresholds: SignalThresholds,
        rate_hz: u32,
        channels: u16,
        format: SampleFormat,
    ) -> SignalDetector {
        let per_ms = u64::from(rate_hz) / 1_000;
        SignalDetector {
            thresholds,
            format,
            window_frames: (per_ms * u64::from(thresholds.window_ms)).max(1),
            hold_frames: per_ms * u64::from(thresholds.off_hold_ms),
            channels: usize::from(channels.max(1)),
            sum_sq: 0.0,
            samples: 0,
            frames: 0,
            quiet_frames: 0,
            present: false,
        }
    }

    /// Whether a signal is present now.
    pub fn present(&self) -> bool {
        self.present
    }

    /// Feed interleaved PCM; the new verdict if it changed.
    pub fn push(&mut self, pcm: &[u8]) -> Option<bool> {
        let before = self.present;
        let width = self.format.bytes_per_sample();
        for frame in pcm.chunks_exact(width * self.channels) {
            for sample in frame.chunks_exact(width) {
                let v = sample_value(self.format, sample);
                self.sum_sq += v * v;
                self.samples += 1;
            }
            self.frames += 1;
            if self.frames >= self.window_frames {
                self.close_window();
            }
        }
        (self.present != before).then_some(self.present)
    }

    fn close_window(&mut self) {
        let rms = (self.sum_sq / self.samples.max(1) as f64).sqrt();
        let level = if rms > 0.0 {
            20.0 * rms.log10()
        } else {
            f64::NEG_INFINITY
        };
        if level >= self.thresholds.on_dbfs {
            self.present = true;
            self.quiet_frames = 0;
        } else if level < self.thresholds.off_dbfs {
            self.quiet_frames += self.frames;
            if self.quiet_frames >= self.hold_frames {
                self.present = false;
            }
        } else {
            self.quiet_frames = 0;
        }
        self.sum_sq = 0.0;
        self.samples = 0;
        self.frames = 0;
    }
}

/// One sample as a fraction of full scale.
fn sample_value(format: SampleFormat, bytes: &[u8]) -> f64 {
    match format {
        SampleFormat::PcmS16Le => f64::from(i16::from_le_bytes([bytes[0], bytes[1]])) / 32_768.0,
        SampleFormat::PcmS24Le => {
            let v = i32::from_le_bytes([0, bytes[0], bytes[1], bytes[2]]) >> 8;
            f64::from(v) / 8_388_608.0
        }
        SampleFormat::PcmF32Le => {
            let v = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            if v.is_finite() {
                f64::from(v)
            } else {
                0.0
            }
        }
    }
}

/// The codecs this endpoint can send an input in: PCM alone, since it encodes
/// nothing (FLAC and Opus are decoded here, never encoded).
pub fn codecs_sent() -> u8 {
    Codec::Pcm.bit()
}

/// Why a `source_control start` was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartRefusal {
    /// The server named an input this endpoint does not have.
    UnknownSource {
        /// The id asked for.
        asked: u8,
        /// The id this endpoint offers.
        offered: u8,
    },
    /// The codec is not one this endpoint listed in `capabilities`.
    CodecNotListed {
        /// The codec asked for.
        codec: Codec,
    },
    /// The codec is listed (this endpoint plays it) and this endpoint cannot
    /// send an input in it.
    CodecNotSent {
        /// The codec asked for.
        codec: Codec,
    },
}

impl StartRefusal {
    /// The `reason=` word for this refusal.
    pub fn name(&self) -> &'static str {
        match self {
            StartRefusal::UnknownSource { .. } => "unknown-source",
            StartRefusal::CodecNotListed { .. } => "codec-not-listed",
            StartRefusal::CodecNotSent { .. } => "codec-not-sent",
        }
    }
}

impl fmt::Display for StartRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StartRefusal::UnknownSource { asked, offered } => write!(
                f,
                "the server asked for source {} and this endpoint offers source {}",
                asked, offered
            ),
            StartRefusal::CodecNotListed { codec } => write!(
                f,
                "the server asked for the input in {}, which this endpoint did not list in its \
                 capabilities",
                codec.name()
            ),
            StartRefusal::CodecNotSent { codec } => write!(
                f,
                "the server asked for the input in {}; this endpoint sends an input in pcm only \
                 (it encodes nothing)",
                codec.name()
            ),
        }
    }
}

/// Whether a `start` can be honoured: the input exists, and the codec is one
/// the endpoint listed (`listed_codecs`, its `capabilities.codecs`) and one it
/// can send an input in ([`codecs_sent`]).
pub fn check_start(
    control: &SourceControl,
    input: &LineInConfig,
    listed_codecs: u8,
) -> Result<(), StartRefusal> {
    if control.source_id != input.source_id {
        return Err(StartRefusal::UnknownSource {
            asked: control.source_id,
            offered: input.source_id,
        });
    }
    if listed_codecs & control.codec.bit() == 0 {
        return Err(StartRefusal::CodecNotListed {
            codec: control.codec,
        });
    }
    if codecs_sent() & control.codec.bit() == 0 {
        return Err(StartRefusal::CodecNotSent {
            codec: control.codec,
        });
    }
    Ok(())
}

/// The `stream_format` a started input is announced with.
pub fn stream_format(input: &LineInConfig) -> StreamFormat {
    let channel_map = if input.channels == 1 {
        vec![ChannelPosition::Mono]
    } else {
        vec![ChannelPosition::FrontLeft, ChannelPosition::FrontRight]
    };
    StreamFormat {
        codec: Codec::Pcm,
        sample_format: input.sample_format,
        sample_rate_hz: input.rate_hz,
        channel_map,
        frames_per_chunk: input.frames_per_chunk(),
        codec_config: Vec::new(),
    }
}

/// What the source role has done, shared with whoever reports it.
#[derive(Debug, Default)]
pub struct SourceStats {
    /// Frames read from the capture device.
    pub frames_captured: AtomicU64,
    /// Overruns the capture device signalled.
    pub overruns: AtomicU64,
    /// `source_offer`s sent after the first one.
    pub offers_sent: AtomicU64,
    /// Starts honoured.
    pub starts: AtomicU64,
    /// Starts refused by name.
    pub refused_starts: AtomicU64,
    /// Stops honoured (each one sent `stream_end`).
    pub stops: AtomicU64,
    /// `audio_chunk`s sent upstream.
    pub chunks_sent: AtomicU64,
    /// Chunks dropped while held for want of a sync offset.
    pub dropped_no_offset: AtomicU64,
    /// Whether a signal is present, as last offered.
    pub signal: AtomicBool,
}

impl SourceStats {
    /// One status line.
    pub fn line(&self) -> String {
        let g = |a: &AtomicU64| a.load(Ordering::Relaxed);
        format!(
            "source frames_captured={} overruns={} offers={} starts={} refused_starts={} \
             stops={} chunks_sent={} dropped_no_offset={} signal={}",
            g(&self.frames_captured),
            g(&self.overruns),
            g(&self.offers_sent),
            g(&self.starts),
            g(&self.refused_starts),
            g(&self.stops),
            g(&self.chunks_sent),
            g(&self.dropped_no_offset),
            u8::from(self.signal.load(Ordering::Relaxed))
        )
    }
}

/// Where the source role's messages go: up the session.
pub trait Upstream: Send {
    /// Send one message in a record of its own.
    fn send(&mut self, message: &Message) -> io::Result<()>;
}

/// One session writer shared by the playout loop's time-sync exchange (as a
/// byte [`Write`]) and the source role (as an [`Upstream`]). Each write or send
/// holds the lock for one whole frame, so records never interleave.
///
/// The playout loop never waits behind the source role: a byte write (the
/// time-sync request, one whole frame per write) that finds the writer busy
/// is dropped whole and counted ([`SharedWriter::dropped_writes`]). A missed
/// request costs one exchange, which the filter is built to tolerate; a
/// playout loop stalled behind an upstream send would cost audio.
pub struct SharedWriter<W: Write> {
    inner: Arc<Mutex<SecureWriter<W>>>,
    dropped: Arc<AtomicU64>,
}

impl<W: Write> SharedWriter<W> {
    /// Share `writer`.
    pub fn new(writer: SecureWriter<W>) -> SharedWriter<W> {
        SharedWriter {
            inner: Arc::new(Mutex::new(writer)),
            dropped: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Byte writes dropped because the writer was busy.
    pub fn dropped_writes(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, SecureWriter<W>> {
        match self.inner.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

impl<W: Write> Clone for SharedWriter<W> {
    fn clone(&self) -> SharedWriter<W> {
        SharedWriter {
            inner: Arc::clone(&self.inner),
            dropped: Arc::clone(&self.dropped),
        }
    }
}

impl<W: Write> Write for SharedWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.inner.try_lock() {
            Ok(mut g) => g.write(buf),
            Err(TryLockError::Poisoned(p)) => p.into_inner().write(buf),
            Err(TryLockError::WouldBlock) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                Ok(buf.len())
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.inner.try_lock() {
            Ok(mut g) => g.flush(),
            Err(TryLockError::Poisoned(p)) => p.into_inner().flush(),
            Err(TryLockError::WouldBlock) => Ok(()),
        }
    }
}

impl<W: Write + Send> Upstream for SharedWriter<W> {
    fn send(&mut self, message: &Message) -> io::Result<()> {
        self.lock().send(message)
    }
}

/// Why the source role stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceStop {
    /// The session is ending (the caller said so).
    Stopped,
    /// The capture device failed.
    DeviceFailed(String),
    /// A send up the session failed.
    ConnectionLost(String),
}

impl SourceStop {
    /// The `reason=` word.
    pub fn name(&self) -> &'static str {
        match self {
            SourceStop::Stopped => "stopped",
            SourceStop::DeviceFailed(_) => "capture-device-failed",
            SourceStop::ConnectionLost(_) => "connection-lost",
        }
    }
}

/// Everything the source role runs with besides its device and its upstream.
pub struct SourceSetup {
    /// The configured input.
    pub input: LineInConfig,
    /// The codecs this endpoint listed in `capabilities`.
    pub listed_codecs: u8,
    /// This endpoint's monotonic clock, in ns: the timeline the sync exchange
    /// stamps on.
    pub clock: Box<dyn Fn() -> u64 + Send>,
    /// The session's counters, whose `offset` the playout loop publishes.
    pub counters: Arc<Counters>,
    /// The server's `source_control`s.
    pub controls: Receiver<SourceControl>,
    /// Signal presence thresholds.
    pub thresholds: SignalThresholds,
    /// Where status lines go.
    pub log: Box<dyn FnMut(&str) + Send>,
}

/// A running source role.
pub struct SourceHandle {
    keep: Arc<AtomicBool>,
    join: JoinHandle<SourceStop>,
    /// What it has done so far.
    pub stats: Arc<SourceStats>,
}

impl SourceHandle {
    /// Ask it to stop (a started input is ended with `stream_end`) and wait
    /// for the read in progress to return.
    pub fn stop(self) -> (SourceStop, Arc<SourceStats>) {
        self.keep.store(false, Ordering::SeqCst);
        let stop = self
            .join
            .join()
            .unwrap_or_else(|_| SourceStop::DeviceFailed("the source thread panicked".into()));
        (stop, self.stats)
    }
}

/// Run the source role on its own thread.
pub fn spawn<C, U>(capture: C, upstream: U, setup: SourceSetup) -> SourceHandle
where
    C: CaptureSource + 'static,
    U: Upstream + 'static,
{
    let keep = Arc::new(AtomicBool::new(true));
    let stats = Arc::new(SourceStats::default());
    let join = {
        let keep = Arc::clone(&keep);
        let stats = Arc::clone(&stats);
        thread::spawn(move || {
            let mut capture = capture;
            let mut upstream = upstream;
            let go = move || keep.load(Ordering::SeqCst);
            run(&mut capture, &mut upstream, setup, &go, &stats)
        })
    };
    SourceHandle { keep, join, stats }
}

/// A started input.
struct Streaming {
    next_sequence: u32,
    sent_any: bool,
    last_timestamp_ns: u64,
    held: VecDeque<(u64, Vec<u8>)>,
}

/// The source role's loop. [`spawn`] runs it on a thread; it returns when
/// `keep` goes false, the device fails, or the session does.
pub fn run<C: CaptureSource, U: Upstream>(
    capture: &mut C,
    upstream: &mut U,
    mut setup: SourceSetup,
    keep: &dyn Fn() -> bool,
    stats: &SourceStats,
) -> SourceStop {
    let input = setup.input.clone();
    let rate = u128::from(input.rate_hz.max(1));
    let chunk_frames = input.frames_per_chunk() as usize;
    let chunk_ns = (chunk_frames as u128 * 1_000_000_000 / rate) as u64;
    let max_held = (MAX_HELD_MS / crate::config::LINE_IN_CHUNK_MS).max(1) as usize;
    let mut detector = SignalDetector::new(
        setup.thresholds,
        input.rate_hz,
        input.channels,
        input.sample_format,
    );
    let mut pcm = vec![0u8; chunk_frames * capture.frame_len()];
    let mut streaming: Option<Streaming> = None;
    let log = &mut setup.log;

    let lost = |e: io::Error| SourceStop::ConnectionLost(e.to_string());
    loop {
        if !keep() {
            if let Some(s) = streaming.take() {
                let _ = end_stream(upstream, s, chunk_ns, &setup.counters, stats, &input);
            }
            return SourceStop::Stopped;
        }

        // The server's requests, in order, between chunks.
        while let Ok(control) = setup.controls.try_recv() {
            match control.action {
                SourceAction::Start => {
                    if let Err(refusal) = check_start(&control, &input, setup.listed_codecs) {
                        stats.refused_starts.fetch_add(1, Ordering::Relaxed);
                        log(&format!(
                            "source-start-refused source_id={} codec={} reason={} detail={}",
                            control.source_id,
                            control.codec.name(),
                            refusal.name(),
                            refusal
                        ));
                        continue;
                    }
                    if streaming.is_some() {
                        log(&format!(
                            "source-start-ignored source_id={} reason=already-started",
                            control.source_id
                        ));
                        continue;
                    }
                    if let Err(e) = upstream.send(&Message::StreamFormat(stream_format(&input))) {
                        return lost(e);
                    }
                    stats.starts.fetch_add(1, Ordering::Relaxed);
                    log(&format!(
                        "source-started source_id={} codec=pcm rate_hz={} channels={} \
                         sample_format={} frames_per_chunk={}",
                        input.source_id,
                        input.rate_hz,
                        input.channels,
                        input.sample_format.name(),
                        chunk_frames
                    ));
                    streaming = Some(Streaming {
                        next_sequence: 0,
                        sent_any: false,
                        last_timestamp_ns: 0,
                        held: VecDeque::new(),
                    });
                }
                SourceAction::Stop => match streaming.take() {
                    Some(s) => {
                        if let Err(e) =
                            end_stream(upstream, s, chunk_ns, &setup.counters, stats, &input)
                        {
                            return lost(e);
                        }
                        log(&format!("source-stopped source_id={}", input.source_id));
                    }
                    None => log(&format!(
                        "source-stop-ignored source_id={} reason=not-started",
                        control.source_id
                    )),
                },
            }
        }

        // One chunk from the device, and the instant its first frame was
        // digitized on this endpoint's clock.
        let read = match capture.read(&mut pcm) {
            Ok(r) => r,
            Err(e) => {
                log(&format!(
                    "source-device-failed source_id={} device={} detail={}",
                    input.source_id,
                    capture.device(),
                    e
                ));
                if let Some(s) = streaming.take() {
                    let _ = end_stream(upstream, s, chunk_ns, &setup.counters, stats, &input);
                }
                if stats.signal.swap(false, Ordering::Relaxed) {
                    let _ = offer(upstream, &input, false, stats);
                }
                return SourceStop::DeviceFailed(e.to_string());
            }
        };
        let now_ns = (setup.clock)();
        // `None` is the device in its overrun state right after this read;
        // the read itself reports that overrun, and the stamp then takes the
        // delay as zero (the chunk is at most a ring late, and the count says
        // so).
        let delay = match capture.delay_frames() {
            Ok(d) => d.unwrap_or(0).max(0) as u128,
            Err(e) => {
                log(&format!(
                    "source-device-failed source_id={} device={} detail={}",
                    input.source_id,
                    capture.device(),
                    e
                ));
                if let Some(s) = streaming.take() {
                    let _ = end_stream(upstream, s, chunk_ns, &setup.counters, stats, &input);
                }
                return SourceStop::DeviceFailed(e.to_string());
            }
        };
        let frames = read.frames as u128;
        let back_ns = ((delay + frames) * 1_000_000_000 / rate) as u64;
        let captured_at_ns = now_ns.saturating_sub(back_ns);
        stats
            .frames_captured
            .fetch_add(read.frames, Ordering::Relaxed);
        if read.overran {
            let n = stats.overruns.fetch_add(1, Ordering::Relaxed) + 1;
            log(&format!(
                "source-overrun source_id={} device={} overruns={} detail=the capture device \
                 signalled an overrun; frames before this chunk were lost",
                input.source_id,
                capture.device(),
                n
            ));
        }

        if let Some(present) = detector.push(&pcm) {
            stats.signal.store(present, Ordering::Relaxed);
            if let Err(e) = offer(upstream, &input, present, stats) {
                return lost(e);
            }
            log(&format!(
                "source-offer source_id={} signal={}",
                input.source_id,
                u8::from(present)
            ));
        }

        if let Some(s) = streaming.as_mut() {
            s.held.push_back((captured_at_ns, pcm.clone()));
            if s.held.len() > max_held {
                s.held.pop_front();
                stats.dropped_no_offset.fetch_add(1, Ordering::Relaxed);
            }
            if let Some(offset_ns) = setup.counters.offset.get() {
                if let Err(e) = send_held(upstream, s, offset_ns, &input, stats) {
                    return lost(e);
                }
            }
        }
    }
}

fn offer<U: Upstream>(
    upstream: &mut U,
    input: &LineInConfig,
    signal: bool,
    stats: &SourceStats,
) -> io::Result<()> {
    upstream.send(&Message::SourceOffer(SourceOffer {
        source_id: input.source_id,
        kind: input.kind,
        signal,
        name: input.name.clone(),
    }))?;
    stats.offers_sent.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

/// A capture instant on this endpoint's clock, on the server timeline.
fn server_time(captured_at_ns: u64, offset_ns: i64) -> u64 {
    (i128::from(captured_at_ns) + i128::from(offset_ns)).clamp(0, i128::from(u64::MAX)) as u64
}

fn send_held<U: Upstream>(
    upstream: &mut U,
    s: &mut Streaming,
    offset_ns: i64,
    input: &LineInConfig,
    stats: &SourceStats,
) -> io::Result<()> {
    while let Some((captured_at_ns, pcm)) = s.held.pop_front() {
        let timestamp_ns = server_time(captured_at_ns, offset_ns);
        upstream.send(&Message::AudioChunk(AudioChunk {
            sequence: s.next_sequence,
            timestamp_ns,
            sample_rate_hz: input.rate_hz,
            channels: input.channels,
            sample_format: input.sample_format,
            reserved: [0u8; RESERVED_LEN],
            audio_data: pcm,
        }))?;
        s.next_sequence = s.next_sequence.wrapping_add(1);
        s.sent_any = true;
        s.last_timestamp_ns = timestamp_ns;
        stats.chunks_sent.fetch_add(1, Ordering::Relaxed);
    }
    Ok(())
}

/// Send what is held (when an offset is known), then `stream_end`: the final
/// chunk's sequence, and one configured chunk duration past its timestamp
/// (`docs/protocol.md`, 0x03). A stream that sent no chunk ends with the
/// sequence before its first (wrapping) and an end timestamp of 0.
fn end_stream<U: Upstream>(
    upstream: &mut U,
    mut s: Streaming,
    chunk_ns: u64,
    counters: &Counters,
    stats: &SourceStats,
    input: &LineInConfig,
) -> io::Result<()> {
    match counters.offset.get() {
        Some(offset_ns) => send_held(upstream, &mut s, offset_ns, input, stats)?,
        None => {
            stats
                .dropped_no_offset
                .fetch_add(s.held.len() as u64, Ordering::Relaxed);
            s.held.clear();
        }
    }
    let end = if s.sent_any {
        StreamEnd {
            final_sequence: s.next_sequence.wrapping_sub(1),
            end_timestamp_ns: s.last_timestamp_ns.saturating_add(chunk_ns),
        }
    } else {
        StreamEnd {
            final_sequence: s.next_sequence.wrapping_sub(1),
            end_timestamp_ns: 0,
        }
    };
    upstream.send(&Message::StreamEnd(end))?;
    stats.stops.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chorus_protocol::v2::SourceKind;

    fn tone(frames: usize, amplitude: f64) -> Vec<u8> {
        let mut out = Vec::with_capacity(frames * 4);
        for i in 0..frames {
            let v = (amplitude * (i as f64 * 0.0576).sin() * 32_767.0) as i16;
            out.extend_from_slice(&v.to_le_bytes());
            out.extend_from_slice(&v.to_le_bytes());
        }
        out
    }

    fn detector() -> SignalDetector {
        SignalDetector::new(
            SignalThresholds::default(),
            48_000,
            2,
            SampleFormat::PcmS16Le,
        )
    }

    #[test]
    fn a_tone_is_a_signal_at_once_and_silence_takes_the_hold_to_withdraw_it() {
        let mut d = detector();
        assert_eq!(d.push(&vec![0u8; 48_000 * 4]), None, "silence stays absent");
        // -20 dBFS peak: well above the on level.
        let on = d.push(&tone(4_800, 0.1));
        assert_eq!(on, Some(true), "one window of tone makes it present");
        // 1.9 s of silence: still present (the hold is 2 s).
        assert_eq!(d.push(&vec![0u8; 91_200 * 4]), None);
        assert!(d.present());
        assert_eq!(d.push(&vec![0u8; 9_600 * 4]), Some(false));
    }

    #[test]
    fn a_level_between_the_thresholds_neither_starts_nor_ends_a_signal() {
        let mut d = detector();
        // About -55 dBFS RMS: between -60 and -50.
        let middling = tone(48_000, 0.0025);
        assert_eq!(d.push(&middling), None, "not loud enough to start one");
        d.push(&tone(4_800, 0.1));
        assert!(d.present());
        for _ in 0..5 {
            assert_eq!(d.push(&middling), None, "and not quiet enough to end one");
        }
    }

    #[test]
    fn the_twenty_four_bit_and_float_layouts_are_read_as_levels_too() {
        let mut d = SignalDetector::new(
            SignalThresholds::default(),
            48_000,
            1,
            SampleFormat::PcmS24Le,
        );
        let loud: Vec<u8> = (0..4_800)
            .flat_map(|i| {
                let v = if i % 2 == 0 { 0x40_0000i32 } else { -0x40_0000 };
                let b = v.to_le_bytes();
                [b[0], b[1], b[2]]
            })
            .collect();
        assert_eq!(d.push(&loud), Some(true));
        let mut f = SignalDetector::new(
            SignalThresholds::default(),
            48_000,
            1,
            SampleFormat::PcmF32Le,
        );
        let loud: Vec<u8> = (0..4_800).flat_map(|_| 0.5f32.to_le_bytes()).collect();
        assert_eq!(f.push(&loud), Some(true));
    }

    #[test]
    fn a_start_is_refused_by_name_for_an_unknown_input_an_unlisted_codec_or_one_not_sent() {
        let input = LineInConfig::new("hw:1");
        let start = |source_id, codec| SourceControl {
            source_id,
            action: SourceAction::Start,
            codec,
        };
        let listed = Codec::Pcm.bit() | Codec::Flac.bit();
        assert_eq!(check_start(&start(1, Codec::Pcm), &input, listed), Ok(()));
        let e = check_start(&start(2, Codec::Pcm), &input, listed).unwrap_err();
        assert_eq!(e.name(), "unknown-source");
        let e = check_start(&start(1, Codec::Opus), &input, listed).unwrap_err();
        assert_eq!(e.name(), "codec-not-listed");
        assert!(e.to_string().contains("did not list"), "{}", e);
        let e = check_start(&start(1, Codec::Flac), &input, listed).unwrap_err();
        assert_eq!(e.name(), "codec-not-sent");
        assert!(e.to_string().contains("pcm only"), "{}", e);
    }

    #[test]
    fn a_stereo_input_is_announced_front_left_and_right_and_a_mono_one_as_mono() {
        let mut input = LineInConfig::new("hw:1");
        let f = stream_format(&input);
        assert_eq!(f.codec, Codec::Pcm);
        assert_eq!(
            f.channel_map,
            [ChannelPosition::FrontLeft, ChannelPosition::FrontRight]
        );
        assert_eq!(f.frames_per_chunk, 960);
        input.channels = 1;
        input.kind = SourceKind::HdmiArc;
        assert_eq!(stream_format(&input).channel_map, [ChannelPosition::Mono]);
    }

    #[test]
    fn a_capture_instant_is_mapped_through_the_offset_and_never_wraps() {
        assert_eq!(server_time(5_000, 2_000), 7_000);
        assert_eq!(server_time(5_000, -2_000), 3_000);
        assert_eq!(server_time(5_000, -9_000), 0);
    }
}
