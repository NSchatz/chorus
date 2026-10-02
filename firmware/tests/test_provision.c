/* Goal 14: Wi-Fi provisioning on the host build, and the store it keeps a
 * network in.
 *
 * What is graded here is the DECIDING half (firmware/src/provision.c and
 * firmware/src/store.c), against a simulated radio that has a network in range
 * and an access point of its own (fake_radio) and a simulated store whose
 * event log holds keys and lengths and never a value (fake_store):
 *
 *   - the store seam's key rule and bounds;
 *   - the join form's parser, on good bodies and on hostile ones;
 *   - every state and every transition between them, read off the unit's own
 *     trace AND off what reached the radio and the store;
 *   - the setup secret: made once, kept, never the same on two speakers;
 *   - that no network name, no passphrase and no setup secret reaches a log
 *     line, a page, or the store's event log;
 *   - a full run: first boot unprovisioned, access point up, form posted, the
 *     network joined, the credentials stored, a "reboot" that joins with no
 *     access point, a network that went away bringing the access point back
 *     with the reason, and a reset that erases.
 *
 * What is NOT graded here, and is said rather than implied: the binding
 * (firmware/main/esp_provision.c) and the handshake of ESP-IDF's provisioning
 * manager. Its protocomm component does not build for the host (its
 * CMakeLists.txt returns early on the linux target), so that half runs on the
 * owner's bench, docs/bench-packet.md S9.
 *
 * The network below is a made-up one. Nothing here is anybody's. */

#include "chorus/provision.h"
#include "chorus/store.h"
#include "chorus/wifi.h"
#include "fake_radio.h"
#include "fake_store.h"
#include "harness.h"

#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#define NETWORK "example-network"
#define PASSPHRASE "example-passphrase-1"
#define FORM "ssid=example-network&secret=example-passphrase-1"

#define LOG_BYTES 16384

/* One speaker's worth of everything the unit is handed. The store is separate
 * from the rest so that a "reboot" keeps it and nothing else. */
typedef struct {
    fake_store_t *flash;
    chorus_store_t store;
    fake_radio_t antenna;
    chorus_radio_t radio;
    chorus_provision_t unit;
    /* Every line the unit handed its log sink, newline-separated. */
    char log[LOG_BYTES];
    size_t log_used;
    /* The random source: a seeded generator, or one of the two broken ones. */
    uint32_t seed;
    int random_refuses;
    int random_is_stuck;
} speaker_t;

static int speaker_ap_start(void *ctx, const char *name, const char *key)
{
    return fake_radio_ap_start(&((speaker_t *)ctx)->antenna, name, key);
}

static int speaker_ap_stop(void *ctx)
{
    return fake_radio_ap_stop(&((speaker_t *)ctx)->antenna);
}

static const char *speaker_join_reason(void *ctx)
{
    return fake_radio_join_reason(&((speaker_t *)ctx)->antenna);
}

static int speaker_random(void *ctx, uint8_t *out, size_t length)
{
    speaker_t *speaker = (speaker_t *)ctx;
    if (speaker->random_refuses) {
        return -1;
    }
    for (size_t i = 0; i < length; i++) {
        if (speaker->random_is_stuck) {
            /* Every byte is one the unit must throw away. */
            out[i] = 0xFF;
            continue;
        }
        /* xorshift32: deterministic, and nothing like a real source. The test
         * needs different speakers to differ, not secrecy. */
        speaker->seed ^= speaker->seed << 13;
        speaker->seed ^= speaker->seed >> 17;
        speaker->seed ^= speaker->seed << 5;
        out[i] = (uint8_t)(speaker->seed >> 11);
    }
    return 0;
}

static void speaker_log(void *ctx, const char *line)
{
    speaker_t *speaker = (speaker_t *)ctx;
    int n = snprintf(speaker->log + speaker->log_used, sizeof(speaker->log) - speaker->log_used,
                     "%s\n", line);
    if (n > 0 && (size_t)n < sizeof(speaker->log) - speaker->log_used) {
        speaker->log_used += (size_t)n;
    }
}

static chorus_wifi_config_t a_link(chorus_transport_t transport)
{
    chorus_wifi_config_t link;
    memset(&link, 0, sizeof(link));
    link.transport = transport;
    link.power_save = CHORUS_WIFI_PS_NONE;
    /* As the committed configuration has it: both declared `unknown`. */
    link.ssid_known = 0;
    link.secret_known = 0;
    snprintf(link.source, sizeof(link.source), "firmware/config/endpoint.conf");
    return link;
}

/* Power the speaker: a fresh radio and a fresh unit over `flash`, which is
 * whatever the last power-up left in it. */
static void power_up(speaker_t *speaker, fake_store_t *flash, chorus_transport_t transport,
                     uint32_t seed)
{
    memset(speaker, 0, sizeof(*speaker));
    speaker->flash = flash;
    speaker->store = fake_store_as_store(flash);
    fake_radio_init(&speaker->antenna);
    speaker->radio = fake_radio(&speaker->antenna);
    speaker->seed = seed;
    chorus_provision_platform_t platform;
    memset(&platform, 0, sizeof(platform));
    platform.ctx = speaker;
    platform.ap_start = speaker_ap_start;
    platform.ap_stop = speaker_ap_stop;
    platform.join_reason = speaker_join_reason;
    platform.random = speaker_random;
    platform.log = speaker_log;
    chorus_wifi_config_t link = a_link(transport);
    chorus_provision_init(&speaker->unit, &link, &speaker->store, &speaker->radio, &platform);
}

/* Whether the unit's trace, from `from` on, is exactly these states. */
static int trace_is(const chorus_provision_t *unit, size_t from, size_t count,
                    const chorus_provision_state_t *want)
{
    if (unit->trace_count != from + count) {
        return 0;
    }
    for (size_t i = 0; i < count; i++) {
        if (unit->trace[from + i] != want[i]) {
            return 0;
        }
    }
    return 1;
}

static void print_trace(const chorus_provision_t *unit)
{
    printf("     trace:");
    for (size_t i = 0; i < unit->trace_count; i++) {
        printf(" %s", chorus_provision_state_name(unit->trace[i]));
    }
    printf("\n");
}

#define TRACE(...)                                                                                 \
    (const chorus_provision_state_t[])                                                             \
    {                                                                                              \
        __VA_ARGS__                                                                                \
    }
#define COUNT(...) (sizeof(TRACE(__VA_ARGS__)) / sizeof(chorus_provision_state_t))
#define TRACE_IS(unit, from, ...) trace_is((unit), (from), COUNT(__VA_ARGS__), TRACE(__VA_ARGS__))

static int stored_equals(const fake_store_t *flash, const char *key, const char *text)
{
    size_t length = 0;
    const uint8_t *value = fake_store_peek(flash, key, &length);
    return value != NULL && length == strlen(text) && memcmp(value, text, length) == 0;
}

static chorus_provision_credentials_t a_network(const char *ssid, const char *secret)
{
    chorus_provision_credentials_t network;
    memset(&network, 0, sizeof(network));
    snprintf(network.ssid, sizeof(network.ssid), "%s", ssid);
    network.ssid_length = strlen(network.ssid);
    snprintf(network.secret, sizeof(network.secret), "%s", secret);
    network.secret_length = strlen(network.secret);
    return network;
}

/* --- the store seam ---------------------------------------------------------- */

static chorus_store_status_t lying_get(void *context, const char *key, void *out, size_t capacity,
                                       size_t *length)
{
    (void)context;
    (void)key;
    (void)out;
    /* A medium that claims more than it was given room for. */
    *length = capacity + 1;
    return CHORUS_STORE_OK;
}

