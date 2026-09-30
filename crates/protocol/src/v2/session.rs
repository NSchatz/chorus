//! A v2 session over any byte stream: the handshake, adoption, and records.
//!
//! No socket and no clock: everything here is generic over `Read` and
//! `Write`, and a caller that wants a timeout sets it on its own stream. The
//! order of events is `docs/protocol.md`, "The session":
//!
//! 1. The endpoint sends `handshake_init` (magic, version 2, suite, Noise
//!    message 1). A server whose first frame from a peer is a v1 message
//!    refuses it by name: `session_refused` with reason `protocol_version`.
//! 2. The server answers `handshake_response` (Noise message 2, carrying its
//!    id). The endpoint checks the server's key against its own pin.
//! 3. The endpoint sends `handshake_finish` (Noise message 3, carrying its id).
//!    The server checks the endpoint's key against the pin taken at adoption;
//!    a changed key is refused with `session_refused` `key_changed` and
//!    surfaced to the caller as [`SessionError::KeyChanged`].
//! 4. Everything after is `secure_record` frames, each carrying whole v2
//!    frames. [`SecureWriter`] and [`SecureReader`] hide the records: a v1
//!    frame written to one comes out of the other as the same bytes, which is
//!    what lets the v1 audio path run unchanged inside a v2 session.

use std::collections::VecDeque;
use std::fmt;
use std::io::{self, Read, Write};

use crate::codec::{HEADER_LEN, MAX_PAYLOAD_LEN};
use crate::v2::adoption::{KeyChange, Verdict, MAX_ID_LEN};
use crate::v2::catalog::{RefusalReason, Suite, Type, MAGIC, PROTOCOL_VERSION};
use crate::v2::codec::{decode_frame, encode, Outcome};
use crate::v2::messages::{
    HandshakeFinish, HandshakeInit, HandshakeResponse, Message, SecureRecord, SessionRefused,
};
use crate::v2::noise::{
    fingerprint, CipherState, Initiator, Keypair, NoiseError, Responder, TAG_LEN,
};

/// Most plaintext one record carries: a Noise message is at most 65535 bytes
/// and the tag takes 16 of them.
pub const MAX_RECORD_PLAINTEXT: usize = MAX_PAYLOAD_LEN - TAG_LEN;

/// Who this side is: an id and a long-term key.
#[derive(Debug, Clone)]
pub struct Identity {
    /// The id the peer pins this side's key to (1 to 255 bytes of UTF-8).
    pub id: String,
    /// The long-term key pair.
    pub keypair: Keypair,
}

/// Why a session did not start, or stopped.
#[derive(Debug)]
pub enum SessionError {
    /// The byte stream failed (a timeout included).
    Io(io::Error),
    /// The peer spoke protocol v1. It was refused by name.
    PeerSpeaksV1 {
        /// The v1 message it opened with.
        first: &'static str,
    },
    /// The peer offered a version or suite this side does not speak.
    UnsupportedVersion {
        /// What it offered.
        detail: String,
    },
    /// The peer refused this session and said why.
    Refused {
        /// Its reason.
        reason: RefusalReason,
        /// Its sentence.
        detail: String,
    },
    /// The peer's key is not the one pinned for its id. It was refused.
    KeyChanged(KeyChange),
    /// The owner removed this peer. It was refused.
    Removed {
        /// Its id.
        id: String,
    },
    /// The key exchange failed.
    Noise(NoiseError),
    /// The peer sent something the session does not allow here.
    Protocol(String),
    /// The server never answered `handshake_init`: it may speak protocol v1,
    /// which steps over the frame without replying.
    NoV2Answer(io::Error),
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SessionError::Io(e) => write!(f, "the connection failed: {}", e),
            SessionError::PeerSpeaksV1 { first } => write!(
                f,
                "refused a chorus protocol v1 peer (its first frame was {}); this side speaks v2 only",
                first
            ),
            SessionError::UnsupportedVersion { detail } => write!(f, "refused: {}", detail),
            SessionError::Refused { reason, detail } => {
                write!(f, "the peer refused the session ({}): {}", reason.name(), detail)
            }
            SessionError::KeyChanged(change) => write!(f, "key changed: {}", change),
            SessionError::Removed { id } => write!(f, "{} was removed by the owner; refused", id),
            SessionError::Noise(e) => write!(f, "the key exchange failed: {}", e),
            SessionError::Protocol(what) => write!(f, "protocol error: {}", what),
            SessionError::NoV2Answer(e) => write!(
                f,
                "the server did not answer the v2 handshake ({}); it may speak chorus protocol v1",
                e
            ),
        }
    }
}

