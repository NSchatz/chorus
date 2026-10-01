//! Protocol v2 on the audio connection: who this server is, who it has
//! adopted, and the session each client connection runs before any audio.
//!
//! # Identity and adoption
//!
//! The server holds one long-term X25519 key, `server.key` in its identity
//! directory (64 hex digits and a newline, mode 0600, created from
//! `/dev/urandom` on first start and never printed), and the endpoints it has
//! adopted, `adopted-endpoints` beside it (the text form of
//! [`PinStore`], rewritten atomically after every adoption). The first
//! handshake under an endpoint id pins that endpoint's key; a later one under
//! the same id with another key is refused with `session_refused`
//! `key_changed` and surfaced by name, and the pin file is left as it was.
//!
//! # Where the handshake runs
//!
//! In the slot's reader thread ([`crate::clients`]), never in the acceptor, so
//! a slow or silent peer holds one slot for at most the handshake timeout and
//! never stalls an accept. The writer half waits for the [`Greeting`] the
//! reader hands it and sends nothing on the connection before then, so no
//! audio frame ever leaves this server outside a `secure_record`.

use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chorus_audio::StreamFormat as PcmFormat;
use chorus_protocol::v2::adoption::{PinStore, Verdict};
use chorus_protocol::v2::negotiate::{default_preference, negotiate, Refusal, Source};
use chorus_protocol::v2::noise::{fingerprint, Keypair};
use chorus_protocol::v2::session::{
    accept, Identity, RecordSealer, SecureReader, SessionError, MAX_RECORD_PLAINTEXT,
};
use chorus_protocol::v2::{
    roles, Capabilities, ChannelPosition, Codec, Hello, Link, Message, OutputDelay, StreamFormat,
    PROTOCOL_VERSION,
};

use crate::control::ControlState;
use crate::controller::ControllerAction;
use crate::linein::LineIns;
use crate::router::{Router, SessionStart};
use crate::stream::Fanout;
use chorus_protocol::{CHUNK_HEADER_LEN, HEADER_LEN};

/// The file holding the server's long-term secret key.
pub const KEY_FILE: &str = "server.key";

/// The file holding the endpoints this server has adopted.
pub const ADOPTED_FILE: &str = "adopted-endpoints";

/// The id this server presents when `--server-id` is not given.
pub const DEFAULT_SERVER_ID: &str = "chorus-server";

/// How long a peer has to finish the handshake and send its `hello` and
/// `capabilities`.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(2);

/// The read timeout the request reader runs with once the session is up: the
/// interval at which it looks up to see whether it is still wanted.
pub const STREAM_READ_TIMEOUT: Duration = Duration::from_millis(200);

/// 32 bytes from the kernel's random source.
pub fn random_32() -> io::Result<[u8; 32]> {
    let mut bytes = [0u8; 32];
    fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn parse_key_hex(text: &str) -> Option<[u8; 32]> {
    let text = text.trim();
    if text.len() != 64 {
        return None;
    }
    let mut key = [0u8; 32];
    for (i, k) in key.iter_mut().enumerate() {
        *k = u8::from_str_radix(text.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(key)
}

/// Read the long-term key at `path`, or create it from `/dev/urandom` with
/// mode 0600 if there is none. The secret is never printed.
pub fn load_or_create_key(path: &Path) -> io::Result<Keypair> {
    match fs::read_to_string(path) {
        Ok(text) => parse_key_hex(&text)
            .map(Keypair::from_secret)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "{} is not a key: it must hold 64 hex digits and a newline",
                        path.display()
                    ),
                )
            }),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let secret = random_32()?;
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(path)?;
            file.write_all(format!("{}\n", hex(&secret)).as_bytes())?;
            file.sync_all()?;
            Ok(Keypair::from_secret(secret))
        }
        Err(e) => Err(e),
    }
}

