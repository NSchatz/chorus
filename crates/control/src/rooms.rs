//! The values catalog version 2 adds: link facts, channel roles and the
//! layouts a bonded set may take, civil times and weekly windows, sources,
//! and the persisted definitions (saved groups, alarms, autoplay rules).
//!
//! Every type here has ONE spelling, used by the wire, the state message and
//! the state file alike, and a parser that accepts exactly that spelling and
//! nothing near it. The reason is the one [`crate::catalog::Volume`] gives for
//! itself: a value that goes in and comes back different is the thing a golden
//! vector exists to catch, and a lenient parser is where such values come from.
//!
//! # No clock is read here
//!
//! A [`CivilTime`] is an INPUT. Whether a quiet-hours window is active is a
//! question this module answers for a time it is handed
//! ([`QuietWindow::contains`]); which time it is now, in which zone, across
//! which daylight-saving change, is `crates/schedule`'s business and the
//! caller's. That keeps the room model testable with nothing running and keeps
//! wall-clock time out of anything near the audio path (BRIEF section 3.1).

use std::fmt;

use crate::catalog::{is_identifier, Volume};

/// What is known about how an endpoint reaches the server.
///
/// Distinct from a ROOM's declared transport ([`crate::transport::Transport`]),
/// which is configuration: this is a fact an endpoint reports about itself on
/// `attach`, and `unknown` is what an endpoint that has never said is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Link {
    /// Copper.
    Wired,
    /// A radio.
    Wireless,
    /// Never reported.
    Unknown,
}

impl Link {
    /// Every link value, in the order a refusal names them.
    pub const ALL: [Link; 3] = [Link::Wired, Link::Wireless, Link::Unknown];

    /// The catalog's word for it.
    pub fn name(self) -> &'static str {
        match self {
            Link::Wired => "wired",
            Link::Wireless => "wireless",
            Link::Unknown => "unknown",
        }
    }

    /// Read the catalog's word back.
    pub fn parse(text: &str) -> Option<Link> {
        Link::ALL.iter().copied().find(|l| l.name() == text)
    }
}

impl fmt::Display for Link {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A channel position an endpoint in a bonded set plays.
///
/// The names and numbers are `docs/protocol.md`'s channel-map table ("The
/// channel map"), so a bond's roles and a stream's channel map speak the same
/// positions and a later remap from one to the other needs no table of its
/// own. Only the positions a valid layout can use are here (see
/// [`validate_layout`]); a name from the table that no layout uses, such as
/// `TFL`, is refused naming the layouts rather than accepted and then refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    /// Front left, position 1.
    Fl,
    /// Front right, position 2.
    Fr,
    /// Front centre, position 3.
    Fc,
    /// Low-frequency effects, position 4.
    Lfe,
    /// Back left, position 5.
    Bl,
    /// Back right, position 6.
    Br,
    /// Side left, position 10.
    Sl,
    /// Side right, position 11.
    Sr,
}

impl Role {
    /// Every role, in channel-position order.
    pub const ALL: [Role; 8] = [
        Role::Fl,
        Role::Fr,
        Role::Fc,
        Role::Lfe,
        Role::Bl,
        Role::Br,
        Role::Sl,
        Role::Sr,
    ];

    /// The channel-map name, as `docs/protocol.md` spells it.
    pub fn name(self) -> &'static str {
        match self {
            Role::Fl => "FL",
            Role::Fr => "FR",
            Role::Fc => "FC",
            Role::Lfe => "LFE",
            Role::Bl => "BL",
            Role::Br => "BR",
            Role::Sl => "SL",
            Role::Sr => "SR",
        }
    }

    /// The channel-map position number, as `docs/protocol.md` numbers it.
    pub fn position(self) -> u8 {
        match self {
            Role::Fl => 1,
            Role::Fr => 2,
            Role::Fc => 3,
            Role::Lfe => 4,
            Role::Bl => 5,
            Role::Br => 6,
            Role::Sl => 10,
            Role::Sr => 11,
        }
    }

    /// Read a channel-map name back.
    pub fn parse(text: &str) -> Option<Role> {
        Role::ALL.iter().copied().find(|r| r.name() == text)
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// One endpoint's place in a bonded set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BondMember {
    /// The endpoint.
    pub endpoint: String,
    /// The channel it plays.
    pub role: Role,
}

/// The layouts a bonded set may take, as a refusal names them.
pub const LAYOUTS: &str = "FL FR (stereo), FL FR LFE, FL FR FC, FL FR FC LFE, and FL FR FC with \
                           optional LFE and either SL SR or BL BR (theater)";

/// Whether a set of roles is a layout a bonded set may take.
///
/// Stereo is FL and FR; a sub (LFE) may be added to anything; a centre (FC)
/// makes it a front three; a surround pair, SL SR or BL BR and never one of
/// each, needs the centre. Every role appears at most once, which the caller
/// has already checked. `None` is a valid layout; `Some` says what is wrong.
pub fn validate_layout(roles: &[Role]) -> Option<String> {
    let has = |r: Role| roles.contains(&r);
    if !(has(Role::Fl) && has(Role::Fr)) {
        return Some(format!(
            "every bonded set has a front left and a front right; the layouts are {}",
            LAYOUTS
        ));
    }
    let side = has(Role::Sl) || has(Role::Sr);
    let back = has(Role::Bl) || has(Role::Br);
    if side && back {
        return Some(format!(
            "a set has one surround pair, SL SR or BL BR, and not both; the layouts are {}",
            LAYOUTS
        ));
    }
    if (side && !(has(Role::Sl) && has(Role::Sr))) || (back && !(has(Role::Bl) && has(Role::Br))) {
        return Some(format!(
            "a surround channel comes as a pair; the layouts are {}",
            LAYOUTS
        ));
    }
    if (side || back) && !has(Role::Fc) {
        return Some(format!(
            "a set with surrounds is a theater layout and has a front centre; the layouts are {}",
            LAYOUTS
        ));
    }
    None
}

