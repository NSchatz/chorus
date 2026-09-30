# chorus: embedded platform, network placement, host scheduling, OTA testability, bench packet

Date: 2026-09-29. Read-only research for the /goal program plan (decisions K37, K25, K51, K35, K34,
K38, plus FLEET-10 testability). Nothing in any repo was changed. About 35 minutes.

Citation rules (same as research-endpoint-hardware.md):
- `[URL, read 2026-09-29]`: the page, header or PDF was opened and read today.
- `(snippet)`: from a search-engine result summary only. Lower confidence.
- `ASSUMED`: memory or inference, not verified today. `UNCERTAIN`: sources conflict or partial.
- `(arithmetic, mine)`: derived here from cited numbers.

Process note: this session's WebSearch budget ran out after 3 searches (the shared planning
session had used 197 of 200). Everything else below was read with direct fetches of known
Espressif, GitHub, Debian, OPNsense and vendor URLs. Hard rules kept: no GPL or CERN-OHL-S
source or design file was opened (QEMU and rt-tests were read only through docs, READMEs,
release notes and package metadata; ESP-IDF headers and sources read are Apache-2.0).

---

## 0. Findings that change or sharpen the plan (read first)

1. **The endpoint firmware has no playout path yet.** `firmware/main/esp_hal.c` creates the I2S
   TX channel and reconfigures its clock, but nothing calls `i2s_channel_write` and no DMA event
   callback is registered (grep of `firmware/main/*.c`, read 2026-09-29). So "frames the DMA
   consumed" (BRIEF 5.3) does not exist on either chip today; it is new work whichever MCU wins,
   and the mechanism is the same on both (section 1.3).
