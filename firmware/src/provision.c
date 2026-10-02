/* Wi-Fi provisioning: the decisions (chorus/provision.h says what and why).
 *
 * Read the header first. What is worth saying here is only how the rules in
 * it are kept:
 *
 *   - every state change goes through `enter`, which is also what writes the
 *     trace, so the path a test reads is the path that was taken;
 *   - every line for the log goes through `say`, and no call to it is handed
 *     a network name, a passphrase or the setup secret. The test greps for
 *     all three;
 *   - every join goes through `join_once`, which is `chorus_wifi_bring_up`:
 *     the power save mode is set and read back before the radio is asked to
 *     join, here as everywhere else;
 *   - a passphrase that is no longer needed is wiped (`forget_network`), so a
 *     refused form or a failed join does not leave one sitting in the
 *     structure. */

#include "chorus/provision.h"

#include <stdarg.h>
#include <stdio.h>
#include <string.h>

/* 31 symbols: the lower-case letters and digits with 0, 1, i, l and o left
 * out, so a secret read off a console and typed into a phone has no
 * look-alikes in it. */
static const char ALPHABET[] = "abcdefghjkmnpqrstuvwxyz23456789";
#define ALPHABET_SIZE (sizeof(ALPHABET) - 1)

/* The stored form of the setup record: `<suffix>:<secret>`. */
#define SETUP_RECORD_BYTES (CHORUS_PROVISION_NAME_SUFFIX_CHARS + 1 + CHORUS_PROVISION_SETUP_CHARS)

const char *chorus_provision_state_name(chorus_provision_state_t state)
{
    switch (state) {
    case CHORUS_PROVISION_UNPROVISIONED:
        return "unprovisioned";
    case CHORUS_PROVISION_AP_UP:
        return "ap-up";
    case CHORUS_PROVISION_CREDENTIALS_RECEIVED:
        return "credentials-received";
    case CHORUS_PROVISION_JOINING:
        return "joining";
    case CHORUS_PROVISION_JOINED:
        return "joined";
    case CHORUS_PROVISION_JOIN_FAILED:
        return "join-failed";
    case CHORUS_PROVISION_PROVISIONED_AT_BOOT:
        return "provisioned-at-boot";
    case CHORUS_PROVISION_RESET:
        return "reset";
    case CHORUS_PROVISION_WIRED:
        return "wired";
    case CHORUS_PROVISION_REFUSED:
        return "refused";
    }
    return "unknown";
}

const char *chorus_provision_form_status_name(chorus_provision_form_status_t status)
{
    switch (status) {
    case CHORUS_PROVISION_FORM_OK:
        return "ok";
    case CHORUS_PROVISION_FORM_TOO_LONG:
        return "form-too-long";
    case CHORUS_PROVISION_FORM_MALFORMED:
        return "form-malformed";
    case CHORUS_PROVISION_FORM_BAD_ESCAPE:
        return "form-bad-escape";
    case CHORUS_PROVISION_FORM_EMBEDDED_NUL:
        return "form-embedded-nul";
    case CHORUS_PROVISION_FORM_UNKNOWN_FIELD:
        return "form-unknown-field";
    case CHORUS_PROVISION_FORM_DUPLICATE_FIELD:
        return "form-duplicate-field";
    case CHORUS_PROVISION_SSID_MISSING:
        return "ssid-missing";
    case CHORUS_PROVISION_SSID_EMPTY:
        return "ssid-empty";
    case CHORUS_PROVISION_SSID_TOO_LONG:
        return "ssid-too-long";
    case CHORUS_PROVISION_SECRET_MISSING:
        return "secret-missing";
    case CHORUS_PROVISION_SECRET_TOO_SHORT:
        return "secret-too-short";
    case CHORUS_PROVISION_SECRET_TOO_LONG:
        return "secret-too-long";
    case CHORUS_PROVISION_SECRET_NOT_HEX:
        return "secret-not-hex";
    case CHORUS_PROVISION_SECRET_NOT_PRINTABLE:
        return "secret-not-printable";
    case CHORUS_PROVISION_NOT_ACCEPTING:
        return "not-accepting";
    }
    return "unknown";
}