/// A time of day, to the minute: `HH:MM`, 00:00 to 23:59.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ClockTime(u16);

impl ClockTime {
    /// Minutes in a day.
    pub const MINUTES_PER_DAY: u16 = 24 * 60;

    /// From minutes after midnight, refusing a value past 23:59.
    pub fn from_minutes(minutes: u16) -> Option<ClockTime> {
        (minutes < ClockTime::MINUTES_PER_DAY).then_some(ClockTime(minutes))
    }

    /// Minutes after midnight.
    pub fn minutes(self) -> u16 {
        self.0
    }

    /// The one spelling: two digits, a colon, two digits.
    pub fn literal(self) -> String {
        format!("{:02}:{:02}", self.0 / 60, self.0 % 60)
    }

    /// Read the one spelling back. `7:00`, `07:0` and `24:00` are refused.
    pub fn parse(text: &str) -> Option<ClockTime> {
        let bytes = text.as_bytes();
        if bytes.len() != 5 || bytes[2] != b':' {
            return None;
        }
        let digits = [bytes[0], bytes[1], bytes[3], bytes[4]];
        if !digits.iter().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let hour = u16::from(digits[0] - b'0') * 10 + u16::from(digits[1] - b'0');
        let minute = u16::from(digits[2] - b'0') * 10 + u16::from(digits[3] - b'0');
        if hour > 23 || minute > 59 {
            return None;
        }
        Some(ClockTime(hour * 60 + minute))
    }
}

impl fmt::Display for ClockTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.literal())
    }
}

/// The days of the week, Monday first, as the catalog names them.
pub const DAY_NAMES: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

/// A set of weekdays. Bit 0 is Monday, bit 6 Sunday.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Days(u8);

impl Days {
    /// No day at all, which an alarm uses to mean "once".
    pub const NONE: Days = Days(0);

    /// From a mask, refusing a bit past Sunday.
    pub fn from_mask(mask: u8) -> Option<Days> {
        (mask < 0x80).then_some(Days(mask))
    }

    /// The mask.
    pub fn mask(self) -> u8 {
        self.0
    }

    /// Whether there is no day in the set.
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Whether `weekday` (0 is Monday) is in the set.
    pub fn has(self, weekday: u8) -> bool {
        weekday < 7 && self.0 & (1 << weekday) != 0
    }

    /// The day names, in week order: the one order the catalog accepts.
    pub fn names(self) -> Vec<&'static str> {
        (0..7u8)
            .filter(|d| self.has(*d))
            .map(|d| DAY_NAMES[usize::from(d)])
            .collect()
    }

    /// Read a list of day names back.
    ///
    /// Strictly in week order and without a repeat, so a set has one spelling:
    /// `["tue","mon"]` and `["mon","mon"]` are refused rather than tidied.
    pub fn from_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Result<Days, String> {
        let mut mask = 0u8;
        let mut last: Option<usize> = None;
        for name in names {
            let index = DAY_NAMES.iter().position(|d| *d == name).ok_or_else(|| {
                format!(
                    "'{}' is not a day; the days are {}",
                    name,
                    DAY_NAMES.join(", ")
                )
            })?;
            if let Some(previous) = last {
                if index <= previous {
                    return Err(format!(
                        "the days are listed once each and in week order ({}); '{}' is out of \
                         place",
                        DAY_NAMES.join(", "),
                        name
                    ));
                }
            }
            last = Some(index);
            mask |= 1 << index;
        }
        Ok(Days(mask))
    }
}

/// A civil (wall-clock) time in the house's own time zone: a weekday and a
/// time of day. What `crates/schedule` turns an instant into, and the only
/// form of "now" the room model ever sees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CivilTime {
    /// 0 is Monday, 6 is Sunday.
    pub weekday: u8,
    /// The time of day.
    pub time: ClockTime,
}

/// A weekly window during which a room's volume is capped.
///
/// `start` is inside the window and `end` is not. A window whose end is not
/// after its start runs past midnight into the next day, and `days` are the
/// days it STARTS on: a Friday 22:00 to 07:00 window covers Saturday 03:00.
/// A window whose start and end are equal is refused, because "empty" and "all
/// day" would both be readings of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuietWindow {
    /// The days the window starts on. Never empty.
    pub days: Days,
    /// When it starts.
    pub start: ClockTime,
    /// When it ends.
    pub end: ClockTime,
    /// The cap while it is active.
    pub limit: Volume,
}

impl QuietWindow {
    /// Whether `now` is inside this window.
    pub fn contains(&self, now: CivilTime) -> bool {
        let m = now.time.minutes();
        let (s, e) = (self.start.minutes(), self.end.minutes());
        if s < e {
            self.days.has(now.weekday) && s <= m && m < e
        } else {
            let yesterday = (now.weekday + 6) % 7;
            (self.days.has(now.weekday) && m >= s) || (self.days.has(yesterday) && m < e)
        }
    }

    /// Why a window is not one, or `None`.
    pub fn problem(&self) -> Option<String> {
        if self.days.is_empty() {
            return Some("a quiet-hours window names at least one day".to_string());
        }
        if self.start == self.end {
            return Some(format!(
                "a quiet-hours window starting and ending at {} is ambiguous (empty or all day); \
                 give it a different end",
                self.start
            ));
        }
        None
    }

    /// The persisted spelling: `mon,tue 22:00-07:00 0.300`.
    pub fn persisted(&self) -> String {
        format!(
            "{} {}-{} {}",
            self.days.names().join(","),
            self.start,
            self.end,
            self.limit.literal()
        )
    }

