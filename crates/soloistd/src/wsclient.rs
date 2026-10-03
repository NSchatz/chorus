//! The WebSocket client of Soloist's local API: the socket, the timeouts
//! and the random bytes around `chorus_soloist::ws`'s pure framing.
//!
//! One thread per connection attempt. It waits for Soloist to write
//! `ws.port` into its data directory, connects, shakes hands, and then
//! reads frames until the connection ends or it is told to stop. A second,
//! short-lived thread writes the commands the supervisor hands it, so a
//! command never waits for a read to time out.

use std::fs::File;
use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chorus_soloist::ws::{self, Decoder, Message, Role};
use chorus_soloist::{WS_ADDR_FILE, WS_PORT_FILE};

/// How often the data directory is looked at for `ws.port`, and how long a
/// read waits before the stop flag is looked at again.
const POLL: Duration = Duration::from_millis(25);

/// The longest a connect, a handshake read or a write may take.
const IO_TIMEOUT: Duration = Duration::from_secs(5);

/// What the client reports to the supervisor.
#[derive(Debug)]
pub enum WsReport {
    /// The handshake succeeded: Soloist's API is connected.
    Up,
    /// One JSON text frame.
    Event(String),
    /// No connection within the bound: a fault (Soloist "starts without the
    /// WebSocket API and logs a warning" when it cannot bind).
    Fault(String),
    /// A connection that was up has ended.
    Lost(String),
}

/// Where Soloist says its API listens, if it has said so: `ws.port`, and
/// `ws.addr` when present (else loopback). The files' format is not stated
/// by Soloist's documentation, so they are read leniently: surrounding
/// whitespace is ignored.
pub fn endpoint(data_dir: &Path) -> Option<SocketAddr> {
    let port: u16 = std::fs::read_to_string(data_dir.join(WS_PORT_FILE))
        .ok()?
        .trim()
        .parse()
        .ok()?;
    if port == 0 {
        return None;
    }
    let addr = std::fs::read_to_string(data_dir.join(WS_ADDR_FILE))
        .ok()
        .and_then(|text| text.trim().parse::<IpAddr>().ok())
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    Some(SocketAddr::new(addr, port))
}

/// Random bytes from the kernel, for the handshake nonce and frame masks
/// (RFC 6455 section 5.3 asks for an unpredictable mask per frame).
struct Random(File);

impl Random {
    fn open() -> io::Result<Random> {
        File::open("/dev/urandom").map(Random)
    }

    fn bytes<const N: usize>(&mut self) -> io::Result<[u8; N]> {
        let mut out = [0u8; N];
        self.0.read_exact(&mut out)?;
        Ok(out)
    }
}

/// Connect and shake hands; the bytes that followed the handshake's head
/// are returned with the stream.
fn connect(addr: SocketAddr, random: &mut Random) -> io::Result<(TcpStream, Vec<u8>)> {
    let mut stream = TcpStream::connect_timeout(&addr, IO_TIMEOUT)?;
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    let key = ws::client_key(random.bytes()?);
    stream.write_all(ws::handshake_request(&addr.to_string(), "/", &key).as_bytes())?;
    let mut head = Vec::new();
    let mut chunk = [0u8; 1024];
    let end = loop {
        if let Some(end) = ws::head_end(&head) {
            break end;
        }
        if head.len() > ws::MAX_HANDSHAKE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the handshake answer is too long",
            ));
        }
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "closed during the handshake",
            ));
        }
        head.extend_from_slice(&chunk[..n]);
    };
    ws::check_handshake_response(&head[..end], &key)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    Ok((stream, head[end..].to_vec()))
}

/// A running client: drop the sender to end its writer; set `stop` to end
/// its reader.
pub struct Client {
    /// Commands to send, each one JSON text.
    pub commands: Sender<String>,
}

/// Start a client for the Soloist whose data directory is `data_dir`.
/// Reports go to `report`; `stop` ends it quietly.
pub fn spawn(
    data_dir: PathBuf,
    timeout: Duration,
    stop: Arc<AtomicBool>,
    report: impl Fn(WsReport) + Send + 'static,
) -> Client {
    let (commands, outbox) = std::sync::mpsc::channel::<String>();
    thread::spawn(move || run(&data_dir, timeout, &stop, outbox, &report));
    Client { commands }
}

