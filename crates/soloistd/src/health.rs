//! `chorus-soloistd --health-check`: the probe a container healthcheck runs.
//!
//! It starts nothing and connects to nothing. It must not connect to the
//! receiver's socket: that socket takes one connection at a time and a new
//! one replaces the old, so a probe that connected would drop chorus-server
//! every time it ran. What it looks at instead:
//!
//! 1. the index file the supervisor of this container wrote in its runtime
//!    directory after claiming a receiver ([`INDEX_FILE`]);
//! 2. `r<i>.lock` is held: an exclusive `flock` from here would block, so a
//!    supervisor is alive (a lock this probe can take belongs to nobody);
//! 3. `r<i>.sock` is a socket and `r<i>.pcm` is a FIFO;
//! 4. with `--pipewire auto`, PipeWire's socket exists in the runtime
//!    directory.
//!
//! Exit 0 healthy, 1 unhealthy (Docker's healthcheck contract), one line
//! either way.

use std::fs::{self, File};
use std::os::unix::fs::FileTypeExt;
use std::path::Path;

use chorus_soloist::{lock_file_name, pcm_file_name, socket_file_name};

use crate::args::PipewireMode;
use crate::pipewire::SOCKET;

/// The file in the runtime directory that holds the receiver index this
/// container's supervisor claimed, as decimal text and a line feed.
pub const INDEX_FILE: &str = "receiver";

/// Record the claimed index for the probe. The runtime directory is private
/// to the container, so the file names this supervisor and no other.
pub fn write_index(runtime: &Path, index: usize) -> std::io::Result<()> {
    fs::create_dir_all(runtime)?;
    fs::write(runtime.join(INDEX_FILE), format!("{index}\n"))
}

/// Look at one receiver; `Ok` names it, `Err` says what is wrong.
pub fn check(soloist_dir: &Path, runtime: &Path, pipewire: PipewireMode) -> Result<String, String> {
    let index_file = runtime.join(INDEX_FILE);
    let text = fs::read_to_string(&index_file)
        .map_err(|e| format!("no receiver is claimed ({}: {e})", index_file.display()))?;
    let index: usize = text
        .trim()
        .parse()
        .map_err(|_| format!("{} does not hold an index", index_file.display()))?;
    let lock_path = soloist_dir.join(lock_file_name(index));
    let lock = File::open(&lock_path).map_err(|e| format!("{}: {e}", lock_path.display()))?;
    match lock.try_lock() {
        Err(fs::TryLockError::WouldBlock) => {}
        Ok(()) => {
            let _ = lock.unlock();
            return Err(format!("receiver r{index}'s lock is held by no supervisor"));
        }
        Err(fs::TryLockError::Error(e)) => return Err(format!("{}: {e}", lock_path.display())),
    }
    let kind = |name: String| {
        let path = soloist_dir.join(name);
        fs::metadata(&path)
            .map(|meta| meta.file_type())
            .map_err(|e| format!("{}: {e}", path.display()))
    };
    if !kind(socket_file_name(index))?.is_socket() {
        return Err(format!("r{index}.sock is not a socket"));
    }
    if !kind(pcm_file_name(index))?.is_fifo() {
        return Err(format!("r{index}.pcm is not a FIFO"));
    }
    if pipewire == PipewireMode::Auto {
        let socket = runtime.join(SOCKET);
        let is_socket = fs::metadata(&socket)
            .map(|meta| meta.file_type().is_socket())
            .unwrap_or(false);
        if !is_socket {
            return Err(format!("PipeWire's socket {} is missing", socket.display()));
        }
    }
    Ok(format!("receiver r{index}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    #[test]
    fn healthy_only_with_a_held_lock_a_socket_a_fifo_and_pipewire() {
        let root =
            std::env::temp_dir().join(format!("chorus-soloistd-health-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let (dir, runtime) = (root.join("receivers"), root.join("runtime"));
        fs::create_dir_all(&dir).unwrap();
        let none = PipewireMode::None;

        let why = check(&dir, &runtime, none).unwrap_err();
        assert!(why.contains("no receiver is claimed"), "{why}");

        write_index(&runtime, 2).unwrap();
        assert_eq!(fs::read_to_string(runtime.join(INDEX_FILE)).unwrap(), "2\n");
        let why = check(&dir, &runtime, none).unwrap_err();
        assert!(why.contains("r2.lock"), "{why}");

        // A lock file nobody holds: a dead supervisor's.
        let held = File::create(dir.join("r2.lock")).unwrap();
        let why = check(&dir, &runtime, none).unwrap_err();
        assert!(why.contains("held by no supervisor"), "{why}");
        // The probe let go of the lock it took.
        held.try_lock().unwrap();

        let why = check(&dir, &runtime, none).unwrap_err();
        assert!(why.contains("r2.sock"), "{why}");
        let _listener = UnixListener::bind(dir.join("r2.sock")).unwrap();
        let why = check(&dir, &runtime, none).unwrap_err();
        assert!(why.contains("r2.pcm"), "{why}");
        crate::sys::make_fifo(&dir.join("r2.pcm"), 0o660).unwrap();
        assert_eq!(check(&dir, &runtime, none).unwrap(), "receiver r2");

        let why = check(&dir, &runtime, PipewireMode::Auto).unwrap_err();
        assert!(why.contains("PipeWire's socket"), "{why}");
        let _pipewire = UnixListener::bind(runtime.join(SOCKET)).unwrap();
        assert_eq!(
            check(&dir, &runtime, PipewireMode::Auto).unwrap(),
            "receiver r2"
        );

        fs::write(runtime.join(INDEX_FILE), "many\n").unwrap();
        let why = check(&dir, &runtime, none).unwrap_err();
        assert!(why.contains("does not hold an index"), "{why}");
        fs::remove_dir_all(&root).unwrap();
    }
}
