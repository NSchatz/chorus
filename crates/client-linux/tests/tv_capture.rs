//! A TV input through the client's real source loop, in modelled time (goal
//! 13, the TV path).
//!
//! What is real: `source::run` for an `optical` input (it hands it to
//! `run_tv`), the TV front end (`tvcapture`: the IEC 61937 scan, the lock and
//! range checks) and the rate matcher (`ratematch`: the DLL, the ratio loop,
//! the Catmull-Rom resampler), the codec check, `stream_format`, the
//! `audio_chunk` stamps and the `source_offer`s. What is modelled: the TV
//! (`common::tv::ModelledTv`: a sample clock off nominal by a set number of
//! ppm and drifting slowly, wakeups late by up to 100 us, stalls, IEC 61937
//! bursts) and the session (a probe that grades every message as it is
//! sent, so ten minutes need no memory). The hub's clock is the model's, and
//! the server timeline is it plus a fixed offset.
//!
//! The graded quantities: the stamp of each chunk against the TRUE capture
//! instant of the TV frame position the chunk starts on (read back off a
//! quadrature signal's phase), the spacing of the stamps, the ring counters,
//! a tone's frequency at the output, and the refusals. None of this is
//! timing evidence (BRIEF.md section 3.1 rule 3): the TV's clock, its jitter
//! and its drift are model inputs.

mod common;

use std::f64::consts::PI;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use chorus_client_linux::config::LineInConfig;
use chorus_client_linux::ratematch::SETTLED_BOUND_FRAMES;
use chorus_client_linux::source::{self, SignalThresholds, SourceSetup, SourceStats, Upstream};
use chorus_client_linux::tvcapture::{IEC61937_PA, IEC61937_PB};
use chorus_client_linux::Counters;
use chorus_protocol::v2::{Codec, Message, SourceAction, SourceControl, SourceKind};
use chorus_protocol::{AudioChunk, SampleFormat};
use common::tv::{ClockSegment, Content, ModelledTv, TvTimeline};

/// The server timeline is the hub's clock plus this.
const OFFSET_NS: i64 = 7_000_000_000;
/// Each period's wakeup is late by up to this (a model value).
const JITTER_NS: f64 = 100_000.0;
const CHUNK_NS: u64 = 20_000_000;
const QUADRATURE: Content = Content::Quadrature {
    period_frames: 4_800.0,
    amp: 0.5,
};
/// Grading starts this far in: the DLL's warm-up (0.5 s) and the ratio loop's
/// start-up bandwidth (4 s) and a settling margin.
const SETTLE_S: f64 = 60.0;

/// What a chunk is graded on, as the probe sees it.
type OnChunk = Box<dyn FnMut(&AudioChunk) + Send>;

/// The session, as a probe: every chunk handed to `on_chunk`, every offer
/// kept.
struct Probe {
    on_chunk: OnChunk,
    offers: Arc<Mutex<Vec<bool>>>,
}

impl Upstream for Probe {
    fn send(&mut self, message: &Message) -> std::io::Result<()> {
        match message {
            Message::AudioChunk(c) => (self.on_chunk)(c),
            Message::SourceOffer(o) => self.offers.lock().unwrap().push(o.signal),
            _ => {}
        }
        Ok(())
    }
}

struct Outcome {
    lines: Vec<String>,
    stats: SourceStats,
    offers: Vec<bool>,
}

/// Run the source loop on `tv` for `seconds` of modelled time, started at
/// once, every chunk to `on_chunk`.
fn run(mut tv: ModelledTv, seconds: f64, on_chunk: OnChunk) -> Outcome {
    let clock = tv.clock();
    let input = LineInConfig {
        kind: SourceKind::Optical,
        sample_format: SampleFormat::PcmF32Le,
        ..LineInConfig::new("modelled-tv")
    };
    let counters = Arc::new(Counters::new());
    counters.offset.publish(Some(OFFSET_NS));
    let (tx, controls) = mpsc::channel();
    tx.send(SourceControl {
        source_id: 1,
        action: SourceAction::Start,
        codec: Codec::Pcm,
    })
    .unwrap();
    let lines = Arc::new(Mutex::new(Vec::new()));
    let offers = Arc::new(Mutex::new(Vec::new()));
    let setup = SourceSetup {
        input,
        listed_codecs: Codec::Pcm.bit(),
        clock: {
            let clock = Arc::clone(&clock);
            Box::new(move || clock.load(Ordering::SeqCst))
        },
        counters,
        controls,
        thresholds: SignalThresholds::default(),
        tv_power: None,
        log: {
            let lines = Arc::clone(&lines);
            let clock = Arc::clone(&clock);
            Box::new(move |l| {
                let at = clock.load(Ordering::SeqCst) as f64 / 1e9;
                lines.lock().unwrap().push(format!("{:.3} {}", at, l));
            })
        },
    };
    let mut probe = Probe {
        on_chunk,
        offers: Arc::clone(&offers),
    };
    let stats = SourceStats::default();
    let end = (seconds * 1e9) as u64;
    let keep = move || clock.load(Ordering::SeqCst) < end;
    let stop = source::run(&mut tv, &mut probe, setup, &keep, &stats);
    assert_eq!(stop.name(), "stopped");
    drop(tx);
    let lines = lines.lock().unwrap().clone();
    let offers = offers.lock().unwrap().clone();
    Outcome {
        lines,
        stats,
        offers,
    }
}

