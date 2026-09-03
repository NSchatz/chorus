//! Generating the committed fixture inputs from their committed parameters.
//!
//! # Why the inputs are committed as well as their parameters
//!
//! Guardrail 3 in `CLAUDE.md` says a timing claim needs a measurement someone
//! else can check. "Someone else" cannot check a number computed from a capture
//! they do not have, so every figure this rig publishes about a fixture is
//! published about a file in the tree. And a file in the tree with no recipe is
//! a magic number, so each one sits beside a `.params` file holding every value
//! it was made from. Regenerating from those parameters reproduces the input
//! byte for byte, which is what `crates/measure/tests/report_shape.rs` asserts.
//!
//! # The one caveat on "byte for byte", stated rather than discovered
//!
//! The waveforms go through `sin`, `cos` and `ln` in the platform's maths
//! library, which is not required to be correctly rounded and may differ by one
//! unit in the last place between implementations. Every sample is then rounded
//! to 16 bits, and one ulp of an f64 near full scale is about 2e-12 of a
//! quantisation step, so a differing last bit would have to land within 2e-12
//! of a rounding boundary to change a byte. If that ever happens the
//! regeneration test goes red and says so, which is the behaviour wanted: the
//! alternative is a fixture that drifts quietly.
//!
//! # Adding a fixture
//!
//! Add a `.params` file. `chorus-measure fixtures` reads every one in the
//! directory, so there is nothing to register, which is the same arrangement
//! `fixtures/sync/` already uses.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::chirp::{ChirpError, ChirpSpec};
use crate::rng::Rng;
use crate::wav::{self, WAVE_FORMAT_PCM};

/// The directory the fixtures live in, relative to the repository root.
///
/// At the repository root and not inside a crate, because
/// `docs/decisions/0002-repository-layout-and-ci.md` puts them there: a
/// second-language implementation has to read them without a Cargo project.
pub const FIXTURES_DIR: &str = "fixtures/measure";

/// Why a fixture could not be generated.
#[derive(Debug, Clone, PartialEq)]
pub enum FixtureError {
    /// A parameter file could not be read.
    Unreadable {
        /// The path that was tried.
        path: PathBuf,
        /// What the operating system said.
        detail: String,
    },
    /// A parameter line is not `key = value`.
    Malformed {
        /// The path that was read.
        path: PathBuf,
        /// One-based line number.
        line: usize,
        /// What was wrong with it.
        detail: String,
    },
    /// A parameter this generator needs is absent.
    Missing {
        /// The path that was read.
        path: PathBuf,
        /// The key that is not in it.
        key: String,
    },
    /// A parameter is present and unusable.
    Unusable {
        /// The path that was read.
        path: PathBuf,
        /// The key whose value is wrong.
        key: String,
        /// The value as written.
        value: String,
    },
    /// The `kind` names a generator that does not exist.
    UnknownKind {
        /// The path that was read.
        path: PathBuf,
        /// The kind as written.
        kind: String,
    },
    /// The chirp the parameters ask for is not one this rig will build.
    Chirp {
        /// The path that was read.
        path: PathBuf,
        /// Why.
        error: ChirpError,
    },
}

impl fmt::Display for FixtureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FixtureError::Unreadable { path, detail } => {
                write!(f, "fixture parameters '{}': {}", path.display(), detail)
            }
            FixtureError::Malformed { path, line, detail } => {
                write!(f, "{} line {}: {}", path.display(), line, detail)
            }
            FixtureError::Missing { path, key } => {
                write!(f, "{} declares no '{}'", path.display(), key)
            }
            FixtureError::Unusable { path, key, value } => {
                write!(f, "{} gives '{}' as '{}', which is unusable", path.display(), key, value)
            }
            FixtureError::UnknownKind { path, kind } => write!(
                f,
                "{} asks for kind '{}', which no generator in this crate produces",
                path.display(),
                kind
            ),
            FixtureError::Chirp { path, error } => {
                write!(f, "{}: {}", path.display(), error)
            }
        }
    }
}

impl std::error::Error for FixtureError {}

/// One fixture's committed parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct FixtureParams {
    /// Where they were read from.
    pub path: PathBuf,
    /// The generator that turns them into bytes.
    pub kind: String,
    /// The file they produce, relative to the parameter file's directory.
    pub output: String,
    values: BTreeMap<String, String>,
}

impl FixtureParams {
    /// Read one parameter file.
    pub fn read(path: &Path) -> Result<FixtureParams, FixtureError> {
        let text = std::fs::read_to_string(path).map_err(|e| FixtureError::Unreadable {
            path: path.to_path_buf(),
            detail: e.to_string(),
        })?;
        FixtureParams::parse(path, &text)
    }

