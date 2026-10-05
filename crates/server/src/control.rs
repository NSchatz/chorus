//! The control channel: subscribers attach, commands apply, and the resulting
//! state goes to everybody.
//!
//! # Every thread this needs exists before the scheduling report
//!
//! This is the hazard `crates/server/src/main.rs` names in its own module
//! documentation and the one AC-12 is written about. `std::thread::spawn`
//! inherits the creating thread's scheduling policy, and
//! `deploy/run-server.sh` runs this binary with `--ulimit rtprio=20`, so a
//! control plane that spawned a thread per subscriber would be putting
//! real-time threads on a host that also runs Home Assistant and MQTT, none of
//! which the report the host contract is graded on had ever seen.
//!
//! So the shape here is `crates/server/src/clients.rs`'s shape, for the same
//! reason: a FIXED pool of worker threads, created before the report is taken,
//! each registering itself, and a connection arriving with every worker busy is
//! refused by name rather than served by a thread nobody declared. Two more
//! threads serve it, created with the pool: the **event writer**
//! (`crate::events`), which holds every `GET /api/events` stream so that a
//! subscriber costs no worker (audit finding B-5), and the **conductor**
//! (`crate::conductor`), which carries every change to the sessions it
//! concerns (`room_volume`, `controller_state`, which stream slot a session
//! hears). The whole process is `6 + 2N + M` threads with the control plane
//! on, against `3 + 2N` with it off, plus the TV relay's one with any
//! `--slots` (goal 13, `crate::tvrelay`) whatever their number, plus the MQTT
//! publisher's one with `--mqtt-broker` and none without (goal 15,
//! `crate::mqtt`), and
//! `crates/server/tests/control_thread_population.rs` grades that against
//! `/proc`.
//!
//! # It speaks HTTP, and why
//!
//! One port, three routes that matter: the page, `POST /api/command` carrying
//! one control message, and `GET /api/events`, which is a server-sent event
//! stream carrying one state message per change. A browser can open the last of
//! those with nothing but `EventSource`, and a shell script can open it with a
//! socket and a `GET` line, so THE UI AND THE VERIFICATION SCRIPTS ARE THE SAME
//! SUBSCRIBER. That is deliberate: a check that exercised a second, private
//! subscriber protocol would not be checking the thing the browser uses.
//!
//! `GET /api/controller-events` is the same subscriber for a different
//! payload: one `controller_event` message per controller command accepted (a
//! speaker's button press), nothing at the start and nothing replayed. An
//! event is not a state change, so it rides in no state message.
//!
//! The catalog is unchanged by that choice. `docs/control-plane.md` defines the
//! messages, `fixtures/control/` pins their bytes, and HTTP is the envelope
//! they arrive in.
//!
//! # What it is not
//!
//! There is no authentication, no authorisation and no TLS here, and the
//! listen address is configured rather than defaulted to anything reachable.
//! `docs/control-plane.md` says so out loud rather than leaving it to be
//! discovered.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use chorus_control::catalog::{decode_message, ControllerEvent, Refusal, VoiceRun, VoiceWake};
use chorus_control::fanout::ControlFanout;
use chorus_control::persist;
use chorus_control::rooms::{CivilTime, NowPlaying, PlayState, Role, Source};
use chorus_control::sound::Polarity;
use chorus_control::speakers::{KeyChange, NotListed, SpeakerNow};
use chorus_control::zones::{Zone, Zones};
use chorus_control::Command;
use chorus_hostctl::ThreadRegistry;
use chorus_protocol::v2::{
    roles, sound_flags, sound_fold, ControllerCommand, ControllerState, Playback, RoomVolume,
    Sound, SoundFilter,
};

use crate::controller::{translate, ControllerAction, THOUSANDTHS_PER_POINT};
use crate::events::EventStreams;
use crate::firmware::Firmware;
use crate::hostreport::register_ordinary_thread;
use crate::router::SessionStart;
use crate::schedule_runtime::Effect;
use crate::session::Adoptions;
use crate::slot_table::SlotTable;

/// Longest request the control channel will read: request line, headers and
/// body together.
///
/// A control message is a few hundred bytes and the largest legitimate request
/// here is a `POST` of one. This is what stops a peer that opens a connection
/// and never stops typing from being an allocation. It holds because every byte
/// of a request is read through one `take` of this many bytes plus one, so a
/// line with no end is cut at the bound rather than grown until an allocation
/// fails (under `mlockall` that failure is the whole process, audio included).
pub const MAX_REQUEST_BYTES: usize = 16 * 1024;

/// How long a worker gives one peer to deliver its whole request, counted from
/// the moment the worker picks the connection up.
///
/// A read timeout alone is per `read` call, so a peer sending one byte just
/// inside it would keep its worker for ever, and a handful of such peers would
/// hold the whole fixed pool. This is a deadline for the REQUEST: every read is
/// given only what is left of it, on the monotonic clock, and a request not
/// complete by then is answered `408` and the slot comes back. A real request
/// arrives in one segment on a LAN, so this is generous by orders of magnitude.
pub const REQUEST_DEADLINE: Duration = Duration::from_secs(5);

/// How long, after refusing a request it did not finish reading, a worker keeps
/// reading and discarding what the peer is still sending before it closes.
///
/// Closing a socket with unread data in it makes the kernel answer with a
/// reset, and a reset can cost the peer the refusal that said why. A short,
/// bounded drain after the answer is what keeps a `413` or `431` readable; it
/// is bounded in time and in bytes so it cannot become the hold it prevents.
const LINGER: Duration = Duration::from_millis(250);
const LINGER_BYTES: usize = 64 * 1024;

/// How often a held-open event stream sends a comment line.
///
/// A server-sent event stream that says nothing looks identical to one whose
/// connection has died, to a proxy and to a browser both. A `:` line is a
/// comment in the event-stream format and is what keeps it visibly alive; it
/// is also what makes the event writer notice a peer that has gone, because
/// writing to a closed socket is the only way this end learns.
pub const KEEPALIVE: Duration = Duration::from_secs(15);

/// How long a write to a control peer may block before that peer is dropped.
///
/// [`chorus_control::fanout::ControlFanout`] answers the slow subscriber at the
/// APPLICATION layer: every subscriber's queue is bounded, and one that stops
/// consuming is removed at the ceiling with what it missed counted. That is the
/// whole of the criterion and it holds. It is not the whole of the hazard.
///
/// A peer that stops draining its TCP receive window - rather than closing -
/// blocks its worker inside `write` for as long as the kernel is willing to
/// wait, which is indefinitely. The fanout still drops the subscriber, but the
/// WORKER never gets back to `recv_timeout`, so its slot in the fixed pool is
/// never returned; enough such peers and the accept loop answers everyone
/// `503`, which is a fixed thread pool being denied to the people entitled to
/// it. The pool is fixed on purpose (`crates/server/src/main.rs`: every thread
/// exists before the scheduling report), so a stuck slot cannot be replaced by
/// growing the pool and has to be reclaimed instead.
///
/// This bounds it. A write that cannot make progress in this long is an error,
/// the connection is dropped and the slot comes back. Deliberately far longer
/// than any legitimate write here needs - a state message is a few hundred
/// bytes and the largest this server can build is tens of kilobytes - so a
/// merely slow peer is not cut off, and shorter than [`KEEPALIVE`] so a stuck
/// stream is reclaimed before the next comment line would have been due.
///
/// The event writer (`crate::events`) holds every event stream to the same
/// bound, counted as time without write progress, because it writes them all
/// and one that could block it would delay every other.
pub const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// Why the control channel could not start.
#[derive(Debug)]
pub enum ControlRefused {
    /// The configured address could not be bound, or was denied.
    Bind {
        /// The address that was asked for.
        address: String,
        /// What the operating system said.
        cause: io::Error,
    },
    /// The persisted state file could not be read, or is not this format.
    State {
        /// The file that was read.
        path: String,
        /// What was wrong with it.
        detail: String,
    },
    /// The zones named on the command line are not a set of zones.
    Zones {
        /// What was wrong.
        detail: String,
    },
}

impl std::fmt::Display for ControlRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ControlRefused::Bind { address, cause } => write!(
                f,
                "the control channel address {} could not be bound: {}. This server will not \
                 serve audio while reporting itself as controllable, so it is stopping instead",
                address, cause
            ),
            ControlRefused::State { path, detail } => write!(
                f,
                "the persisted zone state at {} could not be read: {}. Starting with defaults \
                 would silently discard whatever a person had already set, so it is stopping \
                 instead",
                path, detail
            ),
            ControlRefused::Zones { detail } => write!(f, "{}", detail),
        }
    }
}

impl std::error::Error for ControlRefused {}

/// The room model and the stream slots it is served on, under one lock so a
/// change and its slot plan commit together or not at all.
struct Held {
    zones: Zones,
    /// The stream slots (`--slots S`), or `None` in the one-stream shape,
    /// where every group is served the one stream and nothing needs a slot.
    slots: Option<SlotTable>,
}

/// Everything the control plane holds, shared by every worker.
pub struct ControlState {
    held: Mutex<Held>,
    fanout: Arc<ControlFanout>,
    /// The second fanout: one `controller_event` per controller command
    /// accepted, to every `GET /api/controller-events` stream. The same
    /// bounded queue and the same drop of a subscriber at its ceiling as the
    /// state's, and nothing kept: a broadcast with nobody attached is gone,
    /// so a stream opened afterwards is never sent a past press.
    presses: Arc<ControlFanout>,
    /// (voice, P8) The third fanout: one `voice_wake` per wake word heard,
    /// to every `GET /api/voice-events` stream, under the rules of the
    /// second: bounded, and nothing kept for a stream opened later.
    voice_events: Arc<ControlFanout>,
    /// (voice, P8) The voice path ([`ControlState::voice_through`]): where a
    /// `voice_start` opens a run and the reader of one is handed its audio.
    /// Unset, every `voice_start` is refused by name.
    voice: OnceLock<Arc<crate::voice::Voice>>,
    /// (voice, P8) The one address a run's audio is served to
    /// (`--voice-integration`, the home automation's own). Unset, no run
    /// opens, because nothing could read it.
    voice_integration: OnceLock<IpAddr>,
    /// The visualizer stream's tap (`crate::lights`), which the router owns:
    /// what a `GET /api/visualizer` stream subscribes to. Set once, at start.
    lights: OnceLock<Arc<crate::lights::LightTap>>,
    state_file: Option<PathBuf>,
    /// Commands applied since the process started.
    applied: AtomicU64,
    /// Commands refused since the process started.
    refused: AtomicU64,
    /// Connections turned away because every worker was busy, and event
    /// streams turned away because every one the event writer may hold was
    /// held.
    turned_away: AtomicU64,
    /// The event streams the event writer holds, and how one is handed to it.
    events: EventStreams,
    /// Poked after every change, so the conductor carries it to the sessions
    /// it concerns. Bounded at one: a poke already waiting covers every
    /// change made before the conductor next looks.
    conductor_wake: SyncSender<()>,
    conductor_woken: Mutex<Option<Receiver<()>>>,
    /// Every person's command applied since the conductor last looked, in
    /// order: the schedule runtime is told of each (`on_command_applied`),
    /// and never of a change it made itself.
    applied_commands: Mutex<Vec<Command>>,
    /// What the page is served with, so the UI is one artifact and not three.
    ui: Ui,
    /// (goal 14) The pins the audio sessions adopt through, once the server
    /// has an identity ([`ControlState::adopt_through`]): `speaker_forget`
    /// removes a speaker's pin here, and nothing else in the control plane
    /// touches it.
    adoptions: OnceLock<Arc<Adoptions>>,
    /// (goal 14) The staged firmware images and the installs in progress,
    /// once the server has made them ([`ControlState::firmware_through`]):
    /// `firmware_install` starts a transfer here, inside the command's own
    /// commit, and nothing else in the control plane does.
    firmware: OnceLock<Arc<Firmware>>,
    /// (goal 15) Each speaker's latest `telemetry`, for `GET /metrics`
    /// (`crate::metrics`). Under its own lock and outside the room model: a
    /// report about once a second per speaker never fans a state out.
    telemetry: crate::metrics::TelemetryStore,
    /// (goal 15) Where accepted controller commands are left for the MQTT
    /// publisher, once the server has made one
    /// ([`ControlState::publish_events_through`]); empty with the publisher
    /// off, and then nothing is left anywhere.
    mqtt_events: OnceLock<crate::mqtt::EventTap>,
    /// (goal 16) How many network media players this server runs
    /// (`--players`; [`ControlState::set_players`]): a `take` whose source
    /// names a player it does not run is refused by name.
    players: AtomicUsize,
    /// (goal 17) The Soloist receivers' link ([`ControlState::soloist_through`]):
    /// where `soloist_restart` and `playback` go, and whose numbers the
    /// metrics endpoint prints. Unset on a server without
    /// `--soloist-receivers`.
    soloist: OnceLock<Arc<crate::soloist::Link>>,
    /// (goal 18) Who this server is, for `GET /api/server`
    /// ([`ControlState::set_server`]): its `id` and its version string.
    server: OnceLock<(String, String)>,
    /// (goal 18) What carries out an `announce`
    /// ([`ControlState::announce_through`]); a state without one refuses
    /// the command by name.
    announcer: OnceLock<Arc<crate::announce::Announcer>>,
}

/// The files the browser is served: the page, what it loads, and the document
/// every region of the page links to. The document is served rather than only
/// committed, because a link to an explanation that answers 404 in a browser is
/// a link to nothing.
struct Ui {
    html: &'static str,
    tokens: &'static str,
    css: &'static str,
    js: &'static str,
    doc: &'static str,
}

