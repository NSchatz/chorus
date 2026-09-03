//! The device-backed half: emitting the chirp and recording the two line
//! outputs.
//!
//! Nothing in the ordinary test suite runs anything here that touches a device,
//! and that is deliberate rather than an omission. What the suite does run is
//! the refusal: on a machine with no capture device, this module says which
//! prerequisite is missing and exits non-zero. `tools/lib.sh` records why the
//! repository insists on that shape - a check whose environment is absent and
//! which reports green makes every row of the evidence table worthless, and
//! this repository has been bitten by exactly that.
//!
//! The device is reached through `chorus-alsa`, which loads `libasound.so.2`
//! with `dlopen` at run time. So this crate builds, and its tests pass, on a
//! machine with no audio stack at all.

use std::fmt;

use chorus_alsa::{AlsaError, Format, Pcm};
use chorus_audio::MonotonicTimeline;

use crate::chirp::ChirpSpec;
use crate::wav::{self, WAVE_FORMAT_PCM};

/// The ring this rig asks a device for, in microseconds.
const BUFFER_US: u32 = 200_000;

/// What a probe found out about a device.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceProbe {
    /// The device that was asked about.
    pub device: String,
    /// Whether it opened for capture at the rate this rig wants.
    pub usable: bool,
    /// What happened, in words an operator can act on.
    pub detail: String,
}

impl fmt::Display for DeviceProbe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "capture-device-probe device={} usable={} detail={}",
            self.device,
            u8::from(self.usable),
            self.detail
        )
    }
}

/// Try to open `device` for capture, and say what happened either way.
///
/// Never panics and never returns an error: the caller wants a verdict it can
/// print, and "there is no ALSA on this machine" is a different sentence from
/// "this device does not exist".
pub fn probe_capture_device(device: &str, rate_hz: u32) -> DeviceProbe {
    if let Err(e) = chorus_alsa::runtime_available() {
        return DeviceProbe {
            device: device.to_string(),
            usable: false,
            detail: e.to_string(),
        };
    }
    match Pcm::open_capture(device, Format::S16Le, 2, rate_hz, BUFFER_US) {
        Ok(_) => DeviceProbe {
            device: device.to_string(),
            usable: true,
            detail: format!("opened for two-channel capture at {} Hz", rate_hz),
        },
        Err(e) => DeviceProbe {
            device: device.to_string(),
            usable: false,
            detail: e.to_string(),
        },
    }
}

/// What a device-backed run produced.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedCapture {
    /// The recording, as a RIFF/WAVE container ready to be written to disk.
    pub wav: Vec<u8>,
    /// Frames recorded.
    pub frames: usize,
    /// How long the recording took, on a monotonic clock.
    pub elapsed_us: u64,
    /// Whether the device reported an overrun during it.
    pub overran: bool,
}

/// Record `frames` frames of two-channel audio from `device`.
///
/// The elapsed time is taken from `chorus_audio::MonotonicTimeline`, which is
/// this repository's one monotonic time base, for the reason its own module
/// documentation gives: a settable clock read here would put a step in the
/// middle of a recording's own timeline.
pub fn record(device: &str, rate_hz: u32, frames: usize) -> Result<RecordedCapture, AlsaError> {
    let mut pcm = Pcm::open_capture(device, Format::S16Le, 2, rate_hz, BUFFER_US)?;
    let frame_len = pcm.frame_len();
    let mut raw = vec![0u8; frames * frame_len];
    let timeline = MonotonicTimeline::new();
    let report = pcm.read(&mut raw)?;
    let elapsed_us = timeline.now_us();

    let mut samples = Vec::with_capacity(frames * 2);
    for at in (0..raw.len()).step_by(2) {
        samples.push(i16::from_le_bytes([raw[at], raw[at + 1]]));
    }
    Ok(RecordedCapture {
        wav: wav::write_wav(&samples, 2, rate_hz, 16, WAVE_FORMAT_PCM),
        frames: report.frames_written as usize,
        elapsed_us,
        overran: report.underran,
    })
}

/// Play `chirp` out of `device` for `frames` frames.
///
/// The amplitude ceiling is enforced in [`ChirpSpec::new`], which is what
/// builds the argument to this function, so an over-level run cannot reach
/// here: there is no path from a requested amplitude to a device that does not
/// pass the refusal first.
pub fn emit(device: &str, rate_hz: u32, frames: usize, chirp: &ChirpSpec) -> Result<u64, AlsaError> {
    let mut pcm = Pcm::open(device, Format::S16Le, 2, rate_hz, BUFFER_US)?;
    let mut raw = Vec::with_capacity(frames * 4);
    for n in 0..frames {
        let value = wav::to_i16(chirp.at(n as f64 / f64::from(rate_hz)));
        raw.extend_from_slice(&value.to_le_bytes());
        raw.extend_from_slice(&value.to_le_bytes());
    }
    let timeline = MonotonicTimeline::new();
    let report = pcm.write(&raw)?;
    pcm.drain()?;
    let _ = report;
    Ok(timeline.now_us())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_device_that_is_not_there_probes_as_unusable_and_says_why() {
        let probe = probe_capture_device("chorus-no-such-capture-device", 96_000);
        assert!(!probe.usable);
        assert!(!probe.detail.is_empty());
        let said = probe.to_string();
        assert!(said.contains("usable=0"), "{}", said);
        assert!(said.contains("chorus-no-such-capture-device"), "{}", said);
    }

    #[test]
    fn recording_from_a_device_that_is_not_there_is_an_error_and_not_silence() {
        let err = record("chorus-no-such-capture-device", 96_000, 128).unwrap_err();
        // Either the device is not there, or there is no ALSA at all. Both are
        // refusals; neither hands back a buffer of zeroes.
        assert!(
            matches!(err, AlsaError::Call { .. } | AlsaError::RuntimeMissing { .. }),
            "{:?}",
            err
        );
    }
}
