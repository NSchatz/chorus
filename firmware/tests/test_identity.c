/* The board's identity across boots (goal 14, chorus/identity.h), over the
 * fake store.
 *
 * What is graded: the id and the key are made once and are the same after a
 * "reboot" (a second run over the same fake); the id is random bytes and
 * nothing else; a value the store did not keep is refused rather than used; a
 * stored value that is not an identity is refused and never overwritten; the
 * server's pin survives a reboot and a changed server key is still refused
 * with the pin untouched (the rule goal 6's session-outage.sh grades end to
 * end against a real server); and the session itself, handed a store, takes
 * its id and key from it. */

#include "chorus/identity.h"
#include "chorus/session.h"
#include "fake_store.h"
#include "harness.h"

#include <inttypes.h>
#include <unistd.h>

/* A random source that is a counter: each byte is the next value, starting at
 * `next`. What the unit made is then known exactly. */
typedef struct {
    uint8_t next;
    size_t calls;
    int fails;
} counting_random_t;

static int counting_random(void *ctx, uint8_t *out, size_t len)
{
    counting_random_t *source = (counting_random_t *)ctx;
    source->calls++;
    if (source->fails) {
        return -1;
    }
    for (size_t i = 0; i < len; i++) {
        out[i] = source->next++;
    }
    return 0;
}

static fake_store_t fake;
static fake_store_t other;

static void the_id_is_made_once_and_survives_a_reboot(void)
{
    chorus_section("the endpoint id: made at first boot, the same at every boot after");
    fake_store_init(&fake);
    chorus_store_t store = fake_store_as_store(&fake);
    counting_random_t random = {0xA0, 0, 0};

    char id[CHORUS_IDENTITY_ID_LEN + 1];
    int created = 0;
    chorus_identity_status_t status =
        chorus_identity_id(&store, counting_random, &random, id, sizeof(id), &created);
    chorus_check(status == CHORUS_IDENTITY_OK && created == 1, "first boot makes an id: %s (%s)",
                 id, chorus_identity_status_name(status));
    chorus_check(strcmp(id, "chorus-a0a1a2a3a4a5") == 0,
                 "it is `chorus-` and the six random bytes as twelve lower-case hex digits, and "
                 "nothing else: %s",
                 id);
    chorus_check(chorus_identity_id_is_wellformed(id) && strlen(id) == CHORUS_IDENTITY_ID_LEN,
                 "%zu characters, well formed", strlen(id));
    chorus_check(fake_store_sets_kept(&fake, CHORUS_IDENTITY_KEY_ID) == 1 && random.calls == 1,
                 "kept in the store under `%s` with one write, from one draw of %u random bytes",
                 CHORUS_IDENTITY_KEY_ID, (unsigned)CHORUS_IDENTITY_ID_RANDOM_BYTES);
    size_t held_len = 0;
    const uint8_t *held = fake_store_peek(&fake, CHORUS_IDENTITY_KEY_ID, &held_len);
    chorus_check(held != NULL && held_len == CHORUS_IDENTITY_ID_LEN &&
                     memcmp(held, id, held_len) == 0,
                 "the stored value is the id's %zu characters", held_len);

    /* The reboot: the same medium, a random source that would give a
     * different id if it were asked. */
    counting_random_t later = {0x10, 0, 0};
    char again[CHORUS_IDENTITY_ID_LEN + 1];
    status = chorus_identity_id(&store, counting_random, &later, again, sizeof(again), &created);
    chorus_check(status == CHORUS_IDENTITY_OK && created == 0 && strcmp(again, id) == 0,
                 "after a reboot the id is the same one: %s", again);
    chorus_check(later.calls == 0 && fake_store_count(&fake, FAKE_STORE_SET, NULL) == 1,
                 "and nothing was drawn or written to read it");
}

