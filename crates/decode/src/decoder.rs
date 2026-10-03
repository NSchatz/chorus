//! One piece of media, opened and decoded to interleaved `f32` frames.
//!
//! Symphonia is used through its public API only (docs.rs for 0.6.1,
//! <https://docs.rs/symphonia-core/0.6.1/symphonia_core/>, read 2026-10-03):
//! the probe, `FormatReader::next_packet`, `AudioDecoder::decode` with
//! `AudioDecoderOptions::gapless`, the metadata log, and `FormatReader::seek`.

use std::fmt;
use std::io::{self, SeekFrom};

use symphonia::core::codecs::audio::well_known::{
    CODEC_ID_AAC, CODEC_ID_AC3, CODEC_ID_ALAC, CODEC_ID_EAC3, CODEC_ID_FLAC, CODEC_ID_MP1,
    CODEC_ID_MP2, CODEC_ID_MP3, CODEC_ID_OPUS, CODEC_ID_VORBIS,
};
use symphonia::core::codecs::audio::{AudioCodecParameters, AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::probe::Hint as SymHint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, Track, TrackType};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::{MetadataOptions, MetadataRevision, StandardTag};
use symphonia::core::units::Timestamp;

use crate::media::{read_full, Media, Source};

/// What the caller knows about the media before a byte is read. Neither field
/// decides anything alone: the bytes are sniffed, and a wrong type on bytes
/// that decode is ignored (UPnP control points send generic types).
#[derive(Debug, Clone, Default)]
pub struct Hint {
    /// The media type, as an HTTP `Content-Type` gives it.
    pub mime: Option<String>,
    /// The file name's extension, without the dot.
    pub extension: Option<String>,
}

/// The formats chorus decodes (proposal P9). There is no AAC here and never a
/// build that has it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    /// MPEG-1 and MPEG-2 Layer III.
    Mp3,
    /// FLAC, in its own container.
    Flac,
    /// Vorbis in Ogg.
    Vorbis,
    /// Opus in Ogg, mapping family 0; always decoded at 48 kHz.
    Opus,
    /// Apple Lossless in MP4.
    Alac,
    /// Linear PCM: WAV, or raw L16.
    Pcm,
}

impl Codec {
    /// The lower-case name used in diagnostics and docs.
    pub fn name(self) -> &'static str {
        match self {
            Codec::Mp3 => "mp3",
            Codec::Flac => "flac",
            Codec::Vorbis => "vorbis",
            Codec::Opus => "opus",
            Codec::Alac => "alac",
            Codec::Pcm => "pcm",
        }
    }
}

/// The shape of what [`Decoder::read`] gives back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Format {
    /// The codec.
    pub codec: Codec,
    /// Frames per second of the decoded audio.
    pub rate: u32,
    /// Channels per frame.
    pub channels: u16,
    /// Bits per sample of a lossless source; `None` for a lossy one. Sources of
    /// up to 24 bits are exact in the `f32` output: sample / 2^(bits-1).
    pub bits: Option<u8>,
    /// The frames the media decodes to, after gapless trimming, when the
    /// container says so up front. A stream, and Ogg on unseekable media, do
    /// not.
    pub frames: Option<u64>,
}

/// What the media says about itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tags {
    /// The track's title.
    pub title: Option<String>,
    /// The artist.
    pub artist: Option<String>,
    /// The album.
    pub album: Option<String>,
}

/// Why media could not be opened or decoded.
#[derive(Debug)]
pub enum DecodeError {
    /// chorus does not decode this, by decision or by scope; the text names
    /// what: `aac`, `opus mapping family 1`,
    /// `mp4 with moov at the end on a non-seekable source`.
    Unsupported(String),
    /// The media claims a supported format and does not hold to it.
    Malformed(String),
    /// Reading the media failed.
    Io(io::Error),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::Unsupported(what) => write!(f, "unsupported: {what}"),
            DecodeError::Malformed(what) => write!(f, "malformed: {what}"),
            DecodeError::Io(e) => write!(f, "read failed: {e}"),
        }
    }
}

impl std::error::Error for DecodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DecodeError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for DecodeError {
    fn from(e: io::Error) -> Self {
        DecodeError::Io(e)
    }
}

