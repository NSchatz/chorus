//! HDMI-CEC for chorus's Linux hub: the hub is the TV's Audio System
//! (logical address 5), so the TV's remote turns the room's volume up and
//! down, the TV shows the room's level, and the TV turning on or off
//! becomes the TV input's signal (goal 13, the TV path; `docs/cec.md`).
//!
//! - [`codec`]: the message, its header, opcodes and operands, as bytes;
//!   golden vectors in `fixtures/cec/`.
//! - [`validate`]: whether a received message is well formed for its
//!   opcode, as Android's validator decides (Apache-2.0, cited there).
//! - [`role`]: the Audio System state machine, pure (no device, no clock).
//! - [`power`]: the TV's power shared with the source role, and the rule
//!   that makes the TV input's offered signal from it.
//! - [`adapter`]: what the role needs of an adapter; [`kernel`] is
//!   `/dev/cecN` through the kernel's CEC API, [`fake`] a bus in memory with
//!   a scripted TV.
//! - [`driver`]: the role on an adapter, one turn at a time.
//!
//! Clean room (BRIEF.md guardrail 1, K33): built from the kernel's CEC API
//! documentation prose, the MIT `cec_linux` crate and Android's Apache-2.0
//! HDMI service and CTS tests. No GPL source (libcec, v4l-utils' `cec-ctl`
//! and `cec-follower`, the kernel's C source and uapi headers, vivid) was
//! opened; goal 13's research file lists what was read.
//!
//! Unsafe code lives in [`kernel`] alone, the ioctl binding; the rest of the
//! crate is held to the workspace's `unsafe_code = "deny"`.

#![warn(missing_docs)]

pub mod adapter;
pub mod codec;
pub mod driver;
pub mod fake;
pub mod kernel;
pub mod power;
pub mod role;
pub mod validate;

pub use adapter::{Adapter, AdapterError, Claim, Claimed, Event, TxStatus};
pub use codec::{AudioStatus, Message, PhysicalAddress};
pub use driver::{Driver, DriverCounters};
pub use fake::{FakeAdapter, FakeBus, FakeTv, TvKind};
pub use kernel::KernelAdapter;
pub use power::{SignalReason, TvPower, TvPowerState, TvSignal};
pub use role::{AudioSystem, Config, Effect, MuteRequest, VolumeKey};
