//! Server-authoritative zone state.
//!
//! One process owns this and every subscriber is told what it says. There is
//! no state anywhere else that a subscriber could be reading instead, which is
//! what makes "fan the resulting state out to all subscribers" a complete
//! description rather than half of a reconciliation problem.
//!
//! # A change is applied whole or not at all
//!
//! [`Zones::apply`] validates everything a command asks for BEFORE it changes
//! anything, so a refused command leaves the state byte-identical to what it
//! was. That is not a nicety: the criterion is that a refused message leaves
//! every subscriber's state unchanged, and a validate-as-you-go apply would
//! satisfy it for some commands and not for others.
//!
//! # Every zone is in a group, always
//!
//! A group is the unit a stream is served to, so a zone in no group is a zone
//! with nothing to play. Ungrouping therefore puts a zone into a group of its
//! own, named for the zone, rather than into an absent state the state message
//! would have to spell `null`. One consequence worth stating: `ungroup` on a
//! zone that is already alone is not an error and changes nothing.

use crate::catalog::{is_display_name, is_identifier, Command, Refusal, Volume};
use crate::json::{self, Value};

/// One zone: what it is called, what it plays, and how loudly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Zone {
    /// The identifier, which never changes and is what commands name.
    pub id: String,
    /// The human-set name, which is what a person sees.
    pub name: String,
    /// The group whose stream this zone plays.
    pub group: String,
    /// The amplitude factor applied to the PCM this zone's endpoints play.
    pub volume: Volume,
    /// Whether this zone is muted.
    pub muted: bool,
    /// Every endpoint this zone has ever had, in the order they first
    /// attached, persisted across a restart.
    pub endpoints: Vec<String>,
    /// The subset of `endpoints` attached right now. Never persisted: which
    /// endpoints are switched on is a fact about now.
    pub present: Vec<String>,
}

impl Zone {
    /// A zone with the shipped defaults: named for its identifier, alone in a
    /// group of its own, at full scale and unmuted.
    pub fn new(id: &str) -> Zone {
        Zone {
            id: id.to_string(),
            name: id.to_string(),
            group: id.to_string(),
            volume: Volume::FULL,
            muted: false,
            endpoints: Vec::new(),
            present: Vec::new(),
        }
    }

    /// The amplitude factor this zone's endpoints apply, which is zero while
    /// it is muted.
    pub fn gain(&self) -> Volume {
        if self.muted {
            Volume::SILENT
        } else {
            self.volume
        }
    }
}

/// Every zone the server knows about, and the serial that says which version
/// of that a subscriber is holding.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Zones {
    zones: Vec<Zone>,
    serial: u64,
    /// Where each group's audio stream is served, and where a group with no
    /// entry of its own is served.
    group_audio: Vec<(String, String)>,
    default_audio: String,
}

impl Zones {
    /// An empty set of zones, with every group's stream served at
    /// `default_audio` until told otherwise.
    pub fn new(default_audio: &str) -> Zones {
        Zones {
            zones: Vec::new(),
            serial: 0,
            group_audio: Vec::new(),
            default_audio: default_audio.to_string(),
        }
    }

    /// Say where one group's stream is served.
    pub fn set_group_audio(&mut self, group: &str, address: &str) {
        match self.group_audio.iter_mut().find(|(g, _)| g == group) {
            Some((_, a)) => *a = address.to_string(),
            None => self
                .group_audio
                .push((group.to_string(), address.to_string())),
        }
        self.group_audio.sort();
    }

    /// Where a group's stream is served.
    pub fn audio_for(&self, group: &str) -> &str {
        self.group_audio
            .iter()
            .find(|(g, _)| g == group)
            .map(|(_, a)| a.as_str())
            .unwrap_or(&self.default_audio)
    }

    /// How many changes have been applied since this state was created or
    /// loaded.
    ///
    /// A subscriber can tell a state message it has already seen from one it
    /// has not without comparing the whole thing.
    pub fn serial(&self) -> u64 {
        self.serial
    }

    /// Set the serial, which loading persisted state does and nothing else
    /// should.
    pub fn set_serial(&mut self, serial: u64) {
        self.serial = serial;
    }

