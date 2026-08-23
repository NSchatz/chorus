//! Grading a delay log without the client running.
//!
//! Everything a run claims about itself is graded here, from the saved file
//! alone. That is the point of writing the file at all: a claim that can only
//! be checked by the process that made it is not evidence.
//!
//! This unit reads; it never runs beside the audio path and nothing on that
//! path depends on it.

use std::collections::BTreeMap;
use std::fmt;

/// One sample line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sample {
    /// The client's own monotonic timeline, microseconds.
    pub mono_us: u64,
    /// The device-reported delay, microseconds.
    pub delay_us: i64,
    /// Buffer occupancy, microseconds.
    pub occupancy_us: u64,
    /// Whether this sample is inside the graded interval.
    pub graded: bool,
}

/// One event line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// The client's own monotonic timeline, microseconds.
    pub mono_us: u64,
    /// What kind of event.
    pub kind: String,
    /// The remaining `key=value` fields.
    pub fields: BTreeMap<String, String>,
}

/// A parsed delay log.
#[derive(Debug, Clone, Default)]
pub struct ParsedLog {
    /// The `config` record.
    pub config: BTreeMap<String, String>,
    /// The `summary` record, if the run wrote one.
    pub summary: BTreeMap<String, String>,
    /// Every `sample` line, in file order.
    pub samples: Vec<Sample>,
    /// Every `event` line, in file order.
    pub events: Vec<Event>,
}

/// Why a log could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// One-based line number.
    pub line: usize,
    /// What was wrong.
    pub detail: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.detail)
    }
}

impl std::error::Error for ParseError {}

