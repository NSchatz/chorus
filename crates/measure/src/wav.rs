//! Reading and writing the two-channel PCM recordings this rig analyses.
//!
//! Hand-rolled, for the reason `docs/decisions/0002-repository-layout-and-ci.md`
//! gives: this repository carries zero third-party dependencies and no
//! lockfile, and a RIFF/WAVE reader that accepts exactly one layout is small
//! and instructive. What it accepts is deliberately narrow - 16-bit signed
//! little-endian PCM, two channels, at a rate the run declares - because a
//! reader that quietly coerced something else would turn a wrong capture into a
//! plausible number, which is the failure this whole harness exists to prevent.
//!
//! Every refusal names what it read AND what it required. An error that says
//! only "bad file" leaves an operator with a capture interface and no idea
//! which knob is wrong.

use std::fmt;
use std::path::{Path, PathBuf};

/// The channel count a capture of two endpoint line outputs has to carry: one
/// endpoint per channel.
pub const REQUIRED_CHANNELS: u16 = 2;

/// The sample width this reader accepts.
pub const REQUIRED_BITS_PER_SAMPLE: u16 = 16;

/// `WAVE_FORMAT_PCM`, the only format tag this reader accepts.
pub const WAVE_FORMAT_PCM: u16 = 1;

/// `WAVE_FORMAT_IEEE_FLOAT`, named so a refusal can say what it read rather
/// than printing a bare number.
pub const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;

/// `WAVE_FORMAT_EXTENSIBLE`.
pub const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;

/// The name of a format tag, for a refusal that has to be readable.
pub fn format_tag_name(tag: u16) -> String {
    match tag {
        WAVE_FORMAT_PCM => "WAVE_FORMAT_PCM".to_string(),
        WAVE_FORMAT_IEEE_FLOAT => "WAVE_FORMAT_IEEE_FLOAT".to_string(),
        WAVE_FORMAT_EXTENSIBLE => "WAVE_FORMAT_EXTENSIBLE".to_string(),
        other => format!("format tag {}", other),
    }
}

/// Why a capture could not be read.
///
/// Each variant carries both halves of the sentence an operator needs: what
/// the file actually holds, and what this rig required of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureError {
    /// The file could not be opened or read at all.
    Unreadable {
        /// The path that was tried.
        path: PathBuf,
        /// What the operating system said.
        detail: String,
    },
    /// The file is not a RIFF/WAVE container.
    NotRiffWave {
        /// The path that was read.
        path: PathBuf,
        /// The first bytes, as far as they are printable.
        read: String,
    },
    /// A chunk this reader needs is not in the file.
    ChunkMissing {
        /// The path that was read.
        path: PathBuf,
        /// The chunk that is absent.
        chunk: &'static str,
    },
    /// The recording does not carry one channel per endpoint.
    WrongChannelCount {
        /// The path that was read.
        path: PathBuf,
        /// Channels the file declares.
        read: u16,
        /// Channels this rig requires.
        required: u16,
    },
    /// The sample layout is not one this reader accepts.
    UnsupportedSampleFormat {
        /// The path that was read.
        path: PathBuf,
        /// The layout the file declares.
        read: String,
        /// The layout this rig requires.
        required: String,
    },
    /// The file's sample rate is not the one the run declared.
    SampleRateMismatch {
        /// The path that was read.
        path: PathBuf,
        /// The rate the file declares.
        read: u32,
        /// The rate the run declared.
        declared: u32,
    },
    /// The data chunk claims more bytes than the file holds.
    TruncatedBody {
        /// The path that was read.
        path: PathBuf,
        /// Bytes the data chunk header declares.
        declared_bytes: u64,
        /// Bytes actually present after that header.
        present_bytes: u64,
    },
    /// The data chunk holds no whole frame.
    NoFrames {
        /// The path that was read.
        path: PathBuf,
    },
    /// The container is malformed in a way none of the above names.
    Malformed {
        /// The path that was read.
        path: PathBuf,
        /// What is wrong with it.
        detail: String,
    },
}

