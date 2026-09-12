//! The transport tier a zone is declared in, and what each tier is held to.
//!
//! # What this is not
//!
//! It is not part of the control catalog. `CATALOG_VERSION` does not move, no
//! message carries a transport, no vector under `fixtures/control/` changes a
//! byte, and `docs/control-plane.md`'s message table is unextended. A zone's
//! transport is declared where the set of zones is declared, on the server's
//! command line, because that document already states the set of zones is
//! configured and not commanded: the rooms in a house and the wires between
//! them are facts about a building, and a typo has to be a refusal rather than
//! a room nobody has or a tier nobody chose. `crates/control/tests/refusals.rs`
//! is where a control message that tries to change one is refused.
//!
//! # Why the numbers are compiled and committed
//!
//! The same arrangement `config/sync.conf` and `crates/client-linux/src/sync.rs`
//! already use, for the same reason: the shipped binaries carry the constants,
//! `config/transport.conf` carries them too, and a test asserts the file and the
//! compiled values agree. One copy of a number, checkable from either side.
//!
//! # None of this is a measurement
//!
//! A bound here is a TARGET a tier is held to, taken from BRIEF.md section 2.2.
//! Whether a wireless zone meets it is a measured distribution over a real
//! radio; that is operator graded, it is NOT passed in this repository, and
//! `docs/verification-record.md` says so in as many words.

use std::fmt;

use crate::zones::Zone;

/// Which transport a zone is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Transport {
    /// Copper. The tier the house is built on and the tier SYNC-4 measured.
    Wired,
    /// Wi-Fi. A convenience tier with a deeper buffer and a looser bound.
    Wireless,
}

/// Every transport a zone may be declared with, in the order they are reported.
///
/// `config/transport.conf` carries the same list under `transports`, and
/// `crates/server/tests/wireless_zones.rs` asserts the two agree.
pub const TRANSPORTS: &[Transport] = &[Transport::Wired, Transport::Wireless];

/// What a zone that declares no transport is.
///
/// Wired, deliberately. A zone nobody has thought about must not inherit the
/// looser tier by silence: an undeclared zone is held to the tighter bound,
/// which is the direction that fails safe.
pub const DEFAULT_TRANSPORT: Transport = Transport::Wired;

/// Inter-device error a wired zone is held to, in microseconds.
///
/// BRIEF.md section 2.2, "Stereo pair / same room, acceptable < 0.5 ms". This is
/// the bound SYNC-4 was written against and this phase does not move it.
pub const WIRED_BOUND_US: u64 = 500;

/// Inter-device error a wireless zone is held to, in microseconds.
///
/// BRIEF.md section 2.2, "Multiroom music, different rooms, acceptable < 5 ms".
pub const WIRELESS_BOUND_US: u64 = 5_000;

impl Transport {
    /// The word the committed configuration and every report use.
    pub fn name(self) -> &'static str {
        match self {
            Transport::Wired => "wired",
            Transport::Wireless => "wireless",
        }
    }

    /// Parse a declared transport, or `None` for a word this build does not
    /// have.
    pub fn parse(text: &str) -> Option<Transport> {
        TRANSPORTS.iter().copied().find(|t| t.name() == text)
    }

    /// The inter-device bound this tier is held to, in microseconds.
    pub fn bound_us(self) -> u64 {
        match self {
            Transport::Wired => WIRED_BOUND_US,
            Transport::Wireless => WIRELESS_BOUND_US,
        }
    }

    /// Every permitted transport, as a refusal names them.
    pub fn permitted() -> String {
        TRANSPORTS
            .iter()
            .map(|t| t.name())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl fmt::Display for Transport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The buffer policy a group held to the wireless tier applies.
///
/// Depth and latency, which is what BRIEF.md section 9's "bigger buffers" means
/// operationally. Deliberately not a memory placement: the endpoint tree
/// prohibits external-RAM placement outright, so where the bytes live is not
/// this policy's business.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WirelessPolicy {
    /// Smallest buffer occupancy a run is graded against, in microseconds.
    pub min_us: u64,
    /// Largest buffer occupancy a run is graded against, in microseconds.
    pub max_us: u64,
    /// Buffered audio accumulated before the first frame is written.
    pub start_fill_us: u64,
    /// The device-reported delay the playout loop holds.
    pub device_target_us: u64,
    /// The fixed end-to-end playout latency every endpoint in the group
    /// applies. Content due at t is audible at t + this, at every endpoint.
    pub playout_latency_us: u64,
}

