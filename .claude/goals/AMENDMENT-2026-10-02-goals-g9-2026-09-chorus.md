# Amendment to the 2026-09-chorus program: gate tiers (goals program, goal 9, 2026-10-02)

Written by the goals program (NSchatz/goals) (its brief, sections 13 and 0.13, goal 9, R8) and merged into chorus between goals, during a drain of this program (no 2026-09-chorus goal running). It applies to every 2026-09-chorus goal whose ledger is created after it merged, and **never against a line of that goal's goal file**: where a goal file names a file, command, gate, label or rule, that line stays in force for that goal (the list below, and the lists of the earlier goals amendments). A goal already running when it merged keeps the contract it started with.

- **Spec version:** 1.0
- **What it supersedes:** in the brief's section 0.3 (the merge rule) and section 4.7 item 1, the gate a PR passes before its self-merge is `make tier-fast`, and `make tier-full` (which is `make gate`) passes before a goal's last merge. `make gate-fast` stays the gate of a docs-only or ledger-only change. The brief, its goal files, the earlier amendments and every checkpoint file are otherwise unchanged.

## The owner's words it carries out (the goals interview, 2026-10-01)

| Row | Decision, as recorded |
|---|---|
| W15 | Per-goal costs the new standard may cut: **Compact context + Tiered gates (fast gate per PR, full gate at goal end and nightly on main)** |
| W36 | Scope extras: **Tiered gates per repo** (the goals program adds fast/full tiers to each repo's Makefile via PRs into 3d, chorus, devices, holdfast, home, shopkit) |
| W7 | **Between goals, self-merged**: no goal sees its contract change mid-run; no per-amendment approval |

As later words of the owner these beat earlier lines where they conflict, except where a goal file wins.

## What changes

1. **The tiers** (`make tier-fast` and `make tier-full`, both run by `tools/gate.sh`, which times every step and prints the wall-clock):
   - `make tier-fast`: the conventions checks, `cargo fmt --check`, `cargo clippy -D warnings` and `cargo test --workspace`; the determinism run, `firmware-check`, `verify`, the alsa-null run, both firmware images, the image and the endpoint packages are left to the full tier;
   - `make tier-full`: `make gate`, unchanged.
   `make gate` and `make gate-fast` keep their meaning.
2. **Every PR:** `timeout 3600 flock -o -w 1800 /cache/locks/chorus-heavy.lock make tier-fast` (or the lock tool's `goals lock chorus-heavy`) passes on the branch up to date with `origin/main`, with its last 20 lines, its per-step timings and its wall-clock in the PR body, as the merge rule says for `make gate`. A docs-only or ledger-only change keeps `make gate-fast`.
3. **A goal's last merge:** `make tier-full` (`make gate`) passes on the branch first, its tail in that PR and in the ledger.
4. **Nightly:** the goals supervisor runs `make tier-full` on `origin/main` every night from 08:00 UTC in its own worktree (`/cache/wt/chorus/nightly`) under `goals-heavy` and `chorus-heavy`, and skips the night if those locks stay held 30 minutes. A goal reads the latest result in the pinned status issue's Trends or with `goals nightly schedule`.

## Left in force (goal files win)

- g14 A: "The OTA state machine survives every injected fault in make gate and rolls back a bad image (test tail)": `make gate`, as written.
- g21 A: "The P5 stack builds, embeds in chorus-server, and its unit tests and one browser smoke test pass in make gate (tail)": `make gate`, as written.
- g27 A: "make gate passes on a fresh clone of origin/main (tail with wall-clock)": `make gate`, as written.
- Every goal file's git line ("Git per §0.3: a branch + PR per track, self-merged only after the local gate passes with its tail in the PR"): it names no command, so the local gate is `make tier-fast` (item 2).
