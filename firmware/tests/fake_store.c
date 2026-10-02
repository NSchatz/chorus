#include "fake_store.h"

#include <stdio.h>
#include <string.h>

void fake_store_init(fake_store_t *fake)
{
    memset(fake, 0, sizeof(*fake));
}

void fake_store_fail_next_set(fake_store_t *fake)
{
    fake->sets_to_fail++;
}

static void record(fake_store_t *fake, fake_store_event_kind_t kind, const char *key, size_t length,
                   chorus_store_status_t status)
{
    if (fake->event_count >= FAKE_STORE_MAX_EVENTS) {
        return;
    }
    fake_store_event_t *event = &fake->events[fake->event_count++];
    event->kind = kind;
    snprintf(event->key, sizeof(event->key), "%s", key);
    event->length = length;
    event->status = status;
}

static fake_store_entry_t *find(fake_store_t *fake, const char *key)
{
    for (size_t i = 0; i < FAKE_STORE_MAX_ENTRIES; i++) {
        if (fake->entries[i].used && strcmp(fake->entries[i].key, key) == 0) {
            return &fake->entries[i];
        }
    }
    return NULL;
}

static chorus_store_status_t fake_get(void *context, const char *key, void *out, size_t capacity,
                                      size_t *length)
{
    fake_store_t *fake = (fake_store_t *)context;
    chorus_store_status_t status = CHORUS_STORE_OK;
    size_t found = 0;
    const fake_store_entry_t *entry = find(fake, key);
    if (fake->gets_fail) {
        status = CHORUS_STORE_FAILED;
    } else if (entry == NULL) {
        status = CHORUS_STORE_MISSING;
    } else if (entry->length > capacity) {
        status = CHORUS_STORE_TOO_LARGE;
    } else {
        memcpy(out, entry->value, entry->length);
        found = entry->length;
        *length = found;
    }
    record(fake, FAKE_STORE_GET, key, found, status);
    return status;
}

static chorus_store_status_t fake_set(void *context, const char *key, const void *value,
                                      size_t length)
{
    fake_store_t *fake = (fake_store_t *)context;
    chorus_store_status_t status = CHORUS_STORE_OK;
    if (fake->sets_to_fail > 0) {
        fake->sets_to_fail--;
        status = CHORUS_STORE_FAILED;
    } else {
        fake_store_entry_t *entry = find(fake, key);
        for (size_t i = 0; entry == NULL && i < FAKE_STORE_MAX_ENTRIES; i++) {
            if (!fake->entries[i].used) {
                entry = &fake->entries[i];
            }
        }
        if (entry == NULL || length > sizeof(entry->value)) {
            /* A full medium refuses, as a full NVS partition does. */
            status = CHORUS_STORE_FAILED;
        } else {
            entry->used = 1;
            snprintf(entry->key, sizeof(entry->key), "%s", key);
            if (length > 0) {
                memcpy(entry->value, value, length);
            }
            entry->length = length;
        }
    }
    record(fake, FAKE_STORE_SET, key, length, status);
    return status;
}

static chorus_store_status_t fake_erase(void *context, const char *key)
{
    fake_store_t *fake = (fake_store_t *)context;
    fake_store_entry_t *entry = find(fake, key);
    if (entry != NULL) {
        memset(entry, 0, sizeof(*entry));
    }
    record(fake, FAKE_STORE_ERASE, key, 0, CHORUS_STORE_OK);
    return CHORUS_STORE_OK;
}

chorus_store_t fake_store_as_store(fake_store_t *fake)
{
    chorus_store_t store;
    store.context = fake;
    store.get = fake_get;
    store.set = fake_set;
    store.erase = fake_erase;
    return store;
}

size_t fake_store_count(const fake_store_t *fake, fake_store_event_kind_t kind, const char *key)
{
    size_t count = 0;
    for (size_t i = 0; i < fake->event_count; i++) {
        if (fake->events[i].kind == kind &&
            (key == NULL || strcmp(fake->events[i].key, key) == 0)) {
            count++;
        }
    }
    return count;
}

size_t fake_store_sets_kept(const fake_store_t *fake, const char *key)
{
    size_t count = 0;
    for (size_t i = 0; i < fake->event_count; i++) {
        if (fake->events[i].kind == FAKE_STORE_SET && fake->events[i].status == CHORUS_STORE_OK &&
            strcmp(fake->events[i].key, key) == 0) {
            count++;
        }
    }
    return count;
}

const uint8_t *fake_store_peek(const fake_store_t *fake, const char *key, size_t *length)
{
    for (size_t i = 0; i < FAKE_STORE_MAX_ENTRIES; i++) {
        if (fake->entries[i].used && strcmp(fake->entries[i].key, key) == 0) {
            *length = fake->entries[i].length;
            return fake->entries[i].value;
        }
    }
    *length = 0;
    return NULL;
}

const char *fake_store_event_kind_name(fake_store_event_kind_t kind)
{
    switch (kind) {
    case FAKE_STORE_GET:
        return "get";
    case FAKE_STORE_SET:
        return "set";
    case FAKE_STORE_ERASE:
        return "erase";
    }
    return "unknown";
}

void fake_store_print(const fake_store_t *fake)
{
    /* Keys, lengths and verdicts only: a value may be a key or a credential. */
    for (size_t i = 0; i < fake->event_count; i++) {
        printf("    store %s %s length=%zu %s\n", fake_store_event_kind_name(fake->events[i].kind),
               fake->events[i].key, fake->events[i].length,
               chorus_store_status_name(fake->events[i].status));
    }
}