/// Write `text` to `path` by writing a temporary beside it and renaming it
/// over the old one, so a reader never sees half a file.
pub fn write_atomically(path: &Path, text: &str) -> io::Result<()> {
    let mut temporary = PathBuf::from(path);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "pins".to_string());
    temporary.set_file_name(format!(".{}.writing", name));
    fs::write(&temporary, text)?;
    fs::rename(&temporary, path)
}

/// Where this server's identity lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentitySource {
    /// A directory holding `server.key` and `adopted-endpoints`.
    Directory(PathBuf),
    /// A key made for this process alone and an adoption store held in
    /// memory: for tests and throwaway runs, and asked for by name.
    Ephemeral,
}

/// The endpoints this server has adopted, and the file they persist to.
#[derive(Debug)]
pub struct Adoptions {
    store: Mutex<PinStore>,
    path: Option<PathBuf>,
}

impl Adoptions {
    /// An empty store kept in memory only.
    pub fn in_memory() -> Adoptions {
        Adoptions {
            store: Mutex::new(PinStore::new()),
            path: None,
        }
    }

    /// Load the store at `path`, or start an empty one if there is no file.
    pub fn load(path: &Path) -> Result<Adoptions, String> {
        let store = match fs::read_to_string(path) {
            Ok(text) => {
                PinStore::from_text(&text).map_err(|e| format!("{}: {}", path.display(), e))?
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => PinStore::new(),
            Err(e) => return Err(format!("{}: {}", path.display(), e)),
        };
        Ok(Adoptions {
            store: Mutex::new(store),
            path: Some(path.to_path_buf()),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, PinStore> {
        match self.store.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// The store's verdict on a handshake, pinning a new id and persisting
    /// the pin before the session goes on.
    pub fn check(&self, id: &str, key: &[u8; 32]) -> Result<Verdict, String> {
        let mut store = self.lock();
        let verdict = store.check(id, key);
        if verdict == Verdict::Adopted {
            if let Some(path) = &self.path {
                if let Err(e) = write_atomically(path, &store.to_text()) {
                    // A pin that could not be kept is not an adoption: the
                    // next start would adopt whatever key came first.
                    store.forget(id);
                    return Err(format!(
                        "the adoption of {} could not be written to {}: {}",
                        id,
                        path.display(),
                        e
                    ));
                }
            }
        }
        Ok(verdict)
    }

    /// Every key change refused since this store was loaded.
    pub fn key_changes(&self) -> Vec<chorus_protocol::v2::adoption::KeyChange> {
        self.lock().key_changes().to_vec()
    }
}

/// The explicit channel map for a channel count, or `None` for a count this
/// server has no layout for.
pub fn channel_map(channels: u16) -> Option<Vec<ChannelPosition>> {
    use ChannelPosition::*;
    Some(match channels {
        1 => vec![Mono],
        2 => vec![FrontLeft, FrontRight],
        3 => vec![FrontLeft, FrontRight, FrontCenter],
        4 => vec![FrontLeft, FrontRight, BackLeft, BackRight],
        6 => vec![
            FrontLeft,
            FrontRight,
            FrontCenter,
            LowFrequency,
            BackLeft,
            BackRight,
        ],
        8 => vec![
            FrontLeft,
            FrontRight,
            FrontCenter,
            LowFrequency,
            BackLeft,
            BackRight,
            SideLeft,
            SideRight,
        ],
        _ => return None,
    })
}

/// Why a stream this server was configured with cannot be offered in v2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OfferRefused {
    /// The channel count has no layout.
    NoChannelMap {
        /// The configured channel count.
        channels: u16,
    },
    /// One `audio_chunk` frame is larger than a `secure_record` can carry.
    ChunkTooLarge {
        /// Bytes in one whole `audio_chunk` frame at this configuration.
        frame_len: usize,
        /// The configured chunk duration.
        chunk_us: u64,
    },
}

impl fmt::Display for OfferRefused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OfferRefused::NoChannelMap { channels } => write!(
                f,
                "{} channels has no channel map: protocol v2 announces every stream with one, \
                 and this server has layouts for 1, 2, 3, 4, 6 and 8 channels",
                channels
            ),
            OfferRefused::ChunkTooLarge {
                frame_len,
                chunk_us,
            } => write!(
                f,
                "a {} byte audio chunk does not fit a v2 record ({} bytes); shorten --chunk-us \
                 (now {})",
                frame_len, MAX_RECORD_PLAINTEXT, chunk_us
            ),
        }
    }
}

impl std::error::Error for OfferRefused {}

/// Bytes in one whole `audio_chunk` frame carrying `frames` frames.
pub fn audio_chunk_frame_len(format: &PcmFormat, frames: usize) -> usize {
    HEADER_LEN + CHUNK_HEADER_LEN + frames * format.frame_len()
}

/// What this server offers every endpoint: the configured stream, announced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    /// The stream format announcement.
    pub stream_format: StreamFormat,
    /// The output delay announced to every endpoint.
    pub output_delay_ns: u64,
    /// The software string in this server's `hello`.
    pub software: String,
}

