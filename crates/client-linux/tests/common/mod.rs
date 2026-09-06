//! A modelled audio device, and the pieces the client tests share.
//!
//! # Why a model, and exactly what it is and is not evidence for
//!
//! The client's buffering, its behaviour at each bound, its underrun
//! accounting, its drains and its exits are all decisions the client makes
//! about what a device told it. Those decisions are testable without a device,
//! and testing them without one is the only way they get tested at all on a
//! machine with no sound card. So this file models a device: a ring that
//! drains at the nominal rate against the same monotonic clock the client
//! uses, that reports its delay the way `alsa` defines it, that blocks when it
//! is full, and that signals an underrun when it runs dry.
//!
//! **What this is evidence for**: the client's logic. Start fill, the graded
//! interval, the ceiling behaviour, the counters, the drain, the exit codes.
//!
//! **What it is not evidence for**: that `libasound` behaves as modelled, that
//! a real card's reported delay tracks this way, or that ten minutes of audio
//! came out of a speaker. Those need a device, and this repository's own
//! working agreement is blunt about the difference between a simulation and a
//! measurement. The evidence for those lives in a run on real hardware.
//!
//! # Two modelled devices, and why there are two
//!
//! [`ModelledDevice`] runs against the real monotonic clock and blocks like a
//! real blocking write does. It is what the playout tests need, because what
//! they are about is a client thread's decisions in real time.
//!
//! [`ModelledDac`] runs against [`VirtualClock`], which a test advances by
//! hand, and never blocks. It is what the sync tests need, because an hour of
//! a loop that corrects at one exchange a second is an hour of wall clock
//! against the first one and about a second against the second one. The
//! difference is the clock, not the model: both drain at a nominal rate, both
//! report their delay the way `alsa` defines it, and neither infers an
//! underrun from that delay.
//!
//! An hour of modelled time is a MODELLED result and is never a measurement.
//! `docs/verification-record.md` says so where the numbers are recorded.
//!
//! This file is under `tests/`, not in the library, so no binary can reach it.

#![allow(dead_code)]

use std::collections::VecDeque;
use std::io::{self, Read};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chorus_client_linux::sink::{PcmSink, SinkError, SinkWrite};
use chorus_client_linux::sync::{Correction, PlayoutCorrector, SyncConfig, SyncLoop};
use chorus_protocol::{encode, AudioChunk, Message, SampleFormat, StreamEnd, TimeSync, RESERVED_LEN};
use chorus_sync::{JitterModel, Rng};

/// A device modelled closely enough to drive the client's decisions.
pub struct ModelledDevice {
    device: String,
    rate_hz: u32,
    frame_len: usize,
    ring_frames: u64,
    /// Frames accepted and not yet played.
    queued: f64,
    /// Frames played out since the device opened.
    played: u64,
    last_tick: Instant,
    in_xrun: bool,
    /// True between a drain and the next write. A stopped device is not an
    /// underrunning one: a real `snd_pcm_drain` leaves the stream in SETUP,
    /// not in XRUN, which is exactly why draining is the right way to end a
    /// run and letting the ring dry out is not.
    stopped: bool,
    underruns: u64,
    /// Set to make every call fail, which is how a device going away mid-run
    /// is modelled.
    fails_from: Arc<Mutex<Option<Instant>>>,
    /// Set to answer every delay query with zero however much the ring holds,
    /// which is what the ALSA `null` device does for ever.
    reports_zero_delay: bool,
}

impl ModelledDevice {
    pub fn new(device: &str, rate_hz: u32, frame_len: usize, ring_us: u64) -> ModelledDevice {
        ModelledDevice {
            device: device.to_string(),
            rate_hz,
            frame_len,
            ring_frames: ring_us * u64::from(rate_hz) / 1_000_000,
            queued: 0.0,
            played: 0,
            last_tick: Instant::now(),
            in_xrun: false,
            stopped: true,
            underruns: 0,
            fails_from: Arc::new(Mutex::new(None)),
            reports_zero_delay: false,
        }
    }

    /// A handle that makes the device fail from now on, modelling a device
    /// that was removed mid-run.
    pub fn failure_switch(&self) -> Arc<Mutex<Option<Instant>>> {
        Arc::clone(&self.fails_from)
    }

