/* The endpoint's DNS-SD browse (goal 14, chorus/discovery.h).
 *
 * The bytes half reads the SAME committed packets as the Rust crate
 * (fixtures/discovery, crates/discovery/tests/dnssd_vectors.rs): every `.params`
 * of kind `query` must be produced byte for byte, and every `.hex` with an
 * `.expected` beside it must resolve to exactly what that file says. The
 * directory is walked, not listed: a vector added there is graded here with
 * no registration, and one this reader cannot read fails.
 *
 * The rest needs no socket either: the browse and the fallback order run over
 * a link that is a list of datagrams, and over the fake store. Whether real
 * multicast carries any of it is `make verify-endpoint-mdns`, which refuses by
 * name where the link does not. */

#include "chorus/discovery.h"
#include "chorus/identity.h"
#include "fake_store.h"
#include "fixture_text.h"
#include "harness.h"

#include <dirent.h>
#include <inttypes.h>

#define AUDIO CHORUS_DISCOVERY_AUDIO_SERVICE
#define CONTROL "_chorus-ctl._tcp.local."
#define MAX_PACKET 2048

static void fixture_path(char *out, size_t cap, const char *stem, const char *extension)
{
    char relative[256];
    snprintf(relative, sizeof(relative), "fixtures/discovery/%.95s.%.16s", stem, extension);
    chorus_repo_path(out, cap, relative);
}

static long read_packet(const char *stem, uint8_t *out, size_t cap)
{
    char path[512];
    static char text[16384];
    fixture_path(path, sizeof(path), stem, "hex");
    if (fixture_read(path, text, sizeof(text)) < 0) {
        return -1;
    }
    return fixture_parse_hex(text, out, cap);
}

/* Every vector's stem, from the directory: the names of its `.params` files,
 * sorted. */
#define MAX_STEMS 32
static char stems[MAX_STEMS][96];
static size_t stem_count;
static size_t unread_files;

static int by_name(const void *a, const void *b)
{
    return strcmp((const char *)a, (const char *)b);
}

static void walk_the_directory(void)
{
    char path[512];
    chorus_repo_path(path, sizeof(path), "fixtures/discovery");
    DIR *dir = opendir(path);
    stem_count = 0;
    unread_files = 0;
    if (dir == NULL) {
        return;
    }
    struct dirent *entry;
    while ((entry = readdir(dir)) != NULL) {
        const char *dot = strrchr(entry->d_name, '.');
        if (entry->d_name[0] == '.' || dot == NULL) {
            continue;
        }
        if (strcmp(dot, ".params") == 0 && stem_count < MAX_STEMS &&
            (size_t)(dot - entry->d_name) < sizeof(stems[0])) {
            memcpy(stems[stem_count], entry->d_name, (size_t)(dot - entry->d_name));
            stems[stem_count][dot - entry->d_name] = '\0';
            stem_count++;
        } else if (strcmp(dot, ".params") != 0 && strcmp(dot, ".hex") != 0 &&
                   strcmp(dot, ".expected") != 0) {
            unread_files++;
        }
    }
    closedir(dir);
    qsort(stems, stem_count, sizeof(stems[0]), by_name);
}

static void a_browse_query_is_the_committed_packet(void)
{
    chorus_section("the browse query, byte for byte (fixtures/discovery, kind = query)");
    chorus_check(stem_count > 0 && unread_files == 0,
                 "fixtures/discovery holds %zu vectors and no file of a kind this reader does "
                 "not read",
                 stem_count);
    size_t ran = 0;
    for (size_t i = 0; i < stem_count; i++) {
        char path[512];
        static char params[8192];
        char kind[32];
        char service[128];
        fixture_path(path, sizeof(path), stems[i], "params");
        if (fixture_read(path, params, sizeof(params)) < 0 ||
            fixture_field(params, "kind", kind, sizeof(kind)) == NULL) {
            chorus_check(0, "%s.params is readable and names its kind", stems[i]);
            continue;
        }
        if (strcmp(kind, "query") != 0) {
            continue;
        }
        uint8_t want[MAX_PACKET];
        uint8_t made[MAX_PACKET];
        long want_len = read_packet(stems[i], want, sizeof(want));
        long made_len = (fixture_field(params, "service", service, sizeof(service)) == NULL)
                            ? -1
                            : chorus_discovery_browse_query(service, 0, made, sizeof(made));
        chorus_check(
            want_len > 0 && made_len == want_len && memcmp(want, made, (size_t)want_len) == 0,
            "%s.hex: the query for %s is the committed %ld bytes", stems[i], service, want_len);
        ran++;

        /* The form the browse really sends differs in one bit and no byte
         * else: the unicast-response bit of the question's class. */
        uint8_t unicast[MAX_PACKET];
        long unicast_len = chorus_discovery_browse_query(service, 1, unicast, sizeof(unicast));
        int one_bit = unicast_len == want_len && want_len > 2;
        for (long b = 0; one_bit && b < want_len; b++) {
            uint8_t expect = (b == want_len - 2) ? (uint8_t)(want[b] | 0x80u) : want[b];
            one_bit = unicast[b] == expect;
        }
        chorus_check(one_bit,
                     "%s: with the unicast-response bit it is the same packet but for that bit",
                     stems[i]);
    }
    chorus_check(ran == 2, "both service types have a committed query vector (%zu ran)", ran);

    uint8_t small[20];
    char long_label[80];
    memset(long_label, 'a', sizeof(long_label) - 1);
    long_label[sizeof(long_label) - 1] = '\0';
    uint8_t out[MAX_PACKET];
    chorus_check(chorus_discovery_browse_query(AUDIO, 0, small, sizeof(small)) == -1,
                 "a buffer the query does not fit is refused, not overrun");
    chorus_check(chorus_discovery_browse_query(long_label, 0, out, sizeof(out)) == -1 &&
                     chorus_discovery_browse_query("a..b.", 0, out, sizeof(out)) == -1 &&
                     chorus_discovery_browse_query("a..", 0, out, sizeof(out)) == -1,
                 "a service type with a label over 63 bytes or an empty label does not encode");
}

