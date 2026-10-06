# The LCR set: three two-ways and the hidden TV hub

version: 1

- Status: designed on paper, 2026-10-06. Nothing is ordered, printed, built or measured. The
  set adds no acoustic design: its three speakers are `chorus-twoway-v1`
  (`docs/hardware/twoway-speaker.md`) unchanged, and its hub is P4's "At install: the two TV
  hubs" (`docs/proposals/P4-bench-purchase.md`). Every number not read from a cited page is
  arithmetic on cited numbers or marked **ASSUMED**. The prices rest on the two-way's budget,
  which is PROPOSED (the owner decides its class budget and tier), so the totals here are
  PROPOSED for the same reason.
- What this is: the theater front of both TV rooms, the living room and the master bedroom, as
  K72's "separate-LCR set (three clean speakers with no visible controls; TV inputs on a hidden
  hub)" within P2's settled scope (Option A: stereo LPCM from the TV's optical output, CEC
  through the kernel's CEC API, on a Linux hub). The soundbar variant of K72 is not designed
  (P13, decision 0231).
- The choices and their reasons: the decision record
  `docs/decisions/0232-the-lcr-set-is-three-two-ways-and-a-pi-hub.md`. The devices repository's
  record for the set (`projects/chorus-lcr/v1/`) is made from this file elsewhere.

## What is in one set

| Part | How many | What it is | Where it is designed |
|---|---|---|---|
| Left, centre and right speakers | 3 | `chorus-twoway-v1`, the good (designed) tier, each on 24 V from its own mains adapter and on wired Ethernet; no visible controls (a hidden pairing button and a rear status light, K68) | `docs/hardware/twoway-speaker.md` |
| The TV hub | 1 | a Raspberry Pi 5 with a HiFiBerry Digi+ I/O HAT in a printed case behind the TV, on its 27 W USB-C supply and wired Ethernet, running `chorus-client` | this file, from P4's H1 to H4 and F1 to F3 |

The centre is the same speaker as the left and right, stood or laid as the room allows. A
two-way on its side turns its woofer and tweeter side by side, which changes its horizontal
dispersion near the 2000 Hz crossover; whether the centre is laid down is the room's, and a
centre laid down is **ASSUMED** acceptable until it is measured in place.

The set is not the whole room. A 5.1 TV room is the set plus `chorus-sub-v1`
(`docs/hardware/subwoofer.md`) and two `chorus-compact-v1` surrounds
(`docs/hardware/compact-speaker.md`), as P4's house plan has it.

## The hub's wiring

The bench packet's TV session wires and enables the same hub
(`docs/bench-packet.md`, S8); this section is that wiring as installed.

```
TV optical out --TOSLINK--> Digi+ I/O optical in --I2S (40-pin header)--> Pi 5
TV HDMI in    <--micro-HDMI to HDMI--  Pi 5 HDMI0 (CEC only; no picture is sent)
Pi 5 Ethernet --patch cable--> the room's switch port (the speaker network)
27 W USB-C supply --> Pi 5
```

