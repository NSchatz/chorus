#include "chorus/discovery.h"

#include "chorus/identity.h"

#include <stdio.h>
#include <string.h>

/* The record types this unit reads (RFC 1035 section 3.2.2, RFC 2782 for SRV,
 * RFC 3596 for AAAA), the DNS limits on a name and a label, and how many
 * compression pointers a name may follow before it is called a loop: the same
 * numbers as crates/discovery/src/wire.rs. */
#define TYPE_A 1u
#define TYPE_PTR 12u
#define TYPE_TXT 16u
#define TYPE_AAAA 28u
#define TYPE_SRV 33u
#define CLASS_IN 1u
#define CLASS_UNICAST_RESPONSE 0x8000u
#define FLAG_RESPONSE 0x8000u
#define MAX_NAME_LEN 255u
#define MAX_LABEL_LEN 63u
#define MAX_POINTERS 64u
#define HEADER_LEN 12u

const char *chorus_discovery_status_name(chorus_discovery_status_t status)
{
    switch (status) {
    case CHORUS_DISCOVERY_OK:
        return "ok";
    case CHORUS_DISCOVERY_NOT_A_MESSAGE:
        return "not-a-dns-message";
    case CHORUS_DISCOVERY_A_QUERY:
        return "a-query-not-a-response";
    case CHORUS_DISCOVERY_BAD_SERVICE:
        return "bad-service-type";
    }
    return "unknown";
}

const char *chorus_located_how_name(chorus_located_how_t how)
{
    switch (how) {
    case CHORUS_LOCATED_MDNS:
        return "mdns";
    case CHORUS_LOCATED_LAST_GOOD:
        return "last-good";
    case CHORUS_LOCATED_STATIC:
        return "static-fallback";
    case CHORUS_LOCATED_NOWHERE:
        return "nowhere";
    }
    return "unknown";
}

static uint16_t be16(const uint8_t *bytes)
{
    return (uint16_t)(((uint16_t)bytes[0] << 8) | bytes[1]);
}

/* --- names ------------------------------------------------------------------- */

/* A name read one label at a time, following compression pointers. Following
 * is bounded twice, as the Rust decoder's is: a pointer may only point
 * BACKWARDS, which alone makes it terminate, and the number followed is
 * capped. A message crafted to make a resolver loop is refused instead. */
typedef struct {
    const uint8_t *bytes;
    size_t length;
    size_t cursor;
    size_t followed;
    size_t encoded;
    /* Just past the name AS IT APPEARS where the walk began: for a compressed
     * name, just past its first pointer and not past whatever it pointed at.
     * Valid once the walk has ended. */
    size_t after;
    int jumped;
} name_walk_t;

static void walk_start(name_walk_t *walk, const uint8_t *bytes, size_t length, size_t at)
{
    walk->bytes = bytes;
    walk->length = length;
    walk->cursor = at;
    walk->followed = 0;
    walk->encoded = 1;
    walk->after = at;
    walk->jumped = 0;
}

/* 1 with the next label; 0 at the end of the name; -1 when it is not a name. */
static int walk_next(name_walk_t *walk, const uint8_t **label, size_t *label_len)
{
    for (;;) {
        if (walk->cursor >= walk->length) {
            return -1;
        }
        uint8_t first = walk->bytes[walk->cursor];
        if ((first & 0xC0u) == 0) {
            walk->cursor++;
            if (first == 0) {
                if (!walk->jumped) {
                    walk->after = walk->cursor;
                }
                return 0;
            }
            if (walk->cursor + first > walk->length) {
                return -1;
            }
            walk->encoded += 1u + first;
            if (walk->encoded > MAX_NAME_LEN) {
                return -1;
            }
            *label = walk->bytes + walk->cursor;
            *label_len = first;
            walk->cursor += first;
            return 1;
        }
        if ((first & 0xC0u) != 0xC0u) {
            /* Reserved label-length bits: not a name. */
            return -1;
        }
        if (walk->cursor + 1 >= walk->length) {
            return -1;
        }
        size_t target = ((size_t)(first & 0x3Fu) << 8) | walk->bytes[walk->cursor + 1];
        if (!walk->jumped) {
            walk->after = walk->cursor + 2;
            walk->jumped = 1;
        }
        if (target >= walk->cursor || ++walk->followed > MAX_POINTERS) {
            return -1;
        }
        walk->cursor = target;
    }
}

