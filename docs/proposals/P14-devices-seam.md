# P14: The devices seam (chorus PRs in devices before devices finishes)

> Note (2026-10-08): devices' `builds/chorus-*` is now `projects/chorus-*/v<n>/` (devices
> `docs/electronics-restructure.md`). The text below is the dated proposal, unchanged.

- Decisions: K23
- Status: PROPOSED (chorus goal 1, 2026-09-30); decided at Checkpoint K
- Outcome: overtaken. The speaker designs landed through the devices repo's own tasks (devices pull requests 120, 121 and 128; `docs/decisions/0233-goal-25-closed-the-twoway-and-sub-landed-in-devices.md`, `0234-goal-26-closed-no-rack-amp-or-soundbar-and-the-lcr-set-landed.md`), so no chorus pull request in devices was needed (recorded 2026-10-06)
- If deferred: Drafts on chorus branches only
- Builds on: goal 24 (§28 item 3, the compact speaker "a devices PR if devices has finished or P14 was approved; otherwise the draft branch"), goal 25 (§29 item 3, the two-way and the subwoofer, "landing per §28 item 3"), goal 26 (§30 item 4, the rack amp, soundbar and LCR set), goal 27 (§31 item 4, "if it has [finished], or P14 was approved, open the devices PRs from the draft branches")

## Question

May chorus, before the devices program has finished, open PRs in the owner's devices repo limited to
`builds/chorus-*` (and the inventory part records those builds need), under devices' `make check`
and `make invariants`, holding a new `/cache/locks/devices-merge.lock`, touching nothing else in
devices?

The owner decisions that bound it, quoted:

- K23: "speaker designs live in [the owner's devices repo] (`builds/chorus-*`), made with [the owner's shared Python library]. chorus
  files rows ... once an owner's program has finished (its last COMPLETE line on main), chorus
  serves its own rows by PR in that repo under the owner's rules and gate, citing the row. Prints
  stay the owner's. Not chosen: designs in chorus/hardware; rows only, never PRs."
- K5: "a row open after its owner's program finished may be served by the requester under the
  owner's rules."
- K49: "[the owner's devices repo]: rows while devices runs; after its last COMPLETE, PRs serving chorus rows
  (`builds/chorus-*`) under devices' gate."
- K50 counts "every hardware design packaged in devices" as program success.
- Brief §0.13 defines "finished" for devices as `COMPLETE (goal 5)` in
  `.claude/goals/2026-09-devices-g5.status.md` on devices' `origin/main` (or goal 4's line if
  `CHECKPOINT-D.approved` says "glide6 first"; it does not, see below).

"Not chosen: designs in chorus/hardware" means the fallback drafts are a waiting room, not a home:
every option below ends with the designs in devices.

## Constraints that bind every option

