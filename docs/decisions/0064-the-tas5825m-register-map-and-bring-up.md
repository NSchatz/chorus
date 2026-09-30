# 0064: the TAS5825M's register map comes from TI's datasheet with its page on every line, and bring-up follows the datasheet's startup procedure with 32-bit slots on the wire

- Status: accepted (goal 9, 2026-09-30)
- Decided by: the goal (brief section 13 item 1; audit A-10's second half; ADR 0015's open
  question "The register map")
- Supersedes: ADR 0015's register-map rows (declared `unknown`) and its bring-up order steps 4-8
- Implemented in: `firmware/config/endpoint.conf` (the `amp_` keys, `i2s_wire_slot_bit_width`),
  `firmware/src/amp.c`, `firmware/include/chorus/amp.h`, `firmware/src/endpoint_config.c`,
  `firmware/src/i2s.c`, `firmware/main/esp_hal.c`, `firmware/main/app_main.c`,
  `firmware/tests/fake_amp.c`, `firmware/tests/test_amp.c`, `tools/conventions/check-amp-map.sh`
- The reading: `docs/research/tas5825m-register-map.md`

## Context

Since EMBEDDED-5 the amplifier's register map was eight keys reading `unknown`, because the
research pass could not extract text from TI's PDF, and the sequencer refused at the first of
them. That kept the endpoint safe and silent. Goal 9 extracted the text rootless (pypdf) from
TI's TAS5825M datasheet, SLASEH7H revision H (January 2023), and read the map off it. Reading it
turned up two defects the `unknown` keys had been hiding:

1. **The sequencer would never have reached the part.** It drove PDN low (its "high impedance"),
   then read the device id over I2C. PDN low is shutdown, with the internal regulators off
   (datasheet p. 4); the datasheet's own startup procedure (p. 42) raises PDN, waits 5 ms, and
   only then talks I2C. On the bench the first read would have been a NACK and the endpoint would
   have reported `amplifier-did-not-answer` for a healthy part.
2. **The bit clock was one the part does not accept.** 24-bit slots at 48 kHz stereo give 48 bit
   clocks per frame; the TAS5825M supports 32 or 64 in I2S format (pp. 7, 29) and reports any
   other ratio as a clock error with its outputs in Hi-Z (p. 29).

## Decision

**The map.** Every register id and value the endpoint uses is an `amp_` key in
`firmware/config/endpoint.conf`, and each line cites its datasheet page on the line itself
(`# TAS5825M SLASEH7H p. N`, short because the endpoint reads the file into a 16 KiB buffer). Twenty-one register-map bytes (address, book and
page select, identity, three fault registers and their clear value, the clock-fault bit, fault
clear, analog gain, audio format, state control and its Hi-Z and Play values, the power-state
register and its Play value), the gain code, three minimum waits, and the bit clocks per frame.
No register literal enters the endpoint source (`endpoint_scan.c` still enforces it). A new rule,
"Datasheet-cited amplifier map" (`docs/conventions.md` rule 23, `check-amp-map.sh`), fails the
gate on an `amp_` line that reads `unknown` or cites no page inside the datasheet's 106.

**The gain.** `amp_analog_gain_db = 0.0` above the part's lowest setting, as before (policy, ADR
0015); the lowest setting is AGAIN code `0x1F`, -15.5 dB (p. 63). The digital volume stays at its
0 dB reset value (p. 57). Raising either wants a measured speaker behind it.

**The address.** `0x4C`, the 0 ohm ADR strap (Table 9-5, p. 39), **ASSUMED**: the reference
board's maker names no strap in its permissively licensed configuration. The boards Needs item
(named by `board_needs_item`) and bench session S5 settle it; a wrong address is refused as a part
that does not answer, with the output dead.

**The faults.** No single register holds them all, so bring-up and the playback poll read
CHAN_FAULT, GLOBAL_FAULT1 and GLOBAL_FAULT2 (pp. 77-79) and require each to read 0. The clock
fault (GLOBAL_FAULT1 bit 2) is reported for a halted clock (p. 29) and latches until FAULT_CLEAR
bit 7 is written (pp. 78, 82); the datasheet does not say whether it sets in Deep Sleep before any
clock, so that one bit is excused only before the endpoint starts the clock, the latch is cleared
once the clock runs, and every register is read again with nothing excused before Play. WARNING
(73h) is a warning and is not graded.

