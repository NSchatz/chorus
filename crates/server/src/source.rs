//! Where the PCM comes from.
//!
//! Every source is local, because this phase is about what happens to bytes
//! after they arrive rather than about where they came from:
//!
//! - a **file** of raw interleaved PCM in the configured format, which is what
//!   the chunking verifications feed, because a file of known content is the
//!   only way to assert that the chunks concatenated are the input;
//! - a **FIFO** (a named pipe, `fifo:<path>` or any path that is one), the
//!   development input BRIEF.md section 2.1 names. It is NOT a file: a player
//!   that pauses or changes track closes its end, and that must not end every
//!   client's stream. See [`FifoSource`];
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

use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::os::unix::fs::{FileTypeExt, OpenOptionsExt};
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

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

/// `O_NONBLOCK` for `open(2)` on the Linux architectures chorus builds for.
///
/// std has no safe spelling of it and this crate denies unsafe code, so the
/// flag is passed through `OpenOptionsExt::custom_flags`. The value is the
/// one the permissively licensed `libc` crate declares for Linux on x86_64 and
/// aarch64, 2048 (octal 04000): docs.rs/libc/latest/x86_64-unknown-linux-gnu
/// and .../aarch64-unknown-linux-gnu, `constant.O_NONBLOCK.html`, read
/// 2026-09-30. Any other target gets no FIFO source rather than an unchecked
/// flag.
#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
const O_NONBLOCK: Option<i32> = Some(0o4000);
#[cfg(not(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
)))]
const O_NONBLOCK: Option<i32> = None;

/// How often an empty pipe is looked at again while a chunk is being waited
/// for. A millisecond is a twentieth of the default chunk.
const FIFO_POLL: Duration = Duration::from_millis(1);

/// A named pipe, held open for as long as the stream runs (audit B-2).
///
/// Three things make a pipe different from a file, and each is handled here:
///
/// - **A writer that closes is not the end.** The pipe is opened read-WRITE,
///   so this process is itself a writer and a read never sees end-of-file
///   when the player closes its end; the next player to open the pipe for
///   writing carries on the same stream. `read` never returns `Ok(0)`, so a
///   pipe source never ends a stream and never sends `stream_end`.
/// - **No writer is not a hang.** A read-only open of a pipe blocks until a
///   writer appears; a read-write open does not, on Linux ("Under Linux,
///   opening a FIFO for read and write will succeed both in blocking and
///   nonblocking mode", fifo(7), https://man7.org/linux/man-pages/man7/fifo.7.html,
///   read 2026-09-30). The pipe is also non-blocking, so the supervisor that
///   opens it and the audio thread that reads it never wait on a player.
/// - **An underrun is silence, on time.** Every `read` hands back exactly one
///   chunk. Whatever whole frames the pipe holds go first; if the pipe has not
///   filled the chunk within half a chunk period, the rest of it is silence
///   (all-zero bytes, which is silence in every supported format). The
///   emitter paces chunks on the monotonic timeline as it does for every
///   source, so the stream keeps its cadence through a pause. A partial frame
///   is held back until its remaining bytes arrive, so silence never splits a
///   frame.
pub struct FifoSource {
    path: String,
    pipe: File,
    chunk_bytes: usize,
    frame_len: usize,
    wait: Duration,
    pending: Vec<u8>,
    silent_frames: u64,
    piped_frames: u64,
}

