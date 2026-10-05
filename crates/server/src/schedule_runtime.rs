//! The schedule runtime: alarms, sleep timers, quiet hours and line-in
//! autoplay, driven over the room model by time the caller passes in.
//!
//! # What this is, and what it is not
//!
//! A PURE module. It reads no clock, opens no socket, spawns no thread and
//! touches no PCM. The server's conductor thread (another track's) owns all of
//! that: it reads the monotonic clock and the civil clock, calls one of the
//! entry points below, and carries out the [`Effect`]s that come back, in
//! order. Because every instant arrives as an argument, every rule here is
//! tested at any instant of any year with modelled time
//! (`crates/server/tests/schedule_runtime.rs`), including the two nights a year
//! daylight saving makes awkward. `audio-path.conf` records this unit as
//! excluded: civil time for scheduling only (K30), and what crosses towards the
//! audio path is a volume in thousandths or a source id, never a timestamp.
//!
//! # The entry points
//!
//! - [`Runtime::tick`]: time passed. Quiet hours follow the civil time, ramps
//!   and fades take their next step, sleep timers and autoplay holds count
//!   down, and alarms due since the previous tick fire. The caller ticks at
//!   least once a second (ASSUMED: alarms and quiet windows are minute
//!   resolution, ramps step once a second) and whenever
//!   [`Runtime::next_deadline_ns`] says.
//! - [`Runtime::on_command_applied`]: a PERSON's command was applied to the
//!   zones (the control plane's or a controller's, never one this runtime
//!   applied itself). It is how `alarm_stop`, `sleep`, and "a person touched a
//!   room this runtime is holding" reach it. A volume or a mute on a room an
//!   autoplay holds keeps the hold (it is not a source change; goal 13), and
//!   ends an alarm holding it, as any command naming the room does.
//! - [`Runtime::on_input_signal`] and [`Runtime::on_input_gone`]: a line-in's
//!   `source_offer` changed, or its endpoint's session ended.
//! - [`Runtime::on_input_standby`] (goal 13, K81): a TV input's signal ended
//!   because the TV went to standby. A TV autoplay whose rule says
//!   `stop_on_standby` (the default) stops at once, without
//!   [`AUTOPLAY_HOLD_MS`], and restores what its rooms played; otherwise it is
//!   a signal gone, hold and all. TV on is an ordinary signal: autoplay's
//!   `take` already takes the target room out of any group it was in.
//! - [`Runtime::on_alarm_source_started`] and
//!   [`Runtime::on_alarm_source_failed`] (goal 17): the answers to
//!   [`Effect::PlayStored`] and [`Effect::PlaySpotify`]. An alarm whose
//!   source is a stored source (`stored:<id>`) cannot be played by the room
//!   model alone: something outside this module (a network media player, a
//!   Soloist receiver) has to start, and it can fail at once or minutes
//!   later. The alarm fires in silence, asks for the play with an effect,
//!   and is told either which source its group plays now (the player, the
//!   receiver) or that it failed, when it rings the fallback chime with the
//!   reason given: an alarm must still wake.
//!
//! # The effects, and the order they come in
//!
//! The runtime changes the zones itself, through the model's own hooks and
//! [`Zones::apply`], so every volume it sets goes through the model's clamp
//! (never above the room's effective limit) and the state a subscriber is sent
//! is the truth. What it cannot do itself comes back as [`Effect`]s:
//!
//! 1. [`Effect::Log`]: a line for the server's log, `schedule ...` key=value.
//! 2. [`Effect::RoomVolume`] for rooms getting QUIETER.
//! 3. [`Effect::SetSource`]: what a formed group plays now changed.
//! 4. [`Effect::SourceControl`]: start or stop an endpoint's input; stops first.
//! 5. [`Effect::RoomVolume`] for rooms getting LOUDER (or only their limit
//!    moving).
//! 6. [`Effect::Persist`]: a persisted fact changed (membership, a volume at
//!    rest, a one-shot alarm disabled); write the state file. Ramp steps do not
//!    ask for it; the end of a ramp does.
//!
//! (goal 17) [`Effect::PlayStored`], [`Effect::PlaySpotify`] and
//! [`Effect::StopStored`] come with the log lines, first: they are requests
//! to the conductor, which answers a play through the two entry points above
//! after it has carried out the rest.
//!
//! Quieter first and louder last is what keeps a source change from being
//! heard at the wrong level: an alarm firing in a room playing music at half
//! volume sends the room to silence before the chime starts, and the end of an
//! alarm silences the chime before the room's own volume comes back.
//!
//! The sources and the room volumes are DERIVED: after every call the runtime
//! compares each formed group's source and each room's gain and effective
//! limit with what it last reported, and reports the difference, whoever made
//! the change (a person's `volume` comes back as a `room_volume` with
//! `ramp_ms` 0). So the conductor needs no diff of its own: it sends each
//! session's first `room_volume` at session start (ADR 0074, "What the server
//! must send") and forwards every later one from here. The same holds for
//! `source_control`: an input is started while some formed group plays it and
//! its endpoint is connected, and stopped when none does. The zones' serial
//! still moves on every change, so the conductor fans the state out exactly as
//! after a command when it moved.
//!
//! The decisions, and every ASSUMED value below, are recorded in
//! `docs/decisions/0076-the-schedule-runtime.md`.

use std::collections::BTreeMap;

use chorus_control::rooms::{
    Alarm as AlarmConfig, CivilTime, ClockTime, InputId, InputRole, Source, StoredKind,
};
use chorus_control::{Command, Volume, Zones};
use chorus_schedule::{
    due_between, Alarm as AlarmSchedule, Chime, Days, Ramp, SleepTimer, TimeOfDay, Zone,
};

/// (ADR 0194) The built-in chimes an alarm may name, in the schedule
/// library's own order: its list (`chorus_schedule::chime::CHIMES`), read and
/// never copied, so a chime added there is listed here.
pub fn chime_names() -> Vec<String> {
    chorus_schedule::chime::CHIMES
        .iter()
        .map(|c| c.name().to_string())
        .collect()
}

/// (ADR 0194) Tell a room model that a schedule runtime runs over it, before
/// any state is served: the chimes an alarm may name are listed in the state
/// (`chimes`), and each sleep timer carries how long it has left
/// (`remaining_s`), which [`Runtime::tick`] counts.
pub fn describe_in(zones: &mut Zones) {
    zones.set_chimes(chime_names());
    zones.set_sleep_counted(true);
}

/// (ADR 0194) The room model's word for a source endpoint's kind of input:
/// the same three, by the same names.
pub fn input_kind(kind: chorus_protocol::v2::SourceKind) -> chorus_control::rooms::InputKind {
    use chorus_control::rooms::InputKind;
    use chorus_protocol::v2::SourceKind;
    match kind {
        SourceKind::LineIn => InputKind::LineIn,
        SourceKind::Optical => InputKind::Optical,
        SourceKind::HdmiArc => InputKind::HdmiArc,
    }
}

const NS_PER_MS: u64 = 1_000_000;
const NS_PER_S: u64 = 1_000_000_000;

/// How long one ramp segment is, ms. Every ramp the runtime runs (an alarm's
/// rise, the fade that ends an alarm, a sleep fade) is sent as successive
/// `room_volume` messages, each with this `ramp_ms` and the gain the plan
/// reaches at the end of the segment, so the endpoint draws one straight line
/// and anything that changes mid-ramp (a limit, a quiet window starting, a
/// person) holds within one segment. ASSUMED: one second; short enough that a
/// cap is never overshot audibly for long, long enough that a ramp is one
/// message a second per room, not a stream.
pub const STEP_MS: u64 = 1000;

/// How long the fade that ends an alarm is, ms. ASSUMED: two seconds; a cut
/// would be harsh, and a person who said stop wants it gone soon.
pub const END_FADE_MS: u64 = 2000;

/// How late an alarm may fire, seconds. One due longer ago than this (the
/// server was down, the clock stepped forward) is skipped and logged rather
/// than ringing at a time nobody asked for. ASSUMED: one minute.
pub const LATE_FIRE_S: i64 = 60;

/// How long a line-in that autoplay started keeps playing after its signal
/// goes, ms, before it is stopped and the rooms restored. On top of the
/// endpoint's own 2 s withdrawal hysteresis (ADR 0066). ASSUMED: thirty
/// seconds, so a record being turned over does not hand the room back.
pub const AUTOPLAY_HOLD_MS: u64 = 30_000;

/// How long a room's gain is ramped down before a lowered limit (a quiet
/// window starting, a lower `limit`) is sent, ms: ADR 0074 asks the server to
/// ramp the gain first and lower the limit after, because the endpoint applies
/// a new limit at once. ASSUMED: one second, one ramp segment.
pub const LIMIT_PULL_MS: u64 = 1000;

/// The chime an alarm plays when its own source cannot be played (a line-in
/// that is not offered or not connected, a chime name that does not exist, a
/// source of `none`). ASSUMED: the bell, the first of `docs/chimes.md`.
pub const FALLBACK_CHIME: Chime = Chime::Bell;

