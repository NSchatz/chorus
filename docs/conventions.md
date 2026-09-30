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
  belongs to and says so.
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
| 20 | The flash guard | `tools/conventions/check-flash-guard.sh`; `tools/conventions/check-flash-guard-fixtures.sh` |
| 21 | Commits | `tools/conventions/check-commits.sh` |
| 22 | Every rule has a check | `tools/conventions/check-conventions.sh` |

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
  (`clippy::undocumented_unsafe_blocks`). Adding a place needs an ADR and a line in
  `check-rust-lints.sh`, which holds the tree to this list:

  | Where | Scope | Why |
  |---|---|---|
  | `crates/alsa` | the crate | the dlopen binding to libasound; every function crosses FFI |
  | `crates/hostctl` | six functions | libc wrappers for getrlimit, setrlimit, `sched_*`, mlockall and gettid; the rest of the crate stays denied |
  | `crates/server/tests/regress_0031_f6.rs` | one test | sets SCHED_BATCH through raw libc, which no chorus wrapper offers |

  `protocol`, `sync`, `audio`, `audio-path` and `measure` go further with
  `#![forbid(unsafe_code)]`.
- `dbg!` and `todo!` do not merge (`clippy::dbg_macro`, `clippy::todo`).
- Review guidance, not a rule: libraries return typed errors; nothing panics on input read from
  the network or a file; binaries map errors to documented exit codes.

## 3. Rust toolchain

One exact toolchain builds, tests, formats and lints chorus: `rust-toolchain.toml`'s `channel`.
`rust-version` in `Cargo.toml` equals it (chorus is an application; the gate's build is the MSRV
check), and so does the `rust:<version>` base of `deploy/Dockerfile`. The gate builds with
`--locked`. An upgrade is its own commit and goes through a proposal (K51).

## 4. C flags

The host build of the firmware cores compiles with
`-std=c11 -Wall -Wextra -Werror -Wshadow -Wpointer-arith -Wstrict-prototypes` and
`-ffp-contract=off -fno-fast-math`. The two floating-point flags are load-bearing: they keep the
C cores bit-exact with the Rust cores on the shared fixtures, so they are never removed.
`check-c-flags.sh` fails if any of these flags leaves `firmware/Makefile`.

## 5. C format

`clang-format` (pinned) with the repository's `.clang-format` (Microsoft base, 100 columns,
Linux braces, includes never sorted: the base that changed the fewest lines of the existing
code), checked with
`--dry-run --Werror` over every tracked C source and header under `firmware/`. A region that
must keep its layout (a table, a register map) is fenced with `// clang-format off` and `on` and
a reason.

## 6. C static analysis

`cppcheck` (pinned) over `firmware/src`, `firmware/main`, `firmware/check` and `firmware/tests`
with `warning` and `portability` enabled, the exhaustive check level and `--error-exitcode=1`
(it found a dangling stack lifetime in `app_main.c` the day it joined, goal 3). A suppression is a line in `firmware/cppcheck-suppressions.txt`
or an inline `cppcheck-suppress` with the reason beside it, and only for a proven false positive.

## 7. Shell scripts

Every tracked shell script is clean under `shellcheck` (pinned), with the repository's
`.shellcheckrc`. A disable is a per-line directive with its reason. Scripts start with
`#!/usr/bin/env bash` and are committed executable.

## 8. Workflows and YAML

`.github/workflows/*.yml` are clean under `actionlint`, and every tracked YAML file under
`yamllint -s` with the repository's `.yamllint` (both pinned). CI calls `make gate` and nothing
else that could drift from a local run (R12).

## 9. Shared fixtures

Protocol, sync and DSP behaviour is specified by files that both the Rust and the C
implementations read: `fixtures/protocol`, `fixtures/sync`, `fixtures/sync/crosscheck`, and
`fixtures/dsp` from goal 12. A behaviour change adds or changes a fixture, never a constant in
one language only. `check-shared-fixtures.sh` holds each directory to a Rust reader and a C
reader and fails on a file of a kind neither reads. `fixtures/control`, `fixtures/discovery` and
`fixtures/measure` are Rust-only by declaration (the endpoint does not speak them yet).
Fixtures are committed and regenerated only by their `make` targets, never by a test run.

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
timing evidence (BRIEF §3.1 rule 3). The reports written before this rule are listed in
`check-measurements.sh` until goal 4 relabels them (K48, audit A-6); the list can only shrink.

