//! OpenHome Playlist:1: the queue the device holds and walks itself.
//!
//! ohP `OpenHome/Av/Playlist/ProviderPlaylist.cpp`, `SourcePlaylist.cpp`,
//! `TrackDatabase.cpp` and `UriProviderPlaylist.cpp` at `cccd06dd`. A control
//! point inserts tracks (a URI and its DIDL-Lite, kept verbatim) and says
//! Play; from then on the device moves from track to track by itself, so the
//! phone can sleep. In chorus the tracks join without a gap: the Playlist
//! hands the player the following track ahead of each boundary, exactly as a
//! control point does with `SetNextAVTransportURI`.
//!
//! # How it is built
//!
//! A [`Playlist`] is the list, the cursor (`Id`), Repeat and Shuffle, and a
//! private **deck**: an [`AvTransport`] of its own that no control point
//! sees. Every transport decision goes through the deck (set the URI, set
//! the next URI, play, pause, seek), so the Playlist returns the same
//! [`Effect`]s the AVTransport service does, takes the same player reports
//! with the same epochs, and inherits the gapless handover and its races
//! from the machine goal 16 tested, rather than restating them. The
//! Playlist's own work is deciding which track is current and which follows.
//!
//! # The rules, with where each was read
//!
//! - Ids are `ui4`, 0 means none (`TrackDatabase.cpp:17`); they come from a
//!   counter that starts at 1 and are never used twice in a run.
//! - `Insert(AfterId, Uri, Metadata)`: `AfterId` 0 is the head; 800 "Id not
//!   found"; 801 "Playlist full" (`ProviderPlaylist.cpp:23-26`, `:394-409`,
//!   `TrackDatabase.cpp:126-140`). The first track of an empty list is cued:
//!   `Id` becomes it and nothing plays (`SourcePlaylist.cpp:429-440`).
//! - `DeleteId`: 800; deleting the track that plays moves on to the one
//!   after it; deleting the cued track while nothing plays cues the one
//!   after it; an empty list stops, `Id` 0 (`ProviderPlaylist.cpp:131-140`,
//!   `:411-424`, `SourcePlaylist.cpp:442-449`). `DeleteAll` likewise
//!   (`:142-146`, `SourcePlaylist.cpp:458-467`).
//! - `IdArray`: the ids in list order (not shuffle order), each a big-endian
//!   32-bit number, concatenated, base64; the action also returns a token,
//!   the list's change counter, and `IdArrayChanged(Token)` says whether the
//!   list changed since (`ProviderPlaylist.cpp:462-481`, `:526-536`).
//! - `ReadList(IdList)`: ids separated by spaces; unknown and non-numeric
//!   ones are skipped; the answer is `<TrackList>` of `<Entry>` with `Id`,
//!   `Uri` and `Metadata`, the last two escaped (`:344-392`).
//! - `Play` on an empty list stops; while playing it restarts the current
//!   track; otherwise it plays the current track, the first when none is
//!   (`SourcePlaylist.cpp:273-322`). `Play`, `SeekId` and `SeekIndex` make
//!   the Playlist the device's source; `Pause`, `Stop`, `Next`, `Previous`
//!   and the two second-seeks do nothing while it is not (`:324-401`).
//! - `Next` at the last track with Repeat off, and the natural end of the
//!   last track, cue the first track and stop; `Previous` at the first track
//!   does the same (`UriProviderPlaylist.cpp:120-189`). With Repeat on the
//!   list wraps and plays on.
//! - `SeekId`: 800; `SeekIndex`: 802 "Index not found"; the second-seeks:
//!   803 "Seek failed", a relative seek before the start lands on 0
//!   (`ProviderPlaylist.cpp:240-301`, `SourcePlaylist.cpp:378-392`).
//! - `SetShuffle(1)` with fewer than two tracks: 804
//!   (`ProviderPlaylist.cpp:218-229`).
//! - `TransportState` is `Playing`, `Paused`, `Stopped` or `Buffering`
//!   (`Playlist1.xml`); it is `Stopped` at start and when another source is
//!   selected, and the list is kept then (`ProviderPlaylist.cpp:94`,
//!   `SourcePlaylist.cpp:244-252`).
//!
//! chorus's own, where the reference says nothing chorus can use:
//!
//! - a URI that is not `http://` or `https://` is refused at `Insert` with
//!   600 Argument Value Invalid: the player's fetch policy takes nothing
//!   else (brief section 4.8), and a track that can never play is better
//!   refused than queued;
//! - Shuffle is a permutation of the ids by a seed the server draws when
//!   Shuffle is switched on: every track once, then (with Repeat) the same
//!   order again;
//! - a queued track that cannot be fetched or decoded stops the list on that
//!   track (`Id` names it), as AVTransport stops on a bad next URI.

