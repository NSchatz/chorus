//! Where the PCM comes from.
//!
//! Two sources, both of them local, because this phase is about what happens
//! to bytes after they arrive rather than about where they came from:
//!
//! - a **file** of raw interleaved PCM in the configured format, which is what
//!   the chunking verifications feed, because a file of known content is the
//!   only way to assert that the chunks concatenated are the input;
//! - a **generated tone**, which is what the ten-minute run plays, because it
//!   is endless, audible, and obviously wrong when it is wrong;
//! - the measurement rig's **chirp**, which is what every run graded by the
//!   RIG-3 harness plays. The lag analyser refuses anything that does not put
//!   most of its energy in the chirp's band (`no-chirp-present`), and a 440 Hz
//!   tone puts essentially none there, so a graded run that streamed the tone
//!   could only ever end in that refusal. The waveform is not restated here:
//!   it is `chorus_measure::ChirpSpec`, built from `config/measure.conf`
//!   through the same amplitude ceiling the capture tool is held to.
//!
//! A source **ends cleanly** when it has delivered its last byte and closed
//! without error. That is the only thing that earns an end-of-stream signal; a
//! source that failed mid-read did not end, it broke, and the client is told
//! the difference.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use chorus_audio::StreamFormat;
use chorus_measure::{ChirpSpec, MeasureConfig};

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
        ToneSource {
            format,
            frames_remaining: frames_for(&format, ms),
            frame_index: 0,
        }
    }
}

