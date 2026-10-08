# 0250: the knobs' ADC binding is compiled only into an image whose board profile wires a knob

- Status: decided, 2026-10-08 (harness task 457, the nightly of 2026-10-07 on main 6853ff5)
- Recorded by: the owner's agent harness, in the pull request that adds this record
- Implemented in: `firmware/main/CMakeLists.txt`, `firmware/main/esp_knobs.h`,
  `firmware/main/esp_knobs.c`, `firmware/endpoint-units.conf`

## Context

Decision 0248 added `firmware/main/esp_knobs.c`, the subwoofer's knobs on ADC1, to every image.
Its one-shot calls link ESP-IDF's `components/esp_adc/adc_common.c`, and with it that unit's
constructor `adc_hw_calibration` (line 59 in the pinned v6.1). The constructor runs before
`app_main` on every boot, whether or not a knob is wired, and calibrates the SAR ADC by waiting
for each conversion with no timeout (`components/esp_hal_ana_conv/adc_hal_common.c:126`,
`while (!adc_oneshot_ll_get_event(event));`). The emulator models no SAR ADC, so the
`qemu-s3-openeth` image stopped there: its console's last line is the eFuse calibration-version
warning that the same constructor prints, and the nightly's `qemu-boot` and `ota-qemu` failed
every check that needs the link (run 37639247167).

## Decision

`esp_knobs.c` is compiled, and `CHORUS_KNOBS` defined, only when the board profile sets
`pin_knob_level` or `pin_knob_phase` to a pin, read from the profile itself the way
`esp_provision.c` is chosen by `link_transport = wireless` (decision 0103). Without it,
`chorus_esp_knobs_start` is an inline that returns 0, so `app_main` is unchanged and no ADC code
is linked. Today only `devkitc-s3-pcm5102-sub` wires knobs.

## Not chosen

- **Skipping the knob code on the emulated board at run time.** The constructor runs before
  any of chorus's code, so nothing at run time can skip it.
- **Patching or replacing ESP-IDF's constructor.** It is the vendor's calibration, which a real
  board with knobs wants; chorus does not carry a modified ESP-IDF.

## What was read

- The nightly run 37639247167's failed log and its `gate-logs` artifact (`qemu-boot.log`,
  `ota-qemu.log`), read 2026-10-08.
- ESP-IDF v6.1 `components/esp_adc/adc_common.c`, `components/esp_hal_ana_conv/adc_hal_common.c`,
  `components/esp_hw_support/adc_share_hw_ctrl.c`, `components/efuse/esp32s3/esp_efuse_rtc_calib.c`.
- In this repository: decisions 0103, 0248; `firmware/main/CMakeLists.txt`, `esp_knobs.c`,
  `esp_provision.h`, `app_main.c`.