/// Start or stop an endpoint's input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputAction {
    /// Start sending it (`source_control` start).
    Start,
    /// Stop sending it (`source_control` stop).
    Stop,
}

/// Something the conductor must do that the room model cannot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// A line for the server's log.
    Log(String),
    /// What a formed group plays changed: route the group's slot to it.
    /// `Chime(name)` is a generated chime by `chorus_schedule::Chime` name
    /// (the slot repeats it while it is the source), `LineIn(input)` the
    /// endpoint's input, `None` silence, `Stream` the configured stream.
    SetSource {
        /// The group.
        group: String,
        /// What it plays now.
        source: Source,
    },
    /// Send `source_control` to the endpoint offering `input`; the conductor
    /// maps the input's name to its `source_id` and picks the codec.
    SourceControl {
        /// The input.
        input: InputId,
        /// Start or stop.
        action: InputAction,
    },
    /// Send `room_volume` to every endpoint of `zone` (a room in a group gets
    /// its own room's values).
    RoomVolume {
        /// The room (zone on the wire).
        zone: String,
        /// Thousandths, already 0 when muted, never above `limit`.
        gain: u16,
        /// The room's effective limit, thousandths.
        limit: u16,
        /// Move to `gain` over this long, ms; 0 is at once. At most
        /// [`STEP_MS`] or [`END_FADE_MS`], far under the wire's 60000.
        ramp_ms: u16,
    },
    /// A persisted fact changed: write the state file.
    Persist,
    /// (goal 17) A ringing alarm's source is a stored stream URL: play it on
    /// a network media player (`PlayerSessions::play_held`, owner
    /// `alarm:<alarm>`, via `alarm`). The conductor answers with
    /// [`Runtime::on_alarm_source_started`] naming the player source, or
    /// [`Runtime::on_alarm_source_failed`] with a reason (no player, a
    /// refused URL, a fetch or decode failure, at once or later).
    PlayStored {
        /// The alarm.
        alarm: String,
        /// The alarm's target: the group that rings.
        target: String,
        /// The stored source's id.
        stored: String,
        /// The `http` or `https` URL.
        url: String,
        /// The stored source's name: the title the room shows.
        name: String,
    },
    /// (goal 17) THE SEAM FOR THE SPOTIFY ALARM SOURCE, filled by the
    /// Soloist server track. A ringing alarm's source is a stored Spotify
    /// URI and Soloist alarms are switched on
    /// ([`Runtime::set_soloist_alarms`]): play it on the target's receiver.
    /// Whoever carries it out answers exactly as for [`Effect::PlayStored`]:
    /// [`Runtime::on_alarm_source_started`] with the receiver's source once
    /// it plays, or [`Runtime::on_alarm_source_failed`] with one of
    /// `soloist-unavailable`, `soloist-logged-out`, `soloist-expired`,
    /// `soloist-timeout`. Until that track lands the conductor answers
    /// `soloist-unavailable` at once, and the switch is off, so the runtime
    /// never emits this and rings the chime with reason `soloist-off`.
    PlaySpotify {
        /// The alarm.
        alarm: String,
        /// The alarm's target: the group that rings.
        target: String,
        /// The stored source's id.
        stored: String,
        /// The Spotify URI.
        uri: String,
        /// The stored source's name.
        name: String,
    },
    /// (goal 17) The alarm no longer plays what was started for it (it
    /// ended, or its rooms were taken): stop it and give it back.
    StopStored {
        /// The alarm.
        alarm: String,
    },
}

/// What a room was doing before the runtime took it, to put it back.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Snapshot {
    group: String,
    volume: Volume,
    muted: bool,
    source: Source,
    /// The other rooms of its group then, so a live group that dissolved
    /// while the room was away can be formed again.
    peers: Vec<String>,
}

/// Who holds a room.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Owner {
    Alarm(String),
    Autoplay(InputId),
}

/// A room the runtime took, and what it was before. One per room: a second
/// activity taking a room inherits the first one's snapshot, so the room goes
/// back to what it was before EITHER took it.
#[derive(Debug, Clone)]
struct Hold {
    owner: Owner,
    snapshot: Snapshot,
}

/// A ramp the runtime is stepping in one room.
#[derive(Debug, Clone, Copy)]
struct Stepped {
    ramp: Ramp,
    /// When the plan's elapsed time is 0, monotonic ns.
    start_ns: u64,
    /// When the next segment is due, monotonic ns.
    next_ns: u64,
    /// An alarm's rise: also held under the model's ramp target, which the
    /// model keeps clamped to the effective limit.
    rise: bool,
}

/// An alarm ringing now.
#[derive(Debug, Clone)]
struct Ringing {
    id: String,
    /// The group it plays in: its target's id.
    group: String,
    /// What it actually plays (after any fallback).
    source: Source,
    /// When it stops by itself (`duration_min`), monotonic ns.
    end_ns: Option<u64>,
    /// Set once it is ending: when the end fade is over.
    ending_until: Option<u64>,
    /// (goal 17) The stored source it plays or waits for, by id.
    stored: Option<String>,
    /// (goal 17) Whether it still waits to be told its stored source
    /// started ([`Runtime::on_alarm_source_started`]); its group plays
    /// nothing until then.
    waiting: bool,
}

/// What an alarm plays when it fires.
enum Plan {
    /// A source the room model plays itself.
    Plays(Source),
    /// The fallback chime, and why.
    Fallback(&'static str),
    /// (goal 17) A stored source something outside has to start.
    Stored(chorus_control::rooms::StoredSource),
}

/// A line-in autoplay started.
#[derive(Debug, Clone)]
struct Playing {
    input: InputId,
    group: String,
    /// When the signal went, monotonic ns; the hold runs from here.
    off_since: Option<u64>,
}

/// A sleep timer counting down.
#[derive(Debug, Clone)]
struct Sleeping {
    target: String,
    timer: SleepTimer,
    /// Captured when the fade begins: each room and its volume then.
    fade: Option<Vec<(String, Volume)>>,
}

/// What a room was last told.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Sent {
    gain: u16,
    limit: u16,
}

/// The schedule runtime's state. One per server, owned by the conductor.
#[derive(Debug)]
pub struct Runtime {
    tz: Zone,
    /// The civil instant of the previous tick; `None` before the first.
    last_utc: Option<i64>,
    /// Each alarm's last ring instant, so none rings twice for one instant,
    /// whatever the wall clock does.
    last_fired: BTreeMap<String, i64>,
    holds: BTreeMap<String, Hold>,
    ramps: BTreeMap<String, Stepped>,
    ringing: Vec<Ringing>,
    playing: Vec<Playing>,
    sleeping: Vec<Sleeping>,
    /// Inputs whose endpoint is connected (it has sent a `source_offer`).
    connected: Vec<InputId>,
    /// Inputs this runtime has started.
    started: Vec<InputId>,
    /// What each formed group was last reported to play.
    sources: BTreeMap<String, Source>,
    /// What each room was last sent.
    sent: BTreeMap<String, Sent>,
    /// A lowered limit waiting for the gain ramp ahead of it, by room: when
    /// it may go out, monotonic ns.
    pending_limit: BTreeMap<String, u64>,
    /// The `ramp_ms` the next report of a room carries.
    hint: BTreeMap<String, u16>,
    persist: bool,
    logs: Vec<Effect>,
    /// (goal 17) Whether a stored Spotify URI is played at all
    /// (`--soloist-alarms`; off by default, P7).
    soloist_alarms: bool,
}

fn vol(thousandths: u16) -> Volume {
    Volume::from_thousandths(i64::from(thousandths)).unwrap_or(Volume::FULL)
}

fn th(v: Volume) -> u16 {
    v.thousandths() as u16
}

/// The schedule library's view of a configured alarm.
fn schedule_of(alarm: &AlarmConfig) -> AlarmSchedule {
    let minutes = alarm.time.minutes();
    let time =
        TimeOfDay::new((minutes / 60) as u8, (minutes % 60) as u8).unwrap_or(TimeOfDay::MIDNIGHT);
    let days = Days::from_bits(alarm.days.mask()).unwrap_or(Days::NONE);
    let mut a = AlarmSchedule::new(time, days);
    a.enabled = alarm.enabled;
    a
}

fn ms_u16(ns: u64) -> u16 {
    (ns / NS_PER_MS).min(u64::from(u16::MAX)) as u16
}

impl Runtime {
    /// A runtime keeping civil time in `tz` (the server loads it at start:
    /// `--tz`, `$TZ`, `/etc/localtime`, else UTC).
    pub fn new(tz: Zone) -> Runtime {
        Runtime {
            tz,
            last_utc: None,
            last_fired: BTreeMap::new(),
            holds: BTreeMap::new(),
            ramps: BTreeMap::new(),
            ringing: Vec::new(),
            playing: Vec::new(),
            sleeping: Vec::new(),
            connected: Vec::new(),
            started: Vec::new(),
            sources: BTreeMap::new(),
            sent: BTreeMap::new(),
            pending_limit: BTreeMap::new(),
            hint: BTreeMap::new(),
            persist: false,
            logs: Vec::new(),
            soloist_alarms: false,
        }
    }

