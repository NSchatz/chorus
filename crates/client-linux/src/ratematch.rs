//! Rate matching for a TV input: a source on the TV's sample clock converted
//! to exactly the nominal rate on the chorus timeline (goal 13, the TV path).
//!
//! # The problem
//!
//! An optical (S/PDIF) or HDMI ARC input is clocked by the TV: the receiver
//! recovers its bit clock from the stream, so the hub captures at the TV's
//! rate, not its own. IEC 60958-3 allows a consumer source's rate to be off
//! by up to +/-1000 ppm (Level II, section 7.2.1 of IS/IEC 60958-3:2003,
//! <https://law.resource.org/pub/in/bis/S04/is.iec.60958.3.2003.pdf>, read
//! 2026-10-01). Played as captured, a stream 300 ppm fast gains 18 ms a
//! minute against the timeline every room plays on: lip sync is lost within
//! minutes and a buffer somewhere eventually over- or underflows.
//!
//! # What this module does
//!
//! Three pure parts, no clock, no device, no thread:
//!
//! 1. [`PeriodDll`]: the second-order delay-locked loop of F. Adriaensen,
//!    "Using a DLL to filter time" (LAC 2005,
//!    <https://kokkinizita.linuxaudio.org/papers/usingdll.pdf>, read
//!    2026-10-01), fed one timestamp per captured period. It filters the
//!    scheduling jitter of the period wakeups out of the timestamps and
//!    estimates the TV's true frame period on the server timeline.
//! 2. [`RatioController`]: the resampling-ratio loop of F. Adriaensen,
//!    "Controlling adaptive resampling" (LAC 2012,
//!    <https://kokkinizita.linuxaudio.org/papers/adapt-resamp.pdf>, read
//!    2026-10-01): the ratio is driven by a delay error that includes the
//!    resampler's fractional position (section 3.2 of the paper: without it
//!    the error is a sawtooth the loop cannot remove), low-passed at twenty
//!    times the loop bandwidth, through a second-order loop at 0.05 Hz, with
//!    a higher bandwidth for the first four seconds (section 3.4).
//! 3. [`RateMatcher`]: the two joined to `chorus_sync`'s Catmull-Rom
//!    resampler ([`CubicResampler`] rendering [`ChunkPlan::constant_rate`]).
//!    Output frame `n` is stamped `S0 + n / rate` on the server timeline: the
//!    stream's sample clock is chorus's, and the loop keeps the capture
//!    instant of the source position output frame `n` plays equal to that
//!    stamp.
//!
//! # The error, exactly
//!
//! After period `k` the DLL gives `t0`, the filtered instant frame `W` (the
//! count captured so far) was digitized, and `Te`, the filtered frame period.
//! The source position whose capture instant equals the stamp of the next
//! output frame, `S0 + O / rate`, is
//!
//! ```text
//! x* = W + (S0 + O / rate - t0) / Te
//! ```
//!
//! and the resampler is at `R` (fractional) for that frame. The loop's error
//! is `e = x* - R`, in source frames: Adriaensen's `W - R + d_res - Delta`
//! with the target delay `Delta` folded into the stamp (the output runs
//! [`SEND_DELAY_NS`] behind the newest capture, so the frames it reads are
//! always there). `e` is also the stamp error in frames: `e * Te` is how far
//! the stamp of output frame `O` is from the capture instant of what it
//! plays.
//!
//! # The loop bound, stated
//!
//! The controller is a type-2 loop (it integrates the error twice), so a
//! source at a constant rate inside the clamp leaves no steady-state error,
//! and a rate that drifts at `a` frames/s^2 leaves `a / wn^2` frames with
//! `wn = 2 pi B` (Adriaensen 2005, section 3: critically damped,
//! `b = sqrt(2) w`, `c = w^2`). At `B = 0.05 Hz` (`wn^2 = 0.0987 /s^2`) a
//! drift of 1 ppm per minute at 48 kHz (`a = 8e-4` frames/s^2) leaves
//! 0.008 frames. The start-up transient is the larger term: an initial
//! ratio error `dr` (the DLL's estimate after [`WARMUP_PERIODS`]) peaks at
//! about `0.46 dr rate / wn` frames at the start-up bandwidth (the peak of a
//! critically damped step response in velocity), under one frame for
//! `dr < 100 ppm`. The tests hold the settled stamp error to
//! [`SETTLED_BOUND_FRAMES`]. All of this is arithmetic on the model, not
//! timing evidence (BRIEF.md section 3.1 rule 3).