static void two_fresh_stores_give_two_ids(void)
{
    chorus_section("two boards: two ids");
    fake_store_init(&fake);
    fake_store_init(&other);
    chorus_store_t a = fake_store_as_store(&fake);
    chorus_store_t b = fake_store_as_store(&other);
    char id_a[CHORUS_IDENTITY_ID_LEN + 1];
    char id_b[CHORUS_IDENTITY_ID_LEN + 1];
    /* NULL is the system random source: what the board uses. */
    chorus_check(chorus_noise_setup() == CHORUS_NOISE_OK, "the crypto library starts");
    chorus_identity_status_t sa = chorus_identity_id(&a, NULL, NULL, id_a, sizeof(id_a), NULL);
    chorus_identity_status_t sb = chorus_identity_id(&b, NULL, NULL, id_b, sizeof(id_b), NULL);
    chorus_check(sa == CHORUS_IDENTITY_OK && sb == CHORUS_IDENTITY_OK &&
                     chorus_identity_id_is_wellformed(id_a) &&
                     chorus_identity_id_is_wellformed(id_b),
                 "each fresh store makes a well-formed id from the system random source");
    chorus_check(strcmp(id_a, id_b) != 0, "and they differ: %s, %s", id_a, id_b);

    uint8_t key_a[CHORUS_NOISE_KEY_LEN];
    uint8_t key_b[CHORUS_NOISE_KEY_LEN];
    sa = chorus_identity_secret(&a, NULL, NULL, key_a, NULL);
    sb = chorus_identity_secret(&b, NULL, NULL, key_b, NULL);
    chorus_check(sa == CHORUS_IDENTITY_OK && sb == CHORUS_IDENTITY_OK &&
                     memcmp(key_a, key_b, sizeof(key_a)) != 0,
                 "and so do their keys");
}

static void the_key_is_made_once_and_survives_a_reboot(void)
{
    chorus_section("the Noise static key: made at first boot, the same at every boot after");
    fake_store_init(&fake);
    chorus_store_t store = fake_store_as_store(&fake);
    counting_random_t random = {0x01, 0, 0};
    uint8_t secret[CHORUS_NOISE_KEY_LEN];
    int created = 0;
    chorus_identity_status_t status =
        chorus_identity_secret(&store, counting_random, &random, secret, &created);
    int as_drawn = 1;
    for (size_t i = 0; i < sizeof(secret); i++) {
        as_drawn = as_drawn && secret[i] == (uint8_t)(0x01 + i);
    }
    chorus_check(status == CHORUS_IDENTITY_OK && created == 1 && as_drawn,
                 "first boot makes a %u-byte key from the random source",
                 (unsigned)CHORUS_NOISE_KEY_LEN);
    size_t held_len = 0;
    const uint8_t *held = fake_store_peek(&fake, CHORUS_IDENTITY_KEY_NOISE, &held_len);
    chorus_check(held != NULL && held_len == CHORUS_NOISE_KEY_LEN &&
                     memcmp(held, secret, held_len) == 0 &&
                     fake_store_sets_kept(&fake, CHORUS_IDENTITY_KEY_NOISE) == 1,
                 "kept under `%s`, 32 bytes, one write", CHORUS_IDENTITY_KEY_NOISE);

    counting_random_t later = {0x80, 0, 0};
    uint8_t again[CHORUS_NOISE_KEY_LEN];
    status = chorus_identity_secret(&store, counting_random, &later, again, &created);
    chorus_check(status == CHORUS_IDENTITY_OK && created == 0 &&
                     memcmp(again, secret, sizeof(secret)) == 0 && later.calls == 0,
                 "after a reboot the key is the same key, and no random byte was drawn");
    chorus_noise_keypair_t first;
    chorus_noise_keypair_t second;
    char fp_first[CHORUS_NOISE_FINGERPRINT_LEN];
    char fp_second[CHORUS_NOISE_FINGERPRINT_LEN];
    chorus_check(chorus_noise_keypair_from_secret(secret, &first) == CHORUS_NOISE_OK &&
                     chorus_noise_keypair_from_secret(again, &second) == CHORUS_NOISE_OK,
                 "both boots' secrets make a key pair");
    chorus_noise_fingerprint(first.public_key, fp_first);
    chorus_noise_fingerprint(second.public_key, fp_second);
    chorus_check(strcmp(fp_first, fp_second) == 0,
                 "so the server sees the fingerprint it pinned: %s", fp_second);
}