fn f32s(c: &AudioChunk) -> Vec<f32> {
    c.audio_data
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect()
}

/// The stamp grading: each chunk's stamp against the true capture instant of
/// the TV position its first frame plays.
#[derive(Debug, Default)]
struct Drift {
    chunks: u64,
    graded: u64,
    max_abs_ns: f64,
    sum_ns: f64,
    /// Consecutive chunks whose stamps are not exactly one chunk apart.
    spacing_faults: u64,
    last: Option<(u32, u64)>,
}

fn grade_drift(timeline: TvTimeline, period_frames: f64) -> (Arc<Mutex<Drift>>, OnChunk) {
    let drift = Arc::new(Mutex::new(Drift::default()));
    let d = Arc::clone(&drift);
    let on_chunk: OnChunk = Box::new(move |c: &AudioChunk| {
        let mut d = d.lock().unwrap();
        d.chunks += 1;
        if let Some((seq, stamp)) = d.last {
            if c.sequence == seq.wrapping_add(1) && c.timestamp_ns.abs_diff(stamp + CHUNK_NS) > 1 {
                d.spacing_faults += 1;
            }
        }
        d.last = Some((c.sequence, c.timestamp_ns));
        let hub_ns = c.timestamp_ns as f64 - OFFSET_NS as f64;
        if hub_ns < SETTLE_S * 1e9 {
            return;
        }
        let s = f32s(c);
        let phase = f64::from(s[0]).atan2(f64::from(s[1]));
        let modulo = (phase / (2.0 * PI)).rem_euclid(1.0) * period_frames;
        let expected = timeline.frames_at(hub_ns);
        let mut wrap = (modulo - expected.rem_euclid(period_frames)) / period_frames;
        wrap -= wrap.round();
        let position = expected + wrap * period_frames;
        let err = hub_ns - timeline.capture_time_ns(position);
        d.graded += 1;
        d.sum_ns += err;
        d.max_abs_ns = d.max_abs_ns.max(err.abs());
    });
    (drift, on_chunk)
}

fn ten_minutes(ppm: f64, drift_ppm_per_s: f64, seed: u64) {
    let tv = ModelledTv::new(
        SampleFormat::PcmF32Le,
        &[ClockSegment::at(0.0, ppm, drift_ppm_per_s)],
        &[(0, QUADRATURE)],
        JITTER_NS,
        seed,
    );
    let (drift, on_chunk) = grade_drift(tv.timeline(), 4_800.0);
    let out = run(tv, 600.0, on_chunk);
    let d = drift.lock().unwrap();
    let frame_ns = 1e9 / 48_000.0;
    eprintln!(
        "{:+} ppm (+{} ppm/s): chunks={} graded={} max|stamp - capture|={:.1} ns mean={:.1} ns \
         {}",
        ppm,
        drift_ppm_per_s,
        d.chunks,
        d.graded,
        d.max_abs_ns,
        d.sum_ns / d.graded.max(1) as f64,
        out.stats.tv_line()
    );
    // Ten minutes less the start: every 20 ms chunk sent.
    assert!(d.chunks >= 29_900, "{} {:?}", d.chunks, out.lines);
    assert!(d.graded >= 26_900, "{}", d.graded);
    assert_eq!(d.spacing_faults, 0, "stamps are exactly one chunk apart");
    assert!(
        d.max_abs_ns < SETTLED_BOUND_FRAMES * frame_ns,
        "stamp drift {:.1} ns over the bound {:.1} ns",
        d.max_abs_ns,
        SETTLED_BOUND_FRAMES * frame_ns
    );
    let g = |a: &std::sync::atomic::AtomicU64| a.load(Ordering::Relaxed);
    assert_eq!(g(&out.stats.tv_relocks), 0, "{:?}", out.lines);
    assert_eq!(g(&out.stats.tv_ring_overflows), 0);
    assert_eq!(g(&out.stats.tv_ring_underflows), 0);
    assert_eq!(out.offers, [true], "offered once, never withdrawn");
    let ppm_seen = out.stats.tv_ppm_milli.load(Ordering::Relaxed) as f64 / 1_000.0;
    let ppm_true = ppm + drift_ppm_per_s * 600.0;
    assert!(
        (ppm_seen - ppm_true).abs() < 50.0,
        "{} vs {}",
        ppm_seen,
        ppm_true
    );
}

