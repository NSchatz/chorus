//! The contract between the server and the host that runs its container.
//!
//! Four things, and they are safety properties rather than tuning:
//!
//! 1. **The ceiling is read, not assumed.** `sched(7)`: "the RLIMIT_RTPRIO
//!    resource limit defines a ceiling on an unprivileged thread's static
//!    priority for the SCHED_RR and SCHED_FIFO policies". The host hands that
//!    ceiling in per container (`docker run --ulimit rtprio=...`). The server
//!    reads it and asks for a priority no greater than it.
//! 2. **Every real-time thread is bounded before it does audio work.**
//!    `sched(7)` again: "A nonblocking infinite loop in a thread scheduled
//!    under the SCHED_FIFO, SCHED_RR, or SCHED_DEADLINE policy can potentially
//!    block all other threads from accessing the CPU forever." `RLIMIT_RTTIME`
//!    is the documented bound, and it is applied here before the thread is
//!    given anything to do.
//! 3. **Which threads are real-time is on the record.** A report nobody can
//!    check is not a safety property, so the report is compared against
//!    `/proc/self/task`, which is the kernel's own answer. That comparison is
//!    only worth making against a list that is COMPLETE, so the enumeration
//!    here is either a complete inventory or an error. The single exception is
//!    a thread that ceased to exist while it was being read, which is omitted
//!    and COUNTED, so a caller can tell "nothing was dropped" from "something
//!    was dropped and it had exited". Every other failure to read a listed
//!    thread refuses the whole enumeration rather than shortening the list:
//!    a short list makes the undeclared-real-time-thread check answer "clean"
//!    for the wrong reason.
//! 4. **A refusal is a refusal.** If the ceiling is zero, or locking memory is
//!    denied, the server says which limit it read and what it wanted and exits
//!    non-zero. It never quietly runs as a normal process while claiming the
//!    scheduling contract holds.
//!
//! # On the shape of RLIMIT_RTTIME
//!
//! `setrlimit(2)` sets a limit on the process. The kernel then enforces
//! `RLIMIT_RTTIME` against **each real-time task individually**: the counter
//! is a per-task one and it resets whenever that task blocks. So one call
//! bounds every real-time thread, and the report says so per thread rather
//! than pretending each got a limit of its own.
//!
//! # Unsafe
//!
//! This crate calls libc. Every call is wrapped so that no caller needs
//! `unsafe`, and every wrapper checks the return value and turns errno into a
//! typed error.

#![warn(missing_docs)]

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::io;
use std::os::raw::c_int;
use std::sync::Mutex;

/// `RLIMIT_MEMLOCK`.
const RLIMIT_MEMLOCK: c_int = 8;
/// `RLIMIT_RTPRIO`.
const RLIMIT_RTPRIO: c_int = 14;
/// `RLIMIT_RTTIME`.
const RLIMIT_RTTIME: c_int = 15;

/// `SCHED_OTHER`.
pub const SCHED_OTHER: c_int = 0;
/// `SCHED_FIFO`.
pub const SCHED_FIFO: c_int = 1;
/// `SCHED_RR`.
pub const SCHED_RR: c_int = 2;

/// `MCL_CURRENT | MCL_FUTURE`.
const MCL_CURRENT_AND_FUTURE: c_int = 1 | 2;

#[repr(C)]
#[derive(Clone, Copy)]
struct RawRlimit {
    rlim_cur: u64,
    rlim_max: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SchedParam {
    sched_priority: c_int,
}

extern "C" {
    fn getrlimit(resource: c_int, rlim: *mut RawRlimit) -> c_int;
    fn setrlimit(resource: c_int, rlim: *const RawRlimit) -> c_int;
    fn sched_setscheduler(pid: c_int, policy: c_int, param: *const SchedParam) -> c_int;
    fn sched_getscheduler(pid: c_int) -> c_int;
    fn sched_get_priority_min(policy: c_int) -> c_int;
    fn sched_get_priority_max(policy: c_int) -> c_int;
    fn mlockall(flags: c_int) -> c_int;
    fn gettid() -> c_int;
}

/// A resource limit as the kernel reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rlimit {
    /// Soft limit, which is the one that binds.
    pub soft: u64,
    /// Hard limit, the ceiling on the soft one.
    pub hard: u64,
}

impl Rlimit {
    /// `RLIM_INFINITY` as this platform spells it.
    pub const INFINITY: u64 = u64::MAX;

