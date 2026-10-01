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
