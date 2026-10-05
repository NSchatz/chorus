//! The app, served under `/app/` from inside this binary.
//!
//! The app's build output (`web/dist`, committed) is compiled in by
//! `build.rs`, which walks the directory and writes the table [`files`]
//! returns: there is no route per file, so a file the build adds is served by
//! the next build of this crate. Nothing here opens a file at run time, and a
//! request's path is only ever compared with the table's, so no request can
//! name a file outside it.
//!
//! `/app/` is the app's permanent path, and these are its rules
//! (`docs/decisions/0000-the-app-is-served-under-app.md`):
//!
//! - `GET /app/` and `GET /app/index.html` are the document; `GET /app/<path>`
//!   is that file of the output; `GET /app` redirects to `/app/`, because the
//!   document names its assets relative to the directory; anything else under
//!   `/app/` is a `404`.
//! - Every file carries its media type, a strong `ETag` made from its bytes,
//!   and the control page's Content-Security-Policy, unchanged. A request whose
//!   `If-None-Match` names the file's tag is answered `304` with no body.
//! - A file whose name carries the hash of its content (`assets/<name>-<hash>.<ext>`)
//!   is `immutable` for a year: a change to it is a new name. Everything else,
//!   the document and the service worker's reserved path [`SERVICE_WORKER_PATH`]
//!   first among them, is `no-cache`: kept, and revalidated before every use,
//!   so a new build is seen at the next load.
//!
//! Nothing is compressed, and nothing here is on the audio path: a control
//! worker writes the response and goes back to the pool.

use std::io::Write;
use std::net::TcpStream;

/// One file of the app's build output.
#[derive(Debug)]
pub struct AppFile {
    /// The path under `/app/`, with no leading slash: `index.html`,
    /// `assets/main-OC5E6PTU.js`.
    pub path: &'static str,
    /// The `Content-Type` it is served with.
    pub media_type: &'static str,
    /// The strong entity tag of its bytes, quotes included.
    pub etag: &'static str,
    /// Its bytes, as committed.
    pub body: &'static [u8],
}

/// The table `build.rs` writes from `web/dist`, in the order of the paths.
static FILES: &[AppFile] = include!(concat!(env!("OUT_DIR"), "/app_files.rs"));

/// Where the app is mounted. Permanent: an installed app's start URL, its
/// service worker's scope and every bookmark are under it.
pub const MOUNT: &str = "/app/";

/// The document, which `/app/` itself answers with.
pub const DOCUMENT_PATH: &str = "index.html";

/// The path reserved for the service worker, under [`MOUNT`], so that its
/// default scope is the app and nothing else. Never `immutable`, whatever a
/// build names it: a browser must be able to fetch a new one.
pub const SERVICE_WORKER_PATH: &str = "sw.js";

/// `Cache-Control` of a file that is revalidated before every use.
pub const REVALIDATE: &str = "no-cache";

/// `Cache-Control` of a file named by the hash of its content.
pub const IMMUTABLE: &str = "public, max-age=31536000, immutable";

/// Every embedded file.
pub fn files() -> &'static [AppFile] {
    FILES
}

/// The file a request path names, if it names one. `path` is the request's
/// path with its query already cut off.
pub fn lookup(path: &str) -> Option<&'static AppFile> {
    let relative = path.strip_prefix(MOUNT)?;
    let relative = if relative.is_empty() {
        DOCUMENT_PATH
    } else {
        relative
    };
    FILES.iter().find(|file| file.path == relative)
}

/// The `Cache-Control` of the file at `path` (relative to [`MOUNT`]).
///
/// [`IMMUTABLE`] only for `assets/<name>-<hash>.<ext>`, the shape the app's
/// build gives a file named by its content: the hash is eight characters of
/// `A-Z` and `0-9`. Anything else is [`REVALIDATE`], which is the safe answer
/// for a file this function knows nothing about.
pub fn cache_control(path: &str) -> &'static str {
    if path == DOCUMENT_PATH || path == SERVICE_WORKER_PATH {
        return REVALIDATE;
    }
    let hashed = path
        .strip_prefix("assets/")
        .filter(|name| !name.contains('/'))
        .and_then(|name| name.rsplit_once('.'))
        .and_then(|(stem, _extension)| stem.rsplit_once('-'))
        .is_some_and(|(name, hash)| {
            !name.is_empty()
                && hash.len() == 8
                && hash
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        });
    if hashed {
        IMMUTABLE
    } else {
        REVALIDATE
    }
}