    /// Make it report a delay of zero for ever, the way the ALSA `null` device
    /// does. It goes on accepting and playing frames: what changes is what it
    /// says about how far they are from a DAC.
    pub fn reports_zero_delay(&mut self) {
        self.reports_zero_delay = true;
    }

    pub fn underruns(&self) -> u64 {
        self.underruns
    }

    fn failed(&self) -> bool {
        self.fails_from
            .lock()
            .map(|g| g.is_some())
            .unwrap_or(false)
    }

    /// Advance the model to now: the DAC has consumed frames at the nominal
    /// rate since the last time anyone looked.
    fn tick(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_tick).as_secs_f64();
        self.last_tick = now;
        if self.stopped {
            return;
        }
        let consumable = elapsed * f64::from(self.rate_hz);
        if consumable <= 0.0 {
            return;
        }
        if self.queued <= 0.0 {
            // Already dry, and the dry spell was counted when it started. A
            // device does not signal a new underrun every time it is asked.
            self.queued = 0.0;
            return;
        }
        let played = consumable.min(self.queued);
        self.queued -= played;
        self.played += played as u64;
        if self.queued <= 0.0 {
            // The ring ran dry while the stream was running. A real device
            // signals XRUN here, and `alsa` warns that its reported delay
            // "will not necessarily got down to 0", which is why the client
            // must never infer this from the delay.
            self.queued = 0.0;
            if !self.in_xrun {
                self.in_xrun = true;
                self.underruns += 1;
            }
        }
    }
}

impl PcmSink for ModelledDevice {
    fn device(&self) -> &str {
        &self.device
    }

    fn frame_len(&self) -> usize {
        self.frame_len
    }

    fn rate_hz(&self) -> u32 {
        self.rate_hz
    }

    fn write(&mut self, pcm: &[u8]) -> Result<SinkWrite, SinkError> {
        if self.failed() {
            return Err(SinkError::Modelled(format!(
                "modelled device '{}' was removed",
                self.device
            )));
        }
        let frames = (pcm.len() / self.frame_len) as u64;
        let mut spun = 0;
        if self.stopped {
            // A write after a drain restarts the stream, as a prepare would.
            self.stopped = false;
            self.last_tick = Instant::now();
        }
        loop {
            self.tick();
            if self.queued + frames as f64 <= self.ring_frames as f64 {
                break;
            }
            // A blocking write waits for room, exactly as ALSA's does.
            std::thread::sleep(Duration::from_millis(1));
            spun += 1;
            if spun > 60_000 {
                return Err(SinkError::Modelled(
                    "the modelled device never made room".to_string(),
                ));
            }
        }
        let underran = self.in_xrun;
        if self.in_xrun {
            // Recover, the way snd_pcm_recover does, and keep going.
            self.in_xrun = false;
        }
        self.queued += frames as f64;
        Ok(SinkWrite {
            frames_written: frames,
            underran,
        })
    }

    fn delay_frames(&mut self) -> Result<i64, SinkError> {
        if self.failed() {
            return Err(SinkError::Modelled(format!(
                "modelled device '{}' was removed",
                self.device
            )));
        }
        self.tick();
        if self.reports_zero_delay {
            return Ok(0);
        }
        Ok(self.queued as i64)
    }

    fn in_xrun(&mut self) -> Result<bool, SinkError> {
        if self.failed() {
            return Err(SinkError::Modelled(format!(
                "modelled device '{}' was removed",
                self.device
            )));
        }
        self.tick();
        let was = self.in_xrun;
        // The client asks once per loop; report the state once and clear it,
        // so one dry spell is one underrun rather than one per poll.
        self.in_xrun = false;
        Ok(was)
    }

    fn drain(&mut self) -> Result<(), SinkError> {
        if self.failed() {
            return Err(SinkError::Modelled(format!(
                "modelled device '{}' was removed",
                self.device
            )));
        }
        // Play out what is held, in real time, without ever running dry: a
        // drain is not an underrun. Taking real time matters, because a
        // drain that finished instantly would make the played-out frame count
        // disagree with the device's nominal rate and turn the no-rate-change
        // check into a check on the model.
        self.tick();
        let held = self.queued;
        if held > 0.0 {
            let seconds = held / f64::from(self.rate_hz);
            std::thread::sleep(Duration::from_secs_f64(seconds));
        }
        self.played += held as u64;
        self.queued = 0.0;
        self.in_xrun = false;
        self.stopped = true;
        self.last_tick = Instant::now();
        Ok(())
    }

