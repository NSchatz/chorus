# SOUND-2: first sound, and the host contract that carries it

What this phase built, how to run it, and every number it chose. Written so
that the phases after it inherit stated choices rather than discovered ones.

## What exists now

| piece | where |
|---|---|
| PCM ingest, chunking, the monotonic timeline | `crates/audio` |
| the wire format, now with an in-band end of stream | `crates/protocol` |
| the server | `crates/server`, binary `chorus-server` |
| the Linux client | `crates/client-linux`, binary `chorus-client` |
| ALSA playback | `crates/alsa` |
| the scheduling and memory contract | `crates/hostctl`, `deploy/` |
| the audio-path enumeration and its checks | `audio-path.conf`, `crates/audio-path` |
| the real-time acquisitions and their ordering check | `real-time-acquisitions.conf`, `crates/audio-path` |
| grading a saved delay log | binary `chorus-delaylog-check` |
| the verifications | `tools/` |
| what ran when this was built, and what did not | `docs/verification-record.md` |

## Which half of the phase this delivers

The roadmap's SOUND-2 outcome has two halves: "A Linux client plays a
server-timestamped PCM stream through real speakers with a stable buffer",
**and** "the server earns its place on the household's production host rather
than borrowing it".

**This work delivers the first half and the container's own scheduling
contract. It does not deploy onto the production host.** That deployment is
`homelab` territory: its inventory, container placement, storage, networking,
TLS and secrets are untouched here, and the spec this work was built from put
that change explicitly out of scope. Everything in `deploy/` is runnable on any
Linux host that can hand a container an `rtprio` ceiling, and none of the
acceptance was graded on the production host.

So: SOUND-2 is not delivered on the strength of this work alone. The next
roadmap reconciliation reads this paragraph, and the pinned tree it describes,
rather than inferring completion from the phase id.

## How to run it

### The server

```
cargo build --release
./target/release/chorus-server --listen 0.0.0.0:4010 --source tone
```

It refuses to start when the host granted no real-time priority, or when it
cannot lock the memory it asked for. On a development machine that grants
neither:

```
./target/release/chorus-server --listen 0.0.0.0:4010 --source tone \
    --allow-non-realtime --allow-unlocked-memory
```

Every status report from such a run says which part of the contract it is
missing. That is deliberate: a log line that does not mention it would let a
run without a real-time policy be mistaken for one with it.

In a container, with the contract granted, `deploy/run-server.sh` is the run
command and `deploy/README.md` explains each `--ulimit`.

### The client

```
./target/release/chorus-client --server 127.0.0.1:4010 --device default \
    --delay-log /tmp/chorus-delay.log
```

`--device` is an ALSA PCM name. `default` is the machine's default card;
`hw:Loopback,0` is a `snd-aloop` loopback; `null` opens and discards.

Two one-minute checks sit behind that, and which one you can run depends on the
device you have:

```
./tools/start-fill-and-log-shape.sh   # any device that OPENS, `null` included
./tools/delay-log-shape.sh            # a device that PACES: a card or a loopback
```

The first grades the start fill, the shape of the delay log and the three bound
relations, and deliberately grades nothing that rests on the delay a device
reports. The second grades the log the way the ten-minute run is graded, which
is why it refuses `null`: a device that reports a delay of zero forever could
only fail it. Both refuse, visibly and non-zero, where their device is absent.

### The ten-minute run

```
./tools/ten-minute-run.sh docs/measurements/ten-minute-run.log
```

Point `CHORUS_CLIENT_DEVICE` at real speakers first. The log it writes IS the
evidence, and it can be graded afterwards, anywhere, by someone who did not run
it:

```
./target/release/chorus-delaylog-check docs/measurements/ten-minute-run.log \
    --min-graded-seconds 600 --require-zero-underruns --require-no-rate-change
```

### The spin test

```
./tools/spin-test.sh
```

Run it in a container started by `deploy/run-server.sh`'s limits, or anywhere
with `RLIMIT_RTPRIO` above zero. It refuses, loudly, where the ceiling is zero.

### Everything that needs no environment

```
make test        # the whole suite, including the audio-path checks
make verify      # the refusal paths, and that unrun checks are visibly unrun
```

## The values this phase chose, and the arithmetic behind them

All of these live in `config/verification.conf` and are the client's compiled
defaults as well. Every one is recorded in the delay log at start, so a run can
be graded without knowing them in advance.

