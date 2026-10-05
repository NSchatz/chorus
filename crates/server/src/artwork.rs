//! A group's now-playing artwork, fetched by this server and served from its
//! own origin (`docs/decisions/0184-artwork-is-proxied-not-the-policy-widened.md`).
//!
//! The app's pages are served under a Content-Security-Policy whose image
//! rule is `img-src 'self' data:`, and a now-playing record's artwork is at
//! somebody else's address (a control point's media server, a station's
//! site). So the browser cannot load it, and this is what does instead:
//! `GET /api/artwork?group=<id>` answers with the image the group's record
//! names. The policy is not touched.
//!
//! The rules, which are the whole of the route's safety:
//!
//! - **The request names a group and nothing else.** The URL fetched is the
//!   one already in that group's now-playing record, which the server's
//!   runtime put there; nothing in a request is ever fetched, so this is not
//!   a proxy anybody can point somewhere.
//! - **The fetch is the media fetcher's** (`chorus-fetch`, ADR 0120) under
//!   the server's fetch policy: `http` and `https` only, never this machine's
//!   loopback, never one of this server's own ports, every redirect resolved
//!   and checked like the first address.
//! - **Bounded in size and in time.** At most [`MAX_ARTWORK_BYTES`] are read
//!   (a declared length above that is refused before the body is touched),
//!   and the whole fetch, from the first connect to the last byte, is given
//!   [`FETCH_DEADLINE`] on the monotonic clock.
//! - **Only an image is passed on.** What was fetched is served when its
//!   first bytes are a JPEG's, a PNG's, a GIF's or a WebP's, with that media
//!   type and `nosniff`; anything else is refused and none of it is sent.
//!   The upstream `Content-Type` is not believed either way.
//! - **A bounded number at once.** A fetch holds its control worker until it
//!   ends, so at most [`Artwork::ceiling`] run at a time (half the pool) and
//!   one more is answered `503`: commands are never queued behind artwork.
//!
//! Nothing here is on the audio path. A control worker (an ordinary thread
//! of the fixed pool, `crate::control`) runs the fetch and writes the
//! answer; no audio thread calls in, nothing is stamped, and the only clock
//! read is `Instant`, for the deadline. Nothing is kept: no cache in memory
//! and none on disk. The answer carries a strong `ETag` made from the bytes
//! served, and `Cache-Control: no-cache`, so a browser keeps the image and
//! asks again before every use; a new track's artwork has other bytes, so
//! another tag, and is never answered `304`.

use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chorus_fetch::{FetchError, Policy};

/// The largest image served, in bytes. ASSUMED, not measured: 4 MiB holds a
/// cover of 1400 by 1400 pixels as a PNG, which is larger than the JPEGs
/// control points and stations publish, and bounds what a worker holds in
/// memory for one answer.
pub const MAX_ARTWORK_BYTES: usize = 4 * 1024 * 1024;

/// How long one whole fetch may take: connects, redirects, the response head
/// and the body together. ASSUMED: 8 s is far longer than a cover takes on a
/// LAN or from a station's site, and short enough that a page waiting for
/// one shows its placeholder and moves on.
pub const FETCH_DEADLINE: Duration = Duration::from_secs(8);

/// The bound on one TCP connect of an artwork fetch. The media fetcher
/// cannot leave a connect early (`chorus_fetch::open_cancellable`), so a
/// fetch can outlive [`FETCH_DEADLINE`] by at most this much. ASSUMED: 3 s.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// How many redirects an artwork fetch follows. ASSUMED: 3; each is checked
/// by the fetch policy like the first address.
pub const MAX_REDIRECTS: u8 = 3;

/// `Cache-Control` of an image served: kept, and revalidated before every
/// use, so the tag decides whether the browser's copy is still the artwork.
pub const CACHE_CONTROL: &str = "no-cache";

/// The size and time bounds of a fetch. [`Bounds::default`] is what the
/// server runs with; a test shortens the deadline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    /// The largest body read, in bytes.
    pub max_bytes: usize,
    /// How long the whole fetch may take.
    pub deadline: Duration,
}

impl Default for Bounds {
    fn default() -> Self {
        Bounds {
            max_bytes: MAX_ARTWORK_BYTES,
            deadline: FETCH_DEADLINE,
        }
    }
}

