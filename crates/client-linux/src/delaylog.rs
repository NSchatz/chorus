//! The delay log: what a run leaves behind for someone who did not run it.
//!
//! # This unit is deliberately off the audio path
//!
//! It is the one exclusion `audio-path.conf` records, and the reason is
//! visible in [`DelayLog::open`]: the header line carries a settable
//! wall-clock time, because a human reading a saved log wants to know which
//! evening it came from, and no monotonic number can tell them. Nothing in
//! this file is read by the chunker, the transport, the buffer or the playout
//! loop; it only ever writes. That is what makes the exclusion safe rather
//! than convenient.
//!
//! Every value the log grades comes from the client's own monotonic timeline
//! and from the device. The wall clock appears once, in the header, and no
//! sample carries it.
//!
//! # Format
//!
//! Line oriented, whitespace separated `key=value`, so a script can parse it
//! with `split` and nothing else, and a human can read it without a tool.
//!
//! ```text
//! # chorus delay log v1
//! config min_us=60000 max_us=300000 start_fill_us=120000 ...
//! sample mono_us=1234567 delay_us=119958 occupancy_us=139958 graded=1
//! event  mono_us=1234999 kind=zone zone=mid occupancy_us=140000
//! summary graded_span_us=600123 delay_min_us=118000 ...
//! ```
//!
//! The event kinds a run writes: `start-fill` (the first write, and the fill it
//! carried), `zone` (occupancy moved between zones), `bound-crossing` (the
//! maximum was reached), `underrun` (the device said so), `graded-close` (the
//! graded interval closed, and why) and `drain-begin`; and, from the sync loop,
//! `sync` (the telemetry line: the offset in use, the round trip of the sample
//! it came from, half that round trip as the bound, and whether it is stale),
//! `sync-exchange` (one admitted), `sync-discard` (one thrown away, with the
//! reason), `correction` (a fine correction, and whether the clamp bit),
//! `hard-resync` (a step, with the error it answered), `sync-stale` (the offset
//! aged past its limit), `sync-no-device-delay` (the device answered zero,
//! which is not a distance to a DAC, so nothing was corrected) and
//! `sync-resume` (a mute ran out); and, from the control plane,
//! `zone-gain` (the zone's volume or mute changed what this endpoint is
//! multiplying its samples by, with the new gain and the state message it came
//! from). A reader that does
//! not know a kind can ignore it: the grader keys on the ones it needs and
//! passes the rest through, which is why a new kind is not a format version.
//!
//! Note that `zone` and `zone-gain` are about different things and the older
//! name is the confusing one: `zone` is a BUFFER OCCUPANCY zone, which this log
//! has carried since SOUND-2, and `zone-gain` is a room. Renaming the older one
//! would break every committed log and every grader that reads it, so the newer
//! kind carries the qualifier instead.

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::time::SystemTime;

/// The version stamp at the top of every log this client writes.
pub const FORMAT_VERSION: &str = "chorus delay log v1";

/// Writes one run's delay log.
pub struct DelayLog {
    out: BufWriter<File>,
}

/// What the header records about a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogHeader {
    /// Minimum bound, microseconds.
    pub min_us: u64,
    /// Maximum bound, microseconds.
    pub max_us: u64,
    /// Start fill, microseconds.
    pub start_fill_us: u64,
    /// Device delay the playout loop holds, microseconds.
    pub device_target_us: u64,
    /// Device the run played through.
    pub device: String,
    /// Frames per second.
    pub rate_hz: u32,
    /// Channels.
    pub channels: u16,
    /// Sample format name.
    pub sample_format: String,
    /// Frames in a full chunk.
    pub frames_per_chunk: u64,
    /// The rate difference the overflow verification applies, ppm.
    pub overflow_skew_ppm: u64,
}

