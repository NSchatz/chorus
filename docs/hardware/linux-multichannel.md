# Multichannel output on the Linux endpoint tier

How a Linux endpoint (K96) drives more than two output channels: the rack amp's zones (K74) and
the theater hub (K72, P2 Option A: stereo LPCM from optical and ARC now, 5.1 later). The software
half is `chorus-client`'s output map (`crates/client-linux/src/outmap.rs`, ADR 0068,
`docs/decisions/0068-the-output-map.md`); this page is the hardware half and how the two
meet.

**The Linux board is an unanswered owner input** (P4 deferred; the Needs items "The room list and
its wiring" and "The rack: free units and depth"). Everything below that names a board, a HAT, a
zone count or a channel layout is the design's example, **ASSUMED**, and parameterised: the
client takes the device's channel count and the map on its command line, so a different board is
a different command line, not different code.

## The constraint

No Raspberry Pi supports I2S MCLK or TDM. Raspberry Pi's own white paper says of MCLK that it is
"NOT supported on any Raspberry Pi SBCs", and of more than two channels: "some hardware solutions
implement multiple data lines, often referred to as SD0, SD1, and SD2; alternatively, it's
possible to divide a single SD line into multiple time slots ... This is known as time-division
multiplexing, or TDM. Raspberry Pi 5 uses the former approach; TDM is NOT supported on any
Raspberry Pi SBCs" (RP-009699-WP "Using the I2S peripherals on Raspberry Pi SBCs", Release 1,
pp. 4-5, [W1], read 2026-09-30). Raspberry Pi 4 and earlier have "one I2S peripheral ... with one
bidirectional data lane" (W1 p. 6): stereo only.

So multichannel on a Pi means the Raspberry Pi 5's parallel data lanes, or another board.

## The options

| | A. Pi 5 + parallel-lane 8-channel DAC HAT | B. USB Audio Class 2 interface | C. Non-Pi SBC with I2S TDM |
|---|---|---|---|
| Channels | 8 out, one ALSA device | 8 or more, one ALSA device | 8 per TDM port, more with several |
| Clock domains | one: RP1's audio PLL, the Pi is the I2S clock producer | the interface's own crystal (asynchronous USB), a second domain beside the board's | one, the SoC's |
| Chorus sync | the device delay the client already disciplines; one clock per card | the same, against a clock the host only follows by feedback | the same |
| Rates | 48 kHz family native to the audio PLL (below) | what the interface offers | what the SoC and codec offer |
| Maturity | one product uses all four lanes (W1 p. 14) | class-compliant, driven by the stock `snd-usb-audio` | a carrier and codec board to design, a vendor kernel |
| Fits | rack amp zones, theater hub | the same, as a fallback | more than 8 channels, MCLK codecs, TDM amplifier ICs |

### A. Raspberry Pi 5 parallel lanes

- RP1, the Pi 5's I/O chip, "has three instances of the Synopsys Designware I2S peripheral ... two
  of which are available on GPIO bank 0"; "I2S0 is a clock-producer (master) with up to 4
  bidirectional channels", "I2S1 is a clock-consumer (slave) with up to 4 bidirectional
  channels", and each "channel's I2S data transmit pin is connected to sdo[n]"; "Maximum audio
  channel data resolution is 32 bits" (RP1 Peripherals datasheet RP-008370-DS-1, section 3.7,
  pp. 51-52, [D1], read 2026-09-30). A "channel" there is a data lane carrying a stereo pair: four
  lanes are eight audio channels on one bit clock and one word clock (GPIO18 `I2S0_SCLK`, GPIO19
  `I2S0_WS`, GPIO21/23/25/27 `I2S0_SDO[0..3]`, D1's bank 0 function table).
- The white paper: "there is one third-party product that takes advantage of all four lanes
  available on RP1: the HiFiBerry DAC8x. To the Advanced Linux Sound Architecture (ALSA), this
  appears as a device that can support up to eight channels (e.g. four stereo pairs)" (W1 p. 14).
- HiFiBerry DAC8x: "8 channel of high-quality audio output", "only compatible with the Raspberry
  Pi5", "up to 192kHz", $64.90 on the maker's shop page ([H1], read 2026-09-30); "Four dedicated
  192kHz/24bit high-quality Burr-Brown DACs", 44.1-192 kHz, `dtoverlay=hifiberry-dac8x`, 2.1 Vrms
  out, "HAT compliant" ([H2], read 2026-09-30). The Studio DAC8x is the balanced variant: "8x
  Balanced output connector (DB25 female), TASCAM pinout", 4.2 Vrms ([H3], read 2026-09-30; price
  not on a fetched page). The ADC8x adds 8 inputs and "can only be used as an add-on to the DAC8x"
  ([H4], read 2026-09-30), which is where the rack amp's line-in (K70) could come from.
