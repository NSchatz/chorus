# 0000: the rack amp has zero zones and zero channels and is not designed, and the soundbar is not designed; the LCR set is the theater front for both TV rooms

- Status: proposed (P13, `Status: PROPOSED`); the count rests on the owner's room list of
  2026-10-04, and the plan that writes it down was approved by the owner on 2026-10-06 (goals
  project 33, planning task 95)
- Recorded by: the owner's agent harness, task 242, in the pull request that adds this record
- Implemented in: `docs/proposals/P13-rack-amp-zones.md`; BRIEF.md section 8.1 (the Linux endpoint
  tier and speaker design rows); `docs/hardware/linux-multichannel.md` ("The rack amp: several
  zones on one card")

## Context

Goal 26 (program brief §30) asks for P13, the 2U rack amp's zones and channels from the room list
(K74), a rack amp package (K70, K74, K96), and the theater front in two variants (K72): a soundbar
and an LCR set with a hidden hub. On 2026-10-04 the owner gave the house plan: six rooms and 19
powered speakers on Ethernet, and "The rack amp (K70, K74) is dropped: no room in the plan wires
speakers back to the rack" (`docs/proposals/P4-bench-purchase.md`; the same list in
`docs/proposals/P12-enclosures.md`). Both TV rooms are 5.1 with two-way L, C and R.

## Decision

1. P13 records zero rack amp zones and zero channels. No rack amp package is designed and
   `builds/chorus-rackamp-v1/` is not made.
2. The soundbar is not designed: no room on the list uses one. The LCR set (three two-ways and the
   hidden TV hub, P2's Option A hub on optical) is the theater front for both TV rooms; its own
   document is the next task.
3. BRIEF.md stops promising a rack amp or soundbar package; `docs/hardware/linux-multichannel.md`
   keeps its rack amp section as the design a rack amp would use, with its example still ASSUMED.
4. What revives them is written in P13: a room whose passive speakers are wired back to the rack
   (then its zone and channel count, K70/K74/K96's feature list and the multichannel page's rack
   amp section); a TV room the owner wants without separate L, C and R (then K72's soundbar
   within P2's settled scope).

## Not chosen

- **Designing the rack amp anyway, parameterised.** It would price and land a product no room
  uses; the room list that K74 waited on now says zero.
- **Designing the soundbar anyway.** No room has one; P2's soundbar ARC input would also need the
  unapproved Option B board.

## What was read

- `docs/proposals/P4-bench-purchase.md`, `docs/proposals/P12-enclosures.md`,
  `docs/proposals/P2-theater-scope.md` and `docs/hardware/linux-multichannel.md` at `origin/main`
  01ef1c8, read 2026-10-06.
- The program brief, §1 (K70, K72, K74, K96) and §30:
  https://github.com/NSchatz/chorus/blob/535ed2816b5735153ac81c52a235024aa806f2b5/.claude/goals/2026-09-chorus.md,
  read 2026-10-06.
- goals project 33 and task 242, read 2026-10-06.
