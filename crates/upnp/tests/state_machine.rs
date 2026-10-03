//! The AVTransport state machine, walked by tables.
//!
//! Each walk is a list of steps: an action as a control point sends it
//! (through the SOAP table, so the argument rules are in the path), a report
//! as the server's player makes it, or a check. After every step whatever
//! was recorded for LastChange is drained into a history, so a walk can say
//! what was evented and, as importantly, what never was.

use chorus_upnp::avtransport::{AvTransport, Effect};
use chorus_upnp::gena::{self, Subscriptions};
use chorus_upnp::lastchange::{event_xml, Change, MODERATION_MS, RCS_NS};
use chorus_upnp::rendering::{self, RenderingControl};
use chorus_upnp::soap::{self, ActionRequest};
use chorus_upnp::{client, description, error, Service, UpnpError};

const U1: &str = "http://192.0.2.50:8000/track1.flac";
const U2: &str = "http://192.0.2.50:8000/track2.flac";
const U3: &str = "http://192.0.2.50:8000/track3.flac";
const M1: &str = "<DIDL-Lite xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><item><dc:title>One</dc:title><res duration=\"0:00:03\">http://192.0.2.50:8000/track1.flac</res></item></DIDL-Lite>";
const M2: &str = "<DIDL-Lite xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><item><dc:title>A &amp; B &lt;\"x\"&gt; \u{e9}</dc:title><res duration=\"0:03:25\">http://192.0.2.50:8000/track2.flac</res></item></DIDL-Lite>";

type Args = &'static [(&'static str, &'static str)];

#[derive(Clone, Copy)]
enum Report {
    Opened(Option<u64>, bool),
    Playing,
    Boundary(Option<u64>),
    NextFailed,
    Ended,
    Failed,
    Position(u64),
}

