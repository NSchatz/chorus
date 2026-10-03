//! The HTTP client against in-test servers on loopback: the three body
//! framings, redirects and their bound, the policy at every hop, Range seeks,
//! timeouts and the header bound.

mod common;

use std::io::{ErrorKind, Read, SeekFrom};
use std::thread;
use std::time::{Duration, Instant};

use chorus_fetch::{open, FetchError, Policy};
use common::{pattern, policy, read_all, redirect, refused, respond, status, unsupported, Server};

#[test]
fn a_file_with_a_content_length_arrives_whole() {
    let body = pattern(100_000, 1);
    let served = body.clone();
    let server = Server::start(move |_, out| respond(out, "audio/flac", &served));
    let mut stream = open(&server.url("/music/track.flac?x=1"), &policy()).unwrap();
    let opened = stream.opened().clone();
    assert_eq!(opened.content_type.as_deref(), Some("audio/flac"));
    assert_eq!(opened.byte_len, Some(100_000));
    assert!(!opened.seekable, "no Accept-Ranges was sent");
    assert_eq!(opened.icy, None);
    assert!(!opened.hls);
    assert_eq!(opened.final_url, server.url("/music/track.flac?x=1"));
    assert_eq!(read_all(&mut stream).unwrap(), body);
    assert_eq!(stream.stream_title(), None);

    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert!(
        request
            .raw
            .starts_with("GET /music/track.flac?x=1 HTTP/1.1\r\n"),
        "{}",
        request.raw
    );
    assert_eq!(
        request.header("Host"),
        Some(server.addr.to_string().as_str())
    );
    assert_eq!(
        request.header("User-Agent"),
        Some(format!("chorus/{}", env!("CARGO_PKG_VERSION")).as_str())
    );
    assert_eq!(request.header("Icy-MetaData"), Some("1"));
    assert_eq!(request.header("Accept"), Some("*/*"));
    assert_eq!(request.header("Connection"), Some("close"));
    assert_eq!(request.header("Range"), None);
}

#[test]
fn a_chunked_body_arrives_whole() {
    let body = pattern(50_000, 2);
    let served = body.clone();
    let server = Server::start(move |_, out| {
        let _ = write!(
            out,
            "HTTP/1.1 200 OK\r\nContent-Type: audio/mpeg\r\nTransfer-Encoding: chunked\r\n\r\n"
        );
        for (i, chunk) in served.chunks(777).enumerate() {
            let _ = write!(out, "{:x}\r\n", chunk.len());
            // Split some chunks across writes, so a chunk boundary and a
            // read boundary do not line up.
            let (a, b) = chunk.split_at(chunk.len() / 2);
            let _ = out.write_all(a);
            if i % 8 == 0 {
                let _ = out.flush();
                thread::sleep(Duration::from_millis(2));
            }
            let _ = out.write_all(b);
            let _ = out.write_all(b"\r\n");
        }
        let _ = out.write_all(b"0\r\n\r\n");
    });
    let mut stream = open(&server.url("/chunked"), &policy()).unwrap();
    assert_eq!(stream.opened().byte_len, None);
    assert!(!stream.opened().seekable);
    assert_eq!(read_all(&mut stream).unwrap(), body);
}

#[test]
fn a_chunked_body_cut_short_is_an_error_not_an_end() {
    let server = Server::start(|_, out| {
        let _ =
            out.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n10\r\nonly eight");
    });
    let mut stream = open(&server.url("/cut"), &policy()).unwrap();
    assert_eq!(
        read_all(&mut stream).unwrap_err().kind(),
        ErrorKind::UnexpectedEof
    );
}

#[test]
fn a_body_with_no_length_is_read_until_the_close() {
    let body = pattern(30_000, 3);
    let served = body.clone();
    let server = Server::start(move |_, out| {
        let _ = out.write_all(b"HTTP/1.0 200 OK\r\nContent-Type: audio/mpeg\r\n\r\n");
        let _ = out.write_all(&served);
    });
    let mut stream = open(&server.url("/live"), &policy()).unwrap();
    assert_eq!(stream.opened().byte_len, None);
    assert!(!stream.opened().seekable);
    assert_eq!(
        stream.seek(SeekFrom::Start(10)).unwrap_err().kind(),
        ErrorKind::Unsupported
    );
    assert_eq!(read_all(&mut stream).unwrap(), body);
}

