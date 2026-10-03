//! The pure half of chorus's UPnP AV media renderer (goal 16, P6's U1).
//!
//! Every room, saved group and live group of a chorus server is a UPnP AV
//! media renderer: a root device of type `MediaRenderer:1` with the services
//! AVTransport:1, RenderingControl:1 and ConnectionManager:1. This crate is
//! everything about that which needs no socket, no thread and no clock, so it
//! is tested as text: time comes in as arguments, random bytes come in as
//! arguments, and what goes on the wire comes out as strings. The sockets,
//! the timers and the player are the server's.
//!
//! - [`uuid`]: SHA-1, UUID version 5 and the device identity (UDN).
//! - [`ssdp`]: discovery: the alive and byebye sets, M-SEARCH and its
//!   responses, the announce scheduler and the per-source rate limit.
//! - [`xml`]: escaping and a small tolerant reader that refuses DOCTYPE.
//! - [`description`]: the device description and the three service
//!   descriptions, generated from one table of actions and state variables.
//! - [`soap`]: control: request parsing and validation against that table,
//!   responses and the UPnPError fault.
//! - [`gena`]: eventing: SUBSCRIBE and UNSUBSCRIBE, the callback rule, the
//!   subscription table, NOTIFY.
//! - [`lastchange`]: the LastChange event documents and their moderation.
//! - [`didl`]: tolerant DIDL-Lite metadata parsing.
//! - [`avtransport`]: the AVTransport state machine.
//! - [`rendering`]: RenderingControl: Master volume and mute.
//! - [`connmgr`]: ConnectionManager: the Sink list.
//! - [`time`]: `H:MM:SS` durations.
//! - [`openhome`]: the OpenHome services Product:2, Volume:2, Info:1, Time:1
//!   and Playlist:1 on the same device (goal 17, P6's Option B): their
//!   tables, their state machines and the plain-variable event path.
//! - [`base64`]: RFC 4648 base64, for the Playlist's `IdArray`.
//! - [`client`]: what a control point needs (the server's scripted control
//!   point test is written against it), tested by round trips against the
//!   device side.
//!
//! The product is a "UPnP AV media renderer". chorus claims no certification
//! and emits no `DLNA.ORG_PN` profile name; DLNA fields in what a control
//! point sends are read as opaque text.
//!
//! Short keys for the specifications cited beside each rule (all read
//! 2026-10-03; the decision record lists URLs and sections):
//!
//! - **UDA11**: UPnP Device Architecture 1.1, 15 October 2008.
//! - **UDA20**: UPnP Device Architecture 2.0, 17 April 2020.
//! - **MR1**: MediaRenderer:1 Device Template 1.01.
//! - **AVT1**: AVTransport:1 Service Template 1.01.
//! - **AVT3**: AVTransport:3 Service, 31 March 2013 (one section, for the
//!   gapless handover).
//! - **RCS1**: RenderingControl:1 Service Template 1.01.
//! - **CM1**: ConnectionManager:1 Service Template 1.01.
//! - **CDS1**: ContentDirectory:1 Service Template 1.01 (DIDL-Lite).
//! - **RFC9562**: Universally Unique IDentifiers.
//!
//! Nothing of the UPnP AV half was read from another implementation's
//! source. The OpenHome half has no specification but its reference
//! implementation: [`openhome`] is written from the service XMLs and the
//! provider sources of ohPipeline and ohNet (both MIT), cited there by path
//! and commit.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod avtransport;
pub mod base64;
pub mod client;
pub mod connmgr;
pub mod description;
pub mod didl;
pub mod gena;
pub mod lastchange;
pub mod openhome;
pub mod rendering;
pub mod soap;
pub mod ssdp;
pub mod time;
pub mod uuid;
pub mod xml;

/// The device type every chorus renderer announces (MR1 section 2.1).
pub const DEVICE_TYPE: &str = "urn:schemas-upnp-org:device:MediaRenderer:1";