- The DAC8x's output connector type and its DAC part number were not on the fetched pages
  (UNCONFIRMED).
- Clock: "The reference clock is a 50MHz crystal input" and "The Audio PLL will run at a VCO
  frequency of 1.536GHz to generate internal audio clocks ... I2S master clock ... 2^N x 48000
  (still an integer division from VCO freq)" (D1 sections 2.5 and 2.5.4, pp. 11-12). 1.536 GHz is
  32000 x 48 kHz but 1.536e9 / (44100 x 64) is 544.2, so the 44.1 kHz family is not an integer
  division of that VCO frequency (arithmetic, not a datasheet statement). The design runs these
  cards at 48 kHz (ASSUMED until measured).
- Crystal tolerance and I2S jitter are not in D1 (UNCONFIRMED); as for every endpoint, the card's
  rate against the server timeline is what the sync loop measures and corrects, and only a
  hardware report under `docs/measurements/` says how well.

### B. A USB Audio Class 2 interface

- The class: USB-IF "Audio Devices Rev. 2.0" ([U1], read 2026-09-30; the page, not the
  specification body). Linux's `snd-usb-audio` is the "Module for USB audio and USB MIDI devices",
  with `autoclock` ("Enable auto-clock selection for UAC2 devices") and `implicit_fb` for
  asynchronous devices ([K1], read 2026-09-30).
- An example: the ESI GIGAPORT eX, "8 independent output channels", "-10dBV RCA connectors", "100%
  class compliant" ([E1], read 2026-09-30), and its maker: "fully class compliant, so it is
  supported by the USB audio driver from the ALSA package" ([E2], read 2026-09-30).
- An asynchronous USB device runs on its own crystal and the host follows it; for chorus that is
  one more clock whose rate the sync loop disciplines against the device's reported delay, which
  it already does for every card. It costs a box on a cable in a 2U chassis.

### C. A non-Pi board with TDM

- Rockchip RK3588: "I2S0/I2S1 with 8 channels", "Support TDM", "Provides master and slave work
  mode", with an `I2S0_MCLK` pin (RK3588 datasheet Rev 1.6, section 1.2.12, p. 13, [R1], read
  2026-09-30, from a board vendor's mirror). NXP i.MX 8M Plus: "18 x I2S TDM (32 b @ 384 kHz)" and
  eARC in the family fact sheet ([N1], read 2026-09-30); lane counts per instance UNCONFIRMED.