    /// Read the persisted spelling back.
    pub fn from_persisted(text: &str) -> Result<QuietWindow, String> {
        let parts: Vec<&str> = text.split_whitespace().collect();
        if parts.len() != 3 {
            return Err(format!(
                "'{}' is not 'days start-end limit' (e.g. 'mon,tue 22:00-07:00 0.300')",
                text
            ));
        }
        let days = Days::from_names(parts[0].split(','))?;
        let (start, end) = parts[1]
            .split_once('-')
            .ok_or_else(|| format!("'{}' is not 'start-end'", parts[1]))?;
        let start = ClockTime::parse(start).ok_or_else(|| format!("'{}' is not HH:MM", start))?;
        let end = ClockTime::parse(end).ok_or_else(|| format!("'{}' is not HH:MM", end))?;
        let limit =
            Volume::parse(parts[2]).ok_or_else(|| format!("'{}' is not a volume", parts[2]))?;
        let window = QuietWindow {
            days,
            start,
            end,
            limit,
        };
        match window.problem() {
            Some(p) => Err(p),
            None => Ok(window),
        }
    }
}

/// What a group is playing.
///
/// The ids are the envelope's: `stream` is the server's configured stream,
/// `none` is nothing, `chime:<name>` one of the generated chimes,
/// `line-in:<endpoint>/<input>` an endpoint's offered line-in, (goal 16)
/// `player:<id>` one of the server's network media players, and (goal 17)
/// `stored:<id>` a stored source ([`StoredSource`]), which only an ALARM
/// names: a formed group never plays `stored:<id>` itself (the runtime turns
/// it into the player or the receiver that plays it), and a `take` naming one
/// is refused by name. This crate
/// validates the spelling; which chimes exist, which inputs are offered and
/// which players the server runs is the runtime's to know, so a well-spelled
/// source naming one that does not exist is the runtime's refusal, not the
/// catalog's.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Source {
    /// The configured stream: what every group plays until told otherwise.
    Stream,
    /// Nothing.
    None,
    /// A generated chime, by name.
    Chime(String),
    /// An endpoint's line-in.
    LineIn(InputId),
    /// (goal 16) One of the server's network media players, by its id (the
    /// server's are `p0`, `p1`, ...). What the player plays is not the
    /// catalog's: the runtime that drives it says so in the group's
    /// [`NowPlaying`]. A player plays in at most one group at a time
    /// (`crate::zones`).
    Player(String),
    /// (goal 17) A stored source, by its id: an alarm's source only. What it
    /// names (a stream URL, a Spotify URI) is in the room model's stored
    /// sources ([`StoredSource`]); a group never plays this spelling.
    Stored(String),
    /// (goal 17) One of the server's Spotify Soloist receivers, by its id
    /// (the server's are `r0`, `r1`, ...): what the Spotify app plays on the
    /// Connect device of a room or group. Like a player it plays in at most
    /// one group at a time, and what it plays is said in the group's
    /// [`NowPlaying`]. Only the server's own receiver manager gives a group
    /// this source: a receiver's audio follows Spotify, not a command, so a
    /// `take` naming it is refused by name.
    Soloist(String),
}

impl Source {
    /// The one spelling.
    pub fn literal(&self) -> String {
        match self {
            Source::Stream => "stream".to_string(),
            Source::None => "none".to_string(),
            Source::Chime(name) => format!("chime:{}", name),
            Source::LineIn(input) => format!("line-in:{}", input.literal()),
            Source::Player(id) => format!("player:{}", id),
            Source::Stored(id) => format!("stored:{}", id),
            Source::Soloist(id) => format!("soloist:{}", id),
        }
    }

    /// Read the one spelling back.
    pub fn parse(text: &str) -> Option<Source> {
        match text {
            "stream" => return Some(Source::Stream),
            "none" => return Some(Source::None),
            _ => {}
        }
        if let Some(name) = text.strip_prefix("chime:") {
            return is_identifier(name).then(|| Source::Chime(name.to_string()));
        }
        if let Some(input) = text.strip_prefix("line-in:") {
            return InputId::parse(input).map(Source::LineIn);
        }
        if let Some(id) = text.strip_prefix("player:") {
            return is_identifier(id).then(|| Source::Player(id.to_string()));
        }
        if let Some(id) = text.strip_prefix("stored:") {
            return is_identifier(id).then(|| Source::Stored(id.to_string()));
        }
        if let Some(id) = text.strip_prefix("soloist:") {
            return is_identifier(id).then(|| Source::Soloist(id.to_string()));
        }
        None
    }

    /// (goal 17) Whether a group that plays this source carries a
    /// now-playing record a runtime may set ([`NowPlaying`]): a player
    /// source, or a Soloist receiver. (A line-in labelled as a streamer
    /// carries one too, which the room model writes itself.)
    pub fn takes_now_playing(&self) -> bool {
        matches!(self, Source::Player(_) | Source::Soloist(_))
    }

    /// (goal 17) Whether this is a Spotify Soloist receiver. The rule it is
    /// here for: a room playing one is PAUSED for an announcement and
    /// resumed after it, never ducked, and a switch to or from one cuts and
    /// never overlaps (proposal P7: the Developer Policy's overlap clause).
    pub fn is_soloist(&self) -> bool {
        matches!(self, Source::Soloist(_))
    }

    /// (goal 17) Whether this source plays in at most one group at a time:
    /// a player or a Soloist receiver, each one stream of audio with one
    /// position. A `take` moves such a source and never copies it.
    pub fn is_exclusive(&self) -> bool {
        matches!(self, Source::Player(_) | Source::Soloist(_))
    }
}

/// What a refusal says a source a group may play is.
pub const SOURCE_SPELLINGS: &str = "'stream', 'none', 'chime:<name>', \
     'line-in:<endpoint>/<input>' or 'player:<id>', each name an identifier";

/// (goal 17) What a refusal says an alarm's source is: a group's sources and
/// a stored source.
pub const ALARM_SOURCE_SPELLINGS: &str = "'stream', 'none', 'chime:<name>', \
     'line-in:<endpoint>/<input>', 'player:<id>' or 'stored:<id>', each name an identifier";

