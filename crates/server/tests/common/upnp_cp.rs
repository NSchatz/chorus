//! The scripted control point's own tools, shared by the two test files that
//! drive the real server binary as a control point would
//! (`upnp_control_point.rs` for UPnP AV, `openhome_control_point.rs` for the
//! OpenHome services): raw HTTP, an in-test media server, signal builders, a
//! room's captured audio, a GENA event listener, and `Home`, a running
//! server with `--upnp` and the control point's sockets.
//!
//! Each of the two includes this file as a module beside `common`
//! (`#[path = "common/upnp_cp.rs"] mod cp;`); it is not part of `common`
//! itself, so the other test binaries do not compile it.

#![allow(dead_code)]

use std::collections::HashMap;
use std::io::{Cursor, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chorus_decode::{Decoder, Hint, Resampler};
use chorus_upnp::client::{self, LastChange, SoapReply, SsdpKind, SsdpMessage};
use chorus_upnp::xml::escape_text;
use chorus_upnp::{soap, Headers, Service};

use crate::common::{self, Player as Listener, RunningServer};

pub const AVT: Service = Service::AvTransport;
pub const RCS: Service = Service::RenderingControl;
pub const CM: Service = Service::ConnectionManager;
pub const DEVICE: &str = "urn:schemas-upnp-org:device:MediaRenderer:1";
pub const LIMIT: Duration = Duration::from_secs(30);

pub type Frame = [i16; 2];
pub const SILENCE: Frame = [0, 0];

pub fn wait<T>(what: &str, mut done: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + LIMIT;
    loop {
        if let Some(v) = done() {
            return v;
        }
        assert!(Instant::now() < deadline, "{what}: not within {LIMIT:?}");
        thread::sleep(Duration::from_millis(5));
    }
}

// ----- HTTP, as a control point speaks it --------------------------------------

pub struct Answer {
    pub start: String,
    pub status: u16,
    pub headers: Headers,
    pub body: String,
}

/// One request on a fresh connection, read to the close.
pub fn exchange(address: &str, request: &[u8]) -> Answer {
    let mut socket = TcpStream::connect(address).expect("the renderers listen");
    socket
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    socket.write_all(request).unwrap();
    let mut raw = Vec::new();
    socket
        .read_to_end(&mut raw)
        .expect("the server answers and closes");
    let text = String::from_utf8_lossy(&raw).to_string();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let (start, headers) = Headers::parse(head);
    Answer {
        start: start.to_string(),
        status: start
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0),
        headers,
        body: body.to_string(),
    }
}

pub fn get(address: &str, path: &str) -> Answer {
    exchange(
        address,
        format!("GET {path} HTTP/1.1\r\nHOST: {address}\r\n\r\n").as_bytes(),
    )
}

pub fn post(address: &str, path: &str, soapaction: &str, body: &str) -> Answer {
    exchange(
        address,
        format!(
            "POST {path} HTTP/1.1\r\nHOST: {address}\r\nCONTENT-TYPE: {}\r\nSOAPACTION: \
             {soapaction}\r\nCONTENT-LENGTH: {}\r\n\r\n{body}",
            soap::CONTENT_TYPE,
            body.len()
        )
        .as_bytes(),
    )
}

// ----- the media server ----------------------------------------------------------

#[derive(Clone)]
pub enum Route {
    File(Arc<Vec<u8>>, String),
    Redirect(String),
}

/// An HTTP server on loopback serving what the test put in it, with range
/// requests honoured.
pub struct MediaServer {
    pub address: String,
    pub accepts: Arc<AtomicUsize>,
    pub routes: Arc<Mutex<HashMap<String, Route>>>,
}

impl MediaServer {
    pub fn start() -> MediaServer {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let accepts = Arc::new(AtomicUsize::new(0));
        let routes: Arc<Mutex<HashMap<String, Route>>> = Arc::default();
        {
            let (accepts, routes) = (Arc::clone(&accepts), Arc::clone(&routes));
            thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(stream) = stream else { return };
                    accepts.fetch_add(1, Ordering::SeqCst);
                    let routes = Arc::clone(&routes);
                    thread::spawn(move || serve_media(stream, &routes));
                }
            });
        }
        MediaServer {
            address,
            accepts,
            routes,
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.address, path)
    }

    pub fn serve(&self, path: &str, body: Vec<u8>, content_type: &str) -> String {
        self.routes.lock().unwrap().insert(
            path.to_string(),
            Route::File(Arc::new(body), content_type.to_string()),
        );
        self.url(path)
    }

    pub fn redirect(&self, path: &str, to: &str) -> String {
        self.routes
            .lock()
            .unwrap()
            .insert(path.to_string(), Route::Redirect(to.to_string()));
        self.url(path)
    }

    pub fn accepts(&self) -> usize {
        self.accepts.load(Ordering::SeqCst)
    }
}

