//! The sync loop on the real playout path.
//!
//! `crates/sync` is a pure library: it estimates an offset from four
//! timestamps and computes a correction from an error. This module is what
//! hands it the timestamps a real exchange produced and the error a real audio
//! device implies, and what turns the correction it returns into frames that
//! actually reach a DAC.
//!
//! # Where the error signal comes from, and where it does not
//!
//! [`SyncLoop::observe`] takes a `&mut dyn PcmSink` and calls
//! [`PcmSink::delay_frames`] on it, which is `snd_pcm_delay`: "the overall
//! latency from the write call to the final DAC". Nothing in this module can
//! see a [`SinkWrite`], because nothing in this module is handed one. The
//! wrong signal - the time a write call took to return - is available in the
//! playout loop and is deliberately unreachable from here: it measures when
//! the device accepted bytes, not when they become audible, and a loop
//! disciplined against it converges beautifully onto the wrong target.
//!
//! A device that will not say how far it is from its DAC stops the run
//! ([`DelayRefused`]). There is no fallback estimate, because every available
//! fallback is the wrong signal wearing a different name.
//!
//! # The error, written out
//!
//! ```text
//! error_ns = (next_write_ts_ns + playout_latency_ns)
//!            - (client_now_ns + device_delay_ns + offset_ns)
//! ```
//!
//! `next_write_ts_ns` is the presentation timestamp, on the server timeline,
//! of the next frame the client will write. `device_delay_ns` is what the
//! device reports, so `client_now_ns + device_delay_ns` is when that frame
//! becomes audible on the client's own monotonic clock, and adding the offset
//! puts that instant on the server timeline. `playout_latency_ns` is the fixed
//! end-to-end latency every endpoint in a group applies; content due at `t` is
//! audible at `t + playout_latency_ns` on the server timeline, at every
//! endpoint, which is what makes two endpoints agree rather than merely each
//! being self-consistent.
//!
//! The sign follows `chorus_sync::Servo`: positive means playout is AHEAD of
//! where the server timeline says it should be, so the content is becoming
//! audible too early.
//!
//! # Why this is not the same job as pacing
//!
//! The playout loop already chooses WHEN to write, which holds the device's
//! reported delay near a target. That controls how much audio the ring holds
//! and moves nothing: a DAC plays the frames it has been given in order, at
//! its own rate, so writing later shortens the ring by exactly as much as it
//! delays the write and the audible instant of a frame does not move.
//!
//! Correction therefore changes WHAT is written, never when. A fine correction
//! of `c` ppm inserts or drops `c` frames per million; a hard resync inserts or
//! drops the whole step at once, under a mute. [`PlayoutCorrector`] is that
//! half, kept separate so the modelled hour and the shipped client drive the
//! same code.

use std::fmt;

use chorus_protocol::TimeSync;
use chorus_sync::{OffsetFilter, Servo, ServoAction, ServoConfig};

use crate::sink::{PcmSink, SinkError};

/// Exchanges in the minimum-round-trip window.
///
/// Committed in `config/sync.conf`; see
/// `docs/decisions/0014-the-sync-loop-on-the-real-path.md` for what each of
/// these was chosen from. `crates/client-linux/tests/sync_loop.rs` asserts the
/// file and these constants agree, so a check and the thing it checks cannot
/// drift apart.
pub const FILTER_WINDOW: usize = 64;

/// Weight of a new sample in the exponential smoother.
pub const SMOOTHING_ALPHA: f64 = 0.0625;

/// Error at or above which the loop steps rather than slews, in microseconds.
pub const HARD_RESYNC_THRESHOLD_US: u64 = 2_000;

/// Largest rate correction the loop will apply, in ppm.
pub const MAX_CORRECTION_PPM: f64 = 300.0;

/// How long an accepted sample may be the newest one before the offset is
/// stale, in milliseconds.
pub const STALENESS_LIMIT_MS: u64 = 10_000;

/// Largest round trip an exchange may report and still be admitted to the
/// window, in microseconds.
pub const MAX_RTT_US: u64 = 100_000;

/// How often the client runs an exchange and updates the servo, in
/// milliseconds.
pub const SYNC_INTERVAL_MS: u64 = 500;

/// The fixed end-to-end playout latency every endpoint applies, in
/// microseconds.
pub const PLAYOUT_LATENCY_US: u64 = 180_000;

/// How much audio is silenced around a hard resync's splice, in microseconds.
pub const MUTE_US: u64 = 20_000;