    /// Parse parameter text already in hand.
    pub fn parse(path: &Path, text: &str) -> Result<FixtureParams, FixtureError> {
        let mut values = BTreeMap::new();
        for (index, raw) in text.lines().enumerate() {
            let line = match raw.find('#') {
                Some(at) => &raw[..at],
                None => raw,
            }
            .trim();
            if line.is_empty() {
                continue;
            }
            let (key, value) = line.split_once('=').ok_or_else(|| FixtureError::Malformed {
                path: path.to_path_buf(),
                line: index + 1,
                detail: format!("'{}' is not 'key = value'", line),
            })?;
            values.insert(key.trim().to_string(), value.trim().to_string());
        }
        let kind = values
            .get("kind")
            .cloned()
            .ok_or_else(|| FixtureError::Missing {
                path: path.to_path_buf(),
                key: "kind".to_string(),
            })?;
        let output = values
            .get("output")
            .cloned()
            .ok_or_else(|| FixtureError::Missing {
                path: path.to_path_buf(),
                key: "output".to_string(),
            })?;
        Ok(FixtureParams {
            path: path.to_path_buf(),
            kind,
            output,
            values,
        })
    }

    /// The file this fixture produces.
    pub fn output_path(&self) -> PathBuf {
        match self.path.parent() {
            Some(dir) => dir.join(&self.output),
            None => PathBuf::from(&self.output),
        }
    }

    fn text(&self, key: &str) -> Result<&str, FixtureError> {
        self.values
            .get(key)
            .map(String::as_str)
            .ok_or_else(|| FixtureError::Missing {
                path: self.path.clone(),
                key: key.to_string(),
            })
    }

    fn number(&self, key: &str) -> Result<f64, FixtureError> {
        let raw = self.text(key)?;
        raw.parse::<f64>().map_err(|_| FixtureError::Unusable {
            path: self.path.clone(),
            key: key.to_string(),
            value: raw.to_string(),
        })
    }

    fn integer(&self, key: &str) -> Result<i64, FixtureError> {
        let raw = self.text(key)?;
        raw.parse::<i64>().map_err(|_| FixtureError::Unusable {
            path: self.path.clone(),
            key: key.to_string(),
            value: raw.to_string(),
        })
    }

    fn flag(&self, key: &str) -> Result<bool, FixtureError> {
        match self.values.get(key).map(String::as_str) {
            None | Some("false") | Some("0") => Ok(false),
            Some("true") | Some("1") => Ok(true),
            Some(other) => Err(FixtureError::Unusable {
                path: self.path.clone(),
                key: key.to_string(),
                value: other.to_string(),
            }),
        }
    }

    fn chirp(&self, amplitude_key: &str) -> Result<ChirpSpec, FixtureError> {
        // The fixtures are files and drive no amplifier, so the ceiling here is
        // full scale: the ceiling that protects a loudspeaker belongs to the
        // device-backed entry point, which is the only thing that emits.
        ChirpSpec::new(
            self.number("chirp_start_hz")?,
            self.number("chirp_end_hz")?,
            self.number("chirp_period_us")?,
            self.number(amplitude_key)?,
            1.0,
            "fixture parameters",
        )
        .map_err(|error| FixtureError::Chirp {
            path: self.path.clone(),
            error,
        })
    }
}

/// Turn one fixture's parameters into the bytes it commits.
pub fn generate(params: &FixtureParams) -> Result<Vec<u8>, FixtureError> {
    match params.kind.as_str() {
        "chirp-pair" => chirp_pair(params),
        "silence" => silence(params),
        "noise" => noise(params),
        "reversed-pair" => reversed_pair(params),
        "mono-chirp" => mono_chirp(params),
        "float-chirp" => float_chirp(params),
        "truncated-chirp-pair" => truncated_chirp_pair(params),
        "free-run" => free_run(params),
        other => Err(FixtureError::UnknownKind {
            path: params.path.clone(),
            kind: other.to_string(),
        }),
    }
}

/// Every fixture in a directory, in a stable order.
pub fn read_all(dir: &Path) -> Result<Vec<FixtureParams>, FixtureError> {
    let entries = std::fs::read_dir(dir).map_err(|e| FixtureError::Unreadable {
        path: dir.to_path_buf(),
        detail: e.to_string(),
    })?;
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "params").unwrap_or(false))
        .collect();
    paths.sort();
    paths.iter().map(|p| FixtureParams::read(p)).collect()
}

fn frames_of(params: &FixtureParams) -> Result<(u32, usize), FixtureError> {
    let rate = params.number("sample_rate_hz")? as u32;
    let frames = (params.number("duration_us")? * f64::from(rate) / 1_000_000.0).round() as usize;
    Ok((rate, frames))
}

