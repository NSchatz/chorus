//! A blocking HTTP/1.1 client for one thing: `GET` a media URL and stream its
//! body. Hand-written over `std::net`, as the rest of chorus's HTTP is.
//!
//! One request per connection (`Connection: close`). The message syntax is RFC
//! 9112 (https://www.rfc-editor.org/rfc/rfc9112, read 2026-10-03): the status
//! line (section 4), header fields (section 5), and the three ways a body ends
//! (section 6.3): a chunked transfer coding (section 7.1), a `Content-Length`,
//! or the close of the connection. Redirects and `Range` are RFC 9110
//! (https://www.rfc-editor.org/rfc/rfc9110, sections 15.4 and 14, read
//! 2026-10-03). The one thing outside those: a SHOUTcast server's `ICY 200 OK`
//! status line, accepted as an HTTP/1.0 answer.
//!
//! Every connect goes through the fetch policy with the address the name
//! resolved to, and connects to that address.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Instant;

use crate::error::FetchError;
use crate::policy::{check_address, is_own_address, Policy};
use crate::tls;
use crate::url::{Scheme, Url};

/// The `User-Agent` chorus sends.
pub const USER_AGENT: &str = concat!("chorus/", env!("CARGO_PKG_VERSION"));

/// How many of a name's allowed addresses are tried before giving up.
/// ASSUMED: enough for an IPv6 address that does not route and the IPv4 one
/// behind it, without multiplying the connect timeout by a long answer.
const MAX_CONNECT_ATTEMPTS: usize = 3;

/// The longest chunk-size line and the most trailer bytes accepted in a
/// chunked body. ASSUMED: a chunk size is a few hex digits and trailers are
/// rare; both bounds only stop a peer that never ends a line.
const MAX_CHUNK_LINE: usize = 256;
const MAX_TRAILER_BYTES: usize = 8 * 1024;

/// The size of the read buffer in front of the socket. ASSUMED: about one TLS
/// record; it bounds what is read ahead of the caller, it does not bound the
/// body.
const BUFFER_BYTES: usize = 16 * 1024;

/// What a fetch carries from request to request: the policy, and the TLS
/// client configuration once an https URL needed it (the roots are read once
/// per stream, not once per HLS segment).
pub(crate) struct Ctx {
    pub policy: Policy,
    tls: Option<Arc<rustls::ClientConfig>>,
}

impl Ctx {
    pub fn new(policy: &Policy) -> Ctx {
        Ctx {
            policy: policy.clone(),
            tls: None,
        }
    }

    fn tls_config(&mut self) -> Result<Arc<rustls::ClientConfig>, FetchError> {
        if let Some(config) = &self.tls {
            return Ok(Arc::clone(config));
        }
        let config = tls::client_config(&self.policy)?;
        self.tls = Some(Arc::clone(&config));
        Ok(config)
    }
}

/// The socket, plain or under TLS.
pub(crate) enum Transport {
    Plain(TcpStream),
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>),
}

impl Read for Transport {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let got = match self {
            Transport::Plain(sock) => sock.read(buf),
            // A peer that closes the socket without a TLS close_notify is an
            // end of stream here, as it is for plain HTTP: a body with a
            // length or a chunked coding still notices that it was cut short,
            // and a body that ends at the close has no other end to offer.
            Transport::Tls(tls) => match tls.read(buf) {
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => Ok(0),
                other => other,
            },
        };
        got.map_err(timeout_named)
    }
}

impl Write for Transport {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Transport::Plain(sock) => sock.write(buf),
            Transport::Tls(tls) => tls.write(buf),
        }
        .map_err(timeout_named)
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Transport::Plain(sock) => sock.flush(),
            Transport::Tls(tls) => tls.flush(),
        }
    }
}

/// A socket timeout surfaces as `WouldBlock` on Linux; callers see `TimedOut`.
fn timeout_named(e: io::Error) -> io::Error {
    match e.kind() {
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => io::Error::new(
            io::ErrorKind::TimedOut,
            "the server sent nothing within the read timeout",
        ),
        _ => e,
    }
}

/// A reader with a buffer in front of it, so the head can be read a byte at a
/// time and what follows the head is still there for the body.
pub(crate) struct Buffered<R> {
    inner: R,
    buf: Box<[u8]>,
    start: usize,
    end: usize,
}