fn from_symphonia(e: SymError) -> DecodeError {
    match e {
        SymError::IoError(e) => DecodeError::Io(e),
        SymError::DecodeError(what) => DecodeError::Malformed(what.to_string()),
        SymError::Unsupported(what) => DecodeError::Unsupported(what.to_string()),
        SymError::LimitError(what) => {
            DecodeError::Malformed(format!("over a decoder limit: {what}"))
        }
        SymError::SeekError(kind) => DecodeError::Malformed(format!("seek: {kind:?}")),
        SymError::ResetRequired => DecodeError::Malformed("the stream changed shape".to_string()),
        other => DecodeError::Malformed(other.to_string()),
    }
}

/// How many leading bytes are looked at before Symphonia's probe runs.
const SNIFF_BYTES: usize = 4096;
/// An Opus decoder needs a run-up before a seek target to converge. RFC 7845
/// section 4 (<https://www.rfc-editor.org/rfc/rfc7845>, read 2026-10-03): "An
/// implementation SHOULD start decoding (and discarding the output) at least
/// 3840 samples (80 ms) prior to the seek target."
/// The same run-up, a little longer, serves MP3 (its bit reservoir and
/// overlap) and Vorbis (its overlap with the block before).
const SEEK_PREROLL: u64 = 4096;

/// The Opus identification header (RFC 7845 section 5.1).
struct OpusHead {
    channels: u8,
    pre_skip: u16,
    gain_q8: i16,
}

fn parse_opus_head(extra: Option<&[u8]>) -> Result<OpusHead, DecodeError> {
    let head = extra.ok_or_else(|| DecodeError::Malformed("opus: no OpusHead".to_string()))?;
    if head.len() < 19 || &head[..8] != b"OpusHead" {
        return Err(DecodeError::Malformed("opus: not an OpusHead".to_string()));
    }
    if head[8] >> 4 != 0 {
        return Err(DecodeError::Unsupported(format!(
            "opus header version {}",
            head[8]
        )));
    }
    let family = head[18];
    if family != 0 {
        return Err(DecodeError::Unsupported(format!(
            "opus mapping family {family}"
        )));
    }
    let channels = head[9];
    if !(1..=2).contains(&channels) {
        return Err(DecodeError::Malformed(format!(
            "opus: mapping family 0 with {channels} channels"
        )));
    }
    Ok(OpusHead {
        channels,
        pre_skip: u16::from_le_bytes([head[10], head[11]]),
        gain_q8: i16::from_le_bytes([head[16], head[17]]),
    })
}

/// The decoder of one Ogg Opus link: libopus as the endpoints build it.
struct OpusDecode {
    decoder: chorus_opus_sys::Decoder,
    channels: u8,
    /// The OpusHead output gain as a linear factor. It is applied here, to the
    /// `f32` output, and not by libopus's `OPUS_SET_GAIN`: in libopus 1.6.1
    /// built as chorus builds it (fixed point with `ENABLE_RES24`) the gain
    /// path saturates the 24-bit samples at the 16-bit limit
    /// (`third_party/opus/src/opus_decoder.c`, `pcm[i] = SATURATE(x, 32767)`
    /// under `ENABLE_RES24`), which clips everything above -48 dBFS as soon
    /// as a gain is set (measured: `opus_output_gain_is_applied` failed with
    /// the decode at 0.022 of its level for a gain of -6.02 dB).
    gain: f32,
    pre_skip: u64,
    pcm: Vec<i32>,
}

/// RFC 7845 section 5.1: the output gain is "a gain to be applied when
/// decoding", a Q7.8 value in dB, and "players and media frameworks SHOULD
/// apply it by default" (<https://www.rfc-editor.org/rfc/rfc7845>, read
/// 2026-10-03): the factor is 10^(gain / (20 * 256)).
fn opus_gain_factor(gain_q8: i16) -> f32 {
    10f64.powf(f64::from(gain_q8) / (20.0 * 256.0)) as f32
}

enum Engine {
    Symphonia(Box<dyn AudioDecoder>),
    Opus(OpusDecode),
}

struct Demuxed {
    reader: Box<dyn FormatReader>,
    track_id: u32,
    engine: Engine,
    /// Decoded frames still to drop from the front: the Opus pre-skip.
    skip: u64,
    /// After a seek: the frame, as the track's timestamps count them, the
    /// output resumes at. Timestamp 0 is the first frame a listener hears (an
    /// encoder's delay lies before it, at negative timestamps), except in
    /// Opus, where it is the first pre-skip frame. Packets before the frame
    /// are decoded for the decoder's sake and dropped.
    resume_at: Option<u64>,
    /// The track's time base, for placing packets.
    time_base: Option<(u32, u32)>,
    rate: u32,
    scratch: Vec<f32>,
}

