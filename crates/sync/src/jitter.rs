//! Network delay models.
//!
//! Jitter here is queuing delay, so it is never negative: a packet can be held
//! up, it cannot arrive before it was sent. That sign matters, because it is
//! what makes the minimum-RTT filter in `servo.rs` work at all. The least
//! queued exchange in a window is the one whose forward and return paths came
//! closest to symmetric, so its offset estimate is the most trustworthy one
//! available.

use crate::rng::Rng;

/// Exponential draws are capped at this multiple of the mean.
///
/// A real queue is bounded by its buffer depth. An uncapped exponential draw
/// would occasionally hand the filter a delay no switch on this network could
/// produce, which would be modelling the distribution rather than the network.
pub const TAIL_CAP_MULTIPLE: f64 = 10.0;

/// How delay above the base one-way time is distributed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JitterModel {
    /// No jitter at all. Useful as a control: the servo has only skew to fight.
    None,
    /// Uniform on `[0, max_us]`. A quiet switched segment.
    Uniform {
        /// Largest delay the model adds, in microseconds.
        max_us: f64,
    },
    /// Exponential with the given mean, capped at [`TAIL_CAP_MULTIPLE`] times
    /// it. A loaded link, where most packets sail through and some queue.
    Exponential {
        /// Mean added delay, in microseconds.
        mean_us: f64,
    },
}

impl JitterModel {
    /// One delay sample, in nanoseconds. Never negative.
    pub fn sample_ns(&self, rng: &mut Rng) -> f64 {
        match *self {
            JitterModel::None => 0.0,
            JitterModel::Uniform { max_us } => rng.next_f64() * max_us * 1000.0,
            JitterModel::Exponential { mean_us } => {
                // Inverse transform. next_f64() is in [0, 1), so 1 - it is in
                // (0, 1] and the logarithm is always finite.
                let u = 1.0 - rng.next_f64();
                let sample_us = -mean_us * u.ln();
                let cap_us = mean_us * TAIL_CAP_MULTIPLE;
                sample_us.min(cap_us) * 1000.0
            }
        }
    }

    /// The scale parameter, in microseconds, whatever the shape is called.
    pub fn scale_us(&self) -> f64 {
        match *self {
            JitterModel::None => 0.0,
            JitterModel::Uniform { max_us } => max_us,
            JitterModel::Exponential { mean_us } => mean_us,
        }
    }

    /// Stable name, as it appears in a scenario file.
    pub fn name(&self) -> &'static str {
        match *self {
            JitterModel::None => "none",
            JitterModel::Uniform { .. } => "uniform",
            JitterModel::Exponential { .. } => "exponential",
        }
    }

    /// Build a model from a scenario file's name and scale.
    pub fn from_name(name: &str, scale_us: f64) -> Option<JitterModel> {
        match name {
            "none" => Some(JitterModel::None),
            "uniform" => Some(JitterModel::Uniform { max_us: scale_us }),
            "exponential" => Some(JitterModel::Exponential { mean_us: scale_us }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{JitterModel, TAIL_CAP_MULTIPLE};
    use crate::rng::Rng;

    #[test]
    fn no_jitter_means_no_jitter() {
        let mut rng = Rng::new(1);
        for _ in 0..100 {
            assert_eq!(JitterModel::None.sample_ns(&mut rng), 0.0);
        }
    }

    #[test]
    fn uniform_stays_inside_its_bound() {
        let model = JitterModel::Uniform { max_us: 150.0 };
        let mut rng = Rng::new(9);
        let mut max_seen: f64 = 0.0;
        for _ in 0..10_000 {
            let sample = model.sample_ns(&mut rng);
            assert!(sample >= 0.0, "jitter is queuing delay, never negative");
            assert!(sample <= 150_000.0);
            max_seen = max_seen.max(sample);
        }
        assert!(max_seen > 140_000.0, "the whole range is reachable");
    }

    #[test]
    fn exponential_is_non_negative_and_capped() {
        let model = JitterModel::Exponential { mean_us: 200.0 };
        let cap_ns = 200.0 * TAIL_CAP_MULTIPLE * 1000.0;
        let mut rng = Rng::new(11);
        let mut total = 0.0;
        let draws = 100_000;
        for _ in 0..draws {
            let sample = model.sample_ns(&mut rng);
            assert!(sample >= 0.0);
            assert!(sample <= cap_ns, "{} exceeds the tail cap", sample);
            total += sample;
        }
        let mean_us = total / draws as f64 / 1000.0;
        assert!(
            mean_us > 180.0 && mean_us < 210.0,
            "sampled mean {} us is not near the configured 200 us",
            mean_us
        );
    }

    #[test]
    fn a_model_reproduces_its_stream() {
        let model = JitterModel::Exponential { mean_us: 120.0 };
        let mut a = Rng::new(4242);
        let mut b = Rng::new(4242);
        for _ in 0..1000 {
            assert_eq!(model.sample_ns(&mut a), model.sample_ns(&mut b));
        }
    }
}
