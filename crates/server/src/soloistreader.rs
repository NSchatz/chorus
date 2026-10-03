//! The Soloist receivers' reader threads (goal 17, `docs/soloist.md`).
//!
//! `--soloist-receivers N` makes N threads, `soloist-reader-0` to
//! `soloist-reader-<N-1>`, at start and never after (a thread made when a
//! receiver starts playing would be a thread the scheduling report never
//! saw, `main.rs`). Thread `i` is the one reader of `r<i>.pcm` in the
//! receiver directory and the one writer of port `i`
//! (`crate::soloistport`).
//!
//! What a reader does, always, whether or not anything plays:
//!
//! 1. Opens the FIFO with the FIFO source's rules (`crate::source`):
//!    read-write and non-blocking, so no writer is not a hang and a writer
//!    that closes is not an end. A FIFO that is not there yet (the receiver
//!    container starts later and makes it) is looked for again every
//!    [`REOPEN`].
//! 2. Reads whatever the pipe holds, so the pipe never fills and never keeps
//!    stale audio: PipeWire's sink drops what a full pipe cannot take and
//!    never stalls Soloist, and a reader that attached late would find the
//!    oldest 64 KiB (`docs/soloist.md`).
//! 3. Converts: interleaved little-endian float32 at 44.1 kHz stereo
//!    (`chorus_soloist`'s constants) to the server's channel count with the
//!    media player's remix (`chorus_decode::remix`) and to the server's rate
//!    with its resampler (`chorus_decode::Resampler`).
//! 4. Writes the port, which discards while no group plays the receiver and
//!    drops what a full ring cannot take.
//!
//! The only clock here is the monotonic one (`Instant`): the pause between
//! two looks at an empty pipe, and how long the pipe was quiet, which tells
//! an underrun from a stream that stopped. Nothing is stamped: the audio
//! thread stamps chunks on its timeline when it cuts them (`crate::slots`).