use super::{bool_text, parse_bool, parse_i4, parse_ui4, Property};
use crate::avtransport::{AvTransport, Effect, TransportState, TransportStatus};
use crate::soap::Invocation;
use crate::xml::escape_text;
use crate::{base64, connmgr, error, time, Outputs, UpnpError};

/// `TracksMax`: 1000, ohPipeline's own (`SourcePlaylist.cpp:125`).
pub const TRACKS_MAX: usize = 1000;

/// How long a changed `IdArray` is held before it is evented, ms
/// (`ProviderPlaylist.h:41`: 300).
pub const ID_ARRAY_HOLD_MS: u64 = 300;

/// The longest URI and metadata an `Insert` takes, bytes each. The server
/// reads no request body over 64 KiB, so this only bounds what one track can
/// hold of that.
pub const MAX_TRACK_TEXT: usize = 32 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Track {
    id: u32,
    uri: String,
    metadata: String,
}

/// The ids as `IdArray` carries them: big-endian 32-bit numbers,
/// concatenated, in base64. An empty list is the empty string.
pub fn encode_ids(ids: &[u32]) -> String {
    let bytes: Vec<u8> = ids.iter().flat_map(|id| id.to_be_bytes()).collect();
    base64::encode(&bytes)
}

/// The ids of an `IdArray` value, or `None` when it is not one.
pub fn decode_ids(text: &str) -> Option<Vec<u32>> {
    let bytes = base64::decode(text)?;
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    Some(
        bytes
            .chunks(4)
            .map(|c| u32::from_be_bytes([c[0], c[1], c[2], c[3]]))
            .collect(),
    )
}

