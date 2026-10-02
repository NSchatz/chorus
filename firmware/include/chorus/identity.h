/* Who this board is, kept across boots (goal 14, docs/decisions/0104-*).
 *
 * Before this unit a board had no identity of its own: every board presented
 * the id `chorus-endpoint` and a Noise key made fresh at each boot, so the
 * server adopted its first boot and refused its second with `key_changed`
 * (docs/protocol.md, "Adoption: trust on first use"). An endpoint that is
 * adopted once and stays adopted needs three things to outlive a power cut:
 *
 *   id          `chorus-` and twelve lower-case hex digits, six bytes from the
 *               random source at first boot. NOT the MAC address: an id is
 *               written into logs, the server's state and pasted bench output,
 *               and a hardware address there is identity the owner did not
 *               choose to publish (K27). Forty-eight random bits: see the
 *               collision note in the decision record.
 *   noise_key   the endpoint's long-term X25519 secret, 32 bytes.
 *   server_pins the servers this endpoint has met, in the adoption store's
 *               text form (`pinned <public key hex> <server id>` per line).
 *
 * plus one convenience, `server_addr`: the address of the last server a
 * session completed its handshake with, which discovery falls back to
 * (chorus/discovery.h).
 *
 * Everything goes through the store seam (chorus/store.h): NVS on the board, a
 * fake in the tests, a directory in the host session binary. This unit is
 * pure: no ESP-IDF, no clock, no heap, no file.
 *
 * The rule the tests grade: A VALUE THAT COULD NOT BE SAVED IS NOT USED. An
 * id or key that was made and not kept would be a different one at the next
 * boot, which is exactly the refusal this unit exists to end; so a failed
 * write refuses by name and the endpoint does not run with it. A stored value
 * that does not read as what it should be is refused the same way and never
 * overwritten: only the owner replaces an identity. */

#ifndef CHORUS_IDENTITY_H
#define CHORUS_IDENTITY_H

#include <stddef.h>
#include <stdint.h>

#include "chorus/noise.h"
#include "chorus/store.h"

/* The store's keys. */
#define CHORUS_IDENTITY_KEY_ID "id"
#define CHORUS_IDENTITY_KEY_NOISE "noise_key"
#define CHORUS_IDENTITY_KEY_PINS "server_pins"
#define CHORUS_IDENTITY_KEY_SERVER "server_addr"

#define CHORUS_IDENTITY_ID_PREFIX "chorus-"
/* Random bytes in an id, and the id's length in characters without its NUL. */
#define CHORUS_IDENTITY_ID_RANDOM_BYTES 6u
#define CHORUS_IDENTITY_ID_LEN (7u + 2u * CHORUS_IDENTITY_ID_RANDOM_BYTES)

typedef enum {
    CHORUS_IDENTITY_OK = 0,
    /* The random source gave nothing. */
    CHORUS_IDENTITY_NO_RANDOM,
    /* The store could not be read (not "holds nothing": that makes one). */
    CHORUS_IDENTITY_UNREADABLE,
    /* The store holds something under the key that is not what belongs there. */
    CHORUS_IDENTITY_NOT_AN_IDENTITY,
    /* A new value was made and the store did not keep it. */
    CHORUS_IDENTITY_NOT_SAVED
} chorus_identity_status_t;

const char *chorus_identity_status_name(chorus_identity_status_t status);

/* The endpoint id: read it, or make one and keep it. `id` takes
 * CHORUS_IDENTITY_ID_LEN characters and a NUL. `created`, when not NULL, says
 * whether this call made it. `random` NULL uses chorus_noise_system_random. */
chorus_identity_status_t chorus_identity_id(const chorus_store_t *store,
                                            chorus_noise_random_fn random, void *random_ctx,
                                            char *id, size_t id_capacity, int *created);

/* The long-term X25519 secret: read it, or make one and keep it. The secret is
 * never printed by this unit. */
chorus_identity_status_t chorus_identity_secret(const chorus_store_t *store,
                                                chorus_noise_random_fn random, void *random_ctx,
                                                uint8_t secret[CHORUS_NOISE_KEY_LEN], int *created);

