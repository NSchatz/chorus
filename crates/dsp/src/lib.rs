//! The chorus DSP library (goal 12): the per-endpoint sound chain and the
//! building blocks it is made of.
//!
//! A pure library: no socket, no thread, no file read and no clock. Every
//! block takes samples and parameters and returns samples, which is what lets
//! the C mirror (`firmware/include/chorus/dsp.h`, `firmware/src/dsp.c`) be held
//! to the same committed fixtures under `fixtures/dsp/`
//! (`tests/shared_fixtures.rs` here, `firmware/tests/test_dsp.c` there): the
//! two implementations run the same algorithms in the same state-update order,
//! so they cannot drift apart.
//!
//! # Numbers
//!
//! Samples are `f32`. Coefficients are designed in `f64` and rounded to `f32`
//! once (BRIEF section 5.6), so a design is the same on a host and on the
//! ESP32-S3 whose FPU is single precision. Rust never contracts `a * b + c`
//! into a fused multiply-add, and the C side is compiled with
//! `-ffp-contract=off -fno-fast-math` for the same reason.
//!
//! # What is here
//!
//! 1. [`biquad`]: the eight RBJ cookbook designs, Transposed Direct Form II
//!    processing, and the `f64` response evaluator ([`biquad::Coefficients::magnitude_db`]).
//! 2. [`crossover`]: the Linkwitz-Riley 4th-order (LR4) split.
//! 3. [`delay`]: whole-sample delay lines with a fixed maximum.
//! 4. [`limiter`]: the look-ahead limiter whose ceiling is never exceeded.
//! 5. [`compressor`]: the night-mode compressor (Giannoulis, Massberg and
//!    Reiss 2012), stereo-linked.
//! 6. [`loudness`]: ISO 226:2003 equal-loudness contours and the shelf gains
//!    that compensate a playback attenuation.
//! 7. [`speech`]: the voice-band boost on the centre, the mid or the mono
//!    channel.
//! 8. and 9. [`chain`]: bass management by role and the endpoint's two-way
//!    split, with [`settings`] holding what configures them.
//! 10. [`chain::Chain`]: all of the above in the order `docs/dsp.md` gives.
//!
//! And one analysis that runs on the server, not in the chain: [`roomfit`], room-correction
//! fitting from a sweep recording (`docs/room-correction.md`).
//!
//! And another the server runs: [`visualizer`], the levels, bands, beat and
//! colour of the visualizer stream (`docs/visualizer.md`).
//!
//! And one mixer the server runs before a room's stream is encoded: [`duck`], the
//! announcement's duck, mix and restore (`docs/dsp.md`, "The announcement mixer").
//!
//! And one reader no chain runs yet: [`design_record`], the versioned JSON export of a speaker
//! design from the owner's shared Python library, whose crossover biquads the blocks above run
//! (`docs/dsp.md`, "Design records").
//!
//! What each block does, every citation and every ASSUMED default:
//! `docs/dsp.md`. The decisions: `docs/decisions/` ("the DSP library").

#![forbid(unsafe_code)]

pub mod biquad;
pub mod chain;
pub mod compressor;
pub mod crossover;
pub mod delay;
pub mod design_record;
pub mod duck;
pub mod fixture;
pub mod limiter;
pub mod loudness;
pub mod roomfit;
pub mod settings;
pub mod speech;
pub mod visualizer;

pub use biquad::{Biquad, Coefficients, Kind};
pub use chain::Chain;
pub use settings::{Driver, EndpointDsp, RoomEqFilter, SoundSettings, TwoWay};

/// Why a design or a configuration was refused. Every refusal names the
/// parameter, so a caller (the catalog, the endpoint's configuration reader)
/// can say which value was wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DspError {
    /// The sample rate is outside 8000..=384000 Hz (the protocol's bounds,
    /// `docs/protocol.md` "0x12 stream format").
    Rate,
    /// A filter frequency is not inside (0, rate/2).
    Frequency,
    /// A Q is not finite and positive.
    Q,
    /// A gain is not finite.
    Gain,
    /// A delay is longer than its fixed maximum.
    DelayTooLong,
    /// The look-ahead is longer than [`limiter::MAX_LOOKAHEAD_FRAMES`].
    Lookahead,
    /// A time constant is not finite and positive.
    TimeConstant,
    /// A channel map is empty, longer than 8, or names a position twice.
    ChannelMap,
    /// A `SoundSettings` field is outside the wire `sound` message's bounds;
    /// the name is the field's.
    Setting(&'static str),
    /// An `EndpointDsp` field is out of range; the name is the field's.
    Endpoint(&'static str),
    /// A buffer handed to `process` does not hold whole frames, or the output
    /// is not the size the chain's output count asks for.
    Buffer,
}

impl core::fmt::Display for DspError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DspError::Rate => write!(f, "the sample rate is outside 8000..=384000 Hz"),
            DspError::Frequency => write!(f, "a filter frequency is not inside (0, rate/2)"),
            DspError::Q => write!(f, "a Q is not finite and positive"),
            DspError::Gain => write!(f, "a gain is not finite"),
            DspError::DelayTooLong => write!(f, "a delay is longer than its fixed maximum"),
            DspError::Lookahead => write!(f, "the look-ahead is longer than its fixed maximum"),
            DspError::TimeConstant => write!(f, "a time constant is not finite and positive"),
            DspError::ChannelMap => write!(f, "the channel map is empty, too long or repeats"),
            DspError::Setting(name) => write!(f, "sound setting {name} is out of range"),
            DspError::Endpoint(name) => write!(f, "endpoint DSP setting {name} is out of range"),
            DspError::Buffer => write!(f, "a buffer does not hold whole frames"),
        }
    }
}

impl std::error::Error for DspError {}

/// The lowest and highest sample rates the chain accepts, the protocol's
/// `stream_format` bounds.
pub const MIN_RATE_HZ: u32 = 8000;
/// See [`MIN_RATE_HZ`].
pub const MAX_RATE_HZ: u32 = 384_000;

/// Decibels to a linear amplitude factor, in `f64` (design time).
pub fn db_to_gain(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

/// A linear amplitude factor to decibels, in `f64` (design time).
pub fn gain_to_db(gain: f64) -> f64 {
    20.0 * gain.log10()
}