pub fn serve_media(mut stream: TcpStream, routes: &Mutex<HashMap<String, Route>>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        match stream.read(&mut byte) {
            Ok(1) => head.push(byte[0]),
            _ => return,
        }
    }
    let head = String::from_utf8_lossy(&head).to_string();
    let path = head.split(' ').nth(1).unwrap_or("/").to_string();
    let range = head.lines().find_map(|l| {
        let (k, v) = l.split_once(':')?;
        k.eq_ignore_ascii_case("range")
            .then(|| v.trim().to_string())
    });
    let route = routes.lock().unwrap().get(&path).cloned();
    match route {
        None => {
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
        }
        Some(Route::Redirect(to)) => {
            let _ = stream.write_all(
                format!("HTTP/1.1 302 Found\r\nLocation: {to}\r\nContent-Length: 0\r\n\r\n")
                    .as_bytes(),
            );
        }
        Some(Route::File(body, content_type)) => {
            let len = body.len();
            let from = range
                .and_then(|r| r.strip_prefix("bytes=").map(str::to_string))
                .and_then(|r| r.split('-').next().and_then(|n| n.parse::<usize>().ok()))
                .filter(|f| *f < len);
            let head = match from {
                Some(from) => format!(
                    "HTTP/1.1 206 Partial Content\r\nContent-Type: {content_type}\r\n\
                     Accept-Ranges: bytes\r\nContent-Range: bytes {from}-{}/{len}\r\n\
                     Content-Length: {}\r\n\r\n",
                    len - 1,
                    len - from
                ),
                None => format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nAccept-Ranges: bytes\r\n\
                     Content-Length: {len}\r\n\r\n"
                ),
            };
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body[from.unwrap_or(0)..]);
        }
    }
}

// ----- media ---------------------------------------------------------------------

