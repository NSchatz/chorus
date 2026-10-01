//! The shared DSP fixtures (goal 12): every file under `fixtures/dsp/`, run
//! against this crate. `firmware/tests/test_dsp.c` runs the same files against
//! the C mirror, so the two implementations are held to one contract
//! (`fixtures/README.md`, `dsp/`; conventions rule 9).
//!
//! Every fixture names its `kind`, its `source` (the cited worked example, or
//! `docs/dsp.md` for chorus's own chain) and the date it was `read`. A kind
//! this reader does not know is a failure, not a skip: a fixture nobody runs
//! is not a contract.

use std::path::{Path, PathBuf};

use chorus_dsp::biquad::{Biquad, Coefficients, Kind};
use chorus_dsp::compressor::{static_curve_db, Compressor, CompressorParams, NIGHT};
use chorus_dsp::crossover::{db_of, Lr4, Lr4Design};
use chorus_dsp::delay::{self, Delay};
use chorus_dsp::fixture::{run_chain, signal, Fields};
use chorus_dsp::limiter::Limiter;
use chorus_dsp::loudness;

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/dsp")
}

/// Counts every check, and collects the failures instead of stopping at the
/// first, so one run shows every fixture that drifted.
#[derive(Default)]
struct Tally {
    checks: usize,
    failures: Vec<String>,
}

impl Tally {
    fn check(&mut self, ok: bool, what: impl FnOnce() -> String) {
        self.checks += 1;
        if !ok {
            self.failures.push(what());
        }
    }
}

fn design(f: &Fields) -> Result<Coefficients, String> {
    let kind = Kind::from_name(f.str("type")?).ok_or("unknown biquad type")?;
    Coefficients::design(
        kind,
        f.num("rate_hz")?,
        f.num("f0_hz")?,
        f.num("q")?,
        f.num("gain_db")?,
    )
    .map_err(|e| format!("the design was refused: {e}"))
}

fn biquad_coefficients(f: &Fields, t: &mut Tally, name: &str) -> Result<(), String> {
    let c = design(f)?;
    let tol = f.num("tolerance")?;
    if f.get("expect_b").is_some() {
        let b = f.list("expect_b")?;
        for (i, (got, want)) in [c.b0, c.b1, c.b2].iter().zip(&b).enumerate() {
            t.check((got - want).abs() <= tol, || {
                format!("{name}: b{i} {got} vs {want}")
            });
        }
    }
    if f.get("expect_b_ratio").is_some() {
        let r = f.list("expect_b_ratio")?;
        let tol_r = f.num("tolerance_ratio")?;
        for (i, (got, want)) in [1.0, c.b1 / c.b0, c.b2 / c.b0].iter().zip(&r).enumerate() {
            t.check((got - want).abs() <= tol_r, || {
                format!("{name}: b{i}/b0 {got} vs {want}")
            });
        }
    }
    let a = f.list("expect_a")?;
    for (i, (got, want)) in [c.a1, c.a2].iter().zip(&a).enumerate() {
        t.check((got - want).abs() <= tol, || {
            format!("{name}: a{} {got} vs {want}", i + 1)
        });
    }
    Ok(())
}

fn biquad_magnitude(f: &Fields, t: &mut Tally, name: &str) -> Result<(), String> {
    let c = design(f)?;
    let rate = f.num("rate_hz")?;
    let tol = f.num("tolerance_db")?;
    let freqs = f.list("freqs_hz")?;
    let expect = f.list("expect_db")?;
    if freqs.len() != expect.len() {
        return Err("freqs_hz and expect_db differ in length".into());
    }
    for (fr, want) in freqs.iter().zip(&expect) {
        let got = c.magnitude_db(*fr, rate);
        let ok = if *want == f64::NEG_INFINITY {
            got < -100.0
        } else {
            (got - want).abs() <= tol
        };
        t.check(ok, || format!("{name}: {fr} Hz is {got} dB, want {want}"));
    }
    Ok(())
}