#[test]
fn a_short_content_length_body_is_an_error() {
    let server = Server::start(|_, out| {
        let _ = out.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nonly this");
    });
    let mut stream = open(&server.url("/short"), &policy()).unwrap();
    assert_eq!(
        read_all(&mut stream).unwrap_err().kind(),
        ErrorKind::UnexpectedEof
    );
}

#[test]
fn an_error_status_is_reported_as_itself() {
    let server = Server::start(|request, out| {
        status(out, if request.target == "/gone" { 410 } else { 404 });
    });
    assert!(matches!(
        open(&server.url("/missing"), &policy()),
        Err(FetchError::Http(404))
    ));
    assert!(matches!(
        open(&server.url("/gone"), &policy()),
        Err(FetchError::Http(410))
    ));
}

#[test]
fn a_compressed_body_is_refused_by_name() {
    let server = Server::start(|_, out| {
        let _ = out.write_all(
            b"HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: 3\r\n\r\nabc",
        );
    });
    assert_eq!(
        unsupported(open(&server.url("/z"), &policy())),
        "http: content coding gzip"
    );
}

#[test]
fn a_redirect_chain_is_followed_to_the_file() {
    let body = pattern(5_000, 4);
    let served = body.clone();
    let last = Server::start(move |request, out| match request.target.as_str() {
        "/c" => redirect(out, 307, "d?token=1"),
        "/d?token=1" => redirect(out, 308, "/files/final.mp3"),
        "/files/final.mp3" => respond(out, "audio/mpeg", &served),
        _ => status(out, 404),
    });
    let onward = last.url("/c");
    let first = Server::start(move |request, out| match request.target.as_str() {
        "/a" => redirect(out, 301, "/b"),
        "/b" => redirect(out, 302, "b2"),
        "/b2" => redirect(out, 303, &onward),
        _ => status(out, 404),
    });
    let mut stream = open(&first.url("/a"), &policy()).unwrap();
    assert_eq!(stream.opened().final_url, last.url("/files/final.mp3"));
    assert_eq!(read_all(&mut stream).unwrap(), body);
    assert_eq!(first.targets(), ["/a", "/b", "/b2"]);
    assert_eq!(last.targets(), ["/c", "/d?token=1", "/files/final.mp3"]);

    // Five redirects is the bound of this policy; one fewer allowed is a refusal.
    let tight = Policy {
        max_redirects: 4,
        ..policy()
    };
    assert_eq!(
        refused(open(&first.url("/a"), &tight)),
        "more than 4 redirects"
    );
}

#[test]
fn a_redirect_loop_ends_at_the_bound() {
    let server = Server::start(|_, out| redirect(out, 302, "/loop"));
    let bounded = Policy {
        max_redirects: 3,
        ..policy()
    };
    assert_eq!(
        refused(open(&server.url("/loop"), &bounded)),
        "more than 3 redirects"
    );
    // The first request and three redirects followed; the fourth is not.
    assert_eq!(server.connections(), 4);

    let none = Policy {
        max_redirects: 0,
        ..policy()
    };
    assert_eq!(
        refused(open(&server.url("/loop"), &none)),
        "more than 0 redirects"
    );
    assert_eq!(server.connections(), 5);
}

#[test]
fn a_redirect_without_a_location_is_malformed() {
    let server = Server::start(|_, out| status(out, 302));
    assert!(matches!(
        open(&server.url("/"), &policy()),
        Err(FetchError::Malformed(_))
    ));
}