/* --- the bounds and the form ------------------------------------------------- */

static int hex_value(char c)
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

chorus_provision_form_status_t chorus_provision_check_credentials(const char *ssid,
                                                                  size_t ssid_length,
                                                                  const char *secret,
                                                                  size_t secret_length)
{
    if (ssid == NULL || ssid_length == 0) {
        return CHORUS_PROVISION_SSID_EMPTY;
    }
    if (ssid_length > CHORUS_PROVISION_SSID_MAX) {
        return CHORUS_PROVISION_SSID_TOO_LONG;
    }
    if (memchr(ssid, '\0', ssid_length) != NULL) {
        return CHORUS_PROVISION_FORM_EMBEDDED_NUL;
    }
    if (secret == NULL || secret_length < CHORUS_PROVISION_SECRET_MIN) {
        /* An open network has no passphrase at all and lands here too: this
         * speaker does not join one. */
        return CHORUS_PROVISION_SECRET_TOO_SHORT;
    }
    if (secret_length > CHORUS_PROVISION_SECRET_MAX) {
        return CHORUS_PROVISION_SECRET_TOO_LONG;
    }
    if (secret_length == CHORUS_PROVISION_SECRET_MAX) {
        /* 64 characters are the key itself, in hexadecimal, and nothing else. */
        for (size_t i = 0; i < secret_length; i++) {
            if (hex_value(secret[i]) < 0) {
                return CHORUS_PROVISION_SECRET_NOT_HEX;
            }
        }
        return CHORUS_PROVISION_FORM_OK;
    }
    for (size_t i = 0; i < secret_length; i++) {
        unsigned char c = (unsigned char)secret[i];
        if (c == '\0') {
            return CHORUS_PROVISION_FORM_EMBEDDED_NUL;
        }
        if (c < 32 || c > 126) {
            return CHORUS_PROVISION_SECRET_NOT_PRINTABLE;
        }
    }
    return CHORUS_PROVISION_FORM_OK;
}

/* Decode one urlencoded value of `length` bytes into `out` (capacity
 * includes the terminator). A value that would not fit reports `too_long`
 * rather than being cut: a passphrase cut to fit is a different passphrase. */
static chorus_provision_form_status_t decode_value(const char *in, size_t length, char *out,
                                                   size_t capacity, size_t *out_length,
                                                   chorus_provision_form_status_t too_long)
{
    size_t used = 0;
    for (size_t i = 0; i < length; i++) {
        char c = in[i];
        if (c == '+') {
            c = ' ';
        } else if (c == '%') {
            if (i + 2 >= length) {
                return CHORUS_PROVISION_FORM_BAD_ESCAPE;
            }
            int high = hex_value(in[i + 1]);
            int low = hex_value(in[i + 2]);
            if (high < 0 || low < 0) {
                return CHORUS_PROVISION_FORM_BAD_ESCAPE;
            }
            c = (char)(high * 16 + low);
            i += 2;
        }
        if (c == '\0') {
            return CHORUS_PROVISION_FORM_EMBEDDED_NUL;
        }
        if (used + 1 >= capacity) {
            return too_long;
        }
        out[used++] = c;
    }
    out[used] = '\0';
    *out_length = used;
    return CHORUS_PROVISION_FORM_OK;
}

