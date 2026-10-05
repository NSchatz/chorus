//! The conductor: every change to the room model, carried to the sessions it
//! concerns, and the schedule runtime run on the clocks.
//!
//! # What it does
//!
//! One thread, created with the rest of the population before the scheduling
//! report (`crates/server/src/main.rs`). It wakes when the control plane
//! commits a change (a command, an endpoint's button, a session coming up:
//! `ControlState::wake_conductor`), when a line-in says something
//! (`crate::linein`), when the schedule runtime has something due, and at
//! least every [`IDLE`]. Each pass:
//!
//! 0. **The players' reports** (goal 17), on a server that runs players
//!    (`--players`) and no UPnP renderers: the conductor is then the one
//!    that takes them (`PlayerSessions::on_report`, `reconcile`); with
//!    `--upnp` the renderers' manager thread does. Either way a HELD session
//!    that ended by itself (an alarm's stored stream URL that failed or ran
//!    out, `PlayerSessions::take_ended`) is handed to the schedule runtime
//!    here (`on_alarm_source_failed`), which rings the fallback chime.
//! 1. **The schedule runtime** (`crate::schedule_runtime`, ADR 0076), when
//!    this server has a control plane: every line-in event
//!    (`on_input_signal`, `on_input_gone`), every person's command applied
//!    since the last pass (`on_command_applied`; never one the runtime made),
//!    then `tick` when one is due (at least once a second of schedule time,
//!    and at `next_deadline_ns`). Each runs over the room model through
//!    `ControlState::runtime`, which plans the slots, persists and fans out
//!    as a command does. Its effects are applied here: `Log` to the log,
//!    `SourceControl` to the input's own session, a ramp step's
//!    `RoomVolume` to every player of the room with its `ramp_ms`, and
//!    (goal 17) `PlayStored`, `PlaySpotify` and `StopStored`: an alarm's
//!    stored stream URL is played by an in-process
//!    `PlayerSessions::play_held` (owner `alarm:<id>`, via `alarm`), and
//!    whatever stops it from starting is answered to the runtime with a
//!    reason (`no-players`, `no-free-player`, `not-started`), as a later
//!    failure is (`url-refused`, `stream-failed`, `stream-ended`).
//! 2. **Routing** (`--slots S`): what each stream slot plays
//!    ([`SlotCommand`] to the audio thread, at its next chunk boundary: the
//!    configured stream, a rendered chime, a line-in's port, a player's
//!    port (goal 16), silence, and a line-in's latency target: one per
//!    port, the largest any group listening to it needs, goal 17), and which
//!    slot each session hears
//!    (`Router::move_to`, between two ticks).
//! 3. **`room_volume`** to every player session of a room whose gain or
//!    effective limit changed (docs/decisions/0074-*), at once.
//!    Then **`sound`** (goal 12) to every player session whose room's sound,
//!    bass management, room EQ or bonded set changed what it would be told
//!    (its role is its own, so a bond change reaches each member).
//! 4. **`controller_state`** to every controller session of a room whose
//!    volume, mute or group changed, wherever the change was made.
//! 5. **The heard latency** of every visualizer session (goal 12): the
//!    playout latency of its room's tier ([`heard_latency_ns`]), which the
//!    audio thread adds to a visualizer frame's stamp for that session
//!    (`crate::router`, `docs/visualizer.md`). And, for every room an HTTP
//!    subscriber watches the visualizer of (`crate::lights`), the slot the
//!    room is on and the same latency.
//! 6. **The TV path** (goal 13, `crate::tvrelay`): which TV inputs play in
//!    low-latency mode, handed to the relay as a list of [`TvPlay`]s; every
//!    other TV input stays on its slot (ADR 0079), and so does one that more
//!    than one group plays (`reason=shared`, goal 17: its hub sends either
//!    datagrams to the relay or chunks to the slots, never both, so a shared
//!    TV input plays on the slot path for every group, the TV's own room
//!    included, until one group is left). A change of mode is one
//!    line, `tv-path mode=low-latency` or `tv-path mode=slot reason=<why>`.
//!
//! Each push is deduped against what the session was last sent
//! (`crate::router`), so a change concerning another room sends nothing, and a
//! push a full queue refused is left owed and tried again on the next pass,
//! [`RETRY`] later.
//!
//! # The clocks, read here and nowhere else in the server
//!
//! Civil time is for scheduling only (K30): it decides when an alarm rings
//! and which quiet-hours window is active, and nothing it yields crosses to
//! the audio path but a volume in thousandths or a source. The monotonic
//! clock times ramps, fades, holds and sleep timers. Both are read ONCE per
//! wake ([`Clocks::now`]). For tests and a host with no time source the civil
//! clock can be held fixed (`--civil-time`) or run from a given instant
//! (`--civil-time-from`), and `--schedule-time-scale` runs the schedule's
//! durations (and the civil clock it runs from) faster; the audio thread's
//! pace is never scaled, and a ramp step's `ramp_ms` on the wire is divided
//! by the same factor so an endpoint's ramp keeps up with the steps.
//!
//! Control code: it touches no PCM and stamps nothing. `audio-path.conf`
//! records it as excluded.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chorus_control::rooms::Source;
use chorus_control::transport::{Transport, ZoneTransports, WIRELESS_POLICY};
use chorus_protocol::v2::{
    features, roles, Codec, Message, RoomVolume, SourceAction, SourceControl, VoiceControl,
};
use chorus_schedule::chime::CHIMES;

use crate::control::{ControlState, Snapshot};
use crate::linein::{InputEvent, LineIns};
use crate::mediaplayer::PlayerReport;
use crate::playersessions::{Metadata, PlayRefused, PlayRequest, PlayerSessions};
use crate::router::Router;
use crate::schedule_runtime::{Effect, InputAction, Runtime};
use crate::slots::{SlotCommand, SlotInput, LOCAL_LATENCY_NS};
use crate::tvrelay::{TvPlay, TvRelay};

/// How long the conductor waits for a poke before it looks up to see whether
/// it is still wanted.
pub const IDLE: Duration = Duration::from_millis(200);

/// How soon a pass that left something owed (a full queue, a full slot
/// channel) is run again. ASSUMED: a few chunk durations at the default
/// 20 ms; a queue that full is 128 items behind already.
pub const RETRY: Duration = Duration::from_millis(50);

