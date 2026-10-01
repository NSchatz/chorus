//! The chimes: golden digests, the level ceiling, silence at both ends, and
//! every rate, format and channel count the stream can use.

use chorus_schedule::chime::{Chime, CHIMES, PEAK};
use chorus_schedule::{encode, render, PcmFormat, RenderError};

/// FNV-1a 64, the hash `crates/client-linux/tests/codec_fixtures.rs` pins
/// decodes with.
fn fnv1a64(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in data {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// The 48 kHz mono s16le render of each chime, hashed. A change here is a
/// change to the sound, so it goes with a note in `docs/chimes.md`; never
/// update a digest to make this test pass without listening to the result.
const GOLDEN_48K_S16LE: [(&str, u64); 3] = [
    ("bell", 0x420b_94e4_a9ab_7bda),
    ("ding-dong", 0x4864_b161_d571_a9e2),
    ("triad", 0x6670_2c0f_0728_d239),
];

const RATES: [u32; 3] = [44_100, 48_000, 96_000];
const FORMATS: [PcmFormat; 3] = [PcmFormat::S16Le, PcmFormat::S24Le, PcmFormat::F32Le];

/// -6 dBFS as a fraction of full scale.
fn ceiling() -> f64 {
    10f64.powf(-6.0 / 20.0)
}

fn decode(bytes: &[u8], format: PcmFormat) -> Vec<f64> {
    bytes
        .chunks_exact(format.bytes())
        .map(|b| match format {
            PcmFormat::S16Le => f64::from(i16::from_le_bytes([b[0], b[1]])) / 32_768.0,
            PcmFormat::S24Le => {
                f64::from(i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) / 8_388_608.0
            }
            PcmFormat::F32Le => f64::from(f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
        })
        .collect()
}

#[test]
fn golden_digests() {
    let got: Vec<(&str, u64)> = GOLDEN_48K_S16LE
        .iter()
        .map(|&(name, _)| {
            let chime = Chime::from_name(name).unwrap();
            (
                name,
                fnv1a64(&render(chime, 48_000, 1, PcmFormat::S16Le).unwrap()),
            )
        })
        .collect();
    assert_eq!(got, GOLDEN_48K_S16LE, "the renders hash to {:x?}", got);
}

#[test]
fn every_rate_format_and_channel_count() {
    for chime in CHIMES {
        for rate in RATES {
            let signal = chime.mono(rate);
            assert_eq!(
                encode(&signal, 1, PcmFormat::S16Le),
                render(chime, rate, 1, PcmFormat::S16Le).unwrap()
            );
            for format in FORMATS {
                let mono = encode(&signal, 1, format);
                let frames = chime.frames(rate);
                assert_eq!(frames as u64, chime.duration_ms() * u64::from(rate) / 1000);
                assert_eq!(mono.len(), frames * format.bytes());
                let x = decode(&mono, format);
                let peak = x.iter().fold(0.0f64, |a, &v| a.max(v.abs()));
                let what = format!("{} {rate} {}", chime.name(), format.name());
                assert!(
                    peak <= ceiling(),
                    "{what}: peak {:.2} dBFS",
                    20.0 * peak.log10()
                );
                assert!(peak >= PEAK * 0.99, "{what}: peak {peak} is not normalised");
                // Starts from zero (the attack) and ends at digital silence.
                assert_eq!(x[0], 0.0, "{what}: first sample");
                assert!(
                    mono[mono.len() - format.bytes()..].iter().all(|&b| b == 0),
                    "{what}: last sample"
                );
                // No click at the start: the first millisecond stays small.
                let ms = rate as usize / 1000;
                assert!(x[..ms].iter().all(|v| v.abs() < 0.2), "{what}: attack");
                // Every channel carries the same signal (checked at one
                // rate: the interleave does not depend on it).
                let counts: &[u16] = if rate == 48_000 { &[2, 6, 8] } else { &[] };
                for &channels in counts {
                    let multi = encode(&signal, channels, format);
                    let n = format.bytes();
                    assert_eq!(multi.len(), mono.len() * usize::from(channels));
                    let same = multi
                        .chunks_exact(n * usize::from(channels))
                        .zip(mono.chunks_exact(n))
                        .all(|(frame, sample)| frame.chunks_exact(n).all(|ch| ch == sample));
                    assert!(same, "{what} x{channels}: a channel differs");
                }
            }
        }
    }
}

#[test]
fn renders_are_deterministic_and_formats_agree() {
    for chime in CHIMES {
        let a = render(chime, 96_000, 2, PcmFormat::F32Le).unwrap();
        assert_eq!(a, render(chime, 96_000, 2, PcmFormat::F32Le).unwrap());
        // The three formats are one signal quantised three ways.
        let f = decode(
            &render(chime, 48_000, 1, PcmFormat::F32Le).unwrap(),
            PcmFormat::F32Le,
        );
        let s16 = decode(
            &render(chime, 48_000, 1, PcmFormat::S16Le).unwrap(),
            PcmFormat::S16Le,
        );
        let s24 = decode(
            &render(chime, 48_000, 1, PcmFormat::S24Le).unwrap(),
            PcmFormat::S24Le,
        );
        for i in 0..f.len() {
            assert!(
                (f[i] - s16[i]).abs() <= 1.0 / 32_768.0,
                "{} s16 at {i}",
                chime.name()
            );
            assert!(
                (f[i] - s24[i]).abs() <= 1.0 / 8_388_608.0,
                "{} s24 at {i}",
                chime.name()
            );
        }
    }
}

#[test]
fn refusals() {
    assert_eq!(
        render(Chime::Bell, 7_999, 1, PcmFormat::S16Le),
        Err(RenderError::Rate(7_999))
    );
    assert_eq!(
        render(Chime::Bell, 48_000, 0, PcmFormat::S16Le),
        Err(RenderError::Channels(0))
    );
    assert_eq!(
        render(Chime::Bell, 48_000, 9, PcmFormat::S16Le),
        Err(RenderError::Channels(9))
    );
}