impl<R: Read> Buffered<R> {
    pub fn new(inner: R) -> Buffered<R> {
        Buffered {
            inner,
            buf: vec![0; BUFFER_BYTES].into_boxed_slice(),
            start: 0,
            end: 0,
        }
    }

    /// The next byte, or `None` at the end of the stream.
    fn byte(&mut self) -> io::Result<Option<u8>> {
        if self.start == self.end {
            self.start = 0;
            self.end = self.inner.read(&mut self.buf)?;
            if self.end == 0 {
                return Ok(None);
            }
        }
        let b = self.buf[self.start];
        self.start += 1;
        Ok(Some(b))
    }

    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.start < self.end {
            let n = out.len().min(self.end - self.start);
            out[..n].copy_from_slice(&self.buf[self.start..self.start + n]);
            self.start += n;
            return Ok(n);
        }
        self.inner.read(out)
    }

    /// One line without its terminator (CRLF or a bare LF), or `None` when
    /// the stream ends before a line does. `budget` is decremented by the
    /// bytes consumed; running out is `Err(None)`.
    fn line(&mut self, budget: &mut usize) -> Result<Option<Vec<u8>>, Option<io::Error>> {
        let mut line = Vec::new();
        loop {
            if *budget == 0 {
                return Err(None);
            }
            match self.byte().map_err(Some)? {
                None => return Ok(None),
                Some(b'\n') => {
                    *budget -= 1;
                    if line.last() == Some(&b'\r') {
                        line.pop();
                    }
                    return Ok(Some(line));
                }
                Some(b) => {
                    *budget -= 1;
                    line.push(b);
                }
            }
        }
    }
}

/// A response's header fields, names lower-cased, in the order received.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Headers(Vec<(String, String)>);

impl Headers {
    /// The first field of that (lower-case) name.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> {
        self.0
            .iter()
            .filter(move |(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// A response's status line and header fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Head {
    pub status: u16,
    /// HTTP/1.1 or later: false for HTTP/1.0 and for `ICY`.
    pub http11: bool,
    pub headers: Headers,
}

/// Reads a response head. `deadline` bounds the whole head, so a server that
/// sends a byte now and then cannot hold a fetch for the header bound times
/// the read timeout.
pub(crate) fn read_head<R: Read>(
    conn: &mut Buffered<R>,
    max_header_bytes: usize,
    deadline: Instant,
) -> Result<Head, FetchError> {
    let mut budget = max_header_bytes;
    let mut next_line = |conn: &mut Buffered<R>| -> Result<Vec<u8>, FetchError> {
        if Instant::now() > deadline {
            return Err(FetchError::Io(io::Error::new(
                io::ErrorKind::TimedOut,
                "the response headers did not arrive within the read timeout",
            )));
        }
        match conn.line(&mut budget) {
            Ok(Some(line)) => Ok(line),
            Ok(None) => Err(FetchError::Io(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "the connection closed before the response headers ended",
            ))),
            Err(Some(e)) => Err(FetchError::Io(e)),
            Err(None) => Err(FetchError::Refused(format!(
                "response headers larger than {max_header_bytes} bytes"
            ))),
        }
    };
    let status_line = next_line(conn)?;
    let (status, http11) = parse_status_line(&status_line)?;
    let mut fields: Vec<(String, String)> = Vec::new();
    loop {
        let line = next_line(conn)?;
        if line.is_empty() {
            break;
        }
        let text = String::from_utf8_lossy(&line);
        if line[0] == b' ' || line[0] == b'\t' {
            // A folded line (obsolete, RFC 9112 section 5.2) continues the
            // field before it.
            if let Some((_, value)) = fields.last_mut() {
                value.push(' ');
                value.push_str(text.trim());
            }
            continue;
        }
        // A line with no colon is not a field; old stream servers send them.
        if let Some((name, value)) = text.split_once(':') {
            fields.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
    }
    Ok(Head {
        status,
        http11,
        headers: Headers(fields),
    })
}

/// `HTTP/1.1 200 OK`, `HTTP/1.0 200 OK` or SHOUTcast's `ICY 200 OK`.
fn parse_status_line(line: &[u8]) -> Result<(u16, bool), FetchError> {
    let bad = || {
        FetchError::Malformed(format!(
            "http: not a status line: {:?}",
            String::from_utf8_lossy(&line[..line.len().min(40)])
        ))
    };
    let text = std::str::from_utf8(line).map_err(|_| bad())?;
    let mut parts = text.splitn(3, ' ');
    let version = parts.next().ok_or_else(bad)?;
    let http11 = match version {
        "ICY" | "HTTP/1.0" => false,
        v if v.starts_with("HTTP/1.") && v.len() == 8 && v.as_bytes()[7].is_ascii_digit() => true,
        _ => return Err(bad()),
    };
    let code = parts.next().ok_or_else(bad)?;
    if code.len() != 3 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad());
    }
    let status: u16 = code.parse().map_err(|_| bad())?;
    if status < 100 {
        return Err(bad());
    }
    Ok((status, http11))
}

