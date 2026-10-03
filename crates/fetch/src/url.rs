//! The URLs chorus fetches: `http` and `https`, a host, a port, a path and a
//! query. Small on purpose: everything the fetch policy must decide about is a
//! field here, and everything it refuses to carry (userinfo, other schemes) is
//! an error by name.
//!
//! Syntax follows RFC 3986 (https://www.rfc-editor.org/rfc/rfc3986, sections 3
//! and 5.2, read 2026-10-03) for the subset used: `scheme "://" host [":" port]
//! path ["?" query]`. A fragment is never sent to a server (RFC 3986 section
//! 3.5), so it is dropped. Relative references are resolved as section 5.2.2
//! and 5.2.4 say, for redirects (`Location`) and HLS playlist entries.

use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crate::error::FetchError;

/// The two schemes the fetch policy allows (brief 4.8, P6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    /// Plain HTTP, default port 80.
    Http,
    /// HTTP over TLS, default port 443.
    Https,
}

impl Scheme {
    /// The scheme's default port.
    pub fn default_port(self) -> u16 {
        match self {
            Scheme::Http => 80,
            Scheme::Https => 443,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Scheme::Http => "http",
            Scheme::Https => "https",
        }
    }
}

/// A parsed absolute URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url {
    /// `http` or `https`.
    pub scheme: Scheme,
    /// The host, lower-cased: a name, a dotted IPv4 address or an IPv6 address
    /// without its brackets.
    pub host: String,
    /// The host as an address when it was written as a literal.
    pub literal: Option<IpAddr>,
    /// The port, the scheme's default when the URL names none.
    pub port: u16,
    /// The path and query as sent in the request line: never empty, always
    /// starting `/`, with no fragment and no byte outside printable ASCII.
    pub target: String,
}

/// The longest URL accepted, in bytes. ASSUMED: far above any stream URL seen
/// in practice and well under the header bound a request must fit.
pub const MAX_URL_BYTES: usize = 4096;

impl Url {
    /// Parses an absolute `http` or `https` URL.
    ///
    /// Refused by name: any other scheme, and userinfo (`user:password@`).
    pub fn parse(text: &str) -> Result<Url, FetchError> {
        let text = text.trim();
        if text.len() > MAX_URL_BYTES {
            return Err(FetchError::Malformed(format!(
                "url: longer than {MAX_URL_BYTES} bytes"
            )));
        }
        let Some((scheme, rest)) = text.split_once(':') else {
            return Err(FetchError::Malformed("url: no scheme".into()));
        };
        let scheme = parse_scheme(scheme)?;
        let Some(rest) = rest.strip_prefix("//") else {
            return Err(FetchError::Malformed("url: no host".into()));
        };
        let rest = rest.split('#').next().unwrap_or("");
        let end = rest.find(['/', '?', '\\']).unwrap_or(rest.len());
        let (authority, tail) = rest.split_at(end);
        if tail.starts_with('\\') {
            return Err(FetchError::Malformed(
                "url: backslash after the host".into(),
            ));
        }
        let (host, literal, port) = parse_authority(authority, scheme)?;
        let target = if tail.starts_with('?') {
            format!("/{tail}")
        } else if tail.is_empty() {
            "/".to_string()
        } else {
            tail.to_string()
        };
        let (path, query) = split_query(&target);
        Ok(Url {
            scheme,
            host,
            literal,
            port,
            target: join_query(&remove_dot_segments(&encode(path)), query.map(encode)),
        })
    }

    /// Resolves a reference found at this URL (a `Location` header, a playlist
    /// entry) into an absolute URL, as RFC 3986 section 5.2.2 does.
    pub fn join(&self, reference: &str) -> Result<Url, FetchError> {
        let reference = reference.trim();
        let reference = reference.split('#').next().unwrap_or("");
        if has_scheme(reference) {
            return Url::parse(reference);
        }
        if let Some(rest) = reference.strip_prefix("//") {
            return Url::parse(&format!("{}://{}", self.scheme.as_str(), rest));
        }
        let (base_path, _) = split_query(&self.target);
        let target = if reference.is_empty() {
            self.target.clone()
        } else if let Some(query) = reference.strip_prefix('?') {
            join_query(base_path, Some(encode(query)))
        } else {
            let (path, query) = split_query(reference);
            let path = encode(path);
            let merged = if path.starts_with('/') {
                path
            } else {
                // RFC 3986 section 5.2.3: everything up to the base path's last slash.
                let cut = base_path.rfind('/').map_or(0, |i| i + 1);
                format!("{}{}", &base_path[..cut], path)
            };
            join_query(&remove_dot_segments(&merged), query.map(encode))
        };
        if target.len() > MAX_URL_BYTES {
            return Err(FetchError::Malformed(format!(
                "url: longer than {MAX_URL_BYTES} bytes"
            )));
        }
        Ok(Url {
            target,
            ..self.clone()
        })
    }

