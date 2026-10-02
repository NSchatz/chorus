/* The store's ESP-IDF binding: chorus/store.h over NVS.
 *
 * What the board keeps across boots (the endpoint id, the Noise key, the
 * server pins, the Wi-Fi network, the setup secret, the OTA note) lives in
 * one NVS namespace, `chorus`, one blob per key. The key rule and the bounds
 * are firmware/src/store.c's and are graded on a host by
 * firmware/tests/test_provision.c against a simulated store; what is here is
 * the wiring to NVS, and like the rest of firmware/main it is NOT
 * HOST-GRADABLE and NOT CLAIMED.
 *
 * Compiled only by ESP-IDF. The host build never sees this file. */

#ifndef CHORUS_ESP_STORE_H
#define CHORUS_ESP_STORE_H

#include "chorus/store.h"

/* Initialise NVS, once, before anything reads the store. Returns 0 when the
 * store is usable. It never erases: a partition NVS cannot read is reported
 * and left as it is, because what it holds is the speaker's identity and its
 * network, and nothing in the image may throw those away on its own.
 * chorus_esp_store then answers NULL, and each user of the store refuses by
 * name. */
int chorus_esp_store_init(void);

/* The NVS-backed store, or NULL until chorus_esp_store_init has succeeded.
 * Always the same object once it exists; valid for the life of the image.
 * chorus_esp_store_init may be called again by whoever needs the store first:
 * a second call is free. */
const chorus_store_t *chorus_esp_store(void);

#endif /* CHORUS_ESP_STORE_H */
