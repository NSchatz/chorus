//! The Prometheus exporter: `GET /metrics` on the control listener (goal 15).
//!
//! Two things live here and nothing else: what the server keeps of each
//! speaker's latest `telemetry` ([`TelemetryStore`]), and the text a scrape is
//! answered with ([`render`]). `docs/telemetry.md` is the reference for every
//! series, its unit and which endpoint fills it; the series names are a
//! contract with the rules and dashboards written against them.
//!
//! # What is kept, and what is not
//!
//! The latest report per speaker and the instant it arrived, on the server's
//! monotonic clock. A report is about once a second per speaker, so keeping
//! one never fans a control state out: the store has its own lock, and
//! nothing here touches the room model. A report from an id the server does
//! not list as a speaker is not kept, which bounds the store at the speaker
//! list's own bound.
//!
//! # The rules of the text
//!
//! - Text exposition format 0.0.4: a `# HELP` and a `# TYPE` line before a
//!   family's samples, each family in one group, no timestamps, a line feed
//!   at the end. Label values escape backslash, double quote and line feed,
//!   and nothing else.
//! - **A value an endpoint reported as unknown is omitted, never zero.** A
//!   zero would be a measurement: 0 dBm, 0 degrees, a perfect clock.
//! - **A disconnected speaker keeps only** `chorus_speaker_connected` (0),
//!   `chorus_speaker_info` and `chorus_speaker_firmware_info`. Its telemetry
//!   series leave the scrape, so Prometheus marks them stale at once instead
//!   of drawing the last value on.
//! - A family with no sample is left out whole, HELP and TYPE included.
//! - Numbers are written from the integers the wire carried, by moving the
//!   decimal point: no float is formatted, so a value reads the same on
//!   every scrape.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::Mutex;
use std::time::Instant;

use chorus_protocol::v2::{Link, Telemetry, TELEMETRY_HEAP_UNKNOWN};

/// The content type of a scrape.
pub const CONTENT_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";

/// A speaker's latest report and when it arrived.
#[derive(Debug, Clone, Copy)]
pub struct Kept {
    /// The report, as the wire carried it.
    pub report: Telemetry,
    /// When it arrived, on the server's monotonic clock.
    pub received: Instant,
}

/// The latest `telemetry` of every speaker that sent one this session.
#[derive(Debug, Default)]
pub struct TelemetryStore {
    held: Mutex<BTreeMap<String, Kept>>,
}

impl TelemetryStore {
    /// An empty store.
    pub fn new() -> TelemetryStore {
        TelemetryStore::default()
    }

    fn locked(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Kept>> {
        match self.held.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Keep `report` as speaker `id`'s latest.
    pub fn keep(&self, id: &str, report: Telemetry, received: Instant) {
        self.locked()
            .insert(id.to_string(), Kept { report, received });
    }

    /// Drop what is kept of `id`: its last session ended.
    pub fn forget(&self, id: &str) {
        self.locked().remove(id);
    }

    /// Everything kept, by speaker id.
    pub fn snapshot(&self) -> BTreeMap<String, Kept> {
        self.locked().clone()
    }
}

/// What the room model says of one adopted speaker, for a scrape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeakerRow {
    /// The id its sessions authenticate as: the `speaker` label.
    pub id: String,
    /// Its display name.
    pub name: String,
    /// The room it is assigned, or empty.
    pub room: String,
    /// Whether a session of it is up.
    pub connected: bool,
    /// The firmware version it last said it runs; empty when it never said.
    pub version: String,
}

/// A label value, escaped as the exposition format asks: backslash, double
/// quote and line feed, nothing else.
pub fn escape_label(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            other => out.push(other),
        }
    }
    out
}

/// `value` with its decimal point moved `decimals` places left, exactly:
/// `scaled(-1250, 9)` is `-0.000001250`.
pub fn scaled(value: i64, decimals: u32) -> String {
    let scale = 10u64.pow(decimals);
    let magnitude = value.unsigned_abs();
    let sign = if value < 0 { "-" } else { "" };
    if decimals == 0 {
        return format!("{}{}", sign, magnitude);
    }
    format!(
        "{}{}.{:0width$}",
        sign,
        magnitude / scale,
        magnitude % scale,
        width = decimals as usize
    )
}

/// One metric family being written: its HELP and TYPE go out with its first
/// sample, so a family with no sample leaves nothing behind.
struct Family<'a> {
    out: &'a mut String,
    name: &'static str,
    kind: &'static str,
    help: &'static str,
    opened: bool,
}