impl Offer {
    /// The offer for the configured stream, refused by name when the channel
    /// count has no map.
    ///
    /// Every `audio_chunk` travels inside one `secure_record`, whose plaintext
    /// is at most [`MAX_RECORD_PLAINTEXT`] bytes of whole frames, so a chunk
    /// duration whose frame is larger is refused here rather than failing on
    /// the first chunk.
    pub fn new(format: &PcmFormat, chunk_us: u64) -> Result<Offer, OfferRefused> {
        let channel_map = channel_map(format.channels).ok_or(OfferRefused::NoChannelMap {
            channels: format.channels,
        })?;
        let frames = format.frames_in(chunk_us).unwrap_or(0);
        let frame_len = audio_chunk_frame_len(format, frames);
        if frame_len > MAX_RECORD_PLAINTEXT {
            return Err(OfferRefused::ChunkTooLarge {
                frame_len,
                chunk_us,
            });
        }
        let frames_per_chunk = frames as u32;
        Ok(Offer {
            stream_format: StreamFormat {
                codec: Codec::Pcm,
                sample_format: format.sample_format,
                sample_rate_hz: format.sample_rate_hz,
                channel_map,
                frames_per_chunk,
                codec_config: Vec::new(),
            },
            output_delay_ns: 0,
            software: format!("chorus-server {}", env!("CARGO_PKG_VERSION")),
        })
    }

    /// Whether an endpoint with these capabilities can play this stream, and
    /// if not, the reason by name and the negotiation's own sentence.
    ///
    /// The choice is `chorus_protocol::v2::negotiate`
    /// (`docs/decisions/0040-codec-negotiation.md`): the wired preference,
    /// since the endpoint's link is not known before its first telemetry, and
    /// PCM as the only codec this server can send today.
    pub fn negotiate(&self, caps: &Capabilities) -> Result<Codec, (&'static str, String)> {
        let f = &self.stream_format;
        let source = Source {
            sample_rate_hz: f.sample_rate_hz,
            channels: f.channel_map.len() as u8,
            sample_format: f.sample_format,
        };
        negotiate(
            &source,
            &default_preference(Link::Wired),
            &[Codec::Pcm],
            caps,
        )
        .map_err(|r| {
            let name = match r {
                Refusal::Rate { .. } => "rate-not-playable",
                Refusal::Channels { .. } => "too-many-channels",
                Refusal::SampleFormat { .. } => "sample-format-not-playable",
                Refusal::NoCommonCodec { .. } => "no-common-codec",
            };
            (name, r.to_string())
        })
    }

    /// What the writer sends first inside the session, in order.
    pub fn greeting(&self) -> Vec<Message> {
        vec![
            Message::Hello(Hello {
                protocol_version: PROTOCOL_VERSION,
                roles: 0,
                name: String::new(),
                software: self.software.clone(),
            }),
            Message::StreamFormat(self.stream_format.clone()),
            Message::OutputDelay(OutputDelay {
                delay_ns: self.output_delay_ns,
            }),
        ]
    }
}

