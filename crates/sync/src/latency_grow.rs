//! Latency growth: moving a live stream's play-at offset without a glitch (K94).
//!
//! A line-in played only in its own room can run at a low latency L_local:
//! the capture endpoint and the playing endpoint are both on the wired tier,
//! so a chunk stamped at its capture instant reaches the player a few
//! milliseconds after it is complete. When another room joins, the group is
//! held to a deeper buffer L_group (the group's `playout_latency_us`, or the
//! wireless tier's when a Wi-Fi room is in it). Jumping the offset at once
//! would leave L_group - L_local of the playing room's timeline with nothing
//! to play: an audible gap. This module grows the offset gradually instead,
//! by playing the source slightly slower than it was captured for a while
//! (a time stretch by resampling) so the play-at stamps fall behind the
//! capture stamps a little more with every chunk.
//!
//! # The model
//!
//! Output frame `j` of the stream (counted from its first chunk) is due at
//! `P_0 + j / fs` on the server timeline, so output chunks are contiguous and
//! every room in the group renders the same stamps: chunk `k` is frames
//! `[k N, (k + 1) N)` and its play-at stamp is `P_0 + k N / fs`. What moves is
//! which SOURCE position output frame `j` plays. Writing `s_j` for it (a
//! fractional source frame index, source frame 0 captured at `c_0`) and
//! `D(j) = j - s_j` for how many output frames the source has fallen behind,
//! the stamp offset at frame `j` is
//!
//! ```text
//! L(j) = (P_0 + j / fs) - (c_0 + s_j / fs) = L_0 + D(j) / fs
//! ```
//!
//! so growing L by `dL` is exactly `D` growing by `dL * fs` frames. The source
//! advances by `1 - r(j)` frames per output frame, where `r` is the rate
//! deviation (positive: slower, the offset grows; negative: faster, it
//! shrinks), and `D` is the integral of `r`.
//!
//! # The rate profile
//!
//! A transition is a raised-cosine ramp of `r` from 0 to a peak, a hold at the
//! peak, and the mirror ramp back to 0:
//!
//! ```text
//! up:    r(u) = p (1 - cos(pi u / R)) / 2          0 <= u <= R
//! hold:  r(u) = p                                  R <= u <= R + H
//! down:  r(u) = p (1 + cos(pi (u - R - H) / R)) / 2
//! ```
//!
//! The rate therefore never steps (it is continuous with a continuous first
//! derivative), its slope is bounded by `|p| pi / (2 R)` per frame, and the
//! area under it is `p (R + H)` frames, which is the growth. A change smaller
//! than `r_max R` frames is made with no hold and a reduced peak; a larger one
//! holds `r_max`. Every quantity is a closed form in `u`, so the source
//! position of any frame is computed, never accumulated: a ten-minute
//! transition ends exactly on its target and floating error does not drift.
//!
//! # Where the server runs it
//!
//! In the line-in source's slot, between the PCM it receives upstream from the
//! capture endpoint (`audio_chunk`s stamped at their capture instants, ADR
//! 0066) and the chunks it stamps and sends to the group. One plan and one
//! resampler per source stream, not per room: every room receives the same
//! resampled chunks with the same stamps, which is what keeps the rooms in
//! step with each other through a transition. It is not the endpoint's hot
//! path, so this is plain `f64` arithmetic.
//!
//! What this does not model (the ADR lists them as follow-ups): a gap in the
//! source (frames an overrun lost show as a jump in the capture stamps; the
//! server would fill it with silence before planning), the capture clock's
//! own drift against the server timeline, and the endpoint-side `output_delay`
//! trims, which are applied on top of the stamps and are unchanged by this.
//!
//! Everything numeric here is a model input. None of it is timing evidence
//! (BRIEF.md section 3.1 rule 3); `docs/measurements/latency-growth-sim.md` is
//! the simulation report and `docs/decisions/` holds the record.

use std::collections::VecDeque;
use std::f64::consts::PI;
use std::fmt;

