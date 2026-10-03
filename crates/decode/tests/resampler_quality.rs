//! The resampler's quality, measured on synthetic tones and sweeps and held to
//! a bound: passband ripple, the level of everything that is not the signal
//! (images when the rate goes up, aliases when it goes down), stopband
//! rejection, and the error of a resampled sweep against the same sweep
//! computed at the output rate.
//!
//! `cargo test -p chorus-decode --test resampler_quality -- --nocapture`
//! prints the figures; `docs/measurements/resampler-quality.md` records one
//! run of it. Everything here is arithmetic on generated signals: no device,
//! no clock.

use chorus_decode::Resampler;
use std::f64::consts::PI;

/// The bounds every measured ratio must hold (the measured figures are better;
/// the report has them).
const PASSBAND_RIPPLE_DB: f64 = 0.01;
/// Everything in the output that is not the input tone, relative to the tone.
const SPURIOUS_DB: f64 = -100.0;
/// A tone above the lower rate's Nyquist frequency, relative to its input level.
const STOPBAND_DB: f64 = -100.0;
/// A resampled sweep against the sweep computed at the output rate.
const SWEEP_ERROR_DB: f64 = -100.0;
/// The passband: up to this fraction of the lower of the two rates.
const PASSBAND_EDGE: f64 = 0.45;

const AMPLITUDE: f64 = 0.5;

fn resample(from: u32, to: u32, input: &[f32]) -> Vec<f32> {
    let mut r = Resampler::new(from, to, 1);
    let mut out = Vec::new();
    // In uneven chunks, as a decoder hands frames over.
    for chunk in input.chunks(1153) {
        r.process(chunk, &mut out);
    }
    r.flush(&mut out);
    out
}

fn tone(rate: u32, hz: f64, frames: usize) -> Vec<f32> {
    (0..frames)
        .map(|i| (AMPLITUDE * (2.0 * PI * hz * i as f64 / f64::from(rate)).sin()) as f32)
        .collect()
}

/// The amplitude of the component at `hz` in `signal` (least squares over a
/// sine and a cosine), and the rms of what is left after removing it.
fn fit(signal: &[f32], rate: u32, hz: f64) -> (f64, f64) {
    let w = 2.0 * PI * hz / f64::from(rate);
    let (mut ss, mut cc, mut sc, mut ys, mut yc) = (0.0f64, 0.0, 0.0, 0.0, 0.0);
    for (i, y) in signal.iter().enumerate() {
        let (s, c) = (w * i as f64).sin_cos();
        let y = f64::from(*y);
        ss += s * s;
        cc += c * c;
        sc += s * c;
        ys += y * s;
        yc += y * c;
    }
    let det = ss * cc - sc * sc;
    let a = (ys * cc - yc * sc) / det;
    let b = (yc * ss - ys * sc) / det;
    let mut rest = 0.0f64;
    for (i, y) in signal.iter().enumerate() {
        let (s, c) = (w * i as f64).sin_cos();
        let e = f64::from(*y) - a * s - b * c;
        rest += e * e;
    }
    (a.hypot(b), (rest / signal.len() as f64).sqrt())
}

/// The amplitudes of two components fitted together (least squares over a
/// sine and a cosine at each frequency), so that a strong tone next to a weak
/// one does not leak into the weak one's figure.
fn fit_two(signal: &[f32], rate: u32, hz_a: f64, hz_b: f64) -> (f64, f64) {
    let w = [
        2.0 * PI * hz_a / f64::from(rate),
        2.0 * PI * hz_b / f64::from(rate),
    ];
    // Normal equations A x = b over the basis [sin a, cos a, sin b, cos b].
    let mut m = [[0.0f64; 5]; 4];
    for (i, y) in signal.iter().enumerate() {
        let (sa, ca) = (w[0] * i as f64).sin_cos();
        let (sb, cb) = (w[1] * i as f64).sin_cos();
        let basis = [sa, ca, sb, cb];
        for r in 0..4 {
            for c in 0..4 {
                m[r][c] += basis[r] * basis[c];
            }
            m[r][4] += basis[r] * f64::from(*y);
        }
    }
    // Gaussian elimination with partial pivoting.
    for col in 0..4 {
        let pivot = (col..4)
            .max_by(|a, b| m[*a][col].abs().total_cmp(&m[*b][col].abs()))
            .expect("four rows");
        m.swap(col, pivot);
        for row in 0..4 {
            if row != col {
                let factor = m[row][col] / m[col][col];
                let pivot_row = m[col];
                for (cell, p) in m[row].iter_mut().zip(pivot_row).skip(col) {
                    *cell -= factor * p;
                }
            }
        }
    }
    let x: Vec<f64> = (0..4).map(|i| m[i][4] / m[i][i]).collect();
    (x[0].hypot(x[1]), x[2].hypot(x[3]))
}

