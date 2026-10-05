//! The app under `/app/`, from the real binary over a real socket.
//!
//! `chorus-server` embeds the app's committed build output (`web/dist`) at
//! build time (`build.rs`, `src/app.rs`) and serves it under `/app/`. These
//! checks read `web/dist` from the tree themselves and hold the server to it,
//! file by file, so nothing here lists the app's files: a file a later build
//! adds is checked the day it is committed, and one the server did not pick up
//! fails by name.
//!
//! What is asserted (`docs/decisions/0182-the-app-is-served-under-app.md`):
//! the document at `/app/` under the control page's Content-Security-Policy,
//! unchanged; every file with its bytes, its media type and an `ETag`; `304`
//! for a matching `If-None-Match`; `no-cache` for the document and the service
//! worker's reserved path and `immutable` for a file named by its hash; `404`
//! for a path under `/app/` that names no file.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use chorus_server::app;

/// A loopback address with the port left to the kernel.
const EPHEMERAL: &str = "127.0.0.1:0";

/// The longest any check here waits for something the server owes it.
const PATIENCE: Duration = Duration::from_secs(30);

/// `crates/server/src/control.rs::CONTENT_SECURITY_POLICY`, written out a
/// second time on purpose: serving the app must not change one byte of it
/// unnoticed, and a check that read the constant could not tell. Its
/// `manifest-src 'self'` is the one directive the app added, for its web
/// manifest (`docs/decisions/0190-the-app-installs-behind-the-login.md`).
const CONTENT_SECURITY_POLICY: &str = "default-src 'none'; script-src 'self'; style-src 'self'; \
     connect-src 'self'; img-src 'self' data:; manifest-src 'self'; base-uri 'none'; \
     form-action 'none'; frame-ancestors 'none'";

struct Server {
    child: Child,
    control: String,
    /// What the child says, line by line. Held for the server's whole life so
    /// the pumps carrying its output never stop draining it.
    lines: mpsc::Receiver<String>,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn pump<R: Read + Send + 'static>(stream: R, tx: mpsc::Sender<String>) {
    thread::spawn(move || {
        for line in BufReader::new(stream).lines().map_while(Result::ok) {
            let _ = tx.send(line);
        }
    });
}