| value | chosen | why |
|---|---|---|
| chunk duration | 20 ms | 960 frames at 48 kHz, so the chunk is a whole number of frames and the timestamp delta is exact. One chunk of PCM is 3840 bytes, comfortably inside the protocol's 65535-byte payload. Short enough that one lost chunk is a small hole, long enough that the per-frame overhead is negligible. |
| minimum bound | 60 ms | Three chunks. Above zero, which is the rule that matters: a minimum of zero would make "inside the bounds" true of an empty buffer, and an empty buffer is where an underrun comes from. |
| maximum bound | 300 ms | Fifteen chunks. Latency a listener would notice if it were the whole path, and small enough that a client holding the maximum plus one chunk is a bounded amount of memory on a shared host. |
| start fill | 120 ms | Six chunks. Strictly between the bounds, with 60 ms of room below and 180 ms above. |
| device delay target | 120 ms | The same as the start fill, so the reported delay lands where it is meant to sit the instant output begins and stays there. |
| overflow rate difference | 2000 ppm | The deliberate difference the over-rate verification applies. |
| CPU-time bound | 200 ms | Two orders of magnitude more than one chunk's worth of work, and short enough that a runaway thread is stopped inside a fifth of a second. |
| real-time priority asked for | 20 | Well above an ordinary thread, far below anything the kernel runs. Clamped to whatever ceiling the host actually granted. |
| locked memory wanted | 64 MiB | More than the audio path holds, so the grant is not marginal. |

**The relation that makes the bounds real.** The span is 300 - 60 = 240 ms. A
source running 2000 ppm faster than the sink adds 2000 microseconds of audio
per second of real time, so it crosses that span in 240000 / 2000 = **120
seconds**, which is inside one ten-minute run. The client refuses at start any
configuration where that number is not under 600 s, and
`chorus-delaylog-check` refuses a log whose recorded configuration does not
satisfy it. Bounds so wide that no run could cross them would leave the delay
assertion carried entirely by the underrun count, which is not the same claim.

**These are choices, not targets.** No number here is borrowed from another
project or from a sync requirement. SYNC-4's inter-device error budget is a
different quantity and this phase asserts nothing about it.

## The transport, and how a length mismatch is detected under it

**TCP.** One connection, one client, chunk frames then one `stream_end` then
close.

TCP does not preserve message boundaries, so the protocol's length-prefixed
framing is what restores them, and the decoder already refuses to consume a
frame whose end it cannot locate. What the decoder cannot see is a declared
length that does not match the bytes actually delivered for it: on a byte
stream that looks exactly like a frame whose payload happens to contain the
next frame's header.

Detection therefore lives in the client's session layer, which knows what this
stream is supposed to look like. The rules, in full:

1. **A frame that fully decoded proves alignment.** Every field was in range
   and the payload was the length its type requires. A mis-aligned reader gets
   that wrong almost immediately.
2. **A frame the decoder rejected is one bad frame.** Its header decoded, so
   the length prefix that steps over it came from a catalogued type. Discard
   it, count it as `discarded_malformed`, keep the session open.
3. **A frame skipped for an unknown type is alignment lost.** This is the one
   rule that differs from the protocol document's decoder behaviour, and it
   differs because of the transport rather than the protocol. Stepping over an
   unassigned type using its length prefix is exactly right when the transport
   preserved that frame's boundaries. On TCP nothing corroborates the prefix:
   if alignment is already lost, the "type" is a PCM sample and the "length" is
   two more, and stepping over them is how a reader stays lost. The client
   closes the session with a typed framing error instead. The cost is that a
   newer server's new message type ends this client's session rather than being
   ignored; the alternative is putting mis-framed bytes on a DAC.
   `docs/protocol.md` records that both readings are conformant.
4. **The stream's shape is fixed by its first chunk.** A later chunk claiming a
   different rate, channel count or sample format did not come from where it
   says. Framing error.
5. **Only the last chunk may be short.** A chunk arriving after a short one
   means the short one was truncated rather than final, so its declared length
   did not match the bytes delivered for it. Framing error.
6. **A sequence already accepted** is a duplicate: discarded, counted as
   `discarded_duplicate`, session stays open.

## The end-of-stream signal

Message type `0x03`, `stream_end`, 12 bytes: a `u32` final sequence and a `u64`
end timestamp on the server timeline. `docs/protocol.md` is the normative
definition of `end_timestamp_ns` and gives its relation - the final chunk's
`timestamp_ns` plus one configured chunk duration - and this phase asserts
nothing about it that document does not say. It is committed as a golden vector
(`fixtures/protocol/stream_end.hex`) like every other type, and adding it
changed neither of FOUNDATION-1's two vectors.

It travels **in band, as data on the connection, after the final chunk and
before the close**. That is the whole point. A transport close and a transport
that broke look identical to the peer: both are a read returning zero. A
receiver that saw `stream_end` knows the sender finished; one that did not
knows it lost the sender. Timing cannot tell those apart, which is why a close
is never the signal.