/// The policy the browser is handed with the page and with every asset the page
/// loads.
///
/// Tight enough to be worth having: nothing loads from anywhere but this origin,
/// there is no inline script or inline style for anything to be smuggled into,
/// and the page cannot be framed. Wide enough that the page still works: the
/// stylesheet and the script are served from here, and `connect-src` is what the
/// state request and the event stream travel on. A policy that silenced the page
/// would be a failure and not a pass, which is why the check that grades this
/// asserts the page still renders its zones and still updates under it.
///
/// It says nothing about what `POST /api/command` accepts. A Content-Security-
/// Policy constrains what a BROWSER may load and connect to; the control
/// listener's acceptance rules are `docs/control-plane.md`'s, and this listener
/// has no authentication either before or after this header, which is a fact
/// about the deployment and not something a response header can change.
const CONTENT_SECURITY_POLICY: &str = "default-src 'none'; script-src 'self'; style-src 'self'; \
     connect-src 'self'; img-src 'self' data:; base-uri 'none'; form-action 'none'; \
     frame-ancestors 'none'";

impl ControlState {
    /// Build the shared state.
    pub fn new(zones: Zones, state_file: Option<PathBuf>) -> ControlState {
        let (conductor_wake, woken) = mpsc::sync_channel(1);
        ControlState {
            held: Mutex::new(Held { zones, slots: None }),
            fanout: Arc::new(ControlFanout::new()),
            presses: Arc::new(ControlFanout::new()),
            voice_events: Arc::new(ControlFanout::new()),
            voice: OnceLock::new(),
            voice_integration: OnceLock::new(),
            lights: OnceLock::new(),
            state_file,
            applied: AtomicU64::new(0),
            refused: AtomicU64::new(0),
            turned_away: AtomicU64::new(0),
            events: EventStreams::new(crate::events::DEFAULT_EVENT_STREAMS),
            conductor_wake,
            conductor_woken: Mutex::new(Some(woken)),
            applied_commands: Mutex::new(Vec::new()),
            ui: Ui {
                html: include_str!("ui/index.html"),
                tokens: include_str!("ui/tokens.css"),
                css: include_str!("ui/chorus.css"),
                js: include_str!("ui/chorus.js"),
                doc: include_str!("../../../docs/control-page.md"),
            },
            adoptions: OnceLock::new(),
            firmware: OnceLock::new(),
            telemetry: crate::metrics::TelemetryStore::new(),
            mqtt_events: OnceLock::new(),
            players: AtomicUsize::new(0),
            soloist: OnceLock::new(),
            server: OnceLock::new(),
            announcer: OnceLock::new(),
        }
    }

    /// (goal 18) Say who this server is: its stable `id`
    /// (`chorus_control::catalog::is_server_id`) and its version string.
    /// Called once, at start, before any request is served.
    pub fn set_server(&self, id: &str, software: &str) {
        let _ = self.server.set((id.to_string(), software.to_string()));
    }

    /// (goal 18) The `server` message `GET /api/server` answers with, or
    /// `None` on a state nobody gave an identity.
    pub fn server_info(&self) -> Option<String> {
        let (id, software) = self.server.get()?;
        let announce_origins = self
            .locked()
            .zones
            .announce_origins()
            .iter()
            .map(|origin| origin.literal())
            .collect();
        Some(
            chorus_control::catalog::ServerInfo {
                id: id.clone(),
                software: software.clone(),
                catalogs: chorus_control::catalog::IMPLEMENTED_VERSIONS.to_vec(),
                announce_origins,
            }
            .encode(),
        )
    }

    /// (goal 18) Carry out `announce` commands through `announcer`. Called
    /// once, at start, before any command is served.
    pub fn announce_through(&self, announcer: Arc<crate::announce::Announcer>) {
        let _ = self.announcer.set(announcer);
    }

    /// (goal 18) Whether the room model would let `url` be announced on
    /// `target` (`Zones::announce_check`).
    pub fn announce_check(&self, target: &str, url: &str) -> Result<(), Refusal> {
        self.locked().zones.announce_check(target, url)
    }

    /// (goal 18) The formed group an announcement to `target` would play in
    /// as things stand (`Zones::announce_group`).
    pub fn announce_group(&self, target: &str) -> Option<String> {
        self.locked().zones.announce_group(target)
    }

    /// (goal 18) The room model's half of starting an announcement
    /// (`Zones::announce_begin`), committed like a command: planned onto the
    /// stream slots (refused by name when none is free), persisted, fanned
    /// out and counted as applied. The schedule runtime is NOT told, as it
    /// is of a person's command: an announcement is temporary and puts back
    /// what it changed, so it ends no autoplay and detaches no room.
    pub fn announce_begin(
        &self,
        target: &str,
        player: Source,
        volume: Option<chorus_control::Volume>,
    ) -> Result<chorus_control::zones::Announced, Refusal> {
        let (announced, state) = {
            let mut held = self.locked();
            let mut announced = None;
            Self::commit(&mut held, |zones| {
                announced = Some(zones.announce_begin(target, player, volume)?);
                Ok(())
            })?;
            self.persist(&held.zones);
            (announced, held.zones.encode_state())
        };
        self.applied.fetch_add(1, Ordering::Relaxed);
        self.publish(state);
        announced
            .ok_or_else(|| Refusal::rejected("t", "the announcement did not start".to_string()))
    }

    /// (ADR 0174) The rooms an announcement to `target` would be mixed over
    /// (`Zones::announce_over_rooms`), and what the group it would play in
    /// plays now (`Zones::announce_source`), read together.
    pub fn announce_over_plan(&self, target: &str) -> (Vec<String>, Option<Source>) {
        let held = self.locked();
        (
            held.zones.announce_over_rooms(target),
            held.zones.announce_source(target),
        )
    }

    /// (ADR 0174) The room model's half of starting an announcement that is
    /// mixed over what its rooms play (`Zones::announce_over_begin`),
    /// committed as [`ControlState::announce_begin`] is.
    pub fn announce_over_begin(
        &self,
        target: &str,
        volume: Option<chorus_control::Volume>,
    ) -> Result<chorus_control::zones::Announced, Refusal> {
        let (announced, state) = {
            let mut held = self.locked();
            let mut announced = None;
            Self::commit(&mut held, |zones| {
                announced = Some(zones.announce_over_begin(target, volume)?);
                Ok(())
            })?;
            self.persist(&held.zones);
            (announced, held.zones.encode_state())
        };
        self.applied.fetch_add(1, Ordering::Relaxed);
        self.publish(state);
        announced
            .ok_or_else(|| Refusal::rejected("t", "the announcement did not start".to_string()))
    }

    /// (ADR 0174) Whether an announcement mixed over `rooms`, started in
    /// `group`, still has them (`Zones::announce_watch`).
    pub fn announce_watch(
        &self,
        rooms: &[String],
        group: &str,
    ) -> chorus_control::zones::AnnounceWatch {
        self.locked().zones.announce_watch(rooms, group)
    }

    /// (ADR 0174) Say which announcements are playing and how the last few
    /// ended (`Zones::set_announcements`): the announcer's hook. Fanned out
    /// only when something changed; nothing is persisted.
    pub fn set_announcements(
        &self,
        announcements: Vec<chorus_control::zones::Announcement>,
    ) -> bool {
        let state = {
            let mut held = self.locked();
            if !held.zones.set_announcements(announcements) {
                return false;
            }
            held.zones.encode_state()
        };
        self.publish(state);
        true
    }

    /// (goal 18) The end of an announcement: the group that plays `player`
    /// now, if one still does, goes back to `back` (to `none` when the room
    /// model refuses `back`), and every room in `volumes` (the room, the
    /// volume it had, the volume the announcement gave it) whose volume is
    /// still the announcement's goes back to the one it had, clamped like
    /// every volume. Never refused for want of a slot
    /// ([`ControlState::runtime`]). Returns what the group was put back on,
    /// or `None` when no group played the player any more.
    pub fn announce_end(
        &self,
        player: &Source,
        back: &Source,
        volumes: &[(String, chorus_control::Volume, chorus_control::Volume)],
    ) -> Option<Source> {
        let mut restored = None;
        self.runtime(|zones| {
            if let Some(group) = zones.source_group(player) {
                let source = match zones.set_group_source(&group, back.clone()) {
                    Ok(()) => back.clone(),
                    Err(_) => {
                        let _ = zones.set_group_source(&group, Source::None);
                        Source::None
                    }
                };
                restored = Some(source);
            }
            for (room, before, set) in volumes {
                if zones.zone(room).is_some_and(|z| z.volume == *set) {
                    let _ = zones.runtime_volume(room, *before);
                }
            }
            vec![Effect::Persist]
        });
        restored
    }

    /// (goal 17) This server runs Soloist receivers, reached through `link`:
    /// the state lists them from now on (every one `absent` until its
    /// supervisor connects), and `soloist_restart` and `playback` have
    /// somewhere to go. Called once, at start, before any command is served.
    pub fn soloist_through(&self, link: Arc<crate::soloist::Link>) {
        self.set_soloist(Some(link.initial_state()));
        let _ = self.soloist.set(link);
    }

    /// (goal 17) Say what the receivers are doing (`Zones::set_soloist`):
    /// the receiver manager's hook. Fanned out only when something changed;
    /// nothing is persisted.
    pub fn set_soloist(&self, soloist: Option<chorus_control::rooms::SoloistState>) -> bool {
        let state = {
            let mut held = self.locked();
            if !held.zones.set_soloist(soloist) {
                return false;
            }
            held.zones.encode_state()
        };
        self.publish(state);
        true
    }

