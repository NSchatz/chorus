/* The discovery link (chorus/discovery.h) over POSIX sockets, for the host
 * session binary's --discover: one UDP socket on an ephemeral port that sends
 * the browse query to the mDNS group and reads what is sent back to it.
 *
 * The same two calls the board binds to lwIP (firmware/main/esp_discovery.c).
 * It never binds port 5353 and never joins the group: the query carries the
 * unicast-response bit, so a responder answers this socket directly, which is
 * what lets it run beside a responder on the same host
 * (crates/discovery/src/net.rs does the same for the Linux client).
 *
 * Host-only. Whether the link carries multicast at all is not this file's to
 * decide: tools/endpoint-mdns-live-run.sh grades it where it does and refuses
 * by name where it does not. */

#ifndef CHORUS_POSIX_DISCOVERY_H
#define CHORUS_POSIX_DISCOVERY_H

#include "chorus/discovery.h"

typedef struct {
    int fd;
} posix_discovery_t;

/* Open the socket. 0, or -1 with why in `detail`; the link of a socket that
 * did not open refuses every send, which a browse reports as "could not run". */
int posix_discovery_open(posix_discovery_t *socket_state, char *detail, size_t detail_len);
void posix_discovery_close(posix_discovery_t *socket_state);

chorus_discovery_link_t posix_discovery_link(posix_discovery_t *socket_state);

#endif /* CHORUS_POSIX_DISCOVERY_H */
