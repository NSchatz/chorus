# The voice-room microphone: a buy list for the owner

Candidate microphone parts and modules for the compact speaker class (K67: "mic + hardware mute
switch"), the one class that carries a microphone (`docs/hardware/controls.md`). The voice path
they feed is proposal P8 Option A with the wake word on the server (K71, K73;
`docs/proposals/P8-voice-path.md`; decisions 0166, 0167, 0169, 0172).

**Nothing here is ordered, and nothing here is chosen.** The purchase is the owner's: he owns no
microphone (K21), and he decided on 2026-10-04 to defer the purchase until this list exists and
then buy one microphone for bench use. Which one, when, and from which seller is his call. No
task waits on the hardware: the voice path is tested against fakes, and on a board the capture
seam stays empty until a part and a driver are chosen (`docs/hardware/controls.md`).

This page chooses no pins and no driver and designs no echo cancellation. Those belong to the
compact speaker design (goal 24) and to the task that binds a capture driver.

## What the voice path needs from a microphone

- **16 kHz, mono, 16-bit PCM** is what leaves the speaker in `mic_audio` (decision 0166). Any
  candidate must be clockable to 16 kHz or to a rate the firmware can convert from; this page
  does not say how.
- **A supply or a data line a switch can break** (the hardware rule, below).
- **One microphone per speaker.** The server detects the wake word (K73); no beamforming or
  on-device wake word is asked of the speaker.

## The hardware mute rule each candidate is held to

From `docs/hardware/controls.md`: the latching switch "physically breaks the microphone's supply
(or its data line, for a digital microphone whose supply cannot be switched) so no firmware fault
can un-mute it", and a second pole of the same switch goes to a GPIO so the firmware knows the
position. So the switch is a latching two-pole part (a DPDT slide or toggle: one pole in the
microphone's supply or data line, one pole to the GPIO). The switch itself is not on this list:
it is a mechanical choice of the compact design (goal 24).

## Candidates

Prices are single-unit list prices in US dollars as each seller's page showed them on the date
read, before shipping and tax. Every number in a row was read from the URL in that row on that
date.

| # | Candidate | Price | Interface | How the mute switch cuts it | URL, date read |
|---|---|---|---|---|---|
| M1 | Adafruit I2S MEMS Microphone Breakout, SPH0645LM4H (product 3421): one bottom-ported mono MEMS microphone; "about 50Hz - 15KHz"; "1.6-3.6V max device only" | $6.95, in stock | I2S, the microphone is clocked by the controller: pins `BCLK`, `LRCL`, `DOUT`, `SEL` (left or right slot), `3V`, `GND` | Pole 1 breaks the `3V` supply pin, the rule's first form: the breakout brings the supply out on its own pin. Pole 2 to a GPIO. | https://www.adafruit.com/product/3421, read 2026-10-05 |
| M2 | Adafruit I2S MEMS Microphone Breakout, ICS-43434 (product 6049): one mono MEMS microphone; 1.6-3.6 V; current 490 uA (high performance mode), 230 uA (low power mode); sample rate 23-51.6 kHz (high performance), 6.25-18.75 kHz (low power); SNR 65 dBA / 64 dBA | $8.95, in stock. The page says the ICS-43434 "has been discontinued" and names the SPH0645LM4H (M1) "a drop-in replacement" | I2S, clocked by the controller: clock, data, word select, select | As M1: pole 1 breaks the supply pin. Pole 2 to a GPIO. | https://www.adafruit.com/product/6049, read 2026-10-05 |
| M3 | Adafruit PDM MEMS Microphone Breakout (product 3492): one MEMS microphone; 1.8-3.3 V; current draw 0.6 mA; SNR 61 dB; sensitivity about -26 dBFS | $4.95, in stock | PDM: one clock in ("Clock rate: 1 - 3.25 MHz"), one data out; the controller filters the 1-bit stream to PCM | As M1: pole 1 breaks the supply pin. Pole 2 to a GPIO. | https://www.adafruit.com/product/3492, read 2026-10-05 |
| M4 | Seeed Studio reSpeaker Lite (2-microphone array on an XMOS XU316, without the optional ESP32-S3 module): SNR 64 dBA, sensitivity -26 dBFS, "Maximum Sampling Rate 16Khz"; the processor runs "interference cancellation, echo cancellation, and noise suppression" on the board | $26.99, in stock | "supports I2S and USB connections"; power "USB 5V, External 5V" | The board has its own mute button ("Mutes audio input when pressed") and a red mute light, but that mute is the board's own logic, not a break in a supply, so it does not meet the rule. To meet it, pole 1 would break the board's 5 V supply (which also powers down its processor) or its I2S data line. Pole 2 to a GPIO. Neither is shown by the pages read. | price: https://www.seeedstudio.com/ReSpeaker-Lite-p-5928.html; specification and mute button: https://wiki.seeedstudio.com/reSpeaker_usb_v3/; both read 2026-10-05 |
| M5 | Seeed Studio reSpeaker XMOS XVF3800 (4-microphone array): the processor runs "AEC, AGC, DoA, VAD, dereverberation, beamforming, and noise suppression" on the board | $60.99, in stock | "dual modes (I2S/USB)" | Not read: the page read says nothing of a mute control or of the supply. As M4, the rule would need a break in the board's supply or its I2S data line. | https://www.seeedstudio.com/ReSpeaker-XVF3800-USB-Mic-Array-p-6488.html, read 2026-10-05 |

### What separates them

- **M1, M2 and M3 are bare microphones on a breakout.** Each has a supply pin a switch pole can
  break, which is the rule's first and simplest form. None processes the signal: echo of the
  speaker's own playback reaches the server as captured. Whether that matters for wake-word
  detection in a compact speaker is the open AEC input of P8, not answered here.
- **M4 and M5 are processing boards.** They cancel echo on the board, which needs the playback
  signal fed back to them as a reference, more pins, and a second processor with its own
  firmware inside the speaker. Their own mute is not a hardware break. M5 has four microphones
  and direction finding, more than a one-microphone speaker whose wake word runs on the server
  asks for.
- **M2 is discontinued by its seller's own page**, so it is on the list only as M1's twin: a
  design that fits M1 fits M2 while stock lasts.

### A suggestion for the one bench microphone

Not a decision: the owner decides. For the one bench microphone of his 2026-10-04 answer, **M1**
(SPH0645LM4H breakout, $6.95) is the smallest step that exercises everything built so far: it is
a single mono microphone, its supply pin takes the mute switch as the rule's first form, and it
is the part its seller names as the current one of the M1/M2 pair. M3 ($4.95) is the alternative
if the compact design prefers a two-wire PDM microphone. M4 is worth buying only once the AEC
question has been asked on the bench with a bare microphone and answered "needed".

## Still unverified against a datasheet

Everything above was read from sellers' product pages and one seller's wiki, not from a
manufacturer's datasheet (CLAUDE.md rule 6: these are starting points, not truth). Before a part
goes into a design, each of these is checked against the manufacturer's datasheet, by revision
and page:

1. **M1 at 16 kHz.** The page read gives no sample-rate range, SNR or supply current for the
   SPH0645LM4H. That it can be clocked to produce 16 kHz, and what clock that takes, is
   unverified. (M2's page gives a low-power range of 6.25-18.75 kHz that contains 16 kHz; that
   too is the seller's page, not the datasheet.)
2. **The data word.** The bit depth, justification and timing of each I2S microphone's output
   word, and so how 16-bit samples are taken from it, were not read.
3. **A cut supply with live clock lines.** With pole 1 open, the controller's clock, word-select
   and select lines may still be driven into an unpowered part. Whether the part then stays
   silent, whether it can be back-powered through its input protection, and whether the design
   must also stop or isolate those lines while muted, is unverified for every candidate. The
   mute rule is only met when the datasheet (or a bench measurement) shows the microphone
   produces no data with its supply broken.
4. **Start-up after unmute.** How long each part takes to give valid samples after its supply
   returns was not read. The firmware gate starts closed and reports `mic_state` only after the
   switch is read, so a slow start costs audio, not privacy.
5. **M3's part.** The page read does not name the PDM microphone's manufacturer part number in
   the text that was read; its datasheet is not identified yet.
6. **M4 and M5.** The supply current, the microphones' part numbers, what the I2S firmware
   outputs (rate, channels, word), whether the board runs as the clock source or is clocked,
   how its echo canceller takes its reference, the firmware's licence, and what the board does
   when its data line or supply is broken: none was read. For M5 no mute fact was read at all.
7. **Acoustics.** Port placement (M1 is bottom-ported: "make sure you have the hole in the
   bottom facing out"), sealing to the enclosure, and distance from the driver are the compact
   design's (goal 24).
8. **Prices and stock** are one seller's on one day. Distributor pages for the bare parts (two
   large distributors were tried on 2026-10-05) refused an automated read, so no bare-part price
   is given; a bare part is a matter for a board design, not for the bench.

## What this page does not do

It orders nothing and spends nothing; it chooses no pins, no driver and no part; it designs no
echo cancellation; it files nothing. The purchase is the owner's, an owner step.

## Sources

- Adafruit product pages 3421, 6049 and 3492 (URLs in the table), read 2026-10-05.
- Seeed Studio product pages for the reSpeaker Lite and the reSpeaker XMOS XVF3800 (URLs in the
  table), and the reSpeaker Lite wiki page https://wiki.seeedstudio.com/reSpeaker_usb_v3/, read
  2026-10-05.
- In this repository: `docs/hardware/controls.md` (the mute rule), `docs/proposals/P8-voice-path.md`
  ("Hardware later"; "Open inputs"), decision 0166 (the audio format).
- No GPL source and no reciprocally licensed hardware design file was opened: sellers' product
  and wiki pages only, no schematic or board file.
