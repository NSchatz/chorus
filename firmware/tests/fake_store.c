#include "fake_store.h"

#include <stdio.h>
#include <string.h>

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

static fake_store_entry_t *vacant(fake_store_t *fake)
{
    for (size_t i = 0; i < FAKE_STORE_MAX_ENTRIES; i++) {
        if (!fake->entries[i].used) {
            return &fake->entries[i];
        }
    }
    return NULL;
}

void fake_store_init(fake_store_t *fake)
{
    memset(fake, 0, sizeof(*fake));
}

static chorus_store_status_t fake_get(void *context, const char *key, void *out, size_t capacity,
                                      size_t *length)
{
    fake_store_t *fake = (fake_store_t *)context;
    if (fake->gets_fail) {
        record(fake, FAKE_STORE_GET, key, 0, CHORUS_STORE_FAILED);
        return CHORUS_STORE_FAILED;
    }
    const fake_store_entry_t *entry = find(fake, key);
    if (entry == NULL) {
        record(fake, FAKE_STORE_GET, key, 0, CHORUS_STORE_MISSING);
        return CHORUS_STORE_MISSING;
    }
    if (entry->length > capacity) {
        record(fake, FAKE_STORE_GET, key, entry->length, CHORUS_STORE_TOO_LARGE);
        return CHORUS_STORE_TOO_LARGE;
    }
    if (entry->length > 0) {
        memcpy(out, entry->value, entry->length);
    }
    *length = entry->length;
    record(fake, FAKE_STORE_GET, key, entry->length, CHORUS_STORE_OK);
    return CHORUS_STORE_OK;
}

static chorus_store_status_t fake_set(void *context, const char *key, const void *value,
                                      size_t length)
{
    fake_store_t *fake = (fake_store_t *)context;
    if (fake->sets_before_failure > 0) {
        fake->sets_before_failure--;
    } else if (fake->sets_to_fail > 0) {
        /* Refused, and nothing changed: the old value is still readable. */
        fake->sets_to_fail--;
        record(fake, FAKE_STORE_SET, key, length, CHORUS_STORE_FAILED);
        return CHORUS_STORE_FAILED;
    }
    if (length > CHORUS_STORE_MAX_VALUE) {
        record(fake, FAKE_STORE_SET, key, length, CHORUS_STORE_TOO_LARGE);
        return CHORUS_STORE_TOO_LARGE;
    }
    fake_store_entry_t *entry = find(fake, key);
    if (entry == NULL) {
        entry = vacant(fake);
    }
    if (entry == NULL) {
        /* A full medium refuses the way a failed one does. */
        record(fake, FAKE_STORE_SET, key, length, CHORUS_STORE_FAILED);
        return CHORUS_STORE_FAILED;
    }
    entry->used = 1;
    snprintf(entry->key, sizeof(entry->key), "%s", key);
    if (length > 0) {
        memcpy(entry->value, value, length);
    }
    entry->length = length;
    record(fake, FAKE_STORE_SET, key, length, CHORUS_STORE_OK);
    return CHORUS_STORE_OK;
}

static chorus_store_status_t fake_erase(void *context, const char *key)
{
    fake_store_t *fake = (fake_store_t *)context;
    if (fake->erases_fail) {
        record(fake, FAKE_STORE_ERASE, key, 0, CHORUS_STORE_FAILED);
        return CHORUS_STORE_FAILED;
    }
    fake_store_entry_t *entry = find(fake, key);
    if (entry != NULL) {
        /* The bytes go too: an erased credential is not left in the fake for
         * a later read to find. */
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

void fake_store_fail_next_set(fake_store_t *fake)
{
    fake->sets_before_failure = 0;
    fake->sets_to_fail = 1;
}

void fake_store_fail_set_after(fake_store_t *fake, int successes)
{
    fake->sets_before_failure = (successes > 0) ? (size_t)successes : 0;
    fake->sets_to_fail = 1;
}

const uint8_t *fake_store_peek(const fake_store_t *fake, const char *key, size_t *length)
{
    for (size_t i = 0; i < FAKE_STORE_MAX_ENTRIES; i++) {
        if (fake->entries[i].used && strcmp(fake->entries[i].key, key) == 0) {
            if (length != NULL) {
                *length = fake->entries[i].length;
            }
            return fake->entries[i].value;
        }
    }
    if (length != NULL) {
        *length = 0;
    }
    return NULL;
}

int fake_store_has(const fake_store_t *fake, const char *key)
{
    return fake_store_peek(fake, key, NULL) != NULL;
}

int fake_store_preload(fake_store_t *fake, const char *key, const void *value, size_t length)
{
    if (length > CHORUS_STORE_MAX_VALUE || strlen(key) > CHORUS_STORE_MAX_KEY) {
        return -1;
    }
    fake_store_entry_t *entry = find(fake, key);
    if (entry == NULL) {
        entry = vacant(fake);
    }
    if (entry == NULL) {
        return -1;
    }
    entry->used = 1;
    snprintf(entry->key, sizeof(entry->key), "%s", key);
    if (length > 0) {
        memcpy(entry->value, value, length);
    }
    entry->length = length;
    return 0;
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

size_t fake_store_render(const fake_store_t *fake, char *out, size_t capacity)
{
    size_t used = 0;
    if (capacity > 0) {
        out[0] = '\0';
    }
    for (size_t i = 0; i < fake->event_count && used + 1 < capacity; i++) {
        const fake_store_event_t *event = &fake->events[i];
        int n = snprintf(out + used, capacity - used, "%s %s length=%zu %s\n",
                         fake_store_event_kind_name(event->kind), event->key, event->length,
                         chorus_store_status_name(event->status));
        if (n < 0 || (size_t)n >= capacity - used) {
            used = capacity - 1;
            break;
        }
        used += (size_t)n;
    }
    return used;
}

void fake_store_print(const fake_store_t *fake)
{
    for (size_t i = 0; i < fake->event_count; i++) {
        const fake_store_event_t *event = &fake->events[i];
        printf("     %zu %s %s length=%zu %s\n", i, fake_store_event_kind_name(event->kind),
               event->key, event->length, chorus_store_status_name(event->status));
    }
}
