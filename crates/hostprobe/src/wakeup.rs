//! The wakeup-jitter probe: how late does a periodic thread wake?
//!
//! The loop is the shape of an audio thread's: an absolute deadline on
//! `CLOCK_MONOTONIC` every `period`, `clock_nanosleep(TIMER_ABSTIME)` until it,
//! and the lateness (`wake - deadline`) recorded for every wakeup. Deadlines are
//! absolute and advance by exactly one period, so one late wakeup does not drift
//! every later one; a wakeup so late that it passed the next deadline counts the
//! deadlines it skipped as overruns and resumes on the next one still ahead.
//!
//! # Self-validation
//!
//! A probe that reports a quiet host because it cannot see anything is worse
//! than none. [`Config::inject_every`] and [`Config::inject_ns`] make every n-th
//! wakeup late ON PURPOSE by at least `inject_ns`: before that sleep the loop first
//! sleeps (relative to its own clock) until `deadline + inject_ns`, so the
//! absolute sleep returns at once and the recorded lateness is at least
//! `inject_ns`, by the kernel's own guarantee that a sleep never ends early. The
//! test in `tests/wakeup_self_validation.rs` checks that the histogram shows
//! every one of them.
//!
//! Only `CLOCK_MONOTONIC` is read (see `sys.rs`).

use std::io;

use crate::histogram::{self, Summary};
use crate::sys;

/// One probe run's parameters.
#[derive(Debug, Clone)]
pub struct Config {
    /// The period between deadlines, in nanoseconds.
    pub period_ns: u64,
    /// How many deadlines to sleep for.
    pub wakeups: usize,
    /// Make every n-th wakeup late on purpose (0: never).
    pub inject_every: usize,
    /// How late an injected wakeup is made, at least, in nanoseconds.
    pub inject_ns: u64,
}

/// One probe run's result.
#[derive(Debug, Clone)]
pub struct Run {
    /// Lateness of every wakeup, in nanoseconds, in the order they happened.
    pub lateness_ns: Vec<i64>,
    /// Which wakeups were made late on purpose (same indices as `lateness_ns`).
    pub injected: Vec<bool>,
    /// Deadlines skipped because a wakeup came after the next one.
    pub overruns: u64,
    /// Monotonic nanoseconds the run took, first deadline to last wakeup.
    pub elapsed_ns: u64,
}

impl Run {
    /// The order statistics of every wakeup's lateness.
    pub fn summary(&self) -> Option<Summary> {
        histogram::summarise(&mut self.lateness_ns.clone())
    }

    /// The order statistics of the wakeups NOT made late on purpose.
    pub fn summary_uninjected(&self) -> Option<Summary> {
        let mut v: Vec<i64> = self
            .lateness_ns
            .iter()
            .zip(&self.injected)
            .filter(|(_, &i)| !i)
            .map(|(&l, _)| l)
            .collect();
        histogram::summarise(&mut v)
    }
}

/// Run the probe on the calling thread, under whatever policy and slack it has.
pub fn run(config: &Config) -> io::Result<Run> {
    if config.period_ns == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "period must be positive",
        ));
    }
    let mut lateness_ns = Vec::with_capacity(config.wakeups);
    let mut injected = Vec::with_capacity(config.wakeups);
    let mut overruns = 0u64;
    let start = sys::monotonic_ns();
    let mut deadline = start + config.period_ns;
    for i in 0..config.wakeups {
        let inject =
            config.inject_every > 0 && config.inject_ns > 0 && (i + 1) % config.inject_every == 0;
        if inject {
            sys::sleep_until_monotonic_ns(deadline + config.inject_ns)?;
        }
        sys::sleep_until_monotonic_ns(deadline)?;
        let woke = sys::monotonic_ns();
        lateness_ns.push(woke as i64 - deadline as i64);
        injected.push(inject);
        deadline += config.period_ns;
        if woke >= deadline {
            let skipped = (woke - deadline) / config.period_ns + 1;
            overruns += skipped;
            deadline += skipped * config.period_ns;
        }
    }
    Ok(Run {
        lateness_ns,
        injected,
        overruns,
        elapsed_ns: sys::monotonic_ns() - start,
    })
}

/// Set the timer slack and report what the kernel then says it is.
///
/// Returns (the value read back, the error if setting it failed).
pub fn apply_timer_slack(slack_ns: u64) -> (Option<u64>, Option<io::Error>) {
    let set = sys::set_timer_slack_ns(slack_ns).err();
    (sys::timer_slack_ns().ok(), set)
}