struct RawL16 {
    media: Box<dyn Media>,
    bytes: Vec<u8>,
    /// A frame's bytes cut by the end of a read, kept for the next one.
    carry: Vec<u8>,
}

enum Inner {
    Demuxed(Box<Demuxed>),
    L16(RawL16),
}

/// One opened piece of media.
pub struct Decoder {
    inner: Inner,
    format: Format,
    tags: Tags,
    seekable: bool,
}

impl Decoder {
    /// Opens `media`: finds the container and the codec from the bytes
    /// (helped, never overruled, by `hint`) and reads its headers and tags.
    ///
    /// Refusals are [`DecodeError::Unsupported`] naming what: `aac` (an MP4
    /// AAC track, an ADTS stream, or bytes nothing recognises that the hint
    /// calls AAC), an Opus mapping family other than 0, an MP4 whose index
    /// (`moov`) lies after its audio on media that cannot seek, and any other
    /// codec or container.
    pub fn open(mut media: Box<dyn Media>, hint: &Hint) -> Result<Decoder, DecodeError> {
        let seekable = media.is_seekable();
        let mut head = vec![0u8; SNIFF_BYTES];
        let n = read_full(media.as_mut(), &mut head)?;
        head.truncate(n);
        if is_adts(&head) {
            return Err(DecodeError::Unsupported("aac".to_string()));
        }
        if !seekable && mp4_index_is_after_the_audio(&head) {
            return Err(DecodeError::Unsupported(
                "mp4 with moov at the end on a non-seekable source".to_string(),
            ));
        }
        let source = if seekable {
            media.seek(SeekFrom::Start(0))?;
            Source::new(media, Vec::new())
        } else {
            Source::new(media, head)
        };
        let stream = MediaSourceStream::new(Box::new(source), MediaSourceStreamOptions::default());
        let mut sym_hint = SymHint::new();
        if let Some(ext) = hint.extension.as_deref() {
            sym_hint.with_extension(ext);
        }
        if let Some(mime) = hint.mime.as_deref() {
            // Parameters (`;rate=...`) are not part of the type the probe knows.
            sym_hint.mime_type(mime.split(';').next().unwrap_or(mime).trim());
        }
        let reader = symphonia::default::get_probe()
            .probe(
                &sym_hint,
                stream,
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(|e| match e {
                SymError::Unsupported(_) if hint_says_aac(hint) => {
                    DecodeError::Unsupported("aac".to_string())
                }
                SymError::Unsupported(what) => {
                    DecodeError::Unsupported(format!("no decodable format found ({what})"))
                }
                // The probe ran off the end of the bytes: too short to be anything.
                SymError::IoError(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                    DecodeError::Unsupported("no decodable format found (too short)".to_string())
                }
                other => from_symphonia(other),
            })?;
        let started = start_track(reader.as_ref(), seekable)?;
        let format = started.format.clone();
        let mut demuxed = Demuxed {
            reader,
            track_id: 0,
            engine: started.engine,
            skip: 0,
            resume_at: None,
            time_base: None,
            rate: 0,
            scratch: Vec::new(),
        };
        demuxed.adopt(started.track, started.skip, &format);
        let mut decoder = Decoder {
            inner: Inner::Demuxed(Box::new(demuxed)),
            format,
            tags: Tags::default(),
            seekable,
        };
        decoder.refresh_tags();
        Ok(decoder)
    }

    /// Opens raw 16-bit big-endian PCM (`audio/L16;rate=..;channels=..`, RFC
    /// 3551 section 4.5.11, `ASSUMED` from memory: network byte order, signed),
    /// which no sniffing can recognise: the caller read the rate and the
    /// channel count from the media type.
    pub fn open_l16(
        media: Box<dyn Media>,
        rate: u32,
        channels: u16,
    ) -> Result<Decoder, DecodeError> {
        if rate == 0 || channels == 0 {
            return Err(DecodeError::Malformed(format!(
                "L16 with rate {rate} and {channels} channels"
            )));
        }
        let seekable = media.is_seekable();
        let frames = media.byte_len().map(|b| b / (2 * u64::from(channels)));
        Ok(Decoder {
            inner: Inner::L16(RawL16 {
                media,
                bytes: vec![0u8; 16384],
                carry: Vec::new(),
            }),
            format: Format {
                codec: Codec::Pcm,
                rate,
                channels,
                bits: Some(16),
                frames,
            },
            tags: Tags::default(),
            seekable,
        })
    }

    /// The shape of the frames the last [`Decoder::read`] gave (or the next
    /// will give, before the first). It changes only at a new link of a
    /// chained Ogg stream, and a `read` never mixes two shapes: check it after
    /// each `read`.
    pub fn format(&self) -> &Format {
        &self.format
    }

    /// The tags read so far; refreshed as the stream reveals more (a new link
    /// of a chained Ogg stream brings new ones).
    pub fn tags(&self) -> &Tags {
        &self.tags
    }

    /// Decodes some more of the media and appends it to `out` as interleaved
    /// `f32` frames, full scale 1.0. Returns the frames appended; `Ok(0)` is
    /// the end of the media. Encoder delay and padding are already trimmed
    /// (gapless). A packet that does not decode is skipped, as a player skips
    /// it; an error is one the stream cannot continue past.
    pub fn read(&mut self, out: &mut Vec<f32>) -> Result<usize, DecodeError> {
        loop {
            let channels = usize::from(self.format.channels);
            let step = match &mut self.inner {
                Inner::L16(raw) => return raw.read(channels, out),
                Inner::Demuxed(demuxed) => demuxed.read_packet(out, channels)?,
            };
            match step {
                Step::Frames(0) => {}
                Step::Frames(n) => {
                    self.refresh_tags();
                    return Ok(n);
                }
                Step::End => return Ok(0),
                Step::NewLink => {
                    if let Inner::Demuxed(demuxed) = &mut self.inner {
                        // Nothing of a later link's length is known up front.
                        let started = start_track(demuxed.reader.as_ref(), false)?;
                        demuxed.engine = started.engine;
                        demuxed.adopt(started.track, started.skip, &started.format);
                        self.format = started.format;
                    }
                    self.refresh_tags();
                }
            }
        }
    }

    /// Moves to `frame` (counted in decoded frames from the start, after
    /// gapless trimming) and returns the frame the next [`Decoder::read`]
    /// starts at, which is `frame`. On media that cannot seek:
    /// [`DecodeError::Unsupported`].
    pub fn seek(&mut self, frame: u64) -> Result<u64, DecodeError> {
        if !self.seekable {
            return Err(DecodeError::Unsupported(
                "seek on a non-seekable source".to_string(),
            ));
        }
        match &mut self.inner {
            Inner::L16(raw) => {
                let offset = frame * 2 * u64::from(self.format.channels);
                raw.media.seek(SeekFrom::Start(offset))?;
                raw.carry.clear();
                Ok(frame)
            }
            Inner::Demuxed(demuxed) => demuxed.seek(frame),
        }
    }

    fn refresh_tags(&mut self) {
        let Inner::Demuxed(demuxed) = &mut self.inner else {
            return;
        };
        let mut metadata = demuxed.reader.metadata();
        // Walk the log to its newest revision; each one may add or replace.
        loop {
            if let Some(revision) = metadata.current() {
                merge_tags(&mut self.tags, revision);
            }
            if metadata.is_latest() || metadata.pop().is_none() {
                break;
            }
        }
    }
}

fn merge_tags(tags: &mut Tags, revision: &MetadataRevision) {
    let containers =
        std::iter::once(&revision.media).chain(revision.per_track.iter().map(|t| &t.metadata));
    for container in containers {
        for tag in &container.tags {
            match &tag.std {
                Some(StandardTag::TrackTitle(v)) => tags.title = Some(v.to_string()),
                Some(StandardTag::Artist(v)) => tags.artist = Some(v.to_string()),
                Some(StandardTag::Album(v)) => tags.album = Some(v.to_string()),
                _ => {}
            }
        }
    }
}

enum Step {
    Frames(usize),
    End,
    NewLink,
}

/// Picks the audio track, builds its decoder, and says what it decodes to.
/// The sample depth in ALAC's setup record, which MP4 carries as the codec's
/// extra data: 24 bytes, the depth in the sixth (frame length, 4 bytes; a
/// compatible-version byte; the bit depth; ...). The layout is Apple's
/// `ALACSpecificConfig` (`ALACMagicCookieDescription.txt` in Apple's
/// Apache-2.0 ALAC sources, `ASSUMED` from memory, not re-read); the 16-bit and
/// the 24-bit fixture both hold it.
fn alac_bit_depth(codec: Codec, extra: Option<&[u8]>) -> Option<u32> {
    let extra = extra.filter(|e| codec == Codec::Alac && e.len() == 24)?;
    Some(u32::from(extra[5])).filter(|bits| matches!(bits, 16 | 20 | 24 | 32))
}

/// What opening a track gives: its decoder, the frames to drop first, its shape.
struct Started {
    track: Track,
    engine: Engine,
    skip: u64,
    format: Format,
}

fn start_track(reader: &dyn FormatReader, lengths_known: bool) -> Result<Started, DecodeError> {
    let track: Track = reader
        .default_track(TrackType::Audio)
        .cloned()
        .ok_or_else(|| DecodeError::Unsupported("no audio track".to_string()))?;
    let params: &AudioCodecParameters = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or_else(|| DecodeError::Unsupported("no audio track".to_string()))?;
    let frames = track.num_frames.filter(|_| lengths_known);
    if params.codec == CODEC_ID_OPUS {
        let head = parse_opus_head(params.extra_data.as_deref())?;
        let decoder = chorus_opus_sys::Decoder::new(head.channels, 0)
            .map_err(|e| DecodeError::Malformed(format!("opus: {e}")))?;
        let skip = u64::from(head.pre_skip);
        let channels = head.channels;
        let engine = Engine::Opus(OpusDecode {
            decoder,
            channels,
            gain: opus_gain_factor(head.gain_q8),
            pre_skip: u64::from(head.pre_skip),
            pcm: vec![0i32; chorus_opus_sys::MAX_FRAMES * usize::from(channels)],
        });
        let format = Format {
            codec: Codec::Opus,
            rate: 48_000,
            channels: u16::from(channels),
            bits: None,
            // The demuxer counts the pre-skip in; the listener never hears it.
            frames: frames.map(|n| n.saturating_sub(skip)),
        };
        return Ok(Started {
            track,
            engine,
            skip,
            format,
        });
    }
    let (codec, lossless) = match params.codec {
        id if id == CODEC_ID_MP3 => (Codec::Mp3, false),
        id if id == CODEC_ID_VORBIS => (Codec::Vorbis, false),
        id if id == CODEC_ID_FLAC => (Codec::Flac, true),
        id if id == CODEC_ID_ALAC => (Codec::Alac, true),
        id if id == CODEC_ID_AAC => return Err(DecodeError::Unsupported("aac".to_string())),
        id if id == CODEC_ID_MP1 => {
            return Err(DecodeError::Unsupported("mpeg layer 1".to_string()))
        }
        id if id == CODEC_ID_MP2 => {
            return Err(DecodeError::Unsupported("mpeg layer 2".to_string()))
        }
        id if id == CODEC_ID_AC3 => return Err(DecodeError::Unsupported("ac-3".to_string())),
        id if id == CODEC_ID_EAC3 => return Err(DecodeError::Unsupported("e-ac-3".to_string())),
        // What is left that Symphonia can decode here is the PCM family.
        _ => (Codec::Pcm, true),
    };
    let decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default().gapless(true))
        .map_err(|e| match e {
            SymError::Unsupported(_) => {
                DecodeError::Unsupported(format!("audio codec {}", params.codec))
            }
            other => from_symphonia(other),
        })?;
    let rate = params
        .sample_rate
        .filter(|r| *r > 0)
        .ok_or_else(|| DecodeError::Malformed("no sample rate".to_string()))?;
    let channels = params
        .channels
        .as_ref()
        .map(|c| c.count())
        .and_then(|c| u16::try_from(c).ok())
        .filter(|c| *c > 0)
        .ok_or_else(|| DecodeError::Malformed("no channel layout".to_string()))?;
    // The container may not say the depth (MP4 does not for ALAC); the decoder,
    // which has read the codec's own setup, does.
    let bits = params
        .bits_per_sample
        .or(decoder.codec_params().bits_per_sample)
        .or_else(|| alac_bit_depth(codec, params.extra_data.as_deref()))
        .and_then(|b| u8::try_from(b).ok())
        .filter(|_| lossless);
    let format = Format {
        codec,
        rate,
        channels,
        bits,
        frames,
    };
    Ok(Started {
        track,
        engine: Engine::Symphonia(decoder),
        skip: 0,
        format,
    })
}

