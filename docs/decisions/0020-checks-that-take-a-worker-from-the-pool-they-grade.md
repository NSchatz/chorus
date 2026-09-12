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
- **No check hands a server a port nobody is holding.** Every server these checks
  start is started on `127.0.0.1:0` and asked where it landed; the one check that
  needs an address of its own holds it across the whole run. The repetition count
  is floored at fifty whether it comes from the committed constant or from
  `CHORUS_DETERMINISM_REPETITIONS`.

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

### Why the ports are the server's and not the check's

The worker pool is not the only thing these four checks share. Each of them
starts a server, and the way to give a server a loopback address is to ask the
kernel for a free port. Doing that in the TEST process means binding
`127.0.0.1:0`, reading the port, closing the socket and passing the number to the
child: between the close and the child's bind the port is held by nobody, and one
of these checks deliberately holds a loopback port of its own while the others
run. A port taken in that interval makes the server exit on a bind it could not
make, which reaches the check as a server that never said it was listening. That
is a red run about ports, on an unchanged build, for a question that is about
threads.

The interval is the defect, not its width, so it is removed rather than narrowed.
`chorus-server` already resolves `127.0.0.1:0` for both of its sockets and prints
where each one landed - `control listening on=` through `ControlPlane::address()`
and `listening on=` through the audio listener's `local_addr()` - so the process
that binds a socket can be the process that holds it, and there is nothing to
hand over. Retrying a failed start on fresh ports would have been the other
answer; it keeps the interval and adds a path that only runs when the race that
should not exist has happened.

The check that needs an address of its own is the unbindable-control-address one,
which must be told an address something else holds. It holds both of its
addresses - the taken control address and the audio address - from before the
server starts until after it has exited. Holding the audio address is also a
better assertion than the one it replaces: "the audio port is still free
afterwards" could be falsified by any other process, while a server that had got
as far as an audio listener on a held port would have been refused on it and
would have said so by name.

### Why the repetition floor binds the override too

`CHORUS_DETERMINISM_REPETITIONS` exists to run MORE repetitions than the
committed count, and the floor applies to it for the same reason it applies to
the constant: the number of repetitions is what the claim is made of, so an
override under the floor would be a way to report the determinism claim
satisfied out of one lucky scheduling. `require_soak_window` in `tools/lib.sh`
already holds `CHORUS_SOAK_SECONDS` to exactly this rule - "a shorter run is a
different claim and this will not make it" - and a check whose claim can be
shrunk from the environment is a check with a way around it. Running the checks a
handful of times while working is running the checks, not making the claim, and
`make one` is how that is done.

### Why the repetition count is committed rather than chosen by the script

It is a constant a check rests on, so it lives in `config/verification.conf` with
the others, and `tools/control-determinism.sh` refuses to run at all when it is
absent rather than picking a number nobody committed to. A check that invented
its own repetition count could be made to pass by making that number one.

## What this does not decide

Anything about the shipped control plane. No behaviour of `chorus-server` is
changed by it: the pool is still fixed, the refusal is still `503` by name, and
the observable the checks synchronise on - the subscriber count in
`GET /api/report` - was already published for AC-11's report half. The two
addresses the checks now read back are already printed by the shipped binary for
the same reason a person reading a log needs them; nothing was added to it, and
`127.0.0.1:0` is an address it already accepted.
