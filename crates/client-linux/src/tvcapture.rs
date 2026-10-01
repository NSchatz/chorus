//! The TV input's front end: what the hub does with an optical or HDMI ARC
//! capture before anything goes upstream (goal 13, the TV path).
//!
//! Pure and event driven: the source role (`crate::source`) feeds it each
//! captured period with the instant the read returned, tells it when a read
//! found no frames or failed, and takes stamped chunks out. No clock, device
//! or thread is touched here, so the tests drive it on modelled time.
//!
//! # What it decides
//!
//! 1. **Non-PCM refusal** ([`NonPcmDetector`]). A TV set to a "bitstream"
//!    or "auto" digital output sends AC-3 or DTS packed as IEC 61937 bursts
//!    in the PCM slots. Played as PCM that is full-scale noise. Each burst
//!    starts with the sync words Pa = 0xF872 and Pb = 0x4E1F in the 16 bits
//!    of the two subframes of one frame (IEC 61937-1:2021 section 6.1.7 and
//!    Table 3, the IEC's preview at
//!    <https://cdn.standards.iteh.ai/samples/101993/f9c9621694de4bdcbc934f16d281bf1d/IEC-61937-1-2021.pdf>,
//!    read 2026-10-01; the burst word sits in time slots 12 to 27, so in a
//!    24-bit sample it is the top 16 bits). The channel-status non-audio bit
//!    (bit 1, IEC 60958-3 byte 0) says the same where the receiver exposes it,
//!    but some encoders leave it clear (Cirrus CS8416 datasheet DS578F5
//!    section 10.2), so the sample scan is always on. A period with a sync
//!    word, or with the bit set, mutes: nothing captured is forwarded and the
//!    input is offered with `signal = false` (reason `non-pcm`). It unmutes
//!    after [`NON_PCM_CLEAN_HOLD_MS`] with neither.
//! 2. **Lock** (no frames for [`NO_FRAMES_MS`], or a read error): reason
//!    `no-lock`. A receiver that loses the TV either stops its clock (the
//!    read stalls) or free-runs muted, per the DIR9001 datasheet's Table 12
//!    (TI SLES198A, read 2026-10-01); the stall is what this catches.
//! 3. **Rate range**: the ratio loop held at its clamp
//!    (`crate::ratematch::MAX_RATIO_PPM`) for [`CLAMP_HOLD_MS`] means a
//!    source outside what chorus follows (a wrong nominal rate, or a broken
//!    clock): reason `rate-out-of-range`. While refused, the matcher measures
//!    again from scratch; a fresh estimate inside the clamp lifts it.
//!
//! Everything else is `crate::ratematch`: the rate-matched output, stamped
//! on the server timeline at exactly the nominal rate, cut into chunks of
//! [`TvFrontEnd::chunk_frames`] frames.

use std::collections::VecDeque;

use chorus_protocol::SampleFormat;

use crate::ratematch::{RateMatcher, Relock, MAX_RATIO_PPM};

/// IEC 61937 sync word 1 (Pa), in subframe 1 (left) of a burst's first
/// frame (IEC 61937-1:2021 Table 3).
pub const IEC61937_PA: u16 = 0xF872;

/// IEC 61937 sync word 2 (Pb), in subframe 2 (right) of the same frame.
pub const IEC61937_PB: u16 = 0x4E1F;

/// How long a muted input must stay clean (no sync word, no non-audio bit)
/// before it is forwarded again, ms. About eight AC-3 repetition periods
/// (1536 frames, 32 ms at 48 kHz); other data types' periods were not read,
/// so ASSUMED (research `capture.md` section 6.5).
pub const NON_PCM_CLEAN_HOLD_MS: u32 = 250;

/// No frames for this long is a lost lock, ms (the envelope's 100 ms; the
/// DIR9001's PLL lock-up time is 100 ms typical). ASSUMED.
pub const NO_FRAMES_MS: u64 = 100;

/// The ratio loop at its clamp this long is a source out of range, ms
/// (research `capture.md` section 6.3). ASSUMED.
pub const CLAMP_HOLD_MS: u64 = 2_000;

