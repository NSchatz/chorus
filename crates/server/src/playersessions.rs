//! Player sessions: a player, the group that plays it, and what the state
//! says is playing (goal 16).
//!
//! The engine (`crate::mediaplayer`) knows URLs and frames; the room model
//! knows groups and sources. A session ties one to the other for whoever
//! plays a URL in a room: it takes a player from the pool, issues the `take`
//! that makes the target's group play it (K78), loads and starts the URL,
//! keeps the group's now-playing record in step with the player's reports,
//! and gives the player back when the track ends, fails, or the group's
//! source stops being this player, whoever changed it.
//!
//! It is in-process only, on purpose. Brief section 4.8 allows no arbitrary
//! URL fetch but the input paths the decisions name (UPnP renders, the home
//! automation's media and TTS URLs from its own address, stored alarm stream
//! URLs), so there is no command on the control API that plays a URL: the
//! callers of [`PlayerSessions::play`] are those named paths, the first of
//! them the UPnP renderer.
//!
//! It runs no thread. Its owner calls [`PlayerSessions::pump`] (or
//! [`PlayerSessions::on_report`] and [`PlayerSessions::reconcile`]) from a
//! thread it already has.
//!
//! Control code, off the audio path: `audio-path.conf` records it as
//! excluded.

use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use chorus_control::rooms::{NowPlaying, PlayState};

use crate::control::ControlState;
use crate::mediaplayer::{Action, Event, MediaInfo, PlayerReport, Players};
use crate::player::player_id;

/// What the caller knows about the media before it is opened (a renderer:
/// the DIDL-Lite metadata). Each field that is present wins over the file's
/// own tags; a title the stream names later (ICY, a new link of a chained Ogg
/// stream) replaces the tags' title.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Metadata {
    /// The title.
    pub title: Option<String>,
    /// The artist.
    pub artist: Option<String>,
    /// The album.
    pub album: Option<String>,
    /// Where the artwork is.
    pub art_url: Option<String>,
    /// The duration the caller was told, used until the decoder knows one.
    pub duration_ms: Option<u64>,
}

/// One request to play a URL in a room or group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayRequest {
    /// Who asks, an identifier of the caller's own (a renderer:
    /// `room:<id>`). One owner holds at most one player.
    pub owner: String,
    /// The room or group that should play it: the `take` command's target.
    pub target: String,
    /// The `http` or `https` URL.
    pub uri: String,
    /// The media type the caller was told.
    pub mime: Option<String>,
    /// What drives the player, for the now-playing record: `upnp`.
    pub via: String,
    /// The caller's epoch, which the player's reports carry back.
    pub epoch: u64,
    /// What the caller knows about the media.
    pub metadata: Metadata,
}

#[derive(Debug, Clone)]
struct Session {
    via: String,
    /// Reports older than this are another play's.
    since: u64,
    hints: Metadata,
    /// What the engine found: the tags and duration of the audible track,
    /// and the stream's latest title.
    found: Option<MediaInfo>,
    stream_title: Option<String>,
    state: PlayState,
}

/// The sessions of a server's players. Plain data behind a mutex.
pub struct PlayerSessions {
    state: Arc<ControlState>,
    players: Arc<Players>,
    held: Mutex<Vec<Option<Session>>>,
    failures: Mutex<Vec<Option<String>>>,
    log: Box<dyn Fn(&str) + Send + Sync>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Whether `target` can be written into a command as it is.
fn is_plain(target: &str) -> bool {
    !target.is_empty()
        && target
            .chars()
            .all(|c| !c.is_control() && c != '"' && c != '\\')
}

impl PlayerSessions {
    /// Sessions over `players`, acting on `state`. `log` gets one line per
    /// start, end and failure.
    pub fn new(
        state: Arc<ControlState>,
        players: Arc<Players>,
        log: Box<dyn Fn(&str) + Send + Sync>,
    ) -> PlayerSessions {
        let count = players.len();
        PlayerSessions {
            state,
            players,
            held: Mutex::new(vec![None; count]),
            failures: Mutex::new(vec![None; count]),
            log,
        }
    }

    /// The players these sessions run on.
    pub fn players(&self) -> &Arc<Players> {
        &self.players
    }

    fn take(&self, target: &str, source: &str) -> Result<(), String> {
        self.state
            .apply(&format!(
                r#"{{"v":2,"t":"take","target":"{}","source":"{}"}}"#,
                target, source
            ))
            .map(|_| ())
            .map_err(|refusal| format!("{}: {}", refusal.field, refusal.detail))
    }

    /// Play `request.uri` on `request.target`: acquire a player (the one the
    /// owner already holds, else an idle one), make the target's group play
    /// it, load and start the URL, and show the group as buffering. Returns
    /// the player's index. Refused, by name, when no player is free, when
    /// the server runs none, or when the `take` is refused (an unknown
    /// target, a player that plays elsewhere); a refusal changes nothing.
    pub fn play(&self, request: &PlayRequest) -> Result<usize, String> {
        self.begin(request, true)
    }

