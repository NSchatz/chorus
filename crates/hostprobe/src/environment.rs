//! What the host looked like around a run, read from `/proc` and `/sys`.
//!
//! A host number without its environment is not a number anyone can compare, so
//! every probe prints these facts before and after it runs. Nothing here reads a
//! clock; a fact that cannot be read is printed as `unreadable (<reason>)`, never
//! guessed.

use std::fs;

/// Read a small text file and trim it, or say why it could not be read.
pub fn read_fact(path: &str) -> String {
    match fs::read_to_string(path) {
        Ok(s) => s.trim().to_string(),
        Err(e) => format!("unreadable ({})", e),
    }
}

/// The cgroup v2 directory this process is in, under `/sys/fs/cgroup`.
pub fn cgroup_dir() -> String {
    let own = fs::read_to_string("/proc/self/cgroup").unwrap_or_default();
    // cgroup v2 has one line, "0::<path>".
    let rel = own
        .lines()
        .find_map(|l| l.strip_prefix("0::"))
        .unwrap_or("/")
        .trim();
    format!("/sys/fs/cgroup{}", if rel == "/" { "" } else { rel })
}

/// `cpu.max` rendered as a CPU count: "2800000 100000" is "28.00 CPUs".
pub fn cpu_quota() -> String {
    let raw = read_fact(&format!("{}/cpu.max", cgroup_dir()));
    let mut parts = raw.split_whitespace();
    match (
        parts.next(),
        parts.next().and_then(|p| p.parse::<f64>().ok()),
    ) {
        (Some("max"), _) => format!("{} (no quota)", raw),
        (Some(q), Some(period)) if period > 0.0 => match q.parse::<f64>() {
            Ok(quota) => format!("{} ({:.2} CPUs)", raw, quota / period),
            Err(_) => raw,
        },
        _ => raw,
    }
}

/// The `nr_throttled` and `throttled_usec` lines of the cgroup's `cpu.stat`.
pub fn cpu_throttling() -> String {
    let stat = read_fact(&format!("{}/cpu.stat", cgroup_dir()));
    let picked: Vec<&str> = stat
        .lines()
        .filter(|l| {
            l.starts_with("nr_periods")
                || l.starts_with("nr_throttled")
                || l.starts_with("throttled_usec")
        })
        .collect();
    if picked.is_empty() {
        stat
    } else {
        picked.join(", ")
    }
}

/// This thread's voluntary and involuntary context switch counts.
pub fn context_switches() -> String {
    let status = read_fact("/proc/thread-self/status");
    let picked: Vec<&str> = status
        .lines()
        .filter(|l| l.contains("ctxt_switches"))
        .map(|l| l.trim())
        .collect();
    picked.join(", ").replace('\t', " ")
}

/// The CPUs this process may run on, as the kernel lists them.
pub fn cpus_allowed() -> String {
    let status = read_fact("/proc/self/status");
    status
        .lines()
        .find_map(|l| l.strip_prefix("Cpus_allowed_list:"))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unreadable".to_string())
}

/// Count the CPUs in a list such as "0-27,56".
pub fn count_cpu_list(list: &str) -> Option<usize> {
    let mut n = 0usize;
    for part in list.split(',').filter(|p| !p.trim().is_empty()) {
        let mut ends = part.trim().splitn(2, '-');
        let a: usize = ends.next()?.parse().ok()?;
        let b: usize = match ends.next() {
            Some(b) => b.parse().ok()?,
            None => a,
        };
        n += b.checked_sub(a)? + 1;
    }
    Some(n)
}

/// The facts printed once, before a run: the ones that do not move during it.
pub fn static_facts() -> Vec<(&'static str, String)> {
    let allowed = cpus_allowed();
    let rtprio = chorus_hostctl::rtprio_ceiling()
        .map(|r| r.soft_display())
        .unwrap_or_else(|e| format!("unreadable ({})", e));
    let memlock = chorus_hostctl::memlock_limit()
        .map(|r| r.soft_display())
        .unwrap_or_else(|e| format!("unreadable ({})", e));
    vec![
        ("kernel", read_fact("/proc/sys/kernel/osrelease")),
        ("kernel_version", read_fact("/proc/sys/kernel/version")),
        (
            "clocksource",
            read_fact("/sys/devices/system/clocksource/clocksource0/current_clocksource"),
        ),
        (
            "preempt_dynamic",
            read_fact("/sys/kernel/debug/sched/preempt"),
        ),
        ("realtime_kernel", read_fact("/sys/kernel/realtime")),
        ("cpus_online", read_fact("/sys/devices/system/cpu/online")),
        (
            "cpus_allowed",
            format!(
                "{} ({} CPUs)",
                allowed,
                count_cpu_list(&allowed).map_or("?".to_string(), |n| n.to_string())
            ),
        ),
        ("cgroup_cpu_max", cpu_quota()),
        ("rlimit_rtprio", rtprio),
        ("rlimit_memlock", memlock),
    ]
}

/// The facts printed before and after a run: the ones the run is measured against.
pub fn moving_facts() -> Vec<(&'static str, String)> {
    vec![
        ("loadavg", read_fact("/proc/loadavg")),
        (
            "psi_cpu",
            read_fact("/proc/pressure/cpu").replace('\n', " | "),
        ),
        ("cgroup_cpu_stat", cpu_throttling()),
        ("thread_ctxt_switches", context_switches()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_lists_count() {
        assert_eq!(count_cpu_list("0-27,56"), Some(29));
        assert_eq!(count_cpu_list("3"), Some(1));
        assert_eq!(count_cpu_list("0-1,4-5"), Some(4));
        assert_eq!(count_cpu_list("5-3"), None);
    }
}
