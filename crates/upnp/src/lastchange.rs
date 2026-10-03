//! LastChange: how AVTransport and RenderingControl event their state.
//!
//! Both services can have several instances, so neither events its variables
//! directly: each has one evented variable, `LastChange`, whose value is a
//! small XML document naming the variables that changed and their new values
//! (AVT1 sections 2.2.27 and 2.3.1; RCS1 sections 2.2.1 and 2.3.1).
//!
//! ## Escaping, counted
//!
//! The `Event` document is the *value* of `LastChange`, so in the event
//! message it is the text of the `<LastChange>` element and is escaped once
//! by [`crate::gena::propertyset`]. A variable whose own value is XML (the
//! DIDL-Lite metadata) sits inside the `Event` document as a `val`
//! attribute and is escaped once by [`event_xml`]. So a subscriber unescapes
//! the property's text once to get the `Event` document, and reading that
//! document's `val` attribute unescapes the metadata once more: metadata
//! travels escaped twice, everything else once.
//!
//! ## Moderation
//!
//! `LastChange` is the one moderated variable of each service, with a
//! maximum event rate of 0.2 seconds (AVT1 section 2.3, table 2; RCS1 section
//! 2.3, table 2). [`Moderator`] is that rule as a pure queue.

/// The namespace of AVTransport's `Event` document (AVT1 section 5, the
/// schema's target namespace).
pub const AVT_NS: &str = "urn:schemas-upnp-org:metadata-1-0/AVT/";
/// The namespace of RenderingControl's `Event` document (RCS1 section 5).
pub const RCS_NS: &str = "urn:schemas-upnp-org:metadata-1-0/RCS/";

/// The shortest time between two LastChange events of one service instance,
/// in milliseconds (AVT1 and RCS1 section 2.3, table 2: "Max Event Rate"
/// 0.2).
pub const MODERATION_MS: u64 = 200;

/// The AVTransport variables that are never evented, directly or through
/// LastChange: they "change almost continiously" while playing, and control
/// points poll GetPositionInfo for them instead (AVT1 section 2.3.1).
pub const NEVER_EVENTED: [&str; 4] = [
    "RelativeTimePosition",
    "AbsoluteTimePosition",
    "RelativeCounterPosition",
    "AbsoluteCounterPosition",
];

/// One variable's new value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// The state variable's name.
    pub name: &'static str,
    /// The audio channel, for RenderingControl's per-channel variables
    /// (RCS1 section 2.2.1: "only the audio-related state variable include a
    /// 'channel' attribute"). Always `Master` in chorus.
    pub channel: Option<&'static str>,
    /// The new value.
    pub value: String,
}

impl Change {
    /// A change of a variable that has no channel.
    pub fn new(name: &'static str, value: impl Into<String>) -> Change {
        Change {
            name,
            channel: None,
            value: value.into(),
        }
    }

    /// A change of a variable of the Master channel.
    pub fn master(name: &'static str, value: impl Into<String>) -> Change {
        Change {
            name,
            channel: Some("Master"),
            value: value.into(),
        }
    }
}

/// The `Event` document for a set of changes of instance 0 (AVT1 section 5,
/// RCS1 sections 2.2.1 and 5): the root `Event` in the service's namespace,
/// one `InstanceID` element with `val="0"`, and under it one empty element
/// per variable, named after the variable, with its value in `val` (escaped
/// for an attribute) and, for a channel variable, `channel`. Order carries
/// no meaning (RCS1 section 2.2.1); it is the order given.
pub fn event_xml(namespace: &str, changes: &[Change]) -> String {
    let mut x = format!("<Event xmlns=\"{namespace}\"><InstanceID val=\"0\">");
    for change in changes {
        x.push('<');
        x.push_str(change.name);
        if let Some(channel) = change.channel {
            x.push_str(&format!(" channel=\"{channel}\""));
        }
        x.push_str(&format!(
            " val=\"{}\"/>",
            crate::xml::escape_attr(&change.value)
        ));
    }
    x.push_str("</InstanceID></Event>");
    x
}