/// Longest a stored source's value may be, in bytes. ASSUMED: 2048, the
/// length every common HTTP client and server accepts in a request line
/// (the bound [`MAX_ART_URL_LEN`] uses).
pub const MAX_STORED_VALUE_LEN: usize = 2048;

/// What kind of thing a stored source names (goal 17).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StoredKind {
    /// An `http` or `https` stream URL, played by a network media player
    /// under the server's fetch policy.
    Url,
    /// A Spotify URI (`spotify:<track|album|playlist|episode>:<id>`), played
    /// by a Soloist receiver when the server runs them.
    Spotify,
}

impl StoredKind {
    /// The catalog's word for it.
    pub fn name(self) -> &'static str {
        match self {
            StoredKind::Url => "url",
            StoredKind::Spotify => "spotify",
        }
    }

    /// Read the catalog's word back.
    pub fn parse(text: &str) -> Option<StoredKind> {
        match text {
            "url" => Some(StoredKind::Url),
            "spotify" => Some(StoredKind::Spotify),
            _ => None,
        }
    }
}

/// A stored source (goal 17, K80; brief section 4.8's "stored alarm stream
/// URLs"): a named thing an alarm can play, entered once by `source_store`
/// and referred to as `stored:<id>`. It is the ONLY way a URL reaches the
/// server through the control API, and only an alarm plays it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSource {
    /// Its identifier.
    pub id: String,
    /// What kind of thing `value` is.
    pub kind: StoredKind,
    /// The URL or the URI.
    pub value: String,
    /// What a person sees, and the title a room shows while it plays.
    pub name: String,
}

impl StoredSource {
    /// Why `value` is not a value of `kind`, or `None` when it is one.
    ///
    /// A `url` is `http://` or `https://` followed by a host, holds no
    /// control character, space, quote or backslash, and is at most
    /// [`MAX_STORED_VALUE_LEN`] bytes. Whether the server may fetch it is the
    /// fetch policy's to say when it is played, not here. A `spotify` value
    /// is `spotify:<track|album|playlist|episode>:<id>`, the id 1 to 64
    /// ASCII letters and digits (ASSUMED: the shape is the goal-17 design
    /// envelope's and the bound is this crate's; no Spotify document was read).
    pub fn value_problem(kind: StoredKind, value: &str) -> Option<String> {
        if value.len() > MAX_STORED_VALUE_LEN {
            return Some(format!(
                "the value is {} bytes and a stored source holds at most {}",
                value.len(),
                MAX_STORED_VALUE_LEN
            ));
        }
        match kind {
            StoredKind::Url => {
                let rest = value
                    .strip_prefix("http://")
                    .or_else(|| value.strip_prefix("https://"));
                let Some(rest) = rest else {
                    return Some(
                        "a stored source of kind 'url' is an 'http://' or 'https://' URL"
                            .to_string(),
                    );
                };
                let host = rest.split(['/', '?', '#']).next().unwrap_or("");
                if host.is_empty() {
                    return Some("the URL names no host".to_string());
                }
                if value
                    .chars()
                    .any(|c| c.is_control() || c == ' ' || c == '"' || c == '\\')
                {
                    return Some(
                        "the URL holds a control character, a space, a quote or a backslash"
                            .to_string(),
                    );
                }
                None
            }
            StoredKind::Spotify => {
                let mut parts = value.split(':');
                let ok = parts.next() == Some("spotify")
                    && matches!(
                        parts.next(),
                        Some("track" | "album" | "playlist" | "episode")
                    )
                    && parts.next().is_some_and(|id| {
                        (1..=64).contains(&id.len())
                            && id.bytes().all(|b| b.is_ascii_alphanumeric())
                    })
                    && parts.next().is_none();
                (!ok).then(|| {
                    "a stored source of kind 'spotify' is \
                     'spotify:<track|album|playlist|episode>:<id>', the id letters and digits"
                        .to_string()
                })
            }
        }
    }
}

/// What an endpoint's input is wired to (goal 17).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InputRole {
    /// A plain line-in: a turntable, a TV, anything.
    LineIn,
    /// A bought, certified network streamer (the box that carries the
    /// licensed receivers chorus does not implement, K60): it plays into its
    /// endpoint's room when its signal appears, and the room shows its name.
    Streamer,
}

impl InputRole {
    /// The catalog's word for it.
    pub fn name(self) -> &'static str {
        match self {
            InputRole::LineIn => "line-in",
            InputRole::Streamer => "streamer",
        }
    }

    /// Read the catalog's word back.
    pub fn parse(text: &str) -> Option<InputRole> {
        match text {
            "line-in" => Some(InputRole::LineIn),
            "streamer" => Some(InputRole::Streamer),
            _ => None,
        }
    }
}

/// A person's label on an input (goal 17): its name and what it is wired to.
/// Configuration: it is kept whether or not the input is offered now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputLabel {
    /// The input.
    pub input: InputId,
    /// What a person sees; the title a room shows while a `streamer` plays.
    pub name: String,
    /// What it is wired to.
    pub role: InputRole,
}

/// What a labelled streamer's now-playing record says drives it.
pub const VIA_STREAMER: &str = "streamer";

/// Whether what a player source plays is playing, paused or still filling its
/// buffer (goal 16).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayState {
    /// Audio is going out.
    Playing,
    /// Held where it is; silence goes out.
    Paused,
    /// Waiting for enough of the media to play; silence goes out.
    Buffering,
}

impl PlayState {
    /// The catalog's word for it.
    pub fn name(self) -> &'static str {
        match self {
            PlayState::Playing => "playing",
            PlayState::Paused => "paused",
            PlayState::Buffering => "buffering",
        }
    }

    /// Read the catalog's word back.
    pub fn parse(text: &str) -> Option<PlayState> {
        match text {
            "playing" => Some(PlayState::Playing),
            "paused" => Some(PlayState::Paused),
            "buffering" => Some(PlayState::Buffering),
            _ => None,
        }
    }
}

