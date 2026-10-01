//! Room-correction fitting: from a recording of an exponential sine sweep played in a room, a
//! handful of bounded peaking filters that flatten the room's low-frequency response toward a
//! target (goal 12, K31; the phone measurement that makes the recording is goal 22's, K87).
//!
//! The pipeline, each stage a public function so a test can hold it on its own:
//!
//! 1. [`Sweep`]: the excitation, an exponential sine sweep, and its inverse filter (Farina, "Simultaneous
//!    measurement of impulse response and distortion with a swept-sine technique", AES 108th
//!    Convention, Paris, 2000, preprint 5093,
//!    https://angelofarina.it/Public/Papers/134-AES00.PDF, read 2026-10-01).
//! 2. [`check_recording`]: a recording that is too short, clipped or too quiet is refused by
//!    name before anything is fitted to it ([`RoomFitError`]).
//! 3. [`impulse_response`]: the recording convolved with the inverse filter is the room's
//!    impulse response, with the harmonic distortion pushed to earlier times where the window
//!    leaves it out (Farina 2000, section 3).
//! 4. [`Spectrum`] and [`smooth`]: the windowed response's magnitude, power-averaged over a
//!    fractional octave around each point of a log-spaced grid.
//! 5. [`fit_response`]: a greedy fit of at most [`ROOM_EQ_MAX_FILTERS`] peaking filters to the
//!    smoothed deviation from a [`Target`], inside the `room_eq` bounds by construction, that
//!    prefers cuts, never lets the combined correction boost above the configured maximum at
//!    any frequency, and does not chase nulls.
//!
//! [`fit_recording`] runs all five. The filters are modelled with exactly the biquad design the
//! endpoints run ([`crate::biquad`], the RBJ peaking filter), evaluated in f64, at the integer
//! quanta the catalog and the wire carry (whole Hz, hundredths of a dB, thousandths of Q), so
//! what the fit predicts is what the room gets.
//!
//! Server-side analysis only: nothing here runs on the audio path, reads a clock or does I/O.

pub mod synthetic;

mod fft;

use crate::biquad::{Coefficients, Kind};
use std::f64::consts::PI;
use std::fmt;

// ---------------------------------------------------------------------------------------------
// The bounds
// ---------------------------------------------------------------------------------------------

/// A fitted correction filter is the chain's own [`RoomEqFilter`]: a peaking EQ in the integer
/// quanta the catalog's `room_eq` filter and the wire's `sound` message carry (whole Hz,
/// hundredths of a dB, thousandths of Q), so the fit's output needs no rounding on its way to
/// an endpoint. Its bounds are the `ROOM_EQ_*` constants of [`crate::settings`], the catalog's
/// own numbers (`crates/control` exports the same as `ROOM_EQ_*`, the C mirror as
/// `CHORUS_DSP_ROOM_EQ_*`).
pub use crate::settings::RoomEqFilter;
pub use crate::settings::{
    ROOM_EQ_FREQ_MAX_HZ, ROOM_EQ_FREQ_MIN_HZ, ROOM_EQ_GAIN_MAX_CDB, ROOM_EQ_GAIN_MIN_CDB,
    ROOM_EQ_MAX_FILTERS, ROOM_EQ_Q_MAX_MILLI, ROOM_EQ_Q_MIN_MILLI,
};

/// The filter's gain in dB.
pub fn gain_db(p: &RoomEqFilter) -> f64 {
    f64::from(p.gain_cdb) / 100.0
}

/// The filter's Q.
pub fn q(p: &RoomEqFilter) -> f64 {
    f64::from(p.q_milli) / 1000.0
}

/// True when every field of `p` is inside the `room_eq` bounds.
pub fn in_bounds(p: &RoomEqFilter) -> bool {
    (ROOM_EQ_FREQ_MIN_HZ..=ROOM_EQ_FREQ_MAX_HZ).contains(&p.freq_hz)
        && (ROOM_EQ_GAIN_MIN_CDB..=ROOM_EQ_GAIN_MAX_CDB).contains(&p.gain_cdb)
        && (ROOM_EQ_Q_MIN_MILLI..=ROOM_EQ_Q_MAX_MILLI).contains(&p.q_milli)
}

/// The filter's magnitude at `f` Hz, in dB, designed at `rate_hz` exactly as an endpoint
/// designs it.
pub fn filter_db(p: &RoomEqFilter, rate_hz: f64, f: f64) -> f64 {
    peaking_db(rate_hz, f64::from(p.freq_hz), gain_db(p), q(p), f)
}

/// The catalog's JSON spelling of one filter, `{"freq_hz":120,"gain_db":-3.5,"q":4.2}`: gain to
/// its 0.01 dB quantum and Q to its 0.001 quantum, trailing zeros dropped.
pub fn filter_json(p: &RoomEqFilter) -> String {
    format!(
        "{{\"freq_hz\":{},\"gain_db\":{},\"q\":{}}}",
        p.freq_hz,
        fixed(i64::from(p.gain_cdb), 2),
        fixed(i64::from(p.q_milli), 3)
    )
}

/// A fixed-point integer as the shortest decimal: `fixed(-350, 2)` is `-3.5`, `fixed(4000, 3)`
/// is `4`.
fn fixed(value: i64, places: u32) -> String {
    let scale = 10i64.pow(places);
    let sign = if value < 0 { "-" } else { "" };
    let whole = value.abs() / scale;
    let frac = value.abs() % scale;
    if frac == 0 {
        return format!("{sign}{whole}");
    }
    let digits = format!("{frac:0width$}", width = places as usize);
    format!("{sign}{whole}.{}", digits.trim_end_matches('0'))
}

/// The `room_eq` catalog command carrying `filters` for `zone`, enabled:
/// `{"type":"room_eq","zone":"...","filters":[...],"enabled":true}`. The zone is written as a
/// JSON string with `"` and `\` escaped; the catalog itself validates it.
pub fn room_eq_command_json(zone: &str, filters: &[RoomEqFilter]) -> String {
    let mut z = String::with_capacity(zone.len());
    for c in zone.chars() {
        match c {
            '"' => z.push_str("\\\""),
            '\\' => z.push_str("\\\\"),
            c if (c as u32) < 0x20 => z.push_str(&format!("\\u{:04x}", c as u32)),
            c => z.push(c),
        }
    }
    let list: Vec<String> = filters.iter().map(filter_json).collect();
    format!(
        "{{\"type\":\"room_eq\",\"zone\":\"{z}\",\"filters\":[{}],\"enabled\":true}}",
        list.join(",")
    )
}

