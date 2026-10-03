//! HLS from an in-test server: a VOD playlist of packed MP3 segments, a
//! master playlist, a live playlist with a sliding window that ends, the
//! policy on every segment, and each named refusal.

mod common;

use std::io::{ErrorKind, Read, SeekFrom};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chorus_fetch::{open, FetchError, Policy};
use common::{policy, read_all, refused, respond, status, unsupported, Server};

const PLAYLIST_TYPE: &str = "application/vnd.apple.mpegurl";

/// The ID3 tag RFC 8216 section 3.4 puts at the start of a packed audio
/// segment: one PRIV frame, owner
/// `com.apple.streaming.transportStreamTimestamp`, payload a 33-bit timestamp
/// as eight big-endian bytes.
fn timestamp_tag(pts: u64) -> Vec<u8> {
    let owner = b"com.apple.streaming.transportStreamTimestamp\0";
    let mut frame = Vec::new();
    frame.extend_from_slice(b"PRIV");
    let size = (owner.len() + 8) as u32;
    frame.extend_from_slice(&[0, 0, 0, size as u8]);
    frame.extend_from_slice(&[0, 0]);
    frame.extend_from_slice(owner);
    frame.extend_from_slice(&(pts & 0x1_ffff_ffff).to_be_bytes());
    let mut tag = b"ID3\x04\x00\x00".to_vec();
    tag.extend_from_slice(&[0, 0, 0, frame.len() as u8]);
    tag.extend_from_slice(&frame);
    tag
}

/// Bytes shaped like MPEG-1 layer 3 frames: each starts with the header
/// 0xFF 0xFB 0x90 0x00 (128 kbit/s, 44.1 kHz) and is 417 bytes long, the
/// payload numbered so segments differ.
fn mp3_like(frames: usize, seed: u8) -> Vec<u8> {
    let mut out = Vec::new();
    for f in 0..frames {
        out.extend_from_slice(&[0xff, 0xfb, 0x90, 0x00]);
        out.extend((0..413).map(|i| (i as u8).wrapping_add(seed).wrapping_add(f as u8)));
    }
    out
}

/// A packed MP3 segment and the audio inside it.
fn segment(index: u8) -> (Vec<u8>, Vec<u8>) {
    let audio = mp3_like(20, index);
    let mut wire = timestamp_tag(u64::from(index) * 90_000);
    wire.extend_from_slice(&audio);
    (wire, audio)
}

fn media_playlist(first: u64, count: u64, ended: bool) -> String {
    let mut text = format!(
        "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:1\n#EXT-X-MEDIA-SEQUENCE:{first}\n"
    );
    for n in first..first + count {
        text.push_str(&format!("#EXTINF:1.000,\nseg{n}.mp3\n"));
    }
    if ended {
        text.push_str("#EXT-X-ENDLIST\n");
    }
    text
}

#[test]
fn a_vod_playlist_plays_as_one_stream() {
    let server = Server::start(|request, out| match request.target.as_str() {
        "/hls/show/index.m3u8" => {
            respond(out, PLAYLIST_TYPE, media_playlist(0, 3, true).as_bytes())
        }
        "/hls/show/seg0.mp3" => respond(out, "audio/mpeg", &segment(0).0),
        "/hls/show/seg1.mp3" => respond(out, "audio/mpeg", &segment(1).0),
        "/hls/show/seg2.mp3" => respond(out, "audio/mpeg", &segment(2).0),
        _ => status(out, 404),
    });
    let mut stream = open(&server.url("/hls/show/index.m3u8"), &policy()).unwrap();
    let opened = stream.opened().clone();
    assert!(opened.hls);
    assert_eq!(opened.content_type.as_deref(), Some("audio/mpeg"));
    assert_eq!(opened.byte_len, None);
    assert!(!opened.seekable);
    assert_eq!(opened.final_url, server.url("/hls/show/index.m3u8"));
    assert_eq!(
        stream.seek(SeekFrom::Start(0)).unwrap_err().kind(),
        ErrorKind::Unsupported
    );

    let got = read_all(&mut stream).unwrap();
    let want: Vec<u8> = (0..3).flat_map(|i| segment(i).1).collect();
    assert_eq!(got.len(), want.len());
    assert!(
        got == want,
        "the concatenation is the audio alone, in order"
    );
    assert!(
        !got.windows(3).any(|w| w == b"ID3"),
        "no ID3 tag is left in the stream"
    );
    // The playlist once (it has ended: no reload), each segment once.
    assert_eq!(
        server.targets(),
        [
            "/hls/show/index.m3u8",
            "/hls/show/seg0.mp3",
            "/hls/show/seg1.mp3",
            "/hls/show/seg2.mp3"
        ]
    );
    // The segment requests carry no ICY header: a segment has no metadata blocks.
    assert_eq!(server.requests()[1].header("Icy-MetaData"), None);
}

