//! Goal 15, line A: the Prometheus exporter serves every listed per-speaker
//! metric in a test scrape, on the real `chorus-server` binary with a real C
//! endpoint session and a real Linux-client session.
//!
//! The two speakers:
//!
//! - **The C endpoint** is the firmware's own session, playout path and
//!   telemetry sender, built for the host (`firmware/build/chorus-endpoint-dsp-session`,
//!   the program `firmware/tests/dsp-session.sh` drives). Its sync error,
//!   buffer fill, rate correction, resyncs and underruns are REAL: they come
//!   from the playout path playing the server's tone through the program's
//!   fake I2S DMA. Its link, RSSI, temperature and heap are STATED FAKES: a
//!   host has no radio, no board temperature and no `heap_caps`, so the test
//!   hands the session's `health` seam (the seam `firmware/main/esp_hal.c`
//!   binds on the board) the five numbers below on the program's command
//!   line, and asserts they cross the wire and come out of the scrape
//!   unchanged. What a real board reports there is a bench item
//!   (`docs/telemetry.md`, "Bench").
//! - **The Linux endpoint** is the shipped client's session
//!   (`session::open`) sending the report the shipped client's playout loop
//!   builds (`chorus_client_linux::run::wire_report`). It knows no signal
//!   strength, temperature or heap, so the scrape must hold none of those
//!   for it: unknown is omitted, never zero.
//!
//! This test builds the C endpoint with the firmware's own Makefile, so it
//! needs what `make firmware-check` needs and fails by name without it.
//! Loopback only. Nothing here is timing evidence: the sync error is checked
//! to be a number inside a wide plausibility band, never against a budget.
//!
//! `cargo test -p chorus-server --test metrics_scrape -- --nocapture` prints
//! the scrape; `make verify-metrics` lints one with promtool.

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use chorus_client_linux::config::ClientConfig;
use chorus_client_linux::run::wire_report;
use chorus_client_linux::sync::Telemetry as SyncTelemetry;
use chorus_control::transport::Transport;
use chorus_protocol::v2::{encode, Message};
use common::{http, Player, RunningServer};

/// The stated fakes handed to the C endpoint's health seam.
const FAKE_RSSI_DBM: &str = "-58";
const FAKE_TEMPERATURE_CENTI_C: &str = "4150";
const FAKE_HEAP_FREE_BYTES: &str = "187432";
const FAKE_HEAP_MIN_FREE_BYTES: &str = "141200";

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("this crate lives at crates/<name> under the repository root")
        .to_path_buf()
}

/// Build the C endpoint's host program with the firmware's own Makefile (a
/// no-op when it is current), under the file lock the other tests that run
/// make into `firmware/build` take (`adoption.rs`, `firmware_install.rs`,
/// `crates/protocol/tests/firmware_session.rs`).
fn c_endpoint() -> PathBuf {
    let root = repository_root();
    let build = root.join("firmware").join("build");
    std::fs::create_dir_all(&build).expect("firmware/build can be made");
    let program = build.join("chorus-endpoint-dsp-session");
    let lock =
        std::fs::File::create(build.join(".chorus-endpoint-session.lock")).expect("a lock file");
    lock.lock().expect("the build lock");
    let made = Command::new("make")
        .current_dir(&root)
        .arg("--no-print-directory")
        .arg("-f")
        .arg(root.join("firmware").join("Makefile"))
        .arg(&program)
        .output();
    let how = "this test runs the real C endpoint, built as `make firmware-check` builds it: it \
               needs make, a C compiler and the pinned ESP-IDF tree (CHORUS_IDF_V61_DIR)";
    match made {
        Ok(out) if out.status.success() => {}
        Ok(out) => panic!(
            "MISSING PREREQUISITE: the C endpoint could not be built. {}.\n{}\n{}",
            how,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
        Err(e) => panic!(
            "MISSING PREREQUISITE: make could not be run ({}). {}",
            e, how
        ),
    }
    assert!(program.is_file(), "{}", how);
    program
}

/// A C endpoint that is killed if the test ends before it does.
struct Endpoint(Option<Child>);

impl Endpoint {
    fn stop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        self.stop();
    }
}

/// An id shaped like the firmware's own: `chorus-` and twelve hex digits.
fn speaker_id(n: u64) -> String {
    format!(
        "chorus-{:012x}",
        (u64::from(std::process::id()) << 16 | n) & 0xffff_ffff_ffff
    )
}

