# 0131: the chorus-soloist image is built without a daemon from sha256-pinned Debian packages unpacked by dpkg-deb onto a digest-pinned trixie base, with a static chorus-soloistd, no Soloist file, a probe that connects to nothing, and a listings check that fails on any Soloist file in an image or a release

- Status: accepted (goal 17, 2026-10-03)
- Decided by: the owner for what is built (P7 Option C: a pool of receiver containers, the
  Soloist binary and key the owner's, never shipped); the goal (program section 21, line A)
  inside the coordinator's goal-17 design envelope (section 5), track
  `chorus-g17/soloist-image`, for everything else. Every number below that is not cited is
  chorus's own choice and is said to be
- Implemented in: `tools/soloist-image.sh`, `tools/soloist-image-test.py`,
  `deploy/soloist/debian-packages.pins`, `deploy/soloist/THIRD-PARTY-NOTICES.md`,
  `deploy/soloist/compose.yaml`, `tools/soloist-lists.py`, `tools/release.sh`, `tools/gate.sh`,
  `tools/conventions/check-pins.sh`, `crates/soloistd/src/health.rs`; described in
  `docs/soloist.md` ("The image", "Running the receivers"), `docs/release.md`,
  `docs/conventions.md` (rule 14's table, rule 24)

## Context

ADR 0130 built the receiver's supervisor. A receiver container also needs PipeWire and
WirePlumber (Soloist plays into PipeWire and nothing else, and without a session manager a
client is never linked to the sink: goal 17's PipeWire probe), on a Debian 13 base (P7). The
server image's method, a static binary inserted onto distroless (`tools/image.sh`), has no step
that installs a package, so this image needed a method of its own, and the program's line A
needs proof, printed, that no Soloist file is in any image, release or the repository.

## What was read

All on 2026-10-03.

- `docs/proposals/P7-spotify-soloist.md`, ADR 0130, `docs/soloist.md`, ADR 0122,
  `tools/image.sh`, `tools/release.sh`, `deploy/Dockerfile`, `deploy/compose.yaml`,
  `tools/gate.sh`, `tools/conventions/check-pins.sh`, `check-conventions.sh`.
- Goal 17's research: the PipeWire probe (PipeWire 1.4.2 and WirePlumber 0.5.8 extracted with
  `dpkg-deb -x` run headless; its 54 package files and their sha256), the homelab digest
  (what a compose service there must look like), the code survey.
- The registry: `crane digest docker.io/library/debian:trixie-slim` and
  `:trixie-20260918-slim` (the same index digest), `crane manifest` of it, and the amd64
  image's own files (`/var/lib/dpkg/status`: 78 packages; `/etc/ld.so.conf.d`;
  `/etc/debian_version`: 13.7).
- snapshot.debian.org at `20260918T000000Z`: the suites' package indexes, read by `apt-get`
  with private state; the 72 package files; a `HEAD` of each of the 47 source packages'
  `.dsc` (all answered 200).
- The packages' Debian metadata: each control file and `usr/share/doc/<package>/copyright`.
  The licence texts the base image carries in `/usr/share/common-licenses` (LGPL-2.1
  sections 2, 4 and 6; GPL-2 sections 2 and 3; GPL-3 section 6).
- Docker Compose v2.40.3: `docker-compose -f deploy/soloist/compose.yaml config` parses the
  file (no daemon, nothing started).
- No GPL or LGPL source was opened: packages were unpacked and run, and their metadata read.
  No Soloist binary or archive was downloaded or run.

## Decision

1. **Daemonless, from pinned package files.** `tools/soloist-image.sh` pulls the base by digest
   with crane, unpacks each pinned `.deb` with `dpkg-deb -x` into a staged tree, and inserts
   the tree with umoci: the tools and the shape of `tools/image.sh`. The pin list
   `deploy/soloist/debian-packages.pins` is one line per package file: name, exact version,
   architecture, sha256, size, source package and version, and the path at
   snapshot.debian.org; the build fetches exactly those from the snapshot timestamp the file
   names, keeps them in a cache keyed by sha256, and verifies every file against its sha256
   and size on every build, cached or fetched. Why this and not a Dockerfile with
   `apt-get install`:
   - the same inputs give the same files: `apt-get install` resolves at build time against
     whatever the mirror holds that day, and pinning `pkg=version` for 60 packages against a
     live mirror fails as soon as a point release replaces one. A snapshot URL plus a sha256
     per file does not move;
   - it is the method the server image already uses, so one way of building, testing
     (unpack, run the unpacked files) and pushing (the OCI layout, unchanged) covers both;
   - the gate and CI build it with no container daemon and no privilege, like every other
     gate step (conventions rule 10), so the image is built and tested on every full gate
     rather than only where a daemon exists;
   - what the image holds is a reviewed, committed list, which is what line A's listing prints.
   What it costs: resolving the list is a manual step (the file's header gives the commands),
   and moving a pin means resolving again; a Dockerfile would resolve by itself.

2. **No `deploy/soloist/Dockerfile`.** The server has a daemon-built form beside the
   daemonless one because the Dockerfile came first. A second form here would be a second,
   different image (apt would run maintainer scripts and bring the excluded packages back)
   that the gate cannot build. One form, tested.

3. **The base is `debian:trixie-20260918-slim` by index digest**, and the snapshot timestamp
   is the same day, `20260918T000000Z`: the base image's own annotation gives
   `org.opencontainers.image.created` 2026-09-18T00:00:00Z, so the packages are resolved
   against the archive state the base was built from, and against the base's own package
   list (apt was given the base's `/var/lib/dpkg/status`). trixie-slim over distroless
   because Soloist is a dynamically linked program whose needs are not documented, and a
   Debian trixie container is where a user reports running it (a LEAD, Soloist issue 1).

4. **The packages: the Depends closure, less twelve.** `pipewire`, `pipewire-bin` and
   `wireplumber` with their Depends (no Recommends) over the base's 78 packages are 72
   packages. The probe's 54 files are all among them with identical sha256; the base holds
   none of the 54 and lacks 18 more that the probe's host already had. Twelve are left out:
   `systemd`, `systemd-sysv`, `libsystemd-shared`, `libpam-systemd`, `libapparmor1`, `dbus`,
   `dbus-bin`, `dbus-daemon`, `dbus-session-bus-common`, `dbus-system-bus-common`,
   `dbus-user-session` and `adduser`. They arrive only because `wireplumber` depends on a
   D-Bus session bus, which depends on systemd's user session. A receiver runs PipeWire and
   WirePlumber with D-Bus off and no init system (the configuration `chorus-soloistd`
   writes), so neither daemon is ever started, and an init system's files in an image that
   never boots are surface with no use. `libdbus-1-3`, the library the modules link, stays.
   The image test proves the exclusion harmless: every `NEEDED` library of every program and
   library in the 60 staged packages resolves in the image.
   The 60 are staged whole, unmodified. That keeps modules a receiver never loads (ALSA,
   Bluetooth, JACK, FFADO, ROC) and their libraries: about 54 MB unpacked. Deleting files
   would make the packages modified ones, which the notices would then have to describe;
   the size is not worth that.

5. **No maintainer script runs, and what that means.** `dpkg-deb -x` unpacks the files and
   runs nothing. By `dpkg-deb -I`, 7 of the 60 packages carry a maintainer script
   (`pipewire`, `pipewire-bin`, `wireplumber`, `libglib2.0-0t64`, `libffado2`,
   `libreadline8t64`, `readline-common`) and 51 carry a `triggers` file, the shared-library
   packages' request for `ldconfig`. The scripts were not read (their packaging is the
   packages' own licence); what they are for is taken from the packages' control data and
   from what the image has to do. `pipewire` depends on `adduser` and
   `init-system-helpers`, so its script concerns a system user and service units: a receiver
   runs as 65532 with no init system, and `chorus-soloistd` starts the two programs itself
   with a configuration it writes, so neither is used. That the others' scripts do nothing a
   receiver needs is not argued, it is tested: the image's PipeWire and WirePlumber start
   under the supervisor, link a client to the sink and deliver its audio, in the full gate.
   The trigger is the one thing that visibly matters, so the build does it: the image's own
   `ldconfig` (static; with `-r <root>` it prefixes paths itself when it may not chroot, as
   seen here) writes `/etc/ld.so.cache` over the base plus the packages, and that file is
   one of the files chorus adds. The test then checks every `NEEDED` name of every staged
   program and library against that cache as the image's `ldconfig -p` reads it back.
   dpkg's database is left as the base's, because dpkg installed none of the 60; so that a
   scanner can still find them, each package's control file is written to
   `/var/lib/dpkg/status.d/<package>` (the form images assembled without dpkg use; that
   scanners read it is `ASSUMED`, from memory, not checked here).

6. **`chorus-soloistd` is static (musl), like `chorus-server`.** It links no C and no
   library (std only, ADR 0130), so a static build needs nothing but the Rust target, and
   the binary is then independent of the base: a base or glibc move cannot break the
   supervisor, the same binary is what the release could ship alone, and the image test can
   hold it to "no `NEEDED`, no `INTERP`". A glibc build matching the base would gain nothing
   here (it uses no NSS, no locale and no dlopen) and would tie it to the base's glibc.

7. **The user is 65532:65532**, chorus-server's, added by name to `/etc/passwd` and
   `/etc/group`, so both containers can use the receiver directory's FIFOs and sockets
   (mode 0660). A rootless insert records every file as root's, so the three directories a
   run may leave unmounted are mode 1777, as `tools/image.sh` does for the server's state.
   The runtime directory `/run/chorus-soloist` must be a tmpfs owned by 65532: the
   supervisor sets it to mode 0700, which it can only do to a directory it owns.

8. **No Soloist file, and a check that says so (conventions rule 24).** `/opt/soloist` is an
   empty directory in the image. `make soloist-lists` (`tools/soloist-lists.py`, a full-tier
   gate step after `image` and `soloist-image`) prints the files chorus adds to each image,
   the Debian package list, the release's artifact names and the tracked files naming
   soloist, and fails by name on: a path naming soloist that is not on its short list of
   chorus's own names; the fake Soloist by name or by content (`FAKE_SOLOIST_`, which the
   fake's binary carries under any name); a file chorus adds to an image that is executable
   and is not byte for byte a `[[bin]]` of this workspace; a tracked file naming soloist that
   is a binary, an archive or of no known kind. It proves itself on planted faults first.
   The envelope's wording, "an ELF whose strings carry Spotify's own markers", is replaced
   by the executable rule: what Soloist's binary carries is not known here, and "every
   executable chorus adds is one this workspace built" needs no such knowledge.
   `tools/release.sh --list` prints the release's names without building, and a release
   refuses to finish unless its directory holds exactly that list, so the list the check
   reads cannot drift from what a release writes.

9. **The licences: a distribution question, not a rule-13 question.** Conventions rule 13's
   allowlist is about what chorus compiles and links: cargo-deny over the workspace's
   crates. Nothing here is linked into chorus: `chorus-soloistd` is static, has no
   dependency outside the workspace, and starts PipeWire and WirePlumber as programs. The
   image is chorus's own MIT OR Apache-2.0 binary beside Debian's unmodified binary
   packages, which by their Debian copyright files are under the MIT (Expat) licence
   (WirePlumber, most of PipeWire), the LGPL (GLib, libpulse, libsndfile, alsa-lib and
   others) and, for four source packages, the GPL (FFTW, GNU Readline, libffado, and the GCC
   runtime with its exception). "Mere aggregation of another work not based on the Library
   with the Library ... on a volume of a storage or distribution medium does not bring the
   other work under the scope of this License" (LGPL-2.1 section 2; GPL-2 section 2 says
   the same of the Program). So chorus's licence is untouched, and what remains is what
   whoever distributes the image owes for those packages: the notices and the source.
   How it is met:
   - notices: every package's `usr/share/doc/<package>/copyright` is in the image, as
     Debian ships it, and the test fails without one;
     `/usr/share/doc/chorus/THIRD-PARTY-NOTICES.md` names every package, version and source
     package; the release attaches that file;
   - source: the notices give, per package, the directory at snapshot.debian.org (the same
     timestamp) that holds its Debian source package; each `.dsc` was checked to exist.
     GPL-3 section 6(d) allows exactly this ("the Corresponding Source may be on a
     different server (operated by you or a third party) ... provided you maintain clear
     directions next to the object code"). LGPL-2.1 section 4 and GPL-2 section 3 say
     "from the same place". **This is the open point**: the repository and its releases
     are private and the program pushes no image (K4, K41), so nothing is distributed
     today; before the image or the release is given to anyone else, the owner either
     accepts the pointer for the LGPL-2.1 and GPL-2 packages or attaches the 47 source
     packages beside the image (a fetch-and-verify step that is not built). This record
     does not decide that for the owner.
   No GPL or LGPL source is read by anyone for any of this (clean-room, BRIEF 3.1).

10. **`--health-check` connects to nothing.** The receiver's socket takes one connection and
    a new one replaces the old (ADR 0130), so a probe that connected would drop
    chorus-server at every interval. The supervisor writes its claimed index to
    `<runtime dir>/receiver`; the probe reads it and looks at the lock (held: an exclusive
    lock from the probe would block), the socket, the FIFO and PipeWire's socket. It costs
    one process start and a handful of file system calls. It says the supervisor is alive
    and holding its receiver, not that Soloist is logged in.

11. **`init: true` in the compose file.** The supervisor reaps its own three children and
    no others (ADR 0130). What Soloist forks is not documented, so an init as PID 1 reaps
    whatever is left and forwards SIGTERM; it costs one small process of the 64.

12. **The compose file's limits are P7's, `ASSUMED`**: `mem_limit: 192m`, `cpus: 0.25`,
    `pids_limit: 64`. The PipeWire half is measured (the probe: about 19 MB RSS, about
    0.25 % of a core, 6 threads); Soloist's half cannot be until the owner's build runs.
    The macvlan network is external and named only; host paths are variables with defaults
    and no address, name or key is in any tracked file.

## What the image test proves, and what it does not

`docs/soloist.md`, "The image", lists both in full. In one paragraph: with no container and no
chroot, the unpacked image's own PipeWire, WirePlumber and `pw-cat` are run by the image's own
loader over the image's own libraries under the image's `chorus-soloistd --pipewire auto`, and
a float32 signal played into `chorus-r0` must come out of `r0.pcm` bit for bit; the same
supervisor is driven over `r0.sock` with the fake Soloist as its `--soloist-bin`. The fake has
no PipeWire client, so the two halves meet only in the supervisor: the audio is `pw-cat`'s. It
does not show Soloist, a container runtime (read-only root, tmpfs, capabilities, limits,
macvlan, Docker's healthcheck), timing, or arm64.

## Options not chosen

- **A Dockerfile with `apt-get install`** (decision 1), and shipping one beside the
  daemonless build (decision 2).
- **Debootstrap or `apt-get install` into a root under fakeroot**: runs maintainer scripts
  without a daemon, but needs a fake root or a user namespace, and the gate has neither
  privilege; the scripts do nothing this image needs.
- **conda-forge's PipeWire**: 1.6.9 has no `pw-cat` and conda-forge has no WirePlumber
  (the probe), and every measurement is of Debian's 1.4.2.
- **Removing the unused modules and their libraries** (decision 4).
- **A glibc `chorus-soloistd`** (decision 6).
- **A health check that reads `hello` from the socket** (decision 10).
- **Named volumes for the shared directories**: a named volume takes its ownership from the
  first image that mounts it, and the server image has no such directory, so the receiver
  directory would be root's whenever the server started first. A host directory the owner
  creates with owner 65532 has no such order.

## Consequences and open points

- The full gate gains two steps, `soloist-image` (33 s cold on the development host on
  2026-10-03, the first run, with 61 fetches; the step prints its wall-clock) and
  `soloist-lists`. Warm, `soloist-image` needs no network. Cold, it needs the registry and
  snapshot.debian.org; if either is unreachable the step fails by name, it never skips.
- The image is amd64 only, like the server image. An arm64 image is a second pin list
  resolved for arm64 and a second target; not built.
- Debian security updates do not reach the image until someone moves the base digest and
  resolves the pins again at a newer timestamp, in one commit. That is the cost of a build
  that reproduces, the same trade `deploy/README.md` states for the server's base.
- The source-package question of decision 9 is the owner's before anything is published.
- Whether Soloist finds what it needs in this image (shared libraries, CA certificates) is
  not documented and not testable here; a missing library is a line in the pin list.
- The server-side flags named in `deploy/compose.yaml` (`--soloist-dir`,
  `--soloist-receivers`) and `chorusctl soloist restart` arrive with the server track; the
  lines are comments until then.
