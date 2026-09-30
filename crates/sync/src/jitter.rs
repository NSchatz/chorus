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

/// Largest delay one packet can take inside a burst, in microseconds.
///
/// The real client refuses an exchange whose round trip exceeds 100 ms
/// (`max_rtt_us` in `config/sync.conf`), so a burst delay is capped at half of
/// that per direction: every exchange this model produces is one the client
/// would admit. What happens to exchanges it would refuse is not modelled.
pub const BURST_CAP_US: f64 = 50_000.0;

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
    /// A two-state (Gilbert-Elliott style) link: a quiet state where delay is
    /// exponential with `mean_us` (capped as [`JitterModel::Exponential`] is),
    /// and a burst state where it is Lomax (Pareto type II), heavy tailed,
    /// capped at [`BURST_CAP_US`]. The state persists from packet to packet and
    /// changes with the given per-packet probabilities, so bursts span
    /// consecutive packets and, at a long enough mean length, consecutive
    /// exchanges. A Wi-Fi link: retries, contention and the access point's
    /// own scheduling come in runs rather than one packet at a time.
    Burst {
        /// Mean added delay in the quiet state, in microseconds.
        mean_us: f64,
        /// Per-packet probability of entering a burst from the quiet state.
        enter_prob: f64,
        /// Per-packet probability of leaving a burst.
        exit_prob: f64,
        /// Lomax scale of the burst state's delay, in microseconds.
        burst_scale_us: f64,
        /// Lomax shape (tail index) of the burst state's delay. Smaller is
        /// heavier; at or below 1 the uncapped mean would be infinite.
        burst_shape: f64,
    },
}

impl JitterModel {
    /// One delay sample, in nanoseconds, from a memoryless model. Never
    /// negative.
    ///
    /// A [`JitterModel::Burst`] has state, so it is sampled through a
    /// [`JitterProcess`]; handed to this function directly it is sampled as
    /// if it were always in its quiet state.
    pub fn sample_ns(&self, rng: &mut Rng) -> f64 {
        match *self {
            JitterModel::None => 0.0,
            JitterModel::Uniform { max_us } => rng.next_f64() * max_us * 1000.0,
            JitterModel::Exponential { mean_us } => exponential_ns(mean_us, rng),
            JitterModel::Burst { mean_us, .. } => exponential_ns(mean_us, rng),
        }
    }

    /// The scale parameter, in microseconds, whatever the shape is called.
    pub fn scale_us(&self) -> f64 {
        match *self {
            JitterModel::None => 0.0,
            JitterModel::Uniform { max_us } => max_us,
            JitterModel::Exponential { mean_us } => mean_us,
            JitterModel::Burst { mean_us, .. } => mean_us,
        }
    }

    /// Stable name, as it appears in a scenario file.
    pub fn name(&self) -> &'static str {
        match *self {
            JitterModel::None => "none",
            JitterModel::Uniform { .. } => "uniform",
            JitterModel::Exponential { .. } => "exponential",
            JitterModel::Burst { .. } => "burst",
        }
    }

    /// Build a memoryless model from a scenario file's name and scale.
    ///
    /// `burst` is not built here: it needs four more parameters, and the
    /// scenario reader builds it from its own keys.
    pub fn from_name(name: &str, scale_us: f64) -> Option<JitterModel> {
        match name {
            "none" => Some(JitterModel::None),
            "uniform" => Some(JitterModel::Uniform { max_us: scale_us }),
            "exponential" => Some(JitterModel::Exponential { mean_us: scale_us }),
            _ => None,
        }
    }
}

/// Inverse-transform exponential draw with mean `mean_us`, capped at
/// [`TAIL_CAP_MULTIPLE`] times the mean, in nanoseconds.
fn exponential_ns(mean_us: f64, rng: &mut Rng) -> f64 {
    // next_f64() is in [0, 1), so 1 - it is in (0, 1] and the logarithm is
    // always finite.
    let u = 1.0 - rng.next_f64();
    let sample_us = -mean_us * u.ln();
    let cap_us = mean_us * TAIL_CAP_MULTIPLE;
    sample_us.min(cap_us) * 1000.0
}

/// Inverse-transform Lomax draw, capped at [`BURST_CAP_US`], in nanoseconds.
///
/// Lomax is the Pareto distribution shifted to start at zero:
/// `scale * (u^(-1/shape) - 1)` for `u` uniform on (0, 1].
fn lomax_ns(scale_us: f64, shape: f64, rng: &mut Rng) -> f64 {
    let u = 1.0 - rng.next_f64();
    let sample_us = scale_us * (u.powf(-1.0 / shape) - 1.0);
    sample_us.min(BURST_CAP_US) * 1000.0
}

