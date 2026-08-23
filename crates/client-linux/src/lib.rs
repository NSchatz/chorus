//! The minimal Linux client: receive, buffer, play out through ALSA, and log
//! the delay the device reports.
//!
//! # What this client is for
//!
//! Making a sound, and leaving behind enough of a record that someone who did
//! not run it can check the claim. Those are the same job: an audio client
//! that plays is easy, and one that can prove what it played is the thing a
//! sync engine can later be built on.
//!
//! # What it deliberately does not do
//!
//! Correct anything. No offset filtering, no rate correction, no resampling,
//! no hard resync, no servo. When the buffer drifts, this client reports the
//! drift and keeps playing at the rate it was given. Two endpoints agreeing is
//! the next phase's job, and a client that quietly corrected would make that
//! phase impossible to measure.
//!
//! # The modules
//!
//! - [`config`]: the bounds and the three relations between them.
//! - [`receive`]: the byte stream back into chunks, and the framing grammar
//!   that keeps mis-framed bytes off the device.
//! - [`buffer`]: occupancy, the counters, and what happens at each bound.
//! - [`sink`]: where frames go. One implementation, [`sink::AlsaSink`].
//! - [`run`]: the graded interval, the priming write, the playout loop, the
//!   drain.
//! - [`delaylog`]: the record a run leaves behind. Excluded from the audio
//!   path, and the only place a settable clock is read.
//! - [`logcheck`]: grading that record afterwards, from the file alone.

#![warn(missing_docs)]

pub mod buffer;
pub mod config;
pub mod delaylog;
pub mod logcheck;
pub mod receive;
pub mod run;
pub mod sink;

pub use buffer::{Buffer, Counters};
pub use config::{ClientConfig, ClientMode, ConfigError};
pub use run::{run_session, RunOutcome, StopReason};
pub use sink::{AlsaSink, PcmSink, SinkError, SinkWrite};