impl Demuxed {
    fn adopt(&mut self, track: Track, skip: u64, format: &Format) {
        self.track_id = track.id;
        self.skip = skip;
        self.resume_at = None;
        self.time_base = track.time_base.map(|tb| (tb.numer.get(), tb.denom.get()));
        self.rate = format.rate;
    }

    /// Frames to timestamp ticks: one tick is numer/denom seconds.
    fn ticks(&self, frames: u64) -> i64 {
        match self.time_base {
            Some((numer, denom)) => {
                let t = u128::from(frames) * u128::from(denom)
                    / (u128::from(numer) * u128::from(self.rate));
                i64::try_from(t).unwrap_or(i64::MAX)
            }
            None => i64::try_from(frames).unwrap_or(i64::MAX),
        }
    }

    fn frames_of(&self, ticks: u64) -> u64 {
        match self.time_base {
            Some((numer, denom)) => {
                let f = u128::from(ticks) * u128::from(numer) * u128::from(self.rate)
                    / u128::from(denom);
                u64::try_from(f).unwrap_or(u64::MAX)
            }
            None => ticks,
        }
    }

    fn read_packet(&mut self, out: &mut Vec<f32>, channels: usize) -> Result<Step, DecodeError> {
        let packet = loop {
            match self.reader.next_packet() {
                Ok(Some(p)) if p.track_id == self.track_id => break p,
                Ok(Some(_)) => {}
                Ok(None) => return Ok(Step::End),
                Err(SymError::ResetRequired) => return Ok(Step::NewLink),
                // Media cut short (a stream that closed, a truncated file) ends here.
                Err(SymError::IoError(e)) if e.kind() == io::ErrorKind::UnexpectedEof => {
                    return Ok(Step::End)
                }
                Err(e) => return Err(from_symphonia(e)),
            }
        };
        let produced = match &mut self.engine {
            Engine::Symphonia(decoder) => match decoder.decode(&packet) {
                Ok(buffer) => {
                    self.scratch.clear();
                    buffer.copy_to_vec_interleaved::<f32>(&mut self.scratch);
                    if buffer.spec().channels().count() != channels {
                        return Err(DecodeError::Malformed(
                            "the channel count changed inside a stream".to_string(),
                        ));
                    }
                    self.scratch.len() / channels
                }
                // Skippable by Symphonia's contract: the packet is bad, the stream goes on.
                Err(SymError::DecodeError(_)) | Err(SymError::IoError(_)) => 0,
                Err(SymError::ResetRequired) => return Ok(Step::NewLink),
                Err(e) => return Err(from_symphonia(e)),
            },
            Engine::Opus(opus) => match opus.decoder.decode_s24(&packet.data, &mut opus.pcm) {
                Ok(frames) => {
                    // RFC 7845 section 4.4: the last page's granule position cuts the
                    // final packet; the demuxer hands that over as `trim_end`.
                    let trim = usize::try_from(packet.trim_end.get()).unwrap_or(usize::MAX);
                    let keep = frames.saturating_sub(trim);
                    self.scratch.clear();
                    let gain = opus.gain;
                    self.scratch
                        .extend(opus.pcm[..keep * channels].iter().map(|s| {
                            let exact = *s as f32 / 8_388_608.0;
                            // A gain of 0 dB leaves the samples libopus's own, bit for bit.
                            if gain == 1.0 {
                                exact
                            } else {
                                exact * gain
                            }
                        }));
                    keep
                }
                // A packet libopus refuses is skipped like any other bad packet.
                Err(_) => 0,
            },
        };
        let drop = match self.resume_at {
            None => {
                let drop = usize::try_from(self.skip)
                    .unwrap_or(usize::MAX)
                    .min(produced);
                self.skip -= drop as u64;
                drop
            }
            Some(resume_at) => {
                // The packet's valid frames end at its timestamp plus its duration,
                // and what the decoder gave are the last `produced` of them (a
                // decoder just reset may give fewer than the packet holds).
                let end_ticks = packet
                    .pts
                    .get()
                    .saturating_add(i64::try_from(packet.dur.get()).unwrap_or(i64::MAX));
                let end = self.frames_of(u64::try_from(end_ticks).unwrap_or(0));
                let first = end.saturating_sub(produced as u64);
                if end > resume_at {
                    self.resume_at = None;
                    self.skip = 0;
                }
                usize::try_from(resume_at.saturating_sub(first))
                    .unwrap_or(usize::MAX)
                    .min(produced)
            }
        };
        out.extend_from_slice(&self.scratch[drop * channels..produced * channels]);
        Ok(Step::Frames(produced - drop))
    }