/// The schedule runtime is ticked at least this often, in schedule time
/// (ADR 0076: alarms and quiet windows are minute resolution, ramps step
/// once a second).
pub const TICK: Duration = Duration::from_secs(1);

/// The latency a wired group plays a line-in at: the endpoints' fixed playout
/// latency, `config/sync.conf` `playout_latency_us` (ADR 0071's L_group for
/// the wired tier); a test holds the two equal.
pub const WIRED_GROUP_LATENCY_NS: i64 = 180_000_000;

/// (goal 17) What plays an alarm's stored stream URL: the server's player
/// sessions, and the players' reports when this thread is the one that
/// takes them (a server without `--upnp`).
pub struct StoredStreams {
    sessions: Arc<PlayerSessions>,
    reports: Option<Receiver<PlayerReport>>,
}

impl StoredStreams {
    /// Over `sessions`. `reports` is the players' one report stream
    /// (`Players::take_reports`) when nothing else takes it; `None` when the
    /// UPnP renderers' manager does.
    pub fn new(
        sessions: Arc<PlayerSessions>,
        reports: Option<Receiver<PlayerReport>>,
    ) -> StoredStreams {
        StoredStreams { sessions, reports }
    }
}

/// What one pass did, for the tests and a status line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PassReport {
    /// `room_volume` messages sent.
    pub room_volumes: usize,
    /// `sound` messages sent (goal 12).
    pub sounds: usize,
    /// `controller_state` messages sent.
    pub controller_states: usize,
    /// Sessions moved between fanouts.
    pub moves: usize,
    /// Slot inputs changed.
    pub inputs: usize,
    /// Pushes owed to a full queue, to try again.
    pub owed: usize,
    /// `voice_control` messages sent.
    pub voice_controls: usize,
}

/// Where the civil clock comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CivilClock {
    /// The host's wall clock (`SystemTime`), read once per wake.
    Live,
    /// Held at this UTC instant, seconds (`--civil-time`).
    Fixed(i64),
    /// This UTC instant, in ns, plus the schedule time elapsed since the
    /// conductor started (`--civil-time-from`).
    From(i128),
}

/// The two clocks the schedule runs on.
#[derive(Debug, Clone, Copy)]
pub struct Clocks {
    civil: CivilClock,
    scale: u32,
    origin: Instant,
}

impl Clocks {
    /// Clocks starting now; `scale` (at least 1) speeds up schedule time.
    pub fn new(civil: CivilClock, scale: u32) -> Clocks {
        Clocks {
            civil,
            scale: scale.max(1),
            origin: Instant::now(),
        }
    }

    /// The schedule's speed-up.
    pub fn scale(&self) -> u32 {
        self.scale
    }

    /// Schedule-time monotonic ns and civil UTC seconds, now.
    pub fn now(&self) -> (u64, i64) {
        let real = self.origin.elapsed().as_nanos();
        let mono = real.saturating_mul(u128::from(self.scale));
        let utc_s = match self.civil {
            CivilClock::Live => SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0),
            CivilClock::Fixed(t) => t,
            CivilClock::From(t0_ns) => ((t0_ns + mono as i128).div_euclid(1_000_000_000)) as i64,
        };
        (mono.min(u128::from(u64::MAX)) as u64, utc_s)
    }

    /// Real time until schedule-time instant `mono_ns`.
    fn until(&self, mono_ns: u64) -> Duration {
        let real_target = Duration::from_nanos(mono_ns / u64::from(self.scale));
        real_target.saturating_sub(self.origin.elapsed())
    }
}

/// The time-driven half of the conductor: the schedule runtime, its clocks,
/// the line-ins it starts and stops, and when it is next due.
pub struct Schedule {
    runtime: Runtime,
    clocks: Clocks,
    line_ins: Option<Arc<LineIns>>,
    transports: ZoneTransports,
    next_tick_ns: u64,
}

impl Schedule {
    /// The runtime on `clocks`, starting and stopping `line_ins` (`None`: no
    /// line-in can be played), with each room's declared tier.
    pub fn new(
        runtime: Runtime,
        clocks: Clocks,
        line_ins: Option<Arc<LineIns>>,
        transports: ZoneTransports,
    ) -> Schedule {
        Schedule {
            runtime,
            clocks,
            line_ins,
            transports,
            next_tick_ns: 0,
        }
    }
}

/// The conductor's state between passes.
pub struct Conductor {
    state: Arc<ControlState>,
    router: Arc<Router>,
    slots: Option<SyncSender<SlotCommand>>,
    /// What each slot was last told to play.
    inputs: Vec<SlotInput>,
    /// The latency each line-in port was last told to grow to.
    targets: Vec<Option<i64>>,
    /// (goal 17) The groups last said to listen to each line-in port, for
    /// the log: a listener joining or leaving is one line even when the
    /// target does not move.
    listeners: Vec<String>,
    schedule: Option<Schedule>,
    /// Each room's declared tier, for the visualizer's heard latency.
    transports: ZoneTransports,
    /// (goal 13) The TV relay, and the mode each TV input was last said to
    /// play in (its log word), so a change is one line.
    tv_relay: Option<Arc<TvRelay>>,
    tv_modes: Vec<(chorus_control::rooms::InputId, String)>,
    /// (goal 16) How many players this server runs (`--players`).
    players: usize,
    /// (goal 17) What plays an alarm's stored stream URL; `None` on a
    /// server without `--players`.
    stored: Option<StoredStreams>,
    /// (goal 17) The Soloist receivers, on a server that runs them.
    soloist: Option<Arc<crate::soloist::Link>>,
    /// (goal 18) The announcements, which this thread ends.
    announcer: Option<Arc<crate::announce::Announcer>>,
    /// (voice, P8) What each voice session was last told, and its buffer.
    voice: Option<Arc<crate::voice::Voice>>,
}

/// When an endpoint of a room on `transport` plays audio stamped `t` on the
/// server timeline: `t` plus this, ns. The wired tier's is the endpoints'
/// fixed playout latency ([`WIRED_GROUP_LATENCY_NS`], `config/sync.conf`);
/// the wireless tier's is its policy's (`WIRELESS_POLICY`, which the Linux
/// client applies in a wireless zone).
pub fn heard_latency_ns(transport: Transport) -> u64 {
    match transport {
        Transport::Wireless => WIRELESS_POLICY.playout_latency_us * 1_000,
        Transport::Wired => WIRED_GROUP_LATENCY_NS as u64,
    }
}

