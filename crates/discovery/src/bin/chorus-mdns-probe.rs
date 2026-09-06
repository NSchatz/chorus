//! Can this host do multicast DNS at all?
//!
//! Exits 0 when UDP port 5353 can be bound and the group 224.0.0.251 joined,
//! and non-zero naming what failed otherwise. Nothing else: it opens the
//! socket, says what happened and closes it.
//!
//! It exists so that `tools/mdns-live-run.sh` can refuse BY NAME on a link that
//! does not carry multicast, rather than running an exchange that finds nothing
//! and reporting that as "no server". Those are two different facts and the
//! second one is what the static fallback exists for; conflating them is how a
//! fallback comes to look as though it had worked.
//!
//!     chorus-mdns-probe

use std::net::{IpAddr, Ipv4Addr};
use std::process::ExitCode;

use chorus_discovery::dnssd::{Advertisement, AUDIO_SERVICE, MDNS_GROUP_V4, MDNS_PORT};
use chorus_discovery::net::Advertiser;

fn main() -> ExitCode {
    let probe = Advertisement {
        instance: "chorus-probe".to_string(),
        service: AUDIO_SERVICE.to_string(),
        host: "chorus-probe.local.".to_string(),
        port: 0,
        addresses: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
        txt: vec![("probe".to_string(), "1".to_string())],
    };
    match Advertiser::open(vec![probe]) {
        Ok(_) => {
            println!(
                "chorus-mdns-probe: usable=1 port={} group={}",
                MDNS_PORT, MDNS_GROUP_V4
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("chorus-mdns-probe: usable=0 detail={}", e);
            println!(
                "chorus-mdns-probe: usable=0 port={} group={}",
                MDNS_PORT, MDNS_GROUP_V4
            );
            ExitCode::from(3)
        }
    }
}
