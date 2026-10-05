//! Announcements: a clip from the home automation's own address, mixed over
//! what a room or a group plays and then gone (goal 18, ADR 0136; the mix:
//! ADR 0173 for the arithmetic, ADR 0175 for where it runs and how long it
//! takes).
//!
//! The `announce` command names a target, a URL and, optionally, a volume.
//! Brief section 4.8 names "HA's media and TTS URLs from HA's own address"
//! as one of the input paths that may make the server fetch a URL, so the
//! URL has to come from an origin the server was started with
//! (`--announce-origin`), the room model checks that
//! (`Zones::announce_check`), and the fetch itself is held to those origins
//! at every connection it makes (`chorus_fetch::Policy::origins`), so a
//! redirect cannot take it anywhere else.
//!
//! # Duck, mix, restore
//!
//! On a server with stream slots the clip is **mixed over** what its rooms
//! play. It plays through a held player session that no group plays
//! ([`PlayerSessions::play_over`], owner `announce:<n>`, `via` `announce`)
//! into an announcement mix (`crate::mixer`): one more stream on the slots'
//! grid, which is the rooms' group's own stream with the music ducked and
//! the clip over it. The conductor routes the player sessions of the rooms
//! that hear the clip to that mix and tells the audio thread to start
//! ([`Announcer::direct`]); the rest of their group stays where it is. So a
//! room target ducks one room of a playing group, a group target ducks every
//! room of it, and no group's source changes. When the clip ends, fails,
//! runs past its bound or is displaced, the music comes back up, each room's
//! volume goes back when the command set one, and once the restore is
//! complete the sessions go back to their group's slot.
//!
//! # Pause, for a Spotify receiver
//!
//! A group whose source is a Soloist receiver is never mixed with a clip
//! (P7, the owner's answer of 2026-10-04: "announcements pause a Soloist
//! source rather than duck it"). For such a group, and on a server with no
//! stream slots, the clip **interrupts** as it did before the mixer: it
//! plays through a held session as the source of the target's group
//! ([`PlayerSessions::play_held`]), the receiver's manager pauses the
//! receiver the moment its group stops playing it, and afterwards the group
//! plays what it played, except that a player or a receiver does not come
//! back by itself ([`back_to`]).
//!
//! # How it ended
//!
//! Every announcement has a number, the `announcement` member of its
//! command's answer, and the state lists it under `announcements` with
//! where it is: `playing`, then `finished` (the clip played to its end),
//! `failed` (it could not be fetched or decoded, or was cut at the bound) or
//! `displaced` (an alarm, a later announcement or a regrouping took its
//! rooms). The last [`ENDED_KEPT`] that are over stay listed, so a caller
//! that watches the state stream sees its own end.
//!
//! Two threads use it. A control worker starts an announcement, inside the
//! command, so a refusal (no player free, no players at all) is the
//! command's answer. The conductor directs and ends it
//! ([`Announcer::direct`], [`Announcer::settle`], once per pass): it is the
//! thread that takes the players' ends and moves the sessions. One mutex
//! holds the table, and it is never held while the room model's lock is
//! wanted by the other side: a start holds it across the play, which takes
//! the model's lock only for moments, and `settle` takes the model's lock
//! only inside its own calls.
//!
//! Control code, off the audio path: `audio-path.conf` records it as
//! excluded. It touches no PCM; what it sends the audio thread is a
//! [`MixCommand`]. The one clock it reads is the monotonic one, for the
//! bound on a clip's length.

use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use chorus_control::catalog::{Refusal, Volume};
use chorus_control::rooms::{Origin, Source};
use chorus_control::zones::{Announced, Announcement, AnnouncementState};

use crate::control::{ControlState, Snapshot};
use crate::mixer::{MixCommand, MixPort};
use crate::player::player_id;
use crate::playersessions::{Metadata, PlayRefused, PlayRequest, PlayerSessions};
use crate::slots::SlotCommand;

/// (goal 18) A server's `id`, from its long-term public key: `chorus-server-`
/// and the 16 hexadecimal digits of the key's fingerprint (the first 8 bytes
/// of its SHA-256, `chorus_protocol::v2::noise::fingerprint`, without the
/// colons). 30 characters of lower-case letters, digits and hyphens. It is
/// the same for as long as the key is: across restarts with an identity
/// directory, and new at every start with `--ephemeral-identity`.
pub fn server_id(public: &[u8; chorus_protocol::v2::noise::KEY_LEN]) -> String {
    format!(
        "chorus-server-{}",
        chorus_protocol::v2::noise::fingerprint(public).replace(':', "")
    )
}

/// (goal 18) The control service's TXT record: `v=<catalog version>`, then
/// `id=<the server's id>`, in that order (`docs/control-plane.md`,
/// "Discovery"). `id` is how a controller that already knows this server
/// recognises it at a new address.
pub fn control_txt(server_id: &str) -> Vec<(String, String)> {
    vec![
        ("v".to_string(), chorus_control::CATALOG_VERSION.to_string()),
        ("id".to_string(), server_id.to_string()),
    ]
}

