# P1: Embedded platform, wired link and ESP-IDF release

- Decisions: K25, K37, K51
- Status: PROPOSED (chorus goal 1, 2026-09-30); decided at Checkpoint K
- If deferred: ESP32-S3 on ESP-IDF v5.3.6 (no upgrade), Wi-Fi plus a W5500 wired path
- Builds on: goal 6 (§10: the ESP-IDF pin, endpoint codecs and mbedTLS encryption), goal 7 (§11: the bench packet), goal 8 (§12: targets, wired link as default, Wi-Fi tier, playout), goal 9 (§13: TAS58xx registers, controls, EMBEDDED-5 packet), goals 24-26 (§28-§30: the speaker design packages name "the settled endpoint board")

## Question

Which embedded chip or chips carry chorus's speakers (ESP32-S3, ESP32-P4, or both as tiers), which
wired link they use (W5500 SPI Ethernet, RMII on the P4's EMAC, or Wi-Fi only; with PoE), and which
ESP-IDF release the firmware moves to. Every option that needs a custom board is flagged, because
devices' rules forbid one without an approved departure (§0.13).

The owner's decisions that bound it:

- K37: "goal 1 proposes ESP32-S3 vs ESP32-P4 (or both, as tiers) together with K25's link, on
  datasheet-verified clocking ... CLAUDE.md rule 8 (fitness, never availability) applies, so
  owning S3 boards is not a reason." Not chosen as final: "keep S3; P4 as reference".
- K25: "goal 1 picks the link with the board (W5500 SPI vs RMII vs Wi-Fi-only boards ...; PoE)
  ... BRIEF §3.2 'wired first' stands as the owner's principle meanwhile."
- K51: "a move to a newer ESP-IDF (e.g. required if K37 picks the P4) is a written proposal decided
  at Checkpoint K, never a silent upgrade."
- K22: the compact speaker is "Era 100-class, ESP32-S3 + TAS58xx, PoE"; the two-way, sub and
  streaming amp are the other classes. K90: "PoE+ for the compact speaker; mains for two-way, sub,
  rack amp and soundbar (with Ethernet)". K91: "a few compact speakers may run on Wi-Fi ... never
  bonded into stereo pairs or theater sets; every other class is wired". K96: Linux endpoints
  where they fit (rack amp, theater hub); "small speakers stay ESP32-class (K37)".
- K62 and K92: FLAC and Opus decoded on the C endpoint; encrypted sessions "with ESP-IDF's mbedTLS
  on the endpoint"; trust-on-first-use key pinning.
- K89: under about $150 of parts per compact speaker.

## Constraints that bind every option

- **Fitness only (CLAUDE.md rule 8, K37).** That the owner owns ESP32-S3 boards (K21, model
  unknown) and that ESP-IDF v5.3.6 is installed here are not reasons for or against anything below.
- **Buy the module (§0.13, devices `CLAUDE.md`).** "A custom board exists only after the owner
  approves a proposal that says it departs from 'buy what can be bought'." Every speaker class
  needs an MCU, a TAS58xx amp (K22), Ethernet (K90) and, for the compact class, PoE+ power. An
  option that can only be built with a custom board is flagged **CUSTOM BOARD** and cannot reach a
  design package until such a departure proposal is approved.
- **US-first by ship-from** for every BOM line (§0.13); a non-US line is an exception the owner
  grants.
- **Guardrails (BRIEF §3.1):** no eFuse burns on development hardware (the safety scans run on
  every target and every ESP-IDF version, §0.7); monotonic clocks in the audio path; timing claims
  only from measurement. Nothing below is a timing claim: it is datasheet capability plus
  `ASSUMED` reasoning, to be measured at EMBEDDED-5.
- **Sync targets (BRIEF §2.2):** stereo pair and surround < 0.5 ms (aspire 0.2 ms); multiroom
  < 5 ms. The repo's bounds: `wired_bound_us = 500`, `wireless_bound_us = 5000`
  (`config/transport.conf:40,44`).
- **Hardware design sources (K39):** permissive or vendor reference designs only; CERN-OHL-S and
  GPL design files are never opened.

## Re-verification of the planning research

The planning research (`research-endpoint-hardware.md`, `research-platform-network.md`,
`verify-theater-platform.md`, all 2026-09-29) recommended **two tiers: ESP32-P4 (v3.x silicon) on
its EMAC as the wired reference, ESP32-S3 as the Wi-Fi tier, on ESP-IDF v6.1.x**. What was
re-checked today, and what changed:

| Claim | Re-check (2026-09-30) | Result |
|---|---|---|
| S3 has no APLL and no EMAC | ESP-IDF v6.1 `soc/esp32s3/include/soc/soc_caps.h`: no `SOC_I2S_SUPPORTS_APLL`, no `SOC_CLK_APLL_SUPPORTED`, no `SOC_EMAC_SUPPORTED` (grep empty); `SOC_WIFI_SUPPORTED 1`, `SOC_AES_SUPPORTED 1`, `SOC_SHA_SUPPORTED 1`, `SOC_I2S_SUPPORTS_TDM (1)` | Confirmed |
| P4 has APLL, EMAC, IEEE 1588 | v6.1 `soc/esp32p4/.../soc_caps.h`: `SOC_EMAC_SUPPORTED 1`, `SOC_EMAC_IEEE1588V2_SUPPORTED (1)`, `SOC_EMAC_REF_CLK_FROM_MPLL (1)`, `SOC_I2S_SUPPORTS_APLL (1)`, `SOC_CLK_APLL_SUPPORTED (1)`, `SOC_ECC_SUPPORTED 1`; no `SOC_WIFI_SUPPORTED` | Confirmed |
| P4 timestamp API is "Experimental"; PPS from rev 3 | v6.1 P4 `esp_eth` page: "Time stamp associated API is currently in 'Experimental Feature' state so be aware it may change with future releases"; "The PPS signal output on GPIO pin is available starting from ESP32-P4 silicon revision 3" | Confirmed |
| P4 v3.x needs v5.5.3+ or v6.0+ | `esp_hw_support/port/esp32p4/Kconfig.hw_support`: at v5.3.6 the maximum is "Rev v1.99" and no v3 option exists; at v5.5.3 and v6.1, "Rev v3.0" and "Rev v3.1" exist, `ESP32P4_SELECTS_REV_LESS_V3` `default n`, min rev default `ESP32P4_REV_MIN_301` | Confirmed |
| v5.3 EOL January 2027; v6.1 latest | ROADMAP.md: "Release v5.3.5, v5.3.6 and v5.3.7 before ESP-IDF v5.3 goes End of Life in January 2027", v5.3.7 planned 2027/01/18; SUPPORT_POLICY.md: "supported for 30 months", Service 12 + Maintenance 18; GitHub releases: v6.1 published 2026-08-27 (latest), v6.0.3 2026-09-02, v5.5.5 2026-07-17, v5.3.6 2026-09-15. v6.1.1 was planned for 2026/09/03 (ROADMAP) but is not published as of today | Confirmed; **new:** no v6.1.x patch release yet |
| S3 TDM limit; P4 TDM | v6.1 S3 I2S page: "only up to 4 slots are supported while the slot is set to 32 bit-width, and 8 slots for 16 bit-width, 16 slots for 8 bit-width"; v6.1 P4 I2S page: "up to 16 slots", "Any data bit-width is supported no matter how many slots are enabled" | Confirmed |
| `i2s_channel_tune_rate()` exists for both chips | Present in the v6.1 S3 and P4 I2S API references ("Dynamically fine-tuning the audio rate at runtime") | Confirmed; it is the S3's rate trim without an APLL |
| W5500 | WIZnet: SPI "up to 80MHz", "8 independent SOCKETs and 32KB of internal memory"; the page names no frame timestamping | Confirmed |
| W5500 driver in v6.x | `espressif/esp-eth-drivers` `w5500/idf_component.yml`: version 2.0.0, `idf: '>=6.0'` (a Component Registry dependency to pin) | Confirmed |
| **New: mbedTLS in v6** | v6.1 migration guide (5.5 to 6.0, Security): "ESP-IDF v6.0 updates to Mbed TLS v4.0, where PSA Crypto is the primary cryptography interface"; "In Mbed TLS v4.0, most legacy cryptography APIs have been removed" | **New finding**: K62's endpoint encryption (goal 6) written on v5.x's legacy `mbedtls_*` API would be rewritten at the next upgrade. The firmware uses no mbedTLS today (`rg mbedtls firmware/` is empty) |
| **New: codec headroom** | Espressif's `esp_audio_codec` README, measured on ESP32-S3R8: Opus decode 48 kHz stereo 5.86% CPU, FLAC decode 44.1 kHz stereo 8.0% CPU (heap 26.6 KB and 89.4 KB); the FLAC figure is measured on real audio, the Opus figure on an encoded sine tone (README note 1, Opus encoded at 90 kbps, complexity 0), so Opus headroom for music is `ASSUMED` until goal 6 measures it | Headroom exists on the S3. This library is under the "Espressif Modified MIT License" ("use EXCLUSIVELY with Espressif Systems products") and is P9's call, not this one; the numbers are cited only as vendor evidence of headroom |
| Off-the-shelf boards | Waveshare ESP32-P4-WIFI6-POE-ETH: "ESP32-P4NRW32X", IP101, "Integrated PoE Module" (class not stated), C6 over SDIO, "360MHz", $24.99-79.99. Waveshare ESP32-P4-ETH lists "ESP32-P4NRW32" (no X). Olimex ESP32-P4-DevKit: "ESP32-P4NRW32 module", 16.00 EUR, no revision stated. Waveshare ESP32-S3-ETH: W5500, "Optional for PoE module ... (IEEE 802.3af-compliant)", $16.99-25.99 | Confirmed; the Olimex and Waveshare P4-ETH revisions stay UNCERTAIN |
| **Changed: a buyable S3 + TAS58xx + Ethernet board** | Crowd Supply: "Esparagus Audio Brick (ESP32-S3)", TAS5825M, W5500 SPI Ethernet, 5-26 V, $59, "Sold and shipped by Crowd Supply", "$8 US Shipping", "Orders placed now ship Jan 25, 2027" (pre-order); "Produced by Sonocotta in Wroclaw, Poland", fulfilment by Mouser. Tindie Louder-ESP32 (S3 or ESP32, TAS5805M, W5500 add-on) "Sold out since Sep 24, 2026"; Louder-ESP32-Plus (TAS5825M) "Sold out since Sep 08, 2026"; the same maker's Louder ESP32 Pro (S3, TAS5825M, on-board Ethernet, $45) is out of stock on Lectronz, the Brick's S3 variants show 0 in stock on Lectronz and Tindie, and Elecrow lists the S3 Brick at $59 as in stock (ship-from not stated, a NON-US EXCEPTION if China) | **Changes the recommendation.** The planning research's two-tier plan put the TAS58xx on a custom board through devices (its "Option B"). §0.13 forbids that without a departure. An S3 + TAS5825M + W5500 board can be bought (one maker, pre-order); no P4 + TAS58xx board can |
| PoE numbers | Ethernet Alliance 802.3bt overview: PSE "Class 4 30 W", PD "Class 4 25.5 W", Type 3 51 W, Type 4 71.3 W. ES228GP: 250 W total, 30 W per port, 802.3af/at only (`verify-theater-platform.md` #9, read 2026-09-29; brief §2) | Confirmed (EA paper re-read; ES228GP page not re-fetched) |
| Simulator numbers per platform (K37) | `docs/measurements/` holds no per-platform model; nothing in the sync crates or reports models a W5500 vs EMAC stamp path (`rg -i 'w5500\|emac\|p4'` over them is empty) | **No simulator number distinguishes the chips today.** Goal 7's 8-room simulation can add a per-platform stamp-path jitter model; its output is labelled simulation and is never timing evidence |

Adversarially verified 2026-09-30 (goal-1 verifier 1): 17 claims confirmed, 0 refuted, 1 partly right, 0 unverifiable; corrections applied; the recommendation stands.

## Options

The link question (K25) is answered inside each platform option. Considered and set aside without a
full row: **Wi-Fi-only boards** (fail K90's PoE+ compact speaker and K91's "every other class is
wired", and BRIEF §3.2 wired first); **classic ESP32 with RMII** (outside K37's S3-vs-P4 frame;
its internally generated RMII clock uses the APLL, "If Wi-Fi and Ethernet are used simultaneously,
the RMII clock cannot be generated by the internal APLL", per Espressif's ESP32-Ethernet-Kit guide
as cited in `research-endpoint-hardware.md`, not re-read today).

### Option A: ESP32-S3 on ESP-IDF v5.3.6, Wi-Fi plus W5500 (the "If deferred" fallback)

- What: the current target and toolchain, unchanged. W5500 glue added for wired classes (goal 8);
  native Wi-Fi for K91's compact Wi-Fi speakers.
- Costs: no toolchain work now. Goal 6 writes endpoint encryption on v5.3's legacy mbedTLS 3.x API.
  An upgrade is still forced within months: v5.3 reaches EOL in January 2027 (last planned patch
  v5.3.7, 2027/01/18), so the upgrade, and a rewrite of the crypto glue to PSA Crypto if the target
  is v6, lands in a later goal anyway.
- Risks: no `i2s_channel_tune_rate()` on v5.3 (added in v5.5, `research-platform-network.md` §1.3;
  present in v6.1 docs), so rate trim is sample insert/delete only (BRIEF §5.3, which is already
  chorus's mechanism, so not a blocker); unmaintained toolchain for the fleet's life; W5500 timing
  path unmeasured (below).
- Custom board: **none needed** (same hardware path as B).
- Fit: meets K22's wording and K91; fails K51's intent of not outliving the toolchain.

### Option B: ESP32-S3 everywhere on ESP-IDF v6.1.x; W5500 wired, PoE+ by a bought splitter, native Wi-Fi

- What: one target (`esp32s3`). Wired classes use W5500 over SPI with the INT pin wired
  (`int_gpio_num` set; the planning research cites a report of 1.32 s responses in polling mode,
  `LEAD`). The compact speaker takes 802.3at through a bought PoE+ splitter with gigabit
  pass-through and a 24 V output (for example PoE Texas GAT-24V25W, "24 Volt 25 Watt", $31.99 at
  its own shop; P4 prices the line), feeding the amp board's 5-26 V input. K91's compact Wi-Fi
  speakers use the S3's own radio with power save off (`esp_wifi_set_ps(WIFI_PS_NONE)`, already in
  `esp_hal.c`). The bought reference board is an S3 + TAS5825M + W5500 board (Esparagus Audio Brick
  ESP32-S3, above); chorus firmware replaces its stock firmware.
- Clocking for sync: no APLL. The S3's I2S clock comes from the 160 MHz PLL through a fractional
  divider; 48 kHz x 384 = 18.432 MHz = 160 MHz / (8 + 49/72) exactly, and neighbouring divider
  settings are about 3.5 ppm apart (`research-platform-network.md` §0.6, arithmetic, `ASSUMED`
  until the logic analyzer reads LRCLK). Correction: sample insert/delete (BRIEF §5.3) as today,
  with `i2s_channel_tune_rate()` (v5.5+, present in v6.1) as an optional coarse trim once a live
  retune is shown glitch-free on the bench (the S3 setter "temporarily sets division to 2" per the
  planning research, UNCERTAIN). TAS5825M and TAS5805M "No MCLK Required" (TI datasheets, per
  `verify-theater-platform.md` side findings), so MCLK jitter is not on the amp path.
- Time stamps: software stamps around SPI transactions and an INT-pin ISR. A constant receive/send
  asymmetry cancels between two identical S3 speakers of a pair; jitter from SPI contention is
  unmeasured (`ASSUMED` tens of microseconds). A mixed pair (S3 with a Linux endpoint) needs a
  measured per-platform constant. BRIEF §6: wired software timestamps support "~0.1-0.2 ms
  typical" (Snapcast-class); whether W5500's extra hop keeps the S3 inside 0.5 ms is **the
  question EMBEDDED-5 answers**.
- Multichannel: two I2S controllers; TDM limited to 4 slots at 32-bit or 8 at 16-bit. Enough for
  every speaker class (stereo, or two amp channels for a two-way); not enough for 8-channel 24-bit
  capture on one port. K96 puts the theater hub and rack amp on Linux, and P2 settles the capture
  hardware.
- Codecs and crypto: Espressif's own measurement shows Opus and FLAC decode at 6-8% of an S3
  (above; the FLAC figure is measured on real audio, the Opus figure on an encoded sine tone, so
  Opus headroom for music is `ASSUMED` until goal 6 measures it); chorus's decoders per P9 may cost more (`ASSUMED` still within budget; goal 6 measures on
  the host, the bench on the chip). AES and SHA accelerators exist (`SOC_AES_SUPPORTED`,
  `SOC_SHA_SUPPORTED`); on v6 they are reached through PSA Crypto drivers. 2.3 Mbit/s of 48k/24
  stereo (BRIEF §6) is small against either (`ASSUMED`).
- PSRAM: 8 MB octal on the N8R8/N16R8 modules; GPIO33-37 reserved (ADR 0015); DMA descriptors stay
  in internal RAM (`sdkconfig.defaults`). 512 KB SRAM, dual LX7 at 240 MHz (S3 datasheet v2.2).
- Costs: money: boards $16.99-25.99 (Waveshare S3-ETH) to $59 (S3 + TAS5825M + W5500, pre-order);
  splitter $31.99; per compact speaker electronics about $91 of the $150 budget (K89), leaving about
  $59 for driver, enclosure, controls and mic (arithmetic from the cited prices; goal 24 prices the
  rest). Effort: goal 6 moves to v6.1 (the `REQUIRES driver` split into `esp_driver_*`, the
  `espressif/w5500` component pinned in `idf_component.yml` with a lock file, PSA Crypto for K62,
  `IDF_PY_BUILD_JOBS` re-verified); goal 8 adds W5500 glue and one pin profile. Gate: one firmware
  compile, as today. Maintenance: v6.1 service to about 2027-08 and support to about 2029-02
  (30-month rule, arithmetic).
- Risks: (1) the W5500 path misses the stereo-pair bound on the bench (mitigation: the named
  escalation to C below, a written departure proposal); (2) single-maker supply for the bought
  S3 + TAS board: the S3 Brick is a pre-order shipping 2027-01-25, the Louder boards are sold out,
  and the maker is in Poland (Crowd Supply ships every order from Mouser's distribution center in
  Mansfield, TX, USA, per its ordering guide read 2026-09-30, so the line is US ship-from; the
  2027-01-25 date is 'the project creator's best estimate' and may slip);
  (3) a newer toolchain: fallback v5.5.x (5.5.3 or later) if a v6.1 regression bites I2S or
  `esp_eth`; (4) the Brick's W5500 INT wiring is unconfirmed: no source read shows the Brick
  routing the W5500 INT pin to an S3 GPIO (the maker's README, Apache-2.0, gives the Brick S3's
  TAS5825M pins but a W5500 pin table only for the Louder boards), so "W5500 wired with INT" is
  `ASSUMED` for the Brick until goal 8 checks the Brick's published schematic (Apache-2.0) or the
  bench does. Without INT the W5500 runs in polling mode, the mode in which the planning research's
  `LEAD` reports 1.32 s responses; P4's B3 (Waveshare S3-ETH, INT on GPIO10) carries the INT-mode
  measurement either way.
- Custom board: **none needed** for any ESP32-class speaker, as long as one bought S3 + TAS58xx
  board stays purchasable. If supply fails, the choices are an owner-approved departure (a carrier
  board) or BRIEF §5.5's escape hatch (a PCM5102A DAC plus an analog class-D board, both buyable),
  which departs from K22's "TAS58xx" and would itself need the owner's word.
- Fit: matches K22's "ESP32-S3 + TAS58xx, PoE", K90 (PoE+ via splitter), K91 (native Wi-Fi), K62
  (headroom; PSA crypto on the maintained API), and devices' buy-the-module rule. Gives up the P4's
  shorter timing path, APLL and 16-slot TDM.

### Option C: two tiers: ESP32-P4 (v3.x) on its EMAC for wired classes, ESP32-S3 for the Wi-Fi tier, on ESP-IDF v6.1.x (the planning research's recommendation)

- What: P4 wired endpoints (RMII to an IP101 PHY, RMII clock from MPLL so the APLL stays free for
  I2S); S3 compact speakers on Wi-Fi. Two targets behind one `esp_hal` seam.
- Clocking for sync: the APLL "can hit an exact 48 kHz (via an 18.432 MHz MCLK)" and its
  sigma-delta modulator "can be nudged in sub-ppm steps" (Espressif AES67 blog, per
  `research-platform-network.md` §1.1), reached through `i2s_channel_tune_rate()`; EMAC RX goes by
  DMA with no SPI hop (`ASSUMED` shorter, less variable stamp path); IEEE 1588 hardware stamps
  (API "Experimental") and PPS on a GPIO (rev 3 only) give the bench an independent clock check.
  Using MAC stamps for chorus's in-band TCP time sync needs custom plumbing or a UDP side channel
  (design note, `ASSUMED`).
- Multichannel: up to 16 TDM slots at any width; three I2S controllers.
- Costs: money: P4 boards $12.99-24.99 (Waveshare) to $59.74-59.92 (Espressif P4X Function EV
  board, Mouser/DigiKey, search snippets). Effort: goal 8 gains a second target, sdkconfig, pin
  rules (strapping GPIO34-38, USB-JTAG 24/25), EMAC and PHY glue, and a second OTA image line
  for FLEET-10; the gate compiles two images (`ASSUMED` roughly double the firmware step); QEMU
  does not emulate the P4, so its OTA gets host fakes only.
- Risks: v1.x vs v3.x silicon confusion when buying (only X-suffix parts, for example
  ESP32-P4NRW32X, run v3.1 firmware); an Experimental timestamp API; Waveshare's P4 board runs at
  360 MHz per its page against the P4 datasheet's 400 MHz.
- Custom board: **CUSTOM BOARD for every wired P4 speaker.** No bought board combines a P4 with any
  TAS58xx, and no standalone TAS58xx I2S module with US stock was found by the planning research
  (TI's TAS5825MEVM needs the PUREPATH-CMBEVM motherboard and is out of stock on ti.com). A
  departure proposal is required before goals 24-26 can name this board.
- Fit: the strongest timing platform on paper, and the right chip if P2 ever needs 8-channel
  24-bit capture on an ESP32-class device. Fails devices' buy-the-module rule without a departure.

### Option D: ESP32-P4 everywhere, ESP32-C6 companion for Wi-Fi, on ESP-IDF v6.1.x

- What: one chip family; K91's Wi-Fi speakers reach the radio through ESP-Hosted over SDIO to a C6
  (as on the Waveshare P4-WIFI6-POE-ETH).
- Costs: as C, plus a second firmware image (the C6's) to build, sign and OTA on every Wi-Fi
  speaker.
- Risks: as C, plus a Wi-Fi tier strictly worse than the S3's on-die radio (two hops; whether
  power-save control forwards through `esp_wifi_remote` is UNCERTAIN in the planning research).
- Custom board: **CUSTOM BOARD for every amplified class** (the same TAS58xx gap as C).
- Fit: poorest fit for K91 and for the buy-the-module rule.

## Comparison

| Criterion | A: S3, v5.3.6 | B: S3, v6.1.x | C: P4 wired + S3 Wi-Fi, v6.1.x | D: P4 + C6, v6.1.x |
|---|---|---|---|---|
| Custom board needed | no | no (while one S3 + TAS58xx board is buyable) | **yes**, every wired P4 speaker | **yes**, every amplified class |
| Wired link | W5500 SPI | W5500 SPI, INT wired | EMAC RMII (IP101) | EMAC RMII |
| PoE+ (compact) | bought splitter | bought splitter, 24 V, 25 W | bought splitter or board PoE (class unstated) | same as C |
| Wi-Fi (K91) | native | native | native (S3 tier) | via C6, second image |
| Audio clock | PLL + fractional divider; insert/delete | same, plus `tune_rate` trim | APLL, sub-ppm steering | APLL |
| Time-stamp path | SPI + ISR (unmeasured) | SPI + ISR (unmeasured) | MAC DMA; HW stamps (Experimental); PPS | same as C |
| TDM | 8 x 16-bit or 4 x 32-bit | same | 16 slots, any width | same |
| Opus / FLAC decode | FLAC 8% of an S3 on real audio; Opus 5.9% on a sine test (Espressif lib) | same | more headroom (400 MHz RISC-V, datasheet) | same |
| Endpoint crypto API (K62) | legacy mbedTLS, rewrite later | PSA Crypto (mbedTLS 4), written once | PSA Crypto | PSA Crypto |
| Toolchain life | EOL Jan 2027 | service to ~2027-08, EOL ~2029-02 | same as B | same as B |
| Gate firmware compiles | 1 | 1 | 2 | 1 (plus C6 image) |
| QEMU for FLEET-10 OTA | S3 yes | S3 yes | S3 yes, P4 no | no |
| Matches K22 wording | yes | yes | compact yes, others change | no |

## Recommendation

**Recommendation:** Option B, ESP32-S3 everywhere on ESP-IDF v6.1.x (W5500 wired with INT, PoE+ by a bought 802.3at splitter, native Wi-Fi for K91), because it is the only platform every speaker class can be built on from bought modules, and v6.1 outlives v5.3's January 2027 EOL with the PSA crypto API that K62 is then written against once.

Why: the P4 is the better timing chip on paper, but chorus's targets (0.5 ms stereo pair) sit inside
what BRIEF §6 expects of wired software timestamps, and the P4's advantage cannot be bought as a
speaker: every wired P4 speaker needs a custom TAS58xx carrier board, which devices' rules forbid
without a departure. The S3 path has a bought S3 + TAS5825M + W5500 board, the native radio K91
needs, and K22's own wording. Pin ESP-IDF **v6.1** (tag object `4dc1c65503e98e9a6b4c3c5646237280ba79829b`,
commit `fff9895c82d744c7237be8847347bdd1b07c6643`, published 2026-08-27), moving to the newest
v6.1.x patch available when goal 6 starts (v6.1.1 was planned for 2026/09/03 and is not yet out);
fallback v5.5.5 (commit `b774170ff46c393eeb5e495ea37936038d3f4f4f`) if a v6.1 regression hits I2S or
`esp_eth`. What it costs: one toolchain migration in goal 6 (driver split, pinned `espressif/w5500`
component, PSA Crypto), W5500 glue in goal 8, about $91 of electronics per compact speaker. What the
owner gives up: the P4's APLL, hardware time stamps, PPS cross-check and 16-slot TDM, unless the
bench says they are needed.

**Named escalation:** if EMBEDDED-5 measures two wired S3 speakers outside the 0.5 ms stereo-pair
bound (or a mixed S3 and Linux pair that no per-platform constant fixes), the next step is a written
departure proposal for Option C's P4 wired tier (a custom TAS58xx carrier through devices), not a
silent switch. P4 buys a P4X development board for exactly this comparison (its full tier).

## If the owner defers

Later goals build on Option A: ESP32-S3 on ESP-IDF v5.3.6 (no upgrade), Wi-Fi plus a W5500 wired
path. Goal 6 writes endpoint encryption against v5.3's legacy mbedTLS API; goal 8 builds the S3 with
W5500 as the wired default and Wi-Fi for compact speakers; no P4 target exists. The cost: the
firmware runs on a toolchain that reaches end of life in January 2027 (v5.3.7 planned 2027/01/18 is
the last release), so a later upgrade proposal is certain and will also move the crypto glue to PSA
Crypto if it targets v6; `i2s_channel_tune_rate()` is unavailable, so rate trim stays insert/delete
only. The hardware path and its bench packet are the same as the recommendation's.

## Open inputs

- The owner's ESP32-S3 boards: module marking and `esptool.py chip_id` (goal-1 Needs item, I12).
  Used only for pin profiles and bench convenience, never as a reason (rule 8).
- Measured W5500 stamp-path jitter and bias on an S3, and the stereo-pair error of two wired S3
  endpoints (EMBEDDED-5, NEEDS-OWNER bench session). Every timing statement above is `ASSUMED`
  until then.
- Whether a live `i2s_channel_tune_rate()` retune glitches the S3's clock (logic analyzer on LRCLK).
- The S3's actual LRCLK accuracy at 48 kHz per ESP-IDF version (`ASSUMED` crystal-limited).
- Chorus's own FLAC and Opus decode cost on the S3 (P9 chooses the decoders; Espressif's 6-8% is
  for its own library, and its Opus figure comes from a sine test).
- Supply of the bought S3 + TAS58xx board (pre-order, estimated ship 2027-01-25; see P4); if it
  fails, the owner chooses a departure or the escape hatch.
- Whether the Brick S3 wires the W5500 INT pin to an S3 GPIO: unconfirmed (`ASSUMED`); goal 8
  checks the Brick's published schematic (Apache-2.0), or the bench does. If it does not, the Brick
  runs the W5500 in polling mode (the planning research's `LEAD` reports 1.32 s responses in that
  mode), and B3 in P4 (INT on GPIO10) carries the INT-mode measurement.
- The compact speaker's PoE+ power budget at the amp: 25.5 W at the PD, about 18-20 W average left
  for the amp after conversion and MCU (`ASSUMED`, `research-endpoint-hardware.md` §5), measured
  in goal 24's packet.
- P2's capture decision: if it needs 8-channel 24-bit capture on an ESP32-class device rather than
  a Linux hub (K96), that is the case for a P4 capture board (and a departure proposal).
- Waveshare's P4 board PoE class and output power, and the revision of Olimex and Waveshare
  P4-ETH boards: not stated on their pages (UNCERTAIN).

## Sources

- ESP-IDF v6.1 `soc_caps.h`, esp32s3: https://raw.githubusercontent.com/espressif/esp-idf/v6.1/components/soc/esp32s3/include/soc/soc_caps.h, read 2026-09-30
- ESP-IDF v6.1 `soc_caps.h`, esp32p4: https://raw.githubusercontent.com/espressif/esp-idf/v6.1/components/soc/esp32p4/include/soc/soc_caps.h, read 2026-09-30
- ESP-IDF P4 `Kconfig.hw_support` at v5.3.6, v5.5.3, v6.1: https://raw.githubusercontent.com/espressif/esp-idf/{v5.3.6,v5.5.3,v6.1}/components/esp_hw_support/port/esp32p4/Kconfig.hw_support, read 2026-09-30
- ESP-IDF ROADMAP.md: https://raw.githubusercontent.com/espressif/esp-idf/master/ROADMAP.md, read 2026-09-30
- ESP-IDF SUPPORT_POLICY.md: https://raw.githubusercontent.com/espressif/esp-idf/master/SUPPORT_POLICY.md, read 2026-09-30
- ESP-IDF releases and tags (GitHub API): https://api.github.com/repos/espressif/esp-idf/releases and /git/ref/tags/v6.1, /git/ref/tags/v5.5.5, read 2026-09-30
- ESP-IDF v6.1 I2S, ESP32-S3: https://docs.espressif.com/projects/esp-idf/en/v6.1/esp32s3/api-reference/peripherals/i2s.html, read 2026-09-30
- ESP-IDF v6.1 I2S, ESP32-P4: https://docs.espressif.com/projects/esp-idf/en/v6.1/esp32p4/api-reference/peripherals/i2s.html, read 2026-09-30
- ESP-IDF v6.1 Ethernet, ESP32-P4: https://docs.espressif.com/projects/esp-idf/en/v6.1/esp32p4/api-reference/network/esp_eth.html, read 2026-09-30
- ESP-IDF migration 5.5 to 6.0, Security: https://docs.espressif.com/projects/esp-idf/en/v6.1/esp32s3/migration-guides/release-6.x/6.0/security.html, read 2026-09-30
- esp-eth-drivers W5500 component manifest: https://raw.githubusercontent.com/espressif/esp-eth-drivers/master/w5500/idf_component.yml, read 2026-09-30
- esp_audio_codec README (performance on ESP32-S3R8, licence): https://raw.githubusercontent.com/espressif/esp-adf-libs/master/esp_audio_codec/README.md and .../LICENSE, read 2026-09-30
- ESP32-S3 Series Datasheet v2.2: https://www.espressif.com/sites/default/files/documentation/esp32-s3_datasheet_en.pdf, read 2026-09-30
- ESP32-P4 Series Datasheet: https://www.espressif.com/sites/default/files/documentation/esp32-p4_datasheet_en.pdf, read 2026-09-30
- WIZnet W5500 documentation: https://docs.wiznet.io/Product/Chip/Ethernet/W5500, read 2026-09-30
- Ethernet Alliance, Overview of 802.3bt v2.1: https://ethernetalliance.org/wp-content/uploads/2019/12/WP_EA_Overview8023bt_V2p1_FINAL.pdf, read 2026-09-30
- Waveshare ESP32-P4-WIFI6-POE-ETH: https://www.waveshare.com/esp32-p4-wifi6-poe-eth.htm, read 2026-09-30
- Waveshare ESP32-P4-ETH: https://www.waveshare.com/esp32-p4-eth.htm, read 2026-09-30
- Waveshare ESP32-S3-ETH: https://www.waveshare.com/esp32-s3-eth.htm, read 2026-09-30
- Olimex ESP32-P4-DevKit: https://www.olimex.com/Products/IoT/ESP32-P4/ESP32-P4-DevKit/open-source-hardware, read 2026-09-30
- Crowd Supply, Esparagus Audio Brick: https://www.crowdsupply.com/sonocotta/esparagus-audio-brick, read 2026-09-30
- Crowd Supply, "Ordering, Paying, Shipping: All the Details" (orders ship from Mouser, Mansfield, TX, USA; pre-order dates are the creator's best estimate): https://www.crowdsupply.com/guide/ordering-paying-shipping-details, read 2026-09-30
- Lectronz, Louder ESP32 Pro (out of stock) and Esparagus Audio Brick (0 left): https://lectronz.com/products/louder-esp32-pro and https://lectronz.com/products/esparagus-audio-brick, read 2026-09-30
- Tindie, Esparagus Audio Brick (0 in stock): https://www.tindie.com/products/sonocotta/esparagus-audio-brick/, read 2026-09-30
- Elecrow, Esparagus Audio Brick ("Single DAC, ESP32-S3", $59, "In stock", "Sold By Sonocotta Poland"): https://www.elecrow.com/esparagus-audio-brick.html, read 2026-09-30
- Sonocotta esparagus-media-center README (Apache-2.0; Brick S3 TAS5825M pins, no Brick W5500 pin table): https://raw.githubusercontent.com/sonocotta/esparagus-media-center/HEAD/README.md, read 2026-09-30
- Tindie, Louder-ESP32 and Louder-ESP32-Plus: https://www.tindie.com/products/sonocotta/louder-esp32/ and https://www.tindie.com/products/sonocotta/louder-esp32-plus/, read 2026-09-30
- TI TAS5825MEVM tool page: https://www.ti.com/tool/TAS5825MEVM, read 2026-09-30
- PoE Texas GAT-24V25W: https://shop.poetexas.com/products/gat-24v25w (Shopify product JSON), read 2026-09-30
- ESP32-P4X Function EV Board, Mouser and DigiKey (search snippet, $59.74 and $59.92; pages not rendered): https://www.mouser.com/ProductDetail/Espressif-Systems/ESP32-P4X-Function-EV-Board?qs=Naspt24KZtm%2F%2FovBNrcBgA%3D%3D and https://www.digikey.com/en/products/detail/espressif-systems/ESP32-P4X-FUNCTION-EV-BOARD/29196984, read 2026-09-30
- Carried from the planning research, not re-fetched today (cited as there): Espressif AES67 on ESP32-P4 blog (https://developer.espressif.com/blog/2026/06/aes67-audio-over-ip-on-the-esp32-p4/), S3 `i2s_ll.h` fractional divider (release/v5.3), ESP32-Ethernet-Kit guide, TAS5825M/TAS5805M datasheets (via `verify-theater-platform.md`), Omada ES228GP spec page, W5500 polling report (`LEAD`)

## What was read

Local: `/cache/tmp/chorus-g1/agent-rules.md`, `/cache/tmp/chorus-g1/proposal-format.md`;
[`.claude/goals/2026-09-chorus.md`](https://github.com/NSchatz/chorus/blob/535ed2816b5735153ac81c52a235024aa806f2b5/.claude/goals/2026-09-chorus.md) (header, §0.6-§0.13, §1 incl. 1.1 and 1.2, §2, §5,
§10-§14, §28-§30); [`.claude/goals/2026-09-chorus-research/research-endpoint-hardware.md`](https://github.com/NSchatz/chorus/blob/535ed2816b5735153ac81c52a235024aa806f2b5/.claude/goals/2026-09-chorus-research/research-endpoint-hardware.md),
`research-platform-network.md`, `verify-theater-platform.md` (whole); baseline
`/cache/wt/chorus/chorus/baseline/BRIEF.md` §2.2, §5.3-§5.5, §6, §8, §10;
`firmware/config/endpoint.conf` (grep), `firmware/sdkconfig.defaults`, `config/transport.conf`
(grep), `docs/decisions/` listing, `docs/measurements/` listing, `rg mbedtls firmware/`.

Web and API (all 2026-09-30): every URL in Sources, plus web searches for P4X EV board pricing, PoE+
splitter and PoE Texas pricing. No GPL source file and no CERN-OHL-S or GPL hardware design file
was opened (Olimex and Sonocotta product pages only; ESP-IDF, esp-eth-drivers and esp-adf-libs
files read are Apache-2.0 or Espressif-licensed manifests and READMEs).