static chorus_provision_form_status_t parse_form(const char *body, size_t length,
                                                 chorus_provision_credentials_t *out)
{
    if (length > CHORUS_PROVISION_FORM_MAX) {
        return CHORUS_PROVISION_FORM_TOO_LONG;
    }
    int have_ssid = 0;
    int have_secret = 0;
    size_t at = 0;
    while (at < length) {
        size_t end = at;
        while (end < length && body[end] != '&') {
            end++;
        }
        /* One pair: body[at..end). An empty pair (a trailing `&`, or `&&`) is
         * not a field and not an error either. */
        if (end > at) {
            size_t equals = at;
            while (equals < end && body[equals] != '=') {
                equals++;
            }
            if (equals == end || equals == at) {
                return CHORUS_PROVISION_FORM_MALFORMED;
            }
            size_t name_length = equals - at;
            const char *value = body + equals + 1;
            size_t value_length = end - equals - 1;
            chorus_provision_form_status_t status;
            /* The names are matched as written: a name that needs escaping is
             * not one of the two this form has. */
            if (name_length == 4 && memcmp(body + at, "ssid", 4) == 0) {
                if (have_ssid) {
                    return CHORUS_PROVISION_FORM_DUPLICATE_FIELD;
                }
                have_ssid = 1;
                status = decode_value(value, value_length, out->ssid, sizeof(out->ssid),
                                      &out->ssid_length, CHORUS_PROVISION_SSID_TOO_LONG);
            } else if (name_length == 6 && memcmp(body + at, "secret", 6) == 0) {
                if (have_secret) {
                    return CHORUS_PROVISION_FORM_DUPLICATE_FIELD;
                }
                have_secret = 1;
                status = decode_value(value, value_length, out->secret, sizeof(out->secret),
                                      &out->secret_length, CHORUS_PROVISION_SECRET_TOO_LONG);
            } else {
                return CHORUS_PROVISION_FORM_UNKNOWN_FIELD;
            }
            if (status != CHORUS_PROVISION_FORM_OK) {
                return status;
            }
        }
        at = end + 1;
    }
    if (!have_ssid) {
        return CHORUS_PROVISION_SSID_MISSING;
    }
    if (!have_secret) {
        return CHORUS_PROVISION_SECRET_MISSING;
    }
    return chorus_provision_check_credentials(out->ssid, out->ssid_length, out->secret,
                                              out->secret_length);
}

chorus_provision_form_status_t chorus_provision_parse_form(const char *body, size_t length,
                                                           chorus_provision_credentials_t *out)
{
    memset(out, 0, sizeof(*out));
    if (body == NULL) {
        return CHORUS_PROVISION_SSID_MISSING;
    }
    chorus_provision_form_status_t status = parse_form(body, length, out);
    if (status != CHORUS_PROVISION_FORM_OK) {
        /* Nothing of a refused body is handed on, not even the half that
         * parsed. */
        memset(out, 0, sizeof(*out));
    }
    return status;
}

/* --- the unit ---------------------------------------------------------------- */

static void say(const chorus_provision_t *p, const char *fmt, ...)
{
    if (p->platform.log == NULL) {
        return;
    }
    char line[160];
    va_list args;
    va_start(args, fmt);
    vsnprintf(line, sizeof(line), fmt, args);
    va_end(args);
    p->platform.log(p->platform.ctx, line);
}

static void enter(chorus_provision_t *p, chorus_provision_state_t state)
{
    p->state = state;
    if (p->trace_count < CHORUS_PROVISION_TRACE) {
        p->trace[p->trace_count++] = state;
    } else {
        p->trace_dropped++;
    }
}

static void set_reason(chorus_provision_t *p, const char *reason)
{
    snprintf(p->reason, sizeof(p->reason), "%s", reason);
}

static chorus_provision_state_t refuse(chorus_provision_t *p, const char *reason)
{
    set_reason(p, reason);
    enter(p, CHORUS_PROVISION_REFUSED);
    say(p, "provision: refused reason=%s", reason);
    return p->state;
}

/* Wipe the network held for the join in hand. */
static void forget_network(chorus_provision_t *p)
{
    memset(p->wifi.ssid, 0, sizeof(p->wifi.ssid));
    memset(p->wifi.secret, 0, sizeof(p->wifi.secret));
    p->wifi.ssid_known = 0;
    p->wifi.secret_known = 0;
}