/* The n-th `instance.<index>.txt.<key> = value` line of an `.expected` text.
 * 1 with the key and value, 0 past the last. */
static int expected_txt(const char *text, unsigned index, size_t nth, char *key, size_t key_cap,
                        char *value, size_t value_cap)
{
    char prefix[48];
    snprintf(prefix, sizeof(prefix), "instance.%u.txt.", index);
    size_t prefix_len = strlen(prefix);
    size_t seen = 0;
    const char *line = text;
    while (*line != '\0') {
        const char *eol = strchr(line, '\n');
        size_t len = (eol == NULL) ? strlen(line) : (size_t)(eol - line);
        if (len > prefix_len && strncmp(line, prefix, prefix_len) == 0) {
            const char *equals = memchr(line, '=', len);
            if (equals != NULL && seen++ == nth) {
                size_t key_len = (size_t)(equals - (line + prefix_len));
                while (key_len > 0 && line[prefix_len + key_len - 1] == ' ') {
                    key_len--;
                }
                if (key_len + 1 > key_cap) {
                    return 0;
                }
                memcpy(key, line + prefix_len, key_len);
                key[key_len] = '\0';
                char full[160];
                snprintf(full, sizeof(full), "%.47s%.95s", prefix, key);
                return fixture_field(text, full, value, value_cap) != NULL;
            }
        }
        line = (eol == NULL) ? line + len : eol + 1;
    }
    return 0;
}

static void every_committed_response_resolves_to_its_committed_answer(void)
{
    chorus_section("resolving every committed response packet (fixtures/discovery, .expected)");
    size_t ran = 0;
    for (size_t i = 0; i < stem_count; i++) {
        char path[512];
        static char expected[8192];
        fixture_path(path, sizeof(path), stems[i], "expected");
        if (fixture_read(path, expected, sizeof(expected)) < 0) {
            continue;
        }
        uint8_t packet[MAX_PACKET];
        long packet_len = read_packet(stems[i], packet, sizeof(packet));
        char service[128];
        char value[256];
        static chorus_discovery_service_t found[CHORUS_DISCOVERY_MAX_INSTANCES];
        size_t count = 0;
        chorus_discovery_status_t status =
            (packet_len < 0 || fixture_field(expected, "service", service, sizeof(service)) == NULL)
                ? CHORUS_DISCOVERY_NOT_A_MESSAGE
                : chorus_discovery_resolve(packet, (size_t)packet_len, service, found,
                                           CHORUS_DISCOVERY_MAX_INSTANCES, &count);
        size_t wanted = (fixture_field(expected, "instances", value, sizeof(value)) == NULL)
                            ? (size_t)-1
                            : (size_t)strtoul(value, NULL, 10);
        chorus_check(status == CHORUS_DISCOVERY_OK && count == wanted,
                     "%s.hex (%ld bytes) resolves: %zu instance(s) of %s", stems[i], packet_len,
                     count, service);
        for (unsigned n = 0; status == CHORUS_DISCOVERY_OK && n < count && n < wanted; n++) {
            char key[64];
            char address[CHORUS_DISCOVERY_ADDRESS_MAX] = "";
            char port[16];
            snprintf(port, sizeof(port), "%u", (unsigned)found[n].port);
            (void)chorus_discovery_socket_address(&found[n], address, sizeof(address));
            const struct {
                const char *field;
                const char *got;
            } fields[] = {{"name", found[n].instance},
                          {"label", found[n].label},
                          {"host", found[n].host},
                          {"port", port},
                          {"address", address}};
            for (size_t f = 0; f < sizeof(fields) / sizeof(fields[0]); f++) {
                snprintf(key, sizeof(key), "instance.%u.%s", n, fields[f].field);
                const char *want = fixture_field(expected, key, value, sizeof(value));
                chorus_check(want != NULL && strcmp(want, fields[f].got) == 0, "%s: %s = %s",
                             stems[i], key, fields[f].got);
            }
            char txt_key[64];
            char got[128];
            size_t txt_checked = 0;
            for (size_t t = 0;
                 expected_txt(expected, n, t, txt_key, sizeof(txt_key), value, sizeof(value));
                 t++) {
                int have = chorus_discovery_txt_value(&found[n], txt_key, got, sizeof(got));
                chorus_check(have && strcmp(got, value) == 0, "%s: instance.%u.txt.%s = %s",
                             stems[i], n, txt_key, have ? got : "(absent)");
                txt_checked++;
            }
            chorus_check(txt_checked > 0 && found[n].txt_dropped == 0,
                         "%s: instance %u's TXT record was read whole (%zu keys graded)", stems[i],
                         n, txt_checked);
        }
        ran++;
    }
    chorus_check(ran >= 3, "at least three response vectors ran (%zu)", ran);
}

static int same_service(const chorus_discovery_service_t *a, const chorus_discovery_service_t *b)
{
    return strcmp(a->instance, b->instance) == 0 && strcmp(a->label, b->label) == 0 &&
           strcmp(a->host, b->host) == 0 && a->port == b->port &&
           a->have_address == b->have_address && memcmp(a->address, b->address, 4) == 0 &&
           a->txt_len == b->txt_len && memcmp(a->txt, b->txt, a->txt_len) == 0;
}