/// One of the services of a chorus renderer: the three of a MediaRenderer:1
/// device (MR1 section 2.2, table 1: RenderingControl and ConnectionManager
/// are required, AVTransport is the one a renderer that pulls media over
/// HTTP implements) and, when the OpenHome services are on, the five of
/// [`openhome`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Service {
    /// AVTransport:1: what plays and the transport state.
    AvTransport,
    /// RenderingControl:1: volume and mute.
    RenderingControl,
    /// ConnectionManager:1: which formats the renderer takes.
    ConnectionManager,
    /// OpenHome Product:2: what the device is and its sources.
    Product,
    /// OpenHome Volume:2: volume, mute and the volume limit.
    Volume,
    /// OpenHome Info:1: what plays now.
    Info,
    /// OpenHome Time:1: where in the track.
    Time,
    /// OpenHome Playlist:1: the queue the device holds and walks itself.
    Playlist,
}

impl Service {
    /// The three UPnP AV services in the order the description and the
    /// discovery messages list them.
    pub const AV: [Service; 3] = [
        Service::AvTransport,
        Service::RenderingControl,
        Service::ConnectionManager,
    ];

    /// The five OpenHome services, in the order they are listed after the
    /// AV ones.
    pub const OPENHOME: [Service; 5] = [
        Service::Product,
        Service::Volume,
        Service::Info,
        Service::Time,
        Service::Playlist,
    ];

    /// Every service there is: [`Service::AV`], then [`Service::OPENHOME`].
    pub const ALL: [Service; 8] = [
        Service::AvTransport,
        Service::RenderingControl,
        Service::ConnectionManager,
        Service::Product,
        Service::Volume,
        Service::Info,
        Service::Time,
        Service::Playlist,
    ];

