//! The versioned control message catalog: what a command is, what the state
//! message is, and what a decoder does with a message it cannot accept.
//!
//! `docs/control-plane.md` is the normative statement of everything in this
//! module, and `fixtures/control/` pins it byte for byte. Where this module and
//! that document disagree, the document is right and this is the defect: a
//! second implementation is graded against the vectors and the prose, never
//! against this code.
//!
//! # The version is on every message, and it is checked first
//!
//! Every message carries `v`, the catalog version. A peer that announces a
//! version this build does not implement is refused the session, told which
//! version it offered and which this build has, and nothing it sent is applied
//! - including the rest of the message the version arrived on. The audio wire
//! took the opposite decision for an unassigned type byte, and
//! `docs/control-plane.md` records why the control catalog cannot: an audio
//! frame a decoder skips costs 20 ms of one stream, and a control message a
//! decoder half-understands changes what a house is doing.
//!
//! # Volume is a decimal, not a float
//!
//! [`Volume`] holds thousandths of full scale as an integer and prints with
//! exactly three fractional digits. Nothing in the catalog, the vectors or the
//! persisted state ever holds a binary floating-point value, so there is no
//! spelling of `0.1` to disagree about between two languages.

use std::fmt;

use crate::json::{self, Value};

/// The catalog version this build speaks.
pub const CATALOG_VERSION: i64 = 1;

/// Every catalog version this build implements.
pub const IMPLEMENTED_VERSIONS: &[i64] = &[1];

/// Full scale, in the thousandths [`Volume`] counts.
pub const VOLUME_SCALE: u32 = 1_000;

/// Longest a zone or group identifier may be.
pub const MAX_IDENTIFIER_LEN: usize = 32;

/// Longest a human-set zone name may be, in characters.
pub const MAX_NAME_LEN: usize = 64;

/// A zone's volume: thousandths of full scale, 0 to 1000 inclusive.
///
/// The wire value is the AMPLITUDE FACTOR, not a position on a perceptual
/// curve: 0.500 means every sample is multiplied by one half.
/// `docs/decisions/0016-the-control-catalog.md` records why the curve is where
/// it is and not here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Volume(u32);

impl Volume {
    /// Silence.
    pub const SILENT: Volume = Volume(0);
    /// Full scale, which is the shipped default.
    pub const FULL: Volume = Volume(VOLUME_SCALE);

    /// A volume from thousandths of full scale, refusing anything outside the
    /// declared range.
    pub fn from_thousandths(v: i64) -> Option<Volume> {
        if (0..=i64::from(VOLUME_SCALE)).contains(&v) {
            Some(Volume(v as u32))
        } else {
            None
        }
    }

    /// Thousandths of full scale.
    pub fn thousandths(self) -> u32 {
        self.0
    }

    /// The amplitude factor, as a ratio of two integers, so that applying it
    /// needs no floating point at all.
    pub fn factor(self) -> (u32, u32) {
        (self.0, VOLUME_SCALE)
    }

    /// The one spelling the catalog uses: three fractional digits, always.
    pub fn literal(self) -> String {
        format!("{}.{:03}", self.0 / VOLUME_SCALE, self.0 % VOLUME_SCALE)
    }

    /// Read the catalog's spelling back.
    ///
    /// Accepts exactly what the catalog declares: an optional integer part, a
    /// decimal point and up to three digits, or a bare integer. It does not
    /// accept an exponent, and it does not round: `0.5001` is refused rather
    /// than silently becoming `0.500`, because a value that came back different
    /// from the value that went in is the thing a golden vector exists to catch.
    pub fn parse(digits: &str) -> Option<Volume> {
        let (whole, fraction) = match digits.split_once('.') {
            Some((w, f)) => (w, f),
            None => (digits, ""),
        };
        if whole.is_empty() || !whole.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        if fraction.len() > 3 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let whole: u32 = whole.parse().ok()?;
        let mut padded = fraction.to_string();
        while padded.len() < 3 {
            padded.push('0');
        }
        let fraction: u32 = if padded.is_empty() { 0 } else { padded.parse().ok()? };
        let total = whole.checked_mul(VOLUME_SCALE)?.checked_add(fraction)?;
        Volume::from_thousandths(i64::from(total))
    }
}

