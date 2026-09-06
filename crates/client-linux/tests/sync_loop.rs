//! The sync loop, closed over a modelled clock pair, a modelled network and a
//! modelled DAC.
//!
//! Read `tests/common/mod.rs` first for what a modelled device is and is not
//! evidence for. In one line: everything here is a MODELLED result. It says
//! the loop's logic is not wrong in the ways these tests can see. It says
//! nothing about two loudspeakers in a room, which is AC-1, which needs
//! hardware this pipeline has no route to, and which
//! `docs/verification-record.md` records as NOT passed.

mod common;

use std::path::PathBuf;

use chorus_client_linux::sink::{PcmSink, SinkError};
use chorus_client_linux::sync::{
    Correction, DiscardReason, ExchangeVerdict, PlayoutCorrector, SyncConfig, SyncLoop,
    FILTER_WINDOW, HARD_RESYNC_THRESHOLD_US, MAX_CORRECTION_PPM, MAX_RTT_US, MUTE_US,
    PLAYOUT_LATENCY_US, SMOOTHING_ALPHA, STALENESS_LIMIT_MS, SYNC_INTERVAL_MS,
};
use chorus_protocol::TimeSync;
use chorus_sync::JitterModel;

use common::{DacFault, ModelParams, ModelledDac, ModelledEndpoint, VirtualClock};

/// One modelled minute, in nanoseconds.
const MINUTE_NS: u64 = 60_000_000_000;

/// AC-10's bound: half AC-1's 0.5 ms inter-device budget, because two
/// endpoints each within 0.25 ms of one timeline are within 0.5 ms of each
/// other.
const BOUND_NS: f64 = 250_000.0;

fn params() -> ModelParams {
    ModelParams::default()
}

fn sync() -> SyncConfig {
    SyncConfig::default()
}

// -------------------------------------------------------------------------
// AC-2: the offset comes from the minimum-delay sample, not the most recent.
// -------------------------------------------------------------------------

/// Build an exchange with a chosen round trip and a chosen true offset.
///
/// `t0` and `t3` are on the client clock, `t1` and `t2` on the server's. The
/// asymmetry is put entirely in the forward leg, which is what makes an
/// exchange's offset estimate wrong by half its round trip and is exactly why
/// RFC 5905 keeps the least queued sample.
fn exchange_with(t0_ns: u64, rtt_ns: u64, true_offset_ns: i64, forward_share: f64) -> TimeSync {
    let forward = (rtt_ns as f64 * forward_share) as u64;
    let back = rtt_ns - forward;
    let turnaround = 1_000u64;
    let t1 = (t0_ns as i64 + true_offset_ns) as u64 + forward;
    let t2 = t1 + turnaround;
    let t3 = t0_ns + forward + turnaround + back;
    TimeSync {
        t0_ns,
        t1_ns: t1,
        t2_ns: t2,
        t3_ns: t3,
    }
}

#[test]
fn the_offset_in_use_comes_from_the_minimum_delay_sample_and_not_the_newest() {
    // RFC 5905 section 10: "the shift register stages are copied to a
    // temporary list and the list sorted by increasing delta ... Let the first
    // stage offset in the sorted list be theta_0."
    //
    // Three exchanges, all reporting the SAME true offset of 1 000 000 ns. Two
    // of them are badly queued in one direction, so their offset estimates are
    // wrong by half their round trip; the middle one sailed through. The
    // newest is the worst.
    let mut loop_ = SyncLoop::new(SyncConfig {
        filter_window: 8,
        // No smoothing, so what comes out is the selected sample and not a
        // blend of it with its predecessors.
        smoothing_alpha: 1.0,
        ..sync()
    });
    let truth = 1_000_000i64;
    loop_.offer(&exchange_with(1_000_000_000, 900_000, truth, 1.0));
    loop_.offer(&exchange_with(2_000_000_000, 20_000, truth, 0.5));
    loop_.offer(&exchange_with(3_000_000_000, 1_500_000, truth, 0.0));

    let telemetry = loop_.telemetry(3_000_000_000);
    assert_eq!(
        telemetry.round_trip_ns,
        Some(20_000),
        "the offset in use has to come from the 20 us exchange"
    );
    let offset = telemetry.offset_ns.expect("three exchanges were accepted");
    assert!(
        (offset - truth).abs() < 20_000,
        "offset {} ns is not the least queued sample's estimate of {} ns",
        offset,
        truth
    );

    // And the counterfactual, so this asserts something: taking the NEWEST
    // sample would have been wrong by roughly half its 1.5 ms round trip.
    let newest = exchange_with(3_000_000_000, 1_500_000, truth, 0.0);
    assert!(
        (newest.offset_ns() - truth).abs() > 700_000,
        "the newest sample's own estimate is {} ns against a true {} ns, so a filter that took \
         it would be visibly wrong",
        newest.offset_ns(),
        truth
    );
}

// -------------------------------------------------------------------------
// AC-3: the error signal is the device-reported delay to the DAC.
// -------------------------------------------------------------------------