/// A jitter model plus the state a bursty one carries between packets.
///
/// Every model is sampled through this in the simulator. For the memoryless
/// models it draws exactly what [`JitterModel::sample_ns`] draws, one value
/// per packet, so a scenario written before the burst model existed produces
/// the identical stream. A burst model draws two per packet, in this order:
/// the state transition, then the delay.
#[derive(Debug, Clone)]
pub struct JitterProcess {
    model: JitterModel,
    in_burst: bool,
}

impl JitterProcess {
    /// A process over `model`, starting in the quiet state.
    pub fn new(model: JitterModel) -> JitterProcess {
        JitterProcess {
            model,
            in_burst: false,
        }
    }

    /// Whether the last packet was delayed in the burst state.
    pub fn in_burst(&self) -> bool {
        self.in_burst
    }

    /// One packet's delay, in nanoseconds. Never negative.
    pub fn sample_ns(&mut self, rng: &mut Rng) -> f64 {
        match self.model {
            JitterModel::Burst {
                mean_us,
                enter_prob,
                exit_prob,
                burst_scale_us,
                burst_shape,
            } => {
                let transition = rng.next_f64();
                if self.in_burst {
                    if transition < exit_prob {
                        self.in_burst = false;
                    }
                } else if transition < enter_prob {
                    self.in_burst = true;
                }
                if self.in_burst {
                    lomax_ns(burst_scale_us, burst_shape, rng)
                } else {
                    exponential_ns(mean_us, rng)
                }
            }
            model => model.sample_ns(rng),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{JitterModel, JitterProcess, BURST_CAP_US, TAIL_CAP_MULTIPLE};
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

    fn wifi_like() -> JitterModel {
        JitterModel::Burst {
            mean_us: 400.0,
            enter_prob: 0.05,
            exit_prob: 0.25,
            burst_scale_us: 3_000.0,
            burst_shape: 1.5,
        }
    }

    #[test]
    fn a_memoryless_process_draws_what_the_model_draws() {
        // The property that keeps every committed vector byte-identical.
        for model in [
            JitterModel::None,
            JitterModel::Uniform { max_us: 60.0 },
            JitterModel::Exponential { mean_us: 150.0 },
        ] {
            let mut a = Rng::new(77);
            let mut b = Rng::new(77);
            let mut process = JitterProcess::new(model);
            for _ in 0..1000 {
                assert_eq!(process.sample_ns(&mut a), model.sample_ns(&mut b));
            }
            assert_eq!(a.next_u64(), b.next_u64(), "the same number of draws");
        }
    }

    #[test]
    fn bursts_come_in_runs_and_have_a_heavy_capped_tail() {
        let mut process = JitterProcess::new(wifi_like());
        let mut rng = Rng::new(5);
        let draws = 200_000;
        let mut in_burst = 0u32;
        let mut runs = 0u32;
        let mut previous = false;
        let mut quiet_peak: f64 = 0.0;
        let mut burst_peak: f64 = 0.0;
        for _ in 0..draws {
            let sample = process.sample_ns(&mut rng);
            assert!(sample >= 0.0);
            assert!(sample <= BURST_CAP_US * 1000.0);
            if process.in_burst() {
                in_burst += 1;
                burst_peak = burst_peak.max(sample);
                if !previous {
                    runs += 1;
                }
            } else {
                quiet_peak = quiet_peak.max(sample);
            }
            previous = process.in_burst();
        }
        // Stationary share of the burst state: enter / (enter + exit) = 1/6.
        let share = in_burst as f64 / draws as f64;
        assert!(share > 0.15 && share < 0.18, "burst share {}", share);
        // Mean run length 1 / exit = 4 packets.
        let mean_run = in_burst as f64 / runs as f64;
        assert!(mean_run > 3.7 && mean_run < 4.3, "mean run {}", mean_run);
        assert!(quiet_peak <= 400.0 * TAIL_CAP_MULTIPLE * 1000.0);
        assert!(
            burst_peak > 10.0 * quiet_peak,
            "the burst tail ({} ns) is not heavier than the quiet one ({} ns)",
            burst_peak,
            quiet_peak
        );
    }

    #[test]
    fn a_burst_process_reproduces_its_stream() {
        let mut a = JitterProcess::new(wifi_like());
        let mut b = JitterProcess::new(wifi_like());
        let mut ra = Rng::new(99);
        let mut rb = Rng::new(99);
        for _ in 0..1000 {
            assert_eq!(a.sample_ns(&mut ra), b.sample_ns(&mut rb));
        }
    }
}
