//! The jitter distribution of a series, and the report it is saved as.
//!
//! # What this measures, and what it cannot
//!
//! A series of relative offset or inter-arrival observations goes in, and the
//! spread of that series comes out: median, p95, maximum and peak to peak,
//! every one of them a deviation from the series' own median. Nothing here
//! corrects, disciplines or tunes anything, and nothing here holds a view about
//! whether a number is good. That is the same rule the rest of this crate
//! follows and `crates/measure/src/lib.rs` states it.
//!
//! The reason `chorus#WIFI-7` wants it is its third assertion: "WHEN a Wi-Fi
//! endpoint is measured with the platform default left in place THE SYSTEM
//! SHALL record the resulting jitter in docs/measurements/ rather than tune the
//! servo against it". Recording it is this module. Tuning a servo against it is
//! what BRIEF.md section 9 forbids, and no constant in `config/sync.conf` moved
//! for this phase.
//!
//! # Why the mode has to be known
//!
//! A jitter figure taken with modem sleep disabled and one taken with the
//! platform default left in place are different measurements of different
//! systems, and the whole point of the phase is the difference between them. A
//! report that did not say which mode was in force would be a number nobody
//! could use, so this refuses to write one: [`PowerSaveMode::parse`] accepts
//! only a mode the endpoint can actually be in, and there is no default.
//!
//! # Time
//!
//! Every figure comes from the timestamps the series file carries, which
//! `docs/protocol.md` already requires to be nanoseconds from a monotonic
//! source. Nothing here reads a clock.

use std::fmt;
use std::path::Path;

use crate::freerun::OffsetSeries;
use crate::report::{relative_display, today_utc, BuildIdentity};

/// The Wi-Fi power save mode a run was taken with.
///
/// The endpoint's own vocabulary, from `firmware/include/chorus/wifi.h`, so a
/// saved report and a published telemetry line say the same word about the same
/// thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerSaveMode {
    /// `WIFI_PS_NONE`: modem sleep disabled entirely.
    None,
    /// `WIFI_PS_MIN_MODEM`: the platform default, which wakes every DTIM.
    MinModem,
    /// `WIFI_PS_MAX_MODEM`: wakes every listen interval.
    MaxModem,
}

/// Every mode a report may name, in the order they are listed.
pub const POWER_SAVE_MODES: &[PowerSaveMode] = &[
    PowerSaveMode::None,
    PowerSaveMode::MinModem,
    PowerSaveMode::MaxModem,
];