- **Audio:** the TV's optical output into the Digi+ I/O's optical input. The TV's digital audio
  output is set to PCM (stereo), never Auto, Bitstream or passthrough: the hub refuses a
  compressed stream (`non-pcm`, `docs/linux-endpoint.md`, "A TV input"). Both chorus TVs have an
  optical output (P4's house plan), so neither needs an ARC extractor.
- **CEC:** the Pi's own HDMI port, through the kernel CEC framework's userspace API
  (`/dev/cecN`), driven by chorus's own Rust code (`docs/cec.md`, ADR 0087). The cable is P4's H3.
  It goes into any HDMI input of the TV; the hub never claims ARC (`--cec-arc` stays off: a Pi's
  HDMI port is a source and cannot receive ARC audio, `docs/cec.md`). Which `/dev/cecN` the
  port nearer the USB-C jack is, is **ASSUMED** `/dev/cec0` until the bench session lists them.
  If the TV will not route its remote's volume to a CEC audio system on a plain HDMI input
  (P2's named risk), P4's fallback is a Pulse-Eight USB-CEC adapter; the fallback changes only
  the device given to `--cec`.
- **Network:** wired Ethernet, as every bonded endpoint must be (K91; a set refuses a member
  whose link is not `wired`, `docs/control-plane.md`). The hub's own lip-sync path is wired
  datagrams to the room (`docs/linux-endpoint.md`, the low-latency path).
- **Power:** the official 27 W USB-C supply (P4's F3); the hub draws far less, but the Pi 5
  limits its USB ports on a lesser supply, and the two bench supplies are already bought.

## The channel map of a 5.1 TV room

| Role (`docs/protocol.md`'s channel map) | Endpoint | Its flags beyond `--zone <room>` |
|---|---|---|
| `FL` | two-way, left | `--two-way crossover-hz=2000,woofer=0,tweeter=1` (the two-way's crossover; the outputs are the ASSUMED example until its board plan wires them; on a firmware endpoint, the firmware's equivalent) |
| `FC` | two-way, centre | the same |
| `FR` | two-way, right | the same |
| `LFE` | `chorus-sub-v1` | none: its role is set by the bond; bass management at 80 Hz (decision 0229) |
| `SL`, `SR` | two `chorus-compact-v1` | none |
| (none) | the TV hub | `--line-in <device> --line-in-kind optical --line-in-name <room> TV --cec /dev/cec0` |

How it goes together:

- **Theater bonding (K30).** The room's set is one `bond` message
  (`docs/control-plane.md`): `FL`, `FR`, `FC`, `LFE`, `SL`, `SR`, each an endpoint of the room
  and wired. It is the "theater" layout the control plane accepts (a front three, an optional
  `LFE`, one surround pair). Every member gets the room's whole stream and computes its own feed
  (`docs/dsp.md`, step 8): with the sub in the set the three two-ways play their channels through
  the LR4 high branch at the room's `crossover_hz`, and the sub plays the low branch of the sum
  of the mains plus the stream's LFE.
- **The hub is not a member.** It is the room's TV source, not a speaker: `--line-in ...
  --line-in-kind optical` offers the TV to any room, and `--cec` makes it the TV's Audio System so
  the TV's remote drives the room's volume and the TV's power starts and stops the TV input
  (`docs/cec.md`; K81's autoplay). Its own ALSA output is the Digi+ I/O's optical and coaxial
  outputs, which are left unconnected. An endpoint in no set plays the two-output downmix
  (`docs/dsp.md`, "What step 6 plays"), so the hub sends that to outputs wired to nothing.
- **What a stereo TV plays on 5.1 (P2 Option A).** The TV sends two channels. `FL` and `FR`
  play them; `FC` plays `(FL + FR) / sqrt 2`, the passive matrix's centre; `LFE` plays the
  managed bass; `SL` and `SR` are silent with the room's `tv_upmix` off (the default) or play the
  ambient difference signal with it on (`docs/dsp.md`, "What step 6 plays"). A 5.1 music stream
  from the server plays each role's own channel.
- **Lip sync.** The TV room plays its own TV on the low-latency path at the room's `L_tv`
  (`--tv-latency-ms`, 25 ms by default, `docs/control-plane.md`); sharing the TV input with
  another group moves it to the slot path at that group's latency (`docs/inputs.md`).

## A start for the hub's command line

The flags are those of `docs/linux-endpoint.md`. The HAT is enabled as the bench packet says
(`dtoverlay=hifiberry-digi`, **ASSUMED** there, `docs/bench-packet.md` S8); `<digi card>` is the
card number `arecord -l` gives, and `/dev/cec0` is **ASSUMED** until `ls /dev/cec*` on the
bench.

```
chorus-client --zone living --device hw:<digi card>,0 \
  --line-in hw:<digi card>,0 --line-in-kind optical --line-in-name "Living room TV" \
  --cec /dev/cec0 --cec-osd-name chorus
```

## The bill of materials (priced, US first)

Single-unit list prices in US dollars before shipping and tax. Each line names its seller and
the date its price was read, or says **ASSUMED**. The two-way's cost is the total
`docs/hardware/twoway-speaker.md` states ("The budget", the good tier, every line read
2026-10-06); it is not re-derived here.

### One set

| Line | Qty | What | Seller and ship-from | Each | Line | Source, date read |
|---|---|---|---|---|---|---|
| L1 | 3 | `chorus-twoway-v1`, good tier, all parts | the two-way's budget | 203.92 | 611.76 | `docs/hardware/twoway-speaker.md`, "The budget", read there 2026-10-06 |
| L2 | 1 | Raspberry Pi 5 2GB (the master bedroom's hub; P4's F2) | CanaKit, US **ASSUMED** (P4) | 77.50 | 77.50 | https://www.canakit.com/raspberry-pi-5-2gb.html, read 2026-10-06, "In Stock" |
| L3 | 1 | HiFiBerry Digi+ I/O (P4's H1) | HiFiBerry, Switzerland; **NON-US EXCEPTION** (shipping and duties extra) | 54.90 | 54.90 | https://www.hifiberry.com/shop/boards/hifiberry-digi-io/, read 2026-10-06, "In stock". No US seller found: PiShop.us's search for it lists no such product (https://www.pishop.us/search.php?search_query=digi%2B+i%2Fo, read 2026-10-06; its old product URL returns 404) |
| L4 | 1 | Raspberry Pi 27 W USB-C supply (P4's F3) | CanaKit, US **ASSUMED** | 12.95 | 12.95 | https://www.canakit.com/official-raspberry-pi-5-power-supply-27w-usb-c.html, read 2026-10-06 |
| L5 | 1 | TOSLINK cable, 6 ft (P4's H2) | Parts Express, Springboro, Ohio | 3.29 | 3.29 | https://www.parts-express.com/Toslink-Digital-Optical-Audio-Cable-6-ft.-240-1062, read 2026-10-04 (P4's $6.58 for two); the page gave an automated read nothing on 2026-10-06 |
| L6 | 1 | Micro-HDMI to standard HDMI cable, 2 m (CEC; P4's H3) | PiShop.us, US | 7.95 | 7.95 | https://www.pishop.us/product/micro-hdmi-to-standard-hdmi-a-m-2m-cable-black/, read 2026-10-06, "IN STOCK" |
| L7 | 1 | Hub case, printed (P4's H4), allocation: 0.08 kg (**ASSUMED**) of a 1 kg ASA spool at $24.99 | Polymaker, US **ASSUMED** | 2.00 | 2.00 | https://shop.polymaker.com/products/asa.js, read 2026-10-06 (the two-way's budget's filament line) |
| L8 | 1 | microSD card | owned (P4: "Owned, not bought: microSD cards") | 0.00 | 0.00 | P4, 2026-10-04 |
| | | **The hub (L2 to L8)** | | | **158.59** | |
| | | **Total, one set (the 2GB hub)** | | | **770.35** | |

### Both TV rooms

| What | Sum |
|---|---|
| Master bedroom set (above, the Pi 5 2GB, F2) | 770.35 |
| Living room set: the same with the Pi 5 4GB (F1), $110.00 at CanaKit (https://www.canakit.com/raspberry-pi-5-4gb.html, read 2026-10-06, "In Stock") in place of L2 | 802.85 |
| **Two-room total** | **1573.20** |

That is six two-ways (1223.52) and two hubs (349.68).

What the totals do and do not say:

- **The hubs' Pis and supplies are already on the bench list.** F1, F2 and F3 are in P4's
  "Buy now" and become the hubs at install, so what the two sets add to money already spent is
  the six two-ways plus P4's H1 to H4: per set 68.14 (L3, L5, L6, L7), both rooms 1359.80.
  L3, L5 and L6 for two hubs are 132.28, P4's H1 to H3 total; the cases add 4.00, where P4
  prices them at none.
- **The two-way's total carries its own caveats** (its budget's notes): its endpoint board line
  is the Esparagus Audio Brick at $59.00, which its US seller listed as "No longer available"
  on the day read; the compact moved to separate modules (decision 0230) and the two-way's board
  plan has not yet. A change there changes L1 and both totals, by three and by six times the
  board's difference.
- **One NON-US EXCEPTION line: L3,** the Digi+ I/O, bought from HiFiBerry in Switzerland with
  shipping and duties on top, as P4's H1 already grants.
- **Not priced:** the hub's Ethernet patch cable and the room's cable run (P4 leaves the cable
  pull and network gear out), the case's screws and standoffs, shipping and tax, and the
  two-way's own unpriced items.

## What the devices repository needs for the hub's case

A request, not a model: the devices repository makes the case. Every size is **ASSUMED** until
the parts are on the bench and measured with calipers.

- **What it holds:** a Raspberry Pi 5 with the Digi+ I/O on its 40-pin header. The Pi 5 board
  is 85 x 56 mm (**ASSUMED**: Raspberry Pi's published figure; the product brief read
  2026-10-06 gives no dimension in its text). HiFiBerry gives the Digi+ I/O as "5.5 × 6.5 × 1.5
  cm" (read 2026-10-06). The stack's height over the Pi's board, with the HAT on standoffs and
  its TOSLINK jack, is **ASSUMED** 30 mm.
- **Size:** inside about 92 x 62 x 36 mm, outside about 98 x 68 x 42 mm at 3 mm walls
  (**ASSUMED**: the board sizes plus 3 to 4 mm clearance each side; arithmetic).
- **Port cut-outs:**
  - the Pi's USB-C power jack and its first micro-HDMI port (CEC);
  - the Pi's RJ45 Ethernet jack;
  - the Digi+ I/O's optical input, sized for the TOSLINK plug's body and its dust-cap removed;
  - the Pi's microSD slot, reachable without opening the case;
  - the Pi's power button, recessed so it cannot be pressed by the TV's back;
  - the Pi's status LEDs visible through a slot or light pipe (optional);
  - the Digi+ I/O's other jacks (coaxial in and out, optical out) and the Pi's four USB ports
    and second micro-HDMI port need no cut-out; leaving the USB block open is an acceptable
    vent.
- **Vents:** Raspberry Pi says the Pi 5 "will perform best with active cooling" and gives an
  operating range of 0 to 70 °C (product brief RP-008348-DS, read 2026-10-06). The hub's load
  is a stereo capture, a resampler and a CEC thread, so the case is **ASSUMED** passive: slots
  in the floor and the lid over the SoC, giving a convection path that does not depend on
  which way the case is mounted. The 0 to 70 °C range is the board's surroundings, so the bench
  session records the air inside the closed case behind a running TV against it, and the SoC
  (`vcgencmd measure_temp`) for throttling (**ASSUMED** to begin near 80 °C, not read); if
  either is reached the case takes Raspberry Pi's Active Cooler and a lid opening for its fan.
- **Mounting behind the TV:** two keyhole slots on the back face for screws into a printed plate
  held by the TV's VESA screws, or a flat back for adhesive mounting tape (**ASSUMED**; the TV
  models and their VESA patterns are the owner's TV Needs item). The cut-outs face down or to
  the side so cables do not bend against the wall. Nothing on the case shows from the front of
  the TV.
- **Material (P12):** P12's printed material is ASA ("Compact: printed ASA", and the two-way's
  printed fittings), with PETG the indoor fallback; P12 does not use PLA next to a heat source
  (heat deflection 55 °C). P4's H4 says "printed from the owner's PLA". This file asks for ASA,
  or PETG if ASA is not yet proven on the owner's printer: a case behind a TV next to a Pi 5 is
  the warm place P12's argument is about. The case is small enough that PLA would still serve
  as a first print to check the fit.

## Within P2's settled scope

- **Stereo LPCM from optical only:** no eARC, no ARC audio, no multichannel capture. The TV's
  5.1 content is downmixed by the TV; the room's centre and surrounds come from the rules in
  "What a stereo TV plays on 5.1".
- **No AC-3 decode:** a compressed stream is refused (`non-pcm`), never decoded; the TV is set
  to PCM.
- **No soundbar:** K72's other variant is not designed (P13, decision 0231). A TV room without
  separate L, C and R would revive it.

## Open items

- The Digi+ I/O's capture on a Pi 5 (P2: HiFiBerry's "limitations" page for recording was not
  readable), its ALSA name and overlay, and the CEC device number: the bench session (S8).
- Whether each TV routes its remote's volume over CEC to a hub on a non-ARC input (P2's risk).
- The SoC temperature in the case behind a TV, and so whether the case needs a fan.
- The case's measured sizes, and the TVs' VESA patterns.
- Whether the centre is laid on its side, and how it measures there.

## Sources

- `docs/hardware/twoway-speaker.md` (the two-way, its budget), `docs/hardware/subwoofer.md`,
  `docs/hardware/compact-speaker.md`, `docs/proposals/P2-theater-scope.md`,
  `docs/proposals/P4-bench-purchase.md`, `docs/proposals/P12-enclosures.md`,
  `docs/proposals/P13-rack-amp-zones.md`, `docs/linux-endpoint.md`, `docs/cec.md`,
  `docs/dsp.md`, `docs/control-plane.md`, `docs/inputs.md`, `docs/bench-packet.md` (S8), at `origin/main` 9144a4f, read
  2026-10-06.
- The sellers' pages in the bill of materials, at the dates in their rows.
- Raspberry Pi 5 product page, https://www.raspberrypi.com/products/raspberry-pi-5/, and product
  brief RP-008348-DS (https://datasheets.raspberrypi.com/rpi5/raspberry-pi-5-product-brief.pdf,
  redirected to pip-assets.raspberrypi.com), both read 2026-10-06.
- HiFiBerry Digi+ I/O, https://www.hifiberry.com/shop/boards/hifiberry-digi-io/, read
  2026-10-06 (product documentation only).
