//! Taking the host contract, and reporting every part of it.
//!
//! The report is not decoration. Three of this phase's assertions are checked
//! against it: the priority obtained sits inside the granted ceiling, every
//! real-time thread carries a CPU-time bound, and no thread runs under a
//! real-time policy that was not reported. A report nobody can compare against
//! `/proc` would be a claim rather than a property, so the comparison is done
//! here, in the process, and printed.

use std::fmt;

use chorus_hostctl::{
    bound_real_time_cpu_time, lock_memory, memlock_limit, policy_name, rtprio_ceiling,
    take_real_time_policy, thread_facts, thread_id, undeclared_real_time_threads, HostError,
    RegisteredThread, Rlimit, ThreadRegistry,
};

/// What the server decided about the host contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RealTimeOutcome {
    /// A real-time policy was obtained.
    Granted {
        /// The ceiling that was read.
        ceiling: u64,
        /// The priority obtained.
        priority: u32,
        /// The policy obtained.
        policy: i32,
        /// The CPU-time bound in force, microseconds.
        rttime_us: u64,
    },
    /// The host granted none, and configuration explicitly allowed running
    /// without one.
    RunningWithout {
        /// The ceiling that was read.
        ceiling: u64,
        /// The priority that was wanted.
        wanted: u32,
    },
}

/// What the server decided about locked memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryLockOutcome {
    /// Audio-path memory is locked.
    Locked {
        /// The locked-memory limit that was read, bytes.
        limit_bytes: u64,
        /// The amount that was wanted, bytes.
        wanted_bytes: u64,
    },
    /// Locking was denied, and configuration explicitly allowed running
    /// unlocked.
    RunningUnlocked {
        /// The locked-memory limit that was read, bytes.
        limit_bytes: u64,
        /// The amount that was wanted, bytes.
        wanted_bytes: u64,
    },
    /// The server was configured not to attempt it.
    NotAttempted {
        /// The locked-memory limit that was read, bytes.
        limit_bytes: u64,
    },
}

impl MemoryLockOutcome {
    /// Whether every status report has to say this run is unlocked.
    pub fn is_unlocked(&self) -> bool {
        !matches!(self, MemoryLockOutcome::Locked { .. })
    }

    /// The phrase every status report carries.
    pub fn phrase(&self) -> String {
        match self {
            MemoryLockOutcome::Locked {
                limit_bytes,
                wanted_bytes,
            } => format!(
                "memory=locked memlock_limit_bytes={} memlock_wanted_bytes={}",
                limit_bytes, wanted_bytes
            ),
            MemoryLockOutcome::RunningUnlocked {
                limit_bytes,
                wanted_bytes,
            } => format!(
                "memory=unlocked-by-configuration memlock_limit_bytes={} \
                 memlock_wanted_bytes={}",
                limit_bytes, wanted_bytes
            ),
            MemoryLockOutcome::NotAttempted { limit_bytes } => format!(
                "memory=unlocked-not-attempted memlock_limit_bytes={}",
                limit_bytes
            ),
        }
    }
}

/// Why the server refused to start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractRefused {
    /// No real-time policy, and running without one was not enabled.
    RealTime {
        /// The underlying reason.
        cause: HostError,
    },
    /// Locking memory was denied, and running unlocked was not enabled.
    MemoryLock {
        /// The underlying reason.
        cause: HostError,
    },
    /// A limit could not be read or set at all.
    Limit {
        /// The underlying reason.
        cause: HostError,
    },
}

impl fmt::Display for ContractRefused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContractRefused::RealTime { cause } => write!(
                f,
                "{}; run with --allow-non-realtime to start anyway, and every status report will \
                 say the run has no real-time policy",
                cause
            ),
            ContractRefused::MemoryLock { cause } => write!(
                f,
                "{}; run with --allow-unlocked-memory to start anyway, and every status report \
                 will say the run has no locked memory",
                cause
            ),
            ContractRefused::Limit { cause } => write!(f, "{}", cause),
        }
    }
}

impl std::error::Error for ContractRefused {}

