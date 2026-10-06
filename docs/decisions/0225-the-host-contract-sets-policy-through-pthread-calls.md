# 0225: the host contract sets a thread's scheduling policy through the pthread calls, because musl's sched_setscheduler is a stub

- Status: decided by the agent (a cheap, reversible fix), 2026-10-06, on the owner's report of the
  deployed server's log
- Recorded by: the owner's agent harness, in the pull request that adds this record
- Implemented in: `crates/hostctl/src/lib.rs` (`take_real_time_policy`, `leave_real_time_policy`,
  `current_policy`); `tools/image.sh` runs `chorus-hostctl`'s tests on the image's musl target

## Context

On 2026-10-06 the owner pasted the deployed chorus-server's log: a restart loop, each run ending

    host contract refused: a real-time policy at priority 20 was denied against a granted rtprio
    ceiling of 20: Function not implemented (os error 38) (errno 38)

The ceiling was granted, so the host was not declining. ENOSYS comes from the libc: the image's
server is a static musl build (`tools/image.sh`), and musl implements `sched_setscheduler` and
`sched_getscheduler` as stubs that always return ENOSYS, because the Linux system calls act on
one thread while POSIX defines them per process (musl source, MIT licence,
`src/sched/sched_setscheduler.c` and `src/sched/sched_getscheduler.c`). The workspace suite runs
on glibc, where both calls are real, so nothing failed before deploy.

Reproduced here: a test that lowers its own thread to `SCHED_OTHER` (which needs no privilege)
failed on `x86_64-unknown-linux-musl` with `Os { code: 38, ... "Function not implemented" }`
against the old code, and passes on both targets against the new.

## Decision

1. `chorus-hostctl` sets and reads the calling thread's policy with `pthread_setschedparam` and
   `pthread_getschedparam` on `pthread_self()`. Both libcs implement them, and they act on one
   thread, which is what every caller means. The error number they return becomes the same
   `PolicyDenied` and `io::Error` as before; the messages and the refusal rules are unchanged.
2. `tools/image.sh` runs `cargo test -p chorus-hostctl` on the musl target before it builds the
   image, so a libc difference in the host contract fails the image build, not a deploy.
3. One `thread_inventory` test matched glibc's wording for EIO; it now matches `(os error 5)`,
   which both libcs print.

The deployed image keeps refusing until an image with this change is published and homelab
pins it (owner actions, 0222). `--allow-non-realtime` starts it meanwhile, and every status
report then says the run has no real-time policy.

## Not chosen

- **The raw system calls through `syscall(2)`**: per-architecture numbers for the same effect.
- **Building the server on glibc**: the static musl image is decided (0122); one call pair does
  not reopen it.

## Sources

- musl libc, `src/sched/sched_setscheduler.c` and `src/sched/sched_getscheduler.c` (MIT): both
  return `__syscall_ret(-ENOSYS)`, https://git.musl-libc.org/cgit/musl/tree/src/sched (read
  2026-10-06).
- `pthread_setschedparam(3)`, Linux man-pages: sets "the scheduling policy and parameters of the
  thread", https://man7.org/linux/man-pages/man3/pthread_setschedparam.3.html (read 2026-10-06).
