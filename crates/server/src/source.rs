//! Where the PCM comes from.
//!
//! Two sources, both of them local, because this phase is about what happens
//! to bytes after they arrive rather than about where they came from:
//!
//! - a **file** of raw interleaved PCM in the configured format, which is what
//!   the chunking verifications feed, because a file of known content is the
//!   only way to assert that the chunks concatenated are the input;
//! - a **generated tone**, which is what the ten-minute run plays, because it
//!   is endless, audible, and obviously wrong when it is wrong.
//!
//! A source **ends cleanly** when it has delivered its last byte and closed
//! without error. That is the only thing that earns an end-of-stream signal; a
//! source that failed mid-read did not end, it broke, and the client is told
//! the difference.

use std::fs::File;
use std::io::{self, Read};

use chorus_audio::StreamFormat;

/// Something that produces PCM.
pub trait PcmSource: Send {
    /// Fill as much of `buf` as is available. `Ok(0)` is the end.
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;

    /// What this source is, for the report.
    fn describe(&self) -> String;
}

/// A file of raw interleaved PCM.
pub struct FileSource {
    path: String,
    file: File,
}

impl FileSource {
    /// Open `path`.
    pub fn open(path: &str) -> io::Result<FileSource> {
        Ok(FileSource {
            path: path.to_string(),
            file: File::open(path)?,
        })
    }
}

impl PcmSource for FileSource {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.file.read(buf)
    }

    fn describe(&self) -> String {
        format!("file {}", self.path)
    }
}

/// A generated tone.
///
/// A quiet 440 Hz sine, computed from a frame counter rather than from a
/// clock, so the samples are a pure function of how many frames have been
/// asked for. That matters: a tone that depended on when it was asked would
/// make a chunking test depend on scheduling.
pub struct ToneSource {
    format: StreamFormat,
    frames_remaining: Option<u64>,
    frame_index: u64,
}

impl ToneSource {
    /// A tone in `format`, lasting `ms` milliseconds, or endless when `ms` is
    /// zero.
    pub fn new(format: StreamFormat, ms: u64) -> ToneSource {
        let frames_remaining = if ms == 0 {
            None
        } else {
            Some(ms * u64::from(format.sample_rate_hz) / 1_000)
        };
        ToneSource {
            format,
            frames_remaining,
            frame_index: 0,
        }
    }

    fn sample_at(&self, frame: u64) -> f32 {
        let t = frame as f64 / f64::from(self.format.sample_rate_hz);
        (0.2 * (2.0 * std::f64::consts::PI * 440.0 * t).sin()) as f32
    }

    fn write_frame(&self, frame: u64, out: &mut [u8]) {
        let value = self.sample_at(frame);
        let bytes_per_sample = self.format.sample_format.bytes_per_sample();
        for channel in 0..self.format.channels as usize {
            let at = channel * bytes_per_sample;
            match bytes_per_sample {
                2 => {
                    let v = (value * i16::MAX as f32) as i16;
                    out[at..at + 2].copy_from_slice(&v.to_le_bytes());
                }
                3 => {
                    let v = (value * 8_388_607.0) as i32;
                    out[at..at + 3].copy_from_slice(&v.to_le_bytes()[..3]);
                }
                _ => {
                    out[at..at + 4].copy_from_slice(&value.to_le_bytes());
                }
            }
        }
    }
}

impl PcmSource for ToneSource {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let frame_len = self.format.frame_len();
        let mut frames_wanted = (buf.len() / frame_len) as u64;
        if let Some(left) = self.frames_remaining {
            frames_wanted = frames_wanted.min(left);
        }
        if frames_wanted == 0 {
            return Ok(0);
        }
        for i in 0..frames_wanted as usize {
            let at = i * frame_len;
            self.write_frame(self.frame_index + i as u64, &mut buf[at..at + frame_len]);
        }
        self.frame_index += frames_wanted;
        if let Some(left) = self.frames_remaining.as_mut() {
            *left -= frames_wanted;
        }
        Ok(frames_wanted as usize * frame_len)
    }

    fn describe(&self) -> String {
        match self.frames_remaining {
            None => "generated 440 Hz tone, endless".to_string(),
            Some(_) => format!(
                "generated 440 Hz tone, {} frames",
                self.frames_remaining.unwrap_or(0) + self.frame_index
            ),
        }
    }
}

/// Build the configured source.
pub fn open(source: &str, format: StreamFormat, tone_ms: u64) -> io::Result<Box<dyn PcmSource>> {
    if source == "tone" {
        Ok(Box::new(ToneSource::new(format, tone_ms)))
    } else {
        Ok(Box::new(FileSource::open(source)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bounded_tone_ends_after_exactly_its_frames() {
        let format = StreamFormat::new(48_000, 2, "pcm_s16le").unwrap();
        let mut tone = ToneSource::new(format, 10);
        let mut buf = vec![0u8; 4096];
        let mut total = 0usize;
        loop {
            let n = tone.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            total += n;
        }
        assert_eq!(total, 480 * 4, "10 ms at 48 kHz is 480 frames");
    }

    #[test]
    fn the_tone_is_a_pure_function_of_the_frame_index() {
        let format = StreamFormat::new(48_000, 2, "pcm_s16le").unwrap();
        let mut a = ToneSource::new(format, 5);
        let mut b = ToneSource::new(format, 5);
        let mut buf_a = vec![0u8; 4096];
        let mut buf_b = vec![0u8; 64];
        let n = a.read(&mut buf_a).unwrap();
        let mut collected = Vec::new();
        loop {
            let m = b.read(&mut buf_b).unwrap();
            if m == 0 {
                break;
            }
            collected.extend_from_slice(&buf_b[..m]);
        }
        assert_eq!(&buf_a[..n], &collected[..]);
    }

    #[test]
    fn an_endless_tone_never_returns_zero() {
        let format = StreamFormat::new(48_000, 2, "pcm_s16le").unwrap();
        let mut tone = ToneSource::new(format, 0);
        let mut buf = vec![0u8; 512];
        for _ in 0..100 {
            assert_eq!(tone.read(&mut buf).unwrap(), 512);
        }
    }
}