/// The largest rate deviation a plan accepts, as a fraction (1 %).
///
/// A guard against a configuration error, not the bound chorus uses: 1 % is
/// about 17 cents, far above any pitch-change threshold, so a plan asked for
/// more is refused rather than obeyed. The bound chorus uses is chosen in the
/// decision record from a cited just-noticeable difference.
pub const MAX_ALLOWED_RATE_DEVIATION: f64 = 0.01;

/// The stretch bound chorus uses: 500 ppm, about 0.87 cents.
///
/// Chosen in the decision record that adds this module from a cited pitch
/// discrimination threshold: the smallest mean frequency difference limen
/// trained listeners showed for steady pure tones, 0.2 % at 1 kHz (Dai and
/// Micheyl 2011, JASA 130(1), https://pmc.ncbi.nlm.nih.gov/articles/PMC3155586/,
/// read 2026-10-01), is four times this. Added to the largest rate correction
/// an endpoint's own servo makes (`max_correction_ppm = 300`,
/// `config/sync.conf`), 800 ppm is still two and a half times under it. The
/// margin is a choice, not a measurement: ASSUMED until a listening test.
pub const MAX_RATE_DEVIATION: f64 = 500e-6;

/// Each raised-cosine ramp's length, in ms. ASSUMED: long enough that the
/// rate's change per 20 ms chunk is under 2 ppm, short against a transition
/// of minutes.
pub const RAMP_MS: u64 = 10_000;

/// How far past a position the interpolator reads, in whole frames.
///
/// Catmull-Rom between frames `i` and `i + 1` reads `i + 2`. If output chunks
/// ended where source chunks end, the last output frame of chunk k would read
/// two frames into source chunk k + 1, and the server would wait a whole
/// source chunk (20 ms) for two frames. So the stream's first output chunk is
/// this many frames short: every later output chunk then ends this many frames
/// before the source chunk it reads, and at rate 1 the frames it reads are
/// exactly that source chunk's. The cost is these frames (about 42 us at
/// 48 kHz), not a chunk. It holds at any offset at or above the initial one
/// (`D >= 0`): the last frame chunk k reads is then `(k + 1) n - D`, inside
/// source chunk k. Growing and shrinking back to the initial latency stay
/// there; a target below the initial latency reads into the next source chunk
/// and the caller must have budgeted for that (the simulator never asks).
pub const LOOKAHEAD_FRAMES: u32 = 2;

/// Why a plan or a render was refused.
#[derive(Debug, Clone, PartialEq)]
pub enum PlanError {
    /// A configuration value is outside what the plan can honour.
    Config(&'static str),
    /// A render needs source frames the resampler has not been given yet.
    NeedSource {
        /// Exclusive end of the source frames the render needs.
        needed: u64,
        /// Exclusive end of the source frames it holds.
        available: u64,
    },
    /// A render needs source frames the resampler already discarded, which
    /// only happens if chunks are rendered out of order.
    Discarded {
        /// The first frame it needs.
        needed: u64,
        /// The first frame it still holds.
        first_held: u64,
    },
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlanError::Config(why) => write!(f, "configuration: {}", why),
            PlanError::NeedSource { needed, available } => write!(
                f,
                "the render needs source frames up to {} and only {} have arrived",
                needed, available
            ),
            PlanError::Discarded { needed, first_held } => write!(
                f,
                "the render needs source frame {} and frames before {} were discarded",
                needed, first_held
            ),
        }
    }
}

impl std::error::Error for PlanError {}

/// What a plan is held to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GrowthConfig {
    /// The stream's sample rate, source and output alike, in Hz.
    pub sample_rate_hz: u32,
    /// Output frames per chunk.
    pub chunk_frames: u32,
    /// The largest |rate deviation| a transition may reach, as a fraction
    /// (500e-6 is 500 ppm, about 0.87 cents).
    pub max_rate_deviation: f64,
    /// The length of each raised-cosine ramp, in output frames.
    pub ramp_frames: u64,
}