impl PowerSaveMode {
    /// The word a report and a telemetry line both use.
    pub fn name(self) -> &'static str {
        match self {
            PowerSaveMode::None => "none",
            PowerSaveMode::MinModem => "min-modem",
            PowerSaveMode::MaxModem => "max-modem",
        }
    }

    /// The platform's own spelling, for a reader who has the ESP-IDF reference
    /// open beside the report.
    pub fn platform_name(self) -> &'static str {
        match self {
            PowerSaveMode::None => "WIFI_PS_NONE",
            PowerSaveMode::MinModem => "WIFI_PS_MIN_MODEM",
            PowerSaveMode::MaxModem => "WIFI_PS_MAX_MODEM",
        }
    }

    /// Whether this is the mode a platform is in when nobody set one.
    ///
    /// "The default Modem-sleep mode is WIFI_PS_MIN_MODEM", from the ESP-IDF
    /// Wi-Fi power save guide, quoted in
    /// `docs/decisions/0021-the-wireless-tier.md`.
    pub fn is_platform_default(self) -> bool {
        self == PowerSaveMode::MinModem
    }

    /// Parse a mode, or `None` for a word no endpoint can be in.
    ///
    /// There is deliberately no default and `unknown` is deliberately not a
    /// mode: a report whose mode is not known is the report this refuses to
    /// write.
    pub fn parse(text: &str) -> Option<PowerSaveMode> {
        POWER_SAVE_MODES.iter().copied().find(|m| m.name() == text)
    }

    /// Every mode, as a refusal names them.
    pub fn permitted() -> String {
        POWER_SAVE_MODES
            .iter()
            .map(|m| m.name())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl fmt::Display for PowerSaveMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The transports a report may name.
///
/// The same two words `config/transport.conf` commits, written here rather than
/// depended on so this crate stays a rig with no view about the system it
/// measures. `crates/measure/tests/report_shape.rs` asserts the two lists are
/// the same words, so they cannot drift apart.
pub const TRANSPORTS: &[&str] = &["wired", "wireless"];

/// The fewest observations a distribution is computed from.
///
/// The same floor `config/measure.conf`'s `free_run_min_points` puts under the
/// slope fit, and for the same reason: a median over a handful of observations
/// is not a distribution. It is repeated here as a constant rather than read
/// from that file because it is a property of "is this a distribution at all"
/// and not a threshold of this rig's analysis; `report_shape.rs` asserts the two
/// agree.
pub const MIN_OBSERVATIONS: usize = 30;

/// What a series' spread is.
///
/// Every figure is a deviation from the series' OWN median, in microseconds. A
/// deviation from zero would be a claim that zero is where the offsets should
/// be, which is a statement about a servo and not about jitter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JitterSummary {
    /// Observations analysed.
    pub observations: usize,
    /// The span they cover, in seconds.
    pub span_s: f64,
    /// The median of the series itself, in microseconds. Every deviation below
    /// is taken from this.
    pub centre_us: f64,
    /// Median absolute deviation, in microseconds.
    pub median_us: f64,
    /// The 95th percentile of the absolute deviations.
    pub p95_us: f64,
    /// The largest absolute deviation.
    pub max_us: f64,
    /// Largest minus smallest observation, in microseconds.
    pub peak_to_peak_us: f64,
    /// Root mean square of the deviations.
    pub rms_us: f64,
}

/// Why no jitter figure was published.
#[derive(Debug, Clone, PartialEq)]
pub enum JitterError {
    /// The series is too short to be a distribution.
    SeriesTooShort {
        /// Observations it holds.
        observations: usize,
        /// Observations a distribution needs.
        required: usize,
    },
    /// The timestamps do not increase, so they are not from a monotonic source.
    TimestampsNotMonotonic {
        /// The observation, counting from zero, that goes backwards.
        at: usize,
        /// The timestamp before it.
        previous_ns: i64,
        /// The timestamp that went backwards.
        this_ns: i64,
    },
    /// The power save mode in force is not one an endpoint can be in.
    ///
    /// The refusal this phase exists for: a jitter figure whose mode is not
    /// known is a number nobody can use, because the whole question is the
    /// difference between the modes.
    ModeNotKnown {
        /// The word that was offered, or the empty string where none was.
        offered: String,
        /// Every mode a report may name.
        permitted: String,
    },
    /// The transport is not one the committed configuration names.
    TransportNotKnown {
        /// The word that was offered.
        offered: String,
        /// Every transport a report may name.
        permitted: String,
    },
}

impl JitterError {
    /// A short, stable token naming which refusal this is.
    pub fn condition(&self) -> &'static str {
        match self {
            JitterError::SeriesTooShort { .. } => "series-too-short",
            JitterError::TimestampsNotMonotonic { .. } => "timestamps-not-monotonic",
            JitterError::ModeNotKnown { .. } => "power-save-mode-not-known",
            JitterError::TransportNotKnown { .. } => "transport-not-known",
        }
    }
}

impl fmt::Display for JitterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JitterError::SeriesTooShort {
                observations,
                required,
            } => write!(
                f,
                "the series carries {} observations and a distribution needs at least {}; no \
                 jitter figure is published",
                observations, required
            ),
            JitterError::TimestampsNotMonotonic {
                at,
                previous_ns,
                this_ns,
            } => write!(
                f,
                "observation {} carries timestamp {} ns after {} ns; these timestamps are \
                 required to come from a monotonic source and these do not",
                at, this_ns, previous_ns
            ),
            JitterError::ModeNotKnown { offered, permitted } => write!(
                f,
                "the Wi-Fi power save mode in force is {}, and a report may name {}. NO REPORT IS \
                 WRITTEN: a jitter figure taken with modem sleep disabled and one taken with the \
                 platform default left in place are measurements of different systems, and the \
                 difference between them is the whole question",
                if offered.is_empty() {
                    "not stated".to_string()
                } else {
                    format!("'{}', which is not a mode an endpoint can be in", offered)
                },
                permitted
            ),
            JitterError::TransportNotKnown { offered, permitted } => write!(
                f,
                "the transport '{}' is not one config/transport.conf names, which are {}. No \
                 report is written",
                offered, permitted
            ),
        }
    }
}