    /// The services a device offers: the AV three, and the OpenHome five
    /// behind them when `openhome` is on (the server's `--upnp-openhome`).
    pub fn offered(openhome: bool) -> &'static [Service] {
        if openhome {
            &Service::ALL
        } else {
            &Service::AV
        }
    }

    /// Where the service sits in [`Service::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }

    /// Whether this is one of the OpenHome services.
    pub fn is_openhome(self) -> bool {
        self.index() >= Service::AV.len()
    }

    /// The service type URN without its version: `urn:<domain>:service:<name>`
    /// (UDA11 section 2.3; for OpenHome the domain `av.openhome.org` is
    /// written `av-openhome-org`, ohNet `OpenHome/Net/Service.cpp:754-772`).
    fn type_stem(self) -> &'static str {
        match self {
            Service::AvTransport => "urn:schemas-upnp-org:service:AVTransport",
            Service::RenderingControl => "urn:schemas-upnp-org:service:RenderingControl",
            Service::ConnectionManager => "urn:schemas-upnp-org:service:ConnectionManager",
            Service::Product => "urn:av-openhome-org:service:Product",
            Service::Volume => "urn:av-openhome-org:service:Volume",
            Service::Info => "urn:av-openhome-org:service:Info",
            Service::Time => "urn:av-openhome-org:service:Time",
            Service::Playlist => "urn:av-openhome-org:service:Playlist",
        }
    }

    /// The version of the service chorus announces. The AV services are
    /// version 1; of OpenHome's, Product and Volume are version 2 and the
    /// rest version 1 (`docs/upnp.md` says why).
    pub fn version(self) -> u32 {
        match self {
            Service::Product | Service::Volume => 2,
            _ => 1,
        }
    }

    /// The service type URN announced (UDA11 section 2.3, `serviceType`).
    pub fn service_type(self) -> &'static str {
        match self {
            Service::AvTransport => "urn:schemas-upnp-org:service:AVTransport:1",
            Service::RenderingControl => "urn:schemas-upnp-org:service:RenderingControl:1",
            Service::ConnectionManager => "urn:schemas-upnp-org:service:ConnectionManager:1",
            Service::Product => "urn:av-openhome-org:service:Product:2",
            Service::Volume => "urn:av-openhome-org:service:Volume:2",
            Service::Info => "urn:av-openhome-org:service:Info:1",
            Service::Time => "urn:av-openhome-org:service:Time:1",
            Service::Playlist => "urn:av-openhome-org:service:Playlist:1",
        }
    }

    /// The service type URN at `version`, for answering a request that named
    /// a lower version than the one announced in that version.
    pub fn service_type_at(self, version: u32) -> String {
        format!("{}:{}", self.type_stem(), version)
    }

    /// The service id (MR1 section 4, the device description: the ids are
    /// `AVTransport`, `RenderingControl` and `ConnectionManager` "prefixed by
    /// urn:upnp-org:serviceId:", which is `upnp-org`, not `schemas-upnp-org`;
    /// OpenHome's are `urn:av-openhome-org:serviceId:<Name>`, ohNet
    /// `OpenHome/Net/Service.cpp:790-809`).
    pub fn service_id(self) -> &'static str {
        match self {
            Service::AvTransport => "urn:upnp-org:serviceId:AVTransport",
            Service::RenderingControl => "urn:upnp-org:serviceId:RenderingControl",
            Service::ConnectionManager => "urn:upnp-org:serviceId:ConnectionManager",
            Service::Product => "urn:av-openhome-org:serviceId:Product",
            Service::Volume => "urn:av-openhome-org:serviceId:Volume",
            Service::Info => "urn:av-openhome-org:serviceId:Info",
            Service::Time => "urn:av-openhome-org:serviceId:Time",
            Service::Playlist => "urn:av-openhome-org:serviceId:Playlist",
        }
    }

    /// The path segment chorus gives the service under `/upnp/<uuid>/`.
    pub fn path(self) -> &'static str {
        match self {
            Service::AvTransport => "avt",
            Service::RenderingControl => "rcs",
            Service::ConnectionManager => "cm",
            Service::Product => "ohp",
            Service::Volume => "ohv",
            Service::Info => "ohi",
            Service::Time => "oht",
            Service::Playlist => "ohl",
        }
    }

    /// The service a path segment names.
    pub fn from_path(segment: &str) -> Option<Service> {
        Service::ALL.into_iter().find(|s| s.path() == segment)
    }

    /// The service a service type URN names exactly, at the version
    /// announced (UDA11 section 1.3.3). [`Service::matching`] is the
    /// version-tolerant form.
    pub fn from_type(urn: &str) -> Option<Service> {
        Service::ALL.into_iter().find(|s| s.service_type() == urn)
    }

    /// The service a service type URN names and the version it asks for,
    /// when chorus can serve that version: the same service at the announced
    /// version or a lower one, down to 1. UDA11 section 2: a higher version
    /// of a service is a superset of the lower ones, so a control point that
    /// binds `Product:1` is served by `Product:2`; a search or an action
    /// naming the lower version is answered in that version (UDA11 section
    /// 1.3.3: "The response MUST specify the same version as was contained in
    /// the search request"; ohNet answers the same way,
    /// `OpenHome/Net/Device/Upnp/DviProtocolUpnp.cpp:685-702`). A higher
    /// version than the announced one, version 0 and a number with a sign or
    /// leading zeros name nothing.
    pub fn matching(urn: &str) -> Option<(Service, u32)> {
        let (stem, version) = urn.rsplit_once(':')?;
        let service = Service::ALL.into_iter().find(|s| s.type_stem() == stem)?;
        if version.is_empty()
            || version.len() > 4
            || version.starts_with('0')
            || !version.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        let version: u32 = version.parse().ok()?;
        (version >= 1 && version <= service.version()).then_some((service, version))
    }

    /// The namespace of the service's LastChange `Event` document (AVT1
    /// section 5, RCS1 section 5: the schemas' target namespaces).
    /// ConnectionManager and the OpenHome services have no LastChange: their
    /// variables are evented directly (CM1 section 2.3; ohNet
    /// `OpenHome/Net/Device/DviSubscription.cpp:393-407`).
    pub fn event_namespace(self) -> Option<&'static str> {
        match self {
            Service::AvTransport => Some("urn:schemas-upnp-org:metadata-1-0/AVT/"),
            Service::RenderingControl => Some("urn:schemas-upnp-org:metadata-1-0/RCS/"),
            _ => None,
        }
    }
}

