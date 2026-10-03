//! Goal 16, acceptance line C: "A UPnP AV renderer per room, saved group and
//! live group plays gapless with metadata and stable identities to a scripted
//! control point".
//!
//! The control point here is scripted: it is written with
//! `chorus_upnp::client` over `std::net` and does what a control point does
//! (search, read descriptions, call actions, subscribe, receive events), in
//! the order the script says. No control point application was run.
//!
//! The server is the real binary (`common::RunningServer`), started with
//! `--upnp` on loopback. Three things about that:
//!
//! - its media comes from an HTTP server on loopback inside this test, so it
//!   is started with `--media-allow-loopback`, the flag that exists for
//!   exactly this and is never set in a deployment;
//! - its discovery socket is on an ephemeral UDP port (`--upnp-ssdp-port 0`)
//!   and its notifications go to a UDP socket of this test
//!   (`--upnp-ssdp-group 127.0.0.1:<port>`), so the test neither needs port
//!   1900 nor hears, nor is heard by, any other SSDP program. Searches are
//!   sent to the server's port with the multicast form's own `HOST` header.
//!   Consequence, said plainly: **no datagram here crosses a multicast
//!   group**; the join and the multicast send are not exercised;
//! - what a room plays is captured by a real client session
//!   (`common::Player`, the Linux client's own session code) attached to the
//!   room, and compared sample for sample.
//!
//! # The test plan of the research digest (17 steps) and where each is
//!
//! 1. Discovery, search: `searches_are_answered_by_target_inside_mx_and_never_without_it`.
//! 2. Discovery, announce: `a_renderer_per_room_saved_group_and_live_group_is_discovered_and_described`
//!    (the alive sets at start), and the scale test.
//! 3. Description: the same test (descriptions, SCPDs, HTTP/1.0, 404, HEAD).
//! 4. Idle state: `idle_state_queries_and_soap_errors_follow_the_services`.
//! 5. SOAP errors and dialects: the same test.
//! 6. Eventing basics: `eventing_follows_gena_and_refuses_callbacks_off_the_subnets`.
//!    NOT covered: a subscription expiring on time (every subscription is
//!    granted 1800 s; expiry is held by `chorus-upnp`'s own tests of
//!    `Subscriptions::expire`), and a loopback callback refused by a server
//!    whose listener is not on loopback (that needs a listener off loopback,
//!    which a test does not bind; the rule's two sides are a unit test in
//!    `crates/server/src/upnp.rs`).
//! 7. Play: `pause_seek_stop_and_the_end_of_media`.
//! 8. Metadata: `metadata_is_kept_verbatim_and_tolerated_when_broken`, and the
//!    gapless test (the room's state shows title, artist, album).
//! 9. Gapless: `a_control_point_plays_two_tracks_gapless_with_metadata` (a
//!    room's renderer), and the same script with the same assertions on a
//!    saved group's renderer and on a live group's renderer, with a client
//!    session capturing in each of the group's two rooms
//!    (`a_saved_group_renderer_plays_two_tracks_gapless_with_metadata_in_every_room`,
//!    `a_live_group_renderer_plays_two_tracks_gapless_with_metadata_in_every_room`):
//!    each room's PCM is compared sample for sample, each room's control
//!    state shows both tracks' metadata, and the two rooms' captures must sit
//!    at the same places on the chunk grid (sequence, timestamp, index in the
//!    chunk). Replace, clear, a next URI that is not found and Next are in
//!    `pause_seek_stop_and_the_end_of_media`. NOT covered: a
//!    SetNextAVTransportURI that arrives after the join was written and
//!    before it is audible (the window is at most the player's ring, and the
//!    script cannot place a request inside it reliably).
//! 10. Formats: `each_supported_format_plays_in_the_room` and
//!     `aac_is_refused_and_never_plays`. The compressed fixtures are 300 ms
//!     long, so "plays" is asserted on the PCM the room received, not on a
//!     PLAYING event that the 200 ms moderation may fold into the STOPPED
//!     that follows it.
//! 11. Fetch rule: `media_urls_obey_the_fetch_rule`. NOT covered here: an
//!     endless or oversized response (the fetcher's own tests).
//! 12. Volume: `volume_is_clamped_by_the_room_limit_and_the_real_value_is_evented`.
//! 13. Moderation: in the eventing test.
//! 14. Groups appear and vanish: `casting_to_a_group_takes_its_rooms` (K78,
//!     the players' pool; what a group's rooms receive is in step 9's two
//!     group tests, not here),
//!     `a_vanished_group_says_byebye_and_its_description_is_gone`,
//!     `a_live_group_keeps_its_identity_when_the_same_rooms_re_form` (and the
//!     rename).
//! 15. Stable identity: `identities_are_uuid_v5_and_survive_a_restart`.
//! 16. Shutdown: `stopping_the_server_says_byebye_for_every_renderer`.
//! 17. Scale: `sixteen_renderers_are_announced_searched_and_subscribed`.

mod common;
// The control point's tools, shared with `openhome_control_point.rs`. `Home`
// starts these servers with `--upnp-openhome off`: this file is the UPnP AV
// half, and what it asserts (six discovery rows, three services) is that
// half alone; the other file runs the default, with the OpenHome services.
#[path = "common/upnp_cp.rs"]
mod cp;

use cp::*;

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::{TcpListener, UdpSocket};
use std::thread;
use std::time::{Duration, Instant};

use chorus_upnp::client::{self, SoapReply, SsdpKind, SsdpMessage};
use chorus_upnp::uuid::{udn, Target, CHORUS_NAMESPACE};
use chorus_upnp::xml::escape_text;
use chorus_upnp::Service;

// ----- the tests -----------------------------------------------------------------

