//! The three syscalls the wakeup probe needs, and nothing else.
//!
//! - `clock_gettime(CLOCK_MONOTONIC)`: the probe's only clock. `clock_nanosleep(2)`
//!   describes `CLOCK_MONOTONIC` as "a nonsettable, monotonically increasing clock"
//!   (man-pages 6.19, https://man7.org/linux/man-pages/man2/clock_nanosleep.2.html,
//!   read 2026-09-30).
//! - `clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME)`: an absolute deadline, so a
//!   late wakeup does not push every later deadline back with it.
//! - `prctl(PR_SET_TIMERSLACK)` / `prctl(PR_GET_TIMERSLACK)`: the timer slack the
//!   kernel may add to this thread's sleeps. `PR_SET_TIMERSLACK(2const)` names no
//!   capability for it and says "Timer slack is not applied to threads that are
//!   scheduled under a real-time scheduling policy" (man-pages 6.19,
//!   https://man7.org/linux/man-pages/man2/PR_SET_TIMERSLACK.2const.html, read
//!   2026-09-30).
//!
//! The constant values are the ones the MIT/Apache `libc` crate 0.2.189 carries for
//! Linux (`src/unix/linux_like/mod.rs` and `linux_l4re_shared.rs`).
//!
//! No settable clock is read here: this unit is on the audio path's list
//! (`audio-path.conf`) so that the scanner holds it to that.

// Unsafe allowed in this module: it is the libc FFI boundary of the wakeup probe
// (docs/decisions/0047-host-probes.md); every block names its invariant.
#![allow(unsafe_code)]

use std::io;
use std::os::raw::{c_int, c_ulong};

const CLOCK_MONOTONIC: c_int = 1;
const TIMER_ABSTIME: c_int = 1;
const PR_SET_TIMERSLACK: c_int = 29;
const PR_GET_TIMERSLACK: c_int = 30;
const EINTR: c_int = 4;

#[repr(C)]
struct Timespec {
    tv_sec: i64,
    tv_nsec: i64,
}

extern "C" {
    fn clock_gettime(clock: c_int, ts: *mut Timespec) -> c_int;
    fn clock_nanosleep(
        clock: c_int,
        flags: c_int,
        request: *const Timespec,
        remain: *mut Timespec,
    ) -> c_int;
    fn prctl(option: c_int, arg2: c_ulong, arg3: c_ulong, arg4: c_ulong, arg5: c_ulong) -> c_int;
}

/// Nanoseconds on `CLOCK_MONOTONIC`.
pub fn monotonic_ns() -> u64 {
    let mut ts = Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: &mut ts points at a live repr(C) timespec (two 64-bit fields on the
    // 64-bit Linux targets chorus builds for) that the kernel only writes during the
    // call; CLOCK_MONOTONIC always exists, so the call cannot fail.
    unsafe { clock_gettime(CLOCK_MONOTONIC, &mut ts) };
    (ts.tv_sec as u64) * 1_000_000_000 + ts.tv_nsec as u64
}

/// Sleep until `deadline_ns` on `CLOCK_MONOTONIC`, restarting on a signal.
pub fn sleep_until_monotonic_ns(deadline_ns: u64) -> io::Result<()> {
    let request = Timespec {
        tv_sec: (deadline_ns / 1_000_000_000) as i64,
        tv_nsec: (deadline_ns % 1_000_000_000) as i64,
    };
    loop {
        // SAFETY: &request points at a live repr(C) timespec the kernel only reads;
        // remain may be null for an absolute sleep (clock_nanosleep(2)).
        let rc = unsafe {
            clock_nanosleep(
                CLOCK_MONOTONIC,
                TIMER_ABSTIME,
                &request,
                std::ptr::null_mut(),
            )
        };
        match rc {
            0 => return Ok(()),
            // An absolute sleep is restarted with the same deadline (clock_nanosleep(2)).
            EINTR => continue,
            // clock_nanosleep returns the error number rather than setting errno.
            e => return Err(io::Error::from_raw_os_error(e)),
        }
    }
}

/// Set this thread's timer slack in nanoseconds (`slack_ns` of 0 resets it to the default).
pub fn set_timer_slack_ns(slack_ns: u64) -> io::Result<()> {
    // SAFETY: no pointers cross the boundary; PR_SET_TIMERSLACK reads arg2 as an
    // unsigned long and ignores the rest.
    let rc = unsafe { prctl(PR_SET_TIMERSLACK, slack_ns as c_ulong, 0, 0, 0) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// This thread's current timer slack in nanoseconds.
pub fn timer_slack_ns() -> io::Result<u64> {
    // SAFETY: no pointers cross the boundary; PR_GET_TIMERSLACK returns the slack
    // as the call's result.
    let rc = unsafe { prctl(PR_GET_TIMERSLACK, 0, 0, 0, 0) };
    if rc < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(rc as u64)
    }
}
