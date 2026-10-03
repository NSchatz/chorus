//! The versioned control message catalog: what a command is, what the state
//! message is, and what a decoder does with a message it cannot accept.
//!
//! `docs/control-plane.md` is the normative statement of everything in this
//! module, and `fixtures/control/` pins it byte for byte. Where this module and
//! that document disagree, the document is right and this is the defect: a
//! second implementation is graded against the vectors and the prose, never
//! against this code.
//!
//! # The version is on every message, and it is checked first
//!
//! Every message carries `v`, the catalog version. A peer that announces a
//! version this build does not implement is refused the session, told which
//! version it offered and which this build has, and nothing it sent is applied,
//! including the rest of the message the version arrived on. The audio wire
//! took the opposite decision for an unassigned type byte, and
//! `docs/control-plane.md` records why the control catalog cannot: an audio
//! frame a decoder skips costs 20 ms of one stream, and a control message a
//! decoder half-understands changes what a house is doing.
//!
//! # Volume is a decimal, not a float
//!
//! [`Volume`] holds thousandths of full scale as an integer and prints with
//! exactly three fractional digits. Nothing in the catalog, the vectors or the
//! persisted state ever holds a binary floating-point value, so there is no
//! spelling of `0.1` to disagree about between two languages.
//!
//! # Two versions, and a message is written at the lowest that has it
//!
//! Catalog version 2 (rooms, bonds, saved and live groups, limits, quiet
//! hours, alarms; `docs/decisions/0075-control-catalog-v2.md` records it, as
//! 0016 records v1) is a superset of version 1. A build implementing both accepts a v1
//! command at `"v":1` or `"v":2`, refuses a v2-only command at `"v":1` as not
//! a command of that version, and WRITES every command at the lowest version
//! that declares it ([`Command::min_version`]). That last rule is what keeps
//! every committed v1 vector byte-identical under a v2 build, and it is also
//! the useful one on a network: a v1 command a v2 build writes is one a v1
//! server still reads.

use std::fmt;

use crate::json::{self, Value};
use crate::rooms::{
    validate_layout, Alarm, Autoplay, BondMember, ClockTime, Days, InputId, InputLabel, InputRole,
    Link, PlaybackAction, QuietWindow, Role, SoloistState, Source, StoredKind, StoredSource,
    ALARM_SOURCE_SPELLINGS, MAX_DEFINITIONS, MAX_DURATION_MIN, MAX_QUIET_WINDOWS, MAX_RAMP_S,
    MAX_SLEEP_MIN, SOURCE_SPELLINGS,
};
use crate::sound::{
    EqFilter, FixedPoint, Polarity, CROSSOVER_HZ, ROOM_EQ_MAX_FILTERS, SUB_LEVEL_CDB, TONE_DB,
};
use crate::theater::{TvUpmix, AV_TRIM_MS};

/// The catalog version this build speaks: the highest it implements, and the
/// version its state message and its session refusal carry.
pub const CATALOG_VERSION: i64 = 2;

/// Every catalog version this build implements.
pub const IMPLEMENTED_VERSIONS: &[i64] = &[1, 2];

/// The oldest catalog version, which an `error` answering a message whose own
/// version could not be read is written at: every peer reads it.
pub const LOWEST_VERSION: i64 = 1;

/// Full scale, in the thousandths [`Volume`] counts.
pub const VOLUME_SCALE: u32 = 1_000;

/// Longest a zone or group identifier may be.
pub const MAX_IDENTIFIER_LEN: usize = 32;

/// Longest a human-set zone name may be, in characters.
pub const MAX_NAME_LEN: usize = 64;

/// A zone's volume: thousandths of full scale, 0 to 1000 inclusive.
///
/// The wire value is the AMPLITUDE FACTOR, not a position on a perceptual
/// curve: 0.500 means every sample is multiplied by one half.
/// `docs/decisions/0016-the-control-catalog.md` records why the curve is where
/// it is and not here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Volume(u32);

impl Volume {
    /// Silence.
    pub const SILENT: Volume = Volume(0);
    /// Full scale, which is the shipped default.
    pub const FULL: Volume = Volume(VOLUME_SCALE);

    /// A volume from thousandths of full scale, refusing anything outside the
    /// declared range.
    pub fn from_thousandths(v: i64) -> Option<Volume> {
        if (0..=i64::from(VOLUME_SCALE)).contains(&v) {
            Some(Volume(v as u32))
        } else {
            None
        }
    }

    /// Thousandths of full scale.
    pub fn thousandths(self) -> u32 {
        self.0
    }

    /// The amplitude factor, as a ratio of two integers, so that applying it
    /// needs no floating point at all.
    pub fn factor(self) -> (u32, u32) {
        (self.0, VOLUME_SCALE)
    }

    /// The one spelling the catalog uses: three fractional digits, always.
    pub fn literal(self) -> String {
        format!("{}.{:03}", self.0 / VOLUME_SCALE, self.0 % VOLUME_SCALE)
    }

    /// Read the catalog's spelling back.
    ///
    /// Accepts exactly what the catalog declares: an optional integer part, a
    /// decimal point and up to three digits, or a bare integer. It does not
    /// accept an exponent, and it does not round: `0.5001` is refused rather
    /// than silently becoming `0.500`, because a value that came back different
    /// from the value that went in is the thing a golden vector exists to catch.
    pub fn parse(digits: &str) -> Option<Volume> {
        let (whole, fraction) = match digits.split_once('.') {
            Some((w, f)) => (w, f),
            None => (digits, ""),
        };
        if whole.is_empty() || !whole.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        if fraction.len() > 3 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let whole: u32 = whole.parse().ok()?;
        let mut padded = fraction.to_string();
        while padded.len() < 3 {
            padded.push('0');
        }
        let fraction: u32 = if padded.is_empty() {
            0
        } else {
            padded.parse().ok()?
        };
        let total = whole.checked_mul(VOLUME_SCALE)?.checked_add(fraction)?;
        Volume::from_thousandths(i64::from(total))
    }
}

impl fmt::Display for Volume {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.literal())
    }
}

