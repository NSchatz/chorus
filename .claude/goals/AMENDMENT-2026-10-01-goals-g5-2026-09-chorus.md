# Amendment to the chorus program: the new goal supervisor (goals program, goal 5, 2026-10-01)

Written by the goals program (NSchatz/goals, brief `.claude/goals/2026-10-goals.md` §9 and §0.13, goal 5)
and merged into chorus between goals, during a drain of this program (no chorus goal was running). It
applies to every chorus goal whose ledger is created after it merged, and **never against a line of that
goal's goal file**: where a goal file names a file, command, gate, label or rule, that line stays in force
for that goal (the list below). A goal already running when it merged keeps the contract it started with.

- **Spec version:** none yet. The goals program releases spec v1 in its goal 6; it reaches chorus only by a
  later amendment that says what changed (W35).
- **What it supersedes:** every place this brief or `AMENDMENT-2026-10-01.md` names `claude-goal-chain` as
  chorus's runner (amendment L6, L33-34), "goals 2 to 27 then chain without further review" (brief L8),
  read as below. Nothing else in the brief, `CHECKPOINT-K.approved` or `AMENDMENT-2026-10-01.md` changes.

## Noah's words it carries out (the goals interview, 2026-10-01)

| Row | Noah's decision, as recorded |
|---|---|
| W4 | Running programs: **"Migrate as pieces land"**: as each new piece merges, running programs are amended to use it |
| W7 | **Between goals, self-merged**: the program PRs an AMENDMENT file into each affected repo, self-merges under that repo's merge lock, and the supervisor holds that program's next goal until it lands; no goal sees its contract change mid-run |
| W19 | Supervisor self-healing: **All four** (relaunch dead sessions, stop hung work, re-set goals cleared by errors, clean up disk) |
| W20 | Judging "goal done": **Checker, then evaluator** |
| W26 | Locks: **Supervisor-managed**: one lock tool, fixed names, logged wait and hold times |
| W30, W31 | The runner: PR #67 **ported to NSchatz/goals**; supervisor cut-over **"Shadow, then swap"** |
| W64 | Delegated checkpoint review: **Fresh independent reviewer** for checkpoints after goal 1; goal-1 checkpoints stay Noah's alone |

These are the reversals R4 (the runner) and R5 (delegated review) of the goals brief §1.1. As Noah's later
words (2026-10-01) they beat the earlier amendment's runner lines (goals brief §0.13, Precedence).

## What changes

1. **The runner (R4).** Since 2026-10-01 18:18:17Z (NSchatz/goals `ops/swap.toml`) the goals supervisor
   (`goals supervise`, tmux window `claude:goals-supervisor`) runs chorus; `claude-goal-chain` is stopped
   and kept inert (NSchatz/goals `docs/runbook.md` has the fallback). It starts chorus goal n+1 only when
   goal n's COMPLETE line is on origin and its session ended with a met verdict (or a printed GOAL REPORT)
   whose origin check passed; it never starts a goal 1 and never starts a goal twice. Each goal runs in
   its own interactive session with Remote Control, in window `chorus-g<n>`, registered as a goal session;
   the session ends once its goal is met and origin-checked. "Chain" in this brief reads "the supervisor
   starts the next goal". The lanes check is `goals lanes chorus` (the CLI is linked at
   `/cache/goals/<container>/bin/goals`). chorus has no lanes manifest, so it stays a strict n-1 chain
   and finishes at goal 27's COMPLETE line.
2. **Self-healing (W19).** A dead goal session is relaunched with its goal re-issued (a push after two
   failures); a goal cleared by an error is re-set once the cause clears; a session paused at a usage
   limit gets one "Continue." after the reset; Claude Code is pinned and changes only between goals.
   Every action is logged, and one that touches a goal also goes in that goal's ledger under
   `## Supervisor actions`, which the goal commits with its next ledger commit.
3. **The checker (W20).** Before the evaluator judges a goal, `goals check` verifies it on origin: the
   `COMPLETE (goal n)` line, no `TODO` or `DOING` row, the report's lettered lines, merged PRs and clean
   trees. It runs in the plugin's Stop hook (at most 3 blocks) and again in the supervisor after a met
   verdict (at most 2 re-issues, then the goal is held and Noah is pushed). A goal may run it itself
   before its final report: `goals check /workspace/chorus 2026-09-chorus <n>` (or the `/goals:check`
   skill).