    /// (goal 17) The server half of `soloist_restart` and `playback`, after
    /// the room model agreed: hand the command to the receiver manager.
    fn soloist_effects(&self, zones: &Zones, command: &Command) -> Result<(), Refusal> {
        let Some(link) = self.soloist.get() else {
            return Ok(());
        };
        match command {
            Command::SoloistRestart => link.restart(),
            Command::Playback { target, action } => {
                let group = zones.playback_group(target)?;
                if let Source::Soloist(id) = zones.source(&group) {
                    if let Some(receiver) = chorus_soloist::receiver_index(&id) {
                        link.playback(receiver, *action);
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// (goal 16) Say how many network media players this server runs
    /// (`--players`): `p0` to `p<players-1>`. Called once, before any
    /// command is served.
    pub fn set_players(&self, players: usize) {
        self.players.store(players, Ordering::SeqCst);
    }

    /// (goal 16) How many players this server runs.
    pub fn players(&self) -> usize {
        self.players.load(Ordering::SeqCst)
    }

    /// (goal 16) The formed group playing `player:<id>` now, if one is: where
    /// the player's driver puts what it is playing, and how it learns that
    /// nobody plays it any more.
    pub fn player_group(&self, id: &str) -> Option<String> {
        self.locked().zones.player_group(id)
    }

    /// (goal 16) What `group`'s player source is playing now, as the room
    /// model holds it.
    pub fn now_playing(&self, group: &str) -> Option<NowPlaying> {
        self.locked().zones.now_playing(group).cloned()
    }

    /// (goal 16) Say what `group`'s player source is playing now, or (`None`)
    /// that nothing is known: the runtime hook a player's driver calls
    /// (`Zones::set_now_playing`). The state is fanned out, and the conductor
    /// woken (a controller is shown whether its room is paused), only when
    /// something changed; nothing is persisted, because nothing persisted
    /// moved. Refused by name for a group that is not formed or does not play
    /// a player source.
    pub fn set_now_playing(
        &self,
        group: &str,
        playing: Option<NowPlaying>,
    ) -> Result<bool, Refusal> {
        let state = {
            let mut held = self.locked();
            if !held.zones.set_now_playing(group, playing)? {
                return Ok(false);
            }
            held.zones.encode_state()
        };
        self.publish(state);
        Ok(true)
    }

    /// (goal 16) A `take` that names a player this server does not run is
    /// refused by name; which players exist is the runtime's to know, as the
    /// catalog says.
    fn player_exists(&self, command: &Command) -> Result<(), Refusal> {
        let Command::Take {
            source: Some(Source::Player(id)),
            ..
        } = command
        else {
            return Ok(());
        };
        let players = self.players();
        if crate::player::player_index(id, players).is_some() {
            return Ok(());
        }
        Err(Refusal::rejected(
            "source",
            format!(
                "there is no player '{}' on this server; its players are {}",
                id,
                crate::player::player_list(players)
            ),
        ))
    }

    /// (goal 15) Leave every accepted controller command at `tap` for the
    /// MQTT publisher (`crate::mqtt`). Called once, before any session
    /// exists.
    pub fn publish_events_through(&self, tap: crate::mqtt::EventTap) {
        let _ = self.mqtt_events.set(tap);
    }

    /// The fanout, for a caller that wants to report on it.
    pub fn fanout(&self) -> &Arc<ControlFanout> {
        &self.fanout
    }

    /// The state message as it stands, at the build's own catalog version.
    pub fn encoded_state(&self) -> String {
        self.locked().zones.encode_state()
    }

    /// The event streams the event writer holds (`crate::events`).
    pub fn events(&self) -> &EventStreams {
        &self.events
    }

    /// Hold at most `ceiling` event streams (`--event-streams`). Called once,
    /// before the event writer exists.
    pub fn set_event_streams(&mut self, ceiling: usize) {
        self.events = EventStreams::new(ceiling);
    }

    /// The visualizer stream's tap, stamped on `timeline`: from now on a
    /// `GET /api/visualizer` stream subscribes to it, and a new frame wakes
    /// the event writer. Called once, after [`ControlState::set_event_streams`]
    /// and before any thread runs.
    pub fn set_lights(
        &self,
        lights: Arc<crate::lights::LightTap>,
        timeline: chorus_audio::MonotonicTimeline,
    ) {
        lights.connect(timeline, self.events.waker());
        let _ = self.lights.set(lights);
    }

    /// The conductor's half of the wake, taken once by the conductor thread.
    pub fn take_conductor_wake(&self) -> Option<Receiver<()>> {
        lock(&self.conductor_woken).take()
    }

    /// Tell the conductor something may have changed. Never blocks: a poke
    /// already waiting covers this one.
    pub fn wake_conductor(&self) {
        let _ = self.conductor_wake.try_send(());
    }

    /// Fan a new state out: to every subscriber, to the event writer that
    /// writes their streams, and to the conductor that carries it to the
    /// audio sessions.
    fn publish(&self, state: String) {
        self.fanout.broadcast(Arc::new(state));
        self.events.wake();
        self.wake_conductor();
    }

    /// Serve the room model on `slots` stream slots (`--slots S`, S > 0).
    ///
    /// Every group that needs a slot gets one, in the order its first room was
    /// configured; a group past the ceiling starts with its source set to
    /// `none` (no slot, its rooms hear silence) rather than the server
    /// refusing to start, and the groups that did are returned so the caller
    /// can say so. A person can give one a source again once a slot is free.
    pub fn serve_on_slots(&self, slots: usize) -> Vec<String> {
        let mut held = self.locked();
        let silenced: Vec<String> = SlotTable::needing(&held.zones)
            .into_iter()
            .skip(slots)
            .collect();
        for group in &silenced {
            let _ = held.zones.set_group_source(group, Source::None);
        }
        // At most `slots` groups need one now, so the plan fits.
        let table = SlotTable::new(slots)
            .plan(&held.zones)
            .unwrap_or_else(|_| SlotTable::new(slots));
        held.slots = Some(table);
        silenced
    }

    /// Every person's command applied since the last call, in order.
    pub fn take_applied(&self) -> Vec<Command> {
        std::mem::take(&mut *lock(&self.applied_commands))
    }

    /// Let the schedule runtime change the room model (`change` is one of
    /// its entry points over the zones), then plan the stream slots, persist
    /// when an effect asks to and fan the state out when anything changed.
    ///
    /// Unlike a person's command, a change the runtime makes is never refused
    /// for want of a slot (an alarm must not be lost to a full table): a
    /// group it would give a source with no slot free plays nothing instead
    /// (its source `none`), reported as a `Log` effect, the rule
    /// [`ControlState::serve_on_slots`] applies at start.
    pub fn runtime(&self, change: impl FnOnce(&mut Zones) -> Vec<Effect>) -> Vec<Effect> {
        let (mut effects, state) = {
            let mut held = self.locked();
            let before = held.zones.serial();
            let mut next = held.zones.clone();
            let mut effects = change(&mut next);
            if let Some(table) = &held.slots {
                let mut planned = table.plan(&next);
                // At most one group is silenced per pass, and there are
                // only so many groups.
                let mut guard = next.zones().len() + 1;
                while planned.is_err() && guard > 0 {
                    guard -= 1;
                    let Some(group) = SlotTable::needing(&next)
                        .into_iter()
                        .rev()
                        .find(|g| table.slot_of(g).is_none())
                    else {
                        break;
                    };
                    let _ = next.set_group_source(&group, Source::None);
                    effects.push(Effect::Log(format!(
                        "schedule group={} source=none outcome=no-free-slot slots={}",
                        group,
                        table.len()
                    )));
                    planned = table.plan(&next);
                }
                match planned {
                    Ok(t) => held.slots = Some(t),
                    // Nothing fits even so: leave the model as it was.
                    Err(_) => return effects,
                }
            }
            held.zones = next;
            if effects.contains(&Effect::Persist) {
                self.persist(&held.zones);
            }
            let changed = held.zones.serial() != before;
            (effects, changed.then(|| held.zones.encode_state()))
        };
        if let Some(state) = state {
            self.publish(state);
        }
        effects.retain(|e| *e != Effect::Persist);
        effects
    }

    /// The slot table, `slot=group` for every slot, for a status line; `None`
    /// in the one-stream shape.
    pub fn slots_report(&self) -> Option<String> {
        self.locked().slots.as_ref().map(SlotTable::report)
    }

    /// Say what civil time it is, which decides which quiet-hours windows are
    /// active (`Zones::set_civil_time`), and fan the result out when anything
    /// changed. The time is the caller's: this reads no clock. Returns whether
    /// anything changed.
    pub fn set_civil_time(&self, now: Option<CivilTime>) -> bool {
        let state = {
            let mut held = self.locked();
            if !held.zones.set_civil_time(now) {
                return false;
            }
            self.persist(&held.zones);
            held.zones.encode_state()
        };
        self.publish(state);
        true
    }

    /// Everything the conductor and a starting session need to know about the
    /// room model, read in one go under the lock.
    pub fn snapshot(&self) -> Snapshot {
        let held = self.locked();
        let zones = &held.zones;
        let rooms = zones
            .zones()
            .iter()
            .map(|z| RoomView {
                id: z.id.clone(),
                present: z.present.clone(),
                endpoints: z.endpoints.clone(),
                room_volume: room_volume_of(z),
                sounds: z
                    .endpoints
                    .iter()
                    .map(|e| (e.clone(), sound_of(z, e)))
                    .collect(),
                controller_state: controller_state_of(z, zones.now_playing(&z.group)),
                route: match &held.slots {
                    None => Some(0),
                    Some(table) => table.slot_of(&z.group),
                },
                av_trim_ms: z.av_trim_ms,
                voice_enabled: z.voice_enabled,
            })
            .collect();
        let slots = match &held.slots {
            None => Vec::new(),
            Some(table) => table
                .held()
                .iter()
                .map(|g| {
                    g.as_ref().map(|group| SlotGroup {
                        group: group.clone(),
                        source: zones.source(group),
                        rooms: zones
                            .zones()
                            .iter()
                            .filter(|z| &z.group == group)
                            .map(|z| z.id.clone())
                            .collect(),
                        low_latency: match zones.source(group) {
                            Source::LineIn(input) => zones
                                .autoplay_rules()
                                .iter()
                                .find(|r| r.input == input)
                                .is_none_or(|r| r.low_latency),
                            _ => false,
                        },
                    })
                })
                .collect(),
        };
        Snapshot { rooms, slots }
    }

    /// Where a session that just came up starts, and what its greeting says
    /// (docs/decisions/0074-*: `room_volume` before the first audio).
    pub fn session_start(&self, endpoint: &str, roles: u16, idle: usize) -> SessionStart {
        self.snapshot().start(endpoint, roles, idle)
    }

    fn persist(&self, zones: &Zones) {
        if let Some(path) = &self.state_file {
            if let Err(e) = persist::write_file(path, zones) {
                eprintln!(
                    "chorus-server: the zone state could not be persisted: {}. The change is in \
                     force in this process and will NOT survive a restart",
                    e
                );
            }
        }
    }

    /// The state message as it stands, at catalog version `version`: the v1
    /// shape for `?v=1` peers (docs/control-plane.md, "How the messages
    /// travel"), the v2 shape otherwise.
    pub fn encoded_state_at(&self, version: i64) -> String {
        self.locked().zones.encode_state_at(version)
    }

    /// The line a run prints about what the control plane did.
    pub fn report(&self) -> String {
        format!(
            "control applied={} refused={} turned_away={} {} {} controller-events \
             press_subscribers={} press_dropped_subscribers={} press_dropped_events={} \
             visualizer light_subscribers={} light_frames={} light_superseded={}",
            self.applied.load(Ordering::Relaxed),
            self.refused.load(Ordering::Relaxed),
            self.turned_away.load(Ordering::Relaxed),
            self.fanout.report(),
            self.events.report(),
            // Its own keys: a reader of the state fanout's counters
            // (`tools/house-soak/report.py`) keeps reading those.
            self.presses.subscribers(),
            self.presses.dropped_subscribers(),
            self.presses.dropped_messages(),
            // The visualizer streams: nothing is dropped there, a frame not
            // sent was superseded by a later one.
            self.lights.get().map_or(0, |l| l.subscribers()),
            self.lights.get().map_or(0, |l| l.sent()),
            self.lights.get().map_or(0, |l| l.superseded())
        )
    }

    fn locked(&self) -> std::sync::MutexGuard<'_, Held> {
        lock(&self.held)
    }

    /// Apply `change` to a copy of the room model, plan the stream slots over
    /// the copy, and install both only when the plan fits: a change that
    /// would need a slot more than the server has is refused by name and
    /// leaves everything as it was (docs: the stream slots' ADR).
    fn commit(
        held: &mut Held,
        change: impl FnOnce(&mut Zones) -> Result<(), Refusal>,
    ) -> Result<(), Refusal> {
        let mut next = held.zones.clone();
        change(&mut next)?;
        if let Some(table) = &held.slots {
            held.slots = Some(table.plan(&next)?);
        }
        held.zones = next;
        Ok(())
    }

    /// Apply one control message, persist the result and fan it out.
    ///
    /// The state is persisted BEFORE it is fanned out, so that a subscriber
    /// which has been told a change happened cannot be told something the disk
    /// would contradict after a restart.
    ///
    /// A refusal is answered at the catalog version the message was written
    /// at (a v1 peer's refusal says `"v":1`); a change is answered with the
    /// state every subscriber is sent, which is the build's own (v2): one
    /// state, the same bytes for everybody. A v2 state's `zones` carry every
    /// field a v1 state's do, under the same names.
    pub fn apply(&self, text: &str) -> Result<String, Refusal> {
        let (version, command) = decode_message(text)?;
        self.apply_decoded(version, command)
    }

    /// (goal 17) Apply a command the server built itself, with everything
    /// [`ControlState::apply`] does after decoding. This is how the Soloist
    /// receiver manager gives a group its receiver as a source: the decoder
    /// refuses `soloist:` in a client's `take` by name, because a receiver's
    /// audio follows the Spotify app and not a command.
    pub fn apply_command(&self, command: Command) -> Result<String, Refusal> {
        self.apply_decoded(chorus_control::catalog::CATALOG_VERSION, command)
    }

    fn apply_decoded(&self, version: i64, command: Command) -> Result<String, Refusal> {
        // (goal 18) An announcement is carried out by the announcer, which
        // needs a player before the room model changes: it commits through
        // `announce_begin` once it has one.
        if let Command::Announce {
            target,
            url,
            volume,
        } = &command
        {
            let Some(announcer) = self.announcer.get() else {
                self.announce_check(target, url)
                    .map_err(|r| r.at(version))?;
                return Err(Refusal::rejected(
                    "t",
                    "no-players: this server has nothing to play an announcement with".to_string(),
                )
                .at(version));
            };
            return announcer
                .announce(self, target, url, *volume)
                .map_err(|r| r.at(version));
        }
        // (voice, P8) A run is the voice path's, not the room model's: the
        // model agrees or refuses, and nothing of the run is a state.
        match &command {
            Command::VoiceStart { zone } => {
                return self.voice_start(zone).map_err(|r| r.at(version));
            }
            Command::VoiceStop { zone } => {
                let state = {
                    let held = self.locked();
                    // A room that is muted or switched off has no run and
                    // is not an error to stop; a room that does not exist
                    // is, in the words every command naming it is refused.
                    if held.zones.zone(zone).is_none() {
                        held.zones
                            .voice_start_check(zone)
                            .map_err(|r| r.at(version))?;
                    }
                    held.zones.encode_state()
                };
                if let Some(voice) = self.voice.get() {
                    voice.stop(zone);
                }
                self.applied.fetch_add(1, Ordering::Relaxed);
                return Ok(state);
            }
            _ => {}
        }
        let state = {
            let mut held = self.locked();
            Self::commit(&mut held, |zones| {
                self.player_exists(&command)?;
                zones.apply(&command)?;
                // (goal 14) Forgetting a speaker forgets its pin too, after
                // the room model has agreed and before anything is installed:
                // a pin file that could not be rewritten refuses the command
                // whole, so the record and the pin never disagree.
                if let Command::SpeakerForget { speaker } = &command {
                    self.forget_pin(speaker)?;
                }
                // (goal 14) The firmware commands' server half, inside the
                // same commit: a transfer that cannot start (the guard, the
                // file changed since it was verified) refuses the command
                // whole and nothing is installed or marked.
                self.firmware_effects(zones, &command)?;
                self.soloist_effects(zones, &command)?;
                Ok(())
            })
            .map_err(|r| r.at(version))?;
            lock(&self.applied_commands).push(command.clone());
            // A failure to persist is reported and not swallowed, and not a
            // reason to refuse the command either: the change IS in force in
            // this process, and saying it was refused would be a lie in the
            // other direction.
            self.persist(&held.zones);
            held.zones.encode_state()
        };
        self.applied.fetch_add(1, Ordering::Relaxed);
        self.publish(state.clone());
        Ok(state)
    }

    /// Apply what an endpoint's button asked for (its `controller_command`)
    /// to the zone that endpoint is attached to, through the same checks,
    /// persistence and fanout as `POST /api/command` (the controller role is
    /// not a second control plane: docs/protocol.md, K65, K81, I10).
    ///
    /// The zone is the one whose present endpoints (else whose membership)
    /// names `endpoint`, the session's authenticated id; an endpoint attached
    /// to no zone is refused by name. What comes back carries the
    /// `controller_state` to answer the endpoint with.
    pub fn controller(
        &self,
        endpoint: &str,
        command: &ControllerCommand,
    ) -> Result<ControllerApplied, Refusal> {
        let result = {
            let mut held = self.locked();
            let zone = held
                .zones
                .zones()
                .iter()
                .find(|z| z.present.iter().any(|e| e == endpoint))
                .or_else(|| {
                    held.zones
                        .zones()
                        .iter()
                        .find(|z| z.endpoints.iter().any(|e| e == endpoint))
                })
                .map(|z| z.id.clone())
                .ok_or_else(|| {
                    Refusal::rejected(
                        "endpoint",
                        format!(
                            "endpoint '{}' sent a controller command and is attached to no zone",
                            endpoint
                        ),
                    )
                });
            zone.and_then(|zone| {
                let action = translate(&held.zones, &zone, command)?;
                let mut changed = None;
                if let ControllerAction::Apply(change) = &action {
                    Self::commit(&mut held, |zones| zones.apply(change))?;
                    lock(&self.applied_commands).push(change.clone());
                    self.persist(&held.zones);
                    changed = Some(held.zones.encode_state());
                }
                let z = held.zones.zone(&zone).expect("translate found the zone");
                let state = controller_state_of(z, held.zones.now_playing(&z.group));
                Ok((
                    ControllerApplied {
                        zone,
                        action,
                        state,
                    },
                    changed,
                ))
            })
        };
        let (applied, changed) = match result {
            Ok(v) => v,
            Err(refusal) => {
                self.refused.fetch_add(1, Ordering::Relaxed);
                return Err(refusal);
            }
        };
        let outcome = match &applied.action {
            ControllerAction::Apply(_) => "applied",
            ControllerAction::Transport(_) => "waits-for-an-input",
        };
        // (goal 15) The MQTT publisher's event, left before the state is
        // fanned out so the publisher is woken with it already waiting.
        // Never blocks: this runs on a client reader thread.
        if let Some(tap) = self.mqtt_events.get() {
            tap.offer(
                endpoint,
                &applied.zone,
                command.command.name(),
                command.value,
                &command.target,
                outcome,
            );
        }
        // The same press to every `GET /api/controller-events` stream, as its
        // own message and before the state it may have changed: an event is
        // not a state change and rides in no state message. Never blocks (a
        // bounded `try_send` per subscriber), and with nobody attached the
        // message is built and dropped, kept for no one.
        self.presses.broadcast(Arc::new(
            ControllerEvent {
                endpoint: endpoint.to_string(),
                zone: applied.zone.clone(),
                command: command.command.name().to_string(),
                value: i64::from(command.value),
                target: command.target.clone(),
                outcome: outcome.to_string(),
            }
            .encode(),
        ));
        self.events.wake();
        if let Some(state) = changed {
            self.applied.fetch_add(1, Ordering::Relaxed);
            self.publish(state);
        }
        Ok(applied)
    }

    /// (voice, P8) The voice path, and the one address a run's audio is
    /// served to (`--voice-integration`; `None`: every `voice_start` is
    /// refused by name). From now on the event writer makes the path's pass
    /// and a run opening or ending wakes the conductor. Called once, at
    /// start, after [`ControlState::set_event_streams`] and before any
    /// thread runs.
    pub fn voice_through(&self, voice: Arc<crate::voice::Voice>, integration: Option<IpAddr>) {
        voice.connect(self.events.waker(), self.conductor_wake.clone());
        let _ = self.voice.set(voice);
        if let Some(address) = integration {
            let _ = self.voice_integration.set(address.to_canonical());
        }
    }

    /// (voice, P8) Open a run in `zone` and answer with it: the `voice_run`
    /// message, which is the only place the run's identifier is ever
    /// written. Refused by name when the room has voice switched off
    /// (`voice-disabled`), when no microphone of it is live (`mic-muted`),
    /// and when this server could serve the run to nobody
    /// (`no-voice-integration`).
    fn voice_start(&self, zone: &str) -> Result<String, Refusal> {
        let endpoints = {
            let held = self.locked();
            held.zones.voice_start_check(zone)?;
            held.zones
                .zone(zone)
                .map(|z| z.present.clone())
                .unwrap_or_default()
        };
        let (Some(voice), Some(_)) = (self.voice.get(), self.voice_integration.get()) else {
            return Err(Refusal::rejected(
                "t",
                "no-voice-integration: this server was started without --voice-integration, \
                 so no address may read a voice run's audio and none is opened"
                    .to_string(),
            ));
        };
        let random = crate::session::random_32().map_err(|e| {
            Refusal::rejected(
                "t",
                format!("no-voice-run: a run identifier could not be made ({})", e),
            )
        })?;
        let run: String = random[..16].iter().map(|b| format!("{:02x}", b)).collect();
        let Some(limit) = voice.start(zone, &endpoints, run.clone(), Instant::now()) else {
            // The gate closed between the room model's answer and here.
            return Err(Refusal::rejected(
                "zone",
                format!(
                    "mic-muted: no microphone in room '{}' reports its gate live (the mute \
                     switch is the speaker's own, and no command opens it)",
                    zone
                ),
            ));
        };
        self.applied.fetch_add(1, Ordering::Relaxed);
        // The event writer prints the run's line and watches its limit.
        self.events.wake();
        Ok(VoiceRun {
            zone: zone.to_string(),
            run,
            limit_ms: i64::try_from(limit.as_millis()).unwrap_or(i64::MAX),
        }
        .encode())
    }

    /// (voice, P8) The voice path's pass, made by the event writer on every
    /// one of its own (`crate::events`): the wake word is heard, each one is
    /// fanned out as a `voice_wake` to the `GET /api/voice-events` streams,
    /// runs past their limit end, and the path's status lines are printed.
    /// Returns when the next open run reaches its limit.
    pub fn voice_pass(&self, now: Instant) -> Option<Instant> {
        let voice = self.voice.get()?;
        let pass = voice.pass(now);
        for line in &pass.lines {
            println!("chorus-server: {}", line);
        }
        for wake in pass.wakes {
            // Built and dropped when nobody is attached: kept for no one.
            self.voice_events.broadcast(Arc::new(
                VoiceWake {
                    zone: wake.room,
                    phrase: wake.phrase,
                }
                .encode(),
            ));
        }
        pass.due
    }

    /// Mark an endpoint as gone and fan out the result.
    fn endpoint_left(&self, endpoint: &str) {
        let state = {
            let mut held = self.locked();
            if !held.zones.endpoint_left(endpoint) {
                return;
            }
            if let Some(path) = &self.state_file {
                let _ = persist::write_file(path, &held.zones);
            }
            held.zones.encode_state()
        };
        self.publish(state);
    }
}

/// The speaker registry's server half (goal 14): what the session layer says
/// about adoption, presence and refused key changes, each fanned out like any
/// other change. `chorus_control::speakers` holds the records; the pins stay
/// the session layer's ([`Adoptions`]).
impl ControlState {
    /// Adopt through `adoptions` from now on, and make a speaker record for
    /// every id it already has pinned and this state does not list (a server
    /// upgraded from before goal 14, or a state file that was removed).
    /// Called once, at start, before any session. Returns a line for each id
    /// that could not be listed.
    pub fn adopt_through(&self, adoptions: Arc<Adoptions>) -> Vec<String> {
        let mut unlisted = Vec::new();
        let state = {
            let mut held = self.locked();
            let before = held.zones.serial();
            let mut created = false;
            for (id, key) in adoptions.pins() {
                match held.zones.speaker_adopted(&id, &key) {
                    Ok(made) => created |= made,
                    Err(why) => unlisted.push(not_listed_line(&id, &why)),
                }
            }
            if created {
                self.persist(&held.zones);
            }
            (held.zones.serial() != before).then(|| held.zones.encode_state())
        };
        let _ = self.adoptions.set(adoptions);
        if let Some(state) = state {
            self.publish(state);
        }
        unlisted
    }

    /// Remove `speaker`'s pin, so its next session is adopted afresh.
    fn forget_pin(&self, speaker: &str) -> Result<(), Refusal> {
        let Some(adoptions) = self.adoptions.get() else {
            return Ok(());
        };
        adoptions.forget(speaker).map(|_| ()).map_err(|e| {
            Refusal::rejected(
                "speaker",
                format!(
                    "speaker '{}' was not forgotten: {}. Nothing changed",
                    speaker, e
                ),
            )
        })
    }

    /// A session of `id` is up: its record is made if this is the first the
    /// server has seen of it (auto-adoption: the handshake pinned its key,
    /// and this lists it, unnamed and in no room), it is marked present, and
    /// what its `hello` said is kept. Returns the lines to log: one when a
    /// record was created, or one naming why the id is not listed.
    pub fn speaker_session_up(
        &self,
        id: &str,
        key: &str,
        software: &str,
        session_roles: u16,
    ) -> Vec<String> {
        let names: Vec<String> = roles::NAMES
            .iter()
            .filter(|(bit, _)| session_roles & bit != 0)
            .map(|(_, name)| name.to_string())
            .collect();
        let mut lines = Vec::new();
        let state = {
            let mut held = self.locked();
            let before = held.zones.serial();
            match held.zones.speaker_adopted(id, key) {
                Ok(true) => {
                    self.persist(&held.zones);
                    let name = held
                        .zones
                        .speakers()
                        .get(id)
                        .map(|s| s.name.clone())
                        .unwrap_or_default();
                    lines.push(format!(
                        "speaker listed id={} name=\"{}\" named=0",
                        id, name
                    ));
                }
                Ok(false) => {}
                Err(why) => lines.push(not_listed_line(id, &why)),
            }
            held.zones.speaker_session_up(id, software, &names);
            (held.zones.serial() != before).then(|| held.zones.encode_state())
        };
        if let Some(state) = state {
            self.publish(state);
        }
        lines
    }

    /// A session of `id` has ended.
    pub fn speaker_session_down(&self, id: &str) {
        let (changed, present) = {
            let mut held = self.locked();
            let changed = held
                .zones
                .speaker_session_down(id)
                .then(|| held.zones.encode_state());
            let present = held
                .zones
                .speakers()
                .get(id)
                .is_some_and(|s| s.now.present());
            (changed, present)
        };
        // Its last session took its telemetry with it (`GET /metrics`
        // exposes a report only "this session").
        if !present {
            self.telemetry.forget(id);
        }
        if let Some(state) = changed {
            self.publish(state);
        }
    }

    /// A handshake under an adopted id offered another key and was refused:
    /// surfaced in the state message's `key_changes`, where the app and Home
    /// Assistant read it, and not only in the log.
    pub fn speaker_key_changed(&self, id: &str, pinned: &str, offered: &str) {
        let state = {
            let mut held = self.locked();
            let change = KeyChange {
                id: id.to_string(),
                pinned: pinned.to_string(),
                offered: offered.to_string(),
            };
            if !held.zones.speaker_key_changed(change) {
                return;
            }
            held.zones.encode_state()
        };
        self.publish(state);
    }

    /// (voice, P8) Speaker `id` said what its mic gate is (`mic_state`): the
    /// room's `mic_muted` follows, and the state is fanned out when it
    /// changed. Never persisted; no command reaches this.
    pub fn speaker_mic_gate(&self, id: &str, live: bool) {
        let state = {
            let mut held = self.locked();
            if !held.zones.speaker_mic_gate(id, live) {
                return;
            }
            held.zones.encode_state()
        };
        self.publish(state);
    }

    /// (voice, P8) The room `endpoint` is in right now (the rule
    /// [`Snapshot::room_of`] uses) and whether that room has voice switched
    /// on; `None` for an endpoint in no room. Read for every `mic_audio`
    /// frame, so a room switched off drops its very next frame.
    pub fn voice_room(&self, endpoint: &str) -> Option<(String, bool)> {
        let held = self.locked();
        let zones = held.zones.zones();
        zones
            .iter()
            .find(|z| z.present.iter().any(|e| e == endpoint))
            .or_else(|| {
                zones
                    .iter()
                    .find(|z| z.endpoints.iter().any(|e| e == endpoint))
            })
            .map(|z| (z.id.clone(), z.voice_enabled))
    }

    /// Change what is known about speaker `id` right now (a per-speaker
    /// runtime fact, [`SpeakerNow`]) and fan the state out when it changed.
    /// Never persisted. Returns whether anything changed.
    pub fn speaker_now(&self, id: &str, change: impl FnOnce(&mut SpeakerNow)) -> bool {
        let state = {
            let mut held = self.locked();
            if !held.zones.speaker_now(id, change) {
                return false;
            }
            held.zones.encode_state()
        };
        self.publish(state);
        true
    }
}

/// The exporter (goal 15): what a speaker's `telemetry` leaves behind and
/// the scrape made of it. Neither changes the room model nor publishes a
/// state: a scrape reads, and a report is kept beside the model.
impl ControlState {
    /// Speaker `id` sent a `telemetry`: keep it as its latest, stamped with
    /// the server's monotonic clock. A report from an id that is not a
    /// listed speaker is not kept, so the store is bounded as the list is.
    pub fn telemetry_reported(&self, id: &str, report: &chorus_protocol::v2::Telemetry) {
        let listed = self.locked().zones.speakers().get(id).is_some();
        if listed {
            self.telemetry.keep(id, *report, Instant::now());
        }
    }

    /// The text `GET /metrics` answers with (`crate::metrics::render`).
    pub fn metrics(&self) -> String {
        let speakers: Vec<crate::metrics::SpeakerRow> = self
            .locked()
            .zones
            .speakers()
            .all()
            .iter()
            .map(|s| crate::metrics::SpeakerRow {
                id: s.id.clone(),
                name: s.name.clone(),
                room: s.room.clone().unwrap_or_default(),
                connected: s.now.present(),
                // What its `firmware_status` says it runs, else its
                // `hello`'s software string; empty until it has said either.
                version: s
                    .now
                    .firmware
                    .as_ref()
                    .map(|f| f.version.clone())
                    .filter(|v| !v.is_empty())
                    .unwrap_or_else(|| s.now.software.clone()),
            })
            .collect();
        let mut text = crate::metrics::render(
            env!("CARGO_PKG_VERSION"),
            &speakers,
            &self.telemetry.snapshot(),
            Instant::now(),
        );
        // (goal 17) The Soloist receivers, on a server that runs them.
        if let Some(link) = self.soloist.get() {
            text.push_str(&link.metrics());
        }
        text
    }
}

/// Firmware (goal 14): the server half of `firmware_install`,
/// `firmware_cancel` and `firmware_rescan`, and what a session's
/// `firmware_status` says, each fanned out like any other change. The facts
/// are `chorus_control::firmware`'s; the files and the transfers are
/// [`Firmware`]'s.
impl ControlState {
    /// Keep the firmware images and installs in `firmware` from now on, and
    /// list what its directory holds. Called once, at start, before any
    /// session. Nothing is offered to anybody by this.
    pub fn firmware_through(&self, firmware: Arc<Firmware>) {
        let images = firmware.scan();
        let state = {
            let mut held = self.locked();
            held.zones
                .set_firmware_images(images)
                .then(|| held.zones.encode_state())
        };
        let _ = self.firmware.set(firmware);
        if let Some(state) = state {
            self.publish(state);
        }
    }

    /// The firmware command's effect beyond the room model, on the copy the
    /// commit is building. `zones` has already applied the command.
    fn firmware_effects(&self, zones: &mut Zones, command: &Command) -> Result<(), Refusal> {
        let Some(firmware) = self.firmware.get() else {
            return Ok(());
        };
        match command {
            Command::FirmwareInstall { speaker, image, .. } => {
                // The model marked exactly these `requested`: the speakers
                // whose install now holds this image and no transfer yet.
                let targets: Vec<String> = zones
                    .speakers()
                    .all()
                    .iter()
                    .filter(|s| speaker.as_deref().is_none_or(|id| id == s.id))
                    .filter(|s| {
                        s.now.firmware.as_ref().is_some_and(|f| {
                            f.state == chorus_control::firmware::REQUESTED
                                && f.install
                                    .as_ref()
                                    .is_some_and(|i| i.image == *image && i.transfer == 0)
                        })
                    })
                    .map(|s| s.id.clone())
                    .collect();
                let staged = zones
                    .firmware_images()
                    .and_then(|images| images.iter().find(|i| i.name == *image))
                    .cloned()
                    .expect("the model accepted the install, so the image is staged");
                let field = if speaker.is_some() { "speaker" } else { "all" };
                for (id, transfer) in firmware.start(&targets, &staged, field)? {
                    zones.speaker_now(&id, |now| {
                        if let Some(install) =
                            now.firmware.as_mut().and_then(|f| f.install.as_mut())
                        {
                            install.transfer = transfer;
                        }
                    });
                }
            }
            Command::FirmwareCancel { speaker } => {
                firmware.cancel(speaker);
            }
            Command::FirmwareRescan => {
                zones.set_firmware_images(firmware.scan());
            }
            _ => {}
        }
        Ok(())
    }

    /// A speaker said what it runs and where its install stands (its
    /// `firmware_status`, translated by [`Firmware::status`]).
    pub fn firmware_reported(&self, id: &str, report: &chorus_control::firmware::Report) {
        self.speaker_now(id, |now| {
            chorus_control::firmware::SpeakerFirmware::absorb(&mut now.firmware, report)
        });
    }

    /// The session a speaker's install travelled in ended before the image
    /// was verified: the install is `interrupted`, and is not resumed.
    pub fn firmware_interrupted(&self, id: &str) {
        self.speaker_now(id, |now| {
            if let Some(fw) = &mut now.firmware {
                fw.ended(chorus_control::firmware::INTERRUPTED, "session_ended");
            }
        });
    }
}

fn not_listed_line(id: &str, why: &NotListed) -> String {
    format!(
        "speaker not listed id={} reason={} detail=\"{}\"",
        id,
        why.name(),
        match why {
            NotListed::NotAnIdentifier =>
                "its key is pinned and it plays, but no control command can name this id"
                    .to_string(),
            NotListed::Full => format!(
                "this server lists at most {} speakers; forget one (speaker_forget)",
                chorus_control::speakers::MAX_SPEAKERS
            ),
        }
    )
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// What a room's players are told on the audio wire: the gain to play at
/// (the volume, 0 when muted), never above the room's effective limit even
/// though the model already clamps it (the server's clamp is the first line,
/// ADR 0074), and that limit; at once (`ramp_ms` 0, ASSUMED: a person's
/// change is applied as it is made, with no de-click ramp).
pub fn room_volume_of(zone: &Zone) -> RoomVolume {
    let limit = zone.effective_limit().thousandths().min(1_000);
    RoomVolume {
        gain: zone.gain().thousandths().min(limit) as u16,
        limit: limit as u16,
        ramp_ms: 0,
    }
}

/// What one player of a room is told on the audio wire about the room's
/// sound (docs/protocol.md, "0x39 sound"; goal 12): the room's tone, flags,
/// bass management and correction filters, all the catalog's values in the
/// wire's own units (no conversion: both are whole dB, Hz, hundredths of a dB
/// and thousandths), plus `endpoint`'s channel position in the room's bonded
/// set (0 when it is in none) and whether the set has a sub. Every member of
/// a set gets the room's whole stream and does its own bass management from
/// these two.
///
/// Goal 13 adds the theater block: the room's `tv_upmix`, and `fold`, what
/// the room's bonded set lacks (no centre member, no surround pair), so a
/// front member folds those channels of a 5.1 stream into its own (ITU-R
/// BS.775-4 Table 2, `docs/dsp.md` "The theater maps"). Only the FL and FR
/// members are told a fold (no other role folds); a room with no set folds
/// nothing: its endpoint plays the stream as it is.
pub fn sound_of(zone: &Zone, endpoint: &str) -> Sound {
    let s = &zone.sound;
    let mut flags = 0u8;
    for (on, bit) in [
        (s.loudness, sound_flags::LOUDNESS),
        (s.night, sound_flags::NIGHT),
        (s.speech, sound_flags::SPEECH),
        (zone.room_eq.enabled, sound_flags::ROOM_EQ),
        (
            zone.bass.sub_polarity == Polarity::Inverted,
            sound_flags::SUB_INVERTED,
        ),
    ] {
        if on {
            flags |= bit;
        }
    }
    Sound {
        bass_db: s.bass,
        treble_db: s.treble,
        flags,
        role: zone.role_of(endpoint).map_or(0, |r| r.position()),
        sub_present: zone.has_sub(),
        crossover_hz: zone.bass.crossover_hz,
        sub_level_cdb: zone.bass.sub_level_cdb,
        filters: zone
            .room_eq
            .filters
            .iter()
            .map(|f| SoundFilter {
                freq_hz: f.freq_hz,
                gain_cdb: f.gain_cdb,
                q_milli: f.q_milli,
            })
            .collect(),
        tv_upmix: s.tv_upmix.wire(),
        // Only a front member folds; every other member is told 0, so what
        // it is sent is what goal 12 sent it.
        fold: match zone.role_of(endpoint) {
            Some(Role::Fl | Role::Fr) => {
                fold_of(&zone.bond.iter().map(|m| m.role).collect::<Vec<_>>())
            }
            _ => 0,
        },
    }
}

/// `sound`'s `fold` bits for a bonded set of `roles`: what it lacks. No set,
/// no fold.
pub fn fold_of(roles: &[Role]) -> u8 {
    if roles.is_empty() {
        return 0;
    }
    let mut fold = 0;
    if !roles.contains(&Role::Fc) {
        fold |= sound_fold::CENTRE;
    }
    if !roles
        .iter()
        .any(|r| matches!(r, Role::Bl | Role::Br | Role::Sl | Role::Sr))
    {
        fold |= sound_fold::SURROUND;
    }
    fold
}

/// What a room's controllers are shown (docs/protocol.md, "0x33 controller
/// state"): its volume in points, its mute and its group.
///
/// `playing` is what the room's group's player source is playing, when the
/// runtime has said (goal 16): a room whose group is paused shows paused.
/// Every other room is playing, because the server streams to every attached
/// endpoint; a player that is buffering is shown as playing, the wire having
/// no third word (`docs/protocol.md`).
pub fn controller_state_of(zone: &Zone, playing: Option<&NowPlaying>) -> ControllerState {
    let points =
        (i64::from(zone.volume.thousandths()) + THOUSANDTHS_PER_POINT / 2) / THOUSANDTHS_PER_POINT;
    ControllerState {
        volume: points.clamp(0, 100) as u8,
        muted: zone.muted,
        playback: match playing.map(|p| p.state) {
            Some(PlayState::Paused) => Playback::Paused,
            _ => Playback::Playing,
        },
        group: zone.group.clone(),
    }
}

/// The group a stream slot serves, as the conductor routes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotGroup {
    /// The group.
    pub group: String,
    /// What it plays.
    pub source: Source,
    /// Its rooms, in configured order.
    pub rooms: Vec<String>,
    /// (goal 13) For a line-in source: whether it may play in low-latency
    /// mode. An input with an autoplay rule follows the rule's `low_latency`;
    /// one without a rule may (the rule's default, ASSUMED for a command).
    pub low_latency: bool,
}

/// One room as the conductor routes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomView {
    /// The room's id.
    pub id: String,
    /// Its endpoints attached now.
    pub present: Vec<String>,
    /// Every endpoint it has had.
    pub endpoints: Vec<String>,
    /// What its players are told.
    pub room_volume: RoomVolume,
    /// What each of its endpoints is told about the room's sound, by
    /// endpoint: the same room values, each with its own role.
    pub sounds: Vec<(String, Sound)>,
    /// What its controllers are shown.
    pub controller_state: ControllerState,
    /// The fanout its group is served on: the one stream (0) in the
    /// one-stream shape, its group's slot in the slot shape, `None` when its
    /// group has no slot (source `none`).
    pub route: Option<usize>,
    /// (goal 13) Its A/V trim, ms: positive delays the audio; the TV relay
    /// moves its stamps by it (`crate::tvrelay`).
    pub av_trim_ms: i16,
    /// (voice, P8) Whether the room has voice switched on: what its voice
    /// endpoints are told as `voice_control.uplink`.
    pub voice_enabled: bool,
}

/// The room model, read once, as the conductor plans over it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Snapshot {
    /// Every room.
    pub rooms: Vec<RoomView>,
    /// The group each stream slot serves (`None`: a free slot); empty in
    /// the one-stream shape.
    pub slots: Vec<Option<SlotGroup>>,
}

impl RoomView {
    /// The `sound` for one of this room's endpoints (role 0 for one the
    /// membership does not name, which a present endpoint always is in).
    pub fn sound_for(&self, endpoint: &str) -> Option<&Sound> {
        self.sounds
            .iter()
            .find(|(e, _)| e == endpoint)
            .map(|(_, s)| s)
    }
}

impl Snapshot {
    /// The room an endpoint plays in: the one it is attached to now, else the
    /// one whose membership names it (the rule `ControlState::controller`
    /// uses, so a button and the audio agree on the room).
    pub fn room_of(&self, endpoint: &str) -> Option<&RoomView> {
        self.rooms
            .iter()
            .find(|r| r.present.iter().any(|e| e == endpoint))
            .or_else(|| {
                self.rooms
                    .iter()
                    .find(|r| r.endpoints.iter().any(|e| e == endpoint))
            })
    }

    /// Where a session of `endpoint` with `roles` belongs, and what it is
    /// told. A session in no room is routed to `idle` and told nothing: there
    /// is no room limit to send it, and in the slot shape it hears silence.
    pub fn start(&self, endpoint: &str, session_roles: u16, idle: usize) -> SessionStart {
        let Some(room) = self.room_of(endpoint) else {
            return SessionStart {
                route: idle,
                ..SessionStart::default()
            };
        };
        SessionStart {
            route: room.route.unwrap_or(idle),
            room_volume: (session_roles & roles::PLAYER != 0).then_some(room.room_volume),
            sound: if session_roles & roles::PLAYER != 0 {
                room.sound_for(endpoint).cloned()
            } else {
                None
            },
            controller_state: (session_roles & roles::CONTROLLER != 0)
                .then(|| room.controller_state.clone()),
            ..SessionStart::default()
        }
    }
}

/// What one controller command did, from [`ControlState::controller`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControllerApplied {
    /// The zone the endpoint is attached to.
    pub zone: String,
    /// What the command became: a change applied, or a transport request
    /// for the room's input.
    pub action: ControllerAction,
    /// The `controller_state` for the endpoint, after the change.
    pub state: ControllerState,
}

/// One control worker's slot.
struct Slot {
    to_worker: SyncSender<TcpStream>,
}

/// The control channel's fixed thread pool and the listener it serves.
pub struct ControlPlane {
    listener: TcpListener,
    address: String,
    state: Arc<ControlState>,
    slots: Vec<Slot>,
    free: Receiver<usize>,
}

impl ControlPlane {
    /// Bind the configured address.
    ///
    /// Done before any thread is created and before the audio listener is
    /// bound, so a server that cannot be controlled never reaches the point of
    /// serving audio.
    pub fn bind(address: &str, state: Arc<ControlState>) -> Result<ControlPlane, ControlRefused> {
        let listener = TcpListener::bind(address).map_err(|cause| ControlRefused::Bind {
            address: address.to_string(),
            cause,
        })?;
        let bound = listener
            .local_addr()
            .map(|a| a.to_string())
            .unwrap_or_else(|_| address.to_string());
        Ok(ControlPlane {
            listener,
            address: bound,
            state,
            slots: Vec::new(),
            free: mpsc::channel().1,
        })
    }