/// The RBJ peaking filter's magnitude at `f`, through the crate's own biquad design: the one
/// place the fit models a filter, so it can only ever model the one the endpoints run.
fn peaking_db(rate_hz: f64, f0: f64, gain_db: f64, q: f64, f: f64) -> f64 {
    // The design refuses only an f0 outside (0, rate/2), a Q that is not positive or a gain that
    // is not finite; the fit's quanta are never any of those, so the 0 dB fallback is never
    // taken.
    Coefficients::design(Kind::Peaking, rate_hz, f0, q, gain_db)
        .map_or(0.0, |c| c.magnitude_db(f, rate_hz))
}

// ---------------------------------------------------------------------------------------------
// The sweep
// ---------------------------------------------------------------------------------------------

/// An exponential sine sweep (Farina 2000, section 4):
///
/// x(t) = A sin[ (w1 T / ln(w2/w1)) (e^((t/T) ln(w2/w1)) - 1) ],  0 <= t < T,
///
/// whose instantaneous frequency, the derivative of the phase, is
/// f(t) = f1 e^((t/T) ln(f2/f1)): it rises by the same ratio every second, so every octave gets
/// the same time and the same energy (a pink spectrum).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sweep {
    /// Sample rate, Hz.
    pub rate_hz: u32,
    /// Start frequency, Hz.
    pub f1_hz: f64,
    /// End frequency, Hz.
    pub f2_hz: f64,
    /// Length in samples (T = samples / rate).
    pub samples: usize,
    /// Peak amplitude, full scale = 1.
    pub amplitude: f64,
    /// A half-raised-cosine fade-in over the first this-many samples. There is no fade-out: the
    /// sweep stops at its last zero crossing instead, as Farina recommends ("Advancements in
    /// impulse response measurements by sine sweeps", AES 122nd Convention, Vienna, 2007, paper
    /// 7121, section on pre-ringing, https://angelofarina.it/Public/Papers/226-AES122.pdf, read
    /// 2026-10-01), because a fade at either end rings in the deconvolved response.
    pub fade_in_samples: usize,
}

impl Sweep {
    /// The sweep goal 22's measurement starts from: 10 Hz to 20 kHz over 5 s at half full scale
    /// with a 0.1 s fade-in (the research's recommendation, `docs/research/room-correction-
    /// sources.md` 1a; ASSUMED values: 10 Hz puts the fade-in an octave below the fit band, 5 s
    /// keeps a 100 ppm clock mismatch between speaker and phone under a millisecond and the
    /// second harmonic 0.46 s ahead of the linear response, half full scale leaves headroom
    /// for the room's modes).
    pub fn recommended(rate_hz: u32) -> Sweep {
        Sweep {
            rate_hz,
            f1_hz: 10.0,
            f2_hz: 20_000.0,
            samples: 5 * rate_hz as usize,
            amplitude: 0.5,
            fade_in_samples: rate_hz as usize / 10,
        }
    }

    /// The duration T in seconds.
    pub fn duration_s(&self) -> f64 {
        self.samples as f64 / f64::from(self.rate_hz)
    }

    /// Farina's L = T / ln(w2/w1): the time the sweep takes to rise by a factor of e. The N-th
    /// harmonic's response arrives L ln(N) before the linear one after deconvolution.
    pub fn rate_constant_s(&self) -> f64 {
        self.duration_s() / (self.f2_hz / self.f1_hz).ln()
    }

    /// The instantaneous frequency at `t` seconds, f1 e^(t / L).
    pub fn instantaneous_hz(&self, t: f64) -> f64 {
        self.f1_hz * (t / self.rate_constant_s()).exp()
    }

    /// The sweep's samples.
    pub fn signal(&self) -> Vec<f64> {
        let rate = f64::from(self.rate_hz);
        let l = self.rate_constant_s();
        // K = w1 L, so the phase is K (e^(t/L) - 1).
        let k = 2.0 * PI * self.f1_hz * l;
        let fade = self.fade_in_samples.min(self.samples);
        let mut x: Vec<f64> = (0..self.samples)
            .map(|n| {
                let t = n as f64 / rate;
                let mut v = self.amplitude * (k * ((t / l).exp() - 1.0)).sin();
                if n < fade {
                    v *= 0.5 - 0.5 * (PI * n as f64 / fade as f64).cos();
                }
                v
            })
            .collect();
        // Stop at the last zero crossing: everything after the last sign change is silence.
        if let Some(last) = (1..x.len())
            .rev()
            .find(|&n| (x[n - 1] < 0.0) != (x[n] < 0.0))
        {
            // Of the two samples either side of the crossing, keep the one nearer zero as the
            // last sound.
            let cut = if x[last].abs() < x[last - 1].abs() {
                last + 1
            } else {
                last
            };
            x[cut..].iter_mut().for_each(|v| *v = 0.0);
        }
        x
    }

    /// The inverse filter (Farina 2000, sections 3 and 6): the sweep reversed in time, with an
    /// amplitude envelope that falls 6 dB per octave as the reversed sweep's frequency falls,
    /// "starting from 0 dB and ending to -6 log2(w2/w1)" (the envelope f / f2). The sweep spends
    /// equal time per octave, so its spectrum falls 3 dB per octave; the reversed sweep's does
    /// too; the envelope's +6 dB per octave makes their product flat. Scaled so the sweep
    /// convolved with it is a unit impulse in the band (see [`Sweep::inverse_scale`]).
    pub fn inverse(&self) -> Vec<f64> {
        let rate = f64::from(self.rate_hz);
        let x = self.signal();
        let n = x.len();
        let mut inv: Vec<f64> = (0..n)
            .map(|i| {
                let src = n - 1 - i;
                let f = self.instantaneous_hz(src as f64 / rate);
                x[src] * f / self.f2_hz
            })
            .collect();
        let scale = self.inverse_scale(&x, &inv);
        for v in inv.iter_mut() {
            *v *= scale;
        }
        inv
    }

    /// The factor that makes sweep (*) inverse have unit gain: the reciprocal of the mean
    /// magnitude of their product spectrum over the band two octaves inside the sweep's ends
    /// (one octave from each end, where neither the start nor the fade disturbs it).
    fn inverse_scale(&self, x: &[f64], inv: &[f64]) -> f64 {
        let n = fft::next_pow2(x.len() + inv.len() - 1);
        let mut fx = fft::padded(x, n);
        let mut fi = fft::padded(inv, n);
        fft::forward(&mut fx);
        fft::forward(&mut fi);
        let df = f64::from(self.rate_hz) / n as f64;
        let lo = (2.0 * self.f1_hz / df).ceil() as usize;
        let hi = ((self.f2_hz / 2.0) / df).floor() as usize;
        let mut sum = 0.0;
        let mut count = 0usize;
        for k in lo..=hi.max(lo) {
            sum += fx[k].mul(fi[k]).norm_sqr().sqrt();
            count += 1;
        }
        count as f64 / sum
    }
}

