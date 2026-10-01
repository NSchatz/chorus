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
 * property the outage-of-minutes run exists to grade.
 *
 * Every connection is a chorus protocol v2 session (docs/protocol.md, "The
 * session, in order"): the endpoint opens the Noise XX handshake as the
 * initiator with its id and long-term key, checks the server's key against
 * its pin (pinning a server it has never met, refusing one whose key
 * changed), and from then on everything on the connection is a
 * secure_record: its hello and capabilities, the time-sync exchange, and the
 * server's stream_format, output_delay and audio. A server whose key changed
 * is not an outage: the endpoint sends session_refused key_changed and the
 * run ends, because only the owner changes a pin. */

#ifndef CHORUS_SESSION_H
#define CHORUS_SESSION_H

#include <stddef.h>
#include <stdint.h>

#include "chorus/noise.h"
#include "chorus/playout.h"
#include "chorus/protocol_v2.h"
#include "chorus/sync.h"
#include "chorus/telemetry.h"

#define CHORUS_SESSION_ADDRESS_MAX 128
#define CHORUS_SESSION_ID_MAX 256

/* The id used when the configuration names none. An id is what the server
 * pins this endpoint's key to, so a real endpoint names its own. */
#define CHORUS_SESSION_DEFAULT_ID "chorus-endpoint"

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

    /* Who this endpoint is: its id (1 to 255 bytes; empty uses
     * CHORUS_SESSION_DEFAULT_ID) and where its long-term X25519 secret lives
     * (64 hex digits and a newline, mode 0600, made from the random source
     * when the file is absent). NULL keeps a key for this run only. */
    char endpoint_id[CHORUS_SESSION_ID_MAX];
    const char *key_path;
    /* Where the pinned servers live, in the adoption store's text form (one
     * `pinned <public key hex> <server id>` line per server). NULL keeps the
     * pins for this run only. */
    const char *server_pins_path;
    /* How long the server has to answer handshake_init. Zero uses 2000 ms. */
    uint32_t handshake_timeout_ms;
    /* The random source for keys. NULL uses chorus_noise_system_random; a
     * test hands in fixed keys. */
    chorus_noise_random_fn random;
    void *random_ctx;
    /* The playout path (firmware/include/chorus/playout.h): audio chunks, PCM
     * or decoded, go into its jitter buffer and the filtered offset goes to
     * its loop. NULL counts chunks as received and plays nothing (the host
     * session binary). */
    chorus_playout_t *playout;

    /* The endpoint console's two seams (audit A-13), both optional. NULL
     * leaves the run exactly as it was: `server` above for its whole length,
     * and telemetry only in the event log.
     *
     * `server_update` is asked before every connection attempt; it returns 1
     * and writes a `host:port` into `address` when the console set a new one,
     * 0 otherwise. An address that does not split is refused by name in the
     * event log and the old one kept. `on_telemetry` is handed the telemetry
     * at every event the run publishes, which is what the console's `status`
     * prints. Both run on the session's own task. */
    int (*server_update)(void *ctx, char *address, size_t address_len);
    void *server_update_ctx;
    void (*on_telemetry)(void *ctx, const chorus_telemetry_t *telemetry);
    void *telemetry_ctx;

    /* Handed every `sound` (0x39) the server sends, after the decoder has
     * held it to its ranges and the run has kept it in the result
     * (chorus_session_last_sound) and handed it to the playout path's sound
     * chain (chorus_playout_set_sound, chorus/endpoint_dsp.h). Optional: NULL
     * hands it nowhere else (docs/decisions/0081-*). Runs on the session's
     * own task. */
    void (*on_sound)(void *ctx, const chorus_v2_sound_t *sound);
    void *sound_ctx;
} chorus_session_config_t;

/* Why a run ended. Never "the server went away": that is not an end, it is a
 * reconnect. */
typedef enum {
    CHORUS_SESSION_RAN_ITS_TIME = 0,
    CHORUS_SESSION_ADDRESS_UNUSABLE,
    CHORUS_SESSION_STOPPED_ON_AMP_FAULT,
    CHORUS_SESSION_LOG_UNWRITABLE,
    /* The endpoint's key or its pin store could not be read or made. */
    CHORUS_SESSION_IDENTITY_UNUSABLE,
    /* A server presented a key other than the one pinned to its id. Refused
     * with session_refused key_changed, and the run ends: only the owner
     * changes a pin. */
    CHORUS_SESSION_SERVER_KEY_CHANGED
} chorus_session_end_t;

const char *chorus_session_end_name(chorus_session_end_t end);

typedef struct {
    chorus_session_end_t end;
    chorus_telemetry_t telemetry;
    /* The longest gap between the link going down and coming back up again,
     * in nanoseconds on the endpoint's monotonic clock. This is the outage the
     * endpoint actually rode through, measured rather than assumed. */
    uint64_t longest_outage_ns;
    /* How many distinct connections carried audio, counted from
     * `chunks_received`: carried to the endpoint, not played by it (audit
     * A-9). The name stays because firmware/tests/session-outage.sh reads it
     * off the summary line by that key. */
    uint32_t connections_that_played;
    /* Protocol v2: handshakes completed, servers pinned by this run,
     * refusals received from the server, and records each way. */
    uint32_t handshakes;
    uint32_t servers_pinned;
    uint32_t refusals_received;
    uint64_t records_received;
    uint64_t records_sent;
    /* Frames inside records that were rejected (one frame each, never the
     * session: a record's frame boundaries are known). */
    uint64_t rejected_frames;
    /* The last `sound` the server sent, kept across reconnects for the whole
     * run (a new session is not a reason to forget the room's sound, as a
     * new session is not a reason to play louder, ADR 0074), and how many
     * arrived. Read through chorus_session_last_sound. */
    uint8_t have_sound;
    chorus_v2_sound_t sound;
    uint32_t sounds_received;
    /* The fingerprint of the endpoint's own public key. */
    char key_fingerprint[CHORUS_NOISE_FINGERPRINT_LEN];
    char detail[512];
} chorus_session_result_t;

/* Run one session. Returns 0 when it ended because it ran its time, and -1
 * otherwise with `end` and `detail` saying why. */
int chorus_session_run(const chorus_session_config_t *config, chorus_session_result_t *out);

/* Keep `sound` as the last one received: what the session does with every
 * `sound` it decodes, exposed so a test can hold the store to it. */
void chorus_session_keep_sound(chorus_session_result_t *result, const chorus_v2_sound_t *sound);

/* The last `sound` received: 1 with `*out` set, or 0 when none has arrived
 * this run (the endpoint then plays flat, the catalog's defaults being the
 * server's to send). */
int chorus_session_last_sound(const chorus_session_result_t *result, chorus_v2_sound_t *out);

#endif /* CHORUS_SESSION_H */