    /// The address actually bound, which is the configured one with any
    /// ephemeral port resolved.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// The shared state.
    pub fn state(&self) -> &Arc<ControlState> {
        &self.state
    }

    /// Create every worker thread this plane will ever run.
    ///
    /// Each registers itself and sends one unit down `ready`, exactly as
    /// [`crate::clients::ClientPool`] does, so the caller can wait for the
    /// whole population before taking a scheduling report of it.
    pub fn spawn_workers(
        &mut self,
        workers: usize,
        registry: Arc<ThreadRegistry>,
        ready: Sender<()>,
    ) {
        let (free_tx, free) = mpsc::channel::<usize>();
        for index in 0..workers {
            let (to_worker, jobs) = mpsc::sync_channel::<TcpStream>(1);
            let registry = Arc::clone(&registry);
            let state = Arc::clone(&self.state);
            let ready = ready.clone();
            let returning = free_tx.clone();
            thread::spawn(move || {
                register_ordinary_thread(&format!("control-worker-{}", index), &registry);
                if ready.send(()).is_err() {
                    return;
                }
                drop(ready);
                for connection in jobs {
                    serve_connection(connection, &state);
                    if returning.send(index).is_err() {
                        return;
                    }
                }
            });
            self.slots.push(Slot { to_worker });
            if free_tx.send(index).is_err() {
                break;
            }
        }
        self.free = free;
    }