impl std::error::Error for SessionError {}

impl From<io::Error> for SessionError {
    fn from(e: io::Error) -> Self {
        SessionError::Io(e)
    }
}

impl From<NoiseError> for SessionError {
    fn from(e: NoiseError) -> Self {
        SessionError::Noise(e)
    }
}

/// A session that finished its handshake.
#[derive(Debug)]
pub struct Established {
    /// The peer's id, as it sent it inside the handshake.
    pub peer_id: String,
    /// The peer's long-term key, authenticated by the handshake.
    pub peer_key: [u8; 32],
    /// Whether the peer was adopted just now or already known.
    pub verdict: Verdict,
    /// The Noise handshake hash (both sides hold the same value).
    pub handshake_hash: [u8; 32],
    /// Seals what this side sends.
    pub sealer: RecordSealer,
    /// Opens what this side receives.
    pub opener: RecordOpener,
}

/// The prologue both sides mix in: the 7 bytes of `handshake_init` that come
/// before the Noise message (magic, version, suite), so a changed version or
/// suite fails the handshake rather than being negotiated down.
pub fn prologue(protocol_version: u16, suite: Suite) -> [u8; 7] {
    let mut p = [0u8; 7];
    p[..4].copy_from_slice(&MAGIC);
    p[4..6].copy_from_slice(&protocol_version.to_be_bytes());
    p[6] = suite.to_wire();
    p
}

/// The id payload of Noise messages 2 and 3: a length byte, then UTF-8.
pub fn id_payload(id: &str) -> Result<Vec<u8>, SessionError> {
    if id.is_empty() || id.len() > MAX_ID_LEN {
        return Err(SessionError::Protocol(format!(
            "an id is 1 to 255 bytes, not {}",
            id.len()
        )));
    }
    let mut out = vec![id.len() as u8];
    out.extend_from_slice(id.as_bytes());
    Ok(out)
}

fn parse_id(payload: &[u8]) -> Result<String, SessionError> {
    let bad = || SessionError::Protocol("the handshake payload is not an id".to_string());
    let (&n, rest) = payload.split_first().ok_or_else(bad)?;
    let n = n as usize;
    if n == 0 || rest.len() < n {
        return Err(bad());
    }
    String::from_utf8(rest[..n].to_vec()).map_err(|_| bad())
}

/// Read one whole frame: its type byte and payload.
pub fn read_frame<R: Read>(r: &mut R) -> io::Result<(u8, Vec<u8>)> {
    let mut header = [0u8; HEADER_LEN];
    r.read_exact(&mut header)?;
    let len = u16::from_be_bytes([header[1], header[2]]) as usize;
    let mut payload = vec![0u8; len];
    r.read_exact(&mut payload)?;
    Ok((header[0], payload))
}