    fn seek(&mut self, frame: u64) -> Result<u64, DecodeError> {
        // In an Opus stream the track's frame 0 is the first pre-skip frame.
        let (target, lossy) = match &self.engine {
            Engine::Opus(opus) => (frame + opus.pre_skip, true),
            Engine::Symphonia(decoder) => (frame, decoder.codec_params().bits_per_sample.is_none()),
        };
        // A lossy decoder needs a run-up to converge on what a straight decode
        // would have given; a lossless one needs none.
        let ask = if lossy {
            target.saturating_sub(SEEK_PREROLL)
        } else {
            target
        };
        self.reader
            .seek(
                SeekMode::Accurate,
                SeekTo::Timestamp {
                    ts: Timestamp::new(self.ticks(ask)),
                    track_id: self.track_id,
                },
            )
            .map_err(from_symphonia)?;
        self.resume_at = Some(target);
        match &mut self.engine {
            Engine::Symphonia(decoder) => decoder.reset(),
            Engine::Opus(opus) => {
                opus.decoder = chorus_opus_sys::Decoder::new(opus.channels, 0)
                    .map_err(|e| DecodeError::Malformed(format!("opus: {e}")))?;
            }
        }
        Ok(frame)
    }
}

impl RawL16 {
    fn read(&mut self, channels: usize, out: &mut Vec<f32>) -> Result<usize, DecodeError> {
        let frame_bytes = 2 * channels;
        loop {
            let n = match self.media.read(&mut self.bytes) {
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(DecodeError::Io(e)),
            };
            if n == 0 {
                // A trailing partial frame is not audio.
                return Ok(0);
            }
            self.carry.extend_from_slice(&self.bytes[..n]);
            let whole = self.carry.len() / frame_bytes * frame_bytes;
            if whole == 0 {
                continue;
            }
            out.extend(
                self.carry[..whole]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|b| f32::from(i16::from_be_bytes(*b)) / 32_768.0),
            );
            self.carry.drain(..whole);
            return Ok(whole / frame_bytes);
        }
    }
}