/// Why a TV input is not forwarded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The TV sends an encoded bitstream (IEC 61937), not PCM.
    NonPcm,
    /// No frames, or the device failed: the receiver has no lock.
    NoLock,
    /// The source's rate is outside the ratio clamp.
    RateOutOfRange,
}

impl Refusal {
    /// The `reason=` word.
    pub fn name(self) -> &'static str {
        match self {
            Refusal::NonPcm => "non-pcm",
            Refusal::NoLock => "no-lock",
            Refusal::RateOutOfRange => "rate-out-of-range",
        }
    }

    /// What the owner should do, for the log line.
    pub fn advice(self) -> &'static str {
        match self {
            Refusal::NonPcm => {
                "the TV is sending a compressed format (for example AC-3): set the TV's digital \
                 audio output to PCM"
            }
            Refusal::NoLock => "no signal from the TV: it is off, unplugged or not sending",
            Refusal::RateOutOfRange => {
                "the TV's sample rate is not the configured one, or its clock is out of range"
            }
        }
    }
}

/// What the capture source says about the stream besides its samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChannelStatus {
    /// Channel-status bit 1 (non-audio), when the receiver exposes it:
    /// `None` when it does not (an ALSA capture on this client today).
    pub non_audio: Option<bool>,
}

/// The top 16 bits of one sample, where an IEC 61937 burst word sits.
pub fn top16(format: SampleFormat, bytes: &[u8]) -> u16 {
    match format {
        SampleFormat::PcmS16Le => u16::from_le_bytes([bytes[0], bytes[1]]),
        SampleFormat::PcmS24Le => u16::from_le_bytes([bytes[1], bytes[2]]),
        SampleFormat::PcmF32Le => {
            let v = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            if v.is_finite() {
                ((f64::from(v) * 32_768.0).round().clamp(-32_768.0, 32_767.0) as i16) as u16
            } else {
                0
            }
        }
    }
}

/// The IEC 61937 scan and the mute it decides.
#[derive(Debug, Clone)]
pub struct NonPcmDetector {
    format: SampleFormat,
    channels: usize,
    hold_frames: u64,
    clean_frames: u64,
    muted: bool,
    /// Sync words found.
    pub bursts: u64,
    /// The last sample of the previous push (a mono capture pairs samples
    /// across a push boundary).
    carry: Option<u16>,
}

impl NonPcmDetector {
    /// A detector for this shape, starting unmuted.
    pub fn new(rate_hz: u32, channels: u16, format: SampleFormat) -> NonPcmDetector {
        NonPcmDetector {
            format,
            channels: usize::from(channels.max(1)),
            hold_frames: u64::from(rate_hz) * u64::from(NON_PCM_CLEAN_HOLD_MS) / 1_000,
            clean_frames: 0,
            muted: false,
            bursts: 0,
            carry: None,
        }
    }

    /// Whether the input is muted now.
    pub fn muted(&self) -> bool {
        self.muted
    }

    /// Scan one period; whether it is muted (the period with the sync word
    /// is itself muted).
    pub fn push(&mut self, pcm: &[u8], status: ChannelStatus) -> bool {
        let width = self.format.bytes_per_sample();
        let frame_len = width * self.channels;
        let frames = (pcm.len() / frame_len) as u64;
        let mut found = status.non_audio == Some(true);
        if self.channels >= 2 {
            for frame in pcm.chunks_exact(frame_len) {
                if top16(self.format, &frame[..width]) == IEC61937_PA
                    && top16(self.format, &frame[width..2 * width]) == IEC61937_PB
                {
                    self.bursts += 1;
                    found = true;
                }
            }
        } else {
            // A mono capture of a stereo link sees the two subframes as
            // consecutive samples.
            for sample in pcm.chunks_exact(width) {
                let w = top16(self.format, sample);
                if self.carry == Some(IEC61937_PA) && w == IEC61937_PB {
                    self.bursts += 1;
                    found = true;
                }
                self.carry = Some(w);
            }
        }
        if found {
            self.muted = true;
            self.clean_frames = 0;
        } else if self.muted {
            self.clean_frames += frames;
            if self.clean_frames >= self.hold_frames {
                self.muted = false;
            }
        }
        self.muted
    }
}

