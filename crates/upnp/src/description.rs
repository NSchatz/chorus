//! Description: the device description and the three service descriptions
//! (SCPDs), generated from one table.
//!
//! UDA11 section 2. The [`ServiceTable`]s below are the single statement of
//! what chorus's three services offer: the SCPD text a control point
//! fetches is written from them ([`scpd`]), the SOAP layer validates an
//! action's arguments against them ([`crate::soap::validate`]), the state
//! machines answer in their argument order, and the tests hold them to the
//! specifications. A control point decides what a renderer can do from the
//! SCPD (whether there is a `Pause`, a `SetNextAVTransportURI`, a `Volume`
//! variable, `REL_TIME` among the seek modes), so the tables list exactly
//! what is implemented and nothing else.
//!
//! All URLs in a description are relative paths with a leading slash and no
//! host: UDA11 section 2.3 says they "MUST be relative to the URL at which
//! the device description is located", and a path is right on every
//! interface the server has. There is no `URLBase` ("UPnP 1.1 devices MUST
//! NOT include URLBase").

use crate::openhome::tables;
use crate::ssdp;
use crate::uuid::Uuid;
use crate::xml::escape_text;
use crate::{Service, DEVICE_TYPE};

/// Whether an argument goes in or comes out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    /// An input argument.
    In,
    /// An output argument.
    Out,
}

/// One argument of an action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Arg {
    /// The argument's name.
    pub name: &'static str,
    /// Its direction.
    pub dir: Dir,
    /// Its `relatedStateVariable`, which carries its type.
    pub var: &'static str,
}

/// One action: its name and its arguments, inputs first (UDA11 section 2.5:
/// "Input arguments MUST be listed first").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Action {
    /// The action's name.
    pub name: &'static str,
    /// Its arguments in SCPD order.
    pub args: &'static [Arg],
}

impl Action {
    /// The input arguments in order.
    pub fn inputs(&self) -> impl Iterator<Item = &'static Arg> {
        self.args.iter().filter(|a| a.dir == Dir::In)
    }

    /// The output arguments in order.
    pub fn outputs(&self) -> impl Iterator<Item = &'static Arg> {
        self.args.iter().filter(|a| a.dir == Dir::Out)
    }
}

/// What values a state variable may take, as the SCPD says it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Allowed {
    /// No list and no range.
    Any,
    /// An `allowedValueList`.
    List(&'static [&'static str]),
    /// An `allowedValueRange` with its step.
    Range {
        /// `minimum`.
        min: i64,
        /// `maximum`.
        max: i64,
        /// `step`.
        step: i64,
    },
}

/// One state variable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StateVar {
    /// The variable's name.
    pub name: &'static str,
    /// Its UPnP data type (`string`, `ui4`, `i4`, `ui2`, `boolean`).
    pub data_type: &'static str,
    /// Whether it is evented directly (`sendEvents="yes"`).
    pub evented: bool,
    /// Its allowed values.
    pub allowed: Allowed,
}

/// Everything one service offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServiceTable {
    /// Which service.
    pub service: Service,
    /// Its actions in SCPD order.
    pub actions: &'static [Action],
    /// Its state variables in SCPD order.
    pub variables: &'static [StateVar],
}

impl ServiceTable {
    /// The action of that name.
    pub fn action(&self, name: &str) -> Option<&'static Action> {
        self.actions.iter().find(|a| a.name == name)
    }

    /// The state variable of that name.
    pub fn variable(&self, name: &str) -> Option<&'static StateVar> {
        self.variables.iter().find(|v| v.name == name)
    }
}

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

const fn v(name: &'static str, data_type: &'static str, allowed: Allowed) -> StateVar {
    StateVar {
        name,
        data_type,
        evented: false,
        allowed,
    }
}

const fn evented(name: &'static str) -> StateVar {
    StateVar {
        name,
        data_type: "string",
        evented: true,
        allowed: Allowed::Any,
    }
}

/// Every action's first input (AVT1 section 2.2.30, RCS1 section 2.2.20).
const ID: Arg = i("InstanceID", "A_ARG_TYPE_InstanceID");

