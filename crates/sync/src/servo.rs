//! Offset filtering and the two-tier correction law.
//!
//! Both pieces come from BRIEF.md 5.3: prefer the minimum-RTT sample in a
//! sliding window, smooth it lightly, then correct with a proportional plus
//! integral law clamped to a few hundred ppm, with a hard resync tier for when
//! the error is too large to slew away.
//!
//! One thing here is not in BRIEF.md, and the simulator is what found it. A
//! sliding window hands back an offset that was measured up to a window ago,
//! and the offset between two crystals is moving the whole time, so a stale
//! sample is wrong by the relative skew times its age. At the reference
//! cadence of one exchange per second and an eight deep window, that term
//! alone was the dominant error in every scenario: 872 us of the modelled
//! playout error at 100 ppm relative skew, against jitter contributions of
//! tens of microseconds. The filter therefore projects the sample it selects
//! forward to now, using a drift rate it estimates from the offsets
//! themselves. The measurement and the reasoning are in
//! `docs/decisions/0006-sync-simulator-and-servo.md`.

/// Largest offset drift the filter will believe, in ppm.
///
/// Two crystals at the extremes of the modelled range are 2000 ppm apart.
/// Past that an apparent drift is a measurement artefact rather than a
/// crystal, and believing it would let one unlucky pair of samples throw the
/// estimate a long way.
pub const MAX_TRACKED_DRIFT_PPM: f64 = 2_500.0;

/// Sliding window of offset estimates, filtered by minimum round trip time
/// and projected forward to the present.
#[derive(Debug, Clone)]
pub struct OffsetFilter {
    capacity: usize,
    alpha: f64,
    window: Vec<Sample>,
    /// The estimate and the client time it is valid at.
    smoothed: Option<(f64, f64)>,
    /// Nanoseconds of offset per nanosecond of client time.
    drift: f64,
    /// The sample the last `push` selected out of the window.
    selected: Option<Sample>,
}

/// One exchange in the window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    /// Client time the exchange completed at, in nanoseconds.
    pub at_ns: f64,
    /// Round trip time of the exchange, in nanoseconds.
    pub rtt_ns: f64,
    /// Offset estimate the exchange produced, in nanoseconds.
    pub offset_ns: f64,
}

impl OffsetFilter {
    /// A filter over `capacity` exchanges, smoothing with the given weight.
    ///
    /// `capacity` is clamped up to 1: a window of nothing is not a window.
    pub fn new(capacity: usize, alpha: f64) -> OffsetFilter {
        OffsetFilter {
            capacity: capacity.max(1),
            alpha,
            window: Vec::new(),
            smoothed: None,
            drift: 0.0,
            selected: None,
        }
    }

    /// Add one exchange and return the filtered offset estimate, as of
    /// `at_ns` on the client clock.
    pub fn push(&mut self, at_ns: f64, rtt_ns: f64, offset_ns: f64) -> f64 {
        if self.window.len() == self.capacity {
            self.window.remove(0);
        }
        self.window.push(Sample {
            at_ns,
            rtt_ns,
            offset_ns,
        });

        self.drift = self.estimate_drift();

        // The least queued exchange in the window is the most trustworthy
        // measurement, and it is also usually not the most recent one, so it
        // is projected forward to now before it is used.
        let best = best_of(&self.window);
        self.selected = Some(best);
        let aged = best.offset_ns + self.drift * (at_ns - best.at_ns);

        // Smoothing a quantity that is itself moving would reintroduce the
        // same staleness through the back door, so the previous estimate is
        // carried forward at the drift rate before it is blended.
        let next = match self.smoothed {
            None => aged,
            Some((previous, previous_at)) => {
                let predicted = previous + self.drift * (at_ns - previous_at);
                predicted + self.alpha * (aged - predicted)
            }
        };
        self.smoothed = Some((next, at_ns));
        next
    }

    /// Estimated rate at which the offset is moving, in ppm.
    ///
    /// Taken from the offset samples alone. It is deliberately **not** taken
    /// from the servo's current correction, which is the same quantity and
    /// would be much less noisy: using it would close a loop through the
    /// servo with gain `kp * age / interval`, which at the reference gains
    /// and window is greater than one, and positive.
    pub fn drift_ppm(&self) -> f64 {
        self.drift * 1e6
    }

    /// The current filtered estimate, if any exchange has happened.
    pub fn estimate(&self) -> Option<f64> {
        self.smoothed.map(|(value, _)| value)
    }

