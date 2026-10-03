//! AVTransport:1: the state machine.
//!
//! AVT1. [`AvTransport`] holds every AVTransport state variable of one
//! renderer. Its inputs are of two kinds and it has no others:
//!
//! - **actions** from control points, one method each (and
//!   [`AvTransport::invoke`] for a validated SOAP request), which return the
//!   out arguments and a list of [`Effect`]s: what the server must now do
//!   with its player;
//! - **reports** from the server's player ([`AvTransport::media_opened`],
//!   [`AvTransport::playing`], [`AvTransport::track_boundary`],
//!   [`AvTransport::next_failed`], [`AvTransport::ended`],
//!   [`AvTransport::failed`], [`AvTransport::position`]).
//!
//! An action never waits for the player: UDA11 section 3.2.1 gives an action
//! 30 seconds, and fetching media can take longer, so `SetAVTransportURI`
//! returns at once and a failure arrives later as a report (TransportState
//! STOPPED, TransportStatus ERROR_OCCURRED).
//!
//! Every change of an evented variable is recorded in the moderation queue
//! ([`AvTransport::events`]) by comparing the variables before and after
//! each input, so no path can change a variable without eventing it.
//!
//! Only five transport states are used: NO_MEDIA_PRESENT, STOPPED,
//! TRANSITIONING, PLAYING and PAUSED_PLAYBACK. chorus does not record.
//!
//! ## Reports and the epoch
//!
//! The player runs on its own thread, so a report can arrive after the
//! control point has moved on (an "ended" for a track that was just replaced).
//! [`AvTransport::epoch`] is a counter that goes up whenever what is loaded
//! or whether it runs is decided anew (a new URI, a Stop, a promoted next
//! URI). The server tags what it asks of the player with the epoch it read
//! after the action, the player's reports carry that tag back, and a report
//! from an older epoch is ignored.

use crate::didl;
use crate::lastchange::{Change, Moderator};
use crate::soap::Invocation;
use crate::time;
use crate::{error, instance_is_zero, Outputs, UpnpError};

/// TransportState (AVT1 section 2.2.1), without the two recording states.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportState {
    /// No URI is set.
    NoMediaPresent,
    /// A URI is set and nothing plays.
    Stopped,
    /// On the way to PLAYING: the media is being fetched or sought.
    Transitioning,
    /// Audio is playing.
    Playing,
    /// Paused inside the media.
    PausedPlayback,
}

impl TransportState {
    /// The value as the specification spells it.
    pub fn as_str(self) -> &'static str {
        match self {
            TransportState::NoMediaPresent => "NO_MEDIA_PRESENT",
            TransportState::Stopped => "STOPPED",
            TransportState::Transitioning => "TRANSITIONING",
            TransportState::Playing => "PLAYING",
            TransportState::PausedPlayback => "PAUSED_PLAYBACK",
        }
    }
}

/// TransportStatus (AVT1 section 2.2.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportStatus {
    /// `OK`.
    Ok,
    /// `ERROR_OCCURRED`: the last attempt to fetch or decode failed.
    ErrorOccurred,
}

impl TransportStatus {
    /// The value as the specification spells it.
    pub fn as_str(self) -> &'static str {
        match self {
            TransportStatus::Ok => "OK",
            TransportStatus::ErrorOccurred => "ERROR_OCCURRED",
        }
    }
}

/// What the server must do with its player after an input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Drop whatever is loaded or queued and open this URI (so its duration
    /// and its faults are known early) without starting it.
    Load {
        /// AVTransportURI.
        uri: String,
        /// Its metadata, as sent.
        metadata: String,
    },
    /// Play the current URI from the current position (0 after a Stop or a
    /// Load, or where a Seek put it), opening it if it is not open. A next
    /// URI that was queued and not cleared still follows it. Report
    /// [`AvTransport::playing`] when the first audio is out.
    Start,
    /// Hold the audio where it is.
    Pause,
    /// Carry on from where the audio was held.
    Resume,
    /// Stop the audio and return to the start of the media.
    Stop,
    /// Move to this position in the current media. When playing, report
    /// [`AvTransport::playing`] once audio is out again from there.
    SeekTo {
        /// The target, in milliseconds from the start.
        ms: u64,
    },
    /// Fetch and decode this URI ahead so it can follow the current one
    /// without a gap, replacing any next URI queued before.
    QueueNext {
        /// NextAVTransportURI.
        uri: String,
        /// Its metadata, as sent.
        metadata: String,
    },
    /// Forget the queued next URI.
    ClearNext,
    /// Go to the queued next URI now; report
    /// [`AvTransport::track_boundary`] when its first audio is out.
    SkipToNext,
}