/// Take the real-time policy for the calling thread, or say why not.
///
/// The CPU-time bound is applied **before** this returns, so a thread that
/// takes a real-time policy through this function is bounded before it can do
/// any audio work. That ordering is the safety property; a bound applied after
/// the first buffer would leave exactly the window `sched(7)` warns about.
pub fn take_contract_for_this_thread(
    role: &str,
    wanted_priority: u32,
    rttime_us: u64,
    allow_non_realtime: bool,
    registry: &ThreadRegistry,
) -> Result<RealTimeOutcome, ContractRefused> {
    let ceiling = rtprio_ceiling().map_err(|cause| ContractRefused::Limit { cause })?;

    // The bound goes on first. If the policy is then granted, the thread is
    // already bounded; if it is refused, the bound cost nothing.
    let applied = bound_real_time_cpu_time(rttime_us)
        .map_err(|cause| ContractRefused::Limit { cause })?;

    match take_real_time_policy(wanted_priority) {
        Ok(grant) => {
            registry.register(RegisteredThread {
                role: role.to_string(),
                tid: thread_id(),
                wants_real_time: true,
                priority: Some(grant.priority),
                rttime_us: Some(applied.soft),
            });
            Ok(RealTimeOutcome::Granted {
                ceiling: grant.ceiling,
                priority: grant.priority,
                policy: grant.policy,
                rttime_us: applied.soft,
            })
        }
        Err(cause) => {
            if !allow_non_realtime {
                return Err(ContractRefused::RealTime { cause });
            }
            registry.register(RegisteredThread {
                role: role.to_string(),
                tid: thread_id(),
                wants_real_time: false,
                priority: None,
                rttime_us: None,
            });
            Ok(RealTimeOutcome::RunningWithout {
                ceiling: ceiling.soft,
                wanted: wanted_priority,
            })
        }
    }
}

/// Register a thread that is deliberately not real-time.
pub fn register_ordinary_thread(role: &str, registry: &ThreadRegistry) {
    registry.register(RegisteredThread {
        role: role.to_string(),
        tid: thread_id(),
        wants_real_time: false,
        priority: None,
        rttime_us: None,
    });
}

/// Decide the memory-locking question and report it either way.
pub fn decide_memory_lock(
    attempt: bool,
    wanted_bytes: u64,
    allow_unlocked: bool,
) -> Result<MemoryLockOutcome, ContractRefused> {
    let limit: Rlimit = memlock_limit().map_err(|cause| ContractRefused::Limit { cause })?;
    if !attempt {
        return Ok(MemoryLockOutcome::NotAttempted {
            limit_bytes: limit.soft,
        });
    }
    match lock_memory(wanted_bytes) {
        Ok(limit) => Ok(MemoryLockOutcome::Locked {
            limit_bytes: limit.soft,
            wanted_bytes,
        }),
        Err(cause) => {
            if !allow_unlocked {
                return Err(ContractRefused::MemoryLock { cause });
            }
            Ok(MemoryLockOutcome::RunningUnlocked {
                limit_bytes: limit.soft,
                wanted_bytes,
            })
        }
    }
}

/// The scheduling report: what the process says, and what the kernel says.
///
/// Returns the lines to print and whether the two agree. Disagreement is a
/// hard failure for the caller, not a warning: a real-time thread nobody
/// declared is the thing the whole contract exists to prevent.
pub fn scheduling_report(registry: &ThreadRegistry) -> (Vec<String>, bool) {
    let mut lines = Vec::new();
    let facts = match thread_facts() {
        Ok(f) => f,
        Err(e) => {
            lines.push(format!("scheduling-report error=/proc/self/task: {}", e));
            return (lines, false);
        }
    };

    for declared in registry.snapshot() {
        let kernel = facts.iter().find(|f| f.tid == declared.tid);
        lines.push(format!(
            "thread role={} tid={} declared_real_time={} declared_priority={} \
             declared_rttime_us={} kernel_policy={} kernel_priority={}",
            declared.role,
            declared.tid,
            u8::from(declared.wants_real_time),
            declared
                .priority
                .map(|p| p.to_string())
                .unwrap_or_else(|| "-".to_string()),
            declared
                .rttime_us
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".to_string()),
            kernel
                .map(|k| policy_name(k.policy).to_string())
                .unwrap_or_else(|| "gone".to_string()),
            kernel
                .map(|k| k.rt_priority.to_string())
                .unwrap_or_else(|| "-".to_string()),
        ));
    }

    for fact in &facts {
        if registry.snapshot().iter().all(|d| d.tid != fact.tid) {
            lines.push(format!(
                "thread role=unregistered tid={} comm={} kernel_policy={} kernel_priority={}",
                fact.tid,
                fact.comm,
                policy_name(fact.policy),
                fact.rt_priority
            ));
        }
    }

    let undeclared = undeclared_real_time_threads(registry, &facts);
    for u in &undeclared {
        lines.push(format!(
            "UNDECLARED-REAL-TIME-THREAD tid={} comm={} policy={} priority={}",
            u.facts.tid,
            u.facts.comm,
            policy_name(u.facts.policy),
            u.facts.rt_priority
        ));
    }
    lines.push(format!(
        "scheduling-report threads={} real_time_declared={} undeclared_real_time={}",
        facts.len(),
        registry
            .snapshot()
            .iter()
            .filter(|d| d.wants_real_time)
            .count(),
        undeclared.len()
    ));

    (lines, undeclared.is_empty())
}