impl GrowthConfig {
    /// Check the values a plan relies on.
    pub fn validate(&self) -> Result<(), PlanError> {
        if !(8_000..=384_000).contains(&self.sample_rate_hz) {
            return Err(PlanError::Config(
                "sample_rate_hz is outside 8000-384000 (the protocol's range)",
            ));
        }
        if self.chunk_frames <= LOOKAHEAD_FRAMES {
            return Err(PlanError::Config(
                "chunk_frames is not above LOOKAHEAD_FRAMES",
            ));
        }
        let r = self.max_rate_deviation;
        if !r.is_finite() || r <= 0.0 || r > MAX_ALLOWED_RATE_DEVIATION {
            return Err(PlanError::Config(
                "max_rate_deviation is not in (0, MAX_ALLOWED_RATE_DEVIATION]",
            ));
        }
        if self.ramp_frames == 0 {
            return Err(PlanError::Config("ramp_frames is 0"));
        }
        Ok(())
    }

    /// Nanoseconds per frame.
    pub fn frame_ns(&self) -> f64 {
        1e9 / f64::from(self.sample_rate_hz)
    }

    /// The largest change of the rate deviation from one chunk's first frame
    /// to the next chunk's, for a transition at the full peak: the profile's
    /// slope bound times a chunk.
    pub fn max_rate_change_per_chunk(&self) -> f64 {
        self.max_rate_deviation * PI / (2.0 * self.ramp_frames as f64)
            * f64::from(self.chunk_frames)
    }

    /// How long a transition of `delta_ns` of latency takes, in ns of output.
    pub fn transition_ns(&self, delta_ns: f64) -> f64 {
        RateProfile::for_change(self, delta_ns.abs() / self.frame_ns()).length_frames()
            * self.frame_ns()
    }
}

/// One transition's rate profile, in output frames from its start.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RateProfile {
    /// Peak rate deviation, signed (positive grows the latency).
    pub peak: f64,
    /// Ramp length, in frames.
    pub ramp: f64,
    /// Hold length, in frames.
    pub hold: f64,
}

impl RateProfile {
    /// The profile that changes `D` by `delta_frames` (signed).
    pub fn for_change(config: &GrowthConfig, delta_frames: f64) -> RateProfile {
        let ramp = config.ramp_frames as f64;
        let r_max = config.max_rate_deviation;
        let magnitude = delta_frames.abs();
        let (peak, hold) = if magnitude >= r_max * ramp {
            (r_max, magnitude / r_max - ramp)
        } else {
            (magnitude / ramp, 0.0)
        };
        RateProfile {
            peak: peak.copysign(delta_frames),
            ramp,
            hold,
        }
    }

    /// Total length, in frames.
    pub fn length_frames(&self) -> f64 {
        2.0 * self.ramp + self.hold
    }

    /// The change in `D` the whole profile makes, in frames.
    pub fn total(&self) -> f64 {
        self.peak * (self.ramp + self.hold)
    }

    /// The rate deviation at `u` frames from the start.
    pub fn rate(&self, u: f64) -> f64 {
        let (p, r, h) = (self.peak, self.ramp, self.hold);
        if u <= 0.0 {
            0.0
        } else if u < r {
            p * (1.0 - (PI * u / r).cos()) / 2.0
        } else if u <= r + h {
            p
        } else if u < 2.0 * r + h {
            p * (1.0 + (PI * (u - r - h) / r).cos()) / 2.0
        } else {
            0.0
        }
    }

    /// The integral of the rate from the start to `u`: how far `D` has moved.
    pub fn integral(&self, u: f64) -> f64 {
        let (p, r, h) = (self.peak, self.ramp, self.hold);
        if u <= 0.0 {
            0.0
        } else if u < r {
            p / 2.0 * (u - r / PI * (PI * u / r).sin())
        } else if u <= r + h {
            p * r / 2.0 + p * (u - r)
        } else if u < 2.0 * r + h {
            let v = u - r - h;
            p * r / 2.0 + p * h + p / 2.0 * (v + r / PI * (PI * v / r).sin())
        } else {
            self.total()
        }
    }
}

/// The offset state that holds over one chunk: the settled offset plus the
/// transition in progress, if any. Small and `Copy`, so a [`ChunkPlan`]
/// carries it and evaluates any of its frames on its own.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    /// `D` before the transition, in frames.
    pub settled_frames: f64,
    /// The transition: the output frame it starts at, and its profile.
    pub transition: Option<(u64, RateProfile)>,
}

