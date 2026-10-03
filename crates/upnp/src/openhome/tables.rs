//! The OpenHome service tables: the actions and state variables of
//! Product:2, Volume:2, Info:1, Time:1 and Playlist:1, in the form
//! [`crate::description`] writes SCPDs from and [`crate::soap::validate`]
//! holds requests to.
//!
//! Transcribed from the service XMLs of ohPipeline (MIT) at commit
//! `cccd06dd49ab154f43e1f24e009fe066a14bf15f`, directory
//! `OpenHome/Av/ServiceXml/OpenHome/`: `Product2.xml`, `Volume2.xml`,
//! `Info1.xml`, `Time1.xml`, `Playlist1.xml` (read 2026-10-03). Action
//! order, argument order, related variables, data types and `sendEvents`
//! are the XMLs'. One oddity is the XML's own and kept: `Product`'s `Source`
//! action relates its `SystemName` out argument to `SourceName`.

use crate::description::{Action, Allowed, Arg, Dir, ServiceTable, StateVar};
use crate::Service;

const fn i(name: &'static str, var: &'static str) -> Arg {
    Arg {
        name,
        dir: Dir::In,
        var,
    }
}

const fn o(name: &'static str, var: &'static str) -> Arg {
    Arg {
        name,
        dir: Dir::Out,
        var,
    }
}

const fn var(name: &'static str, data_type: &'static str, evented: bool) -> StateVar {
    StateVar {
        name,
        data_type,
        evented,
        allowed: Allowed::Any,
    }
}

/// Product:2 (`Product2.xml`).
pub const PRODUCT: ServiceTable = ServiceTable {
    service: Service::Product,
    actions: &[
        Action {
            name: "Manufacturer",
            args: &[
                o("Name", "ManufacturerName"),
                o("Info", "ManufacturerInfo"),
                o("Url", "ManufacturerUrl"),
                o("ImageUri", "ManufacturerImageUri"),
            ],
        },
        Action {
            name: "Model",
            args: &[
                o("Name", "ModelName"),
                o("Info", "ModelInfo"),
                o("Url", "ModelUrl"),
                o("ImageUri", "ModelImageUri"),
            ],
        },
        Action {
            name: "Product",
            args: &[
                o("Room", "ProductRoom"),
                o("Name", "ProductName"),
                o("Info", "ProductInfo"),
                o("Url", "ProductUrl"),
                o("ImageUri", "ProductImageUri"),
            ],
        },
        Action {
            name: "Standby",
            args: &[o("Value", "Standby")],
        },
        Action {
            name: "SetStandby",
            args: &[i("Value", "Standby")],
        },
        Action {
            name: "SourceCount",
            args: &[o("Value", "SourceCount")],
        },
        Action {
            name: "SourceXml",
            args: &[o("Value", "SourceXml")],
        },
        Action {
            name: "SourceIndex",
            args: &[o("Value", "SourceIndex")],
        },
        Action {
            name: "SetSourceIndex",
            args: &[i("Value", "SourceIndex")],
        },
        Action {
            name: "SetSourceIndexByName",
            args: &[i("Value", "SourceName")],
        },
        Action {
            name: "SetSourceBySystemName",
            args: &[i("Value", "SourceSystemName")],
        },
        Action {
            name: "Source",
            args: &[
                i("Index", "SourceIndex"),
                o("SystemName", "SourceName"),
                o("Type", "SourceType"),
                o("Name", "SourceName"),
                o("Visible", "SourceVisible"),
            ],
        },
        Action {
            name: "Attributes",
            args: &[o("Value", "Attributes")],
        },
        Action {
            name: "SourceXmlChangeCount",
            args: &[o("Value", "SourceXmlChangeCount")],
        },
    ],
    variables: &[
        var("ManufacturerName", "string", true),
        var("ManufacturerInfo", "string", true),
        var("ManufacturerUrl", "string", true),
        var("ManufacturerImageUri", "string", true),
        var("ModelName", "string", true),
        var("ModelInfo", "string", true),
        var("ModelUrl", "string", true),
        var("ModelImageUri", "string", true),
        var("ProductRoom", "string", true),
        var("ProductName", "string", true),
        var("ProductInfo", "string", true),
        var("ProductUrl", "string", true),
        var("ProductImageUri", "string", true),
        var("Standby", "boolean", true),
        var("SourceIndex", "ui4", true),
        var("SourceCount", "ui4", true),
        var("SourceXml", "string", true),
        var("Attributes", "string", true),
        var("SourceXmlChangeCount", "ui4", false),
        var("SourceType", "string", false),
        var("SourceName", "string", false),
        var("SourceSystemName", "string", false),
        var("SourceVisible", "boolean", false),
    ],
};

