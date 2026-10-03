//! Firmware images on the server: staged, verified, and sent to a speaker
//! only when somebody says so (goal 14; K93, I13).
//!
//! # The rule this module exists to keep
//!
//! Nothing installs without an explicit install action. Staging an image in
//! `--firmware-dir` lists it. A speaker whose board has a staged image of
//! another version is shown `update_available`. A session coming up, a
//! reconnect, a server restart: none of them sends a `firmware_offer`. The
//! one path to [`Firmware::start`] is the control plane applying a
//! `firmware_install` command (`crate::control::ControlState::apply`), and a
//! transfer that was in progress when the server stopped is not resumed by
//! the next one: this module persists nothing, and a speaker found receiving
//! a transfer nobody here is sending is told to abandon it.
//!
//! # Staging and verification
//!
//! One image is two files in the firmware directory: `<name>.bin` and
//! `<name>.manifest`, a text file of `key = value` lines (`#` comments)
//! holding exactly `version`, `board`, `size` and `sha256`. [`scan`] reads
//! the directory at start and on every `firmware_rescan` and grades each
//! image ([`Verified`] or refused with a reason by name): the name is a
//! catalog identifier; the manifest reads; the file's size is the manifest's
//! and within the wire's bound; its SHA-256 is the manifest's; its first byte
//! is the ESP application image magic `0xE9`; and the version in its
//! application description is the manifest's, so the version the server
//! offers is the version the speaker will report once it runs the image.
//! A refused image is listed with its reason and is never offered. The file
//! is read again, and its size and digest checked again, when an install is
//! commanded: what is sent is what was verified, or nothing is.
//!
//! The image format facts are ESP-IDF v6.1's (Apache-2.0, read 2026-10-02 at
//! the pinned tag): `ESP_IMAGE_HEADER_MAGIC 0xE9`
//! (`components/bootloader_support/include/esp_app_format.h:77`), the header
//! is 24 bytes (`:111`), and the application description sits at the fixed
//! offset `sizeof(esp_image_header_t) + sizeof(esp_image_segment_header_t)`
//! = 32 (`docs/en/api-reference/system/app_image_format.rst:135`), its magic
//! word `0xABCD5432` and its 32-byte `version` at offset 16 of the structure
//! (`components/esp_app_format/include/esp_app_desc.h:21-30`).
//!
//! # The sender: no thread of its own
//!
//! Image bytes travel in the speaker's own session
//! (`docs/protocol.md`, "Firmware update"): the offer and every chunk are
//! put on that session's own outbound queue, the one its audio is on, and
//! sealed by its own writer. Who puts them there:
//!
//! - the control worker applying `firmware_install` queues the offer (one
//!   small frame, never blocking);
//! - the session's own READER thread does the rest. Each `firmware_status`
//!   the speaker sends arrives on it, and it looks at its transfer every time
//!   it looks up from its socket ([`Firmware::pump`], at least every
//!   `crate::session::STREAM_READ_TIMEOUT`): it tops the queue up to the
//!   window and no further.
//!
//! So the thread population is what it was (`6 + 2N + M`), nothing here
//! sleeps or reads a clock, and a transfer cannot outrun its speaker: at most
//! [`WINDOW_CHUNKS`] chunks of [`CHUNK_BYTES`] are beyond the speaker's last
//! acknowledgement, which bounds both the bytes ahead of an audio chunk on
//! the socket and the queue slots firmware holds (16 of
//! `crate::stream::SUBSCRIBER_QUEUE_LIMIT`'s 128). A full queue is not
//! waited on: the chunk is offered to it again on the next look.
//!
//! # The owner-at-bench guard (program section 0.7)
//!
//! A transfer to a peer whose address is not loopback writes to a real
//! device, and that is the owner's action. [`Firmware::start`] refuses it,
//! by the name `owner-not-at-bench`, unless the owner's bench variable reads
//! exactly `1` in this process's environment ([`owner_at_bench`], the one
//! approved read form; `tools/conventions/check-flash-guard.sh`). Nothing in
//! this repository sets it; `docs/firmware-updates.md` says how the owner's
//! deploy does. Tests and the emulator run reach the server on loopback.

use std::fs;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use chorus_control::catalog::is_identifier;
use chorus_control::firmware::{Image, Report, MAX_IMAGES};
use chorus_control::Refusal;
use chorus_protocol::v2::{
    features, FirmwareChunk, FirmwareOffer, FirmwareReason, FirmwareState, FirmwareStatus, Message,
    FIRMWARE_ACK_EVERY, FIRMWARE_MAX_SIZE, FIRMWARE_MAX_TEXT, FIRMWARE_SLOT_UNKNOWN,
    FIRMWARE_WINDOW_CHUNKS,
};

use crate::router::Router;

/// The most bytes one `firmware_chunk` of this server carries.
///
/// ASSUMED: 1024, a quarter of the wire's bound
/// (`chorus_protocol::v2::FIRMWARE_MAX_CHUNK_BYTES`). A chunk shares the
/// session's connection with its audio, and a smaller one is a shorter wait
/// for the audio chunk queued behind it. Not measured: no timing claim is
/// made of it, and a bench session may move it.
pub const CHUNK_BYTES: u16 = 1024;

/// The most chunks this server keeps beyond the speaker's last
/// acknowledgement.
///
/// ASSUMED: one acknowledgement's worth (`FIRMWARE_ACK_EVERY`, 16), half of
/// what the wire allows (`FIRMWARE_WINDOW_CHUNKS`, 32). The speaker
/// acknowledges every 16 chunks it has written, so a smaller window would
/// stall and a larger one only puts more firmware ahead of the audio. Not
/// measured.
pub const WINDOW_CHUNKS: u32 = FIRMWARE_ACK_EVERY;

// The window this server uses must be one the wire's rule allows, and one an
// acknowledgement can open again.
const _: () =
    assert!(WINDOW_CHUNKS <= FIRMWARE_WINDOW_CHUNKS && WINDOW_CHUNKS >= FIRMWARE_ACK_EVERY);

/// The first byte of an ESP application image (`ESP_IMAGE_HEADER_MAGIC`).
pub const ESP_IMAGE_MAGIC: u8 = 0xE9;

/// Where an ESP application image's description starts: after the 24-byte
/// image header and the 8-byte header of its first segment.
const APP_DESCRIPTION_OFFSET: usize = 24 + 8;

/// The description's magic word (`ESP_APP_DESC_MAGIC_WORD`), little-endian
/// in the image.
const APP_DESCRIPTION_MAGIC: u32 = 0xABCD_5432;

/// Where the description's `version` field starts, and its length.
const APP_VERSION_OFFSET: usize = APP_DESCRIPTION_OFFSET + 16;
const APP_VERSION_LEN: usize = 32;

/// The file extension of an image and of its manifest.
const IMAGE_EXTENSION: &str = "bin";
const MANIFEST_EXTENSION: &str = "manifest";

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// The SHA-256 of `bytes` as 64 lower-case hex digits: the form a manifest
/// and the state message hold. The digest is the protocol crate's own
/// (`FirmwareOffer::digest_of`), the one the offer carries.
pub fn digest_hex(bytes: &[u8]) -> String {
    hex(&FirmwareOffer::digest_of(bytes))
}

