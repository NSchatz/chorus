//! Printing a part of the state message.
//!
//! With `--json` the output is the server's own JSON: the part asked for, cut
//! out of the state message and written back with the catalog's canonical
//! writer, which keeps every member's order and every number's digits. So a
//! `--json` output is a substring of the state the server sent wherever the
//! part is one value of it, and nothing here ever reformats a number.
//!
//! Without it the output is a table or `key: value` lines for a person. That
//! form is not a contract; scripts use `--json`.

use chorus_control::json::{self, Value};

use crate::parse::{Target, View};

/// The array under `key`, or nothing where the state has no such member (the
/// server leaves `speakers`, `key_changes` and `firmware` out when empty).
fn items<'a>(state: &'a Value, key: &str) -> &'a [Value] {
    match state.get(key) {
        Some(Value::Arr(items)) => items,
        _ => &[],
    }
}

/// A value as a person reads it.
fn plain(value: &Value) -> String {
    match value {
        Value::Null => "-".to_string(),
        Value::Bool(true) => "yes".to_string(),
        Value::Bool(false) => "no".to_string(),
        Value::Num(digits) => digits.clone(),
        Value::Str(text) if text.is_empty() => "-".to_string(),
        Value::Str(text) => text.clone(),
        Value::Arr(items) if items.is_empty() => "-".to_string(),
        Value::Arr(items)
            if items
                .iter()
                .all(|i| !matches!(i, Value::Arr(_) | Value::Obj(_))) =>
        {
            items.iter().map(plain).collect::<Vec<_>>().join(",")
        }
        nested => json::write(nested),
    }
}

/// The member `key` of `value`, as a person reads it.
fn cell(value: &Value, key: &str) -> String {
    value.get(key).map(plain).unwrap_or_else(|| "-".to_string())
}

fn has_id(value: &Value, id: &str) -> bool {
    value.get("id").and_then(Value::as_str) == Some(id)
}

fn ids(values: &[Value]) -> String {
    let ids: Vec<&str> = values
        .iter()
        .filter_map(|v| v.get("id").and_then(Value::as_str))
        .collect();
    if ids.is_empty() {
        "none".to_string()
    } else {
        ids.join(", ")
    }
}

/// Left-aligned columns, two spaces apart, no trailing spaces.
fn table(head: &[&str], rows: &[Vec<String>]) -> String {
    let mut widths: Vec<usize> = head.iter().map(|h| h.chars().count()).collect();
    for row in rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.chars().count());
        }
    }
    let mut out = String::new();
    let mut line = |cells: Vec<&str>| {
        let mut text = String::new();
        for (cell, width) in cells.iter().zip(&widths) {
            text.push_str(cell);
            text.extend(std::iter::repeat_n(' ', width - cell.chars().count() + 2));
        }
        out.push_str(text.trim_end());
        out.push('\n');
    };
    line(head.to_vec());
    for row in rows {
        line(row.iter().map(String::as_str).collect());
    }
    out
}

/// Every member of an object as a `key: value` line.
fn lines(object: &Value) -> String {
    let mut out = String::new();
    if let Value::Obj(members) = object {
        for (key, value) in members {
            out.push_str(&format!("{}: {}\n", key, plain(value)));
        }
    }
    out
}

fn json_line(value: &Value) -> String {
    format!("{}\n", json::write(value))
}

fn object(members: Vec<(&str, Value)>) -> Value {
    Value::Obj(
        members
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

fn array(items: &[Value]) -> Value {
    Value::Arr(items.to_vec())
}

fn room<'a>(state: &'a Value, id: &str) -> Result<&'a Value, String> {
    let zones = items(state, "zones");
    zones
        .iter()
        .find(|z| has_id(z, id))
        .ok_or_else(|| format!("there is no room '{}'; the rooms are {}", id, ids(zones)))
}

fn rooms(state: &Value) -> String {
    let zones = items(state, "zones");
    if zones.is_empty() {
        return "no rooms\n".to_string();
    }
    let count = |zone: &Value, key: &str| match zone.get(key) {
        Some(Value::Arr(items)) => items.len(),
        _ => 0,
    };
    let rows: Vec<Vec<String>> = zones
        .iter()
        .map(|z| {
            vec![
                cell(z, "id"),
                cell(z, "name"),
                cell(z, "group"),
                cell(z, "volume"),
                cell(z, "muted"),
                // A v1 state has no limit; the cell says so rather than guess.
                cell(z, "effective_limit"),
                format!("{}/{}", count(z, "present"), count(z, "endpoints")),
            ]
        })
        .collect();
    table(
        &[
            "ROOM", "NAME", "GROUP", "VOLUME", "MUTED", "LIMIT", "PRESENT",
        ],
        &rows,
    )
}