impl fmt::Display for Volume {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.literal())
    }
}

/// What a peer asked the server to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Announce a catalog version and open the session.
    Hello,
    /// Say that an endpoint is playing this zone.
    Attach {
        /// The zone.
        zone: String,
        /// The endpoint's identifier.
        endpoint: String,
    },
    /// Give a zone a human-set name.
    Name {
        /// The zone.
        zone: String,
        /// The name to set.
        name: String,
    },
    /// Put a zone into a group, so it plays that group's stream.
    Group {
        /// The zone.
        zone: String,
        /// The group to join.
        group: String,
    },
    /// Take a zone out of whatever group it is in.
    ///
    /// A zone is never in no group at all: ungrouping puts it into a group of
    /// its own, named for the zone. `docs/control-plane.md` says why there is
    /// no "grouped with nothing" state to represent.
    Ungroup {
        /// The zone.
        zone: String,
    },
    /// Set a zone's volume.
    Volume {
        /// The zone.
        zone: String,
        /// The amplitude factor to apply.
        volume: Volume,
    },
    /// Mute or unmute a zone.
    Mute {
        /// The zone.
        zone: String,
        /// Whether it is muted.
        muted: bool,
    },
}

impl Command {
    /// The catalog's name for this command.
    pub fn type_name(&self) -> &'static str {
        match self {
            Command::Hello => "hello",
            Command::Attach { .. } => "attach",
            Command::Name { .. } => "name",
            Command::Group { .. } => "group",
            Command::Ungroup { .. } => "ungroup",
            Command::Volume { .. } => "volume",
            Command::Mute { .. } => "mute",
        }
    }

    /// The zone this command is about, if it is about one.
    pub fn zone(&self) -> Option<&str> {
        match self {
            Command::Hello => None,
            Command::Attach { zone, .. }
            | Command::Name { zone, .. }
            | Command::Group { zone, .. }
            | Command::Ungroup { zone }
            | Command::Volume { zone, .. }
            | Command::Mute { zone, .. } => Some(zone),
        }
    }

    /// This command as the object the catalog declares, in the declared field
    /// order.
    pub fn value(&self) -> Value {
        let mut members = vec![
            ("v".to_string(), Value::int(CATALOG_VERSION)),
            ("t".to_string(), Value::text(self.type_name())),
        ];
        match self {
            Command::Hello => {}
            Command::Attach { zone, endpoint } => {
                members.push(("zone".to_string(), Value::text(zone)));
                members.push(("endpoint".to_string(), Value::text(endpoint)));
            }
            Command::Name { zone, name } => {
                members.push(("zone".to_string(), Value::text(zone)));
                members.push(("name".to_string(), Value::text(name)));
            }
            Command::Group { zone, group } => {
                members.push(("zone".to_string(), Value::text(zone)));
                members.push(("group".to_string(), Value::text(group)));
            }
            Command::Ungroup { zone } => {
                members.push(("zone".to_string(), Value::text(zone)));
            }
            Command::Volume { zone, volume } => {
                members.push(("zone".to_string(), Value::text(zone)));
                members.push(("volume".to_string(), Value::Num(volume.literal())));
            }
            Command::Mute { zone, muted } => {
                members.push(("zone".to_string(), Value::text(zone)));
                members.push(("muted".to_string(), Value::Bool(*muted)));
            }
        }
        Value::Obj(members)
    }

    /// The bytes this command is on the wire.
    pub fn encode(&self) -> String {
        json::write(&self.value())
    }
}

/// Why a message was not applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// The field the refusal is about. `""` where the whole message is.
    pub field: String,
    /// What was wrong, in words a person can act on.
    pub detail: String,
    /// Whether this refusal ends the session.
    pub kind: RefusalKind,
}

/// The kind of refusal, which is what decides whether the session survives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefusalKind {
    /// The message was not well-formed JSON, or not the shape the catalog
    /// declares. One message is refused; the session stays open.
    Malformed,
    /// A value was outside a declared range, or named something that does not
    /// exist. One message is refused; the session stays open.
    Rejected,
    /// The peer announced a catalog version this build does not implement.
    /// The session is refused, and nothing from the peer is applied.
    UnknownVersion {
        /// What the peer offered, where it offered a number at all.
        offered: Option<i64>,
    },
}