impl Segment {
    /// `D` at output frame `j`.
    pub fn offset_frames(&self, j: u64) -> f64 {
        match self.transition {
            Some((start, profile)) if j > start => {
                self.settled_frames + profile.integral((j - start) as f64)
            }
            _ => self.settled_frames,
        }
    }

    /// The rate deviation at output frame `j`.
    pub fn rate(&self, j: u64) -> f64 {
        match self.transition {
            Some((start, profile)) if j > start => profile.rate((j - start) as f64),
            _ => 0.0,
        }
    }
}

/// One output chunk: what it plays and when.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChunkPlan {
    /// The chunk's index from the stream's first.
    pub index: u64,
    /// Output frames in it: the configured chunk, except the stream's first,
    /// which is [`LOOKAHEAD_FRAMES`] short.
    pub frames: u32,
    /// Its first frame's index on the output timeline.
    pub first_frame: u64,
    /// When its first frame is due, server timeline, in ns.
    pub play_at_ns: i64,
    /// The source position (fractional frame) its first frame plays.
    pub source_start: f64,
    /// The source position the next chunk's first frame plays: this chunk
    /// consumes `source_end - source_start` source frames.
    pub source_end: f64,
    /// The rate deviation at its first frame.
    pub rate_start: f64,
    /// Its play-at stamp minus the capture instant of `source_start`, in ns:
    /// the latency it is played at.
    pub offset_ns: f64,
    segment: Segment,
}

impl ChunkPlan {
    /// Source frames it consumes (fractional).
    pub fn source_frames(&self) -> f64 {
        self.source_end - self.source_start
    }

    /// The source position output frame `i` of this chunk plays, for `i` in
    /// `0..=frames` (`frames` is the next chunk's first).
    pub fn source_position(&self, i: u32) -> f64 {
        let j = self.first_frame + u64::from(i);
        j as f64 - self.segment.offset_frames(j)
    }

    /// The rate deviation at output frame `i` of this chunk.
    pub fn rate(&self, i: u32) -> f64 {
        self.segment.rate(self.first_frame + u64::from(i))
    }

    /// Exclusive end of the source frames rendering this chunk reads: the
    /// cubic interpolator reads two frames past the last position's floor.
    pub fn source_frames_needed(&self) -> u64 {
        let last = self.source_position(self.frames - 1).max(0.0);
        last.floor() as u64 + 3
    }
}

/// The plan: hands out output chunks in order, moving the offset toward the
/// latest target by the profile above.
#[derive(Debug, Clone)]
pub struct LatencyPlan {
    config: GrowthConfig,
    capture_origin_ns: i64,
    initial_latency_ns: i64,
    segment: Segment,
    target_frames: f64,
    next_chunk: u64,
}

impl LatencyPlan {
    /// A plan for a source whose frame 0 was captured at `capture_origin_ns`
    /// (server timeline), played at `initial_latency_ns`.
    pub fn new(
        config: GrowthConfig,
        capture_origin_ns: i64,
        initial_latency_ns: i64,
    ) -> Result<LatencyPlan, PlanError> {
        config.validate()?;
        if initial_latency_ns < 0 {
            return Err(PlanError::Config("initial_latency_ns is negative"));
        }
        Ok(LatencyPlan {
            config,
            capture_origin_ns,
            initial_latency_ns,
            segment: Segment {
                settled_frames: 0.0,
                transition: None,
            },
            target_frames: 0.0,
            next_chunk: 0,
        })
    }

    /// The configuration.
    pub fn config(&self) -> &GrowthConfig {
        &self.config
    }

    /// Ask for a new latency.
    ///
    /// Takes effect at the next chunk boundary if no transition is running.
    /// A target set while one is running is held until it ends (the rate is
    /// then back at 0, so the next profile starts from rest and the rate stays
    /// continuous); only the latest target is kept. Growing and shrinking are
    /// the same mechanism with the sign of the rate reversed.
    pub fn set_target(&mut self, latency_ns: i64) -> Result<(), PlanError> {
        if latency_ns < 0 {
            return Err(PlanError::Config("target latency is negative"));
        }
        self.target_frames = (latency_ns - self.initial_latency_ns) as f64 / self.config.frame_ns();
        Ok(())
    }