static void compression_and_case_do_not_change_the_answer(void)
{
    chorus_section("the same advertisement written two ways");
    uint8_t plain[MAX_PACKET];
    uint8_t compressed[MAX_PACKET];
    long plain_len = read_packet("advertisement-audio", plain, sizeof(plain));
    long compressed_len =
        read_packet("advertisement-audio-compressed", compressed, sizeof(compressed));
    static chorus_discovery_service_t a;
    static chorus_discovery_service_t b;
    size_t count_a = 0;
    size_t count_b = 0;
    chorus_discovery_status_t status_a =
        chorus_discovery_resolve(plain, (size_t)plain_len, AUDIO, &a, 1, &count_a);
    chorus_discovery_status_t status_b =
        chorus_discovery_resolve(compressed, (size_t)compressed_len, AUDIO, &b, 1, &count_b);
    chorus_check(compressed_len > 0 && compressed_len < plain_len,
                 "the compressed vector is smaller (%ld bytes against %ld)", compressed_len,
                 plain_len);
    chorus_check(status_a == CHORUS_DISCOVERY_OK && status_b == CHORUS_DISCOVERY_OK &&
                     count_a == 1 && count_b == 1 && same_service(&a, &b),
                 "and both resolve to the same instance, host, port, address and TXT");

    /* A responder that capitalises differently is the same responder. */
    uint8_t shouted[MAX_PACKET];
    memcpy(shouted, plain, (size_t)plain_len);
    for (long i = 12; i < plain_len; i++) {
        if (shouted[i] >= 'a' && shouted[i] <= 'z') {
            shouted[i] = (uint8_t)(shouted[i] - 'a' + 'A');
        }
    }
    size_t count = 0;
    static chorus_discovery_service_t c;
    chorus_discovery_status_t status =
        chorus_discovery_resolve(shouted, (size_t)plain_len, AUDIO, &c, 1, &count);
    chorus_check(status == CHORUS_DISCOVERY_OK && count == 1 && c.port == a.port &&
                     memcmp(c.address, a.address, 4) == 0,
                 "names are matched case-insensitively: the upper-cased packet resolves too (%s)",
                 c.instance);

    status = chorus_discovery_resolve(plain, (size_t)plain_len, CONTROL, &c, 1, &count);
    chorus_check(status == CHORUS_DISCOVERY_OK && count == 0,
                 "a response for another service type resolves to nothing");
    char got[64];
    chorus_check(chorus_discovery_txt_value(&a, "CTL", got, sizeof(got)) == 1 &&
                     strcmp(got, "4020") == 0 &&
                     chorus_discovery_txt_value(&a, "absent", got, sizeof(got)) == 0 &&
                     chorus_discovery_txt_value(&a, "group", got, 4) == 0,
                 "a TXT key is found whatever its case, an absent one is absent, and a value "
                 "that does not fit is not cut");
}

static void a_packet_that_is_not_a_message_is_refused_not_half_read(void)
{
    chorus_section("packets that are not advertisements");
    static chorus_discovery_service_t found[CHORUS_DISCOVERY_MAX_INSTANCES];
    size_t count = 0;
    size_t cuts = 0;
    size_t dialable = 0;
    size_t corruptions = 0;
    size_t insane = 0;
    for (size_t i = 0; i < stem_count; i++) {
        char path[512];
        static char expected[8192];
        fixture_path(path, sizeof(path), stems[i], "expected");
        if (fixture_read(path, expected, sizeof(expected)) < 0) {
            continue;
        }
        uint8_t good[MAX_PACKET];
        long good_len = read_packet(stems[i], good, sizeof(good));
        /* Every prefix: it either fails to decode or decodes to something
         * with nothing to dial in it. What it must never do is make a host
         * and port out of a message that was cut. */
        for (long cut = 0; cut < good_len; cut++) {
            /* A fresh copy sized to the cut, so a read past it is a read past
             * an allocation and not into the rest of the packet. */
            uint8_t *copy = malloc((size_t)cut + 1);
            if (copy == NULL) {
                chorus_check(0, "a %ld-byte copy could not be allocated", cut);
                break;
            }
            memcpy(copy, good, (size_t)cut);
            if (chorus_discovery_resolve(copy, (size_t)cut, AUDIO, found,
                                         CHORUS_DISCOVERY_MAX_INSTANCES,
                                         &count) == CHORUS_DISCOVERY_OK) {
                for (size_t n = 0; n < count && n < CHORUS_DISCOVERY_MAX_INSTANCES; n++) {
                    dialable += found[n].have_address;
                }
            }
            free(copy);
            cuts++;
        }
        /* Every single-byte corruption: any verdict is allowed, a crash or an
         * instance that is not NUL-terminated text is not. */
        for (long at = 0; at < good_len; at++) {
            for (unsigned flip = 0; flip < 3; flip++) {
                static const uint8_t MASKS[] = {0xFF, 0x80, 0x01};
                uint8_t bad[MAX_PACKET];
                memcpy(bad, good, (size_t)good_len);
                bad[at] ^= MASKS[flip];
                if (chorus_discovery_resolve(bad, (size_t)good_len, AUDIO, found,
                                             CHORUS_DISCOVERY_MAX_INSTANCES,
                                             &count) == CHORUS_DISCOVERY_OK) {
                    for (size_t n = 0; n < count && n < CHORUS_DISCOVERY_MAX_INSTANCES; n++) {
                        insane +=
                            memchr(found[n].instance, '\0', sizeof(found[n].instance)) == NULL ||
                            memchr(found[n].host, '\0', sizeof(found[n].host)) == NULL ||
                            found[n].txt_len > sizeof(found[n].txt);
                    }
                }
                corruptions++;
            }
        }
    }
    chorus_check(cuts > 400 && dialable == 0,
                 "%zu truncations of the committed advertisements: none produced an address to "
                 "dial",
                 cuts);
    chorus_check(corruptions > 1200 && insane == 0,
                 "%zu single-byte corruptions: every one was read safely", corruptions);

    uint8_t zeros[4] = {0, 0, 0, 0};
    chorus_check(chorus_discovery_resolve(zeros, sizeof(zeros), AUDIO, found, 1, &count) ==
                     CHORUS_DISCOVERY_NOT_A_MESSAGE,
                 "four bytes are not a DNS message");
    uint8_t query[MAX_PACKET];
    long query_len = chorus_discovery_browse_query(AUDIO, 0, query, sizeof(query));
    chorus_check(chorus_discovery_resolve(query, (size_t)query_len, AUDIO, found, 1, &count) ==
                     CHORUS_DISCOVERY_A_QUERY,
                 "a query is not an answer: %s",
                 chorus_discovery_status_name(CHORUS_DISCOVERY_A_QUERY));
    uint8_t plain[MAX_PACKET];
    long plain_len = read_packet("advertisement-audio", plain, sizeof(plain));
    chorus_check(chorus_discovery_resolve(plain, (size_t)plain_len, "a..b", found, 1, &count) ==
                     CHORUS_DISCOVERY_BAD_SERVICE,
                 "a service type that is not a name is refused by name");
}