impl<'a> Family<'a> {
    fn new(
        out: &'a mut String,
        name: &'static str,
        kind: &'static str,
        help: &'static str,
    ) -> Family<'a> {
        Family {
            out,
            name,
            kind,
            help,
            opened: false,
        }
    }

    fn sample(&mut self, labels: &[(&str, &str)], value: &str) {
        if !self.opened {
            self.opened = true;
            let _ = writeln!(self.out, "# HELP {} {}", self.name, self.help);
            let _ = writeln!(self.out, "# TYPE {} {}", self.name, self.kind);
        }
        self.out.push_str(self.name);
        if !labels.is_empty() {
            self.out.push('{');
            for (i, (label, text)) in labels.iter().enumerate() {
                if i > 0 {
                    self.out.push(',');
                }
                let _ = write!(self.out, "{}=\"{}\"", label, escape_label(text));
            }
            self.out.push('}');
        }
        let _ = writeln!(self.out, " {}", value);
    }
}

/// The wire's link as the `link` label spells it.
fn link_label(link: Link) -> &'static str {
    match link {
        Link::Unknown => "unknown",
        Link::Wired => "wired",
        Link::Wireless => "wifi",
    }
}

/// The text of one scrape. `speakers` is every adopted speaker in the room
/// model's order; `kept` is the telemetry store's snapshot; `now` is the
/// instant ages are taken against.
pub fn render(
    server_version: &str,
    speakers: &[SpeakerRow],
    kept: &BTreeMap<String, Kept>,
    now: Instant,
) -> String {
    let mut out = String::new();
    // Only a connected speaker's report is exposed: a session that ended
    // takes its measurements out of the scrape with it.
    let live: Vec<(&SpeakerRow, &Kept)> = speakers
        .iter()
        .filter(|s| s.connected)
        .filter_map(|s| kept.get(&s.id).map(|k| (s, k)))
        .collect();

    Family::new(
        &mut out,
        "chorus_server_build_info",
        "gauge",
        "The chorus-server build that answered this scrape; the value is always 1.",
    )
    .sample(&[("version", server_version)], "1");

    Family::new(
        &mut out,
        "chorus_speakers",
        "gauge",
        "Speakers this server has adopted, connected or not.",
    )
    .sample(&[], &speakers.len().to_string());

    let mut f = Family::new(
        &mut out,
        "chorus_speaker_connected",
        "gauge",
        "Whether a session of the speaker is up: 1 connected, 0 not. Present for every adopted speaker.",
    );
    for s in speakers {
        f.sample(&[("speaker", &s.id)], if s.connected { "1" } else { "0" });
    }

    let mut f = Family::new(
        &mut out,
        "chorus_speaker_info",
        "gauge",
        "The speaker's display name and assigned room (empty when it has none); the value is always 1.",
    );
    for s in speakers {
        f.sample(
            &[("speaker", &s.id), ("name", &s.name), ("room", &s.room)],
            "1",
        );
    }

    let mut f = Family::new(
        &mut out,
        "chorus_speaker_firmware_info",
        "gauge",
        "The firmware version the speaker last said it runs, kept while it is disconnected; the value is always 1. Absent until the speaker has said.",
    );
    for s in speakers.iter().filter(|s| !s.version.is_empty()) {
        f.sample(&[("speaker", &s.id), ("version", &s.version)], "1");
    }

    let mut f = Family::new(
        &mut out,
        "chorus_speaker_telemetry_age_seconds",
        "gauge",
        "Seconds since the speaker's latest telemetry report arrived, on the server's monotonic clock.",
    );
    for (s, k) in &live {
        let age_ms = now.saturating_duration_since(k.received).as_millis();
        let age_ms = i64::try_from(age_ms).unwrap_or(i64::MAX);
        f.sample(&[("speaker", &s.id)], &scaled(age_ms, 3));
    }

    let mut f = Family::new(
        &mut out,
        "chorus_speaker_sync_error_seconds",
        "gauge",
        "The speaker's own estimate of its playout error against the server timeline, signed, in seconds. Not a measured inter-speaker error. Absent while unknown.",
    );
    for (s, k) in &live {
        if k.report.sync_error_ns != i64::MIN {
            f.sample(&[("speaker", &s.id)], &scaled(k.report.sync_error_ns, 9));
        }
    }

    let mut f = Family::new(
        &mut out,
        "chorus_speaker_buffer_fill_seconds",
        "gauge",
        "Audio buffered ahead of the speaker's playout point, in seconds.",
    );
    for (s, k) in &live {
        f.sample(
            &[("speaker", &s.id)],
            &scaled(i64::from(k.report.buffer_fill_us), 6),
        );
    }

    let mut f = Family::new(
        &mut out,
        "chorus_speaker_rate_correction_ratio",
        "gauge",
        "The rate correction the speaker's servo has in force, signed, as a ratio (1e-6 is 1 ppm).",
    );
    for (s, k) in &live {
        f.sample(
            &[("speaker", &s.id)],
            &scaled(i64::from(k.report.correction_ppb), 9),
        );
    }

    let mut f = Family::new(
        &mut out,
        "chorus_speaker_resyncs_total",
        "counter",
        "Hard resynchronisations the speaker reports since its session began.",
    );
    for (s, k) in &live {
        f.sample(&[("speaker", &s.id)], &k.report.resyncs.to_string());
    }

    let mut f = Family::new(
        &mut out,
        "chorus_speaker_underruns_total",
        "counter",
        "Playback underruns the speaker reports since its session began.",
    );
    for (s, k) in &live {
        f.sample(&[("speaker", &s.id)], &k.report.underruns.to_string());
    }

    let mut f = Family::new(
        &mut out,
        "chorus_speaker_link_info",
        "gauge",
        "How the speaker says it reaches the network: link is wired, wifi or unknown; the value is always 1.",
    );
    for (s, k) in &live {
        f.sample(
            &[("speaker", &s.id), ("link", link_label(k.report.link))],
            "1",
        );
    }

    let mut f = Family::new(
        &mut out,
        "chorus_speaker_rssi_dbm",
        "gauge",
        "The Wi-Fi signal strength the speaker reports, in decibel-milliwatts (dBm). Absent when unknown or wired.",
    );
    for (s, k) in &live {
        if k.report.rssi_dbm != i8::MIN && k.report.link != Link::Wired {
            f.sample(&[("speaker", &s.id)], &k.report.rssi_dbm.to_string());
        }
    }

    let mut f = Family::new(
        &mut out,
        "chorus_speaker_heap_free_bytes",
        "gauge",
        "The speaker's free heap, in bytes. Absent when the endpoint does not report it.",
    );
    for (s, k) in &live {
        if k.report.heap_free_bytes != TELEMETRY_HEAP_UNKNOWN {
            f.sample(&[("speaker", &s.id)], &k.report.heap_free_bytes.to_string());
        }
    }

    let mut f = Family::new(
        &mut out,
        "chorus_speaker_heap_min_free_bytes",
        "gauge",
        "The least free heap the speaker has seen since it booted, in bytes. Absent when the endpoint does not report it.",
    );
    for (s, k) in &live {
        if k.report.heap_min_free_bytes != TELEMETRY_HEAP_UNKNOWN {
            f.sample(
                &[("speaker", &s.id)],
                &k.report.heap_min_free_bytes.to_string(),
            );
        }
    }

    let mut f = Family::new(
        &mut out,
        "chorus_speaker_temperature_celsius",
        "gauge",
        "The temperature the speaker reports, in degrees Celsius. Absent when unknown.",
    );
    for (s, k) in &live {
        if k.report.temperature_centi_c != i16::MIN {
            f.sample(
                &[("speaker", &s.id)],
                &scaled(i64::from(k.report.temperature_centi_c), 2),
            );
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn report() -> Telemetry {
        Telemetry {
            taken_ns: 5_000_000_000,
            sync_error_ns: -1250,
            buffer_fill_us: 180_000,
            underruns: 1,
            resyncs: 2,
            correction_ppb: -3500,
            link: Link::Wireless,
            rssi_dbm: -58,
            temperature_centi_c: 4150,
            heap_free_bytes: 187_432,
            heap_min_free_bytes: 141_200,
        }
    }

    fn unknown() -> Telemetry {
        Telemetry {
            taken_ns: 1,
            sync_error_ns: i64::MIN,
            buffer_fill_us: 0,
            underruns: 0,
            resyncs: 0,
            correction_ppb: 0,
            link: Link::Unknown,
            rssi_dbm: i8::MIN,
            temperature_centi_c: i16::MIN,
            heap_free_bytes: TELEMETRY_HEAP_UNKNOWN,
            heap_min_free_bytes: TELEMETRY_HEAP_UNKNOWN,
        }
    }

    fn row(id: &str, connected: bool) -> SpeakerRow {
        SpeakerRow {
            id: id.to_string(),
            name: format!("Speaker {}", id),
            room: "kitchen".to_string(),
            connected,
            version: "1.4.2".to_string(),
        }
    }

    fn scrape(speakers: &[SpeakerRow], reports: &[(&str, Telemetry)]) -> String {
        let then = Instant::now();
        let kept: BTreeMap<String, Kept> = reports
            .iter()
            .map(|(id, report)| {
                (
                    id.to_string(),
                    Kept {
                        report: *report,
                        received: then,
                    },
                )
            })
            .collect();
        render("0.1.0", speakers, &kept, then + Duration::from_millis(250))
    }

    #[test]
    fn numbers_are_the_wires_integers_with_the_point_moved() {
        assert_eq!(scaled(-1250, 9), "-0.000001250");
        assert_eq!(scaled(180_000, 6), "0.180000");
        assert_eq!(scaled(4150, 2), "41.50");
        assert_eq!(scaled(-5, 2), "-0.05");
        assert_eq!(scaled(0, 9), "0.000000000");
        assert_eq!(scaled(7, 0), "7");
        assert_eq!(scaled(i64::MAX, 9), "9223372036.854775807");
        assert_eq!(scaled(i64::MIN + 1, 9), "-9223372036.854775807");
    }

    #[test]
    fn a_label_value_escapes_backslash_quote_and_line_feed_and_nothing_else() {
        assert_eq!(escape_label(r#"a\b"c"#), r#"a\\b\"c"#);
        assert_eq!(escape_label("two\nlines"), "two\\nlines");
        assert_eq!(escape_label("Küche {1}, =ok"), "Küche {1}, =ok");
        let mut named = row("s1", true);
        named.name = "The \"big\" one\\\n".to_string();
        let text = scrape(&[named], &[]);
        assert!(
            text.contains(
                "chorus_speaker_info{speaker=\"s1\",name=\"The \\\"big\\\" one\\\\\\n\",room=\"kitchen\"} 1\n"
            ),
            "{}",
            text
        );
        assert_eq!(text.lines().count(), text.matches('\n').count());
    }

    #[test]
    fn every_series_of_a_reporting_speaker_is_there_with_its_unit() {
        let text = scrape(&[row("s1", true)], &[("s1", report())]);
        for line in [
            "chorus_server_build_info{version=\"0.1.0\"} 1",
            "chorus_speakers 1",
            "chorus_speaker_connected{speaker=\"s1\"} 1",
            "chorus_speaker_info{speaker=\"s1\",name=\"Speaker s1\",room=\"kitchen\"} 1",
            "chorus_speaker_firmware_info{speaker=\"s1\",version=\"1.4.2\"} 1",
            "chorus_speaker_telemetry_age_seconds{speaker=\"s1\"} 0.250",
            "chorus_speaker_sync_error_seconds{speaker=\"s1\"} -0.000001250",
            "chorus_speaker_buffer_fill_seconds{speaker=\"s1\"} 0.180000",
            "chorus_speaker_rate_correction_ratio{speaker=\"s1\"} -0.000003500",
            "chorus_speaker_resyncs_total{speaker=\"s1\"} 2",
            "chorus_speaker_underruns_total{speaker=\"s1\"} 1",
            "chorus_speaker_link_info{speaker=\"s1\",link=\"wifi\"} 1",
            "chorus_speaker_rssi_dbm{speaker=\"s1\"} -58",
            "chorus_speaker_heap_free_bytes{speaker=\"s1\"} 187432",
            "chorus_speaker_heap_min_free_bytes{speaker=\"s1\"} 141200",
            "chorus_speaker_temperature_celsius{speaker=\"s1\"} 41.50",
        ] {
            assert!(
                text.lines().any(|l| l == line),
                "missing {:?} in\n{}",
                line,
                text
            );
        }
        assert!(text.ends_with('\n'));
    }

    #[test]
    fn every_family_has_help_and_type_before_its_samples_and_is_one_group() {
        let text = scrape(
            &[row("s1", true), row("s2", true), row("s3", false)],
            &[("s1", report()), ("s2", unknown())],
        );
        let mut seen: Vec<String> = Vec::new();
        let mut helped = String::new();
        let mut typed = String::new();
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("# HELP ") {
                helped = rest.split(' ').next().unwrap().to_string();
                assert!(!seen.contains(&helped), "{} is in two groups", helped);
                seen.push(helped.clone());
            } else if let Some(rest) = line.strip_prefix("# TYPE ") {
                let mut words = rest.split(' ');
                typed = words.next().unwrap().to_string();
                let kind = words.next().unwrap();
                assert_eq!(typed, helped, "TYPE follows its own HELP");
                assert_eq!(
                    kind == "counter",
                    typed.ends_with("_total"),
                    "{} is a {}",
                    typed,
                    kind
                );
            } else {
                let name = line.split(['{', ' ']).next().unwrap();
                assert_eq!(name, typed, "a sample outside its family's group: {}", line);
            }
        }
        assert_eq!(seen.len(), 16, "{:?}", seen);
    }

    #[test]
    fn what_an_endpoint_reports_as_unknown_is_omitted_and_never_zero() {
        let text = scrape(&[row("s1", true)], &[("s1", unknown())]);
        for absent in [
            "chorus_speaker_sync_error_seconds",
            "chorus_speaker_rssi_dbm",
            "chorus_speaker_heap_free_bytes",
            "chorus_speaker_heap_min_free_bytes",
            "chorus_speaker_temperature_celsius",
        ] {
            assert!(!text.contains(absent), "{} is there:\n{}", absent, text);
        }
        assert!(text.contains("chorus_speaker_link_info{speaker=\"s1\",link=\"unknown\"} 1\n"));
        assert!(text.contains("chorus_speaker_buffer_fill_seconds{speaker=\"s1\"} 0.000000\n"));
    }

    #[test]
    fn a_wired_speaker_has_no_signal_strength_whatever_it_sent() {
        let mut wired = report();
        wired.link = Link::Wired;
        let text = scrape(&[row("s1", true)], &[("s1", wired)]);
        assert!(!text.contains("chorus_speaker_rssi_dbm"), "{}", text);
        assert!(text.contains("chorus_speaker_link_info{speaker=\"s1\",link=\"wired\"} 1\n"));
    }

    #[test]
    fn a_disconnected_speaker_keeps_only_connected_info_and_firmware_info() {
        // Its report is still in the store (the race between a scrape and
        // the session's end): it must not be exposed.
        let text = scrape(&[row("gone", false)], &[("gone", report())]);
        let samples: Vec<&str> = text
            .lines()
            .filter(|l| !l.starts_with('#') && l.contains("gone"))
            .collect();
        assert_eq!(
            samples,
            vec![
                "chorus_speaker_connected{speaker=\"gone\"} 0",
                "chorus_speaker_info{speaker=\"gone\",name=\"Speaker gone\",room=\"kitchen\"} 1",
                "chorus_speaker_firmware_info{speaker=\"gone\",version=\"1.4.2\"} 1",
            ]
        );
    }

    #[test]
    fn a_speaker_that_never_said_its_version_has_no_firmware_info() {
        let mut quiet = row("s1", false);
        quiet.version = String::new();
        let text = scrape(&[quiet], &[]);
        assert!(!text.contains("chorus_speaker_firmware_info"), "{}", text);
        assert!(text.contains("chorus_speaker_connected{speaker=\"s1\"} 0\n"));
    }

    #[test]
    fn a_report_from_an_id_that_is_not_a_listed_speaker_is_not_exposed() {
        let text = scrape(&[row("s1", true)], &[("stranger", report())]);
        assert!(!text.contains("stranger"), "{}", text);
        assert!(text.contains("chorus_speakers 1\n"));
    }

    #[test]
    fn with_no_speaker_the_scrape_is_the_build_and_a_count_of_zero() {
        let text = scrape(&[], &[]);
        assert_eq!(
            text,
            "# HELP chorus_server_build_info The chorus-server build that answered this scrape; the value is always 1.\n\
             # TYPE chorus_server_build_info gauge\n\
             chorus_server_build_info{version=\"0.1.0\"} 1\n\
             # HELP chorus_speakers Speakers this server has adopted, connected or not.\n\
             # TYPE chorus_speakers gauge\n\
             chorus_speakers 0\n"
        );
    }

    #[test]
    fn the_store_keeps_the_latest_report_and_forgets_on_request() {
        let store = TelemetryStore::new();
        let now = Instant::now();
        store.keep("s1", unknown(), now);
        store.keep("s1", report(), now);
        assert_eq!(store.snapshot()["s1"].report, report());
        store.forget("s1");
        assert!(store.snapshot().is_empty());
    }
}