2. **ESP32-P4 production silicon (v3.x) needs ESP-IDF v5.5.3+ or v6.0+, and v5.3 cannot build
   for it.** "When using ESP32-P4 chip revision v3.x, you must upgrade ESP-IDF to v5.5.3 or
   later, or v6.0 or later", and "v1.x and v3.x chips cannot share the same firmware image"
   [https://documentation.espressif.com/esp32-p4-chip-revision-v3.x_user_guide_en.pdf (User
   Guide v1.1, 2026.08), read 2026-09-29]. ESP-IDF's compatibility table lists only P4 "v1.0,
   v1.3 ... Supported since ESP-IDF v5.3"
   [https://github.com/espressif/esp-idf/blob/master/COMPATIBILITY.md, read 2026-09-29].
   v3.x parts carry an X suffix (ESP32-P4NRW16X / P4NRW32X, same PDF). **Any P4 purchase must
   be an X-suffix / "P4X" board.**
3. **ESP-IDF v5.3 is already out of service and reaches end of life in January 2027**, inside
   this program's likely span: v5.3 released 25 Jul 2024, service to 25 Jul 2025
   [https://dl.espressif.com/dl/esp-idf/support-periods.svg, read 2026-09-29]; "Release v5.3.5,
   v5.3.6 and v5.3.7 before ESP-IDF v5.3 goes End of Life in January 2027"
   [https://github.com/espressif/esp-idf/blob/master/ROADMAP.md, read 2026-09-29]. The current
   stable docs are v6.1 [https://docs.espressif.com/projects/esp-idf/en/stable/esp32/versions.html,
   read 2026-09-29].
4. **Espressif's QEMU does not emulate the P4, and does not emulate I2S or Wi-Fi on any
   target.** Targets: ESP32, ESP32-S3, ESP32-C3; emulated on all three: NOR flash SPI/MMU, flash
   encryption, eFuse, timers, crypto, TWAI, **OpenCores Ethernet**; not emulated: Wi-Fi, BT, USB,
   I2C, I2S, GPIO matrix [https://github.com/espressif/esp-toolchain-docs/blob/main/qemu/README.md,
   read 2026-09-29]. FLEET-10's A/B + rollback flow can run on an emulated S3 with the real
   bootloader; the P4 gets host fakes only (section 3).
5. **This container cannot run the prebuilt QEMU as is**: of QEMU's documented runtime libraries
   (`libgcrypt20 libglib2.0-0 libpixman-1-0 libsdl2-2.0-0 libslirp0`
   [https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-guides/tools/qemu.html,
   read 2026-09-29]) only libgcrypt is present (`ldconfig -p`, run 2026-09-29). libnuma (needed
   by Debian's rt-tests) is also absent. Either a base-image rebuild (NEEDS-NOAH, per the global
   CLAUDE.md "a system library needs a base-image rebuild") or a gray-zone rootless extraction of
   pinned Debian .debs into /cache with LD_LIBRARY_PATH (dpkg-deb and ar are present).
6. **The S3 CAN trim its audio rate without an APLL, but only by rewriting a fractional divider**:
   S3 MCLK = source / (N + b/a) with N up to 256 and a, b up to 512, source 160 MHz
   [https://raw.githubusercontent.com/espressif/esp-idf/release/v5.3/components/hal/esp32s3/include/hal/i2s_ll.h,
   read 2026-09-29]. 48 kHz x 384 = 18.432 MHz = 160 MHz / (8 + 49/72) exactly; the nearest
   representable neighbours are 8 + 311/457 and 8 + 326/479, about 3.5 ppm away (arithmetic,
   mine). `i2s_channel_tune_rate()` (ESP-IDF v5.5+) retunes exactly this divider on chips
   without APLL and the APLL where present
   [https://raw.githubusercontent.com/espressif/esp-idf/master/components/esp_driver_i2s/i2s_common.c,
   read 2026-09-29]. The S3 setter "temporarily sets division to 2 before applying target
   coefficients" (same i2s_ll.h), so a live retune may glitch the clock: UNCERTAIN, logic
   analyzer first. BRIEF 5.3's sample insert/delete stays the S3 default.
7. **P4 hardware timestamps are reachable from ESP-IDF v5.5, and have a clock API in v6.x, but
   are "Experimental" in every version.** v5.3 and v5.4 document only the ioctl
   `ETH_MAC_ESP_CMD_PTP_ENABLE`
   [https://docs.espressif.com/projects/esp-idf/en/v5.3/esp32p4/api-reference/network/esp_eth.html and
   https://docs.espressif.com/projects/esp-idf/en/v5.4/esp32p4/api-reference/network/esp_eth.html,
   read 2026-09-29]. v5.5 adds get/set PTP time, target-time callbacks, RX timestamps via
   `stack_input_info`, TX timestamps via `esp_eth_transmit_ctrl_vargs()` and L2 TAP
   [https://docs.espressif.com/projects/esp-idf/en/v5.5/esp32p4/api-reference/network/esp_eth.html,
   read 2026-09-29]. v6.1 has `esp_eth_mac_ptp_enable`, `esp_eth_mac_get/set_ptp_time`, an
   "Ethernet Time" Kconfig menu (`ETH_CLOCK_ADJTIME_*`), and "The PPS signal output on GPIO pin
   is available starting from ESP32-P4 silicon revision 3"
   [https://docs.espressif.com/projects/esp-idf/en/stable/esp32p4/api-reference/network/esp_eth.html and
   https://raw.githubusercontent.com/espressif/esp-idf/master/components/esp_eth/Kconfig, read 2026-09-29].
8. **Routed speaker traffic would hairpin one trunk.** In the staged homelab design every VLAN
   rides OPNsense `igc1` ("Mgmt untagged, VLANs 10, 20, 30, 40, 50 tagged"), so a speaker on
   any VLAN other than Servers reaches chorus-server through OPNsense and back over the same
   1 GbE cable (homelab-ro `docs/network.md` lines 37 and 53, read 2026-09-29).

---

## 1. K37 + K25: embedded platform and link

### 1.1 The facts per chip

| Property | ESP32-S3 | ESP32-P4 (v3.x silicon) |
|---|---|---|
| I2S clock sources | PLL_F160M (default), PLL_D2 240 MHz, XTAL, EXTERNAL; no APLL [research-endpoint-hardware.md finding 1, citing https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-reference/peripherals/clk_tree.html] | APLL ("Audio PLL clock"), default, external MCLK-in [https://docs.espressif.com/projects/esp-idf/en/stable/esp32p4/api-reference/peripherals/i2s.html, read 2026-09-29]; v3.x "Added 160 MHz clock source to I2S" [P4 v3.x user guide PDF, read 2026-09-29] |
| 48 kHz accuracy | exact nominal at 384 x fs via 8 + 49/72 (arithmetic, mine); rate error = the crystal's ppm (ASSUMED); fractional-N means MCLK edges wander by up to one 160 MHz period, 6.25 ns (ASSUMED from the N + b/a structure); TAS58xx and PCM5102A re-derive their clocks from BCLK by PLL, so MCLK jitter is filtered and MCLK need not be wired [research-endpoint-hardware.md finding 6] | "The audio PLL can hit an exact 48 kHz (via an 18.432 MHz MCLK)" and its "sigma-delta modulator can be nudged in sub-ppm steps at runtime" [https://developer.espressif.com/blog/2026/06/aes67-audio-over-ip-on-the-esp32-p4/, read 2026-09-29] |
| Rate trim | divider steps of about 3.5 ppm near nominal (arithmetic, mine), via `i2s_channel_tune_rate` (v5.5+); live-retune glitch UNCERTAIN; external MCLK oscillator is fixed-rate (cannot trim unless a VCXO, ASSUMED) | APLL sub-ppm steering (blog above); `i2s_channel_tune_rate` takes the APLL branch when `SOC_I2S_SUPPORTS_APLL` [i2s_common.c, read 2026-09-29]; DatanoiseTV's Apache-2.0 AES67 stack steers "APLL SDM (Sigma-Delta Modulator) register" and "Requires ESP-IDF v5.5+" [https://github.com/DatanoiseTV/aes67-esp32p4, read 2026-09-29] |
| TDM capture (theater) | "only up to 4 slots ... 32 bit-width, and 8 slots for 16 bit-width" [research-theater.md section 2] | "up to 16 slots", "Any data bit-width is supported no matter how many slots are enabled" [P4 I2S page above]; three I2S controllers (same page) |
| Wired Ethernet | no MAC; SPI W5500 (10/100 PHY, SPI "up to 80MHz", 32 KB buffers, 8 sockets; no timestamping mentioned [https://docs.wiznet.io/Product/Chip/Ethernet/W5500, read 2026-09-29]); lwIP throughput about 10-16 Mbit/s reported (research-endpoint-hardware.md finding 3, snippets) vs 2.3 Mbit/s needed | internal EMAC, RMII (IP101 on common boards), IEEE 1588 timestamps (finding 0.7), PPS on rev 3 |
| W5500 driver | v5.3: in core, `eth_w5500_config_t { int_gpio_num /* set -1 ... to poll rx status periodically */; poll_period_ms; ... }`, default `.int_gpio_num = 4, .poll_period_ms = 0` [https://raw.githubusercontent.com/espressif/esp-idf/release/v5.3/components/esp_eth/include/esp_eth_mac_spi.h, read 2026-09-29]; v6.x: "Actual chip drivers are available as components in Component Registry" [esp_eth Kconfig, master, read 2026-09-29], W5500 in esp-eth-drivers [https://github.com/espressif/esp-eth-drivers, read 2026-09-29] | n/a (EMAC + PHY driver in core) |
| Wi-Fi | native radio; `esp_wifi_set_ps(WIFI_PS_NONE)` direct (the firmware already calls it, `esp_hal.c:327`) | none on chip; ESP32-C6 companion via ESP-Hosted (Apache-2.0; SDIO/SPI/UART transports; P4 listed as a host) [https://components.espressif.com/components/espressif/esp_hosted and https://github.com/espressif/esp-hosted-mcu, read 2026-09-29]; e.g. Waveshare P4-WIFI6-POE-ETH uses "ESP32-C6-MINI1-U-H8 ... SDIO" [https://www.waveshare.com/esp32-p4-wifi6-poe-eth.htm, read 2026-09-29] |
| PSRAM | 8 MB octal on the usual N16R8 / R8 modules; GPIO33-37 then reserved (ADR 0015) | 32 MB in package on P4NRW32(X) [Waveshare pages, read 2026-09-29] |
| Pins | strapping 0/3/45/46; USB-JTAG 19/20 (ADR 0015) | 55 GPIOs; strapping GPIO34-38; USB-JTAG GPIO24/25 [https://docs.espressif.com/projects/esp-idf/en/stable/esp32p4/api-reference/peripherals/gpio.html, read 2026-09-29] |
| Min ESP-IDF | v4.4 (COMPATIBILITY.md) | v5.3 for v1.x; **v5.5.3+ or v6.0+ for v3.x** (finding 0.2) |
| Board cost | Waveshare ESP32-S3-ETH $16.99-25.99 (W5500, PoE module optional) [research-endpoint-hardware.md 1.3] | Waveshare ESP32-P4-WIFI6-POE-ETH $24.99, **ESP32-P4NRW32X (v3)**, IP101, PoE on board, C6 [https://www.waveshare.com/esp32-p4-wifi6-poe-eth.htm, read 2026-09-29]; Waveshare ESP32-P4-ETH $12.99+, part "ESP32-P4NRW32" (no X: revision UNCERTAIN) [https://www.waveshare.com/esp32-p4-eth.htm, read 2026-09-29]; Olimex ESP32-P4-DevKit 16 EUR, "ESP32-P4NRW32 module", no revision stated [https://www.olimex.com/Products/IoT/ESP32-P4/ESP32-P4-DevKit/open-source-hardware, read 2026-09-29]; ESP32-P4X-Function-EV-Board DigiKey $59.92 (snippet, research-endpoint-hardware.md 1.5) |
| Custom-board MCU cost | S3-WROOM-1-N16R8 $5.18 + W5500 $2.72 (research-endpoint-hardware.md 8, snippets) | P4 module/chip not priced (gap); IP101 PHY not priced |

Linux endpoints are out of K37's scope but set the reference: Pi 5 / CM5 with ALSA
`snd_pcm_delay` (research-endpoint-hardware.md 3).

### 1.2 How each link behaves in chorus's time sync

chorus's time sync rides in-band on the stream's TCP connection: message type `0x01 time sync`,
"The server answers on the same connection" (`docs/protocol.md` lines 39 and 60, read
2026-09-29), port 4010 (`firmware/config/endpoint.conf` `server_address`).

- **S3 + W5500**: the endpoint's t0 and t3 are software stamps taken around SPI transactions and
  an INT-pin ISR plus a task wakeup. The receive side (INT, ISR, SPI burst read, lwIP) is longer
  than the transmit side (SPI write, then the W5500 sends), so there is a **constant per-platform
  bias** of half that difference in the offset estimate, plus jitter from SPI bus contention
  (ASSUMED; order tens of microseconds, unmeasured). A constant bias shared by every S3 endpoint
  cancels between two S3 speakers of a stereo pair; it does not cancel between an S3 and a Linux
  endpoint, so a mixed pair needs a measured per-platform constant (the same shape as the
  per-model DAC latency constant in research-endpoint-hardware.md finding 11). Polling mode is
  the known hazard: wire INT (1.32 s responses reported without it, snippet in finding 3).
- **P4 + EMAC**: RX goes MAC DMA to lwIP with no SPI hop, so the software-stamp path is shorter
  and less variable (ASSUMED). Hardware timestamps exist at L2 (v5.5+), but chorus's exchange is
  TCP through lwIP; tying a MAC RX timestamp to a TCP payload needs custom plumbing, whereas a
  UDP or raw-L2 time-sync side channel (via the L2 TAP path the v5.5 docs name) would carry it
  naturally (ASSUMED design note). PTP-grade sync is not required by BRIEF 5.3; its value here
  is (a) a tighter offset estimate if the budget ever needs it and (b) **PPS out as an
  independent logic-analyzer cross-check of the endpoint's clock** (rev 3 only).
- **P4 over Wi-Fi (C6 companion)**: two hops (radio to C6, C6 to P4 over SDIO), a second firmware
  image to build, sign and OTA, and power save must be off on the co-processor. Whether
  `esp_wifi_set_ps` is forwarded through `esp_wifi_remote` was not confirmed (the page fetched
  404'd): UNCERTAIN. For the Wi-Fi tier's 5 ms bound (`config/transport.conf`
  `wireless_bound_us = 5000`) the extra hop is probably tolerable (ASSUMED) but it is strictly
  worse than the S3's native radio.

### 1.3 "Frames the DMA consumed" on each chip (same code on both)

- v5.3 already has what is needed: `i2s_channel_register_event_callback` ("DMA event callbacks
  can only be registered or deregistered before the channel is enabled"), `on_sent` receiving
  `i2s_event_data_t { void *data (deprecated); void *dma_buf; size_t size; }`, and
  `i2s_channel_preload_data` [release/v5.3 `i2s_common.h` and `i2s_types.h`,
  https://raw.githubusercontent.com/espressif/esp-idf/release/v5.3/components/esp_driver_i2s/include/driver/,
  read 2026-09-29]. The callback carries **no timestamp** ("No timestamp is recorded", i2s_common.c
  on master, read 2026-09-29).
- Design (ASSUMED, the embedded analogue of `snd_pcm_delay`): in `on_sent` (ISR) increment a
  descriptor counter and stamp the monotonic clock; `frames_queued = frames_written -
  descriptors_done * dma_frame_num`; the playout instant of the next written frame is
  `t_last_on_sent + (frames_queued - frames_in_flight) / fs` plus a per-platform constant for the
  I2S FIFO and the DAC/amp's own delay, measured once with the GPIO-marker method (BRIEF 10).
  With `i2s_dma_frame_num = 240` the callback fires every 5 ms, and the anchor's error is ISR
  latency (microseconds, ASSUMED), not the 5 ms granularity. Enable the TX `auto_clear` option
  so an underrun plays silence (ASSUMED option name; verify).
- v5.5+ adds `i2s_channel_tune_rate(handle, i2s_tuning_config_t {tune_mode, tune_mclk_val,
  max_delta_mclk, min_delta_mclk}, i2s_tuning_info_t {curr_mclk_hz, delta_mclk_hz, water_mark})`
  [release/v5.5 `i2s_common.h` and master `i2s_types.h`, read 2026-09-29]; `water_mark` is buffer
  fill in percent (i2s_common.c), too coarse to be the servo's error but a useful health metric.
- ADR 0015's "ONE clock reader, `firmware/src/monotonic.c`" rule means the ISR stamp must go
  through that unit (ISR-safe, ASSUMED `esp_timer_get_time` is callable from ISR; verify).

### 1.4 What the existing C firmware must change

Common to every option (this is EMBEDDED-5's unfinished half, not a platform cost):
playout task (jitter buffer to `i2s_channel_write`), the `on_sent` anchor above wired into
`chorus/sync.h`'s servo input, sample insert/delete in the write path, and per-board pin
profiles: the one pin map in `endpoint.conf` was chosen without a board, and bought boards fix
their own Ethernet pins (Waveshare S3-ETH has W5500 INT on GPIO10, research-endpoint-hardware.md
1.3, next to the committed `pin_i2c_scl = 9`); whether the committed map collides with a given
board is UNCERTAIN until checked against that board's schematic. `endpoint.conf` today says
`link_transport = wireless` while `config/transport.conf` says `default_transport = wired`;
whichever wired option wins adds a `wired` glue path.

- **S3 only**: keep `CONFIG_IDF_TARGET="esp32s3"`; add W5500 glue (`esp_eth_mac_new_w5500`,
  SPI bus, INT pin, `REQUIRES esp_eth`); on v6.x add `espressif/w5500` from the Component Registry
  pinned in `idf_component.yml` + lock file; pin checker gains SPI and INT pins.
- **P4 only**: second target (`sdkconfig.defaults.esp32p4` with `CONFIG_IDF_TARGET="esp32p4"`,
  `CONFIG_ESP32P4_SELECTS_REV_LESS_V3=n`), IDF bump to v5.5.3+ or v6.x (K51), per-target pin
  rules (strapping 34-38, USB-JTAG 24/25, no octal-PSRAM range), `clk_src = I2S_CLK_SRC_APLL`,
  EMAC + IP101 glue, Wi-Fi via `esp_wifi_remote` + esp_hosted (and a C6 image) for any P4 on
  Wi-Fi. The host-built C11 cores (protocol, sync, amp, session, telemetry) are target-free and
  compile unchanged (ADR 0015: "compiled here exactly as the host build compiles them"); keep
  `-ffp-contract=off -fno-fast-math` on the RISC-V toolchain too so the exchange-by-exchange
  cross-check stays bit-exact (the RISC-V F/D extensions have fused multiply-add, ASSUMED).
- **Two-tier**: both of the above behind one `esp_hal` seam, two sdkconfig files, a build
  matrix of two images in `make gate` (gate time roughly doubles for the firmware compile,
  ASSUMED), and the scans (`endpoint_scan.c`) run over both.
- **IDF v6.x in any option**: the firmware already uses `driver/i2c_master.h` and
  `driver/i2s_std.h` (no legacy drivers), so the migration is mostly `REQUIRES driver` becoming
  `esp_driver_i2s esp_driver_i2c esp_driver_gpio`: "it is strongly recommended to remove [driver]
  component dependencies, and add new driver component (usually esp_driver_xxx)"
  [https://docs.espressif.com/projects/esp-idf/en/stable/esp32/migration-guides/release-6.x/6.0/peripherals.html,
  read 2026-09-29].

### 1.5 Options, costs, risks

| Option | Fitness for chorus | Costs | Risks |
|---|---|---|---|
| A. S3 everywhere (W5500 wired, native Wi-Fi) | one image, cheapest custom BOM, native Wi-Fi; wired timing goes through SPI + ISR; no hardware timestamps; rate trim only by insert/delete or a coarse divider retune; theater capture limited to 8 ch at 16-bit | lowest; stays on v5.3 possible until Jan 2027 | stereo-pair < 0.5 ms (aspire 0.2) unproven on W5500; theater 24-bit 8 ch impossible on one port; an S3 wired speaker mixed with Linux needs a calibrated bias |
| B. P4 everywhere (EMAC wired, C6 for Wi-Fi) | best wired timing path (MAC DMA, optional HW stamps, PPS cross-check), APLL sub-ppm steering, 16-slot TDM, 32 MB PSRAM | every Wi-Fi speaker carries a second chip and a second OTA image; IDF bump mandatory | Wi-Fi tier strictly worse than S3; PTP API "Experimental"; v1.x/v3.x silicon confusion when buying |
| **C. Two-tier: P4 wired reference, S3 Wi-Fi tier** | each chip used where it is strongest: P4 for every timing-critical zone (stereo pair, sub, surround, theater capture), S3 for Wi-Fi-only rooms | two targets in the gate; one seam; IDF bump | the S3-W5500 wired path stays unmeasured unless the bench includes one (it should: section 6) |

**RECOMMENDATION (K37 + K25): C, two-tier.** ESP32-P4 (v3.x silicon) with its internal EMAC on
PoE copper is the reference embedded endpoint for every zone with a tight bound (stereo pair,
surround, subwoofer, theater capture); ESP32-S3 with its native radio is the Wi-Fi convenience
tier. S3 + W5500 wired stays a measured candidate for a cheap multiroom-only speaker, decided by
EMBEDDED-5 data, not now. Reasons, fitness only (CLAUDE.md rule 8): the P4 removes the SPI hop
from the timing path, can steer its audio clock in sub-ppm steps instead of inserting samples,
has hardware timestamps and a PPS pin that give the bench an independent clock check, and is
the only one of the two that can take 8 channels of 24-bit TDM for the theater; the S3 is the
better radio device because its Wi-Fi is on-die. Wired P4 boards cost the same as wired S3
boards ($12.99-24.99 vs $16.99-25.99). Confidence: medium. BRIEF 5.4 says "ESP32-S3 ... Deploy
... for distributed music speakers" and "whether ESP32-P4 earns a slot" is open: this is a
proposed change to record in docs/decisions/ and show at Checkpoint K, not a silent brief edit.

---

## 2. ESP-IDF version (K51)

| Release | Released | Service ends | EOL | What chorus gets |
|---|---|---|---|---|
| v5.3 | 25 Jul 2024 | 25 Jul 2025 | Jan 2027 ("v5.3.5, v5.3.6 and v5.3.7 before ... End of Life in January 2027", ROADMAP.md) | new I2S std/TDM driver, `on_sent`, OTA rollback (long-standing), P4 v1.x only, PTP enable ioctl only |
| v5.4 | 05 Jan 2025 | 05 Jan 2026 | about Jul 2027 (30 months, arithmetic) | nothing chorus-specific found (PTP still ioctl-only in its P4 docs) |
| v5.5 | 21 Jul 2025 | 21 Jul 2026 | about Jan 2028 (arithmetic) | `i2s_channel_tune_rate`; P4 EMAC timestamps (RX/TX, get/set time, target callbacks); **v5.5.3+ builds P4 v3.x**; bugfix releases 5.5.3-5.5.7 planned in 2026 (ROADMAP.md) |
| v6.0 | 20 Mar 2026 | 20 Mar 2027 | about Sep 2028 (arithmetic) | P4 v3.x; PTP clock abstraction and "Ethernet Time" Kconfig; SPI Ethernet drivers moved to the Component Registry; legacy `driver/i2s.h` removed |
| v6.1 | 25 Aug 2026 | 25 Aug 2027 | about Feb 2029 (arithmetic) | current stable docs; all of the above |
| v6.2 | planned 31 Dec 2026 (ROADMAP.md) | | | |

Sources: support-periods.svg (dates; the chart legend calls the service period "Recommended for
new designs"), ROADMAP.md, versions.html ("supported for 30 months after the initial stable
release date", 12 months service + 18 maintenance), all read 2026-09-29. The svg-derived dates
were read through a summarizer that mislabelled columns; release + service-end pairs are
consistent with the 12-month rule, so medium-high confidence.

Options: (a) stay on v5.3 (EOL Jan 2027, cannot build P4 v3.x, no `tune_rate`); (b) v5.5.x
(minimum for P4 v3.x, already in maintenance, EOL about Jan 2028); (c) v6.1.x (service until
Aug 2027, most complete P4 PTP API, SPI Ethernet as a registry component to pin).

**RECOMMENDATION (K51): propose v6.1.x at Checkpoint K**, pinned to an exact tag and commit in
`endpoint.conf` `espidf_version` (the existing refusal in `tools/firmware-image.sh` keeps working),
with the `REQUIRES` migration and a pinned `espressif/w5500` component if S3-wired survives.
Fallback if a v6 regression bites: v5.5.x at 5.5.3 or later. Staying on v5.3 is incompatible
with any P4 v3.x recommendation and with the program outliving January 2027. Confidence:
medium-high. The pinned-v5.3 compile in goal 1 (K51) remains useful as the baseline the upgrade
is diffed against.

---

## 3. OTA testability without hardware (FLEET-10)

### 3.1 Espressif QEMU

- Targets: ESP32, ESP32-S3, ESP32-C3 (no P4, no C6) [esp-toolchain-docs qemu README and
  https://github.com/espressif/qemu/releases, read 2026-09-29]. The ESP-IDF QEMU guide page for
  the P4 returns 404 [https://docs.espressif.com/projects/esp-idf/en/stable/esp32p4/api-guides/tools/qemu.html,
  read 2026-09-29].
- Latest release `esp-develop-9.2.2-20260417` (17 Apr 2026: SDMMC on S3, interrupt fixes); earlier
  releases added octal PSRAM on S3 and TWAI; prebuilt `qemu-xtensa-softmmu-...-x86_64-linux-gnu.tar.xz`
  and `qemu-riscv32-...` assets [https://github.com/espressif/qemu/releases, read 2026-09-29].
- Flash + partitions + bootloader: QEMU "uses the qemu_flash.bin file ... generated based on ...
  flash_args", holding "bootloader, partition table, and application firmware, placed at their
  respective memory offsets" [https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-guides/tools/qemu.html,
  read 2026-09-29], so the real second-stage bootloader, otadata and rollback logic run
  (ASSUMED from that description; prove it in the first hour of FLEET-10).
- eFuse: file-backed `qemu_efuse.bin`, `idf.py qemu efuse-burn` "without permanent hardware
  changes" (S3 QEMU guide). Guardrail 2 is untouched, and anti-rollback could even be exercised
  virtually (a choice for FLEET-10, not a requirement).
- Network: OpenCores Ethernet is emulated on all three targets (README); ESP-IDF's
  `ETH_USE_OPENETH` "can be used when an ESP-IDF application is executed in QEMU. This driver is
  not supported when running on a real chip" [esp_eth Kconfig, master, read 2026-09-29]. So an
  emulated S3 can fetch images from a host HTTP server (host forwarding details not documented
  on the pages read: ASSUMED standard QEMU user networking).
- Not emulated: I2S, I2C, Wi-Fi, GPIO matrix (README). OTA tests must not depend on the audio or
  amp path; the firmware's `esp_hal` seam already isolates them.
- Automation: `pytest-embedded` has a `qemu` service (`pytest --embedded-services qemu`)
  [https://github.com/espressif/pytest-embedded, read 2026-09-29]; a plain shell harness over
  `qemu-system-xtensa` serial output is also enough.
- Install, rootless: `python $IDF_PATH/tools/idf_tools.py install qemu-xtensa qemu-riscv32`
  (S3 QEMU guide) into an `IDF_TOOLS_PATH` under /cache, or the pinned release tarball with a
  recorded sha256. **Blocker in this container: missing glib, pixman, SDL2, slirp** (finding 0.5).
- Licence: QEMU is GPL ("See the GNU General Public License", README). chorus runs it as a
  tool, never links or copies it; its source is never opened (clean-room rule).

### 3.2 Host fakes

- ESP-IDF's Linux target simulates esp_partition, nvs_flash, lwip, esp_netif, esp_http_client
  and FreeRTOS, but app update and bootloader rollback are absent from its support table
  [https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-guides/host-apps.html,
  read 2026-09-29]. So the A/B decision logic must be chorus's own pure C.
- The shape already exists in this repo: `firmware/tests/fake_amp.c` grades the amp bring-up
  order on an event log the driver cannot reach (ADR 0015). The OTA equivalent: a pure-C
  `ota_state` unit (states: running-valid, downloading, written-unverified, pending-verify,
  valid, rolled-back) over a fake two-slot flash + otadata with fault injection at every step
  (power loss mid-write, bad digest, image that fails self-test, image that boot-loops, server
  gone mid-download), graded on host in `make gate`; the ESP-IDF glue maps it onto
  `esp_ota_begin/write/end`, `esp_ota_set_boot_partition`,
  `esp_ota_mark_app_valid_cancel_rollback`, `esp_ota_mark_app_invalid_rollback_and_reboot`, with
  `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE` [https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-reference/system/ota.html,
  read 2026-09-29]. Anti-rollback (`CONFIG_BOOTLOADER_APP_ANTI_ROLLBACK`) burns eFuses on real
  chips (same page): stays off on dev hardware (guardrail 2).
- `endpoint_scan.c` rule 3 ("no OTA activation") must be amended by FLEET-10 as a recorded
  change: OTA calls allowed in one named glue unit only.

**RECOMMENDATION (OTA testability): two layers.** (1) Host: the pure-C OTA state machine plus
fault-injecting fake flash, in `make gate`, for both targets. (2) Emulator: a `make ota-qemu`
target that boots an S3 image in Espressif QEMU with openeth, OTA-updates it from a host HTTP
server to a good image, then to a deliberately bad one, and asserts the rollback from the
serial log; it refuses by name when QEMU's libraries are missing. P4 OTA is layer 1 plus a
NEEDS-NOAH hardware packet. For the libraries: prefer a NEEDS-NOAH base-image rebuild adding
libglib2.0-0t64, libpixman-1-0, libsdl2-2.0-0, libslirp0 (and libnuma1 for section 5);
extracting pinned .debs into /cache is a gray-zone fallback the owner should approve first,
because the global CLAUDE.md says there is "no self-service path" for system libraries.
Confidence: high for layer 1, medium for layer 2 (bootloader-rollback-under-QEMU is ASSUMED
until the spike passes).

---

## 4. K35: speaker network placement

### 4.1 chorus's traffic

| Flow | Direction | Transport | Notes |
|---|---|---|---|
| Audio stream + in-band time sync | endpoint opens to server | TCP 4010 (`endpoint.conf`, `docs/protocol.md`) | time-sync samples queue behind audio on the same TCP stream; min-RTT filtering absorbs it |
| Control / web UI | clients to server | TCP 4020 listener in the server (grep of `crates/server/src`, read 2026-09-29) and Traefik for the PWA | endpoints may not need it at all |
| Discovery | server advertises `_chorus._tcp` | mDNS 5353 | convenience only (below) |
| Future low-latency path | server to endpoint | UDP + FEC unicast | no multicast planned |
| OTA | endpoint pulls from server | HTTP(S) on a server port | or server-push; FLEET-10 decides |
| Telemetry, HA | server-side (MQTT 1883/8883 already allowed from IoT) | | |

IGMP, PIM and multicast routing are not needed: every chorus flow is unicast.

### 4.2 What a router hop does to sync

- The NTP-style offset is "exact only for symmetric paths" (BRIEF 5.3). The error is half the
  one-way asymmetry. A routed hop adds forwarding delay in both directions; the constant part
  cancels, the queueing part appears as RTT spread and is what min-RTT selection discards
  (BRIEF 5.3's own filter). Magnitude on a J6412 + i226 OPNsense: unmeasured; ASSUMED tens of
  microseconds idle, more when the shared trunk is busy.
- Inter-device error is what matters (BRIEF 2.2). Two speakers on the same VLAN take the same
  path, so a common path bias cancels between them. A routed speaker next to an L2 speaker (or a
  Linux endpoint on another VLAN) does not share the bias.
- The trunk (finding 0.8) also carries Plex remote streams and all internet traffic, and the
  server has one 1 GbE uplink (survey-homelab.md section 4). Routed placement couples chorus's
  RTT tail to that load.
- Measurable in software today, no hardware: the Linux client and server can run on two hosts
  or namespaces with and without a routed hop, and the client already logs RTT histograms
  (BRIEF 10). On the real network it is NEEDS-NOAH after the OPNsense cutover.

### 4.3 mDNS

OPNsense's `os-mdns-repeater` "requires at least 2 interfaces, and no more than 5"
[https://docs.opnsense.org/manual/how-tos/multicast-dns.html, read 2026-09-29]; the page
documents no per-service filter, so `_chorus._tcp` is repeated like everything else (ASSUMED
from the absence). The staged design joins Trusted, IoT and Servers (3 of 5). chorus-server on
host networking (K34) advertises on the Servers interface directly; the Docker `mdns-reflector`
only bridges into the `homeassistant` network (survey-homelab.md 2). The homelab rule
"Discovery is a convenience; every integration points at a reserved address" matches chorus:
endpoints keep `server_address` configured, mDNS is the fallback.

### 4.4 Options

| Option | Sync path | Security posture | Homelab PR content (aliases and ports only, no addresses) |
|---|---|---|---|
| a. IoT VLAN + rule | routed via OPNsense, hairpin on `igc1` | speakers share a segment with printers, ecobee, ESPHome; one pinhole to the server | `opnsense_aliases`: `chorus_speakers` (host list from DHCP reservations); `opnsense_port_aliases`: `chorus` [4010, OTA port, future UDP port]; `opnsense_rules`: IoT, source `chorus_speakers`, to `server`, ports `chorus`; DHCP reservations in `host_vars/opnsense.yml`; host nftables role: accept the IoT net on `chorus` ports only (after phase B `firewall_lan4` excludes IoT); docs/network.md policy row |
| **b. Dedicated Audio VLAN (e.g. tag 60)** | routed via OPNsense (same as a) | speakers isolated from IoT gadgets and from each other's neighbours; policy says exactly what speakers may do | `opnsense_networks`: `{tag: 60, name: audio, cidr: <a /24>, pool: [...]}`; add `audio` to `opnsense_house_ifs` (DNS, NTP to the firewall); rules: `audio -> server` on `chorus`, `servers -> audio` any (server-initiated control, OTA push), no `audio -> internet`, no `trusted -> audio` (control goes through the server's UI); mDNS repeater interfaces + audio (4 of 5); switch port VLAN table rows in docs/network.md (owner applies on the standalone ES228GP UI); UniFi SSID tagged 60 for Wi-Fi-tier speakers (owner GUI step); host nftables: accept the audio net on `chorus` ports only; docs/network.md networks + policy rows |
| b'. Audio VLAN with a server leg | L2, no router hop | as b, but the server is multi-homed into the speaker segment | as b, plus a tagged sub-interface (or spare NIC) on the server in VLAN 60 via the Ansible network role; chorus-server binds its speaker listener to that interface only; host nftables scoped to that interface |
| c. Servers VLAN | L2, no router hop | weakest: OTA-updatable embedded devices beside the server, reaching every port `firewall_lan4` opens to Servers | switch ports to VLAN 20; enlarge the Servers DHCP pool (20 addresses today); no OPNsense rule; host firewall already open to Servers |

**RECOMMENDATION (K35): b, a dedicated Audio VLAN, routed at first, with b' as the measured
escape hatch.** Fitness: identical sync path to a, a cleaner security story than a or c, and the
policy table says exactly what a speaker may do. If the routed-vs-L2 measurement shows the hop
costs a material share of the 500 us wired bound (`config/transport.conf` `wired_bound_us`),
move to b' (server leg), which removes the hop without putting speakers in Servers. Draft the
homelab PR (titled "(chorus, never merged by agents)", survey-homelab.md 0.6) but do not open
it until the owner decides (K35). Confidence: medium; the router-hop cost is unmeasured and
could make b' the day-one choice.

---

## 5. K34: host scheduling, what can be measured where

Measured in this container, 2026-09-29: `ulimit -r` 0, `ulimit -l` 8192, CapEff 0,
`/proc/self/timerslack_ns` 50000 (50 us default slack), clocksource tsc, no
`/sys/kernel/realtime`; this session box runs on the same host kernel as production
(survey-homelab.md 4), with a 2-CPU quota.

| Evidence | Where | How | Status |
|---|---|---|---|
| Wakeup jitter of a SCHED_OTHER audio thread with slack set to 1 ns (`prctl(PR_SET_TIMERSLACK)` is unprivileged, ASSUMED) under real host load (holdfast's 24-CPU re-encode) | this container | a chorus-owned Rust probe: `clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME)` loop, histogram p50/p99/p99.9/max, report in docs/measurements/ | software goal can do it now; label it "SCHED_OTHER, cpu.max 2 CPUs, no rtprio" |
| The probe's self-validation | this container | inject a known sleep and check the histogram | software |
| Server-side t1/t2 stamp quality independent of scheduling | this container | compare user-space stamps against kernel software RX timestamps (`SO_TIMESTAMPNS`, unprivileged; TCP support ASSUMED, verify) on the time-sync socket | software; if kernel stamps work, the server's sync accuracy stops depending on RT priority at all |
| SCHED_FIFO wakeup jitter with rtprio + memlock ulimits, host networking, `cpus:` quota | production host, the deployed chorus-server container | same probe inside the container from the K34 homelab PR | NEEDS-NOAH (owner merges and applies homelab) |
| cpu.max throttling of non-RT helper threads (survey-homelab.md 4) | production host | probe threads at both policies side by side | NEEDS-NOAH |
| C-state (C6 133 us exit) effect | production host | probe with and without a `/dev/cpu_dma_latency` hold (root) | NEEDS-NOAH, optional |
| cyclictest cross-check | production host | Debian trixie `rt-tests` 2.6-1.1 (`apt install rt-tests`; depends libnuma1, python3) [https://packages.debian.org/trixie/rt-tests, read 2026-09-29]; GPL tool, run only, source never opened | NEEDS-NOAH |
| Permissive cross-check in the container | this container | `jitterdebugger`: "MIT license", "only dependency to glibc (incl pthread)", "a re-implementation of cyclictest" [https://github.com/igaw/jitterdebugger, read 2026-09-29]; buildable rootless with the system compiler; its source may be read | software |

cyclictest in this container: not installed; Debian's binary needs libnuma1 (absent), and
building rt-tests from source would mean handling GPL source; not worth it when jitterdebugger
(MIT) and chorus's own probe cover the in-container case.

Why it matters less than it looks (analysis, ASSUMED): with `playout_latency_us = 180000`
(`config/sync.conf`), server-side scheduling jitter only matters where the server stamps t1/t2;
audio production has 180 ms of slack. Kernel receive timestamps would decouple even that.

**RECOMMENDATION (K34 evidence): a software goal ships the probe, its self-test, an
in-container SCHED_OTHER baseline under real holdfast load, and the kernel-vs-user stamp
comparison; the SCHED_FIFO-in-production numbers and the cyclictest cross-check are one
NEEDS-NOAH packet run after the owner applies the K34 homelab PR.** No timing claim about the
server is made from the container numbers alone (guardrail 3). Confidence: high.

---

## 6. K38: bench buy packet (tiers, consistent with K37 option C)

Prices are from research-endpoint-hardware.md (all read 2026-09-29) unless marked; "snippet"
prices are unconfirmed. The program never orders (K4, K38).

Owned already (K21, K36): ESP32-S3 boards (model unknown), soldering station, multimeter,
woodworking shop. Useful now for WIFI-7 (Wi-Fi tier, native radio) and for a line-level S3 test
if the boards expose I2S-capable pins. One NEEDS-NOAH line: read the module marking (e.g.
ESP32-S3-WROOM-1-N16R8) so the pin profile and PSRAM mode are known, not guessed.

### Tier 1: rig + two wired Linux endpoints (SOUND-2, RIG-3, SYNC-4), all US ship-from

| Item | Qty | Each | Store | Line |
|---|---|---|---|---|
| Raspberry Pi 5 2GB | 2 | $65.00 | PiShop.us | $130.00 |
| Raspberry Pi DAC+ (PCM5122, Pi-master clock) | 2 | $29.95 | PiShop.us | $59.90 |
| Pi 5 27 W USB-C PSU, microSD, case | 2 | about $25 (ASSUMED) | PiShop.us | about $50 |
| Behringer UMC202HD | 1 | $89 (snippet) | Sweetwater | $89 |
| fx2lafw 24 MHz 8 ch logic analyzer | 1 | $26.95 | SparkFun | $26.95 |
| RCA to 1/4" TS cables, RCA Y-splitter (rig self-calibration), jumpers | 1 set | about $25 (ASSUMED) | any US | about $25 |
| Small gigabit switch for the bench, if the staged ES228GP is not yet in service | 1 | about $20 (ASSUMED) | any US | about $20 |
| **Tier 1 total** | | | | **about $400** |

SOUND-2 "first sound" needs something to listen on: any powered speaker or headphones the owner
has (ASSUMED owned; otherwise add one).

### Tier 2: the embedded endpoint (EMBEDDED-5), P4 reference + S3 wired counter-hypothesis

| Item | Qty | Each | Store, ship-from | Note |
|---|---|---|---|---|
| Waveshare ESP32-P4-WIFI6-POE-ETH (ESP32-P4NRW32X = v3.x, IP101, PoE on board, C6) | 1 (2 for a later P4 pair) | $24.99 | waveshare.com, China (ASSUMED) | **fails US-first**; PoE class not stated; board says 360 MHz (v3 allows 400) |
| US alternative: ESP32-P4X-Function-EV-Board (v3.x "X", RMII Ethernet, ES8311, C6, no PoE) | 1 | $59.92 DigiKey / $59.74 Mouser (snippets) | US | the US-first-compliant P4 path |
| PCM5102A DAC module 2-pack (line out into the rig; PLL from BCK, no MCLK) | 1 | $10.91 (snippet) | Amazon US | same analog path as the Linux pair's measurement |
| Waveshare ESP32-S3-ETH with PoE module (W5500, INT on GPIO10) | 1 | up to $25.99 | Amazon US listing, ship-from unverified | measures the S3-wired alternative K37 leaves open |
| 802.3at PoE injector, if the switch is not live | 1 | about $25 (ASSUMED) | any US | |
| **Tier 2 total** | | | | **about $90 (Waveshare P4) or about $125 (P4X EV board, US)** |

Avoid for now: P4 boards whose listing says "ESP32-P4NRW32" without the X (Waveshare
ESP32-P4-ETH, Olimex ESP32-P4-DevKit): revision UNCERTAIN, and a v1.x chip needs its own image
and has no PPS (finding 0.2, 0.7).

### Tier 3 (optional): amplified sound on the bench

- Escape-hatch amp: the tier-2 PCM5102A into a TPA3116D2 board ($15.98 Parts Express, snippet,
  US) plus a 24 V supply (ASSUMED about $20). Proves the analog chain; no TAS bring-up risk.
- TAS5825M register bring-up (the `unknown` keys in `endpoint.conf`): the only purchasable boards
  are Sonocotta's (S3 or ESP32, Poland, sold out, Tindie US shipping suspended); a P4 + TAS58xx
  board does not exist off the shelf, so the TAS lands on the custom board through devices (K23).
  An owner US-first exception for one Louder-ESP32-Plus ($29 + $5 W5500) would let the register
  map be closed on real silicon before the custom board; the driver is target-independent C.

### Tier 4 (upgrades, not needed to retire the four phases)

DSLogic Plus $149 (buffered 10 ns capture, sigrok) instead of the fx2lafw; Scarlett 2i2 4th gen
$224.99 instead of the UMC202HD; a measurement mic only when acoustic (not sync) work starts.

**RECOMMENDATION (K38): Tier 1 + Tier 2 with the US P4X-Function-EV-Board (about $525 total),
or with the Waveshare P4 board as an explicit US-first exception (about $490) if PoE on the P4
bench matters more than ship-from.** Include the single S3-ETH so K37's S3-wired question is
settled by measurement. Tier 3 by owner choice. Confidence: medium (snippet prices; PoE class of
the Waveshare P4 board unknown).

---

## 7. Decision table

| Decision | Options | Recommendation | Confidence | What would change it |
|---|---|---|---|---|
| K37 embedded platform | S3 only; P4 only; two-tier | **Two-tier: P4 (v3.x) wired reference, S3 Wi-Fi tier** | medium | EMBEDDED-5 shows S3 + W5500 holding the stereo-pair bound (then S3 wired becomes the cheap tier); P4 v3 supply problems; ESP-Hosted forwarding power-save cleanly (would make P4 Wi-Fi viable) |
| K25 endpoint link | W5500 SPI; P4 EMAC RMII; Wi-Fi; PoE | **P4 EMAC on PoE copper for timing-critical zones; S3 native Wi-Fi for convenience zones; W5500 only if measured good** | medium | the same EMBEDDED-5 data; a PoE budget on the ES228GP that cannot feed the fleet |
| K51 ESP-IDF | v5.3; v5.5.x; v6.1.x | **v6.1.x, pinned tag + commit; fallback v5.5.3+** | medium-high | a v6 regression in I2S or esp_eth; a decision to drop the P4 (then v5.5 or v6.1 still, because v5.3 EOL is Jan 2027) |
| FLEET-10 testability | QEMU S3; host fakes; both | **Both: host state machine + fake flash in `make gate`; `make ota-qemu` on S3 with openeth** | high (host) / medium (QEMU) | QEMU libraries not provided (host layer only); bootloader rollback not reproducing under QEMU |
| K35 speaker placement | IoT + rule; Audio VLAN; Audio VLAN + server leg; Servers VLAN | **Dedicated Audio VLAN, routed; server leg if the hop measures costly** | medium | routed-vs-L2 RTT and offset measurement; owner's appetite for another VLAN/SSID |
| K34 scheduling evidence | container only; host only; both | **Probe + in-container baseline + kernel-stamp comparison now; SCHED_FIFO and cyclictest on the host as NEEDS-NOAH** | high | kernel RX timestamps unusable on TCP (then RT priority matters more and the host run is critical) |
| K38 bench packet | Tier 1 only; 1+2; 1+2+3; upgrades | **Tier 1 + Tier 2 (P4X EV board US, or Waveshare P4 as exception) + one S3-ETH; Tier 3 optional** | medium | K37 going S3-only (drop the P4, add a second S3-ETH); owner exception for Sonocotta TAS boards |

---

## 8. Everything read (all 2026-09-29)

Local files: /workspace/BRIEF.md (2.2, 3.2, 5.3-5.5, 5.9, 6, 10); /workspace/docs/decisions/0015-the-esp32-s3-endpoint.md;
/workspace/firmware/config/endpoint.conf; /workspace/firmware/sdkconfig.defaults;
/workspace/config/transport.conf; /workspace/config/sync.conf; /workspace/firmware/main/CMakeLists.txt;
includes and greps of /workspace/firmware/main/esp_hal.c and app_main.c; greps of /workspace/docs/protocol.md
and /workspace/crates/server/src; /cache/tmp/plan-2026-09-chorus/decisions.md (rows K1-K27, K34-K39, K51);
research-endpoint-hardware.md (whole); research-theater.md section 2; survey-homelab.md sections 0, 2, 4;
homelab-ro/docs/network.md (lines 1-60, 92-125, 157-172, grep hits) and homelab-ro/ansible/inventory/host_vars/opnsense.yml (rule and alias structure).

Web (search result summaries, 3 searches): ESP-IDF support periods; ESP32-P4 v3 IDF requirement; ESP-IDF P4 PTP.

Web pages and files fetched:
- https://docs.espressif.com/projects/esp-idf/en/stable/esp32/versions.html
- https://dl.espressif.com/dl/esp-idf/support-periods.svg
- https://github.com/espressif/esp-idf/blob/master/ROADMAP.md
- https://github.com/espressif/esp-idf/blob/master/COMPATIBILITY.md
- https://documentation.espressif.com/esp32-p4-chip-revision-v3.x_user_guide_en.pdf (via redirect from espressif.com)
- https://www.espressif.com/en/news/ESP32_P4_v3.x_Upgrade (body not rendered)
- https://www.cnx-software.com/2026/03/23/esp32-p4-revision-3-0-gains-new-power-rail-requires-new-pcb-design-and-firmware/
- https://github.com/waveshareteam/ESP32-P4-Platform/blob/main/docs/ESP32P4_REVISION_CONFIG.md
- https://docs.espressif.com/projects/esp-idf/en/stable/esp32p4/api-reference/network/esp_eth.html
- https://docs.espressif.com/projects/esp-idf/en/v5.3/esp32p4/api-reference/network/esp_eth.html
- https://docs.espressif.com/projects/esp-idf/en/v5.4/esp32p4/api-reference/network/esp_eth.html
- https://docs.espressif.com/projects/esp-idf/en/v5.5/esp32p4/api-reference/network/esp_eth.html
- https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-reference/network/esp_eth.html
- https://raw.githubusercontent.com/espressif/esp-idf/master/components/esp_eth/Kconfig
- https://raw.githubusercontent.com/espressif/esp-idf/release/v5.3/components/esp_eth/include/esp_eth_mac_spi.h
- https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-reference/peripherals/i2s.html
- https://docs.espressif.com/projects/esp-idf/en/v5.5/esp32s3/api-reference/peripherals/i2s.html
- https://docs.espressif.com/projects/esp-idf/en/stable/esp32p4/api-reference/peripherals/i2s.html
- https://docs.espressif.com/projects/esp-idf/en/stable/esp32p4/api-reference/peripherals/gpio.html
- https://raw.githubusercontent.com/espressif/esp-idf/master/components/esp_driver_i2s/include/driver/i2s_common.h
- https://raw.githubusercontent.com/espressif/esp-idf/release/v5.3/components/esp_driver_i2s/include/driver/i2s_common.h
- https://raw.githubusercontent.com/espressif/esp-idf/release/v5.5/components/esp_driver_i2s/include/driver/i2s_common.h
- https://raw.githubusercontent.com/espressif/esp-idf/master/components/esp_driver_i2s/include/driver/i2s_types.h
- https://raw.githubusercontent.com/espressif/esp-idf/release/v5.3/components/esp_driver_i2s/include/driver/i2s_types.h
- https://raw.githubusercontent.com/espressif/esp-idf/master/components/esp_driver_i2s/i2s_common.c
- https://raw.githubusercontent.com/espressif/esp-idf/release/v5.3/components/hal/esp32s3/include/hal/i2s_ll.h
- https://raw.githubusercontent.com/espressif/esp-idf/master/components/hal/esp32s3/include/hal/i2s_ll.h (404)
- https://docs.espressif.com/projects/esp-idf/en/stable/esp32/migration-guides/release-6.x/6.0/peripherals.html
- https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-reference/system/ota.html
- https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-guides/host-apps.html
- https://github.com/espressif/esp-toolchain-docs/blob/main/qemu/README.md
- https://github.com/espressif/qemu/releases
- https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-guides/tools/qemu.html
- https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-guides/tools/qemu.html
- https://docs.espressif.com/projects/esp-idf/en/stable/esp32p4/api-guides/tools/qemu.html (404)
- https://raw.githubusercontent.com/espressif/esp-idf/master/examples/common_components/protocol_examples_common/Kconfig.projbuild
- https://github.com/espressif/pytest-embedded
- https://github.com/espressif/esp-hosted-mcu and https://raw.githubusercontent.com/espressif/esp-hosted-mcu/main/README.md
- https://github.com/espressif/esp-hosted-mcu/blob/main/docs/sdio.md (404)
- https://components.espressif.com/components/espressif/esp_hosted
- https://docs.espressif.com/projects/esp-idf/en/stable/esp32p4/api-reference/network/esp_wifi_remote.html (404)
- https://github.com/espressif/esp-eth-drivers and https://github.com/espressif/esp-eth-drivers/tree/master/w5500
- https://docs.wiznet.io/Product/Chip/Ethernet/W5500
- https://developer.espressif.com/blog/2026/06/aes67-audio-over-ip-on-the-esp32-p4/
- https://github.com/DatanoiseTV/aes67-esp32p4 (README, Apache-2.0)
- https://github.com/scrambletools/esp_ptp (README)
- https://docs.espressif.com/projects/esp-dev-kits/en/latest/esp32p4/esp32-p4-function-ev-board/user_guide.html
- https://www.waveshare.com/esp32-p4-eth.htm
- https://www.waveshare.com/esp32-p4-wifi6-poe-eth.htm
- https://www.olimex.com/Products/IoT/ESP32-P4/ESP32-P4-DevKit/open-source-hardware
- https://www.omadanetworks.com/us/business-networking/omada-switch-easy-managed/es228gp/ (404)
- https://docs.opnsense.org/manual/how-tos/multicast-dns.html
- https://www.rfc-editor.org/rfc/rfc5905 (the asymmetry text was not located in the rendered excerpt; the claim above rests on BRIEF 5.3)
- https://packages.debian.org/trixie/rt-tests
- https://wiki.linuxfoundation.org/realtime/documentation/howto/tools/rt-tests (403)
- https://git.kernel.org/pub/scm/utils/rt-tests/rt-tests.git/about/ (403)
- https://github.com/igaw/jitterdebugger (README, MIT)

Local commands (read-only): `ulimit -r`, `ulimit -l`, `/proc/self/timerslack_ns`, `/proc/self/status`
capabilities, current clocksource, `ldconfig -p` library checks, `which dpkg-deb ar cyclictest`,
text extraction of the downloaded P4 v3.x PDF with pypdf into the session scratchpad.