/* --- packets built here, for the cases no committed vector holds -------------- */

typedef struct {
    uint8_t bytes[MAX_PACKET];
    size_t length;
    unsigned records;
} builder_t;

static void put_name(builder_t *b, const char *dotted)
{
    const char *at = dotted;
    while (*at != '\0') {
        const char *dot = strchr(at, '.');
        size_t len = (dot == NULL) ? strlen(at) : (size_t)(dot - at);
        if (len > 0) {
            b->bytes[b->length++] = (uint8_t)len;
            memcpy(b->bytes + b->length, at, len);
            b->length += len;
        }
        at += len + ((dot == NULL) ? 0 : 1);
    }
    b->bytes[b->length++] = 0;
}

static void put_u16(builder_t *b, unsigned value)
{
    b->bytes[b->length++] = (uint8_t)(value >> 8);
    b->bytes[b->length++] = (uint8_t)(value & 0xFF);
}

static void start(builder_t *b)
{
    memset(b, 0, sizeof(*b));
    b->bytes[2] = 0x84; /* a response, authoritative */
    b->length = 12;
}

/* A record header; returns where its data length goes. */
static size_t begin_record(builder_t *b, const char *name, unsigned type)
{
    put_name(b, name);
    put_u16(b, type);
    put_u16(b, 0x8001);
    put_u16(b, 0);
    put_u16(b, 120);
    size_t length_at = b->length;
    put_u16(b, 0);
    b->records++;
    return length_at;
}

static void end_record(builder_t *b, size_t length_at)
{
    size_t data = b->length - length_at - 2;
    b->bytes[length_at] = (uint8_t)(data >> 8);
    b->bytes[length_at + 1] = (uint8_t)(data & 0xFF);
    b->bytes[6] = (uint8_t)(b->records >> 8);
    b->bytes[7] = (uint8_t)(b->records & 0xFF);
}

static void put_ptr(builder_t *b, const char *service, const char *instance)
{
    size_t at = begin_record(b, service, 12);
    put_name(b, instance);
    end_record(b, at);
}

static void put_srv(builder_t *b, const char *instance, unsigned port, const char *host)
{
    size_t at = begin_record(b, instance, 33);
    put_u16(b, 0);
    put_u16(b, 0);
    put_u16(b, port);
    put_name(b, host);
    end_record(b, at);
}

static void put_a(builder_t *b, const char *host, unsigned last)
{
    size_t at = begin_record(b, host, 1);
    b->bytes[b->length++] = 192;
    b->bytes[b->length++] = 0;
    b->bytes[b->length++] = 2;
    b->bytes[b->length++] = (uint8_t)last;
    end_record(b, at);
}

static void put_raw(builder_t *b, const char *name, unsigned type, const uint8_t *data, size_t len)
{
    size_t at = begin_record(b, name, type);
    memcpy(b->bytes + b->length, data, len);
    b->length += len;
    end_record(b, at);
}