- Worth it only for more than 8 channels, codecs that need MCLK, TDM amplifier ICs on one bus, or
  a theater hub that must take HDMI eARC itself (goal 13's question, not this one).

## The choice

**Option A, a Raspberry Pi 5 with a parallel-lane 8-channel DAC HAT (the HiFiBerry DAC8x class),
for both the rack amp and the theater hub. The fallback is option B, a class-compliant UAC2
interface.** Option C is not chosen: it trades a documented, stocked part for a board design, and
nothing in K72 or K74 as known today needs more than eight channels on one card.

Why A: one card is one ALSA device on one clock (the RP1 audio PLL, the Pi producing the I2S
clocks), so every zone on the card shares one sample clock and the sync loop has one device rate
to discipline per box, not one per zone. It is the only way a Pi reaches eight channels (W1), the
white paper names exactly this product, and the ADC8x stacks on it for the line-in the rack amp
carries (K70). Why B as the fallback: nothing to build, any Linux board, the same client and map;
the cost is a second clock domain beside the board's and a USB cable in the chassis.

The board model is the owner's input (P4 deferred), so this is ASSUMED: the example below is a
Pi 5 with a DAC8x, and changing it changes the command lines, not the code.

## How the two products use it

### The rack amp: several zones on one card

Each zone plays its own stream (a zone is what the server serves a stream to), and one
`chorus-client` plays one zone's stream. So **one chorus-client per zone**, each on the channels of
the card that are its zone's, and the card is shared through ALSA's `dshare` plugin: "This plugin
provides sharing channels. Unlike share plugin, this plugin doesn't need the explicit server program
but accesses the shared buffer concurrently from each client", with `bindings` that map a client
channel to a slave channel ([A1], read 2026-09-30). The alternative, one client per card playing
every zone, would need one stream carrying every zone's audio, which is not how zones are served;
it is right only for the theater hub, below.

Consequences, each a design rule:

- **One rate per card.** `dshare`'s slave is a single configuration (rate, format, period, buffer)
  every client shares (A1: the direct plugins support "only a single configuration"; that dshare
  shares it is read from its one `slave` block). Every zone on a card plays at the card's rate
  (48 kHz, ASSUMED); a stream at another rate is refused at open or resampled above `dshare`.
- **Each zone's delay is the shared ring's.** Every client reads its device delay from the one
  shared buffer, which is what its sync loop disciplines. That several clients' reported delays
  track one card's position the way a sole client's does is a claim for a hardware report, not
  for this page (LEAD until measured).
- **The map runs inside each client.** A zone's client opens its `dshare` PCM with the zone's
  channel count and maps its stream onto it (the sub on the same card is one more bound channel,
  fed `FL+FR`).

Example, ASSUMED (four output pairs; the zone and channel count is P13, goal 26, from the room
list): an 8-channel card as zone A on 0-1, zone B on 2-3, zone C on 4-5, and zone A's sub plus a
line out to the AVR on 6-7. The `dshare` PCMs (in the endpoint's `asoundrc`; the slave is the
card):

```
pcm_slave.rack { pcm "hw:DAC8x,0"  channels 8  rate 48000  format S24_3LE }
pcm.zone_a { type dshare  ipc_key 4821  slave rack  bindings { 0 0  1 1  2 6 } }
pcm.zone_b { type dshare  ipc_key 4821  slave rack  bindings { 0 2  1 3 } }
pcm.zone_c { type dshare  ipc_key 4821  slave rack  bindings { 0 4  1 5 } }
pcm.avr    { type dshare  ipc_key 4821  slave rack  bindings { 0 7 } }
```

and zone A's client, with its sub 1.5 ms late and trimmed:

```
chorus-client --zone zone-a --device zone_a --output-channels 3 \
  --output 0=FL --output 1=FR --output 2=FL+FR,gain-db=3.0,delay-us=1500
```

The card name, `ipc_key`, format and the sub trim are illustrative (ASSUMED); the ALSA
configuration syntax is A1's.

### The theater hub: one stream, one client

The theater hub plays one stream: stereo LPCM today (P2 Option A), 5.1 later. **One chorus-client
for the whole card**, with a map over all its channels. Stereo today:

```
chorus-client --device hw:DAC8x,0 --output-channels 8 --output 0=FL --output 1=FR
```

and 5.1 later, with the stream's positions routed to the outputs the front, centre, sub and
surround amplifiers are wired to, and the centre and surrounds trimmed for distance:

```
chorus-client --device hw:DAC8x,0 --output-channels 8 \
  --output 0=FL --output 1=FR --output 2=FC,delay-us=900 --output 3=LFE \
  --output 4=SL,delay-us=2500 --output 5=SR,delay-us=2500
```

A 5.1 stream from the theater path arrives in whatever order its transport used and says so in
its channel map (`docs/protocol.md`, "The channel map"); the map above names positions, not
indices, so it is right for any of them. Which DAC8x output is which lane and pin is the HAT's
documentation (not read here; ASSUMED lane `n` carries channels `2n` and `2n+1`).

## Timing, said once

The output map changes what each channel carries, never when a frame plays. A channel given a
delay plays **exactly its delay later than the sync target**, on purpose (a speaker-distance or
sub-alignment trim, at most 50 ms, `MAX_DELAY_US`); every other channel plays at the target. The
device delay the client disciplines, the buffer and the sync loop are the same with or without a
map (ADR 0068). No timing claim is made here: every number above is cited or ASSUMED,
and the first measured one belongs in `docs/measurements/`.

## Sources

- [W1] Raspberry Pi Ltd, "Using the I2S peripherals on Raspberry Pi SBCs", RP-009699-WP, Release 1,
  <https://pip-assets.raspberrypi.com/categories/1259-audio-camera-and-display/documents/RP-009699-WP-1-Using%20the%20I2S%20peripherals%20on%20Raspberry%20Pi%20SBCs.pdf>,
  read 2026-09-30 (its prose only; the device-tree listings it embeds are not used).
- [D1] Raspberry Pi Ltd, "RP1 Peripherals", RP-008370-DS-1,
  <https://datasheets.raspberrypi.com/rp1/rp1-peripherals.pdf>, read 2026-09-30.
- [H1] <https://www.hifiberry.com/shop/boards/hifiberry-dac8x/>, read 2026-09-30.
- [H2] <https://www.hifiberry.com/docs/data-sheets/datasheet-dac8x/>, read 2026-09-30.
- [H3] <https://www.hifiberry.com/docs/data-sheets/datasheet-studiodac8x/>, read 2026-09-30.
- [H4] <https://www.hifiberry.com/docs/data-sheets/datasheet-adc8x-add-on/>, read 2026-09-30.
- [A1] ALSA project, "PCM (digital audio) plugins",
  <https://www.alsa-project.org/alsa-doc/alsa-lib/pcm_plugins.html>, read 2026-09-30 (the
  documentation page; no alsa-lib source).