use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::os::unix::fs::{FileTypeExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use chorus_decode::Resampler;
use chorus_hostctl::ThreadRegistry;
use chorus_soloist::{pcm_file_name, PCM_CHANNELS, PCM_FRAME_BYTES, PCM_RATE, PCM_SAMPLE_BYTES};

use crate::hostreport::register_ordinary_thread;
use crate::soloistport::{SoloistPort, STREAM_GAP};
use crate::source::O_NONBLOCK;

/// How long a reader sleeps after it found the pipe empty. ASSUMED: 5 ms, a
/// quarter of the default chunk and a ninth of a quantum; the fill target
/// (`crate::soloistport::FILL_TARGET_MS`) is what absorbs it.
pub const POLL: Duration = Duration::from_millis(5);

/// How often a FIFO that is not there yet is looked for again. ASSUMED.
pub const REOPEN: Duration = Duration::from_millis(250);

/// The most one read takes: the kernel's default pipe size, 64 KiB.
const READ_BYTES: usize = 64 * 1024;

/// What one reader has counted since the server started.
#[derive(Debug, Default)]
pub struct ReaderStats {
    /// Frames read from the FIFO, played or not.
    pub frames_read: AtomicU64,
    /// Whether the FIFO is open now.
    pub open: AtomicBool,
}

/// The conversion from the FIFO's format to a port's: bytes in, interleaved
/// f32 frames at the port's rate and channel count out. Pure: no clock, no
/// file.
pub struct Converter {
    rate: u32,
    channels: u16,
    resampler: Resampler,
    /// The bytes of a frame that has not wholly arrived.
    partial: Vec<u8>,
    decoded: Vec<f32>,
    remixed: Vec<f32>,
    out: Vec<f32>,
}

impl Converter {
    /// A converter to `rate` Hz and `channels` channels.
    pub fn new(rate: u32, channels: u16) -> Converter {
        Converter {
            rate,
            channels,
            resampler: Resampler::new(PCM_RATE, rate, channels),
            partial: Vec::with_capacity(PCM_FRAME_BYTES),
            decoded: Vec::new(),
            remixed: Vec::new(),
            out: Vec::new(),
        }
    }

    /// Forget the resampler's history and any partial frame: the next bytes
    /// start a new stream.
    pub fn reset(&mut self) {
        self.resampler = Resampler::new(PCM_RATE, self.rate, self.channels);
        self.partial.clear();
    }

    /// Convert `bytes`; returns the frames the FIFO delivered whole and the
    /// converted samples (which a resampler may hold part of until more
    /// arrives). A sample that is not a finite number is silence: the
    /// resampler's history must not be poisoned by one.
    pub fn feed(&mut self, bytes: &[u8]) -> (usize, &[f32]) {
        self.decoded.clear();
        self.remixed.clear();
        self.out.clear();
        let mut rest = bytes;
        if !self.partial.is_empty() {
            let need = PCM_FRAME_BYTES - self.partial.len();
            let take = need.min(rest.len());
            self.partial.extend_from_slice(&rest[..take]);
            rest = &rest[take..];
            if self.partial.len() == PCM_FRAME_BYTES {
                let frame = std::mem::take(&mut self.partial);
                Self::decode(&frame, &mut self.decoded);
                self.partial = frame;
                self.partial.clear();
            }
        }
        let whole = rest.len() / PCM_FRAME_BYTES * PCM_FRAME_BYTES;
        Self::decode(&rest[..whole], &mut self.decoded);
        self.partial.extend_from_slice(&rest[whole..]);
        let frames = self.decoded.len() / PCM_CHANNELS;
        chorus_decode::remix(
            &self.decoded,
            PCM_CHANNELS as u16,
            self.channels,
            &mut self.remixed,
        );
        self.resampler.process(&self.remixed, &mut self.out);
        (frames, &self.out)
    }

    fn decode(bytes: &[u8], out: &mut Vec<f32>) {
        out.extend(bytes.as_chunks::<PCM_SAMPLE_BYTES>().0.iter().map(|b| {
            let sample = f32::from_le_bytes(*b);
            if sample.is_finite() {
                sample
            } else {
                0.0
            }
        }));
    }
}

/// Open the FIFO at `path` as the FIFO source does: read-write and
/// non-blocking. `None` while it is not there, or is not a FIFO.
fn open(path: &Path) -> Option<File> {
    let nonblock = O_NONBLOCK?;
    let kind = std::fs::metadata(path).ok()?;
    if !kind.file_type().is_fifo() {
        return None;
    }
    OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(nonblock)
        .open(path)
        .ok()
}

fn run(path: PathBuf, port: Arc<SoloistPort>, stats: Arc<ReaderStats>, keep: Arc<AtomicBool>) {
    let mut converter = Converter::new(port.rate_hz(), port.channels() as u16);
    let mut scratch = vec![0u8; READ_BYTES];
    let mut pipe: Option<File> = None;
    // When the pipe last had bytes: `None` before the first.
    let mut last: Option<Instant> = None;
    while keep.load(Ordering::SeqCst) {
        let Some(file) = pipe.as_mut() else {
            pipe = open(&path);
            stats.open.store(pipe.is_some(), Ordering::SeqCst);
            if pipe.is_none() {
                thread::sleep(REOPEN);
            }
            continue;
        };
        match file.read(&mut scratch) {
            // Cannot happen while this process holds the write end; an
            // empty pipe and not an end.
            Ok(0) => thread::sleep(POLL),
            Ok(n) => {
                let now = Instant::now();
                let quiet = last.map_or(Duration::MAX, |t| now.duration_since(t));
                last = Some(now);
                if quiet >= STREAM_GAP {
                    // A new stream: nothing of the old one's tail is mixed
                    // into its first frames.
                    converter.reset();
                }
                let (frames, samples) = converter.feed(&scratch[..n]);
                stats
                    .frames_read
                    .fetch_add(frames as u64, Ordering::Relaxed);
                port.write(samples, quiet);
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => thread::sleep(POLL),
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => {
                // The descriptor went bad: open the path again.
                pipe = None;
                stats.open.store(false, Ordering::SeqCst);
                thread::sleep(REOPEN);
            }
        }
    }
}

/// Create the reader threads, one per port: `soloist-reader-<i>` reads
/// `r<i>.pcm` under `dir` into `ports[i]`. Each registers itself as an
/// ordinary thread and sends one unit down `ready` before it opens
/// anything, as every thread of the population does. Returns how many
/// threads were created.
///
/// # Panics
///
/// When `ports` and `stats` differ in length: a wiring mistake in the
/// binary, found at start.
pub fn spawn(
    dir: &Path,
    ports: &[Arc<SoloistPort>],
    stats: &[Arc<ReaderStats>],
    keep: &Arc<AtomicBool>,
    registry: &Arc<ThreadRegistry>,
    ready: &Sender<()>,
) -> usize {
    assert_eq!(ports.len(), stats.len(), "one stats block per port");
    for (index, (port, stats)) in ports.iter().zip(stats).enumerate() {
        let path = dir.join(pcm_file_name(index));
        let port = Arc::clone(port);
        let stats = Arc::clone(stats);
        let keep = Arc::clone(keep);
        let registry = Arc::clone(registry);
        let ready = ready.clone();
        thread::spawn(move || {
            register_ordinary_thread(&format!("soloist-reader-{}", index), &registry);
            if ready.send(()).is_err() {
                return;
            }
            drop(ready);
            run(path, port, stats, keep);
        });
    }
    ports.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chorus_soloist::encode_frame;

    fn ramp(from: u64, frames: usize) -> Vec<u8> {
        (from..from + frames as u64)
            .flat_map(|n| {
                let v = (n % 8192) as f32 / 8192.0 - 0.5;
                encode_frame(v, -v)
            })
            .collect()
    }

    #[test]
    fn at_the_fifos_own_rate_and_channels_the_samples_pass_unchanged() {
        let mut converter = Converter::new(PCM_RATE, 2);
        let bytes = ramp(0, 100);
        let (frames, out) = converter.feed(&bytes);
        assert_eq!(frames, 100);
        let want: Vec<f32> = (0..100u64)
            .flat_map(|n| {
                let v = n as f32 / 8192.0 - 0.5;
                [v, -v]
            })
            .collect();
        assert_eq!(out, want.as_slice());
    }

    #[test]
    fn a_frame_split_across_two_reads_is_one_frame() {
        let mut whole = Converter::new(PCM_RATE, 2);
        let bytes = ramp(0, 50);
        let want = whole.feed(&bytes).1.to_vec();
        let mut split = Converter::new(PCM_RATE, 2);
        let mut got = Vec::new();
        let mut frames = 0;
        for piece in [&bytes[..3], &bytes[3..5], &bytes[5..213], &bytes[213..]] {
            let (n, out) = split.feed(piece);
            frames += n;
            got.extend_from_slice(out);
        }
        assert_eq!(frames, 50);
        assert_eq!(got, want);
    }

    #[test]
    fn to_48_khz_it_is_the_media_players_resampler_and_to_mono_its_remix() {
        let bytes = ramp(0, 4410);
        let mut converter = Converter::new(48_000, 1);
        let (frames, out) = converter.feed(&bytes);
        assert_eq!(frames, 4410);
        let stereo: Vec<f32> = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect();
        let mut mono = Vec::new();
        chorus_decode::remix(&stereo, 2, 1, &mut mono);
        let mut want = Vec::new();
        Resampler::new(PCM_RATE, 48_000, 1).process(&mono, &mut want);
        assert_eq!(out, want.as_slice());
        assert!(out.len() > 4600 && out.len() <= 4800, "{}", out.len());
    }

    #[test]
    fn a_sample_that_is_not_a_number_is_silence() {
        let mut converter = Converter::new(PCM_RATE, 2);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&f32::NAN.to_le_bytes());
        bytes.extend_from_slice(&f32::INFINITY.to_le_bytes());
        let (frames, out) = converter.feed(&bytes);
        assert_eq!((frames, out), (1, [0.0f32, 0.0].as_slice()));
    }
}
