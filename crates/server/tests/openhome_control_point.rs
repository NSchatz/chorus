//! A scripted OpenHome control point against the real server (goal 17; P6,
//! Option B; `docs/upnp.md`, "OpenHome").
//!
//! Everything here runs the shipped `chorus-server` binary and speaks to it
//! over real sockets on loopback, as `upnp_control_point.rs` does for the
//! UPnP AV half, with the same tools (`tests/common/upnp_cp.rs`). The server
//! runs with `--upnp` and its default, `--upnp-openhome on`; the last test
//! switches it off.
//!
//! The plan is the digest's (the OpenHome research note of goal 17, section
//! 5), by test:
//!
//! - `the_description_lists_the_five_services_and_a_lower_version_is_served`:
//!   the description and the five SCPDs, searches by version (a search for
//!   `Product:1` is answered as `Product:1`, for `Product:3` not at all,
//!   eleven rows for `ssdp:all`), a SOAP request in `Product:1` answered in
//!   `Product:1`, and the initial event of each service (SEQ 0, every
//!   evented variable, one property each, no LastChange, booleans as 0/1).
//! - `a_three_track_playlist_plays_through_gapless_with_no_control_point_action`:
//!   Insert, IdArray and its moderated event, IdArrayChanged, Read, ReadList,
//!   then Play and nothing more from the script: the room's captured audio is
//!   the three files joined sample for sample, both joins inside a chunk,
//!   while `Id`, Info and Time event each track and `TransportState` never
//!   leaves `Playing` until the list ends (`Stopped`, the first track cued).
//! - `next_previous_seek_and_delete_move_the_playlist`: Next, Previous,
//!   SeekId, SeekIndex, the second-seeks, Pause, DeleteId of a queued and of
//!   the playing track, Repeat, DeleteAll, their faults, and the Time event
//!   once a second while playing and not at all while paused.
//! - `sources_volume_and_the_avtransport_takeover`: `SourceXml` with a
//!   line-in as `Analog` and an optical input as `Digital`; a source switch
//!   that takes the room off the Playlist and onto the line-in (the control
//!   state's source, the events, and the room's audio going quiet); Volume
//!   clamped by the room limit with ohPipeline's clamp-then-refuse rule and
//!   `VolumeLimit` evented when the limit changes; AVTransport taking the
//!   player from the Playlist and the Playlist taking it back; a source
//!   change made outside OpenHome; standby.
//! - `the_switch_off_removes_the_services`.
//!
//! **Not covered here, and said in `docs/upnp.md`:** no control point
//! application was run (the control point is this script); the line-in in
//! the source test offers its inputs and is selected, but streams no audio
//! (line-in audio has its own tests, `alarms_sleep_autoplay.rs`); the
//! `NetAux` Spotify source is unit-tested only, because no source spelled
//! `soloist:` exists in the control catalog on this branch; group targets
//! use the same code as the room target and are not walked again here.

mod common;
#[path = "common/upnp_cp.rs"]
mod cp;

use cp::*;

use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use chorus_protocol::v2::{roles, Message as V2Message, SourceKind, SourceOffer};
use chorus_upnp::client::{self, SoapReply};
use chorus_upnp::description;
use chorus_upnp::openhome::playlist::decode_ids;
use chorus_upnp::xml::escape_text;
use chorus_upnp::Service;

use common::Player as Listener;

const PRODUCT: Service = Service::Product;
const VOLUME: Service = Service::Volume;
const INFO: Service = Service::Info;
const TIME: Service = Service::Time;
const PLAYLIST: Service = Service::Playlist;

/// A variable of a plain (not LastChange) event.
fn prop(n: &Notified, name: &str) -> Option<String> {
    n.vars
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.clone())
}

/// Wait until an event of `sid` from index `from` on carries `name` =
/// `value` as a plain property; the index after it.
fn until(events: &Events, sid: &str, from: usize, name: &str, value: &str) -> usize {
    wait(&format!("an event with {name}={value}"), || {
        events
            .of(sid)
            .iter()
            .enumerate()
            .skip(from)
            .find(|(_, n)| prop(n, name).as_deref() == Some(value))
            .map(|(i, _)| i + 1)
    })
}

/// Every value `name` was evented with on `sid`, in order.
fn evented(events: &Events, sid: &str, name: &str) -> Vec<String> {
    events
        .of(sid)
        .iter()
        .filter_map(|n| prop(n, name))
        .collect()
}

fn value(reply: &SoapReply, name: &str) -> String {
    reply
        .value(name)
        .unwrap_or_else(|| panic!("no {name} in {reply:?}"))
        .to_string()
}

fn get_value(home: &Home, device: &Device, service: Service, action: &str) -> String {
    value(&home.ok(device, service, action, &[]), "Value")
}

fn insert(home: &Home, device: &Device, after: u32, uri: &str, metadata: &str) -> u32 {
    let reply = home.ok(
        device,
        PLAYLIST,
        "Insert",
        &[
            ("AfterId", &after.to_string()),
            ("Uri", uri),
            ("Metadata", metadata),
        ],
    );
    value(&reply, "NewId").parse().expect("an id")
}

