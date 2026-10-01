//! The visualizer's fixture audio (`fixtures/visualizer/`, Rust only by
//! declaration: the server computes the stream, no endpoint does).
//!
//! Every `.params` file names a recipe (`kind`), the audio it makes (a mono
//! 16-bit WAV beside it) and what the analysis of that audio must show
//! (`expect`). Two things are checked for each:
//!
//! 1. Regenerating the audio from its parameters reproduces the committed
//!    WAV byte for byte (`CHORUS_WRITE_FIXTURES=1` rewrites it instead). The
//!    recipes go through `sin`, `cos` and `exp` in the platform's maths
//!    library; the caveat `crates/measure/src/fixtures.rs` states about one
//!    ulp and a 16-bit rounding boundary applies here unchanged.
//! 2. The analysis of the committed WAV meets the expectation: beats at the
//!    known onsets within the stated tolerance, the band holding a tone's
//!    energy and the peak's mapping, a sweep's band and hue climbing, and
//!    silence silent.
//!
//! Nothing here is timing evidence: the audio is synthetic and the instants
//! are sample indices.

use std::collections::BTreeMap;
use std::f64::consts::PI;
use std::path::{Path, PathBuf};

use chorus_dsp::visualizer::{level_byte, Analyzer, Frame, BANDS};

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/visualizer")
}

struct Params {
    path: PathBuf,
    values: BTreeMap<String, String>,
}

impl Params {
    fn load(path: &Path) -> Params {
        let text = std::fs::read_to_string(path).unwrap();
        let mut values = BTreeMap::new();
        for (i, line) in text.lines().enumerate() {
            let line = line.split('#').next().unwrap().trim();
            if line.is_empty() {
                continue;
            }
            let (k, v) = line
                .split_once('=')
                .unwrap_or_else(|| panic!("{}:{}: not key = value", path.display(), i + 1));
            values.insert(k.trim().to_string(), v.trim().to_string());
        }
        Params {
            path: path.to_path_buf(),
            values,
        }
    }

    fn text(&self, key: &str) -> &str {
        self.values
            .get(key)
            .unwrap_or_else(|| panic!("{}: no {}", self.path.display(), key))
    }

    fn num(&self, key: &str) -> f64 {
        self.text(key)
            .parse()
            .unwrap_or_else(|_| panic!("{}: {} is not a number", self.path.display(), key))
    }

    fn nums(&self, key: &str) -> Vec<f64> {
        self.text(key)
            .split_whitespace()
            .map(|v| v.parse().unwrap())
            .collect()
    }
}

/// The recipe's samples, -1.0 to 1.0.
fn render(p: &Params) -> Vec<f64> {
    let rate = p.num("sample_rate_hz");
    let len = (p.num("duration_ms") * rate / 1_000.0).round() as usize;
    let t = |n: usize| n as f64 / rate;
    match p.text("kind") {
        "silence" => vec![0.0; len],
        "tone" => {
            let (f, a) = (p.num("tone_hz"), p.num("amplitude"));
            (0..len).map(|n| a * (2.0 * PI * f * t(n)).sin()).collect()
        }
        "sweep" => {
            let (f0, f1, a) = (
                p.num("sweep_start_hz"),
                p.num("sweep_end_hz"),
                p.num("amplitude"),
            );
            let span = p.num("duration_ms") / 1_000.0;
            let k = (f1 / f0).ln();
            (0..len)
                .map(|n| a * (2.0 * PI * f0 * span / k * ((t(n) / span * k).exp() - 1.0)).sin())
                .collect()
        }
        "kick" => {
            let period = (60.0 / p.num("bpm") * rate).round() as usize;
            let kick = (p.num("kick_ms") * rate / 1_000.0).round() as usize;
            let (a, fs, fe) = (
                p.num("amplitude"),
                p.num("kick_start_hz"),
                p.num("kick_end_hz"),
            );
            let (pt, dt) = (
                p.num("pitch_tau_ms") / 1_000.0,
                p.num("decay_tau_ms") / 1_000.0,
            );
            (0..len)
                .map(|n| {
                    let i = n % period;
                    if i >= kick {
                        return 0.0;
                    }
                    let s = t(i);
                    let phase = 2.0 * PI * (fe * s + (fs - fe) * pt * (1.0 - (-s / pt).exp()));
                    a * (-s / dt).exp() * phase.cos()
                })
                .collect()
        }
        other => panic!("{}: no recipe {}", p.path.display(), other),
    }
}

