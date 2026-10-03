//! The Soloist WebSocket API, as its reference page documents it.
//!
//! Source: Spotify Soloist's "WebSocket API" reference page, read 2026-10-03
//! (`docs/soloist.md` carries the digest and the citation). Everything a
//! client sends is a JSON text frame with `"type": "command"`; everything the
//! server sends is a JSON text frame with a `type` naming the event.
//!
//! Two halves:
//!
//! - [`Command`] builds exactly the documented command objects, key for key
//!   (the page's own examples are fixtures, `fixtures/soloist/command-*.json`).
//! - [`parse_event`] reads an event tolerantly. The page says every decoration
//!   is optional and gives no schema, so every field but `type` is optional
//!   here: a missing or mistyped member reads as absent, an unknown member is
//!   ignored, and an event type this model does not know is kept as
//!   [`Event::Other`] with its name. Only a text that is not a JSON object
//!   with a string `type` is an error.
//!
//! The API has no request ids: a `command_result` carries only the command's
//! name, and an `error` only a message. A client must not match on the
//! message text (one string is documented, the rest is not stated).
//!
//! `position.timestamp_ms` is Soloist's wall clock. It is metadata for a
//! progress bar and must never reach the audio path.

use chorus_control::json::{self, Value};

/// A command a client may send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `get_auth_state`: answered with `auth_state`; needs no login.
    GetAuthState,
    /// `get_state`: answered with `playback_state`.
    GetState,
    /// `get_queue`: answered with `queue_changed`. `limit` absent or 0 asks
    /// for every entry.
    GetQueue {
        /// The most entries wanted in each direction.
        limit: Option<u64>,
    },
    /// `play`: resume, or start a playable Spotify URI (track, album,
    /// playlist or episode).
    Play {
        /// What to play; absent resumes.
        uri: Option<String>,
    },
    /// `pause`.
    Pause,
    /// `skip_next`.
    SkipNext,
    /// `skip_prev`: the previous track, or the start of this one.
    SkipPrev,
    /// `seek` to a position in milliseconds.
    Seek {
        /// The position.
        position_ms: u64,
    },
    /// `set_volume`, 0 to 100 (a larger value is sent as 100).
    SetVolume {
        /// The volume in percent.
        volume: u8,
    },
    /// `set_shuffle`.
    SetShuffle {
        /// On or off.
        enabled: bool,
    },
    /// `set_repeat_context`.
    SetRepeatContext {
        /// On or off.
        enabled: bool,
    },
    /// `set_repeat_track`.
    SetRepeatTrack {
        /// On or off.
        enabled: bool,
    },
    /// `add_to_queue`: a Spotify track URI only.
    AddToQueue {
        /// The track.
        uri: String,
    },
    /// `activate`: become the active Spotify Connect device.
    Activate,
    /// `deactivate`: give up active-device status.
    Deactivate,
}