impl std::error::Error for JitterError {}

/// The spread of a series, or a refusal.
pub fn analyse(series: &OffsetSeries) -> Result<JitterSummary, JitterError> {
    for (at, pair) in series.observations.windows(2).enumerate() {
        if pair[1].t_ns < pair[0].t_ns {
            return Err(JitterError::TimestampsNotMonotonic {
                at: at + 1,
                previous_ns: pair[0].t_ns,
                this_ns: pair[1].t_ns,
            });
        }
    }
    let observations = series.observations.len();
    if observations < MIN_OBSERVATIONS {
        return Err(JitterError::SeriesTooShort {
            observations,
            required: MIN_OBSERVATIONS,
        });
    }

    let mut offsets_us: Vec<f64> = series
        .observations
        .iter()
        .map(|o| o.offset_ns as f64 / 1_000.0)
        .collect();
    offsets_us.sort_by(|a, b| a.partial_cmp(b).expect("a series carries no NaN"));
    let centre_us = median_of(&offsets_us);
    let peak_to_peak_us = offsets_us[observations - 1] - offsets_us[0];

    let mut deviations: Vec<f64> = offsets_us.iter().map(|o| (o - centre_us).abs()).collect();
    deviations.sort_by(|a, b| a.partial_cmp(b).expect("a deviation is not NaN"));
    let median_us = median_of(&deviations);
    let p95_us = percentile_of(&deviations, 0.95);
    let max_us = deviations[observations - 1];
    let rms_us = (deviations.iter().map(|d| d * d).sum::<f64>() / observations as f64).sqrt();

    Ok(JitterSummary {
        observations,
        span_s: series.span_s(),
        centre_us,
        median_us,
        p95_us,
        max_us,
        peak_to_peak_us,
        rms_us,
    })
}

/// The median of a sorted, non-empty slice.
fn median_of(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

/// The nearest-rank percentile of a sorted, non-empty slice.
///
/// Nearest rank rather than an interpolation, so the figure a report carries is
/// always an observation that was actually taken.
fn percentile_of(sorted: &[f64], fraction: f64) -> f64 {
    let n = sorted.len();
    let rank = (fraction * n as f64).ceil() as usize;
    sorted[rank.clamp(1, n) - 1]
}

/// Everything a jitter run knows about itself.
pub struct JitterRun<'a> {
    /// What to call this run in its report.
    pub label: &'a str,
    /// The series analysed.
    pub series: &'a OffsetSeries,
    /// The figures.
    pub summary: &'a JitterSummary,
    /// The power save mode that was in force. There is no `None` here on
    /// purpose: a run with no known mode never gets this far.
    pub mode: PowerSaveMode,
    /// The transport the series was taken over.
    pub transport: &'a str,
    /// Whether the series came from a committed fixture rather than from a
    /// radio. A modelled series bounds the ANALYSIS and says nothing about any
    /// radio, and a reader is owed that distinction in one word.
    pub from_fixture: bool,
    /// The build measured.
    pub build: &'a BuildIdentity,
    /// The command that reproduces this run.
    pub command: &'a str,
    /// The repository root, so paths print as a reader could open them.
    pub root: &'a Path,
}

