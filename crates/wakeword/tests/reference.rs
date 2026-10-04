//! The detector against the committed fixtures in `fixtures/wakeword/`.
//!
//! Two kinds of claim are checked here. The first is the task's own: the
//! wake phrase is detected, with its name, and silence, noise and music are
//! not. The second is what makes the first trustworthy: the frontend and the
//! interpreter give, value for value, what the upstream implementation gave
//! for the same audio (`fixtures/wakeword/make-reference.py` wrote those
//! files with TensorFlow Lite's own frontend and its reference kernels).

use std::path::{Path, PathBuf};

use chorus_wakeword::frontend::{Frontend, CHANNELS, STEP, WINDOW};
use chorus_wakeword::{builtin, Detector, Error, Manifest};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/wakeword")
}

/// The samples of a 16 kHz mono 16-bit WAV file.
fn wav(name: &str) -> Vec<i16> {
    let bytes = std::fs::read(fixtures().join(name)).unwrap();
    assert_eq!(&bytes[..4], b"RIFF", "{name}");
    assert_eq!(&bytes[8..16], b"WAVEfmt ", "{name}");
    let u16_at = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    let u32_at =
        |at: usize| u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    // PCM, mono, 16 kHz, 16 bits.
    assert_eq!(
        (u16_at(20), u16_at(22), u32_at(24), u16_at(34)),
        (1, 1, 16_000, 16),
        "{name}"
    );
    let mut at = 20 + u32_at(16) as usize;
    while &bytes[at..at + 4] != b"data" {
        at += 8 + u32_at(at + 4) as usize;
    }
    let len = u32_at(at + 4) as usize;
    bytes[at + 8..at + 8 + len]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| i16::from_le_bytes(*c))
        .collect()
}

fn numbers(name: &str) -> Vec<Vec<u16>> {
    let text = std::fs::read_to_string(fixtures().join(name)).unwrap();
    text.lines()
        .map(|l| l.split(' ').map(|v| v.parse().unwrap()).collect())
        .collect()
}

fn okay_nabu() -> Detector {
    let model = builtin()
        .iter()
        .find(|b| b.name == "okay_nabu")
        .expect("the vendored okay_nabu model");
    Detector::from_builtin(model).unwrap()
}

fn features(samples: &[i16]) -> Vec<[u16; CHANNELS]> {
    let mut frontend = Frontend::new();
    let mut rest = samples;
    let mut frames = Vec::new();
    while !rest.is_empty() {
        let (taken, frame) = frontend.process(rest);
        rest = &rest[taken..];
        frames.extend(frame);
    }
    frames
}

/// A linear congruential generator: the same noise on every machine.
fn noise(samples: usize, amplitude: i32) -> Vec<i16> {
    let mut state: u32 = 0x1234_5678;
    (0..samples)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (((state >> 16) as i32 - 32768) * amplitude / 32768) as i16
        })
        .collect()
}

/// The finish line: the phrase is detected, by name, in the wake-phrase fixture.
#[test]
fn the_wake_phrase_is_detected_with_its_name() {
    let samples = wav("okay-nabu.wav");
    let mut detector = okay_nabu();
    assert_eq!(detector.phrase(), "Okay Nabu");
    let detections = detector.process(&samples);
    assert_eq!(detections.len(), 1, "{detections:?}");
    let d = &detections[0];
    assert_eq!(d.phrase, "Okay Nabu");
    assert!(d.probability > 0.97, "{d:?}");
    // Recognised after the phrase began and before the recording ended.
    assert!(
        d.at_sample > 8_000 && d.at_sample <= samples.len() as u64,
        "{d:?}"
    );
    println!(
        "detected {:?} at sample {} ({} ms), mean probability {:.3}",
        d.phrase,
        d.at_sample,
        d.at_sample / 16,
        d.probability
    );
}

/// The block size the audio arrives in does not change the answer.
#[test]
fn any_block_size_gives_the_same_detection() {
    let samples = wav("okay-nabu.wav");
    let whole = okay_nabu().process(&samples);
    for block in [1, 7, 160, 256, 1000] {
        let mut detector = okay_nabu();
        let pieces: Vec<_> = samples
            .chunks(block)
            .flat_map(|c| detector.process(c))
            .collect();
        assert_eq!(pieces, whole, "blocks of {block}");
    }
}

/// The finish line: no detection on silence, on noise or on music.
#[test]
fn silence_noise_and_music_are_not_detected() {
    let cases: [(&str, Vec<i16>); 5] = [
        ("ten seconds of silence", vec![0; 160_000]),
        ("ten seconds of quiet noise", noise(160_000, 300)),
        ("ten seconds of loud noise", noise(160_000, 20_000)),
        ("the music fixture", wav("music.wav")),
        // Music four times over, so the noise estimate and the gain have settled.
        ("the music fixture, looped", wav("music.wav").repeat(4)),
    ];
    for (what, samples) in cases {
        let detections = okay_nabu().process(&samples);
        assert!(detections.is_empty(), "{what}: {detections:?}");
        println!("no detection in {what} ({} samples)", samples.len());
    }
}