/// Longest a now-playing title, artist or album may be, in bytes of UTF-8.
/// A longer one is cut at the last character boundary at or below this.
/// ASSUMED: 256 bytes shows any title a page or a remote has room for, and
/// bounds what a media file's tags or a control point can put into every
/// state message (three of these per playing group, once per member room).
pub const MAX_NOW_PLAYING_TEXT: usize = 256;

/// Longest a now-playing art URL may be, in bytes. A longer one is dropped,
/// not cut: a cut URL names something else. ASSUMED: 2048, the length every
/// common HTTP client and server accepts in a request line.
pub const MAX_ART_URL_LEN: usize = 2048;

/// Largest a now-playing duration may be, in ms: the largest whole number
/// every JSON reader holds exactly (2^53 - 1, RFC 8259 section 6). A larger
/// one is dropped (the duration is then unknown).
pub const MAX_DURATION_MS: u64 = (1 << 53) - 1;

/// What a group's player source is playing now (goal 16): a fact about now,
/// set by the server's runtime (`crate::zones::Zones::set_now_playing`),
/// never by a command and never persisted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NowPlaying {
    /// The title, when known.
    pub title: Option<String>,
    /// The artist, when known.
    pub artist: Option<String>,
    /// The album, when known.
    pub album: Option<String>,
    /// Where the artwork is, when known: an `http` or `https` URL.
    pub art_url: Option<String>,
    /// How long the media is, ms, when known (a live stream has none).
    pub duration_ms: Option<u64>,
    /// Playing, paused or buffering.
    pub state: PlayState,
    /// What is driving the player, an identifier: `upnp` for a UPnP AV
    /// control point (later goals add their own).
    pub via: String,
}

impl NowPlaying {
    /// The record held to its bounds, which is what the room model stores:
    /// in a title, artist or album every control character becomes a space,
    /// the ends are trimmed, the text is cut to [`MAX_NOW_PLAYING_TEXT`]
    /// bytes at a character boundary, and an empty one is absent; an art URL
    /// that is not `http://` or `https://`, holds a control character or a
    /// space, or is longer than [`MAX_ART_URL_LEN`] is absent; a duration
    /// above [`MAX_DURATION_MS`] is absent.
    pub fn bounded(self) -> NowPlaying {
        NowPlaying {
            title: self.title.and_then(bounded_text),
            artist: self.artist.and_then(bounded_text),
            album: self.album.and_then(bounded_text),
            art_url: self.art_url.filter(|u| {
                (u.starts_with("http://") || u.starts_with("https://"))
                    && u.len() <= MAX_ART_URL_LEN
                    && !u.chars().any(|c| c.is_control() || c == ' ')
            }),
            duration_ms: self.duration_ms.filter(|d| *d <= MAX_DURATION_MS),
            state: self.state,
            via: self.via,
        }
    }
}

/// A displayed text held to [`MAX_NOW_PLAYING_TEXT`]; `None` when nothing is
/// left of it.
fn bounded_text(text: String) -> Option<String> {
    let clean: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let mut clean = clean.trim();
    if clean.len() > MAX_NOW_PLAYING_TEXT {
        let mut end = MAX_NOW_PLAYING_TEXT;
        while !clean.is_char_boundary(end) {
            end -= 1;
        }
        clean = clean[..end].trim_end();
    }
    (!clean.is_empty()).then(|| clean.to_string())
}

/// One endpoint's input: `<endpoint>/<input>`, both identifiers.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct InputId {
    /// The endpoint offering it.
    pub endpoint: String,
    /// Which of its inputs.
    pub input: String,
}

impl InputId {
    /// The one spelling.
    pub fn literal(&self) -> String {
        format!("{}/{}", self.endpoint, self.input)
    }

    /// Read the one spelling back.
    pub fn parse(text: &str) -> Option<InputId> {
        let (endpoint, input) = text.split_once('/')?;
        (is_identifier(endpoint) && is_identifier(input)).then(|| InputId {
            endpoint: endpoint.to_string(),
            input: input.to_string(),
        })
    }
}

/// A persisted group definition (K59): always listed, active or not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedGroup {
    /// The group identifier its rooms join when it is taken.
    pub id: String,
    /// The human-set name.
    pub name: String,
    /// Its rooms, in the order given.
    pub zones: Vec<String>,
}

/// A configured alarm. Firing it is the runtime's; this is what it fires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alarm {
    /// The alarm's identifier.
    pub id: String,
    /// A room or a saved group.
    pub target: String,
    /// When, in civil time.
    pub time: ClockTime,
    /// Which days. Empty means once, at the next `time`.
    pub days: Days,
    /// What it plays.
    pub source: Source,
    /// The volume it ramps to, clamped to each room's effective limit when it
    /// is applied like every other volume.
    pub volume: Volume,
    /// Seconds from silence to `volume`.
    pub ramp_s: u32,
    /// Minutes it plays before stopping by itself; 0 plays until stopped.
    pub duration_min: u32,
    /// Whether it fires at all.
    pub enabled: bool,
}

/// Longest an alarm's ramp may be, in seconds. ASSUMED: ten minutes is far past
/// any wake-up ramp a commercial alarm offers and bounds a typo.
pub const MAX_RAMP_S: u32 = 600;

/// Longest an alarm may play by itself, in minutes. ASSUMED: twelve hours.
pub const MAX_DURATION_MIN: u32 = 720;

/// Longest a sleep timer may run, in minutes. ASSUMED: twelve hours.
pub const MAX_SLEEP_MIN: u32 = 720;

