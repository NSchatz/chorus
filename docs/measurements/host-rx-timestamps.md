# User-space receive stamps against kernel software receive timestamps, on the shared development host

Date: 2026-09-30
Source: host
Build measured: `66b44873fc9628d0ff9d1a63564e458ac9e577c1`
Build note: `cargo build --release --locked -p chorus-hostprobe` of that commit (the merge of PR
#47), run from `target/release/chorus-rx-stamps`.
Timing evidence: none. This is a host run in a shared container: SCHED_OTHER, no rtprio
(`RLIMIT_RTPRIO` 0), a cgroup quota of 28 CPUs, other programs' work on the same machine,
loopback and a container veth rather than the production NIC. It is NOT timing evidence for the
production server (BRIEF.md section 3.1 rule 3); it says what the mechanism delivers and how
large the gap it closes is here.

## The question

The server's time-sync stamps are user-space stamps on its monotonic timeline:
`t1` is taken when a request has been read and decoded off the client's connection, `t2` when the
reply is encoded (`crates/server/src/stream.rs`, `read_requests` and `write_outbound`). The
connection is **TCP**: the request arrives on the same `TcpStream` the audio leaves by, inside
the v2 session (`crates/server/src/clients.rs`). Everything between the request packet entering
the kernel and that read returning (the socket wakeup, the scheduler, the read itself) is inside
`t1`. The kernel can stamp the packet on arrival: `SOF_TIMESTAMPING_RX_SOFTWARE` asks for "rx
timestamps when data enters the kernel. These timestamps are generated just after a device driver
hands a packet to the kernel receive stack" (Documentation/networking/timestamping.rst,
<https://docs.kernel.org/networking/timestamping.html>, read 2026-09-30).

## Verified against primary sources (read 2026-09-30)

- **Do TCP sockets get software receive stamps?** The research marked this ASSUMED. The kernel
  documentation says `SO_TIMESTAMPING` "Supports generating timestamps for stream sockets"
  (timestamping.rst section 1) and describes stream semantics in detail only for transmit;
  socket(7) says the `SO_TIMESTAMPNS` stamp is "the reception time of the last packet passed to
  the user in this call" (man-pages 6.19, <https://man7.org/linux/man-pages/man7/socket.7.html>).
  Observed here: **yes**. Every one of 15000 TCP reads over three runs carried a
  `SCM_TIMESTAMPING` record with a non-zero `ts[0]`, and `tests/rx_stamps.rs` (in the gate)
  asserts it for TCP and UDP.
- **Which clock?** socket(7): "The clock used for the timestamp is CLOCK_REALTIME" (for
  `SO_TIMESTAMPNS`); timestamping.rst calls `SO_TIMESTAMP`'s stamp "(not necessarily monotonic)
  system time" and says `SO_TIMESTAMPNS` is the "Same timestamping mechanism". So the kernel's
  software receive stamp is on the settable clock, not the server's monotonic timeline.
- **Privilege?** None was needed for software receive stamps here (an unprivileged process,
  CapEff 0); the documentation names admin rights only for configuring hardware timestamping on
  a device (timestamping.rst section 3).
- **Record layout:** "ts[0] holds a software timestamp if set, ts[1] is again deprecated and
  ts[2] holds a hardware timestamp if set" (timestamping.rst 2.1.2); the probe uses
  `SO_TIMESTAMPING_NEW` (the kernel accepted option 65 in every run), which the documentation
  recommends ("Always use SO_TIMESTAMPING_NEW").

## Method and the clock domain

`chorus-rx-stamps` (`crates/hostprobe`, ADR 0047) enables
`SOF_TIMESTAMPING_RX_SOFTWARE | SOF_TIMESTAMPING_SOFTWARE` on the receiving socket, blocks in
`recvmsg`, and reads `CLOCK_REALTIME` first thing after it returns, then `CLOCK_MONOTONIC`.
`user - kernel` is then one duration on one clock. Two guards against the settable clock:

- A sample across which `CLOCK_REALTIME - CLOCK_MONOTONIC` moved by more than 1 ms (a step) is
  discarded and counted. None was, in any run.
- A second view that does not depend on the offset between the clocks: for consecutive packets,
  (gap between the user `CLOCK_MONOTONIC` stamps) - (gap between the kernel stamps). A slew of
  the settable clock still scales the kernel gaps by its rate (adjtime(3): "by some small
  percentage"); over the 2-10 ms gaps here that is ASSUMED negligible against the microseconds
  measured.

Transports: TCP over 127.0.0.1 (the server's socket kind), one 16-byte write per 2 ms with
`TCP_NODELAY`; UDP over 127.0.0.1, one datagram per 2 ms; ICMP echo from an unprivileged ping
socket (icmp(7), `ping_group_range`) to the container's default gateway through `eth0`, a veth,
one per 10 ms, the reply stamped. The last is the one path here where the stamped packet comes in
through a network device other than loopback; there is no other machine to send from.

Load while it ran: 1-minute load average 8.3 to 11.6, the same shared host as
`host-wakeup-jitter.md` (the runs were interleaved with it, 13:54-14:01 UTC).

## Results

`user - kernel`, microseconds. No read was unstamped, coalesced (more than one message per
read), discarded for a clock step, or lost, in any run.

| transport | run | packets | min | p50 | p99 | p99.9 | max |
|---|---|---|---|---|---|---|---|
| tcp-loopback | 1 | 5000 | 10.622 | 57.669 | 93.241 | 104.074 | 111.985 |
| tcp-loopback | 2 | 5000 | 9.065 | 56.729 | 94.718 | 106.453 | 116.363 |
| tcp-loopback | 3 | 5000 | 9.203 | 61.147 | 95.912 | 107.888 | 314.510 |
| udp-loopback | 1 | 5000 | 6.319 | 39.282 | 69.031 | 79.764 | 153.453 |
| udp-loopback | 2 | 5000 | 6.360 | 53.269 | 72.950 | 89.286 | 109.286 |
| udp-loopback | 3 | 5000 | 6.531 | 42.195 | 66.766 | 80.413 | 152.003 |
| icmp-gateway (eth0) | 1 | 2000 | 5.093 | 16.200 | 33.913 | 39.687 | 44.293 |
| icmp-gateway (eth0) | 2 | 2000 | 5.033 | 16.214 | 28.157 | 33.924 | 45.007 |
| icmp-gateway (eth0) | 3 | 2000 | 5.458 | 9.945 | 28.987 | 41.300 | 45.510 |

The consecutive-packet view, (user gap) - (kernel gap), microseconds; its median is within
0.2 us of zero in every run, which is what it should be if both stamps see the same packets and
the settable clock did not move under them:

| transport | run | min | p50 | p99 | p99.9 | max |
|---|---|---|---|---|---|---|
| tcp-loopback | 1 | -61.162 | -0.064 | 42.558 | 70.961 | 90.344 |
| tcp-loopback | 2 | -67.198 | -0.171 | 43.826 | 61.058 | 70.835 |
| tcp-loopback | 3 | -290.028 | -0.036 | 39.783 | 58.111 | 234.331 |
| udp-loopback | 1 | -132.627 | -0.176 | 42.370 | 55.999 | 138.582 |
| udp-loopback | 2 | -67.684 | -0.070 | 43.562 | 51.574 | 58.716 |
| udp-loopback | 3 | -129.395 | -0.091 | 37.259 | 55.900 | 134.566 |
| icmp-gateway (eth0) | 1 | -26.065 | -0.059 | 15.450 | 25.105 | 31.993 |
| icmp-gateway (eth0) | 2 | -34.160 | -0.032 | 12.455 | 22.566 | 30.600 |
| icmp-gateway (eth0) | 3 | -24.870 | -0.064 | 15.555 | 24.582 | 25.063 |

## What it means for the server's t1 and t2

- **The mechanism works on the server's socket kind.** TCP delivers a kernel software receive
  stamp with every read, unprivileged, with no change to the protocol.
- **The gap it would close is tens of microseconds here, about 100 us at the tail.** On TCP over
  loopback, a user stamp taken the moment `recvmsg` returns is 57-61 us after the kernel's stamp
  at the median and 93-96 us at p99, 314 us at worst. The server's real `t1` is later still: it
  is taken after the v2 record is decrypted and the frame decoded, which this check does not
  include. The size tracks the wakeup latency of a blocked SCHED_OTHER thread in
  `host-wakeup-jitter.md` (p50 24-42 us); the ICMP path, where the reader blocks microseconds
  after sending and the reply follows quickly, sits lower (p50 10-16 us). That the delta is
  mostly the reader's wakeup is an inference from those two observations, not measured
  separately.
- **In the sync arithmetic** a `t1` late by `d` moves that exchange's offset by `d/2` and adds
  `d` to its round trip (`crates/protocol/src/message.rs`), so the client's minimum-round-trip
  filter already prefers the exchanges with the smallest `d`. Against the 500 us wired bound
  (`config/transport.conf` `wired_bound_us`) a tail of 100 us at `t1` is up to 50 us of offset
  error on the exchanges the filter does not reject: not negligible, not dominant, and on
  SCHED_OTHER only. The production SCHED_FIFO run decides how much is left there.
- **`t2` cannot be taken from the kernel in the same message.** A transmit stamp is reported
  after the send, on the error queue (timestamping.rst 2.1.1), so it cannot be written into the
  reply it stamps; using it would need a follow-up message carrying `t2` (the two-step pattern),
  which is a protocol change.
- **The clock domain is the obstacle, not the socket.** The kernel stamp is `CLOCK_REALTIME`. To
  put it on the server's monotonic timeline the server would have to read `CLOCK_REALTIME` on the
  time-sync path, at least as a duration (`t1 = monotonic_now - (realtime_now - kernel_stamp)`,
  with a step guard like this check's). BRIEF.md section 3.1 keeps settable clocks off the audio
  and timestamp path and the program may only tighten that guardrail, so this is not a change a
  goal can make.

## Proposal (not adopted; the production stamping is unchanged)

Keep the user-space monotonic `t1`/`t2` as they are. If the production SCHED_FIFO packet
(`host-wakeup-jitter.md`, "Next") shows the server's wakeup tail is a material share of the
wired bound, the options for the owner are: (a) a kernel receive stamp for `t1`, converted to a
monotonic duration as above, which needs the owner's decision on the guardrail because it reads
the settable clock on the timestamp path; (b) hardware receive timestamps on a NIC that has a PHC
(timestamping.rst section 3 and the PTP clock API), which moves the stamp into a third clock
domain and depends on the production NIC; (c) nothing, if the SCHED_FIFO tail is small. This
report recommends (c) until the production numbers exist, and (a) only as a proposal with the
guardrail question stated.

## Re-run

```
cargo build --release --locked -p chorus-hostprobe
target/release/chorus-rx-stamps --transport tcp-loopback --samples 5000 --period-us 2000
target/release/chorus-rx-stamps --transport udp-loopback --samples 5000 --period-us 2000
target/release/chorus-rx-stamps --transport icmp-gateway --samples 2000 --period-us 10000
```

Not under the heavy-job lock. The ICMP transport needs the caller's group inside
`net.ipv4.ping_group_range` (here `0 2147483647`). The output prints the gateway's interface, not
its address.

## Next

The receive-stamp check is not in the server image; a production-host version of it would
stamp a real endpoint's requests on the real NIC, which is the server change in (a) above rather
than a separate probe. Nothing further is asked of the owner for this report beyond the wakeup
packet.
