# The subwoofer: acoustic design, amplifier sizing and controls

version: 1

- Status: designed on paper, 2026-10-06. Nothing is ordered, cut, built or measured. Every
  number here is a datasheet value or a calculation from datasheet values (CLAUDE.md rule 6:
  starting points, not truth); the first built box is measured before any number here is called
  confirmed.
- What this is: chorus's source of truth for the subwoofer's acoustics (K22, K69, K90): the
  driver and the amplifier in three priced tiers, the box alignment and port, the protective
  high-pass, the amplifier's size and the arithmetic behind it, the low-pass and bass
  management, the level and phase knobs and what each sets in chorus's DSP, the pairing button
  and status light, the tolerances an enclosure must hold, and the priced budget. The enclosure
  model, the board and electronics plan, the knob and ADC pins, the bill of materials and the
  part records are made from it elsewhere and are not in this file.
- The choice of driver, alignment, amplifier power and crossover, and its reasons: the decision
  record `docs/decisions/0229-the-subwoofer-driver-alignment-and-amplifier.md`. The driver and
  amplifier survey: `docs/research/subwoofer-drivers-and-amplifiers.md`.
- A change to any design number bumps `version` and re-exports the record.

**How the numbers were computed.** Every computed number names the call that produced it. The
calls are those of the acoustics package of the owner's shared Python library at release
**v1.50.0** (the current release on 2026-10-06; between v1.49.0, the compact's and the
two-way's release, and v1.50.0 the acoustics package changed only its own version number), run
with `uv run` from the release tag on 2026-10-06 (the same release and method as the design
record's provenance, `fixtures/design-record/chorus-sub-v1.provenance`). The package's air is
its default, `AIR_SB` (1.2 kg/m³, 344 m/s). Where the package has no call for a number (it
computes no cone excursion and no vent volume for a vented box, and no sound level), the number
is marked **arithmetic**, the formula is written out beside it, and its inputs are package
calls or sheet values.

## The shape

One 12 inch driver in a braced MDF box with one round vent, a bought mains-powered plate
amplifier of 100 W in the box's rear wall, and chorus's endpoint beside it. The endpoint does
all the signal processing (the low-pass, the level, the polarity, the protective high-pass) at
48 kHz in chorus's DSP form and hands the amplifier a line-level signal; the amplifier's own
filters are set out of the way once. Power is the mains; the network is wired Ethernet. The
rear panel carries the amplifier's plate, a pairing button, a level knob, a phase knob and the
status light.

## Driver

Three tiers, priced, each a driver with its amplifier. **Version 1 designs the good tier**:
every acoustic number below is for its driver. The better and best tiers share one dearer
driver and differ in their amplifier; they are priced alternatives with the box the package
designs for that driver, and taking one is a new `version` (a new box, a new vent, a new
high-pass, a new limit).

Prices are single-unit list prices in US dollars, before shipping and tax, read 2026-10-06.
The seller's product pages are drawn by script and give an automated read an empty page, so
each price and stock state was read from the seller's own item service (the "Price as read"
URL), which is what the page itself shows.

| Tier | Driver | Amplifier | Driver and amplifier | The package's box for the driver | Output at the amplifier's limit (arithmetic, "Amplifier sizing") |
|---|---|---|---|---|---|
| **good (designed)** | Dayton Audio SD315A-88, $89.98 | Dayton Audio SPA100-D, 100 W into 4 ohms, $109.98 | **$199.96** | QB3, 73.54 L, 28.7 Hz, -3 dB at 32.8 Hz | 110.7 dB at 1 m |
| better | Dayton Audio RSS315HO-44, $249.98 | Dayton Audio SPA250, 156 W into 8 ohms, $206.98 | $456.96 | QB3, 40.78 L, 23.3 Hz, -3 dB at 25.1 Hz | 106.6 dB at 1 m |
| best | Dayton Audio RSS315HO-44, $249.98 | Dayton Audio SPA500, 273 W into 8 ohms, $399.98 | $649.96 | the same as better | 109.0 dB at 1 m |

What the tiers buy is extension in a smaller box, not output: the dearer driver reaches 7.6 Hz
lower from 55% of the volume, and its efficiency is a quarter of the good tier's (0.14% against
0.56%, `Driver.efficiency(AIR_SB)`), so it needs several times the power for the same level.
The good tier is the loudest per dollar and the only one of the three whose vent is short
enough to build as a straight tube (the dearer driver's 40.78 L at 23.3 Hz wants a 4 inch vent
1019 mm long, `port_length_m`, which is a folded vent or a passive radiator; the package
designs neither). The RSS315HO-44's row is
`vented_box_for_driver(Driver(fs_hz=21.5, qes=0.41, qms=3.70, vas_m3=litres(61.3), re_ohm=6.5, sd_m2=507.1e-4, xmax_m=14.0e-3), ql=7)`,
with its two 4 ohm coils in series (its sheet prints "4+4 ohms" and Re 6.5 ohms; the series
wiring is **ASSUMED** from those two figures, the sheet does not say).

### The designed tier's driver

| | Driver |
|---|---|
| Make | Dayton Audio |
| Model | SD315A-88, 12 inch subwoofer, dual voice coil, 8 ohms per coil |
| Vendor | Parts Express, part 295-488 |
| URL | https://www.parts-express.com/Dayton-Audio-SD315A-88-12-DVC-Subwoofer-295-488 |
| Price as read | https://www.parts-express.com/api/items?country=US&currency=USD&language=en&fieldset=details&q=295-488 |
| Price | $89.98, in stock (26) |
| Date read | 2026-10-06 |
| Ship-from | Springboro, Ohio, USA (the seller's shipping page, https://www.parts-express.com/shipping, footer "725 Pleasant Valley Dr. Springboro, OH 45066 USA", read 2026-10-06; the page has no sentence that says where orders ship from, so the footer address is **ASSUMED** to be the warehouse, as in the compact's and the two-way's surveys) |
| Maker's spec sheet | https://www.daytonaudio.com/images/resources/295-488-dayton-audio-sd315a-88-specifications-47086.pdf |
| Maker's page | https://www.daytonaudio.com/product/136/sd315a-88-12-dvc-subwoofer-4-ohm |

### The alternatives' driver

| | Driver (better, best) |
|---|---|
| Make | Dayton Audio |
| Model | RSS315HO-44, 12 inch Reference HO subwoofer, dual voice coil, 4 ohms per coil |
| Vendor | Parts Express, part 295-467 |
| URL | https://www.parts-express.com/Dayton-Audio-RSS315HO-44-12-Reference-Series-HO-DVC-Subwoofer-295-467 |
| Price as read | https://www.parts-express.com/api/items?country=US&currency=USD&language=en&fieldset=details&q=295-467 |
| Price | $249.98, in stock (37) |
| Date read | 2026-10-06 |
| Ship-from | Springboro, Ohio, USA (as above) |
| Maker's spec sheet | https://www.daytonaudio.com/images/resources/295-467--rss315ho-44-reference-ho-dvc-subwoofer-specifications.pdf |
| Values printed | 4+4 ohms, Re 6.5 ohms, Fs 21.5 Hz, Qms 3.70, Qes 0.41, Qts 0.37, Vas 61.3 liters, Sd 507.1 cm², Xmax 14.0 mm, Vd 720.6 cm³, 84.6 dB @ 2.83V/1m, 700 watts, 20 - 500 Hz |

### The Thiele/Small values used, and their source

From the maker's spec sheet above (read 2026-10-06; the seller's item service gives the same
values except where a row says so):

