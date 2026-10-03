//! RFC 6455 (The WebSocket Protocol) as pure functions: the opening
//! handshake and the frame codec, bytes in and bytes out.
//!
//! Written from the specification, https://www.rfc-editor.org/rfc/rfc6455
//! (read 2026-10-03); section numbers below are that document's. The caller
//! owns the socket, the timeouts and the random bytes (a nonce for the key,
//! section 4.1, and a mask for every client frame, section 5.3): this module
//! reads no clock, no file and no random source.
//!
//! What is here is what a client of Soloist's local API needs, and the
//! mirror image a test server needs (the fake Soloist):
//!
//! - the client's request ([`handshake_request`]) and the check of the
//!   server's answer, including `Sec-WebSocket-Accept`
//!   ([`check_handshake_response`]); the server's side of both
//!   ([`read_handshake_request`], [`handshake_response`]);
//! - frames out ([`encode_frame`] and the `client_*` helpers, always masked;
//!   a server passes no mask);
//! - frames in ([`Decoder`]): reassembly of fragmented messages, control
//!   frames between fragments, and a bound on a message's size.
//!
//! No extension and no subprotocol is ever offered, so none may be accepted:
//! a response naming one fails the handshake (section 4.1), and a frame with
//! a reserved bit set fails the connection (section 5.2).
//!
//! SHA-1 and base64 are private to this module: the handshake is their only
//! use here (the decision record of goal 17's track S1 says why they are not
//! shared with `crates/upnp`).

/// The GUID every server appends to the client's key (section 1.3).
const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// The longest handshake head (status line and headers) either side accepts.
pub const MAX_HANDSHAKE: usize = 8192;

/// The default bound on one reassembled message: Soloist's largest documented
/// event (a `playback_state` or a `queue_changed` of 10 entries a side) is a
/// few kilobytes; a megabyte is far above anything legitimate.
pub const DEFAULT_MAX_MESSAGE: usize = 1 << 20;

/// SHA-1, RFC 3174 (https://www.rfc-editor.org/rfc/rfc3174, read 2026-10-03).
/// Used only for `Sec-WebSocket-Accept`, where the RFC requires it; it
/// protects nothing.
fn sha1(data: &[u8]) -> [u8; 20] {
    // Section 6.1: the initial hash value.
    let mut h: [u32; 5] = [
        0x6745_2301,
        0xEFCD_AB89,
        0x98BA_DCFE,
        0x1032_5476,
        0xC3D2_E1F0,
    ];
    // Section 4: a 1 bit, zeros to 56 mod 64 bytes, then the length in bits
    // as 64 bits, most significant first.
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&(data.len() as u64).wrapping_mul(8).to_be_bytes());
    for block in message.as_chunks::<64>().0 {
        // Section 6.1, method 1.
        let mut w = [0u32; 80];
        for (t, word) in block.as_chunks::<4>().0.iter().enumerate() {
            w[t] = u32::from_be_bytes(*word);
        }
        for t in 16..80 {
            w[t] = (w[t - 3] ^ w[t - 8] ^ w[t - 14] ^ w[t - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (t, word) in w.iter().enumerate() {
            // Section 5: the functions and constants of each round.
            let (f, k) = match t {
                0..=19 => ((b & c) | (!b & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(*word)
                .wrapping_add(k);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        for (slot, add) in h.iter_mut().zip([a, b, c, d, e]) {
            *slot = slot.wrapping_add(add);
        }
    }
    let mut out = [0u8; 20];
    for (chunk, word) in out.as_chunks_mut::<4>().0.iter_mut().zip(h) {
        *chunk = word.to_be_bytes();
    }
    out
}

/// Base64 with padding, RFC 4648 section 4
/// (https://www.rfc-editor.org/rfc/rfc4648, read 2026-10-03).
fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        let sextet = |shift: u32| ALPHABET[((n >> shift) & 63) as usize] as char;
        out.push(sextet(18));
        out.push(sextet(12));
        out.push(if chunk.len() > 1 { sextet(6) } else { '=' });
        out.push(if chunk.len() > 2 { sextet(0) } else { '=' });
    }
    out
}

/// The `Sec-WebSocket-Key` for a 16-byte nonce (section 4.1: "a randomly
/// selected 16-byte value that has been base64-encoded").
pub fn client_key(nonce: [u8; 16]) -> String {
    base64(&nonce)
}

/// The `Sec-WebSocket-Accept` a server must answer a key with (section 1.3,
/// section 4.2.2 step 5.4): base64 of the SHA-1 of the key, as text, followed
/// by the GUID.
pub fn accept_for(key: &str) -> String {
    base64(&sha1(format!("{}{GUID}", key.trim()).as_bytes()))
}

/// The client's opening handshake (section 4.1), ending in the empty line.
///
/// `host` is the `Host` header's value (`127.0.0.1:4455`), `path` the
/// resource name (`/`). No `Origin` (this is not a browser), no extension,
/// no subprotocol.
pub fn handshake_request(host: &str, path: &str, key: &str) -> String {
    format!(
        "GET {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: {key}\r\n\
         Sec-WebSocket-Version: 13\r\n\
         \r\n"
    )
}

/// Why a handshake failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandshakeError {
    /// The status line is not `HTTP/1.1 101`; carries the line.
    NotSwitching(String),
    /// No `Upgrade: websocket` header (section 4.1, "an |Upgrade| header
    /// field ... ASCII case-insensitive match for the value 'websocket'").
    NoUpgrade,
    /// No `Connection` header with the `Upgrade` token.
    NoConnectionUpgrade,
    /// `Sec-WebSocket-Accept` is absent or is not the value for the key.
    BadAccept,
    /// The server named an extension; none was offered.
    Extension(String),
    /// The server named a subprotocol; none was offered.
    Subprotocol(String),
    /// A request that is not a WebSocket upgrade (the server side's check).
    BadRequest(String),
    /// The head is not text, or a header line has no colon.
    Malformed,
}

impl std::fmt::Display for HandshakeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HandshakeError::NotSwitching(line) => {
                write!(f, "the server did not switch protocols: {line}")
            }
            HandshakeError::NoUpgrade => write!(f, "no Upgrade: websocket header"),
            HandshakeError::NoConnectionUpgrade => write!(f, "no Connection: Upgrade header"),
            HandshakeError::BadAccept => {
                write!(f, "Sec-WebSocket-Accept is missing or wrong for the key")
            }
            HandshakeError::Extension(e) => write!(f, "an extension nobody offered: {e}"),
            HandshakeError::Subprotocol(p) => write!(f, "a subprotocol nobody offered: {p}"),
            HandshakeError::BadRequest(why) => write!(f, "not a WebSocket upgrade: {why}"),
            HandshakeError::Malformed => write!(f, "a malformed HTTP head"),
        }
    }
}