fn db(ratio: f64) -> f64 {
    20.0 * ratio.max(1e-30).log10()
}

struct Figures {
    ripple_db: f64,
    spurious_db: f64,
    stopband_db: Option<f64>,
    image_db: Option<f64>,
    sweep_error_db: f64,
    gain_at_20k_db: Option<f64>,
}

fn measure(from: u32, to: u32) -> Figures {
    let low = f64::from(from.min(to));
    let frames = from as usize / 4; // a quarter of a second
    let edge = {
        let r = Resampler::new(from, to, 1);
        // Leave the ends out: there the window reaches past the signal.
        2 * r.delay_frames() + 16
    };
    let inner = |out: &[f32]| out[edge..out.len() - edge].to_vec();

    // Passband: 12 tones, logarithmically spaced from 20 Hz to the passband edge.
    let top = PASSBAND_EDGE * low;
    let mut ripple = 0.0f64;
    let mut spurious = f64::MIN;
    for k in 0..12 {
        let hz = 20.0 * (top / 20.0).powf(f64::from(k) / 11.0);
        let out = inner(&resample(from, to, &tone(from, hz, frames)));
        let (amplitude, rest) = fit(&out, to, hz);
        ripple = ripple.max(db(amplitude / AMPLITUDE).abs());
        spurious = spurious.max(db(rest / (AMPLITUDE / 2f64.sqrt())));
    }
    let gain_at_20k_db = (20_000.0 < 0.5 * low).then(|| {
        let out = inner(&resample(from, to, &tone(from, 20_000.0, frames)));
        db(fit(&out, to, 20_000.0).0 / AMPLITUDE)
    });

    // Going down: tones between the output's Nyquist frequency and the input's
    // must not come out (they would be aliases).
    let stopband_db = (to < from).then(|| {
        let (lo, hi) = (0.5 * f64::from(to), 0.5 * f64::from(from));
        let mut worst = f64::MIN;
        for k in 0..8 {
            let hz = lo + (hi - lo) * (f64::from(k) + 0.02) / 8.0;
            let out = inner(&resample(from, to, &tone(from, hz, frames)));
            let rms =
                (out.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / out.len() as f64).sqrt();
            worst = worst.max(db(rms / (AMPLITUDE / 2f64.sqrt())));
        }
        worst
    });

    // Going up: a tone at f has an image at (from - f); where that image is
    // below the output's Nyquist frequency it must not come out.
    let image_db = (to > from).then(|| {
        let lowest = (f64::from(from) - 0.5 * f64::from(to)).max(20.0);
        let highest = 0.5 * f64::from(from);
        let mut worst = f64::MIN;
        for k in 0..8 {
            let hz = lowest + (highest - lowest) * (f64::from(k) + 0.5) / 8.0;
            let out = inner(&resample(from, to, &tone(from, hz, frames)));
            let (_, image) = fit_two(&out, to, hz, f64::from(from) - hz);
            worst = worst.max(db(image / AMPLITUDE));
        }
        worst
    });

    // A logarithmic sweep across the passband against the same sweep computed
    // at the output rate: the output is time-aligned, so they compare directly.
    let seconds = frames as f64 / f64::from(from);
    let k = (top / 20.0).ln();
    let sweep_at = |t: f64| {
        AMPLITUDE * (2.0 * PI * 20.0 * seconds / k * ((t / seconds * k).exp() - 1.0)).sin()
    };
    let input: Vec<f32> = (0..frames)
        .map(|i| sweep_at(i as f64 / f64::from(from)) as f32)
        .collect();
    let out = resample(from, to, &input);
    let mut err = 0.0f64;
    let mut n = 0usize;
    for (i, y) in out.iter().enumerate().take(out.len() - edge).skip(edge) {
        let e = f64::from(*y) - sweep_at(i as f64 / f64::from(to));
        err += e * e;
        n += 1;
    }
    let sweep_error_db = db((err / n as f64).sqrt() / (AMPLITUDE / 2f64.sqrt()));

    Figures {
        ripple_db: ripple,
        spurious_db: spurious,
        stopband_db,
        image_db,
        sweep_error_db,
        gain_at_20k_db,
    }
}

