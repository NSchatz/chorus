//! The UPnP golden vectors, `fixtures/upnp/` (Rust-only by declaration: the
//! renderer runs in chorus-server alone, there is no C implementation;
//! `tools/conventions/check-shared-fixtures.sh`, `fixtures/README.md`).
//!
//! Apart from the four generated descriptions, every vector was typed from
//! the specifications' templates, not produced by this crate: the builders
//! must write those bytes and the parsers must read those fields. The one
//! test below walks every kind of vector and then fails if any file in the
//! directory was read by none of it, so a vector cannot sit there unused.

use std::collections::BTreeSet;
use std::net::IpAddr;
use std::path::PathBuf;

use chorus_upnp::avtransport::{AvTransport, Effect};
use chorus_upnp::client;
use chorus_upnp::connmgr;
use chorus_upnp::description::{self, DeviceInfo};
use chorus_upnp::didl;
use chorus_upnp::gena::{self, CallbackRefusal, Cidr, Subscribe};
use chorus_upnp::lastchange::{event_xml, AVT_NS, RCS_NS};
use chorus_upnp::rendering::RenderingControl;
use chorus_upnp::soap::{self, SoapError};
use chorus_upnp::ssdp::{self, Advert, SearchDrop};
use chorus_upnp::uuid::{udn, Target, Uuid, CHORUS_NAMESPACE};
use chorus_upnp::xml::XmlError;
use chorus_upnp::{Headers, Service, UpnpError};

struct Fixtures {
    dir: PathBuf,
    read: BTreeSet<String>,
}

type Fields = Vec<(String, String)>;

fn get<'a>(fields: &'a Fields, key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

fn need<'a>(fields: &'a Fields, key: &str) -> &'a str {
    get(fields, key).unwrap_or_else(|| panic!("the fields have no {key}"))
}

/// `prefix.0`, `prefix.1`, ... in order.
fn list<'a>(fields: &'a Fields, prefix: &str) -> Vec<&'a str> {
    (0..)
        .map_while(|i| get(fields, &format!("{prefix}.{i}")))
        .collect()
}

impl Fixtures {
    fn new() -> Fixtures {
        Fixtures {
            dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/upnp"),
            read: BTreeSet::new(),
        }
    }

