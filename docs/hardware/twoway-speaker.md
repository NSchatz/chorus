# The active two-way: acoustic design

version: 1

- Status: designed on paper, 2026-10-06. Nothing is ordered, cut, built or measured. Every
  number here is a datasheet value or a calculation from datasheet values (CLAUDE.md rule 6:
  starting points, not truth); the first built box is measured before any number here is called
  confirmed.
- What this is: chorus's source of truth for the active two-way's acoustics (K22, K68, K90):
  the drivers in three priced tiers, the box alignment and port, the baffle step, the crossover,
  the DSP EQ and the tolerances an enclosure must hold, with the amplifier arrangement, the
  power, the controls and the priced budget. The enclosure model, the board and electronics
  plan, the bill of materials and the part records are made from it elsewhere and are not in
  this file.
- The choice of drivers, alignment, crossover frequency and amplifier arrangement, and its
  reasons: the decision record
  `docs/decisions/0217-the-two-way-speaker-drivers-and-alignment.md`. The driver survey:
  `docs/research/twoway-speaker-drivers.md`.
- A change to any design number bumps `version` and re-exports the record.

**How the numbers were computed.** Every computed number names the call that produced it. The
calls are those of the acoustics package of the owner's shared Python library at release
**v1.49.0** (the current release on 2026-10-06), run with `uv run` from the release tag on
2026-10-06 (the same release and method as the design record's provenance,
`fixtures/design-record/chorus-twoway-v1.provenance`). The package's air is its default,
`AIR_SB` (1.2 kg/m³, 344 m/s). One section's coefficients (the shelf of the DSP EQ) come from
chorus's own designer, because that release designs no shelf; it is said there, and its
response is checked with the package.

## The shape

A two-way in one braced plywood box: one 6 1/2 inch woofer and one 1 1/8 inch dome tweeter,
each on its own channel of a single stereo TAS5825M (no passive crossover part). The crossover
and the EQ are digital, at 48 kHz, in chorus's DSP form. The box is vented. Power is a 24 V
supply fed from the mains; the network is wired Ethernet. There is nothing on the front but
the two drivers and the vent.

## Drivers

Three tiers, priced. **Version 1 designs the good tier**: every acoustic number below is for
its woofer and tweeter. The better and best tiers are priced alternatives with the box the
package designs for their woofer; taking one is a new `version` (a new vent, a new EQ, and for
the best tier a new trim).

Prices are single-unit list prices in US dollars, before shipping and tax, read 2026-10-06.
The seller's product pages are drawn by script and give an automated read an empty page, so
each price and stock state was read from the seller's own item service (the "Price as read"
URL), which is what the page itself shows.

| Tier | Woofer | Tweeter | Pair | The package's box for the woofer |
|---|---|---|---|---|
| **good (designed)** | Dayton Audio DC160-8, $34.98 | Dayton Audio DC28F-8, $24.98 | **$59.96** | QB3, 9.29 L, 41.5 Hz, -3 dB at 46.8 Hz |
| better | Dayton Audio RS180-8, $79.98 | Dayton Audio DC28F-8, $24.98 | $104.96 | QB3, 9.62 L, 45.2 Hz, -3 dB at 53.2 Hz |
| best | Dayton Audio RS180-8, $79.98 | Peerless DA25TX00-08, $56.25 | $136.23 | the same as better |

The better and best tiers share a woofer: it is the one woofer of the survey whose box is the
good tier's size (9.62 L against 9.29 L, so the same cabinet with a different vent,
**ASSUMED** until the enclosure model checks the cut-outs) with 1.8 times the displacement
(74.8 cm³ against 42.5 cm³). What the tiers buy is output and a stiffer cone and dome, not
bass extension: the good tier's box reaches lowest. The RS180-8's row is
`vented_box_for_driver(Driver(fs_hz=35.7, qes=0.42, qms=1.22, vas_m3=litres(24.4)), ql=7)`.

### The designed tier's drivers

| | Woofer | Tweeter |
|---|---|---|
| Make | Dayton Audio | Dayton Audio |
| Model | DC160-8, 6 1/2 inch Classic woofer, treated paper cone, 8 ohm | DC28F-8, 1 1/8 inch silk dome tweeter, 8 ohm |
| Vendor | Parts Express, part 295-305 | Parts Express, part 275-070 |
| URL | https://www.parts-express.com/Dayton-Audio-DC160-8-6-1-2-Classic-Woofer-295-305 | https://www.parts-express.com/Dayton-Audio-DC28F-8-1-1-8-Silk-Dome-Tweeter-275-070 |
| Price as read | https://www.parts-express.com/api/items?country=US&currency=USD&language=en&fieldset=details&q=295-305 | https://www.parts-express.com/api/items?country=US&currency=USD&language=en&fieldset=details&q=275-070 |
| Price | $34.98, in stock (323) | $24.98, in stock (145) |
| Date read | 2026-10-06 | 2026-10-06 |
| Ship-from | Springboro, Ohio, USA (the seller's shipping page, https://www.parts-express.com/shipping, footer "725 Pleasant Valley Dr." "Springboro, OH 45066 USA", read 2026-10-06; the page has no sentence that says where orders ship from, so the footer address is **ASSUMED** to be the warehouse, as in the compact's survey) | the same |
| Maker's spec sheet | https://www.daytonaudio.com/images/resources/295-305-dayton-audio-dc160-8-specifications-46146.pdf | https://www.daytonaudio.com/images/resources/275-070-dayton-audio-dc28f-8-silk-dome-tweeter-specifications.pdf |