fn playlist(home: &Home, device: &Device, action: &str, argument: Option<&str>) {
    match argument {
        Some(v) => home.ok(device, PLAYLIST, action, &[("Value", v)]),
        None => home.ok(device, PLAYLIST, action, &[]),
    };
}

/// The renderer called `name`, once it has appeared.
fn renderer(home: &Home, name: &str) -> Device {
    wait("the renderer appears", || {
        home.devices().into_iter().find(|d| d.name == name)
    })
}

/// A device's rows among search responses.
fn rows_of(found: &[(client::SsdpMessage, Duration)], udn: &str) -> Vec<String> {
    found
        .iter()
        .filter(|(m, _)| m.usn.contains(udn))
        .map(|(m, _)| m.target.clone())
        .collect()
}

#[test]
fn the_description_lists_the_five_services_and_a_lower_version_is_served() {
    let mut home = Home::start_openhome(&["kitchen"], &[]);
    let kitchen = renderer(&home, "kitchen");
    let path = kitchen
        .location
        .strip_prefix(&format!("http://{}", home.http))
        .unwrap()
        .to_string();
    let d = client::parse_description(&get(&home.http, &path).body).unwrap();
    // The same MediaRenderer:1 device, the five services behind the three.
    assert_eq!(d.device_type, DEVICE);
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
    for service in Service::OPENHOME {
        let urls = d.service(service).unwrap();
        assert_eq!(urls.service_id, service.service_id());
        let answer = get(&home.http, &urls.scpd_url);
        assert_eq!(answer.status, 200, "{}", urls.scpd_url);
        let scpd = client::parse_scpd(&answer.body).unwrap();
        let table = description::table(service);
        let actions: Vec<&str> = scpd.actions.iter().map(|a| a.name.as_str()).collect();
        let pinned: Vec<&str> = table.actions.iter().map(|a| a.name).collect();
        assert_eq!(actions, pinned, "{service:?}");
        assert!(scpd.variable("LastChange").is_none());
    }
    // None of what goal 17 leaves out (K64).
    let text = get(&home.http, &path).body;
    for absent in [
        "Radio",
        "Credentials",
        "Pins",
        "OAuth",
        "service:Transport",
        "Sender",
    ] {
        assert!(!text.contains(absent), "{absent}");
    }

    // Searches: a lower version is answered in that version; a higher one
    // and a service chorus has not are not answered.
    let listen = Duration::from_millis(1100);
    for (st, answers) in [
        ("urn:av-openhome-org:service:Product:1", 1),
        ("urn:av-openhome-org:service:Product:2", 1),
        ("urn:av-openhome-org:service:Product:3", 0),
        ("urn:av-openhome-org:service:Radio:1", 0),
    ] {
        let found = home.search(st, 1, listen);
        assert_eq!(rows_of(&found, &kitchen.udn).len(), answers, "{st}");
        for (m, _) in &found {
            assert_eq!(m.target, st, "the searched version is echoed");
            assert!(m.usn.ends_with(st), "{}", m.usn);
        }
    }
    let all = rows_of(&home.search("ssdp:all", 1, listen), &kitchen.udn);
    assert_eq!(all.len(), 11, "{all:?}");
    for service in Service::ALL {
        assert!(
            all.iter().any(|t| t == service.service_type()),
            "{service:?}"
        );
    }

    // SOAP: a request naming Product:1 is answered in Product:1.
    let v1 = "urn:av-openhome-org:service:Product:1";
    let call = client::soap_request_raw(v1, "Attributes", &[]);
    let control = kitchen.path(PRODUCT, "control");
    let answer = post(&home.http, &control, &call.soapaction, &call.body);
    assert_eq!(answer.status, 200);
    assert!(
        answer
            .body
            .contains(&format!("<u:AttributesResponse xmlns:u=\"{v1}\">")),
        "{}",
        answer.body
    );
    assert_eq!(
        client::parse_soap_reply(&answer.body)
            .unwrap()
            .value("Value"),
        Some("Info Time Volume")
    );
    let call = client::soap_request_raw("urn:av-openhome-org:service:Product:3", "Attributes", &[]);
    let answer = post(&home.http, &control, &call.soapaction, &call.body);
    assert_eq!(
        client::parse_soap_reply(&answer.body).unwrap().fault_code(),
        Some(401)
    );

    // What the product says it is.
    let product = home.ok(&kitchen, PRODUCT, "Product", &[]);
    assert_eq!(value(&product, "Room"), "kitchen");
    assert_eq!(value(&product, "Name"), "chorus");
    assert_eq!(
        value(&home.ok(&kitchen, PRODUCT, "Manufacturer", &[]), "Name"),
        "chorus"
    );
    assert_eq!(
        value(&home.ok(&kitchen, PRODUCT, "Model", &[]), "Name"),
        "chorus room"
    );
    assert_eq!(get_value(&home, &kitchen, PRODUCT, "Standby"), "0");
    assert_eq!(get_value(&home, &kitchen, PRODUCT, "SourceCount"), "2");

    // The initial event of each service: SEQ 0, one property per evented
    // variable of the SCPD, in its order, no LastChange, booleans 0 or 1.
    let events = Events::start();
    for service in Service::OPENHOME {
        let sid = home.subscribe(&kitchen, service, &events.callback(service.path()));
        let initial = wait("the initial event", || events.of(&sid).first().cloned());
        assert_eq!(initial.seq, 0);
        let names: Vec<&str> = initial.vars.iter().map(|(n, _)| n.as_str()).collect();
        let table: Vec<&str> = description::table(service)
            .variables
            .iter()
            .filter(|v| v.evented)
            .map(|v| v.name)
            .collect();
        assert_eq!(names, table, "{service:?}");
        for var in description::table(service).variables {
            if var.evented && var.data_type == "boolean" {
                let v = prop(&initial, var.name).unwrap();
                assert!(v == "0" || v == "1", "{}: {v}", var.name);
            }
        }
    }
    assert!(failures(&mut home).is_empty());
}