/// The part of a jitter report that has to be identical on a second run over
/// the same input.
pub fn jitter_figures(run: &JitterRun<'_>) -> String {
    let s = run.summary;
    let mut out = String::new();
    out.push_str("| figure | value |\n|---|---|\n");
    out.push_str(&format!(
        "| series | `{}` |\n",
        relative_display(&run.series.path, run.root)
    ));
    out.push_str(&format!("| power save mode in force | {} ({}) |\n",
        run.mode.name(), run.mode.platform_name()));
    out.push_str(&format!("| transport | {} |\n", run.transport));
    out.push_str(&format!(
        "| observations | {} over {:.1} s |\n",
        s.observations, s.span_s
    ));
    out.push_str(&format!("| centre of the series | {:+.1} us |\n", s.centre_us));
    out.push_str(&format!("| median deviation | {:.1} us |\n", s.median_us));
    out.push_str(&format!("| p95 deviation | {:.1} us |\n", s.p95_us));
    out.push_str(&format!("| maximum deviation | {:.1} us |\n", s.max_us));
    out.push_str(&format!("| peak to peak | {:.1} us |\n", s.peak_to_peak_us));
    out.push_str(&format!("| RMS deviation | {:.1} us |\n", s.rms_us));
    out
}

/// Render a whole jitter report.
pub fn render_jitter_report(run: &JitterRun<'_>) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# Wireless jitter: {}, power save {}\n\n",
        run.label,
        run.mode.name()
    ));
    out.push_str(&format!("Date: {}\n", today_utc()));
    out.push_str(&format!("Build measured: `{}`\n", run.build.commit));
    out.push_str(&format!("Tree at that commit: {}\n", run.build.tree_state()));
    out.push_str(&format!(
        "Power save mode in force: **{}** (`{}`){}\n",
        run.mode.name(),
        run.mode.platform_name(),
        if run.mode.is_platform_default() {
            ", which is the platform default left in place"
        } else {
            ", which is not the platform default"
        }
    ));
    out.push_str(&format!("Transport: **{}**\n", run.transport));
    out.push_str(&format!("Reproduce with: `{}`\n\n", run.command));

    out.push_str("## Figures\n\n");
    out.push_str(&jitter_figures(run));

    out.push_str("\n## What the series says about itself\n\n");
    if let Some(declared) = run.series.declared_jitter_us {
        out.push_str(&format!(
            "The series states it carries {:.1} us of jitter.\n",
            declared
        ));
    } else {
        out.push_str("The series states no jitter level of its own.\n");
    }
    if let Some(rate) = run.series.declared_rate_ppm {
        out.push_str(&format!(
            "\nIt states a relative rate of {:+.4} ppm, which the figures above do not use: every \
             deviation is taken from the series' own median, so a steady drift moves the centre \
             and not the spread.\n",
            rate
        ));
    }

    out.push_str("\n## Method\n\n");
    out.push_str(
        "Every figure is the deviation of an observation from the series' OWN median, in \
         microseconds, with the percentile taken by nearest rank so that every figure printed is \
         an observation that was actually taken. A deviation from zero would be a claim that zero \
         is where the offsets should be, which is a statement about a servo and not about \
         jitter.\n",
    );
    out.push_str(
        "\nNothing here corrects, disciplines or tunes anything. `config/sync.conf` holds the \
         servo constants SYNC-4 fixed and this phase moved none of them: BRIEF.md section 9 is \
         explicit that the answer to Wi-Fi jitter is a bigger buffer or a wired zone, never servo \
         aggression.\n",
    );

    out.push_str("\n## What this report does not establish\n\n");
    if run.from_fixture {
        out.push_str(
            "**THIS SERIES IS A COMMITTED FIXTURE AND NOT A MEASUREMENT.** It was generated from \
             committed parameters at a stated jitter level, so what the figures above establish \
             is what this analysis does with that file, and nothing whatever about any radio, any \
             access point or any room. No claim about Wi-Fi rests on it.\n",
        );
        out.push_str(
            "\nA measured series needs an ESP32-S3 endpoint on a real wireless link, a second \
             endpoint in another room and the RIG-3 capture rig, which is what \
             `tools/wireless-characterization-run.sh` refuses by name without. The criteria that \
             would be answered by such a run are operator graded, and \
             `docs/verification-record.md` records them as NOT passed.\n",
        );
    } else {
        out.push_str(
            "This series was taken over a real link. It establishes what the jitter was during \
             that run, on that link, with that access point, and it is not a general statement \
             about Wi-Fi. Whether a wireless zone holds the multiroom bound is a separate \
             question answered by a capture of two endpoints' line outputs, not by this series.\n",
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::freerun::Observation;
    use std::path::PathBuf;

    fn a_series(offsets_ns: &[i64]) -> OffsetSeries {
        OffsetSeries {
            path: PathBuf::from("mem"),
            label: "test".to_string(),
            declared_rate_ppm: None,
            declared_jitter_us: None,
            observations: offsets_ns
                .iter()
                .enumerate()
                .map(|(n, offset_ns)| Observation {
                    t_ns: n as i64 * 500_000_000,
                    offset_ns: *offset_ns,
                })
                .collect(),
        }
    }

    #[test]
    fn a_flat_series_has_no_jitter() {
        let summary = analyse(&a_series(&[7_000; 40])).unwrap();
        assert_eq!(summary.observations, 40);
        assert_eq!(summary.centre_us, 7.0);
        assert_eq!(summary.median_us, 0.0);
        assert_eq!(summary.p95_us, 0.0);
        assert_eq!(summary.max_us, 0.0);
        assert_eq!(summary.peak_to_peak_us, 0.0);
    }

    #[test]
    fn the_spread_is_measured_from_the_series_own_centre() {
        // Forty observations a microsecond either side of a centre that sits a
        // whole millisecond away from zero. Every figure below is about the
        // microsecond and not about the millisecond: a spread taken from zero
        // would report 1000 us of centre as jitter, which is what this test
        // exists to catch.
        let offsets: Vec<i64> = (0..40)
            .map(|n| 1_000_000 + if n % 2 == 0 { 1_000 } else { -1_000 })
            .collect();
        let summary = analyse(&a_series(&offsets)).unwrap();
        assert_eq!(summary.centre_us, 1_000.0, "{:?}", summary);
        assert_eq!(summary.median_us, 1.0, "{:?}", summary);
        assert_eq!(summary.max_us, 1.0, "{:?}", summary);
        assert_eq!(summary.peak_to_peak_us, 2.0, "{:?}", summary);
        assert!(
            summary.max_us < summary.centre_us / 100.0,
            "the spread is a microsecond and the centre is a millisecond: {:?}",
            summary
        );
    }

    #[test]
    fn a_series_too_short_to_be_a_distribution_is_refused() {
        let err = analyse(&a_series(&[0; 10])).unwrap_err();
        assert_eq!(err.condition(), "series-too-short");
        assert!(err.to_string().contains("no jitter figure is published"));
    }

    #[test]
    fn a_series_whose_timestamps_go_backwards_is_refused() {
        let mut series = a_series(&[0; 40]);
        series.observations[20].t_ns -= 5_000_000_000;
        let err = analyse(&series).unwrap_err();
        assert_eq!(err.condition(), "timestamps-not-monotonic");
    }

    #[test]
    fn only_a_mode_an_endpoint_can_be_in_parses() {
        assert_eq!(PowerSaveMode::parse("none"), Some(PowerSaveMode::None));
        assert_eq!(
            PowerSaveMode::parse("min-modem"),
            Some(PowerSaveMode::MinModem)
        );
        assert_eq!(
            PowerSaveMode::parse("max-modem"),
            Some(PowerSaveMode::MaxModem)
        );
        assert_eq!(PowerSaveMode::parse("unknown"), None);
        assert_eq!(PowerSaveMode::parse(""), None);
        assert_eq!(PowerSaveMode::parse("WIFI_PS_NONE"), None);
        assert!(PowerSaveMode::MinModem.is_platform_default());
        assert!(!PowerSaveMode::None.is_platform_default());
    }

    #[test]
    fn the_percentile_is_an_observation_that_was_taken() {
        let sorted: Vec<f64> = (1..=100).map(|n| n as f64).collect();
        assert_eq!(percentile_of(&sorted, 0.95), 95.0);
        assert_eq!(percentile_of(&sorted, 1.0), 100.0);
        assert_eq!(percentile_of(&sorted, 0.0), 1.0);
    }
}