    /// The file exactly as committed.
    fn raw(&mut self, name: &str) -> String {
        self.read.insert(name.to_string());
        std::fs::read_to_string(self.dir.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
    }

    /// A document: the file without its final line feed.
    fn document(&mut self, name: &str) -> String {
        let text = self.raw(name);
        text.strip_suffix('\n').unwrap_or(&text).to_string()
    }

    /// Header-only messages: written with LF in the file, CRLF on the wire.
    fn wire(&mut self, name: &str) -> String {
        self.raw(name).replace('\n', "\r\n")
    }

    /// `key = value` lines; `#` starts a comment.
    fn fields(&mut self, name: &str) -> Fields {
        self.raw(name)
            .lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
            .map(|l| {
                let (k, v) = l.split_once('=').unwrap_or_else(|| panic!("{name}: {l}"));
                (k.trim().to_string(), v.trim().to_string())
            })
            .collect()
    }

    /// The stems of the files named `<prefix><stem><suffix>`, sorted.
    fn stems(&self, prefix: &str, suffix: &str) -> Vec<String> {
        let mut out: Vec<String> = self
            .all()
            .into_iter()
            .filter_map(|n| {
                n.strip_prefix(prefix)
                    .and_then(|r| r.strip_suffix(suffix))
                    .map(str::to_string)
            })
            .collect();
        out.sort();
        out
    }

    fn all(&self) -> Vec<String> {
        std::fs::read_dir(&self.dir)
            .expect("fixtures/upnp exists")
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect()
    }
}

fn kitchen() -> DeviceInfo {
    DeviceInfo {
        udn: udn(&CHORUS_NAMESPACE, "srv1", &Target::Room("kitchen")),
        friendly_name: "Kitchen".into(),
        model_name: "chorus room".into(),
        model_number: "0.1.0".into(),
    }
}

fn ssdp_messages(f: &mut Fixtures) {
    let p = f.fields("ssdp-advert.fields");
    let advert = Advert {
        udn: Uuid::parse(need(&p, "udn")).unwrap(),
        location: need(&p, "location").into(),
        server: need(&p, "server").into(),
        max_age_s: need(&p, "max_age_s").parse().unwrap(),
        boot_id: need(&p, "boot_id").parse().unwrap(),
        config_id: need(&p, "config_id").parse().unwrap(),
        search_port: Some(need(&p, "search_port").parse().unwrap()),
    };
    assert_eq!(
        advert.udn,
        kitchen().udn,
        "the vectors are about the kitchen"
    );
    assert_eq!(advert.server, ssdp::server_token("Linux", "6.12", "0.1.0"));
    assert_eq!(ssdp::alive_set(&advert).concat(), f.wire("ssdp-alive.ssdp"));
    assert_eq!(
        ssdp::byebye_set(&advert).concat(),
        f.wire("ssdp-byebye.ssdp")
    );
    // What the device sends, a control point reads.
    for m in ssdp::alive_set(&advert) {
        let read = client::parse_ssdp(m.as_bytes()).unwrap();
        assert_eq!(read.kind, client::SsdpKind::Alive);
        assert_eq!(read.udn(), Some(advert.udn));
    }

    let searches = f.stems("msearch-", ".fields");
    assert!(searches.len() >= 7);
    for name in searches {
        let expect = f.fields(&format!("msearch-{name}.fields"));
        let datagram = f.wire(&format!("msearch-{name}.ssdp"));
        let multicast = need(&expect, "multicast") == "true";
        let parsed = ssdp::parse_search(datagram.as_bytes(), multicast);
        match need(&expect, "result") {
            "ok" => {
                let search = parsed.unwrap_or_else(|e| panic!("{name}: {e:?}"));
                assert_eq!(search.st, need(&expect, "st"), "{name}");
                assert_eq!(
                    search.window_ms.to_string(),
                    need(&expect, "window_ms"),
                    "{name}"
                );
                let responses = ssdp::search_responses(&search, &advert, Some(need(&p, "date")));
                assert_eq!(
                    responses.len().to_string(),
                    need(&expect, "responses"),
                    "{name}"
                );
                if !responses.is_empty() {
                    assert_eq!(
                        responses.concat(),
                        f.wire(&format!("msearch-{name}.responses.ssdp")),
                        "{name}"
                    );
                }
            }
            "NoMx" => assert_eq!(parsed, Err(SearchDrop::NoMx), "{name}"),
            "WrongMan" => assert_eq!(parsed, Err(SearchDrop::WrongMan), "{name}"),
            other => panic!("{name}: unknown result {other}"),
        }
    }
}

fn descriptions(f: &mut Fixtures) {
    let info = kitchen();
    let config_id = description::device_config_id(&info);
    let text = f.document("description-room.xml");
    assert_eq!(text, description::device_description(&info, config_id));
    let d = client::parse_description(&text).unwrap();
    assert_eq!(d.config_id, Some(config_id));
    assert_eq!(d.spec_version, Some((1, 1)));
    assert!(!d.has_url_base && !text.contains("URLBase"));
    assert_eq!(d.udn, format!("uuid:{}", info.udn));
    assert_eq!(d.friendly_name, "Kitchen");
    assert_eq!(d.manufacturer, "chorus");
    assert_eq!(d.device_type, "urn:schemas-upnp-org:device:MediaRenderer:1");
    // UDA 1.1 section 2.3: the order of the device element's children.
    assert_eq!(
        d.element_order,
        [
            "deviceType",
            "friendlyName",
            "manufacturer",
            "modelDescription",
            "modelName",
            "modelNumber",
            "UDN",
            "serviceList"
        ]
    );
    assert_eq!(d.services.len(), 3);
    assert!(!text.to_ascii_lowercase().contains("dlna"));

    for service in Service::ALL {
        let urls = d.service(service).unwrap();
        assert_eq!(urls.service_id, service.service_id());
        // Relative URLs under /upnp/<uuid>/, each routing to its resource.
        let prefix = format!("/upnp/{}/{}/", info.udn, service.path());
        for url in [&urls.scpd_url, &urls.control_url, &urls.event_sub_url] {
            assert!(url.starts_with(&prefix), "{url}");
            assert!(description::route(url).is_some());
        }
        let name = format!("scpd-{}.xml", service.path());
        let text = f.document(&name);
        assert_eq!(text, description::scpd(service, config_id), "{name}");
        // Self-consistency, read from the text a control point gets.
        let scpd = client::parse_scpd(&text).unwrap();
        assert_eq!(scpd.config_id, Some(config_id));
        assert_eq!(scpd.spec_version, Some((1, 1)));
        assert!(!scpd.variables.is_empty());
        for action in &scpd.actions {
            let mut seen_out = false;
            for arg in &action.arguments {
                assert!(
                    scpd.variable(&arg.related).is_some(),
                    "{name}: {}.{} names {}",
                    action.name,
                    arg.name,
                    arg.related
                );
                match arg.direction.as_str() {
                    "out" => seen_out = true,
                    "in" => assert!(!seen_out, "{name}: {} has an in after an out", action.name),
                    other => panic!("{name}: direction {other}"),
                }
            }
        }
        let evented: Vec<&str> = scpd
            .variables
            .iter()
            .filter(|v| v.send_events)
            .map(|v| v.name.as_str())
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
            _ => assert_eq!(evented, ["LastChange"], "{name}"),
        }
    }
    // What control points look for (and what must not be there).
    let avt = client::parse_scpd(&f.document("scpd-avt.xml")).unwrap();
    for present in [
        "SetAVTransportURI",
        "SetNextAVTransportURI",
        "GetMediaInfo",
        "GetTransportInfo",
        "GetPositionInfo",
        "GetDeviceCapabilities",
        "GetTransportSettings",
        "Stop",
        "Play",
        "Pause",
        "Seek",
        "Next",
        "Previous",
        "GetCurrentTransportActions",
    ] {
        assert!(avt.action(present).is_some(), "{present}");
    }
    for absent in ["Record", "SetRecordQualityMode", "SetPlayMode"] {
        assert!(avt.action(absent).is_none(), "{absent}");
    }
    assert_eq!(
        avt.variable("A_ARG_TYPE_SeekMode").unwrap().allowed_values,
        ["TRACK_NR", "REL_TIME"]
    );
    assert_eq!(
        avt.variable("TransportPlaySpeed").unwrap().allowed_values,
        ["1"]
    );
    let states = &avt.variable("TransportState").unwrap().allowed_values;
    assert!(!states.iter().any(|s| s.contains("RECORD")));
    let rcs = client::parse_scpd(&f.document("scpd-rcs.xml")).unwrap();
    for present in [
        "ListPresets",
        "SelectPreset",
        "GetMute",
        "SetMute",
        "GetVolume",
        "SetVolume",
    ] {
        assert!(rcs.action(present).is_some(), "{present}");
    }
    assert_eq!(
        rcs.variable("Volume").unwrap().range,
        Some(("0".to_string(), "100".to_string(), Some("1".to_string())))
    );
    assert_eq!(
        rcs.variable("A_ARG_TYPE_Channel").unwrap().allowed_values,
        ["Master"]
    );
    let cm = client::parse_scpd(&f.document("scpd-cm.xml")).unwrap();
    assert!(cm.action("PrepareForConnection").is_none());
    assert_eq!(cm.actions.len(), 3);
}