    /// The soft limit rendered for a human, with infinity spelled out.
    pub fn soft_display(&self) -> String {
        if self.soft == Rlimit::INFINITY {
            "unlimited".to_string()
        } else {
            self.soft.to_string()
        }
    }
}

/// Why the host contract could not be honoured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostError {
    /// A resource limit could not be read.
    LimitUnreadable {
        /// Which limit, spelled as `sched(7)` and `getrlimit(2)` spell it.
        limit: &'static str,
        /// The errno the kernel returned.
        errno: i32,
        /// That errno rendered.
        detail: String,
    },
    /// A resource limit could not be set.
    LimitNotSettable {
        /// Which limit.
        limit: &'static str,
        /// The value that was asked for.
        wanted: u64,
        /// The hard limit that was in the way, if that is what it was.
        hard: u64,
        /// The errno the kernel returned.
        errno: i32,
        /// That errno rendered.
        detail: String,
    },
    /// The container was granted no real-time priority at all.
    ///
    /// Not an error in the machinery: the host declined to grant the contract,
    /// and the caller decides what to do about it.
    CeilingIsZero {
        /// The ceiling that was read.
        ceiling: u64,
    },
    /// The kernel refused the scheduling policy.
    PolicyDenied {
        /// The ceiling that was read.
        ceiling: u64,
        /// The priority that was asked for.
        wanted: u32,
        /// The errno the kernel returned.
        errno: i32,
        /// That errno rendered.
        detail: String,
    },
    /// A priority outside what the policy accepts on this kernel.
    PriorityOutOfRange {
        /// The priority that was asked for.
        wanted: u32,
        /// Lowest priority this policy accepts.
        min: u32,
        /// Highest priority this policy accepts.
        max: u32,
    },
    /// Locking memory was denied.
    MemoryLockDenied {
        /// The locked-memory limit that was read, in bytes.
        limit_bytes: u64,
        /// The amount the server wanted locked, in bytes.
        wanted_bytes: u64,
        /// The errno `mlockall` returned, or 0 when the limit alone decided it
        /// and no call was made.
        errno: i32,
        /// Why, in words.
        detail: String,
    },
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostError::LimitUnreadable {
                limit,
                errno,
                detail,
            } => write!(f, "cannot read {}: {} (errno {})", limit, detail, errno),
            HostError::LimitNotSettable {
                limit,
                wanted,
                hard,
                errno,
                detail,
            } => write!(
                f,
                "cannot set {} to {}: hard limit is {}, {} (errno {})",
                limit, wanted, hard, detail, errno
            ),
            HostError::CeilingIsZero { ceiling } => write!(
                f,
                "the granted rtprio ceiling is {}, so no real-time priority is available; \
                 grant one with `docker run --ulimit rtprio=<n>`",
                ceiling
            ),
            HostError::PolicyDenied {
                ceiling,
                wanted,
                errno,
                detail,
            } => write!(
                f,
                "a real-time policy at priority {} was denied against a granted rtprio ceiling \
                 of {}: {} (errno {})",
                wanted, ceiling, detail, errno
            ),
            HostError::PriorityOutOfRange { wanted, min, max } => write!(
                f,
                "real-time priority {} is outside the {} to {} this kernel accepts",
                wanted, min, max
            ),
            HostError::MemoryLockDenied {
                limit_bytes,
                wanted_bytes,
                errno,
                detail,
            } => write!(
                f,
                "locking audio-path memory was denied: the locked-memory limit read is {} bytes \
                 and {} bytes were wanted ({}, errno {}); grant it with \
                 `docker run --ulimit memlock=<bytes>`",
                limit_bytes, wanted_bytes, detail, errno
            ),
        }
    }
}

impl std::error::Error for HostError {}

fn errno_now() -> (i32, String) {
    let e = io::Error::last_os_error();
    (e.raw_os_error().unwrap_or(0), e.to_string())
}

fn read_limit(resource: c_int, name: &'static str) -> Result<Rlimit, HostError> {
    let mut raw = RawRlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: raw is a live out-parameter of exactly the size the kernel
    // writes.
    let rc = unsafe { getrlimit(resource, &mut raw) };
    if rc != 0 {
        let (errno, detail) = errno_now();
        return Err(HostError::LimitUnreadable {
            limit: name,
            errno,
            detail,
        });
    }
    Ok(Rlimit {
        soft: raw.rlim_cur,
        hard: raw.rlim_max,
    })
}

/// The real-time priority ceiling this container was granted.
///
/// `RLIMIT_RTPRIO`. Zero means the host granted none, which is a decision by
/// the host and not a failure of this process.
pub fn rtprio_ceiling() -> Result<Rlimit, HostError> {
    read_limit(RLIMIT_RTPRIO, "RLIMIT_RTPRIO")
}