static void hold_network(chorus_provision_t *p, const chorus_provision_credentials_t *network)
{
    forget_network(p);
    memcpy(p->wifi.ssid, network->ssid, network->ssid_length);
    memcpy(p->wifi.secret, network->secret, network->secret_length);
    p->wifi.ssid_known = 1;
    p->wifi.secret_known = 1;
}

void chorus_provision_init(chorus_provision_t *p, const chorus_wifi_config_t *link,
                           const chorus_store_t *store, chorus_radio_t *radio,
                           const chorus_provision_platform_t *platform)
{
    memset(p, 0, sizeof(*p));
    p->wifi = *link;
    /* Whatever the configuration named is not a source: the committed file
     * says `unknown`, and the store is where a network lives. */
    forget_network(p);
    p->store = store;
    p->radio = radio;
    p->platform = *platform;
    p->state = CHORUS_PROVISION_UNPROVISIONED;
}

static int in_alphabet(const char *text, size_t length)
{
    for (size_t i = 0; i < length; i++) {
        if (text[i] == '\0' || strchr(ALPHABET, text[i]) == NULL) {
            return 0;
        }
    }
    return 1;
}

/* `count` characters of the alphabet, each uniform: a byte that would bias
 * the choice is thrown away and another is drawn. Returns 0 on success. */
static int draw(const chorus_provision_t *p, char *out, size_t count)
{
    /* The largest multiple of the alphabet's size that fits a byte. */
    const unsigned limit = 256u - (256u % (unsigned)ALPHABET_SIZE);
    size_t made = 0;
    /* 64 draws of 32 bytes: a source that cannot fill 18 characters out of
     * 2048 bytes is not a random source. */
    for (int round = 0; round < 64 && made < count; round++) {
        uint8_t bytes[32];
        if (p->platform.random == NULL ||
            p->platform.random(p->platform.ctx, bytes, sizeof(bytes)) != 0) {
            return -1;
        }
        for (size_t i = 0; i < sizeof(bytes) && made < count; i++) {
            if (bytes[i] < limit) {
                out[made++] = ALPHABET[bytes[i] % ALPHABET_SIZE];
            }
        }
        memset(bytes, 0, sizeof(bytes));
    }
    return (made == count) ? 0 : -1;
}

/* Load the setup record, or make one and store it. Returns NULL on success,
 * else the refusal's name. */
static const char *load_setup(chorus_provision_t *p)
{
    char record[SETUP_RECORD_BYTES];
    size_t length = 0;
    chorus_store_status_t status =
        chorus_store_get(p->store, CHORUS_PROVISION_KEY_SETUP, record, sizeof(record), &length);
    if (status == CHORUS_STORE_FAILED || status == CHORUS_STORE_BAD_KEY) {
        return "setup-secret-unreadable";
    }
    int usable =
        status == CHORUS_STORE_OK && length == SETUP_RECORD_BYTES &&
        record[CHORUS_PROVISION_NAME_SUFFIX_CHARS] == ':' &&
        in_alphabet(record, CHORUS_PROVISION_NAME_SUFFIX_CHARS) &&
        in_alphabet(record + CHORUS_PROVISION_NAME_SUFFIX_CHARS + 1, CHORUS_PROVISION_SETUP_CHARS);
    if (!usable) {
        /* First boot, or a record this build cannot read: make one. */
        if (draw(p, record, CHORUS_PROVISION_NAME_SUFFIX_CHARS) != 0 ||
            draw(p, record + CHORUS_PROVISION_NAME_SUFFIX_CHARS + 1,
                 CHORUS_PROVISION_SETUP_CHARS) != 0) {
            memset(record, 0, sizeof(record));
            return "no-random-source";
        }
        record[CHORUS_PROVISION_NAME_SUFFIX_CHARS] = ':';
        if (chorus_store_set(p->store, CHORUS_PROVISION_KEY_SETUP, record, sizeof(record)) !=
            CHORUS_STORE_OK) {
            memset(record, 0, sizeof(record));
            return "setup-secret-not-stored";
        }
        say(p, "provision: setup secret created");
    } else {
        say(p, "provision: setup secret loaded");
    }
    snprintf(p->ap_name, sizeof(p->ap_name), "%s%.*s", CHORUS_PROVISION_NAME_PREFIX,
             CHORUS_PROVISION_NAME_SUFFIX_CHARS, record);
    memcpy(p->setup_secret, record + CHORUS_PROVISION_NAME_SUFFIX_CHARS + 1,
           CHORUS_PROVISION_SETUP_CHARS);
    p->setup_secret[CHORUS_PROVISION_SETUP_CHARS] = '\0';
    memset(record, 0, sizeof(record));
    return NULL;
}