/// How a body ends (RFC 9112 section 6.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Framing {
    /// This many bytes remain.
    Length(u64),
    /// Chunked transfer coding.
    Chunked(Chunk),
    /// The body is everything until the connection closes: a live stream.
    UntilClose,
    /// The body has ended.
    Done,
}

/// Where a chunked body's decoder is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Chunk {
    /// At a chunk-size line.
    Size,
    /// Inside a chunk with this many bytes left.
    Data(u64),
    /// At the CRLF after a chunk's data.
    DataEnd,
}

/// The framing a response head declares.
pub(crate) fn framing_of(head: &Head) -> Result<Framing, FetchError> {
    if head.status == 204 || head.status == 304 || head.status < 200 {
        return Ok(Framing::Done);
    }
    let mut codings = head
        .headers
        .all("transfer-encoding")
        .flat_map(|v| v.split(','))
        .map(|c| c.trim().to_ascii_lowercase())
        .filter(|c| !c.is_empty() && c != "identity")
        .peekable();
    if codings.peek().is_some() {
        let all: Vec<String> = codings.collect();
        if all.len() == 1 && all[0] == "chunked" {
            return Ok(Framing::Chunked(Chunk::Size));
        }
        return Err(FetchError::Unsupported(format!(
            "http: transfer coding {}",
            all.join(", ")
        )));
    }
    let mut length: Option<u64> = None;
    for value in head.headers.all("content-length") {
        for part in value.split(',') {
            let part = part.trim();
            let parsed = match part.parse::<u64>() {
                Ok(n) if part.bytes().all(|b| b.is_ascii_digit()) => n,
                _ => {
                    return Err(FetchError::Malformed(
                        "http: Content-Length is not a number".into(),
                    ))
                }
            };
            if length.is_some_and(|seen| seen != parsed) {
                return Err(FetchError::Malformed(
                    "http: two different Content-Length values".into(),
                ));
            }
            length = Some(parsed);
        }
    }
    Ok(match length {
        Some(0) => Framing::Done,
        Some(n) => Framing::Length(n),
        None => Framing::UntilClose,
    })
}

/// A response body being read: never buffered whole, read at the caller's
/// pace.
pub(crate) struct Body<R> {
    conn: Buffered<R>,
    framing: Framing,
    /// Bytes read ahead by `peek`, handed out again first.
    ahead: Vec<u8>,
}

fn cut_short(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::UnexpectedEof, format!("http: {what}"))
}

fn bad_chunk(what: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("http: chunked body: {what}"),
    )
}

impl<R: Read> Body<R> {
    pub fn new(conn: Buffered<R>, framing: Framing) -> Body<R> {
        Body {
            conn,
            framing,
            ahead: Vec::new(),
        }
    }

    /// Up to the first `n` bytes of what is left, without consuming them
    /// (fewer only when the body ends first).
    pub fn peek(&mut self, n: usize) -> io::Result<&[u8]> {
        while self.ahead.len() < n {
            let mut more = vec![0u8; n - self.ahead.len()];
            let got = self.read_framed(&mut more)?;
            if got == 0 {
                break;
            }
            self.ahead.extend_from_slice(&more[..got]);
        }
        Ok(&self.ahead[..self.ahead.len().min(n)])
    }