/// Most quiet-hours windows one room may have. ASSUMED: enough for a
/// different window every day of the week, with one to spare.
pub const MAX_QUIET_WINDOWS: usize = 8;

/// Most saved groups, alarms or autoplay rules a house may hold, each. ASSUMED:
/// a bound on the state message's size (every one of these is in every state
/// message), well past what a house of a dozen rooms uses.
pub const MAX_DEFINITIONS: usize = 32;

/// An autoplay rule: when this input offers a signal, play it here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Autoplay {
    /// The input.
    pub input: InputId,
    /// A room or a saved group.
    pub target: String,
    /// Whether the rule is in force.
    pub enabled: bool,
    /// (goal 13, K81) For a TV input (`optical` or `hdmi_arc`): the TV's
    /// standby stops it at once, without the hold, and restores what the
    /// target played. Default true; a non-TV input ignores it.
    pub stop_on_standby: bool,
    /// (goal 13) For a TV input: play it in low-latency mode when the target
    /// is one wired room (the integration track's relay), else the slot path.
    /// Default true; false keeps the TV on the slot path always.
    pub low_latency: bool,
}

/// A sleep timer as asked for. The countdown is the runtime's, on the
/// monotonic clock; this is the request, which is why it is not persisted (a
/// restarted server cannot know how much of it was left).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SleepTimer {
    /// A room or a group that exists now.
    pub target: String,
    /// Minutes asked for.
    pub minutes: u32,
    /// (ADR 0194) Whole seconds left, rounded up, as the server's schedule
    /// runtime last counted them; `None` in a model nothing counts down.
    pub remaining_s: Option<u32>,
}

/// (ADR 0194) What kind of input an offered input is, in the sync protocol's
/// own words for a source endpoint's inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    /// An analogue line input.
    LineIn,
    /// An optical (TOSLINK) S/PDIF input.
    Optical,
    /// HDMI ARC or eARC from a TV.
    HdmiArc,
}

impl InputKind {
    /// Every kind, in the protocol's order.
    pub const ALL: [InputKind; 3] = [InputKind::LineIn, InputKind::Optical, InputKind::HdmiArc];

    /// The catalog's name for it, which is the protocol's.
    pub fn name(self) -> &'static str {
        match self {
            InputKind::LineIn => "line_in",
            InputKind::Optical => "optical",
            InputKind::HdmiArc => "hdmi_arc",
        }
    }

    /// Read the name back.
    pub fn from_name(name: &str) -> Option<InputKind> {
        InputKind::ALL.into_iter().find(|k| k.name() == name)
    }

    /// Whether it is a TV's input (`docs/inputs.md`): its audio is the
    /// picture's, and the TV going to standby stops what plays it.
    pub fn is_tv(self) -> bool {
        matches!(self, InputKind::Optical | InputKind::HdmiArc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clock_time_has_one_spelling() {
        for (text, minutes) in [("00:00", 0u16), ("07:30", 450), ("23:59", 1439)] {
            let t = ClockTime::parse(text).unwrap();
            assert_eq!(t.minutes(), minutes);
            assert_eq!(t.literal(), text);
        }
        for text in ["24:00", "7:30", "07:3", "07-30", "12:60", "", "ab:cd"] {
            assert_eq!(ClockTime::parse(text), None, "{}", text);
        }
    }

    #[test]
    fn days_are_listed_once_and_in_week_order() {
        let d = Days::from_names(["mon", "wed", "sun"]).unwrap();
        assert_eq!(d.names(), vec!["mon", "wed", "sun"]);
        assert!(Days::from_names(["wed", "mon"]).is_err());
        assert!(Days::from_names(["mon", "mon"]).is_err());
        assert!(Days::from_names(["monday"]).is_err());
    }

    #[test]
    fn a_window_past_midnight_belongs_to_the_day_it_starts() {
        let w = QuietWindow {
            days: Days::from_names(["fri"]).unwrap(),
            start: ClockTime::parse("22:00").unwrap(),
            end: ClockTime::parse("07:00").unwrap(),
            limit: Volume::from_thousandths(300).unwrap(),
        };
        let at = |weekday: u8, t: &str| CivilTime {
            weekday,
            time: ClockTime::parse(t).unwrap(),
        };
        assert!(w.contains(at(4, "22:00")), "Friday at the start");
        assert!(w.contains(at(5, "03:00")), "Saturday morning");
        assert!(!w.contains(at(5, "07:00")), "the end is outside");
        assert!(!w.contains(at(4, "21:59")));
        assert!(!w.contains(at(3, "23:00")), "Thursday is not a start day");
        assert!(
            !w.contains(at(4, "03:00")),
            "Friday morning belongs to Thursday"
        );
    }

    #[test]
    fn the_layouts_are_the_ones_the_envelope_names() {
        use Role::*;
        for ok in [
            &[Fl, Fr][..],
            &[Fl, Fr, Lfe],
            &[Fl, Fr, Fc],
            &[Fl, Fr, Fc, Lfe],
            &[Fl, Fr, Fc, Sl, Sr],
            &[Fl, Fr, Fc, Lfe, Sl, Sr],
            &[Fl, Fr, Fc, Lfe, Bl, Br],
        ] {
            assert_eq!(validate_layout(ok), None, "{:?}", ok);
        }
        for bad in [
            &[Fl][..],
            &[Fr, Lfe],
            &[Fl, Fr, Sl, Sr],
            &[Fl, Fr, Fc, Sl],
            &[Fl, Fr, Fc, Sl, Sr, Bl, Br],
            &[Fl, Fr, Fc, Sl, Br],
        ] {
            assert!(validate_layout(bad).is_some(), "{:?}", bad);
        }
    }

    #[test]
    fn a_source_has_one_spelling() {
        for text in [
            "stream",
            "none",
            "chime:bell",
            "line-in:endpoint-a/line-1",
            "player:p0",
        ] {
            assert_eq!(Source::parse(text).unwrap().literal(), text);
        }
        for text in [
            "Stream",
            "chime:",
            "chime:Bell",
            "line-in:a",
            "line-in:a/",
            "player:",
            "player:P0",
            "player:p0/x",
            "x",
        ] {
            assert_eq!(Source::parse(text), None, "{}", text);
        }
    }

    #[test]
    fn a_now_playing_record_is_held_to_its_bounds() {
        let long = "\u{e9}".repeat(200); // 400 bytes of two-byte characters
        let record = NowPlaying {
            title: Some(format!("  A\ttitle\n{}", long)),
            artist: Some(" \r\n ".to_string()),
            album: Some("Album".to_string()),
            art_url: Some("javascript:alert(1)".to_string()),
            duration_ms: Some(u64::MAX),
            state: PlayState::Buffering,
            via: "upnp".to_string(),
        }
        .bounded();
        let title = record.title.unwrap();
        assert!(title.starts_with("A title "), "{}", title);
        assert!(title.len() <= MAX_NOW_PLAYING_TEXT, "{}", title.len());
        assert!(title.ends_with('\u{e9}'), "cut on a character boundary");
        assert_eq!(record.artist, None, "nothing left is nothing said");
        assert_eq!(record.album.as_deref(), Some("Album"));
        assert_eq!(record.art_url, None, "only http and https");
        assert_eq!(record.duration_ms, None);
        // An odd bound: a three-byte character straddling it is dropped whole.
        let wide = "\u{20ac}".repeat(100); // 300 bytes
        let cut = bounded_text(wide).unwrap();
        assert_eq!(cut.len(), 255);
        let url = format!("https://example.invalid/{}", "a".repeat(MAX_ART_URL_LEN));
        let kept = NowPlaying {
            title: None,
            artist: None,
            album: None,
            art_url: Some("http://192.0.2.7:8200/art/1.jpg".to_string()),
            duration_ms: Some(MAX_DURATION_MS),
            state: PlayState::Playing,
            via: "upnp".to_string(),
        };
        assert_eq!(kept.clone().bounded(), kept);
        assert_eq!(
            NowPlaying {
                art_url: Some(url),
                ..kept
            }
            .bounded()
            .art_url,
            None,
            "a URL past the bound is dropped, never cut"
        );
        for state in [PlayState::Playing, PlayState::Paused, PlayState::Buffering] {
            assert_eq!(PlayState::parse(state.name()), Some(state));
        }
    }

    #[test]
    fn a_quiet_window_survives_its_persisted_spelling() {
        let text = "mon,tue,sun 22:00-07:00 0.300";
        let w = QuietWindow::from_persisted(text).unwrap();
        assert_eq!(w.persisted(), text);
        assert!(QuietWindow::from_persisted("mon 22:00-22:00 0.300").is_err());
        assert!(QuietWindow::from_persisted(" 22:00-07:00 0.300").is_err());
    }
}

/// What a `playback` command asks of the Spotify receiver a group plays
/// (goal 17, K65).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackAction {
    /// Pause.
    Pause,
    /// Carry on after a pause.
    Resume,
    /// The next track.
    Next,
    /// The previous track, or the start of this one.
    Previous,
}

