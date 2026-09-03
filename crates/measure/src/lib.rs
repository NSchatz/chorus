//! The measurement harness: two endpoint line outputs in, median, p95 and
//! maximum inter-device lag out, with a report saved where someone who did not
//! run it can read it.
//!
//! # Why this exists before the servo
//!
//! `CLAUDE.md` guardrail 3: a timing claim needs a measurement, and "sounds
//! synced" is not evidence. `documentation/roadmaps/chorus.md` puts the rig
//! ahead of the sync engine on purpose - "a harness written after the servo
//! tends to be written to agree with it". So nothing in this crate corrects,
//! disciplines or tunes anything, and nothing in it holds a view about whether
//! a measured number is good. It measures, it says how confident it is, and
//! where it cannot resolve the two outputs it refuses and writes no number.
//!
//! # The shape
//!
//! | module | what it does |
//! |---|---|
//! | [`wav`] | reads a two-channel PCM recording, refusing every malformed case by name |
//! | [`lag`] | cross-correlates sliding windows and reduces them to median, p95 and maximum |
//! | [`freerun`] | fits the relative offset series of two uncorrected clients and reports ppm |
//! | [`report`] | renders a run into `docs/measurements/` and records the baseline later runs cite |
//! | [`chirp`] | the sweep, and the amplitude ceiling that stands in front of a real amplifier |
//! | [`fixtures`] | regenerates the committed fixture inputs from their committed parameters |
//! | [`capture`] | the device-backed half: emit, record, and refuse by name where there is no device |
//! | [`config`] | every threshold this rig declares, read from `config/measure.conf` |
//! | [`rng`] | the seeded generator the fixtures reproduce from |
//!
//! # The analysis takes a FILE
//!
//! An already-captured recording is a first-class input, not a fallback. If
//! analysis could only run against a live device, then nobody without two
//! endpoints and an interface could check any claim this rig makes, and
//! guardrail 3's whole point - a claim someone else can check - would rest on
//! an uncheckable tool. The device-backed entry point is a producer of capture
//! files; everything downstream of it reads one.
//!
//! # Time
//!
//! Nothing here derives a figure from a clock read during analysis. Lags come
//! from sample indices and a declared sample rate; drift comes from timestamps
//! the series file carries, which `docs/protocol.md` already requires to be
//! nanoseconds from a monotonic source. Where elapsed time IS taken - how long
//! a recording ran, how long an analysis took - it comes from
//! `chorus_audio::MonotonicTimeline`. The one settable-clock read in this crate
//! is the human-readable date in a report header, which is a log field;
//! `audio-path.conf` records that exception and
//! `crates/measure/tests/no_settable_wall_clock.rs` asserts it is the only one.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod capture;
pub mod chirp;
pub mod config;
pub mod fixtures;
pub mod freerun;
pub mod lag;
pub mod report;
pub mod rng;
pub mod wav;

pub use chirp::{ChirpError, ChirpSpec};
pub use config::{ConfigError, MeasureConfig};
pub use freerun::{OffsetSeries, SlopeError, SlopeFit, SlopeSettings};
pub use lag::{LagError, LagSettings, LagSummary, SIGN_CONVENTION};
pub use report::{Baseline, BuildIdentity, ReportError};
pub use wav::{Capture, CaptureError};

use std::path::{Path, PathBuf};

/// The repository root, from the crate's own location in it.
///
/// The same idiom `chorus_audio_path::scan::repository_root` uses, for the same
/// reason: a fixture path has to resolve identically whichever directory
/// `cargo test` was invoked from.
pub fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("this crate lives at crates/<name> under the repository root")
        .to_path_buf()
}
