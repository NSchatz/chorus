# Drivers for the compact speaker: a survey of small woofers and dome tweeters at US sellers

Research for the compact speaker's acoustic design (`docs/hardware/compact-speaker.md`), 2026-10-06;
what `docs/decisions/0000-the-compact-speaker-drivers-and-alignment.md` rests on. Every number
was read on 2026-10-06 from the URL pattern named for it. Labels: NOT READ (no page read gives
it), LEAD (seen in a search snippet only; never used as a fact).

## What was asked

One woofer of 3 to 4.5 inches and one small dome tweeter for an active two-way in a printed box
of about 2 to 4 litres, each driver on its own channel of a TAS5825M at 24 V, crossed over
digitally. US seller, US ship-from (the US-first rule), single-unit list price. P1 leaves about
$59 of the $150 budget (K89) after the endpoint board and the PoE+ splitter; the enclosure, the
microphone, the mute switch and the controls take about $34 of that, so **the two drivers
together may cost about $25**.

## How it was read

The seller surveyed is Parts Express. Its product pages are drawn by script: an automated read
of `https://www.parts-express.com/<slug>` returns an empty page. The store's own item service
answers a plain request with the same data the page shows:

- `https://www.parts-express.com/api/items?country=US&currency=USD&language=en&fieldset=details&q=<model>`
  (price, stock, and the specification fields; the fields carry no units, so a unit below is
  taken from the maker's sheet where there is one).
- The maker's spec sheets: `https://www.daytonaudio.com/images/resources/<file>.pdf`, text
  extracted.

Ship-from: the seller's shipping page (https://www.parts-express.com/shipping) carries the
footer "725 Pleasant Valley Dr. Springboro, OH 45066 USA" and "Most orders ship same day when
received before 4PM ET"; its store configuration names the same address as the ship-from
address. LEAD: a search snippet of its FAQ, "ships orders Monday through Friday from its
warehouse in Springboro, Ohio".

## Woofers

All sold by Parts Express, all in stock on the day read.

| Model (seller's part) | Price | Product page slug | Spec sheet file |
|---|---|---|---|
| Dayton Audio TCP115-4 (295-415) | $15.98 | `Dayton-Audio-TCP115-4-4-Treated-Paper-Cone-Midbass-Woofer-4-Ohm-295-415` | `295-415--dayton-audio-tcp115-4-spec-sheet` |
| Dayton Audio TCP115-8 (295-416) | $15.98 | `Dayton-Audio-TCP115-8-4-Treated-Paper-Cone-Midbass-Woofer-8-Ohm-295-416` | `295-416--dayton-audio-tcp115-8-specification-sheet` |
| Dayton Audio PCS115-4 (295-660) | $12.98 | `Dayton-Audio-PCS115-4-4-Poly-Cone-Woofer-4-Ohm-295-660` | `295-660--dayton-audio-pcs115-4-specification-sheet` |
| Dayton Audio PC83-4 (295-154) | $15.49 | `Dayton-Audio-PC83-4-3-Full-Range-Poly-Cone-Driver-295-154` | `295-154-dayton-audio-pc83-4-specifications` |
| Dayton Audio PC105-4 (295-158) | $17.79 | `Dayton-Audio-PC105-4-4-Full-Range-Poly-Cone-Driver-295-158` | `295-158-dayton-audio-pc105-4-specifications` |
| Dayton Audio PC105-8 (295-160) | $17.79 | `Dayton-Audio-PC105-8-4-Full-Range-Poly-Cone-Driver-295-160` | `295-160-dayton-audio-pc105-8-specifications` |
| GRS 4SMP-4 (292-854) | $15.99 | `GRS-4-Woofer-Surface-Mount-Poly-Cone-4-Ohm-4SMP-4-292-854` | NOT READ |
| GRS 4SMP-8 (292-856) | $15.99 | `GRS-4-Woofer-Surface-Mount-Poly-Cone-8-Ohm-4SMP-8-292-856` | NOT READ |
| Peerless TC9FD18-08 (264-1062) | $12.75 | `Peerless-TC9FD18-08-3-1-2-Full-Range-Paper-Cone-Woofer-264-1062` | NOT READ |
| GRS 4PF-8 (292-404) | $9.99 | `GRS-4PF-8-4-Paper-Cone-Foam-Surround-Woofer-292-404` | NOT READ |

Values as printed on the maker's sheet (the Dayton Audio rows) or as the item service gives
them without units (the GRS and Peerless rows):

| Model | Nominal | Fs | Re | Qms | Qes | Qts | Vas | Sd | Xmax | Sensitivity | RMS power | Usable range |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| TCP115-4 | 4 ohms | 53.8 Hz | 3.2 ohms | 3.14 | 0.40 | 0.35 | 3.1 liters | 50.3 cm² | 4.0 mm | 86.8 dB @ 2.83V/1m | 40 watts | 55 - 5,000 Hz |
| TCP115-8 | 8 ohms | 59.2 Hz | 7.8 ohms | 2.3 | 0.54 | 0.43 | 2.52 liters | 50 cm² | 4 mm | 81.9 dB @ 2.83V/1m | 40 watts | 55 - 6,000 Hz |
| PCS115-4 | 4 ohms | 88 Hz | 3.6 ohms | 3.11 | 1.05 | 0.79 | 1.77 liters | 51.5 cm² | 2.75 mm | 86.1 dB @ 2.83V/1m | 20 watts | 60 - 5,200 Hz |
| PC83-4 | 4 ohms | 80.1 Hz | 3.9 ohms | 3.41 | 0.64 | 0.54 | 0.07 ft³ | 30.2 cm² | 2.0 mm | 86.8 dB @ 2.83V/1m | 30 watts | 80 - 20,000 Hz |
| PC105-4 | 4 ohms | 81.4 Hz | 3.3 ohms | 2.17 | 0.67 | 0.51 | 0.13 ft³ | 52.8 cm² | 2.0 mm | 90.3 dB @ 2.83V/1m | 40 watts | 80 - 15,000 Hz |
| PC105-8 | 8 ohms | 84.8 Hz | 6.7 ohms | 2.29 | 0.88 | 0.64 | 0.12 ft³ | 52.8 cm² | 2.0 mm | 86.3 dB @ 2.83V/1m | 40 watts | 80 - 15,000 Hz |
| 4SMP-4 | 4.0 | 58.0 | 3.7 | 3.05 | 0.67 | 0.55 | 0.14 | 54.0 | 4.0 | 86.0 | 30 | 50 - 6,000 |
| 4SMP-8 | 8.0 | 63.0 | 5.5 | 3.0 | 0.83 | 0.65 | 0.141 | 54.0 | 4.0 | 84.4 | 30 | 50 - 6,000 |
| TC9FD18-08 | 8.0 | 125.0 | 6.3 | 2.7 | 1.33 | 0.89 | 0.04 | 36.3 | 2.55 | 83.5 | 30 | 100 to 20,000 |
| 4PF-8 | 8.0 | 137.0 | 6.8 | 5.81 | 2.9 | 1.94 | 0.04 | 55.2 | 1.5 | 83.0 | 40 | 100 to 10,000 |

The TCP115-4's other printed values: Le 0.97 mH, Mms 9.9 g, Cms 0.88 mm/N, BL 5.2 Tm,
Vd 20.1 cm³, voice coil 1 inch. Its outside diameter, cut-out and depth are 4.57, 3.77 and 2.32
in the item service (inches by the maker's drawing scale: NOT READ as a unit).

Differences between the two sources: the PCS115-4's Qes is 1.05 on the sheet and 1.5 in the
item service; the TCP115-4's sheet is titled "Poly Cone" where the seller says "Treated Paper";
the PC105-4's BL is "N/A" on the sheet and 3.3 in the service.

No woofer's page or sheet has any wording on outdoor, humid, moisture or weather use.

Over $18 on the day read, so not surveyed further: DS90-8 $21.98, DS115-8 $26.98, ND90-4
$25.98, ND91-4 $27.98, DSA90-8 $26.98, DMA105-4 $19.98, DA115-8 $24.98.

## Tweeters

All sold by Parts Express, all in stock on the day read.

| Model (seller's part) | Price | Product page slug | Spec sheet file |
|---|---|---|---|
| Dayton Audio ND13FA-4 (275-104) | $7.49 | `Dayton-Audio-ND13FA-4-1-2-Soft-Dome-Neodymium-Tweeter-4-Ohm-275-104` | `275-104--dayton-audio-nd13fa-4-spec-sheet` |
| Dayton Audio ND16FA-6 (275-025) | $8.98 | `Dayton-Audio-ND16FA-6-5-8-Soft-Dome-Neodymium-Tweeter-275-025` | `275-025-dayton-audio-nd16fa-6-specifications-46192` |
| Dayton Audio ND20FA-6 (275-030) | $10.98 | `Dayton-Audio-ND20FA-6-3-4-Soft-Dome-Neodymium-Tweeter-275-030` | `275-030-dayton-audio-nd20fa-6-specifications-46118` |
| Dayton Audio TD20F-4 (275-020) | $10.98 | `Dayton-Audio-TD20F-4-3-4-Soft-Dome-Neodymium-Tweeter-4-Ohm-275-020` | `275-020--dayton-audio-td20f-4-spec-sheet` |
| Dayton Audio ND25FN-4 (275-053) | $11.29 | `Dayton-Audio-ND25FN-4-1-Silk-Dome-Neodymium-Tweeter-Element-4-Ohm-275-053` | `275-053--dayton-audio-ND25FN-4-specifications` |
| Dayton Audio ND25FA-4 (275-059) | $11.49 | `Dayton-Audio-ND25FA-4-1-Soft-Dome-Neodymium-Tweeter-275-059` | `275-059--dayton-audio-ND25FA-4-specifications` |
| Dayton Audio ND20FB-4 (275-035) | $12.98 | `Dayton-Audio-ND20FB-4-Rear-Mount-3-4-Soft-Dome-Neodymium-Tweeter-275-035` | `275-035-dayton-audio-nd20fb-4-specifications-46117` |

As printed on the maker's sheets; the faceplate, cut-out and depth are the item service's
(no units):

| Model | Nominal | Fs | Re | Sensitivity | RMS power | Usable range | Faceplate / cut-out / depth |
|---|---|---|---|---|---|---|---|
| ND13FA-4 | 4 ohms | 2832 Hz | 3.07 ohms | 88.5 dB 2.83V/1m | 20 watts | 4,500 - 20,000 Hz | 1.77 / 1.16 / 0.45 |
| ND16FA-6 | 6 ohms | 2125 Hz | 5.8 ohms | 88 dB @ 2.83V/1m | 10 watts | 3,500 - 27,000 Hz | 1.28 / 1.28 / 0.57 |
| ND20FA-6 | 6 ohms | 2,005 Hz | 5.2 ohms | 90 dB @ 1W/1m | 15 watts | 3,500 - 25,000 Hz | 1.77 / 1.30 / 0.59 |
| TD20F-4 | 4 ohms | 1696 Hz | 3.4 ohms | 90.0 dB @ 2.83V/1m | 20 watts | 3,000 - 20,000 Hz | 2.56 / 1.69 / 0.59 |
| ND25FN-4 | 4 ohms | 1,350 Hz | 3.2 ohms | 90 dB @ 2.83V/1m | 20 watts | 2,500 - 20,000 Hz | 1.61 / 1.34 / 0.83 |
| ND25FA-4 | 4 ohms | 1,350 Hz | 3.2 ohms | 90 dB @ 2.83V/1m | 20 watts | 2,500 - 20,000 Hz | 2.60 / 1.77 / 1.0 |
| ND20FB-4 | 4 ohms | 2,072 Hz | 3.2 ohms | 90 dB @ 1W/1m | 15 watts | 3,500 - 25,000 Hz | 1.54 / 1.41 / 0.75 |

The ND16FA-6's other printed values: Qms 3.45, Qes 6.09, Qts 2.20, Le 0.04 mH @ 10kHz,
Sd 2.0 cm², voice coil 16 mm. No sheet or page prints a recommended lowest crossover frequency
or slope for any of these tweeters (NOT READ); only the usable range is given.

## Pairs under the cap

Sums of the prices above (arithmetic):

| Woofer | with ND13FA-4 | with ND16FA-6 | with ND20FA-6 or TD20F-4 | with ND25FN-4 |
|---|---|---|---|---|
| TCP115-4 or TCP115-8, $15.98 | $23.47 | $24.96 | $26.96 | $27.27 |
| GRS 4SMP-4, $15.99 | $23.48 | $24.97 | $26.97 | $27.28 |
| PC83-4, $15.49 | $22.98 | $24.47 | $26.47 | $26.78 |
| PCS115-4, $12.98 | $20.47 | $21.96 | $23.96 | $24.27 |
| TC9FD18-08, $12.75 | $20.24 | $21.73 | $23.73 | $24.04 |

## What the survey says, for the decision

- A vented alignment the acoustics package designs (QB3 or B4) needs a Qts at or below 0.405 at
  a leakage Q of 7. One woofer under $18 has it: the TCP115-4 (0.35). The TCP115-8 (0.43) is
  just above; every other candidate is above 0.5, which is a closed-box or full-range driver.
- The TCP115-4 also has the lowest resonance (53.8 Hz) and the largest excursion (4.0 mm, with
  the 4SMP) of the list, and at 4 ohms takes the most power from a 24 V amplifier.
- With the TCP115-4 at $15.98, a tweeter of at most about $9 keeps the pair near $25: the
  ND16FA-6 (usable from 3,500 Hz) or the ND13FA-4 (from 4,500 Hz, close to the top of the
  woofer's 5,000 Hz).

## Not read

Other US sellers (two speaker-part shops and two large distributors) were not surveyed: the one
seller's list already holds a pair inside the cap, and the maker's sheets are the source for
the parameters either way. The GRS and Peerless makers' sheets were not read (the seller's item
service only). Parts Express's FAQ and terms pages are script-drawn and gave nothing.

## What was read

On 2026-10-06. On the web: Parts Express's item service for every part in the tables (the URL
pattern above), its shipping page, and its store configuration (the ship-from address); the
Dayton Audio spec sheets named in the tables (the URL pattern above). In this repository:
`docs/proposals/P1-embedded-platform.md` (the electronics' share of the budget),
`docs/proposals/P12-enclosures.md` (the enclosure's share), `docs/hardware/voice-mic.md`. From
the acoustics package of the owner's shared Python library, release v1.49.0: its README (the
vented alignments it designs and the Qts above which it refuses). Part of the survey was done
by a research agent; the two chosen drivers' rows and sheets and every tweeter's price were
then read a second time against the item service, and the other rows are as that agent read them. No GPL source and no reciprocally licensed hardware design file was opened; no
speaker design program was used or read.
