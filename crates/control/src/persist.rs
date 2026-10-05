//! The persisted zone state: what survives a restart, and in what shape.
//!
//! # Why this is a `key = value` file and not the state message
//!
//! The obvious thing would be to write the state message's JSON to a file and
//! read it back. It is rejected for one reason: the state message is a WIRE
//! format with a catalog version on it, and a wire format that is also a
//! storage format cannot be changed without a migration. This file is a
//! separate format with its own version, in the `key = value` shape every other
//! committed file in this repository uses (`config/sync.conf`,
//! `audio-path.conf`, `fixtures/*/*.fields`), so it is reviewable in a diff and
//! readable by a person with an editor when something has gone wrong.
//!
//! `docs/decisions/0018-the-persisted-zone-state.md` records the rest of it,
//! including what is deliberately NOT persisted.
//!
//! # Every value the catalog accepts comes back, byte for byte
//!
//! A `key = value` file with `#` comments has one hazard, and this format hit
//! it: a zone NAME is human-set text and the catalog accepts a `#` in one, so
//! `name = Kitchen #1` written plainly reads back as `Kitchen`, and `name = #1`
//! reads back as nothing at all - which the loader then refuses, so the server
//! replacing a killed one does not start. That is silent state loss on exactly
//! the path the restart criterion is about.
//!
//! So every value is written ESCAPED and read UNESCAPED, with two escapes and
//! no more: `\\` for a backslash and `\#` for a hash. A `#` that is not
//! preceded by a backslash still starts a comment, so the file stays
//! hand-editable and hand-commentable, and an escape this format does not have
//! is refused by name rather than guessed at.
//!
//! The invariant, which [`write_file`] enforces on every write rather than
//! trusting: **a state file this build writes is one this build reads back as
//! the same state**. A render that does not survive its own loader is never
//! installed over a file that can be read.
//!
//! # A write is a rename
//!
//! The file is written to a temporary beside it and renamed over the old one,
//! so a process killed mid-write leaves either the old state or the new one and
//! never half of either. That matters here more than usual: the criterion this
//! serves is about a server killed with SIGKILL.
//!
//! And a rename is only as durable as what the disk was told (goal 2's audit,
//! B-9): the temporary is `fsync`ed BEFORE it is renamed, so the name never
//! points at data still in a page cache, and the directory is `fsync`ed AFTER,
//! so the rename itself survives a power cut and not only a killed process.
//!
//! # Format 2 (catalog v2)
//!
//! Format 2 adds a room's `limit`, `quiet` hours and `bond`, and four section
//! kinds: `[endpoint <id>]` (its link), `[saved-group <id>]`, `[alarm <id>]`
//! and `[autoplay <endpoint>/<input>]`. A format 1 file still loads, unchanged,
//! with every format 2 field at its default; the next write is format 2. What
//! is a fact about NOW stays out, as in format 1: which endpoints are present,
//! which quiet window is active, which alarm is ringing, what a group is
//! playing, a sleep timer's countdown, and which inputs are offered.
//!
//! # Format 3 (goal 12: per-room sound)
//!
//! Format 3 adds nine fields to `[zone]`: `bass`, `treble`, `loudness`,
//! `night`, `speech`, `crossover_hz`, `sub_level_db`, `sub_polarity`,
//! `room_eq` (0 or 1, whether the filters are applied) and `room_eq_filters`
//! (`freq_hz gain_db q` per filter, `; ` between them). Every one is required
//! in a format 3 file. A format 1 or 2 file loads unchanged with every room
//! at the sound defaults (`crate::sound`); the next write is format 3.
//!
//! # Format 4 (goal 13: the TV path)
//!
//! Format 4 adds two fields to `[zone]`, `tv_upmix` (`off` or `ambient`) and
//! `av_trim_ms` (a whole number, -100 to 200), and two to `[autoplay]`,
//! `stop_on_standby` and `low_latency` (0 or 1). Every one is required in a
//! format 4 file. A format 1, 2 or 3 file loads unchanged with every room at
//! `tv_upmix = off` and no trim and every rule at 1 for both (their
//! defaults, `crate::theater` and `crate::rooms::Autoplay`); the next write is
//! format 4.
//!
//! # Format 5 (goal 14: the adopted speakers)
//!
//! Format 5 adds one section kind, `[speaker <id>]`, with three fields, each
//! required: `name`, `named` (0 or 1: whether a person named it) and `room`
//! (a zone of this file, or empty for none). A format 1 to 4 file loads
//! unchanged with no speaker record (the server makes one for every key it
//! has pinned when it starts, unnamed and in no room); the next write is
//! format 5. What is a fact about now stays out, as ever: whether a speaker's
//! session is up, its software, its roles, its key's fingerprint (the pin
//! file is the server's, `adopted-endpoints`), and the key changes refused.
//!
//! # Format 6 (goal 17: stored sources and input labels)
//!
//! Format 6 adds two section kinds: `[stored-source <id>]` with three fields,
//! each required: `kind` (`url` or `spotify`), `value` (held to the rule the
//! catalog holds a `source_store` to) and `name`; and
//! `[input-label <endpoint>/<input>]` with two, each required: `name` and
//! `role` (`line-in` or `streamer`). An alarm whose source is `stored:<id>`
//! names a stored source of this file. A format 1 to 5 file loads unchanged
//! with neither; the next write is format 6. What a stored source's player
//! is playing, and whether a labelled input is offered, are facts about now
//! and stay out.
//!
//! # Format 7 (quiet hours switched off and on)
//!
//! Format 7 adds one field to `[zone]`, required: `quiet_enabled` (0 or 1,
//! whether the room's quiet-hours windows cap it). The windows in `quiet`
//! are stored either way. A format 1 to 6 file loads unchanged with every
//! room's quiet hours enabled, which is what those formats meant; the next
//! write is format 7.
//!
//! # Format 8 (the voice path switched on per room)
//!
//! Format 8 adds one field to `[zone]`, required: `voice_enabled` (0 or 1,
//! whether the room's microphones may send audio to the voice path). A
//! format 1 to 7 file loads unchanged with every room's voice path off,
//! which is the default; the next write is format 8. What a microphone's
//! gate reports is a fact about now and stays out.
//!
//! # Format 9 (a room's choice of wake words)
//!
//! Format 9 adds one field to `[zone]`, required: `wake_words`, either `*`
//! (the room listens for every wake word the server runs, the default) or
//! the ids the room chose, comma-separated, possibly none. A format 1 to 8
//! file loads unchanged with `*` in every room; the next write is format 9.
//! Which models a build runs is a fact about the build and stays out, so an
//! id is held to its shape only.
//!
//! # Format 10 (the correction a room can go back to)
//!
//! Format 10 adds two fields to `[zone]`, each required: `room_eq_undo`
//! (`none`, or 0 or 1: whether the correction `room_eq_undo` puts back was
//! applied) and `room_eq_undo_filters` (its filters, written as
//! `room_eq_filters` is; empty when `room_eq_undo` is `none`). A format 1 to
//! 9 file loads unchanged with nothing to undo in any room; the next write is
//! format 10. A recording a correction was fitted from is never in this file
//! or any other: the server does not keep one
//! (`docs/decisions/0200-a-recording-is-fitted-and-not-kept.md`).

