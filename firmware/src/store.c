/* What the endpoint keeps across boots: the checks in front of the medium.
 *
 * One small seam (chorus/store.h) for the endpoint id, the Noise key, the
 * server pins, the Wi-Fi credentials and the OTA note. On the board the medium
 * is NVS (firmware/main/esp_store.c), in the tests a fake with an event log
 * (firmware/tests/fake_store.c). This unit is what both sit behind: the key
 * rule and the bounds are decided HERE, once, so that a key the board would
 * refuse is refused on the host too and a medium is never asked for something
 * the interface forbids.
 *
 * The key rule is NVS's own, narrowed: "the maximum key length is currently 15
 * characters" (ESP-IDF v6.1 docs/en/api-reference/storage/nvs_flash.rst:33,
 * read 2026-10-02; NVS_KEY_NAME_MAX_SIZE is 16 with the terminator,
 * components/nvs_flash/include/nvs.h:60), and of those only [a-z0-9_], so a key
 * is also a file name in a host store and never needs escaping.
 *
 * Nothing here logs a value. A value may be a credential. */

#include "chorus/store.h"

#include <string.h>

/* 1 when `key` is 1 to CHORUS_STORE_MAX_KEY bytes of [a-z0-9_]. */
static int key_is_valid(const char *key)
{
    if (key == NULL) {
        return 0;
    }
    size_t length = 0;
    for (; key[length] != '\0'; length++) {
        char c = key[length];
        int ok = (c >= 'a' && c <= 'z') || (c >= '0' && c <= '9') || c == '_';
        if (!ok || length >= CHORUS_STORE_MAX_KEY) {
            return 0;
        }
    }
    return length >= 1;
}

chorus_store_status_t chorus_store_get(const chorus_store_t *store, const char *key, void *out,
                                       size_t capacity, size_t *length)
{
    if (length != NULL) {
        *length = 0;
    }
    if (store == NULL || store->get == NULL || length == NULL || (out == NULL && capacity > 0)) {
        return CHORUS_STORE_FAILED;
    }
    if (!key_is_valid(key)) {
        return CHORUS_STORE_BAD_KEY;
    }
    size_t got = 0;
    chorus_store_status_t status = store->get(store->context, key, out, capacity, &got);
    if (status != CHORUS_STORE_OK) {
        return status;
    }
    /* A medium that claims more than the caller's buffer, or more than the
     * interface allows, is not believed: the caller would read past what was
     * written. */
    if (got > capacity || got > CHORUS_STORE_MAX_VALUE) {
        return CHORUS_STORE_TOO_LARGE;
    }
    *length = got;
    return CHORUS_STORE_OK;
}

chorus_store_status_t chorus_store_set(const chorus_store_t *store, const char *key,
                                       const void *value, size_t length)
{
    if (store == NULL || store->set == NULL || (value == NULL && length > 0)) {
        return CHORUS_STORE_FAILED;
    }
    if (!key_is_valid(key)) {
        return CHORUS_STORE_BAD_KEY;
    }
    if (length > CHORUS_STORE_MAX_VALUE) {
        return CHORUS_STORE_TOO_LARGE;
    }
    return store->set(store->context, key, value, length);
}

chorus_store_status_t chorus_store_erase(const chorus_store_t *store, const char *key)
{
    if (store == NULL || store->erase == NULL) {
        return CHORUS_STORE_FAILED;
    }
    if (!key_is_valid(key)) {
        return CHORUS_STORE_BAD_KEY;
    }
    return store->erase(store->context, key);
}

const char *chorus_store_status_name(chorus_store_status_t status)
{
    switch (status) {
    case CHORUS_STORE_OK:
        return "ok";
    case CHORUS_STORE_MISSING:
        return "missing";
    case CHORUS_STORE_TOO_LARGE:
        return "too-large";
    case CHORUS_STORE_BAD_KEY:
        return "bad-key";
    case CHORUS_STORE_FAILED:
        return "failed";
    }
    return "unknown";
}
