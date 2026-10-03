//! chorus's media fetcher: a URL in, a stream of bytes out.
//!
//! [`open`] takes an `http` or `https` URL and a [`Policy`] and returns a
//! [`Stream`] that implements `Read`. On the way it:
//!
//! - refuses what the fetch policy refuses (brief section 4.8, proposal P6):
//!   other schemes, userinfo, and every address that is loopback, link-local,
//!   unspecified, multicast, broadcast or one of the server's own listeners,
//!   checked on the resolved address before every connect, redirects and HLS
//!   segments included ([`policy`]);
//! - speaks HTTP/1.1 by hand over `std::net`, with TLS from rustls for https
//!   (the client is private; its rules are in the decision record);
//! - takes ICY metadata back out of a radio stream and keeps the latest
//!   title ([`icy`]);
//! - plays an HLS playlist of packed MP3 segments as one stream and refuses
//!   every other kind of HLS by name ([`hls`]).
//!
//! Nothing here decodes audio, starts a thread or reads a settable clock:
//! every timeout and the HLS reload schedule run on `std::time::Instant`.
//! (TLS certificate validity is checked by rustls against the system time,
//! which no code here reads.)
//!
//! The decision record is `docs/decisions/0120-the-media-fetcher.md`; the
//! owner's page is `docs/streams.md`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod error;
pub mod hls;
mod http;
pub mod icy;
pub mod policy;
mod tls;
pub mod url;

use std::io::{self, Read, SeekFrom};
use std::time::Instant;

pub use error::FetchError;
pub use http::{Cancel, CANCEL_SLICE, USER_AGENT};
pub use icy::IcyInfo;
pub use policy::Policy;
pub use tls::{ca_bundle_path, DEFAULT_CA_BUNDLE};

use hls::HlsStream;
use http::{Body, Ctx, Response, Transport};
use icy::IcyFilter;
use url::Url;

/// What `open` learned from the response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opened {
    /// The `Content-Type`, as sent. For an HLS stream: `audio/mpeg`, what
    /// its segments are.
    pub content_type: Option<String>,
    /// The body's length when the server declared one.
    pub byte_len: Option<u64>,
    /// Whether [`Stream::seek`] works: the server said `Accept-Ranges: bytes`
    /// (or answered a range request with 206) and declared a length.
    pub seekable: bool,
    /// The station's ICY headers, when the response had any.
    pub icy: Option<IcyInfo>,
    /// The URL that answered, after redirects. For an HLS stream: the media
    /// playlist's.
    pub final_url: String,
    /// Whether this is an HLS stream (the bytes are concatenated segments).
    pub hls: bool,
}

/// The furthest a forward seek is served by reading and dropping bytes on
/// the open connection instead of a new range request. ASSUMED: a new
/// request costs a connect (and a TLS handshake); 256 KiB arrives sooner than
/// that on any link that plays music.
const SEEK_BY_READING_MAX: u64 = 256 * 1024;

/// An open media stream. `Read` yields the media bytes at the reader's pace;
/// nothing is buffered beyond one small socket buffer.
pub struct Stream {
    opened: Opened,
    inner: Inner,
}

enum Inner {
    Plain(Box<Plain>),
    Hls(Box<HlsStream>),
}

struct Plain {
    ctx: Ctx,
    url: Url,
    /// The response body being read; `None` after a seek, until the next
    /// read re-requests, and at the end of a seekable body.
    body: Option<Body<Transport>>,
    /// The position of the next byte `body` yields.
    position: u64,
    /// Where the next read must start, when a seek moved it.
    target: Option<u64>,
    len: Option<u64>,
    seekable: bool,
    icy: Option<IcyFilter>,
}

/// Opens `url` under `policy`.
///
/// An HLS playlist (by content type, or a body starting `#EXTM3U`) is opened
/// as the stream of its segments, the first of which is fetched and judged
/// before this returns.
pub fn open(url: &str, policy: &Policy) -> Result<Stream, FetchError> {
    open_with(url, policy, None)
}

/// [`open`], for a caller that may give the fetch up: `cancel` is asked at
/// least every [`CANCEL_SLICE`] while the fetch waits on a connected socket
/// (the response head, the TLS handshake, every later read of the stream,
/// HLS segments and reloads included) and once it answers `true` the wait
/// ends with an `io::Error` of kind `TimedOut`. The read timeout of the
/// policy still bounds each wait as before.
///
/// What it does not interrupt: name resolution, and a TCP connect in
/// progress, which `std::net` offers no way to leave early; a connect is
/// bounded by the policy's `connect_timeout`.
pub fn open_cancellable(url: &str, policy: &Policy, cancel: Cancel) -> Result<Stream, FetchError> {
    open_with(url, policy, Some(cancel))
}

fn open_with(url: &str, policy: &Policy, cancel: Option<Cancel>) -> Result<Stream, FetchError> {
    let url = Url::parse(url)?;
    let mut ctx = Ctx::new(policy, cancel);
    let began = Instant::now();
    let Response {
        url,
        head,
        mut body,
    } = http::get(&mut ctx, &url, None, true)?;

    let content_type = head.headers.get("content-type").map(str::to_string);
    let by_type = content_type
        .as_deref()
        .is_some_and(hls::playlist::is_playlist_type);
    if by_type || hls::playlist::looks_like_playlist(body.peek(7)?) {
        let text = hls::read_playlist(&mut body)?;
        drop(body);
        let stream = HlsStream::start(ctx, url, &text, began)?;
        return Ok(Stream {
            opened: Opened {
                content_type: Some("audio/mpeg".to_string()),
                byte_len: None,
                seekable: false,
                icy: None,
                final_url: stream.playlist_url().to_string(),
                hls: true,
            },
            inner: Inner::Hls(Box::new(stream)),
        });
    }

    let icy = icy_info(&head.headers)?;
    let metaint = icy.as_ref().and_then(|info| info.metaint);
    let len = match http::framing_of(&head)? {
        http::Framing::Length(n) => Some(n),
        http::Framing::Done => Some(0),
        _ => None,
    };
    let accepts_ranges = head
        .headers
        .get("accept-ranges")
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("bytes"));
    let seekable = accepts_ranges && len.is_some() && metaint.is_none();
    Ok(Stream {
        opened: Opened {
            content_type,
            byte_len: len,
            seekable,
            icy,
            final_url: url.to_string(),
            hls: false,
        },
        inner: Inner::Plain(Box::new(Plain {
            ctx,
            url,
            body: Some(body),
            position: 0,
            target: None,
            len,
            seekable,
            icy: metaint.map(IcyFilter::new),
        })),
    })
}

