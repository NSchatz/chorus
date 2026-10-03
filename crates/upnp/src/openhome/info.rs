//! OpenHome Info:1: what plays now.
//!
//! ohP `OpenHome/Av/ProviderInfo.cpp` at `cccd06dd`. Info reports whatever
//! the device plays, whichever source is selected: a Playlist track, a URI
//! cast over AVTransport, a line-in. Control points watch the three counters
//! to notice a change cheaply: `TrackCount` goes up by one at each track
//! start, `DetailsCount` returns to 0 with a new track and goes up when the
//! decoded stream is known, `MetatextCount` likewise for stream text (an
//! internet radio's "now playing").

use super::{bool_text, Property};
use crate::soap::Invocation;
use crate::{error, Outputs, UpnpError};

/// What the decoder found out about the stream.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Details {
    /// Whole seconds; 0 when unknown.
    pub duration_s: u32,
    /// Bits per second; 0 when unknown.
    pub bit_rate: u32,
    /// Bits per sample; 0 for a lossy stream.
    pub bit_depth: u32,
    /// Hz.
    pub sample_rate: u32,
    /// Whether the codec is lossless.
    pub lossless: bool,
    /// The codec's name.
    pub codec_name: String,
}

/// The Info state of one renderer.
#[derive(Clone, Debug, Default)]
pub struct Info {
    track_count: u32,
    details_count: u32,
    metatext_count: u32,
    uri: String,
    metadata: String,
    details: Details,
    metatext: String,
}

impl Info {
    /// Nothing has played: counters 0, strings empty (ohP
    /// `ProviderInfo.cpp:21-35`).
    pub fn new() -> Info {
        Info::default()
    }

    /// The track's URI and metadata.
    pub fn current(&self) -> (&str, &str) {
        (&self.uri, &self.metadata)
    }

    /// A track starts: its URI and the metadata exactly as the control point
    /// gave it. `TrackCount` goes up; the details and the stream text start
    /// over (ohP `ProviderInfo.cpp:57-71`, `:152-167`).
    pub fn track(&mut self, uri: &str, metadata: &str) {
        self.track_count = self.track_count.wrapping_add(1);
        self.uri = uri.to_string();
        self.metadata = metadata.to_string();
        self.details_count = 0;
        self.details = Details::default();
        self.metatext_count = 0;
        self.metatext.clear();
    }

    /// [`Info::track`] when the URI or the metadata differ from what is
    /// shown, and nothing otherwise: for what is not a track with a start (a
    /// line-in, another protocol's source), which is described again on
    /// every look. Returns whether it changed.
    pub fn follow(&mut self, uri: &str, metadata: &str) -> bool {
        if self.uri == uri && self.metadata == metadata {
            return false;
        }
        self.track(uri, metadata);
        true
    }

    /// The decoded stream is known (ohP `ProviderInfo.cpp:187-206`). The
    /// same details again are not counted.
    pub fn details(&mut self, details: Details) {
        if self.details_count > 0 && self.details == details {
            return;
        }
        self.details = details;
        self.details_count = self.details_count.wrapping_add(1);
    }

    /// The stream names a text (ohP `ProviderInfo.cpp:169-181`).
    pub fn metatext(&mut self, text: &str) {
        if self.metatext == text {
            return;
        }
        self.metatext = text.to_string();
        self.metatext_count = self.metatext_count.wrapping_add(1);
    }

    /// Every evented variable with its value, in the table's order.
    pub fn evented(&self) -> Vec<Property> {
        vec![
            ("TrackCount", self.track_count.to_string()),
            ("DetailsCount", self.details_count.to_string()),
            ("MetatextCount", self.metatext_count.to_string()),
            ("Uri", self.uri.clone()),
            ("Metadata", self.metadata.clone()),
            ("Duration", self.details.duration_s.to_string()),
            ("BitRate", self.details.bit_rate.to_string()),
            ("BitDepth", self.details.bit_depth.to_string()),
            ("SampleRate", self.details.sample_rate.to_string()),
            ("Lossless", bool_text(self.details.lossless)),
            ("CodecName", self.details.codec_name.clone()),
            ("Metatext", self.metatext.clone()),
        ]
    }

    /// Performs an Info action that passed [`crate::soap::validate`].
    pub fn invoke(&self, invocation: &Invocation) -> Result<Outputs, UpnpError> {
        Ok(match invocation.action.name {
            "Counters" => vec![
                ("TrackCount", self.track_count.to_string()),
                ("DetailsCount", self.details_count.to_string()),
                ("MetatextCount", self.metatext_count.to_string()),
            ],
            "Track" => vec![
                ("Uri", self.uri.clone()),
                ("Metadata", self.metadata.clone()),
            ],
            "Details" => vec![
                ("Duration", self.details.duration_s.to_string()),
                ("BitRate", self.details.bit_rate.to_string()),
                ("BitDepth", self.details.bit_depth.to_string()),
                ("SampleRate", self.details.sample_rate.to_string()),
                ("Lossless", bool_text(self.details.lossless)),
                ("CodecName", self.details.codec_name.clone()),
            ],
            "Metatext" => vec![("Value", self.metatext.clone())],
            _ => return Err(error::INVALID_ACTION),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openhome::tables::INFO;

    fn call(i: &Info, action: &str) -> Outputs {
        i.invoke(&Invocation {
            action: INFO.action(action).unwrap(),
            inputs: vec![],
        })
        .unwrap()
    }

    fn values(out: &Outputs) -> Vec<&str> {
        out.iter().map(|(_, v)| v.as_str()).collect()
    }

    #[test]
    fn the_counters_follow_tracks_details_and_stream_text() {
        let mut i = Info::new();
        assert_eq!(values(&call(&i, "Counters")), ["0", "0", "0"]);
        assert_eq!(values(&call(&i, "Track")), ["", ""]);
        i.track("http://192.0.2.9/a", "<DIDL-Lite/>");
        assert_eq!(values(&call(&i, "Counters")), ["1", "0", "0"]);
        let flac = Details {
            duration_s: 181,
            bit_rate: 1_411_200,
            bit_depth: 16,
            sample_rate: 44_100,
            lossless: true,
            codec_name: "flac".into(),
        };
        i.details(flac.clone());
        i.details(flac);
        i.metatext("Station: Song");
        i.metatext("Station: Song");
        assert_eq!(values(&call(&i, "Counters")), ["1", "1", "1"]);
        assert_eq!(
            values(&call(&i, "Details")),
            ["181", "1411200", "16", "44100", "1", "flac"]
        );
        assert_eq!(values(&call(&i, "Metatext")), ["Station: Song"]);
        // The next track starts the details and the text over, even when it
        // is the same URI again.
        i.track("http://192.0.2.9/a", "<DIDL-Lite/>");
        assert_eq!(values(&call(&i, "Counters")), ["2", "0", "0"]);
        assert_eq!(values(&call(&i, "Details")), ["0", "0", "0", "0", "0", ""]);
        // What has no start is followed by its description.
        assert!(!i.follow("http://192.0.2.9/a", "<DIDL-Lite/>"));
        assert!(i.follow("line-in:amp/line-1", ""));
        assert_eq!(values(&call(&i, "Counters")), ["3", "0", "0"]);
        assert_eq!(i.current(), ("line-in:amp/line-1", ""));
        let evented: Vec<&str> = i.evented().iter().map(|(n, _)| *n).collect();
        let table: Vec<&str> = INFO.variables.iter().map(|v| v.name).collect();
        assert_eq!(evented, table);
    }
}