/// A UPnP action error: what the `UPnPError` fault of UDA11 section 3.2.5
/// carries. The constants are in [`error`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UpnpError {
    /// `errorCode`.
    pub code: u16,
    /// `errorDescription`: the short name the specification gives the code.
    pub description: &'static str,
}

/// The error codes chorus returns, each with the name its specification gives
/// it. A code means different things in different services (701 and 702
/// especially), so the constants carry the service in their names.
pub mod error {
    use super::UpnpError;

    const fn e(code: u16, description: &'static str) -> UpnpError {
        UpnpError { code, description }
    }

    /// UDA11 table 3-3: "No action by that name at this service."
    pub const INVALID_ACTION: UpnpError = e(401, "Invalid Action");
    /// UDA11 table 3-3: "not enough in args, args in the wrong order, one or
    /// more in args are of the wrong data type".
    pub const INVALID_ARGS: UpnpError = e(402, "Invalid Args");
    /// UDA11 table 3-3: "MAY be returned if current state of service prevents
    /// invoking that action."
    pub const ACTION_FAILED: UpnpError = e(501, "Action Failed");
    /// UDA11 table 3-3: "The argument value is invalid".
    pub const ARGUMENT_VALUE_INVALID: UpnpError = e(600, "Argument Value Invalid");
    /// UDA11 table 3-3: "less than the minimum or more than the maximum value
    /// of the allowed value range, or is not in the allowed value list".
    pub const ARGUMENT_VALUE_OUT_OF_RANGE: UpnpError = e(601, "Argument Value Out of Range");

    /// AVT1 section 2.4.8.4: "The immediate transition from current transport
    /// state to desired transport state is not supported by this device."
    pub const AVT_TRANSITION_NOT_AVAILABLE: UpnpError = e(701, "Transition not available");
    /// AVT1 section 2.4.9.4: "The media does not contain any contents that
    /// can be played."
    pub const AVT_NO_CONTENTS: UpnpError = e(702, "No contents");
    /// AVT1 section 2.4.9.4: "The media cannot be read".
    pub const AVT_READ_ERROR: UpnpError = e(703, "Read error");
    /// AVT1 section 2.4.9.4: "The storage format of the currently loaded
    /// media is not supported for playback by this device."
    pub const AVT_FORMAT_NOT_SUPPORTED: UpnpError = e(704, "Format not supported for playback");
    /// AVT1 section 2.4.8.4: the transport is "hold locked". chorus has no
    /// such lock and never returns it; listed so the table is whole.
    pub const AVT_TRANSPORT_LOCKED: UpnpError = e(705, "Transport is locked");
    /// AVT1 section 2.4.11.3 (Record, which chorus does not offer).
    pub const AVT_WRITE_ERROR: UpnpError = e(706, "Write error");
    /// AVT1 section 2.4.11.3 (Record).
    pub const AVT_MEDIA_PROTECTED: UpnpError = e(707, "Media is protected or not writable");
    /// AVT1 section 2.4.11.3 (Record).
    pub const AVT_FORMAT_NOT_SUPPORTED_FOR_RECORDING: UpnpError =
        e(708, "Format not supported for recording");
    /// AVT1 section 2.4.11.3 (Record).
    pub const AVT_MEDIA_FULL: UpnpError = e(709, "Media is full");
    /// AVT1 section 2.4.12.4: "The specified seek mode is not supported by
    /// the device."
    pub const AVT_SEEK_MODE_NOT_SUPPORTED: UpnpError = e(710, "Seek mode not supported");
    /// AVT1 section 2.4.12.4: "The specified seek target is not specified in
    /// terms of the seek mode, or is not present on the media."
    pub const AVT_ILLEGAL_SEEK_TARGET: UpnpError = e(711, "Illegal seek target");
    /// AVT1 section 2.4.15.4 (SetPlayMode, which chorus does not offer).
    pub const AVT_PLAY_MODE_NOT_SUPPORTED: UpnpError = e(712, "Play mode not supported");
    /// AVT1 section 2.4.16.4 (SetRecordQualityMode, not offered).
    pub const AVT_RECORD_QUALITY_NOT_SUPPORTED: UpnpError = e(713, "Record quality not supported");
    /// AVT1 section 2.4.1.4: "The specified resource has a MIME-type which is
    /// not supported by the AVTransport service".
    pub const AVT_ILLEGAL_MIME_TYPE: UpnpError = e(714, "Illegal MIME-type");
    /// AVT1 section 2.4.1.4: "the resource is already being played by other
    /// means".
    pub const AVT_CONTENT_BUSY: UpnpError = e(715, "Content 'BUSY'");
    /// AVT1 section 2.4.1.4: "The specified resource cannot be found in the
    /// network".
    pub const AVT_RESOURCE_NOT_FOUND: UpnpError = e(716, "Resource not found");
    /// AVT1 section 2.4.9.4: "The specified playback speed is not supported
    /// by the AVTransport service."
    pub const AVT_PLAY_SPEED_NOT_SUPPORTED: UpnpError = e(717, "Play speed not supported");
    /// AVT1 section 2.4.1.4: "The specified instanceID is invalid for this
    /// AVTransport."
    pub const AVT_INVALID_INSTANCE_ID: UpnpError = e(718, "Invalid InstanceID");
    /// AVT1 section 2.4.1.4: "The DNS Server is not available". AVTransport:1
    /// numbers this 737, not 719.
    pub const AVT_NO_DNS_SERVER: UpnpError = e(737, "No DNS Server");
    /// AVT1 section 2.4.1.4: "Unable to resolve the Fully Qualified Domain
    /// Name."
    pub const AVT_BAD_DOMAIN_NAME: UpnpError = e(738, "Bad Domain Name");
    /// AVT1 section 2.4.1.4: "The server that hosts the resource is
    /// unreachable or unresponsive".
    pub const AVT_SERVER_ERROR: UpnpError = e(739, "Server Error");