/// Two channels carrying the same chirp, the second delayed by an amount that
/// need not be a whole number of samples.
fn chirp_pair(params: &FixtureParams) -> Result<Vec<u8>, FixtureError> {
    let (rate, frames) = frames_of(params)?;
    let chirp = params.chirp("amplitude")?;
    let delay_s = params.number("delay_us")? / 1_000_000.0;
    let noise_amplitude = params.number("noise_amplitude")?;
    let exchange = params.flag("exchange_channels")?;
    let mut rng = Rng::new(params.integer("seed")? as u64);

    let mut interleaved = Vec::with_capacity(frames * 2);
    for n in 0..frames {
        let t = n as f64 / f64::from(rate);
        // Independent noise per channel, drawn in a fixed order so the file is
        // the same every time.
        let noise_a = rng.next_symmetric() * noise_amplitude;
        let noise_b = rng.next_symmetric() * noise_amplitude;
        let a = chirp.at(t) + noise_a;
        let b = chirp.at(t - delay_s) + noise_b;
        let (first, second) = if exchange { (b, a) } else { (a, b) };
        interleaved.push(wav::to_i16(first));
        interleaved.push(wav::to_i16(second));
    }
    Ok(wav::write_wav(&interleaved, 2, rate, 16, WAVE_FORMAT_PCM))
}

/// Two silent channels.
fn silence(params: &FixtureParams) -> Result<Vec<u8>, FixtureError> {
    let (rate, frames) = frames_of(params)?;
    Ok(wav::write_wav(&vec![0i16; frames * 2], 2, rate, 16, WAVE_FORMAT_PCM))
}

/// Two channels of independent full-band noise: energy everywhere, and no
/// chirp anywhere.
fn noise(params: &FixtureParams) -> Result<Vec<u8>, FixtureError> {
    let (rate, frames) = frames_of(params)?;
    let amplitude = params.number("amplitude")?;
    let mut rng = Rng::new(params.integer("seed")? as u64);
    let mut interleaved = Vec::with_capacity(frames * 2);
    for _ in 0..frames {
        interleaved.push(wav::to_i16(rng.next_symmetric() * amplitude));
        interleaved.push(wav::to_i16(rng.next_symmetric() * amplitude));
    }
    Ok(wav::write_wav(&interleaved, 2, rate, 16, WAVE_FORMAT_PCM))
}

/// The chirp on one channel and the same chirp reversed in time on the other.
///
/// This is the fixture that separates "no chirp is present" from "the
/// correlation peak never cleared the confidence floor". Reversing a real
/// signal in time leaves its spectrum's magnitude untouched, so both channels
/// carry a chirp's worth of energy in the declared band and the presence check
/// passes; but an up-sweep correlates against a down-sweep at roughly one over
/// the square root of the time-bandwidth product, which for this sweep is under
/// a tenth, so the confidence floor is what refuses the run.
fn reversed_pair(params: &FixtureParams) -> Result<Vec<u8>, FixtureError> {
    let (rate, frames) = frames_of(params)?;
    let chirp = params.chirp("amplitude")?;
    let forward: Vec<f64> = (0..frames)
        .map(|n| chirp.at(n as f64 / f64::from(rate)))
        .collect();
    let mut interleaved = Vec::with_capacity(frames * 2);
    for n in 0..frames {
        interleaved.push(wav::to_i16(forward[n]));
        interleaved.push(wav::to_i16(forward[frames - 1 - n]));
    }
    Ok(wav::write_wav(&interleaved, 2, rate, 16, WAVE_FORMAT_PCM))
}

/// One channel where two are required.
fn mono_chirp(params: &FixtureParams) -> Result<Vec<u8>, FixtureError> {
    let (rate, frames) = frames_of(params)?;
    let chirp = params.chirp("amplitude")?;
    let samples: Vec<i16> = (0..frames)
        .map(|n| wav::to_i16(chirp.at(n as f64 / f64::from(rate))))
        .collect();
    Ok(wav::write_wav(&samples, 1, rate, 16, WAVE_FORMAT_PCM))
}

/// A layout this reader does not accept, in a container that is otherwise
/// perfectly well formed.
fn float_chirp(params: &FixtureParams) -> Result<Vec<u8>, FixtureError> {
    let (rate, frames) = frames_of(params)?;
    let chirp = params.chirp("amplitude")?;
    let delay_s = params.number("delay_us")? / 1_000_000.0;
    let mut interleaved = Vec::with_capacity(frames * 2);
    for n in 0..frames {
        let t = n as f64 / f64::from(rate);
        interleaved.push(chirp.at(t) as f32);
        interleaved.push(chirp.at(t - delay_s) as f32);
    }
    Ok(wav::write_wav_f32(&interleaved, 2, rate))
}