/// The moderation queue of one service instance.
///
/// "multiple state changes are accumulated in the LastChange state variable
/// until its moderation period expires"; when a variable changes more than
/// once in a period the event holds "a single entry for that state variable
/// reflecting its current (most recent) value"; after the event is sent its
/// contents are cleared, and the clearing is not itself evented (RCS1
/// section 2.3.1). The first change after a quiet period goes out at once;
/// further changes wait until [`MODERATION_MS`] after the last event.
///
/// The position variables of [`NEVER_EVENTED`] are refused by
/// [`Moderator::record`], so they cannot reach an event by any path.
#[derive(Clone, Debug, Default)]
pub struct Moderator {
    pending: Vec<Change>,
    last_sent_ms: Option<u64>,
}

impl Moderator {
    /// An empty queue that has never sent.
    pub fn new() -> Moderator {
        Moderator::default()
    }

    /// Notes a change. A later change of the same variable (and channel)
    /// replaces the earlier one in place. Returns `false`, and keeps
    /// nothing, for a variable that is never evented.
    pub fn record(&mut self, change: Change) -> bool {
        if NEVER_EVENTED.contains(&change.name) {
            return false;
        }
        match self
            .pending
            .iter_mut()
            .find(|c| c.name == change.name && c.channel == change.channel)
        {
            Some(slot) => slot.value = change.value,
            None => self.pending.push(change),
        }
        true
    }

    /// Whether any change waits.
    pub fn is_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// The changes that wait, without taking them.
    pub fn pending(&self) -> &[Change] {
        &self.pending
    }

    /// When the waiting changes may be sent: `None` when nothing waits,
    /// otherwise the earliest `now_ms` at which [`Moderator::take`] yields
    /// them (0 when no event was ever sent).
    pub fn due_ms(&self) -> Option<u64> {
        if self.pending.is_empty() {
            return None;
        }
        Some(self.last_sent_ms.map_or(0, |last| last + MODERATION_MS))
    }

    /// The accumulated changes, if any wait and the moderation period since
    /// the last event has passed; the queue is then empty and the period
    /// restarts at `now_ms`.
    pub fn take(&mut self, now_ms: u64) -> Option<Vec<Change>> {
        match self.due_ms() {
            Some(due) if now_ms >= due => {
                self.last_sent_ms = Some(now_ms);
                Some(std::mem::take(&mut self.pending))
            }
            _ => None,
        }
    }