use std::fmt;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::catalog::{
    is_display_name, is_identifier, is_wake_word_id, Command, Volume, MAX_WAKE_WORDS,
};
use crate::rooms::{
    validate_layout, Alarm, Autoplay, BondMember, ClockTime, Days, InputId, InputLabel, InputRole,
    Link, QuietWindow, Role, SavedGroup, Source, StoredKind, StoredSource, MAX_DURATION_MIN,
    MAX_QUIET_WINDOWS, MAX_RAMP_S,
};
use crate::sound::{
    EqFilter, FixedPoint, Polarity, RoomEq, CROSSOVER_HZ, ROOM_EQ_MAX_FILTERS, SUB_LEVEL_CDB,
    TONE_DB,
};
use crate::speakers::Speaker;
use crate::theater::{TvUpmix, AV_TRIM_MS};
use crate::zones::{Zone, Zones};

/// The version of this file format, which is what every write produces.
pub const STATE_FORMAT: u32 = 10;

/// Every format this build reads.
pub const READ_FORMATS: &[u32] = &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

/// Why persisted state could not be read.
#[derive(Debug)]
pub enum StateError {
    /// The file could not be read or written.
    Io(io::Error),
    /// The file is not this format.
    Malformed {
        /// One-based line number, or zero where the fault is the whole file.
        line: usize,
        /// What was wrong.
        detail: String,
    },
}

impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StateError::Io(e) => write!(f, "{}", e),
            StateError::Malformed { line: 0, detail } => write!(f, "{}", detail),
            StateError::Malformed { line, detail } => write!(f, "line {}: {}", line, detail),
        }
    }
}

impl std::error::Error for StateError {}

impl From<io::Error> for StateError {
    fn from(e: io::Error) -> StateError {
        StateError::Io(e)
    }
}

/// Write one value the way this format holds it: a backslash and a hash carry
/// a backslash in front of them, and nothing else is touched.
///
/// Applied to EVERY value rendered, not only to the one field that is known to
/// need it today, so a field that later becomes free text cannot reintroduce
/// the fault by being forgotten here.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c == '\\' || c == '#' {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Read one value back.
///
/// Refuses an escape this format does not have rather than dropping the
/// backslash or the character after it: a value nobody can write here is
/// better than a value that comes back as something else.
fn unescape(text: &str) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('#') => out.push('#'),
            Some(other) => {
                return Err(format!(
                "'\\{}' is not an escape this format has; the only escapes are '\\\\' and '\\#'",
                other
            ))
            }
            None => return Err("a value ends with a lone '\\'".to_string()),
        }
    }
    Ok(out)
}

/// The part of one line that is not a comment.
///
/// A `#` starts a comment only where it is not escaped, which is what makes a
/// `#` inside a value survive the round trip. The escape sequences are left in
/// place for [`unescape`] to resolve after the line has been split.
fn code(raw: &str) -> &str {
    let mut chars = raw.char_indices();
    while let Some((at, c)) = chars.next() {
        match c {
            '\\' => {
                chars.next();
            }
            '#' => return &raw[..at],
            _ => {}
        }
    }
    raw
}

