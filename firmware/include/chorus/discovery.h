/* Finding the server: a DNS-SD browse, and what to dial when it finds nothing
 * (goal 14, docs/decisions/0104-*; audit A-10's server-address part).
 *
 * Until this unit the board dialled `server_address` from endpoint.conf, and
 * the committed value is loopback: a board cannot reach anything there. The
 * server already says where it is (`chorus-server --advertise`, RFC 6763 over
 * multicast DNS, crates/discovery); this is the endpoint's half, in C.
 *
 * Three parts, and only the last touches a network:
 *
 *   the bytes    the PTR query for a service type, and what a response packet
 *                advertises: instance, SRV host and port, TXT, the host's A
 *                record. Held to the SAME committed packets as the Rust crate
 *                (fixtures/discovery, firmware/tests/test_discovery.c), so the
 *                two resolvers cannot drift apart.
 *   the decision where to connect: the server discovery found; else the last
 *                server this endpoint shook hands with (the store's
 *                `server_addr`, chorus/identity.h); else the configured
 *                `server_address` when it is not loopback; else nothing, said
 *                by name. Graded over a fake link and the fake store.
 *   the link     two calls, send the query and wait for a datagram, bound to
 *                lwIP on the board (firmware/main/esp_discovery.c) and to
 *                POSIX sockets in the host session binary
 *                (firmware/tests/posix_discovery.c).
 *
 * It is a browser, not a responder (the endpoint advertises nothing), and it
 * is as much of RFC 6762 as one endpoint finding one server needs: no cache,
 * no known-answer list, no continuous querying. Pure: no ESP-IDF, no clock of
 * its own, no heap. IPv4 only, as the session's dialler is (its `host:port`
 * form takes no bracketed IPv6): an AAAA record is read, held to its sixteen
 * bytes, and not dialled. */

#ifndef CHORUS_DISCOVERY_H
#define CHORUS_DISCOVERY_H

#include <stddef.h>
#include <stdint.h>

#include "chorus/store.h"

/* The service type the audio stream is advertised as (crates/discovery,
 * AUDIO_SERVICE), the port multicast DNS runs on and its IPv4 group
 * (RFC 6762 sections 2 and 3). */
#define CHORUS_DISCOVERY_AUDIO_SERVICE "_chorus-audio._tcp.local."
#define CHORUS_DISCOVERY_MDNS_PORT 5353u
#define CHORUS_DISCOVERY_MDNS_GROUP_V4 "224.0.0.251"

/* The largest datagram the browser reads: one Ethernet frame's worth of UDP
 * payload (1500 less the IPv4 and UDP headers). RFC 6762 section 17 allows a
 * responder more on a link with a larger MTU; a chorus advertisement is under
 * 300 bytes, and a longer datagram is cut by the socket and then refused whole
 * by the decoder rather than half read. */
#define CHORUS_DISCOVERY_MAX_DATAGRAM 1472u

/* A dotted name with its trailing dot and NUL: 253 characters at the DNS
 * limit of 255 encoded bytes. */
#define CHORUS_DISCOVERY_NAME_MAX 256u
#define CHORUS_DISCOVERY_LABEL_MAX 64u
/* The TXT strings kept per instance, as they are on the wire (a length byte
 * and that many bytes each). Whole strings only: one that would not fit is
 * left out and counted. */
#define CHORUS_DISCOVERY_TXT_MAX 256u
/* Instances kept from one packet. More are counted and not kept. */
#define CHORUS_DISCOVERY_MAX_INSTANCES 4u
#define CHORUS_DISCOVERY_ADDRESS_MAX 128u

typedef enum {
    CHORUS_DISCOVERY_OK = 0,
    /* The bytes are not a DNS message this decoder accepts. */
    CHORUS_DISCOVERY_NOT_A_MESSAGE,
    /* A well-formed message that is a query: nothing is advertised by it. */
    CHORUS_DISCOVERY_A_QUERY,
    /* The service type is not a name that encodes. */
    CHORUS_DISCOVERY_BAD_SERVICE
} chorus_discovery_status_t;

const char *chorus_discovery_status_name(chorus_discovery_status_t status);

/* What a resolver made of one advertised instance. */
typedef struct {
    /* The service instance name in full, and its first label. */
    char instance[CHORUS_DISCOVERY_NAME_MAX];
    char label[CHORUS_DISCOVERY_LABEL_MAX];
    /* The host and port the SRV record names. */
    char host[CHORUS_DISCOVERY_NAME_MAX];
    uint16_t port;
    /* The first A record the message carried for that host. Without one the
     * message named a host and gave no way to reach it: a partial answer. */
    uint8_t have_address;
    uint8_t address[4];
    uint8_t txt[CHORUS_DISCOVERY_TXT_MAX];
    size_t txt_len;
    size_t txt_dropped;
} chorus_discovery_service_t;

/* The query an endpoint browses with: one PTR question for `service`, the
 * query identifier zero (RFC 6762 section 18.1). `unicast_response` sets the
 * question's top class bit (section 5.4), so a responder answers the asking
 * socket directly and the endpoint need not bind port 5353 or join the group;
 * clear, the bytes are the committed query vectors'. Returns the length, or
 * -1 when the service does not encode or `capacity` is too small. */
long chorus_discovery_browse_query(const char *service, int unicast_response, uint8_t *out,
                                   size_t capacity);

/* Every instance of `service` a response packet advertises, in the order
 * their PTR records appear, the first `capacity` of them written to `out` and
 * all of them counted in `*found`. The whole message is held to the DNS wire
 * format first: a packet with one malformed record resolves to nothing.
 * An instance with no SRV record is not an instance (nothing to connect to)
 * and is neither written nor counted. */