/// Proportional gain, inherited unchanged from `docs/decisions/0006`.
pub const SERVO_KP: f64 = 0.4;

/// Integral gain, inherited unchanged from `docs/decisions/0006`.
pub const SERVO_KI: f64 = 0.08;

/// Everything the loop is configured with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SyncConfig {
    /// Exchange and servo cadence, in milliseconds.
    pub interval_ms: u64,
    /// Exchanges in the minimum-round-trip window.
    pub filter_window: usize,
    /// Weight of a new sample in the smoother.
    pub smoothing_alpha: f64,
    /// Error at or above which the loop steps rather than slews.
    pub hard_resync_threshold_ns: f64,
    /// Largest rate correction, in ppm.
    pub max_correction_ppm: f64,
    /// How old the newest accepted sample may be before the offset is stale.
    pub staleness_limit_ns: u64,
    /// Largest admissible round trip.
    pub max_rtt_ns: u64,
    /// The fixed end-to-end playout latency, in nanoseconds.
    pub playout_latency_ns: u64,
    /// Audio silenced around a hard resync's splice, in nanoseconds.
    pub mute_ns: u64,
    /// Proportional gain.
    pub kp: f64,
    /// Integral gain.
    pub ki: f64,
}

impl Default for SyncConfig {
    fn default() -> SyncConfig {
        SyncConfig {
            interval_ms: SYNC_INTERVAL_MS,
            filter_window: FILTER_WINDOW,
            smoothing_alpha: SMOOTHING_ALPHA,
            hard_resync_threshold_ns: HARD_RESYNC_THRESHOLD_US as f64 * 1_000.0,
            max_correction_ppm: MAX_CORRECTION_PPM,
            staleness_limit_ns: STALENESS_LIMIT_MS * 1_000_000,
            max_rtt_ns: MAX_RTT_US * 1_000,
            playout_latency_ns: PLAYOUT_LATENCY_US * 1_000,
            mute_ns: MUTE_US * 1_000,
            kp: SERVO_KP,
            ki: SERVO_KI,
        }
    }
}

impl SyncConfig {
    /// The servo gains and limits this configuration implies.
    pub fn servo(&self) -> ServoConfig {
        ServoConfig {
            kp: self.kp,
            ki: self.ki,
            max_correction_ppm: self.max_correction_ppm,
            hard_resync_threshold_ns: self.hard_resync_threshold_ns,
            filter_window: self.filter_window,
            smoothing_alpha: self.smoothing_alpha,
        }
    }
}

/// Why an exchange was thrown away instead of being admitted to the window.
///
/// Each of these describes timestamps that cannot have come from a real round
/// trip. `TimeSync::rtt_ns` saturates rather than wrapping, so a nonsensical
/// exchange arrives here as a plausible-looking zero; naming the reason is
/// what keeps it from being averaged in as an unusually good sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscardReason {
    /// The client received the reply before, or at, the moment it sent the
    /// request.
    ClientIntervalNotPositive {
        /// Client transmit.
        t0_ns: u64,
        /// Client receive.
        t3_ns: u64,
    },
    /// The server transmitted before it received.
    ServerIntervalNegative {
        /// Server receive.
        t1_ns: u64,
        /// Server transmit.
        t2_ns: u64,
    },
    /// The server says it held the request longer than the whole round trip
    /// took, so the network time would be negative.
    RoundTripUnderflows {
        /// How long the exchange took on the client clock.
        client_elapsed_ns: u64,
        /// How long the server says it held it.
        server_elapsed_ns: u64,
    },
    /// The round trip is above the configured ceiling.
    RoundTripAboveCeiling {
        /// The round trip that was measured.
        rtt_ns: u64,
        /// The ceiling it exceeded.
        ceiling_ns: u64,
    },
}

impl DiscardReason {
    /// The short stable name, for a log line a script can key on.
    pub fn name(self) -> &'static str {
        match self {
            DiscardReason::ClientIntervalNotPositive { .. } => "client-interval-not-positive",
            DiscardReason::ServerIntervalNegative { .. } => "server-interval-negative",
            DiscardReason::RoundTripUnderflows { .. } => "round-trip-underflows",
            DiscardReason::RoundTripAboveCeiling { .. } => "round-trip-above-ceiling",
        }
    }
}

