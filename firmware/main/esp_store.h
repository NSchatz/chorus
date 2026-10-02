/* The store seam's board binding (chorus/store.h): NVS, namespace `chorus`,
 * one blob per key.
 *
 * NVS encryption stays off: it is on the refused Kconfig list
 * (firmware/check/efuse-kconfig.list), because turning it on is an eFuse
 * decision and this tree makes none. What the store holds is therefore
 * readable by whoever holds the board, which is the same trust the pins of a
 * speaker already rest on.
 *
 * NOT HOST-GRADABLE and NOT CLAIMED, like the rest of this directory: what the
 * endpoint DOES with the store is firmware/src/identity.c's, graded on a host
 * over a fake. Compiled only by ESP-IDF. */

#ifndef CHORUS_ESP_STORE_H
#define CHORUS_ESP_STORE_H

#include "chorus/store.h"

/* Initialise NVS. May be called more than once. Returns 0 when the store can
 * be used. NVS is never erased as a whole by this call: a partition that will
 * not initialise is refused by name, because erasing it would erase the
 * board's identity and its Wi-Fi credentials with it. */
int chorus_esp_store_init(void);

/* The NVS-backed store, or NULL before a successful chorus_esp_store_init. */
const chorus_store_t *chorus_esp_store(void);

#endif /* CHORUS_ESP_STORE_H */