impl Refusal {
    /// A refusal of one message that leaves the session open.
    pub fn rejected(field: &str, detail: String) -> Refusal {
        Refusal {
            field: field.to_string(),
            detail,
            kind: RefusalKind::Rejected,
        }
    }

    /// A refusal because the text was not a message at all.
    pub fn malformed(field: &str, detail: String) -> Refusal {
        Refusal {
            field: field.to_string(),
            detail,
            kind: RefusalKind::Malformed,
        }
    }

    /// Whether this refusal ends the session.
    pub fn ends_the_session(&self) -> bool {
        matches!(self.kind, RefusalKind::UnknownVersion { .. })
    }

    /// The message the server sends back, in the declared field order.
    pub fn value(&self) -> Value {
        match &self.kind {
            RefusalKind::UnknownVersion { offered } => Value::Obj(vec![
                ("v".to_string(), Value::int(CATALOG_VERSION)),
                ("t".to_string(), Value::text("refused")),
                ("field".to_string(), Value::text(&self.field)),
                ("detail".to_string(), Value::text(&self.detail)),
                (
                    "offered".to_string(),
                    match offered {
                        Some(v) => Value::int(*v),
                        None => Value::Null,
                    },
                ),
                (
                    "implemented".to_string(),
                    Value::Arr(IMPLEMENTED_VERSIONS.iter().map(|v| Value::int(*v)).collect()),
                ),
            ]),
            _ => Value::Obj(vec![
                ("v".to_string(), Value::int(CATALOG_VERSION)),
                ("t".to_string(), Value::text("error")),
                ("field".to_string(), Value::text(&self.field)),
                ("detail".to_string(), Value::text(&self.detail)),
            ]),
        }
    }

    /// The bytes this refusal is on the wire.
    pub fn encode(&self) -> String {
        json::write(&self.value())
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.field.is_empty() {
            write!(f, "{}", self.detail)
        } else {
            write!(f, "field '{}': {}", self.field, self.detail)
        }
    }
}

impl std::error::Error for Refusal {}

