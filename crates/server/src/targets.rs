//! The targets of a control state: one per room, per saved group and per
//! live group, each with the key the UPnP renderers and the Soloist
//! receivers both name it by (`room:<id>`, `group:<id>`,
//! `live:<member ids, sorted, joined by +>`; `chorus_upnp::uuid::Target`).
//!
//! Read out of the state message the control plane fans out, so anything
//! that follows the fanout sees what a subscriber sees. Shared since goal
//! 17: the UPnP renderers (`crate::upnp`) and the Soloist receiver manager
//! (`crate::soloist`) both follow targets, and the second must work on a
//! server started without `--upnp`.
//!
//! Control code: strings and arithmetic, no PCM, no clock;
//! `audio-path.conf` records it as excluded.

use std::collections::BTreeMap;

use chorus_control::json::{self, Value};
use chorus_upnp::uuid::Target;

/// What a target is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Room,
    Saved,
    Live,
}

/// One target as the control state has it now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Spec {
    /// `room:<id>`, `group:<id>` or `live:<members>`: the name the UDN is
    /// made of, and the owner name in the player pool.
    pub(crate) key: String,
    pub(crate) kind: Kind,
    /// The friendlyName.
    pub(crate) name: String,
    /// The member rooms.
    pub(crate) rooms: Vec<String>,
    /// What a `take` names to make this target's rooms play together.
    pub(crate) take: String,
    /// The formed group to set the group volume of, when the target's rooms
    /// are one formed group now (a saved group that is not formed has none).
    pub(crate) group: Option<String>,
    /// Volume in thousandths and mute, as the control state holds them.
    pub(crate) volume: u16,
    pub(crate) mute: bool,
    /// The ceiling of `volume` now, in thousandths: a room's effective limit
    /// (its limit, or a quiet hour's when one is active); for a group the
    /// average of its rooms' effective limits, which is the most the control
    /// plane's group volume (an average of rooms each clamped to its own
    /// limit) can reach.
    pub(crate) limit: u16,
    /// What the target's rooms play now, as the control state spells it
    /// (`stream`, `none`, `line-in:<endpoint>/<input>`, `player:<id>`, ...),
    /// when they are one formed group; empty for a saved group that is not
    /// formed.
    pub(crate) source: String,
    /// That group's now-playing record, when it has one.
    pub(crate) playing: Option<Playing>,
    /// The inputs the endpoints of the target's rooms offer, as
    /// `<endpoint>/<input>` with a label when the state gives one.
    pub(crate) inputs: Vec<(String, Option<String>)>,
}

/// A group's now-playing record, as far as Info and Time tell it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Playing {
    pub(crate) title: Option<String>,
    pub(crate) artist: Option<String>,
    pub(crate) album: Option<String>,
    pub(crate) art_url: Option<String>,
    pub(crate) duration_ms: Option<u64>,
}

pub(crate) fn thousandths(v: Option<&Value>) -> u16 {
    v.and_then(Value::as_num)
        .and_then(|n| n.parse::<f64>().ok())
        .map_or(0, |f| (f * 1000.0).round().clamp(0.0, 1000.0) as u16)
}

pub(crate) fn strings(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Arr(items)) => items
            .iter()
            .filter_map(|i| i.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

pub(crate) fn items<'a>(state: &'a Value, key: &str) -> &'a [Value] {
    match state.get(key) {
        Some(Value::Arr(items)) => items,
        _ => &[],
    }
}

pub(crate) fn playing_of(group: &Value) -> Option<Playing> {
    let record = group.get("now_playing")?;
    let text = |key: &str| record.get(key).and_then(Value::as_str).map(str::to_string);
    Some(Playing {
        title: text("title"),
        artist: text("artist"),
        album: text("album"),
        art_url: text("art_url"),
        duration_ms: record
            .get("duration_ms")
            .and_then(Value::as_num)
            .and_then(|n| n.parse().ok()),
    })
}

/// The inputs the state lists, each `<endpoint>/<input>` with its label when
/// there is one. An item is the input's literal, or (once inputs carry
/// labels) an object naming it in `id` or `input` with a `name`.
pub(crate) fn inputs_of(state: &Value) -> Vec<(String, Option<String>)> {
    items(state, "inputs")
        .iter()
        .filter_map(|item| match item {
            Value::Obj(_) => {
                let id = item
                    .get("id")
                    .or_else(|| item.get("input"))
                    .and_then(Value::as_str)?;
                let name = item
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|n| !n.is_empty());
                Some((id.to_string(), name.map(str::to_string)))
            }
            other => other.as_str().map(|id| (id.to_string(), None)),
        })
        .collect()
}

/// One room of the state, as the targets need it.
pub(crate) struct RoomFacts {
    name: String,
    volume: u16,
    mute: bool,
    limit: u16,
    group: String,
    endpoints: Vec<String>,
}

/// One formed group of the state.
pub(crate) struct GroupFacts {
    kind: String,
    members: Vec<String>,
    volume: u16,
    source: String,
    playing: Option<Playing>,
}

