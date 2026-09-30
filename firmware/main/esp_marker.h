/* The GPIO marker on the board (BRIEF.md section 10's digital cross-check).
 *
 * Which boundary is marked, and when its buffer starts, is decided in
 * firmware/src/playout.c (chorus_playout_marker_due) and graded on a host by
 * firmware/tests/test_playout.c. This binds it to a GPIO and a GPTimer: the
 * I2S `on_sent` interrupt asks the playout path whether the buffer now
 * starting holds a marked boundary, and if so arms a one-shot alarm for the
 * boundary's instant, whose callback sets the pin to the boundary's parity.
 * NOT HOST-GRADABLE and NOT CLAIMED, like the rest of firmware/main: the
 * edge's placement against the audio at the amplifier's output is what the
 * EMBEDDED-5 bench session measures (docs/bench-packet.md).
 *
 * Compiled only by ESP-IDF. */

#ifndef CHORUS_ESP_MARKER_H
#define CHORUS_ESP_MARKER_H

#include "chorus/endpoint_config.h"
#include "chorus/playout.h"

/* Configure the marker pin (driven low) and its timer. Does nothing and
 * returns 0 when pin_marker is `none`. Returns -1 having said why. */
int chorus_esp_marker_start(const chorus_endpoint_config_t *config);

/* In the I2S interrupt, right after chorus_playout_on_dma_sent. */
void chorus_esp_marker_on_dma_sent(chorus_playout_t *playout);

#endif /* CHORUS_ESP_MARKER_H */
