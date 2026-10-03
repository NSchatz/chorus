//! Control: SOAP action requests, responses and the UPnPError fault.
//!
//! UDA11 section 3.2. A control point POSTs an envelope to a service's
//! control URL; the first child of `Body` is the action element, named after
//! the action, in the service type's namespace, and its children are the
//! arguments. The HTTP side (the method, `CONTENT-TYPE`, the body length)
//! is the server's; this module takes the `SOAPACTION` header's value and
//! the body's text.
//!
//! Tolerance, as UDA11 section 3.2.1 demands ("a device MUST accept action
//! invocations that use other legal XML namespace prefixes") and as real
//! control points need: any prefix or a default namespace on the action
//! element, `SOAPACTION` with or without its double quotes, arguments in any
//! order, unknown elements ignored. Where the header and the body disagree
//! about the action, the body is believed: it is what carries the arguments.

use crate::description::{table, Action};
use crate::xml::{self, escape_text, Limits, Node, XmlError};
use crate::{error, Service, UpnpError};

/// The SOAP 1.1 envelope namespace (UDA11 section 3.2.1).
pub const ENVELOPE_NS: &str = "http://schemas.xmlsoap.org/soap/envelope/";
/// The SOAP 1.1 encoding style every UPnP envelope names.
pub const ENCODING_NS: &str = "http://schemas.xmlsoap.org/soap/encoding/";
/// The namespace of the `UPnPError` element (UDA11 section 3.2.5).
pub const CONTROL_NS: &str = "urn:schemas-upnp-org:control-1-0";
/// The `CONTENT-TYPE` of every SOAP message chorus sends (UDA11 section
/// 3.2.2).
pub const CONTENT_TYPE: &str = "text/xml; charset=\"utf-8\"";

/// An action request as it arrived, before it is held to the table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionRequest {
    /// The service type the request names: the action element's namespace,
    /// or the `SOAPACTION` header's when the element has none.
    pub service_type: String,
    /// The action's name: the local name of the first child of `Body`.
    pub action: String,
    /// The argument elements in the order sent, each value unescaped once.
    pub arguments: Vec<(String, String)>,
}

impl ActionRequest {
    /// The value of the first argument of that name.
    pub fn argument(&self, name: &str) -> Option<&str> {
        self.arguments
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// Why a request body is not an action request. The server answers these
/// with HTTP 400; they are not UPnP errors, since no action was understood.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoapError {
    /// The body is not XML this crate reads (a DOCTYPE included).
    Xml(XmlError),
    /// The XML is not a SOAP envelope with a body and an action element.
    NotAnEnvelope,
}

/// Splits a `SOAPACTION` value into service type and action name: "the
/// service type, hash mark, and name of action to be invoked, all enclosed
/// in double quotes" (UDA11 section 3.2.1). The quotes may be missing.
pub fn parse_soapaction(value: &str) -> Option<(&str, &str)> {
    let v = value.trim();
    let v = v
        .strip_prefix('"')
        .map(|s| s.strip_suffix('"').unwrap_or(s))
        .unwrap_or(v);
    let (service_type, action) = v.rsplit_once('#')?;
    if service_type.is_empty() || action.is_empty() {
        return None;
    }
    Some((service_type, action))
}

/// The value of an argument element. Text is unescaped once by the reader
/// (so escaped DIDL-Lite arrives as the DIDL-Lite document, and a CDATA
/// section arrives as written). An argument sent as real child elements,
/// which some control points do with metadata, is returned as its markup.
fn argument_value(node: &Node) -> String {
    if node.children.is_empty() {
        node.text.clone()
    } else {
        node.inner_xml.trim().to_string()
    }
}

/// Reads an action request from the `SOAPACTION` header's value (if the
/// request had one) and the body.
pub fn parse_request(soapaction: Option<&str>, body: &str) -> Result<ActionRequest, SoapError> {
    let root = xml::parse(body, Limits::DEFAULT).map_err(SoapError::Xml)?;
    if root.local != "Envelope" {
        return Err(SoapError::NotAnEnvelope);
    }
    let action = root
        .child("Body")
        .and_then(|b| b.children.first())
        .ok_or(SoapError::NotAnEnvelope)?;
    let from_header = soapaction.and_then(parse_soapaction);
    let service_type = match (&action.namespace, from_header) {
        (Some(ns), _) => ns.clone(),
        (None, Some((service_type, _))) => service_type.to_string(),
        (None, None) => String::new(),
    };
    Ok(ActionRequest {
        service_type,
        action: action.local.clone(),
        arguments: action
            .children
            .iter()
            .map(|c| (c.local.clone(), argument_value(c)))
            .collect(),
    })
}

/// A request that passed the table: the action and its input values in the
/// table's order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invocation {
    /// The action's table entry.
    pub action: &'static Action,
    /// The input argument values, one per input in SCPD order.
    pub inputs: Vec<String>,
}