    /// How many worker threads this plane created.
    pub fn threads(&self) -> usize {
        self.slots.len()
    }

    /// Run the accept loop. Called on a thread the caller created before its
    /// scheduling report; this function creates none.
    pub fn accept_loop(self, keep: Arc<AtomicBool>) {
        let ControlPlane {
            listener,
            state,
            slots,
            free,
            ..
        } = self;
        while keep.load(Ordering::SeqCst) {
            let (connection, peer) = match listener.accept() {
                Ok(v) => v,
                Err(_) => return,
            };
            let _ = connection.set_nodelay(true);
            let index = match free.try_recv() {
                Ok(index) => index,
                Err(_) => {
                    state.turned_away.fetch_add(1, Ordering::Relaxed);
                    refuse_busy(connection, peer, slots.len());
                    continue;
                }
            };
            if slots[index].to_worker.send(connection).is_err() {
                return;
            }
        }
    }
}

/// Turn a connection away because every worker is busy.
///
/// The same argument `crates/server/src/clients.rs` makes about audio clients:
/// a server that grows a thread per connection has no bound on threads at all,
/// so the connection is answered honestly and closed.
fn refuse_busy(mut connection: TcpStream, peer: SocketAddr, workers: usize) {
    let body = format!(
        "{{\"v\":1,\"t\":\"error\",\"field\":\"\",\"detail\":\"every one of this server's {} \
         control workers is busy; try again\"}}",
        workers
    );
    let _ = write!(
        connection,
        "HTTP/1.1 503 Service Unavailable\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = connection.flush();
    eprintln!(
        "chorus-server: control connection refused peer={} reason=no-free-control-worker \
         workers={}",
        peer, workers
    );
}

/// One HTTP request, and the response to it.
struct Request {
    method: String,
    path: String,
    /// The `Content-Type` header, as sent, if there was one.
    content_type: Option<String>,
    /// The `Origin` header, as sent, if there was one.
    origin: Option<String>,
    /// The `Host` header, as sent, if there was one.
    host: Option<String>,
    body: String,
}

/// Why a request could not be read, each with the answer it gets.
#[derive(Debug, PartialEq, Eq)]
enum Unreadable {
    /// Not HTTP this server can parse, or the peer went before finishing.
    Malformed,
    /// The request line and headers passed [`MAX_REQUEST_BYTES`].
    HeadTooLarge,
    /// The body would have taken the request past [`MAX_REQUEST_BYTES`].
    BodyTooLarge,
    /// The request was not complete by [`REQUEST_DEADLINE`].
    TimedOut,
}

impl Unreadable {
    fn status(&self) -> &'static str {
        match self {
            Unreadable::Malformed => "400 Bad Request",
            Unreadable::HeadTooLarge => "431 Request Header Fields Too Large",
            Unreadable::BodyTooLarge => "413 Content Too Large",
            Unreadable::TimedOut => "408 Request Timeout",
        }
    }

    fn detail(&self) -> String {
        match self {
            Unreadable::Malformed => "this is not an HTTP request this server can read".to_string(),
            Unreadable::HeadTooLarge | Unreadable::BodyTooLarge => format!(
                "a request to this server is at most {} bytes, request line, headers and body \
                 together",
                MAX_REQUEST_BYTES
            ),
            Unreadable::TimedOut => format!(
                "the request was not complete within {} ms of the connection being picked up",
                REQUEST_DEADLINE.as_millis()
            ),
        }
    }

    fn from_io(error: &io::Error) -> Unreadable {
        match error.kind() {
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => Unreadable::TimedOut,
            _ => Unreadable::Malformed,
        }
    }
}

/// A socket whose every read is given only what is left of one deadline.
///
/// This is what turns a per-read timeout into a per-request one. The deadline
/// is an [`Instant`], so a wall clock being stepped cannot lengthen or shorten
/// it.
struct DeadlineReader {
    socket: TcpStream,
    until: Instant,
}

impl Read for DeadlineReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let left = self.until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "the request deadline has passed",
            ));
        }
        self.socket.set_read_timeout(Some(left))?;
        self.socket.read(buf)
    }
}

