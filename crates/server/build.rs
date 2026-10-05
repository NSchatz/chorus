//! Embeds the app's committed build output, `web/dist`, in the server.
//!
//! The standard library alone: no build dependency, and no other program is
//! started. The output is already in the tree as bytes (`web/README.md`; the
//! gate rebuilds it and fails on a difference), so this script only walks the
//! directory and writes `$OUT_DIR/app_files.rs`, one `AppFile` per file, which
//! `src/app.rs` includes. A file added to `web/dist` is therefore served after
//! the next build of this crate with no route written by hand
//! (`docs/decisions/0000-the-app-is-served-under-app.md`).
//!
//! What it decides per file, at build time so that a mistake stops the build
//! and not a browser:
//!
//! - the path under `/app/`, with `/` separators on every host;
//! - the media type, from the extension, out of the closed table below. An
//!   extension the table does not name fails the build: under the server's
//!   Content-Security-Policy a script or a stylesheet with the wrong type is
//!   silently not applied, so a type is never guessed;
//! - the entity tag, from the file's bytes.

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

/// The extensions the app may ship and the `Content-Type` each is served with.
const MEDIA_TYPES: &[(&str, &str)] = &[
    ("html", "text/html; charset=utf-8"),
    ("css", "text/css; charset=utf-8"),
    ("js", "text/javascript; charset=utf-8"),
    ("mjs", "text/javascript; charset=utf-8"),
    ("json", "application/json"),
    ("map", "application/json"),
    ("webmanifest", "application/manifest+json"),
    ("txt", "text/plain; charset=utf-8"),
    ("svg", "image/svg+xml"),
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("webp", "image/webp"),
    ("ico", "image/x-icon"),
    ("woff2", "font/woff2"),
    ("wasm", "application/wasm"),
];

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets it"));
    let dist = manifest.join("../../web/dist");
    let dist = dist.canonicalize().unwrap_or_else(|e| {
        panic!(
            "{}: {e}. chorus-server embeds the app's committed output; a build context \
             (an image, a copy of the tree) has to carry web/dist",
            dist.display()
        )
    });

    let mut files = Vec::new();
    walk(&dist, &mut files);
    // The table's order is the paths' order, never the directory's.
    files.sort();
    assert!(
        files.iter().any(|f| f == &dist.join("index.html")),
        "{} has no index.html: the app's output is incomplete",
        dist.display()
    );

    let mut table = String::from("&[\n");
    for file in &files {
        let relative = file.strip_prefix(&dist).expect("walked from the root");
        let path = relative
            .iter()
            .map(|part| {
                part.to_str()
                    .unwrap_or_else(|| panic!("{}: a name that is not UTF-8", file.display()))
            })
            .collect::<Vec<_>>()
            .join("/");
        assert!(
            path.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'-' | b'_')),
            "web/dist/{path}: a served path is letters, digits, '.', '-', '_' and '/' only"
        );
        let extension = relative.extension().and_then(|e| e.to_str()).unwrap_or("");
        let media_type = MEDIA_TYPES
            .iter()
            .find(|(known, _)| *known == extension)
            .map(|(_, media_type)| *media_type)
            .unwrap_or_else(|| {
                panic!(
                    "web/dist/{path}: no media type for the extension {extension:?}; \
                     add it to MEDIA_TYPES in crates/server/build.rs"
                )
            });
        let body = fs::read(file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        println!("cargo:rerun-if-changed={}", file.display());
        writeln!(
            table,
            "    AppFile {{ path: {path:?}, media_type: {media_type:?}, etag: {:?}, \
             body: include_bytes!({:?}) }},",
            entity_tag(&body),
            file.to_str()
                .expect("checked above, and the root is cargo's"),
        )
        .expect("writing to a String");
    }
    table.push_str("]\n");

    let out = PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets it")).join("app_files.rs");
    fs::write(&out, table).unwrap_or_else(|e| panic!("{}: {e}", out.display()));
    println!("cargo:rerun-if-changed=build.rs");
}

/// Every regular file under `dir`, and a rebuild whenever a directory's
/// entries change (a file added, removed or renamed).
fn walk(dir: &Path, files: &mut Vec<PathBuf>) {
    println!("cargo:rerun-if-changed={}", dir.display());
    let entries = fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    for entry in entries {
        let path = entry
            .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
            .path();
        if path.is_dir() {
            walk(&path, files);
        } else {
            files.push(path);
        }
    }
}

/// A strong entity tag: the file's 64-bit FNV-1a hash and its length, quoted.
///
/// A validator, not a signature: it has to differ when the committed bytes
/// differ, and nobody chooses those bytes to make it collide. The standard
/// library has no cryptographic hash and this script takes no dependency.
fn entity_tag(body: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in body {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("\"{hash:016x}-{:x}\"", body.len())
}
