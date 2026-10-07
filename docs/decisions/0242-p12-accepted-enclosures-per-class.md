# 0242: P12 is accepted as written: the compact printed, the two-way and the LCR set in braced birch plywood with printed fittings, the subwoofer in braced MDF

- Status: decided by the owner, 2026-10-07 (harness task 317, filed by task 263, the 2026-09
  program report); accepts `docs/proposals/P12-enclosures.md` (K88), PROPOSED since 2026-10-06
- Recorded by: the owner's agent harness, in the pull request that adds this record
- Implemented in: the devices repo, which owns the enclosure models (the amendment of 2026-10-01,
  item 3); nothing in chorus changes beyond the documents this record updates

## Context

P12 answered K88 per speaker class: wood from the owner's shop, printed on the owner's printer,
or mixed. It recommended Option C, mixed by class. Two things had moved since it was written:

- The devices repo asked the owner about the compact's enclosure first (devices task 35, then
  task 51; devices `docs/decisions.md`, 2026-10-06). The owner chose P12's compact answer,
  printed, with its construction; devices builds it in PETG, P12's own indoor fallback, because
  shopkit's rule `fdm:P29` prints ASA for car parts only. That settled devices' build, not P12.
- Goal 26 closed with no rack amp and no soundbar designed (decision 0234), so P12's soundbar
  and rack amp lines no longer have a build.

## What was read

All on 2026-10-07.

- `docs/proposals/P12-enclosures.md` (Option C, the compact section, If deferred, open items).
- The devices repo's `docs/decisions.md` on its main branch: "chorus-compact-v1's enclosure
  material" (task 35) and "chorus-compact-v1's enclosure is PETG, not ASA" (task 51).
- Decision 0234, `docs/parity.md` row K88, BRIEF.md's open line on enclosure material.

## Options the owner was given

Only what was still open was asked: the two-way, the LCR set and the subwoofer.

1. Accept P12 as written (recommended).
2. Change the subwoofer to 18 mm birch plywood too (about 14 USD more per box, a clear finish
   instead of paint).
3. Defer: P12 stays PROPOSED and its "If deferred" line stands.

## Decision

1. **Option 1: P12 is ACCEPTED as written,** Option C, mixed by class:
   - the compact: printed, 8 mm walls with four perimeters and 60% infill, ribbed to 80 mm, one
     body and one gasketed panel, net volume held to ± 2% and leakage to `ql` of 7 or better.
     P12 names ASA; the material printed now is PETG, as devices decided, and ASA returns only
     through a shopkit exemption to `fdm:P29`. The construction and tolerances do not change.
   - the active two-way and the LCR set: braced 18 mm birch plywood, no unbraced span over about
     200 mm, with printed ports, flares, electronics carriers and router templates.
   - the subwoofer: braced 18 mm MDF with a doubled (36 mm) baffle, painted; a printed port flare
     if vented.
   - the soundbar and the rack amp: no build (decision 0234), so P12's lines for them stand as
     written and apply to nothing.
2. **No class changed,** so no new enclosure task goes to the devices repo; its existing builds
   already follow P12's recommendation.
3. **What stays ASSUMED:** P12's cut tolerance on the owner's saw, the wood volume figure, the
   sealing and ringing of printed walls and the shop facts it lists. Accepting the proposal does
   not measure them; the first box of each class does (P12's open items, BRIEF 3.1).
4. **What stays open:** whether the living-room compacts may look printed or want a wood or
   fabric face, the outdoor pair's class, and the recalculation of P12's illustrative costs at
   the designed volumes (1.80 L, 9.29 L, 73.54 L). None blocks a build.

## Consequences

- `docs/proposals/P12-enclosures.md`'s Status line reads ACCEPTED; `docs/parity.md` row K88 is
  done; BRIEF.md's line on enclosure material says so.
- `docs/hardware/compact-speaker.md`, `twoway-speaker.md` and `subwoofer.md` no longer call their
  enclosure path ASSUMED from a PROPOSED P12; their tolerance arithmetic is unchanged.