fn groups(state: &Value) -> String {
    let live = items(state, "groups");
    let saved = items(state, "saved_groups");
    let mut out = if live.is_empty() {
        "no groups\n".to_string()
    } else {
        let rows: Vec<Vec<String>> = live
            .iter()
            .map(|g| {
                vec![
                    cell(g, "id"),
                    cell(g, "kind"),
                    cell(g, "volume"),
                    cell(g, "source"),
                    cell(g, "zones"),
                ]
            })
            .collect();
        table(&["GROUP", "KIND", "VOLUME", "SOURCE", "ROOMS"], &rows)
    };
    if !saved.is_empty() {
        let rows: Vec<Vec<String>> = saved
            .iter()
            .map(|g| {
                vec![
                    cell(g, "id"),
                    cell(g, "name"),
                    cell(g, "active"),
                    cell(g, "zones"),
                ]
            })
            .collect();
        out.push('\n');
        out.push_str(&table(&["SAVED", "NAME", "ACTIVE", "ROOMS"], &rows));
    }
    out
}

fn volume<'a>(state: &'a Value, target: &Target) -> Result<(&'a Value, String), String> {
    match target {
        Target::Room(id) => {
            let zone = room(state, id)?;
            let mut line = format!(
                "{} volume={} muted={}",
                id,
                cell(zone, "volume"),
                cell(zone, "muted")
            );
            if zone.get("limit").is_some() {
                line.push_str(&format!(
                    " limit={} effective_limit={}",
                    cell(zone, "limit"),
                    cell(zone, "effective_limit")
                ));
            }
            line.push('\n');
            Ok((zone, line))
        }
        Target::Group(id) => {
            let groups = items(state, "groups");
            let group = groups.iter().find(|g| has_id(g, id)).ok_or_else(|| {
                format!(
                    "there is no group '{}' now; the groups are {}",
                    id,
                    ids(groups)
                )
            })?;
            let line = format!(
                "{} volume={} rooms={}\n",
                id,
                cell(group, "volume"),
                cell(group, "zones")
            );
            Ok((group, line))
        }
    }
}

fn inputs(state: &Value) -> String {
    let inputs = items(state, "inputs");
    if inputs.is_empty() {
        return "no inputs\n".to_string();
    }
    let rows: Vec<Vec<String>> = inputs
        .iter()
        .map(|input| {
            let name = plain(input);
            let source = format!("line-in:{}", name);
            let playing: Vec<Value> = items(state, "groups")
                .iter()
                .filter(|g| g.get("source").and_then(Value::as_str) == Some(source.as_str()))
                .filter_map(|g| g.get("id").cloned())
                .collect();
            vec![name, plain(&Value::Arr(playing))]
        })
        .collect();
    table(&["INPUT", "PLAYING IN"], &rows)
}

/// The rooms that list `id` among their endpoints.
fn rooms_of(state: &Value, id: &str) -> Value {
    Value::Arr(
        items(state, "zones")
            .iter()
            .filter(|z| {
                matches!(z.get("endpoints"), Some(Value::Arr(e))
                    if e.iter().any(|e| e.as_str() == Some(id)))
            })
            .filter_map(|z| z.get("id").cloned())
            .collect(),
    )
}

fn endpoints(state: &Value) -> String {
    let speakers = items(state, "speakers");
    let attached = items(state, "endpoints");
    let changes = items(state, "key_changes");
    let mut parts = Vec::new();
    if !speakers.is_empty() {
        let rows: Vec<Vec<String>> = speakers
            .iter()
            .map(|s| {
                vec![
                    cell(s, "id"),
                    cell(s, "name"),
                    cell(s, "room"),
                    cell(s, "present"),
                    cell(s, "link"),
                    cell(s, "software"),
                    cell(s, "roles"),
                ]
            })
            .collect();
        parts.push(table(
            &[
                "SPEAKER", "NAME", "ROOM", "PRESENT", "LINK", "SOFTWARE", "ROLES",
            ],
            &rows,
        ));
    }
    if !attached.is_empty() {
        let rows: Vec<Vec<String>> = attached
            .iter()
            .map(|e| {
                let id = cell(e, "id");
                let rooms = plain(&rooms_of(state, &id));
                vec![id, cell(e, "link"), rooms]
            })
            .collect();
        parts.push(table(&["ENDPOINT", "LINK", "ROOMS"], &rows));
    }
    if !changes.is_empty() {
        let rows: Vec<Vec<String>> = changes
            .iter()
            .map(|c| vec![cell(c, "id"), cell(c, "pinned"), cell(c, "offered")])
            .collect();
        parts.push(table(&["KEY CHANGED", "PINNED", "OFFERED"], &rows));
    }
    if parts.is_empty() {
        return "no speakers or endpoints\n".to_string();
    }
    parts.join("\n")
}

fn images(state: &Value) -> &[Value] {
    match state.get("firmware").and_then(|f| f.get("images")) {
        Some(Value::Arr(images)) => images,
        _ => &[],
    }
}