/// Start the server on ports the kernel picks and read back where its control
/// channel landed.
fn start() -> Server {
    let mut child = Command::new(env!("CARGO_BIN_EXE_chorus-server"))
        .args([
            "--listen",
            EPHEMERAL,
            "--source",
            "tone",
            "--serve-forever",
            "--allow-non-realtime",
            "--allow-unlocked-memory",
            "--ephemeral-identity",
            "--control-listen",
            EPHEMERAL,
            "--zone",
            "kitchen",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the server binary runs");
    let (tx, lines) = mpsc::channel();
    pump(child.stdout.take().unwrap(), tx.clone());
    pump(child.stderr.take().unwrap(), tx);
    let mut server = Server {
        child,
        control: String::new(),
        lines,
    };
    let mut said = Vec::new();
    let mut control = None;
    let deadline = Instant::now() + PATIENCE;
    while Instant::now() < deadline {
        let Ok(line) = server.lines.recv_timeout(Duration::from_millis(200)) else {
            continue;
        };
        if let Some(at) = line.find("control listening on=") {
            let rest = &line[at + "control listening on=".len()..];
            control = rest.split_whitespace().next().map(str::to_string);
        }
        let ready = line.starts_with("chorus-server: listening on=");
        said.push(line);
        if ready {
            if let Some(control) = control {
                server.control = control;
                return server;
            }
        }
    }
    panic!("the server never said where it is listening: {:?}", said);
}

/// One response: the status line, the headers and the body's bytes.
#[derive(Debug)]
struct Response {
    status: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Response {
    /// The one header of this name. Two of them would be two answers.
    fn header(&self, name: &str) -> Option<&str> {
        let mut found = self
            .headers
            .iter()
            .filter(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str());
        let first = found.next();
        assert!(found.next().is_none(), "{name} is sent twice: {:?}", self);
        first
    }
}

fn exchange(address: &str, request: &str) -> Option<Response> {
    let mut socket = TcpStream::connect(address).expect("the control channel is listening");
    socket.set_read_timeout(Some(PATIENCE)).unwrap();
    socket.set_write_timeout(Some(PATIENCE)).unwrap();
    let _ = socket.write_all(request.as_bytes());
    let mut seen = Vec::new();
    let _ = socket.read_to_end(&mut seen);
    let split = seen.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = String::from_utf8(seen[..split].to_vec()).expect("a response head is text");
    let mut lines = head.split("\r\n");
    let status = lines.next()?.to_string();
    let headers = lines
        .map(|line| {
            let (name, value) = line.split_once(':').expect("a header has a colon");
            (name.trim().to_string(), value.trim().to_string())
        })
        .collect();
    Some(Response {
        status,
        headers,
        body: seen[split + 4..].to_vec(),
    })
}

/// One request, retried past the moment a worker has closed its last
/// connection and not yet handed its slot back (the accept loop answers that
/// `503`), and past nothing else.
fn ask(address: &str, request: &str) -> Response {
    let started = Instant::now();
    loop {
        match exchange(address, request) {
            Some(response) if !response.status.contains("503") || started.elapsed() >= PATIENCE => {
                return response
            }
            None if started.elapsed() >= PATIENCE => panic!("no response to {request:?}"),
            _ => thread::sleep(Duration::from_millis(20)),
        }
    }
}

/// `GET <path>` with `extra` header lines.
fn get(address: &str, path: &str, extra: &str) -> Response {
    ask(
        address,
        &format!("GET {path} HTTP/1.1\r\nHost: {address}\r\n{extra}Connection: close\r\n\r\n"),
    )
}

/// `web/dist` in the tree this test was built from.
fn dist() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/dist")
}

/// Every file under `web/dist`, as the path the server mounts it at (relative
/// to `/app/`) and its bytes, read from the tree and not from the server.
fn committed_output() -> Vec<(String, Vec<u8>)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        for entry in std::fs::read_dir(dir).expect("web/dist is in the tree") {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let relative = path.strip_prefix(root).unwrap();
                let name = relative
                    .iter()
                    .map(|part| part.to_str().unwrap())
                    .collect::<Vec<_>>()
                    .join("/");
                out.push((name, std::fs::read(&path).unwrap()));
            }
        }
    }
    let root = dist();
    let mut out = Vec::new();
    walk(&root, &root, &mut out);
    out.sort();
    out
}

/// What a file of this name must be served as. The server's own table is in
/// `build.rs`; this is the handful the app ships today and the ones the next
/// tasks add, stated a second time so a wrong type there fails here.
fn expected_media_type(path: &str) -> &'static str {
    match path.rsplit_once('.').map(|(_, extension)| extension) {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("webmanifest") => "application/manifest+json",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        other => panic!("{path}: say here what {other:?} is served as"),
    }
}

/// The asset paths a document names in `href="..."` and `src="..."`.
fn named_assets(document: &str) -> Vec<String> {
    let mut found = Vec::new();
    for attribute in ["href=\"", "src=\""] {
        for (at, _) in document.match_indices(attribute) {
            let rest = &document[at + attribute.len()..];
            let value = &rest[..rest.find('"').expect("the attribute closes")];
            if !value.starts_with("data:") {
                found.push(value.to_string());
            }
        }
    }
    found
}