impl Command {
    /// The command's name on the wire.
    pub fn name(&self) -> &'static str {
        match self {
            Command::GetAuthState => "get_auth_state",
            Command::GetState => "get_state",
            Command::GetQueue { .. } => "get_queue",
            Command::Play { .. } => "play",
            Command::Pause => "pause",
            Command::SkipNext => "skip_next",
            Command::SkipPrev => "skip_prev",
            Command::Seek { .. } => "seek",
            Command::SetVolume { .. } => "set_volume",
            Command::SetShuffle { .. } => "set_shuffle",
            Command::SetRepeatContext { .. } => "set_repeat_context",
            Command::SetRepeatTrack { .. } => "set_repeat_track",
            Command::AddToQueue { .. } => "add_to_queue",
            Command::Activate => "activate",
            Command::Deactivate => "deactivate",
        }
    }

    /// Whether this is a query: answered with its event and no
    /// `command_result`.
    pub fn is_query(&self) -> bool {
        matches!(
            self,
            Command::GetAuthState | Command::GetState | Command::GetQueue { .. }
        )
    }

    /// Whether Soloist refuses this without a logged-in Spotify Connect
    /// session (every command but `get_auth_state`).
    pub fn requires_login(&self) -> bool {
        !matches!(self, Command::GetAuthState)
    }

    /// The command object, members in the order the reference page writes
    /// them: `type`, `command`, then the command's own field.
    pub fn to_value(&self) -> Value {
        let mut members = vec![
            ("type".to_string(), Value::text("command")),
            ("command".to_string(), Value::text(self.name())),
        ];
        let mut put = |key: &str, value: Value| members.push((key.to_string(), value));
        match self {
            Command::GetQueue { limit: Some(limit) } => put("limit", Value::Num(limit.to_string())),
            Command::Play { uri: Some(uri) } => put("uri", Value::text(uri)),
            Command::Seek { position_ms } => {
                put("position_ms", Value::Num(position_ms.to_string()))
            }
            Command::SetVolume { volume } => {
                put("volume", Value::int(i64::from((*volume).min(100))))
            }
            Command::SetShuffle { enabled }
            | Command::SetRepeatContext { enabled }
            | Command::SetRepeatTrack { enabled } => put("enabled", Value::Bool(*enabled)),
            Command::AddToQueue { uri } => put("uri", Value::text(uri)),
            _ => {}
        }
        Value::Obj(members)
    }

    /// The command as one line of JSON, ready to be a text frame.
    pub fn to_json(&self) -> String {
        json::write(&self.to_value())
    }

    /// The two commands that set a repeat mode from a raw client, in the
    /// order the reference page gives: the mode to switch off first.
    pub fn repeat(mode: &Repeat) -> [Command; 2] {
        match mode {
            Repeat::Context => [
                Command::SetRepeatTrack { enabled: false },
                Command::SetRepeatContext { enabled: true },
            ],
            Repeat::Track => [
                Command::SetRepeatContext { enabled: false },
                Command::SetRepeatTrack { enabled: true },
            ],
            Repeat::Off | Repeat::Other(_) => [
                Command::SetRepeatTrack { enabled: false },
                Command::SetRepeatContext { enabled: false },
            ],
        }
    }
}

/// Playback status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// `idle`.
    Idle,
    /// `playing`.
    Playing,
    /// `paused`.
    Paused,
    /// `buffering`.
    Buffering,
    /// A status the reference page does not list, kept as written.
    Other(String),
}

impl Status {
    fn from_text(text: &str) -> Status {
        match text {
            "idle" => Status::Idle,
            "playing" => Status::Playing,
            "paused" => Status::Paused,
            "buffering" => Status::Buffering,
            other => Status::Other(other.to_string()),
        }
    }

    /// The status as the wire spells it.
    pub fn as_str(&self) -> &str {
        match self {
            Status::Idle => "idle",
            Status::Playing => "playing",
            Status::Paused => "paused",
            Status::Buffering => "buffering",
            Status::Other(s) => s,
        }
    }

    /// Whether audio is, or is about to be, coming out: `playing`.
    pub fn is_playing(&self) -> bool {
        matches!(self, Status::Playing)
    }
}

/// Repeat mode (`options.repeat`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Repeat {
    /// `off`.
    Off,
    /// `context`: the playlist or album repeats.
    Context,
    /// `track`: the track repeats.
    Track,
    /// A mode the reference page does not list, kept as written.
    Other(String),
}

/// A cover image: only a URL and a size name are documented (no pixels).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cover {
    /// Where the image is.
    pub url: String,
    /// `small`, `default`, `large` or `xlarge`; empty when absent.
    pub size: String,
}

/// An entity: a track, episode, artist, album, playlist, show or ad, with the
/// decorations that were present.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Entity {
    /// The Spotify URI; empty when no entity is available or it is unknown.
    pub uri: String,
    /// `track`, `episode`, `artist`, `album`, `playlist`, `show`, `ad`,
    /// `unknown`, or empty.
    pub entity_type: String,
    /// `decorations.identity.name`.
    pub name: Option<String>,
    /// `decorations.visual_identity.cover[]`, in the order sent.
    pub covers: Vec<Cover>,
    /// `decorations.parent.entity`: an album for a track, a show for an
    /// episode.
    pub parent: Option<Box<Entity>>,
    /// `decorations.creators[].entity`, in the order sent.
    pub creators: Vec<Entity>,
    /// `decorations.playback.duration_ms`.
    pub duration_ms: Option<u64>,
    /// `decorations.playback.content_ratings[]`.
    pub content_ratings: Vec<String>,
}