impl Conductor {
    /// A conductor over `state`'s room model and `router`'s sessions, telling
    /// the audio thread what each slot plays over `slots` (`None` in the
    /// one-stream shape, which has no slot to tell).
    pub fn new(
        state: Arc<ControlState>,
        router: Arc<Router>,
        slots: Option<SyncSender<SlotCommand>>,
    ) -> Conductor {
        let inputs = vec![SlotInput::Silence; router.slots()];
        let targets = vec![None; router.slots()];
        let listeners = vec![String::new(); router.slots()];
        Conductor {
            state,
            router,
            slots,
            inputs,
            targets,
            listeners,
            schedule: None,
            transports: ZoneTransports::default(),
            tv_relay: None,
            tv_modes: Vec::new(),
            players: 0,
            stored: None,
            soloist: None,
            announcer: None,
            voice: None,
        }
    }

    /// (voice, P8) Tell every voice session whether its room has voice
    /// switched on (`voice_control`), as part of every pass, and keep what
    /// it was told in `voice` (`crate::voice`).
    pub fn with_voice(mut self, voice: Arc<crate::voice::Voice>) -> Conductor {
        self.voice = Some(voice);
        self
    }

    /// (goal 18) End the announcements of `announcer` as part of every pass
    /// (`crate::announce`).
    pub fn with_announcer(mut self, announcer: Arc<crate::announce::Announcer>) -> Conductor {
        self.announcer = Some(announcer);
        self
    }

    /// (goal 17) This server runs Soloist receivers, reached through `link`
    /// (`crate::soloist`): a group whose source is `soloist:r<i>` plays port
    /// `i`, a port is told whether any group plays it, and an alarm's
    /// stored Spotify URI is played by its target's receiver.
    pub fn with_soloist(mut self, link: Arc<crate::soloist::Link>) -> Conductor {
        self.soloist = Some(link);
        self
    }

    /// (goal 17) Play alarms' stored stream URLs through `streams`.
    pub fn with_stored_streams(mut self, streams: StoredStreams) -> Conductor {
        self.stored = Some(streams);
        self
    }

    /// (goal 16) This server runs `players` network media players
    /// (`--players`, `crate::player`): a group whose source is `player:p<i>`
    /// plays port `i`.
    pub fn with_players(mut self, players: usize) -> Conductor {
        self.players = players;
        self
    }

    /// Play TV inputs in low-latency mode through `relay` where they can
    /// (goal 13, `crate::tvrelay`).
    pub fn with_tv_relay(mut self, relay: Arc<TvRelay>) -> Conductor {
        self.tv_relay = Some(relay);
        self
    }

    /// Each room's declared tier (`--zone <id>=<transport>`), for the visualizer's
    /// heard latency; a room not declared is wired.
    pub fn with_transports(mut self, transports: ZoneTransports) -> Conductor {
        self.transports = transports;
        self
    }

    /// Run the schedule runtime as part of every pass.
    pub fn with_schedule(mut self, schedule: Schedule) -> Conductor {
        self.schedule = Some(schedule);
        self
    }

    /// The loop's wake: `true` when woken or timed out and still wanted,
    /// `false` when the run is stopping. Waits at most until the schedule
    /// runtime is next due.
    pub fn wait(&self, woken: &Receiver<()>, owed: bool, keep: &AtomicBool) -> bool {
        let mut timeout = if owed { RETRY } else { IDLE };
        // (goal 17) While this thread takes the players' reports and an
        // alarm's stream is live, look for them at the retry pace: a report
        // does not wake the conductor.
        if self
            .stored
            .as_ref()
            .is_some_and(|s| s.reports.is_some() && s.sessions.any_held())
        {
            timeout = RETRY;
        }
        // (ADR 0174) While an announcement is playing or its music is
        // coming back: the end of a restore wakes nobody.
        if self.announcer.as_ref().is_some_and(|a| a.any_live()) {
            timeout = RETRY;
        }
        if let Some(s) = &self.schedule {
            timeout = timeout.min(s.clocks.until(s.next_tick_ns));
        }
        match woken.recv_timeout(timeout) {
            Ok(()) | Err(RecvTimeoutError::Timeout) => keep.load(Ordering::SeqCst),
            Err(RecvTimeoutError::Disconnected) => false,
        }
    }

