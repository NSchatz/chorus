//! Server-authoritative zone state.
//!
//! One process owns this and every subscriber is told what it says. There is
//! no state anywhere else that a subscriber could be reading instead, which is
//! what makes "fan the resulting state out to all subscribers" a complete
//! description rather than half of a reconciliation problem.
//!
//! # A change is applied whole or not at all
//!
//! [`Zones::apply`] works on a COPY of the state and installs the copy only
//! when every step succeeded, so a refused command leaves the state
//! byte-identical to what it was. That is not a nicety: the criterion is that a
//! refused message leaves every subscriber's state unchanged, and catalog v2's
//! commands (a `take` that moves several rooms and dissolves a group, a group
//! volume that scales every room) are exactly where a validate-as-you-go apply
//! would get it right for some steps and not for others.
//!
//! # Every zone is in a group, always
//!
//! A group is the unit a stream is served to, so a zone in no group is a zone
//! with nothing to play. Ungrouping therefore puts a zone into a group of its
//! own, named for the zone, rather than into an absent state the state message
//! would have to spell `null`. One consequence worth stating: `ungroup` on a
//! zone that is already alone is not an error and changes nothing.
//!
//! # Rooms, groups and the clamp (catalog v2)
//!
//! A ROOM is a zone (the wire keeps the key `zone`). A group is formed by the
//! rooms whose `group` names it: a room alone in a group named for itself is
//! kind `room`, a group whose id is a saved definition is kind `saved`, and any
//! other is kind `live`. Saved definitions are listed always, active or not.
//!
//! **Every volume path is clamped, never refused, to the room's effective
//! limit**: `min(limit, the cap of every quiet-hours window active now)`. A
//! volume above it is set to it; lowering a limit, or a window becoming
//! active, pulls the volume down; a window ending raises nothing. The paths
//! are `volume`, `volume_step`, `group_volume`, `group_volume_step`, the
//! controller role (which becomes those commands), and the runtime hooks
//! ([`Zones::runtime_volume`], [`Zones::start_ramp`]). One function,
//! [`Zones::effective_limit`], is what all of them consult.
//!
//! # What the runtime drives
//!
//! Firing alarms, counting sleep timers down, ramping, choosing sources and
//! knowing the time are a later track's (the server's runtime on
//! `crates/schedule`). This model reads no clock and owns no timer; it gives the
//! runtime these hooks, each of which bumps the serial when it changes
//! something, so the caller fans the state out exactly as it does after a
//! command:
//!
//! - [`Zones::set_civil_time`] (or [`Zones::set_active_quiet`]) says which
//!   quiet-hours windows are active, and clamps;
//! - [`Zones::set_group_source`] says what a group plays;
//! - [`Zones::set_now_playing`] says what a group's player source is playing
//!   (goal 16: title, artist, album, artwork, duration, state);
//! - [`Zones::start_ramp`], [`Zones::runtime_volume`] and [`Zones::stop_ramp`]
//!   move a room's volume over time, every step clamped;
//! - [`Zones::set_alarm_ringing`] records an alarm as ringing or not;
//! - [`Zones::sleep_expired`] removes a sleep timer that ran out;
//! - [`Zones::offer_input`] and [`Zones::withdraw_input`] say which line-ins
//!   are offered.
//!
//! # Player sources and what is playing (goal 16)
//!
//! `player:<id>` is one of the server's network media players. **A player
//! plays in at most one group**: a `take` or a runtime hook that would give a
//! second formed group the same player is refused by name (field `source`).
//! When groups re-form, the player follows its group as every source does (a
//! live group forming, a group dissolving into its last room); the one case
//! where a source is otherwise COPIED, `take` pushing rooms out of the
//! target's group, keeps the player with the target and leaves the rooms
//! pushed out playing `none` (or, when the same `take` gives the target
//! another source, the player goes with the rooms pushed out).
//!
//! What the player is playing is the group's [`NowPlaying`] record: a fact
//! about now, never persisted, set only by the runtime, held to its bounds,
//! moved with the group's source, dropped when the group is no longer formed
//! and cleared the moment the group's source stops being a player source.
//! The state message carries it only where there is one (`now_playing` on the
//! `groups[]` entry, `source` and `now_playing` on each member room), so a
//! server with no player says the bytes it said before goal 16.
//!
//! # Speakers (goal 14)
//!
//! The speakers the server has adopted are held here too
//! ([`crate::speakers`]), because a speaker's room is a fact about the rooms:
//! `speaker_room` makes the speaker a member of the room (its `endpoints`),
//! and a speaker with an assigned room is marked present and absent by its
//! SESSION ([`Zones::speaker_session_up`], [`Zones::speaker_session_down`]),
//! which is what lets an endpoint with no control client (the firmware) be in
//! a room at all. The session layer's hooks bump the serial when they change
//! something, as the runtime's do.
//!
//! # Firmware (goal 14, explicit installs)
//!
//! The staged images ([`Zones::set_firmware_images`]) and what each speaker
//! reported it runs ([`crate::speakers::SpeakerNow::firmware`]) are held here
//! because `firmware_install` is refused or accepted against both, by name:
//! [`Zones::firmware_targets`] says who an install would reach, and the
//! server starts the transfers for exactly those, inside the same commit. The
//! model starts nothing itself: accepting the command marks each target
//! `requested`, and that is all.

use crate::catalog::{
    alarm_value, autoplay_value, centi_db_value, filters_value, input_kind_value,
    input_label_value, is_display_name, is_identifier, members_value, soloist_value,
    stored_source_value, texts, window_value, Command, Refusal, Volume, WakeWord,
    MAX_IDENTIFIER_LEN, VOLUME_SCALE,
};
use crate::firmware::{self, image_value, Image};
use crate::json::{self, Value};
use crate::rooms::{
    Alarm, Autoplay, BondMember, CivilTime, InputId, InputKind, InputLabel, InputRole, Link,
    NowPlaying, Origin, PlayState, QuietWindow, Role, SavedGroup, SleepTimer, SoloistState, Source,
    StoredSource, MAX_DEFINITIONS, VIA_STREAMER,
};
use crate::sound::{BassManagement, RoomEq, SoundSettings};
use crate::speakers::{key_change_value, speaker_value, KeyChange, NotListed, Speaker, Speakers};
use crate::transport::{Transport, ZoneTransports};

/// One zone: what it is called, what it plays, and how loudly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Zone {
    /// The identifier, which never changes and is what commands name.
    pub id: String,
    /// The human-set name, which is what a person sees.
    pub name: String,
    /// The group whose stream this zone plays.
    pub group: String,
    /// The amplitude factor applied to the PCM this zone's endpoints play,
    /// always at or below the effective limit.
    pub volume: Volume,
    /// Whether this zone is muted.
    pub muted: bool,
    /// Every endpoint this zone has ever had, in the order they first
    /// attached, persisted across a restart.
    pub endpoints: Vec<String>,
    /// The subset of `endpoints` attached right now. Never persisted: which
    /// endpoints are switched on is a fact about now.
    pub present: Vec<String>,
    /// (v2) The room's maximum volume. Full scale by default.
    pub limit: Volume,
    /// (v2) The room's quiet-hours windows.
    pub quiet: Vec<QuietWindow>,
    /// (v2) Which of `quiet` are active now, one flag per window. Never
    /// persisted: it is a fact about now, set by the runtime.
    pub quiet_active: Vec<bool>,
    /// (v2) Whether the room's quiet hours are switched on. While they are
    /// off the windows stay stored and keep their `quiet_active` flags (the
    /// clock is still inside them or not), and none of them caps the room.
    pub quiet_enabled: bool,
    /// (v2) Whether the room's voice path is switched on (K73's software
    /// gate, `voice_enabled`). Off by default: no microphone of the room is
    /// asked for audio until a person switches it on.
    pub voice_enabled: bool,
    /// (v2, voice) The ids of the wake words the room listens for, as a
    /// `voice_wake_words` chose them; `None` until one did, which means every
    /// one the server runs. An id the running build has no model for (a
    /// state file from another build) is kept and matches nothing.
    pub wake_words: Option<Vec<String>>,
    /// (v2) The bonded set, or empty where the room has none and plays the
    /// stream's channels as v1 did.
    pub bond: Vec<BondMember>,
    /// (v2) A ramp the runtime is running, by its target. Never persisted.
    pub ramp: Option<Volume>,
    /// (v2, goal 12) Tone, loudness, night mode and speech enhancement.
    pub sound: SoundSettings,
    /// (v2, goal 12) Bass management, in force when `bond` has an `LFE`
    /// member ([`Zone::has_sub`]).
    pub bass: BassManagement,
    /// (v2, goal 12) The room-correction EQ.
    pub room_eq: RoomEq,
    /// (v2, goal 13) The A/V trim, ms: positive plays the room's TV audio
    /// later ([`crate::theater`]).
    pub av_trim_ms: i16,
}

impl Zone {
    /// A zone with the shipped defaults: named for its identifier, alone in a
    /// group of its own, at full scale and unmuted, with no limit below full
    /// scale, no quiet hours and no bonded set.
    pub fn new(id: &str) -> Zone {
        Zone {
            id: id.to_string(),
            name: id.to_string(),
            group: id.to_string(),
            volume: Volume::FULL,
            muted: false,
            endpoints: Vec::new(),
            present: Vec::new(),
            limit: Volume::FULL,
            quiet: Vec::new(),
            quiet_active: Vec::new(),
            quiet_enabled: true,
            voice_enabled: false,
            wake_words: None,
            bond: Vec::new(),
            ramp: None,
            sound: SoundSettings::default(),
            bass: BassManagement::default(),
            room_eq: RoomEq::default(),
            av_trim_ms: 0,
        }
    }

    /// Whether the room's bonded set has a sub (an `LFE` member), which is
    /// what turns its bass management on.
    pub fn has_sub(&self) -> bool {
        self.bond.iter().any(|b| b.role == Role::Lfe)
    }

    /// The channel role `endpoint` plays in this room's bonded set, if any.
    pub fn role_of(&self, endpoint: &str) -> Option<Role> {
        self.bond
            .iter()
            .find(|b| b.endpoint == endpoint)
            .map(|b| b.role)
    }

    /// The amplitude factor this zone's endpoints apply, which is zero while
    /// it is muted.
    pub fn gain(&self) -> Volume {
        if self.muted {
            Volume::SILENT
        } else {
            self.volume
        }
    }

    /// `min(limit, the cap of every active quiet-hours window)`, and `limit`
    /// alone while the room's quiet hours are switched off.
    pub fn effective_limit(&self) -> Volume {
        if !self.quiet_enabled {
            return self.limit;
        }
        self.quiet
            .iter()
            .zip(self.quiet_active.iter().chain(std::iter::repeat(&false)))
            .filter(|(_, active)| **active)
            .map(|(w, _)| w.limit)
            .fold(self.limit, std::cmp::min)
    }

    /// Pull the volume (and a running ramp's target) down to the effective
    /// limit. Never raises anything.
    fn clamp(&mut self) {
        let cap = self.effective_limit();
        self.volume = self.volume.min(cap);
        if let Some(target) = self.ramp {
            self.ramp = Some(target.min(cap));
        }
    }
}

/// What kind of group a formed group is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupKind {
    /// One room alone in the group named for it.
    Room,
    /// Any other group that is not saved.
    Live,
    /// A group whose id is a saved definition.
    Saved,
}

impl GroupKind {
    /// The catalog's word for it.
    pub fn name(self) -> &'static str {
        match self {
            GroupKind::Room => "room",
            GroupKind::Live => "live",
            GroupKind::Saved => "saved",
        }
    }
}

/// A group as it is formed right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormedGroup {
    /// Its identifier.
    pub id: String,
    /// What kind it is.
    pub kind: GroupKind,
    /// Its rooms, in configured order.
    pub zones: Vec<String>,
}

/// Every zone the server knows about, and the serial that says which version
/// of that a subscriber is holding.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Zones {
    zones: Vec<Zone>,
    serial: u64,
    /// Where each group's audio stream is served, and where a group with no
    /// entry of its own is served.
    group_audio: Vec<(String, String)>,
    default_audio: String,
    /// (v2) Each room's declared transport. Configuration, never persisted.
    transports: ZoneTransports,
    /// (v2) What each endpoint has said about its link, sorted by endpoint.
    links: Vec<(String, Link)>,
    /// (v2) Saved group definitions, sorted by id.
    saved: Vec<SavedGroup>,
    /// (v2) What each formed group plays, where it is not [`Source::Stream`].
    sources: Vec<(String, Source)>,
    /// (v2, goal 16) What each formed group's player source is playing now,
    /// sorted by group. Only a group whose source is a player source has one.
    /// Never persisted.
    now_playing: Vec<(String, NowPlaying)>,
    /// (v2) Alarms, sorted by id.
    alarms: Vec<Alarm>,
    /// (v2) Alarms ringing now. Never persisted.
    ringing: Vec<String>,
    /// (v2) Sleep timers asked for, sorted by target. Never persisted.
    sleep: Vec<SleepTimer>,
    /// (v2) Autoplay rules, sorted by input.
    autoplay: Vec<Autoplay>,
    /// (v2) Inputs offered now, sorted. Never persisted.
    inputs: Vec<InputId>,
    /// (v2, goal 17) Stored sources, sorted by id.
    stored: Vec<StoredSource>,
    /// (v2, goal 17) Input labels, sorted by input.
    labels: Vec<InputLabel>,
    /// (v2) The civil time the quiet-hours flags were last set for.
    now: Option<CivilTime>,
    /// (v2, goal 14) The adopted speakers and the key changes refused.
    speakers: Speakers,
    /// (v2, goal 14) The firmware images staged in `--firmware-dir`, as the
    /// server last graded them; `None` when it has no such directory. Never
    /// persisted: the directory is.
    firmware_images: Option<Vec<Image>>,
    /// (v2, goal 17) The Soloist receivers, their build and its expiry
    /// warning, as the server's receiver manager last said; `None` on a
    /// server without `--soloist-receivers`. Never persisted.
    soloist: Option<SoloistState>,
    /// (v2, goal 18) The origins an `announce` URL may come from
    /// (`--announce-origin`), in the order configured. Never persisted and
    /// never in the state: it is the server's configuration, served on
    /// `GET /api/server`.
    announce_origins: Vec<Origin>,
    /// (v2, voice) The wake-word models the server runs, listed in the
    /// state as `wake_words` when there is any. Never persisted.
    wake_words: Vec<WakeWord>,
    /// (announcements, ADR 0175) The announcements the server is playing
    /// and the last few that are over, each with its outcome, written into
    /// the state as `announcements` when there is any. Never persisted.
    announcements: Vec<Announcement>,
    /// (ADR 0194) The built-in chimes the server can ring, written into the
    /// state as `chimes` when it said any. Never persisted: a fact about the
    /// build.
    chimes: Vec<String>,
    /// (ADR 0194) Whether a schedule runtime counts the sleep timers down
    /// here, which is what puts `remaining_s` on each. Never persisted.
    sleep_counted: bool,
    /// (ADR 0194) The kind of each offered input whose kind the server
    /// said, sorted by input. Never persisted.
    input_kinds: Vec<(InputId, InputKind)>,
    /// (room correction, ADR 0000) The measurement sweep the server is
    /// playing, or the last one it played, with its outcome, written into
    /// the state as `measurement` once there has been one. Never persisted.
    measurement: Option<Measurement>,
}