use std::f64::consts::{PI, SQRT_2};

use chorus_sync::latency_grow::{ChunkPlan, CubicResampler};

/// Frames per captured period: 5 ms at 48 kHz, BRIEF.md 5.7's 5 ms capture
/// chunk (research `capture.md` section 6.1). ASSUMED until a bench report
/// shows the hub keeps up with it.
pub const PERIOD_FRAMES: u32 = 240;

/// The DLL's bandwidth, Hz. Research `capture.md` section 6.2's proposal
/// (Adriaensen 2005 gives no single value); at 200 periods/s it is
/// `w = 0.0314`, `b = 0.0444`, `c = 0.000987`. ASSUMED.
pub const DLL_BANDWIDTH_HZ: f64 = 1.0;

/// The ratio loop's normal bandwidth, Hz: Adriaensen 2012 section 3.3,
/// "around 0.05 Hz".
pub const LOOP_BANDWIDTH_HZ: f64 = 0.05;

/// The ratio loop's start-up bandwidth, Hz, for the first
/// [`STARTUP_SECONDS`]: Adriaensen 2012 section 3.4 runs "at higher
/// bandwidth for the first 4 seconds" without a number; 0.5 Hz is research
/// `capture.md` section 6.3's proposal. ASSUMED.
pub const STARTUP_BANDWIDTH_HZ: f64 = 0.5;

/// How long the start-up bandwidth runs, s (Adriaensen 2012 section 3.4).
pub const STARTUP_SECONDS: f64 = 4.0;

/// The error low-pass's corner as a multiple of the loop bandwidth
/// (Adriaensen 2012 section 3.3: "20 times the loop bandwidth").
pub const LOWPASS_FACTOR: f64 = 20.0;

/// The ratio clamp, ppm: IEC 60958-3 Level II's +/-1000 ppm plus margin, the
/// DIR9001's accept window ("+/-1500 ppm", TI SLES198A,
/// <https://www.ti.com/lit/ds/symlink/dir9001.pdf>, read 2026-10-01). A source
/// the DLL measures outside it is refused (`crate::tvcapture`).
pub const MAX_RATIO_PPM: f64 = 1_500.0;

/// How far behind the newest captured frame the output runs, ns: the
/// interpolator reads two frames ahead and the loop's settled error is a
/// fraction of a frame, so 2 ms (96 frames at 48 kHz) leaves room for the
/// start-up transient. Adds to the send latency, never to the stamps.
/// ASSUMED.
pub const SEND_DELAY_NS: f64 = 2_000_000.0;

/// Periods the DLL runs before the output starts: 0.5 s at 200 periods/s,
/// about three time constants at 1 Hz, so the ratio the loop starts from is
/// the DLL's settled estimate. The capture is read whether or not the input
/// is played, so this is paid once per lock, not per start. ASSUMED.
pub const WARMUP_PERIODS: u32 = 100;

/// A DLL error this large (ns) is a broken timeline, not jitter: a stall the
/// read did not report, or a step in the server offset. The matcher locks
/// again from the next period. Two periods at 48 kHz. ASSUMED.
pub const RELOCK_ERROR_NS: f64 = 10_000_000.0;

/// Source frames the matcher holds at most between periods: 50 ms at
/// 48 kHz, ten periods, far above the [`SEND_DELAY_NS`] plus one period it
/// holds when locked. A push past it is an overflow and a relock.
pub const RING_FRAMES: usize = 2_400;

/// The settled stamp error the tests hold the loop to, in frames (see the
/// module documentation: well under one frame once settled).
pub const SETTLED_BOUND_FRAMES: f64 = 1.0;

/// The DLL of Adriaensen 2005, one update per period of a fixed frame count.
///
/// Times are `f64` ns relative to whatever origin the caller chose (the
/// matcher's lock instant), so their precision stays far below a nanosecond
/// for days.
#[derive(Debug, Clone)]
pub struct PeriodDll {
    period_frames: f64,
    b: f64,
    c: f64,
    /// Filtered time of the current period's end.
    t0: f64,
    /// Predicted time of the next period's end.
    t1: f64,
    /// Filtered period, ns.
    e2: f64,
    nominal_period_ns: f64,
}