static void the_store_seam_checks_keys_and_bounds_before_the_medium(void)
{
    chorus_section("the store: the key rule and the bounds are decided in front of the medium");
    static fake_store_t flash;
    fake_store_init(&flash);
    chorus_store_t store = fake_store_as_store(&flash);
    char out[CHORUS_STORE_MAX_VALUE + 8];
    size_t length = 99;

    chorus_check(chorus_store_get(&store, "id", out, sizeof(out), &length) ==
                         CHORUS_STORE_MISSING &&
                     length == 0,
                 "a key that holds nothing reads as missing, with length 0");
    chorus_check(chorus_store_set(&store, "id", "abc", 3) == CHORUS_STORE_OK &&
                     chorus_store_get(&store, "id", out, sizeof(out), &length) == CHORUS_STORE_OK &&
                     length == 3 && memcmp(out, "abc", 3) == 0,
                 "a value written is the value read, whole");

    static const char *const bad_keys[] = {"",           "sixteen_chars_xx", "Upper",   "with-dash",
                                           "with space", "dot.ted",          "slash/ed"};
    size_t before = flash.event_count;
    int all_refused = 1;
    for (size_t i = 0; i < sizeof(bad_keys) / sizeof(bad_keys[0]); i++) {
        all_refused &= chorus_store_get(&store, bad_keys[i], out, sizeof(out), &length) ==
                       CHORUS_STORE_BAD_KEY;
        all_refused &= chorus_store_set(&store, bad_keys[i], "x", 1) == CHORUS_STORE_BAD_KEY;
        all_refused &= chorus_store_erase(&store, bad_keys[i]) == CHORUS_STORE_BAD_KEY;
    }
    all_refused &= chorus_store_set(&store, NULL, "x", 1) == CHORUS_STORE_BAD_KEY;
    chorus_check(all_refused, "seven malformed keys and a NULL one are refused as bad-key");
    chorus_check(flash.event_count == before,
                 "and none of them reached the medium (%zu events before, %zu after)", before,
                 flash.event_count);
    chorus_check(chorus_store_set(&store, "fifteen_chars_x", "x", 1) == CHORUS_STORE_OK,
                 "a 15-character key, the longest NVS takes, is accepted");

    static char big[CHORUS_STORE_MAX_VALUE + 1];
    memset(big, 'v', sizeof(big));
    chorus_check(chorus_store_set(&store, "big", big, CHORUS_STORE_MAX_VALUE) == CHORUS_STORE_OK,
                 "a value of exactly %d bytes is stored", CHORUS_STORE_MAX_VALUE);
    before = flash.event_count;
    chorus_check(chorus_store_set(&store, "big", big, CHORUS_STORE_MAX_VALUE + 1) ==
                         CHORUS_STORE_TOO_LARGE &&
                     flash.event_count == before,
                 "one byte more is too-large and never reaches the medium");
    chorus_check(chorus_store_get(&store, "big", out, 16, &length) == CHORUS_STORE_TOO_LARGE &&
                     length == 0,
                 "a value that does not fit the caller's buffer is too-large, not cut");

    fake_store_fail_next_set(&flash);
    chorus_check(chorus_store_set(&store, "id", "zzzz", 4) == CHORUS_STORE_FAILED &&
                     stored_equals(&flash, "id", "abc"),
                 "a set the medium refuses leaves the old value readable");
    chorus_check(chorus_store_set(&store, "id", "zzzz", 4) == CHORUS_STORE_OK &&
                     stored_equals(&flash, "id", "zzzz"),
                 "and the next set goes through");

    chorus_check(chorus_store_erase(&store, "id") == CHORUS_STORE_OK &&
                     !fake_store_has(&flash, "id") &&
                     chorus_store_erase(&store, "id") == CHORUS_STORE_OK,
                 "erase removes the key, and erasing a missing key is ok");

    chorus_store_t liar = store;
    liar.get = lying_get;
    chorus_check(chorus_store_get(&liar, "id", out, 8, &length) == CHORUS_STORE_TOO_LARGE &&
                     length == 0,
                 "a medium that claims more bytes than the buffer holds is not believed");
    chorus_check(chorus_store_get(NULL, "id", out, sizeof(out), &length) == CHORUS_STORE_FAILED &&
                     chorus_store_set(NULL, "id", "x", 1) == CHORUS_STORE_FAILED &&
                     chorus_store_erase(NULL, "id") == CHORUS_STORE_FAILED,
                 "no store at all is failed, not a crash");

    static const chorus_store_status_t all[] = {CHORUS_STORE_OK, CHORUS_STORE_MISSING,
                                                CHORUS_STORE_TOO_LARGE, CHORUS_STORE_BAD_KEY,
                                                CHORUS_STORE_FAILED};
    int distinct = 1;
    for (size_t i = 0; i < 5; i++) {
        for (size_t j = i + 1; j < 5; j++) {
            distinct &=
                strcmp(chorus_store_status_name(all[i]), chorus_store_status_name(all[j])) != 0;
        }
    }
    chorus_check(distinct, "the five statuses have five names");

    char rendered[4096];
    fake_store_render(&flash, rendered, sizeof(rendered));
    chorus_check(strstr(rendered, "zzzz") == NULL && strstr(rendered, "abc") == NULL &&
                     strstr(rendered, "set id length=4 ok") != NULL,
                 "the fake's event log names keys and lengths and never a value");
}

/* --- the form ---------------------------------------------------------------- */

static void the_form_parser_reads_what_a_phone_sends(void)
{
    chorus_section("the join form: what a phone's browser posts is read");
    chorus_provision_credentials_t got;

    chorus_check(chorus_provision_parse_form(FORM, strlen(FORM), &got) ==
                         CHORUS_PROVISION_FORM_OK &&
                     strcmp(got.ssid, NETWORK) == 0 && strcmp(got.secret, PASSPHRASE) == 0 &&
                     got.ssid_length == strlen(NETWORK) && got.secret_length == strlen(PASSPHRASE),
                 "ssid then secret: both recovered with their lengths");

    const char *reversed = "secret=example-passphrase-1&ssid=example-network";
    chorus_check(chorus_provision_parse_form(reversed, strlen(reversed), &got) ==
                         CHORUS_PROVISION_FORM_OK &&
                     strcmp(got.ssid, NETWORK) == 0 && strcmp(got.secret, PASSPHRASE) == 0,
                 "secret then ssid: the order does not matter");

    const char *spaced = "ssid=example+network+2&secret=two+words+here";
    chorus_check(
        chorus_provision_parse_form(spaced, strlen(spaced), &got) == CHORUS_PROVISION_FORM_OK &&
            strcmp(got.ssid, "example network 2") == 0 && strcmp(got.secret, "two words here") == 0,
        "a plus sign is a space");

    const char *escaped = "ssid=caf%C3%A9%26co&secret=a%3Db%25c%2Bd%21%7e";
    chorus_check(chorus_provision_parse_form(escaped, strlen(escaped), &got) ==
                         CHORUS_PROVISION_FORM_OK &&
                     strcmp(got.ssid, "caf\xC3\xA9&co") == 0 && got.ssid_length == 8 &&
                     strcmp(got.secret, "a=b%c+d!~") == 0,
                 "percent escapes decode, in either case, including & = %% and + themselves");

    char body[CHORUS_PROVISION_FORM_MAX + 64];
    char longest_ssid[CHORUS_PROVISION_SSID_MAX + 1];
    memset(longest_ssid, 's', CHORUS_PROVISION_SSID_MAX);
    longest_ssid[CHORUS_PROVISION_SSID_MAX] = '\0';
    char hex_key[CHORUS_PROVISION_SECRET_MAX + 1];
    for (size_t i = 0; i < CHORUS_PROVISION_SECRET_MAX; i++) {
        hex_key[i] = "0123456789abcdefABCDEF"[i % 22];
    }
    hex_key[CHORUS_PROVISION_SECRET_MAX] = '\0';
    snprintf(body, sizeof(body), "ssid=%s&secret=%s", longest_ssid, hex_key);
    chorus_check(chorus_provision_parse_form(body, strlen(body), &got) ==
                         CHORUS_PROVISION_FORM_OK &&
                     got.ssid_length == 32 && got.secret_length == 64,
                 "a 32-byte name and a 64-digit hexadecimal key are the upper bounds, accepted");

    char passphrase63[64];
    memset(passphrase63, 'p', 63);
    passphrase63[63] = '\0';
    snprintf(body, sizeof(body), "ssid=n&secret=%s", passphrase63);
    chorus_check(chorus_provision_parse_form(body, strlen(body), &got) ==
                         CHORUS_PROVISION_FORM_OK &&
                     got.ssid_length == 1 && got.secret_length == 63,
                 "a 1-byte name and a 63-character passphrase are accepted");
    const char *shortest = "ssid=n&secret=12345678&";
    chorus_check(chorus_provision_parse_form(shortest, strlen(shortest), &got) ==
                         CHORUS_PROVISION_FORM_OK &&
                     got.secret_length == 8,
                 "an 8-character passphrase is accepted, and a trailing & is not a field");

    /* The body need not be terminated: exactly its bytes, on the heap, with
     * nothing readable after them. */
    size_t length = strlen(FORM);
    char *exact = malloc(length);
    if (exact != NULL) {
        memcpy(exact, FORM, length);
        chorus_check(chorus_provision_parse_form(exact, length, &got) == CHORUS_PROVISION_FORM_OK &&
                         strcmp(got.secret, PASSPHRASE) == 0,
                     "a body with no terminator after it parses from its length alone");
        free(exact);
    }
}

typedef struct {
    const char *what;
    const char *body;
    /* 0: strlen(body). Otherwise the length, for bodies with a zero byte. */
    size_t length;
    chorus_provision_form_status_t want;
} hostile_t;

static int is_zeroed(const chorus_provision_credentials_t *network)
{
    const unsigned char *bytes = (const unsigned char *)network;
    for (size_t i = 0; i < sizeof(*network); i++) {
        if (bytes[i] != 0) {
            return 0;
        }
    }
    return 1;
}

