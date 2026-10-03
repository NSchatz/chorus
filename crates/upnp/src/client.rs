//! The control point's side: what a program needs to find a renderer, call
//! its actions and follow its events.
//!
//! chorus is a renderer, not a control point; this module exists so the
//! server's end-to-end test can drive the real sockets with a scripted
//! control point, and it is held to the device side by round trips: every
//! message built here is read by the device-side parser, and every message
//! the device side builds is read here.
//!
//! The requests are built the way the specification's templates write them.
//! [`soap_request_raw`] also builds requests the table would refuse, because
//! a test has to be able to send a wrong one.

use crate::description::table;
use crate::soap::{CONTROL_NS, ENCODING_NS, ENVELOPE_NS};
use crate::uuid::Uuid;
use crate::xml::{self, escape_text, Limits, Node, XmlError};
use crate::{ssdp, Headers, Service};

/// An M-SEARCH for the multicast group (UDA11 section 1.3.2).
pub fn msearch(st: &str, mx: u32) -> String {
    format!(
        "M-SEARCH * HTTP/1.1\r\nHOST: {}\r\nMAN: \"ssdp:discover\"\r\nMX: {mx}\r\nST: {st}\r\n\r\n",
        ssdp::MULTICAST
    )
}

/// What kind of discovery message a datagram is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SsdpKind {
    /// `NOTIFY` with `NTS: ssdp:alive`.
    Alive,
    /// `NOTIFY` with `NTS: ssdp:byebye`.
    Byebye,
    /// `NOTIFY` with `NTS: ssdp:update`.
    Update,
    /// A `200 OK` response to an M-SEARCH.
    SearchResponse,
}

/// A discovery message, read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SsdpMessage {
    /// Which message.
    pub kind: SsdpKind,
    /// `NT` of a notification or `ST` of a search response.
    pub target: String,
    /// `USN`.
    pub usn: String,
    /// `LOCATION`.
    pub location: Option<String>,
    /// `max-age` of `CACHE-CONTROL`.
    pub max_age_s: Option<u32>,
    /// `SERVER`.
    pub server: Option<String>,
    /// `BOOTID.UPNP.ORG`.
    pub boot_id: Option<u32>,
    /// `CONFIGID.UPNP.ORG`.
    pub config_id: Option<u32>,
    /// `SEARCHPORT.UPNP.ORG`.
    pub search_port: Option<u16>,
    /// Every header field, for checks this struct has no field for (`EXT`,
    /// `DATE`, `HOST`).
    pub headers: Headers,
}

impl SsdpMessage {
    /// The device's UUID, from the USN's `uuid:` part.
    pub fn udn(&self) -> Option<Uuid> {
        let rest = self.usn.strip_prefix("uuid:")?;
        Uuid::parse(rest.split("::").next()?)
    }
}

/// Reads a discovery datagram: a `NOTIFY * HTTP/1.1` or a search response.
/// `None` for anything else, an M-SEARCH included.
pub fn parse_ssdp(datagram: &[u8]) -> Option<SsdpMessage> {
    let text = std::str::from_utf8(datagram).ok()?;
    let (start, headers) = Headers::parse(text);
    let (kind, target) = if start == "NOTIFY * HTTP/1.1" {
        let kind = match headers.get("NTS")? {
            "ssdp:alive" => SsdpKind::Alive,
            "ssdp:byebye" => SsdpKind::Byebye,
            "ssdp:update" => SsdpKind::Update,
            _ => return None,
        };
        (kind, headers.get("NT")?)
    } else if start.starts_with("HTTP/1.1 200") {
        (SsdpKind::SearchResponse, headers.get("ST")?)
    } else {
        return None;
    };
    let number = |name: &str| headers.get(name).and_then(|v| v.parse::<u32>().ok());
    let max_age_s = headers.get("CACHE-CONTROL").and_then(|v| {
        v.split(',').find_map(|d| {
            let (k, n) = d.split_once('=')?;
            k.trim()
                .eq_ignore_ascii_case("max-age")
                .then(|| n.trim().parse().ok())?
        })
    });
    Some(SsdpMessage {
        kind,
        target: target.to_string(),
        usn: headers.get("USN")?.to_string(),
        location: headers.get("LOCATION").map(str::to_string),
        max_age_s,
        server: headers.get("SERVER").map(str::to_string),
        boot_id: number("BOOTID.UPNP.ORG"),
        config_id: number("CONFIGID.UPNP.ORG"),
        search_port: number("SEARCHPORT.UPNP.ORG").and_then(|p| u16::try_from(p).ok()),
        headers,
    })
}