    fn frames_played(&mut self) -> Result<u64, SinkError> {
        self.tick();
        Ok(self.played)
    }
}

/// Build one encoded audio chunk frame.
pub fn chunk_frame(sequence: u32, timestamp_ns: u64, frames: usize) -> Vec<u8> {
    encode(&Message::AudioChunk(AudioChunk {
        sequence,
        timestamp_ns,
        sample_rate_hz: 48_000,
        channels: 2,
        sample_format: SampleFormat::PcmS16Le,
        reserved: [0u8; RESERVED_LEN],
        audio_data: vec![(sequence % 251) as u8; frames * 4],
    }))
    .expect("a well formed chunk encodes")
}

/// Build the in-band end-of-stream frame.
pub fn end_frame(final_sequence: u32, end_timestamp_ns: u64) -> Vec<u8> {
    encode(&Message::StreamEnd(StreamEnd {
        final_sequence,
        end_timestamp_ns,
    }))
    .expect("a stream end encodes")
}

/// A reader that hands out bytes at a chosen pace, so a test can feed the
/// client faster or slower than its device consumes without a socket.
pub struct PacedReader {
    parts: Vec<(Duration, Vec<u8>)>,
    at: usize,
    offset: usize,
    started: Instant,
    /// Set to end the stream with a close rather than running out of parts.
    close_at_end: bool,
    reads: Arc<AtomicU64>,
}

impl PacedReader {
    pub fn new(parts: Vec<(Duration, Vec<u8>)>, close_at_end: bool) -> PacedReader {
        PacedReader {
            parts,
            at: 0,
            offset: 0,
            started: Instant::now(),
            close_at_end,
            reads: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn reads(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.reads)
    }
}

impl Read for PacedReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            if self.at >= self.parts.len() {
                if self.close_at_end {
                    return Ok(0);
                }
                std::thread::sleep(Duration::from_millis(5));
                return Err(io::Error::new(io::ErrorKind::WouldBlock, "paced reader idle"));
            }
            let (due, bytes) = &self.parts[self.at];
            if self.started.elapsed() < *due {
                std::thread::sleep(Duration::from_millis(1));
                return Err(io::Error::new(io::ErrorKind::WouldBlock, "not due yet"));
            }
            let remaining = &bytes[self.offset..];
            if remaining.is_empty() {
                self.at += 1;
                self.offset = 0;
                continue;
            }
            let n = remaining.len().min(buf.len());
            buf[..n].copy_from_slice(&remaining[..n]);
            self.offset += n;
            self.reads.fetch_add(1, Ordering::Relaxed);
            return Ok(n);
        }
    }
}

/// A stream of `count` chunks, each due at its nominal time scaled by
/// `rate_scale` (below 1.0 is faster than real time).
pub fn paced_chunks(
    count: u32,
    frames_per_chunk: usize,
    chunk_us: u64,
    rate_scale: f64,
    close_at_end: bool,
    with_end_signal: bool,
) -> PacedReader {
    let mut parts = Vec::new();
    for i in 0..count {
        let due = Duration::from_micros((chunk_us as f64 * i as f64 * rate_scale) as u64);
        parts.push((
            due,
            chunk_frame(
                i,
                u64::from(i) * chunk_us * 1_000,
                frames_per_chunk,
            ),
        ));
    }
    if with_end_signal {
        let due = Duration::from_micros((chunk_us as f64 * count as f64 * rate_scale) as u64);
        parts.push((
            due,
            end_frame(count.saturating_sub(1), u64::from(count) * chunk_us * 1_000),
        ));
    }
    PacedReader::new(parts, close_at_end)
}

// --------------------------------------------------------------------------
// The modelled closed loop: virtual clock, virtual DAC, virtual server.
// --------------------------------------------------------------------------

/// A clock a test advances by hand, in nanoseconds.
#[derive(Debug, Clone, Default)]
pub struct VirtualClock(Arc<AtomicU64>);

impl VirtualClock {
    /// A clock at zero.
    pub fn new() -> VirtualClock {
        VirtualClock::default()
    }

    /// Nanoseconds since this clock's epoch.
    pub fn now_ns(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }

    /// Move it forward.
    pub fn set_ns(&self, ns: u64) {
        self.0.store(ns, Ordering::SeqCst);
    }
}