fn biquad_reference(f: &Fields, t: &mut Tally, name: &str) -> Result<(), String> {
    let c = design(f)?;
    let rate = f.num("rate_hz")?;
    let rb = f.list("ref_b")?;
    let ra = f.list("ref_a")?;
    let r = Coefficients {
        b0: rb[0],
        b1: rb[1],
        b2: rb[2],
        a1: ra[0],
        a2: ra[1],
    };
    let tol = f.num("tolerance_db")?;
    for fr in f.list("freqs_hz")? {
        let (got, want) = (c.magnitude_db(fr, rate), r.magnitude_db(fr, rate));
        t.check((got - want).abs() <= tol, || {
            format!("{name}: {fr} Hz is {got} dB, the printed filter {want}")
        });
    }
    // The running f32 section against the printed filter in f64 Direct Form I.
    let frames = f.num("impulse_frames")? as usize;
    let tol = f.num("tolerance_impulse")?;
    let mut q = Biquad::new(&c);
    let (mut x1, mut x2, mut y1, mut y2) = (0f64, 0f64, 0f64, 0f64);
    let mut worst = 0f64;
    for n in 0..frames {
        let x = if n == 0 { 1.0 } else { 0.0 };
        let y = r.b0 * x + r.b1 * x1 + r.b2 * x2 - r.a1 * y1 - r.a2 * y2;
        let got = q.process(x as f32) as f64;
        worst = worst.max((got - y).abs());
        (x2, x1, y2, y1) = (x1, x, y1, y);
    }
    t.check(worst <= tol, || {
        format!("{name}: impulse differs from the printed filter by {worst}")
    });
    Ok(())
}

fn lr4(f: &Fields, t: &mut Tally, name: &str) -> Result<(), String> {
    let rate = f.num("rate_hz")?;
    let fc = f.num("crossover_hz")?;
    let d = Lr4Design::new(rate, fc).map_err(|e| e.to_string())?;
    let at = f.num("at_crossover_db")?;
    let tol = f.num("tolerance_db")?;
    let lo = db_of(d.response_low(fc, rate));
    let hi = db_of(d.response_high(fc, rate));
    t.check((lo - at).abs() <= tol, || {
        format!("{name}: low {lo} dB at fc")
    });
    t.check((hi - at).abs() <= tol, || {
        format!("{name}: high {hi} dB at fc")
    });
    let sum_tol = f.num("sum_tolerance_db")?;
    let phase_tol = f.num("phase_tolerance_deg")?;
    for fr in f.list("freqs_hz")? {
        let s = db_of(d.response_sum(fr, rate));
        t.check(s.abs() <= sum_tol, || {
            format!("{name}: sum {s} dB at {fr} Hz")
        });
        let (l, h) = (d.response_low(fr, rate), d.response_high(fr, rate));
        let diff = (l.1.atan2(l.0) - h.1.atan2(h.0)).to_degrees();
        let diff = (diff + 540.0).rem_euclid(360.0) - 180.0;
        t.check(diff.abs() <= phase_tol, || {
            format!("{name}: branches {diff} degrees apart at {fr} Hz")
        });
    }
    // The running split: a unit sine at fc, each branch settles at 0.5.
    let frames = f.num("sine_frames")? as usize;
    let tol = f.num("sine_tolerance")?;
    let x = signal(&format!("sine {fc} 1"), frames, rate)?;
    let mut s = Lr4::new(&d);
    let (mut low, mut high) = (Vec::new(), Vec::new());
    for &v in &x {
        let (l, h) = s.split(v);
        low.push(l);
        high.push(h);
    }
    for (b, y) in [("low", &low), ("high", &high)] {
        let a = amplitude(&y[frames - frames / 10..]);
        t.check((a - 0.5).abs() <= tol, || {
            format!("{name}: the running {b} branch's amplitude is {a}")
        });
    }
    Ok(())
}

/// A sine's amplitude from its samples: sqrt(2 x mean square), exact over a
/// whole number of cycles.
fn amplitude(y: &[f32]) -> f64 {
    let ms = y.iter().map(|&v| v as f64 * v as f64).sum::<f64>() / y.len() as f64;
    (2.0 * ms).sqrt()
}

