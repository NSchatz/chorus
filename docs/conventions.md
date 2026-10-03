# chorus conventions

chorus's own rules for code, fixtures, dependencies, pins, docs, identity and commits, written
from chorus's requirements (decided 2026-09-29 by the owner, K18). Each rule exists because chorus
needs it: two implementations of one protocol that must not drift, firmware that must never burn
an eFuse or flash itself, timing claims that must be measured, and a private repository built to
be run by outsiders.

How the rules work:

- **Every rule names its check, and every check runs in `make gate`** (and in `make gate-fast`
  when it needs no build). A check is a tool invocation or a short script under
  `tools/conventions/check-<name>.sh`; the gate runs every such script as its own timed step,
  and `check-conventions.sh` holds the table below to the scripts and gate steps that exist, in
  both directions.
- A check fails naming its rule here and the fix. It never writes a per-file record, and it is
  much smaller than what it protects.
- A check that needs a private input (the identity term list) or history a shallow clone lacks
  prints `SKIPPED` with the reason under CI, and fails anywhere else.
- A rule that cannot be checked cheaply is not a rule. Review guidance lives in the section it
  belongs to and is marked **Review-only (no check)**: it is what a reviewer looks for, no script
  enforces it, and the gate passing says nothing about it.
- The pinned tools come from `mise.toml` (`mise install`, rootless); the Rust toolchain from
  `rust-toolchain.toml`; ESP-IDF from `firmware/config/endpoint.conf`.

## The rules and their checks

| # | Rule | Check (gate step) |
|---|---|---|
| 1 | Rust format | gate step `fmt`: `cargo fmt --all --check` |
| 2 | Rust lints and unsafe | gate step `clippy`: `cargo clippy --workspace --all-targets --locked -- -D warnings`; `tools/conventions/check-rust-lints.sh` |
| 3 | Rust toolchain | `tools/conventions/check-pins.sh`; gate steps `build`, `test` (`--locked`, on the pinned toolchain) |
| 4 | C flags | `tools/conventions/check-c-flags.sh`; gate step `firmware-check` (builds with them) |
| 5 | C format | `tools/conventions/check-c-format.sh` |
| 6 | C static analysis | `tools/conventions/check-cppcheck.sh` |
| 7 | Shell scripts | `tools/conventions/check-shell.sh` |
| 8 | Workflows and YAML | `tools/conventions/check-workflows.sh` |
| 9 | Shared fixtures | `tools/conventions/check-shared-fixtures.sh`; gate steps `test`, `firmware-check` (both read them) |
| 10 | Tests need no device | gate steps `test`, `verify`, `firmware-check` (run with no device, sound card or network); `make verify` (`unrun-checks-are-visibly-unrun.sh`) |
| 11 | Measurement provenance | `tools/conventions/check-measurements.sh` |
| 12 | Licence | `tools/conventions/check-licence.sh` |
| 13 | Dependencies and licences | `tools/conventions/check-licence.sh` (cargo-deny over `deny.toml`) |
| 14 | Pins | `tools/conventions/check-pins.sh` |
| 15 | Decision records | `tools/conventions/check-adrs.sh` |
| 16 | Clean-room provenance | `tools/conventions/check-provenance.sh` |
| 17 | CLAUDE.md length | `tools/conventions/check-claude-md.sh` |
| 18 | No em dashes | `tools/conventions/check-em-dash.sh`; `tools/conventions/check-commits.sh` (messages) |
| 19 | Identity and secrets | `tools/conventions/check-identity.sh` |
| 20 | The flash guard | `tools/conventions/check-flash-guard.sh`; `tools/conventions/check-flash-guard-fixtures.sh`; `tools/conventions/check-flash-tools-refuse.sh` |
| 21 | Commits | `tools/conventions/check-commits.sh` |
| 22 | Every rule has a check | `tools/conventions/check-conventions.sh` |
| 23 | Datasheet-cited amplifier map | `tools/conventions/check-amp-map.sh`; gate step `firmware-check` (`test_amp` drives the datasheet-modelled part with the committed map) |

