//! What the topics carry: pieces of the control state message, unchanged.
//!
//! P10 settled that MQTT reuses the control API's state message and its
//! `fixtures/control`, so there is one schema. The way to keep that true is
//! not to encode anything a second time: a room's payload is the bytes of its
//! `zones[]` entry in the state message, and a saved group's is the bytes of
//! its `saved_groups[]` entry, cut out of the message the server already
//! encoded. A consumer that can read `GET /api/state` can read these, and
//! `docs/control-plane.md` is their schema.

use chorus_control::json::{self, Value};

/// The retained objects one state message holds: `(id, object bytes)` for
/// every room and every saved group, in the message's own order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Retained {
    /// Every `zones[]` entry.
    pub rooms: Vec<(String, String)>,
    /// Every `saved_groups[]` entry.
    pub groups: Vec<(String, String)>,
}

/// Where the JSON value starting at `at` ends (one past its last byte), by
/// its brackets and quotes alone. The text is the server's own encoding; a
/// text that ends early gives `None`.
fn value_end(bytes: &[u8], at: usize) -> Option<usize> {
    match bytes.get(at)? {
        b'"' => {
            let mut i = at + 1;
            loop {
                match bytes.get(i)? {
                    b'\\' => i += 2,
                    b'"' => return Some(i + 1),
                    _ => i += 1,
                }
            }
        }
        b'{' | b'[' => {
            let mut depth = 0usize;
            let mut i = at;
            loop {
                match bytes.get(i)? {
                    b'"' => {
                        i = value_end(bytes, i)?;
                        continue;
                    }
                    b'{' | b'[' => depth += 1,
                    b'}' | b']' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(i + 1);
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
        }
        _ => {
            let mut i = at;
            while let Some(b) = bytes.get(i) {
                if matches!(b, b',' | b'}' | b']') || b.is_ascii_whitespace() {
                    break;
                }
                i += 1;
            }
            (i > at).then_some(i)
        }
    }
}

fn skip_space(bytes: &[u8], mut at: usize) -> usize {
    while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
        at += 1;
    }
    at
}

/// The byte ranges of the elements of the array starting at `at`.
fn elements(bytes: &[u8], at: usize) -> Option<Vec<(usize, usize)>> {
    if bytes.get(at) != Some(&b'[') {
        return None;
    }
    let mut out = Vec::new();
    let mut i = skip_space(bytes, at + 1);
    if bytes.get(i) == Some(&b']') {
        return Some(out);
    }
    loop {
        let end = value_end(bytes, i)?;
        out.push((i, end));
        i = skip_space(bytes, end);
        match bytes.get(i)? {
            b',' => i = skip_space(bytes, i + 1),
            b']' => return Some(out),
            _ => return None,
        }
    }
}

/// The byte range of the value of the top-level member `key`.
fn member(bytes: &[u8], key: &str) -> Option<(usize, usize)> {
    let mut i = skip_space(bytes, 0);
    if bytes.get(i) != Some(&b'{') {
        return None;
    }
    i = skip_space(bytes, i + 1);
    loop {
        if bytes.get(i) != Some(&b'"') {
            return None;
        }
        let key_end = value_end(bytes, i)?;
        let found = &bytes[i + 1..key_end - 1] == key.as_bytes();
        i = skip_space(bytes, key_end);
        if bytes.get(i) != Some(&b':') {
            return None;
        }
        i = skip_space(bytes, i + 1);
        let end = value_end(bytes, i)?;
        if found {
            return Some((i, end));
        }
        i = skip_space(bytes, end);
        match bytes.get(i)? {
            b',' => i = skip_space(bytes, i + 1),
            _ => return None,
        }
    }
}

fn objects(state: &str, key: &str) -> Result<Vec<(String, String)>, String> {
    let bytes = state.as_bytes();
    let (at, _) = member(bytes, key)
        .ok_or_else(|| format!("the state message has no top-level '{}'", key))?;
    let spans = elements(bytes, at).ok_or_else(|| format!("'{}' is not an array", key))?;
    let mut out = Vec::with_capacity(spans.len());
    for (from, to) in spans {
        // Every byte that ends a span is ASCII (a quote, a bracket or part of
        // a number), so the cut falls on a character boundary.
        let object = &state[from..to];
        let id = json::parse(object)
            .ok()
            .as_ref()
            .and_then(|v| v.get("id"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| format!("an entry of '{}' has no string 'id'", key))?;
        out.push((id, object.to_string()));
    }
    Ok(out)
}

/// Cut a control state message (catalog v2, as `GET /api/state` serves it and
/// the fanout carries it) into its rooms and saved groups.
///
/// Each object is the message's own bytes for that entry, not a re-encoding.
pub fn retained_of(state: &str) -> Result<Retained, String> {
    Ok(Retained {
        rooms: objects(state, "zones")?,
        groups: objects(state, "saved_groups")?,
    })
}

/// The payload of `<prefix>/speakers/<endpoint id>/event`: one controller
/// command the server accepted, as one JSON object in the catalog's canonical
/// form (no spaces, this key order).
///
/// - `endpoint`: the endpoint whose button it was (its authenticated id);
/// - `zone`: the room that endpoint plays in, which the command acted on;
/// - `command`: the protocol's name for it (`volume_step`, `toggle`, ...);
/// - `value`: the command's argument, 0 where it takes none;
/// - `target`: the group for `join`, empty otherwise;
/// - `outcome`: `applied` when it changed the room, `waits-for-an-input`
///   for a transport command, which acts on an input (docs/protocol.md).
pub fn controller_event(
    endpoint: &str,
    zone: &str,
    command: &str,
    value: i16,
    target: &str,
    outcome: &str,
) -> String {
    json::write(&Value::Obj(vec![
        ("endpoint".to_string(), Value::text(endpoint)),
        ("zone".to_string(), Value::text(zone)),
        ("command".to_string(), Value::text(command)),
        ("value".to_string(), Value::int(i64::from(value))),
        ("target".to_string(), Value::text(target)),
        ("outcome".to_string(), Value::text(outcome)),
    ]))
}