/// The longest a clip may play before it is cut and the group restored.
/// ASSUMED: 10 minutes, far above any spoken announcement and short enough
/// that a URL that turns out to be an endless stream does not hold a room.
pub const MAX_CLIP: Duration = Duration::from_secs(600);

/// How many announcements that are over the state keeps listing. ASSUMED:
/// eight, more than a house announces at once, so a caller that looks a
/// moment late still finds its own.
pub const ENDED_KEPT: usize = 8;

/// How long a mix may sit restored under a clip that is still playing
/// before it is told to start again. ASSUMED: half a second. It happens
/// only when a clip is replaced in the last moments of the one before (the
/// audio thread saw that one's end before the player was reloaded).
const RESTART_AFTER: Duration = Duration::from_millis(500);

/// One announcement that was displaced (its group was given something else
/// to play by an alarm, an autoplay or a person), for the schedule runtime:
/// a room the runtime holds must not be put back on the announcement's
/// player, nor at the announcement's volume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Displaced {
    /// The player source the announcement had given its group (an
    /// announcement that was mixed over its rooms gave it to nobody).
    pub player: Source,
    /// What the group goes back to instead.
    pub previous: Source,
    /// The room, the volume it had, the volume the announcement gave it.
    pub volumes: Vec<(String, Volume, Volume)>,
}

/// The audio thread's side of the announcement mixes, as the announcer
/// holds it: where the commands go and what each mix says back.
pub struct Mixes {
    commands: SyncSender<SlotCommand>,
    ports: Vec<Arc<MixPort>>,
}

impl Mixes {
    /// The mixes behind `ports`, told through `commands` (the slots' own
    /// command channel).
    pub fn new(commands: SyncSender<SlotCommand>, ports: Vec<Arc<MixPort>>) -> Mixes {
        Mixes { commands, ports }
    }
}

/// How a mixed announcement is ending.
#[derive(Debug, Clone, Copy)]
struct Ending {
    /// What the audio thread is told.
    command: MixCommand,
    /// Whether it was.
    sent: bool,
    /// Whether the session is still held and is let go once the restore
    /// is complete (a clip that fades with it needs its player until then).
    release: bool,
}

/// The mixed half of an announcement.
#[derive(Debug)]
struct Over {
    mix: usize,
    /// The group its rooms were in when it started.
    group: String,
    /// The rooms that hear it.
    rooms: Vec<String>,
    /// Whether the audio thread was told to start this clip.
    started: bool,
    /// The base slot the audio thread was last told.
    base: Option<usize>,
    ending: Option<Ending>,
    /// Since when the mix has been seen restored under a playing clip.
    restored_since: Option<Instant>,
}

#[derive(Debug)]
struct Live {
    id: u64,
    owner: String,
    player: usize,
    /// What the group played before the first clip of this run.
    previous: Source,
    volumes: Vec<(String, Volume, Volume)>,
    since: Instant,
    /// `Some` for an announcement mixed over its rooms; `None` for one that
    /// interrupts its group.
    over: Option<Over>,
}

#[derive(Debug, Default)]
struct Table {
    /// Announcements started, for their numbers.
    started: u64,
    live: Vec<Live>,
    /// Commands sent to each mix, over the run.
    sent: Vec<u64>,
    /// What the state lists: every one that is playing, and the last
    /// [`ENDED_KEPT`] that are over.
    records: Vec<Announcement>,
}

impl Table {
    /// Say how announcement `id` ended. The ones that are over are listed
    /// after the ones that play, in the order they ended, and the oldest of
    /// them past [`ENDED_KEPT`] are forgotten.
    fn record(&mut self, id: u64, state: AnnouncementState, reason: Option<String>) {
        if let Some(at) = self.records.iter().position(|r| r.id == id) {
            let mut record = self.records.remove(at);
            record.state = state;
            record.reason = reason;
            self.records.push(record);
        }
        let over = self
            .records
            .iter()
            .filter(|r| r.state != AnnouncementState::Playing)
            .count();
        let mut drop = over.saturating_sub(ENDED_KEPT);
        self.records.retain(|r| {
            if drop > 0 && r.state != AnnouncementState::Playing {
                drop -= 1;
                false
            } else {
                true
            }
        });
    }
}

