/* The session supervisor: join, play, and rejoin with no human action.
 *
 * A transport close and a transport that broke look identical to the peer:
 * both are a read returning zero. `stream_end` is what tells them apart, and
 * everything else is an outage. The supervisor treats every outage the same
 * way - link down, backoff, connect again, resume - because "the server or the
 * network disappears and returns" covers both and the endpoint cannot tell
 * which happened.
 *
 * It never gives up and it never spins: the backoff starts short because most
 * outages are a server restart, and it is capped so that a link down for an
 * hour is not a busy loop. There is no retry budget to exhaust, which is the
 * property the outage-of-minutes run exists to grade. */

#ifndef CHORUS_SESSION_H
#define CHORUS_SESSION_H

#include <stddef.h>
#include <stdint.h>

#include "chorus/sync.h"
#include "chorus/telemetry.h"

#define CHORUS_SESSION_ADDRESS_MAX 128

typedef struct {
    char server[CHORUS_SESSION_ADDRESS_MAX];
    uint32_t first_backoff_ms;
    uint32_t max_backoff_ms;
    /* How long to run for. Zero runs until told to stop. */
    uint32_t run_seconds;
    /* How often to run a time-sync exchange, in milliseconds. */
    uint32_t sync_interval_ms;
    /* Exchanges in the minimum-round-trip window, and the smoother's weight.
     * Both come from config/sync.conf, so both endpoints filter the same way. */
    uint32_t filter_window;
    double smoothing_alpha;
    /* Where to write one telemetry line per event. NULL prints nothing. */
    const char *event_log_path;
} chorus_session_config_t;

/* Why a run ended. Never "the server went away": that is not an end, it is a
 * reconnect. */
typedef enum {
    CHORUS_SESSION_RAN_ITS_TIME = 0,
    CHORUS_SESSION_ADDRESS_UNUSABLE,
    CHORUS_SESSION_STOPPED_ON_AMP_FAULT,
    CHORUS_SESSION_LOG_UNWRITABLE
} chorus_session_end_t;

const char *chorus_session_end_name(chorus_session_end_t end);

typedef struct {
    chorus_session_end_t end;
    chorus_telemetry_t telemetry;
    /* The longest gap between the link going down and coming back up again,
     * in nanoseconds on the endpoint's monotonic clock. This is the outage the
     * endpoint actually rode through, measured rather than assumed. */
    uint64_t longest_outage_ns;
    /* How many distinct connections carried audio. */
    uint32_t connections_that_played;
    char detail[256];
} chorus_session_result_t;

/* Run one session. Returns 0 when it ended because it ran its time. */
int chorus_session_run(const chorus_session_config_t *config, chorus_session_result_t *out);

#endif /* CHORUS_SESSION_H */