/// (ADR 0000) Where a measurement sweep is: what a caller of `measure_sweep`
/// waits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeasurementState {
    /// Its room hears the sweep's stream: the silence before it, the sweep
    /// or the silence after it.
    Playing,
    /// The sweep and the silence after it played to their end.
    Finished,
    /// It was called off before its end.
    Cancelled,
}

impl MeasurementState {
    /// The word the state message carries.
    pub fn name(self) -> &'static str {
        match self {
            MeasurementState::Playing => "playing",
            MeasurementState::Finished => "finished",
            MeasurementState::Cancelled => "cancelled",
        }
    }
}

/// (ADR 0000) One measurement sweep as the state message carries it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Measurement {
    /// Its number, counted from 1 over the server's run.
    pub id: u64,
    /// The room that hears it.
    pub zone: String,
    /// Where it is.
    pub state: MeasurementState,
    /// The volume the room plays it at: the command's, clamped to the
    /// room's effective limit, or the room's own.
    pub volume: Volume,
    /// The silence before the sweep, ms.
    pub lead_ms: u64,
    /// The sweep itself, ms.
    pub sweep_ms: u64,
    /// The silence after the sweep, ms.
    pub tail_ms: u64,
    /// Why it was cancelled, in the server's words.
    pub reason: Option<String>,
}

impl Measurement {
    fn value(&self) -> Value {
        let mut members = vec![
            ("id".to_string(), Value::int(self.id as i64)),
            ("zone".to_string(), Value::text(&self.zone)),
            ("state".to_string(), Value::text(self.state.name())),
            ("volume".to_string(), Value::Num(self.volume.literal())),
            ("lead_ms".to_string(), Value::int(self.lead_ms as i64)),
            ("sweep_ms".to_string(), Value::int(self.sweep_ms as i64)),
            ("tail_ms".to_string(), Value::int(self.tail_ms as i64)),
        ];
        if let Some(reason) = &self.reason {
            members.push(("reason".to_string(), Value::text(reason)));
        }
        Value::Obj(members)
    }
}

/// (ADR 0000) What [`Zones::measure_begin`] changed, which is what the end
/// of the sweep puts back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Measured {
    /// The sweep's number.
    pub id: u64,
    /// The volume the room had.
    pub before: Volume,
    /// The volume the room plays the sweep at.
    pub set: Volume,
}

/// (ADR 0175) Where an announcement is: what a caller of `announce` waits
/// on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnouncementState {
    /// Its rooms are ducked for it, or its clip is playing.
    Playing,
    /// Its clip played to its end.
    Finished,
    /// Its clip could not be fetched or decoded, or was cut at the bound.
    Failed,
    /// Something else took its rooms: an alarm, an announcement that
    /// replaced it, a person who regrouped them.
    Displaced,
}

impl AnnouncementState {
    /// The word the state message carries.
    pub fn name(self) -> &'static str {
        match self {
            AnnouncementState::Playing => "playing",
            AnnouncementState::Finished => "finished",
            AnnouncementState::Failed => "failed",
            AnnouncementState::Displaced => "displaced",
        }
    }
}

/// (ADR 0175) One announcement as the state message carries it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Announcement {
    /// Its number: the `announcement` member of its command's answer.
    pub id: u64,
    /// The command's target.
    pub target: String,
    /// The rooms that hear it.
    pub rooms: Vec<String>,
    /// Where it is.
    pub state: AnnouncementState,
    /// Why it failed or what displaced it, in the server's words.
    pub reason: Option<String>,
}

impl Announcement {
    fn value(&self) -> Value {
        let mut members = vec![
            ("id".to_string(), Value::int(self.id as i64)),
            ("target".to_string(), Value::text(&self.target)),
            ("rooms".to_string(), texts(&self.rooms)),
            ("state".to_string(), Value::text(self.state.name())),
        ];
        if let Some(reason) = &self.reason {
            members.push(("reason".to_string(), Value::text(reason)));
        }
        Value::Obj(members)
    }
}

/// (ADR 0175) What the server looks at to decide whether an announcement
/// mixed over its rooms still has them ([`Zones::announce_watch`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnounceWatch {
    /// Those of its rooms that are still in the group it started in.
    pub here: Vec<String>,
    /// The alarm ringing in one of its rooms, if any.
    pub ringing: Option<String>,
    /// What that group plays now.
    pub source: Source,
}

/// (goal 18) What [`Zones::announce_begin`] changed, which is what the end of
/// the announcement puts back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Announced {
    /// The formed group that plays the clip.
    pub group: String,
    /// What the group played before.
    pub previous: Source,
    /// The rooms that hear the clip: the whole group for
    /// [`Zones::announce_begin`], the target's own rooms for
    /// [`Zones::announce_over_begin`].
    pub rooms: Vec<String>,
    /// Every room of the group whose volume the announcement set: the room,
    /// the volume it had, and the volume it was given (clamped).
    pub volumes: Vec<(String, Volume, Volume)>,
}

/// What a target names.
enum Target {
    /// A room, by index.
    Room(usize),
    /// A saved group definition, by id, formed or not.
    Saved(String),
    /// A formed group that is not saved, by id.
    Formed(String),
}

impl Zones {
    /// An empty set of zones, with every group's stream served at
    /// `default_audio` until told otherwise.
    pub fn new(default_audio: &str) -> Zones {
        Zones {
            default_audio: default_audio.to_string(),
            ..Zones::default()
        }
    }

    /// Say where one group's stream is served.
    pub fn set_group_audio(&mut self, group: &str, address: &str) {
        match self.group_audio.iter_mut().find(|(g, _)| g == group) {
            Some((_, a)) => *a = address.to_string(),
            None => self
                .group_audio
                .push((group.to_string(), address.to_string())),
        }
        self.group_audio.sort();
    }

    /// Where a group's stream is served.
    pub fn audio_for(&self, group: &str) -> &str {
        self.group_audio
            .iter()
            .find(|(g, _)| g == group)
            .map(|(_, a)| a.as_str())
            .unwrap_or(&self.default_audio)
    }

    /// Say which transport each room was declared with (`--zone <id>=<tier>`).
    /// Configuration: never persisted, never commanded.
    pub fn set_transports(&mut self, transports: ZoneTransports) {
        self.transports = transports;
    }

    /// How many changes have been applied since this state was created or
    /// loaded.
    ///
    /// A subscriber can tell a state message it has already seen from one it
    /// has not without comparing the whole thing.
    pub fn serial(&self) -> u64 {
        self.serial
    }

    /// Set the serial, which loading persisted state does and nothing else
    /// should.
    pub fn set_serial(&mut self, serial: u64) {
        self.serial = serial;
    }

    /// Every zone, in the order they were added.
    pub fn zones(&self) -> &[Zone] {
        &self.zones
    }

    /// One zone by identifier.
    pub fn zone(&self, id: &str) -> Option<&Zone> {
        self.zones.iter().find(|z| z.id == id)
    }

    /// Whether any zone has been configured.
    pub fn is_empty(&self) -> bool {
        self.zones.is_empty()
    }

    /// The effective limit of a room, or `None` for a room that is not one.
    pub fn effective_limit(&self, zone: &str) -> Option<Volume> {
        self.zone(zone).map(Zone::effective_limit)
    }

    /// What an endpoint has said about its link; `unknown` where it never has.
    pub fn link(&self, endpoint: &str) -> Link {
        self.links
            .iter()
            .find(|(e, _)| e == endpoint)
            .map(|(_, l)| *l)
            .unwrap_or(Link::Unknown)
    }

    /// Every endpoint's link fact that has been reported, sorted by endpoint.
    pub fn links(&self) -> &[(String, Link)] {
        &self.links
    }

    /// Record an endpoint's link, which loading persisted state does.
    pub fn set_link(&mut self, endpoint: &str, link: Link) {
        match self.links.iter_mut().find(|(e, _)| e == endpoint) {
            Some((_, l)) => *l = link,
            None => {
                self.links.push((endpoint.to_string(), link));
                self.links.sort();
            }
        }
    }

    /// Every saved group definition, sorted by id.
    pub fn saved_groups(&self) -> &[SavedGroup] {
        &self.saved
    }

    /// Every alarm, sorted by id.
    pub fn alarms(&self) -> &[Alarm] {
        &self.alarms
    }

    /// Every autoplay rule, sorted by input.
    pub fn autoplay_rules(&self) -> &[Autoplay] {
        &self.autoplay
    }

    /// Every sleep timer asked for, sorted by target.
    pub fn sleep_timers(&self) -> &[SleepTimer] {
        &self.sleep
    }

    /// Every input offered now.
    pub fn inputs(&self) -> &[InputId] {
        &self.inputs
    }

    /// Whether an alarm is ringing.
    pub fn is_ringing(&self, alarm: &str) -> bool {
        self.ringing.iter().any(|a| a == alarm)
    }

    /// Install a persisted definition, which loading persisted state does.
    /// The caller has validated it; this keeps the sort order.
    pub fn restore_saved_group(&mut self, group: SavedGroup) {
        upsert(&mut self.saved, group, |g| g.id.clone());
    }

    /// Install a persisted alarm, which loading persisted state does.
    pub fn restore_alarm(&mut self, alarm: Alarm) {
        upsert(&mut self.alarms, alarm, |a| a.id.clone());
    }

    /// (goal 17) Every stored source, sorted by id.
    pub fn stored_sources(&self) -> &[StoredSource] {
        &self.stored
    }

    /// (goal 17) The stored source with this id.
    pub fn stored_source(&self, id: &str) -> Option<&StoredSource> {
        self.stored.iter().find(|s| s.id == id)
    }

    /// (goal 17) Every input label, sorted by input.
    pub fn input_labels(&self) -> &[InputLabel] {
        &self.labels
    }

    /// (goal 17) An input's label, when a person gave it one.
    pub fn input_label(&self, input: &InputId) -> Option<&InputLabel> {
        self.labels.iter().find(|l| l.input == *input)
    }

    /// (goal 17) The room an input's endpoint plays in: the one whose
    /// present endpoints, else whose membership, names it. A `streamer`
    /// input autoplays there.
    pub fn room_of_endpoint(&self, endpoint: &str) -> Option<&str> {
        self.zones
            .iter()
            .find(|z| z.present.iter().any(|e| e == endpoint))
            .or_else(|| {
                self.zones
                    .iter()
                    .find(|z| z.endpoints.iter().any(|e| e == endpoint))
            })
            .map(|z| z.id.as_str())
    }

    /// Install a persisted stored source, which loading persisted state does.
    pub fn restore_stored_source(&mut self, stored: StoredSource) {
        upsert(&mut self.stored, stored, |s| s.id.clone());
    }

    /// Install a persisted input label, which loading persisted state does.
    pub fn restore_input_label(&mut self, label: InputLabel) {
        upsert(&mut self.labels, label, |l| l.input.literal());
    }

    /// Install a persisted autoplay rule, which loading persisted state does.
    pub fn restore_autoplay(&mut self, rule: Autoplay) {
        upsert(&mut self.autoplay, rule, |r| r.input.literal());
    }

    /// Add a zone, which configuration does and no control message does.
    ///
    /// A control message cannot create a zone: the set of rooms is a fact
    /// about a house and is configured, and a typo in a zone name has to be a
    /// refusal rather than a new room nobody has.
    pub fn add(&mut self, mut zone: Zone) -> Result<(), Refusal> {
        if !is_identifier(&zone.id) {
            return Err(Refusal::rejected(
                "zone",
                format!("'{}' is not a zone identifier", zone.id),
            ));
        }
        if !is_display_name(&zone.name) {
            return Err(Refusal::rejected(
                "name",
                format!("'{}' is not a zone name", zone.name.escape_debug()),
            ));
        }
        if !is_identifier(&zone.group) {
            return Err(Refusal::rejected(
                "group",
                format!("'{}' is not a group identifier", zone.group),
            ));
        }
        if self.zone(&zone.id).is_some() {
            return Err(Refusal::rejected(
                "zone",
                format!("the zone '{}' is already configured", zone.id),
            ));
        }
        zone.quiet_active.resize(zone.quiet.len(), false);
        zone.clamp();
        self.zones.push(zone);
        self.serial += 1;
        Ok(())
    }

    /// Mark an endpoint as gone, which a dropped control session does.
    ///
    /// It stays in the zone's membership; only its presence changes. See
    /// `docs/decisions/0019-a-persisted-endpoint-that-never-comes-back.md`.
    ///
    /// A speaker that was assigned its room (`speaker_room`) and has a
    /// session up stays present: its presence is its session's, and a control
    /// client saying goodbye does not end the session that is playing.
    pub fn endpoint_left(&mut self, endpoint: &str) -> bool {
        if self
            .speakers
            .get(endpoint)
            .is_some_and(|s| s.room.is_some() && s.now.present())
        {
            return false;
        }
        self.clear_presence(endpoint)
    }

    /// Take `endpoint` out of every room's `present`. Whether it was in one;
    /// the serial moves only when it was.
    fn clear_presence(&mut self, endpoint: &str) -> bool {
        let mut changed = false;
        for zone in &mut self.zones {
            let before = zone.present.len();
            zone.present.retain(|e| e != endpoint);
            changed |= zone.present.len() != before;
        }
        if changed {
            self.serial += 1;
        }
        changed
    }

    /// Apply one command, whole or not at all.
    ///
    /// A command that changes nothing still succeeds and still bumps the
    /// serial, so that a subscriber which asked for something already true is
    /// answered with the state rather than with silence. (`hello` is the one
    /// exception: it is an announcement, not a change.)
    pub fn apply(&mut self, command: &Command) -> Result<(), Refusal> {
        if let Command::Hello = command {
            return Ok(());
        }
        let mut next = self.clone();
        next.change(command)?;
        next.prune();
        next.serial += 1;
        *self = next;
        Ok(())
    }

    fn index(&self, id: &str) -> Result<usize, Refusal> {
        self.zones.iter().position(|z| z.id == id).ok_or_else(|| {
            Refusal::rejected(
                "zone",
                format!(
                    "there is no zone '{}'; the zones configured on this server are {}",
                    id,
                    self.zone_list()
                ),
            )
        })
    }

