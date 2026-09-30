# 0069: the Linux endpoint ships as one .deb per architecture, cross-built for glibc 2.36 with zig, run by a hardened systemd unit with real-time limits

- Status: accepted (goal 10, 2026-09-30)
- Decided by: the goal (brief section 14 item 2: "a Linux endpoint package (binary, systemd
  unit, config, real-time limits) for arm64 and x86_64, built rootless, attached to the next
  release"; the goal-7 follow-up "make verify-host on the Linux endpoint")
- Follows: ADR 0008 (libasound by `dlopen`), ADR 0010 (the host contract), ADR 0022 (the
  CPU-time bound goes on first), ADR 0044 (Symphonia and libopus in the client), ADR 0047 (the
  host probes)
- Implemented in: `tools/endpoint-package.sh` (`make endpoint-packages`, gate step
  `endpoint-packages`), `deploy/endpoint/` (the unit, the config, `chorus-verify-host`, the
  README), `crates/client-linux/src/realtime.rs` with `--rt-priority`, `--rttime-us` and
  `--no-delay-log` in `config.rs`, `chorus_hostctl::leave_real_time_policy`,
  `CHORUS_BIN_DIR` in `tools/lib.sh`, `tools/release.sh`, `rust-toolchain.toml` (two targets),
  `mise.toml` and `mise.lock` (zig, cargo-zigbuild); held by
  `crates/client-linux/tests/endpoint_package_config.rs`, the unit tests in `realtime.rs` and
  `config.rs`, and the checks inside `tools/endpoint-package.sh`
- Install page: `docs/linux-endpoint.md`

## Context

`chorus-client` has been run from a checkout (`cargo build`, `target/debug`). K96 makes Linux
endpoints a product tier (the rack amp, the theater hub), so it needs to arrive on a Raspberry
Pi or a small x86_64 machine as something the owner installs and enables, and it has to be
built here, where there is no root, no container daemon and no aarch64 C compiler. The client
is a dynamically linked glibc program: it `dlopen`s `libasound.so.2` at run time (ADR 0008),
and it compiles C (libopus, through `crates/opus-sys` and the `cc` crate), so a cross build
needs a C cross compiler and a linker for the target.

## Decisions

### Targets and the glibc floor

`aarch64-unknown-linux-gnu` (Debian `arm64`) and `x86_64-unknown-linux-gnu` (`amd64`), both
built against **glibc 2.36**. The OSes an endpoint runs, read 2026-09-30:

| OS | glibc | Source |
|---|---|---|
| Debian 13 "trixie" (stable) | 2.41 (`libc6 2.41-12+deb13u4`) | https://packages.debian.org/trixie/libc6 |
| Debian 12 "bookworm" (oldstable) | 2.36 (`libc6 2.36-9+deb12u14`) | https://packages.debian.org/bookworm/libc6 |
| Raspberry Pi OS (64-bit), 2026-09-15, "A port of Debian Trixie" | 2.41 (`libc6 2.41-12+rpt1+deb13u4`) | http://archive.raspberrypi.com/debian/dists/trixie/main/binary-arm64/Packages; https://downloads.raspberrypi.com/os_list_imagingutility_v4.json |
| Raspberry Pi OS (Legacy, 64-bit), 2026-09-15, "A port of Debian Bookworm" | 2.36 (`libc6 2.36-9+rpt2+deb12u14`) | http://archive.raspberrypi.com/debian/dists/bookworm/main/binary-arm64/Packages; the same OS list |

