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
//! Nothing here was read from another implementation's source.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod avtransport;
pub mod client;
pub mod connmgr;
pub mod description;
pub mod didl;
pub mod gena;
pub mod lastchange;
pub mod rendering;
pub mod soap;
pub mod ssdp;
pub mod time;
pub mod uuid;
pub mod xml;

/// The device type every chorus renderer announces (MR1 section 2.1).
pub const DEVICE_TYPE: &str = "urn:schemas-upnp-org:device:MediaRenderer:1";

/// One of the three services of a MediaRenderer:1 device (MR1 section 2.2,
/// table 1: RenderingControl and ConnectionManager are required, AVTransport
/// is the one a renderer that pulls media over HTTP implements).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Service {
    /// AVTransport:1: what plays and the transport state.
    AvTransport,
    /// RenderingControl:1: volume and mute.
    RenderingControl,
    /// ConnectionManager:1: which formats the renderer takes.
    ConnectionManager,
}

impl Service {
    /// The three services in the order the description and the discovery
    /// messages list them.
    pub const ALL: [Service; 3] = [
        Service::AvTransport,
        Service::RenderingControl,
        Service::ConnectionManager,
    ];

    /// The service type URN (UDA11 section 2.3, `serviceType`).
    pub fn service_type(self) -> &'static str {
        match self {
            Service::AvTransport => "urn:schemas-upnp-org:service:AVTransport:1",
            Service::RenderingControl => "urn:schemas-upnp-org:service:RenderingControl:1",
            Service::ConnectionManager => "urn:schemas-upnp-org:service:ConnectionManager:1",
        }
    }

    /// The service id (MR1 section 4, the device description: the ids are
    /// `AVTransport`, `RenderingControl` and `ConnectionManager` "prefixed by
    /// urn:upnp-org:serviceId:", which is `upnp-org`, not `schemas-upnp-org`).
    pub fn service_id(self) -> &'static str {
        match self {
            Service::AvTransport => "urn:upnp-org:serviceId:AVTransport",
            Service::RenderingControl => "urn:upnp-org:serviceId:RenderingControl",
            Service::ConnectionManager => "urn:upnp-org:serviceId:ConnectionManager",
        }
    }

    /// The path segment chorus gives the service under `/upnp/<uuid>/`.
    pub fn path(self) -> &'static str {
        match self {
            Service::AvTransport => "avt",
            Service::RenderingControl => "rcs",
            Service::ConnectionManager => "cm",
        }
    }

    /// The service a path segment names.
    pub fn from_path(segment: &str) -> Option<Service> {
        Service::ALL.into_iter().find(|s| s.path() == segment)
    }

    /// The service a service type URN names; only version 1 is offered, so a
    /// URN of another version names nothing (UDA11 section 1.3.3).
    pub fn from_type(urn: &str) -> Option<Service> {
        Service::ALL.into_iter().find(|s| s.service_type() == urn)
    }

    /// The namespace of the service's LastChange `Event` document (AVT1
    /// section 5, RCS1 section 5: the schemas' target namespaces).
    /// ConnectionManager has no LastChange: its variables are evented
    /// directly (CM1 section 2.3).
    pub fn event_namespace(self) -> Option<&'static str> {
        match self {
            Service::AvTransport => Some("urn:schemas-upnp-org:metadata-1-0/AVT/"),
            Service::RenderingControl => Some("urn:schemas-upnp-org:metadata-1-0/RCS/"),
            Service::ConnectionManager => None,
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
        for s in Service::ALL {
            assert_eq!(Service::from_type(s.service_type()), Some(s));
            assert_eq!(Service::from_path(s.path()), Some(s));
            assert!(s.service_id().starts_with("urn:upnp-org:serviceId:"));
        }
        // Only version 1 exists here.
        assert_eq!(
            Service::from_type("urn:schemas-upnp-org:service:AVTransport:2"),
            None
        );
        assert_eq!(Service::ConnectionManager.event_namespace(), None);
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