static void what_a_resolver_leaves_out_and_keeps(void)
{
    chorus_section("partial, repeated and crowded advertisements");
    static builder_t b;
    static chorus_discovery_service_t found[CHORUS_DISCOVERY_MAX_INSTANCES];
    size_t count = 0;
    char address[CHORUS_DISCOVERY_ADDRESS_MAX];

    start(&b);
    put_ptr(&b, AUDIO, "ghost." AUDIO);
    put_a(&b, "ghost.local.", 50);
    chorus_discovery_status_t status =
        chorus_discovery_resolve(b.bytes, b.length, AUDIO, found, 4, &count);
    chorus_check(status == CHORUS_DISCOVERY_OK && count == 0,
                 "an instance with no SRV record is left out rather than guessed");

    start(&b);
    put_ptr(&b, AUDIO, "far." AUDIO);
    put_srv(&b, "far." AUDIO, 4010, "far.local.");
    status = chorus_discovery_resolve(b.bytes, b.length, AUDIO, found, 4, &count);
    chorus_check(status == CHORUS_DISCOVERY_OK && count == 1 && found[0].have_address == 0 &&
                     strcmp(found[0].host, "far.local.") == 0 && found[0].port == 4010 &&
                     chorus_discovery_socket_address(&found[0], address, sizeof(address)) == 0,
                 "an instance whose host has no address is returned with its host name and "
                 "nothing to dial");

    start(&b);
    put_ptr(&b, AUDIO, "twice." AUDIO);
    put_ptr(&b, AUDIO, "TWICE." AUDIO);
    put_srv(&b, "twice." AUDIO, 4010, "twice.local.");
    put_a(&b, "twice.local.", 51);
    status = chorus_discovery_resolve(b.bytes, b.length, AUDIO, found, 4, &count);
    chorus_check(status == CHORUS_DISCOVERY_OK && count == 1,
                 "an instance named by two PTR records is one instance");

    static const uint8_t V6[16] = {0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1};
    start(&b);
    put_ptr(&b, AUDIO, "dual." AUDIO);
    put_srv(&b, "dual." AUDIO, 4010, "dual.local.");
    put_raw(&b, "dual.local.", 28, V6, sizeof(V6));
    put_a(&b, "dual.local.", 52);
    status = chorus_discovery_resolve(b.bytes, b.length, AUDIO, found, 4, &count);
    chorus_check(status == CHORUS_DISCOVERY_OK && count == 1 &&
                     chorus_discovery_socket_address(&found[0], address, sizeof(address)) == 1 &&
                     strcmp(address, "192.0.2.52:4010") == 0,
                 "a host with an AAAA and an A record is dialled at its IPv4 address: %s", address);

    start(&b);
    for (unsigned i = 0; i < 6; i++) {
        char instance[96];
        char host[32];
        snprintf(instance, sizeof(instance), "room%u." AUDIO, i);
        snprintf(host, sizeof(host), "room%u.local.", i);
        put_ptr(&b, AUDIO, instance);
        put_srv(&b, instance, 4010 + i, host);
        put_a(&b, host, 60 + i);
    }
    status = chorus_discovery_resolve(b.bytes, b.length, AUDIO, found, 4, &count);
    chorus_check(status == CHORUS_DISCOVERY_OK && count == 6 &&
                     strcmp(found[0].label, "room0") == 0 && strcmp(found[3].label, "room3") == 0 &&
                     found[3].port == 4013,
                 "six instances in one packet: all six counted, the first four kept, in the "
                 "order their PTR records came");

    /* A TXT record larger than an instance keeps: whole strings only. */
    uint8_t txt[600];
    size_t txt_len = 0;
    for (unsigned i = 0; i < 5; i++) {
        txt[txt_len++] = 100;
        memset(txt + txt_len, 'a' + (int)i, 100);
        txt[txt_len + 1] = '=';
        txt_len += 100;
    }
    start(&b);
    put_ptr(&b, AUDIO, "wordy." AUDIO);
    put_srv(&b, "wordy." AUDIO, 4010, "wordy.local.");
    put_raw(&b, "wordy." AUDIO, 16, txt, txt_len);
    status = chorus_discovery_resolve(b.bytes, b.length, AUDIO, found, 4, &count);
    char value[128];
    chorus_check(status == CHORUS_DISCOVERY_OK && count == 1 && found[0].txt_len == 202 &&
                     found[0].txt_dropped == 3 &&
                     chorus_discovery_txt_value(&found[0], "b", value, sizeof(value)) == 1 &&
                     strlen(value) == 98 &&
                     chorus_discovery_txt_value(&found[0], "c", value, sizeof(value)) == 0,
                 "a TXT record past what an instance keeps: two whole strings kept, three counted "
                 "as dropped, none cut in half");
}

static void hostile_names_and_records_are_refused(void)
{
    chorus_section("messages built to make a resolver loop or overrun");
    static builder_t b;
    static chorus_discovery_service_t found[2];
    size_t count = 0;

    struct {
        const char *what;
        uint8_t name[8];
        size_t name_len;
    } names[] = {
        {"a compression pointer that points at itself", {0xC0, 12}, 2},
        {"a compression pointer that points forwards", {0xC0, 40}, 2},
        {"a compression pointer one byte short", {0xC0}, 1},
        {"a label length with reserved bits", {0x80, 'a', 0}, 3},
        {"a label that runs past the message", {0x3F, 'a'}, 2},
        {"a name with no end", {0x01, 'a'}, 2},
    };
    for (size_t i = 0; i < sizeof(names) / sizeof(names[0]); i++) {
        uint8_t packet[64];
        memset(packet, 0, sizeof(packet));
        packet[2] = 0x84;
        packet[7] = 1;
        memcpy(packet + 12, names[i].name, names[i].name_len);
        chorus_check(chorus_discovery_resolve(packet, 12 + names[i].name_len, AUDIO, found, 2,
                                              &count) == CHORUS_DISCOVERY_NOT_A_MESSAGE &&
                         count == 0,
                     "%s is refused", names[i].what);
    }

    /* Two pointers that point at each other: backwards-only stops the second. */
    uint8_t loop[32];
    memset(loop, 0, sizeof(loop));
    loop[2] = 0x84;
    loop[7] = 1;
    loop[12] = 0xC0;
    loop[13] = 14;
    loop[14] = 0xC0;
    loop[15] = 12;
    chorus_check(chorus_discovery_resolve(loop, 26, AUDIO, found, 2, &count) ==
                     CHORUS_DISCOVERY_NOT_A_MESSAGE,
                 "two pointers that point at each other are refused, not followed");

    /* A name over the 255-byte limit, reached through pointers. */
    start(&b);
    {
        size_t at = b.length;
        for (unsigned i = 0; i < 5; i++) {
            b.bytes[b.length++] = 63;
            memset(b.bytes + b.length, 'x', 63);
            b.length += 63;
        }
        b.bytes[b.length++] = 0;
        (void)at;
        b.bytes[7] = 1;
    }
    chorus_check(chorus_discovery_resolve(b.bytes, b.length + 10, AUDIO, found, 2, &count) ==
                     CHORUS_DISCOVERY_NOT_A_MESSAGE,
                 "a name of 321 encoded bytes is over the DNS limit and refused");

    static const struct {
        const char *what;
        unsigned type;
        uint8_t data[8];
        size_t len;
    } records[] = {
        {"an A record of five bytes", 1, {192, 0, 2, 1, 9}, 5},
        {"an A record of three bytes", 1, {192, 0, 2}, 3},
        {"an AAAA record of four bytes", 28, {1, 2, 3, 4}, 4},
        {"an SRV record shorter than its fixed part", 33, {0, 0, 0, 0, 15}, 5},
        {"a TXT string that runs past its record", 16, {9, 'v', '=', '1'}, 4},
        {"a PTR whose target is not a name", 12, {0x80}, 1},
    };
    for (size_t i = 0; i < sizeof(records) / sizeof(records[0]); i++) {
        start(&b);
        put_ptr(&b, AUDIO, "ok." AUDIO);
        put_srv(&b, "ok." AUDIO, 4010, "ok.local.");
        put_a(&b, "ok.local.", 70);
        put_raw(&b, "other.local.", records[i].type, records[i].data, records[i].len);
        chorus_discovery_status_t status =
            chorus_discovery_resolve(b.bytes, b.length, AUDIO, found, 2, &count);
        chorus_check(status == CHORUS_DISCOVERY_NOT_A_MESSAGE && count == 0,
                     "%s refuses the whole packet, good records and all", records[i].what);
    }

    start(&b);
    put_ptr(&b, AUDIO, "ok." AUDIO);
    b.bytes[7] = 3; /* the header promises records the packet does not hold */
    chorus_check(chorus_discovery_resolve(b.bytes, b.length, AUDIO, found, 2, &count) ==
                     CHORUS_DISCOVERY_NOT_A_MESSAGE,
                 "a header that counts more records than the packet holds is refused");

    /* A line break in a name does not reach a log as a line break. */
    start(&b);
    put_ptr(&b, AUDIO, "a\nb." AUDIO);
    put_srv(&b, "a\nb." AUDIO, 4010, "ok.local.");
    chorus_discovery_status_t status =
        chorus_discovery_resolve(b.bytes, b.length, AUDIO, found, 2, &count);
    chorus_check(status == CHORUS_DISCOVERY_OK && count == 1 &&
                     strcmp(found[0].label, "a?b") == 0 && strchr(found[0].instance, '\n') == NULL,
                 "a control byte in an instance name is written as `?`: %s", found[0].instance);
}