/// The seek modes offered (AVT1 section 2.2.28): the required `TRACK_NR`
/// and `REL_TIME`.
const SEEK_TRACK_NR: &str = "TRACK_NR";
const SEEK_REL_TIME: &str = "REL_TIME";

/// The AVTransport state of one renderer. See the module documentation.
#[derive(Clone, Debug)]
pub struct AvTransport {
    state: TransportState,
    status: TransportStatus,
    uri: String,
    metadata: String,
    next_uri: String,
    next_metadata: String,
    duration_ms: Option<u64>,
    seekable: bool,
    position_ms: u64,
    /// Paused inside a track that was started: Play resumes instead of
    /// starting.
    held: bool,
    next_is_bad: bool,
    last_failure: Option<String>,
    epoch: u64,
    evented_before: Vec<String>,
    events: Moderator,
}

impl Default for AvTransport {
    fn default() -> AvTransport {
        AvTransport::new()
    }
}

fn is_http(uri: &str) -> bool {
    let lower = uri.get(..8).unwrap_or(uri).to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

impl AvTransport {
    /// A transport with no media.
    pub fn new() -> AvTransport {
        let mut t = AvTransport {
            state: TransportState::NoMediaPresent,
            status: TransportStatus::Ok,
            uri: String::new(),
            metadata: String::new(),
            next_uri: String::new(),
            next_metadata: String::new(),
            duration_ms: None,
            seekable: true,
            position_ms: 0,
            held: false,
            next_is_bad: false,
            last_failure: None,
            epoch: 0,
            evented_before: Vec::new(),
            events: Moderator::new(),
        };
        t.evented_before = t.evented().into_iter().map(|c| c.value).collect();
        t
    }

    /// TransportState.
    pub fn state(&self) -> TransportState {
        self.state
    }

    /// TransportStatus.
    pub fn status(&self) -> TransportStatus {
        self.status
    }

    /// AVTransportURI and its metadata.
    pub fn current(&self) -> (&str, &str) {
        (&self.uri, &self.metadata)
    }

    /// NextAVTransportURI and its metadata; both empty when none is set.
    pub fn next_queued(&self) -> (&str, &str) {
        (&self.next_uri, &self.next_metadata)
    }

    /// What the player last said went wrong, if the last thing that happened
    /// was a failure.
    pub fn last_failure(&self) -> Option<&str> {
        self.last_failure.as_deref()
    }

    /// The epoch reports must carry (see the module documentation).
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The moderation queue the changes are recorded in; the server takes
    /// events from it ([`Moderator::take`]).
    pub fn events(&mut self) -> &mut Moderator {
        &mut self.events
    }

    fn has_media(&self) -> bool {
        self.state != TransportState::NoMediaPresent
    }

    fn duration_text(&self) -> String {
        if self.has_media() {
            // AVT1 section 2.2.14: NOT_IMPLEMENTED when the duration is not
            // known (a live stream).
            time::format_or_not_implemented(self.duration_ms)
        } else {
            time::format(0)
        }
    }

    fn can_go_next(&self) -> bool {
        !self.next_uri.is_empty() && !self.next_is_bad
    }

    /// CurrentTransportActions (AVT1 section 2.2.26): the "transport-
    /// controlling actions that can be successfully invoked for the current
    /// resource at this specific point in time", comma-separated without
    /// spaces. `Seek` is listed only for seekable media and `Next` only when
    /// a next URI is queued; `Previous` never (there is no earlier track).
    pub fn transport_actions(&self) -> String {
        let mut actions: Vec<&str> = match self.state {
            TransportState::NoMediaPresent => vec![],
            TransportState::Stopped => vec!["Play"],
            TransportState::Transitioning => vec!["Stop"],
            TransportState::Playing => vec!["Stop", "Pause"],
            TransportState::PausedPlayback => vec!["Play", "Stop"],
        };
        let settled = !matches!(
            self.state,
            TransportState::NoMediaPresent | TransportState::Transitioning
        );
        if settled && self.seekable {
            actions.push("Seek");
        }
        if matches!(
            self.state,
            TransportState::Stopped | TransportState::Playing
        ) && self.can_go_next()
        {
            actions.push("Next");
        }
        actions.join(",")
    }

    /// Every indirectly evented variable with its current value, in the
    /// state table's order: the content of the initial event of a
    /// subscription (RCS1 section 2.3.1, which AVT1 section 2.3.1 refers
    /// to). The four position variables are not among them (AVT1 section
    /// 2.3.1), nor are the `A_ARG_TYPE_` ones.
    pub fn evented(&self) -> Vec<Change> {
        let media = self.has_media();
        let tracks = if media { "1" } else { "0" };
        vec![
            Change::new("TransportState", self.state.as_str()),
            Change::new("TransportStatus", self.status.as_str()),
            // AVT1 section 2.2.3: NONE with no resource, NETWORK for one
            // from the network.
            Change::new(
                "PlaybackStorageMedium",
                if media { "NETWORK" } else { "NONE" },
            ),
            Change::new("RecordStorageMedium", time::NOT_IMPLEMENTED),
            Change::new("PossiblePlaybackStorageMedia", "NETWORK"),
            Change::new("PossibleRecordStorageMedia", time::NOT_IMPLEMENTED),
            Change::new("CurrentPlayMode", "NORMAL"),
            Change::new("TransportPlaySpeed", "1"),
            Change::new("RecordMediumWriteStatus", time::NOT_IMPLEMENTED),
            Change::new("CurrentRecordQualityMode", time::NOT_IMPLEMENTED),
            Change::new("PossibleRecordQualityModes", time::NOT_IMPLEMENTED),
            Change::new("NumberOfTracks", tracks),
            Change::new("CurrentTrack", tracks),
            Change::new("CurrentTrackDuration", self.duration_text()),
            Change::new("CurrentMediaDuration", self.duration_text()),
            Change::new("CurrentTrackMetaData", self.metadata.clone()),
            Change::new("CurrentTrackURI", self.uri.clone()),
            Change::new("AVTransportURI", self.uri.clone()),
            Change::new("AVTransportURIMetaData", self.metadata.clone()),
            Change::new("NextAVTransportURI", self.next_uri.clone()),
            Change::new("NextAVTransportURIMetaData", self.next_metadata.clone()),
            Change::new("CurrentTransportActions", self.transport_actions()),
        ]
    }

    /// Records every evented variable whose value differs from what it was
    /// after the previous input.
    fn commit(&mut self) {
        let now = self.evented();
        for (change, before) in now.iter().zip(&self.evented_before) {
            if change.value != *before {
                self.events.record(change.clone());
            }
        }
        self.evented_before = now.into_iter().map(|c| c.value).collect();
    }

    fn duration_hint(uri: &str, metadata: &str) -> Option<u64> {
        didl::parse(metadata).and_then(|m| m.duration_for(uri))
    }

    /// Makes `uri` the current media, not started.
    fn load(&mut self, uri: &str, metadata: &str) {
        self.uri = uri.to_string();
        self.metadata = metadata.to_string();
        self.duration_ms = Self::duration_hint(uri, metadata);
        self.seekable = true;
        self.position_ms = 0;
        self.held = false;
        self.status = TransportStatus::Ok;
        self.last_failure = None;
        self.epoch += 1;
    }

    fn clear_next(&mut self) {
        self.next_uri.clear();
        self.next_metadata.clear();
        self.next_is_bad = false;
    }

    // ----- actions -----

    /// SetAVTransportURI (AVT1 section 2.4.1). The metadata is stored
    /// exactly as given and never has to parse.
    ///
    /// State (section 2.4.1.3): from NO_MEDIA_PRESENT to STOPPED; from
    /// PLAYING the new URI starts playing, through TRANSITIONING ("allowed
    /// to temporarily go to the 'TRANSITIONING' state before going back to
    /// 'PLAYING'"); "In all other cases, this action does not change the
    /// transport state".
    ///
    /// chorus's own rules, where the specification is silent:
    /// - an empty URI clears the media: NO_MEDIA_PRESENT (how control points
    ///   "eject");
    /// - a URI that is not `http://` or `https://` is refused with 716
    ///   Resource not found and changes nothing: chorus fetches nothing else;
    /// - the next URI is cleared: it was queued to follow the media this
    ///   replaces.
    pub fn set_av_transport_uri(
        &mut self,
        uri: &str,
        metadata: &str,
    ) -> Result<Vec<Effect>, UpnpError> {
        let uri = uri.trim();
        if uri.is_empty() {
            let was_active = self.has_media();
            self.load("", "");
            self.duration_ms = None;
            self.clear_next();
            self.state = TransportState::NoMediaPresent;
            self.commit();
            return Ok(if was_active {
                vec![Effect::Stop]
            } else {
                vec![]
            });
        }
        if !is_http(uri) {
            return Err(error::AVT_RESOURCE_NOT_FOUND);
        }
        let was_playing = matches!(
            self.state,
            TransportState::Playing | TransportState::Transitioning
        );
        self.load(uri, metadata);
        self.clear_next();
        let mut effects = vec![Effect::Load {
            uri: uri.to_string(),
            metadata: metadata.to_string(),
        }];
        if was_playing {
            self.state = TransportState::Transitioning;
            effects.push(Effect::Start);
        } else if self.state == TransportState::NoMediaPresent {
            self.state = TransportState::Stopped;
        }
        self.commit();
        Ok(effects)
    }

    /// SetNextAVTransportURI (AVT1 section 2.4.2): "the URI of the resource
    /// to be controlled when the playback of the current resource ...
    /// finishes", so the device can "'prefetch' the data to be played next,
    /// in order to provide a seamless transition". "This action does not
    /// change the transport state" (section 2.4.2.3).
    ///
    /// A second call replaces the first, at any time up to the handover. An
    /// empty URI clears the next URI. chorus's own rules: with no current
    /// media there is nothing to follow, 701 Transition not available; a URI
    /// that is not `http://` or `https://` is 716.
    pub fn set_next_av_transport_uri(
        &mut self,
        uri: &str,
        metadata: &str,
    ) -> Result<Vec<Effect>, UpnpError> {
        if !self.has_media() {
            return Err(error::AVT_TRANSITION_NOT_AVAILABLE);
        }
        let uri = uri.trim();
        if uri.is_empty() {
            let had = !self.next_uri.is_empty();
            self.clear_next();
            self.commit();
            return Ok(if had { vec![Effect::ClearNext] } else { vec![] });
        }
        if !is_http(uri) {
            return Err(error::AVT_RESOURCE_NOT_FOUND);
        }
        self.next_uri = uri.to_string();
        self.next_metadata = metadata.to_string();
        self.next_is_bad = false;
        self.commit();
        Ok(vec![Effect::QueueNext {
            uri: uri.to_string(),
            metadata: metadata.to_string(),
        }])
    }

    /// GetMediaInfo (AVT1 section 2.4.3). The metadata comes back exactly as
    /// it was given.
    pub fn get_media_info(&self) -> Outputs {
        let media = self.has_media();
        vec![
            ("NrTracks", if media { "1" } else { "0" }.to_string()),
            ("MediaDuration", self.duration_text()),
            ("CurrentURI", self.uri.clone()),
            ("CurrentURIMetaData", self.metadata.clone()),
            ("NextURI", self.next_uri.clone()),
            ("NextURIMetaData", self.next_metadata.clone()),
            (
                "PlayMedium",
                if media { "NETWORK" } else { "NONE" }.to_string(),
            ),
            ("RecordMedium", time::NOT_IMPLEMENTED.to_string()),
            ("WriteStatus", time::NOT_IMPLEMENTED.to_string()),
        ]
    }

    /// GetTransportInfo (AVT1 section 2.4.4).
    pub fn get_transport_info(&self) -> Outputs {
        vec![
            ("CurrentTransportState", self.state.as_str().to_string()),
            ("CurrentTransportStatus", self.status.as_str().to_string()),
            ("CurrentSpeed", "1".to_string()),
        ]
    }

    /// GetPositionInfo (AVT1 section 2.4.5). `RelTime` is the position the
    /// server last gave ([`AvTransport::position`]); with one track per
    /// media the absolute position is the same. The counters are not
    /// supported and hold "the maximum value of the i4 data type" (AVT1
    /// section 2.2.25).
    pub fn get_position_info(&self) -> Outputs {
        let media = self.has_media();
        let position = time::format(self.position_ms);
        vec![
            ("Track", if media { "1" } else { "0" }.to_string()),
            ("TrackDuration", self.duration_text()),
            ("TrackMetaData", self.metadata.clone()),
            ("TrackURI", self.uri.clone()),
            ("RelTime", position.clone()),
            ("AbsTime", position),
            ("RelCount", time::NOT_IMPLEMENTED_I4.to_string()),
            ("AbsCount", time::NOT_IMPLEMENTED_I4.to_string()),
        ]
    }

    /// GetDeviceCapabilities (AVT1 section 2.4.6): network media, no
    /// recording.
    pub fn get_device_capabilities(&self) -> Outputs {
        vec![
            ("PlayMedia", "NETWORK".to_string()),
            ("RecMedia", time::NOT_IMPLEMENTED.to_string()),
            ("RecQualityModes", time::NOT_IMPLEMENTED.to_string()),
        ]
    }

    /// GetTransportSettings (AVT1 section 2.4.7).
    pub fn get_transport_settings(&self) -> Outputs {
        vec![
            ("PlayMode", "NORMAL".to_string()),
            ("RecQualityMode", time::NOT_IMPLEMENTED.to_string()),
        ]
    }

    /// GetCurrentTransportActions (AVT1 section 2.4.17).
    pub fn get_current_transport_actions(&self) -> Outputs {
        vec![("Actions", self.transport_actions())]
    }

    /// Stop (AVT1 section 2.4.8): "Changes TransportState to 'STOPPED'"; the
    /// position returns to the start and the URI stays.
    ///
    /// The specification allows Stop "in all transport states except in
    /// state 'NO_MEDIA_PRESENT'". chorus answers a Stop with no media with
    /// success and no change rather than 701: control points send Stop
    /// before every SetAVTransportURI as a matter of course, and an error
    /// there breaks them (chorus's tolerance, not the specification's text).
    pub fn stop(&mut self) -> Result<Vec<Effect>, UpnpError> {
        match self.state {
            TransportState::NoMediaPresent | TransportState::Stopped => Ok(vec![]),
            _ => {
                self.state = TransportState::Stopped;
                self.position_ms = 0;
                self.held = false;
                self.epoch += 1;
                self.commit();
                Ok(vec![Effect::Stop])
            }
        }
    }

    /// Play (AVT1 section 2.4.9): "allowed in the 'STOPPED', 'PLAYING', and
    /// 'PAUSED_PLAYBACK' transport states"; it "Changes TransportState to
    /// 'PLAYING'", optionally through TRANSITIONING.
    ///
    /// - `Speed` other than `1`: 717 Play speed not supported (the SCPD
    ///   lists only "1");
    /// - no media: 701 Transition not available;
    /// - STOPPED: TRANSITIONING until the player reports
    ///   [`AvTransport::playing`]; a TransportStatus of ERROR_OCCURRED from
    ///   an earlier failure returns to OK, since this is a new attempt;
    /// - PAUSED_PLAYBACK: PLAYING at once (the audio is held, not gone);
    /// - PLAYING or TRANSITIONING: success, no change (control points send
    ///   Play while the renderer is still getting there).
    pub fn play(&mut self, speed: &str) -> Result<Vec<Effect>, UpnpError> {
        if speed.trim() != "1" {
            return Err(error::AVT_PLAY_SPEED_NOT_SUPPORTED);
        }
        let effects = match self.state {
            TransportState::NoMediaPresent => return Err(error::AVT_TRANSITION_NOT_AVAILABLE),
            TransportState::Playing | TransportState::Transitioning => vec![],
            TransportState::PausedPlayback if self.held => {
                self.state = TransportState::Playing;
                self.held = false;
                vec![Effect::Resume]
            }
            TransportState::Stopped | TransportState::PausedPlayback => {
                self.state = TransportState::Transitioning;
                self.status = TransportStatus::Ok;
                self.last_failure = None;
                vec![Effect::Start]
            }
        };
        self.commit();
        Ok(effects)
    }

    /// Pause (AVT1 section 2.4.10): "always allowed while playing"; "In
    /// other cases, the action may fail with error code 701"; "does not
    /// operate as a toggle". From PLAYING, and from TRANSITIONING (the
    /// player holds as soon as it has audio), to PAUSED_PLAYBACK; already
    /// paused is a success with no change; STOPPED and no media are 701.
    pub fn pause(&mut self) -> Result<Vec<Effect>, UpnpError> {
        match self.state {
            TransportState::NoMediaPresent | TransportState::Stopped => {
                Err(error::AVT_TRANSITION_NOT_AVAILABLE)
            }
            TransportState::PausedPlayback => Ok(vec![]),
            TransportState::Playing | TransportState::Transitioning => {
                self.state = TransportState::PausedPlayback;
                self.held = true;
                self.commit();
                Ok(vec![Effect::Pause])
            }
        }
    }

    /// Seek (AVT1 section 2.4.12).
    ///
    /// - `Unit` other than `REL_TIME` and `TRACK_NR` (the two the SCPD
    ///   lists; section 2.2.28: "Only value 'TRACK_NR' is required"): 710
    ///   Seek mode not supported;
    /// - a `REL_TIME` target that is not a time, or lies past a known
    ///   duration; a `TRACK_NR` target other than `1` (the one track; `1`
    ///   seeks to its start): 711 Illegal seek target;
    /// - media that cannot be sought (a live stream): 710 for `REL_TIME`;
    /// - no media, or TRANSITIONING: 701 (section 2.4.12.2: allowed in
    ///   STOPPED and PLAYING, "in other states the action may fail with
    ///   error code 701"; chorus also allows it in PAUSED_PLAYBACK).
    ///
    /// Effect on state (section 2.4.12.3): "Changes TransportState to
    /// 'TRANSITIONING' and then returns immediately. When the desired
    /// position is reached, TransportState will return to the previous
    /// transport state". From PLAYING that is what happens here, with
    /// [`AvTransport::playing`] as the return. From STOPPED and
    /// PAUSED_PLAYBACK the position is simply set: there is no audio to wait
    /// for, and a TRANSITIONING that ended in the same instant would be
    /// folded away by the event moderation anyway.
    pub fn seek(&mut self, unit: &str, target: &str) -> Result<Vec<Effect>, UpnpError> {
        let unit = unit.trim();
        if unit != SEEK_REL_TIME && unit != SEEK_TRACK_NR {
            return Err(error::AVT_SEEK_MODE_NOT_SUPPORTED);
        }
        if matches!(
            self.state,
            TransportState::NoMediaPresent | TransportState::Transitioning
        ) {
            return Err(error::AVT_TRANSITION_NOT_AVAILABLE);
        }
        let ms = if unit == SEEK_TRACK_NR {
            if target.trim() != "1" {
                return Err(error::AVT_ILLEGAL_SEEK_TARGET);
            }
            0
        } else {
            if !self.seekable {
                return Err(error::AVT_SEEK_MODE_NOT_SUPPORTED);
            }
            let ms = time::parse(target).ok_or(error::AVT_ILLEGAL_SEEK_TARGET)?;
            if self.duration_ms.is_some_and(|d| ms > d) {
                return Err(error::AVT_ILLEGAL_SEEK_TARGET);
            }
            ms
        };
        self.position_ms = ms;
        if self.state == TransportState::Playing {
            self.state = TransportState::Transitioning;
        }
        self.commit();
        Ok(vec![Effect::SeekTo { ms }])
    }

    /// Next (AVT1 section 2.4.13): "functionally equivalent to
    /// Seek(TRACK_NR,CurrentTrackNr+1). This action does not 'cycle' back to
    /// the first track."
    ///
    /// The media has one track, so without a next URI there is no track 2:
    /// 711 Illegal seek target. With a next URI queued, chorus treats it as
    /// the following track (chorus's reading; the specification does not
    /// connect the two):
    /// - PLAYING: the player skips to it now, and the variables change when
    ///   it reports [`AvTransport::track_boundary`]; the state stays
    ///   PLAYING, as at a gapless handover;
    /// - STOPPED: the next URI becomes the current one at once, still
    ///   STOPPED.
    ///
    /// No media: 701. PAUSED_PLAYBACK and TRANSITIONING: 701 (section
    /// 2.4.13.2: "allowed in the STOPPED and PLAYING transport states").
    pub fn next(&mut self) -> Result<Vec<Effect>, UpnpError> {
        if !self.has_media() {
            return Err(error::AVT_TRANSITION_NOT_AVAILABLE);
        }
        if !self.can_go_next() {
            return Err(error::AVT_ILLEGAL_SEEK_TARGET);
        }
        match self.state {
            TransportState::Playing => Ok(vec![Effect::SkipToNext]),
            TransportState::Stopped => {
                let (uri, metadata) = (self.next_uri.clone(), self.next_metadata.clone());
                self.load(&uri, &metadata);
                self.clear_next();
                self.commit();
                Ok(vec![Effect::Load { uri, metadata }])
            }
            _ => Err(error::AVT_TRANSITION_NOT_AVAILABLE),
        }
    }

    /// Previous (AVT1 section 2.4.14): "functionally equivalent to
    /// Seek(TRACK_NR,CurrentTrackNr-1) ... does not 'cycle' back to the last
    /// track." There is one track and nothing before it: 711 Illegal seek
    /// target, or 701 with no media. The action is in the SCPD because the
    /// specification requires it.
    pub fn previous(&mut self) -> Result<Vec<Effect>, UpnpError> {
        if !self.has_media() {
            return Err(error::AVT_TRANSITION_NOT_AVAILABLE);
        }
        Err(error::AVT_ILLEGAL_SEEK_TARGET)
    }

    /// Performs an AVTransport action that passed [`crate::soap::validate`].
    /// `InstanceID` other than 0 is 718 Invalid InstanceID (AVT1 section
    /// 2.4.1.4), checked before anything else; an action that fails leaves
    /// the state as it was.
    pub fn invoke(&mut self, invocation: &Invocation) -> Result<(Outputs, Vec<Effect>), UpnpError> {
        if !instance_is_zero(invocation.input("InstanceID"))? {
            return Err(error::AVT_INVALID_INSTANCE_ID);
        }
        let done = |effects: Vec<Effect>| (Vec::new(), effects);
        Ok(match invocation.action.name {
            "SetAVTransportURI" => done(self.set_av_transport_uri(
                invocation.input("CurrentURI"),
                invocation.input("CurrentURIMetaData"),
            )?),
            "SetNextAVTransportURI" => done(self.set_next_av_transport_uri(
                invocation.input("NextURI"),
                invocation.input("NextURIMetaData"),
            )?),
            "GetMediaInfo" => (self.get_media_info(), vec![]),
            "GetTransportInfo" => (self.get_transport_info(), vec![]),
            "GetPositionInfo" => (self.get_position_info(), vec![]),
            "GetDeviceCapabilities" => (self.get_device_capabilities(), vec![]),
            "GetTransportSettings" => (self.get_transport_settings(), vec![]),
            "Stop" => done(self.stop()?),
            "Play" => done(self.play(invocation.input("Speed"))?),
            "Pause" => done(self.pause()?),
            "Seek" => done(self.seek(invocation.input("Unit"), invocation.input("Target"))?),
            "Next" => done(self.next()?),
            "Previous" => done(self.previous()?),
            "GetCurrentTransportActions" => (self.get_current_transport_actions(), vec![]),
            _ => return Err(error::INVALID_ACTION),
        })
    }

    // ----- reports from the player -----

    fn current_epoch(&self, epoch: u64) -> bool {
        epoch == self.epoch && self.has_media()
    }

    /// The played position in the current media, in milliseconds: what is
    /// audible now, from the server's playout timeline (not what has been
    /// fetched or decoded). The server gives it before it answers
    /// GetPositionInfo and when it pauses. Never evented (AVT1 section
    /// 2.3.1). Ignored while STOPPED or with no media, where the position is
    /// 0 by definition.
    pub fn position(&mut self, played_ms: u64) {
        if matches!(
            self.state,
            TransportState::Playing
                | TransportState::Transitioning
                | TransportState::PausedPlayback
        ) {
            self.position_ms = self.duration_ms.map_or(played_ms, |d| played_ms.min(d));
        }
    }

    /// The current media was opened: its real duration, when the decoder
    /// knows one, and whether it can be sought. A known duration replaces
    /// what the control point's metadata claimed; an unknown one (`None`)
    /// leaves that claim standing, as the best there is.
    pub fn media_opened(&mut self, epoch: u64, duration_ms: Option<u64>, seekable: bool) -> bool {
        if !self.current_epoch(epoch) {
            return false;
        }
        self.duration_ms = duration_ms.or(self.duration_ms);
        self.seekable = seekable;
        self.commit();
        true
    }

    /// Audio of the current media is out: TRANSITIONING becomes PLAYING.
    /// In any other state it changes nothing (a Pause or a Stop got there
    /// first).
    pub fn playing(&mut self, epoch: u64) -> bool {
        if !self.current_epoch(epoch) || self.state != TransportState::Transitioning {
            return false;
        }
        self.state = TransportState::Playing;
        self.commit();
        true
    }

    /// The first audio of the next URI is out: the gapless handover.
    ///
    /// What the specification says (AVT1 section 2.4.2.3, repeated nearly
    /// word for word in AVT3 section 5.4.3.3): "when the playback of the
    /// current resource finishes, state variable AVTransportURI changes to
    /// the value of state variable NextAVTransportURI. The same holds for
    /// AVTransportURIMetaData and NextAVTransportURI MetaData. ... In such
    /// case, the state variable NextAVTransportURI will be set to NULL
    /// (empty string)."
    ///
    /// So here: AVTransportURI and CurrentTrackURI take the next URI,
    /// AVTransportURIMetaData and CurrentTrackMetaData its metadata;
    /// NextAVTransportURI and NextAVTransportURIMetaData become empty; the
    /// durations become the new track's (the given one, else what its
    /// metadata claimed); the position restarts at 0.
    ///
    /// **TransportState stays PLAYING, with no TRANSITIONING and no STOPPED
    /// in between. That is an inference, not the specification's text:**
    /// neither AVT1 nor AVT3 says what the transport state is at the
    /// boundary or which events fire. The reasoning: the action exists "to
    /// provide a seamless transition" (AVT1 section 2.4.2), the
    /// specification's own test for TRANSITIONING is "a noticable amount of
    /// time before a human user would actually see or hear the media"
    /// (section 2.4.1.3) and at a gapless boundary there is none, and a
    /// control point that saw STOPPED there would take the track for ended
    /// and push the next one itself. All the changes are recorded together,
    /// so they leave in one LastChange event.
    ///
    /// Ignored (returns `false`) when no next URI is queued, in an old
    /// epoch, or when nothing is playing.
    pub fn track_boundary(&mut self, epoch: u64, duration_ms: Option<u64>, seekable: bool) -> bool {
        if !self.current_epoch(epoch)
            || self.next_uri.is_empty()
            || !matches!(
                self.state,
                TransportState::Playing | TransportState::Transitioning
            )
        {
            return false;
        }
        self.uri = std::mem::take(&mut self.next_uri);
        self.metadata = std::mem::take(&mut self.next_metadata);
        self.next_is_bad = false;
        self.duration_ms = duration_ms.or_else(|| Self::duration_hint(&self.uri, &self.metadata));
        self.seekable = seekable;
        self.position_ms = 0;
        self.state = TransportState::Playing;
        self.commit();
        true
    }

    /// The queued next URI cannot be played (not found, not a format chorus
    /// decodes, AAC inside an MP4). AVT1 section 2.4.2.3: "the
    /// TransportState should be kept. After the current URI finishes
    /// playing, the transition to that illegal URI cannot be made. and the
    /// TransportState should be set to 'STOPPED'." So nothing changes now
    /// but `Next` leaving CurrentTransportActions; [`AvTransport::ended`]
    /// then stops with ERROR_OCCURRED.
    pub fn next_failed(&mut self, epoch: u64, reason: &str) -> bool {
        if !self.current_epoch(epoch) || self.next_uri.is_empty() {
            return false;
        }
        self.next_is_bad = true;
        self.last_failure = Some(reason.to_string());
        self.commit();
        true
    }

    /// The current media played to its end and nothing followed it in the
    /// audio.
    ///
    /// - No next URI: STOPPED, position 0 (AVT1 section 2.5.1, figure 1:
    ///   "end of media" leads to STOPPED).
    /// - A next URI that failed ([`AvTransport::next_failed`]): STOPPED with
    ///   TransportStatus ERROR_OCCURRED, and the next URI is cleared (AVT1
    ///   section 2.4.2.3).
    /// - A next URI that is good but was not ready in time, so the join was
    ///   not gapless: it becomes the current URI as the specification says,
    ///   and because there *is* a noticeable wait now, the state goes
    ///   through TRANSITIONING; the returned effects load and start it.
    ///
    /// Ignored in an old epoch and when already stopped.
    pub fn ended(&mut self, epoch: u64) -> Vec<Effect> {
        if !self.current_epoch(epoch) || self.state == TransportState::Stopped {
            return vec![];
        }
        self.position_ms = 0;
        self.held = false;
        let effects = if self.can_go_next() {
            let (uri, metadata) = (self.next_uri.clone(), self.next_metadata.clone());
            self.load(&uri, &metadata);
            self.clear_next();
            self.state = TransportState::Transitioning;
            vec![Effect::Load { uri, metadata }, Effect::Start]
        } else {
            if self.next_is_bad {
                self.status = TransportStatus::ErrorOccurred;
            }
            self.clear_next();
            self.state = TransportState::Stopped;
            self.epoch += 1;
            vec![]
        };
        self.commit();
        effects
    }

    /// The current media cannot be fetched or decoded (or stopped being
    /// fetchable midway): STOPPED with TransportStatus ERROR_OCCURRED (AVT1
    /// section 2.4.1.3: "If the renderer fails to locate or download the
    /// resource at the URI the TransportState should change to 'STOPPED'").
    /// The URI stays, so a Play retries it. `reason` is kept for
    /// [`AvTransport::last_failure`]; it names what was refused, for example
    /// `unsupported: aac`.
    pub fn failed(&mut self, epoch: u64, reason: &str) -> bool {
        if !self.current_epoch(epoch) {
            return false;
        }
        self.state = TransportState::Stopped;
        self.status = TransportStatus::ErrorOccurred;
        self.last_failure = Some(reason.to_string());
        self.position_ms = 0;
        self.held = false;
        self.epoch += 1;
        self.commit();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_transport_has_no_media_and_nothing_to_event() {
        let mut t = AvTransport::new();
        assert_eq!(t.state(), TransportState::NoMediaPresent);
        assert_eq!(t.status(), TransportStatus::Ok);
        assert_eq!(t.transport_actions(), "");
        assert!(!t.events().is_pending());
        assert_eq!(t.epoch(), 0);
        assert_eq!(t.current(), ("", ""));
        assert_eq!(t.next_queued(), ("", ""));
        assert_eq!(t.last_failure(), None);
        assert_eq!(
            AvTransport::default().state(),
            TransportState::NoMediaPresent
        );
    }

    #[test]
    fn the_state_names_are_the_specifications() {
        let names: Vec<&str> = [
            TransportState::NoMediaPresent,
            TransportState::Stopped,
            TransportState::Transitioning,
            TransportState::Playing,
            TransportState::PausedPlayback,
        ]
        .iter()
        .map(|s| s.as_str())
        .collect();
        assert_eq!(
            names,
            [
                "NO_MEDIA_PRESENT",
                "STOPPED",
                "TRANSITIONING",
                "PLAYING",
                "PAUSED_PLAYBACK"
            ]
        );
        assert_eq!(TransportStatus::ErrorOccurred.as_str(), "ERROR_OCCURRED");
    }

    #[test]
    fn only_http_and_https_are_uris() {
        for ok in ["http://192.0.2.1/a", "HTTPS://example.net/a", "Http://x"] {
            assert!(is_http(ok), "{ok}");
        }
        for bad in [
            "file:///etc/passwd",
            "ftp://192.0.2.1/a",
            "http:/x",
            "x",
            "",
            "rtsp://192.0.2.1/",
        ] {
            assert!(!is_http(bad), "{bad}");
        }
    }
}
