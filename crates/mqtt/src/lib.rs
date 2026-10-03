//! The pure half of chorus's MQTT publisher (goal 15, P10 Option M2).
//!
//! chorus-server can publish what it already knows to an MQTT broker: off by
//! default, read-only, publish-only (`docs/mqtt.md`,
//! `crates/server/src/mqtt.rs`). This crate is everything about that which
//! needs no socket, no thread and no clock, so it is tested as bytes:
//!
//! - [`codec`]: the MQTT 3.1.1 packets a publish-only client sends (CONNECT,
//!   PUBLISH, PINGREQ, DISCONNECT) and the three it reads (CONNACK, PUBACK,
//!   PINGRESP), with the Remaining Length varint. There is no SUBSCRIBE
//!   encoder here at all: what cannot be encoded cannot be sent.
//! - [`topic`]: the topic names, the prefix rule and the rule that makes an
//!   id safe as one topic level.
//! - [`payload`]: a room's and a saved group's object cut out of the control
//!   state message byte for byte, and the event a controller command becomes.
//!
//! Section and statement numbers in the comments (`SPEC 3.1.2`,
//! `[MQTT-3.1.2-9]`) are those of OASIS "MQTT Version 3.1.1", OASIS Standard,
//! 29 October 2014
//! (<http://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html>,
//! read 2026-10-03). Nothing here was read from another implementation.

#![warn(missing_docs)]

pub mod codec;
pub mod payload;
pub mod topic;