static void a_failed_write_refuses_rather_than_running_unsaved(void)
{
    chorus_section("a value the store did not keep is not used");
    fake_store_init(&fake);
    chorus_store_t store = fake_store_as_store(&fake);
    counting_random_t random = {0x30, 0, 0};

    uint8_t secret[CHORUS_NOISE_KEY_LEN];
    memset(secret, 0, sizeof(secret));
    int created = 1;
    fake_store_fail_next_set(&fake);
    chorus_identity_status_t status =
        chorus_identity_secret(&store, counting_random, &random, secret, &created);
    int untouched = 1;
    for (size_t i = 0; i < sizeof(secret); i++) {
        untouched = untouched && secret[i] == 0;
    }
    chorus_check(status == CHORUS_IDENTITY_NOT_SAVED && created == 0,
                 "a key whose write the medium refused is refused: %s",
                 chorus_identity_status_name(status));
    chorus_check(untouched, "and the unsaved key was not handed to the caller");
    size_t held_len = 0;
    chorus_check(fake_store_peek(&fake, CHORUS_IDENTITY_KEY_NOISE, &held_len) == NULL,
                 "the store holds no key");

    char id[CHORUS_IDENTITY_ID_LEN + 1];
    fake_store_fail_next_set(&fake);
    status = chorus_identity_id(&store, counting_random, &random, id, sizeof(id), &created);
    chorus_check(status == CHORUS_IDENTITY_NOT_SAVED && id[0] == '\0' && created == 0,
                 "an id whose write the medium refused is refused, and none is handed out: %s",
                 chorus_identity_status_name(status));

    /* The medium recovers: the next boot makes an identity and keeps it. */
    status = chorus_identity_secret(&store, counting_random, &random, secret, &created);
    chorus_identity_status_t id_status =
        chorus_identity_id(&store, counting_random, &random, id, sizeof(id), NULL);
    chorus_check(status == CHORUS_IDENTITY_OK && created == 1 && id_status == CHORUS_IDENTITY_OK &&
                     fake_store_sets_kept(&fake, CHORUS_IDENTITY_KEY_NOISE) == 1 &&
                     fake_store_sets_kept(&fake, CHORUS_IDENTITY_KEY_ID) == 1,
                 "once the medium takes a write, the next boot makes and keeps one: %s", id);

    counting_random_t dry = {0, 0, 1};
    fake_store_init(&fake);
    status = chorus_identity_secret(&store, counting_random, &dry, secret, NULL);
    id_status = chorus_identity_id(&store, counting_random, &dry, id, sizeof(id), NULL);
    chorus_check(status == CHORUS_IDENTITY_NO_RANDOM && id_status == CHORUS_IDENTITY_NO_RANDOM &&
                     fake_store_count(&fake, FAKE_STORE_SET, NULL) == 0,
                 "a random source that gives nothing makes nothing and writes nothing: %s",
                 chorus_identity_status_name(status));
}

