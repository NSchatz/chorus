//! The committed enumerations of the audio or timestamp path and of the
//! real-time acquisitions, and the three checks that keep them honest.
//!
//! # Why an enumeration and not a runtime property
//!
//! "Is this code on the audio path" cannot be answered by watching a process
//! run: a unit that reads a settable clock once an hour looks identical to one
//! that never does. So the path is a **list**, committed to
//! `audio-path.conf`, and this crate grades two things about it:
//!
//! 1. **No listed unit reads a settable wall-clock source.** The guardrail
//!    this repository has carried since its first commit. A stepped clock is
//!    what breaks playback, and `timens7` confirms the one clock a container's
//!    time namespace does **not** protect is the settable one.
//! 2. **The list is complete.** A list you can shrink is not a check. If a
//!    listed unit depends on a first-party unit that is neither listed nor
//!    recorded as an exclusion with a reason, the suite fails. That closes the
//!    obvious hole: move the clock read one file down and list nothing.
//!
//! Third-party dependencies are outside this by definition. This repository
//! has none, and auditing code it does not own is not something a green suite
//! should ever depend on.
//!
//! # The third check, and why it lives here
//!
//! 3. **Every real-time acquisition applies the CPU-time bound first.** A
//!    different invariant - scheduling rather than timestamps - and the same
//!    shape of answer: a committed enumeration
//!    (`real-time-acquisitions.conf`), a source scan that grades it, and red
//!    demonstrations that are committed rather than described. It lives in
//!    this crate because it is the same machinery, and a second crate that
//!    scanned source the same way would drift from this one. See
//!    [`realtime`].
//!
//! # What counts as a unit, and what counts as depending on one
//!
//! A **unit** is one Rust source file in this repository, named by its path
//! from the repository root.
//!
//! Unit A **depends on** unit B when either:
//!
//! - A declares B as a module (`mod b;` resolving to `b.rs` or `b/mod.rs`
//!   beside A), or
//! - A names a workspace crate in a `use` or a path expression, in which case
//!   A depends on that crate's root file, which in turn declares its own
//!   modules.
//!
//! Module declaration counts as a dependency on purpose. It is the relation
//! that lets a crate root pull a file into the build, so a list that named a
//! crate root and not its modules would be a list with a hole in it. The price
//! is that every module of a listed crate has to be decided about, one way or
//! the other, which is the point.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod list;
pub mod realtime;
pub mod scan;

pub use list::{AudioPathList, Excluded, ListError};
pub use realtime::{Acquisition, AcquisitionList, RealTimeFinding, SiteListError};
pub use scan::{ClockRead, Finding, MissingUnit};