2.36 is the oldest glibc of a supported release of either OS, so one build runs on all four.
The package says so (`Depends: libc6 (>= 2.36)`), and `tools/endpoint-package.sh` refuses a
binary whose highest `GLIBC_` symbol version is above it (today's binaries need 2.34).
The ALSA library dependency is `libasound2t64 | libasound2`: trixie's package is
`libasound2t64` (1.2.14) and bookworm's `libasound2` (1.2.8), both installing
`libasound.so.2` (https://packages.debian.org/trixie/libasound2t64,
https://packages.debian.org/bookworm/libasound2, the trixie arm64 file list
https://packages.debian.org/trixie/arm64/libasound2t64/filelist; the Raspberry Pi archive
rebuilds both, same names). The client names no libasound at link time (readelf shows only
`libc.so.6`, `libm.so.6` and the loader), so the dependency is the package's, not the linker's.
32-bit Raspberry Pi OS (`armhf`) is not built: the Linux board is still an unanswered owner
input (P4 deferred), and a 64-bit one is ASSUMED; `armhf` is one more target line if it is not.

### The C compiler and linker: zig through cargo-zigbuild

zig 0.16.0 (MIT; https://ziglang.org/download/index.json, released 2026-04-13) as the C
compiler and linker, driven by cargo-zigbuild 0.23.4 (MIT;
https://github.com/rust-cross/cargo-zigbuild/releases/tag/v0.23.4, 2026-09-02), both pinned in
`mise.toml` with their sha256 in `mise.lock` (conventions rule 14) and installed rootless by
mise. The target is written `<triple>.2.36`: zig ships the glibc symbol stubs of each version
and links against the one named, so the floor is a number in the build command, not a sysroot
to fetch and pin per architecture.

Weighed against a conda-forge cross gcc (`gcc_linux-aarch64` 16.2.0 with
`sysroot_linux-aarch64`, whose builds offer glibc 2.17, 2.28, 2.34 and 2.39,
https://api.anaconda.org/package/conda-forge/sysroot_linux-aarch64, read 2026-09-30), on
fitness for chorus's requirements:

- **One pinned artifact for both architectures.** zig is one tarball with one sha256 per host
  platform, pinned the way every other tool here is (mise). The conda-forge route is a
  compiler, binutils and a sysroot per target architecture, resolved by a solver into an
  environment; pinning it means an explicit lock of every package, a second pin mechanism.
- **The floor is chosen, not inherited.** With zig the floor is 2.36 exactly and changing it is
  one number; with conda-forge it is whichever sysroot build exists (2.34 or 2.39 near ours).
- **The link is Rust-aware.** cargo-zigbuild adapts rustc's linker invocation for zig (the
  `-lgcc_s` and unwinder cases), which a bare `CC`/linker override leaves to chorus.

Against zig: it is pre-1.0 and its linker warns about rustc's `-Wl,-O1` ("ignoring deprecated
linker optimization setting '1'", a `linker_messages` warning printed and left visible), and
libopus is compiled by zig's clang in the package while the gate's tests compile it with the
host's C compiler. The codec fixtures run on the native build; the packaged client is run here
only on x86_64 and only to open the ALSA `null` device (below). That gap is a follow-up.

Not chosen, and why (fitness, not what is installed): a static musl client cannot `dlopen`
the system's glibc-built `libasound.so.2` (ADR 0008 chose run-time binding so the client needs
no ALSA at build time); `cross` (cross-rs) runs builds in a container and needs a container
runtime, which a rootless build here does not have (podman and buildah fail with `unshare:
Operation not permitted`, brief section 0.11), and it would make the build depend on images
outside the pin table; building natively on a Pi makes the release depend on a device.

### The package format: a Debian binary package built with dpkg-deb

`chorus-endpoint_<version>_<arch>.deb`, because both target OSes are Debian: `apt install
./file.deb` resolves `libc6` and the ALSA library and gives conffile handling and a clean
remove and purge. Built with `dpkg-deb --root-owner-group -Zxz --build` ("Set the owner and
group for each entry in the filesystem tree data to root with id 0. Note: This option can be
useful for rootless builds", dpkg-deb(1), https://man7.org/linux/man-pages/man1/dpkg-deb.1.html,
read 2026-09-30; dpkg-deb 1.22.22 here), so neither root nor fakeroot is needed.
Reproducible where cheap: `SOURCE_DATE_EPOCH` is the commit's time ("used as the timestamp
... in the deb(5)'s ar(5) container and used to clamp the mtime in the tar(5) file entries",
the same page), every mtime is set to it, modes are set explicitly, and the check builds each
package twice from fresh stages and requires the two to be byte-identical (they were; a cold
rebuild of the binaries gave the same sha256 too, on the same checkout path).

No maintainer scripts: the unit needs no account (DynamicUser, below), and the service is not
enabled on install (the owner enables it, docs/linux-endpoint.md), so there is nothing for a
`postinst` to do. `/etc/chorus/client.conf` is listed in `DEBIAN/conffiles`.
Not chosen: nfpm or fpm (a second packaging tool to pin for a format `dpkg-deb` writes
directly); a tarball with an install script (loses dependency resolution and conffiles).

Contents: `chorus-client` and `chorus-wakeup-probe` in `/usr/bin`; the unit in
`/usr/lib/systemd/system` (systemd reads it there on both bookworm and trixie); the conffile;
`chorus-verify-host` in `/usr/bin` with the scripts and probes it runs under `/usr/lib/chorus`;
a README and a DEP-5 `copyright` listing every crate linked in with its licence and reproducing
the libopus BSD-3-Clause, MIT and Apache-2.0 texts. `Recommends: python3` (the verify-host
graders are Python).

### The unit: `chorus-client.service`

Directive semantics from systemd.exec(5), systemd.service(5) and systemd.resource-control(5)
(https://www.freedesktop.org/software/systemd/man/latest/, read 2026-09-30):

- **Who.** `DynamicUser=yes` ("a UNIX user and group pair is allocated dynamically when the
  unit is started"), so no sysusers.d entry or maintainer script; `SupplementaryGroups=audio`
  for `/dev/snd`; `StateDirectory=chorus-client` holds the protocol v2 identity (key and server
  pins) across restarts; `RuntimeDirectory=` for a bench run's delay log. The front-panel hook
  (input and GPIO groups, `DeviceAllow=char-input r`, `char-gpiochip rw`, and `ReadWritePaths=`
  for an LED under the read-only `/sys`) is documented in the unit as a drop-in, ASSUMED until
  the front-panel track runs it.
- **Real-time limits.** `LimitRTPRIO=20`, `LimitRTTIME=200ms`, `LimitMEMLOCK=64M`, the
  server's contract values (`config/verification.conf`), not measured on an endpoint
  (ASSUMED). `RestrictRealtime=` is left off on purpose ("any attempts to enable realtime
  scheduling in a process of the unit are refused").
- **Hardening that keeps ALSA.** `DevicePolicy=closed` with `DeviceAllow=char-alsa rw`
  ("char-alsa" is the specifier "for all ALSA sound devices"); `PrivateDevices=` is not used
  because it adds "no physical devices" to its `/dev`. `ProtectClock=yes` implies
  `DeviceAllow=char-rtc r`, which is why the device policy is written out rather than left to
  `auto`. `SystemCallFilter=@system-service` includes `@resources` (`setrlimit`,
  `sched_setscheduler`) and `@memlock` (`systemd-analyze syscall-filter` on systemd 257 here;
  on bookworm's 252, ASSUMED to be the same). The rest (`ProtectSystem=strict`,
  `ProtectHome`, `PrivateTmp`, kernel and cgroup protections, `RestrictAddressFamilies=AF_UNIX
  AF_INET AF_INET6 AF_NETLINK`, `MemoryDenyWriteExecute`, an empty capability set) gives an
  offline `systemd-analyze security` exposure of 1.8 ("OK"). None of it has run on a device
  with a sound card: that the hardening leaves playback working is ASSUMED until the owner's
  first install (a follow-up below).
- **Restart.** `Restart=always`, `RestartSec=2s`, `StartLimitIntervalSec=0` (a speaker keeps
  trying while its server is away), `RestartPreventExitStatus=2` (a refused configuration
  stays stopped rather than looping).
- **Configuration.** `EnvironmentFile=/etc/chorus/client.conf` holding one unquoted
  `CHORUS_CLIENT_ARGS=` line, expanded as `$CHORUS_CLIENT_ARGS` ("Use "$FOO" as a separate word
  on the command line, in which case it will be replaced by the value of the environment
  variable split at whitespace", systemd.service(5)). A `--config` file in the client was
  weighed and not built: the client's arguments are its one configuration surface, other
  tracks are adding flags to it now, and an argument list in an environment file needs no
  second parser to keep in step. `endpoint_package_config.rs` holds the shipped line, and the
  real-time line it documents, to the client's own parser and to the unit's limits.

### Real-time playout in the client, opt-in

`chorus-client --rt-priority <n> [--rttime-us <us>]` (default off) runs the playout thread
under `SCHED_FIFO` in the repository's order: `RLIMIT_RTTIME` first, then the policy through
`chorus_hostctl::take_real_time_policy`, clamped to the ceiling (ADR 0022;
`real-time-acquisitions.conf` lists the new unit, so the ordering check grades it). Only the
playout thread: the policy is taken after the receiving thread is spawned and left at the end
of each session with the new `chorus_hostctl::leave_real_time_policy`, because a thread
created by a real-time thread inherits its policy ("The default setting of the
inherit-scheduler attribute in a newly initialized thread attributes object is
PTHREAD_INHERIT_SCHED", pthread_attr_setinheritsched(3),
https://man7.org/linux/man-pages/man3/pthread_attr_setinheritsched.3.html), so the next session's receiving thread would otherwise be real-time unasked. That
function is the seventh `unsafe` libc wrapper in `crates/hostctl` (conventions rule 2's table and
`check-rust-lints.sh` say seven). A host with no ceiling is refused at start (exit 2,
`stopped reason=real-time-refused`). The client does not report a thread inventory the way the
server does (ADR 0012): the host contract grades the server, and the client's one real-time
thread is named on its status line.

### `--no-delay-log`

The client writes a delay-log sample every 100 ms (`SAMPLE_INTERVAL_US`); an installed
endpoint running for weeks would grow that file without bound (about 60 MB a day at about 70
bytes a line, arithmetic, ASSUMED line length). `--no-delay-log` keeps the run and its grading
exactly as before and writes nothing; the shipped config uses it, and documents
`--delay-log /run/chorus-client/delay.log` (a tmpfs) for a bench run.

### `make verify-host` on the endpoint

`chorus-verify-host` runs the repository's own `tools/host-contract.sh` and
`tools/spin-test.sh`, copied unchanged with `tools/lib.sh` and `config/verification.conf`,
against `chorus-server` and `chorus-rt-spin` built for the target and installed under
`/usr/lib/chorus/probe` (`CHORUS_BIN_DIR`, `CHORUS_SKIP_BUILD=1`). The endpoint package
carries `chorus-server` only for the host contract, off `PATH`. Run under `systemd-run` with
the service's limits (docs/linux-endpoint.md); without a ceiling each check refuses by name
(exit 3), and the package check asserts that refusal from the unpacked tree here.

### Checks, the gate and the release

`tools/endpoint-package.sh` checks, per architecture: every binary's machine, interpreter,
needed libraries and highest `GLIBC_` version (readelf; `file` is not installed here and is not
needed); `dpkg-deb --info` and `--contents` against the expected list, root ownership, the
conffile and the dependency; two byte-identical builds; `systemd-analyze verify` of the unit
(below); and where the machine runs the architecture, the packaged client opening the ALSA
`null` device through the `libasound` it `dlopen`s, refusing `--rt-priority 0` by name, and
`chorus-verify-host` refusing by name with no ceiling. Both architectures run in the gate:
45 s cold and 9 s warm, measured here 2026-09-30.

`systemd-analyze verify` runs rootless with `--root=` ("With ... verify ... operate on files
underneath the specified root path", systemd-analyze(1)) over the unpacked package plus copies
of the host's `.target` and `.slice` units (without `sysinit.target` and the rest of the
default dependencies it stops with "Unit sysinit.target not found"). It exits 0 over an
unknown key and only prints it, so any output fails the check, and a self-test feeds it a copy
of the unit with one misspelt directive and requires the finding.

`tools/release.sh` builds both packages into `dist/v<ver>/` beside the other artifacts, in
`SHA256SUMS`, attached by `gh release create` (docs/release.md). Because the client links the
four Symphonia crates (MPL-2.0, ADR 0044), the release now also attaches each crate's `.crate`,
checked against `Cargo.lock`'s checksum, and names its source in the notes (P9).

## What was read

All read 2026-09-30.

- https://packages.debian.org/trixie/libc6, https://packages.debian.org/bookworm/libc6,
  https://packages.debian.org/trixie/libasound2t64, https://packages.debian.org/bookworm/libasound2,
  https://packages.debian.org/trixie/arm64/libasound2t64/filelist
- http://archive.raspberrypi.com/debian/dists/trixie/Release and the trixie and bookworm
  `main/binary-arm64/Packages` indexes; https://downloads.raspberrypi.com/os_list_imagingutility_v4.json
- https://ziglang.org/download/index.json; https://codeberg.org/ziglang/zig/src/tag/0.16.0/LICENSE
  (MIT, the same text as the tarball's `LICENSE`)
- https://github.com/rust-cross/cargo-zigbuild/releases (v0.23.0 to v0.23.4) and the repository's
  licence (MIT, GitHub API)
- https://api.anaconda.org/package/conda-forge/sysroot_linux-aarch64 and
  https://api.anaconda.org/package/conda-forge/gcc_linux-aarch64 (metadata only)
- https://www.freedesktop.org/software/systemd/man/latest/systemd.exec.html,
  systemd.service.html, systemd.resource-control.html, systemd-analyze.html; `systemd-analyze
  syscall-filter` output on systemd 257 here
- https://man7.org/linux/man-pages/man1/dpkg-deb.1.html,
  https://man7.org/linux/man-pages/man3/pthread_attr_setinheritsched.3.html
- chorus's own ADRs 0008, 0010, 0012, 0022, 0044, 0047, `crates/hostctl`, `crates/client-linux`,
  `tools/host-contract.sh`, `tools/spin-test.sh`, `tools/lib.sh`, `tools/image.sh`,
  `tools/release.sh`, `docs/release.md`, `deploy/README.md`

No GPL or LGPL source was opened: glibc and alsa-lib are named by their Debian package
metadata and file lists only, systemd (LGPL) by its manual pages and `systemd-analyze` output,
and gcc by conda-forge's package metadata.

## Follow-ups

- The owner's first install on a Pi and on an x86_64 box: that the hardened unit plays through
  a real card, and `chorus-verify-host` under `systemd-run` (a bench report; goal 10's Needs).
- Run the codec fixtures against the cross-built client (an x86_64 test build through zig, or
  qemu-user for arm64), so the packaged libopus is held to the same fixtures as the native one.
- Per-crate copyright notices (the MIT and BSD crates' own lines) in the package's `copyright`,
  generated from each crate's licence files, before the finale's release (goal 27).
- The measured real-time values for an endpoint (priority, bound, memlock), replacing the
  server's ASSUMED ones, once `chorus-verify-host` has run on one.
