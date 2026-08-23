//! What a stream is, and which streams the server refuses.
//!
//! A format is refused at start, by name and by number, rather than being
//! guessed at. The alternative - interpreting bytes under an assumed format -
//! produces sound, which is what makes it dangerous: a run that is wrong is
//! indistinguishable from a run that is right until someone listens.

use std::fmt;

use chorus_protocol::{SampleFormat, MAX_CHANNELS, MAX_SAMPLE_RATE_HZ, MIN_SAMPLE_RATE_HZ};

/// The sample formats this phase's server supports end to end.
///
/// `docs/protocol.md` defines three; the wire carries all three and the
/// chunker is format-agnostic beyond the frame size, so all three are
/// supported. Which one a run uses is configuration.
pub const SUPPORTED_FORMATS: [&str; 3] = ["pcm_s16le", "pcm_s24le", "pcm_f32le"];

/// A PCM stream's shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamFormat {
    /// Frames per second.
    pub sample_rate_hz: u32,
    /// Channels per frame.
    pub channels: u16,
    /// Layout of one sample.
    pub sample_format: SampleFormat,
}

/// Why a stream format was refused.
///
/// Typed rather than a string so the caller can exit on it without matching
/// on prose, and so the message always names the offending value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnsupportedFormat {
    /// A sample format name that is not in the catalog.
    SampleFormat {
        /// The name that was configured or declared.
        name: String,
        /// The names that would have been accepted.
        supported: &'static [&'static str; 3],
    },
    /// A sample rate outside the band `docs/protocol.md` defines.
    SampleRate {
        /// The rate that was configured or declared.
        hz: u32,
        /// Lowest accepted rate.
        min_hz: u32,
        /// Highest accepted rate.
        max_hz: u32,
    },
    /// A channel count outside the band `docs/protocol.md` defines.
    Channels {
        /// The count that was configured or declared.
        channels: u32,
        /// Highest accepted count.
        max: u16,
    },
    /// A chunk duration that is not a whole number of frames at this rate, or
    /// is zero.
    ChunkDuration {
        /// The duration that was configured, in microseconds.
        chunk_us: u64,
        /// The rate it was checked against.
        sample_rate_hz: u32,
    },
    /// A chunk that would not fit in one protocol frame.
    ChunkTooLarge {
        /// Bytes the chunk payload would need.
        needed: usize,
        /// Bytes a frame payload can carry.
        max: usize,
    },
}

impl fmt::Display for UnsupportedFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnsupportedFormat::SampleFormat { name, supported } => write!(
                f,
                "unsupported sample format '{}': this server supports {}",
                name,
                supported.join(", ")
            ),
            UnsupportedFormat::SampleRate { hz, min_hz, max_hz } => write!(
                f,
                "unsupported sample rate {} Hz: this server supports {} to {} Hz",
                hz, min_hz, max_hz
            ),
            UnsupportedFormat::Channels { channels, max } => write!(
                f,
                "unsupported channel count {}: this server supports 1 to {}",
                channels, max
            ),
            UnsupportedFormat::ChunkDuration {
                chunk_us,
                sample_rate_hz,
            } => write!(
                f,
                "chunk duration {} us is not a whole number of frames at {} Hz",
                chunk_us, sample_rate_hz
            ),
            UnsupportedFormat::ChunkTooLarge { needed, max } => write!(
                f,
                "a chunk of {} bytes does not fit a protocol frame payload of {} bytes",
                needed, max
            ),
        }
    }
}

impl std::error::Error for UnsupportedFormat {}

impl StreamFormat {
    /// Build a stream format from configured values, refusing anything the
    /// protocol or this server does not carry.
    ///
    /// Every refusal names the value that was refused. Nothing is coerced,
    /// rounded or assumed.
    pub fn new(
        sample_rate_hz: u32,
        channels: u32,
        sample_format_name: &str,
    ) -> Result<StreamFormat, UnsupportedFormat> {
        let sample_format = SampleFormat::from_name(sample_format_name).ok_or_else(|| {
            UnsupportedFormat::SampleFormat {
                name: sample_format_name.to_string(),
                supported: &SUPPORTED_FORMATS,
            }
        })?;
        if !(MIN_SAMPLE_RATE_HZ..=MAX_SAMPLE_RATE_HZ).contains(&sample_rate_hz) {
            return Err(UnsupportedFormat::SampleRate {
                hz: sample_rate_hz,
                min_hz: MIN_SAMPLE_RATE_HZ,
                max_hz: MAX_SAMPLE_RATE_HZ,
            });
        }
        if channels == 0 || channels > u32::from(MAX_CHANNELS) {
            return Err(UnsupportedFormat::Channels {
                channels,
                max: MAX_CHANNELS,
            });
        }
        Ok(StreamFormat {
            sample_rate_hz,
            channels: channels as u16,
            sample_format,
        })
    }