pub fn fixture(name: &str) -> Vec<u8> {
    let path = format!(
        "{}/../../fixtures/decode/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// Frame `n` of the generated signal: never silence, and no two alike.
pub fn ramp_frame(n: usize) -> Frame {
    [1 + (n % 30_011) as i16, -(1 + (n / 30_011) as i16)]
}

pub fn ramp(from: usize, frames: usize) -> Vec<Frame> {
    (from..from + frames).map(ramp_frame).collect()
}

/// A 16-bit stereo WAV file of `frames`.
pub fn wav(rate: u32, frames: &[Frame]) -> Vec<u8> {
    let data = (frames.len() * 4) as u32;
    let mut out = Vec::with_capacity(44 + frames.len() * 4);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for f in frames {
        out.extend_from_slice(&f[0].to_le_bytes());
        out.extend_from_slice(&f[1].to_le_bytes());
    }
    out
}

/// Raw big-endian 16-bit PCM, what `audio/L16` is.
pub fn l16(frames: &[Frame]) -> Vec<u8> {
    frames
        .iter()
        .flat_map(|f| [f[0].to_be_bytes(), f[1].to_be_bytes()].concat())
        .collect()
}

/// How many frames the server's decoder and resampler make of `bytes` at
/// 48 kHz.
pub fn frames_at_48k(bytes: &[u8], extension: &str) -> usize {
    let hint = Hint {
        mime: None,
        extension: Some(extension.to_string()),
    };
    let mut decoder = Decoder::open(Box::new(Cursor::new(bytes.to_vec())), &hint).expect("decodes");
    let rate = decoder.format().rate;
    let mut pcm = Vec::new();
    while decoder.read(&mut pcm).expect("decodes") > 0 {}
    if rate == 48_000 {
        return pcm.len() / 2;
    }
    let mut resampler = Resampler::new(rate, 48_000, 2);
    let mut out = Vec::new();
    resampler.process(&pcm, &mut out);
    resampler.flush(&mut out);
    out.len() / 2
}

/// A DIDL-Lite document for one audio item.
pub fn didl(
    title: &str,
    artist: &str,
    album: &str,
    url: &str,
    mime: &str,
    duration: &str,
) -> String {
    format!(
        concat!(
            r#"<DIDL-Lite xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/" "#,
            r#"xmlns:dc="http://purl.org/dc/elements/1.1/" "#,
            r#"xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/">"#,
            r#"<item id="1" parentID="0" restricted="1"><dc:title>{}</dc:title>"#,
            r#"<upnp:artist>{}</upnp:artist><upnp:album>{}</upnp:album>"#,
            r#"<upnp:albumArtURI>http://192.0.2.10:8200/art/cover.jpg</upnp:albumArtURI>"#,
            r#"<upnp:class>object.item.audioItem.musicTrack</upnp:class>"#,
            r#"<res protocolInfo="http-get:*:{}:*" duration="{}">{}</res></item></DIDL-Lite>"#
        ),
        escape_text(title),
        escape_text(artist),
        escape_text(album),
        mime,
        duration,
        escape_text(url)
    )
}

// ----- what a room plays, captured -----------------------------------------------

/// One chunk as received: its sequence, its timestamp on the server's
/// timeline and its PCM.
pub type Chunk = (u32, u64, Vec<u8>);

/// One frame as received: its chunk's sequence and timestamp, its index in
/// the chunk, and the frame.
pub type Placed = (u32, u64, usize, Frame);

/// Everything a listener received, chunk by chunk.
#[derive(Clone, Default)]
pub struct Heard(pub Arc<Mutex<Vec<Chunk>>>);

impl Heard {
    pub fn from(mut listener: Listener, keep: Arc<AtomicBool>) -> Heard {
        let heard = Heard::default();
        let into = heard.clone();
        thread::spawn(move || {
            while keep.load(Ordering::SeqCst) {
                if let Some(chunk) = listener.next_chunk(Duration::from_millis(100)) {
                    into.0.lock().unwrap().push((
                        chunk.sequence,
                        chunk.timestamp_ns,
                        chunk.audio_data,
                    ));
                }
            }
        });
        heard
    }

    pub fn chunks(&self) -> usize {
        self.0.lock().unwrap().len()
    }

    /// Every frame received from chunk `from` on, with the sequence of the
    /// chunk it came in.
    pub fn frames_since(&self, from: usize) -> Vec<(u32, Frame)> {
        self.placed_since(from)
            .into_iter()
            .map(|(sequence, _, _, frame)| (sequence, frame))
            .collect()
    }

    /// Every frame received from chunk `from` on, with where the server put
    /// it: the sequence and the timestamp of its chunk, and its index inside
    /// that chunk.
    pub fn placed_since(&self, from: usize) -> Vec<Placed> {
        let chunks = self.0.lock().unwrap();
        let mut out = Vec::new();
        for (sequence, timestamp_ns, data) in chunks.iter().skip(from) {
            for (index, f) in data.as_chunks::<4>().0.iter().enumerate() {
                out.push((
                    *sequence,
                    *timestamp_ns,
                    index,
                    [
                        i16::from_le_bytes([f[0], f[1]]),
                        i16::from_le_bytes([f[2], f[3]]),
                    ],
                ));
            }
        }
        out
    }

    pub fn loud_since(&self, from: usize) -> usize {
        self.frames_since(from)
            .iter()
            .filter(|(_, f)| *f != SILENCE)
            .count()
    }
}

// ----- the control point's event listener ----------------------------------------

#[derive(Clone, Debug)]
pub struct Notified {
    pub path: String,
    pub sid: String,
    pub seq: u32,
    pub at: Instant,
    pub vars: Vec<(String, String)>,
}

impl Notified {
    pub fn last_change(&self) -> Option<LastChange> {
        self.vars
            .iter()
            .find(|(n, _)| n == "LastChange")
            .map(|(_, v)| client::parse_last_change(v).expect("a LastChange document"))
    }

    pub fn var(&self, name: &str) -> Option<String> {
        self.last_change()?.get(name).map(str::to_string)
    }
}

/// The NOTIFY listener of the control point.
pub struct Events {
    pub address: String,
    pub log: Arc<Mutex<Vec<Notified>>>,
}

impl Events {
    pub fn start() -> Events {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let log: Arc<Mutex<Vec<Notified>>> = Arc::default();
        {
            let log = Arc::clone(&log);
            thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else { return };
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                    let mut raw = Vec::new();
                    let mut scratch = [0u8; 4096];
                    let (head_end, length) = loop {
                        if let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                            let head = String::from_utf8_lossy(&raw[..end]).to_string();
                            let (_, headers) = Headers::parse(&head);
                            let length: usize = headers
                                .get("CONTENT-LENGTH")
                                .and_then(|l| l.parse().ok())
                                .unwrap_or(0);
                            break (end, length);
                        }
                        match stream.read(&mut scratch) {
                            Ok(n) if n > 0 => raw.extend_from_slice(&scratch[..n]),
                            _ => break (0, 0),
                        }
                    };
                    if head_end == 0 {
                        continue;
                    }
                    while raw.len() < head_end + 4 + length {
                        match stream.read(&mut scratch) {
                            Ok(n) if n > 0 => raw.extend_from_slice(&scratch[..n]),
                            _ => break,
                        }
                    }
                    let at = Instant::now();
                    let head = String::from_utf8_lossy(&raw[..head_end]).to_string();
                    let (start, headers) = Headers::parse(&head);
                    let body = String::from_utf8_lossy(&raw[head_end + 4..]).to_string();
                    assert!(start.starts_with("NOTIFY "), "{start}");
                    assert_eq!(headers.get("NT"), Some("upnp:event"));
                    assert_eq!(headers.get("NTS"), Some("upnp:propchange"));
                    log.lock().unwrap().push(Notified {
                        path: start.split(' ').nth(1).unwrap_or("").to_string(),
                        sid: headers.get("SID").unwrap_or("").to_string(),
                        seq: headers
                            .get("SEQ")
                            .and_then(|s| s.parse().ok())
                            .expect("a SEQ"),
                        at,
                        vars: client::parse_propertyset(&body).expect("a propertyset"),
                    });
                    let _ = stream.write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    );
                }
            });
        }
        Events { address, log }
    }

    pub fn callback(&self, label: &str) -> String {
        format!("http://{}/{}", self.address, label)
    }

    pub fn of(&self, sid: &str) -> Vec<Notified> {
        self.log
            .lock()
            .unwrap()
            .iter()
            .filter(|n| n.sid == sid)
            .cloned()
            .collect()
    }

    /// Wait until an event of `sid` from index `from` on carries `name` =
    /// `value`; the index after it.
    pub fn until(&self, sid: &str, from: usize, name: &str, value: &str) -> usize {
        wait(&format!("an event with {name}={value}"), || {
            self.of(sid)
                .iter()
                .enumerate()
                .skip(from)
                .find(|(_, n)| n.var(name).as_deref() == Some(value))
                .map(|(i, _)| i + 1)
        })
    }
}