fn frame_bytes(type_byte: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.push(type_byte);
    out.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

fn decode_one(type_byte: u8, payload: &[u8]) -> Result<Message, SessionError> {
    match decode_frame(&frame_bytes(type_byte, payload)).outcome {
        Outcome::Decoded(m) => Ok(m),
        Outcome::Rejected(e) => Err(SessionError::Protocol(e.to_string())),
        Outcome::SkippedUnknownType { message_type, .. } => Err(SessionError::Protocol(format!(
            "unexpected message type 0x{:02x}",
            message_type
        ))),
    }
}

fn send<W: Write>(w: &mut W, message: &Message) -> Result<(), SessionError> {
    let frame = encode(message).map_err(|e| SessionError::Protocol(e.to_string()))?;
    w.write_all(&frame)?;
    w.flush()?;
    Ok(())
}

/// Send `session_refused` in the clear, best effort: the refusal is already
/// decided, and a peer that has gone away does not change it.
pub fn refuse<W: Write>(w: &mut W, reason: RefusalReason, detail: &str) {
    let mut detail = detail.to_string();
    detail.truncate(crate::v2::catalog::MAX_LONG_TEXT);
    let _ = send(
        w,
        &Message::SessionRefused(SessionRefused { reason, detail }),
    );
}

/// The server's side of the handshake.
///
/// `check` is the adoption store's verdict on the endpoint's id and key; it
/// pins a new id itself. A refusal is sent to the peer before the error is
/// returned, so the caller only has to log it and close.
pub fn accept<S, F>(
    stream: &mut S,
    me: &Identity,
    ephemeral: Keypair,
    mut check: F,
) -> Result<Established, SessionError>
where
    S: Read + Write,
    F: FnMut(&str, &[u8; 32]) -> Verdict,
{
    let (type_byte, payload) = read_frame(stream)?;
    match Type::from_wire(type_byte) {
        Some(t) if t.is_v1() => {
            let error = SessionError::PeerSpeaksV1 { first: t.name() };
            refuse(stream, RefusalReason::ProtocolVersion, &error.to_string());
            return Err(error);
        }
        Some(Type::HandshakeInit) => {}
        _ => {
            let detail = format!(
                "expected handshake_init, got message type 0x{:02x}",
                type_byte
            );
            refuse(stream, RefusalReason::HandshakeFailed, &detail);
            return Err(SessionError::Protocol(detail));
        }
    }
    let init = match decode_frame(&frame_bytes(type_byte, &payload)).outcome {
        Outcome::Decoded(Message::HandshakeInit(init)) => init,
        Outcome::Rejected(e) => {
            let (reason, detail) = if payload.len() >= 7 && payload[..4] == MAGIC && payload[6] != 1
            {
                (
                    RefusalReason::UnsupportedSuite,
                    format!("suite {} is not offered; this server offers 1", payload[6]),
                )
            } else {
                (RefusalReason::HandshakeFailed, e.to_string())
            };
            refuse(stream, reason, &detail);
            return Err(SessionError::UnsupportedVersion { detail });
        }
        other => return Err(SessionError::Protocol(format!("{:?}", other))),
    };
    if init.protocol_version != PROTOCOL_VERSION {
        let detail = format!(
            "the peer offered chorus protocol version {}; this server speaks {}",
            init.protocol_version, PROTOCOL_VERSION
        );
        refuse(stream, RefusalReason::ProtocolVersion, &detail);
        return Err(SessionError::UnsupportedVersion { detail });
    }
    let mut responder = Responder::new(
        &prologue(init.protocol_version, init.suite),
        me.keypair.clone(),
        ephemeral,
    );
    if let Err(e) = responder.read_message1(&init.noise) {
        refuse(stream, RefusalReason::HandshakeFailed, &e.to_string());
        return Err(e.into());
    }
    let message2 = responder.write_message2(&id_payload(&me.id)?)?;
    send(
        stream,
        &Message::HandshakeResponse(HandshakeResponse { noise: message2 }),
    )?;

    let (type_byte, payload) = read_frame(stream)?;
    let finish = match decode_one(type_byte, &payload)? {
        Message::HandshakeFinish(f) => f,
        Message::SessionRefused(r) => {
            return Err(SessionError::Refused {
                reason: r.reason,
                detail: r.detail,
            })
        }
        other => {
            let detail = format!(
                "expected handshake_finish, got {}",
                other.message_type().name()
            );
            refuse(stream, RefusalReason::HandshakeFailed, &detail);
            return Err(SessionError::Protocol(detail));
        }
    };
    let (payload, transport) = match responder.read_message3(&finish.noise) {
        Ok(done) => done,
        Err(e) => {
            refuse(stream, RefusalReason::HandshakeFailed, &e.to_string());
            return Err(e.into());
        }
    };
    let peer_id = parse_id(&payload)?;
    let verdict = check(&peer_id, &transport.remote_static);
    match &verdict {
        Verdict::KeyChanged { pinned, offered } => {
            let change = KeyChange {
                id: peer_id,
                pinned: fingerprint(pinned),
                offered: fingerprint(offered),
            };
            refuse(stream, RefusalReason::KeyChanged, &change.to_string());
            return Err(SessionError::KeyChanged(change));
        }
        Verdict::Removed => {
            refuse(
                stream,
                RefusalReason::NotAdopted,
                &format!("{} was removed by the owner", peer_id),
            );
            return Err(SessionError::Removed { id: peer_id });
        }
        Verdict::Adopted | Verdict::Known => {}
    }
    Ok(Established {
        peer_id,
        peer_key: transport.remote_static,
        verdict,
        handshake_hash: transport.handshake_hash,
        sealer: RecordSealer {
            cipher: transport.send,
        },
        opener: RecordOpener {
            cipher: transport.receive,
        },
    })
}

/// The endpoint's side of the handshake.
///
/// `check` is the endpoint's own store's verdict on the server's id and key.
/// A server whose key changed is refused (the endpoint sends `session_refused`
/// and stops) and surfaced as [`SessionError::KeyChanged`].
pub fn connect<S, F>(
    stream: &mut S,
    me: &Identity,
    ephemeral: Keypair,
    mut check: F,
) -> Result<Established, SessionError>
where
    S: Read + Write,
    F: FnMut(&str, &[u8; 32]) -> Verdict,
{
    let suite = Suite::NoiseXx25519ChaChaPolySha256;
    let mut initiator = Initiator::new(
        &prologue(PROTOCOL_VERSION, suite),
        me.keypair.clone(),
        ephemeral,
    );
    let message1 = initiator.write_message1(&[])?;
    send(
        stream,
        &Message::HandshakeInit(HandshakeInit {
            protocol_version: PROTOCOL_VERSION,
            suite,
            noise: message1,
        }),
    )?;
    let (type_byte, payload) = match read_frame(stream) {
        Ok(frame) => frame,
        Err(e) => return Err(SessionError::NoV2Answer(e)),
    };
    let response = match decode_one(type_byte, &payload)? {
        Message::HandshakeResponse(r) => r,
        Message::SessionRefused(r) => {
            return Err(SessionError::Refused {
                reason: r.reason,
                detail: r.detail,
            })
        }
        other => {
            return Err(SessionError::Protocol(format!(
                "expected handshake_response, got {}",
                other.message_type().name()
            )))
        }
    };
    let server_id = parse_id(&initiator.read_message2(&response.noise)?)?;
    let server_key = initiator.remote_static().ok_or(NoiseError::OutOfOrder)?;
    let verdict = check(&server_id, &server_key);
    match &verdict {
        Verdict::KeyChanged { pinned, offered } => {
            let change = KeyChange {
                id: server_id,
                pinned: fingerprint(pinned),
                offered: fingerprint(offered),
            };
            refuse(stream, RefusalReason::KeyChanged, &change.to_string());
            return Err(SessionError::KeyChanged(change));
        }
        Verdict::Removed => {
            refuse(
                stream,
                RefusalReason::NotAdopted,
                &format!("{} was removed by the owner", server_id),
            );
            return Err(SessionError::Removed { id: server_id });
        }
        Verdict::Adopted | Verdict::Known => {}
    }
    let (message3, transport) = initiator.write_message3(&id_payload(&me.id)?)?;
    send(
        stream,
        &Message::HandshakeFinish(HandshakeFinish { noise: message3 }),
    )?;
    Ok(Established {
        peer_id: server_id,
        peer_key: server_key,
        verdict,
        handshake_hash: transport.handshake_hash,
        sealer: RecordSealer {
            cipher: transport.send,
        },
        opener: RecordOpener {
            cipher: transport.receive,
        },
    })
}

/// Seals whole v2 frames into `secure_record` frames.
#[derive(Debug)]
pub struct RecordSealer {
    cipher: CipherState,
}

impl RecordSealer {
    /// Seal one or more whole frames (at most [`MAX_RECORD_PLAINTEXT`] bytes)
    /// into one `secure_record` frame. The record's own 3-byte header is the
    /// associated data, so its type and length are authenticated too.
    pub fn seal(&mut self, frames: &[u8]) -> Result<Vec<u8>, SessionError> {
        if frames.len() > MAX_RECORD_PLAINTEXT {
            return Err(SessionError::Protocol(format!(
                "{} bytes exceed a record's {} byte plaintext limit",
                frames.len(),
                MAX_RECORD_PLAINTEXT
            )));
        }
        let header = record_header(frames.len() + TAG_LEN);
        let ciphertext = self.cipher.encrypt_with_ad(&header, frames)?;
        let mut out = header.to_vec();
        out.extend(ciphertext);
        Ok(out)
    }

    /// Seal one message into a record of its own.
    pub fn seal_message(&mut self, message: &Message) -> Result<Vec<u8>, SessionError> {
        if message.message_type().is_plaintext() {
            return Err(SessionError::Protocol(format!(
                "{} travels in the clear, never inside a record",
                message.message_type().name()
            )));
        }
        let frame = encode(message).map_err(|e| SessionError::Protocol(e.to_string()))?;
        self.seal(&frame)
    }
}

fn record_header(payload_len: usize) -> [u8; HEADER_LEN] {
    let len = (payload_len as u16).to_be_bytes();
    [Type::SecureRecord.to_wire(), len[0], len[1]]
}

/// Opens `secure_record` frames.
#[derive(Debug)]
pub struct RecordOpener {
    cipher: CipherState,
}

impl RecordOpener {
    /// Open one record's payload (the ciphertext) into the frames it carries.
    pub fn open(&mut self, record: &SecureRecord) -> Result<Vec<u8>, SessionError> {
        let header = record_header(record.ciphertext.len());
        Ok(self.cipher.decrypt_with_ad(&header, &record.ciphertext)?)
    }
}

/// Writes whole frames through a session as records.
///
/// Bytes written are cut at frame boundaries: every record carries only whole
/// frames, however the caller splits its writes, and a partial frame waits for
/// the rest of itself. A v1 writer handed one of these is unchanged.
pub struct SecureWriter<W: Write> {
    inner: W,
    sealer: RecordSealer,
    pending: Vec<u8>,
    records: u64,
}

impl<W: Write> SecureWriter<W> {
    /// Wrap a stream whose handshake produced `sealer`.
    pub fn new(inner: W, sealer: RecordSealer) -> SecureWriter<W> {
        SecureWriter {
            inner,
            sealer,
            pending: Vec::new(),
            records: 0,
        }
    }

    /// Send one v2 message in a record of its own.
    pub fn send(&mut self, message: &Message) -> io::Result<()> {
        let record = self.sealer.seal_message(message).map_err(to_io)?;
        self.inner.write_all(&record)?;
        self.records += 1;
        self.inner.flush()
    }

    /// Records sent so far.
    pub fn records(&self) -> u64 {
        self.records
    }

    fn drain_whole_frames(&mut self) -> io::Result<()> {
        loop {
            let mut take = 0;
            while self.pending.len() - take >= HEADER_LEN {
                let at = take;
                let len = u16::from_be_bytes([self.pending[at + 1], self.pending[at + 2]]) as usize;
                let frame_len = HEADER_LEN + len;
                if self.pending.len() - at < frame_len {
                    break;
                }
                if frame_len > MAX_RECORD_PLAINTEXT {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("a {} byte frame does not fit a record", frame_len),
                    ));
                }
                if take + frame_len > MAX_RECORD_PLAINTEXT {
                    break;
                }
                take += frame_len;
            }
            if take == 0 {
                return Ok(());
            }
            let record = self.sealer.seal(&self.pending[..take]).map_err(to_io)?;
            self.inner.write_all(&record)?;
            self.records += 1;
            self.pending.drain(..take);
        }
    }
}