impl fmt::Display for DiscardReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiscardReason::ClientIntervalNotPositive { t0_ns, t3_ns } => write!(
                f,
                "the reply was received at {} ns having been sent at {} ns, which is not a round \
                 trip",
                t3_ns, t0_ns
            ),
            DiscardReason::ServerIntervalNegative { t1_ns, t2_ns } => write!(
                f,
                "the server says it transmitted at {} ns having received at {} ns",
                t2_ns, t1_ns
            ),
            DiscardReason::RoundTripUnderflows {
                client_elapsed_ns,
                server_elapsed_ns,
            } => write!(
                f,
                "the server says it held the request {} ns and the whole exchange took {} ns, so \
                 the network time would be negative",
                server_elapsed_ns, client_elapsed_ns
            ),
            DiscardReason::RoundTripAboveCeiling { rtt_ns, ceiling_ns } => write!(
                f,
                "the round trip of {} ns is above the {} ns ceiling this run was configured with",
                rtt_ns, ceiling_ns
            ),
        }
    }
}

/// What the loop did with one exchange.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExchangeVerdict {
    /// Admitted to the window.
    Accepted {
        /// Round trip of this exchange.
        rtt_ns: u64,
        /// Offset this exchange estimated.
        offset_ns: i64,
    },
    /// Thrown away, for the named reason. The window is untouched and the loop
    /// continues on the samples it already has.
    Discarded(DiscardReason),
}

/// The device would not say how far it is from its DAC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelayRefused {
    /// The device that refused, by the name it was opened under.
    pub device: String,
    /// What it said.
    pub cause: SinkError,
}

impl fmt::Display for DelayRefused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the audio device '{}' would not report its delay to the DAC: {}. The run stops here: \
             there is no second estimate of when a frame becomes audible, and the return of a \
             write call is not one",
            self.device, self.cause
        )
    }
}

impl std::error::Error for DelayRefused {}

/// What the loop decided on one tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Correction {
    /// The device answered with a delay of zero, which is not a distance to a
    /// DAC. Nothing is corrected.
    ///
    /// Two devices report this and both mean "do not form an error from me".
    /// A device with no ring - the ALSA `null` device accepts every frame
    /// instantly and reports zero forever - has nothing to be a distance
    /// from, and a loop disciplined against it would insert silence for as
    /// long as it ran. A real device reporting zero has run dry, and `alsa`
    /// warns that on underrun the reported delay "will not necessarily got
    /// down to 0", so a zero from a real device is a fault reading and not a
    /// small one.
    ///
    /// This is not a fallback to another signal. It is the absence of the one
    /// signal this loop is entitled to, and the answer is to correct nothing.
    /// It says nothing about the offset: how old the newest accepted sample is
    /// is judged here exactly as it is in every other state, so a device with
    /// no delay to report cannot make a stale offset read as fresh.
    NoDeviceDelay,
    /// No exchange has been accepted yet, so there is no offset and nothing is
    /// corrected.
    NoOffset,
    /// The newest accepted sample is older than the staleness limit. Playback
    /// continues; the correction already in force stays in force; no new one is
    /// computed.
    Stale {
        /// How old the newest accepted sample is.
        age_ns: u64,
    },
    /// Slew: apply this rate correction until the next tick.
    Fine {
        /// The correction to apply, already clamped.
        correction_ppm: f64,
        /// Whether the clamp cut the raw value down.
        clamped: bool,
        /// The error this correction answers.
        error_ns: f64,
    },
    /// Step: mute, move the playout pointer by `step_ns`, resume.
    HardResync {
        /// How far the playout pointer moves, in nanoseconds.
        step_ns: f64,
        /// The error that was too large to slew away.
        error_ns: f64,
    },
}

/// What a client publishes about its own timing health.
///
/// `offset_ns`, `round_trip_ns` and `bound_ns` are `Option` and not zero on
/// purpose: a client that has run no successful exchange has no offset and no
/// bound, and publishing a zero for either would be publishing a perfectly
/// synchronised endpoint with a perfect error bound.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Telemetry {
    /// The offset in use, in nanoseconds, or `None` if no exchange has been
    /// accepted.
    pub offset_ns: Option<i64>,
    /// The round trip of the exchange that offset came from.
    pub round_trip_ns: Option<u64>,
    /// Half that round trip: the bound on the offset, per RFC 5905 section 4,
    /// where the synchronisation distance is `delta / 2 + epsilon`.
    pub bound_ns: Option<u64>,
    /// Whether the newest accepted sample is older than the staleness limit.
    pub stale: bool,
    /// Age of the newest accepted sample, or `None` if there is none.
    pub age_ns: Option<u64>,
    /// The rate correction in force, in ppm.
    pub correction_ppm: f64,
    /// Whether the correction in force was cut down by the clamp.
    pub clamped: bool,
    /// Hard resyncs so far.
    pub hard_resyncs: u32,
    /// Exchanges admitted to the window.
    pub accepted: u64,
    /// Exchanges thrown away.
    pub discarded: u64,
}

