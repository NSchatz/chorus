//! Rendering a run into `docs/measurements/`, and the free-run baseline later
//! runs cite.
//!
//! # Why this unit is the one place a wall clock is read
//!
//! Every number this rig publishes comes from sample indices and a declared
//! sample rate, or from timestamps a series file carries, never from a clock
//! read while the analysis runs. The single exception is the human-readable
//! date in a report's header, which is a log field: it tells a reader which
//! evening a saved report came from and nothing computes anything from it.
//! `audio-path.conf` records that exception with its reason, exactly as it does
//! for the delay-log writer, and `crates/measure/tests/no_settable_wall_clock.rs`
//! asserts that this file is the ONLY unit of this crate that reads one.
//!
//! # Naming the build
//!
//! The roadmap phase requires a report to name the build it measured, and this
//! is the mechanism: `git rev-parse HEAD` for the commit and `git status
//! --porcelain` for whether that tree was clean. A run that cannot establish
//! both refuses rather than writing a report whose provenance is a guess.
//!
//! # Why the write is atomic
//!
//! A report half written is worse than no report: it looks like evidence. The
//! body goes to a temporary file beside the destination and is renamed into
//! place, so a denied or interrupted write leaves nothing behind that could be
//! read as a measurement.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::freerun::{OffsetSeries, SlopeFit, SlopeSettings};
use crate::lag::{LagSettings, LagSummary, SIGN_CONVENTION};
use crate::wav::Capture;

/// Where every saved report goes, relative to the repository root.
pub const MEASUREMENTS_DIR: &str = "docs/measurements";

/// The committed artifact the free-run baseline is recorded in, relative to
/// [`MEASUREMENTS_DIR`].
pub const BASELINE_FILE: &str = "free-run-baseline.conf";

/// The build a report was measured against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildIdentity {
    /// The commit the measured tree was at.
    pub commit: String,
    /// Whether that tree carried no uncommitted change.
    pub clean: bool,
}

impl fmt::Display for BuildIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({})",
            self.commit,
            if self.clean {
                "clean tree"
            } else {
                "tree had uncommitted changes"
            }
        )
    }
}

/// Why a report could not be produced or saved.
#[derive(Debug, Clone, PartialEq)]
pub enum ReportError {
    /// The commit of the measured tree could not be established.
    BuildIdentityUnavailable {
        /// The tree that was asked about.
        root: PathBuf,
        /// Why the question could not be answered.
        detail: String,
    },
    /// The destination directory is not there.
    DestinationMissing {
        /// The directory that was asked for.
        path: PathBuf,
        /// What the operating system said.
        detail: String,
    },
    /// The destination exists and is not a directory.
    DestinationNotADirectory {
        /// The path that was asked for.
        path: PathBuf,
    },
    /// The destination could not be written to.
    DestinationNotWritable {
        /// The file that was being written.
        path: PathBuf,
        /// What the operating system said.
        detail: String,
    },
    /// A committed artifact could not be read.
    ArtifactUnreadable {
        /// The path that was tried.
        path: PathBuf,
        /// What the operating system said.
        detail: String,
    },
    /// A committed artifact does not carry a value this rig needs.
    ArtifactIncomplete {
        /// The path that was read.
        path: PathBuf,
        /// The key that is absent.
        key: String,
    },
}

impl ReportError {
    /// A short, stable token naming which refusal this is.
    pub fn condition(&self) -> &'static str {
        match self {
            ReportError::BuildIdentityUnavailable { .. } => "build-identity-unavailable",
            ReportError::DestinationMissing { .. } => "destination-missing",
            ReportError::DestinationNotADirectory { .. } => "destination-not-a-directory",
            ReportError::DestinationNotWritable { .. } => "destination-not-writable",
            ReportError::ArtifactUnreadable { .. } => "artifact-unreadable",
            ReportError::ArtifactIncomplete { .. } => "artifact-incomplete",
        }
    }
}