/// The server's announcements.
pub struct Announcer {
    sessions: Option<Arc<PlayerSessions>>,
    /// The configured origins, as the fetcher reads them.
    origins: Vec<chorus_fetch::Origin>,
    /// The announcement mixes, on a server with stream slots and players.
    mixes: Option<Mixes>,
    table: Mutex<Table>,
    max_clip: Duration,
    log: Box<dyn Fn(&str) + Send + Sync>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// What the group goes back to after an announcement that interrupted it:
/// what it played, except a source that plays in one group at a time and is
/// let go the moment its group stops playing it. A player is given back then
/// (a cast's session ends there) and a Spotify receiver is paused by its
/// manager, so going back to either would leave the group on a source
/// nothing drives: it plays `none` instead.
fn back_to(previous: &Source) -> Source {
    if previous.is_exclusive() {
        Source::None
    } else {
        previous.clone()
    }
}

/// The command's answer: the state, with the announcement's number as its
/// last member.
fn answer(state: String, id: u64) -> String {
    match state.strip_suffix('}') {
        Some(open) => format!("{},\"announcement\":{}}}", open, id),
        None => state,
    }
}

fn no_players() -> Refusal {
    Refusal::rejected(
        "t",
        "no-players: this server was started without --players, so it has nothing to play an \
         announcement with"
            .to_string(),
    )
}

/// The refusal a play that did not start is answered with.
fn not_played(refused: PlayRefused, begun: Option<Result<Announced, Refusal>>) -> Refusal {
    match refused {
        PlayRefused::NoPlayers => no_players(),
        PlayRefused::NoFreePlayer(count) => Refusal::rejected(
            "t",
            format!(
                "no-free-player: all {} of this server's players are in use (a cast, an \
                 alarm's stream, another announcement); nothing was changed",
                count
            ),
        ),
        PlayRefused::Target => {
            Refusal::rejected("target", "the target is not an identifier".to_string())
        }
        PlayRefused::Take(words) => match begun {
            Some(Err(refusal)) => refusal,
            _ => Refusal::rejected("t", words),
        },
    }
}

impl Announcer {
    /// Announcements over `sessions` (`None` on a server without
    /// `--players`), from `origins` only. `log` gets one line per start and
    /// end.
    pub fn new(
        sessions: Option<Arc<PlayerSessions>>,
        origins: &[Origin],
        log: Box<dyn Fn(&str) + Send + Sync>,
    ) -> Announcer {
        Announcer {
            sessions,
            origins: origins
                .iter()
                .filter_map(|o| chorus_fetch::Origin::parse(&o.literal()).ok())
                .collect(),
            mixes: None,
            table: Mutex::new(Table::default()),
            max_clip: MAX_CLIP,
            log,
        }
    }

    /// The same, with another bound on a clip's length (the tests').
    pub fn with_max_clip(mut self, max_clip: Duration) -> Announcer {
        self.max_clip = max_clip;
        self
    }

    /// Mix announcements over what their rooms play, through `mixes`
    /// (ADR 0175). Without it every announcement interrupts its group.
    pub fn with_mixes(mut self, mixes: Mixes) -> Announcer {
        lock(&self.table).sent = vec![0; mixes.ports.len()];
        self.mixes = Some(mixes);
        self
    }

    /// Whether any announcement is playing or restoring.
    pub fn any_live(&self) -> bool {
        !lock(&self.table).live.is_empty()
    }

    /// Send one command to a mix; whether the audio thread's queue took it.
    fn tell(&self, table: &mut Table, command: MixCommand) -> bool {
        let Some(mixes) = &self.mixes else {
            return false;
        };
        if mixes.commands.try_send(SlotCommand::Mix(command)).is_err() {
            return false;
        }
        if let Some(sent) = table.sent.get_mut(command.mix()) {
            *sent += 1;
        }
        true
    }

    /// Carry out one `announce` command on `state`. Returns the state as it
    /// stands once the announcement has started, with its number as the
    /// `announcement` member; refused by name, with nothing changed, for an
    /// unknown target (`target`), a target an alarm is ringing in
    /// (`target`), a URL whose origin is not configured (`url`), and when
    /// the server runs no player, has none free or has no mix free (`t`).
    pub fn announce(
        &self,
        state: &ControlState,
        target: &str,
        url: &str,
        volume: Option<Volume>,
    ) -> Result<String, Refusal> {
        state.announce_check(target, url)?;
        // The fetcher's own reading of the URL has to agree: it is the one
        // that connects.
        match chorus_fetch::Origin::parse(url) {
            Ok(origin) if self.origins.contains(&origin) => {}
            Ok(_) => {
                return Err(Refusal::rejected(
                    "url",
                    "the fetcher reads this URL's origin as one that is not configured \
                     (--announce-origin)"
                        .to_string(),
                ))
            }
            Err(why) => {
                return Err(Refusal::rejected(
                    "url",
                    format!("the URL cannot be fetched: {}", why),
                ))
            }
        }
        let Some(sessions) = &self.sessions else {
            return Err(no_players());
        };
        let mut table = lock(&self.table);
        let group = state.announce_group(target);
        let (rooms, source) = state.announce_over_plan(target);
        // A Spotify receiver is paused, never ducked (P7), and a group an
        // announcement is already interrupting takes the next one the same
        // way: both go the way of ADR 0136.
        let interrupting = table.live.iter().any(|live| {
            live.over.is_none()
                && group.is_some()
                && state.player_group(&player_id(live.player)) == group
        });
        let mixed =
            self.mixes.is_some() && !interrupting && !source.is_some_and(|s| s.is_soloist());
        if mixed {
            self.announce_over(state, sessions, &mut table, target, url, volume, &rooms)
        } else {
            self.announce_instead(state, sessions, &mut table, target, url, volume)
        }
    }

