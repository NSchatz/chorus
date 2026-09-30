//! chorus's one controller model: a speaker's physical controls, its status
//! LED and its microphone gate, shared by every endpoint.
//!
//! The Rust twin of `firmware/src/controls.c` (`docs/decisions/0063-*`, and
//! the decision that made it a twin, `docs/decisions/0080-*`). The two are one model in two
//! languages and are held to the same committed fixtures,
//! `fixtures/controls/*`: `crates/controls/tests/shared_fixtures.rs` drives
//! the exact input scripts of `firmware/tests/test_controls.c` and must
//! produce the same `controller_command` bytes and the same LED moments. A
//! behaviour change is a fixture change read by both, never a constant in one
//! language only (conventions rule 9).
//!
//! A pure library: nothing here reads a clock, a pin or a file. A binding
//! (the ESP32-S3's GPIO and ADC in C, the Linux client's evdev and LED class
//! in `crates/client-linux/src/front_panel.rs`) hands in a level or an ADC
//! code with the monotonic time it was sampled at (BRIEF.md guardrail 4) and
//! takes back [`Action`]s. What leaves an endpoint is a protocol v2
//! `controller_command`: a button is the controller role of
//! `docs/protocol.md`, never a second control plane, and the server decides
//! what the request changes and clamps it by the room's limits (K81, I10).
//!
//! Every time constant is ASSUMED, exactly as in `controls.h`: chosen, not
//! measured, until a bench session on the owner's hardware says otherwise.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod led;
pub mod model;

pub use led::{Led, LedInputs, LedOutput, LedState};
pub use model::{
    Action, ActionKind, Controls, Input, InputRefused, LedKind, Profile, SpeakerClass,
};