/// A capture whose data chunk header promises more than the file holds.
fn truncated_chirp_pair(params: &FixtureParams) -> Result<Vec<u8>, FixtureError> {
    let mut bytes = chirp_pair(params)?;
    let keep = params.integer("keep_bytes")? as usize;
    if keep >= bytes.len() {
        return Err(FixtureError::Unusable {
            path: params.path.clone(),
            key: "keep_bytes".to_string(),
            value: format!("{}, which is not shorter than the {} bytes generated", keep, bytes.len()),
        });
    }
    bytes.truncate(keep);
    Ok(bytes)
}

/// A series of relative offset observations at a known constant rate, with
/// optional Gaussian jitter at a stated level.
fn free_run(params: &FixtureParams) -> Result<Vec<u8>, FixtureError> {
    let points = params.integer("points")? as usize;
    let interval_s = params.number("interval_s")?;
    let ppm = params.number("rate_ppm")?;
    let jitter_us = params.number("jitter_us")?;
    let initial_offset_ns = params.integer("initial_offset_ns")?;
    let mut rng = Rng::new(params.integer("seed")? as u64);

    let mut out = String::new();
    out.push_str(&format!(
        "# Relative offset observations between two clients running with correction\n\
         # disabled, generated from {}. Every timestamp is nanoseconds from a monotonic\n\
         # source, which is what docs/protocol.md requires of every timestamp this project\n\
         # moves; a settable clock here would put a step in a straight line and be read as\n\
         # drift.\n\
         #\n\
         # Format: key = value, then [observations], then one 't_ns offset_ns' pair per line.\n\n",
        params
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    ));
    out.push_str(&format!("label = {}\n", params.text("label")?));
    out.push_str(&format!("declared_rate_ppm = {}\n", ppm));
    out.push_str(&format!("declared_jitter_us = {}\n", jitter_us));
    out.push_str("\n[observations]\n");
    for n in 0..points {
        let t_ns = (n as f64 * interval_s * 1e9).round() as i64;
        let ideal = t_ns as f64 * ppm / 1e6;
        let jitter = if jitter_us > 0.0 {
            rng.next_normal() * jitter_us * 1000.0
        } else {
            0.0
        };
        let offset_ns = initial_offset_ns + (ideal + jitter).round() as i64;
        out.push_str(&format!("{} {}\n", t_ns, offset_ns));
    }
    Ok(out.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHIRP_PAIR: &str = "\
kind = chirp-pair
output = example.wav
sample_rate_hz = 96000
duration_us = 10000
chirp_start_hz = 1000
chirp_end_hz = 8000
chirp_period_us = 20000
amplitude = 0.25
delay_us = 254.0
noise_amplitude = 0.0
seed = 1
";

    #[test]
    fn a_fixture_generates_the_same_bytes_every_time() {
        let params = FixtureParams::parse(Path::new("mem.params"), CHIRP_PAIR).unwrap();
        let once = generate(&params).unwrap();
        let twice = generate(&params).unwrap();
        assert_eq!(once, twice);
        assert_eq!(once.len(), 44 + 960 * 4);
    }

    #[test]
    fn an_unknown_kind_is_refused_rather_than_producing_an_empty_file() {
        let params =
            FixtureParams::parse(Path::new("mem.params"), "kind = wishful\noutput = x\n").unwrap();
        let err = generate(&params).unwrap_err();
        assert!(err.to_string().contains("wishful"), "{}", err);
    }

    #[test]
    fn a_parameter_file_with_no_kind_is_refused() {
        let err = FixtureParams::parse(Path::new("mem.params"), "output = x\n").unwrap_err();
        assert!(err.to_string().contains("'kind'"), "{}", err);
    }

    #[test]
    fn exchanging_the_channels_exchanges_the_samples_and_nothing_else() {
        let plain = FixtureParams::parse(Path::new("mem.params"), CHIRP_PAIR).unwrap();
        let swapped_text = format!("{}exchange_channels = true\n", CHIRP_PAIR);
        let swapped = FixtureParams::parse(Path::new("mem.params"), &swapped_text).unwrap();
        let a = generate(&plain).unwrap();
        let b = generate(&swapped).unwrap();
        assert_eq!(a.len(), b.len());
        for frame in 0..960 {
            let at = 44 + frame * 4;
            assert_eq!(a[at..at + 2], b[at + 2..at + 4]);
            assert_eq!(a[at + 2..at + 4], b[at..at + 2]);
        }
    }

    #[test]
    fn a_truncation_that_would_not_truncate_is_refused() {
        let text = CHIRP_PAIR.replace("kind = chirp-pair", "kind = truncated-chirp-pair")
            + "keep_bytes = 99999999\n";
        let params = FixtureParams::parse(Path::new("mem.params"), &text).unwrap();
        let err = generate(&params).unwrap_err();
        assert!(err.to_string().contains("keep_bytes"), "{}", err);
    }
}