/// ADTS framing (AAC as radio streams and `.aac` files carry it): a 12-bit
/// sync word with the layer bits 00, where MPEG audio (MP3) never has 00
/// (ISO/IEC 13818-7's ADTS header as summarised in the MultimediaWiki page
/// "ADTS", <https://wiki.multimedia.cx/index.php/ADTS>, read 2026-10-03). Two
/// headers one frame length apart are asked for when the bytes reach that far,
/// so a stray 0xFFF in other data is not called AAC. An ID3v2 tag in front is
/// stepped over.
fn is_adts(head: &[u8]) -> bool {
    let mut at = 0usize;
    if head.len() >= 10 && &head[..3] == b"ID3" {
        let size = head[6..10]
            .iter()
            .fold(0usize, |acc, b| (acc << 7) | usize::from(b & 0x7f));
        at = 10 + size;
    }
    let header = |at: usize| -> Option<usize> {
        let h = head.get(at..at + 7)?;
        if h[0] != 0xff || h[1] & 0xf6 != 0xf0 {
            return None;
        }
        let length =
            (usize::from(h[3] & 0x03) << 11) | (usize::from(h[4]) << 3) | usize::from(h[5] >> 5);
        (length >= 7).then_some(length)
    };
    match header(at) {
        None => false,
        Some(length) => match head.get(at + length..at + length + 7) {
            Some(_) => header(at + length).is_some(),
            None => true,
        },
    }
}