impl CaptureError {
    /// The path this error is about.
    pub fn path(&self) -> &Path {
        match self {
            CaptureError::Unreadable { path, .. }
            | CaptureError::NotRiffWave { path, .. }
            | CaptureError::ChunkMissing { path, .. }
            | CaptureError::WrongChannelCount { path, .. }
            | CaptureError::UnsupportedSampleFormat { path, .. }
            | CaptureError::SampleRateMismatch { path, .. }
            | CaptureError::TruncatedBody { path, .. }
            | CaptureError::NoFrames { path }
            | CaptureError::Malformed { path, .. } => path,
        }
    }

    /// A short, stable token naming which refusal this is, for a script that
    /// has to grade a run without parsing prose.
    pub fn condition(&self) -> &'static str {
        match self {
            CaptureError::Unreadable { .. } => "unreadable",
            CaptureError::NotRiffWave { .. } => "not-riff-wave",
            CaptureError::ChunkMissing { .. } => "chunk-missing",
            CaptureError::WrongChannelCount { .. } => "wrong-channel-count",
            CaptureError::UnsupportedSampleFormat { .. } => "unsupported-sample-format",
            CaptureError::SampleRateMismatch { .. } => "sample-rate-mismatch",
            CaptureError::TruncatedBody { .. } => "truncated-body",
            CaptureError::NoFrames { .. } => "no-frames",
            CaptureError::Malformed { .. } => "malformed",
        }
    }
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CaptureError::Unreadable { path, detail } => {
                write!(f, "capture '{}' could not be read: {}", path.display(), detail)
            }
            CaptureError::NotRiffWave { path, read } => write!(
                f,
                "capture '{}' is not a RIFF/WAVE recording: it starts with '{}' and a \
                 RIFF....WAVE header was required",
                path.display(),
                read
            ),
            CaptureError::ChunkMissing { path, chunk } => write!(
                f,
                "capture '{}' carries no '{}' chunk, which a WAVE recording requires",
                path.display(),
                chunk
            ),
            CaptureError::WrongChannelCount {
                path,
                read,
                required,
            } => write!(
                f,
                "capture '{}' carries {} channel(s) and {} were required, one per endpoint \
                 line output",
                path.display(),
                read,
                required
            ),
            CaptureError::UnsupportedSampleFormat {
                path,
                read,
                required,
            } => write!(
                f,
                "capture '{}' is {} and {} was required",
                path.display(),
                read,
                required
            ),
            CaptureError::SampleRateMismatch {
                path,
                read,
                declared,
            } => write!(
                f,
                "capture '{}' was recorded at {} Hz and the run declared {} Hz",
                path.display(),
                read,
                declared
            ),
            CaptureError::TruncatedBody {
                path,
                declared_bytes,
                present_bytes,
            } => write!(
                f,
                "capture '{}' has a truncated body: its data chunk declares {} bytes and \
                 {} bytes are present",
                path.display(),
                declared_bytes,
                present_bytes
            ),
            CaptureError::NoFrames { path } => write!(
                f,
                "capture '{}' holds no whole frame, and at least one was required",
                path.display()
            ),
            CaptureError::Malformed { path, detail } => {
                write!(f, "capture '{}' is malformed: {}", path.display(), detail)
            }
        }
    }
}

impl std::error::Error for CaptureError {}

/// A two-channel recording, de-interleaved and scaled to `[-1, 1)`.
#[derive(Debug, Clone, PartialEq)]
pub struct Capture {
    /// Where it came from.
    pub path: PathBuf,
    /// The rate it was recorded at, which equals the rate the run declared.
    pub sample_rate_hz: u32,
    /// The first captured output. The sign convention in [`crate::lag`] is
    /// stated relative to this channel.
    pub a: Vec<f64>,
    /// The second captured output.
    pub b: Vec<f64>,
}

