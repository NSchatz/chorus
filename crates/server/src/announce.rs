//! Announcements: a clip from the home automation's own address, played in a
//! room or a group and then gone (goal 18, ADR 0133).
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
//! What this module does is deliberately small: **interrupt and restore**.
//! The clip plays through a held player session
//! ([`PlayerSessions::play_held`], owner `announce:<n>`, `via` `announce`) as
//! the source of the target's group, exactly the way a ringing alarm's
//! stored URL plays, and when the clip ends, fails, runs past its bound or
//! is displaced, what the group played before comes back, and so does each
//! room's volume when the command set one. There is no mixing and no
//! ducking: the ducking mixer is a later goal's (K31), which replaces the
//! playback here and keeps the command.
//!
//! Two threads use it. A control worker starts an announcement, inside the
//! command, so a refusal (no player free, no players at all) is the
//! command's answer. The conductor ends it ([`Announcer::settle`], once per
//! pass): it is the thread that takes the players' ends. One mutex holds the
//! table, and it is never held while the room model's lock is wanted by the
//! other side: a start holds it across `play_held`, which takes the model's
//! lock only for moments, and `settle` takes the model's lock only inside
//! its own calls.
//!
//! Control code, off the audio path: `audio-path.conf` records it as
//! excluded. The one clock it reads is the monotonic one, for the bound on
//! a clip's length.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use chorus_control::catalog::{Refusal, Volume};
use chorus_control::rooms::{Origin, Source};
use chorus_control::zones::Announced;

use crate::control::ControlState;
use crate::player::player_id;
use crate::playersessions::{Metadata, PlayRefused, PlayRequest, PlayerSessions};

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

/// One announcement that was displaced (its group was given something else
/// to play by an alarm, an autoplay or a person), for the schedule runtime:
/// a room the runtime holds must not be put back on the announcement's
/// player, nor at the announcement's volume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Displaced {
    /// The player source the announcement had given its group.
    pub player: Source,
    /// What the group goes back to instead.
    pub previous: Source,
    /// The room, the volume it had, the volume the announcement gave it.
    pub volumes: Vec<(String, Volume, Volume)>,
}

#[derive(Debug)]
struct Live {
    owner: String,
    player: usize,
    /// What the group played before the first clip of this run.
    previous: Source,
    volumes: Vec<(String, Volume, Volume)>,
    since: Instant,
}

#[derive(Debug, Default)]
struct Table {
    /// Announcements started, for the owner's number.
    started: u64,
    live: Vec<Live>,
}