impl Entity {
    /// Whether this is the "no entity" envelope: an empty URI and no name.
    pub fn is_empty(&self) -> bool {
        self.uri.is_empty() && self.name.is_none()
    }

    /// The names of the creators that have one, joined by `, `: the artist
    /// line of a now-playing record. `None` when no creator is named.
    pub fn artist(&self) -> Option<String> {
        let names: Vec<&str> = self
            .creators
            .iter()
            .filter_map(|c| c.name.as_deref())
            .filter(|n| !n.is_empty())
            .collect();
        if names.is_empty() {
            None
        } else {
            Some(names.join(", "))
        }
    }

    /// The parent's name: the album of a track, the show of an episode.
    pub fn album(&self) -> Option<&str> {
        self.parent.as_ref().and_then(|p| p.name.as_deref())
    }

    /// The cover to show: the `large` one, else the first.
    pub fn cover_url(&self) -> Option<&str> {
        self.covers
            .iter()
            .find(|c| c.size == "large")
            .or_else(|| self.covers.first())
            .map(|c| c.url.as_str())
    }
}

/// A playback position anchor.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Position {
    /// The position at `timestamp_ms`, in milliseconds.
    pub position_ms: Option<u64>,
    /// Soloist's wall clock when the position was taken, Unix milliseconds.
    /// Wall clock: never use it in the audio path.
    pub timestamp_ms: Option<u64>,
    /// Position speed; `0.0` means the position is not advancing.
    pub speed: Option<f64>,
}

/// Playback options.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Options {
    /// Whether shuffle is on.
    pub shuffle: Option<bool>,
    /// The repeat mode.
    pub repeat: Option<Repeat>,
    /// The requested playback speed.
    pub playback_speed: Option<f64>,
    /// `modes`: the string-valued members, in the order sent.
    pub modes: Vec<(String, String)>,
}

/// One entry of `available_actions`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    /// The player action's name (`pause`, `seek_forward`, ...). These are
    /// action names, not command names.
    pub name: String,
    /// `step_ms` when the action advertises one.
    pub step_ms: Option<u64>,
}

/// The full playback snapshot.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlaybackState {
    /// `status`.
    pub status: Option<Status>,
    /// The current playable entity.
    pub item: Option<Entity>,
    /// The current context (playlist, album, ...).
    pub context: Option<Entity>,
    /// The position anchor.
    pub position: Option<Position>,
    /// Volume in percent, 0 to 100.
    pub volume: Option<u8>,
    /// Whether Soloist is the active Spotify Connect device.
    pub is_active: Option<bool>,
    /// Shuffle, repeat, speed and modes.
    pub options: Option<Options>,
    /// The actions available now, in the order sent.
    pub available_actions: Vec<Action>,
}

/// One entry of the play queue.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct QueueEntry {
    /// The stable entry id, or the item's URI.
    pub uid: Option<String>,
    /// `context`, `queue` or `autoplay`.
    pub source: Option<String>,
    /// The entry's entity.
    pub item: Option<Entity>,
}

/// An event Soloist sent.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// `auth_state`: on connect, on a login change, and for `get_auth_state`.
    AuthState {
        /// Whether a Spotify Connect session is logged in. Absent reads as
        /// logged out.
        logged_in: bool,
        /// Whether Soloist is the active device.
        is_active: Option<bool>,
        /// The Spotify Connect device name.
        device_name: Option<String>,
    },
    /// `playback_state`: the full snapshot.
    PlaybackState(Box<PlaybackState>),
    /// `track_changed`.
    TrackChanged {
        /// The new item.
        item: Option<Entity>,
    },
    /// `playback_changed`.
    PlaybackChanged {
        /// The new status.
        status: Option<Status>,
    },
    /// `volume_changed`.
    VolumeChanged {
        /// The new volume in percent.
        volume: Option<u8>,
    },
    /// `device_changed`.
    DeviceChanged {
        /// Whether Soloist is the active device.
        is_active: Option<bool>,
        /// The device name.
        device_name: Option<String>,
    },
    /// `context_changed`.
    ContextChanged {
        /// The new context.
        context: Option<Entity>,
    },
    /// `options_changed`.
    OptionsChanged {
        /// The new options.
        options: Option<Options>,
    },
    /// `position_sync`: the position anchor moved.
    PositionSync {
        /// The new anchor.
        position: Option<Position>,
    },
    /// `queue_changed`: broadcasts carry at most 10 entries each way.
    QueueChanged {
        /// Most recently played first.
        previous: Vec<QueueEntry>,
        /// Next to play first.
        upcoming: Vec<QueueEntry>,
    },
    /// `command_result`: the command was accepted and dispatched (not: it
    /// has taken effect).
    CommandResult {
        /// The command's name.
        command: String,
    },
    /// `error`: sent only to the client whose message was refused; the
    /// connection stays open.
    Error {
        /// The message. Do not match on it.
        message: String,
    },
    /// An event type this model does not know.
    Other {
        /// Its `type`.
        kind: String,
    },
}

