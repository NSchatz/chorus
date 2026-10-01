//! Which group plays on which stream slot, planned before a change commits.
//!
//! # The rule
//!
//! A group NEEDS a slot when it is formed (at least one room is in it) and
//! its source is not `none`. Every group that needs one has one, always: a
//! change to the room model (a command, a controller's button, the runtime) is
//! applied to a copy, the table is planned over the copy, and only when the
//! plan fits in the server's `--slots S` is either installed. A change that
//! would need one slot more is refused by name, with nothing applied and
//! nothing persisted: the same whole-or-nothing rule `Zones::apply` keeps
//! (`chorus_control::zones`).
//!
//! # Sticky, lowest first
//!
//! A group keeps the slot it has for as long as it needs one, so its rooms'
//! sessions are not moved for a change that did not concern them. A slot is
//! freed when its group stops needing it (dissolved, or its source set to
//! `none`) BEFORE new groups are placed, and a new group takes the lowest free
//! slot. So a room joining another's group, which retires one group id and
//! forms another, usually lands the new group on the slot the old one held.
//!
//! Pure: arithmetic on the room model it is handed. No clock, no PCM, no
//! socket.

use chorus_control::catalog::Refusal;
use chorus_control::rooms::Source;
use chorus_control::zones::Zones;

/// The slot each group holds, by slot index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotTable {
    held: Vec<Option<String>>,
}

impl SlotTable {
    /// `slots` free slots.
    pub fn new(slots: usize) -> SlotTable {
        SlotTable {
            held: vec![None; slots],
        }
    }

    /// How many slots there are.
    pub fn len(&self) -> usize {
        self.held.len()
    }

    /// Whether there are none (the one-stream shape has no table at all).
    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }

    /// The group on each slot.
    pub fn held(&self) -> &[Option<String>] {
        &self.held
    }

    /// The slot a group holds.
    pub fn slot_of(&self, group: &str) -> Option<usize> {
        self.held.iter().position(|g| g.as_deref() == Some(group))
    }

    /// Every formed group that needs a slot, in the order its first room was
    /// configured.
    pub fn needing(zones: &Zones) -> Vec<String> {
        zones
            .formed_groups()
            .into_iter()
            .map(|g| g.id)
            .filter(|g| zones.source(g) != Source::None)
            .collect()
    }

    /// The table `zones` needs, from this one: or a refusal naming the
    /// ceiling and the groups holding it when it does not fit.
    pub fn plan(&self, zones: &Zones) -> Result<SlotTable, Refusal> {
        let needing = SlotTable::needing(zones);
        let mut next = self.clone();
        for held in next.held.iter_mut() {
            if held.as_ref().is_some_and(|g| !needing.contains(g)) {
                *held = None;
            }
        }
        let mut unplaced = Vec::new();
        for group in &needing {
            if next.slot_of(group).is_some() {
                continue;
            }
            match next.held.iter().position(Option::is_none) {
                Some(free) => next.held[free] = Some(group.clone()),
                None => unplaced.push(group.clone()),
            }
        }
        if unplaced.is_empty() {
            return Ok(next);
        }
        let holding: Vec<String> = next.held.iter().flatten().cloned().collect();
        Err(Refusal::rejected(
            "target",
            format!(
                "every one of this server's {} stream slots is in use (groups {}), and this \
                 change needs one more for {}; free one (set a group's source to none, or group \
                 rooms together) or start the server with --slots {}",
                self.held.len(),
                holding.join(", "),
                unplaced.join(", "),
                self.held.len() + unplaced.len()
            ),
        ))
    }

    /// `slot=group` for every slot, `-` for a free one, for a status line.
    pub fn report(&self) -> String {
        self.held
            .iter()
            .enumerate()
            .map(|(i, g)| format!("{}={}", i, g.as_deref().unwrap_or("-")))
            .collect::<Vec<_>>()
            .join(",")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chorus_control::catalog::decode_message;
    use chorus_control::zones::Zone;

    fn house(rooms: &[&str]) -> Zones {
        let mut zones = Zones::new("127.0.0.1:4010");
        for r in rooms {
            zones.add(Zone::new(r)).unwrap();
        }
        zones
    }

    fn apply(zones: &mut Zones, text: &str) {
        let (_, command) = decode_message(text).expect("a command");
        zones.apply(&command).expect("it applies");
    }

    #[test]
    fn every_group_that_plays_has_a_slot_and_keeps_it() {
        let mut zones = house(&["a", "b", "c"]);
        let table = SlotTable::new(3).plan(&zones).unwrap();
        assert_eq!(table.report(), "0=a,1=b,2=c");
        apply(&mut zones, r#"{"v":2,"t":"join","zone":"c","target":"b"}"#);
        let table = table.plan(&zones).unwrap();
        // b and c form a live group; b's group dissolved, freeing slot 1 first.
        assert_eq!(
            table.slot_of("a"),
            Some(0),
            "a was not concerned and did not move"
        );
        assert_eq!(table.held().iter().flatten().count(), 2);
    }

    #[test]
    fn a_change_that_needs_one_slot_more_is_refused_by_name() {
        let mut zones = house(&["a", "b", "c"]);
        apply(
            &mut zones,
            r#"{"v":2,"t":"take","target":"c","source":"none"}"#,
        );
        let table = SlotTable::new(2).plan(&zones).unwrap();
        assert_eq!(
            table.slot_of("c"),
            None,
            "a group with source none needs no slot"
        );
        apply(
            &mut zones,
            r#"{"v":2,"t":"take","target":"c","source":"stream"}"#,
        );
        let refusal = table.plan(&zones).unwrap_err();
        assert_eq!(refusal.field, "target");
        assert!(
            refusal
                .detail
                .contains("every one of this server's 2 stream slots is in use")
                && refusal.detail.contains("groups a, b")
                && refusal.detail.contains("--slots 3"),
            "{}",
            refusal.detail
        );
    }
}
