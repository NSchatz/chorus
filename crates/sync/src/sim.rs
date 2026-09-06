//! The simulator: two virtual clocks, a network, an exchange, a servo, and
//! the modelled playout-error series that comes out.
//!
//! What is modelled and what is deliberately not is written down in
//! `docs/decisions/0006-sync-simulator-and-servo.md`. The short version: this
//! keeps servo logic honest in CI. It does not predict a number. The number
//! comes from the measurement rig on real hardware.

use crate::config::{ConfigError, SimConfig, SERVER_TURNAROUND_NS};
use crate::rng::Rng;
use crate::servo::{OffsetFilter, Sample, Servo, ServoAction};

/// One sample of the modelled playout error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayoutSample {
    /// True time since the start of the run, in nanoseconds.
    pub t_ns: u64,
    /// Playout position minus the true server timeline, in nanoseconds.
    ///
    /// Positive means playout is ahead. No participant in the model can
    /// observe this; it is ground truth for the assertion, which is the whole
    /// reason a simulator is worth having.
    pub error_ns: i64,
}

/// Everything one run produced.
#[derive(Debug, Clone, PartialEq)]
pub struct SimResult {
    /// The modelled playout-error series, one sample per step.
    pub samples: Vec<PlayoutSample>,
    /// Time sync exchanges the client ran.
    pub exchanges: u32,
    /// Times the servo had to step rather than slew.
    pub hard_resyncs: u32,
    /// The rate correction in force when the run ended, in ppm.
    pub final_correction_ppm: f64,
}

impl SimResult {
    /// Index of the first sample from which the error stays inside
    /// `bound_ns` for the whole rest of the run.
    ///
    /// `None` means it never does, which is the failure this exists to catch:
    /// converging once and drifting out again later is not converging.
    pub fn settle_index(&self, bound_ns: i64) -> Option<usize> {
        let mut settle = 0usize;
        for (index, sample) in self.samples.iter().enumerate() {
            if sample.error_ns.saturating_abs() >= bound_ns {
                settle = index + 1;
            }
        }
        if settle >= self.samples.len() {
            None
        } else {
            Some(settle)
        }
    }

    /// True time at which the error came inside `bound_ns` for good.
    pub fn settle_time_ns(&self, bound_ns: i64) -> Option<u64> {
        self.settle_index(bound_ns)
            .map(|index| self.samples[index].t_ns)
    }

    /// Largest absolute error from `index` onwards.
    pub fn max_abs_error_after(&self, index: usize) -> i64 {
        self.samples[index.min(self.samples.len())..]
            .iter()
            .map(|s| s.error_ns.saturating_abs())
            .max()
            .unwrap_or(0)
    }

    /// Largest absolute error anywhere in the run.
    pub fn max_abs_error(&self) -> i64 {
        self.max_abs_error_after(0)
    }

    /// Whether the error came inside `bound_ns` by `deadline_ns` and stayed
    /// there for the rest of the run.
    pub fn holds_below(&self, bound_ns: i64, deadline_ns: u64) -> bool {
        match self.settle_time_ns(bound_ns) {
            Some(settled) => settled <= deadline_ns,
            None => false,
        }
    }
}

/// What one time sync exchange did, from the inputs to the servo's decision.
///
/// The playout-error series says whether the loop converged; this says HOW,
/// which is what a second implementation of the same arithmetic has to be held
/// to. A C mirror that converged by a different route would agree with
/// [`SimResult`] and disagree here, and disagreeing here is what the committed
/// cross-check vectors under `fixtures/sync/crosscheck/` exist to catch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExchangeRecord {
    /// Zero-based index of the exchange in the run.
    pub index: u32,
    /// Client time the exchange completed at, in nanoseconds.
    pub at_ns: f64,
    /// Round trip this exchange measured.
    pub rtt_ns: f64,
    /// The offset this one exchange estimated, before filtering.
    pub offset_estimate_ns: f64,
    /// What the filter returned once it had this exchange.
    pub filtered_offset_ns: f64,
    /// The sample the filter SELECTED out of its window for this exchange.
    pub selected: Sample,
    /// What the servo decided.
    pub action: ServoAction,
}

