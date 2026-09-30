/* The playout path on the board: its storage, its lock and its writer task.
 *
 * Everything the path DECIDES is in firmware/src/playout.c and graded on a
 * host by firmware/tests/test_playout.c; this binds it to FreeRTOS and to the
 * I2S driver. NOT HOST-GRADABLE and NOT CLAIMED, like the rest of
 * firmware/main: nothing here has driven a pin, and a timing claim about it
 * needs a hardware report in docs/measurements/.
 *
 * Compiled only by ESP-IDF. */

#ifndef CHORUS_ESP_PLAYOUT_H
#define CHORUS_ESP_PLAYOUT_H

#include "chorus/endpoint_config.h"
#include "chorus/playout.h"
#include "chorus/sync_conf.h"

/* Allocate the jitter buffer from internal RAM, build the path from the two
 * committed configurations and attach it to the I2S interrupt (which preloads
 * the DMA ring). Call after chorus_esp_hal_init and before the amplifier's
 * bring-up starts the clock. Returns the path, or NULL having said why. */
chorus_playout_t *chorus_esp_playout_create(const chorus_endpoint_config_t *config,
                                            const chorus_sync_conf_t *sync);

/* Start the writer task: it fills one DMA buffer at a time, writes it with
 * i2s_channel_write, and ticks the loop every sync_interval_ms (every buffer
 * until the loop has acquired the timeline). Returns 0. */
int chorus_esp_playout_start(chorus_playout_t *playout);

/* The path chorus_esp_playout_create made, or NULL before it (or after it
 * failed): the console's `resources` reads its FIFO depth and marker counts. */
chorus_playout_t *chorus_esp_playout_get(void);

#endif /* CHORUS_ESP_PLAYOUT_H */