/* Read the stored network. Returns 1 when one is there and passes the bounds,
 * 0 when there is none to use, -1 when the store itself failed. */
static int load_network(chorus_provision_t *p, chorus_provision_credentials_t *network)
{
    memset(network, 0, sizeof(*network));
    chorus_store_status_t ssid =
        chorus_store_get(p->store, CHORUS_PROVISION_KEY_SSID, network->ssid,
                         CHORUS_PROVISION_SSID_MAX, &network->ssid_length);
    chorus_store_status_t secret =
        chorus_store_get(p->store, CHORUS_PROVISION_KEY_SECRET, network->secret,
                         CHORUS_PROVISION_SECRET_MAX, &network->secret_length);
    if (ssid == CHORUS_STORE_FAILED || secret == CHORUS_STORE_FAILED) {
        memset(network, 0, sizeof(*network));
        return -1;
    }
    if (ssid == CHORUS_STORE_MISSING && secret == CHORUS_STORE_MISSING) {
        return 0;
    }
    if (ssid != CHORUS_STORE_OK || secret != CHORUS_STORE_OK ||
        chorus_provision_check_credentials(network->ssid, network->ssid_length, network->secret,
                                           network->secret_length) != CHORUS_PROVISION_FORM_OK) {
        /* Half a pair, or a value no form would have let through: not a
         * network. It is left where it is; the next good join replaces it. */
        memset(network, 0, sizeof(*network));
        say(p, "provision: the stored network is unusable and is ignored");
        return 0;
    }
    return 1;
}

/* One join of the network in hand, through the wireless bring-up. Returns 1
 * when the link is up. On failure the reason is set. */
static int join_once(chorus_provision_t *p)
{
    enter(p, CHORUS_PROVISION_JOINING);
    p->joins++;
    chorus_wifi_status_t status = chorus_wifi_bring_up(&p->wifi, p->radio, &p->report);
    if (p->report.link_up) {
        p->reason[0] = '\0';
        return 1;
    }
    const char *reason = NULL;
    if (status == CHORUS_WIFI_JOIN_REFUSED && p->platform.join_reason != NULL) {
        reason = p->platform.join_reason(p->platform.ctx);
    }
    if (reason == NULL || reason[0] == '\0') {
        reason = chorus_wifi_status_name(status);
    }
    set_reason(p, reason);
    enter(p, CHORUS_PROVISION_JOIN_FAILED);
    say(p, "provision: join failed reason=%s", p->reason);
    return 0;
}

static chorus_provision_state_t raise_ap(chorus_provision_t *p, uint64_t now_ms)
{
    if (!p->ap_up) {
        if (p->platform.ap_start == NULL ||
            p->platform.ap_start(p->platform.ctx, p->ap_name, p->setup_secret) != 0) {
            return refuse(p, "ap-start-refused");
        }
        p->ap_up = 1;
        say(p, "provision: AP up name=%s", p->ap_name);
    }
    p->rejoin_at_ms = now_ms + (uint64_t)CHORUS_PROVISION_REJOIN_SECONDS * 1000u;
    enter(p, CHORUS_PROVISION_AP_UP);
    return p->state;
}