/// How a modelled DAC misbehaves, for the unhappy paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DacFault {
    /// It behaves.
    #[default]
    None,
    /// It will not report its delay, and says why.
    RefusesDelay,
    /// It answers every delay query with zero however much it holds, which is
    /// what the ALSA `null` device does for ever and what a real card can
    /// report while it is in an xrun. It still plays what it is given: what is
    /// modelled here is the report, not the ring.
    ReportsZeroDelay,
}

/// A DAC that drains on a virtual clock and reports its delay the way `alsa`
/// defines it: frames a write made now would wait behind.
///
/// It never blocks. The harness paces its own writes, which is what the real
/// playout loop does too; a blocking write here would only model the thread
/// sleeping, which is not what any of these tests are about.
#[derive(Debug)]
pub struct ModelledDac {
    device: String,
    /// The rate the DEVICE is configured at and the client converts frames
    /// with. The client has no way to learn the true one.
    nominal_rate_hz: u32,
    /// The crystal error of this endpoint, in ppm.
    ppm: f64,
    frame_len: usize,
    clock: VirtualClock,
    queued: f64,
    played: f64,
    last_tick_ns: u64,
    in_xrun: bool,
    underruns: u64,
    fault: DacFault,
    /// Bytes actually handed over, so a test can check WHAT was written.
    pub written: Vec<u8>,
    keep_written: bool,
}

impl ModelledDac {
    /// A DAC at the nominal rate, whose crystal is `ppm` off it.
    pub fn new(device: &str, nominal_rate_hz: u32, frame_len: usize, ppm: f64, clock: VirtualClock) -> ModelledDac {
        let last_tick_ns = clock.now_ns();
        ModelledDac {
            device: device.to_string(),
            nominal_rate_hz,
            ppm,
            frame_len,
            clock,
            queued: 0.0,
            played: 0.0,
            last_tick_ns,
            in_xrun: false,
            underruns: 0,
            fault: DacFault::None,
            written: Vec::new(),
            keep_written: false,
        }
    }

    /// Keep every byte handed over, so a test can look at it.
    pub fn keep_written(&mut self) {
        self.keep_written = true;
    }

    /// Make it refuse to report its delay from now on.
    pub fn set_fault(&mut self, fault: DacFault) {
        self.fault = fault;
    }

    /// Frames it currently holds, which no participant in the model can see.
    pub fn queued_frames(&self) -> f64 {
        self.queued
    }

    /// Underruns it has signalled.
    pub fn underruns(&self) -> u64 {
        self.underruns
    }

    /// Frames per second this crystal actually produces.
    pub fn true_rate_hz(&self) -> f64 {
        f64::from(self.nominal_rate_hz) * (1.0 + self.ppm * 1e-6)
    }

    fn tick(&mut self) {
        let now = self.clock.now_ns();
        let elapsed_ns = now.saturating_sub(self.last_tick_ns);
        self.last_tick_ns = now;
        if elapsed_ns == 0 {
            return;
        }
        let consumable = elapsed_ns as f64 / 1e9 * self.true_rate_hz();
        if self.queued <= 0.0 {
            self.queued = 0.0;
            return;
        }
        let played = consumable.min(self.queued);
        self.queued -= played;
        self.played += played;
        if self.queued <= 0.0 {
            self.queued = 0.0;
            if !self.in_xrun {
                self.in_xrun = true;
                self.underruns += 1;
            }
        }
    }
}

impl PcmSink for ModelledDac {
    fn device(&self) -> &str {
        &self.device
    }

    fn frame_len(&self) -> usize {
        self.frame_len
    }

    fn rate_hz(&self) -> u32 {
        self.nominal_rate_hz
    }

    fn write(&mut self, pcm: &[u8]) -> Result<SinkWrite, SinkError> {
        self.tick();
        let frames = (pcm.len() / self.frame_len) as u64;
        let underran = self.in_xrun;
        self.in_xrun = false;
        self.queued += frames as f64;
        if self.keep_written {
            self.written.extend_from_slice(pcm);
        }
        Ok(SinkWrite {
            frames_written: frames,
            underran,
        })
    }

    fn delay_frames(&mut self) -> Result<i64, SinkError> {
        if self.fault == DacFault::RefusesDelay {
            return Err(SinkError::Modelled(format!(
                "modelled device '{}' returned -EIO from snd_pcm_delay",
                self.device
            )));
        }
        self.tick();
        if self.fault == DacFault::ReportsZeroDelay {
            return Ok(0);
        }
        Ok(self.queued as i64)
    }