    /// One pass: the schedule runtime's work, then read the room model once
    /// and make every session agree with it.
    pub fn pass(&mut self) -> PassReport {
        let mut effects = self.player_events();
        effects.extend(self.soloist_events());
        effects.extend(self.run_schedule());
        let answers = self.stored_requests(&effects);
        effects.extend(answers);
        self.settle_announcements();
        let snapshot = self.state.snapshot();
        let mut report = PassReport::default();
        self.apply_effects(&effects, &snapshot, &mut report);
        for session in self.router.sessions() {
            let room = snapshot.room_of(&session.endpoint);
            if let Some(room) = room {
                if session.roles & roles::PLAYER != 0 {
                    match self.router.push_room_volume(session.id, room.room_volume) {
                        Some(true) => report.room_volumes += 1,
                        Some(false) => {}
                        None => report.owed += 1,
                    }
                    // After room_volume, as in the greeting.
                    if let Some(sound) = room.sound_for(&session.endpoint) {
                        match self.router.push_sound(session.id, sound) {
                            Some(true) => report.sounds += 1,
                            Some(false) => {}
                            None => report.owed += 1,
                        }
                    }
                }
            }
        }
        // Inputs a slot starts playing go out before the sessions move and
        // the ones it stops playing after, so a group moving to another slot
        // (with the moves under the audio thread's grid guard) is heard on
        // one slot or the other at every tick, never on a slot not yet
        // playing it.
        self.route_inputs(&snapshot, &mut report, true);
        // (ADR 0174) The rooms an announcement is mixed over: their player
        // sessions hear its mix, from the command to the end of the
        // restore. Everything else about them is their room's as before.
        let mixed = self
            .announcer
            .as_ref()
            .map(|a| a.routes())
            .unwrap_or_default();
        for session in self.router.sessions() {
            let room = snapshot.room_of(&session.endpoint);
            let mix = room
                .filter(|_| session.roles & roles::PLAYER != 0)
                .and_then(|r| mixed.iter().find(|(id, _)| *id == r.id))
                .and_then(|(_, mix)| self.router.mix_route(*mix));
            let route = mix
                .or_else(|| room.and_then(|r| r.route))
                .unwrap_or_else(|| self.router.idle());
            if self.router.slots() > 0 && self.router.move_to(session.id, route) {
                report.moves += 1;
            }
            let Some(room) = room else {
                continue;
            };
            if session.roles & roles::VISUALIZER != 0 {
                self.router
                    .set_heard_latency(session.id, heard_latency_ns(self.transports.of(&room.id)));
            }
            if session.roles & roles::CONTROLLER != 0 {
                match self
                    .router
                    .push_controller_state(session.id, &room.controller_state, false)
                {
                    Some(true) => report.controller_states += 1,
                    Some(false) => {}
                    None => report.owed += 1,
                }
            }
        }
        // (ADR 0174) Only now, with the sessions on their mixes, is the
        // audio thread told to start a clip's duck.
        if let Some(announcer) = &self.announcer {
            report.owed += announcer.direct(&snapshot);
        }
        self.voice_controls(&snapshot, &mut report);
        // The rooms an HTTP subscriber watches the visualizer of
        // (`crate::lights`): the slot each is on, and when it hears it. A
        // room whose group has no slot, and every room in the one-stream
        // shape, is on none and is sent nothing.
        let slots = self.router.slots();
        self.router.lights().place(|zone| {
            let route = snapshot
                .rooms
                .iter()
                .find(|r| r.id == zone)
                .and_then(|r| r.route)
                .filter(|route| *route < slots);
            (route, heard_latency_ns(self.transports.of(zone)))
        });
        self.route_inputs(&snapshot, &mut report, false);
        self.tv_path(&snapshot);
        report
    }

    /// (voice, P8) `voice_control` to every voice session whose room's
    /// `voice_enabled` is not what it was last told: `uplink` on while the
    /// room has voice switched on, off the moment it has not (or the
    /// endpoint is in no room). A session starts off, so one in a room with
    /// voice off is sent nothing. `listening` is on while the room has a run
    /// open (`crate::voice`; a run opening or ending wakes this pass). This
    /// never opens a
    /// microphone: the gate is the endpoint's own, and the intake reads the
    /// room model for every frame whatever the endpoint was told.
    fn voice_controls(&mut self, snapshot: &Snapshot, report: &mut PassReport) {
        let Some(voice) = self.voice.clone() else {
            return;
        };
        for session in self.router.sessions() {
            if session.roles & roles::VOICE == 0 {
                continue;
            }
            let room = snapshot
                .room_of(&session.endpoint)
                .filter(|room| room.voice_enabled);
            let wanted = VoiceControl {
                uplink: room.is_some(),
                // A run is open in the room (`crate::voice`): its endpoints
                // show that it is being listened to.
                listening: room.is_some_and(|room| voice.listening(&room.id)),
            };
            match voice.told(session.id) {
                Some(told) if told == wanted => {}
                // Not known to the intake yet (its reader is a step behind
                // the router's registration): tried again on the next pass.
                None => report.owed += 1,
                Some(_) => {
                    if self
                        .router
                        .push_message(session.id, &Message::VoiceControl(wanted))
                    {
                        if let Some(line) = voice.tell(session.id, wanted) {
                            println!("chorus-server: {}", line);
                        }
                        report.voice_controls += 1;
                    } else {
                        report.owed += 1;
                    }
                }
            }
        }
    }

    /// (goal 13) Which TV inputs play in low-latency mode: a TV's input
    /// (`optical` or `hdmi_arc`) streaming from its hub, whose group is one
    /// room, the room wired, the input's autoplay rule (if any) saying
    /// `low_latency`, a chunk that fits one datagram, and the hub and every
    /// player of the room advertising `low_latency`. Everything else plays on
    /// its slot, unchanged; the relay is told the list and does the rest.
    fn tv_path(&mut self, snapshot: &Snapshot) {
        let Some(relay) = self.tv_relay.clone() else {
            return;
        };
        let Some(line_ins) = self.schedule.as_ref().and_then(|s| s.line_ins.clone()) else {
            return;
        };
        let sessions = self.router.sessions();
        let capable = |id: u64| {
            sessions
                .iter()
                .find(|s| s.id == id)
                .filter(|s| s.features & features::LOW_LATENCY != 0)
                .and_then(|s| s.peer)
        };
        let mut wanted = Vec::new();
        let mut modes = Vec::new();
        for group in snapshot.slots.iter().flatten() {
            let Source::LineIn(input) = &group.source else {
                continue;
            };
            // (goal 17) How many groups play this input: more than one, and
            // every one of them hears the slot path.
            let listeners = snapshot
                .slots
                .iter()
                .flatten()
                .filter(|g| g.source == group.source)
                .count();
            let Some((hub, _)) = line_ins.tv_source(input) else {
                continue;
            };
            let room = group.rooms.first().cloned().unwrap_or_default();
            let players: Vec<u64> = sessions
                .iter()
                .filter(|s| {
                    s.roles & roles::PLAYER != 0
                        && snapshot.room_of(&s.endpoint).map(|r| r.id.as_str())
                            == Some(room.as_str())
                })
                .map(|s| s.id)
                .collect();
            let addressed: Vec<(u64, std::net::IpAddr)> = players
                .iter()
                .filter_map(|&id| capable(id).map(|ip| (id, ip)))
                .collect();
            let trim_ms = snapshot
                .rooms
                .iter()
                .find(|r| r.id == room)
                .map_or(0, |r| r.av_trim_ms);
            let why = if !group.low_latency {
                Some("rule")
            } else if listeners > 1 {
                Some("shared")
            } else if group.rooms.len() != 1 {
                Some("grouped")
            } else if self.transports.of(&room) == Transport::Wireless {
                Some("wireless")
            } else if !relay.fits() {
                Some("chunk-does-not-fit")
            } else if players.is_empty() {
                Some("no-player")
            } else if addressed.len() != players.len() {
                Some("player-not-capable")
            } else {
                match capable(hub) {
                    None => Some("hub-not-capable"),
                    Some(hub_ip) => {
                        let mut players = addressed;
                        players.sort();
                        let play = TvPlay {
                            input: input.clone(),
                            room: room.clone(),
                            hub: (hub, hub_ip),
                            players,
                            trim_ms,
                        };
                        // Wanted even when it was refused: the relay keeps
                        // a refusal only while it is wanted, and offers it
                        // again once what it is made of changes.
                        wanted.push(play);
                        None
                    }
                }
            };
            modes.push((input.clone(), group.group.clone(), room, why));
        }
        relay.reconcile(&wanted);
        let modes: Vec<(chorus_control::rooms::InputId, String)> = modes
            .into_iter()
            .map(|(input, group, room, why)| {
                let refused =
                    why.is_none() && wanted.iter().any(|p| p.input == input && relay.refused(p));
                let mode = match (why, refused) {
                    (None, false) => {
                        format!("mode=low-latency input={} room={}", input.literal(), room)
                    }
                    (why, _) => format!(
                        "mode=slot input={} group={} reason={}",
                        input.literal(),
                        group,
                        why.unwrap_or("refused")
                    ),
                };
                (input, mode)
            })
            .collect();
        // (goal 17) One line per input: a shared input's groups all say the
        // same mode (`reason=shared`), so the first stands for them.
        let mut modes = modes;
        let mut seen: Vec<chorus_control::rooms::InputId> = Vec::new();
        modes.retain(|(input, _)| {
            let first = !seen.contains(input);
            seen.push(input.clone());
            first
        });
        for (input, mode) in &modes {
            let known = self.tv_modes.iter().find(|(i, _)| i == input);
            if known.map(|(_, m)| m) != Some(mode) {
                println!("chorus-server: tv-path {}", mode);
            }
        }
        self.tv_modes = modes;
    }