/// The committed wireless policy.
///
/// Every value's provenance is in `docs/decisions/0021-the-wireless-tier.md`,
/// and `config/transport.conf` carries the same five numbers.
pub const WIRELESS_POLICY: WirelessPolicy = WirelessPolicy {
    min_us: 120_000,
    max_us: 900_000,
    start_fill_us: 400_000,
    device_target_us: 300_000,
    playout_latency_us: 500_000,
};

/// What a group is held to, and which zone's declaration decided it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupTier {
    /// The group.
    pub group: String,
    /// The transport the whole group is held to.
    pub transport: Transport,
    /// The bound that transport is held to, in microseconds.
    pub bound_us: u64,
    /// The zone whose declaration set it, where one did.
    ///
    /// A group is a unit a stream is served to and every endpoint in it plays
    /// the same timeline, so a group cannot be half wired: one wireless zone
    /// makes the whole group wireless. Naming the zone that did it is what
    /// turns "this group is on the loose bound" from a surprise into a fact
    /// somebody declared.
    pub set_by: Option<String>,
}

impl GroupTier {
    /// The buffer policy this group's endpoints apply, where it is the wireless
    /// one.
    pub fn wireless_policy(&self) -> Option<WirelessPolicy> {
        match self.transport {
            Transport::Wireless => Some(WIRELESS_POLICY),
            Transport::Wired => None,
        }
    }

    /// One line, as a server reports it at start.
    pub fn report(&self) -> String {
        let mut out = format!(
            "group id={} transport={} bound_us={} set_by={}",
            self.group,
            self.transport,
            self.bound_us,
            self.set_by.as_deref().unwrap_or("none")
        );
        match self.wireless_policy() {
            Some(policy) => out.push_str(&format!(
                " policy=wireless playout_latency_us={} min_us={} max_us={} start_fill_us={} \
                 device_target_us={}",
                policy.playout_latency_us,
                policy.min_us,
                policy.max_us,
                policy.start_fill_us,
                policy.device_target_us
            )),
            None => out.push_str(" policy=wired"),
        }
        out
    }
}

/// Which transport each zone was declared with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ZoneTransports {
    declared: Vec<(String, Transport)>,
}

impl ZoneTransports {
    /// From the server's own zone declarations, in the order they were given.
    pub fn new(declared: &[(String, Transport)]) -> ZoneTransports {
        ZoneTransports {
            declared: declared.to_vec(),
        }
    }

    /// The transport a zone was declared with, or the default for a zone that
    /// declared none.
    pub fn of(&self, zone: &str) -> Transport {
        self.declared
            .iter()
            .find(|(id, _)| id == zone)
            .map(|(_, t)| *t)
            .unwrap_or(DEFAULT_TRANSPORT)
    }

    /// One line per zone, as a server reports it at start, so that no zone's
    /// tier is implicit.
    pub fn report(&self, zone: &str) -> String {
        let transport = self.of(zone);
        format!(
            "zone id={} transport={} bound_us={}",
            zone,
            transport,
            transport.bound_us()
        )
    }

    /// What one group is held to.
    ///
    /// A group holding any wireless zone is wireless, and the zone named is the
    /// first one in configured order that made it so. First rather than any, so
    /// that two runs over the same declaration name the same zone.
    pub fn group_tier(&self, zones: &[Zone], group: &str) -> GroupTier {
        let set_by = zones
            .iter()
            .filter(|zone| zone.group == group)
            .find(|zone| self.of(&zone.id) == Transport::Wireless)
            .map(|zone| zone.id.clone());
        let transport = match set_by {
            Some(_) => Transport::Wireless,
            None => Transport::Wired,
        };
        GroupTier {
            group: group.to_string(),
            transport,
            bound_us: transport.bound_us(),
            set_by,
        }
    }