impl std::error::Error for HandshakeError {}

/// Where an HTTP head ends in `buffer`: the index just past the first empty
/// line, if it has arrived. A caller reads until this is `Some` or the
/// buffer passes [`MAX_HANDSHAKE`]; bytes past the index are frames.
pub fn head_end(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|i| i + 4)
}

/// The header lines of a head as `(lower-case name, trimmed value)`.
fn headers(head: &str) -> Result<Vec<(String, String)>, HandshakeError> {
    let mut out = Vec::new();
    for line in head.split("\r\n").skip(1) {
        if line.is_empty() {
            break;
        }
        let (name, value) = line.split_once(':').ok_or(HandshakeError::Malformed)?;
        out.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
    }
    Ok(out)
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.as_str())
}

/// Whether a comma-separated header value carries a token, ASCII
/// case-insensitively.
fn has_token(value: &str, token: &str) -> bool {
    value
        .split(',')
        .any(|t| t.trim().eq_ignore_ascii_case(token))
}

/// Check the server's answer to the opening handshake (section 4.1, the
/// client's validation steps 1 to 6): status 101, `Upgrade: websocket`,
/// `Connection: Upgrade`, the right `Sec-WebSocket-Accept` for `key`, and no
/// extension or subprotocol (none was offered).
pub fn check_handshake_response(head: &[u8], key: &str) -> Result<(), HandshakeError> {
    let head = std::str::from_utf8(head).map_err(|_| HandshakeError::Malformed)?;
    let status = head.split("\r\n").next().unwrap_or("");
    let mut parts = status.split(' ');
    let version = parts.next().unwrap_or("");
    let code = parts.next().unwrap_or("");
    if !version.starts_with("HTTP/1.") || code != "101" {
        return Err(HandshakeError::NotSwitching(
            status.chars().take(80).collect(),
        ));
    }
    let headers = headers(head)?;
    if !header(&headers, "upgrade").is_some_and(|v| v.eq_ignore_ascii_case("websocket")) {
        return Err(HandshakeError::NoUpgrade);
    }
    if !header(&headers, "connection").is_some_and(|v| has_token(v, "upgrade")) {
        return Err(HandshakeError::NoConnectionUpgrade);
    }
    if header(&headers, "sec-websocket-accept") != Some(accept_for(key).as_str()) {
        return Err(HandshakeError::BadAccept);
    }
    if let Some(extension) = header(&headers, "sec-websocket-extensions") {
        return Err(HandshakeError::Extension(extension.to_string()));
    }
    if let Some(protocol) = header(&headers, "sec-websocket-protocol") {
        return Err(HandshakeError::Subprotocol(protocol.to_string()));
    }
    Ok(())
}