static void the_form_parser_refuses_hostile_input_by_name(void)
{
    chorus_section("the join form: hostile bodies are refused by name and leave nothing behind");
    static const hostile_t cases[] = {
        {"an empty body", "", 0, CHORUS_PROVISION_SSID_MISSING},
        {"only a name", "ssid=example-network", 0, CHORUS_PROVISION_SECRET_MISSING},
        {"only a passphrase", "secret=example-passphrase-1", 0, CHORUS_PROVISION_SSID_MISSING},
        {"an empty name", "ssid=&secret=example-passphrase-1", 0, CHORUS_PROVISION_SSID_EMPTY},
        {"a 33-byte name", "ssid=123456789012345678901234567890123&secret=example-passphrase-1", 0,
         CHORUS_PROVISION_SSID_TOO_LONG},
        {"a 7-character passphrase", "ssid=example-network&secret=1234567", 0,
         CHORUS_PROVISION_SECRET_TOO_SHORT},
        {"an empty passphrase (an open network)", "ssid=example-network&secret=", 0,
         CHORUS_PROVISION_SECRET_TOO_SHORT},
        {"a 65-character passphrase",
         "ssid=example-network&secret="
         "12345678901234567890123456789012345678901234567890123456789012345",
         0, CHORUS_PROVISION_SECRET_TOO_LONG},
        {"64 characters that are not hexadecimal",
         "ssid=example-network&secret="
         "g234567890123456789012345678901234567890123456789012345678901234",
         0, CHORUS_PROVISION_SECRET_NOT_HEX},
        {"a percent sign with letters after it", "ssid=ex%zzample&secret=example-passphrase-1", 0,
         CHORUS_PROVISION_FORM_BAD_ESCAPE},
        {"an escape cut short by the end", "ssid=example-network&secret=example-passphrase%4", 0,
         CHORUS_PROVISION_FORM_BAD_ESCAPE},
        {"a bare percent sign at the end", "ssid=example-network&secret=example-passphrase%", 0,
         CHORUS_PROVISION_FORM_BAD_ESCAPE},
        {"an escape cut short by the next field", "ssid=example%4&secret=example-passphrase-1", 0,
         CHORUS_PROVISION_FORM_BAD_ESCAPE},
        {"an escaped zero byte in the name", "ssid=exam%00ple&secret=example-passphrase-1", 0,
         CHORUS_PROVISION_FORM_EMBEDDED_NUL},
        {"an escaped zero byte in the passphrase", "ssid=example-network&secret=example%00-pass", 0,
         CHORUS_PROVISION_FORM_EMBEDDED_NUL},
        {"a raw zero byte in the body", "ssid=exam\0ple&secret=example-passphrase-1", 41,
         CHORUS_PROVISION_FORM_EMBEDDED_NUL},
        {"a field this form does not have", "ssid=example-network&secret=example-passphrase-1&x=1",
         0, CHORUS_PROVISION_FORM_UNKNOWN_FIELD},
        {"a field name in another case", "SSID=example-network&secret=example-passphrase-1", 0,
         CHORUS_PROVISION_FORM_UNKNOWN_FIELD},
        {"the name twice", "ssid=example-network&ssid=other&secret=example-passphrase-1", 0,
         CHORUS_PROVISION_FORM_DUPLICATE_FIELD},
        {"the passphrase twice", "ssid=example-network&secret=example-passphrase-1&secret=x", 0,
         CHORUS_PROVISION_FORM_DUPLICATE_FIELD},
        {"a pair with no equals sign", "ssid&secret=example-passphrase-1", 0,
         CHORUS_PROVISION_FORM_MALFORMED},
        {"a pair with no name", "=example-network&secret=example-passphrase-1", 0,
         CHORUS_PROVISION_FORM_MALFORMED},
        {"a control character in the passphrase", "ssid=example-network&secret=example%07pass", 0,
         CHORUS_PROVISION_SECRET_NOT_PRINTABLE},
        {"a byte above ASCII in the passphrase", "ssid=example-network&secret=example%C3%A9pass", 0,
         CHORUS_PROVISION_SECRET_NOT_PRINTABLE},
        {"a line break smuggled into the passphrase",
         "ssid=example-network&secret=example%0D%0Apass", 0, CHORUS_PROVISION_SECRET_NOT_PRINTABLE},
    };
    for (size_t i = 0; i < sizeof(cases) / sizeof(cases[0]); i++) {
        chorus_provision_credentials_t got;
        memset(&got, 0xAA, sizeof(got));
        size_t length = cases[i].length != 0 ? cases[i].length : strlen(cases[i].body);
        chorus_provision_form_status_t status =
            chorus_provision_parse_form(cases[i].body, length, &got);
        chorus_check(status == cases[i].want && is_zeroed(&got),
                     "%s is refused %s (got %s) and hands nothing on", cases[i].what,
                     chorus_provision_form_status_name(cases[i].want),
                     chorus_provision_form_status_name(status));
    }

    /* Length alone refuses a body that is too long: one byte over, and a
     * megabyte, which is never read past its bound either way. */
    chorus_provision_credentials_t got;
    size_t huge_length = 1024 * 1024;
    char *huge = malloc(huge_length);
    if (huge != NULL) {
        memset(huge, 'a', huge_length);
        memcpy(huge, "ssid=", 5);
        chorus_check(chorus_provision_parse_form(huge, CHORUS_PROVISION_FORM_MAX + 1, &got) ==
                             CHORUS_PROVISION_FORM_TOO_LONG &&
                         is_zeroed(&got),
                     "a body of %d bytes, one over the bound, is form-too-long",
                     CHORUS_PROVISION_FORM_MAX + 1);
        chorus_check(chorus_provision_parse_form(huge, huge_length, &got) ==
                             CHORUS_PROVISION_FORM_TOO_LONG &&
                         is_zeroed(&got),
                     "a megabyte of body is form-too-long");
        free(huge);
    }
    chorus_check(chorus_provision_parse_form(NULL, 0, &got) == CHORUS_PROVISION_SSID_MISSING,
                 "no body at all is ssid-missing, not a crash");

    /* Every prefix of a good body, and every single-byte change to it, either
     * parses to a network inside the bounds or is refused and zeroed. Each
     * candidate is copied to the heap at its exact length so a read past the
     * end is a fault a checker would see. */
    size_t length = strlen(FORM);
    size_t tried = 0;
    size_t accepted = 0;
    int sound = 1;
    for (size_t cut = 0; cut <= length; cut++) {
        char *copy = malloc(cut + 1);
        if (copy == NULL) {
            sound = 0;
            break;
        }
        memcpy(copy, FORM, cut);
        chorus_provision_form_status_t status = chorus_provision_parse_form(copy, cut, &got);
        tried++;
        if (status == CHORUS_PROVISION_FORM_OK) {
            accepted++;
            sound &=
                chorus_provision_check_credentials(got.ssid, got.ssid_length, got.secret,
                                                   got.secret_length) == CHORUS_PROVISION_FORM_OK;
        } else {
            sound &= is_zeroed(&got);
        }
        free(copy);
    }
    static const unsigned char changes[] = {0x00, '%', '&', '=', '+', 0x7F, 0xFF, ' '};
    for (size_t at = 0; at < length && sound; at++) {
        for (size_t c = 0; c < sizeof(changes); c++) {
            char *copy = malloc(length);
            if (copy == NULL) {
                sound = 0;
                break;
            }
            memcpy(copy, FORM, length);
            copy[at] = (char)changes[c];
            chorus_provision_form_status_t status = chorus_provision_parse_form(copy, length, &got);
            tried++;
            if (status == CHORUS_PROVISION_FORM_OK) {
                accepted++;
                sound &= strlen(got.ssid) == got.ssid_length &&
                         strlen(got.secret) == got.secret_length &&
                         chorus_provision_check_credentials(got.ssid, got.ssid_length, got.secret,
                                                            got.secret_length) ==
                             CHORUS_PROVISION_FORM_OK;
            } else {
                sound &= is_zeroed(&got);
            }
            free(copy);
        }
    }
    chorus_check(sound,
                 "%zu prefixes and single-byte changes of a good body: %zu parsed to a network "
                 "inside the bounds, the other %zu were refused and zeroed",
                 tried, accepted, tried - accepted);

    /* The bounds alone, as a network from Espressif's client or the store
     * meets them. */
    chorus_check(chorus_provision_check_credentials("a\0b", 3, PASSPHRASE, strlen(PASSPHRASE)) ==
                         CHORUS_PROVISION_FORM_EMBEDDED_NUL &&
                     chorus_provision_check_credentials(NULL, 0, PASSPHRASE, 20) ==
                         CHORUS_PROVISION_SSID_EMPTY &&
                     chorus_provision_check_credentials(NETWORK, strlen(NETWORK), NULL, 0) ==
                         CHORUS_PROVISION_SECRET_TOO_SHORT,
                 "the bounds check refuses a zero byte in a name, no name and no passphrase");

    int distinct = 1;
    for (int i = CHORUS_PROVISION_FORM_OK; i <= CHORUS_PROVISION_NOT_ACCEPTING; i++) {
        const char *name = chorus_provision_form_status_name((chorus_provision_form_status_t)i);
        distinct &= strcmp(name, "unknown") != 0;
        for (int j = i + 1; j <= CHORUS_PROVISION_NOT_ACCEPTING; j++) {
            distinct &=
                strcmp(name,
                       chorus_provision_form_status_name((chorus_provision_form_status_t)j)) != 0;
        }
    }
    chorus_check(distinct, "every refusal has its own name (%d of them)",
                 CHORUS_PROVISION_NOT_ACCEPTING + 1);
}