/// The phrase said twice is two detections, each reported once, the second after the first.
#[test]
fn the_phrase_said_twice_is_detected_twice() {
    let samples = wav("okay-nabu.wav");
    let twice = [samples.clone(), vec![0; 16_000], samples.clone()].concat();
    let detections = okay_nabu().process(&twice);
    assert_eq!(detections.len(), 2, "{detections:?}");
    assert!(
        detections[1].at_sample > samples.len() as u64 + 16_000,
        "{detections:?}"
    );
}

/// After a reset the detector behaves as new: the phrase is found again, at the same place.
#[test]
fn reset_starts_over() {
    let samples = wav("okay-nabu.wav");
    let mut detector = okay_nabu();
    let first = detector.process(&samples);
    detector.process(&noise(16_000, 5_000));
    detector.reset();
    assert_eq!(detector.process(&samples), first);
}

/// The frontend gives upstream's features, bit for bit, on both recordings.
#[test]
fn the_frontend_matches_the_reference_bit_for_bit() {
    for name in ["okay-nabu", "music"] {
        let samples = wav(&format!("{name}.wav"));
        let got = features(&samples);
        let want = numbers(&format!("{name}.features"));
        assert_eq!(got.len(), (samples.len() - WINDOW) / STEP + 1, "{name}");
        assert_eq!(got.len(), want.len(), "{name}: frame count");
        for (i, (g, w)) in got.iter().zip(&want).enumerate() {
            assert_eq!(g.as_slice(), w.as_slice(), "{name}: frame {i}");
        }
        println!("{name}: {} feature frames equal the reference", got.len());
    }
}

/// The interpreter gives the output of TensorFlow Lite's reference kernels, value for value,
/// on both recordings.
#[test]
fn the_interpreter_matches_the_reference_value_for_value() {
    for name in ["okay-nabu", "music"] {
        let mut detector = okay_nabu();
        let want: Vec<u8> = numbers(&format!("{name}.outputs"))
            .into_iter()
            .map(|l| l[0] as u8)
            .collect();
        let mut got = Vec::new();
        for frame in numbers(&format!("{name}.features")) {
            got.extend(detector.raw_frame(&<[u16; CHANNELS]>::try_from(frame).unwrap()));
        }
        assert_eq!(got, want, "{name}");
        println!(
            "{name}: {} model outputs equal the reference (largest {})",
            got.len(),
            got.iter().max().unwrap()
        );
    }
}

/// Every file in the fixture directory is one this test file reads or one of the two
/// scripts that made them: a stray file would be a fixture nothing checks.
#[test]
fn every_fixture_is_read() {
    let known = [
        "make-music.py",
        "make-reference.py",
        "music.features",
        "music.outputs",
        "music.wav",
        "okay-nabu.features",
        "okay-nabu.outputs",
        "okay-nabu.wav",
        "README.md",
    ];
    let mut seen = 0;
    for entry in std::fs::read_dir(fixtures()).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        assert!(
            known.contains(&name.as_str()),
            "fixtures/wakeword/{name} is read by no test"
        );
        seen += 1;
    }
    assert_eq!(seen, known.len(), "a fixture is missing");
}

/// A damaged model file is an error, never a panic: every truncation, and a byte changed
/// at every 61st position, loads or fails cleanly, and what loads runs.
#[test]
fn a_damaged_model_never_panics() {
    let good = builtin()[0];
    let manifest = Manifest::from_json(good.manifest).unwrap();
    let frame = [300u16; CHANNELS];
    for len in (0..good.model.len()).step_by(37) {
        assert!(
            Detector::new(&good.model[..len], manifest.clone()).is_err(),
            "truncated to {len} bytes"
        );
    }
    let mut loaded = 0;
    for at in (0..good.model.len()).step_by(61) {
        for flip in [0x01u8, 0x80, 0xff] {
            let mut bytes = good.model.to_vec();
            bytes[at] ^= flip;
            if let Ok(mut d) = Detector::new(&bytes, manifest.clone()) {
                loaded += 1;
                for _ in 0..6 {
                    d.raw_frame(&frame);
                }
            }
        }
    }
    println!("{loaded} damaged models still loaded, and ran without a panic");
    assert_eq!(
        Detector::new(b"not a model", manifest.clone()).err(),
        Some(Error::Malformed("not a TFLite file (no TFL3 identifier)"))
    );
}