/// Read one command off the wire.
///
/// In this order, and the order is the contract:
///
/// 1. the text is JSON this reader accepts, or the message is malformed;
/// 2. `v` is present, is a whole number, and is a version this build
///    implements, or the SESSION is refused;
/// 3. `t` names a command in this catalog;
/// 4. the fields that command declares are present, of the declared type and
///    inside the declared range;
/// 5. no field is present that the command does not declare.
///
/// Step 5 is deliberate and it is the opposite of what the audio wire does with
/// a longer-than-expected payload. See `docs/control-plane.md`.
pub fn decode_command(text: &str) -> Result<Command, Refusal> {
    let value = json::parse(text).map_err(|e| {
        Refusal::malformed(
            "",
            format!("the message is not well-formed JSON: {}", e),
        )
    })?;
    let members = match &value {
        Value::Obj(members) => members,
        other => {
            return Err(Refusal::malformed(
                "",
                format!("a control message is an object and this is {}", other.kind()),
            ))
        }
    };

    let version = match value.get("v") {
        None => {
            return Err(Refusal {
                field: "v".to_string(),
                detail: format!(
                    "the message carries no catalog version; this build implements {}",
                    version_list()
                ),
                kind: RefusalKind::UnknownVersion { offered: None },
            })
        }
        Some(v) => v,
    };
    let offered = version
        .as_num()
        .and_then(|digits| digits.parse::<i64>().ok());
    match offered {
        Some(v) if IMPLEMENTED_VERSIONS.contains(&v) => {}
        offered => {
            return Err(Refusal {
                field: "v".to_string(),
                detail: format!(
                    "catalog version {} was offered and this build implements {}; nothing from \
                     this peer has been applied",
                    match offered {
                        Some(v) => v.to_string(),
                        None => format!("'{}'", json::write(version)),
                    },
                    version_list()
                ),
                kind: RefusalKind::UnknownVersion { offered },
            })
        }
    }

    let type_name = match value.get("t").and_then(Value::as_str) {
        Some(t) => t.to_string(),
        None => {
            return Err(Refusal::rejected(
                "t",
                "the message carries no type, or its type is not a string".to_string(),
            ))
        }
    };

    let command = match type_name.as_str() {
        "hello" => {
            expect_fields(members, &["v", "t"])?;
            Command::Hello
        }
        "attach" => {
            expect_fields(members, &["v", "t", "zone", "endpoint"])?;
            Command::Attach {
                zone: identifier(&value, "zone")?,
                endpoint: identifier(&value, "endpoint")?,
            }
        }
        "name" => {
            expect_fields(members, &["v", "t", "zone", "name"])?;
            Command::Name {
                zone: identifier(&value, "zone")?,
                name: display_name(&value, "name")?,
            }
        }
        "group" => {
            expect_fields(members, &["v", "t", "zone", "group"])?;
            Command::Group {
                zone: identifier(&value, "zone")?,
                group: identifier(&value, "group")?,
            }
        }
        "ungroup" => {
            expect_fields(members, &["v", "t", "zone"])?;
            Command::Ungroup {
                zone: identifier(&value, "zone")?,
            }
        }
        "volume" => {
            expect_fields(members, &["v", "t", "zone", "volume"])?;
            let zone = identifier(&value, "zone")?;
            let digits = match value.get("volume") {
                Some(Value::Num(digits)) => digits.clone(),
                Some(other) => {
                    return Err(Refusal::rejected(
                        "volume",
                        format!(
                            "the volume is {} and the catalog declares a number from 0.000 to \
                             1.000",
                            other.kind()
                        ),
                    ))
                }
                None => unreachable!("expect_fields required it"),
            };
            let volume = Volume::parse(&digits).ok_or_else(|| {
                Refusal::rejected(
                    "volume",
                    format!(
                        "the volume {} is outside the range the catalog declares, which is 0.000 \
                         to 1.000 in steps of 0.001",
                        digits
                    ),
                )
            })?;
            Command::Volume { zone, volume }
        }
        "mute" => {
            expect_fields(members, &["v", "t", "zone", "muted"])?;
            let zone = identifier(&value, "zone")?;
            let muted = match value.get("muted").and_then(Value::as_bool) {
                Some(m) => m,
                None => {
                    return Err(Refusal::rejected(
                        "muted",
                        "the muted field is not true or false".to_string(),
                    ))
                }
            };
            Command::Mute { zone, muted }
        }
        other => {
            return Err(Refusal::rejected(
                "t",
                format!(
                    "'{}' is not a command in catalog version {}",
                    other, CATALOG_VERSION
                ),
            ))
        }
    };
    Ok(command)
}