/// The server's announcements.
pub struct Announcer {
    sessions: Option<Arc<PlayerSessions>>,
    /// The configured origins, as the fetcher reads them.
    origins: Vec<chorus_fetch::Origin>,
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

/// What the group goes back to: what it played, except a player source,
/// which is given back the moment its group stops playing it (a cast's
/// session ends there), so going back to it would leave the group on a
/// player nothing drives.
fn back_to(previous: &Source) -> Source {
    match previous {
        Source::Player(_) => Source::None,
        other => other.clone(),
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

    /// Whether any announcement is playing.
    pub fn any_live(&self) -> bool {
        !lock(&self.table).live.is_empty()
    }

    /// Carry out one `announce` command on `state`. Returns the state as it
    /// stands once the group plays the clip's player; refused by name, with
    /// nothing changed, for an unknown target (`target`), a target an alarm
    /// is ringing in (`target`), a URL whose origin is not configured
    /// (`url`), and when the server runs no player or has none free (`t`).
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
            return Err(Refusal::rejected(
                "t",
                "no-players: this server was started without --players, so it has nothing to \
                 play an announcement with"
                    .to_string(),
            ));
        };
        let mut table = lock(&self.table);
        // An announcement already playing in the target's group is replaced
        // on its own player, and what the group goes back to stays what it
        // was before the first of them.
        let group = state.announce_group(target);
        let replaced = table.live.iter().position(|live| {
            sessions.in_session(&live.owner)
                && state.player_group(&player_id(live.player)) == group
                && group.is_some()
        });
        let owner = match replaced {
            Some(at) => table.live[at].owner.clone(),
            None => {
                table.started += 1;
                format!("announce:{}", table.started)
            }
        };
        let request = PlayRequest {
            owner: owner.clone(),
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
        };
        let mut begun: Option<Result<Announced, Refusal>> = None;
        let played = sessions.play_held(&request, &mut |source| {
            let Some(player) = Source::parse(source) else {
                return Err(format!("'{}' is not a source", source));
            };
            let outcome = state.announce_begin(target, player, volume);
            let answer = outcome
                .as_ref()
                .map(|_| ())
                .map_err(|refusal| refusal.to_string());
            begun = Some(outcome);
            answer
        });
        let player = match played {
            Ok(player) => player,
            Err(PlayRefused::NoPlayers) => {
                return Err(Refusal::rejected(
                    "t",
                    "no-players: this server was started without --players, so it has nothing \
                     to play an announcement with"
                        .to_string(),
                ))
            }
            Err(PlayRefused::NoFreePlayer(count)) => {
                return Err(Refusal::rejected(
                    "t",
                    format!(
                        "no-free-player: all {} of this server's players are in use (a cast, an \
                         alarm's stream, another announcement); nothing was changed",
                        count
                    ),
                ))
            }
            Err(PlayRefused::Target) => {
                return Err(Refusal::rejected(
                    "target",
                    "the target is not an identifier".to_string(),
                ))
            }
            Err(PlayRefused::Take(words)) => {
                return Err(match begun {
                    Some(Err(refusal)) => refusal,
                    _ => Refusal::rejected("t", words),
                })
            }
        };
        let Some(Ok(announced)) = begun else {
            // `play_held` answered `Ok` only because the closure did.
            return Err(Refusal::rejected(
                "t",
                "the announcement did not start".to_string(),
            ));
        };
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
            "announce owner={} target={} group={} plays=player:{} previous={} volume={}{}",
            owner,
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
        table.live.push(Live {
            owner,
            player,
            previous,
            volumes,
            since: Instant::now(),
        });
        // Read before the table is let go: the conductor cannot end this
        // announcement (a clip that fails at once) before its answer is made.
        let answer = state.encoded_state();
        drop(table);
        Ok(answer)
    }

    /// End every announcement that is over, on the conductor's thread, once
    /// per pass: one whose clip ended or failed, or ran past the bound, has
    /// its group put back on what it played and its rooms on the volumes
    /// they had; one whose group was given something else meanwhile is only
    /// let go (its player given back, its volumes put back where nobody
    /// changed them) and returned, for the schedule runtime to be told.
    pub fn settle(&self, state: &ControlState) -> Vec<Displaced> {
        let Some(sessions) = &self.sessions else {
            return Vec::new();
        };
        let mut table = lock(&self.table);
        let mut displaced = Vec::new();
        let mut at = 0;
        while at < table.live.len() {
            let live = &table.live[at];
            let id = player_id(live.player);
            let playing = sessions.player_of(&live.owner) == Some(live.player)
                && sessions.in_session(&live.owner);
            let group = state.player_group(&id);
            let why = match (&group, playing) {
                (None, _) => "displaced",
                (Some(_), false) => "ended",
                (Some(_), true) if live.since.elapsed() >= self.max_clip => "cut-at-the-bound",
                (Some(_), true) => {
                    at += 1;
                    continue;
                }
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
            (self.log)(&format!(
                "announce owner={} {} group={} restored={}{}",
                live.owner,
                why,
                group.as_deref().unwrap_or("none"),
                match &restored {
                    Some(source) => source.literal(),
                    None => "nothing".to_string(),
                },
                match &failure {
                    Some(reason) => format!(" failure=\"{}\"", reason),
                    None => String::new(),
                }
            ));
            if why == "displaced" {
                displaced.push(Displaced {
                    player,
                    previous: back,
                    volumes: live.volumes,
                });
            }
        }
        displaced
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
    fn a_group_never_goes_back_to_a_player() {
        assert_eq!(back_to(&Source::Player("p0".to_string())), Source::None);
        assert_eq!(back_to(&Source::Stream), Source::Stream);
        assert_eq!(back_to(&Source::None), Source::None);
    }
}
