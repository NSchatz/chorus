/* firmware/include/chorus/store.h: what the endpoint keeps across boots. */
#ifndef CHORUS_STORE_H
#define CHORUS_STORE_H

#include <stddef.h>

/* One value is at most this many bytes (a key, a pin list, a credential). */
#define CHORUS_STORE_MAX_VALUE 1024
/* A key is 1 to 15 bytes of [a-z0-9_] (the NVS key limit). */
#define CHORUS_STORE_MAX_KEY 15

typedef enum {
    CHORUS_STORE_OK = 0,
    CHORUS_STORE_MISSING,   /* the key holds nothing */
    CHORUS_STORE_TOO_LARGE, /* the value does not fit the caller's buffer or the store's bound */
    CHORUS_STORE_BAD_KEY,
    CHORUS_STORE_FAILED /* the medium refused; nothing changed */
} chorus_store_status_t;

typedef struct chorus_store {
    void *context;
    /* Reads the whole value. *length is the value's length on OK. */
    chorus_store_status_t (*get)(void *context, const char *key, void *out, size_t capacity,
                                 size_t *length);
    /* Replaces the whole value, or does nothing: a failed set leaves the old value readable. */
    chorus_store_status_t (*set)(void *context, const char *key, const void *value, size_t length);
    /* Removes the key. Removing a missing key is OK. */
    chorus_store_status_t (*erase)(void *context, const char *key);
} chorus_store_t;

chorus_store_status_t chorus_store_get(const chorus_store_t *store, const char *key, void *out,
                                       size_t capacity, size_t *length);
chorus_store_status_t chorus_store_set(const chorus_store_t *store, const char *key,
                                       const void *value, size_t length);
chorus_store_status_t chorus_store_erase(const chorus_store_t *store, const char *key);
const char *chorus_store_status_name(chorus_store_status_t status);

#endif