impl DelayLog {
    /// Create the log and write its header.
    ///
    /// The header, and only the header, carries a wall-clock stamp: a saved
    /// log that cannot say which evening it came from is much harder to use,
    /// and this unit is excluded from the audio path precisely so that it can
    /// carry one.
    pub fn open<P: AsRef<Path>>(path: P, header: &LogHeader) -> io::Result<DelayLog> {
        let file = File::create(path)?;
        let mut out = BufWriter::new(file);
        writeln!(out, "# {}", FORMAT_VERSION)?;
        writeln!(
            out,
            "# written by the delay-log writer, which is excluded from the audio or timestamp \
             path; see audio-path.conf"
        )?;
        writeln!(out, "# started_wall_unix_s={}", wall_clock_seconds())?;
        writeln!(
            out,
            "config min_us={} max_us={} start_fill_us={} device_target_us={} device={} \
             rate_hz={} channels={} sample_format={} frames_per_chunk={} overflow_skew_ppm={}",
            header.min_us,
            header.max_us,
            header.start_fill_us,
            header.device_target_us,
            header.device,
            header.rate_hz,
            header.channels,
            header.sample_format,
            header.frames_per_chunk,
            header.overflow_skew_ppm
        )?;
        out.flush()?;
        Ok(DelayLog { out })
    }

    /// Record one sample.
    ///
    /// `mono_us` comes from the client's own monotonic timeline. `graded` says
    /// whether this sample falls inside the graded interval; a sample outside
    /// it is recorded and asserted about by nothing.
    pub fn sample(
        &mut self,
        mono_us: u64,
        delay_us: i64,
        occupancy_us: u64,
        graded: bool,
    ) -> io::Result<()> {
        writeln!(
            self.out,
            "sample mono_us={} delay_us={} occupancy_us={} graded={}",
            mono_us,
            delay_us,
            occupancy_us,
            u8::from(graded)
        )
    }

    /// Record something that happened, with the fields that make it checkable.
    pub fn event(&mut self, mono_us: u64, kind: &str, detail: &str) -> io::Result<()> {
        writeln!(
            self.out,
            "event mono_us={} kind={} {}",
            mono_us, kind, detail
        )
    }

    /// Record the run's summary line.
    #[allow(clippy::too_many_arguments)]
    pub fn summary(&mut self, fields: &LogSummary) -> io::Result<()> {
        writeln!(
            self.out,
            "summary graded_span_us={} graded_samples={} delay_min_us={} delay_max_us={} \
             margin_to_min_us={} margin_to_max_us={} underruns={} discarded_overflow={} \
             discarded_late={} discarded_duplicate={} discarded_malformed={} \
             frames_written={} frames_played={} nominal_frames={}",
            fields.graded_span_us,
            fields.graded_samples,
            fields.delay_min_us,
            fields.delay_max_us,
            fields.margin_to_min_us,
            fields.margin_to_max_us,
            fields.underruns,
            fields.discarded_overflow,
            fields.discarded_late,
            fields.discarded_duplicate,
            fields.discarded_malformed,
            fields.frames_written,
            fields.frames_played,
            fields.nominal_frames
        )?;
        self.out.flush()
    }

    /// Flush what has been written so far.
    pub fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

/// The end-of-run numbers, including the extremes and margins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LogSummary {
    /// Length of the graded interval, microseconds.
    pub graded_span_us: u64,
    /// Samples inside it.
    pub graded_samples: u64,
    /// Smallest device-reported delay seen inside it.
    pub delay_min_us: i64,
    /// Largest device-reported delay seen inside it.
    pub delay_max_us: i64,
    /// Distance from the smallest delay to the minimum bound.
    pub margin_to_min_us: i64,
    /// Distance from the largest delay to the maximum bound.
    pub margin_to_max_us: i64,
    /// Underruns counted from the device's own signal.
    pub underruns: u64,
    /// Chunks discarded at the maximum bound.
    pub discarded_overflow: u64,
    /// Chunks discarded as late.
    pub discarded_late: u64,
    /// Chunks discarded as duplicates.
    pub discarded_duplicate: u64,
    /// Frames discarded as malformed.
    pub discarded_malformed: u64,
    /// Frames handed to the device.
    pub frames_written: u64,
    /// Frames the device actually played out.
    pub frames_played: u64,
    /// Frames the device's nominal rate says should have played over the run.
    pub nominal_frames: u64,
}

/// Seconds since the Unix epoch, for the header line only.
///
/// The one settable-clock read in this repository, in the one unit that is
/// excluded from the audio or timestamp path, used for nothing but a human
/// reading a saved file.
fn wall_clock_seconds() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
