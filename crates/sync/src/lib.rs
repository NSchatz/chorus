//! chorus clock synchronisation.
//!
//! A pure library: no clock is read, no socket is opened, no thread is
//! spawned. It models two crystals, a network that delays and jitters, the
//! RFC 5905 section 8 exchange that estimates the offset between them, and the
//! servo that disciplines a modelled playout pointer to the server timeline.
//!
//! The point of it is BRIEF.md 5.3's strong recommendation: build a
//! deterministic simulator for the sync engine before touching hardware, so
//! servo logic can be developed and regression-tested in CI. What is modelled,
//! what is not, and why, is in `docs/decisions/0006-sync-simulator-and-servo.md`.
//!
//! ```
//! use chorus_sync::{run, SimConfig};
//!
//! let config = SimConfig::default();
//! let result = run(&config).expect("the default configuration is modellable");
//!
//! // The run starts on the client's own clock and has to find the server's.
//! assert!(result.samples[0].error_ns.abs() > 1_000_000);
//! // One millisecond, held for the rest of the run.
//! assert!(result.holds_below(1_000_000, 10_000_000_000));
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod config;
pub mod jitter;
pub mod rng;
pub mod scenario;
pub mod servo;
pub mod sim;

pub use config::{
    ConfigError, SimConfig, MAX_DELAY_US, MAX_SKEW_PPM, MAX_STEPS, SERVER_TURNAROUND_NS,
};
pub use jitter::JitterModel;
pub use rng::Rng;
pub use scenario::{Scenario, ScenarioError};
pub use servo::{OffsetFilter, Sample, Servo, ServoAction, ServoConfig};
pub use sim::{run, PlayoutSample, SimResult};