    /// [`PlayerSessions::play`] for an owner whose player already has
    /// `request.uri` loaded (a renderer loads at SetAVTransportURI and starts
    /// at Play): the same, without the load, so what is open, and a next URI
    /// queued behind it, are kept.
    pub fn start_loaded(&self, request: &PlayRequest) -> Result<usize, String> {
        self.begin(request, false)
    }

    fn begin(&self, request: &PlayRequest, load: bool) -> Result<usize, String> {
        if self.players.is_empty() {
            return Err("players: this server was started without --players".to_string());
        }
        if !is_plain(&request.target) {
            return Err("target: not an identifier".to_string());
        }
        let had = self.players.held_by(&request.owner);
        let Some(index) = self.players.acquire(&request.owner) else {
            return Err(format!(
                "players: no free player: all {} are in use",
                self.players.len()
            ));
        };
        let id = player_id(index);
        if let Err(refused) = self.take(&request.target, &format!("player:{id}")) {
            if had.is_none() {
                self.players.release(index);
            }
            return Err(refused);
        }
        lock(&self.held)[index] = Some(Session {
            via: request.via.clone(),
            since: request.epoch,
            hints: request.metadata.clone(),
            found: None,
            stream_title: None,
            state: PlayState::Buffering,
        });
        lock(&self.failures)[index] = None;
        if let Some(handle) = self.players.handle(index) {
            if load {
                handle.send(
                    request.epoch,
                    Action::Load {
                        uri: request.uri.clone(),
                        mime: request.mime.clone(),
                    },
                );
            }
            handle.send(request.epoch, Action::Start);
        }
        self.show(index);
        (self.log)(&format!(
            "player {} plays for {} on {} via {}",
            id, request.owner, request.target, request.via
        ));
        Ok(index)
    }

    /// End the owner's session and keep its player: the group that plays the
    /// player gets the source `none` (which clears its now-playing record)
    /// and the player stays the owner's, loaded, for the owner to stop and
    /// start again. For a renderer's Stop, which is often followed by a Play
    /// at once: a player given back would have to be unloaded first, and one
    /// that is still unloading cannot be taken. Returns whether the owner had
    /// a session.
    pub fn suspend(&self, owner: &str) -> bool {
        let Some(index) = self.players.held_by(owner) else {
            return false;
        };
        if lock(&self.held)[index].take().is_none() {
            return false;
        }
        let id = player_id(index);
        if let Some(group) = self.state.player_group(&id) {
            if let Err(refused) = self.take(&group, "none") {
                (self.log)(&format!(
                    "player {id}: group {group} not stopped: {refused}"
                ));
            }
        }
        (self.log)(&format!("player {id} stopped for {owner}"));
        true
    }

    /// Whether the owner's player is in a session (its group plays it and
    /// its now-playing record is kept).
    pub fn in_session(&self, owner: &str) -> bool {
        self.players
            .held_by(owner)
            .is_some_and(|index| lock(&self.held)[index].is_some())
    }

    /// Replace what the caller knows about the audible track (a renderer at
    /// a gapless boundary: the next URI's metadata).
    pub fn set_metadata(&self, owner: &str, metadata: Metadata) {
        let Some(index) = self.players.held_by(owner) else {
            return;
        };
        if let Some(session) = lock(&self.held)[index].as_mut() {
            session.hints = metadata;
        }
        self.show(index);
    }

    /// Hold (`true`) or carry on (`false`) the owner's player, and show it.
    pub fn set_paused(&self, owner: &str, epoch: u64, paused: bool) -> bool {
        let Some(index) = self.players.held_by(owner) else {
            return false;
        };
        let Some(handle) = self.players.handle(index) else {
            return false;
        };
        handle.send(
            epoch,
            if paused {
                Action::Pause
            } else {
                Action::Resume
            },
        );
        if let Some(session) = lock(&self.held)[index].as_mut() {
            session.state = if paused {
                PlayState::Paused
            } else {
                PlayState::Playing
            };
        }
        self.show(index);
        true
    }

    /// Stop the owner's player, set its group's source to `none` and give the
    /// player back. Returns whether the owner held one.
    pub fn stop(&self, owner: &str) -> bool {
        let Some(index) = self.players.held_by(owner) else {
            return false;
        };
        self.finish(index, None);
        true
    }

    /// The player `owner` holds.
    pub fn player_of(&self, owner: &str) -> Option<usize> {
        self.players.held_by(owner)
    }

    /// Why player `index`'s last play failed, until its next play.
    pub fn last_failure(&self, index: usize) -> Option<String> {
        lock(&self.failures).get(index).cloned().flatten()
    }

