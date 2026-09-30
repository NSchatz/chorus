# 0056: board profiles over endpoint.conf, the W5500 as the default link, and one image per link profile in the gate

- Status: accepted (goal 8, 2026-09-30)
- Decided by: the goal (P1 as approved at Checkpoint K: Option B; brief section 12 item 1; audit
  A-14 and the link part of A-10)
- Implemented in: `firmware/boards/`, `firmware/config/endpoint.conf`,
  `firmware/include/chorus/link.h`, `firmware/src/link.c`, `firmware/src/endpoint_config.c`,
  `firmware/main/esp_link.[ch]`, `firmware/main/idf_component.yml`, `firmware/dependencies.lock`,
  `firmware/main/CMakeLists.txt`, `firmware/main/app_main.c`, `tools/firmware-image.sh`,
  `tools/gate.sh`, `tools/lib.sh`, `tools/conventions/check-pins.sh`; held by
  `firmware/tests/test_link.c` (`make firmware-check`, target `link`) and the config check

## Context

P1 settled the platform: an ESP32-S3 in every speaker, on ESP-IDF v6.1; the wired classes on a
WIZnet W5500 over SPI "with INT"; PoE+ by a bought 802.3at splitter; the S3's own radio for the
compact speakers that may run on Wi-Fi (K91). Until this goal the endpoint had one link, Wi-Fi,
as its default (`link_transport = wireless`, audit A-14), no Ethernet code at all, and one
configuration file whose pin map named no real board. The owner's own ESP32-S3 boards are still
unidentified: the goal-1 Needs item "Your ESP32-S3 boards: module markings and a read-only chip
report" is unanswered (shopkit `NEEDS-NOAH.md`, read 2026-09-30), so brief section 0.8 applies:
the board is a parameter and its model is marked ASSUMED, naming that item.

## What was read

All read 2026-09-30:

- `docs/proposals/P1-embedded-platform.md` and `CHECKPOINT-K.approved` (P1: Option B).
- Sonocotta's esparagus-media-center (Apache-2.0; `gh api repos/sonocotta/esparagus-media-center`
  reports `apache-2.0`): `README.md` (the pin tables: "TAS5805M/TAS5825M DAC (Louder Esparagus,
  Audio Brick)" gives the Brick S3's I2C 9/8 and PWDN 17; the W5500 table covers the Louder boards
  only), and the maker's own configuration for the Brick S3,
  `firmware/esphome/5-audio-brick-s3/audio-brick-s3-idf.yaml` at commit `d5e4c58` (a YAML file of
  the maker's, not ESPHome's runtime source): I2S BCLK GPIO14, LRCLK GPIO15, DOUT GPIO16,
  `tas58xx_enable_pin: GPIO17`, SDA GPIO8, SCL GPIO9, SPI CLK 12, MOSI 11, MISO 13,
  `eth_cs_pin: GPIO10`, `eth_interrupt_pin: GPIO6`, `eth_reset_pin: GPIO5`, `flash_size: 8MB`,
  `psram: mode: octal`.
  https://raw.githubusercontent.com/sonocotta/esparagus-media-center/HEAD/README.md and
  https://raw.githubusercontent.com/sonocotta/esparagus-media-center/HEAD/firmware/esphome/5-audio-brick-s3/audio-brick-s3-idf.yaml
- The W5500 component: https://components.espressif.com/api/components/espressif/w5500 (versions
  2.0.0 of 2026-06-25, 1.0.1, 1.0.0; none yanked) and its manifest,
  https://raw.githubusercontent.com/espressif/esp-eth-drivers/master/w5500/idf_component.yml
  (`version: 2.0.0`, `idf: '>=6.0'`, `espressif/wiznet_common: ^1.0.0`; esp-eth-drivers
  `master` at `ed7342c` for `w5500/`), its headers `esp_eth_mac_w5500.h` and
  `esp_eth_phy_w5500.h` (`ETH_W5500_DEFAULT_CONFIG`, `int_gpio_num`, `poll_period_ms`), and
  `wiznet_common/idf_component.yml` (1.0.0). Apache-2.0.
- WIZnet's W5500 page, https://docs.wiznet.io/Product/Chip/Ethernet/W5500: "SPI (Serial
  Peripheral Interface) up to 80MHz".