/// A sink whose `write` and whose `delay_frames` disagree by a large, known
/// amount, so an error signal derived from the wrong one is visibly wrong.
///
/// `write` returns instantly and reports every frame accepted, which is what
/// the return of a write call looks like on a device with room in its ring:
/// it says nothing at all about when those frames become audible. The delay
/// this device reports is the only thing that does.
struct DisagreeingSink {
    delay_frames: i64,
    writes: u32,
    frames_written: u64,
}

impl PcmSink for DisagreeingSink {
    fn device(&self) -> &str {
        "disagreeing"
    }
    fn frame_len(&self) -> usize {
        4
    }
    fn rate_hz(&self) -> u32 {
        48_000
    }
    fn write(&mut self, pcm: &[u8]) -> Result<chorus_client_linux::SinkWrite, SinkError> {
        self.writes += 1;
        let frames = (pcm.len() / 4) as u64;
        self.frames_written += frames;
        Ok(chorus_client_linux::SinkWrite {
            frames_written: frames,
            underran: false,
        })
    }
    fn delay_frames(&mut self) -> Result<i64, SinkError> {
        Ok(self.delay_frames)
    }
    fn in_xrun(&mut self) -> Result<bool, SinkError> {
        Ok(false)
    }
    fn drain(&mut self) -> Result<(), SinkError> {
        Ok(())
    }
    fn frames_played(&mut self) -> Result<u64, SinkError> {
        Ok(0)
    }
}

#[test]
fn the_error_moves_with_the_delay_the_device_reports_and_not_with_what_write_returned() {
    // `alsa`: "For playback the delay is defined as the time that a frame that
    // is written to the PCM stream shortly after this call will take to be
    // actually audible. It is as such the overall latency from the write call
    // to the final DAC."
    let config = SyncConfig {
        // A window of one and no smoothing, so the offset is exactly what the
        // one exchange said and the error is arithmetic rather than a filter.
        filter_window: 1,
        smoothing_alpha: 1.0,
        hard_resync_threshold_ns: 1e12,
        ..sync()
    };

    let error_at = |delay_frames: i64| -> f64 {
        let mut loop_ = SyncLoop::new(config);
        // A zero-offset, zero-round-trip exchange, so nothing but the delay
        // and the timestamps is in the answer.
        loop_.offer(&TimeSync {
            t0_ns: 0,
            t1_ns: 0,
            t2_ns: 0,
            t3_ns: 2,
        });
        let mut sink = DisagreeingSink {
            delay_frames,
            writes: 0,
            frames_written: 0,
        };
        // Two chunks of audio handed over, so anything derived from the write
        // call has something to be derived from.
        sink.write(&vec![0u8; 960 * 4]).unwrap();
        sink.write(&vec![0u8; 960 * 4]).unwrap();
        assert_eq!(sink.frames_written, 1_920);
        match loop_
            .observe(&mut sink, 1_000_000_000, 1_000_000_000, 48_000)
            .expect("the device reports its delay")
        {
            Correction::Fine { error_ns, .. } => error_ns,
            other => panic!("expected a fine correction, got {:?}", other),
        }
    };

    // The two runs hand the device identical audio and identical timestamps,
    // and differ only in what the DEVICE says about its distance from the DAC.
    // 48 000 frames of difference is exactly one second of it.
    let near = error_at(4_800);
    let far = error_at(52_800);
    assert!(
        (near - far - 1_000_000_000.0).abs() < 1.0,
        "one second more reported delay has to move the error one second: {} against {}",
        near,
        far
    );

    // And the signal the criterion forbids is constant across those two runs:
    // both wrote 1920 frames and both had every frame accepted, so an error
    // derived from the return of the write call could not tell them apart.
}

#[test]
fn a_device_that_reports_a_delay_of_zero_is_not_a_signal_and_is_not_corrected_against() {
    // The ALSA `null` device accepts every frame instantly and reports a delay
    // of zero forever, so there is nothing for a distance to the DAC to be a
    // distance FROM. A loop that formed an error from it would find itself
    // 180 ms early on every tick, for ever, and insert silence for as long as
    // the run lasted. `tools/lib.sh` refuses that device for the same reason.
    //
    // The answer is to correct NOTHING, which is the opposite of falling back
    // to another signal.
    let clock = VirtualClock::new();
    let mut dac = ModelledDac::new("null", 48_000, 4, 0.0, clock.clone());
    let mut loop_ = SyncLoop::new(sync());
    loop_.offer(&exchange_with(1_000_000, 200_000, 0, 0.5));

    for tick in 0..16u64 {
        let outcome = loop_
            .observe(&mut dac, 1_000_000_000 + tick, 2_000_000_000, 48_000)
            .expect("the device answers, it just answers zero");
        assert_eq!(
            outcome,
            Correction::NoDeviceDelay,
            "tick {} corrected against a delay of zero",
            tick
        );
    }
    assert_eq!(loop_.servo_updates(), 0, "the servo was never run");
    assert_eq!(loop_.hard_resyncs(), 0);
    // The offset it learned is still published; what is missing is the other
    // half of the error, not the offset.
    assert!(loop_.telemetry(1_000_000_000).offset_ns.is_some());

    // A device with a ring is corrected against normally.
    dac.write(&vec![0u8; 8_640 * 4]).unwrap();
    assert!(matches!(
        loop_
            .observe(&mut dac, 1_000_000_000, 1_000_000_000, 48_000)
            .expect("the device reports its delay"),
        Correction::Fine { .. } | Correction::HardResync { .. }
    ));
    assert_eq!(loop_.servo_updates(), 1);
}