// ---------------------------------------------------------------------------------------------
// Refusals
// ---------------------------------------------------------------------------------------------

/// Why a recording cannot be fitted. Each names the problem and carries the numbers, so the
/// measurement UX (goal 22) can tell the person what to do about it.
#[derive(Debug, Clone, PartialEq)]
pub enum RoomFitError {
    /// Shorter than the sweep plus the response window: the sweep was cut off, or the room's
    /// decay was.
    TooShort { samples: usize, needed: usize },
    /// A run of samples at full scale: the microphone or its converter clipped, and the
    /// response is not the room's.
    Clipped { at_sample: usize, run: usize },
    /// The loudest sample is below [`FitConfig::min_peak_dbfs`]: the sweep was too quiet (or
    /// not playing).
    TooQuiet { peak_dbfs: f64, min_dbfs: f64 },
    /// The impulse response's peak stands less than [`FitConfig::min_snr_db`] above the noise
    /// before it: too much background noise for the fit to tell modes from noise.
    TooNoisy { snr_db: f64, min_db: f64 },
    /// The sweep or configuration cannot be used (a bound outside the catalog's, a sweep that
    /// does not cover the fit band); a programming error rather than a bad recording.
    BadConfig(String),
}

impl RoomFitError {
    /// The refusal's name: `too_short`, `clipped`, `too_quiet`, `too_noisy`, `bad_config`.
    pub fn name(&self) -> &'static str {
        match self {
            RoomFitError::TooShort { .. } => "too_short",
            RoomFitError::Clipped { .. } => "clipped",
            RoomFitError::TooQuiet { .. } => "too_quiet",
            RoomFitError::TooNoisy { .. } => "too_noisy",
            RoomFitError::BadConfig(_) => "bad_config",
        }
    }
}

impl fmt::Display for RoomFitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RoomFitError::TooShort { samples, needed } => write!(
                f,
                "too_short: the recording is {samples} samples, at least {needed} are needed \
                 (the whole sweep and the room's decay after it)"
            ),
            RoomFitError::Clipped { at_sample, run } => write!(
                f,
                "clipped: {run} consecutive samples at full scale from sample {at_sample}; \
                 measure again with the volume lower"
            ),
            RoomFitError::TooQuiet {
                peak_dbfs,
                min_dbfs,
            } => write!(
                f,
                "too_quiet: the recording peaks at {peak_dbfs:.1} dBFS, below {min_dbfs:.1} \
                 dBFS; measure again with the volume higher"
            ),
            RoomFitError::TooNoisy { snr_db, min_db } => write!(
                f,
                "too_noisy: the response stands {snr_db:.1} dB above the noise, below \
                 {min_db:.1} dB; measure again in a quieter room or louder"
            ),
            RoomFitError::BadConfig(why) => write!(f, "bad_config: {why}"),
        }
    }
}

impl std::error::Error for RoomFitError {}

// ---------------------------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------------------------

/// Everything the fit is tuned by. [`FitConfig::default`] is the value goal 22's measurement
/// starts from; every number in it is cited or ASSUMED in `docs/room-correction.md`.
#[derive(Debug, Clone, PartialEq)]
pub struct FitConfig {
    /// The fit band's lower edge, Hz. ASSUMED 20 Hz: the catalog's lower bound; the fit raises
    /// it to the speaker's own low-frequency extension (see `extension_drop_db`).
    pub band_lo_hz: f64,
    /// The fit band's upper edge, Hz: the modal region, below which a room's response is
    /// dominated by separate standing waves a minimum-phase filter can address, and above which
    /// it is a dense statistical sum that varies from seat to seat (the Schroeder frequency,
    /// f_s = 2000 sqrt(T60 / V)). ASSUMED 300 Hz: f_s for a small, dry living room; see
    /// `docs/room-correction.md` for the arithmetic.
    pub band_hi_hz: f64,
    /// The level reference band, Hz: the response's median level here is what the target is
    /// placed against (ASSUMED 300 Hz to 3 kHz: above the modal region, inside the band every
    /// phone microphone records flat enough).
    pub level_band_hz: (f64, f64),
    /// Smoothing, as 1/N octave (ASSUMED N = 12: fine enough to keep a mode's shape, coarse
    /// enough to average out the seat-to-seat ripple).
    pub smoothing_fraction: f64,
    /// Grid points per octave the fit is evaluated on (ASSUMED 48).
    pub points_per_octave: f64,
    /// The largest combined boost the correction may apply at any frequency, dB (REW's
    /// "Overall Max Boost", https://www.roomeqwizard.com/help/help_en-GB/html/eqwindow.html,
    /// read 2026-10-01). At most the catalog's +3.00 dB; ASSUMED equal to it.
    pub max_boost_db: f64,
    /// No boosting filter is centred below this, Hz (ASSUMED 100 Hz): a phone microphone's
    /// response below 100 Hz is bounded only to +-20 dB of its midband even on an Android
    /// device's unprocessed source (Android 11 CDD section 5.11,
    /// https://source.android.com/docs/compatibility/11/android-11-cdd, read 2026-10-01), so a
    /// dip there may be the microphone's and boosting it would only add excursion.
    pub min_boost_hz: f64,
    /// A dip that reaches this many dB below the target is a null: a cancellation that boost
    /// cannot fill (the boost cancels as well). The whole dip around it is left alone: the fit
    /// pays for any change to it, boost or cut (ASSUMED 6 dB).
    pub null_depth_db: f64,
    /// The weight of a dip's error relative to a peak's (ASSUMED 0.5): the fit prefers cuts.
    pub dip_weight: f64,
    /// The fit stops once no weighted deviation in the band exceeds this, dB (ASSUMED 1.0).
    pub tolerance_db: f64,
    /// A filter whose gain the fit refines to less than this is not worth a slot, and the fit
    /// stops, dB (ASSUMED 0.5).
    pub min_gain_db: f64,
    /// The band's lower edge rises to the lowest frequency at which the smoothed response comes
    /// within this many dB of the level (ASSUMED 3 dB, the speaker's -3 dB point): below it is
    /// the speaker's roll-off, which a correction must not boost. The half octave above that
    /// edge, where the roll-off is still under way, is left alone like a null where it is
    /// below the target (ASSUMED half an octave): lifting it would only extend the speaker
    /// downward at the cost of its excursion.
    pub extension_drop_db: f64,
    /// The impulse response's window after its peak, seconds (ASSUMED 0.5 s: a mode of Q 10 at
    /// 30 Hz decays by 60 dB in 6.9 Q / (pi f), about 0.73 s, so 0.5 s keeps more than 40 dB
    /// of every mode the bounds can address, and gives 2 Hz resolution).
    pub window_s: f64,
    /// The window before the peak, seconds (ASSUMED 5 ms), faded in with a half raised cosine.
    pub pre_window_s: f64,
    /// A recording whose loudest sample is below this is refused, dBFS (ASSUMED -50 dBFS).
    pub min_peak_dbfs: f64,
    /// A response whose peak stands less than this above the noise before it is refused, dB
    /// (ASSUMED 40 dB).
    pub min_snr_db: f64,
    /// A run of at least this many consecutive samples at or above `clip_level` is clipping
    /// (ASSUMED 3 samples at 0.999 full scale; one full-scale sample can be a peak, three in a
    /// row are a converter's rail).
    pub clip_run: usize,
    /// See `clip_run`.
    pub clip_level: f64,
    /// The sample rate the filters are modelled at, Hz (ASSUMED 48 kHz, the stream rate; at
    /// the fit's frequencies a peaking filter designed at 44.1 or 96 kHz differs from it by
    /// well under the 0.01 dB quantum).
    pub model_rate_hz: f64,
}

