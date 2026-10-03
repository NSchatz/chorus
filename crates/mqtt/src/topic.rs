//! The topics chorus publishes, and the rules that keep them well-formed.
//!
//! | Topic | Retained | Payload |
//! |---|---|---|
//! | `<prefix>/server/status` | yes | `online`, or `offline` (the last will) |
//! | `<prefix>/rooms/<room id>/state` | yes | the room's object from the state message |
//! | `<prefix>/groups/<saved group id>/state` | yes | the saved group's object |
//! | `<prefix>/speakers/<endpoint id>/event` | no | one accepted controller command |
//!
//! That is the whole list (`docs/mqtt.md`). There is no command topic and
//! nothing under `homeassistant/`: P10 settled a read-only publisher with no
//! Home Assistant discovery, and [`Topics::new`] refuses a prefix that would
//! put chorus's topics there.

use std::fmt::Write as _;

/// The prefix when `--mqtt-prefix` is not given. The `v1` is the version of
/// this topic layout, not of the control catalog.
pub const DEFAULT_PREFIX: &str = "chorus/v1";

/// The client identifier when `--mqtt-client-id` is not given.
pub const DEFAULT_CLIENT_ID: &str = "chorus";

/// The payload of the status topic while the server is connected.
pub const ONLINE: &str = "online";

/// The payload of the status topic once it is not: the last will, and what a
/// clean stop publishes itself.
pub const OFFLINE: &str = "offline";

/// The longest client identifier every broker must accept [MQTT-3.1.3-5].
pub const MAX_CLIENT_ID_LEN: usize = 23;

/// Whether `id` is a client identifier every MQTT 3.1.1 broker must accept:
/// 1 to 23 bytes of `0-9`, `a-z`, `A-Z` [MQTT-3.1.3-5]. A broker may accept
/// more (SPEC 3.1.3.1); chorus asks for no more than the portable set.
pub fn check_client_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > MAX_CLIENT_ID_LEN {
        return Err(format!(
            "'{}' is {} bytes; an MQTT client id every broker accepts is 1 to {}",
            id,
            id.len(),
            MAX_CLIENT_ID_LEN
        ));
    }
    if !id.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(format!(
            "'{}' holds something other than 0-9, a-z and A-Z, the characters every broker \
             accepts in a client id",
            id
        ));
    }
    Ok(())
}

/// One id as one topic level.
///
/// A topic name is split into levels at `/`, and `+` and `#` are wildcards
/// that a topic name must not hold [MQTT-3.3.2-2]; U+0000 is forbidden
/// [MQTT-4.7.3-2] and the other control characters are ones a broker may
/// close the connection on (SPEC 1.5.3). So each of `/`, `+`, `#`, `%` and
/// every control character becomes `%XX` for each byte of its UTF-8 form
/// (upper-case hexadecimal), and everything else is kept as it is. `%` is
/// escaped too, which is what makes the mapping one to one: two ids never
/// share a level. An empty id becomes a lone `%`, which no escape produces,
/// so no topic ever has an empty level.
///
/// The catalog's own ids are lower-case letters, digits and `-`
/// (`chorus_control::catalog::is_identifier`) and pass through unchanged;
/// the rule is for an endpoint id, which the endpoint chooses.
pub fn segment(id: &str) -> String {
    if id.is_empty() {
        return "%".to_string();
    }
    let mut out = String::with_capacity(id.len());
    let mut utf8 = [0u8; 4];
    for c in id.chars() {
        if matches!(c, '/' | '+' | '#' | '%') || c.is_control() {
            for byte in c.encode_utf8(&mut utf8).bytes() {
                let _ = write!(out, "%{:02X}", byte);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// The topics under one prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Topics {
    prefix: String,
}

impl Topics {
    /// The topics under `prefix`, or why it is not a prefix chorus publishes
    /// under.
    ///
    /// A prefix is one or more levels separated by `/`: no empty level (so no
    /// leading, trailing or doubled `/`), no wildcard, no control character,
    /// at most 128 bytes, not beginning with `$` (those topics are the
    /// broker's own, SPEC 4.7.2), and its first level is not `homeassistant`
    /// (P10: chorus publishes no Home Assistant discovery, and this is what
    /// makes that hold whatever the flag says).
    pub fn new(prefix: &str) -> Result<Topics, String> {
        if prefix.is_empty() {
            return Err("the prefix is empty".to_string());
        }
        if prefix.len() > 128 {
            return Err(format!("the prefix is {} bytes; at most 128", prefix.len()));
        }
        if prefix.split('/').any(str::is_empty) {
            return Err(format!(
                "'{}' has an empty level (a leading, trailing or doubled '/')",
                prefix
            ));
        }
        if prefix.contains(['+', '#']) {
            return Err(format!(
                "'{}' holds '+' or '#', which are wildcards and not allowed in a topic name",
                prefix
            ));
        }
        if prefix.chars().any(char::is_control) {
            return Err("the prefix holds a control character".to_string());
        }
        if prefix.starts_with('$') {
            return Err(format!(
                "'{}' begins with '$', and those topics are the broker's own",
                prefix
            ));
        }
        if prefix.split('/').next() == Some("homeassistant") {
            return Err(format!(
                "'{}' is under homeassistant/, where Home Assistant reads device discovery; \
                 chorus publishes none (docs/mqtt.md)",
                prefix
            ));
        }
        Ok(Topics {
            prefix: prefix.to_string(),
        })
    }

    /// The prefix.
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// `<prefix>/server/status`.
    pub fn status(&self) -> String {
        format!("{}/server/status", self.prefix)
    }

    /// `<prefix>/rooms/<room id>/state`.
    pub fn room(&self, id: &str) -> String {
        format!("{}/rooms/{}/state", self.prefix, segment(id))
    }

    /// `<prefix>/groups/<saved group id>/state`.
    pub fn group(&self, id: &str) -> String {
        format!("{}/groups/{}/state", self.prefix, segment(id))
    }

    /// `<prefix>/speakers/<endpoint id>/event`.
    pub fn speaker_event(&self, id: &str) -> String {
        format!("{}/speakers/{}/event", self.prefix, segment(id))
    }
}
