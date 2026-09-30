# Wakeup jitter of a SCHED_OTHER periodic thread, on the shared development host

Date: 2026-09-30
Source: host
Build measured: `66b44873fc9628d0ff9d1a63564e458ac9e577c1`
Build note: `cargo build --release --locked -p chorus-hostprobe` of that commit (the merge of PR
#47), run from `target/release/chorus-wakeup-probe`.
Timing evidence: none. This is a host run in a shared container: SCHED_OTHER, no rtprio
(`RLIMIT_RTPRIO` 0), a cgroup quota of 28 CPUs, other programs' work on the same machine. It is
NOT timing evidence for the production server (BRIEF.md section 3.1 rule 3). The production
figure is the SCHED_FIFO run inside the deployed chorus-server container, which is the owner's
packet after the homelab deploy ("Next", below).

## What was measured

`chorus-wakeup-probe` (`crates/hostprobe`, ADR 0047): on its main thread, an absolute deadline
on `CLOCK_MONOTONIC` every period, `clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME)` until it,
and `lateness = wake - deadline` for every wakeup. Timer slack set to 1 ns through
`prctl(PR_SET_TIMERSLACK)` (read back as 1 ns in every run; unprivileged, as
`PR_SET_TIMERSLACK(2const)` implies by naming no capability, man-pages 6.19,
<https://man7.org/linux/man-pages/man2/PR_SET_TIMERSLACK.2const.html>, read 2026-09-30; the
research's ASSUMED "unprivileged" is now verified here, and
`tests/wakeup_self_validation.rs` holds it). The default slack in this container is 50 us
(`/proc/self/timerslack_ns` 50000). An overrun is a deadline skipped because a wakeup came after
the next one. Percentiles are nearest-rank.

The self-test (`cargo test -p chorus-hostprobe --test wakeup_self_validation`) makes every
10th of 1000 wakeups late by at least 3 ms on purpose and requires the histogram to show all
100 at or above 3 ms (p99, p99.9 and max at or above it, 100 overruns); it passed in the gate of
PR #47.

## The host

```
kernel           6.12.107+deb13-amd64, #1 SMP PREEMPT_DYNAMIC Debian 6.12.107-1 (2026-08-29)
clocksource      tsc
realtime kernel  no (/sys/kernel/realtime absent)
cpus             0-55 online and allowed (56)
cgroup cpu.max   2800000 100000 (28.00 CPUs)
rlimit_rtprio    0
rlimit_memlock   8388608
policy           SCHED_OTHER (as started)
timerslack       1 ns (asked 1 ns)
```

Load: the maker container is shared by other agent sessions and other programs. Load average
over the six runs (13:53-14:01 UTC) was 7.99 to 11.91 (1-minute), 10.25 to 13.91 (5-minute);
earlier the same day it was 23 to 26. What `ps` showed one second into the first run, by CPU:
`ffmpeg` 146%, `cppcheck` 100%, two `holdfast.test` 29% and 15%, `engine.test` 20%, `go` 7%,
several `claude` sessions 1-9%. CPU pressure (`/proc/pressure/cpu`, some avg10) was 0.00-0.02
around every run. The cgroup's `nr_throttled` stayed at 312 through all six runs: the quota
throttled nothing while the probe ran.

## Results

Lateness in microseconds. Every run: 1 voluntary context switch per wakeup, 0 or 1 involuntary.

| run | start (UTC) | period | wakeups | overruns | p50 | p99 | p99.9 | max | load 1-min before/after |
|---|---|---|---|---|---|---|---|---|---|
| 1 ms, 1 | 13:53:30 | 1 ms | 60000 | 27 | 37.387 | 74.322 | 594.535 | 5479.237 | 10.75 / 11.13 |
| 1 ms, 2 | 13:56:10 | 1 ms | 60000 | 9 | 35.984 | 64.653 | 514.845 | 1628.696 | 11.18 / 8.59 |
| 1 ms, 3 | 13:58:50 | 1 ms | 60000 | 5 | 41.983 | 77.607 | 474.838 | 1978.986 | 9.30 / 8.47 |
| 5 ms, 1 | 13:55:10 | 5 ms | 12000 | 0 | 23.768 | 65.684 | 92.467 | 1248.727 | 11.91 / 11.18 |
| 5 ms, 2 | 13:57:50 | 5 ms | 12000 | 0 | 39.983 | 87.926 | 819.813 | 2760.905 | 8.40 / 9.30 |
| 5 ms, 3 | 14:00:30 | 5 ms | 12000 | 0 | 37.051 | 64.292 | 86.855 | 2192.435 | 7.99 / 9.67 |

The minimum was 3.0-4.3 us in every run. Wakeups at or above 100 us / 500 us / 1 ms:

| run | >= 100 us | >= 500 us | >= 1 ms |
|---|---|---|---|
| 1 ms, 1 | 286 | 85 | 12 |
| 1 ms, 2 | 141 | 64 | 9 |
| 1 ms, 3 | 259 | 55 | 5 |
| 5 ms, 1 | 10 | 4 | 1 |
| 5 ms, 2 | 110 | 48 | 2 |
| 5 ms, 3 | 8 | 1 | 1 |

The shape of run "1 ms, 1" (power-of-two buckets, us): <4: 207, 4-8: 6184, 8-16: 10431,
16-32: 6122, 32-64: 35908, 64-128: 945, 128-256: 72, 256-512: 50, 512-1024: 69, 1-2 ms: 5,
2-4 ms: 6, 4-8 ms: 1. The distribution is two-humped: about a third of the wakeups land in
4-32 us and most of the rest in 32-64 us. Why is not measured here; an idle-state exit on the
sleeping CPU is one candidate (the research notes a C6 exit latency of 133 us on the production
host, ASSUMED to apply in some form here), and a production run with and without a
`/dev/cpu_dma_latency` hold is the way to tell (research-platform-network.md section 5).

## What it means, and what it does not

- For an ordinary SCHED_OTHER thread on this shared host, a wakeup is typically 25-40 us late,
  the 99th percentile under 90 us, and roughly one wakeup in a thousand to one in two thousand
  (at 1 ms) is 0.5 ms or more late, with a worst case of 5.5 ms in three minutes.
- The server's audio production has 180 ms of slack (`config/sync.conf`
  `playout_latency_us = 180000`), so lateness of this size does not starve the stream. Where it
  matters is the time-sync stamps `t1`/`t2`: a stamp taken late by `d` moves that exchange's
  offset estimate by `d/2` and lengthens its measured round trip by `d`
  (`crates/protocol/src/message.rs`, `offset_ns` and `rtt_ns`), which the client's
  minimum-round-trip filter then tends to reject. `host-rx-timestamps.md` measures how much of
  `t1` is the host rather than the network.