    /// (goal 17) THE SEAM FOR THE SPOTIFY ALARM SOURCE, filled by the
    /// Soloist server track: switch stored Spotify URIs on as alarm sources
    /// (`--soloist-alarms`). Off, which is how every server starts, an alarm
    /// whose source is one rings the fallback chime with reason
    /// `soloist-off`; on, it fires in silence and asks with
    /// [`Effect::PlaySpotify`].
    pub fn set_soloist_alarms(&mut self, on: bool) {
        self.soloist_alarms = on;
    }

    /// (goal 17) The stored source a ringing alarm plays or waits for.
    pub fn alarm_stored(&self, alarm: &str) -> Option<&str> {
        self.ringing
            .iter()
            .find(|r| r.id == alarm)
            .and_then(|r| r.stored.as_deref())
    }

    /// (goal 17) The answer to [`Effect::PlayStored`] or
    /// [`Effect::PlaySpotify`]: what was asked for is ready, and `source`
    /// (`player:<id>` today; the receiver's source when the Soloist server
    /// track lands) is what the alarm's group plays from now. Returns whether
    /// the alarm took it: `false` when it no longer rings, is ending, was
    /// not waiting, or the room model refused the source (the alarm then
    /// rings the fallback chime, reason `source-refused`); the caller then
    /// gives the player or the receiver back.
    pub fn on_alarm_source_started(
        &mut self,
        alarm: &str,
        source: Source,
        now_mono_ns: u64,
        zones: &mut Zones,
    ) -> (bool, Vec<Effect>) {
        let at = self
            .ringing
            .iter()
            .position(|r| r.id == alarm && r.waiting && r.ending_until.is_none());
        let Some(at) = at else {
            return (false, self.finish(now_mono_ns, zones));
        };
        let group = self.ringing[at].group.clone();
        if members(&group, zones).is_empty() || zones.source(&group) != self.ringing[at].source {
            // Its group plays something else now: a person chose.
            return (false, self.finish(now_mono_ns, zones));
        }
        if let Err(refusal) = zones.set_group_source(&group, source.clone()) {
            self.fall_back(at, "source-refused", &refusal.detail, zones);
            return (false, self.finish(now_mono_ns, zones));
        }
        self.ringing[at].source = source.clone();
        self.ringing[at].waiting = false;
        let line = format!(
            "schedule alarm={} started stored={} plays={}",
            alarm,
            self.ringing[at].stored.as_deref().unwrap_or("-"),
            source.literal()
        );
        self.log(line);
        (true, self.finish(now_mono_ns, zones))
    }

    /// (goal 17) The other answer: what the alarm's stored source needed
    /// could not start, or stopped (`reason` is one word for the log,
    /// `detail` the fetcher's, the decoder's or the receiver's own). The
    /// alarm rings the fallback chime from now. Nothing happens for an alarm
    /// that no longer rings, is ending, or plays no stored source.
    pub fn on_alarm_source_failed(
        &mut self,
        alarm: &str,
        reason: &str,
        detail: &str,
        now_mono_ns: u64,
        zones: &mut Zones,
    ) -> Vec<Effect> {
        let at = self
            .ringing
            .iter()
            .position(|r| r.id == alarm && r.stored.is_some() && r.ending_until.is_none());
        if let Some(at) = at {
            self.fall_back(at, reason, detail, zones);
        }
        self.finish(now_mono_ns, zones)
    }

    /// Ringing alarm `at` plays the fallback chime instead of its stored
    /// source, when its group still plays what the alarm gave it.
    fn fall_back(&mut self, at: usize, reason: &str, detail: &str, zones: &mut Zones) {
        let fallback = Source::Chime(FALLBACK_CHIME.name().to_string());
        let group = self.ringing[at].group.clone();
        if !members(&group, zones).is_empty() && zones.source(&group) == self.ringing[at].source {
            let _ = zones.set_group_source(&group, fallback.clone());
        }
        let wanted = self.ringing[at].stored.take().unwrap_or_default();
        self.ringing[at].source = fallback.clone();
        self.ringing[at].waiting = false;
        let line = format!(
            "schedule alarm={} fallback=chime reason={} wanted=stored:{} plays={} detail=\"{}\"",
            self.ringing[at].id,
            reason,
            wanted,
            fallback.literal(),
            detail.replace('"', "'")
        );
        self.log(line);
    }

    /// The alarms ringing now, ending ones included, by id.
    pub fn ringing(&self) -> Vec<&str> {
        self.ringing.iter().map(|r| r.id.as_str()).collect()
    }

    /// When an alarm last rang (UTC seconds), as far as this process knows.
    pub fn last_fired(&self, alarm: &str) -> Option<i64> {
        self.last_fired.get(alarm).copied()
    }

    /// Whether a room is held by an alarm or an autoplay, to be restored.
    pub fn is_held(&self, zone: &str) -> bool {
        self.holds.contains_key(zone)
    }

    /// Whether autoplay is playing an input now (holds included).
    pub fn is_autoplaying(&self, input: &InputId) -> bool {
        self.playing.iter().any(|p| p.input == *input)
    }

    /// Whether a sleep timer is counting down for a target.
    pub fn is_sleeping(&self, target: &str) -> bool {
        self.sleeping.iter().any(|s| s.target == target)
    }

    /// (goal 18) An announcement that the runtime displaced is over, and
    /// what it had given a group must not come back: every room the runtime
    /// holds whose snapshot says it played `player` (the announcement's
    /// player, given back by now) goes back to `previous` instead, and one
    /// whose snapshot holds a volume the announcement set (whether or not
    /// it played the player: an announcement mixed over its rooms gives
    /// nobody its player) goes back to the volume it had before (`volumes`: the room, the volume it had, the
    /// volume the announcement gave it). Returns how many snapshots changed.
    pub fn announcement_over(
        &mut self,
        player: &Source,
        previous: &Source,
        volumes: &[(String, Volume, Volume)],
    ) -> usize {
        let mut changed = 0;
        for (room, hold) in self.holds.iter_mut() {
            let snap = &mut hold.snapshot;
            let mut moved = false;
            if snap.source == *player {
                snap.source = previous.clone();
                moved = true;
            }
            // (ADR 0175) An announcement mixed over its rooms gave no group
            // its player, so only the volume it set can be in a snapshot.
            if let Some((_, before, set)) = volumes.iter().find(|(r, _, _)| r == room) {
                if snap.volume == *set && set != before {
                    snap.volume = *before;
                    moved = true;
                }
            }
            changed += usize::from(moved);
        }
        changed
    }

    /// The earliest monotonic instant something is due: a ramp segment, an
    /// alarm's end, a sleep fade or expiry, an autoplay hold, a lowered
    /// limit. The caller ticks by then (and at least once a second for the
    /// civil schedule).
    pub fn next_deadline_ns(&self) -> Option<u64> {
        let ramps = self.ramps.values().map(|r| r.next_ns);
        let alarms = self
            .ringing
            .iter()
            .filter_map(|r| r.ending_until.or(r.end_ns));
        let sleeps = self.sleeping.iter().map(|s| {
            if s.fade.is_some() {
                s.timer.expiry_ns()
            } else {
                s.timer.fade_start_ns()
            }
        });
        let holds = self
            .playing
            .iter()
            .filter_map(|p| p.off_since.map(|t| t + AUTOPLAY_HOLD_MS * NS_PER_MS));
        let limits = self.pending_limit.values().copied();
        ramps
            .chain(alarms)
            .chain(sleeps)
            .chain(holds)
            .chain(limits)
            .min()
    }

    /// Time passed: `now_mono_ns` from the monotonic clock, `now_utc_s` from
    /// the civil one (UTC seconds since the Unix epoch).
    pub fn tick(&mut self, now_mono_ns: u64, now_utc_s: i64, zones: &mut Zones) -> Vec<Effect> {
        self.quiet_hours(now_utc_s, zones);
        self.step_ramps(now_mono_ns, zones);
        self.step_sleeps(now_mono_ns, zones);
        self.step_alarms(now_mono_ns, zones);
        self.step_autoplay(now_mono_ns, zones);
        self.poll_alarms(now_mono_ns, now_utc_s, zones);
        self.finish(now_mono_ns, zones)
    }