/// The locked-memory limit this container was granted, in bytes.
pub fn memlock_limit() -> Result<Rlimit, HostError> {
    read_limit(RLIMIT_MEMLOCK, "RLIMIT_MEMLOCK")
}

/// The CPU-time bound currently in force for real-time threads, in
/// microseconds.
pub fn rttime_limit() -> Result<Rlimit, HostError> {
    read_limit(RLIMIT_RTTIME, "RLIMIT_RTTIME")
}

/// Bound every real-time thread of this process to `micros` of CPU time
/// without blocking.
///
/// Soft and hard are set to the same value on purpose. `getrlimit(2)` says a
/// process that reaches the soft limit is sent `SIGXCPU`, whose default action
/// terminates it, and that it keeps receiving `SIGXCPU` once a second until
/// the hard limit brings `SIGKILL`. Setting them equal means the bound fires
/// once, promptly, and a handler that swallowed `SIGXCPU` could not turn the
/// bound into a suggestion.
pub fn bound_real_time_cpu_time(micros: u64) -> Result<Rlimit, HostError> {
    let current = rttime_limit()?;
    let raw = RawRlimit {
        rlim_cur: micros,
        rlim_max: micros,
    };
    // SAFETY: raw is a live, fully initialised rlimit.
    let rc = unsafe { setrlimit(RLIMIT_RTTIME, &raw) };
    if rc != 0 {
        let (errno, detail) = errno_now();
        return Err(HostError::LimitNotSettable {
            limit: "RLIMIT_RTTIME",
            wanted: micros,
            hard: current.hard,
            errno,
            detail,
        });
    }
    rttime_limit()
}

/// What a real-time request obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RealTimeGrant {
    /// The ceiling that was read before asking.
    pub ceiling: u64,
    /// The priority actually obtained.
    pub priority: u32,
    /// The policy actually obtained.
    pub policy: c_int,
}

/// Put the calling thread under `SCHED_FIFO` at a priority no greater than the
/// granted ceiling.
///
/// `wanted` is the priority the configuration asks for. The request is clamped
/// to the ceiling rather than refused for being ambitious: asking for more
/// than the host granted is a configuration that outlived its host, and the
/// safe reading is "as much as I am allowed", never "more than I am allowed".
pub fn take_real_time_policy(wanted: u32) -> Result<RealTimeGrant, HostError> {
    let ceiling = rtprio_ceiling()?;
    if ceiling.soft == 0 {
        return Err(HostError::CeilingIsZero {
            ceiling: ceiling.soft,
        });
    }
    // SAFETY: both calls take a policy constant and return a plain int.
    let (min, max) = unsafe {
        (
            sched_get_priority_min(SCHED_FIFO),
            sched_get_priority_max(SCHED_FIFO),
        )
    };
    if min < 0 || max < 0 {
        let (errno, detail) = errno_now();
        return Err(HostError::PolicyDenied {
            ceiling: ceiling.soft,
            wanted,
            errno,
            detail,
        });
    }
    let ceiling_priority = ceiling.soft.min(max as u64) as u32;
    if ceiling_priority < min as u32 {
        return Err(HostError::PriorityOutOfRange {
            wanted,
            min: min as u32,
            max: ceiling_priority,
        });
    }
    let priority = wanted.clamp(min as u32, ceiling_priority);

    let param = SchedParam {
        sched_priority: priority as c_int,
    };
    // SAFETY: pid 0 is the calling thread on Linux, and param is live.
    let rc = unsafe { sched_setscheduler(0, SCHED_FIFO, &param) };
    if rc != 0 {
        let (errno, detail) = errno_now();
        return Err(HostError::PolicyDenied {
            ceiling: ceiling.soft,
            wanted,
            errno,
            detail,
        });
    }
    // SAFETY: pid 0 is the calling thread.
    let policy = unsafe { sched_getscheduler(0) };
    Ok(RealTimeGrant {
        ceiling: ceiling.soft,
        priority,
        policy,
    })
}

