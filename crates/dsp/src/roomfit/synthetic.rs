//! Synthetic rooms: the recordings `fixtures/roomfit/` holds, made from the parameters in their
//! `.params` files, so a test can regenerate each one byte for byte and a reviewer reads the
//! parameters rather than the PCM (the `fixtures/measure` precedent, `fixtures/README.md`).
//!
//! A room here is a chain, applied to the sweep after a silent lead-in (the playback-to-capture
//! latency): a speaker's low-frequency roll-off (a second-order high-pass), the room's modes
//! (peaking boosts: near its resonance a mode is a second-order resonance, which is what a
//! peaking biquad is), its nulls (deep, narrow peaking cuts: a cancellation at one seat), a
//! level offset (the playback volume and the microphone's sensitivity), then the background
//! noise (seeded, roughly Gaussian) and the converter (16-bit, rounded and clamped, which is
//! how a clipped fixture clips). The chain runs in f64 Direct Form I, not the endpoints' f32
//! processing: the room is not the thing under test.
//!
//! The module parses the `.params` text and builds and parses the WAV bytes, and does no I/O:
//! the test and `make roomfit-fixtures` (`crates/dsp/examples/roomfit_fixtures.rs`) read and
//! write the files.

use super::Sweep;
use crate::biquad;

/// A `.params` file: `key = value` lines, `#` comments, arrays space-separated.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Params {
    pairs: Vec<(String, String)>,
}

impl Params {
    /// Parses the text. A line that is not blank, not a comment and has no `=` is an error.
    pub fn parse(text: &str) -> Result<Params, String> {
        let mut pairs = Vec::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (k, v) = line
                .split_once('=')
                .ok_or_else(|| format!("line {}: no '=' in {line:?}", n + 1))?;
            pairs.push((k.trim().to_string(), v.trim().to_string()));
        }
        Ok(Params { pairs })
    }

    /// The value of `key`, if present.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.pairs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// The value of `key`, required.
    pub fn text(&self, key: &str) -> Result<&str, String> {
        self.get(key).ok_or_else(|| format!("missing key {key}"))
    }

    /// A number, required.
    pub fn num(&self, key: &str) -> Result<f64, String> {
        let v = self.text(key)?;
        v.parse()
            .map_err(|_| format!("{key} = {v:?} is not a number"))
    }

    /// A whole number, required.
    pub fn int(&self, key: &str) -> Result<u64, String> {
        let v = self.text(key)?;
        v.parse()
            .map_err(|_| format!("{key} = {v:?} is not a whole number"))
    }

    /// A space-separated list of numbers; absent or empty is an empty list.
    pub fn nums(&self, key: &str) -> Result<Vec<f64>, String> {
        self.get(key)
            .unwrap_or("")
            .split_whitespace()
            .map(|t| {
                t.parse()
                    .map_err(|_| format!("{key}: {t:?} is not a number"))
            })
            .collect()
    }
}

/// One resonance or null: a peaking filter's centre, gain and Q.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Peak {
    pub freq_hz: f64,
    pub gain_db: f64,
    pub q: f64,
}

/// Everything a synthetic recording is made from.
#[derive(Debug, Clone, PartialEq)]
pub struct Room {
    pub sweep: Sweep,
    /// Silence before the sweep starts, samples (the latency).
    pub lead_in_samples: usize,
    /// The recording's whole length, samples.
    pub total_samples: usize,
    /// The speaker's high-pass corner and Q.
    pub highpass_hz: f64,
    pub highpass_q: f64,
    /// The room's modes (positive gains).
    pub modes: Vec<Peak>,
    /// The seat's nulls (negative gains).
    pub nulls: Vec<Peak>,
    /// The level offset, dB.
    pub level_db: f64,
    /// The background noise's RMS, dBFS (full scale = 1).
    pub noise_rms_dbfs: f64,
    /// The noise generator's seed.
    pub seed: u64,
}

fn peaks(p: &Params, prefix: &str) -> Result<Vec<Peak>, String> {
    let f = p.nums(&format!("{prefix}_freq_hz"))?;
    let g = p.nums(&format!("{prefix}_gain_db"))?;
    let q = p.nums(&format!("{prefix}_q"))?;
    if f.len() != g.len() || f.len() != q.len() {
        return Err(format!(
            "{prefix}_freq_hz, _gain_db and _q differ in length"
        ));
    }
    Ok((0..f.len())
        .map(|i| Peak {
            freq_hz: f[i],
            gain_db: g[i],
            q: q[i],
        })
        .collect())
}