/* Step over the name at `*at`. 0, or -1 when it is not a name. */
static int skip_name(const uint8_t *bytes, size_t length, size_t *at)
{
    name_walk_t walk;
    walk_start(&walk, bytes, length, *at);
    const uint8_t *label = NULL;
    size_t label_len = 0;
    int step;
    while ((step = walk_next(&walk, &label, &label_len)) == 1) {
    }
    if (step != 0) {
        return -1;
    }
    *at = walk.after;
    return 0;
}

static uint8_t lower(uint8_t c)
{
    return (c >= 'A' && c <= 'Z') ? (uint8_t)(c + ('a' - 'A')) : c;
}

/* Whether two names are the same name: label by label, ASCII
 * case-insensitively, which is what DNS says (RFC 1035 section 2.3.3) and
 * what a resolver that meets a responder capitalising differently needs.
 * Compared on the wire labels and not on dotted text, so a label that holds a
 * dot is not mistaken for two. */
static int names_equal(const uint8_t *a, size_t a_length, size_t a_at, const uint8_t *b,
                       size_t b_length, size_t b_at)
{
    name_walk_t wa;
    name_walk_t wb;
    walk_start(&wa, a, a_length, a_at);
    walk_start(&wb, b, b_length, b_at);
    for (;;) {
        const uint8_t *la = NULL;
        const uint8_t *lb = NULL;
        size_t la_len = 0;
        size_t lb_len = 0;
        int sa = walk_next(&wa, &la, &la_len);
        int sb = walk_next(&wb, &lb, &lb_len);
        if (sa < 0 || sb < 0 || sa != sb) {
            return 0;
        }
        if (sa == 0) {
            return 1;
        }
        if (la_len != lb_len) {
            return 0;
        }
        for (size_t i = 0; i < la_len; i++) {
            if (lower(la[i]) != lower(lb[i])) {
                return 0;
            }
        }
    }
}

/* The dotted form with its trailing dot, and the first label on its own. A
 * byte that would end or break a line of a log is written as `?`: a name
 * comes off the network and goes into a log. */
static void name_text(const uint8_t *bytes, size_t length, size_t at, char *out, size_t capacity,
                      char *first_label, size_t first_capacity)
{
    name_walk_t walk;
    walk_start(&walk, bytes, length, at);
    size_t used = 0;
    size_t labels = 0;
    const uint8_t *label = NULL;
    size_t label_len = 0;
    if (first_label != NULL && first_capacity > 0) {
        first_label[0] = '\0';
    }
    while (walk_next(&walk, &label, &label_len) == 1) {
        size_t start = used;
        for (size_t i = 0; i < label_len && used + 2 < capacity; i++) {
            uint8_t c = label[i];
            out[used++] = (c < 0x20u || c == 0x7Fu) ? '?' : (char)c;
        }
        if (labels == 0 && first_label != NULL && first_capacity > 0) {
            size_t take = used - start;
            if (take > first_capacity - 1) {
                take = first_capacity - 1;
            }
            memcpy(first_label, out + start, take);
            first_label[take] = '\0';
        }
        if (used + 1 < capacity) {
            out[used++] = '.';
        }
        labels++;
    }
    if (labels == 0 && used + 1 < capacity) {
        out[used++] = '.';
    }
    out[used] = '\0';
}

/* A dotted name to its wire form, written out in full. The trailing dot is
 * optional. Returns the length, or -1 for an empty or over-long label, a name
 * over the DNS limit, or no room. */
static long encode_name(const char *text, uint8_t *out, size_t capacity)
{
    size_t text_len = strlen(text);
    if (text_len > 0 && text[text_len - 1] == '.') {
        text_len--;
    }
    size_t used = 0;
    size_t at = 0;
    while (at < text_len) {
        const char *dot = memchr(text + at, '.', text_len - at);
        size_t label_len = (dot == NULL) ? text_len - at : (size_t)(dot - (text + at));
        if (label_len == 0 || label_len > MAX_LABEL_LEN || used + 1 + label_len + 1 > capacity ||
            used + 1 + label_len + 1 > MAX_NAME_LEN) {
            return -1;
        }
        out[used++] = (uint8_t)label_len;
        memcpy(out + used, text + at, label_len);
        used += label_len;
        at += label_len + ((dot == NULL) ? 0 : 1);
        if (dot != NULL && at == text_len) {
            /* `a..` or a dot before the trailing one: an empty label. */
            return -1;
        }
    }
    if (used + 1 > capacity) {
        return -1;
    }
    out[used++] = 0;
    return (long)used;
}

