//! Room-correction fitting from fixture recordings (goal 12, done-when line D): every recording
//! in `fixtures/roomfit/` regenerates byte for byte from its `.params`; from every room the fit
//! returns at most 8 filters inside the `room_eq` bounds, finds the room's modes, reduces the
//! in-band deviation, and leaves the nulls alone; every degenerate recording is refused by
//! name; and the sweep method itself is held to two properties Farina's paper states.

use chorus_dsp::biquad::{Coefficients, Kind};
use chorus_dsp::roomfit::synthetic::{parse_wav, wav_bytes, Params, Room};
use chorus_dsp::roomfit::{
    filter_json, fit_recording, fit_response, gain_db, impulse_response, in_bounds, log_grid, q,
    room_eq_command_json, smooth, FitConfig, RoomEqFilter, RoomFitError, Spectrum, Sweep, Target,
    ROOM_EQ_FREQ_MAX_HZ, ROOM_EQ_FREQ_MIN_HZ, ROOM_EQ_GAIN_MAX_CDB, ROOM_EQ_GAIN_MIN_CDB,
    ROOM_EQ_MAX_FILTERS, ROOM_EQ_Q_MAX_MILLI, ROOM_EQ_Q_MIN_MILLI,
};
use chorus_dsp::Biquad;
use std::path::{Path, PathBuf};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/roomfit")
}

/// Every fixture: its name, its parameters, its room and its recording.
struct Fixture {
    name: String,
    params: Params,
    room: Room,
    bytes: Vec<u8>,
}