    /// Every zone, in the order they were added.
    pub fn zones(&self) -> &[Zone] {
        &self.zones
    }

    /// One zone by identifier.
    pub fn zone(&self, id: &str) -> Option<&Zone> {
        self.zones.iter().find(|z| z.id == id)
    }

    /// Whether any zone has been configured.
    pub fn is_empty(&self) -> bool {
        self.zones.is_empty()
    }

    /// Add a zone, which configuration does and no control message does.
    ///
    /// A control message cannot create a zone: the set of rooms is a fact
    /// about a house and is configured, and a typo in a zone name has to be a
    /// refusal rather than a new room nobody has.
    pub fn add(&mut self, zone: Zone) -> Result<(), Refusal> {
        if !is_identifier(&zone.id) {
            return Err(Refusal::rejected(
                "zone",
                format!("'{}' is not a zone identifier", zone.id),
            ));
        }
        if !is_display_name(&zone.name) {
            return Err(Refusal::rejected(
                "name",
                format!("'{}' is not a zone name", zone.name.escape_debug()),
            ));
        }
        if !is_identifier(&zone.group) {
            return Err(Refusal::rejected(
                "group",
                format!("'{}' is not a group identifier", zone.group),
            ));
        }
        if self.zone(&zone.id).is_some() {
            return Err(Refusal::rejected(
                "zone",
                format!("the zone '{}' is already configured", zone.id),
            ));
        }
        self.zones.push(zone);
        self.serial += 1;
        Ok(())
    }

    /// Mark an endpoint as gone, which a dropped control session does.
    ///
    /// It stays in the zone's membership; only its presence changes. See
    /// `docs/decisions/0019-a-persisted-endpoint-that-never-comes-back.md`.
    pub fn endpoint_left(&mut self, endpoint: &str) -> bool {
        let mut changed = false;
        for zone in &mut self.zones {
            let before = zone.present.len();
            zone.present.retain(|e| e != endpoint);
            changed |= zone.present.len() != before;
        }
        if changed {
            self.serial += 1;
        }
        changed
    }

    /// Apply one command, whole or not at all.
    ///
    /// A command that changes nothing still succeeds and still bumps the
    /// serial, so that a subscriber which asked for something already true is
    /// answered with the state rather than with silence.
    pub fn apply(&mut self, command: &Command) -> Result<(), Refusal> {
        // Validate first, against a copy of nothing: every check here reads
        // the state and writes none of it.
        if let Some(id) = command.zone() {
            if self.zone(id).is_none() {
                return Err(Refusal::rejected(
                    "zone",
                    format!(
                        "there is no zone '{}'; the zones configured on this server are {}",
                        id,
                        self.zone_list()
                    ),
                ));
            }
        }
        let index = command
            .zone()
            .and_then(|id| self.zones.iter().position(|z| z.id == id));

        match (command, index) {
            (Command::Hello, _) => return Ok(()),
            (Command::Attach { endpoint, .. }, Some(at)) => {
                let zone = &mut self.zones[at];
                if !zone.endpoints.iter().any(|e| e == endpoint) {
                    zone.endpoints.push(endpoint.clone());
                }
                if !zone.present.iter().any(|e| e == endpoint) {
                    zone.present.push(endpoint.clone());
                }
            }
            (Command::Name { name, .. }, Some(at)) => self.zones[at].name = name.clone(),
            (Command::Group { group, .. }, Some(at)) => self.zones[at].group = group.clone(),
            (Command::Ungroup { .. }, Some(at)) => {
                self.zones[at].group = self.zones[at].id.clone();
            }
            (Command::Volume { volume, .. }, Some(at)) => self.zones[at].volume = *volume,
            (Command::Mute { muted, .. }, Some(at)) => self.zones[at].muted = *muted,
            (_, None) => unreachable!("a command about a zone has been checked to have one"),
        }
        self.serial += 1;
        Ok(())
    }