/// An image, fetched and judged, ready to be served.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// The media type its first bytes say it is.
    pub media_type: &'static str,
    /// The strong entity tag of its bytes, quotes included.
    pub etag: String,
    /// Its bytes, as fetched.
    pub bytes: Vec<u8>,
}

/// Why no image is served, each with the status it is answered with.
#[derive(Debug)]
pub enum Refused {
    /// As many fetches as may run at once are running.
    Busy,
    /// The fetch was not done by its deadline.
    TimedOut,
    /// What came back does not start as a JPEG, PNG, GIF or WebP does.
    NotAnImage,
    /// What came back is, or says it is, larger than the bound.
    TooLarge(usize),
    /// The fetch failed or the fetch policy refused it: the fetcher's words.
    Upstream(String),
}

impl Refused {
    /// The status line's code and reason.
    pub fn status(&self) -> &'static str {
        match self {
            Refused::Busy => "503 Service Unavailable",
            Refused::TimedOut => "504 Gateway Timeout",
            Refused::NotAnImage | Refused::TooLarge(_) | Refused::Upstream(_) => "502 Bad Gateway",
        }
    }

    /// One word for the log.
    pub fn reason(&self) -> &'static str {
        match self {
            Refused::Busy => "busy",
            Refused::TimedOut => "timed-out",
            Refused::NotAnImage => "not-an-image",
            Refused::TooLarge(_) => "too-large",
            Refused::Upstream(_) => "fetch-failed",
        }
    }
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refused::Busy => write!(
                f,
                "as many artwork fetches as this server runs at once are running; try again"
            ),
            Refused::TimedOut => write!(f, "the artwork was not fetched within its deadline"),
            Refused::NotAnImage => write!(
                f,
                "what the artwork URL answered with is not an image this server serves (JPEG, \
                 PNG, GIF or WebP)"
            ),
            Refused::TooLarge(max) => write!(
                f,
                "what the artwork URL answered with is larger than {} bytes",
                max
            ),
            Refused::Upstream(why) => write!(f, "the artwork could not be fetched: {}", why),
        }
    }
}

/// The artwork fetcher of one server: its policy, its bounds and how many
/// fetches are running.
#[derive(Debug)]
pub struct Artwork {
    policy: Policy,
    bounds: Bounds,
    ceiling: AtomicUsize,
    running: AtomicUsize,
}

/// One running fetch's place under the ceiling, given back when dropped.
struct Permit<'a>(&'a AtomicUsize);

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Artwork {
    /// A fetcher under the server's fetch policy (`policy`: its address
    /// rules, its own ports, its CA bundle) and the production bounds.
    pub fn new(policy: Policy) -> Artwork {
        Artwork::with_bounds(policy, Bounds::default())
    }

    /// The same, under `bounds`. The address rules of `policy` are kept as
    /// they are; its connect timeout and redirect bound are tightened to
    /// this route's ([`CONNECT_TIMEOUT`], [`MAX_REDIRECTS`]) and its read
    /// timeout to the deadline, which ends the fetch first.
    pub fn with_bounds(policy: Policy, bounds: Bounds) -> Artwork {
        Artwork {
            policy: Policy {
                connect_timeout: policy.connect_timeout.min(CONNECT_TIMEOUT),
                read_timeout: policy.read_timeout.min(bounds.deadline),
                max_redirects: policy.max_redirects.min(MAX_REDIRECTS),
                ..policy
            },
            bounds,
            ceiling: AtomicUsize::new(1),
            running: AtomicUsize::new(0),
        }
    }

    /// Hold the fetches running at once to half of `workers` control
    /// workers, and to one at least: the other half is always free for
    /// commands and state, whatever an artwork's origin does.
    pub fn serve_from_workers(&self, workers: usize) {
        self.ceiling.store((workers / 2).max(1), Ordering::SeqCst);
    }

    /// How many fetches may run at once.
    pub fn ceiling(&self) -> usize {
        self.ceiling.load(Ordering::SeqCst)
    }

    fn permit(&self) -> Option<Permit<'_>> {
        let ceiling = self.ceiling();
        self.running
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |running| {
                (running < ceiling).then_some(running + 1)
            })
            .ok()
            .map(|_| Permit(&self.running))
    }

    /// Fetch the image at `url`, which the caller took from a now-playing
    /// record and from nowhere else.
    ///
    /// Runs on the calling thread until the image is whole, the fetch fails
    /// or the deadline passes. The deadline is an [`Instant`]: the fetcher
    /// is asked to give up through its cancel hook (every
    /// `chorus_fetch::CANCEL_SLICE` of a wait on a connected socket, and
    /// before every connect) and this loop looks at it between reads, so an
    /// origin that sends a byte at a time is held to it too.
    pub fn fetch(&self, url: &str) -> Result<Image, Refused> {
        let Some(_permit) = self.permit() else {
            return Err(Refused::Busy);
        };
        let max = self.bounds.max_bytes;
        let until = Instant::now() + self.bounds.deadline;
        let late = move || Instant::now() >= until;
        let failed = |error: FetchError| {
            if late() {
                Refused::TimedOut
            } else {
                Refused::Upstream(error.to_string())
            }
        };
        let mut stream =
            chorus_fetch::open_cancellable(url, &self.policy, Arc::new(late)).map_err(failed)?;
        let declared = stream.opened().byte_len;
        if stream.opened().hls {
            // A playlist: the fetcher would go on to play it.
            return Err(Refused::NotAnImage);
        }
        if declared.is_some_and(|len| len > max as u64) {
            return Err(Refused::TooLarge(max));
        }
        let mut bytes = Vec::with_capacity(declared.map_or(0, |len| len as usize));
        let mut judged = false;
        let mut buffer = [0u8; 16 * 1024];
        loop {
            if late() {
                return Err(Refused::TimedOut);
            }
            let read = match stream.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => read,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(failed(FetchError::from(error))),
            };
            if bytes.len() + read > max {
                return Err(Refused::TooLarge(max));
            }
            bytes.extend_from_slice(&buffer[..read]);
            // Judged as soon as there is enough to judge, so the rest of
            // something that is not an image is never read.
            if !judged && bytes.len() >= SNIFF_BYTES {
                if media_type_of(&bytes).is_none() {
                    return Err(Refused::NotAnImage);
                }
                judged = true;
            }
        }
        let Some(media_type) = media_type_of(&bytes) else {
            return Err(Refused::NotAnImage);
        };
        Ok(Image {
            media_type,
            etag: etag_of(&bytes),
            bytes,
        })
    }
}