    fn request(
        &self,
        sessions: &PlayerSessions,
        owner: &str,
        target: &str,
        url: &str,
    ) -> PlayRequest {
        PlayRequest {
            owner: owner.to_string(),
            target: target.to_string(),
            uri: url.to_string(),
            mime: None,
            via: "announce".to_string(),
            epoch: sessions.held_epoch(),
            metadata: Metadata {
                title: Some("Announcement".to_string()),
                ..Metadata::default()
            },
            origins: self.origins.clone(),
        }
    }

    /// An announcement mixed over what its rooms play (ADR 0175).
    #[allow(clippy::too_many_arguments)]
    fn announce_over(
        &self,
        state: &ControlState,
        sessions: &PlayerSessions,
        table: &mut Table,
        target: &str,
        url: &str,
        volume: Option<Volume>,
        rooms: &[String],
    ) -> Result<String, Refusal> {
        let ports = self.mixes.as_ref().map_or(0, |m| m.ports.len());
        // An announcement already playing in some of these rooms, in the
        // group they are in, is replaced on its own mix and player: the
        // duck stays where it is and the new clip plays over it.
        let group = state.announce_group(target);
        let replaced = table.live.iter().position(|live| {
            live.over.as_ref().is_some_and(|over| {
                Some(&over.group) == group.as_ref() && over.rooms.iter().any(|r| rooms.contains(r))
            })
        });
        let (owner, mix) = match replaced {
            Some(at) => (
                table.live[at].owner.clone(),
                table.live[at].over.as_ref().map_or(0, |over| over.mix),
            ),
            None => {
                let free = (0..ports).find(|m| {
                    !table
                        .live
                        .iter()
                        .any(|live| live.over.as_ref().is_some_and(|over| over.mix == *m))
                });
                let Some(free) = free else {
                    return Err(Refusal::rejected(
                        "t",
                        format!(
                            "no-free-mix: all {} of this server's announcement mixes are in use \
                             (an announcement in other rooms, or one whose music is still coming \
                             back); nothing was changed",
                            ports
                        ),
                    ));
                };
                (format!("announce:{}", table.started + 1), free)
            }
        };
        let request = self.request(sessions, &owner, target, url);
        let mut begun: Option<Result<Announced, Refusal>> = None;
        let played = sessions.play_over(&request, &mut |_| {
            let outcome = state.announce_over_begin(target, volume);
            let said = outcome
                .as_ref()
                .map(|_| ())
                .map_err(|refusal| refusal.to_string());
            begun = Some(outcome);
            said
        });
        let player = match played {
            Ok(player) => player,
            Err(refused) => return Err(not_played(refused, begun)),
        };
        let Some(Ok(announced)) = begun else {
            return Err(Refusal::rejected(
                "t",
                "the announcement did not start".to_string(),
            ));
        };
        table.started += 1;
        let id = table.started;
        let mut volumes = announced.volumes;
        let mut heard = announced.rooms;
        if let Some(at) = replaced {
            let old = table.live.remove(at);
            // A room both clips set goes back to what it had before the
            // first, unless somebody changed it in between.
            for (room, before, _) in volumes.iter_mut() {
                if let Some((_, first, set)) = old.volumes.iter().find(|(r, _, _)| r == room) {
                    if set == before {
                        *before = *first;
                    }
                }
            }
            for kept in old.volumes {
                if !volumes.iter().any(|(r, _, _)| *r == kept.0) {
                    volumes.push(kept);
                }
            }
            // The rooms of the one replaced keep hearing the mix: they are
            // ducked already, and going back to the music now would be a
            // jump in level.
            // (One that was already over, its music still coming back,
            // keeps the outcome it had.)
            if let Some(over) = old.over {
                for room in over.rooms {
                    if !heard.contains(&room) {
                        heard.push(room);
                    }
                }
                if over.ending.is_none() {
                    table.record(
                        old.id,
                        AnnouncementState::Displaced,
                        Some(format!("replaced by announcement {}", id)),
                    );
                }
            }
        }
        // A room another live announcement holds (in another group, or one
        // it set the volume of) is this one's now.
        for other in table.live.iter_mut() {
            if let Some(over) = other.over.as_mut() {
                over.rooms.retain(|room| !heard.contains(room));
            }
            other.volumes.retain(|(room, first, set)| {
                match volumes.iter_mut().find(|(r, _, _)| r == room) {
                    Some((_, before, _)) => {
                        if before == set {
                            *before = *first;
                        }
                        false
                    }
                    None => true,
                }
            });
        }
        (self.log)(&format!(
            "announce owner={} id={} target={} group={} plays=player:{} previous={} mix={} \
             rooms={} volume={}{}",
            owner,
            id,
            target,
            announced.group,
            player_id(player),
            announced.previous.literal(),
            mix,
            heard.join(","),
            volume.map_or("unchanged".to_string(), |v| v.literal()),
            if replaced.is_some() {
                " replaces=the-one-playing"
            } else {
                ""
            }
        ));
        table.records.push(Announcement {
            id,
            target: target.to_string(),
            rooms: heard.clone(),
            state: AnnouncementState::Playing,
            reason: None,
        });
        table.live.push(Live {
            id,
            owner,
            player,
            previous: announced.previous,
            volumes,
            since: Instant::now(),
            over: Some(Over {
                mix,
                group: announced.group,
                rooms: heard,
                started: false,
                base: None,
                ending: None,
                restored_since: None,
            }),
        });
        state.set_announcements(table.records.clone());
        // Read before the table is let go: the conductor cannot end this
        // announcement (a clip that fails at once) before its answer is made.
        Ok(answer(state.encoded_state(), id))
    }

