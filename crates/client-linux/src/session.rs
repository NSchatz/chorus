//! Protocol v2 on this endpoint's audio connection: who it is, which server
//! it trusts, and the session every connection opens before any audio.
//!
//! # Identity and the server's pin
//!
//! The endpoint holds one long-term X25519 key, `endpoint.key` in its
//! identity directory (64 hex digits and a newline, mode 0600, created from
//! `/dev/urandom` on first start and never printed), and the key of every
//! server it has talked to, `server-pins` beside it (the text form of
//! [`PinStore`]). The first server under an id is pinned; a later one under
//! the same id with another key is refused by name and nothing is played.
//! The endpoint's id is what the server pins this endpoint's key to, so it
//! must be stable across restarts: `--endpoint-id`, or else `--endpoint`.
//!
//! # The session
//!
//! [`open`] runs the Noise XX handshake with a 2 s read timeout, sends
//! `hello` and `capabilities`, restores the connection's previous read
//! timeout, and hands back a [`SecureReader`] for the v1 receive path to read
//! unchanged and a [`SecureWriter`] for the time-sync exchange. The server's
//! `stream_format` and `output_delay` land in [`Announced`], and
//! [`check_announcement`] holds the first chunk to what was announced.
//!
//! A server that never answers the handshake (a v1 server steps over the
//! unknown frame and waits) ends the wait at the timeout and is refused with a
//! sentence naming chorus protocol v1.

use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chorus_protocol::v2::adoption::{KeyChange, PinStore, Verdict};
use chorus_protocol::v2::noise::{fingerprint, Keypair};
use chorus_protocol::v2::session::{connect, Identity, SecureReader, SecureWriter, SessionError};
use chorus_protocol::v2::{
    roles, Capabilities, Codec, Hello, Message, RefusalReason, SourceControl, SourceOffer,
    StreamFormat, PROTOCOL_VERSION,
};
use chorus_protocol::{SampleFormat, MAX_CHANNELS, MAX_SAMPLE_RATE_HZ, MIN_SAMPLE_RATE_HZ};

use crate::coded::CodedStream;
use crate::config::ClientConfig;
use crate::receive::StreamShape;

/// The file holding the endpoint's long-term secret key.
pub const KEY_FILE: &str = "endpoint.key";

/// The file holding the servers this endpoint has pinned.
pub const PINS_FILE: &str = "server-pins";

/// How long the server has to answer each handshake message.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(2);

/// The standard sample rates inside the band the receive path accepts
/// (`MIN_SAMPLE_RATE_HZ` to `MAX_SAMPLE_RATE_HZ`). The client plays any rate in
/// that band (ALSA is opened at the stream's own rate, and nothing between the
/// socket and the sink depends on it), but `capabilities` carries a list of at
/// most sixteen, so it names these.
pub const OFFERED_RATES_HZ: [u32; 13] = [
    8_000, 11_025, 16_000, 22_050, 32_000, 44_100, 48_000, 88_200, 96_000, 176_400, 192_000,
    352_800, 384_000,
];

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

fn write_atomically(path: &Path, text: &str) -> io::Result<()> {
    let mut temporary = PathBuf::from(path);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "pins".to_string());
    temporary.set_file_name(format!(".{}.writing", name));
    fs::write(&temporary, text)?;
    fs::rename(&temporary, path)
}

/// Who this endpoint is and which servers it trusts.
#[derive(Debug)]
pub struct EndpointIdentity {
    /// The id and long-term key presented in the handshake.
    pub identity: Identity,
    pins: PinStore,
    pins_path: Option<PathBuf>,
}

