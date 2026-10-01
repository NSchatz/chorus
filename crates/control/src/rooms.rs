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
/// `none` is nothing, `chime:<name>` one of the generated chimes, and
/// `line-in:<endpoint>/<input>` an endpoint's offered line-in. This crate
/// validates the spelling; which chimes exist and which inputs are offered is
/// the runtime's to know, so a well-spelled source naming one that does not
/// exist is the runtime's refusal, not the catalog's.
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
}

impl Source {
    /// The one spelling.
    pub fn literal(&self) -> String {
        match self {
            Source::Stream => "stream".to_string(),
            Source::None => "none".to_string(),
            Source::Chime(name) => format!("chime:{}", name),
            Source::LineIn(input) => format!("line-in:{}", input.literal()),
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
        None
    }
}

/// What a refusal says a source is.
pub const SOURCE_SPELLINGS: &str =
    "'stream', 'none', 'chime:<name>' or 'line-in:<endpoint>/<input>', each name an identifier";

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
        for text in ["stream", "none", "chime:bell", "line-in:endpoint-a/line-1"] {
            assert_eq!(Source::parse(text).unwrap().literal(), text);
        }
        for text in [
            "Stream",
            "chime:",
            "chime:Bell",
            "line-in:a",
            "line-in:a/",
            "x",
        ] {
            assert_eq!(Source::parse(text), None, "{}", text);
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