/// One scrape: the status line, the headers and the body.
fn scrape(server: &RunningServer) -> String {
    let (status, body) = http(
        &server.control,
        "GET /metrics HTTP/1.1\r\nHost: chorus\r\nConnection: close\r\n\r\n",
    );
    assert!(status.contains("200"), "GET /metrics answered {}", status);
    body
}

/// The samples of a scrape: `name{labels}` to its value text.
fn samples(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .map(|l| {
            let (series, value) = l.rsplit_once(' ').expect("a sample is `series value`");
            (series.to_string(), value.to_string())
        })
        .collect()
}

fn value(samples: &BTreeMap<String, String>, series: &str) -> f64 {
    samples
        .get(series)
        .unwrap_or_else(|| panic!("the scrape has no {}", series))
        .parse()
        .unwrap_or_else(|e| panic!("{} is not a number: {}", series, e))
}

fn of(metric: &str, speaker: &str) -> String {
    format!("{}{{speaker=\"{}\"}}", metric, speaker)
}

/// Poll `/metrics` until `done` holds of a scrape, and return that scrape.
fn scrape_until(server: &RunningServer, what: &str, done: impl Fn(&str) -> bool) -> String {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let text = scrape(server);
        if done(&text) {
            return text;
        }
        assert!(
            Instant::now() < deadline,
            "{}: not within 20 s; the last scrape was\n{}",
            what,
            text
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// THE LINE-A TEST. One scrape of a real server with a real C endpoint
/// session and a real Linux-client session holds every series of the
/// exporter's table for the C speaker with plausible values, holds for the
/// Linux speaker what a Linux endpoint knows and nothing it does not, and is
/// well formed (HELP and TYPE for every family, one group per family). When
/// the C endpoint's session ends, only `connected` 0, `info` and
/// `firmware_info` remain of it.
///
/// Which values are real and which are stated fakes is in this file's
/// module documentation.
#[test]
fn the_exporter_serves_every_listed_metric_in_a_test_scrape() {
    let program = c_endpoint();
    let dir = std::env::temp_dir().join(format!("chorus-metrics-scrape-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");

    let server =
        RunningServer::start(&["--source", "tone", "--serve-forever", "--zone", "kitchen"]);
    // The content type is the text exposition format's.
    {
        use std::io::{Read, Write};
        let mut raw = std::net::TcpStream::connect(&server.control).expect("the control listener");
        raw.write_all(b"GET /metrics HTTP/1.1\r\nHost: chorus\r\nConnection: close\r\n\r\n")
            .unwrap();
        let mut answer = String::new();
        raw.read_to_string(&mut answer).unwrap();
        assert!(
            answer.contains("Content-Type: text/plain; version=0.0.4; charset=utf-8\r\n"),
            "{}",
            answer
        );
        assert!(
            answer.contains("chorus_speakers 0\n"),
            "nobody is adopted yet: {}",
            answer
        );
    }

    let c_speaker = speaker_id(1);
    let linux_speaker = speaker_id(2);
    let mut c_run = Endpoint(Some(
        Command::new(&program)
            .args([
                "--server",
                &server.audio,
                "--endpoint-id",
                &c_speaker,
                "--run-seconds",
                "60",
                "--capture",
                dir.join("c-speaker.raw").to_str().unwrap(),
                "--health-link",
                "wireless",
                "--health-rssi-dbm",
                FAKE_RSSI_DBM,
                "--health-temperature-centi-c",
                FAKE_TEMPERATURE_CENTI_C,
                "--health-heap-free-bytes",
                FAKE_HEAP_FREE_BYTES,
                "--health-heap-min-free-bytes",
                FAKE_HEAP_MIN_FREE_BYTES,
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap_or_else(|e| panic!("{} runs: {}", program.display(), e)),
    ));
    let mut linux = Player::connect(&server.audio, &linux_speaker, 0);

    // Both are adopted by connecting. Name the C speaker and put both in the
    // kitchen, so the room's tone reaches them and the C endpoint's playout
    // loop has a stream to form an error against.
    scrape_until(&server, "both speakers are adopted and connected", |text| {
        let s = samples(text);
        s.get(&of("chorus_speaker_connected", &c_speaker))
            .is_some_and(|v| v == "1")
            && s.get(&of("chorus_speaker_connected", &linux_speaker))
                .is_some_and(|v| v == "1")
    });
    server.applied(&format!(
        r#"{{"v":2,"t":"speaker_name","speaker":"{}","name":"Kitchen \"left\""}}"#,
        c_speaker
    ));
    for id in [&c_speaker, &linux_speaker] {
        server.applied(&format!(
            r#"{{"v":2,"t":"speaker_room","speaker":"{}","room":"kitchen"}}"#,
            id
        ));
    }

    // The Linux endpoint's report, as the shipped client's playout loop
    // builds it, up the session the shipped client opens.
    let report = wire_report(
        3_000_000_000,
        Some(-42_000.0),
        120_000,
        0,
        &SyncTelemetry {
            offset_ns: Some(1_000),
            round_trip_ns: Some(200_000),
            bound_ns: Some(100_000),
            stale: false,
            age_ns: Some(0),
            correction_ppm: 1.25,
            clamped: false,
            hard_resyncs: 0,
            accepted: 1,
            discarded: 0,
        },
        Transport::Wired,
    );
    {
        use std::io::Write;
        let frame = encode(&Message::Telemetry(report)).expect("the report encodes");
        linux.session.writer.write_all(&frame).unwrap();
        linux.session.writer.flush().unwrap();
    }
    assert_eq!(ClientConfig::default().transport, Transport::Wired);

    // ONE scrape, once the C endpoint's loop has formed an error and the
    // Linux endpoint's report has arrived.
    let text = scrape_until(&server, "every series is there", |text| {
        let s = samples(text);
        s.contains_key(&of("chorus_speaker_sync_error_seconds", &c_speaker))
            && s.contains_key(&of("chorus_speaker_sync_error_seconds", &linux_speaker))
            && value(&s, &of("chorus_speaker_buffer_fill_seconds", &c_speaker)) > 0.0
    });
    println!(
        "--- GET /metrics (the line-A scrape) ---\n{}--- end ---",
        text
    );
    let s = samples(&text);

    // The server's own two series.
    assert_eq!(
        s.get(&format!(
            "chorus_server_build_info{{version=\"{}\"}}",
            env!("CARGO_PKG_VERSION")
        ))
        .map(String::as_str),
        Some("1")
    );
    assert_eq!(value(&s, "chorus_speakers"), 2.0);

    // --- the C speaker: every series of the table -------------------------
    let c = |metric: &str| value(&s, &of(metric, &c_speaker));
    assert_eq!(c("chorus_speaker_connected"), 1.0);
    assert_eq!(
        s.get(&format!(
            "chorus_speaker_info{{speaker=\"{}\",name=\"Kitchen \\\"left\\\"\",room=\"kitchen\"}}",
            c_speaker
        ))
        .map(String::as_str),
        Some("1"),
        "the name's quotes are escaped in the label"
    );
    let firmware_info = s
        .keys()
        .find(|k| {
            k.starts_with(&format!(
                "chorus_speaker_firmware_info{{speaker=\"{}\",version=\"",
                c_speaker
            ))
        })
        .expect("the C speaker's firmware_info");
    assert!(
        !firmware_info.contains("version=\"\""),
        "a version is named: {}",
        firmware_info
    );
    assert_eq!(s[firmware_info], "1");
    let age = c("chorus_speaker_telemetry_age_seconds");
    assert!((0.0..3.0).contains(&age), "a report a second: age {}", age);
    // Real, from the playout path. A plausibility band, not a budget.
    let error = c("chorus_speaker_sync_error_seconds");
    assert!(error.abs() < 1.0, "sync error {} s", error);
    let fill = c("chorus_speaker_buffer_fill_seconds");
    assert!(fill > 0.0 && fill < 5.0, "buffer fill {} s", fill);
    let correction = c("chorus_speaker_rate_correction_ratio");
    assert!(correction.abs() < 0.01, "rate correction {}", correction);
    let resyncs = c("chorus_speaker_resyncs_total");
    assert!(resyncs >= 0.0 && resyncs.fract() == 0.0 && resyncs < 1000.0);
    let underruns = c("chorus_speaker_underruns_total");
    assert!(underruns >= 0.0 && underruns.fract() == 0.0 && underruns < 1000.0);
    // Stated fakes, through the health seam, unchanged.
    assert_eq!(
        s.get(&format!(
            "chorus_speaker_link_info{{speaker=\"{}\",link=\"wifi\"}}",
            c_speaker
        ))
        .map(String::as_str),
        Some("1")
    );
    assert_eq!(c("chorus_speaker_rssi_dbm"), -58.0);
    assert_eq!(
        s[&of("chorus_speaker_temperature_celsius", &c_speaker)],
        "41.50"
    );
    assert_eq!(c("chorus_speaker_heap_free_bytes"), 187_432.0);
    assert_eq!(c("chorus_speaker_heap_min_free_bytes"), 141_200.0);

    // --- the Linux speaker: what it knows, and nothing it does not ---------
    let l = |metric: &str| value(&s, &of(metric, &linux_speaker));
    assert_eq!(l("chorus_speaker_connected"), 1.0);
    assert_eq!(
        s[&of("chorus_speaker_sync_error_seconds", &linux_speaker)],
        "-0.000042000"
    );
    assert_eq!(
        s[&of("chorus_speaker_buffer_fill_seconds", &linux_speaker)],
        "0.120000"
    );
    assert_eq!(
        s[&of("chorus_speaker_rate_correction_ratio", &linux_speaker)],
        "0.000001250"
    );
    assert_eq!(l("chorus_speaker_resyncs_total"), 0.0);
    assert_eq!(l("chorus_speaker_underruns_total"), 0.0);
    assert!(s.contains_key(&format!(
        "chorus_speaker_link_info{{speaker=\"{}\",link=\"wired\"}}",
        linux_speaker
    )));
    for unknown in [
        "chorus_speaker_rssi_dbm",
        "chorus_speaker_temperature_celsius",
        "chorus_speaker_heap_free_bytes",
        "chorus_speaker_heap_min_free_bytes",
    ] {
        assert!(
            !s.contains_key(&of(unknown, &linux_speaker)),
            "{} is unknown to a Linux endpoint and must be omitted",
            unknown
        );
    }

    // --- the shape: HELP and TYPE for every family, one group each ---------
    let mut family = String::new();
    let mut families = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("# HELP ") {
            family = rest.split(' ').next().unwrap().to_string();
            assert!(!families.contains(&family), "{} has two groups", family);
            families.push(family.clone());
        } else if let Some(rest) = line.strip_prefix("# TYPE ") {
            assert_eq!(rest.split(' ').next().unwrap(), family);
        } else {
            assert_eq!(line.split(['{', ' ']).next().unwrap(), family, "{}", line);
        }
    }
    let mut expected = vec![
        "chorus_server_build_info",
        "chorus_speakers",
        "chorus_speaker_connected",
        "chorus_speaker_info",
        "chorus_speaker_firmware_info",
        "chorus_speaker_telemetry_age_seconds",
        "chorus_speaker_sync_error_seconds",
        "chorus_speaker_buffer_fill_seconds",
        "chorus_speaker_rate_correction_ratio",
        "chorus_speaker_resyncs_total",
        "chorus_speaker_underruns_total",
        "chorus_speaker_link_info",
        "chorus_speaker_rssi_dbm",
        "chorus_speaker_heap_free_bytes",
        "chorus_speaker_heap_min_free_bytes",
        "chorus_speaker_temperature_celsius",
    ];
    expected.sort_unstable();
    families.sort();
    assert_eq!(families, expected, "exactly the documented families");
    assert!(text.ends_with('\n'));

    // --- a speaker whose session ended keeps three series ------------------
    c_run.stop();
    let after = scrape_until(&server, "the C speaker is shown disconnected", |text| {
        samples(text)
            .get(&of("chorus_speaker_connected", &c_speaker))
            .is_some_and(|v| v == "0")
    });
    let left: Vec<String> = samples(&after)
        .into_keys()
        .filter(|k| k.contains(&c_speaker))
        .map(|k| k.split('{').next().unwrap().to_string())
        .collect();
    assert_eq!(
        left,
        vec![
            "chorus_speaker_connected",
            "chorus_speaker_firmware_info",
            "chorus_speaker_info"
        ],
        "a disconnected speaker's telemetry leaves the scrape:\n{}",
        after
    );
    assert!(
        samples(&after).contains_key(&of("chorus_speaker_rate_correction_ratio", &linux_speaker)),
        "the speaker that is still connected keeps its series"
    );
    drop(linux);
    let _ = std::fs::remove_dir_all(&dir);
}