/* --- a link that is a list of datagrams --------------------------------------- */

#define SCRIPT_MAX 48

typedef struct {
    /* What each receive returns, in order: a packet, 0 (nothing in the
     * wait), or -1 (the socket failed). Past the end: nothing. */
    struct {
        const uint8_t *bytes;
        long length;
    } script[SCRIPT_MAX];
    size_t script_len;
    size_t next;
    /* What the unit did, which it cannot reach. */
    size_t sends;
    size_t receives;
    uint64_t waited_ms;
    uint8_t last_query[512];
    size_t last_query_len;
    int send_fails;
    /* After this many sends, every further send fails. Zero: never. */
    size_t sends_before_failing;
} fake_link_t;

static int fake_send(void *context, const uint8_t *query, size_t length)
{
    fake_link_t *fake = (fake_link_t *)context;
    if (fake->send_fails ||
        (fake->sends_before_failing > 0 && fake->sends >= fake->sends_before_failing)) {
        return -1;
    }
    fake->sends++;
    fake->last_query_len = (length < sizeof(fake->last_query)) ? length : sizeof(fake->last_query);
    memcpy(fake->last_query, query, fake->last_query_len);
    return 0;
}

static long fake_receive(void *context, uint8_t *out, size_t capacity, uint32_t wait_ms)
{
    fake_link_t *fake = (fake_link_t *)context;
    fake->receives++;
    if (fake->next >= fake->script_len) {
        fake->waited_ms += wait_ms;
        return 0;
    }
    long length = fake->script[fake->next].length;
    const uint8_t *bytes = fake->script[fake->next].bytes;
    fake->next++;
    if (length <= 0) {
        fake->waited_ms += (length == 0) ? wait_ms : 0;
        return length;
    }
    if ((size_t)length > capacity) {
        length = (long)capacity;
    }
    memcpy(out, bytes, (size_t)length);
    return length;
}

static chorus_discovery_link_t fake_link(fake_link_t *fake)
{
    chorus_discovery_link_t link = {fake, fake_send, fake_receive};
    return link;
}

static void say(fake_link_t *fake, const uint8_t *bytes, long length)
{
    fake->script[fake->script_len].bytes = bytes;
    fake->script[fake->script_len].length = length;
    fake->script_len++;
}

static uint8_t scratch[CHORUS_DISCOVERY_MAX_DATAGRAM];
static uint8_t advert[MAX_PACKET];
static long advert_len;
static uint8_t spare[MAX_PACKET];
static long spare_len;