/// Run one simulation.
///
/// Refuses a configuration outside the modelled ranges rather than reporting a
/// playout-error result for it.
pub fn run(config: &SimConfig) -> Result<SimResult, ConfigError> {
    run_recorded(config).map(|(result, _)| result)
}

/// The same run, with one [`ExchangeRecord`] per time sync exchange.
///
/// [`run`] is this function with the records dropped, so there is one loop and
/// not two: a recording path that drifted from the graded one would be a
/// cross-check against something nothing runs.
pub fn run_recorded(config: &SimConfig) -> Result<(SimResult, Vec<ExchangeRecord>), ConfigError> {
    config.validate()?;

    let mut rng = Rng::new(config.seed);
    let mut filter = OffsetFilter::new(config.servo.filter_window, config.servo.smoothing_alpha);
    let mut servo = Servo::new(config.servo);

    let steps = config.steps();
    let step_ns = (config.step_ms as f64) * 1_000_000.0;
    let sync_interval_ns = (config.sync_interval_ms as f64) * 1_000_000.0;
    let interval_s = (config.sync_interval_ms as f64) / 1_000.0;
    let base_delay_ns = config.base_one_way_delay_us * 1_000.0;

    let server_rate = 1.0 + config.server_ppm * 1e-6;
    let client_rate = 1.0 + config.client_ppm * 1e-6;
    let epoch_offset_ns = config.initial_offset_ns as f64;

    // The client starts playing out on its own clock, knowing nothing about
    // the server timeline. Acquiring it is the servo's first job.
    let mut playout_ns = epoch_offset_ns;
    let mut correction_ppm = 0.0f64;
    let mut next_sync_ns = sync_interval_ns;
    let mut exchanges = 0u32;

    let mut samples = Vec::with_capacity(steps as usize);
    let mut records: Vec<ExchangeRecord> = Vec::new();

    for step in 0..steps {
        let t_ns = (step + 1) as f64 * step_ns;

        // Playout advances at the client's crystal rate, plus whatever the
        // servo is currently correcting by.
        playout_ns += step_ns * (1.0 + (config.client_ppm + correction_ppm) * 1e-6);

        while t_ns >= next_sync_ns {
            let client_now_ns = t_ns * client_rate + epoch_offset_ns;
            let forward_ns = base_delay_ns + config.jitter.sample_ns(&mut rng);
            let return_ns = base_delay_ns + config.jitter.sample_ns(&mut rng);

            // RFC 5905 section 8. Every timestamp is taken on the clock of the
            // device that took it.
            let t0 = client_now_ns;
            let t1 = (t_ns + forward_ns) * server_rate;
            let t2 = (t_ns + forward_ns + SERVER_TURNAROUND_NS) * server_rate;
            let t3 = (t_ns + forward_ns + SERVER_TURNAROUND_NS + return_ns) * client_rate
                + epoch_offset_ns;

            let offset_estimate = ((t1 - t0) + (t2 - t3)) / 2.0;
            let rtt = (t3 - t0) - (t2 - t1);
            let filtered_offset = filter.push(client_now_ns, rtt, offset_estimate);

            // What the client believes the server timeline reads right now.
            let server_estimate_ns = client_now_ns + filtered_offset;
            let observed_error_ns = playout_ns - server_estimate_ns;

            let action = servo.update(observed_error_ns, interval_s);

            records.push(ExchangeRecord {
                index: exchanges,
                at_ns: client_now_ns,
                rtt_ns: rtt,
                offset_estimate_ns: offset_estimate,
                filtered_offset_ns: filtered_offset,
                selected: filter
                    .selected()
                    .expect("a push just happened, so a sample was selected"),
                action,
            });

            match action {
                ServoAction::Fine {
                    correction_ppm: correction,
                } => correction_ppm = correction,
                ServoAction::HardResync { step_ns: jump } => {
                    playout_ns += jump;
                    correction_ppm = 0.0;
                }
            }

            exchanges += 1;
            next_sync_ns += sync_interval_ns;
        }

        let server_timeline_ns = t_ns * server_rate;
        samples.push(PlayoutSample {
            t_ns: t_ns as u64,
            error_ns: (playout_ns - server_timeline_ns).round() as i64,
        });
    }

    Ok((
        SimResult {
            samples,
            exchanges,
            hard_resyncs: servo.hard_resyncs(),
            final_correction_ppm: servo.correction_ppm(),
        },
        records,
    ))
}