static void drop_ap(chorus_provision_t *p)
{
    if (p->ap_up) {
        if (p->platform.ap_stop != NULL) {
            /* An access point that would not go down is said, and the speaker
             * is on its network all the same. */
            if (p->platform.ap_stop(p->platform.ctx) != 0) {
                say(p, "provision: the access point would not stop");
            }
        }
        p->ap_up = 0;
        say(p, "provision: AP down");
    }
}

chorus_provision_state_t chorus_provision_boot(chorus_provision_t *p, uint64_t now_ms)
{
    p->reason[0] = '\0';
    if (p->wifi.transport != CHORUS_TRANSPORT_WIRELESS) {
        enter(p, CHORUS_PROVISION_WIRED);
        say(p, "provision: wired link, nothing to provision");
        return p->state;
    }
    const char *refusal = load_setup(p);
    if (refusal != NULL) {
        return refuse(p, refusal);
    }
    chorus_provision_credentials_t network;
    int found = load_network(p, &network);
    if (found < 0) {
        return refuse(p, "store-failed");
    }
    p->stored = found;
    if (!found) {
        enter(p, CHORUS_PROVISION_UNPROVISIONED);
        say(p, "provision: unprovisioned");
        return raise_ap(p, now_ms);
    }
    enter(p, CHORUS_PROVISION_PROVISIONED_AT_BOOT);
    say(p, "provision: provisioned at boot");
    hold_network(p, &network);
    memset(&network, 0, sizeof(network));
    for (unsigned attempt = 1; attempt <= CHORUS_PROVISION_BOOT_JOIN_ATTEMPTS; attempt++) {
        say(p, "provision: joining (attempt %u of %u)", attempt,
            CHORUS_PROVISION_BOOT_JOIN_ATTEMPTS);
        if (join_once(p)) {
            enter(p, CHORUS_PROVISION_JOINED);
            say(p, "provision: joined");
            return p->state;
        }
    }
    /* The stored network stays stored: the reason is kept for the form, and
     * the access point tries it again by itself (chorus_provision_tick). */
    forget_network(p);
    return raise_ap(p, now_ms);
}

/* Keep the network in hand. Returns 1 when both keys were written. A second
 * write that fails takes the first back out, so the store never holds one
 * network's name beside another's passphrase as if they were a pair. */
static int keep_network(chorus_provision_t *p)
{
    size_t ssid_length = strlen(p->wifi.ssid);
    size_t secret_length = strlen(p->wifi.secret);
    if (chorus_store_set(p->store, CHORUS_PROVISION_KEY_SSID, p->wifi.ssid, ssid_length) !=
        CHORUS_STORE_OK) {
        return 0;
    }
    if (chorus_store_set(p->store, CHORUS_PROVISION_KEY_SECRET, p->wifi.secret, secret_length) !=
        CHORUS_STORE_OK) {
        (void)chorus_store_erase(p->store, CHORUS_PROVISION_KEY_SSID);
        p->stored = 0;
        return 0;
    }
    p->stored = 1;
    return 1;
}

chorus_provision_state_t chorus_provision_submit(chorus_provision_t *p,
                                                 const chorus_provision_credentials_t *network,
                                                 uint64_t now_ms,
                                                 chorus_provision_form_status_t *refusal)
{
    chorus_provision_form_status_t status = CHORUS_PROVISION_FORM_OK;
    if (p->state != CHORUS_PROVISION_AP_UP) {
        status = CHORUS_PROVISION_NOT_ACCEPTING;
    } else {
        status = chorus_provision_check_credentials(network->ssid, network->ssid_length,
                                                    network->secret, network->secret_length);
    }
    if (refusal != NULL) {
        *refusal = status;
    }
    if (status != CHORUS_PROVISION_FORM_OK) {
        if (p->state == CHORUS_PROVISION_AP_UP) {
            set_reason(p, chorus_provision_form_status_name(status));
        }
        say(p, "provision: form refused reason=%s", chorus_provision_form_status_name(status));
        return p->state;
    }
    enter(p, CHORUS_PROVISION_CREDENTIALS_RECEIVED);
    say(p, "provision: credentials received ssid_bytes=%u secret_bytes=%u",
        (unsigned)network->ssid_length, (unsigned)network->secret_length);
    hold_network(p, network);
    if (!join_once(p)) {
        forget_network(p);
        return raise_ap(p, now_ms);
    }
    if (!keep_network(p)) {
        /* On the network, and unable to remember it: said as a failure, with
         * the access point left up, because the next boot would not join. */
        set_reason(p, "credentials-not-stored");
        enter(p, CHORUS_PROVISION_JOIN_FAILED);
        say(p, "provision: join failed reason=%s", p->reason);
        forget_network(p);
        return raise_ap(p, now_ms);
    }
    say(p, "provision: credentials stored");
    drop_ap(p);
    enter(p, CHORUS_PROVISION_JOINED);
    say(p, "provision: joined");
    return p->state;
}

