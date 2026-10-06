# 0229: the subwoofer is a vented 12 inch box with a bought 100 W plate amplifier: a Dayton Audio SD315A-88 (the good tier of three priced tiers) in 73.54 L tuned to 28.7 Hz (the QB3 alignment the acoustics package designs), a 4 inch vent, a fourth-order protective high-pass at 24 Hz, and chorus's LR4 bass management at 80 Hz

- Status: accepted, 2026-10-06. A paper design: nothing is built or measured, and every number
  is a starting point the first box confirms or corrects.
- Decided by: the task for the scope ("a mains-powered sub with its driver chosen and priced
  US-first in good, better and best tiers, the box alignment (net volume, port and tuning, or
  sealed and why), an amplifier sized by research against the driver's excursion and thermal
  limits, the low-pass and bass management that work with any room's mains, the level and
  phase knobs with what each does in chorus's DSP, the pairing button and status LED, and the
  tolerances an enclosure must hold"). Which driver, which tier is designed, which alignment,
  how much amplifier and which crossover are this record's. They are cheap to reverse while
  nothing is ordered: a change is a new `version` of the design file and a re-exported record.
  The class's parts budget and the tier that is bought are not decided here: the design file
  proposes them and the owner decides. How the endpoint feeds the amplifier is the board
  plan's, and the owner's where it departs from P1.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `docs/hardware/subwoofer.md` (the design, version 1),
  `docs/research/subwoofer-drivers-and-amplifiers.md` (the survey),
  `fixtures/design-record/chorus-sub-v1.json` and `.provenance`,
  `crates/dsp/tests/design_record.rs` (`the_sub_record_is_the_exported_one_and_runs`),
  `fixtures/README.md`, `docs/dsp.md`.

## Context

The subwoofer (K22: "bigger amp, low-pass, bass management") is mains-powered with wired
Ethernet (K90) and carries a pairing button, a status LED and level and phase knobs (K69). It
has no decided parts budget: K89 caps the compact and asks the other classes for "budgets
proposed with priced alternatives". P1 (PROPOSED) gives every ESP32-class speaker one bought
ESP32-S3 + TAS5825M + W5500 board; P12 (PROPOSED) gives this class a braced 18 mm MDF box with
a doubled baffle. The owner's room list has two of them, each beside three active two-ways
that end at 48.5 Hz (decision 0217).

The released acoustics package (v1.50.0; its acoustics code is v1.49.0's) designs sealed boxes
and two vented alignments (QB3 and B4), ports, and LR4 and LR2 crossovers; its design record,
version 1, holds crossovers only. One LR4 is all this design needs of the record, so the
record holds the design. The package computes a displacement-limited power for a closed box
only, and leaves the volume a vent carries to the caller; both are worked out in the design
file as arithmetic on the package's own alignment, and said to be.

chorus's DSP already has the bass management (`docs/dsp.md`, chain item 8): one LR4 at the
room's `crossover_hz`, default 80 Hz, its low branch on the `LFE` role and its high branch on
every main, with a subwoofer level and a polarity. The firmware already reads the two knobs
(decision 0063) and folds them into those settings.

## What was read

On 2026-10-06. `docs/research/subwoofer-drivers-and-amplifiers.md` (the survey and its
sources: the seller's item service, the makers' spec sheets and manuals, the endpoint boards'
product pages, TI's TAS5825M datasheet pages 1, 6 and 9). The README of the acoustics package
of the owner's shared Python library at release v1.50.0. In this repository:
`docs/proposals/P1-embedded-platform.md`, `docs/proposals/P12-enclosures.md`,
`docs/hardware/controls.md`, `docs/hardware/twoway-speaker.md`,
`docs/decisions/0217-the-two-way-speaker-drivers-and-alignment.md`, `docs/dsp.md` (chain item
8, "Design records"), `crates/dsp/src/settings.rs`, `crates/dsp/src/crossover.rs`,
`crates/dsp/tests/design_record.rs`, `firmware/include/chorus/controls.h`,
`firmware/include/chorus/endpoint_dsp.h`, `firmware/src/endpoint_dsp.c`,
`firmware/src/controls.c`, `fixtures/design-record/chorus-twoway-v1.provenance`,
`fixtures/README.md`. The sellers' pages for the budget lines, each cited where
`docs/hardware/subwoofer.md` uses it. No GPL source and no reciprocally licensed hardware
design file was opened.

## Decision

1. **Three tiers of driver with amplifier, and version 1 designs the good one.** Good: SD315A-88
   with a 100 W plate amplifier, $199.96 the two. Better: RSS315HO-44 with a plate amplifier
   that gives 156 W into its 8 ohms, $456.96. Best: the same driver with one that gives 273 W,
   $649.96. All at one US seller, in stock on the day. The good tier is designed because it is
   the loudest of the three at its amplifier's limit (110.7 dB at 1 m against 106.6 and
   109.0 dB), its vent is the only one that is a straight tube, and its two parts together
   cost less than the better tier's amplifier alone. The dearer tiers buy 7.6 Hz of extension from 55%
   of the volume.
2. **Driver: Dayton Audio SD315A-88** ($89.98), its two 8 ohm coils in parallel (4 ohms). Of
   the in-stock drivers whose Qts the package's vented alignments accept, it is the one whose
   box reaches the low thirties with a buildable vent, and it is four times as efficient as
   the dearer driver.
3. **Alignment: vented, the QB3 box `vented_box_for_driver(driver, ql=7)` returns:** 73.54 L
   net, tuned to 28.7 Hz, -3 dB at 32.8 Hz. The same volume closed is -3 dB at 52.8 Hz, above
   the two-ways it is there to extend. The box the package designs is taken as it is. P12's
   50 L was an illustration; 73.54 L is the figure.
4. **Port: one 4 inch (101.6 mm) vent, 326.6 mm long,** flared: 17.5 m/s of air at the
   driver's limit, where the vent carries 2.1 times the cone's displaced volume. Two 3 inch
   vents, 372.2 mm each, are the computed fallback.
5. **A protective high-pass: fourth-order Butterworth at 24 Hz,** two sections in chorus's
   form. A second-order one leaves the cone's travel under the tuning as the limit; this one
   does not, for 0.7 Hz of extension (the system is 3 dB down at 33.5 Hz). No EQ.
6. **Amplifier power: 100 W into 4 ohms.** The target (the bass of three two-ways at full
   power, 110.4 dB at 1 m) needs 92 W; the cone reaches its 7.0 mm at 98 W (19.83 V, a sine at
   43.3 Hz); the coil is rated 120 W. The endpoint stops at 19.8 V at the driver.
7. **The amplifier is a bought mains plate amplifier fed at line level by the endpoint,** the
   Dayton Audio SPA100-D ($109.98) in the designed tier, its own low-pass, boost and phase
   switch set out of the way once. Not the endpoint board's own amplifier chip, which gives
   53 W into this driver as one channel, 2.7 dB under what the cone allows.
8. **Crossover: chorus's LR4 bass management, default 80 Hz,** the chain's own default; the
   room sets it from 40 to 200 Hz for whatever mains it has. Nothing in the box is tuned to
   one pair of mains.
9. **The knobs map onto settings the DSP has:** the level knob (a cut of 0 to 12 dB) onto
   `sub_level_cdb`, the phase knob onto `sub_polarity_inverted` at its midpoint. A continuous
   phase control is a follow-up in the DSP, not part of this design.
10. **The record is named `chorus-sub-v1`** and holds the one crossover, at 80 Hz. The box,
    the vent and the high-pass stay in the design file until a later record schema can carry
    them.
11. **The class budget is PROPOSED, not decided:** under about $400 of parts at the designed
    tier (the list totals 360.69), about $620 and $815 for the other two.

## Alternatives not taken

- **A closed box.** No vent and no high-pass to need. Every closed box of the survey's
  drivers ends between 32 and 61 Hz, and the ones that go deep are larger than this vented
  box or need several hundred watts.
- **Designing the better tier** (RSS315HO-44). 25 Hz from 40.78 L, at $257 more, for less
  output unless the amplifier is the $399.98 one; its vent would be a metre of 4 inch tube,
  which is a folded vent or a passive radiator, neither of which the package designs. It
  stays a priced alternative with its box computed.
- **The endpoint board's own amplifier as one bridged-parallel channel.** No added part and
  P1's module unchanged, for 2.4 dB under the target. The fallback if the budget must fall by
  the amplifier's price.
- **A larger amplifier for the good tier.** The cone and the coil are both spent near 100 W.
- **A bare amplifier module with no filters of its own** (a 100 W mains module at $119.98).
  Cleaner in the signal path; it puts a bare mains board in a box the owner builds. Priced in
  the design file as the alternative.
- **A plate amplifier with its own DSP** ($269.98), whose every filter can be disabled. More
  than twice the price for the same 4 ohm limit this driver cannot use.
- **A second-order high-pass.** Leaves 53 to 91 W as the cone's limit, set under the tuning.

## Consequences

- The enclosure model has its numbers: 73.54 L net, a 4 inch by 326.6 mm vent with 102 mm of
  clearance, a 272 mm cut-out in a doubled baffle, a 7 by 6 inch cut-out for the amplifier's
  plate, and the tolerances table.
- The board plan has an open item: the endpoint board P1 recommends has no line output, so
  the feed to the amplifier is a DAC on its bus, its maker's line-out sibling, or P1's own
  fallback pair.
- The proposed budget and the tier are the owner's to decide; the design file marks both
  PROPOSED.
- Follow-ups in the DSP, named and not done: a per-endpoint high-pass and voltage limit for
  the `LFE` role (the two sections and 19.8 V of the design file), and a continuous phase
  control behind the phase knob.
- A finding in the test: at an 80 Hz crossover the running filters' single-precision
  coefficients move the designed response by up to 0.006 dB, so the design-record test now
  takes that rounding out before its 0.001 dB check and holds the rounding under 0.02 dB.
- The first box measures everything: tuning, leakage, the cone's travel, vent noise, the
  amplifier's real output, the level at the seat.
