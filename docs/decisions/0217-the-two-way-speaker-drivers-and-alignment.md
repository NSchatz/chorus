# 0217: the active two-way is a vented box on one TAS5825M: a Dayton Audio DC160-8 woofer and DC28F-8 tweeter (the good tier of three priced tiers) in 9.29 L tuned to 41.5 Hz (the QB3 alignment the acoustics package designs), crossed over LR4 at 2000 Hz, one amplifier channel per driver, with a 3 dB baffle-step shelf at 550 Hz

- Status: accepted, 2026-10-06. A paper design: nothing is built or measured, and every number
  is a starting point the first box confirms or corrects.
- Decided by: the task for the scope ("a mains-powered speaker with one woofer and one tweeter
  on TAS58xx amplification, drivers chosen and priced US-first with good, better and best
  alternatives, the box alignment with net volume and port (or sealed, with the reason), the
  baffle step, the LR4 crossover, the DSP EQ, and the tolerances an enclosure must hold, all
  computed with the released acoustics package"). Which drivers, which tier is designed, which
  alignment, which crossover frequency and how the drivers sit on the amplifier are this
  record's. They are cheap to reverse while nothing is ordered: a change is a new `version` of
  the design file and a re-exported record. The class's parts budget and the tier that is
  bought are not decided here: the design file proposes them and the owner decides.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `docs/hardware/twoway-speaker.md` (the design, version 1),
  `docs/research/twoway-speaker-drivers.md` (the survey),
  `fixtures/design-record/chorus-twoway-v1.json` and `.provenance`,
  `crates/dsp/tests/design_record.rs` (`the_twoway_record_is_the_exported_one_and_runs`),
  `fixtures/README.md`, `docs/dsp.md`.

## Context

The active two-way (K22) is mains-powered with wired Ethernet (K90) and shows no controls
(K68). It has no decided parts budget: K89 caps the compact and asks the other classes for
"budgets proposed with priced alternatives". P1 (PROPOSED) gives it the compact's bought
ESP32-S3 + TAS5825M + W5500 board, "two amp channels for a two-way"; P12 (PROPOSED) gives it a
braced 18 mm birch plywood box with a printed port. The owner's room list has six of them: the
left, centre and right of two 5.1 rooms, each beside a subwoofer.

The released acoustics package (v1.49.0, still the current release) designs sealed boxes and
two vented alignments (QB3 and B4), ports, the baffle step, and LR4 and LR2 crossovers; its
design record, version 1, holds crossovers only. One LR4 is all this design needs of it, so
the record holds the design.

## What was read

On 2026-10-06. `docs/research/twoway-speaker-drivers.md` (the survey and its sources: the
seller's item service and the makers' spec sheets). The README of the acoustics package of the
owner's shared Python library at release v1.49.0, and its `baffle` module's notes. In this
repository: `docs/proposals/P1-embedded-platform.md`, `docs/proposals/P12-enclosures.md`,
`docs/hardware/controls.md`, `docs/hardware/compact-speaker.md`,
`docs/decisions/0216-the-compact-speaker-drivers-and-alignment.md`,
`docs/research/tas5825m-register-map.md`, `docs/dsp.md` ("Design records"),
`crates/dsp/src/biquad.rs`, `crates/dsp/tests/design_record.rs`,
`fixtures/design-record/chorus-compact-v1.provenance`, `fixtures/README.md`. TI's TAS5825M
datasheet (pages 1 and 6). The sellers' pages for the budget lines, each cited where
`docs/hardware/twoway-speaker.md` uses it. No GPL source and no reciprocally licensed hardware
design file was opened.

## Decision

1. **Three tiers, and version 1 designs the good one.** Good: DC160-8 with DC28F-8, $59.96 the
   pair. Better: RS180-8 with DC28F-8, $104.96. Best: RS180-8 with Peerless DA25TX00-08,
   $136.23. All at Parts Express, shipping from Ohio, in stock on the day. The good tier is
   designed because its box reaches lowest of the three (46.8 Hz against 53.2 Hz in the same
   size of box), its two sheets leave the widest room for the crossover, and six are to be
   built. The dearer tiers buy displacement (74.8 cm³ against 42.5 cm³) and stiffer
   diaphragms, which a front channel beside a subwoofer needs least.
2. **Woofer: Dayton Audio DC160-8** ($34.98). Of the 8 ohm woofers under $40 in the survey it
   is the one with a Qts (0.34) low enough for a vented alignment the package designs and a
   resonance under 40 Hz (35.7 Hz). At 8 ohms (Re 6.6) it is far above the amplifier's 3.2 ohm
   minimum load, where the compact's woofer sits on it.
3. **Tweeter: Dayton Audio DC28F-8** ($24.98). Usable from 1,300 Hz with a resonance of 834 Hz,
   50 watts, 8 ohms: it can be crossed over where the woofer's sheet still calls its response
   smooth. The $11.49 ND25FA-4 starts at 2,500 Hz with 20 watts.
4. **Alignment: vented, the QB3 box `vented_box_for_driver(woofer, ql=7)` returns:** 9.289 L
   net, tuned to 41.5 Hz, -3 dB at 46.8 Hz. The same volume closed is -3 dB at 76.2 Hz. The
   box the package designs is taken as it is. P12's 15 L was an illustration; 9.29 L is the
   figure.
5. **Port: one 35 mm vent, 154.6 mm long,** printed, flared: at most 11.5 m/s of air at full
   excursion, and short enough to run straight in a cavity 190 mm deep. A 40 mm, 206.1 mm vent
   is the computed fallback.
6. **Crossover: LR4 at 2000 Hz.** The woofer's sheet: "Smooth frequency response up to 2 kHz";
   the tweeter's range starts at 1,300 Hz, and at its resonance the high branch is 30.8 dB
   down. LR4 because it is the alignment chorus's chain already runs (`Lr4`) and its branches
   sum flat. It is the frequency of chorus's ASSUMED two-way example; here it follows from
   the two sheets.
7. **Amplifier arrangement: one TAS5825M in its 2.0 mode, the woofer on one bridge-tied channel
   and the tweeter on the other.** TI's datasheet gives 2 × 30 W into 8 ohms at 24 V (1%
   distortion). Into 8 ohms a channel is limited by the supply's voltage, so a second chip run
   as a bridged-parallel channel, which raises current and not voltage, would give this woofer
   the same 30 W. 30 W is 60% of each driver's 50 watt rating. It is P1's module unchanged.
