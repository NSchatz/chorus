/* The subwoofer's level and phase knobs on the board (K69, ADR 0063).
 *
 * What a code means (its step, its hysteresis, a cut never a boost) is decided
 * in firmware/src/controls.c and graded on a host by
 * firmware/tests/test_controls.c; how the knobs reach the sound chain
 * (chorus_playout_set_sub_knobs, then chorus_endpoint_dsp_settings' sub level
 * and polarity) by firmware/tests/test_endpoint_dsp.c. This binds the two
 * wipers (pin_knob_level, pin_knob_phase: linear potentiometers across 3.3 V,
 * the anticlockwise end at 0 V) to ADC1 one-shot reads at 12 bits, stamped on
 * the monotonic clock, in a small task. NOT HOST-GRADABLE and NOT CLAIMED,
 * like the rest of firmware/main: what a knob reads on the owner's board is a
 * bench item.
 *
 * Compiled only by ESP-IDF. */

#ifndef CHORUS_ESP_KNOBS_H
#define CHORUS_ESP_KNOBS_H

#include "chorus/endpoint_config.h"
#include "chorus/playout.h"

/* Configure the knobs' ADC1 channels and start the task that reads them into
 * `playout`'s sound chain. Does nothing and returns 0 when the board wires no
 * knob. Returns -1 having said why; the endpoint plays on with the knobs at
 * 0 dB and 0 degrees. */
int chorus_esp_knobs_start(const chorus_endpoint_config_t *config, chorus_playout_t *playout);

#endif /* CHORUS_ESP_KNOBS_H */