/// One frame of `value` on every channel, in `format`'s sample layout.
fn write_frame(format: &StreamFormat, value: f32, out: &mut [u8]) {
    let bytes_per_sample = format.sample_format.bytes_per_sample();
    for channel in 0..format.channels as usize {
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

/// Fill `buf` with whole frames of a generated waveform, `sample_at(frame)`
/// on every channel, counting on from `*frame_index` and stopping at
/// `*frames_remaining` when there is one.
fn fill_generated<F: Fn(u64) -> f32>(
    format: &StreamFormat,
    frame_index: &mut u64,
    frames_remaining: &mut Option<u64>,
    sample_at: F,
    buf: &mut [u8],
) -> usize {
    let frame_len = format.frame_len();
    let mut frames_wanted = (buf.len() / frame_len) as u64;
    if let Some(left) = *frames_remaining {
        frames_wanted = frames_wanted.min(left);
    }
    if frames_wanted == 0 {
        return 0;
    }
    for i in 0..frames_wanted as usize {
        let at = i * frame_len;
        let value = sample_at(*frame_index + i as u64);
        write_frame(format, value, &mut buf[at..at + frame_len]);
    }
    *frame_index += frames_wanted;
    if let Some(left) = frames_remaining.as_mut() {
        *left -= frames_wanted;
    }
    frames_wanted as usize * frame_len
}

fn frames_for(format: &StreamFormat, ms: u64) -> Option<u64> {
    if ms == 0 {
        None
    } else {
        Some(ms * u64::from(format.sample_rate_hz) / 1_000)
    }
}

impl PcmSource for ToneSource {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let format = self.format;
        let rate = f64::from(format.sample_rate_hz);
        let tone = |frame: u64| {
            let t = frame as f64 / rate;
            (0.2 * (2.0 * std::f64::consts::PI * 440.0 * t).sin()) as f32
        };
        Ok(fill_generated(
            &format,
            &mut self.frame_index,
            &mut self.frames_remaining,
            tone,
            buf,
        ))
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

/// The measurement rig's chirp, as the grouped stream carries it.
///
/// Every sample is `ChirpSpec::at(frame / rate)`, the same function of
/// continuous time the capture tool and the committed lag fixtures evaluate,
/// so the stream carries exactly the sweep `crates/measure` defines and the
/// lag analyser looks for, at whatever rate the stream runs. Like the tone it
/// is a pure function of the frame counter, never of a clock, and every
/// channel carries the same sample.
///
/// A `ChirpSpec` can only be built through its amplitude ceiling, so there is
/// no way to construct one of these above `config/measure.conf`'s limit.
pub struct ChirpSource {
    format: StreamFormat,
    chirp: ChirpSpec,
    frames_remaining: Option<u64>,
    frame_index: u64,
}

impl ChirpSource {
    /// The chirp in `format`, lasting `ms` milliseconds, or endless when `ms`
    /// is zero.
    pub fn new(format: StreamFormat, chirp: ChirpSpec, ms: u64) -> ChirpSource {
        ChirpSource {
            format,
            chirp,
            frames_remaining: frames_for(&format, ms),
            frame_index: 0,
        }
    }
}

impl PcmSource for ChirpSource {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let format = self.format;
        let chirp = self.chirp;
        let rate = f64::from(format.sample_rate_hz);
        let sample = |frame: u64| chirp.at(frame as f64 / rate) as f32;
        Ok(fill_generated(
            &format,
            &mut self.frame_index,
            &mut self.frames_remaining,
            sample,
            buf,
        ))
    }

    fn describe(&self) -> String {
        let (lo, hi) = self.chirp.band_hz();
        let what = format!(
            "generated measurement chirp {:.0} to {:.0} Hz every {:.0} us at {} full scale",
            lo,
            hi,
            self.chirp.period_s() * 1_000_000.0,
            self.chirp.amplitude()
        );
        match self.frames_remaining {
            None => format!("{}, endless", what),
            Some(left) => format!("{}, {} frames", what, left + self.frame_index),
        }
    }
}

/// The rig's chirp, from the measurement configuration at `measure_config`
/// and held to the amplitude ceiling it declares; `amplitude` is `None` for
/// the ceiling itself, which is what `chorus-measure-capture` defaults to.
///
/// The same reader and the same constructor the capture tool uses, over the
/// same file, so what the endpoints play is the sweep the lag analyser looks
/// for. The error is a sentence for an operator, naming the file.
pub fn rig_chirp(measure_config: &Path, amplitude: Option<f64>) -> Result<ChirpSpec, String> {
    let text = std::fs::read_to_string(measure_config).map_err(|e| {
        format!(
            "the measurement configuration '{}' could not be read: {}. --source chirp takes the \
             band, the period and the amplitude ceiling from it; pass --measure-config if it is \
             not in the working directory",
            measure_config.display(),
            e
        )
    })?;
    let measure = MeasureConfig::parse(measure_config, &text).map_err(|e| e.to_string())?;
    ChirpSpec::new(
        measure.chirp_start_hz,
        measure.chirp_end_hz,
        measure.chirp_period_us,
        amplitude.unwrap_or(measure.chirp_amplitude_ceiling),
        measure.chirp_amplitude_ceiling,
        &measure_config.display().to_string(),
    )
    .map_err(|e| e.to_string())
}

/// Build the configured source.
///
/// `chirp` is the rig's chirp, already built and already held to its ceiling;
/// it is required exactly when `source` is `chirp`.
pub fn open(
    source: &str,
    format: StreamFormat,
    tone_ms: u64,
    chirp: Option<&ChirpSpec>,
) -> io::Result<Box<dyn PcmSource>> {
    match source {
        "tone" => Ok(Box::new(ToneSource::new(format, tone_ms))),
        "chirp" => match chirp {
            Some(chirp) => Ok(Box::new(ChirpSource::new(format, *chirp, tone_ms))),
            None => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the chirp source was asked for and no chirp was built from config/measure.conf",
            )),
        },
        _ => Ok(Box::new(FileSource::open(source)?)),
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

    fn committed_chirp() -> ChirpSpec {
        rig_chirp(
            &chorus_measure::repository_root().join(chorus_measure::config::CONFIG_FILE),
            None,
        )
        .expect("config/measure.conf is committed")
    }

    #[test]
    fn the_chirp_source_carries_exactly_the_rigs_waveform() {
        // f32 LE so the comparison is against the waveform itself rather than
        // a quantisation of it.
        let format = StreamFormat::new(48_000, 2, "pcm_f32le").unwrap();
        let chirp = committed_chirp();
        let mut source = ChirpSource::new(format, chirp, 50);
        let mut buf = vec![0u8; 8 * 2400];
        let n = source.read(&mut buf).unwrap();
        assert_eq!(n, 8 * 2400, "50 ms at 48 kHz is 2400 frames");
        for frame in 0..2400usize {
            let want = chirp.at(frame as f64 / 48_000.0) as f32;
            for channel in 0..2 {
                let at = frame * 8 + channel * 4;
                let got = f32::from_le_bytes(buf[at..at + 4].try_into().unwrap());
                assert_eq!(got, want, "frame {} channel {}", frame, channel);
            }
        }
        assert_eq!(source.read(&mut buf).unwrap(), 0, "a bounded chirp ends");
    }

    #[test]
    fn a_chirp_above_the_declared_ceiling_is_refused_naming_both_numbers() {
        let path = chorus_measure::repository_root().join(chorus_measure::config::CONFIG_FILE);
        let err = rig_chirp(&path, Some(0.9)).unwrap_err();
        assert!(err.contains("0.9"), "{}", err);
        assert!(err.contains("no audio has been emitted"), "{}", err);
    }

    #[test]
    fn the_chirp_source_needs_a_chirp() {
        let format = StreamFormat::new(48_000, 2, "pcm_s16le").unwrap();
        assert!(open("chirp", format, 10, None).is_err());
        assert!(open("chirp", format, 10, Some(&committed_chirp())).is_ok());
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
