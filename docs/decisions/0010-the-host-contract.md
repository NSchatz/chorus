# 0010: the scheduling and memory contract, and why refusing is the safe answer

- Status: decided
- Recorded by: umbrella spec S0015-chorus-sound-2 (roadmap phase SOUND-2)
- `sched(7)`, `capabilities(7)`, `time_namespaces(7)`, Docker's `--ulimit`

## Decision

The server reads the real-time priority ceiling its container was granted and
takes a priority no greater than it; bounds every real-time thread with
`RLIMIT_RTTIME` **before** that thread does any audio work; decides the
memory-locking question and reports it either way; and **exits non-zero** when
the host grants neither, unless configuration explicitly enabled running
without that part of the contract, in which case every subsequent status report
says so.

The report is compared against `/proc/self/task` inside the process, and a
thread running under a real-time policy that was not reported is a hard
failure.

## Why this is a safety decision and not a tuning one

`sched(7)`, about the policy this server asks for: "A nonblocking infinite loop
in a thread scheduled under the SCHED_FIFO, SCHED_RR, or SCHED_DEADLINE policy
can potentially block all other threads from accessing the CPU forever."

The server is meant to run on a machine that runs other things. A bug in a
hobby audio daemon should not be able to take the rest of the household's
services with it, and the manual names the safeguard rather than advising care:
`RLIMIT_RTTIME` per process, enforced by the kernel against each real-time task
individually and reset whenever that task blocks.

So the bound is not a setting. It is the difference between a bug here and an
outage elsewhere, and the phase's verification demands that it **fire**, not
that it be configured: `tools/spin-test.sh` runs a deliberate non-yielding loop
in a real-time thread beside a normal-priority heartbeat and asserts both that
the offender dies and that the heartbeat never loses a second.

## Why the bound goes on before the policy

`take_contract_for_this_thread` sets `RLIMIT_RTTIME` first and asks for the
policy second. The order is the property. A bound applied after the thread
started doing work would leave exactly the window `sched(7)` warns about, and
the window would be short enough to be invisible in testing and long enough to
matter on the one day something loops.

If the policy is then refused, the bound cost nothing.

## Why soft and hard are set to the same value

`getrlimit(2)` says a process reaching the soft `RLIMIT_RTTIME` limit is sent
`SIGXCPU`, whose default action terminates it, and that if the signal is caught
the process keeps receiving `SIGXCPU` once a second until the hard limit brings
`SIGKILL`. Setting them equal means the bound fires once, promptly, and a
handler that swallowed `SIGXCPU` could not turn the bound into a suggestion.

## Why refusing is the safe answer

The alternative is a server that quietly runs as an ordinary process while the
deployment believes it has a real-time policy. That failure is invisible: it
looks like a working system with occasional audible glitches that nobody can
attribute, on a machine where the honest explanation ("it never had the
priority") is the last thing anyone would check.

Refusing names the limit it read and the priority it wanted, so the fix is one
flag on the run command. The same reasoning applies to locked memory: a page
fault on the audio path under memory pressure is a stall that scheduling
priority cannot help with, and a server silently running unlocked would produce
exactly the same unattributable glitches.

Two escapes exist, both explicit:

- `--allow-non-realtime`
- `--allow-unlocked-memory`

Both are opt-in, and a run that uses either says so in **every** status report
it prints, not once at start. A single start-up line scrolls away; a phrase on
every line does not.

## Why the report is checked against /proc

A report nobody can compare against anything is a claim. `/proc/self/task` is
the kernel's own answer to "which of your threads are real-time and at what
priority", so the server reads it, prints both, and fails hard when they
disagree. The comparison also catches the case a self-report structurally
cannot: a thread the process did not know it had.

Field 40 of `/proc/<tid>/stat` is `rt_priority` and field 41 is `policy`. The
parser splits after the **last** `)` because the command name is field 2, is
parenthesised, and may itself contain spaces and parentheses. Getting that
wrong is the classic way to misread this file, and there is a test with a
deliberately hostile command name.

## What the container grants, and what it cannot

`deploy/run-server.sh` is the other half of `deploy/Dockerfile`, because a
Dockerfile cannot grant a resource limit. Docker's reference documents the
spelling: `--ulimit <type>=<soft>[:<hard>]`, with `rtprio` as "Maximum
real-time scheduling priority" and `memlock` as "Maximum locked-in-memory
address space". `capabilities(7)` names the bigger hammers, `CAP_SYS_NICE` and
`CAP_IPC_LOCK`; limits are the smaller ones and are what this uses.

`RLIMIT_RTTIME` is deliberately **not** set on the run command. It has to be in
force from the first instruction of a real-time thread, which only the process
can arrange.

## What this decision does not do

It does not deploy anything onto anyone's production host. That is a different
repository's territory, it was explicitly out of scope for the spec this work
was built from, and `docs/sound-2.md` records which half of the roadmap phase's
outcome therefore remains.

## Revisit when

The server grows a second real-time thread, or a measurement shows the 200 ms
CPU-time bound is either too tight for a legitimate burst or too loose to
protect the host.
