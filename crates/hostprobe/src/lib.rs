//! Host evidence for chorus (goal 7, K34): what a development or production host
//! does to the two things the server's timing rests on.
//!
//! - [`wakeup`]: how late a periodic thread wakes under the host's real load
//!   (`chorus-wakeup-probe`).
//! - [`rxstamp`]: how far a user-space receive stamp lands from the kernel's own
//!   software receive timestamp for the same packet (`chorus-rx-stamps`).
//! - [`udploss`]: a LAN's datagram loss, loss bursts and transit variation at
//!   the TV path's cadence (`chorus-udp-loss`, goal 13).
//!
//! Neither is timing evidence for the production server unless it is run there
//! (BRIEF.md section 3.1 rule 3); reports say where each ran.

#![warn(missing_docs)]

pub mod environment;
pub mod histogram;
mod net;
pub mod rxstamp;
mod sys;
pub mod udploss;
pub mod wakeup;

pub use sys::{monotonic_ns, timer_slack_ns};