impl Telemetry {
    /// One line for a report or a log, with `none` where there is no answer.
    ///
    /// `none` rather than `0` is the whole point: see [`Telemetry`].
    pub fn line(&self) -> String {
        fn or_none<T: fmt::Display>(value: Option<T>) -> String {
            match value {
                Some(v) => v.to_string(),
                None => "none".to_string(),
            }
        }
        format!(
            "offset_ns={} round_trip_ns={} bound_ns={} stale={} age_ns={} correction_ppm={:.3} \
             clamped={} hard_resyncs={} accepted={} discarded={}",
            or_none(self.offset_ns),
            or_none(self.round_trip_ns),
            or_none(self.bound_ns),
            u8::from(self.stale),
            or_none(self.age_ns),
            self.correction_ppm,
            u8::from(self.clamped),
            self.hard_resyncs,
            self.accepted,
            self.discarded
        )
    }
}

/// The client half of the exchange, the filter, the servo, and the error the
/// servo is driven with.
#[derive(Debug)]
pub struct SyncLoop {
    config: SyncConfig,
    filter: OffsetFilter,
    servo: Servo,
    /// The filtered offset in use, and the client time it was formed at.
    offset_ns: Option<f64>,
    /// Client time of the newest ACCEPTED exchange.
    newest_accepted_ns: Option<u64>,
    correction_ppm: f64,
    clamped: bool,
    accepted: u64,
    discarded: u64,
}

impl SyncLoop {
    /// A loop that has run no exchange.
    pub fn new(config: SyncConfig) -> SyncLoop {
        SyncLoop {
            filter: OffsetFilter::new(config.filter_window, config.smoothing_alpha),
            servo: Servo::new(config.servo()),
            config,
            offset_ns: None,
            newest_accepted_ns: None,
            correction_ppm: 0.0,
            clamped: false,
            accepted: 0,
            discarded: 0,
        }
    }

    /// The configuration this loop runs under.
    pub fn config(&self) -> SyncConfig {
        self.config
    }

    /// The request to put on the wire: `t0` stamped, the rest left for the
    /// server and for [`SyncLoop::complete`].
    pub fn request(&self, client_now_ns: u64) -> TimeSync {
        TimeSync {
            t0_ns: client_now_ns,
            t1_ns: 0,
            t2_ns: 0,
            t3_ns: 0,
        }
    }

    /// The exchange a reply completes, with `t3` stamped at receipt.
    ///
    /// `t0`, `t1` and `t2` come off the reply: `t0` is the client's own
    /// transmit stamp echoed back, and the two middle ones are the server's.
    /// `t3` is taken here because here is where the reply arrived.
    pub fn complete(reply: &TimeSync, client_now_ns: u64) -> TimeSync {
        TimeSync {
            t0_ns: reply.t0_ns,
            t1_ns: reply.t1_ns,
            t2_ns: reply.t2_ns,
            t3_ns: client_now_ns,
        }
    }

    /// Offer one completed exchange to the window.
    pub fn offer(&mut self, exchange: &TimeSync) -> ExchangeVerdict {
        if let Some(reason) = nonsense(exchange, self.config.max_rtt_ns) {
            self.discarded += 1;
            return ExchangeVerdict::Discarded(reason);
        }
        let rtt_ns = exchange.rtt_ns();
        let offset_ns = exchange.offset_ns();
        let at_ns = exchange.t3_ns as f64;
        let filtered = self.filter.push(at_ns, rtt_ns as f64, offset_ns as f64);
        self.offset_ns = Some(filtered);
        self.newest_accepted_ns = Some(exchange.t3_ns);
        self.accepted += 1;
        ExchangeVerdict::Accepted { rtt_ns, offset_ns }
    }

