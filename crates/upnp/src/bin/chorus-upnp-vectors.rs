//! Writes the generated documents among the UPnP golden vectors:
//! `make upnp-vectors` (`fixtures/README.md`). The device description and
//! the three service descriptions are long and are written from the tables
//! in `chorus_upnp::description`, so they are regenerated here rather than
//! typed; every other vector under `fixtures/upnp` was written by hand from
//! the specifications and this program does not touch it.

use chorus_upnp::description::{device_config_id, device_description, scpd, DeviceInfo};
use chorus_upnp::uuid::{udn, Target, CHORUS_NAMESPACE};
use chorus_upnp::Service;
use std::path::Path;

fn main() {
    let Some(dir) = std::env::args().nth(1) else {
        eprintln!("usage: chorus-upnp-vectors <fixtures/upnp>");
        std::process::exit(2);
    };
    let dir = Path::new(&dir);
    let info = DeviceInfo {
        udn: udn(&CHORUS_NAMESPACE, "srv1", &Target::Room("kitchen")),
        friendly_name: "Kitchen".to_string(),
        model_name: "chorus room".to_string(),
        model_number: "0.1.0".to_string(),
    };
    let config_id = device_config_id(&info);
    let mut files = vec![(
        "description-room.xml".to_string(),
        device_description(&info, config_id),
    )];
    for service in Service::ALL {
        files.push((
            format!("scpd-{}.xml", service.path()),
            scpd(service, config_id),
        ));
    }
    for (name, text) in files {
        let path = dir.join(&name);
        if let Err(e) = std::fs::write(&path, format!("{text}\n")) {
            eprintln!("{}: {e}", path.display());
            std::process::exit(1);
        }
        println!("{}", path.display());
    }
}