    fn in_xrun(&mut self) -> Result<bool, SinkError> {
        self.tick();
        let was = self.in_xrun;
        self.in_xrun = false;
        Ok(was)
    }

    fn drain(&mut self) -> Result<(), SinkError> {
        self.tick();
        self.played += self.queued;
        self.queued = 0.0;
        Ok(())
    }

    fn frames_played(&mut self) -> Result<u64, SinkError> {
        self.tick();
        Ok(self.played as u64)
    }
}

/// What a modelled endpoint is made of.
#[derive(Debug, Clone, Copy)]
pub struct ModelParams {
    /// Nominal stream rate.
    pub rate_hz: u32,
    /// Bytes per frame.
    pub frame_len: usize,
    /// Chunk duration on the server timeline, in nanoseconds.
    pub chunk_ns: u64,
    /// The server's crystal, in ppm off nominal.
    pub server_ppm: f64,
    /// The client's crystal, in ppm off nominal. It drives BOTH the client's
    /// monotonic clock and its DAC, which is the same simplification
    /// `crates/sync/src/sim.rs` makes and for the same reason: on one board
    /// they are usually the same oscillator, and modelling two would add a
    /// term nothing in this phase corrects.
    pub client_ppm: f64,
    /// Difference between the two clocks' epochs, in nanoseconds.
    pub epoch_offset_ns: i64,
    /// One-way network delay before jitter, in nanoseconds.
    pub base_one_way_ns: f64,
    /// How queuing delay above the base is distributed.
    pub jitter: JitterModel,
    /// How long the server holds a request before answering, in nanoseconds.
    pub turnaround_ns: f64,
    /// Seed for the jitter draws.
    pub seed: u64,
    /// The device delay the pacing holds, in nanoseconds.
    pub device_target_ns: u64,
    /// Audio to accumulate before the first write, in nanoseconds.
    pub start_fill_ns: u64,
    /// True time between model steps, in nanoseconds.
    pub step_ns: u64,
}

impl Default for ModelParams {
    fn default() -> ModelParams {
        ModelParams {
            rate_hz: 48_000,
            frame_len: 4,
            chunk_ns: 20_000_000,
            server_ppm: 0.0,
            client_ppm: 40.0,
            epoch_offset_ns: 12_345_000,
            // The quiet wired segment fixtures/sync/01-wired-quiet.cfg models:
            // 120 us of base delay and at most 60 us of switch queuing.
            base_one_way_ns: 120_000.0,
            jitter: JitterModel::Uniform { max_us: 60.0 },
            turnaround_ns: 50_000.0,
            seed: 0x5EED_5111,
            device_target_ns: 120_000_000,
            start_fill_ns: 120_000_000,
            step_ns: 5_000_000,
        }
    }
}

/// One sample of the modelled run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoopSample {
    /// True time since the run started, in nanoseconds.
    pub t_ns: u64,
    /// Ground truth: how far the content about to become audible is from where
    /// the server timeline says it should be, in nanoseconds. Positive means
    /// playout is ahead. No participant in the model can observe this.
    pub true_error_ns: f64,
    /// The error the loop itself formed, when it formed one on this step.
    pub loop_error_ns: Option<f64>,
    /// The device-reported delay, in nanoseconds, as the client reads it.
    pub delay_ns: f64,
}

/// What one modelled run produced.
#[derive(Debug, Clone)]
pub struct LoopResult {
    /// One sample per step.
    pub samples: Vec<LoopSample>,
    /// Exchanges the loop accepted.
    pub accepted: u64,
    /// Exchanges the loop threw away.
    pub discarded: u64,
    /// Hard resyncs, and the true time each happened at.
    pub hard_resyncs_at_ns: Vec<u64>,
    /// Ticks the loop reported its offset as stale.
    pub stale_ticks: u64,
    /// Ticks the correction was clamped.
    pub clamped_ticks: u64,
    /// Frames of silence inserted and frames of audio dropped.
    pub inserted_frames: u64,
    /// Frames of audio dropped.
    pub dropped_frames: u64,
    /// Underruns the modelled DAC signalled.
    pub underruns: u64,
    /// Times the servo has been run since the endpoint was built, which is
    /// once per tick that formed an error and never on a stale one.
    pub servo_updates: u32,
    /// The telemetry as it stood at the end.
    pub telemetry: chorus_client_linux::sync::Telemetry,
}