/// Lock this process's memory, current and future.
///
/// Returns the limit that was read on success, and a typed denial when either
/// the limit is below what was wanted or `mlockall` refuses. The limit is
/// checked first so that a host which granted too little is told exactly what
/// it granted and what was wanted, rather than being handed `ENOMEM`.
pub fn lock_memory(wanted_bytes: u64) -> Result<Rlimit, HostError> {
    let limit = memlock_limit()?;
    if limit.soft != Rlimit::INFINITY && limit.soft < wanted_bytes {
        return Err(HostError::MemoryLockDenied {
            limit_bytes: limit.soft,
            wanted_bytes,
            errno: 0,
            detail: "the granted locked-memory limit is below what was wanted".to_string(),
        });
    }
    // SAFETY: a plain flags argument.
    let rc = unsafe { mlockall(MCL_CURRENT_AND_FUTURE) };
    if rc != 0 {
        let (errno, detail) = errno_now();
        return Err(HostError::MemoryLockDenied {
            limit_bytes: limit.soft,
            wanted_bytes,
            errno,
            detail,
        });
    }
    Ok(limit)
}

/// The calling thread's kernel thread id.
pub fn thread_id() -> i32 {
    // SAFETY: gettid takes nothing and returns a pid_t.
    unsafe { gettid() }
}

/// This thread's scheduling policy, straight from the kernel.
pub fn current_policy() -> c_int {
    // SAFETY: pid 0 is the calling thread.
    unsafe { sched_getscheduler(0) }
}

/// A scheduling policy rendered the way `sched(7)` names it.
pub fn policy_name(policy: c_int) -> &'static str {
    match policy {
        0 => "SCHED_OTHER",
        1 => "SCHED_FIFO",
        2 => "SCHED_RR",
        3 => "SCHED_BATCH",
        5 => "SCHED_IDLE",
        6 => "SCHED_DEADLINE",
        _ => "unknown",
    }
}

/// The name of one thread, or the fact that it could not be read.
///
/// A name is never defaulted. The empty string is a perfectly good `String`,
/// and a thread reported under one is a thread reported under a name it does
/// not have. That is the quiet wrong answer this crate exists to refuse, so
/// the two cases are different values rather than the same one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreadName {
    /// The name the kernel gave in `/proc/self/task/<tid>/comm`.
    Known(String),
    /// The name could not be read, or was empty. The thread is reported
    /// anyway: the question this inventory answers is about policies, and a
    /// nameless thread is still a thread.
    Unavailable,
}

impl ThreadName {
    /// The name, when there is one.
    pub fn known(&self) -> Option<&str> {
        match self {
            ThreadName::Known(name) => Some(name),
            ThreadName::Unavailable => None,
        }
    }
}

impl fmt::Display for ThreadName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ThreadName::Known(name) => write!(f, "{}", name),
            ThreadName::Unavailable => write!(f, "<name-unavailable>"),
        }
    }
}

/// What the kernel says about one thread of this process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadFacts {
    /// Kernel thread id.
    pub tid: i32,
    /// The name in `/proc/self/task/<tid>/comm`, or the fact that it could not
    /// be read.
    pub comm: ThreadName,
    /// Scheduling policy, from field 41 of `/proc/self/task/<tid>/stat`.
    pub policy: c_int,
    /// Real-time priority, from field 40 of the same file. Zero for a thread
    /// that is not real-time.
    pub rt_priority: u32,
}

impl ThreadFacts {
    /// Whether this thread runs under a real-time policy.
    pub fn is_real_time(&self) -> bool {
        self.policy == SCHED_FIFO || self.policy == SCHED_RR
    }
}

/// `ESRCH`, which is what the kernel answers when it is answering about the
/// thread rather than about the file.
const ESRCH: i32 = 3;

/// Why a thread inventory could not be completed.
///
/// Every variant is a refusal rather than a shorter list. The one omission the
/// enumeration makes is a thread that ceased to exist, and that one is counted
/// rather than hidden: see [`ThreadInventory::vanished`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InventoryError {
    /// The thread listing itself could not be read.
    ListingUnreadable {
        /// What was being listed.
        source: String,
        /// The errno the kernel returned.
        errno: i32,
        /// That errno rendered.
        detail: String,
    },
    /// The listing offered an entry that names no thread at all.
    ///
    /// Under the uninterpretable-record rule: an entry nobody can read is not
    /// evidence that a thread is absent.
    EntryNamesNoThread {
        /// The entry, as the listing spelled it.
        entry: String,
    },
    /// A listed thread could not be read, for a reason other than that thread
    /// having ceased to exist.
    ThreadUnreadable {
        /// The thread the listing named.
        tid: i32,
        /// What was being read from it.
        what: &'static str,
        /// The errno the kernel returned.
        errno: i32,
        /// That errno rendered.
        detail: String,
    },
    /// A listed thread's scheduling record was readable and could not be
    /// interpreted as a policy and a real-time priority.
    RecordUninterpretable {
        /// The thread whose record it was.
        tid: i32,
        /// What was wrong with it.
        detail: String,
    },
    /// The enumeration finished with no thread in it.
    ///
    /// The process doing the asking is itself a thread, so an empty answer is
    /// evidence that the answer is wrong.
    Empty {
        /// How many entries the listing offered.
        listed: usize,
        /// How many of those had ceased to exist.
        vanished: usize,
    },
}

