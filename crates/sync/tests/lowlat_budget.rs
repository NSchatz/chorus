//! The low-latency TV path in the simulator (goal 13): the budget the
//! defaults promise holds chunk by chunk, the FEC removes the residual loss
//! its negative control shows, a lead below the floor is refused for a
//! reason, and the committed report is what the generator writes.
//!
//! The scenarios are `config/sim-lowlat/*.lowlat`, Rust-only for the reason
//! the houses are (ADR 0048). What is pinned is a property of the model, not
//! a timing claim (BRIEF.md section 3.1 rule 3).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use chorus_protocol::v2::lowlat::DEFAULTS;
use chorus_sync::lowlat_sim::{
    bernoulli_expectation, report, run_lowlat, run_with_latency, LossModel, LowLatResult,
    LowLatScenario, SCENARIOS,
};

fn repo(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

fn read(path: &str) -> String {
    let path = repo(path);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{} is unreadable: {}", path.display(), e))
}

/// Every committed scenario, run once for all the tests here.
fn runs() -> &'static [(LowLatScenario, LowLatResult)] {
    static RUNS: OnceLock<Vec<(LowLatScenario, LowLatResult)>> = OnceLock::new();
    RUNS.get_or_init(|| {
        SCENARIOS
            .iter()
            .map(|name| {
                let s = LowLatScenario::parse(&read(&format!("config/sim-lowlat/{}", name)))
                    .unwrap_or_else(|e| panic!("{}: {}", name, e));
                let r = run_lowlat(&s).unwrap_or_else(|e| panic!("{}: {}", name, e));
                (s, r)
            })
            .collect()
    })
}

fn named(name: &str) -> &'static (LowLatScenario, LowLatResult) {
    runs()
        .iter()
        .find(|(s, _)| s.name == name)
        .unwrap_or_else(|| panic!("no scenario {}", name))
}

#[test]
fn every_committed_scenario_is_listed_and_named_after_its_file() {
    let mut on_disk: Vec<String> = fs::read_dir(repo("config/sim-lowlat"))
        .expect("config/sim-lowlat exists")
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    on_disk.sort();
    let mut listed: Vec<String> = SCENARIOS.iter().map(|s| s.to_string()).collect();
    listed.sort();
    assert_eq!(on_disk, listed);
    for (s, _) in runs() {
        assert!(SCENARIOS.contains(&format!("{}.lowlat", s.name).as_str()));
    }
}

#[test]
fn the_default_plan_is_inside_its_range_and_above_its_floor() {
    assert_eq!(DEFAULTS.chunk_ns(), 2_500_000);
    assert!(DEFAULTS.l_tv_ns >= DEFAULTS.floor_ns());
    assert!(DEFAULTS.check_latency(DEFAULTS.l_tv_ns).is_ok());
    let budget: u64 = DEFAULTS.budget().iter().map(|i| i.ns).sum();
    assert_eq!(budget, DEFAULTS.floor_ns());
    println!(
        "defaults: chunk {} ms, k {} depth {}, FEC wait {} ms, floor {} ms, L_tv {} ms",
        DEFAULTS.chunk_ns() as f64 / 1e6,
        DEFAULTS.fec_k,
        DEFAULTS.fec_depth,
        DEFAULTS.fec_wait_ns() as f64 / 1e6,
        DEFAULTS.floor_ns() as f64 / 1e6,
        DEFAULTS.l_tv_ns as f64 / 1e6
    );
}

#[test]
fn at_or_above_the_floor_no_chunk_is_ever_late() {
    for (s, r) in runs() {
        assert!(s.plan.l_tv_ns >= s.plan.floor_ns(), "{}", s.name);
        assert_eq!(r.fec.late, 0, "{}: a chunk past its playout point", s.name);
        assert_eq!(r.control.late, 0, "{}", s.name);
        assert!(r.fec.min_lead_ns >= 0, "{}", s.name);
        // The worst repaired chunk is ready inside the budget's own part
        // before the endpoint's output path.
        let before_output = s.plan.floor_ns() - s.plan.endpoint_output_ns - s.plan.dac_filter_ns;
        assert!(
            r.fec.worst_repaired_ns <= before_output,
            "{}: {} > {}",
            s.name,
            r.fec.worst_repaired_ns,
            before_output
        );
        assert_eq!(
            r.fec.on_time + r.fec.late + r.fec.lost,
            r.fec.chunks,
            "{}",
            s.name
        );
    }
}