    /// The `Host` header's value: the host, bracketed when IPv6, and the port
    /// when it is not the scheme's default (RFC 9110 section 7.2).
    pub fn host_header(&self) -> String {
        let host = match self.literal {
            Some(IpAddr::V6(_)) => format!("[{}]", self.host),
            _ => self.host.clone(),
        };
        if self.port == self.scheme.default_port() {
            host
        } else {
            format!("{}:{}", host, self.port)
        }
    }
}

impl fmt::Display for Url {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}://{}{}",
            self.scheme.as_str(),
            self.host_header(),
            self.target
        )
    }
}

fn parse_scheme(scheme: &str) -> Result<Scheme, FetchError> {
    if scheme.eq_ignore_ascii_case("http") {
        Ok(Scheme::Http)
    } else if scheme.eq_ignore_ascii_case("https") {
        Ok(Scheme::Https)
    } else if is_scheme(scheme) {
        Err(FetchError::Refused(format!(
            "scheme {}: only http and https are fetched",
            scheme.to_ascii_lowercase()
        )))
    } else {
        Err(FetchError::Malformed("url: no scheme".into()))
    }
}

/// RFC 3986 section 3.1: `ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )`.
fn is_scheme(text: &str) -> bool {
    let mut bytes = text.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_alphabetic())
        && bytes.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
}

fn has_scheme(reference: &str) -> bool {
    match reference.find([':', '/', '?']) {
        Some(i) if reference.as_bytes()[i] == b':' => is_scheme(&reference[..i]),
        _ => false,
    }
}

fn parse_authority(
    authority: &str,
    scheme: Scheme,
) -> Result<(String, Option<IpAddr>, u16), FetchError> {
    if authority.contains('@') {
        return Err(FetchError::Refused(
            "userinfo in the url: a url with user:password@ is not fetched".into(),
        ));
    }
    let (host, port_text) = if let Some(rest) = authority.strip_prefix('[') {
        let Some((inside, after)) = rest.split_once(']') else {
            return Err(FetchError::Malformed("url: unclosed [ in the host".into()));
        };
        let port = match after.strip_prefix(':') {
            Some(port) => Some(port),
            None if after.is_empty() => None,
            None => {
                return Err(FetchError::Malformed(
                    "url: text after ] in the host".into(),
                ))
            }
        };
        let Ok(v6) = inside.parse::<Ipv6Addr>() else {
            return Err(FetchError::Malformed(
                "url: the bracketed host is not an IPv6 address".into(),
            ));
        };
        let port = parse_port(port, scheme)?;
        return Ok((v6.to_string(), Some(IpAddr::V6(v6)), port));
    } else {
        match authority.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        }
    };
    let port = parse_port(port_text, scheme)?;
    if host.is_empty() {
        return Err(FetchError::Malformed("url: no host".into()));
    }
    if host.len() > 253
        || !host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_'))
    {
        return Err(FetchError::Malformed(
            "url: the host is not a name or an address".into(),
        ));
    }
    let host = host.to_ascii_lowercase();
    let literal = host.parse::<Ipv4Addr>().ok().map(IpAddr::V4);
    Ok((host, literal, port))
}

fn parse_port(text: Option<&str>, scheme: Scheme) -> Result<u16, FetchError> {
    match text {
        None | Some("") => Ok(scheme.default_port()),
        Some(digits) => match digits.parse::<u16>() {
            Ok(port) if port != 0 && digits.bytes().all(|b| b.is_ascii_digit()) => Ok(port),
            _ => Err(FetchError::Malformed(
                "url: the port is not 1 to 65535".into(),
            )),
        },
    }
}

fn split_query(target: &str) -> (&str, Option<&str>) {
    match target.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (target, None),
    }
}