/// Everything a slot needs to run a session: identity, adoptions, the offer
/// and where to say what happened.
pub struct SessionContext {
    /// This server's id and long-term key.
    pub identity: Identity,
    /// The endpoints it has adopted.
    pub adoptions: Adoptions,
    /// The stream it offers.
    pub offer: Offer,
    /// Where status lines go (`key=value` lines, one per event).
    pub log: Box<dyn Fn(&str) + Send + Sync>,
    /// Called once per session that is up and negotiated: the stream may
    /// start for it.
    pub on_session: Box<dyn Fn() + Send + Sync>,
    /// `hello` messages received from endpoints inside sessions.
    pub hellos: AtomicU64,
    /// `telemetry` messages received from endpoints inside sessions.
    pub telemetry: AtomicU64,
    /// The control plane an endpoint's `controller_command` is applied
    /// through, or `None` when this server runs without one (the command is
    /// then refused by name).
    pub control: Option<Arc<ControlState>>,
    /// Which stream each session hears (`crate::router`): every session is
    /// registered with it once it is up, and leaves it when it ends.
    pub router: Arc<Router>,
    /// Where a source-role session's line-in goes (`crate::linein`), or
    /// `None` when this server plays no line-in (no control plane, or the
    /// one-stream shape): its source messages are then ignored, as before.
    pub line_ins: Option<Arc<LineIns>>,
}

impl fmt::Debug for SessionContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionContext")
            .field("id", &self.identity.id)
            .field("key", &fingerprint(&self.identity.keypair.public))
            .finish_non_exhaustive()
    }
}

impl SessionContext {
    /// A context that logs nowhere and signals nothing, for tests.
    pub fn quiet(identity: Identity, offer: Offer) -> SessionContext {
        SessionContext {
            identity,
            adoptions: Adoptions::in_memory(),
            offer,
            log: Box::new(|_| {}),
            on_session: Box::new(|| {}),
            hellos: AtomicU64::new(0),
            telemetry: AtomicU64::new(0),
            control: None,
            router: Arc::new(Router::single(Arc::new(Fanout::new()))),
            line_ins: None,
        }
    }

    /// Where a session that just came up starts, and what its greeting tells
    /// it: from the room model when this server runs a control plane, else
    /// the one stream and nothing more (there is no room to have a volume).
    pub fn start_for(&self, greeting: &Greeting) -> SessionStart {
        let start = match &self.control {
            Some(control) => {
                control.session_start(&greeting.endpoint_id, greeting.roles, self.router.idle())
            }
            None => SessionStart {
                route: self.router.idle(),
                ..SessionStart::default()
            },
        };
        SessionStart {
            visualizer_bands: greeting.visualizer_bands,
            ..start
        }
    }

    fn say(&self, line: &str) {
        (self.log)(line)
    }
}

/// What the reader hands the writer once a session is up.
pub struct Greeting {
    /// Seals everything the writer sends.
    pub sealer: RecordSealer,
    /// The first messages inside the session: the offer's, then (with a
    /// control plane) the room's `room_volume` for a player and its
    /// `controller_state` for a controller, before the first audio
    /// (docs/decisions/0074-*).
    pub messages: Vec<Message>,
    /// The endpoint's authenticated id.
    pub endpoint_id: String,
    /// The roles its `hello` declared.
    pub roles: u16,
    /// The most visualizer bands its `capabilities` asked for (0 to 64,
    /// validated by the decoder); what its `visualizer_frame`s carry.
    pub visualizer_bands: u8,
}

/// The session the reader runs its requests over.
pub type SessionReader<'a> = SecureReader<&'a TcpStream>;

fn refusal_reason(e: &SessionError) -> &'static str {
    match e {
        SessionError::PeerSpeaksV1 { .. } => "protocol-v1",
        SessionError::UnsupportedVersion { .. } => "unsupported-version",
        SessionError::Refused { .. } => "refused-by-peer",
        SessionError::KeyChanged(_) => "key-changed",
        SessionError::Removed { .. } => "removed",
        SessionError::Noise(_) => "handshake-failed",
        SessionError::Protocol(_) => "handshake-failed",
        SessionError::NoV2Answer(_) => "no-answer",
        SessionError::Io(e)
            if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut =>
        {
            "handshake-timeout"
        }
        SessionError::Io(_) => "handshake-io",
    }
}