    /// (goal 17) The players' side of a pass: take their reports when this
    /// thread is the one that does, and tell the schedule runtime of every
    /// alarm's stream that ended by itself.
    fn player_events(&mut self) -> Vec<Effect> {
        let Some(streams) = self.stored.as_ref() else {
            return Vec::new();
        };
        if let Some(reports) = &streams.reports {
            while let Ok(report) = reports.try_recv() {
                streams.sessions.on_report(&report);
            }
            streams.sessions.reconcile();
        }
        let ended = streams.sessions.take_ended();
        let mut effects = Vec::new();
        for end in ended {
            let Some(alarm) = end.owner.strip_prefix("alarm:") else {
                continue;
            };
            let (reason, detail) = match &end.failure {
                None => ("stream-ended", "the stream ended"),
                Some(why) if why.starts_with("refused:") => ("url-refused", why.as_str()),
                Some(why) => ("stream-failed", why.as_str()),
            };
            effects.extend(self.alarm_source_failed(alarm, reason, detail));
        }
        effects
    }

    /// (goal 18) End the announcements that are over, after the schedule
    /// ran (so an alarm that fired in this pass has already taken its
    /// rooms), and tell the runtime of each one it displaced: a room it
    /// holds goes back to what played before the announcement, never to the
    /// announcement's player.
    fn settle_announcements(&mut self) {
        let Some(announcer) = &self.announcer else {
            return;
        };
        for over in announcer.settle(&self.state) {
            if let Some(schedule) = self.schedule.as_mut() {
                let rooms =
                    schedule
                        .runtime
                        .announcement_over(&over.player, &over.previous, &over.volumes);
                if rooms > 0 {
                    println!(
                        "chorus-server: schedule announcement over player={} rooms={} \
                         go-back-to={}",
                        over.player.literal(),
                        rooms,
                        over.previous.literal()
                    );
                }
            }
        }
    }

    /// (goal 17) The receiver manager's answers to the alarms' Spotify
    /// requests: a receiver that plays becomes its alarm's source (and the
    /// manager is told whether the alarm still wanted it), and every way it
    /// did not start rings the fallback chime with its reason.
    fn soloist_events(&mut self) -> Vec<Effect> {
        let Some(link) = self.soloist.clone() else {
            return Vec::new();
        };
        let mut effects = Vec::new();
        for answer in link.take_answers() {
            match answer {
                crate::soloist::AlarmAnswer::Started {
                    alarm,
                    receiver,
                    source,
                } => {
                    let mut took = false;
                    if let Some(schedule) = self.schedule.as_mut() {
                        let (mono, _) = schedule.clocks.now();
                        let runtime = &mut schedule.runtime;
                        effects.extend(self.state.runtime(|zones| {
                            let (ok, out) =
                                runtime.on_alarm_source_started(&alarm, source, mono, zones);
                            took = ok;
                            out
                        }));
                    }
                    link.alarm_taken(receiver, took);
                }
                crate::soloist::AlarmAnswer::Failed {
                    alarm,
                    reason,
                    detail,
                } => effects.extend(self.alarm_source_failed(&alarm, reason, &detail)),
            }
        }
        effects
    }

    /// Tell the schedule runtime an alarm's stored source failed.
    fn alarm_source_failed(&mut self, alarm: &str, reason: &str, detail: &str) -> Vec<Effect> {
        let Some(schedule) = self.schedule.as_mut() else {
            return Vec::new();
        };
        let (mono, _) = schedule.clocks.now();
        let runtime = &mut schedule.runtime;
        self.state
            .runtime(|zones| runtime.on_alarm_source_failed(alarm, reason, detail, mono, zones))
    }

    /// (goal 17) Carry out the runtime's requests about stored sources and
    /// answer each: what comes back is the runtime's effects for the answers.
    fn stored_requests(&mut self, effects: &[Effect]) -> Vec<Effect> {
        let mut answers = Vec::new();
        for effect in effects {
            match effect {
                Effect::PlayStored {
                    alarm,
                    target,
                    stored: _,
                    url,
                    name,
                } => answers.extend(self.play_stored(alarm, target, url, name)),
                // The Spotify alarm source: the receiver manager checks the
                // target's receiver, sends `play` and answers later
                // ([`Conductor::soloist_events`]). A server that runs no
                // receiver answers the failure at once.
                Effect::PlaySpotify {
                    alarm, target, uri, ..
                } => match &self.soloist {
                    Some(link) => link.play_alarm(alarm, target, uri),
                    None => answers.extend(self.alarm_source_failed(
                        alarm,
                        "soloist-unavailable",
                        "this server runs no Soloist receiver (--soloist-receivers)",
                    )),
                },
                Effect::StopStored { alarm } => {
                    if let Some(streams) = &self.stored {
                        streams.sessions.release_held(&format!("alarm:{}", alarm));
                    }
                }
                _ => {}
            }
        }
        answers
    }