/// AVTransport:1 as chorus offers it. AVT1 section 2.4, table 3: every
/// required action (SetAVTransportURI, GetMediaInfo, GetTransportInfo,
/// GetPositionInfo, GetDeviceCapabilities, GetTransportSettings, Stop, Play,
/// Seek, Next, Previous) and the optional ones chorus implements
/// (SetNextAVTransportURI, Pause, GetCurrentTransportActions). Record,
/// SetPlayMode and SetRecordQualityMode are optional and absent. Arguments
/// and their related variables are AVT1 tables 4 to 20.
pub const AVTRANSPORT: ServiceTable = ServiceTable {
    service: Service::AvTransport,
    actions: &[
        Action {
            name: "SetAVTransportURI",
            args: &[
                ID,
                i("CurrentURI", "AVTransportURI"),
                i("CurrentURIMetaData", "AVTransportURIMetaData"),
            ],
        },
        Action {
            name: "SetNextAVTransportURI",
            args: &[
                ID,
                i("NextURI", "NextAVTransportURI"),
                i("NextURIMetaData", "NextAVTransportURIMetaData"),
            ],
        },
        Action {
            name: "GetMediaInfo",
            args: &[
                ID,
                o("NrTracks", "NumberOfTracks"),
                o("MediaDuration", "CurrentMediaDuration"),
                o("CurrentURI", "AVTransportURI"),
                o("CurrentURIMetaData", "AVTransportURIMetaData"),
                o("NextURI", "NextAVTransportURI"),
                o("NextURIMetaData", "NextAVTransportURIMetaData"),
                o("PlayMedium", "PlaybackStorageMedium"),
                o("RecordMedium", "RecordStorageMedium"),
                o("WriteStatus", "RecordMediumWriteStatus"),
            ],
        },
        Action {
            name: "GetTransportInfo",
            args: &[
                ID,
                o("CurrentTransportState", "TransportState"),
                o("CurrentTransportStatus", "TransportStatus"),
                o("CurrentSpeed", "TransportPlaySpeed"),
            ],
        },
        Action {
            name: "GetPositionInfo",
            args: &[
                ID,
                o("Track", "CurrentTrack"),
                o("TrackDuration", "CurrentTrackDuration"),
                o("TrackMetaData", "CurrentTrackMetaData"),
                o("TrackURI", "CurrentTrackURI"),
                o("RelTime", "RelativeTimePosition"),
                o("AbsTime", "AbsoluteTimePosition"),
                o("RelCount", "RelativeCounterPosition"),
                o("AbsCount", "AbsoluteCounterPosition"),
            ],
        },
        Action {
            name: "GetDeviceCapabilities",
            args: &[
                ID,
                o("PlayMedia", "PossiblePlaybackStorageMedia"),
                o("RecMedia", "PossibleRecordStorageMedia"),
                o("RecQualityModes", "PossibleRecordQualityModes"),
            ],
        },
        Action {
            name: "GetTransportSettings",
            args: &[
                ID,
                o("PlayMode", "CurrentPlayMode"),
                o("RecQualityMode", "CurrentRecordQualityMode"),
            ],
        },
        Action {
            name: "Stop",
            args: &[ID],
        },
        Action {
            name: "Play",
            args: &[ID, i("Speed", "TransportPlaySpeed")],
        },
        Action {
            name: "Pause",
            args: &[ID],
        },
        Action {
            name: "Seek",
            args: &[
                ID,
                i("Unit", "A_ARG_TYPE_SeekMode"),
                i("Target", "A_ARG_TYPE_SeekTarget"),
            ],
        },
        Action {
            name: "Next",
            args: &[ID],
        },
        Action {
            name: "Previous",
            args: &[ID],
        },
        Action {
            name: "GetCurrentTransportActions",
            args: &[ID, o("Actions", "CurrentTransportActions")],
        },
    ],
    // AVT1 section 2.2, table 1. The allowed value lists hold only what
    // chorus uses: the recording states are absent from TransportState, the
    // play mode is NORMAL alone, the one speed is "1" (AVT1 section 2.2.8),
    // and the seek modes are the required TRACK_NR plus REL_TIME (AVT1
    // section 2.2.28: "Only value 'TRACK_NR' is required").
    variables: &[
        v(
            "TransportState",
            "string",
            Allowed::List(&[
                "STOPPED",
                "PLAYING",
                "TRANSITIONING",
                "PAUSED_PLAYBACK",
                "NO_MEDIA_PRESENT",
            ]),
        ),
        v(
            "TransportStatus",
            "string",
            Allowed::List(&["OK", "ERROR_OCCURRED"]),
        ),
        v(
            "PlaybackStorageMedium",
            "string",
            Allowed::List(&["NONE", "NETWORK"]),
        ),
        v(
            "RecordStorageMedium",
            "string",
            Allowed::List(&["NOT_IMPLEMENTED"]),
        ),
        v("PossiblePlaybackStorageMedia", "string", Allowed::Any),
        v("PossibleRecordStorageMedia", "string", Allowed::Any),
        v("CurrentPlayMode", "string", Allowed::List(&["NORMAL"])),
        v("TransportPlaySpeed", "string", Allowed::List(&["1"])),
        v(
            "RecordMediumWriteStatus",
            "string",
            Allowed::List(&["NOT_IMPLEMENTED"]),
        ),
        v(
            "CurrentRecordQualityMode",
            "string",
            Allowed::List(&["NOT_IMPLEMENTED"]),
        ),
        v("PossibleRecordQualityModes", "string", Allowed::Any),
        v(
            "NumberOfTracks",
            "ui4",
            Allowed::Range {
                min: 0,
                max: 1,
                step: 1,
            },
        ),
        v(
            "CurrentTrack",
            "ui4",
            Allowed::Range {
                min: 0,
                max: 1,
                step: 1,
            },
        ),
        v("CurrentTrackDuration", "string", Allowed::Any),
        v("CurrentMediaDuration", "string", Allowed::Any),
        v("CurrentTrackMetaData", "string", Allowed::Any),
        v("CurrentTrackURI", "string", Allowed::Any),
        v("AVTransportURI", "string", Allowed::Any),
        v("AVTransportURIMetaData", "string", Allowed::Any),
        v("NextAVTransportURI", "string", Allowed::Any),
        v("NextAVTransportURIMetaData", "string", Allowed::Any),
        v("RelativeTimePosition", "string", Allowed::Any),
        v("AbsoluteTimePosition", "string", Allowed::Any),
        v("RelativeCounterPosition", "i4", Allowed::Any),
        v("AbsoluteCounterPosition", "i4", Allowed::Any),
        v("CurrentTransportActions", "string", Allowed::Any),
        evented("LastChange"),
        v(
            "A_ARG_TYPE_SeekMode",
            "string",
            Allowed::List(&["TRACK_NR", "REL_TIME"]),
        ),
        v("A_ARG_TYPE_SeekTarget", "string", Allowed::Any),
        v("A_ARG_TYPE_InstanceID", "ui4", Allowed::Any),
    ],
};

