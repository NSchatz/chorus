/* A simulated key-value store (the board's NVS), with one event log.
 *
 * The same design as fake_amp.c and fake_radio.c (docs/decisions/0015-*): the
 * log is appended by the FAKE and never by the unit under test, which sees
 * only the chorus_store_t it was handed. A test asserts on what reached the
 * medium, in what order, and what the medium answered.
 *
 * It keeps its values for as long as the fake_store_t lives, so a test
 * "reboots" by running the unit a second time over the same fake. A set can be
 * made to fail: the old value stays readable, which is the seam's contract
 * (chorus/store.h) and what a failed NVS commit leaves behind. */

#ifndef CHORUS_FAKE_STORE_H
#define CHORUS_FAKE_STORE_H

#include "chorus/store.h"

#include <stddef.h>
#include <stdint.h>

typedef enum {
    FAKE_STORE_GET,
    FAKE_STORE_SET,
    FAKE_STORE_ERASE
} fake_store_event_kind_t;

typedef struct {
    fake_store_event_kind_t kind;
    char key[CHORUS_STORE_MAX_KEY + 1];
    /* The value's length: what a get returned, what a set was handed. */
    size_t length;
    chorus_store_status_t status;
} fake_store_event_t;

#define FAKE_STORE_MAX_EVENTS 128
#define FAKE_STORE_MAX_ENTRIES 12

typedef struct {
    int used;
    char key[CHORUS_STORE_MAX_KEY + 1];
    uint8_t value[CHORUS_STORE_MAX_VALUE];
    size_t length;
} fake_store_entry_t;

typedef struct {
    fake_store_entry_t entries[FAKE_STORE_MAX_ENTRIES];
    fake_store_event_t events[FAKE_STORE_MAX_EVENTS];
    size_t event_count;
    /* How many of the next sets refuse (the medium fails; nothing changes). */
    size_t sets_to_fail;
    /* Set to make every get refuse with FAILED (a medium that cannot be read). */
    int gets_fail;
} fake_store_t;

void fake_store_init(fake_store_t *fake);

/* The next set refuses with CHORUS_STORE_FAILED and changes nothing. */
void fake_store_fail_next_set(fake_store_t *fake);

/* The store interface, with every call bound to the fake. */
chorus_store_t fake_store_as_store(fake_store_t *fake);

/* How many events of `kind` happened, and how many of them on `key` (NULL
 * counts every key). */
size_t fake_store_count(const fake_store_t *fake, fake_store_event_kind_t kind, const char *key);
/* How many sets of `key` the medium accepted. */
size_t fake_store_sets_kept(const fake_store_t *fake, const char *key);
/* The value the medium holds under `key`, or NULL. Not an event: the test
 * looking is not the unit reading. */
const uint8_t *fake_store_peek(const fake_store_t *fake, const char *key, size_t *length);

const char *fake_store_event_kind_name(fake_store_event_kind_t kind);
void fake_store_print(const fake_store_t *fake);

#endif /* CHORUS_FAKE_STORE_H */
