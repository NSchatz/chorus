//! HTTPS against an in-test rustls server whose certificates are made at run
//! time (`common/pki.rs`): a trusted certificate works, by address and by
//! name; a certificate for another host and one from an untrusted root are
//! both refused before any request is sent; a missing or empty CA bundle is
//! the named error and opens no socket.

mod common;

use std::io::{Read, SeekFrom};

use chorus_fetch::{open, FetchError, Policy};
use common::pki::Authority;
use common::{pattern, policy, read_all, redirect, respond, status, Server};

fn tls_error(result: Result<chorus_fetch::Stream, FetchError>) -> String {
    match result {
        Err(FetchError::Tls(why)) => why,
        Err(other) => panic!("expected a tls error, got {other:?}"),
        Ok(_) => panic!("expected a tls error, got a stream"),
    }
}

#[test]
fn a_trusted_certificate_is_accepted_by_address_and_by_name() {
    let authority = Authority::new("chorus test root A");
    let bundle = authority.bundle("trusted");
    let body = pattern(200_000, 11);
    let served = body.clone();
    let server = Server::start_tls(
        authority.server(&["localhost"], &[[127, 0, 0, 1]]),
        move |request, out| match request.target.as_str() {
            "/moved" => redirect(out, 302, "/track.mp3"),
            _ => respond(out, "audio/mpeg", &served),
        },
    );
    let trusting = Policy {
        ca_bundle: Some(bundle.clone()),
        ..policy()
    };

    let mut stream = open(&server.https_url("/track.mp3"), &trusting).unwrap();
    assert_eq!(stream.opened().byte_len, Some(200_000));
    assert_eq!(stream.opened().final_url, server.https_url("/track.mp3"));
    assert!(read_all(&mut stream).unwrap() == body);

    let by_name = format!("https://localhost:{}/moved", server.port());
    let mut stream = open(&by_name, &trusting).unwrap();
    assert_eq!(
        stream.opened().final_url,
        format!("https://localhost:{}/track.mp3", server.port())
    );
    assert!(read_all(&mut stream).unwrap() == body);
    assert_eq!(server.targets(), ["/track.mp3", "/moved", "/track.mp3"]);
    std::fs::remove_file(bundle).unwrap();
}

#[test]
fn a_seek_over_https_reconnects_with_a_range() {
    let authority = Authority::new("chorus test root B");
    let bundle = authority.bundle("range");
    let body = pattern(700_000, 12);
    let served = body.clone();
    let server = Server::start_tls(
        authority.server(&[], &[[127, 0, 0, 1]]),
        move |request, out| {
            let from = request
                .header("Range")
                .and_then(|r| r.strip_prefix("bytes="))
                .and_then(|r| r.strip_suffix('-'))
                .and_then(|n| n.parse::<usize>().ok())
                .unwrap_or(0);
            let code = if request.header("Range").is_some() {
                "206 Partial Content"
            } else {
                "200 OK"
            };
            let _ = write!(
                out,
                "HTTP/1.1 {code}\r\nAccept-Ranges: bytes\r\nContent-Range: bytes {from}-{}/{}\r\n\
             Content-Length: {}\r\n\r\n",
                served.len() - 1,
                served.len(),
                served.len() - from
            );
            let _ = out.write_all(&served[from..]);
        },
    );
    let trusting = Policy {
        ca_bundle: Some(bundle.clone()),
        ..policy()
    };
    let mut stream = open(&server.https_url("/a.m4a"), &trusting).unwrap();
    assert!(stream.opened().seekable);
    stream.seek(SeekFrom::End(-5000)).unwrap();
    assert!(read_all(&mut stream).unwrap() == body[695_000..]);
    stream.seek(SeekFrom::Start(10)).unwrap();
    let mut head = [0u8; 20];
    stream.read_exact(&mut head).unwrap();
    assert_eq!(head, body[10..30]);
    std::fs::remove_file(bundle).unwrap();
}