#[test]
fn the_loop_never_sees_what_a_write_returned() {
    // A structural check, because the arithmetic one above can only show that
    // the delay is IN the error. `SyncLoop::observe` is handed a `PcmSink` and
    // calls `delay_frames` on it; `SinkWrite` is not in its signature, is not
    // in `Correction`, and is not stored on the loop. The way to keep that
    // true is for the source of the loop to contain no mention of it.
    let source = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/sync.rs"),
    )
    .expect("the sync module is committed and readable");
    let code: Vec<&str> = source
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !(trimmed.starts_with("//") || trimmed.starts_with("*"))
        })
        .collect();
    for name in ["SinkWrite", "frames_written", "underran", ".write("] {
        assert!(
            !code.iter().any(|line| line.contains(name)),
            "crates/client-linux/src/sync.rs mentions `{}` outside its documentation, so the \
             loop can reach the return of a write call",
            name
        );
    }
    assert!(
        code.iter().any(|line| line.contains("delay_frames()")),
        "the loop has to read the device's delay, or it is deriving its error from something else"
    );
}

// -------------------------------------------------------------------------
// AC-5: past the threshold it mutes, realigns and resumes.
// -------------------------------------------------------------------------

/// A loop with one symmetric, exactly-zero-offset exchange accepted, and a DAC
/// primed with exactly the playout latency.
///
/// With `delay == playout_latency` and `offset == 0`, the error the loop forms
/// reduces to `next_write_ts - now`, so a test asks for the error it wants by
/// choosing the timestamp. The exchange is `((100 - 0) + (200 - 300)) / 2`,
/// which is zero, over a 200 ns round trip.
fn primed(config: SyncConfig) -> (SyncLoop, ModelledDac, u64) {
    let clock = VirtualClock::new();
    clock.set_ns(1_000_000_000);
    let mut loop_ = SyncLoop::new(config);
    loop_.offer(&TimeSync {
        t0_ns: 0,
        t1_ns: 100,
        t2_ns: 200,
        t3_ns: 300,
    });
    let mut dac = ModelledDac::new("modelled-dac", 48_000, 4, 0.0, clock.clone());
    let frames = (config.playout_latency_ns as f64 / 1e9 * 48_000.0) as usize;
    dac.write(&vec![7u8; frames * 4]).unwrap();
    let now_ns = clock.now_ns();
    (loop_, dac, now_ns)
}

#[test]
fn an_error_past_the_threshold_mutes_realigns_and_resumes_rather_than_slewing() {
    let config = sync();
    let (mut loop_, mut dac, now_ns) = primed(config);

    // 40 ms of error against a 2 ms threshold.
    let outcome = loop_
        .observe(&mut dac, now_ns, now_ns + 40_000_000, 48_000)
        .expect("the device reports its delay");

    let step_ns = match outcome {
        Correction::HardResync { step_ns, error_ns } => {
            assert!(
                error_ns.abs() >= config.hard_resync_threshold_ns,
                "the error {} ns has to be past the {} ns threshold for this to be the tier",
                error_ns,
                config.hard_resync_threshold_ns
            );
            step_ns
        }
        other => panic!(
            "an error past the threshold has to step, not slew; got {:?}",
            other
        ),
    };
    assert_eq!(
        loop_.telemetry(now_ns).correction_ppm,
        0.0,
        "a step is not chased with a rate correction"
    );

    // Mute, realign, resume: the corrector silences the splice, moves the
    // pointer by the whole step, and then output resumes.
    let mut corrector = PlayoutCorrector::new(48_000, 4);
    corrector.set_rate_correction(250.0);
    corrector.hard_resync(step_ns, config.mute_ns);
    assert_eq!(corrector.correction_ppm(), 0.0, "the slew is abandoned");
    assert!(corrector.muted(), "the splice is covered");

    let mut moved = 0i64;
    let mut resumed_after = None;
    for chunk in 0..64 {
        let shaped = corrector.shape(&vec![7u8; 960 * 4]);
        let frames_out = (shaped.len() / 4) as i64;
        moved += 960 - frames_out;
        if !corrector.muted() && resumed_after.is_none() {
            resumed_after = Some(chunk);
        }
    }
    // `moved` counts frames removed, positive, which moves the playout pointer
    // FORWARD by that much; a negative count is silence inserted, which moves
    // it back. That is the same sign as the servo's step.
    let moved_ns = moved as f64 / 48_000.0 * 1e9;
    assert!(
        (moved_ns - step_ns).abs() < 25_000.0,
        "the playout moved {} ns against a step of {} ns",
        moved_ns,
        step_ns
    );
    assert!(
        corrector.muted_frames() > 0,
        "the splice was not muted at all"
    );
    assert!(
        resumed_after.is_some(),
        "playout never resumed after the mute"
    );
    assert!(
        corrector.muted_frames() as f64 / 48_000.0 * 1e9 <= config.mute_ns as f64 + 1.0,
        "the mute ran longer than it was configured to"
    );
}

