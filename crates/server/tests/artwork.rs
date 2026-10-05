//! A group's now-playing artwork, served from the control plane's own origin
//! (`docs/decisions/0184-artwork-is-proxied-not-the-policy-widened.md`).
//!
//! The control plane is assembled in this process (the room model, the fixed
//! worker pool, the accept loop: the pieces `main.rs` wires) so the test can
//! put a now-playing record into a group as a player's driver does
//! (`ControlState::set_now_playing`). The artwork's origin is a small HTTP
//! server on loopback that this file runs and that keeps every request line
//! it was sent, so "the server fetched this and nothing else" is read off
//! the origin and not inferred.
//!
//! What is asserted: the image's bytes with an image media type, a
//! validator and the control page's Content-Security-Policy, unchanged; `304`
//! for the validator and the new image when the record names another; `404`
//! for a group with no record, a record with no artwork and a group this
//! server does not have; an upstream answer that is not an image or is
//! larger than the bound refused; a URL in the request never fetched; an
//! artwork URL the fetch policy refuses (this server's own control port)
//! never dialled; an origin that never answers given up at the deadline.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chorus_control::rooms::{NowPlaying, PlayState};
use chorus_control::zones::{Zone, Zones};
use chorus_hostctl::ThreadRegistry;
use chorus_server::artwork::{self, Artwork, Bounds};
use chorus_server::control::{ControlPlane, ControlState};
use chorus_server::mediaplayer::fetch_policy;

/// `crates/server/src/control.rs::CONTENT_SECURITY_POLICY`, written out a
/// second time on purpose: serving artwork must not have changed one byte of
/// it, and a check that read the constant could not tell.
const CONTENT_SECURITY_POLICY: &str = "default-src 'none'; script-src 'self'; style-src 'self'; \
     connect-src 'self'; img-src 'self' data:; base-uri 'none'; form-action 'none'; \
     frame-ancestors 'none'";

/// The eight bytes every PNG starts with, then bytes that tell two covers
/// apart. The route judges what it was sent by its first bytes; it decodes
/// nothing.
fn png(mark: &str) -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    bytes.extend_from_slice(mark.as_bytes());
    // Every byte value, so a response that went through text would differ.
    bytes.extend(0u8..=255);
    bytes
}

fn jpeg() -> Vec<u8> {
    let mut bytes = vec![0xff, 0xd8, 0xff, 0xe0];
    bytes.extend_from_slice(b"a jpeg, as far as its first bytes say");
    bytes
}

/// How the origin frames one answer.
enum Framing {
    /// `Content-Length`.
    Length,
    /// No length: the body ends when the connection closes.
    UntilClose,
}

/// One thing the origin serves.
struct Served {
    path: &'static str,
    content_type: &'static str,
    framing: Framing,
    body: Vec<u8>,
}

/// The artwork's origin: an HTTP server on loopback that answers the paths it
/// was given, `404` for any other, and nothing at all for `/stall`.
struct Origin {
    address: String,
    /// Every request line it was sent, in order.
    asked: Arc<Mutex<Vec<String>>>,
}

impl Origin {
    fn start(served: Vec<Served>) -> Origin {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let asked = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&asked);
        thread::spawn(move || {
            // Connections to `/stall`, kept open and never answered.
            let mut held = Vec::new();
            for connection in listener.incoming() {
                let Ok(mut connection) = connection else {
                    return;
                };
                let mut reader = BufReader::new(connection.try_clone().unwrap());
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    continue;
                }
                seen.lock().unwrap().push(line.trim_end().to_string());
                loop {
                    let mut header = String::new();
                    match reader.read_line(&mut header) {
                        Ok(n) if n > 0 && header.trim_end() != "" => {}
                        _ => break,
                    }
                }
                let path = line.split_whitespace().nth(1).unwrap_or("");
                if path == "/stall" {
                    held.push(connection);
                    continue;
                }
                match served.iter().find(|s| s.path == path) {
                    Some(s) => {
                        let length = match s.framing {
                            Framing::Length => format!("Content-Length: {}\r\n", s.body.len()),
                            Framing::UntilClose => String::new(),
                        };
                        let _ = write!(
                            connection,
                            "HTTP/1.1 200 OK\r\nContent-Type: {}\r\n{}Connection: close\r\n\r\n",
                            s.content_type, length
                        );
                        // The server may hang up before an oversized body is
                        // all written; that is the outcome under test.
                        let _ = connection.write_all(&s.body);
                    }
                    None => {
                        let _ = write!(
                            connection,
                            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        );
                    }
                }
                let _ = connection.flush();
            }
        });
        Origin { address, asked }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.address, path)
    }

    fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }
}

