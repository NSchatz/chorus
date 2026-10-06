# Drivers and amplifiers for the subwoofer: a survey of 10 to 15 inch subwoofer drivers, plate amplifiers and amplifier modules at a US seller

Research for the subwoofer's acoustic design (`docs/hardware/subwoofer.md`), 2026-10-06; what
`docs/decisions/0229-the-subwoofer-driver-alignment-and-amplifier.md` rests on. Every number
was read on 2026-10-06 from the URL pattern named for it. Labels: NOT READ (no page read gives
it), LEAD (seen in a search snippet only; never used as a fact). Nothing below is a LEAD.

## What was asked

One subwoofer driver of 10 to 12 inches (an 8 and a 15 for context) for a mains-powered
subwoofer in a braced 18 mm MDF box (P12), and the amplifier that drives it: "an amp sized by
research" (the program's section on this class), a bought module and not a chip, fed by
chorus's endpoint. US seller, US ship-from (the US-first rule), single-unit list price. The
class has no decided parts budget (K89 caps the compact only: "Other classes: budgets
proposed with priced alternatives"), so the survey looks for three tiers of driver with
amplifier. The box is whatever the acoustics package designs for the driver: a sealed box, or
a vented one where the driver's Qts is at or under the package's B4 value (0.405 at a leakage
Q of 7).

## How it was read

The seller surveyed is Parts Express. Its product pages are drawn by script: an automated read
of `https://www.parts-express.com/<slug>` returns an empty page. The store's own item service
answers a plain request with the same data the page shows:

- `https://www.parts-express.com/api/items?country=US&currency=USD&language=en&fieldset=details&q=<seller's part or model>`
  (price, stock quantity, the next expected receipt date, and the specification fields; the
  fields carry no units, so every unit below is the maker's sheet's. An example: the
  SD315A-88's Vas is `5.36` in the service and "151.7 liters" on the maker's sheet).
- Dayton Audio's spec sheets and manuals:
  `https://www.daytonaudio.com/images/resources/<file>.pdf`, found from each product page on
  daytonaudio.com, text extracted.
- One GRS sheet from a copy a European seller hosts
  (https://doc.soundimports.nl/pdf/brands/GRS/10SW-4HE/spec-10SW-4HE.pdf). The Peerless
  sheets that could be read are 2002 to 2005 copies at another seller and disagree widely with
  the item service's values for the same part numbers, so no Peerless driver is carried
  further.
- Adafruit: `https://www.adafruit.com/api/product/<id>`.

## Drivers: price and stock

| Driver | Seller's part | Price | Stock | URL |
|---|---|---|---|---|
| GRS 10SW-4HE, 10 inch | 292-818 | $66.99 | 87 | https://www.parts-express.com/GRS-10SW-4HE-10-Paper-Cone-Rubber-Surround-High-Excursion-Subwoofer-4-Ohm-292-818 |
| Dayton Audio SD270A-88, 10 inch, dual coil | 295-486 | $69.98 | 0, expected 10/23/2026 | https://www.parts-express.com/Dayton-Audio-SD270A-88-10-DVC-Subwoofer-295-486 |
| Dayton Audio SD315A-88, 12 inch, dual coil | 295-488 | $89.98 | 26 | https://www.parts-express.com/Dayton-Audio-SD315A-88-12-DVC-Subwoofer-295-488 |
| Dayton Audio DCS255-4, 10 inch | 295-202 | $99.98 | 0, expected 12/18/2026 | https://www.parts-express.com/Dayton-Audio-DCS255-4-10-Classic-Subwoofer-4-Ohm-295-202 |
| Dayton Audio DCS305-4, 12 inch | 295-204 | $114.98 | 0, expected 10/9/2026 | https://www.parts-express.com/Dayton-Audio-DCS305-4-12-Classic-Subwoofer-4-Ohm-295-204 |
| Dayton Audio RSS210HF-4, 8 inch | 295-456 | $144.98 | 70 | https://www.parts-express.com/Dayton-Audio-RSS210HF-4-8-Reference-Series-HF-Subwoofer-4-Ohm-295-456 |
| Dayton Audio UMII10-22, 10 inch, dual coil | 295-710 | $152.99 | 184 | https://www.parts-express.com/Dayton-Audio-UMII10-22-ULTIMAX-II-10-600W-RMS-DVC-Subwoofer-2-Ohm-Per-Coil-295-710 |
| Dayton Audio UMII12-22, 12 inch, dual coil | 295-712 | $199.99 | 0, expected 11/20/2026 | https://www.parts-express.com/Dayton-Audio-UMII12-22-ULTIMAX-II-12-700W-RMS-DVC-Subwoofer-2-Ohm-Per-Coil-295-712 |
| Dayton Audio RSS265HF-4, 10 inch | 295-460 | $209.98 | 0, expected 11/13/2026 | https://www.parts-express.com/Dayton-Audio-RSS265HF-4-10-Reference-Series-HF-Subwoofer-4-Ohm-295-460 |
| Dayton Audio RSS315HO-4, 12 inch | 295-466 | $209.99 | 252 | https://www.parts-express.com/Dayton-Audio-RSS315HO-4-12-Reference-Series-HO-Subwoofer-4-Ohm-295-466 |
| Dayton Audio RSS265HO-4, 10 inch | 295-462 | $219.98 | 63 | https://www.parts-express.com/Dayton-Audio-RSS265HO-4-10-Reference-Series-HO-Subwoofer-4-Ohm-295-462 |
| Dayton Audio RSS315HF-4, 12 inch | 295-464 | $229.98 | 0, expected 10/16/2026 | https://www.parts-express.com/Dayton-Audio-RSS315HF-4-12-Reference-Series-HF-Subwoofer-4-Ohm-295-464 |
| Dayton Audio RSS265HO-44, 10 inch, dual coil | 295-463 | $239.98 | 22 | https://www.parts-express.com/Dayton-Audio-RSS265HO-44-10-Reference-Series-HO-DVC-Subwoofer-295-463 |
| Dayton Audio RSS315HO-44, 12 inch, dual coil | 295-467 | $249.98 | 37 | https://www.parts-express.com/Dayton-Audio-RSS315HO-44-12-Reference-Series-HO-DVC-Subwoofer-295-467 |
| Dayton Audio RSS390HF-4, 15 inch | 295-468 | $289.98 | 30 | https://www.parts-express.com/Dayton-Audio-RSS390HF-4-15-Reference-Series-HF-Subwoofer-4-Ohm-295-468 |

Also in the service, with the seller's values only (no maker's sheet could be read, so none is
designed with): GRS 10SW-4 ($35.99, 104 in stock, Qts 0.93), GRS 12SW-4 ($41.99, 136, Qts
0.82), GRS 12SW-4HE ($75.99, 362, Qts 0.43). The Dayton Audio UM10-22 reads "This item is no
longer available" and the UM12-22 is gone from the service; the UMII parts replace them.

## Drivers: the values printed on the maker's sheet

Each sheet is `https://www.daytonaudio.com/images/resources/<the file named in its row>` (the
GRS sheet's URL is above).

| Driver | Sheet | Impedance | Re, ohms | Fs, Hz | Qms | Qes | Qts | Vas, L | Sd, cm² | Xmax, mm | Vd, cm³ | BL, Tm | Cms, mm/N | Sensitivity | RMS watts |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 10SW-4HE | spec-10SW-4HE.pdf | 4 ohms | 3.8 | 25.2 | 4.00 | 0.57 | 0.50 | 60.7 | 346.4 | 11 | 379 | 10.8 | 0.36 | 87.5 dB @ 2.83V | 200 |
| SD270A-88 | 295-486-dayton-audio-sd270a-88-specifications-47085.pdf | 4 ohms | 3.16 | 26.2 | 3.23 | 0.50 | 0.43 | 107.5 | 346.4 | 6.0 | 207.8 | 7.80 | 0.64 | 87.8 dB @ 1W/1m | 80 |
| SD315A-88 | 295-488-dayton-audio-sd315a-88-specifications-47086.pdf | 4 ohms | 3.10 | 24.2 | 3.54 | 0.37 | 0.33 | 151.7 | 522.8 | 7.0 | 366.0 | 11.86 | 0.40 | 89.6 dB @ 1W/1m | 120 |
| DCS255-4 | 295-202-dayton-audio-dcs255-4-specifications-46583.pdf | 4 ohms | 3.45 | 37.7 | 7.42 | 0.50 | 0.46 | 32.1 | 346.4 | 8.3 | 287.5 | 12.6 | 0.19 | 91 dB @ 2.83V/1m | 200 |
| DCS305-4 | 295-204-dayton-audio-dcs305-4-specifications-46582.pdf | 4 ohms | 3.3 | 24.2 | 5.07 | 0.43 | 0.40 | 92.3 | 498.8 | 9.3 | 468.8 | 13.7 | 0.27 | 90.5 dB @ 2.83V/1m | 250 |
| RSS210HF-4 | 295-456-dayton-audio-rss210hf-4-specifications-46171.pdf | 4 ohms | 3.1 | 29.6 | 3.25 | 0.69 | 0.57 | 23.7 | 219.0 | 9.0 | 197.1 | 8.12 | 0.37 | 85.6 dB @ 2.83V/1m | 280 |
| UMII10-22 | 295-710--dayton-audio-UMII10-22-spec-sheet.pdf | 2 + 2 ohms, measured in series | 4 | 39 | 2.67 | 0.73 | 0.57 | 21.2 | 345 | 20 | 689 | 13.4 | 0.126 | 87.3 dB @ 2.83V/1m | 600 |
| UMII12-22 | 295-712--dayton-audio-UMII12-22-spec-sheet.pdf | 2 + 2 ohms, measured in series | 4.2 | 31 | 2.75 | 0.71 | 0.56 | 53 | 528 | 22 | 1162 | 14.8 | 0.133 | 88.6 dB @ 2.83V/1m | 800 |
| RSS265HF-4 | 295-460-dayton-audio-rss265hf-4-specifications-46172.pdf | 4 ohms | 3.5 | 25.6 | 3.06 | 0.53 | 0.45 | 54 | 356.3 | 12.3 | 438.3 | 11.43 | 0.32 | 87.8 dB @ 2.83V/1m | 350 |
| RSS315HO-4 | 295-466-dayton-audio-rss315ho-4-specifications-46175.pdf | 4 ohms | 3.2 | 26.2 | 3.63 | 0.33 | 0.31 | 53.7 | 514.7 | 12.3 | 633.1 | 20 | 0.15 | 90.5 dB @ 2.83V/1m | 700 |
| RSS265HO-4 | 295-462-dayton-audio-rss265ho-4-specifications-46173.pdf | 4 ohms | 3.5 | 26.9 | 4.02 | 0.39 | 0.35 | 29.4 | 349.7 | 12.3 | 430.1 | 17.6 | 0.17 | 87.2 dB @ 2.83V/1m | 600 |
| RSS315HF-4 | 295-464-dayton-audio-rss315hf-4-specifications-46174.pdf | 4 ohms | 3.1 | 24.2 | 2.83 | 0.45 | 0.39 | 84.1 | 514.7 | 14.3 | 736.0 | 13.99 | 0.23 | 90.3 dB @ 2.83V/1m | 400 |
| RSS265HO-44 | 295-463-dayton-audio-rss265-ho-44-specifications.pdf | 2 ohms (two 4 ohm coils) | 1.6 | 27.6 | 4.42 | 0.50 | 0.45 | 34.8 | 352.3 | 13.25 | 467.0 | 9.39 | 0.21 | 90.5 dB @ 2.83V/1m | 600 |
| RSS315HO-44 | 295-467--rss315ho-44-reference-ho-dvc-subwoofer-specifications.pdf | 4+4 ohms | 6.5 | 21.5 | 3.70 | 0.41 | 0.37 | 61.3 | 507.1 | 14.0 | 720.6 | 26.5 | 0.17 | 84.6 dB @ 2.83V/1m | 700 |
| RSS390HF-4 | 295-468-dayton-audio-rss390hf-4-specifications-46176.pdf | 4 ohms | 3 | 19.5 | 3.02 | 0.50 | 0.43 | 212 | 829.6 | 14.0 | 1,161.4 | 15 | 0.22 | 91.2 dB @ 2.83V/1m | 500 |

Notes on the sheets:

- **Dual coils.** The UMII sheets say "All specifications measured with voice coils wired in
  series". The SD and RSS-HO dual-coil sheets do not say. By their own numbers: the SD270A-88
  and SD315A-88 print "4 ohms" and a Re near 3.1 ohms for two 8 ohm coils, which fits
  parallel; the RSS265HO-44 prints "2 ohms" and Re 1.6 ohms for two 4 ohm coils, which fits
  parallel; the RSS315HO-44 prints "4+4 ohms" and Re 6.5 ohms, which fits series (and the item
  service lists its impedance as 8.0). Each is an inference, **ASSUMED** where the design uses
  it.
- **The SD315A-88's sensitivity** is 89.6 dB @ 1W/1m on the sheet and 93.75 in the item
  service: the same driver at 1 W and at 2.83 V (the design's arithmetic gives 89.7 and
  93.8 dB from the sheet's Thiele/Small values).
- **The maker's page for the SD270A-88 links the 8 inch SD215A-88's sheet;** the right one is
  at the file name in the table.
- **Box suggestions.** The maker's pages print one "optimum" sealed and one vented volume with
  an F3 and a footnote ("Enclosure volume/F3s based on BassBox "optimum" calculations"); no
  vent tuning is printed, and the seller's own fields give different volumes for the same
  drivers. None is used: the boxes below are the acoustics package's.
- **Dimensions** (the maker's pages): SD315A-88 314 mm outside, 272 mm cut-out, 130 mm deep;
  RSS315HO-44 314 mm, 282 mm, 146 mm. No sheet prints the volume of the driver's own bulk.

## The box the acoustics package designs for each

`vented_box_for_driver(driver, ql=7)` and `sealed_box_for_qtc(driver, 0.7071)` of release
v1.50.0, with `driver = Driver(fs_hz=, qes=, qms=, vas_m3=litres(), re_ohm=, sd_m2=, xmax_m=)`
from the row above; the sealed box's power is `displacement_limited_power_w(box, driver.vd_m3)`
divided by `driver.efficiency(AIR_SB)`. In stock on the day:

| Driver | `Driver.qts` | Vented | Sealed, Qtc 0.707 | Efficiency |
|---|---|---|---|---|
| 10SW-4HE | 0.499 | refused: above the B4 value 0.405, "needs a Chebyshev (C4) vented alignment", which the package does not compute | 60.2 L, -3 dB at 35.7 Hz, 62 W at the cone's limit | 0.165% |
| **SD315A-88** | 0.335 | **QB3, 73.54 L, 28.7 Hz, -3 dB at 32.8 Hz** | 43.9 L, -3 dB at 51.1 Hz, 70 W | 0.564% |
| RSS210HF-4 | 0.569 | refused (C4) | 43.6 L, -3 dB at 36.8 Hz, 36 W | 0.086% |
| UMII10-22 | 0.573 | refused (C4) | 40.7 L, -3 dB at 48.1 Hz, 660 W | 0.167% |
| RSS315HO-4 | 0.303 | QB3, 19.32 L, 34.2 Hz, -3 dB at 40.8 Hz | 12.0 L, -3 dB at 61.2 Hz, 859 W | 0.284% |
| RSS265HO-4 | 0.356 | QB3, 17.22 L, 30.2 Hz, -3 dB at 33.4 Hz | 9.9 L, -3 dB at 53.5 Hz, 461 W | 0.142% |
| RSS265HO-44 | 0.449 | refused (C4) | 23.6 L, -3 dB at 43.4 Hz, 237 W | 0.142% |
| **RSS315HO-44** | 0.369 | **QB3, 40.78 L, 23.3 Hz, -3 dB at 25.1 Hz** | 23.0 L, -3 dB at 41.2 Hz, 435 W | 0.144% |
| RSS390HF-4 | 0.429 | refused (C4) | 123.5 L, -3 dB at 32.1 Hz, 204 W | 0.305% |

The vent each vented box would need, by `port_length_m` at the box's tuning (the design file
gives the arithmetic for the volume a vent carries):

| Driver | Box | A 4 inch (101.6 mm) vent | A 3 inch (76.2 mm) vent |
|---|---|---|---|
| SD315A-88 | 73.54 L at 28.7 Hz | 326.6 mm | 169.8 mm |
| RSS315HO-4 | 19.32 L at 34.2 Hz | about 1.0 m | about 0.55 m |
| RSS265HO-4 | 17.22 L at 30.2 Hz | longer still | about 0.81 m |
| RSS315HO-44 | 40.78 L at 23.3 Hz | 1019 mm | about 0.56 m |

(The "about" figures were computed for 100 and 75 mm tubes: 970.5 and 532.2 mm, 787.1 mm,
and 541.0 mm.)

## Amplifiers

Prices and stock from the item service; the rated power is the maker's manual's line where a
manual was read (Dayton Audio), else the seller's description.

| Amplifier | Seller's part | Price | Stock | Rated power as printed | Supply | Inputs | Its own filters and controls | URL |
|---|---|---|---|---|---|---|---|---|
| Dayton Audio KAB-100Mv2 board | 325-512 | $52.98 | 103 | "Power output (w/ 24 VDC power supply): 1 x 100W @ 2 ohms" | needs 12 to 24 VDC | Bluetooth and a 3.5 mm analog input | volume | https://www.parts-express.com/Dayton-Audio-KAB-100Mv2-1-x-100W-Class-D-Audio-Amplifier-Board-with-aptX-HD-Bluetooth-5.0-325-512 |
| Sure AA-AB31241 board | 320-311 | $84.98 | 173 | "600W x 1 (2 ohms, THD 10%), 350W x 1 (2 ohms, THD 1%)" with 48 VDC | needs 25 to 52 VDC, 13.5 A | one line-level RCA | fixed gain steps; a fan | https://www.parts-express.com/Sure-AA-AB31241-1x600W-TAS5630-Class-D-Amplifier-Board-320-311 |
| **Dayton Audio SPA100-D plate** | 300-805 | **$109.98** | 228 | "Rated Power Output: 100 watts RMS into 4 ohms @ < 1.0% THD" | mains: "115/230 VAC, 60 Hz/50 Hz, 100W", IEC inlet | two line-level RCA | gain; low-pass "Variable 40 Hz – 200 Hz", defeated only by "turning the frequency control to maximum (200 Hz)"; phase "Switched 0° or 180°"; bass boost "Switchable, 6 dB @ 35 Hz"; auto or manual on | https://www.parts-express.com/Dayton-Audio-SPA100-D-100-Watt-Class-D-Subwoofer-Plate-Amplifier-300-805 |
| ICEpower 100AS1 module | 326-260 | $119.98 | 19 | "Power output: 1 x 100W RMS @ 4 ohms 1% THD+N, 1 kHz" | mains: "100~240 VAC 50/60 Hz" | analog; harness sold separately | none (a bare board) | https://www.parts-express.com/ICEpower-100AS1-Class-D-Amplifier-Module-with-Built-In-Power-Supply-1-x-100W-326-260 |
| ICEpower 200AS1 module | 326-271 | $129.98 | 26 | "Power output: 1 x 210W RMS @ 4 ohms 1% THD+N, 1 kHz" | mains: "100~240 VAC 50/60 Hz" | analog; harness sold separately | none | https://www.parts-express.com/ICEpower-200AS1-Class-D-Audio-Amplifier-with-Power-Supply-Module-326-271 |
| ICEpower 300AS1 module | 326-264 | $149.98 | 29 | "Wattage output: 1 x 300W RMS @ 4 ohms 1% THD+N, 20-20,000 Hz" | mains: "100~240 VAC 50/60 Hz" | analog; harness $25.99 | none | https://www.parts-express.com/ICEPower-300AS1-Class-D-Amplifier-Module-with-Built-In-Power-Supply-1-x-300W-326-264 |
| Dayton Audio SA100 plate | 300-802 | $147.98 | 0, expected 10/16/2026 | "75 watts RMS into 8 ohms @ 0.1% THD / 100 watts RMS into 4 ohms @ 0.2% THD" | mains | RCA and speaker-level | gain, low-pass, phase switch | https://www.parts-express.com/Dayton-Audio-SA100-100W-Subwoofer-Amplifier-300-802 |
| Dayton Audio SPA300-D plate | 300-806 | $179.98 | 461 | "Rated Power Output: 300 watts RMS into 4 ohms @ < 1.5% THD" | mains: "115/230 VAC, 60 Hz/50 Hz, 300W"; a USB 5 V 500 mA output | two line-level RCA | as the SPA100-D, boost at 30 Hz | https://www.parts-express.com/Dayton-Audio-SPA300-D-300-Watt-Class-D-Subwoofer-Plate-Amplifier-300-806 |
| Yung SD300-6 plate | 301-510 | $198.98 | 74 | "Measured power output: 300 watts RMS into 4 ohms @ < 1.0% THD" | mains | RCA and speaker-level | level, low-pass, a continuous phase knob, a fixed "Bass boost: 6 dB @ 30 Hz" with no defeat stated | https://www.parts-express.com/Yung-SD300-6-300W-Class-D-Subwoofer-Amp-Module-w-6dB30Hz-301-510 |
| **Dayton Audio SPA250 plate** | 300-803 | **$206.98** | 204 | "156 watts RMS into 8 ohms @ 0.1% THD / 252 watts RMS into 4 ohms @ < 1.0% THD" | mains: "115/230 VAC, 60 Hz/50 Hz, 400W" | RCA and speaker-level | gain; low-pass 40 to 180 Hz; phase switch; switchable boost | https://www.parts-express.com/Dayton-Audio-SPA250-250-Watt-Subwoofer-Amplifier-300-803 |
| Dayton Audio SPA250DSP plate | 300-8010 | $269.98 | 295 | the maker's page: "250 watts RMS into 4 ohms, THD+N <0.1%"; its manual: "THD+N Ratio 20-20kHz Filter, Rated Power @100Hz < 1%" | mains | RCA and XLR | all in its own DSP, each filter can be disabled; a subsonic filter 25 to 40 Hz | https://www.parts-express.com/Dayton-Audio-SPA250DSP-250W-Subwoofer-Plate-Amplifier-with-DSP-300-8010 |
| Hypex FusionAmp FA251 plate | 221-008 | $355.80 | 8 | "Output Power (4Ω): 1 x 250W", "Output Power (8Ω): 1 x 130W" | mains | high level, RCA, XLR | its own DSP | https://www.parts-express.com/Hypex-Direct-FusionAmp-FA251-Mono-Plate-Amplifier-250W-221-008 |
| Dayton Audio SPA500DSP plate | 300-8012 | $364.98 | 55 | the maker's page: "500 watts RMS into 4 ohms"; no 8 ohm figure | mains | RCA and XLR | as the SPA250DSP | https://www.parts-express.com/Dayton-Audio-SPA500DSP-500W-Subwoofer-Plate-Amplifier-with-DSP-300-8012 |
| **Dayton Audio SPA500 plate** | 300-807 | **$399.98** | 247 | "(0.92 % THD) 273 watts* into 8 ohms, 540 watts* into 4 ohms / *Based on one-third power duty cycle" | mains | RCA with an LFE input | gain; low-pass 30 to 200 Hz; phase switch; one parametric band | https://www.parts-express.com/Dayton-Audio-SPA500-500W-Subwoofer-Plate-Amplifier-300-807 |

The Dayton Audio manuals are `https://www.daytonaudio.com/images/resources/` followed by
`300-805-dayton-audio-spa100-d-user-manual.pdf`, `300-806--dayton-audio-spa300-d-user-manual.pdf`,
`300-803--dayton-audio-spa250-user-manual.pdf`, `300-807--dayton-audio-spa500-user-manual.pdf`
and `300-8010-dayton-audio-spa250dsp-user-manual_v3.0.pdf`. The SPA100-D's also gives
"Enclosure Cutout: 7" H x 6" W" and "Dimensions: 8-1/16" H x 7-1/16" W x 2-1/8" D".

No amplifier module with a digital (I2S) input was found in US stock at this seller, and no
manual or description read names a safety listing body.

## The endpoint and its feed to the amplifier

Product pages only.

| Board | What it has | Line-level output | Price, stock | URL |
|---|---|---|---|---|
| Esparagus Audio Brick (P1's module) | ESP32-S3, TAS5825M, "W5500 SPI Ethernet", "Power Source 5-26 V" | none: its maker says of the family "you can't use headphones or an external amp". One channel in bridged-parallel mode is stated: "Output (Bridge, 4Ω, 1% THD+N) ... 1x 53W" | "$59" and "No longer available" at its US seller; "As low as $59.00", "Availability: In stock" at another shop, ship-from not stated | https://www.crowdsupply.com/sonocotta/esparagus-audio-brick, https://www.elecrow.com/esparagus-audio-brick.html, https://sonocotta.com/espragus-audio-brick/ |
| The same maker's line-out board | ESP32 or ESP32-S3, "PCM5100A 32-bit Stereo DAC", "5V from USB-C"; Ethernet only as "a header that allows soldering in the W5500 SPI Ethernet module" | "Non-amplified stereo output, 2.1V RMS" | "As low as $21.00", "Availability: In stock" | https://www.elecrow.com/hifi-esp32.html, https://sonocotta.com/hifi-esp32-and-hifi-esp32s3/ |

TI's TAS5825M datasheet (SLASEH7H, https://www.ti.com/lit/ds/symlink/tas5825m.pdf), page 1:
"1 × 53 W, 1.0 Mode (4-Ω, 22 V, THD+N=1%)", "1 × 65 W, 1.0 Mode (4-Ω, 22 V, THD+N=10%)",
"2 × 30 W, 2.0 Mode (8-Ω, 24 V, THD+N=1%)"; page 6: minimum speaker load 3.2 ohms bridge-tied
and 1.6 ohms in the bridged-parallel mode. (The US seller's page for the board prints "1x 65 W
at 24 V at 4Ω, THD+N = 1%"; TI's 65 W is at 10% distortion.)

## Small parts priced for the budget

| Part | Price, stock | URL |
|---|---|---|
| PCM5102 I2S DAC breakout with a line-level output | $4.95, in stock | https://www.adafruit.com/product/6250 |
| 5 V 2.5 A switching wall supply | $8.25, in stock | https://www.adafruit.com/product/1995 |
| Panel-mount right-angle 10K linear potentiometer with a switch | $1.50, in stock (the plain panel-mount 10K, https://www.adafruit.com/product/562, read 0 in stock and does not state its taper; the seller of the drivers lists only an audio taper at 10K) | https://www.adafruit.com/product/3395 |
| Potentiometer knob | $0.50, in stock | https://www.adafruit.com/product/2047 |
| 16 mm panel-mount momentary push button | $0.95, in stock | https://www.adafruit.com/product/1505 |
| Diffused RGB 10 mm LED, 10 pack | $9.95, 22 in stock (the single diffused RGB LED, https://www.adafruit.com/product/159, read 0 in stock) | https://www.adafruit.com/product/848 |
| 4 inch flared port tube kit: "Create a port up to 17" long", "6-1/4" cutout" | $20.89, 128 in stock | https://www.parts-express.com/Precision-Port-4-Flared-Port-Tube-Kit-268-352 |
| Speaker gasket tape, 50 ft roll | $13.98, 232 in stock | https://www.parts-express.com/Speaker-Gasketing-Tape-1-8-x-3-8-x-50-ft.-Roll-260-540 |
| #8 x 1 inch pan head screws, 100 | $7.79, 165 in stock | https://www.parts-express.com/8-x-1-Deep-Thread-Pan-Head-Screws-Black-100-Pcs.-081-425 |
| 3/4 inch (18 mm) MDF, 4 x 8 ft sheet | "$59.00 /Sheet", "available for Arizona pickup only" | https://www.woodworkerssource.com/plywood-sheet-goods/mdf-34.html |
| Mean Well GST120A24-P1M, 24 V 5 A desktop adapter | $57.27, "In Stock: 1,492 can ship now" | https://www.trcelectronics.com/View/Mean-Well/GST120A24-P1M.shtml |

## What the survey says, for the decision

- **Two in-stock drivers have a vented box the package designs with real extension:** the
  SD315A-88 ($89.98: 73.54 L, -3 dB at 32.8 Hz) and the RSS315HO-44 ($249.98: 40.78 L, -3 dB
  at 25.1 Hz). The two other vented candidates need a vent of half a metre to a metre in a box
  under 20 L.
- **Sealed boxes of these drivers end between 32 and 61 Hz,** most of them above the room's
  two-ways (48.5 Hz), and the deep ones (the 15 inch, 123.5 L) are larger than the vented
  SD315A-88 for no more extension.
- **Only the SD315A-88's vent is a straight tube** (326.6 mm of 4 inch tube).
- **The SD315A-88 is four times as efficient as the RSS315HO-44** (0.564% against 0.144%) and
  takes 120 watts against 700: it wants an amplifier near 100 W, the dearer driver several
  hundred into its 8 ohms.
- **A 100 W mains plate amplifier is $109.98;** the endpoint board's own amplifier chip gives
  53 W into 4 ohms as one channel, by TI's datasheet.
- **The endpoint board P1 recommends has no line output,** so feeding a separate amplifier
  needs a DAC on its bus or its maker's line-out sibling.

## Not read

- Maker's sheets for the GRS 10SW-4, 12SW-4 and 12SW-4HE and for four Peerless drivers (the
  item service names none); the Peerless maker's own site (it refused the read).
- A second US seller's prices (Madisound, Meniscus, Solen); the seller surveyed lists no SB
  Acoustics driver and no 10 or 12 inch Tang Band subwoofer.
- The ICEpower modules' own data sheets (input wiring, safety approvals).
- A safety listing (UL, ETL) for any amplifier: not stated in what was read, which is not
  proof that there is none.
- Amplifier boards with a TAS3251 or an MA12070P in US stock.
- Ship-from for the endpoint board's in-stock seller.
- A 3/4 inch MDF sheet's price at the large US hardware chains (their sites refused an
  automated read); the one price read is a hardwood dealer's, pickup only.
- The inside diameter of the 4 inch port kit's tube (its page gives only the flares'
  diameters and the cut-out).

## What was read

All on 2026-10-06.

- The seller's item service, for every part number in the tables above and for the keyword
  searches that found them; the seller's shipping page (https://www.parts-express.com/shipping).
- The makers' pages on daytonaudio.com for the Dayton Audio drivers and amplifiers, and the
  spec sheets and manuals named above.
- https://doc.soundimports.nl/pdf/brands/GRS/10SW-4HE/spec-10SW-4HE.pdf.
- The endpoint boards' product pages (URLs in their table) and TI's TAS5825M datasheet, pages
  1, 6 and 9.
- https://www.adafruit.com/api/product/ for ids 159, 562, 848, 1505, 1995, 2047, 3395 and
  6250; https://www.woodworkerssource.com/plywood-sheet-goods/mdf-34.html;
  https://www.trcelectronics.com/View/Mean-Well/GST120A24-P1M.shtml.
- The README of the acoustics package of the owner's shared Python library at release v1.50.0
  (the calls used for the boxes and vents above).
- In this repository: `docs/proposals/P1-embedded-platform.md`,
  `docs/proposals/P12-enclosures.md`, `docs/hardware/twoway-speaker.md`,
  `docs/research/twoway-speaker-drivers.md` (the reading method).
- No GPL source and no reciprocally licensed hardware design file was opened; no speaker
  design program's source was read.
