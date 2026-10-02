/* Discovery's board binding (goal 14): the two calls of chorus/discovery.h's
 * link, over an lwIP UDP socket, and the placement of its answer in the
 * session's configuration.
 *
 * Every decision (the query's bytes, what a response advertises, which
 * instance, the fallback order) is firmware/src/discovery.c's and is graded on
 * a host by firmware/tests/test_discovery.c against fixtures/discovery. What is
 * here is a socket, NOT HOST-GRADABLE and NOT CLAIMED like the rest of this
 * directory; the same two calls over POSIX sockets
 * (firmware/tests/posix_discovery.c) are what `make verify-endpoint-mdns` runs
 * against a real advertising server. Compiled only by ESP-IDF. */

#ifndef CHORUS_ESP_DISCOVERY_H
#define CHORUS_ESP_DISCOVERY_H

#include "chorus/session.h"

/* Decide where the server is and write it into `session->server`: a server
 * that answers a browse for `_chorus-audio._tcp.local.`; else the last server
 * this board shook hands with (`session->store`, so this runs after
 * chorus_esp_identity_load); else the address already in `session->server`
 * when it is not loopback. With none of the three the address is left as it
 * is and that is logged by name. Either way the session's `relocate` seam is
 * bound to a browse, so a board that booted before its server, or whose
 * server moved, finds it without a power cycle. Call with the link up. */
void chorus_esp_discovery_locate(chorus_session_config_t *session);

#endif /* CHORUS_ESP_DISCOVERY_H */
