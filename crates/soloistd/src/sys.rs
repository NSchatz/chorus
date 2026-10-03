//! The three things a supervisor needs from the operating system that std
//! has no safe call for: making a FIFO, sending SIGTERM, and noticing its
//! own SIGTERM.
//!
//! Each could have been a child program (`mkfifo`, `kill`) except the last:
//! a supervisor that is PID 1 of its container is not stopped by SIGTERM at
//! all unless it installs a handler (the kernel delivers PID 1 only the
//! signals it handles), and one that is not PID 1 dies at once and leaves
//! Soloist to be killed with its data directory mid-write. Handling the
//! signal is what lets `docker stop` end Soloist the way Soloist documents
//! ("normal daemon shutdown ... removes `ws.addr` and `ws.port`"). Once one
//! libc call is needed, the other two are the same size and remove a
//! dependency on which programs the image carries
//! (the decision record of goal 17's track S1).
//!
//! The declarations are libc's, as the POSIX.1-2017 pages give them
//! (`mkfifo(3p)`, `kill(3p)`, `signal(3p)`); the constants are Linux's for
//! x86_64 and aarch64 (`signal(7)`: SIGINT 2, SIGTERM 15).

// The one place in this crate that talks to libc directly (docs/conventions.md rule 2).
#![allow(unsafe_code)]

use std::ffi::{c_char, c_int, CString};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

const SIGINT: c_int = 2;
const SIGTERM: c_int = 15;
/// `SIG_ERR`, what `signal` returns on failure: `(void (*)(int)) -1`.
const SIG_ERR: usize = usize::MAX;

extern "C" {
    fn mkfifo(path: *const c_char, mode: u32) -> c_int;
    fn kill(pid: c_int, sig: c_int) -> c_int;
    fn signal(signum: c_int, handler: extern "C" fn(c_int)) -> usize;
}

static TERMINATION: AtomicBool = AtomicBool::new(false);

/// The handler: one atomic store, which is async-signal-safe.
extern "C" fn on_termination(_signal: c_int) {
    TERMINATION.store(true, Ordering::SeqCst);
}

/// Make a FIFO at `path` with `mode` (before the umask).
pub fn make_fifo(path: &Path, mode: u32) -> io::Result<()> {
    let path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "a NUL in the path"))?;
    // SAFETY: `path` is a valid NUL-terminated string that outlives the
    // call, and mkfifo keeps no pointer to it.
    let rc = unsafe { mkfifo(path.as_ptr(), mode) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Send SIGTERM to a process. The caller must know the process has not been
/// reaped (it holds the `Child` and has not waited on it), so the id cannot
/// have been reused.
pub fn terminate(pid: u32) -> io::Result<()> {
    let pid = c_int::try_from(pid)
        .ok()
        .filter(|p| *p > 0)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "not a process id"))?;
    // SAFETY: kill takes two integers and touches no memory of this
    // process; `pid` is positive, so it names one process, never a group.
    let rc = unsafe { kill(pid, SIGTERM) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// From now on SIGTERM and SIGINT set a flag instead of ending the process;
/// [`termination_requested`] reads it.
pub fn catch_termination() -> io::Result<()> {
    for sig in [SIGTERM, SIGINT] {
        // SAFETY: the handler is an `extern "C" fn(c_int)` that lives for
        // the whole program and only stores to an atomic, which is
        // async-signal-safe; `signal` keeps the function pointer only.
        let previous = unsafe { signal(sig, on_termination) };
        if previous == SIG_ERR {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Whether SIGTERM or SIGINT has arrived since [`catch_termination`].
pub fn termination_requested() -> bool {
    TERMINATION.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::FileTypeExt;

    #[test]
    fn a_fifo_is_made_once() {
        let dir = std::env::temp_dir().join(format!("chorus-soloistd-sys-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.pcm");
        make_fifo(&path, 0o660).unwrap();
        assert!(std::fs::metadata(&path).unwrap().file_type().is_fifo());
        let again = make_fifo(&path, 0o660).unwrap_err();
        assert_eq!(again.kind(), io::ErrorKind::AlreadyExists);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn sigterm_ends_a_child_that_does_not_catch_it() {
        use std::os::unix::process::ExitStatusExt;
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        terminate(child.id()).unwrap();
        let status = child.wait().unwrap();
        assert_eq!(status.signal(), Some(SIGTERM));
        assert!(terminate(0).is_err());
    }
}