long chorus_discovery_browse_query(const char *service, int unicast_response, uint8_t *out,
                                   size_t capacity)
{
    if (service == NULL || capacity < HEADER_LEN + 1 + 4) {
        return -1;
    }
    memset(out, 0, HEADER_LEN);
    /* One question; the identifier and the flags are zero. */
    out[5] = 1;
    long name_len = encode_name(service, out + HEADER_LEN, capacity - HEADER_LEN - 4);
    if (name_len < 0) {
        return -1;
    }
    size_t at = HEADER_LEN + (size_t)name_len;
    uint16_t qclass = (uint16_t)(CLASS_IN | (unicast_response ? CLASS_UNICAST_RESPONSE : 0u));
    out[at++] = 0;
    out[at++] = (uint8_t)TYPE_PTR;
    out[at++] = (uint8_t)(qclass >> 8);
    out[at++] = (uint8_t)(qclass & 0xFFu);
    return (long)at;
}

/* --- records ----------------------------------------------------------------- */

typedef struct {
    size_t name_at;
    uint16_t type;
    size_t rdata_at;
    size_t rdata_len;
} record_t;

/* Read the record at `*at` and hold its data to its type, as the Rust decoder
 * does: a PTR is a name, an SRV six bytes and a name, a TXT strings that stay
 * inside the record, an A four bytes, an AAAA sixteen. Any other type is
 * carried by its length. 0 with `*at` past it, or -1. */
static int read_record(const uint8_t *bytes, size_t length, size_t *at, record_t *record)
{
    size_t cursor = *at;
    record->name_at = cursor;
    if (skip_name(bytes, length, &cursor) != 0 || cursor + 10 > length) {
        return -1;
    }
    record->type = be16(bytes + cursor);
    record->rdata_len = be16(bytes + cursor + 8);
    record->rdata_at = cursor + 10;
    size_t end = record->rdata_at + record->rdata_len;
    if (end > length) {
        return -1;
    }
    size_t inner = record->rdata_at;
    switch (record->type) {
    case TYPE_PTR:
        if (skip_name(bytes, length, &inner) != 0) {
            return -1;
        }
        break;
    case TYPE_SRV:
        inner += 6;
        if (record->rdata_len < 6 || skip_name(bytes, length, &inner) != 0) {
            return -1;
        }
        break;
    case TYPE_TXT:
        while (inner < end) {
            inner += 1u + bytes[inner];
            if (inner > end) {
                return -1;
            }
        }
        break;
    case TYPE_A:
        if (record->rdata_len != 4) {
            return -1;
        }
        break;
    case TYPE_AAAA:
        if (record->rdata_len != 16) {
            return -1;
        }
        break;
    default:
        break;
    }
    *at = end;
    return 0;
}

/* Hold the whole message to the wire format. 0 with where its records start,
 * how many there are across the three sections, and its flags; -1 when any
 * part of it is malformed. A packet is read whole or not at all. */
static int open_message(const uint8_t *bytes, size_t length, size_t *records_at, size_t *records,
                        uint16_t *flags)
{
    if (bytes == NULL || length < HEADER_LEN) {
        return -1;
    }
    *flags = be16(bytes + 2);
    size_t questions = be16(bytes + 4);
    size_t count = (size_t)be16(bytes + 6) + be16(bytes + 8) + be16(bytes + 10);
    size_t at = HEADER_LEN;
    for (size_t i = 0; i < questions; i++) {
        if (skip_name(bytes, length, &at) != 0 || at + 4 > length) {
            return -1;
        }
        at += 4;
    }
    *records_at = at;
    for (size_t i = 0; i < count; i++) {
        record_t record;
        if (read_record(bytes, length, &at, &record) != 0) {
            return -1;
        }
    }
    *records = count;
    return 0;
}

/* The first record of `type` about the name at (`name`, `name_at`). 1 with the
 * record, or 0. The message has been held to the format already. */