/// One sample as a fraction of full scale.
pub fn decode_sample(format: SampleFormat, bytes: &[u8]) -> f64 {
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

/// One sample at full scale 1.0 appended in `format`, rounded and clamped.
pub fn encode_sample(format: SampleFormat, x: f64, out: &mut Vec<u8>) {
    match format {
        SampleFormat::PcmS16Le => {
            let v = (x * 32_768.0).round().clamp(-32_768.0, 32_767.0) as i16;
            out.extend_from_slice(&v.to_le_bytes());
        }
        SampleFormat::PcmS24Le => {
            let v = (x * 8_388_608.0).round().clamp(-8_388_608.0, 8_388_607.0) as i32;
            out.extend_from_slice(&v.to_le_bytes()[..3]);
        }
        SampleFormat::PcmF32Le => out.extend_from_slice(&(x as f32).to_le_bytes()),
    }
}

/// The shape a TV input is captured and sent in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TvShape {
    /// Nominal rate, Hz.
    pub rate_hz: u32,
    /// Channels.
    pub channels: u16,
    /// Sample layout (captured and sent alike).
    pub format: SampleFormat,
    /// Frames per captured period.
    pub period_frames: u32,
    /// Frames per upstream chunk.
    pub chunk_frames: u32,
}

/// What the front end has counted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TvCounters {
    /// Periods fed.
    pub periods: u64,
    /// Periods muted as non-PCM.
    pub non_pcm_periods: u64,
    /// IEC 61937 sync words found.
    pub bursts: u64,
    /// Times each refusal began: non-pcm, no-lock, rate-out-of-range.
    pub refusals: [u64; 3],
    /// Relocks of the matcher, any reason.
    pub relocks: u64,
    /// Relocks because the ring overflowed.
    pub ring_overflows: u64,
    /// Relocks because the ring underflowed.
    pub ring_underflows: u64,
    /// Rate-matched output frames dropped (refused, no offset, or a partial
    /// chunk cut by a relock).
    pub frames_dropped: u64,
    /// Chunks ready to send.
    pub chunks: u64,
}

/// The front end: refusals, rate matching, chunking.
#[derive(Debug, Clone)]
pub struct TvFrontEnd {
    shape: TvShape,
    nonpcm: NonPcmDetector,
    matcher: RateMatcher,
    scratch: Vec<f64>,
    no_lock: bool,
    out_of_range: bool,
    last_frames_ns: Option<u64>,
    clamp_since_ns: Option<u64>,
    refusal: Option<Refusal>,
    pending: Vec<f64>,
    pending_first: u64,
    pending_lock: u64,
    ready: VecDeque<(u64, Vec<u8>)>,
    counters: TvCounters,
    last_relock: Option<Relock>,
}

impl TvFrontEnd {
    /// A front end for this shape.
    pub fn new(shape: TvShape) -> TvFrontEnd {
        let channels = usize::from(shape.channels.max(1));
        TvFrontEnd {
            shape,
            nonpcm: NonPcmDetector::new(shape.rate_hz, shape.channels, shape.format),
            matcher: RateMatcher::new(shape.channels, shape.rate_hz, shape.period_frames),
            scratch: Vec::with_capacity(shape.period_frames as usize * channels),
            no_lock: false,
            out_of_range: false,
            last_frames_ns: None,
            clamp_since_ns: None,
            refusal: None,
            pending: Vec::with_capacity(shape.chunk_frames as usize * channels),
            pending_first: 0,
            pending_lock: 0,
            ready: VecDeque::new(),
            counters: TvCounters::default(),
            last_relock: None,
        }
    }

    /// Why the input is refused now, if it is.
    pub fn refusal(&self) -> Option<Refusal> {
        self.refusal
    }