chorus_provision_state_t chorus_provision_submit_form(chorus_provision_t *p, const char *body,
                                                      size_t length, uint64_t now_ms,
                                                      chorus_provision_form_status_t *refusal)
{
    chorus_provision_credentials_t network;
    chorus_provision_form_status_t status = chorus_provision_parse_form(body, length, &network);
    if (status != CHORUS_PROVISION_FORM_OK) {
        if (refusal != NULL) {
            *refusal = status;
        }
        if (p->state == CHORUS_PROVISION_AP_UP) {
            set_reason(p, chorus_provision_form_status_name(status));
        }
        say(p, "provision: form refused reason=%s", chorus_provision_form_status_name(status));
        return p->state;
    }
    chorus_provision_state_t state = chorus_provision_submit(p, &network, now_ms, refusal);
    memset(&network, 0, sizeof(network));
    return state;
}

chorus_provision_state_t chorus_provision_tick(chorus_provision_t *p, uint64_t now_ms)
{
    if (p->state != CHORUS_PROVISION_AP_UP || !p->stored || now_ms < p->rejoin_at_ms) {
        return p->state;
    }
    chorus_provision_credentials_t network;
    if (load_network(p, &network) != 1) {
        p->stored = 0;
        return p->state;
    }
    say(p, "provision: trying the stored network again");
    hold_network(p, &network);
    memset(&network, 0, sizeof(network));
    if (!join_once(p)) {
        forget_network(p);
        return raise_ap(p, now_ms);
    }
    drop_ap(p);
    enter(p, CHORUS_PROVISION_JOINED);
    say(p, "provision: joined");
    return p->state;
}

int chorus_provision_reset(chorus_provision_t *p)
{
    if (p->state == CHORUS_PROVISION_WIRED) {
        return -1;
    }
    chorus_store_status_t ssid = chorus_store_erase(p->store, CHORUS_PROVISION_KEY_SSID);
    chorus_store_status_t secret = chorus_store_erase(p->store, CHORUS_PROVISION_KEY_SECRET);
    forget_network(p);
    if (ssid != CHORUS_STORE_OK || secret != CHORUS_STORE_OK) {
        say(p, "provision: reset failed, the store would not erase");
        return -1;
    }
    p->stored = 0;
    p->reason[0] = '\0';
    enter(p, CHORUS_PROVISION_RESET);
    say(p, "provision: reset, the stored network is erased");
    enter(p, CHORUS_PROVISION_UNPROVISIONED);
    if (p->ap_up) {
        enter(p, CHORUS_PROVISION_AP_UP);
    }
    return 0;
}

int chorus_provision_export(const chorus_provision_t *p, chorus_wifi_config_t *link)
{
    if (p->state != CHORUS_PROVISION_JOINED || !p->wifi.ssid_known || !p->wifi.secret_known) {
        return -1;
    }
    memcpy(link->ssid, p->wifi.ssid, sizeof(link->ssid));
    memcpy(link->secret, p->wifi.secret, sizeof(link->secret));
    link->ssid_known = 1;
    link->secret_known = 1;
    return 0;
}