fn join_query(path: &str, query: Option<String>) -> String {
    match query {
        Some(query) => format!("{path}?{query}"),
        None => path.to_string(),
    }
}

/// Percent-encodes every byte that may not travel in a request line as it is:
/// controls, space, DEL and everything outside ASCII. What is already encoded
/// stays as it is. This is what keeps a URL from carrying a CR or LF into the
/// request.
fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_graphic()
            && !matches!(
                b,
                b'"' | b'<' | b'>' | b'\\' | b'^' | b'`' | b'{' | b'|' | b'}'
            )
        {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// RFC 3986 section 5.2.4, on a path that starts with `/`.
fn remove_dot_segments(path: &str) -> String {
    let mut kept: Vec<&str> = Vec::new();
    let segments: Vec<&str> = path.split('/').collect();
    let last = segments.len() - 1;
    let mut trailing_slash = false;
    for (i, segment) in segments.iter().enumerate() {
        match *segment {
            "." => trailing_slash = i == last,
            ".." => {
                kept.pop();
                trailing_slash = i == last;
            }
            "" if i == 0 => {}
            other => {
                kept.push(other);
                trailing_slash = false;
            }
        }
    }
    let mut out = String::with_capacity(path.len());
    for segment in &kept {
        out.push('/');
        out.push_str(segment);
    }
    if trailing_slash || out.is_empty() {
        out.push('/');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Url {
        Url::parse(text).unwrap_or_else(|e| panic!("{text}: {e}"))
    }

    #[test]
    fn a_plain_url_has_its_parts() {
        let u = parse("http://radio.example/live/stream.mp3?token=a%20b&x=1");
        assert_eq!(u.scheme, Scheme::Http);
        assert_eq!(u.host, "radio.example");
        assert_eq!(u.literal, None);
        assert_eq!(u.port, 80);
        assert_eq!(u.target, "/live/stream.mp3?token=a%20b&x=1");
        assert_eq!(u.host_header(), "radio.example");
        assert_eq!(
            u.to_string(),
            "http://radio.example/live/stream.mp3?token=a%20b&x=1"
        );
    }

    #[test]
    fn scheme_and_host_are_case_insensitive_and_ports_default() {
        let u = parse("HTTPS://Radio.Example");
        assert_eq!(
            (u.scheme, u.host.as_str(), u.port),
            (Scheme::Https, "radio.example", 443)
        );
        assert_eq!(u.target, "/");
        let u = parse("https://radio.example:8443/a");
        assert_eq!(u.port, 8443);
        assert_eq!(u.host_header(), "radio.example:8443");
        let u = parse("http://radio.example:/a");
        assert_eq!(u.port, 80);
        let u = parse("http://radio.example?x=1");
        assert_eq!(u.target, "/?x=1");
    }

    #[test]
    fn address_literals_are_recognised() {
        let u = parse("http://192.0.2.7:8000/s");
        assert_eq!(u.literal, Some(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 7))));
        let u = parse("http://[2001:db8::1]:8000/s");
        assert_eq!(u.literal, Some("2001:db8::1".parse().unwrap()));
        assert_eq!(u.host, "2001:db8::1");
        assert_eq!(u.host_header(), "[2001:db8::1]:8000");
        assert_eq!(u.to_string(), "http://[2001:db8::1]:8000/s");
        let u = parse("https://[::ffff:192.0.2.1]/");
        assert_eq!(u.host_header(), "[::ffff:192.0.2.1]");
        // Not a dotted quad: left to the resolver, whose answer the policy checks.
        assert_eq!(parse("http://127.1/").literal, None);
    }

    #[test]
    fn the_fragment_is_dropped() {
        assert_eq!(parse("http://radio.example/a#t=10").target, "/a");
        assert_eq!(parse("http://radio.example#frag").target, "/");
    }

    #[test]
    fn other_schemes_are_refused_by_name() {
        for text in [
            "file:///etc/passwd",
            "ftp://radio.example/a",
            "gopher://radio.example/",
            "rtsp://radio.example/s",
        ] {
            match Url::parse(text) {
                Err(FetchError::Refused(rule)) => {
                    assert!(rule.contains("only http and https"), "{text}: {rule}")
                }
                other => panic!("{text}: {other:?}"),
            }
        }
    }

    #[test]
    fn userinfo_is_refused_by_name() {
        for text in [
            "http://user@radio.example/",
            "http://user:secret@radio.example/",
            "http://radio.example@192.0.2.1/",
        ] {
            match Url::parse(text) {
                Err(FetchError::Refused(rule)) => assert!(rule.starts_with("userinfo"), "{rule}"),
                other => panic!("{text}: {other:?}"),
            }
        }
        // An @ in the path is not userinfo.
        assert_eq!(parse("http://radio.example/a@b").target, "/a@b");
    }

    #[test]
    fn what_does_not_parse_is_malformed() {
        for text in [
            "",
            "radio.example/stream",
            "http:/radio.example/",
            "http://",
            "http:///path",
            "http://radio.example:0/",
            "http://radio.example:65536/",
            "http://radio.example:80a/",
            "http://radio.example:+80/",
            "http://[2001:db8::1/",
            "http://[radio.example]/",
            "http://[2001:db8::1]x/",
            "http://radio example/",
            "http://radio%2eexample/",
            "http://radio.example\\@192.0.2.1/",
            "1http://radio.example/",
        ] {
            assert!(
                matches!(Url::parse(text), Err(FetchError::Malformed(_))),
                "{text:?}: {:?}",
                Url::parse(text)
            );
        }
        let long = format!("http://radio.example/{}", "a".repeat(MAX_URL_BYTES));
        assert!(matches!(Url::parse(&long), Err(FetchError::Malformed(_))));
    }

    #[test]
    fn nothing_that_could_split_a_request_line_survives() {
        let u = parse("http://radio.example/a b\r\nX-Injected: 1?q=\u{e9}\t");
        assert_eq!(u.target, "/a%20b%0D%0AX-Injected:%201?q=%C3%A9");
        assert!(u.target.bytes().all(|b| b.is_ascii_graphic()));
    }

    #[test]
    fn dot_segments_are_removed() {
        assert_eq!(parse("http://radio.example/a/./b/../c").target, "/a/c");
        assert_eq!(parse("http://radio.example/../../a").target, "/a");
        assert_eq!(parse("http://radio.example/a/..").target, "/");
        assert_eq!(parse("http://radio.example/a/b/.").target, "/a/b/");
        assert_eq!(parse("http://radio.example/a//b").target, "/a//b");
    }

    #[test]
    fn references_resolve_against_the_base() {
        // RFC 3986 section 5.4.1's examples, for the forms this parser keeps.
        let base = parse("http://radio.example/b/c/d;p?q");
        for (reference, want) in [
            ("g", "http://radio.example/b/c/g"),
            ("./g", "http://radio.example/b/c/g"),
            ("g/", "http://radio.example/b/c/g/"),
            ("/g", "http://radio.example/g"),
            ("//other.example/g", "http://other.example/g"),
            ("?y", "http://radio.example/b/c/d;p?y"),
            ("g?y", "http://radio.example/b/c/g?y"),
            ("#s", "http://radio.example/b/c/d;p?q"),
            ("g#s", "http://radio.example/b/c/g"),
            (";x", "http://radio.example/b/c/;x"),
            ("", "http://radio.example/b/c/d;p?q"),
            (".", "http://radio.example/b/c/"),
            ("..", "http://radio.example/b/"),
            ("../g", "http://radio.example/b/g"),
            ("../..", "http://radio.example/"),
            ("../../g", "http://radio.example/g"),
            ("../../../g", "http://radio.example/g"),
            (
                "https://other.example:8443/x",
                "https://other.example:8443/x",
            ),
        ] {
            assert_eq!(
                base.join(reference).unwrap().to_string(),
                want,
                "{reference}"
            );
        }
    }

    #[test]
    fn a_reference_keeps_the_base_port_and_cannot_smuggle_a_scheme() {
        let base = parse("https://radio.example:8443/hls/live.m3u8");
        let seg = base.join("seg 1.mp3").unwrap();
        assert_eq!(
            seg.to_string(),
            "https://radio.example:8443/hls/seg%201.mp3"
        );
        assert!(matches!(
            base.join("file:///etc/passwd"),
            Err(FetchError::Refused(_))
        ));
        assert!(matches!(
            base.join("//user@other.example/x"),
            Err(FetchError::Refused(_))
        ));
        // A colon after a slash is a path, not a scheme.
        assert_eq!(base.join("a/b:c").unwrap().target, "/hls/a/b:c");
    }
}