fn parse_digest(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64 || !text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(text.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

/// Whether `text` can travel as an offer's `version` or `board`: 1 to
/// [`FIRMWARE_MAX_TEXT`] bytes with no control character.
fn is_wire_text(text: &str) -> bool {
    !text.is_empty() && text.len() <= FIRMWARE_MAX_TEXT && !text.chars().any(|c| c.is_control())
}

/// What a manifest declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// The image's version.
    pub version: String,
    /// The board profile it was built for.
    pub board: String,
    /// Its size, bytes.
    pub size: u64,
    /// Its SHA-256, 64 lower-case hex digits.
    pub sha256: String,
}

impl Manifest {
    /// Read a manifest: `key = value` lines, `#` comments, exactly the four
    /// keys, each once. The error says what is wrong in words.
    pub fn parse(text: &str) -> Result<Manifest, String> {
        let mut version = None;
        let mut board = None;
        let mut size = None;
        let mut sha256 = None;
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (key, value) = line
                .split_once('=')
                .map(|(k, v)| (k.trim(), v.trim()))
                .ok_or_else(|| format!("line {} is not 'key = value'", n + 1))?;
            let slot = match key {
                "version" => &mut version,
                "board" => &mut board,
                "size" => &mut size,
                "sha256" => &mut sha256,
                other => {
                    return Err(format!(
                        "line {} sets '{}', which a manifest does not hold",
                        n + 1,
                        other
                    ))
                }
            };
            if slot.is_some() {
                return Err(format!("'{}' is set twice", key));
            }
            *slot = Some(value.to_string());
        }
        let need = |v: Option<String>, key: &str| v.ok_or_else(|| format!("'{}' is missing", key));
        let version = need(version, "version")?;
        let board = need(board, "board")?;
        let size = need(size, "size")?;
        let sha256 = need(sha256, "sha256")?;
        if !is_wire_text(&version) {
            return Err(format!(
                "the version is not 1 to {} bytes of text",
                FIRMWARE_MAX_TEXT
            ));
        }
        if !is_wire_text(&board) {
            return Err(format!(
                "the board is not 1 to {} bytes of text",
                FIRMWARE_MAX_TEXT
            ));
        }
        let size = size
            .parse::<u64>()
            .map_err(|_| format!("the size '{}' is not a whole number of bytes", size))?;
        if parse_digest(&sha256).is_none() {
            return Err("the sha256 is not 64 lower-case hex digits".to_string());
        }
        Ok(Manifest {
            version,
            board,
            size,
            sha256,
        })
    }

    /// The manifest's text, as the staging helper writes it.
    pub fn render(&self) -> String {
        format!(
            "# chorus firmware manifest (docs/firmware-updates.md)\nversion = {}\nboard = {}\nsize = {}\nsha256 = {}\n",
            self.version, self.board, self.size, self.sha256
        )
    }
}

/// The version in an ESP application image's description, or why there is
/// none to read.
pub fn app_description_version(image: &[u8]) -> Result<String, &'static str> {
    if image.first() != Some(&ESP_IMAGE_MAGIC) {
        return Err("not-an-esp-image");
    }
    let magic = image
        .get(APP_DESCRIPTION_OFFSET..APP_DESCRIPTION_OFFSET + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    if magic != Some(APP_DESCRIPTION_MAGIC) {
        return Err("no-app-description");
    }
    let field = image
        .get(APP_VERSION_OFFSET..APP_VERSION_OFFSET + APP_VERSION_LEN)
        .ok_or("no-app-description")?;
    let end = field.iter().position(|b| *b == 0).unwrap_or(field.len());
    match std::str::from_utf8(&field[..end]) {
        Ok(version) if !version.is_empty() => Ok(version.to_string()),
        _ => Err("no-app-description"),
    }
}

/// Grade the bytes of an image against what its manifest declares: `None`
/// when they agree, else the reason's name.
pub fn refusal_of(manifest: &Manifest, image: &[u8]) -> Option<&'static str> {
    if manifest.size == 0 {
        return Some("empty");
    }
    if manifest.size > u64::from(FIRMWARE_MAX_SIZE) {
        return Some("too-large");
    }
    if image.len() as u64 != manifest.size {
        return Some("size-mismatch");
    }
    if digest_hex(image) != manifest.sha256 {
        return Some("digest-mismatch");
    }
    match app_description_version(image) {
        Err(reason) => Some(reason),
        Ok(version) if version != manifest.version => Some("version-mismatch"),
        Ok(_) => None,
    }
}

/// Read a file that is at most the wire's image bound, without reading one
/// that is larger into memory first.
fn read_bounded(path: &Path) -> Result<Vec<u8>, &'static str> {
    let meta = fs::metadata(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "no-image-file"
        } else {
            "unreadable"
        }
    })?;
    if !meta.is_file() {
        return Err("no-image-file");
    }
    if meta.len() > u64::from(FIRMWARE_MAX_SIZE) {
        return Err("too-large");
    }
    fs::read(path).map_err(|_| "unreadable")
}

/// Grade one staged image by name.
fn grade(dir: &Path, name: &str) -> Image {
    let mut image = Image {
        name: name.to_string(),
        version: String::new(),
        board: String::new(),
        size: 0,
        sha256: String::new(),
        refused: None,
    };
    let refuse = |mut image: Image, reason: &str| {
        image.refused = Some(reason.to_string());
        image
    };
    if !is_identifier(name) {
        return refuse(image, "name-not-an-identifier");
    }
    let manifest = match fs::read_to_string(dir.join(format!("{}.{}", name, MANIFEST_EXTENSION))) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return refuse(image, "no-manifest"),
        Err(_) => return refuse(image, "unreadable"),
    };
    let manifest = match Manifest::parse(&manifest) {
        Ok(m) => m,
        Err(_) => return refuse(image, "bad-manifest"),
    };
    image.version = manifest.version.clone();
    image.board = manifest.board.clone();
    image.size = manifest.size;
    image.sha256 = manifest.sha256.clone();
    let bytes = match read_bounded(&dir.join(format!("{}.{}", name, IMAGE_EXTENSION))) {
        Ok(bytes) => bytes,
        Err(reason) => return refuse(image, reason),
    };
    match refusal_of(&manifest, &bytes) {
        Some(reason) => refuse(image, reason),
        None => image,
    }
}

/// Whether a file stem can be listed at all: text a state message can carry
/// on one line. (Whether it can be INSTALLED is the identifier rule, graded
/// per image.)
fn listable(stem: &str) -> bool {
    !stem.is_empty() && stem.chars().count() <= 64 && !stem.chars().any(|c| c.is_control())
}

/// Read the firmware directory: every image staged in it, sorted by name,
/// each graded, and a line for the log about each and about anything left
/// out. A directory that cannot be read is an empty list and a line saying
/// so; it is never a reason to offer something.
pub fn scan(dir: &Path) -> (Vec<Image>, Vec<String>) {
    let mut lines = Vec::new();
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            lines.push(format!(
                "firmware dir={} unreadable detail=\"{}\"; no image is staged",
                dir.display(),
                e
            ));
            return (Vec::new(), lines);
        }
    };
    let mut names: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let extension = path.extension().and_then(|e| e.to_str());
        if !matches!(extension, Some(IMAGE_EXTENSION) | Some(MANIFEST_EXTENSION)) {
            continue;
        }
        match path.file_stem().and_then(|s| s.to_str()) {
            Some(stem) if listable(stem) => names.push(stem.to_string()),
            _ => lines.push(format!(
                "firmware image not listed file={:?} reason=name-not-text",
                entry.file_name()
            )),
        }
    }
    names.sort();
    names.dedup();
    let mut images = Vec::new();
    for name in names {
        if images.len() >= MAX_IMAGES {
            lines.push(format!(
                "firmware image not listed name={} reason=too-many detail=\"this server lists at \
                 most {} staged images\"",
                name, MAX_IMAGES
            ));
            continue;
        }
        let image = grade(dir, &name);
        lines.push(format!(
            "firmware image name={} version=\"{}\" board=\"{}\" size={} verdict={}{}",
            image.name,
            image.version,
            image.board,
            image.size,
            if image.verified() {
                "verified"
            } else {
                "refused"
            },
            image
                .refused
                .as_ref()
                .map(|r| format!(" reason={}", r))
                .unwrap_or_default()
        ));
        images.push(image);
    }
    let verified = images.iter().filter(|i| i.verified()).count();
    lines.push(format!(
        "firmware dir={} images={} verified={} refused={}",
        dir.display(),
        images.len(),
        verified,
        images.len() - verified
    ));
    (images, lines)
}