/// The lines of the server's log that say something went wrong.
fn failures(home: &mut Home) -> Vec<String> {
    home.server.drain();
    home.server
        .seen
        .iter()
        .filter(|l| l.contains("panicked") || l.contains("upnp media failed"))
        .cloned()
        .collect()
}

#[test]
fn a_three_track_playlist_plays_through_gapless_with_no_control_point_action() {
    let mut home = Home::start_openhome(&["kitchen"], &[]);
    let kitchen = renderer(&home, "kitchen");
    let heard = home.listen_in("kitchen");
    let events = Events::start();
    let media = MediaServer::start();

    // 2.6 s of one signal, cut twice at frames that are no multiple of a
    // chunk: three files.
    let (total, cut1, cut2) = (124_850usize, 52_807usize, 96_011usize);
    let signal = ramp(0, total);
    let uris = [
        media.serve("/a.wav", wav(48_000, &signal[..cut1]), "audio/wav"),
        media.serve("/b.wav", wav(48_000, &signal[cut1..cut2]), "audio/wav"),
        media.serve("/c.wav", wav(48_000, &signal[cut2..]), "audio/wav"),
    ];
    let didls: Vec<String> = ["Low Tide", "Slack Water", "Flood"]
        .iter()
        .zip(&uris)
        .map(|(title, uri)| {
            didl(
                title,
                "The Harbour Lights",
                "Salt",
                uri,
                "audio/wav",
                "0:00:01",
            )
        })
        .collect();

    let list_sid = home.subscribe(&kitchen, PLAYLIST, &events.callback("ohl"));
    let info_sid = home.subscribe(&kitchen, INFO, &events.callback("ohi"));
    let time_sid = home.subscribe(&kitchen, TIME, &events.callback("oht"));
    wait("the initial events", || {
        [&list_sid, &info_sid, &time_sid]
            .iter()
            .all(|sid| !events.of(sid).is_empty())
            .then_some(())
    });

    // The list starts empty.
    let empty = home.ok(&kitchen, PLAYLIST, "IdArray", &[]);
    let token = value(&empty, "Token");
    assert_eq!(value(&empty, "Array"), "");
    assert_eq!(get_value(&home, &kitchen, PLAYLIST, "Id"), "0");
    assert_eq!(
        get_value(&home, &kitchen, PLAYLIST, "TransportState"),
        "Stopped"
    );
    assert_eq!(get_value(&home, &kitchen, PLAYLIST, "TracksMax"), "1000");

    // Three inserts, each after the one before.
    let a = insert(&home, &kitchen, 0, &uris[0], &didls[0]);
    let b = insert(&home, &kitchen, a, &uris[1], &didls[1]);
    let c = insert(&home, &kitchen, b, &uris[2], &didls[2]);
    assert!(a != 0 && a != b && b != c && a != c);
    home.fault(
        &kitchen,
        PLAYLIST,
        "Insert",
        &[("AfterId", "999"), ("Uri", &uris[0]), ("Metadata", "")],
        800,
    );
    home.fault(
        &kitchen,
        PLAYLIST,
        "Insert",
        &[
            ("AfterId", "0"),
            ("Uri", "file:///etc/passwd"),
            ("Metadata", ""),
        ],
        600,
    );
    // One IdArray event for the burst (held 300 ms), with the three ids as
    // big-endian 32-bit numbers in base64; the first insert cued its track.
    let after_inserts = wait("the IdArray event", || {
        let arrays = evented(&events, &list_sid, "IdArray");
        (arrays.len() >= 2).then_some(arrays)
    });
    assert_eq!(after_inserts[0], "", "the initial event: an empty list");
    assert_eq!(decode_ids(&after_inserts[1]), Some(vec![a, b, c]));
    assert_eq!(after_inserts.len(), 2, "the burst is one event");
    until(&events, &list_sid, 0, "Id", &a.to_string());
    let now = home.ok(&kitchen, PLAYLIST, "IdArray", &[]);
    assert_eq!(decode_ids(&value(&now, "Array")), Some(vec![a, b, c]));
    let changed = |token: &str| {
        value(
            &home.ok(&kitchen, PLAYLIST, "IdArrayChanged", &[("Token", token)]),
            "Value",
        )
    };
    assert_eq!(changed(&token), "1");
    assert_eq!(changed(&value(&now, "Token")), "0");
    // Read and ReadList give the URI and the metadata back verbatim.
    let read = home.ok(&kitchen, PLAYLIST, "Read", &[("Id", &b.to_string())]);
    assert_eq!(value(&read, "Uri"), uris[1]);
    assert_eq!(value(&read, "Metadata"), didls[1]);
    home.fault(&kitchen, PLAYLIST, "Read", &[("Id", "999")], 800);
    let list = home.ok(
        &kitchen,
        PLAYLIST,
        "ReadList",
        &[("IdList", &format!("{a} {b} 999 {c}"))],
    );
    let entry = |id: u32, n: usize| {
        format!(
            "<Entry><Id>{id}</Id><Uri>{}</Uri><Metadata>{}</Metadata></Entry>",
            escape_text(&uris[n]),
            escape_text(&didls[n])
        )
    };
    assert_eq!(
        value(&list, "TrackList"),
        format!(
            "<TrackList>{}{}{}</TrackList>",
            entry(a, 0),
            entry(b, 1),
            entry(c, 2)
        )
    );
    assert_eq!(
        get_value(&home, &kitchen, PLAYLIST, "TransportState"),
        "Stopped"
    );

    // Play. From here the control point sends nothing: the renderer walks
    // its list alone.
    playlist(&home, &kitchen, "Play", None);
    let playing = until(&events, &list_sid, 0, "TransportState", "Playing");
    let stopped = until(&events, &list_sid, playing, "TransportState", "Stopped");

    // What the Playlist evented: every track's id as it became audible, the
    // state unbroken between the first sound and the end of the list, and
    // at the end the first track cued again.
    let ids: Vec<String> = evented(&events, &list_sid, "Id");
    let expected: Vec<String> = [0, a, b, c, a].iter().map(u32::to_string).collect();
    assert_eq!(ids, expected);
    let all = events.of(&list_sid);
    let between: Vec<String> = all[playing..stopped - 1]
        .iter()
        .filter_map(|n| prop(n, "TransportState"))
        .collect();
    assert!(
        between.is_empty(),
        "no state change between the tracks: {between:?}"
    );
    for pair in all.windows(2) {
        assert_eq!(pair[1].seq, pair[0].seq + 1);
    }
    assert_eq!(get_value(&home, &kitchen, PLAYLIST, "Id"), a.to_string());

    // Info: each track's URI and metadata as it started, and the decoder's
    // details; Time: a track count of three.
    wait("Info's three tracks", || {
        (evented(&events, &info_sid, "TrackCount")
            .last()
            .map(String::as_str)
            == Some("3"))
        .then_some(())
    });
    let mut info_uris = evented(&events, &info_sid, "Uri");
    assert_eq!(info_uris.remove(0), "", "the initial event");
    assert_eq!(info_uris, uris);
    let metadata = evented(&events, &info_sid, "Metadata");
    assert_eq!(metadata[1..], didls[..]);
    assert!(evented(&events, &info_sid, "SampleRate").contains(&"48000".to_string()));
    assert!(evented(&events, &info_sid, "BitDepth").contains(&"16".to_string()));
    assert!(evented(&events, &info_sid, "CodecName").contains(&"pcm".to_string()));
    assert!(evented(&events, &info_sid, "Lossless").contains(&"1".to_string()));
    let track = home.ok(&kitchen, INFO, "Track", &[]);
    assert_eq!(value(&track, "Uri"), uris[2]);
    assert_eq!(value(&track, "Metadata"), didls[2]);
    assert_eq!(
        value(&home.ok(&kitchen, INFO, "Counters", &[]), "TrackCount"),
        "3"
    );
    wait("Time's three tracks", || {
        (evented(&events, &time_sid, "TrackCount")
            .last()
            .map(String::as_str)
            == Some("3"))
        .then_some(())
    });
    assert!(evented(&events, &time_sid, "Duration").contains(&"1".to_string()));

    // What the room played: the uncut signal, once, sample for sample.
    let (frames, at) = wait("the whole signal in the room", || {
        let frames = heard.placed_since(0);
        let start = frames.iter().position(|p| p.3 == signal[0])?;
        (frames.len() >= start + total + 4_800).then_some((frames, start))
    });
    let mut worst = 0i32;
    for (n, want) in signal.iter().enumerate() {
        let got = frames[at + n].3;
        for ch in 0..2 {
            worst = worst.max((i32::from(got[ch]) - i32::from(want[ch])).abs());
        }
    }
    assert_eq!(worst, 0, "the three files join sample for sample");
    let received = frames.iter().filter(|p| p.3 != SILENCE).count();
    assert_eq!(received, total, "nothing else was heard, and no gap");
    let sequences: Vec<u32> = frames[at..at + total].iter().map(|p| p.0).collect();
    assert!(
        sequences
            .windows(2)
            .all(|w| w[1] == w[0] || w[1] == w[0].wrapping_add(1)),
        "no chunk is missing inside the three tracks"
    );
    for cut in [cut1, cut2] {
        assert_eq!(
            frames[at + cut - 1].0,
            frames[at + cut].0,
            "the join at {cut} is inside a chunk"
        );
        assert_ne!(frames[at + cut].2, 0, "the join at {cut} is inside a chunk");
    }
    assert!(
        frames[at + total..].iter().all(|p| p.3 == SILENCE),
        "silence after the end"
    );
    // After the end the room plays nothing and shows nothing.
    wait("the room is released", || {
        (!home.room("kitchen").contains("now_playing")).then_some(())
    });
    let failed = failures(&mut home);
    assert!(failed.is_empty(), "{failed:?}");
    println!(
        "openhome playlist gapless: room=kitchen tracks=3 frames={total} received={received} \
         worst_sample_error={worst} joins_inside_chunks=2 state_changes_between_tracks=0"
    );
}

