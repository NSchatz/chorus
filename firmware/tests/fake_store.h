/* A simulated key-value store (chorus/store.h), writing into one event log.
 *
 * The same design as fake_amp and fake_radio (docs/decisions/0015-*): the log
 * is appended by the FAKE, never by the unit under test, so what a test reads
 * is what reached the medium and in what order. The unit only ever sees the
 * `chorus_store_t` this hands out.
 *
 * THE LOG NEVER HOLDS A VALUE. An event is the operation, the key, the length
 * and how it ended. Values may be credentials (the Wi-Fi secret, the Noise
 * key), and a log that carried them would be the leak the provisioning test
 * exists to rule out; `fake_store_print` therefore cannot print one either.
 * A test that needs a value reads it with `fake_store_peek`.
 *
 * What it models of the medium: a set either replaces the whole value or
 * changes nothing (`fake_store_fail_next_set` makes the next one refuse and
 * leaves the old value readable, which is the interface's promise and NVS's
 * behaviour on a full page); a "reboot" is simply a second run over the same
 * `fake_store_t`, since the fake is the flash and outlives the unit. */

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
    /* The length written, or read. Never the bytes. */
    size_t length;
    chorus_store_status_t status;
} fake_store_event_t;

#define FAKE_STORE_MAX_EVENTS 256
#define FAKE_STORE_MAX_ENTRIES 16

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

    /* How many of the next sets refuse (the medium fails; nothing changes),
     * after `sets_before_failure` more have succeeded. */
    size_t sets_to_fail;
    size_t sets_before_failure;
    /* Set to make every get, or every erase, answer FAILED: a medium that is
     * not there at all. */
    int gets_fail;
    int erases_fail;
} fake_store_t;

void fake_store_init(fake_store_t *fake);

/* The store interface over the fake. */
chorus_store_t fake_store_as_store(fake_store_t *fake);

/* The next set refuses with CHORUS_STORE_FAILED and changes nothing; the one
 * after it succeeds again. */
void fake_store_fail_next_set(fake_store_t *fake);
/* The same, after `successes` further sets have succeeded: how a test refuses
 * the second write of a pair. */
void fake_store_fail_set_after(fake_store_t *fake, int successes);

/* The value the medium holds under `key`, or NULL. Not an event: the test
 * looking is not the unit reading, and the log is the unit's traffic only. */
const uint8_t *fake_store_peek(const fake_store_t *fake, const char *key, size_t *length);
/* 1 when the key holds a value. */
int fake_store_has(const fake_store_t *fake, const char *key);
/* Put a value there directly, as a previous life of the board would have left
 * it. Not an event. Returns 0 on success. */
int fake_store_preload(fake_store_t *fake, const char *key, const void *value, size_t length);

/* How many events of `kind` touched `key` (NULL: any key). */
size_t fake_store_count(const fake_store_t *fake, fake_store_event_kind_t kind, const char *key);
/* How many sets of `key` the medium accepted. */
size_t fake_store_sets_kept(const fake_store_t *fake, const char *key);

const char *fake_store_event_kind_name(fake_store_event_kind_t kind);
/* One line per event into `out`: `<kind> <key> length=<n> <status>`. Returns
 * the number of bytes written, not counting the terminator. */
size_t fake_store_render(const fake_store_t *fake, char *out, size_t capacity);
void fake_store_print(const fake_store_t *fake);

#endif /* CHORUS_FAKE_STORE_H */