fn service_of(fields: &Fields) -> Service {
    Service::from_path(need(fields, "service")).expect("avt, rcs or cm")
}

fn pair(text: &str) -> (&str, &str) {
    text.split_once('=').expect("name=value")
}

fn soap_requests(f: &mut Fixtures) {
    let names = f.stems("soap-request-", ".fields");
    assert!(names.len() >= 10);
    for name in names {
        let expect = f.fields(&format!("soap-request-{name}.fields"));
        let body = f.document(&format!("soap-request-{name}.xml"));
        let parsed = soap::parse_request(get(&expect, "soapaction"), &body);
        match need(&expect, "parse") {
            "doctype" => {
                assert_eq!(parsed, Err(SoapError::Xml(XmlError::Doctype)), "{name}");
                continue;
            }
            "ok" => {}
            other => panic!("{name}: unknown parse {other}"),
        }
        let request = parsed.unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert_eq!(
            request.service_type,
            need(&expect, "service_type"),
            "{name}"
        );
        assert_eq!(request.action, need(&expect, "action"), "{name}");
        let args: Vec<(String, String)> = list(&expect, "arg")
            .into_iter()
            .map(pair)
            .map(|(n, v)| (n.to_string(), v.to_string()))
            .collect();
        assert_eq!(request.arguments, args, "{name}");
        let validated = soap::validate(service_of(&expect), &request);
        match need(&expect, "validate") {
            "ok" => {
                let invocation = validated.unwrap_or_else(|e| panic!("{name}: {e:?}"));
                // Each input is found whatever order it was sent in.
                for input in invocation.action.inputs() {
                    assert_eq!(
                        Some(invocation.input(input.name)),
                        request.argument(input.name),
                        "{name}"
                    );
                }
            }
            code => assert_eq!(validated.unwrap_err().code.to_string(), code, "{name}"),
        }
    }
    // The escaped and the CDATA metadata are the same document, and it is
    // the DIDL-Lite vector byte for byte.
    let didl = f.document("didl-track.xml");
    for name in ["seturi-escaped-didl", "seturi-cdata-didl"] {
        let expect = f.fields(&format!("soap-request-{name}.fields"));
        let body = f.document(&format!("soap-request-{name}.xml"));
        let request = soap::parse_request(get(&expect, "soapaction"), &body).unwrap();
        assert_eq!(request.argument("CurrentURIMetaData"), Some(didl.as_str()));
        let title = didl::parse(request.argument("CurrentURIMetaData").unwrap())
            .unwrap()
            .title;
        assert_eq!(title.as_deref(), Some("A & B <\"x\"> \u{e9}"));
    }
}