/* --- the states -------------------------------------------------------------- */

static void a_wired_profile_never_provisions(void)
{
    chorus_section("a wired profile never provisions");
    static fake_store_t flash;
    static speaker_t speaker;
    fake_store_init(&flash);
    power_up(&speaker, &flash, CHORUS_TRANSPORT_WIRED, 1);
    chorus_provision_state_t state = chorus_provision_boot(&speaker.unit, 0);
    chorus_check(state == CHORUS_PROVISION_WIRED &&
                     TRACE_IS(&speaker.unit, 0, CHORUS_PROVISION_WIRED),
                 "boot ends in %s", chorus_provision_state_name(state));
    chorus_check(flash.event_count == 0 && speaker.antenna.event_count == 0,
                 "the store saw %zu events and the radio %zu: nothing was touched",
                 flash.event_count, speaker.antenna.event_count);
    chorus_provision_form_status_t refusal = CHORUS_PROVISION_FORM_OK;
    state = chorus_provision_submit_form(&speaker.unit, FORM, strlen(FORM), 0, &refusal);
    chorus_check(state == CHORUS_PROVISION_WIRED && refusal == CHORUS_PROVISION_NOT_ACCEPTING &&
                     speaker.antenna.joins == 0 && flash.event_count == 0,
                 "a form handed to it is refused %s; no join, no store write",
                 chorus_provision_form_status_name(refusal));
    chorus_check(chorus_provision_tick(&speaker.unit, 1000000000u) == CHORUS_PROVISION_WIRED &&
                     chorus_provision_reset(&speaker.unit) != 0 && flash.event_count == 0,
                 "a tick does nothing and a reset is refused: there is nothing to erase");
}

static int secret_is_well_formed(const char *secret)
{
    if (strlen(secret) != CHORUS_PROVISION_SETUP_CHARS) {
        return 0;
    }
    for (const char *c = secret; *c != '\0'; c++) {
        if (strchr("abcdefghjkmnpqrstuvwxyz23456789", *c) == NULL) {
            return 0;
        }
    }
    return 1;
}

static void the_first_boot_makes_a_setup_secret_and_raises_the_access_point(void)
{
    chorus_section("first boot: unprovisioned, a setup secret made and kept, the access point up");
    static fake_store_t flash;
    static speaker_t speaker;
    fake_store_init(&flash);
    power_up(&speaker, &flash, CHORUS_TRANSPORT_WIRELESS, 0x1234567u);
    chorus_provision_state_t state = chorus_provision_boot(&speaker.unit, 0);
    print_trace(&speaker.unit);
    chorus_check(
        state == CHORUS_PROVISION_AP_UP &&
            TRACE_IS(&speaker.unit, 0, CHORUS_PROVISION_UNPROVISIONED, CHORUS_PROVISION_AP_UP),
        "unprovisioned -> ap-up");
    const char *secret = chorus_provision_setup_secret(&speaker.unit);
    const char *name = chorus_provision_ap_name(&speaker.unit);
    chorus_check(secret_is_well_formed(secret),
                 "the setup secret is %d characters of the 31-symbol alphabet (no 0, 1, i, l, o)",
                 CHORUS_PROVISION_SETUP_CHARS);
    chorus_check(
        strncmp(name, CHORUS_PROVISION_NAME_PREFIX, strlen(CHORUS_PROVISION_NAME_PREFIX)) == 0 &&
            strlen(name) ==
                strlen(CHORUS_PROVISION_NAME_PREFIX) + CHORUS_PROVISION_NAME_SUFFIX_CHARS &&
            strlen(name) <= CHORUS_PROVISION_NAME_MAX,
        "the access point is named %s (%zu bytes, inside 32)", name, strlen(name));
    chorus_check(strstr(name, secret) == NULL && strstr(secret, name + 13) == NULL,
                 "the broadcast name carries no part of the secret");
    chorus_check(speaker.antenna.ap_up && speaker.antenna.ap_starts == 1 &&
                     strcmp(speaker.antenna.ap_name, name) == 0 &&
                     strcmp(speaker.antenna.ap_key, secret) == 0,
                 "the radio raised that access point with the setup secret as its WPA2 key");
    chorus_check(fake_store_count(&flash, FAKE_STORE_SET, CHORUS_PROVISION_KEY_SETUP) == 1 &&
                     fake_store_has(&flash, CHORUS_PROVISION_KEY_SETUP),
                 "the secret was stored once, under %s", CHORUS_PROVISION_KEY_SETUP);
    chorus_check(speaker.antenna.joins == 0 && !fake_store_has(&flash, CHORUS_PROVISION_KEY_SSID),
                 "no join was tried and no network is stored");

    char first_name[CHORUS_PROVISION_NAME_MAX + 1];
    char first_secret[CHORUS_PROVISION_SETUP_CHARS + 1];
    snprintf(first_name, sizeof(first_name), "%s", name);
    snprintf(first_secret, sizeof(first_secret), "%s", secret);

    /* The same board, powered again: the same name and the same secret. */
    static speaker_t again;
    power_up(&again, &flash, CHORUS_TRANSPORT_WIRELESS, 0x7777777u);
    chorus_provision_boot(&again.unit, 0);
    chorus_check(strcmp(chorus_provision_ap_name(&again.unit), first_name) == 0 &&
                     strcmp(chorus_provision_setup_secret(&again.unit), first_secret) == 0 &&
                     fake_store_count(&flash, FAKE_STORE_SET, CHORUS_PROVISION_KEY_SETUP) == 1,
                 "a second boot loads the same name and secret and writes nothing");

    /* Another board: another secret and another name. */
    static fake_store_t other_flash;
    static speaker_t other;
    fake_store_init(&other_flash);
    power_up(&other, &other_flash, CHORUS_TRANSPORT_WIRELESS, 0x89ABCDEu);
    chorus_provision_boot(&other.unit, 0);
    chorus_check(strcmp(chorus_provision_setup_secret(&other.unit), first_secret) != 0 &&
                     strcmp(chorus_provision_ap_name(&other.unit), first_name) != 0,
                 "a second speaker has a different secret and a different name");

    /* A record this build cannot read is replaced, not used. */
    static fake_store_t odd_flash;
    static speaker_t odd;
    fake_store_init(&odd_flash);
    fake_store_preload(&odd_flash, CHORUS_PROVISION_KEY_SETUP, "ABCDEF:000000000000", 19);
    power_up(&odd, &odd_flash, CHORUS_TRANSPORT_WIRELESS, 0x2468ACEu);
    state = chorus_provision_boot(&odd.unit, 0);
    chorus_check(state == CHORUS_PROVISION_AP_UP &&
                     secret_is_well_formed(chorus_provision_setup_secret(&odd.unit)) &&
                     fake_store_count(&odd_flash, FAKE_STORE_SET, CHORUS_PROVISION_KEY_SETUP) == 1,
                 "a stored record outside the alphabet is replaced by a fresh one");
}

static void provisioning_refuses_by_name_when_it_cannot_run(void)
{
    chorus_section("what stops provisioning is named, and no access point is raised on it");
    static fake_store_t flash;
    static speaker_t speaker;

    fake_store_init(&flash);
    power_up(&speaker, &flash, CHORUS_TRANSPORT_WIRELESS, 5);
    fake_store_fail_next_set(&flash);
    chorus_provision_state_t state = chorus_provision_boot(&speaker.unit, 0);
    chorus_check(state == CHORUS_PROVISION_REFUSED &&
                     strcmp(chorus_provision_reason(&speaker.unit), "setup-secret-not-stored") ==
                         0 &&
                     speaker.antenna.ap_starts == 0,
                 "a setup secret the store would not keep: refused %s, no access point",
                 chorus_provision_reason(&speaker.unit));

    fake_store_init(&flash);
    power_up(&speaker, &flash, CHORUS_TRANSPORT_WIRELESS, 5);
    speaker.random_refuses = 1;
    state = chorus_provision_boot(&speaker.unit, 0);
    chorus_check(state == CHORUS_PROVISION_REFUSED &&
                     strcmp(chorus_provision_reason(&speaker.unit), "no-random-source") == 0 &&
                     speaker.antenna.ap_starts == 0 && flash.event_count == 1,
                 "a random source that refuses: refused %s, nothing stored",
                 chorus_provision_reason(&speaker.unit));

    fake_store_init(&flash);
    power_up(&speaker, &flash, CHORUS_TRANSPORT_WIRELESS, 5);
    speaker.random_is_stuck = 1;
    state = chorus_provision_boot(&speaker.unit, 0);
    chorus_check(state == CHORUS_PROVISION_REFUSED &&
                     strcmp(chorus_provision_reason(&speaker.unit), "no-random-source") == 0 &&
                     !fake_store_has(&flash, CHORUS_PROVISION_KEY_SETUP),
                 "a random source stuck at one byte: refused %s rather than a secret of one "
                 "letter",
                 chorus_provision_reason(&speaker.unit));

    fake_store_init(&flash);
    power_up(&speaker, &flash, CHORUS_TRANSPORT_WIRELESS, 5);
    flash.gets_fail = 1;
    state = chorus_provision_boot(&speaker.unit, 0);
    chorus_check(state == CHORUS_PROVISION_REFUSED &&
                     strcmp(chorus_provision_reason(&speaker.unit), "setup-secret-unreadable") ==
                         0 &&
                     fake_store_count(&flash, FAKE_STORE_SET, NULL) == 0,
                 "a store that cannot be read: refused %s, and nothing is written over it",
                 chorus_provision_reason(&speaker.unit));

    fake_store_init(&flash);
    power_up(&speaker, &flash, CHORUS_TRANSPORT_WIRELESS, 5);
    speaker.antenna.ap_start_refuses = 1;
    state = chorus_provision_boot(&speaker.unit, 0);
    chorus_check(state == CHORUS_PROVISION_REFUSED &&
                     strcmp(chorus_provision_reason(&speaker.unit), "ap-start-refused") == 0 &&
                     !speaker.antenna.ap_up,
                 "an access point the platform would not raise: refused %s",
                 chorus_provision_reason(&speaker.unit));
}