    /// One tick of the loop: read the delay the device reports, form the
    /// error, and decide.
    ///
    /// `next_write_ts_ns` is the presentation timestamp of the next frame the
    /// client will write, on the server timeline.
    pub fn observe<S: PcmSink + ?Sized>(
        &mut self,
        sink: &mut S,
        client_now_ns: u64,
        next_write_ts_ns: u64,
        rate_hz: u32,
    ) -> Result<Correction, DelayRefused> {
        // First, and unconditionally: a device that will not say how far it is
        // from its DAC stops the run, whether or not an offset exists yet.
        let delay_frames = match sink.delay_frames() {
            Ok(frames) => frames,
            Err(cause) => {
                return Err(DelayRefused {
                    device: sink.device().to_string(),
                    cause,
                })
            }
        };
        if delay_frames <= 0 {
            return Ok(Correction::NoDeviceDelay);
        }
        let delay_ns = delay_frames as f64 * 1_000_000_000.0 / f64::from(rate_hz);

        let (Some(offset_ns), Some(_)) = (self.offset_ns, self.newest_accepted_ns) else {
            return Ok(Correction::NoOffset);
        };

        if let Some(age_ns) = self.stale_age_ns(client_now_ns) {
            return Ok(Correction::Stale { age_ns });
        }

        let target_ns = next_write_ts_ns as f64 + self.config.playout_latency_ns as f64;
        let audible_ns = client_now_ns as f64 + delay_ns + offset_ns;
        let error_ns = target_ns - audible_ns;

        let interval_s = self.config.interval_ms as f64 / 1_000.0;
        Ok(match self.servo.update(error_ns, interval_s) {
            ServoAction::Fine { correction_ppm } => {
                self.correction_ppm = correction_ppm;
                self.clamped = self.servo.last_correction_was_clamped();
                Correction::Fine {
                    correction_ppm,
                    clamped: self.clamped,
                    error_ns,
                }
            }
            ServoAction::HardResync { step_ns } => {
                // The history is about a playout pointer that is about to
                // move, so it is not about anything any more. The offset
                // estimate itself is kept: it is what the step is computed
                // from, and throwing it away would make the loop reacquire
                // the timeline it just used.
                self.correction_ppm = 0.0;
                self.clamped = false;
                Correction::HardResync { step_ns, error_ns }
            }
        })
    }

    /// How old the newest accepted sample is, when it is older than the
    /// staleness limit, and `None` otherwise.
    ///
    /// The age of the newest accepted sample is a fact about the exchanges and
    /// the clock alone. It does not depend on what the audio device reported,
    /// nor on what this loop decided to do about it: a device with no delay to
    /// report is a reason to correct nothing, not a reason to trust an old
    /// offset. Both the decision to stop computing new corrections and the
    /// flag [`Telemetry::stale`] publishes are taken from here, so no state of
    /// the device can make the two disagree.
    fn stale_age_ns(&self, client_now_ns: u64) -> Option<u64> {
        let age_ns = client_now_ns.saturating_sub(self.newest_accepted_ns?);
        if age_ns > self.config.staleness_limit_ns {
            Some(age_ns)
        } else {
            None
        }
    }

    /// What this client publishes about its own timing health.
    pub fn telemetry(&self, client_now_ns: u64) -> Telemetry {
        let selected = self.filter.selected();
        let round_trip_ns = selected.map(|s| s.rtt_ns.max(0.0) as u64);
        Telemetry {
            offset_ns: self.offset_ns.map(|o| o as i64),
            round_trip_ns,
            // RFC 5905 section 4: "The synchronization distance (LAMBDA) equal
            // to EPSILON + DELTA / 2 represents the maximum error due to all
            // causes." Half the round trip of the sample the offset came from,
            // which is the sample the filter selected and not the newest one.
            bound_ns: round_trip_ns.map(|rtt| rtt / 2),
            stale: self.stale_age_ns(client_now_ns).is_some(),
            age_ns: self
                .newest_accepted_ns
                .map(|at| client_now_ns.saturating_sub(at)),
            correction_ppm: self.correction_ppm,
            clamped: self.clamped,
            hard_resyncs: self.servo.hard_resyncs(),
            accepted: self.accepted,
            discarded: self.discarded,
        }
    }

    /// Servo updates so far, which is one per tick that formed an error.
    pub fn servo_updates(&self) -> u32 {
        self.servo.updates()
    }

    /// Hard resyncs so far.
    pub fn hard_resyncs(&self) -> u32 {
        self.servo.hard_resyncs()
    }
}