// -------------------------------------------------------------------------
// AC-10: the modelled hour.
// -------------------------------------------------------------------------

#[test]
fn a_modelled_hour_holds_below_a_quarter_millisecond_after_the_first_minute() {
    // MODELLED, not measured. An hour of modelled time against modelled
    // crystals, a modelled switch and a modelled DAC. The run starts outside
    // the hard-resync tier on purpose, so the acquisition tier is exercised
    // and the "no hard resync after the first minute" half asserts something.
    let mut endpoint = ModelledEndpoint::new(params(), sync());
    endpoint.initial_misalignment_ns = 30_000_000;
    let result = endpoint.run(60 * MINUTE_NS);

    assert!(
        result.accepted > 3_000,
        "an hour at one exchange a second should accept thousands, got {}",
        result.accepted
    );
    assert_eq!(result.discarded, 0, "the quiet wired model produces no nonsense");
    assert_eq!(result.underruns, 0, "the modelled ring never ran dry");

    let true_peak = result.max_true_error_after(MINUTE_NS);
    let loop_peak = result.max_loop_error_after(MINUTE_NS);
    assert!(
        true_peak < BOUND_NS,
        "modelled playout error peaked at {:.0} ns after the first minute, against a {:.0} ns \
         bound",
        true_peak,
        BOUND_NS
    );
    assert!(
        loop_peak < BOUND_NS,
        "the error the loop itself formed peaked at {:.0} ns after the first minute",
        loop_peak
    );
    assert_eq!(
        result.hard_resyncs_after(MINUTE_NS),
        0,
        "hard resyncs after the first minute: {:?}",
        result
            .hard_resyncs_at_ns
            .iter()
            .filter(|t| **t >= MINUTE_NS)
            .collect::<Vec<_>>()
    );
    assert!(
        !result.hard_resyncs_at_ns.is_empty(),
        "the run has to start outside the bound and be driven in, or the assertion above is \
         about a run that was never wrong"
    );
    assert_eq!(
        result.clamped_ticks, 0,
        "the clamp never bit in this hour, which is what docs/decisions/0014 records"
    );

    // The counterfactual: the same run with the correction law neutered has to
    // fail BOTH halves, or this is asserting that nothing moved. The
    // hard-resync tier is deliberately left in place, so what is switched off
    // is the slew and nothing else - and what happens then is exactly what
    // AC-10's second half forbids: the skew walks the error back out to the
    // threshold and the loop steps, over and over, for the whole run.
    let mut uncorrected = ModelledEndpoint::new(
        params(),
        SyncConfig {
            kp: 0.0,
            ki: 0.0,
            ..sync()
        },
    );
    uncorrected.initial_misalignment_ns = 30_000_000;
    let free = uncorrected.run(10 * MINUTE_NS);
    let free_peak = free.max_true_error_after(MINUTE_NS);
    assert!(
        free_peak > BOUND_NS * 4.0,
        "with the slew switched off a 40 ppm skew has to walk far outside the bound; it peaked \
         at {:.0} ns",
        free_peak
    );
    assert!(
        free.hard_resyncs_after(MINUTE_NS) > 5,
        "and it has to keep stepping: {} hard resyncs after the first minute",
        free.hard_resyncs_after(MINUTE_NS)
    );
}

#[test]
fn the_modelled_hour_holds_at_the_worst_realistic_crystal_pair_too() {
    // 50 ppm fast against 50 ppm slow, which the roadmap's own worst-case
    // scenario puts at the edge of what two real crystals do, on a link with
    // more jitter than the quiet one.
    let mut endpoint = ModelledEndpoint::new(
        ModelParams {
            server_ppm: 50.0,
            client_ppm: -50.0,
            epoch_offset_ns: -8_000_000,
            base_one_way_ns: 250_000.0,
            jitter: JitterModel::Exponential { mean_us: 150.0 },
            seed: 0xC0FFEE_11,
            ..params()
        },
        sync(),
    );
    endpoint.initial_misalignment_ns = -25_000_000;
    let result = endpoint.run(60 * MINUTE_NS);
    let peak = result.max_true_error_after(MINUTE_NS);
    assert!(
        peak < BOUND_NS,
        "modelled playout error peaked at {:.0} ns after the first minute at 100 ppm relative \
         skew",
        peak
    );
    assert_eq!(result.hard_resyncs_after(MINUTE_NS), 0);
    assert_eq!(result.underruns, 0);
    assert_eq!(result.clamped_ticks, 0, "the clamp never bit here either");
}