#[test]
fn the_document_is_served_at_app_under_the_unchanged_policy() {
    let server = start();
    let committed = std::fs::read(dist().join("index.html")).expect("web/dist/index.html");

    let page = get(&server.control, "/app/", "");
    assert_eq!(page.status, "HTTP/1.1 200 OK");
    assert_eq!(page.body, committed, "the document is web/dist/index.html");
    assert_eq!(
        page.header("Content-Type"),
        Some("text/html; charset=utf-8")
    );
    assert_eq!(
        page.header("Content-Security-Policy"),
        Some(CONTENT_SECURITY_POLICY)
    );
    assert_eq!(
        page.header("Content-Length"),
        Some(committed.len().to_string().as_str())
    );
    // The same header the control page has always carried, byte for byte.
    let control_page = get(&server.control, "/", "");
    assert_eq!(control_page.status, "HTTP/1.1 200 OK");
    assert_eq!(
        control_page.header("Content-Security-Policy"),
        page.header("Content-Security-Policy")
    );
    assert_ne!(
        control_page.body, page.body,
        "the control page at / is not the app"
    );

    // `/app/index.html` is the same document, and `/app` leads to `/app/`:
    // the document names its assets relative to the directory.
    let by_name = get(&server.control, "/app/index.html", "");
    assert_eq!(by_name.status, "HTTP/1.1 200 OK");
    assert_eq!(by_name.body, committed);
    let bare = get(&server.control, "/app", "");
    assert_eq!(bare.status, "HTTP/1.1 308 Permanent Redirect");
    assert_eq!(bare.header("Location"), Some("/app/"));

    // Under `script-src 'self'; style-src 'self'` the page works only if what
    // it names is served from here: every asset it names answers.
    let document = String::from_utf8(committed).unwrap();
    let assets = named_assets(&document);
    assert!(
        !assets.is_empty(),
        "the document names its script and style"
    );
    for asset in assets {
        let response = get(&server.control, &format!("/app/{asset}"), "");
        assert_eq!(response.status, "HTTP/1.1 200 OK", "{asset}");
    }
}

#[test]
fn every_file_of_the_output_is_served_with_its_type_its_tag_and_its_cache_rule() {
    let server = start();
    let committed = committed_output();
    assert!(
        committed.iter().any(|(path, _)| path == "index.html"),
        "web/dist has a document"
    );
    assert!(
        committed
            .iter()
            .any(|(path, _)| path.starts_with("assets/")),
        "web/dist has assets"
    );
    // The table in the binary is the directory in the tree: nothing missing,
    // nothing left over. This is what "no route written by hand" means.
    let embedded: Vec<&str> = app::files().iter().map(|file| file.path).collect();
    let on_disk: Vec<&str> = committed.iter().map(|(path, _)| path.as_str()).collect();
    assert_eq!(embedded, on_disk);

    let mut tags = Vec::new();
    for (path, bytes) in &committed {
        let url = format!("/app/{path}");
        let response = get(&server.control, &url, "");
        assert_eq!(response.status, "HTTP/1.1 200 OK", "{url}");
        assert_eq!(&response.body, bytes, "{url}: the committed bytes");
        assert_eq!(
            response.header("Content-Type"),
            Some(expected_media_type(path)),
            "{url}"
        );
        assert_eq!(
            response.header("Content-Security-Policy"),
            Some(CONTENT_SECURITY_POLICY),
            "{url}"
        );

        let tag = response
            .header("ETag")
            .unwrap_or_else(|| panic!("{url}: no ETag"))
            .to_string();
        assert!(
            tag.len() > 2 && tag.starts_with('"') && tag.ends_with('"'),
            "{url}: a strong, quoted tag, not {tag}"
        );
        tags.push(tag.clone());

        // The cache rule: a file named by its hash never changes, everything
        // else is revalidated before every use.
        let cache = response.header("Cache-Control").expect("a cache rule");
        let hashed = path.starts_with("assets/");
        if path == "index.html" || path == app::SERVICE_WORKER_PATH {
            assert_eq!(cache, "no-cache", "{url}");
        } else if hashed {
            assert_eq!(cache, "public, max-age=31536000, immutable", "{url}");
            let stem = path.rsplit_once('.').unwrap().0;
            let hash = stem.rsplit_once('-').expect("a hashed name").1;
            assert_eq!(hash.len(), 8, "{url}: named by its hash");
        } else {
            assert_eq!(cache, "no-cache", "{url}");
        }

        // The validator: the tag back means "not modified", with no body and
        // the same tag and rule; any other tag means the file again.
        for header in [
            format!("If-None-Match: {tag}\r\n"),
            format!("If-None-Match: \"stale\", W/{tag}\r\n"),
        ] {
            let again = get(&server.control, &url, &header);
            assert_eq!(again.status, "HTTP/1.1 304 Not Modified", "{url} {header}");
            assert!(again.body.is_empty(), "{url}: a 304 has no body");
            assert_eq!(again.header("ETag"), Some(tag.as_str()), "{url}");
            assert_eq!(again.header("Cache-Control"), Some(cache), "{url}");
        }
        let changed = get(&server.control, &url, "If-None-Match: \"stale\"\r\n");
        assert_eq!(changed.status, "HTTP/1.1 200 OK", "{url}");
        assert_eq!(&changed.body, bytes, "{url}");
    }
    // A tag is of the bytes: different files, different tags.
    let mut distinct = tags.clone();
    distinct.sort();
    distinct.dedup();
    assert_eq!(
        distinct.len(),
        tags.len(),
        "two files share a tag: {tags:?}"
    );
}