/// Volume:2 (`Volume2.xml`).
pub const VOLUME: ServiceTable = ServiceTable {
    service: Service::Volume,
    actions: &[
        Action {
            name: "Characteristics",
            args: &[
                o("VolumeMax", "VolumeMax"),
                o("VolumeUnity", "VolumeUnity"),
                o("VolumeSteps", "VolumeSteps"),
                o("VolumeMilliDbPerStep", "VolumeMilliDbPerStep"),
                o("BalanceMax", "BalanceMax"),
                o("FadeMax", "FadeMax"),
            ],
        },
        Action {
            name: "SetVolume",
            args: &[i("Value", "Volume")],
        },
        Action {
            name: "VolumeInc",
            args: &[],
        },
        Action {
            name: "VolumeDec",
            args: &[],
        },
        Action {
            name: "Volume",
            args: &[o("Value", "Volume")],
        },
        Action {
            name: "SetBalance",
            args: &[i("Value", "Balance")],
        },
        Action {
            name: "BalanceInc",
            args: &[],
        },
        Action {
            name: "BalanceDec",
            args: &[],
        },
        Action {
            name: "Balance",
            args: &[o("Value", "Balance")],
        },
        Action {
            name: "SetFade",
            args: &[i("Value", "Fade")],
        },
        Action {
            name: "FadeInc",
            args: &[],
        },
        Action {
            name: "FadeDec",
            args: &[],
        },
        Action {
            name: "Fade",
            args: &[o("Value", "Fade")],
        },
        Action {
            name: "SetMute",
            args: &[i("Value", "Mute")],
        },
        Action {
            name: "Mute",
            args: &[o("Value", "Mute")],
        },
        Action {
            name: "VolumeLimit",
            args: &[o("Value", "VolumeLimit")],
        },
        Action {
            name: "UnityGain",
            args: &[o("Value", "UnityGain")],
        },
    ],
    variables: &[
        var("Volume", "ui4", true),
        var("Mute", "boolean", true),
        var("Balance", "i4", true),
        var("Fade", "i4", true),
        var("VolumeLimit", "ui4", true),
        var("VolumeMax", "ui4", true),
        var("VolumeUnity", "ui4", true),
        var("VolumeSteps", "ui4", true),
        var("VolumeMilliDbPerStep", "ui4", true),
        var("BalanceMax", "ui4", true),
        var("FadeMax", "ui4", true),
        var("UnityGain", "boolean", true),
    ],
};

/// Info:1 (`Info1.xml`).
pub const INFO: ServiceTable = ServiceTable {
    service: Service::Info,
    actions: &[
        Action {
            name: "Counters",
            args: &[
                o("TrackCount", "TrackCount"),
                o("DetailsCount", "DetailsCount"),
                o("MetatextCount", "MetatextCount"),
            ],
        },
        Action {
            name: "Track",
            args: &[o("Uri", "Uri"), o("Metadata", "Metadata")],
        },
        Action {
            name: "Details",
            args: &[
                o("Duration", "Duration"),
                o("BitRate", "BitRate"),
                o("BitDepth", "BitDepth"),
                o("SampleRate", "SampleRate"),
                o("Lossless", "Lossless"),
                o("CodecName", "CodecName"),
            ],
        },
        Action {
            name: "Metatext",
            args: &[o("Value", "Metatext")],
        },
    ],
    variables: &[
        var("TrackCount", "ui4", true),
        var("DetailsCount", "ui4", true),
        var("MetatextCount", "ui4", true),
        var("Uri", "string", true),
        var("Metadata", "string", true),
        var("Duration", "ui4", true),
        var("BitRate", "ui4", true),
        var("BitDepth", "ui4", true),
        var("SampleRate", "ui4", true),
        var("Lossless", "boolean", true),
        var("CodecName", "string", true),
        var("Metatext", "string", true),
    ],
};