static void a_good_network_is_joined_then_stored_then_the_access_point_drops(void)
{
    chorus_section("ap-up -> credentials-received -> joining -> joined: joined first, then stored");
    static fake_store_t flash;
    static speaker_t speaker;
    fake_store_init(&flash);
    power_up(&speaker, &flash, CHORUS_TRANSPORT_WIRELESS, 11);
    fake_radio_set_network(&speaker.antenna, NETWORK, PASSPHRASE);
    chorus_provision_boot(&speaker.unit, 0);
    size_t before = speaker.unit.trace_count;

    chorus_provision_form_status_t refusal = CHORUS_PROVISION_NOT_ACCEPTING;
    chorus_provision_state_t state =
        chorus_provision_submit_form(&speaker.unit, FORM, strlen(FORM), 1000, &refusal);
    print_trace(&speaker.unit);
    chorus_check(state == CHORUS_PROVISION_JOINED && refusal == CHORUS_PROVISION_FORM_OK &&
                     TRACE_IS(&speaker.unit, before, CHORUS_PROVISION_CREDENTIALS_RECEIVED,
                              CHORUS_PROVISION_JOINING, CHORUS_PROVISION_JOINED),
                 "the form's network takes the unit through credentials-received and joining to "
                 "joined");
    chorus_check(speaker.antenna.joins == 1 && strcmp(speaker.antenna.joined_ssid, NETWORK) == 0 &&
                     strcmp(speaker.antenna.joined_secret, PASSPHRASE) == 0,
                 "the radio was asked to join once, with the name and passphrase the form "
                 "carried");
    int set_at = fake_radio_first(&speaker.antenna, FAKE_RADIO_SET_POWER_SAVE);
    int read_at = fake_radio_first(&speaker.antenna, FAKE_RADIO_GET_POWER_SAVE);
    int join_at = fake_radio_first(&speaker.antenna, FAKE_RADIO_JOIN);
    chorus_check(set_at >= 0 && read_at > set_at && join_at > read_at &&
                     speaker.antenna.power_save == CHORUS_WIFI_PS_NONE &&
                     speaker.unit.report.mode_in_effect,
                 "the power save mode was set (event %d) and read back (%d) before the join "
                 "(%d): the provisioning join is the wireless bring-up, not a way around it",
                 set_at, read_at, join_at);
    chorus_check(stored_equals(&flash, CHORUS_PROVISION_KEY_SSID, NETWORK) &&
                     stored_equals(&flash, CHORUS_PROVISION_KEY_SECRET, PASSPHRASE),
                 "the network is in the store under %s and %s", CHORUS_PROVISION_KEY_SSID,
                 CHORUS_PROVISION_KEY_SECRET);
    chorus_check(!speaker.antenna.ap_up && speaker.antenna.ap_stops == 1,
                 "the access point was dropped once the speaker was on its network");

    /* What the bring-up after provisioning is handed. */
    chorus_wifi_config_t link = a_link(CHORUS_TRANSPORT_WIRELESS);
    chorus_wifi_report_t report;
    chorus_check(chorus_provision_export(&speaker.unit, &link) == 0 && link.ssid_known &&
                     link.secret_known &&
                     chorus_wifi_bring_up(&link, &speaker.radio, &report) == CHORUS_WIFI_OK &&
                     report.link_up,
                 "the link configuration exported from it brings the link up: the committed "
                 "`unknown` now means provisioned at run time");
    chorus_check(strstr(report.detail, PASSPHRASE) == NULL,
                 "and the bring-up's report still carries no passphrase");

    refusal = CHORUS_PROVISION_FORM_OK;
    size_t joins = speaker.antenna.joins;
    state = chorus_provision_submit_form(&speaker.unit, FORM, strlen(FORM), 2000, &refusal);
    chorus_check(state == CHORUS_PROVISION_JOINED && refusal == CHORUS_PROVISION_NOT_ACCEPTING &&
                     speaker.antenna.joins == joins,
                 "a form that arrives once joined is refused %s: no access point, no form",
                 chorus_provision_form_status_name(refusal));
}

static void a_failed_join_returns_to_the_access_point_with_the_reason(void)
{
    chorus_section("joining -> join-failed -> ap-up: the reason is kept, nothing is stored");
    static fake_store_t flash;
    static speaker_t speaker;
    static char page[4096];
    fake_store_init(&flash);
    power_up(&speaker, &flash, CHORUS_TRANSPORT_WIRELESS, 12);
    fake_radio_set_network(&speaker.antenna, NETWORK, PASSPHRASE);
    chorus_provision_boot(&speaker.unit, 0);
    size_t before = speaker.unit.trace_count;

    chorus_provision_credentials_t wrong = a_network(NETWORK, "not-the-passphrase");
    chorus_provision_form_status_t refusal = CHORUS_PROVISION_NOT_ACCEPTING;
    chorus_provision_state_t state = chorus_provision_submit(&speaker.unit, &wrong, 0, &refusal);
    print_trace(&speaker.unit);
    chorus_check(state == CHORUS_PROVISION_AP_UP && refusal == CHORUS_PROVISION_FORM_OK &&
                     TRACE_IS(&speaker.unit, before, CHORUS_PROVISION_CREDENTIALS_RECEIVED,
                              CHORUS_PROVISION_JOINING, CHORUS_PROVISION_JOIN_FAILED,
                              CHORUS_PROVISION_AP_UP),
                 "a wrong passphrase: credentials-received, joining, join-failed, back to ap-up");
    chorus_check(strcmp(chorus_provision_reason(&speaker.unit), "auth-error") == 0,
                 "the reason kept is the platform's word: %s",
                 chorus_provision_reason(&speaker.unit));
    chorus_check(speaker.antenna.ap_up && speaker.antenna.ap_starts == 1 &&
                     speaker.antenna.ap_stops == 0,
                 "the access point never went down");
    chorus_check(!fake_store_has(&flash, CHORUS_PROVISION_KEY_SSID) &&
                     !fake_store_has(&flash, CHORUS_PROVISION_KEY_SECRET) &&
                     fake_store_count(&flash, FAKE_STORE_SET, CHORUS_PROVISION_KEY_SSID) == 0,
                 "a network that did not join was never written to the store");
    chorus_check(speaker.unit.wifi.secret[0] == '\0' && !speaker.unit.wifi.secret_known,
                 "and its passphrase is not left in the unit either");
    size_t page_bytes = chorus_provision_page(&speaker.unit, page, sizeof(page));
    chorus_check(page_bytes > 0 && strstr(page, "auth-error") != NULL &&
                     strstr(page, "action=\"/join\"") != NULL,
                 "the join page (%zu bytes) shows the reason above the form", page_bytes);

    chorus_provision_credentials_t absent = a_network("example-elsewhere", PASSPHRASE);
    state = chorus_provision_submit(&speaker.unit, &absent, 0, &refusal);
    chorus_check(state == CHORUS_PROVISION_AP_UP &&
                     strcmp(chorus_provision_reason(&speaker.unit), "network-not-found") == 0,
                 "a network that is not in range: %s", chorus_provision_reason(&speaker.unit));

    speaker.antenna.join_refuses = 1;
    chorus_provision_credentials_t good = a_network(NETWORK, PASSPHRASE);
    state = chorus_provision_submit(&speaker.unit, &good, 0, &refusal);
    chorus_check(state == CHORUS_PROVISION_AP_UP &&
                     strcmp(chorus_provision_reason(&speaker.unit), "join-refused") == 0,
                 "a platform with no word for it leaves the bring-up's own: %s",
                 chorus_provision_reason(&speaker.unit));
    speaker.antenna.join_refuses = 0;

    speaker.antenna.set_refuses = 1;
    size_t joins = speaker.antenna.joins;
    state = chorus_provision_submit(&speaker.unit, &good, 0, &refusal);
    chorus_check(state == CHORUS_PROVISION_AP_UP && speaker.antenna.joins == joins &&
                     strcmp(chorus_provision_reason(&speaker.unit), "power-save-mode-refused") == 0,
                 "a radio that will not take the power save mode is not joined at all: %s",
                 chorus_provision_reason(&speaker.unit));
    speaker.antenna.set_refuses = 0;

    /* A refused FORM is not a join: the reason is the parser's, the radio is
     * not asked. */
    joins = speaker.antenna.joins;
    state = chorus_provision_submit_form(&speaker.unit, "ssid=example-network&secret=short", 33, 0,
                                         &refusal);
    chorus_check(state == CHORUS_PROVISION_AP_UP && refusal == CHORUS_PROVISION_SECRET_TOO_SHORT &&
                     speaker.antenna.joins == joins &&
                     strcmp(chorus_provision_reason(&speaker.unit), "secret-too-short") == 0,
                 "a form the parser refuses asks nothing of the radio; the page will say %s",
                 chorus_provision_reason(&speaker.unit));

    state = chorus_provision_submit(&speaker.unit, &good, 0, &refusal);
    chorus_check(state == CHORUS_PROVISION_JOINED &&
                     chorus_provision_reason(&speaker.unit)[0] == '\0' &&
                     stored_equals(&flash, CHORUS_PROVISION_KEY_SECRET, PASSPHRASE),
                 "after all that the right network joins, is stored, and the reason is cleared");
}