    /// The sample out of the window the last [`OffsetFilter::push`] selected.
    ///
    /// The estimate is that sample's offset, projected forward and smoothed, so
    /// this is the exchange the offset in use came FROM. A caller publishing
    /// the error bound on its offset needs that exchange's round trip and not
    /// the newest one's, which is why this is exposed rather than left for a
    /// caller to guess at by repeating the selection rule.
    pub fn selected(&self) -> Option<Sample> {
        self.selected
    }

    /// Forget everything. Used when a hard resync makes the history moot.
    pub fn reset(&mut self) {
        self.window.clear();
        self.smoothed = None;
        self.drift = 0.0;
        self.selected = None;
    }

    /// Slope between the least queued sample of the older half of the window
    /// and the least queued sample of the newer half.
    ///
    /// Zero until there are enough samples for the two halves to be a
    /// baseline worth measuring across.
    fn estimate_drift(&self) -> f64 {
        if self.window.len() < 4 {
            return 0.0;
        }
        let middle = self.window.len() / 2;
        let older = best_of(&self.window[..middle]);
        let newer = best_of(&self.window[middle..]);
        let span_ns = newer.at_ns - older.at_ns;
        if span_ns <= 0.0 {
            return 0.0;
        }
        let drift = (newer.offset_ns - older.offset_ns) / span_ns;
        drift.clamp(
            -MAX_TRACKED_DRIFT_PPM * 1e-6,
            MAX_TRACKED_DRIFT_PPM * 1e-6,
        )
    }
}

/// The least queued sample of a non-empty slice.
fn best_of(samples: &[Sample]) -> Sample {
    let mut best = samples[0];
    for sample in &samples[1..] {
        if sample.rtt_ns < best.rtt_ns {
            best = *sample;
        }
    }
    best
}

/// Gains and limits of the correction law.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ServoConfig {
    /// Proportional gain against the normalised error.
    pub kp: f64,
    /// Integral gain. This is the term that learns the constant relative skew.
    pub ki: f64,
    /// Largest rate correction, in ppm. BRIEF.md 5.3's reference constant.
    pub max_correction_ppm: f64,
    /// Error at or above which the servo stops slewing and steps.
    pub hard_resync_threshold_ns: f64,
    /// Exchanges in the minimum-RTT window.
    pub filter_window: usize,
    /// Weight of a new sample in the exponential smoother.
    pub smoothing_alpha: f64,
}

impl Default for ServoConfig {
    fn default() -> ServoConfig {
        ServoConfig {
            kp: 0.4,
            ki: 0.08,
            max_correction_ppm: 500.0,
            hard_resync_threshold_ns: 3_000_000.0,
            filter_window: 8,
            smoothing_alpha: 0.25,
        }
    }
}

/// What the servo decided to do about an error.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ServoAction {
    /// Slew: apply this rate correction, in ppm, until the next exchange.
    Fine {
        /// The correction to apply, already clamped.
        correction_ppm: f64,
    },
    /// Step: the error is too large to slew away in reasonable time.
    HardResync {
        /// How far the playout pointer moves, in nanoseconds.
        step_ns: f64,
    },
}

/// The correction law.
#[derive(Debug, Clone)]
pub struct Servo {
    config: ServoConfig,
    integral_ppm: f64,
    correction_ppm: f64,
    hard_resyncs: u32,
    updates: u32,
    clamped: bool,
}

impl Servo {
    /// A servo at rest with the given gains.
    pub fn new(config: ServoConfig) -> Servo {
        Servo {
            config,
            integral_ppm: 0.0,
            correction_ppm: 0.0,
            hard_resyncs: 0,
            updates: 0,
            clamped: false,
        }
    }

    /// The correction currently being applied, in ppm.
    pub fn correction_ppm(&self) -> f64 {
        self.correction_ppm
    }

    /// Whether the last fine correction was cut down by the clamp.
    ///
    /// The clamped value is what [`Servo::update`] returns and what a caller
    /// applies; this says the raw value was larger, so a caller can report that
    /// the excess was discarded rather than leave the difference invisible. A
    /// hard resync leaves this false: nothing was clamped, the tier changed.
    pub fn last_correction_was_clamped(&self) -> bool {
        self.clamped
    }

    /// How many times this servo has had to step rather than slew.
    pub fn hard_resyncs(&self) -> u32 {
        self.hard_resyncs
    }

    /// How many exchanges this servo has acted on.
    pub fn updates(&self) -> u32 {
        self.updates
    }