/// What a peer asked the server to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Announce a catalog version and open the session.
    Hello,
    /// Say that an endpoint is playing this zone.
    Attach {
        /// The zone.
        zone: String,
        /// The endpoint's identifier.
        endpoint: String,
        /// How the endpoint reaches the server, where it says (catalog v2).
        /// Absent leaves what was known before, `unknown` for an endpoint never
        /// heard from.
        link: Option<Link>,
    },
    /// Give a zone a human-set name.
    Name {
        /// The zone.
        zone: String,
        /// The name to set.
        name: String,
    },
    /// Put a zone into a group, so it plays that group's stream.
    Group {
        /// The zone.
        zone: String,
        /// The group to join.
        group: String,
    },
    /// Take a zone out of whatever group it is in.
    ///
    /// A zone is never in no group at all: ungrouping puts it into a group of
    /// its own, named for the zone. `docs/control-plane.md` says why there is
    /// no "grouped with nothing" state to represent.
    Ungroup {
        /// The zone.
        zone: String,
    },
    /// Set a zone's volume, clamped to its effective limit.
    Volume {
        /// The zone.
        zone: String,
        /// The amplitude factor to apply.
        volume: Volume,
    },
    /// Mute or unmute a zone.
    Mute {
        /// The zone.
        zone: String,
        /// Whether it is muted.
        muted: bool,
    },
    /// (v2) Put a room into the group another room or a group is in. Joining a
    /// room that is alone forms a live group with an id the server assigns.
    Join {
        /// The room that moves.
        zone: String,
        /// A room, or a group that exists now.
        target: String,
    },
    /// (v2) Make a room's endpoints a bonded set, replacing any set it had.
    Bond {
        /// The room.
        zone: String,
        /// Each endpoint and the channel it plays.
        members: Vec<BondMember>,
    },
    /// (v2) Dissolve a room's bonded set. A room with none is not an error.
    Unbond {
        /// The room.
        zone: String,
    },
    /// (v2) Save, or replace, a named group definition.
    GroupSave {
        /// The group identifier.
        group: String,
        /// The human-set name.
        name: String,
        /// Its rooms, two or more.
        zones: Vec<String>,
    },
    /// (v2) Forget a saved group definition. Its rooms stay where they are.
    GroupDelete {
        /// The saved group.
        group: String,
    },
    /// (v2) Take the room (K78): every room of the target leaves whatever group
    /// it is in and plays in the target.
    Take {
        /// A room, a saved group, or a group that exists now.
        target: String,
        /// What the target plays from now, where the command says.
        source: Option<Source>,
    },
    /// (v2) Set a group's volume, Sonos-style: every room scaled by the same
    /// ratio, each clamped to its own effective limit.
    GroupVolume {
        /// A group that exists now.
        group: String,
        /// The group volume asked for.
        volume: Volume,
    },
    /// (v2) Move a group's volume by a signed step, in thousandths.
    GroupVolumeStep {
        /// A group that exists now.
        group: String,
        /// The step, -1000 to 1000.
        step: i32,
    },
    /// (v2) Move a room's volume by a signed step, in thousandths.
    VolumeStep {
        /// The room.
        zone: String,
        /// The step, -1000 to 1000.
        step: i32,
    },
    /// (v2) Set a room's maximum volume.
    Limit {
        /// The room.
        zone: String,
        /// The ceiling every volume path is clamped to.
        limit: Volume,
    },
    /// (v2) Replace a room's quiet-hours windows. An empty list removes them.
    QuietHours {
        /// The room.
        zone: String,
        /// The windows, at most [`MAX_QUIET_WINDOWS`].
        windows: Vec<QuietWindow>,
    },
    /// (v2) Create or replace an alarm.
    AlarmSet(Alarm),
    /// (v2) Forget an alarm.
    AlarmDelete {
        /// The alarm.
        alarm: String,
    },
    /// (v2) Stop an alarm that is ringing. One that is not is not an error.
    AlarmStop {
        /// The alarm.
        alarm: String,
    },
    /// (v2) Ask for a sleep timer on a room or a group; 0 minutes cancels.
    Sleep {
        /// A room or a group that exists now.
        target: String,
        /// Minutes, 0 to [`MAX_SLEEP_MIN`].
        minutes: u32,
    },
    /// (v2) Create or replace the autoplay rule for one input.
    Autoplay(Autoplay),
    /// (v2, goal 12) Change a room's tone, loudness, night mode or speech
    /// enhancement. Every field but `zone` is optional; an absent one keeps
    /// what the room had.
    Sound {
        /// The room.
        zone: String,
        /// Bass, whole dB.
        bass: Option<i8>,
        /// Treble, whole dB.
        treble: Option<i8>,
        /// Loudness compensation.
        loudness: Option<bool>,
        /// Night mode.
        night: Option<bool>,
        /// Speech enhancement.
        speech: Option<bool>,
        /// (goal 13) What a theater set's surrounds play from a stream with
        /// no surround channel.
        tv_upmix: Option<TvUpmix>,
    },
    /// (v2, goal 12) Change a room's bass management (used when its bonded
    /// set has an `LFE` member). Partial, as `sound`.
    BassManagement {
        /// The room.
        zone: String,
        /// The crossover, Hz.
        crossover_hz: Option<u16>,
        /// The sub's level, hundredths of a dB.
        sub_level_cdb: Option<i16>,
        /// The sub's polarity.
        sub_polarity: Option<Polarity>,
    },
    /// (v2, goal 12) Replace a room's correction filters, or enable or
    /// disable them. Partial, as `sound`; an empty `filters` clears them.
    RoomEq {
        /// The room.
        zone: String,
        /// The filters, at most [`ROOM_EQ_MAX_FILTERS`], each inside the
        /// room-correction bounds.
        filters: Option<Vec<EqFilter>>,
        /// Whether they are applied.
        enabled: Option<bool>,
    },
    /// (v2, goal 13) Set a room's A/V trim: how much later (positive) or
    /// earlier its TV audio plays, ms, [`AV_TRIM_MS`].
    AvTrim {
        /// The room.
        zone: String,
        /// The trim, ms.
        av_trim_ms: i16,
    },
    /// (v2, goal 14) Name an adopted speaker.
    SpeakerName {
        /// The speaker, by the id its sessions authenticate as.
        speaker: String,
        /// What a person sees.
        name: String,
    },
    /// (v2, goal 14) Assign an adopted speaker to a room, or to none: it
    /// becomes a member of the room and leaves every other.
    SpeakerRoom {
        /// The speaker.
        speaker: String,
        /// The room, or `None` (`null` on the wire) for no room.
        room: Option<String>,
    },
    /// (v2, goal 14) Forget an adopted speaker: its record, its place in any
    /// room, and (on the server) its pinned key, so the next session under
    /// its id is adopted afresh. The owner's only way past a changed key.
    SpeakerForget {
        /// The speaker.
        speaker: String,
    },
    /// (v2, goal 14) Install a staged, verified firmware image: on one
    /// speaker (`speaker`), or on every present speaker of the image's board
    /// that does not run its version already (`"all": true`). The ONE thing
    /// that starts a transfer (K93, I13). `force` installs a version the
    /// speaker already runs.
    FirmwareInstall {
        /// The speaker, or `None` for all of them (`"all": true` on the wire).
        speaker: Option<String>,
        /// The staged image, by name.
        image: String,
        /// Install even the version the speaker runs; written only when true.
        force: bool,
    },
    /// (v2, goal 14) Abandon a speaker's install that is not yet verified.
    FirmwareCancel {
        /// The speaker.
        speaker: String,
    },
    /// (v2, goal 14) Read the firmware directory again and grade every image.
    FirmwareRescan,
    /// (v2, goal 17) Store, or replace, a named source an alarm can play
    /// (`stored:<id>`): a stream URL or a Spotify URI.
    SourceStore(StoredSource),
    /// (v2, goal 17) Forget a stored source. Refused while an alarm plays it.
    SourceForget {
        /// The stored source.
        id: String,
    },
    /// (v2, goal 17) Label an input: its name and what it is wired to. An
    /// empty name with the role `line-in` removes the label.
    InputLabel(InputLabel),
    /// (v2, goal 17) Tell every Soloist receiver's supervisor to read its
    /// binary again and start again: what the owner runs after replacing an
    /// expired build.
    SoloistRestart,
    /// (v2, goal 17) Pause, resume or skip what the Spotify receiver of a
    /// target's group is playing. Refused for a group that plays anything
    /// else.
    Playback {
        /// A room, a saved group or a formed group.
        target: String,
        /// What to do.
        action: PlaybackAction,
    },
}