    /// (goal 17) Play an alarm's stored stream URL on a player: the alarm's
    /// group plays the player once the runtime has taken it
    /// (`on_alarm_source_started`), and every way it cannot start is
    /// answered with a reason, which rings the fallback chime.
    fn play_stored(&mut self, alarm: &str, target: &str, url: &str, name: &str) -> Vec<Effect> {
        let Some(streams) = self.stored.as_ref() else {
            return self.alarm_source_failed(
                alarm,
                "no-players",
                "this server was started without --players",
            );
        };
        let Some(schedule) = self.schedule.as_mut() else {
            return Vec::new();
        };
        let request = PlayRequest {
            owner: format!("alarm:{}", alarm),
            target: target.to_string(),
            uri: url.to_string(),
            mime: None,
            via: "alarm".to_string(),
            // One counter for every held play (`PlayerSessions::held_epoch`).
            epoch: streams.sessions.held_epoch(),
            metadata: Metadata {
                title: Some(name.to_string()),
                ..Metadata::default()
            },
            origins: Vec::new(),
        };
        let (mono, _) = schedule.clocks.now();
        let runtime = &mut schedule.runtime;
        let state = &self.state;
        let mut effects = Vec::new();
        let played = streams.sessions.play_held(&request, &mut |source| {
            let Some(source) = Source::parse(source) else {
                return Err(format!("'{}' is not a source", source));
            };
            let mut took = false;
            effects.extend(state.runtime(|zones| {
                let (ok, out) = runtime.on_alarm_source_started(alarm, source, mono, zones);
                took = ok;
                out
            }));
            if took {
                Ok(())
            } else {
                Err("the alarm did not take the player".to_string())
            }
        });
        let failed = match played {
            Ok(_) => None,
            Err(PlayRefused::NoPlayers) => Some(("no-players", PlayRefused::NoPlayers.to_string())),
            Err(refused @ PlayRefused::NoFreePlayer(_)) => {
                Some(("no-free-player", refused.to_string()))
            }
            Err(refused) => Some(("not-started", refused.to_string())),
        };
        if let Some((reason, detail)) = failed {
            effects.extend(self.alarm_source_failed(alarm, reason, &detail));
        }
        effects
    }

    /// The schedule runtime's entry points, in order: what the line-ins said,
    /// what people did, and the tick when it is due.
    fn run_schedule(&mut self) -> Vec<Effect> {
        let Some(schedule) = self.schedule.as_mut() else {
            // No runtime: every applied command is still taken, so the list
            // does not grow for nobody.
            let _ = self.state.take_applied();
            return Vec::new();
        };
        let (mono, utc) = schedule.clocks.now();
        let mut effects = Vec::new();
        if let Some(line_ins) = &schedule.line_ins {
            for event in line_ins.take_events() {
                let runtime = &mut schedule.runtime;
                match event {
                    InputEvent::Signal(input, signal) => effects.extend(
                        self.state
                            .runtime(|zones| runtime.on_input_signal(&input, signal, mono, zones)),
                    ),
                    InputEvent::Standby(input) => effects.extend(
                        self.state
                            .runtime(|zones| runtime.on_input_standby(&input, mono, zones)),
                    ),
                    InputEvent::Gone(input) => effects.extend(
                        self.state
                            .runtime(|zones| runtime.on_input_gone(&input, mono, zones)),
                    ),
                    InputEvent::Refused { input, detail } => {
                        effects.push(Effect::Log(format!(
                            "line-in refused input={} reason=format-mismatch detail=\"{}\"",
                            input.literal(),
                            detail
                        )));
                        effects.push(Effect::SourceControl {
                            input,
                            action: InputAction::Stop,
                        });
                    }
                }
            }
        }
        for command in self.state.take_applied() {
            let runtime = &mut schedule.runtime;
            effects.extend(
                self.state
                    .runtime(|zones| runtime.on_command_applied(&command, mono, zones)),
            );
        }
        if mono >= schedule.next_tick_ns {
            let runtime = &mut schedule.runtime;
            effects.extend(self.state.runtime(|zones| runtime.tick(mono, utc, zones)));
            let step = TICK.as_nanos() as u64;
            let mut next = mono + step;
            if let Some(due) = schedule.runtime.next_deadline_ns() {
                next = next.min(due.max(mono + 1));
            }
            schedule.next_tick_ns = next;
        } else if let Some(due) = schedule.runtime.next_deadline_ns() {
            schedule.next_tick_ns = schedule.next_tick_ns.min(due);
        }
        effects
    }

    /// What the conductor does for the runtime's effects (ADR 0076's table):
    /// the log, `source_control` to an input's session, and a ramp step's
    /// `room_volume`. A source change needs nothing more here: the slots are
    /// routed from the room model below. An at-once `room_volume` is sent by
    /// the pass from the room model; a ramp step goes out here with its
    /// `ramp_ms`, but only while its gain and limit are what the room model
    /// says now (a limit the runtime holds back for a ramp to come down
    /// first goes out at once with the gain instead: ADR 0077's rule that a
    /// limit lowered is in force at the next message).
    fn apply_effects(&self, effects: &[Effect], snapshot: &Snapshot, report: &mut PassReport) {
        let scale = self.schedule.as_ref().map_or(1, |s| s.clocks.scale());
        for effect in effects {
            match effect {
                Effect::Log(line) => println!("chorus-server: {}", line),
                Effect::SetSource { .. } | Effect::Persist => {}
                // (goal 17) Carried out and answered before this, in
                // `stored_requests`.
                Effect::PlayStored { .. }
                | Effect::PlaySpotify { .. }
                | Effect::StopStored { .. } => {}
                Effect::SourceControl { input, action } => self.source_control(input, *action),
                Effect::RoomVolume {
                    zone,
                    gain,
                    limit,
                    ramp_ms,
                } => {
                    if *ramp_ms == 0 {
                        continue;
                    }
                    let Some(room) = snapshot.rooms.iter().find(|r| r.id == *zone) else {
                        continue;
                    };
                    if room.room_volume.gain != *gain || room.room_volume.limit != *limit {
                        continue;
                    }
                    let message = RoomVolume {
                        gain: *gain,
                        limit: *limit,
                        ramp_ms: ramp_ms.div_ceil(scale as u16).max(1),
                    };
                    for session in self.router.sessions() {
                        if session.roles & roles::PLAYER == 0
                            || snapshot.room_of(&session.endpoint).map(|r| &r.id) != Some(zone)
                        {
                            continue;
                        }
                        match self.router.push_room_volume(session.id, message) {
                            Some(true) => report.room_volumes += 1,
                            Some(false) => {}
                            None => report.owed += 1,
                        }
                    }
                }
            }
        }
    }