/// The staging helper: put the image at `source` into the firmware directory
/// `dir` as `<name>.bin` with a manifest written from the image itself (its
/// size, its SHA-256, the version in its application description) and the
/// board it was built for. Local files only: nothing here opens a socket.
///
/// `name` defaults to the board and the version, spelled as an identifier.
/// The result is graded the way the server will grade it, so a helper that
/// reports success staged an image the server will verify.
pub fn stage(source: &Path, dir: &Path, name: Option<&str>, board: &str) -> Result<Image, String> {
    let bytes =
        read_bounded(source).map_err(|reason| format!("{}: {}", source.display(), reason))?;
    let version = app_description_version(&bytes)
        .map_err(|reason| format!("{}: {}", source.display(), reason))?;
    let name = match name {
        Some(name) => name.to_string(),
        None => default_name(board, &version),
    };
    if !is_identifier(&name) {
        return Err(format!(
            "'{}' is not an identifier (1 to 32 lower-case letters, digits and hyphens); pass \
             --name",
            name
        ));
    }
    let manifest = Manifest {
        version,
        board: board.to_string(),
        size: bytes.len() as u64,
        sha256: digest_hex(&bytes),
    };
    // The manifest as written must be one the server reads.
    Manifest::parse(&manifest.render())?;
    fs::create_dir_all(dir).map_err(|e| format!("{}: {}", dir.display(), e))?;
    // The image first, the manifest last and by rename: a server scanning
    // between the two sees an image with no manifest (refused), never a
    // manifest that describes half a file.
    let image_path = dir.join(format!("{}.{}", name, IMAGE_EXTENSION));
    let manifest_path = dir.join(format!("{}.{}", name, MANIFEST_EXTENSION));
    let _ = fs::remove_file(&manifest_path);
    let partial = dir.join(format!(".{}.{}.staging", name, IMAGE_EXTENSION));
    fs::write(&partial, &bytes).map_err(|e| format!("{}: {}", partial.display(), e))?;
    fs::rename(&partial, &image_path).map_err(|e| format!("{}: {}", image_path.display(), e))?;
    let partial = dir.join(format!(".{}.{}.staging", name, MANIFEST_EXTENSION));
    fs::write(&partial, manifest.render()).map_err(|e| format!("{}: {}", partial.display(), e))?;
    fs::rename(&partial, &manifest_path)
        .map_err(|e| format!("{}: {}", manifest_path.display(), e))?;
    let image = grade(dir, &name);
    match &image.refused {
        None => Ok(image),
        Some(reason) => Err(format!("staged as '{}' but refused: {}", name, reason)),
    }
}

/// `chorus-server stage-firmware`'s arguments: `--image <file.bin> --board
/// <profile> --firmware-dir <dir> [--name <name>]`, each once. What it
/// staged, as one line, or what is wrong.
pub fn stage_command(args: &[String]) -> Result<String, String> {
    let mut image = None;
    let mut board = None;
    let mut dir = None;
    let mut name = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let slot = match arg.as_str() {
            "--image" => &mut image,
            "--board" => &mut board,
            "--firmware-dir" => &mut dir,
            "--name" => &mut name,
            other => {
                return Err(format!(
                    "stage-firmware does not take '{}'; it takes --image, --board, \
                     --firmware-dir and --name",
                    other
                ))
            }
        };
        let value = it.next().ok_or_else(|| format!("{} needs a value", arg))?;
        if slot.replace(value.clone()).is_some() {
            return Err(format!("{} is given twice", arg));
        }
    }
    let need =
        |v: Option<String>, flag: &str| v.ok_or_else(|| format!("stage-firmware needs {}", flag));
    let image = need(image, "--image <file.bin>")?;
    let board = need(board, "--board <profile>")?;
    let dir = need(dir, "--firmware-dir <dir>")?;
    if !is_wire_text(&board) {
        return Err(format!(
            "the board '{}' is not 1 to {} bytes of text",
            board, FIRMWARE_MAX_TEXT
        ));
    }
    let staged = stage(Path::new(&image), Path::new(&dir), name.as_deref(), &board)?;
    Ok(format!(
        "staged name={} version=\"{}\" board=\"{}\" size={} sha256={} dir={}",
        staged.name, staged.version, staged.board, staged.size, staged.sha256, dir
    ))
}

/// The name an image is staged under when none is given: the board and the
/// version, lower-cased, anything that is not a letter or digit a hyphen,
/// cut to the identifier bound.
pub fn default_name(board: &str, version: &str) -> String {
    let mut name: String = format!("{}-{}", board, version)
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    name.truncate(32);
    name
}

/// Whether the owner said they are at the bench: the section 0.7 variable
/// reads exactly `1` in this process's environment. The one place the server
/// reads it, in the approved Rust form.
pub fn owner_at_bench() -> bool {
    matches!(std::env::var("CHORUS_OWNER_AT_BENCH").as_deref(), Ok("1"))
}

/// The section 0.7 decision: may a transfer start to a peer at `peer`?
/// Always to this host itself (loopback: the tests, the emulator run);
/// anywhere else only with the owner at the bench.
pub fn transfer_allowed(peer: IpAddr, at_bench: bool) -> Result<(), &'static str> {
    // An IPv4 peer on a dual-stack listener arrives as ::ffff:a.b.c.d.
    if peer.to_canonical().is_loopback() || at_bench {
        Ok(())
    } else {
        Err("owner-not-at-bench")
    }
}

/// One install this server is carrying.
struct Transfer {
    /// The router's id of the session it travels in.
    session: u64,
    /// The speaker.
    endpoint: String,
    /// The offer's transfer id.
    id: u32,
    /// The staged image's name and what the offer says of it.
    image: String,
    version: String,
    board: String,
    /// The image, verified again when the install was commanded.
    bytes: Arc<Vec<u8>>,
    sha256: [u8; 32],
    /// Whether the offer is on the session's queue.
    offered: bool,
    /// Whether the speaker answered the offer with `receiving`.
    receiving: bool,
    /// Bytes queued so far, and bytes the speaker has acknowledged.
    sent: usize,
    acked: usize,
}

#[derive(Default)]
struct Inner {
    next_transfer: u32,
    /// Sessions whose endpoint sent a `firmware_status`: (session, endpoint).
    reported: Vec<(u64, String)>,
    transfers: Vec<Transfer>,
    /// Sessions owed a cancel that their queue had no room for.
    cancels: Vec<u64>,
}

/// The server's firmware state: where images are staged and the installs in
/// progress. Shared by the control plane (commands) and the sessions.
pub struct Firmware {
    dir: Option<PathBuf>,
    router: Arc<Router>,
    log: Box<dyn Fn(&str) + Send + Sync>,
    inner: Mutex<Inner>,
    /// Transfers and owed cancels in hand, so a session with nothing to send
    /// takes no lock when it looks.
    work: AtomicUsize,
    /// `firmware_offer`s (not cancels) put on a session since the start.
    offers: AtomicU64,
}

