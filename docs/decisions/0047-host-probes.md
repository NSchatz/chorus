# 0047: host evidence comes from a chorus-owned probe crate with two small FFI modules

- Status: accepted (goal 7, 2026-09-30)
- Decided by: the goal (brief section 11 item 2, K34; research-platform-network.md section 5)
- Implemented in: `crates/hostprobe` (`chorus-wakeup-probe`, `chorus-rx-stamps`),
  `audio-path.conf`, `real-time-acquisitions.conf`, `tools/conventions/check-rust-lints.sh`,
  `tools/image.sh`, `deploy/Dockerfile`

## Context

K34 asks for host evidence on the two things the server's timing rests on: how late a periodic
thread wakes under real load, and how much of the server's `t1` stamp is the host rather than the
network. Both need syscalls the standard library does not expose: `clock_nanosleep` with
`TIMER_ABSTIME`, `prctl(PR_SET_TIMERSLACK)`, `setsockopt(SO_TIMESTAMPING)` and `recvmsg` with
control messages. The rule "Rust lints and unsafe" requires a record for every new place unsafe
code is allowed.

## What was read

All read 2026-09-30. No GPL source was opened: the Linux kernel is GPL, so its behaviour is
taken from its documentation and man pages only, and from what the probes observe.

- Documentation/networking/timestamping.rst, <https://docs.kernel.org/networking/timestamping.html>
  (section 1 the control interfaces and flags, 2.1 `SCM_TIMESTAMPING` records, 2.1.2 receive timestamps).
- man-pages 6.19 on man7.org: socket(7) (`SO_TIMESTAMP`, `SO_TIMESTAMPNS`),
  clock_nanosleep(2), `PR_SET_TIMERSLACK(2const)`, icmp(7) (`ping_group_range`), adjtime(3).
- The MIT/Apache `libc` crate 0.2.189 (permissive source, from the local cargo registry):
  `src/unix/linux_like/mod.rs`, `src/unix/linux_like/linux/mod.rs`,
  `src/unix/linux_like/linux/arch/generic/mod.rs`, `src/unix/linux_like/linux/gnu/mod.rs`,
  `src/unix/linux_like/linux_l4re_shared.rs` (constant values and struct layouts).
- chorus: [`.claude/goals/2026-09-chorus-research/research-platform-network.md`](https://github.com/NSchatz/chorus/blob/535ed2816b5735153ac81c52a235024aa806f2b5/.claude/goals/2026-09-chorus-research/research-platform-network.md) section 5,
  `crates/hostctl/src/lib.rs`, `crates/server/src/stream.rs`, `crates/server/src/bin/chorus-rt-spin.rs`,
  `crates/audio-path/src/realtime.rs`, ADR 0020, ADR 0022, `tools/image.sh`, `deploy/`.

## Decision

1. **A new crate, `chorus-hostprobe`, not `chorus-hostctl`.** `hostctl` is the server's
   scheduling contract and its `audio-path.conf` exclusion says it "reads resource limits and
   /proc, never a clock". A probe reads clocks by definition, so it lives beside `hostctl` and
   uses it (the rtprio ceiling, the CPU-time bound, `SCHED_FIFO`), rather than widening it.
2. **Unsafe is allowed in exactly two modules, once each**: `src/sys.rs` (clock_gettime on
   `CLOCK_MONOTONIC`, clock_nanosleep, prctl) and `src/net.rs` (setsockopt, recvmsg, a ping
   socket, and `CLOCK_REALTIME`). No external crate: the constants and layouts are the ones the
   MIT/Apache `libc` crate 0.2.189 carries for 64-bit Linux, cited in each module.
3. **The wakeup probe is held to the audio path's clock rule.** Its units are listed on-path in
   `audio-path.conf`, so a settable clock creeping into it turns the suite red; a stepped clock
   inside a jitter probe would be read as a late wakeup.
4. **The receive-stamp check reads `CLOCK_REALTIME`, and is excluded with that reason.** The
   kernel's software receive stamps are `CLOCK_REALTIME` ("The clock used for the timestamp is
   CLOCK_REALTIME", socket(7), man-pages 6.19, https://man7.org/linux/man-pages/man7/socket.7.html,
   read 2026-09-30). The honest comparison is on that clock: `user - kernel` is one duration on
   one clock, a sample across which `CLOCK_REALTIME - CLOCK_MONOTONIC` moved by more than 1 ms is
   discarded and counted, and a second view compares consecutive-packet gaps. The server does not
   link the crate.
5. **`--policy fifo` bounds first.** It calls `bound_real_time_cpu_time` and then
   `take_real_time_policy` in one function, and the binary is listed in
   `real-time-acquisitions.conf`, so ADR 0022's ordering check grades it.
6. **The probe ships in the server image** (`tools/image.sh`, `deploy/Dockerfile`) so the owner's
   production run is a `docker exec` into the deployed container, with that container's own
   rtprio, memlock and CPU grants; the image test runs it once.
7. **The self-test is deterministic by construction.** An injected wakeup is made late by first
   sleeping to `deadline + inject`; a sleep never ends early, so every injected lateness is at
   least `inject` whatever the worker's load, and the test asserts only that (ADR 0020: no
   determinism bought with headroom). Nothing asserts that an uninjected wakeup is quick.

## Consequences

- Host numbers are reports (`docs/measurements/host-*.md`, `Source: host`), never assertions
  and never timing evidence for the production server (BRIEF.md section 3.1 rule 3).
- The server's production stamping is unchanged by this record. Whether `t1` should come from
  the kernel's receive stamp is a separate question with a guardrail in it (the stamp is
  `CLOCK_REALTIME`); the receive-stamp report states the numbers and the proposal.
