//! Where frames go, and what the device says back.
//!
//! The client talks to exactly one thing through this trait: something that
//! accepts interleaved PCM and reports how far the frames it holds are from
//! the DAC. The shipped client constructs exactly one implementation,
//! [`AlsaSink`], in one place, with no configuration reaching the choice: a
//! run either plays through a real audio device or exits non-zero. There is no
//! "pretend" sink behind a flag, because a client that reports itself as
//! playing while producing no audio is the failure this phase is meant to make
//! impossible.
//!
//! The trait exists so that the buffering and accounting can be driven by a
//! modelled device in the test suite. That model lives under `tests/`, not
//! here, so it cannot be reached from the binary at all.

use std::fmt;

use chorus_alsa::{AlsaError, Format, Pcm};
use chorus_protocol::SampleFormat;

/// What a sink reported about one write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SinkWrite {
    /// Frames the device accepted.
    pub frames_written: u64,
    /// Whether the device signalled an underrun during this write.
    ///
    /// The device's own signal. Never inferred from the reported delay, which
    /// `alsa` documents "will not necessarily got down to 0" on underrun.
    pub underran: bool,
}

/// Why a sink could not be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SinkError {
    /// The underlying ALSA device failed.
    Alsa(AlsaError),
    /// A modelled device failed. Only reachable from the test suite.
    Modelled(String),
}

impl fmt::Display for SinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SinkError::Alsa(e) => write!(f, "{}", e),
            SinkError::Modelled(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for SinkError {}

impl From<AlsaError> for SinkError {
    fn from(e: AlsaError) -> SinkError {
        SinkError::Alsa(e)
    }
}

/// Something that plays interleaved PCM and can say how far it is from the
/// DAC.
pub trait PcmSink: Send {
    /// The device name, for reports and errors.
    fn device(&self) -> &str;

    /// Bytes one frame occupies.
    fn frame_len(&self) -> usize;

    /// Frames per second.
    fn rate_hz(&self) -> u32;

    /// Hand whole frames to the device.
    fn write(&mut self, pcm: &[u8]) -> Result<SinkWrite, SinkError>;

    /// Frames the device still has to play before a frame written now becomes
    /// audible.
    fn delay_frames(&mut self) -> Result<i64, SinkError>;

    /// Whether the device is in its underrun state right now.
    fn in_xrun(&mut self) -> Result<bool, SinkError>;

    /// Play out what the device holds, then stop.
    fn drain(&mut self) -> Result<(), SinkError>;

    /// Frames the device has actually played out since it was opened.
    ///
    /// Used to check that no rate change happened: over a run this must match
    /// the device's nominal rate, which it cannot if anything resampled or
    /// retimed the stream.
    fn frames_played(&mut self) -> Result<u64, SinkError>;
}

/// The ALSA device the shipped client plays through.
pub struct AlsaSink {
    pcm: Pcm,
    frames_accepted: u64,
}

impl AlsaSink {
    /// Open `device` for this stream.
    ///
    /// `buffer_us` is what the device ring is asked for. The client asks for
    /// more than its own configured maximum so that the ring is never the
    /// thing that caps the reported delay.
    pub fn open(
        device: &str,
        sample_format: SampleFormat,
        channels: u16,
        rate_hz: u32,
        buffer_us: u32,
    ) -> Result<AlsaSink, SinkError> {
        let format = match sample_format {
            SampleFormat::PcmS16Le => Format::S16Le,
            SampleFormat::PcmS24Le => Format::S24Packed3Le,
            SampleFormat::PcmF32Le => Format::F32Le,
        };
        let pcm = Pcm::open(device, format, channels, rate_hz, buffer_us)?;
        Ok(AlsaSink {
            pcm,
            frames_accepted: 0,
        })
    }
}

impl PcmSink for AlsaSink {
    fn device(&self) -> &str {
        self.pcm.device()
    }

    fn frame_len(&self) -> usize {
        self.pcm.frame_len()
    }

    fn rate_hz(&self) -> u32 {
        self.pcm.rate_hz()
    }

    fn write(&mut self, pcm: &[u8]) -> Result<SinkWrite, SinkError> {
        let report = self.pcm.write(pcm)?;
        self.frames_accepted += report.frames_written;
        Ok(SinkWrite {
            frames_written: report.frames_written,
            underran: report.underran,
        })
    }

    fn delay_frames(&mut self) -> Result<i64, SinkError> {
        Ok(self.pcm.delay_frames()?)
    }

    fn in_xrun(&mut self) -> Result<bool, SinkError> {
        Ok(self.pcm.in_xrun()?)
    }

    fn drain(&mut self) -> Result<(), SinkError> {
        Ok(self.pcm.drain()?)
    }

    fn frames_played(&mut self) -> Result<u64, SinkError> {
        // Frames handed over, less the ones still queued for the DAC. The
        // device's own delay is what makes this the played-out count rather
        // than the written count.
        let delay = self.pcm.delay_frames()?.max(0) as u64;
        Ok(self.frames_accepted.saturating_sub(delay))
    }
}