impl fmt::Display for ReportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReportError::BuildIdentityUnavailable { root, detail } => write!(
                f,
                "the build measured could not be named: '{}' did not yield a commit ({}), and \
                 a report that cannot say what it measured is not evidence",
                root.display(),
                detail
            ),
            ReportError::DestinationMissing { path, detail } => write!(
                f,
                "the report destination '{}' is not there: {}",
                path.display(),
                detail
            ),
            ReportError::DestinationNotADirectory { path } => write!(
                f,
                "the report destination '{}' exists and is not a directory",
                path.display()
            ),
            ReportError::DestinationNotWritable { path, detail } => write!(
                f,
                "the report '{}' could not be written: {}. Nothing partial has been left \
                 behind",
                path.display(),
                detail
            ),
            ReportError::ArtifactUnreadable { path, detail } => write!(
                f,
                "the committed artifact '{}' could not be read: {}",
                path.display(),
                detail
            ),
            ReportError::ArtifactIncomplete { path, key } => write!(
                f,
                "the committed artifact '{}' carries no '{}'",
                path.display(),
                key
            ),
        }
    }
}

impl std::error::Error for ReportError {}

/// Ask git what tree is being measured.
pub fn build_identity(root: &Path) -> Result<BuildIdentity, ReportError> {
    let commit = git(root, &["rev-parse", "HEAD"])?;
    let commit = commit.trim().to_string();
    if commit.len() != 40 || !commit.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(ReportError::BuildIdentityUnavailable {
            root: root.to_path_buf(),
            detail: format!("'git rev-parse HEAD' answered '{}'", commit),
        });
    }
    let status = git(root, &["status", "--porcelain"])?;
    Ok(BuildIdentity {
        commit,
        clean: status.trim().is_empty(),
    })
}