impl InventoryError {
    /// Whether this is the uninterpretable-record rule firing.
    ///
    /// A record that was there and could not be read as one, and a listing
    /// entry that names no thread, are the same rule: refuse the enumeration
    /// naming what could not be read, rather than treat it as a thread that is
    /// not there.
    pub fn is_uninterpretable_record(&self) -> bool {
        matches!(
            self,
            InventoryError::RecordUninterpretable { .. } | InventoryError::EntryNamesNoThread { .. }
        )
    }
}

impl fmt::Display for InventoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InventoryError::ListingUnreadable {
                source,
                errno,
                detail,
            } => write!(f, "cannot list {}: {} (errno {})", source, detail, errno),
            InventoryError::EntryNamesNoThread { entry } => write!(
                f,
                "the thread listing offered '{}', which names no thread",
                entry
            ),
            InventoryError::ThreadUnreadable {
                tid,
                what,
                errno,
                detail,
            } => write!(
                f,
                "cannot read {} of thread {}: {} (errno {})",
                what, tid, detail, errno
            ),
            InventoryError::RecordUninterpretable { tid, detail } => write!(
                f,
                "the scheduling record of thread {} cannot be interpreted: {}",
                tid, detail
            ),
            InventoryError::Empty { listed, vanished } => write!(
                f,
                "the thread inventory came back empty from {} listed entries ({} had exited); \
                 the thread doing the asking is itself a thread, so this answer is wrong",
                listed, vanished
            ),
        }
    }
}

impl std::error::Error for InventoryError {}

impl From<InventoryError> for io::Error {
    fn from(e: InventoryError) -> io::Error {
        io::Error::new(io::ErrorKind::Other, e.to_string())
    }
}

/// A thread inventory that COMPLETED.
///
/// There is no way to build one out of a partial list. The only constructors
/// are [`thread_inventory`] and [`thread_inventory_from`], and both return an
/// error rather than a short answer, so holding one of these is the evidence
/// that the enumeration finished. [`undeclared_real_time_threads_in`] takes
/// one for exactly that reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadInventory {
    threads: Vec<ThreadFacts>,
    vanished: usize,
}

impl ThreadInventory {
    /// One entry for every thread that existed and was readable throughout,
    /// ordered by thread id.
    pub fn threads(&self) -> &[ThreadFacts] {
        &self.threads
    }

    /// How many listed threads had ceased to exist before they could be read.
    ///
    /// Zero means nothing was dropped. Anything else means something was
    /// dropped and it had exited, which is the only reason this enumeration
    /// ever drops anything.
    pub fn vanished(&self) -> usize {
        self.vanished
    }

    /// The entries, for a caller that only needs the list.
    pub fn into_threads(self) -> Vec<ThreadFacts> {
        self.threads
    }
}

/// Where a thread inventory is read from.
///
/// `/proc/self/task` is the only implementation that ships
/// ([`ProcTaskSource`]). The trait exists so that every failure class the
/// enumeration distinguishes can be provoked in a test with no privilege and
/// no real fault: an errno injected here is the errno the kernel would have
/// returned, and the enumeration cannot tell the difference.
pub trait TaskSource {
    /// What is being listed, for the error message when it cannot be.
    fn describe(&self) -> String;

    /// The entries the thread listing offers, in whatever order it offers
    /// them. Entry names are raw, so an entry that names no thread reaches the
    /// enumeration instead of being silently skipped.
    ///
    /// One call is NOT required to be complete: see [`LISTING_PASSES`].
    fn list(&self) -> io::Result<Vec<String>>;

    /// The thread doing the asking, when the source is this process.
    ///
    /// The one thread whose existence needs no listing to establish. It is
    /// added to the listing so that no amount of bad luck in the kernel's
    /// directory iteration can cost the caller its own entry. A source that is
    /// not this process has no such thread and says so.
    fn self_tid(&self) -> Option<i32> {
        None
    }

    /// One listed thread's scheduling record.
    fn read_stat(&self, tid: i32) -> io::Result<String>;

    /// One listed thread's name.
    fn read_comm(&self, tid: i32) -> io::Result<String>;
}