#[test]
fn a_renderer_per_room_saved_group_and_live_group_is_discovered_and_described() {
    let mut home = Home::start(&["kitchen", "den", "study"]);
    // Start-up: each room's six alive messages, sent twice, on one port.
    let alive: Vec<SsdpMessage> = wait("the alive sets of three rooms", || {
        let notes = home.notifications(Duration::from_millis(400));
        (notes.len() >= 36).then(|| notes.iter().map(|(m, _)| m.clone()).collect())
    });
    assert_eq!(alive.len(), 36, "3 renderers x 6 messages x 2 sets");
    let mut per_device: BTreeMap<String, BTreeSet<(String, String)>> = BTreeMap::new();
    let mut locations = BTreeSet::new();
    for m in &alive {
        assert_eq!(m.kind, SsdpKind::Alive);
        assert_eq!(m.headers.get("HOST"), Some("239.255.255.250:1900"));
        assert!(m.max_age_s.unwrap() >= 1800);
        let location = m.location.clone().unwrap();
        assert!(location.starts_with(&format!("http://{}/upnp/", home.http)));
        locations.insert(location);
        per_device
            .entry(m.udn().unwrap().to_string())
            .or_default()
            .insert((m.target.clone(), m.usn.clone()));
    }
    assert_eq!(locations.len(), 3, "a LOCATION per renderer");
    for (udn, pairs) in &per_device {
        let expected: BTreeSet<(String, String)> = [
            (
                "upnp:rootdevice".to_string(),
                format!("uuid:{udn}::upnp:rootdevice"),
            ),
            (format!("uuid:{udn}"), format!("uuid:{udn}")),
            (DEVICE.to_string(), format!("uuid:{udn}::{DEVICE}")),
        ]
        .into_iter()
        .chain(Service::AV.iter().map(|s| {
            (
                s.service_type().to_string(),
                format!("uuid:{udn}::{}", s.service_type()),
            )
        }))
        .collect();
        assert_eq!(pairs, &expected, "the six NT/USN pairs of {udn}");
    }

    // A named room, a saved group and a live group.
    home.server
        .applied(r#"{"v":1,"t":"name","zone":"kitchen","name":"Kitchen"}"#);
    home.server.applied(
        r#"{"v":2,"t":"group_save","group":"downstairs","name":"Downstairs","zones":["kitchen","den"]}"#,
    );
    home.server
        .applied(r#"{"v":2,"t":"join","zone":"study","target":"den"}"#);
    let devices = wait("five renderers", || {
        let d = home.devices();
        (d.len() == 5 && d.iter().any(|d| d.name == "Kitchen")).then_some(d)
    });
    let names: BTreeSet<(&str, &str)> = devices
        .iter()
        .map(|d| (d.name.as_str(), d.model.as_str()))
        .collect();
    assert_eq!(
        names,
        BTreeSet::from([
            ("Kitchen", "chorus room"),
            ("den", "chorus room"),
            ("study", "chorus room"),
            ("Downstairs", "chorus saved group"),
            ("den + study", "chorus live group"),
        ])
    );
    let udns: BTreeSet<&str> = devices.iter().map(|d| d.udn.as_str()).collect();
    assert_eq!(udns.len(), 5, "five UDNs");

    for device in &devices {
        let path = device
            .location
            .strip_prefix(&format!("http://{}", home.http))
            .unwrap();
        let answer = get(&home.http, path);
        assert!(answer.start.starts_with("HTTP/1.1 200"), "{}", answer.start);
        assert!(answer
            .headers
            .get("CONTENT-TYPE")
            .is_some_and(|t| t.starts_with("text/xml")));
        assert_eq!(
            answer.headers.get("CONTENT-LENGTH").unwrap(),
            answer.body.len().to_string()
        );
        let d = client::parse_description(&answer.body).unwrap();
        assert_eq!(d.spec_version, Some((1, 1)));
        assert!(!d.has_url_base, "no URLBase in UDA 1.1");
        assert_eq!(d.config_id, Some(device.config_id), "configId is CONFIGID");
        assert_eq!(d.udn, format!("uuid:{}", device.udn));
        assert_eq!(d.device_type, DEVICE);
        assert_eq!(d.manufacturer, "chorus");
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
        assert!(answer.body.contains("UPnP AV media renderer"));
        assert!(!answer.body.contains("DLNA"), "no DLNA claim");
        assert_eq!(d.services.len(), 3);
        for service in Service::AV {
            let urls = d.service(service).expect("the service");
            for url in [&urls.scpd_url, &urls.control_url, &urls.event_sub_url] {
                assert!(
                    url.starts_with(&format!("/upnp/{}/", device.udn)),
                    "a relative URL under the device: {url}"
                );
            }
            let scpd = get(&home.http, &urls.scpd_url);
            assert_eq!(scpd.status, 200);
            let s = client::parse_scpd(&scpd.body).expect("an SCPD");
            assert_eq!(s.config_id, Some(device.config_id));
            for action in &s.actions {
                let mut seen_out = false;
                for argument in &action.arguments {
                    assert!(
                        s.variable(&argument.related).is_some(),
                        "{}.{} names a variable",
                        action.name,
                        argument.name
                    );
                    match argument.direction.as_str() {
                        "in" => assert!(!seen_out, "{}: in arguments first", action.name),
                        _ => seen_out = true,
                    }
                }
            }
            let required: &[&str] = match service {
                Service::AvTransport => &[
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
                ],
                Service::RenderingControl => &[
                    "ListPresets",
                    "SelectPreset",
                    "GetVolume",
                    "SetVolume",
                    "GetMute",
                    "SetMute",
                ],
                Service::ConnectionManager => &[
                    "GetProtocolInfo",
                    "GetCurrentConnectionIDs",
                    "GetCurrentConnectionInfo",
                ],
                _ => unreachable!("only the AV services are walked here"),
            };
            for name in required {
                assert!(s.action(name).is_some(), "{name}");
            }
            let evented: Vec<&str> = s
                .variables
                .iter()
                .filter(|v| v.send_events)
                .map(|v| v.name.as_str())
                .collect();
            if service != CM {
                assert_eq!(evented, ["LastChange"], "the only evented variable");
            }
        }
    }

    // HTTP/1.0 gets an HTTP/1.0 answer and a closed socket; HEAD no body; an
    // unknown device, and a path that is none, 404.
    let kitchen = devices.iter().find(|d| d.name == "Kitchen").unwrap();
    let path = format!("/upnp/{}/desc.xml", kitchen.udn);
    let old = exchange(
        &home.http,
        format!("GET {path} HTTP/1.0\r\n\r\n").as_bytes(),
    );
    assert!(old.start.starts_with("HTTP/1.0 200"), "{}", old.start);
    assert!(old.body.contains("<friendlyName>Kitchen</friendlyName>"));
    let head = exchange(
        &home.http,
        format!("HEAD {path} HTTP/1.1\r\nHOST: x\r\n\r\n").as_bytes(),
    );
    assert_eq!((head.status, head.body.as_str()), (200, ""));
    assert_eq!(
        get(
            &home.http,
            "/upnp/00000000-0000-5000-8000-000000000000/desc.xml"
        )
        .status,
        404
    );
    assert_eq!(get(&home.http, "/").status, 404);
    assert_eq!(
        exchange(
            &home.http,
            format!("POST {path} HTTP/1.1\r\nCONTENT-LENGTH: 0\r\n\r\n").as_bytes()
        )
        .status,
        405
    );
    println!(
        "control-point: discovered {} renderers (3 rooms, 1 saved group, 1 live group); \
         start-up alive messages {} (3 renderers x 6 x 2 sets)",
        devices.len(),
        alive.len()
    );
}

#[test]
fn searches_are_answered_by_target_inside_mx_and_never_without_it() {
    let home = Home::start(&["kitchen", "den", "study"]);
    let devices = wait("three renderers", || {
        let d = home.devices();
        (d.len() == 3).then_some(d)
    });
    let second = Duration::from_millis(1300);
    thread::sleep(Duration::from_millis(1100));

    // ssdp:all: six per renderer, each whole, all inside MX.
    let all = home.search("ssdp:all", 1, second);
    assert_eq!(all.len(), 18, "6 responses per renderer");
    for (m, after) in &all {
        assert!(*after <= Duration::from_secs(1), "inside MX 1: {after:?}");
        assert!(m.max_age_s.unwrap() >= 1800);
        assert_eq!(m.headers.get("EXT"), Some(""), "EXT is present and empty");
        assert!(m.headers.has("DATE"));
        assert!(m
            .location
            .as_deref()
            .unwrap()
            .starts_with("http://127.0.0.1:"));
        let server: Vec<&str> = m.server.as_deref().unwrap().split(' ').collect();
        assert_eq!(server.len(), 3, "three product tokens: {server:?}");
        assert_eq!(server[1], "UPnP/1.1");
        assert!(server[0].starts_with("Linux/") && server[2].starts_with("chorus/"));
        assert!(m.usn.starts_with("uuid:"));
        assert!(m.boot_id.is_some() && m.config_id.is_some());
    }
    for device in &devices {
        let mine: BTreeSet<&str> = all
            .iter()
            .filter(|(m, _)| m.usn.starts_with(&format!("uuid:{}", device.udn)))
            .map(|(m, _)| m.target.as_str())
            .collect();
        assert_eq!(mine.len(), 6, "{}: {mine:?}", device.name);
    }
    thread::sleep(Duration::from_millis(1100));
    assert_eq!(home.search("upnp:rootdevice", 1, second).len(), 3);
    let one = home.search(&format!("uuid:{}", devices[0].udn), 1, second);
    assert_eq!(one.len(), 1);
    assert_eq!(one[0].0.usn, format!("uuid:{}", devices[0].udn));
    assert_eq!(home.search(DEVICE, 1, second).len(), 3);
    for service in Service::AV {
        let found = home.search(service.service_type(), 1, second);
        assert_eq!(found.len(), 3, "{}", service.service_type());
        assert!(found
            .iter()
            .all(|(m, _)| m.target == service.service_type()));
    }
    thread::sleep(Duration::from_millis(1100));
    // A multicast-form search without MX is dropped; another version of the
    // device type is not offered; a search that is not a discover is dropped.
    let no_mx = format!(
        "M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nST: {DEVICE}\r\n\r\n"
    );
    assert_eq!(home.search_raw(&no_mx, second).len(), 0, "no MX, no answer");
    assert_eq!(
        home.search("urn:schemas-upnp-org:device:MediaRenderer:2", 1, second)
            .len(),
        0
    );
    // A unicast-form search (HOST is the device's own address) has no MX and
    // is answered within a second.
    let unicast = format!(
        "M-SEARCH * HTTP/1.1\r\nHOST: {}\r\nMAN: \"ssdp:discover\"\r\nST: upnp:rootdevice\r\n\r\n",
        home.ssdp
    );
    let found = home.search_raw(&unicast, second);
    assert_eq!(found.len(), 3);
    assert!(found
        .iter()
        .all(|(_, after)| *after <= Duration::from_secs(1)));
    // MX 120 is held to 5 seconds.
    let slow = home.search(DEVICE, 120, Duration::from_millis(5300));
    assert_eq!(slow.len(), 3);
    let latest = slow.iter().map(|(_, after)| *after).max().unwrap();
    assert!(
        latest <= Duration::from_secs(5),
        "MX 120 answered in {latest:?}"
    );
    // One source asking faster than 8 searches a second is not answered past
    // that: 12 at once draw at most 8 answers.
    thread::sleep(Duration::from_millis(1100));
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket
        .set_read_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    let burst = client::msearch(&format!("uuid:{}", devices[0].udn), 1);
    for _ in 0..12 {
        socket.send_to(burst.as_bytes(), home.ssdp).unwrap();
    }
    let sent = Instant::now();
    let mut answers = 0;
    let mut buffer = [0u8; 2048];
    while sent.elapsed() < second {
        if socket.recv_from(&mut buffer).is_ok() {
            answers += 1;
        }
    }
    assert_eq!(answers, 8, "the per-source limit");
    println!(
        "control-point: search ssdp:all 18 of 18 inside MX 1; rootdevice 3; uuid 1; device type \
         3; each service type 3; no MX 0; MediaRenderer:2 0; MX 120 answered in {} ms; 12 \
         searches in a burst drew {} answers",
        latest.as_millis(),
        answers
    );
}

#[test]
fn idle_state_queries_and_soap_errors_follow_the_services() {
    let home = Home::start(&["kitchen"]);
    let kitchen = wait("the kitchen", || home.devices().into_iter().next());
    let zero = [("InstanceID", "0")];

    // Idle.
    assert_eq!(
        home.transport(&kitchen),
        ("NO_MEDIA_PRESENT".to_string(), "OK".to_string())
    );
    assert_eq!(
        home.avt(&kitchen, "GetTransportInfo").value("CurrentSpeed"),
        Some("1")
    );
    let media = home.avt(&kitchen, "GetMediaInfo");
    assert_eq!(media.value("NrTracks"), Some("0"));
    assert_eq!(media.value("CurrentURI"), Some(""));
    assert_eq!(media.value("PlayMedium"), Some("NONE"));
    let position = home.avt(&kitchen, "GetPositionInfo");
    assert_eq!(position.value("RelTime"), Some("0:00:00"));
    assert_eq!(position.value("Track"), Some("0"));
    assert_eq!(
        home.avt(&kitchen, "GetDeviceCapabilities")
            .value("PlayMedia"),
        Some("NETWORK")
    );
    assert_eq!(
        home.avt(&kitchen, "GetTransportSettings").value("PlayMode"),
        Some("NORMAL")
    );
    assert_eq!(
        home.avt(&kitchen, "GetCurrentTransportActions")
            .value("Actions"),
        Some("")
    );
    let protocols = home.ok(&kitchen, CM, "GetProtocolInfo", &[]);
    let sink = protocols.value("Sink").unwrap().to_ascii_lowercase();
    for wanted in ["flac", "mpeg", "ogg", "wav", "l16", "mp4"] {
        assert!(sink.contains(wanted), "{wanted} in {sink}");
    }
    assert!(!sink.contains("aac"), "no AAC in the sink list");
    assert!(!sink.contains("dlna.org_pn"), "no DLNA profile name");
    assert_eq!(protocols.value("Source"), Some(""));
    assert_eq!(
        home.ok(&kitchen, CM, "GetCurrentConnectionIDs", &[])
            .value("ConnectionIDs"),
        Some("0")
    );
    let connection = home.ok(
        &kitchen,
        CM,
        "GetCurrentConnectionInfo",
        &[("ConnectionID", "0")],
    );
    assert_eq!(connection.value("Status"), Some("OK"));
    home.fault(
        &kitchen,
        CM,
        "GetCurrentConnectionInfo",
        &[("ConnectionID", "1")],
        706,
    );
    assert_eq!(
        home.ok(&kitchen, RCS, "ListPresets", &zero)
            .value("CurrentPresetNameList"),
        Some("FactoryDefaults")
    );
    // Stop with no media is tolerated; Play with none is not.
    home.avt(&kitchen, "Stop");
    home.fault(
        &kitchen,
        AVT,
        "Play",
        &[("InstanceID", "0"), ("Speed", "1")],
        701,
    );

    // Errors.
    let raw = |service: Service, action: &str, arguments: &[(&str, &str)]| {
        let call = client::soap_request_raw(service.service_type(), action, arguments);
        let answer = post(
            &home.http,
            &kitchen.path(service, "control"),
            &call.soapaction,
            &call.body,
        );
        (
            answer.status,
            client::parse_soap_reply(&answer.body).unwrap().fault_code(),
        )
    };
    assert_eq!(
        raw(AVT, "Rewind", &zero),
        (500, Some(401)),
        "unknown action"
    );
    assert_eq!(
        raw(AVT, "Play", &zero),
        (500, Some(402)),
        "missing argument"
    );
    assert_eq!(
        raw(AVT, "GetTransportInfo", &[("InstanceID", "7")]),
        (500, Some(718))
    );
    assert_eq!(
        raw(
            RCS,
            "GetVolume",
            &[("InstanceID", "7"), ("Channel", "Master")]
        ),
        (500, Some(702))
    );
    assert_eq!(
        raw(RCS, "GetVolume", &[("InstanceID", "0"), ("Channel", "LF")]),
        (500, Some(402))
    );
    assert_eq!(
        raw(
            RCS,
            "SelectPreset",
            &[("InstanceID", "0"), ("PresetName", "Nope")]
        ),
        (500, Some(701))
    );
    assert_eq!(
        raw(
            AVT,
            "SetAVTransportURI",
            &[
                ("InstanceID", "0"),
                ("CurrentURI", "file:///etc/hostname"),
                ("CurrentURIMetaData", "")
            ]
        ),
        (500, Some(716)),
        "not http"
    );
    // With media loaded and not started.
    let media = MediaServer::start();
    let url = media.serve("/a.wav", wav(48_000, &ramp(0, 48_000)), "audio/wav");
    assert!(matches!(
        home.set_uri(&kitchen, &url, ""),
        SoapReply::Response { .. }
    ));
    assert_eq!(
        raw(AVT, "Play", &[("InstanceID", "0"), ("Speed", "2")]),
        (500, Some(717))
    );
    assert_eq!(
        raw(
            AVT,
            "Seek",
            &[("InstanceID", "0"), ("Unit", "FRAME"), ("Target", "1")]
        ),
        (500, Some(710))
    );
    home.fault(&kitchen, AVT, "Next", &zero, 711);
    home.fault(&kitchen, AVT, "Previous", &zero, 711);

    // Dialects: other prefixes, a default namespace, a chunked body, an
    // unquoted SOAPACTION, HTTP/1.0: all understood.
    let control = kitchen.path(AVT, "control");
    let avt_type = AVT.service_type();
    let prefixed = format!(
        "<?xml version=\"1.0\"?><SOAP-ENV:Envelope \
         xmlns:SOAP-ENV=\"http://schemas.xmlsoap.org/soap/envelope/\"><SOAP-ENV:Body>\
         <m:GetTransportInfo xmlns:m=\"{avt_type}\"><InstanceID>0</InstanceID>\
         </m:GetTransportInfo></SOAP-ENV:Body></SOAP-ENV:Envelope>"
    );
    let default_ns = format!(
        "<Envelope xmlns=\"http://schemas.xmlsoap.org/soap/envelope/\"><Body>\
         <GetTransportInfo xmlns=\"{avt_type}\"><InstanceID xmlns=\"\">0</InstanceID>\
         </GetTransportInfo></Body></Envelope>"
    );
    let state_of = |answer: &Answer| {
        client::parse_soap_reply(&answer.body)
            .unwrap()
            .value("CurrentTransportState")
            .map(str::to_string)
    };
    for body in [&prefixed, &default_ns] {
        let answer = post(
            &home.http,
            &control,
            &format!("\"{avt_type}#GetTransportInfo\""),
            body,
        );
        assert_eq!(answer.status, 200, "{}", answer.body);
        assert_eq!(state_of(&answer).as_deref(), Some("STOPPED"));
        assert_eq!(answer.headers.get("EXT"), Some(""));
        assert_eq!(
            answer.headers.get("CONTENT-LENGTH").unwrap(),
            answer.body.len().to_string()
        );
    }
    let unquoted = post(
        &home.http,
        &control,
        &format!("{avt_type}#GetTransportInfo"),
        &prefixed,
    );
    assert_eq!(state_of(&unquoted).as_deref(), Some("STOPPED"));
    let half = prefixed.len() / 2;
    let chunked = exchange(
        &home.http,
        format!(
            "POST {control} HTTP/1.1\r\nHOST: x\r\nCONTENT-TYPE: text/xml\r\nSOAPACTION: \
             \"{avt_type}#GetTransportInfo\"\r\nTRANSFER-ENCODING: chunked\r\n\r\n{:x}\r\n{}\r\n{:x}\r\n{}\r\n0\r\n\r\n",
            half,
            &prefixed[..half],
            prefixed.len() - half,
            &prefixed[half..]
        )
        .as_bytes(),
    );
    assert_eq!(state_of(&chunked).as_deref(), Some("STOPPED"), "chunked");
    let http10 = exchange(
        &home.http,
        format!(
            "POST {control} HTTP/1.0\r\nCONTENT-TYPE: text/xml; charset=\"utf-8\"\r\nSOAPACTION: \
             \"{avt_type}#GetTransportInfo\"\r\nCONTENT-LENGTH: {}\r\n\r\n{prefixed}",
            prefixed.len()
        )
        .as_bytes(),
    );
    assert!(http10.start.starts_with("HTTP/1.0 200"), "{}", http10.start);
    // Refused before any action: a body that is not text/xml, a DOCTYPE, a
    // body past the bound, M-POST.
    let plain = exchange(
        &home.http,
        format!(
            "POST {control} HTTP/1.1\r\nCONTENT-TYPE: text/plain\r\nCONTENT-LENGTH: {}\r\n\r\n{prefixed}",
            prefixed.len()
        )
        .as_bytes(),
    );
    assert_eq!(plain.status, 415);
    let doctype = format!("<!DOCTYPE x [<!ENTITY a \"b\">]>{prefixed}");
    assert_eq!(
        post(&home.http, &control, "\"x#GetTransportInfo\"", &doctype).status,
        400,
        "DOCTYPE refused"
    );
    let huge = exchange(
        &home.http,
        format!(
            "POST {control} HTTP/1.1\r\nCONTENT-TYPE: text/xml\r\nCONTENT-LENGTH: 1000000\r\n\r\n"
        )
        .as_bytes(),
    );
    assert_eq!(huge.status, 413);
    assert_eq!(
        exchange(
            &home.http,
            format!("M-POST {control} HTTP/1.1\r\nCONTENT-LENGTH: 0\r\n\r\n").as_bytes()
        )
        .status,
        405
    );
    println!(
        "control-point: idle queries answered; error codes 401 402 701 702 706 710 711 716 717 \
         718 as specified; dialects (prefixes, default namespace, chunked, unquoted SOAPACTION, \
         HTTP/1.0) accepted; text/plain 415, DOCTYPE 400, oversize 413, M-POST 405"
    );
}

#[test]
fn eventing_follows_gena_and_refuses_callbacks_off_the_subnets() {
    let home = Home::start(&["kitchen"]);
    let kitchen = wait("the kitchen", || home.devices().into_iter().next());
    let events = Events::start();

    // Subscribe to all three: 200, a SID, the granted TIMEOUT, then SEQ 0
    // with every evented variable.
    let mut sids = Vec::new();
    for service in Service::AV {
        let request = client::subscribe_request(
            &kitchen.path(service, "event"),
            &home.http,
            &events.callback(service.path()),
            Some(300),
        );
        let before = events.log.lock().unwrap().len();
        // The server writes the response before it queues the initial
        // event; what can be seen from here is that the response is whole
        // and that SEQ 0 follows on a connection of its own.
        let answer = exchange(&home.http, request.as_bytes());
        assert_eq!(answer.status, 200, "{}", answer.start);
        let sid = answer.headers.get("SID").unwrap().to_string();
        assert!(sid.starts_with("uuid:"), "{sid}");
        assert_eq!(answer.headers.get("TIMEOUT"), Some("Second-1800"));
        assert_eq!(answer.headers.get("CONTENT-LENGTH"), Some("0"));
        let initial = wait("the initial event", || events.of(&sid).first().cloned());
        assert_eq!(initial.seq, 0);
        assert_eq!(initial.path, format!("/{}", service.path()));
        assert_eq!(before, events.log.lock().unwrap().len() - 1);
        match service {
            Service::ConnectionManager => {
                let names: Vec<&str> = initial.vars.iter().map(|(n, _)| n.as_str()).collect();
                assert_eq!(
                    names,
                    [
                        "SourceProtocolInfo",
                        "SinkProtocolInfo",
                        "CurrentConnectionIDs"
                    ]
                );
            }
            Service::AvTransport => {
                let change = initial.last_change().unwrap();
                assert_eq!(change.get("TransportState"), Some("NO_MEDIA_PRESENT"));
                assert_eq!(change.get("CurrentTransportActions"), Some(""));
                assert!(change.changes.len() >= 20, "every evented variable");
                assert!(change.get("RelativeTimePosition").is_none());
            }
            Service::RenderingControl => {
                let change = initial.last_change().unwrap();
                assert_eq!(change.get("Volume"), Some("100"));
                assert_eq!(change.get("Mute"), Some("0"));
                assert!(change
                    .changes
                    .iter()
                    .filter(|c| c.name == "Volume" || c.name == "Mute")
                    .all(|c| c.channel.as_deref() == Some("Master")));
            }
            _ => unreachable!("only the AV services are walked here"),
        }
        sids.push(sid);
    }
    let rcs_sid = sids[1].clone();
    let rcs_event = kitchen.path(RCS, "event");

    // Renew: the same SID, no initial event.
    let renew = exchange(
        &home.http,
        client::renew_request(&rcs_event, &home.http, &rcs_sid, Some(1800)).as_bytes(),
    );
    assert_eq!(renew.status, 200);
    assert_eq!(renew.headers.get("SID"), Some(rcs_sid.as_str()));
    thread::sleep(Duration::from_millis(300));
    assert_eq!(events.of(&rcs_sid).len(), 1, "no initial event on renewal");

    // Refusals.
    let request = |head: String| exchange(&home.http, head.as_bytes()).status;
    let cb = events.callback("x");
    assert_eq!(
        request(format!(
            "SUBSCRIBE {rcs_event} HTTP/1.1\r\nHOST: x\r\nSID: {rcs_sid}\r\nNT: upnp:event\r\n\r\n"
        )),
        400,
        "SID with NT"
    );
    assert_eq!(
        request(format!(
            "SUBSCRIBE {rcs_event} HTTP/1.1\r\nHOST: x\r\nNT: upnp:event\r\n\r\n"
        )),
        412,
        "no CALLBACK"
    );
    assert_eq!(
        request(format!(
            "SUBSCRIBE {rcs_event} HTTP/1.1\r\nHOST: x\r\nCALLBACK: <{cb}>\r\nNT: upnp:other\r\n\r\n"
        )),
        412,
        "NT wrong"
    );
    assert_eq!(
        request(client::renew_request(
            &rcs_event,
            "x",
            "uuid:00000000-0000-4000-8000-000000000000",
            None
        )),
        412,
        "renewing an unknown SID"
    );
    assert_eq!(
        request(client::unsubscribe_request(
            &rcs_event,
            "x",
            "uuid:00000000-0000-4000-8000-000000000000"
        )),
        412,
        "cancelling an unknown SID"
    );
    for (callback, why) in [
        (
            "http://203.0.113.5:8080/cb",
            "an address outside the subnets",
        ),
        (
            "http://127.0.0.2:8080/cb",
            "a loopback address that is not the requester",
        ),
        ("http://169.254.7.7:8080/cb", "a link-local address"),
        ("http://events.example:8080/cb", "a host name"),
        ("https://127.0.0.1:8443/cb", "not http"),
    ] {
        assert_eq!(
            request(client::subscribe_request(&rcs_event, "x", callback, None)),
            412,
            "{why}"
        );
    }
    // One good URL beside a bad one is still refused: every URL must pass.
    assert_eq!(
        request(format!(
            "SUBSCRIBE {rcs_event} HTTP/1.1\r\nHOST: x\r\nCALLBACK: <{cb}><http://203.0.113.5/cb>\r\nNT: upnp:event\r\n\r\n"
        )),
        412
    );

    // Moderation: 50 SetVolume calls in a burst reach the subscriber as
    // events at least a moderation period apart, SEQ rising by one, the last
    // holding the final value.
    let started = Instant::now();
    for volume in 1..=50 {
        home.ok(
            &kitchen,
            RCS,
            "SetVolume",
            &[
                ("InstanceID", "0"),
                ("Channel", "Master"),
                ("DesiredVolume", &volume.to_string()),
            ],
        );
    }
    let burst = started.elapsed();
    let seen = wait("the final volume", || {
        let seen = events.of(&rcs_sid);
        (seen.last().and_then(|n| n.var("Volume")).as_deref() == Some("50")).then_some(seen)
    });
    for pair in seen.windows(2) {
        assert_eq!(pair[1].seq, pair[0].seq + 1, "SEQ rises by exactly one");
    }
    let moderated = &seen[1..];
    assert!(
        moderated.len() as u128 <= burst.as_millis() / 200 + 2,
        "{} events for a burst of {burst:?}",
        moderated.len()
    );
    for pair in moderated.windows(2) {
        let gap = pair[1].at.duration_since(pair[0].at);
        // Arrival times, so some tolerance for this process's own scheduling.
        assert!(gap >= Duration::from_millis(100), "events {gap:?} apart");
    }

    // A second subscriber whose listener is gone delays nobody, and its
    // subscription lives on.
    let dead = {
        let gone = TcpListener::bind("127.0.0.1:0").unwrap();
        let callback = format!("http://{}/dead", gone.local_addr().unwrap());
        // Its initial event is delivered while the listener still accepts.
        let sid = home.subscribe(&kitchen, RCS, &callback);
        let (mut stream, _) = gone.accept().unwrap();
        let mut scratch = [0u8; 4096];
        let _ = stream.read(&mut scratch);
        let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
        sid
    };
    thread::sleep(Duration::from_millis(300));
    let before = events.of(&rcs_sid).len();
    let asked = Instant::now();
    home.ok(
        &kitchen,
        RCS,
        "SetVolume",
        &[
            ("InstanceID", "0"),
            ("Channel", "Master"),
            ("DesiredVolume", "33"),
        ],
    );
    let arrived = wait("the live subscriber's event", || {
        events.of(&rcs_sid).get(before).map(|n| n.at)
    });
    let delay = arrived.duration_since(asked);
    assert!(
        delay < Duration::from_millis(900),
        "delivered after {delay:?}"
    );
    assert_eq!(
        request(client::renew_request(&rcs_event, "x", &dead, None)),
        200,
        "the dead subscriber's subscription is kept"
    );

    // UNSUBSCRIBE: no more events.
    assert_eq!(
        request(client::unsubscribe_request(&rcs_event, "x", &rcs_sid)),
        200
    );
    let count = events.of(&rcs_sid).len();
    home.ok(
        &kitchen,
        RCS,
        "SetVolume",
        &[
            ("InstanceID", "0"),
            ("Channel", "Master"),
            ("DesiredVolume", "44"),
        ],
    );
    thread::sleep(Duration::from_millis(500));
    assert_eq!(events.of(&rcs_sid).len(), count, "cancelled");
    assert_eq!(
        request(client::renew_request(&rcs_event, "x", &rcs_sid, None)),
        412
    );
    println!(
        "control-point: eventing: 3 subscriptions with SEQ 0 after the response; 50 SetVolume in \
         {} ms gave {} moderated events, last Volume 50; with a dead second subscriber the live \
         one was served {} ms after the action; 11 bad SUBSCRIBE and UNSUBSCRIBE forms refused \
         (400 or 412)",
        burst.as_millis(),
        moderated.len(),
        delay.as_millis()
    );
}

/// What `plays_a_gapless_pair` measured.
struct Gapless {
    /// STOPPED or TRANSITIONING events between the two tracks.
    breaks: usize,
    /// Frames of the whole signal, and the frame the two files join at.
    total: usize,
    cut: usize,
    /// Per room: its id, the frames it received that are not silence, and the
    /// largest difference from the expected signal.
    rooms: Vec<(String, usize, i32)>,
}

impl Gapless {
    /// `kitchen frames expected N received N max diff 0; den ...`.
    fn per_room(&self) -> String {
        self.rooms
            .iter()
            .map(|(room, received, worst)| {
                format!(
                    "{room} frames expected {} received {received} max diff {worst}",
                    self.total
                )
            })
            .collect::<Vec<_>>()
            .join("; ")
    }
}

/// The scripted control point plays two tracks to `renderer` with
/// SetNextAVTransportURI, and everything acceptance line C says about that
/// is asserted: the events at the boundary, the metadata in the control
/// state of every room in `rooms`, and the PCM every one of those rooms
/// received (each `Heard` is a real client session attached to its room),
/// sample for sample. With more than one room, the rooms must also have
/// received the same frames at the same places on the server's chunk grid.
fn plays_a_gapless_pair(home: &Home, renderer: &Device, rooms: &[(&str, &Heard)]) -> Gapless {
    let events = Events::start();
    let media = MediaServer::start();

    // 5 s of one signal, cut at a frame that is no multiple of a chunk.
    let (total, cut) = (240_050usize, 144_007usize);
    let signal = ramp(0, total);
    let first = media.serve("/first.wav", wav(48_000, &signal[..cut]), "audio/wav");
    let second = media.serve("/second.wav", wav(48_000, &signal[cut..]), "audio/wav");
    let first_didl = didl(
        "Low Tide",
        "The Harbour Lights",
        "Salt",
        &first,
        "audio/wav",
        "0:00:03",
    );
    let second_didl = didl(
        "Slack Water",
        "The Harbour Lights",
        "Salt",
        &second,
        "audio/wav",
        "0:00:02",
    );

    let sid = home.subscribe(renderer, AVT, &events.callback("avt"));
    assert!(matches!(
        home.set_uri(renderer, &first, &first_didl),
        SoapReply::Response { .. }
    ));
    assert_eq!(home.transport(renderer).0, "STOPPED");
    assert!(home
        .avt(renderer, "GetCurrentTransportActions")
        .value("Actions")
        .unwrap()
        .contains("Play"));
    home.play(renderer);
    assert!(matches!(
        home.set_next(renderer, &second, &second_didl),
        SoapReply::Response { .. }
    ));
    let info = home.avt(renderer, "GetMediaInfo");
    assert_eq!(info.value("NextURI"), Some(second.as_str()));
    assert_eq!(info.value("NextURIMetaData"), Some(second_didl.as_str()));

    let playing = events.until(&sid, 0, "TransportState", "PLAYING");
    // Track 1 in every room's control state: what the app, chorusctl and
    // MQTT show.
    for (id, _) in rooms {
        let room = wait("track 1 in the room's state", || {
            let room = home.room(id);
            (room.contains(r#""state":"playing""#) && room.contains(r#""title":"Low Tide""#))
                .then_some(room)
        });
        for wanted in [
            r#""source":"player:p0""#,
            r#""title":"Low Tide""#,
            r#""artist":"The Harbour Lights""#,
            r#""album":"Salt""#,
            r#""art_url":"http://192.0.2.10:8200/art/cover.jpg""#,
            r#""via":"upnp""#,
        ] {
            assert!(room.contains(wanted), "{wanted} in {room}");
        }
    }
    assert_eq!(
        home.avt(renderer, "GetPositionInfo").value("TrackDuration"),
        Some("0:00:03"),
        "the decoder's duration, 144007 frames"
    );

    // The boundary, as the control point's NOTIFY listener saw it.
    let swapped = events.until(&sid, playing, "AVTransportURI", &second);
    for (id, _) in rooms {
        let room = wait("track 2 in the room's state", || {
            let room = home.room(id);
            room.contains(r#""title":"Slack Water""#).then_some(room)
        });
        for wanted in [
            r#""artist":"The Harbour Lights""#,
            r#""album":"Salt""#,
            r#""via":"upnp""#,
        ] {
            assert!(room.contains(wanted), "{wanted} in {room}");
        }
    }
    let position = home.avt(renderer, "GetPositionInfo");
    assert_eq!(position.value("TrackURI"), Some(second.as_str()));
    assert_eq!(position.value("TrackMetaData"), Some(second_didl.as_str()));
    assert_eq!(position.value("TrackDuration"), Some("0:00:02"));
    assert_eq!(home.transport(renderer).0, "PLAYING");

    // The end of track 2: STOPPED, evented.
    let stopped = events.until(&sid, swapped, "TransportState", "STOPPED");
    let all = events.of(&sid);
    let between = &all[playing..stopped - 1];
    let states: Vec<String> = between
        .iter()
        .filter_map(|n| n.var("TransportState"))
        .collect();
    let breaks = states
        .iter()
        .filter(|s| *s == "STOPPED" || *s == "TRANSITIONING")
        .count();
    assert_eq!(
        breaks, 0,
        "no STOPPED and no TRANSITIONING between the tracks: {states:?}"
    );
    let swaps: Vec<&Notified> = all
        .iter()
        .filter(|n| n.var("AVTransportURI").as_deref() == Some(second.as_str()))
        .collect();
    assert_eq!(swaps.len(), 1, "exactly one LastChange swaps the URI");
    let swap = swaps[0].last_change().unwrap();
    assert_eq!(
        swap.get("AVTransportURIMetaData"),
        Some(second_didl.as_str())
    );
    assert_eq!(swap.get("CurrentTrackURI"), Some(second.as_str()));
    assert_eq!(swap.get("CurrentTrackMetaData"), Some(second_didl.as_str()));
    assert_eq!(swap.get("CurrentTrackDuration"), Some("0:00:02"));
    assert_eq!(swap.get("NextAVTransportURI"), Some(""));
    assert_eq!(swap.get("NextAVTransportURIMetaData"), Some(""));
    assert_eq!(swap.get("TransportState"), None, "the state did not change");
    for pair in all.windows(2) {
        assert_eq!(pair[1].seq, pair[0].seq + 1);
    }

    // What every room played: the uncut signal, once, sample for sample.
    let mut measured = Vec::new();
    let mut placements: Vec<Vec<Placed>> = Vec::new();
    for (id, heard) in rooms {
        let (frames, at) = wait("the whole signal in the room", || {
            let frames = heard.placed_since(0);
            let start = frames.iter().position(|p| p.3 == signal[0])?;
            (frames.len() >= start + total + 4_800).then_some((frames, start))
        });
        let mut worst = 0i32;
        for (n, want) in signal.iter().enumerate() {
            let got = frames[at + n].3;
            for c in 0..2 {
                worst = worst.max((i32::from(got[c]) - i32::from(want[c])).abs());
            }
        }
        let received = frames.iter().filter(|p| p.3 != SILENCE).count();
        let sequences: Vec<u32> = frames[at..at + total].iter().map(|p| p.0).collect();
        assert!(
            sequences
                .windows(2)
                .all(|w| w[1] == w[0] || w[1] == w[0].wrapping_add(1)),
            "{id}: no chunk is missing inside the two tracks"
        );
        assert_eq!(worst, 0, "{id}: the two files join sample for sample");
        assert_eq!(
            received, total,
            "{id}: nothing else was heard, and no frame is silence"
        );
        assert!(
            frames[at + total..].iter().all(|p| p.3 == SILENCE),
            "{id}: silence after the end"
        );
        // The join is inside a chunk, not at the edge of one: the frame
        // before it and the frame after it came in the same chunk.
        assert_eq!(
            frames[at + cut - 1].0,
            frames[at + cut].0,
            "{id}: the join is not at a chunk boundary"
        );
        assert_ne!(frames[at + cut].2, 0, "{id}: the join is inside a chunk");
        measured.push((id.to_string(), received, worst));
        placements.push(frames[at..at + total].to_vec());
    }
    // One stream for the whole group: every room received each frame in the
    // chunk of the same sequence, with the same timestamp, at the same index.
    for (n, other) in placements.iter().enumerate().skip(1) {
        let differs = placements[0].iter().zip(other).position(|(a, b)| a != b);
        assert_eq!(
            differs,
            None,
            "{} and {} received the signal at different places on the chunk grid: {:?} and {:?}",
            rooms[0].0,
            rooms[n].0,
            differs.map(|d| placements[0][d]),
            differs.map(|d| other[d])
        );
    }
    // After the end the rooms play nothing and show nothing.
    for (id, _) in rooms {
        let room = wait("the room is released", || {
            let room = home.room(id);
            (!room.contains("now_playing")).then_some(room)
        });
        assert!(!room.contains(r#""source":"player"#), "{room}");
    }
    Gapless {
        breaks,
        total,
        cut,
        rooms: measured,
    }
}

#[test]
fn a_control_point_plays_two_tracks_gapless_with_metadata() {
    let home = Home::start(&["kitchen"]);
    let kitchen = wait("the kitchen", || home.devices().into_iter().next());
    let heard = home.listen_in("kitchen");
    let g = plays_a_gapless_pair(&home, &kitchen, &[("kitchen", &heard)]);
    let (_, received, worst) = &g.rooms[0];
    println!(
        "control-point: gapless join: {} STOPPED/TRANSITIONING events between tracks; 1 \
         LastChange swapped AVTransportURI, metadata and duration and cleared \
         NextAVTransportURI; frames expected {} received {received}; max diff {worst}; \
         join at frame {} (not a chunk boundary)",
        g.breaks, g.total, g.cut
    );
    println!(
        "control-point: metadata: room state showed title, artist, album, art and via=upnp for \
         track 1 (Low Tide) and then track 2 (Slack Water)"
    );
}

/// The line a group's gapless test prints.
fn group_line(kind: &str, name: &str, g: &Gapless) -> String {
    format!(
        "control-point: gapless join on the {kind} group {name}: {} STOPPED/TRANSITIONING events \
         between tracks; 1 LastChange swapped AVTransportURI, metadata and duration and cleared \
         NextAVTransportURI; {}; join at frame {} (not a chunk boundary); the rooms received the \
         same frames in chunks of the same sequence and timestamp; metadata (title, artist, \
         album, art, via=upnp) in both rooms for both tracks",
        g.breaks,
        g.per_room(),
        g.cut
    )
}

#[test]
fn a_saved_group_renderer_plays_two_tracks_gapless_with_metadata_in_every_room() {
    let home = Home::start(&["kitchen", "den"]);
    home.server.applied(
        r#"{"v":2,"t":"group_save","group":"downstairs","name":"Downstairs","zones":["kitchen","den"]}"#,
    );
    // A client session in each room, before the group forms: the cast takes
    // the rooms into the group (K78) with their sessions attached.
    let (kitchen, den) = (home.listen_in("kitchen"), home.listen_in("den"));
    let downstairs = wait("the saved group's renderer", || {
        home.devices()
            .into_iter()
            .find(|d| d.model == "chorus saved group" && d.name == "Downstairs")
    });
    let g = plays_a_gapless_pair(&home, &downstairs, &[("kitchen", &kitchen), ("den", &den)]);
    println!("{}", group_line("saved", "Downstairs", &g));
}

#[test]
fn a_live_group_renderer_plays_two_tracks_gapless_with_metadata_in_every_room() {
    let home = Home::start(&["kitchen", "den"]);
    // A client session in each room, then the app joins the den to the
    // kitchen: a live group, and its renderer appears.
    let (kitchen, den) = (home.listen_in("kitchen"), home.listen_in("den"));
    home.server
        .applied(r#"{"v":2,"t":"join","zone":"den","target":"kitchen"}"#);
    let live = wait("the live group's renderer", || {
        home.devices()
            .into_iter()
            .find(|d| d.model == "chorus live group")
    });
    for room in ["kitchen", "den"] {
        let state = home.room(room);
        assert!(state.contains(r#""group":"live-"#), "{state}");
    }
    let g = plays_a_gapless_pair(&home, &live, &[("kitchen", &kitchen), ("den", &den)]);
    println!("{}", group_line("live", &live.name, &g));
}

#[test]
fn pause_seek_stop_and_the_end_of_media() {
    let home = Home::start(&["kitchen"]);
    let kitchen = wait("the kitchen", || home.devices().into_iter().next());
    let events = Events::start();
    let media = MediaServer::start();
    let long = media.serve("/long.wav", wav(48_000, &ramp(0, 6 * 48_000)), "audio/wav");
    let short = media.serve("/short.wav", wav(48_000, &ramp(7, 48_000)), "audio/wav");
    let other = media.serve("/other.wav", wav(48_000, &ramp(9, 2 * 48_000)), "audio/wav");
    let sid = home.subscribe(&kitchen, AVT, &events.callback("avt"));
    let rel = |home: &Home| {
        let info = home.avt(&kitchen, "GetPositionInfo");
        let text = info.value("RelTime").unwrap().to_string();
        let parts: Vec<u64> = text.split(':').map(|p| p.parse().unwrap()).collect();
        assert_eq!(parts.len(), 3, "H:MM:SS: {text}");
        parts[0] * 3600 + parts[1] * 60 + parts[2]
    };

    home.set_uri(&kitchen, &long, "");
    let mut at = events.until(&sid, 0, "AVTransportURI", &long);
    assert_eq!(home.transport(&kitchen).0, "STOPPED");
    home.fault(&kitchen, AVT, "Pause", &[("InstanceID", "0")], 701);
    home.play(&kitchen);
    at = events.until(&sid, at, "TransportState", "PLAYING");
    assert_eq!(
        wait("the duration", || {
            let info = home.avt(&kitchen, "GetPositionInfo");
            (info.value("TrackDuration") == Some("0:00:06")).then_some(6)
        }),
        6
    );
    wait("the position advances", || (rel(&home) >= 1).then_some(()));
    assert!(home
        .avt(&kitchen, "GetCurrentTransportActions")
        .value("Actions")
        .unwrap()
        .contains("Pause"));

    // Pause: frozen. Play: carries on.
    home.avt(&kitchen, "Pause");
    at = events.until(&sid, at, "TransportState", "PAUSED_PLAYBACK");
    let held = rel(&home);
    thread::sleep(Duration::from_millis(1200));
    assert_eq!(rel(&home), held, "RelTime is frozen while paused");
    assert!(home.room("kitchen").contains(r#""state":"paused""#));
    home.play(&kitchen);
    at = events.until(&sid, at, "TransportState", "PLAYING");

    // Seek.
    home.ok(
        &kitchen,
        AVT,
        "Seek",
        &[
            ("InstanceID", "0"),
            ("Unit", "REL_TIME"),
            ("Target", "0:00:04"),
        ],
    );
    at = events.until(&sid, at, "TransportState", "PLAYING");
    let landed = rel(&home);
    assert!(
        (4..=5).contains(&landed),
        "after a seek to 0:00:04: {landed}"
    );
    home.fault(
        &kitchen,
        AVT,
        "Seek",
        &[
            ("InstanceID", "0"),
            ("Unit", "REL_TIME"),
            ("Target", "0:59:00"),
        ],
        711,
    );

    // Stop: STOPPED, position 0, the URI kept, the room released.
    home.avt(&kitchen, "Stop");
    at = events.until(&sid, at, "TransportState", "STOPPED");
    assert_eq!(rel(&home), 0);
    assert_eq!(
        home.avt(&kitchen, "GetMediaInfo").value("CurrentURI"),
        Some(long.as_str())
    );
    wait("the room is released", || {
        (!home.room("kitchen").contains("now_playing")).then_some(())
    });

    // The next URI: set, replaced, cleared.
    home.set_uri(&kitchen, &short, "");
    home.set_next(&kitchen, &long, "");
    home.set_next(&kitchen, &other, "");
    assert_eq!(
        home.avt(&kitchen, "GetMediaInfo").value("NextURI"),
        Some(other.as_str())
    );
    home.set_next(&kitchen, "", "");
    assert_eq!(
        home.avt(&kitchen, "GetMediaInfo").value("NextURI"),
        Some("")
    );
    // End of media with no next: STOPPED, evented, status OK.
    home.play(&kitchen);
    at = events.until(&sid, at, "TransportState", "PLAYING");
    at = events.until(&sid, at, "TransportState", "STOPPED");
    assert_eq!(
        home.transport(&kitchen),
        ("STOPPED".to_string(), "OK".to_string())
    );

    // A next URI that is not there: the current track plays to its end, then
    // STOPPED with ERROR_OCCURRED.
    home.play(&kitchen);
    assert!(matches!(
        home.set_next(&kitchen, &media.url("/missing.wav"), ""),
        SoapReply::Response { .. }
    ));
    at = events.until(&sid, at, "TransportState", "PLAYING");
    at = events.until(&sid, at, "TransportStatus", "ERROR_OCCURRED");
    assert_eq!(
        home.transport(&kitchen),
        ("STOPPED".to_string(), "ERROR_OCCURRED".to_string())
    );
    assert_eq!(
        home.avt(&kitchen, "GetMediaInfo").value("NextURI"),
        Some("")
    );

    // Next with a next URI advances at once and stays PLAYING.
    home.set_uri(&kitchen, &long, "");
    home.play(&kitchen);
    at = events.until(&sid, at, "TransportState", "PLAYING");
    home.set_next(&kitchen, &other, "");
    wait("Next is offered", || {
        home.avt(&kitchen, "GetCurrentTransportActions")
            .value("Actions")
            .unwrap()
            .contains("Next")
            .then_some(())
    });
    let asked = Instant::now();
    home.avt(&kitchen, "Next");
    let swapped = events.until(&sid, at, "AVTransportURI", &other);
    assert!(
        asked.elapsed() < Duration::from_secs(3),
        "at once, not after 6 s"
    );
    assert_eq!(home.transport(&kitchen).0, "PLAYING");
    let states: Vec<String> = events.of(&sid)[at..swapped]
        .iter()
        .filter_map(|n| n.var("TransportState"))
        .collect();
    assert!(states.iter().all(|s| s == "PLAYING"), "{states:?}");
    // An empty URI ejects.
    home.set_uri(&kitchen, "", "");
    assert_eq!(home.transport(&kitchen).0, "NO_MEDIA_PRESENT");
    println!(
        "control-point: transport: play, pause (RelTime frozen at {held} s), resume, seek to \
         0:00:04 (landed at {landed} s), stop (RelTime 0, URI kept), end of media STOPPED/OK, a \
         missing next URI STOPPED/ERROR_OCCURRED after the track, Next advanced in {} ms",
        asked.elapsed().as_millis()
    );
}

#[test]
fn metadata_is_kept_verbatim_and_tolerated_when_broken() {
    let home = Home::start(&["kitchen"]);
    let kitchen = wait("the kitchen", || home.devices().into_iter().next());
    let events = Events::start();
    let media = MediaServer::start();
    let url = media.serve("/a.wav", wav(48_000, &ramp(0, 3 * 48_000)), "audio/wav");
    let sid = home.subscribe(&kitchen, AVT, &events.callback("avt"));
    let title = "A & B <\"x\"> \u{e9}";
    let document = didl(
        title,
        "The Quiet Engines",
        "Slow & Low",
        &url,
        "audio/wav",
        "0:00:03",
    );
    assert!(matches!(
        home.set_uri(&kitchen, &url, &document),
        SoapReply::Response { .. }
    ));
    // Byte for byte after one unescape, by both Get actions and in the event
    // (which is escaped twice on the wire).
    assert_eq!(
        home.avt(&kitchen, "GetMediaInfo")
            .value("CurrentURIMetaData"),
        Some(document.as_str())
    );
    assert_eq!(
        home.avt(&kitchen, "GetPositionInfo").value("TrackMetaData"),
        Some(document.as_str())
    );
    let at = events.until(&sid, 0, "AVTransportURIMetaData", &document);
    home.play(&kitchen);
    let room = wait("the title in the room's state", || {
        let room = home.room("kitchen");
        room.contains(r#""via":"upnp""#).then_some(room)
    });
    assert!(room.contains(r#""title":"A & B <\"x\"> é""#), "{room}");
    assert!(room.contains(r#""artist":"The Quiet Engines""#));
    assert!(room.contains(r#""album":"Slow & Low""#));
    assert!(room.contains(r#""art_url":"http://192.0.2.10:8200/art/cover.jpg""#));
    home.avt(&kitchen, "Stop");
    events.until(&sid, at, "TransportState", "STOPPED");

    // No metadata, garbage, a CDATA section, a DOCTYPE: the action succeeds
    // every time, and what was sent is what comes back.
    let cdata = format!("<![CDATA[{document}]]>");
    let doctype = format!("<!DOCTYPE d [<!ENTITY e \"x\">]>{document}");
    for metadata in ["", "this is not XML <<<", "NOT_IMPLEMENTED", &doctype] {
        assert!(
            matches!(
                home.set_uri(&kitchen, &url, metadata),
                SoapReply::Response { .. }
            ),
            "{metadata}"
        );
        assert_eq!(
            home.avt(&kitchen, "GetMediaInfo")
                .value("CurrentURIMetaData"),
            Some(metadata)
        );
    }
    // CDATA goes into the request raw (not escaped), as control points send it.
    let call = client::soap_request_raw(AVT.service_type(), "SetAVTransportURI", &[]);
    let body = call.body.replace(
        "</u:SetAVTransportURI>",
        &format!(
            "<InstanceID>0</InstanceID><CurrentURI>{}</CurrentURI><CurrentURIMetaData>{cdata}\
             </CurrentURIMetaData></u:SetAVTransportURI>",
            escape_text(&url)
        ),
    );
    let answer = post(
        &home.http,
        &kitchen.path(AVT, "control"),
        &call.soapaction,
        &body,
    );
    assert_eq!(answer.status, 200, "{}", answer.body);
    // With the DOCTYPE document as metadata the room shows the file's own
    // facts, not the document's.
    home.set_uri(&kitchen, &url, &doctype);
    home.play(&kitchen);
    let room = wait("playing with refused metadata", || {
        let room = home.room("kitchen");
        room.contains(r#""state":"playing""#).then_some(room)
    });
    assert!(room.contains(r#""title":null"#), "{room}");
    println!(
        "control-point: metadata `A & B <\"x\"> \u{e9}` came back byte for byte from GetMediaInfo, \
         GetPositionInfo and LastChange and reached the room's state; empty, garbage, \
         NOT_IMPLEMENTED, CDATA and DOCTYPE metadata all left the action successful"
    );
}

#[test]
fn casting_to_a_group_takes_its_rooms() {
    let home = Home::start(&["kitchen", "den", "study"]);
    home.server.applied(
        r#"{"v":2,"t":"group_save","group":"downstairs","name":"Downstairs","zones":["kitchen","den"]}"#,
    );
    let devices = wait("four renderers", || {
        let d = home.devices();
        (d.len() == 4).then_some(d)
    });
    let by = |name: &str| devices.iter().find(|d| d.name == name).unwrap().clone();
    let (kitchen, downstairs) = (by("kitchen"), by("Downstairs"));
    let events = Events::start();
    let media = MediaServer::start();
    let one = media.serve("/one.wav", wav(48_000, &ramp(0, 20 * 48_000)), "audio/wav");
    let two = media.serve("/two.wav", wav(48_000, &ramp(5, 20 * 48_000)), "audio/wav");
    let kitchen_sid = home.subscribe(&kitchen, AVT, &events.callback("kitchen"));
    let group_sid = home.subscribe(&downstairs, AVT, &events.callback("downstairs"));

    // The kitchen plays by itself.
    home.set_uri(&kitchen, &one, "");
    home.play(&kitchen);
    let at = events.until(&kitchen_sid, 0, "TransportState", "PLAYING");
    assert!(home.room("kitchen").contains(r#""group":"kitchen""#));

    // Both players in use (the den's renderer has loaded the other): a third
    // renderer cannot load, and says so with 501 Action Failed.
    let (den, study) = (by("den"), by("study"));
    assert!(matches!(
        home.set_uri(&den, &one, ""),
        SoapReply::Response { .. }
    ));
    assert_eq!(home.set_uri(&study, &one, "").fault_code(), Some(501));
    assert_eq!(home.transport(&study).0, "NO_MEDIA_PRESENT");
    // The den ejects; its player goes back to the pool.
    home.set_uri(&den, "", "");

    // A control point casts to the saved group, which was not formed: its
    // rooms are taken (K78), the group forms and plays, and the kitchen's
    // own renderer says STOPPED.
    let ferry = didl(
        "Ferry",
        "The Harbour Lights",
        "Salt",
        &two,
        "audio/wav",
        "0:00:20",
    );
    wait("a player for the group", || {
        matches!(
            home.set_uri(&downstairs, &two, &ferry),
            SoapReply::Response { .. }
        )
        .then_some(())
    });
    home.play(&downstairs);
    let group_at = events.until(&group_sid, 0, "TransportState", "PLAYING");
    events.until(&kitchen_sid, at, "TransportState", "STOPPED");
    assert_eq!(home.transport(&kitchen).0, "STOPPED");
    let state = home.server.state();
    assert!(
        state.contains(r#"{"id":"downstairs","kind":"saved","zones":["kitchen","den"]"#),
        "{state}"
    );
    for room in ["kitchen", "den"] {
        let room = home.room(room);
        assert!(room.contains(r#""group":"downstairs""#), "{room}");
        assert!(room.contains(r#""title":"Ferry""#) && room.contains(r#""via":"upnp""#));
    }
    assert!(!home.room("study").contains("now_playing"));

    // Somebody else (the app) makes the group play the stream: the group's
    // renderer says STOPPED.
    home.server
        .applied(r#"{"v":2,"t":"take","target":"downstairs","source":"stream"}"#);
    events.until(&group_sid, group_at, "TransportState", "STOPPED");
    assert_eq!(home.transport(&downstairs).0, "STOPPED");
    // Its player went back to the pool with the rooms. Play again takes one
    // anew, loads the URI again and takes the rooms back.
    let stopped_at = events.of(&group_sid).len();
    wait("a player again", || {
        matches!(
            home.call(
                &downstairs,
                AVT,
                "Play",
                &[("InstanceID", "0"), ("Speed", "1")]
            ),
            SoapReply::Response { .. }
        )
        .then_some(())
    });
    events.until(&group_sid, stopped_at, "TransportState", "PLAYING");
    assert!(home.room("den").contains(r#""title":"Ferry""#));
    println!(
        "control-point: K78: casting to the saved group Downstairs took kitchen and den into it; \
         the kitchen's own renderer evented STOPPED; with 2 of 2 players held a third renderer \
         answered 501; the app changing the group's source evented STOPPED on the group's renderer"
    );
}

#[test]
fn volume_is_clamped_by_the_room_limit_and_the_real_value_is_evented() {
    let home = Home::start(&["kitchen", "den"]);
    home.server.applied(
        r#"{"v":2,"t":"group_save","group":"downstairs","name":"Downstairs","zones":["kitchen","den"]}"#,
    );
    let devices = wait("three renderers", || {
        let d = home.devices();
        (d.len() == 3).then_some(d)
    });
    let by = |name: &str| devices.iter().find(|d| d.name == name).unwrap().clone();
    let (kitchen, den, downstairs) = (by("kitchen"), by("den"), by("Downstairs"));
    let events = Events::start();
    let sid = home.subscribe(&kitchen, RCS, &events.callback("rcs"));
    let master = [("InstanceID", "0"), ("Channel", "Master")];
    let volume = |device: &Device| {
        home.ok(device, RCS, "GetVolume", &master)
            .value("CurrentVolume")
            .unwrap()
            .to_string()
    };
    let set = |device: &Device, v: &str| {
        home.ok(
            device,
            RCS,
            "SetVolume",
            &[
                ("InstanceID", "0"),
                ("Channel", "Master"),
                ("DesiredVolume", v),
            ],
        )
    };
    assert_eq!(volume(&kitchen), "100");
    set(&kitchen, "40");
    assert_eq!(volume(&kitchen), "40");
    let mut at = events.until(&sid, 0, "Volume", "40");
    assert!(home.room("kitchen").contains(r#""volume":0.400"#));

    // A limit of 60: SetVolume 80 succeeds, and what holds is 60, at once in
    // GetVolume and in the event; 80 is never evented.
    home.server
        .applied(r#"{"v":2,"t":"limit","zone":"kitchen","limit":0.600}"#);
    set(&kitchen, "80");
    assert_eq!(volume(&kitchen), "60", "clamped by the room's limit");
    at = events.until(&sid, at, "Volume", "60");
    assert!(events
        .of(&sid)
        .iter()
        .all(|n| n.var("Volume").as_deref() != Some("80")));
    let event = events.of(&sid)[at - 1].last_change().unwrap();
    assert!(event
        .changes
        .iter()
        .all(|c| c.channel.as_deref() == Some("Master")));

    // A change made elsewhere (the app) reaches the UPnP subscriber.
    home.server
        .applied(r#"{"v":1,"t":"volume","zone":"kitchen","volume":0.250}"#);
    at = events.until(&sid, at, "Volume", "25");
    assert_eq!(volume(&kitchen), "25");

    // Mute.
    home.ok(
        &kitchen,
        RCS,
        "SetMute",
        &[
            ("InstanceID", "0"),
            ("Channel", "Master"),
            ("DesiredMute", "1"),
        ],
    );
    events.until(&sid, at, "Mute", "1");
    assert_eq!(
        home.ok(&kitchen, RCS, "GetMute", &master)
            .value("CurrentMute"),
        Some("1")
    );
    assert!(home.room("kitchen").contains(r#""muted":true"#));

    // A group target: every room is set, each under its own limit.
    set(&downstairs, "90");
    assert_eq!(volume(&kitchen), "60", "the kitchen's limit holds");
    assert_eq!(volume(&den), "90");
    assert_eq!(volume(&downstairs), "75", "the average of 60 and 90");
    home.ok(
        &downstairs,
        RCS,
        "SetMute",
        &[
            ("InstanceID", "0"),
            ("Channel", "Master"),
            ("DesiredMute", "true"),
        ],
    );
    assert!(home.room("den").contains(r#""muted":true"#));
    home.fault(
        &kitchen,
        RCS,
        "SetVolume",
        &[
            ("InstanceID", "0"),
            ("Channel", "Master"),
            ("DesiredVolume", "101"),
        ],
        402,
    );
    println!(
        "control-point: volume: SetVolume 80 under a room limit of 0.600 gave GetVolume 60 and an \
         event Volume=60 channel=Master (80 was never evented); the app's change to 0.250 was \
         evented as 25; a group SetVolume 90 gave kitchen 60, den 90, group 75"
    );
}

#[test]
fn each_supported_format_plays_in_the_room() {
    let home = Home::start(&["kitchen"]);
    let kitchen = wait("the kitchen", || home.devices().into_iter().next());
    let heard = home.listen_in("kitchen");
    let events = Events::start();
    let media = MediaServer::start();
    let sid = home.subscribe(&kitchen, AVT, &events.callback("avt"));
    let tone = ramp(0, 14_400);
    let cases: Vec<(&str, Vec<u8>, &str, &str, usize)> = vec![
        (
            "wav-tone44-s16.wav",
            fixture("wav-tone44-s16.wav"),
            "audio/wav",
            "wav",
            0,
        ),
        (
            "flac-tone44-s16.flac",
            fixture("flac-tone44-s16.flac"),
            "audio/flac",
            "flac",
            0,
        ),
        (
            "mp3-tone44.mp3",
            fixture("mp3-tone44.mp3"),
            "audio/mpeg",
            "mp3",
            0,
        ),
        (
            "vorbis-tone44.ogg",
            fixture("vorbis-tone44.ogg"),
            "audio/ogg",
            "ogg",
            0,
        ),
        (
            "opus-tone44.opus",
            fixture("opus-tone44.opus"),
            "audio/ogg",
            "opus",
            0,
        ),
        (
            "alac-tone44-s16-moov-first.m4a",
            fixture("alac-tone44-s16-moov-first.m4a"),
            "audio/mp4",
            "m4a",
            0,
        ),
        (
            "l16-ramp48",
            l16(&tone),
            "audio/L16;rate=48000;channels=2",
            "",
            tone.len(),
        ),
    ];
    let mut at = 0;
    let mut said = Vec::new();
    for (name, bytes, content_type, extension, known) in cases {
        let expected = if known > 0 {
            known
        } else {
            frames_at_48k(&bytes, extension)
        };
        let url = media.serve(&format!("/{name}"), bytes, content_type);
        // A res full of DLNA fields is taken as text.
        let metadata = didl(
            name,
            "the generator",
            "fixtures",
            &url,
            content_type,
            "0:00:00",
        )
        .replace(
            ":*\"",
            ":DLNA.ORG_PN=LPCM;DLNA.ORG_OP=01;DLNA.ORG_FLAGS=01700000000000000000000000000000\"",
        );
        let mark = heard.chunks();
        assert!(
            matches!(
                home.set_uri(&kitchen, &url, &metadata),
                SoapReply::Response { .. }
            ),
            "{name}"
        );
        home.play(&kitchen);
        at = events.until(&sid, at, "TransportState", "STOPPED");
        // STOPPED after SetAVTransportURI may be that event; wait for the
        // end of the media by what the transport says.
        wait(&format!("{name} played to its end"), || {
            (heard.loud_since(mark) > 0 && home.transport(&kitchen).0 == "STOPPED").then_some(())
        });
        assert_eq!(home.transport(&kitchen).1, "OK", "{name}");
        thread::sleep(Duration::from_millis(100));
        let loud = heard.loud_since(mark);
        assert!(
            loud * 10 >= expected * 9 && loud <= expected,
            "{name}: {loud} frames that are not silence of {expected} decoded"
        );
        said.push(format!("{name} {loud}/{expected}"));
    }
    println!(
        "control-point: formats heard in the room (frames not silent / frames decoded at 48 kHz): {}",
        said.join(", ")
    );
}

#[test]
fn aac_is_refused_and_never_plays() {
    let mut home = Home::start(&["kitchen"]);
    let kitchen = wait("the kitchen", || home.devices().into_iter().next());
    let heard = home.listen_in("kitchen");
    let events = Events::start();
    let media = MediaServer::start();
    let sid = home.subscribe(&kitchen, AVT, &events.callback("avt"));
    let mut said = Vec::new();
    for (name, content_type) in [
        ("aac-in-mp4.m4a", "audio/mp4"),
        ("aac-adts.aac", "audio/aac"),
    ] {
        let url = media.serve(&format!("/{name}"), fixture(name), content_type);
        let mark = heard.chunks();
        let from = events.of(&sid).len();
        // The URI is accepted (nothing is fetched inside the action); the
        // refusal arrives as STOPPED with ERROR_OCCURRED.
        assert!(matches!(
            home.set_uri(&kitchen, &url, ""),
            SoapReply::Response { .. }
        ));
        events.until(&sid, from, "TransportStatus", "ERROR_OCCURRED");
        assert_eq!(
            home.transport(&kitchen),
            ("STOPPED".to_string(), "ERROR_OCCURRED".to_string())
        );
        // Play tries again and fails again; PLAYING is never evented.
        home.play(&kitchen);
        wait("the second refusal", || {
            (home.transport(&kitchen) == ("STOPPED".to_string(), "ERROR_OCCURRED".to_string()))
                .then_some(())
        });
        thread::sleep(Duration::from_millis(600));
        let states: Vec<String> = events.of(&sid)[from..]
            .iter()
            .filter_map(|n| n.var("TransportState"))
            .collect();
        assert!(!states.iter().any(|s| s == "PLAYING"), "{name}: {states:?}");
        assert_eq!(heard.loud_since(mark), 0, "{name}: not one sample");
        assert!(!home.room("kitchen").contains(r#""state":"playing""#));
        said.push(name);
    }
    home.server
        .wait_for_all(&["upnp media failed", "unsupported: aac"]);
    println!(
        "control-point: AAC refused by name (`unsupported: aac`): {} ended STOPPED + \
         ERROR_OCCURRED, PLAYING never evented, 0 samples heard",
        said.join(" and ")
    );
}

#[test]
fn media_urls_obey_the_fetch_rule() {
    let mut home = Home::start(&["kitchen"]);
    let kitchen = wait("the kitchen", || home.devices().into_iter().next());
    let media = MediaServer::start();
    // Not http: refused inside the action, nothing stored.
    for uri in [
        "file:///etc/hostname",
        "ftp://192.0.2.9/a.flac",
        "rtsp://192.0.2.9/a",
    ] {
        assert_eq!(
            home.set_uri(&kitchen, uri, "").fault_code(),
            Some(716),
            "{uri}"
        );
    }
    assert_eq!(home.transport(&kitchen).0, "NO_MEDIA_PRESENT");
    // Refused by the fetch policy before any connection: the refusal arrives
    // as ERROR_OCCURRED.
    let own_control = format!("http://{}/api/state", home.server.control);
    let own_upnp = format!("http://{}/upnp/{}/desc.xml", home.http, kitchen.udn);
    let redirect = media.redirect("/to-control", &own_control);
    let mut reasons = Vec::new();
    let mut refused = |uri: &str, what: &str| {
        let before = failures(&mut home).len();
        assert!(
            matches!(home.set_uri(&kitchen, uri, ""), SoapReply::Response { .. }),
            "{what}"
        );
        wait(what, || {
            (home.transport(&kitchen) == ("STOPPED".to_string(), "ERROR_OCCURRED".to_string()))
                .then_some(())
        });
        // The player's own words: the fetch policy refused it, by name.
        let line = wait("the failure's line", || {
            failures(&mut home).get(before).cloned()
        });
        let reason = line
            .split("reason=\"")
            .nth(1)
            .and_then(|r| r.split('"').next())
            .unwrap_or("")
            .to_string();
        assert!(reason.starts_with("refused"), "{what}: {line}");
        reasons.push(format!("{what} ({reason})"));
        home.set_uri(&kitchen, "", "");
    };
    refused("http://169.254.10.10/a.flac", "a link-local address");
    refused(&own_control, "the server's own control port");
    refused(&own_upnp, "the server's own renderer port");
    refused(&redirect, "a redirect to the server's own control port");
    // The audio port is refused the same way when it is configured; this
    // server's is `--listen 127.0.0.1:0`, which names no port to refuse
    // (the policy is built from the configuration before the audio socket
    // is bound), so it is not asserted here.
    drop(home);

    // Without --media-allow-loopback (every deployment), a loopback URL is
    // refused and no connection is made to it.
    let strict = Home::start_with(&["kitchen"], &[]);
    let kitchen = wait("the kitchen", || strict.devices().into_iter().next());
    let url = media.serve("/a.wav", wav(48_000, &ramp(0, 4_800)), "audio/wav");
    let before = media.accepts();
    assert!(matches!(
        strict.set_uri(&kitchen, &url, ""),
        SoapReply::Response { .. }
    ));
    wait("a loopback URL refused", || {
        (strict.transport(&kitchen) == ("STOPPED".to_string(), "ERROR_OCCURRED".to_string()))
            .then_some(())
    });
    assert_eq!(media.accepts(), before, "nothing was fetched");
    println!(
        "control-point: fetch rule: file, ftp and rtsp 716; refused by the policy with \
         ERROR_OCCURRED: {}; a loopback URL without --media-allow-loopback ERROR_OCCURRED with 0 \
         connections",
        reasons.join("; ")
    );
}

/// Every "upnp media failed" line the server has printed so far.
fn failures(home: &mut Home) -> Vec<String> {
    home.server.drain();
    home.server
        .seen
        .iter()
        .filter(|l| l.contains("upnp media failed"))
        .cloned()
        .collect()
}

fn byebyes(notes: &[(SsdpMessage, Instant)], udn: &str) -> Vec<SsdpMessage> {
    notes
        .iter()
        .filter(|(m, _)| m.kind == SsdpKind::Byebye && m.usn.starts_with(&format!("uuid:{udn}")))
        .map(|(m, _)| m.clone())
        .collect()
}

fn alives(notes: &[(SsdpMessage, Instant)], udn: &str) -> Vec<SsdpMessage> {
    notes
        .iter()
        .filter(|(m, _)| m.kind == SsdpKind::Alive && m.usn.starts_with(&format!("uuid:{udn}")))
        .map(|(m, _)| m.clone())
        .collect()
}

#[test]
fn a_vanished_group_says_byebye_and_its_description_is_gone() {
    let mut home = Home::start(&["kitchen", "den"]);
    let events = Events::start();
    home.server
        .applied(r#"{"v":2,"t":"join","zone":"den","target":"kitchen"}"#);
    home.server.applied(
        r#"{"v":2,"t":"group_save","group":"downstairs","name":"Downstairs","zones":["kitchen","den"]}"#,
    );
    // join made a live group; saving a group over the same rooms does not
    // dissolve it.
    let devices = wait("the live group and the saved group", || {
        let d = home.devices();
        (d.iter().any(|d| d.model == "chorus live group")
            && d.iter().any(|d| d.model == "chorus saved group"))
        .then_some(d)
    });
    let live = devices
        .iter()
        .find(|d| d.model == "chorus live group")
        .unwrap()
        .clone();
    let saved = devices
        .iter()
        .find(|d| d.model == "chorus saved group")
        .unwrap()
        .clone();
    assert_eq!(live.name, "den + kitchen");
    let path = format!("/upnp/{}/desc.xml", live.udn);
    assert_eq!(get(&home.http, &path).status, 200);
    let sid = home.subscribe(&live, AVT, &events.callback("live"));
    wait("its initial event", || events.of(&sid).first().cloned());
    assert_eq!(
        alives(home.notifications(Duration::from_millis(600)), &live.udn).len(),
        12
    );

    // The live group dissolves: 6 byebye messages (sent twice) with the
    // BOOTID it had, no description, no subscription.
    home.server.applied(r#"{"v":2,"t":"take","target":"den"}"#);
    let gone = wait("the live group's byebye", || {
        let b = byebyes(home.notifications(Duration::from_millis(300)), &live.udn);
        (b.len() >= 12).then_some(b)
    });
    assert_eq!(gone.len(), 12, "6 messages, 2 sets");
    let targets: BTreeSet<&str> = gone.iter().map(|m| m.target.as_str()).collect();
    assert_eq!(targets.len(), 6);
    assert!(gone.iter().all(|m| m.boot_id == Some(live.boot_id)));
    assert!(gone.iter().all(|m| m.location.is_none()));
    assert_eq!(
        get(&home.http, &path).status,
        404,
        "its description is gone"
    );
    assert_eq!(
        exchange(
            &home.http,
            client::renew_request(&live.path(AVT, "event"), &home.http, &sid, None).as_bytes()
        )
        .status,
        404,
        "its subscriptions went with it"
    );
    assert!(home.devices().iter().all(|d| d.udn != live.udn));

    // A saved group that is deleted goes the same way.
    home.server
        .applied(r#"{"v":2,"t":"group_delete","group":"downstairs"}"#);
    let gone = wait("the saved group's byebye", || {
        let b = byebyes(home.notifications(Duration::from_millis(300)), &saved.udn);
        (b.len() >= 12).then_some(b)
    });
    assert_eq!(gone.len(), 12);
    assert_eq!(
        get(&home.http, &format!("/upnp/{}/desc.xml", saved.udn)).status,
        404
    );
    println!(
        "control-point: a dissolved live group sent 12 byebye (6 x 2 sets) with its BOOTID {}, \
         its description answered 404 and its subscription was gone; a deleted saved group the same",
        live.boot_id
    );
}

#[test]
fn a_live_group_keeps_its_identity_when_the_same_rooms_re_form() {
    let mut home = Home::start(&["kitchen", "den", "study"]);
    home.server
        .applied(r#"{"v":2,"t":"join","zone":"den","target":"kitchen"}"#);
    let first = wait("the live group", || {
        home.devices()
            .into_iter()
            .find(|d| d.model == "chorus live group")
    });
    home.server.applied(r#"{"v":2,"t":"take","target":"den"}"#);
    wait("it is gone", || {
        home.devices()
            .iter()
            .all(|d| d.udn != first.udn)
            .then_some(())
    });
    // Another live group in between takes the id `live-1`.
    home.server
        .applied(r#"{"v":2,"t":"join","zone":"study","target":"den"}"#);
    let other = wait("another live group", || {
        home.devices()
            .into_iter()
            .find(|d| d.model == "chorus live group")
    });
    assert_ne!(other.udn, first.udn, "other rooms, another device");
    // The same two rooms again, joined the other way round.
    home.server
        .applied(r#"{"v":2,"t":"take","target":"study"}"#);
    home.server
        .applied(r#"{"v":2,"t":"join","zone":"kitchen","target":"den"}"#);
    let again = wait("the same rooms as a live group", || {
        home.devices()
            .into_iter()
            .find(|d| d.name == "den + kitchen")
    });
    assert_eq!(again.udn, first.udn, "the same members, the same UDN");
    assert_eq!(again.config_id, first.config_id);
    assert!(
        again.boot_id > first.boot_id,
        "a larger BOOTID: {} then {}",
        first.boot_id,
        again.boot_id
    );

    // A rename: byebye, then alive with a new CONFIGID, a larger BOOTID, the
    // new friendlyName and the same UDN.
    let kitchen = home.device("kitchen");
    home.notifications(Duration::from_millis(400));
    let seen = home.notes.len();
    home.server
        .applied(r#"{"v":1,"t":"name","zone":"kitchen","name":"Galley"}"#);
    let renamed = wait("the renamed room", || {
        home.devices().into_iter().find(|d| d.name == "Galley")
    });
    assert_eq!(renamed.udn, kitchen.udn, "a rename keeps the UDN");
    assert_ne!(renamed.config_id, kitchen.config_id, "a new CONFIGID");
    assert!(renamed.boot_id > kitchen.boot_id);
    let notes: Vec<(SsdpMessage, Instant)> =
        home.notifications(Duration::from_millis(600))[seen..].to_vec();
    let bye = byebyes(&notes, &kitchen.udn);
    let alive = alives(&notes, &kitchen.udn);
    assert_eq!(bye.len(), 12);
    assert!(bye.iter().all(|m| m.boot_id == Some(kitchen.boot_id)));
    assert!(alive.len() >= 6);
    assert!(alive
        .iter()
        .all(|m| m.boot_id == Some(renamed.boot_id) && m.config_id == Some(renamed.config_id)));
    let first_alive = notes
        .iter()
        .position(|(m, _)| {
            m.kind == SsdpKind::Alive && m.usn.starts_with(&format!("uuid:{}", kitchen.udn))
        })
        .unwrap();
    let last_bye = notes
        .iter()
        .rposition(|(m, _)| {
            m.kind == SsdpKind::Byebye && m.usn.starts_with(&format!("uuid:{}", kitchen.udn))
        })
        .unwrap();
    assert!(last_bye < first_alive, "byebye first, then alive");
    // The live group's name follows its rooms' names.
    wait("the live group is called by the new name", || {
        home.devices()
            .iter()
            .any(|d| d.name == "den + Galley" && d.udn == first.udn)
            .then_some(())
    });
    println!(
        "control-point: live group re-formed from the same rooms: same UDN {}, BOOTID {} then {}; \
         a rename kept the UDN, changed CONFIGID {} to {} and sent byebye before alive",
        first.udn, first.boot_id, again.boot_id, kitchen.config_id, renamed.config_id
    );
}

#[test]
fn identities_are_uuid_v5_and_survive_a_restart() {
    let rooms = ["kitchen", "den"];
    let mut home = Home::start(&rooms);
    home.server.applied(
        r#"{"v":2,"t":"group_save","group":"downstairs","name":"Downstairs","zones":["kitchen","den"]}"#,
    );
    home.server
        .applied(r#"{"v":2,"t":"join","zone":"den","target":"kitchen"}"#);
    let before = wait("four renderers", || {
        let d = home.devices();
        (d.len() == 4).then_some(d)
    });
    // What the UDNs are made of: UUID version 5 over the server's persisted
    // key fingerprint and the target's name.
    let line = home.server.wait_for("identity id=");
    let key = field(&line, "key=");
    let expect = |target: Target<'_>| udn(&CHORUS_NAMESPACE, &key, &target).to_string();
    let by = |devices: &[Device], name: &str| {
        devices
            .iter()
            .find(|d| d.name == name)
            .unwrap_or_else(|| panic!("{name} in {devices:?}"))
            .clone()
    };
    assert_eq!(by(&before, "kitchen").udn, expect(Target::Room("kitchen")));
    assert_eq!(by(&before, "den").udn, expect(Target::Room("den")));
    assert_eq!(
        by(&before, "Downstairs").udn,
        expect(Target::Group("downstairs"))
    );
    assert_eq!(
        by(&before, "den + kitchen").udn,
        expect(Target::Live(&["kitchen", "den"]))
    );
    for d in &before {
        assert_eq!(d.udn.as_bytes()[14], b'5', "version 5: {}", d.udn);
    }

    // Restart on the same identity and state. BOOTID is the wall clock's
    // second, so the restart is held until the second has turned.
    let dir = home.dir.clone();
    let notify = home.notify.try_clone().unwrap();
    drop(home);
    thread::sleep(Duration::from_millis(1100));
    let again = Home::start_in(dir, notify, &rooms, &["--media-allow-loopback"]);
    let after = wait("four renderers again", || {
        let d = again.devices();
        (d.len() == 4).then_some(d)
    });
    let mut equal = 0;
    for d in &before {
        let now = by(&after, &d.name);
        assert_eq!(now.udn, d.udn, "{}: the same UDN", d.name);
        assert_eq!(now.config_id, d.config_id, "{}: the same CONFIGID", d.name);
        assert!(now.boot_id > d.boot_id, "{}: a larger BOOTID", d.name);
        equal += 1;
    }
    // Another server (another key) has other UDNs for the same rooms.
    let other = Home::start(&rooms);
    let theirs = wait("the other server's renderers", || {
        let d = other.devices();
        (d.len() == 2).then_some(d)
    });
    assert_ne!(by(&theirs, "kitchen").udn, by(&before, "kitchen").udn);
    println!(
        "control-point: identities stable across restart: {equal} of {} UDNs equal (2 rooms, 1 \
         saved group, 1 live group), CONFIGIDs equal, BOOTIDs larger; a second server's kitchen \
         has another UDN",
        before.len()
    );
}

#[test]
fn stopping_the_server_says_byebye_for_every_renderer() {
    // The slot shape serves until its source fails, so the stream is a file
    // this test takes away: the server then ends by itself, through main's
    // own return, which is where the byebye is said. (A killed process says
    // nothing; its advertisements expire after max-age.)
    let dir = fresh_dir();
    let pcm = dir.join("stream.pcm");
    std::fs::write(&pcm, vec![0u8; 48_000 * 4 / 2]).unwrap();
    let pcm_path = pcm.display().to_string();
    let notify = UdpSocket::bind("127.0.0.1:0").unwrap();
    let mut home = Home::start_in(dir, notify, &["kitchen", "den"], &["--source", &pcm_path]);
    let devices = wait("two renderers", || {
        let d = home.devices();
        (d.len() == 2).then_some(d)
    });
    home.notifications(Duration::from_millis(600));
    std::fs::remove_file(&pcm).unwrap();
    assert_eq!(
        home.server.exited(Duration::from_secs(20)),
        Some(false),
        "the server ends when its source cannot be reopened"
    );
    let notes = home.notifications(Duration::from_millis(500)).to_vec();
    for device in &devices {
        let bye = byebyes(&notes, &device.udn);
        assert_eq!(bye.len(), 12, "{}: 6 byebye, 2 sets", device.name);
        assert!(bye.iter().all(|m| m.boot_id == Some(device.boot_id)));
    }
    home.server.wait_for("upnp stopped byebye_renderers=2");
    println!(
        "control-point: shutdown: byebye for 2 of 2 renderers (12 messages each) when the server ended"
    );
}

#[test]
fn sixteen_renderers_are_announced_searched_and_subscribed() {
    let rooms: Vec<String> = (1..=16).map(|i| format!("room-{i}")).collect();
    let rooms: Vec<&str> = rooms.iter().map(String::as_str).collect();
    let mut home = Home::start(&rooms);
    let alive = wait("every start-up announce", || {
        let notes = home.notifications(Duration::from_millis(500));
        (notes.len() >= 192).then_some(notes.len())
    });
    assert_eq!(
        alive, 192,
        "16 renderers x 6 messages x 2 sets, none dropped"
    );
    let all = home.search("ssdp:all", 3, Duration::from_millis(3300));
    assert_eq!(all.len(), 96, "6 responses from each of 16 renderers");
    let latest = all.iter().map(|(_, after)| *after).max().unwrap();
    assert!(latest <= Duration::from_secs(3), "inside MX 3: {latest:?}");
    let devices = wait("sixteen renderers", || {
        let d = home.devices();
        (d.len() == 16).then_some(d)
    });
    let events = Events::start();
    let mut sids = Vec::new();
    for device in &devices {
        for service in Service::AV {
            sids.push(home.subscribe(device, service, &events.callback(service.path())));
        }
    }
    wait("48 initial events", || {
        sids.iter()
            .all(|sid| events.of(sid).first().is_some_and(|n| n.seq == 0))
            .then_some(())
    });
    assert_eq!(sids.iter().collect::<BTreeSet<_>>().len(), 48, "48 SIDs");
    println!(
        "control-point: scale: 16 renderers: {alive} of 192 start-up alive messages received; \
         ssdp:all MX 3 drew {} of 96 responses, the last after {} ms; 48 of 48 subscriptions got \
         their initial event",
        all.len(),
        latest.as_millis()
    );
}
