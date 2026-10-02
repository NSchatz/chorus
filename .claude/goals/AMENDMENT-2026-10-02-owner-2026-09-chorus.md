# Amendment to the 2026-09-chorus program: onto spec v1.1, the brief compacted, the goal files off the old lists (the owner, 2026-10-02)

The owner decided this on 2026-10-02, in the goals session's own chat. After an audit of every repo
against the goal-process spec (NSchatz/goals spec v1.1), the owner wrote **"Everything needs to get
ported over"**. Asked how far to go for the programs with goals left, the owner chose **"Swap +
compact briefs"**: "rewrite each running brief onto the spec: drop its copied contract and keep only
its own parameters (lint's 'planned' profile)". That session wrote this change, and it merges into
chorus during a drain of this program. The owner holds the program; goal 13 is complete and no goal is
running. These are the owner's later words, so they beat the brief and the earlier amendments where
they disagree (spec amendment.md, "Precedence"). Unlike those amendments, this one changes the
remaining goal files themselves, by the owner's choice.

- **Spec version:** 1.1. The pin is the new manifest `2026-09-chorus.lanes.toml`, whose only key is
  `spec = "1.1"`; `goals lint` reports "spec 1.1 planned".
  - The manifest has no `[[goal]]` entries, so the goals still run in order, goal n after goal n-1.
  - It has no `[checkpoints]`: Checkpoint K and its approval file are unchanged. That file predates
    the spec, and lint keeps it as a record.
- **In force for:** goals 14 to 27, whose ledgers are created after this merged.

## What changes

1. **The brief references the spec instead of copying the standing contract.**
   - It has a `**Spec:**` line, and the introduction carries the spec's must-read paragraph.
   - §0 is "Program parameters". It keeps this program's own rules, with every `### 0.x` number:
     - the access list (K49);
     - the gates and their tiers;
     - chorus-heavy and its lock order;
     - the bench and flashing guards (K4, K28, K93), eFuses and `CHORUS_OWNER_AT_BENCH`;
     - the clean-room and identity rules (K27, I5, I6, I18);
     - the agent budget (K29);
     - the scope guards;
     - the environment table;
     - the devices seam (K23, P14 and the owner's 2026-10-01 paragraph);
     - the K-decisions §0 carried.
   - §33, the report formats, points at spec report.md and keeps chorus's (foundation) and
     INCOMPLETE rule.
   - §1-§4, the sections of goals 1 to 13, §32, §34 and §35 are byte for byte unchanged.
2. **The state words stay chorus's own.**
   - NEEDS-OWNER is the spec's state for a step physically impossible for an agent.
   - NEEDS-OWNER-PROGRAM is the spec's state for waiting on another program's unserved request.
   - "The owner's queue" is the issues in NSchatz/goals (`goals needs add`, `/goals:needs`). Its
     label is never written here (K27).
3. **Rules the owner's port retires, or the spec settles, are gone from §0:**
   - "§0-§4 are the contract";
   - reading the old request and Needs files at a goal's start;
   - the requests-as-rows protocol and its ledger-only commits to shopkit `main` for rows and Needs
     lines (the K6 and K49 permissions for those files), which the spec kept only while a remaining
     goal file named them;
   - the shared Needs list, its retried commits and its search-the-file "ask once";
   - "Goals never open issues", now that the owner's queue is issues;
   - a heavy job under `chorus-heavy` alone. A heavy job now holds `goals-heavy`, then the gated
     repo's own heavy lock (spec locks.md): `chorus-heavy` for chorus's own work, `dev-kernel` for
     devices' gate and `3d-kernel` for 3d's. This replaces `AMENDMENT-2026-10-01.md` item 6's "under
     `chorus-heavy`" for devices' gate. Inside chorus one heavy job runs at a time (K29).
4. **Goal files 14 to 27.**
   - Each reads "spec v1.1 (the /goals:spec skill), `.claude/goals/2026-09-chorus.md` §0, §4 and
     §<k>" instead of §0-§4.
   - Physical steps go on the owner's queue.
   - The closing sentence is the spec's, in chorus's state words.
   - Every other line keeps its meaning. The lettered lines that changed are listed below.
   - Goals 1 to 13 are unchanged.
5. **The old lists.** shopkit's human-only-steps list and the request rows of `PROGRAM-REQUESTS.md` were the
   issues' mirror. No remaining goal file of this program names them as the place to write, so they
   become pointers. `PROGRAM-REQUESTS.md` keeps its package owner table.

## What it supersedes

The "Left in force (goal files win)" lists of the goals-g5, g6, g8, g9 and g10 amendments, where they
quote goal-file wording this change replaced: the rows, the `## To chorus` tail, and the Needs list
and its tail. Their other rules stand, and their top notes stay in the brief.

## The lettered lines changed (goal, letter)

- **Requests line, in all fourteen goals** (g14 G, g15 E, g16 D, g17 F, g18-g20 D, g21-g22 E,
  g23-g26 D, g27 G): "request(s) ... (the REST list)" instead of rows and "(the `## To chorus`
  tail)".
- **Clean-repos line, in all fourteen goals** (the letter after the Requests line): "the owner's
  queue is current ... the queue's REST count" instead of the Needs list and its tail.
- **g14 E:** "a packet in the owner's queue (test tail and issue)".
- **g17 E and g19 C:** "issues in the owner's queue (URL and issues)".
- **g21 D:** "the phone check is filed in the owner's queue (... the issue)".
- **g24 C:** "its request updated (... and request)".
- **g27 F:** "are filed in the owner's queue (the issues)".

## Measured

- `goals lint` on the branch: "spec 1.1 planned, manifest ... 0 failed", with the lint of NSchatz/goals
  at adbc76d.
- Must-read per goal (`goals lint --must-read`): 107,870 to 112,734 bytes before; 64,687 to 69,551
  bytes after. The after figure includes the spec's 12,253 bytes.
- The largest goal file is goal 26, at 3,784 characters.