fn delay_fixture(f: &Fields, t: &mut Tally, name: &str) -> Result<(), String> {
    let m = f.num("delay_frames")? as usize;
    let frames = f.num("frames")? as usize;
    let x = signal(f.str("input")?, frames, f.num("rate_hz")?)?;
    let mut d = Delay::new(m).map_err(|e| e.to_string())?;
    let mut exact = true;
    for (n, &v) in x.iter().enumerate() {
        let y = d.process(v);
        let want = if n >= m { x[n - m] } else { 0.0 };
        exact &= y.to_bits() == want.to_bits();
    }
    t.check(exact, || format!("{name}: y(n) is not x(n - {m})"));
    let max = f.num("max_frames")? as usize;
    t.check(max == delay::MAX_FRAMES && Delay::new(max).is_ok(), || {
        format!("{name}: {max} frames is not the maximum taken")
    });
    let refuse = f.num("refuse_frames")? as usize;
    t.check(Delay::new(refuse).is_err(), || {
        format!("{name}: {refuse} frames was not refused")
    });
    Ok(())
}

fn limiter_fixture(f: &Fields, t: &mut Tally, name: &str) -> Result<(), String> {
    let rate = f.num("rate_hz")?;
    let ceiling = f.num("ceiling")? as f32;
    let la = f.num("lookahead_frames")? as usize;
    let frames = f.num("frames")? as usize;
    let mut inputs = Vec::new();
    while let Some(r) = f.get(&format!("input.{}", inputs.len())) {
        inputs.push(signal(r, frames, rate)?);
    }
    let n = inputs.len();
    let mut l = Limiter::new(n, la, f.num("release_ms")?, rate).map_err(|e| e.to_string())?;
    l.set_ceiling(ceiling);
    let tail = f.num("unity_tail_frames")? as usize;
    let (mut within, mut exact) = (true, true);
    let mut frame = vec![0f32; n];
    for i in 0..frames {
        for c in 0..n {
            frame[c] = inputs[c][i];
        }
        l.process_frame(&mut frame);
        for c in 0..n {
            within &= frame[c].abs() <= ceiling;
            if i >= frames - tail {
                let want = if i >= la { inputs[c][i - la] } else { 0.0 };
                exact &= frame[c].to_bits() == want.to_bits();
            }
        }
    }
    t.check(within, || {
        format!("{name}: an output sample exceeded {ceiling}")
    });
    if tail > 0 {
        t.check(exact, || {
            format!("{name}: the last {tail} frames are not the input delayed by {la}")
        });
    }
    match f.str("check")? {
        "ceiling" | "unity" => Ok(()),
        other => Err(format!("unknown limiter check '{other}'")),
    }
}

fn compressor_static(f: &Fields, t: &mut Tally, name: &str) -> Result<(), String> {
    let (th, r, w) = (f.num("threshold_db")?, f.num("ratio")?, f.num("knee_db")?);
    let tol = f.num("tolerance_db")?;
    for (x, want) in f.list("inputs_db")?.iter().zip(f.list("expect_db")?) {
        let got = static_curve_db(*x, th, r, w);
        t.check((got - want).abs() <= tol, || {
            format!("{name}: y({x}) = {got}, want {want}")
        });
    }
    Ok(())
}

fn compressor_timing(f: &Fields, t: &mut Tally, name: &str) -> Result<(), String> {
    let rate = f.num("rate_hz")?;
    let step = f.num("step_db")? as f32;
    let tol = f.num("tolerance_frames")?;
    for (label, ms, from, to) in [
        ("attack", f.num("attack_ms")?, 0.0f32, step),
        ("release", f.num("release_ms")?, step, 0.0f32),
    ] {
        let p = CompressorParams {
            attack_ms: f.num("attack_ms")?,
            release_ms: f.num("release_ms")?,
            ..NIGHT
        };
        let mut c = Compressor::new(&p, rate).map_err(|e| e.to_string())?;
        // Settle at `from`, then step to `to`.
        for _ in 0..(rate as usize * 10) {
            c.smooth(from);
        }
        let (a, b) = (from + 0.1 * (to - from), from + 0.9 * (to - from));
        let past = |g: f32, level: f32| if to < from { g <= level } else { g >= level };
        let (mut t10, mut t90) = (None, None);
        for n in 0..(rate as usize * 10) {
            let g = c.smooth(to);
            if t10.is_none() && past(g, a) {
                t10 = Some(n);
            }
            if t90.is_none() && past(g, b) {
                t90 = Some(n);
                break;
            }
        }
        let want = ms * rate / 1000.0;
        let got = match (t10, t90) {
            (Some(x), Some(y)) => (y - x) as f64,
            _ => f64::NAN,
        };
        t.check((got - want).abs() <= tol, || {
            format!("{name}: {label} 10-90 % in {got} frames, want {want}")
        });
    }
    Ok(())
}