    /// Reads and drops `n` bytes; an early end is an error.
    pub fn skip(&mut self, mut n: u64) -> io::Result<()> {
        let mut scratch = [0u8; 4096];
        while n > 0 {
            let want = scratch.len().min(usize::try_from(n).unwrap_or(usize::MAX));
            let got = self.read(&mut scratch[..want])?;
            if got == 0 {
                return Err(cut_short("the body ended inside a skipped range"));
            }
            n -= got as u64;
        }
        Ok(())
    }

    fn chunk_line(&mut self, budget: &mut usize) -> io::Result<Vec<u8>> {
        match self.conn.line(budget) {
            Ok(Some(line)) => Ok(line),
            Ok(None) => Err(cut_short("the connection closed inside a chunked body")),
            Err(Some(e)) => Err(e),
            Err(None) => Err(bad_chunk("a line is too long")),
        }
    }

    fn read_framed(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        loop {
            match self.framing {
                Framing::Done => return Ok(0),
                Framing::UntilClose => {
                    let n = self.conn.read(out)?;
                    if n == 0 {
                        self.framing = Framing::Done;
                    }
                    return Ok(n);
                }
                Framing::Length(left) => {
                    let want = out.len().min(usize::try_from(left).unwrap_or(usize::MAX));
                    let n = self.conn.read(&mut out[..want])?;
                    if n == 0 {
                        return Err(cut_short(
                            "the connection closed before Content-Length bytes arrived",
                        ));
                    }
                    let left = left - n as u64;
                    self.framing = if left == 0 {
                        Framing::Done
                    } else {
                        Framing::Length(left)
                    };
                    return Ok(n);
                }
                Framing::Chunked(Chunk::Size) => {
                    let mut budget = MAX_CHUNK_LINE;
                    let line = self.chunk_line(&mut budget)?;
                    let text = String::from_utf8_lossy(&line);
                    // A chunk extension (RFC 9112 section 7.1.1) is ignored.
                    let digits = text.split(';').next().unwrap_or("").trim();
                    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
                        return Err(bad_chunk("a chunk size is not hexadecimal"));
                    }
                    let size = u64::from_str_radix(digits, 16)
                        .map_err(|_| bad_chunk("a chunk size is too large"))?;
                    if size == 0 {
                        // The trailer section: fields until an empty line.
                        let mut budget = MAX_TRAILER_BYTES;
                        while !self.chunk_line(&mut budget)?.is_empty() {}
                        self.framing = Framing::Done;
                    } else {
                        self.framing = Framing::Chunked(Chunk::Data(size));
                    }
                }
                Framing::Chunked(Chunk::Data(left)) => {
                    let want = out.len().min(usize::try_from(left).unwrap_or(usize::MAX));
                    let n = self.conn.read(&mut out[..want])?;
                    if n == 0 {
                        return Err(cut_short("the connection closed inside a chunk"));
                    }
                    let left = left - n as u64;
                    self.framing = Framing::Chunked(if left == 0 {
                        Chunk::DataEnd
                    } else {
                        Chunk::Data(left)
                    });
                    return Ok(n);
                }
                Framing::Chunked(Chunk::DataEnd) => {
                    let mut budget = 2;
                    if !self.chunk_line(&mut budget)?.is_empty() {
                        return Err(bad_chunk("no line end after a chunk"));
                    }
                    self.framing = Framing::Chunked(Chunk::Size);
                }
            }
        }
    }
}

impl<R: Read> Read for Body<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if !self.ahead.is_empty() {
            let n = out.len().min(self.ahead.len());
            out[..n].copy_from_slice(&self.ahead[..n]);
            self.ahead.drain(..n);
            return Ok(n);
        }
        self.read_framed(out)
    }
}