/// Read one request, every byte of it through a single bound.
///
/// The request line, each header and the body all come out of one `take` of
/// [`MAX_REQUEST_BYTES`] plus one, so no line can grow past the bound however
/// long the peer keeps typing: reaching the extra byte is the overflow, and it
/// is refused rather than read further.
fn read_request<R: BufRead>(reader: &mut R) -> Result<Request, Unreadable> {
    fn next_line<B: BufRead>(limited: &mut io::Take<B>) -> Result<String, Unreadable> {
        let mut line = Vec::new();
        limited
            .read_until(b'\n', &mut line)
            .map_err(|e| Unreadable::from_io(&e))?;
        if limited.limit() == 0 {
            return Err(Unreadable::HeadTooLarge);
        }
        if line.last() != Some(&b'\n') {
            // The peer closed partway through a line, or before sending one.
            return Err(Unreadable::Malformed);
        }
        String::from_utf8(line).map_err(|_| Unreadable::Malformed)
    }
    let mut limited = reader.take(MAX_REQUEST_BYTES as u64 + 1);
    let line = next_line(&mut limited)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().ok_or(Unreadable::Malformed)?.to_string();
    let path = parts.next().ok_or(Unreadable::Malformed)?.to_string();
    let mut length = 0usize;
    let mut content_type = None;
    let mut origin = None;
    let mut host = None;
    loop {
        let header = next_line(&mut limited)?;
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            let name = name.trim();
            let value = value.trim().to_string();
            if name.eq_ignore_ascii_case("content-length") {
                length = value.parse().map_err(|_| Unreadable::Malformed)?;
            } else if name.eq_ignore_ascii_case("content-type") {
                content_type = Some(value);
            } else if name.eq_ignore_ascii_case("origin") {
                origin = Some(value);
            } else if name.eq_ignore_ascii_case("host") {
                host = Some(value);
            }
        }
    }
    // What is left of the bound, less the one byte that only ever detects an
    // overflow. A body that does not fit is refused unread.
    if length as u64 >= limited.limit() {
        return Err(Unreadable::BodyTooLarge);
    }
    let mut body = vec![0u8; length];
    if length > 0 {
        limited.read_exact(&mut body).map_err(|e| match e.kind() {
            io::ErrorKind::UnexpectedEof => Unreadable::Malformed,
            _ => Unreadable::from_io(&e),
        })?;
    }
    Ok(Request {
        method,
        path,
        content_type,
        origin,
        host,
        body: String::from_utf8_lossy(&body).to_string(),
    })
}

/// Why a state-changing `POST` is refused before its body is looked at, or
/// `None` when it may go on.
///
/// There is no authentication here by design, so what stands between a web
/// page some LAN browser has open and this server's zones is the browser's
/// own rules, and these two are what make them apply:
///
/// - A body that is not declared `application/json` is refused `415`. A
///   cross-site page can send `text/plain` with no preflight; it cannot send
///   `application/json` without one, and a preflight is an `OPTIONS` request
///   this server never approves.
/// - An `Origin` that is present and names anywhere but this server is refused
///   `403`. A browser sends one on every `POST`; the page this server serves
///   posts from this server's own origin, which is the scheme and the `Host`
///   the request was sent to. A client that is not a browser sends none, and
///   is not refused for that.
fn post_refusal(request: &Request) -> Option<(&'static str, String)> {
    let declared_json = request.content_type.as_deref().is_some_and(|value| {
        value
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .eq_ignore_ascii_case("application/json")
    });
    if !declared_json {
        return Some((
            "415 Unsupported Media Type",
            "a command is sent with Content-Type: application/json, and this one was not"
                .to_string(),
        ));
    }
    if let Some(origin) = &request.origin {
        if !same_origin(origin, request.host.as_deref()) {
            return Some((
                "403 Forbidden",
                "a command from a page is accepted only from this server's own origin".to_string(),
            ));
        }
    }
    None
}

/// Whether `origin` names the server this request was sent to.
///
/// The origin's host and port are compared with the `Host` header, which is
/// what the browser addressed. The scheme is not compared: behind a proxy that
/// terminates TLS the page's origin is `https` while this listener speaks plain
/// HTTP, and the host it was addressed by is the same either way.
fn same_origin(origin: &str, host: Option<&str>) -> bool {
    let Some(host) = host else {
        return false;
    };
    let authority = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"));
    match authority {
        Some(authority) => {
            !authority.is_empty()
                && !authority.contains('/')
                && authority.eq_ignore_ascii_case(host.trim())
        }
        // `null`, or anything else that is not an http(s) origin.
        None => false,
    }
}

/// Refuse a request, then drain what the peer is still sending for a short,
/// bounded time so the refusal is not lost to a reset. See [`LINGER`].
fn refuse_unread(connection: &mut TcpStream, status: &str, detail: &str) {
    respond(connection, status, "application/json", &error_body(detail));
    let _ = connection.shutdown(std::net::Shutdown::Write);
    let until = Instant::now() + LINGER;
    let mut scratch = [0u8; 4_096];
    let mut drained = 0usize;
    while drained < LINGER_BYTES {
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() || connection.set_read_timeout(Some(left)).is_err() {
            break;
        }
        match connection.read(&mut scratch) {
            Ok(0) | Err(_) => break,
            Ok(read) => drained += read,
        }
    }
}

/// The error message every refusal in this file is written as.
fn error_body(detail: &str) -> String {
    format!(
        "{{\"v\":1,\"t\":\"error\",\"field\":\"\",\"detail\":\"{}\"}}",
        detail.replace('\\', "\\\\").replace('"', "\\\"")
    )
}

fn respond(connection: &mut TcpStream, status: &str, content_type: &str, body: &str) {
    respond_with(connection, status, content_type, body, "");
}

/// The same response, carrying the Content-Security-Policy. Everything a
/// BROWSER is handed goes through this one: the page, its stylesheet, its
/// script and the document its regions link to.
fn respond_to_browser(connection: &mut TcpStream, status: &str, content_type: &str, body: &str) {
    respond_with(
        connection,
        status,
        content_type,
        body,
        CONTENT_SECURITY_POLICY,
    );
}

fn respond_with(
    connection: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &str,
    policy: &str,
) {
    let policy_header = if policy.is_empty() {
        String::new()
    } else {
        format!("Content-Security-Policy: {}\r\n", policy)
    };
    let _ = write!(
        connection,
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\n{}Connection: close\r\n\r\n{}",
        status,
        content_type,
        body.len(),
        policy_header,
        body
    );
    let _ = connection.flush();
}