impl PeriodDll {
    /// A loop for periods of `period_frames` at nominal `rate_hz`, with loop
    /// bandwidth `bandwidth_hz`, whose first period ended at `t_ns`.
    pub fn new(period_frames: u32, rate_hz: u32, bandwidth_hz: f64, t_ns: f64) -> PeriodDll {
        let period_frames = f64::from(period_frames.max(1));
        let rate = f64::from(rate_hz.max(1));
        let nominal_period_ns = period_frames * 1e9 / rate;
        // Adriaensen 2005 eqs. 9-12: w = 2 pi B / F, b = sqrt(2) w, c = w^2,
        // F the period rate.
        let w = 2.0 * PI * bandwidth_hz * nominal_period_ns / 1e9;
        PeriodDll {
            period_frames,
            b: SQRT_2 * w,
            c: w * w,
            t0: t_ns,
            t1: t_ns + nominal_period_ns,
            e2: nominal_period_ns,
            nominal_period_ns,
        }
    }

    /// One period ended, measured at `t_ns`: the loop error (measured minus
    /// predicted, ns), after the update.
    pub fn update(&mut self, t_ns: f64) -> f64 {
        // The paper's update, in its order: e = t - t1; t0 = t1;
        // t1 += b e + e2; e2 += c e.
        let e = t_ns - self.t1;
        self.t0 = self.t1;
        self.t1 += self.b * e + self.e2;
        self.e2 += self.c * e;
        e
    }

    /// The filtered instant the current period ended.
    pub fn time(&self) -> f64 {
        self.t0
    }

    /// The filtered frame period, ns: the loop's period state `e2` over the
    /// period's frames. The paper's `(t1 - t0) / P` also carries the
    /// proportional term `b e`, which passes the wakeup jitter through at the
    /// loop bandwidth (0.044 of a 400 us jitter is 3500 ppm of a 5 ms
    /// period); `e2` is that same estimate with the jitter integrated out.
    pub fn frame_ns(&self) -> f64 {
        self.e2 / self.period_frames
    }

    /// The source's rate against nominal, as source frames per nominal
    /// frame (above 1: the source runs fast).
    pub fn ratio(&self) -> f64 {
        self.nominal_period_ns / self.e2
    }

    /// [`PeriodDll::ratio`] as ppm.
    pub fn ppm(&self) -> f64 {
        (self.ratio() - 1.0) * 1e6
    }
}

/// The ratio loop of Adriaensen 2012: the error through a second-order
/// low-pass at [`LOWPASS_FACTOR`] times the loop bandwidth, then a
/// second-order loop with the same coefficients as the 2005 DLL, its output
/// a correction to the resampling ratio, clamped to [`MAX_RATIO_PPM`].
///
/// Derivation of the scaling: with `N` output frames per update and a ratio
/// `r` (source frames per output frame), the error advances by
/// `N (r_true - r)` per update; writing `r = 1 + (b e + z) / N` and
/// `z += c e` gives `e' = e - b e - z + N (r_true - 1)`, the 2005 loop, so the
/// same `b` and `c` make it critically damped at bandwidth `B`.
#[derive(Debug, Clone)]
pub struct RatioController {
    updates_per_s: f64,
    frames_per_update: f64,
    lp1: f64,
    lp2: f64,
    w_lp: f64,
    b: f64,
    c: f64,
    z: f64,
    updates: u64,
    startup_updates: u64,
    clamp: f64,
    clamped: bool,
}

impl RatioController {
    /// A loop updated once per `frames_per_update` output frames at
    /// `rate_hz`, starting from `ratio`.
    pub fn new(frames_per_update: u32, rate_hz: u32, ratio: f64) -> RatioController {
        let frames_per_update = f64::from(frames_per_update.max(1));
        let updates_per_s = f64::from(rate_hz.max(1)) / frames_per_update;
        let clamp = MAX_RATIO_PPM * 1e-6;
        let mut c = RatioController {
            updates_per_s,
            frames_per_update,
            lp1: 0.0,
            lp2: 0.0,
            w_lp: 0.0,
            b: 0.0,
            c: 0.0,
            z: 0.0,
            updates: 0,
            startup_updates: (STARTUP_SECONDS * updates_per_s).round() as u64,
            clamp,
            clamped: false,
        };
        c.z = (ratio - 1.0).clamp(-clamp, clamp) * frames_per_update;
        c.set_bandwidth(STARTUP_BANDWIDTH_HZ);
        c
    }