impl Event {
    /// The event's `type` as the wire spells it.
    pub fn kind(&self) -> &str {
        match self {
            Event::AuthState { .. } => "auth_state",
            Event::PlaybackState(_) => "playback_state",
            Event::TrackChanged { .. } => "track_changed",
            Event::PlaybackChanged { .. } => "playback_changed",
            Event::VolumeChanged { .. } => "volume_changed",
            Event::DeviceChanged { .. } => "device_changed",
            Event::ContextChanged { .. } => "context_changed",
            Event::OptionsChanged { .. } => "options_changed",
            Event::PositionSync { .. } => "position_sync",
            Event::QueueChanged { .. } => "queue_changed",
            Event::CommandResult { .. } => "command_result",
            Event::Error { .. } => "error",
            Event::Other { kind } => kind,
        }
    }

    /// The playback status this event reports, if it reports one
    /// (`playback_state` and `playback_changed` do).
    pub fn status(&self) -> Option<&Status> {
        match self {
            Event::PlaybackState(state) => state.status.as_ref(),
            Event::PlaybackChanged { status } => status.as_ref(),
            _ => None,
        }
    }
}

/// Why a text is not an event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    /// Not JSON the workspace's reader accepts.
    Json(String),
    /// JSON, but not an object.
    NotAnObject,
    /// An object with no string `type`.
    NoType,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::Json(e) => write!(f, "not JSON: {e}"),
            ApiError::NotAnObject => write!(f, "not a JSON object"),
            ApiError::NoType => write!(f, "no string \"type\" member"),
        }
    }
}

impl std::error::Error for ApiError {}

/// Read one event from the text of a frame.
pub fn parse_event(text: &str) -> Result<Event, ApiError> {
    let value = json::parse(text).map_err(|e| ApiError::Json(e.to_string()))?;
    event_from_value(&value)
}

/// Read one event from a parsed JSON value (the `event` member of the
/// supervisor protocol's `event` message).
pub fn event_from_value(value: &Value) -> Result<Event, ApiError> {
    if !matches!(value, Value::Obj(_)) {
        return Err(ApiError::NotAnObject);
    }
    let kind = value
        .get("type")
        .and_then(Value::as_str)
        .ok_or(ApiError::NoType)?;
    Ok(match kind {
        "auth_state" => Event::AuthState {
            logged_in: boolean(value, "logged_in").unwrap_or(false),
            is_active: boolean(value, "is_active"),
            device_name: string(value, "device_name"),
        },
        "playback_state" => Event::PlaybackState(Box::new(PlaybackState {
            status: string(value, "status").map(|s| Status::from_text(&s)),
            item: value.get("item").and_then(entity),
            context: value.get("context").and_then(entity),
            position: value.get("position").and_then(position),
            volume: volume(value),
            is_active: boolean(value, "is_active"),
            options: value.get("options").and_then(options),
            available_actions: actions(value.get("available_actions")),
        })),
        "track_changed" => Event::TrackChanged {
            item: value.get("item").and_then(entity),
        },
        "playback_changed" => Event::PlaybackChanged {
            status: string(value, "status").map(|s| Status::from_text(&s)),
        },
        "volume_changed" => Event::VolumeChanged {
            volume: volume(value),
        },
        "device_changed" => Event::DeviceChanged {
            is_active: boolean(value, "is_active"),
            device_name: string(value, "device_name"),
        },
        "context_changed" => Event::ContextChanged {
            context: value.get("context").and_then(entity),
        },
        "options_changed" => Event::OptionsChanged {
            options: value.get("options").and_then(options),
        },
        "position_sync" => Event::PositionSync {
            position: value.get("position").and_then(position),
        },
        "queue_changed" => Event::QueueChanged {
            previous: queue(value.get("previous")),
            upcoming: queue(value.get("upcoming")),
        },
        "command_result" => Event::CommandResult {
            command: string(value, "command").unwrap_or_default(),
        },
        "error" => Event::Error {
            message: string(value, "message").unwrap_or_default(),
        },
        other => Event::Other {
            kind: other.to_string(),
        },
    })
}