static void what_is_stored_is_never_replaced(void)
{
    chorus_section("an unreadable store, and a stored value that is not an identity");
    fake_store_init(&fake);
    chorus_store_t store = fake_store_as_store(&fake);
    counting_random_t random = {0x50, 0, 0};
    char id[CHORUS_IDENTITY_ID_LEN + 1];
    uint8_t secret[CHORUS_NOISE_KEY_LEN];

    chorus_identity_status_t made =
        chorus_identity_id(&store, counting_random, &random, id, sizeof(id), NULL);
    chorus_identity_status_t made_key =
        chorus_identity_secret(&store, counting_random, &random, secret, NULL);
    chorus_check(made == CHORUS_IDENTITY_OK && made_key == CHORUS_IDENTITY_OK,
                 "a board with an identity: %s", id);

    /* The medium stops answering. Absent and unreadable are different: a new
     * identity made over a store that only failed to answer would turn one
     * speaker into two. */
    fake.gets_fail = 1;
    size_t sets_before = fake_store_count(&fake, FAKE_STORE_SET, NULL);
    char unread[CHORUS_IDENTITY_ID_LEN + 1];
    chorus_identity_status_t status =
        chorus_identity_id(&store, counting_random, &random, unread, sizeof(unread), NULL);
    chorus_identity_status_t key_status =
        chorus_identity_secret(&store, counting_random, &random, secret, NULL);
    chorus_check(status == CHORUS_IDENTITY_UNREADABLE && key_status == CHORUS_IDENTITY_UNREADABLE &&
                     fake_store_count(&fake, FAKE_STORE_SET, NULL) == sets_before,
                 "a store that cannot be read is refused (%s) and nothing is written over it",
                 chorus_identity_status_name(status));
    fake.gets_fail = 0;

    static const char *const NOT_IDS[] = {"chorus-A0A1A2A3A4A5", "speaker-a0a1a2a3a4a5",
                                          "chorus-a0a1a2a3a4", "chorus-a0a1a2a3a4a5a6",
                                          "chorus-a0a1a2a3a4zz"};
    for (size_t i = 0; i < sizeof(NOT_IDS) / sizeof(NOT_IDS[0]); i++) {
        fake_store_init(&fake);
        (void)chorus_store_set(&store, CHORUS_IDENTITY_KEY_ID, NOT_IDS[i], strlen(NOT_IDS[i]));
        status = chorus_identity_id(&store, counting_random, &random, unread, sizeof(unread), NULL);
        size_t held_len = 0;
        const uint8_t *held = fake_store_peek(&fake, CHORUS_IDENTITY_KEY_ID, &held_len);
        chorus_check(status == CHORUS_IDENTITY_NOT_AN_IDENTITY && unread[0] == '\0' &&
                         held_len == strlen(NOT_IDS[i]) && memcmp(held, NOT_IDS[i], held_len) == 0,
                     "a stored `%s` is not an id: refused (%s), handed out as nothing, left as "
                     "it was",
                     NOT_IDS[i], chorus_identity_status_name(status));
    }

    static const size_t NOT_KEY_LENGTHS[] = {0, 31, 33, 64};
    for (size_t i = 0; i < sizeof(NOT_KEY_LENGTHS) / sizeof(NOT_KEY_LENGTHS[0]); i++) {
        uint8_t junk[64];
        memset(junk, 0x5A, sizeof(junk));
        fake_store_init(&fake);
        (void)chorus_store_set(&store, CHORUS_IDENTITY_KEY_NOISE, junk, NOT_KEY_LENGTHS[i]);
        key_status = chorus_identity_secret(&store, counting_random, &random, secret, NULL);
        chorus_check(key_status == CHORUS_IDENTITY_NOT_AN_IDENTITY &&
                         fake_store_sets_kept(&fake, CHORUS_IDENTITY_KEY_NOISE) == 1,
                     "a stored key of %zu bytes is not a key: refused (%s) and not overwritten",
                     NOT_KEY_LENGTHS[i], chorus_identity_status_name(key_status));
    }
}

/* --- the pinned servers ------------------------------------------------------ */

static char scratch[CHORUS_STORE_MAX_VALUE + 1];

static int save_to_store(void *ctx, const chorus_pins_t *pins)
{
    return chorus_identity_pins_save((const chorus_store_t *)ctx, pins, scratch, sizeof(scratch));
}

