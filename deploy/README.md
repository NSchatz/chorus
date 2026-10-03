# deploy

The container the chorus server runs in, and the contract the host grants it.

## The files, and why the image is not useful alone

`Dockerfile` builds the server. `run-server.sh` and `compose.yaml` grant it a
real-time priority ceiling and a locked-memory allowance. A Dockerfile cannot
grant either: they arrive per container, on the run command or in the compose
service, which is why the scheduling contract lives beside the image rather
than inside it.

Both run the deployable shape (R14, decided 2026-09-29 by the owner, K24, K34):
host networking, so multicast DNS can work at all and endpoints reach the audio
port with no NAT hop; the control plane on port 4020; the zone state persisted
at `/var/lib/chorus`; `rtprio` 20, `memlock` 64 MiB and one CPU.

- `run-server.sh` passes `--advertise`, which `CHORUS_ADVERTISE=0` turns off on
  a host where another responder holds UDP 5353 (audit L-3).
- `compose.yaml` leaves `--advertise` off, because the homelab's mDNS reflector
  holds UDP 5353 there and the server would refuse by name (exit 9); endpoints
  use their static fallback address.

The homelab runs a copy of `compose.yaml` in the homelab repo's own conventions
(a bind mount instead of the named volume, the control plane bound to the proxy
bridge only, a healthcheck, the image pinned by digest), opened there as a PR
that the owner merges and applies (K28). Nothing here deploys anything.

## Identity and adoption (protocol v2)