    /// RCS1 section 2.4.2.3: "The specified name is not a valid preset name."
    pub const RCS_INVALID_NAME: UpnpError = e(701, "Invalid Name");
    /// RCS1 section 2.4.1.3: "The specified instanceID is invalid." Note the
    /// number: AVTransport's is 718.
    pub const RCS_INVALID_INSTANCE_ID: UpnpError = e(702, "Invalid InstanceID");

    /// CM1 section 2.4.5.4: "The connection reference argument does not refer
    /// to a valid connection established by this service."
    pub const CM_INVALID_CONNECTION_REFERENCE: UpnpError = e(706, "Invalid connection reference");

    /// OpenHome Product: "Source not found", for an index or a name that is
    /// not in the source list (ohPipeline `OpenHome/Av/ProviderProduct.cpp`
    /// lines 238-296, `OpenHome/Av/Utils/FaultCode.cpp:21`, at `cccd06dd`).
    pub const OH_PRODUCT_SOURCE_NOT_FOUND: UpnpError = e(801, "Source not found");
    /// OpenHome Playlist: "Id not found" (ohPipeline
    /// `OpenHome/Av/Playlist/ProviderPlaylist.cpp:23-26`).
    pub const OH_PLAYLIST_ID_NOT_FOUND: UpnpError = e(800, "Id not found");
    /// OpenHome Playlist: "Playlist full" (the same lines).
    pub const OH_PLAYLIST_FULL: UpnpError = e(801, "Playlist full");
    /// OpenHome Playlist: "Index not found" (`ProviderPlaylist.cpp:294-301`).
    pub const OH_PLAYLIST_INDEX_NOT_FOUND: UpnpError = e(802, "Index not found");
    /// OpenHome Playlist: "Seek failed" (`ProviderPlaylist.cpp:240-280`).
    pub const OH_PLAYLIST_SEEK_FAILED: UpnpError = e(803, "Seek failed");
    /// OpenHome Playlist: "Shuffle not currently possible"
    /// (`ProviderPlaylist.cpp:218-229`).
    pub const OH_PLAYLIST_SHUFFLE_NOT_POSSIBLE: UpnpError =
        e(804, "Shuffle not currently possible");
    /// OpenHome Volume: "Action not supported", for balance and fade, which
    /// chorus has not (ohPipeline `OpenHome/Av/ProviderVolume.cpp:15-16`).
    pub const OH_VOLUME_NOT_SUPPORTED: UpnpError = e(801, "Action not supported");
    /// OpenHome Volume: "Volume invalid": above the scale, or above the
    /// limit when the volume already is at the limit
    /// (`ProviderVolume.cpp:18-19`, `VolumeManager.cpp:229-253`).
    pub const OH_VOLUME_INVALID: UpnpError = e(811, "Volume invalid");