/// The reason an exchange cannot describe a real round trip, if there is one.
fn nonsense(exchange: &TimeSync, ceiling_ns: u64) -> Option<DiscardReason> {
    if exchange.t3_ns <= exchange.t0_ns {
        return Some(DiscardReason::ClientIntervalNotPositive {
            t0_ns: exchange.t0_ns,
            t3_ns: exchange.t3_ns,
        });
    }
    if exchange.t2_ns < exchange.t1_ns {
        return Some(DiscardReason::ServerIntervalNegative {
            t1_ns: exchange.t1_ns,
            t2_ns: exchange.t2_ns,
        });
    }
    let client_elapsed_ns = exchange.t3_ns - exchange.t0_ns;
    let server_elapsed_ns = exchange.t2_ns - exchange.t1_ns;
    if server_elapsed_ns > client_elapsed_ns {
        return Some(DiscardReason::RoundTripUnderflows {
            client_elapsed_ns,
            server_elapsed_ns,
        });
    }
    let rtt_ns = client_elapsed_ns - server_elapsed_ns;
    if rtt_ns > ceiling_ns {
        return Some(DiscardReason::RoundTripAboveCeiling {
            rtt_ns,
            ceiling_ns,
        });
    }
    None
}

/// Turning a correction into frames.
///
/// A DAC plays what it has been given, in order, at its own rate, so the only
/// way to move when a frame becomes audible is to change how many frames come
/// before it. A fine correction of `c` ppm therefore drops `c` frames per
/// million when playout is ahead and inserts that many when it is behind; a
/// hard resync does the whole step at once, with the splice muted.
///
/// Fractions are carried rather than rounded away: at 48 kHz a 100 ppm
/// correction is 4.8 frames a second, and rounding each chunk's share to a
/// whole frame would quantise the correction to 50 frames a second, which is
/// 1000 ppm of granularity on a 300 ppm clamp.
#[derive(Debug, Clone)]
pub struct PlayoutCorrector {
    rate_hz: u32,
    frame_len: usize,
    correction_ppm: f64,
    /// Frames owed. Positive drops, negative inserts.
    pending_frames: f64,
    inserted_frames: u64,
    dropped_frames: u64,
    /// Frames still to be silenced around a splice.
    mute_frames: u64,
    muted_frames: u64,
}

impl PlayoutCorrector {
    /// A corrector for a device of this rate and frame size.
    pub fn new(rate_hz: u32, frame_len: usize) -> PlayoutCorrector {
        PlayoutCorrector {
            rate_hz: rate_hz.max(1),
            frame_len: frame_len.max(1),
            correction_ppm: 0.0,
            pending_frames: 0.0,
            inserted_frames: 0,
            dropped_frames: 0,
            mute_frames: 0,
            muted_frames: 0,
        }
    }

    /// The rate correction in force, in ppm.
    pub fn correction_ppm(&self) -> f64 {
        self.correction_ppm
    }

    /// Frames of silence inserted so far.
    pub fn inserted_frames(&self) -> u64 {
        self.inserted_frames
    }

    /// Frames of audio dropped so far.
    pub fn dropped_frames(&self) -> u64 {
        self.dropped_frames
    }

    /// Frames silenced so far, which is what a mute costs.
    pub fn muted_frames(&self) -> u64 {
        self.muted_frames
    }

    /// Whether the output is currently muted.
    pub fn muted(&self) -> bool {
        self.mute_frames > 0
    }

    /// Frames still owed, positive to drop and negative to insert.
    pub fn pending_frames(&self) -> f64 {
        self.pending_frames
    }

    /// Apply a fine correction from here until the next one.
    pub fn set_rate_correction(&mut self, correction_ppm: f64) {
        self.correction_ppm = correction_ppm;
    }

    /// Let `elapsed_ns` of playout time pass at the correction in force.
    pub fn advance(&mut self, elapsed_ns: u64) {
        if self.correction_ppm == 0.0 || elapsed_ns == 0 {
            return;
        }
        let seconds = elapsed_ns as f64 / 1e9;
        self.pending_frames += self.correction_ppm * 1e-6 * seconds * f64::from(self.rate_hz);
    }

    /// Mute, then step the playout pointer by `step_ns`, then resume.
    ///
    /// The three are one call because they are one act: the mute exists to
    /// cover the splice the step makes, and resuming is what the mute running
    /// out means. Silencing rather than inserting silence is deliberate -
    /// inserting would itself move the playout pointer, and then the mute
    /// would be part of the correction instead of covering it.
    pub fn hard_resync(&mut self, step_ns: f64, mute_ns: u64) {
        self.correction_ppm = 0.0;
        self.pending_frames += step_ns * f64::from(self.rate_hz) / 1e9;
        let mute = mute_ns as f64 * f64::from(self.rate_hz) / 1e9;
        self.mute_frames = self.mute_frames.max(mute as u64);
    }