#[test]
fn a_playlist_is_recognised_by_its_first_line_whatever_its_type() {
    let server = Server::start(|request, out| match request.target.as_str() {
        "/list" => respond(out, "text/plain", media_playlist(0, 1, true).as_bytes()),
        "/seg0.mp3" => {
            // Chunked, and with no ID3 tag: tolerated.
            let audio = segment(0).1;
            let _ = write!(out, "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n");
            for chunk in audio.chunks(1000) {
                let _ = write!(out, "{:x}\r\n", chunk.len());
                let _ = out.write_all(chunk);
                let _ = out.write_all(b"\r\n");
            }
            let _ = out.write_all(b"0\r\n\r\n");
        }
        _ => status(out, 404),
    });
    let mut stream = open(&server.url("/list"), &policy()).unwrap();
    assert!(stream.opened().hls);
    assert!(read_all(&mut stream).unwrap() == segment(0).1);
}

#[test]
fn a_master_playlist_leads_to_its_best_mp3_variant() {
    let server = Server::start(|request, out| match request.target.as_str() {
        "/radio/master.m3u8" => respond(
            out,
            "audio/mpegurl",
            b"#EXTM3U\n\
              #EXT-X-STREAM-INF:BANDWIDTH=64000,CODECS=\"mp4a.40.34\"\nlo/index.m3u8\n\
              #EXT-X-STREAM-INF:BANDWIDTH=256000,CODECS=\"mp4a.40.2\"\naac/index.m3u8\n\
              #EXT-X-STREAM-INF:BANDWIDTH=128000,CODECS=\"mp4a.40.34\"\nhi/index.m3u8\n",
        ),
        "/radio/hi/index.m3u8" => {
            respond(out, PLAYLIST_TYPE, media_playlist(7, 2, true).as_bytes())
        }
        "/radio/hi/seg7.mp3" => respond(out, "audio/mpeg", &segment(7).0),
        "/radio/hi/seg8.mp3" => respond(out, "audio/mpeg", &segment(8).0),
        _ => status(out, 404),
    });
    let mut stream = open(&server.url("/radio/master.m3u8"), &policy()).unwrap();
    assert_eq!(
        stream.opened().final_url,
        server.url("/radio/hi/index.m3u8")
    );
    let want: Vec<u8> = [7, 8].into_iter().flat_map(|i| segment(i).1).collect();
    assert!(read_all(&mut stream).unwrap() == want);
    assert_eq!(
        server.targets(),
        [
            "/radio/master.m3u8",
            "/radio/hi/index.m3u8",
            "/radio/hi/seg7.mp3",
            "/radio/hi/seg8.mp3"
        ]
    );
}