fn string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn boolean(value: &Value, key: &str) -> Option<bool> {
    value.get(key).and_then(Value::as_bool)
}

/// A JSON number as a float, when it is finite.
fn float(value: &Value, key: &str) -> Option<f64> {
    let n: f64 = value.get(key)?.as_num()?.parse().ok()?;
    n.is_finite().then_some(n)
}

/// A JSON number as whole milliseconds. Whether Soloist writes these as
/// integers is not stated, so a fraction is rounded and a negative value
/// reads as absent.
fn millis(value: &Value, key: &str) -> Option<u64> {
    let n = float(value, key)?;
    if n < 0.0 || n > 9.0e15 {
        return None;
    }
    // In range by the check above: at most 9.0e15, which a u64 holds exactly.
    Some(n.round() as u64)
}

/// `volume`: a percentage, clamped into 0 to 100.
fn volume(value: &Value) -> Option<u8> {
    let n = float(value, "volume")?;
    // Clamped first, so the conversion cannot be out of range.
    Some(n.clamp(0.0, 100.0).round() as u8)
}

fn entity(value: &Value) -> Option<Entity> {
    entity_at(value, 0)
}

/// Parents nest; the JSON reader already bounds depth, and this stops
/// following parents long before that.
const MAX_ENTITY_DEPTH: usize = 4;

fn entity_at(value: &Value, depth: usize) -> Option<Entity> {
    if !matches!(value, Value::Obj(_)) {
        return None;
    }
    let mut out = Entity {
        uri: string(value, "uri").unwrap_or_default(),
        entity_type: string(value, "entity_type").unwrap_or_default(),
        ..Entity::default()
    };
    let Some(decorations) = value.get("decorations") else {
        return Some(out);
    };
    out.name = decorations.get("identity").and_then(|i| string(i, "name"));
    if let Some(Value::Arr(covers)) = decorations
        .get("visual_identity")
        .and_then(|v| v.get("cover"))
    {
        out.covers = covers
            .iter()
            .filter_map(|c| {
                Some(Cover {
                    url: string(c, "url")?,
                    size: string(c, "size").unwrap_or_default(),
                })
            })
            .collect();
    }
    if depth < MAX_ENTITY_DEPTH {
        out.parent = decorations
            .get("parent")
            .and_then(|p| p.get("entity"))
            .and_then(|e| entity_at(e, depth + 1))
            .map(Box::new);
        if let Some(Value::Arr(creators)) = decorations.get("creators") {
            out.creators = creators
                .iter()
                .filter_map(|c| c.get("entity"))
                .filter_map(|e| entity_at(e, depth + 1))
                .collect();
        }
    }
    if let Some(playback) = decorations.get("playback") {
        out.duration_ms = millis(playback, "duration_ms");
        if let Some(Value::Arr(ratings)) = playback.get("content_ratings") {
            out.content_ratings = ratings
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
        }
    }
    Some(out)
}

fn position(value: &Value) -> Option<Position> {
    if !matches!(value, Value::Obj(_)) {
        return None;
    }
    Some(Position {
        position_ms: millis(value, "position_ms"),
        timestamp_ms: millis(value, "timestamp_ms"),
        speed: float(value, "speed"),
    })
}

