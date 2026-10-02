/* The board's identity, bound to the session (goal 14): the NVS-backed store
 * handed to the session supervisor, and the id it presents.
 *
 * Every decision (what an id is, when one is made, that a value the store did
 * not keep is refused) is firmware/src/identity.c's and is graded on a host by
 * firmware/tests/test_identity.c over a fake store. What is here is the
 * binding, NOT HOST-GRADABLE and NOT CLAIMED like the rest of this directory.
 * Compiled only by ESP-IDF. */

#ifndef CHORUS_ESP_IDENTITY_H
#define CHORUS_ESP_IDENTITY_H

#include "chorus/session.h"

/* Read this board's id and key from the store, making and keeping them at the
 * first boot, and bind the store and the id to `session`. Returns 0, or -1
 * when the board has no identity it can keep: the caller then opens no
 * session, because a session under an id and key that change at the next boot
 * is adopted once and refused ever after. Logs the id and the key's
 * fingerprint, never the key. */
int chorus_esp_identity_load(chorus_session_config_t *session);

#endif /* CHORUS_ESP_IDENTITY_H */
