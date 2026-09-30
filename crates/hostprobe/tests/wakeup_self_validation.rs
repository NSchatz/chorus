//! The probe can see a late wakeup: inject known ones and find every one of them.
//!
//! Deterministic by construction, not by headroom: an injected wakeup is made late
//! by sleeping until `deadline + inject` first, and a sleep never ends before its
//! deadline (clock_nanosleep(2)), so every injected lateness is at least `inject`
//! however busy the worker is. Nothing here asserts that an UNinjected wakeup is
//! quick, because on a shared worker that is exactly what cannot be promised.

use chorus_hostprobe::histogram::count_at_or_above;
use chorus_hostprobe::wakeup::{run, Config};

const PERIOD_NS: u64 = 1_000_000;
const INJECT_NS: u64 = 3_000_000;

#[test]
fn every_injected_wakeup_is_in_the_histogram_at_or_above_the_injection() {
    let config = Config {
        period_ns: PERIOD_NS,
        wakeups: 1_000,
        inject_every: 10,
        inject_ns: INJECT_NS,
    };
    let r = run(&config).expect("the loop runs");
    assert_eq!(r.lateness_ns.len(), 1_000);
    let injected: Vec<i64> = r
        .lateness_ns
        .iter()
        .zip(&r.injected)
        .filter(|(_, &i)| i)
        .map(|(&l, _)| l)
        .collect();
    assert_eq!(
        injected.len(),
        100,
        "every 10th of 1000 wakeups is injected"
    );
    for l in &injected {
        assert!(
            *l >= INJECT_NS as i64,
            "an injected wakeup was only {} ns late",
            l
        );
    }
    // 10% of the samples sit at or above the injection, so the p99 and p99.9 must
    // too, and the max; the histogram cannot report this run as quiet.
    let s = r.summary().unwrap();
    assert!(
        s.p99 >= INJECT_NS as i64,
        "p99 {} ns missed the injection",
        s.p99
    );
    assert!(s.p999 >= INJECT_NS as i64 && s.max >= INJECT_NS as i64);
    assert!(count_at_or_above(&r.lateness_ns, INJECT_NS as i64) >= 100);
    // An injected wakeup lands past the next deadline, so each one is an overrun.
    assert!(
        r.overruns >= 100,
        "only {} overruns for 100 injected late wakeups",
        r.overruns
    );
    // The uninjected view leaves exactly the other 900 out of that count.
    assert_eq!(r.summary_uninjected().unwrap().count, 900);
}

#[test]
fn no_injection_means_nothing_is_marked_and_no_wakeup_is_early() {
    let r = run(&Config {
        period_ns: PERIOD_NS,
        wakeups: 200,
        inject_every: 0,
        inject_ns: INJECT_NS,
    })
    .expect("the loop runs");
    assert!(r.injected.iter().all(|&i| !i));
    // An absolute sleep never returns before its deadline.
    assert!(
        r.lateness_ns.iter().all(|&l| l >= 0),
        "a wakeup came before its deadline"
    );
}

#[test]
fn a_zero_period_is_refused() {
    let e = run(&Config {
        period_ns: 0,
        wakeups: 1,
        inject_every: 0,
        inject_ns: 0,
    })
    .unwrap_err();
    assert_eq!(e.kind(), std::io::ErrorKind::InvalidInput);
}

#[test]
fn timer_slack_can_be_set_to_one_nanosecond_without_privilege() {
    // PR_SET_TIMERSLACK(2const) names no capability; this test runs unprivileged.
    let (read, err) = chorus_hostprobe::wakeup::apply_timer_slack(1);
    assert!(err.is_none(), "setting 1 ns failed: {:?}", err);
    assert_eq!(read, Some(1));
}