fn is_http(uri: &str) -> bool {
    let lower = uri.get(..8).unwrap_or(uri).to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// SplitMix64's output function (as in [`crate::ssdp::SeededJitter`]): a
/// well-mixed number per id for a seed. Nothing depends on its quality but
/// the order looking shuffled.
fn mix(seed: u64, id: u32) -> u64 {
    let mut z = seed
        .wrapping_add(u64::from(id).wrapping_mul(0x9E37_79B9_7F4A_7C15))
        .wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The Playlist state of one renderer. See the module documentation.
#[derive(Clone, Debug)]
pub struct Playlist {
    tracks: Vec<Track>,
    next_id: u32,
    /// The list's change counter: the `IdArray` token.
    seq: u32,
    /// `Id`: the current track, 0 when the list is empty.
    current: u32,
    repeat: bool,
    shuffle: bool,
    seed: u64,
    /// Whether the Playlist is the device's selected source.
    active: bool,
    deck: AvTransport,
    /// The track the deck holds as its current URI, 0 for none.
    deck_id: u32,
    /// The track the deck holds as its next URI, 0 for none.
    queued_id: u32,
    /// The tracks lately handed to the deck as next, newest last: at a
    /// boundary the player names a URI, and this says which track it was.
    handed: Vec<(u32, String)>,
}

impl Default for Playlist {
    fn default() -> Playlist {
        Playlist::new()
    }
}

impl Playlist {
    /// An empty list, stopped, the selected source.
    pub fn new() -> Playlist {
        Playlist {
            tracks: Vec::new(),
            next_id: 1,
            seq: 0,
            current: 0,
            repeat: false,
            shuffle: false,
            seed: 0,
            active: true,
            deck: AvTransport::new(),
            deck_id: 0,
            queued_id: 0,
            handed: Vec::new(),
        }
    }

    // ----- what the server reads -----

    /// The deck: its state, epoch, current and next URIs, duration and
    /// position are what the server gives the player and Info and Time.
    pub fn deck(&self) -> &AvTransport {
        &self.deck
    }

    /// `Id`.
    pub fn id(&self) -> u32 {
        self.current
    }

    /// The ids in list order.
    pub fn ids(&self) -> Vec<u32> {
        self.tracks.iter().map(|t| t.id).collect()
    }

    /// How many tracks there are.
    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    /// Whether the list is empty.
    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    /// The track that plays after the current one, if any (the next in play
    /// order; the first again after the last with Repeat on).
    pub fn following_in_order(&self) -> Option<u32> {
        self.following(self.current)
    }

    /// Whether the Playlist is the selected source.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// `TransportState`, from the deck: `Buffering` while the player
    /// fetches or seeks, `Stopped` with nothing loaded.
    pub fn transport_state(&self) -> &'static str {
        match self.deck.state() {
            TransportState::Playing => "Playing",
            TransportState::PausedPlayback => "Paused",
            TransportState::Transitioning => "Buffering",
            TransportState::Stopped | TransportState::NoMediaPresent => "Stopped",
        }
    }

    /// Whether an action makes the Playlist the device's source
    /// (`SourcePlaylist.cpp:273-276`, `:394-401`).
    pub fn activates(action: &str) -> bool {
        matches!(action, "Play" | "SeekId" | "SeekIndex")
    }

    /// Every evented variable with its value, in the table's order
    /// (`ProviderPlaylist.cpp:58-64`, `:93-97`). `ProtocolInfo` is
    /// ConnectionManager's sink list (`:120-124`).
    pub fn evented(&self) -> Vec<Property> {
        vec![
            ("TransportState", self.transport_state().to_string()),
            ("Repeat", bool_text(self.repeat)),
            ("Shuffle", bool_text(self.shuffle)),
            ("Id", self.current.to_string()),
            ("IdArray", encode_ids(&self.ids())),
            ("TracksMax", TRACKS_MAX.to_string()),
            ("ProtocolInfo", connmgr::sink_protocol_info()),
        ]
    }

    // ----- the list -----

    fn track(&self, id: u32) -> Option<&Track> {
        self.tracks.iter().find(|t| t.id == id)
    }

    fn has(&self, id: u32) -> bool {
        id != 0 && self.track(id).is_some()
    }

    /// The ids in the order they play: the list's, or the shuffled one.
    fn order(&self) -> Vec<u32> {
        let mut ids = self.ids();
        if self.shuffle {
            ids.sort_by_key(|id| (mix(self.seed, *id), *id));
        }
        ids
    }

    fn first(&self) -> u32 {
        self.order().first().copied().unwrap_or(0)
    }

    /// The track that plays after `id`: the next in play order, the first
    /// again after the last with Repeat on, none after the last otherwise.
    fn following(&self, id: u32) -> Option<u32> {
        let order = self.order();
        let at = order.iter().position(|i| *i == id)?;
        match order.get(at + 1) {
            Some(next) => Some(*next),
            None if self.repeat => order.first().copied(),
            None => None,
        }
    }

    fn preceding(&self, id: u32) -> Option<u32> {
        let order = self.order();
        let at = order.iter().position(|i| *i == id)?;
        if at > 0 {
            Some(order[at - 1])
        } else if self.repeat {
            order.last().copied()
        } else {
            None
        }
    }

    // ----- the deck -----

    fn sounding(&self) -> bool {
        matches!(
            self.deck.state(),
            TransportState::Playing | TransportState::Transitioning
        )
    }

    fn engaged(&self) -> bool {
        self.sounding() || self.deck.state() == TransportState::PausedPlayback
    }

    /// The deck's own LastChange queue has no reader.
    fn settle(&mut self) {
        self.deck.events().discard();
    }

    /// Put track `id` on the deck, not started.
    fn load(&mut self, id: u32) -> Vec<Effect> {
        let Some(track) = self.track(id).cloned() else {
            return vec![];
        };
        self.current = id;
        self.deck_id = id;
        self.queued_id = 0;
        self.deck
            .set_av_transport_uri(&track.uri, &track.metadata)
            .unwrap_or_default()
    }

    /// Make track `id` the current one and play it from its start.
    fn cue_and_play(&mut self, id: u32) -> Vec<Effect> {
        let mut effects = self.load(id);
        effects.extend(self.deck.play("1").unwrap_or_default());
        effects.extend(self.plan_next());
        effects
    }

    /// Stop, and cue the first track (the end of the list with Repeat off).
    fn stop_and_cue_first(&mut self) -> Vec<Effect> {
        let effects = self.deck.stop().unwrap_or_default();
        self.current = self.first();
        effects
    }

    /// Hold the deck's next URI to the track that follows the current one.
    /// Only while the current track is on the deck and engaged: a stopped
    /// deck is planned when it is played.
    fn plan_next(&mut self) -> Vec<Effect> {
        if !self.engaged() || self.deck_id != self.current {
            return vec![];
        }
        let want = self.following(self.current).unwrap_or(0);
        let (uri, metadata) = match self.track(want) {
            Some(t) => (t.uri.clone(), t.metadata.clone()),
            None => (String::new(), String::new()),
        };
        if want == self.queued_id && self.deck.next_queued().0 == uri {
            return vec![];
        }
        self.queued_id = want;
        if want != 0 {
            self.handed.push((want, uri.clone()));
            if self.handed.len() > 4 {
                self.handed.remove(0);
            }
        }
        self.deck
            .set_next_av_transport_uri(&uri, &metadata)
            .unwrap_or_default()
    }

    // ----- the source -----

    /// The Playlist became the device's source.
    pub fn activate(&mut self) {
        self.active = true;
    }

    /// Another source was selected, or the rooms were taken by something
    /// else: the transport stops and the list, `Id` and `IdArray` stay
    /// (`SourcePlaylist.cpp:244-252`). Returns the effects of stopping.
    pub fn deactivate(&mut self) -> Vec<Effect> {
        self.active = false;
        self.halt()
    }

    /// Stop the transport and keep everything else (the player was lost, or
    /// the device entered standby).
    pub fn halt(&mut self) -> Vec<Effect> {
        let effects = self.deck.stop().unwrap_or_default();
        self.settle();
        effects
    }

    // ----- actions -----

    fn play(&mut self) -> Vec<Effect> {
        if self.tracks.is_empty() {
            return self.deck.stop().unwrap_or_default();
        }
        if !self.has(self.current) {
            self.current = self.first();
        }
        let on_deck =
            self.deck_id == self.current && self.deck.state() != TransportState::NoMediaPresent;
        match self.deck.state() {
            // While playing, Play starts the current track again.
            TransportState::Playing => self.cue_and_play(self.current),
            TransportState::Transitioning if on_deck => vec![],
            _ if on_deck => {
                let mut effects = self.deck.play("1").unwrap_or_default();
                effects.extend(self.plan_next());
                effects
            }
            _ => self.cue_and_play(self.current),
        }
    }

    fn step(&mut self, forward: bool) -> Vec<Effect> {
        if !self.active || self.tracks.is_empty() {
            return vec![];
        }
        let to = if forward {
            self.following(self.current)
        } else {
            self.preceding(self.current)
        };
        match to {
            Some(id) => self.cue_and_play(id),
            None => self.stop_and_cue_first(),
        }
    }

    fn seek_second(&mut self, second: u64) -> Result<Vec<Effect>, UpnpError> {
        if !self.active {
            return Ok(vec![]);
        }
        if !self.has(self.current) {
            return Err(error::OH_PLAYLIST_SEEK_FAILED);
        }
        // On a copy: a seek the deck refuses must leave nothing changed.
        let mut trial = self.clone();
        let mut effects = Vec::new();
        if trial.deck_id != trial.current || trial.deck.state() == TransportState::NoMediaPresent {
            effects.extend(trial.load(trial.current));
        }
        effects.extend(
            trial
                .deck
                .seek("REL_TIME", &time::format(second.saturating_mul(1000)))
                .map_err(|_| error::OH_PLAYLIST_SEEK_FAILED)?,
        );
        effects.extend(trial.deck.play("1").unwrap_or_default());
        effects.extend(trial.plan_next());
        *self = trial;
        Ok(effects)
    }

    fn insert(
        &mut self,
        after: u32,
        uri: &str,
        metadata: &str,
    ) -> Result<(u32, Vec<Effect>), UpnpError> {
        let uri = uri.trim();
        if !is_http(uri) || uri.len() > MAX_TRACK_TEXT || metadata.len() > MAX_TRACK_TEXT {
            return Err(error::ARGUMENT_VALUE_INVALID);
        }
        let at = match after {
            0 => 0,
            id => {
                self.tracks
                    .iter()
                    .position(|t| t.id == id)
                    .ok_or(error::OH_PLAYLIST_ID_NOT_FOUND)?
                    + 1
            }
        };
        if self.tracks.len() >= TRACKS_MAX {
            return Err(error::OH_PLAYLIST_FULL);
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        self.tracks.insert(
            at,
            Track {
                id,
                uri: uri.to_string(),
                metadata: metadata.to_string(),
            },
        );
        self.seq = self.seq.wrapping_add(1);
        if self.current == 0 {
            self.current = id;
        }
        Ok((id, self.plan_next()))
    }

    fn clear_deck(&mut self) -> Vec<Effect> {
        self.deck_id = 0;
        self.queued_id = 0;
        self.handed.clear();
        self.deck.set_av_transport_uri("", "").unwrap_or_default()
    }

    fn delete(&mut self, id: u32) -> Result<Vec<Effect>, UpnpError> {
        let at = self
            .tracks
            .iter()
            .position(|t| t.id == id)
            .ok_or(error::OH_PLAYLIST_ID_NOT_FOUND)?;
        let after = self.following(id).filter(|f| *f != id);
        self.tracks.remove(at);
        self.seq = self.seq.wrapping_add(1);
        if self.tracks.is_empty() {
            self.current = 0;
            return Ok(self.clear_deck());
        }
        if id != self.current {
            return Ok(self.plan_next());
        }
        Ok(match (self.sounding(), after) {
            (true, Some(next)) => self.cue_and_play(next),
            (true, None) => self.stop_and_cue_first(),
            (false, after) => {
                let effects = self.deck.stop().unwrap_or_default();
                self.current = after.unwrap_or_else(|| self.first());
                effects
            }
        })
    }

    fn delete_all(&mut self) -> Vec<Effect> {
        if !self.tracks.is_empty() {
            self.tracks.clear();
            self.seq = self.seq.wrapping_add(1);
        }
        self.current = 0;
        self.clear_deck()
    }

    fn read_list(&self, id_list: &str) -> String {
        let mut x = String::from("<TrackList>");
        for id in id_list.split_ascii_whitespace() {
            let Some(track) = id.parse::<u32>().ok().and_then(|id| self.track(id)) else {
                continue;
            };
            x.push_str(&format!(
                "<Entry><Id>{}</Id><Uri>{}</Uri><Metadata>{}</Metadata></Entry>",
                track.id,
                escape_text(&track.uri),
                escape_text(&track.metadata)
            ));
        }
        x.push_str("</TrackList>");
        x
    }

    /// Performs a Playlist action that passed [`crate::soap::validate`].
    /// `seed` is used when Shuffle is switched on (random bytes the server
    /// draws; a test gives a constant). An action that fails leaves the
    /// state as it was.
    pub fn invoke(
        &mut self,
        invocation: &Invocation,
        seed: u64,
    ) -> Result<(Outputs, Vec<Effect>), UpnpError> {
        let value = |v: String| (vec![("Value", v)], vec![]);
        let done = |effects: Vec<Effect>| (Vec::new(), effects);
        let result = match invocation.action.name {
            "Play" => done(self.play()),
            "Pause" if self.active => done(self.deck.pause().unwrap_or_default()),
            "Stop" if self.active => done(self.deck.stop().unwrap_or_default()),
            "Pause" | "Stop" => done(vec![]),
            "Next" => done(self.step(true)),
            "Previous" => done(self.step(false)),
            "SetRepeat" => {
                self.repeat = parse_bool(invocation.input("Value"))?;
                done(self.plan_next())
            }
            "Repeat" => value(bool_text(self.repeat)),
            "SetShuffle" => {
                let on = parse_bool(invocation.input("Value"))?;
                if on && self.tracks.len() < 2 {
                    return Err(error::OH_PLAYLIST_SHUFFLE_NOT_POSSIBLE);
                }
                if on && !self.shuffle {
                    self.seed = seed;
                }
                self.shuffle = on;
                done(self.plan_next())
            }
            "Shuffle" => value(bool_text(self.shuffle)),
            "SeekSecondAbsolute" => {
                let second = parse_ui4(invocation.input("Value"))?;
                done(self.seek_second(u64::from(second))?)
            }
            "SeekSecondRelative" => {
                let offset = i64::from(parse_i4(invocation.input("Value"))?);
                let now = (self.deck.position_ms() / 1000) as i64;
                done(self.seek_second((now + offset).max(0) as u64)?)
            }
            "SeekId" => {
                let id = parse_ui4(invocation.input("Value"))?;
                if !self.has(id) {
                    return Err(error::OH_PLAYLIST_ID_NOT_FOUND);
                }
                done(self.cue_and_play(id))
            }
            "SeekIndex" => {
                let index = parse_ui4(invocation.input("Value"))? as usize;
                let id = self
                    .tracks
                    .get(index)
                    .map(|t| t.id)
                    .ok_or(error::OH_PLAYLIST_INDEX_NOT_FOUND)?;
                done(self.cue_and_play(id))
            }
            "TransportState" => value(self.transport_state().to_string()),
            "Id" => value(self.current.to_string()),
            "Read" => {
                let id = parse_ui4(invocation.input("Id"))?;
                let track = self.track(id).ok_or(error::OH_PLAYLIST_ID_NOT_FOUND)?;
                (
                    vec![
                        ("Uri", track.uri.clone()),
                        ("Metadata", track.metadata.clone()),
                    ],
                    vec![],
                )
            }
            "ReadList" => (
                vec![("TrackList", self.read_list(invocation.input("IdList")))],
                vec![],
            ),
            "Insert" => {
                let after = parse_ui4(invocation.input("AfterId"))?;
                let (id, effects) =
                    self.insert(after, invocation.input("Uri"), invocation.input("Metadata"))?;
                (vec![("NewId", id.to_string())], effects)
            }
            "DeleteId" => {
                let id = parse_ui4(invocation.input("Value"))?;
                done(self.delete(id)?)
            }
            "DeleteAll" => done(self.delete_all()),
            "TracksMax" => value(TRACKS_MAX.to_string()),
            "IdArray" => (
                vec![
                    ("Token", self.seq.to_string()),
                    ("Array", encode_ids(&self.ids())),
                ],
                vec![],
            ),
            "IdArrayChanged" => {
                let token = parse_ui4(invocation.input("Token"))?;
                value(bool_text(token != self.seq))
            }
            "ProtocolInfo" => value(connmgr::sink_protocol_info()),
            _ => return Err(error::INVALID_ACTION),
        };
        self.settle();
        Ok(result)
    }

    // ----- reports from the player (see `crate::avtransport`) -----

    /// The played position, which the server gives before an action and
    /// when it pauses.
    pub fn position(&mut self, played_ms: u64) {
        self.deck.position(played_ms);
    }

    /// The current track was opened.
    pub fn media_opened(&mut self, epoch: u64, duration_ms: Option<u64>, seekable: bool) -> bool {
        let taken = self.deck.media_opened(epoch, duration_ms, seekable);
        self.settle();
        taken
    }

    /// Audio of the current track is out.
    pub fn playing(&mut self, epoch: u64) -> bool {
        let taken = self.deck.playing(epoch);
        self.settle();
        taken
    }

    /// The first audio of the track queued behind the current one is out:
    /// the gapless handover. `uri` is what the player says became audible.
    /// Almost always it is the track the deck holds as next. When the list
    /// changed after the join was written and before it was heard, it is
    /// another: the track is found among those lately handed over, the deck
    /// is told what really joined, and the Playlist then goes where the list
    /// now says (the following track, or on from a track that was deleted).
    /// Returns what the server must do next: queue the new following track,
    /// or leave a deleted one. Ignored in an old epoch.
    pub fn track_boundary(
        &mut self,
        epoch: u64,
        uri: &str,
        duration_ms: Option<u64>,
        seekable: bool,
    ) -> Vec<Effect> {
        if !self.deck.current_epoch(epoch) || self.deck.state() == TransportState::Stopped {
            return vec![];
        }
        let joined = if self.track(self.queued_id).is_some_and(|t| t.uri == uri) {
            Some(self.queued_id)
        } else {
            self.handed
                .iter()
                .rev()
                .find(|(_, u)| u == uri)
                .map(|(id, _)| *id)
        };
        if self.deck.next_queued().0 != uri {
            let metadata = joined
                .and_then(|id| self.track(id))
                .map_or_else(String::new, |t| t.metadata.clone());
            let _ = self.deck.set_next_av_transport_uri(uri, &metadata);
        }
        if !self.deck.track_boundary(epoch, duration_ms, seekable) {
            self.settle();
            return vec![];
        }
        self.queued_id = 0;
        let before = self.current;
        let effects = match joined.filter(|id| self.has(*id)) {
            Some(id) => {
                self.current = id;
                self.deck_id = id;
                self.plan_next()
            }
            None => {
                // What plays is no longer in the list: move on from the
                // track before it, as when the playing track is deleted.
                self.deck_id = 0;
                match self.following(before) {
                    Some(next) => self.cue_and_play(next),
                    None => self.stop_and_cue_first(),
                }
            }
        };
        self.settle();
        effects
    }

    /// The track queued behind the current one cannot be played.
    pub fn next_failed(&mut self, epoch: u64, reason: &str) -> bool {
        let taken = self.deck.next_failed(epoch, reason);
        self.settle();
        taken
    }

    /// The current track played to its end and nothing followed it in the
    /// audio. With a good following track that was not ready in time, it is
    /// loaded and started (the returned effects); with one that failed, the
    /// list stops on it; at the end of the list the first track is cued and
    /// the list stops.
    pub fn ended(&mut self, epoch: u64) -> Vec<Effect> {
        if !self.deck.current_epoch(epoch) || self.deck.state() == TransportState::Stopped {
            return vec![];
        }
        let queued = self.queued_id;
        let mut effects = self.deck.ended(epoch);
        self.queued_id = 0;
        if !effects.is_empty() {
            // A late join: the deck made its next URI the current one.
            if self.has(queued) {
                self.current = queued;
                self.deck_id = queued;
                effects.extend(self.plan_next());
            } else {
                effects = self.deck.stop().unwrap_or_default();
                self.deck_id = 0;
                self.current = self.first();
            }
        } else if self.has(queued) && self.deck.status() == TransportStatus::ErrorOccurred {
            self.current = queued;
        } else {
            self.current = self.first();
        }
        self.settle();
        effects
    }

    /// The current track cannot be fetched or decoded: the list stops on it.
    pub fn failed(&mut self, epoch: u64, reason: &str) -> bool {
        let taken = self.deck.failed(epoch, reason);
        self.settle();
        taken
    }
}