/// Whether these leading bytes are an MP4 file whose `mdat` box (the audio)
/// comes before its `moov` box (the index a demuxer needs first). Walks the
/// top-level boxes: a 32-bit big-endian size and a four-character type each
/// (ISO/IEC 14496-12 section 4.2 as Apple's QuickTime File Format
/// documentation describes atoms, `ASSUMED` from memory, not re-read).
fn mp4_index_is_after_the_audio(head: &[u8]) -> bool {
    if head.len() < 12 || &head[4..8] != b"ftyp" {
        return false;
    }
    let mut at = 0usize;
    while let Some(h) = head.get(at..at + 8) {
        let size = u32::from_be_bytes([h[0], h[1], h[2], h[3]]) as usize;
        match &h[4..8] {
            b"moov" => return false,
            b"mdat" => return true,
            _ => {}
        }
        if size < 8 {
            // 0 (to the end of the file) and 1 (a 64-bit size) only make sense on
            // mdat, which returned above.
            return false;
        }
        at = at.saturating_add(size);
    }
    // The walk ran out of sniffed bytes without meeting either: let the demuxer say.
    false
}

fn hint_says_aac(hint: &Hint) -> bool {
    let mime = hint
        .mime
        .as_deref()
        .map(|m| m.split(';').next().unwrap_or(m).trim().to_ascii_lowercase());
    let ext = hint.extension.as_deref().map(str::to_ascii_lowercase);
    matches!(
        mime.as_deref(),
        Some("audio/aac" | "audio/aacp" | "audio/x-aac")
    ) || matches!(ext.as_deref(), Some("aac" | "adts"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adts_is_recognised_and_mp3_is_not() {
        // An ADTS header: sync 0xFFF, MPEG-4, layer 00, no CRC; frame length 9.
        let mut adts = vec![0xff, 0xf1, 0x50, 0x80, 0x01, 0x3f, 0xfc, 0, 0];
        adts.extend_from_slice(&[0xff, 0xf1, 0x50, 0x80, 0x01, 0x3f, 0xfc]);
        assert!(is_adts(&adts));
        // One header and then other bytes where the next should be: not ADTS.
        let mut stray = adts.clone();
        stray[9] = 0x00;
        assert!(!is_adts(&stray));
        // MPEG-1 Layer III: layer bits 01.
        assert!(!is_adts(&[0xff, 0xfb, 0x90, 0x00, 0, 0, 0, 0]));
        assert!(!is_adts(b"RIFF\0\0\0\0WAVE"));
        // Behind an ID3v2 tag of 4 bytes.
        let mut tagged = b"ID3\x04\0\0\0\0\0\x04abcd".to_vec();
        tagged.extend_from_slice(&adts);
        assert!(is_adts(&tagged));
    }

    #[test]
    fn mp4_box_order_is_read_from_the_leading_bytes() {
        let ftyp = b"\0\0\0\x10ftypM4A \0\0\0\0";
        let mut last = ftyp.to_vec();
        last.extend_from_slice(b"\0\0\0\x08free\0\0\x10\0mdat");
        assert!(mp4_index_is_after_the_audio(&last));
        let mut first = ftyp.to_vec();
        first.extend_from_slice(b"\0\0\0\x08moov\0\0\x10\0mdat");
        assert!(!mp4_index_is_after_the_audio(&first));
        assert!(!mp4_index_is_after_the_audio(b"OggS and so on"));
    }

    #[test]
    fn opus_head_names_what_it_refuses() {
        let mut head = b"OpusHead\x01\x02\x38\x01\x80\xbb\0\0\0\0\0".to_vec();
        let parsed = parse_opus_head(Some(&head)).expect("family 0 stereo");
        assert_eq!(
            (parsed.channels, parsed.pre_skip, parsed.gain_q8),
            (2, 312, 0)
        );
        head[18] = 1;
        match parse_opus_head(Some(&head)) {
            Err(DecodeError::Unsupported(what)) => assert_eq!(what, "opus mapping family 1"),
            _ => panic!("family 1 must be refused by name"),
        }
        head[18] = 255;
        assert_eq!(
            parse_opus_head(Some(&head))
                .err()
                .map(|e| e.to_string())
                .as_deref(),
            Some("unsupported: opus mapping family 255")
        );
    }

    #[test]
    fn errors_read_as_their_kind_and_what() {
        assert_eq!(
            DecodeError::Unsupported("aac".into()).to_string(),
            "unsupported: aac"
        );
        assert_eq!(
            DecodeError::Malformed("x".into()).to_string(),
            "malformed: x"
        );
    }
}