    /// Take one report: the group's now-playing record follows the player,
    /// and an end or a failure sets the group's source to `none`, clears the
    /// record and gives the player back. A report from before the session's
    /// play is ignored.
    pub fn on_report(&self, report: &PlayerReport) {
        let index = report.player;
        {
            let mut held = lock(&self.held);
            let Some(Some(session)) = held.get_mut(index) else {
                return;
            };
            if report.epoch < session.since {
                return;
            }
            match &report.event {
                Event::Opened(info) => session.found = Some(info.clone()),
                Event::Boundary(info) => {
                    session.found = Some(info.clone());
                    session.stream_title = None;
                    // What the caller said was about the track before.
                    session.hints = Metadata::default();
                    if session.state == PlayState::Buffering {
                        session.state = PlayState::Playing;
                    }
                }
                Event::Started => {
                    if session.state == PlayState::Buffering {
                        session.state = PlayState::Playing;
                    }
                }
                Event::Title(title) => session.stream_title = Some(title.clone()),
                Event::Ended { .. } | Event::Failed { .. } => {}
                Event::NextOpened(_)
                | Event::NextFailed { .. }
                | Event::SeekDone { .. }
                | Event::SeekRefused { .. } => return,
            }
        }
        match &report.event {
            Event::Ended { .. } => self.finish(index, None),
            Event::Failed { reason } => self.finish(index, Some(reason.clone())),
            _ => self.show(index),
        }
    }

    /// Give back every player whose group no longer plays it: somebody set
    /// the group's source to something else (a `take`, a line-in, an alarm).
    /// The player is unloaded; the group is left as that somebody made it.
    pub fn reconcile(&self) {
        for index in 0..self.players.len() {
            if lock(&self.held)[index].is_none() {
                continue;
            }
            if self.state.player_group(&player_id(index)).is_some() {
                continue;
            }
            lock(&self.held)[index] = None;
            self.unload(index);
            (self.log)(&format!(
                "player {} released: its group plays something else",
                player_id(index)
            ));
        }
    }

    /// Wait up to `wait` for one report, apply it, then [`reconcile`]. The
    /// report is handed back for the caller's own use (a renderer feeds it to
    /// its AVTransport).
    ///
    /// [`reconcile`]: PlayerSessions::reconcile
    pub fn pump(&self, reports: &Receiver<PlayerReport>, wait: Duration) -> Option<PlayerReport> {
        let report = reports.recv_timeout(wait).ok();
        if let Some(report) = &report {
            self.on_report(report);
        }
        self.reconcile();
        report
    }

    fn unload(&self, index: usize) {
        if let Some(handle) = self.players.handle(index) {
            handle.send(u64::MAX, Action::Unload);
        }
        self.players.release(index);
    }

    /// End player `index`'s session: its group's source becomes `none`
    /// (which clears the now-playing record), the player is unloaded and
    /// given back.
    fn finish(&self, index: usize, failure: Option<String>) {
        if lock(&self.held)[index].take().is_none() {
            return;
        }
        let id = player_id(index);
        if let Some(group) = self.state.player_group(&id) {
            if let Err(refused) = self.take(&group, "none") {
                (self.log)(&format!(
                    "player {id}: group {group} not stopped: {refused}"
                ));
            }
        }
        self.unload(index);
        match failure {
            Some(reason) => {
                (self.log)(&format!("player {id} failed: {reason}"));
                lock(&self.failures)[index] = Some(reason);
            }
            None => (self.log)(&format!("player {id} released")),
        }
    }

    /// Write player `index`'s now-playing record into its group.
    fn show(&self, index: usize) {
        let Some(session) = lock(&self.held)[index].clone() else {
            return;
        };
        let Some(group) = self.state.player_group(&player_id(index)) else {
            return;
        };
        let found = session.found.as_ref();
        let tags = found.map(|info| &info.tags);
        let record = NowPlaying {
            title: session
                .hints
                .title
                .clone()
                .or_else(|| session.stream_title.clone())
                .or_else(|| tags.and_then(|t| t.title.clone()))
                .or_else(|| found.and_then(|info| info.station.clone())),
            artist: session
                .hints
                .artist
                .clone()
                .or_else(|| tags.and_then(|t| t.artist.clone())),
            album: session
                .hints
                .album
                .clone()
                .or_else(|| tags.and_then(|t| t.album.clone())),
            art_url: session.hints.art_url.clone(),
            duration_ms: found
                .and_then(|info| info.duration_ms)
                .or(session.hints.duration_ms),
            state: session.state,
            via: session.via.clone(),
        };
        // Refused only when the group stopped playing the player between the
        // two looks; `reconcile` then gives the player back.
        let _ = self.state.set_now_playing(&group, Some(record));
    }
}
