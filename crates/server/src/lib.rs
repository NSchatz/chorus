//! The chorus server: PCM in, timestamped chunks out, under a stated host
//! contract.
//!
//! # The two halves
//!
//! **The audio half** ([`serve`], and `chorus_audio` under it) takes PCM,
//! validates its format, cuts it into fixed-duration chunks on one monotonic
//! timeline, and puts them on a connection. Nothing in it reads a settable
//! clock and nothing in it corrects anything.
//!
//! **The group half** ([`stream`]) is what makes that one stream rather than
//! one stream per client: the chunks are produced once and fanned out, so two
//! clients see the same presentation timestamp for the same content, and each
//! client's connection also carries the time-sync exchange it uses to find
//! that timeline.
//!
//! **The host half** ([`hostreport`], and `chorus_hostctl` under it) reads the
//! real-time priority ceiling the container was granted, takes a priority no
//! greater than it, bounds every real-time thread with a CPU-time limit before
//! that thread does any audio work, decides the memory-locking question, and
//! reports all of it against what `/proc` says. Those are safety properties on
//! a machine that runs other things: `sched(7)` warns that "a nonblocking
//! infinite loop in a thread scheduled under the SCHED_FIFO, SCHED_RR, or
//! SCHED_DEADLINE policy can potentially block all other threads from
//! accessing the CPU forever", so the bound is the difference between a bug
//! here and an outage elsewhere.
//!
//! # What it refuses
//!
//! A format it does not support, a chunk duration that is not a whole number
//! of frames, a granted ceiling of zero, and a denied memory lock. Each of
//! those is a non-zero exit naming what it read and what it wanted, unless
//! configuration explicitly enabled running without that part of the contract,
//! in which case every subsequent status report says so.

#![warn(missing_docs)]

pub mod config;
pub mod hostreport;
pub mod serve;
pub mod source;
pub mod stream;

pub use config::{ServerConfig, ServerConfigError};
pub use hostreport::{ContractRefused, MemoryLockOutcome, RealTimeOutcome};
pub use serve::{serve_stream, ServeError, ServeParams, ServeReport};
pub use stream::{read_requests, write_outbound, Fanout, FanoutSink, Outbound};
