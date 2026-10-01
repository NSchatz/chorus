//! What the server's end-to-end tests share: an endpoint that speaks
//! protocol v2 through the Linux client's own session code.
//!
//! Every audio connection to `chorus-server` is an encrypted v2 session, so a
//! test that wants audio off the real binary opens one exactly the way the
//! shipped client does (`chorus_client_linux::session::open`), and then reads
//! the v1 frames the session carries, unchanged.

#![allow(dead_code)]

use std::io::Read;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use chorus_client_linux::config::ClientConfig;
use chorus_client_linux::session::{self, EndpointIdentity, Session};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// An endpoint id no other test endpoint in this process uses, so a fresh
/// ephemeral key is never presented under an id already pinned to another.
pub fn fresh_id(stem: &str) -> String {
    format!(
        "{}-{}-{}",
        stem,
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

/// Connect to `address` and open a v2 session as a fresh endpoint, with a
/// read timeout of `read_timeout` once the session is up.
pub fn v2_client<A: ToSocketAddrs>(address: A, read_timeout: Duration) -> Session {
    let stream = TcpStream::connect(address).expect("the server is listening");
    stream
        .set_read_timeout(Some(read_timeout))
        .expect("a read timeout");
    let mut me = EndpointIdentity::ephemeral(&fresh_id("test-endpoint")).expect("an identity");
    session::open(stream, &mut me, &ClientConfig::default())
        .unwrap_or_else(|e| panic!("the v2 session opens: {}", e))
}

/// Read the session's audio to its end and report how many plaintext bytes
/// (v1 frames) it carried.
pub fn drain(session: &mut Session) -> usize {
    let mut total = 0usize;
    let mut scratch = vec![0u8; 65_536];
    loop {
        match session.reader.read(&mut scratch) {
            Ok(0) => break,
            Ok(n) => total += n,
            Err(_) => break,
        }
    }
    total
}

// --- the stream slots' and room volume's tests (goal 11) ----------------------

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Instant;

use chorus_protocol::v2::{Message as V2Message, RoomVolume};
use chorus_protocol::{decode_frame, AudioChunk, FrameOutcome, Message};

/// A `chorus-server` on kernel-assigned ports, with its output kept.
pub struct RunningServer {
    child: Child,
    lines: mpsc::Receiver<String>,
    /// Everything it has said so far.
    pub seen: Vec<String>,
    /// Where its audio is served.
    pub audio: String,
    /// Where its control plane listens.
    pub control: String,
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if thread::panicking() {
            while let Ok(line) = self.lines.recv_timeout(Duration::from_millis(200)) {
                self.seen.push(line);
            }
            eprintln!("the server said:\n{}", self.seen.join("\n"));
        }
    }
}

fn address_after(lines: &[String], marker: &str) -> Option<String> {
    lines.iter().find_map(|line| {
        let at = line.find(marker)?;
        Some(
            line[at + marker.len()..]
                .split_whitespace()
                .next()?
                .to_string(),
        )
    })
}

impl RunningServer {
    /// Start the server with a control plane and a throwaway identity, both
    /// sockets on port 0, plus `extra`; return once it is listening for audio.
    pub fn start(extra: &[&str]) -> RunningServer {
        let mut child = Command::new(env!("CARGO_BIN_EXE_chorus-server"))
            .args([
                "--listen",
                "127.0.0.1:0",
                "--control-listen",
                "127.0.0.1:0",
                "--allow-non-realtime",
                "--allow-unlocked-memory",
                "--ephemeral-identity",
            ])
            .args(extra)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the server binary runs");
        let (tx, lines) = mpsc::channel();
        for pipe in [
            Box::new(child.stdout.take().unwrap()) as Box<dyn Read + Send>,
            Box::new(child.stderr.take().unwrap()),
        ] {
            let tx = tx.clone();
            thread::spawn(move || {
                for line in BufReader::new(pipe).lines().map_while(Result::ok) {
                    if tx.send(line).is_err() {
                        return;
                    }
                }
            });
        }
        let mut server = RunningServer {
            child,
            lines,
            seen: Vec::new(),
            audio: String::new(),
            control: String::new(),
        };
        server.wait_for("chorus-server: listening on=");
        server.audio = address_after(&server.seen, "chorus-server: listening on=").unwrap();
        server.control = address_after(&server.seen, "control listening on=").unwrap();
        server
    }

    /// Wait for a line containing `what`, and give it back.
    pub fn wait_for(&mut self, what: &str) -> String {
        if let Some(line) = self.seen.iter().find(|l| l.contains(what)) {
            return line.clone();
        }
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            if let Ok(line) = self.lines.recv_timeout(Duration::from_millis(100)) {
                self.seen.push(line.clone());
                if line.contains(what) {
                    return line;
                }
            }
        }
        panic!(
            "the server never said {:?}; it said:\n{}",
            what,
            self.seen.join("\n")
        );
    }

    /// Take in everything said so far without waiting.
    pub fn drain(&mut self) {
        while let Ok(line) = self.lines.try_recv() {
            self.seen.push(line);
        }
    }

    /// Wait for a line containing every one of `what`, and give it back.
    pub fn wait_for_all(&mut self, what: &[&str]) -> String {
        let all = |l: &String| what.iter().all(|w| l.contains(w));
        if let Some(line) = self.seen.iter().find(|l| all(l)) {
            return line.clone();
        }
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            if let Ok(line) = self.lines.recv_timeout(Duration::from_millis(100)) {
                self.seen.push(line.clone());
                if all(&line) {
                    return line;
                }
            }
        }
        panic!(
            "the server never said {:?}; it said:\n{}",
            what,
            self.seen.join("\n")
        );
    }

    /// `POST /api/command`: the status line and the body.
    pub fn command(&self, body: &str) -> (String, String) {
        http(
            &self.control,
            &format!(
                "POST /api/command HTTP/1.1\r\nHost: chorus\r\nContent-Type: application/json\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            ),
        )
    }

    /// A command that must apply; its answer, the state.
    pub fn applied(&self, body: &str) -> String {
        let (status, answer) = self.command(body);
        assert!(
            status.contains("200"),
            "{} was answered {} {}",
            body,
            status,
            answer
        );
        answer
    }

    /// `GET /api/state`'s body.
    pub fn state(&self) -> String {
        http(
            &self.control,
            "GET /api/state HTTP/1.1\r\nHost: chorus\r\nConnection: close\r\n\r\n",
        )
        .1
    }
}