impl Command {
    /// The catalog's name for this command.
    pub fn type_name(&self) -> &'static str {
        match self {
            Command::Hello => "hello",
            Command::Attach { .. } => "attach",
            Command::Name { .. } => "name",
            Command::Group { .. } => "group",
            Command::Ungroup { .. } => "ungroup",
            Command::Volume { .. } => "volume",
            Command::Mute { .. } => "mute",
            Command::Join { .. } => "join",
            Command::Bond { .. } => "bond",
            Command::Unbond { .. } => "unbond",
            Command::GroupSave { .. } => "group_save",
            Command::GroupDelete { .. } => "group_delete",
            Command::Take { .. } => "take",
            Command::GroupVolume { .. } => "group_volume",
            Command::GroupVolumeStep { .. } => "group_volume_step",
            Command::VolumeStep { .. } => "volume_step",
            Command::Limit { .. } => "limit",
            Command::QuietHours { .. } => "quiet_hours",
            Command::AlarmSet(_) => "alarm_set",
            Command::AlarmDelete { .. } => "alarm_delete",
            Command::AlarmStop { .. } => "alarm_stop",
            Command::Sleep { .. } => "sleep",
            Command::Autoplay(_) => "autoplay",
            Command::Sound { .. } => "sound",
            Command::BassManagement { .. } => "bass_management",
            Command::RoomEq { .. } => "room_eq",
            Command::AvTrim { .. } => "av_trim",
            Command::SpeakerName { .. } => "speaker_name",
            Command::SpeakerRoom { .. } => "speaker_room",
            Command::SpeakerForget { .. } => "speaker_forget",
            Command::FirmwareInstall { .. } => "firmware_install",
            Command::FirmwareCancel { .. } => "firmware_cancel",
            Command::FirmwareRescan => "firmware_rescan",
            Command::SourceStore(_) => "source_store",
            Command::SourceForget { .. } => "source_forget",
            Command::InputLabel(_) => "input_label",
            Command::SoloistRestart => "soloist_restart",
            Command::Playback { .. } => "playback",
        }
    }

    /// The lowest catalog version that declares this command with these
    /// fields, which is the version it is written at.
    pub fn min_version(&self) -> i64 {
        match self {
            Command::Hello
            | Command::Attach { link: None, .. }
            | Command::Name { .. }
            | Command::Group { .. }
            | Command::Ungroup { .. }
            | Command::Volume { .. }
            | Command::Mute { .. } => 1,
            _ => 2,
        }
    }

    /// The room this command is about, if it names one in a `zone` field.
    ///
    /// Commands that name a TARGET (a room or a group) are resolved by
    /// [`crate::zones::Zones::apply`] itself, because a target is not
    /// necessarily a room.
    pub fn zone(&self) -> Option<&str> {
        match self {
            Command::Attach { zone, .. }
            | Command::Name { zone, .. }
            | Command::Group { zone, .. }
            | Command::Ungroup { zone }
            | Command::Volume { zone, .. }
            | Command::Mute { zone, .. }
            | Command::Join { zone, .. }
            | Command::Bond { zone, .. }
            | Command::Unbond { zone }
            | Command::VolumeStep { zone, .. }
            | Command::Limit { zone, .. }
            | Command::QuietHours { zone, .. }
            | Command::Sound { zone, .. }
            | Command::BassManagement { zone, .. }
            | Command::RoomEq { zone, .. }
            | Command::AvTrim { zone, .. } => Some(zone),
            _ => None,
        }
    }

    /// This command as the object the catalog declares, in the declared field
    /// order, at [`Command::min_version`].
    pub fn value(&self) -> Value {
        let mut m = vec![
            ("v".to_string(), Value::int(self.min_version())),
            ("t".to_string(), Value::text(self.type_name())),
        ];
        let mut text = |k: &str, v: &str| m.push((k.to_string(), Value::text(v)));
        match self {
            Command::Hello => {}
            Command::Attach {
                zone,
                endpoint,
                link,
            } => {
                text("zone", zone);
                text("endpoint", endpoint);
                if let Some(link) = link {
                    text("link", link.name());
                }
            }
            Command::Name { zone, name } => {
                text("zone", zone);
                text("name", name);
            }
            Command::Group { zone, group } => {
                text("zone", zone);
                text("group", group);
            }
            Command::Ungroup { zone } | Command::Unbond { zone } => text("zone", zone),
            Command::Volume { zone, volume } => {
                text("zone", zone);
                m.push(("volume".to_string(), Value::Num(volume.literal())));
            }
            Command::Mute { zone, muted } => {
                text("zone", zone);
                m.push(("muted".to_string(), Value::Bool(*muted)));
            }
            Command::Join { zone, target } => {
                text("zone", zone);
                text("target", target);
            }
            Command::Bond { zone, members } => {
                text("zone", zone);
                m.push(("members".to_string(), members_value(members)));
            }
            Command::GroupSave { group, name, zones } => {
                text("group", group);
                text("name", name);
                m.push(("zones".to_string(), texts(zones)));
            }
            Command::GroupDelete { group } => text("group", group),
            Command::Take { target, source } => {
                text("target", target);
                if let Some(source) = source {
                    text("source", &source.literal());
                }
            }
            Command::GroupVolume { group, volume } => {
                text("group", group);
                m.push(("volume".to_string(), Value::Num(volume.literal())));
            }
            Command::GroupVolumeStep { group, step } => {
                text("group", group);
                m.push(("step".to_string(), Value::int(i64::from(*step))));
            }
            Command::VolumeStep { zone, step } => {
                text("zone", zone);
                m.push(("step".to_string(), Value::int(i64::from(*step))));
            }
            Command::Limit { zone, limit } => {
                text("zone", zone);
                m.push(("limit".to_string(), Value::Num(limit.literal())));
            }
            Command::QuietHours { zone, windows } => {
                text("zone", zone);
                m.push((
                    "windows".to_string(),
                    Value::Arr(windows.iter().map(window_value).collect()),
                ));
            }
            Command::AlarmSet(alarm) => {
                if let Value::Obj(fields) = alarm_value(alarm) {
                    m.extend(fields);
                }
            }
            Command::AlarmDelete { alarm } | Command::AlarmStop { alarm } => text("alarm", alarm),
            Command::Sleep { target, minutes } => {
                text("target", target);
                m.push(("minutes".to_string(), Value::int(i64::from(*minutes))));
            }
            Command::Autoplay(rule) => {
                if let Value::Obj(fields) = autoplay_value(rule) {
                    m.extend(fields);
                }
            }
            Command::Sound {
                zone,
                bass,
                treble,
                loudness,
                night,
                speech,
                tv_upmix,
            } => {
                text("zone", zone);
                if let Some(b) = bass {
                    m.push(("bass".to_string(), Value::int(i64::from(*b))));
                }
                if let Some(t) = treble {
                    m.push(("treble".to_string(), Value::int(i64::from(*t))));
                }
                for (key, flag) in [("loudness", loudness), ("night", night), ("speech", speech)] {
                    if let Some(flag) = flag {
                        m.push((key.to_string(), Value::Bool(*flag)));
                    }
                }
                if let Some(u) = tv_upmix {
                    m.push(("tv_upmix".to_string(), Value::text(u.name())));
                }
            }
            Command::BassManagement {
                zone,
                crossover_hz,
                sub_level_cdb,
                sub_polarity,
            } => {
                text("zone", zone);
                if let Some(hz) = crossover_hz {
                    m.push(("crossover_hz".to_string(), Value::int(i64::from(*hz))));
                }
                if let Some(level) = sub_level_cdb {
                    m.push(("sub_level_db".to_string(), centi_db_value(*level)));
                }
                if let Some(polarity) = sub_polarity {
                    m.push(("sub_polarity".to_string(), Value::text(polarity.name())));
                }
            }
            Command::RoomEq {
                zone,
                filters,
                enabled,
            } => {
                text("zone", zone);
                if let Some(filters) = filters {
                    m.push(("filters".to_string(), filters_value(filters)));
                }
                if let Some(enabled) = enabled {
                    m.push(("enabled".to_string(), Value::Bool(*enabled)));
                }
            }
            Command::AvTrim { zone, av_trim_ms } => {
                text("zone", zone);
                m.push(("av_trim_ms".to_string(), Value::int(i64::from(*av_trim_ms))));
            }
            Command::SpeakerName { speaker, name } => {
                text("speaker", speaker);
                text("name", name);
            }
            Command::SpeakerRoom { speaker, room } => {
                text("speaker", speaker);
                m.push((
                    "room".to_string(),
                    match room {
                        Some(room) => Value::text(room),
                        None => Value::Null,
                    },
                ));
            }
            Command::SpeakerForget { speaker } => text("speaker", speaker),
            Command::FirmwareInstall {
                speaker,
                image,
                force,
            } => {
                match speaker {
                    Some(speaker) => text("speaker", speaker),
                    None => m.push(("all".to_string(), Value::Bool(true))),
                }
                m.push(("image".to_string(), Value::text(image)));
                if *force {
                    m.push(("force".to_string(), Value::Bool(true)));
                }
            }
            Command::FirmwareCancel { speaker } => text("speaker", speaker),
            Command::FirmwareRescan => {}
            Command::SourceStore(stored) => {
                text("id", &stored.id);
                text("kind", stored.kind.name());
                text("value", &stored.value);
                text("name", &stored.name);
            }
            Command::SourceForget { id } => text("id", id),
            Command::InputLabel(label) => {
                text("input", &label.input.literal());
                text("name", &label.name);
                text("role", label.role.name());
            }
            Command::SoloistRestart => {}
            Command::Playback { target, action } => {
                text("target", target);
                text("action", action.name());
            }
        }
        Value::Obj(m)
    }

    /// The bytes this command is on the wire.
    pub fn encode(&self) -> String {
        json::write(&self.value())
    }
}

/// A list of strings as a JSON array.
pub fn texts(items: &[String]) -> Value {
    Value::Arr(items.iter().map(|s| Value::text(s)).collect())
}

/// A bonded set's members, in the declared order: `endpoint`, then `role`.
pub fn members_value(members: &[BondMember]) -> Value {
    Value::Arr(
        members
            .iter()
            .map(|b| {
                Value::Obj(vec![
                    ("endpoint".to_string(), Value::text(&b.endpoint)),
                    ("role".to_string(), Value::text(b.role.name())),
                ])
            })
            .collect(),
    )
}

/// One quiet-hours window, in the declared order: `days`, `start`, `end`,
/// `limit`.
pub fn window_value(w: &QuietWindow) -> Value {
    Value::Obj(vec![
        (
            "days".to_string(),
            Value::Arr(w.days.names().into_iter().map(Value::text).collect()),
        ),
        ("start".to_string(), Value::text(&w.start.literal())),
        ("end".to_string(), Value::text(&w.end.literal())),
        ("limit".to_string(), Value::Num(w.limit.literal())),
    ])
}

/// An alarm's fields, in the declared order, as `alarm_set` and the state
/// message both carry them.
pub fn alarm_value(a: &Alarm) -> Value {
    Value::Obj(vec![
        ("alarm".to_string(), Value::text(&a.id)),
        ("target".to_string(), Value::text(&a.target)),
        ("time".to_string(), Value::text(&a.time.literal())),
        (
            "days".to_string(),
            Value::Arr(a.days.names().into_iter().map(Value::text).collect()),
        ),
        ("source".to_string(), Value::text(&a.source.literal())),
        ("volume".to_string(), Value::Num(a.volume.literal())),
        ("ramp_s".to_string(), Value::int(i64::from(a.ramp_s))),
        (
            "duration_min".to_string(),
            Value::int(i64::from(a.duration_min)),
        ),
        ("enabled".to_string(), Value::Bool(a.enabled)),
    ])
}

/// A stored source's fields, in the declared order, as the state message
/// carries them: `id`, `kind`, `value`, `name`.
pub fn stored_source_value(s: &StoredSource) -> Value {
    Value::Obj(vec![
        ("id".to_string(), Value::text(&s.id)),
        ("kind".to_string(), Value::text(s.kind.name())),
        ("value".to_string(), Value::text(&s.value)),
        ("name".to_string(), Value::text(&s.name)),
    ])
}

/// An input label's fields, in the declared order, as the state message
/// carries them: `input`, `name`, `role`.
pub fn input_label_value(l: &InputLabel) -> Value {
    Value::Obj(vec![
        ("input".to_string(), Value::text(&l.input.literal())),
        ("name".to_string(), Value::text(&l.name)),
        ("role".to_string(), Value::text(l.role.name())),
    ])
}

/// The state's `soloist` member (goal 17), in the declared order: the
/// receivers always, the build once a supervisor reported one, the warning
/// and the exhausted targets only when there is something to say.
pub fn soloist_value(s: &SoloistState) -> Value {
    let mut m = vec![(
        "receivers".to_string(),
        Value::Arr(
            s.receivers
                .iter()
                .map(|r| {
                    Value::Obj(vec![
                        ("id".to_string(), Value::text(&r.id)),
                        ("state".to_string(), Value::text(&r.state)),
                        ("target".to_string(), Value::text(&r.target)),
                        ("name".to_string(), Value::text(&r.name)),
                    ])
                })
                .collect(),
        ),
    )];
    if let Some(build) = &s.build {
        let mut b = vec![("version".to_string(), Value::text(&build.version))];
        if let Some(days) = build.expires_in_days {
            b.push(("expires_in_days".to_string(), Value::int(days)));
        }
        m.push(("build".to_string(), Value::Obj(b)));
    }
    if let Some(warning) = &s.warning {
        m.push(("warning".to_string(), Value::text(warning)));
    }
    if !s.exhausted.is_empty() {
        m.push(("exhausted".to_string(), texts(&s.exhausted)));
    }
    Value::Obj(m)
}