The rest of this file is each rule in full, in table order.

## 1. Rust format

`cargo fmt --all --check` with rustfmt's defaults (there is no `rustfmt.toml`): a style nobody
has to learn or argue about. Formatting-only changes land as their own commit.

## 2. Rust lints and unsafe

- The lint set lives once, in the root `Cargo.toml` under `[workspace.lints]`, and every crate
  opts in with `[lints] workspace = true`. clippy runs with `-D warnings`, so a warning is a
  failure.
- `unsafe_code` is denied workspace-wide. It is allowed only where chorus talks to the operating
  system or the sound stack, at the narrowest scope, with a reason beside the allow; every
  `unsafe` block carries a `// SAFETY:` comment stating the invariant it relies on
  (`clippy::undocumented_unsafe_blocks`). `check-rust-lints.sh` holds the tree to this list,
  counting every `allow` or `expect` of `unsafe_code` (or `unsafe_op_in_unsafe_fn`), inner or
  outer, alone or combined with other lints, over one line or several; adding a place means a
  line in the script:

  | Where | Scope | Why |
  |---|---|---|
  | `crates/alsa` | the crate | the dlopen binding to libasound; every function crosses FFI |
  | `crates/hostctl` | seven functions | libc wrappers for getrlimit, setrlimit, `sched_*` (taking a policy and leaving it, the second for the Linux endpoint's playout thread, ADR 0069), mlockall and gettid; the rest of the crate stays denied |
  | `crates/hostprobe/src/sys.rs`, `crates/hostprobe/src/net.rs` | two modules | the host probes' FFI: clock_nanosleep, prctl timer slack, setsockopt and recvmsg for kernel receive stamps (ADR 0047) |
  | `crates/server/tests/regress_0031_f6.rs` | one test | sets SCHED_BATCH through raw libc, which no chorus wrapper offers |

  `protocol`, `sync`, `audio`, `audio-path` and `measure` go further with
  `#![forbid(unsafe_code)]`.
- **Review-only (no check):** a new place on the list above comes with an ADR saying why.
- `dbg!` and `todo!` do not merge (`clippy::dbg_macro`, `clippy::todo`).
- **Review-only (no check):** libraries return typed errors; nothing panics on input read from
  the network or a file; binaries map errors to documented exit codes.

## 3. Rust toolchain

One exact toolchain builds, tests, formats and lints chorus: `rust-toolchain.toml`'s `channel`.
`rust-version` in `Cargo.toml` equals it (chorus is an application; the gate's build is the MSRV
check), and so does the `rust:<version>` base of `deploy/Dockerfile`. The gate builds with
`--locked`. **Review-only (no check):** an upgrade is its own commit and goes through a proposal
(K51).

## 4. C flags

The host build of the firmware cores compiles with
`-std=c11 -Wall -Wextra -Werror -Wshadow -Wpointer-arith -Wstrict-prototypes` and
`-ffp-contract=off -fno-fast-math`. The two floating-point flags are load-bearing: they keep the
C cores bit-exact with the Rust cores on the shared fixtures, so they are never removed, and no
firmware build file (a Makefile, CMake file or sdkconfig) adds a flag that relaxes IEEE floating
point after them: `-ffast-math`, `-Ofast`, `-funsafe-math-optimizations`, `-ffinite-math-only`,
`-fassociative-math` or `-freciprocal-math`. `check-c-flags.sh` fails if any required flag leaves
`firmware/Makefile` or any forbidden one appears, and tests its own pattern first.

## 5. C format

`clang-format` (pinned) with the repository's `.clang-format` (Microsoft base, 100 columns,
Linux braces, includes never sorted: the base that changed the fewest lines of the existing
code), checked with
`--dry-run --Werror` over every tracked C source and header under `firmware/`. A region that
must keep its layout (a table, a register map) is fenced with `// clang-format off` and `on` and
a reason.