/// The `wired loaded` row of `docs/decisions/0014`: the middle of the three
/// modelled scenarios, and the one the record reported with no committed
/// reproduction.
fn wired_loaded() -> ModelParams {
    ModelParams {
        server_ppm: -12.5,
        client_ppm: 38.0,
        epoch_offset_ns: 4_100_000,
        base_one_way_ns: 200_000.0,
        jitter: JitterModel::Exponential { mean_us: 150.0 },
        seed: 0x10AD_ED01,
        ..params()
    }
}

#[test]
fn the_modelled_hour_holds_on_the_loaded_wired_link_too() {
    // The third scenario `docs/decisions/0014` reports. It is here so that all
    // three rows of that table have a committed reproduction and none of them
    // is prose: a reader who does not trust the record can run it.
    let mut endpoint = ModelledEndpoint::new(wired_loaded(), sync());
    endpoint.initial_misalignment_ns = 18_000_000;
    let result = endpoint.run(60 * MINUTE_NS);

    let true_peak = result.max_true_error_after(MINUTE_NS);
    let loop_peak = result.max_loop_error_after(MINUTE_NS);
    println!(
        "0014 wired loaded: ground truth {:.1} us, loop {:.1} us, hard resyncs {} ({} after \
         minute 1), underruns {}, clamped {}, exchanges {} accepted / {} discarded",
        true_peak / 1_000.0,
        loop_peak / 1_000.0,
        result.hard_resyncs_at_ns.len(),
        result.hard_resyncs_after(MINUTE_NS),
        result.underruns,
        result.clamped_ticks,
        result.accepted,
        result.discarded
    );
    assert!(
        true_peak < BOUND_NS,
        "modelled playout error peaked at {:.0} ns after the first minute on the loaded link",
        true_peak
    );
    assert!(loop_peak < BOUND_NS);
    assert_eq!(result.hard_resyncs_after(MINUTE_NS), 0);
    assert!(
        !result.hard_resyncs_at_ns.is_empty(),
        "the run has to start outside the bound and be driven in"
    );
    assert_eq!(result.underruns, 0);
    assert_eq!(result.clamped_ticks, 0);
    assert_eq!(result.discarded, 0, "a loaded link is not a nonsensical one");
    assert!(result.accepted > 3_000, "got {}", result.accepted);
}

#[test]
fn the_sweep_that_fixed_the_window_the_alpha_and_the_interval_reruns() {
    // AC-11 asks that a later reader can tell a value measured here from one
    // borrowed off another project. `docs/decisions/0014` reports a six-row
    // sweep over the worst-case crystal pair, 20 modelled minutes each, and
    // this is that sweep as a committed reproduction: run it with
    // `-- --nocapture` and the table prints.
    //
    // What is asserted is what the decision RESTS on, not the digits it
    // happens to print: the shape FOUNDATION-1 carried does not hold this
    // phase's bound, the shape this phase chose does, and deeper windows with
    // lighter smoothing keep helping all the way down the table (which is why
    // the record has to argue for stopping at 64 rather than pointing at a
    // minimum).
    let worst = ModelParams {
        server_ppm: 50.0,
        client_ppm: -50.0,
        epoch_offset_ns: -8_000_000,
        base_one_way_ns: 250_000.0,
        jitter: JitterModel::Exponential { mean_us: 150.0 },
        seed: 0xC0FFEE_11,
        ..params()
    };
    // window, alpha, interval_ms - the table's own rows, in its own order.
    let candidates = [
        (8usize, 0.25f64, 1_000u64),
        (16, 0.25, 1_000),
        (32, 0.125, 500),
        (64, 0.0625, 500),
        (64, 0.03125, 500),
        (128, 0.03125, 500),
    ];

    let mut peaks = Vec::new();
    for (window, alpha, interval_ms) in candidates {
        let mut endpoint = ModelledEndpoint::new(
            worst,
            SyncConfig {
                filter_window: window,
                smoothing_alpha: alpha,
                interval_ms,
                ..sync()
            },
        );
        endpoint.initial_misalignment_ns = -25_000_000;
        let result = endpoint.run(20 * MINUTE_NS);
        let peak = result.max_true_error_after(MINUTE_NS);
        println!(
            "0014 sweep: window {:>3} alpha {:<8} interval {:>4} ms -> peak {:>6.1} us",
            window,
            alpha,
            interval_ms,
            peak / 1_000.0
        );
        peaks.push(peak);
    }

    assert!(
        peaks[0] > BOUND_NS,
        "the shape ServoConfig::default() carried out of FOUNDATION-1 (window 8, alpha 0.25, \
         1000 ms) has to MISS this phase's 250 us bound, or these constants were not outputs of \
         this phase at all; it peaked at {:.0} ns",
        peaks[0]
    );
    let chosen = peaks[3];
    assert!(
        chosen < BOUND_NS,
        "the chosen shape (window 64, alpha 0.0625, 500 ms) has to hold the bound; it peaked at \
         {:.0} ns",
        chosen
    );
    for pair in peaks.windows(2) {
        assert!(
            pair[1] < pair[0],
            "the sweep is reported as improving monotonically down the table, and it does not: \
             {:.0} ns then {:.0} ns",
            pair[0],
            pair[1]
        );
    }
    assert!(
        peaks[5] < chosen,
        "the record's own argument is that the sweep KEEPS improving past the chosen row and the \
         choice stops for a reason the sweep cannot show; if the deepest row were not better, \
         that paragraph would be wrong"
    );
}

