# 0012: the CPU-time bound goes on first, and a check says so

- Status: decided
- Recorded by: umbrella spec S0020-chorus-sound-2-docs-fix
- Follows: `docs/decisions/0010-the-host-contract.md`,
  `docs/decisions/0011-the-audio-path-is-a-list.md`

## Decision

Every real-time acquisition in this repository applies the `RLIMIT_RTTIME`
CPU-time bound **before** it takes the scheduling policy, and the test suite
fails if any of them does not. The acquisitions are enumerated in
`real-time-acquisitions.conf`; an acquisition in a unit that file does not name
fails the suite as unaccounted for.

## Why the ordering, and not merely the bound

`sched(7)`: "A nonblocking infinite loop in a thread scheduled under the
SCHED_FIFO, SCHED_RR, or SCHED_DEADLINE policy can potentially block all other
threads from accessing the CPU forever." `RLIMIT_RTTIME` is the documented
answer to that, and `tools/spin-test.sh` is the entry point that makes it fire.

A bound applied *after* the policy is not the same safety property. Between the
two calls the thread is real-time and unbounded, and anything that spins in
that window starves the host exactly as the manual describes. The ordering is
the property; the bound alone is a configuration.

## Why a check, when the tree already did it

It already did. `docs/verification-record.md` said so, in the past tense, and
named a check that did not exist. That is the failure this record exists to
close: a true statement about an undefended invariant reads exactly like a
defended one, and the next refactor to move a line finds nothing in its way.

So the ordering is graded rather than remembered, in the ordinary suite, on
every change. It needs no ceiling and no privilege because it reads source
text, which is why it runs where the spin test cannot.

## Why the acquisitions are a list

Same argument as `docs/decisions/0011`, applied to a different question. A
check that grades only the sites it happens to find cannot tell "both
acquisitions are fine" from "there is a third one I did not look at". The file
makes the set of acquisitions a thing someone decided rather than a thing the
scanner inferred.

`real-time-acquisitions.conf` has no exclusion section, and that is the
deliberate difference from `audio-path.conf`. There, an exclusion carries a
reason and the reason can be true: a unit that only ever writes a log header
really is off the audio path. Here there is no sentence that makes an unbounded
real-time thread safe, so the only way to answer a finding is to move the
bound. Being on the list does not excuse a unit from the ordering check; it
subjects it to one.

## What the check does not catch, said out loud

It scans source text, one line at a time, and it inherits the limits
`docs/decisions/0011` already records for its neighbour: a string literal that
spans lines and a `/* */` block comment are not tracked across lines.

Two limits are its own, and both are in the safe direction:

- A raw `sched_setscheduler` call that bypassed `chorus-hostctl` would not be
  seen as an acquisition. Every caller in this tree goes through the wrapper,
  which is where the ordering can be stated at all.
- A bound applied in one function for a policy taken in another is not
  credited. The check fires where it need not have, and the answer is to put
  the bound next to the acquisition, which is where it belongs.

Unlike its neighbour, this check reads a name inside a string literal as
prose. It has to: this repository quotes the very function names it searches
for, in doc comments, in error messages and in the checker's own constants, and
a scanner that read those as code would fire on its own documentation.

## Why the demonstrations are committed rather than branched

Three of them, in `crates/audio-path/tests/real_time_ordering.rs`, each against
a scratch copy of the tree: one swaps the two statements of a real acquisition
so the policy comes first, one adds an acquisition in a unit the list does not
name, and one puts the names in a doc comment, a line comment and two string
literals and asserts nothing fires. The reasoning is the one 0011 already gave:
a branch proves a check is not vacuous once, for whoever was told the branch
name; a committed test proves it on every run, for everyone.

## Consequences

- `crates/hostctl`'s own ceiling-zero unit test applies the bound before it
  asks for a policy. It is a site like any other, and a committed
  counter-example in a test is a counter-example just the same.
- A new thread that wants a real-time policy has to say so in
  `real-time-acquisitions.conf` and bound itself first, or the suite fails.

## Revisit when

A real-time acquisition has to happen somewhere the enclosing function cannot
also carry the bound, or the first dependency arrives with a threading model of
its own.