**The sequence** is the datasheet's startup procedure (p. 42) with the endpoint's checks inside it
(`chorus/amp.h` numbers it): PDN low; the refusals that need no bus (gain ceiling, unknown keys,
clock rules, the bit-clock ratio); PDN high and at least 5 ms; book 0, page 0 (p. 41); DIE_ID
`0x95` (p. 72); the faults with the clock bit excused; the gain and SAP_CTRL1 (I2S, 24-bit word,
p. 53); the I2S clock, into a part in Deep Sleep whose output does not switch; FAULT_CLEAR; Hi-Z
with the DSP enabled (`0x02`, p. 48); at least 5 ms; the faults with nothing excused; Play
(`0x03`); POWER_STATE read back as 3 (p. 73). A refusal before the clock starts no clock and
leaves PDN low. A refusal after it (a fault, a part that did not reach Play) drives PDN low first
and stops the clock after, so no clock change is made into a switching output, the invariant ADR
0015 set. A new status names the last case: `amplifier-did-not-reach-play`.

**The shutdown** is the datasheet's (p. 43): Hi-Z over I2C so the digital volume ramps down, at
least 6 ms, PDN low, then the clock (`chorus_amp_shut_down`). The endpoint's ordinary stop paths
(link down, a task that could not start, the session ending) use it; a fault still drops PDN at
once, because a faulted part is not given time.

**The wire slot.** `i2s_wire_slot_bit_width = 32`: the 24-bit sample sits MSB-first in a 32-bit
slot (p. 30, "Audio data word = 24-bit, SCLK = 64fs"), 64 bit clocks per frame. ESP-IDF v6.1
carries a data width inside a wider slot (`i2s_std_slot_config_t::slot_bit_width`; the pinned
tree, Apache-2.0), and the DMA buffer keeps 3-byte samples, so the playout path does not change.
`i2s_slot_bit_width = 24` and `i2s_mclk_multiple = 384` stay (ESP-IDF's 24-bit rule, ADR 0015);
18.432 MHz divides into 3.072 MHz six times. `amp_sclk_per_frame = 64` commits the part, and both
`make firmware-check` and bring-up refuse an I2S configuration that disagrees
(`bclk-ratio-unsupported-by-amplifier`).

## Consequences

- The stage interface (`chorus_output_stage_t`) is now the power-down line and the waits:
  `high_impedance` (PDN low), `power_up` (PDN high) and `wait_ms` (at least, never less: the ESP
  binding waits one tick more than the rounded count). Play is a register, not a pin.
- The fake amplifier models the part from the datasheet with its own register numbers (no I2C
  with PDN low, the 5 ms waits, Deep Sleep at reset, Hi-Z for Play without a clock, latching
  faults), and `test_amp` drives it with the COMMITTED `endpoint.conf`: a healthy bring-up is the
  map and the datasheet agreeing, not the map agreeing with itself.
- Bench session S5 no longer stops at an `unknown` key; the image now brings the part up (or
  refuses by name: a board without the part, or with another strap, answers
  `amplifier-did-not-answer`). `docs/bench-packet.md` S5 says so.
- DSP coefficients (the startup procedure's step 5, TI's PPC3 process flows) are not loaded: the
  part runs its ROM flow. Goal 12 (DSP) decides whether any are.

## Alternatives not taken

- **16-bit slots (32 bit clocks per frame).** Also a supported ratio, but it would drop the 24-bit
  path the endpoint and its MCLK rule were built around and give the DSP of goal 12 less headroom.
- **Probing all four addresses.** It would hide the strap rather than record it; the address is a
  board fact the owner's answer settles, and a wrong one is already refused by name.
- **Reading faults only after the clock.** The datasheet's order starts the clock before Hi-Z, but
  a part that already reports a short or an over-temperature at power-up should never see a clock;
  so the faults are read before as well, with only the clock bit excused.