impl Capture {
    /// Frames in the recording.
    pub fn frames(&self) -> usize {
        self.a.len()
    }

    /// How long the recording runs, in microseconds.
    pub fn duration_us(&self) -> f64 {
        self.frames() as f64 * 1_000_000.0 / f64::from(self.sample_rate_hz)
    }
}

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// Read a capture from disk and hold it to the rate the run declared.
pub fn read_capture(path: &Path, declared_rate_hz: u32) -> Result<Capture, CaptureError> {
    let bytes = std::fs::read(path).map_err(|e| CaptureError::Unreadable {
        path: path.to_path_buf(),
        detail: e.to_string(),
    })?;
    parse_capture(path, &bytes, declared_rate_hz)
}

/// The `fmt ` chunk's fields, as read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FormatChunk {
    tag: u16,
    channels: u16,
    sample_rate_hz: u32,
    bits_per_sample: u16,
}

/// Parse a capture already in memory.
///
/// Split from [`read_capture`] so a test can hand it bytes it built itself,
/// which is how the malformed cases are exercised without a file for every
/// one of them.
pub fn parse_capture(
    path: &Path,
    bytes: &[u8],
    declared_rate_hz: u32,
) -> Result<Capture, CaptureError> {
    let path = path.to_path_buf();
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        let head: String = bytes
            .iter()
            .take(12)
            .map(|b| {
                if b.is_ascii_graphic() {
                    char::from(*b)
                } else {
                    '.'
                }
            })
            .collect();
        return Err(CaptureError::NotRiffWave { path, read: head });
    }

    let mut format: Option<FormatChunk> = None;
    let mut data: Option<(usize, u64)> = None;
    let mut at = 12usize;
    while at + 8 <= bytes.len() {
        let id = [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]];
        let size = u64::from(u32_at(bytes, at + 4));
        let body = at + 8;
        let present = (bytes.len() - body) as u64;
        match &id {
            b"fmt " => {
                if size < 16 || present < 16 {
                    return Err(CaptureError::Malformed {
                        path,
                        detail: format!(
                            "its 'fmt ' chunk declares {} bytes and a PCM one needs at least 16",
                            size
                        ),
                    });
                }
                format = Some(FormatChunk {
                    tag: u16_at(bytes, body),
                    channels: u16_at(bytes, body + 2),
                    sample_rate_hz: u32_at(bytes, body + 4),
                    bits_per_sample: u16_at(bytes, body + 14),
                });
            }
            b"data" => {
                if size > present {
                    return Err(CaptureError::TruncatedBody {
                        path,
                        declared_bytes: size,
                        present_bytes: present,
                    });
                }
                data = Some((body, size));
                break;
            }
            _ => {}
        }
        if size > present {
            return Err(CaptureError::Malformed {
                path,
                detail: format!(
                    "its '{}' chunk declares {} bytes and only {} are present",
                    String::from_utf8_lossy(&id),
                    size,
                    present
                ),
            });
        }
        // RIFF chunks are word aligned: an odd body carries one pad byte.
        at = body + size as usize + (size % 2) as usize;
    }

    let format = format.ok_or(CaptureError::ChunkMissing {
        path: path.clone(),
        chunk: "fmt ",
    })?;
    let (data_at, data_len) = data.ok_or(CaptureError::ChunkMissing {
        path: path.clone(),
        chunk: "data",
    })?;

    // The order below is the order an operator would want to be told about:
    // the container's shape first, then the rate the run declared.
    if format.channels != REQUIRED_CHANNELS {
        return Err(CaptureError::WrongChannelCount {
            path,
            read: format.channels,
            required: REQUIRED_CHANNELS,
        });
    }
    if format.tag != WAVE_FORMAT_PCM || format.bits_per_sample != REQUIRED_BITS_PER_SAMPLE {
        return Err(CaptureError::UnsupportedSampleFormat {
            path,
            read: format!(
                "{} at {} bits per sample",
                format_tag_name(format.tag),
                format.bits_per_sample
            ),
            required: format!(
                "{} at {} bits per sample",
                format_tag_name(WAVE_FORMAT_PCM),
                REQUIRED_BITS_PER_SAMPLE
            ),
        });
    }
    if format.sample_rate_hz != declared_rate_hz {
        return Err(CaptureError::SampleRateMismatch {
            path,
            read: format.sample_rate_hz,
            declared: declared_rate_hz,
        });
    }

    let frame_bytes = usize::from(REQUIRED_CHANNELS) * 2;
    let frames = data_len as usize / frame_bytes;
    if frames == 0 {
        return Err(CaptureError::NoFrames { path });
    }

    let mut a = Vec::with_capacity(frames);
    let mut b = Vec::with_capacity(frames);
    const SCALE: f64 = 1.0 / 32768.0;
    for frame in 0..frames {
        let at = data_at + frame * frame_bytes;
        a.push(f64::from(u16_at(bytes, at) as i16) * SCALE);
        b.push(f64::from(u16_at(bytes, at + 2) as i16) * SCALE);
    }

    Ok(Capture {
        path,
        sample_rate_hz: format.sample_rate_hz,
        a,
        b,
    })
}