impl EndpointIdentity {
    /// Load (or create) the identity in `dir`: `endpoint.key` and
    /// `server-pins`.
    pub fn load(dir: &Path, id: &str) -> Result<EndpointIdentity, String> {
        check_id(id)?;
        fs::create_dir_all(dir).map_err(|e| format!("{}: {}", dir.display(), e))?;
        let key_path = dir.join(KEY_FILE);
        let keypair =
            load_or_create_key(&key_path).map_err(|e| format!("{}: {}", key_path.display(), e))?;
        let pins_path = dir.join(PINS_FILE);
        let pins = match fs::read_to_string(&pins_path) {
            Ok(text) => {
                PinStore::from_text(&text).map_err(|e| format!("{}: {}", pins_path.display(), e))?
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => PinStore::new(),
            Err(e) => return Err(format!("{}: {}", pins_path.display(), e)),
        };
        Ok(EndpointIdentity {
            identity: Identity {
                id: id.to_string(),
                keypair,
            },
            pins,
            pins_path: Some(pins_path),
        })
    }

    /// A key for this process alone and pins kept in memory: for tests and
    /// throwaway runs.
    pub fn ephemeral(id: &str) -> Result<EndpointIdentity, String> {
        let secret = random_32().map_err(|e| format!("/dev/urandom: {}", e))?;
        EndpointIdentity::from_secret(id, secret)
    }

    /// An identity with a given secret and pins in memory: for tests that
    /// need a key they know.
    pub fn from_secret(id: &str, secret: [u8; 32]) -> Result<EndpointIdentity, String> {
        check_id(id)?;
        Ok(EndpointIdentity {
            identity: Identity {
                id: id.to_string(),
                keypair: Keypair::from_secret(secret),
            },
            pins: PinStore::new(),
            pins_path: None,
        })
    }

    /// This endpoint's key fingerprint, for logs.
    pub fn fingerprint(&self) -> String {
        fingerprint(&self.identity.keypair.public)
    }

    fn check(&mut self, id: &str, key: &[u8; 32]) -> Result<Verdict, String> {
        let verdict = self.pins.check(id, key);
        if verdict == Verdict::Adopted {
            if let Some(path) = &self.pins_path {
                if let Err(e) = write_atomically(path, &self.pins.to_text()) {
                    self.pins.forget(id);
                    return Err(format!(
                        "the pin for server {} could not be written to {}: {}",
                        id,
                        path.display(),
                        e
                    ));
                }
            }
        }
        Ok(verdict)
    }
}

fn check_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 255 {
        return Err(format!(
            "an endpoint id is 1 to 255 bytes, not {}",
            id.len()
        ));
    }
    Ok(())
}

/// What the server announced inside the session.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Announced {
    /// The server's `hello`.
    pub server_hello: Option<Hello>,
    /// The latest `stream_format`.
    pub stream_format: Option<StreamFormat>,
    /// The latest `output_delay`, in ns.
    pub output_delay_ns: Option<u64>,
    /// Other v2 messages received and not acted on.
    pub other: u64,
    /// `coded_chunk`s decoded into the receive path (`coded.rs`).
    pub decoded_chunks: u64,
    /// Where the server's `source_control`s go (the source role's channel).
    pub source_controls: SourceControls,
}

/// The sending end of the source role's control channel, kept with what the
/// server announced so that every handler the session's reader is given
/// (`record`) forwards `source_control` to it. It compares equal to any other:
/// it is plumbing, not something the server announced.
#[derive(Debug, Clone, Default)]
pub struct SourceControls(Option<mpsc::Sender<SourceControl>>);

impl PartialEq for SourceControls {
    fn eq(&self, _: &SourceControls) -> bool {
        true
    }
}

impl Eq for SourceControls {}

/// An open session.
pub struct Session {
    /// Yields the v1 frames inside the records, for the receive path.
    pub reader: SecureReader<TcpStream>,
    /// Seals what goes up, for the time-sync exchange.
    pub writer: SecureWriter<TcpStream>,
    /// What the server has announced so far.
    pub announced: Arc<Mutex<Announced>>,
    /// The server's id.
    pub server_id: String,
    /// The server key's fingerprint.
    pub server_key: String,
    /// Whether the server was pinned just now (first use) rather than known.
    pub pinned_now: bool,
    /// The server's `source_control` messages, in order, for the source role
    /// (`crate::source`). Nothing reads it on an endpoint with no line-in.
    pub source_control: Receiver<SourceControl>,
}

/// Why a session did not open.
#[derive(Debug)]
pub enum SessionRefusal {
    /// The server never answered the handshake, or answered with v1 audio.
    MaySpeakV1 {
        /// What was seen.
        detail: String,
    },
    /// The server refused the session and said why.
    RefusedByServer {
        /// The reason's name (`key_changed`, `protocol_version`, ...).
        reason: &'static str,
        /// The server's sentence.
        detail: String,
    },
    /// The server's key is not the one pinned for its id.
    ServerKeyChanged(KeyChange),
    /// The connection or the handshake failed.
    Failed {
        /// What happened.
        detail: String,
    },
}

impl SessionRefusal {
    /// The `reason=` word for this refusal on a status line.
    pub fn reason(&self) -> String {
        match self {
            SessionRefusal::MaySpeakV1 { .. } => "server-may-speak-v1".to_string(),
            SessionRefusal::RefusedByServer { reason, .. } => {
                format!("session-refused-{}", reason.replace('_', "-"))
            }
            SessionRefusal::ServerKeyChanged(_) => "server-key-changed".to_string(),
            SessionRefusal::Failed { .. } => "session-failed".to_string(),
        }
    }
}