#[test]
fn a_redirect_to_a_refused_address_is_refused_and_nothing_is_fetched_from_it() {
    // The "server's own listener": a port on this machine the policy denies.
    let own = Server::start(|_, out| respond(out, "text/plain", b"the control port's secrets"));
    let target = own.url("/api/state");
    let front = Server::start(move |request, out| match request.target.as_str() {
        "/own" => redirect(out, 302, &target),
        "/link-local" => redirect(out, 302, "http://169.254.169.254/latest/meta-data/"),
        "/link-local-v6" => redirect(out, 302, "http://[fe80::1]/"),
        "/mapped" => redirect(out, 302, "http://[::ffff:169.254.169.254]/"),
        "/unspecified" => redirect(out, 302, "http://0.0.0.0:9/"),
        "/multicast" => redirect(out, 302, "http://239.255.255.250:1900/"),
        "/broadcast" => redirect(out, 302, "http://255.255.255.255/"),
        "/file" => redirect(out, 302, "file:///etc/passwd"),
        "/userinfo" => redirect(out, 302, "http://user:secret@192.0.2.1/"),
        _ => status(out, 404),
    });
    let guarded = Policy {
        denied_ports_on_self: vec![own.port()],
        ..policy()
    };

    assert_eq!(
        refused(open(&front.url("/own"), &guarded)),
        format!("the server's own port {} at 127.0.0.1", own.port())
    );
    assert_eq!(
        own.connections(),
        0,
        "nothing was fetched from the refused address"
    );

    for (path, rule) in [
        ("/link-local", "link-local address 169.254.169.254"),
        ("/link-local-v6", "link-local address fe80::1"),
        ("/mapped", "link-local address 169.254.169.254"),
        ("/unspecified", "unspecified address 0.0.0.0"),
        ("/multicast", "multicast address 239.255.255.250"),
        ("/broadcast", "broadcast address 255.255.255.255"),
        ("/file", "scheme file: only http and https are fetched"),
        (
            "/userinfo",
            "userinfo in the url: a url with user:password@ is not fetched",
        ),
    ] {
        assert_eq!(refused(open(&front.url(path), &guarded)), rule, "{path}");
    }

    // The same port asked for directly is refused before any socket opens.
    assert!(refused(open(&own.url("/api/state"), &guarded)).starts_with("the server's own port"));
    assert_eq!(own.connections(), 0);
    // Without the denial the same redirect is followed: the refusal above was the policy's.
    let mut stream = open(&front.url("/own"), &policy()).unwrap();
    assert_eq!(
        read_all(&mut stream).unwrap(),
        b"the control port's secrets"
    );
    assert_eq!(own.connections(), 1);
}

#[test]
fn loopback_is_refused_before_any_socket_is_opened() {
    let server = Server::start(|_, out| respond(out, "audio/mpeg", b"never served"));
    let production = Policy {
        allow_loopback: false,
        ..policy()
    };
    assert_eq!(
        refused(open(&server.url("/stream"), &production)),
        "loopback address 127.0.0.1"
    );
    let by_name = format!("http://localhost:{}/stream", server.port());
    assert!(refused(open(&by_name, &production)).starts_with("loopback address "));
    let v6 = format!("http://[::1]:{}/stream", server.port());
    assert_eq!(refused(open(&v6, &production)), "loopback address ::1");
    let mapped = format!("http://[::ffff:127.0.0.1]:{}/stream", server.port());
    assert_eq!(
        refused(open(&mapped, &production)),
        "loopback address 127.0.0.1"
    );
    assert_eq!(server.connections(), 0);
}

#[test]
fn refused_urls_never_reach_the_network() {
    for (url, rule) in [
        (
            "ftp://radio.example/stream.mp3",
            "scheme ftp: only http and https are fetched",
        ),
        (
            "file:///etc/passwd",
            "scheme file: only http and https are fetched",
        ),
        (
            "http://user:secret@radio.example/stream",
            "userinfo in the url: a url with user:password@ is not fetched",
        ),
        (
            "http://169.254.169.254/",
            "link-local address 169.254.169.254",
        ),
        ("http://224.0.0.251:5353/", "multicast address 224.0.0.251"),
        ("http://[ff02::fb]/", "multicast address ff02::fb"),
        ("http://0.0.0.0/", "unspecified address 0.0.0.0"),
        ("https://[::]/", "unspecified address ::"),
    ] {
        assert_eq!(refused(open(url, &Policy::default())), rule, "{url}");
    }
    assert!(matches!(
        open("not a url", &policy()),
        Err(FetchError::Malformed(_))
    ));
}