    /// What it has counted.
    pub fn counters(&self) -> TvCounters {
        let mut c = self.counters;
        c.bursts = self.nonpcm.bursts;
        c
    }

    /// The matcher, for reports and tests.
    pub fn matcher(&self) -> &RateMatcher {
        &self.matcher
    }

    /// The last relock's reason, taken (for the log).
    pub fn take_relock(&mut self) -> Option<Relock> {
        self.last_relock.take()
    }

    /// Frames per upstream chunk.
    pub fn chunk_frames(&self) -> u32 {
        self.shape.chunk_frames
    }

    /// Change the upstream chunk size from the next chunk (the seam for the
    /// low-latency path: 120 frames, 2.5 ms, when the server offers
    /// direction 2). A partial chunk in hand is dropped and counted.
    pub fn set_chunk_frames(&mut self, frames: u32) {
        self.shape.chunk_frames = frames.max(1);
        self.drop_pending();
    }

    /// Drop every chunk not yet taken and the partial one (a stream ended or
    /// started: the next chunk begins at the next output frame).
    pub fn clear_output(&mut self) {
        self.drop_pending();
        self.ready.clear();
    }

    /// The next stamped chunk: its server-timeline stamp and its bytes.
    pub fn pop_chunk(&mut self) -> Option<(u64, Vec<u8>)> {
        self.ready.pop_front()
    }

    /// One captured period (exactly `period_frames`), whose read returned
    /// at `now_ns` on the hub's monotonic clock with `delay_frames` captured
    /// and unread, `offset_ns` the published sync offset (server = hub +
    /// offset; `None` before the first). Returns the refusal if it changed.
    pub fn period(
        &mut self,
        pcm: &[u8],
        now_ns: u64,
        delay_frames: i64,
        offset_ns: Option<i64>,
        status: ChannelStatus,
    ) -> Option<Option<Refusal>> {
        self.counters.periods += 1;
        self.last_frames_ns = Some(now_ns);
        let muted = self.nonpcm.push(pcm, status);
        if muted {
            self.counters.non_pcm_periods += 1;
        }
        let width = self.shape.format.bytes_per_sample();
        self.scratch.clear();
        self.scratch.extend(
            pcm.chunks_exact(width)
                .map(|s| decode_sample(self.shape.format, s)),
        );
        let read_at = i128::from(now_ns) + i128::from(offset_ns.unwrap_or(0));
        let produced = self
            .matcher
            .push_period(&self.scratch, read_at, delay_frames);
        if let Some(why) = produced.relocked {
            self.counters.relocks += 1;
            match why {
                Relock::RingOverflow => self.counters.ring_overflows += 1,
                Relock::RingUnderflow => self.counters.ring_underflows += 1,
                Relock::TimingJump => {}
            }
            self.last_relock = Some(why);
            self.clamp_since_ns = None;
        }

        // Lock and range.
        if self.matcher.running() {
            if self.out_of_range {
                // A fresh measurement after the refusal reset the matcher.
                let inside = self
                    .matcher
                    .estimate_ppm()
                    .is_some_and(|p| p.abs() < MAX_RATIO_PPM);
                if inside {
                    self.out_of_range = false;
                } else {
                    self.matcher.reset();
                }
            } else if self.matcher.clamped() {
                let since = *self.clamp_since_ns.get_or_insert(now_ns);
                if now_ns.saturating_sub(since) >= CLAMP_HOLD_MS * 1_000_000 {
                    self.out_of_range = true;
                    self.clamp_since_ns = None;
                    self.matcher.reset();
                }
            } else {
                self.clamp_since_ns = None;
            }
            if self.matcher.running() {
                self.no_lock = false;
            }
        }
        let changed = self.update_refusal();

        // Output.
        let forward = self.refusal.is_none() && offset_ns.is_some() && self.matcher.running();
        let frames = produced.frames;
        if frames > 0 {
            if forward {
                self.take_output(produced.first_index, produced.lock);
            } else {
                self.counters.frames_dropped += frames as u64;
                self.drop_pending();
            }
        } else if !forward {
            self.drop_pending();
        }
        changed
    }