impl Invocation {
    /// The value of the input argument of that name. Panics when the action
    /// has no such input: the names come from the same table.
    pub fn input(&self, name: &str) -> &str {
        let at = self
            .action
            .inputs()
            .position(|a| a.name == name)
            .unwrap_or_else(|| panic!("{} has no input {name}", self.action.name));
        &self.inputs[at]
    }
}

/// Holds a request to the table of the service whose control URL it was
/// sent to (UDA11 section 3.2.1 and table 3-3):
///
/// - a service type other than this service's, or an action the service
///   does not list: 401 Invalid Action;
/// - an input argument missing ("Every 'in' argument in the definition of
///   the action in the service description MUST be" sent): 402 Invalid Args;
/// - elements that are not arguments of the action are ignored, and so is
///   the order.
pub fn validate(service: Service, request: &ActionRequest) -> Result<Invocation, UpnpError> {
    if request.service_type != service.service_type() {
        return Err(error::INVALID_ACTION);
    }
    let action = table(service)
        .action(&request.action)
        .ok_or(error::INVALID_ACTION)?;
    let inputs = action
        .inputs()
        .map(|arg| request.argument(arg.name).map(str::to_string))
        .collect::<Option<Vec<String>>>()
        .ok_or(error::INVALID_ARGS)?;
    Ok(Invocation { action, inputs })
}

fn envelope(body: &str) -> String {
    format!(
        "<?xml version=\"1.0\"?>\n<s:Envelope xmlns:s=\"{ENVELOPE_NS}\" s:encodingStyle=\"{ENCODING_NS}\"><s:Body>{body}</s:Body></s:Envelope>"
    )
}

/// The body of a successful action response (UDA11 section 3.2.2): the
/// element `<action>Response` in the service type's namespace, holding the
/// out arguments in the order given, each value escaped once. An action
/// with no out arguments gets the empty element.
pub fn build_response(service: Service, action: &str, out: &[(&str, String)]) -> String {
    let mut inner = format!(
        "<u:{action}Response xmlns:u=\"{}\">",
        service.service_type()
    );
    for (name, value) in out {
        inner.push_str(&format!("<{name}>{}</{name}>", escape_text(value)));
    }
    inner.push_str(&format!("</u:{action}Response>"));
    envelope(&inner)
}

/// The body of an error response, sent with HTTP status 500 (UDA11 section
/// 3.2.5): `faultcode` "MUST be 'Client'" in the envelope's namespace,
/// `faultstring` "MUST be 'UPnPError'", and the code and its description in
/// the `detail`.
pub fn build_fault(error: &UpnpError) -> String {
    envelope(&format!(
        "<s:Fault><faultcode>s:Client</faultcode><faultstring>UPnPError</faultstring><detail><UPnPError xmlns=\"{CONTROL_NS}\"><errorCode>{}</errorCode><errorDescription>{}</errorDescription></UPnPError></detail></s:Fault>",
        error.code,
        escape_text(error.description)
    ))
}