impl LoopResult {
    /// The largest absolute ground-truth error at or after `from_ns`.
    pub fn max_true_error_after(&self, from_ns: u64) -> f64 {
        self.samples
            .iter()
            .filter(|s| s.t_ns >= from_ns)
            .map(|s| s.true_error_ns.abs())
            .fold(0.0, f64::max)
    }

    /// The largest absolute loop-formed error at or after `from_ns`.
    pub fn max_loop_error_after(&self, from_ns: u64) -> f64 {
        self.samples
            .iter()
            .filter(|s| s.t_ns >= from_ns)
            .filter_map(|s| s.loop_error_ns)
            .map(f64::abs)
            .fold(0.0, f64::max)
    }

    /// Hard resyncs at or after `from_ns`.
    pub fn hard_resyncs_after(&self, from_ns: u64) -> usize {
        self.hard_resyncs_at_ns.iter().filter(|t| **t >= from_ns).count()
    }
}

/// The modelled endpoint: a server timeline, a network, a jitter buffer, a
/// DAC, and the client's REAL sync loop and corrector driving all of it.
///
/// The point of building it this way is that `SyncLoop` and `PlayoutCorrector`
/// here are the same types `crates/client-linux/src/run.rs` uses on the real
/// path. What is modelled is everything AROUND them.
pub struct ModelledEndpoint {
    params: ModelParams,
    sync: SyncConfig,
    clock: VirtualClock,
    dac: ModelledDac,
    loop_: SyncLoop,
    corrector: PlayoutCorrector,
    rng: Rng,
    /// True time, in nanoseconds.
    t_ns: u64,
    /// Chunks the client holds: presentation timestamp and PCM.
    queue: VecDeque<(u64, Vec<u8>)>,
    /// The next chunk the server will cut.
    next_sequence: u64,
    /// Presentation timestamp of the next frame to write.
    next_write_ts_ns: u64,
    started: bool,
    last_advance_ns: u64,
    next_tick_client_ns: u64,
    /// Set to make every exchange from now on nonsensical, for AC-13.
    pub corrupt_exchanges: bool,
    /// Set to make the server stop answering, for AC-15.
    pub server_silent: bool,
    /// Set to add this many nanoseconds to the client's playout position
    /// before the run starts, for the acquisition case.
    pub initial_misalignment_ns: i64,
}

impl ModelledEndpoint {
    /// A modelled endpoint at true time zero.
    pub fn new(params: ModelParams, sync: SyncConfig) -> ModelledEndpoint {
        let clock = VirtualClock::new();
        clock.set_ns(client_clock_ns(&params, 0.0));
        let dac = ModelledDac::new(
            "modelled-dac",
            params.rate_hz,
            params.frame_len,
            params.client_ppm,
            clock.clone(),
        );
        ModelledEndpoint {
            corrector: PlayoutCorrector::new(params.rate_hz, params.frame_len),
            loop_: SyncLoop::new(sync),
            rng: Rng::new(params.seed),
            params,
            sync,
            clock,
            dac,
            t_ns: 0,
            queue: VecDeque::new(),
            next_sequence: 0,
            next_write_ts_ns: 0,
            started: false,
            last_advance_ns: 0,
            next_tick_client_ns: 0,
            corrupt_exchanges: false,
            server_silent: false,
            initial_misalignment_ns: 0,
        }
    }

