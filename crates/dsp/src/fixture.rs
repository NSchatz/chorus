//! The `fixtures/dsp/` text format, the signal recipes its files name, and
//! the chain runner its `chain` fixtures use. Test support, public so the
//! shared-fixture test and the `chain_samples` example (which prints a chain
//! fixture's golden samples) read one implementation; the C mirror's reader is
//! `firmware/tests/test_dsp.c`. Parsing text is not I/O: the caller reads the
//! file.
//!
//! The format (`fixtures/README.md`, `dsp/`): one `key = value` per line, `#`
//! starts a comment, a list is space-separated. A signal recipe is one or more
//! terms joined by `+`, summed in `f64` and rounded to `f32` once per sample:
//!
//! - `sine <hz> <amp>`: `amp sin(2 pi hz n / rate)`;
//! - `burst <hz> <amp> <from> <to>`: the sine, only for `from <= n < to`;
//! - `impulse <amp>`: `amp` at `n = 0`;
//! - `step <amp>`: `amp` from `n = 0`;
//! - `ramp <step>`: `step (n + 1)`;
//! - `noise <seed> <amp>`: a 64-bit LCG (Knuth's MMIX constants)
//!   `s = s * 6364136223846793005 + 1442695040888963407`, then
//!   `amp ((s >> 40) / 2^24 * 2 - 1)`;
//! - `silence`.

use crate::settings::{Driver, EndpointDsp, RoomEqFilter, SoundSettings, TwoWay};
use crate::Chain;

/// A parsed fixture: its `key = value` pairs in file order.
#[derive(Clone, Debug, Default)]
pub struct Fields {
    pairs: Vec<(String, String)>,
}

impl Fields {
    /// Parses a fixture's text.
    pub fn parse(text: &str) -> Result<Fields, String> {
        let mut pairs = Vec::new();
        for (i, line) in text.lines().enumerate() {
            let line = match line.find('#') {
                Some(h) => &line[..h],
                None => line,
            }
            .trim();
            if line.is_empty() {
                continue;
            }
            let (k, v) = line
                .split_once('=')
                .ok_or_else(|| format!("line {}: no '='", i + 1))?;
            pairs.push((k.trim().to_string(), v.trim().to_string()));
        }
        Ok(Fields { pairs })
    }

    /// The value of `key`, if present.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.pairs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Every key, in file order.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.pairs.iter().map(|(k, _)| k.as_str())
    }

    /// The value of `key`, or an error naming it.
    pub fn str(&self, key: &str) -> Result<&str, String> {
        self.get(key).ok_or_else(|| format!("no '{key}'"))
    }

    /// `key` as one number.
    pub fn num(&self, key: &str) -> Result<f64, String> {
        let v = self.str(key)?;
        v.parse::<f64>()
            .map_err(|_| format!("'{key}' is not a number: {v}"))
    }

    /// `key` as one number, or `default` when absent.
    pub fn num_or(&self, key: &str, default: f64) -> Result<f64, String> {
        if self.get(key).is_some() {
            self.num(key)
        } else {
            Ok(default)
        }
    }

    /// `key` as a space-separated list of numbers.
    pub fn list(&self, key: &str) -> Result<Vec<f64>, String> {
        self.str(key)?
            .split_whitespace()
            .map(|t| {
                t.parse::<f64>()
                    .map_err(|_| format!("'{key}' holds a non-number: {t}"))
            })
            .collect()
    }
}

fn lcg(state: &mut u64) -> f64 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    ((*state >> 40) as f64 / 16_777_216.0) * 2.0 - 1.0
}

/// Renders a signal recipe (see the module) to `frames` samples at `rate_hz`.
pub fn signal(recipe: &str, frames: usize, rate_hz: f64) -> Result<Vec<f32>, String> {
    let mut acc = vec![0f64; frames];
    for term in recipe.split('+') {
        let words: Vec<&str> = term.split_whitespace().collect();
        let nums: Vec<f64> = words
            .iter()
            .skip(1)
            .map(|w| w.parse::<f64>().map_err(|_| format!("recipe '{recipe}'")))
            .collect::<Result<_, _>>()?;
        let want = |k: usize| -> Result<(), String> {
            if nums.len() == k {
                Ok(())
            } else {
                Err(format!("recipe term '{}' takes {k} numbers", term.trim()))
            }
        };
        let tau = 2.0 * core::f64::consts::PI;
        match words.first().copied() {
            Some("sine") => {
                want(2)?;
                for (n, a) in acc.iter_mut().enumerate() {
                    *a += nums[1] * (tau * nums[0] * n as f64 / rate_hz).sin();
                }
            }
            Some("burst") => {
                want(4)?;
                for (n, a) in acc.iter_mut().enumerate() {
                    if (n as f64) >= nums[2] && (n as f64) < nums[3] {
                        *a += nums[1] * (tau * nums[0] * n as f64 / rate_hz).sin();
                    }
                }
            }
            Some("impulse") => {
                want(1)?;
                if let Some(a) = acc.first_mut() {
                    *a += nums[0];
                }
            }
            Some("step") => {
                want(1)?;
                acc.iter_mut().for_each(|a| *a += nums[0]);
            }
            Some("ramp") => {
                want(1)?;
                for (n, a) in acc.iter_mut().enumerate() {
                    *a += nums[0] * (n as f64 + 1.0);
                }
            }
            Some("noise") => {
                want(2)?;
                let mut s = nums[0] as u64;
                for a in acc.iter_mut() {
                    *a += nums[1] * lcg(&mut s);
                }
            }
            Some("silence") => want(0)?,
            _ => return Err(format!("unknown recipe term '{}'", term.trim())),
        }
    }
    Ok(acc.into_iter().map(|a| a as f32).collect())
}