fn run(
    data_dir: &Path,
    timeout: Duration,
    stop: &Arc<AtomicBool>,
    outbox: Receiver<String>,
    report: &dyn Fn(WsReport),
) {
    let mut random = match Random::open() {
        Ok(random) => random,
        Err(e) => return report(WsReport::Fault(format!("no random source: {e}"))),
    };
    // Wait for ws.port, then connect; both inside the one bound.
    let deadline = Instant::now() + timeout;
    let mut last_error = format!("no {WS_PORT_FILE} in the data directory");
    let (stream, rest) = loop {
        if stop.load(Ordering::SeqCst) {
            return;
        }
        if let Some(addr) = endpoint(data_dir) {
            match connect(addr, &mut random) {
                Ok(connected) => break connected,
                Err(e) => last_error = format!("{addr}: {e}"),
            }
        }
        if Instant::now() >= deadline {
            return report(WsReport::Fault(format!(
                "no WebSocket within {} ms ({last_error})",
                timeout.as_millis()
            )));
        }
        thread::sleep(POLL);
    };
    let writer = match stream.try_clone() {
        Ok(clone) => Arc::new(Mutex::new((clone, random))),
        Err(e) => return report(WsReport::Fault(format!("the socket cannot be shared: {e}"))),
    };
    report(WsReport::Up);

    // The writer: one masked text frame per command, until the supervisor
    // drops its sender or a write fails (the reader then sees the end too).
    let for_commands = Arc::clone(&writer);
    thread::spawn(move || {
        for text in outbox {
            if send(&for_commands, |mask| ws::client_text(&text, mask)).is_err() {
                break;
            }
        }
    });

    let mut stream = stream;
    let _ = stream.set_read_timeout(Some(POLL));
    let mut decoder = Decoder::new(Role::Client, ws::DEFAULT_MAX_MESSAGE);
    decoder.feed(&rest);
    let mut chunk = [0u8; 8192];
    let why = 'connection: loop {
        loop {
            match decoder.next_message() {
                Ok(None) => break,
                Ok(Some(Message::Text(text))) => report(WsReport::Event(text)),
                Ok(Some(Message::Ping(data))) => {
                    if let Err(e) = send(&writer, |mask| ws::client_pong(&data, mask)) {
                        break 'connection format!("a pong could not be sent: {e}");
                    }
                }
                Ok(Some(Message::Close { code, .. })) => {
                    let _ = send(&writer, |mask| ws::client_close(1000, mask));
                    break 'connection format!("Soloist closed the WebSocket (code {code:?})");
                }
                // Soloist's API is JSON text frames; anything else is ignored.
                Ok(Some(Message::Binary(_) | Message::Pong(_))) => {}
                Err(e) => break 'connection format!("a bad frame: {e}"),
            }
        }
        if stop.load(Ordering::SeqCst) {
            let _ = send(&writer, |mask| ws::client_close(1000, mask));
            let _ = stream.shutdown(std::net::Shutdown::Both);
            return;
        }
        match stream.read(&mut chunk) {
            Ok(0) => break "Soloist closed the connection".to_string(),
            Ok(n) => decoder.feed(&chunk[..n]),
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => break format!("read: {e}"),
        }
    };
    let _ = stream.shutdown(std::net::Shutdown::Both);
    if !stop.load(Ordering::SeqCst) {
        report(WsReport::Lost(why));
    }
}