    fn set_bandwidth(&mut self, bandwidth_hz: f64) {
        let w = 2.0 * PI * bandwidth_hz / self.updates_per_s;
        self.b = SQRT_2 * w;
        self.c = w * w;
        let corner = LOWPASS_FACTOR * bandwidth_hz;
        self.w_lp = 1.0 - (-2.0 * PI * corner / self.updates_per_s).exp();
    }

    /// One update with error `e` (source frames, positive: the resampler is
    /// behind where the stamps say it should be): the ratio to render the
    /// next period at.
    pub fn update(&mut self, e: f64) -> f64 {
        self.updates += 1;
        if self.updates == self.startup_updates {
            self.set_bandwidth(LOOP_BANDWIDTH_HZ);
        }
        self.lp1 += self.w_lp * (e - self.lp1);
        self.lp2 += self.w_lp * (self.lp1 - self.lp2);
        let ef = self.lp2;
        self.z += self.c * ef;
        // Anti-windup: the integrator alone never asks past the clamp.
        let zmax = self.clamp * self.frames_per_update;
        self.z = self.z.clamp(-zmax, zmax);
        let dev = (self.b * ef + self.z) / self.frames_per_update;
        self.clamped = dev.abs() >= self.clamp;
        1.0 + dev.clamp(-self.clamp, self.clamp)
    }

    /// Whether the last ratio was held at the clamp.
    pub fn clamped(&self) -> bool {
        self.clamped
    }
}

/// Why the matcher dropped its lock (and starts again from the next period).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relock {
    /// The DLL's error passed [`RELOCK_ERROR_NS`].
    TimingJump,
    /// More source than the ring holds.
    RingOverflow,
    /// The output asked for source not captured yet.
    RingUnderflow,
}

impl Relock {
    /// The `reason=` word.
    pub fn name(self) -> &'static str {
        match self {
            Relock::TimingJump => "timing-jump",
            Relock::RingOverflow => "ring-overflow",
            Relock::RingUnderflow => "ring-underflow",
        }
    }
}

/// What one period produced.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Produced {
    /// Output frames appended to [`RateMatcher::output`].
    pub frames: usize,
    /// The output index (since the lock) of the first of them.
    pub first_index: u64,
    /// The lock this output belongs to: it changes on every relock, and the
    /// output index restarts at 0.
    pub lock: u64,
    /// Set when this period broke the lock.
    pub relocked: Option<Relock>,
}

/// The DLL, the ratio loop and the resampler, joined.
#[derive(Debug, Clone)]
pub struct RateMatcher {
    channels: usize,
    rate_hz: u32,
    period_frames: u32,
    out_frame_ns: f64,
    origin_ns: i128,
    dll: Option<PeriodDll>,
    ctl: Option<RatioController>,
    resampler: CubicResampler,
    /// Source frames captured since the lock (the DLL's `W`).
    frames_in: u64,
    /// The source frame index the resampler's position 0 is.
    base: u64,
    periods: u32,
    /// Output state: stamp of output frame 0 (relative ns), output frames
    /// produced, the source position of the next one, the ratio.
    s0: f64,
    out_frames: u64,
    position: f64,
    ratio: f64,
    lock: u64,
    last_error_frames: f64,
    output: Vec<f64>,
}

impl RateMatcher {
    /// A matcher for `channels` at nominal `rate_hz`, captured in periods of
    /// `period_frames`.
    pub fn new(channels: u16, rate_hz: u32, period_frames: u32) -> RateMatcher {
        let channels = usize::from(channels.max(1));
        RateMatcher {
            channels,
            rate_hz: rate_hz.max(1),
            period_frames: period_frames.max(1),
            out_frame_ns: 1e9 / f64::from(rate_hz.max(1)),
            origin_ns: 0,
            dll: None,
            ctl: None,
            resampler: CubicResampler::with_capacity(channels, RING_FRAMES),
            frames_in: 0,
            base: 0,
            periods: 0,
            s0: 0.0,
            out_frames: 0,
            position: 0.0,
            ratio: 1.0,
            lock: 0,
            last_error_frames: 0.0,
            output: Vec::with_capacity(2 * period_frames as usize * channels),
        }
    }

    /// Forget the lock: the next period starts a new one.
    pub fn reset(&mut self) {
        self.dll = None;
        self.ctl = None;
        self.resampler.clear();
        self.frames_in = 0;
        self.base = 0;
        self.periods = 0;
        self.out_frames = 0;
        self.output.clear();
    }