// ----- the house -----------------------------------------------------------------

pub static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

/// One renderer as the control point found it.
#[derive(Clone, Debug)]
pub struct Device {
    pub udn: String,
    pub name: String,
    pub model: String,
    pub boot_id: u32,
    pub config_id: u32,
    pub location: String,
}

impl Device {
    pub fn path(&self, service: Service, leaf: &str) -> String {
        format!("/upnp/{}/{}/{}", self.udn, service.path(), leaf)
    }
}

/// A running server with `--upnp`, and the control point's sockets.
pub struct Home {
    pub server: RunningServer,
    pub http: String,
    pub ssdp: SocketAddr,
    pub notify: UdpSocket,
    pub dir: PathBuf,
    pub keep: Arc<AtomicBool>,
    pub notes: Vec<(SsdpMessage, Instant)>,
}

impl Drop for Home {
    fn drop(&mut self) {
        self.keep.store(false, Ordering::SeqCst);
    }
}

pub fn fresh_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "chorus-upnp-{}-{}",
        std::process::id(),
        NEXT_DIR.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

pub fn field(line: &str, key: &str) -> String {
    let at = line
        .find(key)
        .unwrap_or_else(|| panic!("no {key} in {line}"));
    line[at + key.len()..]
        .split_whitespace()
        .next()
        .unwrap()
        .to_string()
}

impl Home {
    pub fn start(rooms: &[&str]) -> Home {
        Home::start_with(rooms, &["--media-allow-loopback"])
    }

    /// A server with the OpenHome services on every renderer (the default
    /// of `--upnp`, said out loud).
    pub fn start_openhome(rooms: &[&str], extra: &[&str]) -> Home {
        let mut all = vec!["--media-allow-loopback", "--upnp-openhome", "on"];
        all.extend_from_slice(extra);
        Home::start_with(rooms, &all)
    }

    pub fn start_with(rooms: &[&str], extra: &[&str]) -> Home {
        let notify = UdpSocket::bind("127.0.0.1:0").unwrap();
        Home::start_in(fresh_dir(), notify, rooms, extra)
    }