Vendored C under `third_party/` (upstream's code, byte for byte, pinned in
`third_party/README.md`) is outside this rule and rule 6 by scope, deliberately: both cover
`firmware/` only, and the vendored trees are compiled without the warning set. They are not outside
the safety scan (the endpoint scan walks every tree its unit list names under `[vendored]`) nor
the repository-wide rules (em dashes, identity).

## 6. C static analysis

`cppcheck` (pinned) over `firmware/src`, `firmware/main`, `firmware/check` and `firmware/tests`
with `warning` and `portability` enabled, the exhaustive check level and `--error-exitcode=1`
(it found a dangling stack lifetime in `app_main.c` the day it joined, goal 3). A suppression is a line in `firmware/cppcheck-suppressions.txt`
or an inline `cppcheck-suppress` with the reason beside it, and only for a proven false positive.

## 7. Shell scripts

Every tracked shell script is clean under `shellcheck` (pinned), with the repository's
`.shellcheckrc`. A disable is a per-line directive with its reason. Scripts start with
`#!/usr/bin/env bash` and are committed executable; a sourced library instead has no shebang
and a `# shellcheck shell=bash` line.

## 8. Workflows and YAML

`.github/workflows/*.yml` are clean under `actionlint`, and every tracked YAML file under
`yamllint -s` with the repository's `.yamllint` (both pinned). CI calls `make gate` and nothing
else that could drift from a local run (R12).

## 9. Shared fixtures

Protocol, sync and DSP behaviour is specified by files that both the Rust and the C
implementations read: `fixtures/protocol`, `fixtures/sync`, `fixtures/sync/crosscheck`, and
`fixtures/dsp` from goal 12. A behaviour change adds or changes a fixture, never a constant in
one language only. `check-shared-fixtures.sh` holds each directory to a Rust reader and a C
reader and fails on a file of a kind neither reads. `fixtures/control`,
`fixtures/measure`, `fixtures/schedule`, `fixtures/roomfit` and `fixtures/cec` are Rust-only by declaration (the endpoint does not speak them yet; room-correction fitting runs on the server; CEC runs on the Linux hub alone).
`fixtures/upnp` (goal 16) is Rust-only by declaration too: the UPnP AV media renderer runs in
chorus-server alone, and `crates/upnp/tests/fixtures.rs` reads every vector and fails on a file
it does not read.
`fixtures/protocol/v2` (protocol v2, goal 5) and `fixtures/protocol/v2/noise` are shared like the
rest: since the endpoint moved to v2 (goal 6) the check holds them to their Rust readers and to
`firmware/tests/test_protocol_v2.c` and `firmware/tests/test_noise.c`.
`fixtures/protocol/lowlat` (goal 13) holds the low-latency datagrams and their loss cases, read by
`crates/protocol/tests/lowlat.rs` and `firmware/tests/test_lowlat.c`.
`fixtures/codec` (goal 6) holds FLAC and Opus streams as the wire carries them with their
reference decodes, read by `firmware/tests/test_codec.c` and
`crates/client-linux/tests/codec_fixtures.rs`; `fixtures/README.md` says what each file is.
`fixtures/discovery` (shared since goal 14, when the endpoint got its own DNS-SD browse) holds the
browse queries and the advertisements with what each resolves to, read by
`crates/discovery/tests/dnssd_vectors.rs` and `firmware/tests/test_discovery.c`.
`fixtures/volume` (goal 11) is the `room_volume` sequence the real server sends over every volume
path, captured by `crates/server/tests/limits_hold_for_every_volume_path.rs` and fed through the C
endpoint's volume path by `firmware/tests/test_volume.c`.
**Review-only (no check):** fixtures are committed and regenerated only by their `make` targets,
never by a test run.

## 10. Tests need no device