    fn source_control(&self, input: &chorus_control::rooms::InputId, action: InputAction) {
        let Some(line_ins) = self.schedule.as_ref().and_then(|s| s.line_ins.as_ref()) else {
            return;
        };
        let (addressed, word) = match action {
            InputAction::Start => (line_ins.start(input), "start"),
            InputAction::Stop => (line_ins.stop(input), "stop"),
        };
        let Some(at) = addressed else {
            println!(
                "chorus-server: line-in {} input={} outcome=not-offered-or-no-free-port",
                word,
                input.literal()
            );
            return;
        };
        let message = Message::SourceControl(SourceControl {
            source_id: at.source_id,
            action: match action {
                InputAction::Start => SourceAction::Start,
                InputAction::Stop => SourceAction::Stop,
            },
            codec: Codec::Pcm,
        });
        let sent = self.router.push_message(at.session, &message);
        println!(
            "chorus-server: line-in {} input={} source_id={} port={} sent={} {}",
            word,
            input.literal(),
            at.source_id,
            at.port.map_or("-".to_string(), |p| p.to_string()),
            u8::from(sent),
            line_ins.report()
        );
    }

    /// What a slot plays for its group's source, from what this server holds.
    fn input_for(&self, source: &Source) -> SlotInput {
        match source {
            Source::Stream => SlotInput::Stream,
            Source::None => SlotInput::Silence,
            Source::Chime(name) => CHIMES
                .iter()
                .position(|c| c.name() == name)
                .map_or(SlotInput::Silence, |i| SlotInput::Chime(i as u8)),
            Source::LineIn(input) => self
                .schedule
                .as_ref()
                .and_then(|s| s.line_ins.as_ref())
                .and_then(|l| l.port_of(input))
                .map_or(SlotInput::Silence, |p| SlotInput::LineIn(p as u8)),
            // (goal 16) A player this server does not run plays silence
            // here; a command naming one never gets this far (it is refused
            // by name, `ControlState::apply`).
            Source::Player(id) => crate::player::player_index(id, self.players)
                .map_or(SlotInput::Silence, |p| SlotInput::Player(p as u8)),
            // (goal 17) Never a group's source (the room model refuses it):
            // an alarm's stored source plays as the player it is given.
            Source::Stored(_) => SlotInput::Silence,
            // (goal 17) A receiver this server does not run plays silence.
            Source::Soloist(id) => {
                let receivers = self.soloist.as_ref().map_or(0, |l| l.receivers());
                chorus_soloist::receiver_index(id)
                    .filter(|r| *r < receivers)
                    .map_or(SlotInput::Silence, |r| SlotInput::Soloist(r as u8))
            }
        }
    }

    /// The latency one group needs of a line-in: L_local while it is the
    /// source endpoint's own room alone AND the input's only listener
    /// (`sole`), else the group's tier latency. (goal 17) A port has one
    /// plan, so every group listening to it plays at the largest of these
    /// ([`Conductor::route_inputs`]); the source's own room is on the local
    /// latency only while nobody else listens.
    fn latency_for(
        &self,
        snapshot: &Snapshot,
        rooms: &[String],
        source: &Source,
        sole: bool,
    ) -> i64 {
        let Source::LineIn(input) = source else {
            return LOCAL_LATENCY_NS;
        };
        let own = snapshot.room_of(&input.endpoint).map(|r| r.id.as_str());
        if sole && rooms.len() == 1 && own == Some(rooms[0].as_str()) {
            return LOCAL_LATENCY_NS;
        }
        let wireless = self.schedule.as_ref().is_some_and(|s| {
            rooms
                .iter()
                .any(|r| s.transports.of(r) == Transport::Wireless)
        });
        if wireless {
            WIRELESS_POLICY.playout_latency_us as i64 * 1_000
        } else {
            WIRED_GROUP_LATENCY_NS
        }
    }