fn iso226(f: &Fields, t: &mut Tally, name: &str) -> Result<(), String> {
    let rows = f.list("table_freqs_hz")?;
    let (af, lu, tf) = (f.list("af")?, f.list("lu")?, f.list("tf")?);
    for (k, fr) in rows.iter().enumerate() {
        let i = loudness::table_index(*fr).ok_or(format!("{fr} Hz is not in the table"))?;
        let ok = (loudness::AF[i] - af[k]).abs() <= 1e-12
            && (loudness::LU[i] - lu[k]).abs() <= 1e-12
            && (loudness::TF[i] - tf[k]).abs() <= 1e-12;
        t.check(ok, || format!("{name}: the table row at {fr} Hz differs"));
    }
    let tol = f.num("tolerance_db")?;
    let freqs = f.list("freqs_hz")?;
    let phon = f.list("phon")?;
    for ((fr, p), want) in freqs.iter().zip(&phon).zip(f.list("expect_spl_db")?) {
        let got = loudness::spl_db(*fr, *p).ok_or("not a table frequency")?;
        t.check((got - want).abs() <= tol, || {
            format!("{name}: Lp({fr} Hz, {p} phon) = {got}, want {want}")
        });
    }
    Ok(())
}

fn loudness_fixture(f: &Fields, t: &mut Tally, name: &str) -> Result<(), String> {
    let tol = f.num("tolerance_db")?;
    let lows = f.list("expect_low_db")?;
    let highs = f.list("expect_high_db")?;
    for (k, att) in f.list("attenuations_db")?.iter().enumerate() {
        let (lo, hi) = loudness::shelf_gains_db(*att);
        t.check(
            (lo - lows[k]).abs() <= tol && (hi - highs[k]).abs() <= tol,
            || format!("{name}: {att} dB down gives {lo}, {hi}"),
        );
    }
    for (g, want) in f
        .list("room_gains")?
        .iter()
        .zip(f.list("expect_attenuation_db")?)
    {
        let got = loudness::attenuation_db(*g as f32);
        t.check(got == want, || {
            format!("{name}: room gain {g} quantises to {got}, want {want}")
        });
    }
    Ok(())
}