    /// A person's command was applied to `zones`. Never called for a change
    /// this runtime made itself.
    pub fn on_command_applied(
        &mut self,
        command: &Command,
        now_mono_ns: u64,
        zones: &mut Zones,
    ) -> Vec<Effect> {
        match command {
            Command::AlarmStop { alarm } => self.end_alarm(alarm, &[], now_mono_ns, "stop", zones),
            Command::AlarmDelete { alarm } => {
                self.end_alarm(alarm, &[], now_mono_ns, "deleted", zones)
            }
            // (goal 17) An input labelled a streamer while its signal is
            // present plays now, as its signal appearing would: the label is
            // the rule.
            Command::InputLabel(label)
                if label.role == InputRole::Streamer
                    && zones.inputs().contains(&label.input)
                    && !self.playing.iter().any(|p| p.input == label.input) =>
            {
                let input = label.input.clone();
                if let Some(target) = self.autoplay_target(&input, zones) {
                    self.start_autoplay(&input, &target, zones);
                }
            }
            Command::Sleep { target, minutes } => {
                let reason = if *minutes == 0 { "sleep-0" } else { "replaced" };
                self.cancel_sleep(target, &[], reason, zones);
                if let Some(timer) = SleepTimer::new(now_mono_ns, *minutes) {
                    self.sleeping.push(Sleeping {
                        target: target.clone(),
                        timer,
                        fade: None,
                    });
                    self.log(format!(
                        "schedule sleep target={} minutes={} set",
                        target, minutes
                    ));
                }
            }
            _ => {}
        }
        let touched = self.rooms_touched(command, zones);
        if !touched.is_empty() {
            self.touched(
                &touched,
                is_volume_change(command),
                is_level_change(command),
                now_mono_ns,
                zones,
            );
        }
        self.finish(now_mono_ns, zones)
    }

    /// An input's `source_offer` said its signal is present (`true`) or not.
    /// The runtime offers and withdraws the input in the zones (the state's
    /// `inputs` lists the ones with a signal), and starts or holds autoplay.
    pub fn on_input_signal(
        &mut self,
        input: &InputId,
        signal: bool,
        now_mono_ns: u64,
        zones: &mut Zones,
    ) -> Vec<Effect> {
        if !self.connected.contains(input) {
            self.connected.push(input.clone());
        }
        if signal {
            zones.offer_input(input.clone());
            if let Some(p) = self.playing.iter_mut().find(|p| p.input == *input) {
                if p.off_since.take().is_some() {
                    let line = format!(
                        "schedule autoplay input={} signal back within the hold; stop cancelled",
                        input.literal()
                    );
                    self.log(line);
                }
            } else if let Some(target) = self.autoplay_target(input, zones) {
                self.start_autoplay(input, &target, zones);
            }
        } else {
            zones.withdraw_input(input);
            let mut held = false;
            if let Some(p) = self.playing.iter_mut().find(|p| p.input == *input) {
                if p.off_since.is_none() {
                    p.off_since = Some(now_mono_ns);
                    held = true;
                }
            }
            if held {
                self.log(format!(
                    "schedule autoplay input={} signal gone; holding {} ms",
                    input.literal(),
                    AUTOPLAY_HOLD_MS
                ));
            }
        }
        self.finish(now_mono_ns, zones)
    }

    /// (goal 13) A TV input's signal ended because the TV went to standby
    /// (`InputEvent::Standby`; only an `optical` or `hdmi_arc` input's offer
    /// is ever one). The input is withdrawn as for any signal gone; an
    /// autoplay playing it whose rule says `stop_on_standby` stops NOW and
    /// restores its rooms (no hold: the TV is off, nobody is watching), and
    /// one whose rule says otherwise holds as a signal gone does.
    pub fn on_input_standby(
        &mut self,
        input: &InputId,
        now_mono_ns: u64,
        zones: &mut Zones,
    ) -> Vec<Effect> {
        let stop = zones
            .autoplay_rules()
            .iter()
            .find(|r| r.input == *input)
            .is_none_or(|r| r.stop_on_standby);
        if !stop || !self.playing.iter().any(|p| p.input == *input) {
            return self.on_input_signal(input, false, now_mono_ns, zones);
        }
        if !self.connected.contains(input) {
            self.connected.push(input.clone());
        }
        zones.withdraw_input(input);
        self.end_autoplay(input, "standby", zones);
        self.finish(now_mono_ns, zones)
    }

    /// An input's endpoint is gone (its session ended or it sent
    /// `stream_end`). Nothing is sent to it any more; whatever played it
    /// stops now: an autoplay is restored without a hold, an alarm falls back
    /// to the chime (an alarm must still wake), and any other group playing it
    /// goes silent.
    pub fn on_input_gone(
        &mut self,
        input: &InputId,
        now_mono_ns: u64,
        zones: &mut Zones,
    ) -> Vec<Effect> {
        self.connected.retain(|i| i != input);
        self.started.retain(|i| i != input);
        zones.withdraw_input(input);
        if self.playing.iter().any(|p| p.input == *input) {
            self.end_autoplay(input, "input-gone", zones);
        }
        let line_in = Source::LineIn(input.clone());
        let fallback = Source::Chime(FALLBACK_CHIME.name().to_string());
        for i in 0..self.ringing.len() {
            if self.ringing[i].source == line_in {
                let group = self.ringing[i].group.clone();
                if zones.source(&group) == line_in {
                    let _ = zones.set_group_source(&group, fallback.clone());
                }
                self.ringing[i].source = fallback.clone();
                let line = format!(
                    "schedule alarm={} fallback=chime reason=input-gone input={}",
                    self.ringing[i].id,
                    input.literal()
                );
                self.log(line);
            }
        }
        for g in zones.formed_groups() {
            if zones.source(&g.id) == line_in {
                let _ = zones.set_group_source(&g.id, Source::None);
                self.log(format!(
                    "schedule group={} source=none reason=input-gone input={}",
                    g.id,
                    input.literal()
                ));
            }
        }
        self.finish(now_mono_ns, zones)
    }

    // --- quiet hours -----------------------------------------------------

    /// Feed the civil time to the model, which decides which windows are
    /// active and pulls volumes down; log the rooms whose limit moved.
    fn quiet_hours(&mut self, now_utc_s: i64, zones: &mut Zones) {
        let civil = self.tz.to_civil(now_utc_s);
        let minutes = u16::from(civil.hour) * 60 + u16::from(civil.minute);
        let Some(time) = ClockTime::from_minutes(minutes) else {
            return;
        };
        let now = CivilTime {
            weekday: civil.weekday.index(),
            time,
        };
        let before: Vec<(String, Volume)> = zones
            .zones()
            .iter()
            .map(|z| (z.id.clone(), z.effective_limit()))
            .collect();
        if zones.set_civil_time(Some(now)) {
            for (id, was) in before {
                let Some(z) = zones.zone(&id) else { continue };
                let limit = z.effective_limit();
                if limit != was {
                    let line = format!(
                        "schedule quiet-hours zone={} effective_limit={} was={} volume={}",
                        id,
                        limit.literal(),
                        was.literal(),
                        z.volume.literal()
                    );
                    self.log(line);
                }
            }
        }
    }

    // --- ramps -----------------------------------------------------------

    fn step_ramps(&mut self, now: u64, zones: &mut Zones) {
        let due: Vec<String> = self
            .ramps
            .iter()
            .filter(|(_, r)| r.next_ns <= now)
            .map(|(z, _)| z.clone())
            .collect();
        for zone in due {
            self.step_ramp(&zone, now, zones);
        }
    }

    /// One segment of a room's ramp: set the volume the plan reaches at the
    /// end of the segment (the endpoint ramps to it over the segment), held
    /// under the model's target for a rise.
    fn step_ramp(&mut self, zone: &str, now: u64, zones: &mut Zones) {
        let Some(r) = self.ramps.get(zone).copied() else {
            return;
        };
        let elapsed = now.saturating_sub(r.start_ns);
        let ahead = elapsed
            .saturating_add(STEP_MS * NS_PER_MS)
            .min(r.ramp.duration_ns);
        let mut value = r.ramp.sample(ahead);
        if r.rise {
            if let Some(target) = zones.zone(zone).and_then(|z| z.ramp) {
                value = value.min(th(target));
            }
        }
        let _ = zones.runtime_volume(zone, vol(value));
        self.hint
            .insert(zone.to_string(), ms_u16(ahead.saturating_sub(elapsed)));
        if ahead >= r.ramp.duration_ns {
            self.ramps.remove(zone);
            if r.rise {
                let _ = zones.stop_ramp(zone);
            }
            self.persist = true;
        } else if let Some(r) = self.ramps.get_mut(zone) {
            r.next_ns = now + STEP_MS * NS_PER_MS;
        }
    }

    /// Stop a room's ramp where it is.
    fn drop_ramp(&mut self, zone: &str, zones: &mut Zones) {
        if let Some(r) = self.ramps.remove(zone) {
            if r.rise {
                let _ = zones.stop_ramp(zone);
            }
        }
    }

    /// Fade a room from its volume now to 0 over `duration_ns`, the first
    /// segment at once. Returns whether there was anything to fade.
    fn start_fade(
        &mut self,
        zone: &str,
        from: Volume,
        start_ns: u64,
        duration_ns: u64,
        now: u64,
        zones: &mut Zones,
    ) -> bool {
        self.drop_ramp(zone, zones);
        if from == Volume::SILENT {
            return false;
        }
        self.ramps.insert(
            zone.to_string(),
            Stepped {
                ramp: Ramp {
                    from: th(from),
                    to: 0,
                    duration_ns,
                },
                start_ns,
                next_ns: now,
                rise: false,
            },
        );
        self.step_ramp(zone, now, zones);
        true
    }