    /// The latency the last target asked for, in ns.
    pub fn target_ns(&self) -> f64 {
        self.initial_latency_ns as f64 + self.target_frames * self.config.frame_ns()
    }

    /// The latency the running transition ends at (the settled latency when
    /// none is running), in ns. A target set mid-transition is not this: it
    /// waits for the running one to end.
    pub fn heading_ns(&self) -> f64 {
        let end =
            self.segment.settled_frames + self.segment.transition.map_or(0.0, |(_, p)| p.total());
        self.initial_latency_ns as f64 + end * self.config.frame_ns()
    }

    /// Whether the offset is at the target with no transition running.
    pub fn is_settled(&self) -> bool {
        self.segment.transition.is_none() && self.segment.settled_frames == self.target_frames
    }

    /// The next output chunk.
    pub fn next_chunk(&mut self) -> ChunkPlan {
        let n = u64::from(self.config.chunk_frames);
        let k = self.next_chunk;
        // Output chunk k >= 1 is frames [k n - A, (k + 1) n - A): see
        // LOOKAHEAD_FRAMES for why the first is A frames short.
        let a = u64::from(LOOKAHEAD_FRAMES);
        let j0 = if k == 0 { 0 } else { k * n - a };
        let j1 = (k + 1) * n - a;

        // A finished transition folds into the settled offset. The fold uses
        // the profile's exact total rather than the integral at some frame,
        // so the settled value is the target to the last bit the arithmetic
        // allows.
        if let Some((start, profile)) = self.segment.transition {
            if (j0 - start) as f64 >= profile.length_frames() {
                self.segment.settled_frames += profile.total();
                self.segment.transition = None;
                if (self.segment.settled_frames - self.target_frames).abs() < 1e-6 {
                    self.segment.settled_frames = self.target_frames;
                }
            }
        }
        if self.segment.transition.is_none() && self.segment.settled_frames != self.target_frames {
            let delta = self.target_frames - self.segment.settled_frames;
            self.segment.transition = Some((j0, RateProfile::for_change(&self.config, delta)));
        }

        let fs = i128::from(self.config.sample_rate_hz);
        let elapsed_ns = ((i128::from(j0) * 1_000_000_000 + fs / 2) / fs) as i64;
        let play_at_ns = self.capture_origin_ns + self.initial_latency_ns + elapsed_ns;

        let segment = self.segment;
        let source_start = j0 as f64 - segment.offset_frames(j0);
        let source_end = j1 as f64 - segment.offset_frames(j1);
        let capture_ns = self.capture_origin_ns as f64 + source_start * self.config.frame_ns();
        self.next_chunk += 1;
        ChunkPlan {
            index: k,
            frames: (j1 - j0) as u32,
            first_frame: j0,
            play_at_ns,
            source_start,
            source_end,
            rate_start: segment.rate(j0),
            offset_ns: play_at_ns as f64 - capture_ns,
            segment,
        }
    }
}

/// A four-point cubic (Catmull-Rom) interpolator over a stream of source
/// frames, rendering the positions a [`ChunkPlan`] names.
///
/// Catmull-Rom is the cubic Hermite spline whose tangents are the central
/// differences of the neighbouring samples: it passes through every sample,
/// reproduces any straight line exactly (the property the simulator's
/// position check uses), and needs one frame behind and two ahead. It is
/// chosen as a simple, documented interpolator for the model; for a stretch
/// of under a tenth of a percent its aliasing is far below the rate change
/// itself, and a windowed-sinc resampler is a drop-in replacement behind the
/// same `render` if listening says otherwise.
#[derive(Debug, Clone)]
pub struct CubicResampler {
    channels: usize,
    first_held: u64,
    samples: VecDeque<f64>,
}

impl CubicResampler {
    /// A resampler for `channels` interleaved channels, holding nothing.
    pub fn new(channels: usize) -> CubicResampler {
        assert!(channels > 0, "a stream has at least one channel");
        CubicResampler {
            channels,
            first_held: 0,
            samples: VecDeque::new(),
        }
    }