static void the_pin_survives_and_a_changed_server_key_is_still_refused(void)
{
    chorus_section("the server's pin: kept across a reboot, never moved by a changed key");
    fake_store_init(&fake);
    chorus_store_t store = fake_store_as_store(&fake);
    static chorus_pins_t pins;
    uint8_t server_key[CHORUS_NOISE_KEY_LEN];
    uint8_t other_key[CHORUS_NOISE_KEY_LEN];
    uint8_t pinned[CHORUS_NOISE_KEY_LEN];
    memset(server_key, 0x41, sizeof(server_key));
    memset(other_key, 0x42, sizeof(other_key));

    int bad_line = -1;
    chorus_identity_status_t loaded =
        chorus_identity_pins_load(&store, &pins, &bad_line, scratch, sizeof(scratch));
    chorus_check(loaded == CHORUS_IDENTITY_OK && !pins.pins[0].used,
                 "a store with no pins is an endpoint that has met no server");

    chorus_pin_verdict_t verdict =
        chorus_pins_check(&pins, "chorus-server", server_key, pinned, save_to_store, &store);
    chorus_check(verdict == CHORUS_PIN_ADOPTED &&
                     fake_store_sets_kept(&fake, CHORUS_IDENTITY_KEY_PINS) == 1,
                 "the first server met is pinned, and the pin is in the store before the verdict");
    size_t text_len = 0;
    const uint8_t *text = fake_store_peek(&fake, CHORUS_IDENTITY_KEY_PINS, &text_len);
    static const char WANT[] =
        "# chorus adopted peers: <pinned|removed> <public key hex> <id>\n"
        "pinned 4141414141414141414141414141414141414141414141414141414141414141 "
        "chorus-server\n";
    chorus_check(text != NULL && text_len == sizeof(WANT) - 1 && memcmp(text, WANT, text_len) == 0,
                 "in the adoption store's text form, the one the key and pin files use (%zu bytes)",
                 text_len);
    static uint8_t first_text[CHORUS_STORE_MAX_VALUE];
    memcpy(first_text, text, text_len);

    /* The reboot: nothing in RAM survives; the pins come from the store. */
    static chorus_pins_t rebooted;
    loaded = chorus_identity_pins_load(&store, &rebooted, &bad_line, scratch, sizeof(scratch));
    verdict =
        chorus_pins_check(&rebooted, "chorus-server", server_key, pinned, save_to_store, &store);
    chorus_check(loaded == CHORUS_IDENTITY_OK && verdict == CHORUS_PIN_KNOWN &&
                     fake_store_count(&fake, FAKE_STORE_SET, CHORUS_IDENTITY_KEY_PINS) == 1,
                 "after a reboot the same server with the same key is known, and nothing is "
                 "written");

    memset(pinned, 0, sizeof(pinned));
    verdict =
        chorus_pins_check(&rebooted, "chorus-server", other_key, pinned, save_to_store, &store);
    chorus_check(verdict == CHORUS_PIN_CHANGED && memcmp(pinned, server_key, sizeof(pinned)) == 0,
                 "the same server id with ANOTHER key is refused as changed, and the pinned key "
                 "is the one named");
    size_t after_len = 0;
    const uint8_t *after = fake_store_peek(&fake, CHORUS_IDENTITY_KEY_PINS, &after_len);
    chorus_check(fake_store_count(&fake, FAKE_STORE_SET, CHORUS_IDENTITY_KEY_PINS) == 1 &&
                     after_len == text_len && memcmp(after, first_text, after_len) == 0,
                 "and the pin did not move: the store's bytes are unchanged, no write was tried");
    verdict =
        chorus_pins_check(&rebooted, "chorus-server", server_key, pinned, save_to_store, &store);
    chorus_check(verdict == CHORUS_PIN_KNOWN, "the pinned key is still the one that is known");

    /* A pin the store did not keep is not an adoption. */
    fake_store_fail_next_set(&fake);
    verdict =
        chorus_pins_check(&rebooted, "spare-server", other_key, pinned, save_to_store, &store);
    chorus_check(verdict == CHORUS_PIN_UNSAVED && !rebooted.pins[1].used,
                 "a second server whose pin the medium refused is not adopted, and is not held "
                 "in RAM either");
    verdict =
        chorus_pins_check(&rebooted, "spare-server", other_key, pinned, save_to_store, &store);
    static chorus_pins_t third_boot;
    loaded = chorus_identity_pins_load(&store, &third_boot, &bad_line, scratch, sizeof(scratch));
    chorus_check(verdict == CHORUS_PIN_ADOPTED && loaded == CHORUS_IDENTITY_OK &&
                     third_boot.pins[0].used && third_boot.pins[1].used &&
                     strcmp(third_boot.pins[1].id, "spare-server") == 0 &&
                     memcmp(third_boot.pins[1].key, other_key, sizeof(other_key)) == 0,
                 "once the medium takes the write it is adopted, and both pins read back");
}