    /// Every AVTransport:1 error code with its name, for the tests that hold
    /// the table to the specification.
    pub const AVT_ALL: [UpnpError; 21] = [
        AVT_TRANSITION_NOT_AVAILABLE,
        AVT_NO_CONTENTS,
        AVT_READ_ERROR,
        AVT_FORMAT_NOT_SUPPORTED,
        AVT_TRANSPORT_LOCKED,
        AVT_WRITE_ERROR,
        AVT_MEDIA_PROTECTED,
        AVT_FORMAT_NOT_SUPPORTED_FOR_RECORDING,
        AVT_MEDIA_FULL,
        AVT_SEEK_MODE_NOT_SUPPORTED,
        AVT_ILLEGAL_SEEK_TARGET,
        AVT_PLAY_MODE_NOT_SUPPORTED,
        AVT_RECORD_QUALITY_NOT_SUPPORTED,
        AVT_ILLEGAL_MIME_TYPE,
        AVT_CONTENT_BUSY,
        AVT_RESOURCE_NOT_FOUND,
        AVT_PLAY_SPEED_NOT_SUPPORTED,
        AVT_INVALID_INSTANCE_ID,
        AVT_NO_DNS_SERVER,
        AVT_BAD_DOMAIN_NAME,
        AVT_SERVER_ERROR,
    ];
}