- The pinned ESP-IDF v6.1 tree (Apache-2.0): `components/esp_eth/Kconfig`
  (`ETH_USE_SPI_ETHERNET`, default y), `components/esp_eth/include/esp_eth_driver.h`
  (`ETH_DEFAULT_CONFIG`, `ETH_CMD_S_MAC_ADDR`), `esp_eth_phy.h` (`reset_gpio_num`, "-1 means
  no hardware reset"), `components/esp_netif/include/esp_netif_defaults.h`
  (`ESP_NETIF_DEFAULT_ETH`, `IP_EVENT_ETH_GOT_IP`),
  `components/esp_hal_gpspi/esp32s3/include/soc/spi_pins.h` (`SPI2_IOMUX_PIN_NUM_CS 10`,
  `_MOSI 11`, `_CLK 12`, `_MISO 13`), `docs/en/api-reference/peripherals/spi_master.rst` ("GPIO
  Matrix and IO_MUX": "Allows signals with clock frequencies only up to 40 MHz, as opposed to 80
  MHz if IO_MUX pins are used") and `docs/en/api-reference/network/esp_eth.rst`.

No GPL source was opened.

## Decision

1. **Board profiles over one base.** `firmware/boards/<profile>.conf` sets only board keys
   (`board_*`, `pin_*`, `eth_*`, `link_transport`) and only keys `endpoint.conf` already carries;
   the one reader (`chorus_endpoint_config_parse_profile`) lays it over the base and refuses any
   other key by name, so a profile can never carry a clock rule, a credential or a toolchain pin.
   `endpoint.conf` carries the default profile's values (it stays a complete file for every tool
   that reads it), names the default in `board_profile`, and the config check fails if the two
   disagree (`--agrees`). The image embeds the chosen profile beside `endpoint.conf` under one
   fixed name; `app_main` logs the board, its status and the link at boot.
2. **Two profiles for the one target.** `brick-s3-wired` (the default): P1's bought reference
   board, the Esparagus Audio Brick (ESP32-S3) with a TAS5825M and a W5500, on the wired link.
   `compact-s3-wifi`: the same board on the S3's radio, the compact speakers' Wi-Fi tier (K91),
   whose behaviour is `chorus#WIFI-7`'s bring-up unchanged. Both mark the model **ASSUMED** and
   name the Needs item verbatim; the config check refuses an ASSUMED model that names no item.
   The pins are the maker's (read above); two values are not read from any source and say so in
   the file: `pin_i2s_mclk = none` (neither the maker's table nor its configuration names an MCLK
   pin, so the TAS5825M is assumed to run from BCLK; goal 9's registers and the bench confirm it)
   and `eth_spi_clock_mhz = 20` (a conservative start, not measured).
3. **The W5500 is the default link, with its interrupt line.** `link_transport = wired` is the
   default (A-14 closed). The decisions live in a pure unit, `link.c`: a wired endpoint brings up
   the W5500 (init, start, an address within `link_address_timeout_ms`) and never touches the
   radio; a wireless endpoint never touches the W5500; a wired pin map is refused by name before
   anything is touched when the INT line is `none` (the driver would poll, and P1 records a LEAD
   of 1.32 s responses in that mode), when an SPI line is unrouted, when a pin breaks the GPIO
   rules of `i2s.c` (now exported as `chorus_gpio_validate`) or is shared with another W5500 or
   audio signal, or when the SPI clock is past the W5500's 80 MHz or past the GPIO matrix's 40
   MHz off the SPI2 IO_MUX pins. The Brick's W5500 sits exactly on those IO_MUX pins, which is
   why the profile names `spi2`. The ESP-IDF binding, `esp_link.c`, uses esp_eth, esp_netif and
   the W5500 driver, gives the W5500 the S3's own Ethernet address (read with `esp_read_mac`, never
   written), and waits for `IP_EVENT_ETH_GOT_IP`.
4. **The driver is pinned.** `firmware/main/idf_component.yml` names `espressif/w5500: "==2.0.0"`
   (the newest; it needs ESP-IDF 6.0 or later); `firmware/dependencies.lock` records it and its
   dependency `espressif/wiznet_common` 1.0.0 by component hash. The component manager fetches
   them into `firmware/managed_components` (ignored). `check-pins.sh` fails an unpinned component
   or one without a hash in the lock, and the pins table in `docs/conventions.md` lists it. The
   downloaded driver is not under `firmware/` source scanning (it exists only after a build); the
   image guard, which reads the linked image, covers it for eFuse writers.
5. **The gate builds one image per link profile.** `firmware-esp32s3-wired` and
   `firmware-esp32s3-wifi`, each through ccache in its own persistent directory
   (`firmware/build/gate-esp32s3-<profile>`), each through `tools/firmware-image.sh`
   (`CHORUS_BOARD_PROFILE`), which prints the target, the board, its status and Needs item, and
   the link, and runs the image guard. The gate copies those lines into its summary.
6. **The server address stays configured** (A-10's link part): discovery and provisioning with
   adoption are goal 14's (brief section 18); `endpoint.conf` says so where it sets the address.
7. **Out of this record:** the bench images for the owner's own boards. When the Needs item is
   answered, a profile for that board is added (or the model here is confirmed) and the ASSUMED
   status goes.

## Not chosen

- **A second copy of every key per profile** (complete profile files): the values would be
  written twice with nothing to hold them together; the overlay plus the `--agrees` check keeps
  one copy of each.
- **Selecting the link at run time from NVS**: the link is a fact about the board and the build,
  and a run-time switch would ship the W5500 code to a board that has none; provisioning is goal
  14's.
- **Polling mode without INT**: refused by the rule above.
- **Vendoring the W5500 driver under `third_party/`**: P1 names the Component Registry with a
  lock file, which pins by hash as tightly and keeps upstream's updates a one-line, reviewed
  change.
- **RMII Ethernet**: the ESP32-S3 has no Ethernet MAC (BRIEF section 5.4, R8).