Nothing in `make gate` needs a board, a sound card, the network or a privilege: hardware is
faked (the simulator, fixtures, the fake amp and radio). A check that needs an environment the
gate lacks is a separate `make` target that refuses by name and exits non-zero rather than
passing on an easier case (`tools/lib.sh`); `make verify` proves every such entry point refuses
visibly. Timing tests are deterministic or carry the busy-worker refusal of ADR 0020.

## 11. Measurement provenance

A timing claim is evidence only with a report under `docs/measurements/`. Every report carries
`Source: hardware|host|simulation|synthetic` and `Build measured: <sha>` naming a commit in this
history (a squash-merged change cites the merge commit, not a branch commit). Only `hardware` is
timing evidence (BRIEF §3.1 rule 3). Every report complies: the ones written before this rule
were relabelled on 2026-09-30 (goal 4, K48, audit A-6), and each says which commit replaced the
one it first named. On a shallow clone the commit lookup prints `SKIPPED`.

## 12. Licence

chorus is dual-licensed **MIT OR Apache-2.0** (K26): `LICENSE-MIT` and `LICENSE-APACHE` at the
root, and `license.workspace = true` in every crate, which the workspace sets to
`"MIT OR Apache-2.0"`. A contribution is offered under both.

## 13. Dependencies and licences

- Every crate the workspace builds, chorus's own included, has a licence on the allowlist of
  `deny.toml`: MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, 0BSD, ISC, Zlib, Unlicense and
  CC0-1.0 (brief section 4.7). Anything else needs an ADR naming the crate, then an exception in
  `deny.toml` citing it (K95). The exceptions so far: Unicode-3.0 for `unicode-ident` (ADR
  0039) and MPL-2.0 for the four Symphonia crates FLAC decoding uses (ADR 0044, P9).
- Crates come from crates.io only, one version of each (`cargo deny check sources bans`).
- **Review-only (no check):** a new external crate comes with an ADR answering BRIEF §3.2's
  question (why not build it); cargo-deny checks its licence and source, not the ADR.
- JavaScript, when the app arrives (P5): exact versions beside a committed lockfile
  (`check-pins.sh`); **review-only (no check)** until the app's own check exists: scripts off,
  and the same allowlist.

## 14. Pins

Everything that builds or checks chorus is pinned to an exact version, and to a digest where one
exists (brief section 0.9):

| What | Where | Digest |
|---|---|---|
| Rust toolchain | `rust-toolchain.toml` | the release channel manifest, cited there |
| Gate tools | `mise.toml` | `mise.lock` sha256 per binary; the PyPI wheel sha256 in `mise.toml` |
| Container bases | `deploy/Dockerfile` `FROM ...@sha256:` | the image digest |
| CI actions | `uses: owner/action@<40-hex>` | the commit |
| ESP-IDF | `firmware/config/endpoint.conf` | the tag's commit |
| ESP-IDF components | `firmware/main/idf_component.yml` (`==x.y.z`) | `firmware/dependencies.lock` component hashes |
| Crates | `Cargo.lock`, `--locked` | the lockfile checksums |
| The emulator (goal 14) | `tools/qemu/pins.conf` (the QEMU release, micromamba), `tools/qemu/libs.explicit.txt` (its conda-forge libraries, each an exact build) | the release archive's, the program's and micromamba's sha256; a sha256 per library package |

`check-pins.sh` checks the table: exact versions, the digests, the three Rust toolchain names
agreeing, the ESP-IDF tag and commit, each ESP-IDF component's exact version with its hash in
the lock, and the emulator's record and library list (`tools/qemu-env.sh` installs exactly those and
verifies an install against them). **Review-only (no check):** a pinned version is never
upgraded silently (an upgrade is its own commit saying why, and a toolchain upgrade, Rust or
ESP-IDF, goes through a proposal, K51), and every version, number or licence a pin rests on cites
its URL and the date read.

## 15. Decision records