static void the_pin_set_has_bounds_and_a_form(void)
{
    chorus_section("the pin set: its bounds and its text form");
    fake_store_init(&fake);
    chorus_store_t store = fake_store_as_store(&fake);
    static chorus_pins_t pins;
    memset(&pins, 0, sizeof(pins));
    uint8_t key[CHORUS_NOISE_KEY_LEN];
    uint8_t pinned[CHORUS_NOISE_KEY_LEN];
    int all_adopted = 1;
    for (int i = 0; i < CHORUS_PINS_MAX; i++) {
        char id[32];
        snprintf(id, sizeof(id), "server-%d", i);
        memset(key, 0x60 + i, sizeof(key));
        all_adopted = all_adopted && chorus_pins_check(&pins, id, key, pinned, save_to_store,
                                                       &store) == CHORUS_PIN_ADOPTED;
    }
    size_t text_len = 0;
    (void)fake_store_peek(&fake, CHORUS_IDENTITY_KEY_PINS, &text_len);
    chorus_check(all_adopted && text_len <= CHORUS_STORE_MAX_VALUE,
                 "%d servers with ids of this length fit one store value: %zu of %d bytes",
                 CHORUS_PINS_MAX, text_len, CHORUS_STORE_MAX_VALUE);
    size_t sets_before = fake_store_count(&fake, FAKE_STORE_SET, NULL);
    chorus_check(chorus_pins_check(&pins, "one-too-many", key, pinned, save_to_store, &store) ==
                         CHORUS_PIN_UNSAVED &&
                     fake_store_count(&fake, FAKE_STORE_SET, NULL) == sets_before,
                 "a server past the %d the set holds is not adopted, and none is evicted for it",
                 CHORUS_PINS_MAX);

    /* Ids so long that the set renders past a store value: refused, and the
     * stored set stays the old one. */
    fake_store_init(&fake);
    memset(&pins, 0, sizeof(pins));
    char long_id[CHORUS_PIN_ID_MAX];
    chorus_pin_verdict_t last = CHORUS_PIN_ADOPTED;
    int adopted = 0;
    for (int i = 0; i < CHORUS_PINS_MAX && last == CHORUS_PIN_ADOPTED; i++) {
        memset(long_id, 'a' + i, sizeof(long_id) - 1);
        long_id[sizeof(long_id) - 1] = '\0';
        last = chorus_pins_check(&pins, long_id, key, pinned, save_to_store, &store);
        adopted += (last == CHORUS_PIN_ADOPTED);
    }
    static chorus_pins_t read_back;
    int bad_line = 0;
    chorus_identity_status_t loaded =
        chorus_identity_pins_load(&store, &read_back, &bad_line, scratch, sizeof(scratch));
    int held = 0;
    for (int i = 0; i < CHORUS_PINS_MAX; i++) {
        held += read_back.pins[i].used;
    }
    chorus_check(last == CHORUS_PIN_UNSAVED && adopted > 0 && adopted < CHORUS_PINS_MAX &&
                     loaded == CHORUS_IDENTITY_OK && held == adopted,
                 "with 255-byte ids the %dth server does not fit a store value: not adopted, and "
                 "the %d before it read back whole",
                 adopted + 1, held);

    static const struct {
        const char *what;
        const char *text;
        int line;
    } BAD[] = {
        {"a line that is not a pin", "# header\nhello\n", 2},
        {"a key that is not hex",
         "pinned zz41414141414141414141414141414141414141414141414141414141414141 s\n", 1},
        {"a key that is short", "pinned 4141 server\n", 1},
        {"a pin with no id",
         "pinned 4141414141414141414141414141414141414141414141414141414141414141 \n", 1},
        {"a server id met twice",
         "pinned 4141414141414141414141414141414141414141414141414141414141414141 s\n"
         "pinned 4242424242424242424242424242424242424242424242424242424242424242 s\n",
         2},
        {"a removed entry, which only the server's store has",
         "removed 4141414141414141414141414141414141414141414141414141414141414141 s\n", 1},
    };
    for (size_t i = 0; i < sizeof(BAD) / sizeof(BAD[0]); i++) {
        bad_line = 0;
        int parsed = chorus_pins_parse(BAD[i].text, strlen(BAD[i].text), &read_back, &bad_line);
        chorus_check(parsed == -1 && bad_line == BAD[i].line && !read_back.pins[0].used,
                     "%s is refused at line %d, and no pin is taken from the text", BAD[i].what,
                     bad_line);
    }

    fake_store_init(&fake);
    (void)chorus_store_set(&store, CHORUS_IDENTITY_KEY_PINS, "garbage\n", 8);
    loaded = chorus_identity_pins_load(&store, &read_back, &bad_line, scratch, sizeof(scratch));
    chorus_check(loaded == CHORUS_IDENTITY_NOT_AN_IDENTITY && bad_line == 1,
                 "stored pins that do not read are refused (%s), not treated as no pins",
                 chorus_identity_status_name(loaded));
    fake.gets_fail = 1;
    loaded = chorus_identity_pins_load(&store, &read_back, &bad_line, scratch, sizeof(scratch));
    chorus_check(loaded == CHORUS_IDENTITY_UNREADABLE,
                 "and pins that cannot be read are refused (%s): an endpoint that forgot its pins "
                 "would adopt whichever server answered first",
                 chorus_identity_status_name(loaded));
    fake.gets_fail = 0;

    memset(&pins, 0, sizeof(pins));
    chorus_check(chorus_pins_check(&pins, "two\nlines", key, pinned, NULL, NULL) ==
                         CHORUS_PIN_UNSAVED &&
                     chorus_pins_check(&pins, "", key, pinned, NULL, NULL) == CHORUS_PIN_UNSAVED,
                 "a server id that could not be written as one line of the form is not pinned");
}

