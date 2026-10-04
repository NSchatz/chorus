# P4: The bench purchase packet

- Decisions: K38
- Status: PROPOSED (chorus goal 1, 2026-09-30); decided at Checkpoint K; every line re-evaluated from the day's offers on 2026-10-04, at the owner's request, for the owner's choice of tier
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
- The owner, 2026-10-04, asked which tier to buy: "The bench list needs to be extensively
  re-evaluated. All options need to be researched and reviewed to ensure best bang for our buck.
  Everything needs to be searched across the web for current information. I will buy products
  from Ali-express as needed to save money". So a non-US seller is a pick where it is the best
  value; it stays flagged as an exception, as K38 asks.

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
  did not show ship-from, it says so. Since the owner's statement of 2026-10-04 a non-US offer
  wins a line when it is the better value, and the US offer is named beside it.
- **Rule 8:** the owner's own S3 boards are used where they fit (WIFI-7 on the native radio), but
  nothing is chosen because they exist. Their model is a goal-1 Needs answer.
- **Prices:** "read" means the seller's page was opened that day; "snippet" means a search result
  summary only (the page was blocked or not rendered); `ASSUMED` means an estimate. Confirm every
  snippet and `ASSUMED` price in a browser before buying.

## Re-verification of the planning research (2026-09-30)

Kept as the record of the first reading; the offers of 2026-10-04 are in the next section.

The planning research (`research-platform-network.md` §6, `research-endpoint-hardware.md` §4 and
§8) proposed Tier 1 (two Pi 5 + RPi DAC+, UMC202HD, fx2lafw analyzer, about $400) plus Tier 2 (a P4
board, a PCM5102A, one Waveshare S3-ETH, a PoE injector, about $90-125), recommending about $525.
Re-checked on 2026-09-30:

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

## Re-evaluation of 2026-10-04: the offers per line

Every line was searched again on 2026-10-04. Each row is one offer: seller, ship-from, price,
URL and how it was read. All rows were read 2026-10-04. The price-basis words:

- **read**: the seller's page was opened and the price quoted from it.
- **search page**: an AliExpress search-results page was read. It shows a listing's title, its
  lowest-variant or promotional price and its URL, but not the store name, the ship-from or the
  shipping cost; the item pages themselves redirect to a login and could not be read. Ship-from
  is China `ASSUMED` for every such row, and the price is confirmed at checkout.
- **snippet**: a web search summary only.
- **not read**: the page refused the read (HTTP 403, a CAPTCHA or a script-only page); it says so.

### Minimal tier lines (A1 to A8)