/// The kernel's own answer: `/proc/self/task`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcTaskSource;

impl TaskSource for ProcTaskSource {
    fn describe(&self) -> String {
        "/proc/self/task".to_string()
    }

    fn self_tid(&self) -> Option<i32> {
        Some(thread_id())
    }

    fn list(&self) -> io::Result<Vec<String>> {
        let mut names = Vec::new();
        for entry in fs::read_dir("/proc/self/task")? {
            names.push(entry?.file_name().to_string_lossy().into_owned());
        }
        Ok(names)
    }

    fn read_stat(&self, tid: i32) -> io::Result<String> {
        fs::read_to_string(format!("/proc/self/task/{}/stat", tid))
    }

    fn read_comm(&self, tid: i32) -> io::Result<String> {
        fs::read_to_string(format!("/proc/self/task/{}/comm", tid))
    }
}

/// How many times the thread listing is taken before the enumeration uses it.
///
/// `/proc/<pid>/task` is generated as it is read and its iteration is not
/// atomic: a thread created or destroyed during the scan moves the kernel's
/// cursor and can cost a DIFFERENT, live thread its entry. That is not a
/// hypothesis. Measured on this repository's own suite, a single listing lost
/// a live thread on about 3 runs in 100, and the thread it lost was the one
/// doing the asking, whose `stat` was readable the whole time and which a
/// listing taken microseconds later contained. The numbers and the method are
/// in `docs/measurements/hostctl-thread-inventory-repeat.md`.
///
/// So the listing is taken again until two consecutive passes agree, and the
/// entries are UNIONED. Union and never intersection: a pass that missed a
/// thread must not be able to cost that thread its place, and an entry that is
/// in the union and has since exited is exactly the vanished case, which is
/// counted rather than hidden.
pub const LISTING_PASSES: usize = 5;

/// Whether this error means the thread stopped existing, which is the one
/// omission the enumeration makes.
///
/// `ENOENT` is what reading a dead thread's files gives, and `ESRCH` is the
/// same fact spelled about the task. Nothing else means "gone": a refusal, a
/// transient failure and a record nobody can read all mean the answer is not
/// known, and an answer that is not known is not an absence.
fn thread_is_gone(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::NotFound || e.raw_os_error() == Some(ESRCH)
}

fn errno_of(e: &io::Error) -> (i32, String) {
    (e.raw_os_error().unwrap_or(0), e.to_string())
}

fn describe_record(stat: &str) -> String {
    if stat.trim().is_empty() {
        return "the record is empty".to_string();
    }
    format!(
        "field 40 (rt_priority) and field 41 (policy) could not be read out of the {} bytes there",
        stat.len()
    )
}

/// Every thread of this process, as the kernel sees it, or why not.
///
/// Read from `/proc`, deliberately, rather than from a list the process keeps:
/// the point of the check this feeds is to catch a thread the process did not
/// know it had.
pub fn thread_inventory() -> Result<ThreadInventory, InventoryError> {
    thread_inventory_from(&ProcTaskSource)
}

/// Every entry any of [`LISTING_PASSES`] listings offered, plus the thread
/// doing the asking.
///
/// Stops as soon as two consecutive passes agree, which is the common case and
/// costs two readdirs of a directory with a handful of entries in it.
fn stable_listing(source: &dyn TaskSource) -> Result<Vec<String>, InventoryError> {
    let once = |source: &dyn TaskSource| -> Result<BTreeSet<String>, InventoryError> {
        source
            .list()
            .map(|names| names.into_iter().collect())
            .map_err(|e| {
                let (errno, detail) = errno_of(&e);
                InventoryError::ListingUnreadable {
                    source: source.describe(),
                    errno,
                    detail,
                }
            })
    };

    let mut previous = once(source)?;
    let mut union = previous.clone();
    for _ in 1..LISTING_PASSES {
        let pass = once(source)?;
        let agreed = pass == previous;
        union.extend(pass.iter().cloned());
        if agreed {
            break;
        }
        previous = pass;
    }
    if let Some(tid) = source.self_tid() {
        union.insert(tid.to_string());
    }
    Ok(union.into_iter().collect())
}