impl PlaybackAction {
    /// Every action, in the catalog's order.
    pub const ALL: [PlaybackAction; 4] = [
        PlaybackAction::Pause,
        PlaybackAction::Resume,
        PlaybackAction::Next,
        PlaybackAction::Previous,
    ];

    /// The one spelling.
    pub fn name(self) -> &'static str {
        match self {
            PlaybackAction::Pause => "pause",
            PlaybackAction::Resume => "resume",
            PlaybackAction::Next => "next",
            PlaybackAction::Previous => "previous",
        }
    }

    /// Read the one spelling back.
    pub fn parse(text: &str) -> Option<PlaybackAction> {
        PlaybackAction::ALL.into_iter().find(|a| a.name() == text)
    }
}

/// One Spotify Soloist receiver, as the state lists it (goal 17).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoloistReceiver {
    /// `r0`, `r1`, ...
    pub id: String,
    /// What its supervisor says it is doing (`idle`, `starting`, `running`,
    /// `expired`, `failed`, `no-binary`), or `absent` while the server has
    /// no connection to it.
    pub state: String,
    /// The target it is assigned to (`room:<id>`, `group:<id>`,
    /// `live:<a>+<b>`), or empty.
    pub target: String,
    /// The Spotify Connect device name it runs under, or empty.
    pub name: String,
}

/// The Soloist build the receivers run (goal 17).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoloistBuild {
    /// The first line of `soloist --version`.
    pub version: String,
    /// Whole days until the build expires (negative once it has), when the
    /// version names a build time.
    pub expires_in_days: Option<i64>,
}

/// The state's `soloist` member (goal 17): the receivers, the build and its
/// expiry warning. A fact about now, set by the server's receiver manager
/// (`crate::zones::Zones::set_soloist`), never by a command and never
/// persisted; a server without `--soloist-receivers` has none and writes no
/// such member.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SoloistState {
    /// Every receiver, in order.
    pub receivers: Vec<SoloistReceiver>,
    /// The build, once a supervisor has reported one.
    pub build: Option<SoloistBuild>,
    /// "Soloist build expires in N days" from 14 days before, "Soloist
    /// build expired" after.
    pub warning: Option<String>,
    /// The targets that have no receiver because the pool is too small.
    pub exhausted: Vec<String>,
}

/// (goal 18) An origin an announcement's URL may come from: a scheme, a host
/// and a port (`--announce-origin <scheme://host[:port]>`). Brief section
/// 4.8 names "HA's media and TTS URLs from HA's own address" as an input
/// path; this is that address, as the server's operator configured it.
///
/// Compared whole: the scheme, the host (ASCII case-insensitive; a name is
/// compared as text and never resolved here) and the port, the scheme's
/// default (80 or 443) when none is written.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Origin {
    scheme: &'static str,
    host: String,
    port: u16,
}

