//! `chorus-rt-spin`: a real-time thread with a deliberate non-yielding loop
//! in it, so the CPU-time bound can be shown to FIRE rather than merely to be
//! configured.
//!
//! `sched(7)`: "A nonblocking infinite loop in a thread scheduled under the
//! SCHED_FIFO, SCHED_RR, or SCHED_DEADLINE policy can potentially block all
//! other threads from accessing the CPU forever." This binary is that loop, on
//! purpose, with the documented safeguard in place. Run beside a
//! normal-priority heartbeat, it answers the only question that matters about
//! the safeguard: does the host keep making progress, and does the offender
//! die.
//!
//! It is a separate binary rather than a flag on the server precisely so that
//! nothing which ships as the server contains a spin loop. There is no
//! configuration of `chorus-server` that spins.
//!
//! ```text
//! chorus-rt-spin --rttime-us 200000 --rt-priority 20
//! ```
//!
//! Exit codes:
//! - `3`  the host granted no real-time priority, so the bound cannot be shown
//!        to fire here. This is the missing-prerequisite exit, and it names
//!        both the prerequisite and the criterion.
//! - killed by `SIGXCPU`  the expected outcome: the bound fired.
//! - `1`  the spin ran far past its bound and was still alive, which is the
//!        failure this test exists to catch.

use std::io::Write;
use std::process::ExitCode;
use std::time::Instant;

use chorus_hostctl::{
    bound_real_time_cpu_time, current_policy, policy_name, rtprio_ceiling, take_real_time_policy,
    thread_id,
};

const EXIT_NO_PREREQUISITE: u8 = 3;
const EXIT_BOUND_DID_NOT_FIRE: u8 = 1;

fn main() -> ExitCode {
    let mut rttime_us = 200_000u64;
    let mut rt_priority = 20u32;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--rttime-us" => rttime_us = args.next().and_then(|v| v.parse().ok()).unwrap_or(rttime_us),
            "--rt-priority" => {
                rt_priority = args.next().and_then(|v| v.parse().ok()).unwrap_or(rt_priority)
            }
            other => {
                eprintln!("chorus-rt-spin: unknown argument '{}'", other);
                return ExitCode::from(2);
            }
        }
    }

    let ceiling = match rtprio_ceiling() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("chorus-rt-spin: {}", e);
            return ExitCode::from(EXIT_NO_PREREQUISITE);
        }
    };
    if ceiling.soft == 0 {
        eprintln!(
            "chorus-rt-spin: missing prerequisite: a granted rtprio ceiling above zero. \
             RLIMIT_RTPRIO reads {} here. This entry point verifies that a real-time thread \
             which runs without yielding past its CPU-time limit is terminated by that limit \
             and does not starve the host; it cannot be verified without a real-time policy, \
             and it is NOT reported as passed, skipped or otherwise satisfied. \
             Grant one with `docker run --ulimit rtprio=<n>`.",
            ceiling.soft
        );
        return ExitCode::from(EXIT_NO_PREREQUISITE);
    }

    let applied = match bound_real_time_cpu_time(rttime_us) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("chorus-rt-spin: {}", e);
            return ExitCode::from(EXIT_NO_PREREQUISITE);
        }
    };
    let grant = match take_real_time_policy(rt_priority) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("chorus-rt-spin: {}", e);
            return ExitCode::from(EXIT_NO_PREREQUISITE);
        }
    };

    println!(
        "chorus-rt-spin: tid={} rtprio_ceiling={} rtprio_obtained={} policy={} rttime_us={}",
        thread_id(),
        grant.ceiling,
        grant.priority,
        policy_name(current_policy()),
        applied.soft
    );
    println!(
        "chorus-rt-spin: spinning without yielding; the bound above has to terminate this \
         process, and a normal-priority heartbeat beside it has to keep making progress"
    );
    let _ = std::io::stdout().flush();

    // The deliberate non-yielding loop. No syscall, no sleep, no allocation:
    // nothing in here gives the CPU up voluntarily.
    let started = Instant::now();
    let mut accumulator: u64 = 0;
    let give_up_after = std::time::Duration::from_micros(rttime_us.saturating_mul(50).max(5_000_000));
    loop {
        accumulator = accumulator.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        if accumulator == 0 {
            // Unreachable in practice; here so the loop cannot be optimised
            // away as having no effect.
            std::hint::black_box(accumulator);
        }
        // A cheap counter, checked rarely, so the loop stays non-yielding: an
        // Instant::now() every iteration would be a syscall on some kernels
        // and would make this a yielding loop rather than the one sched(7)
        // warns about.
        if accumulator % (1 << 26) == 0 && started.elapsed() > give_up_after {
            eprintln!(
                "chorus-rt-spin: the CPU-time bound of {} us did NOT fire: this process is still \
                 alive after {} s of non-yielding real-time execution",
                applied.soft,
                started.elapsed().as_secs()
            );
            return ExitCode::from(EXIT_BOUND_DID_NOT_FIRE);
        }
    }
}