#[cfg(test)]
mod tests {
    use super::run;
    use crate::config::SimConfig;
    use crate::jitter::JitterModel;

    const ONE_MS_NS: i64 = 1_000_000;

    #[test]
    fn a_run_produces_one_sample_per_step() {
        let config = SimConfig {
            duration_ms: 10_000,
            step_ms: 10,
            ..SimConfig::default()
        };
        let result = run(&config).expect("a default configuration is valid");
        assert_eq!(result.samples.len(), 1_000);
        assert_eq!(result.samples[0].t_ns, 10_000_000);
        assert_eq!(result.samples[999].t_ns, 10_000_000_000);
    }

    #[test]
    fn a_run_starts_wrong_and_is_driven_in() {
        let config = SimConfig::default();
        let result = run(&config).expect("valid");
        assert!(
            result.samples[0].error_ns.abs() > ONE_MS_NS,
            "the run starts outside the bound, or it is proving nothing"
        );
        assert!(
            result.holds_below(ONE_MS_NS, 10_000_000_000),
            "settled at {:?}, peak after settling {}",
            result.settle_time_ns(ONE_MS_NS),
            result.max_abs_error_after(result.settle_index(ONE_MS_NS).unwrap_or(0))
        );
        assert_eq!(result.hard_resyncs, 1, "one acquisition, no thrashing after");
    }

    #[test]
    fn the_servo_is_what_holds_the_bound() {
        // The same run with the correction law neutered has to fail, or the
        // regression is asserting nothing.
        let mut config = SimConfig {
            duration_ms: 120_000,
            client_ppm: 40.0,
            server_ppm: 0.0,
            jitter: JitterModel::None,
            ..SimConfig::default()
        };
        config.servo.kp = 0.0;
        config.servo.ki = 0.0;
        // Leave the hard resync tier in place: it acquires the timeline once,
        // and then nothing corrects the 40 ppm drift.
        let result = run(&config).expect("valid");
        assert!(
            !result.holds_below(ONE_MS_NS, 60_000_000_000),
            "an uncorrected 40 ppm skew must drift out of the bound"
        );
        // Not just at the start, where every run is wrong: long after the
        // hard resync tier acquired the timeline, the drift is still there.
        assert!(
            result.max_abs_error_after(6_000) > ONE_MS_NS,
            "peak error in the second minute was only {} ns",
            result.max_abs_error_after(6_000)
        );
    }

    #[test]
    fn the_exchange_cadence_is_honoured() {
        let config = SimConfig {
            duration_ms: 10_000,
            sync_interval_ms: 500,
            ..SimConfig::default()
        };
        let result = run(&config).expect("valid");
        assert_eq!(result.exchanges, 20);
    }

    #[test]
    fn a_clock_pair_with_no_skew_and_no_jitter_holds_exactly() {
        let config = SimConfig {
            duration_ms: 30_000,
            server_ppm: 0.0,
            client_ppm: 0.0,
            initial_offset_ns: 0,
            jitter: JitterModel::None,
            ..SimConfig::default()
        };
        let result = run(&config).expect("valid");
        assert_eq!(
            result.max_abs_error(),
            0,
            "identical clocks on a quiet link need no correction at all"
        );
        assert_eq!(result.hard_resyncs, 0);
    }

    #[test]
    fn a_negative_initial_offset_is_acquired_the_same_way() {
        let config = SimConfig {
            initial_offset_ns: -25_000_000,
            ..SimConfig::default()
        };
        let result = run(&config).expect("valid");
        assert!(result.samples[0].error_ns < -ONE_MS_NS);
        assert!(result.holds_below(ONE_MS_NS, 10_000_000_000));
    }
}