static void the_browse_over_a_link(void)
{
    chorus_section("the browse, over a link that is a list of datagrams");
    static fake_link_t fake;
    static chorus_discovery_service_t found;
    chorus_discovery_link_t link = fake_link(&fake);
    char address[CHORUS_DISCOVERY_ADDRESS_MAX];

    memset(&fake, 0, sizeof(fake));
    say(&fake, advert, advert_len);
    chorus_browse_status_t status =
        chorus_discovery_browse(&link, AUDIO, 1000, scratch, sizeof(scratch), &found);
    chorus_check(status == CHORUS_BROWSE_FOUND &&
                     chorus_discovery_socket_address(&found, address, sizeof(address)) == 1 &&
                     strcmp(address, "192.0.2.40:4010") == 0,
                 "a server that answers is found: %s at %s", found.instance, address);
    chorus_check(fake.sends == 1 && fake.receives == 1 && fake.waited_ms == 0,
                 "with one query and no waiting past the answer");
    uint8_t want[MAX_PACKET];
    long want_len = chorus_discovery_browse_query(AUDIO, 1, want, sizeof(want));
    uint8_t committed[MAX_PACKET];
    long committed_len = read_packet("browse-query-audio", committed, sizeof(committed));
    chorus_check(
        fake.last_query_len == (size_t)want_len &&
            memcmp(fake.last_query, want, (size_t)want_len) == 0 && committed_len == want_len &&
            memcmp(committed, want, (size_t)want_len - 2) == 0 && (want[want_len - 2] & 0x80u) != 0,
        "the query on the link is the committed query packet with the unicast-response "
        "bit set (%ld bytes)",
        want_len);

    memset(&fake, 0, sizeof(fake));
    status = chorus_discovery_browse(&link, AUDIO, 1000, scratch, sizeof(scratch), &found);
    chorus_check(status == CHORUS_BROWSE_NOTHING && fake.waited_ms == 1000,
                 "a silent link is `nothing found` after the whole window: %" PRIu64 " ms of waits",
                 fake.waited_ms);
    chorus_check(fake.sends == 2,
                 "and the query was sent twice: one lost datagram is not an absent server");

    memset(&fake, 0, sizeof(fake));
    fake.sends_before_failing = 1;
    status = chorus_discovery_browse(&link, AUDIO, 1000, scratch, sizeof(scratch), &found);
    chorus_check(status == CHORUS_BROWSE_NOTHING && fake.waited_ms == 1000,
                 "a second query that could not be sent does not fail the browse the first began");

    memset(&fake, 0, sizeof(fake));
    for (int i = 0; i < 9; i++) {
        say(&fake, NULL, 0);
    }
    say(&fake, advert, advert_len);
    status = chorus_discovery_browse(&link, AUDIO, 1000, scratch, sizeof(scratch), &found);
    chorus_check(status == CHORUS_BROWSE_FOUND && fake.waited_ms == 450,
                 "an answer that comes late in the window is taken when it comes (after %" PRIu64
                 " ms)",
                 fake.waited_ms);

    memset(&fake, 0, sizeof(fake));
    fake.send_fails = 1;
    status = chorus_discovery_browse(&link, AUDIO, 1000, scratch, sizeof(scratch), &found);
    chorus_check(status == CHORUS_BROWSE_LINK_FAILED && fake.receives == 0,
                 "a query that could not be sent is `could not run`, not `nothing found`");
    memset(&fake, 0, sizeof(fake));
    say(&fake, NULL, -1);
    status = chorus_discovery_browse(&link, AUDIO, 1000, scratch, sizeof(scratch), &found);
    chorus_check(status == CHORUS_BROWSE_LINK_FAILED, "and so is a socket that failed");

    /* Port 5353 is a shared channel: other responders' traffic, queries and
     * noise are passed over. */
    static const uint8_t NOISE[] = {0xde, 0xad, 0xbe, 0xef, 1, 2, 3};
    static builder_t partial;
    static builder_t printer;
    start(&partial);
    put_ptr(&partial, AUDIO, "far." AUDIO);
    put_srv(&partial, "far." AUDIO, 4010, "far.local.");
    start(&printer);
    put_ptr(&printer, "_ipp._tcp.local.", "printer._ipp._tcp.local.");
    put_srv(&printer, "printer._ipp._tcp.local.", 631, "printer.local.");
    put_a(&printer, "printer.local.", 99);
    memset(&fake, 0, sizeof(fake));
    say(&fake, NOISE, sizeof(NOISE));
    say(&fake, want, want_len);
    say(&fake, printer.bytes, (long)printer.length);
    say(&fake, partial.bytes, (long)partial.length);
    say(&fake, spare, spare_len);
    say(&fake, advert, advert_len);
    status = chorus_discovery_browse(&link, AUDIO, 1000, scratch, sizeof(scratch), &found);
    chorus_check(status == CHORUS_BROWSE_FOUND && strcmp(found.label, "spare") == 0 &&
                     chorus_discovery_socket_address(&found, address, sizeof(address)) == 1 &&
                     strcmp(address, "192.0.2.41:4110") == 0 && fake.receives == 5,
                 "noise, a query, another service's advertisement and an instance with no address "
                 "are passed over; the FIRST instance with an address is taken: %s at %s",
                 found.instance, address);

    memset(&fake, 0, sizeof(fake));
    for (int i = 0; i < SCRIPT_MAX - 1; i++) {
        say(&fake, printer.bytes, (long)printer.length);
    }
    say(&fake, advert, advert_len);
    status = chorus_discovery_browse(&link, AUDIO, 1000, scratch, sizeof(scratch), &found);
    chorus_check(status == CHORUS_BROWSE_NOTHING && fake.receives == CHORUS_DISCOVERY_MAX_PACKETS,
                 "a link that never falls silent does not hold the browse open: it ends after %u "
                 "datagrams",
                 (unsigned)CHORUS_DISCOVERY_MAX_PACKETS);
}