/// Render the persisted form of `zones`.
///
/// Deterministic: the same state renders the same bytes, so a state file is
/// reviewable in a diff and a test can assert on it. Always the current
/// format, [`STATE_FORMAT`].
pub fn render(zones: &Zones) -> String {
    let mut out = String::new();
    out.push_str("# chorus zone state, written by chorus-server.\n");
    out.push_str("#\n");
    out.push_str("# Format: key = value, one [zone <id>] section per zone. An unescaped '#'\n");
    out.push_str("# starts a comment; a value writes a hash as '\\#' and a backslash as '\\\\',\n");
    out.push_str("# and has no other escape, so any name the catalog accepts comes back.\n");
    out.push_str("# docs/decisions/0018-the-persisted-zone-state.md says what is here and what\n");
    out.push_str(
        "# deliberately is not. Editing this file by hand is supported; the server reads\n",
    );
    out.push_str("# it once at start and refuses to start on a file it cannot parse.\n");
    out.push_str("# Format 2 adds [endpoint], [saved-group], [alarm] and [autoplay] sections\n");
    out.push_str("# and a room's limit, quiet hours and bonded set (docs/control-plane.md);\n");
    out.push_str("# format 3 a room's sound, bass management and correction EQ; format 4\n");
    out.push_str("# a room's TV upmix and A/V trim and an autoplay rule's TV behaviour;\n");
    out.push_str("# format 5 the adopted speakers ([speaker]: name, named, room);\n");
    out.push_str("# format 6 stored sources ([stored-source]) and input labels ([input-label]);\n");
    out.push_str("# format 7 whether a room's quiet hours are switched on (quiet_enabled);\n");
    out.push_str("# format 8 whether a room's voice path is switched on (voice_enabled);\n");
    out.push_str("# format 9 which wake words a room listens for (wake_words, * for every one);\n");
    out.push_str(
        "# format 10 the correction a room's undo puts back (room_eq_undo, none or 0 or 1).\n",
    );
    out.push('\n');
    out.push_str(&format!("format = {}\n", STATE_FORMAT));
    out.push_str(&format!("serial = {}\n", zones.serial()));
    for zone in zones.zones() {
        out.push('\n');
        out.push_str(&format!("[zone {}]\n", escape(&zone.id)));
        out.push_str(&format!("name = {}\n", escape(&zone.name)));
        out.push_str(&format!("group = {}\n", escape(&zone.group)));
        out.push_str(&format!("volume = {}\n", escape(&zone.volume.literal())));
        out.push_str(&format!("muted = {}\n", u8::from(zone.muted)));
        out.push_str(&format!(
            "endpoints = {}\n",
            zone.endpoints
                .iter()
                .map(|e| escape(e))
                .collect::<Vec<_>>()
                .join(",")
        ));
        out.push_str(&format!("limit = {}\n", zone.limit.literal()));
        out.push_str(&format!(
            "quiet = {}\n",
            zone.quiet
                .iter()
                .map(|w| w.persisted())
                .collect::<Vec<_>>()
                .join("; ")
        ));
        out.push_str(&format!(
            "bond = {}\n",
            zone.bond
                .iter()
                .map(|b| format!("{}:{}", b.role, escape(&b.endpoint)))
                .collect::<Vec<_>>()
                .join(",")
        ));
        out.push_str(&format!("bass = {}\n", zone.sound.bass));
        out.push_str(&format!("treble = {}\n", zone.sound.treble));
        out.push_str(&format!("loudness = {}\n", u8::from(zone.sound.loudness)));
        out.push_str(&format!("night = {}\n", u8::from(zone.sound.night)));
        out.push_str(&format!("speech = {}\n", u8::from(zone.sound.speech)));
        out.push_str(&format!("crossover_hz = {}\n", zone.bass.crossover_hz));
        out.push_str(&format!(
            "sub_level_db = {}\n",
            FixedPoint::literal(i64::from(zone.bass.sub_level_cdb), 2)
        ));
        out.push_str(&format!("sub_polarity = {}\n", zone.bass.sub_polarity));
        out.push_str(&format!("room_eq = {}\n", u8::from(zone.room_eq.enabled)));
        out.push_str(&format!(
            "room_eq_filters = {}\n",
            zone.room_eq
                .filters
                .iter()
                .map(|f| f.persisted())
                .collect::<Vec<_>>()
                .join("; ")
        ));
        out.push_str(&format!(
            "room_eq_undo = {}\n",
            match &zone.room_eq_undo {
                None => "none".to_string(),
                Some(before) => u8::from(before.enabled).to_string(),
            }
        ));
        out.push_str(&format!(
            "room_eq_undo_filters = {}\n",
            zone.room_eq_undo
                .iter()
                .flat_map(|before| &before.filters)
                .map(|f| f.persisted())
                .collect::<Vec<_>>()
                .join("; ")
        ));
        out.push_str(&format!("tv_upmix = {}\n", zone.sound.tv_upmix));
        out.push_str(&format!("av_trim_ms = {}\n", zone.av_trim_ms));
        out.push_str(&format!(
            "quiet_enabled = {}\n",
            u8::from(zone.quiet_enabled)
        ));
        out.push_str(&format!(
            "voice_enabled = {}\n",
            u8::from(zone.voice_enabled)
        ));
        out.push_str(&format!(
            "wake_words = {}\n",
            match &zone.wake_words {
                None => "*".to_string(),
                Some(chosen) => chosen.join(","),
            }
        ));
    }
    for (endpoint, link) in zones.links() {
        out.push('\n');
        out.push_str(&format!("[endpoint {}]\n", escape(endpoint)));
        out.push_str(&format!("link = {}\n", link));
    }
    for group in zones.saved_groups() {
        out.push('\n');
        out.push_str(&format!("[saved-group {}]\n", escape(&group.id)));
        out.push_str(&format!("name = {}\n", escape(&group.name)));
        out.push_str(&format!("zones = {}\n", group.zones.join(",")));
    }
    for alarm in zones.alarms() {
        out.push('\n');
        out.push_str(&format!("[alarm {}]\n", escape(&alarm.id)));
        out.push_str(&format!("target = {}\n", alarm.target));
        out.push_str(&format!("time = {}\n", alarm.time));
        out.push_str(&format!("days = {}\n", alarm.days.names().join(",")));
        out.push_str(&format!("source = {}\n", alarm.source.literal()));
        out.push_str(&format!("volume = {}\n", alarm.volume.literal()));
        out.push_str(&format!("ramp_s = {}\n", alarm.ramp_s));
        out.push_str(&format!("duration_min = {}\n", alarm.duration_min));
        out.push_str(&format!("enabled = {}\n", u8::from(alarm.enabled)));
    }
    for rule in zones.autoplay_rules() {
        out.push('\n');
        out.push_str(&format!("[autoplay {}]\n", rule.input.literal()));
        out.push_str(&format!("target = {}\n", rule.target));
        out.push_str(&format!("enabled = {}\n", u8::from(rule.enabled)));
        out.push_str(&format!(
            "stop_on_standby = {}\n",
            u8::from(rule.stop_on_standby)
        ));
        out.push_str(&format!("low_latency = {}\n", u8::from(rule.low_latency)));
    }
    for stored in zones.stored_sources() {
        out.push('\n');
        out.push_str(&format!("[stored-source {}]\n", escape(&stored.id)));
        out.push_str(&format!("kind = {}\n", stored.kind.name()));
        out.push_str(&format!("value = {}\n", escape(&stored.value)));
        out.push_str(&format!("name = {}\n", escape(&stored.name)));
    }
    for label in zones.input_labels() {
        out.push('\n');
        out.push_str(&format!("[input-label {}]\n", label.input.literal()));
        out.push_str(&format!("name = {}\n", escape(&label.name)));
        out.push_str(&format!("role = {}\n", label.role.name()));
    }
    for speaker in zones.speakers().all() {
        out.push('\n');
        out.push_str(&format!("[speaker {}]\n", escape(&speaker.id)));
        out.push_str(&format!("name = {}\n", escape(&speaker.name)));
        out.push_str(&format!("named = {}\n", u8::from(speaker.named)));
        out.push_str(&format!(
            "room = {}\n",
            escape(speaker.room.as_deref().unwrap_or(""))
        ));
    }
    out
}

/// One section of the file: its kind, its id, its fields and the line its
/// header is on.
struct Section {
    kind: String,
    id: String,
    fields: Vec<(String, String)>,
    line: usize,
}