static int find_record(const uint8_t *bytes, size_t length, size_t records_at, size_t records,
                       uint16_t type, const uint8_t *name, size_t name_length, size_t name_at,
                       record_t *found)
{
    size_t at = records_at;
    for (size_t i = 0; i < records; i++) {
        record_t record;
        if (read_record(bytes, length, &at, &record) != 0) {
            return 0;
        }
        if (record.type == type &&
            names_equal(bytes, length, record.name_at, name, name_length, name_at)) {
            *found = record;
            return 1;
        }
    }
    return 0;
}

static void fill_service(const uint8_t *bytes, size_t length, size_t records_at, size_t records,
                         size_t instance_at, const record_t *srv, chorus_discovery_service_t *out)
{
    memset(out, 0, sizeof(*out));
    name_text(bytes, length, instance_at, out->instance, sizeof(out->instance), out->label,
              sizeof(out->label));
    size_t host_at = srv->rdata_at + 6;
    name_text(bytes, length, host_at, out->host, sizeof(out->host), NULL, 0);
    out->port = be16(bytes + srv->rdata_at + 4);

    record_t txt;
    if (find_record(bytes, length, records_at, records, TYPE_TXT, bytes, length, instance_at,
                    &txt)) {
        size_t at = txt.rdata_at;
        size_t end = txt.rdata_at + txt.rdata_len;
        while (at < end) {
            size_t string_len = bytes[at];
            /* An empty string says nothing (RFC 6763 section 6.1: an empty TXT
             * record is one zero byte) and is not kept. */
            if (string_len > 0) {
                if (out->txt_len + 1 + string_len <= sizeof(out->txt)) {
                    memcpy(out->txt + out->txt_len, bytes + at, 1 + string_len);
                    out->txt_len += 1 + string_len;
                } else {
                    out->txt_dropped++;
                }
            }
            at += 1 + string_len;
        }
    }

    record_t address;
    if (find_record(bytes, length, records_at, records, TYPE_A, bytes, length, host_at, &address)) {
        memcpy(out->address, bytes + address.rdata_at, 4);
        out->have_address = 1;
    }
}

/* The resolver. With `dialable_only`, an instance with no address is passed
 * over as if it were not there: what a browse wants, which takes the first
 * instance it can connect to. */
static chorus_discovery_status_t resolve(const uint8_t *packet, size_t length, const char *service,
                                         int dialable_only, chorus_discovery_service_t *out,
                                         size_t capacity, size_t *found)
{
    uint8_t wanted[MAX_NAME_LEN + 1];
    *found = 0;
    long wanted_len = (service == NULL) ? -1 : encode_name(service, wanted, sizeof(wanted));
    if (wanted_len < 0) {
        return CHORUS_DISCOVERY_BAD_SERVICE;
    }
    size_t records_at = 0;
    size_t records = 0;
    uint16_t flags = 0;
    if (open_message(packet, length, &records_at, &records, &flags) != 0) {
        return CHORUS_DISCOVERY_NOT_A_MESSAGE;
    }
    if ((flags & FLAG_RESPONSE) == 0) {
        return CHORUS_DISCOVERY_A_QUERY;
    }

    size_t at = records_at;
    for (size_t i = 0; i < records; i++) {
        record_t pointer;
        if (read_record(packet, length, &at, &pointer) != 0) {
            return CHORUS_DISCOVERY_NOT_A_MESSAGE;
        }
        if (pointer.type != TYPE_PTR ||
            !names_equal(packet, length, pointer.name_at, wanted, (size_t)wanted_len, 0)) {
            continue;
        }
        /* An instance named by two PTR records is one instance: the first. */
        int seen = 0;
        size_t earlier_at = records_at;
        for (size_t k = 0; k < i && !seen; k++) {
            record_t earlier;
            if (read_record(packet, length, &earlier_at, &earlier) != 0) {
                return CHORUS_DISCOVERY_NOT_A_MESSAGE;
            }
            seen = earlier.type == TYPE_PTR &&
                   names_equal(packet, length, earlier.name_at, wanted, (size_t)wanted_len, 0) &&
                   names_equal(packet, length, earlier.rdata_at, packet, length, pointer.rdata_at);
        }
        if (seen) {
            continue;
        }
        /* An instance with no SRV record has no host and no port: there is
         * nothing to connect to, and it is left out rather than guessed. */
        record_t srv;
        if (!find_record(packet, length, records_at, records, TYPE_SRV, packet, length,
                         pointer.rdata_at, &srv)) {
            continue;
        }
        if (dialable_only) {
            size_t host_at = srv.rdata_at + 6;
            record_t address;
            if (!find_record(packet, length, records_at, records, TYPE_A, packet, length, host_at,
                             &address)) {
                continue;
            }
        }
        if (*found < capacity && out != NULL) {
            fill_service(packet, length, records_at, records, pointer.rdata_at, &srv, &out[*found]);
        }
        (*found)++;
    }
    return CHORUS_DISCOVERY_OK;
}