// -------------------------------------------------------------------------
// AC-13: a nonsensical exchange is discarded, and the loop carries on.
// -------------------------------------------------------------------------

#[test]
fn an_exchange_that_cannot_be_a_round_trip_is_discarded_and_the_window_carries_on() {
    let config = sync();
    let mut loop_ = SyncLoop::new(config);
    // Three good exchanges first, so there is something to carry on with.
    for i in 1..=3u64 {
        assert!(matches!(
            loop_.offer(&exchange_with(i * 1_000_000_000, 200_000, 5_000_000, 0.5)),
            ExchangeVerdict::Accepted { .. }
        ));
    }
    let good = loop_.telemetry(3_000_000_000);

    // A round trip that would underflow: the server says it held the request
    // longer than the whole exchange took.
    match loop_.offer(&TimeSync {
        t0_ns: 4_000_000_000,
        t1_ns: 9_000_000_000,
        t2_ns: 9_500_000_000,
        t3_ns: 4_000_300_000,
    }) {
        ExchangeVerdict::Discarded(DiscardReason::RoundTripUnderflows { .. }) => {}
        other => panic!("expected an underflow discard, got {:?}", other),
    }
    // A round trip above the configured ceiling.
    match loop_.offer(&exchange_with(
        5_000_000_000,
        config.max_rtt_ns + 1_000,
        5_000_000,
        0.5,
    )) {
        ExchangeVerdict::Discarded(DiscardReason::RoundTripAboveCeiling { rtt_ns, ceiling_ns }) => {
            assert!(rtt_ns > ceiling_ns);
        }
        other => panic!("expected a ceiling discard, got {:?}", other),
    }
    // A reply that arrived before it was sent.
    match loop_.offer(&TimeSync {
        t0_ns: 6_000_000_000,
        t1_ns: 1,
        t2_ns: 2,
        t3_ns: 5_999_000_000,
    }) {
        ExchangeVerdict::Discarded(DiscardReason::ClientIntervalNotPositive { .. }) => {}
        other => panic!("expected a not-a-round-trip discard, got {:?}", other),
    }

    let after = loop_.telemetry(6_000_000_000);
    assert_eq!(after.discarded, 3);
    assert_eq!(after.accepted, 3, "no nonsense reached the window");
    assert_eq!(
        after.offset_ns, good.offset_ns,
        "the loop continues on the samples it already has"
    );
    assert_eq!(after.round_trip_ns, good.round_trip_ns);
    assert_eq!(after.bound_ns, good.bound_ns);

    // And the same over the closed loop: every exchange nonsense from the
    // start of the run, and the run keeps playing on what it had.
    let mut endpoint = ModelledEndpoint::new(params(), config);
    let first = endpoint.run(2 * MINUTE_NS);
    assert!(first.accepted > 60);
    endpoint.corrupt_exchanges = true;
    let second = endpoint.run(4 * MINUTE_NS);
    assert!(
        second.discarded > 60,
        "the corrupted stretch produced {} discards",
        second.discarded
    );
    assert_eq!(
        second.accepted, first.accepted,
        "not one nonsensical exchange was admitted"
    );
    assert_eq!(second.underruns, 0, "audio kept flowing throughout");
}

// -------------------------------------------------------------------------
// AC-15: the server goes quiet, audio keeps playing, the offset goes stale.
// -------------------------------------------------------------------------