    // --- holds and restore -----------------------------------------------

    /// The rooms a target names now: a room, a saved group's rooms, or a
    /// formed group's.
    fn target_rooms(target: &str, zones: &Zones) -> Vec<String> {
        if zones.zone(target).is_some() {
            return vec![target.to_string()];
        }
        if let Some(saved) = zones.saved_groups().iter().find(|g| g.id == target) {
            return saved
                .zones
                .iter()
                .filter(|z| zones.zone(z).is_some())
                .cloned()
                .collect();
        }
        members(target, zones)
    }

    fn snapshot_of(zone: &str, zones: &Zones) -> Option<Snapshot> {
        let z = zones.zone(zone)?;
        Some(Snapshot {
            group: z.group.clone(),
            volume: z.volume,
            muted: z.muted,
            source: zones.source(&z.group),
            peers: members(&z.group, zones)
                .into_iter()
                .filter(|m| m != zone)
                .collect(),
        })
    }

    /// Hold `rooms` for `owner`, snapshotting each (or inheriting the
    /// snapshot of whoever held it before).
    fn hold(&mut self, owner: &Owner, rooms: &[String], zones: &mut Zones) {
        for room in rooms {
            let snapshot = match self.holds.remove(room) {
                Some(h) => h.snapshot,
                None => match Runtime::snapshot_of(room, zones) {
                    Some(s) => s,
                    None => continue,
                },
            };
            self.drop_ramp(room, zones);
            self.holds.insert(
                room.clone(),
                Hold {
                    owner: owner.clone(),
                    snapshot,
                },
            );
        }
    }

    /// Release every room `owner` holds, in configured order.
    fn release(&mut self, owner: &Owner, zones: &mut Zones) -> Vec<(String, Snapshot)> {
        let mut out = Vec::new();
        for z in zones.zones() {
            if self.holds.get(&z.id).is_some_and(|h| h.owner == *owner) {
                if let Some(h) = self.holds.remove(&z.id) {
                    out.push((z.id.clone(), h.snapshot));
                }
            }
        }
        for (room, _) in &out {
            self.drop_ramp(room, zones);
        }
        out
    }

    /// Put rooms back as their snapshots say: group, volume (clamped, like
    /// every volume), mute, and the source of a group made only of them.
    fn restore(&mut self, rooms: Vec<(String, Snapshot)>, zones: &mut Zones) {
        let mut pending: Vec<String> = rooms.iter().map(|(r, _)| r.clone()).collect();
        for (room, snap) in &rooms {
            pending.retain(|p| p != room);
            let Some(current) = zones.zone(room).map(|z| z.group.clone()) else {
                continue;
            };
            if current != snap.group {
                let saved = zones.saved_groups().iter().any(|g| g.id == snap.group);
                let command = if snap.group == *room {
                    Command::Ungroup { zone: room.clone() }
                } else if saved || !members(&snap.group, zones).is_empty() {
                    Command::Join {
                        zone: room.clone(),
                        target: snap.group.clone(),
                    }
                } else if let Some(peer) = snap
                    .peers
                    .iter()
                    .find(|p| !pending.contains(p) && zones.zone(p).is_some())
                {
                    // The live group it was in dissolved while it was away:
                    // joining a room that was in it forms it again (under a
                    // new id, the only kind a live group has).
                    Command::Join {
                        zone: room.clone(),
                        target: peer.clone(),
                    }
                } else {
                    Command::Ungroup { zone: room.clone() }
                };
                self.apply(&command, zones);
            }
            let _ = zones.runtime_volume(room, snap.volume);
            if zones.zone(room).is_some_and(|z| z.muted != snap.muted) {
                self.apply(
                    &Command::Mute {
                        zone: room.clone(),
                        muted: snap.muted,
                    },
                    zones,
                );
            }
        }
        let restored: Vec<&String> = rooms.iter().map(|(r, _)| r).collect();
        for (room, snap) in &rooms {
            let Some(group) = zones.zone(room).map(|z| z.group.clone()) else {
                continue;
            };
            let all_ours = members(&group, zones).iter().all(|m| restored.contains(&m));
            if all_ours
                && zones.source(&group) != snap.source
                && zones.set_group_source(&group, snap.source.clone()).is_err()
            {
                // (goal 16) The one source that can be refused here: a
                // player another group took while this one was away. The
                // group is not left playing what the runtime gave it (an
                // alarm's chime would ring on): it plays nothing.
                let _ = zones.set_group_source(&group, Source::None);
                self.log(format!(
                    "schedule restore group={} source=none reason=player-busy wanted={}",
                    group,
                    snap.source.literal()
                ));
            }
        }
        self.persist = true;
    }

    /// Apply a command of the runtime's own, logging a refusal (which would
    /// be a defect here: every command is built from the model's own state).
    fn apply(&mut self, command: &Command, zones: &mut Zones) -> bool {
        match zones.apply(command) {
            Ok(()) => true,
            Err(refusal) => {
                self.log(format!(
                    "schedule internal command={} refused: {}",
                    command.type_name(),
                    refusal
                ));
                false
            }
        }
    }

    // --- alarms ----------------------------------------------------------

    fn poll_alarms(&mut self, now: u64, utc: i64, zones: &mut Zones) {
        let Some(t0) = self.last_utc else {
            // The first tick rings nothing missed while the server was down.
            self.last_utc = Some(utc);
            return;
        };
        if utc < t0 {
            self.log(format!(
                "schedule civil clock stepped back by {} s; alarms that rang keep their last ring",
                t0 - utc
            ));
            self.last_utc = Some(utc);
            return;
        }
        let schedules: Vec<(String, AlarmSchedule)> = zones
            .alarms()
            .iter()
            .map(|a| (a.id.clone(), schedule_of(a)))
            .collect();
        for due in due_between(&schedules, &self.tz, t0, utc) {
            if self.last_fired.get(&due.key).is_some_and(|&l| due.at <= l) {
                continue;
            }
            self.last_fired.insert(due.key.clone(), due.at);
            let Some(config) = zones.alarms().iter().find(|a| a.id == due.key).cloned() else {
                continue;
            };
            if config.days.is_empty() {
                // A one-shot alarm has had its chance, rung or skipped.
                let mut off = config.clone();
                off.enabled = false;
                self.apply(&Command::AlarmSet(off), zones);
                self.persist = true;
            }
            if utc - due.at > LATE_FIRE_S {
                self.log(format!(
                    "schedule alarm={} skipped late due={} now={} late_s={}",
                    config.id,
                    due.at,
                    utc,
                    utc - due.at
                ));
                continue;
            }
            if self.ringing.iter().any(|r| r.id == config.id) {
                self.log(format!(
                    "schedule alarm={} due while ringing; not refired",
                    config.id
                ));
                continue;
            }
            self.fire(&config, due.at, now, zones);
        }
        self.last_utc = Some(utc);
    }

    /// What the alarm will actually play, or why not its own source.
    fn alarm_plan(&self, config: &AlarmConfig, zones: &Zones) -> Plan {
        match &config.source {
            Source::Chime(name) if Chime::from_name(name).is_some() => {
                Plan::Plays(config.source.clone())
            }
            Source::Chime(_) => Plan::Fallback("unknown-chime"),
            // (goal 17) An input plays in any number of groups: one another
            // group already plays is played here too.
            Source::LineIn(input) if zones.inputs().contains(input) => {
                Plan::Plays(config.source.clone())
            }
            Source::LineIn(_) => Plan::Fallback("not-offered"),
            Source::Stream => Plan::Plays(Source::Stream),
            Source::None => Plan::Fallback("source-none"),
            // (goal 16) What a player plays is its control point's to say;
            // an alarm has no media to hand it, so it rings the bell.
            Source::Player(_) => Plan::Fallback("player-source"),
            // (goal 17) Nor can an alarm name a receiver: what it plays is
            // the Spotify app's to say. An alarm's Spotify source is a
            // stored URI, below. (The catalog refuses this spelling in an
            // alarm; the arm is for a model built without it.)
            Source::Soloist(_) => Plan::Fallback("receiver-source"),
            // (goal 17) A stored source: a stream URL is played by a player
            // the conductor starts; a Spotify URI by a Soloist receiver, and
            // only when the server was told to (off by default, P7).
            Source::Stored(id) => match zones.stored_source(id) {
                None => Plan::Fallback("not-stored"),
                Some(stored) => match stored.kind {
                    StoredKind::Url => Plan::Stored(stored.clone()),
                    StoredKind::Spotify if self.soloist_alarms => Plan::Stored(stored.clone()),
                    StoredKind::Spotify => Plan::Fallback("soloist-off"),
                },
            },
        }
    }