/// Write one frame, built with a fresh mask.
fn send(
    writer: &Mutex<(TcpStream, Random)>,
    frame: impl FnOnce([u8; 4]) -> Vec<u8>,
) -> io::Result<()> {
    let mut guard = writer.lock().unwrap_or_else(|e| e.into_inner());
    let mask = guard.1.bytes()?;
    guard.0.write_all(&frame(mask))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::mpsc;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("chorus-soloistd-ws-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_endpoint_files_are_read_leniently() {
        let dir = scratch("endpoint");
        assert_eq!(endpoint(&dir), None);
        std::fs::write(dir.join("ws.port"), " 4455\n").unwrap();
        assert_eq!(endpoint(&dir), Some("127.0.0.1:4455".parse().unwrap()));
        std::fs::write(dir.join("ws.addr"), "::1\r\n").unwrap();
        assert_eq!(endpoint(&dir), Some("[::1]:4455".parse().unwrap()));
        std::fs::write(dir.join("ws.addr"), "not an address").unwrap();
        assert_eq!(endpoint(&dir), Some("127.0.0.1:4455".parse().unwrap()));
        for bad in ["", "0", "port", "70000", "-1"] {
            std::fs::write(dir.join("ws.port"), bad).unwrap();
            assert_eq!(endpoint(&dir), None, "{bad:?}");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A server that speaks just enough RFC 6455: the handshake, one text
    /// frame split over two fragments with a ping between, then it reads
    /// the pong and one command and closes.
    #[test]
    fn a_client_shakes_hands_reassembles_pongs_and_sends_masked_commands() {
        let dir = scratch("client");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        std::fs::write(
            dir.join("ws.port"),
            listener.local_addr().unwrap().port().to_string(),
        )
        .unwrap();
        let server = thread::spawn(move || {
            let (mut peer, _) = listener.accept().unwrap();
            peer.set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while ws::head_end(&head).is_none() {
                peer.read_exact(&mut byte).unwrap();
                head.push(byte[0]);
            }
            let key = ws::read_handshake_request(&head).unwrap();
            peer.write_all(ws::handshake_response(&key).as_bytes())
                .unwrap();
            let mut out = ws::encode_frame(false, ws::Opcode::Text, br#"{"type":"#, None);
            out.extend(ws::encode_frame(true, ws::Opcode::Ping, b"hi", None));
            out.extend(ws::encode_frame(
                true,
                ws::Opcode::Continuation,
                br#""x"}"#,
                None,
            ));
            peer.write_all(&out).unwrap();
            let mut decoder = Decoder::new(Role::Server, 4096);
            let mut got = Vec::new();
            let mut chunk = [0u8; 256];
            while got.len() < 2 {
                let n = peer.read(&mut chunk).unwrap();
                assert!(n > 0, "the client hung up early");
                decoder.feed(&chunk[..n]);
                while let Some(message) = decoder.next_message().unwrap() {
                    got.push(message);
                }
            }
            peer.write_all(&ws::encode_frame(
                true,
                ws::Opcode::Close,
                &[0x03, 0xE8],
                None,
            ))
            .unwrap();
            got
        });
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let client = spawn(dir.clone(), Duration::from_secs(10), stop, move |r| {
            let _ = tx.send(r);
        });
        let wait = Duration::from_secs(10);
        assert!(matches!(rx.recv_timeout(wait).unwrap(), WsReport::Up));
        match rx.recv_timeout(wait).unwrap() {
            WsReport::Event(text) => assert_eq!(text, r#"{"type":"x"}"#),
            other => panic!("{other:?}"),
        }
        client
            .commands
            .send(r#"{"type":"command","command":"pause"}"#.to_string())
            .unwrap();
        let got = server.join().unwrap();
        assert!(got.contains(&Message::Pong(b"hi".to_vec())), "{got:?}");
        assert!(
            got.contains(&Message::Text(
                r#"{"type":"command","command":"pause"}"#.into()
            )),
            "{got:?}"
        );
        assert!(matches!(rx.recv_timeout(wait).unwrap(), WsReport::Lost(_)));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn no_port_file_within_the_bound_is_a_fault() {
        let dir = scratch("fault");
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let _client = spawn(dir.clone(), Duration::from_millis(100), stop, move |r| {
            let _ = tx.send(r);
        });
        match rx.recv_timeout(Duration::from_secs(10)).unwrap() {
            WsReport::Fault(why) => assert!(why.contains("ws.port"), "{why}"),
            other => panic!("{other:?}"),
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_server_that_is_not_a_websocket_is_a_fault() {
        let dir = scratch("nothttp");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        std::fs::write(
            dir.join("ws.port"),
            listener.local_addr().unwrap().port().to_string(),
        )
        .unwrap();
        let server = thread::spawn(move || {
            // Answer every attempt with a plain 200 until the client gives up.
            listener.set_nonblocking(true).unwrap();
            let end = Instant::now() + Duration::from_millis(600);
            while Instant::now() < end {
                if let Ok((mut peer, _)) = listener.accept() {
                    let _ = peer.write_all(b"HTTP/1.1 200 OK\r\n\r\n");
                }
                thread::sleep(Duration::from_millis(5));
            }
        });
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let _client = spawn(dir.clone(), Duration::from_millis(200), stop, move |r| {
            let _ = tx.send(r);
        });
        match rx.recv_timeout(Duration::from_secs(10)).unwrap() {
            WsReport::Fault(why) => assert!(why.contains("did not switch protocols"), "{why}"),
            other => panic!("{other:?}"),
        }
        server.join().unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