    /// Whether output is being produced (the DLL has warmed up).
    pub fn running(&self) -> bool {
        self.ctl.is_some()
    }

    /// The DLL's estimate of the source's rate against nominal, ppm, once it
    /// has run a period.
    pub fn estimate_ppm(&self) -> Option<f64> {
        self.dll.as_ref().map(PeriodDll::ppm)
    }

    /// The ratio the output is rendered at now.
    pub fn ratio(&self) -> f64 {
        self.ratio
    }

    /// Whether the loop's ratio is held at the clamp.
    pub fn clamped(&self) -> bool {
        self.ctl.as_ref().is_some_and(RatioController::clamped)
    }

    /// The loop's last error, source frames (the stamp error of the next
    /// output frame).
    pub fn error_frames(&self) -> f64 {
        self.last_error_frames
    }

    /// The output the last period produced, interleaved.
    pub fn output(&self) -> &[f64] {
        &self.output
    }

    /// The server-timeline stamp of output frame `index` of the current lock,
    /// ns: `S0 + index / rate`.
    pub fn stamp_ns(&self, index: u64) -> u64 {
        let rel = self.s0 + index as f64 * self.out_frame_ns;
        (self.origin_ns + rel.round() as i128).clamp(0, i128::from(u64::MAX)) as u64
    }

    /// The source position (fractional, frames since the lock) output frame
    /// `index`'s stamp should play, by the DLL: for tests and reports.
    pub fn source_position_of_next(&self) -> f64 {
        self.base as f64 + self.position
    }

    /// One captured period: `samples` (interleaved, exactly the period),
    /// read back at `read_at_ns` on the server timeline with the device
    /// reporting `delay_frames` frames digitized and not yet read. Appends
    /// the output it allows to [`RateMatcher::output`] (cleared first).
    pub fn push_period(
        &mut self,
        samples: &[f64],
        read_at_ns: i128,
        delay_frames: i64,
    ) -> Produced {
        self.output.clear();
        let mut produced = Produced {
            lock: self.lock,
            ..Produced::default()
        };
        let frames = samples.len() / self.channels;
        debug_assert_eq!(frames, self.period_frames as usize);
        let origin_ns = self.origin_ns;
        let (e, t0, te, dll_ratio) = match self.dll.as_mut() {
            None => {
                self.begin(samples, read_at_ns, delay_frames);
                return produced;
            }
            Some(dll) => {
                // The instant the next frame (the one after this period) was
                // digitized: the read's return less the device's delay.
                let delay_ns = delay_frames.max(0) as f64 * dll.frame_ns();
                let t = (read_at_ns - origin_ns) as f64 - delay_ns;
                let e = dll.update(t);
                (e, dll.time(), dll.frame_ns(), dll.ratio())
            }
        };
        if e.abs() > RELOCK_ERROR_NS {
            return self.relock(Relock::TimingJump, samples, read_at_ns, delay_frames);
        }
        if self.ctl.is_none() {
            // Warming up: keep only the newest period, re-based.
            self.resampler.clear();
            self.base = self.frames_in;
        }
        if self.resampler.room() < frames {
            return self.relock(Relock::RingOverflow, samples, read_at_ns, delay_frames);
        }
        self.resampler.push(samples);
        self.frames_in += frames as u64;
        self.periods = self.periods.saturating_add(1);

        if self.ctl.is_none() {
            if self.periods < WARMUP_PERIODS {
                return produced;
            }
            // Start: output frame 0 is stamped SEND_DELAY_NS behind the
            // newest capture, and the resampler starts exactly at the source
            // position captured then (a fractional skip, so the initial
            // error is zero: Adriaensen 2012 section 3.4's one-off skip).
            self.s0 = t0 - SEND_DELAY_NS;
            self.out_frames = 0;
            self.position = (self.frames_in - self.base) as f64 - SEND_DELAY_NS / te;
            self.ratio = dll_ratio.clamp(1.0 - MAX_RATIO_PPM * 1e-6, 1.0 + MAX_RATIO_PPM * 1e-6);
            self.ctl = Some(RatioController::new(
                self.period_frames,
                self.rate_hz,
                self.ratio,
            ));
        }

        // Output every frame whose stamp is at least SEND_DELAY_NS old.
        let due = ((t0 - SEND_DELAY_NS - self.s0) / self.out_frame_ns).floor();
        let target = if due < 0.0 { 0 } else { due as u64 + 1 };
        let n = target.saturating_sub(self.out_frames) as u32;
        if n > 0 {
            let plan = ChunkPlan::constant_rate(self.position, self.ratio, n);
            if self.resampler.render(&plan, &mut self.output).is_err() {
                self.output.clear();
                return self.relock(Relock::RingUnderflow, samples, read_at_ns, delay_frames);
            }
            produced.frames = n as usize;
            produced.first_index = self.out_frames;
            self.position = plan.source_end;
            self.out_frames += u64::from(n);
        }

        // The error at the next output frame, with the resampler's
        // fractional position in it.
        let stamp_next = self.s0 + self.out_frames as f64 * self.out_frame_ns;
        let x_star = (self.frames_in - self.base) as f64 + (stamp_next - t0) / te;
        let e = x_star - self.position;
        self.last_error_frames = e;
        if let Some(ctl) = self.ctl.as_mut() {
            self.ratio = ctl.update(e);
        }
        produced
    }