impl Default for FitConfig {
    fn default() -> FitConfig {
        FitConfig {
            band_lo_hz: 20.0,
            band_hi_hz: 300.0,
            level_band_hz: (300.0, 3000.0),
            smoothing_fraction: 12.0,
            points_per_octave: 48.0,
            max_boost_db: 3.0,
            min_boost_hz: 100.0,
            null_depth_db: 6.0,
            dip_weight: 0.5,
            tolerance_db: 1.0,
            min_gain_db: 0.5,
            extension_drop_db: 3.0,
            window_s: 0.5,
            pre_window_s: 0.005,
            min_peak_dbfs: -50.0,
            min_snr_db: 40.0,
            clip_run: 3,
            clip_level: 0.999,
            model_rate_hz: 48_000.0,
        }
    }
}

impl FitConfig {
    fn validate(&self) -> Result<(), RoomFitError> {
        let bad = |s: String| Err(RoomFitError::BadConfig(s));
        let (flo, fhi) = (
            f64::from(ROOM_EQ_FREQ_MIN_HZ),
            f64::from(ROOM_EQ_FREQ_MAX_HZ),
        );
        if !(self.band_lo_hz >= flo && self.band_hi_hz <= fhi && self.band_lo_hz < self.band_hi_hz)
        {
            return bad(format!(
                "the fit band {}..{} Hz is not inside the room_eq bounds {flo}..{fhi} Hz",
                self.band_lo_hz, self.band_hi_hz
            ));
        }
        let boost_max = f64::from(ROOM_EQ_GAIN_MAX_CDB) / 100.0;
        if !(0.0..=boost_max).contains(&self.max_boost_db) {
            return bad(format!(
                "max_boost_db {} is outside 0..{boost_max}",
                self.max_boost_db
            ));
        }
        if self.smoothing_fraction <= 0.0 || self.points_per_octave <= 0.0 || self.window_s <= 0.0 {
            return bad("smoothing, grid density and window must be positive".to_string());
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// The recording and its impulse response
// ---------------------------------------------------------------------------------------------

/// Refuses a recording that cannot be fitted: too short to hold the sweep and the window after
/// it, clipped, or too quiet. (Noise is judged after deconvolution, in [`impulse_response`].)
pub fn check_recording(
    recording: &[f32],
    sweep: &Sweep,
    cfg: &FitConfig,
) -> Result<(), RoomFitError> {
    let window = (cfg.window_s * f64::from(sweep.rate_hz)).round() as usize;
    let needed = sweep.samples + window;
    if recording.len() < needed {
        return Err(RoomFitError::TooShort {
            samples: recording.len(),
            needed,
        });
    }
    let mut run = 0usize;
    for (i, s) in recording.iter().enumerate() {
        if f64::from(s.abs()) >= cfg.clip_level {
            run += 1;
            if run >= cfg.clip_run {
                // Report the whole run, from its start.
                let start = i + 1 - run;
                let mut end = i + 1;
                while end < recording.len() && f64::from(recording[end].abs()) >= cfg.clip_level {
                    end += 1;
                }
                return Err(RoomFitError::Clipped {
                    at_sample: start,
                    run: end - start,
                });
            }
        } else {
            run = 0;
        }
    }
    let peak = recording
        .iter()
        .fold(0.0f64, |m, s| m.max(f64::from(s.abs())));
    let peak_dbfs = if peak > 0.0 {
        20.0 * peak.log10()
    } else {
        f64::NEG_INFINITY
    };
    if peak_dbfs < cfg.min_peak_dbfs {
        return Err(RoomFitError::TooQuiet {
            peak_dbfs,
            min_dbfs: cfg.min_peak_dbfs,
        });
    }
    Ok(())
}

/// The room's impulse response, as deconvolution found it.
#[derive(Debug, Clone, PartialEq)]
pub struct ImpulseResponse {
    /// Sample rate, Hz.
    pub rate_hz: u32,
    /// The windowed response, starting `pre_window_s` before its peak.
    pub samples: Vec<f64>,
    /// Where the peak fell in the recording, in samples after the sweep's start would have
    /// been with no delay: the playback-to-capture latency plus the sound's flight.
    pub delay_samples: usize,
    /// The peak's height above the RMS of the noise before it (between the second harmonic's
    /// arrival and the window), dB.
    pub snr_db: f64,
}

/// Convolves the recording with the sweep's inverse filter (Farina 2000, section 3), finds the
/// linear response's peak at or after the sweep's length, and cuts the window around it. The
/// harmonic distortion products land L ln(N) before the peak (Farina 2000, section 4), at least
/// L ln 2 before it, which is far more than the pre-window, so the window holds the linear
/// response alone.
pub fn impulse_response(
    recording: &[f32],
    sweep: &Sweep,
    cfg: &FitConfig,
) -> Result<ImpulseResponse, RoomFitError> {
    cfg.validate()?;
    check_recording(recording, sweep, cfg)?;
    let rate = f64::from(sweep.rate_hz);
    let rec: Vec<f64> = recording.iter().map(|s| f64::from(*s)).collect();
    let full = fft::convolve(&rec, &sweep.inverse());
    let window = (cfg.window_s * rate).round() as usize;
    let pre = (cfg.pre_window_s * rate).round() as usize;
    // The linear response of a system with no delay peaks at index samples - 1; latency only
    // moves it later. Search everywhere a whole window after the peak still fits.
    let first = sweep.samples - 1;
    let last = rec.len().saturating_sub(window).max(first);
    let mut peak_at = first;
    let mut peak = 0.0f64;
    for (i, v) in full.iter().enumerate().take(last + 1).skip(first) {
        if v.abs() > peak {
            peak = v.abs();
            peak_at = i;
        }
    }
    // Noise: between the second harmonic's arrival (L ln 2 before the peak, plus a margin of the
    // pre-window) and the pre-window. Empty when the sweep is too short for one, which only a
    // configuration far from the default can make.
    let l_ln2 = (sweep.rate_constant_s() * 2f64.ln() * rate) as usize;
    let noise_lo = peak_at.saturating_sub(l_ln2.saturating_sub(2 * pre));
    let noise_hi = peak_at.saturating_sub(2 * pre);
    let snr_db = if noise_hi > noise_lo {
        let p: f64 = full[noise_lo..noise_hi].iter().map(|v| v * v).sum::<f64>()
            / (noise_hi - noise_lo) as f64;
        if p > 0.0 {
            10.0 * (peak * peak / p).log10()
        } else {
            f64::INFINITY
        }
    } else {
        f64::INFINITY
    };
    if snr_db < cfg.min_snr_db {
        return Err(RoomFitError::TooNoisy {
            snr_db,
            min_db: cfg.min_snr_db,
        });
    }
    let start = peak_at.saturating_sub(pre);
    let end = (peak_at + window).min(full.len());
    let mut samples: Vec<f64> = full[start..end].to_vec();
    let lead = peak_at - start;
    for (i, v) in samples.iter_mut().enumerate().take(lead) {
        *v *= 0.5 - 0.5 * (PI * i as f64 / lead as f64).cos();
    }
    // The last quarter of the window fades out (a half raised cosine), so the cut does not add
    // a step's ripple to the spectrum (ASSUMED a quarter).
    let n = samples.len();
    let tail = (n - lead) / 4;
    for i in 0..tail {
        let w = 0.5 - 0.5 * (PI * i as f64 / tail as f64).cos();
        samples[n - 1 - i] *= w;
    }
    Ok(ImpulseResponse {
        rate_hz: sweep.rate_hz,
        samples,
        delay_samples: peak_at - first,
        snr_db,
    })
}

// ---------------------------------------------------------------------------------------------
// Magnitude and smoothing
// ---------------------------------------------------------------------------------------------

/// A magnitude spectrum on the transform's evenly spaced bins.
#[derive(Debug, Clone, PartialEq)]
pub struct Spectrum {
    /// Bin spacing, Hz.
    pub bin_hz: f64,
    /// |H(k bin_hz)|^2 for k = 0 ..= N/2.
    pub power: Vec<f64>,
}

impl Spectrum {
    /// The impulse response's spectrum, zero-padded to at least 2^16 points (ASSUMED: 0.73 Hz
    /// bins at 48 kHz, so a 1/12-octave band at 20 Hz still spans a bin) and its own length.
    pub fn of(ir: &ImpulseResponse) -> Spectrum {
        let n = fft::next_pow2(ir.samples.len().max(1 << 16));
        let mut data = fft::padded(&ir.samples, n);
        fft::forward(&mut data);
        Spectrum {
            bin_hz: f64::from(ir.rate_hz) / n as f64,
            power: data[..=n / 2].iter().map(|z| z.norm_sqr()).collect(),
        }
    }

    /// The power at `f` Hz, linearly interpolated between the bins either side.
    fn power_at(&self, f: f64) -> f64 {
        let x = (f / self.bin_hz).max(0.0);
        let k = x.floor() as usize;
        if k + 1 >= self.power.len() {
            return *self.power.last().unwrap_or(&0.0);
        }
        let t = x - k as f64;
        self.power[k] * (1.0 - t) + self.power[k + 1] * t
    }
}

/// `per_octave` log-spaced frequencies from `lo` to `hi` Hz inclusive of `lo`.
pub fn log_grid(lo: f64, hi: f64, per_octave: f64) -> Vec<f64> {
    let count = ((hi / lo).log2() * per_octave).floor() as usize;
    (0..=count)
        .map(|i| lo * 2f64.powf(i as f64 / per_octave))
        .collect()
}

/// Fractional-octave smoothing: at each frequency f of `grid`, the mean power of the bins
/// between f 2^(-1/2N) and f 2^(1/2N) (a rectangular 1/N-octave window, the constant-relative-
/// bandwidth average of Hatziantoniou and Mourjopoulos, "Generalized Fractional-Octave
/// Smoothing of Audio and Acoustic Responses", JAES 48(4), 2000, pp. 259-280,
/// https://aes2.org/publications/elibrary-page/?id=12070, its abstract read 2026-10-01; the
/// rectangular window is ASSUMED, the paper also gives shaped ones), in dB. Power, not dB, is
/// averaged, so a narrow null is not given more weight than the energy it holds. A band
/// narrower than a bin takes the interpolated power at f.
pub fn smooth(spectrum: &Spectrum, grid: &[f64], fraction: f64) -> Vec<f64> {
    let half = 2f64.powf(1.0 / (2.0 * fraction));
    // Prefix sums make each band's mean O(1).
    let mut prefix = Vec::with_capacity(spectrum.power.len() + 1);
    prefix.push(0.0);
    let mut acc = 0.0;
    for p in &spectrum.power {
        acc += p;
        prefix.push(acc);
    }
    grid.iter()
        .map(|&f| {
            let lo = ((f / half) / spectrum.bin_hz).ceil() as usize;
            let hi = (((f * half) / spectrum.bin_hz).floor() as usize)
                .min(spectrum.power.len().saturating_sub(1));
            let p = if hi >= lo {
                (prefix[hi + 1] - prefix[lo]) / (hi + 1 - lo) as f64
            } else {
                spectrum.power_at(f)
            };
            10.0 * p.max(1e-30).log10()
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// The target
// ---------------------------------------------------------------------------------------------

/// The response the correction aims for, relative to the measured level: (Hz, dB) points
/// interpolated linearly in log frequency, held flat beyond the ends.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    points: Vec<(f64, f64)>,
}

impl Target {
    /// Flat: 0 dB everywhere (ASSUMED as the default; a house curve with a low-frequency rise is
    /// a preference goal 22's UX can offer through [`Target::new`]).
    pub fn flat() -> Target {
        Target {
            points: vec![(1000.0, 0.0)],
        }
    }

    /// A target through `points`, sorted by frequency; panics on an empty list or a frequency
    /// that is not positive.
    pub fn new(mut points: Vec<(f64, f64)>) -> Target {
        assert!(!points.is_empty() && points.iter().all(|p| p.0 > 0.0));
        points.sort_by(|a, b| a.0.total_cmp(&b.0));
        Target { points }
    }

    /// The target at `f` Hz, dB.
    pub fn at(&self, f: f64) -> f64 {
        let p = &self.points;
        if f <= p[0].0 {
            return p[0].1;
        }
        for w in p.windows(2) {
            if f <= w[1].0 {
                let t = (f / w[0].0).ln() / (w[1].0 / w[0].0).ln();
                return w[0].1 + t * (w[1].1 - w[0].1);
            }
        }
        p[p.len() - 1].1
    }
}

// ---------------------------------------------------------------------------------------------
// The fit
// ---------------------------------------------------------------------------------------------

/// What a fit found and did.
#[derive(Debug, Clone, PartialEq)]
pub struct Fit {
    /// The filters, at most [`ROOM_EQ_MAX_FILTERS`], each inside the bounds, sorted by
    /// frequency.
    pub filters: Vec<RoomEqFilter>,
    /// The response's level in the level band (the median of the smoothed response), dB.
    pub level_db: f64,
    /// The band the fit worked in, Hz, after the low edge followed the speaker's extension.
    pub band_hz: (f64, f64),
    /// The grid the fit was evaluated on.
    pub grid_hz: Vec<f64>,
    /// The smoothed response minus level minus target on the grid, dB, before correction.
    pub deviation_db: Vec<f64>,
    /// The same after the filters, dB (deviation plus the correction).
    pub residual_db: Vec<f64>,
    /// True where the grid point is left alone: in a null's dip (see
    /// [`FitConfig::null_depth_db`]) or below the target in the speaker's roll-off (see
    /// [`FitConfig::extension_drop_db`]).
    pub left_alone: Vec<bool>,
}

impl Fit {
    /// The RMS of the deviation over the grid before the correction, dB, outside the points
    /// left alone (which no correction is meant to touch, and a null would otherwise dominate
    /// the figure).
    pub fn rms_before_db(&self) -> f64 {
        rms(&self.outside_nulls(&self.deviation_db))
    }

    /// The RMS of the deviation over the grid after the correction, dB, outside the points left
    /// alone.
    pub fn rms_after_db(&self) -> f64 {
        rms(&self.outside_nulls(&self.residual_db))
    }

    fn outside_nulls(&self, v: &[f64]) -> Vec<f64> {
        v.iter()
            .zip(&self.left_alone)
            .filter(|(_, n)| !**n)
            .map(|(x, _)| *x)
            .collect()
    }

    /// The combined correction at `f` Hz, dB, modelled at `rate_hz`.
    pub fn correction_db(&self, rate_hz: f64, f: f64) -> f64 {
        self.filters.iter().map(|p| filter_db(p, rate_hz, f)).sum()
    }
}

fn rms(v: &[f64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    (v.iter().map(|x| x * x).sum::<f64>() / v.len() as f64).sqrt()
}

/// The whole pipeline: refuse, deconvolve, smooth, fit.
pub fn fit_recording(
    recording: &[f32],
    sweep: &Sweep,
    target: &Target,
    cfg: &FitConfig,
) -> Result<Fit, RoomFitError> {
    cfg.validate()?;
    let need_hi = cfg.band_hi_hz.max(cfg.level_band_hz.1);
    if sweep.f1_hz > cfg.band_lo_hz || sweep.f2_hz < need_hi {
        return Err(RoomFitError::BadConfig(format!(
            "the sweep covers {}..{} Hz, the fit needs {}..{} Hz",
            sweep.f1_hz, sweep.f2_hz, cfg.band_lo_hz, need_hi
        )));
    }
    let ir = impulse_response(recording, sweep, cfg)?;
    let spectrum = Spectrum::of(&ir);
    // One grid from the band's low edge to the level band's top: the fit uses the lower part,
    // the level the upper.
    let grid = log_grid(cfg.band_lo_hz, cfg.level_band_hz.1, cfg.points_per_octave);
    let smoothed = smooth(&spectrum, &grid, cfg.smoothing_fraction);
    fit_response(&grid, &smoothed, target, cfg)
}

/// Fits the correction to a smoothed response (`response_db` on `grid_hz`, ascending). Public
/// so a response from elsewhere (a test's model, a future averaged multi-seat measurement) can
/// be fitted the same way.
pub fn fit_response(
    grid_hz: &[f64],
    response_db: &[f64],
    target: &Target,
    cfg: &FitConfig,
) -> Result<Fit, RoomFitError> {
    cfg.validate()?;
    let mut level: Vec<f64> = grid_hz
        .iter()
        .zip(response_db)
        .filter(|(f, _)| **f >= cfg.level_band_hz.0 && **f <= cfg.level_band_hz.1)
        .map(|(_, d)| *d)
        .collect();
    if level.is_empty() {
        return Err(RoomFitError::BadConfig(
            "the grid has no point in the level band".to_string(),
        ));
    }
    level.sort_by(f64::total_cmp);
    let level_db = level[level.len() / 2];
    // The band's low edge follows the speaker's extension: the first point from the bottom
    // that comes within extension_drop_db of the level.
    let lo = grid_hz
        .iter()
        .zip(response_db)
        .find(|(f, d)| **f >= cfg.band_lo_hz && **d >= level_db - cfg.extension_drop_db)
        .map(|(f, _)| *f)
        .unwrap_or(cfg.band_lo_hz)
        .min(cfg.band_hi_hz);
    let hi = cfg.band_hi_hz;
    let (grid, deviation): (Vec<f64>, Vec<f64>) = grid_hz
        .iter()
        .zip(response_db)
        .filter(|(f, _)| **f >= lo && **f <= hi)
        .map(|(f, d)| (*f, d - level_db - target.at(*f)))
        .unzip();
    let mut left_alone = null_mask(&deviation, cfg.null_depth_db);
    let rolloff_top = lo * 2f64.sqrt();
    for ((m, f), d) in left_alone.iter_mut().zip(&grid).zip(&deviation) {
        if *f < rolloff_top && *d < 0.0 {
            *m = true;
        }
    }
    let mut fitter = Fitter::new(&grid, &deviation, &left_alone, cfg);
    fitter.run();
    let mut filters = fitter.filters();
    filters.sort_by_key(|p| p.freq_hz);
    let residual: Vec<f64> = grid
        .iter()
        .zip(&deviation)
        .map(|(f, d)| {
            d + filters
                .iter()
                .map(|p| filter_db(p, cfg.model_rate_hz, *f))
                .sum::<f64>()
        })
        .collect();
    Ok(Fit {
        filters,
        level_db,
        band_hz: (lo, hi),
        grid_hz: grid,
        deviation_db: deviation,
        residual_db: residual,
        left_alone,
    })
}

/// Marks every point of every dip (a run of points below the target) that reaches
/// `depth` dB below it.
fn null_mask(dev: &[f64], depth: f64) -> Vec<bool> {
    let mut mask = vec![false; dev.len()];
    let mut i = 0;
    while i < dev.len() {
        if dev[i] >= 0.0 {
            i += 1;
            continue;
        }
        let start = i;
        while i < dev.len() && dev[i] < 0.0 {
            i += 1;
        }
        if dev[start..i].iter().any(|d| *d <= -depth) {
            mask[start..i].iter_mut().for_each(|m| *m = true);
        }
    }
    mask
}

/// A filter while it is being fitted: the integer quanta, plus its response on the fit grid
/// and on the boost-check grid, kept so moving one filter costs one filter's evaluation.
#[derive(Debug, Clone)]
struct Cand {
    f: i32,
    g: i32,
    q: i32,
    on_fit: Vec<f64>,
    on_check: Vec<f64>,
}

/// The greedy fit. Each round adds the filter that best addresses the largest remaining
/// weighted deviation (a peak before a dip of the same size, by `dip_weight`), refines it,
/// then refines every filter together, and keeps the round only if the cost fell by more than
/// 1% (ASSUMED). Every move is on the integer quanta and is checked against the bounds and the
/// boost ceiling before it is taken, so no state the fit ever holds is outside them.
struct Fitter<'a> {
    grid: &'a [f64],
    dev: &'a [f64],
    left_alone: &'a [bool],
    cfg: &'a FitConfig,
    /// Where the combined boost is checked: the fit grid and a wide grid from 10 Hz to 20 kHz
    /// (ASSUMED 24 per octave), plus each boosting filter's own centre (where a peaking boost
    /// is largest) at check time.
    check: Vec<f64>,
    cands: Vec<Cand>,
}

impl<'a> Fitter<'a> {
    fn new(
        grid: &'a [f64],
        dev: &'a [f64],
        left_alone: &'a [bool],
        cfg: &'a FitConfig,
    ) -> Fitter<'a> {
        let mut check = grid.to_vec();
        check.extend(log_grid(10.0, 20_000.0, 24.0));
        Fitter {
            grid,
            dev,
            left_alone,
            cfg,
            check,
            cands: Vec::new(),
        }
    }

    fn filters(&self) -> Vec<RoomEqFilter> {
        self.cands
            .iter()
            .filter(|c| c.g != 0)
            .map(|c| RoomEqFilter {
                freq_hz: c.f as u16,
                gain_cdb: c.g as i16,
                q_milli: c.q as u16,
            })
            .collect()
    }

    fn make(&self, f: i32, g: i32, q: i32) -> Cand {
        let rate = self.cfg.model_rate_hz;
        let (ff, gg, qq) = (f64::from(f), f64::from(g) / 100.0, f64::from(q) / 1000.0);
        let eval = |x: &f64| peaking_db(rate, ff, gg, qq, *x);
        Cand {
            f,
            g,
            q,
            on_fit: self.grid.iter().map(eval).collect(),
            on_check: self.check.iter().map(eval).collect(),
        }
    }

    /// The quanta a candidate may take: the catalog's bounds, the fit band for the frequency
    /// (rounded inward), and the configured boost ceiling for the gain. The boost rules that
    /// depend on more than one quantum are in `allowed`.
    fn limits(&self) -> [(i32, i32); 3] {
        let lo = (self.grid.first().copied().unwrap_or(20.0).ceil() as i32)
            .max(i32::from(ROOM_EQ_FREQ_MIN_HZ));
        let hi = (self.grid.last().copied().unwrap_or(1000.0).floor() as i32)
            .min(i32::from(ROOM_EQ_FREQ_MAX_HZ))
            .max(lo);
        let gmax =
            ((self.cfg.max_boost_db * 100.0).floor() as i32).min(i32::from(ROOM_EQ_GAIN_MAX_CDB));
        [
            (lo, hi),
            (i32::from(ROOM_EQ_GAIN_MIN_CDB), gmax),
            (
                i32::from(ROOM_EQ_Q_MIN_MILLI),
                i32::from(ROOM_EQ_Q_MAX_MILLI),
            ),
        ]
    }

    /// The rules on a boost: centred at or above `min_boost_hz`, and a Q low enough that the
    /// boost's own resonance rings out within 0.5 s. REW limits boost Q so the filter's "60dB
    /// decay time" does not "exceed approximately 500 ms" (the EQ window help, above); an RBJ
    /// peaking boost's poles have Q_pole = Q A (A = 10^(gain/40), the cookbook's a0 = 1 +
    /// alpha / A), and a resonance decays 60 dB in ln(1000) Q_pole / (pi f0), so
    /// Q <= 0.2274 f0 / A (the research's derivation, 1d). Cuts have Q_pole = Q / A < Q and
    /// need neither rule.
    fn allowed(&self, f: i32, g: i32, q: i32) -> bool {
        if g <= 0 {
            return true;
        }
        let a = 10f64.powf(f64::from(g) / 100.0 / 40.0);
        let decay_s = 1000f64.ln() * (f64::from(q) / 1000.0) * a / (PI * f64::from(f));
        f64::from(f) >= self.cfg.min_boost_hz && decay_s <= 0.5
    }

    /// The weighted squared deviation over the grid after `cands`, with `skip` left out and
    /// `extra` added (see `weighted`).
    fn cost_with(&self, skip: Option<usize>, extra: Option<&Cand>) -> f64 {
        let mut sum = 0.0;
        for i in 0..self.grid.len() {
            let mut r = self.dev[i];
            for (j, c) in self.cands.iter().enumerate() {
                if Some(j) != skip {
                    r += c.on_fit[i];
                }
            }
            if let Some(e) = extra {
                r += e.on_fit[i];
            }
            sum += self.weighted(i, r).powi(2);
        }
        sum
    }

    /// Residual `r` at grid point `i`, weighted: a peak counts in full; a dip counts with
    /// `dip_weight` (on the cost, so its square root here); at a point left alone, what counts
    /// is the correction itself, r minus the deviation, so the fit gains nothing there and pays
    /// for touching it either way.
    fn weighted(&self, i: usize, r: f64) -> f64 {
        let w = self.cfg.dip_weight.sqrt();
        if self.left_alone[i] {
            w * (r - self.dev[i])
        } else if r >= 0.0 {
            r
        } else {
            w * r
        }
    }

    /// True when the combined correction, with `skip` replaced by `extra`, boosts no more than
    /// the ceiling anywhere it is checked, including at every boosting filter's centre.
    fn feasible(&self, skip: Option<usize>, extra: &Cand) -> bool {
        let ceiling = self.cfg.max_boost_db + 1e-9;
        for i in 0..self.check.len() {
            let mut c = extra.on_check[i];
            for (j, k) in self.cands.iter().enumerate() {
                if Some(j) != skip {
                    c += k.on_check[i];
                }
            }
            if c > ceiling {
                return false;
            }
        }
        let rate = self.cfg.model_rate_hz;
        let others = self
            .cands
            .iter()
            .enumerate()
            .filter(|(j, _)| Some(*j) != skip)
            .map(|(_, c)| c);
        let all: Vec<&Cand> = others.chain(std::iter::once(extra)).collect();
        for centre in all.iter().filter(|c| c.g > 0) {
            let f = f64::from(centre.f);
            let total: f64 = all
                .iter()
                .map(|c| {
                    peaking_db(
                        rate,
                        f64::from(c.f),
                        f64::from(c.g) / 100.0,
                        f64::from(c.q) / 1000.0,
                        f,
                    )
                })
                .sum();
            if total > ceiling {
                return false;
            }
        }
        true
    }

    /// Coordinate descent on one filter's quanta (slot `slot` of `cands`, or a new one when
    /// `slot` is None), each step halving to the quantum when no move improves.
    fn refine(&self, slot: Option<usize>, start: Cand) -> Cand {
        let lim = self.limits();
        let mut best = start;
        let mut best_cost = self.cost_with(slot, Some(&best));
        let mut steps = [(best.f / 20).max(1), 100, (best.q / 5).max(1)];
        let mut rounds = 0;
        while steps.iter().any(|s| *s > 0) && rounds < 200 {
            rounds += 1;
            let mut moved = false;
            for p in 0..3 {
                if steps[p] == 0 {
                    continue;
                }
                for dir in [1, -1] {
                    let mut v = [best.f, best.g, best.q];
                    v[p] = (v[p] + dir * steps[p]).clamp(lim[p].0, lim[p].1);
                    if v == [best.f, best.g, best.q] {
                        continue;
                    }
                    if !self.allowed(v[0], v[1], v[2]) {
                        continue;
                    }
                    let c = self.make(v[0], v[1], v[2]);
                    let cost = self.cost_with(slot, Some(&c));
                    if cost < best_cost - 1e-12 && self.feasible(slot, &c) {
                        best = c;
                        best_cost = cost;
                        moved = true;
                        break;
                    }
                }
            }
            if !moved {
                for s in steps.iter_mut() {
                    *s /= 2;
                }
            }
        }
        best
    }

    /// The largest weighted residual and where it is.
    fn worst(&self) -> Option<(usize, f64)> {
        let mut best: Option<(usize, f64, f64)> = None;
        for i in 0..self.grid.len() {
            let r = self.dev[i] + self.cands.iter().map(|c| c.on_fit[i]).sum::<f64>();
            let w = self.weighted(i, r);
            if best.is_none_or(|(_, b, _)| w.abs() > b) {
                best = Some((i, w.abs(), r));
            }
        }
        best.map(|(i, _, r)| (i, r))
    }

    /// The residual's width around grid point `i`: the octaves between the points either side
    /// where it falls to half its height (in dB), as a Q. Bandwidth N octaves gives
    /// Q = sqrt(2^N) / (2^N - 1) (the RBJ cookbook's relation between Q and bandwidth).
    fn q_guess(&self, i: usize, r: f64) -> i32 {
        let resid = |k: usize| self.dev[k] + self.cands.iter().map(|c| c.on_fit[k]).sum::<f64>();
        let half = r / 2.0;
        let inside = |k: usize| {
            if r > 0.0 {
                resid(k) > half
            } else {
                resid(k) < half
            }
        };
        let mut a = i;
        while a > 0 && inside(a - 1) {
            a -= 1;
        }
        let mut b = i;
        while b + 1 < self.grid.len() && inside(b + 1) {
            b += 1;
        }
        let n = (self.grid[b] / self.grid[a])
            .log2()
            .max(1.0 / self.cfg.points_per_octave);
        let q = 2f64.powf(n / 2.0) / (2f64.powf(n) - 1.0);
        ((q * 1000.0).round() as i32).clamp(
            i32::from(ROOM_EQ_Q_MIN_MILLI),
            i32::from(ROOM_EQ_Q_MAX_MILLI),
        )
    }

    fn run(&mut self) {
        let lim = self.limits();
        let mut cost = self.cost_with(None, None);
        while self.cands.len() < ROOM_EQ_MAX_FILTERS {
            let Some((i, r)) = self.worst() else { break };
            if self.weighted(i, r).abs() <= self.cfg.tolerance_db {
                break;
            }
            let f = (self.grid[i].round() as i32).clamp(lim[0].0, lim[0].1);
            let g = ((-r * 100.0).round() as i32).clamp(lim[1].0, lim[1].1);
            let q = self.q_guess(i, r);
            let mut start = self.make(f, g, q);
            if !self.allowed(f, g, q) || !self.feasible(None, &start) {
                // A boost the rules do not allow here starts at nothing and finds what it can.
                start = self.make(f, g.min(0), q);
            }
            let fresh = self.refine(None, start);
            if f64::from(fresh.g.abs()) < self.cfg.min_gain_db * 100.0 {
                break;
            }
            let before = self.cands.clone();
            self.cands.push(fresh);
            for _ in 0..2 {
                for slot in 0..self.cands.len() {
                    let c = self.cands[slot].clone();
                    self.cands[slot] = self.refine(Some(slot), c);
                }
            }
            let now = self.cost_with(None, None);
            let weakest = self.cands.iter().map(|c| c.g.abs()).min().unwrap_or(0);
            if now > cost * 0.99 || f64::from(weakest) < self.cfg.min_gain_db * 100.0 {
                self.cands = before;
                break;
            }
            cost = now;
        }
        self.cands.retain(|c| c.g != 0);
    }
}