/// Run the server's side of a session on `source`: the handshake with
/// adoption, then the endpoint's `hello` and `capabilities`, then the
/// negotiation. On success the connection's read timeout is back to
/// [`STREAM_READ_TIMEOUT`] and the caller gets the reader to serve requests
/// over and the greeting for the writer. Every failure has been logged by
/// name and the caller only has to close.
pub fn establish<'a>(
    source: &'a TcpStream,
    peer: SocketAddr,
    ctx: &Arc<SessionContext>,
) -> Option<(SessionReader<'a>, Greeting)> {
    let _ = source.set_read_timeout(Some(HANDSHAKE_TIMEOUT));
    let ephemeral = match random_32() {
        Ok(bytes) => Keypair::from_secret(bytes),
        Err(e) => {
            ctx.say(&format!(
                "client refused peer={} reason=no-randomness detail=\"{}\"",
                peer, e
            ));
            return None;
        }
    };
    let mut adoption_failure = None;
    let mut stream = source;
    let established = accept(&mut stream, &ctx.identity, ephemeral, |id, key| {
        match ctx.adoptions.check(id, key) {
            Ok(v) => v,
            Err(e) => {
                adoption_failure = Some(e);
                Verdict::Removed
            }
        }
    });
    let established = match established {
        Ok(e) => e,
        Err(e) => {
            if let Some(failure) = adoption_failure {
                ctx.say(&format!(
                    "client refused peer={} reason=adoption-not-persisted detail=\"{}\"",
                    peer, failure
                ));
            } else if let SessionError::KeyChanged(change) = &e {
                ctx.say(&format!(
                    "endpoint key changed id={} pinned={} offered={}; refused",
                    change.id, change.pinned, change.offered
                ));
            } else {
                ctx.say(&format!(
                    "client refused peer={} reason={} detail=\"{}\"",
                    peer,
                    refusal_reason(&e),
                    e
                ));
            }
            return None;
        }
    };
    let key = fingerprint(&established.peer_key);
    if established.verdict == Verdict::Adopted {
        ctx.say(&format!(
            "endpoint adopted id={} key={}",
            established.peer_id, key
        ));
    }
    let mut reader = SecureReader::new(source, established.opener);
    let mut hello = None;
    let mut caps = None;
    while hello.is_none() || caps.is_none() {
        match reader.next_message() {
            Ok(Message::Hello(h)) if hello.is_none() => hello = Some(h),
            Ok(Message::Capabilities(c)) if caps.is_none() => caps = Some(c),
            Ok(other) => {
                ctx.say(&format!(
                    "client refused peer={} id={} reason=no-hello-and-capabilities \
                     detail=\"expected hello and capabilities, got {}\"",
                    peer,
                    established.peer_id,
                    other.message_type().name()
                ));
                return None;
            }
            Err(e) => {
                let reason = if e.kind() == io::ErrorKind::WouldBlock
                    || e.kind() == io::ErrorKind::TimedOut
                {
                    "handshake-timeout"
                } else {
                    "handshake-io"
                };
                ctx.say(&format!(
                    "client refused peer={} id={} reason={} detail=\"waiting for hello and \
                     capabilities: {}\"",
                    peer, established.peer_id, reason, e
                ));
                return None;
            }
        }
    }
    let (hello, caps) = match (hello, caps) {
        (Some(h), Some(c)) => (h, c),
        _ => return None,
    };
    ctx.hellos.fetch_add(1, Ordering::Relaxed);
    if let Err((reason, detail)) = ctx.offer.negotiate(&caps) {
        ctx.say(&format!(
            "client refused peer={} id={} reason={} detail=\"{}\"",
            peer, established.peer_id, reason, detail
        ));
        return None;
    }
    let _ = source.set_read_timeout(Some(STREAM_READ_TIMEOUT));
    let f = &ctx.offer.stream_format;
    ctx.say(&format!(
        "client session peer={} id={} key={} verdict={} roles={} software=\"{}\" codec=pcm \
         rate_hz={} channels={} sample_format={} visualizer_bands={}",
        peer,
        established.peer_id,
        key,
        if established.verdict == Verdict::Adopted {
            "adopted"
        } else {
            "known"
        },
        hello.roles,
        hello.software,
        f.sample_rate_hz,
        f.channel_map.len(),
        f.sample_format.name(),
        caps.visualizer_bands
    ));
    // What the endpoint says from now on (its telemetry, a later hello) is
    // counted; none of it reaches the v1 request reader.
    let counting = Arc::clone(ctx);
    reader.set_handler(Box::new(move |m| count(&counting, &m)));
    Some((
        reader,
        Greeting {
            sealer: established.sealer,
            messages: ctx.offer.greeting(),
            endpoint_id: established.peer_id,
            roles: hello.roles,
            visualizer_bands: caps.visualizer_bands,
        },
    ))
}

