//! A modelled TV on an optical or HDMI ARC capture, in modelled time.
//!
//! It is the line-in's modelled capture (`tests/line_in_source.rs`) extended
//! with what a TV input adds: its own sample clock, off nominal by a number
//! of ppm and drifting slowly, a stall (the receiver loses the TV and its
//! clock stops), IEC 61937 bursts in place of PCM, and a channel-status
//! non-audio bit. Time is a [`super::VirtualClock`]-style counter the reads
//! advance, so ten minutes of capture run in seconds and the client's
//! source loop (`source::run`) reads it as its monotonic clock.
//!
//! **What this is evidence for**: the rate matcher's and the refusals'
//! logic against a TV clock with a known error. **What it is not**: how a
//! real receiver's clock wanders, how late a real period wakeup is, or what
//! a real TV sends. The jitter and drift values are model inputs.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use chorus_client_linux::source::{CaptureError, CaptureRead, CaptureSource};
use chorus_client_linux::tvcapture::{ChannelStatus, IEC61937_PA, IEC61937_PB};
use chorus_protocol::SampleFormat;

/// Nominal rate of every modelled TV.
pub const TV_RATE_HZ: f64 = 48_000.0;

/// One stretch of the TV's clock: from `from_ns` (true time) it runs at
/// `ppm` off nominal plus `drift_ppm_per_s` per second since `from_ns`, or
/// not at all (`stopped`, a stall).
#[derive(Debug, Clone, Copy)]
pub struct ClockSegment {
    /// When it starts, true ns.
    pub from_ns: f64,
    /// Offset from nominal at its start, ppm.
    pub ppm: f64,
    /// Drift, ppm per second.
    pub drift_ppm_per_s: f64,
    /// The clock is stopped (no frames at all).
    pub stopped: bool,
}

impl ClockSegment {
    /// A running segment.
    pub fn at(from_s: f64, ppm: f64, drift_ppm_per_s: f64) -> ClockSegment {
        ClockSegment {
            from_ns: from_s * 1e9,
            ppm,
            drift_ppm_per_s,
            stopped: false,
        }
    }

    /// A stall from `from_s`.
    pub fn stall(from_s: f64) -> ClockSegment {
        ClockSegment {
            from_ns: from_s * 1e9,
            ppm: 0.0,
            drift_ppm_per_s: 0.0,
            stopped: true,
        }
    }
}

/// What the TV sends from a given frame on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Content {
    /// A tone of `hz` in TRUE time (as if the TV rendered it from an
    /// accurate reference), amplitude `amp`, the same on both channels.
    Tone { hz: f64, amp: f64 },
    /// A quadrature pair at `hz` cycles per TV frame period (left sine,
    /// right cosine of `2 pi i / period`), so any fractional TV frame
    /// position is read back off the output by its phase.
    Quadrature { period_frames: f64, amp: f64 },
    /// IEC 61937 data bursts: Pa, Pb, Pc (data type 1, AC-3), Pd, then a zero
    /// payload, repeating every 1536 frames (research `capture.md` 1.4 and
    /// 6.5's synthetic stream).
    Iec61937,
}

/// The TV's clock alone: when each of its frames was digitized, in true
/// time. Cloned out of a [`ModelledTv`] so a test can grade what the source
/// loop sends while the loop owns the capture.
#[derive(Debug, Clone)]
pub struct TvTimeline {
    segments: Vec<(ClockSegment, f64)>,
}

impl TvTimeline {
    /// Frames the TV has digitized by true time `t_ns` (fractional).
    pub fn frames_at(&self, t_ns: f64) -> f64 {
        let (seg, n0) = self
            .segments
            .iter()
            .rev()
            .find(|(s, _)| s.from_ns <= t_ns)
            .copied()
            .unwrap_or(self.segments[0]);
        n0 + frames_in(&seg, t_ns - seg.from_ns)
    }

    /// The true instant TV frame position `p` (fractional) was digitized.
    pub fn capture_time_ns(&self, p: f64) -> f64 {
        let (seg, n0) = self
            .segments
            .iter()
            .rev()
            .find(|(s, n0)| !s.stopped && *n0 <= p)
            .copied()
            .unwrap_or(self.segments[0]);
        // Solve n0 + frames_in(seg, dt) = p for dt by Newton's method.
        let mut dt = (p - n0) / TV_RATE_HZ * 1e9;
        for _ in 0..4 {
            let f = n0 + frames_in(&seg, dt) - p;
            let rate = TV_RATE_HZ * (1.0 + 1e-6 * (seg.ppm + seg.drift_ppm_per_s * dt / 1e9)) / 1e9;
            dt -= f / rate;
        }
        seg.from_ns + dt
    }
}

/// The model.
pub struct ModelledTv {
    format: SampleFormat,
    clock_ns: Arc<AtomicU64>,
    timeline: TvTimeline,
    contents: Vec<(u64, Content)>,
    non_audio: Vec<(f64, Option<bool>)>,
    pos: u64,
    delay: i64,
    jitter_ns: f64,
    rng: u64,
    /// Reads served.
    pub reads: u64,
}

