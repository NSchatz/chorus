//! The playout thread's real-time policy, when the operator asks for one.
//!
//! `--rt-priority <n>` runs the playout loop (the thread that writes to the
//! ALSA device, `run::run_session`'s caller) under `SCHED_FIFO`; without it,
//! the default, every thread of the client stays `SCHED_OTHER`. The order is
//! the repository's (ADR 0022, `real-time-acquisitions.conf`): the CPU-time
//! bound (`RLIMIT_RTTIME`, `--rttime-us`) goes on first, then the policy is
//! taken through `chorus_hostctl`, clamped to the rtprio ceiling the host
//! granted (under systemd, `LimitRTPRIO=` in `chorus-client.service`).
//!
//! Only the playout thread. The receiving thread is spawned before the policy
//! is taken, so it stays `SCHED_OTHER`, and the policy is left again when the
//! session ends ([`PlayoutRealTime`]'s `Drop`), because `pthread_create(3)`
//! gives a new thread its creator's policy: the next session's receiving
//! thread would otherwise be real-time without anyone deciding so.
//!
//! A host that grants no ceiling is found at start ([`check_host`]) and the
//! client refuses to run (exit 2, `stopped reason=real-time-refused`), rather
//! than playing as a normal process while its configuration says otherwise.
//!
//! This unit reads no clock.

use chorus_hostctl::{
    bound_real_time_cpu_time, leave_real_time_policy, policy_name, rtprio_ceiling,
    take_real_time_policy, HostError,
};

use crate::config::ClientConfig;

/// What the operator is told when a ceiling is missing, beside the host's own
/// words: where an installed endpoint grants one.
const WHERE_TO_GRANT: &str = "on an installed endpoint the ceiling is LimitRTPRIO= in \
     chorus-client.service (and the bound's ceiling LimitRTTIME=)";

/// Check at start that a requested real-time policy can be had: the CPU-time
/// bound is settable and the rtprio ceiling is above zero. `Ok(None)` when no
/// policy was asked for; `Ok(Some(line))` names what was found.
pub fn check_host(config: &ClientConfig) -> Result<Option<String>, String> {
    let Some(priority) = config.rt_priority else {
        return Ok(None);
    };
    let bound = bound_real_time_cpu_time(config.rttime_us)
        .map_err(|e| format!("{}; {}", e, WHERE_TO_GRANT))?;
    let ceiling = rtprio_ceiling().map_err(|e| e.to_string())?;
    if ceiling.soft == 0 {
        return Err(format!(
            "{}; {}",
            HostError::CeilingIsZero {
                ceiling: ceiling.soft
            },
            WHERE_TO_GRANT
        ));
    }
    Ok(Some(format!(
        "real-time requested thread=playout priority={} ceiling={} rttime_us={}",
        priority,
        ceiling.soft,
        bound.soft_display()
    )))
}

/// The playout thread under `SCHED_FIFO`, for as long as this lives.
#[derive(Debug)]
pub struct PlayoutRealTime {
    /// What was obtained, as the status line says it.
    pub line: String,
}

impl Drop for PlayoutRealTime {
    fn drop(&mut self) {
        if let Err(e) = leave_real_time_policy() {
            eprintln!(
                "chorus-client: the playout thread could not leave its real-time policy: {}",
                e
            );
        }
    }
}

/// Take the policy for the calling thread, the bound first. `Ok(None)` when
/// none was asked for.
pub fn take_for_playout(config: &ClientConfig) -> Result<Option<PlayoutRealTime>, String> {
    let Some(priority) = config.rt_priority else {
        return Ok(None);
    };
    // The bound before the policy: between the two a thread would be
    // real-time and unbounded (sched(7); ADR 0022).
    let bound = bound_real_time_cpu_time(config.rttime_us).map_err(|e| e.to_string())?;
    let grant = take_real_time_policy(priority).map_err(|e| e.to_string())?;
    Ok(Some(PlayoutRealTime {
        line: format!(
            "real-time thread=playout policy={} priority={} ceiling={} rttime_us={}",
            policy_name(grant.policy),
            grant.priority,
            grant.ceiling,
            bound.soft_display()
        ),
    }))
}

/// [`take_for_playout`], with what happened said on standard output (the
/// status line) or standard error (a failure, after which the session plays
/// on under `SCHED_OTHER`: [`check_host`] already refused a host with no
/// ceiling at start, so this is a kernel refusal nobody configured, and
/// stopping the music over it would be the worse answer).
pub fn enter_playout(config: &ClientConfig) -> Option<PlayoutRealTime> {
    match take_for_playout(config) {
        Ok(Some(taken)) => {
            println!("chorus-client: {}", taken.line);
            Some(taken)
        }
        Ok(None) => None,
        Err(e) => {
            eprintln!(
                "chorus-client: the playout thread could not take its real-time policy: {}; \
                 this session plays under SCHED_OTHER",
                e
            );
            println!("chorus-client: real-time thread=playout policy=SCHED_OTHER refused=1");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_taken_or_checked_when_nothing_was_asked_for() {
        let config = ClientConfig::default();
        assert_eq!(check_host(&config), Ok(None));
        assert!(take_for_playout(&config).unwrap().is_none());
    }

    #[test]
    fn a_host_with_no_ceiling_is_refused_by_name_and_one_with_a_ceiling_is_used() {
        // The answer depends on the host running the suite, so both are
        // graded: with no ceiling (a development container, `ulimit -r` 0) the
        // refusal names the ceiling and where an endpoint grants one; with one,
        // the playout policy is taken, reported and left again.
        let config = ClientConfig {
            rt_priority: Some(1),
            rttime_us: 200_000,
            ..ClientConfig::default()
        };
        let ceiling = rtprio_ceiling().expect("RLIMIT_RTPRIO is readable").soft;
        if ceiling == 0 {
            let refused = check_host(&config).unwrap_err();
            assert!(refused.contains("ceiling is 0"), "{}", refused);
            assert!(refused.contains("LimitRTPRIO="), "{}", refused);
        } else {
            let line = check_host(&config).unwrap().unwrap();
            assert!(line.contains("thread=playout"), "{}", line);
            // On a thread of its own, so the suite's threads are never left
            // real-time whatever happens here.
            std::thread::spawn(move || {
                let taken = take_for_playout(&config).unwrap().unwrap();
                assert!(taken.line.contains("policy=SCHED_FIFO"), "{}", taken.line);
                drop(taken);
                assert_eq!(chorus_hostctl::current_policy(), chorus_hostctl::SCHED_OTHER);
            })
            .join()
            .unwrap();
        }
    }
}