fn all() -> Vec<Fixture> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(fixtures())
        .expect("fixtures/roomfit exists")
        .map(|e| e.expect("a directory entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "params"))
        .collect();
    paths.sort();
    assert!(
        paths.len() >= 6,
        "fixtures/roomfit holds {} rooms",
        paths.len()
    );
    paths
        .into_iter()
        .map(|p| {
            let params = Params::parse(&std::fs::read_to_string(&p).unwrap()).unwrap();
            let room = Room::from_params(&params).unwrap();
            let bytes = std::fs::read(fixtures().join(params.text("output").unwrap())).unwrap();
            Fixture {
                name: p.file_stem().unwrap().to_string_lossy().into_owned(),
                params,
                room,
                bytes,
            }
        })
        .collect()
}

/// The fixtures whose `expect` is `fit`, each fitted with the defaults to a flat target.
fn fitted() -> Vec<(Fixture, chorus_dsp::roomfit::Fit)> {
    all()
        .into_iter()
        .filter(|f| f.params.text("expect").unwrap() == "fit")
        .map(|f| {
            let (rate, samples) = parse_wav(&f.bytes).unwrap();
            assert_eq!(rate, f.room.sweep.rate_hz);
            let fit = fit_recording(
                &samples,
                &f.room.sweep,
                &Target::flat(),
                &FitConfig::default(),
            )
            .unwrap_or_else(|e| panic!("{}: {e}", f.name));
            eprintln!(
                "{}: level {:.2} dB, band {:.1}..{:.1} Hz, rms {:.2} -> {:.2} dB, filters {:?}",
                f.name,
                fit.level_db,
                fit.band_hz.0,
                fit.band_hz.1,
                fit.rms_before_db(),
                fit.rms_after_db(),
                fit.filters
                    .iter()
                    .map(|p| (p.freq_hz, gain_db(p), q(p)))
                    .collect::<Vec<_>>()
            );
            (f, fit)
        })
        .collect()
}

#[test]
fn every_recording_regenerates_byte_for_byte_from_its_params() {
    for f in all() {
        let made = wav_bytes(f.room.sweep.rate_hz, &f.room.render().unwrap());
        assert!(
            made == f.bytes,
            "{}: regenerating does not reproduce the committed recording (make roomfit-fixtures)",
            f.name
        );
        assert!(
            f.bytes.len() < 200_000,
            "{}: {} bytes",
            f.name,
            f.bytes.len()
        );
    }
}

/// (a) At most 8 filters, each inside the room_eq bounds, and the command the catalog takes.
#[test]
fn every_fit_is_at_most_eight_filters_inside_the_bounds() {
    let fits = fitted();
    assert_eq!(fits.len(), 3);
    for (f, fit) in &fits {
        assert!(!fit.filters.is_empty(), "{}: no filters", f.name);
        assert!(
            fit.filters.len() <= ROOM_EQ_MAX_FILTERS,
            "{}: {} filters",
            f.name,
            fit.filters.len()
        );
        for p in &fit.filters {
            assert!(in_bounds(p), "{}: {p:?} is outside the bounds", f.name);
            assert!((ROOM_EQ_FREQ_MIN_HZ..=ROOM_EQ_FREQ_MAX_HZ).contains(&p.freq_hz));
            assert!((ROOM_EQ_GAIN_MIN_CDB..=ROOM_EQ_GAIN_MAX_CDB).contains(&p.gain_cdb));
            assert!((ROOM_EQ_Q_MIN_MILLI..=ROOM_EQ_Q_MAX_MILLI).contains(&p.q_milli));
        }
        // The boost rules: no boost below 100 Hz, and every boost rings out within 0.5 s.
        for p in fit.filters.iter().filter(|p| p.gain_cdb > 0) {
            assert!(p.freq_hz >= 100, "{}: a boost at {} Hz", f.name, p.freq_hz);
            let a = 10f64.powf(gain_db(p) / 40.0);
            let t60 = 1000f64.ln() * q(p) * a / (std::f64::consts::PI * f64::from(p.freq_hz));
            assert!(t60 <= 0.5, "{}: {p:?} rings for {t60:.3} s", f.name);
        }
        let json = room_eq_command_json("living", &fit.filters);
        assert!(json.starts_with(
            "{\"v\":2,\"t\":\"room_eq\",\"zone\":\"living\",\"filters\":[{\"freq_hz\":"
        ));
        assert_eq!(json.matches("\"freq_hz\"").count(), fit.filters.len());
    }
}

/// (b) Every mode the room was made with has a cut within 1/12 octave of it (ASSUMED tolerance:
/// the smoothing's own width), and a mode inside the cut bound is cut to within 2.5 dB of its
/// height (a mode beyond the bound by at least the bound's 12 dB, possibly over two filters).
#[test]
fn the_fit_finds_every_synthetic_resonance() {
    let tol = 2f64.powf(1.0 / 12.0);
    for (f, fit) in fitted() {
        for m in &f.room.modes {
            let near: Vec<&RoomEqFilter> = fit
                .filters
                .iter()
                .filter(|p| {
                    let r = f64::from(p.freq_hz) / m.freq_hz;
                    p.gain_cdb < 0 && r < tol && r > 1.0 / tol
                })
                .collect();
            assert!(
                !near.is_empty(),
                "{}: no cut near the {} Hz mode",
                f.name,
                m.freq_hz
            );
            let cut = -fit.correction_db(48_000.0, m.freq_hz);
            let want = m.gain_db.min(12.0);
            assert!(
                cut >= want - 2.5 && cut <= m.gain_db + 2.5,
                "{}: the correction at the {} Hz mode (+{} dB) cuts {cut:.2} dB",
                f.name,
                m.freq_hz,
                m.gain_db
            );
        }
    }
}

/// (c) The RMS deviation from the target over the fit band, outside the points left alone (the
/// nulls and the speaker's roll-off), falls by at least a factor of 4 (ASSUMED: the fixtures
/// hold about 10), and so it does when measured against the room's true response rather than
/// the fit's own smoothed estimate.
#[test]
fn the_fit_reduces_the_in_band_deviation() {
    for (f, fit) in fitted() {
        let before = fit.rms_before_db();
        let after = fit.rms_after_db();
        assert!(
            after * 4.0 <= before,
            "{}: rms {before:.2} -> {after:.2} dB",
            f.name
        );
        // The research's further criteria (docs/research/room-correction-sources.md, 1f): the
        // largest deviation above the target after correction is at most 3 dB, and no point that
        // was above the target ends more than 0.5 dB above where it was (a cut never makes a peak
        // worse).
        for ((x, d), r) in fit
            .grid_hz
            .iter()
            .zip(&fit.deviation_db)
            .zip(&fit.residual_db)
        {
            assert!(
                *r <= 3.0,
                "{}: {r:.2} dB above the target at {x:.1} Hz",
                f.name
            );
            if *d > 0.0 {
                assert!(
                    r - d <= 0.5,
                    "{}: {x:.1} Hz went from {d:.2} to {r:.2} dB",
                    f.name
                );
            }
        }
        // Against the ground truth: the room's response (level-free) on the same grid, outside
        // the nulls.
        let truth = |g: &[f64], with: bool| -> f64 {
            let v: Vec<f64> = g
                .iter()
                .zip(&fit.left_alone)
                .filter(|(_, n)| !**n)
                .map(|(x, _)| x)
                .map(|x| {
                    f.room.response_db(*x).unwrap()
                        + if with {
                            fit.correction_db(48_000.0, *x)
                        } else {
                            0.0
                        }
                })
                .collect();
            let mean = v.iter().sum::<f64>() / v.len() as f64;
            (v.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / v.len() as f64).sqrt()
        };
        let (tb, ta) = (truth(&fit.grid_hz, false), truth(&fit.grid_hz, true));
        eprintln!("{}: true rms {tb:.2} -> {ta:.2} dB", f.name);
        assert!(ta * 4.0 <= tb, "{}: true rms {tb:.2} -> {ta:.2} dB", f.name);
    }
}

/// (d) At every null the combined correction boosts no more than the configured maximum (+3.00
/// dB), and nowhere from 10 Hz to 20 kHz does it boost more than that.
#[test]
fn the_fit_does_not_fill_a_null_beyond_the_max_boost() {
    let cfg = FitConfig::default();
    let mut nulls = 0;
    for (f, fit) in fitted() {
        for n in &f.room.nulls {
            nulls += 1;
            let at = fit.correction_db(cfg.model_rate_hz, n.freq_hz);
            assert!(
                at <= cfg.max_boost_db + 1e-9,
                "{}: +{at:.2} dB at the {} Hz null",
                f.name,
                n.freq_hz
            );
        }
        for x in log_grid(10.0, 20_000.0, 96.0) {
            let c = fit.correction_db(cfg.model_rate_hz, x);
            assert!(
                c <= cfg.max_boost_db + 1e-9,
                "{}: +{c:.3} dB at {x:.1} Hz",
                f.name
            );
        }
    }
    assert!(nulls >= 2);
}

/// (e) A recording that is too quiet, clipped, too short or too noisy is refused, by name.
#[test]
fn degenerate_recordings_are_refused_by_name() {
    let mut seen = Vec::new();
    for f in all() {
        let expect = f.params.text("expect").unwrap().to_string();
        if expect == "fit" {
            continue;
        }
        let (_, samples) = parse_wav(&f.bytes).unwrap();
        let err = fit_recording(
            &samples,
            &f.room.sweep,
            &Target::flat(),
            &FitConfig::default(),
        )
        .expect_err(&f.name);
        eprintln!("{}: {err}", f.name);
        assert_eq!(err.name(), expect, "{}: {err}", f.name);
        assert!(err.to_string().starts_with(&format!("{expect}: ")));
        seen.push(expect);
    }
    seen.sort();
    assert_eq!(seen, ["clipped", "too_noisy", "too_quiet", "too_short"]);
}

/// A misconfigured fit is refused, never fitted outside the bounds.
#[test]
fn a_configuration_outside_the_bounds_is_refused() {
    let grid = log_grid(20.0, 3000.0, 48.0);
    let flat = vec![0.0; grid.len()];
    let mut cfg = FitConfig {
        max_boost_db: 6.0,
        ..FitConfig::default()
    };
    assert_eq!(
        fit_response(&grid, &flat, &Target::flat(), &cfg)
            .unwrap_err()
            .name(),
        "bad_config"
    );
    cfg = FitConfig {
        band_hi_hz: 2000.0,
        ..FitConfig::default()
    };
    assert_eq!(
        fit_response(&grid, &flat, &Target::flat(), &cfg)
            .unwrap_err()
            .name(),
        "bad_config"
    );
    // A flat response needs nothing.
    let fit = fit_response(&grid, &flat, &Target::flat(), &FitConfig::default()).unwrap();
    assert!(fit.filters.is_empty());
}

/// With boosting off entirely, a dip-only response gets no filter at all, and a peak gets a cut.
#[test]
fn with_no_boost_allowed_only_cuts_are_fitted() {
    let cfg = FitConfig {
        max_boost_db: 0.0,
        ..FitConfig::default()
    };
    let grid = log_grid(20.0, 3000.0, 48.0);
    let resp: Vec<f64> = grid
        .iter()
        .map(|f| {
            -4.0 * (-((f / 60.0f64).log2() * 4.0).powi(2)).exp()
                + 6.0 * (-((f / 140.0f64).log2() * 5.0).powi(2)).exp()
        })
        .collect();
    let fit = fit_response(&grid, &resp, &Target::flat(), &cfg).unwrap();
    assert!(!fit.filters.is_empty());
    assert!(
        fit.filters.iter().all(|p| p.gain_cdb < 0),
        "{:?}",
        fit.filters
    );
    for x in log_grid(10.0, 20_000.0, 96.0) {
        assert!(fit.correction_db(cfg.model_rate_hz, x) <= 1e-9);
    }
}

/// A target curve is honoured: fitted to a room's own response with a +4 dB low-frequency
/// shelf in the target, the residual against that target is what falls.
#[test]
fn a_target_curve_moves_the_fit() {
    let grid = log_grid(20.0, 3000.0, 48.0);
    let resp: Vec<f64> = grid
        .iter()
        .map(|f| 8.0 * (-((f / 80.0f64).log2() * 5.0).powi(2)).exp())
        .collect();
    let house = Target::new(vec![(40.0, 4.0), (200.0, 0.0)]);
    assert_eq!(house.at(20.0), 4.0);
    assert_eq!(house.at(1000.0), 0.0);
    assert!((house.at((40.0f64 * 200.0).sqrt()) - 2.0).abs() < 1e-12);
    let flat = fit_response(&grid, &resp, &Target::flat(), &FitConfig::default()).unwrap();
    let tilted = fit_response(&grid, &resp, &house, &FitConfig::default()).unwrap();
    // The house curve asks for more low end, so it cuts the 80 Hz mode less.
    assert!(tilted.correction_db(48_000.0, 80.0) > flat.correction_db(48_000.0, 80.0) + 1.0);
}

/// The recommended sweep (5 s, with its 0.1 s fade-in) holds the same two properties: the
/// instantaneous frequency of the formula and a flat, single impulse through its inverse.
#[test]
fn the_recommended_sweep_is_a_delayed_impulse_too() {
    let s = Sweep::recommended(48_000);
    assert_eq!(s.samples, 240_000);
    assert!((s.instantaneous_hz(s.duration_s()) - s.f2_hz).abs() < 1e-6);
    assert!((s.instantaneous_hz(0.0) - s.f1_hz).abs() < 1e-12);
    let mut rec: Vec<f32> = s.signal().iter().map(|x| *x as f32).collect();
    rec.extend(std::iter::repeat_n(0.0f32, 30_000));
    let ir = impulse_response(&rec, &s, &FitConfig::default()).unwrap();
    assert_eq!(ir.delay_samples, 0);
    let spectrum = Spectrum::of(&ir);
    let grid = log_grid(20.0, 10_000.0, 12.0);
    for (f, db) in grid.iter().zip(smooth(&spectrum, &grid, 3.0)) {
        assert!(db.abs() < 1.0, "{f:.0} Hz: {db:.2} dB");
    }
}

fn fixture_sweep() -> Sweep {
    Sweep {
        rate_hz: 48_000,
        f1_hz: 10.0,
        f2_hz: 20_000.0,
        samples: 48_000,
        amplitude: 0.5,
        fade_in_samples: 0,
    }
}

/// Worked example, Farina 2000 section 4: the sweep's instantaneous frequency is
/// f(t) = f1 e^((t/T) ln(f2/f1)). Measured from the sweep's own zero crossings (half a period
/// between successive ones), it must match the formula within 0.5% at 1/8, 1/4, 1/2 and 3/4 of
/// the sweep.
#[test]
fn the_sweep_follows_farinas_instantaneous_frequency() {
    let s = fixture_sweep();
    let x = s.signal();
    let rate = f64::from(s.rate_hz);
    // Zero crossings, interpolated linearly between samples.
    let mut zc = Vec::new();
    for i in 1..x.len() {
        if (x[i - 1] < 0.0) != (x[i] < 0.0) && x[i - 1] != x[i] {
            zc.push((i as f64 - 1.0 + x[i - 1] / (x[i - 1] - x[i])) / rate);
        }
    }
    let t_total = s.duration_s();
    for frac in [0.125, 0.25, 0.5, 0.75] {
        let t = frac * t_total;
        // Two successive crossings are half a period apart; their midpoint is where that
        // frequency holds (to 0.1% at 25 Hz, where the frequency moves fastest per period).
        let i = zc.iter().position(|c| *c >= t).unwrap();
        let (a, b) = (zc[i], zc[i + 1]);
        let measured = 1.0 / (2.0 * (b - a));
        let formula = s.f1_hz * ((((a + b) / 2.0) / t_total) * (s.f2_hz / s.f1_hz).ln()).exp();
        assert!(
            (measured / formula - 1.0).abs() < 0.005,
            "t {t}: {measured} Hz against {formula} Hz"
        );
        assert!((s.instantaneous_hz((a + b) / 2.0) / formula - 1.0).abs() < 1e-12);
    }
}

/// Worked example, Farina 2000 section 3: the sweep convolved with its inverse filter is a
/// delayed Dirac delta. Here: the peak lands at exactly the sweep's length less one, its
/// magnitude response is flat within 1 dB (1/3-octave smoothed) over the band an octave inside the sweep's ends,
/// and nothing more than 30 ms from the peak rises above -60 dB of it.
#[test]
fn the_sweep_through_its_inverse_is_a_delayed_impulse() {
    let s = fixture_sweep();
    let rec: Vec<f32> = {
        let mut v: Vec<f32> = s.signal().iter().map(|x| *x as f32).collect();
        v.extend(std::iter::repeat_n(0.0f32, 30_000));
        v
    };
    let cfg = FitConfig::default();
    let ir = impulse_response(&rec, &s, &cfg).unwrap();
    assert_eq!(
        ir.delay_samples, 0,
        "the impulse lands at the sweep's length less one"
    );
    let pre = (cfg.pre_window_s * 48_000.0) as usize;
    let peak = ir.samples[pre];
    let (peak_at, _) = ir
        .samples
        .iter()
        .enumerate()
        .fold((0, 0.0f64), |(bi, bv), (i, v)| {
            if v.abs() > bv {
                (i, v.abs())
            } else {
                (bi, bv)
            }
        });
    assert_eq!(peak_at, pre);
    // A unit impulse band-limited to f1..f2 peaks at the band's share of the spectrum,
    // (f2 - f1) / (rate / 2).
    let want = (s.f2_hz - s.f1_hz) / 24_000.0;
    // Within 5%: the band's two ends, where the sweep starts and fades, are not flat.
    assert!((peak / want - 1.0).abs() < 0.05, "peak {peak}, want {want}");
    for (i, v) in ir.samples.iter().enumerate() {
        if i.abs_diff(pre) > 1440 {
            assert!(
                20.0 * (v.abs() / peak.abs()).log10() < -60.0,
                "sample {i}: {v}"
            );
        }
    }
    let spectrum = Spectrum::of(&ir);
    let grid = log_grid(20.0, 10_000.0, 12.0);
    for (f, db) in grid.iter().zip(smooth(&spectrum, &grid, 3.0)) {
        assert!(db.abs() < 1.0, "{f:.0} Hz: {db:.2} dB");
    }
}

/// The refusals' wording names each one, for the measurement UX.
#[test]
fn refusals_name_themselves() {
    let cases = [
        (
            RoomFitError::TooShort {
                samples: 1,
                needed: 2,
            },
            "too_short",
        ),
        (
            RoomFitError::Clipped {
                at_sample: 1,
                run: 3,
            },
            "clipped",
        ),
        (
            RoomFitError::TooQuiet {
                peak_dbfs: -60.0,
                min_dbfs: -50.0,
            },
            "too_quiet",
        ),
        (
            RoomFitError::TooNoisy {
                snr_db: 10.0,
                min_db: 40.0,
            },
            "too_noisy",
        ),
        (RoomFitError::BadConfig("x".into()), "bad_config"),
    ];
    for (e, name) in cases {
        assert_eq!(e.name(), name);
        assert!(e.to_string().starts_with(&format!("{name}: ")));
    }
}

/// The catalog spelling, byte for byte as the catalog's own `room_eq` vector
/// (`fixtures/control/v2/room_eq.json`, goal 12's sound-catalog track) spells it: whole Hz, gain
/// with two places, Q with three.
#[test]
fn a_fit_is_spelled_as_the_catalog_spells_room_eq() {
    let filters = [
        RoomEqFilter {
            freq_hz: 42,
            gain_cdb: -600,
            q_milli: 4500,
        },
        RoomEqFilter {
            freq_hz: 120,
            gain_cdb: -325,
            q_milli: 2000,
        },
    ];
    assert_eq!(
        room_eq_command_json("living", &filters),
        "{\"v\":2,\"t\":\"room_eq\",\"zone\":\"living\",\"filters\":[{\"freq_hz\":42,\"gain_db\":-6.00,\
         \"q\":4.500},{\"freq_hz\":120,\"gain_db\":-3.25,\"q\":2.000}],\"enabled\":true}"
    );
    let edge = RoomEqFilter {
        freq_hz: 20,
        gain_cdb: -5,
        q_milli: 10_000,
    };
    assert_eq!(
        filter_json(&edge),
        "{\"freq_hz\":20,\"gain_db\":-0.05,\"q\":10.000}"
    );
    let edge = RoomEqFilter {
        freq_hz: 1000,
        gain_cdb: 300,
        q_milli: 500,
    };
    assert_eq!(
        filter_json(&edge),
        "{\"freq_hz\":1000,\"gain_db\":3.00,\"q\":0.500}"
    );
    assert_eq!(
        room_eq_command_json("a\"b", &[]),
        "{\"v\":2,\"t\":\"room_eq\",\"zone\":\"a\\\"b\",\"filters\":[],\"enabled\":true}"
    );
}

/// The fit's model is what the endpoints run: each room's recording, played through the fitted
/// filters as the chain runs them (the RBJ design rounded once to f32, Transposed Direct Form II
/// in f32), measures as the fit predicted. Re-measured with the same analysis, outside the points
/// left alone, the corrected recording's deviation matches the fit's predicted residual with an
/// RMS difference under 0.25 dB and no point more than 1.25 dB apart (ASSUMED tolerances: the
/// prediction adds the correction to the smoothed response in dB, while the measurement smooths
/// the corrected power, and the two differ most at a sharp mode's centre; the fixtures hold an
/// RMS of 0.12 to 0.13 dB and a worst point of 0.53 to 0.99 dB), and the corrected recording's own
/// deviation is down at least 4 times from the uncorrected one.
#[test]
fn the_filters_as_the_endpoints_run_them_correct_the_recording_as_predicted() {
    let cfg = FitConfig::default();
    for (f, fit) in fitted() {
        let (_, samples) = parse_wav(&f.bytes).unwrap();
        let mut sections: Vec<Biquad> = fit
            .filters
            .iter()
            .map(|p| {
                Biquad::new(
                    &Coefficients::design(
                        Kind::Peaking,
                        cfg.model_rate_hz,
                        f64::from(p.freq_hz),
                        q(p),
                        gain_db(p),
                    )
                    .unwrap(),
                )
            })
            .collect();
        let corrected: Vec<f32> = samples
            .iter()
            .map(|x| sections.iter_mut().fold(*x, |v, s| s.process(v)))
            .collect();
        let again = fit_recording(&corrected, &f.room.sweep, &Target::flat(), &cfg).unwrap();
        // Compare on the first fit's grid and band; the second fit's own band edge may differ.
        let measured = |x: f64| -> f64 {
            let i = again.grid_hz.iter().position(|g| (g - x).abs() < 1e-9);
            i.map(|i| again.deviation_db[i] + again.level_db - fit.level_db)
                .unwrap_or(f64::NAN)
        };
        let mut compared = 0;
        let (mut worst, mut sq) = (0.0f64, 0.0f64);
        for (i, x) in fit.grid_hz.iter().enumerate() {
            let m = measured(*x);
            if fit.left_alone[i] || m.is_nan() {
                continue;
            }
            compared += 1;
            let predicted = fit.residual_db[i];
            worst = worst.max((m - predicted).abs());
            sq += (m - predicted).powi(2);
            assert!(
                (m - predicted).abs() < 1.25,
                "{}: at {x:.1} Hz the corrected recording measures {m:.2} dB, the fit predicted \
                 {predicted:.2} dB",
                f.name
            );
        }
        eprintln!(
            "{}: corrected vs predicted over {compared} points: worst {worst:.2} dB, rms {:.2} dB",
            f.name,
            (sq / f64::from(compared)).sqrt()
        );
        assert!(compared > 100, "{}: {compared} points compared", f.name);
        assert!((sq / f64::from(compared)).sqrt() < 0.25, "{}", f.name);
        assert!(
            again.rms_before_db() * 4.0 <= fit.rms_before_db(),
            "{}: corrected {:.2} dB, uncorrected {:.2} dB",
            f.name,
            again.rms_before_db(),
            fit.rms_before_db()
        );
    }
}