impl Room {
    /// The room a `.params` file describes.
    pub fn from_params(p: &Params) -> Result<Room, String> {
        if p.text("kind")? != "synthetic-room" {
            return Err(format!("kind {} is not synthetic-room", p.text("kind")?));
        }
        let rate = p.int("rate_hz")?;
        Ok(Room {
            sweep: Sweep {
                rate_hz: u32::try_from(rate).map_err(|_| "rate_hz is too large".to_string())?,
                f1_hz: p.num("sweep_f1_hz")?,
                f2_hz: p.num("sweep_f2_hz")?,
                samples: p.int("sweep_samples")? as usize,
                amplitude: p.num("sweep_amplitude")?,
                fade_in_samples: p.int("sweep_fade_in_samples")? as usize,
            },
            lead_in_samples: p.int("lead_in_samples")? as usize,
            total_samples: p.int("total_samples")? as usize,
            highpass_hz: p.num("highpass_hz")?,
            highpass_q: p.num("highpass_q")?,
            modes: peaks(p, "mode")?,
            nulls: peaks(p, "null")?,
            level_db: p.num("level_db")?,
            noise_rms_dbfs: p.num("noise_rms_dbfs")?,
            seed: p.int("seed")?,
        })
    }

    /// The room's chain as biquad coefficient sets, in order.
    fn chain(&self) -> Vec<biquad::Coeffs> {
        let rate = f64::from(self.sweep.rate_hz);
        let mut out = vec![biquad::highpass(rate, self.highpass_hz, self.highpass_q)];
        for p in self.modes.iter().chain(self.nulls.iter()) {
            out.push(biquad::peaking(rate, p.freq_hz, p.q, p.gain_db));
        }
        out
    }

    /// The room's true magnitude at `f` Hz, dB, without the level offset: the ground truth a
    /// fit is checked against.
    pub fn response_db(&self, f: f64) -> f64 {
        let rate = f64::from(self.sweep.rate_hz);
        self.chain().iter().map(|c| c.magnitude_db(rate, f)).sum()
    }

    /// The recording, as the 16-bit samples the WAV holds.
    pub fn render(&self) -> Vec<i16> {
        let mut x = vec![0.0f64; self.total_samples];
        for (i, v) in self.sweep.signal().into_iter().enumerate() {
            if let Some(slot) = x.get_mut(self.lead_in_samples + i) {
                *slot = v;
            }
        }
        for c in self.chain() {
            // Direct Form I in f64.
            let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
            for v in x.iter_mut() {
                let x0 = *v;
                let y0 = c.b0 * x0 + c.b1 * x1 + c.b2 * x2 - c.a1 * y1 - c.a2 * y2;
                x2 = x1;
                x1 = x0;
                y2 = y1;
                y1 = y0;
                *v = y0;
            }
        }
        let gain = 10f64.powf(self.level_db / 20.0);
        // The sum of four uniforms on [-1, 1) has variance 4/3 and is close enough to Gaussian
        // for background noise, without a logarithm whose last bit could differ by platform.
        let noise_scale = 10f64.powf(self.noise_rms_dbfs / 20.0) / (4.0f64 / 3.0).sqrt();
        let mut rng = SplitMix64(self.seed);
        x.iter()
            .map(|v| {
                let n: f64 = (0..4).map(|_| rng.symmetric()).sum();
                let s = v * gain + n * noise_scale;
                (s * 32767.0).round().clamp(-32768.0, 32767.0) as i16
            })
            .collect()
    }
}

/// SplitMix64 (Steele, Lea and Flood, "Fast splittable pseudorandom number generators",
/// OOPSLA 2014, pp. 453-472, https://doi.org/10.1145/2660193.2660195, the record at
/// https://dblp.org/rec/conf/oopsla/SteeleLF14.html read 2026-10-01): the same few lines
/// `crates/measure/src/rng.rs` and `crates/sync` carry, owned here for the same reason (the
/// generator is part of the fixture, and this crate has no dependencies).
struct SplitMix64(u64);

impl SplitMix64 {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A double in [-1, 1) from the top 53 bits.
    fn symmetric(&mut self) -> f64 {
        const SCALE: f64 = 1.0 / (1u64 << 53) as f64;
        (self.next_u64() >> 11) as f64 * SCALE * 2.0 - 1.0
    }
}

/// A mono 16-bit PCM WAV file's bytes (RIFF, a 16-byte `fmt ` chunk, one `data` chunk).
pub fn wav_bytes(rate_hz: u32, samples: &[i16]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate_hz.to_le_bytes());
    out.extend_from_slice(&(rate_hz * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// The rate and samples (full scale = 1) of a WAV in exactly the shape [`wav_bytes`] writes.
pub fn parse_wav(bytes: &[u8]) -> Result<(u32, Vec<f32>), String> {
    let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
    let u32_at =
        |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..16] != b"WAVEfmt " {
        return Err("not a RIFF WAVE file with fmt first".to_string());
    }
    if u16_at(20) != 1 || u16_at(22) != 1 || u16_at(34) != 16 || &bytes[36..40] != b"data" {
        return Err("not mono 16-bit PCM with data after fmt".to_string());
    }
    let len = u32_at(40) as usize;
    if bytes.len() != 44 + len {
        return Err(format!(
            "data is {len} bytes, the file has {}",
            bytes.len() - 44
        ));
    }
    let samples = bytes[44..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| f32::from(i16::from_le_bytes(*c)) / 32768.0)
        .collect();
    Ok((u32_at(24), samples))
}
