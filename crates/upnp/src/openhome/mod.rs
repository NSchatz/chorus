//! The OpenHome services on a chorus renderer (goal 17; P6, Option B):
//! Product:2, Volume:2, Info:1, Time:1 and Playlist:1.
//!
//! OpenHome has no specification text to cite: its definition is the service
//! XMLs and the reference implementation, ohPipeline and ohNet, both MIT.
//! Every rule here names the file and lines it was read from, at
//!
//! - **ohP**: `github.com/openhome/ohPipeline` at
//!   `cccd06dd49ab154f43e1f24e009fe066a14bf15f`;
//! - **ohN**: `github.com/openhome/ohNet` at
//!   `b16816876a88e5f1783225114470025458eb1b61`
//!
//! (read 2026-10-03; `docs/decisions/0128-openhome-services.md` lists what
//! was read). No code was
//! copied: the tables are transcribed data and the state machines are
//! chorus's own.
//!
//! What is here, all of it text in and text out:
//!
//! - [`tables`]: the five service tables.
//! - [`product`]: the device's names, its source list (`SourceXml`), the
//!   current source and standby.
//! - [`volume`]: volume on the 0 to 100 scale RenderingControl uses, mute,
//!   and `VolumeLimit` with ohPipeline's clamp-or-refuse rule.
//! - [`info`] and [`time`]: what plays and where in it.
//! - [`playlist`]: the queue. The device holds it and walks it, so tracks
//!   follow each other with no control point connected.
//! - [`Tracker`]: the event path of plain evented variables.
//!
//! What is not here, on purpose (K64): Radio, Credentials, OAuth, Pins and
//! Transport. The Product `Attributes` name none of them.
//!
//! Wire conventions (ohN `OpenHome/Net/Device/Upnp/DviServerUpnp.cpp:1311-1319`,
//! `OpenHome/Net/Device/DviSubscription.cpp:393-457`): a `boolean` is
//! written `1` or `0` in responses and events; a `bin.base64` value is
//! base64 in both; an event is a plain property set, one property per
//! variable, with no LastChange.

pub mod info;
pub mod playlist;
pub mod product;
pub mod tables;
pub mod time;
pub mod volume;

use crate::{error, UpnpError};

/// One evented variable and its value now.
pub type Property = (&'static str, String);

/// A `boolean` as OpenHome writes it: `1` or `0`.
pub fn bool_text(value: bool) -> String {
    if value { "1" } else { "0" }.to_string()
}

/// A `boolean` argument (UDA11 section 2.5: `0`, `false` or `no`; `1`,
/// `true` or `yes`), any case; anything else is 402 Invalid Args.
pub fn parse_bool(text: &str) -> Result<bool, UpnpError> {
    let t = text.trim();
    if ["1", "true", "yes"]
        .iter()
        .any(|v| t.eq_ignore_ascii_case(v))
    {
        Ok(true)
    } else if ["0", "false", "no"]
        .iter()
        .any(|v| t.eq_ignore_ascii_case(v))
    {
        Ok(false)
    } else {
        Err(error::INVALID_ARGS)
    }
}

/// A `ui4` argument; text that is not one is 402 Invalid Args.
pub fn parse_ui4(text: &str) -> Result<u32, UpnpError> {
    let t = text.trim();
    if t.is_empty() || t.len() > 10 || !t.bytes().all(|b| b.is_ascii_digit()) {
        return Err(error::INVALID_ARGS);
    }
    t.parse().map_err(|_| error::INVALID_ARGS)
}

/// An `i4` argument; text that is not one is 402 Invalid Args.
pub fn parse_i4(text: &str) -> Result<i32, UpnpError> {
    let t = text.trim();
    let digits = t.strip_prefix(['-', '+']).unwrap_or(t);
    if digits.is_empty() || digits.len() > 10 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(error::INVALID_ARGS);
    }
    t.strip_prefix('+')
        .unwrap_or(t)
        .parse()
        .map_err(|_| error::INVALID_ARGS)
}

/// The event path of a service whose variables are evented directly.
///
/// ohNet's rule (ohN `OpenHome/Net/Device/DviSubscription.cpp:219-258`): the
/// initial event of a subscription carries every evented variable; each
/// later event carries only the variables whose values changed, several in
/// one message when they changed together; a variable set to the value it
/// has is not evented (ohP `OpenHome/Av/ProviderInfo.cpp:174`). ohNet keeps
/// "changed since this subscriber's last event" per subscriber; a tracker
/// keeps one "last sent" value per variable for the service and its owner
/// sends what [`Tracker::poll`] returns to every subscriber that has had its
/// initial event, which says the same thing to each of them.
///
/// One kind of moderation exists, for the Playlist's `IdArray` (ohP
/// `OpenHome/Av/Playlist/ProviderPlaylist.h:41`, `ProviderPlaylist.cpp:491-521`):
/// a changed value is held for a period and what is sent then is the value
/// of that moment, so a burst of inserts is one event.
#[derive(Clone, Debug, Default)]
pub struct Tracker {
    sent: Vec<String>,
    /// When the held (moderated) variables are due, if any changed.
    held_until_ms: Option<u64>,
}