fn soap_responses(f: &mut Fixtures) {
    let names = f.stems("soap-response-", ".fields");
    assert!(names.len() >= 4);
    for name in names {
        let expect = f.fields(&format!("soap-response-{name}.fields"));
        let service = service_of(&expect);
        let action = need(&expect, "action");
        let table = description::table(service).action(action).unwrap();
        let out: Vec<(&'static str, String)> = match get(&expect, "from") {
            None => list(&expect, "out")
                .into_iter()
                .map(pair)
                .map(|(n, v)| {
                    let arg = table
                        .outputs()
                        .find(|a| a.name == n)
                        .expect("an out argument");
                    (arg.name, v.to_string())
                })
                .collect(),
            Some("connmgr") => {
                let call = client::soap_request(service, action, &[]).unwrap();
                let request = soap::parse_request(Some(&call.soapaction), &call.body).unwrap();
                connmgr::invoke(&soap::validate(service, &request).unwrap()).unwrap()
            }
            Some("avtransport-after-seturi-track") => {
                let didl = f.document("didl-track.xml");
                let uri = didl::parse(&didl).unwrap().resources[0].uri.clone();
                let mut t = AvTransport::new();
                t.set_av_transport_uri(&uri, &didl).unwrap();
                t.get_media_info()
            }
            Some(other) => panic!("{name}: unknown source {other}"),
        };
        let body = f.document(&format!("soap-response-{name}.xml"));
        assert_eq!(soap::build_response(service, action, &out), body, "{name}");
        // And a control point reads the same values back.
        let reply = client::parse_soap_reply(&body).unwrap();
        for (n, v) in &out {
            assert_eq!(reply.value(n), Some(v.as_str()), "{name}: {n}");
        }
    }
    let names = f.stems("soap-fault-", ".fields");
    assert!(names.len() >= 6);
    for name in names {
        let expect = f.fields(&format!("soap-fault-{name}.fields"));
        let code: u16 = need(&expect, "code").parse().unwrap();
        let description = need(&expect, "description");
        let known = [
            chorus_upnp::error::INVALID_ACTION,
            chorus_upnp::error::INVALID_ARGS,
            chorus_upnp::error::AVT_INVALID_INSTANCE_ID,
            chorus_upnp::error::RCS_INVALID_INSTANCE_ID,
            chorus_upnp::error::CM_INVALID_CONNECTION_REFERENCE,
            chorus_upnp::error::AVT_CONTENT_BUSY,
        ];
        let error: UpnpError = *known
            .iter()
            .find(|e| e.code == code && e.description == description)
            .unwrap_or_else(|| panic!("{name}: no such error constant"));
        let body = f.document(&format!("soap-fault-{name}.xml"));
        assert_eq!(soap::build_fault(&error), body, "{name}");
        assert_eq!(
            client::parse_soap_reply(&body).unwrap(),
            client::SoapReply::Fault {
                code,
                description: description.to_string()
            }
        );
    }
}