fn version_list() -> String {
    IMPLEMENTED_VERSIONS
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Refuse a message carrying a field the command does not declare.
fn expect_fields(members: &[(String, Value)], declared: &[&str]) -> Result<(), Refusal> {
    for (key, _) in members {
        if !declared.contains(&key.as_str()) {
            return Err(Refusal::rejected(
                key,
                format!(
                    "this command declares the fields {} and carries no '{}'",
                    declared.join(", "),
                    key
                ),
            ));
        }
    }
    for key in declared {
        if !members.iter().any(|(k, _)| k == key) {
            return Err(Refusal::rejected(
                key,
                format!("this command requires the field '{}' and it is absent", key),
            ));
        }
    }
    Ok(())
}

/// A zone, group or endpoint identifier: lower-case letters, digits and
/// hyphens, one to [`MAX_IDENTIFIER_LEN`] of them.
///
/// Narrow on purpose. An identifier appears in a DNS-SD instance name, in a
/// persisted state file's section header and in a URL query, and a character
/// that has to be escaped differently in each of those is a character that will
/// eventually be escaped wrongly in one of them.
pub fn is_identifier(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= MAX_IDENTIFIER_LEN
        && text
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn identifier(value: &Value, field: &str) -> Result<String, Refusal> {
    let text = match value.get(field).and_then(Value::as_str) {
        Some(t) => t,
        None => {
            return Err(Refusal::rejected(
                field,
                format!("the field '{}' is not a string", field),
            ))
        }
    };
    if !is_identifier(text) {
        return Err(Refusal::rejected(
            field,
            format!(
                "'{}' is not an identifier: the catalog declares 1 to {} characters of lower-case \
                 letters, digits and hyphens",
                text, MAX_IDENTIFIER_LEN
            ),
        ));
    }
    Ok(text.to_string())
}

/// A human-set name.
///
/// Anything printable up to [`MAX_NAME_LEN`] characters, with no control
/// character in it: the persisted state format holds a name on one line, and a
/// name carrying a newline would be a name that does not come back.
pub fn is_display_name(text: &str) -> bool {
    !text.is_empty()
        && text.chars().count() <= MAX_NAME_LEN
        && !text.chars().any(|c| c.is_control())
        && text.trim() == text
}

fn display_name(value: &Value, field: &str) -> Result<String, Refusal> {
    let text = match value.get(field).and_then(Value::as_str) {
        Some(t) => t,
        None => {
            return Err(Refusal::rejected(
                field,
                format!("the field '{}' is not a string", field),
            ))
        }
    };
    if !is_display_name(text) {
        return Err(Refusal::rejected(
            field,
            format!(
                "'{}' is not a name: the catalog declares 1 to {} characters, no control \
                 character, and no leading or trailing space",
                text.escape_debug(),
                MAX_NAME_LEN
            ),
        ));
    }
    Ok(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_volume_prints_with_three_places_and_reads_back_identical() {
        for thousandths in [0u32, 1, 500, 999, 1_000] {
            let v = Volume::from_thousandths(i64::from(thousandths)).unwrap();
            let text = v.literal();
            assert_eq!(text.len(), 5, "{}", text);
            assert_eq!(Volume::parse(&text), Some(v), "{}", text);
        }
        assert_eq!(Volume::SILENT.literal(), "0.000");
        assert_eq!(Volume::FULL.literal(), "1.000");
    }

    #[test]
    fn a_volume_outside_the_declared_range_is_not_a_volume() {
        assert_eq!(Volume::from_thousandths(1_001), None);
        assert_eq!(Volume::from_thousandths(-1), None);
        for text in ["1.001", "2.000", "-0.500", "0.5001", "1e0", ".5", "0.5x", ""] {
            assert_eq!(Volume::parse(text), None, "{}", text);
        }
    }

    #[test]
    fn every_command_round_trips_through_its_own_bytes() {
        for command in [
            Command::Hello,
            Command::Attach {
                zone: "kitchen".to_string(),
                endpoint: "endpoint-a".to_string(),
            },
            Command::Name {
                zone: "kitchen".to_string(),
                name: "Kitchen".to_string(),
            },
            Command::Group {
                zone: "kitchen".to_string(),
                group: "downstairs".to_string(),
            },
            Command::Ungroup {
                zone: "kitchen".to_string(),
            },
            Command::Volume {
                zone: "kitchen".to_string(),
                volume: Volume::from_thousandths(500).unwrap(),
            },
            Command::Mute {
                zone: "kitchen".to_string(),
                muted: true,
            },
        ] {
            let bytes = command.encode();
            assert_eq!(decode_command(&bytes), Ok(command), "{}", bytes);
        }
    }

    #[test]
    fn an_unknown_version_refuses_the_session_and_names_both_sides() {
        let refusal = decode_command(r#"{"v":9,"t":"hello"}"#).unwrap_err();
        assert!(refusal.ends_the_session());
        assert_eq!(refusal.field, "v");
        assert!(refusal.detail.contains("catalog version 9 was offered"), "{}", refusal);
        assert!(refusal.detail.contains("implements 1"), "{}", refusal);
        assert!(refusal.encode().contains(r#""offered":9"#), "{}", refusal.encode());
        assert!(
            refusal.encode().contains(r#""implemented":[1]"#),
            "{}",
            refusal.encode()
        );
    }

    #[test]
    fn a_field_the_command_does_not_declare_is_refused() {
        let refusal =
            decode_command(r#"{"v":1,"t":"ungroup","zone":"kitchen","volume":0.500}"#).unwrap_err();
        assert_eq!(refusal.field, "volume");
        assert!(!refusal.ends_the_session());
    }
}
