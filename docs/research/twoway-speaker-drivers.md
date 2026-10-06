# Drivers for the active two-way: a survey of 5 to 7 inch woofers and dome tweeters at a US seller

Research for the active two-way's acoustic design (`docs/hardware/twoway-speaker.md`), 2026-10-06;
what `docs/decisions/0000-the-two-way-speaker-drivers-and-alignment.md` rests on. Every number
was read on 2026-10-06 from the URL pattern named for it. Labels: NOT READ (no page read gives
it), LEAD (seen in a search snippet only; never used as a fact). Nothing below is a LEAD.

## What was asked

One woofer of 5 to 7 inches and one dome tweeter of 3/4 to 1 1/8 inch for an active two-way in a
braced 18 mm birch plywood box (P12), vented, of roughly 8 to 20 litres, each driver on its own
channel of a TAS5825M at 24 V (minimum load 3.2 ohms in its 2.0 mode), crossed over digitally.
US seller, US ship-from (the US-first rule), single-unit list price. The class has no decided
parts budget (K89 caps the compact only: "Other classes: budgets proposed with priced
alternatives"), so the survey looks for three tiers of pair: good (about $40 to $60 the pair),
better (about $70 to $110) and best (about $130 to $220).

## How it was read

The seller surveyed is Parts Express. Its product pages are drawn by script: an automated read
of `https://www.parts-express.com/<slug>` returns an empty page. The store's own item service
answers a plain request with the same data the page shows:

- `https://www.parts-express.com/api/items?country=US&currency=USD&language=en&fieldset=details&q=<seller's part or model>`
  (price, stock quantity, and the specification fields; the fields carry no units, so every
  unit below is the maker's sheet's. An example: the DC160-8's Vas is `0.63` in the service and
  "17.9 liters" on the maker's sheet).
- Dayton Audio's spec sheets: `https://www.daytonaudio.com/images/resources/<file>.pdf`, found
  from each product page on daytonaudio.com, text extracted. The dimension drawings in them are
  images: a diameter, a cut-out or a depth of a Dayton part is the item service's (NOT READ on
  the sheet).
- Peerless (Tymphany) sheets: the seller-hosted copies,
  `https://www.parts-express.com/pedocs/specs/<part>--tymphany-<model>-spec-sheet.pdf` (the
  DA25TX00-08's is `264-1676--peerless-da25tx00-08-spec-sheet.pdf`). The maker's own site was
  not read.

Ship-from: the seller's shipping page (https://www.parts-express.com/shipping) carries the
footer "725 Pleasant Valley Dr." "Springboro, OH 45066 USA" and "Most orders ship same day when
received before 4PM ET." The page has no sentence that says where orders ship from; the footer
address is the one the compact's survey found named as the ship-from address in the store's
configuration (`docs/research/compact-speaker-drivers.md`).

## Woofers

All sold by Parts Express. Price and stock as the item service gave them:

| Model (seller's part) | Price | Stock | Product page slug |
|---|---|---|---|
| Dayton Audio DC160-8 (295-305) | $34.98 | in stock (323) | `Dayton-Audio-DC160-8-6-1-2-Classic-Woofer-295-305` |
| Dayton Audio DC160-4 (295-309) | $34.98 | in stock (85) | `Dayton-Audio-DC160-4-6-1-2-Classic-Woofer-Speaker-295-309` |
| Dayton Audio DA175-8 (295-335) | $39.98 | in stock (102) | `Dayton-Audio-DA175-8-7-Aluminum-Cone-Woofer-295-335` |
| Dayton Audio DS175-8 (295-428) | $49.98 | in stock (445) | `Dayton-Audio-DS175-8-6-1-2-Designer-Series-Woofer-295-428` |
| Dayton Audio DSA175-8 (295-528) | $54.98 | out of stock (0) | `Dayton-Audio-DSA175-8-6-1-2-Designer-Series-Aluminum-Cone-Woofer-295-528` |
| Dayton Audio RS150-4 (295-372) | $59.98 | in stock (36) | `Dayton-Audio-RS150-4-6-Reference-Woofer-4-Ohm-295-372` |
| Dayton Audio RS150-8 (295-354) | $59.98 | in stock (85) | `Dayton-Audio-RS150-8-6-Reference-Woofer-295-354` |
| Dayton Audio RS180-8 (295-355) | $79.98 | in stock (122) | `Dayton-Audio-RS180-8-7-Reference-Woofer-295-355` |
| Dayton Audio RS180-4 (295-374) | $79.98 | out of stock (0) | `Dayton-Audio-RS180-4-7-Reference-Woofer-4-Ohm-295-374` |
| Dayton Audio RS180P-8 (295-365) | $79.98 | in stock (16) | `Dayton-Audio-RS180P-8-7-Reference-Paper-Woofer-8-Ohm-295-365` |
| Dayton Audio RS180S-8 (295-364) | $79.98 | out of stock (0) | `Dayton-Audio-RS180S-8-7-Reference-Shielded-Woofer-8-Ohm-295-364` |
| Dayton Audio DA135-8 (295-330) | $27.98 | in stock (184) | `Dayton-Audio-DA135-8-5-1-4-Aluminum-Cone-Woofer-295-330` |
| Dayton Audio DC130B-4 (295-307) | $29.98 | in stock (16) | `Dayton-Audio-DC130B-4-5-1-4-Classic-Woofer-Speaker-295-307` |
| Dayton Audio DSA135-8 (295-526) | $39.98 | in stock (1339) | `Dayton-Audio-DSA135-8-5-Designer-Series-Aluminum-Cone-Woofer-295-526` |
| Dayton Audio RS125-8 (295-353) | $49.98 | in stock (127) | `Dayton-Audio-RS125-8-5-Reference-Woofer-295-353` |
| Dayton Audio RS125-4 (295-370) | $49.98 | in stock (103) | `Dayton-Audio-RS125-4-5-Reference-Woofer-4-Ohm-295-370` |
| Dayton Audio RS150P-4A (295-563) | $59.98 | in stock (64) | `Dayton-Audio-RS150P-4A-6-Reference-Paper-Woofer-4-Ohm-295-563` |
| Dayton Audio RS180P-4 (295-375) | $79.98 | in stock (165) | `Dayton-Audio-RS180P-4-7-Reference-Paper-Woofer-4-Ohm-295-375` |
| Dayton Audio SIG150-4 (295-652) | $42.98 | in stock (108) | `Dayton-Audio-SIG150-4-5.25-Signature-Series-Woofer-60W-Driver-4-Ohm` |
| Dayton Audio SIG180-4 (295-654) | $54.98 | in stock (111) | `Dayton-Audio-SIG180-4-6.5-Signature-Series-Woofer-80W-Driver-4-Ohm` |
| Dayton Audio CFC180-8 (295-609) | $32.98 | in stock (380) | `Dayton-Audio-CFC180-8-6-1-2-Carbon-Fiber-Woofer-8-Ohm-295-609` |
| Dayton Audio DS135-8 (295-426) | $39.98 | in stock (310) | `Dayton-Audio-DS135-8-5-Designer-Series-Woofer-295-426` |
| Peerless by Tymphany HDS-P830875 (264-1092) | $49.50 | in stock (8) | `Peerless-830875-6-1-2-Nomex-Cone-HDS-Woofer-264-1092` |
| Peerless by Tymphany SDS-P830656 (264-1078) | $16.50 | in stock (16) | `Peerless-830656-5-1-4-Paper-Cone-SDS-Woofer-264-1078` |
| Peerless by Tymphany HDS-P835025 (264-1086) | $49.50 | in stock (8) | `Peerless-835025-6-1-2-Aluminum-Cone-HDS-Woofer-264-1086` |
| Peerless by Tymphany HDS-P830874 (264-1090) | $45.00 | in stock (52) | `Peerless-830874-6-1-2-PPB-Cone-HDS-Woofer-264-1090` |
| Peerless by Tymphany HDS-P830860 (264-1080) | $33.75 | in stock (231) | `Peerless-830860-5-1-4-PPB-Cone-HDS-Woofer-264-1080` |
| Peerless by Tymphany SDS-160F25CP01-08 (264-1592) | $23.79 | in stock (142) | `Peerless-SDS-160F25CP01-08-6-1-2-Paper-Cone-Woofer-8-Ohm-264-1592` |
| Peerless by Tymphany HDS-P830883 (264-1094) | $56.25 | in stock (10) | `Peerless-830883-6-1-2-Nomex-Cone-HDS-Woofer-264-1094` |
| Peerless by Tymphany SLS-P830946 (264-1148) | $49.50 | in stock (18) | `Peerless-830946-6-1-2-Paper-Cone-Woofer-Speaker-4-Ohm-264-1148` |
| Peerless by Tymphany HDS-P830991 (264-1074) | $37.50 | in stock (19) | `Peerless-830991-5-1-4-GFC-Cone-HDS-Woofer-264-1074` |
| HiVi M6N (297-441) | $27.98 | in stock (51) | `HiVi-M6N-6-Aluminum-Magnesium-Midbass-297-441` |
| Wavecor WF146WA02 (298-1166) | $77.00 | in stock (1) | `Wavecor-WF146WA02-5-3-4-Paper-Cone-Mid-Woofer-8-Ohm-298-1166` |
| Wavecor WF146WA01 (298-1164) | $77.00 | in stock (12) | `Wavecor-WF146WA01-5-3-4-Paper-Cone-Mid-Woofer-4-Ohm-298-1164` |
| Wavecor WF146WA06 (298-1170) | $71.00 | in stock (5) | `Wavecor-WF146WA06-5-3-4-Glass-Fiber-Cone-Mid-Woofer-8-Ohm-298-1170` |
| Wavecor WF146WA05 (298-1168) | $71.00 | in stock (15) | `Wavecor-WF146WA05-5-3-4-Glass-Fiber-Cone-Mid-Woofer-4-Ohm-298-1168` |
| Wavecor WF168WA02 (298-1190) | $81.00 | in stock (8) | `Wavecor-WF168WA02-6-1-2-Paper-Cone-Mid-Woofer-8-Ohm-298-1190` |
| Wavecor WF168WA06 (298-1194) | $77.00 | in stock (10) | `Wavecor-WF168WA06-6-1-2-Glass-Fiber-Cone-Mid-Woofer-8-Ohm-298-1194` |
| Wavecor WF152BD14 (298-1232) | $131.00 | in stock (10) | `Wavecor-WF152BD14-6-Black-Coated-Paper-Glass-Fiber-Cone-Midwoofer-8-Ohm-298-1232` |
| Wavecor WF152BD13 (298-1231) | $131.00 | in stock (2) | `Wavecor-WF152BD13-6-Black-Coated-Paper-Glass-Fiber-Cone-Midwoofer-4-Ohm-298-1231` |
| Wavecor WF152BD10 (298-1230) | $147.00 | in stock (10) | `Wavecor-WF152BD10-6-Kevlar-Carbon-Fiber-Cone-Midwoofer-8-Ohm-298-1230` |
| Wavecor WF152BD09 (298-1229) | $147.00 | in stock (10) | `Wavecor-WF152BD09-6-Kevlar-Carbon-Fiber-Cone-Midwoofer-4-Ohm-298-1229` |
| Wavecor WF182BD10 (298-1206) | $158.00 | in stock (7) | `Wavecor-WF182BD10-7-Balanced-Drive-Paper-Glass-Fiber-Cone-Mid-Woofer-8-Ohm-298-1206` |
| Morel CAW 538 (297-082) | $189.00 | in stock (5) | `Morel-CAW538-5-Cast-Frame-Woofer-297-082` |
| Morel CAW 638 (297-084) | $203.70 | in stock (7) | `Morel-CAW638-6-Cast-Frame-Woofer-297-084` |

Dayton Audio, as printed on the maker's sheets:

| Model | Nominal | Re | Fs | Qms | Qes | Qts | Vas | Sd | Xmax | Sensitivity | RMS power | Usable range |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| DC160-8 | 8 ohms | 6.6 ohms | 35.7 Hz | 3.46 | 0.38 | 0.34 | 17.9 liters | 134.8 cm² | 3.15 mm | 86.1 dB @ 2.83V/1m | 50 watts | 30 - 4,000 Hz |
| DC160-4 | 4 ohms | 3.4 ohms | 39 Hz | 3.31 | 0.49 | 0.43 | 15 liters | 136.8 cm² | 3.4 mm | 88.3 dB @ 2.83V/1m | 60 watts | 30 - 4,000 Hz |
| DA175-8 | 8 ohms | 5.9 ohms | 39.0 Hz | 3.31 | 0.70 | 0.58 | 16.3 liters | 132.7 cm² | 4.25 mm | 85 dB @ 1W/1m | 50 watts | 35 - 10,000 Hz |
| DS175-8 | 8 ohms | 5.7 ohms | 37 Hz | 1.63 | 0.32 | 0.27 | 17 liters | 128.7 cm² | 5.25 mm | 87.7 dB @ 2.83V/1m | 70 watts | 40 - 4,500 Hz |
| DSA175-8 | 8 ohms | 5.8 ohms | 38.36 Hz | 1.66 | 0.35 | 0.29 | 18.7 liters | 128.7 cm² | 5.3 mm | 88.1 dB @ 2.83V/1m | 70 watts | 38 - 7,500 Hz |
| RS150-4 | 4 ohms | 3.1 ohms | 45.1 Hz | 1.96 | 0.40 | 0.33 | 16.4 liters | 85 cm² | 4.4 mm | 91.8 dB @ 2.83V/1m | 40 watts | 48 - 4,000 Hz |
| RS150-8 | 8 ohms | 6.3 ohms | 47.8 Hz | 1.66 | 0.43 | 0.34 | 16.6 liters | 85 cm² | 4.0 mm | 88.7 dB @ 2.83V/1m | 40 watts | 47 - 4,200 Hz |
| RS180-8 | 8 ohms | 6.4 ohms | 35.7 Hz | 1.22 | 0.42 | 0.31 | 24.4 liters | 124.7 cm² | 6.0 mm | 87.1 dB @ 2.83V/1m | 60 watts | 39 - 3,600 Hz |
| RS180-4 | 4 ohms | 3.1 ohms | 38.4 Hz | 2.13 | 0.59 | 0.46 | 21.2 liters | 124.7 cm² | 6.0 mm | 89.2 dB @ 2.83V/1m | 60 watts | 39 - 3,600 Hz |
| RS180P-8 | 8 ohms | 6.3 ohms | 45.8 Hz | 1.49 | 0.44 | 0.34 | 22 liters | 126.7 cm² | 6 mm | 89.8 dB @ 2.83V/1m | 60 watts | 45 - 8,000 Hz |
| RS180S-8 | 8 ohms | 6.4 ohms | 37.7 Hz | 1.45 | 0.49 | 0.36 | 23 liters | 124.7 cm² | 6.0 mm | 86.9 dB @ 2.83V/1m | 60 watts | 41 - 3,600 Hz |
| DA135-8 | 8 ohms | 6.4 ohms | 56.5 Hz | 2.61 | 0.68 | 0.54 | 7.4 liters | 75.4 cm² | 3.0 mm | 85 dB @ 1W/1m | 30 watts | 50 - 15,000 Hz |
| DC130B-4 | 4 ohms | 3.2 ohms | 51.8 Hz | 2.52 | 0.41 | 0.35 | 12.3 liters | 91.6 cm² | 2.5 mm | 92.1 dB @ 2.83V/1m | 40 watts | 50 - 5,000 Hz |
| DSA135-8 | 8 ohms | 5.9 ohms | 51.81 Hz | 2.04 | 0.47 | 0.38 | 7.93 liters | 75.4 cm² | 4.9 mm | 87 dB @ 2.83V/1m | 50 watts | 51 - 9,000 Hz |
| RS125-8 | 8 ohms | 6.2 ohms | 59.2 Hz | 1.74 | 0.42 | 0.34 | 4.77 liters | 52.8 cm² | 4.0 mm | 86.8 dB @ 2.83V/1m | 30 watts | 65 - 5,400 Hz |
| RS125-4 | 4 ohms | 2.9 ohms | 57.2 Hz | 2.04 | 0.45 | 0.37 | 5.29 liters | 52.8 cm² | 4.0 mm | 89.9 dB @ 2.83V/1m | 30 watts | 65 - 5,400 Hz |
| RS180P-4 | 4 ohms | 3.1 ohms | 37.7 Hz | 1.98 | 0.48 | 0.39 | 27.3 liters | 126.7 cm² | 6 mm | 90.8 dB @ 2.83V/1m | 60 watts | 35 - 8,000 Hz |
| SIG150-4 | 4 ohms | 3.7 ohms | 61.5 Hz | 5.78 | 0.53 | 0.49 | 8.7 liters | 96 cm² | 4 mm* (*Xmax @ 82% BL) | 91.1 dB @ 2.83V/1m | 60 watts (AES 426B) | 60 - 4,000 Hz |
| SIG180-4 | 4 ohms | 3.7 ohms | 40.5 Hz | 5.19 | 0.41 | 0.38 | 23.2 liters | 143 cm² | 5 mm* (*Xmax @ 82% BL) | 91.0 dB @ 2.83V/1m | 80 watts (AES 426B) | 40 - 4,000 Hz |
| CFC180-8 | 8 ohms | 8.0 ohms | 51 Hz | 3.64 | 0.59 | 0.51 | 12.44 liters | 129.8 cm² | 4 mm | 86.6 dB@2.83V/1m | 40 watts | 45 - 6,000 Hz |
| DS135-8 | 8 ohms | 5.9 ohms | 52.5 Hz | 2.02 | 0.48 | 0.39 | 7.27 liters | 75.4 cm² | 4.85 mm | 86.6 dB @ 2.83V/1m | 50 watts | 50 - 7,000 Hz |

The DC160-8's other printed values: Le 2.26 mH @ 1 kHz, Mms 29.3g, Cms 0.68 mm/N, BL 10.7 Tm,
Vd 42.5 cm³, voice coil 35 mm; "Smooth frequency response up to 2 kHz and great bass response";
"Low 34 Hz free-air resonance" (the table prints 35.7 Hz). In the item service (no units): overall
outside diameter 6.5, baffle cut-out 5.69, depth 3.24, bolt circle 6.16, four mounting holes,
cone "Treated Paper", surround "Rubber", frame "Steel". The RS180-8's: Le 0.73 mH @ 1 kHz,
Mms 17.9g, Cms 1.12 mm/N, BL 7.82 Tm, Vd 74.8 cm³; in the service overall diameter 7.11,
cut-out 5.75, depth 3.19, six holes, cone "Aluminum", frame "Cast Aluminum". The RS150P-4A's
sheet has no extractable text (NOT READ).

Peerless by Tymphany, as printed on the seller-hosted maker's sheets:

| Model | Sheet date | Re | Zmin | Fs | Qms | Qes | Qts | Vas | Sd | Xmax | Sensitivity at 2.83 V | Rated noise power | Test bandwidth |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| HDS-P830875 | Last Update 2017-04-25 | 6.37 Ohms | 7.18 Ohms | 45.82 Hz | 3.19 | 0.41 | 0.36 | 21.78 L | 143.1 cm^2 | 5.3 mm | 89.04 dB +/- 1.0db | 75 W | 40Hz - 3kHz |
| HDS-P835025 | Last Update 2017-04-25 | 6.34 Ohms | 7.03 Ohms | 38.24 Hz | 3.32 | 0.42 | 0.37 | 24.76 L | 143.1 cm^2 | 5.4 mm | 86.64 dB +/- 1.0db | 55 W | 20Hz - 3kHz |
| HDS-P830874 | Last Update 2017-03-03 | 6.5 Ohms | 7.32 Ohms | 58 Hz | 2.29 | 0.53 | 0.43 | 12.5 L | 143.1 cm^2 | 5.3 mm | 88.44 dB +/- 1.0db | 75 W | 50Hz - 4kHz |
| HDS-P830883 | Last Update 2017-03-03 | 5.85 Ohms | 6.67 Ohms | 42.85 Hz | 2.18 | 0.46 | 0.38 | 22.9 L | 143.1 cm^2 | 5.6 mm | 87.46 dB +/- 1.0db | 70 W | 40Hz - 3kHz |
| HDS-P830860 | Last Update 2017-03-03 | 5.9 Ohms | 6.35 Ohms | 61.6 Hz | 2.21 | 0.59 | 0.47 | 8.41 L | 89.9 cm^2 | 3.5 mm | 87.5 dB +/- 1.0db | 50 W | 60Hz - 5kHz |
| HDS-P830991 | Last Update 2017-04-25 | 5.99 Ohms | 6.65 Ohms | 65.94 Hz | 2.96 | 0.58 | 0.49 | 6.81 L | 89.9 cm^2 | 4.6 mm | 87.06 dB +/- 1.0db | 30 W | 20Hz - 5kHz |
| SDS-P830656 | Printed: July 2025 | 5.9 Ω | 6.5 Ω | 65 Hz | 3.47 | 0.76 | 0.62 | 6.58 L | 86.6 cm2 | 5.46 mm (Xmech 10 mm) | 86.1 dB | 60 W (IEC 268-5) | 50 - 5000 Hz |
| SDS-160F25CP01-08 | Last Update 2017-03-03 | 6.05 Ohms | 6.44 Ohms | 44.75 Hz | 2.74 | 0.55 | 0.46 | 30.77 L | 141 cm^2 | 3.5 mm | 89.46 dB +/- 1.0db | 60 W | 40Hz - 4kHz |
| SLS-P830946 | Last Update 2017-04-25 | 2.73 Ohms | 3.64 Ohms | 42.18 Hz | 4.92 | 0.38 | 0.35 | 11.2 L | 123.5 cm^2 | 8.2 mm | 86.7 dB +/- 1.0db | 75 W | 20Hz - 1kHz |

Wavecor, Morel and HiVi woofers are in the price table only: their makers' data was read in
part (Wavecor's web tables, whose two-number "before burn-in, after burn-in" rows did not map
to model columns with certainty; Morel's sheets for the CAW 538 and CAW 638; nothing for HiVi),
and none is used below, so their parameters are not repeated here (NOT READ for this note's
purposes).

Differences between the maker's sheet and the item service:

- Dayton: DC160-8 Xmax `3.2` in the service against "3.15 mm"; DC130B-4 Fs `50.5` against
  "51.8 Hz"; RS180P-4 BL `5.03` against "4.67 Tm". The DA135-8 and DA175-8 sheets give
  sensitivity "@ 1W/1m", every other Dayton woofer sheet "@ 2.83V/1m".
- Peerless: the service is unreliable. HDS-P835025: Fs `43.0`, Qts `0.42`, Xmax `10.0` in the
  service against 38.24 Hz, 0.37 and 5.4 mm on the sheet. HDS-P830875: the service's Re `7.18`
  is the sheet's Zmin. SDS-P830656: Fs `57.0` against 65 Hz. HDS-P830874, HDS-P830860,
  HDS-P830883: power `50.0`, `30.0`, `30.0` against 75 W, 50 W, 70 W.
- Four ohm parts whose printed Re is below the amplifier's 3.2 ohm minimum load: RS125-4 (2.9),
  RS150-4, RS180-4 and RS180P-4 (3.1), Peerless SLS-P830946 (2.73, Zmin 3.64). Dayton's sheets
  print no minimum impedance.

No woofer's page or sheet has any wording on outdoor, humid, moisture or weather use.

Not sold by this seller on the day (the item service returned no item): SB Acoustics, SEAS and
Scan-Speak parts, the Dayton ES180Ti-8.

## Tweeters

All sold by Parts Express:

| Model (seller's part) | Price | Stock | Product page slug |
|---|---|---|---|
| Dayton Audio DC28F-8 (275-070) | $24.98 | in stock (145) | `Dayton-Audio-DC28F-8-1-1-8-Silk-Dome-Tweeter-275-070` |
| Dayton Audio DC28FS-8 (275-075) | $28.79 | in stock (147) | `Dayton-Audio-DC28FS-8-1-1-8-Silk-Dome-Shielded-Tweeter-275-075` |
| Dayton Audio DC25T-8 (275-045) | $17.98 | in stock (766) | `Dayton-Audio-DC25T-8-1-Titanium-Dome-Tweeter-275-045` |
| Dayton Audio ND25FA-4 (275-059) | $11.49 | in stock (407) | `Dayton-Audio-ND25FA-4-1-Soft-Dome-Neodymium-Tweeter-275-059` |
| Dayton Audio ND28F-6 (275-040) | $15.98 | in stock (418) | `Dayton-Audio-ND28F-6-1-1-8-Soft-Dome-Neodymium-Tweeter-275-040` |
| Dayton Audio RST28F-4 (275-141) | $39.98 | in stock (9) | `Dayton-Audio-RST28F-4-1-1-8-Reference-Series-Fabric-Dome-Tweeter-4-Ohm-275-141` |
| Dayton Audio RST28A-4 (275-131) | $39.98 | out of stock (0) | `Dayton-Audio-RST28A-4-1-1-8-Reference-Series-Aluminum-Dome-Tweeter-4-Ohm-275-131` |
| Dayton Audio AMT Mini-8 (275-095) | $23.98 | in stock (300) | `Dayton-Audio-AMT-Mini-8-Air-Motion-Transformer-Tweeter-275-095` |
| Peerless by Tymphany DX25BG60-04 (264-1478) | $34.50 | in stock (80) | `Peerless-DX25BG60-04-1-Silk-Dome-Tweeter-4-Ohm-264-1478` |
| Peerless by Tymphany DA25TX00-08 (264-1676) | $56.25 | in stock (18) | `Peerless-DA25TX00-08-1-Corundum-Dome-Tweeter-264-1676` |
| Peerless by Tymphany D27TG35-06 (264-1022) | $28.50 | in stock (157) | `Peerless-D27TG-35-06-1-Silk-Dome-Tweeter-264-1022` |
| Peerless by Tymphany XT25TG30-04 (264-1016) | $28.50 | in stock (78) | `Peerless-XT25TG30-04-1-Dual-Ring-Radiator-Tweeter-264-1016` |
| Peerless by Tymphany XT25SC90-04 (264-1014) | $18.00 | in stock (156) | `Peerless-XT25SC90-04-1-Dual-Ring-Radiator-Tweeter-264-1014` |
| Peerless by Tymphany XT25BG60-04 (264-1012) | $41.98 | in stock (6) | `Peerless-XT25BG60-04-1-Dual-Ring-Radiator-Tweeter-264-1012` |
| Peerless by Tymphany NE25VTS-04 (264-1034) | $42.00 | in stock (67) | `Peerless-NE25VTS-04-1-Silk-Dome-Tweeter-264-1034` |
| Peerless by Tymphany DX25TG59-04 (264-1020) | $19.50 | in stock (149) | `Peerless-DX25TG59-04-1-Fabric-Dome-Tweeter-264-1020` |
| HiVi RT1.3WE (297-421) | $48.98 | in stock (109) | `HiVi-RT1.3WE-Isodynamic-Tweeter-297-421` |
| HiVi Q2R (297-418) | $22.98 | in stock (58) | `HiVi-Q2R-1-1-8-Textile-Dome-Tweeter-297-418` |
| Morel MDT 29 (277-010) | $66.00 | in stock (64) | `Morel-MDT-29-1-1-8-Soft-Dome-Tweeter-277-010` |
| Morel MDT 12 (277-060) | $52.50 | in stock (41) | `Morel-MDT-12-1-1-8-Neodymium-Tweeter-277-060` |
| Morel CAT 298 (277-080) | $84.00 | in stock (7) | `Morel-CAT-298-1-1-8-Soft-Dome-Tweeter-277-080` |
| Morel MDT 39 (277-035) | $62.00 | in stock (12) | `Morel-MDT-39-1-1-8-Compact-Dome-Tweeter-277-035` |
| Wavecor TW030WA07 (298-1112) | $77.00 | in stock (10) | `Wavecor-TW030WA07-30mm-Textile-Dome-Tweeter-with-Rear-Chamber-8-Ohm-298-1112` |
| Wavecor TW030WA06 (298-1110) | $79.00 | in stock (6) | `Wavecor-TW030WA06-30mm-Textile-Dome-Tweeter-with-Rear-Chamber-with-Ferrofluid-4-Ohm-298-1110` |
| Wavecor TW030WA21 (298-1219) | $77.00 | in stock (36) | `Wavecor-TW030WA21-30mm-Graphene-Dome-Tweeter-4-Ohm-298-1219` |
| Wavecor TW030WA22 (298-1220) | $77.00 | in stock (26) | `Wavecor-TW030WA22-30mm-Graphene-Dome-Tweeter-8-Ohm-298-1220` |
| Wavecor TW022WA09 (298-1217) | $74.00 | in stock (42) | `Wavecor-TW022WA09-22mm-Chambered-Neo-Textile-Tweeter-4-Ohm-298-1217` |
| Wavecor TW022WA10 (298-1218) | $74.00 | in stock (44) | `Wavecor-TW022WA09-22mm-Chambered-Neo-Textile-Tweeter-Pair-4-Ohm-298-1218` |
| EPIQUE by Dayton Audio EC30-4 (275-500) | $69.98 | in stock (220) | `Epique-High-Resolution-Tweeter-Ceramic-Dome-30mm-4-Ohm-EC30-4-275-500` |
| Dayton Audio TD25F-4 (275-022) | $19.98 | in stock (548) | `Dayton-Audio-TD25F-4-1-Soft-Dome-Tweeter-4-Ohm-275-022` |
| Wavecor TW030WA02 (298-2106) | $76.00 | in stock (29) | `Wavecor-TW030WA02-30-mm-Textile-Dome-Neodymium-Tweeter-with-Heat-Sink-and-Ferrofluid-298-2106` |

Dayton Audio, as printed on the maker's sheets:

| Model | Nominal | Re | Fs | Qts | Sensitivity | RMS power | Usable range |
|---|---|---|---|---|---|---|---|
| DC28F-8 | 8 ohms | 5.4 ohms | 834 Hz | 0.50 | 89 dB @ 1W/1m | 50 watts | 1,300 - 20,000 Hz |
| DC28FS-8 | 8 ohms | 5.4 ohms | 905 Hz | 0.97 | 89 dB @ 1W/1m | 50 watts | 1,600 - 20,000 Hz |
| DC25T-8 | 8 ohms | 7.7 ohms | 1468 Hz | 0.47 | 93 dB @ 1W/1m | 50 watts | 3,000 - 20,000 Hz |
| ND25FA-4 | 4 ohms | 3.2 ohms | 1,350 Hz | 1.56 | 90 dB @ 2.83V/1m | 20 watts | 2,500 - 20,000 Hz |
| ND28F-6 | 6 ohms | 5.4 ohms | 1,097 Hz | 1.24 | 88.4 dB @ 1W/1m | 30 watts | 2,200 - 20,000 Hz |
| RST28F-4 | 4 ohms | 3.0 ohms | 710 Hz | 0.92 | 93.5 dB @ 2.83V/1m | 80 watts | 1,400 - 20,000 Hz |
| RST28A-4 | 4 ohms | 3.0 ohms | 775 Hz | 0.93 | 92.5 dB @ 2.83V/1m | 80 watts | 1,500 - 20,000 Hz |
| AMT Mini-8 | 8 ohms | 7 ohms | N/A | N/A | 89 dB @ 2.83W/1m | 15 watts | 4,000 - 40,000 Hz |
| TD25F-4 | 4 ohms | 3.5 ohms | 900 Hz | 0.94 | 91 @ 2.83V/1m | 20 watts | 2,000 - 20,000 Hz |

The DC28F-8's other printed values: Le 0.09 mH @ 10 kHz, Qms 0.81, Qes 1.33, Sd 6.6 cm², voice
coil 29 mm; "Treated silk dome", "Damped rear chamber lowers resonance", "Ferrofluid cooled voice
coil". In the item service (no units): overall outside diameter 4.33, cut-out 2.91, depth 1.53.
The AMT Mini-8 is an air motion transformer, not a dome; its sheet prints "89 dB @ 2.83W/1m"
as written here.

Peerless by Tymphany, as printed on the seller-hosted maker's sheets:

| Model | Sheet date | Re | Zmin | Fs | Qts | Sensitivity at 2.83 V | Rated noise power | Test bandwidth |
|---|---|---|---|---|---|---|---|---|
| DX25BG60-04 | Rev 3, 2017-11-30 | 3.47 Ohms | 4.25 Ohms | 415.36 Hz | 0.35 | 93.99 dB +/- 1.0db | 80 W | 2.5kHz - 20kHz |
| DX25TG59-04 | 2017-03-03 | 2.93 Ohms | 3.78 Ohms | 640.02 Hz | 0.57 | 93.02 dB +/- 1.0db | 100 W | 2.5KHz-20KHz |
| DA25TX00-08 | 2018-03-11 | 6.89 Ohms | 7.5 Ohms | 633.75 Hz | 0.6 | 88.94 dB +/- 1.0db | 100 W | 2K-20KHz |
| D27TG35-06 | 2018-05-14 | 4.89 Ohms | 5.49 Ohms | 911.7 Hz | 0.55 | 92.77 dB +/- 1.0db | 100 W | 2.5KHz - 20kHz |
| XT25TG30-04 | 2017-04-25 | 3 Ohms | 3.47 Ohms | 436.13 Hz | 0.44 | 90.84 dB +/- 1.0db | 110 W | 400hz - 20khz |
| XT25BG60-04 | 2017-05-24 | 2.99 Ohms | 3.44 Ohms | 589.1 Hz | 0.32 | 92.26 dB +/- 1.0db | 100 W | 500Hz - 20kHz |
| XT25SC90-04 | Printed: August 2021 | 3.2 Ω | 3.6 Ω | 830 Hz | 1 | 90.2 dB | 100 W (IEC 268-5) | Frequency Range 2000 - 20000 Hz |
| NE25VTS-04 | 2018-05-21 | 3.15 Ohms | 3.9 Ohms | 732.86 Hz | 0.89 | 91.26 dB +/- 1.0db | 80 W | 2kHz - 20kHz |

The DA25TX00-08's sheet also prints sensitivity 88.7 dB at 1 W, Qms 1.98, Qes 0.85, Sd
6.6 cm^2, Xmax 0.7 mm, "Corundum dome" and "the dome is very delicate so we recommend the grille
is fitted in normal operation"; in the item service (no units) overall outside diameter 4.8125,
cut-out 3.5, depth 2.75.

A lowest crossover frequency is printed for few of them: the TD25F-4's sheet ("Low Fs for use
with crossovers as low as 2,000 Hz (@12 dB/octave)"), and in the seller's text only for the
DC28FS-8 ("as low as 1,800 Hz") and the DA25TX00-08 ("as low as 1,200 Hz"). For the DC28F-8
neither the sheet nor the seller prints one (NOT READ); the usable range is the only guide.

Differences: the item service's sensitivity for the D27TG35-06, DX25TG59-04 and DA25TX00-08 is
the sheet's 1 W figure, not its 2.83 V figure; its frequency range for the XT25TG30-04 and
DA25TX00-08 is not the sheet's test bandwidth. No Dayton tweeter differs in a field both carry.

No tweeter's page or sheet has any wording on outdoor or humid use. Not sold by this seller on
the day: SB Acoustics and SEAS tweeters, the Dayton RS28F-4 and DSN25F-4.

## Pairs by tier

Sums of the prices above (arithmetic), every part in stock on the day:

| Tier | Woofer | Tweeter | Pair |
|---|---|---|---|
| good | Dayton Audio DC160-8, $34.98 | Dayton Audio ND25FA-4, $11.49 | $46.47 |
| good | Dayton Audio DC160-8, $34.98 | Dayton Audio DC28F-8, $24.98 | $59.96 |
| good | Dayton Audio DA175-8, $39.98 | Dayton Audio ND28F-6, $15.98 | $55.96 |
| better | Dayton Audio DS175-8, $49.98 | Dayton Audio DC28F-8, $24.98 | $74.96 |
| better | Peerless HDS-P830875, $49.50 (8 in stock) | Peerless DX25BG60-04, $34.50 | $84.00 |
| better | Dayton Audio RS150-8, $59.98 | Dayton Audio DC28F-8, $24.98 | $84.96 |
| better | Dayton Audio RS180-8, $79.98 | Dayton Audio DC28F-8, $24.98 | $104.96 |
| better | Dayton Audio RS180-8, $79.98 | Peerless XT25TG30-04, $28.50 | $108.48 |
| best | Dayton Audio RS180-8, $79.98 | Peerless DA25TX00-08, $56.25 (18 in stock) | $136.23 |
| best | Dayton Audio RS180P-8, $79.98 (16 in stock) | Epique EC30-4, $69.98 | $149.96 |
| best | Wavecor WF152BD14, $131.00 (10 in stock) | Wavecor TW030WA22, $77.00 | $208.00 |

## What the survey says, for the decision

The vented box the acoustics package designs for each 8 ohm woofer whose Qts allows one
(`vented_box_for_driver(driver, ql=7)` on the sheet's Fs, Qes, Qms and Vas; release v1.49.0, run
2026-10-06), and the same volume closed (`sealed_box(driver, vab_m3)`):

| Woofer | Price | `Driver.qts` | Alignment | Net volume | Tuning | -3 dB vented | -3 dB closed | Vd |
|---|---|---|---|---|---|---|---|---|
| DC160-8 | $34.98 | 0.342 | QB3 | 9.29 L | 41.5 Hz | 46.8 Hz | 76.2 Hz | 42.5 cm³ |
| DS175-8 | $49.98 | 0.268 | QB3 | 4.40 L | 54.2 Hz | 67.2 Hz | 100.9 Hz | 67.6 cm³ |
| RS150-8 | $59.98 | 0.342 | QB3 | 8.55 L | 55.7 Hz | 63.0 Hz | 102.3 Hz | 34.0 cm³ |
| RS180-8 | $79.98 | 0.312 | QB3 | 9.62 L | 45.2 Hz | 53.2 Hz | 83.4 Hz | 74.8 cm³ |
| RS180P-8 | $79.98 | 0.340 | QB3 | 11.14 L | 53.6 Hz | 60.8 Hz | 98.5 Hz | 76.0 cm³ |
| HDS-P830875 | $49.50 | 0.363 | QB3 | 13.72 L | 50.5 Hz | 55.0 Hz | 92.2 Hz | NOT READ |
| HDS-P835025 | $49.50 | 0.373 | QB3 | 17.06 L | 41.1 Hz | 44.0 Hz | 75.0 Hz | NOT READ |
| DSA135-8 | $39.98 | 0.382 | QB3 | 5.96 L | 54.5 Hz | 57.3 Hz | 99.3 Hz | 36.6 cm³ |

- The package designs a vented box (QB3 or B4) only at a Qts at or below 0.405 for a leakage Q
  of 7. The DA175-8 (0.58), the CFC180-8 (0.51), the DA135-8 (0.54) and the Peerless SDS
  woofers are above it: closed-box drivers.
- Of the 8 ohm woofers under $40, one has a Qts that low and a resonance under 40 Hz: the
  DC160-8. Its box is the deepest of the Dayton list (46.8 Hz in 9.3 L); only the HDS-P835025
  goes lower (44.0 Hz), in nearly twice the volume, with eight in stock and an item-service
  record that disagrees with its sheet.
- The RS180-8's box is nearly the DC160-8's size (9.62 L against 9.29 L) with 1.8 times the
  displacement (74.8 cm³ against 42.5 cm³) and 6.4 Hz less extension.
- Every 8 ohm part is far above the amplifier's 3.2 ohm minimum; several 4 ohm parts are under
  it (above).
- A tweeter usable an octave or more below the woofer's upper limit: the DC28F-8 (from
  1,300 Hz, Fs 834 Hz, 50 watts, 8 ohms) at $24.98. The $11.49 ND25FA-4 starts at 2,500 Hz
  with 20 watts, above where the DC160-8's sheet calls its response smooth ("up to 2 kHz").

## Not read

Other US sellers of drivers were not surveyed: the one seller's list holds pairs at all three
tiers, and the makers' sheets are the source of the parameters either way. The makers' own
sites for Peerless and Morel were not read (seller-hosted copies only). HiVi's data, the Morel
MDT 29 and MDT 12 sheets and several Wavecor pages were not read. No maker's dimension drawing
was read.

## What was read

On 2026-10-06. On the web: Parts Express's item service for every part in the tables (the URL
pattern above) and its shipping page; the Dayton Audio spec sheets and the seller-hosted
Peerless sheets for the parts in the parameter tables (the URL patterns above). In this
repository: `docs/proposals/P1-embedded-platform.md`, `docs/proposals/P12-enclosures.md`,
`docs/research/compact-speaker-drivers.md`. From the acoustics package of the owner's shared
Python library, release v1.49.0: its README (the vented alignments it designs and the Qts above
which it refuses), and the calls named above. The survey was done by a research agent; the four
parts the design names (DC160-8, DC28F-8, RS180-8, DA25TX00-08) were then read a second time
against the item service, and the DC160-8's and the DC28F-8's maker's sheets a second time from
the maker's site; the other rows are as that agent read them. No GPL source and no
reciprocally licensed hardware design file was opened; no speaker design program was used or
read.