    /// Every group these zones are in, in the order the zones were configured.
    pub fn groups(zones: &[Zone]) -> Vec<String> {
        let mut groups: Vec<String> = Vec::new();
        for zone in zones {
            if !groups.iter().any(|g| g == &zone.group) {
                groups.push(zone.group.clone());
            }
        }
        groups
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_undeclared_zone_is_wired_and_held_to_the_tighter_bound() {
        let transports = ZoneTransports::new(&[("bedroom".to_string(), Transport::Wireless)]);
        assert_eq!(transports.of("kitchen"), Transport::Wired);
        assert_eq!(transports.of("kitchen").bound_us(), WIRED_BOUND_US);
        assert_eq!(transports.of("bedroom"), Transport::Wireless);
        assert_eq!(transports.of("bedroom").bound_us(), WIRELESS_BOUND_US);
        assert!(
            WIRELESS_BOUND_US > WIRED_BOUND_US,
            "the wireless tier is the looser one, or none of this means anything"
        );
    }

    #[test]
    fn only_a_word_the_committed_configuration_names_parses() {
        assert_eq!(Transport::parse("wired"), Some(Transport::Wired));
        assert_eq!(Transport::parse("wireless"), Some(Transport::Wireless));
        assert_eq!(Transport::parse("wifi"), None);
        assert_eq!(Transport::parse("Wireless"), None);
        assert_eq!(Transport::parse(""), None);
        assert_eq!(Transport::permitted(), "wired, wireless");
    }

    #[test]
    fn one_wireless_zone_makes_the_whole_group_wireless_and_says_which() {
        let mut kitchen = Zone::new("kitchen");
        kitchen.group = "downstairs".to_string();
        let mut bedroom = Zone::new("bedroom");
        bedroom.group = "downstairs".to_string();
        let study = Zone::new("study");
        let zones = vec![kitchen, bedroom, study];

        let transports = ZoneTransports::new(&[
            ("kitchen".to_string(), Transport::Wired),
            ("bedroom".to_string(), Transport::Wireless),
            ("study".to_string(), Transport::Wired),
        ]);

        let downstairs = transports.group_tier(&zones, "downstairs");
        assert_eq!(downstairs.transport, Transport::Wireless);
        assert_eq!(downstairs.bound_us, WIRELESS_BOUND_US);
        assert_eq!(downstairs.set_by.as_deref(), Some("bedroom"));
        assert!(downstairs.wireless_policy().is_some());

        let alone = transports.group_tier(&zones, "study");
        assert_eq!(alone.transport, Transport::Wired);
        assert_eq!(alone.bound_us, WIRED_BOUND_US);
        assert_eq!(alone.set_by, None);
        assert!(alone.wireless_policy().is_none());
    }

    #[test]
    fn the_group_report_names_the_zone_and_the_policy() {
        let mut bedroom = Zone::new("bedroom");
        bedroom.group = "downstairs".to_string();
        let zones = vec![bedroom];
        let transports = ZoneTransports::new(&[("bedroom".to_string(), Transport::Wireless)]);
        let line = transports.group_tier(&zones, "downstairs").report();
        assert!(line.contains("transport=wireless"), "{}", line);
        assert!(line.contains("bound_us=5000"), "{}", line);
        assert!(line.contains("set_by=bedroom"), "{}", line);
        assert!(line.contains("playout_latency_us=500000"), "{}", line);
    }

    #[test]
    fn the_groups_are_listed_in_the_order_the_zones_were_configured() {
        let mut kitchen = Zone::new("kitchen");
        kitchen.group = "downstairs".to_string();
        let mut bedroom = Zone::new("bedroom");
        bedroom.group = "downstairs".to_string();
        let study = Zone::new("study");
        let groups = ZoneTransports::groups(&[kitchen, bedroom, study]);
        assert_eq!(groups, vec!["downstairs".to_string(), "study".to_string()]);
    }
}
