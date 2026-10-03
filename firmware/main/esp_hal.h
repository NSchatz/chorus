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

#include "driver/i2s_std.h"

#include "chorus/amp.h"
#include "chorus/endpoint_config.h"
#include "chorus/playout.h"
#include "chorus/session.h"
#include "chorus/wifi.h"

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

/* The largest DMA buffer the playout binding preloads, in bytes: 240 frames of
 * two 32-bit slots, above the committed 240 x 2 x 3 (firmware/config/endpoint.conf). */
#define CHORUS_ESP_HAL_MAX_DMA_BYTES 1920u

/* Attach the playout path to the I2S TX interrupt and preload the stopped
 * channel's DMA buffers with silence, counted as written. Must run after
 * chorus_esp_hal_init and BEFORE the amplifier's bring-up first starts the
 * clock. Returns 0. */
int chorus_esp_hal_attach_playout(chorus_playout_t *playout,
                                  const chorus_endpoint_config_t *config);

/* The TX channel the playout writer writes to. */
i2s_chan_handle_t chorus_esp_hal_i2s_tx(void);

/* The radio, as the wireless bring-up takes it.
 *
 * Hands back the five calls `chorus/wifi.h` declares, backed by the platform's
 * own. Nothing is brought up by this function itself: it binds, and
 * `chorus_wifi_bring_up` decides. Which mode to set, whether the readback
 * agrees, whether a coexistence makes the setting ineffective and whether to
 * join at all with a credential unknown are all decisions, they are all in
 * firmware/src/wifi.c, and they are all graded by firmware/tests/test_wifi.c on
 * a machine with no radio.
 *
 * NOT HOST-GRADABLE and NOT CLAIMED, exactly as the rest of this file is. The
 * criterion that would grade this wiring is AC-2, which is operator graded and
 * NOT passed; tools/wireless-characterization-run.sh refuses by name until
 * somebody has an ESP32-S3 on a wireless link and the capture rig, and
 * docs/verification-record.md quotes that refusal. */
void chorus_esp_hal_radio(chorus_radio_t *radio);

/* The session's health seam on the board (goal 15, chorus/session.h
 * `health`); `ctx` is the board's `chorus_transport_t`, by pointer.
 *
 * What it fills, and from where: `link` from the committed transport (wired,
 * wireless; the emulator's link stays unknown, it is neither); `rssi_dbm`
 * from esp_wifi_sta_get_rssi on a wireless board that is associated; the heap
 * from heap_caps over the internal 8-bit heap, the figures the console's
 * `resources` prints. The temperature stays UNKNOWN: no board profile has a
 * temperature sensor, and the SoC's die sensor is not a board temperature
 * (docs/telemetry.md names it as a follow-up).
 *
 * NOT HOST-GRADABLE and NOT CLAIMED, as the rest of this file: the values a
 * real board reports are a bench item (docs/telemetry.md, "Bench"). */
void chorus_esp_hal_health(void *ctx, chorus_session_health_t *health);

#endif /* CHORUS_ESP_HAL_H */