/// The one audio channel chorus has (RCS1 section 2.2.19: "Master" is the
/// logical channel for the whole output).
const CHANNEL: Arg = i("Channel", "A_ARG_TYPE_Channel");

/// RenderingControl:1 as chorus offers it. RCS1 section 2.4, table 3: the
/// required ListPresets and SelectPreset (sections 2.4.1, 2.4.2) and the
/// optional GetMute, SetMute, GetVolume and SetVolume (sections 2.4.27 to
/// 2.4.30). The decibel and loudness actions and every picture control are
/// optional and absent; RCS1 section 2.4.30.2 says Volume and VolumeDB "need
/// to change consistently", and offering one scale keeps that true.
pub const RENDERING_CONTROL: ServiceTable = ServiceTable {
    service: Service::RenderingControl,
    actions: &[
        Action {
            name: "ListPresets",
            args: &[ID, o("CurrentPresetNameList", "PresetNameList")],
        },
        Action {
            name: "SelectPreset",
            args: &[ID, i("PresetName", "A_ARG_TYPE_PresetName")],
        },
        Action {
            name: "GetMute",
            args: &[ID, CHANNEL, o("CurrentMute", "Mute")],
        },
        Action {
            name: "SetMute",
            args: &[ID, CHANNEL, i("DesiredMute", "Mute")],
        },
        Action {
            name: "GetVolume",
            args: &[ID, CHANNEL, o("CurrentVolume", "Volume")],
        },
        Action {
            name: "SetVolume",
            args: &[ID, CHANNEL, i("DesiredVolume", "Volume")],
        },
    ],
    // RCS1 section 2.2. Volume is `ui2`, "from a minimum of 0 to some device
    // specific maximum" (section 2.2.16): 0 to 100 in steps of 1 here.
    variables: &[
        v("PresetNameList", "string", Allowed::Any),
        evented("LastChange"),
        v("Mute", "boolean", Allowed::Any),
        v(
            "Volume",
            "ui2",
            Allowed::Range {
                min: 0,
                max: 100,
                step: 1,
            },
        ),
        v("A_ARG_TYPE_Channel", "string", Allowed::List(&["Master"])),
        v("A_ARG_TYPE_InstanceID", "ui4", Allowed::Any),
        v(
            "A_ARG_TYPE_PresetName",
            "string",
            Allowed::List(&["FactoryDefaults"]),
        ),
    ],
};