#[test]
fn a_server_that_stops_answering_leaves_audio_playing_and_the_offset_marked_stale() {
    let config = sync();
    let mut endpoint = ModelledEndpoint::new(params(), config);
    let warm = endpoint.run(3 * MINUTE_NS);
    assert!(warm.accepted > 100);
    assert_eq!(warm.stale_ticks, 0);

    // The server goes quiet. The first stretch still corrects, because the
    // newest accepted sample is not yet older than the staleness limit; then
    // it goes stale and stays that way.
    endpoint.server_silent = true;
    let going = endpoint.run(6 * MINUTE_NS);
    assert!(going.telemetry.stale, "the offset has to be reported stale");
    let held_correction = going.telemetry.correction_ppm;
    let updates_when_stale = going.servo_updates;
    let accepted_when_stale = going.accepted;

    // Three more quiet minutes with the offset already stale.
    let quiet = endpoint.run(9 * MINUTE_NS);

    assert!(quiet.telemetry.stale, "still stale");
    assert!(
        quiet.telemetry.offset_ns.is_some(),
        "the offset it last had is still the offset it has; stale is a flag on it, not its absence"
    );
    assert_eq!(quiet.accepted, accepted_when_stale, "nothing was answered");
    assert_eq!(quiet.underruns, 0, "audio kept playing");
    assert!(
        quiet.samples.len() > 1_000,
        "the run kept producing playout samples while the server was quiet"
    );
    assert_eq!(
        quiet.servo_updates, updates_when_stale,
        "the servo was not run once over three fully stale minutes, so no NEW correction was \
         computed"
    );
    assert_eq!(
        quiet.telemetry.correction_ppm, held_correction,
        "the correction in force is held rather than reset; holdover, not a jump to zero"
    );
    assert!(
        quiet.stale_ticks > 300,
        "only {} ticks reported stale over three quiet minutes",
        quiet.stale_ticks
    );

    // The staleness limit is what decides when: the ticks inside it still
    // corrected, and the ones past it did not.
    let ticks_per_limit = config.staleness_limit_ns / (config.interval_ms * 1_000_000);
    let ticks_in_six_quiet_minutes = 3 * 60 * 1_000 / config.interval_ms;
    assert!(
        going.stale_ticks + ticks_per_limit + 2 >= ticks_in_six_quiet_minutes,
        "went stale later than one staleness limit after the last answer: {} stale ticks of {}",
        going.stale_ticks,
        ticks_in_six_quiet_minutes
    );

    // And a server that comes back is not a special case.
    endpoint.server_silent = false;
    let back = endpoint.run(11 * MINUTE_NS);
    assert!(!back.telemetry.stale, "the offset is fresh again");
    assert!(back.accepted > quiet.accepted);
    assert!(back.servo_updates > quiet.servo_updates);
}

// -------------------------------------------------------------------------
// AC-16: a device that will not report its delay stops the run, by name.
// -------------------------------------------------------------------------

#[test]
fn a_device_that_will_not_report_its_delay_stops_the_run_and_says_which_one() {
    let clock = VirtualClock::new();
    let mut dac = ModelledDac::new("hw:CARD=sulky,DEV=0", 48_000, 4, 0.0, clock.clone());
    dac.set_fault(DacFault::RefusesDelay);
    let mut loop_ = SyncLoop::new(sync());
    loop_.offer(&exchange_with(1_000_000, 200_000, 0, 0.5));

    let refused = loop_
        .observe(&mut dac, 2_000_000, 2_000_000, 48_000)
        .expect_err("a device that refuses its delay stops the run");
    assert_eq!(refused.device, "hw:CARD=sulky,DEV=0");
    assert!(
        refused.to_string().contains("hw:CARD=sulky,DEV=0"),
        "the message has to name the device: {}",
        refused
    );
    assert!(
        refused.to_string().contains("snd_pcm_delay"),
        "and what it said: {}",
        refused
    );
    assert!(
        refused
            .to_string()
            .contains("the return of a write call is not one"),
        "and that there is no fallback: {}",
        refused
    );

    // No fallback: the refusal happens before anything is corrected, even
    // though this device would happily accept every frame it was handed.
    assert_eq!(
        loop_.telemetry(2_000_000).correction_ppm,
        0.0,
        "nothing was corrected from a substitute signal"
    );
    assert_eq!(loop_.servo_updates(), 0, "the servo was never run");

    // A device that reports its delay again is not a special case: the
    // refusal is per call, so this is the same loop against a working one.
    dac.set_fault(DacFault::None);
    assert!(loop_.observe(&mut dac, 3_000_000, 3_000_000, 48_000).is_ok());
}

// -------------------------------------------------------------------------
// AC-17: a correction past the clamp is applied clamped, and reported so.
// -------------------------------------------------------------------------