- None of this says what SCHED_FIFO at rtprio 20 does on the production host; that is the next
  run.

## Re-run

```
cargo build --release --locked -p chorus-hostprobe
target/release/chorus-wakeup-probe --period-us 1000 --seconds 60 --raw w1ms.raw
target/release/chorus-wakeup-probe --period-us 5000 --seconds 60 --raw w5ms.raw
```

Not under the heavy-job lock: the host's own load is what is being measured. The raw lateness
series (one value in ns per wakeup) were written with `--raw` and are not committed; the
summaries above are the record.

## Next (the owner's production run)

After the homelab deploy of an image that carries the probe (`tools/image.sh` puts
`/usr/local/bin/chorus-wakeup-probe` in it from PR #47 on), on the production host, with the
server container running as deployed (`container_name: chorus-server`, `ulimits` rtprio 20 and
memlock 64 MiB, `cpus: 1.0`, `deploy/compose.yaml`):

```
# SCHED_FIFO at the server's own priority, bounded first, 3 x 60 s at 1 ms and 1 x 60 s at 5 ms
for i in 1 2 3; do
  docker exec chorus-server /usr/local/bin/chorus-wakeup-probe \
    --period-us 1000 --seconds 60 --policy fifo --rt-priority 20 --lock-memory-mib 16 \
    > "wakeup-fifo-1ms-$i.txt"
done
docker exec chorus-server /usr/local/bin/chorus-wakeup-probe \
  --period-us 5000 --seconds 60 --policy fifo --rt-priority 20 > wakeup-fifo-5ms.txt
# the same container, SCHED_OTHER, side by side with a FIFO run (cpu.max throttling of
# non-real-time helper threads, survey-homelab.md 4)
docker exec chorus-server /usr/local/bin/chorus-wakeup-probe --period-us 1000 --seconds 60 \
  > wakeup-other-1ms.txt &
docker exec chorus-server /usr/local/bin/chorus-wakeup-probe --period-us 1000 --seconds 60 \
  --policy fifo --rt-priority 20 > wakeup-fifo-1ms-beside-other.txt
wait
```

Each output file carries its own environment block (kernel, clocksource, `cpu.max`, rlimits,
the policy it obtained, slack, load and throttling before and after). Exit 3 means the container
granted no real-time priority, and names the limit it read. That a `docker exec` process gets the
container's `ulimits` is ASSUMED; the probe prints `rlimit_rtprio`, so the run shows it either
way. The files go back as a `bench/*` PR or to the coordinator, and become
`docs/measurements/production-wakeup-jitter.md`, labelled with its source per
`docs/measurements/README.md` and naming the production host as where it ran. The cyclictest cross-check and the `/dev/cpu_dma_latency` run stay in
the same NEEDS-OWNER packet (research-platform-network.md section 5).