    /// Bytes one frame (one sample on every channel) occupies.
    pub fn frame_len(&self) -> usize {
        self.channels as usize * self.sample_format.bytes_per_sample()
    }

    /// Frames in `chunk_us` microseconds of this stream, if that is a whole
    /// number.
    ///
    /// A chunk duration that is not a whole number of frames is refused rather
    /// than rounded, because rounding it would make the constant timestamp
    /// delta of the chunker a lie by a fraction of a frame per chunk.
    pub fn frames_in(&self, chunk_us: u64) -> Result<usize, UnsupportedFormat> {
        if chunk_us == 0 {
            return Err(UnsupportedFormat::ChunkDuration {
                chunk_us,
                sample_rate_hz: self.sample_rate_hz,
            });
        }
        let numerator = chunk_us * u64::from(self.sample_rate_hz);
        if numerator % 1_000_000 != 0 {
            return Err(UnsupportedFormat::ChunkDuration {
                chunk_us,
                sample_rate_hz: self.sample_rate_hz,
            });
        }
        let frames = numerator / 1_000_000;
        if frames == 0 {
            return Err(UnsupportedFormat::ChunkDuration {
                chunk_us,
                sample_rate_hz: self.sample_rate_hz,
            });
        }
        Ok(frames as usize)
    }

    /// Microseconds `frames` frames of this stream occupy.
    ///
    /// Exact whenever `frames` is a whole number of frames at a rate that
    /// divides a microsecond grid; otherwise truncated toward zero, which is
    /// the conservative direction for a buffer bound.
    pub fn frames_to_us(&self, frames: u64) -> u64 {
        frames * 1_000_000 / u64::from(self.sample_rate_hz)
    }

    /// Nanoseconds `frames` frames of this stream occupy.
    pub fn frames_to_ns(&self, frames: u64) -> u64 {
        frames * 1_000_000_000 / u64::from(self.sample_rate_hz)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_format_name_is_refused_by_name() {
        let err = StreamFormat::new(48_000, 2, "pcm_s20le").unwrap_err();
        assert_eq!(
            err,
            UnsupportedFormat::SampleFormat {
                name: "pcm_s20le".to_string(),
                supported: &SUPPORTED_FORMATS,
            }
        );
        assert!(err.to_string().contains("pcm_s20le"));
    }

    #[test]
    fn a_rate_outside_the_band_is_refused_with_the_band() {
        let err = StreamFormat::new(4_000, 2, "pcm_s16le").unwrap_err();
        assert_eq!(
            err,
            UnsupportedFormat::SampleRate {
                hz: 4_000,
                min_hz: MIN_SAMPLE_RATE_HZ,
                max_hz: MAX_SAMPLE_RATE_HZ,
            }
        );
        assert!(err.to_string().contains("4000"));
    }

    #[test]
    fn a_channel_count_outside_the_band_is_refused() {
        assert!(StreamFormat::new(48_000, 0, "pcm_s16le").is_err());
        assert!(StreamFormat::new(48_000, 9, "pcm_s16le").is_err());
        assert!(StreamFormat::new(48_000, 8, "pcm_s16le").is_ok());
    }

    #[test]
    fn a_chunk_duration_that_is_not_a_whole_number_of_frames_is_refused() {
        let f = StreamFormat::new(44_100, 2, "pcm_s16le").unwrap();
        assert_eq!(f.frames_in(20_000).unwrap(), 882);
        // 44100 Hz has no whole frame count in 1 us.
        assert!(f.frames_in(1).is_err());
        assert!(f.frames_in(0).is_err());
    }

    #[test]
    fn frame_length_follows_the_protocol_definition() {
        let f = StreamFormat::new(48_000, 2, "pcm_s24le").unwrap();
        assert_eq!(f.frame_len(), 6);
        assert_eq!(f.frames_in(20_000).unwrap(), 960);
        assert_eq!(f.frames_to_us(960), 20_000);
        assert_eq!(f.frames_to_ns(960), 20_000_000);
    }
}
