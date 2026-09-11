# 0020: checks that take a worker from the pool they grade

- Status: decided
- Recorded by: PRODUCT-6, AC-12
- Implemented in: `crates/server/tests/control_thread_population.rs`,
  `tools/control-determinism.sh`, `config/verification.conf`
  (`control_determinism_repetitions`)

## The question

AC-12 is about the control plane's FIXED worker pool: every thread exists before
the scheduling report, and no subscriber may cause another one. The only place
that property is visible from is a connection, and every connection is served by
one of those same fixed workers. So the check competes with itself for the thing
it is checking: hold three event streams open on a four-worker plane and every
command has one worker left to be served by, and the worker that served the last
one has to have handed its slot back before the next connection arrives.

A connection that arrives while all of them are busy is answered
`503 Service Unavailable` and closed. That answer is correct, and the checks
grade it deliberately elsewhere. The question is what a check should do when it
meets that answer at a point where it required service.

## Decision

**Establish service before judging an assertion, provoke the pool rather than
wait on it, and never buy determinism with headroom.**

- Every request these checks require to be SERVED is retried past a busy-worker
  refusal until it is served, within a bounded wait. No assertion is judged on
  an answer the control plane did not serve.
- Where the wait runs out, the check FAILS naming the refusal verbatim, the
  worker ceiling in force and how many streams were being held. It does not
  retry without limit, does not block for ever, and never passes.
- An attachment is established by the served event stream - the status line, the
  content type and the opening state - and not by bytes coming back, because a
  refusal is bytes coming back too.
- The waiting is synchronised on `GET /api/report`'s subscriber count, which the
  control plane already publishes, rather than on a sleep.
- A worker still holding a stream whose subscriber has gone is PROVOKED with a
  command that is applied: fanning out is a write, and writing is the only way
  that end learns its peer left.
- The claim that this is deterministic is made by `make
  verify-control-determinism`, which runs the checks for the repetitions
  `config/verification.conf` commits to, on one build, on two CPUs, and runs them
  again against a plane too small to serve them and requires that to go red.

## Reasoning

### Why not raise the worker ceiling

Four workers with three streams held is not an awkward configuration these
checks stumbled into; it is the configuration that puts the pool under the
question. Raising the ceiling until commands always find a free worker would
make the checks pass more often by asking less: nothing would then be holding
the pool at all, and the population being asserted about would be a population
nothing had stressed. The ceiling is an input to the property, not a knob.

### Why not serialize the checks

`--test-threads=1` would reduce the contention that makes the interleaving
visible, and would do it by running the checks in a way nothing else runs them.
The deployment runs a busy control plane; so does CI, where several of these
servers are alive at once. A check that is only correct when nothing else is
happening is not a check on this system.

### Why not sleep

A sleep is a guess about a scheduler, and it is wrong in the two directions at
once: too short on a loaded runner, and wasted on an idle one. The subscriber
count at `GET /api/report` is the control plane saying what is attached, and a
subscriber leaves that count at the same moment the worker holding its stream is
done with it, which is exactly the fact these checks are waiting for.

### Why provoke rather than wait

A worker parked on an event stream waits for something to send. If its
subscriber has gone, nothing tells it so until it writes, and left alone it will
not write until the keepalive interval - fifteen seconds - is up. Waiting that
out would be fifteen seconds of nothing per check. A command that is applied is
fanned out to every subscriber, so provoking one turns a fifteen-second wait into
a write that fails at once and a slot that comes back.

### Why the repetition count is committed rather than chosen by the script

It is a constant a check rests on, so it lives in `config/verification.conf` with
the others, and `tools/control-determinism.sh` refuses to run at all when it is
absent rather than picking a number nobody committed to. A check that invented
its own repetition count could be made to pass by making that number one.

## What this does not decide

Anything about the shipped control plane. No behaviour of `chorus-server` is
changed by it: the pool is still fixed, the refusal is still `503` by name, and
the observable the checks synchronise on - the subscriber count in
`GET /api/report` - was already published for AC-11's report half.