/// A control request ready to POST: the `SOAPACTION` header's value and the
/// body. Send it with `CONTENT-TYPE:` [`crate::soap::CONTENT_TYPE`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoapCall {
    /// The `SOAPACTION` value, double quotes included.
    pub soapaction: String,
    /// The envelope.
    pub body: String,
}

/// A control request for any service type, action name and arguments, with
/// no check at all (UDA11 section 3.2.1's template): for sending what a
/// renderer must refuse.
pub fn soap_request_raw(service_type: &str, action: &str, arguments: &[(&str, &str)]) -> SoapCall {
    let mut body = format!(
        "<?xml version=\"1.0\"?>\n<s:Envelope xmlns:s=\"{ENVELOPE_NS}\" s:encodingStyle=\"{ENCODING_NS}\"><s:Body><u:{action} xmlns:u=\"{service_type}\">"
    );
    for (name, value) in arguments {
        body.push_str(&format!("<{name}>{}</{name}>", escape_text(value)));
    }
    body.push_str(&format!("</u:{action}></s:Body></s:Envelope>"));
    SoapCall {
        soapaction: format!("\"{service_type}#{action}\""),
        body,
    }
}

/// A control request for an action of the table, its input arguments put in
/// SCPD order. `Err` names what is wrong: an action the service does not
/// have, an input missing, an argument the action does not take.
pub fn soap_request(
    service: Service,
    action: &str,
    arguments: &[(&str, &str)],
) -> Result<SoapCall, String> {
    let entry = table(service)
        .action(action)
        .ok_or_else(|| format!("{} has no action {action}", service.service_type()))?;
    let mut ordered = Vec::new();
    for input in entry.inputs() {
        let value = arguments
            .iter()
            .find(|(n, _)| *n == input.name)
            .ok_or_else(|| format!("{action} needs the argument {}", input.name))?;
        ordered.push(*value);
    }
    if let Some((extra, _)) = arguments
        .iter()
        .find(|(n, _)| !entry.inputs().any(|i| i.name == *n))
    {
        return Err(format!("{action} takes no argument {extra}"));
    }
    Ok(soap_request_raw(service.service_type(), action, &ordered))
}

/// What came back from a control request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SoapReply {
    /// A `200 OK` body: the action it answers and the out arguments in the
    /// order sent, each unescaped once.
    Response {
        /// The action name (the response element's name without `Response`).
        action: String,
        /// The out arguments.
        values: Vec<(String, String)>,
    },
    /// A `500` body: the UPnPError.
    Fault {
        /// `errorCode`.
        code: u16,
        /// `errorDescription`.
        description: String,
    },
}

impl SoapReply {
    /// The out argument of that name, for a response.
    pub fn value(&self, name: &str) -> Option<&str> {
        match self {
            SoapReply::Response { values, .. } => values
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, v)| v.as_str()),
            SoapReply::Fault { .. } => None,
        }
    }

    /// The error code, for a fault.
    pub fn fault_code(&self) -> Option<u16> {
        match self {
            SoapReply::Fault { code, .. } => Some(*code),
            SoapReply::Response { .. } => None,
        }
    }
}

const NOT_SOAP: XmlError = XmlError::Malformed("not a SOAP response");

