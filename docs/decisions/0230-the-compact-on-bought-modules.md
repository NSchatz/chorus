# 0230: the compact speaker's board profile is three bought modules (DevKitC-1-N8R8, Louder Raspberry Hat Plus, WIZ850io), and its buttons, light and microphone are board keys

- Status: decided, 2026-10-06 (harness task 201; the owner's decision "Separate US modules",
  goals task 34, and the devices repository's build plan, task 50)
- Recorded by: the owner's agent harness, in the pull request that adds this record
- Implemented in: `firmware/boards/devkitc-s3-louderhat-wired.conf`,
  `firmware/config/endpoint.conf` ("the controls, the status light and the microphone"),
  `firmware/src/endpoint_config.c`, `docs/hardware/compact-speaker.md` version 2,
  `docs/hardware/controls.md`

## Context

The compact speaker's endpoint was the Esparagus Audio Brick (ESP32-S3), which `brick-s3-wired`
names. Its seller lists it as "No longer available". The owner chose separate modules bought in
the US where they can be, and the devices repository's plan for chorus-compact-v1 picked them:
the ESP32-S3-DevKitC-1-N8R8, the Sonocotta Louder Raspberry Hat Plus (a TAS5825M; from Poland,
as no US seller had a TAS58xx board at a usable price) and the WIZnet WIZ850io (a W5500). The
plan wired them on the Brick profile's pins and chose free pins for the compact's controls,
light and microphone, which no profile carried keys for.

## Decision

1. **A fourth board profile, `devkitc-s3-louderhat-wired`**, wired, ASSUMED against the same
   Needs item as the reference board. Its Ethernet, I2S, I2C and power-down pins and its SPI
   clock are the reference board's, so the W5500 and amplifier code run unchanged; its flash
   is 8 MB and its PSRAM octal, as the N8R8 is.
2. **The compact's controls are board keys** in `endpoint.conf`, `none` there and on every
   other profile: `pin_button_play_pause`, `pin_button_volume_up`, `pin_button_volume_down`,
   `pin_button_next`, `pin_button_previous`; `board_status_led` (`none` or `ws2812`) with
   `pin_status_led`; `pin_mic_bclk`, `pin_mic_ws`, `pin_mic_din` (I2S1, input) and
   `pin_mic_mute` (low = muted). They take the `pin_` and `board_` prefixes so a profile may
   set them, as any board key.
3. **The check holds them**: each to the GPIO rules every pin is held to, no control on a pin
   any other signal of the profile uses, a light with its data pin and the reverse, a
   microphone's three lines together, a microphone with a mute switch and a mute switch with a
   microphone (`docs/hardware/controls.md`'s hardware rule).
4. **The pins devices chose are taken as they are**, ASSUMED: buttons GPIO1, 2, 4, 7, 18; the
   light GPIO47; the microphone BCLK GPIO40, WS GPIO39, DIN GPIO41; the mute sense GPIO21. None
   breaks a rule here. The channel order stays `two_way_woofer_slot = 0`: the HAT's channel A
   drives the woofer, ASSUMED to be the left slot.
5. **The image does not drive them yet.** No GPIO, LED or I2S-input binding of the controller
   exists; writing them is later work, and this profile is what it reads.
6. **The nightly gate builds the image** (`firmware-esp32s3-modules`), as `profiles_all_built`
   requires of every committed profile.
7. **`compact-speaker.md` version 2** names the modules and prices them, which puts the budget
   6.09 USD over K89's 150 (156.09), and takes the splitter's 80% into the PoE arithmetic:
   about 15 to 17 W continuous to the drivers, not 17 to 19.

## Not chosen

- **Leaving the controls in comments only.** A pin nobody checks is the mistake a table of
  numbers makes easiest; the keys put them under the same check as every other pin.
- **Folding the controls into `chorus_pin_map_t`.** Every test that builds a pin map with
  `memset` would read them as GPIO0, a strapping pin; a separate struct keeps the audio pin map
  and its tests as they are.
- **Opening the HAT's schematic.** The maker's project is GPL-3.0, so its design files are
  never opened (`docs/clean-room.md`); its README gives the Pi header pins and the address.

## What was read

- goals task 201, read 2026-10-06.
- The devices repository's `projects/chorus-compact/v1/log.md` at 6bdbb10 (then under devices' old
  `builds/` directory), read 2026-10-06.
- The Louder Raspberry Hat Plus's README, sonocotta/raspberry-media-center at 3d8a3d7 ("Boards
  Pinout", "Peripheral (Louder)", "DAC Configuration - Louder Raspberry Pi Media Center and
  Hat"), and the repository's licence (GPL-3.0) from GitHub's API, read 2026-10-06.
- In this repository: `firmware/boards/brick-s3-wired.conf`, `firmware/config/endpoint.conf`,
  `firmware/src/endpoint_config.c`, `firmware/src/i2s.c`, `firmware/src/link.c`,
  `docs/hardware/compact-speaker.md` version 1, `docs/hardware/controls.md`, `tools/gate.sh`.
