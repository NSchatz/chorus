//! The house-scale simulation regression (goal 7 item 1, K75).
//!
//! The houses under `config/sim-house/` are Rust-only by design: the
//! endpoint's C core models one endpoint, not a house, so it has nothing to
//! read them with, and every file under `fixtures/sync/` must be read by both
//! implementations (`tools/conventions/check-shared-fixtures.sh`). The
//! single-endpoint arithmetic a house is built from IS held to the C mirror,
//! exchange by exchange, through `fixtures/sync/`.
//!
//! What is pinned here is a property of the model: the committed houses stay
//! inside BRIEF.md section 2.2's ACCEPTABLE bounds for every pair class. It is
//! a simulation property and not a timing claim (BRIEF.md section 3.1 rule 3).

use std::fs;
use std::path::{Path, PathBuf};

use chorus_sync::house::{PairClass, RoomSet, Transport};
use chorus_sync::{run_house, HouseConfig};

fn repo(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

fn house(file: &str) -> HouseConfig {
    let path = repo("config/sim-house").join(file);
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} is unreadable: {}", path.display(), e));
    HouseConfig::parse(&text).unwrap_or_else(|e| panic!("{}: {}", path.display(), e))
}

const HOUSES: [&str; 2] = ["8-rooms-switched.house", "8-rooms-routed.house"];

#[test]
fn every_committed_house_is_one_of_the_two_named_here() {
    let mut found: Vec<String> = fs::read_dir(repo("config/sim-house"))
        .expect("config/sim-house is readable")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    found.sort();
    let mut named: Vec<String> = HOUSES.iter().map(|s| s.to_string()).collect();
    named.sort();
    assert_eq!(found, named, "a house file this test does not grade");
}

#[test]
fn the_house_is_at_k75_scale() {
    for file in HOUSES {
        let h = house(file);
        let mut rooms: Vec<&str> = h.endpoints.iter().map(|e| e.room.as_str()).collect();
        rooms.sort();
        rooms.dedup();
        assert_eq!(rooms.len(), 8, "{}: K75 is up to 8 rooms", file);
        assert!(
            (18..=22).contains(&h.endpoints.len()),
            "{}: K75 is about 20 speakers, found {}",
            file,
            h.endpoints.len()
        );
        assert!(h.endpoints.iter().any(|e| e.set == RoomSet::Stereo));
        assert!(h.endpoints.iter().any(|e| e.set == RoomSet::Theater));
        assert!(
            (0..h.endpoints.len()).any(|i| h.transport(i) == Transport::Wireless),
            "{}: K91's compact Wi-Fi speakers are in the house",
            file
        );
    }
}

#[test]
fn the_switched_and_routed_houses_differ_only_in_their_links() {
    let switched = house(HOUSES[0]);
    let routed = house(HOUSES[1]);
    assert_eq!(switched.endpoints, routed.endpoints);
    assert_eq!(switched.seed, routed.seed);
    assert_eq!(switched.servo, routed.servo);
    assert_eq!(switched.server_ppm, routed.server_ppm);
    assert_eq!(switched.duration_ms, routed.duration_ms);
    assert_eq!(switched.settle_ms, routed.settle_ms);
    assert_eq!(switched.sync_interval_ms, routed.sync_interval_ms);
    assert_ne!(switched.links, routed.links);
}

/// The value of `key` in `config/sync.conf`.
fn sync_conf(key: &str) -> f64 {
    let text = fs::read_to_string(repo("config/sync.conf")).expect("config/sync.conf");
    text.lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter_map(|l| l.split_once('='))
        .find(|(k, _)| k.trim() == key)
        .map(|(_, v)| v.trim().parse::<f64>().expect("a number"))
        .unwrap_or_else(|| panic!("config/sync.conf has no {}", key))
}

#[test]
fn the_house_runs_the_real_clients_servo_constants() {
    for file in HOUSES {
        let h = house(file);
        assert_eq!(h.sync_interval_ms as f64, sync_conf("sync_interval_ms"));
        assert_eq!(h.servo.filter_window as f64, sync_conf("filter_window"));
        assert_eq!(h.servo.smoothing_alpha, sync_conf("smoothing_alpha"));
        assert_eq!(
            h.servo.hard_resync_threshold_ns,
            sync_conf("hard_resync_threshold_us") * 1_000.0
        );
        assert_eq!(h.servo.max_correction_ppm, sync_conf("max_correction_ppm"));
        // kp and ki are compiled constants of the client, not lines of
        // sync.conf (crates/client-linux/src/sync.rs SERVO_KP, SERVO_KI).
        assert_eq!(h.servo.kp, 0.4);
        assert_eq!(h.servo.ki, 0.08);
    }
}

#[test]
fn the_house_stays_inside_the_brief_acceptable_bounds() {
    for file in HOUSES {
        let h = house(file);
        let result = run_house(&h).expect("the committed house runs");
        let classes: Vec<PairClass> = result.classes.iter().map(|c| c.class).collect();
        assert_eq!(
            classes,
            PairClass::ALL.to_vec(),
            "{}: every pair class is present",
            file
        );
        for class in &result.classes {
            println!(
                "{}: {}: {} pairs, p50 {} ns, p95 {} ns, max {} ns, acceptable {} ns: {}",
                file,
                class.class.label(),
                class.pairs,
                class.p50_ns,
                class.p95_ns,
                class.max_ns,
                class.class.acceptable_ns(),
                class.verdict()
            );
            assert!(
                class.within_acceptable(),
                "{}: {} reaches {} ns against BRIEF 2.2's acceptable {} ns",
                file,
                class.class.label(),
                class.max_ns,
                class.class.acceptable_ns()
            );
        }
        for endpoint in &result.endpoints {
            assert_eq!(
                endpoint.hard_resyncs_after_settle, 0,
                "{}: {} stepped after the settle time",
                file, endpoint.id
            );
        }
        let pairs: usize = result.classes.iter().map(|c| c.pairs).sum();
        let n = h.endpoints.len();
        assert_eq!(pairs, n * (n - 1) / 2, "{}: every pair is in a class", file);
    }
}

#[test]
fn a_house_run_is_deterministic() {
    let h = house(HOUSES[0]);
    let first = run_house(&h).expect("runs");
    let second = run_house(&h).expect("runs again");
    assert_eq!(first, second);
}
