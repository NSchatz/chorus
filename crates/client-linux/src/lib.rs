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
//! # What it corrects, and what it will not
//!
//! It runs the RFC 5905 exchange against the server on the connection already
//! carrying audio, filters a window of those exchanges by minimum round trip,
//! forms its error from the delay the audio DEVICE reports to its DAC, and
//! disciplines its playout with `chorus_sync`'s servo: a fine correction that
//! inserts or drops frames, and a hard resync that mutes, steps and resumes.
//! It publishes the offset in use and half the round trip of the exchange that
//! offset came from, as the bound on it.
//!
//! It will not infer audibility from the return of a write call, and it will
//! not carry on with a guess when the device refuses to report its delay. Both
//! are the same refusal: a loop disciplined against the wrong signal converges,
//! and converges onto the wrong target.
//!
//! # The modules
//!
//! - [`config`]: the bounds and the three relations between them.
//! - [`receive`]: the byte stream back into chunks, and the framing grammar
//!   that keeps mis-framed bytes off the device.
//! - [`buffer`]: occupancy, the counters, and what happens at each bound.
//! - [`sink`]: where frames go. One implementation, [`sink::AlsaSink`].
//! - [`sync`]: the exchange, the filter, the error signal, and turning a
//!   correction into frames.
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
pub mod sync;

pub use buffer::{Buffer, Counters};
pub use config::{ClientConfig, ClientMode, ConfigError};
pub use run::{run_session, RunOutcome, StopReason};
pub use sink::{AlsaSink, PcmSink, SinkError, SinkWrite};
pub use sync::{Correction, PlayoutCorrector, SyncConfig, SyncLoop, Telemetry};