- [A2] ALSA project, PCM interface (channel map API),
  <https://www.alsa-project.org/alsa-doc/alsa-lib/group___p_c_m.html>, read 2026-09-30.
- [K1] Linux kernel documentation, "ALSA configuration guide" (`snd-usb-audio`),
  <https://docs.kernel.org/sound/alsa-configuration.html>, read 2026-09-30; and "ALSA PCM channel-mapping
  API", <https://docs.kernel.org/sound/designs/channel-mapping-api.html>, read 2026-09-30.
- [U1] USB-IF, "Audio Devices Rev. 2.0 and Adopters Agreement",
  <https://www.usb.org/document-library/audio-devices-rev-20-and-adopters-agreement>, read 2026-09-30.
- [E1] <https://www.esi-audio.com/products/gigaportex/>, read 2026-09-30.
- [E2] <https://kb.esi-audio.com/?goto=KB00275EN>, read 2026-09-30.
- [R1] Rockchip RK3588 Datasheet Rev 1.6,
  <https://wiki.friendlyelec.com/wiki/images/e/ee/Rockchip_RK3588_Datasheet_V1.6-20231016.pdf>,
  read 2026-09-30.
- [N1] NXP i.MX 8M Plus fact sheet, <https://www.nxp.com/docs/en/fact-sheet/IMX8MPLUSFS.pdf>, read
  2026-09-30.
- LEAD only, not built on: the Audio Injector Octo (maker page unreachable, TLS expired, read
  attempt 2026-09-30); the Behringer UMC1820's class compliance (a search snippet; its maker page,
  <https://www.behringer.com/en/products/0805-AAN>, read 2026-09-30, gives "18 inputs and 20 outs"
  and no Linux statement); ALSA `surround51`/`surround71` channel order (a snippet, as in
  `docs/protocol.md`); Raspberry Pi forum threads t=120165 and t=383641.

No GPL or LGPL source was opened: no Linux kernel source or device-tree source, no alsa-lib
source.