/// ConnectionManager:1 as chorus offers it. CM1 section 2.4: the required
/// GetProtocolInfo, GetCurrentConnectionIDs and GetCurrentConnectionInfo.
/// PrepareForConnection and ConnectionComplete are optional and absent, so
/// there is one connection, ID 0, and control points use InstanceID 0 (MR1
/// section 2.5; AVT1 section 2.2.30). The three evented variables are evented
/// directly, without moderation (CM1 section 2.3, table 2). Arguments and
/// types are the SCPD of CM1 section 3.
pub const CONNECTION_MANAGER: ServiceTable = ServiceTable {
    service: Service::ConnectionManager,
    actions: &[
        Action {
            name: "GetProtocolInfo",
            args: &[
                o("Source", "SourceProtocolInfo"),
                o("Sink", "SinkProtocolInfo"),
            ],
        },
        Action {
            name: "GetCurrentConnectionIDs",
            args: &[o("ConnectionIDs", "CurrentConnectionIDs")],
        },
        Action {
            name: "GetCurrentConnectionInfo",
            args: &[
                i("ConnectionID", "A_ARG_TYPE_ConnectionID"),
                o("RcsID", "A_ARG_TYPE_RcsID"),
                o("AVTransportID", "A_ARG_TYPE_AVTransportID"),
                o("ProtocolInfo", "A_ARG_TYPE_ProtocolInfo"),
                o("PeerConnectionManager", "A_ARG_TYPE_ConnectionManager"),
                o("PeerConnectionID", "A_ARG_TYPE_ConnectionID"),
                o("Direction", "A_ARG_TYPE_Direction"),
                o("Status", "A_ARG_TYPE_ConnectionStatus"),
            ],
        },
    ],
    variables: &[
        evented("SourceProtocolInfo"),
        evented("SinkProtocolInfo"),
        evented("CurrentConnectionIDs"),
        v(
            "A_ARG_TYPE_ConnectionStatus",
            "string",
            Allowed::List(&[
                "OK",
                "ContentFormatMismatch",
                "InsufficientBandwidth",
                "UnreliableChannel",
                "Unknown",
            ]),
        ),
        v("A_ARG_TYPE_ConnectionManager", "string", Allowed::Any),
        v(
            "A_ARG_TYPE_Direction",
            "string",
            Allowed::List(&["Input", "Output"]),
        ),
        v("A_ARG_TYPE_ProtocolInfo", "string", Allowed::Any),
        v("A_ARG_TYPE_ConnectionID", "i4", Allowed::Any),
        v("A_ARG_TYPE_AVTransportID", "i4", Allowed::Any),
        v("A_ARG_TYPE_RcsID", "i4", Allowed::Any),
    ],
};

/// The table of a service. The OpenHome services' tables are in
/// [`crate::openhome::tables`].
pub fn table(service: Service) -> &'static ServiceTable {
    match service {
        Service::AvTransport => &AVTRANSPORT,
        Service::RenderingControl => &RENDERING_CONTROL,
        Service::ConnectionManager => &CONNECTION_MANAGER,
        Service::Product => &tables::PRODUCT,
        Service::Volume => &tables::VOLUME,
        Service::Info => &tables::INFO,
        Service::Time => &tables::TIME,
        Service::Playlist => &tables::PLAYLIST,
    }
}