#[test]
fn below_the_floor_a_repaired_chunk_misses_its_playout_point() {
    // The floor rule's own negative control: the same independent loss with
    // L_tv 4 ms under the floor makes repaired chunks late.
    let (s, r) = named("wired-bernoulli");
    let below = run_with_latency(s, s.plan.floor_ns() - 4_000_000).unwrap();
    println!(
        "wired-bernoulli at L_tv {} ms (floor {} ms): {} late, against {} at the default",
        (s.plan.floor_ns() - 4_000_000) as f64 / 1e6,
        s.plan.floor_ns() as f64 / 1e6,
        below.late,
        r.fec.late
    );
    assert!(below.late > 0);
    assert!(below.residual() > r.fec.residual());
}

#[test]
fn a_clean_wired_link_loses_nothing() {
    let (_, r) = named("wired-clean");
    assert_eq!((r.fec.residual(), r.control.residual()), (0, 0));
}

#[test]
fn fec_removes_the_independent_loss_its_control_shows() {
    let (s, r) = named("wired-bernoulli");
    let (c, cr) = named("wired-bernoulli-nofec");
    let LossModel::Bernoulli { p } = s.up.loss else {
        panic!("wired-bernoulli is independent loss");
    };
    // The control scenario is the same draws without FEC: the same chunks
    // lost, the same residual as this scenario's own control run.
    assert_eq!(
        (c.seed, c.up.loss, c.down.loss),
        (s.seed, s.up.loss, s.down.loss)
    );
    assert_eq!(cr.fec.residual(), r.control.residual());
    assert_eq!(cr.fec.up.data_lost, r.fec.up.data_lost);
    let chunks = r.fec.chunks as f64;
    let expected_without = bernoulli_expectation(p, 0) * chunks;
    let expected_with = bernoulli_expectation(p, s.plan.fec_k) * chunks;
    println!(
        "wired-bernoulli: residual {} with FEC (expected {:.1}), {} without (expected {:.1})",
        r.fec.residual(),
        expected_with,
        r.control.residual(),
        expected_without
    );
    // Within five standard deviations of the binomial expectation.
    let sd = expected_without.sqrt();
    assert!((r.control.residual() as f64 - expected_without).abs() < 5.0 * sd);
    assert!((r.fec.residual() as f64) < expected_with + 5.0 * expected_with.sqrt() + 1.0);
    // FEC removes at least 99 % of it.
    assert!(r.fec.residual() * 100 <= r.control.residual());
    assert!(r.fec.up.recovered + r.fec.down.recovered > 0);
}

#[test]
fn bursts_beat_a_single_parity_and_the_interleave_wins_some_back() {
    let (_, flat) = named("wired-bursts");
    let (_, deep) = named("wired-bursts-interleaved");
    // The same draws: the same chunks lost before any FEC.
    assert_eq!(flat.control.residual(), deep.control.residual());
    println!(
        "bursts (mean 2 chunks): residual {} without FEC, {} with k 4, {} with k 4 depth 2",
        flat.control.residual(),
        flat.fec.residual(),
        deep.fec.residual()
    );
    assert!(flat.fec.residual() < flat.control.residual());
    assert!(deep.fec.residual() < flat.fec.residual());
    // A burst model makes runs a single parity cannot cover.
    assert!(flat.fec.up.longest_burst >= 3);
}

#[test]
fn the_lip_sync_verdicts_are_the_window_applied() {
    for (s, _) in runs() {
        let expected = !s.name.contains("game-mode");
        assert_eq!(
            s.lip_sync_ok(),
            expected,
            "{}: {} ns",
            s.name,
            s.lip_sync_ns()
        );
    }
    let (s, _) = named("wired-clean");
    // -(1 ms TV lag + 0.0625 ms S/PDIF receiver + L_tv 25 ms), ADR 0093.
    assert_eq!(s.lip_sync_ns(), -26_062_500);
}

#[test]
fn a_run_repeats_exactly() {
    let (s, r) = named("wired-bursts");
    assert_eq!(&run_lowlat(s).unwrap(), r);
}

#[test]
fn the_committed_report_is_what_the_generator_writes() {
    let committed = read("docs/measurements/low-latency-budget-sim.md");
    let generated = report(&"0".repeat(40), "", runs());
    // The two provenance lines name the build and how the binary relates to
    // it; nothing else may differ.
    let strip = |t: &str| -> Vec<String> {
        t.lines()
            .filter(|l| !l.starts_with("Build measured: ") && !l.starts_with("Build note: "))
            .map(str::to_string)
            .collect()
    };
    assert!(
        strip(&committed) == strip(&generated),
        "regenerate docs/measurements/low-latency-budget-sim.md with chorus-sim-lowlat"
    );
    assert!(committed.contains("\nSource: simulation\n"));
    assert!(committed.contains("NOT TIMING EVIDENCE"));
}