/// The server side: read a client's opening handshake (section 4.2.1) and
/// return its `Sec-WebSocket-Key`. For test servers; chorus is only ever a
/// client of Soloist.
pub fn read_handshake_request(head: &[u8]) -> Result<String, HandshakeError> {
    let head = std::str::from_utf8(head).map_err(|_| HandshakeError::Malformed)?;
    let bad = |why: &str| HandshakeError::BadRequest(why.to_string());
    let request = head.split("\r\n").next().unwrap_or("");
    let mut parts = request.split(' ');
    if parts.next() != Some("GET") {
        return Err(bad("not a GET"));
    }
    let _path = parts.next().ok_or_else(|| bad("no request target"))?;
    if !parts.next().is_some_and(|v| v.starts_with("HTTP/1.")) {
        return Err(bad("not HTTP/1.1"));
    }
    let headers = headers(head)?;
    if !header(&headers, "upgrade").is_some_and(|v| has_token(v, "websocket")) {
        return Err(bad("no Upgrade: websocket"));
    }
    if !header(&headers, "connection").is_some_and(|v| has_token(v, "upgrade")) {
        return Err(bad("no Connection: Upgrade"));
    }
    if header(&headers, "sec-websocket-version") != Some("13") {
        return Err(bad("Sec-WebSocket-Version is not 13"));
    }
    match header(&headers, "sec-websocket-key") {
        Some(key) if !key.is_empty() => Ok(key.to_string()),
        _ => Err(bad("no Sec-WebSocket-Key")),
    }
}

/// The server's answer to a valid opening handshake (section 4.2.2).
pub fn handshake_response(key: &str) -> String {
    format!(
        "HTTP/1.1 101 Switching Protocols\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Accept: {}\r\n\
         \r\n",
        accept_for(key)
    )
}

/// A frame's opcode (section 5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opcode {
    /// 0x0: a continuation of a fragmented message.
    Continuation,
    /// 0x1: text.
    Text,
    /// 0x2: binary.
    Binary,
    /// 0x8: close.
    Close,
    /// 0x9: ping.
    Ping,
    /// 0xA: pong.
    Pong,
}

impl Opcode {
    fn bits(self) -> u8 {
        match self {
            Opcode::Continuation => 0x0,
            Opcode::Text => 0x1,
            Opcode::Binary => 0x2,
            Opcode::Close => 0x8,
            Opcode::Ping => 0x9,
            Opcode::Pong => 0xA,
        }
    }

    fn from_bits(bits: u8) -> Option<Opcode> {
        Some(match bits {
            0x0 => Opcode::Continuation,
            0x1 => Opcode::Text,
            0x2 => Opcode::Binary,
            0x8 => Opcode::Close,
            0x9 => Opcode::Ping,
            0xA => Opcode::Pong,
            _ => return None,
        })
    }

    fn is_control(self) -> bool {
        matches!(self, Opcode::Close | Opcode::Ping | Opcode::Pong)
    }
}

/// One frame on the wire (section 5.2). With a mask the payload is masked
/// (section 5.3) and the mask bit set, as every client frame must be
/// (section 5.1); a server passes `None`.
pub fn encode_frame(fin: bool, opcode: Opcode, payload: &[u8], mask: Option<[u8; 4]>) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 14);
    out.push(if fin { 0x80 } else { 0 } | opcode.bits());
    let mask_bit = if mask.is_some() { 0x80 } else { 0 };
    match payload.len() {
        // Each arm's range is what makes its conversion exact.
        n @ 0..=125 => out.push(mask_bit | n as u8),
        n @ 126..=0xFFFF => {
            out.push(mask_bit | 126);
            out.extend_from_slice(&(n as u16).to_be_bytes());
        }
        n => {
            out.push(mask_bit | 127);
            out.extend_from_slice(&(n as u64).to_be_bytes());
        }
    }
    match mask {
        Some(key) => {
            out.extend_from_slice(&key);
            out.extend(payload.iter().enumerate().map(|(i, b)| b ^ key[i % 4]));
        }
        None => out.extend_from_slice(payload),
    }
    out
}