### The alternatives' drivers

| | Woofer (better, best) | Tweeter (best) |
|---|---|---|
| Make | Dayton Audio | Peerless by Tymphany |
| Model | RS180-8, 7 inch Reference woofer, aluminum cone, 8 ohm | DA25TX00-08, 1 inch corundum dome tweeter, 8 ohm |
| Vendor | Parts Express, part 295-355 | Parts Express, part 264-1676 |
| URL | https://www.parts-express.com/Dayton-Audio-RS180-8-7-Reference-Woofer-295-355 | https://www.parts-express.com/Peerless-DA25TX00-08-1-Corundum-Dome-Tweeter-264-1676 |
| Price as read | https://www.parts-express.com/api/items?country=US&currency=USD&language=en&fieldset=details&q=295-355 | https://www.parts-express.com/api/items?country=US&currency=USD&language=en&fieldset=details&q=264-1676 |
| Price | $79.98, in stock (122) | $56.25, in stock (18) |
| Date read | 2026-10-06 | 2026-10-06 |
| Ship-from | Springboro, Ohio, USA (as above) | the same |
| Maker's spec sheet | https://www.daytonaudio.com/images/resources/295-355--dayton-audio-rs180-8-reference-woofer-8-ohm-specifications.pdf | https://www.parts-express.com/pedocs/specs/264-1676--peerless-da25tx00-08-spec-sheet.pdf (the seller-hosted copy) |
| Values printed (the survey's read of the sheet) | Re 6.4 ohms, Fs 35.7 Hz, Qms 1.22, Qes 0.42, Qts 0.31, Vas 24.4 liters, Sd 124.7 cm², Xmax 6.0 mm, Vd 74.8 cm³, 87.1 dB @ 2.83V/1m, 60 watts, 39 - 3,600 Hz | Re 6.89 Ohms, Zmin 7.5 Ohms, Fs 633.75 Hz, Qts 0.6, 88.94 dB at 2.83 V, 100 W rated noise power, test bandwidth 2K-20KHz |

### The Thiele/Small values used, and their source

Woofer (DC160-8), from the maker's spec sheet above (read 2026-10-06; the seller's item service
gives the same values except where a row says so):