#[test]
fn a_live_playlist_slides_is_reloaded_on_the_rfcs_schedule_and_ends() {
    // The window is three one-second segments. It slides by one on every
    // second load (so every other reload finds the playlist unchanged), and
    // the load that reveals segment 5 also carries ENDLIST.
    let loads = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&loads);
    let server = Server::start(move |request, out| {
        if request.target == "/live/index.m3u8" {
            let load = counter.fetch_add(1, Ordering::SeqCst) as u64;
            let first = load / 2;
            respond(
                out,
                PLAYLIST_TYPE,
                media_playlist(first, 3, first == 3).as_bytes(),
            );
        } else if let Some(n) = request
            .target
            .strip_prefix("/live/seg")
            .and_then(|rest| rest.strip_suffix(".mp3"))
            .and_then(|n| n.parse::<u8>().ok())
        {
            respond(out, "audio/mpeg", &segment(n).0);
        } else {
            status(out, 404);
        }
    });
    let mut stream = open(&server.url("/live/index.m3u8"), &policy()).unwrap();
    let got = read_all(&mut stream).unwrap();
    let want: Vec<u8> = (0..6).flat_map(|i| segment(i).1).collect();
    assert_eq!(got.len(), want.len());
    assert!(got == want, "segments 0 to 5, each once, in order");

    let requests = server.requests();
    let segments: Vec<&str> = requests
        .iter()
        .map(|r| r.target.as_str())
        .filter(|t| t.contains("seg"))
        .collect();
    assert_eq!(
        segments,
        [
            "/live/seg0.mp3",
            "/live/seg1.mp3",
            "/live/seg2.mp3",
            "/live/seg3.mp3",
            "/live/seg4.mp3",
            "/live/seg5.mp3"
        ]
    );
    // Seven loads: 0 changed (the first), 1 unchanged, 2 changed, 3 unchanged,
    // 4 changed, 5 unchanged, 6 changed and ended. RFC 8216 section 6.3.4: at
    // least the target duration (1 s) after a load that found a change, half
    // of it after one that did not. The server's stamps are taken when a
    // request arrives, the client's when its load began, so allow 100 ms.
    let stamps: Vec<_> = requests
        .iter()
        .filter(|r| r.target == "/live/index.m3u8")
        .map(|r| r.at)
        .collect();
    assert_eq!(stamps.len(), 7, "no load after ENDLIST");
    for (i, pair) in stamps.windows(2).enumerate() {
        let gap = pair[1].duration_since(pair[0]);
        let previous_changed = i % 2 == 0;
        let least = if previous_changed { 900 } else { 400 };
        assert!(
            gap >= Duration::from_millis(least),
            "load {} came {gap:?} after load {i}, which {} the playlist",
            i + 1,
            if previous_changed {
                "changed"
            } else {
                "did not change"
            }
        );
    }
}