/// A value in hundredths of a dB, as the catalog writes it: two places.
pub fn centi_db_value(cdb: i16) -> Value {
    Value::Num(FixedPoint::literal(i64::from(cdb), 2))
}

/// Room-correction filters, each in the declared order: `freq_hz`,
/// `gain_db`, `q`.
pub fn filters_value(filters: &[EqFilter]) -> Value {
    Value::Arr(
        filters
            .iter()
            .map(|f| {
                Value::Obj(vec![
                    ("freq_hz".to_string(), Value::int(i64::from(f.freq_hz))),
                    ("gain_db".to_string(), centi_db_value(f.gain_cdb)),
                    (
                        "q".to_string(),
                        Value::Num(FixedPoint::literal(i64::from(f.q_milli), 3)),
                    ),
                ])
            })
            .collect(),
    )
}

/// An autoplay rule's fields, in the declared order. Goal 13's
/// `stop_on_standby` and `low_latency` are written only when false (their
/// default is true), so a goal-11 rule keeps its bytes in a command and in
/// the state message.
pub fn autoplay_value(rule: &Autoplay) -> Value {
    let mut fields = vec![
        ("input".to_string(), Value::text(&rule.input.literal())),
        ("target".to_string(), Value::text(&rule.target)),
        ("enabled".to_string(), Value::Bool(rule.enabled)),
    ];
    if !rule.stop_on_standby {
        fields.push(("stop_on_standby".to_string(), Value::Bool(false)));
    }
    if !rule.low_latency {
        fields.push(("low_latency".to_string(), Value::Bool(false)));
    }
    Value::Obj(fields)
}

/// Why a message was not applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// The field the refusal is about. `""` where the whole message is.
    pub field: String,
    /// What was wrong, in words a person can act on.
    pub detail: String,
    /// Whether this refusal ends the session.
    pub kind: RefusalKind,
    /// The catalog version the refusal is written at: the version of the
    /// message it answers where that was read, [`LOWEST_VERSION`] where it was
    /// not, and the build's own version for a session refusal.
    pub version: i64,
}

/// The kind of refusal, which is what decides whether the session survives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefusalKind {
    /// The message was not well-formed JSON, or not the shape the catalog
    /// declares. One message is refused; the session stays open.
    Malformed,
    /// A value was outside a declared range, or named something that does not
    /// exist. One message is refused; the session stays open.
    Rejected,
    /// The peer announced a catalog version this build does not implement.
    /// The session is refused, and nothing from the peer is applied.
    UnknownVersion {
        /// What the peer offered, where it offered a number at all.
        offered: Option<i64>,
        /// Every version the refusing build implements.
        implemented: Vec<i64>,
    },
}

impl Refusal {
    /// A refusal of one message that leaves the session open.
    pub fn rejected(field: &str, detail: String) -> Refusal {
        Refusal {
            field: field.to_string(),
            detail,
            kind: RefusalKind::Rejected,
            version: LOWEST_VERSION,
        }
    }

    /// A refusal because the text was not a message at all.
    pub fn malformed(field: &str, detail: String) -> Refusal {
        Refusal {
            field: field.to_string(),
            detail,
            kind: RefusalKind::Malformed,
            version: LOWEST_VERSION,
        }
    }

    /// The same refusal, written at catalog version `version`: what a server
    /// does with a refusal its room model made of a message whose version it
    /// read. A session refusal keeps its own version.
    pub fn at(mut self, version: i64) -> Refusal {
        if !self.ends_the_session() {
            self.version = version;
        }
        self
    }

    /// Whether this refusal ends the session.
    pub fn ends_the_session(&self) -> bool {
        matches!(self.kind, RefusalKind::UnknownVersion { .. })
    }

    /// The message the server sends back, in the declared field order.
    pub fn value(&self) -> Value {
        match &self.kind {
            RefusalKind::UnknownVersion {
                offered,
                implemented,
            } => Value::Obj(vec![
                ("v".to_string(), Value::int(self.version)),
                ("t".to_string(), Value::text("refused")),
                ("field".to_string(), Value::text(&self.field)),
                ("detail".to_string(), Value::text(&self.detail)),
                (
                    "offered".to_string(),
                    match offered {
                        Some(v) => Value::int(*v),
                        None => Value::Null,
                    },
                ),
                (
                    "implemented".to_string(),
                    Value::Arr(implemented.iter().map(|v| Value::int(*v)).collect()),
                ),
            ]),
            _ => Value::Obj(vec![
                ("v".to_string(), Value::int(self.version)),
                ("t".to_string(), Value::text("error")),
                ("field".to_string(), Value::text(&self.field)),
                ("detail".to_string(), Value::text(&self.detail)),
            ]),
        }
    }

    /// The bytes this refusal is on the wire.
    pub fn encode(&self) -> String {
        json::write(&self.value())
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.field.is_empty() {
            write!(f, "{}", self.detail)
        } else {
            write!(f, "field '{}': {}", self.field, self.detail)
        }
    }
}

impl std::error::Error for Refusal {}

/// Read one command off the wire, by this build.
///
/// In this order, and the order is the contract:
///
/// 1. the text is JSON this reader accepts, or the message is malformed;
/// 2. `v` is present, is a whole number, and is a version this build
///    implements, or the SESSION is refused;
/// 3. `t` names a command in that version of the catalog;
/// 4. the fields that command declares are present, of the declared type and
///    inside the declared range;
/// 5. no field is present that the command does not declare at that version.
///
/// Step 5 is deliberate and it is the opposite of what the audio wire does with
/// a longer-than-expected payload. See `docs/control-plane.md`.
pub fn decode_command(text: &str) -> Result<Command, Refusal> {
    decode_message(text).map(|(_, command)| command)
}

/// [`decode_command`], also saying which catalog version the message was
/// written at, so that what answers it can answer at the same version.
pub fn decode_message(text: &str) -> Result<(i64, Command), Refusal> {
    decode_message_with(text, IMPLEMENTED_VERSIONS)
}

/// [`decode_command`] as a build implementing exactly `implemented` would
/// read it.
///
/// The shipped build is [`IMPLEMENTED_VERSIONS`]. A smaller set is what lets a
/// test reproduce a refusal an older build wrote, byte for byte: the committed
/// v1 vector `refused-unknown-version` was written by a build implementing
/// `[1]`, and its own `.fields` says so (`implemented = 1`).
pub fn decode_command_with(text: &str, implemented: &[i64]) -> Result<Command, Refusal> {
    decode_message_with(text, implemented).map(|(_, command)| command)
}

fn decode_message_with(text: &str, implemented: &[i64]) -> Result<(i64, Command), Refusal> {
    let value = json::parse(text).map_err(|e| {
        Refusal::malformed("", format!("the message is not well-formed JSON: {}", e))
    })?;
    let members = match &value {
        Value::Obj(members) => members,
        other => {
            return Err(Refusal::malformed(
                "",
                format!(
                    "a control message is an object and this is {}",
                    other.kind()
                ),
            ))
        }
    };
    let highest = implemented.iter().copied().max().unwrap_or(LOWEST_VERSION);
    let unknown = |offered: Option<i64>, detail: String| Refusal {
        field: "v".to_string(),
        detail,
        kind: RefusalKind::UnknownVersion {
            offered,
            implemented: implemented.to_vec(),
        },
        version: highest,
    };

    let version = match value.get("v") {
        None => {
            return Err(unknown(
                None,
                format!(
                    "the message carries no catalog version; this build implements {}",
                    version_list(implemented)
                ),
            ))
        }
        Some(v) => v,
    };
    let offered = version
        .as_num()
        .and_then(|digits| digits.parse::<i64>().ok());
    let v = match offered {
        Some(v) if implemented.contains(&v) => v,
        offered => {
            return Err(unknown(
                offered,
                format!(
                    "catalog version {} was offered and this build implements {}; nothing from \
                     this peer has been applied",
                    match offered {
                        Some(v) => v.to_string(),
                        None => format!("'{}'", json::write(version)),
                    },
                    version_list(implemented)
                ),
            ))
        }
    };
    decode_body(&value, members, v).map(|command| (v, command))
}