fn image_table(images: &[Value]) -> String {
    if images.is_empty() {
        return "no staged images\n".to_string();
    }
    let rows: Vec<Vec<String>> = images
        .iter()
        .map(|i| {
            vec![
                cell(i, "name"),
                cell(i, "version"),
                cell(i, "board"),
                cell(i, "size"),
                cell(i, "verdict"),
                cell(i, "reason"),
            ]
        })
        .collect();
    table(
        &["IMAGE", "VERSION", "BOARD", "SIZE", "VERDICT", "REASON"],
        &rows,
    )
}

fn update_table(reporting: &[Value]) -> String {
    if reporting.is_empty() {
        return "no speaker reports its firmware\n".to_string();
    }
    let rows: Vec<Vec<String>> = reporting
        .iter()
        .filter_map(|s| s.get("firmware").map(|f| (s, f)))
        .map(|(s, f)| {
            let available = f.get("update_available").and_then(Value::as_bool) == Some(true);
            vec![
                cell(s, "id"),
                cell(s, "name"),
                cell(f, "version"),
                cell(f, "board"),
                cell(f, "slot"),
                cell(f, "state"),
                cell(f, "reason"),
                if available {
                    cell(f, "image_version")
                } else {
                    "-".to_string()
                },
                cell(f, "image"),
                format!("{}/{}", cell(f, "received"), cell(f, "size")),
            ]
        })
        .collect();
    table(
        &[
            "SPEAKER",
            "NAME",
            "RUNS",
            "BOARD",
            "SLOT",
            "STATE",
            "REASON",
            "UPDATE",
            "INSTALLING",
            "RECEIVED",
        ],
        &rows,
    )
}

/// Print `view` of `state`: the text for stdout, or what was not found.
pub fn render(state: &Value, view: &View, as_json: bool) -> Result<String, String> {
    let either = |value: Value, text: String| {
        if as_json {
            json_line(&value)
        } else {
            text
        }
    };
    Ok(match view {
        View::Rooms => either(array(items(state, "zones")), rooms(state)),
        View::Room(id) => {
            let zone = room(state, id)?;
            either(zone.clone(), lines(zone))
        }
        View::Groups => either(
            object(vec![
                ("groups", array(items(state, "groups"))),
                ("saved_groups", array(items(state, "saved_groups"))),
            ]),
            groups(state),
        ),
        View::Volume(target) => {
            let (value, line) = volume(state, target)?;
            either(value.clone(), line)
        }
        View::Inputs => either(array(items(state, "inputs")), inputs(state)),
        View::Endpoints => either(
            object(vec![
                ("endpoints", array(items(state, "endpoints"))),
                ("speakers", array(items(state, "speakers"))),
                ("key_changes", array(items(state, "key_changes"))),
            ]),
            endpoints(state),
        ),
        View::Endpoint(id) => {
            let speaker = items(state, "speakers").iter().find(|s| has_id(s, id));
            let endpoint = items(state, "endpoints").iter().find(|e| has_id(e, id));
            if speaker.is_none() && endpoint.is_none() {
                return Err(format!(
                    "there is no speaker or endpoint '{}'; the speakers are {} and the \
                     endpoints are {}",
                    id,
                    ids(items(state, "speakers")),
                    ids(items(state, "endpoints"))
                ));
            }
            let mut text = speaker.or(endpoint).map(lines).unwrap_or_default();
            if speaker.is_none() {
                text.push_str(&format!("rooms: {}\n", plain(&rooms_of(state, id))));
            }
            either(
                object(vec![
                    ("endpoint", endpoint.cloned().unwrap_or(Value::Null)),
                    ("speaker", speaker.cloned().unwrap_or(Value::Null)),
                ]),
                text,
            )
        }
        View::Images => either(array(images(state)), image_table(images(state))),
        View::Updates => {
            let reporting: Vec<Value> = items(state, "speakers")
                .iter()
                .filter(|s| s.get("firmware").is_some())
                .cloned()
                .collect();
            either(array(&reporting), update_table(&reporting))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_is_aligned_with_no_trailing_spaces() {
        let text = table(
            &["A", "LONGER"],
            &[
                vec!["one".to_string(), "x".to_string()],
                vec!["a-longer-cell".to_string(), "-".to_string()],
            ],
        );
        assert_eq!(
            text,
            "A              LONGER\none            x\na-longer-cell  -\n"
        );
    }

    #[test]
    fn a_value_reads_as_a_person_would_write_it() {
        let value = json::parse(
            r#"{"a":null,"b":true,"c":0.500,"d":"","e":[],"f":["x","y"],"g":[{"k":1}]}"#,
        )
        .unwrap();
        assert_eq!(
            lines(&value),
            "a: -\nb: yes\nc: 0.500\nd: -\ne: -\nf: x,y\ng: [{\"k\":1}]\n"
        );
    }
}