/// The request chorus sends. `range_from` asks for the body from that byte on
/// (RFC 9110 section 14.1.2); `icy` says the client understands in-band
/// stream metadata.
pub(crate) fn request_bytes(url: &Url, range_from: Option<u64>, icy: bool) -> String {
    let mut request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: {}\r\nAccept: */*\r\nAccept-Encoding: identity\r\n",
        url.target,
        url.host_header(),
        USER_AGENT
    );
    if icy {
        request.push_str("Icy-MetaData: 1\r\n");
    }
    if let Some(from) = range_from {
        request.push_str(&format!("Range: bytes={from}-\r\n"));
    }
    request.push_str("Connection: close\r\n\r\n");
    request
}

/// The addresses a URL's host stands for: the literal itself, or what the
/// system resolver answers, once.
fn resolve(url: &Url) -> Result<Vec<SocketAddr>, FetchError> {
    if let Some(ip) = url.literal {
        return Ok(vec![SocketAddr::new(ip, url.port)]);
    }
    let addrs: Vec<SocketAddr> = (url.host.as_str(), url.port)
        .to_socket_addrs()
        .map_err(|e| {
            FetchError::Io(io::Error::new(
                e.kind(),
                format!("cannot resolve {}: {e}", url.host),
            ))
        })?
        .collect();
    if addrs.is_empty() {
        return Err(FetchError::Io(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} resolves to no address", url.host),
        )));
    }
    Ok(addrs)
}

/// Resolves, checks every address against the policy, and connects to an
/// address that passed: the address checked is the address connected to. A
/// refused address is never connected to; when every address is refused the
/// first refusal is the error and no socket was opened.
fn connect(ctx: &mut Ctx, url: &Url) -> Result<Transport, FetchError> {
    let mut refusal = None;
    let mut allowed = Vec::new();
    for addr in resolve(url)? {
        match check_address(addr, &ctx.policy, &is_own_address) {
            Ok(()) => allowed.push(addr),
            Err(e) => {
                refusal.get_or_insert(e);
            }
        }
    }
    if allowed.is_empty() {
        return Err(
            refusal.unwrap_or_else(|| FetchError::Io(io::Error::from(io::ErrorKind::NotFound)))
        );
    }
    // The roots are read before any socket is opened, so a missing bundle
    // is an error that reached no server.
    let tls_config = match url.scheme {
        Scheme::Http => None,
        Scheme::Https => Some(ctx.tls_config()?),
    };
    let mut failure = None;
    for addr in allowed.into_iter().take(MAX_CONNECT_ATTEMPTS) {
        match TcpStream::connect_timeout(&addr, ctx.policy.connect_timeout) {
            Ok(sock) => {
                sock.set_read_timeout(Some(ctx.policy.read_timeout))?;
                sock.set_write_timeout(Some(ctx.policy.read_timeout))?;
                sock.set_nodelay(true)?;
                return match tls_config {
                    None => Ok(Transport::Plain(sock)),
                    Some(config) => {
                        Ok(Transport::Tls(Box::new(tls::handshake(config, url, sock)?)))
                    }
                };
            }
            Err(e) => {
                failure.get_or_insert(io::Error::new(e.kind(), format!("connect to {addr}: {e}")));
            }
        }
    }
    Err(FetchError::Io(failure.unwrap_or_else(|| {
        io::Error::from(io::ErrorKind::NotFound)
    })))
}

/// A response whose status was 200 or 206, with its body still on the wire.
pub(crate) struct Response {
    /// The URL that answered, after redirects.
    pub url: Url,
    pub head: Head,
    pub body: Body<Transport>,
}

impl std::fmt::Debug for Response {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Response({} {})", self.head.status, self.url)
    }
}

/// `GET url`, following redirects (301, 302, 303, 307, 308; RFC 9110 section
/// 15.4) up to the policy's bound. Every hop is parsed, checked and connected
/// like the first. A status other than 200 or 206 is `FetchError::Http`.
pub(crate) fn get(
    ctx: &mut Ctx,
    url: &Url,
    range_from: Option<u64>,
    icy: bool,
) -> Result<Response, FetchError> {
    let mut url = url.clone();
    let mut redirects = 0u32;
    loop {
        let transport = connect(ctx, &url)?;
        let deadline = Instant::now() + ctx.policy.read_timeout;
        let mut conn = Buffered::new(transport);
        conn.inner
            .write_all(request_bytes(&url, range_from, icy).as_bytes())
            .and_then(|()| conn.inner.flush())?;
        let head = loop {
            let head = read_head(&mut conn, ctx.policy.max_header_bytes, deadline)?;
            // Interim responses (RFC 9110 section 15.2) come before the real one.
            if head.status >= 200 {
                break head;
            }
        };
        match head.status {
            200 | 206 => {
                if let Some(coding) = head.headers.get("content-encoding") {
                    if !coding.eq_ignore_ascii_case("identity") && !coding.is_empty() {
                        return Err(FetchError::Unsupported(format!(
                            "http: content coding {coding}"
                        )));
                    }
                }
                let framing = framing_of(&head)?;
                return Ok(Response {
                    url,
                    head,
                    body: Body::new(conn, framing),
                });
            }
            301 | 302 | 303 | 307 | 308 => {
                redirects += 1;
                if redirects > u32::from(ctx.policy.max_redirects) {
                    return Err(FetchError::Refused(format!(
                        "more than {} redirects",
                        ctx.policy.max_redirects
                    )));
                }
                let Some(location) = head.headers.get("location") else {
                    return Err(FetchError::Malformed(format!(
                        "http: a {} redirect with no Location",
                        head.status
                    )));
                };
                url = url.join(location)?;
            }
            status => return Err(FetchError::Http(status)),
        }
    }
}

/// The first byte position of a `Content-Range: bytes first-last/total`
/// (RFC 9110 section 14.4), and the total when it is given.
pub(crate) fn content_range(value: &str) -> Option<(u64, Option<u64>)> {
    let rest = value.trim().strip_prefix("bytes")?.trim_start();
    let (range, total) = rest.split_once('/')?;
    let (first, _last) = range.split_once('-')?;
    let first = first.trim().parse().ok()?;
    Some((first, total.trim().parse().ok()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// A reader that hands out its bytes in pieces of a fixed size.
    struct Pieces {
        data: Vec<u8>,
        at: usize,
        piece: usize,
    }

    impl Read for Pieces {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            let n = self.piece.min(out.len()).min(self.data.len() - self.at);
            out[..n].copy_from_slice(&self.data[self.at..self.at + n]);
            self.at += n;
            Ok(n)
        }
    }

    fn conn(data: &[u8], piece: usize) -> Buffered<Pieces> {
        Buffered::new(Pieces {
            data: data.to_vec(),
            at: 0,
            piece,
        })
    }

    fn head_of(text: &str) -> Result<Head, FetchError> {
        let deadline = Instant::now() + Duration::from_secs(5);
        read_head(&mut conn(text.as_bytes(), 3), 1024, deadline)
    }

    fn body_of(wire: &[u8], framing: Framing, piece: usize, read: usize) -> io::Result<Vec<u8>> {
        let mut body = Body::new(conn(wire, piece), framing);
        let mut out = Vec::new();
        let mut buf = vec![0u8; read];
        loop {
            match body.read(&mut buf)? {
                0 => return Ok(out),
                n => out.extend_from_slice(&buf[..n]),
            }
        }
    }

    #[test]
    fn status_lines_parse() {
        assert_eq!(parse_status_line(b"HTTP/1.1 200 OK").unwrap(), (200, true));
        assert_eq!(
            parse_status_line(b"HTTP/1.0 302 Found").unwrap(),
            (302, false)
        );
        assert_eq!(parse_status_line(b"ICY 200 OK").unwrap(), (200, false));
        assert_eq!(parse_status_line(b"HTTP/1.1 404").unwrap(), (404, true));
        assert_eq!(
            parse_status_line(b"HTTP/1.1 206 Partial Content").unwrap(),
            (206, true)
        );
        for bad in [
            &b"HTTP/2 200 OK"[..],
            b"HTTP/1.1 20 OK",
            b"HTTP/1.1 2000 OK",
            b"HTTP/1.1 abc OK",
            b"HTTP/1.1 099 Low",
            b"SSH-2.0-OpenSSH",
            b"",
            b"ICY",
            b"\xff\xfb\x90\x00",
        ] {
            assert!(
                matches!(parse_status_line(bad), Err(FetchError::Malformed(_))),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn a_head_parses_with_folded_lines_and_bare_line_feeds() {
        let head = head_of(
            "HTTP/1.1 200 OK\r\nContent-Type: audio/mpeg\r\nICY-Name:  Example Radio \r\n\
             X-Folded: a\r\n  b\r\nnot a field\r\nContent-Length: 5\n\nhello",
        )
        .unwrap();
        assert_eq!(head.status, 200);
        assert!(head.http11);
        assert_eq!(head.headers.get("content-type"), Some("audio/mpeg"));
        assert_eq!(head.headers.get("icy-name"), Some("Example Radio"));
        assert_eq!(head.headers.get("x-folded"), Some("a b"));
        assert_eq!(head.headers.get("content-length"), Some("5"));
        assert_eq!(head.headers.get("absent"), None);
    }

    #[test]
    fn the_body_starts_right_after_the_head() {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut c = conn(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello", 64);
        let head = read_head(&mut c, 1024, deadline).unwrap();
        let mut body = Body::new(c, framing_of(&head).unwrap());
        let mut out = Vec::new();
        body.read_to_end(&mut out).unwrap();
        assert_eq!(out, b"hello");
    }

    #[test]
    fn oversize_headers_are_refused_by_name() {
        let big = format!("HTTP/1.1 200 OK\r\nX-Pad: {}\r\n\r\n", "a".repeat(2000));
        match head_of(&big) {
            Err(FetchError::Refused(rule)) => {
                assert_eq!(rule, "response headers larger than 1024 bytes")
            }
            other => panic!("{other:?}"),
        }
        // Exactly at the bound is accepted.
        let exact = "HTTP/1.1 200 OK\r\n\r\n";
        let deadline = Instant::now() + Duration::from_secs(5);
        assert!(read_head(&mut conn(exact.as_bytes(), 3), exact.len(), deadline).is_ok());
        assert!(matches!(
            read_head(&mut conn(exact.as_bytes(), 3), exact.len() - 1, deadline),
            Err(FetchError::Refused(_))
        ));
    }

    #[test]
    fn a_head_that_ends_early_or_late_is_an_error() {
        assert!(matches!(
            head_of("HTTP/1.1 200 OK\r\nContent-Type: a"),
            Err(FetchError::Io(e)) if e.kind() == io::ErrorKind::UnexpectedEof
        ));
        let past = Instant::now() - Duration::from_millis(1);
        assert!(matches!(
            read_head(&mut conn(b"HTTP/1.1 200 OK\r\n\r\n", 3), 1024, past),
            Err(FetchError::Io(e)) if e.kind() == io::ErrorKind::TimedOut
        ));
    }

    fn framing(text: &str) -> Result<Framing, FetchError> {
        framing_of(&head_of(text).unwrap())
    }

    #[test]
    fn framing_follows_the_head() {
        assert_eq!(
            framing("HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\n").unwrap(),
            Framing::Length(10)
        );
        assert_eq!(
            framing("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n").unwrap(),
            Framing::Done
        );
        assert_eq!(
            framing("ICY 200 OK\r\nicy-br: 128\r\n\r\n").unwrap(),
            Framing::UntilClose
        );
        assert_eq!(
            framing("HTTP/1.1 204 No Content\r\n\r\n").unwrap(),
            Framing::Done
        );
        // Chunked wins over a length (RFC 9112 section 6.3, rule 3).
        assert_eq!(
            framing("HTTP/1.1 200 OK\r\nContent-Length: 10\r\nTransfer-Encoding: Chunked\r\n\r\n")
                .unwrap(),
            Framing::Chunked(Chunk::Size)
        );
        assert_eq!(
            framing("HTTP/1.1 200 OK\r\nContent-Length: 7\r\nContent-Length: 7\r\n\r\n").unwrap(),
            Framing::Length(7)
        );
        assert!(matches!(
            framing("HTTP/1.1 200 OK\r\nContent-Length: 7\r\nContent-Length: 8\r\n\r\n"),
            Err(FetchError::Malformed(_))
        ));
        assert!(matches!(
            framing("HTTP/1.1 200 OK\r\nContent-Length: -1\r\n\r\n"),
            Err(FetchError::Malformed(_))
        ));
        assert!(matches!(
            framing("HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip, chunked\r\n\r\n"),
            Err(FetchError::Unsupported(what)) if what == "http: transfer coding gzip, chunked"
        ));
    }

    #[test]
    fn a_length_body_ends_at_its_length_and_notices_a_short_one() {
        for piece in [1, 2, 5, 64] {
            assert_eq!(
                body_of(b"helloEXTRA", Framing::Length(5), piece, 3).unwrap(),
                b"hello"
            );
        }
        let err = body_of(b"hel", Framing::Length(5), 2, 8).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn an_until_close_body_is_everything() {
        assert_eq!(
            body_of(b"all of it", Framing::UntilClose, 2, 4).unwrap(),
            b"all of it"
        );
        assert_eq!(body_of(b"", Framing::UntilClose, 2, 4).unwrap(), b"");
    }

    #[test]
    fn chunked_bodies_decode_at_any_piece_size() {
        let wire = b"5\r\nhello\r\n1;ext=1\r\n \r\nA\r\nworld01234\r\n0\r\nTrailer: x\r\n\r\nNEXT";
        for piece in [1, 2, 3, 7, 64] {
            for read in [1, 4, 64] {
                assert_eq!(
                    body_of(wire, Framing::Chunked(Chunk::Size), piece, read).unwrap(),
                    b"hello world01234",
                    "pieces of {piece}, reads of {read}"
                );
            }
        }
        // Bare line feeds are tolerated.
        assert_eq!(
            body_of(b"3\nabc\n0\n\n", Framing::Chunked(Chunk::Size), 2, 8).unwrap(),
            b"abc"
        );
    }

    #[test]
    fn broken_chunked_bodies_are_errors() {
        let chunked = Framing::Chunked(Chunk::Size);
        for (wire, kind) in [
            (&b"5\r\nhel"[..], io::ErrorKind::UnexpectedEof),
            (b"5\r\nhello\r\n", io::ErrorKind::UnexpectedEof),
            (b"5\r\nhello\r\n0\r\n", io::ErrorKind::UnexpectedEof),
            (b"zz\r\nhello\r\n", io::ErrorKind::InvalidData),
            (b"\r\nhello\r\n", io::ErrorKind::InvalidData),
            (b"-5\r\nhello\r\n", io::ErrorKind::InvalidData),
            (b"5\r\nhelloXX\r\n", io::ErrorKind::InvalidData),
            (b"fffffffffffffffff\r\n", io::ErrorKind::InvalidData),
        ] {
            let err = body_of(wire, chunked, 3, 8).unwrap_err();
            assert_eq!(err.kind(), kind, "{:?}", String::from_utf8_lossy(wire));
        }
        let long = vec![b'1'; MAX_CHUNK_LINE + 10];
        assert_eq!(
            body_of(&long, chunked, 64, 8).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn peek_does_not_consume_and_skip_does() {
        let mut body = Body::new(
            conn(b"3\r\n#EX\r\n4\r\nTM3U\r\n2\r\n\nx\r\n0\r\n\r\n", 2),
            Framing::Chunked(Chunk::Size),
        );
        assert_eq!(body.peek(7).unwrap(), b"#EXTM3U");
        assert_eq!(body.peek(3).unwrap(), b"#EX");
        body.skip(4).unwrap();
        let mut rest = Vec::new();
        body.read_to_end(&mut rest).unwrap();
        assert_eq!(rest, b"M3U\nx");
        let mut short = Body::new(conn(b"ab", 2), Framing::UntilClose);
        assert_eq!(short.peek(7).unwrap(), b"ab");
        assert_eq!(
            short.skip(3).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
    }

    #[test]
    fn the_request_is_what_the_brief_says() {
        let url = Url::parse("http://radio.example:8000/live?x=1").unwrap();
        assert_eq!(
            request_bytes(&url, None, true),
            format!(
                "GET /live?x=1 HTTP/1.1\r\nHost: radio.example:8000\r\nUser-Agent: chorus/{}\r\n\
                 Accept: */*\r\nAccept-Encoding: identity\r\nIcy-MetaData: 1\r\nConnection: close\r\n\r\n",
                env!("CARGO_PKG_VERSION")
            )
        );
        let ranged = request_bytes(&url, Some(1234), false);
        assert!(ranged.contains("\r\nRange: bytes=1234-\r\n"));
        assert!(!ranged.contains("Icy-MetaData"));
    }

    #[test]
    fn content_range_parses() {
        assert_eq!(content_range("bytes 100-199/1000"), Some((100, Some(1000))));
        assert_eq!(content_range("bytes 0-0/*"), Some((0, None)));
        assert_eq!(content_range("bytes */1000"), None);
        assert_eq!(content_range("items 1-2/3"), None);
    }
}