- **devices' rules bind every `builds/chorus-*` package** (devices `CLAUDE.md` "Out of scope",
  devices brief §0.8): no selling, compliance or distribution content; "Fabricating what can be
  bought ... Buy the module; the custom work is the device" (a custom board only after the owner
  approves a proposal that departs from it, devices brief §4.8); no second project dressed as a
  prerequisite. Every BOM line is an inventory part record (`builds/README.md`: "part IDs must
  exist in the inventory repo").
- **devices is edited only by its own program** until it finishes: devices brief §0.13, "The home,
  3d and devices repos are each edited only by their own program (after that program has finished,
  §0.13 'Requests' applies)". P14 (a) or (b) is a delegation of one namespace that only the owner
  can grant; it goes in `CHECKPOINT-K.approved`, not in any agent's judgement.
- **Inventory ownership** (devices brief §0.13, chorus brief §0.13): devices owns parts, fasteners
  and items; home owns suppliers (home has finished, so chorus self-serves supplier records under
  home's rules regardless of P14). inventory merges hold `/cache/locks/inventory-merge.lock` and run
  inventory's `make check` (validate, prices, identity, doctor; inventory `Makefile`).
- **Lock order** (chorus brief §0.3): outer to inner `shopkit-release`, then a shared merge lock,
  then `chorus-heavy`; never request a shared lock while holding `chorus-heavy`. A new
  `devices-merge.lock` sits at the shared-merge level.
- **Hardware is read-only** (K4, chorus brief §0.7): no order, print or flash in any option.
- **Identity** (K27): no name, address, LAN address or hostname in any devices file. devices'
  build template headings are devices' rule (chorus brief §0.7).
- CLAUDE.md rule 8: the choice is argued on fitness for chorus's requirements (a packaged,
  gated design the owner can build from), never on which tools happen to be installed.

## Re-verification of the planning research

The planning input is `review-cross-program-v1.md` H1 (2026-09-29), which proposed this question.
Re-checked against devices `origin/main` = `29f9e3c` (2026-09-30 01:10 UTC, `git ls-remote`
matches the sibling clone) and the shared library's `origin/main` = `081325f`:

| H1 said (2026-09-29) | Re-checked 2026-09-30 | Changed? |
|---|---|---|
| devices is on goal 3 | Goals 1, 2 and 3 are COMPLETE: `COMPLETE (goal 1): 2026-09-29`, `COMPLETE (goal 2): 2026-09-29`, `COMPLETE (goal 3): 2026-09-30` in their ledgers. Goal 4 is running: its ledger's "Resume here" says sourcing landed (inventory#27, devices#41) and "the close-out branch `dev-g4/close-out` as a fresh clone ... `make check` + `make invariants`" is running, then PR, review, COMPLETE | Yes: devices moved two goals |
| goal 5 waits on glide6's Checkpoint A | Still true. devices brief §9: goal 5 needs `round2` in `hardware/mouse/glide6/pick.toml` to name a dummy and a `look_approved` log entry; `pick.toml` line 23 reads `round2 = ""` ("Committed UNSET"). `CHECKPOINT-D.approved`: "the default, glide6 goal 2 last (goal 5); no 'glide6 first'" and "Not approved here (stay [the owner's Needs list]): Checkpoint A (A1-A2 print and pick, A3 the look, A4 the purchases)" | No |
| once goal 4 finishes, devices goes idle and nobody reads chorus's rows | Still true: devices brief §0.13 serves rows "in its next track"; after goal 4 the next track is goal 5, BLOCKED until the owner prints | No |
| a new `/cache/locks/devices-merge.lock` | `/cache/locks/` holds `dev-kernel.lock`, `inventory-merge.lock`, `shopkit-merge.lock` and others; no `devices-merge.lock` exists | No (confirmed new) |
| (not examined) | **devices' own landers take no merge lock.** devices brief §0.3: "In repos only this program edits, one lander at a time rebases, re-runs the gate, merges." A `devices-merge.lock` therefore serialises only chorus's landers; it does not stop `main` moving under a chorus gate | New |
| (not examined) | **`make check` does not check a new build directory.** The `check` recipe (devices `Makefile:453-479`) costs `$(BUILDDIR)` (proto42) and calls `node-v1-check`, `house-sensors-check` and `cars-check` by name; no step globs `builds/*`. A `builds/chorus-*/bom.csv` passes `make check` unchecked unless the library's `inv bom builds/chorus-<x>` is run beside it (the command `builds/README.md` gives) | New |
| (not examined) | **devices would not run `make invariants` on a builds-only PR.** devices brief §4.2: it runs "before merging any PR whose diff touches `firmware/`, `hardware/`, `layouts/`, `specs/`, `baseline.json`, `pyproject.toml`, `uv.lock` or the `Makefile`". A diff limited to `builds/chorus-*` touches none. By default the target also takes `dev-kernel.lock` (devices' own program lock) with `flock -w 3600` and pins to CPUs 8-10 (`Makefile:492-497`) | New |
| (not examined) | **devices' identity scan reads home's private terms** through its library config's `[repos] home = "../home"`. chorus never touches home; chorus goal 1 builds a stand-in at `/cache/chorus-private/shopkit-home/private/scrub-strings.txt` (chorus brief §5 item 4) for `SHOPKIT_REPO_HOME`. A chorus-run devices gate therefore scans with chorus's term list, not home's full one | New |
| the chorus-1 row | Filed in the shared library's `PROGRAM-REQUESTS.md` `## To devices`, state OPEN, asking for DEFERRED with the note "chorus serves after devices finishes (chorus K23)" and naming P14 | Filed |

Gate timings from devices' own ledgers (devices' 3 CPUs, `SHOPKIT_WORKERS=3`):