static void a_store_that_refuses_is_a_failure_not_a_silent_success(void)
{
    chorus_section("a network that joined and could not be stored leaves the access point up");
    static fake_store_t flash;
    static speaker_t speaker;
    chorus_provision_form_status_t refusal;

    fake_store_init(&flash);
    power_up(&speaker, &flash, CHORUS_TRANSPORT_WIRELESS, 13);
    chorus_provision_boot(&speaker.unit, 0);
    fake_store_fail_next_set(&flash);
    chorus_provision_state_t state =
        chorus_provision_submit_form(&speaker.unit, FORM, strlen(FORM), 0, &refusal);
    chorus_check(state == CHORUS_PROVISION_AP_UP &&
                     strcmp(chorus_provision_reason(&speaker.unit), "credentials-not-stored") ==
                         0 &&
                     speaker.antenna.ap_up && !fake_store_has(&flash, CHORUS_PROVISION_KEY_SSID),
                 "the first write refused: %s, the access point stays, nothing is stored",
                 chorus_provision_reason(&speaker.unit));

    fake_store_init(&flash);
    power_up(&speaker, &flash, CHORUS_TRANSPORT_WIRELESS, 13);
    chorus_provision_boot(&speaker.unit, 0);
    fake_store_fail_set_after(&flash, 1);
    state = chorus_provision_submit_form(&speaker.unit, FORM, strlen(FORM), 0, &refusal);
    chorus_check(state == CHORUS_PROVISION_AP_UP &&
                     !fake_store_has(&flash, CHORUS_PROVISION_KEY_SSID) &&
                     !fake_store_has(&flash, CHORUS_PROVISION_KEY_SECRET) &&
                     fake_store_count(&flash, FAKE_STORE_ERASE, CHORUS_PROVISION_KEY_SSID) == 1,
                 "the second write refused: the first is taken back out, so the store never "
                 "holds half a network");

    /* Half a network, however it got there, is not a network at boot. */
    static speaker_t half;
    fake_store_init(&flash);
    fake_store_preload(&flash, CHORUS_PROVISION_KEY_SSID, NETWORK, strlen(NETWORK));
    power_up(&half, &flash, CHORUS_TRANSPORT_WIRELESS, 14);
    state = chorus_provision_boot(&half.unit, 0);
    chorus_check(
        state == CHORUS_PROVISION_AP_UP && half.antenna.joins == 0 &&
            TRACE_IS(&half.unit, 0, CHORUS_PROVISION_UNPROVISIONED, CHORUS_PROVISION_AP_UP),
        "a name with no passphrase in the store boots unprovisioned, with no join");
    fake_store_init(&flash);
    fake_store_preload(&flash, CHORUS_PROVISION_KEY_SSID, NETWORK, strlen(NETWORK));
    fake_store_preload(&flash, CHORUS_PROVISION_KEY_SECRET, "abc", 3);
    power_up(&half, &flash, CHORUS_TRANSPORT_WIRELESS, 14);
    state = chorus_provision_boot(&half.unit, 0);
    chorus_check(state == CHORUS_PROVISION_AP_UP && half.antenna.joins == 0,
                 "a stored passphrase outside the bounds is not handed to the radio");
}

static void a_stored_network_that_will_not_join_brings_the_access_point_back(void)
{
    chorus_section("provisioned-at-boot: three joins, then the access point, and the store kept");
    static fake_store_t flash;
    static speaker_t speaker;
    fake_store_init(&flash);
    fake_store_preload(&flash, CHORUS_PROVISION_KEY_SSID, NETWORK, strlen(NETWORK));
    fake_store_preload(&flash, CHORUS_PROVISION_KEY_SECRET, PASSPHRASE, strlen(PASSPHRASE));
    power_up(&speaker, &flash, CHORUS_TRANSPORT_WIRELESS, 15);
    /* The router is off: no network in range under that name. */
    fake_radio_set_network(&speaker.antenna, "example-other-network", PASSPHRASE);
    uint64_t boot_ms = 5000;
    chorus_provision_state_t state = chorus_provision_boot(&speaker.unit, boot_ms);
    print_trace(&speaker.unit);
    /* Past the setup record's own state there is none: the trace starts at
     * provisioned-at-boot. */
    chorus_check(state == CHORUS_PROVISION_AP_UP &&
                     TRACE_IS(&speaker.unit, 0, CHORUS_PROVISION_PROVISIONED_AT_BOOT,
                              CHORUS_PROVISION_JOINING, CHORUS_PROVISION_JOIN_FAILED,
                              CHORUS_PROVISION_JOINING, CHORUS_PROVISION_JOIN_FAILED,
                              CHORUS_PROVISION_JOINING, CHORUS_PROVISION_JOIN_FAILED,
                              CHORUS_PROVISION_AP_UP),
                 "provisioned-at-boot, then joining and join-failed %u times, then ap-up",
                 CHORUS_PROVISION_BOOT_JOIN_ATTEMPTS);
    chorus_check(speaker.antenna.joins == CHORUS_PROVISION_BOOT_JOIN_ATTEMPTS &&
                     speaker.antenna.ap_starts == 1 &&
                     fake_radio_first(&speaker.antenna, FAKE_RADIO_AP_START) >
                         fake_radio_first(&speaker.antenna, FAKE_RADIO_JOIN),
                 "%zu joins (ASSUMED %u), and the access point only after the last of them",
                 speaker.antenna.joins, CHORUS_PROVISION_BOOT_JOIN_ATTEMPTS);
    chorus_check(strcmp(chorus_provision_reason(&speaker.unit), "network-not-found") == 0,
                 "the reason is kept for the form: %s", chorus_provision_reason(&speaker.unit));
    chorus_check(stored_equals(&flash, CHORUS_PROVISION_KEY_SSID, NETWORK) &&
                     stored_equals(&flash, CHORUS_PROVISION_KEY_SECRET, PASSPHRASE) &&
                     fake_store_count(&flash, FAKE_STORE_ERASE, NULL) == 0,
                 "the stored network is still stored: a router that is off is not a wrong "
                 "passphrase");

    /* A bad network typed into the form does not replace the good one. */
    chorus_provision_credentials_t wrong = a_network("example-typo", "some-passphrase");
    chorus_provision_form_status_t refusal;
    state = chorus_provision_submit(&speaker.unit, &wrong, boot_ms + 1000, &refusal);
    chorus_check(state == CHORUS_PROVISION_AP_UP &&
                     stored_equals(&flash, CHORUS_PROVISION_KEY_SSID, NETWORK) &&
                     stored_equals(&flash, CHORUS_PROVISION_KEY_SECRET, PASSPHRASE),
                 "a network that does not join does not overwrite the stored one");

    /* The access point tries the stored network again by itself, on time. */
    uint64_t due = boot_ms + 1000 + (uint64_t)CHORUS_PROVISION_REJOIN_SECONDS * 1000u;
    size_t joins = speaker.antenna.joins;
    state = chorus_provision_tick(&speaker.unit, due - 1);
    chorus_check(state == CHORUS_PROVISION_AP_UP && speaker.antenna.joins == joins,
                 "one millisecond before %u s (ASSUMED) have passed, a tick asks nothing of the "
                 "radio",
                 CHORUS_PROVISION_REJOIN_SECONDS);
    state = chorus_provision_tick(&speaker.unit, due);
    chorus_check(state == CHORUS_PROVISION_AP_UP && speaker.antenna.joins == joins + 1 &&
                     speaker.antenna.ap_up,
                 "at %u s the stored network is tried once; still gone, so the access point "
                 "stays",
                 CHORUS_PROVISION_REJOIN_SECONDS);
    state = chorus_provision_tick(&speaker.unit, due + 1000);
    chorus_check(state == CHORUS_PROVISION_AP_UP && speaker.antenna.joins == joins + 1,
                 "and the next try waits a full interval again");
    /* The router comes back. */
    fake_radio_set_network(&speaker.antenna, NETWORK, PASSPHRASE);
    size_t before = speaker.unit.trace_count;
    state = chorus_provision_tick(&speaker.unit,
                                  due + (uint64_t)CHORUS_PROVISION_REJOIN_SECONDS * 1000u);
    chorus_check(
        state == CHORUS_PROVISION_JOINED && !speaker.antenna.ap_up &&
            TRACE_IS(&speaker.unit, before, CHORUS_PROVISION_JOINING, CHORUS_PROVISION_JOINED),
        "when the network is back the next try joins and drops the access point, with "
        "nobody at the speaker");

    /* With nothing stored there is nothing to try, however long it waits. */
    static fake_store_t empty;
    static speaker_t fresh;
    fake_store_init(&empty);
    power_up(&fresh, &empty, CHORUS_TRANSPORT_WIRELESS, 16);
    chorus_provision_boot(&fresh.unit, 0);
    state = chorus_provision_tick(&fresh.unit, 1000000000u);
    chorus_check(state == CHORUS_PROVISION_AP_UP && fresh.antenna.joins == 0,
                 "an unprovisioned speaker's access point never joins anything by itself");
}