    /// An announcement that interrupts its group (ADR 0136): the clip is the
    /// group's source for as long as it plays.
    fn announce_instead(
        &self,
        state: &ControlState,
        sessions: &PlayerSessions,
        table: &mut Table,
        target: &str,
        url: &str,
        volume: Option<Volume>,
    ) -> Result<String, Refusal> {
        // An announcement already playing in the target's group is replaced
        // on its own player, and what the group goes back to stays what it
        // was before the first of them.
        let group = state.announce_group(target);
        // (Whether or not its clip is still playing: one that just ended and
        // was not settled yet is replaced the same way, so the group never
        // goes back to that one's own player.)
        let replaced = table.live.iter().position(|live| {
            live.over.is_none()
                && group.is_some()
                && state.player_group(&player_id(live.player)) == group
        });
        let owner = match replaced {
            Some(at) => table.live[at].owner.clone(),
            None => format!("announce:{}", table.started + 1),
        };
        let request = self.request(sessions, &owner, target, url);
        let mut begun: Option<Result<Announced, Refusal>> = None;
        let played = sessions.play_held(&request, &mut |source| {
            let Some(player) = Source::parse(source) else {
                return Err(format!("'{}' is not a source", source));
            };
            let outcome = state.announce_begin(target, player, volume);
            let said = outcome
                .as_ref()
                .map(|_| ())
                .map_err(|refusal| refusal.to_string());
            begun = Some(outcome);
            said
        });
        let player = match played {
            Ok(player) => player,
            Err(refused) => return Err(not_played(refused, begun)),
        };
        let Some(Ok(announced)) = begun else {
            // `play_held` answered `Ok` only because the closure did.
            return Err(Refusal::rejected(
                "t",
                "the announcement did not start".to_string(),
            ));
        };
        table.started += 1;
        let id = table.started;
        let mut volumes = announced.volumes;
        let previous = match replaced {
            Some(at) => {
                let old = table.live.remove(at);
                // A room both clips set goes back to what it had before the
                // first, unless somebody changed it in between.
                for (room, before, _) in volumes.iter_mut() {
                    if let Some((_, first, set)) = old.volumes.iter().find(|(r, _, _)| r == room) {
                        if set == before {
                            *before = *first;
                        }
                    }
                }
                for kept in old.volumes {
                    if !volumes.iter().any(|(r, _, _)| *r == kept.0) {
                        volumes.push(kept);
                    }
                }
                table.record(
                    old.id,
                    AnnouncementState::Displaced,
                    Some(format!("replaced by announcement {}", id)),
                );
                old.previous
            }
            None => announced.previous,
        };
        // A room another live announcement set (it was moved into this
        // target's group by the take of a saved group) goes back to what it
        // had before that one; this one answers for it now.
        for other in table.live.iter_mut() {
            other.volumes.retain(|(room, first, set)| {
                match volumes.iter_mut().find(|(r, _, _)| r == room) {
                    Some((_, before, _)) => {
                        if before == set {
                            *before = *first;
                        }
                        false
                    }
                    None => true,
                }
            });
        }
        (self.log)(&format!(
            "announce owner={} id={} target={} group={} plays=player:{} previous={} \
             instead-of-the-music volume={}{}",
            owner,
            id,
            target,
            announced.group,
            player_id(player),
            previous.literal(),
            volume.map_or("unchanged".to_string(), |v| v.literal()),
            if replaced.is_some() {
                " replaces=the-one-playing"
            } else {
                ""
            }
        ));
        table.records.push(Announcement {
            id,
            target: target.to_string(),
            rooms: announced.rooms,
            state: AnnouncementState::Playing,
            reason: None,
        });
        table.live.push(Live {
            id,
            owner,
            player,
            previous,
            volumes,
            since: Instant::now(),
            over: None,
        });
        state.set_announcements(table.records.clone());
        // Read before the table is let go: the conductor cannot end this
        // announcement (a clip that fails at once) before its answer is made.
        Ok(answer(state.encoded_state(), id))
    }