/// Reads the body of a control response or fault (UDA11 sections 3.2.2 and
/// 3.2.5).
pub fn parse_soap_reply(body: &str) -> Result<SoapReply, XmlError> {
    let root = xml::parse(body, Limits::DEFAULT)?;
    if root.local != "Envelope" {
        return Err(NOT_SOAP);
    }
    let first = root
        .child("Body")
        .and_then(|b| b.children.first())
        .ok_or(NOT_SOAP)?;
    if first.local == "Fault" {
        let error = first
            .find("UPnPError")
            .filter(|e| e.namespace.as_deref() == Some(CONTROL_NS))
            .ok_or(NOT_SOAP)?;
        let code = error
            .child_text("errorCode")
            .and_then(|c| c.parse().ok())
            .ok_or(NOT_SOAP)?;
        return Ok(SoapReply::Fault {
            code,
            description: error
                .child_text("errorDescription")
                .unwrap_or("")
                .to_string(),
        });
    }
    let action = first.local.strip_suffix("Response").ok_or(NOT_SOAP)?;
    Ok(SoapReply::Response {
        action: action.to_string(),
        values: first
            .children
            .iter()
            .map(|c| (c.local.clone(), c.text.clone()))
            .collect(),
    })
}

fn timeout_line(timeout_s: Option<u32>) -> String {
    timeout_s.map_or_else(String::new, |t| format!("TIMEOUT: Second-{t}\r\n"))
}

/// A `SUBSCRIBE` for a new subscription (UDA11 section 4.1.2). `path` is the
/// service's event subscription URL's path, `host` the renderer's
/// `address:port`, `callback` the URL events are to be sent to.
pub fn subscribe_request(path: &str, host: &str, callback: &str, timeout_s: Option<u32>) -> String {
    format!(
        "SUBSCRIBE {path} HTTP/1.1\r\nHOST: {host}\r\nCALLBACK: <{callback}>\r\nNT: upnp:event\r\n{}\r\n",
        timeout_line(timeout_s)
    )
}

/// A `SUBSCRIBE` that renews a subscription (UDA11 section 4.1.3).
pub fn renew_request(path: &str, host: &str, sid: &str, timeout_s: Option<u32>) -> String {
    format!(
        "SUBSCRIBE {path} HTTP/1.1\r\nHOST: {host}\r\nSID: {sid}\r\n{}\r\n",
        timeout_line(timeout_s)
    )
}

/// An `UNSUBSCRIBE` (UDA11 section 4.1.4).
pub fn unsubscribe_request(path: &str, host: &str, sid: &str) -> String {
    format!("UNSUBSCRIBE {path} HTTP/1.1\r\nHOST: {host}\r\nSID: {sid}\r\n\r\n")
}

/// The variables of an event message's body (UDA11 section 4.3.2), each
/// value unescaped once: for AVTransport and RenderingControl one
/// `LastChange` whose value is the `Event` document.
pub fn parse_propertyset(body: &str) -> Result<Vec<(String, String)>, XmlError> {
    let root = xml::parse(body, Limits::DEFAULT)?;
    if root.local != "propertyset" {
        return Err(XmlError::Malformed("not a propertyset"));
    }
    Ok(root
        .children_named("property")
        .flat_map(|p| p.children.iter())
        .map(|v| (v.local.clone(), v.text.clone()))
        .collect())
}

/// One variable of a LastChange `Event` document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VarChange {
    /// The `InstanceID` it belongs to.
    pub instance: u32,
    /// The variable's name.
    pub name: String,
    /// The `channel` attribute, for RenderingControl's channel variables.
    pub channel: Option<String>,
    /// The `val` attribute, unescaped once.
    pub value: String,
}

/// A LastChange `Event` document, read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LastChange {
    /// The `Event` element's namespace.
    pub namespace: Option<String>,
    /// Every variable in document order.
    pub changes: Vec<VarChange>,
}

impl LastChange {
    /// The value of a variable of instance 0 (of any channel).
    pub fn get(&self, name: &str) -> Option<&str> {
        self.changes
            .iter()
            .find(|c| c.instance == 0 && c.name == name)
            .map(|c| c.value.as_str())
    }
}