#[test]
fn the_service_workers_reserved_path_is_never_immutable() {
    // The path and its rule were fixed before the worker existed, so that the
    // build that added it could not ship it immutable.
    assert_eq!(app::SERVICE_WORKER_PATH, "sw.js");
    // The worker exists now (`web/src/sw.js`, built to `web/dist/sw.js`), and
    // the server answers for it as a script that is revalidated before every
    // use: a browser must always be able to fetch a new one. The same holds
    // for the manifest an installed app is read from, which no hash names.
    let server = start();
    for (path, media_type) in [
        ("sw.js", "text/javascript; charset=utf-8"),
        ("manifest.webmanifest", "application/manifest+json"),
    ] {
        assert!(
            app::files().iter().any(|file| file.path == path),
            "web/dist has {path}"
        );
        let url = format!("/app/{path}");
        let response = get(&server.control, &url, "");
        assert_eq!(response.status, "HTTP/1.1 200 OK", "{url}");
        assert_eq!(response.header("Cache-Control"), Some("no-cache"), "{url}");
        assert_eq!(response.header("Content-Type"), Some(media_type), "{url}");
        // The policy lets the page load its manifest and its worker:
        // `manifest-src` for the one, `script-src` (which `worker-src` falls
        // back to) for the other.
        let policy = response.header("Content-Security-Policy").expect("a policy");
        assert!(policy.contains("manifest-src 'self'"), "{url}: {policy}");
        assert!(policy.contains("script-src 'self'"), "{url}: {policy}");
    }
    assert_eq!(app::cache_control(app::SERVICE_WORKER_PATH), "no-cache");
    assert_eq!(app::cache_control(app::DOCUMENT_PATH), "no-cache");
    assert_eq!(
        app::cache_control("assets/main-OC5E6PTU.js"),
        "public, max-age=31536000, immutable"
    );
    assert_eq!(app::cache_control("assets/sw.js"), "no-cache");
}

#[test]
fn a_path_under_app_that_names_no_file_is_a_404() {
    let server = start();
    for path in [
        "/app/nothing.js",
        "/app/assets/",
        "/app/assets/main-AAAAAAAA.js",
        "/app/index.html/",
        "/app//",
        "/app/../Cargo.toml",
        "/app/%2e%2e/Cargo.toml",
        "/application",
    ] {
        let response = get(&server.control, path, "");
        assert_eq!(response.status, "HTTP/1.1 404 Not Found", "{path}");
        assert!(response.header("ETag").is_none(), "{path}");
    }
    // Only a GET reads the app.
    let posted = ask(
        &server.control,
        &format!(
            "POST /app/ HTTP/1.1\r\nHost: {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            server.control
        ),
    );
    assert_eq!(posted.status, "HTTP/1.1 404 Not Found");
}