Every audio connection is an encrypted protocol v2 session. The server keeps its
long-term key and the endpoints it has adopted beside the zone state:
`/var/lib/chorus/server.key` (created on first start, mode 0600, never logged)
and `/var/lib/chorus/adopted-endpoints`. Both deployments pass `--state-file`,
whose directory is the identity directory unless `--identity-dir <dir>` says
otherwise, so the named volume (or the homelab's bind mount) keeps the server's
key and every pin across restarts and image upgrades. `--server-id <id>` (default
`chorus-server`) is the id endpoints pin that key to. A server with no
`--identity-dir` and no `--state-file` refuses to start (exit 2) unless it is
given `--ephemeral-identity`, which is for tests and throwaway runs only.

An endpoint presenting a different key under an id already adopted is refused
and logged as `endpoint key changed id=<id> pinned=<fp> offered=<fp>; refused`;
a protocol v1 endpoint is refused as `client refused ... reason=protocol-v1`.

A Linux endpoint (`chorus-client`) needs `--identity-dir <dir>` (its
`endpoint.key` and the server pins in `server-pins`) and a stable
`--endpoint-id` (default: its `--endpoint` name), because the server pins the
endpoint's key to that id. `--ephemeral-identity` is the same escape hatch for
tests.

## The healthcheck

The released image is distroless: no shell and no curl. `chorus-server
--health-check <addr:port>` is the probe a compose `healthcheck:` runs instead:
it starts nothing, sends `GET /api/state` to the control plane, and exits 0 on
a 200 answer and 1 otherwise (Docker's healthcheck contract). `make image`
runs it against the unpacked image, both ways.

## The daemonless image (`make image`)

`make image` builds the same server as an OCI image tarball without a container
daemon (`tools/image.sh`): a static musl `chorus-server` on the digest-pinned
`gcr.io/distroless/static-debian12:nonroot`, assembled with umoci, written to
`target/image/chorus-server-oci.tar`. Its test unpacks the tarball rootless,
runs `chorus-server --help`, starts the unpacked server and reads
`GET /api/state` back, and checks that this Dockerfile's build context compiles.
Pushing the tarball to a registry is the owner's step, never the build's.

## The Soloist receiver image (`make soloist-image`) and its compose file

`soloist/` is the second image and its reference deployment: one Spotify Soloist receiver
per container (PipeWire, WirePlumber and `chorus-soloistd`), run as a pool of identical
replicas beside chorus-server. `docs/soloist.md` ("The image" and "Running the receivers")
is the page; `docs/decisions/0131-the-chorus-soloist-image.md` is the record. In short:

- `make soloist-image` (`tools/soloist-image.sh`) builds
  `target/image/chorus-soloist-oci.tar` with no container daemon: the digest-pinned
  `debian:trixie-20260918-slim`, the 60 Debian packages of `soloist/debian-packages.pins`
  fetched from snapshot.debian.org and held to their sha256, and a static `chorus-soloistd`.
  Its test unpacks the tarball and runs the image's own PipeWire and WirePlumber under the
  image's supervisor. Warm, it needs no network.
- **The image holds no Spotify software.** Soloist is proprietary: the owner places the
  binary and the API key on the host, and `soloist/compose.yaml` mounts both read-only.
  `make soloist-lists` holds both images and the release to that (conventions rule 24).
- `soloist/compose.yaml` is the reference form: `deploy.replicas`, an external macvlan
  network (named, not defined here), the receiver directory shared with chorus-server,
  `read_only`, `cap_drop: [ALL]`, `no-new-privileges`, limits per receiver, and the
  image's own `--health-check`.
- `soloist/server.compose.yaml` is the server's half, an override of `compose.yaml` that a
  host running the receivers adds (`-f compose.yaml -f soloist/server.compose.yaml`): the
  receiver directory mounted, and `--slots`, `--soloist-dir` and `--soloist-receivers` on the
  command. Without it the server runs no Soloist code. `make image` holds the override's
  command to `compose.yaml`'s and starts the image's server with those flags.
- `soloist/THIRD-PARTY-NOTICES.md` is the head of the notices the image carries; the build
  appends the package table.

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
is the only half that makes two builds of this file the same build. chorus's own
pinning rule replaces the retired umbrella clauses (K18, R13).

| stage | tag | digest | resolved |
|---|---|---|---|
| build | `docker.io/library/rust:1.98.1-slim-bookworm` | `sha256:ff521445a372125ed4f76e1453a1f8098f2d05332d1601d30db1c1f62757e730` | 2026-09-30 (goal 2, B-16) |
| runtime | `docker.io/library/debian:bookworm-slim` | `sha256:88200866dfff7ea7f5cbcb6ec7c8a701889efe6fe859fe64d6990e4b07ea4171` | 2026-09-08 |

Each digest is the multi-architecture index digest the tag resolved to, which is
what a `FROM` line takes.

To re-resolve either one, ask the registry what the tag points at TODAY and put
the answer in `Dockerfile`. The two commands, exactly:

```
curl -fsSL https://hub.docker.com/v2/namespaces/library/repositories/rust/tags/1.98.1-slim-bookworm | jq -r .digest
curl -fsSL https://hub.docker.com/v2/namespaces/library/repositories/debian/tags/bookworm-slim | jq -r .digest
```

Either one, from a machine with a Docker daemon and no `jq`:

```
docker buildx imagetools inspect docker.io/library/rust:1.98.1-slim-bookworm --format '{{.Manifest.Digest}}'
docker buildx imagetools inspect docker.io/library/debian:bookworm-slim --format '{{.Manifest.Digest}}'
```

Check by eye that the new value has both halves: the umbrella-derived pinning
check that used to confirm it was retired on 2026-09-30 (K18, R13).

### What each pin costs, said out loud

`rust:1.98.1-slim-bookworm` is a patch-version tag; whether its publisher
re-pushes it is not checked here (ASSUMED that it may), so the digest is what
keeps two builds the same, and moving it is a deliberate change.

`bookworm-slim` is a ROLLING tag. Its publisher moved it on 2026-08-25 and will
move it again, which is precisely why the digest and not the tag is the
reference here - and it is also the cost, because pinning it freezes Debian's
security refreshes at that date until someone moves this pin on purpose. That is
the trade digest pinning accepts in exchange for a build that reproduces, and this
section is the mitigation: moving the pin is the two commands above, not an
archaeology exercise.

There is no scheduled job watching these for rot, deliberately. A stale pin
shows up when a build fails, and the failure names the pin.

## The action pin

`.github/workflows/ci.yml` pins `actions/checkout` to the commit
`11d5960a326750d5838078e36cf38b85af677262`, with `# v4` beside it so a reader
can tell which release that is. The `v4` tag is moved by its publisher
under every consumer that wrote `@v4`; when this pin was taken it resolved to a
commit dated 2026-07-16 that had changed the fork-checkout guard. To move it:

```
curl -fsSL https://api.github.com/repos/actions/checkout/commits/v4 | jq -r .sha
```

## Not deployed here

This tree does not deploy onto anyone's production host and does not touch any
inventory, service definition, reverse proxy or secret. It is the container's
own scheduling contract, runnable on any Linux host that can hand a container
an `rtprio` ceiling. The homelab deploy is a PR in the homelab repo that agents
never merge (K28); releases and the registry push are in `docs/release.md`. See `docs/sound-2.md` for which half of the phase's
outcome that leaves for later.

## Checking the contract holds

```
./deploy/run-server.sh &          # in a container with the limits granted
./tools/host-contract.sh          # inside that container
./tools/spin-test.sh              # inside that container
```

Both entry points exit non-zero naming the missing prerequisite when the host
granted no real-time priority, rather than reporting themselves green.