/// A whole text message as one masked client frame.
pub fn client_text(text: &str, mask: [u8; 4]) -> Vec<u8> {
    encode_frame(true, Opcode::Text, text.as_bytes(), Some(mask))
}

/// The pong answering a ping: the ping's application data, masked
/// (section 5.5.3). A ping's payload is at most 125 bytes; a longer slice is
/// cut to that.
pub fn client_pong(ping_payload: &[u8], mask: [u8; 4]) -> Vec<u8> {
    let payload = &ping_payload[..ping_payload.len().min(125)];
    encode_frame(true, Opcode::Pong, payload, Some(mask))
}

/// A masked close frame with a status code and no reason (section 5.5.1;
/// 1000 is a normal closure, section 7.4.1).
pub fn client_close(code: u16, mask: [u8; 4]) -> Vec<u8> {
    encode_frame(true, Opcode::Close, &code.to_be_bytes(), Some(mask))
}

/// Which side a [`Decoder`] is: it decides which frames must be masked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Reading a server's frames: they must not be masked (section 5.1).
    Client,
    /// Reading a client's frames: they must be masked (section 5.1).
    Server,
}

/// One complete message, or a control frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// A text message, fragments joined.
    Text(String),
    /// A binary message, fragments joined.
    Binary(Vec<u8>),
    /// A ping; answer it with a pong carrying the same data.
    Ping(Vec<u8>),
    /// A pong.
    Pong(Vec<u8>),
    /// A close frame: its status code, if it carried one, and its reason.
    Close {
        /// The status code (section 7.4).
        code: Option<u16>,
        /// The reason, lossily decoded.
        reason: String,
    },
}

/// Why a byte stream is not WebSocket frames this decoder accepts. Every one
/// of them ends the connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsError {
    /// A reserved bit is set; no extension was negotiated (section 5.2).
    ReservedBits,
    /// An opcode section 5.2 does not define.
    UnknownOpcode(u8),
    /// A server frame that is masked, or a client frame that is not
    /// (section 5.1).
    Masking,
    /// A control frame longer than 125 bytes or fragmented (section 5.5).
    ControlFrame,
    /// A continuation with nothing to continue, or a new data frame inside a
    /// fragmented message (section 5.4).
    Fragmentation,
    /// A message longer than the decoder's bound; carries the bound.
    TooLong(usize),
    /// A text message that is not UTF-8 (section 8.1).
    NotUtf8,
    /// A close frame with a one-byte body (section 5.5.1).
    BadClose,
}

impl std::fmt::Display for WsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WsError::ReservedBits => write!(f, "a reserved frame bit is set"),
            WsError::UnknownOpcode(op) => write!(f, "unknown opcode {op:#x}"),
            WsError::Masking => write!(f, "a frame is masked the wrong way for its sender"),
            WsError::ControlFrame => write!(f, "a control frame is fragmented or over 125 bytes"),
            WsError::Fragmentation => write!(f, "fragments out of order"),
            WsError::TooLong(max) => write!(f, "a message is longer than {max} bytes"),
            WsError::NotUtf8 => write!(f, "a text message is not UTF-8"),
            WsError::BadClose => write!(f, "a close frame with a one-byte body"),
        }
    }
}

impl std::error::Error for WsError {}

/// A streaming frame decoder: feed it bytes as they arrive, take messages
/// out.
#[derive(Debug)]
pub struct Decoder {
    role: Role,
    max_message: usize,
    buffer: Vec<u8>,
    /// The message being reassembled: whether it is text, and its bytes.
    partial: Option<(bool, Vec<u8>)>,
}

impl Decoder {
    /// A decoder for one side with a bound on one message's payload.
    pub fn new(role: Role, max_message: usize) -> Decoder {
        Decoder {
            role,
            max_message,
            buffer: Vec::new(),
            partial: None,
        }
    }

