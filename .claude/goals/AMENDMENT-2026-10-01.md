# Amendment to the chorus program: "finished", the devices seam, the speaker split (2026-10-01)

Approved by the owner, 2026-10-01, in a Claude Code session's own chat: "Just proceed. This is my
approval." This file amends `2026-09-chorus.md` the way `CHECKPOINT-K.approved` does: where they
disagree, this file wins. The same decisions amend the devices and 3d programs (their own
`AMENDMENT-2026-10-01.md`) and `/plan-program` and `claude-goal-chain` (cosyte/claude-containers).

## The owner's words

> The devices goal is blocking all other work. The devices encompasses much more than just the
> mouse or keyboard. It should not be blocking more goal work from completing. How do we ensure
> this doesn't happen in the future?

The owner's answers that bind chorus (the question, then the answer verbatim):

| Question | Answer |
|---|---|
| Which work is stuck behind the devices mouse goal? | "chorus's devices work", "3d's g8, same pattern", "More devices goals" |
| When does a program count as finished for other programs? | "Agent-doable goals done": goals waiting on the owner's physical step are parked and don't hold the program open; finished = every non-parked goal COMPLETE |
| Where does the fix apply? | "Future and current" |
| devices' lanes? | "Per domain", "Those + Audio" (Input, House, Vehicles, Audio) |
| Who does the speaker builds in devices' Audio lane? | "Split": chorus designs the acoustics (shopkit-acoustics, drivers, crossovers); devices' Audio lane goals do the enclosures, boards and BOMs from chorus's rows |
| A finished program's parked goal resumes while others PR into its repo? | "Shared merge lock" |

## What changes

1. **"When a program has finished" (§0.13).** devices and 3d have finished when every goal in
   their lanes manifests (`.claude/goals/*.lanes.toml` on their `origin/main`) without
   `parked_until` has its `COMPLETE (goal <n>)` line. A program without a manifest keeps the old
   test (its last goal's `COMPLETE` line). Today both have finished: devices since its goal 4
   (glide6 goal 2 is parked until the owner's fit-kit picks), 3d since its goal 7 (goal 8 is parked
   until the owner's PLA campaign). The check is the script in devices'
   `.claude/goals/AMENDMENT-2026-10-01.md` item 3, run with the repo's path; once the rebuilt image
   carries it, `claude-goal-chain lanes /workspace/<repo>`.
2. **The devices seam (K23, P14).** devices has finished, so P14's question is moot: chorus opens
   PRs in devices, limited to `builds/chorus-*` and the inventory part records those builds need,
   under devices' rules, `make check` and `make invariants`, holding
   `flock /cache/locks/devices-merge.lock` from the final `git pull --rebase` to `gh pr merge`.
   Draft branches in chorus are only the fallback for when the check above fails. Row `chorus-1`'s
   deferral is met.
3. **The speaker split (K23, K22, K88 amended), goals 24-26.** chorus designs the acoustics: the
   drivers (priced, US-first), the box alignment and port, the baffle step, the crossover, the DSP
   EQ through chorus, and the acoustic design record, with shopkit-acoustics; P12 and P13 stay
   chorus's proposals. chorus lands that design in `builds/chorus-<build>/` as a devices PR (item
   2). The enclosures, boards and BOMs, with their inventory part records, are request rows to
   `## To devices` in shopkit's `PROGRAM-REQUESTS.md`, one per build, each with its acceptance
   check (the class's budget, K89; the PoE class against the switch budget, K90; the controls; the
   mic's hardware mute), served by devices' Audio lane goals. A done-when line of goals 24-26 that
   asks for an enclosure, a board, a BOM or part records is met by that filed row (row id and
   acceptance check shown); "landed" means the acoustic design as a devices PR.
4. **3d.** 3d has finished, so chorus may serve its own rows to 3d by PR in 3d, under 3d's rules and
   gate, holding `flock /cache/locks/3d-merge.lock`; prints stay the owner's.

## Addendum: the devices 2026-10 program (devices J9, J20; the owner, 2026-10-01)

The owner's answer on the seam, verbatim: "chorus may PR anytime (Recommended)", the option that
read "chorus PRs `builds/chorus-*` (and their part records) into devices whenever it's ready, under
devices' gate and devices-merge.lock, whether or not devices has finished". Declined: "chorus waits
for finished". The devices side is `NSchatz/devices` `.claude/goals/2026-10-devices.md` §0.13.

5. **devices has open agent goals again.** Its 2026-10 program (`.claude/goals/2026-10-devices.md`)
   leaves item 1's check printing "not finished" until its goals 1, 2, 7, 11 and 17 are COMPLETE.
   Nothing in chorus waits on that any more (item 6).
6. **P14 is approved (devices J20).** chorus opens PRs in devices at any time, whether or not devices
   has finished, adding only `builds/chorus-<build>/acoustics.md` and its shopkit-acoustics export
   (and, in inventory, new part records for the drivers that file names, under
   `inventory-merge.lock`, landed first). Hold `flock -o -w 1800 /cache/locks/devices-merge.lock`
   from the final `git pull --rebase` to `gh pr merge`; lock order `shopkit-release`, then one of
   `shopkit-merge`, `inventory-merge` or `devices-merge`, then `chorus-heavy`. Gate: devices
   `SHOPKIT_WORKERS=8 make check` under `chorus-heavy`; `make invariants` only when the diff touches
   devices brief §4.2's paths. Enclosure models, `bom.csv`, `log.md`, renders, boards and every other
   part record are devices' Audio lane's (its goals 14-16), by row. A draft branch is only the
   fallback when devices' gate stays red after three fix rounds. Every "devices has finished or P14
   was approved" in the brief reads true.
7. **The file devices waits on.** Each build's acoustic design record lands at exactly
   `builds/chorus-<build>/acoustics.md` for `chorus-compact-v1`, `chorus-twoway-v1`,
   `chorus-sub-v1`, `chorus-rackamp-v1`, `chorus-soundbar-v1` and `chorus-lcr-v1`, with its
   shopkit-acoustics export beside it under the name goal 23 gives it. devices' goals 14-16 start
   when these paths are on devices `origin/main`. The file lands once, complete: a `version:` line;
   the drivers (make, model, vendor, URL, price, date, ship-from); the endpoint module chorus's
   firmware targets (P1); net volume, port, baffle step, crossover, DSP EQ and the tolerances an
   enclosure must hold; P12's path. A later change bumps `version` and files a new row. Goal 26 lands
   the rack amp's, soundbar's and LCR's files in one devices PR.
8. **Rows.** `chorus-1` is `ACCEPTED (2026-10 devices goals 14-16)` (devices sets it); the per-build
   rows of goals 24-26 are served by devices goals 14-16; devices' row naming item 7's paths
   (`dev-14`, filed by devices goal 1) is accepted for goals 24-26.
