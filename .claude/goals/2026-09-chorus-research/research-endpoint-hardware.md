# chorus: endpoint and bench hardware research (for the planning interview)

Date: 2026-09-29. Read-only web research, about 30 minutes, four parallel passes (ESP32 boards; amps and PoE; Linux endpoints and the measurement rig; prior art and silicon facts). Nothing in any repo was changed.

Citation rules used throughout:
- `[URL, read 2026-09-29]`: the page or PDF was opened and read today.
- `(snippet)`: the fact came from a search-engine result summary because the page returned 403/429 or would not render. Lower confidence.
- `ASSUMED`: from memory or inference, not verified today.
- `UNCERTAIN`: sources conflict or the reading was partial.

Owner constraints carried in from the sibling `devices` program (local file `/cache/tmp/plan-2026-09-chorus/devices-brief.md`, lines 195-199 and 482-486): "buy what can be bought", and "US-first by ship-from" (a line from a non-US seller needs a same-part US source, except parts JLC places from LCSC stock). Ship-from is recorded below where it was visible; where no US source exists, that is flagged as an owner decision.

Deeper per-item notes (every claim cited) are in the scratch files this summary was built from:
`/scratch/claude-1000/-workspace/a63a26c5-3a53-4feb-8ba6-fcb14ed68583/scratchpad/research-esp32-boards.md`, `research-amps-poe.md`, `research-linux-rig.md` (the last one includes the USB-interface and logic-analyzer sub-reports).

---

## 0. Findings that correct or sharpen BRIEF.md (read these first)