#[test]
fn next_previous_seek_and_delete_move_the_playlist() {
    let mut home = Home::start_openhome(&["kitchen"], &[]);
    let kitchen = renderer(&home, "kitchen");
    let events = Events::start();
    let media = MediaServer::start();
    // Three tracks of 20 s: long enough that nothing ends by itself.
    let uris: Vec<String> = (0..3)
        .map(|n| {
            media.serve(
                &format!("/{n}.wav"),
                wav(48_000, &ramp(n * 1_000_000, 20 * 48_000)),
                "audio/wav",
            )
        })
        .collect();
    let list_sid = home.subscribe(&kitchen, PLAYLIST, &events.callback("ohl"));
    let time_sid = home.subscribe(&kitchen, TIME, &events.callback("oht"));
    let a = insert(&home, &kitchen, 0, &uris[0], "");
    let b = insert(&home, &kitchen, a, &uris[1], "");
    let c = insert(&home, &kitchen, b, &uris[2], "");
    let id = |n: u32| n.to_string();
    let now_playing = |from: usize, track: u32| {
        let at = until(&events, &list_sid, from, "Id", &id(track));
        wait("the track plays", || {
            (get_value(&home, &kitchen, PLAYLIST, "TransportState") == "Playing").then_some(())
        });
        assert_eq!(get_value(&home, &kitchen, PLAYLIST, "Id"), id(track));
        at
    };

    playlist(&home, &kitchen, "Play", None);
    let mark = until(&events, &list_sid, 0, "TransportState", "Playing");

    // Time: one event a second while playing, Seconds going up by one.
    let seconds = wait("three Time events", || {
        let seen: Vec<(u32, Instant)> = events
            .of(&time_sid)
            .iter()
            .skip(1)
            .filter_map(|n| Some((prop(n, "Seconds")?.parse().ok()?, n.at)))
            .filter(|(s, _)| *s > 0)
            .collect();
        (seen.len() >= 3).then_some(seen)
    });
    for pair in seconds.windows(2) {
        assert_eq!(pair[1].0, pair[0].0 + 1, "{seconds:?}");
        let gap = pair[1].1.duration_since(pair[0].1);
        assert!(
            gap > Duration::from_millis(500) && gap < Duration::from_millis(1500),
            "about a second apart: {gap:?}"
        );
    }
    let time = home.ok(&kitchen, TIME, "Time", &[]);
    assert_eq!(value(&time, "Duration"), "20");
    assert_eq!(value(&time, "TrackCount"), "1");

    // Next, Previous, SeekId, SeekIndex.
    playlist(&home, &kitchen, "Next", None);
    let mark = now_playing(mark, b);
    playlist(&home, &kitchen, "Next", None);
    let mark = now_playing(mark, c);
    playlist(&home, &kitchen, "Previous", None);
    let mark = now_playing(mark, b);
    playlist(&home, &kitchen, "SeekId", Some(&id(a)));
    let mark = now_playing(mark, a);
    home.fault(&kitchen, PLAYLIST, "SeekId", &[("Value", "999")], 800);
    playlist(&home, &kitchen, "SeekIndex", Some("2"));
    let mark = now_playing(mark, c);
    home.fault(&kitchen, PLAYLIST, "SeekIndex", &[("Value", "3")], 802);
    assert_eq!(get_value(&home, &kitchen, PLAYLIST, "Id"), id(c));

    // Seeking by seconds inside the track.
    playlist(&home, &kitchen, "SeekSecondAbsolute", Some("5"));
    let position = || -> u32 {
        value(&home.ok(&kitchen, TIME, "Time", &[]), "Seconds")
            .parse()
            .unwrap()
    };
    wait("the position past 5 s", || {
        let s = position();
        (5..=8).contains(&s).then_some(())
    });
    home.fault(
        &kitchen,
        PLAYLIST,
        "SeekSecondAbsolute",
        &[("Value", "21")],
        803,
    );
    wait("playing again after the seek", || {
        (get_value(&home, &kitchen, PLAYLIST, "TransportState") == "Playing").then_some(())
    });
    playlist(&home, &kitchen, "SeekSecondRelative", Some("-100"));
    wait("the position back at the start", || {
        (position() <= 2).then_some(())
    });

    // Pause holds the state and the Time events; Play carries on.
    wait("playing again after the seek", || {
        (get_value(&home, &kitchen, PLAYLIST, "TransportState") == "Playing").then_some(())
    });
    playlist(&home, &kitchen, "Pause", None);
    let mark = until(&events, &list_sid, mark, "TransportState", "Paused");
    thread::sleep(Duration::from_millis(300));
    let quiet = events.of(&time_sid).len();
    thread::sleep(Duration::from_millis(1300));
    assert_eq!(
        events.of(&time_sid).len(),
        quiet,
        "no Time event while paused"
    );
    playlist(&home, &kitchen, "Play", None);
    let mark = until(&events, &list_sid, mark, "TransportState", "Playing");

    // Deleting a track that does not play changes the list and nothing
    // that sounds.
    playlist(&home, &kitchen, "DeleteId", Some(&id(b)));
    wait("the IdArray without b", || {
        evented(&events, &list_sid, "IdArray")
            .last()
            .and_then(|v| decode_ids(v))
            .filter(|ids| *ids == vec![a, c])
    });
    assert_eq!(
        get_value(&home, &kitchen, PLAYLIST, "TransportState"),
        "Playing"
    );
    assert_eq!(get_value(&home, &kitchen, PLAYLIST, "Id"), id(c));
    home.fault(&kitchen, PLAYLIST, "DeleteId", &[("Value", "999")], 800);

    // Repeat: Next at the last track wraps and plays.
    playlist(&home, &kitchen, "SetRepeat", Some("1"));
    until(&events, &list_sid, mark, "Repeat", "1");
    playlist(&home, &kitchen, "Next", None);
    let mark = now_playing(mark, a);
    playlist(&home, &kitchen, "SetRepeat", Some("0"));
    // Without it, Next at the last track stops with the first cued.
    playlist(&home, &kitchen, "SeekId", Some(&id(c)));
    let mark = now_playing(mark, c);
    playlist(&home, &kitchen, "Next", None);
    let mark = until(&events, &list_sid, mark, "TransportState", "Stopped");
    assert_eq!(get_value(&home, &kitchen, PLAYLIST, "Id"), id(a));

    // Deleting the track that plays moves on; the last one leaves an empty,
    // stopped list.
    playlist(&home, &kitchen, "Play", None);
    let mark = until(&events, &list_sid, mark, "TransportState", "Playing");
    playlist(&home, &kitchen, "DeleteId", Some(&id(a)));
    let mark = now_playing(mark, c);
    home.fault(&kitchen, PLAYLIST, "SetShuffle", &[("Value", "1")], 804);
    playlist(&home, &kitchen, "DeleteAll", None);
    until(&events, &list_sid, mark, "TransportState", "Stopped");
    until(&events, &list_sid, mark, "Id", "0");
    wait("the empty IdArray", || {
        (evented(&events, &list_sid, "IdArray")
            .last()
            .map(String::as_str)
            == Some(""))
        .then_some(())
    });
    assert_eq!(
        value(&home.ok(&kitchen, PLAYLIST, "IdArray", &[]), "Array"),
        ""
    );
    let failed = failures(&mut home);
    assert!(failed.is_empty(), "{failed:?}");
}