/// The ICY headers of a response, when it has any.
fn icy_info(headers: &http::Headers) -> Result<Option<IcyInfo>, FetchError> {
    let text = |name: &str| {
        headers
            .get(name)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
    };
    let metaint = match headers.get("icy-metaint") {
        None => None,
        Some(value) => match value.trim().parse::<usize>() {
            Ok(0) => None,
            Ok(n) if n <= icy::MAX_METAINT => Some(n),
            _ => {
                return Err(FetchError::Malformed(format!(
                    "icy: icy-metaint is not a number from 0 to {}",
                    icy::MAX_METAINT
                )))
            }
        },
    };
    let info = IcyInfo {
        name: text("icy-name"),
        bitrate_kbps: headers.get("icy-br").and_then(|v| {
            // Some servers repeat the value: "128, 128".
            v.split(',')
                .next()
                .and_then(|first| first.trim().parse().ok())
        }),
        genre: text("icy-genre"),
        metaint,
    };
    Ok((info != IcyInfo::default()).then_some(info))
}

impl Stream {
    /// What `open` learned from the response.
    pub fn opened(&self) -> &Opened {
        &self.opened
    }

    /// The latest ICY `StreamTitle`, when the stream carries in-band
    /// metadata and a block with a title has been read past.
    pub fn stream_title(&self) -> Option<String> {
        match &self.inner {
            Inner::Plain(plain) => plain
                .icy
                .as_ref()
                .and_then(|filter| filter.title())
                .map(str::to_string),
            Inner::Hls(_) => None,
        }
    }

    /// Moves the read position, when the stream is seekable
    /// ([`Opened::seekable`]); otherwise an error of kind `Unsupported`. The
    /// new position is returned. The connection is not touched here: the next
    /// read re-requests from the new position with a `Range` header (or reads
    /// forward to it when it is near).
    pub fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        match &mut self.inner {
            Inner::Plain(plain) => plain.seek(pos),
            Inner::Hls(_) => Err(not_seekable()),
        }
    }
}

fn not_seekable() -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, "the stream is not seekable")
}

impl Read for Stream {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        match &mut self.inner {
            Inner::Plain(plain) => plain.read(out),
            Inner::Hls(hls) => hls.read(out),
        }
    }
}

impl Plain {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let (true, Some(len)) = (self.seekable, self.len) else {
            return Err(not_seekable());
        };
        let current = self.target.unwrap_or(self.position);
        let target = match pos {
            SeekFrom::Start(n) => Some(n),
            SeekFrom::Current(delta) => current.checked_add_signed(delta),
            SeekFrom::End(delta) => len.checked_add_signed(delta),
        };
        let Some(target) = target else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a seek to before the start of the stream",
            ));
        };
        self.target = Some(target);
        Ok(target)
    }

    /// Brings the body to a seek's target: nothing to do when it is already
    /// there, a short read-and-drop when it is just ahead, a new request with
    /// `Range: bytes=<target>-` otherwise.
    fn reposition(&mut self, target: u64) -> io::Result<()> {
        if target == self.position && self.body.is_some() {
            return Ok(());
        }
        if self.len.is_some_and(|len| target >= len) {
            // At or past the end: nothing to request (a range starting there
            // is unsatisfiable, RFC 9110 section 14.1.2); reads return 0.
            self.body = None;
            self.position = target;
            return Ok(());
        }
        if let Some(body) = &mut self.body {
            if target > self.position && target - self.position <= SEEK_BY_READING_MAX {
                body.skip(target - self.position)?;
                self.position = target;
                return Ok(());
            }
        }
        self.body = None;
        let response = http::get(&mut self.ctx, &self.url, Some(target), false)
            .map_err(FetchError::into_io)?;
        let start = match response.head.status {
            206 => response
                .head
                .headers
                .get("content-range")
                .and_then(http::content_range)
                .map(|(first, _)| first),
            // A server may ignore Range and send the whole body (RFC 9110
            // section 14.2); that is only the body asked for from byte 0.
            _ => Some(0),
        };
        if start != Some(target) {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "the server did not honour the range request",
            ));
        }
        self.body = Some(response.body);
        self.position = target;
        Ok(())
    }

    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        if let Some(target) = self.target {
            self.reposition(target)?;
            self.target = None;
        }
        let Some(body) = &mut self.body else {
            return Ok(0);
        };
        loop {
            let n = body.read(out)?;
            self.position += n as u64;
            if n == 0 {
                return Ok(0);
            }
            match &mut self.icy {
                None => return Ok(n),
                Some(filter) => {
                    // A read that held only metadata yields nothing to hand
                    // out, which must not look like the end of the stream.
                    let kept = filter.strip(out, n);
                    if kept > 0 {
                        return Ok(kept);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stream_can_cross_threads() {
        fn sendable<T: Send>() {}
        sendable::<Stream>();
    }
}