| Run | `make check` | `make invariants` |
|---|---|---|
| goal 1 (#25, #28, #30, #36) | 469-682 s | 1135-2983 s (goal-1 range quoted as "2983-3741 s with the look models built cold") |
| goal 2 (#38) | 322 s | 3623 s |
| goal 3 (#40) | 461 s | 3539 s |
| goal 4 (main@04a6462) | 372 s | (running at read time) |

So `make check` is 5-11 minutes and `make invariants` 19-62 minutes on devices' CPUs. chorus runs
on a 2-CPU quota (`cpu.max` 200000/100000, read 2026-09-30); the same runs there would take longer
(**ASSUMED** about 1.5x: 8-17 minutes and about 28-95 minutes). inventory's `make check` was not timed in
any ledger read (**ASSUMED** under two minutes: four subcommands of the shared library's CLI over YAML).

devices' `main` moved 49 times since 2026-09-28; 10 of those commits touched `Makefile`, 12
`pyproject.toml` and 12 `uv.lock` (`git log --since=2026-09-28 --name-only`). No devices PR was open
at read time (`gh pr list --state open` on the owner's devices repo returned `[]`).

Adversarially verified 2026-09-30 (goal-1 verifier 3): 13 claims confirmed, 0 refuted, 1 partly right, 0 unverifiable; corrections applied; the recommendation stands.

## Options

### Option A: approve as stated

- What: chorus goals 24-26 open and self-merge PRs in devices touching only `builds/chorus-*`, and
  PRs in inventory adding the part records those builds need. Each devices PR runs `make check` and
  `make invariants` on the branch up to date with devices `origin/main`, holding
  `flock -o -w 1800 /cache/locks/devices-merge.lock` from the final pull to `gh pr merge`; each
  inventory PR holds `inventory-merge.lock` and runs inventory's `make check`.
- Costs:
  - Money: none.
  - Gate time: six builds (`chorus-compact-v1`, `-twoway-v1`, `-sub-v1`, `-rackamp-v1`,
    `-soundbar-v1`, `-lcr-v1`, chorus brief §28-§30), grouped into about three PRs (one per goal),
    each about 1-2 hours of `make check` plus `make invariants` on chorus's quota, so about 3-6 hours
    across goals 24-26, most of it `make invariants` building glide6's looks, proto42's firmware and
    the car images, none of which a `builds/chorus-*` diff can change.
  - Lock contention: run with its defaults, `make invariants` takes `dev-kernel.lock` for up to an
    hour and pins to devices' CPUs, so a running devices goal 5 waits on chorus (and vice versa).
  - Effort: small; the drafts are written either way (§0.13), and the PR is the draft plus a gate.
  - Owner's review: after the fact, in devices' history; no step waits on the owner.
- Risks:
  - The gate as stated does not check what the PR adds: `make check` never costs a new build's BOM
    (see re-verification), so a PR could merge an unpriced or dangling `part_id`.
  - `devices-merge.lock` is one-sided; devices' own lander can move `main` between chorus's gate and
    merge. With a `builds/chorus-*`-only diff a textual conflict is impossible, but a devices change
    to the library's `inv bom` behaviour or to the pin could make the merged BOM fail later.
  - The identity scan runs with chorus's term stand-in, weaker than home's list.
  - A devices session meets foreign commits in a repo its brief says only it edits.
- Fit: meets K23's intent and K50 ("every hardware design packaged in devices") inside the program;
  spends about an hour per PR on a check that cannot fail because of the PR.

### Option B: approve with limits (recommended)

- What: as A, with these limits written into `CHECKPOINT-K.approved`:
  1. **When.** Only after devices' goal 4 ledger has `COMPLETE (goal 4)`, and only while no open
     devices PR touches `builds/chorus-*`, `data/parts/` or `data/fasteners.yml` in inventory (read
     with `gh pr list` before the branch is cut and again before merge).
  2. **Where.** devices: new files under `builds/chorus-<build>-v<n>/` only (BOM, `budgets.csv`,
     `log.md` on devices' template, committed renders and wiring diagrams, the acoustics design
     record). inventory: new part and fastener records only, never an edit to an existing record,
     a schema, `records.toml` or a root file. Anything else a build needs (a printed enclosure's
     `@part` modules under `hardware/` and `specs/`, a Makefile target, a devices re-pin to a
     library tag with its acoustics package) stays a row to devices and a draft, served after devices
     finishes. Generated files are committed with the command that made them in `log.md`.
  3. **Gate.** On the branch up to date with devices `origin/main`: devices `make check` with
     `SHOPKIT_REPO_INVENTORY` pointing at inventory `main` holding the records and
     `SHOPKIT_REPO_HOME` at chorus's term stand-in, **plus** the library's `inv bom builds/chorus-<x>`
     for every build in the PR (exit 0: every line priced, every budget within its max).
     `make invariants` only when devices' own §4.2 path rule requires it, which a diff under limit 2
     never does. The inventory PR lands first (inventory `make check` under `inventory-merge.lock`).
  4. **Lock and race.** Hold `devices-merge.lock` (lock order: shared merge locks, then
     `chorus-heavy`, the gate inside it under `timeout 3600 flock -o -w 1800
     /cache/locks/chorus-heavy.lock`). Immediately before `gh pr merge`, compare devices
     `origin/main` with the gated base: if it moved in any path, merge `origin/main` into the branch
     and re-run the gate (at most the brief's 3 fix rounds).
  5. **Visibility.** Each PR cites row chorus-1 and adds a `DONE <PR>` FYI row to devices' section
     of `PROGRAM-REQUESTS.md` naming the files, so a devices session sees it at its next phase
     boundary. If devices DECLINES chorus-1 (only devices changes that row's state), chorus stops
     opening PRs and falls back to C.
- Costs:
  - Money: none.
  - Gate time: about 8-17 minutes of `make check` plus seconds of the library's `inv bom` per devices PR,
    and about two minutes of inventory `make check` (**ASSUMED**), so under an hour in total across
    goals 24-26, instead of A's 3-6 hours.
  - Lock contention: none on `dev-kernel.lock`; `devices-merge.lock` only against chorus's own
    agents; `inventory-merge.lock` is already shared with devices by both briefs, so waits there are
    the normal cost of inventory work.
  - Effort: small; one extra command per build, one FYI row per PR.
  - Owner's review: the same as A; the owner can revert a devices PR, and chorus-1's state is a
    standing veto for devices.
- Risks:
  - A build whose enclosure is printed (P12, goal 24) lands in devices without its printed parts;
    the log says so and a row carries them. A build whose enclosure is wood (cut lists and drawings
    in the build directory) lands whole.
  - The identity scan still uses chorus's term list; mitigated by chorus's own gate scan on the same
    text in chorus first (the draft branch carries the same files) and by the added-line scan the
    brief already uses for homelab.
  - If devices goal 5 runs at the same time (the owner finished Checkpoint A), the inventory
    category files (`data/parts/electronic.yml` and others) are the one place both programs append;
    `inventory-merge.lock` and "rebase, re-run, merge" handle it, and a conflict is resolved by
    re-appending chorus's new records, never by editing devices' lines.
- Fit: meets K23 and K50 inside the program, keeps devices' rules and gate (a PR devices itself
  would merge passes the same checks), checks what chorus adds (the BOM), and costs devices nothing
  while it waits on Checkpoint A.

### Option C: decline; drafts on chorus branches only until devices finishes (the fallback)

- What: the brief's §0.13 fallback and the "If deferred" line. Each design lives on a pushed chorus
  branch `chorus-g<n>/devices-<build>` with no PR, passes devices' `make check` in a local,
  never-pushed devices worktree with the draft applied (and, under this proposal's finding, the
  build's library `inv bom`), and row chorus-1 carries the tail and branch SHA. inventory part
  records stay in the draft too (a devices-owned registry). Goal 27 re-checks devices and opens the
  PRs if it has finished; otherwise the Needs item "start a devices session to serve rows chorus-<n>"
  stands.
- Costs:
  - Money: none.
  - Gate time: the same local `make check` per build as B (the fallback already runs it), no merge.
  - Owner's time: one devices session started by the owner after the program, which must re-gate
    every draft against a devices `main` that has moved (49 commits since 2026-09-28) and a
    newer pin of the shared library; each draft may need rework then.
  - Lock contention: none.
- Risks:
  - K50's "every hardware design packaged in devices" is not met at the finale unless the owner
    completes Checkpoint A and devices goal 5 finishes first (both owner-paced: printing and picking
    two fit-kit rounds).
  - Drafts rot: the checked tail proves the draft against a devices `main` of its own day only.
  - Part records that other drafts reuse cannot be shared across drafts except by copying.
- Fit: the safest for devices' sole-editor rule; the weakest for K50 and for the owner's workload
  after the program.

## Comparison

| Criterion | A: as stated | B: with limits | C: drafts only |
|---|---|---|---|
| Designs in devices by the finale (K23, K50) | Yes | Yes (printed enclosure parts may follow as rows) | Only if devices finishes first |
| Checks what chorus adds (BOM priced, part IDs exist) | No (`make check` skips new builds) | Yes (the library's `inv bom` per build) | Yes, locally, against that day's `main` |
| Gate time, goals 24-26 | about 3-6 h | under 1 h | about the same as B, no merge |
| Holds devices' `dev-kernel.lock` | Yes, up to 1 h per PR | No | No |
| Race with devices' own lander | Unhandled (one-sided lock) | Re-check `origin/main` before merge | None |
| Conflict risk in devices files | None textual (new namespace) | None (new files only) | Deferred to the serving session |
| Conflict risk in inventory | Category files, under the shared lock | Same, append-only | Deferred |
| Owner review burden | After the fact | After the fact; chorus-1 is a veto | A devices session to start and review later |
| Changes devices' sole-editor rule | Yes, for `builds/chorus-*` | Yes, for new files under `builds/chorus-*` and new part records | No |

## Recommendation

**Recommendation:** Option B, approve with limits: PRs only after devices goal 4 is COMPLETE, new files under `builds/chorus-*` and new inventory part records only, gated by devices' `make check` plus the library's `inv bom` per build (invariants only where devices' own path rule asks), because it lands every design in devices under devices' rules while checking what chorus adds and holding no devices lock.

Why: the question's gate as stated spends about an hour per PR on `make invariants`, which cannot
fail because of a `builds/chorus-*` diff and would hold devices' own kernel lock, while
`make check` never looks at a new build's BOM. B swaps that for the one check that matters for a
BOM-only package and keeps everything else devices' own gate does. It costs under an hour of gate
time across three goals and one FYI row per PR. The owner gives up devices' sole-editor rule for
new files in one namespace and new part records while devices waits on Checkpoint A, and accepts a
devices identity scan run with chorus's term list rather than home's. Printed enclosure parts, a
devices Makefile target or a devices re-pin are not delegated; if a build needs them, they wait for
devices as rows.

## If the owner defers

Goals 24-26 build on "Drafts on chorus branches only" (option C): each design on a pushed
`chorus-g<n>/devices-<build>` branch with no PR, devices' `make check` (and the build's
the library's `inv bom`) passing in a local never-pushed worktree, row chorus-1 carrying the tail and the
branch SHA; goal 27 re-checks devices and opens the PRs if it has finished, otherwise the Needs item
"start a devices session to serve rows chorus-<n>" stands, and the program report lists P14 under
"Proposals awaiting the owner". The cost is K50's devices packaging left to a session after the
program and drafts that must be re-gated against a devices `main` that keeps moving.

## Open inputs

- **The owner's word on the delegation itself** (only the owner can amend devices' sole-editor
  rule; `CHECKPOINT-K.approved`).
- Whether devices' own sessions should also take `devices-merge.lock` (a devices-brief change that
  only the owner or a devices session can make); B does not depend on it.
- inventory `make check` duration: **ASSUMED** under two minutes; goal 24 times it.
- The 1.5x slowdown of devices' gates on chorus's 2-CPU quota: **ASSUMED**; goal 24 times the first
  run.
- P12's enclosure choice (goal 24) decides whether any build needs printed parts outside
  `builds/chorus-*`.
- devices goal 4's COMPLETE line: running at read time; goal 24 reads it.

## Sources

- devices `CLAUDE.md` ("Out of scope", "Commands", "Where things live"), the owner's devices repo, `origin/main` `29f9e3c`, read 2026-09-30
- devices brief `.claude/goals/2026-09-devices.md` §0 (goal table), §0.3, §0.4, §0.8, §0.13, §4.2, §4.7, §4.8, §9, §12, read 2026-09-30
- devices ledgers `devices@29f9e3c:.claude/goals/2026-09-devices-g1.status.md` (items 1.2, 3.1, 3.2, 5.1, 13.1), `-g2` (10.3), `-g3` (8.1, 8.2), `-g4` (2.4, 3.1, "Resume here"), and `CHECKPOINT-D.approved`, read 2026-09-30
- devices `Makefile` lines 75, 453-510 (`check`, `invariants`, `INVARIANTS_LOCK`, `INVARIANTS_CPUS`), 546-739 (node-v1, house-sensors, cars checks), read 2026-09-30
- devices `builds/README.md`, `builds/_template/{log.md,bom.csv}`, `builds/node-v1/log.md`, its library config, `hardware/mouse/glide6/pick.toml`, read 2026-09-30
- inventory `CLAUDE.md`, `Makefile`, `data/parts/electronic.yml` (record format), the owner's inventory repo (local clone `9f9b491`; `origin/main` `5b57402` differs by one commit touching tools, consumables and receipts only, via `gh api .../compare`), read 2026-09-30
- the shared library's `PROGRAM-REQUESTS.md` row chorus-1 (`origin/main` `081325f`), read 2026-09-30
- `review-cross-program-v1.md` H1, M1, M2, M6 (chorus planning research), read 2026-09-30
- chorus brief `2026-09-chorus.md` §0.1, §0.3, §0.7, §0.13, §1 (K5, K23, K49, K50), §3.4, §5, §28-§31, read 2026-09-30

## What was read

Local files only; no web page was needed for this proposal. chorus: [`.claude/goals/2026-09-chorus.md`](https://github.com/NSchatz/chorus/blob/535ed2816b5735153ac81c52a235024aa806f2b5/.claude/goals/2026-09-chorus.md)
(§0.1, §0.3, §0.7, §0.13, §1, §2, §3.4, §4.8, §5, §11, §24, §28-§31),
[`.claude/goals/2026-09-chorus-research/review-cross-program-v1.md`](https://github.com/NSchatz/chorus/blob/535ed2816b5735153ac81c52a235024aa806f2b5/.claude/goals/2026-09-chorus-research/review-cross-program-v1.md),
`/cache/tmp/chorus-g1/agent-rules.md`, `/cache/tmp/chorus-g1/proposal-format.md`,
`/cache/tmp/chorus-g1/verify/verify-3.md` (P14 section). devices
(`/cache/wt/chorus/sib/devices`, read-only): the files listed under Sources, plus `git log`,
`git ls-remote` and `gh pr list`. inventory (`/cache/wt/chorus/sib/inventory`, read-only): the files
listed under Sources, `git ls-remote`, `gh api` compare. The shared library (its sibling clone under `/cache/wt/chorus/sib/`,
read-only): `PROGRAM-REQUESTS.md`. `/cache/locks/` listing and `/sys/fs/cgroup/cpu.max`. No GPL
source and no reciprocal hardware design file was opened. No repo was modified.
