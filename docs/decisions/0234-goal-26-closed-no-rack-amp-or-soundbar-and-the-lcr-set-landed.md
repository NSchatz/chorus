# 0234: goal 26 is closed: P13 is written (zero zones, zero channels, PROPOSED), the rack amp and the soundbar are not designed, and the LCR set landed in the devices repo with its totals

- Status: decided, 2026-10-06 (harness task 245, project 33)
- Recorded by: the owner's agent harness, in the pull request that adds this record
- Implemented in: chorus pull requests 231 (P13 and decision 0231) and 232 (the LCR set and
  decision 0232); the devices repo, pull request 128 (devices harness task 244); nothing in chorus
  changes but this record and its index line

## Context

Goal 26 (the 2026-10 program's section 30, retired with the goal-program files in #141) asked for
P13, the 2U rack amp's zones and channels from the room list (K74), marked PROPOSED; a rack amp
package (K70, K74, K96); and the theater front in two variants (K72), a soundbar and an LCR set
with a hidden hub. The old feature's finish line (goals issue #328) is that these are written or
landed. The owner's house plan of 2026-10-04 dropped the rack amp and puts a soundbar in no room,
and the owner approved the plan that writes this down on 2026-10-06 (goals project 33, planning
task 95). This record makes the finish line checkable from chorus's main.

## Decision

Goal 26 is closed:

| Item | Where it is | What it says |
|---|---|---|
| P13 | `docs/proposals/P13-rack-amp-zones.md`, 112 lines | line 4, `Status: PROPOSED (2026-10-06)`, "zero zones and zero channels"; lines 50 and 52, "Zones: 0." and "Channels: 0." |
| The rack amp | not designed: decision 0231, item 1; P13, "Zones and channels" | no room wires passive speakers back to the rack, so no zone, no channel, no package, and no devices `projects/chorus-rack/v1/` |
| The soundbar | not designed: decision 0231, item 2 and "Not chosen"; P13, "The theater front: the soundbar is not designed" | no room on the owner's list uses one; P2's soundbar ARC input would also need the unapproved Option B board |
| The LCR set | `docs/hardware/lcr-set.md` (`version: 1`), decision 0232 | three unchanged `chorus-twoway-v1` speakers and the hidden Raspberry Pi 5 hub |
| Landed in devices by | https://github.com/NSchatz/devices/pull/128 (task 244) | `projects/chorus-lcr/v1/acoustics.md`, transcribed from chorus `e00b730` |
| Per-set total | 770.35 USD (the 2GB hub) | the same in chorus's doc and the devices record |
| Two-room total | 1573.20 USD (the master bedroom's 2GB hub and the living room's 4GB hub) | the same in chorus's doc and the devices record |

The totals are re-read from `docs/hardware/lcr-set.md` on chorus main at `b4cae7b` ("The bill of
materials"), unchanged since `e00b730`, the commit the devices record names; they rest on the
two-way's PROPOSED budget, so they stay PROPOSED, the owner's to decide. What revives the rack amp
or the soundbar is written in P13 and decision 0231, item 4.

BRIEF.md is not changed: it carries no rack amp, soundbar or LCR number that a landed one
contradicts (section 8.1 was corrected in #231).

### What stays open (named, not implemented here)

The LCR set's "Open items" stay open (the Digi+ I/O's capture and the CEC device on the bench,
CEC volume from each TV, the hub's temperature in its case, the case's sizes and the TVs' VESA
patterns, the centre on its side), and its totals carry the two-way's endpoint board line (the
Brick at 59.00 USD, listed "No longer available" on the day read), which moves L1 and both
totals ("What the totals do and do not say"). The hub's case model, `bom.csv` against
inventory, log and renders are the devices repo's (its project 8), not chorus's.

## Not chosen

- **Closing the goal on the harness tasks' state alone.** It would leave the finish line
  checkable only in the harness, not from chorus's main.

## What was read

- Harness tasks 245 and 244 and goals project 33, read 2026-10-06.
- chorus `origin/main` at `b4cae7b`: `docs/proposals/P13-rack-amp-zones.md`,
  `docs/decisions/0231-no-rack-amp-and-no-soundbar.md`,
  `docs/decisions/0232-the-lcr-set-is-three-two-ways-and-a-pi-hub.md`,
  `docs/hardware/lcr-set.md` ("The bill of materials", "What the totals do and do not say", "Open items") and BRIEF.md section 8.1,
  read 2026-10-06.
- The owner's devices repo, `origin/main` at `4858734`: `projects/chorus-lcr/v1/acoustics.md`
  (its header and totals); its merged pull request 128, read 2026-10-06.
