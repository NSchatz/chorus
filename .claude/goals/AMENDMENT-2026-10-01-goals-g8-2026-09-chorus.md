# Amendment to the 2026-09-chorus program: the owner's queue and requests as issues (goals program, goal 8, 2026-10-01)

Written by the goals program (NSchatz/goals) (its brief, sections 12 and 0.13, goal 8, R3) and merged into chorus between goals, during a drain of this program (no 2026-09-chorus goal running). It applies to every 2026-09-chorus goal whose ledger is created after it merged, and **never against a line of that goal's goal file**: where a goal file names a file, section, label or rule, that line stays in force for that goal (the list below). A goal already running when it merged keeps the contract it started with.

- **Spec version:** 1.0
- **What it supersedes:** nothing is removed. The lists this program writes (shopkit `NEEDS-NOAH.md` (the Needs list, brief §0.6) and shopkit `PROGRAM-REQUESTS.md` (`## To chorus`, `## To devices`)) stay live; from the next goal each item and request is also an issue in the owner's queue: issues in NSchatz/goals, made from the list by one command, and the requests addressed to chorus are served from those issues as well as from the rows.

## The owner's words it carries out (the goals interview, 2026-10-01)

| Row | Decision, as recorded |
|---|---|
| W21 | the owner's queue: **One GitHub issue per item in the goals program's repository**, labelled by repo and kind (physical, purchase, credential, decision), safety first; the owner answers/closes from the GitHub app; a push on each new one (W9) |
| W25 | Program-to-program requests: **Issues in the goals program's repository** (labelled from/to, state, acceptance check); every goal serves the open issues addressed to its program |
| W37 | Existing the owner items and requests: **Migrate all, de-duplicated**: every open the owner item and request row becomes one issue in the goals program's repository, linked to its source; old files become pointers |
| W7 | **Between goals, self-merged**: no goal sees its contract change mid-run; no per-amendment approval |

As later words of the owner these beat earlier lines where they conflict, except where a goal file wins.

## What changes

`G="${GOALS_STATE_DIR:-/cache/goals/${CLAUDE_PROJECT_NAME:-$(hostname)}}/bin/goals"` (the goals command the supervisor runs).

1. **Filing a human-only step.** Write it in the list the goal file names, as before, and push it. Then run `"$G" needs sync --repo shopkit` (or, before the push, `"$G" needs sync --repo shopkit --tree <your worktree>`): each open item of the list that has no issue becomes one, labelled `needs-noah`, `repo:<name>` and `kind:<physical|purchase|credential|decision>`, with a link to its line, and the owner gets a push. The command asks once (an item already filed is never filed again); the supervisor runs the same sync every 15 minutes. A step no list of this program holds is filed with `"$G" needs add --repo chorus --kind <kind> --title ... --do ... --tool ... --expect ... --changes ...`.
2. **Ending a step.** The owner says it in a session's chat; that session marks the item done in the list as the list's own rule says; the next sync closes the issue. Closing an issue alone records nothing.
3. **Requests.** A request row is written as the goal file says, then made an issue by `"$G" needs sync --repo shopkit`; a request with no row is filed with `"$G" request add --from chorus --to <owner> --title ... --what ... --acceptance ...`. Each goal serves the requests addressed to chorus that were open at its start, from the rows and from `"$G" request list --to chorus` (the REST list, never search), and moves each one on in both places: the row's state cell, and `"$G" request state <n> <accepted|deferred|done|declined> --by chorus --note "<goal n, PR/sha, follow-up or why>"`. Only the addressee changes a state.
4. **Ownerless requests.** A request still open when its owner's program has finished is served by the requester, under the owner's rules and gate, citing it.
5. **The mirror.** Each list carries a generated block at its end, `## Issues (mirror)`, rewritten by the sync from the issues' states. Never edit it by hand; edit the list above it. The list becomes a pointer only after this program (and every other program that names it) has finished.

## Left in force (goal files win)

- Each remaining goal file's first line (g14-g27): "Physical steps go on the Needs list (§0.6); carry on.": the Needs list stays shopkit `NEEDS-NOAH.md`; the issue is made from it.
- The done-when lines "...the Needs list is current..." (g14-g27) and the lines that file Needs items (g17 L8, g19 L6, g21 L7, g27 L9): the list, as written.
- The done-when lines "Requests: every row addressed to chorus is DONE, ACCEPTED (goal named), DEFERRED (follow-up listed) or DECLINED (why); rows it filed are listed with their state (the `## To chorus` tail)" (g14-g27): the rows, as written.
- g24, g25, g26 line 1: "one request row per build to devices' Audio lane (`## To devices`) with acceptance checks": the row, as written.
- The NEEDS-OWNER state words of the goal files (g14 L5 and the last lines): unchanged.
