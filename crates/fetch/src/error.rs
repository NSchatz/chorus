//! Why a fetch did not produce bytes.

use std::fmt;
use std::io;

/// Why `open` (or a later read or seek) failed.
///
/// A refusal names its rule and an unsupported stream names what it is, so the
/// words can be shown to the owner as they are (`docs/streams.md` lists them).
#[derive(Debug)]
pub enum FetchError {
    /// The fetch policy refused the URL or an address, by the rule's name.
    Refused(String),
    /// The stream is something chorus does not play, by name (`hls: aac ...`).
    Unsupported(String),
    /// The server answered with this HTTP status.
    Http(u16),
    /// TLS could not be set up or the certificate was not accepted.
    Tls(String),
    /// A socket error, a timeout among them.
    Io(io::Error),
    /// The URL, the response or a playlist does not parse.
    Malformed(String),
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FetchError::Refused(rule) => write!(f, "refused: {rule}"),
            FetchError::Unsupported(what) => write!(f, "unsupported: {what}"),
            FetchError::Http(status) => write!(f, "http status {status}"),
            FetchError::Tls(why) => write!(f, "tls: {why}"),
            FetchError::Io(e) => write!(f, "io: {e}"),
            FetchError::Malformed(why) => write!(f, "malformed: {why}"),
        }
    }
}

impl std::error::Error for FetchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            FetchError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for FetchError {
    /// An `io::Error` that carries a `FetchError` (see [`FetchError::into_io`])
    /// comes back as that error; any other is `Io`.
    fn from(e: io::Error) -> Self {
        if e.get_ref().is_some_and(|inner| inner.is::<FetchError>()) {
            let kind = e.kind();
            return match e.into_inner().map(|inner| inner.downcast::<FetchError>()) {
                Some(Ok(fetch)) => *fetch,
                _ => FetchError::Io(io::Error::from(kind)),
            };
        }
        FetchError::Io(e)
    }
}

impl FetchError {
    /// The error as a `Read` or `seek` must return it. `Io` is passed through;
    /// everything else rides inside an `io::Error` and is found again with
    /// [`FetchError::in_io`].
    pub fn into_io(self) -> io::Error {
        let kind = match &self {
            FetchError::Io(_) => {
                return match self {
                    FetchError::Io(e) => e,
                    _ => io::Error::from(io::ErrorKind::Other),
                }
            }
            FetchError::Refused(_) => io::ErrorKind::PermissionDenied,
            FetchError::Unsupported(_) => io::ErrorKind::Unsupported,
            FetchError::Malformed(_) | FetchError::Tls(_) => io::ErrorKind::InvalidData,
            FetchError::Http(_) => io::ErrorKind::Other,
        };
        io::Error::new(kind, self)
    }

    /// The `FetchError` a read or a seek failed with, when the `io::Error`
    /// carries one (a refusal met in the middle of a stream, for example an
    /// HLS segment that turns out to be AAC).
    pub fn in_io(e: &io::Error) -> Option<&FetchError> {
        e.get_ref().and_then(|inner| inner.downcast_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_words_name_the_kind() {
        assert_eq!(
            FetchError::Refused("loopback address 127.0.0.1".into()).to_string(),
            "refused: loopback address 127.0.0.1"
        );
        assert_eq!(
            FetchError::Unsupported("hls: aac (mp4a.40.2)".into()).to_string(),
            "unsupported: hls: aac (mp4a.40.2)"
        );
        assert_eq!(FetchError::Http(404).to_string(), "http status 404");
    }

    #[test]
    fn a_fetch_error_survives_the_trip_through_io() {
        let io = FetchError::Unsupported("hls: aac".into()).into_io();
        assert_eq!(io.kind(), io::ErrorKind::Unsupported);
        assert!(
            matches!(FetchError::in_io(&io), Some(FetchError::Unsupported(w)) if w == "hls: aac")
        );
        assert!(matches!(FetchError::from(io), FetchError::Unsupported(w) if w == "hls: aac"));
        let plain = io::Error::from(io::ErrorKind::TimedOut);
        assert!(FetchError::in_io(&plain).is_none());
        assert!(
            matches!(FetchError::from(plain), FetchError::Io(e) if e.kind() == io::ErrorKind::TimedOut)
        );
    }
}
