# P4: The bench purchase packet

- Decisions: K38
- Status: PROPOSED (chorus goal 1, 2026-09-30); decided at Checkpoint K
- If deferred: The recommended tier is filed as the Needs packet, marked "awaiting choice"
- Builds on: goal 7 (§11 item 5: "Bench packet (P4 as settled): the buy list, wiring, the exact commands for SOUND-2, RIG-3 and SYNC-4"), goal 8 and goal 9 (§12, §13: the P1 board and the EMBEDDED-5 packet), goal 10 (§14: the Linux tier), goal 24 (§28: the compact speaker's PoE+ power path)

## Question

What should the owner buy for the bench, in tiers, priced with URLs and dates, US-first by
ship-from, consistent with P1 and with what later bench packets need? The owner's decisions:

- K38: "No cap; propose: the program writes the bench buy packet it judges right (with priced
  alternatives, US-first by ship-from per devices' rule, any non-US line flagged as an exception),
  and the owner picks at Checkpoint K; the program never orders."
- K4: "never flash, order, spend". This file is a list for the owner, nothing more.
- K21 and K36: the owner has "ESP32-S3 boards only (model, PSRAM, Ethernet not given)", a soldering
  station, a multimeter and a woodworking shop; "No oscilloscope, no measurement mic, no logic
  analyzer, no audio interface", no amp or DAC module, no Linux board. "Build packets assume only
  these; anything else is a buy-list line."
- K96: Linux endpoints are a product tier where they fit (rack amp, theater hub).
- K90: the compact speaker is PoE+; the Omada ES228GP is staged, not live (brief §2).

## Constraints that bind every option

- **The phases the bench serves** (BRIEF §8, §10): SOUND-2 (first sound on a Linux client), RIG-3
  (dual-input capture and cross-correlation: "two endpoints' line outputs into the L/R of one USB
  audio interface", about 10 us resolution at 96 kHz; a GPIO marker read by a logic analyzer as
  the digital cross-check), SYNC-4 (two wired Linux clients, sub-millisecond), EMBEDDED-5 (the P1
  board, TAS alive, synced against a Linux client), WIFI-7 (one wireless endpoint).
- **P1 (as recommended):** ESP32-S3 everywhere, W5500 wired with INT, PoE+ through a bought
  802.3at splitter, native Wi-Fi. The bench must include an S3 + TAS58xx + W5500 board (TAS
  register bring-up, goal 9) and an S3 + W5500 pair it can measure at line level. P1's named
  escalation (a P4 wired tier) needs one P4X board, placed in the full tier.
- **US-first by ship-from** (§0.13); non-US lines are flagged **NON-US EXCEPTION**. Where the page
  did not show ship-from, it says so.
- **Rule 8:** the owner's own S3 boards are used where they fit (WIFI-7 on the native radio), but
  nothing is chosen because they exist. Their model is a goal-1 Needs answer.
- **Prices:** "read" means the seller's page was opened today; "snippet" means a search result
  summary only (the page was blocked or not rendered); `ASSUMED` means an estimate. Confirm every
  snippet and `ASSUMED` price in a browser before buying.

## Re-verification of the planning research

The planning research (`research-platform-network.md` §6, `research-endpoint-hardware.md` §4 and
§8) proposed Tier 1 (two Pi 5 + RPi DAC+, UMC202HD, fx2lafw analyzer, about $400) plus Tier 2 (a P4
board, a PCM5102A, one Waveshare S3-ETH, a PoE injector, about $90-125), recommending about $525.
Re-checked today:

| Item | Re-check (2026-09-30) | Change |
|---|---|---|
| Raspberry Pi 5 2GB, PiShop.us | $65.00; "Maximum Purchase: 1 unit" | Confirmed; two units means two orders |
| Raspberry Pi DAC+ (PCM5122) | $29.95, "IN STOCK" (URL moved to `raspberry-pi-dac-green-pcb`) | Confirmed |
| Raspberry Pi DigiAMP+ (TAS5756M, 12-24 V) | $42.95, "IN STOCK" | Confirmed |
| Pi 27 W USB-C PSU | $12.95, "IN STOCK" (the planning research estimated $25 for PSU, SD and case) | Priced |
| Pi SD card 32 GB | $19.95 (stock not shown) | Priced |
| Pi I2S limits | Raspberry Pi whitepaper RP-009699-WP-1 (build 20/02/2026): MCLK "NOT supported on any Raspberry Pi SBCs"; "TDM is NOT supported on any Raspberry Pi SBCs" | Confirmed. Pi DAC HATs need no MCLK (PCM5122 PLL from BCK, per the planning research); multichannel on a Pi means parallel lanes (goal 10's design question, not a bench purchase now) |
| SparkFun USB Logic Analyzer 24 MHz 8 ch (TOL-18627) | $26.95, "In stock" | Confirmed |
| Behringer UMC202HD | Sweetwater $86.90, in stock (snippet; the planning research had $89 snippet); class-compliant UAC2 per `verify-theater-platform.md` #11a | Price updated (snippet) |
| P4 board for the bench | P1 now recommends the S3 with the P4 as a named escalation, so the P4 moves from the recommended tier to the full tier. ESP32-P4X-Function-EV-Board: Mouser $59.74, DigiKey $59.92 (snippets) | **Changed** (follows P1) |
| TAS58xx board | Tindie Louder-ESP32 "Sold out since Sep 24, 2026"; Louder-ESP32-Plus "Sold out since Sep 08, 2026"; **Crowd Supply Esparagus Audio Brick (ESP32-S3)**: TAS5825M, W5500, 5-26 V, $59, "$8 US Shipping", "Sold and shipped by Crowd Supply", "Orders placed now ship Jan 25, 2027"; TI TAS5825MEVM "Out of stock on TI.com" and needs the PUREPATH-CMBEVM motherboard | **Changed**: a Crowd Supply S3 + TAS5825M + W5500 board replaces the planning research's "no US source, owner exception" for the TAS line (pre-order) |
| PCM5102A DAC | Adafruit PCM5102 I2S DAC (6250): $4.95, "OutOfStock"; "does not need any MCLK" | Priced; out of stock, alternative named |
| PoE+ injector | TP-Link TL-POE160S (802.3at/af, 30 W): B&H $22.99, CDW $22.00 (snippets) | Priced (snippet) |
| PoE+ splitter to 24 V | PoE Texas GAT-24V25W, "24 Volt 25 Watt PoE or DC Output", $31.99 (its own Shopify store) | Priced (read) |
| Waveshare ESP32-S3-ETH | waveshare.com $16.99-25.99 ("Optional for PoE module ... IEEE 802.3af-compliant"); Amazon US listings $25.99-34.99 (snippet; ship-from not visible) | Confirmed; ship-from UNVERIFIED |
| Adafruit ESP32-S3-DevKitC-1-N8R8 | $19.95, InStock | New alternative (no Ethernet) |

Adversarially verified 2026-09-30 (goal-1 verifier 1): 8 claims confirmed, 0 refuted, 2 partly right, 3 unverifiable; corrections applied; the recommendation stands.

## Options

Every line: part, why, qty, unit price, seller, ship-from, URL, date read (all 2026-09-30).

### Option A: Minimal tier (SOUND-2, RIG-3, SYNC-4 on Linux; WIFI-7 on the owner's S3)

| # | Part | Why | Qty | Unit | Line | Seller, ship-from | URL | Price basis |
|---|---|---|---|---|---|---|---|---|
| A1 | Raspberry Pi 5 2GB | the two wired Linux clients of SOUND-2 and SYNC-4 (BRIEF §5.4: develop on Linux first) | 2 | $65.00 | $130.00 | PiShop.us, US (New Castle, DE, per its support page cited in `research-endpoint-hardware.md` §3.1) | https://www.pishop.us/product/raspberry-pi-5-2gb/ | read; 1 per order |
| A2 | Raspberry Pi DAC+ (PCM5122, line out on RCA) | the line outputs RIG-3 captures; Pi-side clock, one crystal per endpoint | 2 | $29.95 | $59.90 | PiShop.us, US | https://www.pishop.us/product/raspberry-pi-dac-green-pcb/ | read, in stock |
| A3 | Raspberry Pi 27 W USB-C PSU (US) | power for A1 | 2 | $12.95 | $25.90 | PiShop.us, US | https://www.pishop.us/product/raspberry-pi-27w-usb-c-power-supply-black-us/ | read, in stock |
| A4 | Raspberry Pi SD card 32 GB | OS for A1 | 2 | $19.95 | $39.90 | PiShop.us, US | https://www.pishop.us/product/raspberry-pi-sd-card-32gb/ | read; stock not shown |
| A5 | Behringer UMC202HD (2-in, 192 kHz, UAC2) | RIG-3's dual-input capture: two line outputs into L and R | 1 | $86.90 | $86.90 | Sweetwater, US | https://www.sweetwater.com/store/detail/UMC202HD--behringer-u-phoria-umc202hd-usb-audio-interface | snippet |
| A6 | SparkFun USB Logic Analyzer 24 MHz 8 ch (TOL-18627), sigrok fx2lafw | RIG-3's digital cross-check: GPIO marker edges | 1 | $26.95 | $26.95 | SparkFun, US | https://www.sparkfun.com/usb-logic-analyzer-24mhz-8-channel.html | read, in stock |
| A7 | Adafruit premium male/male jumper wires, 20 x 3" | marker GPIO to the analyzer | 2 | $1.95 | $3.90 | Adafruit, US | https://www.adafruit.com/product/1956 | read, in stock |
| A8 | Audio cables: 2 x RCA to 1/4" TS, 1 x RCA Y-splitter (rig self-calibration: one source into both inputs) | A2 into A5 | 1 set | $30.00 | $30.00 | any US seller | (not priced per item) | `ASSUMED` |
| | **Minimal total** | | | | **$403.45** | | | |

WIFI-7 uses one of the owner's S3 boards on its native radio (no purchase; model per the goal-1
Needs item). The house LAN serves the bench; no switch is bought.

### Option B: Recommended tier (Minimal plus the P1 embedded bench: EMBEDDED-5 and the PoE+ path)

Everything in A, plus:

| # | Part | Why | Qty | Unit | Line | Seller, ship-from | URL | Price basis |
|---|---|---|---|---|---|---|---|---|
| B1 | Esparagus Audio Brick (ESP32-S3): S3, TAS5825M, W5500 on board, 5-26 V, Apache-2.0 design | P1's bought reference board: TAS5825M register bring-up (goal 9 replaces the 8 `unknown` keys), EMBEDDED-5 sync on the W5500 path; a stereo pair | 2 | $59.00 | $118.00 | Crowd Supply, "Sold and shipped by Crowd Supply", "$8 US Shipping"; fulfilment by Mouser; produced in Poland. Ships from Mouser, Mansfield, TX, USA (Crowd Supply ordering guide, read 2026-09-30) | https://www.crowdsupply.com/sonocotta/esparagus-audio-brick | read; **pre-order, estimated ship 2027-01-25 (creator's estimate; may slip)** |
| B1 alt | Esparagus Audio Brick, "Single DAC, ESP32-S3" (the same board as B1) | an alternative to B1 that would let the TAS register bring-up start before 2027-01-25, if the S3 variant is in stock | (2) | $59.00 | (alternative, not in the totals) | Elecrow, "Sold By Sonocotta Poland"; **NON-US EXCEPTION** (ship-from not stated, China `ASSUMED`) | https://www.elecrow.com/esparagus-audio-brick.html | read; page marked "In stock" (per-variant stock not visible); shipping not priced |
| B2 | Crowd Supply US shipping for B1 | | 1 | $8.00 | $8.00 | Crowd Supply | same | read |
| B3 | Waveshare ESP32-S3-ETH with PoE module (W5500, INT on GPIO10, 802.3af module) | measures the S3 + W5500 sync path at line level now, without waiting for B1; GPIO headers for the marker | 2 | $34.99 | $69.98 | Amazon US listing; **ship-from UNVERIFIED** (if it ships from China, a NON-US EXCEPTION; the maker's own store ships from China, $25.99 with PoE module) | https://www.amazon.com/waveshare-ETH-Development-ESP32-Module/dp/B0DLK8QMFK | snippet (upper bound of $25.99-34.99) |
| B4 | Adafruit PCM5102 I2S DAC, line out, no MCLK | line output for B3 into the rig | 2 | $4.95 | $9.90 | Adafruit, US | https://www.adafruit.com/product/6250 | read; **out of stock** (alternative: a PCM5102A 2-pack on Amazon US, $10.91, snippet from the planning research) |
| B5 | TP-Link TL-POE160S PoE+ injector (802.3at/af, 30 W) | powers the compact-speaker path on the bench until the ES228GP is live | 1 | $22.99 | $22.99 | B&H Photo, US | https://www.bhphotovideo.com/c/product/1633607-REG/tp_link_tl_poe160s_poe_injector_black.html | snippet |
| B6 | PoE Texas GAT-24V25W splitter (802.3at in, 24 V 25 W out, gigabit pass-through) | the compact speaker's power path (K90): PoE+ into a B1's 5-26 V input | 1 | $31.99 | $31.99 | PoE Texas, Austin, TX, USA (store contact page; ship-from inferred from the seller's address) | https://shop.poetexas.com/products/gat-24v25w | read |
| B7 | 24 V DC supply for the second B1 (the mains classes' path) | | 1 | $25.00 | $25.00 | any US seller | (connector per the Brick's docs) | `ASSUMED` |
| B8 | Passive 8 ohm bookshelf speaker pair (e.g. Dayton Audio B652-AIR) | hear the TAS5825M; the owner owns no test speakers per K21 | 1 pair | $70.00 | $70.00 | Parts Express, US (`ASSUMED`) | https://www.parts-express.com/Dayton-Audio-B652-AIR-6-1-2-2-Way-Bookshelf-Speaker-with-AMT-Tweeter-Pair-300-651 | `ASSUMED` (page did not render) |
| B9 | Dummy load and attenuator parts: 2 x 8 ohm 50 W resistors, divider resistors | the TAS output is bridge-tied speaker level; the rig's line inputs need it loaded and divided (the wiring goes in goal 7's packet) | 1 set | $20.00 | $20.00 | any US seller | (not priced per item) | `ASSUMED` |
| B10 | Cat6 patch cables | B1, B3, B5, B6 | 4 | $3.00 | $12.00 | any US seller | (not priced per item) | `ASSUMED` |
| | **Additions** | | | | **$387.86** | | | |
| | **Recommended total (A + B)** | | | | **$791.31** | | | |

### Option C: Full tier (Recommended plus P1's escalation board, a third room and an amplified Linux endpoint)

Everything in A and B, plus:

| # | Part | Why | Qty | Unit | Line | Seller, ship-from | URL | Price basis |
|---|---|---|---|---|---|---|---|---|
| C1 | ESP32-P4X-Function-EV-Board (v3.x "X" silicon, RMII Ethernet, ES8311) | P1's named escalation: APLL, EMAC time stamps and PPS against the S3 + W5500 path | 1 | $59.74 | $59.74 | Mouser, US | https://www.mouser.com/ProductDetail/Espressif-Systems/ESP32-P4X-Function-EV-Board?qs=Naspt24KZtm%2F%2FovBNrcBgA%3D%3D | snippet (DigiKey $59.92) |
| C2 | A third Linux client: Pi 5 2GB, DAC+, 27 W PSU, SD card | a three-room group on real hardware (K75's scale in miniature) | 1 set | $127.85 | $127.85 | PiShop.us, US | the A1-A4 URLs | read |
| C3 | Raspberry Pi DigiAMP+ (TAS5756M, 12-24 V in, powers the Pi) | an amplified Linux endpoint: the streaming-amp and rack-amp class (K70, K96) heard, not only simulated | 1 | $42.95 | $42.95 | PiShop.us, US | https://www.pishop.us/product/raspberry-pi-digiamp-green-pcb/ | read, in stock |
| C4 | Second PoE Texas GAT-24V25W | both B1 boards on PoE+ as a compact stereo pair | 1 | $31.99 | $31.99 | PoE Texas, Austin, TX, USA (store contact page; ship-from inferred from the seller's address) | https://shop.poetexas.com/products/gat-24v25w | read |
| C5 | DreamSourceLab DSLogic Plus (buffered, 10 ns, sigrok) | a buffered analyzer if the fx2lafw drops samples | 1 | $149.00 | $149.00 | DreamSourceLab; **NON-US EXCEPTION** (ship-from not stated, China `ASSUMED`) | https://www.dreamsourcelab.com/shop/logic-analyzer/dslogic-plus/ | planning research, read 2026-09-29, not re-read |
| | **Additions** | | | | **$411.53** | | | |
| | **Full total (A + B + C)** | | | | **$1,202.84** (without the C5 exception: $1,053.84) | | | |

Not in any tier, with reasons: an oscilloscope (the rig and the analyzer cover BRIEF §10's
methods); a measurement mic (K87: room correction uses the phone); a Pi PoE HAT (Pi 5's official
PoE+ HAT is unreleased per the planning research, and the bench runs Pis on USB-C); a
TDM-capable Linux board (goal 10 works on fakes; P2 and goal 26 settle multichannel hardware);
the TI TAS5825MEVM (out of stock on ti.com and needs a second motherboard; B1 does the same job
as a product-grade board); a Scarlett 2i2 (the UMC202HD is enough at 96 kHz).

## Comparison

| Criterion | A: Minimal | B: Recommended | C: Full |
|---|---|---|---|
| Total | $403.45 | $791.31 | $1,202.84 ($1,053.84 without C5) |
| Phases it can run | SOUND-2, RIG-3, SYNC-4, WIFI-7 | plus EMBEDDED-5 (line level now; TAS from 2027-01-25) and the PoE+ path | plus the P4 comparison, a three-room group, an amplified Linux endpoint |
| P1 consistency | S3 only via owned boards | the P1 board and link, measured | plus P1's escalation measured |
| Non-US lines | none | B3 ship-from unverified; the B1 alternative (Elecrow) flagged, not in the total | B3, the B1 alternative, and C5 flagged |
| Snippet or `ASSUMED` share | $116.90 of $403.45 | $336.87 of $791.31 | $545.61 of $1,202.84 |
| Waits on a pre-order | no | B1 (2027-01-25) | B1 |

## Recommendation

**Recommendation:** Option B, the recommended tier at $791.31, because it is the smallest set that runs every bench phase through EMBEDDED-5 on the platform and link P1 recommends, including the compact speaker's PoE+ power path, with every priced line from a US seller except one Amazon listing whose ship-from must be checked.

Why: Minimal proves Linux sync but leaves EMBEDDED-5 and the TAS register map unmeasured, which is
the half of P1 that most needs data. B adds two S3 + W5500 boards measurable at line level now, the
bought S3 + TAS5825M + W5500 reference board (pre-order), and the PoE+ injector and splitter. Full
adds the P4 comparison, which matters only if B's measurements put the W5500 path outside the
0.5 ms stereo-pair bound. What the owner gives up by not buying Full: the P4 cross-check up front
(it can be bought later, when EMBEDDED-5 asks for it).

## If the owner defers

The recommended tier (Option B) is filed as the Needs packet, marked "awaiting choice"; goal 7's
bench packet, wiring and commands are written against it. Nothing is ordered (K4). The cost: bench
sessions cannot start until the owner buys, and B1 is a pre-order shipping 2027-01-25, so the TAS
register bring-up (goal 9's packet) waits for it either way.

## Open inputs

- The owner's ESP32-S3 boards' module marking and `esptool.py chip_id` (goal-1 Needs item): decides
  whether one fits WIFI-7 (needs PSRAM, `ASSUMED`).
- Whether the ES228GP is live when the owner buys; if so, B5 is not needed.
- B1's power connector: the campaign page lists a 4-pin snap-in connector, and the 2026-08-06
  production update changed it to a 2-pin version. B6's output is a 2.1 x 5.5 mm plug, so add a
  line for a 2.1 mm plug-to-bare-wire lead (`ASSUMED` about $5, any US seller; not yet in the tier
  totals). Whether a free GPIO is reachable for the marker is still not stated; if not, B3 carries
  the marker work.
- Whether B1 wires the W5500 INT pin to a GPIO: unconfirmed (see P1); B3's INT on GPIO10 is
  confirmed, so B3 carries the INT-mode measurement either way.
- B3's ship-from (UNVERIFIED), and the B1 alternative's (Elecrow, not stated): checked at checkout
  by the owner.
- Every `ASSUMED` and snippet price: A5, A8, B3, B5, B7-B10, C1, C5.
- Whether the owner already has passive speakers, cables or a spare 24 V supply (then B7, B8, A8
  drop).

## Sources

- PiShop.us, Raspberry Pi 5 2GB: https://www.pishop.us/product/raspberry-pi-5-2gb/, read 2026-09-30
- PiShop.us, Raspberry Pi DAC+: https://www.pishop.us/product/raspberry-pi-dac-green-pcb/, read 2026-09-30
- PiShop.us, Raspberry Pi DigiAMP+: https://www.pishop.us/product/raspberry-pi-digiamp-green-pcb/, read 2026-09-30
- PiShop.us, 27 W USB-C PSU: https://www.pishop.us/product/raspberry-pi-27w-usb-c-power-supply-black-us/, read 2026-09-30
- PiShop.us, Raspberry Pi SD card 32 GB: https://www.pishop.us/product/raspberry-pi-sd-card-32gb/, read 2026-09-30
- Raspberry Pi, "Using the I2S peripherals on Raspberry Pi SBCs" (RP-009699-WP-1): https://pip-assets.raspberrypi.com/categories/1259-audio-camera-and-display/documents/RP-009699-WP-1-Using%20the%20I2S%20peripherals%20on%20Raspberry%20Pi%20SBCs.pdf, read 2026-09-30
- SparkFun USB Logic Analyzer 24 MHz: https://www.sparkfun.com/usb-logic-analyzer-24mhz-8-channel.html, read 2026-09-30
- Adafruit jumper wires 1956: https://www.adafruit.com/product/1956, read 2026-09-30
- Adafruit PCM5102 I2S DAC 6250: https://www.adafruit.com/product/6250, read 2026-09-30
- Adafruit ESP32-S3-DevKitC-1-N8R8 5336: https://www.adafruit.com/product/5336, read 2026-09-30
- Sweetwater, Behringer UMC202HD (search snippet): https://www.sweetwater.com/store/detail/UMC202HD--behringer-u-phoria-umc202hd-usb-audio-interface, read 2026-09-30
- Crowd Supply, Esparagus Audio Brick: https://www.crowdsupply.com/sonocotta/esparagus-audio-brick, read 2026-09-30
- Crowd Supply, "Ordering, Paying, Shipping: All the Details" (all orders ship from Mouser, Mansfield, TX, USA; pre-order dates are the creator's best estimate): https://www.crowdsupply.com/guide/ordering-paying-shipping-details, read 2026-09-30
- Crowd Supply, Esparagus Audio Brick production update of 2026-08-06 ("I replaced power connector with a 2-pin version"): https://www.crowdsupply.com/sonocotta/esparagus-audio-brick/updates/production-progress, read 2026-09-30
- Elecrow, Esparagus Audio Brick ("Single DAC, ESP32-S3", $59, "In stock", "Sold By Sonocotta Poland"): https://www.elecrow.com/esparagus-audio-brick.html, read 2026-09-30
- Tindie, Louder-ESP32 and Louder-ESP32-Plus: https://www.tindie.com/products/sonocotta/louder-esp32/ and https://www.tindie.com/products/sonocotta/louder-esp32-plus/, read 2026-09-30
- Lectronz, Sonocotta store: https://lectronz.com/stores/sonocotta, read 2026-09-30
- TI TAS5825MEVM: https://www.ti.com/tool/TAS5825MEVM, read 2026-09-30
- Waveshare ESP32-S3-ETH: https://www.waveshare.com/esp32-s3-eth.htm, read 2026-09-30
- Amazon, Waveshare ESP32-S3 PoE ETH (search snippet): https://www.amazon.com/waveshare-ETH-Development-ESP32-Module/dp/B0DLK8QMFK, read 2026-09-30
- B&H, TP-Link TL-POE160S (search snippet): https://www.bhphotovideo.com/c/product/1633607-REG/tp_link_tl_poe160s_poe_injector_black.html, read 2026-09-30
- PoE Texas GAT-24V25W: https://shop.poetexas.com/products/gat-24v25w, read 2026-09-30
- PoE Texas contact page ("11821 Buckner Road, Austin, TX 78726"): https://shop.poetexas.com/pages/contact, read 2026-09-30
- Mouser and DigiKey, ESP32-P4X-Function-EV-Board (search snippets): https://www.mouser.com/ProductDetail/Espressif-Systems/ESP32-P4X-Function-EV-Board?qs=Naspt24KZtm%2F%2FovBNrcBgA%3D%3D and https://www.digikey.com/en/products/detail/espressif-systems/ESP32-P4X-FUNCTION-EV-BOARD/29196984, read 2026-09-30
- Parts Express, Dayton Audio B652-AIR (page did not render; price `ASSUMED`): https://www.parts-express.com/Dayton-Audio-B652-AIR-6-1-2-2-Way-Bookshelf-Speaker-with-AMT-Tweeter-Pair-300-651, tried 2026-09-30
- DreamSourceLab DSLogic Plus (from the planning research, read 2026-09-29): https://www.dreamsourcelab.com/shop/logic-analyzer/dslogic-plus/
- P1 (this goal): `docs/proposals/P1-embedded-platform.md`

## What was read

Local: `/cache/tmp/chorus-g1/agent-rules.md`, `/cache/tmp/chorus-g1/proposal-format.md`;
[`.claude/goals/2026-09-chorus.md`](https://github.com/NSchatz/chorus/blob/535ed2816b5735153ac81c52a235024aa806f2b5/.claude/goals/2026-09-chorus.md) (header, §0.6-§0.13, §1, §2, §5, §10-§14, §28-§30);
`research-endpoint-hardware.md`, `research-platform-network.md`, `verify-theater-platform.md`
(whole); baseline `BRIEF.md` §2.2, §5.3-§5.5, §6, §8, §10.

Web (all 2026-09-30): every URL in Sources, plus web searches for UMC202HD, TL-POE160S, P4X EV
board, PoE+ splitters, PoE Texas, B652-AIR and Waveshare S3-ETH pricing. No GPL source file and no
CERN-OHL-S or GPL hardware design file was opened.