impl Section {
    fn get(&self, key: &str) -> Result<String, StateError> {
        self.fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .ok_or(StateError::Malformed {
                line: 0,
                detail: format!("{} '{}' has no '{}'", self.kind, self.id, key),
            })
    }

    fn fail(&self, detail: String) -> StateError {
        StateError::Malformed {
            line: self.line,
            detail: format!("{} '{}': {}", self.kind, self.id, detail),
        }
    }

    fn flag(&self, key: &str) -> Result<bool, StateError> {
        match self.get(key)?.as_str() {
            "0" => Ok(false),
            "1" => Ok(true),
            other => Err(self.fail(format!("{} = '{}', which is not 0 or 1", key, other))),
        }
    }

    fn number(&self, key: &str, max: u32) -> Result<u32, StateError> {
        let text = self.get(key)?;
        match text.parse::<u32>() {
            Ok(n) if n <= max && n.to_string() == text => Ok(n),
            _ => Err(self.fail(format!("{} = '{}', which is not 0 to {}", key, text, max))),
        }
    }
}

/// The section kinds each format has.
fn kinds_of(format: u32) -> &'static [&'static str] {
    match format {
        1 => &["zone"],
        2..=4 => &["zone", "endpoint", "saved-group", "alarm", "autoplay"],
        5 => &[
            "zone",
            "endpoint",
            "saved-group",
            "alarm",
            "autoplay",
            "speaker",
        ],
        _ => &[
            "zone",
            "endpoint",
            "saved-group",
            "alarm",
            "autoplay",
            "speaker",
            "stored-source",
            "input-label",
        ],
    }
}

/// The format a section kind was added in.
fn added_in(kind: &str) -> u32 {
    match kind {
        "zone" => 1,
        "speaker" => 5,
        "stored-source" | "input-label" => 6,
        _ => 2,
    }
}

fn list(text: &str) -> Vec<String> {
    text.split(',')
        .map(|e| e.trim())
        .filter(|e| !e.is_empty())
        .map(|e| e.to_string())
        .collect()
}