    fn begin(&mut self, samples: &[f64], read_at_ns: i128, delay_frames: i64) {
        self.origin_ns = read_at_ns;
        let nominal = self.out_frame_ns;
        let t = -(delay_frames.max(0) as f64) * nominal;
        self.dll = Some(PeriodDll::new(
            self.period_frames,
            self.rate_hz,
            DLL_BANDWIDTH_HZ,
            t,
        ));
        self.resampler.clear();
        self.resampler.push(samples);
        self.frames_in = (samples.len() / self.channels) as u64;
        self.base = 0;
        self.periods = 1;
    }

    fn relock(
        &mut self,
        why: Relock,
        samples: &[f64],
        read_at_ns: i128,
        delay_frames: i64,
    ) -> Produced {
        self.reset();
        self.lock += 1;
        self.begin(samples, read_at_ns, delay_frames);
        Produced {
            lock: self.lock,
            relocked: Some(why),
            ..Produced::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dll_coefficients_are_the_papers_for_one_hertz_at_two_hundred_periods() {
        // Research capture.md 6.2: w = 0.0314, b = 0.0444, c = 0.000987.
        let d = PeriodDll::new(240, 48_000, 1.0, 0.0);
        assert!((d.b - 0.0444).abs() < 1e-4, "{}", d.b);
        assert!((d.c - 0.000987).abs() < 1e-6, "{}", d.c);
    }

    #[test]
    fn the_dll_finds_a_fast_clock_through_jitter() {
        // A source 700 ppm fast, period ends jittered by up to +/-100 us (a
        // model value, not a measured wakeup jitter).
        let period = 5_000_000.0 / 1.0007;
        let mut d = PeriodDll::new(240, 48_000, 1.0, 0.0);
        let mut seed = 1u64;
        for k in 1..2_000u32 {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            let jitter = ((seed >> 33) as f64 / (1u64 << 31) as f64 - 0.5) * 200_000.0;
            d.update(f64::from(k) * period + jitter);
        }
        assert!((d.ppm() - 700.0).abs() < 100.0, "{}", d.ppm());
    }

    #[test]
    fn the_controller_starts_at_its_ratio_and_holds_the_clamp() {
        let mut c = RatioController::new(240, 48_000, 1.0004);
        assert!((c.update(0.0) - 1.0004).abs() < 1e-9);
        let mut r = 1.0;
        for _ in 0..10_000 {
            r = c.update(1e6);
        }
        assert!((r - 1.0015).abs() < 1e-12);
        assert!(c.clamped());
    }

    #[test]
    fn a_matcher_stamps_output_at_exactly_the_nominal_rate() {
        let mut m = RateMatcher::new(1, 48_000, 240);
        let period = 5_000_000.0 / 1.0002;
        let mut index = 0u64;
        let mut produced_frames = 0u64;
        for k in 0..2_000u64 {
            let samples: Vec<f64> = (0..240).map(|i| (k * 240 + i) as f64).collect();
            let at = 1_000_000_000i128 + (k as f64 * period) as i128;
            let p = m.push_period(&samples, at, 0);
            assert!(p.relocked.is_none());
            if p.frames > 0 {
                assert_eq!(p.first_index, index);
                index += p.frames as u64;
                produced_frames += p.frames as u64;
            }
        }
        assert!(m.running());
        assert_eq!(m.stamp_ns(48_000) - m.stamp_ns(0), 1_000_000_000);
        assert!(produced_frames > 0);
        assert!(m.error_frames().abs() < SETTLED_BOUND_FRAMES);
    }
}