#[test]
fn a_correction_past_the_clamp_is_applied_clamped_and_reported_as_clamped() {
    let config = SyncConfig {
        // Threshold well above the error below, so the fine tier is what
        // handles it and the clamp is what bites.
        hard_resync_threshold_ns: 1e12,
        max_correction_ppm: 300.0,
        filter_window: 1,
        smoothing_alpha: 1.0,
        ..sync()
    };
    let (mut loop_, mut dac, now_ns) = primed(config);

    // A 1 ms error asks for far more correction than the clamp allows.
    let outcome = loop_
        .observe(&mut dac, now_ns, now_ns + 1_000_000, 48_000)
        .expect("the device reports its delay");
    let (applied, clamped) = match outcome {
        Correction::Fine {
            correction_ppm,
            clamped,
            ..
        } => (correction_ppm, clamped),
        other => panic!("expected a fine correction, got {:?}", other),
    };
    assert!(clamped, "the clamp bit and has to be reported");
    assert_eq!(
        applied, -config.max_correction_ppm,
        "the CLAMPED value is what is applied"
    );
    let telemetry = loop_.telemetry(now_ns);
    assert!(telemetry.clamped, "and the telemetry says so");
    assert!(telemetry.line().contains("clamped=1"));
    assert_eq!(telemetry.correction_ppm, -config.max_correction_ppm);

    // Applied, not discarded: the clamped value reaches the frames.
    let mut corrector = PlayoutCorrector::new(48_000, 4);
    corrector.set_rate_correction(applied);
    corrector.advance(1_000_000_000);
    let shaped = corrector.shape(&vec![3u8; 960 * 4]);
    assert_eq!(
        shaped.len() / 4,
        960 + 14,
        "300 ppm of a second at 48 kHz is 14.4 frames of silence, and the fraction is carried"
    );
    assert_eq!(corrector.inserted_frames(), 14);

    // A correction inside the clamp is not reported as clamped, so the flag
    // says something rather than being always on.
    let (mut easy, mut easy_dac, easy_now) = primed(config);
    let gentle = easy
        .observe(&mut easy_dac, easy_now, easy_now + 20_000, 48_000)
        .expect("the device reports its delay");
    match gentle {
        Correction::Fine {
            clamped,
            correction_ppm,
            ..
        } => {
            assert!(!clamped);
            assert!(correction_ppm.abs() < config.max_correction_ppm);
        }
        other => panic!("expected a fine correction, got {:?}", other),
    }
    assert!(!easy.telemetry(easy_now).clamped);
    assert!(easy.telemetry(easy_now).line().contains("clamped=0"));
}

// -------------------------------------------------------------------------
// The constants, and the committed file they are also written in.
// -------------------------------------------------------------------------

/// Read one `key = value` out of `config/sync.conf`, as a number.
fn conf(key: &str) -> f64 {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../config/sync.conf")
        .canonicalize()
        .expect("config/sync.conf is committed");
    let text = std::fs::read_to_string(path).expect("config/sync.conf is readable");
    for line in text.lines() {
        let line = match line.find('#') {
            Some(at) => &line[..at],
            None => line,
        };
        if let Some((k, v)) = line.split_once('=') {
            if k.trim() == key {
                return v
                    .trim()
                    .parse()
                    .unwrap_or_else(|_| panic!("config/sync.conf {} is not a number", key));
            }
        }
    }
    panic!("config/sync.conf has no {}", key);
}

#[test]
fn the_committed_constants_and_the_compiled_ones_are_the_same_numbers() {
    // The verification entry points read `config/sync.conf` and pass those
    // values in on the command line, so a check and the thing it checks cannot
    // drift apart. This is the assertion that keeps the two spellings equal.
    assert_eq!(conf("filter_window"), FILTER_WINDOW as f64);
    assert_eq!(conf("smoothing_alpha"), SMOOTHING_ALPHA);
    assert_eq!(
        conf("hard_resync_threshold_us"),
        HARD_RESYNC_THRESHOLD_US as f64
    );
    assert_eq!(conf("max_correction_ppm"), MAX_CORRECTION_PPM);
    assert_eq!(conf("staleness_limit_ms"), STALENESS_LIMIT_MS as f64);
    assert_eq!(conf("max_rtt_us"), MAX_RTT_US as f64);
    assert_eq!(conf("sync_interval_ms"), SYNC_INTERVAL_MS as f64);
    assert_eq!(conf("playout_latency_us"), PLAYOUT_LATENCY_US as f64);
    assert_eq!(conf("mute_us"), MUTE_US as f64);
    // The hour-long run's own schedule, which only the entry point reads. The
    // arithmetic is asserted here because the alternative is finding out on a
    // rig, an hour in, that the last capture was due after the run had ended.
    let run = conf("sync_hour_run_seconds");
    let settle = conf("sync_hour_settle_seconds");
    let captures = conf("sync_hour_captures");
    let capture = conf("sync_hour_capture_seconds");
    assert!(capture > 0.0);
    assert!(captures >= 2.0, "one window is not a distribution");
    assert!(
        settle >= 60.0,
        "AC-1 excludes the first minute, so no capture may start inside it"
    );
    assert!(
        run - settle >= 3_600.0,
        "the graded window, after the settle, is still at least the hour AC-1 asks for"
    );
    // The last capture starts at settle + (captures - 1) * spacing and has to
    // finish before the run does.
    let spacing = ((run - settle) / captures).floor();
    assert!(
        settle + (captures - 1.0) * spacing + capture <= run,
        "the last capture must end inside the run"
    );

    let config = SyncConfig::default();
    assert_eq!(config.filter_window, FILTER_WINDOW);
    assert_eq!(
        config.hard_resync_threshold_ns,
        HARD_RESYNC_THRESHOLD_US as f64 * 1_000.0
    );
    assert_eq!(config.max_correction_ppm, MAX_CORRECTION_PPM);
    assert_eq!(config.staleness_limit_ns, STALENESS_LIMIT_MS * 1_000_000);
    assert_eq!(config.max_rtt_ns, MAX_RTT_US * 1_000);
    assert_eq!(config.playout_latency_ns, PLAYOUT_LATENCY_US * 1_000);
}