#[test]
fn a_tv_200_ppm_fast_keeps_its_stamps_on_its_capture_instants_for_ten_minutes() {
    ten_minutes(200.0, 0.01, 11);
}

#[test]
fn a_tv_200_ppm_slow_keeps_its_stamps_on_its_capture_instants_for_ten_minutes() {
    ten_minutes(-200.0, -0.01, 12);
}

#[test]
fn a_tv_1400_ppm_fast_keeps_its_stamps_on_its_capture_instants_for_ten_minutes() {
    ten_minutes(1_400.0, 0.0015, 13);
}

#[test]
fn a_tv_1400_ppm_slow_keeps_its_stamps_on_its_capture_instants_for_ten_minutes() {
    ten_minutes(-1_400.0, -0.0015, 14);
}

/// A tone's frequency at the output, from the phase of each second's
/// demodulated phasor, fitted by least squares.
#[derive(Debug, Default)]
struct ToneMeter {
    n: u64,
    re: f64,
    im: f64,
    phases: Vec<(f64, f64)>,
}

fn tone_at_output(ppm: f64, drift_ppm_per_s: f64, seed: u64) -> f64 {
    let tv = ModelledTv::new(
        SampleFormat::PcmF32Le,
        &[ClockSegment::at(0.0, ppm, drift_ppm_per_s)],
        &[(
            0,
            Content::Tone {
                hz: 1_000.0,
                amp: 0.5,
            },
        )],
        JITTER_NS,
        seed,
    );
    let meter = Arc::new(Mutex::new(ToneMeter::default()));
    let m = Arc::clone(&meter);
    let w = 2.0 * PI * 1_000.0 / 48_000.0;
    let on_chunk: OnChunk = Box::new(move |c: &AudioChunk| {
        let mut m = m.lock().unwrap();
        for frame in f32s(c).as_chunks::<2>().0 {
            // Output frame n is due n / 48000 s after the first: the phase
            // reference runs on the output's own sample count.
            let x = f64::from(frame[0]);
            let a = w * (m.n % 48_000) as f64;
            m.re += x * a.cos();
            m.im -= x * a.sin();
            m.n += 1;
            if m.n.is_multiple_of(48_000) {
                let t = (m.n / 48_000) as f64;
                let phase = m.im.atan2(m.re);
                m.phases.push((t, phase));
                m.re = 0.0;
                m.im = 0.0;
            }
        }
    });
    let out = run(tv, 600.0, on_chunk);
    let m = meter.lock().unwrap();
    // Unwrap, keep from SETTLE_S, fit phase = a + b t.
    let mut unwrapped = Vec::new();
    let mut last = 0.0;
    let mut turns = 0.0;
    for (i, (t, p)) in m.phases.iter().enumerate() {
        if i > 0 {
            let step = p - last;
            if step > PI {
                turns -= 2.0 * PI;
            } else if step < -PI {
                turns += 2.0 * PI;
            }
        }
        last = *p;
        unwrapped.push((*t, p + turns));
    }
    let fit: Vec<(f64, f64)> = unwrapped
        .into_iter()
        .filter(|(t, _)| *t >= SETTLE_S)
        .collect();
    assert!(fit.len() > 400, "{} seconds measured", fit.len());
    let n = fit.len() as f64;
    let mt = fit.iter().map(|(t, _)| t).sum::<f64>() / n;
    let mp = fit.iter().map(|(_, p)| p).sum::<f64>() / n;
    let slope = fit.iter().map(|(t, p)| (t - mt) * (p - mp)).sum::<f64>()
        / fit.iter().map(|(t, _)| (t - mt) * (t - mt)).sum::<f64>();
    // Each second's phasor is taken over a whole second of the output's own
    // count, so its phase advances by 2 pi (f - 1000) per second.
    let df = slope / (2.0 * PI);
    let off_ppm = df / 1_000.0 * 1e6;
    eprintln!(
        "{:+} ppm TV: the 1 kHz tone at the output is {:+.6} ppm off nominal over {} s; {}",
        ppm,
        off_ppm,
        fit.len(),
        out.stats.tv_line()
    );
    off_ppm
}

