# The compact speaker: acoustic design

version: 2

- Status: designed on paper, 2026-10-06. Nothing is ordered, printed, built or measured. Every
  number here is a datasheet value or a calculation from datasheet values (CLAUDE.md rule 6:
  starting points, not truth); the first built box is measured before any number here is called
  confirmed.
- What this is: chorus's source of truth for the compact class's acoustics (K22, K67, K89, K90):
  the drivers, the box alignment and port, the baffle step, the crossover, the DSP EQ and the
  tolerances an enclosure must hold, with the priced budget and the PoE arithmetic. The
  enclosure model, the board and electronics plan, the bill of materials and the part records
  are made from it elsewhere and are not in this file.
- The choice of drivers and alignment and its reasons: the decision record
  `docs/decisions/0216-the-compact-speaker-drivers-and-alignment.md`. The driver survey:
  `docs/research/compact-speaker-drivers.md`.
- A change to any design number bumps `version` and re-exports the record.
- Version 2 (2026-10-06): the endpoint is three bought modules (the ESP32-S3-DevKitC-1-N8R8,
  the Sonocotta Louder Raspberry Hat Plus and the WIZnet WIZ850io), not the Esparagus Audio
  Brick, which is no longer available; the splitter is the Phihong POE21-240-R. "The endpoint
  module", "The budget" and "PoE" change; the acoustics, the crossover and the design record do
  not.

**How the numbers were computed.** Every computed number names the call that produced it. The
calls are those of the acoustics package of the owner's shared Python library at release
**v1.49.0**, run with `uv run` from the release tag on 2026-10-06 (the same release and method
as the design record's provenance, `fixtures/design-record/chorus-compact-v1.provenance`). The
package's air is its default, `AIR_SB` (1.2 kg/m³, 344 m/s). One section's coefficients (the
shelf of the DSP EQ) come from chorus's own designer, because that release designs no shelf;
it is said there, and its response is checked with the package.

## The shape

A two-way in one printed box: one 4 inch woofer and one 5/8 inch dome tweeter, each on its own
channel of a single stereo TAS5825M (the amplifier's two channels in its 2.0 mode; no passive
crossover part). The crossover and the EQ are digital, at 48 kHz, in chorus's DSP form. The box
is vented. Power and network arrive on one cable (PoE+).

## Drivers

Prices are single-unit list prices in US dollars, before shipping and tax, read 2026-10-06.
The seller's product pages are drawn by script and give an automated read an empty page, so
each price and stock state was read from the seller's own item service (the second URL of a
row), which is what the page itself shows.

| | Woofer | Tweeter |
|---|---|---|
| Make | Dayton Audio | Dayton Audio |
| Model | TCP115-4, 4 inch treated paper cone midbass woofer, 4 ohm | ND16FA-6, 5/8 inch soft dome neodymium tweeter, 6 ohm |
| Vendor | Parts Express, part 295-415 | Parts Express, part 275-025 |
| URL | https://www.parts-express.com/Dayton-Audio-TCP115-4-4-Treated-Paper-Cone-Midbass-Woofer-4-Ohm-295-415 | https://www.parts-express.com/Dayton-Audio-ND16FA-6-5-8-Soft-Dome-Neodymium-Tweeter-275-025 |
| Price as read | https://www.parts-express.com/api/items?country=US&currency=USD&language=en&fieldset=details&q=TCP115-4 | https://www.parts-express.com/api/items?country=US&currency=USD&language=en&fieldset=details&q=ND16FA-6 |
| Price | $15.98, in stock (1200) | $8.98, in stock (882) |
| Date read | 2026-10-06 | 2026-10-06 |
| Ship-from | Springboro, Ohio, USA (the seller's shipping page, https://www.parts-express.com/shipping, footer "725 Pleasant Valley Dr. Springboro, OH 45066 USA", read 2026-10-06) | the same |
| Maker's spec sheet | https://www.daytonaudio.com/images/resources/295-415--dayton-audio-tcp115-4-spec-sheet.pdf | https://www.daytonaudio.com/images/resources/275-025-dayton-audio-nd16fa-6-specifications-46192.pdf |

### The Thiele/Small values used, and their source

Woofer, from the maker's spec sheet above (read 2026-10-06; the seller's item service gives the
same values):