    /// (goal 17) Where an input with a signal autoplays: its rule's target
    /// when it has an enabled rule; nowhere when its rule is disabled; and
    /// with no rule at all, a `streamer` plays into its endpoint's own room
    /// (the role's default), any other input nowhere.
    fn autoplay_target(&mut self, input: &InputId, zones: &Zones) -> Option<String> {
        if let Some(rule) = zones.autoplay_rules().iter().find(|r| r.input == *input) {
            return rule.enabled.then(|| rule.target.clone());
        }
        let label = zones.input_label(input)?;
        if label.role != InputRole::Streamer {
            return None;
        }
        match zones.room_of_endpoint(&input.endpoint) {
            Some(room) => Some(room.to_string()),
            None => {
                self.log(format!(
                    "schedule autoplay input={} role=streamer: its endpoint is in no room; \
                     not played",
                    input.literal()
                ));
                None
            }
        }
    }

    fn fire(&mut self, config: &AlarmConfig, at: i64, now: u64, zones: &mut Zones) {
        let rooms = Runtime::target_rooms(&config.target, zones);
        if rooms.is_empty() {
            self.log(format!(
                "schedule alarm={} target={} names no room now; not fired",
                config.id, config.target
            ));
            return;
        }
        let chime = Source::Chime(FALLBACK_CHIME.name().to_string());
        let (source, stored) = match self.alarm_plan(config, zones) {
            Plan::Plays(source) => (source, None),
            Plan::Fallback(reason) => {
                self.log(format!(
                    "schedule alarm={} fallback=chime reason={} wanted={} plays={}",
                    config.id,
                    reason,
                    config.source.literal(),
                    chime.literal()
                ));
                (chime, None)
            }
            // Silence until the conductor says the player (or the receiver)
            // plays; the ramp runs meanwhile.
            Plan::Stored(stored) => (Source::None, Some(stored)),
        };
        let owner = Owner::Alarm(config.id.clone());
        self.hold(&owner, &rooms, zones);
        let take = Command::Take {
            target: config.target.clone(),
            source: Some(source.clone()),
        };
        if !self.apply(&take, zones) {
            self.release(&owner, zones);
            return;
        }
        for room in &rooms {
            let _ = zones.runtime_volume(room, Volume::SILENT);
            if zones.zone(room).is_some_and(|z| z.muted) {
                self.apply(
                    &Command::Mute {
                        zone: room.clone(),
                        muted: false,
                    },
                    zones,
                );
            }
            if config.ramp_s == 0 {
                let _ = zones.runtime_volume(room, config.volume);
                continue;
            }
            let Ok(target) = zones.start_ramp(room, config.volume) else {
                continue;
            };
            // The first segment is the next tick's: this call's report is the
            // silence the source starts in.
            self.ramps.insert(
                room.clone(),
                Stepped {
                    ramp: Ramp::new(0, th(target), u64::from(config.ramp_s) * 1000),
                    start_ns: now,
                    next_ns: now,
                    rise: true,
                },
            );
        }
        let _ = zones.set_alarm_ringing(&config.id, true);
        self.ringing.push(Ringing {
            id: config.id.clone(),
            group: config.target.clone(),
            source: source.clone(),
            end_ns: (config.duration_min > 0)
                .then(|| now + u64::from(config.duration_min) * 60 * NS_PER_S),
            ending_until: None,
            stored: stored.as_ref().map(|s| s.id.clone()),
            waiting: stored.is_some(),
        });
        self.persist = true;
        self.log(format!(
            "schedule alarm={} fired at={} rooms={} source={} volume={} ramp_s={}",
            config.id,
            at,
            rooms.join(","),
            if stored.is_some() {
                config.source.literal()
            } else {
                source.literal()
            },
            config.volume.literal(),
            config.ramp_s
        ));
        if let Some(stored) = stored {
            let (alarm, target) = (config.id.clone(), config.target.clone());
            self.logs.push(match stored.kind {
                StoredKind::Url => Effect::PlayStored {
                    alarm,
                    target,
                    stored: stored.id,
                    url: stored.value,
                    name: stored.name,
                },
                StoredKind::Spotify => Effect::PlaySpotify {
                    alarm,
                    target,
                    stored: stored.id,
                    uri: stored.value,
                    name: stored.name,
                },
            });
        }
    }

    fn step_alarms(&mut self, now: u64, zones: &mut Zones) {
        let ids: Vec<(String, bool)> = self
            .ringing
            .iter()
            .filter_map(|r| match (r.ending_until, r.end_ns) {
                (Some(u), _) if now >= u => Some((r.id.clone(), true)),
                (None, Some(e)) if now >= e => Some((r.id.clone(), false)),
                _ => None,
            })
            .collect();
        for (id, ended) in ids {
            if ended {
                self.finish_alarm(&id, zones);
            } else {
                self.end_alarm(&id, &[], now, "duration", zones);
            }
        }
    }

    /// Begin an alarm's end: rooms in `detached` are let go as they are (a
    /// person touched them); every other room it holds fades to 0 over
    /// [`END_FADE_MS`], and then [`Runtime::finish_alarm`] restores them.
    fn end_alarm(
        &mut self,
        id: &str,
        detached: &[String],
        now: u64,
        reason: &str,
        zones: &mut Zones,
    ) {
        let Some(at) = self.ringing.iter().position(|r| r.id == id) else {
            return;
        };
        if self.ringing[at].ending_until.is_some() {
            return;
        }
        for room in detached {
            if self
                .holds
                .get(room)
                .is_some_and(|h| h.owner == Owner::Alarm(id.to_string()))
            {
                self.holds.remove(room);
                self.drop_ramp(room, zones);
            }
        }
        let _ = zones.set_alarm_ringing(id, false);
        let owner = Owner::Alarm(id.to_string());
        let held: Vec<String> = self
            .holds
            .iter()
            .filter(|(_, h)| h.owner == owner)
            .map(|(z, _)| z.clone())
            .collect();
        let mut fading = false;
        for room in &held {
            let Some(from) = zones.zone(room).map(|z| z.volume) else {
                continue;
            };
            fading |= self.start_fade(room, from, now, END_FADE_MS * NS_PER_MS, now, zones);
        }
        self.log(format!(
            "schedule alarm={} ending reason={} fading={} detached={}",
            id,
            reason,
            list(&held),
            list(detached)
        ));
        if fading {
            self.ringing[at].ending_until = Some(now + END_FADE_MS * NS_PER_MS);
        } else {
            self.finish_alarm(id, zones);
        }
    }

    /// The end fade is over: silence what the alarm played and restore the
    /// rooms it still holds.
    fn finish_alarm(&mut self, id: &str, zones: &mut Zones) {
        let Some(at) = self.ringing.iter().position(|r| r.id == id) else {
            return;
        };
        let ring = self.ringing.remove(at);
        let _ = zones.set_alarm_ringing(id, false);
        if !members(&ring.group, zones).is_empty() && zones.source(&ring.group) == ring.source {
            let _ = zones.set_group_source(&ring.group, Source::None);
        }
        if ring.stored.is_some() {
            self.logs.push(Effect::StopStored {
                alarm: ring.id.clone(),
            });
        }
        let rooms = self.release(&Owner::Alarm(ring.id.clone()), zones);
        let names: Vec<String> = rooms.iter().map(|(r, _)| r.clone()).collect();
        self.restore(rooms, zones);
        self.log(format!(
            "schedule alarm={} ended restored={}",
            ring.id,
            list(&names)
        ));
    }

    // --- sleep -----------------------------------------------------------

    fn step_sleeps(&mut self, now: u64, zones: &mut Zones) {
        // A timer the model no longer has (its group dissolved, it was
        // cancelled where this runtime was not told) ends here, and one it
        // has that this runtime was not told of starts now.
        let gone: Vec<String> = self
            .sleeping
            .iter()
            .filter(|s| !zones.sleep_timers().iter().any(|t| t.target == s.target))
            .map(|s| s.target.clone())
            .collect();
        for target in gone {
            self.cancel_sleep(&target, &[], "removed", zones);
        }
        for t in zones.sleep_timers().to_vec() {
            if !self.sleeping.iter().any(|s| s.target == t.target) {
                if let Some(timer) = SleepTimer::new(now, t.minutes) {
                    self.sleeping.push(Sleeping {
                        target: t.target.clone(),
                        timer,
                        fade: None,
                    });
                }
            }
        }
        for i in 0..self.sleeping.len() {
            let s = self.sleeping[i].clone();
            if s.fade.is_none() && now >= s.timer.fade_start_ns() {
                let rooms: Vec<(String, Volume)> = Runtime::target_rooms(&s.target, zones)
                    .into_iter()
                    .filter_map(|r| zones.zone(&r).map(|z| (r.clone(), z.volume)))
                    .collect();
                let duration = s.timer.expiry_ns() - s.timer.fade_start_ns();
                for (room, from) in &rooms {
                    self.start_fade(room, *from, s.timer.fade_start_ns(), duration, now, zones);
                }
                self.log(format!(
                    "schedule sleep target={} fading rooms={}",
                    s.target,
                    list(&rooms.iter().map(|(r, _)| r.clone()).collect::<Vec<_>>())
                ));
                self.sleeping[i].fade = Some(rooms);
            }
        }
        // (ADR 0194) What is left of each, in whole seconds rounded up, for
        // the state's `remaining_s`: the model keeps the count every pass
        // and sends a state when the whole minutes left change.
        for s in &self.sleeping {
            let left_ns = s.timer.expiry_ns().saturating_sub(now);
            let left_s = left_ns.div_ceil(NS_PER_MS * 1000);
            zones.sleep_remaining(&s.target, u32::try_from(left_s).unwrap_or(u32::MAX));
        }
        let expired: Vec<String> = self
            .sleeping
            .iter()
            .filter(|s| now >= s.timer.expiry_ns())
            .map(|s| s.target.clone())
            .collect();
        for target in expired {
            self.stop_sleep(&target, zones);
        }
    }

