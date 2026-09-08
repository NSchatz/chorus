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

## The base image pins, and how to move one

Both `FROM` lines in `Dockerfile` carry a tag AND the digest that tag resolved
to. The tag is what a human reads; the digest is what actually resolves, and it
is the only half that makes two builds of this file the same build. The umbrella
records the rule as clauses P1 and P2 of `documentation/pinning-conventions.md`.

| stage | tag | digest | resolved |
|---|---|---|---|
| build | `docker.io/library/rust:1.74-slim-bookworm` | `sha256:53596c66027523b2289c6e7c96bff119416be22d2cf52734b4962e13371c54cf` | 2026-09-08 |
| runtime | `docker.io/library/debian:bookworm-slim` | `sha256:88200866dfff7ea7f5cbcb6ec7c8a701889efe6fe859fe64d6990e4b07ea4171` | 2026-09-08 |

Each digest is the multi-architecture index digest the tag resolved to, which is
what a `FROM` line takes.

To re-resolve either one, ask the registry what the tag points at TODAY and put
the answer in `Dockerfile`. The two commands, exactly:

```
curl -fsSL https://hub.docker.com/v2/namespaces/library/repositories/rust/tags/1.74-slim-bookworm | jq -r .digest
curl -fsSL https://hub.docker.com/v2/namespaces/library/repositories/debian/tags/bookworm-slim | jq -r .digest
```

Either one, from a machine with a Docker daemon and no `jq`:

```
docker buildx imagetools inspect docker.io/library/rust:1.74-slim-bookworm --format '{{.Manifest.Digest}}'
docker buildx imagetools inspect docker.io/library/debian:bookworm-slim --format '{{.Manifest.Digest}}'
```

`make verify-pinning` then confirms the new value has both halves. It resolves
nothing itself, so it is the same check on a machine with no network.

### What each pin costs, said out loud

`rust:1.74-slim-bookworm` is a version tag that was last pushed on 2023-12-19
and has not moved since: pinning it changes almost nothing, and the digest is
insurance against the day it does.

`bookworm-slim` is a ROLLING tag. Its publisher moved it on 2026-08-25 and will
move it again, which is precisely why the digest and not the tag is the
reference here - and it is also the cost, because pinning it freezes Debian's
security refreshes at that date until someone moves this pin on purpose. That is
the trade P1 and P6 accept in exchange for a build that reproduces, and this
section is the mitigation: moving the pin is the two commands above, not an
archaeology exercise.

There is no scheduled job watching these for rot, deliberately (P8). A stale pin
shows up when a build fails, and the failure names the pin.

## The action pin

`.github/workflows/ci.yml` pins `actions/checkout` to the commit
`11d5960a326750d5838078e36cf38b85af677262`, with `# v4` beside it so a reader
can tell which release that is (P3). The `v4` tag is moved by its publisher
under every consumer that wrote `@v4`; when this pin was taken it resolved to a
commit dated 2026-07-16 that had changed the fork-checkout guard. To move it:

```
curl -fsSL https://api.github.com/repos/actions/checkout/commits/v4 | jq -r .sha
```

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