/// [`thread_inventory`], against any [`TaskSource`].
pub fn thread_inventory_from(source: &dyn TaskSource) -> Result<ThreadInventory, InventoryError> {
    let listed = stable_listing(source)?;

    let mut threads = Vec::new();
    let mut vanished = 0usize;
    for entry in &listed {
        let tid: i32 = entry
            .parse()
            .map_err(|_| InventoryError::EntryNamesNoThread {
                entry: entry.clone(),
            })?;

        let stat = match source.read_stat(tid) {
            Ok(stat) => stat,
            // A thread that exited between the listing and the read is not an
            // error; it is simply not a thread any more. It is counted, so
            // that a caller can see it happened.
            Err(e) if thread_is_gone(&e) => {
                vanished += 1;
                continue;
            }
            Err(e) => {
                let (errno, detail) = errno_of(&e);
                return Err(InventoryError::ThreadUnreadable {
                    tid,
                    what: "the scheduling record",
                    errno,
                    detail,
                });
            }
        };

        let (policy, rt_priority) = parse_stat_scheduling(&stat).ok_or_else(|| {
            InventoryError::RecordUninterpretable {
                tid,
                detail: describe_record(&stat),
            }
        })?;

        // A name that cannot be read costs the thread its name, never its
        // place in the list: the thread is there, and what it is called is not
        // what the check above it asks about.
        let comm = match source.read_comm(tid) {
            Ok(name) if !name.trim().is_empty() => ThreadName::Known(name.trim().to_string()),
            _ => ThreadName::Unavailable,
        };

        threads.push(ThreadFacts {
            tid,
            comm,
            policy,
            rt_priority,
        });
    }

    if threads.is_empty() {
        return Err(InventoryError::Empty {
            listed: listed.len(),
            vanished,
        });
    }
    threads.sort_by_key(|t| t.tid);
    Ok(ThreadInventory { threads, vanished })
}

/// Every thread of this process, as the kernel sees it.
///
/// The list out of a COMPLETED inventory, for a caller that only needs the
/// list. An inventory that could not be completed is an error here too, never
/// a short list; [`thread_inventory`] is the same answer with the count of
/// threads that had exited kept.
pub fn thread_facts() -> io::Result<Vec<ThreadFacts>> {
    Ok(thread_inventory()?.into_threads())
}

/// Pull `rt_priority` (field 40) and `policy` (field 41) out of a
/// `/proc/<pid>/stat` line.
///
/// The command name is field 2, is wrapped in parentheses and may itself
/// contain spaces and parentheses, so the split starts after the LAST `)`.
/// Getting that wrong is the classic way to misread this file.
pub fn parse_stat_scheduling(stat: &str) -> Option<(c_int, u32)> {
    let close = stat.rfind(')')?;
    let rest = stat[close + 1..].trim_start();
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // `rest` starts at field 3, so field n is at index n - 3.
    let rt_priority: u32 = fields.get(40 - 3)?.parse().ok()?;
    let policy: c_int = fields.get(41 - 3)?.parse().ok()?;
    Some((policy, rt_priority))
}

/// A thread this process created on purpose, and what it asked the host for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredThread {
    /// What this thread is for, in the server's own words.
    pub role: String,
    /// Kernel thread id.
    pub tid: i32,
    /// Whether the server intended this thread to be real-time.
    pub wants_real_time: bool,
    /// The priority obtained, if it is real-time.
    pub priority: Option<u32>,
    /// The CPU-time bound in force for it, in microseconds, if it is
    /// real-time.
    pub rttime_us: Option<u64>,
}

/// The threads this process created on purpose.
///
/// Process-wide, because the question it exists for is "is there a real-time
/// thread nobody declared", and that has no answer inside a single thread.
#[derive(Debug, Default)]
pub struct ThreadRegistry {
    threads: Mutex<Vec<RegisteredThread>>,
}

impl ThreadRegistry {
    /// An empty registry.
    pub fn new() -> ThreadRegistry {
        ThreadRegistry::default()
    }

    /// Record a thread, from inside that thread.
    pub fn register(&self, thread: RegisteredThread) {
        let mut guard = match self.threads.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.push(thread);
    }

    /// Everything registered so far, in registration order.
    pub fn snapshot(&self) -> Vec<RegisteredThread> {
        match self.threads.lock() {
            Ok(g) => g.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }
}

/// A thread the kernel says is real-time and the process never declared.
///
/// This is the finding the scheduling report exists to make impossible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndeclaredRealTimeThread {
    /// The thread the kernel reported.
    pub facts: ThreadFacts,
}