/// Steps 3 to 5, for a message whose version `v` has been accepted.
fn decode_body(value: &Value, members: &[(String, Value)], v: i64) -> Result<Command, Refusal> {
    let at = |r: Refusal| r.at(v);
    let type_name = match value.get("t").and_then(Value::as_str) {
        Some(t) => t.to_string(),
        None => {
            return Err(at(Refusal::rejected(
                "t",
                "the message carries no type, or its type is not a string".to_string(),
            )))
        }
    };
    let v2 = v >= 2;
    let fields = |required: &[&str], optional: &[&str]| -> Result<(), Refusal> {
        expect_fields_with(members, required, optional).map_err(|r| r.at(v))
    };
    let id = |field: &str| identifier(value, field).map_err(|r| r.at(v));

    let command = match type_name.as_str() {
        "hello" => {
            fields(&["v", "t"], &[])?;
            Command::Hello
        }
        "attach" => {
            fields(
                &["v", "t", "zone", "endpoint"],
                if v2 { &["link"] } else { &[] },
            )?;
            let link = match value.get("link") {
                None => None,
                Some(Value::Str(word)) => Some(Link::parse(word).ok_or_else(|| {
                    at(Refusal::rejected(
                        "link",
                        format!(
                            "'{}' is not a link; the catalog declares wired, wireless or unknown",
                            word
                        ),
                    ))
                })?),
                Some(other) => {
                    return Err(at(Refusal::rejected(
                        "link",
                        format!(
                            "the link is {} and the catalog declares a string",
                            other.kind()
                        ),
                    )))
                }
            };
            Command::Attach {
                zone: id("zone")?,
                endpoint: id("endpoint")?,
                link,
            }
        }
        "name" => {
            fields(&["v", "t", "zone", "name"], &[])?;
            Command::Name {
                zone: id("zone")?,
                name: display_name(value, "name").map_err(at)?,
            }
        }
        "group" => {
            fields(&["v", "t", "zone", "group"], &[])?;
            Command::Group {
                zone: id("zone")?,
                group: id("group")?,
            }
        }
        "ungroup" => {
            fields(&["v", "t", "zone"], &[])?;
            Command::Ungroup { zone: id("zone")? }
        }
        "volume" => {
            fields(&["v", "t", "zone", "volume"], &[])?;
            let zone = id("zone")?;
            let volume = volume_field(value, "volume").map_err(at)?;
            Command::Volume { zone, volume }
        }
        "mute" => {
            fields(&["v", "t", "zone", "muted"], &[])?;
            let zone = id("zone")?;
            let muted = boolean(value, "muted").map_err(at)?;
            Command::Mute { zone, muted }
        }
        "transport" => {
            return Err(at(Refusal::rejected(
                "t",
                format!(
                    "'transport' is not a command in catalog version {}. {}",
                    v, TRANSPORT_IS_CONFIGURED
                ),
            )))
        }
        other if v2 => decode_v2(value, other, &fields)?,
        other => {
            return Err(at(Refusal::rejected(
                "t",
                format!("'{}' is not a command in catalog version {}", other, v),
            )))
        }
    };
    Ok(command)
}

/// Holds a message to its required and optional fields.
type FieldCheck<'a> = dyn Fn(&[&str], &[&str]) -> Result<(), Refusal> + 'a;

