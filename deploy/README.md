# deploy

The container the chorus server runs in, and the contract the host grants it.

## The two files, and why neither is useful alone

`Dockerfile` builds the server. `run-server.sh` grants it a real-time priority
ceiling and a locked-memory allowance. A Dockerfile cannot grant either: they
arrive per container, on the run command, which is why the scheduling contract
lives in a shell script beside the image rather than inside it.

## What the server does with what it is granted

- Reads `RLIMIT_RTPRIO` and asks for a priority no greater than it. Never more.
- Applies `RLIMIT_RTTIME` to itself before any real-time thread does audio
  work, so the bound is in force from the first instruction rather than from
  whenever the process got round to it.
- Reads `RLIMIT_MEMLOCK`, and either locks its audio-path memory or says why it
  did not.
- Reports all of it, and compares the report against `/proc/self/task` so that
  a thread running real-time without being reported is a hard failure rather
  than a surprise.

## What it does when the host grants nothing

Exits non-zero, naming the limit it read and what it wanted. That is the
point: a server that quietly ran as an ordinary process while the deployment
believed it had a real-time policy would be worse than one that refused, and
the failure would only show up as occasional audible glitches.

Two escapes exist, both explicit, and both make every subsequent status report
say so:

- `--allow-non-realtime` starts without a real-time policy.
- `--allow-unlocked-memory` starts without locked memory.

## Not deployed here

This tree does not deploy onto anyone's production host and does not touch any
inventory, service definition, reverse proxy or secret. It is the container's
own scheduling contract, runnable on any Linux host that can hand a container
an `rtprio` ceiling. See `docs/sound-2.md` for which half of the phase's
outcome that leaves for later.

## Checking the contract holds

```
./deploy/run-server.sh &          # in a container with the limits granted
./tools/host-contract.sh          # inside that container
./tools/spin-test.sh              # inside that container
```

Both entry points exit non-zero naming the missing prerequisite when the host
granted no real-time priority, rather than reporting themselves green.