    /// A read found no frames; `now_ns` on the hub's clock. Returns the
    /// refusal if it changed.
    pub fn no_frames(&mut self, now_ns: u64) -> Option<Option<Refusal>> {
        let last = *self.last_frames_ns.get_or_insert(now_ns);
        if now_ns.saturating_sub(last) >= NO_FRAMES_MS * 1_000_000 && !self.no_lock {
            self.lose_lock();
        }
        self.update_refusal()
    }

    /// A read failed. Returns the refusal if it changed.
    pub fn read_failed(&mut self) -> Option<Option<Refusal>> {
        if !self.no_lock {
            self.lose_lock();
        }
        self.update_refusal()
    }

    /// The device lost frames (an overrun): the timeline is broken, so the
    /// matcher locks again from the next period.
    pub fn overran(&mut self) {
        self.matcher.reset();
        self.drop_pending();
        self.counters.relocks += 1;
    }

    fn lose_lock(&mut self) {
        self.no_lock = true;
        self.matcher.reset();
        self.drop_pending();
        self.clamp_since_ns = None;
    }

    fn update_refusal(&mut self) -> Option<Option<Refusal>> {
        let now = if self.no_lock {
            Some(Refusal::NoLock)
        } else if self.out_of_range {
            Some(Refusal::RateOutOfRange)
        } else if self.nonpcm.muted() {
            Some(Refusal::NonPcm)
        } else {
            None
        };
        if now == self.refusal {
            return None;
        }
        if let Some(r) = now {
            let i = match r {
                Refusal::NonPcm => 0,
                Refusal::NoLock => 1,
                Refusal::RateOutOfRange => 2,
            };
            self.counters.refusals[i] += 1;
        }
        self.refusal = now;
        Some(now)
    }

    fn drop_pending(&mut self) {
        let channels = usize::from(self.shape.channels.max(1));
        self.counters.frames_dropped += (self.pending.len() / channels) as u64;
        self.pending.clear();
    }

