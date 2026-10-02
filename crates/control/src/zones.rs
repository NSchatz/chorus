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
//! - [`Zones::start_ramp`], [`Zones::runtime_volume`] and [`Zones::stop_ramp`]
//!   move a room's volume over time, every step clamped;
//! - [`Zones::set_alarm_ringing`] records an alarm as ringing or not;
//! - [`Zones::sleep_expired`] removes a sleep timer that ran out;
//! - [`Zones::offer_input`] and [`Zones::withdraw_input`] say which line-ins
//!   are offered.
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

use crate::catalog::{
    alarm_value, autoplay_value, centi_db_value, filters_value, is_display_name, is_identifier,
    members_value, texts, window_value, Command, Refusal, Volume, MAX_IDENTIFIER_LEN, VOLUME_SCALE,
};
use crate::json::{self, Value};
use crate::rooms::{
    Alarm, Autoplay, BondMember, CivilTime, InputId, Link, QuietWindow, Role, SavedGroup,
    SleepTimer, Source, MAX_DEFINITIONS,
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

    /// `min(limit, the cap of every active quiet-hours window)`.
    pub fn effective_limit(&self) -> Volume {
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
    /// (v2) The civil time the quiet-hours flags were last set for.
    now: Option<CivilTime>,
    /// (v2, goal 14) The adopted speakers and the key changes refused.
    speakers: Speakers,
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
            Command::AlarmSet(alarm) => {
                self.persistent_target(&alarm.target)?;
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
            if let Some(s) = self.source_of(&group) {
                self.set_source(&home, s);
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
            self.set_source(&group, source.clone());
        }
        Ok(())
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

    fn set_source(&mut self, group: &str, source: Source) {
        self.sources.retain(|(g, _)| g != group);
        if source != Source::Stream {
            self.sources.push((group.to_string(), source));
            self.sources.sort();
        }
    }

    fn move_source(&mut self, from: &str, to: &str) {
        if let Some(source) = self.source_of(from) {
            self.sources.retain(|(g, _)| g != from);
            self.set_source(to, source);
        }
    }

    /// Drop what names a group that is no longer formed: its source and its
    /// sleep timer. Runs after every change.
    fn prune(&mut self) {
        let formed: Vec<String> = self.zones.iter().map(|z| z.group.clone()).collect();
        self.sources.retain(|(g, _)| formed.contains(g));
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

    /// Say what a formed group plays.
    pub fn set_group_source(&mut self, group: &str, source: Source) -> Result<(), Refusal> {
        self.formed_members(group)?;
        self.set_source(group, source);
        self.serial += 1;
        Ok(())
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
        let mut changed = self
            .speakers
            .set_now(id, |now| now.sessions = now.sessions.saturating_sub(1));
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
                Value::Obj(vec![
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
                ])
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
                Value::Obj(vec![
                    ("target".to_string(), Value::text(&s.target)),
                    ("minutes".to_string(), Value::int(i64::from(s.minutes))),
                ])
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
                        .map(|s| speaker_value(s, self.link(&s.id)))
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
        ]);
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