    /// The timer ran out: silence the target's groups and put each room's
    /// volume back to where the fade began, so the next thing played starts
    /// where the person left it.
    fn stop_sleep(&mut self, target: &str, zones: &mut Zones) {
        let Some(at) = self.sleeping.iter().position(|s| s.target == target) else {
            return;
        };
        let s = self.sleeping.remove(at);
        let rooms = s.fade.unwrap_or_default();
        for (room, _) in &rooms {
            self.drop_ramp(room, zones);
            if let Some(group) = zones.zone(room).map(|z| z.group.clone()) {
                if zones.source(&group) != Source::None {
                    let _ = zones.set_group_source(&group, Source::None);
                }
            }
        }
        for (room, volume) in &rooms {
            let _ = zones.runtime_volume(room, *volume);
        }
        zones.sleep_expired(target);
        self.persist = true;
        self.log(format!(
            "schedule sleep target={} expired; stopped, volume restored rooms={}",
            target,
            list(&rooms.iter().map(|(r, _)| r.clone()).collect::<Vec<_>>())
        ));
    }

    /// Cancel a sleep timer: a fade under way is undone for every room but
    /// those in `touched` (a person just set them), over one segment.
    fn cancel_sleep(&mut self, target: &str, touched: &[String], reason: &str, zones: &mut Zones) {
        let Some(at) = self.sleeping.iter().position(|s| s.target == target) else {
            return;
        };
        let s = self.sleeping.remove(at);
        for (room, volume) in s.fade.unwrap_or_default() {
            self.drop_ramp(&room, zones);
            if !touched.contains(&room) {
                let _ = zones.runtime_volume(&room, volume);
                self.hint.insert(room, STEP_MS as u16);
            }
        }
        if reason != "replaced" {
            zones.sleep_expired(target);
        }
        self.log(format!(
            "schedule sleep target={} cancelled reason={}",
            target, reason
        ));
    }

    // --- autoplay --------------------------------------------------------

    fn start_autoplay(&mut self, input: &InputId, target: &str, zones: &mut Zones) {
        let line_in = Source::LineIn(input.clone());
        let rooms = Runtime::target_rooms(target, zones);
        if rooms.is_empty() {
            return;
        }
        // (goal 17) An input plays in any number of groups, so another
        // group playing it is no reason to hold back; only a target that
        // already plays it, exactly as the take would leave it, is left as
        // the person who chose it made it (nothing to hold, nothing to
        // restore).
        if members(target, zones) == rooms && zones.source(target) == line_in {
            self.log(format!(
                "schedule autoplay input={} already plays in its target={}; not taken",
                input.literal(),
                target
            ));
            return;
        }
        let owner = Owner::Autoplay(input.clone());
        self.hold(&owner, &rooms, zones);
        let take = Command::Take {
            target: target.to_string(),
            source: Some(line_in),
        };
        if !self.apply(&take, zones) {
            self.release(&owner, zones);
            return;
        }
        self.playing.push(Playing {
            input: input.clone(),
            group: target.to_string(),
            off_since: None,
        });
        self.persist = true;
        self.log(format!(
            "schedule autoplay input={} took target={} rooms={}",
            input.literal(),
            target,
            rooms.join(",")
        ));
    }

    fn step_autoplay(&mut self, now: u64, zones: &mut Zones) {
        let over: Vec<InputId> = self
            .playing
            .iter()
            .filter(|p| {
                p.off_since
                    .is_some_and(|t| now >= t + AUTOPLAY_HOLD_MS * NS_PER_MS)
            })
            .map(|p| p.input.clone())
            .collect();
        for input in over {
            self.end_autoplay(&input, "hold-over", zones);
        }
    }

    /// Stop an autoplay: its group goes silent and the rooms it still holds
    /// are restored. The input's stop follows from no group playing it.
    fn end_autoplay(&mut self, input: &InputId, reason: &str, zones: &mut Zones) {
        let Some(at) = self.playing.iter().position(|p| p.input == *input) else {
            return;
        };
        let p = self.playing.remove(at);
        let line_in = Source::LineIn(input.clone());
        if !members(&p.group, zones).is_empty() && zones.source(&p.group) == line_in {
            let _ = zones.set_group_source(&p.group, Source::None);
        }
        let rooms = self.release(&Owner::Autoplay(input.clone()), zones);
        let names: Vec<String> = rooms.iter().map(|(r, _)| r.clone()).collect();
        self.restore(rooms, zones);
        self.log(format!(
            "schedule autoplay input={} stopped reason={} restored={}",
            input.literal(),
            reason,
            list(&names)
        ));
    }

    // --- a person's commands ---------------------------------------------

    /// The rooms a person's command acts on, for detaching. Settings that
    /// the clamp already honours (`limit`, `quiet_hours`), names, bonds, an
    /// endpoint's `attach` and the schedule's own commands touch none.
    fn rooms_touched(&self, command: &Command, zones: &Zones) -> Vec<String> {
        match command {
            Command::Volume { zone, .. }
            | Command::VolumeStep { zone, .. }
            | Command::Mute { zone, .. }
            | Command::Group { zone, .. }
            | Command::Ungroup { zone } => vec![zone.clone()],
            Command::Join { zone, target } => {
                let mut rooms = vec![zone.clone()];
                if zones.zone(target).is_some() && target != zone {
                    rooms.push(target.clone());
                }
                rooms
            }
            Command::Take { target, .. } => Runtime::target_rooms(target, zones),
            Command::GroupVolume { group, .. } | Command::GroupVolumeStep { group, .. } => {
                members(group, zones)
            }
            _ => Vec::new(),
        }
    }

    /// A person touched `rooms`: a ringing alarm holding one ends (that room
    /// detached, the rest faded and restored), an autoplay lets the room go
    /// unless the command only set a level (`level`: a volume or a mute,
    /// which is not a source change; goal 13), and a volume change cancels a
    /// sleep fade covering one.
    ///
    /// Why a level keeps an autoplay but ends an alarm (ADR 0076's rule is
    /// kept as it was): an alarm is the runtime deciding the room's volume
    /// (its ramp), so a person setting one takes the room back from it; an
    /// autoplay decides only what the room plays, so a person who turns the
    /// TV down (its remote's keys reach the server as `volume_step` and
    /// `mute_set` from the hub, ADR 0087) or the turntable up is still
    /// listening to it, and the TV's standby (or the input's hold) must
    /// still stop it and restore the room. The restore puts back the volume
    /// and mute the room had before the autoplay took it, as for an alarm.
    fn touched(
        &mut self,
        rooms: &[String],
        volume: bool,
        level: bool,
        now: u64,
        zones: &mut Zones,
    ) {
        let mut alarms: Vec<String> = Vec::new();
        for room in rooms {
            match self.holds.get(room).map(|h| h.owner.clone()) {
                Some(Owner::Alarm(id)) => {
                    let ending = self
                        .ringing
                        .iter()
                        .any(|r| r.id == id && r.ending_until.is_some());
                    if ending {
                        self.holds.remove(room);
                        self.drop_ramp(room, zones);
                    } else if !alarms.contains(&id) {
                        alarms.push(id);
                    }
                }
                Some(Owner::Autoplay(_)) if level => {}
                Some(Owner::Autoplay(input)) => {
                    self.holds.remove(room);
                    self.log(format!(
                        "schedule autoplay input={} zone={} detached by a person's command",
                        input.literal(),
                        room
                    ));
                }
                None => {}
            }
        }
        for id in alarms {
            self.end_alarm(&id, rooms, now, "person", zones);
        }
        if volume {
            let targets: Vec<String> = self
                .sleeping
                .iter()
                .filter(|s| {
                    s.fade
                        .as_ref()
                        .is_some_and(|f| f.iter().any(|(r, _)| rooms.contains(r)))
                })
                .map(|s| s.target.clone())
                .collect();
            for target in targets {
                self.cancel_sleep(&target, rooms, "volume-changed", zones);
            }
        }
    }

    // --- reporting -------------------------------------------------------

    fn log(&mut self, line: String) {
        self.logs.push(Effect::Log(line));
    }