/// The service description of a service (UDA11 section 2.5), written from
/// its table: `specVersion` 1.1, the action list, the state table with
/// `sendEvents` always written out.
pub fn scpd(service: Service, config_id: u32) -> String {
    let t = table(service);
    let mut x = String::with_capacity(8192);
    x.push_str("<?xml version=\"1.0\"?>\n");
    x.push_str(&format!(
        "<scpd xmlns=\"urn:schemas-upnp-org:service-1-0\" configId=\"{config_id}\">\n"
    ));
    x.push_str("  <specVersion>\n    <major>1</major>\n    <minor>1</minor>\n  </specVersion>\n");
    x.push_str("  <actionList>\n");
    for action in t.actions {
        x.push_str("    <action>\n");
        x.push_str(&format!("      <name>{}</name>\n", action.name));
        if !action.args.is_empty() {
            x.push_str("      <argumentList>\n");
            for arg in action.args {
                x.push_str("        <argument>\n");
                x.push_str(&format!("          <name>{}</name>\n", arg.name));
                x.push_str(&format!(
                    "          <direction>{}</direction>\n",
                    match arg.dir {
                        Dir::In => "in",
                        Dir::Out => "out",
                    }
                ));
                x.push_str(&format!(
                    "          <relatedStateVariable>{}</relatedStateVariable>\n",
                    arg.var
                ));
                x.push_str("        </argument>\n");
            }
            x.push_str("      </argumentList>\n");
        }
        x.push_str("    </action>\n");
    }
    x.push_str("  </actionList>\n");
    x.push_str("  <serviceStateTable>\n");
    for var in t.variables {
        x.push_str(&format!(
            "    <stateVariable sendEvents=\"{}\">\n",
            if var.evented { "yes" } else { "no" }
        ));
        x.push_str(&format!("      <name>{}</name>\n", var.name));
        x.push_str(&format!("      <dataType>{}</dataType>\n", var.data_type));
        match var.allowed {
            Allowed::Any => {}
            Allowed::List(values) => {
                x.push_str("      <allowedValueList>\n");
                for value in values {
                    x.push_str(&format!("        <allowedValue>{value}</allowedValue>\n"));
                }
                x.push_str("      </allowedValueList>\n");
            }
            Allowed::Range { min, max, step } => {
                x.push_str("      <allowedValueRange>\n");
                x.push_str(&format!("        <minimum>{min}</minimum>\n"));
                x.push_str(&format!("        <maximum>{max}</maximum>\n"));
                x.push_str(&format!("        <step>{step}</step>\n"));
                x.push_str("      </allowedValueRange>\n");
            }
        }
        x.push_str("    </stateVariable>\n");
    }
    x.push_str("  </serviceStateTable>\n");
    x.push_str("</scpd>");
    x
}

/// What a device description says about one renderer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceInfo {
    /// The device's UUID ([`crate::uuid::udn`]).
    pub udn: Uuid,
    /// The name a person sees: the room's or group's display name. UDA11
    /// section 2.3 says it "SHOULD be < 64 characters"; a longer name is cut
    /// at 63 characters when written.
    pub friendly_name: String,
    /// `modelName`: what kind of target this is, for example `chorus room`.
    pub model_name: String,
    /// `modelNumber`: the chorus version.
    pub model_number: String,
    /// Whether the device also offers the OpenHome services
    /// ([`Service::OPENHOME`]), listed after the three AV ones on the same
    /// `MediaRenderer:1` device (the single-device layout, `docs/upnp.md`).
    /// This is the one switch of the layout.
    pub openhome: bool,
}

/// What a path under `/upnp/` names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    /// The device description.
    Description,
    /// A service's description.
    Scpd(Service),
    /// A service's control URL (SOAP `POST`).
    Control(Service),
    /// A service's event subscription URL (`SUBSCRIBE`, `UNSUBSCRIBE`).
    Event(Service),
}

/// `/upnp/<uuid>/desc.xml`: the path of a device's description, which the
/// discovery `LOCATION` points at.
pub fn description_path(udn: &Uuid) -> String {
    format!("/upnp/{udn}/desc.xml")
}

/// `/upnp/<uuid>/<service>/scpd.xml`.
pub fn scpd_path(udn: &Uuid, service: Service) -> String {
    format!("/upnp/{udn}/{}/scpd.xml", service.path())
}

/// `/upnp/<uuid>/<service>/control`.
pub fn control_path(udn: &Uuid, service: Service) -> String {
    format!("/upnp/{udn}/{}/control", service.path())
}

/// `/upnp/<uuid>/<service>/event`. The UUID in the path makes the URL
/// "unique to a particular service within this device" (UDA11 section 4.1.1)
/// and lets one listener route every renderer without a table.
pub fn event_path(udn: &Uuid, service: Service) -> String {
    format!("/upnp/{udn}/{}/event", service.path())
}

/// Which device and resource a request path names, or `None` for anything
/// that is not one of the four path shapes above (a query string is
/// ignored). The server answers 404 for `None` and for a UUID it does not
/// have.
pub fn route(path: &str) -> Option<(Uuid, Resource)> {
    let path = path.split(['?', '#']).next().unwrap_or("");
    let rest = path.strip_prefix("/upnp/")?;
    let mut parts = rest.split('/');
    let udn = Uuid::parse(parts.next()?)?;
    let second = parts.next()?;
    let third = parts.next();
    if parts.next().is_some() {
        return None;
    }
    let resource = match (second, third) {
        ("desc.xml", None) => Resource::Description,
        (service, Some(leaf)) => {
            let service = Service::from_path(service)?;
            match leaf {
                "scpd.xml" => Resource::Scpd(service),
                "control" => Resource::Control(service),
                "event" => Resource::Event(service),
                _ => return None,
            }
        }
        _ => return None,
    };
    Some((udn, resource))
}