These belong in `docs/decisions/` as proposed brief changes if the owner agrees (BRIEF is the owner's document; not edited here).

1. **The ESP32-S3 has no APLL.** ESP-IDF lists the S3's I2S clock sources as PLL_F160M (default), PLL_D2 240 MHz, XTAL and EXTERNAL, and no APLL [https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-reference/peripherals/clk_tree.html, read 2026-09-29]; the esp32s3 `soc_caps.h` has no `SOC_I2S_SUPPORTS_APLL` [https://raw.githubusercontent.com/espressif/esp-idf/master/components/soc/esp32s3/include/soc/soc_caps.h, read 2026-09-29]. The classic ESP32 and the ESP32-P4 do have APLL [https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/peripherals/clk_tree.html and https://docs.espressif.com/projects/esp-idf/en/stable/esp32p4/api-reference/peripherals/clk_tree.html, read 2026-09-29]. BRIEF 5.4 and 6 ("use the APLL clock source") cannot be done on the S3; decision 0015 already left APLL open ("neither Espressif page read documents one for this chip"). S3 options: the 160 MHz PLL through the fractional MCLK divider, or `I2S_CLK_SRC_EXTERNAL` with an audio oscillator on MCLK-in [https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-reference/peripherals/i2s.html, read 2026-09-29]. Arithmetic (mine): 160 MHz / 12.288 MHz = 13 + 1/48, an exact fraction for 48 kHz x 256. A search summary says older ESP-IDF dropped the divider's fractional part (4,102,564 Hz instead of 4,096,000 Hz) and v5.2 fixed it [https://esp32.com/viewtopic.php?t=37997 (snippet; page behind a bot check), UNCERTAIN]. Bench consequence: measure LRCLK with the logic analyzer on day one, per IDF version. BRIEF 5.3's correction mechanism (sample insert/delete) does not need APLL, so this is a quality/jitter question more than a sync blocker.
2. **The ESP32-S3 has no Ethernet MAC.** Espressif's comparison table: Ethernet MAC "1" on ESP32, absent on ESP32-S2 and ESP32-S3 [https://docs.espressif.com/projects/esp-idf/en/v5.0/esp32s3/hw-reference/chip-series-comparison.html, read 2026-09-29]. On an S3, wired means SPI Ethernet (W5500 class). BRIEF 5.4's "RMII PHY is faster but burns ~9 pins" is really a choice between MCUs (ESP32 or P4), not between PHYs on the S3.
3. **W5500 throughput is modest but fine for PCM.** Reports of about 10-16 Mbit/s with lwIP on ESP32/S3, 34.8 Mbit/s with WIZnet's offload [https://esp32.com/viewtopic.php?t=37250 and https://www.hackster.io/tjr0927/esp32-s3-wiznet-w5500-toe-vs-lwip-iperf-performance-test-274510 (snippets)]. 48k/24 stereo is 2.3 Mbit/s (BRIEF 6). The real hazard is polling: a report of 1.32 s responses without the INT pin wired [https://esp32.com/viewtopic.php?t=39779 (snippet)]. Wire W5500 INT.
4. **The ESP32-P4 is the strongest embedded timing platform on paper**: APLL, an internal EMAC with IEEE 1588 hardware time stamping ("Experimental Feature") and PPS out on a GPIO from silicon rev 3 [https://docs.espressif.com/projects/esp-idf/en/stable/esp32p4/api-reference/network/esp_eth.html, read 2026-09-29]; its RMII reference comes from MPLL, leaving APLL free for I2S [esp32p4 soc_caps.h, https://raw.githubusercontent.com/espressif/esp-idf/master/components/soc/esp32p4/include/soc/soc_caps.h, read 2026-09-29]. Espressif published AES67 on an ESP32-P4-Nano in June 2026, steering the APLL sigma-delta in sub-ppm steps instead of dropping samples; no sync accuracy numbers given [https://developer.espressif.com/blog/2026/06/aes67-audio-over-ip-on-the-esp32-p4/, read 2026-09-29]. It needs a companion chip for radio (BRIEF 5.4), which is irrelevant on wired endpoints.
5. **Classic-ESP32 RMII boards usually steal the APLL.** When the ESP32 generates the 50 MHz RMII clock itself, it uses the APLL, and "If Wi-Fi and Ethernet are used simultaneously, the RMII clock cannot be generated by the internal APLL" [https://docs.espressif.com/projects/esp-dev-kits/en/latest/esp32/esp32-ethernet-kit/user_guide.html, read 2026-09-29]. Olimex ESP32-POE and ESP32-POE2 generate it from the ESP32 (schematic net `GPIO17\EMAC_CLK_OUT_180`) [https://raw.githubusercontent.com/OLIMEX/ESP32-POE2/main/HARDWARE/ESP32-PoE2_Rev_B/ESP32-PoE2_Rev_B.kicad_sch, read 2026-09-29].
6. **TAS5805M and TAS5825M need no MCLK**; their internal PLL references SCLK/BCLK [https://www.ti.com/lit/ds/symlink/tas5825m.pdf and https://www.ti.com/lit/ds/symlink/tas5805m.pdf, read 2026-09-29]. So the endpoint's sample rate is whatever BCLK the MCU makes; "is MCLK wired" does not matter for TAS boards.
7. **I2C addresses differ between the siblings.** TAS5825M: ADR resistor to GND, 0x4C/0x4D/0x4E/0x4F (0, 1k, 4.7k, 15k). TAS5805M: ADR resistor to DVDD, 0x2C/0x2D/0x2E/0x2F (4.7k, 15k, 47k, 120k) [same datasheets, read 2026-09-29]. BRIEF 5.5's "typically 0x4C" is right for the TAS5825M only; the TAS5805M fallback in BRIEF 9 changes the address.
8. **TAS5825M power is quoted at two distortion levels**: 24 V, 8 ohm BTL gives 30 W at 1% THD+N and 38 W at 10% [https://www.ti.com/lit/ds/symlink/tas5825m.pdf, read 2026-09-29]. BRIEF 5.5's "2x38 W at 24 V" is the 10% figure.
9. **The MA12070 escape hatch is weak.** The I2S part is the MA12070P (the MA12070 is analog-input) [https://www.infineon.com/dgdl/Infineon-MA12070P-DS-v01_00-EN.pdf?fileId=5546d46264a8de7e0164b761f2f261e4, read 2026-09-29], and it is reported discontinued: Infineon community "discontinued long back ... no next generation", and a TI E2E FAQ proposing TAS5827 / TAS5828M / TPA3221 / TPA3223 as crosses (both snippets; not an Infineon PCN; medium-high confidence).
10. **Pi I2S has no MCLK at all**, and on Pi 5 (RP1) a board is either clock producer or clock consumer, not both: "MCLK is NOT supported on any Raspberry Pi SBCs" (Raspberry Pi whitepaper RP-009699-WP-1, 2026-02-10) [https://pip-assets.raspberrypi.com/categories/1259-audio-camera-and-display/documents/RP-009699-WP-1-Using%20the%20I2S%20peripherals%20on%20Raspberry%20Pi%20SBCs.pdf, read 2026-09-29].
11. **snd_pcm_delay on Pi is burst-granular but excludes the DAC's own latency.** Both RP1 (`dw-axi-dmac`) and older (`bcm2835-dma`) DMA drivers set `DMA_RESIDUE_GRANULARITY_BURST`, so hw_ptr is sub-period; the PCM512x codec driver has no `.delay` callback, so the DAC filter group delay is not reported [https://github.com/raspberrypi/linux/blob/rpi-6.12.y/drivers/dma/dw-axi-dmac/dw-axi-dmac-platform.c and https://github.com/raspberrypi/linux/blob/rpi-6.12.y/sound/soc/codecs/pcm512x.c, read 2026-09-29]. PCM5122 "normal" filter group delay is 20/fs, about 417 us at 48 kHz [https://www.ti.com/lit/ds/symlink/pcm5122.pdf, read 2026-09-29]. That is nearly half the 1 ms target and must be identical across endpoints or calibrated per model; a mixed fleet (Pi + PCM5122 vs ESP32 + TAS5825M with its internal SRC/DSP) needs a measured per-model latency constant. No published snd_pcm_delay accuracy measurement on RP1 was found; chorus has to produce it.
12. **802.3af is 13 W at the PD** in the current standard's terms (12.95 W is the 2003 figure); 802.3at 25.5 W; 802.3bt Type 3 51 W, Type 4 71.3 W; peaks above the class limit are allowed for at most 50 ms per 1 s window (Class 4 peak 28.3 W) [https://ethernetalliance.org/wp-content/uploads/2019/12/WP_EA_Overview8023bt_V2p1_FINAL.pdf, read 2026-09-29]. BRIEF 5.5's numbers hold.

---

## 1. Off-the-shelf ESP32-family boards (amp/DAC and/or wired Ethernet/PoE)

Headline: **no product found in 2025-2026 puts an ESP32-S3 or P4, a TAS58xx amp, Ethernet and PoE on one board** (searches across CNX Software, Crowd Supply, Tindie, Hackaday) [https://www.cnx-software.com/2026/03/14/esparagus-audio-brick-esp32-based-din-rail-65w-hi-fi-amplifier-supports-home-assistant-squeezelite/, read 2026-09-29]. Every option is either "amp + optional W5500, no PoE" (Sonocotta) or "Ethernet/PoE, no real amp" (Olimex, Waveshare, LilyGO).

### 1.1 Sonocotta (Andriy Malyshenko; ships from Poland)

- All boards: ESP32-WROVER-N8R8 or ESP32-S3-WROOM-N8R8 (8 MB PSRAM); every board has a header for an optional W5500 module (S3 pins CLK 12, MOSI 11, MISO 13, CS 10, INT 6, RST 5); no PoE on any product; Louder boards do not route MCLK (only the PCM5122 "Plus" DAC boards, on GPIO0) [https://github.com/sonocotta/esp32-audio-dock, read 2026-09-29].
- Licence: esp32-audio-dock and esparagus-media-center repos are Apache-2.0 with schematic PDFs, Gerbers and STEP, **no KiCad sources**; the TAS5805M driver (esp32-tas5805m-dac) and snapclient fork are **GPL-3.0** (clean-room relevance) [https://api.github.com/users/sonocotta/repos?per_page=100, read 2026-09-29].
- **Louder-ESP32** (TAS5805M, 2x23 W 8 ohm at 1%, 5-26 V, W5500 add-on +$5): $24; sold out since 2026-09-24; Tindie page says "USA shipping temporarily suspended due to tariffs", alternatives Lectronz and Elecrow [https://www.tindie.com/products/sonocotta/louder-esp32/, read 2026-09-29]; Lectronz $24, out of stock [https://lectronz.com/products/louder-esp32, read 2026-09-29].
- **Louder-ESP32-Plus** (TAS5825M; vendor claims 2x32 W 8 ohm at 1%, above the datasheet's 30 W, treat as vendor claim): $29, sold out since 2026-09-08 [https://www.tindie.com/products/sonocotta/louder-esp32-plus/, read 2026-09-29].
- **Louder-ESP32-Mini** (S3, TAS5805M, 1x5 W, 5 V USB-C or USB-PD to 20 V, no Ethernet): $15, in stock [https://www.tindie.com/products/sonocotta/louder-esp32-mini/, read 2026-09-29].
- **Esparagus Audio Brick** (the one with Ethernet on board): ESP32 or S3, 16 MB flash, 8 MB PSRAM, TAS5825M (dual-TAS5825M 2.1 variant), W5500 onboard, 5-26 V, DIN rail, no PoE; $59; Crowd Supply: "ESP32-S3 version: Orders placed now ship Jan 25, 2027", ESP32 version ships now [https://www.crowdsupply.com/sonocotta/esparagus-audio-brick, read 2026-09-29]; Lectronz $59, in stock, variant not stated [https://lectronz.com/stores/sonocotta, read 2026-09-29].
- Amped-ESP32 (PCM5100A + TPA3118/3128 analog class-D, the DAC escape hatch as a product): $25, out of stock [https://www.tindie.com/products/sonocotta/amped-esp32/, read 2026-09-29].

### 1.2 Olimex (open hardware; ships from Bulgaria, ASSUMED; US via DigiKey/Mouser)

- **ESP32-POE**: 802.3af (TPS2375), LAN8720 RMII (ESP32-generated clock, so APLL is taken), WROVER-E 8 MB PSRAM option, OSHW; 17.95 EUR in stock [https://www.olimex.com/Products/IoT/ESP32/ESP32-POE/open-source-hardware, read 2026-09-29]; DigiKey $20.80, Mouser $21.31 (snippets).
- **ESP32-POE2**: the only off-the-shelf ESP32 board found that passes **PoE+ power to external loads**: "IEEE 802.3at PoE support ... Class 4", "Total output for external circuits - 25W max": 0.75 A at 24 V or 1.5 A at 12 V (jumper), no isolation; ESP32-WROVER-E-N4R8; LAN8710A RMII (ESP32-generated clock); 20.95 EUR in stock [https://www.olimex.com/Products/IoT/ESP32/ESP32-POE2/open-source-hardware, read 2026-09-29]. Licence: hardware CERN-OHL-S-2.0, software GPL-3, docs CC BY-SA 4.0; KiCad published [https://github.com/OLIMEX/ESP32-POE2, read 2026-09-29]. DigiKey $28.31, non-stock (snippet).
- **ESP32-P4-DevKit**: ESP32-P4NRW32 (32 MB PSRAM), 16 MB flash, no radio, IP101GRR PHY with its own 25 MHz crystal (the configuration Espressif recommends), no audio; 16.00 EUR in stock; CERN-OHL-S-2.0, KiCad [https://www.olimex.com/Products/IoT/ESP32-P4/ESP32-P4-DevKit/open-source-hardware, read 2026-09-29]. POEv3 add-on is 5 V/1 A only (4.95 EUR) [https://www.olimex.com/Products/IoT/ESP32-P4/POEv3/open-source-hardware, read 2026-09-29]. DigiKey $18.42 (snippet), a US source.

### 1.3 Waveshare (ships from China, ASSUMED; Amazon US listings exist)

- **ESP32-S3-ETH**: ESP32-S3R8, 16 MB flash, 8 MB PSRAM, W5500 (INT on GPIO10), optional 802.3af PoE module (output watts not stated), no audio; $16.99-25.99 across variants; schematic PDF [https://www.waveshare.com/esp32-s3-eth.htm and https://www.waveshare.com/wiki/ESP32-S3-ETH, read 2026-09-29].
- **ESP32-P4-NANO**: P4NRW32, 32 MB PSRAM, ESP32-C6 companion, 100M RJ45 (IP101GRI, snippet), PoE module header, ES8311 codec + NS4150B 2 W mono; $18.99-86.99 [https://www.waveshare.com/esp32-p4-nano.htm, read 2026-09-29]. The board under Espressif's AES67 example.
- **ESP32-P4-ETH**: P4NRW32, 32 MB PSRAM, 100M RJ45, PoE optional, ES8311 + NS4150B, no radio; $12.99-74.99 [https://www.waveshare.com/esp32-p4-eth.htm, read 2026-09-29]. PHY and PoE class not stated.
- **ESP32-P4-WIFI6-POE-ETH**: PoE on board (class not stated); $24.99-79.99 [https://www.waveshare.com/esp32-p4-wifi6-poe-eth.htm, read 2026-09-29].

### 1.4 LilyGO (ships from China, ASSUMED)

- **T-ETH-Lite** (ESP32 + RTL8201, or ESP32-S3 + W5500; 8 MB PSRAM; PoE shield): $14.88, sold out [https://lilygo.cc/products/t-eth-lite, read 2026-09-29]. **T-ETH-Elite** (S3, W5500, 802.3af 36-57 V per CNX snippet): $23.04, sold out [https://lilygo.cc/en-us/products/t-eth-elite-1, read 2026-09-29].

### 1.5 Espressif and others

- **ESP32-Ethernet-Kit-VE**: ESP32-WROVER-E, IP101GRI, PoE board "IEEE 802.3at", "5 V, 1.4 A" out; RMII clock selectable from the PHY (keeps APLL free) [https://docs.espressif.com/projects/esp-dev-kits/en/latest/esp32/esp32-ethernet-kit/user_guide.html, read 2026-09-29]; DigiKey $55.00 (snippet).
- **ESP32-P4X-Function-EV-Board**: RMII Ethernet, ES8311 + NS4150B 3 W, no PoE; DigiKey $59.92, Mouser $59.74 (snippets) [https://docs.espressif.com/projects/esp-dev-kits/en/latest/esp32p4/esp32-p4-function-ev-board/user_guide.html, read 2026-09-29].
- **wESP32** (Silicognition, US seller): ESP32-WROOM-32, RTL8201FI, "IEEE 802.3at Type 1 Class 0 compliant PoE", "12 V output ... 12.95+ W", isolated; $69 board, in stock [https://www.crowdsupply.com/silicognition/wesp32, read 2026-09-29]. No PSRAM (ASSUMED from WROOM).
- **Not fit for audio**: M5Stack PoESP32 (6 W, no PSRAM, Grove port only) [https://docs.m5stack.com/en/unit/poesp32, read 2026-09-29]; WT32-ETH01 (no PSRAM, no PoE; its 50 MHz oscillator does keep APLL free) (snippet).
- **Squeezelite-ESP32 hardware**: SqueezeAMP is PCB-order only (DipTrace, no Ethernet) [https://github.com/philippe44/SqueezeAMP, read 2026-09-29]; Raspiaudio Muse boards are Wi-Fi only (ASSUMED). The squeezelite-esp32 firmware supports LAN8720 RMII and W5500/DM9051 SPI Ethernet, S3 "experimental" (snippet).
- **Other 2025-2026 S3 + W5500 + PoE boards without audio**: Seeed XIAO W5500 PoE adapter (2025) and Makerfabs MaUWB (2026-07) (CNX snippets).

---

## 2. Amplifier chips and hobbyist modules

| Part | Datasheet rev (read 2026-09-29) | PVDD | Power (datasheet) | I2C 7-bit | Rates | Package | Status | Price |
|---|---|---|---|---|---|---|---|---|
| TI TAS5825M | SLASEH7H, Rev H, Jan 2023 [https://www.ti.com/lit/ds/symlink/tas5825m.pdf] | 4.5-26.4 V | 24 V 8 ohm BTL: 30 W at 1%, 38 W at 10%; PBTL 22 V 4 ohm 53 W at 1% | 0x4C-0x4F (ADR to GND) | 32-192 kHz, no MCLK | VQFN-32 5x5 | ACTIVE [https://www.ti.com/product/TAS5825M] | DigiKey $3.97, Mouser $3.15 (snippets); LCSC 62 pcs (snippet) |
| TI TAS5805M | SLASEH5D, Rev D, Nov 2020 [https://www.ti.com/lit/ds/symlink/tas5805m.pdf] | 4.5-26.4 V | 21 V 8 ohm BTL: 23 W at 1%; 12 V 6 ohm 9.9 W at 1%; no 24 V row; min BTL load 6 ohm nominal | 0x2C-0x2F (ADR to DVDD) | 32-96 kHz only | HTSSOP-28 | ACTIVE [https://www.ti.com/product/TAS5805M] | LCSC $0.99 (1), 8,471 in stock [https://www.lcsc.com/product-detail/C478472.html]; Mouser $2.25 (snippet) |
| Infineon MA12070P | V1.1, 2022-02-09 | 4-26 V + 5 V rail | 2x30 W (8 ohm, 22 V, 10%), 2x80 W peak | 0x20-0x23, 100 kbps only | 44.1-192 kHz, needs a CLK phase-locked at 64-512 x fs | QFN-64 | reported discontinued (snippets) | LCSC $6.40, 41 pcs [https://www.lcsc.com/product-detail/C538534.html] |
| TI PCM5102A (DAC) | Rev C, 2015 [https://www.ti.com/product/PCM5102A] | n/a | line out | none (hardware pins) | 8-384 kHz; PLL from BCK, no MCLK | TSSOP-20 | ACTIVE | Amazon US module 2-pack $10.91 (snippet) |

- TI EVMs: TAS5825MEVM $175 at TI (snippet) or $229.69 DigiKey, and it needs the PUREPATH-CMBEVM motherboard; TAS5805MEVM $296.62 DigiKey [https://www.findchips.com/search/tas5825mevm and https://www.findchips.com/search/tas5805mevm, read 2026-09-29]. ti.com showed both chips and EVMs "Out of stock" and rendered no prices (JS page; possibly an artefact, UNCERTAIN).
- PurePath Console 3 and device software are "Request access" gated on both product pages [https://www.ti.com/product/TAS5825M, read 2026-09-29]. With the DSP bypassed chorus may not need it (ASSUMED).
- **Hobbyist TAS58xx boards**: the only purchasable retail ones found are Sonocotta's (section 1.1). AliExpress showed only bare chips; no standalone TAS5805M/TAS5825M I2S breakout with US stock was found (Banggood, Wondom/Parts Express, AliExpress, Amazon searched) [https://www.aliexpress.com/w/wholesale-tas5805m.html and https://www.banggood.com/buy/tas5825m-amp.html, read 2026-09-29]. A Hackaday.io TAS5825M breakout exists as a bare OSH Park PCB [https://hackaday.io/project/175727-tas5825m-i2s-audio-amp, read 2026-09-29]. A Raspberry Pi 2.1 HAT with TAS5825M appeared in 2026-02 (CNX snippet).
- **Escape hatch (DAC + analog class-D)**: PCM5102A modules (Amazon US, HiLetgo and others) plus a TPA3116D2 board, Parts Express $15.98 (snippet, US) or Amazon 2-pack $22.49 (snippet); TPA3255 boards want 24-48 V, beyond an 802.3at splitter (listings, snippet).

---

## 3. Linux endpoints

### 3.1 Boards (US reseller PiShop.us ships from New Castle, DE [https://support.pishop.us/article/42-where-do-you-ship-orders-to-and-how, read 2026-09-29])

- Raspberry Pi raised prices four times on LPDDR4 cost (2025-10-01, 2025-12-01, 2026-02-02, 2026-04-01) [https://www.raspberrypi.com/news/more-memory-driven-price-rises/ and https://www.raspberrypi.com/news/a-new-3gb-raspberry-pi-4-for-83-75-and-more-memory-driven-price-increases/, read 2026-09-29].
- Pi 5 today at PiShop: 1GB $45 (out of stock), **2GB $65 (in stock)**, 4GB $110, 8GB $175 (CanaKit also $175), 16GB $305 [https://www.pishop.us/product/raspberry-pi-5-2gb/ and neighbours, read 2026-09-29].
- Pi Zero 2 W: $17.25 at PiShop (list $15), Wi-Fi only, no wired Ethernet [https://www.pishop.us/product/raspberry-pi-zero-2-w/ and https://www.raspberrypi.com/products/raspberry-pi-zero-2-w/, read 2026-09-29]. Not a wired-tier candidate without a USB NIC.
- CM5: 2GB Lite $67.50, 4GB Lite $100 (PiShop); CM5 IO Board $20 (CanaKit) or $26.95 (PiShop, "REV 2"); the CM5 page says "Gigabit Ethernet PHY supporting IEEE 1588" [https://www.raspberrypi.com/products/compute-module-5/ and https://www.canakit.com/raspberry-pi-compute-module-5-io-board.html, read 2026-09-29].

### 3.2 PoE for Pi

- **The official Pi 5 PoE+ HAT is still not released**: a Raspberry Pi engineer, 2026-05-23: "still under development, however things like the RAM crisis have meant changed priorities" [https://forums.raspberrypi.com/viewtopic.php?p=2377088, read 2026-09-29].
- Waveshare **PoE HAT (F)** for Pi 5/CM5: 802.3af/at, 5 V 4.5 A on GPIO plus 12 V 2 A on a header; $19.99 waveshare.com (China), **$28.95 in stock at PiShop (US)** [https://www.waveshare.com/poe-hat-f.htm and https://www.pishop.us/product/power-over-ethernet-hat-f-for-raspberry-pi-5-cooling-fan-802-3af-at/, read 2026-09-29].
- A PoE HAT plus a DAC HAT on one 40-pin header is a likely stacking conflict (ASSUMED, not verified per product). For a bench, USB-C supplies avoid it; DigiAMP+ and HiFiBerry amps power the Pi from their 12-24 V input instead.

### 3.3 I2S DAC / amp HATs (clock role matters for drift and snd_pcm_delay)

| Board | Chip | Output | Clock role | Overlay | US price (PiShop, in stock) | Maker price |
|---|---|---|---|---|---|---|
| Raspberry Pi DAC+ | PCM5122 | 2 Vrms line | Pi master (ASSUMED; no oscillator mentioned) | `rpi-dacplus` | $29.95 | |
| Raspberry Pi DAC Pro | PCM5242 | line, balanced | Pi master (ASSUMED) | `rpi-dacpro` | $35.95 | |
| Raspberry Pi DigiAMP+ | TAS5756M | 2x25-35 W, 12-24 V in, powers the Pi | Pi master (ASSUMED) | `rpi-digiampplus` | $42.95 | |
| HiFiBerry Amp4 | TAS5756M | up to 2x30 W | slave (Pi master), no on-board clock | `hifiberry-dacplus-std` | $98.45 | $64.90 (Switzerland) |
| HiFiBerry Amp4 Pro | TAS5756M (retailer claim) | 38 W/ch 24 V 8 ohm (<10%) | dual-domain clock, master by default | `hifiberry-amp4pro` | $114.25 | $74.90 |
| HiFiBerry DAC2 Pro | Burr-Brown (PCM5122 family ASSUMED) | line + headphone | own clock (master) or `-std` slave | `hifiberry-dacplus-pro` | $73.75 | $44.90 |
| HiFiBerry DAC2 HD | Burr-Brown (PCM1796 per snippet) | line | own low-jitter clock | `hifiberry-dacplushd` | $164.15 | $109 |
| InnoMaker HiFi DAC HAT | PCM5122 | line + headphone | own oscillators (45.158/49.152 MHz), DAC master | not stated (UNCERTAIN) | $26.95 | $32.99 (China) |

Sources: [https://www.raspberrypi.com/documentation/accessories/audio.html, https://www.hifiberry.com/shop/boards/hifiberry-amp4/, https://www.hifiberry.com/docs/data-sheets/datasheet-amp4-pro/, https://www.pishop.us/product/hifiberry-amp4/, https://www.inno-maker.com/product/hifi-dac-hat/, all read 2026-09-29]; full row-by-row sources in `research-linux-rig.md`. HiFiBerry ships from Switzerland; US orders go by courier and need CBP form 5106 [https://www.hifiberry.com/blog/us-shipping-and-customs-processing/, read 2026-09-29].

Which expose accurate ALSA delay: all I2S HATs share the same Pi-side DMA path (burst-granular hw_ptr, finding 11); none of the codec drivers read adds the DAC's own group delay, so "accurate" means "sub-period, plus a constant you must calibrate". For sync measurement the simplest clock model is a **Pi-master (slave-DAC) board**, where the Pi crystal is the only clock; HiFiBerry itself measured 4-6 Hz pilot drift with a DAC2 Pro in master mode against a remote encoder and recommends slave mode when "the audio stream carries its own timing reference" [https://www.hifiberry.com/blog/techtalk-choose-the-right-clocking-for-your-mpx-setup/, read 2026-09-29]. An alsa-lib bug (open since 2025-07-31) makes `snd_pcm_avail_delay()` return avail and delay out of sync at high rates on Intel SOF [https://github.com/alsa-project/alsa-lib/issues/468, read 2026-09-29]; relevant to the Rust client's choice of call.

---

## 4. Measurement rig

### 4.1 Two-input USB audio interfaces (analog cross-correlation, BRIEF 10)

| Interface | Max rate | Linux | Inputs | Price (store, ship-from) |
|---|---|---|---|---|
| Behringer UMC202HD | 192 kHz | UAC2 (USB 1397:0507), snd-usb-audio; capture fine; playback needed `implicit_fb=1` or kernel > 5.15.13 [https://linux-hardware.org/index.php?id=usb:1397-0507 and https://nandakumar.org/blog/2022/02/umc202hd-linux.html, read 2026-09-29] | 2x symmetric combo, pads | $89 Sweetwater, US (snippet); $58 Thomann, Germany [https://www.thomannmusic.com/behringer_u_phoria_umc202hd.htm, read 2026-09-29] |
| Focusrite Scarlett 2i2 4th gen | 192 kHz | class compliant, not officially supported; scarlett2 mixer driver in Linux 6.8; disable MSD mode (snippets) | 2x combo, +22 dBu line (snippet) | $224.99, us.focusrite.com, in stock [https://us.focusrite.com/products/scarlett-2i2, read 2026-09-29] |
| MOTU M2 | 192 kHz | class compliant on Mac/iOS per MOTU; the M4 sibling works on Linux (kernel 5.8+ for duplex) [https://motu.com/en-us/products/m-series/m2/specs/ and https://panther.kapsi.fi/posts/2020-02-02_motu_m4, read 2026-09-29] | 2x combo | $225 Thomann (Germany); Sweetwater open box $179.96 (snippet) |
| Audient EVO 4 | 96 kHz | "USB 2.0, class-compliant" (Thomann) | 2x combo + DI | $99 Thomann [https://www.thomannmusic.com/audient_evo_4.htm, read 2026-09-29] |
| Avoid | | | Audient iD4 MkII (only one line-capable input), Behringer UMC22 (48 kHz, asymmetric inputs), UCA202/222 (16-bit 48 kHz RCA) | |

No source measured L/R input skew on any of these; USB transport jitter is common to both channels of one stream (ASSUMED). Calibrate the rig once by splitting one source into both inputs. One sample at 96 kHz is 10.4 us (BRIEF 10); decision 0013 already fixes 96 kHz analysis windows.

### 4.2 Logic analyzers (GPIO-at-I2S-write cross-check)

| Device | Rate / resolution | sigrok/PulseView | Price (store) |
|---|---|---|---|
| fx2lafw 24 MHz 8 ch clone | 24 MHz, 41.7 ns; streaming only, no buffer | yes (fx2lafw) [https://sigrok.org/wiki/Fx2lafw, read 2026-09-29] | $26.95 SparkFun, US, in stock [https://www.sparkfun.com/usb-logic-analyzer-24mhz-8-channel.html, read 2026-09-29]; a few dollars on AliExpress (snippet) |
| DreamSourceLab DSLogic Plus | stream 100 MHz on 3 ch (10 ns), 16G-sample depth; buffered 400 MHz on 4 ch | yes [https://sigrok.org/wiki/DreamSourceLab_DSLogic, read 2026-09-29] | $149, in stock, ship-from not stated (China ASSUMED) [https://www.dreamsourcelab.com/shop/logic-analyzer/dslogic-plus/, read 2026-09-29] |
| DSLogic U3Pro16 | 1 GHz stream on 3 ch | no (DSView only, GPL-3.0) | $299 [https://www.dreamsourcelab.com/shop/logic-analyzer/dslogic-u3pro16/, read 2026-09-29] |
| Raspberry Pi Pico / Pico 2 + sigrok-pico | up to 120 Msps theoretical, "<=60 Msps" reliable for 8+ ch; RLE on <=4 ch | yes, mainline since 2023-09 [https://github.com/pico-coder/sigrok-pico, read 2026-09-29] | $3.95 / $5.00 PiShop, US [https://www.pishop.us/product/raspberry-pi-pico/, read 2026-09-29] |
| Saleae Logic 8 | 100 MS/s | not supported ("planned") | $499, sold out [https://store.saleae.com/products/logic-8, read 2026-09-29] |

Two sparse GPIO edges per marker suit any of these; the fx2lafw clears a sub-microsecond goal by more than 20x, but USB hiccups can drop samples (ASSUMED); run it at 12-16 MHz on two channels for margin. On Linux endpoints the GPIO marker is toggled from userspace, so it measures the software path, not the I2S frame; on Pi 5 GPIO goes through RP1 over PCIe (a low-quality source claims 10-50 us toggle latency [https://industrialmonitordirect.com/blogs/knowledgebase/raspberry-pi-5-rp1-gpio-timing-issues-workarounds-and-solutions, read 2026-09-29 via search summary], UNCERTAIN; measure it). A draft kernel PR opened 2026-09-29 adds RP1 PHC GPIO timestamping and PPS out [https://github.com/raspberrypi/linux/pull/7659, read 2026-09-29], not merged.

### 4.3 PTP / hardware timestamping (only if relevant)

- Pi 5 MAC timestamping works with linuxptp; one user reported about +/-5.5 us through ordinary switches [https://forums.raspberrypi.com/viewtopic.php?t=358275, read 2026-09-29]. CM5 PHY supports IEEE 1588 (above).
- Omada: only the S6500 and S7500 series support PTP (FAQ updated 2026-08-19) [https://support.omadanetworks.com/us/document/51076/, read 2026-09-29]. The owner's switch model is unknown.
- Intel I210-T1, Intel RCP $49 (snippet); i226 timing NICs with PPS about $187-200 (snippets).
- Verdict: not needed for a sub-millisecond target (BRIEF 5.3); a PTP-disciplined pair (P4 or CM5) is a possible independent cross-check later.

---

## 5. PoE budget facts

- Power at the PD: 802.3af 13 W, 802.3at 25.5 W (PSE 30 W), 802.3bt Type 3 51 W, Type 4 71.3 W; peak allowance 50 ms per 1 s (Class 4 peak 28.3 W) [https://ethernetalliance.org/wp-content/uploads/2019/12/WP_EA_Overview8023bt_V2p1_FINAL.pdf, read 2026-09-29].
- Omada PoE+ switches (all 802.3af/at only, no bt): TL-SG2210MP 150 W (end of sale), TL-SG2428P 250 W (end of sale), TL-SG3428MP 384 W, 30 W per port [https://www.omadanetworks.com/us/business-networking/omada-switch-access/tl-sg3428mp/ and neighbours, read 2026-09-29]. A bt PD on these falls back to 25.5 W (ASSUMED from the standard).
- Splitters with 12 V / 24 V out (Amazon US listings; prices not rendered): TP-Link TL-POE10R is 802.3af, 12 V at 1 A max (12 W, too little) [https://static.tp-link.com/2017/201712/20171205/TL-POE10R(UN)5.0%20Datasheet.pdf, read 2026-09-29]; DSLRKIT 802.3at 12 V 2 A; PoE Texas 48-to-24 V 25 W; Procet PT-PD208GT 12 V 2.15 A or 24 V 1.05 A (25 W); Tycon POE-SPLT-4824G-P 24 V 24 W; Procet PT-PD208GBH-24 24 V 2.96 A from a bt source [Amazon URLs in `research-amps-poe.md`, read 2026-09-29]; UCTRONICS 802.3at gigabit splitter U5259 $24.99 (snippet) [https://www.uctronics.com/poe-splitter.html].
- PD modules for a custom board (Silvertel selector guide v7.0, January 2026; PDF columns partly scrambled, medium confidence) [https://silvertel.com/images/shortforms/Selector_GuidePoE.pdf, read 2026-09-29]: Ag9912/Ag9924 802.3af 12/24 V about 12-13 W (DigiKey $9, snippet); Ag5300/Ag53012/Ag53024 802.3at Class 4, 24 W continuous (Ag5300 DigiKey $16, snippet); Ag59612/Ag59624 802.3bt Class 6, 51 W. TI TPS2372/TPS23758 reference designs not priced (gap).
- Boards that do 802.3at: Olimex ESP32-POE2 (25 W total to loads, 24 V 0.75 A = 18 W on the 24 V rail), Espressif ESP32-Ethernet-Kit PoE board (5 V 1.4 A only), wESP32 (at-compliant but Class 0, 12.95 W on 12 V), Waveshare PoE HAT (F) (12 V 2 A). **No board found does 802.3bt.**
- Arithmetic (mine, ASSUMED efficiencies): 25.5 W at the PD, about 90% conversion, a few watts for MCU and Ethernet, leaves roughly 18-20 W average for the amp. Music crest factor, the 50 ms peak allowance, bulk capacitance and the amp's limiter decide whether that is enough; measure it. Eight endpoints at 25.5 W is 204 W, above a 150 W TL-SG2210MP.

---

## 6. Prior art in 2025-2026 (context only; clean-room rule: study, not adopt)

| Project | What changed | Licence | Why it matters |
|---|---|---|---|
| Snapcast | v0.35.0 on 2026-03-10; repo moved from badaix/snapcast to snapcast/snapcast [https://github.com/snapcast/snapcast/releases, read 2026-09-29] | GPL-3.0 [https://github.com/snapcast/snapcast, read 2026-09-29] | BRIEF 11's URL is now a redirect; the clean-room rule is unchanged |
| snapcast-rs (metaneutrons) | Rust reimplementation, "compatible with the original C++ Snapcast over the TCP audio transport"; 0.18.0 on 2026-09-12 (search summary) [https://github.com/metaneutrons/snapcast-rs, read 2026-09-29] | GPL-3.0-only | Same language as chorus, GPL: highest paraphrase risk; do not read its sync code closely |
| Sendspin (Open Home Foundation / Music Assistant; formerly "Resonate") | Open protocol: WebSocket, four-timestamp `client/time`/`server/time`, 2-D Kalman filter over offset and drift, Noise-encrypted payloads, versioned roles [https://github.com/Sendspin/spec, read 2026-09-29]; Music Assistant's native player protocol [https://www.music-assistant.io/player-support/sendspin/, read 2026-09-29]; ESPHome component "experimental", protocol "not yet finalized" [https://esphome.io/components/sendspin/, read 2026-09-29]; drift corrected by inserting/removing interpolated samples [https://www.xda-developers.com/sendspin-esphome-multi-room-audio/ (2025-12-17), read 2026-09-29] | Spec: Community Specification License 1.0; time-filter and sendspin-cpp: Apache-2.0 [https://github.com/Sendspin/time-filter, read 2026-09-29; sendspin-cpp via search] | The closest living analogue to chorus, permissively licensed. Kalman vs BRIEF 5.3's min-RTT + median is a useful counterpoint. No published accuracy number |
| SendspinZero (DIY) | ESP32-S3-Zero + PCM5102A, Wi-Fi only, about $10-12 of parts [https://www.cnx-software.com/2026/04/21/diy-sendspin-audio-receiver-supports-multi-room-audio-synchronization-integrates-with-home-assistant/, read 2026-09-29] | open (licence not stated) | Wi-Fi tier reference only |
| aes67-esp32p4 (DatanoiseTV) | AES67 on ESP32-P4-NANO (IP101 RMII, ES8311), PTP with EMAC hardware timestamps, APLL steering; "0.7ms end-to-end latency" at 0.125 ms ptime [https://github.com/DatanoiseTV/aes67-esp32p4, read 2026-09-29] | Apache-2.0 | Proves the P4 path; relevant to the TV phase and to resample-vs-insert |
| esp_ptp (scrambletools) | PTP/gPTP for ESP32-P4 (hardware) and ESP32-C6 (software clock) [https://github.com/scrambletools/esp_ptp, read 2026-09-29] | not shown (UNCERTAIN) | Cross-check tool |
| shairport-sync 5.x | Version 5 adds AirPlay 2 surround and lossless; the announcement discussion shows "Feb 23, 2025" [https://github.com/mikebrady/shairport-sync/discussions/1983, read 2026-09-29]; 5.5.x is current [https://github.com/mikebrady/shairport-sync/releases, read 2026-09-29]. Release years conflict across sources (UNCERTAIN) | MIT-style (ASSUMED) | Still BRIEF 11's NQPTP/DAC-delay reference |
| squeezelite-esp32 | ESP32 plus experimental S3; LAN8720 RMII and W5500/DM9051 SPI Ethernet (snippet) [https://github.com/sle118/squeezelite-esp32] | GPL (ASSUMED) | Its board configs map which boards work wired |
| CarlosDerSeher/snapclient | Olimex ESP32-PoE and WT32-ETH01 configs; TAS5805M, PCM51xx, PCM5102A, MA120x0 [https://github.com/CarlosDerSeher/snapclient, read 2026-09-29] | GPL-3.0 | Already BRIEF 11 |
| Roc Toolkit | Latest v0.4.0 (2024-06-14); nothing newer [https://github.com/roc-streaming/roc-toolkit/releases, read 2026-09-29] | MPL-2.0 (ASSUMED) | FEC reference for the TV path |
| PipeWire AES67 | RTP sink/source modules with a PTP clock node [https://docs.pipewire.org/page_module_rtp_sink.html (snippet)] | MIT (ASSUMED) | A Linux AES67/PTP reference |
| ESP-ADF multi-room | Espressif Wi-Fi "Multi-Room Music" example; users report sync failures after long playback [https://github.com/espressif/esp-adf/issues/886 (snippet)] | not checked | Negative example |

---

## 7. Comparison table (endpoint candidates)

| Candidate | MCU | APLL for I2S | Wired | PoE power to amp | Amp/DAC | PSRAM | Open HW / schematics | Price, store, ship-from | Stock |
|---|---|---|---|---|---|---|---|---|---|
| Sonocotta Louder-ESP32 (S3) | ESP32 or S3 | ESP32 variant only | W5500 add-on (+$5) | none (5-26 V in) | TAS5805M | 8 MB | Apache-2.0, PDF only | $24, Tindie/Lectronz, Poland; Tindie US shipping suspended | sold out |
| Sonocotta Louder-ESP32-Plus | ESP32 or S3 | ESP32 variant only | W5500 add-on | none | TAS5825M | 8 MB | Apache-2.0, PDF only | $29, Poland | sold out |
| Sonocotta Esparagus Audio Brick | ESP32 or S3 | ESP32 variant only | W5500 onboard | none (5-26 V) | TAS5825M | 8 MB | Apache-2.0, PDF only | $59, Lectronz (Poland) / Crowd Supply | Lectronz in stock; S3 ships 2027-01-25 |
| Olimex ESP32-POE2 | ESP32 | taken by RMII clock | LAN8710A RMII | 802.3at, 25 W to loads (24 V 0.75 A) | none | 8 MB | CERN-OHL-S-2.0, KiCad | 20.95 EUR olimex; DigiKey $28.31 non-stock | in stock (Olimex) |
| Olimex ESP32-POE | ESP32 | taken | LAN8720 RMII | 802.3af, 5 V only | none | 8 MB (WROVER) | OSHW | DigiKey $20.80 / Mouser $21.31 (US) | in stock |
| Waveshare ESP32-S3-ETH | S3 | no | W5500 | 802.3af module, watts unstated | none | 8 MB | schematic PDF | $16.99-25.99, China; Amazon US listing | listed |
| LilyGO T-ETH-Lite S3 | S3 | no | W5500 | PoE shield | none | 8 MB | not checked | $14.88, China | sold out |
| Olimex ESP32-P4-DevKit | P4 | yes, + EMAC 1588 | IP101GRR RMII | POEv3 5 V 1 A | none | 32 MB | CERN-OHL-S-2.0, KiCad | 16 EUR; DigiKey $18.42 (US) | in stock |
| Waveshare ESP32-P4-NANO / P4-ETH | P4 | yes, + EMAC 1588 | IP101 RMII | PoE module (class unstated) | ES8311 + 2 W mono | 32 MB | schematic PDF | $18.99+ / $12.99+, China; Amazon US listings | listed |
| wESP32 | ESP32 | not checked | RTL8201FI RMII | 802.3at Class 0, 12.95 W at 12 V | none | none (ASSUMED) | not checked | $69 Crowd Supply, US | in stock |
| Pi 5 2GB + RPi DAC+ | BCM2712 | n/a (Pi master, no MCLK) | GbE, PHC | PoE HAT (F) $28.95 (stacking UNVERIFIED) | PCM5122 line | 2 GB | n/a | $65 + $29.95 PiShop, US | in stock |
| Pi 5 2GB + DigiAMP+ | BCM2712 | n/a | GbE | amp powers Pi from 12-24 V | TAS5756M | 2 GB | n/a | $65 + $42.95 PiShop, US | in stock |
| CM5 2GB Lite + IO board | BCM2712 | n/a | GbE, PHY IEEE 1588 | PoE+ HAT+ support on IO board | via 40-pin HAT | 2 GB | IO board design files (ASSUMED) | $67.50 + $20-26.95, US | in stock |

---

## 8. Recommendation

### Option A (RECOMMENDED for now): minimal two-endpoint wired bench, all bought

Shaped by BRIEF 5.4 ("develop the client logic on Linux first") and roadmap phases 3-5, and by the US-first rule. Prices are today's; items marked ASSUMED are estimates.

Core (phases 3-4, two wired Linux clients plus the rig), all US ship-from:

| Item | Qty | Each | Store | Line |
|---|---|---|---|---|
| Raspberry Pi 5 2GB | 2 | $65.00 | PiShop.us (limit 1 per order noted) | $130.00 |
| Raspberry Pi DAC+ (PCM5122, Pi-master clock, line out) | 2 | $29.95 | PiShop.us | $59.90 |
| Pi 5 USB-C 27 W PSU, microSD, case | 2 | about $25 (ASSUMED) | PiShop.us | about $50 |
| Behringer UMC202HD (192 kHz, UAC2) | 1 | $89 (snippet) | Sweetwater | $89 |
| fx2lafw 24 MHz 8 ch logic analyzer | 1 | $26.95 | SparkFun | $26.95 |
| Cables (RCA to 1/4" TS, jumpers, one RCA Y-splitter for rig self-calibration) | 1 set | about $25 (ASSUMED) | any US | about $25 |
| **Core total** | | | | **about $380** |

Embedded add-on (phase 5), two paths, owner to pick:
- A1, line-level S3 now (sync first, amp later): 2x Waveshare ESP32-S3-ETH with PoE module (up to $25.99 each, Amazon US listing; ship-from per listing unverified) + a PCM5102A module 2-pack ($10.91, Amazon US, snippet). About $65. Measures the S3 + W5500 sync story at line level, the same way as the Pi pair, with no TAS bring-up risk. The TAS5825M arrives with Option B or A2.
- A2, TAS5825M now: 2x Sonocotta Louder-ESP32-Plus S3 ($29 + $5 W5500 each, sold out) or 2x Esparagus Audio Brick ($59, Lectronz in stock; S3 variant availability on Lectronz not stated), plus 2x 802.3at 24 V splitters (about $25 each, UCTRONICS U5259 snippet, or Procet PT-PD208GT, price not rendered). About $120-170. **Fails US-first**: Poland only, Tindie US shipping suspended, and no same-part US source exists; an explicit owner exception.
- Upgrade slot if the budget allows: DSLogic Plus ($149) instead of the fx2lafw for buffered 10 ns capture, and a Scarlett 2i2 4th gen ($224.99) instead of the UMC202HD (the UMC202HD is enough for 96 kHz).

Core plus A1: **about $445**. Core plus A2: **about $500-550** (non-US lines).

### Option B (later): a custom chorus endpoint board via the devices program

Once the Option A bench has frozen the design (BRIEF 5.5: "only consider a custom PCB after the design is proven and frozen"). The devices program already runs SKiDL + KiCad 10 + Freerouting with JLC assembly (local `devices-brief.md` lines 110-113, 232, 586). Parts are LCSC/JLC stock, which the US-first rule exempts when JLC places them.

| Block | Part | Price / stock | Source |
|---|---|---|---|
| MCU | ESP32-S3-WROOM-1-N16R8 (octal PSRAM, GPIO 35-37 reserved per BRIEF 6), or an ESP32-P4 module if the owner moves to P4 | $5.18 (1), LCSC, 1,736 in stock (snippet) | [https://www.lcsc.com/product-detail/WiFi-Modules_Espressif-Systems-ESP32-S3-WROOM-1-N16R8_C2913202.html] |
| Ethernet (S3) | WIZnet W5500, JLC C32843 | $2.72 (1), 27,831 in stock (snippet) | [https://jlcpcb.com/partdetail/WIZNET-W5500/C32843] |
| Ethernet (P4) | IP101GRI/GRR PHY with its own 25 MHz crystal (Olimex P4-DevKit KiCad as the open reference) | not priced | [https://github.com/OLIMEX/ESP32-P4-DevKit, read 2026-09-29] |
| Amp | TAS5825M (RHB) or TAS5805M (JLC C478472, "Economic and Standard" PCBA part) | TAS5825M DigiKey $3.97 (snippet); TAS5805M LCSC $0.99, 8,471 in stock | [https://jlcpcb.com/partdetail/TexasInstruments-TAS5805MPWPR/C478472, read 2026-09-29] |
| PoE PD | Silvertel Ag53024 / Ag5300 (802.3at, 24 W) or a TPS2378-class discrete design (Olimex ESP32-POE2 KiCad is the open reference for a 24 V PoE+ rail) | Ag5300 DigiKey $16 (snippet) | [https://silvertel.com/images/shortforms/Selector_GuidePoE.pdf, read 2026-09-29] |
| Audio clock (S3 only) | optional audio oscillator or programmable synth into MCLK-in (`I2S_CLK_SRC_EXTERNAL`) to replace the missing APLL | not priced (ASSUMED option) | finding 1 |
| Test points | a marker GPIO and the I2S LRCLK brought to a header for the logic analyzer | n/a | BRIEF 10 |

Estimated per board at qty 5, assembled: about $45-60 (ASSUMED: module, W5500, amp and PD from the table plus a magjack, passives, 4-layer PCB and JLC assembly fees; not quoted). Licence note: Olimex references are CERN-OHL-S-2.0 (strongly reciprocal) and Sonocotta's are Apache-2.0 PDFs; reading them is fine, copying layout pulls in the licence. Treat as a gray-zone call for the decision log (BRIEF 3.2). The TAS5805M ESP-IDF driver in the wild is GPL-3.0 (clean-room rule applies).

---

## 9. Grounded questions for the owner (bench hardware)

1. **MCU**: the S3 has no APLL and no EMAC. Keep the S3 (rate trimming by sample insert/delete only, which BRIEF 5.3 already chose, plus an optional external MCLK on a custom board), or promote the ESP32-P4 (APLL, RMII EMAC with IEEE 1588, 32 MB PSRAM, Olimex DevKit $18.42 at DigiKey) to the reference embedded endpoint?
2. **TAS5825M sourcing vs US-first**: the only purchasable TAS5825M + ESP32 boards ship from Poland, sold out or with US shipping suspended, and no US same-part source exists. Grant an exception (A2), measure sync at line level first (A1) and put the TAS on the custom board, or both?
3. **Which Omada switch exactly?** Budgets are 150/250/384 W depending on model, none do 802.3bt, and PTP exists only on S6500/S7500.
4. **What is already on the bench?** Any Pis, DAC HATs, a USB audio interface, a logic analyzer, a scope, ESP32 boards, PoE splitters (the inventory program may know)?
5. **Measurement budget**: UMC202HD (about $58-89) vs Scarlett 2i2 / MOTU M2 (about $225); fx2lafw ($27) vs DSLogic Plus ($149)?
6. **Linux clock model**: Pi-master DACs (RPi DAC+, one clock per endpoint, recommended for the sync phase) vs DAC-master "Pro" boards (independent oscillator the Pi cannot trim)?
7. **Pi power on the bench**: USB-C supplies, or PoE via Waveshare PoE HAT (F) (official Pi 5 PoE+ HAT still unreleased; HAT stacking with a DAC UNVERIFIED)?
8. **Custom board**: built by the devices program (SKiDL/KiCad/JLC), and when: after phase 5 on bought boards, or in parallel? PoE class target per speaker class (at 25.5 W is the switch's ceiling)?
9. **Hardware licence stance**: may a chorus board derive from CERN-OHL-S (Olimex) or Apache-2.0 (Sonocotta) designs, or study-only like GPL code?
10. **Fleet latency constants**: accept a per-model measured latency constant (PCM5122 about 417 us at 48 kHz vs TAS5825M unknown), or require one amp/DAC family everywhere?

---

## 10. Uncertain or not verified

- DigiKey, Mouser, Sweetwater, B&H and most Amazon prices are search snippets (403 or unrendered); confirm in a browser before buying.
- TI list prices and ti.com "Out of stock" (JS-rendered page, possibly an artefact).
- MA12070P discontinuation (community and E2E snippets, not an Infineon PCN).
- Silvertel part-to-spec mapping (column-scrambled PDF).
- Waveshare PoE module class and watts; Waveshare, LilyGO, Olimex, DreamSourceLab ship-from (ASSUMED).
- Whether Sonocotta's Lectronz store ships to the US (Tindie says suspended; Lectronz not checked).
- ESP32-S3 fractional-divider accuracy by IDF version (bot-walled source).
- Pi 5 RP1 GPIO toggle latency (low-quality source).
- PoE HAT plus DAC HAT stacking on Pi 5.
- Shairport-sync release dates (sources disagree).
- InnoMaker overlay name; clock role of the Raspberry Pi DAC boards (ASSUMED Pi master).
- All "about $" totals include ASSUMED line items (PSU, SD, cables, PCB assembly).
