/* The store seam's own half: the key and the bounds are checked here, once,
 * so no medium behind it (NVS on the board, the fake in the tests, files in
 * the host session binary) ever sees a key it could not hold or a value over
 * the bound, and every medium refuses the same things by the same name. */

#include "chorus/store.h"

#include <string.h>

static int key_is_valid(const char *key)
{
    if (key == NULL) {
        return 0;
    }
    size_t length = strlen(key);
    if (length < 1 || length > CHORUS_STORE_MAX_KEY) {
        return 0;
    }
    for (size_t i = 0; i < length; i++) {
        char c = key[i];
        if (!((c >= 'a' && c <= 'z') || (c >= '0' && c <= '9') || c == '_')) {
            return 0;
        }
    }
    return 1;
}

chorus_store_status_t chorus_store_get(const chorus_store_t *store, const char *key, void *out,
                                       size_t capacity, size_t *length)
{
    if (!key_is_valid(key)) {
        return CHORUS_STORE_BAD_KEY;
    }
    if (store == NULL || store->get == NULL || out == NULL || length == NULL) {
        return CHORUS_STORE_FAILED;
    }
    *length = 0;
    size_t found = 0;
    chorus_store_status_t status = store->get(store->context, key, out, capacity, &found);
    if (status != CHORUS_STORE_OK) {
        return status;
    }
    /* A medium that says OK and hands back more than it was given room for is
     * a medium that wrote past the buffer or lied about the length; either
     * way the value is not one to use. */
    if (found > capacity || found > CHORUS_STORE_MAX_VALUE) {
        return CHORUS_STORE_TOO_LARGE;
    }
    *length = found;
    return CHORUS_STORE_OK;
}

chorus_store_status_t chorus_store_set(const chorus_store_t *store, const char *key,
                                       const void *value, size_t length)
{
    if (!key_is_valid(key)) {
        return CHORUS_STORE_BAD_KEY;
    }
    if (length > CHORUS_STORE_MAX_VALUE) {
        return CHORUS_STORE_TOO_LARGE;
    }
    if (store == NULL || store->set == NULL || (value == NULL && length != 0)) {
        return CHORUS_STORE_FAILED;
    }
    return store->set(store->context, key, value, length);
}

chorus_store_status_t chorus_store_erase(const chorus_store_t *store, const char *key)
{
    if (!key_is_valid(key)) {
        return CHORUS_STORE_BAD_KEY;
    }
    if (store == NULL || store->erase == NULL) {
        return CHORUS_STORE_FAILED;
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