    /// Run for `duration_ns` of MODELLED time and report what happened.
    pub fn run(&mut self, duration_ns: u64) -> LoopResult {
        let mut samples = Vec::new();
        let mut hard_resyncs_at_ns = Vec::new();
        let mut stale_ticks = 0u64;
        let mut clamped_ticks = 0u64;
        let frames_per_chunk =
            (self.params.chunk_ns as f64 / 1e9 * f64::from(self.params.rate_hz)) as usize;

        while self.t_ns < duration_ns {
            self.t_ns += self.params.step_ns;
            self.clock
                .set_ns(client_clock_ns(&self.params, self.t_ns as f64));

            self.deliver_chunks(frames_per_chunk);

            let mut loop_error_ns = None;
            let client_now_ns = self.clock.now_ns();
            if self.started && client_now_ns >= self.next_tick_client_ns {
                self.next_tick_client_ns =
                    client_now_ns + self.sync.interval_ms.max(1) * 1_000_000;
                if !self.server_silent {
                    let exchange = self.exchange();
                    self.loop_.offer(&exchange);
                }
                let ts = self
                    .queue
                    .front()
                    .map(|(ts, _)| *ts)
                    .unwrap_or(self.next_write_ts_ns);
                let outcome =
                    self.loop_
                        .observe(&mut self.dac, client_now_ns, ts, self.params.rate_hz);
                match outcome {
                    Err(_) => break,
                    Ok(Correction::NoDeviceDelay) | Ok(Correction::NoOffset) => {}
                    Ok(Correction::Stale { .. }) => stale_ticks += 1,
                    Ok(Correction::Fine {
                        correction_ppm,
                        clamped,
                        error_ns,
                    }) => {
                        loop_error_ns = Some(error_ns);
                        clamped_ticks += u64::from(clamped);
                        self.advance_corrector(client_now_ns);
                        self.corrector.set_rate_correction(correction_ppm);
                    }
                    Ok(Correction::HardResync { step_ns, error_ns }) => {
                        loop_error_ns = Some(error_ns);
                        hard_resyncs_at_ns.push(self.t_ns);
                        self.advance_corrector(client_now_ns);
                        self.corrector.hard_resync(step_ns, self.sync.mute_ns);
                    }
                }
            }

            self.write_what_is_due();

            let delay_frames = self.dac.delay_frames().unwrap_or(0).max(0) as f64;
            samples.push(LoopSample {
                t_ns: self.t_ns,
                true_error_ns: self.true_error_ns(),
                loop_error_ns,
                delay_ns: delay_frames / f64::from(self.params.rate_hz) * 1e9,
            });
        }

        let telemetry = self.loop_.telemetry(self.clock.now_ns());
        LoopResult {
            samples,
            accepted: telemetry.accepted,
            discarded: telemetry.discarded,
            hard_resyncs_at_ns,
            stale_ticks,
            clamped_ticks,
            inserted_frames: self.corrector.inserted_frames(),
            dropped_frames: self.corrector.dropped_frames(),
            underruns: self.dac.underruns(),
            servo_updates: self.loop_.servo_updates(),
            telemetry,
        }
    }

    /// The DAC, for a test that wants to make it misbehave.
    pub fn dac(&mut self) -> &mut ModelledDac {
        &mut self.dac
    }

    /// The loop, for a test that wants to read its telemetry mid-run.
    pub fn sync_loop(&mut self) -> &mut SyncLoop {
        &mut self.loop_
    }

    /// The corrector, so a test can see what reached the device.
    pub fn corrector(&self) -> &PlayoutCorrector {
        &self.corrector
    }

    fn advance_corrector(&mut self, client_now_ns: u64) {
        self.corrector
            .advance(client_now_ns.saturating_sub(self.last_advance_ns));
        self.last_advance_ns = client_now_ns;
    }

    /// Everything the server has emitted by now, delivered.
    fn deliver_chunks(&mut self, frames_per_chunk: usize) {
        let server_now = server_clock_ns(&self.params, self.t_ns as f64);
        // The first chunk is stamped with what the server's clock read when
        // the stream started, exactly as `Chunker::new` does it. A timeline
        // whose origin is zero is the one case where an origin bug is
        // invisible.
        let origin_ns = server_clock_ns(&self.params, 0.0) as u64;
        loop {
            let ts = origin_ns + self.next_sequence * self.params.chunk_ns;
            if (ts as f64) > server_now {
                break;
            }
            let byte = (self.next_sequence % 251) as u8 | 1;
            self.queue
                .push_back((ts, vec![byte; frames_per_chunk * self.params.frame_len]));
            self.next_sequence += 1;
        }
    }