    fn zone_list(&self) -> String {
        if self.zones.is_empty() {
            return "none: this server has no zone configured".to_string();
        }
        self.zones
            .iter()
            .map(|z| z.id.clone())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The state message, in the declared field order.
    pub fn state_value(&self) -> Value {
        let zones = self
            .zones
            .iter()
            .map(|z| {
                Value::Obj(vec![
                    ("id".to_string(), Value::text(&z.id)),
                    ("name".to_string(), Value::text(&z.name)),
                    ("group".to_string(), Value::text(&z.group)),
                    ("volume".to_string(), Value::Num(z.volume.literal())),
                    ("muted".to_string(), Value::Bool(z.muted)),
                    (
                        "endpoints".to_string(),
                        Value::Arr(z.endpoints.iter().map(|e| Value::text(e)).collect()),
                    ),
                    (
                        "present".to_string(),
                        Value::Arr(z.present.iter().map(|e| Value::text(e)).collect()),
                    ),
                    ("audio".to_string(), Value::text(self.audio_for(&z.group))),
                ])
            })
            .collect();
        Value::Obj(vec![
            (
                "v".to_string(),
                Value::int(crate::catalog::CATALOG_VERSION),
            ),
            ("t".to_string(), Value::text("state")),
            ("serial".to_string(), Value::int(self.serial as i64)),
            ("zones".to_string(), Value::Arr(zones)),
        ])
    }

    /// The bytes the state message is on the wire.
    pub fn encode_state(&self) -> String {
        json::write(&self.state_value())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_zones() -> Zones {
        let mut zones = Zones::new("127.0.0.1:4010");
        zones.add(Zone::new("kitchen")).unwrap();
        zones.add(Zone::new("study")).unwrap();
        zones
    }

    #[test]
    fn a_refused_command_leaves_the_state_byte_identical() {
        let mut zones = two_zones();
        let before = zones.encode_state();
        let refusal = zones
            .apply(&Command::Name {
                zone: "no-such-zone".to_string(),
                name: "Nowhere".to_string(),
            })
            .unwrap_err();
        assert_eq!(refusal.field, "zone");
        assert_eq!(zones.encode_state(), before, "nothing may have moved");
        assert_eq!(zones.serial(), 2, "and the serial has not moved either");
    }

    #[test]
    fn ungrouping_puts_a_zone_in_a_group_of_its_own() {
        let mut zones = two_zones();
        zones
            .apply(&Command::Group {
                zone: "kitchen".to_string(),
                group: "downstairs".to_string(),
            })
            .unwrap();
        assert_eq!(zones.zone("kitchen").unwrap().group, "downstairs");
        zones
            .apply(&Command::Ungroup {
                zone: "kitchen".to_string(),
            })
            .unwrap();
        assert_eq!(zones.zone("kitchen").unwrap().group, "kitchen");
    }

    #[test]
    fn a_muted_zone_has_a_gain_of_zero_and_keeps_the_volume_it_had() {
        let mut zones = two_zones();
        let half = Volume::from_thousandths(500).unwrap();
        zones
            .apply(&Command::Volume {
                zone: "kitchen".to_string(),
                volume: half,
            })
            .unwrap();
        zones
            .apply(&Command::Mute {
                zone: "kitchen".to_string(),
                muted: true,
            })
            .unwrap();
        let zone = zones.zone("kitchen").unwrap();
        assert_eq!(zone.gain(), Volume::SILENT);
        assert_eq!(zone.volume, half, "unmuting has to give the volume back");
    }

    #[test]
    fn an_endpoint_that_leaves_stays_in_the_membership_and_leaves_the_presence() {
        let mut zones = two_zones();
        zones
            .apply(&Command::Attach {
                zone: "kitchen".to_string(),
                endpoint: "endpoint-a".to_string(),
            })
            .unwrap();
        assert!(zones.endpoint_left("endpoint-a"));
        let zone = zones.zone("kitchen").unwrap();
        assert_eq!(zone.endpoints, vec!["endpoint-a".to_string()]);
        assert!(zone.present.is_empty());
    }

    #[test]
    fn a_group_plays_the_stream_it_is_pointed_at() {
        let mut zones = two_zones();
        zones.set_group_audio("downstairs", "127.0.0.1:4011");
        assert_eq!(zones.audio_for("downstairs"), "127.0.0.1:4011");
        assert_eq!(zones.audio_for("kitchen"), "127.0.0.1:4010");
    }
}