impl fmt::Display for SessionRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SessionRefusal::MaySpeakV1 { detail } => write!(
                f,
                "the server did not answer the protocol v2 handshake ({}); the server may speak \
                 chorus protocol v1, which this endpoint no longer plays",
                detail
            ),
            SessionRefusal::RefusedByServer { reason, detail } => {
                write!(f, "the server refused the session ({}): {}", reason, detail)
            }
            SessionRefusal::ServerKeyChanged(change) => write!(
                f,
                "server key changed: {}; refused, and nothing is played until the owner forgets \
                 the pin",
                change
            ),
            SessionRefusal::Failed { detail } => write!(f, "the session failed: {}", detail),
        }
    }
}

impl std::error::Error for SessionRefusal {}

/// The `hello` this endpoint sends: `player`, the roles a configured front
/// panel adds (`controller`, and `visualizer` for a light that follows it;
/// `front_panel.rs`), and `source` when a line-in is configured (K65).
pub fn hello(config: &ClientConfig) -> Hello {
    let source = if config.line_in.is_some() {
        roles::SOURCE
    } else {
        0
    };
    Hello {
        protocol_version: PROTOCOL_VERSION,
        roles: roles::PLAYER | source | (config.extra_roles & roles::DEFINED),
        name: config.endpoint.clone(),
        software: format!("chorus-client {}", env!("CARGO_PKG_VERSION")),
    }
}

/// The `capabilities` this endpoint sends: what its receive path and its
/// ALSA sink accept. PCM, FLAC and Opus (`decode.rs`: Opus in mapping family
/// 0, one or two channels, which the server's choice checks against the
/// stream); all three sample formats (`AlsaSink::open` maps each, `ZoneGain` scales each);
/// up to `MAX_CHANNELS`, or with an output map the distinct positions it
/// reads (`OutputMap::max_stream_channels`); the standard rates of [`OFFERED_RATES_HZ`].
pub fn capabilities(config: &ClientConfig) -> Capabilities {
    let formats = [
        SampleFormat::PcmS16Le,
        SampleFormat::PcmS24Le,
        SampleFormat::PcmF32Le,
    ];
    let rates: Vec<u32> = OFFERED_RATES_HZ
        .iter()
        .copied()
        .filter(|r| (MIN_SAMPLE_RATE_HZ..=MAX_SAMPLE_RATE_HZ).contains(r))
        .collect();
    Capabilities {
        codecs: Codec::Pcm.bit() | Codec::Flac.bit() | Codec::Opus.bit(),
        sample_formats: formats
            .iter()
            .fold(0u8, |bits, f| bits | 1 << (f.to_wire() - 1)),
        max_channels: config
            .output_map
            .as_ref()
            .map(|m| m.max_stream_channels())
            .unwrap_or(MAX_CHANNELS as u8),
        sample_rates_hz: rates,
        buffer_ms: (config.max_us / 1_000).min(u64::from(u16::MAX)) as u16,
        intrinsic_latency_ns: 0,
        // One status light when the front panel declared the visualizer role.
        led_count: u16::from(config.extra_roles & roles::VISUALIZER != 0),
        visualizer_bands: 0,
    }
}

/// The first `source_offer` for a configured line-in, sent right after
/// `capabilities`: no signal yet, because none has been measured. The source
/// role offers again when its detector says otherwise (`crate::source`).
pub fn first_source_offer(config: &ClientConfig) -> Option<SourceOffer> {
    config.line_in.as_ref().map(|input| SourceOffer {
        source_id: input.source_id,
        kind: input.kind,
        signal: false,
        name: input.name.clone(),
    })
}

fn is_v1_answer(detail: &str) -> bool {
    ["time_sync", "audio_chunk", "stream_end"]
        .iter()
        .any(|name| detail == format!("expected handshake_response, got {}", name))
}