    /// Tell the audio thread what each slot plays: with `starting`, only the
    /// slots that start playing something.
    fn route_inputs(&mut self, snapshot: &Snapshot, report: &mut PassReport, starting: bool) {
        let Some(slots) = &self.slots else {
            return;
        };
        for (slot, group) in snapshot.slots.iter().enumerate() {
            let wanted = match group {
                Some(g) => self.input_for(&g.source),
                None => SlotInput::Silence,
            };
            if self.inputs.get(slot) != Some(&wanted) && !(starting && wanted == SlotInput::Silence)
            {
                match slots.try_send(SlotCommand::Input {
                    slot,
                    input: wanted,
                }) {
                    Ok(()) => {
                        self.inputs[slot] = wanted;
                        report.inputs += 1;
                    }
                    Err(TrySendError::Full(_)) => {
                        report.owed += 1;
                        continue;
                    }
                    Err(TrySendError::Disconnected(_)) => continue,
                }
            }
        }
        // (goal 17) A Soloist receiver's port is told whether any slot plays
        // it: its reader discards what nobody hears, and a change either
        // way empties the port, so a group that takes a receiver hears
        // nothing that arrived before it did.
        if let Some(link) = &self.soloist {
            for (index, port) in link.ports().iter().enumerate() {
                let played = self.inputs.contains(&SlotInput::Soloist(index as u8));
                if port.is_selected() != played {
                    port.select(played);
                }
            }
        }
        // (goal 17) One latency target per line-in PORT, not per slot: an
        // input may play in any number of groups, every one of them cuts
        // the same port's chunks, and the port has one plan. Its target is
        // the largest any listening group needs, so the group that needs
        // the most is served and the others simply play that much later.
        let mut ports: Vec<(usize, Vec<&crate::control::SlotGroup>)> = Vec::new();
        for g in snapshot.slots.iter().flatten() {
            let SlotInput::LineIn(port) = self.input_for(&g.source) else {
                continue;
            };
            let port = usize::from(port);
            match ports.iter_mut().find(|(p, _)| *p == port) {
                Some((_, groups)) => groups.push(g),
                None => ports.push((port, vec![g])),
            }
        }
        for (port, groups) in ports {
            let sole = groups.len() == 1;
            let latency_ns = groups
                .iter()
                .map(|g| self.latency_for(snapshot, &g.rooms, &g.source, sole))
                .max()
                .unwrap_or(LOCAL_LATENCY_NS);
            let names = groups
                .iter()
                .map(|g| g.group.as_str())
                .collect::<Vec<_>>()
                .join(",");
            let moved = self.targets.get(port).copied().flatten() != Some(latency_ns);
            if moved {
                match slots.try_send(SlotCommand::LatencyTarget { port, latency_ns }) {
                    Ok(()) => {
                        if let Some(t) = self.targets.get_mut(port) {
                            *t = Some(latency_ns);
                        }
                    }
                    Err(TrySendError::Full(_)) => {
                        report.owed += 1;
                        continue;
                    }
                    Err(TrySendError::Disconnected(_)) => continue,
                }
            }
            if moved || self.listeners.get(port) != Some(&names) {
                println!(
                    "chorus-server: line-in latency port={} listeners={} groups={} target_ms={}",
                    port,
                    groups.len(),
                    names,
                    latency_ns / 1_000_000
                );
                if let Some(l) = self.listeners.get_mut(port) {
                    *l = names;
                }
            }
        }
    }
}

/// The `conductor` thread's loop. Returns when `keep` says stop.
pub fn run(mut conductor: Conductor, keep: Arc<AtomicBool>) {
    let Some(woken) = conductor.state.take_conductor_wake() else {
        return;
    };
    // The first pass sets every slot's input from the model as it starts.
    let mut owed = conductor.pass().owed > 0;
    while conductor.wait(&woken, owed, &keep) {
        owed = conductor.pass().owed > 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    use chorus_control::rooms::{CivilTime, ClockTime};
    use chorus_control::zones::{Zone, Zones};
    use chorus_protocol::v2::{decode_frame, Message, Outcome, RoomVolume};

    use crate::stream::{Outbound, SUBSCRIBER_QUEUE_LIMIT};

    fn kitchen() -> Arc<ControlState> {
        let mut zones = Zones::new("127.0.0.1:4010");
        let mut zone = Zone::new("kitchen");
        zone.endpoints.push("speaker".to_string());
        zones.add(zone).unwrap();
        Arc::new(ControlState::new(zones, None))
    }

    fn room_volumes(inbox: &mpsc::Receiver<Outbound>) -> Vec<RoomVolume> {
        inbox
            .try_iter()
            .filter_map(|o| match o {
                Outbound::Frame(bytes) => match decode_frame(&bytes).outcome {
                    Outcome::Decoded(Message::RoomVolume(m)) => Some(m),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_wired_group_latency_is_the_committed_playout_latency() {
        let conf = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../config/sync.conf"
        ))
        .unwrap();
        let us: i64 = conf
            .lines()
            .find_map(|l| l.strip_prefix("playout_latency_us = "))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert_eq!(WIRED_GROUP_LATENCY_NS, us * 1_000);
    }

    #[test]
    fn a_visualizer_frame_is_heard_after_its_rooms_tier_latency() {
        assert_eq!(heard_latency_ns(Transport::Wired), 180_000_000);
        assert_eq!(
            heard_latency_ns(Transport::Wireless),
            WIRELESS_POLICY.playout_latency_us * 1_000
        );
    }

    #[test]
    fn a_civil_clock_from_an_instant_runs_at_the_schedule_scale() {
        let clocks = Clocks::new(CivilClock::From(1_000 * 1_000_000_000), 20);
        std::thread::sleep(Duration::from_millis(120));
        let (mono, utc) = clocks.now();
        // 120 ms real is at least 2.4 s of schedule time.
        assert!(mono >= 2_400_000_000, "{}", mono);
        assert!(utc >= 1_002, "{}", utc);
        assert!(clocks.until(mono + 2_000_000_000) <= Duration::from_millis(100));
        let fixed = Clocks::new(CivilClock::Fixed(77), 1);
        assert_eq!(fixed.now().1, 77);
    }

    #[test]
    fn a_quiet_window_starting_at_the_civil_time_given_is_pushed_once_and_clamped() {
        let state = kitchen();
        let router = Arc::new(Router::single(Arc::new(crate::stream::Fanout::new())));
        let (out, inbox) = mpsc::sync_channel(SUBSCRIBER_QUEUE_LIMIT);
        let start = state.session_start("speaker", roles::PLAYER, router.idle());
        assert_eq!(
            start.room_volume,
            Some(RoomVolume {
                gain: 1000,
                limit: 1000,
                ramp_ms: 0
            })
        );
        router.register("speaker", roles::PLAYER, out, &start);
        let mut conductor = Conductor::new(Arc::clone(&state), Arc::clone(&router), None);
        assert_eq!(
            conductor.pass().room_volumes,
            0,
            "the greeting said it already"
        );
        state
            .apply(
                r#"{"v":2,"t":"quiet_hours","zone":"kitchen","windows":[{"days":["mon"],"start":"22:00","end":"07:00","limit":0.200}]}"#,
            )
            .unwrap();
        assert_eq!(
            conductor.pass().room_volumes,
            0,
            "no civil time yet: no window is active"
        );
        let late = CivilTime {
            weekday: 0,
            time: ClockTime::parse("23:30").unwrap(),
        };
        assert!(state.set_civil_time(Some(late)));
        assert_eq!(conductor.pass().room_volumes, 1);
        assert_eq!(conductor.pass().room_volumes, 0, "deduped");
        assert_eq!(
            room_volumes(&inbox),
            vec![RoomVolume {
                gain: 200,
                limit: 200,
                ramp_ms: 0
            }]
        );
    }
}
