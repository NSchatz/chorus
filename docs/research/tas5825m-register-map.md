# The TAS5825M register map and bring-up, from TI's datasheet

Research for chorus goal 9 (brief section 13, item 1; audit A-10's second half), 2026-09-30.
What `firmware/config/endpoint.conf`'s `amp_` keys, `firmware/src/amp.c`'s sequence and
`docs/decisions/0061-the-tas5825m-register-map-and-bring-up.md` rest on.

Date of research: 2026-09-30. Every URL below was read on 2026-09-30.
Labels: CONFIRMED (read in the primary source, page given), ASSUMED (reasoned, not stated by a
source), LEAD (not confirmed).

Clean-room note: no GPL source file was opened. Read: TI's datasheet (TI's own document); the
board maker's ESP-IDF configuration files in `sonocotta/esparagus-media-center` (Apache-2.0,
per the GitHub licence API); the README of `sonocotta/esp32-tas5805m-dac` (the repository is
GPL-3.0 per the GitHub licence API, so only its README, a document, was read, and none of its
source); ESP-IDF v6.1's I2S driver at the pinned commit (Apache-2.0).

## What was read

- TI's TAS5825M datasheet, SLASEH7H rev H (https://www.ti.com/lit/ds/symlink/tas5825m.pdf, read
  2026-09-30; TI's own document), every page cited below.
- `sonocotta/esparagus-media-center` at `d5e4c58` (Apache-2.0): `firmware/esphome/5-audio-brick-s3/audio-brick-s3-idf.yaml`
  and `firmware/esphome/packages/dac-tas58xx.yaml`, configuration files, read 2026-09-30.
- `sonocotta/esp32-tas5805m-dac` (GPL-3.0): its README only, read 2026-09-30; no source file.
- ESP-IDF v6.1 at `fff9895c82d744c7237be8847347bdd1b07c6643` (Apache-2.0): `components/esp_driver_i2s/include/driver/i2s_std.h`,
  `components/esp_driver_i2s/i2s_std.c`, `components/esp_driver_i2s/i2s_common.c`,
  `components/esp_hal_i2s/include/hal/i2s_types.h`.

## The source

- **TI, "TAS5825M 4.5 V to 26.4 V, 38-W Stereo, Inductor-Less, Digital Input, Closed-Loop
  Class-D Audio Amplifier with 192-kHz Extended Audio Processing"**, literature number
  SLASEH7H, "OCTOBER 2019 - REVISED JANUARY 2023" (revision H), 106 pages.
  URL: https://www.ti.com/lit/ds/symlink/tas5825m.pdf, fetched 2026-09-30,
  sha256 `ed7865c3189587e68f7a65d9848909a13f25af9d1b3930a8a24af11275e503ea` (3,222,103 bytes).