fn opt(v: Option<f64>) -> String {
    v.map_or_else(|| "n/a".to_string(), |v| format!("{v:.3}"))
}

fn hold(from: u32, to: u32) {
    let f = measure(from, to);
    println!(
        "resampler {from} -> {to}: passband ripple {:.5} dB (to {:.0} Hz); spurious {:.1} dB; stopband {} dB; image {} dB; sweep error {:.1} dB; gain at 20 kHz {} dB",
        f.ripple_db,
        PASSBAND_EDGE * f64::from(from.min(to)),
        f.spurious_db,
        opt(f.stopband_db),
        opt(f.image_db),
        f.sweep_error_db,
        opt(f.gain_at_20k_db),
    );
    assert!(
        f.ripple_db <= PASSBAND_RIPPLE_DB,
        "{from} -> {to}: passband ripple {} dB",
        f.ripple_db
    );
    assert!(
        f.spurious_db <= SPURIOUS_DB,
        "{from} -> {to}: spurious {} dB",
        f.spurious_db
    );
    assert!(
        f.sweep_error_db <= SWEEP_ERROR_DB,
        "{from} -> {to}: sweep error {} dB",
        f.sweep_error_db
    );
    if let Some(stop) = f.stopband_db {
        assert!(stop <= STOPBAND_DB, "{from} -> {to}: stopband {stop} dB");
    }
    if let Some(image) = f.image_db {
        assert!(image <= STOPBAND_DB, "{from} -> {to}: image {image} dB");
    }
}

#[test]
fn quality_44100_to_48000() {
    hold(44_100, 48_000);
}

#[test]
fn quality_96000_to_48000() {
    hold(96_000, 48_000);
}

#[test]
fn quality_48000_to_44100() {
    hold(48_000, 44_100);
}

#[test]
fn quality_8000_to_48000() {
    hold(8_000, 48_000);
}

/// A ratio with more phases than the table stores takes the interpolated path;
/// it is held to a bound of its own, measured, not assumed equal.
#[test]
fn quality_of_an_interpolated_ratio_44100_to_47999() {
    let f = measure(44_100, 47_999);
    println!(
        "resampler 44100 -> 47999 (interpolated rows): passband ripple {:.5} dB; spurious {:.1} dB; image {} dB; sweep error {:.1} dB",
        f.ripple_db,
        f.spurious_db,
        opt(f.image_db),
        f.sweep_error_db
    );
    assert!(f.ripple_db <= PASSBAND_RIPPLE_DB);
    assert!(f.spurious_db <= -90.0, "spurious {} dB", f.spurious_db);
    assert!(
        f.sweep_error_db <= -90.0,
        "sweep error {} dB",
        f.sweep_error_db
    );
}

/// The measurement can fail: a crude resampler (nearest input frame) is far
/// outside the same bounds.
#[test]
fn the_measurement_tells_a_bad_resampler() {
    let input = tone(44_100, 1000.0, 22_050);
    let nearest: Vec<f32> = (0..24_000)
        .map(|k| input[(k as f64 * 44_100.0 / 48_000.0).round() as usize % input.len()])
        .collect();
    let (amplitude, rest) = fit(&nearest[..23_000], 48_000, 1000.0);
    assert!((db(amplitude / AMPLITUDE)).abs() < 0.1);
    assert!(
        db(rest / (AMPLITUDE / 2f64.sqrt())) > -40.0,
        "nearest-frame resampling measured as clean"
    );
}