8. **Baffle step: 3 dB of the 6, as a high-shelf cut at 550 Hz, Q 0.5,** for a 210 mm baffle,
   as in the compact and for its reasons (decision 0216, item 6). The woofer is protected by a
   second-order high-pass at 28 Hz and the tweeter is trimmed by 2.9 dB.
9. **Power: a 24 V supply from the mains, 100 W or more;** the budget prices a safety-listed
   120 W desktop adapter, which keeps mains voltage out of the wooden box. Which supply is the
   electronics plan's.
10. **The record is named `chorus-twoway-v1`** and holds the one crossover. The box, the vent
    and the EQ stay in the design file until a later record schema can carry them.
11. **The class budget is PROPOSED, not decided:** under about $225 of parts at the designed
    tier (the list totals 203.92), about $250 and $280 for the other two.

## Alternatives not taken

- **A closed box.** Simpler and no vent. It gives up 29 Hz of extension in the same volume; a
  two-way that plays alone needs it, and one beside a subwoofer loses nothing by the vent.
- **Designing the better tier** (RS180-8). More output from nearly the same box, at $45 more a
  speaker and 6.4 Hz less extension; its aluminum cone's range ends at 3,600 Hz. It stays a
  priced alternative with its box computed.
- **Two amplifier chips, one bridged-parallel channel per driver** (the reading of "a TAS58xx
  pair"). No more power into 8 ohms at 24 V; a second chip, and a board variant its maker
  calls a prototype. It is the step if a later version takes a 4 ohm woofer.
- **A 4 ohm woofer** for more power from 24 V. Several in the survey have an Re under the
  amplifier's 3.2 ohm minimum, and the 8 ohm DC160-8 already has the deepest box.
- **A lower crossover** (1,600 to 1,800 Hz). Inside the tweeter's stated range, but closer to
  its resonance with no gain the sheets show; a measurement may still move it.
- **A supply inside the box** behind a fused inlet. About $38 cheaper; it puts mains wiring in
  a box the owner builds. Left to the electronics plan.

## Consequences

- The enclosure model has its numbers: 9.29 L net, a 35 mm by 154.6 mm vent, a 210 mm baffle,
  driver centres 145 mm apart or less, and the tolerances table.
- The proposed budget and the tier are the owner's to decide; the design file marks both
  PROPOSED.
- The endpoint board could not be bought at its US seller on the day read (P1's open item).
- Nothing runs the record or the EQ in a chain yet; that is a later task.
- The first box measures everything: tuning, leakage, the shelf, the trim, the woofer's
  excursion above the tuning, the amplifier's temperature.