impl std::fmt::Debug for Firmware {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Firmware")
            .field("dir", &self.dir)
            .field("offers", &self.offers())
            .finish_non_exhaustive()
    }
}

fn lock(m: &Mutex<Inner>) -> MutexGuard<'_, Inner> {
    match m.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

impl Firmware {
    /// `dir` is `--firmware-dir`, or `None` (speakers still report what they
    /// run; there is nothing to install). `seed` makes the transfer ids of
    /// this process unlike the last one's, so a speaker still holding part of
    /// a transfer from before a restart is never mistaken for resuming one of
    /// this process's.
    pub fn new(
        dir: Option<PathBuf>,
        router: Arc<Router>,
        seed: u32,
        log: Box<dyn Fn(&str) + Send + Sync>,
    ) -> Firmware {
        Firmware {
            dir,
            router,
            log,
            inner: Mutex::new(Inner {
                next_transfer: seed.max(1),
                ..Inner::default()
            }),
            work: AtomicUsize::new(0),
            offers: AtomicU64::new(0),
        }
    }

    /// The firmware directory, if this server has one.
    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    /// How many `firmware_offer`s this process has sent. Zero until somebody
    /// commands an install.
    pub fn offers(&self) -> u64 {
        self.offers.load(Ordering::Relaxed)
    }

    /// Scan the firmware directory and log what was found. `None` when this
    /// server has no firmware directory.
    pub fn scan(&self) -> Option<Vec<Image>> {
        let dir = self.dir.as_deref()?;
        let (images, lines) = scan(dir);
        for line in lines {
            (self.log)(&line);
        }
        Some(images)
    }

    fn settle(&self, inner: &Inner) {
        self.work.store(
            inner.transfers.len() + inner.cancels.len(),
            Ordering::SeqCst,
        );
    }

    /// Start the install of `image` on every speaker in `targets`: THE one
    /// place a transfer begins, reached only from a `firmware_install`
    /// command. Everything is checked for every target before anything is
    /// queued for any, so a refusal starts nothing. Returns each speaker's
    /// transfer id. `field` is the command's field a refusal names.
    pub fn start(
        &self,
        targets: &[String],
        image: &Image,
        field: &str,
    ) -> Result<Vec<(String, u32)>, Refusal> {
        self.start_with(targets, image, field, owner_at_bench())
    }

    /// [`Firmware::start`] with the bench decision passed in.
    pub fn start_with(
        &self,
        targets: &[String],
        image: &Image,
        field: &str,
        at_bench: bool,
    ) -> Result<Vec<(String, u32)>, Refusal> {
        let refuse = |name: &str, detail: String| {
            Refusal::rejected(field, format!("{}: {}. Nothing was offered", name, detail))
        };
        let sessions = self.router.sessions();
        let mut inner = lock(&self.inner);
        // 1. Every target has a session that takes updates, at an address a
        //    transfer may go to.
        let mut chosen = Vec::new();
        for speaker in targets {
            let session = inner
                .reported
                .iter()
                .filter(|(_, endpoint)| endpoint == speaker)
                .filter_map(|(id, _)| sessions.iter().find(|s| s.id == *id))
                .filter(|s| s.features & features::OTA != 0)
                .max_by_key(|s| s.id);
            let Some((session, Some(peer))) = session.map(|s| (s.id, s.peer)) else {
                return Err(refuse(
                    "speaker-absent",
                    format!("speaker '{}' has no session that takes updates", speaker),
                ));
            };
            if let Err(name) = transfer_allowed(peer, at_bench) {
                return Err(refuse(
                    name,
                    format!(
                        "speaker '{}' is at {}, which is not this host; a transfer to a real \
                         device is the owner's action at the bench (docs/firmware-updates.md)",
                        speaker, peer
                    ),
                ));
            }
            if inner.transfers.iter().any(|t| t.endpoint == *speaker) {
                return Err(refuse(
                    "busy",
                    format!("speaker '{}' has a transfer in progress", speaker),
                ));
            }
            chosen.push((speaker.clone(), session));
        }
        // 2. The image is, now, the file that was verified.
        let not_verified = |why: String| {
            Refusal::rejected(
                "image",
                format!(
                    "image-not-verified: image '{}' {}; run firmware_rescan. Nothing was offered",
                    image.name, why
                ),
            )
        };
        let (Some(dir), Some(sha256)) = (self.dir.as_deref(), parse_digest(&image.sha256)) else {
            return Err(not_verified(
                "is not a staged image of this server".to_string(),
            ));
        };
        let bytes = read_bounded(&dir.join(format!("{}.{}", image.name, IMAGE_EXTENSION)))
            .map_err(|reason| not_verified(format!("can no longer be read ({})", reason)))?;
        if bytes.len() as u64 != image.size || FirmwareOffer::digest_of(&bytes) != sha256 {
            return Err(not_verified(
                "changed on disk since it was verified".to_string(),
            ));
        }
        let bytes = Arc::new(bytes);
        // 3. Nothing can refuse from here on.
        let mut started = Vec::new();
        for (speaker, session) in chosen {
            let id = inner.next_transfer;
            inner.next_transfer = inner.next_transfer.checked_add(1).unwrap_or(1);
            inner.transfers.push(Transfer {
                session,
                endpoint: speaker.clone(),
                id,
                image: image.name.clone(),
                version: image.version.clone(),
                board: image.board.clone(),
                bytes: Arc::clone(&bytes),
                sha256,
                offered: false,
                receiving: false,
                sent: 0,
                acked: 0,
            });
            started.push((speaker, id));
            self.advance(&mut inner, session);
        }
        self.settle(&inner);
        Ok(started)
    }

    /// The owner cancelled speaker `speaker`'s install (`firmware_cancel`):
    /// the transfer is dropped and the speaker told to abandon what it
    /// holds. Whether there was one.
    pub fn cancel(&self, speaker: &str) -> bool {
        let mut inner = lock(&self.inner);
        let Some(at) = inner.transfers.iter().position(|t| t.endpoint == speaker) else {
            return false;
        };
        let transfer = inner.transfers.remove(at);
        (self.log)(&format!(
            "firmware cancel speaker={} transfer={} image={} reason=firmware-cancel",
            transfer.endpoint, transfer.id, transfer.image
        ));
        // An offer that never left needs no cancel.
        if transfer.offered {
            inner.cancels.push(transfer.session);
            self.advance(&mut inner, transfer.session);
        }
        self.settle(&inner);
        true
    }

    /// Put on `session`'s queue what it is owed and has room for: a cancel,
    /// the offer, then chunks up to the window. Never blocks.
    fn advance(&self, inner: &mut Inner, session: u64) {
        if let Some(at) = inner.cancels.iter().position(|s| *s == session) {
            if !self
                .router
                .push_message(session, &Message::FirmwareOffer(FirmwareOffer::cancel()))
            {
                return;
            }
            inner.cancels.remove(at);
        }
        let Some(t) = inner.transfers.iter_mut().find(|t| t.session == session) else {
            return;
        };
        if !t.offered {
            let offer = FirmwareOffer {
                transfer: t.id,
                size: t.bytes.len() as u32,
                sha256: t.sha256,
                chunk_bytes: CHUNK_BYTES,
                version: t.version.clone(),
                board: t.board.clone(),
            };
            if !self
                .router
                .push_message(session, &Message::FirmwareOffer(offer))
            {
                return;
            }
            t.offered = true;
            self.offers.fetch_add(1, Ordering::Relaxed);
            (self.log)(&format!(
                "firmware offer speaker={} transfer={} image={} version=\"{}\" board=\"{}\" \
                 size={} chunk_bytes={}",
                t.endpoint,
                t.id,
                t.image,
                t.version,
                t.board,
                t.bytes.len(),
                CHUNK_BYTES
            ));
        }
        if !t.receiving {
            return;
        }
        let window = (WINDOW_CHUNKS * u32::from(CHUNK_BYTES)) as usize;
        while t.sent < t.bytes.len() && t.sent - t.acked < window {
            let end = (t.sent + usize::from(CHUNK_BYTES)).min(t.bytes.len());
            let chunk = FirmwareChunk {
                transfer: t.id,
                offset: t.sent as u32,
                data: t.bytes[t.sent..end].to_vec(),
            };
            if !self
                .router
                .push_message(session, &Message::FirmwareChunk(chunk))
            {
                // The queue is full of this session's own audio: next look.
                return;
            }
            t.sent = end;
        }
    }

