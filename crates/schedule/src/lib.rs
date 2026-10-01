//! The chorus schedule library: what time it is where the listener lives, and
//! what the server should do about it.
//!
//! The server's alarm, sleep-timer and quiet-hours runtime calls this crate.
//! It is a pure library: no socket, no thread, no file read and, above all,
//! **no clock read**. Every function that needs "now" takes it as an argument,
//! which is what lets every rule below be tested at any instant in any year,
//! including the two seconds a year a daylight-saving change makes awkward.
//!
//! # Civil time is allowed here, and only for scheduling
//!
//! BRIEF.md section 3.1 rule 4 keeps monotonic clocks on the audio path, and
//! `audio-path.conf` enumerates that path. This crate is not on it. An alarm
//! at 07:00 is a statement about the wall clock in a time zone, so scheduling
//! has to speak civil time (K30): the instants this crate takes and returns
//! for alarms and windows are UTC seconds since the Unix epoch, the POSIX
//! timescale, and the server reads them from its settable clock outside this
//! crate. What crosses back towards audio is never a civil instant: a sleep
//! timer and a ramp are monotonic durations ([`sleep`], [`ramp`]), and a
//! volume is an integer in thousandths. `audio-path.conf` records every unit
//! here as excluded, with that reason, and `tests/no_clock.rs` holds the crate
//! to reading no clock of any kind.
//!
//! # What is here
//!
//! - [`civil`]: dates, weekdays, days masks and times of day; arithmetic only.
//! - [`posix`]: the POSIX TZ string (the TZif footer, or a `TZ` value).
//! - [`tzif`]: the TZif zoneinfo reader (RFC 9636, versions 1 to 4).
//! - [`zone`]: a zone (UTC, a fixed offset, a TZ string or a TZif file), the
//!   UTC instant to civil time conversion and its inverse, with the gap and
//!   fold rules.
//! - [`window`]: weekly windows (quiet hours).
//! - [`alarm`]: alarms, snooze, and the "what is due" query the server polls.
//! - [`sleep`]: sleep timers and their fade-out plan.
//! - [`ramp`]: integer volume ramps.
//! - [`chime`]: chorus's own generated chimes, rendered to PCM.
//!
//! The decisions behind all of it, every ASSUMED default among them, are
//! `docs/decisions/` "the schedule library" record; the chimes are listed in
//! `docs/chimes.md`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod alarm;
pub mod chime;
pub mod civil;
pub mod posix;
pub mod ramp;
pub mod sleep;
pub mod tzif;
pub mod window;
pub mod zone;

pub use alarm::{due_between, Alarm, Due, DEFAULT_SNOOZE_MIN};
pub use chime::{encode, render, Chime, PcmFormat, RenderError};
pub use civil::{Civil, Days, TimeOfDay, Weekday};
pub use ramp::{alarm_ramp, Ramp, DEFAULT_ALARM_RAMP_S};
pub use sleep::{SleepStep, SleepTimer, SLEEP_FADE_S};
pub use window::WeeklyWindow;
pub use zone::{Resolved, Zone, ZoneError};