#[test]
fn a_certificate_for_another_host_is_refused() {
    let authority = Authority::new("chorus test root C");
    let bundle = authority.bundle("wrong-host");
    let server = Server::start_tls(
        authority.server(&["radio.example"], &[[192, 0, 2, 1]]),
        |_, out| respond(out, "audio/mpeg", b"never served"),
    );
    let trusting = Policy {
        ca_bundle: Some(bundle.clone()),
        ..policy()
    };
    let why = tls_error(open(&server.https_url("/stream"), &trusting));
    assert!(why.starts_with("127.0.0.1: "), "{why}");
    assert!(
        why.contains("certificate not valid for name \"127.0.0.1\""),
        "{why}"
    );
    let by_name = format!("https://localhost:{}/stream", server.port());
    let why = tls_error(open(&by_name, &trusting));
    assert!(
        why.contains("certificate not valid for name \"localhost\""),
        "{why}"
    );
    assert!(
        server.requests().is_empty(),
        "no request was sent to a server that is not the one named"
    );
    std::fs::remove_file(bundle).unwrap();
}

#[test]
fn a_certificate_from_an_untrusted_root_is_refused() {
    let trusted = Authority::new("chorus test root D");
    let other = Authority::new("chorus test root E");
    let bundle = trusted.bundle("untrusted");
    let server = Server::start_tls(other.server(&["localhost"], &[[127, 0, 0, 1]]), |_, out| {
        respond(out, "audio/mpeg", b"never served")
    });
    let trusting = Policy {
        ca_bundle: Some(bundle.clone()),
        ..policy()
    };
    let why = tls_error(open(&server.https_url("/stream"), &trusting));
    assert!(why.contains("UnknownIssuer"), "{why}");
    assert!(server.requests().is_empty());
    std::fs::remove_file(bundle).unwrap();
}

#[test]
fn a_missing_or_empty_bundle_is_the_named_error_and_opens_no_socket() {
    let authority = Authority::new("chorus test root F");
    let server = Server::start_tls(authority.server(&[], &[[127, 0, 0, 1]]), |_, out| {
        status(out, 200)
    });
    let missing = std::env::temp_dir().join(format!(
        "chorus-fetch-test-{}-no-such-bundle.pem",
        std::process::id()
    ));
    let policy_missing = Policy {
        ca_bundle: Some(missing.clone()),
        ..policy()
    };
    let why = tls_error(open(&server.https_url("/"), &policy_missing));
    assert!(
        why.starts_with(&format!("ca bundle {}: cannot be read", missing.display())),
        "{why}"
    );

    let empty = std::env::temp_dir().join(format!(
        "chorus-fetch-test-{}-empty-bundle.pem",
        std::process::id()
    ));
    std::fs::write(&empty, "").unwrap();
    let policy_empty = Policy {
        ca_bundle: Some(empty.clone()),
        ..policy()
    };
    assert_eq!(
        tls_error(open(&server.https_url("/"), &policy_empty)),
        format!("ca bundle {}: holds no usable certificate", empty.display())
    );
    std::fs::remove_file(empty).unwrap();
    assert_eq!(server.connections(), 0);

    // The same bundle is not needed for plain http.
    let plain = Server::start(|_, out| respond(out, "audio/mpeg", b"plain"));
    let mut stream = open(&plain.url("/"), &policy_missing).unwrap();
    assert_eq!(read_all(&mut stream).unwrap(), b"plain");
}

#[test]
fn a_server_that_does_not_speak_tls_is_a_tls_error() {
    let authority = Authority::new("chorus test root G");
    let bundle = authority.bundle("not-tls");
    let plain = Server::start(|_, out| respond(out, "audio/mpeg", b"plain"));
    let trusting = Policy {
        ca_bundle: Some(bundle.clone()),
        read_timeout: std::time::Duration::from_millis(500),
        ..policy()
    };
    assert!(matches!(
        open(&format!("https://{}/", plain.addr), &trusting),
        Err(FetchError::Tls(_) | FetchError::Io(_))
    ));
    std::fs::remove_file(bundle).unwrap();
}
