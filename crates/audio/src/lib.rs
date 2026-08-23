//! PCM ingest and chunking.
//!
//! The server's job in one sentence: take bytes that claim to be PCM in a
//! declared format, refuse them if the format is not one it supports, and cut
//! them into chunks of a fixed duration that carry a contiguous sequence and a
//! presentation timestamp on one monotonic timeline.
//!
//! Three properties are the whole point, and each has a test named after it:
//!
//! - **No sample is invented or lost.** The chunks concatenated are the input
//!   bytes, in order, except for a trailing remainder shorter than one frame,
//!   which is reported rather than padded ([`Chunker::finish`]).
//! - **Only the last chunk is short.** Every other chunk carries exactly the
//!   configured duration, and the short one is marked final.
//! - **The timeline is monotonic and evenly spaced.** Timestamps advance by
//!   exactly the configured chunk duration, start to start, so a short final
//!   chunk does not disturb the spacing.
//!
//! Nothing here reads a clock at all: [`Chunker`] is a pure function of its
//! configuration and its input, and the one clock the server owns is
//! [`clock::MonotonicTimeline`], which is built on `std::time::Instant`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod chunker;
pub mod clock;
pub mod format;

pub use chunker::{Chunk, Chunker, ChunkerReport};
pub use clock::MonotonicTimeline;
pub use format::{StreamFormat, UnsupportedFormat};