/// Serve one connection, whatever it turns out to be, and return the worker's
/// thread to the pool.
fn serve_connection(connection: TcpStream, state: &Arc<ControlState>) {
    // Both directions are bounded, and for the same reason: this worker's slot
    // in the fixed pool has to come back. The read side is bounded per REQUEST
    // (REQUEST_DEADLINE, through DeadlineReader); see WRITE_TIMEOUT for the
    // write side.
    let _ = connection.set_write_timeout(Some(WRITE_TIMEOUT));
    let reader_socket = match connection.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut reader = BufReader::new(DeadlineReader {
        socket: reader_socket,
        until: Instant::now() + REQUEST_DEADLINE,
    });
    let mut connection = connection;
    let request = match read_request(&mut reader) {
        Ok(request) => request,
        Err(unreadable) => {
            refuse_unread(&mut connection, unreadable.status(), &unreadable.detail());
            return;
        }
    };
    let path = request.path.split('?').next().unwrap_or("/").to_string();
    let version = requested_version(&request.path);
    // Who is asking, for the one route that is served to one address.
    let peer = connection.peer_addr().ok().map(|a| a.ip().to_canonical());
    // The two routes that change anything are held to the rules a browser
    // needs to keep a page elsewhere from using them. See post_refusal.
    if request.method == "POST" && (path == "/api/command" || path == "/api/leaving") {
        if let Some((status, detail)) = post_refusal(&request) {
            state.refused.fetch_add(1, Ordering::Relaxed);
            respond(
                &mut connection,
                status,
                "application/json",
                &error_body(&detail),
            );
            return;
        }
    }
    match (request.method.as_str(), path.as_str()) {
        ("GET", "/") | ("GET", "/index.html") => respond_to_browser(
            &mut connection,
            "200 OK",
            "text/html; charset=utf-8",
            state.ui.html,
        ),
        // The palette, the scale and the type roles, ahead of the stylesheet
        // that spends them. It is its own file because nothing outside it is
        // allowed to carry a literal, and a check that reads "outside the token
        // file" needs an outside to read.
        ("GET", "/tokens.css") => respond_to_browser(
            &mut connection,
            "200 OK",
            "text/css; charset=utf-8",
            state.ui.tokens,
        ),
        ("GET", "/chorus.css") => respond_to_browser(
            &mut connection,
            "200 OK",
            "text/css; charset=utf-8",
            state.ui.css,
        ),
        ("GET", "/chorus.js") => respond_to_browser(
            &mut connection,
            "200 OK",
            "text/javascript; charset=utf-8",
            state.ui.js,
        ),
        // What every region of the page links to. The paragraphs explaining what
        // a figure counts live here rather than on the surface, and a link to an
        // explanation has to answer with the explanation.
        ("GET", "/docs/control-page.md") => respond_to_browser(
            &mut connection,
            "200 OK",
            "text/plain; charset=utf-8",
            state.ui.doc,
        ),
        // (goal 18) Who this server is and what it accepts: not a state,
        // and nothing changes it while the server runs.
        ("GET", "/api/server") => match state.server_info() {
            Some(info) => respond(&mut connection, "200 OK", "application/json", &info),
            None => respond(
                &mut connection,
                "503 Service Unavailable",
                "application/json",
                &error_body("this server has not loaded its identity yet"),
            ),
        },
        ("GET", "/api/state") => respond(
            &mut connection,
            "200 OK",
            "application/json",
            &state.encoded_state_at(version),
        ),
        // The bound's report half, over the wire. AC-11 asks that what a
        // dropped subscriber lost is counted AND reported, and a count that
        // only appears on the server's stdout at end of stream is not
        // reportable to anything that is running.
        ("GET", "/api/report") => respond(
            &mut connection,
            "200 OK",
            "text/plain; charset=utf-8",
            &format!("{}\n", state.report()),
        ),
        // The Prometheus exporter (goal 15, `crate::metrics`): one more
        // route on this listener, served by this worker; no thread, no port.
        ("GET", "/metrics") => respond(
            &mut connection,
            "200 OK",
            crate::metrics::CONTENT_TYPE,
            &state.metrics(),
        ),
        ("GET", "/api/events") => serve_events(connection, state, version),
        ("GET", "/api/controller-events") => {
            serve_message_events(connection, state, &state.presses, ": controller events")
        }
        ("GET", "/api/voice-events") => {
            serve_message_events(connection, state, &state.voice_events, ": voice events")
        }
        ("GET", "/api/voice-audio") => serve_voice_audio(connection, state, &request.path, peer),
        ("GET", "/api/visualizer") => serve_visualizer(connection, state, &request.path),
        ("POST", "/api/command") => {
            match state.apply(request.body.trim()) {
                Ok(applied) => respond(&mut connection, "200 OK", "application/json", &applied),
                Err(refusal) => {
                    state.refused.fetch_add(1, Ordering::Relaxed);
                    let status = if refusal.ends_the_session() {
                        // The version is not one this build implements, so the
                        // session is refused rather than the message.
                        "426 Upgrade Required"
                    } else {
                        "400 Bad Request"
                    };
                    respond(
                        &mut connection,
                        status,
                        "application/json",
                        &refusal.encode(),
                    );
                }
            }
        }
        ("POST", "/api/leaving") => {
            let endpoint = request.body.trim().to_string();
            state.endpoint_left(&endpoint);
            respond(&mut connection, "200 OK", "application/json", "{}");
        }
        _ => respond(
            &mut connection,
            "404 Not Found",
            "application/json",
            "{\"v\":1,\"t\":\"error\",\"field\":\"\",\"detail\":\"no such route\"}",
        ),
    }
}

/// Open one server-sent event stream and hand it to the event writer.
///
/// The first thing written is the state as it stands, so a subscriber is never
/// waiting for a change to learn what is true now; it is written here, by the
/// worker, under the worker's write timeout, and from then on the stream is
/// the event writer's (`crate::events`) and this worker goes back to the pool.
/// A subscriber therefore costs no worker, however long it stays (audit
/// finding B-5). Past the event writer's ceiling (`--event-streams`) a stream
/// is answered `503`, naming the ceiling, and nothing is held.
fn serve_events(mut connection: TcpStream, state: &Arc<ControlState>, version: i64) {
    let Some(claim) = state.events.claim() else {
        state.turned_away.fetch_add(1, Ordering::Relaxed);
        let detail = format!(
            "every one of this server's {} event streams is held; try again, or start the \
             server with a higher --event-streams",
            state.events.ceiling()
        );
        respond(
            &mut connection,
            "503 Service Unavailable",
            "application/json",
            &error_body(&detail),
        );
        return;
    };
    let inbox = state.fanout.subscribe();
    let opening = state.encoded_state_at(version);
    if write!(
        connection,
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\n\
         Connection: close\r\n\r\ndata: {}\n\n",
        opening
    )
    .is_err()
        || connection.flush().is_err()
    {
        return;
    }
    claim.hand_over(connection, inbox, version);
}

/// Open one stream of controller events and hand it to the event writer.
///
/// The same subscriber as `GET /api/events` in everything but what it is
/// sent: a server-sent event stream, one of the event writer's
/// `--event-streams`, refused `503` past that ceiling, kept alive and dropped
/// when stalled by the same rules. It carries one `data: <controller_event>`
/// per controller command the server accepts from now on and NOTHING at the
/// start: a press is not a state, there is no "press as it stands", and one
/// made before this stream was opened is never sent to it. The message is
/// catalog version 2's alone, so `?v=1` changes nothing here.
///
/// `GET /api/voice-events` (voice, P8) is the same subscriber again, fed by
/// the third fanout: one `data: <voice_wake>` per wake word heard from now
/// on, after the opening comment line `: voice events`.
fn serve_message_events(
    mut connection: TcpStream,
    state: &Arc<ControlState>,
    fanout: &ControlFanout,
    opening: &str,
) {
    let Some(claim) = state.events.claim() else {
        state.turned_away.fetch_add(1, Ordering::Relaxed);
        let detail = format!(
            "every one of this server's {} event streams is held; try again, or start the \
             server with a higher --event-streams",
            state.events.ceiling()
        );
        respond(
            &mut connection,
            "503 Service Unavailable",
            "application/json",
            &error_body(&detail),
        );
        return;
    };
    // Attached before the headers go out, so a subscriber that has read them
    // is sent every press accepted after that.
    let inbox = fanout.subscribe();
    if write!(
        connection,
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\n\
         Connection: close\r\n\r\n{}\n\n",
        opening
    )
    .is_err()
        || connection.flush().is_err()
    {
        return;
    }
    // Written as they are: the writer renders nothing for a stream at the
    // build's own version.
    claim.hand_over(connection, inbox, chorus_control::CATALOG_VERSION);
}

/// What the body of `GET /api/voice-audio` is: raw samples, no container.
pub const VOICE_AUDIO_CONTENT_TYPE: &str = "application/octet-stream";

/// The header that says what those samples are.
pub const VOICE_AUDIO_FORMAT: &str = "pcm_s16le; rate=16000; channels=1";

/// (voice, P8) Serve one open voice run's audio to its one reader
/// (`docs/control-plane.md`, "Voice: the wake word, the run and its audio";
/// `docs/decisions/0172-*`).
///
/// `GET /api/voice-audio?run=<id>`: the microphone audio of the room the run
/// is in, raw 16 kHz mono 16-bit little-endian PCM, from the response's
/// headers until the run ends, when the server closes the connection. This
/// is the only route that carries microphone audio, and the control plane
/// has no authentication, so it is served under four rules, checked in this
/// order, each refused by name and logged without the identifier:
///
/// 1. the caller's address is the one the server was started with
///    (`--voice-integration`), else `403`;
/// 2. the request names a run, else `400`;
/// 3. a run with that identifier is open, else `404`: one answer for "no
///    run at all" and "not this one", so a wrong guess learns nothing;
/// 4. nobody is reading the run yet, else `409`.
///
/// The reader is then one of the event writer's streams and costs no worker.
fn serve_voice_audio(
    mut connection: TcpStream,
    state: &Arc<ControlState>,
    target: &str,
    peer: Option<IpAddr>,
) {
    let refuse = |connection: &mut TcpStream, status: &str, reason: &str, detail: &str| {
        state.refused.fetch_add(1, Ordering::Relaxed);
        println!(
            "chorus-server: voice route refused reason={} peer={}",
            reason,
            peer.map_or("unknown".to_string(), |p| p.to_string())
        );
        respond(
            connection,
            status,
            "application/json",
            &error_body(&format!("{}: {}", reason, detail)),
        );
    };
    let allowed = state.voice_integration.get();
    if allowed.is_none() || peer.as_ref() != allowed {
        refuse(
            &mut connection,
            "403 Forbidden",
            "not-the-voice-integration",
            "a voice run's audio is served to the address this server was started with \
             (--voice-integration) and to no other",
        );
        return;
    }
    let query = target.split_once('?').map(|(_, q)| q).unwrap_or("");
    let run = query
        .split('&')
        .find_map(|pair| pair.strip_prefix("run="))
        .unwrap_or("");
    if run.is_empty() {
        refuse(
            &mut connection,
            "400 Bad Request",
            "no-run-named",
            "name the run: GET /api/voice-audio?run=<the identifier voice_start answered with>",
        );
        return;
    }
    let Some(voice) = state.voice.get() else {
        refuse(
            &mut connection,
            "404 Not Found",
            "no-voice-run",
            "no voice run with that identifier is open",
        );
        return;
    };
    // The stream is claimed before the run is, so a reader turned away at
    // the ceiling has taken nothing.
    let Some(claim) = state.events.claim() else {
        state.turned_away.fetch_add(1, Ordering::Relaxed);
        let detail = format!(
            "every one of this server's {} event streams is held; try again, or start the \
             server with a higher --event-streams",
            state.events.ceiling()
        );
        respond(
            &mut connection,
            "503 Service Unavailable",
            "application/json",
            &error_body(&detail),
        );
        return;
    };
    match voice.claim(run, Instant::now()) {
        Ok(()) => {}
        Err(crate::voice::ClaimRefused::NoRun) => {
            refuse(
                &mut connection,
                "404 Not Found",
                "no-voice-run",
                "no voice run with that identifier is open",
            );
            return;
        }
        Err(crate::voice::ClaimRefused::Taken) => {
            refuse(
                &mut connection,
                "409 Conflict",
                "voice-run-taken",
                "this voice run has its reader already, and a run has one",
            );
            return;
        }
    }
    if write!(
        connection,
        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nX-Chorus-Audio-Format: {}\r\n\
         Cache-Control: no-store\r\nConnection: close\r\n\r\n",
        VOICE_AUDIO_CONTENT_TYPE, VOICE_AUDIO_FORMAT
    )
    .is_err()
        || connection.flush().is_err()
    {
        voice.reader_gone(run, "reader-gone");
        return;
    }
    claim.hand_over_mic(connection, Arc::clone(voice), run.to_string());
}

/// Open one room's visualizer stream and hand it to the event writer
/// (`crate::lights`, `docs/visualizer.md`, "The HTTP stream").
///
/// `GET /api/visualizer?zone=<room>`: a server-sent event stream, one of the
/// event writer's `--event-streams`, refused `503` past that ceiling, kept
/// alive and dropped when stalled by the same rules as the other two kinds.
/// It opens with one comment line and no frame; from then on it carries one
/// `data: <visualizer message>` per frame it is sent, at most one every
/// [`crate::lights::MIN_FRAME_INTERVAL`], each the room's latest. A request
/// that names no room is refused `400` and one that names a room this server
/// does not have `404`, both before anything is held.
fn serve_visualizer(mut connection: TcpStream, state: &Arc<ControlState>, target: &str) {
    let query = target.split_once('?').map(|(_, q)| q).unwrap_or("");
    let zone = query
        .split('&')
        .find_map(|pair| pair.strip_prefix("zone="))
        .unwrap_or("");
    if zone.is_empty() {
        state.refused.fetch_add(1, Ordering::Relaxed);
        respond(
            &mut connection,
            "400 Bad Request",
            "application/json",
            &error_body("name the room: GET /api/visualizer?zone=<room id>"),
        );
        return;
    }
    if !state.locked().zones.zones().iter().any(|z| z.id == zone) {
        state.refused.fetch_add(1, Ordering::Relaxed);
        respond(
            &mut connection,
            "404 Not Found",
            "application/json",
            &error_body("this server has no room of that id"),
        );
        return;
    }
    let Some(lights) = state.lights.get() else {
        respond(
            &mut connection,
            "503 Service Unavailable",
            "application/json",
            &error_body("this server has no visualizer stream yet"),
        );
        return;
    };
    let Some(claim) = state.events.claim() else {
        state.turned_away.fetch_add(1, Ordering::Relaxed);
        let detail = format!(
            "every one of this server's {} event streams is held; try again, or start the \
             server with a higher --event-streams",
            state.events.ceiling()
        );
        respond(
            &mut connection,
            "503 Service Unavailable",
            "application/json",
            &error_body(&detail),
        );
        return;
    };
    // Subscribed before the headers go out, and the conductor told at once:
    // its pass says which slot the room is on, which is what makes the audio
    // thread analyse that slot.
    let subscription = lights.subscribe(zone);
    let _ = state.conductor_wake.try_send(());
    if write!(
        connection,
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\n\
         Connection: close\r\n\r\n: visualizer\n\n"
    )
    .is_err()
        || connection.flush().is_err()
    {
        return;
    }
    claim.hand_over_light(connection, subscription);
}

