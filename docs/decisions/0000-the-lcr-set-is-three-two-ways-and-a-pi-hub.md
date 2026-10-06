# 0000: the LCR set is three unchanged two-ways and a hidden Raspberry Pi 5 hub with a Digi+ I/O on the TV's optical output and CEC over the Pi's HDMI; 770.35 USD a set, 1573.20 USD for both TV rooms

- Status: proposed; it writes down the plan the owner approved on 2026-10-06 (goals project 33,
  planning task 95: "LCR set = 3x two-way + the hidden TV hub (Pi, Digi+ I/O, CEC over HDMI)")
- Recorded by: the owner's agent harness, task 243, in the pull request that adds this record
- Implemented in: `docs/hardware/lcr-set.md` version 1; a pointer in
  `docs/hardware/twoway-speaker.md`

## Context

K72 asks for a separate-LCR set: "three clean speakers with no visible controls; TV inputs on a
hidden hub". P2 settled the TV path as Option A: stereo LPCM from optical or ARC, with CEC through
the kernel API on a Linux hub. P13 and decision 0231 made the LCR set the front of both TV rooms
and left the soundbar undesigned. P4's house plan says both TVs have an optical output and that
the two bench Pis (F1, F2) become the hubs, with H1 to H4 bought at install.

## Decision

1. **The speakers are `chorus-twoway-v1`, unchanged,** at its designed (good) tier. The centre
   is the same speaker; laying it on its side is the room's and is measured in place.
2. **The hub is a Raspberry Pi 5 with a HiFiBerry Digi+ I/O** on the TV's optical output, CEC over
   the Pi's own HDMI port into any TV input (no ARC), wired Ethernet and the 27 W supply. It runs
   `chorus-client --line-in ... --line-in-kind optical --cec ...` and is not a member of the
   room's bonded set.
3. **The room is one theater bond:** `FL FC FR` on the two-ways, `LFE` on `chorus-sub-v1`, `SL
   SR` on compacts. A stereo TV plays its centre from the passive matrix and its surrounds as the
   room's `tv_upmix` says.
4. **The price is the two-way's own total, not re-derived:** 203.92 USD each. One set is 770.35
   USD with the 2GB Pi, the living room's 802.85 with the 4GB, both 1573.20. The Digi+ I/O is the
   one NON-US EXCEPTION line: no US seller listed it on 2026-10-06.
5. **The hub's case is asked for in ASA (PETG fallback), not P4's PLA,** following P12's rule
   that PLA is not used next to a heat source. It is passive and vented, with the bench measuring
   the SoC temperature behind a running TV before a fan is added.

## Not chosen

- **A DIR9001 receiver module instead of the Digi+ I/O.** P2 names it; its price was never read
  and it needs wiring and an overlay of its own, where the Digi+ I/O is a HAT P4 already lists.
- **A Pulse-Eight USB-CEC adapter from the start.** The Pi's own HDMI CEC costs nothing extra; the
  adapter stays P4's fallback if a TV will not answer it.
- **The hub inside one of the two-ways (the centre's carrier).** It would put the TV's optical and
  HDMI cables into a speaker and a Pi into a sealed box; the hub is kept behind the TV, where its
  cables are.
- **A PLA case as P4 lists.** P12's heat argument applies to a case behind a TV; PLA stays
  acceptable for a first fit print.

## What was read

- `docs/hardware/twoway-speaker.md`, `docs/proposals/P2-theater-scope.md`,
  `docs/proposals/P4-bench-purchase.md`, `docs/proposals/P12-enclosures.md`,
  `docs/linux-endpoint.md`, `docs/cec.md`, `docs/dsp.md`, `docs/control-plane.md`,
  `docs/inputs.md` and decision 0231 at `origin/main` 9144a4f, read 2026-10-06.
- The sellers' and makers' pages cited in `docs/hardware/lcr-set.md`, read 2026-10-06 (the
  TOSLINK cable's at 2026-10-04, through P4).
- goals project 33 and task 243, read 2026-10-06.