impl<W: Write> Write for SecureWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.pending.extend_from_slice(buf);
        self.drain_whole_frames()?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

fn to_io(e: SessionError) -> io::Error {
    match e {
        SessionError::Io(e) => e,
        other => io::Error::new(io::ErrorKind::InvalidData, other.to_string()),
    }
}

/// What a [`SecureReader`] does with a v2 message that is not one of v1's.
pub type MessageHandler = Box<dyn FnMut(Message) + Send>;

/// Reads records and yields the v1 frames inside them as plain bytes.
///
/// v1's three messages come out of `read` byte for byte, so a v1 reader runs
/// unchanged on top. Every other v2 message goes to the handler. Inside a
/// record, frame boundaries are known (the record carries whole frames), so an
/// unassigned type is stepped over as rule 3 says and counted; there is no
/// alignment to lose. A `session_refused` in the clear ends the stream with
/// the reason as the error; anything else in the clear is a protocol error.
pub struct SecureReader<R: Read> {
    inner: R,
    opener: RecordOpener,
    raw: Vec<u8>,
    plain: VecDeque<u8>,
    handler: Option<MessageHandler>,
    queued: VecDeque<Message>,
    skipped: u64,
    rejected: u64,
}

impl<R: Read> SecureReader<R> {
    /// Wrap a stream whose handshake produced `opener`.
    pub fn new(inner: R, opener: RecordOpener) -> SecureReader<R> {
        SecureReader {
            inner,
            opener,
            raw: Vec::new(),
            plain: VecDeque::new(),
            handler: None,
            queued: VecDeque::new(),
            skipped: 0,
            rejected: 0,
        }
    }

