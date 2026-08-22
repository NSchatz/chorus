# 0007: the server and Linux client language, argued on the merits

- Status: decided, and it confirms 0001 rather than replacing it
- Recorded by: umbrella spec S0011-chorus-technology-review-1
- BRIEF.md 5.1; BRIEF.md section 12, decision 1, and decision 16 ("whether any
  of the recommendations above deserve overturning as evidence arrives")

## Why this entry exists when 0001 already exists

`0001-language-rust.md` records a ruling and says so in its own words: "This
file records a decision that was already made. It does not reopen it." It is a
record of an outcome, and a short one. This entry is the argument behind that
outcome: it puts Rust and Go side by side on the axes BRIEF.md 5.1 itself
raises, reaches a verdict from those axes alone, and is explicit about which of
its statements are measured and which are judgment.

It is not an amendment to 0001 and it does not supersede it. The two agree. If
a reader wants to know what was decided, 0001 is the ruling; if they want to
know whether the ruling survives being argued against the alternative, this is
that argument.

## The two candidates

**Rust** and **Go**, and only those two. BRIEF.md 5.1 lists C/C++ and
Node/Python as options and disposes of both in the same breath it raises them:
C/C++ carries a "memory-safety burden on a network-facing daemon", and
Node/Python are "fine for control plane, unfit for the audio path". Neither was
a live candidate for this axis. C reappears below only as the fixed second
implementation the firmware will carry, which is not under review.

## What this review is not permitted to weigh

Which toolchains happen to be installed on the machine a session runs on. Not
what is on `PATH`, not what a base image ships, not what is one command away.
The umbrella's ADR-0035 rules that reasoning inadmissible, and the test it sets
is applied to every sentence below: each reason here is written so that it
would still hold, unchanged, on a machine with every toolchain already
installed. A reason that evaporates on such a machine is not a reason, and none
is offered.

## The evidence standard in this file

`docs/measurements/` holds no reports. Nothing about chorus has been measured
yet, and the guardrail in CLAUDE.md item 2 is explicit that a timing claim
without a report there is not evidence. So:

**Every comparative latency, GC-pause, throughput and resource statement in
this document is a reasoned engineering judgment, not a measured result.** Each
one is labelled again at the point it is made. This includes BRIEF.md 5.1's own
sentence that "measured comparisons show GC languages missing modest
tail-latency targets under memory pressure where Rust stays flat": those
comparisons were made elsewhere, on other workloads, and no report in this
repository reproduces them for chorus. Inside chorus that sentence is inherited
judgment too.

Two things in this repository produce numbers, and neither is language
evidence. The simulator series in 0006 is a property of a filter and a servo,
and 0006 says itself that "simulates synced" is not evidence either; the golden
vectors are a property of a wire format. Both would read the same in either
candidate language, so neither is cited below as though it favoured one.

## Finding 1: tail latency under memory pressure

The binding quantity for the server is not throughput and not mean latency. It
is the tail. BRIEF.md 2.2 asks for under 0.5 ms of inter-device error for a
stereo pair, aspiring to 0.2 ms, and BRIEF.md 5.1 describes the server's job as
timestamping consistently and pushing chunks "with low tail jitter". A single
late push is absorbed by the jitter buffer; a pause correlated with load is a
disturbance the servo then chases, and BRIEF.md 5.3's correction authority is
clamped near +/- 500 ppm, so what the servo can undo per unit time is bounded.

*Reasoned engineering judgment, not measured.* Under memory pressure a
collector's work scales with allocation rate and live-heap size, so the
runtime's contribution to the tail grows exactly when the system is most
loaded, which is the correlation that makes it awkward: the disturbance arrives
with the load rather than independently of it. Rust's deallocation cost is
deterministic and attributable to the code that owns the memory, so the tail is
a property of the program rather than of a background service inside it.

The honest counterweight, and it is real: Go's collector is concurrent, and
BRIEF.md 5.1 itself concedes only "sub-ms GC that still needs care around the
hot path", not a disqualifying pause. This axis is a difference of margin, not
a knockout. What tips it is the size of the numbers next to each other: the
aspirational figure for a stereo pair is 0.2 ms, which is the same order as the
pause budget the fallback would have to be managed around. A design in which
the timing target and the runtime's own pause budget are the same size is a
design where the runtime is a term in the error equation. Prefer the one where
it is not a term at all.

## Finding 2: GC exposure on the audio hot path

Distinct from finding 1. Finding 1 is about how wide the tail gets; this is
about whether the collector is on the path at all, and what it costs to keep it
off.

BRIEF.md 5.1's own fallback wording carries the price on its face: Go is
admissible "with audio buffers kept off the GC heap". That clause is a standing
obligation, not a one-time setup. Every future change to the chunker, the
jitter buffer and the client playout path has to preserve it, and nothing in
the language checks that it was preserved. *Reasoned engineering judgment, not
measured.*

Chorus compounds the obligation in two ways that are already on record. First,
BRIEF.md 5.4 and the roadmap put the same sync, protocol and DSP cores on Linux
and then port them to an ESP32-S3 as C components; a hand-maintained discipline
that has to survive a port to a different language is a discipline that gets
dropped once, quietly, in a diff nobody flagged. Second, this project's whole
method is to make timing properties structural rather than aspirational:
guardrail 4 forbids wall clocks in the audio path outright rather than advising
against them, 0006 makes the simulator a pure function of a committed
configuration rather than asking runs to be comparable, and 0003 puts every
header and message field in network byte order to remove a class of
disagreement rather than documenting how to avoid it.

Rust fits that method here by removing the obligation instead of writing it
down: with no collector, "no GC on the audio path" is a property of the
language rather than a rule a reviewer has to enforce on every future pull
request. That is the single strongest merits argument in this document.

## Finding 3: memory-safety burden on a network-facing daemon

BRIEF.md 5.1 raises this axis as the reason C/C++ loses. Between the two live
candidates it is close to a tie, and it is recorded as one.

Both Rust and Go are memory-safe against the classes that matter for a daemon
parsing frames from whatever is on the other end of a socket: no use-after-free,
no buffer overrun, bounds-checked indexing. `crates/protocol` is the code where
this matters, and 0005 shows the failure mode is real rather than theoretical:
a frame declaring 1000 bytes of payload with 4 bytes in the buffer, sliced
without the length check, panics and ends the process. That is a liveness
failure in either language, and in neither is it memory corruption. Both
candidates would have needed exactly the check 0005 specifies, and
`tests/decoder_robustness.rs` exists in either world.

Two second-order differences, both small. Rust additionally rules out data
races at compile time, which is worth something because 0001 chose plain
threads and the server's shape is one authoritative timeline with per-connection
workers around it; Go answers the same question with a race detector at test
time, which is a weaker but genuine answer. Against that, neither language's
bounds failure is memory corruption and in both the ordinary consequence is a
crashed unit of execution, so neither candidate buys liveness for free.

Verdict on this axis: a narrow edge to Rust because of the threading model
already chosen, and this is not the axis that decides the review.

## Finding 4: cross-language protocol conformance

The protocol is implemented twice by plan, not by accident: CLAUDE.md working
agreement item 5 says the cores are pure libraries with shared fixtures "so the
Rust and C implementations cannot drift apart", and BRIEF.md roadmap phase 5
places that second implementation in ESP32-S3 firmware written in C. The C
mirror is fixed and outside this review. The question is only which primary
language makes the mirror's job easier.

The conformance mechanism itself is already language-neutral and gives neither
candidate an edge: 0002 puts `fixtures/protocol/` at the repository root,
deliberately outside any Cargo project, "because a second-language
implementation has to read them without a Cargo project". Either candidate
could emit those bytes, and 0003 already removed the endianness class of
disagreement by putting every header and message field in network byte order.

The asymmetry is elsewhere, and it is about the port rather than the fixtures.
*Reasoned engineering judgment, not measured.* Rust and C share a memory model
with no runtime and no collector, so porting a sync or protocol core from Rust
to C is a translation of the same shape of code: the same ownership of buffers,
the same explicit integer widths, the same defined wrapping arithmetic that
0002 requires of the seeded PRNG, which "has to be reproducible byte for byte
across a Rust host and a C mirror". Porting from Go means first removing the
things C has no equivalent for, because a Go core written naturally uses
goroutines, channels and heap-managed buffers, and the mirror then has to
re-derive the logic rather than transcribe it. The larger the
semantic gap between primary and mirror, the more the golden vectors are
carrying alone.

Modest edge to Rust, on the ease of the C port rather than on the fixtures.

## Finding 5: development velocity

This is the axis on which Go leads, and it is the only one BRIEF.md 5.1 attaches
the fallback to: Go is "fast to write", Rust has a "steeper learning curve", and
Go becomes the pragmatic choice "if Rust velocity becomes a problem".

Two observations. First, the fallback is conditional, and its condition is about
this project rather than about languages in general. Second, the condition has
not fired. Roadmap phase 1 landed in Rust with two crates, zero external
dependencies (0002), a full framing decision with golden vectors (0003, 0005),
and a deterministic simulator with a CI regression (0006). The one hard problem
that phase hit is written up at length in 0006, and it was a stale-offset
defect in a filter: a signal-processing problem that would have appeared,
identically, in any language. No entry in this log records velocity as a cost.
*This is a reading of what the repository shows, not a measurement of developer
throughput; no such measurement exists.*

The strongest honest form of the Go case belongs here rather than being left
out. If the bulk of the remaining work were control plane (HTTP, JSON, MQTT,
Home Assistant, a UI backend), Go's standard library and build ergonomics would
be a real advantage and findings 1 and 2 would barely apply, because none of
that code is on the audio path. BRIEF.md 5.1 leaves exactly that door open
("whether the control plane lives in the same process or a sidecar" is listed
as open, and "Keep Node/Python for tooling and, if desired, UI glue"). This
entry does not close that door. What it decides is the language of the audio
path, the sync engine and the protocol core. A control-plane sidecar in another
language remains a separate decision on a separate axis, and it would deserve
its own entry.

## Finding 6: maintainability

Velocity is how fast the first version arrives. Maintainability is what year
three costs, and it is a different question.

- **Dependency posture.** BRIEF.md 3.2 says fewer dependencies is a feature,
  and 0002 records both crates as `std` only with no lockfile because there is
  nothing to lock. Both candidates support that posture and Go's standard
  library is the larger one, which cuts both ways here: less to write, but this
  project's stated rule is that writing the small and instructive pieces is
  part of the point.
- **The refactor property.** Much of this repository is expected to be re-tuned
  once the measurement rig exists. 0006 says so outright: at that point "the
  scenarios stop being plausible and start being measured, the gains get tuned
  against both, and this entry gets a successor that cites numbers". 0005 defers
  its rejection policy to a transport that can decide "on evidence", and 0003
  leaves chunk and buffer sizes to be tuned with data. A codebase that will be
  substantially reworked against measurements benefits from a compiler that
  turns a change of representation into a finite list of call sites to fix
  rather than a search. *Judgment, not measurement.* This favours Rust, and it
  favours it more the longer the project runs.
- **Onboarding.** Rust is harder to pick up, and whoever maintains this later
  pays that cost. It is entered here on Go's side of the ledger honestly. It
  does not outweigh finding 2, because an onboarding cost is paid down with
  time while a runtime on the hot path is only removed by a rewrite.

## Is this one choice, or two?

Deliberately checked, because "the server and the Linux client" could be two
decisions wearing one name, and a split verdict was a permitted outcome of this
review. It is one choice, for a reason internal to the design rather than for
tidiness: BRIEF.md 4 has Linux endpoints running "the same logic" as the
embedded ones, BRIEF.md 5.4 recommends developing the client logic on Linux
first and then porting the sync, protocol and DSP cores to the ESP32-S3, and
`crates/protocol` and `crates/sync` are today pure libraries with no socket, no
clock read and no audio device (0002). The server and the Linux client are the
same cores with different edges. Splitting the language would mean implementing
the timing core a third time, in a third language, and the project already
accepts that cost exactly once for the C firmware mirror in order to reach
hardware. There is no case for paying it again to reach a second host binary.

One clarification, since the phrase invites confusion: "the Rust and C
implementations" in CLAUDE.md item 5 names the primary implementation and the
future firmware mirror. It has never described a split between the server and
the Linux client.

## Verdict

**Rust, for the server and the Linux client both.** This affirms BRIEF.md 5.1's
original recommendation.

The reason it is Rust, stated once, plainly: chorus's differentiating property
is a bounded timing error on the audio path, and Rust is the candidate on which
"nothing the runtime does can widen that error" is a property of the language
instead of a discipline maintained by hand across every future change and one
port to C. Findings 1, 2 and 4 point the same way; finding 3 leans the same way
weakly; finding 6 leans the same way over time. Finding 5 is Go's, it is
conditional, and its condition has not fired.

Go is not refused on principle and its fallback is not repealed. BRIEF.md 5.1
attaches one trigger to it, and 0001 restates it: Rust velocity becoming a
problem. That trigger remains available and would be argued in a successor to
this entry, on evidence.

## Follow-ups, outside this entry's scope

- **No implementation change is owed by this verdict.** At the commit this
  review was written against, chorus is a Cargo workspace of `crates/protocol`
  and `crates/sync` and contains no Go. The earlier Go attempt that the
  umbrella's ADR-0035 describes was discarded before it landed, as 0001's own
  "What was explicitly not a factor" section records. There is therefore no
  existing Go implementation needing owner-level reconsideration, and this
  entry proposes no change to any source file, build file or CI configuration.
- **If Go code ever does land on the server or the Linux client**, whether by a
  revived branch or a future phase, it needs owner-level reconsideration
  against this entry before it is built on. That is the follow-up this review
  would have raised, recorded conditionally because its subject does not exist.
- **Every comparative claim above is unmeasured and should be retested.** When
  the measurement rig lands and `docs/measurements/` holds real reports, the
  latency and GC statements in findings 1 and 2 become checkable for the first
  time. Until then they are judgment, and a successor entry citing numbers
  would be worth more than this one.

## Revisit when

Rust velocity becomes a measured problem for the server or the Linux client
(BRIEF.md 5.1's only trigger for the Go fallback), or the measurement rig
produces reports that contradict findings 1 or 2, or the control-plane
process/sidecar question in BRIEF.md 5.1 is settled in a way that puts a second
language in the tree on purpose.