impl ModelledTv {
    /// A TV whose clock follows `segments` (the first must start at 0) and
    /// whose content follows `contents` (frame index, content; the first at
    /// 0), captured as `format` stereo. Each period's wakeup is late by up to
    /// `jitter_ns`, uniformly (seed `seed`). The hub's clock is the shared
    /// counter, and it is the true time the segments are on (it starts at 0).
    pub fn new(
        format: SampleFormat,
        segments: &[ClockSegment],
        contents: &[(u64, Content)],
        jitter_ns: f64,
        seed: u64,
    ) -> ModelledTv {
        let mut with_n: Vec<(ClockSegment, f64)> = Vec::new();
        for (i, s) in segments.iter().enumerate() {
            let n = if i == 0 {
                0.0
            } else {
                let (prev, n0) = with_n[i - 1];
                n0 + frames_in(&prev, s.from_ns - prev.from_ns)
            };
            with_n.push((*s, n));
        }
        ModelledTv {
            format,
            clock_ns: Arc::new(AtomicU64::new(0)),
            timeline: TvTimeline { segments: with_n },
            contents: contents.to_vec(),
            non_audio: vec![(0.0, None)],
            pos: 0,
            delay: 0,
            jitter_ns,
            rng: seed.max(1),
            reads: 0,
        }
    }

    /// From `at_s` (true time) the receiver reports the non-audio bit as
    /// `bit` (`None`: not exposed).
    pub fn non_audio_from(&mut self, at_s: f64, bit: Option<bool>) {
        self.non_audio.push((at_s * 1e9, bit));
    }

    /// The hub's clock (ns), shared with the source loop.
    pub fn clock(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.clock_ns)
    }

    /// The TV's clock.
    pub fn timeline(&self) -> TvTimeline {
        self.timeline.clone()
    }

    fn content_at(&self, i: u64) -> Content {
        self.contents
            .iter()
            .rev()
            .find(|(from, _)| *from <= i)
            .map_or(self.contents[0].1, |(_, c)| *c)
    }

    fn sample(&self, i: u64) -> (f64, f64) {
        match self.content_at(i) {
            Content::Tone { hz, amp } => {
                let t = self.timeline.capture_time_ns(i as f64) / 1e9;
                let v = amp * (2.0 * std::f64::consts::PI * hz * t).sin();
                (v, v)
            }
            Content::Quadrature { period_frames, amp } => {
                let w = 2.0 * std::f64::consts::PI * (i as f64) / period_frames;
                (amp * w.sin(), amp * w.cos())
            }
            Content::Iec61937 => {
                let word = |w: u16| f64::from(w as i16) / 32_768.0;
                match i % 1536 {
                    0 => (word(IEC61937_PA), word(IEC61937_PB)),
                    1 => (word(0x0001), word(0x3800)),
                    _ => (0.0, 0.0),
                }
            }
        }
    }

    fn encode(&self, x: f64, out: &mut [u8]) {
        match self.format {
            SampleFormat::PcmS16Le => {
                let v = (x * 32_768.0).round().clamp(-32_768.0, 32_767.0) as i16;
                out.copy_from_slice(&v.to_le_bytes());
            }
            SampleFormat::PcmS24Le => {
                let v = (x * 8_388_608.0).round().clamp(-8_388_608.0, 8_388_607.0) as i32;
                out.copy_from_slice(&v.to_le_bytes()[..3]);
            }
            SampleFormat::PcmF32Le => out.copy_from_slice(&(x as f32).to_le_bytes()),
        }
    }

    fn next_jitter(&mut self) -> f64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 11) as f64 / (1u64 << 53) as f64 * self.jitter_ns
    }
}

/// Frames a running segment's clock delivers in `dt_ns` from its start.
fn frames_in(seg: &ClockSegment, dt_ns: f64) -> f64 {
    if seg.stopped {
        return 0.0;
    }
    let t = dt_ns / 1e9;
    TV_RATE_HZ * (t + 1e-6 * (seg.ppm * t + seg.drift_ppm_per_s * t * t / 2.0))
}

impl CaptureSource for ModelledTv {
    fn device(&self) -> &str {
        "modelled-tv"
    }

    fn frame_len(&self) -> usize {
        2 * self.format.bytes_per_sample()
    }

    fn read(&mut self, pcm: &mut [u8]) -> Result<CaptureRead, CaptureError> {
        match self.read_within(pcm, u64::MAX / 4)? {
            Some(r) => Ok(r),
            None => Err(CaptureError::Modelled("the modelled TV stalled".into())),
        }
    }

    fn read_within(
        &mut self,
        pcm: &mut [u8],
        wait_ns: u64,
    ) -> Result<Option<CaptureRead>, CaptureError> {
        let n = (pcm.len() / self.frame_len()) as u64;
        let end = self.pos + n;
        let done_at = self.timeline.capture_time_ns(end as f64);
        let now = self.clock_ns.load(Ordering::SeqCst) as f64;
        if done_at > now + wait_ns as f64 {
            self.clock_ns
                .store((now + wait_ns as f64) as u64, Ordering::SeqCst);
            return Ok(None);
        }
        let jitter = self.next_jitter();
        let back = done_at + jitter;
        // Frames digitized since the period ended and not read yet.
        self.delay = (jitter / 1e9 * TV_RATE_HZ).floor() as i64;
        let width = self.format.bytes_per_sample();
        for k in 0..n {
            let (l, r) = self.sample(self.pos + k);
            let at = (k as usize) * 2 * width;
            self.encode(l, &mut pcm[at..at + width]);
            self.encode(r, &mut pcm[at + width..at + 2 * width]);
        }
        self.pos = end;
        self.reads += 1;
        let back = back.max(now) as u64;
        self.clock_ns.store(back, Ordering::SeqCst);
        Ok(Some(CaptureRead {
            frames: n,
            overran: false,
        }))
    }

    fn delay_frames(&mut self) -> Result<Option<i64>, CaptureError> {
        Ok(Some(self.delay))
    }

    fn channel_status(&mut self) -> ChannelStatus {
        let now = self.clock_ns.load(Ordering::SeqCst) as f64;
        let bit = self
            .non_audio
            .iter()
            .rev()
            .find(|(at, _)| *at <= now)
            .and_then(|(_, b)| *b);
        ChannelStatus { non_audio: bit }
    }
}