    /// Start (or start again) on the identity and the state kept in `dir`.
    pub fn start_in(dir: PathBuf, notify: UdpSocket, rooms: &[&str], extra: &[&str]) -> Home {
        notify
            .set_read_timeout(Some(Duration::from_millis(50)))
            .unwrap();
        let group = notify.local_addr().unwrap().to_string();
        let state_file = dir.join("state").display().to_string();
        let mut args: Vec<String> = [
            "--state-file",
            &state_file,
            "--slots",
            "4",
            "--players",
            "2",
            "--upnp",
            "--upnp-listen",
            "127.0.0.1:0",
            "--upnp-ssdp-port",
            "0",
            "--upnp-ssdp-group",
            &group,
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        for room in rooms {
            args.extend(["--zone".to_string(), room.to_string()]);
        }
        args.extend(extra.iter().map(|s| s.to_string()));
        // The UPnP AV half alone, unless the caller says which it wants.
        if !extra.contains(&"--upnp-openhome") {
            args.extend(["--upnp-openhome".to_string(), "off".to_string()]);
        }
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let identity = dir.display().to_string();
        let mut server =
            RunningServer::start_on("127.0.0.1:0", &["--identity-dir", &identity], &args);
        let line = server.wait_for("upnp renderers listening on=");
        let http = field(&line, "listening on=");
        let port: u16 = field(&line, "ssdp_port=").parse().unwrap();
        Home {
            server,
            http,
            ssdp: SocketAddr::from(([127, 0, 0, 1], port)),
            notify,
            dir,
            keep: Arc::new(AtomicBool::new(true)),
            notes: Vec::new(),
        }
    }

    /// Take in the discovery notifications that arrive within `quiet` of the
    /// last one, and give back all so far.
    pub fn notifications(&mut self, quiet: Duration) -> &[(SsdpMessage, Instant)] {
        let mut buffer = [0u8; 2048];
        let mut last = Instant::now();
        while last.elapsed() < quiet {
            if let Ok((n, _)) = self.notify.recv_from(&mut buffer) {
                let message = client::parse_ssdp(&buffer[..n]).expect("a NOTIFY");
                self.notes.push((message, Instant::now()));
                last = Instant::now();
            }
        }
        &self.notes
    }

    /// One search; every response that arrives within `listen`, with how
    /// long after the search it came. `head` is the whole datagram.
    pub fn search_raw(&self, head: &str, listen: Duration) -> Vec<(SsdpMessage, Duration)> {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket
            .set_read_timeout(Some(Duration::from_millis(50)))
            .unwrap();
        socket.send_to(head.as_bytes(), self.ssdp).unwrap();
        let sent = Instant::now();
        let mut out = Vec::new();
        let mut buffer = [0u8; 2048];
        while sent.elapsed() < listen {
            if let Ok((n, _)) = socket.recv_from(&mut buffer) {
                let message = client::parse_ssdp(&buffer[..n]).expect("a search response");
                assert_eq!(message.kind, SsdpKind::SearchResponse);
                out.push((message, sent.elapsed()));
            }
        }
        out
    }

    pub fn search(&self, st: &str, mx: u32, listen: Duration) -> Vec<(SsdpMessage, Duration)> {
        self.search_raw(&client::msearch(st, mx), listen)
    }

    /// Every renderer, found the way a control point finds them: a search
    /// for the device type, then each LOCATION's description.
    pub fn devices(&self) -> Vec<Device> {
        let found = self.search(DEVICE, 1, Duration::from_millis(1100));
        found
            .iter()
            .filter_map(|(m, _)| {
                let location = m.location.clone().expect("a LOCATION");
                let path = location
                    .strip_prefix(&format!("http://{}", self.http))
                    .unwrap_or_else(|| panic!("{location} is not on {}", self.http));
                let answer = get(&self.http, path);
                if answer.status == 404 {
                    // Gone between the search and the GET: not a device.
                    return None;
                }
                assert_eq!(answer.status, 200, "{location}");
                let d = client::parse_description(&answer.body).expect("a description");
                Some(Device {
                    udn: d.udn.trim_start_matches("uuid:").to_string(),
                    name: d.friendly_name,
                    model: d.model_name,
                    boot_id: m.boot_id.expect("a BOOTID"),
                    config_id: m.config_id.expect("a CONFIGID"),
                    location,
                })
            })
            .collect()
    }

    pub fn device(&self, name: &str) -> Device {
        let devices = self.devices();
        devices
            .iter()
            .find(|d| d.name == name)
            .unwrap_or_else(|| panic!("no renderer called {name} among {devices:?}"))
            .clone()
    }

    pub fn call(
        &self,
        device: &Device,
        service: Service,
        action: &str,
        arguments: &[(&str, &str)],
    ) -> SoapReply {
        let call = client::soap_request(service, action, arguments).expect("a request");
        let answer = post(
            &self.http,
            &device.path(service, "control"),
            &call.soapaction,
            &call.body,
        );
        let reply = client::parse_soap_reply(&answer.body)
            .unwrap_or_else(|e| panic!("{action}: {e:?}: {} {}", answer.start, answer.body));
        match &reply {
            SoapReply::Response { .. } => assert_eq!(answer.status, 200, "{action}"),
            SoapReply::Fault { .. } => {
                assert_eq!(answer.status, 500, "{action}: a fault is HTTP 500")
            }
        }
        reply
    }

    /// An action that must succeed.
    pub fn ok(
        &self,
        device: &Device,
        service: Service,
        action: &str,
        arguments: &[(&str, &str)],
    ) -> SoapReply {
        let reply = self.call(device, service, action, arguments);
        assert!(
            matches!(reply, SoapReply::Response { .. }),
            "{action} {arguments:?}: {reply:?}"
        );
        reply
    }

    /// An action that must fail with `code`.
    pub fn fault(
        &self,
        device: &Device,
        service: Service,
        action: &str,
        arguments: &[(&str, &str)],
        code: u16,
    ) {
        let reply = self.call(device, service, action, arguments);
        assert_eq!(reply.fault_code(), Some(code), "{action} {arguments:?}");
    }

    pub fn avt(&self, device: &Device, action: &str) -> SoapReply {
        self.ok(device, AVT, action, &[("InstanceID", "0")])
    }

    pub fn transport(&self, device: &Device) -> (String, String) {
        let info = self.avt(device, "GetTransportInfo");
        (
            info.value("CurrentTransportState").unwrap().to_string(),
            info.value("CurrentTransportStatus").unwrap().to_string(),
        )
    }

    pub fn set_uri(&self, device: &Device, uri: &str, metadata: &str) -> SoapReply {
        self.call(
            device,
            AVT,
            "SetAVTransportURI",
            &[
                ("InstanceID", "0"),
                ("CurrentURI", uri),
                ("CurrentURIMetaData", metadata),
            ],
        )
    }

    pub fn set_next(&self, device: &Device, uri: &str, metadata: &str) -> SoapReply {
        self.call(
            device,
            AVT,
            "SetNextAVTransportURI",
            &[
                ("InstanceID", "0"),
                ("NextURI", uri),
                ("NextURIMetaData", metadata),
            ],
        )
    }

    pub fn play(&self, device: &Device) {
        self.ok(device, AVT, "Play", &[("InstanceID", "0"), ("Speed", "1")]);
    }

    pub fn subscribe(&self, device: &Device, service: Service, callback: &str) -> String {
        let answer = exchange(
            &self.http,
            client::subscribe_request(
                &device.path(service, "event"),
                &self.http,
                callback,
                Some(1800),
            )
            .as_bytes(),
        );
        assert_eq!(answer.status, 200, "SUBSCRIBE: {}", answer.start);
        answer.headers.get("SID").expect("a SID").to_string()
    }

    /// A real client session in `room`, and what it hears. The room first
    /// plays nothing, so everything heard is what a renderer played.
    pub fn listen_in(&self, room: &str) -> Heard {
        let endpoint = common::fresh_id("speaker");
        self.server.applied(&format!(
            r#"{{"v":1,"t":"attach","zone":"{room}","endpoint":"{endpoint}"}}"#
        ));
        self.server.applied(&format!(
            r#"{{"v":2,"t":"take","target":"{room}","source":"none"}}"#
        ));
        let listener = Listener::connect(&self.server.audio, &endpoint, 0);
        let heard = Heard::from(listener, Arc::clone(&self.keep));
        wait("the session's first chunks", || {
            (heard.chunks() >= 3).then_some(())
        });
        heard
    }

    /// The control state's object for `room`.
    pub fn room(&self, room: &str) -> String {
        let state = self.server.state();
        let at = state
            .find(&format!(r#"{{"id":"{room}","name""#))
            .unwrap_or_else(|| panic!("no room {room} in {state}"));
        let rest = &state[at..];
        let end = rest[1..].find(r#"{"id":""#).map_or(rest.len(), |e| e + 1);
        rest[..end].to_string()
    }
}