#[test]
fn sources_volume_and_the_avtransport_takeover() {
    let mut home = Home::start_openhome(&["kitchen"], &[]);
    home.server
        .applied(r#"{"v":2,"t":"limit","zone":"kitchen","limit":0.600}"#);
    // The room's amplifier: its speaker, and an endpoint that offers an
    // analogue line-in and an optical input from a TV.
    let amp = common::fresh_id("amp");
    home.server.applied(&format!(
        r#"{{"v":1,"t":"attach","zone":"kitchen","endpoint":"{amp}"}}"#
    ));
    home.server
        .applied(r#"{"v":2,"t":"take","target":"kitchen","source":"none"}"#);
    let mut listener = Listener::connect(&home.server.audio, &amp, roles::SOURCE);
    for (source_id, kind, name) in [
        (1, SourceKind::LineIn, "line-1"),
        (2, SourceKind::Optical, "tv"),
    ] {
        listener
            .session
            .writer
            .send(&V2Message::SourceOffer(SourceOffer {
                source_id,
                kind,
                // The control state lists an input while its signal is
                // present.
                signal: true,
                name: name.to_string(),
                reason: 0,
            }))
            .expect("the offer is sent");
    }
    let heard = Heard::from(listener, Arc::clone(&home.keep));
    let kitchen = renderer(&home, "kitchen");
    let events = Events::start();
    let media = MediaServer::start();
    let product_sid = home.subscribe(&kitchen, PRODUCT, &events.callback("ohp"));
    let list_sid = home.subscribe(&kitchen, PLAYLIST, &events.callback("ohl"));
    let volume_sid = home.subscribe(&kitchen, VOLUME, &events.callback("ohv"));
    let avt_sid = home.subscribe(&kitchen, AVT, &events.callback("avt"));
    let rcs_sid = home.subscribe(&kitchen, RCS, &events.callback("rcs"));

    // Product: the source list.
    wait("the two inputs in the control state", || {
        let state = home.server.state();
        (state.contains("/line-1") && state.contains("/tv")).then_some(())
    });
    wait("the two inputs among the sources", || {
        (get_value(&home, &kitchen, PRODUCT, "SourceCount") == "4").then_some(())
    });
    let source = |name: &str, kind: &str, visible: bool, system: &str| {
        format!(
            "<Source><Name>{name}</Name><Type>{kind}</Type><Visible>{visible}</Visible>\
             <SystemName>{system}</SystemName></Source>"
        )
    };
    let line = format!("{amp}/line-1");
    let tv = format!("{amp}/tv");
    let source_xml = format!(
        "<SourceList>{}{}{}{}</SourceList>",
        source("Playlist", "Playlist", true, "Playlist"),
        source("UPnP AV", "UpnpAv", false, "UpnpAv"),
        source("line-1", "Analog", true, &line),
        source("tv", "Digital", true, &tv),
    );
    assert_eq!(get_value(&home, &kitchen, PRODUCT, "SourceXml"), source_xml);
    until(&events, &product_sid, 0, "SourceXml", &source_xml);
    let third = home.ok(&kitchen, PRODUCT, "Source", &[("Index", "2")]);
    assert_eq!(value(&third, "SystemName"), line);
    assert_eq!(value(&third, "Type"), "Analog");
    assert_eq!(value(&third, "Name"), "line-1");
    assert_eq!(value(&third, "Visible"), "1");
    home.fault(&kitchen, PRODUCT, "Source", &[("Index", "4")], 801);
    assert_eq!(get_value(&home, &kitchen, PRODUCT, "SourceIndex"), "0");
    assert!(!source_xml.contains("NetAux"), "nothing plays Spotify here");

    // Volume: the room limit is VolumeLimit; above it is clamped once and
    // refused when already there (K81).
    let characteristics = home.ok(&kitchen, VOLUME, "Characteristics", &[]);
    assert_eq!(value(&characteristics, "VolumeMax"), "100");
    assert_eq!(get_value(&home, &kitchen, VOLUME, "VolumeLimit"), "60");
    home.ok(&kitchen, VOLUME, "SetVolume", &[("Value", "40")]);
    let v = until(&events, &volume_sid, 0, "Volume", "40");
    assert!(home.room("kitchen").contains(r#""volume":0.400"#));
    events.until(&rcs_sid, 0, "Volume", "40");
    home.ok(&kitchen, VOLUME, "SetVolume", &[("Value", "90")]);
    let v = until(&events, &volume_sid, v, "Volume", "60");
    assert!(home.room("kitchen").contains(r#""volume":0.600"#));
    assert_eq!(get_value(&home, &kitchen, VOLUME, "Volume"), "60");
    home.fault(&kitchen, VOLUME, "SetVolume", &[("Value", "90")], 811);
    home.fault(&kitchen, VOLUME, "SetVolume", &[("Value", "101")], 811);
    home.ok(&kitchen, VOLUME, "VolumeInc", &[]);
    assert_eq!(get_value(&home, &kitchen, VOLUME, "Volume"), "60");
    home.ok(&kitchen, VOLUME, "SetMute", &[("Value", "1")]);
    let v = until(&events, &volume_sid, v, "Mute", "1");
    home.ok(&kitchen, VOLUME, "SetMute", &[("Value", "0")]);
    home.fault(&kitchen, VOLUME, "SetBalance", &[("Value", "1")], 801);
    // The owner lowers the limit: VolumeLimit is evented, and the volume it
    // pulled down with it.
    home.server
        .applied(r#"{"v":2,"t":"limit","zone":"kitchen","limit":0.500}"#);
    until(&events, &volume_sid, v, "VolumeLimit", "50");
    until(&events, &volume_sid, v, "Volume", "50");

    // The Playlist plays a long track in the room.
    let track = media.serve("/long.wav", wav(48_000, &ramp(0, 20 * 48_000)), "audio/wav");
    let other = media.serve(
        "/other.wav",
        wav(48_000, &ramp(2_000_000, 20 * 48_000)),
        "audio/wav",
    );
    let a = insert(&home, &kitchen, 0, &track, "");
    playlist(&home, &kitchen, "Play", None);
    let l = until(&events, &list_sid, 0, "TransportState", "Playing");
    wait("the room hears the playlist", || {
        (heard.loud_since(0) > 0).then_some(())
    });
    assert!(home.server.state().contains(r#""source":"player:p0""#));

    // A source switch: the line-in. The room plays it (the control state),
    // the Playlist stops, and its audio is gone from the room.
    home.ok(&kitchen, PRODUCT, "SetSourceIndex", &[("Value", "2")]);
    let p = until(&events, &product_sid, 0, "SourceIndex", "2");
    assert_eq!(get_value(&home, &kitchen, PRODUCT, "SourceIndex"), "2");
    assert!(home
        .server
        .state()
        .contains(&format!(r#""source":"line-in:{line}""#)));
    let l = until(&events, &list_sid, l, "TransportState", "Stopped");
    wait("the playlist's audio leaves the room", || {
        let mark = heard.chunks();
        thread::sleep(Duration::from_millis(150));
        (heard.loud_since(mark) == 0).then_some(())
    });
    // By system name, and by name; an unknown one is 801.
    home.ok(
        &kitchen,
        PRODUCT,
        "SetSourceBySystemName",
        &[("Value", &tv)],
    );
    let p = until(&events, &product_sid, p, "SourceIndex", "3");
    assert!(home
        .server
        .state()
        .contains(&format!(r#""source":"line-in:{tv}""#)));
    home.ok(
        &kitchen,
        PRODUCT,
        "SetSourceIndexByName",
        &[("Value", "line-1")],
    );
    let p = until(&events, &product_sid, p, "SourceIndex", "2");
    home.fault(
        &kitchen,
        PRODUCT,
        "SetSourceIndexByName",
        &[("Value", "nope")],
        801,
    );
    home.fault(&kitchen, PRODUCT, "SetSourceIndex", &[("Value", "9")], 801);
    // Info says what plays: the line-in.
    assert_eq!(
        value(&home.ok(&kitchen, INFO, "Track", &[]), "Uri"),
        format!("line-in:{line}")
    );

    // Playlist Play takes the room back: the source is the Playlist again.
    playlist(&home, &kitchen, "Play", None);
    let p = until(&events, &product_sid, p, "SourceIndex", "0");
    let l = until(&events, &list_sid, l, "TransportState", "Playing");
    assert!(home.server.state().contains(r#""source":"player:p"#));

    // The AVTransport takeover: a URI and Play over AVTransport make UPnP AV
    // the source and stop the Playlist, whose list is kept.
    assert!(matches!(
        home.set_uri(&kitchen, &other, ""),
        SoapReply::Response { .. }
    ));
    home.play(&kitchen);
    let p = until(&events, &product_sid, p, "SourceIndex", "1");
    let l = until(&events, &list_sid, l, "TransportState", "Stopped");
    let av = events.until(&avt_sid, 0, "TransportState", "PLAYING");
    assert_eq!(
        decode_ids(&value(
            &home.ok(&kitchen, PLAYLIST, "IdArray", &[]),
            "Array"
        )),
        Some(vec![a])
    );
    assert_eq!(get_value(&home, &kitchen, PLAYLIST, "Id"), a.to_string());
    wait("Info shows the AVTransport URI", || {
        (value(&home.ok(&kitchen, INFO, "Track", &[]), "Uri") == other).then_some(())
    });
    // While UPnP AV is the source, the Playlist's transport actions do
    // nothing to what plays.
    playlist(&home, &kitchen, "Pause", None);
    playlist(&home, &kitchen, "Next", None);
    assert_eq!(home.transport(&kitchen).0, "PLAYING");
    // And back: Playlist Play stops AVTransport.
    playlist(&home, &kitchen, "Play", None);
    let p = until(&events, &product_sid, p, "SourceIndex", "0");
    events.until(&avt_sid, av, "TransportState", "STOPPED");
    let l = until(&events, &list_sid, l, "TransportState", "Playing");

    // A source change from outside OpenHome (the control plane): the
    // Product follows and the Playlist stops.
    home.server.applied(&format!(
        r#"{{"v":2,"t":"take","target":"kitchen","source":"line-in:{line}"}}"#
    ));
    let p = until(&events, &product_sid, p, "SourceIndex", "2");
    let l = until(&events, &list_sid, l, "TransportState", "Stopped");

    // Standby: a flag that stops what the renderer plays; Play clears it.
    playlist(&home, &kitchen, "Play", None);
    let l = until(&events, &list_sid, l, "TransportState", "Playing");
    home.ok(&kitchen, PRODUCT, "SetStandby", &[("Value", "1")]);
    let p = until(&events, &product_sid, p, "Standby", "1");
    let l = until(&events, &list_sid, l, "TransportState", "Stopped");
    playlist(&home, &kitchen, "Play", None);
    until(&events, &product_sid, p, "Standby", "0");
    until(&events, &list_sid, l, "TransportState", "Playing");
    let failed = failures(&mut home);
    assert!(failed.is_empty(), "{failed:?}");
}

#[test]
fn the_switch_off_removes_the_services() {
    // `Home::start` passes `--upnp-openhome off`.
    let mut home = Home::start(&["kitchen"]);
    home.server.drain();
    assert!(
        home.server
            .seen
            .iter()
            .any(|l| l.contains("upnp renderers listening on=") && l.contains("openhome=off")),
        "the start line says the services are off"
    );
    let kitchen = renderer(&home, "kitchen");
    let path = kitchen
        .location
        .strip_prefix(&format!("http://{}", home.http))
        .unwrap()
        .to_string();
    let text = get(&home.http, &path).body;
    let d = client::parse_description(&text).unwrap();
    assert_eq!(d.services.len(), 3);
    assert!(!text.contains("openhome"));
    for service in Service::OPENHOME {
        assert_eq!(
            get(&home.http, &kitchen.path(service, "scpd.xml")).status,
            404
        );
        let action = description::table(service).actions[0].name;
        let call = client::soap_request_raw(service.service_type(), action, &[]);
        assert_eq!(
            post(
                &home.http,
                &kitchen.path(service, "control"),
                &call.soapaction,
                &call.body
            )
            .status,
            404
        );
        let answer = exchange(
            &home.http,
            client::subscribe_request(
                &kitchen.path(service, "event"),
                &home.http,
                "http://127.0.0.1:9/cb",
                Some(1800),
            )
            .as_bytes(),
        );
        assert_eq!(answer.status, 404);
    }
    let listen = Duration::from_millis(1100);
    assert!(home
        .search("urn:av-openhome-org:service:Product:2", 1, listen)
        .is_empty());
    assert_eq!(
        rows_of(&home.search("ssdp:all", 1, listen), &kitchen.udn).len(),
        6
    );
}
