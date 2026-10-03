//! What the chorusctl tests share: the committed control vectors, and a fake
//! server that replays their bytes.
//!
//! The fake is in-process, on loopback, on a port the kernel picks. It reads
//! one whole HTTP request per connection (the head, then `Content-Length`
//! bytes), keeps it, and answers with the status and body the test gave it,
//! the way the real server frames an answer (`Connection: close` and a
//! `Content-Length`).

#![allow(dead_code)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::thread::JoinHandle;

use chorus_ctl::Outcome;

/// The repository root, found from this crate's manifest.
pub fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("this crate lives at crates/<name> under the repository root")
        .to_path_buf()
}

/// The bytes of one vector's message: `fixtures/control/<vector>.json` without
/// its one trailing newline. `vector` is `name` (catalog v1) or `v2/name`.
pub fn vector(vector: &str) -> String {
    let path = repository_root()
        .join("fixtures/control")
        .join(format!("{}.json", vector));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} is committed and readable: {}", path.display(), e));
    text.strip_suffix('\n')
        .unwrap_or_else(|| panic!("{} ends with one newline", path.display()))
        .to_string()
}

/// One value of a vector's `.fields`, or `None` where it has no such line.
pub fn field(vector: &str, key: &str) -> Option<String> {
    let path = repository_root()
        .join("fixtures/control")
        .join(format!("{}.fields", vector));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} is committed and readable: {}", path.display(), e));
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .find(|(k, _)| k.trim() == key)
        .map(|(_, v)| v.trim().to_string())
}

/// A request the fake server received.
#[derive(Debug, Clone)]
pub struct Request {
    /// The request line and the headers.
    pub head: String,
    /// The body.
    pub body: String,
}

/// A fake control plane answering a fixed list of connections.
pub struct Fake {
    /// Where it listens, `127.0.0.1:<port>`.
    pub address: String,
    serving: JoinHandle<Vec<Request>>,
}

fn read_request(connection: &mut TcpStream) -> Request {
    let mut raw = Vec::new();
    let mut scratch = [0u8; 4096];
    loop {
        if let Some(at) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8(raw[..at].to_vec()).expect("a text head");
            let length = head
                .lines()
                .filter_map(|l| l.split_once(':'))
                .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                .map_or(0, |(_, v)| v.trim().parse::<usize>().expect("a length"));
            if raw.len() >= at + 4 + length {
                let body =
                    String::from_utf8(raw[at + 4..at + 4 + length].to_vec()).expect("a text body");
                return Request { head, body };
            }
        }
        let n = connection.read(&mut scratch).expect("the request arrives");
        assert!(n > 0, "the client closed before its request was whole");
        raw.extend_from_slice(&scratch[..n]);
    }
}

/// The bytes the real server frames an answer with.
pub fn framed(status: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nConnection: close\r\n\r\n{}",
        status,
        body.len(),
        body
    )
}

impl Fake {
    /// Serve one connection per entry of `answers`, in order, writing each
    /// entry's bytes as they are after reading the request.
    pub fn raw(answers: Vec<String>) -> Fake {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let address = listener.local_addr().expect("an address").to_string();
        let serving = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for answer in answers {
                let (mut connection, _) = listener.accept().expect("a connection");
                requests.push(read_request(&mut connection));
                connection
                    .write_all(answer.as_bytes())
                    .expect("the answer goes");
            }
            requests
        });
        Fake { address, serving }
    }

    /// Serve one connection per `(status, body)`, framed as the server does.
    pub fn answering(answers: &[(&str, &str)]) -> Fake {
        Fake::raw(answers.iter().map(|(s, b)| framed(s, b)).collect())
    }

    /// Every request received, once every answer has been given.
    pub fn requests(self) -> Vec<Request> {
        self.serving.join().expect("the fake server ran")
    }
}

/// Run chorusctl with `--server <address>` and these arguments.
pub fn ctl(address: &str, args: &[&str]) -> Outcome {
    let mut all = vec!["--server".to_string(), address.to_string()];
    all.extend(args.iter().map(|a| a.to_string()));
    chorus_ctl::run_with(&all, None)
}

/// A loopback address nothing listens on: bound, read, and closed again.
pub fn closed_port() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    listener.local_addr().expect("an address").to_string()
}