static void the_last_server_is_kept_and_rewritten_only_when_it_moves(void)
{
    chorus_section("the last server that answered");
    fake_store_init(&fake);
    chorus_store_t store = fake_store_as_store(&fake);
    char address[64];
    chorus_check(chorus_identity_server_load(&store, address, sizeof(address)) == 0 &&
                     address[0] == '\0',
                 "a store that has met no server holds no address");
    chorus_check(chorus_identity_server_save(&store, "192.0.2.40:4010") == 0 &&
                     chorus_identity_server_load(&store, address, sizeof(address)) == 1 &&
                     strcmp(address, "192.0.2.40:4010") == 0,
                 "an address saved is the address read back: %s", address);
    chorus_check(chorus_identity_server_save(&store, "192.0.2.40:4010") == 0 &&
                     fake_store_count(&fake, FAKE_STORE_SET, CHORUS_IDENTITY_KEY_SERVER) == 1,
                 "saving the same address again writes nothing (no flash write per boot)");
    chorus_check(chorus_identity_server_save(&store, "192.0.2.41:4110") == 0 &&
                     fake_store_count(&fake, FAKE_STORE_SET, CHORUS_IDENTITY_KEY_SERVER) == 2 &&
                     chorus_identity_server_load(&store, address, sizeof(address)) == 1 &&
                     strcmp(address, "192.0.2.41:4110") == 0,
                 "a server that moved is written once: %s", address);
    fake_store_fail_next_set(&fake);
    chorus_check(chorus_identity_server_save(&store, "192.0.2.42:4010") == -1 &&
                     chorus_identity_server_load(&store, address, sizeof(address)) == 1 &&
                     strcmp(address, "192.0.2.41:4110") == 0,
                 "a write the medium refused is said, and the old address is still the one held");
}

/* --- the session, handed a store --------------------------------------------- */

typedef struct {
    int asked;
} relocate_probe_t;

static int relocate_to_nowhere_else(void *ctx, char *address, size_t address_len)
{
    relocate_probe_t *probe = (relocate_probe_t *)ctx;
    probe->asked++;
    /* Another port nothing listens on: the session must take it up. */
    snprintf(address, address_len, "127.0.0.1:2");
    return 1;
}

static int log_holds(const char *path, const char *needle)
{
    static char text[65536];
    FILE *file = fopen(path, "r");
    if (file == NULL) {
        return 0;
    }
    size_t got = fread(text, 1, sizeof(text) - 1, file);
    fclose(file);
    text[got] = '\0';
    return strstr(text, needle) != NULL;
}

