/* The ESP-IDF binding: the three injectable interfaces, backed by real
 * drivers.
 *
 * This is the ONLY part of the endpoint that is not gradeable on a host, and
 * this repository does not claim it. Everything it binds - the protocol core,
 * the sync core, the bring-up sequencer's ORDER, the clock and pin rules, the
 * session supervisor's rejoin - is graded by `make firmware-check` on a
 * machine with no ESP32-S3, against committed fixtures and an injectable
 * transport. What is left here is the wiring, and the two criteria that grade
 * the wiring (AC-1 and AC-3) are the operator-graded ones:
 * `tools/endpoint-rig-run.sh` refuses by name until somebody has the hardware,
 * and docs/verification-record.md quotes that refusal.
 *
 * Compiled only by ESP-IDF. The host build never sees this file. */

#ifndef CHORUS_ESP_HAL_H
#define CHORUS_ESP_HAL_H

#include "chorus/amp.h"
#include "chorus/endpoint_config.h"

/* Bring up the I2C bus, the amplifier's power-down line and the I2S channel
 * described by `config`, and hand back the three interfaces the sequencer
 * takes. Returns 0 on success.
 *
 * The output stage is placed in high impedance by this call, BEFORE the I2S
 * channel is created, so the sequencer's first command finds it already dead
 * rather than establishing it. That is belt and braces: the sequencer asserts
 * high impedance itself, and firmware/tests/test_amp.c grades that it does. */
int chorus_esp_hal_init(const chorus_endpoint_config_t *config, chorus_i2c_bus_t *bus,
                        chorus_output_stage_t *stage, chorus_i2s_controller_t *controller);

#endif /* CHORUS_ESP_HAL_H */