#[test]
fn a_1_khz_tone_from_a_tv_1400_ppm_fast_comes_out_within_1_ppm_of_nominal() {
    let off = tone_at_output(1_400.0, 0.0, 21);
    assert!(off.abs() < 1.0, "{} ppm", off);
}

#[test]
fn a_1_khz_tone_from_a_tv_200_ppm_slow_and_drifting_comes_out_within_1_ppm_of_nominal() {
    let off = tone_at_output(-200.0, -0.01, 22);
    assert!(off.abs() < 1.0, "{} ppm", off);
}

/// Each chunk sent: its stamp's hub time, s, and whether it carries an IEC
/// 61937 sync word pair.
type Stamps = Arc<Mutex<Vec<(f64, bool)>>>;

/// Every chunk sent, into [`Stamps`].
fn record_stamps() -> (Stamps, OnChunk) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let s = Arc::clone(&seen);
    let on_chunk: OnChunk = Box::new(move |c: &AudioChunk| {
        let x = f32s(c);
        let word = |v: f32| ((f64::from(v) * 32_768.0).round() as i16) as u16;
        let burst = x
            .as_chunks::<2>()
            .0
            .iter()
            .any(|f| word(f[0]) == IEC61937_PA && word(f[1]) == IEC61937_PB);
        let hub = (c.timestamp_ns as f64 - OFFSET_NS as f64) / 1e9;
        s.lock().unwrap().push((hub, burst));
    });
    (seen, on_chunk)
}

fn said(out: &Outcome, what: &str) -> Option<f64> {
    out.lines
        .iter()
        .find(|l| l.contains(what))
        .map(|l| l.split(' ').next().unwrap().parse::<f64>().unwrap())
}

fn tone() -> Content {
    Content::Tone {
        hz: 997.0,
        amp: 0.5,
    }
}

#[test]
fn a_tv_2000_ppm_fast_is_refused_as_rate_out_of_range_and_taken_back_inside_the_clamp() {
    let tv = ModelledTv::new(
        SampleFormat::PcmF32Le,
        &[
            ClockSegment::at(0.0, 2_000.0, 0.0),
            ClockSegment::at(20.0, 100.0, 0.0),
        ],
        &[(0, tone())],
        JITTER_NS,
        31,
    );
    let (seen, on_chunk) = record_stamps();
    let out = run(tv, 40.0, on_chunk);
    let refused = said(
        &out,
        "source-tv-refused source_id=1 reason=rate-out-of-range",
    )
    .unwrap_or_else(|| panic!("{:?}", out.lines));
    let withdrawn = said(&out, "signal=0 reason=rate-out-of-range").expect("withdrawn by name");
    let back = said(&out, "source-tv-accepted").expect("taken back");
    eprintln!(
        "refused at {:.3} s, withdrawn at {:.3} s, accepted at {:.3} s; {}",
        refused,
        withdrawn,
        back,
        out.stats.tv_line()
    );
    // The DLL warms for 0.5 s, then the loop sits at the clamp for 2 s.
    assert!((2.4..3.5).contains(&refused), "{}", refused);
    assert!(back > 20.0 && back < 21.5, "{}", back);
    assert!(out.offers.ends_with(&[true]), "{:?}", out.offers);
    let seen = seen.lock().unwrap();
    // Nothing forwarded between the refusal and the source's return (each
    // chunk's stamp is behind the refusal by at most the send delay and a
    // chunk).
    let during = seen
        .iter()
        .filter(|(t, _)| *t > refused && *t < 20.0)
        .count();
    assert_eq!(during, 0);
    assert!(seen.iter().filter(|(t, _)| *t > back).count() > 800);
    assert!(out.stats.tv_refused_rate.load(Ordering::Relaxed) >= 1);
}

