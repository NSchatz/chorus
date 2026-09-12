//! The chorus control plane: a versioned JSON catalog, the zone state it
//! changes, how that state is persisted, and how it reaches every subscriber.
//!
//! # A second catalog, beside the audio wire and not inside it
//!
//! `docs/protocol.md` and `fixtures/protocol/` are the contract for the audio
//! wire: frames on a TCP connection, big-endian, every timestamp monotonic.
//! Nothing here touches that. The control plane is its OWN catalog with its own
//! version, carried on its own connection, and `docs/control-plane.md` plus
//! `fixtures/control/` pin it the same way. A control message never travels on
//! an audio connection and an audio frame never travels on a control one.
//!
//! Why not one catalog: the audio wire is decoded by a C endpoint on an
//! ESP32-S3 with a fixed frame budget, and a JSON reader is not what belongs on
//! that path. Keeping them apart is also what lets the control catalog be
//! versioned and refused wholesale, which
//! [`catalog::decode_command`] does and the audio decoder deliberately does
//! not.
//!
//! # What is here, and what is not
//!
//! - [`json`] reads and writes the one canonical spelling of a message.
//! - [`catalog`] is the message types, their fields and their ranges.
//! - [`zones`] is the server-authoritative state a command changes.
//! - [`persist`] is what survives a restart.
//! - [`fanout`] is how a change reaches every subscriber, bounded.
//! - [`transport`] is the tier a zone is DECLARED in, beside the catalog and
//!   not inside it: no message carries a transport, `CATALOG_VERSION` does not
//!   move for it, and the vectors are byte-identical. It is here because it is
//!   a fact about a zone, and zones are what this crate owns.
//!
//! There is no socket in this crate, no clock in it, and no thread. The server
//! binds and threads in `crates/server/src/control.rs`; keeping those out of
//! here is what lets every rule above be tested with nothing running.

#![warn(missing_docs)]

pub mod catalog;
pub mod fanout;
pub mod json;
pub mod persist;
pub mod transport;
pub mod zones;

pub use catalog::{decode_command, Command, Refusal, RefusalKind, Volume, CATALOG_VERSION};
pub use fanout::{ControlFanout, CONTROL_QUEUE_LIMIT};
pub use transport::{GroupTier, Transport, WirelessPolicy, ZoneTransports, WIRELESS_POLICY};
pub use zones::{Zone, Zones};