impl Tracker {
    /// A tracker that has sent nothing: the first [`Tracker::poll`] takes
    /// `current` as what every subscriber was told (their initial events
    /// carry the values of their own moment).
    pub fn new() -> Tracker {
        Tracker::default()
    }

    /// The variables of `current` whose values differ from what was last
    /// sent, in `current`'s order, each remembered as sent. A variable named
    /// in `moderated` is held back until `hold_ms` after its change was
    /// first seen, and then goes with its value of that moment. `current`
    /// must name the same variables in the same order on every call.
    pub fn poll(
        &mut self,
        current: &[Property],
        now_ms: u64,
        moderated: &[&str],
        hold_ms: u64,
    ) -> Vec<Property> {
        if self.sent.len() != current.len() {
            self.sent = current.iter().map(|(_, v)| v.clone()).collect();
            self.held_until_ms = None;
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut still_held = false;
        let release = self.held_until_ms.is_some_and(|at| now_ms >= at);
        for ((name, value), sent) in current.iter().zip(self.sent.iter_mut()) {
            if value == sent {
                continue;
            }
            if moderated.contains(name) && !release {
                still_held = true;
                continue;
            }
            *sent = value.clone();
            out.push((*name, value.clone()));
        }
        self.held_until_ms = match (still_held, self.held_until_ms) {
            (false, _) => None,
            (true, None) => Some(now_ms + hold_ms),
            (true, at) => at,
        };
        out
    }

    /// When a held variable is due.
    pub fn due_ms(&self) -> Option<u64> {
        self.held_until_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(a: &str, ids: &str) -> Vec<Property> {
        vec![("Id", a.to_string()), ("IdArray", ids.to_string())]
    }

    #[test]
    fn only_changed_variables_are_evented_and_a_same_value_is_not() {
        let mut t = Tracker::new();
        assert!(t.poll(&vars("0", ""), 0, &[], 0).is_empty());
        assert!(t.poll(&vars("0", ""), 10, &[], 0).is_empty());
        assert_eq!(
            t.poll(&vars("7", ""), 20, &[], 0),
            [("Id", "7".to_string())]
        );
        assert!(t.poll(&vars("7", ""), 30, &[], 0).is_empty());
        // Two that changed together leave together, in table order.
        assert_eq!(
            t.poll(&vars("8", "AAAACA=="), 40, &[], 0),
            vars("8", "AAAACA==")
        );
        assert_eq!(t.due_ms(), None);
    }

    #[test]
    fn a_moderated_variable_waits_and_goes_with_its_latest_value() {
        let mut t = Tracker::new();
        let m = ["IdArray"];
        assert!(t.poll(&vars("0", ""), 0, &m, 300).is_empty());
        // The id leaves at once; the array is held from its first change.
        assert_eq!(
            t.poll(&vars("1", "a"), 100, &m, 300),
            [("Id", "1".to_string())]
        );
        assert_eq!(t.due_ms(), Some(400));
        assert!(t.poll(&vars("1", "ab"), 200, &m, 300).is_empty());
        assert!(t.poll(&vars("1", "abc"), 399, &m, 300).is_empty());
        assert_eq!(t.due_ms(), Some(400));
        assert_eq!(
            t.poll(&vars("1", "abc"), 400, &m, 300),
            [("IdArray", "abc".to_string())]
        );
        assert_eq!(t.due_ms(), None);
        // A change that is undone before the period ends is never sent.
        assert!(t.poll(&vars("1", "x"), 500, &m, 300).is_empty());
        assert_eq!(t.due_ms(), Some(800));
        assert!(t.poll(&vars("1", "abc"), 600, &m, 300).is_empty());
        assert_eq!(t.due_ms(), None);
    }

    #[test]
    fn booleans_and_numbers_are_read_strictly() {
        for yes in ["1", "true", "YES", " True "] {
            assert_eq!(parse_bool(yes), Ok(true), "{yes}");
        }
        for no in ["0", "false", "No"] {
            assert_eq!(parse_bool(no), Ok(false), "{no}");
        }
        for bad in ["", "2", "on", "t"] {
            assert_eq!(parse_bool(bad), Err(error::INVALID_ARGS), "{bad}");
        }
        assert_eq!(
            (bool_text(true), bool_text(false)),
            ("1".into(), "0".into())
        );
        assert_eq!(parse_ui4(" 42 "), Ok(42));
        assert_eq!(parse_ui4("4294967295"), Ok(u32::MAX));
        for bad in ["", "-1", "4294967296", "1.0", "x", "+1"] {
            assert_eq!(parse_ui4(bad), Err(error::INVALID_ARGS), "{bad}");
        }
        assert_eq!(parse_i4("-5"), Ok(-5));
        assert_eq!(parse_i4("+5"), Ok(5));
        for bad in ["", "-", "2147483648", "1e3", "--1"] {
            assert_eq!(parse_i4(bad), Err(error::INVALID_ARGS), "{bad}");
        }
    }
}