| Line | Offer | Seller, ship-from | Price | URL | Basis |
|---|---|---|---|---|---|
| A1 | Raspberry Pi 5 2GB | CanaKit, ship-from not shown on the page (a North American seller; US `ASSUMED`) | US$77.50, "In Stock", no purchase limit shown | https://www.canakit.com/raspberry-pi-5-2gb.html | read |
| A1 | Raspberry Pi 5 2GB | PiShop.us, US | US$77.50, "Maximum Purchase: 1 unit", stock not shown | https://www.pishop.us/product/raspberry-pi-5-2gb/ | read |
| A1 | Raspberry Pi 5 2GB | SparkFun, US | US$70.00, "Out of stock" | https://www.sparkfun.com/raspberry-pi-5-2gb.html | read |
| A1 | Raspberry Pi 5 2GB | Adafruit, US | US$90.00, "Out of stock" | https://www.adafruit.com/product/6007 | read |
| A1 | Raspberry Pi 5 2GB kits | AliExpress listings, China `ASSUMED` | US$88.45 to US$144.75 (kits, dearer than the US board price) | https://www.aliexpress.com/item/1005007022197865.html | snippet |
| A1 | Raspberry Pi 5 1GB (the cheaper equivalent) | PiShop.us, US | US$45.00, maximum purchase 50 | https://www.pishop.us/product/raspberry-pi-5-1gb/ | read |
| A1 | Raspberry Pi 4 Model B 2GB | PiShop.us, US | US$67.50, "IN STOCK", "Maximum Purchase: 1 unit" (older board, only US$10 less: not an option) | https://www.pishop.us/product/raspberry-pi-4-model-b-2gb/ | read |
| A2 | Raspberry Pi DAC+ | PiShop.us, US | US$29.95, "IN STOCK" | https://www.pishop.us/product/raspberry-pi-dac-green-pcb/ | read |
| A2 | Raspberry Pi IQaudio DAC+ (the older board) | SparkFun, US | US$20.00, "Discontinued" | https://www.sparkfun.com/raspberry-pi-iqaudio-dac.html | read |
| A2 | PCM5122 DAC HAT with RCA out | AliExpress | no listing with RCA outputs on the search page (3.5 mm jack boards only) | https://www.aliexpress.com/w/wholesale-pcm5122-raspberry-pi-dac-hat.html | search page |
| A2 | Raspberry Pi DAC+ | CanaKit | the product page guessed for it returned "Page Not Found" | https://www.canakit.com/raspberry-pi-dac-plus.html | not read |
| A3 | Raspberry Pi 27 W USB-C PSU (official) | CanaKit, US `ASSUMED` | US$12.95, in stock, black or white | https://www.canakit.com/official-raspberry-pi-5-power-supply-27w-usb-c.html | read |
| A3 | Raspberry Pi 27 W USB-C PSU (official) | PiShop.us, US | US$12.95, "IN STOCK" | https://www.pishop.us/product/raspberry-pi-27w-usb-c-power-supply-black-us/ | read |
| A3 | "Raspberry Pi 5 Official Power Supply 27W" | AliExpress listing, China `ASSUMED` | US$15.65 (dearer than the US price) | https://www.aliexpress.us/item/3256807925969859.html | search page |
| A3 | unbranded 27 W 5.1 V 5 A USB-C supply | AliExpress listing, China `ASSUMED` | US$2.38 | https://www.aliexpress.us/item/3256808070395034.html | search page |
| A4 | Raspberry Pi SD card 32 GB (A2 class) | PiShop.us, US | US$19.95, stock not shown | https://www.pishop.us/product/raspberry-pi-sd-card-32gb/ | read |
| A4 | Raspberry Pi SD card 32 GB (A2 class) | Adafruit, US | US$13.69, "No longer stocked" | https://www.adafruit.com/product/6010 | read |
| A4 | Raspberry Pi SD card 32 GB (A2 class) | SparkFun, US | US$24.95 | https://www.sparkfun.com/raspberry-pi-a2-class-sd-card-32gb.html | snippet |
| A4 | Micro Center 32 GB microSDHC, class 10 | Micro Center, US | US$13.99 (the page returned HTTP 403) | https://www.microcenter.com/product/658457/micro-center-32gb-microsdhc-card-class-10-flash-memory-card-with-adapter | snippet |
| A4 | SanDisk Ultra 32 GB microSDHC, A1 | Walmart listings, US | US$13.40 to US$19.50, several out of stock | https://www.walmart.com/ip/SanDisk-32GB-Ultra-microSDHC-A1-UHS-I-U1-Class-10-Memory-Card-with-Adapter-Speed-Up-to-120MB-s-SDSQUA4-032G-GN6MA/691543554 | snippet |
| A4 | "SanDisk" 32 GB A1 microSD | AliExpress listings, China `ASSUMED` | US$1.09 promotional (a counterfeit risk for flash cards: not an option) | https://www.aliexpress.us/item/3256807348405755.html | search page |
| A5 | Behringer UMC202HD | Amazon US | US$59.00 (the page returned a CAPTCHA: sold-by and ships-from not read) | https://www.amazon.com/dp/B00QHURUBE | snippet |
| A5 | Behringer UMC202HD | Thomann (its US-dollar site); a German seller, so **NON-US EXCEPTION** | US$56 ("$58" in the page title), "In stock"; shipping "calculated on the checkout page" | https://www.thomannmusic.com/behringer_u_phoria_umc202hd.htm | read |
| A5 | Behringer UMC202HD | Sweetwater, Guitar Center, Musician's Friend, Adorama; US | US$86.90 at each (Sweetwater's page returned HTTP 403) | https://www.sweetwater.com/store/detail/UMC202HD--behringer-u-phoria-umc202hd-usb-audio-interface | snippet |
| A5 | Behringer UMC202HD | AliExpress listings, China `ASSUMED` | US$109.22 to US$143.23 (dearer than every US offer) | https://www.aliexpress.us/item/3256812019977180.html | search page |
| A6 | 24 MHz 8-channel USB logic analyzer (the Cypress FX2 design sigrok's fx2lafw drives) | AliExpress listing, China `ASSUMED`; **NON-US EXCEPTION** | US$5.83, delivery "Oct 13 - 17" | https://www.aliexpress.us/item/3256805202626131.html | search page |
| A6 | the same design, other listings | AliExpress listings, China `ASSUMED` | US$3.33 (a bundle deal), US$6.98, US$9.90 | https://www.aliexpress.us/item/3256813075691909.html | search page |
| A6 | SparkFun USB Logic Analyzer 24 MHz 8 ch (TOL-18627) | SparkFun, US | US$26.95, "In stock" | https://www.sparkfun.com/usb-logic-analyzer-24mhz-8-channel.html | read |
| A7 | 40 male/male jumper wires, 10 cm | AliExpress listing, China `ASSUMED`; **NON-US EXCEPTION** | US$1.09 promotional, list US$4.70 | https://www.aliexpress.us/item/3256812254654063.html | search page |
| A7 | Adafruit premium male/male jumper wires, 20 x 3" | Adafruit, US | US$1.95, "In stock" | https://www.adafruit.com/product/1956 | read |
| A8 | Hosa CPR-202 (dual RCA to dual 1/4" TS, 2 m) | Sweetwater, US | US$16.95 | https://www.sweetwater.com/store/detail/CPR202--hosa-cpr202-2-meter | snippet |
| A8 | Hosa YRA-104 (RCA male to two RCA female, 6") | Sweetwater, US; B&H, US | US$5.95 at each | https://www.sweetwater.com/store/detail/YRA104--hosa-yra-104-y-cable-rca-to-dual-rcaf-6-inch | snippet |
| A8 | dual RCA to dual 6.35 mm TS mono cable | AliExpress listing, China `ASSUMED` | US$1.78 | https://www.aliexpress.us/item/3256811814428373.html | search page |
| A8 | Monoprice RCA and TS cables | Monoprice, US | not read (HTTP 403); a snippet showed only adapters and Y-adapters at US$19.89 and up | https://www.monoprice.com/product?p_id=38079 | not read |

### Recommended tier lines (B1 to B10)

| Line | Offer | Seller, ship-from | Price | URL | Basis |
|---|---|---|---|---|---|
| B1 | Esparagus Audio Brick, option "Single DAC, ESP32-S3" | Elecrow, "Sold By Sonocotta" (Poland); ship-from not stated, China `ASSUMED`; **NON-US EXCEPTION** | US$59.00; the page says "In stock" with limited quantity, stock per option not shown; shipping not shown | https://www.elecrow.com/esparagus-audio-brick.html | read |
| B1 | Esparagus Audio Brick (ESP32-S3) | Crowd Supply, US fulfilment | US$59 + US$8 US shipping, **"No longer available"** (both variants; it was a pre-order on 2026-09-30) | https://www.crowdsupply.com/sonocotta/esparagus-audio-brick | read |
| B1 | Esparagus Audio Brick | Tindie, ships from Poland | US$59.00 (US$57.00 each for 2 to 4); "Single DAC, ESP32-S3": 0 in stock; only the classic ESP32 variant has stock (4 left) | https://www.tindie.com/products/sonocotta/esparagus-audio-brick/ | read |
| B1 | Esparagus Audio Brick | Lectronz (the maker's store), ships from Warsaw, Poland | US$59.00, out of stock | https://lectronz.com/products/esparagus-audio-brick | read |
| B1 | "Esparagus Audio Brick" | AliExpress resellers, China `ASSUMED` | US$186.45 and US$270.57 (three to four times the maker's price: not an option) | https://www.aliexpress.us/item/3256811720152649.html | search page |
| B3 | Waveshare ESP32-S3-ETH, all eight variants (board only up to PoE module plus camera) | Waveshare's own store; ship-from not stated, China `ASSUMED`; **NON-US EXCEPTION** | US$16.99 to US$25.99 (the price of each variant is not shown apart; shipping not shown) | https://www.waveshare.com/esp32-s3-eth.htm | read |
| B3 | "Waveshare ESP32-S3 ETH Development Board, Optional For PoE Module And Camera Module" | AliExpress listing, China `ASSUMED`; **NON-US EXCEPTION** | US$25.10, free shipping shown | https://www.aliexpress.us/item/3256813106838947.html | search page |
| B3 | "Waveshare ESP32-S3 Ethernet Development Board With PoE Power Supply OV5640 Camera" | AliExpress listing, China `ASSUMED` | US$21.83 | https://www.aliexpress.us/item/3256812944934685.html | search page |
| B3 | "ESP32-S3 ETH Camera Development Board PoE RJ45 ... W5500" (maker not named) | AliExpress listing, China `ASSUMED` | US$11.02 | https://www.aliexpress.us/item/3256807758687274.html | search page |
| B3 | Waveshare ESP32-S3-ETH with PoE module | Amazon US | US$25.99 to US$34.99 (the page returned a CAPTCHA; ships-from still unverified) | https://www.amazon.com/waveshare-ETH-Development-ESP32-Module/dp/B0DLK8QMFK | snippet |
| B4 | Adafruit PCM5102 I2S DAC (6250) | Adafruit, US | US$4.95, "In stock" (it was out of stock on 2026-09-30) | https://www.adafruit.com/product/6250 | read |
| B4 | GY-PCM5102 module | AliExpress listings, China `ASSUMED` | US$2.75 and US$3.25 (US$1.09 promotional) | https://www.aliexpress.us/item/3256812524060031.html | search page |
| B5 | TP-Link TL-POE160S | CDW, US | US$22.00 | https://www.cdw.com/product/tp-link-tl-poe160s-802.3at-af-gigabit-poe-injector-non-poe-to-poe-adapt/6401017 | read |
| B5 | TP-Link TL-POE160S | B&H Photo, US | US$22.99 (the page returned HTTP 403) | https://www.bhphotovideo.com/c/product/1633607-REG/tp_link_tl_poe160s_poe_injector_black.html | snippet |
| B5 | "Gigabit PoE+ 802.3AT PoE Injector 52V 30Watt" and "Gigabite 54V 30W POE Injector" | AliExpress listings, China `ASSUMED` | US$8.24 and US$11.74 (item URLs, mains plug and safety marks not shown) | https://www.aliexpress.com/w/wholesale-poe-injector-802.3at-30w-gigabit.html | search page |
| B6 | PoE Texas GAT-24V25W | PoE Texas, Austin, TX, US | US$31.99, **"Sold out"** (it was orderable on 2026-09-30) | https://shop.poetexas.com/products/gat-24v25w | read |
| B6 | PoE Texas GAT-24V25W | Amazon US, the maker's listing | US$31.99, "15 left" as of 2026-09-14 per a price-history site (the page returned a CAPTCHA) | https://www.amazon.com/dp/B07FHSSPR1 | snippet |
| B6 | "Gigabit POE converter, active 48V to passive 24V POE splitter" | AliExpress listing, China `ASSUMED` | US$20.70, free shipping shown (802.3at not stated in the title) | https://www.aliexpress.us/item/3256808618709786.html | search page |
| B6 | "Industrial 48V To 24V 3A Gigabit POE Converter" | AliExpress listing, China `ASSUMED` | US$27.54 | https://www.aliexpress.us/item/3256809556511088.html | search page |
| B7 | Mean Well GST60A24-P1J (24 V 2.5 A, 2.1 mm plug) | DigiKey, Mouser, Jameco; US | US$18.60 at each (Jameco's page returned HTTP 403) | https://www.digikey.com/en/products/detail/mean-well-usa-inc/GST60A24-P1J/7703715 | snippet |
| B7 | unbranded 24 V adapters, 1 A to 10 A | AliExpress listing, China `ASSUMED` | US$5.10 (lowest variant) | https://www.aliexpress.us/item/3256801313203398.html | search page |
| B8 | Dayton Audio B652-AIR pair | Parts Express, US | not read (a script-only page, twice); US$59.80 a pair is the price in a magazine review, not a current offer | https://www.parts-express.com/Dayton-Audio-B652-AIR-6-1-2-2-Way-Bookshelf-Speaker-with-AMT-Tweeter-Pair-300-651 | not read |
| B8 | Dayton Audio B652-AIR pair | Amazon US | "No featured offers available" | https://www.amazon.com/Dayton-Audio-B652-AIR-Bookshelf-Speaker/dp/B00NOA58RS | snippet |
| B8 | Polk Audio T15 pair | Crutchfield, US | not read (HTTP 403) | https://www.crutchfield.com/p_107T15/Polk-Audio-T15.html | not read |
| B9 | aluminium-housed wirewound resistor, 10 W to 100 W, 8 ohm among the values | AliExpress listing, China `ASSUMED`; **NON-US EXCEPTION** | US$1.56 each (lowest variant) | https://www.aliexpress.us/item/3256808746284952.html | search page |
| B9 | a second listing, 2 pieces, 4, 6 or 8 ohm | AliExpress listing, China `ASSUMED` | US$1.09 promotional, list US$4.67 | https://www.aliexpress.us/item/3256809093194237.html | search page |
| B10 | Monoprice Cat6 patch cable, 3 ft, 24 AWG | Monoprice, US | US$2.29 (the site returned HTTP 403) | https://www.monoprice.com/product?p_id=2114 | snippet |
| B10 | Cat6 patch cable, 1 m to 30 m | AliExpress listing, China `ASSUMED` | US$2.10 (lowest variant) | https://www.aliexpress.us/item/3256810301118420.html | search page |

### Full tier lines (C1 to C5)

| Line | Offer | Seller, ship-from | Price | URL | Basis |
|---|---|---|---|---|---|
| C1 | ESP32-P4X-Function-EV-Board | Mouser, US | US$59.74 (the page timed out, then refused an automated read) | https://www.mouser.com/ProductDetail/Espressif-Systems/ESP32-P4X-Function-EV-Board?qs=Naspt24KZtm%2F%2FovBNrcBgA%3D%3D | snippet |
| C1 | ESP32-P4X-Function-EV-Board | DigiKey, US | US$59.92 (the page returned HTTP 403) | https://www.digikey.com/en/products/detail/espressif-systems/ESP32-P4X-FUNCTION-EV-BOARD/29196984 | snippet |
| C1 | "ESP32 P4X Function EV Board" listings | AliExpress listings, China `ASSUMED` | US$12.44, US$20.70 and US$177.44: the low prices are a lowest variant, not the board, and nothing on the page says which variant is the board | https://www.aliexpress.us/item/3256809154083121.html | search page |
| C2 | the A1 to A4 offers | as above | US$140.35 a set | as above | read |
| C3 | Raspberry Pi DigiAMP+ | PiShop.us, US | US$42.95, "IN STOCK" | https://www.pishop.us/product/raspberry-pi-digiamp-green-pcb/ | read |
| C3 | DigiAMP+ at a second seller | SparkFun's search lists only the discontinued IQaudio DAC+ | none found | https://www.sparkfun.com/catalogsearch/result/?q=raspberry+pi+dac | read |
| C4 | the B6 offers | as above | US$31.99 | as above | snippet |
| C5 | DSLogic Plus | DreamSourceLab (the maker); ship-from not stated, China `ASSUMED`; **NON-US EXCEPTION** | US$149.00 (list US$199.00), "In stock", "Free shipping for order amount over $100" | https://www.dreamsourcelab.com/shop/logic-analyzer/dslogic-plus/ | read |
| C5 | "DreamSourceLab DSLogic Plus ... 16 Channels 100MHz" | AliExpress listing, China `ASSUMED` | US$86.20, 5 sold (whether it is the maker's unit is not shown) | https://www.aliexpress.us/item/3256811489186310.html | search page |
| C5 | "DSLogic Basic Plus Pro Logic Analyzer 16 Channels" | AliExpress listing, China `ASSUMED` | US$129.89 (lowest variant), 34 sold | https://www.aliexpress.us/item/3256806787212699.html | search page |

## Options

Every line: the pick, why it is the best value that meets the line's requirement, and the cheaper
equivalent with what it would change in `docs/bench-packet.md`. Prices are the 2026-10-04 offers
above; "Basis" is the pick's price basis. Shipping is not in any total except B2.

### Option A: Minimal tier (SOUND-2, RIG-3, SYNC-4 on Linux; WIFI-7 on the owner's S3)

| # | Pick, with seller and ship-from | Requirement, and why this pick | Qty | Unit | Line | Basis | Cheaper equivalent, and what it changes in the bench packet |
|---|---|---|---|---|---|---|---|
| A1 | Raspberry Pi 5 2GB; CanaKit, US `ASSUMED` | the two wired Linux clients of SOUND-2 and SYNC-4 (BRIEF §5.4), one of which builds the workspace. The price is the same US$77.50 at every seller with stock; CanaKit shows stock and no purchase limit, so both boards and both A3 supplies come in one order, where PiShop.us takes two | 2 | $77.50 | $155.00 | read | Endpoint B as a Raspberry Pi 5 1GB (PiShop.us, US$45.00, read): saves US$32.50. Endpoint B only runs `chorus-client`, which S0 step 5 copies to it; endpoint A stays 2GB because it runs `cargo build`. That 1 GB is enough for the client is `ASSUMED`. Changes: the `CHORUS_BENCH_DEVICE_NOTE` strings of S1 to S3 ("2 x Raspberry Pi 5 2GB") and the A1 row |
| A2 | Raspberry Pi DAC+ (PCM5122, line out on RCA); PiShop.us, US | the line outputs RIG-3 captures; Pi-side clock, one crystal per endpoint. The only offer found in stock; no AliExpress PCM5122 HAT with RCA outputs was found | 2 | $29.95 | $59.90 | read | The B4 part on the Pi's I2S pins (Adafruit 6250, US$4.95): saves US$50.00. Changes: S0 step 6's overlay (`hifiberry-dacplus-std` no longer fits a DAC without I2C control; the right overlay is not verified here), five jumper wires per Pi instead of a HAT, and the A8 cables (the output connector is not RCA). Not recommended: it puts unverified wiring into the reference rig |
| A3 | Raspberry Pi 27 W USB-C PSU (US); CanaKit, US `ASSUMED` | power for A1 at 5 V 5 A. The official supply at US$12.95 is the lowest price for a supply that is known to negotiate 5 A with a Pi 5, and it rides in the A1 order | 2 | $12.95 | $25.90 | read | An unbranded "27W 5.1V 5A" supply on AliExpress (US$2.38, search page): saves about US$21. Changes nothing in the packet. Not recommended: an unlisted mains supply, and a Pi 5 that is under-supplied throttles USB current, which is the UMC202HD's power |
| A4 | Raspberry Pi SD card 32 GB (A2 class); PiShop.us, US | OS for A1. US$19.95 is the only readable in-stock price for the card; it rides in the A2 order | 2 | $19.95 | $39.90 | read; stock not shown | Any 32 GB class A1 card from a US store (SanDisk Ultra at Walmart US$13.40 to US$19.50, Micro Center's own at US$13.99; snippets): saves up to about US$13. Changes nothing. Cards the owner already has cost nothing |
| A5 | Behringer UMC202HD (2-in, 192 kHz, UAC2); Amazon US | RIG-3's dual-input capture: two line outputs into L and R. The same unit is US$86.90 at every US music store; US$59.00 at Amazon US is US$27.90 less | 1 | $59.00 | $59.00 | snippet; sold-by and ships-from to confirm | The same unit from Thomann at US$56 (read, in stock), a German seller: saves US$3 before a shipping cost that the page does not show. Changes nothing |
| A6 | 24 MHz 8-channel USB logic analyzer, the Cypress FX2 design; AliExpress listing 3256805202626131; **NON-US EXCEPTION** (China `ASSUMED`) | RIG-3's digital cross-check: GPIO marker edges read by sigrok's fx2lafw driver. The SparkFun analyzer is the same 24 MHz 8-channel class of device at US$26.95; this listing is US$21.12 less | 1 | $5.83 | $5.83 | search page | This is the cheaper part. Changes: the A6 row's name only; S7.5's `sigrok-cli -d fx2lafw` command is the same. If it does not enumerate under fx2lafw, the SparkFun unit (US, in stock, US$26.95) is the fallback |
| A7 | 40 male/male jumper wires, 10 cm; AliExpress listing 3256812254654063; **NON-US EXCEPTION** (China `ASSUMED`) | marker GPIO to the analyzer. It rides in the A6 order, where the Adafruit wires would be an order of their own with its own shipping | 1 | $4.70 | $4.70 | search page (list price; US$1.09 promotional) | This is the cheaper part. Changes nothing. Adafruit 1956 (US, in stock, US$1.95 for 20) is the US offer |
| A8 | Hosa CPR-202 (dual RCA to dual 1/4" TS, 2 m) and Hosa YRA-104 (RCA male to two RCA female, for the rig's self-calibration: one source into both inputs); Sweetwater, US | A2 into A5. Named parts at US$22.90 replace the earlier US$30.00 estimate | 1 set | $22.90 | $22.90 | snippet | A dual RCA to dual 6.35 mm TS cable on AliExpress (US$1.78, search page) with the same Y cable: saves about US$15. Changes nothing |
| | **Minimal total** | | | | **$373.13** | | |

WIFI-7 uses one of the owner's S3 boards on its native radio (no purchase; model per the goal-1
Needs item). The house LAN serves the bench; no switch is bought.

### Option B: Recommended tier (Minimal plus the P1 embedded bench: EMBEDDED-5 and the PoE+ path)

Everything in A, plus:

| # | Pick, with seller and ship-from | Requirement, and why this pick | Qty | Unit | Line | Basis | Cheaper equivalent, and what it changes in the bench packet |
|---|---|---|---|---|---|---|---|
| B1 | Esparagus Audio Brick, option "Single DAC, ESP32-S3" (S3, TAS5825M, W5500 on board, 5-26 V, Apache-2.0 design); Elecrow, "Sold By Sonocotta"; **NON-US EXCEPTION** (ship-from not stated, China `ASSUMED`) | P1's bought reference board: TAS5825M register bring-up (goal 9 replaces the 8 `unknown` keys), EMBEDDED-5 sync on the W5500 path; a stereo pair. Elecrow is the only seller that shows the S3 board in stock: Crowd Supply now reads "No longer available", Tindie has 0 of the S3 variant, Lectronz is out of stock | 2 | $59.00 | $118.00 | read; stock of the S3 option to confirm in the cart | None found: no other board with an S3, a TAS5825M and a W5500 is on sale. One board instead of two saves US$59.00 and still serves S7.1 to S7.4; S7.5's second endpoint is then a B3 (the packet already allows it) |
| B2 | Elecrow shipping to the US for B1 | | 1 | $20.00 | $20.00 | `ASSUMED` (the page shows no shipping cost) | |
| B3 | Waveshare ESP32-S3-POE-ETH-M (W5500, INT on GPIO10, 802.3af module, pre-soldered headers); AliExpress listing 3256813106838947, or Waveshare's own store; **NON-US EXCEPTION** (China `ASSUMED`) | measures the S3 + W5500 sync path at line level without waiting for B1; GPIO headers for the marker. US$25.99 is the top of the maker's own price range, so it bounds this variant; it replaces an Amazon listing at US$34.99 whose ship-from was never verified | 2 | $25.99 | $51.98 | read (the maker's range US$16.99 to US$25.99; this variant's own price to confirm) | The board without the PoE module (ESP32-S3-ETH-M, from US$16.99 in the same range): saves up to US$18.00. Changes nothing: no session powers B3 over PoE, it runs from USB |
| B4 | Adafruit PCM5102 I2S DAC (6250), line out, no MCLK; Adafruit, US | line output for B3 into the rig. Back in stock at US$4.95 from a US seller with documented pins | 2 | $4.95 | $9.90 | read | A GY-PCM5102 module on AliExpress (US$2.75 to US$3.25, search page): saves about US$4. Changes: its solder-jumper settings have to be checked before use (`ASSUMED`; the board's documentation was not read). Too small a saving to recommend |
| B5 | TP-Link TL-POE160S PoE+ injector (802.3at/af, 30 W); CDW, US | powers the compact-speaker path on the bench until the ES228GP is live. US$22.00 is the lowest readable price | 1 | $22.00 | $22.00 | read | A "Gigabit PoE+ 802.3AT" 30 W injector on AliExpress (US$8.24, search page): saves US$13.76. Changes nothing. Not recommended: a mains device with no listed safety mark. If the ES228GP is live, B5 is not bought at all |
| B6 | PoE Texas GAT-24V25W splitter (802.3at in, 24 V 25 W out on a 2.1 x 5.5 mm plug, gigabit pass-through); Amazon US, the maker's listing | the compact speaker's power path (K90): PoE+ into a B1's 5-26 V input. The maker's own store is sold out today, and its Amazon listing has the same US$31.99 | 1 | $31.99 | $31.99 | snippet; stock to confirm | A gigabit "active 48V to passive 24V" converter on AliExpress (US$20.70, search page): saves US$11.29. Changes: section 3's S7 wiring, because its 24 V comes out as passive PoE on the RJ45, not on a barrel plug, and 802.3at is not stated in its title |
| B7 | Mean Well GST60A24-P1J (24 V 2.5 A desktop supply, 2.1 mm plug); DigiKey, Mouser or Jameco, US | 24 V DC for the second B1 (the mains classes' path). A named, safety-listed supply at US$18.60 replaces the earlier US$25.00 estimate | 1 | $18.60 | $18.60 | snippet; whether a mains cord is in the box to confirm | An unbranded 24 V adapter on AliExpress (from US$5.10, search page): saves about US$13. Changes nothing. Not recommended: an unlisted mains supply feeding a class-D amplifier on the bench |
| B8 | Passive 8 ohm bookshelf speaker pair (e.g. Dayton Audio B652-AIR); Parts Express, US (`ASSUMED`) | hear the TAS5825M; the owner owns no test speakers per K21. No seller's page could be read today, so the estimate stands | 1 pair | $70.00 | $70.00 | `ASSUMED` | Any passive 4 to 8 ohm pair the owner has or finds used: saves up to US$70.00. Changes nothing (the packet says "4-8 ohm") |
| B9 | Dummy load and attenuator parts: 2 x 8 ohm 50 W aluminium-housed resistors (AliExpress listing 3256808746284952, **NON-US EXCEPTION**, China `ASSUMED`), divider resistors and DC-blocking capacitors | the TAS output is bridge-tied speaker level; the rig's line inputs need it loaded and divided (section 3 of the bench packet). The resistors are US$1.56 each at their lowest variant | 1 set | $10.00 | $10.00 | `ASSUMED` for the set (the resistors: search page) | This is the cheaper part (the earlier estimate was US$20.00 from a US seller). Changes nothing |
| B10 | Monoprice Cat6 patch cable, 3 ft; Monoprice, US | B1, B3, B5, B6 | 4 | $2.29 | $9.16 | snippet | Patch cables the owner already has: saves US$9.16. Changes nothing |
| | **Additions** | | | | **$361.63** | | |
| | **Recommended total (A + B)** | | | | **$734.76** | | |

### Option C: Full tier (Recommended plus P1's escalation board, a third room and an amplified Linux endpoint)

Everything in A and B, plus:

| # | Pick, with seller and ship-from | Requirement, and why this pick | Qty | Unit | Line | Basis | Cheaper equivalent, and what it changes in the bench packet |
|---|---|---|---|---|---|---|---|
| C1 | ESP32-P4X-Function-EV-Board (v3.x "X" silicon, RMII Ethernet, ES8311); Mouser, US | P1's named escalation: APLL, EMAC time stamps and PPS against the S3 + W5500 path. The two US distributors are within US$0.18 of each other; no AliExpress listing shows a credible price for the board itself | 1 | $59.74 | $59.74 | snippet (DigiKey US$59.92) | None found |
| C2 | A third Linux client: Pi 5 2GB, DAC+, 27 W PSU, SD card; the A1 to A4 picks | a three-room group on real hardware (K75's scale in miniature) | 1 set | $140.35 | $140.35 | read | A Pi 5 1GB as the third client (it only runs the client): saves US$32.50; `ASSUMED` as in A1. Changes nothing (no session in the packet uses a third client yet) |
| C3 | Raspberry Pi DigiAMP+ (TAS5756M, 12-24 V in, powers the Pi); PiShop.us, US | an amplified Linux endpoint: the streaming-amp and rack-amp class (K70, K96) heard, not only simulated. The only offer found | 1 | $42.95 | $42.95 | read, in stock | None found |
| C4 | Second PoE Texas GAT-24V25W; the B6 pick | both B1 boards on PoE+ as a compact stereo pair | 1 | $31.99 | $31.99 | snippet | As B6 |
| C5 | DreamSourceLab DSLogic Plus (buffered, sigrok); DreamSourceLab, the maker; **NON-US EXCEPTION** (ship-from not stated, China `ASSUMED`) | a buffered analyzer if the fx2lafw device drops samples. The maker's own price is read and in stock with free shipping; bought only if A6 fails | 1 | $149.00 | $149.00 | read, in stock | An AliExpress listing that names the same unit (US$86.20, search page, 5 sold): saves US$62.80. Changes nothing. Whether it is the maker's unit is not shown |
| | **Additions** | | | | **$424.03** | | |
| | **Full total (A + B + C)** | | | | **$1,158.79** (without the C5 exception: $1,009.79) | | |

Not in any tier, with reasons: an oscilloscope (the rig and the analyzer cover BRIEF §10's
methods); a measurement mic (K87: room correction uses the phone); a Pi PoE HAT (Pi 5's official
PoE+ HAT is unreleased per the planning research, and the bench runs Pis on USB-C); a
TDM-capable Linux board (goal 10 works on fakes; P2 and goal 26 settle multichannel hardware);
the TI TAS5825MEVM (out of stock on ti.com on 2026-09-30 and needs a second motherboard; B1 does
the same job as a product-grade board); a Scarlett 2i2 (the UMC202HD is enough at 96 kHz).

## What changed against the 2026-09-30 list

| Line | 2026-09-30 | 2026-10-04 | Change in the line total |
|---|---|---|---|
| A1 | Pi 5 2GB at US$65.00, PiShop.us, two orders | US$77.50 (the maker raised the price on 2026-10-01, per the news item in Sources); the pick moves to CanaKit, one order | +$25.00 |
| A2 | DAC+ US$29.95, PiShop.us | the same | 0 |
| A3 | 27 W PSU US$12.95, PiShop.us | the same price, bought from CanaKit with A1 | 0 |
| A4 | SD card US$19.95, PiShop.us | the same | 0 |
| A5 | UMC202HD US$86.90, Sweetwater (snippet) | US$59.00, Amazon US (snippet); Thomann US$56 read | -$27.90 |
| A6 | SparkFun analyzer US$26.95 | an AliExpress FX2 analyzer at US$5.83 (non-US); SparkFun is the fallback | -$21.12 |
| A7 | Adafruit jumpers, 2 x US$1.95 | one AliExpress pack of 40 at its US$4.70 list price (non-US) | +$0.80 |
| A8 | cables, `ASSUMED` US$30.00 | Hosa CPR-202 and YRA-104, US$22.90 (snippet) | -$7.10 |
| B1 | Crowd Supply pre-order, estimated to ship 2027-01-25 | Crowd Supply reads "No longer available"; the pick is Elecrow's in-stock listing at the same US$59.00 (non-US). The wait for a pre-order is gone | 0 |
| B2 | Crowd Supply US shipping US$8.00 | Elecrow shipping, `ASSUMED` US$20.00 | +$12.00 |
| B3 | Amazon listing US$34.99, ship-from unverified | the maker's top-of-range US$25.99 (AliExpress or waveshare.com; non-US, flagged) | -$18.00 |
| B4 | Adafruit 6250 US$4.95, out of stock | the same price, in stock | 0 |
| B5 | TL-POE160S US$22.99, B&H (snippet) | US$22.00, CDW (read) | -$0.99 |
| B6 | GAT-24V25W US$31.99 from the maker's store | the maker's store is sold out; the same price on its Amazon listing (snippet) | 0 |
| B7 | 24 V supply, `ASSUMED` US$25.00 | Mean Well GST60A24-P1J US$18.60 (snippet) | -$6.40 |
| B8 | speaker pair, `ASSUMED` US$70.00 | unchanged; no offer could be read | 0 |
| B9 | load and divider parts, `ASSUMED` US$20.00, US seller | `ASSUMED` US$10.00 with AliExpress resistors (non-US) | -$10.00 |
| B10 | Cat6, `ASSUMED` 4 x US$3.00 | Monoprice 4 x US$2.29 (snippet) | -$2.84 |
| C1 | P4X board US$59.74, Mouser (snippet) | the same (snippet again; both distributor pages refused the read) | 0 |
| C2 | third client US$127.85 | US$140.35 (the Pi 5 price rise) | +$12.50 |
| C3 | DigiAMP+ US$42.95 | the same | 0 |
| C4 | second splitter US$31.99 | the same price, from the Amazon listing | 0 |
| C5 | DSLogic Plus US$149.00, not re-read | US$149.00, read, in stock | 0 |
| | **Minimal** | $403.45 to **$373.13** | -$30.32 |
| | **Recommended** | $791.31 to **$734.76** | -$56.55 |
| | **Full** | $1,202.84 to **$1,158.79** | -$44.05 |

The 2026-09-30 flags, today: B1 is no longer a pre-order (the campaign store is closed; Elecrow
has stock); B3's unverified Amazon ship-from is replaced by a flagged non-US pick; B4 is back in
stock. New flags: B6 is sold out at its maker's store, and the Pi 5 2GB costs US$12.50 more.

## Comparison

| Criterion | A: Minimal | B: Recommended | C: Full |
|---|---|---|---|
| Total | $373.13 | $734.76 | $1,158.79 ($1,009.79 without C5) |
| Phases it can run | SOUND-2, RIG-3, SYNC-4, WIFI-7 | plus EMBEDDED-5 (line level and TAS) and the PoE+ path | plus the P4 comparison, a three-room group, an amplified Linux endpoint |
| P1 consistency | S3 only via owned boards | the P1 board and link, measured | plus P1's escalation measured |
| Non-US lines | A6, A7 (AliExpress) | plus B1 and its shipping (Elecrow), B3, B9 | plus C5 |
| Snippet, search-page or `ASSUMED` share | $92.43 of $373.13 | $304.16 of $734.76 | $395.89 of $1,158.79 |
| Waits on a pre-order | no | no | no |
| With the two low-risk savings taken (endpoint B, and in Full the third client, as a Pi 5 1GB; B3 without the PoE module at the bottom of its range) | $340.63 | $684.26 | $1,075.79 |

## Recommendation

**Recommendation:** Option B, the recommended tier at $734.76, because it is the smallest set that runs every bench phase through EMBEDDED-5 on the platform and link P1 recommends, including the compact speaker's PoE+ power path, and after the re-evaluation none of it waits on a pre-order.

Why: Minimal proves Linux sync but leaves EMBEDDED-5 and the TAS register map unmeasured, which is
the half of P1 that most needs data. B adds two S3 + W5500 boards measurable at line level, the
bought S3 + TAS5825M + W5500 reference board (in stock at Elecrow), and the PoE+ injector and
splitter. Full adds the P4 comparison, which matters only if B's measurements put the W5500 path
outside the 0.5 ms stereo-pair bound. What the owner gives up by not buying Full: the P4
cross-check up front (it can be bought later, when EMBEDDED-5 asks for it).

Where the money moved: the largest single saving is the UMC202HD (US$27.90, same unit), then the
logic analyzer (US$21.12, an AliExpress FX2 device) and B3 (US$18.00). AliExpress is the pick only
where the part is a commodity with no mains connection (A6, A7, B3, B9); for the mains supplies
(A3, B5, B7), the flash cards (A4) and the audio interface (A5) the cheaper AliExpress offer is
either dearer than the US one or not worth the risk, and each such line says so.

## If the owner defers

The recommended tier (Option B) is filed as the Needs packet, marked "awaiting choice"; goal 7's
bench packet, wiring and commands are written against it. Nothing is ordered (K4). The cost: bench
sessions cannot start until the owner buys. B1's stock at Elecrow is described as limited, and
every other seller of the S3 variant is out, so a long deferral may bring back the wait that the
2026-09-30 list had.

## Open inputs

- The owner's ESP32-S3 boards' module marking and `esptool.py chip_id` (goal-1 Needs item): decides
  whether one fits WIFI-7 (needs PSRAM, `ASSUMED`).
- Whether the ES228GP is live when the owner buys; if so, B5 is not needed.
- B1's stock per option at Elecrow (the page shows one "In stock" for three options) and its
  shipping cost and ship-from: read in the cart by the owner.
- B1's power connector: the campaign page lists a 4-pin snap-in connector, and the 2026-08-06
  production update changed it to a 2-pin version. B6's and B7's output is a 2.1 x 5.5 mm plug, so
  add a line for a 2.1 mm plug-to-bare-wire lead (`ASSUMED` about $5; not yet in the tier
  totals). Whether a free GPIO is reachable for the marker is still not stated; if not, B3 carries
  the marker work.
- Whether B1 wires the W5500 INT pin to a GPIO: unconfirmed (see P1); B3's INT on GPIO10 is
  confirmed, so B3 carries the INT-mode measurement either way.
- Every AliExpress line (A6, A7, B3, B9): the store, the variant, the ship-from and the final
  price are on the item page, which only a logged-in browser shows.
- Every snippet and `ASSUMED` price: A5, A8, B2, B6, B7, B8, B9, B10, C1, C4.
- A5 at Amazon: that the listing is sold or shipped by Amazon or by the maker, not a reseller.
- Whether the owner already has passive speakers, cables, SD cards or a spare 24 V supply (then
  B8, A8, B10, A4, B7 drop).
- Shipping and sales tax are in no total except B2.

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

Read or tried on 2026-10-04 (the re-evaluation; "not read" pages are named in the offers tables):

- CanaKit, Raspberry Pi 5 2GB and the 27 W supply: https://www.canakit.com/raspberry-pi-5-2gb.html and https://www.canakit.com/official-raspberry-pi-5-power-supply-27w-usb-c.html, read 2026-10-04
- PiShop.us: the five product pages above, plus https://www.pishop.us/product/raspberry-pi-5-1gb/ and https://www.pishop.us/product/raspberry-pi-4-model-b-2gb/, read 2026-10-04
- SparkFun: https://www.sparkfun.com/raspberry-pi-5-2gb.html, https://www.sparkfun.com/usb-logic-analyzer-24mhz-8-channel.html, https://www.sparkfun.com/catalogsearch/result/?q=raspberry+pi+dac, read 2026-10-04
- Adafruit: products 6007 (Pi 5 2GB), 6010 (SD card), 6250 (PCM5102), 1956 (jumpers), https://www.adafruit.com/product/6007 and the like, read 2026-10-04
- Phoronix, "Raspberry Pi 5 2GB Price Increases To $77.50 USD" (search snippet): https://www.phoronix.com/news/Raspberry-Pi-5-2GB-Price-77.50, 2026-10-04
- Thomann, Behringer UMC202HD: https://www.thomannmusic.com/behringer_u_phoria_umc202hd.htm, read 2026-10-04
- Sweetwater, Guitar Center, Musician's Friend, Adorama, Amazon, UMC202HD (search snippets; Sweetwater returned HTTP 403, Amazon a CAPTCHA): https://www.guitarcenter.com/Behringer/U-PHORIA-UMC202HD-Audiophile-2x2-24-Bit-192-kHz-USB-Audio-Interface-1428935660246.gc, https://www.adorama.com/beumc202hd.html, https://www.amazon.com/dp/B00QHURUBE, 2026-10-04
- Sweetwater, Hosa CPR-202 and YRA-104 (search snippets): https://www.sweetwater.com/store/detail/CPR202--hosa-cpr202-2-meter and https://www.sweetwater.com/store/detail/YRA104--hosa-yra-104-y-cable-rca-to-dual-rcaf-6-inch, 2026-10-04
- Crowd Supply, Esparagus Audio Brick ("No longer available"): https://www.crowdsupply.com/sonocotta/esparagus-audio-brick, read 2026-10-04
- Elecrow, Esparagus Audio Brick (options "Single DAC, classic ESP32", "Single DAC, ESP32-S3", "Dual DAC, ESP32-S3"): https://www.elecrow.com/esparagus-audio-brick.html, read 2026-10-04
- Tindie and Lectronz, Esparagus Audio Brick: https://www.tindie.com/products/sonocotta/esparagus-audio-brick/, https://lectronz.com/products/esparagus-audio-brick and https://lectronz.com/stores/sonocotta, read 2026-10-04
- Waveshare ESP32-S3-ETH: https://www.waveshare.com/esp32-s3-eth.htm, read 2026-10-04
- CDW, TP-Link TL-POE160S: https://www.cdw.com/product/tp-link-tl-poe160s-802.3at-af-gigabit-poe-injector-non-poe-to-poe-adapt/6401017, read 2026-10-04
- PoE Texas GAT-24V25W ("Sold out"): https://shop.poetexas.com/products/gat-24v25w, read 2026-10-04; its Amazon listing (search snippet): https://www.amazon.com/dp/B07FHSSPR1
- Mean Well GST60A24-P1J at DigiKey, Mouser and Jameco (search snippet): https://www.digikey.com/en/products/detail/mean-well-usa-inc/GST60A24-P1J/7703715, 2026-10-04
- DreamSourceLab, DSLogic Plus and the shop page: https://www.dreamsourcelab.com/shop/logic-analyzer/dslogic-plus/ and https://www.dreamsourcelab.com/shop/, read 2026-10-04
- AliExpress search-results pages (titles, lowest-variant prices and item URLs; no store, ship-from or shipping): https://www.aliexpress.com/w/wholesale-usb-logic-analyzer-24mhz-8ch.html, https://www.aliexpress.com/w/wholesale-waveshare-esp32-s3-eth.html, https://www.aliexpress.com/w/wholesale-poe-splitter-24v-gigabit.html, https://www.aliexpress.com/w/wholesale-pcm5122-raspberry-pi-dac-hat.html, https://www.aliexpress.com/w/wholesale-pcm5102a-i2s-dac.html, https://www.aliexpress.com/w/wholesale-behringer-umc202hd.html, https://www.aliexpress.com/w/wholesale-esp32-p4-function-ev-board.html, https://www.aliexpress.com/w/wholesale-dslogic-plus.html, https://www.aliexpress.com/w/wholesale-24v-3a-power-adapter-5.5x2.1.html, https://www.aliexpress.com/w/wholesale-raspberry-pi-5-27w-power-supply.html, https://www.aliexpress.com/w/wholesale-100w-8-ohm-dummy-load-resistor.html, https://www.aliexpress.com/w/wholesale-cat6-patch-cable-1m.html, https://www.aliexpress.com/w/wholesale-rca-to-6.35mm-mono-cable.html, https://www.aliexpress.com/w/wholesale-dupont-jumper-wire-male-to-male-10cm.html, https://www.aliexpress.com/w/wholesale-poe-injector-802.3at-30w-gigabit.html, https://www.aliexpress.com/w/wholesale-micro-sd-card-32gb-a1.html, https://www.aliexpress.com/w/wholesale-esparagus-audio-brick.html, read 2026-10-04
- Not readable on 2026-10-04 (HTTP 403, a CAPTCHA, a timeout or a script-only page): Amazon product pages, AliExpress item pages (a login redirect), Sweetwater, B&H, Mouser, DigiKey, Jameco, Monoprice, Micro Center, Parts Express, Crutchfield

## What was read

Local: `/cache/tmp/chorus-g1/agent-rules.md`, `/cache/tmp/chorus-g1/proposal-format.md`;
[`.claude/goals/2026-09-chorus.md`](https://github.com/NSchatz/chorus/blob/535ed2816b5735153ac81c52a235024aa806f2b5/.claude/goals/2026-09-chorus.md) (header, §0.6-§0.13, §1, §2, §5, §10-§14, §28-§30);
`research-endpoint-hardware.md`, `research-platform-network.md`, `verify-theater-platform.md`
(whole); baseline `BRIEF.md` §2.2, §5.3-§5.5, §6, §8, §10.

Web (all 2026-09-30): every URL in Sources, plus web searches for UMC202HD, TL-POE160S, P4X EV
board, PoE+ splitters, PoE Texas, B652-AIR and Waveshare S3-ETH pricing. No GPL source file and no
CERN-OHL-S or GPL hardware design file was opened.

Web (2026-10-04, the re-evaluation): every URL in the offers tables and in the 2026-10-04 block of
Sources, plus web searches per line for current prices and for AliExpress listings. No GPL source
file and no CERN-OHL-S or GPL hardware design file was opened.
