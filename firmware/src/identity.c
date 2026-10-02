#include "chorus/identity.h"

#include <stdio.h>
#include <string.h>

static const char HEX[] = "0123456789abcdef";

const char *chorus_identity_status_name(chorus_identity_status_t status)
{
    switch (status) {
    case CHORUS_IDENTITY_OK:
        return "ok";
    case CHORUS_IDENTITY_NO_RANDOM:
        return "no-random";
    case CHORUS_IDENTITY_UNREADABLE:
        return "store-unreadable";
    case CHORUS_IDENTITY_NOT_AN_IDENTITY:
        return "not-an-identity";
    case CHORUS_IDENTITY_NOT_SAVED:
        return "not-saved";
    }
    return "unknown";
}

static int random_bytes(chorus_noise_random_fn random, void *random_ctx, uint8_t *out, size_t len)
{
    chorus_noise_random_fn source = (random != NULL) ? random : chorus_noise_system_random;
    return source(random_ctx, out, len);
}

int chorus_identity_id_is_wellformed(const char *id)
{
    const size_t prefix = strlen(CHORUS_IDENTITY_ID_PREFIX);
    if (id == NULL || strlen(id) != CHORUS_IDENTITY_ID_LEN ||
        strncmp(id, CHORUS_IDENTITY_ID_PREFIX, prefix) != 0) {
        return 0;
    }
    for (size_t i = prefix; i < CHORUS_IDENTITY_ID_LEN; i++) {
        char c = id[i];
        if (!((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f'))) {
            return 0;
        }
    }
    return 1;
}

chorus_identity_status_t chorus_identity_id(const chorus_store_t *store,
                                            chorus_noise_random_fn random, void *random_ctx,
                                            char *id, size_t id_capacity, int *created)
{
    if (created != NULL) {
        *created = 0;
    }
    if (id == NULL || id_capacity < CHORUS_IDENTITY_ID_LEN + 1) {
        return CHORUS_IDENTITY_NOT_AN_IDENTITY;
    }
    id[0] = '\0';
    char held[CHORUS_IDENTITY_ID_LEN + 1];
    size_t length = 0;
    chorus_store_status_t got =
        chorus_store_get(store, CHORUS_IDENTITY_KEY_ID, held, CHORUS_IDENTITY_ID_LEN, &length);
    if (got == CHORUS_STORE_OK) {
        held[length] = '\0';
        if (length != CHORUS_IDENTITY_ID_LEN || !chorus_identity_id_is_wellformed(held)) {
            return CHORUS_IDENTITY_NOT_AN_IDENTITY;
        }
        memcpy(id, held, CHORUS_IDENTITY_ID_LEN + 1);
        return CHORUS_IDENTITY_OK;
    }
    if (got == CHORUS_STORE_TOO_LARGE) {
        /* Something longer than an id lives under the id's key. */
        return CHORUS_IDENTITY_NOT_AN_IDENTITY;
    }
    if (got != CHORUS_STORE_MISSING) {
        /* Unreadable is not absent: making a new id over a store that only
         * failed to answer would turn one speaker into two. */
        return CHORUS_IDENTITY_UNREADABLE;
    }

    uint8_t raw[CHORUS_IDENTITY_ID_RANDOM_BYTES];
    if (random_bytes(random, random_ctx, raw, sizeof(raw)) != 0) {
        return CHORUS_IDENTITY_NO_RANDOM;
    }
    size_t at = strlen(CHORUS_IDENTITY_ID_PREFIX);
    memcpy(held, CHORUS_IDENTITY_ID_PREFIX, at);
    for (size_t i = 0; i < sizeof(raw); i++) {
        held[at++] = HEX[raw[i] >> 4];
        held[at++] = HEX[raw[i] & 0x0F];
    }
    held[at] = '\0';
    if (chorus_store_set(store, CHORUS_IDENTITY_KEY_ID, held, CHORUS_IDENTITY_ID_LEN) !=
        CHORUS_STORE_OK) {
        return CHORUS_IDENTITY_NOT_SAVED;
    }
    memcpy(id, held, CHORUS_IDENTITY_ID_LEN + 1);
    if (created != NULL) {
        *created = 1;
    }
    return CHORUS_IDENTITY_OK;
}

chorus_identity_status_t chorus_identity_secret(const chorus_store_t *store,
                                                chorus_noise_random_fn random, void *random_ctx,
                                                uint8_t secret[CHORUS_NOISE_KEY_LEN], int *created)
{
    if (created != NULL) {
        *created = 0;
    }
    uint8_t held[CHORUS_NOISE_KEY_LEN];
    size_t length = 0;
    chorus_store_status_t got =
        chorus_store_get(store, CHORUS_IDENTITY_KEY_NOISE, held, sizeof(held), &length);
    if (got == CHORUS_STORE_OK) {
        if (length != CHORUS_NOISE_KEY_LEN) {
            memset(held, 0, sizeof(held));
            return CHORUS_IDENTITY_NOT_AN_IDENTITY;
        }
        memcpy(secret, held, CHORUS_NOISE_KEY_LEN);
        memset(held, 0, sizeof(held));
        return CHORUS_IDENTITY_OK;
    }
    if (got == CHORUS_STORE_TOO_LARGE) {
        return CHORUS_IDENTITY_NOT_AN_IDENTITY;
    }
    if (got != CHORUS_STORE_MISSING) {
        return CHORUS_IDENTITY_UNREADABLE;
    }

    if (random_bytes(random, random_ctx, held, sizeof(held)) != 0) {
        return CHORUS_IDENTITY_NO_RANDOM;
    }
    if (chorus_store_set(store, CHORUS_IDENTITY_KEY_NOISE, held, sizeof(held)) != CHORUS_STORE_OK) {
        /* The key is dropped, not used for this boot: a server that pinned it
         * would refuse the next boot's key, and that refusal does not clear
         * by itself. */
        memset(held, 0, sizeof(held));
        return CHORUS_IDENTITY_NOT_SAVED;
    }
    memcpy(secret, held, CHORUS_NOISE_KEY_LEN);
    memset(held, 0, sizeof(held));
    if (created != NULL) {
        *created = 1;
    }
    return CHORUS_IDENTITY_OK;
}

/* --- the pinned servers ------------------------------------------------------ */

static int nibble_of(char c)
{
    if (c >= '0' && c <= '9') {
        return c - '0';
    }
    if (c >= 'a' && c <= 'f') {
        return c - 'a' + 10;
    }
    if (c >= 'A' && c <= 'F') {
        return c - 'A' + 10;
    }
    return -1;
}

static int unhex(const char *text, uint8_t *out, size_t len)
{
    for (size_t i = 0; i < len; i++) {
        int high = nibble_of(text[2 * i]);
        int low = nibble_of(text[2 * i + 1]);
        if (high < 0 || low < 0) {
            return -1;
        }
        out[i] = (uint8_t)((high << 4) | low);
    }
    return 0;
}

int chorus_pins_parse(const char *text, size_t length, chorus_pins_t *pins, int *bad_line)
{
    /* `pinned <64 hex digits> <id>`, the adoption store's text form. */
    const size_t key_at = 7;
    const size_t id_at = key_at + 2 * CHORUS_NOISE_KEY_LEN + 1;
    memset(pins, 0, sizeof(*pins));
    size_t n = 0;
    int line_number = 0;
    size_t at = 0;
    while (at < length) {
        const char *line = text + at;
        const char *eol = memchr(line, '\n', length - at);
        size_t len = (eol == NULL) ? length - at : (size_t)(eol - line);
        at += len + ((eol == NULL) ? 0 : 1);
        line_number++;
        if (len > 0 && line[len - 1] == '\r') {
            len--;
        }
        if (len == 0 || line[0] == '#') {
            continue;
        }
        int bad = len <= id_at || memcmp(line, "pinned ", key_at) != 0 || line[id_at - 1] != ' ' ||
                  n >= CHORUS_PINS_MAX || len - id_at >= CHORUS_PIN_ID_MAX ||
                  memchr(line, '\0', len) != NULL ||
                  unhex(line + key_at, pins->pins[n].key, CHORUS_NOISE_KEY_LEN) != 0;
        for (size_t i = 0; !bad && i < n; i++) {
            /* An id met twice has two pins, and a store that says two things
             * about one server says nothing that can be trusted. */
            bad = strlen(pins->pins[i].id) == len - id_at &&
                  memcmp(pins->pins[i].id, line + id_at, len - id_at) == 0;
        }
        if (bad) {
            if (bad_line != NULL) {
                *bad_line = line_number;
            }
            memset(pins, 0, sizeof(*pins));
            return -1;
        }
        pins->pins[n].used = 1;
        memcpy(pins->pins[n].id, line + id_at, len - id_at);
        pins->pins[n].id[len - id_at] = '\0';
        n++;
    }
    return 0;
}

long chorus_pins_render(const chorus_pins_t *pins, char *out, size_t capacity)
{
    size_t at = 0;
    int wrote =
        snprintf(out, capacity, "# chorus adopted peers: <pinned|removed> <public key hex> <id>\n");
    if (wrote < 0 || (size_t)wrote >= capacity) {
        return -1;
    }
    at = (size_t)wrote;
    for (size_t i = 0; i < CHORUS_PINS_MAX; i++) {
        if (!pins->pins[i].used) {
            continue;
        }
        char hex[2 * CHORUS_NOISE_KEY_LEN + 1];
        for (size_t b = 0; b < CHORUS_NOISE_KEY_LEN; b++) {
            hex[2 * b] = HEX[pins->pins[i].key[b] >> 4];
            hex[2 * b + 1] = HEX[pins->pins[i].key[b] & 0x0F];
        }
        hex[2 * CHORUS_NOISE_KEY_LEN] = '\0';
        wrote = snprintf(out + at, capacity - at, "pinned %s %s\n", hex, pins->pins[i].id);
        if (wrote < 0 || (size_t)wrote >= capacity - at) {
            return -1;
        }
        at += (size_t)wrote;
    }
    return (long)at;
}

chorus_pin_verdict_t chorus_pins_check(chorus_pins_t *pins, const char *id,
                                       const uint8_t key[CHORUS_NOISE_KEY_LEN],
                                       uint8_t pinned[CHORUS_NOISE_KEY_LEN],
                                       chorus_pins_save_fn save, void *save_ctx)
{
    for (size_t i = 0; i < CHORUS_PINS_MAX; i++) {
        if (pins->pins[i].used && strcmp(pins->pins[i].id, id) == 0) {
            memcpy(pinned, pins->pins[i].key, CHORUS_NOISE_KEY_LEN);
            return (memcmp(pinned, key, CHORUS_NOISE_KEY_LEN) == 0) ? CHORUS_PIN_KNOWN
                                                                    : CHORUS_PIN_CHANGED;
        }
    }
    /* An id is one line of the text form: one that is empty, too long or holds
     * a line break could be written and never read back as itself. */
    size_t id_len = strlen(id);
    if (id_len == 0 || id_len >= CHORUS_PIN_ID_MAX || strpbrk(id, "\r\n") != NULL) {
        return CHORUS_PIN_UNSAVED;
    }
    for (size_t i = 0; i < CHORUS_PINS_MAX; i++) {
        if (!pins->pins[i].used) {
            pins->pins[i].used = 1;
            memcpy(pins->pins[i].id, id, id_len + 1);
            memcpy(pins->pins[i].key, key, CHORUS_NOISE_KEY_LEN);
            if (save != NULL && save(save_ctx, pins) != 0) {
                /* A pin that could not be kept is not an adoption: the next
                 * start would pin whatever key came first. */
                memset(&pins->pins[i], 0, sizeof(pins->pins[i]));
                return CHORUS_PIN_UNSAVED;
            }
            return CHORUS_PIN_ADOPTED;
        }
    }
    return CHORUS_PIN_UNSAVED;
}

chorus_identity_status_t chorus_identity_pins_load(const chorus_store_t *store, chorus_pins_t *pins,
                                                   int *bad_line, char *scratch,
                                                   size_t scratch_capacity)
{
    size_t length = 0;
    memset(pins, 0, sizeof(*pins));
    if (bad_line != NULL) {
        *bad_line = 0;
    }
    chorus_store_status_t got =
        chorus_store_get(store, CHORUS_IDENTITY_KEY_PINS, scratch, scratch_capacity, &length);
    if (got == CHORUS_STORE_MISSING) {
        return CHORUS_IDENTITY_OK;
    }
    if (got != CHORUS_STORE_OK) {
        return CHORUS_IDENTITY_UNREADABLE;
    }
    if (chorus_pins_parse(scratch, length, pins, bad_line) != 0) {
        return CHORUS_IDENTITY_NOT_AN_IDENTITY;
    }
    return CHORUS_IDENTITY_OK;
}

int chorus_identity_pins_save(const chorus_store_t *store, const chorus_pins_t *pins, char *scratch,
                              size_t scratch_capacity)
{
    long length = chorus_pins_render(pins, scratch, scratch_capacity);
    if (length < 0) {
        return -1;
    }
    return (chorus_store_set(store, CHORUS_IDENTITY_KEY_PINS, scratch, (size_t)length) ==
            CHORUS_STORE_OK)
               ? 0
               : -1;
}

/* --- the last server that answered ------------------------------------------ */

int chorus_identity_server_load(const chorus_store_t *store, char *address, size_t capacity)
{
    size_t length = 0;
    if (capacity == 0) {
        return 0;
    }
    address[0] = '\0';
    if (chorus_store_get(store, CHORUS_IDENTITY_KEY_SERVER, address, capacity - 1, &length) !=
            CHORUS_STORE_OK ||
        length == 0 || memchr(address, '\0', length) != NULL) {
        address[0] = '\0';
        return 0;
    }
    address[length] = '\0';
    return 1;
}

int chorus_identity_server_save(const chorus_store_t *store, const char *address)
{
    char held[256];
    size_t length = strlen(address);
    if (length == 0 || length >= sizeof(held)) {
        return -1;
    }
    if (chorus_identity_server_load(store, held, sizeof(held)) == 1 && strcmp(held, address) == 0) {
        return 0;
    }
    return (chorus_store_set(store, CHORUS_IDENTITY_KEY_SERVER, address, length) == CHORUS_STORE_OK)
               ? 0
               : -1;
}