/// Whether an `If-None-Match` header names `etag`: `*`, or a list in which one
/// tag is this one. The comparison is the weak one RFC 9110 section 13.1.2
/// asks for, so a `W/` prefix a cache in between added does not defeat it.
pub fn none_match_names(header: &str, etag: &str) -> bool {
    header.split(',').map(str::trim).any(|candidate| {
        candidate == "*" || candidate.strip_prefix("W/").unwrap_or(candidate) == etag
    })
}

/// Answer `GET <path>` for a path that is `/app` or under `/app/`. Returns
/// `false`, having written nothing, when the path names no file: the caller
/// answers as it does for any route it does not have.
///
/// `policy` is the Content-Security-Policy every response to a browser carries.
pub fn respond(
    connection: &mut TcpStream,
    path: &str,
    if_none_match: Option<&str>,
    policy: &str,
) -> bool {
    if path == MOUNT.trim_end_matches('/') {
        let _ = write!(
            connection,
            "HTTP/1.1 308 Permanent Redirect\r\nLocation: {MOUNT}\r\nContent-Length: 0\r\n\
             Cache-Control: {REVALIDATE}\r\nContent-Security-Policy: {policy}\r\n\
             Connection: close\r\n\r\n"
        );
        let _ = connection.flush();
        return true;
    }
    let Some(file) = lookup(path) else {
        return false;
    };
    let cache = cache_control(file.path);
    let etag = file.etag;
    if if_none_match.is_some_and(|header| none_match_names(header, etag)) {
        let _ = write!(
            connection,
            "HTTP/1.1 304 Not Modified\r\nETag: {etag}\r\nCache-Control: {cache}\r\n\
             Content-Security-Policy: {policy}\r\nConnection: close\r\n\r\n"
        );
        let _ = connection.flush();
        return true;
    }
    let sent = write!(
        connection,
        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nETag: {etag}\r\n\
         Cache-Control: {cache}\r\nX-Content-Type-Options: nosniff\r\n\
         Content-Security-Policy: {policy}\r\nConnection: close\r\n\r\n",
        file.media_type,
        file.body.len(),
    );
    if sent.is_ok() {
        let _ = connection.write_all(file.body);
    }
    let _ = connection.flush();
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_document_and_the_service_worker_are_revalidated_and_hashed_assets_are_immutable() {
        assert_eq!(cache_control("index.html"), REVALIDATE);
        assert_eq!(cache_control(SERVICE_WORKER_PATH), REVALIDATE);
        assert_eq!(cache_control("assets/main-OC5E6PTU.js"), IMMUTABLE);
        assert_eq!(cache_control("assets/app-G3ZOX776.css"), IMMUTABLE);
        // Not named by a hash, so never immutable: under assets/ without one,
        // a hash of the wrong shape, a hashed name outside assets/, a manifest.
        for path in [
            "assets/main.js",
            "assets/main-oc5e6ptu.js",
            "assets/main-OC5E6PT.js",
            "assets/-OC5E6PTU.js",
            "assets/deep/main-OC5E6PTU.js",
            "main-OC5E6PTU.js",
            "manifest.webmanifest",
            "assets/sw.js",
        ] {
            assert_eq!(cache_control(path), REVALIDATE, "{path}");
        }
    }

    #[test]
    fn if_none_match_is_a_list_a_star_or_a_weak_tag() {
        let tag = "\"00ff-1\"";
        assert!(none_match_names(tag, tag));
        assert!(none_match_names("*", tag));
        assert!(none_match_names("\"other\", W/\"00ff-1\"", tag));
        assert!(!none_match_names("\"other\"", tag));
        assert!(!none_match_names("00ff-1", tag));
        assert!(!none_match_names("", tag));
    }

    #[test]
    fn only_a_path_of_the_table_is_a_file() {
        assert_eq!(lookup("/app/").map(|f| f.path), Some(DOCUMENT_PATH));
        assert_eq!(
            lookup("/app/index.html").map(|f| f.path),
            Some(DOCUMENT_PATH)
        );
        for path in [
            "/app",
            "/app/nothing.js",
            "/app/../Cargo.toml",
            "/app//",
            "/application/",
        ] {
            assert!(lookup(path).is_none(), "{path}");
        }
    }
}