/// The out arguments of an action, by name, in SCPD order: what
/// [`soap::build_response`] takes.
pub type Outputs = Vec<(&'static str, String)>;

/// Reads an `InstanceID` argument (type `ui4`): `Ok(true)` for 0, the only
/// instance there is when ConnectionManager has no PrepareForConnection
/// (AVT1 section 2.2.30), `Ok(false)` for another number, and 402 for text
/// that is not a `ui4` (UDA11 table 3-3: an argument "of the wrong data
/// type").
pub(crate) fn instance_is_zero(text: &str) -> Result<bool, UpnpError> {
    let t = text.trim();
    if t.is_empty() || t.len() > 10 || !t.bytes().all(|b| b.is_ascii_digit()) {
        return Err(error::INVALID_ARGS);
    }
    match t.parse::<u32>() {
        Ok(n) => Ok(n == 0),
        Err(_) => Err(error::INVALID_ARGS),
    }
}

/// HTTP-style header fields as SSDP, SOAP and GENA carry them: names are
/// case-insensitive, values are kept as sent apart from surrounding white
/// space (UDA11 section 1.1.2: "Header field names are case-insensitive").
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Headers {
    fields: Vec<(String, String)>,
}

impl Headers {
    /// Header fields from name and value pairs, for a server that parsed the
    /// HTTP request itself.
    pub fn from_pairs<'a, I>(pairs: I) -> Headers
    where
        I: IntoIterator<Item = (&'a str, &'a str)>,
    {
        Headers {
            fields: pairs
                .into_iter()
                .map(|(n, v)| (n.trim().to_string(), v.trim().to_string()))
                .collect(),
        }
    }

    /// Splits a message head into its start line and its header fields. Lines
    /// end in CRLF or a bare LF; a line with no colon is skipped (UDA11
    /// section 1.1.2: receivers "MUST be able to skip header fields they do
    /// not understand"); reading stops at the first empty line.
    pub fn parse(head: &str) -> (&str, Headers) {
        let mut lines = head.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l));
        let start = lines.next().unwrap_or("").trim();
        let mut fields = Vec::new();
        for line in lines {
            if line.is_empty() {
                break;
            }
            if let Some((name, value)) = line.split_once(':') {
                fields.push((name.trim().to_string(), value.trim().to_string()));
            }
        }
        (start, Headers { fields })
    }

    /// The first field of that name, any case.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// Whether a field of that name is present, any case.
    pub fn has(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// Every field in the order sent.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.fields.iter().map(|(n, v)| (n.as_str(), v.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_services_are_found_by_type_and_by_path() {
        for (i, s) in Service::ALL.into_iter().enumerate() {
            assert_eq!(Service::from_type(s.service_type()), Some(s));
            assert_eq!(Service::from_path(s.path()), Some(s));
            assert_eq!(s.index(), i);
            assert_eq!(s.service_type_at(s.version()), s.service_type());
            assert_eq!(Service::matching(s.service_type()), Some((s, s.version())));
            assert_eq!(s.is_openhome(), i >= 3);
            if s.is_openhome() {
                assert!(s.service_id().starts_with("urn:av-openhome-org:serviceId:"));
            } else {
                assert!(s.service_id().starts_with("urn:upnp-org:serviceId:"));
            }
        }
        assert_eq!(Service::offered(false), Service::AV);
        assert_eq!(Service::offered(true), Service::ALL);
        assert_eq!(&Service::ALL[..3], Service::AV);
        assert_eq!(&Service::ALL[3..], Service::OPENHOME);
        // Only version 1 of the AV services exists here.
        assert_eq!(
            Service::from_type("urn:schemas-upnp-org:service:AVTransport:2"),
            None
        );
        assert_eq!(
            Service::matching("urn:schemas-upnp-org:service:AVTransport:2"),
            None
        );
        assert_eq!(Service::ConnectionManager.event_namespace(), None);
        assert_eq!(Service::Playlist.event_namespace(), None);
    }

    #[test]
    fn a_lower_version_of_a_service_is_served_and_a_higher_one_is_not() {
        let p = "urn:av-openhome-org:service:Product";
        assert_eq!(
            Service::matching(&format!("{p}:1")),
            Some((Service::Product, 1))
        );
        assert_eq!(
            Service::matching(&format!("{p}:2")),
            Some((Service::Product, 2))
        );
        for bad in [":3", ":0", ":", ":01", ":+1", ":x", "", ":99999"] {
            assert_eq!(Service::matching(&format!("{p}{bad}")), None, "{bad}");
        }
        assert_eq!(
            Service::matching("urn:av-openhome-org:service:Volume:1"),
            Some((Service::Volume, 1))
        );
        assert_eq!(
            Service::matching("urn:av-openhome-org:service:Playlist:2"),
            None
        );
        assert_eq!(
            Service::matching("urn:av-openhome-org:service:Radio:1"),
            None
        );
        assert_eq!(Service::from_type(&format!("{p}:1")), None);
    }

    #[test]
    fn avtransport_uses_737_to_739_and_the_codes_are_distinct() {
        let codes: Vec<u16> = error::AVT_ALL.iter().map(|e| e.code).collect();
        let expected: Vec<u16> = (701..=718).chain(737..=739).collect();
        assert_eq!(codes, expected);
        assert_eq!(error::RCS_INVALID_INSTANCE_ID.code, 702);
        assert_eq!(error::AVT_INVALID_INSTANCE_ID.code, 718);
        assert_eq!(error::CM_INVALID_CONNECTION_REFERENCE.code, 706);
    }

    #[test]
    fn header_names_match_in_any_case_and_unknown_lines_are_skipped() {
        let (start, h) = Headers::parse(
            "M-SEARCH * HTTP/1.1\r\nhOsT: a:1\r\nnonsense\r\nST:  x \r\n\r\nBody: no\r\n",
        );
        assert_eq!(start, "M-SEARCH * HTTP/1.1");
        assert_eq!(h.get("HOST"), Some("a:1"));
        assert_eq!(h.get("st"), Some("x"));
        assert!(!h.has("Body"));
        assert_eq!(h.iter().count(), 2);
    }
}