/// Read the persisted form back.
///
/// Every field is required and nothing is defaulted. A state file with a
/// missing field is a state file somebody edited wrongly, and inventing the
/// missing value is how a zone comes back at a volume nobody set.
///
/// A format 1 file (what every build before catalog v2 wrote) loads
/// unchanged: it has none of format 2's fields, and every room comes back with
/// the v2 defaults (no limit below full scale, no quiet hours, no bonded set),
/// which is exactly the state that build had. The next write is format 2.
pub fn load(text: &str, default_audio: &str) -> Result<Zones, StateError> {
    let mut zones = Zones::new(default_audio);
    let mut serial: Option<u64> = None;
    let mut format: Option<u32> = None;
    let mut sections: Vec<Section> = Vec::new();

    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let body = code(raw).trim();
        if body.is_empty() {
            continue;
        }
        if let Some(rest) = body.strip_prefix('[') {
            let header = rest.strip_suffix(']').ok_or(StateError::Malformed {
                line,
                detail: "a section header is not closed with ']'".to_string(),
            })?;
            let (kind, id) = header.split_once(' ').ok_or(StateError::Malformed {
                line,
                detail: format!("'[{}]' is not a section this format has", header),
            })?;
            if !kinds_of(STATE_FORMAT).contains(&kind) {
                return Err(StateError::Malformed {
                    line,
                    detail: format!("'[{}]' is not a section this format has", header),
                });
            }
            // Unescaped like any other value. An identifier can hold neither a
            // backslash nor a hash, so this is a no-op on every valid header
            // and a named refusal on one somebody hand-edited wrongly.
            let id =
                unescape(id.trim()).map_err(|detail| StateError::Malformed { line, detail })?;
            let valid = if kind == "autoplay" || kind == "input-label" {
                InputId::parse(&id).is_some()
            } else {
                is_identifier(&id)
            };
            if !valid {
                return Err(StateError::Malformed {
                    line,
                    detail: format!("'{}' is not a {} identifier", id, kind),
                });
            }
            if sections.iter().any(|s| s.kind == kind && s.id == id) {
                return Err(StateError::Malformed {
                    line,
                    detail: format!("[{} {}] is given twice", kind, id),
                });
            }
            sections.push(Section {
                kind: kind.to_string(),
                id,
                fields: Vec::new(),
                line,
            });
            continue;
        }
        let (key, value) = body.split_once('=').ok_or(StateError::Malformed {
            line,
            detail: format!("'{}' is not 'key = value'", body),
        })?;
        let key = key.trim().to_string();
        let value =
            unescape(value.trim()).map_err(|detail| StateError::Malformed { line, detail })?;
        match sections.last_mut() {
            Some(section) => {
                if section.fields.iter().any(|(k, _)| *k == key) {
                    return Err(StateError::Malformed {
                        line,
                        detail: format!("'{}' is given twice in this {}", key, section.kind),
                    });
                }
                section.fields.push((key, value));
            }
            None => match key.as_str() {
                "format" => {
                    format = Some(value.parse().map_err(|_| StateError::Malformed {
                        line,
                        detail: format!("'{}' is not a format version", value),
                    })?)
                }
                "serial" => {
                    serial = Some(value.parse().map_err(|_| StateError::Malformed {
                        line,
                        detail: format!("'{}' is not a serial", value),
                    })?)
                }
                other => {
                    return Err(StateError::Malformed {
                        line,
                        detail: format!("'{}' is not a field of this file's header", other),
                    })
                }
            },
        }
    }

    let format = match format {
        Some(f) if READ_FORMATS.contains(&f) => f,
        Some(other) => {
            return Err(StateError::Malformed {
                line: 0,
                detail: format!(
                    "this state file declares format {} and this build writes format {} and \
                     reads formats {}",
                    other,
                    STATE_FORMAT,
                    READ_FORMATS
                        .iter()
                        .map(|f| f.to_string())
                        .collect::<Vec<_>>()
                        .join(" and ")
                ),
            })
        }
        None => {
            return Err(StateError::Malformed {
                line: 0,
                detail: "this state file declares no format version".to_string(),
            })
        }
    };
    if let Some(section) = sections
        .iter()
        .find(|s| !kinds_of(format).contains(&s.kind.as_str()))
    {
        return Err(StateError::Malformed {
            line: section.line,
            detail: format!(
                "a format {} file has no [{}] section; it was added in format {}",
                format,
                section.kind,
                added_in(&section.kind)
            ),
        });
    }

    // Rooms first, because everything else names them.
    let mut bonds: Vec<(String, Vec<BondMember>)> = Vec::new();
    for section in sections.iter().filter(|s| s.kind == "zone") {
        let zone = load_zone(section, format, &mut bonds)?;
        zones.add(zone).map_err(|e| StateError::Malformed {
            line: 0,
            detail: e.to_string(),
        })?;
    }
    for section in sections.iter().filter(|s| s.kind == "endpoint") {
        let word = section.get("link")?;
        let link =
            Link::parse(&word).ok_or_else(|| section.fail(format!("'{}' is not a link", word)))?;
        zones.set_link(&section.id, link);
    }
    for section in sections.iter().filter(|s| s.kind == "saved-group") {
        let name = section.get("name")?;
        if !is_display_name(&name) {
            return Err(section.fail(format!("'{}' is not a name", name)));
        }
        let members = list(&section.get("zones")?);
        if members.len() < 2 {
            return Err(section.fail("a saved group holds two or more rooms".to_string()));
        }
        if zones.zone(&section.id).is_some() {
            return Err(section.fail("a saved group cannot take a room's identifier".to_string()));
        }
        for member in &members {
            if zones.zone(member).is_none() {
                return Err(section.fail(format!("names a zone '{}' this file does not", member)));
            }
        }
        zones.restore_saved_group(SavedGroup {
            id: section.id.clone(),
            name,
            zones: members,
        });
    }
    let target_exists = |zones: &Zones, target: &str| {
        zones.zone(target).is_some() || zones.saved_groups().iter().any(|g| g.id == target)
    };
    // (format 6) Stored sources before the alarms that name them.
    for section in sections.iter().filter(|s| s.kind == "stored-source") {
        let word = section.get("kind")?;
        let kind = StoredKind::parse(&word)
            .ok_or_else(|| section.fail(format!("'{}' is not a kind of stored source", word)))?;
        let value = section.get("value")?;
        if let Some(problem) = StoredSource::value_problem(kind, &value) {
            return Err(section.fail(problem));
        }
        let name = section.get("name")?;
        if !is_display_name(&name) {
            return Err(section.fail(format!("'{}' is not a name", name)));
        }
        zones.restore_stored_source(StoredSource {
            id: section.id.clone(),
            kind,
            value,
            name,
        });
    }
    for section in sections.iter().filter(|s| s.kind == "input-label") {
        let name = section.get("name")?;
        if !is_display_name(&name) {
            return Err(section.fail(format!("'{}' is not a name", name)));
        }
        let word = section.get("role")?;
        let role = InputRole::parse(&word)
            .ok_or_else(|| section.fail(format!("'{}' is not an input's role", word)))?;
        zones.restore_input_label(InputLabel {
            input: InputId::parse(&section.id).expect("checked at the header"),
            name,
            role,
        });
    }
    for section in sections.iter().filter(|s| s.kind == "alarm") {
        let target = section.get("target")?;
        if !target_exists(&zones, &target) {
            return Err(section.fail(format!(
                "targets '{}', which is neither a zone nor a saved group in this file",
                target
            )));
        }
        let time = section.get("time")?;
        let days = section.get("days")?;
        let source = section.get("source")?;
        let volume = section.get("volume")?;
        let alarm = Alarm {
            id: section.id.clone(),
            target,
            time: ClockTime::parse(&time)
                .ok_or_else(|| section.fail(format!("'{}' is not HH:MM", time)))?,
            days: Days::from_names(days.split(',').map(str::trim).filter(|d| !d.is_empty()))
                .map_err(|e| section.fail(e))?,
            source: match Source::parse(&source) {
                Some(Source::Stored(id)) if zones.stored_source(&id).is_none() => {
                    return Err(section.fail(format!(
                        "plays 'stored:{}', which is not a stored source in this file",
                        id
                    )))
                }
                Some(source) => source,
                None => return Err(section.fail(format!("'{}' is not a source", source))),
            },
            volume: Volume::parse(&volume)
                .ok_or_else(|| section.fail(format!("'{}' is not a volume", volume)))?,
            ramp_s: section.number("ramp_s", MAX_RAMP_S)?,
            duration_min: section.number("duration_min", MAX_DURATION_MIN)?,
            enabled: section.flag("enabled")?,
        };
        zones.restore_alarm(alarm);
    }
    for section in sections.iter().filter(|s| s.kind == "autoplay") {
        let target = section.get("target")?;
        if !target_exists(&zones, &target) {
            return Err(section.fail(format!(
                "targets '{}', which is neither a zone nor a saved group in this file",
                target
            )));
        }
        let (stop_on_standby, low_latency) = if format >= 4 {
            (
                section.flag("stop_on_standby")?,
                section.flag("low_latency")?,
            )
        } else {
            (true, true)
        };
        zones.restore_autoplay(Autoplay {
            input: InputId::parse(&section.id).expect("checked at the header"),
            target,
            enabled: section.flag("enabled")?,
            stop_on_standby,
            low_latency,
        });
    }
    // Bonds last, through the same rule a `bond` command is held to, against
    // the links this file just gave back: a hand-edited bond holding an
    // endpoint that is not wired is refused here exactly as it would be on
    // the wire (K91).
    for (zone, members) in bonds {
        zones
            .apply(&Command::Bond {
                zone: zone.clone(),
                members,
            })
            .map_err(|e| StateError::Malformed {
                line: 0,
                detail: format!("zone '{}' has a bonded set this build refuses: {}", zone, e),
            })?;
    }
    // Speakers last of all: a speaker's room is one of this file's zones, and
    // its membership of that room is made to agree with it.
    for section in sections.iter().filter(|s| s.kind == "speaker") {
        let name = section.get("name")?;
        if !is_display_name(&name) {
            return Err(section.fail(format!("'{}' is not a name", name)));
        }
        let named = section.flag("named")?;
        let room = section.get("room")?;
        let room = if room.is_empty() {
            None
        } else if zones.zone(&room).is_some() {
            Some(room)
        } else {
            return Err(section.fail(format!(
                "is assigned a room '{}' this file does not have",
                room
            )));
        };
        let mut speaker = Speaker::new(&section.id);
        speaker.name = name;
        speaker.named = named;
        speaker.room = room;
        zones.restore_speaker(speaker);
    }
    zones.set_serial(serial.unwrap_or(0));
    Ok(zones)
}