/// How many bytes [`media_type_of`] needs to tell every kind it knows.
const SNIFF_BYTES: usize = 12;

/// The media type of an image by its first bytes, or `None` for anything
/// that is not one of the four kinds served.
///
/// | Kind | Starts with |
/// |---|---|
/// | `image/jpeg` | `FF D8 FF` |
/// | `image/png` | `89 50 4E 47 0D 0A 1A 0A` |
/// | `image/gif` | `GIF87a` or `GIF89a` |
/// | `image/webp` | `RIFF`, four bytes of length, `WEBP` |
///
/// SVG is left out on purpose: it is a document that can carry script, and
/// this origin is the control plane's. ASSUMED: the four signatures are
/// written as the formats are commonly described (the image table of the
/// WHATWG MIME Sniffing standard lists the same ones); no specification was
/// re-read for this function, and a signature that is wrong here refuses an
/// image, it never serves something else.
pub fn media_type_of(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
        Some("image/png")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// A strong entity tag: the bytes' 64-bit FNV-1a hash and their length,
/// quoted. The shape `build.rs` gives a file of the app; an identifier for a
/// cache and nothing more.
pub fn etag_of(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("\"{hash:016x}-{:x}\"", bytes.len())
}

/// The group a request for artwork names: the value of `group` in the query
/// of `target`, as written (a group's identifier is lower-case letters,
/// digits and hyphens, so nothing is decoded; a value that is no group's
/// identifier finds no record). Every other parameter is ignored, whatever
/// it is called and whatever it holds.
pub fn group_of(target: &str) -> Option<&str> {
    let query = target.split_once('?').map(|(_, q)| q).unwrap_or("");
    query
        .split('&')
        .find_map(|pair| pair.strip_prefix("group="))
        .filter(|group| !group.is_empty())
}

impl Image {
    /// Write this image as the answer to a `GET`: `304` with no body when
    /// `if_none_match` names its tag, else `200` with its bytes. `policy` is
    /// the Content-Security-Policy every response to a browser carries.
    pub fn respond(&self, connection: &mut TcpStream, if_none_match: Option<&str>, policy: &str) {
        let etag = &self.etag;
        if if_none_match.is_some_and(|header| crate::app::none_match_names(header, etag)) {
            let _ = write!(
                connection,
                "HTTP/1.1 304 Not Modified\r\nETag: {etag}\r\nCache-Control: {CACHE_CONTROL}\r\n\
                 Content-Security-Policy: {policy}\r\nConnection: close\r\n\r\n"
            );
            let _ = connection.flush();
            return;
        }
        let sent = write!(
            connection,
            "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nETag: {etag}\r\n\
             Cache-Control: {CACHE_CONTROL}\r\nX-Content-Type-Options: nosniff\r\n\
             Content-Security-Policy: {policy}\r\nConnection: close\r\n\r\n",
            self.media_type,
            self.bytes.len(),
        );
        if sent.is_ok() {
            let _ = connection.write_all(&self.bytes);
        }
        let _ = connection.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_image_is_told_by_its_first_bytes_and_nothing_else_is_one() {
        assert_eq!(
            media_type_of(&[0xff, 0xd8, 0xff, 0xe1, 0, 0]),
            Some("image/jpeg")
        );
        assert_eq!(
            media_type_of(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0]),
            Some("image/png")
        );
        assert_eq!(media_type_of(b"GIF89a\x01\x00"), Some("image/gif"));
        assert_eq!(media_type_of(b"GIF87a\x01\x00"), Some("image/gif"));
        assert_eq!(
            media_type_of(b"RIFF\x24\x00\x00\x00WEBPVP8 "),
            Some("image/webp")
        );
        for not in [
            &b""[..],
            b"\xff\xd8",
            b"\x89PNG\r\n",
            b"GIF90a",
            b"RIFF\x24\x00\x00\x00WAVEfmt ",
            b"RIFF\x24\x00\x00\x00WEB",
            b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>",
            b"<?xml version=\"1.0\"?><svg/>",
            b"<!doctype html>",
            b"#EXTM3U\n",
            b"{\"v\":2}",
        ] {
            assert_eq!(media_type_of(not), None, "{:?}", not);
        }
    }

    #[test]
    fn only_the_group_is_read_out_of_a_request() {
        assert_eq!(group_of("/api/artwork?group=kitchen"), Some("kitchen"));
        assert_eq!(
            group_of("/api/artwork?url=http://example.invalid/a.png&group=kitchen&v=3"),
            Some("kitchen")
        );
        assert_eq!(group_of("/api/artwork"), None);
        assert_eq!(group_of("/api/artwork?group="), None);
        assert_eq!(
            group_of("/api/artwork?url=http://example.invalid/a.png"),
            None
        );
        assert_eq!(group_of("/api/artwork?subgroup=kitchen"), None);
    }

    #[test]
    fn the_tag_follows_the_bytes() {
        assert_eq!(etag_of(b""), "\"cbf29ce484222325-0\"");
        assert_ne!(etag_of(b"one cover"), etag_of(b"another cover"));
        assert_eq!(etag_of(b"one cover"), etag_of(b"one cover"));
    }

    #[test]
    fn at_most_the_ceiling_run_at_once_and_a_place_comes_back() {
        let artwork = Artwork::new(Policy::default());
        assert_eq!(artwork.ceiling(), 1);
        artwork.serve_from_workers(4);
        assert_eq!(artwork.ceiling(), 2);
        let first = artwork.permit().expect("a place");
        let second = artwork.permit().expect("a second place");
        assert!(artwork.permit().is_none(), "the ceiling holds");
        drop(first);
        let third = artwork.permit().expect("the place came back");
        assert!(artwork.permit().is_none());
        drop((second, third));
        artwork.serve_from_workers(1);
        assert_eq!(artwork.ceiling(), 1, "one worker still fetches");
        artwork.serve_from_workers(0);
        assert_eq!(artwork.ceiling(), 1);
    }

    #[test]
    fn the_routes_policy_is_the_servers_with_tighter_times() {
        let server = Policy {
            denied_ports_on_self: vec![4010, 8080],
            ..Policy::default()
        };
        let artwork = Artwork::new(server.clone());
        assert_eq!(artwork.policy.denied_ports_on_self, [4010, 8080]);
        assert!(!artwork.policy.allow_loopback);
        assert_eq!(artwork.policy.connect_timeout, CONNECT_TIMEOUT);
        assert_eq!(artwork.policy.read_timeout, FETCH_DEADLINE);
        assert_eq!(artwork.policy.max_redirects, MAX_REDIRECTS);
        assert_eq!(artwork.policy.ca_bundle, server.ca_bundle);
    }
}