static void where_the_server_is_in_order(void)
{
    chorus_section("where to connect: discovery, then the last good server, then the static "
                   "address");
    static fake_link_t fake;
    static fake_store_t medium;
    static chorus_located_t located;
    chorus_discovery_link_t link = fake_link(&fake);
    fake_store_init(&medium);
    chorus_store_t store = fake_store_as_store(&medium);
    (void)chorus_identity_server_save(&store, "192.0.2.77:4010");
    char line[512];

    chorus_locate_t request;
    memset(&request, 0, sizeof(request));
    request.link = &link;
    request.service = AUDIO;
    request.window_ms = 300;
    request.store = &store;
    request.configured = "198.51.100.5:4010";

    memset(&fake, 0, sizeof(fake));
    say(&fake, advert, advert_len);
    int status = chorus_discovery_locate(&request, scratch, sizeof(scratch), &located);
    chorus_located_line(&located, line, sizeof(line));
    chorus_check(status == 0 && located.how == CHORUS_LOCATED_MDNS &&
                     strcmp(located.address, "192.0.2.40:4010") == 0 &&
                     strcmp(line, "server-located how=mdns address=192.0.2.40:4010 "
                                  "instance=chorus._chorus-audio._tcp.local.") == 0,
                 "a server that answers wins over everything kept or configured: %s", line);

    memset(&fake, 0, sizeof(fake));
    status = chorus_discovery_locate(&request, scratch, sizeof(scratch), &located);
    chorus_located_line(&located, line, sizeof(line));
    chorus_check(status == 0 && located.how == CHORUS_LOCATED_LAST_GOOD &&
                     strcmp(located.address, "192.0.2.77:4010") == 0 &&
                     strstr(line, "how=last-good address=192.0.2.77:4010 because=discovery "
                                  "returned nothing in 300 ms") != NULL,
                 "a silent link falls back to the last server a session shook hands with, and "
                 "says why: %s",
                 line);

    fake_store_init(&medium);
    memset(&fake, 0, sizeof(fake));
    status = chorus_discovery_locate(&request, scratch, sizeof(scratch), &located);
    chorus_located_line(&located, line, sizeof(line));
    chorus_check(status == 0 && located.how == CHORUS_LOCATED_STATIC &&
                     strcmp(located.address, "198.51.100.5:4010") == 0 &&
                     strstr(line, "how=static-fallback") != NULL &&
                     strstr(line, "because=discovery returned nothing") != NULL,
                 "with no last server kept, the configured address: %s", line);

    memset(&fake, 0, sizeof(fake));
    fake.send_fails = 1;
    status = chorus_discovery_locate(&request, scratch, sizeof(scratch), &located);
    chorus_check(status == 0 && located.how == CHORUS_LOCATED_STATIC &&
                     strstr(located.because, "could not run") != NULL,
                 "a browse that could not run is said as that, not as an empty link: %s",
                 located.because);

    /* The committed endpoint.conf value: loopback, which no board can reach
     * a server at (audit A-10). */
    request.configured = "127.0.0.1:4010";
    memset(&fake, 0, sizeof(fake));
    status = chorus_discovery_locate(&request, scratch, sizeof(scratch), &located);
    chorus_located_line(&located, line, sizeof(line));
    chorus_check(status == -1 && located.how == CHORUS_LOCATED_NOWHERE &&
                     located.address[0] == '\0' && strstr(line, "how=nowhere") != NULL &&
                     strstr(line, "discovery returned nothing in 300 ms") != NULL &&
                     strstr(line, "no last server is kept") != NULL &&
                     strstr(line, "the configured address is loopback") != NULL,
                 "a loopback static address is not a fallback on a board; with nothing else it "
                 "has nowhere to connect and names all three: %s",
                 line);
    request.loopback_is_usable = 1;
    memset(&fake, 0, sizeof(fake));
    status = chorus_discovery_locate(&request, scratch, sizeof(scratch), &located);
    chorus_check(status == 0 && located.how == CHORUS_LOCATED_STATIC &&
                     strcmp(located.address, "127.0.0.1:4010") == 0,
                 "the host session binary, told so, may use a loopback --server as its fallback");
    request.loopback_is_usable = 0;

    request.configured = NULL;
    request.link = NULL;
    request.store = NULL;
    status = chorus_discovery_locate(&request, scratch, sizeof(scratch), &located);
    chorus_check(status == -1 && strstr(located.because, "discovery was not attempted") != NULL &&
                     strstr(located.because, "no static address is configured") != NULL,
                 "with no link, no store and no address it says which it lacked: %s",
                 located.because);

    /* The session's relocate seam: only a server that was FOUND is an answer. */
    chorus_relocator_t relocator = {&link, AUDIO, 300, scratch, sizeof(scratch)};
    char moved[CHORUS_DISCOVERY_ADDRESS_MAX] = "";
    memset(&fake, 0, sizeof(fake));
    int silent = chorus_discovery_relocate(&relocator, moved, sizeof(moved));
    memset(&fake, 0, sizeof(fake));
    say(&fake, spare, spare_len);
    int answered = chorus_discovery_relocate(&relocator, moved, sizeof(moved));
    chorus_check(silent == 0 && answered == 1 && strcmp(moved, "192.0.2.41:4110") == 0,
                 "asked where the server went, discovery answers with one it found (%s) and with "
                 "nothing on a silent link, never with a fallback",
                 moved);

    chorus_check(chorus_discovery_is_loopback("127.0.0.1:4010") &&
                     chorus_discovery_is_loopback("127.8.9.10:1") &&
                     chorus_discovery_is_loopback("LocalHost:4010") &&
                     !chorus_discovery_is_loopback("192.0.2.40:4010") &&
                     !chorus_discovery_is_loopback("1270.example:1") &&
                     !chorus_discovery_is_loopback("chorus.local:4010"),
                 "loopback is 127.0.0.0/8 and `localhost`, and nothing else");
}

int main(void)
{
    walk_the_directory();
    advert_len = read_packet("advertisement-audio", advert, sizeof(advert));
    spare_len = read_packet("advertisement-two-instances", spare, sizeof(spare));
    chorus_check(advert_len > 0 && spare_len > 0, "the committed advertisements are readable");

    a_browse_query_is_the_committed_packet();
    every_committed_response_resolves_to_its_committed_answer();
    compression_and_case_do_not_change_the_answer();
    a_packet_that_is_not_a_message_is_refused_not_half_read();
    what_a_resolver_leaves_out_and_keeps();
    hostile_names_and_records_are_refused();
    the_browse_over_a_link();
    where_the_server_is_in_order();
    return chorus_test_report("test_discovery");
}