fn load_zone(
    section: &Section,
    format: u32,
    bonds: &mut Vec<(String, Vec<BondMember>)>,
) -> Result<Zone, StateError> {
    let id = &section.id;
    let name = section.get("name")?;
    let group = section.get("group")?;
    let volume = section.get("volume")?;
    let muted = section.get("muted")?;
    let endpoints = section.get("endpoints")?;
    if !is_display_name(&name) {
        return Err(StateError::Malformed {
            line: 0,
            detail: format!("zone '{}' has a name that is not a name: '{}'", id, name),
        });
    }
    if !is_identifier(&group) {
        return Err(StateError::Malformed {
            line: 0,
            detail: format!(
                "zone '{}' is in a group that is not an identifier: '{}'",
                id, group
            ),
        });
    }
    let volume = Volume::parse(&volume).ok_or(StateError::Malformed {
        line: 0,
        detail: format!(
            "zone '{}' has a volume of '{}', outside the declared 0.000 to 1.000",
            id, volume
        ),
    })?;
    let muted = match muted.as_str() {
        "0" => false,
        "1" => true,
        other => {
            return Err(StateError::Malformed {
                line: 0,
                detail: format!("zone '{}' has muted = '{}', which is not 0 or 1", id, other),
            })
        }
    };
    let endpoints = list(&endpoints);
    for endpoint in &endpoints {
        if !is_identifier(endpoint) {
            return Err(StateError::Malformed {
                line: 0,
                detail: format!(
                    "zone '{}' names an endpoint '{}' that is not an identifier",
                    id, endpoint
                ),
            });
        }
    }
    let mut zone = Zone::new(id);
    zone.name = name;
    zone.group = group;
    zone.volume = volume;
    zone.muted = muted;
    zone.endpoints = endpoints;
    // Nothing is present at load: which endpoints are switched on is a fact
    // about now and is never read out of a file.
    zone.present = Vec::new();
    if format >= 2 {
        let limit = section.get("limit")?;
        zone.limit = Volume::parse(&limit)
            .ok_or_else(|| section.fail(format!("'{}' is not a limit", limit)))?;
        let quiet = section.get("quiet")?;
        for window in quiet.split(';').map(str::trim).filter(|w| !w.is_empty()) {
            zone.quiet
                .push(QuietWindow::from_persisted(window).map_err(|e| section.fail(e))?);
        }
        if zone.quiet.len() > MAX_QUIET_WINDOWS {
            return Err(section.fail(format!(
                "has more than {} quiet-hours windows",
                MAX_QUIET_WINDOWS
            )));
        }
        let mut members = Vec::new();
        for item in list(&section.get("bond")?) {
            let (role, endpoint) = item
                .split_once(':')
                .ok_or_else(|| section.fail(format!("'{}' is not 'ROLE:endpoint'", item)))?;
            let role = Role::parse(role)
                .ok_or_else(|| section.fail(format!("'{}' is not a bond role", role)))?;
            members.push(BondMember {
                endpoint: endpoint.to_string(),
                role,
            });
        }
        if !members.is_empty() {
            // Held to the decoder's own rules on the members' shape (one role
            // each, a valid layout), by building the command it would accept.
            let roles: Vec<Role> = members.iter().map(|m| m.role).collect();
            if let Some(problem) = validate_layout(&roles) {
                return Err(section.fail(problem));
            }
            let mut seen = roles.clone();
            seen.sort();
            seen.dedup();
            if seen.len() != roles.len() {
                return Err(section.fail("plays one role twice in its bonded set".to_string()));
            }
            bonds.push((id.clone(), members));
        }
    }
    if format >= 3 {
        load_sound(section, &mut zone)?;
    }
    if format >= 4 {
        load_theater(section, &mut zone)?;
    }
    if format >= 7 {
        zone.quiet_enabled = section.flag("quiet_enabled")?;
    }
    if format >= 8 {
        zone.voice_enabled = section.flag("voice_enabled")?;
    }
    if format >= 9 {
        let text = section.get("wake_words")?;
        if text != "*" {
            let chosen = list(&text);
            let mut seen = chosen.clone();
            seen.sort();
            seen.dedup();
            if chosen.len() > MAX_WAKE_WORDS
                || seen.len() != chosen.len()
                || !chosen.iter().all(|id| is_wake_word_id(id))
            {
                return Err(section.fail(format!(
                    "wake_words = '{}', which is not * or at most {} different wake-word ids",
                    text, MAX_WAKE_WORDS
                )));
            }
            zone.wake_words = Some(chosen);
        }
    }
    if format >= 10 {
        load_room_eq_undo(section, &mut zone)?;
    }
    Ok(zone)
}

/// Format 3's sound fields, each required and held to the catalog's range.
fn load_sound(section: &Section, zone: &mut Zone) -> Result<(), StateError> {
    let ranged = |key: &str, min: i64, max: i64| -> Result<i64, StateError> {
        let text = section.get(key)?;
        match text.parse::<i64>() {
            Ok(n) if (min..=max).contains(&n) && n.to_string() == text => Ok(n),
            _ => Err(section.fail(format!(
                "{} = '{}', which is not a whole number from {} to {}",
                key, text, min, max
            ))),
        }
    };
    zone.sound.bass = ranged("bass", i64::from(TONE_DB.0), i64::from(TONE_DB.1))? as i8;
    zone.sound.treble = ranged("treble", i64::from(TONE_DB.0), i64::from(TONE_DB.1))? as i8;
    zone.sound.loudness = section.flag("loudness")?;
    zone.sound.night = section.flag("night")?;
    zone.sound.speech = section.flag("speech")?;
    zone.bass.crossover_hz = ranged(
        "crossover_hz",
        i64::from(CROSSOVER_HZ.0),
        i64::from(CROSSOVER_HZ.1),
    )? as u16;
    let level = section.get("sub_level_db")?;
    zone.bass.sub_level_cdb = FixedPoint::parse(&level, 2)
        .filter(|n| (i64::from(SUB_LEVEL_CDB.0)..=i64::from(SUB_LEVEL_CDB.1)).contains(n))
        .ok_or_else(|| {
            section.fail(format!(
                "sub_level_db = '{}', which is not -12.00 to 6.00",
                level
            ))
        })? as i16;
    let polarity = section.get("sub_polarity")?;
    zone.bass.sub_polarity = Polarity::parse(&polarity)
        .ok_or_else(|| section.fail(format!("'{}' is not a polarity", polarity)))?;
    zone.room_eq.enabled = section.flag("room_eq")?;
    let filters = section.get("room_eq_filters")?;
    for filter in filters.split(';').map(str::trim).filter(|f| !f.is_empty()) {
        zone.room_eq
            .filters
            .push(EqFilter::from_persisted(filter).map_err(|e| section.fail(e))?);
    }
    if zone.room_eq.filters.len() > ROOM_EQ_MAX_FILTERS {
        return Err(section.fail(format!(
            "has more than {} correction filters",
            ROOM_EQ_MAX_FILTERS
        )));
    }
    Ok(())
}