fn subscriptions(f: &mut Fixtures) {
    let names = f.stems("subscribe-", ".fields");
    assert!(names.len() >= 12);
    for name in names {
        let expect = f.fields(&format!("subscribe-{name}.fields"));
        let request = f.wire(&format!("subscribe-{name}.headers"));
        let (start, headers) = Headers::parse(&request);
        let method = need(&expect, "method");
        assert!(start.starts_with(&format!("{method} /upnp/")), "{name}");
        assert!(description::route(start.split(' ').nth(1).unwrap()).is_some());
        let timeout = get(&expect, "timeout_s").map(|t| t.parse::<u32>().unwrap());
        let result = need(&expect, "result");
        if method == "UNSUBSCRIBE" {
            let parsed = gena::parse_unsubscribe(&headers);
            match result {
                "ok" => assert_eq!(parsed.as_deref(), Ok(need(&expect, "sid")), "{name}"),
                status => assert_eq!(parsed, Err(status.parse().unwrap()), "{name}"),
            }
            continue;
        }
        let parsed = gena::parse_subscribe(&headers);
        match result {
            "new" => assert_eq!(
                parsed,
                Ok(Subscribe::New {
                    callbacks: list(&expect, "callback")
                        .into_iter()
                        .map(str::to_string)
                        .collect(),
                    timeout_s: timeout,
                }),
                "{name}"
            ),
            "renew" => assert_eq!(
                parsed,
                Ok(Subscribe::Renew {
                    sid: need(&expect, "sid").to_string(),
                    timeout_s: timeout,
                }),
                "{name}"
            ),
            status => assert_eq!(parsed, Err(status.parse().unwrap()), "{name}"),
        }
    }

    let cases = f.raw("callback-rule.cases");
    let mut count = 0;
    for line in cases
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let cols: Vec<&str> = line.split('|').map(str::trim).collect();
        assert_eq!(cols.len(), 5, "{line}");
        let requester: IpAddr = cols[1].parse().unwrap();
        let subnets: Vec<Cidr> = cols[2]
            .split(',')
            .map(|c| Cidr::parse(c).unwrap())
            .collect();
        let got = gena::callback_allowed(cols[0], requester, &subnets, cols[3] == "true");
        let expected = match cols[4] {
            "ok" => None,
            "NotHttp" => Some(CallbackRefusal::NotHttp),
            "NotAnAddress" => Some(CallbackRefusal::NotAnAddress),
            "Loopback" => Some(CallbackRefusal::Loopback),
            "NotUnicast" => Some(CallbackRefusal::NotUnicast),
            "OutsideSubnets" => Some(CallbackRefusal::OutsideSubnets),
            "NotTheRequester" => Some(CallbackRefusal::NotTheRequester),
            other => panic!("unknown result {other}"),
        };
        assert_eq!(got.err(), expected, "{line}");
        count += 1;
    }
    assert!(count >= 18);
}