    /// From now on, hand v2 messages to `handler` instead of queueing them.
    /// Anything already queued goes to it first, in order.
    pub fn set_handler(&mut self, mut handler: MessageHandler) {
        for m in self.queued.drain(..) {
            handler(m);
        }
        self.handler = Some(handler);
    }

    /// Frames of unassigned types stepped over inside records.
    pub fn skipped(&self) -> u64 {
        self.skipped
    }

    /// v2 frames rejected inside records.
    pub fn rejected(&self) -> u64 {
        self.rejected
    }

    /// The next v2 message that is not one of v1's, reading records as needed.
    /// v1 frames met on the way are kept for `read`.
    pub fn next_message(&mut self) -> io::Result<Message> {
        loop {
            if let Some(m) = self.queued.pop_front() {
                return Ok(m);
            }
            self.pump()?;
        }
    }

    /// Read one more record (or refusal) from the stream and sort its frames.
    fn pump(&mut self) -> io::Result<()> {
        loop {
            if let Some((type_byte, len)) = self.whole_frame() {
                let payload: Vec<u8> = self.raw[HEADER_LEN..HEADER_LEN + len].to_vec();
                self.raw.drain(..HEADER_LEN + len);
                return self.take_frame(type_byte, payload);
            }
            let mut buf = [0u8; 16 * 1024];
            let n = self.inner.read(&mut buf)?;
            if n == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "the peer closed the session",
                ));
            }
            self.raw.extend_from_slice(&buf[..n]);
        }
    }

    fn whole_frame(&self) -> Option<(u8, usize)> {
        if self.raw.len() < HEADER_LEN {
            return None;
        }
        let len = u16::from_be_bytes([self.raw[1], self.raw[2]]) as usize;
        (self.raw.len() >= HEADER_LEN + len).then_some((self.raw[0], len))
    }

    fn take_frame(&mut self, type_byte: u8, payload: Vec<u8>) -> io::Result<()> {
        match Type::from_wire(type_byte) {
            Some(Type::SecureRecord) => {}
            Some(Type::SessionRefused) => {
                let e = match decode_one(type_byte, &payload) {
                    Ok(Message::SessionRefused(r)) => SessionError::Refused {
                        reason: r.reason,
                        detail: r.detail,
                    },
                    _ => SessionError::Protocol("an undecodable session_refused".to_string()),
                };
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionRefused,
                    e.to_string(),
                ));
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("message type 0x{:02x} arrived outside a record", type_byte),
                ))
            }
        }
        let plaintext = self
            .opener
            .open(&SecureRecord {
                ciphertext: payload,
            })
            .map_err(to_io)?;
        let mut at = 0;
        while at < plaintext.len() {
            let d = decode_frame(&plaintext[at..]);
            if d.consumed == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "a record ends inside a frame",
                ));
            }
            let bytes = &plaintext[at..at + d.consumed];
            let type_byte = bytes[0];
            match d.outcome {
                Outcome::Decoded(m) if m.message_type().is_plaintext() => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("{} arrived inside a record", m.message_type().name()),
                    ))
                }
                Outcome::Decoded(m) if m.message_type().is_v1() => self.plain.extend(bytes),
                Outcome::Decoded(m) => match self.handler.as_mut() {
                    Some(h) => h(m),
                    None => self.queued.push_back(m),
                },
                Outcome::SkippedUnknownType { .. } => self.skipped += 1,
                // A rejected v1 frame goes up to the v1 reader, which counts
                // it as one bad frame the way it always has.
                Outcome::Rejected(_) if (0x01..=0x03).contains(&type_byte) => {
                    self.plain.extend(bytes)
                }
                Outcome::Rejected(_) => self.rejected += 1,
            }
            at += d.consumed;
        }
        Ok(())
    }
}

impl<R: Read> Read for SecureReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        while self.plain.is_empty() {
            match self.pump() {
                Ok(()) => {}
                // A clean close with nothing buffered is a clean end of stream
                // for the v1 reader above, which decides what it means.
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof && self.raw.is_empty() => {
                    return Ok(0)
                }
                Err(e) => return Err(e),
            }
        }
        let n = buf.len().min(self.plain.len());
        for (slot, byte) in buf.iter_mut().zip(self.plain.drain(..n)) {
            *slot = byte;
        }
        Ok(n)
    }
}