| Parameter | Value | Used as |
|---|---|---|
| Nominal impedance | "4 ohms", two 8 ohm coils. The sheet does not say how the coils were wired for its figures; a Re of 3.10 ohms for two 8 ohm coils fits parallel, so parallel is **ASSUMED** and is how the driver is wired here | the amplifier's load, 4 ohms |
| Re | 3.10 ohms | `Driver(re_ohm=3.10)` |
| Le | 1.50 mH | not used (the package is small-signal without inductance) |
| Fs | 24.2 Hz | `Driver(fs_hz=24.2)` |
| Qms | 3.54 | `Driver(qms=3.54)` |
| Qes | 0.37 | `Driver(qes=0.37)` |
| Qts | 0.33 (printed) | not an input: `Driver.qts` gives 0.3350 from Qes and Qms |
| Vas | 151.7 liters | `Driver(vas_m3=litres(151.7))` |
| Sd | 522.8 cm² | `Driver(sd_m2=522.8e-4)` |
| Xmax | 7.0 mm | `Driver(xmax_m=7.0e-3)`: the excursion limit |
| Vd | 366.0 cm³ (printed) | not an input: `Driver.vd_m3` gives 365.96 cm³ |
| BL | 11.86 Tm | the excursion arithmetic |
| Cms | 0.40 mm/N | the excursion arithmetic |
| Mms | 109.20 g | a check only: with Cms it gives a resonance of 24.1 Hz and with BL and Re a Qes of 0.366 (arithmetic), so the sheet's line agrees with itself |
| Sensitivity | 89.6 dB @ 1W/1m (the item service's field reads 93.75, which is the same driver at 2.83 V: `Driver.efficiency(AIR_SB)` gives 0.5635%, 89.7 dB for 1 W and 93.8 dB for 2.83 V by the arithmetic of "Amplifier sizing") | the output figures |
| Power handling | 120 watts RMS, 240 watts maximum | the thermal limit |
| Usable range | 24 to 2,000 Hz | the low-pass range |
| Overall diameter, cut-out, depth | 314 mm, 272 mm, 130 mm (the maker's page); five mounting holes on an 11.96 inch circle (the item service, no unit printed; inches **ASSUMED**: 303.8 mm) | the baffle |

The sheet states no rating for humid or outdoor use, and no volume for the driver's own bulk.

## The box: vented, 73.54 L net, tuned to 28.7 Hz

`vented_box_for_driver(driver, ql=7)` with
`driver = Driver(fs_hz=24.2, qes=0.37, qms=3.54, vas_m3=litres(151.7), re_ohm=3.10, sd_m2=522.8e-4, xmax_m=7.0e-3)`
gives:

| Quantity | Value | Call |
|---|---|---|
| Alignment | QB3 | `vented_box_for_driver(driver, ql=7).alignment` (the driver's Qts, 0.3350 by `Driver.qts`, is below the B4 value 0.4048 of `butterworth_qts(7)`) |
| **Net volume** | **73.54 L** | `.vab_m3` (compliance ratio `.alpha` 2.063) |
| **Tuning** | **28.7 Hz** | `.fb_hz` (28.708; tuning ratio `.h` 1.186) |
| -3 dB frequency of the box | 32.8 Hz | `.f3_hz` |
| Box response | -14.2 dB at 20 Hz, -8.3 dB at 25 Hz, -4.4 dB at 30 Hz, -2.2 dB at 35 Hz, -1.1 dB at 40 Hz, -0.3 dB at 50 Hz, -0.02 dB at 80 Hz | `.response_db(f)` |

The enclosure's leakage is the designer's estimate, `ql = 7`: the value P12 says a design
assumes ("a design assumes Q_L = 7 and a good box does better") and the one the leakage
tolerance below is written against.

**Net volume means** the air the driver works into: the cavity after the driver's own bulk
behind the baffle, the vent tube (its air, 2.65 L by `port_area_m2(0.1016)` times the length,
and its wall), the braces, the amplifier's body (it reaches 2 1/8 inches into the box behind
its plate, the maker's figure), the endpoint's carrier if it sits in the cavity and anything
else inside have been taken out. The enclosure model sums those. P12's 50 L for this class was
illustrative; this is the real figure. The box has no fill and no lining in version 1: the
alignment is computed for leakage loss alone, which is all the package models for a vented
box.

**Why vented and not sealed.** In the same 73.54 L closed, `sealed_box(driver, 73.542e-3)`
gives `fc_hz` 42.4 Hz, `qtc` 0.586 and `f3_hz` 52.8 Hz: above the room's two-ways (48.5 Hz,
`docs/hardware/twoway-speaker.md`), so a closed box of this driver would add nothing under
them without a boost the package does not design and the cone's 7 mm could not follow. The
vent buys 20 Hz of extension from the same box. The sealed boxes of the survey's other
drivers end between 32 and 61 Hz (`sealed_box_for_qtc`, the survey's table).

There is no baffle step to correct: `baffle_step_hz(0.45)` puts it at 256 Hz for a box 450 mm
wide (**ASSUMED** width), above the whole band the low-pass passes.

### The port

One round vent, **4 inches (101.6 mm) inside diameter, 326.6 mm long**, flared at both ends:

| Quantity | Value | Call |
|---|---|---|
| Area | 81.07 cm² | `port_area_m2(0.1016)` |
| Length | 326.6 mm | `port_length_m(fb_hz=28.7081, vab_m3=73.5420e-3, area_m2=port_area_m2(0.1016))` (the package's default end correction: one flanged end, one free) |
| **Peak air speed at rated power** | **17.5 m/s** | `port_air_velocity_m_s(782.9e-6, 28.77, port_area_m2(0.1016))`: the vent carrying 782.9 cm³ peak at 28.77 Hz, the frequency where its air moves fastest, with 19.83 V RMS at the driver (the excursion-limited voltage, 98 W into 4 ohms) and the high-pass in place. At the amplifier's full 100 W (20.0 V) the same call gives 17.6 m/s |
| Trim | 29.5 mm of length per 1 Hz of tuning | `port_trim_m(measured_fb_hz=27.7081, fb_hz=28.7081, vab_m3=73.5420e-3, area_m2=port_area_m2(0.1016))` |

**The volume the vent carries is arithmetic**, because the package leaves it to the caller
("the volume the vent carries is the caller's estimate"). With the vent a mass on the box's
air and the leak a resistance beside it, the vent's volume is the cone's times
`1 / |1 - (f/fb)² + j f/(fb ql)|`: at the tuning the cone moves least (2.14 mm peak at
19.83 V) and the vent carries `ql` times the cone's volume, 782.9 cm³ here, which is 2.1 times
the cone's whole displaced volume (366 cm³). The cone's travel is the arithmetic of "Amplifier
sizing". A vent sized by the cone's displaced volume alone
(`port_air_velocity_m_s(driver.vd_m3, 28.7081, port_area_m2(0.1016))`, 8.1 m/s) would read
less than half the real speed.

The package models a straight vent with no flare and no loss, so the length is a starting
value: the first box's tuning is measured from its impedance curve and the vent corrected with
the trim figure. The package sets no air-speed limit (its sources give none as a design
number); 17.5 m/s at the driver's limit is why the vent is the widest that still fits. A
3 inch vent would be 169.8 mm long at 31.0 m/s (the same calls with `port_area_m2(0.0762)`).
The fallback if the first box's vent is audible is two 3 inch vents, each 372.2 mm long, at
15.5 m/s (`port_area_m2(0.0762, count=2)`). The inner end stays at least one diameter
(102 mm) clear of any wall (**ASSUMED** practice), so the cavity is at least 430 mm deep along
the vent. The budget's allocation is a bought 4 inch flared tube kit whose page says "Create a
port up to 17" long" (431.8 mm); its inside diameter is **ASSUMED** to be its nominal 4 inches
until one is measured, and P12's "printed port flare if the design is vented" is the other way
to make the two flares. Whether the mouth is on the front or the rear is the enclosure
model's.

## The DSP sections

Two sections at a sample rate of **48000 Hz**, in chorus's form: `b0 b1 b2 a1 a2` with
`a0 = 1`, the form `chorus_dsp::biquad::Coefficients` holds and the design record uses. They
are the protective high-pass. **There is no EQ in version 1**: the alignment is flat by
design (QB3), the box has no baffle step in its band, and the room's own peaks are the room
correction's.

| # | What | Where | b0 | b1 | b2 | a1 | a2 |
|---|---|---|---|---|---|---|---|
| 1 | Protective high-pass, first section: second-order, 24 Hz, Q 0.5412 | the subwoofer's feed, after the low-pass | 0.9971034911904271 | -1.9942069823808541 | 0.9971034911904271 | -1.9942020618642575 | 0.9942119028974503 |
| 2 | Protective high-pass, second section: second-order, 24 Hz, Q 1.3066 | the same, in series | 0.998796745711351 | -1.997593491422702 | 0.998796745711351 | -1.9975885625502157 | 0.9975984202951881 |

Where they come from: `highpass(48000, 24, 0.541196100146197)` and
`highpass(48000, 24, 1.3065629648763764)`. The two Qs are the fourth-order Butterworth pair,
`1 / (2 cos(pi/8))` and `1 / (2 cos(3 pi/8))` (arithmetic), so the two in series are 3.01 dB
down at 24 Hz and fall at 24 dB per octave under it:
`decibels(cascade_response([s1, s2], f, 48000))` gives -30.4 dB at 10 Hz, -16.4 dB at 15 Hz,
-7.2 dB at 20 Hz, -3.0 dB at 24 Hz, -0.9 dB at 28.7 Hz, -0.2 dB at 35 Hz and -0.07 dB at
40 Hz.

Why it is there: below its tuning a vented box stops loading the cone. Without a high-pass
this cone moves 1.59 mm per volt at 10 Hz (the arithmetic of "Amplifier sizing"), so 4.4 V,
5 W, would take it to its 7 mm. A second-order high-pass is not steep enough to change which
frequency limits the driver (at 24 Hz it leaves 0.42 mm per volt at 16.9 Hz against 0.34 above
the tuning); the fourth-order one at 24 Hz holds the cone under its travel above the tuning at
every lower frequency (at most 0.296 mm per volt, at 21.0 Hz), and costs 0.7 Hz of extension.

**The subwoofer's low end, box and high-pass together** (`.response_db(f)` plus the cascade's
level): -21.4 dB at 20 Hz, -10.6 dB at 25 Hz, -5.1 dB at 30 Hz, -2.4 dB at 35 Hz, -1.2 dB at
40 Hz, -0.3 dB at 50 Hz, so the system is 3 dB down at **33.5 Hz**.

Nothing runs these sections yet. chorus's chain has no per-endpoint high-pass stage for an
`LFE` role today (`docs/dsp.md`, chain item 8); wiring these two sections and the limit of
"Amplifier sizing" into the playback chain or the firmware is a later task (a follow-up, not
part of this design).

## Amplifier sizing

**The target.** Output: the bass of the room's left, centre and right two-ways together. The
owner's room list has two 5.1 rooms, each three active two-ways and one subwoofer, and with a
subwoofer bonded the mains' bass is the subwoofer's to play (`docs/dsp.md`, chain item 8: the
`LFE` role plays "the LR4 low branch of the sum of the stream's main channels"). One two-way
at its amplifier's 30 W is 100.9 dB at 1 m (86.1 dB at 2.83 V and 15.49 V RMS, 30 W into
8 ohms: `86.1 + 20 log10(15.49 / 2.83)`, arithmetic on `docs/hardware/twoway-speaker.md`), and
three in phase are 9.5 dB more: **110.4 dB at 1 m**, into half space, above 40 Hz. Extension:
**3 dB down at 35 Hz or lower**, under the two-ways' own 48.5 Hz and under the lowest
crossover the chain allows (40 Hz). Both targets are this design's (**ASSUMED**: no decision
sets a level for the class); the stream's LFE channel at +10 dB comes on top and is the chain
limiter's to hold.

**Sound level from volts (arithmetic).** One acoustic watt into half space is 112.15 dB at 1 m
(`20 log10(sqrt(rho c / (2 pi)) / 20e-6)` with `AIR_SB`'s 1.2 kg/m³ and 344 m/s). The
driver's reference efficiency, `driver.efficiency(AIR_SB)`, is 0.005635 of the power `V² / Re`,
so the level in the pass band is `112.15 + 10 log10(0.005635 V² / 3.10)`: 89.7 dB for the 1 W
the sheet's 89.6 dB is printed for, and 93.8 dB at 2.83 V.

**What the target needs.** 110.4 dB is `V² = 3.10 x 10^((110.4 - 112.15) / 10) / 0.005635`,
19.18 V RMS, which is **92 W into the driver's nominal 4 ohms** (`19.18² / 4`).

**The excursion-limited power.** The package computes a displacement-limited power for a
closed box only, so the cone's travel in this vented box is arithmetic on the package's own
alignment:

- At rest a volt moves the cone `BL x Cms / Re` = `11.86 x 0.40e-3 / 3.10` = 1.530 mm.
- At a frequency `f` the travel is that times `|1 - (f/fb)² + j f/(fb ql)| / |D(f)|`, where
  `D` is the alignment's own fourth-order polynomial,
  `x⁴ + a1 x³ + a2 x² + a3 x + 1` with `x = j f / f0` and the box's `.f0_hz` 26.358,
  `.a1` 2.8964, `.a2` 4.1946 and `.a3` 3.3825. The numerator is what the vent and the leak
  take from the cone. `|x⁴ / D|` is the box's response, and it reproduces `.response_db(f)` to
  7e-15 dB at nine frequencies from 10 to 200 Hz, which is the check that `D` is the
  package's.
- With the high-pass in front, the peak travel per volt RMS (the above times `sqrt(2)` and the
  cascade's level) is 0.108 mm at the tuning, 0.273 mm at 35 Hz, 0.343 mm at 40 Hz,
  **0.353 mm at 43.3 Hz (the largest)**, 0.330 mm at 50 Hz, 0.269 mm at 60 Hz and 0.173 mm at
  80 Hz; under the tuning it is at most 0.296 mm, at 21.0 Hz.
- The sheet's Xmax, 7.0 mm, is reached at `7.0 / 0.353` = **19.83 V RMS: 98 W into 4 ohms**
  (`19.83² / 4`), by a sine at 43.3 Hz. Music's bass is seldom a full-level sine at the one
  worst frequency, so this is the cautious figure.

**The thermally limited power.** The maker's rating, 120 watts RMS (240 watts maximum). Into
the nominal 4 ohms that is `sqrt(120 x 4)` = **21.9 V RMS: 120 W**.

**The amplifier power chosen: 100 W into 4 ohms** (20.0 V RMS). It is the size at which the
amplifier runs out where the cone does: 92 W needed, 98 W at the cone's limit, 120 W at the
coil's. At its full 20.0 V the cone would travel 7.06 mm, 1% past the sheet's figure, so the
endpoint's output is set to stop at 19.8 V at the driver (**ASSUMED** procedure: the
amplifier's gain is set once, by measurement on the first box, so that the endpoint's full
scale is 19.8 V RMS at the driver's terminals; nothing in the chain sets a per-endpoint
voltage limit today, a follow-up with the high-pass). At 19.83 V the level is
**110.7 dB at 1 m** in the pass band, 0.3 dB over the target; with the low end's levels above
it is 110.4 dB at 50 Hz, 109.5 dB at 40 Hz and 107.7 dB at the system's 33.5 Hz. A larger
amplifier would buy nothing this driver can use.

**Why not the endpoint's own amplifier.** P1's module carries a TAS5825M, and its maker's page
says it can be run as one bridged-parallel channel. TI's datasheet gives that mode "1 × 53 W,
1.0 Mode (4-Ω, 22 V, THD+N=1%)" (SLASEH7H, page 1; minimum load 1.6 ohms, page 6). 53 W into
4 ohms is 14.56 V, 108.0 dB: 2.4 dB under the target and 2.7 dB under what the cone allows,
for no added part. It is the fallback if the class's budget must come down by the
amplifier's price, and it is not the design because K22 gives this class a "bigger amp" and
the driver has the travel for one.

### The amplifier, per tier

A bought module, mains-powered with its own supply, fed a line-level signal by the endpoint.
Prices and stock from the seller's item service
(`https://www.parts-express.com/api/items?country=US&currency=USD&language=en&fieldset=details&q=<part>`)
on 2026-10-06; rated power as each maker's manual prints it.

| Tier | Amplifier | Rated power as printed | Price | URL, date read |
|---|---|---|---|---|
| **good (designed)** | Dayton Audio SPA100-D plate amplifier, part 300-805 | "Rated Power Output: 100 watts RMS into 4 ohms @ < 1.0% THD" | $109.98, in stock (228) | https://www.parts-express.com/Dayton-Audio-SPA100-D-100-Watt-Class-D-Subwoofer-Plate-Amplifier-300-805, 2026-10-06 |
| good, the alternative with no filters of its own | ICEpower 100AS1 module, part 326-260 (a bare board with its mains supply; its wiring harness is sold separately) | "Power output: 1 x 100W RMS @ 4 ohms 1% THD+N, 1 kHz" | $119.98, in stock (19) | https://www.parts-express.com/ICEpower-100AS1-Class-D-Amplifier-Module-with-Built-In-Power-Supply-1-x-100W-326-260, 2026-10-06 |
| better | Dayton Audio SPA250 plate amplifier, part 300-803 | "156 watts RMS into 8 ohms @ 0.1% THD / 252 watts RMS into 4 ohms @ < 1.0% THD" | $206.98, in stock (204) | https://www.parts-express.com/Dayton-Audio-SPA250-250-Watt-Subwoofer-Amplifier-300-803, 2026-10-06 |
| best | Dayton Audio SPA500 plate amplifier, part 300-807 | "(0.92 % THD) 273 watts* into 8 ohms, 540 watts* into 4 ohms / *Based on one-third power duty cycle" | $399.98, in stock (247) | https://www.parts-express.com/Dayton-Audio-SPA500-500W-Subwoofer-Plate-Amplifier-300-807, 2026-10-06 |

- **The designed tier's SPA100-D** (its manual,
  https://www.daytonaudio.com/images/resources/300-805-dayton-audio-spa100-d-user-manual.pdf):
  "Power Requirements: 115/230 VAC, 60 Hz/50 Hz, 100W" through an IEC inlet on its plate, two
  line-level RCA inputs, "Enclosure Cutout: 7" H x 6" W", "Dimensions: 8-1/16" H x 7-1/16" W x
  2-1/8" D". It is a finished mains assembly behind a metal plate, which keeps mains wiring
  the owner makes out of the wooden box. Its own controls are set once and left: the gain as
  above; the low-pass ("Variable 40 Hz – 200 Hz", which the manual says is defeated only by
  "turning the frequency control to maximum (200 Hz)") at its maximum; the phase switch
  ("Switched 0° or 180°") at 0; the bass boost ("Switchable, 6 dB @ 35 Hz") off; the power
  switch on, not auto, so the first note is not lost (**ASSUMED** setting). Its low-pass at
  200 Hz stays in the path: it is two and a half times the default crossover and its phase at
  80 Hz is part of what the phase knob and the first measurement take up (an open item). No
  safety listing was found in the manual's text.
- **The better and best tiers' load is 8 ohms** (the RSS315HO-44's coils in series), so each
  amplifier's 8 ohm figure is the one that counts. That driver's limits by the same arithmetic
  (a fourth-order high-pass at 19 Hz, `BL` 26.5 Tm, `Cms` 0.17 mm/N, Xmax 14.0 mm, 700 watts):
  the cone at 71.1 V RMS, 631 W into 8 ohms; the coil at 74.8 V, 700 W. No amplifier surveyed
  reaches either, so both tiers are limited by the amplifier: 156 W is 35.3 V and 106.6 dB,
  273 W is 46.7 V and 109.0 dB (`112.15 + 10 log10(0.001441 V² / 6.5)`).

## Bass management

- **The low-pass kind:** LR4, the fourth-order alignment whose two branches are each 6.02 dB
  down and in phase at the crossover and add to a flat sum: two identical second-order
  sections per branch.
- **The default frequency:** **80 Hz**, at 48 kHz: the chain's own default
  (`CROSSOVER_DEFAULT_HZ` in `crates/dsp/src/settings.rs`, with its source there).
- **Produced by:** `linkwitz_riley(48000, 80)`, exported with `design_record(...)` and
  `write_record(...)` as `fixtures/design-record/chorus-sub-v1.json` (below).

**How it pairs with a main's high-pass in a bonded set.** It is `docs/dsp.md`, chain item 8,
"Bass management": "A main role, with a subwoofer in the set, plays its channel through the LR4
high branch at `crossover_hz`. The `LFE` role plays the LR4 low branch of the sum of the
stream's main channels, plus the stream's LFE channel at +10 dB, times the subwoofer level,
inverted if set. Every bonded endpoint has the room's whole stream, so each computes its own
feed." The subwoofer is the endpoint with the `LFE` role. The record's low branch is its
low-pass and the record's high branch is what every main in the set plays, so the two are one
crossover computed in two places from one setting. The three settings, all in
`crates/dsp/src/settings.rs` and all the room's, set from the app:

| Setting | Range | Default | What it does |
|---|---|---|---|
| `crossover_hz` | 40 to 200 Hz (`CROSSOVER_MIN_HZ`, `CROSSOVER_MAX_HZ`) | 80 Hz | the one frequency of both branches: the subwoofer's low-pass and every main's high-pass move together |
| the subwoofer level, `sub_level_cdb` | -12.00 to +6.00 dB in hundredths (`SUB_LEVEL_MIN_CDB`, `SUB_LEVEL_MAX_CDB`) | 0 | the gain of the subwoofer's feed |
| the polarity, `sub_polarity_inverted` | normal or inverted | normal | inverts the subwoofer's feed |

**With any room's mains.** The low-pass is the room's setting, not this box's: it follows
whatever mains the room has. At the default 80 Hz the mains' branch is 18.5 dB down at the
two-way's own 48.5 Hz (`decibels(linkwitz_riley(48000, 80).response_high(48.5))`) and the
subwoofer's branch is 15.7 dB down at 120 Hz and 24.6 dB down at 160 Hz (`.response_low(f)`),
far inside the driver's stated 24 to 2,000 Hz. At every setting the low branch is 24.6 dB down
an octave above the crossover (`linkwitz_riley(48000, 40).response_low(80)` and
`linkwitz_riley(48000, 200).response_low(400)`), so the range's top, 200 Hz, for small mains
that end near 150 Hz, is still a tenth of the driver's range. Small mains want the crossover
above their own roll-off and large ones may have it at 40 to 60 Hz; the box's own 33.5 Hz
stays under the lowest setting. A room without a bonded subwoofer plays its mains full range
(`sub_present` false), and this box is not in it.

The section coefficients at the default, each run twice in series, `b0 b1 b2 a1 a2`,
`a0 = 1`. The low branch (the subwoofer):

```text
2.7213807988313326e-05  5.442761597662665e-05  2.7213807988313326e-05  -1.9851906578962613  0.9852995131282146
```

The high branch (a main):

```text
0.9926225427561189  -1.9852450855122379  0.9926225427561189  -1.9851906578962613  0.9852995131282146
```

The record is the source of these numbers; the lines above are copied from it. At any other
`crossover_hz` the chain designs the same alignment itself (`Lr4Design::new`, which the test
holds to the record's sections to 1e-12).

What the crossover does not hold: the driver's and the mains' own responses, their distance
from the listener and the room. The filter is the electrical one; aligning the subwoofer to
the mains in a room is the polarity, the phase knob and the room correction.

## The controls (K69)

K69: "Pairing button, status LED, level + phase knobs (physical, alongside the app's
controls)". `docs/hardware/controls.md` gives the class "a pairing button; level and phase
knobs", a "status LED, follows the visualizer while playing" and no microphone. All four are
on the rear panel, beside the amplifier's plate. This file places none of them and chooses no
pin; they appear in the budget as allocations.

| Control | What it sets | Range | How it is read | The chorus DSP setting it maps onto |
|---|---|---|---|---|
| **Level knob** | a cut of this subwoofer's level, local to the box, beside the room's own level | -12.0 dB to 0.0 dB in 0.5 dB steps (25 positions): a cut, never a boost, "so no knob position can take the sub past the level the room's limit allows" (`firmware/include/chorus/controls.h`, K81, I10) | a potentiometer as a voltage divider on an ADC input, a code of 0 to 4095 (`CHORUS_CONTROLS_KNOB_MAX`); `chorus_controls_knob` turns the code into a position, with 24 codes of hysteresis at each step's edge (**ASSUMED**) so noise does not chatter; fully anticlockwise is the full cut | `sub_level_cdb`: `chorus_endpoint_dsp_settings` (`firmware/src/endpoint_dsp.c`) adds the knob's cut to the room's level from the wire and holds the sum to the setting's range, -1200 to 600 |
| **Phase knob** | the subwoofer's phase against the mains | 0 to 180 degrees in 15 degree steps (13 positions) | the same, a second potentiometer on a second ADC input | `sub_polarity_inverted`, and only that: at 90 degrees or more (`CHORUS_ENDPOINT_DSP_KNOB_INVERT_DEG`, **ASSUMED** midpoint) the feed is inverted, under it not. The knob's inversion and the room's polarity setting cancel when both are set |
| **Pairing button** | nothing in the DSP | a press | a contact, believed after 20 ms at one level (**ASSUMED** debounce) | none: "nothing yet: a local event" that the light shows (`docs/hardware/controls.md`; adoption is trust on first use, K92) |
| **Status light** | nothing: it shows state | one state at a time, by priority: fault, pairing, booting, link down, playing, idle; while playing it follows the visualizer's colour and level | driven by the endpoint, an RGB light | none |

**The gap between the phase knob and the DSP today (a follow-up, not implemented here).** The
knob has thirteen positions and the chain has two phases, normal and inverted: positions 0 to
75 degrees all play normal and 90 to 180 all play inverted, so only the knob's two ends are
exact. A continuous phase control (a variable all-pass section, or a delay, on the `LFE`
role's feed) does not exist in `crates/dsp/src/` or the firmware; adding one is a change to
the DSP and is named here as a follow-up
(`firmware/include/chorus/endpoint_dsp.h` says the same: "until a variable-phase all-pass is
designed (a follow-up)"). Until then the phase knob is a polarity switch with a wide throw.

**No gap for the level knob:** its whole range maps onto `sub_level_cdb`, and its cut and the
app's level add. The potentiometers want a linear taper, since the firmware divides the ADC's
range into equal steps (the budget's allocation is a linear one).

## Power and network (K90): mains, with Ethernet

- **Mains power, no PoE.** K90: "mains for two-way, sub, rack amp and soundbar (with
  Ethernet), since the Omada switch is 802.3af/at only". The subwoofer takes nothing from the
  switch's PoE budget.
- **The amplifier** takes the mains at its own plate (above): at most 100 W by its manual.
- **The endpoint** needs its own low-voltage supply, about 1.5 W (**ASSUMED**, as in the
  compact) at 5 V or more; the budget prices a 5 V adapter as an allocation. Which supply, and
  whether it shares the amplifier's inlet, is the electronics plan's.
- **Ethernet:** the endpoint board's W5500 wired port (below). Wi-Fi is not part of this class.

## The endpoint module, the amplifier's feed and the enclosure path

- **The endpoint module chorus's firmware targets (P1 approved at Checkpoint K, decisions 0042 and 0057; the board ASSUMED):** P1's
  Option B, `docs/proposals/P1-embedded-platform.md`: an ESP32-S3 with a TAS5825M and W5500
  wired Ethernet on one bought board (the Esparagus Audio Brick, ESP32-S3 variant: "Stereo I²S
  DAC (TAS5825M) with built-in D-Class amp", "W5500 SPI Ethernet", "Power Source 5-26 V", per
  its seller's page read 2026-10-06). The firmware's reference board profile is
  `firmware/boards/brick-s3-wired.conf`. If the board changes, this section and the
  budget change and `version` is bumped; the acoustics do not depend on the module.
- **The amplifier is a separate bought module fed from the endpoint, and why.** The module's
  own TAS5825M gives 53 W into this driver ("Amplifier sizing"), half of what the driver can
  use, so the 100 W comes from a bought mains amplifier and the endpoint's amplifier output is
  not used. The endpoint feeds it at line level.
- **The line-level feed is an open item for the board plan.** The module's maker says of the
  board family "you can't use headphones or an external amp" and points to its sibling boards
  with a line output (https://sonocotta.com/espragus-audio-brick/, read 2026-10-06). So the
  feed is one of: a line-level I2S DAC (a PCM5102 class breakout, priced in the budget) on the
  module's I2S bus, if the board plan finds the bus reachable; the maker's line-out sibling
  ("PCM5100A", "Non-amplified stereo output, 2.1V RMS", wired Ethernet as a soldered-on W5500
  header; $21.00, https://www.elecrow.com/hifi-esp32.html, read 2026-10-06), which is a
  different module than P1 recommends; or P1's own fallback, "a PCM5102A DAC plus an analog
  class-D board". Which one is the board plan's and, where it departs from P1, the owner's.
  The acoustics above hold for all three: they need 19.8 V RMS at the driver from a 100 W
  amplifier and the sections of this file run before it.
- **The enclosure path:** `docs/proposals/P12-enclosures.md` (ACCEPTED 2026-10-07 as written,
  decision 0242). Its recommendation: "subwoofer in braced 18 mm MDF with a doubled
  baffle", that is "18 mm MDF, a doubled (36 mm) baffle, window braces so that no unbraced
  span is over about 200 mm, painted; a printed port flare if the design is vented. Birch
  plywood is the alternative at about $14 more per box." P12 puts a cut on the owner's saw at
  ± 0.5 mm (**ASSUMED** there) and the net volume a wooden box holds at about ± 1%. Those are
  what the tolerances below are computed against.
- **Outdoors, bathrooms, the garage:** not this class. The driver states no weather rating and
  P12 rules MDF out there.

## The tolerances an enclosure must hold

Each row's effect is computed by re-running `vented_box(driver, vab_m3, fb_hz, ql=7)` at the
changed value and comparing `.f3_hz` and `.response_db(f)` from 25 to 200 Hz with the
design's; a changed vent or volume goes through
`port_tuning_hz(length_m=..., vab_m3=..., area_m2=...)` first.

| What | Value and tolerance | Effect at the edge of the tolerance |
|---|---|---|
| Net volume | 73.54 L ± 2% (± 1.47 L); P12 expects about ± 1% of a sawn box | at ± 2%: tuning 28.43 to 29.00 Hz, `f3` 32.5 to 33.0 Hz, response within 0.23 dB. At ± 1%: within 0.12 dB |
| Vent length | 326.6 mm ± 2.0 mm along the centre line | tuning 28.64 to 28.78 Hz, response within 0.03 dB |
| Vent inside diameter | 101.6 mm ± 0.5 mm (**ASSUMED** for a bought tube; measure it) | tuning 28.58 to 28.84 Hz, response within 0.05 dB |
| Volume ± 1% and vent length ± 1.0 mm together, worst case | | tuning 28.53 to 28.89 Hz, `f3` 32.7 to 32.9 Hz, response within 0.13 dB |
| Tuning, as measured on the built box | 28.7 Hz ± 3% (27.8 to 29.6 Hz); outside it the vent is trimmed | response within 0.35 dB (± 5% would be 0.61 dB) |
| Leakage | `ql` of 7 or more, shown by the built box's impedance curve (P12) | at `ql` 5: `f3` 34.4 Hz, up to 0.67 dB low; at `ql` 3: `f3` 38.4 Hz, 2.1 dB low. A tighter box only gains: `ql` 10 gives `f3` 31.6 Hz, `ql` 20 gives 30.3 Hz |
| Vent clearance | the inner end 102 mm or more from any wall or brace, the outer mouth unobstructed | not computed (**ASSUMED** practice) |
| Driver seat | on a gasket; the cut-out 272 mm, relieved behind the doubled 36 mm baffle so the wood does not shade the cone's rear (**ASSUMED** practice) | counted in the leakage figure |
| Amplifier plate | on a gasket in a 7 by 6 inch cut-out (the maker's figure); the plate is part of the box's wall | counted in the leakage figure |
| Walls | P12's construction: 18 mm MDF, a doubled baffle, no unbraced span over about 200 mm | not computed here (P12's calculation: first panel resonance near 950 Hz for a 200 x 200 mm braced panel, far above the band) |
| Openings | every opening into the cavity (the driver, the vent seat, the amplifier plate, the cable entries, the pairing button, the two knobs, the status light) sealed, or walled off from it | counted in the leakage figure |
| Fill | none | the alignment assumes none |
| The endpoint and its supply | outside the cavity's air or in a sealed pocket; nothing loose in a box that moves this much air | not computed |

## The budget (PROPOSED: under about $400 of parts per subwoofer, not labour)

K89 caps the compact only and says of the others "budgets proposed with priced alternatives".
This file **proposes** the subwoofer's: **under about $400 of parts at the designed (good)
tier**, with the better tier at about $620 and the best at about $815 as priced alternatives.
It is PROPOSED, not decided: the owner decides the class's budget and tier.

Single-unit list prices in US dollars before shipping and tax, each read from its URL on the
date in its row. The driver and the amplifier are the chosen parts. Every other line is a
**priced allocation**: the part that prices it is an example, and the board plan, the bill of
materials and the enclosure model choose the real ones. Where a part is sold only in a pack,
the line is this speaker's share of the pack and says so (the room list builds two
subwoofers).

| Line | What prices it | Price | URL, date read |
|---|---|---|---|
| Driver, good tier (designed) | Dayton Audio SD315A-88, one | 89.98 | https://www.parts-express.com/Dayton-Audio-SD315A-88-12-DVC-Subwoofer-295-488, 2026-10-06 |
| Amplifier, good tier (designed) | Dayton Audio SPA100-D plate amplifier, one | 109.98 | https://www.parts-express.com/Dayton-Audio-SPA100-D-100-Watt-Class-D-Subwoofer-Plate-Amplifier-300-805, 2026-10-06 |
| Endpoint board (allocation) | Esparagus Audio Brick, ESP32-S3 variant: the ESP32-S3 and the W5500 (its TAS5825M is not used). "As low as $59.00", "Availability: In stock", ship-from not stated; its US seller's page (https://www.crowdsupply.com/sonocotta/esparagus-audio-brick, "$59") said "No longer available" the same day | 59.00 | https://www.elecrow.com/esparagus-audio-brick.html, 2026-10-06 |
| Line-level feed (allocation) | a PCM5102 I2S DAC breakout with a line-level output, in stock | 4.95 | https://www.adafruit.com/product/6250, 2026-10-06 |
| Endpoint supply (allocation) | a 5 V 2.5 A switching wall supply, in stock | 8.25 | https://www.adafruit.com/product/1995, 2026-10-06 |
| Controls: level and phase knobs (allocation) | two panel-mount 10K linear potentiometers at $1.50, in stock | 3.00 | https://www.adafruit.com/product/3395, 2026-10-06 |
| Controls: knob caps (allocation) | two potentiometer knobs at $0.50, in stock | 1.00 | https://www.adafruit.com/product/2047, 2026-10-06 |
| Controls: pairing button (allocation) | a 16 mm panel-mount momentary push button, in stock | 0.95 | https://www.adafruit.com/product/1505, 2026-10-06 |
| Controls: status light (allocation) | one diffused RGB 10 mm LED: 1 of a 10 pack at $9.95 (22 in stock) | 1.00 | https://www.adafruit.com/product/848, 2026-10-06 |
| Enclosure: MDF (allocation) | one 4 x 8 ft sheet of 3/4 inch (18 mm) MDF, "$59.00 /Sheet", store pickup only | 59.00 | https://www.woodworkerssource.com/plywood-sheet-goods/mdf-34.html, 2026-10-06 |
| Enclosure: vent (allocation) | a 4 inch flared port tube kit, "Create a port up to 17" long", in stock (128) | 20.89 | https://www.parts-express.com/Precision-Port-4-Flared-Port-Tube-Kit-268-352, 2026-10-06 |
| Enclosure: gaskets (allocation) | closed-cell foam gasket tape: 6 ft of a 50 ft roll at $13.98 | 1.68 | https://www.parts-express.com/Speaker-Gasketing-Tape-1-8-x-3-8-x-50-ft.-Roll-260-540, 2026-10-06 |
| Enclosure: driver and plate screws (allocation) | #8 x 1 inch pan head screws: 13 of a 100 pack at $7.79 | 1.01 | https://www.parts-express.com/8-x-1-Deep-Thread-Pan-Head-Screws-Black-100-Pcs.-081-425, 2026-10-06 |
| **Total, good tier (designed)** | | **360.69** | |

Total: 360.69 USD at the designed (good) tier, under the proposed 400.00.

The tiers, with every line but the driver and the amplifier unchanged (160.73 USD of
allocations):

| Tier | Driver | Amplifier | Driver and amplifier | Total | URLs, date read |
|---|---|---|---|---|---|
| **good (designed)** | SD315A-88, 89.98 | SPA100-D, 109.98 | 199.96 | **360.69** | the two rows above, 2026-10-06 |
| better | RSS315HO-44, 249.98 | SPA250, 206.98 | 456.96 | 617.69 | https://www.parts-express.com/Dayton-Audio-RSS315HO-44-12-Reference-Series-HO-DVC-Subwoofer-295-467 and https://www.parts-express.com/Dayton-Audio-SPA250-250-Watt-Subwoofer-Amplifier-300-803, 2026-10-06 |
| best | RSS315HO-44, 249.98 | SPA500, 399.98 | 649.96 | 810.69 | the driver's URL in the row above and https://www.parts-express.com/Dayton-Audio-SPA500-500W-Subwoofer-Plate-Amplifier-300-807, 2026-10-06 |

What the total does and does not say:

- **The MDF is a whole sheet.** A box of this design, about 436 x 536 x 454 mm outside around
  an 80 L cavity (the 73.54 L net plus the driver, the vent, the amplifier and the braces;
  **ASSUMED** dimensions, arithmetic), has about 1.8 m² of panel with its second baffle and
  braces; a sheet is 2.97 m², so two boxes do not come out of one. P12 estimated about $26 of
  MDF for a 50 L box. The sheet priced is pickup only at its seller; the yard that supplies
  the owner's is the bill of materials' question.
- **The dearer tiers' other lines are not theirs.** Their box is smaller (40.78 L) and their
  vent is not a straight tube ("Driver"); their totals carry this tier's enclosure lines
  unchanged as a placeholder.
- **The fallback without the amplifier** (the endpoint's own TAS5825M as one channel, 53 W,
  with a 24 V supply in place of the 5 V one): the list less the amplifier, the DAC and the
  5 V supply and plus a 24 V 120 W desktop adapter ($57.27,
  https://www.trcelectronics.com/View/Mean-Well/GST120A24-P1M.shtml, in stock, read
  2026-10-06) is 294.78 USD, for 2.7 dB less output.
- **The cheaper module.** With the maker's line-out sibling ($21.00, above) in place of the
  endpoint board and the DAC, the list is 317.74 USD; its Ethernet is a soldered-on module
  that is not priced, and it is not P1's module.
- **Pack shares.** Bought as whole packs for a single subwoofer (the LED pack 9.95, the gasket
  roll 13.98, the screws 7.79) the same list is 388.72 USD.
- **Ship-from:** the driver, the amplifier, the vent kit, the gasket and the screws from
  Springboro, Ohio (above); the DAC, the supply, the potentiometers, the knobs, the button and
  the LED from Brooklyn, New York ("ALL ORDERS SHIP FROM THE ADAFRUIT FACTORY, BROOKLYN, NY,
  USA", https://www.adafruit.com/shipping, as read for `docs/hardware/compact-speaker.md` on
  2026-10-06); the MDF is collected in Arizona ("available for Arizona pickup only"); the
  endpoint board's in-stock seller states no ship-from and its maker is in Poland, so it is
  the one line that is **not shown to ship from the US**.
- **Not priced:** the mains cord, hookup and speaker wire, the RCA lead from the DAC to the
  amplifier, the Ethernet patch cable and its panel jack, wood glue, paint, feet, a grille,
  shipping and tax.

## The design record

`fixtures/design-record/chorus-sub-v1.json` is the bass-management crossover above as a
version 1 `speaker-design-record`, exported from release v1.50.0 on 2026-10-06 and committed
byte for byte, with `chorus-sub-v1.provenance` beside it (the release tag, the export command
with placeholders for the package's name, the date and the sha256
`7efc717e20999a509a2c5790601803249dd0462a750836cbf6f0363433b6e0ed`). Its response points are
at 20, 40, 80, 160 and 320 Hz. A version 1 record holds crossovers only: the box, the vent and
the high-pass sections live in this file until a later schema can carry them.

`crates/dsp/tests/design_record.rs`, `the_sub_record_is_the_exported_one_and_runs`, holds the
file to that sha256, reads it with chorus's own reader, checks that its frequency is the
chain's default `crossover_hz`, and runs its crossover through chorus's own `f32` filters: on
2026-10-06 the subwoofer's branch ran at -6.020352 dB and the mains' branch at -6.020924 dB at
80 Hz, and the sum at -0.000038 dB. `docs/dsp.md`, "Design records", describes the reader.

**What the test found at this low a crossover.** chorus's running filters hold their
coefficients in single precision, and at 80 Hz for 48 kHz that rounding is no longer
invisible: a section's gain at the bottom of the band is its numerator over `1 + a1 + a2`,
which is 1.09e-4 here, small beside the rounding of `a1` and `a2`. The rounded sections'
designed response is off the record's by up to 0.006324 dB (at 20 Hz, where the subwoofer's
branch runs at -0.039915 dB against the record's -0.033862 dB), against 0.000023 dB for the
two-way's 2000 Hz. The test now takes that rounding out before it holds the running filters
to the record at its 0.001 dB (they are within 0.000990 dB, the worst being the mains' branch
at 40 Hz, 24.6 dB down), and holds the rounding itself under a stated 0.02 dB. Six thousandths
of a decibel is far under anything audible or measurable in a room; it is written down because
the 0.001 dB the other records meet is not what the filters do here, and because double
precision coefficients for the bass-management sections would be a change to
`crates/dsp/src/` (a follow-up if a tighter figure is ever wanted).

## Open items

- **Everything is unmeasured.** The first box gives: the impedance curve (tuning, `ql`), the
  near-field response of the cone and of the vent, vent noise at full level, the cone's travel
  at 43 Hz and under the tuning, the amplifier's real output and its temperature in a closed
  box, and the level at the listening seat.
- **The driver's real parameters.** One datasheet line; unit-to-unit spread is not stated by
  the maker and is not modelled. The coils' wiring for the sheet's figures is **ASSUMED**
  parallel. Large-signal behaviour (the sheet's Xmax is one number, the coil heats, the
  inductance is not modelled) is what the first box shows.
- **The excursion and vent arithmetic is this file's,** built on the package's alignment and
  checked against its response; the package itself computes neither for a vented box.
- **The high-pass and the voltage limit run nowhere yet** (a follow-up in the chain or the
  firmware). Until they do, nothing protects the cone below the tuning but the amplifier's
  own response, which its manual does not state.
- **The phase knob is a polarity switch** until the DSP has a continuous phase control (a
  follow-up, "The controls").
- **The amplifier's own low-pass stays in the path at 200 Hz**; the alternative without one
  is a bare mains module. Its safety listing was not found in what was read.
- **The line-level feed from the endpoint** is the board plan's to settle, and may need the
  owner's word where it departs from P1.
- **The class's budget and tier are PROPOSED,** the owner's to decide.
- **P12 is ACCEPTED** (decision 0242): the enclosure's construction is its recommendation for
  this class; the cut and volume figures it gives are still **ASSUMED** until a box is measured
  (P1 is approved: Option B, decisions 0042 and 0057).
- **The better and best tiers** have a box from the package and no vent, high-pass or record
  of their own; their vent is beyond a straight tube and may be beyond the package.
- **The targets** (110.4 dB at 1 m, 35 Hz) are this design's own.

## Sources

Read 2026-10-06 unless a line says otherwise.

- The driver and the amplifiers: the seller's item service, the makers' pages, spec sheets and
  manuals, URLs in "Driver" and "Amplifier sizing"; the seller's shipping page,
  https://www.parts-express.com/shipping. The survey:
  `docs/research/subwoofer-drivers-and-amplifiers.md`.
- The acoustics package of the owner's shared Python library, release v1.50.0: its README
  (sealed and vented boxes, ports, the baffle step, biquads, crossovers, "The design record",
  "Known limits"). The package cites its own sources for every formula.
- TI, TAS5825M datasheet (SLASEH7H), https://www.ti.com/lit/ds/symlink/tas5825m.pdf, pages 1
  and 6.
- The endpoint boards: https://www.crowdsupply.com/sonocotta/esparagus-audio-brick,
  https://www.elecrow.com/esparagus-audio-brick.html, https://www.elecrow.com/hifi-esp32.html
  and the maker's product pages https://sonocotta.com/espragus-audio-brick/ and
  https://sonocotta.com/hifi-esp32-and-hifi-esp32s3/ (product pages only).
- The budget's other lines: the URLs in the table and under it. The Adafruit prices were read
  from the seller's product service (https://www.adafruit.com/api/product/<id>).
- In this repository: `docs/proposals/P1-embedded-platform.md`, `docs/proposals/P12-enclosures.md`,
  `docs/hardware/controls.md`, `docs/hardware/twoway-speaker.md`,
  `docs/hardware/compact-speaker.md`, `docs/dsp.md`, `crates/dsp/src/settings.rs`,
  `crates/dsp/src/crossover.rs`, `firmware/include/chorus/controls.h`,
  `firmware/include/chorus/endpoint_dsp.h`, `firmware/src/endpoint_dsp.c`,
  `firmware/src/controls.c`.
- No GPL source and no reciprocally licensed hardware design file was opened; no speaker
  design program's source was read.
