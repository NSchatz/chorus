# The thread-inventory flake: what it was, and repeat runs after the fix

Date: 2026-08-24
Change under measurement: the `chorus-hostctl` thread inventory stops treating
every failed read of a listed thread as "that thread is gone", and stops
trusting a single pass of `/proc/self/task` to be complete (umbrella spec
`S0019-chorus-hostctl-thread-loss`).

## The machine

```
kernel                    Linux 6.12.90+deb13.1-amd64 x86_64
CPUs visible              56
toolchain                 rustc 1.98.0, x86_64-unknown-linux-gnu
ulimit -r (RLIMIT_RTPRIO) 0   (so no test here takes a real-time policy)
/dev/snd                  absent
container                 a disposable Linux container, not the deploy target
```

One machine, as the criterion asks. It is not the Proxmox host and it is not a
machine with a sound card, so nothing here says anything about what needs
either; these are about the suite that needs neither.

## The flake, reproduced and named

The work was opened on a report of "1 fail in 27 full-suite runs", with no
detail and no artefact in this repository. The flake was reproduced here and it
is real.

```
cargo test -p chorus-hostctl --lib     # 200 runs, before the fix
```

| | |
|---|---|
| runs | 200 |
| runs that failed | 7 |
| observed rate | 3.5% |
| the test that failed, every time | `tests::the_reported_policy_of_this_thread_matches_proc` |
| how it failed | `this thread is in /proc/self/task` |

That assertion says the thread doing the asking appears in its own thread
inventory. It was failing.

The cause was measured rather than guessed. The failing case was instrumented
to dump, at the moment of failure, the listing the enumeration had used, a
second listing taken immediately after, and whether the thread's own `stat` was
readable. Four captures, all identical in shape:

```
DIAG tid=394018 in_second_listing=true listing=["394013", "394018", "394019"]
     facts=[394013] stat_now=Ok(328)
DIAG tid=394690 in_second_listing=true listing=["394685", "394690", "394691"]
     facts=[394685, 394691] stat_now=Ok(328)
```

Read that carefully: `stat_now=Ok(328)` says the thread's scheduling record was
readable the whole time, and `listing=[...]` taken microseconds later contains
it. The thread existed, was readable, and was **never listed**. In the second
capture the listing lost a thread from the middle of the range while keeping
the ones on either side of it.

That is `/proc/<pid>/task` being generated as it is read. Its iteration is not
atomic: a thread created or destroyed during the scan moves the kernel's cursor
and can cost a different, live thread its entry. The test binary runs its six
tests in parallel and they finish at different times, which is exactly the
churn that provokes it.

So the enumeration had two ways to lose a live thread, not one. The reported
defect (`Err(_) => continue`, which turned every failed read into a silent
omission) was real and is fixed. It was not the one firing here. **A listing
this code trusted to be complete was not complete**, and a thread lost there
looks exactly like a thread that does not exist.

## The fix these runs measure

The listing is taken again until two consecutive passes agree, up to five
passes, and the entries are unioned rather than intersected, so a pass that
missed a thread cannot cost that thread its place. The thread doing the asking
is added unconditionally, because that is the one thread whose existence needs
no listing to establish.

```
cargo test -p chorus-hostctl --lib     # 200 runs, after the fix
```

| | |
|---|---|
| runs | 200 |
| runs that failed | 0 |
| observed rate | 0% |

Same command, same machine, same session, 200 runs each side.

## Thirty consecutive runs of the full workspace suite

```
make test          # which is exactly: cargo test --workspace
```

Thirty consecutive runs, back to back in one shell loop, nothing changed
between them, on the tree at the head of branch
`sdd/S0019-chorus-hostctl-thread-loss`.

| | |
|---|---|
| runs | 30 |
| runs that failed | 0 |
| test-binary results across the 30 runs | 900, all `ok` |
| failing assertions, panics or build errors | 0 |
| wall time of a single run | about 7 s after the first build |

Counted mechanically from the concatenated logs: `grep -c "Leaving directory"`
gives 30 runs, `grep -c "test result: ok"` gives 900, and
`grep -cE "Error [0-9]+|error\[|panicked|FAILED"` gives 0.

## What this does and does not establish

**A clean streak bounds the flake rate. It does not prove the flake gone.**
Thirty green runs are consistent with a fault that fires once in a hundred runs,
or once in a thousand. Treating "we could not make it happen again" as "we fixed
it" is the mistake this section exists to refuse. With the rule of three, zero
failures in 30 independent runs puts the per-run failure probability below
roughly 10% at 95% confidence, and zero in the 200 targeted runs above puts it
below roughly 1.5%. Those are weak bounds. They are the bounds these runs
bought, and a stronger one needs more runs, not more adjectives.

Note what the streak alone could not have told anyone: 30 clean runs of the full
suite were recorded **before** the listing race was found, because at a 3.5%
per-run rate a clean run of 30 happens about one time in five. The streak was
not evidence then and it is not evidence now. What makes the case here is the
targeted 200-run comparison above, the captured diagnosis of the mechanism, and
that both defects now have tests which go red when the old behaviour is put
back:

- `crates/hostctl/tests/thread_inventory.rs`, nine tests, one per failure class
  the enumeration now distinguishes. Reverting `thread_is_gone` to accept every
  errno turns three of them red; reverting the omissions to `continue` and the
  name to `unwrap_or_default()` turns five of them red.
- The listing race specifically: inside
  `inventory_lists_every_live_thread`, a modelled listing that loses one live
  thread on its first pass and tells the truth afterwards. Setting
  `LISTING_PASSES` back to 1 turns it red (`left: 2, right: 3`).

## What is still unmeasured

Everything that needs a granted rtprio ceiling above zero. No thread on this
machine can take a real-time policy, so none of these runs exercises the
inventory against a process that actually has a real-time thread in it. See
`docs/verification-record.md`.