    /// The priming write, and then the paced ones.
    fn write_what_is_due(&mut self) {
        let rate = f64::from(self.params.rate_hz);
        if !self.started {
            let held: usize = self.queue.iter().map(|(_, pcm)| pcm.len()).sum();
            let held_ns = held as f64 / self.params.frame_len as f64 / rate * 1e9;
            if held_ns < self.params.start_fill_ns as f64 {
                return;
            }
            // One call, the whole fill, exactly as run.rs does it.
            let mut primed = Vec::new();
            let mut last = None;
            while (primed.len() as f64 / self.params.frame_len as f64 / rate * 1e9)
                < self.params.start_fill_ns as f64
            {
                match self.queue.pop_front() {
                    Some((ts, pcm)) => {
                        let frames = pcm.len() / self.params.frame_len;
                        primed.extend_from_slice(&pcm);
                        last = Some((ts, frames));
                    }
                    None => break,
                }
            }
            if let Some((ts, frames)) = last {
                self.next_write_ts_ns = ts + (frames as f64 / rate * 1e9) as u64;
            }
            // A deliberate misalignment, so a run can start outside the
            // hard-resync tier and be driven in.
            if self.initial_misalignment_ns != 0 {
                let frames = (self.initial_misalignment_ns.unsigned_abs() as f64 / 1e9 * rate) as usize;
                if self.initial_misalignment_ns > 0 {
                    primed.extend(std::iter::repeat(0u8).take(frames * self.params.frame_len));
                } else {
                    let cut = (frames * self.params.frame_len).min(primed.len());
                    primed.truncate(primed.len() - cut);
                }
            }
            let _ = self.dac.write(&primed);
            self.started = true;
            self.last_advance_ns = self.clock.now_ns();
            self.next_tick_client_ns = self.clock.now_ns();
            return;
        }

        let target_frames = self.params.device_target_ns as f64 / 1e9 * rate;
        loop {
            let delay = self.dac.delay_frames().unwrap_or(0).max(0) as f64;
            let next_frames = match self.queue.front() {
                Some((_, pcm)) => (pcm.len() / self.params.frame_len) as f64,
                None => break,
            };
            if delay + next_frames > target_frames {
                break;
            }
            let (ts, pcm) = self.queue.pop_front().expect("checked just above");
            let client_now_ns = self.clock.now_ns();
            self.advance_corrector(client_now_ns);
            let shaped = self.corrector.shape(&pcm);
            let _ = self.dac.write(&shaped);
            self.next_write_ts_ns = ts + (next_frames / rate * 1e9) as u64;
        }
    }

    /// One RFC 5905 exchange across the modelled network.
    fn exchange(&mut self) -> TimeSync {
        let forward_ns = self.params.base_one_way_ns + self.params.jitter.sample_ns(&mut self.rng);
        let return_ns = self.params.base_one_way_ns + self.params.jitter.sample_ns(&mut self.rng);
        let t0 = client_clock_ns(&self.params, self.t_ns as f64);
        let at_server = self.t_ns as f64 + forward_ns;
        let t1 = server_clock_ns(&self.params, at_server) as u64;
        let t2 = server_clock_ns(&self.params, at_server + self.params.turnaround_ns) as u64;
        let back = at_server + self.params.turnaround_ns + return_ns;
        let t3 = client_clock_ns(&self.params, back);
        if self.corrupt_exchanges {
            // The server says it held the request longer than the whole
            // exchange took, which is a round trip that would underflow.
            return TimeSync {
                t0_ns: t0,
                t1_ns: t1,
                t2_ns: t1 + (t3 - t0) * 4,
                t3_ns: t3,
            };
        }
        TimeSync {
            t0_ns: t0,
            t1_ns: t1,
            t2_ns: t2,
            t3_ns: t3,
        }
    }

    /// Ground truth. Nothing in the model can see this.
    fn true_error_ns(&self) -> f64 {
        let queued = self.dac.queued_frames();
        let audible_in_ns = queued / self.dac.true_rate_hz() * 1e9;
        let audible_at_true_ns = self.t_ns as f64 + audible_in_ns;
        let server_reads = server_clock_ns(&self.params, audible_at_true_ns);
        (self.next_write_ts_ns as f64 + self.sync.playout_latency_ns as f64) - server_reads
    }
}

/// The client's monotonic clock at true time `t_ns`. Its epoch is zero.
fn client_clock_ns(params: &ModelParams, t_ns: f64) -> u64 {
    (t_ns * (1.0 + params.client_ppm * 1e-6)) as u64
}

/// The server's monotonic clock at true time `t_ns`. Its epoch is
/// `epoch_offset_ns` away from the client's, because two monotonic clocks have
/// no shared epoch and finding the difference is the whole job.
fn server_clock_ns(params: &ModelParams, t_ns: f64) -> f64 {
    params.epoch_offset_ns as f64 + t_ns * (1.0 + params.server_ppm * 1e-6)
}