    /// The session's reader looks at its transfer: called every time it
    /// looks up from its socket. A session with nothing owed takes no lock.
    pub fn pump(&self, session: u64) {
        if self.work.load(Ordering::SeqCst) == 0 {
            return;
        }
        let mut inner = lock(&self.inner);
        self.advance(&mut inner, session);
        self.settle(&inner);
    }

    /// A `firmware_status` arrived on `session` from `endpoint`: move its
    /// transfer on, and say what the control plane is to be told.
    pub fn status(&self, session: u64, endpoint: &str, status: &FirmwareStatus) -> Report {
        let mut inner = lock(&self.inner);
        if !inner.reported.iter().any(|(s, _)| *s == session) {
            inner.reported.push((session, endpoint.to_string()));
        }
        let at = inner
            .transfers
            .iter()
            .position(|t| t.session == session && t.id == status.transfer && t.offered);
        let carried = at.is_some();
        match (at, status.state) {
            (Some(at), FirmwareState::Receiving) => {
                let t = &mut inner.transfers[at];
                let first = !t.receiving;
                t.receiving = true;
                t.acked = (status.received as usize).min(t.bytes.len());
                // A gap (bad_offset) or a resume point behind what was
                // queued: go back to what the speaker has. Never behind it.
                if status.reason == FirmwareReason::BadOffset || t.sent < t.acked {
                    t.sent = t.acked;
                }
                if first {
                    (self.log)(&format!(
                        "firmware receiving speaker={} transfer={} from={}",
                        endpoint, status.transfer, status.received
                    ));
                }
            }
            (Some(at), _) => {
                // Verified, refused, or anything else that ends it.
                let t = inner.transfers.remove(at);
                (self.log)(&format!(
                    "firmware {} speaker={} transfer={} image={} received={} reason={}",
                    status.state.name(),
                    endpoint,
                    t.id,
                    t.image,
                    status.received,
                    status.reason.name()
                ));
            }
            (None, FirmwareState::Receiving) => {
                // A transfer nobody here is sending: this server restarted,
                // or the session it travelled in ended. It is NOT resumed:
                // the speaker is told to abandon it, and an install needs a
                // new firmware_install.
                (self.log)(&format!(
                    "firmware cancel speaker={} transfer={} received={} reason=not-resumed",
                    endpoint, status.transfer, status.received
                ));
                if !inner.cancels.contains(&session) {
                    inner.cancels.push(session);
                }
            }
            (None, state) => {
                if state != FirmwareState::Idle || status.transfer != 0 {
                    (self.log)(&format!(
                        "firmware {} speaker={} transfer={} version=\"{}\" slot={} reason={} \
                         image_version=\"{}\"",
                        state.name(),
                        endpoint,
                        status.transfer,
                        status.version,
                        status.slot,
                        status.reason.name(),
                        status.image_version
                    ));
                }
            }
        }
        self.advance(&mut inner, session);
        self.settle(&inner);
        Report {
            state: status.state.name().to_string(),
            reason: status.reason.name().to_string(),
            transfer: status.transfer,
            received: u64::from(status.received),
            version: status.version.clone(),
            board: status.board.clone(),
            slot: (status.slot != FIRMWARE_SLOT_UNKNOWN).then_some(status.slot),
            image_version: status.image_version.clone(),
            carried,
        }
    }