/// The commands only catalog version 2 declares.
fn decode_v2(value: &Value, type_name: &str, fields: &FieldCheck<'_>) -> Result<Command, Refusal> {
    let at = |r: Refusal| r.at(2);
    let id = |field: &str| identifier(value, field).map_err(at);
    let command = match type_name {
        "join" => {
            fields(&["v", "t", "zone", "target"], &[])?;
            Command::Join {
                zone: id("zone")?,
                target: id("target")?,
            }
        }
        "bond" => {
            fields(&["v", "t", "zone", "members"], &[])?;
            let zone = id("zone")?;
            Command::Bond {
                zone,
                members: bond_members(value).map_err(at)?,
            }
        }
        "unbond" => {
            fields(&["v", "t", "zone"], &[])?;
            Command::Unbond { zone: id("zone")? }
        }
        "group_save" => {
            fields(&["v", "t", "group", "name", "zones"], &[])?;
            let group = id("group")?;
            let name = display_name(value, "name").map_err(at)?;
            let zones = identifier_list(value, "zones").map_err(at)?;
            if zones.len() < 2 {
                return Err(at(Refusal::rejected(
                    "zones",
                    "a saved group holds two or more rooms; a single room is already its own group"
                        .to_string(),
                )));
            }
            Command::GroupSave { group, name, zones }
        }
        "group_delete" => {
            fields(&["v", "t", "group"], &[])?;
            Command::GroupDelete {
                group: id("group")?,
            }
        }
        "take" => {
            fields(&["v", "t", "target"], &["source"])?;
            let target = id("target")?;
            let source = match value.get("source") {
                None => None,
                Some(_) => Some(source_field(value, "source").map_err(at)?),
            };
            if let Some(Source::Stored(stored)) = &source {
                // (goal 17) A stored source is an alarm's: brief section 4.8
                // names "stored alarm stream URLs" as the input path, and no
                // command plays a URL in a room (ADR 0124).
                return Err(at(Refusal::rejected(
                    "source",
                    format!(
                        "'stored:{}' is a stored source, which only an alarm plays (alarm_set \
                         with this source); a take cannot name one",
                        stored
                    ),
                )));
            }
            Command::Take { target, source }
        }
        "group_volume" => {
            fields(&["v", "t", "group", "volume"], &[])?;
            Command::GroupVolume {
                group: id("group")?,
                volume: volume_field(value, "volume").map_err(at)?,
            }
        }
        "group_volume_step" => {
            fields(&["v", "t", "group", "step"], &[])?;
            Command::GroupVolumeStep {
                group: id("group")?,
                step: step_field(value).map_err(at)?,
            }
        }
        "volume_step" => {
            fields(&["v", "t", "zone", "step"], &[])?;
            Command::VolumeStep {
                zone: id("zone")?,
                step: step_field(value).map_err(at)?,
            }
        }
        "limit" => {
            fields(&["v", "t", "zone", "limit"], &[])?;
            Command::Limit {
                zone: id("zone")?,
                limit: volume_field(value, "limit").map_err(at)?,
            }
        }
        "quiet_hours" => {
            fields(&["v", "t", "zone", "windows"], &[])?;
            let zone = id("zone")?;
            Command::QuietHours {
                zone,
                windows: quiet_windows(value).map_err(at)?,
            }
        }
        "alarm_set" => {
            fields(
                &[
                    "v",
                    "t",
                    "alarm",
                    "target",
                    "time",
                    "days",
                    "source",
                    "volume",
                    "ramp_s",
                    "duration_min",
                    "enabled",
                ],
                &[],
            )?;
            Command::AlarmSet(Alarm {
                id: id("alarm")?,
                target: id("target")?,
                time: clock_field(value, "time").map_err(at)?,
                days: days_field(value, "days").map_err(at)?,
                source: alarm_source_field(value, "source").map_err(at)?,
                volume: volume_field(value, "volume").map_err(at)?,
                ramp_s: whole(value, "ramp_s", 0, i64::from(MAX_RAMP_S)).map_err(at)? as u32,
                duration_min: whole(value, "duration_min", 0, i64::from(MAX_DURATION_MIN))
                    .map_err(at)? as u32,
                enabled: boolean(value, "enabled").map_err(at)?,
            })
        }
        "alarm_delete" => {
            fields(&["v", "t", "alarm"], &[])?;
            Command::AlarmDelete {
                alarm: id("alarm")?,
            }
        }
        "alarm_stop" => {
            fields(&["v", "t", "alarm"], &[])?;
            Command::AlarmStop {
                alarm: id("alarm")?,
            }
        }
        "sleep" => {
            fields(&["v", "t", "target", "minutes"], &[])?;
            Command::Sleep {
                target: id("target")?,
                minutes: whole(value, "minutes", 0, i64::from(MAX_SLEEP_MIN)).map_err(at)? as u32,
            }
        }
        "autoplay" => {
            fields(
                &["v", "t", "input", "target", "enabled"],
                &["stop_on_standby", "low_latency"],
            )?;
            let input = match value.get("input").and_then(Value::as_str) {
                Some(text) => InputId::parse(text).ok_or_else(|| {
                    at(Refusal::rejected(
                        "input",
                        format!(
                            "'{}' is not an input: the catalog declares '<endpoint>/<input>', \
                             both identifiers",
                            text
                        ),
                    ))
                })?,
                None => {
                    return Err(at(Refusal::rejected(
                        "input",
                        "the field 'input' is not a string".to_string(),
                    )))
                }
            };
            let defaulted = |field: &str| -> Result<bool, Refusal> {
                match value.get(field) {
                    None => Ok(true),
                    Some(_) => boolean(value, field).map_err(at),
                }
            };
            Command::Autoplay(Autoplay {
                input,
                target: id("target")?,
                enabled: boolean(value, "enabled").map_err(at)?,
                stop_on_standby: defaulted("stop_on_standby")?,
                low_latency: defaulted("low_latency")?,
            })
        }
        "sound" => {
            fields(
                &["v", "t", "zone"],
                &["bass", "treble", "loudness", "night", "speech", "tv_upmix"],
            )?;
            let zone = id("zone")?;
            let tone = |field: &str| -> Result<Option<i8>, Refusal> {
                match value.get(field) {
                    None => Ok(None),
                    Some(_) => whole(value, field, i64::from(TONE_DB.0), i64::from(TONE_DB.1))
                        .map(|n| Some(n as i8))
                        .map_err(at),
                }
            };
            let flag = |field: &str| -> Result<Option<bool>, Refusal> {
                match value.get(field) {
                    None => Ok(None),
                    Some(_) => boolean(value, field).map(Some).map_err(at),
                }
            };
            let tv_upmix = match value.get("tv_upmix") {
                None => None,
                Some(v) => {
                    let word = v.as_str().unwrap_or("");
                    Some(TvUpmix::parse(word).ok_or_else(|| {
                        at(Refusal::rejected(
                            "tv_upmix",
                            format!(
                                "'{}' is not a TV upmix; the catalog declares \"off\" or \
                                 \"ambient\"",
                                word
                            ),
                        ))
                    })?)
                }
            };
            Command::Sound {
                zone,
                bass: tone("bass")?,
                treble: tone("treble")?,
                loudness: flag("loudness")?,
                night: flag("night")?,
                speech: flag("speech")?,
                tv_upmix,
            }
        }
        "av_trim" => {
            fields(&["v", "t", "zone", "av_trim_ms"], &[])?;
            Command::AvTrim {
                zone: id("zone")?,
                av_trim_ms: whole(
                    value,
                    "av_trim_ms",
                    i64::from(AV_TRIM_MS.0),
                    i64::from(AV_TRIM_MS.1),
                )
                .map_err(at)? as i16,
            }
        }
        "speaker_name" => {
            fields(&["v", "t", "speaker", "name"], &[])?;
            Command::SpeakerName {
                speaker: id("speaker")?,
                name: display_name(value, "name").map_err(at)?,
            }
        }
        "speaker_room" => {
            fields(&["v", "t", "speaker", "room"], &[])?;
            Command::SpeakerRoom {
                speaker: id("speaker")?,
                // `null` is "no room", said out loud: a `room` left out is a
                // message somebody forgot a field of, and is refused.
                room: match value.get("room") {
                    Some(Value::Null) => None,
                    _ => Some(id("room")?),
                },
            }
        }
        "speaker_forget" => {
            fields(&["v", "t", "speaker"], &[])?;
            Command::SpeakerForget {
                speaker: id("speaker")?,
            }
        }
        "firmware_install" => {
            fields(&["v", "t", "image"], &["speaker", "all", "force"])?;
            // One speaker, or all of them, said out loud: never both, never
            // neither, and `all` is only ever `true`.
            let speaker = match (value.get("speaker"), value.get("all")) {
                (Some(_), Some(_)) => {
                    return Err(at(Refusal::rejected(
                        "all",
                        "firmware_install names one speaker or says \"all\": true, not both"
                            .to_string(),
                    )))
                }
                (Some(_), None) => Some(id("speaker")?),
                (None, Some(_)) => {
                    if !boolean(value, "all").map_err(at)? {
                        return Err(at(Refusal::rejected(
                            "all",
                            "\"all\" is true or is left out; to install on one speaker, name it"
                                .to_string(),
                        )));
                    }
                    None
                }
                (None, None) => {
                    return Err(at(Refusal::rejected(
                        "speaker",
                        "firmware_install names a speaker, or says \"all\": true".to_string(),
                    )))
                }
            };
            Command::FirmwareInstall {
                speaker,
                image: id("image")?,
                force: match value.get("force") {
                    None => false,
                    Some(_) => boolean(value, "force").map_err(at)?,
                },
            }
        }
        "firmware_cancel" => {
            fields(&["v", "t", "speaker"], &[])?;
            Command::FirmwareCancel {
                speaker: id("speaker")?,
            }
        }
        "firmware_rescan" => {
            fields(&["v", "t"], &[])?;
            Command::FirmwareRescan
        }
        "soloist_restart" => {
            fields(&["v", "t"], &[])?;
            Command::SoloistRestart
        }
        "playback" => {
            fields(&["v", "t", "target", "action"], &[])?;
            let target = id("target")?;
            let word = value.get("action").and_then(Value::as_str).unwrap_or("");
            let action = PlaybackAction::parse(word).ok_or_else(|| {
                at(Refusal::rejected(
                    "action",
                    format!(
                        "'{}' is not a playback action: the catalog declares 'pause', 'resume', \
                         'next' and 'previous'",
                        word.escape_debug()
                    ),
                ))
            })?;
            Command::Playback { target, action }
        }
        "source_store" => {
            fields(&["v", "t", "id", "kind", "value", "name"], &[])?;
            let id = id("id")?;
            let word = value.get("kind").and_then(Value::as_str).unwrap_or("");
            let kind = StoredKind::parse(word).ok_or_else(|| {
                at(Refusal::rejected(
                    "kind",
                    format!(
                        "'{}' is not a kind of stored source: the catalog declares 'url' and \
                         'spotify'",
                        word
                    ),
                ))
            })?;
            let stored = match value.get("value").and_then(Value::as_str) {
                Some(text) => text.to_string(),
                None => {
                    return Err(at(Refusal::rejected(
                        "value",
                        "the field 'value' is not a string".to_string(),
                    )))
                }
            };
            if let Some(problem) = StoredSource::value_problem(kind, &stored) {
                return Err(at(Refusal::rejected("value", problem)));
            }
            Command::SourceStore(StoredSource {
                id,
                kind,
                value: stored,
                name: display_name(value, "name").map_err(at)?,
            })
        }
        "source_forget" => {
            fields(&["v", "t", "id"], &[])?;
            Command::SourceForget { id: id("id")? }
        }
        "input_label" => {
            fields(&["v", "t", "input", "name", "role"], &[])?;
            let input = match value.get("input").and_then(Value::as_str) {
                Some(text) => InputId::parse(text).ok_or_else(|| {
                    at(Refusal::rejected(
                        "input",
                        format!(
                            "'{}' is not an input: the catalog declares '<endpoint>/<input>', \
                             both identifiers",
                            text
                        ),
                    ))
                })?,
                None => {
                    return Err(at(Refusal::rejected(
                        "input",
                        "the field 'input' is not a string".to_string(),
                    )))
                }
            };
            let word = value.get("role").and_then(Value::as_str).unwrap_or("");
            let role = InputRole::parse(word).ok_or_else(|| {
                at(Refusal::rejected(
                    "role",
                    format!(
                        "'{}' is not an input's role: the catalog declares 'line-in' and \
                         'streamer'",
                        word
                    ),
                ))
            })?;
            // An empty name with the role 'line-in' removes the label; any
            // other name is a display name.
            let name = match value.get("name").and_then(Value::as_str) {
                Some("") if role == InputRole::LineIn => String::new(),
                Some("") => {
                    return Err(at(Refusal::rejected(
                        "name",
                        "a streamer has a name; an empty name goes with the role 'line-in' \
                         only, and removes the label"
                            .to_string(),
                    )))
                }
                _ => display_name(value, "name").map_err(at)?,
            };
            Command::InputLabel(InputLabel { input, name, role })
        }
        "bass_management" => {
            fields(
                &["v", "t", "zone"],
                &["crossover_hz", "sub_level_db", "sub_polarity"],
            )?;
            let zone = id("zone")?;
            let crossover_hz = match value.get("crossover_hz") {
                None => None,
                Some(_) => Some(
                    whole(
                        value,
                        "crossover_hz",
                        i64::from(CROSSOVER_HZ.0),
                        i64::from(CROSSOVER_HZ.1),
                    )
                    .map_err(at)? as u16,
                ),
            };
            let sub_level_cdb = match value.get("sub_level_db") {
                None => None,
                Some(_) => Some(
                    centi_db(value, "sub_level_db", SUB_LEVEL_CDB.0, SUB_LEVEL_CDB.1)
                        .map_err(at)?,
                ),
            };
            let sub_polarity = match value.get("sub_polarity") {
                None => None,
                Some(v) => {
                    let word = v.as_str().unwrap_or("");
                    Some(Polarity::parse(word).ok_or_else(|| {
                        at(Refusal::rejected(
                            "sub_polarity",
                            format!(
                                "'{}' is not a polarity; the catalog declares \"normal\" or \
                                 \"inverted\"",
                                word
                            ),
                        ))
                    })?)
                }
            };
            Command::BassManagement {
                zone,
                crossover_hz,
                sub_level_cdb,
                sub_polarity,
            }
        }
        "room_eq" => {
            fields(&["v", "t", "zone"], &["filters", "enabled"])?;
            let zone = id("zone")?;
            let filters = match value.get("filters") {
                None => None,
                Some(_) => Some(eq_filters(value).map_err(at)?),
            };
            let enabled = match value.get("enabled") {
                None => None,
                Some(_) => Some(boolean(value, "enabled").map_err(at)?),
            };
            Command::RoomEq {
                zone,
                filters,
                enabled,
            }
        }
        other => {
            return Err(at(Refusal::rejected(
                "t",
                format!("'{}' is not a command in catalog version 2", other),
            )))
        }
    };
    Ok(command)
}

/// A decibel value written with at most two places, from `min` to `max`
/// hundredths, as hundredths.
fn centi_db(value: &Value, field: &str, min: i16, max: i16) -> Result<i16, Refusal> {
    let refuse = |what: String| {
        Refusal::rejected(
            field,
            format!(
                "the field '{}' is {} and the catalog declares a number of dB from {} to {} in \
                 steps of 0.01",
                field,
                what,
                FixedPoint::literal(i64::from(min), 2),
                FixedPoint::literal(i64::from(max), 2)
            ),
        )
    };
    let digits = match value.get(field) {
        Some(Value::Num(digits)) => digits,
        Some(other) => return Err(refuse(other.kind().to_string())),
        None => return Err(refuse("absent".to_string())),
    };
    match FixedPoint::parse(digits, 2) {
        Some(n) if (i64::from(min)..=i64::from(max)).contains(&n) => Ok(n as i16),
        _ => Err(refuse(digits.clone())),
    }
}

