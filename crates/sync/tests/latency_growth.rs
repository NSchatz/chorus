//! K94 in the simulator (goal 11): a line-in played in its own room at a low
//! latency, a second room joining, and the latency growing to the group's
//! without a glitch in the room that was already playing.
//!
//! The scenarios are `config/sim-latency/*.latency`, Rust-only for the reason
//! the houses are (ADR 0048): the endpoint's C core models one endpoint, and
//! every file under `fixtures/sync/` must be read by both implementations.
//! What is pinned is a property of the model, not a timing claim (BRIEF.md
//! section 3.1 rule 3). The glitch criterion is `LatencyRun::failures`; its
//! statement is in the decision record and the report.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use chorus_sync::latency_grow::{MAX_RATE_DEVIATION, RAMP_MS};
use chorus_sync::latency_report::{report, ScenarioRuns};
use chorus_sync::latency_sim::{run_latency, LatencyRun, LatencyScenario, Mode, SCENARIOS};
use chorus_sync::HouseConfig;

fn repo(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

fn read(path: &str) -> String {
    let path = repo(path);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{} is unreadable: {}", path.display(), e))
}

/// `key = value` from a committed conf file, as a whole number.
fn conf(file: &str, key: &str) -> u64 {
    read(file)
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter_map(|l| l.split_once('='))
        .find(|(k, _)| k.trim() == key)
        .and_then(|(_, v)| v.trim().parse().ok())
        .unwrap_or_else(|| panic!("{} has no whole-number {}", file, key))
}

struct Loaded {
    scenario: LatencyScenario,
    house: HouseConfig,
    stretch: LatencyRun,
    naive: LatencyRun,
}

/// Every committed scenario, run once for all the tests here (they are the
/// slow part, and they are deterministic).
fn runs() -> &'static [Loaded] {
    static RUNS: OnceLock<Vec<Loaded>> = OnceLock::new();
    RUNS.get_or_init(|| {
        SCENARIOS
            .iter()
            .map(|name| {
                let scenario =
                    LatencyScenario::parse(&read(&format!("config/sim-latency/{}", name)))
                        .unwrap_or_else(|e| panic!("{}: {}", name, e));
                let house =
                    HouseConfig::parse(&read(&format!("config/sim-house/{}", scenario.house)))
                        .unwrap_or_else(|e| panic!("{}: {}", scenario.house, e));
                let stretch = run_latency(&scenario, &house, Mode::Stretch)
                    .unwrap_or_else(|e| panic!("{}: {}", name, e));
                let naive = run_latency(&scenario, &house, Mode::NaiveJump)
                    .unwrap_or_else(|e| panic!("{}: {}", name, e));
                Loaded {
                    scenario,
                    house,
                    stretch,
                    naive,
                }
            })
            .collect()
    })
}

fn named(name: &str) -> &'static Loaded {
    runs()
        .iter()
        .find(|l| l.scenario.name == name)
        .unwrap_or_else(|| panic!("no scenario {}", name))
}

#[test]
fn every_committed_scenario_is_one_of_the_two_named_here() {
    let mut found: Vec<String> = fs::read_dir(repo("config/sim-latency"))
        .expect("config/sim-latency is readable")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    found.sort();
    let mut listed: Vec<String> = SCENARIOS.iter().map(|s| s.to_string()).collect();
    listed.sort();
    assert_eq!(found, listed, "a scenario file this test does not grade");
}

#[test]
fn the_scenarios_take_each_shared_number_from_its_one_copy() {
    let wired = &named("wired-join").scenario;
    let wifi = &named("wifi-join").scenario;
    assert_eq!(
        wired.l_group_us,
        conf("config/sync.conf", "playout_latency_us")
    );
    assert_eq!(
        wifi.l_group_us,
        conf("config/transport.conf", "wireless_playout_latency_us")
    );
    assert_eq!(
        wifi.join_min_latency_us,
        conf("config/transport.conf", "wireless_min_us")
    );
    assert_eq!(wired.join_min_latency_us, wired.l_local_us);
    for s in [wired, wifi] {
        assert_eq!(s.max_rate_ppm, MAX_RATE_DEVIATION * 1e6, "{}", s.name);
        assert_eq!(s.ramp_ms, RAMP_MS, "{}", s.name);
        // ADR 0066: the line-in's 20 ms chunk and its 48 kHz default.
        assert_eq!((s.chunk_ms, s.sample_rate_hz), (20, 48_000), "{}", s.name);
        // The pair is graded after the house's acquisition settle.
        assert!(s.join_at_ms >= named(&s.name).house.settle_ms, "{}", s.name);
    }
    // The stretch bound plus the endpoint servo's largest correction stays
    // under the cited 0.2 % threshold (the decision record's argument).
    let servo_ppm = conf("config/sync.conf", "max_correction_ppm") as f64;
    assert!(MAX_RATE_DEVIATION * 1e6 + servo_ppm < 0.002 * 1e6 / 2.0);
}