/// A mono 16-bit PCM WAV of `samples` (RIFF, the canonical 44-byte header).
fn wav(rate: u32, samples: &[f64]) -> Vec<u8> {
    let data = samples.len() as u32 * 2;
    let mut w = Vec::with_capacity(44 + data as usize);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&(rate * 2).to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data.to_le_bytes());
    for x in samples {
        let q = (x * 32_767.0).round().clamp(-32_768.0, 32_767.0) as i16;
        w.extend_from_slice(&q.to_le_bytes());
    }
    w
}

/// The committed WAV's samples as the analyser takes them.
fn read_wav(bytes: &[u8]) -> Vec<f32> {
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(&bytes[36..40], b"data");
    bytes[44..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32_768.0)
        .collect()
}

fn fixtures() -> Vec<Params> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir())
        .expect("fixtures/visualizer exists")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "params"))
        .collect();
    paths.sort();
    paths.iter().map(|p| Params::load(p)).collect()
}

/// The analysis of `samples` with a second of silence after, so the last
/// onset is decided and the last frame completes.
fn analyse(rate: u32, samples: &[f32]) -> (Analyzer, Vec<Frame>) {
    let mut a = Analyzer::new(rate).expect("a supported rate");
    let mut frames = Vec::new();
    a.push(samples, 1, &mut frames);
    a.push(&vec![0.0; rate as usize], 1, &mut frames);
    (a, frames)
}

#[test]
fn every_fixture_regenerates_byte_for_byte() {
    let all = fixtures();
    assert_eq!(all.len(), 4, "the four fixtures");
    for p in &all {
        let bytes = wav(p.num("sample_rate_hz") as u32, &render(p));
        let out = dir().join(p.text("output"));
        if std::env::var_os("CHORUS_WRITE_FIXTURES").is_some() {
            std::fs::write(&out, &bytes).unwrap();
        }
        let committed = std::fs::read(&out).unwrap_or_else(|e| panic!("{}: {}", out.display(), e));
        assert!(
            committed == bytes,
            "{} does not regenerate from {} byte for byte",
            out.display(),
            p.path.display()
        );
    }
}

fn hue(f: &Frame) -> f64 {
    let c = f.colour.unwrap();
    let (r, g, b) = (f64::from(c.red), f64::from(c.green), f64::from(c.blue));
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let d = max - min;
    60.0 * if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    }
}

fn loudest(f: &Frame) -> usize {
    (0..BANDS)
        .max_by(|a, b| f.band_db[*a].total_cmp(&f.band_db[*b]))
        .unwrap()
}