fn fields_of(rest: &str) -> BTreeMap<String, String> {
    rest.split_whitespace()
        .filter_map(|token| token.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// Parse a delay log.
pub fn parse(text: &str) -> Result<ParsedLog, ParseError> {
    let mut log = ParsedLog::default();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (kind, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let fields = fields_of(rest);
        let at = |key: &str| -> Result<String, ParseError> {
            fields.get(key).cloned().ok_or_else(|| ParseError {
                line: index + 1,
                detail: format!("a {} record has no {}", kind, key),
            })
        };
        match kind {
            "config" => log.config = fields,
            "summary" => log.summary = fields,
            "sample" => {
                let parse_u64 = |key: &str| -> Result<u64, ParseError> {
                    at(key)?.parse().map_err(|_| ParseError {
                        line: index + 1,
                        detail: format!("{} is not an unsigned number", key),
                    })
                };
                let parse_i64 = |key: &str| -> Result<i64, ParseError> {
                    at(key)?.parse().map_err(|_| ParseError {
                        line: index + 1,
                        detail: format!("{} is not a number", key),
                    })
                };
                log.samples.push(Sample {
                    mono_us: parse_u64("mono_us")?,
                    delay_us: parse_i64("delay_us")?,
                    occupancy_us: parse_u64("occupancy_us")?,
                    graded: at("graded")? == "1",
                });
            }
            "event" => {
                let mono_us: u64 = at("mono_us")?.parse().map_err(|_| ParseError {
                    line: index + 1,
                    detail: "mono_us is not an unsigned number".to_string(),
                })?;
                log.events.push(Event {
                    mono_us,
                    kind: at("kind")?,
                    fields,
                });
            }
            other => {
                return Err(ParseError {
                    line: index + 1,
                    detail: format!("unknown record type '{}'", other),
                })
            }
        }
    }
    Ok(log)
}

/// One thing that was checked and what it said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The check's short name.
    pub check: String,
    /// Whether it held.
    pub ok: bool,
    /// What it found, in words, always with the numbers in it.
    pub detail: String,
}

/// Everything a grading run found.
#[derive(Debug, Clone, Default)]
pub struct Report {
    /// The findings, in the order they were checked.
    pub findings: Vec<Finding>,
}

impl Report {
    fn push(&mut self, check: &str, ok: bool, detail: String) {
        self.findings.push(Finding {
            check: check.to_string(),
            ok,
            detail,
        });
    }

    /// Whether every check held.
    pub fn ok(&self) -> bool {
        self.findings.iter().all(|f| f.ok)
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for finding in &self.findings {
            writeln!(
                f,
                "{} {}: {}",
                if finding.ok { "pass" } else { "FAIL" },
                finding.check,
                finding.detail
            )?;
        }
        Ok(())
    }
}

fn number(map: &BTreeMap<String, String>, key: &str) -> Option<i64> {
    map.get(key)?.parse().ok()
}

/// Grade a parsed log.
///
/// `min_graded_seconds` is how long the graded interval has to be: 600 for the
/// ten-minute run, smaller for the shorter checks that grade the log's shape
/// rather than the run's length.
pub fn grade(log: &ParsedLog, min_graded_seconds: u64) -> Report {
    let mut report = Report::default();

    // The configured values have to be in the file, or nothing else can be
    // graded against them.
    let min_us = number(&log.config, "min_us");
    let max_us = number(&log.config, "max_us");
    let start_fill_us = number(&log.config, "start_fill_us");
    let skew_ppm = number(&log.config, "overflow_skew_ppm");
    let frames_per_chunk = number(&log.config, "frames_per_chunk");
    let rate_hz = number(&log.config, "rate_hz");

    let (min_us, max_us, start_fill_us, skew_ppm, frames_per_chunk, rate_hz) = match (
        min_us,
        max_us,
        start_fill_us,
        skew_ppm,
        frames_per_chunk,
        rate_hz,
    ) {
        (Some(a), Some(b), Some(c), Some(d), Some(e), Some(g)) => (a, b, c, d, e, g),
        _ => {
            report.push(
                "config-record",
                false,
                "the log has no complete config record, so nothing can be graded against the \
                 values the run was configured with"
                    .to_string(),
            );
            return report;
        }
    };
    report.push(
        "config-record",
        true,
        format!(
            "min_us={} max_us={} start_fill_us={} overflow_skew_ppm={}",
            min_us, max_us, start_fill_us, skew_ppm
        ),
    );

    // The three relations that keep the bounds meaningful.
    let span = max_us - min_us;
    let seconds_to_cross = if skew_ppm > 0 { span / skew_ppm } else { i64::MAX };
    report.push(
        "bounds-are-meaningful",
        min_us > 0
            && start_fill_us > min_us
            && start_fill_us < max_us
            && skew_ppm > 0
            && seconds_to_cross < 600,
        format!(
            "min_us={} > 0, {} < start_fill_us={} < {}, span {} us at {} ppm crosses in {} s \
             (has to be under 600)",
            min_us, min_us, start_fill_us, max_us, span, skew_ppm, seconds_to_cross
        ),
    );

    // Sampling rate and completeness.
    report.push(
        "samples-present",
        !log.samples.is_empty(),
        format!("{} samples", log.samples.len()),
    );
    let worst_gap = log
        .samples
        .windows(2)
        .map(|w| w[1].mono_us.saturating_sub(w[0].mono_us))
        .max()
        .unwrap_or(0);
    report.push(
        "sample-interval-no-coarser-than-one-second",
        !log.samples.is_empty() && worst_gap <= 1_000_000,
        format!("the widest gap between samples is {} us", worst_gap),
    );

    let graded: Vec<&Sample> = log.samples.iter().filter(|s| s.graded).collect();
    report.push(
        "graded-samples-present",
        !graded.is_empty(),
        format!("{} graded samples", graded.len()),
    );

    // The graded samples have to be one contiguous run: nothing is graded
    // before the first write, and nothing is graded after the drain begins.
    let first_graded = log.samples.iter().position(|s| s.graded);
    let last_graded = log.samples.iter().rposition(|s| s.graded);
    let contiguous = match (first_graded, last_graded) {
        (Some(a), Some(b)) => log.samples[a..=b].iter().all(|s| s.graded),
        _ => false,
    };
    report.push(
        "ungraded-samples-only-outside-the-graded-interval",
        contiguous,
        match (first_graded, last_graded) {
            (Some(a), Some(b)) => format!(
                "graded samples run from index {} to {} with no ungraded sample between them",
                a, b
            ),
            _ => "there is no graded interval in this log".to_string(),
        },
    );

    let graded_span_us = match (first_graded, last_graded) {
        (Some(a), Some(b)) => log.samples[b].mono_us.saturating_sub(log.samples[a].mono_us),
        _ => 0,
    };
    report.push(
        "graded-interval-long-enough",
        graded_span_us >= min_graded_seconds * 1_000_000,
        format!(
            "the graded interval is {} us and has to be at least {} us",
            graded_span_us,
            min_graded_seconds * 1_000_000
        ),
    );

    // The assertion itself.
    let outside: Vec<&&Sample> = graded
        .iter()
        .filter(|s| s.delay_us < min_us || s.delay_us > max_us)
        .collect();
    report.push(
        "delay-inside-the-bounds-for-the-whole-graded-interval",
        outside.is_empty(),
        match outside.first() {
            None => format!(
                "every one of {} graded samples has {} <= delay_us <= {}",
                graded.len(),
                min_us,
                max_us
            ),
            Some(s) => format!(
                "{} graded samples are outside the bounds, first at mono_us={} with delay_us={}",
                outside.len(),
                s.mono_us,
                s.delay_us
            ),
        },
    );

    // Occupancy is capped by the maximum plus one chunk, always, graded or
    // not: the ceiling is a memory bound and does not take a break.
    let chunk_us = if rate_hz > 0 {
        frames_per_chunk * 1_000_000 / rate_hz
    } else {
        0
    };
    let cap = max_us + chunk_us;
    let over_cap = log
        .samples
        .iter()
        .filter(|s| s.occupancy_us as i64 > cap)
        .count();
    report.push(
        "occupancy-never-past-the-maximum-plus-one-chunk",
        over_cap == 0,
        format!(
            "{} of {} samples exceed {} us (max {} us plus one {} us chunk)",
            over_cap,
            log.samples.len(),
            cap,
            max_us,
            chunk_us
        ),
    );

    // The extremes and margins the run has to report.
    match (
        number(&log.summary, "delay_min_us"),
        number(&log.summary, "delay_max_us"),
        number(&log.summary, "margin_to_min_us"),
        number(&log.summary, "margin_to_max_us"),
    ) {
        (Some(dmin), Some(dmax), Some(mmin), Some(mmax)) => {
            let observed_min = graded.iter().map(|s| s.delay_us).min().unwrap_or(0);
            let observed_max = graded.iter().map(|s| s.delay_us).max().unwrap_or(0);
            report.push(
                "reported-extremes-match-the-samples",
                (graded.is_empty() || (dmin == observed_min && dmax == observed_max))
                    && mmin == dmin - min_us
                    && mmax == max_us - dmax,
                format!(
                    "reported delay_min_us={} delay_max_us={} margin_to_min_us={} \
                     margin_to_max_us={}; samples say {} and {}",
                    dmin, dmax, mmin, mmax, observed_min, observed_max
                ),
            );
        }
        _ => report.push(
            "reported-extremes-match-the-samples",
            false,
            "the summary record does not carry both extremes and both margins".to_string(),
        ),
    }

    report
}

/// Grade the underrun count, which is a claim about the run rather than about
/// the log's shape.
pub fn grade_underruns(log: &ParsedLog, report: &mut Report) {
    match number(&log.summary, "underruns") {
        Some(n) => report.push(
            "zero-underruns",
            n == 0,
            format!("the run reported {} underruns", n),
        ),
        None => report.push(
            "zero-underruns",
            false,
            "the summary record does not carry an underrun count".to_string(),
        ),
    }
}

/// Grade that no rate change happened: what the device played out over the run
/// has to match what its nominal rate says it should have.
///
/// `tolerance_frames` allows for the run ending between two frames, and for
/// nothing else.
pub fn grade_no_rate_change(log: &ParsedLog, tolerance_frames: i64, report: &mut Report) {
    match (
        number(&log.summary, "frames_played"),
        number(&log.summary, "nominal_frames"),
    ) {
        (Some(played), Some(nominal)) => {
            let drift = played - nominal;
            report.push(
                "no-rate-change",
                drift.abs() <= tolerance_frames,
                format!(
                    "the device played {} frames where its nominal rate says {} over the same \
                     interval, a difference of {} frames (tolerance {})",
                    played, nominal, drift, tolerance_frames
                ),
            );
        }
        _ => report.push(
            "no-rate-change",
            false,
            "the summary record does not carry both the played and the nominal frame counts"
                .to_string(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log_text(samples: &str, summary: &str) -> String {
        format!(
            "# chorus delay log v1\n\
             config min_us=60000 max_us=300000 start_fill_us=120000 device_target_us=120000 \
             device=null rate_hz=48000 channels=2 sample_format=pcm_s16le frames_per_chunk=960 \
             overflow_skew_ppm=2000\n{}{}",
            samples, summary
        )
    }

    #[test]
    fn a_well_formed_log_parses_and_grades() {
        let mut samples = String::new();
        for i in 0..20 {
            samples.push_str(&format!(
                "sample mono_us={} delay_us=120000 occupancy_us=140000 graded=1\n",
                i * 100_000
            ));
        }
        let summary = "summary graded_span_us=1900000 graded_samples=20 delay_min_us=120000 \
                       delay_max_us=120000 margin_to_min_us=60000 margin_to_max_us=180000 \
                       underruns=0 discarded_overflow=0 discarded_late=0 discarded_duplicate=0 \
                       discarded_malformed=0 frames_written=91200 frames_played=91200 \
                       nominal_frames=91200\n";
        let log = parse(&log_text(&samples, summary)).unwrap();
        let mut report = grade(&log, 1);
        grade_underruns(&log, &mut report);
        grade_no_rate_change(&log, 480, &mut report);
        assert!(report.ok(), "{}", report);
    }

    #[test]
    fn one_graded_sample_outside_the_bounds_fails_the_delay_check() {
        let mut samples = String::new();
        for i in 0..20 {
            let delay = if i == 7 { 40_000 } else { 120_000 };
            samples.push_str(&format!(
                "sample mono_us={} delay_us={} occupancy_us=140000 graded=1\n",
                i * 100_000,
                delay
            ));
        }
        let log = parse(&log_text(&samples, "")).unwrap();
        let report = grade(&log, 1);
        let finding = report
            .findings
            .iter()
            .find(|f| f.check == "delay-inside-the-bounds-for-the-whole-graded-interval")
            .unwrap();
        assert!(!finding.ok, "{}", report);
        assert!(finding.detail.contains("40000"));
    }

    #[test]
    fn an_ungraded_sample_in_the_middle_of_the_graded_interval_fails() {
        let samples = "sample mono_us=0 delay_us=120000 occupancy_us=140000 graded=1\n\
                       sample mono_us=100000 delay_us=120000 occupancy_us=140000 graded=0\n\
                       sample mono_us=200000 delay_us=120000 occupancy_us=140000 graded=1\n";
        let log = parse(&log_text(samples, "")).unwrap();
        let report = grade(&log, 0);
        let finding = report
            .findings
            .iter()
            .find(|f| f.check == "ungraded-samples-only-outside-the-graded-interval")
            .unwrap();
        assert!(!finding.ok);
    }

    #[test]
    fn a_gap_wider_than_a_second_fails_the_interval_check() {
        let samples = "sample mono_us=0 delay_us=120000 occupancy_us=140000 graded=1\n\
                       sample mono_us=2000000 delay_us=120000 occupancy_us=140000 graded=1\n";
        let log = parse(&log_text(samples, "")).unwrap();
        let report = grade(&log, 0);
        let finding = report
            .findings
            .iter()
            .find(|f| f.check == "sample-interval-no-coarser-than-one-second")
            .unwrap();
        assert!(!finding.ok);
        assert!(finding.detail.contains("2000000"));
    }

    #[test]
    fn occupancy_past_the_maximum_plus_one_chunk_fails() {
        let samples = "sample mono_us=0 delay_us=120000 occupancy_us=400000 graded=1\n";
        let log = parse(&log_text(samples, "")).unwrap();
        let report = grade(&log, 0);
        let finding = report
            .findings
            .iter()
            .find(|f| f.check == "occupancy-never-past-the-maximum-plus-one-chunk")
            .unwrap();
        assert!(!finding.ok);
    }

    #[test]
    fn bounds_no_run_could_cross_are_reported_as_such() {
        let text = "config min_us=60000 max_us=60000000 start_fill_us=120000 \
                    device_target_us=120000 device=null rate_hz=48000 channels=2 \
                    sample_format=pcm_s16le frames_per_chunk=960 overflow_skew_ppm=2000\n\
                    sample mono_us=0 delay_us=120000 occupancy_us=140000 graded=1\n";
        let log = parse(text).unwrap();
        let report = grade(&log, 0);
        let finding = report
            .findings
            .iter()
            .find(|f| f.check == "bounds-are-meaningful")
            .unwrap();
        assert!(!finding.ok);
    }

    #[test]
    fn an_unknown_record_type_is_a_parse_error_rather_than_a_shrug() {
        let err = parse("nonsense a=b\n").unwrap_err();
        assert_eq!(err.line, 1);
        assert!(err.detail.contains("nonsense"));
    }
}