## 12. Licence

chorus is dual-licensed **MIT OR Apache-2.0** (K26): `LICENSE-MIT` and `LICENSE-APACHE` at the
root, and `license.workspace = true` in every crate, which the workspace sets to
`"MIT OR Apache-2.0"`. A contribution is offered under both.

## 13. Dependencies and licences

- Every crate the workspace builds, chorus's own included, has a licence on the allowlist of
  `deny.toml`: MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, 0BSD, ISC, Zlib, Unlicense and
  CC0-1.0 (brief section 4.7). Anything else needs an ADR naming the crate, then an exception in
  `deny.toml` citing it (K95; MPL-2.0 Symphonia under P9 is the first expected).
- Crates come from crates.io only, one version of each (`cargo deny check sources bans`).
- A new external crate needs an ADR answering BRIEF §3.2's question (why not build it).
- JavaScript, when the app arrives (P5): exact versions, a committed lockfile, scripts off, and
  the same allowlist.

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
| Crates | `Cargo.lock`, `--locked` | the lockfile checksums |

A pinned version is never upgraded silently: an upgrade is its own commit saying why, and a
toolchain upgrade (Rust, ESP-IDF) goes through a proposal (K51). Every version, number or licence
a pin rests on cites its URL and the date read.

## 15. Decision records

`docs/decisions/NNNN-<slug>.md`. **The number is the number of the pull request that adds the
record**, zero-padded to four digits, so parallel branches can never claim the same "next"
number (the collisions of 0012 and 0021 came from that). Draft as `0000-<slug>.md` and rename
once the PR exists. A record starts `# NNNN: <title>`, carries `- Status:` (proposed, decided,
superseded by ...), and says what it decides, why, what was not chosen, and its sources with the
date read. Records numbered before this rule keep their numbers; the duplicates are listed in
`check-adrs.sh` until goal 4 renumbers them (K48, audit B-19).

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
repository sets it: outside `docs/`, the name appears only in the approved read forms
`"${CHORUS_OWNER_AT_BENCH:-}"`, `env::var("CHORUS_OWNER_AT_BENCH")` and
`getenv("CHORUS_OWNER_AT_BENCH")` (and, in Markdown, the bare name in backticks), and never with
a default. Owner-facing command lines that set it live only under `docs/`. The program's plan
under `.claude/goals/` quotes the forbidden forms and is not scanned. `check-flash-guard.sh` is
lexical, so it has a fixture test of its own: `check-flash-guard-fixtures.sh` holds it to
twenty-three forbidden forms (JSON and YAML `env`, Makefile `:=`, `?=` and `env`, shell `export`,
`:=`, `:-1`, `declare -x` and a bare read, compose `environment:`, Rust `set_var` and a
defaulting read, C `setenv`, `putenv` and `?:`, Dockerfile `ENV`, systemd `Environment=`, Python
`os.environ`, `mise.toml` and `.cargo/config.toml` `[env]`, a command prefix, and a README
outside `docs/`) and to the approved read forms.

## 21. Commits

A subject reads `<area>: <summary>`: the area is a crate, `firmware`, `tools`, `docs`,
`deploy`, `ci` or `goals` (ledger commits), in lower case, and the subject stays within 100
characters before a squash merge's ` (#N)`, because subjects cite finding and decision IDs. No
em dash, no identity term. Branches are `chorus-g<n>/<topic>`; changes merge as squashed pull
requests after `make gate` passes on the branch up to date with `main`; history is never
rewritten. Checked on every commit since goal 3 began; the older history keeps its subjects.

## 22. Every rule has a check

`check-conventions.sh` reads the table at the top of this file: every rule names at least one
check, every named script exists and is run by the gate, every named gate step exists in
`tools/gate.sh`, and every `tools/conventions/check-*.sh` is named in the table.