/// The device description (UDA11 section 2.3), element order as the
/// specification's template: `specVersion` 1.1, the MediaRenderer:1 device
/// type, the names, the UDN, and the services ([`Service::offered`]) with
/// relative URLs. The
/// manufacturer is "chorus" and the model description says what the device
/// is: a UPnP AV media renderer. No icon, no presentation page, no URLBase.
pub fn device_description(info: &DeviceInfo, config_id: u32) -> String {
    let name: String = info.friendly_name.chars().take(63).collect();
    let mut x = String::with_capacity(2048);
    x.push_str("<?xml version=\"1.0\"?>\n");
    x.push_str(&format!(
        "<root xmlns=\"urn:schemas-upnp-org:device-1-0\" configId=\"{config_id}\">\n"
    ));
    x.push_str("  <specVersion>\n    <major>1</major>\n    <minor>1</minor>\n  </specVersion>\n");
    x.push_str("  <device>\n");
    x.push_str(&format!("    <deviceType>{DEVICE_TYPE}</deviceType>\n"));
    x.push_str(&format!(
        "    <friendlyName>{}</friendlyName>\n",
        escape_text(&name)
    ));
    x.push_str("    <manufacturer>chorus</manufacturer>\n");
    x.push_str("    <modelDescription>chorus UPnP AV media renderer</modelDescription>\n");
    x.push_str(&format!(
        "    <modelName>{}</modelName>\n",
        escape_text(&info.model_name)
    ));
    x.push_str(&format!(
        "    <modelNumber>{}</modelNumber>\n",
        escape_text(&info.model_number)
    ));
    x.push_str(&format!("    <UDN>uuid:{}</UDN>\n", info.udn));
    x.push_str("    <serviceList>\n");
    for &service in Service::offered(info.openhome) {
        x.push_str("      <service>\n");
        x.push_str(&format!(
            "        <serviceType>{}</serviceType>\n",
            service.service_type()
        ));
        x.push_str(&format!(
            "        <serviceId>{}</serviceId>\n",
            service.service_id()
        ));
        x.push_str(&format!(
            "        <SCPDURL>{}</SCPDURL>\n",
            scpd_path(&info.udn, service)
        ));
        x.push_str(&format!(
            "        <controlURL>{}</controlURL>\n",
            control_path(&info.udn, service)
        ));
        x.push_str(&format!(
            "        <eventSubURL>{}</eventSubURL>\n",
            event_path(&info.udn, service)
        ));
        x.push_str("      </service>\n");
    }
    x.push_str("    </serviceList>\n");
    x.push_str("  </device>\n");
    x.push_str("</root>");
    x
}