#[test]
fn a_closed_port_is_an_io_error() {
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    match open(&format!("http://127.0.0.1:{port}/"), &policy()) {
        Err(FetchError::Io(e)) => assert_eq!(e.kind(), ErrorKind::ConnectionRefused),
        other => panic!("{:?}", other.map(|_| ())),
    }
}

/// A file server that honours `Range: bytes=N-`.
fn range_server(body: Vec<u8>, honour: bool) -> Server {
    Server::start(move |request, out| {
        let from = request
            .header("Range")
            .and_then(|r| r.strip_prefix("bytes="))
            .and_then(|r| r.strip_suffix('-'))
            .and_then(|n| n.parse::<usize>().ok());
        match from {
            Some(from) if honour && from < body.len() => {
                let _ = write!(
                    out,
                    "HTTP/1.1 206 Partial Content\r\nAccept-Ranges: bytes\r\nContent-Type: audio/mp4\r\n\
                     Content-Range: bytes {}-{}/{}\r\nContent-Length: {}\r\n\r\n",
                    from,
                    body.len() - 1,
                    body.len(),
                    body.len() - from
                );
                let _ = out.write_all(&body[from..]);
            }
            Some(_) if honour => status(out, 416),
            _ => {
                let _ = write!(
                    out,
                    "HTTP/1.1 200 OK\r\nAccept-Ranges: bytes\r\nContent-Type: audio/mp4\r\nContent-Length: {}\r\n\r\n",
                    body.len()
                );
                let _ = out.write_all(&body);
            }
        }
    })
}

fn read_exactly(stream: &mut chorus_fetch::Stream, n: usize) -> Vec<u8> {
    let mut buf = vec![0u8; n];
    stream.read_exact(&mut buf).unwrap();
    buf
}

#[test]
fn seeking_re_requests_with_a_range() {
    let body = pattern(1_000_000, 5);
    let server = range_server(body.clone(), true);
    let mut stream = open(&server.url("/album/track.m4a"), &policy()).unwrap();
    assert!(stream.opened().seekable);
    assert_eq!(stream.opened().byte_len, Some(1_000_000));
    assert_eq!(read_exactly(&mut stream, 100), &body[..100]);

    // An MP4 with its index at the end: jump there, read, jump back.
    assert_eq!(stream.seek(SeekFrom::End(-1000)).unwrap(), 999_000);
    assert_eq!(read_all(&mut stream).unwrap(), &body[999_000..]);
    assert_eq!(stream.seek(SeekFrom::Start(40)).unwrap(), 40);
    assert_eq!(read_exactly(&mut stream, 60), &body[40..100]);
    assert_eq!(stream.seek(SeekFrom::Current(499_900)).unwrap(), 500_000);
    assert_eq!(read_exactly(&mut stream, 10), &body[500_000..500_010]);
    assert_eq!(stream.seek(SeekFrom::Current(-10)).unwrap(), 500_000);
    assert_eq!(read_exactly(&mut stream, 10), &body[500_000..500_010]);
    let ranges: Vec<Option<String>> = server
        .requests()
        .iter()
        .map(|r| r.header("Range").map(str::to_string))
        .collect();
    assert_eq!(
        ranges,
        [
            None,
            Some("bytes=999000-".to_string()),
            Some("bytes=40-".to_string()),
            Some("bytes=500000-".to_string()),
            Some("bytes=500000-".to_string()),
        ]
    );

    // A short hop forward is read past on the open connection: no new request.
    assert_eq!(stream.seek(SeekFrom::Current(5_000)).unwrap(), 505_010);
    assert_eq!(read_exactly(&mut stream, 10), &body[505_010..505_020]);
    // Two seeks with no read between them cost one request, not two.
    stream.seek(SeekFrom::Start(0)).unwrap();
    stream.seek(SeekFrom::Start(900_000)).unwrap();
    assert_eq!(read_exactly(&mut stream, 10), &body[900_000..900_010]);
    assert_eq!(server.requests().len(), 6);

    // The end, and past it, read as the end without a request.
    assert_eq!(stream.seek(SeekFrom::End(0)).unwrap(), 1_000_000);
    assert_eq!(read_all(&mut stream).unwrap(), b"");
    assert_eq!(stream.seek(SeekFrom::Start(2_000_000)).unwrap(), 2_000_000);
    assert_eq!(read_all(&mut stream).unwrap(), b"");
    assert_eq!(server.requests().len(), 6);
    assert_eq!(
        stream.seek(SeekFrom::End(-2_000_000)).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    // And back from the end.
    stream.seek(SeekFrom::Start(999_990)).unwrap();
    assert_eq!(read_all(&mut stream).unwrap(), &body[999_990..]);
}

#[test]
fn a_server_that_ignores_range_cannot_be_seeked() {
    let body = pattern(600_000, 6);
    let server = range_server(body.clone(), false);
    let mut stream = open(&server.url("/track"), &policy()).unwrap();
    assert!(stream.opened().seekable, "it claimed Accept-Ranges");
    stream.seek(SeekFrom::Start(500_000)).unwrap();
    let mut buf = [0u8; 16];
    assert_eq!(
        stream.read(&mut buf).unwrap_err().kind(),
        ErrorKind::Unsupported
    );
    // A seek back to the start is served by the whole body it sends.
    stream.seek(SeekFrom::Start(0)).unwrap();
    assert_eq!(read_exactly(&mut stream, 16), &body[..16]);
}

#[test]
fn a_server_that_stops_sending_hits_the_read_timeout() {
    let server = Server::start(|request, out| {
        if request.target == "/silent" {
            thread::sleep(Duration::from_secs(8));
            return;
        }
        let _ = out
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\nsome bytes, then nothing");
        let _ = out.flush();
        thread::sleep(Duration::from_secs(8));
    });
    let impatient = Policy {
        read_timeout: Duration::from_millis(300),
        ..policy()
    };

    let mut stream = open(&server.url("/stalls"), &impatient).unwrap();
    let began = Instant::now();
    let err = read_all(&mut stream).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::TimedOut, "{err}");
    let waited = began.elapsed();
    assert!(
        waited >= Duration::from_millis(250) && waited < Duration::from_secs(6),
        "{waited:?}"
    );

    // No headers at all: the same bound, at open.
    let began = Instant::now();
    match open(&server.url("/silent"), &impatient) {
        Err(FetchError::Io(e)) => assert_eq!(e.kind(), ErrorKind::TimedOut),
        other => panic!("{:?}", other.map(|_| ())),
    }
    assert!(began.elapsed() < Duration::from_secs(6));
}