    /// The mix each room of a mixed announcement hears, for the conductor's
    /// routing: a room no announcement plays in is not listed. A room is on
    /// its mix from the command to the end of the restore.
    pub fn routes(&self) -> Vec<(String, usize)> {
        let table = lock(&self.table);
        let mut routes: Vec<(String, usize)> = Vec::new();
        for over in table.live.iter().filter_map(|live| live.over.as_ref()) {
            for room in &over.rooms {
                routes.retain(|(r, _)| r != room);
                routes.push((room.clone(), over.mix));
            }
        }
        routes
    }

    /// Tell the audio thread what each mixed announcement needs, on the
    /// conductor's thread, AFTER the pass has moved the sessions onto their
    /// mixes (so the first frame of the duck is heard by the rooms it is
    /// for): the start of a clip, with the slot its rooms' group plays on,
    /// and that slot again whenever it changes. Returns how many commands
    /// the audio thread's queue did not take; they are sent on the next
    /// pass.
    pub fn direct(&self, snapshot: &Snapshot) -> usize {
        let Some(mixes) = &self.mixes else {
            return 0;
        };
        let mut table = lock(&self.table);
        let mut owed = 0;
        for at in 0..table.live.len() {
            let live = &table.live[at];
            let Some(over) = live.over.as_ref() else {
                continue;
            };
            if over.ending.is_some() {
                continue;
            }
            let (mix, player) = (over.mix, live.player);
            // A Soloist receiver is never the music of a mix (P7).
            let base = snapshot.slots.iter().position(|slot| {
                slot.as_ref()
                    .is_some_and(|g| g.group == over.group && !g.source.is_soloist())
            });
            let busy = mixes.ports.get(mix).is_some_and(|port| {
                port.busy() || port.applied() < table.sent.get(mix).copied().unwrap_or(0)
            });
            let restart = over.started
                && !busy
                && over
                    .restored_since
                    .is_some_and(|since| since.elapsed() >= RESTART_AFTER);
            if !over.started || restart {
                let told = self.tell(&mut table, MixCommand::Start { mix, base, player });
                if let Some(over) = table.live[at].over.as_mut() {
                    over.started |= told;
                    if told {
                        over.base = base;
                        over.restored_since = None;
                    }
                }
                owed += usize::from(!told);
            } else if over.base != base {
                let told = self.tell(&mut table, MixCommand::Base { mix, base });
                if let Some(over) = table.live[at].over.as_mut() {
                    if told {
                        over.base = base;
                    }
                }
                owed += usize::from(!told);
            } else if let Some(over) = table.live[at].over.as_mut() {
                over.restored_since = match (busy, over.restored_since) {
                    (true, _) => None,
                    (false, None) => Some(Instant::now()),
                    (false, since) => since,
                };
            }
        }
        owed
    }

    /// End every announcement that is over, on the conductor's thread, once
    /// per pass. One that was mixed over its rooms: when its clip ended or
    /// failed, ran past the bound or lost its rooms (an alarm rings in one,
    /// they were regrouped, their group took a Spotify receiver), the audio
    /// thread is told to bring the music back, each room's volume goes back
    /// and the state says how it ended; it is let go once the restore is
    /// complete. One that interrupted its group has the group put back on
    /// what it played and its rooms on the volumes they had; one whose
    /// group was given something else meanwhile is only let go (its player
    /// given back, its volumes put back where nobody changed them). Every
    /// displaced one is returned, for the schedule runtime to be told.
    pub fn settle(&self, state: &ControlState) -> Vec<Displaced> {
        let Some(sessions) = &self.sessions else {
            return Vec::new();
        };
        let mut table = lock(&self.table);
        let before = table.records.clone();
        let mut displaced = Vec::new();
        let mut at = 0;
        while at < table.live.len() {
            let gone = if table.live[at].over.is_some() {
                self.settle_over(state, sessions, &mut table, at, &mut displaced)
            } else {
                self.settle_instead(state, sessions, &mut table, at, &mut displaced)
            };
            if !gone {
                at += 1;
            }
        }
        if table.records != before {
            state.set_announcements(table.records.clone());
        }
        displaced
    }