    /// Append whole interleaved source frames.
    pub fn push(&mut self, interleaved: &[f64]) {
        assert_eq!(
            interleaved.len() % self.channels,
            0,
            "a push is a whole number of frames"
        );
        self.samples.extend(interleaved.iter().copied());
    }

    /// Exclusive end of the source frames it holds.
    pub fn available_end(&self) -> u64 {
        self.first_held + (self.samples.len() / self.channels) as u64
    }

    fn sample(&self, frame: i64, channel: usize) -> f64 {
        // Frame -1 (the frame before the stream's first) repeats frame 0, so
        // the first output frame interpolates from real data.
        let frame = frame.max(0) as u64;
        self.samples[(frame - self.first_held) as usize * self.channels + channel]
    }

    /// Render `chunk` into `out` (cleared first), interleaved, then discard
    /// the source frames no later chunk can need.
    pub fn render(&mut self, chunk: &ChunkPlan, out: &mut Vec<f64>) -> Result<(), PlanError> {
        let needed = chunk.source_frames_needed();
        if needed > self.available_end() {
            return Err(PlanError::NeedSource {
                needed,
                available: self.available_end(),
            });
        }
        let first_needed = (chunk.source_start.max(0.0).floor() as u64).saturating_sub(1);
        if first_needed < self.first_held {
            return Err(PlanError::Discarded {
                needed: first_needed,
                first_held: self.first_held,
            });
        }
        out.clear();
        out.reserve(chunk.frames as usize * self.channels);
        for i in 0..chunk.frames {
            let position = chunk.source_position(i).max(0.0);
            let base = position.floor();
            let t = position - base;
            let base = base as i64;
            for c in 0..self.channels {
                let p0 = self.sample(base - 1, c);
                let p1 = self.sample(base, c);
                let p2 = self.sample(base + 1, c);
                let p3 = self.sample(base + 2, c);
                out.push(catmull_rom(p0, p1, p2, p3, t));
            }
        }
        // The next chunk starts at `source_end` and reads one frame behind it.
        let keep_from = (chunk.source_end.max(0.0).floor() as u64).saturating_sub(1);
        if keep_from > self.first_held {
            let drop =
                ((keep_from - self.first_held) as usize * self.channels).min(self.samples.len());
            self.samples.drain(..drop);
            self.first_held = keep_from;
        }
        Ok(())
    }
}

/// Catmull-Rom between `p1` and `p2` at `t` in [0, 1).
pub fn catmull_rom(p0: f64, p1: f64, p2: f64, p3: f64, t: f64) -> f64 {
    p1 + 0.5
        * t
        * (p2 - p0 + t * (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3 + t * (3.0 * (p1 - p2) + p3 - p0)))
}

#[cfg(test)]
mod tests {
    use super::{catmull_rom, CubicResampler, GrowthConfig, LatencyPlan, PlanError, RateProfile};

    fn config() -> GrowthConfig {
        GrowthConfig {
            sample_rate_hz: 48_000,
            chunk_frames: 960,
            max_rate_deviation: 500e-6,
            ramp_frames: 48_000,
        }
    }

    #[test]
    fn a_profile_integrates_to_its_total_and_is_continuous() {
        let c = config();
        let p = RateProfile::for_change(&c, 7_200.0);
        assert_eq!(p.peak, 500e-6);
        assert!((p.total() - 7_200.0).abs() < 1e-9);
        let len = p.length_frames();
        // Continuity at the four joins: up start, up/hold, hold/down, down end.
        for u in [0.0, p.ramp, p.ramp + p.hold, len] {
            assert!(
                (p.rate(u - 1e-3) - p.rate(u + 1e-3)).abs() < 1e-9,
                "a step at {}",
                u
            );
            // Over 2e-3 frames the integral moves by at most the peak times that.
            assert!((p.integral(u - 1e-3) - p.integral(u + 1e-3)).abs() <= 2e-3 * p.peak + 1e-9);
        }
        assert!((p.integral(len) - p.total()).abs() < 1e-9);
        // A small change uses a reduced peak and no hold.
        let small = RateProfile::for_change(&c, -6.0);
        assert_eq!(small.hold, 0.0);
        assert!(small.peak < 0.0 && small.peak.abs() < 500e-6);
        assert!((small.integral(small.length_frames()) + 6.0).abs() < 1e-12);
    }