#[test]
fn an_iec61937_stream_is_refused_as_non_pcm_and_nothing_of_it_is_forwarded() {
    // PCM for 10 s, then bursts for 10 s, then PCM again.
    let tv = ModelledTv::new(
        SampleFormat::PcmF32Le,
        &[ClockSegment::at(0.0, 300.0, 0.0)],
        &[(0, tone()), (480_000, Content::Iec61937), (960_000, tone())],
        JITTER_NS,
        41,
    );
    let (seen, on_chunk) = record_stamps();
    let out = run(tv, 30.0, on_chunk);
    let refused = said(&out, "source-tv-refused source_id=1 reason=non-pcm")
        .unwrap_or_else(|| panic!("{:?}", out.lines));
    let back = said(&out, "source-tv-accepted").expect("taken back");
    assert!(said(&out, "signal=0 reason=non-pcm").is_some());
    eprintln!(
        "non-pcm from {:.3} s to {:.3} s; {}",
        refused,
        back,
        out.stats.tv_line()
    );
    // The TV runs 300 ppm fast and the first burst after frame 480000 is at
    // frame 480768 (a multiple of 1536), captured at 10.013 s; its period
    // ends at 10.017 s.
    assert!((10.013..10.02).contains(&refused), "{}", refused);
    // The clean hold: 250 ms after the last burst (frame 958464, 19.963 s).
    assert!((20.2..20.3).contains(&back), "{}", back);
    let seen = seen.lock().unwrap();
    assert!(
        seen.iter().all(|(_, burst)| !burst),
        "a burst was forwarded"
    );
    let during = seen
        .iter()
        .filter(|(t, _)| *t > 10.013 && *t < 20.2)
        .count();
    assert_eq!(during, 0, "nothing forwarded while refused");
    assert!(seen.iter().filter(|(t, _)| *t > back).count() > 400);
    assert_eq!(out.offers, [true, false, true]);
    assert!(out.stats.tv_non_pcm_periods.load(Ordering::Relaxed) > 2_000);
}

#[test]
fn a_set_non_audio_bit_is_refused_as_non_pcm_even_with_pcm_samples() {
    let mut tv = ModelledTv::new(
        SampleFormat::PcmF32Le,
        &[ClockSegment::at(0.0, -100.0, 0.0)],
        &[(0, tone())],
        JITTER_NS,
        42,
    );
    tv.non_audio_from(0.0, Some(false));
    tv.non_audio_from(5.0, Some(true));
    tv.non_audio_from(8.0, Some(false));
    let (seen, on_chunk) = record_stamps();
    let out = run(tv, 12.0, on_chunk);
    let refused = said(&out, "reason=non-pcm").expect("refused");
    let back = said(&out, "source-tv-accepted").expect("taken back");
    assert!((5.0..5.01).contains(&refused), "{}", refused);
    // 250 ms of clean periods from the first read after 8 s.
    assert!((8.245..8.26).contains(&back), "{}", back);
    let seen = seen.lock().unwrap();
    // The first chunk after the refusal lifts starts SEND_DELAY_NS (2 ms)
    // behind the end of the period that lifted it, which is up to a period
    // (5 ms) and a wakeup's lateness before the read.
    let during: Vec<_> = seen
        .iter()
        .filter(|(t, _)| *t > 5.0 && *t < back - 0.008)
        .collect();
    assert!(
        during.is_empty(),
        "{:?} refused {} back {}",
        during,
        refused,
        back
    );
}

#[test]
fn a_stalled_tv_is_no_lock_after_100_ms_and_taken_back_when_its_clock_returns() {
    let tv = ModelledTv::new(
        SampleFormat::PcmF32Le,
        &[
            ClockSegment::at(0.0, 300.0, 0.0),
            ClockSegment::stall(10.0),
            ClockSegment::at(11.0, 300.0, 0.0),
        ],
        &[(0, tone())],
        JITTER_NS,
        51,
    );
    let (seen, on_chunk) = record_stamps();
    let out = run(tv, 20.0, on_chunk);
    let refused = said(&out, "source-tv-refused source_id=1 reason=no-lock")
        .unwrap_or_else(|| panic!("{:?}", out.lines));
    let back = said(&out, "source-tv-accepted").expect("taken back");
    eprintln!(
        "no-lock at {:.3} s, accepted at {:.3} s; {}",
        refused,
        back,
        out.stats.tv_line()
    );
    // The last period before the stall ended at its last whole period before
    // 10 s (up to 5 ms before); the stall is seen at the first 20 ms wait
    // that ends 100 ms or more after that period's read.
    assert!((10.095..10.13).contains(&refused), "{}", refused);
    // The clock returns at 11 s; the DLL warms 0.5 s.
    assert!((11.49..11.52).contains(&back), "{}", back);
    let seen = seen.lock().unwrap();
    assert!(seen.iter().filter(|(t, _)| *t > back).count() > 400);
    assert_eq!(out.offers, [true, false, true]);
    assert!(said(&out, "signal=0 reason=no-lock").is_some());
}