fn metadata(f: &mut Fixtures) {
    let names = f.stems("didl-", ".fields");
    assert!(names.len() >= 5);
    for name in names {
        let expect = f.fields(&format!("didl-{name}.fields"));
        let text = f.document(&format!("didl-{name}.xml"));
        let parsed = didl::parse(&text);
        if need(&expect, "result") == "none" {
            assert_eq!(parsed, None, "{name}");
            // And the action that carries it still succeeds, keeping the
            // string as it came.
            let mut t = AvTransport::new();
            t.set_av_transport_uri("http://192.0.2.50:8000/a.flac", &text)
                .unwrap();
            assert_eq!(t.current().1, text);
            assert_eq!(t.get_media_info()[3].1, text);
            continue;
        }
        let m = parsed.unwrap_or_else(|| panic!("{name}: no metadata"));
        assert_eq!(m.title.as_deref(), get(&expect, "title"), "{name}");
        assert_eq!(m.artist.as_deref(), get(&expect, "artist"), "{name}");
        assert_eq!(m.album.as_deref(), get(&expect, "album"), "{name}");
        assert_eq!(
            m.album_art_uri.as_deref(),
            get(&expect, "album_art_uri"),
            "{name}"
        );
        assert_eq!(m.class.as_deref(), get(&expect, "class"), "{name}");
        assert_eq!(
            m.resources.len().to_string(),
            need(&expect, "res.count"),
            "{name}"
        );
        for (i, res) in m.resources.iter().enumerate() {
            let key = |k: &str| format!("res.{i}.{k}");
            assert_eq!(res.uri, need(&expect, &key("uri")), "{name}");
            let pi = res.protocol_info.as_ref().unwrap();
            assert_eq!(pi.protocol, need(&expect, &key("protocol")), "{name}");
            assert_eq!(pi.network, need(&expect, &key("network")), "{name}");
            assert_eq!(
                pi.content_format,
                need(&expect, &key("content_format")),
                "{name}"
            );
            assert_eq!(
                pi.additional_info,
                need(&expect, &key("additional_info")),
                "{name}"
            );
            let number = |k: &str| get(&expect, &key(k)).map(|v| v.parse::<u64>().unwrap());
            assert_eq!(res.duration_ms, number("duration_ms"), "{name}");
            assert_eq!(res.size, number("size"), "{name}");
            assert_eq!(
                res.sample_frequency.map(u64::from),
                number("sample_frequency"),
                "{name}"
            );
            assert_eq!(
                res.nr_audio_channels.map(u64::from),
                number("nr_audio_channels"),
                "{name}"
            );
        }
    }
}