    /// React to an observed playout error.
    ///
    /// `error_ns` is positive when playout is ahead of the estimated server
    /// timeline. `interval_s` is the time until the next exchange, which is
    /// what the error is normalised against so that the loop behaves the same
    /// at any cadence.
    pub fn update(&mut self, error_ns: f64, interval_s: f64) -> ServoAction {
        self.updates += 1;

        if error_ns.abs() >= self.config.hard_resync_threshold_ns {
            self.integral_ppm = 0.0;
            self.correction_ppm = 0.0;
            self.clamped = false;
            self.hard_resyncs += 1;
            return ServoAction::HardResync { step_ns: -error_ns };
        }

        // The ppm that would produce this error over one interval.
        let normalised_ppm = error_ns / (interval_s * 1000.0);
        self.integral_ppm += normalised_ppm;

        let raw = -(self.config.kp * normalised_ppm + self.config.ki * self.integral_ppm);
        let clamped = raw.clamp(
            -self.config.max_correction_ppm,
            self.config.max_correction_ppm,
        );
        self.clamped = clamped != raw;
        if self.clamped {
            // Anti-windup: while the output is pinned, the integral does not
            // get to keep charging.
            self.integral_ppm -= normalised_ppm;
        }

        self.correction_ppm = clamped;
        ServoAction::Fine {
            correction_ppm: clamped,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{OffsetFilter, Servo, ServoAction, ServoConfig};


    /// One second of client time, the reference exchange cadence.
    const TICK: f64 = 1e9;

    #[test]
    fn the_filter_prefers_the_least_queued_exchange() {
        let mut filter = OffsetFilter::new(4, 1.0);
        filter.push(TICK, 900_000.0, 5_000.0);
        filter.push(2.0 * TICK, 300_000.0, 1_000.0);
        let estimate = filter.push(3.0 * TICK, 1_500_000.0, 90_000.0);
        assert_eq!(
            estimate, 1_000.0,
            "the 300 us round trip is the trustworthy sample"
        );
        assert_eq!(filter.drift_ppm(), 0.0, "three samples is not a baseline");
    }

    #[test]
    fn the_filter_says_which_exchange_the_estimate_came_from() {
        // A caller that has to publish the error bound on its offset needs the
        // round trip of the SELECTED sample, which here is neither the newest
        // nor the oldest.
        let mut filter = OffsetFilter::new(4, 1.0);
        assert_eq!(filter.selected(), None, "nothing has been pushed");
        filter.push(TICK, 900_000.0, 5_000.0);
        filter.push(2.0 * TICK, 300_000.0, 1_000.0);
        filter.push(3.0 * TICK, 1_500_000.0, 90_000.0);
        let selected = filter.selected().expect("three exchanges happened");
        assert_eq!(selected.rtt_ns, 300_000.0);
        assert_eq!(selected.offset_ns, 1_000.0);
        assert_eq!(selected.at_ns, 2.0 * TICK);
        filter.reset();
        assert_eq!(filter.selected(), None, "a reset forgets the selection too");
    }

    #[test]
    fn the_servo_says_when_it_clamped() {
        let mut servo = Servo::new(ServoConfig::default());
        servo.update(50_000.0, 1.0);
        assert!(!servo.last_correction_was_clamped());
        servo.update(2_999_999.0, 1.0);
        assert!(servo.last_correction_was_clamped());
        // A hard resync clamps nothing: the tier changed.
        servo.update(12_345_000.0, 1.0);
        assert!(!servo.last_correction_was_clamped());
    }

    #[test]
    fn the_filter_window_slides() {
        let mut filter = OffsetFilter::new(2, 1.0);
        filter.push(TICK, 100.0, 1.0);
        filter.push(2.0 * TICK, 200.0, 2.0);
        // The 100 ns sample falls out of a two-deep window here.
        let estimate = filter.push(3.0 * TICK, 300.0, 3.0);
        assert_eq!(estimate, 2.0);
    }

    #[test]
    fn smoothing_moves_part_of_the_way() {
        let mut filter = OffsetFilter::new(1, 0.25);
        assert_eq!(filter.push(TICK, 100.0, 0.0), 0.0);
        assert_eq!(filter.push(2.0 * TICK, 100.0, 100.0), 25.0);
        assert_eq!(filter.push(3.0 * TICK, 100.0, 100.0), 43.75);
    }

    #[test]
    fn a_stale_sample_is_projected_forward_to_now() {
        // Two crystals 100 ppm apart, so the offset between them walks 100 us
        // every second, and every exchange is equally queued so the
        // minimum-RTT rule keeps selecting the oldest sample in the window.
        // That is the worst case for staleness: without projection the filter
        // would report a value up to seven seconds, and 700 us, out of date.
        let mut filter = OffsetFilter::new(8, 0.5);
        let drift_ns_per_s = 100.0 * 1_000.0;
        let offset_at = |exchange: i32| 10_000.0 + drift_ns_per_s * exchange as f64;

        for exchange in 1..=16 {
            filter.push(exchange as f64 * TICK, 900_000.0, offset_at(exchange));
        }

        let truth = offset_at(16);
        let estimate = filter.estimate().expect("sixteen exchanges happened");
        assert!(
            (estimate - truth).abs() < 1_000.0,
            "estimate {} ns against a true offset of {} ns",
            estimate,
            truth
        );
        assert!(
            (filter.drift_ppm() - 100.0).abs() < 1.0,
            "drift estimated at {} ppm against a true 100 ppm",
            filter.drift_ppm()
        );
    }

    #[test]
    fn an_absurd_apparent_drift_is_not_believed() {
        let mut filter = OffsetFilter::new(4, 1.0);
        for exchange in 1..=4 {
            // A microsecond apart, metres of offset apart: nothing about that
            // is a crystal.
            filter.push(exchange as f64 * 1_000.0, 100.0, exchange as f64 * 1e9);
        }
        assert!(
            filter.drift_ppm().abs() <= super::MAX_TRACKED_DRIFT_PPM,
            "drift of {} ppm was believed",
            filter.drift_ppm()
        );
    }

    #[test]
    fn a_large_error_steps_instead_of_slewing() {
        let mut servo = Servo::new(ServoConfig::default());
        let action = servo.update(12_345_000.0, 1.0);
        assert_eq!(
            action,
            ServoAction::HardResync {
                step_ns: -12_345_000.0
            }
        );
        assert_eq!(servo.hard_resyncs(), 1);
        assert_eq!(servo.correction_ppm(), 0.0);
    }

    #[test]
    fn a_small_error_is_slewed_against() {
        let mut servo = Servo::new(ServoConfig::default());
        match servo.update(50_000.0, 1.0) {
            ServoAction::Fine { correction_ppm } => {
                // 50 us over 1 s is 50 ppm; kp 0.4 and ki 0.08 of the first
                // integral step give -24 ppm, and the sign opposes the error.
                assert!(correction_ppm < 0.0);
                assert!((correction_ppm + 24.0).abs() < 1e-9, "{}", correction_ppm);
            }
            other => panic!("expected a fine correction, got {:?}", other),
        }
        assert_eq!(servo.hard_resyncs(), 0);
    }

    #[test]
    fn the_correction_is_clamped() {
        let mut servo = Servo::new(ServoConfig::default());
        // Just under the hard resync threshold, so it stays in the fine tier
        // and demands far more than the clamp allows.
        match servo.update(2_999_999.0, 1.0) {
            ServoAction::Fine { correction_ppm } => {
                assert_eq!(correction_ppm, -500.0)
            }
            other => panic!("expected a fine correction, got {:?}", other),
        }
    }

    #[test]
    fn the_integral_learns_a_constant_skew() {
        // Feed the servo the error a constant 50 ppm skew produces each
        // interval and check that the correction converges on cancelling it.
        let mut servo = Servo::new(ServoConfig::default());
        let skew_ppm = 50.0;
        let interval_s = 1.0;
        let mut error_ns = 0.0;
        for _ in 0..200 {
            let action = servo.update(error_ns, interval_s);
            let correction = match action {
                ServoAction::Fine { correction_ppm } => correction_ppm,
                ServoAction::HardResync { .. } => panic!("no step should be needed"),
            };
            error_ns += (skew_ppm + correction) * interval_s * 1000.0;
        }
        assert!(
            (servo.correction_ppm() + skew_ppm).abs() < 0.5,
            "correction settled at {} ppm against a {} ppm skew",
            servo.correction_ppm(),
            skew_ppm
        );
        assert!(
            error_ns.abs() < 1_000.0,
            "residual error {} ns",
            error_ns
        );
    }

    #[test]
    fn the_loop_does_not_diverge_at_the_worst_realistic_skew() {
        let mut servo = Servo::new(ServoConfig::default());
        let skew_ppm = 100.0;
        let mut error_ns = 0.0;
        let mut peak: f64 = 0.0;
        for _ in 0..500 {
            let correction = match servo.update(error_ns, 1.0) {
                ServoAction::Fine { correction_ppm } => correction_ppm,
                ServoAction::HardResync { .. } => panic!("no step should be needed"),
            };
            error_ns += (skew_ppm + correction) * 1000.0;
            peak = peak.max(error_ns.abs());
        }
        assert!(
            peak < 200_000.0,
            "peak excursion {} ns is larger than the model expects",
            peak
        );
    }
}