    /// `session` has ended. Its transfer, if it had one, ends with it and is
    /// not resumed when the speaker comes back; the speaker's id is returned
    /// so the control plane can say `interrupted`.
    pub fn session_down(&self, session: u64) -> Option<String> {
        let mut inner = lock(&self.inner);
        inner.reported.retain(|(s, _)| *s != session);
        inner.cancels.retain(|s| *s != session);
        let ended = inner
            .transfers
            .iter()
            .position(|t| t.session == session)
            .map(|at| inner.transfers.remove(at));
        self.settle(&inner);
        ended.map(|t| {
            (self.log)(&format!(
                "firmware interrupted speaker={} transfer={} image={} sent={} acked={} \
                 reason=session-ended",
                t.endpoint, t.id, t.image, t.sent, t.acked
            ));
            t.endpoint
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};
    use std::sync::mpsc;

    use crate::router::SessionStart;
    use crate::stream::{Fanout, Outbound, SUBSCRIBER_QUEUE_LIMIT};
    use chorus_protocol::v2::{decode_frame, Outcome};

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("chorus-firmware-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// An image the checks accept: the ESP magic, a first segment starting
    /// with an application description that names `version`, then filler.
    fn esp_image(version: &str, bytes: usize) -> Vec<u8> {
        let mut image = vec![0u8; bytes.max(APP_VERSION_OFFSET + APP_VERSION_LEN)];
        image[0] = ESP_IMAGE_MAGIC;
        image[APP_DESCRIPTION_OFFSET..APP_DESCRIPTION_OFFSET + 4]
            .copy_from_slice(&APP_DESCRIPTION_MAGIC.to_le_bytes());
        image[APP_VERSION_OFFSET..APP_VERSION_OFFSET + version.len()]
            .copy_from_slice(version.as_bytes());
        for (i, b) in image
            .iter_mut()
            .enumerate()
            .skip(APP_VERSION_OFFSET + APP_VERSION_LEN)
        {
            *b = (i * 7 + 3) as u8;
        }
        image
    }

    fn put(dir: &Path, name: &str, image: &[u8], manifest: &str) {
        fs::write(dir.join(format!("{}.bin", name)), image).unwrap();
        fs::write(dir.join(format!("{}.manifest", name)), manifest).unwrap();
    }

    fn manifest_of(image: &[u8], version: &str, board: &str) -> String {
        Manifest {
            version: version.to_string(),
            board: board.to_string(),
            size: image.len() as u64,
            sha256: digest_hex(image),
        }
        .render()
    }

    #[test]
    fn a_manifest_holds_exactly_four_keys_once_each() {
        let good = "# a comment\nversion = 2.0.0\nboard = brick-s3-wired\nsize = 12\nsha256 = \
                    00000000000000000000000000000000000000000000000000000000000000ff\n";
        let m = Manifest::parse(good).unwrap();
        assert_eq!(
            (m.version.as_str(), m.board.as_str(), m.size),
            ("2.0.0", "brick-s3-wired", 12)
        );
        assert_eq!(
            Manifest::parse(&m.render()).unwrap(),
            m,
            "what is written reads back"
        );
        for (bad, why) in [
            (good.replace("size = 12\n", ""), "missing"),
            (format!("{}size = 13\n", good), "twice"),
            (format!("{}colour = red\n", good), "does not hold"),
            (good.replace("size = 12", "size = twelve"), "whole number"),
            (good.replace("00ff", "00FF"), "lower-case hex"),
            (good.replace("00ff", "ff"), "lower-case hex"),
            (good.replace("version = 2.0.0", "version ="), "version"),
            (
                good.replace(
                    "board = brick-s3-wired",
                    &format!("board = {}", "b".repeat(48)),
                ),
                "board",
            ),
            ("just words\n".to_string(), "key = value"),
        ] {
            let e = Manifest::parse(&bad).unwrap_err();
            assert!(e.contains(why), "{:?} was refused as {:?}", bad, e);
        }
    }

    #[test]
    fn a_staged_image_is_verified_or_refused_with_its_reason_by_name() {
        let dir = scratch("scan");
        let good = esp_image("2.0.0", 5000);
        put(
            &dir,
            "good",
            &good,
            &manifest_of(&good, "2.0.0", "brick-s3-wired"),
        );
        // One byte differs from what the manifest's digest was taken of.
        let mut corrupt = good.clone();
        corrupt[4000] ^= 1;
        put(
            &dir,
            "bad-digest",
            &corrupt,
            &manifest_of(&good, "2.0.0", "brick-s3-wired"),
        );
        put(
            &dir,
            "short",
            &good[..4999],
            &manifest_of(&good, "2.0.0", "brick-s3-wired"),
        );
        let mut not_esp = good.clone();
        not_esp[0] = 0x7f;
        put(
            &dir,
            "not-esp",
            &not_esp,
            &manifest_of(&not_esp, "2.0.0", "brick-s3-wired"),
        );
        let mut no_description = good.clone();
        no_description[APP_DESCRIPTION_OFFSET] ^= 0xff;
        put(
            &dir,
            "no-description",
            &no_description,
            &manifest_of(&no_description, "2.0.0", "brick-s3-wired"),
        );
        put(
            &dir,
            "other-version",
            &good,
            &manifest_of(&good, "9.9.9", "brick-s3-wired"),
        );
        put(&dir, "bad-manifest", &good, "version = 2.0.0\n");
        put(
            &dir,
            "Not_An_Identifier",
            &good,
            &manifest_of(&good, "2.0.0", "brick-s3-wired"),
        );
        fs::write(dir.join("no-manifest.bin"), &good).unwrap();
        fs::write(
            dir.join("no-image.manifest"),
            manifest_of(&good, "2.0.0", "brick-s3-wired"),
        )
        .unwrap();
        fs::write(dir.join("notes.txt"), "not an image").unwrap();

        let (images, lines) = scan(&dir);
        let verdicts: Vec<(&str, Option<&str>)> = images
            .iter()
            .map(|i| (i.name.as_str(), i.refused.as_deref()))
            .collect();
        assert_eq!(
            verdicts,
            vec![
                ("Not_An_Identifier", Some("name-not-an-identifier")),
                ("bad-digest", Some("digest-mismatch")),
                ("bad-manifest", Some("bad-manifest")),
                ("good", None),
                ("no-description", Some("no-app-description")),
                ("no-image", Some("no-image-file")),
                ("no-manifest", Some("no-manifest")),
                ("not-esp", Some("not-an-esp-image")),
                ("other-version", Some("version-mismatch")),
                ("short", Some("size-mismatch")),
            ],
            "sorted by name, each with its verdict"
        );
        let good_listed = images.iter().find(|i| i.name == "good").unwrap();
        assert_eq!(good_listed.sha256, digest_hex(&good));
        assert_eq!(
            (good_listed.size, good_listed.version.as_str()),
            (5000, "2.0.0")
        );
        assert!(lines
            .last()
            .unwrap()
            .contains("images=10 verified=1 refused=9"));
        assert!(lines
            .iter()
            .any(|l| l.contains("name=bad-digest") && l.contains("reason=digest-mismatch")));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_image_past_the_wire_bound_or_empty_is_refused_and_the_list_is_bounded() {
        let dir = scratch("bounds");
        let image = esp_image("1", 200);
        let mut huge = manifest_of(&image, "1", "b");
        huge = huge.replace(
            "size = 200",
            &format!("size = {}", u64::from(FIRMWARE_MAX_SIZE) + 1),
        );
        put(&dir, "huge", &image, &huge);
        put(
            &dir,
            "empty",
            &[],
            &manifest_of(&image, "1", "b").replace("size = 200", "size = 0"),
        );
        let (images, _) = scan(&dir);
        assert_eq!(images[0].refused.as_deref(), Some("empty"));
        assert_eq!(images[1].refused.as_deref(), Some("too-large"));
        for n in 0..MAX_IMAGES + 2 {
            put(
                &dir,
                &format!("image-{:02}", n),
                &image,
                &manifest_of(&image, "1", "b"),
            );
        }
        let (images, lines) = scan(&dir);
        assert_eq!(images.len(), MAX_IMAGES);
        assert_eq!(
            lines
                .iter()
                .filter(|l| l.contains("reason=too-many"))
                .count(),
            4
        );
        let (none, lines) = scan(&dir.join("not-there"));
        assert!(none.is_empty() && lines[0].contains("unreadable"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_staging_helper_writes_a_manifest_the_server_verifies() {
        let dir = scratch("stage");
        let source = dir.join("build").join("chorus-endpoint.bin");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        let image = esp_image("2.1.0-rc1", 3000);
        fs::write(&source, &image).unwrap();
        let staged = dir.join("staged");
        let listed = stage(&source, &staged, None, "brick-s3-wired").unwrap();
        assert_eq!(listed.name, "brick-s3-wired-2-1-0-rc1");
        assert_eq!(
            (listed.version.as_str(), listed.board.as_str()),
            ("2.1.0-rc1", "brick-s3-wired")
        );
        assert!(listed.verified());
        let (images, _) = scan(&staged);
        assert_eq!(images, vec![listed]);
        assert_eq!(
            fs::read(staged.join("brick-s3-wired-2-1-0-rc1.bin")).unwrap(),
            image
        );
        // Staging again under a given name, and what is not an image.
        assert!(stage(&source, &staged, Some("kitchen-next"), "brick-s3-wired").is_ok());
        assert!(stage(&source, &staged, Some("Not An Id"), "brick-s3-wired")
            .unwrap_err()
            .contains("not an identifier"));
        fs::write(&source, b"not an image").unwrap();
        assert!(stage(&source, &staged, None, "brick-s3-wired")
            .unwrap_err()
            .contains("not-an-esp-image"));
        let _ = fs::remove_dir_all(&dir);
    }

    /// THE guard's decision (program section 0.7), with a peer that is not
    /// loopback: refused by name without the owner at the bench.
    #[test]
    fn a_transfer_to_a_peer_that_is_not_loopback_is_refused_without_the_owner_at_the_bench() {
        // RFC 5737 documentation addresses stand in for a speaker on a LAN.
        let lan = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 17));
        let lan6 = IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 0x17));
        let mapped = IpAddr::V6(Ipv4Addr::new(192, 0, 2, 17).to_ipv6_mapped());
        for peer in [lan, lan6, mapped] {
            assert_eq!(
                transfer_allowed(peer, false),
                Err("owner-not-at-bench"),
                "{}",
                peer
            );
            assert_eq!(transfer_allowed(peer, true), Ok(()), "{}", peer);
        }
        let loopback = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let loopback6 = IpAddr::V6(Ipv6Addr::LOCALHOST);
        let mapped_loopback = IpAddr::V6(Ipv4Addr::LOCALHOST.to_ipv6_mapped());
        for peer in [loopback, loopback6, mapped_loopback] {
            assert_eq!(transfer_allowed(peer, false), Ok(()), "{}", peer);
        }
        // Nothing in this repository sets the variable, the test run
        // included: the server's own read says the owner is not at the bench.
        assert!(!owner_at_bench());
    }

    struct Rig {
        dir: PathBuf,
        router: Arc<Router>,
        firmware: Firmware,
        log: Arc<Mutex<Vec<String>>>,
    }

    fn rig(name: &str) -> Rig {
        let dir = scratch(name);
        let router = Arc::new(Router::single(Arc::new(Fanout::new())));
        let log = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&log);
        let firmware = Firmware::new(
            Some(dir.clone()),
            Arc::clone(&router),
            41,
            Box::new(move |line| sink.lock().unwrap().push(line.to_string())),
        );
        Rig {
            dir,
            router,
            firmware,
            log,
        }
    }

    impl Rig {
        /// A session of `endpoint` at `peer` that takes updates, with a queue
        /// of `capacity`.
        fn session(
            &self,
            endpoint: &str,
            peer: IpAddr,
            capacity: usize,
        ) -> (u64, mpsc::Receiver<Outbound>) {
            let (out, inbox) = mpsc::sync_channel(capacity);
            let id = self
                .router
                .register(endpoint, 0, out, &SessionStart::default());
            self.router.set_link(id, features::OTA, peer);
            (id, inbox)
        }

        fn staged(&self, name: &str, version: &str, bytes: usize) -> (Image, Vec<u8>) {
            let image = esp_image(version, bytes);
            put(
                &self.dir,
                name,
                &image,
                &manifest_of(&image, version, "brick-s3-wired"),
            );
            let listed = self
                .firmware
                .scan()
                .unwrap()
                .into_iter()
                .find(|i| i.name == name)
                .unwrap();
            assert!(listed.verified());
            (listed, image)
        }

        fn said(&self, what: &str) -> usize {
            self.log
                .lock()
                .unwrap()
                .iter()
                .filter(|l| l.contains(what))
                .count()
        }
    }

    fn idle(version: &str) -> FirmwareStatus {
        FirmwareStatus {
            transfer: 0,
            state: FirmwareState::Idle,
            reason: FirmwareReason::None,
            received: 0,
            version: version.to_string(),
            board: "brick-s3-wired".to_string(),
            slot: 0,
            image_version: String::new(),
        }
    }

    fn receiving(transfer: u32, received: u32) -> FirmwareStatus {
        FirmwareStatus {
            transfer,
            state: FirmwareState::Receiving,
            received,
            image_version: "2.0.0".to_string(),
            ..idle("1.0.0")
        }
    }

    fn messages(inbox: &mpsc::Receiver<Outbound>) -> Vec<Message> {
        inbox
            .try_iter()
            .filter_map(|o| match o {
                Outbound::Frame(bytes) => match decode_frame(&bytes).outcome {
                    Outcome::Decoded(m) => Some(m),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }

    const LOOPBACK: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

    #[test]
    fn nothing_is_queued_for_a_session_until_start_and_then_one_offer() {
        let rig = rig("offer");
        let (image, _) = rig.staged("good", "2.0.0", 40_000);
        let (session, inbox) = rig.session("den", LOOPBACK, SUBSCRIBER_QUEUE_LIMIT);
        // The session comes up, reports, and is looked at many times.
        let report = rig.firmware.status(session, "den", &idle("1.0.0"));
        assert_eq!(
            (report.state.as_str(), report.version.as_str(), report.slot),
            ("idle", "1.0.0", Some(0))
        );
        assert!(!report.carried);
        for _ in 0..10 {
            rig.firmware.pump(session);
        }
        assert!(
            messages(&inbox).is_empty(),
            "a staged image and a session are not an install"
        );
        assert_eq!(rig.firmware.offers(), 0);

        let started = rig
            .firmware
            .start(&["den".to_string()], &image, "speaker")
            .unwrap();
        assert_eq!(started, vec![("den".to_string(), 41)]);
        match &messages(&inbox)[..] {
            [Message::FirmwareOffer(offer)] => {
                assert_eq!(
                    (offer.transfer, offer.size, offer.chunk_bytes),
                    (41, 40_000, CHUNK_BYTES)
                );
                assert_eq!(
                    (offer.version.as_str(), offer.board.as_str()),
                    ("2.0.0", "brick-s3-wired")
                );
                assert_eq!(hex(&offer.sha256), image.sha256);
            }
            other => panic!("{:?}", other),
        }
        assert_eq!(rig.firmware.offers(), 1);
        // No chunk until the speaker says it is receiving.
        rig.firmware.pump(session);
        assert!(messages(&inbox).is_empty());
        let _ = fs::remove_dir_all(&rig.dir);
    }

    #[test]
    fn chunks_stay_inside_the_window_and_follow_the_acknowledgements_to_the_end() {
        let rig = rig("window");
        let (image, bytes) = rig.staged("good", "2.0.0", 40_000);
        let (session, inbox) = rig.session("den", LOOPBACK, SUBSCRIBER_QUEUE_LIMIT);
        rig.firmware.status(session, "den", &idle("1.0.0"));
        rig.firmware
            .start(&["den".to_string()], &image, "speaker")
            .unwrap();
        messages(&inbox);
        let window = (WINDOW_CHUNKS * u32::from(CHUNK_BYTES)) as usize;
        let mut got = Vec::new();
        let mut acked = 0usize;
        let report = rig.firmware.status(session, "den", &receiving(41, 0));
        assert!(report.carried);
        loop {
            // However often the reader looks, the window holds.
            for _ in 0..3 {
                rig.firmware.pump(session);
            }
            let batch = messages(&inbox);
            for m in &batch {
                match m {
                    Message::FirmwareChunk(c) => {
                        assert_eq!(c.transfer, 41);
                        assert_eq!(c.offset as usize, got.len(), "in order, no gap");
                        assert!(c.data.len() <= usize::from(CHUNK_BYTES));
                        got.extend_from_slice(&c.data);
                    }
                    other => panic!("{:?}", other),
                }
            }
            assert!(
                got.len() - acked <= window,
                "never more than the window unacknowledged"
            );
            if got.len() == bytes.len() {
                break;
            }
            assert!(!batch.is_empty(), "an acknowledgement opens the window");
            acked = got.len();
            rig.firmware
                .status(session, "den", &receiving(41, acked as u32));
        }
        assert_eq!(got, bytes, "the image, byte for byte");
        let verified = FirmwareStatus {
            state: FirmwareState::Verified,
            received: bytes.len() as u32,
            ..receiving(41, 0)
        };
        rig.firmware.status(session, "den", &verified);
        assert_eq!(rig.said("firmware verified speaker=den transfer=41"), 1);
        rig.firmware.pump(session);
        assert!(messages(&inbox).is_empty(), "nothing after the end");
        assert_eq!(
            rig.firmware.session_down(session),
            None,
            "a finished transfer is not interrupted"
        );
        let _ = fs::remove_dir_all(&rig.dir);
    }

    #[test]
    fn a_full_queue_is_never_waited_on_and_the_transfer_goes_on_when_it_drains() {
        let rig = rig("full");
        let (image, bytes) = rig.staged("good", "2.0.0", 9_000);
        // A queue with room for three items: the session's audio is in the
        // way of the firmware, never the other way round.
        let (session, inbox) = rig.session("den", LOOPBACK, 3);
        rig.firmware.status(session, "den", &idle("1.0.0"));
        rig.firmware
            .start(&["den".to_string()], &image, "speaker")
            .unwrap();
        rig.firmware.status(session, "den", &receiving(41, 0));
        let mut got = Vec::new();
        let mut looks = 0;
        while got.len() < bytes.len() {
            rig.firmware.pump(session);
            for m in messages(&inbox) {
                if let Message::FirmwareChunk(c) = m {
                    assert_eq!(c.offset as usize, got.len());
                    got.extend_from_slice(&c.data);
                }
            }
            looks += 1;
            assert!(looks < 100, "the transfer stalled");
        }
        assert_eq!(got, bytes);
        let _ = fs::remove_dir_all(&rig.dir);
    }

    #[test]
    fn a_gap_goes_back_to_what_the_speaker_has() {
        let rig = rig("gap");
        let (image, bytes) = rig.staged("good", "2.0.0", 30_000);
        let (session, inbox) = rig.session("den", LOOPBACK, SUBSCRIBER_QUEUE_LIMIT);
        rig.firmware.status(session, "den", &idle("1.0.0"));
        rig.firmware
            .start(&["den".to_string()], &image, "speaker")
            .unwrap();
        rig.firmware.status(session, "den", &receiving(41, 0));
        messages(&inbox);
        let gap = FirmwareStatus {
            reason: FirmwareReason::BadOffset,
            ..receiving(41, 2048)
        };
        rig.firmware.status(session, "den", &gap);
        match messages(&inbox).first() {
            Some(Message::FirmwareChunk(c)) => {
                assert_eq!(c.offset, 2048);
                assert_eq!(c.data, bytes[2048..3072]);
            }
            other => panic!("{:?}", other),
        }
        let _ = fs::remove_dir_all(&rig.dir);
    }

    #[test]
    fn start_refuses_by_name_and_then_nothing_was_offered() {
        let rig = rig("refusals");
        let (image, _) = rig.staged("good", "2.0.0", 9_000);
        // RFC 5737: a speaker somewhere that is not this host.
        let lan = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 17));
        let (near, near_inbox) = rig.session("near", LOOPBACK, SUBSCRIBER_QUEUE_LIMIT);
        let (far, far_inbox) = rig.session("far", lan, SUBSCRIBER_QUEUE_LIMIT);
        rig.firmware.status(near, "near", &idle("1.0.0"));
        rig.firmware.status(far, "far", &idle("1.0.0"));

        // The guard: both targets are refused when one of them is not on
        // loopback, and the one that is gets no offer either.
        let both = ["near".to_string(), "far".to_string()];
        let refusal = rig
            .firmware
            .start_with(&both, &image, "all", false)
            .unwrap_err();
        assert_eq!(refusal.field, "all");
        assert!(
            refusal
                .detail
                .starts_with("owner-not-at-bench: speaker 'far'"),
            "{}",
            refusal.detail
        );
        assert!(messages(&near_inbox).is_empty() && messages(&far_inbox).is_empty());
        // A session nobody reported on, or none at all.
        let refusal = rig
            .firmware
            .start_with(&["gone".to_string()], &image, "speaker", false)
            .unwrap_err();
        assert!(
            refusal.detail.starts_with("speaker-absent:"),
            "{}",
            refusal.detail
        );
        // The file changed after it was verified.
        let path = rig.dir.join("good.bin");
        let mut changed = fs::read(&path).unwrap();
        changed[5000] ^= 1;
        fs::write(&path, &changed).unwrap();
        let refusal = rig
            .firmware
            .start_with(&["near".to_string()], &image, "speaker", false)
            .unwrap_err();
        assert_eq!(refusal.field, "image");
        assert!(
            refusal.detail.starts_with("image-not-verified:"),
            "{}",
            refusal.detail
        );
        fs::remove_file(&path).unwrap();
        let refusal = rig
            .firmware
            .start_with(&["near".to_string()], &image, "speaker", false)
            .unwrap_err();
        assert!(
            refusal.detail.contains("no-image-file"),
            "{}",
            refusal.detail
        );
        assert_eq!(rig.firmware.offers(), 0);
        assert_eq!(rig.said("firmware offer"), 0);

        // With the owner at the bench the far speaker is offered it.
        let (image, _) = rig.staged("good", "2.0.0", 9_000);
        let started = rig.firmware.start_with(&both, &image, "all", true).unwrap();
        assert_eq!(started.len(), 2);
        assert!(matches!(
            messages(&far_inbox)[..],
            [Message::FirmwareOffer(_)]
        ));
        // And a second install while one is in progress is busy.
        let refusal = rig
            .firmware
            .start_with(&["far".to_string()], &image, "speaker", true)
            .unwrap_err();
        assert!(refusal.detail.starts_with("busy:"), "{}", refusal.detail);
        let _ = fs::remove_dir_all(&rig.dir);
    }

    #[test]
    fn a_transfer_ends_with_its_session_and_an_orphan_is_cancelled_not_resumed() {
        let rig = rig("orphan");
        let (image, _) = rig.staged("good", "2.0.0", 30_000);
        let (session, inbox) = rig.session("den", LOOPBACK, SUBSCRIBER_QUEUE_LIMIT);
        rig.firmware.status(session, "den", &idle("1.0.0"));
        rig.firmware
            .start(&["den".to_string()], &image, "speaker")
            .unwrap();
        rig.firmware.status(session, "den", &receiving(41, 0));
        messages(&inbox);
        rig.router.unregister(session);
        assert_eq!(rig.firmware.session_down(session), Some("den".to_string()));
        assert_eq!(rig.said("firmware interrupted speaker=den transfer=41"), 1);

        // The speaker rejoins, still holding part of transfer 41.
        let (again, inbox) = rig.session("den", LOOPBACK, SUBSCRIBER_QUEUE_LIMIT);
        let report = rig.firmware.status(again, "den", &receiving(41, 16_384));
        assert!(!report.carried, "this server is not carrying it any more");
        match &messages(&inbox)[..] {
            [Message::FirmwareOffer(cancel)] => assert!(cancel.is_cancel()),
            other => panic!("expected the cancel and nothing else, got {:?}", other),
        }
        for _ in 0..5 {
            rig.firmware.pump(again);
        }
        assert!(
            messages(&inbox).is_empty(),
            "no chunk, no offer: not resumed"
        );
        assert_eq!(
            rig.firmware.offers(),
            1,
            "the one the command started, and no other"
        );
        assert_eq!(rig.said("reason=not-resumed"), 1);
        let _ = fs::remove_dir_all(&rig.dir);
    }

    #[test]
    fn a_cancel_drops_the_transfer_and_tells_the_speaker() {
        let rig = rig("cancel");
        let (image, _) = rig.staged("good", "2.0.0", 30_000);
        let (session, inbox) = rig.session("den", LOOPBACK, SUBSCRIBER_QUEUE_LIMIT);
        rig.firmware.status(session, "den", &idle("1.0.0"));
        assert!(!rig.firmware.cancel("den"), "nothing to cancel");
        rig.firmware
            .start(&["den".to_string()], &image, "speaker")
            .unwrap();
        rig.firmware.status(session, "den", &receiving(41, 0));
        messages(&inbox);
        assert!(rig.firmware.cancel("den"));
        match &messages(&inbox)[..] {
            [Message::FirmwareOffer(cancel)] => assert!(cancel.is_cancel()),
            other => panic!("{:?}", other),
        }
        rig.firmware.pump(session);
        assert!(messages(&inbox).is_empty());
        // The next install is a new transfer id.
        let started = rig
            .firmware
            .start(&["den".to_string()], &image, "speaker")
            .unwrap();
        assert_eq!(started[0].1, 42);
        let _ = fs::remove_dir_all(&rig.dir);
    }
}