/// The LastChange documents and event bodies, by walking the gapless
/// sequence: idle, SetAVTransportURI, Play, SetNextAVTransportURI, the
/// boundary, the end.
fn events(f: &mut Fixtures) {
    let first = f.document("didl-first.xml");
    let track = f.document("didl-track.xml");
    let uri1 = didl::parse(&first).unwrap().resources[0].uri.clone();
    let uri2 = didl::parse(&track).unwrap().resources[0].uri.clone();
    let lc = |changes: &[chorus_upnp::lastchange::Change]| event_xml(AVT_NS, changes);

    let mut t = AvTransport::new();
    let idle = lc(&t.evented());
    assert_eq!(idle, f.document("lastchange-avt-initial-idle.xml"));
    assert_eq!(
        gena::propertyset(&[("LastChange", &idle)]),
        f.document("notify-avt-initial-idle.xml")
    );
    let mut now = 10_000;
    let mut take = |t: &mut AvTransport| {
        now += 200;
        lc(&t.events().take(now).expect("changes wait"))
    };

    let effects = t.set_av_transport_uri(&uri1, &first).unwrap();
    assert_eq!(
        effects,
        [Effect::Load {
            uri: uri1.clone(),
            metadata: first.clone()
        }]
    );
    assert_eq!(take(&mut t), f.document("lastchange-avt-seturi.xml"));

    assert_eq!(t.play("1").unwrap(), [Effect::Start]);
    assert!(t.playing(t.epoch()));
    assert_eq!(take(&mut t), f.document("lastchange-avt-playing.xml"));

    let effects = t.set_next_av_transport_uri(&uri2, &track).unwrap();
    assert_eq!(
        effects,
        [Effect::QueueNext {
            uri: uri2.clone(),
            metadata: track.clone()
        }]
    );
    assert_eq!(take(&mut t), f.document("lastchange-avt-setnext.xml"));

    // The boundary: one set of changes, and no transport state among them.
    assert!(t.track_boundary(t.epoch(), Some(205_000), true));
    assert!(t
        .events()
        .pending()
        .iter()
        .all(|c| c.name != "TransportState"));
    let boundary = take(&mut t);
    assert_eq!(boundary, f.document("lastchange-avt-gapless-boundary.xml"));
    for never in ["STOPPED", "TRANSITIONING", "TransportState"] {
        assert!(!boundary.contains(never), "{never}");
    }
    let body = gena::propertyset(&[("LastChange", &boundary)]);
    assert_eq!(body, f.document("notify-avt-gapless-boundary.xml"));
    // A subscriber: one unescape to the Event, one more to the metadata.
    let props = client::parse_propertyset(&body).unwrap();
    let read = client::parse_last_change(&props[0].1).unwrap();
    assert_eq!(read.get("AVTransportURI"), Some(uri2.as_str()));
    assert_eq!(read.get("AVTransportURIMetaData"), Some(track.as_str()));
    assert_eq!(read.get("NextAVTransportURI"), Some(""));
    assert_eq!(
        didl::parse(read.get("CurrentTrackMetaData").unwrap())
            .unwrap()
            .title
            .as_deref(),
        Some("A & B <\"x\"> \u{e9}")
    );

    assert!(t.ended(t.epoch()).is_empty());
    assert_eq!(take(&mut t), f.document("lastchange-avt-ended.xml"));

    // RenderingControl.
    let mut rcs = RenderingControl::new(300, false);
    assert_eq!(
        event_xml(RCS_NS, &rcs.evented()),
        f.document("lastchange-rcs-initial.xml")
    );
    rcs.report(600, true);
    let changed = event_xml(RCS_NS, &rcs.events().take(0).unwrap());
    assert_eq!(changed, f.document("lastchange-rcs-volume-mute.xml"));
    let body = gena::propertyset(&[("LastChange", &changed)]);
    assert_eq!(body, f.document("notify-rcs-volume-mute.xml"));

    // The whole NOTIFY request: CRLF head, then the body as it is.
    let file = f.document("notify-request.http");
    let (head, file_body) = file.split_once("\n\n").unwrap();
    assert_eq!(file_body, body);
    let callback = gena::parse_callback_url("http://192.0.2.50:49152/cb").unwrap();
    assert_eq!(
        gena::build_notify(&callback, &gena::sid_from_random([0xab; 16]), 3, &body),
        format!("{}\r\n\r\n{body}", head.replace('\n', "\r\n"))
    );

    // ConnectionManager: three direct properties.
    let cm = connmgr::evented();
    let pairs: Vec<(&str, &str)> = cm.iter().map(|(n, v)| (*n, v.as_str())).collect();
    assert_eq!(
        gena::propertyset(&pairs),
        f.document("notify-cm-initial.xml")
    );
}

#[test]
fn every_vector_holds_and_every_file_is_read() {
    let mut f = Fixtures::new();
    ssdp_messages(&mut f);
    descriptions(&mut f);
    soap_requests(&mut f);
    soap_responses(&mut f);
    subscriptions(&mut f);
    metadata(&mut f);
    events(&mut f);
    let unread: Vec<String> = f
        .all()
        .into_iter()
        .filter(|name| !f.read.contains(name))
        .collect();
    assert!(
        unread.is_empty(),
        "fixtures/upnp holds files no test reads: {unread:?}"
    );
    assert!(f.read.len() >= 100, "{} vectors read", f.read.len());
}

/// The generated vectors are what `make upnp-vectors` writes now: the same
/// function calls as `src/bin/chorus-upnp-vectors.rs`, compared with the
/// committed files with their final line feed.
#[test]
fn the_generated_vectors_are_current() {
    let mut f = Fixtures::new();
    let info = kitchen();
    let config_id = description::device_config_id(&info);
    assert_eq!(
        f.raw("description-room.xml"),
        format!("{}\n", description::device_description(&info, config_id))
    );
    for service in Service::ALL {
        assert_eq!(
            f.raw(&format!("scpd-{}.xml", service.path())),
            format!("{}\n", description::scpd(service, config_id))
        );
    }
}