chorus_discovery_status_t chorus_discovery_resolve(const uint8_t *packet, size_t length,
                                                   const char *service,
                                                   chorus_discovery_service_t *out, size_t capacity,
                                                   size_t *found)
{
    return resolve(packet, length, service, 0, out, capacity, found);
}

int chorus_discovery_txt_value(const chorus_discovery_service_t *service, const char *key,
                               char *out, size_t capacity)
{
    size_t key_len = strlen(key);
    size_t at = 0;
    while (at < service->txt_len) {
        size_t string_len = service->txt[at];
        const uint8_t *string = service->txt + at + 1;
        at += 1 + string_len;
        const uint8_t *equals = memchr(string, '=', string_len);
        size_t name_len = (equals == NULL) ? string_len : (size_t)(equals - string);
        if (name_len != key_len) {
            continue;
        }
        int same = 1;
        for (size_t i = 0; i < key_len && same; i++) {
            same = lower(string[i]) == lower((uint8_t)key[i]);
        }
        if (!same) {
            continue;
        }
        /* A string with no `=` is a key with no value (RFC 6763 section 6.4). */
        size_t value_len = (equals == NULL) ? 0 : string_len - name_len - 1;
        if (value_len + 1 > capacity) {
            return 0;
        }
        if (value_len > 0) {
            memcpy(out, equals + 1, value_len);
        }
        out[value_len] = '\0';
        return 1;
    }
    return 0;
}

int chorus_discovery_socket_address(const chorus_discovery_service_t *service, char *out,
                                    size_t capacity)
{
    if (!service->have_address) {
        return 0;
    }
    int wrote = snprintf(out, capacity, "%u.%u.%u.%u:%u", (unsigned)service->address[0],
                         (unsigned)service->address[1], (unsigned)service->address[2],
                         (unsigned)service->address[3], (unsigned)service->port);
    return (wrote > 0 && (size_t)wrote < capacity) ? 1 : 0;
}

int chorus_discovery_is_loopback(const char *address)
{
    if (address == NULL) {
        return 0;
    }
    const char *colon = strrchr(address, ':');
    size_t host_len = (colon == NULL) ? strlen(address) : (size_t)(colon - address);
    if (host_len >= 4 && strncmp(address, "127.", 4) == 0) {
        return 1;
    }
    static const char NAME[] = "localhost";
    if (host_len == sizeof(NAME) - 1) {
        for (size_t i = 0; i < host_len; i++) {
            if (lower((uint8_t)address[i]) != (uint8_t)NAME[i]) {
                return 0;
            }
        }
        return 1;
    }
    return 0;
}

/* --- the browse and the decision --------------------------------------------- */

chorus_browse_status_t chorus_discovery_browse(const chorus_discovery_link_t *link,
                                               const char *service, uint32_t window_ms,
                                               uint8_t *scratch, size_t scratch_capacity,
                                               chorus_discovery_service_t *found)
{
    uint8_t query[HEADER_LEN + MAX_NAME_LEN + 1 + 4];
    long query_len = chorus_discovery_browse_query(service, 1, query, sizeof(query));
    if (link == NULL || link->send == NULL || link->receive == NULL || query_len < 0 ||
        link->send(link->context, query, (size_t)query_len) != 0) {
        return CHORUS_BROWSE_LINK_FAILED;
    }
    uint32_t slices = window_ms / CHORUS_DISCOVERY_SLICE_MS;
    if (slices == 0) {
        slices = 1;
    }
    uint32_t silent = 0;
    uint32_t packets = 0;
    int asked_again = 0;
    while (silent < slices && packets < CHORUS_DISCOVERY_MAX_PACKETS) {
        long got =
            link->receive(link->context, scratch, scratch_capacity, CHORUS_DISCOVERY_SLICE_MS);
        if (got < 0) {
            return CHORUS_BROWSE_LINK_FAILED;
        }
        if (got == 0) {
            silent++;
            if (!asked_again && silent < slices && silent * 2 >= slices) {
                /* One lost datagram is not an absent server. A second query
                 * that cannot be sent is not a failure of the first. */
                asked_again = 1;
                (void)link->send(link->context, query, (size_t)query_len);
            }
            continue;
        }
        packets++;
        size_t count = 0;
        /* A datagram that is not an advertisement of this service (another
         * responder's traffic, a query, noise) is passed over, never an
         * error: port 5353 is a shared channel. */
        if (resolve(scratch, (size_t)got, service, 1, found, 1, &count) == CHORUS_DISCOVERY_OK &&
            count > 0) {
            return CHORUS_BROWSE_FOUND;
        }
    }
    return CHORUS_BROWSE_NOTHING;
}