chorus_discovery_status_t chorus_discovery_resolve(const uint8_t *packet, size_t length,
                                                   const char *service,
                                                   chorus_discovery_service_t *out, size_t capacity,
                                                   size_t *found);

/* One TXT value by key (ASCII case-insensitive, RFC 6763 section 6.4). 1 with
 * the value (empty for a key with no `=`), 0 when the key is absent or the
 * value does not fit. */
int chorus_discovery_txt_value(const chorus_discovery_service_t *service, const char *key,
                               char *out, size_t capacity);

/* `a.b.c.d:port`, the form the session dials. 1, or 0 when the instance has
 * no address. */
int chorus_discovery_socket_address(const chorus_discovery_service_t *service, char *out,
                                    size_t capacity);

/* Whether a `host:port` names this machine itself (127.0.0.0/8 or
 * `localhost`): the committed endpoint.conf value, which no board can reach a
 * server at. */
int chorus_discovery_is_loopback(const char *address);

/* --- the link ---------------------------------------------------------------- */

typedef struct chorus_discovery_link {
    void *context;
    /* Send one datagram to the mDNS group. 0, or -1 when it could not go. */
    int (*send)(void *context, const uint8_t *query, size_t length);
    /* Wait up to `wait_ms` for one datagram. Its length; 0 when none came in
     * that time; -1 when the socket failed. */
    long (*receive)(void *context, uint8_t *out, size_t capacity, uint32_t wait_ms);
} chorus_discovery_link_t;

/* How long one wait is: the browse's unit of time. The unit has no clock; a
 * browse of `window_ms` is that many milliseconds of waits that brought
 * nothing, so it ends on time on a silent link, and CHORUS_DISCOVERY_MAX_PACKETS
 * bounds it on a loud one. */
#define CHORUS_DISCOVERY_SLICE_MS 50u
#define CHORUS_DISCOVERY_MAX_PACKETS 32u
/* The default browse window. ASSUMED: RFC 6762 section 6 has a responder
 * delay a shared-record answer by 20 to 120 ms, and the Linux client's own
 * default is of this order; not measured on a board. */
#define CHORUS_DISCOVERY_DEFAULT_WINDOW_MS 1500u

typedef enum {
    CHORUS_BROWSE_FOUND = 0,
    /* The link carried no answer with an address in the window. */
    CHORUS_BROWSE_NOTHING,
    /* The query could not be sent or the socket failed: discovery could not
     * run, which is not the same as there being no server. */
    CHORUS_BROWSE_LINK_FAILED
} chorus_browse_status_t;

/* Browse the link for `service`: send the query (with the unicast-response
 * bit), and again halfway through the window if nothing has answered (one
 * lost datagram is not an absent server), and take the FIRST instance that
 * resolves to an address. `scratch` holds one datagram
 * (CHORUS_DISCOVERY_MAX_DATAGRAM bytes). */
chorus_browse_status_t chorus_discovery_browse(const chorus_discovery_link_t *link,
                                               const char *service, uint32_t window_ms,
                                               uint8_t *scratch, size_t scratch_capacity,
                                               chorus_discovery_service_t *found);

typedef enum {
    /* Discovery answered. */
    CHORUS_LOCATED_MDNS = 0,
    /* Discovery did not; the last server this endpoint shook hands with. */
    CHORUS_LOCATED_LAST_GOOD,
    /* Neither; the configured static address. */
    CHORUS_LOCATED_STATIC,
    /* Nothing to connect to. */
    CHORUS_LOCATED_NOWHERE
} chorus_located_how_t;

const char *chorus_located_how_name(chorus_located_how_t how);

typedef struct {
    chorus_located_how_t how;
    char address[CHORUS_DISCOVERY_ADDRESS_MAX];
    /* The instance that answered, when one did. */
    char instance[CHORUS_DISCOVERY_NAME_MAX];
    /* What discovery did, in words a log can carry: why a fallback was taken. */
    char because[160];
} chorus_located_t;

typedef struct {
    /* The link to browse on. NULL does not browse (discovery not attempted). */
    const chorus_discovery_link_t *link;
    const char *service;
    uint32_t window_ms;
    /* Where the last good address lives. NULL has none. */
    const chorus_store_t *store;
    /* The configured static address. NULL or empty has none. */
    const char *configured;
    /* Whether a loopback `configured` counts. The board: no (the committed
     * value). The host session binary with an explicit --server: yes, loopback
     * is where its server is. */
    int loopback_is_usable;
} chorus_locate_t;

/* Decide where the server is, in the order above. Returns 0 with `out` filled
 * (`how` is never NOWHERE then), or -1 with `how` NOWHERE and `because` saying
 * what each of the three lacked. */
int chorus_discovery_locate(const chorus_locate_t *request, uint8_t *scratch,
                            size_t scratch_capacity, chorus_located_t *out);

/* The session's `relocate` seam (chorus/session.h), bound to a browse: asked
 * when a run of connection attempts reached nothing, it looks for the server
 * again and answers only with one it FOUND. No fallback here: the session
 * already has an address, and the question is whether the server moved. */
typedef struct {
    const chorus_discovery_link_t *link;
    const char *service;
    uint32_t window_ms;
    uint8_t *scratch;
    size_t scratch_capacity;
} chorus_relocator_t;

/* `relocator` is a chorus_relocator_t. 1 with the address, else 0. */
int chorus_discovery_relocate(void *relocator, char *address, size_t address_len);

/* One line for a log: `server-located how=<how> address=<a> ...`. */
void chorus_located_line(const chorus_located_t *located, char *out, size_t capacity);

#endif /* CHORUS_DISCOVERY_H */