fn options(value: &Value) -> Option<Options> {
    if !matches!(value, Value::Obj(_)) {
        return None;
    }
    let modes = match value.get("modes") {
        Some(Value::Obj(members)) => members
            .iter()
            .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
            .collect(),
        _ => Vec::new(),
    };
    Some(Options {
        shuffle: boolean(value, "shuffle"),
        repeat: string(value, "repeat").map(|r| match r.as_str() {
            "off" => Repeat::Off,
            "context" => Repeat::Context,
            "track" => Repeat::Track,
            _ => Repeat::Other(r),
        }),
        playback_speed: float(value, "playback_speed"),
        modes,
    })
}

fn actions(value: Option<&Value>) -> Vec<Action> {
    match value {
        Some(Value::Obj(members)) => members
            .iter()
            .map(|(name, v)| Action {
                name: name.clone(),
                step_ms: millis(v, "step_ms"),
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn queue(value: Option<&Value>) -> Vec<QueueEntry> {
    match value {
        Some(Value::Arr(entries)) => entries
            .iter()
            .filter(|e| matches!(e, Value::Obj(_)))
            .map(|e| QueueEntry {
                uid: string(e, "uid"),
                source: string(e, "source"),
                item: e.get("item").and_then(entity),
            })
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_has_the_documented_envelope() {
        let all = [
            Command::GetAuthState,
            Command::GetState,
            Command::GetQueue { limit: None },
            Command::Play { uri: None },
            Command::Pause,
            Command::SkipNext,
            Command::SkipPrev,
            Command::Seek { position_ms: 1 },
            Command::SetVolume { volume: 1 },
            Command::SetShuffle { enabled: true },
            Command::SetRepeatContext { enabled: true },
            Command::SetRepeatTrack { enabled: true },
            Command::AddToQueue {
                uri: "spotify:track:x".into(),
            },
            Command::Activate,
            Command::Deactivate,
        ];
        for command in &all {
            let value = command.to_value();
            assert_eq!(value.get("type").and_then(Value::as_str), Some("command"));
            assert_eq!(
                value.get("command").and_then(Value::as_str),
                Some(command.name())
            );
            assert_eq!(json::parse(&command.to_json()).unwrap(), value);
            assert!(!command.to_json().contains('\n'));
        }
        assert!(!Command::GetAuthState.requires_login());
        assert!(all[1..].iter().all(Command::requires_login));
        assert_eq!(all.iter().filter(|c| c.is_query()).count(), 3);
    }

    #[test]
    fn a_volume_above_100_is_sent_as_100() {
        assert_eq!(
            Command::SetVolume { volume: 250 }.to_json(),
            r#"{"type":"command","command":"set_volume","volume":100}"#
        );
    }

    #[test]
    fn repeat_modes_are_the_documented_pairs() {
        assert_eq!(
            Command::repeat(&Repeat::Off),
            [
                Command::SetRepeatTrack { enabled: false },
                Command::SetRepeatContext { enabled: false }
            ]
        );
        assert_eq!(
            Command::repeat(&Repeat::Context),
            [
                Command::SetRepeatTrack { enabled: false },
                Command::SetRepeatContext { enabled: true }
            ]
        );
        assert_eq!(
            Command::repeat(&Repeat::Track),
            [
                Command::SetRepeatContext { enabled: false },
                Command::SetRepeatTrack { enabled: true }
            ]
        );
    }

    #[test]
    fn only_a_missing_type_or_a_non_object_is_an_error() {
        assert_eq!(parse_event("[]"), Err(ApiError::NotAnObject));
        assert_eq!(parse_event("{}"), Err(ApiError::NoType));
        assert_eq!(parse_event(r#"{"type":7}"#), Err(ApiError::NoType));
        assert!(matches!(parse_event("{"), Err(ApiError::Json(_))));
        assert_eq!(
            parse_event(r#"{"type":"build_expiring","days":3}"#),
            Ok(Event::Other {
                kind: "build_expiring".into()
            })
        );
    }

    #[test]
    fn every_member_but_type_is_optional() {
        assert_eq!(
            parse_event(r#"{"type":"auth_state"}"#),
            Ok(Event::AuthState {
                logged_in: false,
                is_active: None,
                device_name: None
            })
        );
        assert_eq!(
            parse_event(r#"{"type":"playback_state"}"#),
            Ok(Event::PlaybackState(Box::default()))
        );
        for kind in [
            "track_changed",
            "playback_changed",
            "volume_changed",
            "device_changed",
            "context_changed",
            "options_changed",
            "position_sync",
            "queue_changed",
            "command_result",
            "error",
        ] {
            let event = parse_event(&format!(r#"{{"type":"{kind}"}}"#)).unwrap();
            assert_eq!(event.kind(), kind);
        }
    }

    #[test]
    fn mistyped_members_read_as_absent() {
        let event = parse_event(
            r#"{"type":"playback_state","status":7,"item":"x","context":[],"position":3,
                "volume":"loud","is_active":"yes","options":null,"available_actions":[]}"#,
        )
        .unwrap();
        assert_eq!(event, Event::PlaybackState(Box::default()));
        let event = parse_event(
            r#"{"type":"track_changed","item":{"uri":5,"decorations":{"identity":[],
                "visual_identity":{"cover":[{"size":"large"},7,{"url":"u"}]},
                "parent":{"entity":3},"creators":[{"entity":{}},{"nope":1},4],
                "playback":{"duration_ms":"long","content_ratings":[1,"explicit"]}}}}"#,
        )
        .unwrap();
        let Event::TrackChanged { item: Some(item) } = event else {
            panic!("not a track_changed with an item");
        };
        assert_eq!(item.uri, "");
        assert_eq!(item.name, None);
        assert_eq!(
            item.covers,
            vec![Cover {
                url: "u".into(),
                size: String::new()
            }]
        );
        assert_eq!(item.parent, None);
        assert_eq!(item.creators, vec![Entity::default()]);
        assert_eq!(item.artist(), None);
        assert_eq!(item.duration_ms, None);
        assert_eq!(item.content_ratings, vec!["explicit".to_string()]);
    }

    #[test]
    fn numbers_are_read_leniently() {
        let event = parse_event(
            r#"{"type":"position_sync","position":{"position_ms":45000.4,"timestamp_ms":1.747654321e12,"speed":0}}"#,
        )
        .unwrap();
        assert_eq!(
            event,
            Event::PositionSync {
                position: Some(Position {
                    position_ms: Some(45000),
                    timestamp_ms: Some(1_747_654_321_000),
                    speed: Some(0.0)
                })
            }
        );
        let volume = |text: &str| match parse_event(text).unwrap() {
            Event::VolumeChanged { volume } => volume,
            other => panic!("{other:?}"),
        };
        assert_eq!(
            volume(r#"{"type":"volume_changed","volume":42.6}"#),
            Some(43)
        );
        assert_eq!(
            volume(r#"{"type":"volume_changed","volume":250}"#),
            Some(100)
        );
        assert_eq!(volume(r#"{"type":"volume_changed","volume":-3}"#), Some(0));
        assert_eq!(
            parse_event(r#"{"type":"position_sync","position":{"position_ms":-1}}"#).unwrap(),
            Event::PositionSync {
                position: Some(Position::default())
            }
        );
    }

    #[test]
    fn the_cover_is_the_large_one_else_the_first() {
        let cover = |size: &str| Cover {
            url: format!("https://example.invalid/{size}"),
            size: size.to_string(),
        };
        let mut entity = Entity {
            covers: vec![cover("small"), cover("large"), cover("xlarge")],
            ..Entity::default()
        };
        assert_eq!(entity.cover_url(), Some("https://example.invalid/large"));
        entity.covers.remove(1);
        assert_eq!(entity.cover_url(), Some("https://example.invalid/small"));
        entity.covers.clear();
        assert_eq!(entity.cover_url(), None);
    }

    #[test]
    fn deep_parents_stop_being_followed() {
        let mut text = String::from(r#"{"uri":"leaf"}"#);
        for i in 0..10 {
            text =
                format!(r#"{{"uri":"level{i}","decorations":{{"parent":{{"entity":{text}}}}}}}"#);
        }
        let event = parse_event(&format!(r#"{{"type":"track_changed","item":{text}}}"#)).unwrap();
        let Event::TrackChanged { item: Some(item) } = event else {
            panic!("no item");
        };
        let mut depth = 0;
        let mut at = &item;
        while let Some(parent) = &at.parent {
            depth += 1;
            at = parent;
        }
        assert_eq!(depth, MAX_ENTITY_DEPTH);
    }
}