int chorus_discovery_locate(const chorus_locate_t *request, uint8_t *scratch,
                            size_t scratch_capacity, chorus_located_t *out)
{
    memset(out, 0, sizeof(*out));
    char discovery[96];
    snprintf(discovery, sizeof(discovery), "discovery was not attempted");
    if (request->link != NULL) {
        chorus_discovery_service_t found;
        chorus_browse_status_t browsed = chorus_discovery_browse(
            request->link, request->service, request->window_ms, scratch, scratch_capacity, &found);
        if (browsed == CHORUS_BROWSE_FOUND &&
            chorus_discovery_socket_address(&found, out->address, sizeof(out->address))) {
            out->how = CHORUS_LOCATED_MDNS;
            snprintf(out->instance, sizeof(out->instance), "%s", found.instance);
            return 0;
        }
        if (browsed == CHORUS_BROWSE_LINK_FAILED) {
            snprintf(discovery, sizeof(discovery), "discovery could not run at all");
        } else {
            snprintf(discovery, sizeof(discovery), "discovery returned nothing in %u ms",
                     (unsigned)request->window_ms);
        }
    }
    if (request->store != NULL &&
        chorus_identity_server_load(request->store, out->address, sizeof(out->address)) == 1) {
        out->how = CHORUS_LOCATED_LAST_GOOD;
        snprintf(out->because, sizeof(out->because), "%s", discovery);
        return 0;
    }
    int configured = request->configured != NULL && request->configured[0] != '\0';
    int loopback = configured && chorus_discovery_is_loopback(request->configured);
    if (configured && (!loopback || request->loopback_is_usable) &&
        strlen(request->configured) < sizeof(out->address)) {
        out->how = CHORUS_LOCATED_STATIC;
        snprintf(out->address, sizeof(out->address), "%s", request->configured);
        snprintf(out->because, sizeof(out->because), "%s", discovery);
        return 0;
    }
    out->how = CHORUS_LOCATED_NOWHERE;
    out->address[0] = '\0';
    snprintf(out->because, sizeof(out->because), "%s, no last server is kept, and %s", discovery,
             loopback ? "the configured address is loopback" : "no static address is configured");
    return -1;
}

int chorus_discovery_relocate(void *relocator, char *address, size_t address_len)
{
    const chorus_relocator_t *ask = (const chorus_relocator_t *)relocator;
    chorus_discovery_service_t found;
    if (ask == NULL ||
        chorus_discovery_browse(ask->link, ask->service, ask->window_ms, ask->scratch,
                                ask->scratch_capacity, &found) != CHORUS_BROWSE_FOUND) {
        return 0;
    }
    return chorus_discovery_socket_address(&found, address, address_len);
}

void chorus_located_line(const chorus_located_t *located, char *out, size_t capacity)
{
    switch (located->how) {
    case CHORUS_LOCATED_MDNS:
        snprintf(out, capacity, "server-located how=mdns address=%s instance=%s", located->address,
                 located->instance);
        return;
    case CHORUS_LOCATED_LAST_GOOD:
    case CHORUS_LOCATED_STATIC:
        snprintf(out, capacity, "server-located how=%s address=%s because=%s",
                 chorus_located_how_name(located->how), located->address, located->because);
        return;
    case CHORUS_LOCATED_NOWHERE:
        break;
    }
    snprintf(out, capacity, "server-located how=nowhere because=%s", located->because);
}