/// Compare what the process declared against what the kernel says.
///
/// Returns every thread that is real-time in the inventory and absent from the
/// registry. An empty result is the property the scheduling report asserts: no
/// thread runs under a real-time policy that was not reported.
///
/// It takes a [`ThreadInventory`] rather than a slice on purpose. An empty
/// result from a list that lost a thread on the way is the wrong answer to the
/// only question this function exists to ask, and a slice would let a caller
/// hand it one without either of them noticing.
pub fn undeclared_real_time_threads_in(
    registry: &ThreadRegistry,
    inventory: &ThreadInventory,
) -> Vec<UndeclaredRealTimeThread> {
    let declared: Vec<i32> = registry
        .snapshot()
        .iter()
        .filter(|t| t.wants_real_time)
        .map(|t| t.tid)
        .collect();
    inventory
        .threads()
        .iter()
        .filter(|f| f.is_real_time() && !declared.contains(&f.tid))
        .map(|f| UndeclaredRealTimeThread { facts: f.clone() })
        .collect()
}

/// Answer "is any thread real-time without having been declared" about this
/// process, or say why it cannot be answered.
///
/// An inventory that did not complete is reported to the caller as the error
/// it is. It is never reported as zero undeclared real-time threads, which is
/// what a shortened list would have made it look like.
pub fn undeclared_real_time_threads(
    registry: &ThreadRegistry,
) -> Result<Vec<UndeclaredRealTimeThread>, InventoryError> {
    undeclared_real_time_threads_from(registry, &ProcTaskSource)
}

/// [`undeclared_real_time_threads`], against any [`TaskSource`].
pub fn undeclared_real_time_threads_from(
    registry: &ThreadRegistry,
    source: &dyn TaskSource,
) -> Result<Vec<UndeclaredRealTimeThread>, InventoryError> {
    let inventory = thread_inventory_from(source)?;
    Ok(undeclared_real_time_threads_in(registry, &inventory))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ceiling_and_the_limits_are_readable_on_any_linux() {
        assert!(rtprio_ceiling().is_ok());
        assert!(memlock_limit().is_ok());
        assert!(rttime_limit().is_ok());
    }

    #[test]
    fn a_stat_line_with_spaces_and_parentheses_in_the_command_still_parses() {
        // Fields 3 onward, with 40 = rt_priority and 41 = policy. The command
        // name is deliberately hostile.
        let mut fields: Vec<String> = Vec::new();
        for n in 4..=52 {
            fields.push(match n {
                40 => "42".to_string(),
                41 => "1".to_string(),
                _ => "0".to_string(),
            });
        }
        let stat = format!("1234 (evil ) name (x)) S {}", fields.join(" "));
        assert_eq!(parse_stat_scheduling(&stat), Some((1, 42)));
    }

    #[test]
    fn this_process_has_no_real_time_thread_it_did_not_declare() {
        let registry = ThreadRegistry::new();
        let facts = thread_facts().expect("/proc/self/task is readable");
        assert!(!facts.is_empty(), "a process has at least one thread");
        // The test harness declares nothing and takes no real-time policy, so
        // the two answers agree at zero. The answer comes out of a completed
        // inventory or not at all.
        let undeclared = undeclared_real_time_threads(&registry)
            .expect("the inventory of this process completes");
        assert!(undeclared.is_empty());
    }

    #[test]
    fn the_reported_policy_of_this_thread_matches_proc() {
        let tid = thread_id();
        let facts = thread_facts().unwrap();
        let me = facts
            .iter()
            .find(|f| f.tid == tid)
            .expect("this thread is in /proc/self/task");
        assert_eq!(me.policy, current_policy());
    }

    #[test]
    fn a_ceiling_of_zero_is_reported_as_the_host_declining_rather_than_a_failure() {
        // The container this suite usually runs in is granted no rtprio at
        // all, which is the interesting case; where a ceiling exists the
        // request succeeds and there is nothing to assert here.
        let ceiling = rtprio_ceiling().unwrap();
        if ceiling.soft == 0 {
            match take_real_time_policy(10) {
                Err(HostError::CeilingIsZero { ceiling: c }) => assert_eq!(c, 0),
                other => panic!("expected CeilingIsZero, got {:?}", other),
            }
        }
    }

    #[test]
    fn locking_more_than_the_granted_limit_is_denied_with_both_numbers() {
        let limit = memlock_limit().unwrap();
        if limit.soft == Rlimit::INFINITY {
            return;
        }
        let wanted = limit.soft.saturating_add(1 << 30);
        match lock_memory(wanted) {
            Err(HostError::MemoryLockDenied {
                limit_bytes,
                wanted_bytes,
                ..
            }) => {
                assert_eq!(limit_bytes, limit.soft);
                assert_eq!(wanted_bytes, wanted);
            }
            other => panic!("expected MemoryLockDenied, got {:?}", other),
        }
    }
}