`docs/decisions/NNNN-<slug>.md`. **The number is the number of the pull request that adds the
record**, zero-padded to four digits, so parallel branches can never claim the same "next"
number (the old duplicates of 0012 and 0021 came from that). Draft as `0000-<slug>.md` and
rename once the PR exists. A record starts `# NNNN: <title>`, carries `- Status:` (proposed,
decided, accepted, superseded by ...), and is listed in the index `docs/decisions/README.md`.
The records numbered before this rule are a closed set, exactly 0001-0027, each once: the
duplicates became 0022-0024 on 2026-09-30 (decided 2026-09-29 by the owner, K48, audit B-19) and
0025-0027 record three decisions made before the program (audit B-8). A new file numbered below
0032 fails. `check-adrs.sh` checks all of that, and the PR number of records from 0032 on (on a
shallow clone that part prints `SKIPPED`). **Review-only (no check):** a record says what it
decides, why, what was not chosen, and its sources with the date read.

## 16. Clean-room provenance

chorus is written clean-room against GPL references (BRIEF §3.1 rule 1; K33, K39): agents read a
GPL project's docs, issues and protocol descriptions, never its source, and never open a
reciprocally licensed hardware design. Every proposal (`docs/proposals/`), research note
(`docs/research/`) and decision record numbered from 0032 has a `## What was read` section
listing each source with its date; `docs/clean-room.md` records the position for the code
written before this rule.

## 17. CLAUDE.md length

`CLAUDE.md` stays at or under 200 lines (K52). Detail belongs here, in `docs/decisions/` and in
`docs/measurements/`.

## 18. No em dashes

No U+2014 anywhere: tracked files, commit messages, PR bodies and release notes (BRIEF §3.1
rule 5). Use a colon, a comma or parentheses.

## 19. Identity and secrets

No personal name, email address, LAN address, MAC address, SSID, account ID, API key or real
hostname in any tracked file, commit message, PR body or release note (K27). Prose says "the
owner"; the `NSchatz/<repo>` slug is the one allowed form of the account handle (I18); example
addresses come from RFC 5737 (`192.0.2.0/24`, `198.51.100.0/24`, `203.0.113.0/24`) and example
MACs from RFC 7042 (`00-00-5E-00-53-xx`). chorus needs no secret of its own; the owner's keys are
reached by reference only.

`check-identity.sh` enforces it three ways: the private term list, kept outside the repository
(`/cache/chorus-private/identity-terms.txt`, or `CHORUS_IDENTITY_TERMS`; never committed,
never printed) matched case-sensitively over tracked files and every commit message from goal 3
on; a scan for private IPv4 and non-documentation MAC addresses; and `gitleaks` over the history
and the tree. The term list missing fails the gate; under CI, where it cannot exist, the step
prints `SKIPPED` with the reason.

## 20. The flash guard

Every tool that can write to a device (flashing, an OTA push to a real address) refuses unless
the owner, at the bench, set `CHORUS_OWNER_AT_BENCH` to `1` (K4, K93). Nothing in this
repository sets it. Outside `docs/`, the name appears only in an approved read form, used the one
way its kind of file allows (a whitelist, so an unforeseen form fails): shell
`[ "${CHORUS_OWNER_AT_BENCH:-}" != 1 ]` (or `=`, `==`, `[[ ]]`) on one line; Rust
`matches!(env::var("CHORUS_OWNER_AT_BENCH").as_deref(), Ok("1"))` or `... == Ok("1")`; C
`const char *v = getenv("CHORUS_OWNER_AT_BENCH");` alone on its line with the next comparing
`strcmp(v, "1")`; Python `os.getenv("CHORUS_OWNER_AT_BENCH") == "1"`; and in Markdown the bare
name in backticks. None of those lines may supply a default or a go-ahead other than `1`
(`unwrap_or`, `map_or`, `or_else`, `is_err`, `is_ok`, `.ok()`, `.or(`, `?:`, Python `or`, `:=`),
a shell read assigned to a variable fails (the two-line default), any other kind of file may not
name it, and the name built from pieces fails anywhere. Owner-facing command lines that set it
live only under `docs/`. Not scanned: `docs/`, the program's plan under `.claude/goals/`, the
fixtures, and exactly the two scripts that build the name from pieces to do their job
(`check-flash-guard.sh` and `check-flash-tools-refuse.sh`); the fixture test fails if that list
grows.