static void a_reset_erases_the_network_and_only_the_network(void)
{
    chorus_section("reset: the two network keys go, the setup secret stays");
    static fake_store_t flash;
    static speaker_t speaker;
    fake_store_init(&flash);
    power_up(&speaker, &flash, CHORUS_TRANSPORT_WIRELESS, 17);
    chorus_provision_boot(&speaker.unit, 0);
    chorus_provision_form_status_t refusal;
    chorus_provision_submit_form(&speaker.unit, FORM, strlen(FORM), 0, &refusal);
    char name[CHORUS_PROVISION_NAME_MAX + 1];
    char secret[CHORUS_PROVISION_SETUP_CHARS + 1];
    snprintf(name, sizeof(name), "%s", chorus_provision_ap_name(&speaker.unit));
    snprintf(secret, sizeof(secret), "%s", chorus_provision_setup_secret(&speaker.unit));

    /* A reset the store refuses changes nothing and says so. */
    flash.erases_fail = 1;
    chorus_check(chorus_provision_reset(&speaker.unit) != 0 &&
                     speaker.unit.state == CHORUS_PROVISION_JOINED &&
                     stored_equals(&flash, CHORUS_PROVISION_KEY_SSID, NETWORK),
                 "a store that will not erase: the reset fails and the state is still joined");
    flash.erases_fail = 0;

    size_t before = speaker.unit.trace_count;
    int rc = chorus_provision_reset(&speaker.unit);
    chorus_check(rc == 0 && TRACE_IS(&speaker.unit, before, CHORUS_PROVISION_RESET,
                                     CHORUS_PROVISION_UNPROVISIONED),
                 "from joined: reset, then unprovisioned (the board restarts from here)");
    chorus_check(!fake_store_has(&flash, CHORUS_PROVISION_KEY_SSID) &&
                     !fake_store_has(&flash, CHORUS_PROVISION_KEY_SECRET) &&
                     fake_store_has(&flash, CHORUS_PROVISION_KEY_SETUP),
                 "%s and %s are gone and %s is not", CHORUS_PROVISION_KEY_SSID,
                 CHORUS_PROVISION_KEY_SECRET, CHORUS_PROVISION_KEY_SETUP);
    chorus_check(speaker.unit.wifi.secret[0] == '\0' && speaker.unit.wifi.ssid[0] == '\0',
                 "the unit holds no network in RAM either");

    static speaker_t again;
    power_up(&again, &flash, CHORUS_TRANSPORT_WIRELESS, 18);
    chorus_provision_state_t state = chorus_provision_boot(&again.unit, 0);
    chorus_check(state == CHORUS_PROVISION_AP_UP && again.antenna.joins == 0 &&
                     strcmp(chorus_provision_ap_name(&again.unit), name) == 0 &&
                     strcmp(chorus_provision_setup_secret(&again.unit), secret) == 0,
                 "the next boot is unprovisioned: the access point returns under the same name "
                 "and passphrase");

    /* With the access point up (a stored network that would not join), a
     * reset ends in ap-up and stops the unattended retries. */
    static speaker_t stuck;
    fake_store_init(&flash);
    fake_store_preload(&flash, CHORUS_PROVISION_KEY_SSID, NETWORK, strlen(NETWORK));
    fake_store_preload(&flash, CHORUS_PROVISION_KEY_SECRET, PASSPHRASE, strlen(PASSPHRASE));
    power_up(&stuck, &flash, CHORUS_TRANSPORT_WIRELESS, 19);
    stuck.antenna.join_refuses = 1;
    chorus_provision_boot(&stuck.unit, 0);
    before = stuck.unit.trace_count;
    rc = chorus_provision_reset(&stuck.unit);
    size_t joins = stuck.antenna.joins;
    chorus_check(rc == 0 &&
                     TRACE_IS(&stuck.unit, before, CHORUS_PROVISION_RESET,
                              CHORUS_PROVISION_UNPROVISIONED, CHORUS_PROVISION_AP_UP) &&
                     stuck.antenna.ap_up && stuck.antenna.ap_starts == 1 &&
                     chorus_provision_reason(&stuck.unit)[0] == '\0',
                 "with the access point up: reset, unprovisioned, ap-up, the same access point");
    chorus_check(chorus_provision_tick(&stuck.unit, 1000000000u) == CHORUS_PROVISION_AP_UP &&
                     stuck.antenna.joins == joins,
                 "and nothing is retried afterwards: there is no stored network to retry");
}

static void every_state_has_a_name(void)
{
    chorus_section("every state has its own name");
    int distinct = 1;
    for (int i = CHORUS_PROVISION_UNPROVISIONED; i <= CHORUS_PROVISION_REFUSED; i++) {
        const char *name = chorus_provision_state_name((chorus_provision_state_t)i);
        distinct &= strcmp(name, "unknown") != 0;
        for (int j = i + 1; j <= CHORUS_PROVISION_REFUSED; j++) {
            distinct &= strcmp(name, chorus_provision_state_name((chorus_provision_state_t)j)) != 0;
        }
    }
    chorus_check(distinct, "%d states, %d names", CHORUS_PROVISION_REFUSED + 1,
                 CHORUS_PROVISION_REFUSED + 1);
}

static void the_pages_echo_nothing_a_phone_sent(void)
{
    chorus_section("the pages: fixed text and a refusal's name, nothing a phone sent");
    static fake_store_t flash;
    static speaker_t speaker;
    static char page[4096];
    fake_store_init(&flash);
    power_up(&speaker, &flash, CHORUS_TRANSPORT_WIRELESS, 20);
    chorus_provision_boot(&speaker.unit, 0);
    size_t bytes = chorus_provision_page(&speaker.unit, page, sizeof(page));
    chorus_check(
        bytes > 0 && bytes == strlen(page) && strstr(page, "name=\"ssid\"") != NULL &&
            strstr(page, "name=\"secret\"") != NULL && strstr(page, "type=\"password\"") != NULL &&
            strstr(page, "method=\"post\"") != NULL && strstr(page, "class=\"why\"") == NULL,
        "the join page (%zu bytes) is a form of two fields, posted, with no refusal on "
        "a fresh speaker",
        bytes);
    chorus_check(strstr(page, "<script") == NULL && strstr(page, "http") == NULL &&
                     strstr(page, "src=") == NULL,
                 "it has no script and refers to nothing outside itself");
    chorus_check(chorus_provision_page(&speaker.unit, page, 64) == 0,
                 "a buffer too small for it gets nothing, not a cut page");

    /* A reason that is not a plain word, however it got there, is replaced. */
    snprintf(speaker.unit.reason, sizeof(speaker.unit.reason), "<script>alert(1)</script>");
    bytes = chorus_provision_page(&speaker.unit, page, sizeof(page));
    chorus_check(bytes > 0 && strstr(page, "alert") == NULL && strstr(page, "unnamed") != NULL,
                 "a reason that is not a word of [a-z0-9-] is shown as `unnamed`, never echoed");

    bytes = chorus_provision_reply_page(CHORUS_PROVISION_FORM_OK, page, sizeof(page));
    chorus_check(bytes > 0 && strstr(page, "Received") != NULL,
                 "the reply to an accepted form (%zu bytes) says it was received", bytes);
    bytes = chorus_provision_reply_page(CHORUS_PROVISION_SECRET_TOO_SHORT, page, sizeof(page));
    chorus_check(bytes > 0 && strstr(page, "secret-too-short") != NULL,
                 "the reply to a refused form names the refusal");
}