/// Open a session on a connected stream: the handshake, then `hello` and
/// `capabilities`. The stream's read timeout is [`HANDSHAKE_TIMEOUT`] while
/// the server is waited on and is put back to what it was before.
pub fn open(
    stream: TcpStream,
    me: &mut EndpointIdentity,
    config: &ClientConfig,
) -> Result<Session, SessionRefusal> {
    let failed = |e: io::Error| SessionRefusal::Failed {
        detail: e.to_string(),
    };
    let before = stream.read_timeout().map_err(failed)?;
    stream
        .set_read_timeout(Some(HANDSHAKE_TIMEOUT))
        .map_err(failed)?;
    let ephemeral = Keypair::from_secret(random_32().map_err(failed)?);
    let identity = me.identity.clone();
    let mut pin_failure = None;
    let mut handshake = &stream;
    let established = connect(&mut handshake, &identity, ephemeral, |id, key| {
        match me.check(id, key) {
            Ok(v) => v,
            Err(e) => {
                pin_failure = Some(e);
                Verdict::Removed
            }
        }
    });
    let established = match established {
        Ok(e) => e,
        Err(e) => {
            if let Some(detail) = pin_failure {
                return Err(SessionRefusal::Failed { detail });
            }
            return Err(match e {
                SessionError::NoV2Answer(io) => SessionRefusal::MaySpeakV1 {
                    detail: io.to_string(),
                },
                SessionError::Protocol(detail) if is_v1_answer(&detail) => {
                    SessionRefusal::MaySpeakV1 { detail }
                }
                SessionError::Refused { reason, detail } => SessionRefusal::RefusedByServer {
                    reason: reason.name(),
                    detail,
                },
                SessionError::KeyChanged(change) => SessionRefusal::ServerKeyChanged(change),
                other => SessionRefusal::Failed {
                    detail: other.to_string(),
                },
            });
        }
    };
    let writer_stream = stream.try_clone().map_err(failed)?;
    let writer_timeout_handle = stream.try_clone().map_err(failed)?;
    let mut writer = SecureWriter::new(writer_stream, established.sealer);
    writer
        .send(&Message::Hello(hello(config)))
        .and_then(|_| writer.send(&Message::Capabilities(capabilities(config))))
        .map_err(failed)?;
    if let Some(offer) = first_source_offer(config) {
        writer.send(&Message::SourceOffer(offer)).map_err(failed)?;
    }

    // The server's verdict on this endpoint's key comes after the handshake
    // (Noise XX authenticates the endpoint last), so the session is only open
    // once the server's own `hello` has arrived: a refusal (`key_changed`,
    // `not_adopted`) arrives instead, in the clear, and is surfaced here by
    // name rather than on the first read of audio.
    let (controls_tx, source_control) = mpsc::channel::<SourceControl>();
    let announced = Arc::new(Mutex::new(Announced {
        source_controls: SourceControls(Some(controls_tx)),
        ..Announced::default()
    }));
    let mut reader = SecureReader::new(stream, established.opener);
    loop {
        match reader.next_message() {
            Ok(m) => {
                let is_hello = matches!(m, Message::Hello(_));
                record(&announced, m);
                if is_hello {
                    break;
                }
            }
            Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => {
                let text = e.to_string();
                let reason = refusal_name(&text);
                let prefix = format!("the peer refused the session ({}): ", reason);
                let detail = text.strip_prefix(&prefix).unwrap_or(&text).to_string();
                return Err(SessionRefusal::RefusedByServer { reason, detail });
            }
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                return Err(SessionRefusal::Failed {
                    detail: "the server closed the session after this endpoint's capabilities \
                             without greeting it (see the server's log for the reason; it may \
                             not play this endpoint's formats)"
                        .to_string(),
                })
            }
            Err(e) => return Err(failed(e)),
        }
    }
    {
        let recorder = Arc::clone(&announced);
        reader.set_handler(Box::new(move |m| record(&recorder, m)));
        reader.set_translator(CodedStream::new(Arc::clone(&announced)).into_translator());
    }
    let stream_timeout = writer_timeout_handle.set_read_timeout(before);
    stream_timeout.map_err(failed)?;
    Ok(Session {
        reader,
        writer,
        announced,
        server_id: established.peer_id,
        server_key: fingerprint(&established.peer_key),
        pinned_now: established.verdict == Verdict::Adopted,
        source_control,
    })
}

/// From now on, also show every v2 message the session receives to `also`
/// (the front panel takes `controller_state`, `visualizer_frame` and
/// `color`); what [`open`] records is recorded as before.
pub fn also_hand(
    reader: &mut SecureReader<TcpStream>,
    announced: &Arc<Mutex<Announced>>,
    mut also: Box<dyn FnMut(&Message) + Send>,
) {
    let recorder = Arc::clone(announced);
    reader.set_handler(Box::new(move |m| {
        also(&m);
        record(&recorder, m);
    }));
}