/// The CONFIGID of a device: [`ssdp::config_id`] over the device description
/// and the SCPDs of the services it offers, each written with `configId="0"` (the number cannot
/// be part of what it is the hash of). UDA11 section 1.2.2: the
/// configuration is "the DDD of the root device ... and the SCPDs of all the
/// contained services". The same device gets the same number after a
/// restart; a rename changes it.
pub fn device_config_id(info: &DeviceInfo) -> u32 {
    let mut all = device_description(info, 0);
    for &service in Service::offered(info.openhome) {
        all.push_str(&scpd(service, 0));
    }
    ssdp::config_id(all.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client;
    use std::collections::BTreeSet;

    fn info() -> DeviceInfo {
        DeviceInfo {
            udn: Uuid::parse("3b8fa6e6-bb30-5005-b768-3e87f0af9a9a").unwrap(),
            friendly_name: "Kitchen".into(),
            model_name: "chorus room".into(),
            model_number: "0.1.0".into(),
            openhome: false,
        }
    }

    fn names(t: &ServiceTable) -> Vec<&'static str> {
        t.actions.iter().map(|a| a.name).collect()
    }

    /// AVT1 table 3, RCS1 table 3, CM1 section 2.4: every required action is
    /// present; the optional ones are exactly those chorus implements.
    #[test]
    fn required_actions_are_present_and_refused_ones_absent() {
        let avt = names(&AVTRANSPORT);
        for required in [
            "SetAVTransportURI",
            "GetMediaInfo",
            "GetTransportInfo",
            "GetPositionInfo",
            "GetDeviceCapabilities",
            "GetTransportSettings",
            "Stop",
            "Play",
            "Seek",
            "Next",
            "Previous",
        ] {
            assert!(avt.contains(&required), "{required}");
        }
        for optional in [
            "SetNextAVTransportURI",
            "Pause",
            "GetCurrentTransportActions",
        ] {
            assert!(avt.contains(&optional), "{optional}");
        }
        assert_eq!(avt.len(), 14);
        for absent in ["Record", "SetRecordQualityMode", "SetPlayMode"] {
            assert!(!avt.contains(&absent), "{absent}");
        }
        assert_eq!(
            names(&RENDERING_CONTROL),
            [
                "ListPresets",
                "SelectPreset",
                "GetMute",
                "SetMute",
                "GetVolume",
                "SetVolume"
            ]
        );
        assert_eq!(
            names(&CONNECTION_MANAGER),
            [
                "GetProtocolInfo",
                "GetCurrentConnectionIDs",
                "GetCurrentConnectionInfo"
            ]
        );
        assert!(CONNECTION_MANAGER.action("PrepareForConnection").is_none());
    }

    #[test]
    fn the_tables_are_self_consistent() {
        for service in Service::AV {
            let t = table(service);
            assert_eq!(t.service, service);
            assert!(!t.variables.is_empty(), "UDA11 2.5: one or more variables");
            let vars: BTreeSet<&str> = t.variables.iter().map(|v| v.name).collect();
            assert_eq!(vars.len(), t.variables.len(), "no variable twice");
            let actions: BTreeSet<&str> = t.actions.iter().map(|a| a.name).collect();
            assert_eq!(actions.len(), t.actions.len(), "no action twice");
            for action in t.actions {
                let mut seen_out = false;
                let mut arg_names = BTreeSet::new();
                for arg in action.args {
                    assert!(
                        vars.contains(arg.var),
                        "{}.{} names {}",
                        action.name,
                        arg.name,
                        arg.var
                    );
                    assert!(arg_names.insert(arg.name), "{}.{}", action.name, arg.name);
                    match arg.dir {
                        Dir::Out => seen_out = true,
                        Dir::In => assert!(
                            !seen_out,
                            "{}: in argument {} after an out argument",
                            action.name, arg.name
                        ),
                    }
                }
                assert_eq!(
                    action.inputs().count() + action.outputs().count(),
                    action.args.len()
                );
            }
            // Every variable that is not evented is used by an argument or
            // is one of AVTransport's and RenderingControl's own state.
            let evented: Vec<&str> = t
                .variables
                .iter()
                .filter(|v| v.evented)
                .map(|v| v.name)
                .collect();
            match service {
                Service::ConnectionManager => assert_eq!(
                    evented,
                    [
                        "SourceProtocolInfo",
                        "SinkProtocolInfo",
                        "CurrentConnectionIDs"
                    ]
                ),
                _ => assert_eq!(evented, ["LastChange"]),
            }
        }
        // The first input of every AVTransport and RenderingControl action.
        for t in [&AVTRANSPORT, &RENDERING_CONTROL] {
            for action in t.actions {
                assert_eq!(action.args[0], ID, "{}", action.name);
            }
        }
        assert_eq!(
            AVTRANSPORT.variable("A_ARG_TYPE_SeekMode").unwrap().allowed,
            Allowed::List(&["TRACK_NR", "REL_TIME"])
        );
        assert_eq!(
            RENDERING_CONTROL.variable("Volume").unwrap().allowed,
            Allowed::Range {
                min: 0,
                max: 100,
                step: 1
            }
        );
    }

    #[test]
    fn the_description_has_relative_urls_and_no_urlbase() {
        let d = device_description(&info(), 77);
        assert!(d.starts_with("<?xml version=\"1.0\"?>\n<root xmlns=\"urn:schemas-upnp-org:device-1-0\" configId=\"77\">"));
        assert!(!d.contains("URLBase"));
        assert!(!d.contains("http://"));
        assert!(d.contains("<major>1</major>\n    <minor>1</minor>"));
        assert!(d.contains("<UDN>uuid:3b8fa6e6-bb30-5005-b768-3e87f0af9a9a</UDN>"));
        assert!(d.contains("<manufacturer>chorus</manufacturer>"));
        assert!(d.contains("UPnP AV media renderer"));
        assert!(!d.to_ascii_lowercase().contains("dlna"));
        assert!(d.contains(
            "<controlURL>/upnp/3b8fa6e6-bb30-5005-b768-3e87f0af9a9a/rcs/control</controlURL>"
        ));
        // UDA11 2.3 element order.
        let order = [
            "<deviceType>",
            "<friendlyName>",
            "<manufacturer>",
            "<modelDescription>",
            "<modelName>",
            "<modelNumber>",
            "<UDN>",
            "<serviceList>",
        ];
        let at: Vec<usize> = order.iter().map(|e| d.find(e).unwrap()).collect();
        assert!(at.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn the_openhome_switch_adds_five_services_behind_the_three() {
        let av = client::parse_description(&device_description(&info(), 1)).unwrap();
        assert_eq!(av.services.len(), 3);
        let mut with = info();
        with.openhome = true;
        let text = device_description(&with, 1);
        let d = client::parse_description(&text).unwrap();
        let types: Vec<&str> = d.services.iter().map(|s| s.service_type.as_str()).collect();
        assert_eq!(
            types,
            [
                "urn:schemas-upnp-org:service:AVTransport:1",
                "urn:schemas-upnp-org:service:RenderingControl:1",
                "urn:schemas-upnp-org:service:ConnectionManager:1",
                "urn:av-openhome-org:service:Product:2",
                "urn:av-openhome-org:service:Volume:2",
                "urn:av-openhome-org:service:Info:1",
                "urn:av-openhome-org:service:Time:1",
                "urn:av-openhome-org:service:Playlist:1",
            ]
        );
        // The device is the same MediaRenderer:1 (the single-device layout).
        assert_eq!(d.device_type, DEVICE_TYPE);
        assert!(text.contains("<serviceId>urn:av-openhome-org:serviceId:Product</serviceId>"));
        assert!(text.contains(&format!(
            "<SCPDURL>/upnp/{}/ohl/scpd.xml</SCPDURL>",
            with.udn
        )));
        // The switch changes the configuration, so the CONFIGID too.
        assert_ne!(device_config_id(&info()), device_config_id(&with));
        // Every SCPD parses and says what its table says.
        for service in Service::OPENHOME {
            let scpd = client::parse_scpd(&scpd(service, 5)).unwrap();
            let t = table(service);
            assert_eq!(scpd.actions.len(), t.actions.len());
            assert_eq!(scpd.variables.len(), t.variables.len());
            for (read, var) in scpd.variables.iter().zip(t.variables) {
                assert_eq!(
                    (
                        read.name.as_str(),
                        read.data_type.as_str(),
                        read.send_events
                    ),
                    (var.name, var.data_type, var.evented)
                );
            }
        }
    }

    #[test]
    fn names_are_escaped_and_long_ones_cut() {
        let mut i = info();
        i.friendly_name = "Tom & Jerry's <room>".into();
        let d = device_description(&i, 1);
        assert!(d.contains("<friendlyName>Tom &amp; Jerry's &lt;room&gt;</friendlyName>"));
        i.friendly_name = "\u{e9}".repeat(100);
        let d = device_description(&i, 1);
        let name = d
            .split("<friendlyName>")
            .nth(1)
            .unwrap()
            .split('<')
            .next()
            .unwrap();
        assert_eq!(name.chars().count(), 63);
    }

    #[test]
    fn paths_route_back_to_what_they_name() {
        let u = info().udn;
        assert_eq!(
            route(&description_path(&u)),
            Some((u, Resource::Description))
        );
        for s in Service::ALL {
            assert_eq!(route(&scpd_path(&u, s)), Some((u, Resource::Scpd(s))));
            assert_eq!(route(&control_path(&u, s)), Some((u, Resource::Control(s))));
            assert_eq!(route(&event_path(&u, s)), Some((u, Resource::Event(s))));
        }
        assert_eq!(
            route(&format!("{}?x=1", description_path(&u))),
            Some((u, Resource::Description))
        );
        for bad in [
            "/",
            "/upnp/",
            "/upnp/nope/desc.xml",
            "/upnp/3b8fa6e6-bb30-5005-b768-3e87f0af9a9a",
            "/upnp/3b8fa6e6-bb30-5005-b768-3e87f0af9a9a/avt",
            "/upnp/3b8fa6e6-bb30-5005-b768-3e87f0af9a9a/avt/control/x",
            "/upnp/3b8fa6e6-bb30-5005-b768-3e87f0af9a9a/xyz/control",
            "/upnp/3b8fa6e6-bb30-5005-b768-3e87f0af9a9a/avt/other",
            "/upnp/3b8fa6e6-bb30-5005-b768-3e87f0af9a9a/desc.xml/x",
            "/other/3b8fa6e6-bb30-5005-b768-3e87f0af9a9a/desc.xml",
        ] {
            assert_eq!(route(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_config_id_follows_the_description() {
        let a = device_config_id(&info());
        assert_eq!(a, device_config_id(&info()));
        assert!(a <= 0x00ff_ffff);
        let mut renamed = info();
        renamed.friendly_name = "Scullery".into();
        assert_ne!(a, device_config_id(&renamed));
    }
}
