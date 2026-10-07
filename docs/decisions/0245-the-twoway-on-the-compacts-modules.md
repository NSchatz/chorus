# 0245: the two-way's board profile is the compact's three modules with a pairing button key, and twoway-speaker.md version 2 names them and the external 24 V adapter

- Status: decided, 2026-10-07 (harness task 320; the owner's decision "Compact's modules" in
  the devices repository's task 274, and that repository's plan for chorus-twoway-v1)
- Recorded by: the owner's agent harness, in the pull request that adds this record
- Implemented in: `firmware/boards/devkitc-s3-louderhat-twoway.conf`,
  `firmware/config/endpoint.conf` (`pin_button_pairing`), `firmware/src/endpoint_config.c`,
  `tools/gate.sh`, `docs/hardware/twoway-speaker.md` version 2, `docs/hardware/controls.md`

## Context

`docs/hardware/twoway-speaker.md` version 1 named the Esparagus Audio Brick (ESP32-S3) as the
two-way's endpoint and left the supply to the electronics plan. The Brick was out of stock at
Tindie, Lectronz and Crowd Supply on 2026-10-06. The owner chose the compact's three modules
(decision 0230) for the two-way too, and the devices repository's plan chose an external Mean
Well GST120A24-P1M adapter over an enclosed supply, wired the boards on the compact's pins and
put the class's hidden pairing button on GPIO1. No profile had a key for a pairing button.

## Decision

1. **A fifth board profile, `devkitc-s3-louderhat-twoway`**: the boards, flash, PSRAM, Ethernet,
   I2S, I2C, power-down and status light pins of `devkitc-s3-louderhat-wired`, with none of the
   compact's five buttons and no microphone, as K68 gives the class. ASSUMED, against the same
   Needs item.
2. **`pin_button_pairing`**, a new board key, `none` in `endpoint.conf` and on every other
   profile, a button to ground against the internal pull-up. The check holds it as every
   control: the GPIO rules and one signal per pin.
3. **GPIO1 is taken as devices chose it**, ASSUMED; it breaks no rule. The channel order stays
   `two_way_woofer_slot = 0`: the HAT's channel A drives the woofer, ASSUMED to be the left slot.
4. **The image does not drive the button yet**, as decision 0230 said of the compact's controls.
5. **The nightly gate builds the image** (`firmware-esp32s3-twoway`), as `profiles_all_built`
   requires of every committed profile.
6. **`twoway-speaker.md` version 2** names the three boards and the adapter, and prices the
   light as the RGB pixel the profile drives (`board_status_led = ws2812`) rather than a plain
   LED: 205.14 USD at the good tier, 1.22 more than version 1, still under the proposed 225.
   The acoustics, the EQ and `chorus-twoway-v1.json` do not change.

## Not chosen

- **Reusing `pin_button_play_pause` for pairing.** The input means something else in the
  controller (`CHORUS_INPUT_PAIRING` is its own input); a profile would say play/pause where
  the speaker has a pairing button, and the subwoofer and streaming amp need the key too.
- **Updating `docs/hardware/lcr-set.md` here.** It quotes version 1's 203.92 by date as its L1
  line; moving its set totals is a change of its own.

## What was read

- goals task 320, read 2026-10-07.
- The devices repository's `projects/chorus-twoway/v1/log.md` at 997bd24, read 2026-10-07.
- In this repository: `firmware/boards/devkitc-s3-louderhat-wired.conf`,
  `firmware/config/endpoint.conf`, `firmware/src/endpoint_config.c`, `firmware/src/controls.c`,
  `docs/hardware/twoway-speaker.md` version 1, `docs/hardware/compact-speaker.md`,
  `docs/hardware/controls.md`, decision 0230, `tools/gate.sh`.