#[derive(Clone, Copy)]
enum Step {
    /// An action that succeeds with these effects (see `short`).
    Ok(&'static str, Args, &'static str),
    /// An action that fails with this error code, changing nothing.
    Err(&'static str, Args, u16),
    /// A report from the player; whether it was taken.
    Player(Report, bool),
    /// An out argument of a Get action.
    Out(&'static str, &'static str, &'static str),
    /// TransportState, TransportStatus, CurrentTransportActions.
    Is(&'static str, &'static str, &'static str),
    /// The changes drained by the previous step, as `Name=value` joined by
    /// `;` (empty: nothing was recorded).
    Evented(&'static str),
}

fn short(effect: &Effect) -> String {
    match effect {
        Effect::Load { uri, .. } => format!("Load({uri})"),
        Effect::Start => "Start".into(),
        Effect::Pause => "Pause".into(),
        Effect::Resume => "Resume".into(),
        Effect::Stop => "Stop".into(),
        Effect::SeekTo { ms } => format!("SeekTo({ms})"),
        Effect::QueueNext { uri, .. } => format!("QueueNext({uri})"),
        Effect::ClearNext => "ClearNext".into(),
        Effect::SkipToNext => "SkipToNext".into(),
    }
}

struct Rig {
    t: AvTransport,
    now: u64,
    /// Every change ever recorded, in order.
    history: Vec<Change>,
    last: Vec<Change>,
}

type Outcome = Result<(Vec<(&'static str, String)>, Vec<Effect>), UpnpError>;

impl Rig {
    fn new() -> Rig {
        Rig {
            t: AvTransport::new(),
            now: 0,
            history: Vec::new(),
            last: Vec::new(),
        }
    }

    fn act(&mut self, action: &str, args: &[(&str, &str)]) -> Outcome {
        let mut arguments = vec![("InstanceID".to_string(), "0".to_string())];
        for (n, v) in args {
            if *n == "InstanceID" {
                arguments[0].1 = v.to_string();
            } else {
                arguments.push((n.to_string(), v.to_string()));
            }
        }
        let request = ActionRequest {
            service_type: Service::AvTransport.service_type().into(),
            action: action.into(),
            arguments,
        };
        let invocation = soap::validate(Service::AvTransport, &request)?;
        self.t.invoke(&invocation)
    }

    fn drain(&mut self) {
        self.now += 1000;
        self.last = self.t.events().take(self.now).unwrap_or_default();
        self.history.extend(self.last.iter().cloned());
    }

    fn snapshot(&self) -> String {
        format!("{:?}", self.t.evented())
    }

    fn run(&mut self, walk: &str, steps: &[Step]) {
        for (i, step) in steps.iter().enumerate() {
            let at = format!("{walk}, step {i}");
            match *step {
                Step::Ok(action, args, effects) => {
                    let (_, got) = self
                        .act(action, args)
                        .unwrap_or_else(|e| panic!("{at}: {action} failed with {e:?}"));
                    let got: Vec<String> = got.iter().map(short).collect();
                    assert_eq!(got.join("|"), effects, "{at}: {action}");
                    self.drain();
                }
                Step::Err(action, args, code) => {
                    let before = self.snapshot();
                    let epoch = self.t.epoch();
                    let error = self
                        .act(action, args)
                        .err()
                        .unwrap_or_else(|| panic!("{at}: {action} succeeded"));
                    assert_eq!(error.code, code, "{at}: {action} gave {error:?}");
                    assert_eq!(
                        self.snapshot(),
                        before,
                        "{at}: a failed action changed state"
                    );
                    assert_eq!(self.t.epoch(), epoch, "{at}");
                    assert!(!self.t.events().is_pending(), "{at}");
                    self.last.clear();
                }
                Step::Player(report, taken) => {
                    let e = self.t.epoch();
                    let got = match report {
                        Report::Opened(d, s) => self.t.media_opened(e, d, s),
                        Report::Playing => self.t.playing(e),
                        Report::Boundary(d) => self.t.track_boundary(e, d, true),
                        Report::NextFailed => self.t.next_failed(e, "unsupported: aac"),
                        Report::Ended => {
                            let before = self.snapshot();
                            let effects = self.t.ended(e);
                            assert!(effects.is_empty(), "{at}: {effects:?}");
                            self.snapshot() != before
                        }
                        Report::Failed => self.t.failed(e, "http 404"),
                        Report::Position(ms) => {
                            self.t.position(ms);
                            taken
                        }
                    };
                    assert_eq!(got, taken, "{at}");
                    self.drain();
                }
                Step::Out(action, name, value) => {
                    let (out, effects) = self.act(action, &[]).unwrap();
                    assert!(effects.is_empty(), "{at}");
                    let got = out
                        .iter()
                        .find(|(n, _)| *n == name)
                        .unwrap_or_else(|| panic!("{at}: {action} has no {name}"));
                    assert_eq!(got.1, value, "{at}: {action}.{name}");
                    // A Get action's out arguments are the table's, in order.
                    let names: Vec<&str> = out.iter().map(|(n, _)| *n).collect();
                    let table: Vec<&str> = description::AVTRANSPORT
                        .action(action)
                        .unwrap()
                        .outputs()
                        .map(|a| a.name)
                        .collect();
                    assert_eq!(names, table, "{at}");
                }
                Step::Is(state, status, actions) => {
                    assert_eq!(self.t.state().as_str(), state, "{at}");
                    assert_eq!(self.t.status().as_str(), status, "{at}");
                    assert_eq!(self.t.transport_actions(), actions, "{at}");
                    // The Get actions agree with the variables.
                    let (info, _) = self.act("GetTransportInfo", &[]).unwrap();
                    assert_eq!(info[0].1, state, "{at}");
                    assert_eq!(info[1].1, status, "{at}");
                    let (a, _) = self.act("GetCurrentTransportActions", &[]).unwrap();
                    assert_eq!(a[0].1, actions, "{at}");
                }
                Step::Evented(expected) => {
                    let got: Vec<String> = self
                        .last
                        .iter()
                        .map(|c| format!("{}={}", c.name, c.value))
                        .collect();
                    assert_eq!(got.join(";"), expected, "{at}");
                }
            }
        }
    }

    fn ever(&self, name: &str, value: &str) -> bool {
        self.history
            .iter()
            .any(|c| c.name == name && c.value == value)
    }
}

use Report::*;
use Step::{Err as Refused, Evented, Is, Ok as Does, Out, Player};

const SET1: Step = Does(
    "SetAVTransportURI",
    &[("CurrentURI", U1), ("CurrentURIMetaData", M1)],
    "Load(http://192.0.2.50:8000/track1.flac)",
);
const PLAY: Step = Does("Play", &[("Speed", "1")], "Start");
const SETNEXT2: Step = Does(
    "SetNextAVTransportURI",
    &[("NextURI", U2), ("NextURIMetaData", M2)],
    "QueueNext(http://192.0.2.50:8000/track2.flac)",
);

#[test]
fn idle_queries_answer_with_no_media() {
    Rig::new().run(
        "idle",
        &[
            Is("NO_MEDIA_PRESENT", "OK", ""),
            Out("GetTransportInfo", "CurrentSpeed", "1"),
            Out("GetMediaInfo", "NrTracks", "0"),
            Out("GetMediaInfo", "MediaDuration", "0:00:00"),
            Out("GetMediaInfo", "CurrentURI", ""),
            Out("GetMediaInfo", "CurrentURIMetaData", ""),
            Out("GetMediaInfo", "NextURI", ""),
            Out("GetMediaInfo", "NextURIMetaData", ""),
            Out("GetMediaInfo", "PlayMedium", "NONE"),
            Out("GetMediaInfo", "RecordMedium", "NOT_IMPLEMENTED"),
            Out("GetMediaInfo", "WriteStatus", "NOT_IMPLEMENTED"),
            Out("GetPositionInfo", "Track", "0"),
            Out("GetPositionInfo", "TrackDuration", "0:00:00"),
            Out("GetPositionInfo", "TrackMetaData", ""),
            Out("GetPositionInfo", "TrackURI", ""),
            Out("GetPositionInfo", "RelTime", "0:00:00"),
            Out("GetPositionInfo", "AbsTime", "0:00:00"),
            Out("GetPositionInfo", "RelCount", "2147483647"),
            Out("GetPositionInfo", "AbsCount", "2147483647"),
            Out("GetDeviceCapabilities", "PlayMedia", "NETWORK"),
            Out("GetDeviceCapabilities", "RecMedia", "NOT_IMPLEMENTED"),
            Out(
                "GetDeviceCapabilities",
                "RecQualityModes",
                "NOT_IMPLEMENTED",
            ),
            Out("GetTransportSettings", "PlayMode", "NORMAL"),
            Out("GetTransportSettings", "RecQualityMode", "NOT_IMPLEMENTED"),
            Out("GetCurrentTransportActions", "Actions", ""),
            // Stop with nothing loaded succeeds quietly; nothing is evented.
            Does("Stop", &[], ""),
            Evented(""),
            // A report with nothing loaded is ignored.
            Player(Playing, false),
            Player(Ended, false),
            Player(Failed, false),
            Player(Position(5000), false),
            Out("GetPositionInfo", "RelTime", "0:00:00"),
        ],
    );
}

#[test]
fn set_uri_play_pause_play_seek_stop() {
    let mut rig = Rig::new();
    rig.run(
        "transport",
        &[
            SET1,
            Evented("TransportState=STOPPED;PlaybackStorageMedium=NETWORK;NumberOfTracks=1;CurrentTrack=1;CurrentTrackDuration=0:00:03;CurrentMediaDuration=0:00:03;CurrentTrackMetaData=<DIDL-Lite xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><item><dc:title>One</dc:title><res duration=\"0:00:03\">http://192.0.2.50:8000/track1.flac</res></item></DIDL-Lite>;CurrentTrackURI=http://192.0.2.50:8000/track1.flac;AVTransportURI=http://192.0.2.50:8000/track1.flac;AVTransportURIMetaData=<DIDL-Lite xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><item><dc:title>One</dc:title><res duration=\"0:00:03\">http://192.0.2.50:8000/track1.flac</res></item></DIDL-Lite>;CurrentTransportActions=Play,Seek"),
            Is("STOPPED", "OK", "Play,Seek"),
            Out("GetMediaInfo", "NrTracks", "1"),
            Out("GetMediaInfo", "MediaDuration", "0:00:03"),
            Out("GetMediaInfo", "CurrentURI", U1),
            Out("GetMediaInfo", "CurrentURIMetaData", M1),
            Out("GetMediaInfo", "PlayMedium", "NETWORK"),
            Out("GetPositionInfo", "Track", "1"),
            Out("GetPositionInfo", "TrackURI", U1),
            Out("GetPositionInfo", "TrackMetaData", M1),
            // The decoder's duration replaces the control point's claim.
            Player(Opened(Some(3500), true), true),
            Evented(""),
            Player(Opened(Some(4000), true), true),
            Evented("CurrentTrackDuration=0:00:04;CurrentMediaDuration=0:00:04"),
            PLAY,
            Evented("TransportState=TRANSITIONING;CurrentTransportActions=Stop"),
            Is("TRANSITIONING", "OK", "Stop"),
            // Play again while getting there: accepted, nothing changes.
            Does("Play", &[("Speed", "1")], ""),
            Evented(""),
            Player(Playing, true),
            Evented("TransportState=PLAYING;CurrentTransportActions=Stop,Pause,Seek"),
            Is("PLAYING", "OK", "Stop,Pause,Seek"),
            Does("Play", &[("Speed", " 1 ")], ""),
            Player(Position(1500), true),
            Evented(""),
            Out("GetPositionInfo", "RelTime", "0:00:01"),
            Out("GetPositionInfo", "AbsTime", "0:00:01"),
            Out("GetPositionInfo", "TrackDuration", "0:00:04"),
            Does("Pause", &[], "Pause"),
            Evented("TransportState=PAUSED_PLAYBACK;CurrentTransportActions=Play,Stop,Seek"),
            Is("PAUSED_PLAYBACK", "OK", "Play,Stop,Seek"),
            // Not a toggle.
            Does("Pause", &[], ""),
            Is("PAUSED_PLAYBACK", "OK", "Play,Stop,Seek"),
            Out("GetPositionInfo", "RelTime", "0:00:01"),
            Does("Play", &[("Speed", "1")], "Resume"),
            Evented("TransportState=PLAYING;CurrentTransportActions=Stop,Pause,Seek"),
            Is("PLAYING", "OK", "Stop,Pause,Seek"),
            Does("Seek", &[("Unit", "REL_TIME"), ("Target", "0:00:02")], "SeekTo(2000)"),
            Evented("TransportState=TRANSITIONING;CurrentTransportActions=Stop"),
            // Until audio is out again, the player's old position is not
            // taken: the target stands.
            Player(Position(1700), false),
            Out("GetPositionInfo", "RelTime", "0:00:02"),
            Player(Playing, true),
            Is("PLAYING", "OK", "Stop,Pause,Seek"),
            // A position past the end reads as the end.
            Player(Position(9000), true),
            Out("GetPositionInfo", "RelTime", "0:00:04"),
            Does("Seek", &[("Unit", "TRACK_NR"), ("Target", "1")], "SeekTo(0)"),
            Player(Playing, true),
            Does("Stop", &[], "Stop"),
            Evented("TransportState=STOPPED;CurrentTransportActions=Play,Seek"),
            Is("STOPPED", "OK", "Play,Seek"),
            Out("GetPositionInfo", "RelTime", "0:00:00"),
            Out("GetMediaInfo", "CurrentURI", U1),
            Out("GetMediaInfo", "CurrentURIMetaData", M1),
            // Stop when stopped: success, no effect, nothing evented.
            Does("Stop", &[], ""),
            Evented(""),
            // A seek while stopped sets where Play will start.
            Does("Seek", &[("Unit", "REL_TIME"), ("Target", "0:00:01")], "SeekTo(1000)"),
            Is("STOPPED", "OK", "Play,Seek"),
            Evented(""),
            // And while paused, the pause holds.
            PLAY,
            Player(Playing, true),
            Does("Pause", &[], "Pause"),
            Does("Seek", &[("Unit", "REL_TIME"), ("Target", "0:00:03")], "SeekTo(3000)"),
            Is("PAUSED_PLAYBACK", "OK", "Play,Stop,Seek"),
            Out("GetPositionInfo", "RelTime", "0:00:03"),
            Does("Play", &[("Speed", "1")], "Resume"),
            // Pause while still transitioning after a seek holds too.
            Does("Seek", &[("Unit", "REL_TIME"), ("Target", "0:00:00")], "SeekTo(0)"),
            Does("Pause", &[], "Pause"),
            Player(Playing, false),
            Is("PAUSED_PLAYBACK", "OK", "Play,Stop,Seek"),
        ],
    );
    // No position variable was ever recorded for an event.
    for never in chorus_upnp::lastchange::NEVER_EVENTED {
        assert!(rig.history.iter().all(|c| c.name != never), "{never}");
    }
}

#[test]
fn the_end_of_the_media_is_stopped() {
    Rig::new().run(
        "end of media",
        &[
            SET1,
            PLAY,
            Player(Playing, true),
            Player(Position(2900), true),
            Player(Ended, true),
            Evented("TransportState=STOPPED;CurrentTransportActions=Play,Seek"),
            Is("STOPPED", "OK", "Play,Seek"),
            Out("GetPositionInfo", "RelTime", "0:00:00"),
            Out("GetMediaInfo", "CurrentURI", U1),
            // A second "ended" for the same play is from an older epoch.
            Player(Ended, false),
            // Play starts it again.
            PLAY,
            Player(Playing, true),
            Is("PLAYING", "OK", "Stop,Pause,Seek"),
        ],
    );
}

#[test]
fn the_gapless_sequence_keeps_playing_and_events_once() {
    let mut rig = Rig::new();
    rig.run(
        "gapless, before the boundary",
        &[
            SET1,
            PLAY,
            Player(Playing, true),
            SETNEXT2,
            Evented("NextAVTransportURI=http://192.0.2.50:8000/track2.flac;NextAVTransportURIMetaData=<DIDL-Lite xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><item><dc:title>A &amp; B &lt;\"x\"&gt; \u{e9}</dc:title><res duration=\"0:03:25\">http://192.0.2.50:8000/track2.flac</res></item></DIDL-Lite>;CurrentTransportActions=Stop,Pause,Seek,Next"),
            Is("PLAYING", "OK", "Stop,Pause,Seek,Next"),
            Out("GetMediaInfo", "NextURI", U2),
            Out("GetMediaInfo", "NextURIMetaData", M2),
            Out("GetMediaInfo", "CurrentURI", U1),
            Player(Position(2999), true),
        ],
    );
    let before = rig.history.len();
    let epoch = rig.t.epoch();
    rig.run(
        "gapless, the boundary",
        &[
            Player(Boundary(None), true),
            // The exact content of the one LastChange: the URI variables
            // and durations move, the next URI empties, and there is no
            // TransportState in it at all.
            Evented("CurrentTrackDuration=0:03:25;CurrentMediaDuration=0:03:25;CurrentTrackMetaData=<DIDL-Lite xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><item><dc:title>A &amp; B &lt;\"x\"&gt; \u{e9}</dc:title><res duration=\"0:03:25\">http://192.0.2.50:8000/track2.flac</res></item></DIDL-Lite>;CurrentTrackURI=http://192.0.2.50:8000/track2.flac;AVTransportURI=http://192.0.2.50:8000/track2.flac;AVTransportURIMetaData=<DIDL-Lite xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><item><dc:title>A &amp; B &lt;\"x\"&gt; \u{e9}</dc:title><res duration=\"0:03:25\">http://192.0.2.50:8000/track2.flac</res></item></DIDL-Lite>;NextAVTransportURI=;NextAVTransportURIMetaData=;CurrentTransportActions=Stop,Pause,Seek"),
            Is("PLAYING", "OK", "Stop,Pause,Seek"),
            Out("GetMediaInfo", "CurrentURI", U2),
            Out("GetMediaInfo", "CurrentURIMetaData", M2),
            Out("GetMediaInfo", "NextURI", ""),
            Out("GetMediaInfo", "NextURIMetaData", ""),
            Out("GetMediaInfo", "NrTracks", "1"),
            Out("GetPositionInfo", "Track", "1"),
            Out("GetPositionInfo", "TrackURI", U2),
            Out("GetPositionInfo", "TrackMetaData", M2),
            Out("GetPositionInfo", "TrackDuration", "0:03:25"),
            Out("GetPositionInfo", "RelTime", "0:00:00"),
            // A second boundary with nothing queued is not one.
            Player(Boundary(None), false),
            Evented(""),
        ],
    );
    assert_eq!(rig.t.epoch(), epoch, "the handover is the same play");
    // From SetNextAVTransportURI on, no STOPPED and no TRANSITIONING was
    // ever queued, and no TransportState at all at the boundary.
    assert!(rig.history[before..]
        .iter()
        .all(|c| c.name != "TransportState"));
    assert_eq!(
        rig.history
            .iter()
            .filter(|c| c.name == "TransportState")
            .map(|c| c.value.as_str())
            .collect::<Vec<_>>(),
        ["STOPPED", "TRANSITIONING", "PLAYING"],
        "the only states ever evented: loading, starting, playing"
    );
    // The control point answers with the following track, and it goes on.
    rig.run(
        "gapless, the next handover",
        &[
            Does(
                "SetNextAVTransportURI",
                &[("NextURI", U3), ("NextURIMetaData", "")],
                "QueueNext(http://192.0.2.50:8000/track3.flac)",
            ),
            // The decoder knows this one's duration.
            Player(Boundary(Some(61_000)), true),
            Is("PLAYING", "OK", "Stop,Pause,Seek"),
            Out("GetMediaInfo", "CurrentURI", U3),
            Out("GetMediaInfo", "CurrentURIMetaData", ""),
            Out("GetMediaInfo", "MediaDuration", "0:01:01"),
            Player(Ended, true),
            Is("STOPPED", "OK", "Play,Seek"),
        ],
    );
    assert!(!rig.ever("TransportStatus", "ERROR_OCCURRED"));
}

#[test]
fn the_next_uri_is_replaced_and_cleared() {
    Rig::new().run(
        "replace and clear",
        &[
            // With no media there is nothing to follow.
            Refused(
                "SetNextAVTransportURI",
                &[("NextURI", U2), ("NextURIMetaData", "")],
                701,
            ),
            SET1,
            // Stored while stopped, too.
            SETNEXT2,
            Is("STOPPED", "OK", "Play,Seek,Next"),
            PLAY,
            Player(Playing, true),
            // Replace.
            Does(
                "SetNextAVTransportURI",
                &[("NextURI", U3), ("NextURIMetaData", "")],
                "QueueNext(http://192.0.2.50:8000/track3.flac)",
            ),
            Evented(
                "NextAVTransportURI=http://192.0.2.50:8000/track3.flac;NextAVTransportURIMetaData=",
            ),
            Out("GetMediaInfo", "NextURI", U3),
            // Clear.
            Does(
                "SetNextAVTransportURI",
                &[("NextURI", ""), ("NextURIMetaData", "")],
                "ClearNext",
            ),
            Evented("NextAVTransportURI=;CurrentTransportActions=Stop,Pause,Seek"),
            Is("PLAYING", "OK", "Stop,Pause,Seek"),
            // Clearing nothing is a success with nothing to do.
            Does(
                "SetNextAVTransportURI",
                &[("NextURI", ""), ("NextURIMetaData", "")],
                "",
            ),
            Evented(""),
            // With nothing queued, the end is the end.
            Player(Boundary(None), false),
            Player(Ended, true),
            Is("STOPPED", "OK", "Play,Seek"),
            // A new current URI drops a queued next one.
            SETNEXT2,
            Does(
                "SetAVTransportURI",
                &[("CurrentURI", U3), ("CurrentURIMetaData", "")],
                "Load(http://192.0.2.50:8000/track3.flac)",
            ),
            Out("GetMediaInfo", "NextURI", ""),
            Out("GetMediaInfo", "MediaDuration", "NOT_IMPLEMENTED"),
            Is("STOPPED", "OK", "Play,Seek"),
        ],
    );
}

#[test]
fn a_next_uri_that_fails_stops_after_the_current_one() {
    // The next URI turns out unplayable while the current one plays: the
    // state is kept, and the end of the current one is STOPPED with an
    // error (AVTransport:1 section 2.4.2.3).
    let mut rig = Rig::new();
    rig.run(
        "a bad next URI",
        &[
            SET1,
            PLAY,
            Player(Playing, true),
            SETNEXT2,
            Player(NextFailed, true),
            Evented("CurrentTransportActions=Stop,Pause,Seek"),
            Is("PLAYING", "OK", "Stop,Pause,Seek"),
            Out("GetMediaInfo", "NextURI", U2),
            Refused("Next", &[], 711),
            Player(Ended, true),
            Evented("TransportState=STOPPED;TransportStatus=ERROR_OCCURRED;NextAVTransportURI=;NextAVTransportURIMetaData=;CurrentTransportActions=Play,Seek"),
            Is("STOPPED", "ERROR_OCCURRED", "Play,Seek"),
            Out("GetMediaInfo", "CurrentURI", U1),
            // A new attempt clears the error.
            PLAY,
            Evented("TransportState=TRANSITIONING;TransportStatus=OK;CurrentTransportActions=Stop"),
        ],
    );
    assert_eq!(rig.t.last_failure(), None);

    // The next URI fails after the boundary: it is the current one by then.
    let mut rig = Rig::new();
    rig.run(
        "a failure after the boundary",
        &[
            SET1,
            PLAY,
            Player(Playing, true),
            SETNEXT2,
            Player(Boundary(None), true),
            Player(Failed, true),
            Evented("TransportState=STOPPED;TransportStatus=ERROR_OCCURRED;CurrentTransportActions=Play,Seek"),
            Is("STOPPED", "ERROR_OCCURRED", "Play,Seek"),
            Out("GetMediaInfo", "CurrentURI", U2),
            Out("GetPositionInfo", "RelTime", "0:00:00"),
            // The player's later reports about that play are ignored.
            Player(Playing, false),
            Player(Ended, false),
        ],
    );
    assert_eq!(rig.t.last_failure(), Some("http 404"));

    // A replaced bad next URI is good again.
    Rig::new().run(
        "a bad next URI replaced",
        &[
            SET1,
            PLAY,
            Player(Playing, true),
            SETNEXT2,
            Player(NextFailed, true),
            Does(
                "SetNextAVTransportURI",
                &[("NextURI", U3), ("NextURIMetaData", "")],
                "QueueNext(http://192.0.2.50:8000/track3.flac)",
            ),
            Is("PLAYING", "OK", "Stop,Pause,Seek,Next"),
            Player(Boundary(None), true),
            Is("PLAYING", "OK", "Stop,Pause,Seek"),
        ],
    );
}

#[test]
fn a_boundary_reported_while_paused_still_moves_the_variables() {
    let mut rig = Rig::new();
    rig.run(
        "paused at the boundary",
        &[
            SET1,
            PLAY,
            Player(Playing, true),
            SETNEXT2,
            Does("Pause", &[], "Pause"),
            Player(Boundary(None), true),
            Is("PAUSED_PLAYBACK", "OK", "Play,Stop,Seek"),
            Out("GetMediaInfo", "CurrentURI", U2),
            Out("GetMediaInfo", "NextURI", ""),
            Does("Play", &[("Speed", "1")], "Resume"),
            Is("PLAYING", "OK", "Stop,Pause,Seek"),
            // Stopped, a boundary is not one.
            SETNEXT2,
            Does("Stop", &[], "Stop"),
            Player(Boundary(None), false),
            Out("GetMediaInfo", "CurrentURI", U2),
            Out("GetMediaInfo", "NextURI", U2),
        ],
    );
    assert_eq!(rig.t.duration_ms(), Some(205_000));
    assert_eq!(rig.t.position_ms(), 0);
}

#[test]
fn a_failing_current_uri_stops_with_an_error() {
    Rig::new().run(
        "a failure",
        &[
            SET1,
            Player(Failed, true),
            Is("STOPPED", "ERROR_OCCURRED", "Play,Seek"),
            // A new URI is a clean start.
            Does(
                "SetAVTransportURI",
                &[("CurrentURI", U2), ("CurrentURIMetaData", M2)],
                "Load(http://192.0.2.50:8000/track2.flac)",
            ),
            Is("STOPPED", "OK", "Play,Seek"),
            PLAY,
            Player(Failed, true),
            Evented("TransportState=STOPPED;TransportStatus=ERROR_OCCURRED;CurrentTransportActions=Play,Seek"),
        ],
    );
}

#[test]
fn next_with_and_without_a_next_uri() {
    let mut rig = Rig::new();
    rig.run(
        "Next",
        &[
            Refused("Next", &[], 701),
            Refused("Previous", &[], 701),
            SET1,
            // One track and nothing queued: there is no track 2.
            Refused("Next", &[], 711),
            Refused("Previous", &[], 711),
            // Stopped with a next URI: it becomes the current one at once.
            SETNEXT2,
            Does("Next", &[], "Load(http://192.0.2.50:8000/track2.flac)"),
            Evented("CurrentTrackDuration=0:03:25;CurrentMediaDuration=0:03:25;CurrentTrackMetaData=<DIDL-Lite xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><item><dc:title>A &amp; B &lt;\"x\"&gt; \u{e9}</dc:title><res duration=\"0:03:25\">http://192.0.2.50:8000/track2.flac</res></item></DIDL-Lite>;CurrentTrackURI=http://192.0.2.50:8000/track2.flac;AVTransportURI=http://192.0.2.50:8000/track2.flac;AVTransportURIMetaData=<DIDL-Lite xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><item><dc:title>A &amp; B &lt;\"x\"&gt; \u{e9}</dc:title><res duration=\"0:03:25\">http://192.0.2.50:8000/track2.flac</res></item></DIDL-Lite>;NextAVTransportURI=;NextAVTransportURIMetaData=;CurrentTransportActions=Play,Seek"),
            Is("STOPPED", "OK", "Play,Seek"),
            Refused("Next", &[], 711),
            // Playing with a next URI: the player skips, and the variables
            // move when it reports the boundary; the state stays PLAYING.
            PLAY,
            Player(Playing, true),
            Does(
                "SetNextAVTransportURI",
                &[("NextURI", U3), ("NextURIMetaData", "")],
                "QueueNext(http://192.0.2.50:8000/track3.flac)",
            ),
            Does("Next", &[], "SkipToNext"),
            Evented(""),
            Is("PLAYING", "OK", "Stop,Pause,Seek,Next"),
            Out("GetMediaInfo", "CurrentURI", U2),
            Player(Boundary(Some(10_000)), true),
            Is("PLAYING", "OK", "Stop,Pause,Seek"),
            Out("GetMediaInfo", "CurrentURI", U3),
            Refused("Next", &[], 711),
            Refused("Previous", &[], 711),
            // Paused or transitioning with a next URI: 701.
            SETNEXT2,
            Does("Pause", &[], "Pause"),
            Refused("Next", &[], 701),
            Does("Play", &[("Speed", "1")], "Resume"),
            Does("Seek", &[("Unit", "REL_TIME"), ("Target", "0:00:05")], "SeekTo(5000)"),
            Refused("Next", &[], 701),
        ],
    );
    assert!(!rig.ever("TransportState", "NO_MEDIA_PRESENT"));
}

#[test]
fn every_error_code_the_state_machine_returns() {
    let mut rig = Rig::new();
    // 718 for another instance and 402 for a non-number, on every action.
    for action in description::AVTRANSPORT.actions {
        let mut args: Vec<(&str, &str)> = action.inputs().skip(1).map(|a| (a.name, "1")).collect();
        args.push(("InstanceID", "7"));
        assert_eq!(
            rig.act(action.name, &args).err(),
            Some(error::AVT_INVALID_INSTANCE_ID),
            "{}",
            action.name
        );
        *args.last_mut().unwrap() = ("InstanceID", "zero");
        assert_eq!(
            rig.act(action.name, &args).err(),
            Some(error::INVALID_ARGS),
            "{}",
            action.name
        );
        *args.last_mut().unwrap() = ("InstanceID", "-0");
        assert_eq!(rig.act(action.name, &args).err(), Some(error::INVALID_ARGS));
    }
    assert_eq!(error::AVT_INVALID_INSTANCE_ID.code, 718);
    rig.run(
        "errors",
        &[
            // 401 and 402 from the table.
            Refused("Record", &[], 401),
            Refused("SetPlayMode", &[("NewPlayMode", "NORMAL")], 401),
            Refused("Play", &[], 402),
            Refused("Seek", &[("Unit", "REL_TIME")], 402),
            Refused("SetAVTransportURI", &[("CurrentURI", U1)], 402),
            // No media: 701 for every transport-changing action but Stop.
            Refused("Play", &[("Speed", "1")], 701),
            Refused("Pause", &[], 701),
            Refused("Seek", &[("Unit", "REL_TIME"), ("Target", "0:00:01")], 701),
            Refused("Next", &[], 701),
            Refused("Previous", &[], 701),
            // 717 before the state is looked at.
            Refused("Play", &[("Speed", "2")], 717),
            Refused("Play", &[("Speed", "1/2")], 717),
            Refused("Play", &[("Speed", "")], 717),
            Refused("Play", &[("Speed", "-1")], 717),
            // 716: only http and https are fetched.
            Refused(
                "SetAVTransportURI",
                &[
                    ("CurrentURI", "file:///etc/passwd"),
                    ("CurrentURIMetaData", ""),
                ],
                716,
            ),
            Refused(
                "SetAVTransportURI",
                &[
                    ("CurrentURI", "ftp://192.0.2.50/a.flac"),
                    ("CurrentURIMetaData", ""),
                ],
                716,
            ),
            SET1,
            Refused(
                "SetNextAVTransportURI",
                &[("NextURI", "rtsp://192.0.2.50/a"), ("NextURIMetaData", "")],
                716,
            ),
            Refused("Play", &[("Speed", "2")], 717),
            // Stopped: Pause is 701.
            Refused("Pause", &[], 701),
            // 710: seek modes other than REL_TIME and TRACK_NR.
            Refused("Seek", &[("Unit", "ABS_TIME"), ("Target", "0:00:01")], 710),
            Refused("Seek", &[("Unit", "FRAME"), ("Target", "1")], 710),
            Refused("Seek", &[("Unit", "REL_COUNT"), ("Target", "1")], 710),
            Refused("Seek", &[("Unit", "rel_time"), ("Target", "0:00:01")], 710),
            Refused("Seek", &[("Unit", ""), ("Target", "")], 710),
            // 711: targets that are not times, lie past the end, or name a
            // track that is not there.
            Refused("Seek", &[("Unit", "REL_TIME"), ("Target", "soon")], 711),
            Refused("Seek", &[("Unit", "REL_TIME"), ("Target", "")], 711),
            Refused("Seek", &[("Unit", "REL_TIME"), ("Target", "0:00:04")], 711),
            Refused("Seek", &[("Unit", "REL_TIME"), ("Target", "-0:00:01")], 711),
            Refused("Seek", &[("Unit", "TRACK_NR"), ("Target", "2")], 711),
            Refused("Seek", &[("Unit", "TRACK_NR"), ("Target", "0")], 711),
            Refused("Seek", &[("Unit", "TRACK_NR"), ("Target", "x")], 711),
            Refused("Next", &[], 711),
            Refused("Previous", &[], 711),
            // Transitioning: Seek is 701.
            PLAY,
            Refused("Seek", &[("Unit", "REL_TIME"), ("Target", "0:00:01")], 701),
            Is("TRANSITIONING", "OK", "Stop"),
        ],
    );
    // The codes above are the specification's names.
    for (e, code, name) in [
        (
            error::AVT_TRANSITION_NOT_AVAILABLE,
            701,
            "Transition not available",
        ),
        (
            error::AVT_SEEK_MODE_NOT_SUPPORTED,
            710,
            "Seek mode not supported",
        ),
        (error::AVT_ILLEGAL_SEEK_TARGET, 711, "Illegal seek target"),
        (error::AVT_RESOURCE_NOT_FOUND, 716, "Resource not found"),
        (
            error::AVT_PLAY_SPEED_NOT_SUPPORTED,
            717,
            "Play speed not supported",
        ),
        (error::AVT_INVALID_INSTANCE_ID, 718, "Invalid InstanceID"),
        (error::AVT_NO_DNS_SERVER, 737, "No DNS Server"),
        (error::AVT_BAD_DOMAIN_NAME, 738, "Bad Domain Name"),
        (error::AVT_SERVER_ERROR, 739, "Server Error"),
        (error::INVALID_ACTION, 401, "Invalid Action"),
        (error::INVALID_ARGS, 402, "Invalid Args"),
    ] {
        assert_eq!((e.code, e.description), (code, name));
    }
}

#[test]
fn a_new_uri_while_playing_keeps_playing_and_an_empty_one_ejects() {
    let mut rig = Rig::new();
    rig.run(
        "replace while playing",
        &[
            SET1,
            PLAY,
            Player(Playing, true),
            Does(
                "SetAVTransportURI",
                &[("CurrentURI", U2), ("CurrentURIMetaData", M2)],
                "Load(http://192.0.2.50:8000/track2.flac)|Start",
            ),
            Is("TRANSITIONING", "OK", "Stop"),
            Player(Playing, true),
            Is("PLAYING", "OK", "Stop,Pause,Seek"),
            Out("GetMediaInfo", "CurrentURI", U2),
            // Paused: the state is kept, and Play starts the new media
            // rather than resuming the old.
            Does("Pause", &[], "Pause"),
            Does(
                "SetAVTransportURI",
                &[("CurrentURI", U1), ("CurrentURIMetaData", M1)],
                "Load(http://192.0.2.50:8000/track1.flac)",
            ),
            Is("PAUSED_PLAYBACK", "OK", "Play,Stop,Seek"),
            Out("GetPositionInfo", "RelTime", "0:00:00"),
            Does("Play", &[("Speed", "1")], "Start"),
            Is("TRANSITIONING", "OK", "Stop"),
            Player(Playing, true),
            // An empty URI clears the media.
            Does(
                "SetAVTransportURI",
                &[("CurrentURI", ""), ("CurrentURIMetaData", "")],
                "Stop",
            ),
            Evented("TransportState=NO_MEDIA_PRESENT;PlaybackStorageMedium=NONE;NumberOfTracks=0;CurrentTrack=0;CurrentTrackDuration=0:00:00;CurrentMediaDuration=0:00:00;CurrentTrackMetaData=;CurrentTrackURI=;AVTransportURI=;AVTransportURIMetaData=;CurrentTransportActions="),
            Is("NO_MEDIA_PRESENT", "OK", ""),
            Player(Playing, false),
            Does(
                "SetAVTransportURI",
                &[("CurrentURI", "  "), ("CurrentURIMetaData", "")],
                "",
            ),
            Evented(""),
        ],
    );
}

#[test]
fn reports_from_an_older_epoch_are_ignored() {
    let mut t = AvTransport::new();
    t.set_av_transport_uri(U1, M1).unwrap();
    let old = t.epoch();
    t.play("1").unwrap();
    // The control point replaces the media before the player answers.
    t.set_av_transport_uri(U2, M2).unwrap();
    assert_ne!(t.epoch(), old);
    let before = format!("{:?}", t.evented());
    assert!(!t.media_opened(old, Some(1), false));
    assert!(!t.playing(old));
    assert!(!t.track_boundary(old, None, true));
    assert!(!t.next_failed(old, "x"));
    assert!(t.ended(old).is_empty());
    assert!(!t.failed(old, "x"));
    assert_eq!(format!("{:?}", t.evented()), before);
    assert_eq!(t.state().as_str(), "TRANSITIONING");
    assert!(t.playing(t.epoch()));
    // Stop starts a new epoch as well.
    let playing = t.epoch();
    t.stop().unwrap();
    assert!(!t.playing(playing));
    assert!(t.ended(playing).is_empty());
    assert_eq!(t.state().as_str(), "STOPPED");
}

#[test]
fn a_stream_with_no_end_cannot_be_sought() {
    Rig::new().run(
        "a live stream",
        &[
            Does(
                "SetAVTransportURI",
                &[
                    ("CurrentURI", "https://radio.example/stream"),
                    ("CurrentURIMetaData", ""),
                ],
                "Load(https://radio.example/stream)",
            ),
            Out("GetMediaInfo", "MediaDuration", "NOT_IMPLEMENTED"),
            Player(Opened(None, false), true),
            Evented("CurrentTransportActions=Play"),
            Is("STOPPED", "OK", "Play"),
            Refused("Seek", &[("Unit", "REL_TIME"), ("Target", "0:00:01")], 710),
            PLAY,
            Player(Playing, true),
            Is("PLAYING", "OK", "Stop,Pause"),
            Out("GetPositionInfo", "TrackDuration", "NOT_IMPLEMENTED"),
            Player(Position(7_200_000), true),
            Out("GetPositionInfo", "RelTime", "2:00:00"),
            // TRACK_NR 1 is the required mode and restarts the stream.
            Does(
                "Seek",
                &[("Unit", "TRACK_NR"), ("Target", "1")],
                "SeekTo(0)",
            ),
        ],
    );
}

#[test]
fn a_good_next_uri_that_was_not_joined_follows_through_transitioning() {
    let mut t = AvTransport::new();
    t.set_av_transport_uri(U1, M1).unwrap();
    t.play("1").unwrap();
    assert!(t.playing(t.epoch()));
    t.set_next_av_transport_uri(U2, M2).unwrap();
    let epoch = t.epoch();
    // The player ran out of the current track before the next was ready.
    let effects = t.ended(epoch);
    assert_eq!(
        effects,
        [
            Effect::Load {
                uri: U2.into(),
                metadata: M2.into()
            },
            Effect::Start
        ]
    );
    assert_eq!(t.state().as_str(), "TRANSITIONING");
    assert_eq!(t.current(), (U2, M2));
    assert_eq!(t.next_queued(), ("", ""));
    assert_ne!(t.epoch(), epoch);
    assert!(t.playing(t.epoch()));
    assert_eq!(t.state().as_str(), "PLAYING");
}

#[test]
fn the_initial_event_holds_every_evented_variable_and_no_position() {
    let mut t = AvTransport::new();
    t.set_av_transport_uri(U1, M1).unwrap();
    let evented: Vec<&str> = t.evented().iter().map(|c| c.name).collect();
    let table: Vec<&str> = description::AVTRANSPORT
        .variables
        .iter()
        .map(|v| v.name)
        .filter(|n| {
            !n.starts_with("A_ARG_TYPE_")
                && *n != "LastChange"
                && !chorus_upnp::lastchange::NEVER_EVENTED.contains(n)
        })
        .collect();
    assert_eq!(
        evented, table,
        "the state table's order, positions left out"
    );
    assert_eq!(evented.len(), 22);
}

/// Moderation and event keys together, as the server will drive them: 50
/// volume changes in 100 ms, two subscribers, a 1 ms timer.
#[test]
fn fifty_changes_in_100_ms_are_moderated_and_keys_go_up_by_one() {
    let mut rcs = RenderingControl::new(0, false);
    let mut subs = Subscriptions::standard();
    let callback = |port: u16| {
        vec![gena::parse_callback_url(&format!("http://192.0.2.50:{port}/cb")).unwrap()]
    };
    for (i, port) in [49_152u16, 49_153].into_iter().enumerate() {
        let sid = gena::sid_from_random([i as u8 + 1; 16]);
        subs.subscribe(sid.clone(), callback(port), None, 0)
            .unwrap();
        assert_eq!(subs.initial(&sid).unwrap().seq, 0);
    }
    // (time, sid, seq, volume in the event)
    let mut sent: Vec<(u64, String, u32, String)> = Vec::new();
    for now in 0..1000u64 {
        if now < 100 && now % 2 == 0 {
            let volume = (now / 2 + 1) as u16;
            let call = client::soap_request(
                Service::RenderingControl,
                "SetVolume",
                &[
                    ("InstanceID", "0"),
                    ("Channel", "Master"),
                    ("DesiredVolume", &volume.to_string()),
                ],
            )
            .unwrap();
            let request = soap::parse_request(Some(&call.soapaction), &call.body).unwrap();
            let invocation = soap::validate(Service::RenderingControl, &request).unwrap();
            let (_, effects) = rcs.invoke(&invocation).unwrap();
            // The server applies the request and reports what holds.
            let [rendering::Effect::SetVolume { thousandths }] = effects[..] else {
                panic!("one SetVolume effect")
            };
            rcs.report(thousandths, false);
        }
        if let Some(changes) = rcs.events().take(now) {
            let event = event_xml(RCS_NS, &changes);
            let body = gena::propertyset(&[("LastChange", &event)]);
            for notify in subs.event(now) {
                let request = notify.request(&notify.callbacks[0], &body);
                let (_, sent_body) = request.split_once("\r\n\r\n").unwrap();
                let props = client::parse_propertyset(sent_body).unwrap();
                let read = client::parse_last_change(&props[0].1).unwrap();
                assert_eq!(read.changes.len(), 1, "one entry per variable");
                assert_eq!(read.changes[0].channel.as_deref(), Some("Master"));
                sent.push((
                    now,
                    notify.sid,
                    notify.seq,
                    read.get("Volume").unwrap().to_string(),
                ));
            }
        }
    }
    let first_sid = sent[0].1.clone();
    let one: Vec<&(u64, String, u32, String)> = sent.iter().filter(|s| s.1 == first_sid).collect();
    // At most one event per 200 ms; the 50 changes made two.
    assert_eq!(one.len(), 2);
    assert!(one.windows(2).all(|w| w[1].0 - w[0].0 >= MODERATION_MS));
    assert_eq!((one[0].0, one[1].0), (0, 200));
    // The last holds the final value.
    assert_eq!(one.last().unwrap().3, "50");
    assert_eq!(rcs.volume(), 50);
    // Every subscriber's key goes up by exactly one per message, from the
    // initial event's 0.
    for sid in [
        gena::sid_from_random([1; 16]),
        gena::sid_from_random([2; 16]),
    ] {
        let keys: Vec<u32> = sent.iter().filter(|s| s.1 == sid).map(|s| s.2).collect();
        assert_eq!(keys, [1, 2], "{sid}");
    }
}