impl Origin {
    /// Read `scheme://host[:port]`, nothing after it but an optional single
    /// `/`. Says why not when it is not one.
    pub fn parse(text: &str) -> Result<Origin, String> {
        let (origin, rest) = Origin::split(text)?;
        if !(rest.is_empty() || rest == "/") {
            return Err(format!(
                "'{}' carries a path, a query or a fragment; an origin is \
                 scheme://host[:port] and nothing else",
                text.escape_debug()
            ));
        }
        Ok(origin)
    }

    /// The origin of an `http` or `https` URL. Says why not when the URL
    /// has none this type can hold (another scheme, no host, userinfo, a
    /// port that is not one).
    pub fn of_url(url: &str) -> Result<Origin, String> {
        Origin::split(url).map(|(origin, _)| origin)
    }

    fn split(text: &str) -> Result<(Origin, &str), String> {
        let lower = text.get(..8).unwrap_or(text).to_ascii_lowercase();
        let (scheme, default_port, rest) = if lower.starts_with("http://") {
            ("http", 80, &text[7..])
        } else if lower.starts_with("https://") {
            ("https", 443, &text[8..])
        } else {
            return Err("the scheme is not 'http' or 'https'".to_string());
        };
        let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let (authority, rest) = rest.split_at(end);
        if authority.contains('@') {
            return Err("it carries userinfo ('user@'), which is never fetched".to_string());
        }
        let (host, port) = if let Some(inner) = authority.strip_prefix('[') {
            // An IPv6 literal, in brackets, as a URL writes one.
            let Some((address, after)) = inner.split_once(']') else {
                return Err("an address in brackets has no closing bracket".to_string());
            };
            if address.is_empty()
                || !address
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() || b == b':' || b == b'.')
            {
                return Err("the address in brackets is not an IPv6 address".to_string());
            }
            let port = match after.strip_prefix(':') {
                Some(port) => Some(port),
                None if after.is_empty() => None,
                None => return Err("text follows the address in brackets".to_string()),
            };
            (format!("[{}]", address.to_ascii_lowercase()), port)
        } else {
            let (host, port) = match authority.split_once(':') {
                Some((host, port)) => (host, Some(port)),
                None => (authority, None),
            };
            if host.is_empty() {
                return Err("it names no host".to_string());
            }
            if host.len() > 253
                || !host
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
            {
                return Err(
                    "the host is not ASCII letters, digits, hyphens and dots (at most 253)"
                        .to_string(),
                );
            }
            (host.to_ascii_lowercase(), port)
        };
        let port = match port {
            None => default_port,
            Some(digits) => {
                let number = if !digits.is_empty()
                    && digits.len() <= 5
                    && digits.bytes().all(|b| b.is_ascii_digit())
                {
                    digits.parse::<u32>().ok()
                } else {
                    None
                };
                match number {
                    Some(n) if (1..=65535).contains(&n) => n as u16,
                    _ => return Err("the port is not a number from 1 to 65535".to_string()),
                }
            }
        };
        Ok((Origin { scheme, host, port }, rest))
    }

    /// `http` or `https`.
    pub fn scheme(&self) -> &str {
        self.scheme
    }

    /// The host, lower-cased; an IPv6 address keeps its brackets.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The port, the scheme's default when none was written.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The one spelling: `scheme://host`, then `:port` unless it is the
    /// scheme's default.
    pub fn literal(&self) -> String {
        let default_port = if self.scheme == "https" { 443 } else { 80 };
        if self.port == default_port {
            format!("{}://{}", self.scheme, self.host)
        } else {
            format!("{}://{}:{}", self.scheme, self.host, self.port)
        }
    }
}

#[cfg(test)]
mod origin_tests {
    use super::Origin;

    #[test]
    fn an_origin_is_a_scheme_a_host_and_a_port_and_has_one_spelling() {
        let o = Origin::parse("http://HA.example:8123").unwrap();
        assert_eq!(
            (o.scheme(), o.host(), o.port()),
            ("http", "ha.example", 8123)
        );
        assert_eq!(o.literal(), "http://ha.example:8123");
        assert_eq!(
            Origin::parse("https://ha.example:443/").unwrap().literal(),
            "https://ha.example"
        );
        assert_eq!(
            Origin::parse("http://[2001:DB8::1]:8123")
                .unwrap()
                .literal(),
            "http://[2001:db8::1]:8123"
        );
        for bad in [
            "ha.example:8123",
            "ftp://ha.example",
            "http://",
            "http://ha.example:8123/api",
            "http://ha.example:0",
            "http://ha.example:65536",
            "http://ha.example:",
            "http://user@ha.example",
            "http://ha exam.ple",
            "http://[::1",
            "http://[::1]x",
        ] {
            assert!(Origin::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_urls_origin_is_what_precedes_its_path_and_userinfo_never_passes() {
        let allowed = Origin::parse("http://ha.example:8123").unwrap();
        let of = |url: &str| Origin::of_url(url);
        assert_eq!(
            of("http://ha.example:8123/api/tts_proxy/abc.mp3").unwrap(),
            allowed
        );
        assert_eq!(of("HTTP://HA.EXAMPLE:8123?x=1").unwrap(), allowed);
        // The same host on another port or scheme is another origin.
        assert_ne!(of("http://ha.example/abc.mp3").unwrap(), allowed);
        assert_ne!(of("https://ha.example:8123/abc.mp3").unwrap(), allowed);
        assert_ne!(
            of("http://ha.example.evil.example:8123/a").unwrap(),
            allowed
        );
        // The classic confusion: the allowed origin written as userinfo.
        assert!(of("http://ha.example:8123@evil.example/abc.mp3").is_err());
        assert!(of("file:///etc/passwd").is_err());
    }
}