    /// Add bytes read from the connection.
    pub fn feed(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    /// The next complete message or control frame, `Ok(None)` when more
    /// bytes are needed. After an error the connection is over: do not call
    /// again.
    pub fn next_message(&mut self) -> Result<Option<Message>, WsError> {
        loop {
            let Some((fin, opcode, payload)) = self.next_frame()? else {
                return Ok(None);
            };
            match opcode {
                Opcode::Ping => return Ok(Some(Message::Ping(payload))),
                Opcode::Pong => return Ok(Some(Message::Pong(payload))),
                Opcode::Close => {
                    return match payload.len() {
                        0 => Ok(Some(Message::Close {
                            code: None,
                            reason: String::new(),
                        })),
                        1 => Err(WsError::BadClose),
                        _ => Ok(Some(Message::Close {
                            code: Some(u16::from_be_bytes([payload[0], payload[1]])),
                            reason: String::from_utf8_lossy(&payload[2..]).into_owned(),
                        })),
                    };
                }
                Opcode::Text | Opcode::Binary => {
                    if self.partial.is_some() {
                        return Err(WsError::Fragmentation);
                    }
                    self.partial = Some((opcode == Opcode::Text, payload));
                }
                Opcode::Continuation => match &mut self.partial {
                    None => return Err(WsError::Fragmentation),
                    Some((_, bytes)) => {
                        if bytes.len() + payload.len() > self.max_message {
                            return Err(WsError::TooLong(self.max_message));
                        }
                        bytes.extend_from_slice(&payload);
                    }
                },
            }
            if fin {
                if let Some((text, bytes)) = self.partial.take() {
                    return if text {
                        String::from_utf8(bytes)
                            .map(|t| Some(Message::Text(t)))
                            .map_err(|_| WsError::NotUtf8)
                    } else {
                        Ok(Some(Message::Binary(bytes)))
                    };
                }
            }
        }
    }

    /// One whole frame off the front of the buffer, unmasked.
    fn next_frame(&mut self) -> Result<Option<(bool, Opcode, Vec<u8>)>, WsError> {
        let b = &self.buffer;
        if b.len() < 2 {
            return Ok(None);
        }
        if b[0] & 0x70 != 0 {
            return Err(WsError::ReservedBits);
        }
        let fin = b[0] & 0x80 != 0;
        let opcode = Opcode::from_bits(b[0] & 0x0F).ok_or(WsError::UnknownOpcode(b[0] & 0x0F))?;
        let masked = b[1] & 0x80 != 0;
        if masked != (self.role == Role::Server) {
            return Err(WsError::Masking);
        }
        let short = usize::from(b[1] & 0x7F);
        if opcode.is_control() && (!fin || short > 125) {
            return Err(WsError::ControlFrame);
        }
        let (length, mut at) = match short {
            126 => {
                if b.len() < 4 {
                    return Ok(None);
                }
                (u64::from(u16::from_be_bytes([b[2], b[3]])), 4)
            }
            127 => {
                if b.len() < 10 {
                    return Ok(None);
                }
                let mut wide = [0u8; 8];
                wide.copy_from_slice(&b[2..10]);
                (u64::from_be_bytes(wide), 10)
            }
            n => (n as u64, 2),
        };
        // The bound is checked on the declared length, before any payload is
        // buffered, so a peer cannot make this side hold more than the bound.
        if length > self.max_message as u64 {
            return Err(WsError::TooLong(self.max_message));
        }
        // At most max_message, a usize, by the check above.
        let length = length as usize;
        let key = if masked {
            if b.len() < at + 4 {
                return Ok(None);
            }
            let key = [b[at], b[at + 1], b[at + 2], b[at + 3]];
            at += 4;
            Some(key)
        } else {
            None
        };
        if b.len() < at + length {
            return Ok(None);
        }
        let mut payload = b[at..at + length].to_vec();
        if let Some(key) = key {
            for (i, byte) in payload.iter_mut().enumerate() {
                *byte ^= key[i % 4];
            }
        }
        self.buffer.drain(..at + length);
        Ok(Some((fin, opcode, payload)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// RFC 3174 section 7.3: TEST1 and TEST2 and their digests.
    #[test]
    fn sha1_matches_rfc_3174() {
        assert_eq!(
            hex(&sha1(b"abc")),
            "A9 99 3E 36 47 06 81 6A BA 3E 25 71 78 50 C2 6C 9C D0 D8 9D"
        );
        assert_eq!(
            hex(&sha1(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "84 98 3E 44 1C 3B D2 6E BA AE 4A A1 F9 51 29 E5 E5 46 70 F1"
        );
    }

    /// RFC 4648 section 10.
    #[test]
    fn base64_matches_rfc_4648() {
        for (plain, coded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(plain.as_bytes()), coded);
        }
    }

    /// RFC 6455 section 1.3: the sample nonce and the accept value it gives.
    #[test]
    fn the_accept_value_of_section_1_3() {
        assert_eq!(client_key(*b"the sample nonce"), "dGhlIHNhbXBsZSBub25jZQ==");
        assert_eq!(
            accept_for("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }

    /// The key of section 1.3's example, made from its nonce (the literal
    /// is asserted once, in the test above).
    fn sample() -> String {
        client_key(*b"the sample nonce")
    }

    #[test]
    fn the_request_is_the_handshake_of_section_4_1() {
        let sample = sample();
        let request = handshake_request("127.0.0.1:4455", "/", &sample);
        assert_eq!(
            request,
            format!(
                "GET / HTTP/1.1\r\nHost: 127.0.0.1:4455\r\nUpgrade: websocket\r\n\
                 Connection: Upgrade\r\nSec-WebSocket-Key: {sample}\r\n\
                 Sec-WebSocket-Version: 13\r\n\r\n"
            )
        );
        assert_eq!(head_end(request.as_bytes()), Some(request.len()));
        assert_eq!(
            read_handshake_request(request.as_bytes()),
            Ok(sample.clone())
        );
        let response = handshake_response(&sample);
        assert!(response.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n"));
        assert_eq!(
            check_handshake_response(response.as_bytes(), &sample),
            Ok(())
        );
    }

    #[test]
    fn a_response_is_checked_field_by_field() {
        let sample = sample();
        let good = "HTTP/1.1 101 Switching Protocols\r\nupgrade: WebSocket\r\n\
                    CONNECTION: keep-alive, Upgrade\r\n\
                    Sec-WebSocket-Accept:   s3pPLMBiTxaQ9kYGzzhZRbK+xOo=  \r\n\r\n";
        assert_eq!(check_handshake_response(good.as_bytes(), &sample), Ok(()));
        let check = |head: &str| check_handshake_response(head.as_bytes(), &sample);
        assert!(matches!(
            check(&good.replace("101", "200")),
            Err(HandshakeError::NotSwitching(_))
        ));
        assert!(matches!(
            check("HTTP/1.1 400 Bad Request\r\n\r\n"),
            Err(HandshakeError::NotSwitching(_))
        ));
        assert_eq!(
            check(&good.replace("upgrade: WebSocket\r\n", "")),
            Err(HandshakeError::NoUpgrade)
        );
        assert_eq!(
            check(&good.replace("keep-alive, Upgrade", "keep-alive")),
            Err(HandshakeError::NoConnectionUpgrade)
        );
        assert_eq!(
            check(&good.replace("s3pP", "s3pQ")),
            Err(HandshakeError::BadAccept)
        );
        assert_eq!(
            check_handshake_response(good.as_bytes(), &client_key([0; 16])),
            Err(HandshakeError::BadAccept)
        );
        assert_eq!(
            check(&good.replace(
                "\r\n\r\n",
                "\r\nSec-WebSocket-Extensions: permessage-deflate\r\n\r\n"
            )),
            Err(HandshakeError::Extension("permessage-deflate".into()))
        );
        assert_eq!(
            check(&good.replace("\r\n\r\n", "\r\nSec-WebSocket-Protocol: chat\r\n\r\n")),
            Err(HandshakeError::Subprotocol("chat".into()))
        );
        assert_eq!(
            check(&good.replace("upgrade: WebSocket", "no colon here")),
            Err(HandshakeError::Malformed)
        );
        assert_eq!(
            check_handshake_response(&[0xFF, 0xFE], &sample),
            Err(HandshakeError::Malformed)
        );
    }

    #[test]
    fn a_request_that_is_not_an_upgrade_is_refused() {
        let good = handshake_request("h", "/", "k");
        for (from, to) in [
            ("GET", "POST"),
            ("Upgrade: websocket\r\n", ""),
            ("Connection: Upgrade\r\n", ""),
            ("Version: 13", "Version: 8"),
            ("Sec-WebSocket-Key: k\r\n", ""),
        ] {
            let bad = good.replace(from, to);
            assert!(
                matches!(
                    read_handshake_request(bad.as_bytes()),
                    Err(HandshakeError::BadRequest(_))
                ),
                "{from}"
            );
        }
    }

    fn all(decoder: &mut Decoder) -> Vec<Message> {
        let mut out = Vec::new();
        while let Some(message) = decoder.next_message().expect("valid frames") {
            out.push(message);
        }
        out
    }

    /// RFC 6455 section 5.7, every example.
    #[test]
    fn the_frames_of_section_5_7() {
        // "A single-frame unmasked text message".
        let unmasked = [0x81, 0x05, 0x48, 0x65, 0x6c, 0x6c, 0x6f];
        assert_eq!(encode_frame(true, Opcode::Text, b"Hello", None), unmasked);
        let mut client = Decoder::new(Role::Client, 1024);
        client.feed(&unmasked);
        assert_eq!(all(&mut client), vec![Message::Text("Hello".into())]);

        // "A single-frame masked text message".
        let masked = [
            0x81, 0x85, 0x37, 0xfa, 0x21, 0x3d, 0x7f, 0x9f, 0x4d, 0x51, 0x58,
        ];
        assert_eq!(client_text("Hello", [0x37, 0xfa, 0x21, 0x3d]), masked);
        let mut server = Decoder::new(Role::Server, 1024);
        server.feed(&masked);
        assert_eq!(all(&mut server), vec![Message::Text("Hello".into())]);

        // "A fragmented unmasked text message".
        let first = [0x01, 0x03, 0x48, 0x65, 0x6c];
        let second = [0x80, 0x02, 0x6c, 0x6f];
        assert_eq!(encode_frame(false, Opcode::Text, b"Hel", None), first);
        assert_eq!(
            encode_frame(true, Opcode::Continuation, b"lo", None),
            second
        );
        client.feed(&first);
        assert_eq!(all(&mut client), vec![]);
        client.feed(&second);
        assert_eq!(all(&mut client), vec![Message::Text("Hello".into())]);

        // "Unmasked Ping request and masked Ping response".
        let ping = [0x89, 0x05, 0x48, 0x65, 0x6c, 0x6c, 0x6f];
        let pong = [
            0x8a, 0x85, 0x37, 0xfa, 0x21, 0x3d, 0x7f, 0x9f, 0x4d, 0x51, 0x58,
        ];
        client.feed(&ping);
        assert_eq!(all(&mut client), vec![Message::Ping(b"Hello".to_vec())]);
        assert_eq!(client_pong(b"Hello", [0x37, 0xfa, 0x21, 0x3d]), pong);
        server.feed(&pong);
        assert_eq!(all(&mut server), vec![Message::Pong(b"Hello".to_vec())]);

        // "256 bytes binary message in a single unmasked frame".
        let data = vec![0xABu8; 256];
        let frame = encode_frame(true, Opcode::Binary, &data, None);
        assert_eq!(frame[..4], [0x82, 0x7E, 0x01, 0x00]);
        assert_eq!(frame.len(), 4 + 256);
        client.feed(&frame);
        assert_eq!(all(&mut client), vec![Message::Binary(data)]);

        // "64KiB binary message in a single unmasked frame".
        let data = vec![0xCDu8; 65536];
        let frame = encode_frame(true, Opcode::Binary, &data, None);
        assert_eq!(
            frame[..10],
            [0x82, 0x7F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00]
        );
        let mut big = Decoder::new(Role::Client, 65536);
        big.feed(&frame);
        assert_eq!(all(&mut big), vec![Message::Binary(data)]);
    }

    #[test]
    fn frames_arrive_a_byte_at_a_time() {
        let mut bytes = encode_frame(false, Opcode::Text, "caf".as_bytes(), None);
        bytes.extend(encode_frame(true, Opcode::Ping, b"p", None));
        bytes.extend(encode_frame(
            true,
            Opcode::Continuation,
            "\u{e9}!".as_bytes(),
            None,
        ));
        bytes.extend(encode_frame(true, Opcode::Text, &vec![b'x'; 300], None));
        bytes.extend(encode_frame(
            true,
            Opcode::Close,
            &[0x03, 0xE8, b'o', b'k'],
            None,
        ));
        let mut decoder = Decoder::new(Role::Client, 1024);
        let mut got = Vec::new();
        for byte in bytes {
            decoder.feed(&[byte]);
            got.extend(all(&mut decoder));
        }
        assert_eq!(
            got,
            vec![
                Message::Ping(b"p".to_vec()),
                Message::Text("caf\u{e9}!".into()),
                Message::Text("x".repeat(300)),
                Message::Close {
                    code: Some(1000),
                    reason: "ok".into()
                },
            ]
        );
    }

    #[test]
    fn a_masked_frame_round_trips_at_every_length_class() {
        for length in [0usize, 1, 125, 126, 127, 65535, 65536, 70000] {
            let text: String = (0..length)
                .map(|i| (b'a' + (i % 26) as u8) as char)
                .collect();
            let frame = client_text(&text, [1, 2, 3, 4]);
            assert_eq!(frame[1] & 0x80, 0x80, "the mask bit");
            if length > 4 {
                assert!(!frame.windows(text.len()).any(|w| w == text.as_bytes()));
            }
            let mut server = Decoder::new(Role::Server, 1 << 20);
            server.feed(&frame);
            assert_eq!(all(&mut server), vec![Message::Text(text)]);
        }
    }

    fn error_of(role: Role, max: usize, bytes: &[u8]) -> WsError {
        let mut decoder = Decoder::new(role, max);
        decoder.feed(bytes);
        loop {
            match decoder.next_message() {
                Err(e) => return e,
                Ok(Some(_)) => {}
                Ok(None) => panic!("no error from {bytes:02x?}"),
            }
        }
    }

    #[test]
    fn what_the_decoder_refuses() {
        // A reserved bit (section 5.2).
        assert_eq!(
            error_of(Role::Client, 64, &[0xC1, 0x00]),
            WsError::ReservedBits
        );
        // Opcode 0x3 is reserved.
        assert_eq!(
            error_of(Role::Client, 64, &[0x83, 0x00]),
            WsError::UnknownOpcode(3)
        );
        // A masked frame from a server, an unmasked one from a client (5.1).
        assert_eq!(
            error_of(Role::Client, 64, &client_text("x", [0; 4])),
            WsError::Masking
        );
        assert_eq!(
            error_of(
                Role::Server,
                64,
                &encode_frame(true, Opcode::Text, b"x", None)
            ),
            WsError::Masking
        );
        // A fragmented ping, and a ping of 126 bytes (5.5).
        assert_eq!(
            error_of(Role::Client, 64, &[0x09, 0x00]),
            WsError::ControlFrame
        );
        assert_eq!(
            error_of(
                Role::Client,
                1024,
                &encode_frame(true, Opcode::Ping, &[0; 126], None)
            ),
            WsError::ControlFrame
        );
        // A continuation of nothing; a text frame inside a fragmented text (5.4).
        assert_eq!(
            error_of(Role::Client, 64, &[0x80, 0x00]),
            WsError::Fragmentation
        );
        let mut bytes = encode_frame(false, Opcode::Text, b"a", None);
        bytes.extend(encode_frame(true, Opcode::Text, b"b", None));
        assert_eq!(error_of(Role::Client, 64, &bytes), WsError::Fragmentation);
        // The bound, on one frame's declared length (before its payload
        // arrives) and over fragments.
        assert_eq!(
            error_of(Role::Client, 64, &[0x81, 0x7F, 0, 0, 0, 1, 0, 0, 0, 0]),
            WsError::TooLong(64)
        );
        let mut bytes = encode_frame(false, Opcode::Text, &[b'a'; 40], None);
        bytes.extend(encode_frame(true, Opcode::Continuation, &[b'a'; 40], None));
        assert_eq!(error_of(Role::Client, 64, &bytes), WsError::TooLong(64));
        // Text that is not UTF-8 (8.1); a close with half a status code.
        assert_eq!(
            error_of(
                Role::Client,
                64,
                &encode_frame(true, Opcode::Text, &[0xFF], None)
            ),
            WsError::NotUtf8
        );
        assert_eq!(
            error_of(
                Role::Client,
                64,
                &encode_frame(true, Opcode::Close, &[3], None)
            ),
            WsError::BadClose
        );
    }

    #[test]
    fn close_frames() {
        assert_eq!(
            client_close(1000, [0; 4]),
            [0x88, 0x82, 0, 0, 0, 0, 0x03, 0xE8]
        );
        let mut decoder = Decoder::new(Role::Client, 64);
        decoder.feed(&[0x88, 0x00]);
        assert_eq!(
            all(&mut decoder),
            vec![Message::Close {
                code: None,
                reason: String::new()
            }]
        );
    }
}