fn chain(f: &Fields, t: &mut Tally, name: &str) -> Result<(), String> {
    // Two block sizes: the result must not depend on how the frames arrive.
    let run = run_chain(f, 4096)?;
    let again = run_chain(f, 37)?;
    t.check(run.outputs == again.outputs, || {
        format!("{name}: the output depends on the block size")
    });
    let outs = run.outputs.len();
    let want_outs = f.num("out_channels")? as usize;
    t.check(outs == want_outs, || format!("{name}: {outs} outputs"));
    let lat = run.chain.latency_frames();
    if f.get("latency_frames").is_some() {
        let want = f.num("latency_frames")? as usize;
        t.check(lat == want, || format!("{name}: latency {lat}"));
    }
    let frames = f.num("frames")? as usize;
    let room = f.num("room_gain")? as f32;
    let limit = f.num("limit_gain")? as f32;
    for o in 0..outs {
        if let Some(c) = f.get(&format!("exact.{o}")) {
            let c: usize = c.parse().map_err(|_| "exact.<o> names a channel")?;
            let x = &run.inputs[c];
            let y = &run.outputs[o];
            let ok = (0..frames).all(|i| {
                let want = if i >= lat { x[i - lat] * room } else { 0.0 };
                y[i].to_bits() == want.to_bits()
            });
            t.check(ok, || {
                format!("{name}: output {o} is not input {c} times the gain")
            });
        }
        if let Some(p) = f.get(&format!("amplitude.{o}")) {
            let want: f64 = p.parse().map_err(|_| "amplitude.<o> is a number")?;
            let from = f.num("amplitude_from")? as usize;
            let tol = f.num("amplitude_tolerance")?;
            let got = amplitude(&run.outputs[o][from..]);
            t.check((got - want).abs() <= tol, || {
                format!("{name}: output {o} has amplitude {got}, want {want}")
            });
        }
        if let Some(s) = f.get(&format!("samples.{o}")) {
            let want: Vec<f64> = s
                .split_whitespace()
                .map(|v| v.parse::<f64>().map_err(|_| "samples.<o>".to_string()))
                .collect::<Result<_, _>>()?;
            let from = f.num("samples_from")? as usize;
            let tol = f.num("samples_tolerance")?;
            let got = &run.outputs[o][from..from + want.len()];
            let worst = got
                .iter()
                .zip(&want)
                .fold(0f64, |m, (g, w)| m.max((*g as f64 - w).abs()));
            t.check(worst <= tol, || {
                format!("{name}: output {o} differs from its samples by {worst}")
            });
        }
    }
    if f.get("sum_amplitude").is_some() {
        let want = f.num("sum_amplitude")?;
        let from = f.num("amplitude_from")? as usize;
        let tol = f.num("amplitude_tolerance")?;
        let sum: Vec<f32> = (from..frames)
            .map(|i| run.outputs.iter().map(|o| o[i]).sum::<f32>())
            .collect();
        let got = amplitude(&sum);
        t.check((got - want).abs() <= tol, || {
            format!("{name}: the outputs' sum has amplitude {got}, want {want}")
        });
    }
    if f.get("ceiling_holds").is_some() {
        let ceiling = if limit < 1.0 { limit } else { 1.0 };
        let ok = run.outputs.iter().flatten().all(|v| v.abs() <= ceiling);
        t.check(ok, || format!("{name}: an output exceeded {ceiling}"));
    }
    Ok(())
}

#[test]
fn every_dsp_fixture_holds() {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir())
        .expect("fixtures/dsp exists")
        .map(|e| e.expect("a directory entry").path())
        .filter(|p| p.is_file())
        .collect();
    files.sort();
    assert!(!files.is_empty(), "fixtures/dsp holds no fixtures");
    let mut t = Tally::default();
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let text = std::fs::read_to_string(path).expect("a fixture reads");
        let f = Fields::parse(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
        // Provenance: every fixture cites what its expected values rest on.
        let cited = f.get("source").is_some_and(|s| !s.is_empty()) && f.get("read").is_some();
        t.check(cited, || format!("{name}: no source and read date"));
        let result = match f.get("kind").unwrap_or("") {
            "biquad_coefficients" => biquad_coefficients(&f, &mut t, &name),
            "biquad_magnitude" => biquad_magnitude(&f, &mut t, &name),
            "biquad_reference" => biquad_reference(&f, &mut t, &name),
            "lr4" => lr4(&f, &mut t, &name),
            "delay" => delay_fixture(&f, &mut t, &name),
            "limiter" => limiter_fixture(&f, &mut t, &name),
            "compressor_static" => compressor_static(&f, &mut t, &name),
            "compressor_timing" => compressor_timing(&f, &mut t, &name),
            "iso226" => iso226(&f, &mut t, &name),
            "loudness" => loudness_fixture(&f, &mut t, &name),
            "chain" => chain(&f, &mut t, &name),
            other => Err(format!("unknown kind '{other}'")),
        };
        if let Err(e) = result {
            t.failures.push(format!("{name}: {e}"));
        }
    }
    println!(
        "fixtures/dsp: {} fixtures, {} checks, {} failed",
        files.len(),
        t.checks,
        t.failures.len()
    );
    assert!(t.failures.is_empty(), "{:#?}", t.failures);
}