impl FifoSource {
    /// Open the pipe at `path` for chunks of `chunk_us` in `format`.
    ///
    /// Refused, by name, when `path` is not a FIFO: a plain file keeps the
    /// file source's end-of-stream behaviour, and the two are not guessed
    /// between.
    pub fn open(path: &str, format: StreamFormat, chunk_us: u64) -> io::Result<FifoSource> {
        let kind = std::fs::metadata(path).map_err(|e| {
            io::Error::new(
                e.kind(),
                format!(
                    "fifo:{} cannot be opened: {}; create the pipe first (mkfifo {})",
                    path, e, path
                ),
            )
        })?;
        if !kind.file_type().is_fifo() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "fifo:{} is not a FIFO; a plain file is read with --source {} and ends the                      stream at its last byte",
                    path, path
                ),
            ));
        }
        let Some(nonblock) = O_NONBLOCK else {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "the FIFO source is built only for Linux on x86_64 and aarch64",
            ));
        };
        let frames = format
            .frames_in(chunk_us)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
        let frame_len = format.frame_len();
        let pipe = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(nonblock)
            .open(path)?;
        Ok(FifoSource {
            path: path.to_string(),
            pipe,
            chunk_bytes: frames.max(1) * frame_len,
            frame_len,
            wait: Duration::from_micros(chunk_us / 2),
            pending: Vec::new(),
            silent_frames: 0,
            piped_frames: 0,
        })
    }

    /// Frames handed out as silence because the pipe had nothing for them.
    pub fn silent_frames(&self) -> u64 {
        self.silent_frames
    }

    /// Frames handed out from the pipe.
    pub fn piped_frames(&self) -> u64 {
        self.piped_frames
    }

    /// Move what the pipe holds into `pending`, up to `want` bytes. Never
    /// blocks.
    fn drain_pipe(&mut self, want: usize) -> io::Result<()> {
        let mut scratch = [0u8; 4096];
        while self.pending.len() < want {
            let room = (want - self.pending.len()).min(scratch.len());
            match self.pipe.read(&mut scratch[..room]) {
                // Cannot happen while this process holds the write end; if it
                // ever does, it is an empty pipe and not an end.
                Ok(0) => return Ok(()),
                Ok(n) => self.pending.extend_from_slice(&scratch[..n]),
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(()),
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

impl PcmSource for FifoSource {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let want = self
            .chunk_bytes
            .min(buf.len() / self.frame_len * self.frame_len);
        if want == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the FIFO source was asked to fill less than one frame",
            ));
        }
        // A monotonic deadline: the wait is a bound on how long the emitter
        // is held, not a timestamp of anything.
        let deadline = Instant::now() + self.wait;
        loop {
            self.drain_pipe(want)?;
            if self.pending.len() >= want || Instant::now() >= deadline {
                break;
            }
            thread::sleep(FIFO_POLL);
        }
        let whole = (self.pending.len().min(want) / self.frame_len) * self.frame_len;
        buf[..whole].copy_from_slice(&self.pending[..whole]);
        self.pending.drain(..whole);
        buf[whole..want].fill(0);
        self.piped_frames += (whole / self.frame_len) as u64;
        self.silent_frames += ((want - whole) / self.frame_len) as u64;
        Ok(want)
    }

    fn describe(&self) -> String {
        format!(
            "fifo {}, held open read-write; a closed writer is silence, never an end",
            self.path
        )
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
/// it is required exactly when `source` is `chirp`. `chunk_us` is the chunk
/// duration, which a FIFO needs to fill an underrun with exactly one chunk of
/// silence.
///
/// `fifo:<path>` is a named pipe and is refused if `path` is not one; a bare
/// path that IS a pipe is read as one too, because reading a pipe as a file is
/// the defect audit B-2 records. Any other path is a file, which ends the
/// stream at its last byte.
pub fn open(
    source: &str,
    format: StreamFormat,
    chunk_us: u64,
    tone_ms: u64,
    chirp: Option<&ChirpSpec>,
) -> io::Result<Box<dyn PcmSource>> {
    if let Some(path) = source.strip_prefix("fifo:") {
        return Ok(Box::new(FifoSource::open(path, format, chunk_us)?));
    }
    if std::fs::metadata(source)
        .map(|m| m.file_type().is_fifo())
        .unwrap_or(false)
    {
        return Ok(Box::new(FifoSource::open(source, format, chunk_us)?));
    }
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
        assert!(open("chirp", format, 20_000, 10, None).is_err());
        assert!(open("chirp", format, 20_000, 10, Some(&committed_chirp())).is_ok());
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

    /// A real named pipe in a fresh directory, made by `mkfifo(1)` so the test
    /// needs no unsafe code.
    fn a_real_fifo(stem: &str) -> (std::path::PathBuf, String) {
        let dir = std::env::temp_dir().join(format!("chorus-{}-{}", stem, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pcm.fifo");
        let made = std::process::Command::new("mkfifo")
            .arg(&path)
            .status()
            .expect("mkfifo runs");
        assert!(made.success(), "mkfifo made {}", path.display());
        let text = path.display().to_string();
        (dir, text)
    }

    #[test]
    fn a_fifo_with_no_writer_opens_at_once_and_reads_one_chunk_of_silence() {
        let format = StreamFormat::new(48_000, 2, "pcm_s16le").unwrap();
        let (dir, path) = a_real_fifo("fifo-no-writer");
        let started = Instant::now();
        let mut pipe = open(&format!("fifo:{}", path), format, 20_000, 0, None).unwrap();
        let mut buf = vec![0xAAu8; 8192];
        let n = pipe.read(&mut buf).unwrap();
        assert_eq!(n, 960 * 4, "one 20 ms chunk at 48 kHz stereo s16");
        assert!(buf[..n].iter().all(|b| *b == 0), "an empty pipe is silence");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "neither the open nor the read waited for a writer: {:?}",
            started.elapsed()
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_fifo_whose_writer_closes_and_reopens_carries_on_and_never_ends() {
        use std::io::Write;
        let format = StreamFormat::new(48_000, 2, "pcm_s16le").unwrap();
        let (dir, path) = a_real_fifo("fifo-reopen");
        let mut pipe = FifoSource::open(&path, format, 20_000).unwrap();
        let chunk = 960 * 4;
        let mut buf = vec![0u8; chunk];

        // The first player writes two chunks and a partial frame, and closes.
        let first: Vec<u8> = (0..2 * chunk + 3).map(|i| (i % 251) as u8 | 1).collect();
        {
            let mut writer = OpenOptions::new().write(true).open(&path).unwrap();
            writer.write_all(&first).unwrap();
        }
        for k in 0..2 {
            assert_eq!(pipe.read(&mut buf).unwrap(), chunk);
            assert_eq!(&buf[..], &first[k * chunk..(k + 1) * chunk], "chunk {}", k);
        }
        // The writer is gone: never an end, always a whole chunk of silence,
        // and the three bytes of a partial frame are held back, not played.
        for _ in 0..5 {
            assert_eq!(
                pipe.read(&mut buf).unwrap(),
                chunk,
                "a closed writer is not Ok(0)"
            );
            assert!(buf.iter().all(|b| *b == 0));
        }
        assert_eq!(pipe.piped_frames(), 2 * 960);
        assert_eq!(pipe.silent_frames(), 5 * 960);

        // A second player opens the same pipe; its first byte completes the
        // held frame and the stream carries on with its audio.
        let second: Vec<u8> = (0..chunk + 1).map(|i| (i % 241) as u8 | 1).collect();
        {
            let mut writer = OpenOptions::new().write(true).open(&path).unwrap();
            writer.write_all(&second).unwrap();
        }
        assert_eq!(pipe.read(&mut buf).unwrap(), chunk);
        let mut want = first[2 * chunk..].to_vec();
        want.extend_from_slice(&second[..chunk - 3]);
        assert_eq!(
            &buf[..],
            &want[..],
            "the held partial frame, then the second writer"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_plain_file_is_not_a_fifo_and_still_ends_at_its_last_byte() {
        let format = StreamFormat::new(48_000, 2, "pcm_s16le").unwrap();
        let dir = std::env::temp_dir().join(format!("chorus-plain-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pcm.raw");
        std::fs::write(&path, [1u8; 16]).unwrap();
        let text = path.display().to_string();
        let err = open(&format!("fifo:{}", text), format, 20_000, 0, None)
            .err()
            .expect("a plain file behind fifo: is refused");
        assert!(err.to_string().contains("is not a FIFO"), "{}", err);
        let mut file = open(&text, format, 20_000, 0, None).unwrap();
        let mut buf = vec![0u8; 64];
        assert_eq!(file.read(&mut buf).unwrap(), 16);
        assert_eq!(file.read(&mut buf).unwrap(), 0, "a file ends");
        let missing = open("fifo:/nonexistent/chorus.fifo", format, 20_000, 0, None)
            .err()
            .expect("a missing pipe is refused");
        assert!(missing.to_string().contains("mkfifo"), "{}", missing);
        let _ = std::fs::remove_dir_all(dir);
    }
}