/// Format 10's undo fields, each required: the correction `room_eq_undo`
/// puts back, held to what a `room_eq` is held to.
fn load_room_eq_undo(section: &Section, zone: &mut Zone) -> Result<(), StateError> {
    let kept = section.get("room_eq_undo")?;
    let filters = section.get("room_eq_undo_filters")?;
    let enabled = match kept.as_str() {
        "none" => {
            if !filters.trim().is_empty() {
                return Err(section.fail(
                    "room_eq_undo = none, and room_eq_undo_filters is not empty".to_string(),
                ));
            }
            return Ok(());
        }
        "0" => false,
        "1" => true,
        other => {
            return Err(section.fail(format!(
                "room_eq_undo = '{}', which is not none, 0 or 1",
                other
            )))
        }
    };
    let mut before = RoomEq {
        enabled,
        filters: Vec::new(),
    };
    for filter in filters.split(';').map(str::trim).filter(|f| !f.is_empty()) {
        before
            .filters
            .push(EqFilter::from_persisted(filter).map_err(|e| section.fail(e))?);
    }
    if before.filters.len() > ROOM_EQ_MAX_FILTERS {
        return Err(section.fail(format!(
            "has more than {} correction filters to go back to",
            ROOM_EQ_MAX_FILTERS
        )));
    }
    zone.room_eq_undo = Some(before);
    Ok(())
}

/// Format 4's TV fields, each required and held to the catalog's range.
fn load_theater(section: &Section, zone: &mut Zone) -> Result<(), StateError> {
    let upmix = section.get("tv_upmix")?;
    zone.sound.tv_upmix = TvUpmix::parse(&upmix)
        .ok_or_else(|| section.fail(format!("'{}' is not a TV upmix", upmix)))?;
    let trim = section.get("av_trim_ms")?;
    zone.av_trim_ms = match trim.parse::<i16>() {
        Ok(n) if (AV_TRIM_MS.0..=AV_TRIM_MS.1).contains(&n) && n.to_string() == trim => n,
        _ => {
            return Err(section.fail(format!(
                "av_trim_ms = '{}', which is not a whole number from {} to {}",
                trim, AV_TRIM_MS.0, AV_TRIM_MS.1
            )))
        }
    };
    Ok(())
}

/// Read the state file at `path`, or `None` where there is none yet.
pub fn read_file(path: &Path, default_audio: &str) -> Result<Option<Zones>, StateError> {
    match std::fs::read_to_string(path) {
        Ok(text) => load(&text, default_audio).map(Some),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(StateError::Io(e)),
    }
}

/// Write the state file at `path`, atomically.
///
/// The rendered text is read back through this module's own loader before it
/// is installed, and a render that does not survive its own loader is refused
/// rather than written. That is the invariant the restart criterion rests on -
/// a file this build wrote is a file this build reads back as the same state -
/// and checking it here means a future field carrying text nobody escaped is a
/// loud refusal on the machine that wrote it, not a zone that quietly comes
/// back under a different name after the next restart.
pub fn write_file(path: &Path, zones: &Zones) -> Result<(), StateError> {
    let text = render(zones);
    check_round_trip(&text)?;
    let mut temporary = PathBuf::from(path);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "zone-state".to_string());
    temporary.set_file_name(format!(".{}.writing", name));
    {
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(text.as_bytes())?;
        // B-9: the data is on the disk before any name points at it.
        file.sync_all()?;
    }
    std::fs::rename(&temporary, path)?;
    // B-9: and the rename is on the disk too. A directory is opened read-only
    // to be synced; on a platform that cannot, the rename has still happened.
    let directory = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    std::fs::File::open(&directory)?.sync_all()?;
    Ok(())
}

