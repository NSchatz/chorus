/* The endpoint console (audit A-13), bound to ESP-IDF's console REPL.
 *
 * Every decision is in firmware/src/console.c and graded on a host by
 * firmware/tests/test_console.c; this binds it to the serial line the board
 * boots its console on and to the session's two hooks.
 *
 * NOT HOST-GRADABLE and NOT CLAIMED, like the rest of firmware/main: whether a
 * reply reaches the bench machine's serial port is a bench session's question
 * (tools/decode-cost-run.sh, tools/wireless-characterization-run.sh). */

#ifndef CHORUS_CONSOLE_ESP_H
#define CHORUS_CONSOLE_ESP_H

#include "chorus/endpoint_config.h"
#include "chorus/session.h"

/* Start the REPL on its own task, as soon as the committed configuration has
 * parsed: before the amplifier and the link, so the console answers (and
 * decode-cost runs) on a board whose bring-up stops early. Returns 0 when the
 * REPL started; on failure the endpoint runs on without a console and says so
 * in its log. */
int chorus_esp_console_start(const chorus_endpoint_config_t *config);

/* Install the console's two hooks into `session`, which the caller then runs:
 * `server` applies from its next connection attempt and `status` prints what
 * it last published. */
void chorus_esp_console_attach(chorus_session_config_t *session);

#endif /* CHORUS_CONSOLE_ESP_H */