| Parameter | Value | Used as |
|---|---|---|
| Nominal impedance | 4 ohms | the amplifier's load |
| Re | 3.2 ohms | `Driver(re_ohm=3.2)` |
| Le | 0.97 mH | not used (the package is small-signal without inductance) |
| Fs | 53.8 Hz | `Driver(fs_hz=53.8)` |
| Qms | 3.14 | `Driver(qms=3.14)` |
| Qes | 0.40 | `Driver(qes=0.40)` |
| Qts | 0.35 (printed) | not an input: `Driver.qts` gives 0.3548 from Qes and Qms |
| Vas | 3.1 liters | `Driver(vas_m3=litres(3.1))` |
| Sd | 50.3 cm² | `Driver(sd_m2=50.3e-4)` |
| Xmax | 4.0 mm | `Driver(xmax_m=4.0e-3)` |
| Vd | 20.1 cm³ (printed) | not an input: `Driver.vd_m3` gives 20.12 cm³ |
| Mms, Cms, BL | 9.9 g, 0.88 mm/N, 5.2 Tm | not used |
| Sensitivity | 86.8 dB at 2.83 V, 1 m | the tweeter's level trim |
| Power handling | 40 watts RMS | the woofer channel's limit |
| Usable range | 55 to 5,000 Hz | the crossover frequency |
| Overall diameter, cut-out, depth | 4.57, 3.77, 2.32 (the seller's figures, inches) | the baffle |

The spec sheet's title calls the cone "Poly" where the seller's page says "Treated Paper"; the
numbers on the two agree. Neither states any rating for humid or outdoor use.

Tweeter, from the maker's spec sheet above (read 2026-10-06):

| Parameter | Value | Used as |
|---|---|---|
| Nominal impedance, Re | 6 ohms, 5.8 ohms | the amplifier's load |
| Fs | 2125 Hz | the crossover frequency |
| Qms, Qes, Qts | 3.45, 6.09, 2.20 | not used |
| Sensitivity | 88 dB at 2.83 V, 1 m | the tweeter's level trim |
| Power handling | 10 watts RMS | the tweeter channel's limit |
| Usable range | 3,500 to 27,000 Hz | the crossover frequency |
| Overall diameter, cut-out, depth | 1.28, 1.28, 0.57 (the seller's figures, inches; a press fit) | the baffle |

## The box: vented, 1.80 L net, tuned to 60.5 Hz

`vented_box_for_driver(woofer, ql=7)` with
`woofer = Driver(fs_hz=53.8, qes=0.40, qms=3.14, vas_m3=litres(3.1), re_ohm=3.2, sd_m2=50.3e-4, xmax_m=4.0e-3)`
gives:

| Quantity | Value | Call |
|---|---|---|
| Alignment | QB3 | `vented_box_for_driver(woofer, ql=7).alignment` (the driver's Qts, 0.3548 by `Driver.qts`, is below the B4 value 0.4048 of `butterworth_qts(7)`) |
| **Net volume** | **1.804 L** | `.vab_m3` (compliance ratio `.alpha` 1.718) |
| **Tuning** | **60.5 Hz** | `.fb_hz` (60.55; tuning ratio `.h` 1.125) |
| -3 dB frequency of the box | 66.9 Hz | `.f3_hz` |
| Box response | -9.1 dB at 50 Hz, -4.9 dB at 60 Hz, -2.4 dB at 70 Hz, -1.2 dB at 80 Hz, -0.3 dB at 100 Hz | `.response_db(f)` |

The enclosure's leakage is the designer's estimate, `ql = 7`: the value P12 gives for a printed
box and the one its leakage tolerance is written against.

**Net volume means** the air the woofer works into: the cavity after the woofer's own bulk
behind the baffle, the port tube (its air, 40.1 cm³ by `port_area_m2(0.020)` times the length,
and its wall), the ribs and anything else inside have been taken out. The enclosure model sums
those. The box has no fill and no lining in version 1: the alignment is computed for leakage
loss alone, which is all the package models for a vented box.

**Why vented and not sealed.** In the same 1.804 L closed, `sealed_box(woofer, 1.804e-3)` gives
`fc_hz` 88.7 Hz, `qtc` 0.585 and `f3_hz` 110.9 Hz: the vent buys 44 Hz of bass extension from a
box of the same size, which is the difference between a small speaker that carries music alone
and one that needs a subwoofer. The cost is a long vent and its air speed (below).

### The port

One round vent, **20.0 mm inside diameter, 127.7 mm long**, flared at both ends:

| Quantity | Value | Call |
|---|---|---|
| Area | 3.142 cm² | `port_area_m2(0.020)` |
| Length | 127.7 mm | `port_length_m(fb_hz=60.5476, vab_m3=1.80417e-3, area_m2=port_area_m2(0.020))` (the package's default end correction: one flanged end, one free) |
| Peak air speed, at most | 24.4 m/s | `port_air_velocity_m_s(woofer.vd_m3, 60.5476, port_area_m2(0.020))`: the vent carrying the cone's whole displaced volume (20.12 cm³) at the tuning frequency, which is an upper bound reached only at full excursion |
| Trim | 4.8 mm of length per 1 Hz of tuning | `port_trim_m(measured_fb_hz=59.5476, fb_hz=60.5476, vab_m3=1.80417e-3, area_m2=port_area_m2(0.020))` |

The vent is longer than the box is deep, so it bends once inside the box; its length is taken
along its centre line. The package models a straight vent with no flare, bend or loss, so the
length is a starting value (**ASSUMED** to hold through one smooth bend): the first box's
tuning is measured from its impedance curve and the vent corrected with the trim figure. The
package sets no air-speed limit (its sources give none as a design number), so whether 24 m/s
at full excursion is audible is an open item; the fallback is a 22 mm vent, 156.2 mm long, at
most 20.1 m/s (the same two calls with `port_area_m2(0.022)`). The inner end stays at least one
diameter (20 mm) clear of any wall (**ASSUMED** practice). Where the mouth is, and that it
faces down behind a screen on a speaker used outdoors (P12), is the enclosure model's.

## The baffle step: 820 Hz

The baffle is **140 mm wide**. `baffle_step_hz(0.140)` gives 821.4 Hz, used as 820 Hz: below
it the box radiates all round and the response falls towards -6.02 dB (`BAFFLE_STEP_DB`), and
at 820 Hz it is 3.01 dB down (`baffle_step_db(820, 820)`).

The EQ takes back **3 dB** of the 6, not all of it. These speakers stand on a counter or a
shelf near a wall, where the wall returns part of the loss; the package's baffle module quotes
its source that in a room "3 or 4 dB of diffraction loss correction may result in an overall
response that is closer to neutral" and leaves the amount to the designer. 3 dB is **ASSUMED**
until a built box is measured in a room; chorus's room correction works on top of it.

## The crossover: LR4 at 3500 Hz

- **Kind:** LR4, the fourth-order alignment whose two branches are each 6.02 dB down and in
  phase at the crossover and add to a flat sum: two identical second-order sections per branch.
- **Frequency:** 3500 Hz, at 48 kHz.
- **Produced by:** `linkwitz_riley(48000, 3500)`, exported with `design_record(...)` and
  `write_record(...)` as `fixtures/design-record/chorus-compact-v1.json` (below).

Why 3500 Hz: it is the bottom of the tweeter's stated usable range (3,500 Hz) and inside the
woofer's (to 5,000 Hz). At the tweeter's resonance (2125 Hz) the high branch is already 18.8 dB
down (`decibels(linkwitz_riley(48000, 3500).response_high(2125))`), which is what keeps a
10 watt dome out of trouble. The drivers' centres are 82 mm apart or less (**ASSUMED** from the
two outside diameters; the enclosure model places them), under one wavelength at 3500 Hz
(98 mm at 344 m/s, arithmetic).

Each low-branch section (run twice in series), `b0 b1 b2 a1 a2`, `a0 = 1`:

```text
0.03927923259214919  0.07855846518429838  0.03927923259214919  -1.3664078166641995  0.5235247470327963
```

Each high-branch section (run twice in series):

```text
0.7224831409242489  -1.4449662818484978  0.7224831409242489  -1.3664078166641995  0.5235247470327963
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
| 1 | Baffle-step shelf: a high shelf, -3 dB, 820 Hz, Q 0.5 | before the crossover (both branches) | 0.7205354541360051 | -1.2816161552779781 | 0.5699025495695691 | -1.8121506089698867 | 0.8209724573974829 |
| 2 | Woofer protection: a second-order high-pass, 40 Hz, Q 0.7071 | the low branch | 0.9963044429693492 | -1.9926088859386983 | 0.9963044429693492 | -1.992595228750302 | 0.992622543127095 |
| 3 | Tweeter level trim: -1.2 dB | the high branch | 0.8709635899560807 | 0 | 0 | 0 | 0 |

Where each comes from:

1. **The shelf.** Release v1.49.0 of the acoustics package designs low-pass and high-pass
   sections only (its README lists shelving sections as out of scope), so this one section is
   designed by chorus's own cookbook designer:
   `Coefficients::design(Kind::HighShelf, 48000.0, 820.0, 0.5, -3.0)` in `crates/dsp/src/biquad.rs`,
   printed on 2026-10-06. It is a cut above the step, never a boost below it, so it costs no
   headroom. The package checks it: `Biquad(b0, b1, b2, a1, a2).magnitude_db(f, 48000)` on the
   five numbers above gives -0.04 dB at 100 Hz, -0.58 dB at 400 Hz, -1.50 dB at 820 Hz,
   -2.38 dB at 1600 Hz and -2.93 dB at 5000 Hz, within 0.024 dB of half of the full mirror
   network, `baffle_step_compensation_db(f, 820) * 3 / BAFFLE_STEP_DB`, at every one of
   thirteen frequencies from 50 Hz to 20 kHz. (At Q 0.7071 the same shelf is up to 0.45 dB off
   that curve; the first-order step wants the gentler section.)
2. **The high-pass.** `highpass(48000, 40)` (its default Q, `BUTTERWORTH_Q`, 0.7071). Below its
   tuning a vented box stops loading the cone, so the woofer is not driven there. chorus's own
   `Coefficients::design(Kind::Highpass, 48000.0, 40.0, FRAC_1_SQRT_2, 0.0)` gives the same five
   numbers digit for digit. With it the speaker's low end is `.response_db(f)` of the box plus
   `highpass(48000, 40).magnitude_db(f, 48000)`: -5.6 dB at 60 Hz, -2.9 dB at 70 Hz, -1.5 dB at
   80 Hz, -0.4 dB at 100 Hz, so the system is 3 dB down near **69 Hz**.
3. **The trim.** The two sensitivities are both printed for 2.83 V, and both channels of the
   amplifier have the same gain, so the tweeter is turned down by their difference, 88.0 - 86.8
   = 1.2 dB: `b0 = 10^(-1.2/20)` (arithmetic; `Biquad(0.8709635899560807, 0, 0, 0, 0).magnitude_db(1000, 48000)`
   gives -1.2). **ASSUMED** from the datasheets; set by measurement on the first box.

Nothing runs these sections yet. Wiring the record and the EQ into the playback chain or the
firmware, and choosing whether they run in chorus's chain or in the amplifier's own filters
(TI's datasheet for the TAS5825M lists "2 × 15 BQs", page 1), is a later task.

**Channel limits (for that task, ASSUMED from the datasheets):** the woofer channel at most
12.6 V RMS (40 watts into 4 ohms), the tweeter channel at most 7.7 V RMS (10 watts into
6 ohms), both arithmetic. The amplifier's supply is 24 V, so both limits are below what it can
swing and have to be held by chorus's limiter, not by the supply.

## The tolerances an enclosure must hold

Each row's effect is computed by re-running `vented_box(woofer, vab_m3, fb_hz, ql=7)` at the
changed value and comparing `.f3_hz` and `.response_db(f)` from 50 to 300 Hz with the design's;
a changed vent or volume goes through `port_tuning_hz(length_m=..., vab_m3=..., area_m2=...)`
first.

| What | Value and tolerance | Effect at the edge of the tolerance |
|---|---|---|
| Net volume | 1.804 L ± 2% (± 36 mL), P12's figure for a printed box | tuning 59.95 to 61.16 Hz, `f3` 66.5 to 67.5 Hz, response within 0.25 dB |
| Vent length | 127.7 mm ± 0.5 mm along the centre line (P12) | tuning ± 0.1 Hz, response within 0.03 dB |
| Vent inside diameter | 20.0 mm ± 0.2 mm (**ASSUMED** printable; P12 gives ± 0.5 mm on lengths) | tuning 59.97 to 61.12 Hz, response within 0.14 dB |
| Volume and vent length together, worst case | | tuning 59.85 to 61.27 Hz, `f3` 66.5 to 67.4 Hz, response within 0.27 dB |
| Tuning, as measured on the built box | 60.5 Hz ± 3% (58.7 to 62.4 Hz); outside it the vent is trimmed | response within 0.45 dB (± 5% would be 0.76 dB) |
| Leakage | `ql` of 7 or more, shown by the built box's impedance curve (P12) | at `ql` 5: `f3` 70.4 Hz, up to 0.70 dB low; at `ql` 3: `f3` 79.0 Hz, 2.2 dB low. A tighter box only gains: `ql` 10 gives `f3` 64.5 Hz, `ql` 20 gives 61.9 Hz |
| Baffle width | 140 mm ± 2 mm | the step at 809.9 to 833.3 Hz (`baffle_step_hz(0.142)`, `baffle_step_hz(0.138)`) against the EQ's 820 Hz |
| Baffle edges | rounded, 10 mm radius or more (P12) | not computed: the package models the smooth step, not edge ripple |
| Driver centres | 82 mm apart or less, on the baffle's vertical centre line | under one wavelength at the crossover (arithmetic) |
| Walls | P12's construction: no flat span over 80 mm without a rib, first panel resonance near 1.7 kHz or above | not computed here (P12's calculation) |
| Openings | every opening into the cavity (drivers, panel, cable entry, controls, microphone port) sealed, or walled off from it (P12) | counted in the leakage figure |
| Fill | none | the alignment assumes none |

## The endpoint module and the enclosure path

- **The endpoint module (the owner's decision of 2026-10-06, "Separate US modules"):** P1's
  Option B, `docs/proposals/P1-embedded-platform.md` (an ESP32-S3, a TAS5825M, W5500 wired
  Ethernet; approved at Checkpoint K, decisions 0042 and 0057), built from three bought boards rather than the
  Esparagus Audio Brick, which its seller lists as "No longer available" (read 2026-10-06):
  - **Espressif ESP32-S3-DevKitC-1-N8R8:** the ESP32-S3-WROOM-1-N8R8, 8 MB flash and 8 MB
    octal PSRAM (Espressif's user guide v1.1, "Ordering Information").
  - **Sonocotta Louder Raspberry Hat Plus, 1X (2.0):** one TAS5825M, both channels in 2.0 mode,
    I2C address 0x4C, no MCLK line, "7..26V from external source" and a 5 V output that powers
    the host (the maker's README, sonocotta/raspberry-media-center at 3d8a3d7). Channel A
    drives the woofer and channel B the tweeter; which of A and B is the left I2S slot is
    **ASSUMED** (A = left) until a bench tone says.
  - **WIZnet WIZ850io:** a W5500 with its own RJ45 jack and magnetics.

  They are wired by jumper leads on the reference board's Ethernet, I2S, I2C and power-down
  pins; the board profile is `firmware/boards/devkitc-s3-louderhat-wired.conf`, which also names
  the five buttons, the status light and the microphone (`docs/hardware/controls.md`). The
  HAT takes 24 V from a bought PoE+ splitter, the Phihong POE21-240-R, and feeds the DevKitC
  its 5 V. The wiring, the sellers and the sizes of every part are the devices repository's
  build plan for chorus-compact-v1. The acoustics do not depend on the module as long as it
  has two amplifier channels at 24 V.
- **The amplifier's load.** TI's TAS5825M datasheet (page 6, read 2026-10-06) gives the minimum
  speaker load in BTL mode as 3.2 ohms minimum, 4 ohms nominal. The woofer is 4 ohms nominal
  with an Re of 3.2 ohms: at the limit, not under it. The datasheet tabulates no output power
  at 24 V into 4 ohms in that mode, so the woofer channel's power is bounded by the limits
  above and by the PoE supply, and its heat is a thing the first box measures.
- **The enclosure path:** `docs/proposals/P12-enclosures.md` (ACCEPTED 2026-10-07 as written,
  decision 0242): printed (ASA in P12, PETG in the devices repo for now), 8 mm walls with four
  perimeters and 60% infill, ribbed to 80 mm, one body and one gasketed panel, net volume held
  to ± 2% and leakage to `ql` of 7 or better. Those are the tolerances the table above is
  computed against.
- **Controls and microphone:** as `docs/hardware/controls.md` gives the compact class (five
  buttons, the status light, a microphone behind a latching two-pole mute switch that breaks
  its supply) and `docs/hardware/voice-mic.md` lists. This file places and chooses none of
  them; they appear in the budget as allocations.
- **The outdoor pair:** P12 asks the acoustic design for a closed box or a downward-facing
  screened port, and a weather-rated driver, on a deck. Version 1 answers the port (it may face
  down behind a screen) and does not answer the driver: neither driver's page states any
  weather rating. An outdoor variant is an open item, not this version.

## The budget (K89: under about $150 of parts, not labour)

Single-unit list prices in US dollars before shipping and tax, each read from its URL on the
date in its row. The drivers are chorus's chosen parts; the endpoint's boards, the splitter,
the status light, the mute switch, the panel jack and the speaker wire are the parts the devices
repository's build plan chose and priced (read by it on that date). Every line marked
**allocation** is a priced allocation: the part that prices it is an example, and the bill of
materials and the enclosure model choose the real ones. Where a part is sold only in a pack, the line is this
speaker's share of the pack and says so (the room list builds more than one speaker).

| Line | What prices it | Price | URL, date read |
|---|---|---|---|
| Woofer | Dayton Audio TCP115-4, one | 15.98 | https://www.parts-express.com/Dayton-Audio-TCP115-4-4-Treated-Paper-Cone-Midbass-Woofer-4-Ohm-295-415, 2026-10-06 |
| Tweeter | Dayton Audio ND16FA-6, one | 8.98 | https://www.parts-express.com/Dayton-Audio-ND16FA-6-5-8-Soft-Dome-Neodymium-Tweeter-275-025, 2026-10-06 |
| Endpoint board | Espressif ESP32-S3-DevKitC-1-N8R8, Mouser 356-EP32S3DVKTC1N8R8 (a search listing; the live page refused a scripted read) | 15.00 | https://www.mouser.com/en/ProductDetail/Espressif-Systems/ESP32-S3-DevKitC-1-N8R8?qs=7D1LtPJG0i2PiuUUKucutQ%3D%3D, 2026-10-06 |
| Amplifier board | Sonocotta Louder Raspberry Hat Plus, 1X (2.0), TAS5825M, Tindie, 27 in stock | 25.00 | https://www.tindie.com/products/sonocotta/louder-raspberry-hat-plus/, 2026-10-06 |
| Ethernet module | WIZnet WIZ850io (W5500), Mouser (a search listing) | 19.58 | https://www.mouser.com/ProductDetail/WIZnet/WIZ850io?qs=W0yvOO0ixfFLSlENQWBCKg%3D%3D, 2026-10-06 |
| PoE+ splitter | Phihong POE21-240-R, 802.3at to 24 V at 21 W, "Efficiency 80% (typical)", Bravo Electro, "In Stock". The PoE Texas GAT-24V25W (24 V, 25 W, 90%, $31.99) was "Sold out" | 31.35 | https://www.bravoelectro.com/poe21-240-r.html, 2026-10-06 |
| Status light | one NeoPixel Mini Button PCB (WS2812B/SK6812): 1 of a 5 pack at $4.95. The Brick carried its light; these boards do not | 0.99 | https://www.adafruit.com/product/1612, 2026-10-06 |
| Controls (allocation) | five tactile buttons: 5 of a 20 pack at $2.50 | 0.63 | https://www.adafruit.com/product/367, 2026-10-06 |
| Microphone (allocation) | I2S MEMS microphone breakout, SPH0645LM4H (M1 of `docs/hardware/voice-mic.md`), in stock | 6.95 | https://www.adafruit.com/product/3421, 2026-10-06 |
| Mute switch | E-Switch EG2211, a through-hole DPDT slide switch that can be hand-wired, DigiKey (a search listing) | 1.03 | digikey.com search listing, 2026-10-06 |
| Panel Ethernet jack | Adafruit 909, a panel-mount RJ45 extension (version 1 left the cable entry unpriced) | 4.95 | https://www.adafruit.com/product/909, 2026-10-06 |
| Speaker wire | 18 AWG, about 1 m of a 50 ft (15.2 m) roll at $15.98 (a search listing) | 1.05 | parts-express.com, Parts Express 101-782, 2026-10-06 |
| Enclosure: ASA (allocation) | 0.90 kg of a 1 kg spool at $24.99 (P12's mass for a compact box) | 22.49 | https://shop.polymaker.com/products/asa.js, 2026-10-06 |
| Enclosure: panel gasket (allocation) | closed-cell foam gasket tape: 3 ft of a 50 ft roll at $13.98 | 0.84 | https://www.parts-express.com/Speaker-Gasketing-Tape-1-8-x-3-8-x-50-ft.-Roll-260-540, 2026-10-06 |
| Enclosure: threaded inserts (allocation) | M3 brass heat-set inserts: 8 of a 50 pack at $5.95 | 0.95 | https://www.adafruit.com/product/4255, 2026-10-06 |
| Enclosure: screws (allocation) | M3 machine screws: 8 of a 420 piece set at $16.95 | 0.32 | https://www.adafruit.com/product/4685, 2026-10-06 |
| **Total** | | **156.09** | |

Total: 156.09 USD, **over the 150.00 limit by 6.09**.

What the total does and does not say:

- **It is 6.09 over the limit.** The three boards and the splitter (90.93) cost what the
  Brick and the PoE Texas splitter did (90.99); what is new is the status light (0.99, the
  Brick had one), the panel Ethernet jack (4.95) and the speaker wire (1.05), with the mute
  switch 0.72 cheaper. The total counts pack parts by share and the enclosure at 0.90 kg of ASA
  (an estimate for a 2.25 L cavity in 8 mm walls at P12's 78% of a solid wall's mass,
  **ASSUMED** dimensions; a separate electronics chamber would add to it). Bought as whole packs
  for a single speaker (buttons 2.50, light 4.95, inserts 5.95, gasket 13.98, screws 16.95,
  speaker wire 15.98, a whole spool 24.99) the same list is 214.12 USD. Ways back under the
  limit (a sealed cable gland instead of the panel jack, the GAT-24V25W if it returns) are the
  bill of materials' to propose; none is decided here.
- **Ship-from:** the drivers, the gasket and the speaker wire from Springboro, Ohio; the
  DevKitC and the WIZ850io from Mansfield, Texas (Mouser); the light, the microphone, the panel
  jack, the inserts and the screws from Brooklyn, New York ("ALL ORDERS SHIP FROM THE ADAFRUIT
  FACTORY, BROOKLYN, NY, USA", https://www.adafruit.com/shipping). **The amplifier board ships
  from Poland** by UPS, and its page says standard shipments to the US are on hold because of
  new tariffs, so its shipping and any duty are high and not in the price: no TAS58xx board was
  found at a US seller at a usable price on the day read. The splitter's, the switch's and the
  filament's pages state no ship-from; their sellers are US shops (**ASSUMED** US ship-from).
- **Stock:** the DevKitC's and the WIZ850io's prices and stock are search listings, not the
  live pages; the DevKitC was out of stock at DigiKey and Adafruit. The live stock is checked
  when the order is placed.
- **Room in it:** the two-wire PDM microphone (M3, $4.95, https://www.adafruit.com/product/3492,
  read 2026-10-06) instead of M1 takes 2.00 off.
- **Not priced:** hookup wire, jumper leads, the barrel lead from the splitter to the HAT, the
  Ethernet patch lead, the switch's perfboard carrier, a port screen for an outdoor speaker, any
  finish, shipping and tax.

## PoE (K90): Class 4, against a 250 W switch

**The speaker is an 802.3at (PoE+) Type 2, Class 4 powered device.** The class numbers are the
standard's, from the Ethernet Alliance's overview (Table 13, page 35,
https://ethernetalliance.org/wp-content/uploads/2018/04/WP_EA_Overview8023bt_FINAL.pdf, read
2026-10-06): Class 4 is 30.00 W at the switch port and **25.50 W at the powered device**;
Class 3 is 14.00 W and 13.00 W.

What leaves for the amplifier (arithmetic on the numbers cited; nothing measured):

| Step | Watts | From |
|---|---|---|
| At the powered device | 25.5 | Class 4 |
| Out of the splitter at 24 V | 20.40 | 25.5 × 0.80: Phihong's datasheet gives "Efficiency 80% (typical)". Under its 21 W rating, so the class is the bound |
| The endpoint's processor, Ethernet, light and microphone, and the HAT's 5 V and 3.3 V regulators | -1.5 | **ASSUMED**; not measured (the W5500 alone is 0.47 W, 141 mA at 3.3 V, WIZnet) |
| Into the amplifier's supply | about 18.9 | |
| Out of the amplifier to the two drivers, continuous | about 15 to 17 | × 0.80 to 0.90: TI's datasheet gives "> 90% Power efficiency"; 0.80 is the floor version 1 took from the Brick's page |

So about **15 to 17 W continuous** for the two drivers together, about 2 W under version 1's 17
to 19 W because this splitter is 80% efficient and the sold-out PoE Texas one 90% (P1 estimated
18 to 20 W), inside the woofer's 40 watt rating; short peaks above it come from the board's supply
capacitors (**ASSUMED**, not sized here). The 25.5 W ceiling is the firmware's to respect: a
speaker that draws more is shut off by the switch.

**The switch** (P3: 250 W in total, 30 W per port, 802.3af/at; re-read 2026-10-06 at
https://www.omadanetworks.com/us/business-networking/omada-switch-agile/es228gp/: "PoE Budget:
250W available for all PoE+ ports, 30W max per port", 24 PoE+ ports). Its user guide
(https://static.tp-link.com/upload/manual/2026/202605/20260527/1900002935_Omada%20Agile%20(Easy%20Managed)%20Switch_UG.pdf,
pages 106 to 113, read 2026-10-06) gives the port limit for "Class 4" as 30 W and for "Class 3"
as 15.4 W, a default "System Power Limit 240 W", and powers off low-priority ports "when the
supply power exceeds the system power limit". It does not say whether the budget is reserved by
class or counted by measured draw.

By class allocation (**ASSUMED** to be how the switch budgets, as in P3), each compact takes
30 W of the budget whatever it draws:

| Case | Arithmetic | Fits? |
|---|---|---|
| 8 rooms, one compact each, nothing else powered | 8 × 30 = 240 W of 250 W | **Yes, 8 fit**, with 10 W left; it is exactly the guide's default 240 W system limit |
| 8 rooms with the access point also on the switch | 240 + 15.4 = 255.4 W | **No: 7 fit** (7 × 30 + 15.4 = 225.4 W). The access point's class is not published; 15.4 W is the Class 3 port limit (**ASSUMED**, P3) |
| The owner's room list: nine PoE+ compacts | 9 × 30 = 270 W | **No: 8 of the 9 fit** by class with nothing else powered, **7 of the 9** with the access point at 15.4 W |

By measured draw, if the switch counts that instead (**ASSUMED** possible; not stated by the
guide): nine compacts and the access point (at most 6.5 W, P3) fit while the nine average
(250 - 6.5) / 9 = 27.1 W or less at the port, or 25.9 W at the default 240 W limit. A Class 4
device may draw up to 30 W at the port, so nine at full power at once (270 W) do not fit even
then; they fit in practice only because music's average power is far below its peak
(**ASSUMED**), or if the firmware caps each speaker's draw.

Said plainly: **by class, 8 compacts fit on this switch alone and 7 beside the access point;
the room list's nine do not fit by class.** What is ASSUMED: that the switch allocates by
class, the access point's class, the 1.5 W of the endpoint's own electronics, the amplifier
and splitter efficiencies as their makers state them, and that Phihong's splitter negotiates
Class 4 (its datasheet does not state the class; 21 W out at 80% needs 26.25 W in, above Class
3's 13.00 W at the device). How the ninth speaker (and the eighth beside the access point) is powered, by a second
PoE source, by measured-draw budgeting shown on the switch's own PoE page, or by a firmware
power cap, is the owner's network decision and an open item of P3; this file decides none.

## The design record

`fixtures/design-record/chorus-compact-v1.json` is the crossover above as a version 1
`speaker-design-record`, exported from release v1.49.0 on 2026-10-06 and committed byte for
byte, with `chorus-compact-v1.provenance` beside it (the release tag, the export command with
placeholders for the package's name, the date and the sha256
`eb578e02df7df63e3b376e8b2f49a9fda812ab0898fcd159de03f35ddce9b67d`). A version 1 record holds
crossovers only: the box, the vent and the EQ sections live in this file until a later schema
can carry them.

`crates/dsp/tests/design_record.rs`, `the_compact_record_is_the_exported_one_and_runs`, holds
the file to that sha256, reads it with chorus's own reader, and runs its crossover through
chorus's own `f32` filters: on 2026-10-06 the branches ran at -6.020599 and -6.020601 dB at
3500 Hz and the sum at 0.000000 dB, within 0.000006 dB of the record at all six of its
response points (500, 1000, 1750, 3500, 7000 and 14000 Hz; the test's tolerance is 0.001 dB).
`docs/dsp.md`, "Design records", describes the reader.

## Open items

- **Everything is unmeasured.** The first box gives: the impedance curve (tuning, `ql`), the
  near-field and on-axis response (the shelf's 3 dB, the tweeter's trim and delay, the drivers'
  own roll-offs at 3500 Hz), vent noise at full level, and the amplifier's temperature into a
  4 ohm woofer at 24 V.
- **The drivers' real parameters.** One datasheet line per driver; unit-to-unit spread is not
  stated by the maker and is not modelled.
- **Vent noise** at up to 24.4 m/s: the 22 mm fallback is computed above.
- **The outdoor pair:** no weather-rated driver is chosen.
- **P12 is ACCEPTED** (decision 0242): the construction is its compact answer; the devices
  repo prints it in PETG until shopkit allows ASA off a car part. The tolerances are **ASSUMED**
  until the first box is measured (P1 is approved: Option B, decisions 0042 and 0057).
- **The budget is 6.09 USD over** on shares of packs, before the unpriced lines and the
  amplifier board's shipping from Poland.
- **The amplifier's channel order:** which of the HAT's channels A and B is the left I2S slot
  (ASSUMED A), confirmed with a tone on one channel before a driver is connected.
- **PoE for nine:** above.

## Sources

Read 2026-10-06 unless a line says otherwise.

- The woofer and the tweeter: the seller's item service and the maker's spec sheets, URLs in
  "Drivers"; the seller's shipping page, https://www.parts-express.com/shipping.
- The acoustics package of the owner's shared Python library, release v1.49.0: its README
  (sealed and vented boxes, ports, the baffle step, biquads, crossovers, "The design record")
  and its `baffle` module's notes. The package cites its own sources for every formula.
- The amplifier: TI, TAS5825M datasheet (SLASEH7H), https://www.ti.com/lit/ds/symlink/tas5825m.pdf,
  pages 1, 6 and 8.
- The endpoint modules: the devices repository's build plan for chorus-compact-v1 (NSchatz/devices
  at 6bdbb10, `projects/chorus-compact/v1/log.md`, then under devices' old
  `builds/` directory), which read the sellers' pages in the budget;
  Espressif's ESP32-S3-DevKitC-1 user guide v1.1,
  https://docs.espressif.com/projects/esp-dev-kits/en/latest/esp32s3/esp32-s3-devkitc-1/user_guide_v1.1.html;
  the Louder Raspberry Hat Plus's README, https://github.com/sonocotta/raspberry-media-center/blob/3d8a3d7f28bb6956408fb2f7a1ac272632ae7c46/README.md
  (GPL-3.0: the README only, never its design files); WIZnet,
  https://docs.wiznet.io/Product/ioModule/WIZ850io. The Brick's page,
  https://www.crowdsupply.com/sonocotta/esparagus-audio-brick ("No longer available").
- The splitter: Phihong's datasheet, https://www.phihong.com/wp-content/uploads/POE21.pdf;
  version 1's, https://shop.poetexas.com/products/gat-24v25w ("Sold out").
- PoE classes: the Ethernet Alliance overview above. The switch: its product page and user
  guide above.
- The budget's other lines: the URLs in the table.
- In this repository: `docs/proposals/P1-embedded-platform.md`, `docs/proposals/P3-speaker-network.md`,
  `docs/proposals/P12-enclosures.md`, `docs/hardware/controls.md`, `docs/hardware/voice-mic.md`,
  `docs/research/tas5825m-register-map.md`, `docs/dsp.md`, `crates/dsp/src/biquad.rs`.
- No GPL source and no reciprocally licensed hardware design file was opened; no speaker
  design program's source was read.