/// Render interleaved 16-bit samples into a RIFF/WAVE container.
///
/// `channels` and `format_tag` are arguments rather than constants because the
/// committed degenerate fixtures are exactly the containers this rig has to
/// refuse, and a writer that could only produce acceptable files could not
/// produce them.
pub fn write_wav(
    samples: &[i16],
    channels: u16,
    sample_rate_hz: u32,
    bits_per_sample: u16,
    format_tag: u16,
) -> Vec<u8> {
    let bytes_per_sample = u32::from(bits_per_sample) / 8;
    let block_align = u32::from(channels) * bytes_per_sample;
    let byte_rate = sample_rate_hz * block_align;
    let data_len = (samples.len() * 2) as u32;

    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&format_tag.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate_hz.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&(block_align as u16).to_le_bytes());
    out.extend_from_slice(&bits_per_sample.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

/// Render interleaved 32-bit floats into a RIFF/WAVE container.
///
/// Only the committed `unsupported-format` fixture needs this: it exists so
/// that the refusal in front of an unsupported layout is graded against a real
/// container rather than a hand-poked byte.
pub fn write_wav_f32(samples: &[f32], channels: u16, sample_rate_hz: u32) -> Vec<u8> {
    let block_align = u32::from(channels) * 4;
    let byte_rate = sample_rate_hz * block_align;
    let data_len = (samples.len() * 4) as u32;

    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&WAVE_FORMAT_IEEE_FLOAT.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate_hz.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&(block_align as u16).to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

/// Clamp a `[-1, 1]` sample to the 16-bit range without wrapping.
///
/// Saturation rather than a cast, because a cast that wraps turns an
/// over-level sample into a full-scale sample of the opposite sign, which
/// sounds like a click and reads like data.
pub fn to_i16(sample: f64) -> i16 {
    let scaled = sample * 32767.0;
    if scaled >= 32767.0 {
        i16::MAX
    } else if scaled <= -32768.0 {
        i16::MIN
    } else {
        scaled.round() as i16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_capture(frames: usize) -> Vec<u8> {
        let mut samples = Vec::new();
        for n in 0..frames {
            samples.push((n as i16).wrapping_mul(37));
            samples.push((n as i16).wrapping_mul(-11));
        }
        write_wav(&samples, 2, 96_000, 16, WAVE_FORMAT_PCM)
    }

    #[test]
    fn a_two_channel_pcm_capture_round_trips() {
        let bytes = a_capture(8);
        let capture = parse_capture(Path::new("mem"), &bytes, 96_000).unwrap();
        assert_eq!(capture.frames(), 8);
        assert_eq!(capture.sample_rate_hz, 96_000);
        assert!((capture.a[1] - 37.0 / 32768.0).abs() < 1e-12);
        assert!((capture.b[1] + 11.0 / 32768.0).abs() < 1e-12);
    }

    #[test]
    fn a_file_that_is_not_riff_is_refused_by_name() {
        let err = parse_capture(Path::new("mem"), b"not a wave file at all", 96_000).unwrap_err();
        assert_eq!(err.condition(), "not-riff-wave");
        assert!(err.to_string().contains("RIFF"));
    }

    #[test]
    fn a_mono_capture_names_what_it_read_and_what_was_required() {
        let bytes = write_wav(&[1, 2, 3, 4], 1, 96_000, 16, WAVE_FORMAT_PCM);
        let err = parse_capture(Path::new("mem"), &bytes, 96_000).unwrap_err();
        assert_eq!(err.condition(), "wrong-channel-count");
        let said = err.to_string();
        assert!(said.contains("1 channel"), "{}", said);
        assert!(said.contains("2 were required"), "{}", said);
    }

    #[test]
    fn a_float_capture_names_both_layouts() {
        let bytes = write_wav_f32(&[0.0, 0.0, 0.5, -0.5], 2, 96_000);
        let err = parse_capture(Path::new("mem"), &bytes, 96_000).unwrap_err();
        assert_eq!(err.condition(), "unsupported-sample-format");
        let said = err.to_string();
        assert!(said.contains("WAVE_FORMAT_IEEE_FLOAT"), "{}", said);
        assert!(said.contains("WAVE_FORMAT_PCM"), "{}", said);
    }

    #[test]
    fn a_truncated_body_names_both_byte_counts() {
        let mut bytes = a_capture(64);
        bytes.truncate(bytes.len() - 100);
        let err = parse_capture(Path::new("mem"), &bytes, 96_000).unwrap_err();
        assert_eq!(err.condition(), "truncated-body");
        let said = err.to_string();
        assert!(said.contains("256 bytes"), "{}", said);
        assert!(said.contains("156 bytes"), "{}", said);
    }

    #[test]
    fn a_rate_the_run_did_not_declare_is_refused() {
        let bytes = a_capture(8);
        let err = parse_capture(Path::new("mem"), &bytes, 48_000).unwrap_err();
        assert_eq!(err.condition(), "sample-rate-mismatch");
        let said = err.to_string();
        assert!(said.contains("96000"), "{}", said);
        assert!(said.contains("48000"), "{}", said);
    }

    #[test]
    fn an_empty_data_chunk_is_refused_rather_than_analysed() {
        let bytes = write_wav(&[], 2, 96_000, 16, WAVE_FORMAT_PCM);
        let err = parse_capture(Path::new("mem"), &bytes, 96_000).unwrap_err();
        assert_eq!(err.condition(), "no-frames");
    }

    #[test]
    fn a_chunk_before_the_data_one_is_skipped_rather_than_tripped_over() {
        // Real capture interfaces write LIST/INFO chunks. Skipping an unknown
        // chunk is the difference between reading a real capture and refusing
        // one for a reason that has nothing to do with the audio.
        let mut bytes = a_capture(4);
        let mut with_list = bytes.drain(..12).collect::<Vec<u8>>();
        with_list.extend_from_slice(b"LIST");
        with_list.extend_from_slice(&5u32.to_le_bytes());
        with_list.extend_from_slice(b"INFOx");
        with_list.push(0); // the pad byte an odd chunk carries
        with_list.extend_from_slice(&bytes);
        let riff_len = (with_list.len() - 8) as u32;
        with_list[4..8].copy_from_slice(&riff_len.to_le_bytes());
        let capture = parse_capture(Path::new("mem"), &with_list, 96_000).unwrap();
        assert_eq!(capture.frames(), 4);
    }

    #[test]
    fn an_over_level_sample_saturates_rather_than_wrapping() {
        assert_eq!(to_i16(2.0), i16::MAX);
        assert_eq!(to_i16(-2.0), i16::MIN);
        assert_eq!(to_i16(0.0), 0);
    }
}