fn flag(f: &Fields, key: &str) -> Result<bool, String> {
    Ok(f.num_or(key, 0.0)? != 0.0)
}

fn driver(f: &Fields, key: &str) -> Result<Driver, String> {
    let v = f.list(key)?;
    if v.len() != 3 {
        return Err(format!("'{key}' is trim_cdb delay_us inverted"));
    }
    Ok(Driver {
        trim_cdb: v[0] as i16,
        delay_us: v[1] as u32,
        inverted: v[2] != 0.0,
    })
}

/// A `chain` fixture's settings.
pub fn sound_settings(f: &Fields) -> Result<SoundSettings, String> {
    let mut room_eq = Vec::new();
    if f.get("room_eq").is_some() {
        let v = f.list("room_eq")?;
        if v.len() % 3 != 0 {
            return Err("'room_eq' is freq_hz gain_cdb q_milli triples".into());
        }
        for t in v.chunks(3) {
            room_eq.push(RoomEqFilter {
                freq_hz: t[0] as u16,
                gain_cdb: t[1] as i16,
                q_milli: t[2] as u16,
            });
        }
    }
    Ok(SoundSettings {
        bass_db: f.num_or("bass_db", 0.0)? as i8,
        treble_db: f.num_or("treble_db", 0.0)? as i8,
        loudness: flag(f, "loudness")?,
        night: flag(f, "night")?,
        speech: flag(f, "speech")?,
        room_eq_enabled: flag(f, "room_eq_enabled")?,
        sub_polarity_inverted: flag(f, "sub_polarity_inverted")?,
        role: f.num_or("role", 0.0)? as u8,
        sub_present: flag(f, "sub_present")?,
        crossover_hz: f.num_or("crossover_hz", 80.0)? as u16,
        sub_level_cdb: f.num_or("sub_level_cdb", 0.0)? as i16,
        room_eq,
    })
}

/// A `chain` fixture's endpoint configuration.
pub fn endpoint_dsp(f: &Fields) -> Result<EndpointDsp, String> {
    let mut e = EndpointDsp::default();
    if flag(f, "two_way")? {
        e.two_way = Some(TwoWay {
            crossover_hz: f.num("two_way_hz")? as u32,
            woofer: driver(f, "woofer")?,
            tweeter: driver(f, "tweeter")?,
        });
    }
    if f.get("output_delay_us").is_some() {
        for (slot, v) in e.output_delay_us.iter_mut().zip(f.list("output_delay_us")?) {
            *slot = v as u32;
        }
    }
    Ok(e)
}

/// What a `chain` fixture's run produced.
pub struct ChainRun {
    pub chain: Chain,
    /// The inputs, one vector per stream channel.
    pub inputs: Vec<Vec<f32>>,
    /// The outputs, one vector per output.
    pub outputs: Vec<Vec<f32>>,
}

/// Builds a `chain` fixture's chain and runs its input through it, in blocks
/// of `block_frames` (the result does not depend on the block size; the
/// tests run more than one).
pub fn run_chain(f: &Fields, block_frames: usize) -> Result<ChainRun, String> {
    let rate = f.num("rate_hz")? as u32;
    let map: Vec<u8> = f.list("channel_map")?.iter().map(|&p| p as u8).collect();
    let frames = f.num("frames")? as usize;
    let mut chain = Chain::new(&sound_settings(f)?, &endpoint_dsp(f)?, &map, rate)
        .map_err(|e| format!("the chain was refused: {e}"))?;
    let mut inputs = Vec::new();
    for c in 0..map.len() {
        inputs.push(signal(f.str(&format!("input.{c}"))?, frames, rate as f64)?);
    }
    let room_gain = f.num("room_gain")? as f32;
    let limit_gain = f.num("limit_gain")? as f32;
    let (n, outs) = (map.len(), chain.out_channels());
    let mut interleaved = vec![0f32; frames * n];
    for (c, ch) in inputs.iter().enumerate() {
        for (i, &v) in ch.iter().enumerate() {
            interleaved[i * n + c] = v;
        }
    }
    let mut out = vec![0f32; frames * outs];
    let mut at = 0;
    while at < frames {
        let k = block_frames.min(frames - at);
        chain
            .process(
                &interleaved[at * n..(at + k) * n],
                &mut out[at * outs..(at + k) * outs],
                room_gain,
                limit_gain,
            )
            .map_err(|e| format!("process refused: {e}"))?;
        at += k;
    }
    let outputs = (0..outs)
        .map(|o| (0..frames).map(|i| out[i * outs + o]).collect())
        .collect();
    Ok(ChainRun {
        chain,
        inputs,
        outputs,
    })
}