/* Whether `id` has the form this unit makes. */
int chorus_identity_id_is_wellformed(const char *id);

/* --- the pinned servers ------------------------------------------------------ */

/* One pin per server id this endpoint has met. */
#define CHORUS_PINS_MAX 8
#define CHORUS_PIN_ID_MAX 256
/* The longest text a full pin set renders to: a header line, and per pin
 * `pinned `, 64 hex digits, a space, an id of up to 255 bytes and a newline.
 * A store value is bounded lower (CHORUS_STORE_MAX_VALUE), so on the board a
 * set that renders past that bound is not saved and the pin is refused. */
#define CHORUS_PINS_TEXT_MAX 2816

typedef struct {
    int used;
    char id[CHORUS_PIN_ID_MAX];
    uint8_t key[CHORUS_NOISE_KEY_LEN];
} chorus_pin_t;

typedef struct {
    chorus_pin_t pins[CHORUS_PINS_MAX];
} chorus_pins_t;

/* Read the adoption store's text form into `pins` (emptied first). Returns 0,
 * or -1 with the 1-based number of the offending line in `*bad_line`: a line
 * that is not `pinned <64 hex> <id>`, an id met twice, or more pins than fit. */
int chorus_pins_parse(const char *text, size_t length, chorus_pins_t *pins, int *bad_line);

/* Write that form. Returns the text's length (NUL-terminated in `out`), or -1
 * when it does not fit `capacity`. */
long chorus_pins_render(const chorus_pins_t *pins, char *out, size_t capacity);

typedef enum {
    CHORUS_PIN_ADOPTED,
    CHORUS_PIN_KNOWN,
    CHORUS_PIN_CHANGED,
    CHORUS_PIN_UNSAVED
} chorus_pin_verdict_t;

/* Keeps the whole set: 0 when it was kept. */
typedef int (*chorus_pins_save_fn)(void *ctx, const chorus_pins_t *pins);

/* Check a server's key against its pin. A server never met is pinned, and the
 * pin is saved before the verdict is ADOPTED: a pin that could not be kept is
 * not an adoption (the next start would pin whatever key came first), so the
 * verdict is UNSAVED and the set is left as it was. A key other than the
 * pinned one is CHANGED, `pinned` holds the pinned key, and NOTHING is
 * written: only the owner changes a pin. */
chorus_pin_verdict_t chorus_pins_check(chorus_pins_t *pins, const char *id,
                                       const uint8_t key[CHORUS_NOISE_KEY_LEN],
                                       uint8_t pinned[CHORUS_NOISE_KEY_LEN],
                                       chorus_pins_save_fn save, void *save_ctx);

/* The pins through the store seam, read and written as text in the caller's
 * `scratch` (CHORUS_STORE_MAX_VALUE + 1 bytes holds any value the store
 * takes). Load: OK with an empty set when the store holds none; UNREADABLE
 * (a pin set that cannot be read is not an empty one: starting with no pins
 * would adopt whichever server answered first) or NOT_AN_IDENTITY otherwise.
 * Save: 0 when kept, -1 when the set does not fit a store value or the store
 * refused it; the stored set is then the old one. */
chorus_identity_status_t chorus_identity_pins_load(const chorus_store_t *store, chorus_pins_t *pins,
                                                   int *bad_line, char *scratch,
                                                   size_t scratch_capacity);
int chorus_identity_pins_save(const chorus_store_t *store, const chorus_pins_t *pins, char *scratch,
                              size_t scratch_capacity);

/* --- the last server that answered ------------------------------------------ */

/* The `host:port` of the last server a session shook hands with. Load returns
 * 1 with the address, 0 when there is none (or it cannot be read: a fallback
 * that is absent is not an error). Save writes only when the address differs
 * from the one held, so a board that always finds its server at the same
 * place never writes flash for it; 0 when held or kept, -1 when not kept. */
int chorus_identity_server_load(const chorus_store_t *store, char *address, size_t capacity);
int chorus_identity_server_save(const chorus_store_t *store, const char *address);

#endif /* CHORUS_IDENTITY_H */