#[test]
fn headers_dripped_slowly_are_bounded_as_a_whole() {
    // Each line arrives inside the read timeout, the head as a whole does not.
    let server = Server::start(|_, out| {
        let _ = out.write_all(b"HTTP/1.1 200 OK\r\n");
        for i in 0..40 {
            let _ = write!(out, "X-Drip-{i}: 1\r\n");
            let _ = out.flush();
            thread::sleep(Duration::from_millis(100));
        }
        let _ = out.write_all(b"\r\n");
    });
    let impatient = Policy {
        read_timeout: Duration::from_millis(400),
        ..policy()
    };
    let began = Instant::now();
    match open(&server.url("/drip"), &impatient) {
        Err(FetchError::Io(e)) => assert_eq!(e.kind(), ErrorKind::TimedOut),
        other => panic!("{:?}", other.map(|_| ())),
    }
    assert!(
        began.elapsed() < Duration::from_millis(3900),
        "{:?}",
        began.elapsed()
    );
}

#[test]
fn oversize_headers_are_refused() {
    let server = Server::start(|_, out| {
        let _ = out.write_all(b"HTTP/1.1 200 OK\r\n");
        for i in 0..2000 {
            let _ = write!(out, "X-Padding-{i}: {}\r\n", "p".repeat(40));
        }
        let _ = out.write_all(b"\r\n");
    });
    assert_eq!(
        refused(open(&server.url("/big-head"), &policy())),
        "response headers larger than 8192 bytes"
    );
}

#[test]
fn what_is_not_http_is_malformed() {
    let server = Server::start(|_, out| {
        let _ = out.write_all(b"SSH-2.0-OpenSSH_9.9\r\n");
    });
    assert!(matches!(
        open(&server.url("/"), &policy()),
        Err(FetchError::Malformed(_))
    ));
}