fn count(ctx: &SessionContext, m: &Message) {
    match m {
        Message::Telemetry(_) => {
            ctx.telemetry.fetch_add(1, Ordering::Relaxed);
        }
        Message::Hello(_) => {
            ctx.hellos.fetch_add(1, Ordering::Relaxed);
        }
        _ => {}
    }
}

/// From now on, route what the endpoint of `greeting` says: counted as
/// [`establish`] counts it, and each `controller_command` applied through
/// the control plane ([`ControlState::controller`], which translates it with
/// `crate::controller::translate` and applies it with the zones' own checks)
/// and answered with a `controller_state` on the endpoint's own outbound
/// queue, through the router so the conductor's pushes are deduped against
/// it (`session` is the router's id for this session; docs/protocol.md "The
/// four roles": only to a peer that declared the controller role). A command
/// from a peer that did not declare the role, or with no control plane to
/// apply it, is refused by name in the log and changes nothing.
pub fn route_controller(
    reader: &mut SessionReader<'_>,
    ctx: &Arc<SessionContext>,
    greeting: &Greeting,
    session: u64,
) {
    let ctx = Arc::clone(ctx);
    let endpoint = greeting.endpoint_id.clone();
    let is_controller = greeting.roles & roles::CONTROLLER != 0;
    let is_source = greeting.roles & roles::SOURCE != 0;
    reader.set_handler(Box::new(move |m| {
        count(&ctx, &m);
        // A source's offer and its started input's format (ADR 0066) go to
        // the line-ins (`crate::linein`); its chunks arrive on the v1 path,
        // through `read_requests_and_upstream`.
        if let (true, Some(line_ins)) = (is_source, ctx.line_ins.as_ref()) {
            match &m {
                Message::SourceOffer(offer) => {
                    line_ins.offer(&endpoint, session, offer);
                    return;
                }
                Message::StreamFormat(format) => {
                    line_ins.stream_format(session, format);
                    return;
                }
                _ => {}
            }
        }
        let Message::ControllerCommand(command) = m else {
            return;
        };
        let refused = |reason: &str, detail: &str| {
            ctx.say(&format!(
                "controller refused id={} command={} reason={} detail=\"{}\"",
                endpoint,
                command.command.name(),
                reason,
                detail
            ))
        };
        if !is_controller {
            refused(
                "no-controller-role",
                "the endpoint's hello did not declare the controller role",
            );
            return;
        }
        let Some(control) = ctx.control.as_ref() else {
            refused(
                "no-control-plane",
                "this server runs without --control-listen, so it holds no zone to change",
            );
            return;
        };
        match control.controller(&endpoint, &command) {
            Ok(applied) => {
                let did = match &applied.action {
                    ControllerAction::Apply(_) => "applied".to_string(),
                    ControllerAction::Transport(request) => {
                        format!("transport-{:?}-waits-for-an-input", request).to_lowercase()
                    }
                };
                ctx.say(&format!(
                    "controller id={} zone={} command={} value={} target={} {} volume={} \
                     muted={} group={}",
                    endpoint,
                    applied.zone,
                    command.command.name(),
                    command.value,
                    if command.target.is_empty() {
                        "-"
                    } else {
                        &command.target
                    },
                    did,
                    applied.state.volume,
                    u8::from(applied.state.muted),
                    applied.state.group
                ));
                // Never blocks the reader on an endpoint that is not
                // reading; a state it misses is superseded by the next.
                let _ = ctx
                    .router
                    .push_controller_state(session, &applied.state, true);
            }
            Err(refusal) => refused(&refusal.field, &refusal.detail),
        }
    }));
}

