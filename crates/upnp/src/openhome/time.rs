//! OpenHome Time:1: where in the track.
//!
//! ohP `OpenHome/Av/ProviderTime.cpp` at `cccd06dd`. Three numbers:
//! `TrackCount` (up by one at each track start, `:64-72`), `Duration` in
//! whole seconds (0 when unknown, `:86-96`) and `Seconds`, the position in
//! whole seconds (`:79-84`), which returns to 0 when playing stops
//! (`:51-56`). `Seconds` changes once a second while something plays, and
//! each change is an event: the steadiest event load of the five services.
//! The server reads the position only while a subscriber exists.

use super::Property;
use crate::soap::Invocation;
use crate::{error, Outputs, UpnpError};

/// The Time state of one renderer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Time {
    track_count: u32,
    duration_s: u32,
    seconds: u32,
}

impl Time {
    /// Nothing has played: 0, 0, 0 (ohP `ProviderTime.cpp:19-25`).
    pub fn new() -> Time {
        Time::default()
    }

    /// A track starts.
    pub fn track(&mut self, duration_ms: Option<u64>) {
        self.track_count = self.track_count.wrapping_add(1);
        self.seconds = 0;
        self.duration(duration_ms);
    }

    /// The track's duration is known, or is not (`None`: 0).
    pub fn duration(&mut self, duration_ms: Option<u64>) {
        self.duration_s = duration_ms.map_or(0, |ms| (ms / 1000).min(u64::from(u32::MAX)) as u32);
    }

    /// The played position.
    pub fn position(&mut self, position_ms: u64) {
        self.seconds = (position_ms / 1000).min(u64::from(u32::MAX)) as u32;
    }

    /// Playing stopped: the position returns to 0.
    pub fn stopped(&mut self) {
        self.seconds = 0;
    }

    /// The position in whole seconds.
    pub fn seconds(&self) -> u32 {
        self.seconds
    }

    /// Every evented variable with its value, in the table's order.
    pub fn evented(&self) -> Vec<Property> {
        vec![
            ("TrackCount", self.track_count.to_string()),
            ("Duration", self.duration_s.to_string()),
            ("Seconds", self.seconds.to_string()),
        ]
    }

    /// Performs the Time action.
    pub fn invoke(&self, invocation: &Invocation) -> Result<Outputs, UpnpError> {
        match invocation.action.name {
            "Time" => Ok(self.evented()),
            _ => Err(error::INVALID_ACTION),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openhome::tables::TIME;

    #[test]
    fn time_counts_tracks_and_whole_seconds() {
        let mut t = Time::new();
        let read = |t: &Time| -> Vec<String> {
            t.invoke(&Invocation {
                action: TIME.action("Time").unwrap(),
                inputs: vec![],
            })
            .unwrap()
            .into_iter()
            .map(|(_, v)| v)
            .collect()
        };
        assert_eq!(read(&t), ["0", "0", "0"]);
        t.track(Some(181_999));
        t.position(4_999);
        assert_eq!(read(&t), ["1", "181", "4"]);
        t.position(5_000);
        assert_eq!(t.seconds(), 5);
        // The next track restarts the position; an unknown duration is 0.
        t.track(None);
        assert_eq!(read(&t), ["2", "0", "0"]);
        t.duration(Some(3_000));
        t.position(2_500);
        t.stopped();
        assert_eq!(read(&t), ["2", "3", "0"]);
        let names: Vec<&str> = t.evented().iter().map(|(n, _)| *n).collect();
        let table: Vec<&str> = TIME.variables.iter().map(|v| v.name).collect();
        assert_eq!(names, table);
        let outs: Vec<&str> = TIME.actions[0].outputs().map(|a| a.name).collect();
        assert_eq!(names, outs);
    }
}