4. **End states (W20).** BLOCKED and INCOMPLETE (this brief's §0.10 and §33 formats, unchanged) are end
   states: a BLOCKED goal is held once and gets no note and no new turn; an INCOMPLETE goal holds the
   program and pushes Noah. The old watcher's not-met notes ("Chain stopped...") no longer arrive.
5. **Checkpoints (R5, W64).** chorus's one checkpoint, K after goal 1, is approved. A later checkpoint,
   if a future amendment adds one, goes to a **fresh independent reviewer**: a separate new session on a
   model at least as strong as the goal's, which sees only the packet and the repos and writes its
   verdict and reasons in the checkpoint's issue in NSchatz/goals. A goal-1 checkpoint is Noah's alone.
   The same-session delegated review (`CLAUDE_GOAL_CHAIN_REVIEW`) is never used again. No remaining chorus
   goal file names a checkpoint, so no goal-file line changes.
6. **Locks (W26; goals brief §0.3).** `goals lock <name> -w <seconds> -- <cmd>` takes flock(2) on the same
   `/cache/locks/<name>.lock` files, so it and `flock -o` exclude each other, and logs waits and holds.
   The names stay: `chorus-heavy`, `devices-merge`, `3d-merge`, `inventory-merge`, `shopkit-merge`,
   `shopkit-release`. **Order:** `shopkit-release`, then at most one `*-merge`, then `goals-heavy`, then
   the gated repo's own heavy lock; never two programs' heavy locks at once; never a shared lock while
   holding a heavy one. A holder that shows no progress for 1800 s is stopped by the supervisor, ledgered,
   and its goal retries. `timeout 3600 flock -o -w 1800 /cache/locks/chorus-heavy.lock make gate` keeps
   working unchanged.
7. **Merges from the goals program.** The goals program merges into chorus only between chorus goals,
   in a drain (the supervisor starts no chorus goal while a goals PR is queued, and merges it once none
   is mid-run, re-gated on the current main under `chorus-heavy`).

**Unchanged:** chorus is under a program hold since the swap, on Noah's words ("Stop the goal chains on
holdfast and chorus", 2026-10-01 16:03Z). No chorus goal starts until Noah releases it in a session's own
chat (`goals release chorus/2026-09-chorus --why "<Noah's words>"`). This amendment does not release it.

## Left in force (goal files win; goals brief §0.13; goal 1's seam inventory)

None of these changes until chorus finishes or the later amendment named:

| Goal-file lines | What stays | Changed later by |
|---|---|---|
| g14-g27 L1 ("Physical steps go on the Needs list (§0.6)"); each goal's Needs-tail line (g14 L11, g15 L9, g16 L8, g17 L10, g18-g20 L8, g21 L9, g22 L9, g23-g26 L8, g27 L11); g17 L8, g19 L6, g21 L7, g27 L9 | shopkit `NEEDS-NOAH.md` is chorus's live Needs list | goals goal 8 (side by side, mirrored into issues) |
| each goal's Requests line (g14 L10, g15 L8, g16 L7, g17 L9, g18-g20 L7, g21 L8, g22 L8, g23-g26 L7, g27 L10); g24-g26 L1 (`## To devices` rows) | shopkit `PROGRAM-REQUESTS.md` rows, the `## To chorus` tail | goals goal 8 |
| each goal's closing line (g14 L14, g15 L12, g16 L11, g17 L13, g18-g20 L11, g21 L12, g22 L12, g23-g26 L11, g27 L14); g14 L5 | the states `NEEDS-OWNER` and `NEEDS-OWNER-PROGRAM` | none while chorus runs |
| g14-g27 L1 (the local gate); g14 L4, g21 L4, g27 L4 | `make gate` (and `make gate-fast` for docs) | goals goal 9 (adds tiers under new names) |
| g23 L1, L4, L5 | shopkit's rules, gate (`make ci`) and release protocol | goals goal 10 |
| g24-g26 L1, g24 L6 | devices PRs under devices' rules and `/cache/locks/devices-merge.lock` | none |