fn git(root: &Path, args: &[&str]) -> Result<String, ReportError> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| ReportError::BuildIdentityUnavailable {
            root: root.to_path_buf(),
            detail: format!("'git {}' could not be run: {}", args.join(" "), e),
        })?;
    if !output.status.success() {
        return Err(ReportError::BuildIdentityUnavailable {
            root: root.to_path_buf(),
            detail: format!(
                "'git {}' exited {}: {}",
                args.join(" "),
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The free-run baseline a later run cites.
#[derive(Debug, Clone, PartialEq)]
pub struct Baseline {
    /// The measured relative rate, in ppm.
    pub ppm: f64,
    /// The 95% confidence half-width of that rate, in ppm.
    pub half_width_ppm: f64,
    /// What it was measured from: `fixture` or `hardware`. A baseline measured
    /// from a committed fixture bounds the ESTIMATOR and says nothing about any
    /// crystal, and a reader is owed that distinction in one word.
    pub source: String,
    /// The series it was fitted from.
    pub series: String,
    /// The report that established it.
    pub established_by: String,
    /// The commit that report was written against.
    pub established_at_commit: String,
}

impl Baseline {
    /// Render the committed artifact.
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str(
            "# The free-run drift baseline, recorded by the measurement harness.\n\
             #\n\
             # This file is written by `chorus-measure free-run` and is the value later runs\n\
             # cite. It is a MEASUREMENT, not a target: the roadmap phase that asks for it is\n\
             # explicit that the baseline is what the rig measures, and no check anywhere\n\
             # compares a number against a borrowed ppm range.\n\
             #\n\
             # `source` is load bearing. A baseline fitted from a committed fixture bounds the\n\
             # ESTIMATOR against a known rate and says nothing whatever about any real\n\
             # crystal; only `source = hardware` is a statement about two clients running with\n\
             # correction disabled.\n\
             #\n\
             # Format: key = value, '#' starts a comment.\n\n",
        );
        out.push_str(&format!("source = {}\n", self.source));
        out.push_str(&format!("ppm = {:.4}\n", self.ppm));
        out.push_str(&format!("half_width_ppm = {:.4}\n", self.half_width_ppm));
        out.push_str(&format!("series = {}\n", self.series));
        out.push_str(&format!("established_by = {}\n", self.established_by));
        out.push_str(&format!(
            "established_at_commit = {}\n",
            self.established_at_commit
        ));
        out
    }

    /// Read the committed artifact.
    pub fn read(path: &Path) -> Result<Baseline, ReportError> {
        let text = std::fs::read_to_string(path).map_err(|e| ReportError::ArtifactUnreadable {
            path: path.to_path_buf(),
            detail: e.to_string(),
        })?;
        let mut values: BTreeMap<String, String> = BTreeMap::new();
        for raw in text.lines() {
            let line = match raw.find('#') {
                Some(at) => &raw[..at],
                None => raw,
            }
            .trim();
            if let Some((key, value)) = line.split_once('=') {
                values.insert(key.trim().to_string(), value.trim().to_string());
            }
        }
        let take = |key: &str| -> Result<String, ReportError> {
            values
                .get(key)
                .cloned()
                .ok_or_else(|| ReportError::ArtifactIncomplete {
                    path: path.to_path_buf(),
                    key: key.to_string(),
                })
        };
        let number = |key: &str| -> Result<f64, ReportError> {
            take(key)?
                .parse::<f64>()
                .map_err(|_| ReportError::ArtifactIncomplete {
                    path: path.to_path_buf(),
                    key: key.to_string(),
                })
        };
        Ok(Baseline {
            ppm: number("ppm")?,
            half_width_ppm: number("half_width_ppm")?,
            source: take("source")?,
            series: take("series")?,
            established_by: take("established_by")?,
            established_at_commit: take("established_at_commit")?,
        })
    }
}

/// Write `body` into `dir` as `name`, atomically, or refuse naming the path and
/// the reason.
pub fn write_report(dir: &Path, name: &str, body: &str) -> Result<PathBuf, ReportError> {
    let metadata = std::fs::metadata(dir).map_err(|e| ReportError::DestinationMissing {
        path: dir.to_path_buf(),
        detail: e.to_string(),
    })?;
    if !metadata.is_dir() {
        return Err(ReportError::DestinationNotADirectory {
            path: dir.to_path_buf(),
        });
    }
    let destination = dir.join(name);
    let temporary = dir.join(format!(".{}.{}.partial", name, std::process::id()));
    if let Err(e) = std::fs::write(&temporary, body) {
        let _ = std::fs::remove_file(&temporary);
        return Err(ReportError::DestinationNotWritable {
            path: destination,
            detail: e.to_string(),
        });
    }
    if let Err(e) = std::fs::rename(&temporary, &destination) {
        let _ = std::fs::remove_file(&temporary);
        return Err(ReportError::DestinationNotWritable {
            path: destination,
            detail: e.to_string(),
        });
    }
    Ok(destination)
}

/// A path as a report should print it: relative to the repository root when it
/// is inside it, and as given when it is not.
///
/// A saved report is read by someone who was not there, on a different machine.
/// An absolute path from the machine that ran it tells that reader nothing and
/// leaks a directory layout into a committed file; a repository-relative one is
/// a path they can actually open.
pub fn relative_display(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

/// The date, as a log field, from the one wall-clock read this crate permits.
///
/// `SystemTime` is settable, which is precisely why nothing computes anything
/// from this. It is here so a reader of a saved report knows roughly when it
/// was taken.
pub fn today_utc() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (year, month, day) = civil_from_days(seconds.div_euclid(86_400));
    format!("{:04}-{:02}-{:02}", year, month, day)
}

/// Howard Hinnant's `civil_from_days`, which turns a count of days since
/// 1970-01-01 into a proleptic Gregorian date with no table and no dependency.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}

/// Everything a lag run knows about itself.
pub struct LagRun<'a> {
    /// What to call this run in its report.
    pub label: &'a str,
    /// The capture analysed.
    pub capture: &'a Capture,
    /// The settings the run declared.
    pub settings: &'a LagSettings,
    /// The figures.
    pub summary: &'a LagSummary,
    /// The build measured.
    pub build: &'a BuildIdentity,
    /// The free-run baseline this run was compared against, where one has been
    /// recorded.
    pub baseline: Option<&'a Baseline>,
    /// The command that reproduces this run.
    pub command: &'a str,
    /// How long the analysis itself took, from a monotonic source.
    pub analysis_us: u64,
    /// The repository root, so paths print as a reader could open them.
    pub root: &'a Path,
}

/// The part of a lag report that has to be identical on a second run over the
/// same input.
///
/// Split out so the reproducibility assertion is about the FIGURES, not about
/// a header carrying a date and a wall time.
pub fn lag_figures(run: &LagRun<'_>) -> String {
    let s = run.summary;
    let capture = run.capture;
    let mut out = String::new();
    out.push_str("| figure | value |\n|---|---|\n");
    out.push_str(&format!(
        "| capture | `{}` |\n",
        relative_display(&capture.path, run.root)
    ));
    out.push_str(&format!(
        "| capture sample rate | {} Hz |\n",
        capture.sample_rate_hz
    ));
    out.push_str(&format!(
        "| capture length | {} frames, {:.1} ms |\n",
        capture.frames(),
        capture.duration_us() / 1000.0
    ));
    out.push_str(&format!(
        "| analysis windows | {} offered, {} used |\n",
        s.windows_total, s.windows_used
    ));
    out.push_str(&format!("| median lag | {:+.3} us |\n", s.median_us));
    out.push_str(&format!("| p95 lag (absolute) | {:.3} us |\n", s.p95_abs_us));
    out.push_str(&format!(
        "| maximum lag (absolute) | {:.3} us |\n",
        s.max_abs_us
    ));
    out.push_str(&format!(
        "| spread across windows | {:+.3} us to {:+.3} us |\n",
        s.min_us, s.max_us
    ));
    out.push_str(&format!(
        "| weakest correlation used | {:.4} |\n",
        s.min_coefficient
    ));
    out.push_str(&format!("| which output leads | {} |\n", s.who_leads()));
    out
}

/// Render a whole lag report.
pub fn render_lag_report(run: &LagRun<'_>) -> String {
    let settings = run.settings;
    let rate = f64::from(run.capture.sample_rate_hz);
    let us = |frames: usize| frames as f64 * 1_000_000.0 / rate;
    let mut out = String::new();

    out.push_str(&format!("# Inter-device lag: {}\n\n", run.label));
    out.push_str(&format!("Date: {}\n", today_utc()));
    out.push_str(&format!("Build measured: `{}`\n", run.build.commit));
    out.push_str(&format!(
        "Tree at that commit: {}\n",
        if run.build.clean {
            "clean"
        } else {
            "carried uncommitted changes when this run was taken"
        }
    ));
    out.push_str(&format!("Reproduce with: `{}`\n\n", run.command));

    out.push_str("## Sign convention\n\n");
    out.push_str(SIGN_CONVENTION);
    out.push_str(".\n\n");

    out.push_str("## Figures\n\n");
    out.push_str(&lag_figures(run));

    out.push_str("\n## The free-run baseline this run cites\n\n");
    match run.baseline {
        Some(baseline) => {
            out.push_str(&format!(
                "Baseline: {:+.4} ppm (+/-{:.4} ppm), measured from {}, established by \
                 `{}` at commit `{}`.\n",
                baseline.ppm,
                baseline.half_width_ppm,
                baseline.source,
                baseline.established_by,
                baseline.established_at_commit
            ));
            if baseline.source != "hardware" {
                out.push_str(
                    "\nThat baseline was fitted from a committed fixture, not from two clients \
                     running with correction disabled. It bounds this rig's estimator against a \
                     known rate. It is NOT a statement about any real crystal, and no claim \
                     about hardware drift rests on it.\n",
                );
            }
        }
        None => out.push_str(
            "No free-run baseline has been recorded, so this run cites none. That is a gap in \
             the record and not a passing result.\n",
        ),
    }

    out.push_str("\n## What the run declared before it looked at the capture\n\n");
    out.push_str("| setting | value |\n|---|---|\n");
    out.push_str(&format!(
        "| analysis window | {} frames, {:.1} ms |\n",
        settings.window_frames,
        us(settings.window_frames) / 1000.0
    ));
    out.push_str(&format!(
        "| window hop | {} frames, {:.2} ms |\n",
        settings.hop_frames,
        us(settings.hop_frames) / 1000.0
    ));
    out.push_str(&format!(
        "| lag search range | +/-{} frames, +/-{:.0} us |\n",
        settings.max_lag_frames,
        us(settings.max_lag_frames)
    ));
    out.push_str(&format!(
        "| confidence floor | {:.3} normalised correlation |\n",
        settings.confidence_floor
    ));
    out.push_str(&format!(
        "| silence floor | {:.1} dBFS |\n",
        settings.silence_floor_dbfs
    ));
    out.push_str(&format!(
        "| chirp band | {:.0} to {:.0} Hz, at least {:.2} of channel energy |\n",
        settings.chirp_band_hz.0, settings.chirp_band_hz.1, settings.chirp_band_fraction_floor
    ));
    out.push_str(&format!(
        "| windows required | {} |\n",
        settings.min_resolved_windows
    ));

    out.push_str("\n## Method\n\n");
    out.push_str(
        "Sliding-window cross-correlation of the two captured channels, with the peak located \
         on a continuum by a windowed-sinc reconstruction of the correlation and a parabolic \
         vertex on the reconstruction. `docs/decisions/0013-the-measurement-rig.md` records \
         why, and the accuracy that has been demonstrated against fixtures with known \
         ground truth.\n",
    );
    out.push_str(&format!(
        "\nAnalysis time on a monotonic clock: {:.1} ms. That figure is a diagnostic and \
         nothing derived from the capture depends on it.\n",
        run.analysis_us as f64 / 1000.0
    ));

    out.push_str("\n## What this report does not establish\n\n");
    out.push_str(
        "A capture taken from a file establishes what the estimator does with that file. \
         Whether the two ENDPOINTS were that far apart is a question about a capture taken \
         through real line outputs into one interface, and only a report over such a capture \
         answers it. `docs/verification-record.md` records which criteria of this phase are \
         operator graded for exactly that reason.\n",
    );
    out
}

/// Everything a free-run run knows about itself.
pub struct FreeRunRun<'a> {
    /// What to call this run in its report.
    pub label: &'a str,
    /// The series fitted.
    pub series: &'a OffsetSeries,
    /// The settings the run declared.
    pub settings: &'a SlopeSettings,
    /// The fit.
    pub fit: &'a SlopeFit,
    /// The build measured.
    pub build: &'a BuildIdentity,
    /// The baseline this run recorded.
    pub baseline: &'a Baseline,
    /// The command that reproduces this run.
    pub command: &'a str,
    /// The repository root, so paths print as a reader could open them.
    pub root: &'a Path,
}

/// The part of a free-run report that has to be identical on a second run.
pub fn free_run_figures(run: &FreeRunRun<'_>) -> String {
    let fit = run.fit;
    let mut out = String::new();
    out.push_str("| figure | value |\n|---|---|\n");
    out.push_str(&format!(
        "| series | `{}` |\n",
        relative_display(&run.series.path, run.root)
    ));
    out.push_str(&format!("| observations | {} |\n", fit.points));
    out.push_str(&format!("| span | {:.1} s |\n", fit.span_s));
    out.push_str(&format!("| relative rate | {:+.4} ppm |\n", fit.ppm));
    out.push_str(&format!(
        "| 95% confidence half-width | +/-{:.4} ppm |\n",
        fit.half_width_ppm
    ));
    out.push_str(&format!(
        "| residual jitter | {:.1} us RMS |\n",
        fit.residual_rms_ns / 1000.0
    ));
    out.push_str(&format!(
        "| offset at the first observation | {:+.0} ns |\n",
        fit.intercept_ns
    ));
    out
}

/// Render a whole free-run report.
pub fn render_free_run_report(run: &FreeRunRun<'_>) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Free-run drift: {}\n\n", run.label));
    out.push_str(&format!("Date: {}\n", today_utc()));
    out.push_str(&format!("Build measured: `{}`\n", run.build.commit));
    out.push_str(&format!(
        "Tree at that commit: {}\n",
        if run.build.clean {
            "clean"
        } else {
            "carried uncommitted changes when this run was taken"
        }
    ));
    out.push_str(&format!("Reproduce with: `{}`\n\n", run.command));

    out.push_str("## Figures\n\n");
    out.push_str(&free_run_figures(run));

    out.push_str("\n## What the run declared before it looked at the series\n\n");
    out.push_str("| setting | value |\n|---|---|\n");
    out.push_str(&format!(
        "| observations required | {} |\n",
        run.settings.min_points
    ));
    out.push_str(&format!(
        "| span required | {:.0} s |\n",
        run.settings.min_span_s
    ));
    out.push_str(&format!(
        "| widest publishable half-width | +/-{:.3} ppm |\n",
        run.settings.max_half_width_ppm
    ));
    if let Some(declared) = run.series.declared_rate_ppm {
        out.push_str(&format!(
            "| rate the fixture states it was generated at | {:+.4} ppm |\n",
            declared
        ));
    }
    if let Some(jitter) = run.series.declared_jitter_us {
        out.push_str(&format!(
            "| jitter the fixture states it carries | {:.1} us |\n",
            jitter
        ));
    }

    out.push_str("\n## Recorded as the baseline\n\n");
    out.push_str(&format!(
        "This slope has been recorded in `{}/{}` as the free-run baseline later runs cite, \
         with `source = {}`.\n",
        MEASUREMENTS_DIR, BASELINE_FILE, run.baseline.source
    ));
    if run.baseline.source != "hardware" {
        out.push_str(
            "\n`source = fixture` is the honest label here. This series was generated from \
             committed parameters at a known rate; the fit recovering that rate bounds the \
             ESTIMATOR and says nothing about any real crystal. A hardware baseline needs two \
             clients running with correction disabled, which is operator graded in \
             `docs/verification-record.md`.\n",
        );
    }

    out.push_str("\n## Method\n\n");
    out.push_str(
        "Ordinary least squares of relative offset against elapsed time, with the slope's own \
         95% confidence half-width from the residuals. A run publishes no ppm figure when that \
         half-width is wider than the run declared, or when the series is shorter than the run \
         declared. `docs/decisions/0013-the-measurement-rig.md` records both thresholds and \
         why they are where they are.\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_date_helper_agrees_with_known_days() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_000), (2022, 1, 8));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        // A leap day, which a wrong era calculation gets wrong by one.
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    }

    #[test]
    fn the_date_is_a_date() {
        let today = today_utc();
        assert_eq!(today.len(), 10);
        assert_eq!(today.as_bytes()[4], b'-');
    }

    #[test]
    fn a_baseline_round_trips_through_its_committed_form() {
        let baseline = Baseline {
            ppm: 37.5,
            half_width_ppm: 0.0042,
            source: "fixture".to_string(),
            series: "fixtures/measure/free-run-noiseless.offsets".to_string(),
            established_by: "docs/measurements/example.md".to_string(),
            established_at_commit: "0".repeat(40),
        };
        let dir = std::env::temp_dir().join(format!("chorus-baseline-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(BASELINE_FILE);
        std::fs::write(&path, baseline.render()).unwrap();
        let read = Baseline::read(&path).unwrap();
        assert_eq!(read, baseline);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_baseline_missing_a_value_is_refused_by_name() {
        let dir = std::env::temp_dir().join(format!("chorus-baseline-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(BASELINE_FILE);
        std::fs::write(&path, "source = fixture\nppm = 1.0\n").unwrap();
        let err = Baseline::read(&path).unwrap_err();
        assert_eq!(err.condition(), "artifact-incomplete");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_destination_is_refused_naming_the_path() {
        let missing = std::env::temp_dir().join("chorus-no-such-measurements-dir");
        let _ = std::fs::remove_dir_all(&missing);
        let err = write_report(&missing, "x.md", "body").unwrap_err();
        assert_eq!(err.condition(), "destination-missing");
        assert!(err.to_string().contains("chorus-no-such-measurements-dir"));
    }

    #[test]
    fn this_repository_can_name_the_build_it_is_measuring() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("this crate lives at crates/<name> under the repository root");
        let build = build_identity(root).expect("this checkout is a git tree");
        assert_eq!(build.commit.len(), 40);
    }
}