static void the_session_takes_its_identity_from_the_store(void)
{
    chorus_section("the session supervisor with a store: its id and key are the store's");
    fake_store_init(&fake);
    chorus_store_t store = fake_store_as_store(&fake);
    char log_path[512];
    snprintf(log_path, sizeof(log_path), "%s/chorus-test-identity-%ld.log",
             (getenv("TMPDIR") != NULL) ? getenv("TMPDIR") : "/tmp", (long)getpid());

    static chorus_session_config_t config;
    memset(&config, 0, sizeof(config));
    /* Loopback port 1: nothing listens there, so every attempt is refused at
     * once and the run is its identity and its reconnect loop, no server. */
    snprintf(config.server, sizeof(config.server), "127.0.0.1:1");
    config.first_backoff_ms = 20;
    config.max_backoff_ms = 40;
    config.run_seconds = 1;
    config.sync_interval_ms = 500;
    config.filter_window = 8;
    config.smoothing_alpha = 0.1;
    config.event_log_path = log_path;
    config.store = &store;
    relocate_probe_t probe = {0};
    config.relocate = relocate_to_nowhere_else;
    config.relocate_ctx = &probe;

    static chorus_session_result_t first;
    int ran = chorus_session_run(&config, &first);
    size_t id_len = 0;
    const uint8_t *id_held = fake_store_peek(&fake, CHORUS_IDENTITY_KEY_ID, &id_len);
    char id[CHORUS_IDENTITY_ID_LEN + 1] = "";
    if (id_held != NULL && id_len == CHORUS_IDENTITY_ID_LEN) {
        memcpy(id, id_held, id_len);
        id[id_len] = '\0';
    }
    chorus_check(ran == 0 && first.end == CHORUS_SESSION_RAN_ITS_TIME,
                 "a first run with an empty store runs (%s)", chorus_session_end_name(first.end));
    chorus_check(chorus_identity_id_is_wellformed(id) &&
                     fake_store_sets_kept(&fake, CHORUS_IDENTITY_KEY_NOISE) == 1,
                 "and left an id and a key in the store: %s", id);
    char line[128];
    snprintf(line, sizeof(line), "identity id=%s key=%s store=store", id, first.key_fingerprint);
    chorus_check(log_holds(log_path, line), "its start line names them: %s", line);
    chorus_check(probe.asked > 0 && log_holds(log_path, "event=server-relocated") &&
                     log_holds(log_path, "server=127.0.0.1:2"),
                 "after attempts that reached nothing it asked where the server went (%d times) "
                 "and took the answer up",
                 probe.asked);
    chorus_check(fake_store_count(&fake, FAKE_STORE_SET, CHORUS_IDENTITY_KEY_SERVER) == 0,
                 "an address no handshake completed with is not kept as the last good server");

    static chorus_session_result_t second;
    size_t sets_before = fake_store_count(&fake, FAKE_STORE_SET, NULL);
    ran = chorus_session_run(&config, &second);
    chorus_check(ran == 0 && strcmp(second.key_fingerprint, first.key_fingerprint) == 0 &&
                     log_holds(log_path, line),
                 "a second run over the same store (a reboot) presents the same id and the same "
                 "key: %s",
                 second.key_fingerprint);
    chorus_check(fake_store_count(&fake, FAKE_STORE_SET, NULL) == sets_before, "and wrote nothing");

    /* An id the configuration names wins over the store's (the host binary's
     * --endpoint-id), and the key is still the store's. */
    snprintf(config.endpoint_id, sizeof(config.endpoint_id), "kitchen");
    static chorus_session_result_t named;
    ran = chorus_session_run(&config, &named);
    chorus_check(ran == 0 && log_holds(log_path, "identity id=kitchen key=") &&
                     strcmp(named.key_fingerprint, first.key_fingerprint) == 0,
                 "a configured id is used as given, with the store's key");
    config.endpoint_id[0] = '\0';

    fake_store_init(&fake);
    fake_store_fail_next_set(&fake);
    static chorus_session_result_t refused;
    ran = chorus_session_run(&config, &refused);
    chorus_check(ran == -1 && refused.end == CHORUS_SESSION_IDENTITY_UNUSABLE &&
                     strstr(refused.detail, "not-saved") != NULL,
                 "a store that will not keep the identity ends the run by name rather than "
                 "running unsaved: %s",
                 refused.detail);

    /* Without a store the run is as it was before this goal. */
    config.store = NULL;
    static chorus_session_result_t plain;
    ran = chorus_session_run(&config, &plain);
    chorus_check(ran == 0 &&
                     log_holds(log_path, "identity id=" CHORUS_SESSION_DEFAULT_ID " key=") &&
                     log_holds(log_path, "store=this-run"),
                 "with no store and no paths: the default id and a key for this run only, as "
                 "before");
    remove(log_path);
}

int main(void)
{
    the_id_is_made_once_and_survives_a_reboot();
    two_fresh_stores_give_two_ids();
    the_key_is_made_once_and_survives_a_reboot();
    a_failed_write_refuses_rather_than_running_unsaved();
    what_is_stored_is_never_replaced();
    the_pin_survives_and_a_changed_server_key_is_still_refused();
    the_pin_set_has_bounds_and_a_form();
    the_last_server_is_kept_and_rewritten_only_when_it_moves();
    the_session_takes_its_identity_from_the_store();
    return chorus_test_report("test_identity");
}