/// Resolve the identity source from the flags: `--ephemeral-identity`, else
/// `--identity-dir`, else the directory of `--state-file`.
pub fn identity_source(
    identity_dir: Option<&str>,
    state_file: Option<&str>,
    ephemeral: bool,
) -> Option<IdentitySource> {
    if ephemeral {
        return Some(IdentitySource::Ephemeral);
    }
    if let Some(dir) = identity_dir {
        return Some(IdentitySource::Directory(PathBuf::from(dir)));
    }
    let state = Path::new(state_file?);
    let dir = match state.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    Some(IdentitySource::Directory(dir))
}

/// Load (or create) the identity and the adoption store.
pub fn load_identity(source: &IdentitySource, id: &str) -> Result<(Identity, Adoptions), String> {
    match source {
        IdentitySource::Ephemeral => {
            let secret = random_32().map_err(|e| format!("/dev/urandom: {}", e))?;
            Ok((
                Identity {
                    id: id.to_string(),
                    keypair: Keypair::from_secret(secret),
                },
                Adoptions::in_memory(),
            ))
        }
        IdentitySource::Directory(dir) => {
            if let Err(e) = fs::create_dir_all(dir) {
                return Err(format!("{}: {}", dir.display(), e));
            }
            let key_path = dir.join(KEY_FILE);
            let keypair = load_or_create_key(&key_path)
                .map_err(|e| format!("{}: {}", key_path.display(), e))?;
            let adoptions = Adoptions::load(&dir.join(ADOPTED_FILE))?;
            Ok((
                Identity {
                    id: id.to_string(),
                    keypair,
                },
                adoptions,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "chorus-server-session-{}-{}",
            name,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_key_is_created_once_with_mode_0600_and_read_back_unchanged() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("key");
        let source = IdentitySource::Directory(dir.clone());
        let (first, _) = load_identity(&source, "s").unwrap();
        let mode = fs::metadata(dir.join(KEY_FILE))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        let text = fs::read_to_string(dir.join(KEY_FILE)).unwrap();
        assert_eq!(text.len(), 65);
        assert!(text.ends_with('\n'));
        let (second, _) = load_identity(&source, "s").unwrap();
        assert_eq!(first.keypair.public, second.keypair.public);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_adoption_is_persisted_and_a_changed_key_leaves_the_file_alone() {
        let dir = scratch("pins");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(ADOPTED_FILE);
        let a = Adoptions::load(&path).unwrap();
        assert_eq!(a.check("den", &[1; 32]).unwrap(), Verdict::Adopted);
        let written = fs::read_to_string(&path).unwrap();
        assert!(written.contains(" den\n"));
        let b = Adoptions::load(&path).unwrap();
        assert!(!b.check("den", &[2; 32]).unwrap().admits());
        assert_eq!(fs::read_to_string(&path).unwrap(), written);
        assert_eq!(b.key_changes().len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_supported_channel_count_has_an_explicit_map_and_others_are_refused() {
        for (n, names) in [
            (1u16, "MONO"),
            (2, "FL FR"),
            (3, "FL FR FC"),
            (4, "FL FR BL BR"),
            (6, "FL FR FC LFE BL BR"),
            (8, "FL FR FC LFE BL BR SL SR"),
        ] {
            let map = channel_map(n).unwrap();
            let got: Vec<&str> = map.iter().map(|p| p.name()).collect();
            assert_eq!(got.join(" "), names);
        }
        for n in [5u16, 7] {
            assert!(channel_map(n).is_none());
        }
    }

    #[test]
    fn a_chunk_that_does_not_fit_a_record_is_refused_at_startup_by_name() {
        // 8 kHz stereo pcm_s16le, 4 bytes a frame, and long chunks so the
        // numbers land in the window between the two bounds.
        let f = PcmFormat::new(8_000, 2, "pcm_s16le").unwrap();
        // 2.046 s is 16368 frames: 65472 bytes of PCM, a 65507 byte frame. It fits.
        assert!(Offer::new(&f, 2_046_000).is_ok());
        // 2.046875 s is 16375 frames: 65500 bytes of PCM, a 65532 byte
        // payload the v1 chunker accepts (its bound is 65535), and a 65535
        // byte frame a record does not carry. 2.1 s is past both bounds.
        for (chunk_us, want) in [(2_046_875u64, 65_535usize), (2_100_000, 67_235)] {
            match Offer::new(&f, chunk_us) {
                Err(OfferRefused::ChunkTooLarge { frame_len, .. }) => assert_eq!(frame_len, want),
                other => panic!("expected a refusal at {} us, got {:?}", chunk_us, other),
            }
        }
        let text = Offer::new(&f, 2_046_875).unwrap_err().to_string();
        assert!(
            text.starts_with(
                "a 65535 byte audio chunk does not fit a v2 record (65519 bytes); shorten --chunk-us"
            ),
            "{}",
            text
        );
    }

    #[test]
    fn negotiation_names_what_the_endpoint_cannot_play() {
        let offer = Offer::new(&PcmFormat::new(48_000, 2, "pcm_s16le").unwrap(), 20_000).unwrap();
        assert_eq!(offer.stream_format.frames_per_chunk, 960);
        let caps = Capabilities {
            codecs: Codec::Pcm.bit(),
            sample_formats: 0b111,
            max_channels: 8,
            sample_rates_hz: vec![44_100, 48_000],
            buffer_ms: 300,
            intrinsic_latency_ns: 0,
            led_count: 0,
            visualizer_bands: 0,
        };
        assert!(offer.negotiate(&caps).is_ok());
        let no_rate = Capabilities {
            sample_rates_hz: vec![44_100],
            ..caps.clone()
        };
        assert_eq!(
            offer.negotiate(&no_rate).unwrap_err().0,
            "rate-not-playable"
        );
        let mono = Capabilities {
            max_channels: 1,
            ..caps.clone()
        };
        assert_eq!(offer.negotiate(&mono).unwrap_err().0, "too-many-channels");
        let no_s16 = Capabilities {
            sample_formats: 0b110,
            ..caps.clone()
        };
        assert_eq!(
            offer.negotiate(&no_s16).unwrap_err().0,
            "sample-format-not-playable"
        );
    }

    /// The Linux client lists FLAC and Opus once it decodes them (goal 6), and
    /// this server still sends only PCM: the negotiation picks from what the
    /// server can send, so a client that decodes more still gets PCM.
    #[test]
    fn an_endpoint_that_also_decodes_flac_and_opus_still_gets_pcm() {
        let offer = Offer::new(&PcmFormat::new(48_000, 2, "pcm_s16le").unwrap(), 20_000).unwrap();
        let caps = Capabilities {
            codecs: Codec::Pcm.bit() | Codec::Flac.bit() | Codec::Opus.bit(),
            sample_formats: 0b111,
            max_channels: 8,
            sample_rates_hz: vec![44_100, 48_000],
            buffer_ms: 300,
            intrinsic_latency_ns: 0,
            led_count: 0,
            visualizer_bands: 0,
        };
        assert_eq!(offer.negotiate(&caps), Ok(Codec::Pcm));
    }
}