#[test]
fn a_wired_room_joins_and_leaves_without_a_glitch() {
    let l = named("wired-join");
    let r = &l.stretch;
    assert_eq!(r.failures(), Vec::<String>::new());
    assert_eq!(r.glitch.frames_checked, l.scenario.duration_ms * 48 - 2);
    // Grown, then shrunk back, each in the planned time to within a chunk.
    assert_eq!(r.transitions.len(), 2);
    for t in &r.transitions {
        let took = (t.reached_chunk.expect("reached") - t.requested_chunk) as f64 * 20e6;
        let planned = r.config.transition_ns(t.to_ns - t.from_ns);
        assert!((took - planned).abs() <= 60e6, "{} vs {}", took, planned);
    }
    assert!((r.chunks.last().expect("ran").offset_ns - 30e6).abs() < 1.0);
    // A wired room starts at once: its chunks arrive in time at L_local.
    assert_eq!(r.join_start_chunk, Some(r.transitions[0].requested_chunk));
    assert_eq!(r.join_underruns, 0);
    assert!(r.glitch.max_rate_change_per_chunk < 2e-6);
}

#[test]
fn a_wifi_room_joins_when_the_buffer_is_deep_enough_and_the_playing_room_never_glitches() {
    let l = named("wifi-join");
    let r = &l.stretch;
    assert_eq!(r.failures(), Vec::<String>::new());
    let start = r.join_start_chunk.expect("the Wi-Fi room plays");
    let at = &r.chunks[start as usize];
    assert!(at.offset_ns >= l.scenario.join_min_latency_us as f64 * 1e3 - 1.0);
    assert!(
        start > r.transitions[0].requested_chunk,
        "it starts later than the join"
    );
    assert_eq!(r.join_underruns, 0);
    assert!((r.chunks.last().expect("ran").offset_ns - 500e6).abs() < 1.0);
}

#[test]
fn the_naive_jump_fails_the_criterion_with_a_gap() {
    for l in runs() {
        let n = &l.naive;
        let failures = n.failures();
        assert!(
            failures.iter().any(|f| f.starts_with("(a)")),
            "{}: {:?}",
            l.scenario.name,
            failures
        );
        let jump_frames = (l.scenario.l_group_us - l.scenario.l_local_us) * 48 / 1_000;
        assert_eq!(n.glitch.inserted_frames, jump_frames, "{}", l.scenario.name);
    }
}

#[test]
fn a_run_reproduces() {
    let base = &named("wired-join").scenario;
    let mut short = base.clone();
    short.duration_ms = 100_000;
    short.join_at_ms = 60_000;
    short.leave_at_ms = None;
    short.l_group_us = 35_000;
    let house = &named("wired-join").house;
    let a = run_latency(&short, house, Mode::Stretch).expect("runs");
    let b = run_latency(&short, house, Mode::Stretch).expect("runs");
    assert_eq!(a, b);
    assert_eq!(a.failures(), Vec::<String>::new());
}

#[test]
fn the_committed_report_is_what_the_generator_writes() {
    let committed = read("docs/measurements/latency-growth-sim.md");
    let loaded = runs();
    let views: Vec<ScenarioRuns<'_>> = loaded
        .iter()
        .map(|l| ScenarioRuns {
            scenario: &l.scenario,
            house: &l.house,
            stretch: &l.stretch,
            naive: &l.naive,
        })
        .collect();
    let generated = report(&"0".repeat(40), "", &views);
    // The two build lines name the commit and say how the binary relates to
    // it; nothing else may differ.
    let body = |text: &str| -> Vec<String> {
        text.lines()
            .filter(|l| !l.starts_with("Build measured: ") && !l.starts_with("Build note: "))
            .map(str::to_string)
            .collect()
    };
    assert_eq!(
        body(&committed),
        body(&generated),
        "regenerate docs/measurements/latency-growth-sim.md with chorus-sim-latency"
    );
}