/* --- the full run ------------------------------------------------------------ */

/* Whether any of the three things that must never be written down is in
 * `text`. */
static int leaks(const char *text, const char *setup_secret)
{
    return strstr(text, PASSPHRASE) != NULL || strstr(text, NETWORK) != NULL ||
           strstr(text, setup_secret) != NULL;
}

static void the_full_host_run(void)
{
    chorus_section("the full run: first boot to a reboot that joins with no access point");
    static fake_store_t flash;
    static speaker_t speaker;
    static char page[4096];
    static char store_log[16384];
    static char everything[3 * LOG_BYTES + 4096];
    size_t everything_used = 0;
    fake_store_init(&flash);

    /* 1. First boot: nothing stored. */
    power_up(&speaker, &flash, CHORUS_TRANSPORT_WIRELESS, 0xC0FFEEu);
    fake_radio_set_network(&speaker.antenna, NETWORK, PASSPHRASE);
    chorus_provision_state_t state = chorus_provision_boot(&speaker.unit, 0);
    chorus_check(state == CHORUS_PROVISION_AP_UP && speaker.antenna.ap_up &&
                     speaker.antenna.joins == 0,
                 "1. first boot is unprovisioned: the access point is up and nothing was joined");
    char setup_secret[CHORUS_PROVISION_SETUP_CHARS + 1];
    snprintf(setup_secret, sizeof(setup_secret), "%s",
             chorus_provision_setup_secret(&speaker.unit));

    /* 2. The phone joins the access point with the secret off the console and
     * fetches the page. */
    chorus_check(strcmp(speaker.antenna.ap_key, setup_secret) == 0 &&
                     chorus_provision_page(&speaker.unit, page, sizeof(page)) > 0 &&
                     !leaks(page, setup_secret),
                 "2. the access point's key is the setup secret; the page served carries no "
                 "name or secret");

    /* 3. The form is posted. */
    chorus_provision_form_status_t refusal = CHORUS_PROVISION_NOT_ACCEPTING;
    state = chorus_provision_submit_form(&speaker.unit, FORM, strlen(FORM), 30000, &refusal);
    chorus_check(state == CHORUS_PROVISION_JOINED && refusal == CHORUS_PROVISION_FORM_OK &&
                     strcmp(speaker.antenna.joined_ssid, speaker.antenna.network_ssid) == 0,
                 "3. the form is posted and the speaker joins the radio's network");
    chorus_check(stored_equals(&flash, CHORUS_PROVISION_KEY_SSID, NETWORK) &&
                     stored_equals(&flash, CHORUS_PROVISION_KEY_SECRET, PASSPHRASE) &&
                     !speaker.antenna.ap_up,
                 "4. the credentials are in the store and the access point is down");
    print_trace(&speaker.unit);
    everything_used += (size_t)snprintf(everything + everything_used,
                                        sizeof(everything) - everything_used, "%s", speaker.log);

    /* 5. "Reboot": a new radio and a new unit over the same store. */
    static speaker_t second;
    power_up(&second, &flash, CHORUS_TRANSPORT_WIRELESS, 0xBADC0DEu);
    fake_radio_set_network(&second.antenna, NETWORK, PASSPHRASE);
    state = chorus_provision_boot(&second.unit, 0);
    print_trace(&second.unit);
    chorus_check(state == CHORUS_PROVISION_JOINED &&
                     TRACE_IS(&second.unit, 0, CHORUS_PROVISION_PROVISIONED_AT_BOOT,
                              CHORUS_PROVISION_JOINING, CHORUS_PROVISION_JOINED),
                 "5. after a reboot: provisioned-at-boot, joining, joined");
    chorus_check(second.antenna.ap_starts == 0 &&
                     fake_radio_count(&second.antenna, FAKE_RADIO_AP_START) == 0 &&
                     second.antenna.joins == 1 &&
                     strcmp(second.antenna.joined_secret, PASSPHRASE) == 0,
                 "6. with no access point at all: zero ap-start events, one join, from the "
                 "store");
    everything_used += (size_t)snprintf(everything + everything_used,
                                        sizeof(everything) - everything_used, "%s", second.log);

    /* 7. The network goes away; the next boot's joins fail. */
    static speaker_t third;
    power_up(&third, &flash, CHORUS_TRANSPORT_WIRELESS, 0xFEEDu);
    fake_radio_set_network(&third.antenna, NETWORK, "the-passphrase-was-changed");
    state = chorus_provision_boot(&third.unit, 0);
    chorus_check(state == CHORUS_PROVISION_AP_UP && third.antenna.ap_up &&
                     third.antenna.joins == CHORUS_PROVISION_BOOT_JOIN_ATTEMPTS &&
                     strcmp(chorus_provision_reason(&third.unit), "auth-error") == 0 &&
                     chorus_provision_page(&third.unit, page, sizeof(page)) > 0 &&
                     strstr(page, "auth-error") != NULL && !leaks(page, setup_secret),
                 "7. a failed join returns to the access point with the reason (%s) on the "
                 "page",
                 chorus_provision_reason(&third.unit));
    chorus_check(strcmp(third.antenna.ap_key, setup_secret) == 0,
                 "8. under the same setup secret as the first boot");

    /* 9. Reset, then a boot. */
    chorus_check(chorus_provision_reset(&third.unit) == 0 &&
                     !fake_store_has(&flash, CHORUS_PROVISION_KEY_SSID) &&
                     !fake_store_has(&flash, CHORUS_PROVISION_KEY_SECRET),
                 "9. reset erases the stored network");
    everything_used += (size_t)snprintf(everything + everything_used,
                                        sizeof(everything) - everything_used, "%s", third.log);
    static speaker_t fourth;
    power_up(&fourth, &flash, CHORUS_TRANSPORT_WIRELESS, 0xABCDu);
    state = chorus_provision_boot(&fourth.unit, 0);
    chorus_check(
        state == CHORUS_PROVISION_AP_UP && fourth.antenna.joins == 0 &&
            TRACE_IS(&fourth.unit, 0, CHORUS_PROVISION_UNPROVISIONED, CHORUS_PROVISION_AP_UP),
        "10. and the boot after it is unprovisioned again");
    everything_used += (size_t)snprintf(everything + everything_used,
                                        sizeof(everything) - everything_used, "%s", fourth.log);

    /* The rule the whole unit is built around, held over everything the run
     * produced: the unit's lines for the log, and the store's event log. */
    chorus_section("no network name, passphrase or setup secret was written down anywhere");
    fake_store_render(&flash, store_log, sizeof(store_log));
    printf("     the unit's lines over the run (%zu bytes):\n", everything_used);
    for (const char *line = everything; *line != '\0';) {
        const char *end = strchr(line, '\n');
        size_t length = end != NULL ? (size_t)(end - line) : strlen(line);
        printf("       %.*s\n", (int)length, line);
        line += length + (end != NULL ? 1 : 0);
    }
    chorus_check(everything_used > 0 && strstr(everything, "provision: joined") != NULL &&
                     strstr(everything, "provision: AP up name=") != NULL,
                 "the unit logged the run (%zu bytes of lines)", everything_used);
    chorus_check(strstr(everything, PASSPHRASE) == NULL,
                 "the network's passphrase is in no line the unit logged");
    chorus_check(strstr(everything, NETWORK) == NULL,
                 "the network's name is in no line the unit logged");
    chorus_check(strstr(everything, setup_secret) == NULL,
                 "the setup secret is in no line the unit logged (the binding prints it, once, "
                 "on the console)");
    chorus_check(strstr(everything, "the-passphrase-was-changed") == NULL,
                 "a passphrase that failed is in no line either");
    chorus_check(flash.event_count > 0 && strstr(store_log, "set wifi_secret length=20 ok") != NULL,
                 "the store's event log recorded the write of the passphrase as a key and a "
                 "length (%zu events)",
                 flash.event_count);
    chorus_check(!leaks(store_log, setup_secret), "and holds no name, passphrase or setup secret");
    chorus_check(strstr(everything, "ssid_bytes=15 secret_bytes=20") != NULL,
                 "what the unit says of a received network is its lengths: ssid_bytes=15 "
                 "secret_bytes=20");
}

int main(void)
{
    the_store_seam_checks_keys_and_bounds_before_the_medium();
    the_form_parser_reads_what_a_phone_sends();
    the_form_parser_refuses_hostile_input_by_name();
    every_state_has_a_name();
    a_wired_profile_never_provisions();
    the_first_boot_makes_a_setup_secret_and_raises_the_access_point();
    provisioning_refuses_by_name_when_it_cannot_run();
    a_good_network_is_joined_then_stored_then_the_access_point_drops();
    a_failed_join_returns_to_the_access_point_with_the_reason();
    a_store_that_refuses_is_a_failure_not_a_silent_success();
    a_stored_network_that_will_not_join_brings_the_access_point_back();
    a_reset_erases_the_network_and_only_the_network();
    the_pages_echo_nothing_a_phone_sent();
    the_full_host_run();
    return chorus_test_report("test_provision");
}