    /// The change one command makes, on `self` (which [`Zones::apply`] has
    /// made a copy, so a refusal part way through installs nothing).
    fn change(&mut self, command: &Command) -> Result<(), Refusal> {
        let at = match command.zone() {
            Some(id) => Some(self.index(id)?),
            None => None,
        };
        let room = || at.expect("a command naming a zone has been resolved to one");
        match command {
            Command::Hello => {}
            Command::Attach { endpoint, link, .. } => {
                if let Some(link) = link {
                    if *link != Link::Wired {
                        if let Some(bonded) = self.bonded_in(endpoint) {
                            return Err(Refusal::rejected(
                                "link",
                                format!(
                                    "endpoint '{}' plays in room '{}''s bonded set and reports \
                                     its link as {}; a bonded set holds wired endpoints only \
                                     (K91), so unbond room '{}' first",
                                    endpoint, bonded, link, bonded
                                ),
                            ));
                        }
                    }
                    self.set_link(endpoint, *link);
                }
                // A speaker the owner assigned a room (`speaker_room`) plays
                // there: an `attach` naming another room is accepted, so the
                // endpoint's control client keeps working, and attaches it to
                // the room it was assigned (the ADR: the explicit assignment
                // wins over the endpoint's own start-up flag).
                let assigned = self
                    .speakers
                    .get(endpoint)
                    .and_then(|s| s.room.as_deref())
                    .and_then(|r| self.zones.iter().position(|z| z.id == r));
                let zone = &mut self.zones[assigned.unwrap_or_else(room)];
                if !zone.endpoints.iter().any(|e| e == endpoint) {
                    zone.endpoints.push(endpoint.clone());
                }
                if !zone.present.iter().any(|e| e == endpoint) {
                    zone.present.push(endpoint.clone());
                }
            }
            Command::Name { name, .. } => self.zones[room()].name = name.clone(),
            Command::Group { group, .. } => self.zones[room()].group = group.clone(),
            Command::Ungroup { .. } => {
                let i = room();
                self.zones[i].group = self.zones[i].id.clone();
            }
            Command::Volume { volume, .. } => self.set_volume(room(), *volume),
            Command::Mute { muted, .. } => self.zones[room()].muted = *muted,
            Command::Join { target, .. } => self.join(room(), target)?,
            Command::Bond { members, .. } => self.bond(room(), members)?,
            Command::Unbond { .. } => self.zones[room()].bond.clear(),
            Command::GroupSave { group, name, zones } => {
                if self.zone(group).is_some() {
                    return Err(Refusal::rejected(
                        "group",
                        format!(
                            "'{}' is a room's identifier, and a room alone is already its own \
                             group; a saved group takes an identifier no room has",
                            group
                        ),
                    ));
                }
                for id in zones {
                    if self.zone(id).is_none() {
                        return Err(Refusal::rejected(
                            "zones",
                            format!(
                                "there is no zone '{}'; the zones configured on this server are {}",
                                id,
                                self.zone_list()
                            ),
                        ));
                    }
                }
                let saved = SavedGroup {
                    id: group.clone(),
                    name: name.clone(),
                    zones: zones.clone(),
                };
                self.within_bound(self.saved.iter().any(|g| g.id == *group), self.saved.len())?;
                upsert(&mut self.saved, saved, |g| g.id.clone());
            }
            Command::GroupDelete { group } => {
                let before = self.saved.len();
                self.saved.retain(|g| g.id != *group);
                if self.saved.len() == before {
                    return Err(Refusal::rejected(
                        "group",
                        format!(
                            "there is no saved group '{}'; the saved groups are {}",
                            group,
                            list_or_none(self.saved.iter().map(|g| g.id.clone()))
                        ),
                    ));
                }
            }
            Command::Take { target, source } => self.take(target, source.as_ref())?,
            Command::GroupVolume { group, volume } => {
                let members = self.formed_members(group)?;
                self.scale_group(&members, *volume);
            }
            Command::GroupVolumeStep { group, step } => {
                let members = self.formed_members(group)?;
                let current = self.average(&members);
                let wanted = (i64::from(current.thousandths()) + i64::from(*step))
                    .clamp(0, i64::from(VOLUME_SCALE));
                self.scale_group(&members, thousandths(wanted));
            }
            Command::VolumeStep { step, .. } => {
                let i = room();
                let wanted = (i64::from(self.zones[i].volume.thousandths()) + i64::from(*step))
                    .clamp(0, i64::from(VOLUME_SCALE));
                self.set_volume(i, thousandths(wanted));
            }
            Command::Limit { limit, .. } => {
                let zone = &mut self.zones[room()];
                zone.limit = *limit;
                zone.clamp();
            }
            Command::QuietHours { windows, .. } => {
                let i = room();
                let now = self.now;
                let zone = &mut self.zones[i];
                zone.quiet = windows.clone();
                zone.quiet_active = windows
                    .iter()
                    .map(|w| now.is_some_and(|t| w.contains(t)))
                    .collect();
                zone.clamp();
            }
            Command::QuietHoursEnabled { enabled, .. } => {
                // Switching them back on inside a window pulls the volume
                // down to the cap; switching them off raises nothing.
                let zone = &mut self.zones[room()];
                zone.quiet_enabled = *enabled;
                zone.clamp();
            }
            Command::VoiceEnabled { enabled, .. } => {
                self.zones[room()].voice_enabled = *enabled;
            }
            Command::VoiceWakeWords { zone, wake_words } => {
                if let Some(unknown) = wake_words
                    .iter()
                    .find(|id| !self.wake_words.iter().any(|w| &w.id == *id))
                {
                    let runs = self
                        .wake_words
                        .iter()
                        .map(|w| w.id.as_str())
                        .collect::<Vec<_>>();
                    return Err(Refusal::rejected(
                        "wake_words",
                        format!(
                            "unknown-wake-word: this server runs no wake word '{}', so room \
                             '{}' cannot listen for it; it runs {}",
                            unknown,
                            zone,
                            if runs.is_empty() {
                                "none".to_string()
                            } else {
                                runs.join(", ")
                            }
                        ),
                    ));
                }
                self.zones[room()].wake_words = Some(wake_words.clone());
            }
            Command::VoiceStart { zone } => {
                // The room model agrees or refuses; the run is the server's
                // to open (crates/server/src/voice.rs), and nothing of it is
                // kept here.
                self.voice_start_check(zone)?;
            }
            // The room exists (resolved above); ending a run is the
            // server's, and a room with no run is not an error.
            Command::VoiceStop { .. } => {}
            Command::AlarmSet(alarm) => {
                self.persistent_target(&alarm.target)?;
                if let Source::Stored(id) = &alarm.source {
                    self.stored_exists(id, "source")?;
                }
                self.within_bound(
                    self.alarms.iter().any(|a| a.id == alarm.id),
                    self.alarms.len(),
                )?;
                upsert(&mut self.alarms, alarm.clone(), |a| a.id.clone());
            }
            Command::AlarmDelete { alarm } => {
                self.alarm_exists(alarm)?;
                self.alarms.retain(|a| a.id != *alarm);
                self.ringing.retain(|a| a != alarm);
            }
            Command::AlarmStop { alarm } => {
                self.alarm_exists(alarm)?;
                self.ringing.retain(|a| a != alarm);
            }
            Command::Sleep { target, minutes } => {
                if self.zone(target).is_none() && self.members(target).is_empty() {
                    return Err(self.no_target(target, "a room or a group that is formed now"));
                }
                self.sleep.retain(|s| s.target != *target);
                if *minutes > 0 {
                    upsert(
                        &mut self.sleep,
                        SleepTimer {
                            target: target.clone(),
                            minutes: *minutes,
                            // (ADR 0194) All of it is left at the instant
                            // it is asked for; the runtime counts from here.
                            remaining_s: self.sleep_counted.then_some(*minutes * 60),
                        },
                        |s| s.target.clone(),
                    );
                }
            }
            Command::Autoplay(rule) => {
                self.persistent_target(&rule.target)?;
                self.within_bound(
                    self.autoplay.iter().any(|r| r.input == rule.input),
                    self.autoplay.len(),
                )?;
                upsert(&mut self.autoplay, rule.clone(), |r| r.input.literal());
            }
            Command::SourceStore(stored) => {
                self.within_bound(
                    self.stored.iter().any(|s| s.id == stored.id),
                    self.stored.len(),
                )?;
                upsert(&mut self.stored, stored.clone(), |s| s.id.clone());
            }
            Command::SourceForget { id } => {
                self.stored_exists(id, "id")?;
                let users: Vec<String> = self
                    .alarms
                    .iter()
                    .filter(|a| a.source == Source::Stored(id.clone()))
                    .map(|a| a.id.clone())
                    .collect();
                if !users.is_empty() {
                    return Err(Refusal::rejected(
                        "id",
                        format!(
                            "stored source '{}' is what alarm {} plays; give the alarm another \
                             source or delete it first",
                            id,
                            users
                                .iter()
                                .map(|a| format!("'{}'", a))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    ));
                }
                self.stored.retain(|s| s.id != *id);
            }
            Command::InputLabel(label) => {
                if label.name.is_empty() {
                    // The role is 'line-in' (the decoder's rule): no label.
                    self.labels.retain(|l| l.input != label.input);
                } else {
                    self.within_bound(
                        self.labels.iter().any(|l| l.input == label.input),
                        self.labels.len(),
                    )?;
                    upsert(&mut self.labels, label.clone(), |l| l.input.literal());
                }
                self.relabel(&label.input);
            }
            Command::Sound {
                bass,
                treble,
                loudness,
                night,
                speech,
                tv_upmix,
                ..
            } => {
                // Partial: an absent field keeps what the room had. No
                // volume path is touched, so the clamp has nothing to do.
                let sound = &mut self.zones[room()].sound;
                sound.bass = bass.unwrap_or(sound.bass);
                sound.treble = treble.unwrap_or(sound.treble);
                sound.loudness = loudness.unwrap_or(sound.loudness);
                sound.night = night.unwrap_or(sound.night);
                sound.speech = speech.unwrap_or(sound.speech);
                sound.tv_upmix = tv_upmix.unwrap_or(sound.tv_upmix);
            }
            Command::AvTrim { av_trim_ms, .. } => {
                // Its range is the decoder's; the relay clamps a trim the
                // floor cannot carry (crate::theater), which is not this.
                self.zones[room()].av_trim_ms = *av_trim_ms;
            }
            Command::BassManagement {
                crossover_hz,
                sub_level_cdb,
                sub_polarity,
                ..
            } => {
                let bass = &mut self.zones[room()].bass;
                bass.crossover_hz = crossover_hz.unwrap_or(bass.crossover_hz);
                bass.sub_level_cdb = sub_level_cdb.unwrap_or(bass.sub_level_cdb);
                bass.sub_polarity = sub_polarity.unwrap_or(bass.sub_polarity);
            }
            Command::RoomEq {
                filters, enabled, ..
            } => {
                let eq = &mut self.zones[room()].room_eq;
                if let Some(filters) = filters {
                    eq.filters = filters.clone();
                }
                eq.enabled = enabled.unwrap_or(eq.enabled);
            }
            Command::SpeakerName { speaker, name } => {
                self.speaker_exists(speaker)?;
                let record = self.speakers.get_mut(speaker).expect("checked above");
                record.name = name.clone();
                record.named = true;
            }
            Command::SpeakerRoom { speaker, room } => {
                self.speaker_exists(speaker)?;
                let target = match room {
                    Some(id) => {
                        Some(self.zones.iter().position(|z| z.id == *id).ok_or_else(|| {
                            Refusal::rejected(
                                "room",
                                format!(
                                    "there is no zone '{}'; the zones configured on this server \
                                     are {}",
                                    id,
                                    self.zone_list()
                                ),
                            )
                        })?)
                    }
                    None => None,
                };
                self.not_bonded_elsewhere(speaker, room.as_deref())?;
                let present = self.speakers.get(speaker).is_some_and(|s| s.now.present());
                for (i, zone) in self.zones.iter_mut().enumerate() {
                    if Some(i) == target {
                        continue;
                    }
                    zone.endpoints.retain(|e| e != speaker);
                    zone.present.retain(|e| e != speaker);
                }
                if let Some(i) = target {
                    let zone = &mut self.zones[i];
                    if !zone.endpoints.iter().any(|e| e == speaker) {
                        zone.endpoints.push(speaker.clone());
                    }
                    // Its presence is its session's from here on.
                    zone.present.retain(|e| e != speaker);
                    if present {
                        zone.present.push(speaker.clone());
                    }
                }
                self.speakers.get_mut(speaker).expect("checked above").room = room.clone();
            }
            Command::SpeakerForget { speaker } => {
                if self.speakers.get(speaker).is_none()
                    && !self.speakers.key_changes().iter().any(|c| c.id == *speaker)
                {
                    return Err(self.no_speaker(speaker));
                }
                self.not_bonded_elsewhere(speaker, None)?;
                for zone in &mut self.zones {
                    zone.endpoints.retain(|e| e != speaker);
                    zone.present.retain(|e| e != speaker);
                }
                self.links.retain(|(e, _)| e != speaker);
                self.speakers.forget(speaker);
            }
            Command::FirmwareInstall {
                speaker,
                image,
                force,
            } => {
                let (image, targets) = self.firmware_targets(speaker.as_deref(), image, *force)?;
                for target in targets {
                    self.speakers.set_now(&target, |now| {
                        if let Some(fw) = &mut now.firmware {
                            fw.requested(&image);
                        }
                    });
                }
            }
            Command::FirmwareCancel { speaker } => {
                self.speaker_exists(speaker)?;
                let mut state = String::new();
                let cancelled = self.speakers.set_now(speaker, |now| {
                    if let Some(fw) = &mut now.firmware {
                        state = fw.state.clone();
                        fw.ended(firmware::CANCELLED, firmware::NO_REASON);
                    }
                });
                if !cancelled {
                    return Err(Refusal::rejected(
                        "speaker",
                        format!(
                            "nothing-to-cancel: speaker '{}' has no transfer to cancel (its \
                             firmware state is {}); an image already verified is the speaker's \
                             to boot",
                            speaker,
                            if state.is_empty() { "unknown" } else { &state }
                        ),
                    ));
                }
            }
            Command::SoloistRestart => {
                self.has_receivers()?;
                // The supervisors are the server's to tell
                // (crates/server/src/control.rs).
            }
            Command::Playback { target, .. } => {
                self.has_receivers()?;
                self.playback_group(target)?;
            }
            Command::Announce { target, url, .. } => {
                // The room model agrees or refuses; the clip is the server's
                // to play (crates/server/src/announce.rs), and it is the
                // server that changes what the group plays, when it has a
                // player for it.
                self.announce_check(target, url)?;
            }
            Command::MeasureSweep { zone, .. } => {
                // The room model agrees or refuses; the sweep is the
                // server's to play (crates/server/src/measure.rs), which
                // sets the room's volume when it starts
                // ([`Zones::measure_begin`]).
                self.measure_check(zone)?;
            }
            Command::FirmwareRescan => {
                if self.firmware_images.is_none() {
                    return Err(Refusal::rejected(
                        "t",
                        "no-firmware-dir: this server was started without --firmware-dir, so \
                         there is no directory to read"
                            .to_string(),
                    ));
                }
                // The directory is the server's to read; the model only
                // agrees that there is one (crates/server/src/control.rs).
            }
        }
        Ok(())
    }

    /// Who `firmware_install` would reach: `speaker`, or (`None`) every
    /// present speaker of the image's board that takes updates, is not busy
    /// and does not run the image's version already (any version with
    /// `force`). Refused by name, with nothing changed, when the image is not
    /// staged or not verified, when the named speaker cannot take it, or when
    /// `all` reaches nobody. Each refusal's detail starts with its name:
    /// `unknown-image`, `image-not-verified`, `speaker-absent`,
    /// `not-updatable`, `busy`, `wrong-board`, `already-running`,
    /// `nothing-to-install` (and an unknown speaker is the catalog's usual
    /// refusal of the `speaker` field).
    pub fn firmware_targets(
        &self,
        speaker: Option<&str>,
        image: &str,
        force: bool,
    ) -> Result<(Image, Vec<String>), Refusal> {
        let images = self.firmware_images.as_deref().unwrap_or(&[]);
        let Some(staged) = images.iter().find(|i| i.name == image) else {
            let names: Vec<&str> = images.iter().map(|i| i.name.as_str()).collect();
            return Err(Refusal::rejected(
                "image",
                format!(
                    "unknown-image: there is no staged image '{}'; {}",
                    image,
                    match (&self.firmware_images, names.is_empty()) {
                        (None, _) => "this server was started without --firmware-dir".to_string(),
                        (Some(_), true) => "no image is staged".to_string(),
                        (Some(_), false) => format!("the staged images are {}", names.join(", ")),
                    }
                ),
            ));
        };
        if let Some(reason) = &staged.refused {
            return Err(Refusal::rejected(
                "image",
                format!(
                    "image-not-verified: image '{}' was refused ({}) and is never offered",
                    image, reason
                ),
            ));
        }
        let targets = match speaker {
            Some(id) => {
                self.speaker_exists(id)?;
                let record = self.speakers.get(id).expect("checked above");
                Self::takes(record, staged, force).map_err(|(name, why)| {
                    Refusal::rejected("speaker", format!("{}: speaker '{}' {}", name, id, why))
                })?;
                vec![id.to_string()]
            }
            None => {
                let all: Vec<String> = self
                    .speakers
                    .all()
                    .iter()
                    .filter(|s| Self::takes(s, staged, force).is_ok())
                    .map(|s| s.id.clone())
                    .collect();
                if all.is_empty() {
                    return Err(Refusal::rejected(
                        "all",
                        format!(
                            "nothing-to-install: no present speaker of board '{}' takes image \
                             '{}' ({}{})",
                            staged.board,
                            image,
                            "each is absent, busy, of another board, or runs version ",
                            staged.version
                        ),
                    ));
                }
                all
            }
        };
        Ok((staged.clone(), targets))
    }

    /// Whether one speaker can be given `image` now; the refusal's name and
    /// its words where it cannot.
    fn takes(speaker: &Speaker, image: &Image, force: bool) -> Result<(), (&'static str, String)> {
        if !speaker.now.present() {
            return Err(("speaker-absent", "has no session up".to_string()));
        }
        let Some(fw) = &speaker.now.firmware else {
            return Err((
                "not-updatable",
                "has not said what firmware it runs, so it takes no updates".to_string(),
            ));
        };
        if fw.busy() {
            return Err((
                "busy",
                format!(
                    "has an install in progress (its firmware state is {})",
                    fw.state
                ),
            ));
        }
        if fw.board != image.board {
            return Err((
                "wrong-board",
                format!(
                    "is board '{}' and image '{}' was built for board '{}'",
                    fw.board, image.name, image.board
                ),
            ));
        }
        if fw.version == image.version && !force {
            return Err((
                "already-running",
                format!(
                    "already runs version {}; add \"force\": true to install it again",
                    fw.version
                ),
            ));
        }
        Ok(())
    }

    fn no_speaker(&self, speaker: &str) -> Refusal {
        Refusal::rejected(
            "speaker",
            format!(
                "there is no speaker '{}'; the speakers adopted on this server are {}",
                speaker,
                self.speakers.id_list()
            ),
        )
    }

    fn speaker_exists(&self, speaker: &str) -> Result<(), Refusal> {
        match self.speakers.get(speaker) {
            Some(_) => Ok(()),
            None => Err(self.no_speaker(speaker)),
        }
    }

    /// Refuse to take a speaker out of a room whose bonded set it plays in:
    /// a set with a member gone is not a layout, and which role goes is the
    /// owner's call (`unbond`), not a side effect of moving a speaker.
    fn not_bonded_elsewhere(&self, speaker: &str, staying_in: Option<&str>) -> Result<(), Refusal> {
        match self.bonded_in(speaker) {
            Some(bonded) if Some(bonded.as_str()) != staying_in => Err(Refusal::rejected(
                "speaker",
                format!(
                    "speaker '{}' plays in room '{}''s bonded set; unbond room '{}' first",
                    speaker, bonded, bonded
                ),
            )),
            _ => Ok(()),
        }
    }

    /// Refuse a new definition past [`MAX_DEFINITIONS`]; replacing one is
    /// always allowed.
    fn within_bound(&self, replacing: bool, held: usize) -> Result<(), Refusal> {
        if !replacing && held >= MAX_DEFINITIONS {
            return Err(Refusal::rejected(
                "",
                format!(
                    "this server holds at most {} of these and has that many; delete one first",
                    MAX_DEFINITIONS
                ),
            ));
        }
        Ok(())
    }

    fn alarm_exists(&self, alarm: &str) -> Result<(), Refusal> {
        if self.alarms.iter().any(|a| a.id == alarm) {
            return Ok(());
        }
        Err(Refusal::rejected(
            "alarm",
            format!(
                "there is no alarm '{}'; the alarms are {}",
                alarm,
                list_or_none(self.alarms.iter().map(|a| a.id.clone()))
            ),
        ))
    }

    fn stored_exists(&self, id: &str, field: &str) -> Result<(), Refusal> {
        if self.stored.iter().any(|s| s.id == id) {
            return Ok(());
        }
        Err(Refusal::rejected(
            field,
            format!(
                "there is no stored source '{}'; the stored sources are {}",
                id,
                list_or_none(self.stored.iter().map(|s| s.id.clone()))
            ),
        ))
    }

    /// (goal 17) Refuse a source no group plays: `stored:<id>` is an alarm's
    /// spelling, never a group's.
    fn group_source(source: &Source) -> Result<(), Refusal> {
        match source {
            Source::Stored(id) => Err(Refusal::rejected(
                "source",
                format!(
                    "'stored:{}' is a stored source, which only an alarm plays; a group plays \
                     the player or the receiver the runtime gives it",
                    id
                ),
            )),
            _ => Ok(()),
        }
    }

    /// A target that outlives a restart: a room or a saved group. A live group
    /// is not one, because its id names nothing once it dissolves.
    fn persistent_target(&self, target: &str) -> Result<(), Refusal> {
        if self.zone(target).is_some() || self.saved.iter().any(|g| g.id == target) {
            return Ok(());
        }
        Err(self.no_target(target, "a room or a saved group"))
    }

    fn no_target(&self, target: &str, what: &str) -> Refusal {
        Refusal::rejected(
            "target",
            format!(
                "'{}' is not {}; the rooms are {}, and the groups formed now are {}",
                target,
                what,
                self.zone_list(),
                list_or_none(self.formed_groups().into_iter().map(|g| g.id))
            ),
        )
    }

    /// The room, if any, whose bonded set holds `endpoint`.
    fn bonded_in(&self, endpoint: &str) -> Option<String> {
        self.zones
            .iter()
            .find(|z| z.bond.iter().any(|b| b.endpoint == endpoint))
            .map(|z| z.id.clone())
    }

    fn bond(&mut self, at: usize, members: &[BondMember]) -> Result<(), Refusal> {
        let room = self.zones[at].id.clone();
        if self.transports.of(&room) == Transport::Wireless {
            return Err(Refusal::rejected(
                "zone",
                format!(
                    "room '{}' is declared wireless ('--zone {}=wireless'), and a bonded set \
                     holds wired endpoints only (K91; docs/decisions/0024-the-wireless-tier.md): \
                     a room on the wireless tier cannot hold a bond",
                    room, room
                ),
            ));
        }
        for member in members {
            let endpoint = &member.endpoint;
            if !self.zones[at].endpoints.iter().any(|e| e == endpoint) {
                return Err(Refusal::rejected(
                    "members",
                    format!(
                        "endpoint '{}' ({}) is not an endpoint of room '{}', whose endpoints are \
                         {}; a bonded set is made of a room's own endpoints",
                        endpoint,
                        member.role,
                        room,
                        list_or_none(self.zones[at].endpoints.iter().cloned())
                    ),
                ));
            }
            let link = self.link(endpoint);
            if link != Link::Wired {
                return Err(Refusal::rejected(
                    "members",
                    format!(
                        "endpoint '{}' ({}) has a link that is {}, not wired; a bonded set holds \
                         wired endpoints only (K91), because a stereo pair or a theater is held \
                         to the wired tier's bound and a radio is not",
                        endpoint, member.role, link
                    ),
                ));
            }
            if let Some(other) = self.bonded_in(endpoint) {
                if other != room {
                    return Err(Refusal::rejected(
                        "members",
                        format!(
                            "endpoint '{}' ({}) already plays in room '{}''s bonded set",
                            endpoint, member.role, other
                        ),
                    ));
                }
            }
        }
        self.zones[at].bond = members.to_vec();
        Ok(())
    }

    /// Every volume a command or the runtime sets goes through here.
    fn set_volume(&mut self, at: usize, volume: Volume) {
        let zone = &mut self.zones[at];
        zone.volume = volume.min(zone.effective_limit());
    }

    /// The rooms of a formed group, by index, in configured order.
    fn members(&self, group: &str) -> Vec<usize> {
        self.zones
            .iter()
            .enumerate()
            .filter(|(_, z)| z.group == group)
            .map(|(i, _)| i)
            .collect()
    }

    fn formed_members(&self, group: &str) -> Result<Vec<usize>, Refusal> {
        let members = self.members(group);
        if members.is_empty() {
            return Err(Refusal::rejected(
                "group",
                format!(
                    "no room is in a group '{}'; the groups formed now are {}",
                    group,
                    list_or_none(self.formed_groups().into_iter().map(|g| g.id))
                ),
            ));
        }
        Ok(members)
    }

    /// The group volume: the average of its rooms' volumes, rounded half up.
    fn average(&self, members: &[usize]) -> Volume {
        let n = members.len() as u64;
        if n == 0 {
            return Volume::SILENT;
        }
        let sum: u64 = members
            .iter()
            .map(|i| u64::from(self.zones[*i].volume.thousandths()))
            .sum();
        thousandths(((sum + n / 2) / n) as i64)
    }

    /// The group volume of a formed group, or `None` where none is formed.
    pub fn group_volume(&self, group: &str) -> Option<Volume> {
        let members = self.members(group);
        (!members.is_empty()).then(|| self.average(&members))
    }

    /// Set a group volume Sonos-style (`docs/decisions/` records the
    /// definition and its citation): every room is scaled by `wanted / current`
    /// so the balance between rooms is kept, rounded half up, then clamped to
    /// its own effective limit; from a group volume of 0 every room is set to
    /// `wanted`, because there is no balance left to keep.
    fn scale_group(&mut self, members: &[usize], wanted: Volume) {
        let current = u64::from(self.average(members).thousandths());
        let wanted = u64::from(wanted.thousandths());
        for i in members {
            let v = u64::from(self.zones[*i].volume.thousandths());
            // From a group volume of 0 there is no balance to keep.
            let next = (v * wanted + current / 2)
                .checked_div(current)
                .unwrap_or(wanted);
            self.set_volume(*i, thousandths(next.min(u64::from(VOLUME_SCALE)) as i64));
        }
    }

    fn resolve(&self, target: &str) -> Option<Target> {
        if let Some(i) = self.zones.iter().position(|z| z.id == target) {
            return Some(Target::Room(i));
        }
        if self.saved.iter().any(|g| g.id == target) {
            return Some(Target::Saved(target.to_string()));
        }
        if !self.members(target).is_empty() {
            return Some(Target::Formed(target.to_string()));
        }
        None
    }

    /// `join`: room `at` plays in the group `target` (a room or a formed or
    /// saved group) is in.
    fn join(&mut self, at: usize, target: &str) -> Result<(), Refusal> {
        let group = match self.resolve(target) {
            None => return Err(self.no_target(target, "a room or a group")),
            Some(Target::Room(t)) => {
                let group = self.zones[t].group.clone();
                if t != at && group == self.zones[t].id && self.members(&group).len() == 1 {
                    // A room alone in its own group: the two of them form a
                    // live group, with an id nobody chose and nothing can
                    // collide with, playing what the target room was playing.
                    let live = self.fresh_live_id();
                    self.move_source(&group, &live);
                    self.zones[t].group = live.clone();
                    live
                } else {
                    group
                }
            }
            Some(Target::Saved(id)) | Some(Target::Formed(id)) => id,
        };
        let left = self.zones[at].group.clone();
        if left == group {
            return Ok(());
        }
        self.zones[at].group = group;
        self.dissolve_if_alone(&left);
        Ok(())
    }

    /// `take` (K78): every room of the target leaves whatever group it is in
    /// and plays in the target's group. Rooms left behind keep playing what
    /// they were; a group that is not saved and is left with one room
    /// dissolves into that room's own group, still playing.
    fn take(&mut self, target: &str, source: Option<&Source>) -> Result<(), Refusal> {
        let (rooms, group) = match self.resolve(target) {
            None => return Err(self.no_target(target, "a room, a saved group or a formed group")),
            Some(Target::Room(i)) => (vec![i], self.zones[i].id.clone()),
            Some(Target::Saved(id)) => {
                let saved = self
                    .saved
                    .iter()
                    .find(|g| g.id == id)
                    .expect("resolved to a saved group")
                    .zones
                    .clone();
                let rooms = saved
                    .iter()
                    .filter_map(|z| self.zones.iter().position(|r| r.id == *z))
                    .collect();
                (rooms, id)
            }
            Some(Target::Formed(id)) => (self.members(&id), id),
        };
        // Rooms already in the target group that are not the target's own
        // leave it, together, still playing what it played.
        let strangers: Vec<usize> = self
            .members(&group)
            .into_iter()
            .filter(|i| !rooms.contains(i))
            .collect();
        if !strangers.is_empty() {
            let home = if strangers.len() == 1 {
                self.zones[strangers[0]].id.clone()
            } else {
                self.fresh_live_id()
            };
            match self.source_of(&group) {
                // (goal 16) A player plays in one group, so it cannot be
                // copied as any other source is, and (goal 17) neither can
                // a Soloist receiver. It stays with the target, and the
                // rooms pushed out play nothing; unless this take gives the
                // target something else to play, when the source (and what
                // it is playing) goes with the rooms pushed out.
                Some(s) if s.is_exclusive() => {
                    if source.is_none_or(|new| *new == s) {
                        self.set_source(&home, Source::None);
                    } else {
                        self.move_source(&group, &home);
                    }
                }
                Some(s) => self.set_source(&home, s),
                None => {}
            }
            for i in &strangers {
                self.zones[*i].group = home.clone();
            }
        }
        let mut left: Vec<String> = Vec::new();
        for i in &rooms {
            let from = self.zones[*i].group.clone();
            if from != group && !left.contains(&from) {
                left.push(from);
            }
            self.zones[*i].group = group.clone();
        }
        for from in left {
            self.dissolve_if_alone(&from);
        }
        if let Some(source) = source {
            Zones::group_source(source)?;
            self.player_is_free(&group, source)?;
            self.set_source(&group, source.clone());
        }
        Ok(())
    }

    /// (goal 16) Refuse to give `group` a player source another formed group
    /// is playing: a player is one stream of decoded audio, and two groups
    /// playing it would be two listeners of one position nobody chose.
    /// (goal 17) The same for a Soloist receiver.
    fn player_is_free(&self, group: &str, source: &Source) -> Result<(), Refusal> {
        match source {
            Source::Player(id) => match self.player_group(id) {
                Some(other) if other != group => Err(Refusal::rejected(
                    "source",
                    format!(
                        "player '{}' is playing in group '{}', and a player plays in one group \
                         at a time; take '{}' instead, or stop it there first (take '{}' with \
                         source 'none')",
                        id, other, other, other
                    ),
                )),
                _ => Ok(()),
            },
            Source::Soloist(id) => match self.source_group(source) {
                Some(other) if other != group => Err(Refusal::rejected(
                    "source",
                    format!(
                        "receiver '{}' is playing in group '{}', and a receiver plays in one \
                         group at a time",
                        id, other
                    ),
                )),
                _ => Ok(()),
            },
            _ => Ok(()),
        }
    }

    /// (goal 17) The formed group playing exactly `source`, if one is.
    pub fn source_group(&self, source: &Source) -> Option<String> {
        self.sources
            .iter()
            .find(|(g, s)| s == source && !self.members(g).is_empty())
            .map(|(g, _)| g.clone())
    }

    /// (goal 16) The formed group playing `player:<id>`, if one is.
    pub fn player_group(&self, id: &str) -> Option<String> {
        self.sources
            .iter()
            .find(|(g, s)| matches!(s, Source::Player(p) if p == id) && !self.members(g).is_empty())
            .map(|(g, _)| g.clone())
    }

    /// (goal 16) What a formed group's player source is playing now, when the
    /// runtime has said.
    pub fn now_playing(&self, group: &str) -> Option<&NowPlaying> {
        self.now_playing
            .iter()
            .find(|(g, _)| g == group)
            .map(|(_, n)| n)
    }

    /// A group that is not saved and holds one room, not its own, becomes that
    /// room's own group, carrying what it played.
    fn dissolve_if_alone(&mut self, group: &str) {
        let members = self.members(group);
        if members.len() != 1 || self.saved.iter().any(|g| g.id == group) {
            return;
        }
        let own = self.zones[members[0]].id.clone();
        if own == group {
            return;
        }
        self.move_source(group, &own);
        self.zones[members[0]].group = own;
    }

    /// `live-<n>`, the smallest `n` from 1 that names no room, no formed group
    /// and no saved group.
    fn fresh_live_id(&self) -> String {
        let taken = |id: &str| {
            self.zones.iter().any(|z| z.id == id || z.group == id)
                || self.saved.iter().any(|g| g.id == id)
        };
        (1u32..)
            .map(|n| format!("live-{}", n))
            .find(|id| id.len() <= MAX_IDENTIFIER_LEN && !taken(id))
            .expect("fewer groups than numbers")
    }

    fn source_of(&self, group: &str) -> Option<Source> {
        self.sources
            .iter()
            .find(|(g, _)| g == group)
            .map(|(_, s)| s.clone())
    }

    /// What a formed group plays: [`Source::Stream`] unless told otherwise.
    pub fn source(&self, group: &str) -> Source {
        self.source_of(group).unwrap_or(Source::Stream)
    }

    /// Every change of what a group plays goes through here, which is what
    /// keeps a now-playing record only where a player source is, or (goal
    /// 17) a line-in labelled as a streamer, whose record the model writes
    /// itself: the label's name as the title, `via` `streamer`.
    fn set_source(&mut self, group: &str, source: Source) {
        // A record goes with a player source; one a streamer's label wrote
        // does not outlive the line-in it described.
        if !source.takes_now_playing() || !self.source(group).takes_now_playing() {
            self.now_playing.retain(|(g, _)| g != group);
        }
        if let Some(record) = self.streamer_record(&source) {
            self.now_playing.push((group.to_string(), record));
            self.now_playing.sort_by(|a, b| a.0.cmp(&b.0));
        }
        self.sources.retain(|(g, _)| g != group);
        if source != Source::Stream {
            self.sources.push((group.to_string(), source));
            self.sources.sort();
        }
    }

    /// (goal 17) The now-playing record of a line-in labelled as a streamer:
    /// what it plays is the streamer's own business (it reaches chorus as
    /// analogue or S/PDIF audio), so the room shows the label.
    fn streamer_record(&self, source: &Source) -> Option<NowPlaying> {
        let Source::LineIn(input) = source else {
            return None;
        };
        let label = self.input_label(input)?;
        (label.role == InputRole::Streamer).then(|| {
            NowPlaying {
                title: Some(label.name.clone()),
                artist: None,
                album: None,
                art_url: None,
                duration_ms: None,
                state: PlayState::Playing,
                via: VIA_STREAMER.to_string(),
            }
            .bounded()
        })
    }

    /// (goal 17) An input's label changed: every group playing the input
    /// shows the new label, or none.
    fn relabel(&mut self, input: &InputId) {
        let source = Source::LineIn(input.clone());
        let groups: Vec<String> = self
            .sources
            .iter()
            .filter(|(_, s)| *s == source)
            .map(|(g, _)| g.clone())
            .collect();
        let record = self.streamer_record(&source);
        for group in groups {
            self.now_playing.retain(|(g, _)| *g != group);
            if let Some(record) = &record {
                self.now_playing.push((group, record.clone()));
            }
        }
        self.now_playing.sort_by(|a, b| a.0.cmp(&b.0));
    }

    /// Group `to` plays what `from` played, and `from` no longer does; what
    /// it was playing (goal 16) goes with it.
    fn move_source(&mut self, from: &str, to: &str) {
        if let Some(source) = self.source_of(from) {
            let playing = self.now_playing(from).cloned();
            self.sources.retain(|(g, _)| g != from);
            self.now_playing.retain(|(g, _)| g != from);
            self.set_source(to, source);
            if let Some(playing) = playing {
                self.now_playing.retain(|(g, _)| g != to);
                self.now_playing.push((to.to_string(), playing));
                self.now_playing.sort_by(|a, b| a.0.cmp(&b.0));
            }
        }
    }

    /// Drop what names a group that is no longer formed: its source, what it
    /// was playing and its sleep timer. Runs after every change.
    fn prune(&mut self) {
        let formed: Vec<String> = self.zones.iter().map(|z| z.group.clone()).collect();
        self.sources.retain(|(g, _)| formed.contains(g));
        self.now_playing.retain(|(g, _)| formed.contains(g));
        let zones: Vec<String> = self.zones.iter().map(|z| z.id.clone()).collect();
        self.sleep
            .retain(|s| zones.contains(&s.target) || formed.contains(&s.target));
    }

    /// Every formed group, in the order its first room was configured.
    pub fn formed_groups(&self) -> Vec<FormedGroup> {
        let mut out: Vec<FormedGroup> = Vec::new();
        for zone in &self.zones {
            match out.iter_mut().find(|g| g.id == zone.group) {
                Some(g) => g.zones.push(zone.id.clone()),
                None => out.push(FormedGroup {
                    id: zone.group.clone(),
                    kind: GroupKind::Live,
                    zones: vec![zone.id.clone()],
                }),
            }
        }
        for g in &mut out {
            g.kind = if self.saved.iter().any(|s| s.id == g.id) {
                GroupKind::Saved
            } else if g.zones.len() == 1 && g.zones[0] == g.id {
                GroupKind::Room
            } else {
                GroupKind::Live
            };
        }
        out
    }

    /// Whether a saved group is active: every one of its rooms is in it.
    pub fn is_active(&self, saved: &SavedGroup) -> bool {
        saved
            .zones
            .iter()
            .all(|z| self.zone(z).is_some_and(|r| r.group == saved.id))
    }

    // --- the runtime's hooks ---------------------------------------------

    /// Say what civil time it is, which decides which quiet-hours windows are
    /// active, and clamp every room to its effective limit. `None` makes every
    /// window inactive (a server with no time source). Returns whether
    /// anything changed; the serial moves only when it did.
    pub fn set_civil_time(&mut self, now: Option<CivilTime>) -> bool {
        self.now = now;
        let mut changed = false;
        for zone in &mut self.zones {
            let active: Vec<bool> = zone
                .quiet
                .iter()
                .map(|w| now.is_some_and(|t| w.contains(t)))
                .collect();
            changed |= set_quiet(zone, active);
        }
        if changed {
            self.serial += 1;
        }
        changed
    }

    /// Say directly which of a room's quiet-hours windows are active (one flag
    /// per window, in order), for a runtime that decides it with
    /// `crates/schedule` rather than through [`Zones::set_civil_time`]. Clamps.
    pub fn set_active_quiet(&mut self, zone: &str, active: &[bool]) -> Result<bool, Refusal> {
        let at = self.index(zone)?;
        if active.len() != self.zones[at].quiet.len() {
            return Err(Refusal::rejected(
                "windows",
                format!(
                    "room '{}' has {} quiet-hours windows and {} flags were given",
                    zone,
                    self.zones[at].quiet.len(),
                    active.len()
                ),
            ));
        }
        let changed = set_quiet(&mut self.zones[at], active.to_vec());
        if changed {
            self.serial += 1;
        }
        Ok(changed)
    }

    /// Say what a formed group plays. A player source another formed group is
    /// playing is refused by name (goal 16); a source that is not a player
    /// source clears the group's now-playing record.
    pub fn set_group_source(&mut self, group: &str, source: Source) -> Result<(), Refusal> {
        self.formed_members(group)?;
        Zones::group_source(&source)?;
        self.player_is_free(group, &source)?;
        self.set_source(group, source);
        self.serial += 1;
        Ok(())
    }

    /// (goal 16) Say what a formed group's player source is playing now, or
    /// (`None`) that nothing is known. The record is held to its bounds
    /// ([`NowPlaying::bounded`]). Refused by name, with nothing changed, for a
    /// group that is not formed, a group whose source is not a player source
    /// (a record describes what a player plays), and a `via` that is not an
    /// identifier. Returns whether anything changed; the serial moves only
    /// when it did, so a runtime that says the same thing again fans nothing
    /// out.
    pub fn set_now_playing(
        &mut self,
        group: &str,
        playing: Option<NowPlaying>,
    ) -> Result<bool, Refusal> {
        self.formed_members(group)?;
        let playing = match playing {
            None => None,
            Some(record) => {
                let source = self.source(group);
                if !source.takes_now_playing() {
                    return Err(Refusal::rejected(
                        "group",
                        format!(
                            "group '{}' plays '{}', which is not a player source; a now-playing \
                             record says what a player source ('player:<id>') is playing",
                            group,
                            source.literal()
                        ),
                    ));
                }
                if !is_identifier(&record.via) {
                    return Err(Refusal::rejected(
                        "via",
                        format!(
                            "'{}' is not an identifier; 'via' names what drives the player \
                             (for example 'upnp')",
                            record.via.escape_debug()
                        ),
                    ));
                }
                Some(record.bounded())
            }
        };
        if self.now_playing(group) == playing.as_ref() {
            return Ok(false);
        }
        self.now_playing.retain(|(g, _)| g != group);
        if let Some(record) = playing {
            self.now_playing.push((group.to_string(), record));
            self.now_playing.sort_by(|a, b| a.0.cmp(&b.0));
        }
        self.serial += 1;
        Ok(true)
    }

    /// Start a ramp in a room towards `target`, clamped to its effective limit
    /// (and kept clamped if the limit falls while it runs). The runtime then
    /// samples its ramp and sets each step with [`Zones::runtime_volume`].
    pub fn start_ramp(&mut self, zone: &str, target: Volume) -> Result<Volume, Refusal> {
        let at = self.index(zone)?;
        let target = target.min(self.zones[at].effective_limit());
        self.zones[at].ramp = Some(target);
        self.serial += 1;
        Ok(target)
    }

    /// End a room's ramp, wherever its volume got to.
    pub fn stop_ramp(&mut self, zone: &str) -> Result<(), Refusal> {
        let at = self.index(zone)?;
        self.zones[at].ramp = None;
        self.serial += 1;
        Ok(())
    }

    /// Set a room's volume from the runtime (a ramp step, a sleep fade, an
    /// alarm's volume), clamped like every other path. Returns what was set.
    pub fn runtime_volume(&mut self, zone: &str, volume: Volume) -> Result<Volume, Refusal> {
        let at = self.index(zone)?;
        self.set_volume(at, volume);
        self.serial += 1;
        Ok(self.zones[at].volume)
    }

    /// Record an alarm as ringing, or as not.
    pub fn set_alarm_ringing(&mut self, alarm: &str, ringing: bool) -> Result<(), Refusal> {
        self.alarm_exists(alarm)?;
        self.ringing.retain(|a| a != alarm);
        if ringing {
            self.ringing.push(alarm.to_string());
            self.ringing.sort();
        }
        self.serial += 1;
        Ok(())
    }

    /// Remove a sleep timer the runtime counted down. Returns whether there
    /// was one.
    pub fn sleep_expired(&mut self, target: &str) -> bool {
        let before = self.sleep.len();
        self.sleep.retain(|s| s.target != target);
        let changed = self.sleep.len() != before;
        if changed {
            self.serial += 1;
        }
        changed
    }

    /// (ADR 0194) The built-in chimes the server can ring, for the state's
    /// `chimes`: what an alarm's `chime:<name>` may name. Set once, at start,
    /// before any state is served, so the serial does not move.
    pub fn set_chimes(&mut self, chimes: Vec<String>) {
        self.chimes = chimes;
    }

    /// (ADR 0194) The built-in chimes the server said it can ring.
    pub fn chimes(&self) -> &[String] {
        &self.chimes
    }

    /// (ADR 0194) Say a schedule runtime counts the sleep timers down here:
    /// each one then carries `remaining_s`, all of it at the instant it is
    /// asked for and the runtime's count after ([`Zones::sleep_remaining`]).
    /// Set once, at start, before any state is served, so the serial does
    /// not move.
    pub fn set_sleep_counted(&mut self, counted: bool) {
        self.sleep_counted = counted;
    }

    /// (ADR 0194) The runtime's count of a sleep timer: `seconds` whole
    /// seconds are left, rounded up. The count is kept every time, so a
    /// state read at any instant says it to the second; the serial moves,
    /// and so a state is sent, only when the whole minutes left (rounded up)
    /// change, which is at most once a minute for each timer. Returns
    /// whether the serial moved. Nothing happens in a model no runtime
    /// counts ([`Zones::set_sleep_counted`]) or for a target with no timer.
    pub fn sleep_remaining(&mut self, target: &str, seconds: u32) -> bool {
        if !self.sleep_counted {
            return false;
        }
        let Some(timer) = self.sleep.iter_mut().find(|s| s.target == target) else {
            return false;
        };
        let before = timer.remaining_s;
        timer.remaining_s = Some(seconds);
        let moved = before.map(|s| s.div_ceil(60)) != Some(seconds.div_ceil(60));
        if moved {
            self.serial += 1;
        }
        moved
    }

    /// (ADR 0194) Say what kind an input is, for the state's `input_kinds`.
    /// The server says it before it offers the input. Returns whether the
    /// state changed: the kind is new or different and the input is offered
    /// now.
    pub fn set_input_kind(&mut self, input: &InputId, kind: InputKind) -> bool {
        if self.input_kind(input) == Some(kind) {
            return false;
        }
        upsert(&mut self.input_kinds, (input.clone(), kind), |(i, _)| {
            i.clone()
        });
        let shown = self.inputs.contains(input);
        if shown {
            self.serial += 1;
        }
        shown
    }

    /// (ADR 0194) The kind the server said an input is.
    pub fn input_kind(&self, input: &InputId) -> Option<InputKind> {
        self.input_kinds
            .iter()
            .find(|(i, _)| i == input)
            .map(|(_, k)| *k)
    }

    /// Say an input is offered (its signal is present). Returns whether it was
    /// not already.
    pub fn offer_input(&mut self, input: InputId) -> bool {
        if self.inputs.contains(&input) {
            return false;
        }
        self.inputs.push(input);
        self.inputs.sort();
        self.serial += 1;
        true
    }

    /// Say an input is no longer offered. Returns whether it was.
    pub fn withdraw_input(&mut self, input: &InputId) -> bool {
        let before = self.inputs.len();
        self.inputs.retain(|i| i != input);
        // (ADR 0194) Its kind goes with it: the server says it again before
        // the next offer.
        self.input_kinds.retain(|(i, _)| i != input);
        let changed = self.inputs.len() != before;
        if changed {
            self.serial += 1;
        }
        changed
    }

    // --- the session layer's hooks (goal 14) --------------------------------

    /// The adopted speakers and the key changes refused.
    pub fn speakers(&self) -> &Speakers {
        &self.speakers
    }

    /// Install a persisted speaker record, which loading persisted state
    /// does. The caller has validated it; its room's membership is made to
    /// agree with it, as `speaker_room` leaves it.
    pub fn restore_speaker(&mut self, speaker: Speaker) {
        if let Some(room) = &speaker.room {
            if let Some(zone) = self.zones.iter_mut().find(|z| z.id == *room) {
                if !zone.endpoints.contains(&speaker.id) {
                    zone.endpoints.push(speaker.id.clone());
                }
            }
        }
        self.speakers.restore(speaker);
    }

    /// Say that `id` is adopted (its key is pinned) and what its key's
    /// fingerprint is: the record is created, unnamed and in no room, if
    /// there is none. `Ok(true)` when a record was created, which is what the
    /// caller persists on; an id that cannot be listed is refused by name and
    /// changes nothing.
    pub fn speaker_adopted(&mut self, id: &str, key: &str) -> Result<bool, NotListed> {
        let created = self.speakers.adopt(id)?;
        let told = self.speakers.set_now(id, |now| now.key = key.to_string());
        if created || told {
            self.serial += 1;
        }
        Ok(created)
    }

    /// Say that a session of speaker `id` is up, with what its `hello` said.
    /// A speaker with an assigned room is marked present in it. Whether
    /// anything changed; nothing does for an id with no record.
    pub fn speaker_session_up(&mut self, id: &str, software: &str, roles: &[String]) -> bool {
        let mut changed = self.speakers.set_now(id, |now| {
            now.sessions = now.sessions.saturating_add(1);
            now.software = software.to_string();
            now.roles = roles.to_vec();
        });
        let room = self.speakers.get(id).and_then(|s| s.room.clone());
        if let Some(zone) = room.and_then(|r| self.zones.iter_mut().find(|z| z.id == r)) {
            if !zone.endpoints.iter().any(|e| e == id) {
                zone.endpoints.push(id.to_string());
                changed = true;
            }
            if !zone.present.iter().any(|e| e == id) {
                zone.present.push(id.to_string());
                changed = true;
            }
        }
        if changed {
            self.serial += 1;
        }
        changed
    }

    /// Say that a session of speaker `id` has ended. When it was the last
    /// one, a speaker with an assigned room is marked absent from it (an
    /// unassigned one's presence is its control client's, as before goal 14).
    pub fn speaker_session_down(&mut self, id: &str) -> bool {
        let mut changed = self.speakers.set_now(id, |now| {
            now.sessions = now.sessions.saturating_sub(1);
            // A gate is a session's word: with no session there is none, and
            // no word reads as muted.
            if now.sessions == 0 {
                now.mic_live = false;
            }
        });
        let gone = self
            .speakers
            .get(id)
            .is_some_and(|s| s.room.is_some() && !s.now.present());
        if gone {
            for zone in &mut self.zones {
                let before = zone.present.len();
                zone.present.retain(|e| e != id);
                changed |= zone.present.len() != before;
            }
        }
        if changed {
            self.serial += 1;
        }
        changed
    }

    /// Say what the mic gate of speaker `id` is, from its own `mic_state`
    /// (the hardware switch, 0063; the protocol has no message that opens
    /// it): `live` or muted. Never persisted, and back to muted when the
    /// speaker's last session ends. Whether anything changed; nothing does
    /// for an id with no record.
    pub fn speaker_mic_gate(&mut self, id: &str, live: bool) -> bool {
        let changed = self.speakers.set_now(id, |now| now.mic_live = live);
        if changed {
            self.serial += 1;
        }
        changed
    }

    /// Whether room `zone` hears nothing: `true` unless a speaker present in
    /// it has reported its mic gate live. A room with no microphone, and one
    /// whose microphones have said nothing yet, is muted.
    pub fn mic_muted(&self, zone: &Zone) -> bool {
        !zone.present.iter().any(|e| {
            self.speakers
                .get(e)
                .is_some_and(|s| s.now.present() && s.now.mic_live)
        })
    }

    /// The staged firmware images as the server graded them, or `None` when
    /// it has no firmware directory. Whether anything changed; the serial
    /// moves only when it did.
    pub fn set_firmware_images(&mut self, images: Option<Vec<Image>>) -> bool {
        if self.firmware_images == images {
            return false;
        }
        self.firmware_images = images;
        self.serial += 1;
        true
    }

    /// (goal 17) The Soloist receivers, their build and its warning, or
    /// `None` on a server that runs no receiver. Whether anything changed;
    /// the serial moves only when it did.
    pub fn set_soloist(&mut self, soloist: Option<SoloistState>) -> bool {
        if self.soloist == soloist {
            return false;
        }
        self.soloist = soloist;
        self.serial += 1;
        true
    }

    /// (voice, P8) Whether a voice run may open in room `zone` now, or the
    /// refusal that says why not, by name: `voice-disabled` for a room whose
    /// voice path is switched off, `mic-muted` for one with no microphone
    /// reporting its gate live (a room with no microphone at all included).
    pub fn voice_start_check(&self, zone: &str) -> Result<(), Refusal> {
        let room = &self.zones[self.index(zone)?];
        if !room.voice_enabled {
            return Err(Refusal::rejected(
                "zone",
                format!(
                    "voice-disabled: room '{}' has voice switched off, so nothing listens \
                     there; voice_enabled switches it on",
                    zone
                ),
            ));
        }
        if self.mic_muted(room) {
            return Err(Refusal::rejected(
                "zone",
                format!(
                    "mic-muted: no microphone in room '{}' reports its gate live (the mute \
                     switch is the speaker's own, and no command opens it)",
                    zone
                ),
            ));
        }
        Ok(())
    }

    /// (voice, P8) The wake-word models the server runs, for the state's
    /// `wake_words`. Set once, at start, before any state is served, so the
    /// serial does not move. Never persisted: it is a fact about the build.
    pub fn set_wake_words(&mut self, wake_words: Vec<WakeWord>) {
        self.wake_words = wake_words;
    }

    /// (voice, P8) The wake-word models the server runs.
    pub fn wake_words(&self) -> &[WakeWord] {
        &self.wake_words
    }

    /// (voice) Whether room `zone` listens for the wake word spelled
    /// `phrase`: every room does until a `voice_wake_words` chose for it,
    /// and then only for the models it named. A room this server does not
    /// have listens for nothing.
    pub fn listens_for(&self, zone: &str, phrase: &str) -> bool {
        let Some(room) = self.zones.iter().find(|z| z.id == zone) else {
            return false;
        };
        match &room.wake_words {
            None => true,
            Some(chosen) => self
                .wake_words
                .iter()
                .any(|w| w.phrase == phrase && chosen.contains(&w.id)),
        }
    }

    /// (goal 18) The origins an `announce` URL may come from. Set once, at
    /// start; the state does not carry them, so the serial does not move.
    pub fn set_announce_origins(&mut self, origins: Vec<Origin>) {
        self.announce_origins = origins;
    }

    /// (goal 18) The origins an `announce` URL may come from.
    pub fn announce_origins(&self) -> &[Origin] {
        &self.announce_origins
    }

    /// (goal 18) Whether an `announce` naming `target` and `url` may be
    /// played: the target is a room, a saved group or a formed group, and
    /// the URL's scheme, host and port are those of a configured origin.
    /// Refused by name otherwise, the target first.
    pub fn announce_check(&self, target: &str, url: &str) -> Result<(), Refusal> {
        if self.resolve(target).is_none() {
            return Err(self.no_target(target, "a room, a saved group or a formed group"));
        }
        // An alarm that is ringing keeps its rooms: an announcement would
        // take the group from the alarm's own source, and an alarm must
        // still wake.
        let hearers = self.announce_rooms(target);
        for alarm in &self.alarms {
            if !self.ringing.contains(&alarm.id) {
                continue;
            }
            let rings_in = self.announce_rooms(&alarm.target);
            if let Some(room) = hearers.iter().find(|r| rings_in.contains(r)) {
                return Err(Refusal::rejected(
                    "target",
                    format!(
                        "alarm '{}' is ringing in room '{}'; an announcement does not interrupt \
                         a ringing alarm (stop it with alarm_stop, or announce when it has \
                         ended)",
                        alarm.id, room
                    ),
                ));
            }
        }
        if self.announce_origins.is_empty() {
            return Err(Refusal::rejected(
                "url",
                "no-announce-origin: this server was started without --announce-origin, so it \
                 announces no URL; start it with --announce-origin <scheme://host[:port]>, the \
                 home automation's own address"
                    .to_string(),
            ));
        }
        let listed = || {
            self.announce_origins
                .iter()
                .map(Origin::literal)
                .collect::<Vec<_>>()
                .join(", ")
        };
        match Origin::of_url(url) {
            Ok(origin) if self.announce_origins.contains(&origin) => Ok(()),
            Ok(origin) => Err(Refusal::rejected(
                "url",
                format!(
                    "the origin '{}' is not one this server announces from; its announce \
                     origins are {}",
                    origin.literal(),
                    listed()
                ),
            )),
            Err(why) => Err(Refusal::rejected(
                "url",
                format!(
                    "the URL has no origin this server can compare: {}; its announce origins \
                     are {}",
                    why,
                    listed()
                ),
            )),
        }
    }

    /// (goal 18) The formed group an `announce` naming `target` would play
    /// in as things stand: the group a room is in, a saved group's id when
    /// it is formed, a formed group itself. `None` for an unknown target and
    /// for a saved group none of whose rooms is in it yet.
    pub fn announce_group(&self, target: &str) -> Option<String> {
        let group = match self.resolve(target)? {
            Target::Room(i) => self.zones[i].group.clone(),
            Target::Saved(id) | Target::Formed(id) => id,
        };
        (!self.members(&group).is_empty()).then_some(group)
    }

    /// The rooms that would hear an announcement to `target`, or that an
    /// alarm with that target rings in: a room and whoever shares its
    /// group, a saved group's rooms and whoever is in it now, a formed
    /// group's rooms.
    fn announce_rooms(&self, target: &str) -> Vec<String> {
        let mut rooms: Vec<String> = Vec::new();
        let group = match self.resolve(target) {
            None => return rooms,
            Some(Target::Room(i)) => self.zones[i].group.clone(),
            Some(Target::Saved(id)) => {
                if let Some(saved) = self.saved.iter().find(|g| g.id == id) {
                    rooms.extend(saved.zones.iter().cloned());
                }
                id
            }
            Some(Target::Formed(id)) => id,
        };
        for at in self.members(&group) {
            if !rooms.contains(&self.zones[at].id) {
                rooms.push(self.zones[at].id.clone());
            }
        }
        rooms
    }

    /// (goal 18) Start an announcement the checks have passed: the target's
    /// group plays `player` (a `player:<id>` the server took for the clip),
    /// and with `volume` every room of the group is set to it, clamped to
    /// the room's effective limit like every volume path. A saved group that
    /// is not active is taken first (K78), as any play on a saved group
    /// does; a room plays in the group it is in, and so does everybody else
    /// in that group. Returns what was changed, for the end to put back.
    /// Refused by name, with nothing changed, for an unknown target and for
    /// a player another group is playing.
    pub fn announce_begin(
        &mut self,
        target: &str,
        player: Source,
        volume: Option<Volume>,
    ) -> Result<Announced, Refusal> {
        let mut next = self.clone();
        let group = match next.resolve(target) {
            None => return Err(self.no_target(target, "a room, a saved group or a formed group")),
            Some(Target::Room(i)) => next.zones[i].group.clone(),
            Some(Target::Formed(id)) => id,
            Some(Target::Saved(id)) => {
                let active = next
                    .saved
                    .iter()
                    .find(|g| g.id == id)
                    .is_some_and(|g| next.is_active(g));
                if !active {
                    next.take(target, None)?;
                }
                id
            }
        };
        let members = next.formed_members(&group)?;
        let previous = next.source(&group);
        Zones::group_source(&player)?;
        next.player_is_free(&group, &player)?;
        next.set_source(&group, player);
        let rooms = members
            .iter()
            .map(|at| next.zones[*at].id.clone())
            .collect();
        let mut volumes = Vec::new();
        if let Some(volume) = volume {
            for at in members {
                let before = next.zones[at].volume;
                next.set_volume(at, volume);
                volumes.push((next.zones[at].id.clone(), before, next.zones[at].volume));
            }
        }
        next.prune();
        next.serial += 1;
        *self = next;
        Ok(Announced {
            group,
            previous,
            rooms,
            volumes,
        })
    }

    /// (ADR 0175) The rooms an announcement to `target` is mixed over: a
    /// room alone, whatever group it is in; every room of a formed group;
    /// a saved group's rooms (the ones in it now when it is active, the
    /// ones it lists when it has yet to be taken). Empty for an unknown
    /// target.
    pub fn announce_over_rooms(&self, target: &str) -> Vec<String> {
        let ids = |members: Vec<usize>| -> Vec<String> {
            members
                .iter()
                .map(|at| self.zones[*at].id.clone())
                .collect()
        };
        match self.resolve(target) {
            None => Vec::new(),
            Some(Target::Room(i)) => vec![self.zones[i].id.clone()],
            Some(Target::Formed(id)) => ids(self.members(&id)),
            Some(Target::Saved(id)) => match self.saved.iter().find(|g| g.id == id) {
                Some(saved) if !self.is_active(saved) => saved.zones.clone(),
                _ => ids(self.members(&id)),
            },
        }
    }

    /// (ADR 0175) What the group an announcement to `target` would play in
    /// plays now, or `None` when there is no such group yet.
    pub fn announce_source(&self, target: &str) -> Option<Source> {
        self.announce_group(target).map(|group| self.source(&group))
    }

    /// (ADR 0175) Start an announcement that is mixed over what its rooms
    /// play: no group's source changes. A saved group that is not active is
    /// taken first (K78), as any play on a saved group does, and with
    /// `volume` every room that hears the clip
    /// ([`Zones::announce_over_rooms`]) is set to it, clamped to the room's
    /// effective limit like every volume path. Returns the group the rooms
    /// are in, what it plays, the rooms and the volumes changed, for the
    /// end to put back. Refused by name, with nothing changed, for an
    /// unknown target.
    pub fn announce_over_begin(
        &mut self,
        target: &str,
        volume: Option<Volume>,
    ) -> Result<Announced, Refusal> {
        let mut next = self.clone();
        let (group, only) = match next.resolve(target) {
            None => return Err(self.no_target(target, "a room, a saved group or a formed group")),
            Some(Target::Room(i)) => (next.zones[i].group.clone(), Some(i)),
            Some(Target::Formed(id)) => (id, None),
            Some(Target::Saved(id)) => {
                let active = next
                    .saved
                    .iter()
                    .find(|g| g.id == id)
                    .is_some_and(|g| next.is_active(g));
                if !active {
                    next.take(target, None)?;
                }
                (id, None)
            }
        };
        let members = match only {
            Some(i) => vec![i],
            None => next.formed_members(&group)?,
        };
        let previous = next.source(&group);
        let rooms = members
            .iter()
            .map(|at| next.zones[*at].id.clone())
            .collect();
        let mut volumes = Vec::new();
        if let Some(volume) = volume {
            for at in members {
                let before = next.zones[at].volume;
                next.set_volume(at, volume);
                volumes.push((next.zones[at].id.clone(), before, next.zones[at].volume));
            }
        }
        next.prune();
        next.serial += 1;
        *self = next;
        Ok(Announced {
            group,
            previous,
            rooms,
            volumes,
        })
    }

    /// (ADR 0175) Whether an announcement mixed over `rooms`, started in
    /// `group`, still has them: which of them are still in that group,
    /// whether an alarm rings in any of them, and what the group plays.
    pub fn announce_watch(&self, rooms: &[String], group: &str) -> AnnounceWatch {
        let here: Vec<String> = rooms
            .iter()
            .filter(|room| self.zone(room).is_some_and(|z| z.group == group))
            .cloned()
            .collect();
        let ringing = self
            .alarms
            .iter()
            .filter(|alarm| self.ringing.contains(&alarm.id))
            .find(|alarm| {
                let rings_in = self.announce_rooms(&alarm.target);
                rooms.iter().any(|room| rings_in.contains(room))
            })
            .map(|alarm| alarm.id.clone());
        AnnounceWatch {
            here,
            ringing,
            source: self.source(group),
        }
    }

    /// (ADR 0175) The announcements the state lists. Whether anything
    /// changed; the serial moves only when it did.
    pub fn set_announcements(&mut self, announcements: Vec<Announcement>) -> bool {
        if self.announcements == announcements {
            return false;
        }
        self.announcements = announcements;
        self.serial += 1;
        true
    }

    /// (ADR 0175) The announcements the state lists.
    pub fn announcements(&self) -> &[Announcement] {
        &self.announcements
    }

    /// (ADR 0000) Whether a measurement sweep may play in room `zone` now,
    /// or the refusal that says why not, by name: an unknown room (`zone`),
    /// `measuring` for a room that is playing a sweep already and for any
    /// room while another plays one (a server plays one sweep at a time),
    /// `no-speaker` for a room no endpoint is attached to, `muted` for a
    /// muted room, and `alarm-ringing` for a room an alarm rings in.
    pub fn measure_check(&self, zone: &str) -> Result<(), Refusal> {
        let room = &self.zones[self.index(zone)?];
        if let Some(playing) = self
            .measurement
            .as_ref()
            .filter(|m| m.state == MeasurementState::Playing)
        {
            return Err(Refusal::rejected(
                "zone",
                if playing.zone == zone {
                    format!(
                        "measuring: room '{}' is playing measurement sweep {} already; it lasts \
                         {} ms and ends by itself",
                        zone,
                        playing.id,
                        playing.lead_ms + playing.sweep_ms + playing.tail_ms
                    )
                } else {
                    format!(
                        "measuring: room '{}' is playing measurement sweep {} and a server \
                         plays one sweep at a time; it lasts {} ms and ends by itself",
                        playing.zone,
                        playing.id,
                        playing.lead_ms + playing.sweep_ms + playing.tail_ms
                    )
                },
            ));
        }
        if room.present.is_empty() {
            return Err(Refusal::rejected(
                "zone",
                format!(
                    "no-speaker: no speaker is attached to room '{}', so the server cannot play \
                     a sweep there",
                    zone
                ),
            ));
        }
        if room.muted {
            return Err(Refusal::rejected(
                "zone",
                format!(
                    "muted: room '{}' is muted, so a sweep there would be silence; unmute it \
                     first",
                    zone
                ),
            ));
        }
        for alarm in &self.alarms {
            if self.ringing.contains(&alarm.id)
                && self.announce_rooms(&alarm.target).iter().any(|r| r == zone)
            {
                return Err(Refusal::rejected(
                    "zone",
                    format!(
                        "alarm-ringing: alarm '{}' is ringing in room '{}'; a sweep does not \
                         interrupt a ringing alarm (stop it with alarm_stop, or measure when it \
                         has ended)",
                        alarm.id, zone
                    ),
                ));
            }
        }
        Ok(())
    }

    /// (ADR 0000) Start a measurement sweep in room `zone`: with `volume`
    /// the room is set to it, clamped to the room's effective limit like
    /// every volume path, and the state names the sweep as `playing`, at the
    /// volume the room has now and with the lengths the server gave
    /// (`lead_ms`, `sweep_ms`, `tail_ms`). No source and no group changes.
    /// Returns the sweep's number and the volumes, for the end to put back.
    /// Refused by name, with nothing changed, as [`Zones::measure_check`]
    /// refuses.
    pub fn measure_begin(
        &mut self,
        zone: &str,
        volume: Option<Volume>,
        lengths_ms: (u64, u64, u64),
    ) -> Result<Measured, Refusal> {
        self.measure_check(zone)?;
        let at = self.index(zone)?;
        let before = self.zones[at].volume;
        if let Some(volume) = volume {
            self.set_volume(at, volume);
        }
        let set = self.zones[at].volume;
        let id = self.measurement.as_ref().map_or(0, |m| m.id) + 1;
        self.measurement = Some(Measurement {
            id,
            zone: zone.to_string(),
            state: MeasurementState::Playing,
            volume: set,
            lead_ms: lengths_ms.0,
            sweep_ms: lengths_ms.1,
            tail_ms: lengths_ms.2,
            reason: None,
        });
        self.serial += 1;
        Ok(Measured { id, before, set })
    }

    /// (ADR 0000) The end of measurement sweep `id`: the state says how it
    /// ended (`finished`, or `cancelled` with `reason`), and the room's
    /// volume goes back to `before` when it is still the one the sweep set
    /// (`set`), clamped like every volume; a volume somebody changed
    /// meanwhile is left alone. Whether anything changed; nothing does for
    /// a sweep that is not the one playing.
    pub fn measure_end(
        &mut self,
        id: u64,
        state: MeasurementState,
        reason: Option<String>,
        before: Volume,
        set: Volume,
    ) -> bool {
        let Some(playing) = self
            .measurement
            .as_mut()
            .filter(|m| m.id == id && m.state == MeasurementState::Playing)
        else {
            return false;
        };
        playing.state = state;
        playing.reason = reason;
        let zone = playing.zone.clone();
        if let Ok(at) = self.index(&zone) {
            if self.zones[at].volume == set {
                self.set_volume(at, before);
            }
        }
        self.serial += 1;
        true
    }

    /// (ADR 0000) The alarm ringing in room `zone`, if any: what calls a
    /// sweep off.
    pub fn measure_watch(&self, zone: &str) -> Option<String> {
        self.alarms
            .iter()
            .filter(|alarm| self.ringing.contains(&alarm.id))
            .find(|alarm| self.announce_rooms(&alarm.target).iter().any(|r| r == zone))
            .map(|alarm| alarm.id.clone())
    }

    /// (ADR 0000) The measurement sweep the state names.
    pub fn measurement(&self) -> Option<&Measurement> {
        self.measurement.as_ref()
    }

    /// (goal 17) What the receiver manager last said, if this server runs
    /// receivers.
    pub fn soloist(&self) -> Option<&SoloistState> {
        self.soloist.as_ref()
    }

    fn has_receivers(&self) -> Result<(), Refusal> {
        if self.soloist.is_some() {
            return Ok(());
        }
        Err(Refusal::rejected(
            "t",
            "no-receivers: this server was started without --soloist-receivers, so there is \
             no Spotify receiver to tell"
                .to_string(),
        ))
    }

    /// (goal 17) The formed group a `playback` command naming `target`
    /// reaches: the group the room is in, the saved group when it is active,
    /// or the formed group itself. Refused by name for a target that is
    /// none of those and for a group that does not play a Spotify receiver.
    pub fn playback_group(&self, target: &str) -> Result<String, Refusal> {
        let group = match self.resolve(target) {
            None => return Err(self.no_target(target, "a room, a saved group or a formed group")),
            Some(Target::Room(i)) => self.zones[i].group.clone(),
            Some(Target::Saved(id)) | Some(Target::Formed(id)) => id,
        };
        let source = if self.members(&group).is_empty() {
            Source::None
        } else {
            self.source(&group)
        };
        if source.is_soloist() {
            return Ok(group);
        }
        Err(Refusal::rejected(
            "target",
            format!(
                "'{}' plays '{}', which is not a Spotify receiver; a playback command (pause, \
                 resume, next, previous) goes to the receiver a group is playing",
                target,
                source.literal()
            ),
        ))
    }

    /// The staged firmware images, or `None` when the server has no firmware
    /// directory.
    pub fn firmware_images(&self) -> Option<&[Image]> {
        self.firmware_images.as_deref()
    }

    /// Change what is known about a speaker now ([`crate::speakers::SpeakerNow`]):
    /// the hook a later per-speaker runtime fact is set through. Whether
    /// anything changed; the serial moves only when it did.
    pub fn speaker_now(
        &mut self,
        id: &str,
        change: impl FnOnce(&mut crate::speakers::SpeakerNow),
    ) -> bool {
        let changed = self.speakers.set_now(id, change);
        if changed {
            self.serial += 1;
        }
        changed
    }

    /// Record a handshake refused because the key under an adopted id
    /// changed. The pin is not this model's and is not touched.
    pub fn speaker_key_changed(&mut self, change: KeyChange) -> bool {
        let changed = self.speakers.key_changed(change);
        if changed {
            self.serial += 1;
        }
        changed
    }

    fn zone_list(&self) -> String {
        if self.zones.is_empty() {
            return "none: this server has no zone configured".to_string();
        }
        self.zones
            .iter()
            .map(|z| z.id.clone())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The state message, at the build's own catalog version.
    pub fn state_value(&self) -> Value {
        self.state_value_at(crate::catalog::CATALOG_VERSION)
    }

    /// The state message as catalog version `version` declares it: `1` is the
    /// v1 shape, byte for byte what a v1 build sent (for `?v=1` peers and the
    /// v1 vectors), and anything else is the v2 shape.
    pub fn state_value_at(&self, version: i64) -> Value {
        if version == 1 {
            return self.state_value_v1();
        }
        let zones = self.zones.iter().map(|z| self.zone_value_v2(z)).collect();
        let groups = self
            .formed_groups()
            .into_iter()
            .map(|g| {
                let mut group = vec![
                    ("id".to_string(), Value::text(&g.id)),
                    ("kind".to_string(), Value::text(g.kind.name())),
                    ("zones".to_string(), texts(&g.zones)),
                    (
                        "volume".to_string(),
                        Value::Num(self.group_volume(&g.id).unwrap_or(Volume::SILENT).literal()),
                    ),
                    (
                        "source".to_string(),
                        Value::text(&self.source(&g.id).literal()),
                    ),
                    ("audio".to_string(), Value::text(self.audio_for(&g.id))),
                ];
                // (goal 16) Written only when there is something to say, so
                // a group with no player source says the bytes it said before
                // and the committed state vectors did not move.
                if let Some(playing) = self.now_playing(&g.id) {
                    group.push(("now_playing".to_string(), now_playing_value(playing)));
                }
                Value::Obj(group)
            })
            .collect();
        let saved = self
            .saved
            .iter()
            .map(|g| {
                Value::Obj(vec![
                    ("id".to_string(), Value::text(&g.id)),
                    ("name".to_string(), Value::text(&g.name)),
                    ("zones".to_string(), texts(&g.zones)),
                    ("active".to_string(), Value::Bool(self.is_active(g))),
                ])
            })
            .collect();
        let mut endpoints: Vec<String> = self.links.iter().map(|(e, _)| e.clone()).collect();
        for zone in &self.zones {
            endpoints.extend(zone.endpoints.iter().cloned());
        }
        endpoints.sort();
        endpoints.dedup();
        let endpoints = endpoints
            .iter()
            .map(|e| {
                Value::Obj(vec![
                    ("id".to_string(), Value::text(e)),
                    ("link".to_string(), Value::text(self.link(e).name())),
                ])
            })
            .collect();
        let alarms = self
            .alarms
            .iter()
            .map(|a| {
                let mut v = alarm_value(a);
                if let Value::Obj(fields) = &mut v {
                    fields.push(("ringing".to_string(), Value::Bool(self.is_ringing(&a.id))));
                }
                v
            })
            .collect();
        let sleep = self
            .sleep
            .iter()
            .map(|s| {
                let mut timer = vec![
                    ("target".to_string(), Value::text(&s.target)),
                    ("minutes".to_string(), Value::int(i64::from(s.minutes))),
                ];
                // (ADR 0194) Written only where a runtime counts the timer
                // down, so a model nothing counts says the bytes it said
                // before and the committed state vectors did not move.
                if let Some(remaining) = s.remaining_s {
                    timer.push(("remaining_s".to_string(), Value::int(i64::from(remaining))));
                }
                Value::Obj(timer)
            })
            .collect();
        let mut state = vec![
            ("v".to_string(), Value::int(version)),
            ("t".to_string(), Value::text("state")),
            ("serial".to_string(), Value::int(self.serial as i64)),
            ("zones".to_string(), Value::Arr(zones)),
            ("groups".to_string(), Value::Arr(groups)),
            ("saved_groups".to_string(), Value::Arr(saved)),
            ("endpoints".to_string(), Value::Arr(endpoints)),
            ("alarms".to_string(), Value::Arr(alarms)),
            ("sleep".to_string(), Value::Arr(sleep)),
            (
                "autoplay".to_string(),
                Value::Arr(self.autoplay.iter().map(autoplay_value).collect()),
            ),
            (
                "inputs".to_string(),
                Value::Arr(
                    self.inputs
                        .iter()
                        .map(|i| Value::text(&i.literal()))
                        .collect(),
                ),
            ),
        ];
        // (goal 17) Written only when there is something to say, so a server
        // with no stored source and no labelled input sends the bytes it sent
        // before goal 17 and the committed state vectors did not move.
        if !self.stored.is_empty() {
            state.push((
                "stored_sources".to_string(),
                Value::Arr(self.stored.iter().map(stored_source_value).collect()),
            ));
        }
        if !self.labels.is_empty() {
            state.push((
                "input_labels".to_string(),
                Value::Arr(self.labels.iter().map(input_label_value).collect()),
            ));
        }
        // (goal 14) Written only when there is something to say, so a server
        // that has adopted nothing sends the bytes it sent before goal 14 and
        // the committed state vectors did not move.
        if !self.speakers.all().is_empty() {
            state.push((
                "speakers".to_string(),
                Value::Arr(
                    self.speakers
                        .all()
                        .iter()
                        .map(|s| {
                            speaker_value(
                                s,
                                self.link(&s.id),
                                self.firmware_images.as_deref().unwrap_or(&[]),
                            )
                        })
                        .collect(),
                ),
            ));
        }
        if !self.speakers.key_changes().is_empty() {
            state.push((
                "key_changes".to_string(),
                Value::Arr(
                    self.speakers
                        .key_changes()
                        .iter()
                        .map(key_change_value)
                        .collect(),
                ),
            ));
        }
        // (goal 14) The staged images, written only by a server with a
        // firmware directory, last of all for the same reason.
        if let Some(images) = &self.firmware_images {
            state.push((
                "firmware".to_string(),
                Value::Obj(vec![(
                    "images".to_string(),
                    Value::Arr(images.iter().map(image_value).collect()),
                )]),
            ));
        }
        // (goal 17) The Soloist receivers, written only by a server that
        // runs them, after everything else for the same reason.
        if let Some(soloist) = &self.soloist {
            state.push(("soloist".to_string(), soloist_value(soloist)));
        }
        // (voice, P8) The wake-word models, written only by a server that
        // runs any, after everything else for the same reason.
        if !self.wake_words.is_empty() {
            state.push((
                "wake_words".to_string(),
                Value::Arr(self.wake_words.iter().map(WakeWord::value).collect()),
            ));
        }
        // (ADR 0175) The announcements, written only while there is one to
        // name, after everything else for the same reason.
        if !self.announcements.is_empty() {
            state.push((
                "announcements".to_string(),
                Value::Arr(self.announcements.iter().map(Announcement::value).collect()),
            ));
        }
        // (ADR 0194) The kind of each offered input the server said the kind
        // of, in the order of `inputs`, and the built-in chimes: each written
        // only when there is something to say, after everything else for the
        // same reason.
        let kinds: Vec<Value> = self
            .inputs
            .iter()
            .filter_map(|i| self.input_kind(i).map(|k| input_kind_value(i, k)))
            .collect();
        if !kinds.is_empty() {
            state.push(("input_kinds".to_string(), Value::Arr(kinds)));
        }
        if !self.chimes.is_empty() {
            state.push(("chimes".to_string(), texts(&self.chimes)));
        }
        // (ADR 0000) The measurement sweep, written once there has been
        // one, after everything else for the same reason.
        if let Some(measurement) = &self.measurement {
            state.push(("measurement".to_string(), measurement.value()));
        }
        Value::Obj(state)
    }

    fn zone_value_v2(&self, z: &Zone) -> Value {
        let mut fields = match self.zone_value_v1(z) {
            Value::Obj(fields) => fields,
            _ => unreachable!("a zone is an object"),
        };
        let quiet = z
            .quiet
            .iter()
            .enumerate()
            .map(|(i, w)| {
                let mut v = window_value(w);
                if let Value::Obj(f) = &mut v {
                    f.push((
                        "active".to_string(),
                        Value::Bool(z.quiet_active.get(i).copied().unwrap_or(false)),
                    ));
                }
                v
            })
            .collect();
        fields.extend([
            (
                "transport".to_string(),
                Value::text(self.transports.of(&z.id).name()),
            ),
            ("limit".to_string(), Value::Num(z.limit.literal())),
            (
                "effective_limit".to_string(),
                Value::Num(z.effective_limit().literal()),
            ),
            ("quiet".to_string(), Value::Arr(quiet)),
            ("quiet_enabled".to_string(), Value::Bool(z.quiet_enabled)),
            ("bond".to_string(), members_value(&z.bond)),
            (
                "ramp".to_string(),
                match z.ramp {
                    Some(target) => Value::Num(target.literal()),
                    None => Value::Null,
                },
            ),
            (
                "sound".to_string(),
                Value::Obj(vec![
                    ("bass".to_string(), Value::int(i64::from(z.sound.bass))),
                    ("treble".to_string(), Value::int(i64::from(z.sound.treble))),
                    ("loudness".to_string(), Value::Bool(z.sound.loudness)),
                    ("night".to_string(), Value::Bool(z.sound.night)),
                    ("speech".to_string(), Value::Bool(z.sound.speech)),
                    ("tv_upmix".to_string(), Value::text(z.sound.tv_upmix.name())),
                ]),
            ),
            (
                "av_trim_ms".to_string(),
                Value::int(i64::from(z.av_trim_ms)),
            ),
            (
                "bass_management".to_string(),
                Value::Obj(vec![
                    (
                        "crossover_hz".to_string(),
                        Value::int(i64::from(z.bass.crossover_hz)),
                    ),
                    (
                        "sub_level_db".to_string(),
                        centi_db_value(z.bass.sub_level_cdb),
                    ),
                    (
                        "sub_polarity".to_string(),
                        Value::text(z.bass.sub_polarity.name()),
                    ),
                    ("active".to_string(), Value::Bool(z.has_sub())),
                ]),
            ),
            (
                "room_eq".to_string(),
                Value::Obj(vec![
                    ("enabled".to_string(), Value::Bool(z.room_eq.enabled)),
                    ("filters".to_string(), filters_value(&z.room_eq.filters)),
                ]),
            ),
            ("voice_enabled".to_string(), Value::Bool(z.voice_enabled)),
            ("mic_muted".to_string(), Value::Bool(self.mic_muted(z))),
        ]);
        // (voice) The room's own choice of wake words, only once it made
        // one: a room without the member listens for every one the server
        // runs (the state's own `wake_words`).
        if let Some(chosen) = &z.wake_words {
            fields.push((
                "wake_words".to_string(),
                Value::Arr(chosen.iter().map(|id| Value::text(id)).collect()),
            ));
        }
        // (goal 16) What the room's group is playing, on the room itself and
        // only when there is a record: the room object is what a consumer of
        // one room reads (the MQTT room topic is this object, byte for byte),
        // and it carries the group's `source` with it so that consumer need
        // not read `groups[]` to know which player it is.
        if let Some(playing) = self.now_playing(&z.group) {
            fields.push((
                "source".to_string(),
                Value::text(&self.source(&z.group).literal()),
            ));
            fields.push(("now_playing".to_string(), now_playing_value(playing)));
        }
        Value::Obj(fields)
    }

    fn zone_value_v1(&self, z: &Zone) -> Value {
        Value::Obj(vec![
            ("id".to_string(), Value::text(&z.id)),
            ("name".to_string(), Value::text(&z.name)),
            ("group".to_string(), Value::text(&z.group)),
            ("volume".to_string(), Value::Num(z.volume.literal())),
            ("muted".to_string(), Value::Bool(z.muted)),
            (
                "endpoints".to_string(),
                Value::Arr(z.endpoints.iter().map(|e| Value::text(e)).collect()),
            ),
            (
                "present".to_string(),
                Value::Arr(z.present.iter().map(|e| Value::text(e)).collect()),
            ),
            ("audio".to_string(), Value::text(self.audio_for(&z.group))),
        ])
    }

    /// The v1 state message, in the declared field order.
    fn state_value_v1(&self) -> Value {
        let zones = self.zones.iter().map(|z| self.zone_value_v1(z)).collect();
        Value::Obj(vec![
            ("v".to_string(), Value::int(1)),
            ("t".to_string(), Value::text("state")),
            ("serial".to_string(), Value::int(self.serial as i64)),
            ("zones".to_string(), Value::Arr(zones)),
        ])
    }

    /// The bytes the state message is on the wire, at the build's own catalog
    /// version.
    pub fn encode_state(&self) -> String {
        json::write(&self.state_value())
    }

    /// The bytes the state message is at catalog version `version`.
    pub fn encode_state_at(&self, version: i64) -> String {
        json::write(&self.state_value_at(version))
    }
}

/// A now-playing record as the state message carries it: every member always,
/// `null` where a fact is not known, in this order.
fn now_playing_value(playing: &NowPlaying) -> Value {
    let text = |t: &Option<String>| match t {
        Some(t) => Value::text(t),
        None => Value::Null,
    };
    Value::Obj(vec![
        ("title".to_string(), text(&playing.title)),
        ("artist".to_string(), text(&playing.artist)),
        ("album".to_string(), text(&playing.album)),
        ("art_url".to_string(), text(&playing.art_url)),
        (
            "duration_ms".to_string(),
            match playing.duration_ms {
                Some(ms) => Value::int(ms as i64),
                None => Value::Null,
            },
        ),
        ("state".to_string(), Value::text(playing.state.name())),
        ("via".to_string(), Value::text(&playing.via)),
    ])
}

/// Replace the flags of a room's windows and clamp; whether anything moved.
fn set_quiet(zone: &mut Zone, active: Vec<bool>) -> bool {
    let before = (zone.quiet_active.clone(), zone.volume, zone.ramp);
    zone.quiet_active = active;
    zone.clamp();
    before != (zone.quiet_active.clone(), zone.volume, zone.ramp)
}

fn thousandths(v: i64) -> Volume {
    Volume::from_thousandths(v.clamp(0, i64::from(VOLUME_SCALE))).expect("clamped into range")
}

/// Insert or replace by key, keeping the list sorted by key.
fn upsert<T, K: Ord>(list: &mut Vec<T>, item: T, key: impl Fn(&T) -> K) {
    let k = key(&item);
    list.retain(|x| key(x) != k);
    list.push(item);
    list.sort_by_key(|x| key(x));
}

fn list_or_none(items: impl Iterator<Item = String>) -> String {
    let items: Vec<String> = items.collect();
    if items.is_empty() {
        "none".to_string()
    } else {
        items.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_zones() -> Zones {
        let mut zones = Zones::new("127.0.0.1:4010");
        zones.add(Zone::new("kitchen")).unwrap();
        zones.add(Zone::new("study")).unwrap();
        zones
    }

    #[test]
    fn a_refused_command_leaves_the_state_byte_identical() {
        let mut zones = two_zones();
        let before = zones.encode_state();
        let refusal = zones
            .apply(&Command::Name {
                zone: "no-such-zone".to_string(),
                name: "Nowhere".to_string(),
            })
            .unwrap_err();
        assert_eq!(refusal.field, "zone");
        assert_eq!(zones.encode_state(), before, "nothing may have moved");
        assert_eq!(zones.serial(), 2, "and the serial has not moved either");
    }

    #[test]
    fn ungrouping_puts_a_zone_in_a_group_of_its_own() {
        let mut zones = two_zones();
        zones
            .apply(&Command::Group {
                zone: "kitchen".to_string(),
                group: "downstairs".to_string(),
            })
            .unwrap();
        assert_eq!(zones.zone("kitchen").unwrap().group, "downstairs");
        zones
            .apply(&Command::Ungroup {
                zone: "kitchen".to_string(),
            })
            .unwrap();
        assert_eq!(zones.zone("kitchen").unwrap().group, "kitchen");
    }

    #[test]
    fn a_muted_zone_has_a_gain_of_zero_and_keeps_the_volume_it_had() {
        let mut zones = two_zones();
        let half = Volume::from_thousandths(500).unwrap();
        zones
            .apply(&Command::Volume {
                zone: "kitchen".to_string(),
                volume: half,
            })
            .unwrap();
        zones
            .apply(&Command::Mute {
                zone: "kitchen".to_string(),
                muted: true,
            })
            .unwrap();
        let zone = zones.zone("kitchen").unwrap();
        assert_eq!(zone.gain(), Volume::SILENT);
        assert_eq!(zone.volume, half, "unmuting has to give the volume back");
    }

    #[test]
    fn an_endpoint_that_leaves_stays_in_the_membership_and_leaves_the_presence() {
        let mut zones = two_zones();
        zones
            .apply(&Command::Attach {
                zone: "kitchen".to_string(),
                endpoint: "endpoint-a".to_string(),
                link: None,
            })
            .unwrap();
        assert!(zones.endpoint_left("endpoint-a"));
        let zone = zones.zone("kitchen").unwrap();
        assert_eq!(zone.endpoints, vec!["endpoint-a".to_string()]);
        assert!(zone.present.is_empty());
    }

    #[test]
    fn a_group_plays_the_stream_it_is_pointed_at() {
        let mut zones = two_zones();
        zones.set_group_audio("downstairs", "127.0.0.1:4011");
        assert_eq!(zones.audio_for("downstairs"), "127.0.0.1:4011");
        assert_eq!(zones.audio_for("kitchen"), "127.0.0.1:4010");
    }
}