/// One HTTP request on a fresh connection: the status line and the body.
pub fn http(address: &str, request: &str) -> (String, String) {
    let mut socket = TcpStream::connect(address).expect("the control plane listens");
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    socket.write_all(request.as_bytes()).unwrap();
    let mut answer = String::new();
    let _ = socket.read_to_string(&mut answer);
    let (head, body) = answer.split_once("\r\n\r\n").unwrap_or((&answer, ""));
    (
        head.lines().next().unwrap_or("").to_string(),
        body.to_string(),
    )
}

/// A player session of a known endpoint id, reading its chunks and keeping
/// every v2 message the session carried (`room_volume`, `controller_state`).
pub struct Player {
    /// The session.
    pub session: Session,
    /// Every v2 message received, in order.
    pub messages: Arc<Mutex<Vec<V2Message>>>,
    pending: Vec<u8>,
}

impl Player {
    /// Open a session as `endpoint`, declaring the player role and `extra`
    /// roles.
    pub fn connect(address: &str, endpoint: &str, extra_roles: u16) -> Player {
        let config = ClientConfig {
            extra_roles,
            ..ClientConfig::default()
        };
        Player::connect_with(address, endpoint, &config)
    }

    /// Open a session as `endpoint` with the Linux client's `config` (its
    /// `hello` and `capabilities` are what that configuration declares).
    pub fn connect_with(address: &str, endpoint: &str, config: &ClientConfig) -> Player {
        let stream = TcpStream::connect(address).expect("the server is listening");
        stream
            .set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        let mut me = EndpointIdentity::ephemeral(endpoint).expect("an identity");
        let mut session = session::open(stream, &mut me, config)
            .unwrap_or_else(|e| panic!("the v2 session opens: {}", e));
        let messages = Arc::new(Mutex::new(Vec::new()));
        {
            let messages = Arc::clone(&messages);
            session::also_hand(
                &mut session.reader,
                &session.announced,
                Box::new(move |m| messages.lock().unwrap().push(m.clone())),
            );
        }
        Player {
            session,
            messages,
            pending: Vec::new(),
        }
    }

    /// The session and the messages it records, for a test that drives its
    /// reader and writer from threads of its own.
    pub fn split(self) -> (Session, Arc<Mutex<Vec<V2Message>>>) {
        (self.session, self.messages)
    }

    /// The next audio chunk, or `None` within `limit`.
    pub fn next_chunk(&mut self, limit: Duration) -> Option<AudioChunk> {
        let deadline = Instant::now() + limit;
        let mut scratch = vec![0u8; 65_536];
        loop {
            let mut at = 0usize;
            let mut found = None;
            while at < self.pending.len() {
                let d = decode_frame(&self.pending[at..]);
                if d.consumed == 0 {
                    break;
                }
                at += d.consumed;
                if let FrameOutcome::Decoded(Message::AudioChunk(c)) = d.outcome {
                    found = Some(c);
                    break;
                }
            }
            self.pending.drain(..at);
            if found.is_some() {
                return found;
            }
            if Instant::now() >= deadline {
                return None;
            }
            match self.session.reader.read(&mut scratch) {
                Ok(0) => return None,
                Ok(n) => self.pending.extend_from_slice(&scratch[..n]),
                Err(_) => {}
            }
        }
    }

    /// Every `room_volume` received so far.
    pub fn room_volumes(&self) -> Vec<RoomVolume> {
        self.messages
            .lock()
            .unwrap()
            .iter()
            .filter_map(|m| match m {
                V2Message::RoomVolume(r) => Some(*r),
                _ => None,
            })
            .collect()
    }

    /// Read chunks until `done` holds of what was received, or panic naming
    /// `what` after `limit`.
    pub fn until(&mut self, what: &str, limit: Duration, done: impl Fn(&Player) -> bool) {
        let deadline = Instant::now() + limit;
        while !done(self) {
            assert!(
                Instant::now() < deadline,
                "{}: not within {:?}; messages {:?}",
                what,
                limit,
                self.messages.lock().unwrap()
            );
            let _ = self.next_chunk(Duration::from_millis(50));
        }
    }
}