#[test]
fn every_segment_url_passes_the_policy() {
    // A second server stands for one of chorus's own listeners.
    let own = Server::start(|_, out| respond(out, "audio/mpeg", &segment(9).0));
    let foreign = own.url("/seg-on-the-control-port.mp3");
    let server = Server::start(move |request, out| {
        match request.target.as_str() {
        "/index.m3u8" => respond(
            out,
            PLAYLIST_TYPE,
            format!(
                "#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXTINF:1,\nseg0.mp3\n#EXTINF:1,\n{foreign}\n\
                 #EXTINF:1,\nhttp://169.254.169.254/seg.mp3\n#EXT-X-ENDLIST\n"
            )
            .as_bytes(),
        ),
        "/first-is-refused.m3u8" => respond(
            out,
            PLAYLIST_TYPE,
            b"#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXTINF:1,\nhttp://169.254.169.254/seg.mp3\n#EXT-X-ENDLIST\n",
        ),
        "/seg0.mp3" => respond(out, "audio/mpeg", &segment(0).0),
        _ => status(out, 404),
    }
    });
    let guarded = Policy {
        denied_ports_on_self: vec![own.port()],
        ..policy()
    };

    // The first segment plays; the second names the denied port and is
    // refused in the middle of the stream, by name.
    let mut stream = open(&server.url("/index.m3u8"), &guarded).unwrap();
    let mut got = Vec::new();
    let err = stream.read_to_end(&mut got).unwrap_err();
    assert!(got == segment(0).1);
    assert_eq!(err.kind(), ErrorKind::PermissionDenied);
    match FetchError::in_io(&err) {
        Some(FetchError::Refused(rule)) => {
            assert_eq!(
                rule,
                &format!("the server's own port {} at 127.0.0.1", own.port())
            )
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        own.connections(),
        0,
        "nothing was fetched from the refused address"
    );

    // A refused first segment is refused by `open`.
    assert_eq!(
        refused(open(&server.url("/first-is-refused.m3u8"), &guarded)),
        "link-local address 169.254.169.254"
    );
}

/// A server with one playlist at `/index.m3u8` and whatever its handler adds.
fn one_segment_stream(segment_name: &'static str, first_segment: Vec<u8>) -> Server {
    Server::start(move |request, out| {
        if request.target == "/index.m3u8" {
            respond(
                out,
                PLAYLIST_TYPE,
                format!("#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXTINF:6,\n{segment_name}\n#EXT-X-ENDLIST\n")
                    .as_bytes(),
            );
        } else if request.target == format!("/{segment_name}") {
            respond(out, "application/octet-stream", &first_segment);
        } else {
            status(out, 404);
        }
    })
}

fn playlist_only(text: &'static str) -> Server {
    Server::start(move |request, out| {
        if request.target == "/index.m3u8" {
            respond(out, PLAYLIST_TYPE, text.as_bytes());
        } else {
            status(out, 404);
        }
    })
}

#[test]
fn aac_is_refused_by_name_from_codecs_and_from_the_segment() {
    let master = playlist_only(
        "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=48000,CODECS=\"mp4a.40.5\"\nhe/index.m3u8\n\
         #EXT-X-STREAM-INF:BANDWIDTH=128000,CODECS=\"mp4a.40.2\"\nlc/index.m3u8\n",
    );
    assert_eq!(
        unsupported(open(&master.url("/index.m3u8"), &policy())),
        "hls: aac (mp4a.40.2)"
    );
    assert_eq!(master.targets(), ["/index.m3u8"], "no variant was fetched");

    // No CODECS to go by: the segment's ADTS sync word says it.
    let mut adts = timestamp_tag(0);
    adts.extend_from_slice(&[0xff, 0xf1, 0x50, 0x80, 0x02, 0x1f, 0xfc]);
    adts.extend_from_slice(&[0u8; 200]);
    let server = one_segment_stream("seg0.aac", adts);
    assert_eq!(
        unsupported(open(&server.url("/index.m3u8"), &policy())),
        "hls: aac (adts segments)"
    );
}

#[test]
fn a_later_segment_that_is_aac_stops_the_stream_by_name() {
    let server = Server::start(|request, out| {
        match request.target.as_str() {
        "/index.m3u8" => respond(
            out,
            PLAYLIST_TYPE,
            b"#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXTINF:1,\ngood.mp3\n#EXTINF:1,\nbad.aac\n#EXT-X-ENDLIST\n",
        ),
        "/good.mp3" => respond(out, "audio/mpeg", &segment(1).0),
        "/bad.aac" => respond(out, "audio/aac", &[0xff, 0xf1, 0x50, 0x80, 0, 0, 0, 0, 0, 0, 0, 0]),
        _ => status(out, 404),
    }
    });
    let mut stream = open(&server.url("/index.m3u8"), &policy()).unwrap();
    let mut got = Vec::new();
    let err = stream.read_to_end(&mut got).unwrap_err();
    assert!(got == segment(1).1);
    assert_eq!(err.kind(), ErrorKind::Unsupported);
    assert!(matches!(
        FetchError::in_io(&err),
        Some(FetchError::Unsupported(what)) if what == "hls: aac (adts segments)"
    ));
}

#[test]
fn transport_stream_segments_are_refused_by_name() {
    let mut ts = vec![0u8; 188 * 3];
    for packet in ts.chunks_mut(188) {
        packet[0] = 0x47;
    }
    let server = one_segment_stream("seg0.ts", ts);
    assert_eq!(
        unsupported(open(&server.url("/index.m3u8"), &policy())),
        "hls: mpeg-2 transport stream segments"
    );
}

#[test]
fn fragmented_mp4_is_refused_by_name_from_the_tag_and_from_the_segment() {
    let tagged = playlist_only(
        "#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-TARGETDURATION:6\n#EXT-X-MAP:URI=\"init.mp4\"\n\
         #EXTINF:6,\nseg0.m4s\n#EXT-X-ENDLIST\n",
    );
    assert_eq!(
        unsupported(open(&tagged.url("/index.m3u8"), &policy())),
        "hls: fragmented mp4 segments (EXT-X-MAP)"
    );
    assert_eq!(tagged.targets(), ["/index.m3u8"]);

    for brand in [&b"ftypiso5"[..], b"stypmsdh"] {
        let mut mp4 = vec![0, 0, 0, 0x18];
        mp4.extend_from_slice(brand);
        mp4.extend_from_slice(&[0u8; 64]);
        let server = one_segment_stream("seg0.m4s", mp4);
        assert_eq!(
            unsupported(open(&server.url("/index.m3u8"), &policy())),
            "hls: fragmented mp4 segments"
        );
    }
}

#[test]
fn encrypted_playlists_are_refused_by_name() {
    let aes = playlist_only(
        "#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXT-X-KEY:METHOD=AES-128,URI=\"key.bin\"\n\
         #EXTINF:6,\nseg0.mp3\n#EXT-X-ENDLIST\n",
    );
    assert_eq!(
        unsupported(open(&aes.url("/index.m3u8"), &policy())),
        "hls: encrypted segments (EXT-X-KEY METHOD=AES-128)"
    );
    assert_eq!(
        aes.targets(),
        ["/index.m3u8"],
        "neither the key nor a segment was fetched"
    );

    let sample = playlist_only(
        "#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXT-X-KEY:METHOD=SAMPLE-AES,URI=\"skd://keys.example/1\"\n\
         #EXTINF:6,\nseg0.mp3\n#EXT-X-ENDLIST\n",
    );
    assert_eq!(
        unsupported(open(&sample.url("/index.m3u8"), &policy())),
        "hls: encrypted segments (EXT-X-KEY METHOD=SAMPLE-AES)"
    );
}

#[test]
fn byte_range_playlists_are_refused_by_name() {
    let server = playlist_only(
        "#EXTM3U\n#EXT-X-VERSION:4\n#EXT-X-TARGETDURATION:6\n#EXTINF:6,\n#EXT-X-BYTERANGE:1000@0\n\
         all.mp3\n#EXT-X-ENDLIST\n",
    );
    assert_eq!(
        unsupported(open(&server.url("/index.m3u8"), &policy())),
        "hls: byte-range segments (EXT-X-BYTERANGE)"
    );
    assert_eq!(server.targets(), ["/index.m3u8"]);
}

#[test]
fn other_playlists_and_segments_have_their_own_words() {
    let plain = playlist_only("#EXTM3U\n#EXTINF:-1,Example Radio\nhttp://radio.example/stream\n");
    assert_eq!(
        unsupported(open(&plain.url("/index.m3u8"), &policy())),
        "m3u: a plain playlist, not HLS (no EXT-X-TARGETDURATION); use the stream's own url"
    );
    let html = one_segment_stream("seg0.mp3", b"<html>not found</html>".to_vec());
    assert_eq!(
        unsupported(open(&html.url("/index.m3u8"), &policy())),
        "hls: segments that are not packed mp3 audio"
    );
    let not_a_playlist = Server::start(|_, out| respond(out, PLAYLIST_TYPE, b"<html></html>"));
    assert!(matches!(
        open(&not_a_playlist.url("/index.m3u8"), &policy()),
        Err(FetchError::Malformed(_))
    ));
    let missing_segment =
        playlist_only("#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXTINF:6,\nseg0.mp3\n#EXT-X-ENDLIST\n");
    assert!(matches!(
        open(&missing_segment.url("/index.m3u8"), &policy()),
        Err(FetchError::Http(404))
    ));
}