| Parameter | Value | Used as |
|---|---|---|
| Nominal impedance | 8 ohms | the amplifier's load |
| Re | 6.6 ohms | `Driver(re_ohm=6.6)` |
| Le | 2.26 mH @ 1 kHz | not used (the package is small-signal without inductance) |
| Fs | 35.7 Hz | `Driver(fs_hz=35.7)` |
| Qms | 3.46 | `Driver(qms=3.46)` |
| Qes | 0.38 | `Driver(qes=0.38)` |
| Qts | 0.34 (printed) | not an input: `Driver.qts` gives 0.3424 from Qes and Qms |
| Vas | 17.9 liters | `Driver(vas_m3=litres(17.9))` |
| Sd | 134.8 cm² | `Driver(sd_m2=134.8e-4)` |
| Xmax | 3.15 mm (the item service prints 3.2) | `Driver(xmax_m=3.15e-3)` |
| Vd | 42.5 cm³ (printed) | not an input: `Driver.vd_m3` gives 42.46 cm³ |
| Mms, Cms, BL | 29.3 g, 0.68 mm/N, 10.7 Tm | not used |
| Sensitivity | 86.1 dB at 2.83 V, 1 m | the tweeter's level trim |
| Power handling | 50 watts RMS | the woofer channel's limit |
| Usable range | 30 to 4,000 Hz; "Smooth frequency response up to 2 kHz" | the crossover frequency |
| Overall diameter, cut-out, depth, bolt circle | 6.5, 5.69, 3.24, 6.16, four holes (the seller's figures, no unit printed; inches **ASSUMED**: 165.1, 144.5, 82.3 and 156.5 mm) | the baffle |

Tweeter (DC28F-8), from the maker's spec sheet above (read 2026-10-06):

| Parameter | Value | Used as |
|---|---|---|
| Nominal impedance, Re | 8 ohms, 5.4 ohms | the amplifier's load |
| Fs | 834 Hz | the crossover frequency |
| Qms, Qes, Qts | 0.81, 1.33, 0.50 | not used |
| Sensitivity | 89 dB at 1 W, 1 m | the tweeter's level trim |
| Power handling | 50 watts RMS | the tweeter channel's limit |
| Usable range | 1,300 to 20,000 Hz | the crossover frequency |
| Overall diameter, cut-out, depth | 4.33, 2.91, 1.53 (the seller's figures, no unit printed; inches **ASSUMED**: 110.0, 73.9 and 38.9 mm) | the baffle |

Neither driver's page or sheet states any rating for humid or outdoor use.

## The box: vented, 9.29 L net, tuned to 41.5 Hz

`vented_box_for_driver(woofer, ql=7)` with
`woofer = Driver(fs_hz=35.7, qes=0.38, qms=3.46, vas_m3=litres(17.9), re_ohm=6.6, sd_m2=134.8e-4, xmax_m=3.15e-3)`
gives:

| Quantity | Value | Call |
|---|---|---|
| Alignment | QB3 | `vented_box_for_driver(woofer, ql=7).alignment` (the driver's Qts, 0.3424 by `Driver.qts`, is below the B4 value 0.4048 of `butterworth_qts(7)`) |
| **Net volume** | **9.289 L** | `.vab_m3` (compliance ratio `.alpha` 1.927) |
| **Tuning** | **41.5 Hz** | `.fb_hz` (41.51; tuning ratio `.h` 1.163) |
| -3 dB frequency of the box | 46.8 Hz | `.f3_hz` |
| Box response | -8.9 dB at 35 Hz, -5.8 dB at 40 Hz, -3.6 dB at 45 Hz, -2.2 dB at 50 Hz, -0.8 dB at 60 Hz, -0.15 dB at 80 Hz | `.response_db(f)` |

The enclosure's leakage is the designer's estimate, `ql = 7`: the value P12 says a design
assumes ("a design assumes Q_L = 7 and a good box does better") and the one the leakage
tolerance below is written against.

**Net volume means** the air the woofer works into: the cavity after the woofer's own bulk
behind the baffle, the tweeter's rear chamber, the port tube (its air, 148.7 cm³ by
`port_area_m2(0.035)` times the length, and its wall), the braces, the electronics carrier if
it sits in the cavity and anything else inside have been taken out. The enclosure model sums
those. P12's 15 L for this class was illustrative; this is the real figure. The box has no
fill and no lining in version 1: the alignment is computed for leakage loss alone, which is all
the package models for a vented box.

**Why vented and not sealed.** In the same 9.289 L closed, `sealed_box(woofer, 9.289e-3)` gives
`fc_hz` 61.1 Hz, `qtc` 0.586 and `f3_hz` 76.2 Hz: the vent buys 29 Hz of bass extension from a
box of the same size. A two-way that plays alone in a room needs it; one that is a front
channel beside a subwoofer is crossed over above both figures by chorus's bass management and
loses nothing by the vent.

### The port

One round vent, **35.0 mm inside diameter, 154.6 mm long**, flared at both ends, printed (P12):

| Quantity | Value | Call |
|---|---|---|
| Area | 9.621 cm² | `port_area_m2(0.035)` |
| Length | 154.6 mm | `port_length_m(fb_hz=41.5067, vab_m3=9.28918e-3, area_m2=port_area_m2(0.035))` (the package's default end correction: one flanged end, one free) |
| Peak air speed, at most | 11.5 m/s | `port_air_velocity_m_s(woofer.vd_m3, 41.5067, port_area_m2(0.035))`: the vent carrying the cone's whole displaced volume (42.46 cm³) at the tuning frequency, an upper bound reached only at full excursion |
| Trim | 9.0 mm of length per 1 Hz of tuning | `port_trim_m(measured_fb_hz=40.5067, fb_hz=41.5067, vab_m3=9.28918e-3, area_m2=port_area_m2(0.035))` |

The package models a straight vent with no flare and no loss, so the length is a starting
value: the first box's tuning is measured from its impedance curve and the vent corrected with
the trim figure. The inner end stays at least one diameter (35 mm) clear of any wall
(**ASSUMED** practice), so a straight vent needs a cavity at least 190 mm deep along it; in a
shallower box it bends once, its length taken along its centre line (**ASSUMED** to hold
through one smooth bend). The package sets no air-speed limit (its sources give none as a
design number); 11.5 m/s is under half the compact's figure. A wider vent is the fallback if
the first box's vent is audible: 40 mm, 206.1 mm long, at most 8.8 m/s (the same two calls with
`port_area_m2(0.040)`). Whether the mouth is on the front or the rear is the enclosure model's;
a rear mouth wants about a diameter or more of air behind the box (**ASSUMED**).

## The baffle step: 550 Hz

The baffle is **210 mm wide**. `baffle_step_hz(0.210)` gives 547.6 Hz, used as 550 Hz: below
it the box radiates all round and the response falls towards -6.02 dB (`BAFFLE_STEP_DB`), and
at 550 Hz it is 3.01 dB down (`baffle_step_db(550, 550)`).

The EQ takes back **3 dB** of the 6, not all of it. These speakers stand on a shelf, a stand or
a console near a wall, where the wall returns part of the loss; the package's baffle module
quotes its source that in a room "3 or 4 dB of diffraction loss correction may result in an
overall response that is closer to neutral" and leaves the amount to the designer. 3 dB is
**ASSUMED** until a built box is measured in a room; chorus's room correction works on top of
it.

## The crossover: LR4 at 2000 Hz

- **Kind:** LR4, the fourth-order alignment whose two branches are each 6.02 dB down and in
  phase at the crossover and add to a flat sum: two identical second-order sections per branch.
- **Frequency:** 2000 Hz, at 48 kHz.
- **Produced by:** `linkwitz_riley(48000, 2000)`, exported with `design_record(...)` and
  `write_record(...)` as `fixtures/design-record/chorus-twoway-v1.json` (below).

Why 2000 Hz: the woofer's sheet calls its response smooth "up to 2 kHz" inside a usable range
that ends at 4,000 Hz, and the tweeter's usable range starts at 1,300 Hz. At the top of the
woofer's range the low branch is 25.2 dB down
(`decibels(linkwitz_riley(48000, 2000).response_low(4000))`); at the bottom of the tweeter's
the high branch is 16.5 dB down (`.response_high(1300)`) and at the tweeter's resonance
(834 Hz) 30.8 dB down (`.response_high(834)`). The drivers' centres are 145 mm apart or less
(the two outside diameters allow 137.6 mm, arithmetic on the **ASSUMED** inches; the enclosure
model places them), under one wavelength at 2000 Hz (172 mm at 344 m/s, arithmetic).

It is also the frequency chorus's chain has used as its example (`docs/dsp.md`, "Defaults and
ASSUMED values", the row "Two-way example": 2 kHz, until the drivers are designed). This design
arrives at it from the two sheets; the example's value is now a designed one for this speaker.

Each low-branch section (run twice in series), `b0 b1 b2 a1 a2`, `a0 = 1`:

```text
0.01440144034651121  0.02880288069302242  0.01440144034651121  -1.6329931618554523  0.6905989232414971
```

Each high-branch section (run twice in series):

```text
0.8308980212742373  -1.6617960425484746  0.8308980212742373  -1.6329931618554523  0.6905989232414971
```

The low branch feeds the woofer's amplifier channel and the high branch the tweeter's. The
record is the source of these numbers; the lines above are copied from it.

What the crossover does not hold: the drivers' own responses and their acoustic offset. The
filter is the electrical one. No delay is set between the branches in version 1 (**ASSUMED**
zero until the first box is measured on the tweeter's axis).

## The DSP EQ

Three sections at a sample rate of **48000 Hz**, in chorus's form: `b0 b1 b2 a1 a2` with
`a0 = 1`, the form `chorus_dsp::biquad::Coefficients` holds and the design record uses.

| # | What | Where | b0 | b1 | b2 | a1 | a2 |
|---|---|---|---|---|---|---|---|
| 1 | Baffle-step shelf: a high shelf, -3 dB, 550 Hz, Q 0.5 | before the crossover (both branches) | 0.716506259405197 | -1.3247409548469324 | 0.6123249358999363 | -1.87209002449846 | 0.8761802649566612 |
| 2 | Woofer protection: a second-order high-pass, 28 Hz, Q 0.7071 | the low branch | 0.9974116737880857 | -1.9948233475761714 | 0.9974116737880857 | -1.9948166481323713 | 0.9948300470199712 |
| 3 | Tweeter level trim: -2.9 dB | the high branch | 0.7161434102129021 | 0 | 0 | 0 | 0 |

Where each comes from:

1. **The shelf.** Release v1.49.0 of the acoustics package designs low-pass and high-pass
   sections only (its README lists shelving sections as out of scope), so this one section is
   designed by chorus's own cookbook designer:
   `Coefficients::design(Kind::HighShelf, 48000.0, 550.0, 0.5, -3.0)` in `crates/dsp/src/biquad.rs`,
   printed on 2026-10-06. It is a cut above the step, never a boost below it, so it costs no
   headroom. The package checks it: `Biquad(b0, b1, b2, a1, a2).magnitude_db(f, 48000)` on the
   five numbers above gives -0.10 dB at 100 Hz, -0.69 dB at 300 Hz, -1.50 dB at 550 Hz,
   -2.30 dB at 1000 Hz and -2.97 dB at 5000 Hz, within 0.022 dB of half of the full mirror
   network, `baffle_step_compensation_db(f, 550) * 3 / BAFFLE_STEP_DB`, at every one of
   thirteen frequencies from 50 Hz to 20 kHz.
2. **The high-pass.** `highpass(48000, 28)` (its default Q, `BUTTERWORTH_Q`, 0.7071). Below its
   tuning a vented box stops loading the cone, so the woofer is not driven there. chorus's own
   `Coefficients::design(Kind::Highpass, 48000.0, 28.0, FRAC_1_SQRT_2, 0.0)` gives the same five
   numbers digit for digit. With it the speaker's low end is `.response_db(f)` of the box plus
   `highpass(48000, 28).magnitude_db(f, 48000)`: -6.7 dB at 40 Hz, -4.2 dB at 45 Hz, -2.6 dB at
   50 Hz, -1.0 dB at 60 Hz, -0.2 dB at 80 Hz, so the system is 3 dB down near **48.5 Hz**.
3. **The trim.** The woofer's sensitivity is printed for 2.83 V and the tweeter's for 1 W; into
   the tweeter's nominal 8 ohms 1 W is 2.83 V (arithmetic, **ASSUMED** to be what the sheet
   means), and both channels of the amplifier have the same gain, so the tweeter is turned
   down by their difference, 89.0 - 86.1 = 2.9 dB: `b0 = 10^(-2.9/20)` (arithmetic;
   `Biquad(0.7161434102129021, 0, 0, 0, 0).magnitude_db(1000, 48000)` gives -2.9). **ASSUMED**
   from the datasheets; set by measurement on the first box.

Nothing runs these sections yet. Wiring the record and the EQ into the playback chain or the
firmware, and choosing whether they run in chorus's chain or in the amplifier's own filters
(TI's datasheet for the TAS5825M lists "2 × 15 BQs", page 1), is a later task.

## The amplifier arrangement

**One TAS5825M in its 2.0 mode (two bridge-tied channels): the woofer on one channel, the
tweeter on the other.** The crossover's low branch is the stream the woofer's channel plays and
its high branch the tweeter's; which channel is which, and every pin, is the board plan's.

Why this and not two amplifier chips:

- TI's TAS5825M datasheet (SLASEH7H, page 1, read 2026-10-06) gives "2 × 30 W, 2.0 Mode (8-Ω,
  24 V, THD+N=1%)" and "2 × 38 W, 2.0 Mode (8-Ω, 24 V, THD+N=10%)". Both drivers are 8 ohms
  (Re 6.6 and 5.4 ohms), far above the 2.0 mode's minimum load (3.2 ohms, page 6).
- Into 8 ohms at 24 V a channel is limited by its supply voltage, not by its current: 24 V of
  peak swing is 17.0 V RMS, 36 W into 8 ohms (arithmetic), which the datasheet's 30 W at 1%
  distortion agrees with. A second chip run as one bridged-parallel channel ("1.0 Mode"; the
  datasheet's figures for it are into 4 ohms at 22 V, and its minimum load is 1.6 ohms, page
  6) raises the current a channel can give, not its voltage, so into this 8 ohm woofer it
  would deliver the same 30 W. It would pay only with a 4 ohm woofer.
- 30 W is 60% of the woofer's 50 watt rating and of the tweeter's: the amplifier runs out
  before either driver's printed rating does.
- It is the module P1 recommends, unchanged: one board, one amplifier chip, "two amp channels
  for a two-way" (`docs/proposals/P1-embedded-platform.md`). The same board's maker lists a
  variant with a second TAS5825M ("Dual DAC, ESP32-S3", $69.00 at
  https://www.elecrow.com/esparagus-audio-brick.html, read 2026-10-06; the maker's own page
  calls it "Dual DAC model (prototype)"): the step up if a later version takes a 4 ohm woofer.

**Channel limits (ASSUMED from the datasheets):** each driver at most 20.0 V RMS (50 watts into
8 ohms, arithmetic). The amplifier's 24 V supply cannot swing that far (17.0 V RMS), so here,
unlike in the compact, the supply holds both limits. What the supply does not hold is the
woofer's excursion: the package computes no displacement-limited power for a vented box, the
high-pass keeps drive out of the range below the tuning, and the cone's travel just above it at
full level is an open item for the first box.

## Power and network (K90): mains, with Ethernet

- **Mains power, no PoE.** K90: "mains for two-way, sub, rack amp and soundbar (with
  Ethernet), since the Omada switch is 802.3af/at only". The two-way takes nothing from the
  switch's PoE budget.
- **The supply's size (arithmetic on the numbers cited; nothing measured).** Two channels at
  the datasheet's 30 W are 60 W out; at the 80 to 90% amplifier efficiency the board's page
  and TI's datasheet give (`docs/hardware/compact-speaker.md`, "PoE"), that is 67 to 75 W in,
  plus about 1.5 W for the processor, the Ethernet and the light (**ASSUMED**, as in the
  compact). A 24 V supply of 100 W or more covers both channels at full sine power at once,
  which music never asks for; the budget below prices a 120 W one.
- **Which supply** is the electronics plan's. The budget's allocation is a safety-listed 24 V
  desktop adapter with an IEC inlet, which keeps every mains-voltage part outside the wooden
  box and makes the speaker's own input 24 V DC. An enclosed supply inside the box behind a
  fused IEC inlet is cheaper (priced under the budget) and puts mains wiring in a box the owner
  builds; that choice is not this file's.
- **Ethernet:** the endpoint board's W5500 wired port (below). Wi-Fi is not part of this class.

## The endpoint module, the controls and the enclosure path

- **The endpoint module chorus's firmware targets (ASSUMED while P1 is PROPOSED):** P1's
  Option B, `docs/proposals/P1-embedded-platform.md`: an ESP32-S3 with a TAS5825M and W5500
  wired Ethernet on one bought board (the Esparagus Audio Brick, ESP32-S3 variant: "Stereo I²S
  DAC (TAS5825M) with built-in D-Class amp", "W5500 SPI Ethernet", "Power Source 5-26 V", "2x
  30 W at 24 V at 8Ω, THD+N = 1% (Power mode)", "RGB LED (status and notifications)", per its
  seller's page read 2026-10-06), fed 24 V by the mains supply. It is the compact's module
  without the PoE+ splitter. The firmware's reference board profile is
  `firmware/boards/brick-s3-wired.conf`. If P1 is decided otherwise, this section and the
  budget change and `version` is bumped; the acoustics do not depend on the module as long as
  it has two amplifier channels at 24 V into 8 ohms.
- **Controls (K68):** none visible. "a clean front; a hidden pairing button and a rear status
  light only"; `docs/hardware/controls.md` gives the class "a hidden pairing button" and "a
  rear status light, status only (never the visualizer, so the front stays clean)" and no
  microphone. So: one pairing button and one status light, both on the rear, nothing on the
  front, the sides or the top. This file places neither; they appear in the budget as
  allocations. Both pass through the box's wall, so both are openings the enclosure seals or
  walls off (the tolerances below).
- **The enclosure path:** `docs/proposals/P12-enclosures.md` (PROPOSED, so its answer for this
  class is **ASSUMED** here). Its recommendation: "active two-way and LCR in braced 18 mm
  birch plywood with printed ports, carriers and templates", that is 18 mm birch plywood
  (13-ply, void-free), "braced so that no unbraced span is over about 200 mm", with printed
  fittings: the port and its flare, the electronics carrier and the router templates for the
  driver cut-outs; a clear coat on the faces. P12 puts a cut on the owner's saw at ± 0.5 mm
  (**ASSUMED** there) and the net volume a wooden box holds at about ± 1%. Those are what the
  tolerances below are computed against.
- **Outdoors:** not this class. Neither driver states a weather rating.

## The tolerances an enclosure must hold

Each row's effect is computed by re-running `vented_box(woofer, vab_m3, fb_hz, ql=7)` at the
changed value and comparing `.f3_hz` and `.response_db(f)` from 40 to 300 Hz with the design's;
a changed vent or volume goes through `port_tuning_hz(length_m=..., vab_m3=..., area_m2=...)`
first.

| What | Value and tolerance | Effect at the edge of the tolerance |
|---|---|---|
| Net volume | 9.29 L ± 2% (± 186 mL); P12 expects about ± 1% of a sawn box | at ± 2%: tuning 41.10 to 41.93 Hz, `f3` 46.5 to 47.2 Hz, response within 0.20 dB. At ± 1%: within 0.10 dB |
| Vent length | 154.6 mm ± 1.0 mm along the centre line | tuning 41.39 to 41.62 Hz, response within 0.03 dB |
| Vent inside diameter | 35.0 mm ± 0.2 mm (**ASSUMED** printable) | tuning 41.29 to 41.73 Hz, response within 0.05 dB |
| Volume ± 1% and vent length ± 0.5 mm together, worst case | | tuning 41.24 to 41.77 Hz, `f3` 46.7 to 47.0 Hz, response within 0.11 dB |
| Tuning, as measured on the built box | 41.5 Hz ± 3% (40.3 to 42.8 Hz); outside it the vent is trimmed | response within 0.25 dB (± 5% would be 0.42 dB) |
| Leakage | `ql` of 7 or more, shown by the built box's impedance curve (P12) | at `ql` 5: `f3` 49.2 Hz, up to 0.68 dB low; at `ql` 3: `f3` 55.1 Hz, 2.1 dB low. A tighter box only gains: `ql` 10 gives `f3` 45.2 Hz, `ql` 20 gives 43.3 Hz |
| Baffle width | 210 mm ± 2 mm | the step at 542.5 to 552.9 Hz (`baffle_step_hz(0.212)`, `baffle_step_hz(0.208)`) against the EQ's 550 Hz |
| Baffle edges | rounded or chamfered (**ASSUMED** 12 mm or more; P12 states no figure for this class) | not computed: the package models the smooth step, not edge ripple |
| Driver centres | 145 mm apart or less, on the baffle's vertical centre line, the tweeter above | under one wavelength at the crossover (arithmetic) |
| Driver seats | each driver on a gasket, flush or surface mounted as the enclosure model chooses; the woofer's cut-out relieved behind the baffle so 18 mm of plywood does not shade the cone's rear (**ASSUMED** practice) | counted in the leakage figure |
| Walls | P12's construction: 18 mm birch plywood, no unbraced span over about 200 mm | not computed here (P12's calculation: first panel resonance near 1.0 kHz for a 200 x 300 mm braced panel) |
| Openings | every opening into the cavity (drivers, vent seat, cable entry, the pairing button, the status light, the DC or mains inlet) sealed, or walled off from it | counted in the leakage figure |
| Fill | none | the alignment assumes none |

## The budget (PROPOSED: under about $225 of parts per two-way, not labour)

K89 caps the compact only and says of the others "budgets proposed with priced alternatives".
This file **proposes** the two-way's: **under about $225 of parts at the designed (good)
tier**, with the better tier at about $250 and the best at about $280 as priced alternatives.
It is PROPOSED, not decided: the owner decides the class's budget and tier.

Single-unit list prices in US dollars before shipping and tax, each read from its URL on the
date in its row. The drivers are the chosen parts. Every other line is a **priced allocation**:
the part that prices it is an example, and the board plan, the bill of materials and the
enclosure model choose the real ones. Where a part is sold only in a pack, the line is this
speaker's share of the pack and says so (the room list builds six two-ways).

| Line | What prices it | Price | URL, date read |
|---|---|---|---|
| Woofer, good tier (designed) | Dayton Audio DC160-8, one | 34.98 | https://www.parts-express.com/Dayton-Audio-DC160-8-6-1-2-Classic-Woofer-295-305, 2026-10-06 |
| Tweeter, good tier (designed) | Dayton Audio DC28F-8, one | 24.98 | https://www.parts-express.com/Dayton-Audio-DC28F-8-1-1-8-Silk-Dome-Tweeter-275-070, 2026-10-06 |
| Endpoint board with the amplifier (allocation) | Esparagus Audio Brick, ESP32-S3 variant: the ESP32-S3, the TAS5825M and the W5500 on one board, so the amplifier has no line of its own. "$59"; the page said "No longer available" on the day read (another shop listed it at $59.00, "In stock", https://www.elecrow.com/esparagus-audio-brick.html, ship-from not stated) | 59.00 | https://www.crowdsupply.com/sonocotta/esparagus-audio-brick, 2026-10-06 |
| Mains supply (allocation) | Mean Well GST120A24-P1M desktop adapter: "24Vdc 5A", 120 W, "3 pole AC inlet IEC320-C14", "UL62368-1 listed"; in stock (1,492) | 57.27 | https://www.trcelectronics.com/View/Mean-Well/GST120A24-P1M.shtml, 2026-10-06 |
| Controls: pairing button (allocation) | a 16 mm panel-mount momentary push button, in stock | 0.95 | https://www.adafruit.com/product/1505, 2026-10-06 |
| Controls: status light (allocation) | one diffused 5 mm LED: 1 of a 25 pack at $4.00 | 0.16 | https://www.adafruit.com/product/299, 2026-10-06 |
| Controls: light holder (allocation) | a 5 mm panel LED holder: 1 of a 5 pack at $0.95 | 0.19 | https://www.adafruit.com/product/2174, 2026-10-06 |
| Enclosure: plywood (allocation) | 3/4 inch (13-ply, 18 mm) Baltic birch: one 20 x 30 inch panel at $13.29 and one 20 x 20 inch at $8.86, 0.645 m² together | 22.15 | https://ocoochhardwoods.com/plywood/baltic-birch-plywood/, 2026-10-06 |
| Enclosure: printed port, flare, carrier and templates (allocation) | 0.10 kg of a 1 kg spool of ASA at $24.99 | 2.50 | https://shop.polymaker.com/products/asa.js, 2026-10-06 |
| Enclosure: driver gaskets (allocation) | closed-cell foam gasket tape: 4 ft of a 50 ft roll at $13.98 | 1.12 | https://www.parts-express.com/Speaker-Gasketing-Tape-1-8-x-3-8-x-50-ft.-Roll-260-540, 2026-10-06 |
| Enclosure: driver screws (allocation) | #8 x 1 inch pan head screws: 8 of a 100 pack at $7.79 | 0.62 | https://www.parts-express.com/8-x-1-Deep-Thread-Pan-Head-Screws-Black-100-Pcs.-081-425, 2026-10-06 |
| **Total, good tier (designed)** | | **203.92** | |

Total: 203.92 USD at the designed (good) tier, under the proposed 225.00.

The tiers, with every line but the drivers unchanged (143.96 USD of allocations):

| Tier | Woofer | Tweeter | Drivers | Total | URLs, date read |
|---|---|---|---|---|---|
| **good (designed)** | DC160-8, 34.98 | DC28F-8, 24.98 | 59.96 | **203.92** | the two rows above, 2026-10-06 |
| better | RS180-8, 79.98 | DC28F-8, 24.98 | 104.96 | 248.92 | https://www.parts-express.com/Dayton-Audio-RS180-8-7-Reference-Woofer-295-355 and the tweeter's row above, 2026-10-06 |
| best | RS180-8, 79.98 | DA25TX00-08, 56.25 | 136.23 | 280.19 | the woofer's URL in the row above and https://www.parts-express.com/Peerless-DA25TX00-08-1-Corundum-Dome-Tweeter-264-1676, 2026-10-06 |

What the total does and does not say:

- **The plywood is an estimate.** A box of this design, about 210 x 376 x 214 mm outside
  around a 10.5 L cavity (the 9.29 L net plus the woofer, the vent and a brace; **ASSUMED**
  dimensions, arithmetic), has about 0.44 m² of panel with its brace; the two panels priced
  are 0.645 m². Whether the parts nest on them is the enclosure model's. P12 estimated about
  $17 for a 15 L box.
- **The supply is the dearest allocation after the board.** Two cheaper ones were read the
  same day: a 24 V 5 A desktop adapter the drivers' seller describes as "UL certified for
  safety" at $30.25, in stock (63)
  (https://www.parts-express.com/24-VDC-5A-Switching-Power-Supply-with-2.5-x-5.5mm-Plug-120-055),
  which would make the total 176.90; and an enclosed open-terminal supply for inside the box,
  the Mean Well LRS-100-24 ("24Vdc 4.5A", "Max Power: 108 Watts", "UL62368-1") at $16.48, in
  stock (769) (https://www.trcelectronics.com/View/Mean-Well/LRS-100-24.shtml), with a fused,
  switched IEC inlet at $2.98
  (https://www.parts-express.com/IEC-AC-Power-Jack-Chassis-Mount-with-Switch-and-Fuse-Holder-090-978),
  which would make it 166.11 and puts mains wiring in the box.
- **Pack shares.** Bought as whole packs for a single speaker (LEDs 4.00, holders 0.95,
  gasket 13.98, screws 7.79, a whole spool 24.99) the same list is 251.04 USD.
- **Ship-from:** the drivers, the gasket and the screws from Springboro, Ohio (above); the
  endpoint board from Mansfield, Texas though made in Poland
  (`docs/hardware/compact-speaker.md`, "The budget"); the button, the LED and its holder from
  Brooklyn, New York (the same section). The supply's, the plywood's and the filament's pages
  state no ship-from; their sellers are US shops (**ASSUMED** US ship-from).
- **One line could not be bought on the day read:** the endpoint board at its US seller.
  Supply is P1's open item and the board plan's, not this file's.
- **Not priced:** the mains cord, hookup wire, the DC lead and its panel jack, the Ethernet
  patch cable and its panel jack, wood glue, the finish, a grille, a fuse, shipping and tax.

## The design record

`fixtures/design-record/chorus-twoway-v1.json` is the crossover above as a version 1
`speaker-design-record`, exported from release v1.49.0 on 2026-10-06 and committed byte for
byte, with `chorus-twoway-v1.provenance` beside it (the release tag, the export command with
placeholders for the package's name, the date and the sha256
`b2dbe5805ce21b43ebc7404f4f627c78020bb3a7a5c8613f6ed7235218e0dcf5`). A version 1 record holds
crossovers only: the box, the vent and the EQ sections live in this file until a later schema
can carry them.

`crates/dsp/tests/design_record.rs`, `the_twoway_record_is_the_exported_one_and_runs`, holds
the file to that sha256, reads it with chorus's own reader, and runs its crossover through
chorus's own `f32` filters: on 2026-10-06 the branches ran at -6.020601 and -6.020597 dB at
2000 Hz and the sum at 0.000001 dB, within 0.000027 dB of the record at all seven of its
response points (250, 500, 1000, 2000, 4000, 8000 and 16000 Hz; the test's tolerance is
0.001 dB). `docs/dsp.md`, "Design records", describes the reader.

## Open items

- **Everything is unmeasured.** The first box gives: the impedance curve (tuning, `ql`), the
  near-field and on-axis response (the shelf's 3 dB, the tweeter's trim and delay, the
  drivers' own roll-offs at 2000 Hz), vent noise at full level, the woofer's excursion just
  above the tuning, and the amplifier's temperature.
- **The drivers' real parameters.** One datasheet line per driver; unit-to-unit spread is not
  stated by the maker and is not modelled. The drivers' dimensions are the seller's unit-less
  figures, taken as inches.
- **The tweeter's sensitivity** is printed for 1 W where the woofer's is for 2.83 V; the trim
  rests on reading them as the same drive.
- **The class's budget and tier are PROPOSED,** the owner's to decide.
- **P1 and P12 are PROPOSED:** the module and the enclosure's construction are ASSUMED from
  their recommendations.
- **The supply:** outside the box or inside it is the electronics plan's choice.
- **The better and best tiers** have a box from the package and no vent, EQ or record of
  their own; each is a later version if taken.

## Sources

Read 2026-10-06 unless a line says otherwise.

- The drivers: the seller's item service and the makers' spec sheets, URLs in "Drivers"; the
  seller's shipping page, https://www.parts-express.com/shipping. The survey:
  `docs/research/twoway-speaker-drivers.md`.
- The acoustics package of the owner's shared Python library, release v1.49.0: its README
  (sealed and vented boxes, ports, the baffle step, biquads, crossovers, "The design record")
  and its `baffle` module's notes. The package cites its own sources for every formula.
- The amplifier: TI, TAS5825M datasheet (SLASEH7H), https://www.ti.com/lit/ds/symlink/tas5825m.pdf,
  pages 1 and 6.
- The endpoint board: https://www.crowdsupply.com/sonocotta/esparagus-audio-brick,
  https://www.elecrow.com/esparagus-audio-brick.html and the maker's product page
  https://sonocotta.com/espragus-audio-brick/ (product pages only).
- The budget's other lines: the URLs in the table and under it. The button's, the LED's and
  the holder's prices were read from the seller's product list
  (https://www.adafruit.com/api/products) by the survey.
- In this repository: `docs/proposals/P1-embedded-platform.md`, `docs/proposals/P12-enclosures.md`,
  `docs/hardware/controls.md`, `docs/hardware/compact-speaker.md`,
  `docs/research/tas5825m-register-map.md`, `docs/dsp.md`, `crates/dsp/src/biquad.rs`.
- No GPL source and no reciprocally licensed hardware design file was opened; no speaker
  design program's source was read.
