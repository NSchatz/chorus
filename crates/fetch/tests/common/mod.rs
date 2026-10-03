//! What the integration tests share: a small HTTP server on a loopback
//! ephemeral port, run on threads inside the test, optionally under TLS; a
//! policy that allows loopback; and patterned bytes to serve.
//!
//! The server is not a model of a good server. It reads one request head,
//! hands it to the test's handler with the connection, and closes. It counts
//! connections accepted and request heads read, so a test can show that
//! nothing was fetched from a refused address.

#![allow(dead_code)]

pub mod pki;

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chorus_fetch::{FetchError, Policy, Stream};

/// One request as the server read it.
#[derive(Debug, Clone)]
pub struct Request {
    /// The request target: path and query.
    pub target: String,
    /// The whole head, as sent.
    pub raw: String,
    /// When the head had been read, on the monotonic clock.
    pub at: Instant,
}

impl Request {
    /// The value of a header, by case-insensitive name.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.raw.lines().skip(1).find_map(|line| {
            let (n, v) = line.split_once(':')?;
            n.eq_ignore_ascii_case(name).then_some(v.trim())
        })
    }
}

type Handler = dyn Fn(&Request, &mut dyn Write) + Send + Sync;

/// A server on `127.0.0.1` at a port the system chose.
pub struct Server {
    pub addr: SocketAddr,
    connections: Arc<AtomicUsize>,
    requests: Arc<Mutex<Vec<Request>>>,
    stop: Arc<AtomicBool>,
}

impl Server {
    /// A plain HTTP server answering with `handler`.
    pub fn start(handler: impl Fn(&Request, &mut dyn Write) + Send + Sync + 'static) -> Server {
        Server::start_with(None, Arc::new(handler))
    }

    /// The same under TLS, with the server's certificate chain and key.
    pub fn start_tls(
        config: Arc<rustls::ServerConfig>,
        handler: impl Fn(&Request, &mut dyn Write) + Send + Sync + 'static,
    ) -> Server {
        Server::start_with(Some(config), Arc::new(handler))
    }

    fn start_with(tls: Option<Arc<rustls::ServerConfig>>, handler: Arc<Handler>) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let addr = listener.local_addr().expect("the listener's address");
        let connections = Arc::new(AtomicUsize::new(0));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (count, log, stopped) = (
            Arc::clone(&connections),
            Arc::clone(&requests),
            Arc::clone(&stop),
        );
        thread::spawn(move || {
            for sock in listener.incoming() {
                if stopped.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(sock) = sock else { continue };
                count.fetch_add(1, Ordering::SeqCst);
                let (handler, log, tls) = (Arc::clone(&handler), Arc::clone(&log), tls.clone());
                thread::spawn(move || serve(sock, tls, &*handler, &log));
            }
        });
        Server {
            addr,
            connections,
            requests,
            stop,
        }
    }

    /// `http://127.0.0.1:<port><path>`.
    pub fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }

    /// `https://127.0.0.1:<port><path>`.
    pub fn https_url(&self, path: &str) -> String {
        format!("https://{}{}", self.addr, path)
    }

    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// How many connections were accepted.
    pub fn connections(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }

    /// Every request head read so far, in order.
    pub fn requests(&self) -> Vec<Request> {
        self.requests.lock().expect("the request log").clone()
    }

    /// The targets of every request so far.
    pub fn targets(&self) -> Vec<String> {
        self.requests().into_iter().map(|r| r.target).collect()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // Wake the accept loop so its thread ends. This is the server's own
        // bookkeeping, so it is not counted by tests that read `connections`
        // (they read it before the server is dropped).
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect_timeout(&self.addr, Duration::from_millis(200));
    }
}

fn read_head(conn: &mut dyn Read) -> io::Result<String> {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if conn.read(&mut byte)? == 0 {
            return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
        }
        head.push(byte[0]);
        if head.len() > 64 * 1024 {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
    }
    Ok(String::from_utf8_lossy(&head).into_owned())
}

fn serve(
    sock: TcpStream,
    tls: Option<Arc<rustls::ServerConfig>>,
    handler: &Handler,
    log: &Mutex<Vec<Request>>,
) {
    let _ = sock.set_read_timeout(Some(Duration::from_secs(10)));
    let _ = sock.set_nodelay(true);
    let record = |raw: String| {
        let target = raw.split(' ').nth(1).unwrap_or("").to_string();
        let request = Request {
            target,
            raw,
            at: Instant::now(),
        };
        log.lock().expect("the request log").push(request.clone());
        request
    };
    match tls {
        None => {
            let mut sock = sock;
            let Ok(raw) = read_head(&mut sock) else {
                return;
            };
            let request = record(raw);
            handler(&request, &mut sock);
            let _ = sock.flush();
        }
        Some(config) => {
            let Ok(conn) = rustls::ServerConnection::new(config) else {
                return;
            };
            let mut tls = rustls::StreamOwned::new(conn, sock);
            // A client that refuses the certificate ends the handshake here,
            // before any request is read.
            let Ok(raw) = read_head(&mut tls) else { return };
            let request = record(raw);
            handler(&request, &mut tls);
            tls.conn.send_close_notify();
            let _ = tls.flush();
        }
    }
}

/// The policy the tests fetch under: loopback allowed, short bounds.
pub fn policy() -> Policy {
    Policy {
        allow_loopback: true,
        denied_ports_on_self: Vec::new(),
        max_redirects: 5,
        connect_timeout: Duration::from_secs(5),
        read_timeout: Duration::from_secs(5),
        max_header_bytes: 8 * 1024,
        ca_bundle: None,
    }
}

/// `len` bytes that differ by position, so a wrong offset shows.
pub fn pattern(len: usize, seed: u8) -> Vec<u8> {
    (0..len)
        .map(|i| {
            (i as u8)
                .wrapping_mul(31)
                .wrapping_add((i >> 8) as u8)
                .wrapping_add(seed)
        })
        .collect()
}

/// A `200 OK` with a `Content-Length` and the body.
pub fn respond(out: &mut dyn Write, content_type: &str, body: &[u8]) {
    let _ = write!(
        out,
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = out.write_all(body);
}

/// A redirect with that status to that `Location`.
pub fn redirect(out: &mut dyn Write, status: u16, location: &str) {
    let _ = write!(
        out,
        "HTTP/1.1 {status} Moved\r\nLocation: {location}\r\nContent-Length: 0\r\n\r\n"
    );
}

/// A bare status with no body.
pub fn status(out: &mut dyn Write, status: u16) {
    let _ = write!(out, "HTTP/1.1 {status} Status\r\nContent-Length: 0\r\n\r\n");
}

/// Reads a stream to its end.
pub fn read_all(stream: &mut Stream) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    stream.read_to_end(&mut out)?;
    Ok(out)
}

/// The words of a refusal, or a panic saying what came instead.
pub fn refused(result: Result<Stream, FetchError>) -> String {
    match result {
        Err(FetchError::Refused(rule)) => rule,
        Err(other) => panic!("expected a refusal, got {other:?}"),
        Ok(_) => panic!("expected a refusal, got a stream"),
    }
}

/// The words of an unsupported-stream error, or a panic.
pub fn unsupported(result: Result<Stream, FetchError>) -> String {
    match result {
        Err(FetchError::Unsupported(what)) => what,
        Err(other) => panic!("expected unsupported, got {other:?}"),
        Ok(_) => panic!("expected unsupported, got a stream"),
    }
}
