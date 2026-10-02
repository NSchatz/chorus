# Amendment to the 2026-09-chorus program: shared packages and releases (goals program, goal 10, 2026-10-02)

Written by the goals program (NSchatz/goals) (its brief, sections 14 and 0.13, goal 10, R1 and R2) and merged into chorus between goals, during a drain of this program (no 2026-09-chorus goal running). It applies to every 2026-09-chorus goal whose ledger is created after it merged, and **never against a line of that goal's goal file**: where a goal file names a file, command, gate, label or rule, that line stays in force for that goal (the list below, and the lists of the earlier goals amendments). A goal already running when it merged keeps the contract it started with.

- **Spec version:** 1.0
- **What it supersedes:** in the brief's section 0.13, the rule that a change in a shopkit package another program owns is asked for as a request and waits for its owner (the ownership table's use, K44's "joins shopkit's ownership table"), and the last sentence of the paragraph "Releases of shopkit (for `shopkit-acoustics`)": "At most one shopkit release per goal". The brief, its goal files, the earlier amendments and every checkpoint file are otherwise unchanged.
- **In force from:** the day every package-owning program with goals left (chorus 2026-09 and devices 2026-10; home and 3d have finished) carries its goal-10 amendment on its origin/main. `goals ship shopkit --in-force` prints that day, or which program is still missing; shopkit's `CLAUDE.md` carries the same line. Until then the brief's rules stand as written.

## The owner's words it carries out (the goals interview, 2026-10-01)

| Row | Decision, as recorded |
|---|---|
| W22 | **"Goals should be able to work across shared repos. For example, shopkit is shared across the repos."** |
| W23 | Ownership in shared repos: **Any goal, guarded**: any goal may change any shared package (shopkit, etc.) in its own PR if the shared repo's gate passes and every consuming repo's tests pass against the change; owners remain the record of who knows the code |
| W24 | Shipping a shared-library change: **One goal, ordered PRs**: the same goal does library PR -> release tag -> bump and test each consumer, under one release lock; no cap on releases per goal; pins stay reproducible |
| W7 | **Between goals, self-merged**: no goal sees its contract change mid-run; no per-amendment approval |

As later words of the owner these beat earlier lines where they conflict, except where a goal file wins.

## What changes

1. **Any goal may change any shopkit package (W23).** A chorus goal that needs a change in a package another program owns makes it in its own shopkit PR instead of filing a request and waiting, when both pass on the branch up to date with shopkit's `origin/main`:
   - shopkit's gate (`make ci`, under `shopkit-merge`), its tail in the PR body;
   - `goals consumers-test shopkit --ref <branch>`: the fast tier (`make tier-fast`) of every repo that pins shopkit (found from the pins in each repo's `pyproject.toml`, never from a list), run against the branch under `goals-heavy`. Each consumer's line prints, from inside its own test processes, the path of the shopkit it imported; a run that imported the pinned release is a failure, never a pass. Its tail goes in the PR body too.
   The same holds the other way: a goal of another program may change `shopkit-acoustics` under the same two checks.
2. **Owners stay the record of who knows the code.** The ownership table (shopkit `PROGRAM-REQUESTS.md`) is unchanged and now says whom to ask and whose `CLAUDE.md` and rules to read before changing a package. "Add or deprecate, never break" still binds every change. A request is still the way to ask an owner to do the work; it is no longer required before a guarded change.
3. **One goal ships (W24).** `goals ship shopkit --branch <branch>` does, in one goal and holding `shopkit-release` from start to end: the library PR (item 1's two checks, then the merge), the release (the next free version after the newest tag on origin, `python tools/release.py set`, `uv lock`, `make ci`, the release PR, the tag on its merge commit; a tag is never moved and never re-used) and each consumer's pin bump through that consumer's fast tier. There is **no cap on releases per goal**. The brief's steps by hand, under the same lock, stay valid without the cap.
4. **A consumer bump never lands mid-goal.** A bump into a repo whose program has goals left is gated, queued (`goals queue`) and merged in a drain, so it is there before that program's next goal starts; a bump into a finished program's repo merges at once under that repo's merge lock; the goal's own repo it bumps itself. chorus pins no shopkit package today, so nothing is bumped here.
5. **Never against a goal file that limits a goal to its own packages** (3d goal 8: "Edit only 3d-owned shopkit packages"): such a goal keeps asking by request.

## Left in force (goal files win)

- g23 scope: "shopkit-acoustics under shopkit's rules, gate and release": shopkit's rules, as its `CLAUDE.md` states them on the day goal 23 starts.
- g23 A: "shopkit-acoustics is merged under shopkit's gate with worked-example tests (PR URL and the make ci tail)": `make ci`, as written.
- g23 B: "It is released per shopkit's protocol (tag and release PR URL)": a tag and a release PR, which item 3 produces.
- Every goal file's requests line ("every row addressed to chorus is DONE, ACCEPTED (goal named), DEFERRED (follow-up listed) or DECLINED (why)"): requests addressed to chorus are served as before.
