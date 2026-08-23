# 0011: the audio path is an enumeration, and the list is graded too

- Status: decided
- Recorded by: umbrella spec S0015-chorus-sound-2 (roadmap phase SOUND-2)
- CLAUDE.md guardrail 4: monotonic clocks in the audio path

## Decision

The audio or timestamp path is a **committed list**, `audio-path.conf`. The
test suite fails if any listed unit reads a settable wall-clock source, and it
fails if the list omits a first-party unit that a listed unit depends on and
that is not recorded in the same file as an exclusion with a reason.

## Why a list

The guardrail says no component of the audio or timestamp path may read a
settable wall clock. To enforce that, something has to know what the path is,
and "is this code on the audio path" is not a question you can answer by
watching a process run: a unit that reads a settable clock once an hour looks
exactly like one that never does.

So the path is written down. That is the only form in which the guardrail can
be checked rather than remembered.

## Why the list itself is checked

A list you can shrink is not a check. The obvious way to defeat the first
check is to move the clock read one file down and not list that file, and
nothing about the first check would notice.

So the second check exists: start from the listed units, follow every `mod`
declaration and every use of another workspace crate, and require that
everything reached is either listed or recorded as an exclusion with a reason.
An exclusion with an empty reason is a parse error, because an exclusion
without a reason is just a shorter list.

Following `mod` declarations means every module of a listed crate has to be
decided about, one way or the other. That is the price and it is the point:
the file is now a map of what the audio path is and what it deliberately is
not, and adding a module to a listed crate makes someone say which.

## The one exclusion that matters

`crates/client-linux/src/delaylog.rs` reads a settable clock, once, and writes
it into the header of a saved log so that a human reading that file can tell
which evening it came from. No monotonic number can do that.

It is safe because of what the unit is rather than what it promises: it only
ever writes, nothing on the path reads anything back from it, and no decision
anywhere depends on the value. Every quantity the log grades comes from the
client's own monotonic timeline and from the device.

Recording it, with that reason, in the same file as the list is what makes the
exclusion reviewable instead of invisible.

## What the check does not catch, said out loud

It scans source text. A clock read reached through a macro it cannot see, or
through a name it does not know, is not caught. What it does catch is every
ordinary way of reading a settable clock in Rust, including the libc and
`chrono` spellings that would arrive with the first dependency, and every way
of quietly moving one off the list.

A cleverer check would be a compiler plugin, and a compiler plugin is a
dependency. The trade is recorded rather than hidden.

## Why the demonstrations are committed rather than branched

Two demonstrations prove the check is not vacuous: one introduces a settable
clock read into a listed unit, and one adds a first-party unit under a listed
one without listing it. Both are expected to turn the check red.

They live in `crates/audio-path/tests/audio_path.rs`, each copying the source
tree into a scratch directory, mutating the copy, and asserting the scan goes
red. A branch would have proved the same thing once, for whoever was told the
branch name; a committed test proves it on every run, for everyone, and
survives the merge.

## Consequences

- Adding a module to a crate on the path fails the suite until someone decides
  whether it is on the path.
- Third-party dependencies are outside the check by definition. This
  repository has none, and a green suite should never depend on auditing code
  it does not own.
- `crates/sync` is not on the list. It is a pure library with no I/O and it is
  not wired to audio yet. The phase that wires it in adds it.

## Revisit when

The first external dependency lands and the question of auditing code this
repository does not own becomes real, or the servo joins the audio path and
`crates/sync` has to be listed.