/// `room_eq`'s `filters`: at most [`ROOM_EQ_MAX_FILTERS`], each inside the
/// room-correction bounds. Every refusal names `filters`, and its detail the
/// filter and the field inside it.
fn eq_filters(value: &Value) -> Result<Vec<EqFilter>, Refusal> {
    let items = objects(value, "filters", &["freq_hz", "gain_db", "q"])?;
    if items.len() > ROOM_EQ_MAX_FILTERS {
        return Err(Refusal::rejected(
            "filters",
            format!(
                "a room holds at most {} correction filters and this lists {}",
                ROOM_EQ_MAX_FILTERS,
                items.len()
            ),
        ));
    }
    let mut out = Vec::new();
    for (n, item) in items.into_iter().enumerate() {
        let inner = |r: Refusal| {
            Refusal::rejected(
                "filters",
                format!("in 'filters', filter {}: {}", n, r.detail),
            )
        };
        // Read wide, then held to the bounds below, so a value out of range
        // is refused by the bounds' own words.
        let freq_hz = whole(item, "freq_hz", 0, i64::from(u16::MAX)).map_err(inner)? as u16;
        let gain_cdb = centi_db(item, "gain_db", i16::MIN, i16::MAX).map_err(inner)?;
        let q_text = match item.get("q") {
            Some(Value::Num(digits)) => digits.clone(),
            _ => String::new(),
        };
        let q_milli = FixedPoint::parse(&q_text, 3)
            .and_then(|q| u16::try_from(q).ok())
            .ok_or_else(|| {
                Refusal::rejected(
                    "filters",
                    format!(
                        "in 'filters', filter {}: the field 'q' is '{}' and the catalog declares \
                         a number with at most three places",
                        n, q_text
                    ),
                )
            })?;
        let filter = EqFilter {
            freq_hz,
            gain_cdb,
            q_milli,
        };
        if let Some(problem) = filter.problem() {
            return Err(Refusal::rejected(
                "filters",
                format!("in 'filters', filter {}: {}", n, problem),
            ));
        }
        out.push(filter);
    }
    Ok(out)
}

fn version_list(implemented: &[i64]) -> String {
    implemented
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn volume_field(value: &Value, field: &str) -> Result<Volume, Refusal> {
    let digits = match value.get(field) {
        Some(Value::Num(digits)) => digits.clone(),
        Some(other) => {
            return Err(Refusal::rejected(
                field,
                format!(
                    "the {} is {} and the catalog declares a number from 0.000 to 1.000",
                    field,
                    other.kind()
                ),
            ))
        }
        None => {
            return Err(Refusal::rejected(
                field,
                format!(
                    "this command requires the field '{}' and it is absent",
                    field
                ),
            ))
        }
    };
    Volume::parse(&digits).ok_or_else(|| {
        Refusal::rejected(
            field,
            format!(
                "the {} {} is outside the range the catalog declares, which is 0.000 to 1.000 in \
                 steps of 0.001",
                field, digits
            ),
        )
    })
}

fn boolean(value: &Value, field: &str) -> Result<bool, Refusal> {
    value.get(field).and_then(Value::as_bool).ok_or_else(|| {
        Refusal::rejected(field, format!("the {} field is not true or false", field))
    })
}

/// A whole number from `min` to `max`, written with no fraction or exponent.
fn whole(value: &Value, field: &str, min: i64, max: i64) -> Result<i64, Refusal> {
    let refuse = |what: String| {
        Refusal::rejected(
            field,
            format!(
                "the field '{}' is {} and the catalog declares a whole number from {} to {}",
                field, what, min, max
            ),
        )
    };
    let digits = match value.get(field) {
        Some(Value::Num(digits)) => digits,
        Some(other) => return Err(refuse(other.kind().to_string())),
        None => return Err(refuse("absent".to_string())),
    };
    let body = digits.strip_prefix('-').unwrap_or(digits);
    if body.is_empty() || !body.bytes().all(|b| b.is_ascii_digit()) || digits == "-0" {
        return Err(refuse(digits.clone()));
    }
    match digits.parse::<i64>() {
        Ok(n) if (min..=max).contains(&n) => Ok(n),
        _ => Err(refuse(digits.clone())),
    }
}

fn step_field(value: &Value) -> Result<i32, Refusal> {
    let scale = i64::from(VOLUME_SCALE);
    whole(value, "step", -scale, scale).map(|n| n as i32)
}

fn clock_field(value: &Value, field: &str) -> Result<ClockTime, Refusal> {
    let text = value.get(field).and_then(Value::as_str).unwrap_or("");
    ClockTime::parse(text).ok_or_else(|| {
        Refusal::rejected(
            field,
            format!(
                "'{}' is not a time: the catalog declares \"HH:MM\", 00:00 to 23:59",
                text
            ),
        )
    })
}

fn days_field(value: &Value, field: &str) -> Result<Days, Refusal> {
    let items = match value.get(field) {
        Some(Value::Arr(items)) => items,
        _ => {
            return Err(Refusal::rejected(
                field,
                format!("the field '{}' is not an array of day names", field),
            ))
        }
    };
    let mut names = Vec::new();
    for item in items {
        match item.as_str() {
            Some(name) => names.push(name),
            None => {
                return Err(Refusal::rejected(
                    field,
                    format!(
                        "the field '{}' holds something that is not a day name",
                        field
                    ),
                ))
            }
        }
    }
    Days::from_names(names).map_err(|detail| Refusal::rejected(field, detail))
}

fn source_field(value: &Value, field: &str) -> Result<Source, Refusal> {
    let text = value.get(field).and_then(Value::as_str).unwrap_or("");
    not_a_receiver(
        field,
        Source::parse(text).ok_or_else(|| {
            Refusal::rejected(
                field,
                format!(
                    "'{}' is not a source: the catalog declares {}",
                    text, SOURCE_SPELLINGS
                ),
            )
        })?,
    )
}

/// (goal 17) An alarm's source: a group's sources and `stored:<id>`.
fn alarm_source_field(value: &Value, field: &str) -> Result<Source, Refusal> {
    let text = value.get(field).and_then(Value::as_str).unwrap_or("");
    not_a_receiver(
        field,
        Source::parse(text).ok_or_else(|| {
            Refusal::rejected(
                field,
                format!(
                    "'{}' is not a source: the catalog declares {}",
                    text, ALARM_SOURCE_SPELLINGS
                ),
            )
        })?,
    )
}

/// (goal 17) No command names a Soloist receiver as a source: a receiver's
/// audio follows the Spotify app, and the server's receiver manager is the
/// only one that gives a group that source (design: take the room, K78).
fn not_a_receiver(field: &str, source: Source) -> Result<Source, Refusal> {
    match source {
        Source::Soloist(id) => Err(Refusal::rejected(
            field,
            format!(
                "'soloist:{}' is a Spotify receiver, which plays where the Spotify app plays \
                 it: choose the room or group as the device in the Spotify app; a command \
                 cannot name one",
                id
            ),
        )),
        other => Ok(other),
    }
}

fn identifier_list(value: &Value, field: &str) -> Result<Vec<String>, Refusal> {
    let items = match value.get(field) {
        Some(Value::Arr(items)) => items,
        _ => {
            return Err(Refusal::rejected(
                field,
                format!("the field '{}' is not an array of identifiers", field),
            ))
        }
    };
    let mut out: Vec<String> = Vec::new();
    for item in items {
        let text = item.as_str().unwrap_or("");
        if !is_identifier(text) {
            return Err(Refusal::rejected(
                field,
                format!(
                    "'{}' is not an identifier: the catalog declares 1 to {} characters of \
                     lower-case letters, digits and hyphens",
                    text, MAX_IDENTIFIER_LEN
                ),
            ));
        }
        if out.iter().any(|o| o == text) {
            return Err(Refusal::rejected(
                field,
                format!("'{}' is listed twice", text),
            ));
        }
        out.push(text.to_string());
    }
    if out.len() > MAX_DEFINITIONS {
        return Err(Refusal::rejected(
            field,
            format!("at most {} are listed", MAX_DEFINITIONS),
        ));
    }
    Ok(out)
}

/// The objects of an array field, each held to its own declared fields.
fn objects<'a>(
    value: &'a Value,
    field: &str,
    declared: &[&str],
) -> Result<Vec<&'a Value>, Refusal> {
    let items = match value.get(field) {
        Some(Value::Arr(items)) => items,
        _ => {
            return Err(Refusal::rejected(
                field,
                format!("the field '{}' is not an array", field),
            ))
        }
    };
    let mut out = Vec::new();
    for item in items {
        match item {
            Value::Obj(members) => {
                expect_fields_with(members, declared, &[]).map_err(|r| {
                    Refusal::rejected(field, format!("in '{}': {}", field, r.detail))
                })?;
                out.push(item);
            }
            other => {
                return Err(Refusal::rejected(
                    field,
                    format!(
                        "every member of '{}' is an object and one is {}",
                        field,
                        other.kind()
                    ),
                ))
            }
        }
    }
    Ok(out)
}