    /// Drop activities nothing plays any more: an alarm whose every room
    /// another activity took, an autoplay whose line-in no group plays (a
    /// person chose something else, or an alarm took its rooms).
    fn reap(&mut self, zones: &mut Zones) {
        let orphaned: Vec<String> = self
            .ringing
            .iter()
            .filter(|r| r.ending_until.is_none())
            .filter(|r| {
                !self
                    .holds
                    .values()
                    .any(|h| h.owner == Owner::Alarm(r.id.clone()))
            })
            .map(|r| r.id.clone())
            .collect();
        for id in orphaned {
            if self
                .ringing
                .iter()
                .any(|r| r.id == id && r.stored.is_some())
            {
                self.logs.push(Effect::StopStored { alarm: id.clone() });
            }
            self.ringing.retain(|r| r.id != id);
            let _ = zones.set_alarm_ringing(&id, false);
            self.log(format!("schedule alarm={} ended: its rooms were taken", id));
        }
        // (goal 17) Its own group, not any group: another group sharing the
        // input does not keep an autoplay whose target stopped playing it.
        let silent: Vec<InputId> = self
            .playing
            .iter()
            .filter(|p| {
                members(&p.group, zones).is_empty()
                    || zones.source(&p.group) != Source::LineIn(p.input.clone())
            })
            .map(|p| p.input.clone())
            .collect();
        for input in silent {
            self.playing.retain(|p| p.input != input);
            let owner = Owner::Autoplay(input.clone());
            self.holds.retain(|_, h| h.owner != owner);
            self.log(format!(
                "schedule autoplay input={} ended: its target no longer plays it",
                input.literal()
            ));
        }
    }

    /// Report what changed: room volumes quieter first, then sources, then
    /// input starts and stops, then louder room volumes, then persistence.
    fn finish(&mut self, now: u64, zones: &mut Zones) -> Vec<Effect> {
        self.reap(zones);
        let mut out = std::mem::take(&mut self.logs);
        let mut louder = Vec::new();
        for z in zones.zones() {
            let gain = th(z.gain());
            let limit = th(z.effective_limit());
            let mut ramp_ms = self.hint.remove(&z.id).unwrap_or(0);
            let Some(prev) = self.sent.get(&z.id).copied() else {
                self.sent.insert(z.id.clone(), Sent { gain, limit });
                out.push(Effect::RoomVolume {
                    zone: z.id.clone(),
                    gain,
                    limit,
                    ramp_ms: 0,
                });
                continue;
            };
            let mut send_limit = limit;
            if limit < prev.limit {
                match self.pending_limit.get(&z.id).copied() {
                    Some(at) if now < at => send_limit = prev.limit,
                    Some(_) => {
                        self.pending_limit.remove(&z.id);
                    }
                    None if gain < prev.gain => {
                        // Ramp the gain down first; the lower limit follows.
                        self.pending_limit
                            .insert(z.id.clone(), now + LIMIT_PULL_MS * NS_PER_MS);
                        send_limit = prev.limit;
                        ramp_ms = ramp_ms.max(LIMIT_PULL_MS as u16);
                    }
                    None => {}
                }
            } else {
                self.pending_limit.remove(&z.id);
            }
            let next = Sent {
                gain,
                limit: send_limit,
            };
            if next == prev {
                continue;
            }
            self.sent.insert(z.id.clone(), next);
            let effect = Effect::RoomVolume {
                zone: z.id.clone(),
                gain,
                limit: send_limit,
                ramp_ms,
            };
            if gain < prev.gain {
                out.push(effect);
            } else {
                louder.push(effect);
            }
        }
        let mut now_sources: BTreeMap<String, Source> = BTreeMap::new();
        for g in zones.formed_groups() {
            let source = zones.source(&g.id);
            if self.sources.get(&g.id) != Some(&source) {
                out.push(Effect::SetSource {
                    group: g.id.clone(),
                    source: source.clone(),
                });
            }
            now_sources.insert(g.id, source);
        }
        self.sources = now_sources;
        let playing: Vec<InputId> = self
            .sources
            .values()
            .filter_map(|s| match s {
                Source::LineIn(i) => Some(i.clone()),
                _ => None,
            })
            .collect();
        for input in self.started.clone() {
            if !playing.contains(&input) {
                self.started.retain(|i| *i != input);
                out.push(Effect::SourceControl {
                    input,
                    action: InputAction::Stop,
                });
            }
        }
        for input in playing {
            let reachable = self.connected.contains(&input) || zones.inputs().contains(&input);
            if reachable && !self.started.contains(&input) {
                self.started.push(input.clone());
                out.push(Effect::SourceControl {
                    input,
                    action: InputAction::Start,
                });
            }
        }
        out.extend(louder);
        if std::mem::take(&mut self.persist) {
            out.push(Effect::Persist);
        }
        out
    }
}

/// The rooms of a formed group, in configured order.
fn members(group: &str, zones: &Zones) -> Vec<String> {
    zones
        .zones()
        .iter()
        .filter(|z| z.group == group)
        .map(|z| z.id.clone())
        .collect()
}

fn list(rooms: &[String]) -> String {
    if rooms.is_empty() {
        "none".to_string()
    } else {
        rooms.join(",")
    }
}

/// Whether a command is a person changing a volume: what cancels a sleep
/// fade.
fn is_volume_change(command: &Command) -> bool {
    matches!(
        command,
        Command::Volume { .. }
            | Command::VolumeStep { .. }
            | Command::GroupVolume { .. }
            | Command::GroupVolumeStep { .. }
    )
}

/// Whether a command only sets a level (a volume or a mute): what does not
/// detach a room from an autoplay (goal 13). A sleep fade is cancelled by a
/// volume change alone ([`is_volume_change`]): a mute leaves the fade's
/// volumes as they were.
fn is_level_change(command: &Command) -> bool {
    is_volume_change(command) || matches!(command, Command::Mute { .. })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chorus_control::zones::Zone as Room;

    fn house() -> Zones {
        let mut zones = Zones::new("127.0.0.1:4010");
        for id in ["kitchen", "bedroom"] {
            let mut room = Room::new(id);
            room.volume = vol(500);
            zones.add(room).unwrap();
        }
        zones
    }

    #[test]
    fn the_first_report_is_every_room_and_every_group_at_once() {
        let mut zones = house();
        let mut rt = Runtime::new(Zone::utc());
        let out = rt.tick(0, 0, &mut zones);
        let volumes = out
            .iter()
            .filter(|e| matches!(e, Effect::RoomVolume { ramp_ms: 0, .. }))
            .count();
        let sources = out
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    Effect::SetSource {
                        source: Source::Stream,
                        ..
                    }
                )
            })
            .count();
        assert_eq!((volumes, sources), (2, 2));
        // Nothing changed: nothing is reported.
        assert!(rt.tick(1, 1, &mut zones).is_empty());
    }

    #[test]
    fn quieter_goes_before_a_source_change_and_louder_after() {
        let mut zones = house();
        let mut rt = Runtime::new(Zone::utc());
        rt.tick(0, 0, &mut zones);
        zones.runtime_volume("kitchen", vol(100)).unwrap();
        zones.runtime_volume("bedroom", vol(900)).unwrap();
        zones.set_group_source("kitchen", Source::None).unwrap();
        let out = rt.tick(1, 1, &mut zones);
        let kind = |e: &Effect| match e {
            Effect::RoomVolume { zone, .. } => format!("volume:{}", zone),
            Effect::SetSource { group, .. } => format!("source:{}", group),
            other => format!("{:?}", other),
        };
        let order: Vec<String> = out.iter().map(kind).collect();
        assert_eq!(
            order,
            ["volume:kitchen", "source:kitchen", "volume:bedroom"]
        );
    }

    #[test]
    fn a_lowered_limit_follows_the_gain_ramp_down() {
        let mut zones = house();
        let mut rt = Runtime::new(Zone::utc());
        rt.tick(0, 0, &mut zones);
        zones
            .apply(&Command::Limit {
                zone: "kitchen".into(),
                limit: vol(200),
            })
            .unwrap();
        let out = rt.on_command_applied(
            &Command::Limit {
                zone: "kitchen".into(),
                limit: vol(200),
            },
            10,
            &mut zones,
        );
        assert_eq!(
            out,
            [Effect::RoomVolume {
                zone: "kitchen".into(),
                gain: 200,
                limit: 1000,
                ramp_ms: LIMIT_PULL_MS as u16
            }]
        );
        assert_eq!(rt.next_deadline_ns(), Some(10 + LIMIT_PULL_MS * NS_PER_MS));
        assert!(
            rt.tick(11, 0, &mut zones).is_empty(),
            "held until the ramp is done"
        );
        let out = rt.tick(10 + LIMIT_PULL_MS * NS_PER_MS, 1, &mut zones);
        assert_eq!(
            out,
            [Effect::RoomVolume {
                zone: "kitchen".into(),
                gain: 200,
                limit: 200,
                ramp_ms: 0
            }]
        );
    }
}
