# 0000: the subwoofer's board profile is a DevKitC, a PCM5102A line DAC and a WIZ850io, a new line-dac audio output with its filter delay counted, the knobs read by ADC1, and subwoofer.md version 2

- Status: decided, 2026-10-07 (harness task 322; the owner's decision "DevKitC+DAC+WIZ" in the
  devices repository's task 276, and that repository's plan for chorus-sub-v1)
- Recorded by: the owner's agent harness, in the pull request that adds this record
- Implemented in: `firmware/boards/devkitc-s3-pcm5102-sub.conf`, `firmware/config/endpoint.conf`
  (`board_audio_output = line-dac`, `board_output_delay_frames`, `pin_knob_level`,
  `pin_knob_phase`), `firmware/include/chorus/line_dac.h`, `firmware/src/line_dac.c`,
  `firmware/src/endpoint_config.c`, `firmware/src/playout.c`, `firmware/main/app_main.c`,
  `firmware/main/esp_hal.c`, `firmware/main/esp_knobs.c`, `tools/gate.sh`,
  `docs/hardware/subwoofer.md` version 2, `docs/hardware/controls.md`

## Context

`docs/hardware/subwoofer.md` version 1 named the Esparagus Audio Brick (ESP32-S3) as the
endpoint, whose TAS5825M the subwoofer does not use, and left the line-level feed to its 100 W
plate amplifier (a Dayton Audio SPA100-D) open between three options. The Brick was out of stock
at every seller on 2026-10-06. The owner chose an ESP32-S3-DevKitC-1-N8R8, an Adafruit 6250
(TI PCM5102A, 2.1 V RMS line out) and a WIZ850io, version 1's option 3; the devices repository's
plan wired them on the compact's pins, put the DAC's mute pad on the power-down line GPIO17, the
pairing button on GPIO1, the knobs on GPIO2 and GPIO4 (ADC1) and the light on GPIO47, chose a
Mean Well GST18U05-P1J 5 V adapter, and asked for the DAC's filter delay to be accounted.

The firmware knew one output, `amplifier`: an I2C bus, a TAS5825M register map, a fault watch.
A PCM5102A has none of them. The knobs' ADC binding was not written (app_main said so).

## Decision

1. **A sixth board profile, `devkitc-s3-pcm5102-sub`**: the DevKitC's flash and PSRAM, the
   W5500's and the I2S pins of `devkitc-s3-louderhat-wired`, both I2C pins `none`, the mute on
   GPIO17, and the class's controls (K69). ASSUMED, against the same Needs item.
2. **`board_audio_output = line-dac`**, a third output beside `amplifier` and `none`, with its
   own bring-up (`chorus/line_dac.h`): the mute low, the clock checked against the PCM5102A's PLL
   ratios (32 or 64 BCK a frame, TI SLAS859C Table 11, p. 25) and started into a muted part, the
   mute released once the playout writer runs, and stopped mute first. No I2C bus is created, no
   register written, no fault read and no fault watch started. Its report is the amplifier's
   report type, so telemetry reads either. The check refuses I2C pins on a line DAC, holds every
   other output to its I2C pins as before, and holds a line DAC's clock to the PLL's ratios.
3. **`board_output_delay_frames`**, a board key counted in the playout path's device delay (as
   the sound chain's limiter look-ahead already is), 0 in `endpoint.conf`; the check holds a
   `line-dac` profile to the PCM5102A's normal filter's 22 tS (SLAS859C Table 4, p. 17), 458 us
   at 48 kHz. The datasheet's summary table says 20 tS (p. 4); the filter's own table is taken.
   It is a datasheet figure, not a measurement, and no timing claim is made on it. The
   TAS5825M's own delay is not read and stays 0. The GPIO marker, a cross-check of the I2S line,
   does not count it.
4. **`pin_knob_level` and `pin_knob_phase`**, board keys held to ADC1 (GPIO1 to GPIO10) and the
   one-signal-per-pin rule, and their binding (`firmware/main/esp_knobs.c`): 12-bit one-shot
   reads every 50 ms (ASSUMED) into `chorus_controls_knob`, the steps it decides handed to the
   chain through `chorus_playout_set_sub_knobs`. GPIO2 and GPIO4 are taken as devices chose them.
5. **The LFE feed on both slots** needed no change: the chain's one output for the `LFE` role
   is already written to both slots. `firmware/tests/test_endpoint_dsp.c` now shows it on the
   profile's own configuration, with the delay in the device delay and the knob codes reaching
   `sub_level_cdb` and the polarity.
6. **The nightly gate builds the image** (`firmware-esp32s3-sub`), as `profiles_all_built`
   requires of every committed profile.
7. **`subwoofer.md` version 2** names the three boards and the adapter, settles the feed as the
   line DAC on both of the amplifier's inputs, and gives the amplifier's gain for 2.1 V in:
   19.8 V RMS at the driver, 19.5 dB. 346.09 USD at the good tier, 14.60 under version 1. The
   acoustics, the high-pass, the amplifier and `chorus-sub-v1.json` do not change.

## Not chosen

- **Calling the DAC an `amplifier` with an empty register map.** Every `amp_` key would read
  `unknown` and the sequencer would refuse by name; and a fault watch on a part with no fault
  register would read nothing and call it clear.
- **The DAC's delay as a constant in code alone.** A profile states what its board does; the
  check ties the profile's figure to the datasheet's, so neither drifts.
- **The low-latency filter (FIL high, 3.5 tS, p. 17).** Its delay is smaller but it is an IIR
  with a different pass band, and the breakout's default is the normal filter; the 458 us is
  inside the playout latency and is counted, so nothing is gained by changing the wiring.
- **Unmuting at bring-up, as the amplifier reaches Play.** The board's plan asked that the mute
  be held until playout starts; the DAC's 104-sample soft ramp (p. 16) then begins on the
  writer's first frames.

## What was read

- goals task 322, read 2026-10-07.
- The devices repository's `projects/chorus-sub/v1/log.md` at ade93a1, read 2026-10-07.
- TI, PCM510xA datasheet SLAS859C (May 2012, revised May 2015),
  https://www.ti.com/lit/ds/symlink/pcm5102a.pdf, read 2026-10-07, sha256
  a522083606b8e994875883215046fbdb92fd362996ca97719d259fbe9df07a85: pp. 4, 5, 16, 17, 25.
- Adafruit, https://www.adafruit.com/product/6250 and
  https://learn.adafruit.com/adafruit-pcm510x-i2s-dac/pinouts, read 2026-10-07.
- Dayton Audio SPA100-D user manual, "Low-Level Inputs" and "Maximum Input Sensitivity
  Voltage", read 2026-10-07.
- ESP-IDF v6.1 `components/soc/esp32s3/include/soc/adc_channel.h` and
  `components/esp_adc/include/esp_adc/adc_oneshot.h`.
- In this repository: decisions 0063, 0229, 0245; `firmware/src/amp.c`, `endpoint_config.c`,
  `endpoint_dsp.c`, `controls.c`, `playout.c`; `firmware/main/app_main.c`, `esp_hal.c`;
  `docs/hardware/subwoofer.md` version 1, `docs/hardware/controls.md`.