    /// One pass over the mixed announcement at `at`; whether it was let go.
    fn settle_over(
        &self,
        state: &ControlState,
        sessions: &PlayerSessions,
        table: &mut Table,
        at: usize,
        displaced: &mut Vec<Displaced>,
    ) -> bool {
        let live = &table.live[at];
        let Some(over) = live.over.as_ref() else {
            return false;
        };
        let (id, mix, started) = (live.id, over.mix, over.started);
        if over.ending.is_none() {
            let watch = state.announce_watch(&over.rooms, &over.group);
            let playing = sessions.player_of(&live.owner) == Some(live.player)
                && sessions.in_session(&live.owner);
            let cancel = |fade| MixCommand::Cancel { mix, fade };
            // (why, how it ended, in whose words, what the mix is told,
            // whether the session is still to be released)
            let end = if let Some(alarm) = &watch.ringing {
                Some((
                    "displaced",
                    AnnouncementState::Displaced,
                    Some(format!("alarm '{}' rings in its rooms", alarm)),
                    cancel(true),
                    true,
                ))
            } else if watch.here.is_empty() {
                Some((
                    "displaced",
                    AnnouncementState::Displaced,
                    Some("its rooms were regrouped or taken by another announcement".to_string()),
                    cancel(true),
                    true,
                ))
            } else if watch.source.is_soloist() {
                Some((
                    "displaced",
                    AnnouncementState::Displaced,
                    Some(
                        "its group plays a Spotify receiver, which is never mixed with a clip"
                            .to_string(),
                    ),
                    cancel(true),
                    true,
                ))
            } else if !playing {
                match sessions.last_failure(live.player) {
                    Some(reason) => Some((
                        "ended",
                        AnnouncementState::Failed,
                        Some(reason),
                        cancel(false),
                        false,
                    )),
                    None => Some((
                        "ended",
                        AnnouncementState::Finished,
                        None,
                        MixCommand::Finish { mix },
                        false,
                    )),
                }
            } else if live.since.elapsed() >= self.max_clip {
                Some((
                    "cut-at-the-bound",
                    AnnouncementState::Failed,
                    Some(format!(
                        "cut at the bound of {} s on a clip's length",
                        self.max_clip.as_secs()
                    )),
                    cancel(true),
                    true,
                ))
            } else {
                None
            };
            let Some((why, how, reason, command, release)) = end else {
                // A room that left the group (a person moved it) hears the
                // announcement no more and gets its volume back.
                if watch.here.len() != over.rooms.len() {
                    let left: Vec<(String, Volume, Volume)> = live
                        .volumes
                        .iter()
                        .filter(|(room, _, _)| !watch.here.contains(room))
                        .cloned()
                        .collect();
                    state.announce_end(
                        &Source::Player(player_id(live.player)),
                        &Source::None,
                        &left,
                    );
                    let live = &mut table.live[at];
                    live.volumes
                        .retain(|(room, _, _)| watch.here.contains(room));
                    if let Some(over) = live.over.as_mut() {
                        over.rooms = watch.here;
                    }
                }
                return false;
            };
            // The volumes go back as the music starts to: it comes up at
            // the level the rooms had.
            let player = Source::Player(player_id(live.player));
            state.announce_end(&player, &Source::None, &live.volumes);
            (self.log)(&format!(
                "announce owner={} id={} {} group={} restored={} outcome={}{}",
                live.owner,
                id,
                why,
                over.group,
                watch.source.literal(),
                how.name(),
                match (&reason, how) {
                    (Some(reason), AnnouncementState::Failed) => format!(" failure=\"{}\"", reason),
                    (Some(reason), _) => format!(" reason=\"{}\"", reason),
                    (None, _) => String::new(),
                }
            ));
            if how == AnnouncementState::Displaced {
                displaced.push(Displaced {
                    player,
                    previous: watch.source,
                    volumes: live.volumes.clone(),
                });
            }
            table.record(id, how, reason);
            if let Some(over) = table.live[at].over.as_mut() {
                over.ending = Some(Ending {
                    command,
                    // A mix that was never started has nothing to end.
                    sent: !started,
                    release,
                });
            }
        }
        let Some(mut ending) = table.live[at].over.as_ref().and_then(|over| over.ending) else {
            return false;
        };
        if !ending.sent {
            ending.sent = self.tell(table, ending.command);
            if let Some(over) = table.live[at].over.as_mut() {
                over.ending = Some(ending);
            }
        }
        let restored = self
            .mixes
            .as_ref()
            .and_then(|mixes| mixes.ports.get(mix))
            .is_none_or(|port| {
                !port.busy() && port.applied() >= table.sent.get(mix).copied().unwrap_or(0)
            });
        if !(ending.sent && restored) {
            return false;
        }
        let live = table.live.remove(at);
        if ending.release {
            sessions.release_held(&live.owner);
        }
        (self.log)(&format!(
            "announce owner={} id={} music-restored mix={}",
            live.owner, live.id, mix
        ));
        true
    }

