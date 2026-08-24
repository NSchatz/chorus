# 0012: the thread inventory is complete, or it is an error

- Status: decided
- Recorded by: umbrella spec S0019-chorus-hostctl-thread-loss
- CLAUDE.md working agreement 1 (cheap decisions get made and noted); the host
  contract's third property in `crates/hostctl/src/lib.rs`

## Decision

Enumerating this process's threads has exactly three outcomes, not two:

1. a complete inventory;
2. a complete inventory that names how many listed threads had **exited**
   before they could be read, counted rather than hidden;
3. an **error** naming the thread and the reason.

Everything that is not "that thread stopped existing" is outcome 3. The
undeclared-real-time-thread question is answered from a `ThreadInventory`,
which only outcomes 1 and 2 produce, so it cannot be answered from a list that
lost a thread on the way.

## Why

The contract's third property is that which threads are real-time is on the
record, checked against `/proc/self/task` because that is the kernel's own
answer. The enumeration behind it treated every failure to read a listed thread
as evidence that the thread was gone: a catch-all `Err(_) => continue` whose
comment claimed only the exited case, an `unwrap_or_default()` that turned a
failed name read into a thread named `""`, and a record that could not be
interpreted skipped with no signal.

Each of those shortens the list. A short list makes
"no thread is real-time without being reported" answer **clean**, because the
thread that would have contradicted it is the one that was dropped. That is the
worst shape a safety check can have: it fails in the direction of looking fine,
and the check that was supposed to catch the problem is the thing reporting
success.

An exited thread is different in kind, and only in kind. It genuinely is not a
thread any more, so omitting it is correct. It is counted anyway, because
"nothing was dropped" and "something was dropped and it had exited" are
different facts and a caller reading a report is entitled to tell them apart.

## The listing is not trusted on one pass either

There were two ways to lose a live thread here, not one, and only the first was
in the report that opened this work.

`/proc/<pid>/task` is generated as it is read and its iteration is not atomic:
a thread created or destroyed during the scan moves the kernel's cursor and can
cost a **different, live** thread its entry. Measured on this repository's own
suite before the fix, a single listing lost a live thread on 7 runs out of 200,
and the thread it lost was the one doing the asking, whose `stat` was readable
throughout and which a listing taken microseconds later contained. The captures
are in `docs/measurements/hostctl-thread-inventory-repeat.md`.

A thread lost in the listing is indistinguishable from a thread that does not
exist, so it lands in the same place as the defect above: the check reports
clean because the contradicting thread was dropped.

So the listing is taken again until two consecutive passes agree, up to
`LISTING_PASSES`, and the entries are **unioned**, never intersected: a pass
that missed a thread must not be able to cost it its place, and an entry in the
union that has since exited is the vanished case, which is counted. The thread
doing the asking is added unconditionally, because it is the one thread whose
existence needs no listing to establish. The common case costs two readdirs of
a directory with a handful of entries in it.

This does not make the listing perfect. If every pass missed the same thread,
the union would miss it too. It moves a measured 3.5% per-run loss to zero
observed in 200 runs, and the bound that buys is stated honestly in the
measurement file rather than rounded up to "fixed".

## Why an empty inventory is an error

The thread doing the asking is itself a thread. An enumeration that comes back
with nothing has not discovered a process with no threads; it has discovered
that its own answer is wrong. Returning an empty list would let the
undeclared-real-time check pass trivially, which is the same failure as above
with the volume turned up.

## What it costs

A host on which this enumeration now fails will see the server refuse where it
previously started, with exit code 7 and the reason named. That is the intended
direction: a server that cannot check its own scheduling contract should say so
rather than start while claiming the contract holds, which is the fourth
property in the same module ("a refusal is a refusal").

## Why the seam exists

`TaskSource` is the interface the enumeration reads the kernel through, and
`ProcTaskSource` is the only implementation that ships. It exists so the
failure classes above are testable: a test cannot make the kernel refuse to
read one thread of its own process, cannot arrange for a record to be
uninterpretable, and cannot make a thread vanish at a chosen microsecond
without becoming a coin toss. Injecting the errno the kernel would have
returned is deterministic, needs no privilege, and the enumeration cannot tell
the difference. Tests that depend on the machine they run on are how a suite
acquires the flakes this work was opened about.