/// The control plane, running in this process.
struct House {
    control: String,
    state: Arc<ControlState>,
    keep: Arc<AtomicBool>,
}

impl Drop for House {
    fn drop(&mut self) {
        // The accept loop looks at the flag after its next connection.
        self.keep.store(false, Ordering::SeqCst);
        let _ = TcpStream::connect(&self.control);
    }
}

/// A server with two rooms and one player, the kitchen playing it; artwork
/// fetched under the server's own fetch policy with loopback allowed (the
/// origin lives there) and under `bounds`.
fn house(bounds: Bounds) -> House {
    let mut zones = Zones::new("127.0.0.1:1");
    zones.add(Zone::new("kitchen")).unwrap();
    zones.add(Zone::new("study")).unwrap();
    let state = Arc::new(ControlState::new(zones, None));
    state.set_players(1);
    let mut plane = ControlPlane::bind("127.0.0.1:0", Arc::clone(&state)).unwrap();
    let control = plane.address().to_string();
    let control_port: u16 = control.rsplit(':').next().unwrap().parse().unwrap();
    // What `main.rs` does: this server's own listeners are named in the
    // policy, so no artwork URL can make it call itself.
    state.artwork_through(Artwork::with_bounds(
        fetch_policy(&[control_port], true),
        bounds,
    ));
    let (ready, readied) = mpsc::channel();
    plane.spawn_workers(4, Arc::new(ThreadRegistry::new()), ready);
    for _ in 0..plane.threads() {
        readied.recv_timeout(Duration::from_secs(30)).unwrap();
    }
    let keep = Arc::new(AtomicBool::new(true));
    let running = Arc::clone(&keep);
    thread::spawn(move || plane.accept_loop(running));
    state
        .apply(r#"{"v":2,"t":"take","target":"kitchen","source":"player:p0"}"#)
        .unwrap();
    House {
        control,
        state,
        keep,
    }
}

/// Say what the kitchen's player is playing: `title`, with artwork at
/// `art_url` when there is one.
fn playing(house: &House, title: &str, art_url: Option<String>) {
    house
        .state
        .set_now_playing(
            "kitchen",
            Some(NowPlaying {
                title: Some(title.to_string()),
                artist: None,
                album: None,
                art_url,
                duration_ms: None,
                state: PlayState::Playing,
                via: "upnp".to_string(),
            }),
        )
        .unwrap();
}

/// One answer of the control plane.
struct Answer {
    status: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Answer {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    fn code(&self) -> &str {
        self.status.split_whitespace().nth(1).unwrap_or("")
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }
}

/// `GET <target>` on a fresh connection, with `If-None-Match` when given.
fn get(house: &House, target: &str, if_none_match: Option<&str>) -> Answer {
    let mut socket = TcpStream::connect(&house.control).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    let validator = if_none_match
        .map(|tag| format!("If-None-Match: {}\r\n", tag))
        .unwrap_or_default();
    write!(
        socket,
        "GET {} HTTP/1.1\r\nHost: chorus\r\n{}Connection: close\r\n\r\n",
        target, validator
    )
    .unwrap();
    let mut answer = Vec::new();
    socket.read_to_end(&mut answer).unwrap();
    let split = answer
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("a response head");
    let head = String::from_utf8_lossy(&answer[..split]).to_string();
    let mut lines = head.lines();
    let status = lines.next().unwrap_or("").to_string();
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(n, v)| (n.trim().to_string(), v.trim().to_string()))
        .collect();
    Answer {
        status,
        headers,
        body: answer[split + 4..].to_vec(),
    }
}

#[test]
fn a_groups_artwork_is_served_from_this_origin_with_a_validator() {
    let first = png("first");
    let second = jpeg();
    let origin = Origin::start(vec![
        Served {
            path: "/covers/first.png?size=large",
            content_type: "image/png",
            framing: Framing::Length,
            body: first.clone(),
        },
        Served {
            path: "/covers/second",
            // An origin that does not say what it serves: the bytes decide.
            content_type: "application/octet-stream",
            framing: Framing::UntilClose,
            body: second.clone(),
        },
    ]);
    let house = house(Bounds::default());
    playing(
        &house,
        "First",
        Some(origin.url("/covers/first.png?size=large")),
    );

    let shown = get(&house, "/api/artwork?group=kitchen", None);
    assert_eq!(shown.code(), "200", "{}", shown.text());
    assert_eq!(shown.header("content-type"), Some("image/png"));
    assert_eq!(shown.body, first, "the image's bytes, unchanged");
    assert_eq!(
        shown.header("content-length"),
        Some(first.len().to_string().as_str())
    );
    // Kept by the browser and revalidated before every use.
    assert_eq!(shown.header("cache-control"), Some("no-cache"));
    assert_eq!(shown.header("x-content-type-options"), Some("nosniff"));
    assert_eq!(
        shown.header("content-security-policy"),
        Some(CONTENT_SECURITY_POLICY),
        "the policy is the control page's, unchanged"
    );
    let tag = shown.header("etag").expect("a validator").to_string();
    assert!(tag.starts_with('"') && tag.ends_with('"'), "{tag}");

    // The same artwork, asked for again with the validator: nothing is sent.
    let again = get(&house, "/api/artwork?group=kitchen", Some(&tag));
    assert_eq!(again.code(), "304", "{}", again.text());
    assert!(again.body.is_empty());
    assert_eq!(again.header("etag"), Some(tag.as_str()));

    // A new track: the same request, holding the old validator, is answered
    // with the new artwork and not `304`.
    playing(&house, "Second", Some(origin.url("/covers/second")));
    let next = get(&house, "/api/artwork?group=kitchen", Some(&tag));
    assert_eq!(next.code(), "200", "{}", next.text());
    assert_eq!(next.header("content-type"), Some("image/jpeg"));
    assert_eq!(next.body, second);
    assert_ne!(next.header("etag"), Some(tag.as_str()));

    // The origin was asked for the record's URLs and for nothing else.
    assert_eq!(
        origin.asked(),
        [
            "GET /covers/first.png?size=large HTTP/1.1",
            "GET /covers/first.png?size=large HTTP/1.1",
            "GET /covers/second HTTP/1.1",
        ]
    );
}

#[test]
fn a_group_with_no_record_or_no_artwork_is_not_found() {
    let origin = Origin::start(Vec::new());
    let house = house(Bounds::default());

    // The study plays no player source: it has no now-playing record.
    let none = get(&house, "/api/artwork?group=study", None);
    assert_eq!(none.code(), "404", "{}", none.text());
    assert_eq!(none.header("content-type"), Some("application/json"));
    // The kitchen's player has said nothing yet.
    let quiet = get(&house, "/api/artwork?group=kitchen", None);
    assert_eq!(quiet.code(), "404", "{}", quiet.text());
    // A record with no artwork.
    playing(&house, "Untitled", None);
    assert!(house.state.now_playing("kitchen").is_some());
    let bare = get(&house, "/api/artwork?group=kitchen", None);
    assert_eq!(bare.code(), "404", "{}", bare.text());
    // A group this server does not have: the same answer.
    let nowhere = get(&house, "/api/artwork?group=attic", None);
    assert_eq!(nowhere.code(), "404", "{}", nowhere.text());
    assert_eq!(nowhere.text(), none.text());
    // No group named.
    let unnamed = get(&house, "/api/artwork", None);
    assert_eq!(unnamed.code(), "400", "{}", unnamed.text());
    // The record's artwork is gone at its origin.
    playing(&house, "Gone", Some(origin.url("/nothing-here")));
    let gone = get(&house, "/api/artwork?group=kitchen", None);
    assert_eq!(gone.code(), "502", "{}", gone.text());
    assert_eq!(origin.asked(), ["GET /nothing-here HTTP/1.1"]);
}

#[test]
fn an_upstream_answer_that_is_not_an_image_or_is_too_large_is_refused() {
    let too_large = {
        let mut bytes = png("large");
        bytes.resize(artwork::MAX_ARTWORK_BYTES + 1, 0x5a);
        bytes
    };
    let at_the_bound = {
        let mut bytes = png("bound");
        bytes.resize(artwork::MAX_ARTWORK_BYTES, 0x5a);
        bytes
    };
    let origin = Origin::start(vec![
        Served {
            path: "/page",
            content_type: "text/html",
            framing: Framing::Length,
            body: b"<!doctype html><script>alert(1)</script>".to_vec(),
        },
        Served {
            // Declared an image; it is not one.
            path: "/liar.png",
            content_type: "image/png",
            framing: Framing::Length,
            body: b"<!doctype html><script>alert(1)</script>".to_vec(),
        },
        Served {
            // A real image type this route does not serve: it can carry script.
            path: "/drawing.svg",
            content_type: "image/svg+xml",
            framing: Framing::Length,
            body: b"<svg xmlns=\"http://www.w3.org/2000/svg\"><script>alert(1)</script></svg>"
                .to_vec(),
        },
        Served {
            path: "/empty",
            content_type: "image/png",
            framing: Framing::Length,
            body: Vec::new(),
        },
        Served {
            path: "/large-declared",
            content_type: "image/png",
            framing: Framing::Length,
            body: too_large.clone(),
        },
        Served {
            path: "/large-undeclared",
            content_type: "image/png",
            framing: Framing::UntilClose,
            body: too_large,
        },
        Served {
            path: "/bound",
            content_type: "image/png",
            framing: Framing::Length,
            body: at_the_bound.clone(),
        },
    ]);
    let house = house(Bounds::default());

    for (path, words) in [
        ("/page", "not an image"),
        ("/liar.png", "not an image"),
        ("/drawing.svg", "not an image"),
        ("/empty", "not an image"),
        ("/large-declared", "larger than"),
        ("/large-undeclared", "larger than"),
    ] {
        playing(&house, path, Some(origin.url(path)));
        let refused = get(&house, "/api/artwork?group=kitchen", None);
        assert_eq!(refused.code(), "502", "{path}: {}", refused.text());
        assert_eq!(
            refused.header("content-type"),
            Some("application/json"),
            "{path}"
        );
        assert!(refused.text().contains(words), "{path}: {}", refused.text());
        assert!(
            !refused.text().contains("alert") && refused.body.len() < 1024,
            "{path}: nothing of the upstream body is passed on"
        );
    }
    // The bound is the largest image served, not the smallest refused.
    playing(&house, "bound", Some(origin.url("/bound")));
    let served = get(&house, "/api/artwork?group=kitchen", None);
    assert_eq!(served.code(), "200", "{}", served.text());
    assert_eq!(served.body.len(), artwork::MAX_ARTWORK_BYTES);
    assert!(served.body == at_the_bound);
}

#[test]
fn the_request_carries_no_url_the_client_chose() {
    let cover = png("cover");
    let origin = Origin::start(vec![Served {
        path: "/cover.png",
        content_type: "image/png",
        framing: Framing::Length,
        body: cover.clone(),
    }]);
    // Somewhere a client would like this server to fetch from instead.
    let elsewhere = Origin::start(vec![Served {
        path: "/secret.png",
        content_type: "image/png",
        framing: Framing::Length,
        body: png("secret"),
    }]);
    let house = house(Bounds::default());
    playing(&house, "Cover", Some(origin.url("/cover.png")));
    let wanted = elsewhere.url("/secret.png");

    // A URL beside the group changes nothing: the record's artwork is served.
    for target in [
        format!("/api/artwork?group=kitchen&url={wanted}"),
        format!("/api/artwork?url={wanted}&group=kitchen"),
        format!("/api/artwork?group=kitchen&art_url={wanted}&src={wanted}"),
    ] {
        let shown = get(&house, &target, None);
        assert_eq!(shown.code(), "200", "{target}: {}", shown.text());
        assert_eq!(shown.body, cover, "{target}");
    }
    // A URL in place of the group names no group and fetches nothing.
    let before = origin.asked().len();
    for (target, code) in [
        (format!("/api/artwork?url={wanted}"), "400"),
        (format!("/api/artwork?group={wanted}"), "404"),
        (format!("/api/artwork/{wanted}"), "404"),
        (format!("/api/artwork/kitchen?url={wanted}"), "404"),
    ] {
        let refused = get(&house, &target, None);
        assert_eq!(refused.code(), code, "{target}: {}", refused.text());
    }
    assert_eq!(
        origin.asked().len(),
        before,
        "nothing was fetched for those"
    );
    assert!(
        origin
            .asked()
            .iter()
            .all(|line| line == "GET /cover.png HTTP/1.1"),
        "{:?}",
        origin.asked()
    );
    assert_eq!(
        elsewhere.asked(),
        Vec::<String>::new(),
        "the server never connected to a URL the request carried"
    );
}

#[test]
fn artwork_at_this_servers_own_port_is_refused_by_the_fetch_policy() {
    let house = house(Bounds::default());
    // The record names the control plane itself: with no rule, the route
    // would make the server read its own unauthenticated API.
    playing(
        &house,
        "Loop",
        Some(format!("http://{}/api/state", house.control)),
    );
    let refused = get(&house, "/api/artwork?group=kitchen", None);
    assert_eq!(refused.code(), "502", "{}", refused.text());
    assert!(
        refused.text().contains("the server's own port"),
        "{}",
        refused.text()
    );
    // A scheme the fetcher does not speak never reaches the record at all
    // (`NowPlaying::bounded`), so there is nothing to serve.
    playing(&house, "File", Some("file:///etc/hostname".to_string()));
    let absent = get(&house, "/api/artwork?group=kitchen", None);
    assert_eq!(absent.code(), "404", "{}", absent.text());
}

#[test]
fn an_origin_that_never_answers_is_given_up_at_the_deadline() {
    let origin = Origin::start(Vec::new());
    let deadline = Duration::from_millis(600);
    let house = house(Bounds {
        deadline,
        ..Bounds::default()
    });
    playing(&house, "Stall", Some(origin.url("/stall")));
    let began = Instant::now();
    let late = get(&house, "/api/artwork?group=kitchen", None);
    let took = began.elapsed();
    assert_eq!(late.code(), "504", "{}", late.text());
    // Not a timing claim about the server: only that the answer came because
    // of the deadline (not before it) and long before the fetcher's own 15 s
    // read timeout would have ended the wait.
    assert!(took >= deadline, "{took:?}");
    assert!(took < Duration::from_secs(10), "{took:?}");
    // The worker is back: the control plane still answers.
    let state = get(&house, "/api/state", None);
    assert_eq!(state.code(), "200");
}
