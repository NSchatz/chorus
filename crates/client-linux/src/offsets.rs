//! The offsets series: one `t_ns offset_ns` observation per sync tick, in the
//! `.offsets` format `crates/measure/src/freerun.rs` reads (audit A-4, A-13).
//!
//! `t_ns` is the tick's instant on the SERVER timeline (the client's monotonic
//! now plus the filtered offset in use), and `offset_ns` is the playout error
//! the sync loop formed at that tick: how far this endpoint's audible output is
//! from where the server timeline puts it. Two clients' series on the same
//! server can therefore be lined up by `chorus-measure pair`, and with
//! correction disabled (`--free-run`) the slope of their difference is the
//! relative rate of the two DACs: the free-run baseline RIG-3 asks for.
//!
//! The header says which mode the series was taken in, `correction = disabled`
//! or `correction = enabled`, and `chorus-measure pair` refuses anything but
//! `disabled`. A series with the servo running is the servo's residual, which is
//! what the wireless characterization's jitter figure is taken over, and never a
//! baseline.
//!
//! Every value here is monotonic-derived. Nothing in this unit reads a clock.

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

/// Writes one run's offsets series.
pub struct OffsetsWriter {
    out: BufWriter<File>,
    observations: u64,
}

impl OffsetsWriter {
    /// Create the series and write its header.
    pub fn open<P: AsRef<Path>>(path: P, label: &str, free_run: bool) -> io::Result<Self> {
        let mut out = BufWriter::new(File::create(path)?);
        if free_run {
            writeln!(
                out,
                "# chorus-client --free-run: correction DISABLED, a free-run series. The\n\
                 # endpoint's DAC walked at its own rate and nothing was corrected."
            )?;
        } else {
            writeln!(
                out,
                "# chorus-client --offsets-out with correction ENABLED: the servo's residual,\n\
                 # which is a jitter series and never a free-run baseline."
            )?;
        }
        writeln!(
            out,
            "# t_ns: the tick's instant on the server timeline; offset_ns: the playout error\n\
             # the sync loop formed then (positive: audible output ahead of the timeline).\n\
             #\n\
             # Format: key = value, then [observations], then one 't_ns offset_ns' pair per line.\n"
        )?;
        writeln!(out, "label = {}", label)?;
        writeln!(
            out,
            "correction = {}",
            if free_run { "disabled" } else { "enabled" }
        )?;
        writeln!(out, "time_base = server")?;
        writeln!(out, "quantity = playout-error")?;
        writeln!(out, "\n[observations]")?;
        out.flush()?;
        Ok(OffsetsWriter {
            out,
            observations: 0,
        })
    }

    /// Record one observation, flushed at once: a bench script may fetch the
    /// series while the run is still going, and at two lines a second the
    /// flush costs nothing.
    pub fn observe(&mut self, server_now_ns: i64, error_ns: f64) -> io::Result<()> {
        self.observations += 1;
        writeln!(self.out, "{} {}", server_now_ns, error_ns.round() as i64)?;
        self.out.flush()
    }

    /// Observations written so far.
    pub fn observations(&self) -> u64 {
        self.observations
    }

    /// Flush what has been written so far.
    pub fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_series_carries_its_mode_and_time_base() {
        let dir = std::env::temp_dir().join(format!("chorus-offsets-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.offsets");
        let mut w = OffsetsWriter::open(&path, "endpoint-a", true).unwrap();
        w.observe(1_000_000_000, 12.4).unwrap();
        w.observe(1_500_000_000, -7.6).unwrap();
        w.flush().unwrap();
        assert_eq!(w.observations(), 2);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\ncorrection = disabled\n"), "{}", text);
        assert!(text.contains("\ntime_base = server\n"), "{}", text);
        assert!(
            text.ends_with("[observations]\n1000000000 12\n1500000000 -8\n"),
            "{}",
            text
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