- Text extracted rootless with `uv run --with pypdf` (pypdf's `extract_text` per page). The
  printed page number in each page's footer equals the PDF page index, so "p. N" below is both.
- Why earlier goals had `unknown`: the goal-1 research pass could not extract text from the PDF.
  pypdf could; no value below is from memory.

## Facts, page by page (CONFIRMED unless marked)

| Fact | Page | Where it is used |
|---|---|---|
| PDN: "Power down, active-low. PDN place the amplifier in Shutdown, turn off all internal regulators." | 4 (pin table) | PDN low is the dead state; the part does not answer I2C there (ASSUMED from "turn off all internal regulators" and the startup procedure's I2C-after-PDN order; the fake models it) |
| ADR: "A table of resistor value (Pull down to GND) decides device I2C address. See Table 9-5." | 4 | `amp_i2c_address` |
| Supported SCLK frequencies "32 64 fS"; fS 32 to 192 kHz; SCLK up to 24.576 MHz | 7 (recommended operating conditions, serial audio port) | `amp_sclk_per_frame = 64`, `i2s_wire_slot_bit_width = 32` |
| DVDD in shutdown (PDN = 0.8 V) 7.4 uA; Deep Sleep 0.82 mA | 7 | shutdown is deeper than Deep Sleep |
| "Supports 3-wire digital audio interface (no MCLK required)" | 1 | `pin_i2s_mclk = none` is not an assumption about the part any more, only about the board |
| Table 9-1: I2S/LJ/RJ, 32/24/20/16 data bits, 32 to 192 kHz, SCLK rate "64, 32" fS | 29 | the 48 fS that two 24-bit slots give is not supported |
| "When Clock halt, non-supported SCLK to LRCLK(FS) ratio is detected, the device reports Clock Error in Register 113 (Register Address 0x71)"; on a clock halt "the device puts all channels into the Hi-Z state"; it returns by itself when the clock recovers | 29 | the clock-fault bit is excused only before the clock starts |
| "Default setting is I2S and 24 bit word length"; format in 0x33 D[5:4], word length 0x33 D[1:0] | 29 | `amp_audio_format_value` |
| Data format figures: "Audio data word = 24-bit, SCLK = 64fs" | 30 | a 24-bit word in a 32-bit slot |
| I2C at 100 and 400 kHz; "the user must change from page to page"; "All registers are listed ... and is in Page 0" | 39 | book 0, page 0 before any access |
| 7-bit address: MSBs "factory preset to 10011"; Table 9-5: ADR 0 ohm to GND `1001100` (0x4C), 1 kohm `1001101` (0x4D), 4.7 kohm `1001110` (0x4E), 15 kohm `1001111` (0x4F) | 39 | `amp_i2c_address = 0x4C` (strap ASSUMED, below) |
| "On Page 0x00 of each book, Register 0x7f is used to change the book. Register 0x00 of each page is used to change the page. To change a Page first write 0x00 to Register 0x00 to switch to Page 0 then write the book number to Register 0x7f on Page 0." | 41 | `amp_reg_page_select = 0x00`, `amp_reg_book_select = 0x7F`, `amp_page_book_zero = 0x00` |
| Startup procedures (9.5.3.1): "3. Once power supplies are stable, bring up PDN to High and wait 5 ms at least, then start SCLK, LRCLK. 4. Once I2S clock are stable, set the device into HiZ state and enable DSP via the I2C control port. 5. Wait 5 ms at least. Then initialize the DSP Coefficient, then set the device to Play state." | 42 | the sequence; `amp_power_up_wait_ms = 5`, `amp_dsp_settle_wait_ms = 5` |
| Shutdown procedures (9.5.3.2): "Configure the Register 0x03h -D[1:0]=10 (Hiz) via the I2C control port or Pull PDN low. 3. Wait at least 6 ms (this time depends on the LRCLK rate, digital volume and digital volume ramp down rate)." The 6 ms is "based on LRCLK (Fs) = 48kHz, Digital volume ramp down update every sample period, decreased by 0.5dB for each update, digital volume =24dB" | 43 | `amp_shutdown_wait_ms = 6`; `chorus_amp_shut_down` |
| OCSD: "the I2 fault register saves a record"; DC detect: outputs go Hi-Z | 43 | the fault registers are the record |
| Control-port register table: 01h RESET_CTRL, 03h DEVICE_CTRL2, 33h SAP_CTRL1, 54h AGAIN, 67h DIE_ID, 68h POWER_STATE, 70h CHAN_FAULT, 71h GLOBAL_FAULT1, 72h GLOBAL_FAULT2, 73h WARNING, 78h FAULT_CLEAR | 44-45 | every register id |
| DEVICE_CTRL2 (03h), reset 0x10: bit 4 DIS_DSP (reset 1; "needs to be made 0 only after all the input clocks are settled"), bit 3 MUTE, bits 1-0 CTRL_STATE "00: Deep Sleep 01: Sleep 10: Hiz, 11: PLAY" | 48 | `amp_reg_state_control = 0x03`, `amp_state_hiz = 0x02`, `amp_state_play = 0x03` |
| SAP_CTRL1 (33h), reset 0x02: DATA_FORMAT bits 5-4 `00: I2S`; WORD_LENGTH bits 1-0 `10: 24 bits` | 53 | `amp_reg_audio_format = 0x33`, `amp_audio_format_value = 0x02` |
| DIG_VOL (4Ch) reset 30h | 57 | left at reset (0 dB digital); DIG_VOL_CTRL1 (4Eh, reset 0x33, p. 59) left at reset, which is what p. 43's 6 ms assumes |
| AGAIN (54h), reset 0x00: ANA_GAIN bits 4-0, "00000: 0 dB (29.5V peak voltage) 00001:-0.5db 11111: -15.5 dB" | 63 | `amp_reg_analog_gain = 0x54`, `amp_analog_gain_code = 0x1F` (the lowest setting, 0 dB above it) |
| DIE_ID (67h), reset 95h, read-only, `10010101` | 72 | `amp_reg_device_id = 0x67`, `amp_device_id_value = 0x95` |
| POWER_STATE (68h): STATE_RPT "0: Deep sleep 1: Seep 2: HIZ 3: Play" | 73 | `amp_reg_power_state = 0x68`, `amp_power_state_play = 0x03` |
| CHAN_FAULT (70h): bit 3 CH1 DC, bit 2 CH2 DC, bit 1 CH1 OC, bit 0 CH2 OC, each "Clear this fault by setting bit 7 of [FAULT_CLEAR] to 1 or this bit keeps 1" | 77 | `amp_reg_fault_channel = 0x70` |
| GLOBAL_FAULT1 (71h): bit 7 OTP CRC, 6 BQ write, 5 EEPROM load, 2 CLK_FAULT_I, 1 PVDD OV, 0 PVDD UV; the clock, OV and UV faults auto-recover but their bits latch until cleared | 77-78 | `amp_reg_fault_global1 = 0x71`, `amp_clock_fault_bit = 0x04` |
| GLOBAL_FAULT2 (72h): bit 2 CBC CH2, bit 1 CBC CH1, bit 0 OTSD | 79 | `amp_reg_fault_global2 = 0x72` |
| WARNING (73h): CBC warnings and over-temperature warning levels 112 C to 146 C | 79 | not read at bring-up: warnings, not faults (a telemetry candidate for goal 15) |
| FAULT_CLEAR (78h): bit 7 ANALOG_FAULT_CLEAR, "WRITE CLEAR BIT once write this bit to 1, device clears analog fault" | 82 | `amp_reg_fault_clear = 0x78`, `amp_fault_clear_command = 0x80` |
| Every fault bit resets to 0 (reset columns of Tables 9-56 to 9-58) | 77-79 | `amp_fault_clear_value = 0x00` |

## What the datasheet does not say, and what chorus assumes

- **The board's ADR strap (ASSUMED 0x4C).** The reference board's maker configures the part in
  `firmware/esphome/packages/dac-tas58xx.yaml` (esparagus-media-center at `d5e4c58`, Apache-2.0,
  read 2026-09-30) by variant, enable pin and bus only; it names no address. Its driver library
  is GPL-3.0 and its README names none either; its source was not opened. So the address is the
  0 ohm strap's, marked ASSUMED, and the owner's board answer (the boards Needs item) or the S5
  boot log settles it: a wrong address is refused as `amplifier-did-not-answer` with the output
  dead, never guessed around.
- **Whether CLK_FAULT_I sets in Deep Sleep before any clock.** Not stated. The sequencer handles
  both: it excuses that one bit before the clock starts, clears the latch once the clock runs,
  and then reads every fault register with nothing excused. The fake tests both behaviours.
- **I2C in shutdown.** Not stated as a sentence; the startup procedure puts I2C after PDN high
  and the 5 ms wait, and the PDN pin description says shutdown turns off all internal
  regulators. The sequencer never addresses the part with PDN low.
- **DSP coefficients.** Step 5 of the startup procedure initialises DSP coefficients (from TI's
  PPC3 tool, "TAS5825M Process Flows", an application note). chorus loads none: the part runs
  the ROM default flow. Goal 12 (DSP) decides whether any coefficient is ever loaded; the
  digital volume stays at its 0 dB reset value and the analog gain at its lowest.
- **The part's supported ratios vs the board's MCLK.** The part needs no MCLK (p. 1); the
  ESP32-S3's MCLK multiple still sets its internal divider, so `i2s_mclk_multiple = 384` stays
  (ESP-IDF's 24-bit rule) and divides evenly into 64 bit clocks per frame (18.432 MHz / 3.072
  MHz = 6).

## ESP-IDF v6.1: a 24-bit sample in a 32-bit slot

`components/esp_driver_i2s/include/driver/i2s_std.h` at the pinned commit
`fff9895c82d744c7237be8847347bdd1b07c6643` (Apache-2.0): `I2S_STD_PHILIPS_SLOT_DEFAULT_CONFIG`
sets `.slot_bit_width = I2S_SLOT_BIT_WIDTH_AUTO`, which "equals to data bit-width"
(`components/esp_hal_i2s/include/hal/i2s_types.h`); `I2S_SLOT_BIT_WIDTH_32BIT = (32)` exists;
`i2s_std.c` computes `candidate.total_frame_bits = 2 * norm_slot.slot_bit_width` and normalises
a slot narrower than the data up to the data width. The DMA buffer's bytes per sample follow the
data width (`i2s_common.c`, `(data_bit_width + 7) / 8` on this hardware version), so the playout
path's 3-byte samples do not change.