## The clock decision

Every timestamp on the wire and every value in the delay log comes from
`std::time::Instant`, which is `CLOCK_MONOTONIC` on Linux, with the epoch taken
at process start. The two endpoints' epochs are unrelated, which the protocol
already says and which the time-sync exchange exists because of.

`time_namespaces(7)` is why this is the safe choice inside a container: a time
namespace virtualizes `CLOCK_MONOTONIC` with a fixed per-namespace offset and
explicitly does not touch `CLOCK_REALTIME`. So a containerized endpoint's
monotonic clock may be offset from the host's, and it stays monotonic, and no
NTP step can move it.

`audio-path.conf` enumerates every unit on the audio or timestamp path, and
`crates/audio-path` fails the suite if any of them reads a settable clock, or
if the list omits a first-party unit the listed ones reach. The one exclusion
that reads a settable clock is the delay-log writer, once, for a header line a
human reads.

The same crate carries a third check, on a different invariant:
`real-time-acquisitions.conf` enumerates every unit that takes a real-time
scheduling policy, and `crates/audio-path --test real_time_ordering` fails the
suite if any of those sites takes the policy before applying the CPU-time
bound, or if an acquisition turns up in a unit the file does not name. See
`docs/decisions/0012`.

## The memory-locking decision

**The server attempts to lock its memory by default, and refuses to start when
the host denies it.**

Why attempt it: the server is a real-time process sharing a machine with other
services. A page fault on the audio path under memory pressure is a stall the
scheduling priority cannot help with, and `capabilities(7)`'s `CAP_IPC_LOCK`
and Docker's `--ulimit memlock` exist precisely so that a process like this can
avoid one.

Why refuse rather than continue: a server that silently ran unlocked while the
deployment believed it was locked would produce occasional glitches that nobody
could attribute. Refusing names the limit it read and the amount it wanted, so
the fix is one flag on the run command.

`--allow-unlocked-memory` is the explicit escape, and a run that uses it says
`memory=unlocked-by-configuration` in every status report it ever prints.

## What is deliberately absent

- **Any correction.** No offset filtering, no rate correction, no resampling,
  no hard resync, no servo. When the buffer drifts, the client reports the
  drift and keeps playing at the rate it was given. Discarding at the ceiling
  is overflow handling and not correction: the timeline and the playback rate
  are untouched.
- **Any inter-device claim.** Nothing here says two endpoints agree, and
  nothing here could: this phase's run is a single-client run and there is no
  measurement harness yet.
- **Multi-client, zones, groups, volume, mute, discovery, reconnect.** A client
  that loses its server exits.
- **Compression on the wire.**

## For SYNC-4, RIG-3 and TV-9

- The error signal SYNC-4 needs is already read and already logged:
  `snd_pcm_delay`, which `alsa` defines as "the overall latency from the write
  call to the final DAC". The client never uses the return of a write call for
  anything, and never infers an underrun from the delay.
- The playout loop holds the device's delay near a target by choosing **when**
  to write. That is the single place a servo would take over, and it changes
  nothing about the samples.
- The `audio_chunk` header's fourteen reserved bytes are untouched and still
  reserved for TV-9.
- The delay log format is versioned (`chorus delay log v1`) and
  `chorus-delaylog-check` parses it, so a later phase can add columns without
  invalidating a saved run.

## What a device has to be able to do

`chorus-client --probe-device` opens a device, writes 200 ms into it and asks
for the delay. It reports `paces=1` when the delay came back nonzero and
`paces=0` when it did not.

This distinction is load-bearing and easy to miss. The ALSA `null` device opens
perfectly well, accepts every frame instantly and reports a delay of zero
forever. It is a real ALSA playback device and it is no use at all for
verifying anything about a reported delay, because it has no ring to report
about. The verifications that grade the delay require `paces=1` and refuse
otherwise, naming the reason. The verifications that do not, such as the
end-of-stream and connection-loss checks, run against `null` happily.

So the checks split by what they grade, not by what they touch:

| entry point | needs | grades |
|---|---|---|
| `tools/stream-end-and-loss.sh` | a device that opens | the two ways a stream stops |
| `tools/start-fill-and-log-shape.sh` | a device that opens | the start fill, the record's shape, the bound relations |
| `tools/delay-log-shape.sh` | a device that paces | the above plus the delay inside its bounds |
| `tools/overflow-run.sh` | a device that paces | the behaviour at both bounds, and that no rate changed |
| `tools/ten-minute-run.sh` | a device that paces, real speakers for the run that counts | ten continuous minutes, zero underruns, the extremes and margins |

`docs/verification-record.md` says which of these ran when this work was built,
and files the exact command and the verbatim refusal for each one that did not.