fn record(announced: &Arc<Mutex<Announced>>, m: Message) {
    let mut a = match announced.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    match m {
        Message::Hello(h) => a.server_hello = Some(h),
        Message::StreamFormat(f) => a.stream_format = Some(f),
        Message::OutputDelay(d) => a.output_delay_ns = Some(d.delay_ns),
        Message::SourceControl(c) => {
            // A send fails only when nothing runs the source role (an
            // endpoint with no line-in); it is still counted.
            if let Some(tx) = &a.source_controls.0 {
                let _ = tx.send(c);
            }
            a.other += 1;
        }
        _ => a.other += 1,
    }
}

/// The reason name inside a refusal surfaced by `SecureReader`, whose text is
/// `the peer refused the session (<reason>): <detail>`.
fn refusal_name(text: &str) -> &'static str {
    RefusalReason::ALL
        .iter()
        .find(|r| text.contains(&format!("({})", r.name())))
        .map(|r| r.name())
        .unwrap_or("unknown")
}

/// Hold the first chunk to the announcement: a stream whose chunks disagree
/// with the `stream_format` that preceded them is a framing error.
pub fn check_announcement(announced: &Announced, shape: &StreamShape) -> Result<(), String> {
    let Some(f) = &announced.stream_format else {
        return Err(
            "framing error: the first audio chunk arrived with no stream_format announced before it"
                .to_string(),
        );
    };
    // A FLAC or Opus stream reaches the receive path as the audio_chunk frames
    // its decoded coded_chunks were cut into (coded.rs); plain audio_chunk
    // frames under a coded announcement are a framing error.
    if f.codec != Codec::Pcm && announced.decoded_chunks == 0 {
        return Err(format!(
            "framing error: the stream was announced as {} and arrived as pcm audio_chunk frames",
            f.codec.name()
        ));
    }
    if f.sample_rate_hz != shape.sample_rate_hz
        || f.channel_map.len() != usize::from(shape.channels)
        || f.sample_format != shape.sample_format
    {
        return Err(format!(
            "framing error: the stream was announced as {} Hz, {} channels, {} and the first \
             chunk is {} Hz, {} channels, {}",
            f.sample_rate_hz,
            f.channel_map.len(),
            f.sample_format.name(),
            shape.sample_rate_hz,
            shape.channels,
            shape.sample_format.name()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chorus_protocol::v2::ChannelPosition;

    #[test]
    fn the_capabilities_name_all_three_codecs_all_three_formats_and_rates_inside_the_band() {
        let c = capabilities(&ClientConfig::default());
        assert_eq!(
            c.codecs,
            Codec::Pcm.bit() | Codec::Flac.bit() | Codec::Opus.bit()
        );
        assert_eq!(c.sample_formats, 0b111);
        assert_eq!(c.max_channels, 8);
        assert!(c.sample_rates_hz.contains(&48_000) && c.sample_rates_hz.contains(&44_100));
        assert!(c.sample_rates_hz.len() <= 16);
    }

    #[test]
    fn an_announcement_that_disagrees_with_the_first_chunk_is_a_framing_error() {
        let shape = StreamShape {
            sample_rate_hz: 48_000,
            channels: 2,
            sample_format: SampleFormat::PcmS16Le,
            frames_per_chunk: 960,
        };
        let mut a = Announced::default();
        assert!(check_announcement(&a, &shape).is_err());
        a.stream_format = Some(StreamFormat {
            codec: Codec::Pcm,
            sample_format: SampleFormat::PcmS16Le,
            sample_rate_hz: 48_000,
            channel_map: vec![ChannelPosition::FrontLeft, ChannelPosition::FrontRight],
            frames_per_chunk: 960,
            codec_config: Vec::new(),
        });
        assert!(check_announcement(&a, &shape).is_ok());
        let other = StreamShape {
            sample_rate_hz: 44_100,
            ..shape
        };
        let e = check_announcement(&a, &other).unwrap_err();
        assert!(e.starts_with("framing error"), "{}", e);
    }

    #[test]
    fn a_pin_file_is_written_on_first_use_and_a_changed_server_key_is_refused() {
        let dir = std::env::temp_dir().join(format!("chorus-client-pins-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut me = EndpointIdentity::load(&dir, "den").unwrap();
        assert_eq!(me.check("srv", &[1; 32]).unwrap(), Verdict::Adopted);
        let again = EndpointIdentity::load(&dir, "den").unwrap();
        assert_eq!(
            again.identity.keypair.public, me.identity.keypair.public,
            "the key survives a restart"
        );
        let mut again = again;
        assert!(!again.check("srv", &[2; 32]).unwrap().admits());
        let _ = fs::remove_dir_all(&dir);
    }
}