    /// Shape one chunk of PCM into what should actually be written.
    ///
    /// Drops frames from the front, inserts silence at the front, and silences
    /// whatever a mute still owes, in that order.
    ///
    /// The mute covers the SPLICE, and the splice is where audio resumes. When
    /// this call inserted silence, the join the ear would hear is at the far
    /// end of that insertion, not at its start, so the mute begins where the
    /// insertion stops. Silencing the insertion instead would spend the mute
    /// on frames the corrector had just zeroed itself and report them in
    /// [`PlayoutCorrector::muted_frames`] as though a splice had been covered:
    /// a backward step larger than the mute would then leave the only audible
    /// discontinuity it makes uncovered, and say it had covered it.
    pub fn shape(&mut self, pcm: &[u8]) -> Vec<u8> {
        let frames_in = pcm.len() / self.frame_len;
        let mut out: Vec<u8> = Vec::with_capacity(pcm.len());

        let mut from = 0usize;
        // Where audio resumes in `out`, which is where a mute has to start.
        let mut splice_at = 0usize;
        if self.pending_frames >= 1.0 {
            let drop = (self.pending_frames as u64).min(frames_in as u64);
            self.pending_frames -= drop as f64;
            self.dropped_frames += drop;
            from = drop as usize * self.frame_len;
        } else if self.pending_frames <= -1.0 {
            let insert = (-self.pending_frames) as u64;
            self.pending_frames += insert as f64;
            self.inserted_frames += insert;
            out.resize(insert as usize * self.frame_len, 0);
            splice_at = out.len();
        }
        out.extend_from_slice(&pcm[from..]);

        if self.mute_frames > 0 {
            let frames_after_splice = ((out.len() - splice_at) / self.frame_len) as u64;
            let silence = self.mute_frames.min(frames_after_splice);
            let bytes = silence as usize * self.frame_len;
            out[splice_at..splice_at + bytes].fill(0);
            self.mute_frames -= silence;
            self.muted_frames += silence;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exchange(t0: u64, t1: u64, t2: u64, t3: u64) -> TimeSync {
        TimeSync {
            t0_ns: t0,
            t1_ns: t1,
            t2_ns: t2,
            t3_ns: t3,
        }
    }

    #[test]
    fn a_sensible_exchange_is_admitted() {
        let mut loop_ = SyncLoop::new(SyncConfig::default());
        // 400 us round trip, server clock 1 s ahead of the client's.
        let verdict = loop_.offer(&exchange(0, 1_000_000_200, 1_000_000_300, 500));
        match verdict {
            ExchangeVerdict::Accepted { rtt_ns, .. } => assert_eq!(rtt_ns, 400),
            other => panic!("expected an accepted exchange, got {:?}", other),
        }
    }

    #[test]
    fn every_nonsensical_shape_is_named_rather_than_averaged_in() {
        let ceiling = 100_000_000u64;
        assert!(matches!(
            nonsense(&exchange(500, 1, 2, 500), ceiling),
            Some(DiscardReason::ClientIntervalNotPositive { .. })
        ));
        assert!(matches!(
            nonsense(&exchange(0, 900, 100, 1_000), ceiling),
            Some(DiscardReason::ServerIntervalNegative { .. })
        ));
        assert!(matches!(
            nonsense(&exchange(0, 100, 5_000, 1_000), ceiling),
            Some(DiscardReason::RoundTripUnderflows { .. })
        ));
        assert!(matches!(
            nonsense(&exchange(0, 10, 20, 500_000_000), ceiling),
            Some(DiscardReason::RoundTripAboveCeiling { .. })
        ));
        assert_eq!(nonsense(&exchange(0, 100, 200, 1_000), ceiling), None);
    }

    #[test]
    fn a_fine_correction_becomes_frames_at_the_rate_it_names() {
        let mut corrector = PlayoutCorrector::new(48_000, 4);
        corrector.set_rate_correction(100.0);
        corrector.advance(1_000_000_000);
        // 100 ppm of a second at 48 kHz is 4.8 frames, and the fraction is
        // carried rather than lost.
        assert!((corrector.pending_frames() - 4.8).abs() < 1e-9);
        let shaped = corrector.shape(&vec![9u8; 960 * 4]);
        assert_eq!(shaped.len() / 4, 960 - 4, "four whole frames dropped");
        assert!((corrector.pending_frames() - 0.8).abs() < 1e-9);
        assert_eq!(corrector.dropped_frames(), 4);
    }

    #[test]
    fn a_negative_correction_inserts_silence_instead() {
        let mut corrector = PlayoutCorrector::new(48_000, 4);
        corrector.set_rate_correction(-100.0);
        corrector.advance(1_000_000_000);
        let shaped = corrector.shape(&vec![9u8; 960 * 4]);
        assert_eq!(shaped.len() / 4, 964);
        assert_eq!(&shaped[..4 * 4], &[0u8; 16], "the insertion is silence");
        assert_eq!(corrector.inserted_frames(), 4);
    }

    #[test]
    fn a_hard_resync_mutes_the_splice_without_moving_the_pointer_itself() {
        let mut corrector = PlayoutCorrector::new(48_000, 4);
        corrector.set_rate_correction(250.0);
        // 10 ms of step, forward, under a 5 ms mute.
        corrector.hard_resync(10_000_000.0, 5_000_000);
        assert_eq!(corrector.correction_ppm(), 0.0, "a step is not a slew");
        assert!((corrector.pending_frames() - 480.0).abs() < 1e-9);

        let shaped = corrector.shape(&vec![9u8; 960 * 4]);
        assert_eq!(shaped.len() / 4, 480, "480 frames of the chunk were dropped");
        // The mute is 240 frames and silences the front of what is left; it
        // adds no frames of its own, so the step is 480 frames and not 720.
        assert_eq!(corrector.muted_frames(), 240);
        assert!(shaped[..240 * 4].iter().all(|b| *b == 0));
        assert!(shaped[240 * 4..].iter().all(|b| *b == 9));
        assert!(!corrector.muted(), "the mute ran out, so playout resumed");
    }

    #[test]
    fn a_backward_step_larger_than_the_mute_still_mutes_real_audio() {
        let mut corrector = PlayoutCorrector::new(48_000, 4);
        // 40 ms of step, BACKWARD, under a 20 ms mute: the step is twice the
        // mute, so the inserted silence alone would swallow the whole mute.
        corrector.hard_resync(-40_000_000.0, 20_000_000);
        assert!((corrector.pending_frames() + 1_920.0).abs() < 1e-9);

        let shaped = corrector.shape(&vec![9u8; 3_840 * 4]);
        assert_eq!(shaped.len() / 4, 1_920 + 3_840, "1920 frames were inserted");
        assert_eq!(corrector.inserted_frames(), 1_920);
        assert_eq!(corrector.muted_frames(), 960, "20 ms of mute at 48 kHz");

        // The insertion is silent because it is an insertion, and the 960
        // frames after it are silent because they are the mute. What must NOT
        // happen is the mute landing inside the insertion, which would leave
        // audio resuming at full level straight off a 40 ms gap.
        assert!(shaped[..1_920 * 4].iter().all(|b| *b == 0), "the insertion");
        assert!(
            shaped[1_920 * 4..(1_920 + 960) * 4].iter().all(|b| *b == 0),
            "the mute lands on the audio that resumes, not on the insertion"
        );
        assert!(
            shaped[(1_920 + 960) * 4..].iter().all(|b| *b == 9),
            "and playout resumes at full level after it"
        );
        assert!(!corrector.muted(), "the mute ran out, so playout resumed");
    }

    #[test]
    fn a_mute_with_no_audio_left_to_cover_is_carried_to_the_next_chunk() {
        let mut corrector = PlayoutCorrector::new(48_000, 4);
        corrector.hard_resync(-20_000_000.0, 10_000_000);
        // A chunk that is entirely consumed by the insertion leaves nothing
        // after the splice, so the mute is owed still rather than spent.
        let shaped = corrector.shape(&[]);
        assert_eq!(shaped.len() / 4, 960, "the whole insertion, no audio");
        assert_eq!(corrector.muted_frames(), 0, "nothing real was muted yet");
        assert!(corrector.muted(), "the mute is still owed");

        let shaped = corrector.shape(&vec![9u8; 960 * 4]);
        assert_eq!(corrector.muted_frames(), 480, "10 ms at 48 kHz");
        assert!(shaped[..480 * 4].iter().all(|b| *b == 0));
        assert!(shaped[480 * 4..].iter().all(|b| *b == 9));
    }

    #[test]
    fn no_exchange_means_no_offset_and_no_bound_rather_than_zeroes() {
        let loop_ = SyncLoop::new(SyncConfig::default());
        let telemetry = loop_.telemetry(1_000);
        assert_eq!(telemetry.offset_ns, None);
        assert_eq!(telemetry.round_trip_ns, None);
        assert_eq!(telemetry.bound_ns, None);
        assert!(telemetry.line().contains("offset_ns=none"));
        assert!(telemetry.line().contains("bound_ns=none"));
    }
}