/// Reads a LastChange value (AVT1 section 5, RCS1 section 5).
pub fn parse_last_change(event: &str) -> Result<LastChange, XmlError> {
    let root = xml::parse(event, Limits::DEFAULT)?;
    if root.local != "Event" {
        return Err(XmlError::Malformed("not an Event document"));
    }
    let mut changes = Vec::new();
    for instance in root.children_named("InstanceID") {
        let id = instance
            .attr("val")
            .and_then(|v| v.trim().parse().ok())
            .ok_or(XmlError::Malformed("an InstanceID without a number"))?;
        for var in &instance.children {
            changes.push(VarChange {
                instance: id,
                name: var.local.clone(),
                channel: var.attr("channel").map(str::to_string),
                value: var
                    .attr("val")
                    .ok_or(XmlError::Malformed("a variable without val"))?
                    .to_string(),
            });
        }
    }
    Ok(LastChange {
        namespace: root.namespace,
        changes,
    })
}

/// One `service` element of a device description.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceUrls {
    /// `serviceType`.
    pub service_type: String,
    /// `serviceId`.
    pub service_id: String,
    /// `SCPDURL`, as written (relative to the description's URL).
    pub scpd_url: String,
    /// `controlURL`, as written.
    pub control_url: String,
    /// `eventSubURL`, as written.
    pub event_sub_url: String,
}

/// A device description, read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceDescription {
    /// The root element's `configId`.
    pub config_id: Option<u32>,
    /// `specVersion` as (major, minor).
    pub spec_version: Option<(u32, u32)>,
    /// Whether a `URLBase` element is present (it must not be, in UDA 1.1).
    pub has_url_base: bool,
    /// `deviceType`.
    pub device_type: String,
    /// `friendlyName`.
    pub friendly_name: String,
    /// `manufacturer`.
    pub manufacturer: String,
    /// `modelName`.
    pub model_name: String,
    /// `UDN`, with its `uuid:` prefix.
    pub udn: String,
    /// The names of the `device` element's children in document order.
    pub element_order: Vec<String>,
    /// The services.
    pub services: Vec<ServiceUrls>,
}

impl DeviceDescription {
    /// The URLs of a service, by its type.
    pub fn service(&self, service: Service) -> Option<&ServiceUrls> {
        self.services
            .iter()
            .find(|s| s.service_type == service.service_type())
    }
}

fn text(node: &Node, child: &str) -> String {
    node.child_text(child).unwrap_or("").to_string()
}

/// Reads a device description (UDA11 section 2.3).
pub fn parse_description(document: &str) -> Result<DeviceDescription, XmlError> {
    let root = xml::parse(document, Limits::DEFAULT)?;
    let device = root
        .child("device")
        .filter(|_| root.local == "root")
        .ok_or(XmlError::Malformed("not a device description"))?;
    let version = root.child("specVersion").and_then(|v| {
        Some((
            v.child_text("major")?.parse().ok()?,
            v.child_text("minor")?.parse().ok()?,
        ))
    });
    Ok(DeviceDescription {
        config_id: root.attr("configId").and_then(|c| c.parse().ok()),
        spec_version: version,
        has_url_base: root.child("URLBase").is_some(),
        device_type: text(device, "deviceType"),
        friendly_name: text(device, "friendlyName"),
        manufacturer: text(device, "manufacturer"),
        model_name: text(device, "modelName"),
        udn: text(device, "UDN"),
        element_order: device.children.iter().map(|c| c.local.clone()).collect(),
        services: device
            .child("serviceList")
            .map(|list| {
                list.children_named("service")
                    .map(|s| ServiceUrls {
                        service_type: text(s, "serviceType"),
                        service_id: text(s, "serviceId"),
                        scpd_url: text(s, "SCPDURL"),
                        control_url: text(s, "controlURL"),
                        event_sub_url: text(s, "eventSubURL"),
                    })
                    .collect()
            })
            .unwrap_or_default(),
    })
}

/// One argument of an action in a service description.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScpdArgument {
    /// `name`.
    pub name: String,
    /// `direction`: `in` or `out`.
    pub direction: String,
    /// `relatedStateVariable`.
    pub related: String,
}

/// One action in a service description.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScpdAction {
    /// `name`.
    pub name: String,
    /// The arguments in document order.
    pub arguments: Vec<ScpdArgument>,
}