const char *chorus_provision_ap_name(const chorus_provision_t *p)
{
    return p->ap_name;
}

const char *chorus_provision_setup_secret(const chorus_provision_t *p)
{
    return p->setup_secret;
}

const char *chorus_provision_reason(const chorus_provision_t *p)
{
    return p->reason;
}

/* --- the pages ---------------------------------------------------------------
 *
 * Plain HTML, no script, no external reference: the phone is on the speaker's
 * own access point and can reach nothing else. The only variable text is a
 * refusal's NAME, which is one of this unit's own words (or the platform's
 * join word, which is filtered to [a-z0-9-] below), so nothing a phone sent
 * is ever echoed into a page. */

static const char PAGE_HEAD[] =
    "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">"
    "<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">"
    "<title>chorus speaker setup</title>"
    "<style>body{font-family:sans-serif;margin:1.5em;max-width:28em}"
    "label{display:block;margin-top:1em}input{width:100%;font-size:1em;padding:.4em;"
    "box-sizing:border-box}button{margin-top:1.2em;font-size:1em;padding:.5em 1.2em}"
    ".why{border:1px solid;padding:.6em;margin-top:1em}</style></head><body>"
    "<h1>chorus speaker setup</h1>";

static const char PAGE_FORM[] =
    "<p>Give this speaker the Wi-Fi network it should join.</p>"
    "<form method=\"post\" action=\"/join\">"
    "<label>Network name<input name=\"ssid\" maxlength=\"32\" required "
    "autocapitalize=\"none\" autocorrect=\"off\"></label>"
    "<label>Passphrase<input name=\"secret\" type=\"password\" minlength=\"8\" "
    "maxlength=\"64\" required></label>"
    "<button type=\"submit\">Join</button></form>";

static const char PAGE_TAIL[] = "</body></html>";

/* A reason is shown only when it is a word: lower-case letters, digits and
 * hyphens. Anything else is replaced, never echoed. */
static const char *safe_word(const char *word)
{
    if (word[0] == '\0') {
        return "";
    }
    for (const char *c = word; *c != '\0'; c++) {
        int ok = (*c >= 'a' && *c <= 'z') || (*c >= '0' && *c <= '9') || *c == '-';
        if (!ok) {
            return "unnamed";
        }
    }
    return word;
}

static size_t finish(int written, size_t capacity)
{
    if (written < 0 || (size_t)written >= capacity) {
        return 0;
    }
    return (size_t)written;
}

size_t chorus_provision_page(const chorus_provision_t *p, char *out, size_t capacity)
{
    const char *why = safe_word(p->reason);
    int written;
    if (why[0] != '\0') {
        written = snprintf(out, capacity,
                           "%s<p class=\"why\">The last try did not work: <b>%s</b>. Check the "
                           "network name and passphrase and try again.</p>%s%s",
                           PAGE_HEAD, why, PAGE_FORM, PAGE_TAIL);
    } else {
        written = snprintf(out, capacity, "%s%s%s", PAGE_HEAD, PAGE_FORM, PAGE_TAIL);
    }
    return finish(written, capacity);
}

size_t chorus_provision_reply_page(chorus_provision_form_status_t status, char *out,
                                   size_t capacity)
{
    int written;
    if (status == CHORUS_PROVISION_FORM_OK) {
        written = snprintf(out, capacity,
                           "%s<p>Received. The speaker is joining that network now.</p>"
                           "<p>If it joins, this setup network disappears and the speaker shows "
                           "up in chorus. If it cannot, this setup network stays: join it again "
                           "and reload this page to see why.</p>%s",
                           PAGE_HEAD, PAGE_TAIL);
    } else {
        written = snprintf(out, capacity,
                           "%s<p class=\"why\">Not accepted: <b>%s</b>.</p>"
                           "<p><a href=\"/\">Back to the form</a></p>%s",
                           PAGE_HEAD, chorus_provision_form_status_name(status), PAGE_TAIL);
    }
    return finish(written, capacity);
}