#[test]
fn every_fixture_shows_what_it_says() {
    for p in fixtures() {
        let rate = p.num("sample_rate_hz") as u32;
        let samples = read_wav(&std::fs::read(dir().join(p.text("output"))).unwrap());
        let (a, frames) = analyse(rate, &samples);
        let ms = |f: &Frame| f.at_sample as f64 * 1_000.0 / f64::from(rate);
        let name = p.path.file_name().unwrap().to_string_lossy().to_string();
        match p.text("expect") {
            "beats" => {
                let want = p.nums("expect_beats_ms");
                let tolerance = p.num("beat_tolerance_ms");
                let beats: Vec<(f64, u8)> = frames
                    .iter()
                    .filter(|f| f.beat > 0)
                    .map(|f| (ms(f), f.beat))
                    .collect();
                for (at, strength) in &beats {
                    println!("{name}: beat at {at:.1} ms strength {strength}");
                }
                assert_eq!(
                    beats.len(),
                    want.len(),
                    "{name}: one beat per kick: {beats:?}"
                );
                for ((at, strength), w) in beats.iter().zip(&want) {
                    assert!(
                        (at - w).abs() <= tolerance,
                        "{name}: a beat at {at} ms for the kick at {w} ms"
                    );
                    assert!(
                        f64::from(*strength) >= p.num("beat_min"),
                        "{name}: the beat at {at} ms is only {strength}"
                    );
                }
            }
            "tone" => {
                let band = a.band_of(p.num("tone_hz")).unwrap();
                let level = p.num("expect_band_db");
                let peak = level_byte(level);
                let settled: Vec<&Frame> = frames
                    .iter()
                    .filter(|f| ms(f) >= p.num("settle_ms") && ms(f) < p.num("duration_ms") - 100.0)
                    .collect();
                assert!(settled.len() >= 20, "{name}: {} frames", settled.len());
                for f in &settled {
                    let top = loudest(f);
                    assert!(
                        top.abs_diff(band) <= 1,
                        "{name}: loudest band {top}, tone in {band}"
                    );
                    let power: f64 = f.band_db[band - 1..=band + 1]
                        .iter()
                        .map(|d| 10f64.powf(f64::from(*d) / 10.0))
                        .sum();
                    let db = 10.0 * power.log10();
                    assert!(
                        (db - level).abs() <= p.num("band_tolerance_db"),
                        "{name}: the tone's bands hold {db:.2} dB, want {level}"
                    );
                    assert!(
                        f64::from(f.peak.abs_diff(peak)) <= p.num("peak_tolerance"),
                        "{name}: peak {} want {peak}",
                        f.peak
                    );
                    assert_eq!(f.beat, 0, "{name}: a steady tone does not beat");
                }
                println!(
                    "{name}: band {band} loudest, {:.2} dB over its neighbours, peak {}",
                    {
                        let f = settled[settled.len() / 2];
                        let power: f64 = f.band_db[band - 1..=band + 1]
                            .iter()
                            .map(|d| 10f64.powf(f64::from(*d) / 10.0))
                            .sum();
                        10.0 * power.log10()
                    },
                    settled[0].peak
                );
            }
            "rising" => {
                let heard: Vec<&Frame> = frames
                    .iter()
                    .filter(|f| ms(f) < p.num("duration_ms") && f.peak > 0)
                    .collect();
                let mut highest = 0usize;
                for f in &heard {
                    let top = loudest(f);
                    assert!(
                        top + 1 >= highest,
                        "{name}: the loudest band fell from {highest} to {top} at {} ms",
                        ms(f)
                    );
                    highest = highest.max(top);
                }
                let (first, last) = (loudest(heard[2]), loudest(heard[heard.len() - 1]));
                assert!(last > first + 30, "{name}: from band {first} to {last}");
                let colours: Vec<&Frame> = heard
                    .iter()
                    .copied()
                    .filter(|f| f.colour.is_some())
                    .collect();
                assert!(colours.len() >= 2, "{name}: {} colours", colours.len());
                let (h0, h1) = (hue(colours[0]), hue(colours[colours.len() - 1]));
                assert!(
                    h0 > h1 + 60.0,
                    "{name}: the hue went from {h0:.0} to {h1:.0}"
                );
                println!(
                    "{name}: loudest band {first} -> {last}, hue {h0:.0} -> {h1:.0} over {} colours",
                    colours.len()
                );
            }
            "silent" => {
                assert!(!frames.is_empty());
                assert!(
                    frames.iter().all(|f| f.is_silent() && f.colour.is_none()),
                    "{name}"
                );
                println!("{name}: {} frames, all silent, no colour", frames.len());
            }
            other => panic!("{name}: no expectation {other}"),
        }
    }
}