/// Time:1 (`Time1.xml`).
pub const TIME: ServiceTable = ServiceTable {
    service: Service::Time,
    actions: &[Action {
        name: "Time",
        args: &[
            o("TrackCount", "TrackCount"),
            o("Duration", "Duration"),
            o("Seconds", "Seconds"),
        ],
    }],
    variables: &[
        var("TrackCount", "ui4", true),
        var("Duration", "ui4", true),
        var("Seconds", "ui4", true),
    ],
};

/// Playlist:1 (`Playlist1.xml`).
pub const PLAYLIST: ServiceTable = ServiceTable {
    service: Service::Playlist,
    actions: &[
        Action {
            name: "Play",
            args: &[],
        },
        Action {
            name: "Pause",
            args: &[],
        },
        Action {
            name: "Stop",
            args: &[],
        },
        Action {
            name: "Next",
            args: &[],
        },
        Action {
            name: "Previous",
            args: &[],
        },
        Action {
            name: "SetRepeat",
            args: &[i("Value", "Repeat")],
        },
        Action {
            name: "Repeat",
            args: &[o("Value", "Repeat")],
        },
        Action {
            name: "SetShuffle",
            args: &[i("Value", "Shuffle")],
        },
        Action {
            name: "Shuffle",
            args: &[o("Value", "Shuffle")],
        },
        Action {
            name: "SeekSecondAbsolute",
            args: &[i("Value", "Absolute")],
        },
        Action {
            name: "SeekSecondRelative",
            args: &[i("Value", "Relative")],
        },
        Action {
            name: "SeekId",
            args: &[i("Value", "Id")],
        },
        Action {
            name: "SeekIndex",
            args: &[i("Value", "Index")],
        },
        Action {
            name: "TransportState",
            args: &[o("Value", "TransportState")],
        },
        Action {
            name: "Id",
            args: &[o("Value", "Id")],
        },
        Action {
            name: "Read",
            args: &[i("Id", "Id"), o("Uri", "Uri"), o("Metadata", "Metadata")],
        },
        Action {
            name: "ReadList",
            args: &[i("IdList", "IdList"), o("TrackList", "TrackList")],
        },
        Action {
            name: "Insert",
            args: &[
                i("AfterId", "Id"),
                i("Uri", "Uri"),
                i("Metadata", "Metadata"),
                o("NewId", "Id"),
            ],
        },
        Action {
            name: "DeleteId",
            args: &[i("Value", "Id")],
        },
        Action {
            name: "DeleteAll",
            args: &[],
        },
        Action {
            name: "TracksMax",
            args: &[o("Value", "TracksMax")],
        },
        Action {
            name: "IdArray",
            args: &[o("Token", "IdArrayToken"), o("Array", "IdArray")],
        },
        Action {
            name: "IdArrayChanged",
            args: &[i("Token", "IdArrayToken"), o("Value", "IdArrayChanged")],
        },
        Action {
            name: "ProtocolInfo",
            args: &[o("Value", "ProtocolInfo")],
        },
    ],
    variables: &[
        StateVar {
            name: "TransportState",
            data_type: "string",
            evented: true,
            allowed: Allowed::List(&["Playing", "Paused", "Stopped", "Buffering"]),
        },
        var("Repeat", "boolean", true),
        var("Shuffle", "boolean", true),
        var("Id", "ui4", true),
        var("IdArray", "bin.base64", true),
        var("TracksMax", "ui4", true),
        var("ProtocolInfo", "string", true),
        var("Index", "ui4", false),
        var("Relative", "i4", false),
        var("Absolute", "ui4", false),
        var("IdList", "string", false),
        var("TrackList", "string", false),
        var("Uri", "string", false),
        var("Metadata", "string", false),
        var("IdArrayToken", "ui4", false),
        var("IdArrayChanged", "boolean", false),
    ],
};