/// The state's serial and its targets: one per room, per saved group and per
/// live group.
pub(crate) fn specs_of(state: &str) -> Option<(i64, Vec<Spec>)> {
    let state = json::parse(state).ok()?;
    let serial = state
        .get("serial")
        .and_then(Value::as_num)
        .and_then(|n| n.parse().ok())?;
    let text = |v: &Value, key: &str| v.get(key).and_then(Value::as_str).map(str::to_string);
    let offered = inputs_of(&state);
    let mut rooms: BTreeMap<String, RoomFacts> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    for zone in items(&state, "zones") {
        let id = text(zone, "id")?;
        let limit = zone
            .get("effective_limit")
            .or_else(|| zone.get("limit"))
            .map_or(1000, |v| thousandths(Some(v)));
        order.push(id.clone());
        rooms.insert(
            id.clone(),
            RoomFacts {
                name: text(zone, "name").unwrap_or_else(|| id.clone()),
                volume: thousandths(zone.get("volume")),
                mute: zone.get("muted").and_then(Value::as_bool).unwrap_or(false),
                limit,
                group: text(zone, "group").unwrap_or_else(|| id.clone()),
                endpoints: strings(zone.get("endpoints")),
            },
        );
    }
    let mut formed: BTreeMap<String, GroupFacts> = BTreeMap::new();
    for group in items(&state, "groups") {
        let id = text(group, "id")?;
        formed.insert(
            id,
            GroupFacts {
                kind: text(group, "kind").unwrap_or_default(),
                members: strings(group.get("zones")),
                volume: thousandths(group.get("volume")),
                source: text(group, "source").unwrap_or_default(),
                playing: playing_of(group),
            },
        );
    }
    let all_muted = |members: &[String]| {
        !members.is_empty() && members.iter().all(|m| rooms.get(m).is_some_and(|r| r.mute))
    };
    // The average of the members' values, rounded half up: the control
    // plane's group volume, and so the group's ceiling too.
    let average = |members: &[String], of: &dyn Fn(&RoomFacts) -> u16| {
        let sum: u32 = members
            .iter()
            .filter_map(|m| rooms.get(m).map(|r| u32::from(of(r))))
            .sum();
        let n = members.len().max(1) as u32;
        ((2 * sum + n) / (2 * n)) as u16
    };
    // The inputs the endpoints of these rooms offer, in the state's order.
    let inputs_for = |members: &[String]| -> Vec<(String, Option<String>)> {
        offered
            .iter()
            .filter(|(id, _)| {
                id.split_once('/').is_some_and(|(endpoint, _)| {
                    members.iter().any(|m| {
                        rooms
                            .get(m)
                            .is_some_and(|r| r.endpoints.iter().any(|e| e == endpoint))
                    })
                })
            })
            .cloned()
            .collect()
    };
    let playing_in = |group: &str| {
        formed
            .get(group)
            .map(|g| (g.source.clone(), g.playing.clone()))
    };
    let mut specs = Vec::new();
    for id in &order {
        let room = &rooms[id];
        let (source, playing) = playing_in(&room.group).unwrap_or_default();
        let members = vec![id.clone()];
        specs.push(Spec {
            key: Target::Room(id).name(),
            kind: Kind::Room,
            name: room.name.clone(),
            inputs: inputs_for(&members),
            rooms: members,
            take: id.clone(),
            group: None,
            volume: room.volume,
            mute: room.mute,
            limit: room.limit,
            source,
            playing,
        });
    }
    for saved in items(&state, "saved_groups") {
        let id = text(saved, "id")?;
        let members = strings(saved.get("zones"));
        let (group, volume, mute) = match formed.get(&id) {
            Some(g) => (Some(id.clone()), g.volume, all_muted(&g.members)),
            // Not formed: the average of its rooms, as the group volume
            // would be.
            None => (None, average(&members, &|r| r.volume), all_muted(&members)),
        };
        let (source, playing) = playing_in(&id).unwrap_or_default();
        specs.push(Spec {
            key: Target::Group(&id).name(),
            kind: Kind::Saved,
            name: text(saved, "name").unwrap_or_else(|| id.clone()),
            limit: average(&members, &|r| r.limit),
            inputs: inputs_for(&members),
            rooms: members,
            take: id,
            group,
            volume,
            mute,
            source,
            playing,
        });
    }
    for (id, g) in &formed {
        if g.kind != "live" {
            continue;
        }
        let mut sorted: Vec<&str> = g.members.iter().map(String::as_str).collect();
        sorted.sort_unstable();
        sorted.dedup();
        // A live group is called by its rooms: their display names in the
        // order of their ids, joined by " + ".
        let name = sorted
            .iter()
            .map(|m| rooms.get(*m).map_or(*m, |r| r.name.as_str()))
            .collect::<Vec<_>>()
            .join(" + ");
        specs.push(Spec {
            key: Target::Live(&sorted).name(),
            kind: Kind::Live,
            name,
            limit: average(&g.members, &|r| r.limit),
            inputs: inputs_for(&g.members),
            rooms: g.members.clone(),
            take: id.clone(),
            group: Some(id.clone()),
            volume: g.volume,
            mute: all_muted(&g.members),
            source: g.source.clone(),
            playing: g.playing.clone(),
        });
    }
    Some((serial, specs))
}
