//! Offset filtering and the two-tier correction law.
//!
//! Both pieces come straight from BRIEF.md 5.3: prefer the minimum-RTT sample
//! in a sliding window, smooth it lightly, then correct with a proportional
//! plus integral law clamped to a few hundred ppm, with a hard resync tier for
//! when the error is too large to slew away. The reasoning and the gain
//! derivation are in `docs/decisions/0006-sync-simulator-and-servo.md`.

/// Sliding window of offset estimates, filtered by minimum round trip time.
#[derive(Debug, Clone)]
pub struct OffsetFilter {
    capacity: usize,
    alpha: f64,
    window: Vec<Sample>,
    smoothed: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
struct Sample {
    rtt_ns: f64,
    offset_ns: f64,
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
        }
    }

    /// Add one exchange and return the filtered offset estimate.
    pub fn push(&mut self, rtt_ns: f64, offset_ns: f64) -> f64 {
        if self.window.len() == self.capacity {
            self.window.remove(0);
        }
        self.window.push(Sample { rtt_ns, offset_ns });

        let mut best = self.window[0];
        for sample in &self.window[1..] {
            if sample.rtt_ns < best.rtt_ns {
                best = *sample;
            }
        }

        let next = match self.smoothed {
            None => best.offset_ns,
            Some(previous) => previous + self.alpha * (best.offset_ns - previous),
        };
        self.smoothed = Some(next);
        next
    }

    /// The current filtered estimate, if any exchange has happened.
    pub fn estimate(&self) -> Option<f64> {
        self.smoothed
    }

    /// Forget everything. Used when a hard resync makes the history moot.
    pub fn reset(&mut self) {
        self.window.clear();
        self.smoothed = None;
    }
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
        }
    }

    /// The correction currently being applied, in ppm.
    pub fn correction_ppm(&self) -> f64 {
        self.correction_ppm
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
        if clamped != raw {
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

    #[test]
    fn the_filter_prefers_the_least_queued_exchange() {
        let mut filter = OffsetFilter::new(4, 1.0);
        filter.push(900_000.0, 5_000.0);
        filter.push(300_000.0, 1_000.0);
        let estimate = filter.push(1_500_000.0, 90_000.0);
        assert_eq!(
            estimate, 1_000.0,
            "the 300 us round trip is the trustworthy sample"
        );
    }

    #[test]
    fn the_filter_window_slides() {
        let mut filter = OffsetFilter::new(2, 1.0);
        filter.push(100.0, 1.0);
        filter.push(200.0, 2.0);
        // The 100 ns sample falls out of a two-deep window here.
        let estimate = filter.push(300.0, 3.0);
        assert_eq!(estimate, 2.0);
    }

    #[test]
    fn smoothing_moves_part_of_the_way() {
        let mut filter = OffsetFilter::new(1, 0.25);
        assert_eq!(filter.push(100.0, 0.0), 0.0);
        assert_eq!(filter.push(100.0, 100.0), 25.0);
        assert_eq!(filter.push(100.0, 100.0), 43.75);
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