/// One state variable in a service description.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScpdVariable {
    /// `name`.
    pub name: String,
    /// `dataType`.
    pub data_type: String,
    /// `sendEvents`: `yes` unless the attribute says `no` (UDA11 section
    /// 2.5: the default is yes).
    pub send_events: bool,
    /// The `allowedValueList`, empty when there is none.
    pub allowed_values: Vec<String>,
    /// The `allowedValueRange` as (minimum, maximum, step).
    pub range: Option<(String, String, Option<String>)>,
}

/// A service description, read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scpd {
    /// The root element's `configId`.
    pub config_id: Option<u32>,
    /// `specVersion` as (major, minor).
    pub spec_version: Option<(u32, u32)>,
    /// The actions in document order.
    pub actions: Vec<ScpdAction>,
    /// The state variables in document order.
    pub variables: Vec<ScpdVariable>,
}

impl Scpd {
    /// The action of that name.
    pub fn action(&self, name: &str) -> Option<&ScpdAction> {
        self.actions.iter().find(|a| a.name == name)
    }

    /// The state variable of that name.
    pub fn variable(&self, name: &str) -> Option<&ScpdVariable> {
        self.variables.iter().find(|v| v.name == name)
    }
}

/// Reads a service description (UDA11 section 2.5).
pub fn parse_scpd(document: &str) -> Result<Scpd, XmlError> {
    let root = xml::parse(document, Limits::DEFAULT)?;
    if root.local != "scpd" {
        return Err(XmlError::Malformed("not a service description"));
    }
    let actions = root
        .child("actionList")
        .map(|list| {
            list.children_named("action")
                .map(|a| ScpdAction {
                    name: text(a, "name"),
                    arguments: a
                        .child("argumentList")
                        .map(|args| {
                            args.children_named("argument")
                                .map(|arg| ScpdArgument {
                                    name: text(arg, "name"),
                                    direction: text(arg, "direction"),
                                    related: text(arg, "relatedStateVariable"),
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default();
    let variables = root
        .child("serviceStateTable")
        .map(|t| {
            t.children_named("stateVariable")
                .map(|v| ScpdVariable {
                    name: text(v, "name"),
                    data_type: text(v, "dataType"),
                    send_events: v.attr("sendEvents") != Some("no"),
                    allowed_values: v
                        .child("allowedValueList")
                        .map(|l| {
                            l.children_named("allowedValue")
                                .map(|a| a.text.trim().to_string())
                                .collect()
                        })
                        .unwrap_or_default(),
                    range: v.child("allowedValueRange").map(|r| {
                        (
                            text(r, "minimum"),
                            text(r, "maximum"),
                            r.child_text("step").map(str::to_string),
                        )
                    }),
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Scpd {
        config_id: root.attr("configId").and_then(|c| c.parse().ok()),
        spec_version: root.child("specVersion").and_then(|v| {
            Some((
                v.child_text("major")?.parse().ok()?,
                v.child_text("minor")?.parse().ok()?,
            ))
        }),
        actions,
        variables,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::description::{device_description, scpd, DeviceInfo};
    use crate::gena::{self, Subscribe};
    use crate::lastchange::{event_xml, Change, AVT_NS, RCS_NS};
    use crate::soap::{build_fault, build_response, parse_request, validate};
    use crate::{error, Service};

    #[test]
    fn an_msearch_is_read_by_the_device_side() {
        let m = msearch("urn:schemas-upnp-org:device:MediaRenderer:1", 2);
        let s = ssdp::parse_search(m.as_bytes(), true).unwrap();
        assert_eq!(s.st, "urn:schemas-upnp-org:device:MediaRenderer:1");
        assert_eq!(s.window_ms, 2000);
        assert_eq!(parse_ssdp(m.as_bytes()), None);
    }

    #[test]
    fn the_devices_discovery_messages_are_read() {
        let advert = ssdp::Advert {
            udn: Uuid::parse("3b8fa6e6-bb30-5005-b768-3e87f0af9a9a").unwrap(),
            location: "http://192.0.2.10:49200/upnp/3b8fa6e6-bb30-5005-b768-3e87f0af9a9a/desc.xml"
                .into(),
            server: ssdp::server_token("Linux", "6.12", "0.1.0"),
            max_age_s: 1800,
            boot_id: 17,
            config_id: 99,
            search_port: Some(49_201),
            openhome: false,
        };
        for (i, m) in ssdp::alive_set(&advert).iter().enumerate() {
            let p = parse_ssdp(m.as_bytes()).unwrap();
            assert_eq!(p.kind, SsdpKind::Alive);
            assert_eq!(p.udn(), Some(advert.udn));
            assert_eq!(p.target, ssdp::targets(&advert.udn, false)[i].0);
            assert_eq!(p.usn, ssdp::targets(&advert.udn, false)[i].1);
            assert_eq!(p.location.as_deref(), Some(advert.location.as_str()));
            assert_eq!(p.max_age_s, Some(1800));
            assert_eq!(
                (p.boot_id, p.config_id, p.search_port),
                (Some(17), Some(99), Some(49_201))
            );
            assert_eq!(
                p.server.as_deref(),
                Some("Linux/6.12 UPnP/1.1 chorus/0.1.0")
            );
        }
        for m in ssdp::byebye_set(&advert) {
            let p = parse_ssdp(m.as_bytes()).unwrap();
            assert_eq!(p.kind, SsdpKind::Byebye);
            assert_eq!((p.location, p.max_age_s), (None, None));
            assert_eq!(p.boot_id, Some(17));
        }
        let search = ssdp::Search {
            st: "ssdp:all".into(),
            window_ms: 1000,
        };
        let responses = ssdp::search_responses(&search, &advert, None);
        assert_eq!(responses.len(), 6);
        for r in responses {
            let p = parse_ssdp(r.as_bytes()).unwrap();
            assert_eq!(p.kind, SsdpKind::SearchResponse);
            assert_eq!(p.headers.get("EXT"), Some(""));
            assert_eq!(p.udn(), Some(advert.udn));
        }
        assert_eq!(
            parse_ssdp(b"NOTIFY * HTTP/1.1\r\nNTS: ssdp:other\r\nNT: x\r\nUSN: y\r\n\r\n"),
            None
        );
        assert_eq!(parse_ssdp(b"HTTP/1.1 404 Not Found\r\n\r\n"), None);
        assert_eq!(parse_ssdp(b"\xff"), None);
        let update =
            parse_ssdp(b"NOTIFY * HTTP/1.1\r\nNTS: ssdp:update\r\nNT: x\r\nUSN: nope\r\n\r\n")
                .unwrap();
        assert_eq!(update.kind, SsdpKind::Update);
        assert_eq!(update.udn(), None);
    }

    #[test]
    fn every_action_of_every_table_round_trips_through_soap() {
        for service in Service::ALL {
            for action in table(service).actions {
                let values: Vec<(&str, String)> = action
                    .inputs()
                    .enumerate()
                    .map(|(i, a)| (a.name, format!("v{i} & <{}>", a.name)))
                    .collect();
                let args: Vec<(&str, &str)> =
                    values.iter().map(|(n, v)| (*n, v.as_str())).collect();
                let call = soap_request(service, action.name, &args).unwrap();
                assert_eq!(
                    call.soapaction,
                    format!("\"{}#{}\"", service.service_type(), action.name)
                );
                let request = parse_request(Some(&call.soapaction), &call.body).unwrap();
                let invocation = validate(service, &request).unwrap();
                assert_eq!(invocation.action.name, action.name);
                let expected: Vec<&str> = values.iter().map(|(_, v)| v.as_str()).collect();
                assert_eq!(invocation.inputs, expected);
                // And the response with every out argument set.
                let out: Vec<(&str, String)> = action
                    .outputs()
                    .map(|a| (a.name, format!("<{}> & \"x\"", a.name)))
                    .collect();
                let reply = parse_soap_reply(&build_response(service, action.name, &out)).unwrap();
                let SoapReply::Response {
                    action: name,
                    values,
                } = &reply
                else {
                    panic!("a response")
                };
                assert_eq!(name, action.name);
                assert_eq!(values.len(), out.len());
                for (n, v) in &out {
                    assert_eq!(reply.value(n), Some(v.as_str()));
                }
                assert_eq!(reply.fault_code(), None);
            }
        }
    }

    #[test]
    fn the_checked_builder_refuses_what_the_table_refuses() {
        assert!(soap_request(Service::AvTransport, "Record", &[("InstanceID", "0")]).is_err());
        assert!(soap_request(Service::AvTransport, "Play", &[("InstanceID", "0")]).is_err());
        assert!(soap_request(
            Service::AvTransport,
            "Stop",
            &[("InstanceID", "0"), ("Speed", "1")]
        )
        .is_err());
        // Arguments in any order are put in the table's.
        let call = soap_request(
            Service::AvTransport,
            "Play",
            &[("Speed", "1"), ("InstanceID", "0")],
        )
        .unwrap();
        assert!(call
            .body
            .contains("<InstanceID>0</InstanceID><Speed>1</Speed>"));
        // The raw builder sends anything.
        let raw = soap_request_raw("urn:x", "Nope", &[]);
        let request = parse_request(Some(&raw.soapaction), &raw.body).unwrap();
        assert_eq!(
            validate(Service::AvTransport, &request),
            Err(error::INVALID_ACTION)
        );
    }

    #[test]
    fn faults_are_read() {
        for e in error::AVT_ALL {
            let reply = parse_soap_reply(&build_fault(&e)).unwrap();
            assert_eq!(
                reply,
                SoapReply::Fault {
                    code: e.code,
                    description: e.description.to_string()
                }
            );
            assert_eq!(reply.fault_code(), Some(e.code));
            assert_eq!(reply.value("x"), None);
        }
        for bad in [
            "<a/>",
            "<Envelope><Body/></Envelope>",
            "<Envelope><Body><Fault/></Body></Envelope>",
            "<Envelope><Body><Other/></Body></Envelope>",
        ] {
            assert_eq!(parse_soap_reply(bad), Err(NOT_SOAP), "{bad}");
        }
        assert_eq!(parse_soap_reply("<!DOCTYPE a><a/>"), Err(XmlError::Doctype));
    }

    #[test]
    fn subscription_requests_are_read_by_the_device_side() {
        let parse = |request: &str| {
            let (start, headers) = Headers::parse(request);
            (start.to_string(), headers)
        };
        let (start, h) = parse(&subscribe_request(
            "/upnp/u/avt/event",
            "192.0.2.10:49200",
            "http://192.0.2.50:49152/cb",
            Some(300),
        ));
        assert_eq!(start, "SUBSCRIBE /upnp/u/avt/event HTTP/1.1");
        assert_eq!(
            gena::parse_subscribe(&h),
            Ok(Subscribe::New {
                callbacks: vec!["http://192.0.2.50:49152/cb".into()],
                timeout_s: Some(300)
            })
        );
        let (_, h) = parse(&subscribe_request("/e", "h", "http://192.0.2.50/cb", None));
        assert_eq!(
            gena::parse_subscribe(&h),
            Ok(Subscribe::New {
                callbacks: vec!["http://192.0.2.50/cb".into()],
                timeout_s: None
            })
        );
        let (_, h) = parse(&renew_request("/e", "h", "uuid:abc", Some(1800)));
        assert_eq!(
            gena::parse_subscribe(&h),
            Ok(Subscribe::Renew {
                sid: "uuid:abc".into(),
                timeout_s: Some(1800)
            })
        );
        let (start, h) = parse(&unsubscribe_request("/e", "h", "uuid:abc"));
        assert_eq!(start, "UNSUBSCRIBE /e HTTP/1.1");
        assert_eq!(gena::parse_unsubscribe(&h), Ok("uuid:abc".into()));
    }

    #[test]
    fn event_bodies_and_last_change_are_read() {
        let didl = "<DIDL-Lite><dc:title>A &amp; B</dc:title></DIDL-Lite>";
        let avt = event_xml(
            AVT_NS,
            &[
                Change::new("TransportState", "PLAYING"),
                Change::new("AVTransportURIMetaData", didl),
            ],
        );
        let body = gena::propertyset(&[("LastChange", &avt)]);
        let props = parse_propertyset(&body).unwrap();
        assert_eq!(props, [("LastChange".to_string(), avt.clone())]);
        let lc = parse_last_change(&props[0].1).unwrap();
        assert_eq!(lc.namespace.as_deref(), Some(AVT_NS));
        assert_eq!(lc.get("TransportState"), Some("PLAYING"));
        assert_eq!(lc.get("AVTransportURIMetaData"), Some(didl));
        assert_eq!(lc.get("Nope"), None);
        let rcs = parse_last_change(&event_xml(RCS_NS, &[Change::master("Volume", "20")])).unwrap();
        assert_eq!(
            rcs.changes,
            [VarChange {
                instance: 0,
                name: "Volume".into(),
                channel: Some("Master".into()),
                value: "20".into()
            }]
        );
        // ConnectionManager: three direct properties.
        let cm = crate::connmgr::evented();
        let pairs: Vec<(&str, &str)> = cm.iter().map(|(n, v)| (*n, v.as_str())).collect();
        let props = parse_propertyset(&gena::propertyset(&pairs)).unwrap();
        assert_eq!(props.len(), 3);
        assert_eq!(
            props[2],
            ("CurrentConnectionIDs".to_string(), "0".to_string())
        );
        assert!(parse_propertyset("<a/>").is_err());
        assert!(parse_last_change("<a/>").is_err());
        assert!(
            parse_last_change("<Event><InstanceID><X val=\"1\"/></InstanceID></Event>").is_err()
        );
        assert!(
            parse_last_change("<Event><InstanceID val=\"0\"><X/></InstanceID></Event>").is_err()
        );
    }

    #[test]
    fn the_description_and_the_scpds_are_read_back_as_the_tables() {
        let info = DeviceInfo {
            udn: Uuid::parse("3b8fa6e6-bb30-5005-b768-3e87f0af9a9a").unwrap(),
            friendly_name: "Tom & Jerry".into(),
            model_name: "chorus room".into(),
            model_number: "0.1.0".into(),
            openhome: false,
        };
        let d = parse_description(&device_description(&info, 4242)).unwrap();
        assert_eq!(d.config_id, Some(4242));
        assert_eq!(d.spec_version, Some((1, 1)));
        assert!(!d.has_url_base);
        assert_eq!(d.device_type, crate::DEVICE_TYPE);
        assert_eq!(d.friendly_name, "Tom & Jerry");
        assert_eq!(d.manufacturer, "chorus");
        assert_eq!(d.udn, "uuid:3b8fa6e6-bb30-5005-b768-3e87f0af9a9a");
        assert_eq!(d.services.len(), 3);
        for service in Service::AV {
            let urls = d.service(service).unwrap();
            assert_eq!(urls.service_id, service.service_id());
            for url in [&urls.scpd_url, &urls.control_url, &urls.event_sub_url] {
                assert!(
                    url.starts_with("/upnp/3b8fa6e6-bb30-5005-b768-3e87f0af9a9a/"),
                    "{url}"
                );
                assert!(crate::description::route(url).is_some());
            }
            // The SCPD text says exactly what the table says.
            let s = parse_scpd(&scpd(service, 4242)).unwrap();
            assert_eq!(s.config_id, Some(4242));
            assert_eq!(s.spec_version, Some((1, 1)));
            let t = table(service);
            assert_eq!(s.actions.len(), t.actions.len());
            for (read, entry) in s.actions.iter().zip(t.actions) {
                assert_eq!(read.name, entry.name);
                assert_eq!(read.arguments.len(), entry.args.len());
                for (a, b) in read.arguments.iter().zip(entry.args) {
                    assert_eq!(a.name, b.name);
                    assert_eq!(a.direction == "in", b.dir == crate::description::Dir::In);
                    assert_eq!(a.related, b.var);
                    assert!(s.variable(&a.related).is_some());
                }
            }
            assert_eq!(s.variables.len(), t.variables.len());
            for (read, entry) in s.variables.iter().zip(t.variables) {
                assert_eq!(read.name, entry.name);
                assert_eq!(read.data_type, entry.data_type);
                assert_eq!(read.send_events, entry.evented);
            }
            assert!(s.action("Nope").is_none());
        }
        assert!(parse_description("<scpd/>").is_err());
        assert!(parse_scpd("<root/>").is_err());
    }
}