    /// Drops what waits without sending it and without restarting the
    /// period: for a renderer that goes away.
    pub fn discard(&mut self) {
        self.pending.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml::{self, unescape, Limits};

    #[test]
    fn the_avt_event_document_is_the_schemas_shape() {
        let x = event_xml(
            AVT_NS,
            &[
                Change::new("TransportState", "PLAYING"),
                Change::new("CurrentTrackDuration", "0:03:25"),
                Change::new("NextAVTransportURI", ""),
            ],
        );
        assert_eq!(
            x,
            "<Event xmlns=\"urn:schemas-upnp-org:metadata-1-0/AVT/\"><InstanceID val=\"0\"><TransportState val=\"PLAYING\"/><CurrentTrackDuration val=\"0:03:25\"/><NextAVTransportURI val=\"\"/></InstanceID></Event>"
        );
    }

    #[test]
    fn rcs_volume_and_mute_carry_the_master_channel() {
        let x = event_xml(
            RCS_NS,
            &[
                Change::master("Volume", "20"),
                Change::master("Mute", "0"),
                Change::new("PresetNameList", "FactoryDefaults"),
            ],
        );
        assert_eq!(
            x,
            "<Event xmlns=\"urn:schemas-upnp-org:metadata-1-0/RCS/\"><InstanceID val=\"0\"><Volume channel=\"Master\" val=\"20\"/><Mute channel=\"Master\" val=\"0\"/><PresetNameList val=\"FactoryDefaults\"/></InstanceID></Event>"
        );
    }

    /// The escaping count of the module documentation, end to end: the
    /// property text unescaped once is the Event document; the metadata in
    /// it unescaped once more is the DIDL-Lite the control point sent.
    #[test]
    fn metadata_is_escaped_twice_and_everything_else_once() {
        let didl = "<DIDL-Lite xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n<dc:title>A &amp; B &lt;\"x\"&gt; \u{e9}</dc:title></DIDL-Lite>";
        let uri = "http://192.0.2.50/a?x=1&y=2";
        let event = event_xml(
            AVT_NS,
            &[
                Change::new("AVTransportURI", uri),
                Change::new("AVTransportURIMetaData", didl),
            ],
        );
        let body = crate::gena::propertyset(&[("LastChange", &event)]);
        // Nothing of the inner documents is markup in the body.
        assert!(!body.contains("<Event"));
        assert!(body.contains("&lt;Event xmlns="));
        assert!(body.contains("&amp;lt;DIDL-Lite"));
        // A subscriber: parse the property set, take the text (one unescape,
        // by the reader), parse that as the Event document, read `val` (one
        // more).
        let set = xml::parse(&body, Limits::DEFAULT).unwrap();
        let text = &set.find("LastChange").unwrap().text;
        assert_eq!(text, &event);
        let doc = xml::parse(text, Limits::DEFAULT).unwrap();
        let instance = doc.child("InstanceID").unwrap();
        assert_eq!(instance.attr("val"), Some("0"));
        assert_eq!(
            instance.child("AVTransportURI").unwrap().attr("val"),
            Some(uri)
        );
        let meta = instance
            .child("AVTransportURIMetaData")
            .unwrap()
            .attr("val")
            .unwrap();
        assert_eq!(meta, didl, "byte for byte, the line break included");
        // And that string is itself a document whose title unescapes to the
        // characters the user sees.
        let title = xml::parse(meta, Limits::DEFAULT).unwrap();
        assert_eq!(title.child_text("title"), Some("A & B <\"x\"> \u{e9}"));
        // Unescaping the raw property text once by hand gives the same.
        let raw = body
            .split("<LastChange>")
            .nth(1)
            .unwrap()
            .split("</LastChange>")
            .next()
            .unwrap();
        assert_eq!(unescape(raw), event);
    }

    #[test]
    fn position_variables_never_enter_the_queue() {
        let mut m = Moderator::new();
        for name in NEVER_EVENTED {
            assert!(!m.record(Change::new(name, "0:00:01")));
        }
        assert!(!m.is_pending());
        assert_eq!(m.due_ms(), None);
        assert_eq!(m.take(1_000_000), None);
    }

    #[test]
    fn changes_coalesce_to_the_latest_value_per_variable_and_channel() {
        let mut m = Moderator::new();
        assert!(m.record(Change::master("Volume", "10")));
        m.record(Change::master("Mute", "1"));
        m.record(Change::master("Volume", "11"));
        m.record(Change::new("Volume", "99"));
        m.record(Change::master("Volume", "12"));
        assert_eq!(m.pending().len(), 3);
        assert_eq!(
            m.take(0).unwrap(),
            [
                Change::master("Volume", "12"),
                Change::master("Mute", "1"),
                Change::new("Volume", "99"),
            ]
        );
        assert!(!m.is_pending(), "cleared after the event");
        assert_eq!(m.take(10_000), None, "and the clearing is not an event");
        m.record(Change::new("TransportState", "STOPPED"));
        m.discard();
        assert_eq!(m.take(10_000), None);
    }

    /// 50 changes in 100 ms: at most one event per 200 ms, and the last
    /// event holds the final value.
    #[test]
    fn fifty_changes_in_100_ms_make_two_events_200_ms_apart() {
        let mut m = Moderator::new();
        let mut events: Vec<(u64, Vec<Change>)> = Vec::new();
        for now in 1000..2000u64 {
            let since = now - 1000;
            if since < 100 && since % 2 == 0 {
                m.record(Change::master("Volume", (since / 2 + 1).to_string()));
            }
            if let Some(changes) = m.take(now) {
                events.push((now, changes));
            }
        }
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].0, 1000, "the first change goes out at once");
        assert_eq!(events[1].0, 1200, "the rest wait out the period");
        assert!(events.windows(2).all(|w| w[1].0 - w[0].0 >= MODERATION_MS));
        assert_eq!(events[0].1, [Change::master("Volume", "1")]);
        assert_eq!(events[1].1, [Change::master("Volume", "50")]);
        // After a quiet period the next change is immediate again.
        m.record(Change::master("Volume", "51"));
        assert_eq!(m.due_ms(), Some(1400));
        assert!(m.take(5000).is_some());
    }
}
