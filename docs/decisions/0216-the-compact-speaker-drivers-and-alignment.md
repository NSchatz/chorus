# 0216: the compact speaker is a vented two-way: a Dayton Audio TCP115-4 woofer and ND16FA-6 tweeter in 1.80 L tuned to 60.5 Hz (the QB3 alignment the acoustics package designs), crossed over LR4 at 3500 Hz, with a 3 dB baffle-step shelf at 820 Hz

- Status: accepted, 2026-10-06. A paper design: nothing is built or measured, and every number
  is a starting point the first box confirms or corrects.
- Decided by: the task for the scope ("a two-way (one woofer, one tweeter, one per channel of a
  single TAS58xx) with drivers chosen and priced US-first, the box alignment ... the baffle
  step, the crossover, the DSP EQ, and the tolerances an enclosure must hold, all computed with
  the released acoustics package"; a budget of at most $150; PoE+). Which drivers, which
  alignment, which crossover frequency and how much baffle-step correction are this record's.
  They are cheap to reverse while nothing is ordered: a change is a new `version` of the design
  file and a re-exported record.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `docs/hardware/compact-speaker.md` (the design, version 1),
  `docs/research/compact-speaker-drivers.md` (the survey),
  `fixtures/design-record/chorus-compact-v1.json` and `.provenance`,
  `crates/dsp/tests/design_record.rs` (`the_compact_record_is_the_exported_one_and_runs`),
  `fixtures/README.md`, `docs/dsp.md`.

## Context

The compact class (K22) is a small PoE+ smart speaker under about $150 of parts (K89, K90).
P1 (PROPOSED) puts about $91 of that into a bought ESP32-S3 + TAS5825M + W5500 board and a
PoE+ splitter; P12 (PROPOSED) puts about $23 into a printed ASA box. The microphone, its mute
switch, the buttons and the box's hardware take about $11 more. That leaves about $25 for the
drivers. The amplifier has two channels, so a two-way needs no passive crossover: each driver
gets a channel and the crossover is digital.

The released acoustics package (v1.49.0) designs sealed boxes and two vented alignments (QB3
and B4), ports, the baffle step, and LR4 and LR2 crossovers; its design record, version 1,
holds crossovers only.

## What was read

On 2026-10-06. `docs/research/compact-speaker-drivers.md` (the survey and its sources: the
seller's item service and the maker's spec sheets). The README of the acoustics package of the
owner's shared Python library at release v1.49.0, and its `baffle` module's notes. In this
repository: `docs/proposals/P1-embedded-platform.md`, `docs/proposals/P3-speaker-network.md`
(lines 193 to 206 and 285 to 297), `docs/proposals/P12-enclosures.md`, `docs/hardware/controls.md`,
`docs/hardware/voice-mic.md`, `docs/research/tas5825m-register-map.md`, `docs/dsp.md` ("Design
records"), `crates/dsp/src/biquad.rs`, `crates/dsp/tests/design_record.rs`,
`fixtures/design-record/lr4-2000hz-48k.provenance`, `fixtures/README.md`. TI's TAS5825M
datasheet (pages 1, 6 and 8). The sellers' pages for the budget lines and the switch maker's
product page and user guide, each cited where `docs/hardware/compact-speaker.md` uses it. No
GPL source and no reciprocally licensed hardware design file was opened.

## Decision

1. **Woofer: Dayton Audio TCP115-4** ($15.98, Parts Express, ships from Ohio). It is the one
   woofer under $18 in the survey whose Qts (0.35) is low enough for a vented alignment the
   package designs, it has the lowest resonance (53.8 Hz) and the most excursion (4.0 mm) of
   the list, and at 4 ohms it takes the most power from a 24 V amplifier. Its Re of 3.2 ohms
   is exactly the TAS5825M's minimum BTL load: allowed, at the limit, and so an item for the
   first box's measurement.
2. **Tweeter: Dayton Audio ND16FA-6** ($8.98, same seller). Usable from 3,500 Hz, 6 ohms, a
   32 mm press-fit body that lets the two drivers sit under one wavelength apart at the
   crossover. The ND20FA-6 (15 watts against 10, the same 3,500 Hz) is the better part and
   costs $2.00 more, which the budget does not have: the total is 149.88 with the ND16FA-6.
3. **Alignment: vented, the QB3 box `vented_box_for_driver(woofer, ql=7)` returns:** 1.804 L
   net, tuned to 60.5 Hz, -3 dB at 66.9 Hz. The same volume closed is -3 dB at 110.9 Hz. The
   box the package designs is taken as it is, not enlarged: a 3 L box tuned to 58 Hz would
   reach 55 Hz but with a response the package labels no alignment, more plastic than the
   budget holds, and a longer vent.
4. **Port: one 20 mm vent, 127.7 mm long,** bent once, flared. It is the smallest area whose
   length is still printable inside the box; the price is up to 24.4 m/s of air at full
   excursion, with a 22 mm, 156.2 mm vent as the computed fallback.
5. **Crossover: LR4 at 3500 Hz,** the bottom of the tweeter's stated range and inside the
   woofer's. LR4 because it is the alignment chorus's chain already runs (`Lr4`), its branches
   sum flat, and its 24 dB per octave keeps the small dome 18.8 dB down at its resonance.
6. **Baffle step: 3 dB of the 6, as a high-shelf cut at 820 Hz, Q 0.5.** A cut above the step
   costs no headroom where a boost below it would; 3 dB because these speakers stand near
   walls. The shelf's coefficients come from chorus's own designer, since the released package
   designs no shelf, and the package's response call checks them against its baffle-step curve.
7. **The record is named `chorus-compact-v1`** and holds the one crossover. The box, the vent
   and the EQ stay in the design file until a later record schema can carry them.

## Alternatives not taken

- **A closed box.** Simpler, no vent noise, better outdoors. It gives up 44 Hz of extension in
  the same volume, which in a speaker meant to play alone is most of its bass.
- **A full-range driver and no tweeter** (several in the survey). Cheaper and no crossover, but
  it leaves one amplifier channel unused and the task asks for a two-way.
- **An 8 ohm woofer** (TCP115-8). Kinder to the amplifier; its Qts of 0.43 is above what the
  package designs a vented box for, and it is 4.9 dB less sensitive at the same voltage.
- **Full 6 dB baffle-step correction.** Right in free space, too much on a counter by a wall.

## Consequences

- The budget closes at 149.88 USD only with pack parts counted by share and the box at 0.90 kg
  of ASA; two of its lines were out of stock on the day read.
- By class allocation eight compacts fit the 250 W switch alone, seven beside the access point,
  and the room list's nine do not: said in the design file, decided nowhere here.
- Nothing runs the record or the EQ in a chain yet, and nothing limits the channels to the
  drivers' ratings yet; both are later tasks.
- The first box measures everything: tuning, leakage, vent noise, the shelf, the trim, the
  amplifier's temperature.