/// The catalog version a `GET` asks its state in: `?v=1` is the v1 shape, and
/// anything else (no query, or a version this build does not render) is the
/// build's own.
fn requested_version(target: &str) -> i64 {
    let query = target.split_once('?').map(|(_, q)| q).unwrap_or("");
    if query.split('&').any(|pair| pair == "v=1") {
        1
    } else {
        chorus_control::CATALOG_VERSION
    }
}

/// Build the zone state a run starts from: the persisted file where there is
/// one, and the configured zones otherwise.
///
/// The two are not merged. A state file is the whole answer or it is not
/// consulted, because merging would mean deciding whether a zone in the file
/// and not on the command line had been deleted or had never been configured,
/// and there is no way to tell those apart.
/// `docs/decisions/0018-the-persisted-zone-state.md` records that.
pub fn initial_state(
    state_file: Option<&str>,
    configured: &[String],
    group_audio: &[(String, String)],
    default_audio: &str,
) -> Result<(Zones, PathBuf, bool), ControlRefused> {
    let path = PathBuf::from(state_file.unwrap_or("chorus-zones.state"));
    let mut loaded = None;
    if state_file.is_some() {
        match persist::read_file(&path, default_audio) {
            Ok(zones) => loaded = zones,
            Err(e) => {
                return Err(ControlRefused::State {
                    path: path.display().to_string(),
                    detail: e.to_string(),
                })
            }
        }
    }
    let from_file = loaded.is_some();
    let mut zones = match loaded {
        Some(zones) => zones,
        None => {
            let mut zones = Zones::new(default_audio);
            for id in configured {
                zones
                    .add(chorus_control::zones::Zone::new(id))
                    .map_err(|e| ControlRefused::Zones {
                        detail: format!("the zone '{}' was refused: {}", id, e),
                    })?;
            }
            zones
        }
    };
    for (group, address) in group_audio {
        zones.set_group_audio(group, address);
    }
    Ok((zones, path, from_file))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chorus_control::zones::Zone;

    #[test]
    fn a_front_member_is_told_what_its_set_lacks_and_the_rooms_tv_upmix() {
        use chorus_control::rooms::{BondMember, Role};
        use chorus_control::theater::TvUpmix;
        // Every layout validate_layout allows, and what a front member folds.
        assert_eq!(fold_of(&[]), 0, "no set, no fold");
        assert_eq!(
            fold_of(&[Role::Fl, Role::Fr]),
            sound_fold::CENTRE | sound_fold::SURROUND
        );
        assert_eq!(
            fold_of(&[Role::Fl, Role::Fr, Role::Lfe]),
            sound_fold::CENTRE | sound_fold::SURROUND
        );
        assert_eq!(
            fold_of(&[Role::Fl, Role::Fr, Role::Fc]),
            sound_fold::SURROUND
        );
        assert_eq!(
            fold_of(&[Role::Fl, Role::Fr, Role::Fc, Role::Lfe, Role::Sl, Role::Sr]),
            0
        );
        assert_eq!(
            fold_of(&[Role::Fl, Role::Fr, Role::Fc, Role::Bl, Role::Br]),
            0
        );
        let mut z = Zone::new("lounge");
        z.sound.tv_upmix = TvUpmix::Ambient;
        z.bond = [("a", Role::Fl), ("b", Role::Fr), ("c", Role::Lfe)]
            .into_iter()
            .map(|(e, role)| BondMember {
                endpoint: e.to_string(),
                role,
            })
            .collect();
        let fl = sound_of(&z, "a");
        assert_eq!((fl.role, fl.fold, fl.tv_upmix), (1, 3, 1));
        let sub = sound_of(&z, "c");
        assert_eq!((sub.role, sub.fold, sub.tv_upmix), (4, 0, 1));
    }

    #[test]
    fn a_command_that_is_applied_reaches_every_subscriber() {
        let mut zones = Zones::new("127.0.0.1:4010");
        zones.add(Zone::new("kitchen")).unwrap();
        let state = Arc::new(ControlState::new(zones, None));
        let a = state.fanout.subscribe();
        let b = state.fanout.subscribe();
        let applied = state
            .apply(r#"{"v":1,"t":"volume","zone":"kitchen","volume":0.250}"#)
            .expect("it applies");
        assert!(applied.contains(r#""volume":0.250"#), "{}", applied);
        assert_eq!(a.recv().unwrap().as_str(), applied);
        assert_eq!(
            b.recv().unwrap().as_str(),
            applied,
            "including the subscriber that did not send the command"
        );
    }

    /// Goal 14: the speakers a server lists are the ones its pins hold, a
    /// session lists a new one, and `speaker_forget` removes the pin with the
    /// record, or refuses whole when the pin file cannot be rewritten.
    #[test]
    fn a_speaker_is_listed_from_its_pin_and_forgotten_with_it_or_not_at_all() {
        use crate::session::{Adoptions, ADOPTED_FILE};
        let dir = std::env::temp_dir().join(format!("chorus-speakers-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(ADOPTED_FILE);
        let adoptions = Arc::new(Adoptions::load(&path).unwrap());
        // Pinned before the control plane knew of it (an upgraded server).
        assert!(adoptions.check("den", &[1; 32]).unwrap().admits());
        assert!(adoptions.check("Not An Id", &[2; 32]).unwrap().admits());

        let mut zones = Zones::new("127.0.0.1:4010");
        zones.add(Zone::new("kitchen")).unwrap();
        let state = Arc::new(ControlState::new(zones, None));
        let unlisted = state.adopt_through(Arc::clone(&adoptions));
        assert_eq!(unlisted.len(), 1, "{:?}", unlisted);
        assert!(
            unlisted[0].starts_with("speaker not listed id=Not An Id reason=id-not-an-identifier"),
            "{}",
            unlisted[0]
        );
        let listed = state.encoded_state();
        assert!(
            listed.contains(r#""speakers":[{"id":"den","name":"Speaker den","named":false,"room":null,"present":false"#),
            "{}",
            listed
        );

        // A new id's first session lists it, says so once, and marks it
        // present; its end marks it absent.
        assert!(adoptions.check("hall", &[3; 32]).unwrap().admits());
        let lines = state.speaker_session_up("hall", "aaaa:bbbb:cccc:dddd", "sw 1", roles::PLAYER);
        assert_eq!(
            lines,
            vec![r#"speaker listed id=hall name="Speaker hall" named=0"#.to_string()]
        );
        assert!(state
            .speaker_session_up("hall", "aaaa:bbbb:cccc:dddd", "sw 1", roles::PLAYER)
            .is_empty());
        assert!(state.encoded_state().contains(
            r#"{"id":"hall","name":"Speaker hall","named":false,"room":null,"present":true,"software":"sw 1","link":"unknown","key":"aaaa:bbbb:cccc:dddd","roles":["player"]}"#
        ));
        state.speaker_session_down("hall");
        state.speaker_session_down("hall");
        assert!(state.encoded_state().contains(
            r#""id":"hall","name":"Speaker hall","named":false,"room":null,"present":false"#
        ));

        // A refused key change is in the state.
        state.speaker_key_changed("den", "1111:2222:3333:4444", "5555:6666:7777:8888");
        assert!(state.encoded_state().contains(
            r#""key_changes":[{"id":"den","pinned":"1111:2222:3333:4444","offered":"5555:6666:7777:8888"}]"#
        ));

        // The pin file cannot be rewritten: the command is refused whole.
        std::fs::remove_dir_all(&dir).unwrap();
        let before = state.encoded_state();
        let refusal = state
            .apply(r#"{"v":2,"t":"speaker_forget","speaker":"den"}"#)
            .unwrap_err();
        assert_eq!(refusal.field, "speaker");
        assert!(refusal.detail.contains("was not forgotten"), "{}", refusal);
        assert_eq!(state.encoded_state(), before, "nothing moved");
        assert!(
            adoptions.pins().iter().any(|(id, _)| id == "den"),
            "and the pin is still held"
        );

        // With the directory back, the record, the key change and the pin go
        // together, and the next handshake under the id is an adoption.
        std::fs::create_dir_all(&dir).unwrap();
        let after = state
            .apply(r#"{"v":2,"t":"speaker_forget","speaker":"den"}"#)
            .unwrap();
        assert!(!after.contains(r#""id":"den""#), "{}", after);
        assert!(!after.contains("key_changes"), "{}", after);
        assert!(!adoptions.pins().iter().any(|(id, _)| id == "den"));
        assert!(!std::fs::read_to_string(&path).unwrap().contains(" den\n"));
        assert_eq!(
            adoptions.check("den", &[9; 32]).unwrap(),
            chorus_protocol::v2::adoption::Verdict::Adopted
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_command_that_is_refused_reaches_nobody() {
        let mut zones = Zones::new("127.0.0.1:4010");
        zones.add(Zone::new("kitchen")).unwrap();
        let state = Arc::new(ControlState::new(zones, None));
        let subscriber = state.fanout.subscribe();
        let before = state.encoded_state();
        assert!(state
            .apply(r#"{"v":1,"t":"volume","zone":"kitchen","volume":9.000}"#)
            .is_err());
        assert_eq!(state.encoded_state(), before);
        assert!(
            subscriber.try_recv().is_err(),
            "a refused command must fan nothing out"
        );
    }

    #[test]
    fn a_line_with_no_end_is_cut_at_the_bound_and_not_read_past_it() {
        let endless = vec![b'a'; 4 * MAX_REQUEST_BYTES];
        let mut reader: &[u8] = &endless;
        assert_eq!(
            read_request(&mut reader).err(),
            Some(Unreadable::HeadTooLarge)
        );
        assert_eq!(
            reader.len(),
            endless.len() - MAX_REQUEST_BYTES - 1,
            "exactly the bound plus the one byte that detects the overflow is read"
        );
    }

    #[test]
    fn the_bound_covers_the_body_as_well_as_the_head() {
        let head = "POST /api/command HTTP/1.1\r\nContent-Type: application/json\r\n";
        let fits = MAX_REQUEST_BYTES - head.len() - "Content-Length: 99999\r\n\r\n".len();
        let at_the_bound = format!(
            "{}Content-Length: {:05}\r\n\r\n{}",
            head,
            fits,
            "x".repeat(fits)
        );
        assert_eq!(at_the_bound.len(), MAX_REQUEST_BYTES);
        let request = read_request(&mut at_the_bound.as_bytes()).expect("a request at the bound");
        assert_eq!(request.body.len(), fits);
        assert_eq!(request.content_type.as_deref(), Some("application/json"));
        let one_over = format!(
            "{}Content-Length: {:05}\r\n\r\n{}",
            head,
            fits + 1,
            "x".repeat(fits + 1)
        );
        assert_eq!(
            read_request(&mut one_over.as_bytes()).err(),
            Some(Unreadable::BodyTooLarge)
        );
    }

    #[test]
    fn a_request_cut_short_is_malformed() {
        for text in ["", "GET / HTTP/1.1", "GET / HTTP/1.1\r\nHost: x"] {
            assert_eq!(
                read_request(&mut text.as_bytes()).err(),
                Some(Unreadable::Malformed),
                "{:?}",
                text
            );
        }
        let short_body = "POST /api/command HTTP/1.1\r\nContent-Length: 10\r\n\r\nabc";
        assert_eq!(
            read_request(&mut short_body.as_bytes()).err(),
            Some(Unreadable::Malformed)
        );
    }

    fn command_request(content_type: Option<&str>, origin: Option<&str>) -> Request {
        Request {
            method: "POST".to_string(),
            path: "/api/command".to_string(),
            content_type: content_type.map(str::to_string),
            origin: origin.map(str::to_string),
            host: Some("chorus.example:4011".to_string()),
            body: String::new(),
        }
    }

    #[test]
    fn a_command_must_be_declared_json_and_come_from_here_if_from_a_page() {
        let json = Some("application/json");
        for (content_type, origin, status) in [
            (json, None, None),
            (Some("Application/JSON; charset=utf-8"), None, None),
            (json, Some("http://chorus.example:4011"), None),
            (json, Some("https://CHORUS.example:4011"), None),
            (Some("text/plain"), None, Some("415")),
            (None, None, Some("415")),
            (Some("application/jsonx"), None, Some("415")),
            (json, Some("http://elsewhere.example"), Some("403")),
            (json, Some("http://chorus.example"), Some("403")),
            (json, Some("null"), Some("403")),
            (json, Some("http://chorus.example:4011/x"), Some("403")),
        ] {
            let refusal = post_refusal(&command_request(content_type, origin));
            assert_eq!(
                refusal.map(|(s, _)| &s[..3]),
                status,
                "{:?} {:?}",
                content_type,
                origin
            );
        }
        let mut no_host = command_request(json, Some("http://chorus.example:4011"));
        no_host.host = None;
        assert!(post_refusal(&no_host).is_some());
    }

    #[test]
    fn a_state_file_that_cannot_be_read_stops_the_run_rather_than_starting_from_defaults() {
        let mut path = std::env::temp_dir();
        path.push(format!("chorus-bad-state-{}.conf", std::process::id()));
        std::fs::write(&path, "format = 99\n").unwrap();
        let err = initial_state(
            Some(&path.display().to_string()),
            &[],
            &[],
            "127.0.0.1:4010",
        )
        .expect_err("a state file this build does not understand is refused");
        assert!(err.to_string().contains("could not be read"), "{}", err);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_state_file_that_is_not_there_yet_is_not_an_error() {
        let mut path = std::env::temp_dir();
        path.push(format!("chorus-absent-state-{}.conf", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let (zones, _, from_file) = initial_state(
            Some(&path.display().to_string()),
            &["kitchen".to_string()],
            &[],
            "127.0.0.1:4010",
        )
        .expect("a first run has no state file");
        assert!(!from_file);
        assert!(zones.zone("kitchen").is_some());
    }
}