    fn take_output(&mut self, first_index: u64, lock: u64) {
        let channels = usize::from(self.shape.channels.max(1));
        let held = (self.pending.len() / channels) as u64;
        if held > 0 && (lock != self.pending_lock || first_index != self.pending_first + held) {
            self.drop_pending();
        }
        if self.pending.is_empty() {
            self.pending_first = first_index;
            self.pending_lock = lock;
        }
        let chunk = self.shape.chunk_frames as usize * channels;
        let mut at = 0usize;
        let out = self.matcher.output();
        while at < out.len() {
            let take = (chunk - self.pending.len()).min(out.len() - at);
            self.pending.extend_from_slice(&out[at..at + take]);
            at += take;
            if self.pending.len() == chunk {
                let stamp = self.matcher.stamp_ns(self.pending_first);
                let width = self.shape.format.bytes_per_sample();
                let mut bytes = Vec::with_capacity(chunk * width);
                for x in &self.pending {
                    encode_sample(self.shape.format, *x, &mut bytes);
                }
                self.ready.push_back((stamp, bytes));
                self.counters.chunks += 1;
                self.pending.clear();
                self.pending_first += u64::from(self.shape.chunk_frames);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame16(l: u16, r: u16) -> Vec<u8> {
        let mut v = l.to_le_bytes().to_vec();
        v.extend_from_slice(&r.to_le_bytes());
        v
    }

    #[test]
    fn a_sync_word_pair_mutes_at_once_and_a_clean_hold_unmutes() {
        let mut d = NonPcmDetector::new(48_000, 2, SampleFormat::PcmS16Le);
        let clean = vec![0u8; 240 * 4];
        assert!(!d.push(&clean, ChannelStatus::default()));
        let mut burst = vec![0u8; 239 * 4];
        burst.extend(frame16(IEC61937_PA, IEC61937_PB));
        assert!(d.push(&burst, ChannelStatus::default()));
        // 245 ms clean: still muted; 250 ms: unmuted.
        for _ in 0..49 {
            assert!(d.push(&clean, ChannelStatus::default()));
        }
        assert!(!d.push(&clean, ChannelStatus::default()));
        assert_eq!(d.bursts, 1);
    }

    #[test]
    fn pa_in_the_left_channel_alone_is_pcm_and_the_non_audio_bit_mutes() {
        let mut d = NonPcmDetector::new(48_000, 2, SampleFormat::PcmS16Le);
        let mut pcm = frame16(IEC61937_PA, 0x1234);
        pcm.extend(frame16(0x0000, IEC61937_PB));
        assert!(!d.push(&pcm, ChannelStatus::default()));
        assert!(d.push(
            &pcm,
            ChannelStatus {
                non_audio: Some(true)
            }
        ));
    }

    #[test]
    fn the_scan_reads_the_top_sixteen_bits_of_every_layout() {
        let mut s24 = vec![0x55, 0x72, 0xF8];
        assert_eq!(top16(SampleFormat::PcmS24Le, &s24), IEC61937_PA);
        s24 = vec![0x00, 0x1F, 0x4E];
        assert_eq!(top16(SampleFormat::PcmS24Le, &s24), IEC61937_PB);
        let f = (f32::from(IEC61937_PB as i16) / 32_768.0).to_le_bytes();
        assert_eq!(top16(SampleFormat::PcmF32Le, &f), IEC61937_PB);
        let f = (f32::from(IEC61937_PA as i16) / 32_768.0).to_le_bytes();
        assert_eq!(top16(SampleFormat::PcmF32Le, &f), IEC61937_PA);
    }

    fn shape() -> TvShape {
        TvShape {
            rate_hz: 48_000,
            channels: 2,
            format: SampleFormat::PcmS16Le,
            period_frames: 240,
            chunk_frames: 960,
        }
    }

    #[test]
    fn a_read_error_is_no_lock_until_the_matcher_runs_again() {
        let mut tv = TvFrontEnd::new(shape());
        let pcm = vec![0u8; 240 * 4];
        let period_ns = 5_000_000u64;
        let mut now = 0u64;
        for _ in 0..200 {
            now += period_ns;
            assert_eq!(
                tv.period(&pcm, now, 0, Some(0), ChannelStatus::default()),
                None
            );
        }
        assert!(tv.matcher().running());
        assert_eq!(tv.read_failed(), Some(Some(Refusal::NoLock)));
        assert_eq!(tv.read_failed(), None, "one refusal, not one per error");
        assert!(
            !tv.matcher().running(),
            "the timeline is broken: lock again"
        );
        let mut lifted = None;
        for k in 0..200u32 {
            now += period_ns;
            if let Some(change) = tv.period(&pcm, now, 0, Some(0), ChannelStatus::default()) {
                lifted = Some((k, change));
                break;
            }
        }
        // The DLL warms for WARMUP_PERIODS (the first period starts it).
        assert_eq!(lifted, Some((crate::ratematch::WARMUP_PERIODS - 1, None)));
        assert_eq!(tv.counters().refusals, [0, 1, 0]);
    }

    #[test]
    fn no_frames_for_100_ms_is_no_lock() {
        let mut tv = TvFrontEnd::new(shape());
        let pcm = vec![0u8; 240 * 4];
        tv.period(&pcm, 1_000_000, 0, Some(0), ChannelStatus::default());
        assert_eq!(tv.no_frames(80_000_000), None);
        assert_eq!(tv.no_frames(101_000_000), Some(Some(Refusal::NoLock)));
    }

    #[test]
    fn samples_round_trip_through_every_layout() {
        for format in [
            SampleFormat::PcmS16Le,
            SampleFormat::PcmS24Le,
            SampleFormat::PcmF32Le,
        ] {
            let mut bytes = Vec::new();
            encode_sample(format, -0.25, &mut bytes);
            assert_eq!(decode_sample(format, &bytes), -0.25);
        }
    }
}