    /// One pass over the interrupting announcement at `at`; whether it was
    /// let go.
    fn settle_instead(
        &self,
        state: &ControlState,
        sessions: &PlayerSessions,
        table: &mut Table,
        at: usize,
        displaced: &mut Vec<Displaced>,
    ) -> bool {
        let live = &table.live[at];
        let id = player_id(live.player);
        let playing = sessions.player_of(&live.owner) == Some(live.player)
            && sessions.in_session(&live.owner);
        let group = state.player_group(&id);
        let why = match (&group, playing) {
            (None, _) => "displaced",
            (Some(_), false) => "ended",
            (Some(_), true) if live.since.elapsed() >= self.max_clip => "cut-at-the-bound",
            (Some(_), true) => return false,
        };
        let live = table.live.remove(at);
        // A no-op unless the session is still there (the bound, or a
        // group that moved on before the sessions noticed).
        sessions.release_held(&live.owner);
        let player = Source::Player(id.clone());
        let back = back_to(&live.previous);
        let restored = state.announce_end(&player, &back, &live.volumes);
        let failure = if why == "ended" {
            sessions.last_failure(live.player)
        } else {
            None
        };
        let (how, reason) = match (why, &failure) {
            ("displaced", _) => (
                AnnouncementState::Displaced,
                Some("its group was given something else to play".to_string()),
            ),
            ("ended", None) => (AnnouncementState::Finished, None),
            ("ended", Some(reason)) => (AnnouncementState::Failed, Some(reason.clone())),
            _ => (
                AnnouncementState::Failed,
                Some(format!(
                    "cut at the bound of {} s on a clip's length",
                    self.max_clip.as_secs()
                )),
            ),
        };
        (self.log)(&format!(
            "announce owner={} id={} {} group={} restored={} outcome={}{}",
            live.owner,
            live.id,
            why,
            group.as_deref().unwrap_or("none"),
            match &restored {
                Some(source) => source.literal(),
                None => "nothing".to_string(),
            },
            how.name(),
            match &failure {
                Some(reason) => format!(" failure=\"{}\"", reason),
                None => String::new(),
            }
        ));
        table.record(live.id, how, reason);
        if why == "displaced" {
            displaced.push(Displaced {
                player,
                previous: back,
                volumes: live.volumes,
            });
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_servers_id_is_its_keys_fingerprint_and_a_valid_identifier() {
        let key = [7u8; chorus_protocol::v2::noise::KEY_LEN];
        let id = server_id(&key);
        let fingerprint = chorus_protocol::v2::noise::fingerprint(&key);
        assert_eq!(
            id,
            format!("chorus-server-{}", fingerprint.replace(':', ""))
        );
        assert_eq!(id.len(), 30);
        assert!(chorus_control::catalog::is_server_id(&id), "{id}");
        // The same key, the same id; another key, another id.
        assert_eq!(server_id(&key), id);
        assert_ne!(server_id(&[8u8; chorus_protocol::v2::noise::KEY_LEN]), id);
    }

    #[test]
    fn the_control_txt_record_is_the_version_then_the_id() {
        assert_eq!(
            control_txt("chorus-server-0123456789abcdef"),
            [
                ("v".to_string(), "2".to_string()),
                (
                    "id".to_string(),
                    "chorus-server-0123456789abcdef".to_string()
                ),
            ]
        );
    }

    #[test]
    fn the_answer_is_the_state_with_the_announcements_number_last() {
        assert_eq!(
            answer(r#"{"v":2,"t":"state","serial":4}"#.to_string(), 7),
            r#"{"v":2,"t":"state","serial":4,"announcement":7}"#
        );
    }

    #[test]
    fn the_state_keeps_every_playing_announcement_and_the_last_few_that_are_over() {
        let mut table = Table::default();
        for id in 1..=(ENDED_KEPT as u64 + 3) {
            table.records.push(Announcement {
                id,
                target: "kitchen".to_string(),
                rooms: vec!["kitchen".to_string()],
                state: AnnouncementState::Playing,
                reason: None,
            });
        }
        // Every one but the first ends: the first is still listed, and of
        // the others the oldest are dropped.
        for id in 2..=(ENDED_KEPT as u64 + 3) {
            table.record(id, AnnouncementState::Finished, None);
        }
        let listed: Vec<u64> = table.records.iter().map(|r| r.id).collect();
        assert_eq!(listed.len(), ENDED_KEPT + 1);
        assert_eq!(listed[0], 1);
        assert_eq!(listed[1], 4);
        table.record(
            1,
            AnnouncementState::Failed,
            Some("http status 404".to_string()),
        );
        let listed: Vec<u64> = table.records.iter().map(|r| r.id).collect();
        assert_eq!(listed.len(), ENDED_KEPT);
        assert_eq!(listed[0], 5, "the one that ended longest ago is forgotten");
        assert_eq!(listed[ENDED_KEPT - 1], 1, "in the order they ended");
    }

    #[test]
    fn a_group_never_goes_back_to_a_player_or_a_receiver() {
        assert_eq!(back_to(&Source::Player("p0".to_string())), Source::None);
        assert_eq!(back_to(&Source::Soloist("r0".to_string())), Source::None);
        assert_eq!(
            back_to(&Source::Chime("bell".to_string())),
            Source::Chime("bell".to_string())
        );
        assert_eq!(back_to(&Source::Stream), Source::Stream);
        assert_eq!(back_to(&Source::None), Source::None);
    }
}