/// Whether a `CONTENT-TYPE` value is the `text/xml` a control request must
/// carry (UDA11 section 3.2.1; anything else is answered "415 Unsupported
/// Media Type"). The charset parameter and its quoting are not checked.
pub fn is_xml_content_type(value: &str) -> bool {
    value
        .split(';')
        .next()
        .is_some_and(|t| t.trim().eq_ignore_ascii_case("text/xml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const AVT: &str = "urn:schemas-upnp-org:service:AVTransport:1";

    fn play(prefix_open: &str, prefix_close: &str) -> String {
        format!(
            "<?xml version=\"1.0\"?><s:Envelope xmlns:s=\"{ENVELOPE_NS}\" s:encodingStyle=\"{ENCODING_NS}\"><s:Body>{prefix_open}<InstanceID>0</InstanceID><Speed>1</Speed>{prefix_close}</s:Body></s:Envelope>"
        )
    }

    #[test]
    fn soapaction_is_read_quoted_and_unquoted() {
        let q = format!("\"{AVT}#Play\"");
        assert_eq!(parse_soapaction(&q), Some((AVT, "Play")));
        let u = format!("{AVT}#Play");
        assert_eq!(parse_soapaction(&u), Some((AVT, "Play")));
        let half = format!("\"{AVT}#Play");
        assert_eq!(parse_soapaction(&half), Some((AVT, "Play")));
        assert_eq!(
            parse_soapaction(&format!("  \"{AVT}#Play\"  ")),
            Some((AVT, "Play"))
        );
        for bad in ["", "\"\"", "Play", "#Play", "urn:x#", "\"#\""] {
            assert_eq!(parse_soapaction(bad), None, "{bad}");
        }
    }

    #[test]
    fn any_prefix_or_a_default_namespace_names_the_action() {
        let forms = [
            play(&format!("<u:Play xmlns:u=\"{AVT}\">"), "</u:Play>"),
            play(&format!("<m:Play xmlns:m=\"{AVT}\">"), "</m:Play>"),
            play(&format!("<Play xmlns=\"{AVT}\">"), "</Play>"),
        ];
        for body in &forms {
            let r = parse_request(None, body).unwrap();
            assert_eq!(r.service_type, AVT);
            assert_eq!(r.action, "Play");
            assert_eq!(
                r.arguments,
                [
                    ("InstanceID".into(), "0".into()),
                    ("Speed".into(), "1".into())
                ]
            );
            let inv = validate(Service::AvTransport, &r).unwrap();
            assert_eq!(inv.action.name, "Play");
            assert_eq!(inv.inputs, ["0", "1"]);
            assert_eq!(inv.input("Speed"), "1");
        }
        // No namespace on the element: the header supplies the service type.
        let bare = play("<Play>", "</Play>");
        let r = parse_request(Some(&format!("{AVT}#Play")), &bare).unwrap();
        assert_eq!(r.service_type, AVT);
        // And with neither, the request names no service: 401.
        let r = parse_request(None, &bare).unwrap();
        assert_eq!(
            validate(Service::AvTransport, &r),
            Err(error::INVALID_ACTION)
        );
    }

    #[test]
    fn the_body_is_believed_over_the_header() {
        let body = play(&format!("<u:Play xmlns:u=\"{AVT}\">"), "</u:Play>");
        let r = parse_request(Some(&format!("\"{AVT}#Stop\"")), &body).unwrap();
        assert_eq!(r.action, "Play");
    }

    #[test]
    fn the_table_decides_401_and_402() {
        let req = |action: &str, args: &[(&str, &str)]| ActionRequest {
            service_type: AVT.into(),
            action: action.into(),
            arguments: args
                .iter()
                .map(|(n, v)| (n.to_string(), v.to_string()))
                .collect(),
        };
        // Unknown action, and actions chorus refuses by leaving them out.
        for name in [
            "Nope",
            "Record",
            "SetPlayMode",
            "SetRecordQualityMode",
            "play",
        ] {
            assert_eq!(
                validate(Service::AvTransport, &req(name, &[("InstanceID", "0")])),
                Err(error::INVALID_ACTION),
                "{name}"
            );
        }
        // A missing input.
        assert_eq!(
            validate(Service::AvTransport, &req("Play", &[("InstanceID", "0")])),
            Err(error::INVALID_ARGS)
        );
        assert_eq!(
            validate(Service::AvTransport, &req("Play", &[("Speed", "1")])),
            Err(error::INVALID_ARGS)
        );
        // Any order, extras ignored, the first of a repeated argument wins.
        let inv = validate(
            Service::AvTransport,
            &req(
                "Play",
                &[
                    ("Extra", "x"),
                    ("Speed", "1"),
                    ("InstanceID", "0"),
                    ("Speed", "2"),
                ],
            ),
        )
        .unwrap();
        assert_eq!(inv.inputs, ["0", "1"]);
        // The right action sent to the wrong service's control URL.
        assert_eq!(
            validate(
                Service::RenderingControl,
                &req("Play", &[("InstanceID", "0"), ("Speed", "1")])
            ),
            Err(error::INVALID_ACTION)
        );
        // An action with no inputs.
        let cm = ActionRequest {
            service_type: Service::ConnectionManager.service_type().into(),
            action: "GetProtocolInfo".into(),
            arguments: vec![],
        };
        assert!(validate(Service::ConnectionManager, &cm)
            .unwrap()
            .inputs
            .is_empty());
    }

    #[test]
    fn argument_values_are_unescaped_exactly_once() {
        let didl = "<DIDL-Lite><dc:title>A &amp; B &lt;\"x\"&gt; \u{e9}</dc:title></DIDL-Lite>";
        let uri = "http://192.0.2.50:8000/a?x=1&y=2";
        let forms = [
            // Escaped, as the specification requires.
            format!(
                "<CurrentURIMetaData>{}</CurrentURIMetaData>",
                escape_text(didl)
            ),
            // In a CDATA section.
            format!("<CurrentURIMetaData><![CDATA[{didl}]]></CurrentURIMetaData>"),
            // As real child elements.
            format!("<CurrentURIMetaData>\n {didl}\n</CurrentURIMetaData>"),
        ];
        for meta in forms {
            let body = format!(
                "<s:Envelope xmlns:s=\"{ENVELOPE_NS}\"><s:Body><u:SetAVTransportURI xmlns:u=\"{AVT}\"><InstanceID>0</InstanceID><CurrentURI>{}</CurrentURI>{meta}</u:SetAVTransportURI></s:Body></s:Envelope>",
                escape_text(uri)
            );
            let r = parse_request(None, &body).unwrap();
            assert_eq!(r.argument("CurrentURI"), Some(uri));
            assert_eq!(r.argument("CurrentURIMetaData"), Some(didl), "{body}");
        }
    }

    #[test]
    fn what_is_not_an_action_request_is_an_error() {
        assert_eq!(parse_request(None, "<a/>"), Err(SoapError::NotAnEnvelope));
        assert_eq!(
            parse_request(
                None,
                &format!("<s:Envelope xmlns:s=\"{ENVELOPE_NS}\"><s:Body/></s:Envelope>")
            ),
            Err(SoapError::NotAnEnvelope)
        );
        assert_eq!(
            parse_request(None, &format!("<s:Envelope xmlns:s=\"{ENVELOPE_NS}\"/>")),
            Err(SoapError::NotAnEnvelope)
        );
        assert!(matches!(
            parse_request(None, "not xml"),
            Err(SoapError::Xml(XmlError::Malformed(_)))
        ));
        assert_eq!(
            parse_request(
                None,
                "<!DOCTYPE x [<!ENTITY e SYSTEM \"file:///etc/passwd\">]><s:Envelope/>"
            ),
            Err(SoapError::Xml(XmlError::Doctype))
        );
        let big = format!("<s:Envelope>{}</s:Envelope>", " ".repeat(70_000));
        assert_eq!(
            parse_request(None, &big),
            Err(SoapError::Xml(XmlError::TooLarge))
        );
    }

    #[test]
    fn responses_and_faults_have_the_specifications_shape() {
        let r = build_response(
            Service::RenderingControl,
            "GetVolume",
            &[("CurrentVolume", "20".to_string())],
        );
        assert_eq!(
            r,
            "<?xml version=\"1.0\"?>\n<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body><u:GetVolumeResponse xmlns:u=\"urn:schemas-upnp-org:service:RenderingControl:1\"><CurrentVolume>20</CurrentVolume></u:GetVolumeResponse></s:Body></s:Envelope>"
        );
        let empty = build_response(Service::AvTransport, "Stop", &[]);
        assert!(empty.contains("<u:StopResponse xmlns:u=\"urn:schemas-upnp-org:service:AVTransport:1\"></u:StopResponse>"));
        let escaped = build_response(Service::AvTransport, "X", &[("V", "a<b&c".to_string())]);
        assert!(escaped.contains("<V>a&lt;b&amp;c</V>"));
        let f = build_fault(&error::AVT_INVALID_INSTANCE_ID);
        assert!(f.contains("<faultcode>s:Client</faultcode><faultstring>UPnPError</faultstring>"));
        assert!(f.contains("<UPnPError xmlns=\"urn:schemas-upnp-org:control-1-0\"><errorCode>718</errorCode><errorDescription>Invalid InstanceID</errorDescription></UPnPError>"));
        let f = build_fault(&error::AVT_CONTENT_BUSY);
        assert!(f.contains("Content 'BUSY'"));
    }

    #[test]
    fn the_content_type_check_ignores_parameters() {
        for ok in [
            "text/xml",
            "text/xml; charset=\"utf-8\"",
            "TEXT/XML;charset=utf-8",
            " text/xml ",
        ] {
            assert!(is_xml_content_type(ok), "{ok}");
        }
        for bad in ["text/plain", "application/xml", "", "text/xmlx"] {
            assert!(!is_xml_content_type(bad), "{bad}");
        }
    }
}