`check-flash-guard.sh` is lexical, so it has a fixture test of its own:
`check-flash-guard-fixtures.sh` holds it to thirty-six forbidden forms (JSON and YAML `env`,
Makefile `:=`, `?=` and `env`, shell `export`, `:=`, `:-1`, `declare -x`, a bare read, a read
assigned to a variable and a two-line default, compose `environment:`, Rust `set_var`,
`unwrap_or_else`, `map_or`, `.or(`, `unwrap_or_default` and `is_err()`, C `setenv`, `putenv`,
`?:`, a ternary default and a reassigned read, Dockerfile `ENV`, systemd `Environment=`, Python
`os.environ[...] =`, `os.environ.get(..., "1")` and `or "1"`, `mise.toml` and
`.cargo/config.toml` `[env]`, a command prefix, a README outside `docs/`, and the name built from
pieces three ways) and to the approved read forms. What no lexical scan sees (a name decoded at
run time) is why `check-flash-tools-refuse.sh` runs every flashing tool: every tracked file that
reads the guard or calls esptool, espefuse or `idf.py flash` must be on its list, and each is run
against a complete fixture image and port with shims for those programs, with the variable unset,
empty, `0`, `true`, `yes`, ` 1`, `1 ` and `01`; each must exit non-zero naming the owner-at-bench
variable with no shim called. The guarded tool is `tools/firmware-flash.sh` (ADR 0062). A
program that is not a shell tool can be a listed **guard reader** instead (goal 14: the server's
firmware sender, `crates/server/src/firmware.rs`, which refuses a transfer to a peer that is not
loopback as `owner-not-at-bench`): it names the test in it that grades the refusal without the
variable, the check holds the file to holding that test and reading the guard, and `make gate`'s
test step runs it. A reader that calls a flashing program is not a reader; it must be a listed
tool. The search for readers and writers is itself checked to have found the listed tool (until
goal 14 it was handed to a shell builtin through `xargs` and found nothing).

## 21. Commits

A subject reads `<area>: <summary>` with a lower-case area, and stays within 100 characters
before a squash merge's ` (#N)`, because subjects cite finding and decision IDs. No em dash, no
identity term. `check-commits.sh` checks the shape, the length and the em dash on every commit
since goal 3 began (the older history keeps its subjects); identity terms are
`check-identity.sh`'s.

**Review-only (no check):** the area names what changed (a crate, `firmware`, `tools`, `docs`,
`deploy`, `ci`, or `goals` for ledger commits); branches are `chorus-g<n>/<topic>`; changes merge
as squashed pull requests after `make gate` passes on the branch up to date with `main`; history
is never rewritten.

## 22. Every rule has a check

`check-conventions.sh` reads the table at the top of this file: every rule names at least one
check, every named script exists and is run by the gate, every named gate step exists in
`tools/gate.sh`, and every `tools/conventions/check-*.sh` is named in the table.

## 23. Datasheet-cited amplifier map

Every `amp_` key in `firmware/config/endpoint.conf` has a value, never `unknown`, and cites on its
own line the page of TI's TAS5825M datasheet it was read from, in the form `TAS5825M SLASEH7H p. N`
(or `pp. N, M`; SLASEH7H is revision H), with every page inside the datasheet's 106. A register
value nobody can trace to a page is a value nobody can check, and an over-set amplifier damages a
loudspeaker rather than failing a test. `check-amp-map.sh` grades the file and first proves itself
on three scratch maps (an `unknown` key, an uncited key and an out-of-range page must each fail).
The reading behind the values is `docs/research/tas5825m-register-map.md`; a new amplifier part
brings its own datasheet revision and this rule's citation form with it.