/// Refuse a rendered state that this module's own loader does not give back
/// unchanged.
///
/// Rendering is the only thing compared, because rendering is total over the
/// persisted state: two states that render identically restore identically.
fn check_round_trip(text: &str) -> Result<(), StateError> {
    // The default audio address is not persisted and does not appear in a
    // render, so any placeholder answers here.
    let back = load(text, "0.0.0.0:0").map_err(|e| StateError::Malformed {
        line: 0,
        detail: format!(
            "this build rendered a state file it cannot read back ({}), so it was not written",
            e
        ),
    })?;
    let again = render(&back);
    if again != text {
        return Err(StateError::Malformed {
            line: 0,
            detail: "this build rendered a state file that reads back as a different state, so it \
                     was not written"
                .to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Command;

    fn a_state() -> Zones {
        let mut zones = Zones::new("127.0.0.1:4010");
        zones.add(Zone::new("kitchen")).unwrap();
        zones.add(Zone::new("study")).unwrap();
        zones
            .apply(&Command::Name {
                zone: "kitchen".to_string(),
                name: "Kitchen".to_string(),
            })
            .unwrap();
        zones
            .apply(&Command::Group {
                zone: "kitchen".to_string(),
                group: "downstairs".to_string(),
            })
            .unwrap();
        zones
            .apply(&Command::Volume {
                zone: "kitchen".to_string(),
                volume: Volume::from_thousandths(375).unwrap(),
            })
            .unwrap();
        zones
            .apply(&Command::Mute {
                zone: "study".to_string(),
                muted: true,
            })
            .unwrap();
        zones
            .apply(&Command::Attach {
                zone: "kitchen".to_string(),
                endpoint: "endpoint-a".to_string(),
                link: None,
            })
            .unwrap();
        zones
    }

    #[test]
    fn what_is_written_is_what_comes_back() {
        let zones = a_state();
        let text = render(&zones);
        let back = load(&text, "127.0.0.1:4010").expect("it reads back");
        assert_eq!(back.serial(), zones.serial());
        for zone in zones.zones() {
            let loaded = back.zone(&zone.id).expect("every zone comes back");
            assert_eq!(loaded.name, zone.name);
            assert_eq!(loaded.group, zone.group);
            assert_eq!(loaded.volume, zone.volume);
            assert_eq!(loaded.muted, zone.muted);
            assert_eq!(loaded.endpoints, zone.endpoints);
            assert!(loaded.present.is_empty(), "presence is never persisted");
        }
        assert_eq!(
            render(&back),
            text,
            "and rendering it again is the same file"
        );
    }

    #[test]
    fn a_state_file_this_build_does_not_understand_is_refused_rather_than_guessed() {
        // Format 10 is this build's own; the next one is not.
        let err = load("format = 11\nserial = 1\n", "x").unwrap_err();
        assert!(err.to_string().contains("declares format 11"), "{}", err);
        let err = load("serial = 1\n", "x").unwrap_err();
        assert!(err.to_string().contains("no format version"), "{}", err);
    }

    #[test]
    fn a_missing_field_is_a_refusal_and_not_a_default() {
        let text = "format = 1\nserial = 3\n\n[zone kitchen]\nname = Kitchen\ngroup = kitchen\n";
        let err = load(text, "x").unwrap_err();
        assert!(err.to_string().contains("has no 'volume'"), "{}", err);
    }

    /// Names chosen to sit on the format's own punctuation, not on what a
    /// tidy demonstration would set. Every one of them is a name
    /// `is_display_name` accepts, so every one of them can reach the state
    /// file through the shipped UI's rename box.
    const AWKWARD_NAMES: &[&str] = &[
        "Kitchen #1",
        "#1",
        "#",
        "##",
        "Back\\Room",
        "\\",
        "C:\\Music\\#3",
        "a = b",
        "[zone kitchen]",
        "name = not a key",
        "inner   spaces   kept",
        "Küche #2 - Ober\u{00e4}tage",
        "\u{1f50a} #main",
        "]",
        "trailing hash #",
        "trailing slash \\",
        "format = 2",
    ];

    #[test]
    fn every_name_the_catalog_accepts_survives_the_state_file() {
        for name in AWKWARD_NAMES {
            assert!(
                is_display_name(name),
                "the test's own corpus must only hold names the catalog accepts: {:?}",
                name
            );
            let mut zones = Zones::new("127.0.0.1:4010");
            zones.add(Zone::new("kitchen")).unwrap();
            zones
                .apply(&Command::Name {
                    zone: "kitchen".to_string(),
                    name: (*name).to_string(),
                })
                .unwrap();
            let text = render(&zones);
            let back = load(&text, "127.0.0.1:4010").unwrap_or_else(|e| {
                panic!("{:?} made an unreadable state file: {}\n{}", name, e, text)
            });
            assert_eq!(
                back.zones()[0].name,
                *name,
                "{:?} did not come back from the state file:\n{}",
                name,
                text
            );
            assert_eq!(render(&back), text, "and it renders back to the same file");
        }
    }

    #[test]
    fn a_state_file_that_would_not_read_back_is_never_written() {
        // A name with a newline in it is one the catalog refuses, so it can
        // only reach here by construction - which is the case this guard is
        // for. The file on disk must be left alone.
        let directory = std::env::temp_dir().join(format!("chorus-persist-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("zones.state");

        let mut good = Zones::new("127.0.0.1:4010");
        good.add(Zone::new("kitchen")).unwrap();
        good.apply(&Command::Name {
            zone: "kitchen".to_string(),
            name: "Kitchen #1".to_string(),
        })
        .unwrap();
        write_file(&path, &good).expect("a name with a hash in it writes");
        let on_disk = std::fs::read_to_string(&path).unwrap();

        let mut broken = good.clone();
        broken
            .apply(&Command::Name {
                zone: "kitchen".to_string(),
                // `Zones::add` and the decoder both refuse this, so it can only
                // reach a render by construction - which is the case this guard
                // exists for.
                name: "Kitchen\nStudy".to_string(),
            })
            .unwrap();
        let err =
            write_file(&path, &broken).expect_err("a state that cannot be read back is refused");
        assert!(
            err.to_string().contains("cannot read back")
                || err.to_string().contains("different state"),
            "{}",
            err
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            on_disk,
            "and the readable file that was already there is untouched"
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_hash_still_starts_a_comment_and_an_escape_this_format_lacks_is_refused() {
        let text = "format = 1\nserial = 3\n\n[zone kitchen]\nname = Kitchen # the good one\n\
                    group = kitchen\nvolume = 1.000\nmuted = 0\nendpoints =\n";
        let zones = load(text, "x").expect("an unescaped hash is still a comment");
        assert_eq!(zones.zones()[0].name, "Kitchen");

        let text = "format = 1\nserial = 3\n\n[zone kitchen]\nname = Back\\Room\n\
                    group = kitchen\nvolume = 1.000\nmuted = 0\nendpoints =\n";
        let err = load(text, "x").unwrap_err();
        assert!(
            err.to_string().contains("is not an escape this format has"),
            "{}",
            err
        );

        let text = "format = 1\nserial = 3\n\n[zone kitchen]\nname = Back\\\n\
                    group = kitchen\nvolume = 1.000\nmuted = 0\nendpoints =\n";
        let err = load(text, "x").unwrap_err();
        assert!(err.to_string().contains("lone '\\'"), "{}", err);
    }

    #[test]
    fn a_volume_outside_the_range_does_not_come_back_as_something_else() {
        let text = "format = 1\nserial = 3\n\n[zone kitchen]\nname = K\ngroup = kitchen\n\
                    volume = 9.000\nmuted = 0\nendpoints =\n";
        let err = load(text, "x").unwrap_err();
        assert!(err.to_string().contains("outside the declared"), "{}", err);
    }
}