    #[test]
    fn a_plan_grows_to_its_target_and_stops() {
        let c = config();
        let mut plan = LatencyPlan::new(c, 1_000_000_000, 30_000_000).expect("valid");
        let first = plan.next_chunk();
        assert_eq!(first.play_at_ns, 1_030_000_000);
        assert!((first.offset_ns - 30e6).abs() < 1.0);
        plan.set_target(40_000_000).expect("valid");
        let mut last = first;
        let mut previous = first;
        for _ in 0..2_000 {
            let chunk = plan.next_chunk();
            // Contiguous: each chunk starts where the previous one ended.
            let expected = f64::from(previous.frames) * c.frame_ns();
            assert!(((chunk.play_at_ns - previous.play_at_ns) as f64 - expected).abs() <= 1.0);
            assert!((chunk.source_start - previous.source_end).abs() < 1e-9);
            // Stamps are whole ns, so an offset can wobble by the rounding.
            assert!(chunk.offset_ns >= previous.offset_ns - 1.0);
            previous = chunk;
            last = chunk;
        }
        assert!(plan.is_settled());
        assert!((last.offset_ns - 40e6).abs() < 1.0, "{}", last.offset_ns);
        assert!((c.transition_ns(10e6) - 21e9).abs() < 1.0);
    }

    #[test]
    fn a_target_set_mid_transition_waits_for_the_rate_to_come_to_rest() {
        let mut plan = LatencyPlan::new(config(), 0, 30_000_000).expect("valid");
        plan.set_target(31_000_000).expect("valid");
        let mut rates = Vec::new();
        for k in 0..450 {
            if k == 10 {
                plan.set_target(29_000_000).expect("valid");
            }
            rates.push(plan.next_chunk());
        }
        assert!(plan.is_settled());
        let peak = rates.iter().map(|c| c.offset_ns).fold(f64::MIN, f64::max);
        assert!(
            (peak - 31e6).abs() < 1.0,
            "the first target is reached first"
        );
        assert!((rates.last().expect("ran").offset_ns - 29e6).abs() < 1.0);
    }

    #[test]
    fn catmull_rom_reproduces_a_line_and_passes_through_samples() {
        for t in [0.0, 0.25, 0.5, 0.999] {
            assert!((catmull_rom(1.0, 2.0, 3.0, 4.0, t) - (2.0 + t)).abs() < 1e-12);
        }
        assert_eq!(catmull_rom(9.0, -3.0, 5.0, 1.0, 0.0), -3.0);
    }

    #[test]
    fn the_resampler_renders_positions_and_asks_for_what_it_lacks() {
        let c = config();
        let mut plan = LatencyPlan::new(c, 0, 30_000_000).expect("valid");
        plan.set_target(35_000_000).expect("valid");
        let mut rs = CubicResampler::new(2);
        let mut fed = 0u64;
        let mut out = Vec::new();
        for _ in 0..300 {
            let chunk = plan.next_chunk();
            if chunk.source_frames_needed() > fed {
                assert!(matches!(
                    rs.render(&chunk, &mut out),
                    Err(PlanError::NeedSource { .. })
                ));
            }
            while fed < chunk.source_frames_needed() {
                // Channel 0 carries the frame index, channel 1 its negative.
                let block: Vec<f64> = (fed..fed + 960)
                    .flat_map(|i| [i as f64, -(i as f64)])
                    .collect();
                rs.push(&block);
                fed += 960;
            }
            rs.render(&chunk, &mut out).expect("enough source");
            for i in 0..chunk.frames {
                let want = chunk.source_position(i).max(0.0);
                assert!((out[2 * i as usize] - want).abs() < 1e-6);
                assert!((out[2 * i as usize + 1] + want).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn a_bad_configuration_is_refused() {
        let mut c = config();
        c.max_rate_deviation = 0.02;
        assert!(LatencyPlan::new(c, 0, 0).is_err());
        let mut c = config();
        c.ramp_frames = 0;
        assert!(c.validate().is_err());
        assert!(LatencyPlan::new(config(), 0, -1).is_err());
    }
}