/// The table of an OpenHome service; `None` for an AV one.
pub fn table(service: Service) -> Option<&'static ServiceTable> {
    match service {
        Service::Product => Some(&PRODUCT),
        Service::Volume => Some(&VOLUME),
        Service::Info => Some(&INFO),
        Service::Time => Some(&TIME),
        Service::Playlist => Some(&PLAYLIST),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn names(t: &ServiceTable) -> Vec<&'static str> {
        t.actions.iter().map(|a| a.name).collect()
    }

    fn evented(t: &ServiceTable) -> Vec<&'static str> {
        t.variables
            .iter()
            .filter(|v| v.evented)
            .map(|v| v.name)
            .collect()
    }

    /// The counts of the XMLs (the digest's appendix, generated from them).
    #[test]
    fn the_tables_have_the_actions_and_variables_of_the_service_xmls() {
        let counts: Vec<(usize, usize, usize)> = Service::OPENHOME
            .into_iter()
            .map(|s| {
                let t = table(s).unwrap();
                (t.actions.len(), t.variables.len(), evented(t).len())
            })
            .collect();
        assert_eq!(
            counts,
            [
                (14, 23, 18),
                (17, 12, 12),
                (4, 12, 12),
                (1, 3, 3),
                (24, 16, 7)
            ]
        );
        assert_eq!(
            names(&PRODUCT),
            [
                "Manufacturer",
                "Model",
                "Product",
                "Standby",
                "SetStandby",
                "SourceCount",
                "SourceXml",
                "SourceIndex",
                "SetSourceIndex",
                "SetSourceIndexByName",
                "SetSourceBySystemName",
                "Source",
                "Attributes",
                "SourceXmlChangeCount"
            ]
        );
        assert_eq!(
            evented(&PLAYLIST),
            [
                "TransportState",
                "Repeat",
                "Shuffle",
                "Id",
                "IdArray",
                "TracksMax",
                "ProtocolInfo"
            ]
        );
        assert_eq!(
            PLAYLIST.variable("IdArray").unwrap().data_type,
            "bin.base64"
        );
        // What goal 17 leaves out is not in any table (K64).
        for t in [&PRODUCT, &VOLUME, &INFO, &TIME, &PLAYLIST] {
            for absent in ["DeleteMultiple", "Move", "StandbyTransitioning", "PlayAs"] {
                assert!(t.action(absent).is_none(), "{absent}");
            }
        }
    }

    #[test]
    fn the_tables_are_self_consistent() {
        for service in Service::OPENHOME {
            let t = table(service).unwrap();
            assert_eq!(t.service, service);
            let vars: BTreeSet<&str> = t.variables.iter().map(|v| v.name).collect();
            assert_eq!(vars.len(), t.variables.len());
            let actions: BTreeSet<&str> = t.actions.iter().map(|a| a.name).collect();
            assert_eq!(actions.len(), t.actions.len());
            for action in t.actions {
                let mut seen_out = false;
                let mut arg_names = BTreeSet::new();
                for arg in action.args {
                    assert!(vars.contains(arg.var), "{}.{}", action.name, arg.name);
                    assert!(arg_names.insert(arg.name), "{}.{}", action.name, arg.name);
                    match arg.dir {
                        Dir::Out => seen_out = true,
                        Dir::In => assert!(!seen_out, "{}", action.name),
                    }
                }
            }
        }
        assert!(Service::AV.into_iter().all(|s| table(s).is_none()));
    }
}
