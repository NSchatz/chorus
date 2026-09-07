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

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use crate::catalog::{is_display_name, is_identifier, Volume};
use crate::zones::{Zone, Zones};

/// The version of this file format.
pub const STATE_FORMAT: u32 = 1;

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
/// reviewable in a diff and a test can assert on it.
pub fn render(zones: &Zones) -> String {
    let mut out = String::new();
    out.push_str("# chorus zone state, written by chorus-server.\n");
    out.push_str("#\n");
    out.push_str("# Format: key = value, one [zone <id>] section per zone. An unescaped '#'\n");
    out.push_str("# starts a comment; a value writes a hash as '\\#' and a backslash as '\\\\',\n");
    out.push_str("# and has no other escape, so any name the catalog accepts comes back.\n");
    out.push_str("# docs/decisions/0018-the-persisted-zone-state.md says what is here and what\n");
    out.push_str("# deliberately is not. Editing this file by hand is supported; the server reads\n");
    out.push_str("# it once at start and refuses to start on a file it cannot parse.\n");
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
    }
    out
}

/// Read the persisted form back.
///
/// Every field is required and nothing is defaulted. A state file with a
/// missing field is a state file somebody edited wrongly, and inventing the
/// missing value is how a zone comes back at a volume nobody set.
pub fn load(text: &str, default_audio: &str) -> Result<Zones, StateError> {
    let mut zones = Zones::new(default_audio);
    let mut serial: Option<u64> = None;
    let mut format: Option<u32> = None;
    let mut current: Option<(String, Vec<(String, String)>)> = None;
    let mut pending: Vec<(String, Vec<(String, String)>)> = Vec::new();

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
            let id = header
                .strip_prefix("zone ")
                .ok_or(StateError::Malformed {
                    line,
                    detail: format!("'[{}]' is not a section this format has", header),
                })?
                .trim();
            // Unescaped like any other value. An identifier can hold neither a
            // backslash nor a hash, so this is a no-op on every valid header
            // and a named refusal on one somebody hand-edited wrongly.
            let id = unescape(id).map_err(|detail| StateError::Malformed { line, detail })?;
            if !is_identifier(&id) {
                return Err(StateError::Malformed {
                    line,
                    detail: format!("'{}' is not a zone identifier", id),
                });
            }
            if let Some(section) = current.take() {
                pending.push(section);
            }
            current = Some((id, Vec::new()));
            continue;
        }
        let (key, value) = body.split_once('=').ok_or(StateError::Malformed {
            line,
            detail: format!("'{}' is not 'key = value'", body),
        })?;
        let key = key.trim().to_string();
        let value = unescape(value.trim()).map_err(|detail| StateError::Malformed { line, detail })?;
        match current.as_mut() {
            Some((_, fields)) => {
                if fields.iter().any(|(k, _)| *k == key) {
                    return Err(StateError::Malformed {
                        line,
                        detail: format!("'{}' is given twice in this zone", key),
                    });
                }
                fields.push((key, value));
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
    if let Some(section) = current.take() {
        pending.push(section);
    }

    match format {
        Some(STATE_FORMAT) => {}
        Some(other) => {
            return Err(StateError::Malformed {
                line: 0,
                detail: format!(
                    "this state file declares format {} and this build writes and reads format {}",
                    other, STATE_FORMAT
                ),
            })
        }
        None => {
            return Err(StateError::Malformed {
                line: 0,
                detail: "this state file declares no format version".to_string(),
            })
        }
    }

    for (id, fields) in pending {
        let get = |key: &str| -> Result<String, StateError> {
            fields
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
                .ok_or(StateError::Malformed {
                    line: 0,
                    detail: format!("zone '{}' has no '{}'", id, key),
                })
        };
        let name = get("name")?;
        let group = get("group")?;
        let volume = get("volume")?;
        let muted = get("muted")?;
        let endpoints = get("endpoints")?;
        if !is_display_name(&name) {
            return Err(StateError::Malformed {
                line: 0,
                detail: format!("zone '{}' has a name that is not a name: '{}'", id, name),
            });
        }
        if !is_identifier(&group) {
            return Err(StateError::Malformed {
                line: 0,
                detail: format!("zone '{}' is in a group that is not an identifier: '{}'", id, group),
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
        let endpoints: Vec<String> = endpoints
            .split(',')
            .map(|e| e.trim())
            .filter(|e| !e.is_empty())
            .map(|e| e.to_string())
            .collect();
        for endpoint in &endpoints {
            if !is_identifier(endpoint) {
                return Err(StateError::Malformed {
                    line: 0,
                    detail: format!("zone '{}' names an endpoint '{}' that is not an identifier", id, endpoint),
                });
            }
        }
        zones
            .add(Zone {
                id: id.clone(),
                name,
                group,
                volume,
                muted,
                endpoints,
                // Nothing is present at load: which endpoints are switched on
                // is a fact about now and is never read out of a file.
                present: Vec::new(),
            })
            .map_err(|e| StateError::Malformed {
                line: 0,
                detail: e.to_string(),
            })?;
    }
    zones.set_serial(serial.unwrap_or(0));
    Ok(zones)
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
    std::fs::write(&temporary, &text)?;
    std::fs::rename(&temporary, path)?;
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
        assert_eq!(render(&back), text, "and rendering it again is the same file");
    }

    #[test]
    fn a_state_file_this_build_does_not_understand_is_refused_rather_than_guessed() {
        let err = load("format = 2\nserial = 1\n", "x").unwrap_err();
        assert!(err.to_string().contains("declares format 2"), "{}", err);
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
            let back = load(&text, "127.0.0.1:4010")
                .unwrap_or_else(|e| panic!("{:?} made an unreadable state file: {}\n{}", name, e, text));
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
        let err = write_file(&path, &broken).expect_err("a state that cannot be read back is refused");
        assert!(
            err.to_string().contains("cannot read back") || err.to_string().contains("different state"),
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
        assert!(err.to_string().contains("is not an escape this format has"), "{}", err);

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