fn bond_members(value: &Value) -> Result<Vec<BondMember>, Refusal> {
    let mut members: Vec<BondMember> = Vec::new();
    for item in objects(value, "members", &["endpoint", "role"])? {
        let endpoint = identifier(item, "endpoint")
            .map_err(|r| Refusal::rejected("members", format!("in 'members': {}", r.detail)))?;
        let word = item.get("role").and_then(Value::as_str).unwrap_or("");
        let role = Role::parse(word).ok_or_else(|| {
            Refusal::rejected(
                "members",
                format!(
                    "endpoint '{}' has the role '{}', which is not a channel position a bonded \
                     set uses ({}; docs/protocol.md, the channel map)",
                    endpoint,
                    word,
                    Role::ALL.map(|r| r.name()).join(", ")
                ),
            )
        })?;
        if members.iter().any(|m| m.endpoint == endpoint) {
            return Err(Refusal::rejected(
                "members",
                format!(
                    "endpoint '{}' is listed twice; an endpoint plays one role",
                    endpoint
                ),
            ));
        }
        if let Some(other) = members.iter().find(|m| m.role == role) {
            return Err(Refusal::rejected(
                "members",
                format!(
                    "endpoints '{}' and '{}' both play {}; a role is played by one endpoint",
                    other.endpoint, endpoint, role
                ),
            ));
        }
        members.push(BondMember { endpoint, role });
    }
    let roles: Vec<Role> = members.iter().map(|m| m.role).collect();
    if let Some(problem) = validate_layout(&roles) {
        return Err(Refusal::rejected("members", problem));
    }
    Ok(members)
}

fn quiet_windows(value: &Value) -> Result<Vec<QuietWindow>, Refusal> {
    let items = objects(value, "windows", &["days", "start", "end", "limit"])?;
    if items.len() > MAX_QUIET_WINDOWS {
        return Err(Refusal::rejected(
            "windows",
            format!(
                "a room has at most {} quiet-hours windows",
                MAX_QUIET_WINDOWS
            ),
        ));
    }
    let mut out = Vec::new();
    for item in items {
        let inner =
            |r: Refusal| Refusal::rejected("windows", format!("in 'windows': {}", r.detail));
        let window = QuietWindow {
            days: days_field(item, "days").map_err(inner)?,
            start: clock_field(item, "start").map_err(inner)?,
            end: clock_field(item, "end").map_err(inner)?,
            limit: volume_field(item, "limit").map_err(inner)?,
        };
        if let Some(problem) = window.problem() {
            return Err(Refusal::rejected("windows", problem));
        }
        out.push(window);
    }
    Ok(out)
}

/// What a refusal says when a message tries to change a zone's transport.
///
/// The rule is not new and this is not a new rule: `docs/control-plane.md`
/// already states that the set of zones is CONFIGURED and not commanded,
/// because the rooms in a house are a fact about a building. Which wire or
/// radio a room is on is the same kind of fact, so it is declared in the same
/// place, and a message that tries to change it gets told where it is declared
/// rather than a generic complaint about an unknown field.
pub const TRANSPORT_IS_CONFIGURED: &str =
    "a zone's transport is CONFIGURED and not commanded, exactly as the set of zones is: it is \
     declared with '--zone <id>=<transport>' on the server's command line, and the tiers and \
     what each is held to are committed in config/transport.conf. No message in this catalog \
     changes it, and nothing in this message has been applied";

/// Refuse a message carrying a field the command does not declare, or
/// missing one it requires. `optional` fields may be absent.
fn expect_fields_with(
    members: &[(String, Value)],
    declared: &[&str],
    optional: &[&str],
) -> Result<(), Refusal> {
    for (key, _) in members {
        if !declared.contains(&key.as_str()) && !optional.contains(&key.as_str()) {
            if key == "transport" {
                return Err(Refusal::rejected(
                    "transport",
                    TRANSPORT_IS_CONFIGURED.to_string(),
                ));
            }
            return Err(Refusal::rejected(
                key,
                format!(
                    "this command declares the fields {} and carries no '{}'",
                    declared
                        .iter()
                        .chain(optional.iter())
                        .copied()
                        .collect::<Vec<_>>()
                        .join(", "),
                    key
                ),
            ));
        }
    }
    for key in declared {
        if !members.iter().any(|(k, _)| k == key) {
            return Err(Refusal::rejected(
                key,
                format!("this command requires the field '{}' and it is absent", key),
            ));
        }
    }
    Ok(())
}

/// A zone, group or endpoint identifier: lower-case letters, digits and
/// hyphens, one to [`MAX_IDENTIFIER_LEN`] of them.
///
/// Narrow on purpose. An identifier appears in a DNS-SD instance name, in a
/// persisted state file's section header and in a URL query, and a character
/// that has to be escaped differently in each of those is a character that will
/// eventually be escaped wrongly in one of them.
pub fn is_identifier(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= MAX_IDENTIFIER_LEN
        && text
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn identifier(value: &Value, field: &str) -> Result<String, Refusal> {
    let text = match value.get(field).and_then(Value::as_str) {
        Some(t) => t,
        None => {
            return Err(Refusal::rejected(
                field,
                format!("the field '{}' is not a string", field),
            ))
        }
    };
    if !is_identifier(text) {
        return Err(Refusal::rejected(
            field,
            format!(
                "'{}' is not an identifier: the catalog declares 1 to {} characters of lower-case \
                 letters, digits and hyphens",
                text, MAX_IDENTIFIER_LEN
            ),
        ));
    }
    Ok(text.to_string())
}

/// A human-set name.
///
/// Anything printable up to [`MAX_NAME_LEN`] characters, with no control
/// character in it: the persisted state format holds a name on one line, and a
/// name carrying a newline would be a name that does not come back.
pub fn is_display_name(text: &str) -> bool {
    !text.is_empty()
        && text.chars().count() <= MAX_NAME_LEN
        && !text.chars().any(|c| c.is_control())
        && text.trim() == text
}

fn display_name(value: &Value, field: &str) -> Result<String, Refusal> {
    let text = match value.get(field).and_then(Value::as_str) {
        Some(t) => t,
        None => {
            return Err(Refusal::rejected(
                field,
                format!("the field '{}' is not a string", field),
            ))
        }
    };
    if !is_display_name(text) {
        return Err(Refusal::rejected(
            field,
            format!(
                "'{}' is not a name: the catalog declares 1 to {} characters, no control \
                 character, and no leading or trailing space",
                text.escape_debug(),
                MAX_NAME_LEN
            ),
        ));
    }
    Ok(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_volume_prints_with_three_places_and_reads_back_identical() {
        for thousandths in [0u32, 1, 500, 999, 1_000] {
            let v = Volume::from_thousandths(i64::from(thousandths)).unwrap();
            let text = v.literal();
            assert_eq!(text.len(), 5, "{}", text);
            assert_eq!(Volume::parse(&text), Some(v), "{}", text);
        }
        assert_eq!(Volume::SILENT.literal(), "0.000");
        assert_eq!(Volume::FULL.literal(), "1.000");
    }

    #[test]
    fn a_volume_outside_the_declared_range_is_not_a_volume() {
        assert_eq!(Volume::from_thousandths(1_001), None);
        assert_eq!(Volume::from_thousandths(-1), None);
        for text in [
            "1.001", "2.000", "-0.500", "0.5001", "1e0", ".5", "0.5x", "",
        ] {
            assert_eq!(Volume::parse(text), None, "{}", text);
        }
    }

    #[test]
    fn every_command_round_trips_through_its_own_bytes() {
        for command in [
            Command::Hello,
            Command::Attach {
                zone: "kitchen".to_string(),
                endpoint: "endpoint-a".to_string(),
                link: None,
            },
            Command::Attach {
                zone: "kitchen".to_string(),
                endpoint: "endpoint-a".to_string(),
                link: Some(Link::Wired),
            },
            Command::Name {
                zone: "kitchen".to_string(),
                name: "Kitchen".to_string(),
            },
            Command::Group {
                zone: "kitchen".to_string(),
                group: "downstairs".to_string(),
            },
            Command::Ungroup {
                zone: "kitchen".to_string(),
            },
            Command::Volume {
                zone: "kitchen".to_string(),
                volume: Volume::from_thousandths(500).unwrap(),
            },
            Command::Mute {
                zone: "kitchen".to_string(),
                muted: true,
            },
        ] {
            let bytes = command.encode();
            assert_eq!(decode_command(&bytes), Ok(command), "{}", bytes);
        }
    }

    #[test]
    fn an_unknown_version_refuses_the_session_and_names_both_sides() {
        let refusal = decode_command(r#"{"v":9,"t":"hello"}"#).unwrap_err();
        assert!(refusal.ends_the_session());
        assert_eq!(refusal.field, "v");
        assert!(
            refusal.detail.contains("catalog version 9 was offered"),
            "{}",
            refusal
        );
        assert!(refusal.detail.contains("implements 1"), "{}", refusal);
        assert!(
            refusal.encode().contains(r#""offered":9"#),
            "{}",
            refusal.encode()
        );
        assert!(
            refusal.encode().contains(r#""implemented":[1,2]"#),
            "{}",
            refusal.encode()
        );
    }

    #[test]
    fn a_field_the_command_does_not_declare_is_refused() {
        let refusal =
            decode_command(r#"{"v":1,"t":"ungroup","zone":"kitchen","volume":0.500}"#).unwrap_err();
        assert_eq!(refusal.field, "volume");
        assert!(!refusal.ends_the_session());
    }
}
