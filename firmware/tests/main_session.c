/* The endpoint's session supervisor, as a program.
 *
 * This is what firmware/tests/session-outage.sh drives against a REAL
 * chorus-server on a real socket, killing and restarting the server under it.
 * It is not a mock and it does not model a network: it opens a TCP connection,
 * runs the protocol v2 handshake as the endpoint, decodes the committed
 * protocol out of the encrypted records, runs the time-sync exchange, and
 * rejoins when the far end goes away.
 *
 *   chorus-endpoint-session --server 127.0.0.1:4010 --run-seconds 20 \
 *       --log /tmp/endpoint.log --endpoint-id kitchen \
 *       --key /var/lib/chorus/endpoint.key --server-pins /var/lib/chorus/server-pins
 *
 * The key and the pins are files on the host build; without --key the key is
 * made for this run only, and without --server-pins the pins are kept for this
 * run only.
 *
 * Every constant it is not given comes from firmware/config/endpoint.conf, so
 * a run and the file that describes it cannot drift apart. */

#include "chorus/endpoint_config.h"
#include "chorus/session.h"
#include "chorus/sync_conf.h"

#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void usage(void)
{
    fprintf(stderr,
            "usage: chorus-endpoint-session [--server host:port] [--run-seconds n]\n"
            "                               [--log path] [--sync-interval-ms n]\n"
            "                               [--first-backoff-ms n] [--max-backoff-ms n]\n"
            "                               [--endpoint-id id] [--key path]\n"
            "                               [--server-pins path] [--handshake-timeout-ms n]\n");
}

int main(int argc, char **argv)
{
    chorus_endpoint_config_t committed;
    char detail[512];
    detail[0] = '\0';
    if (chorus_endpoint_config_load(&committed, chorus_endpoint_config_default_path(), detail,
                                    sizeof(detail)) != 0) {
        fprintf(stderr, "chorus-endpoint-session: %s\n", detail);
        return 2;
    }

    chorus_session_config_t config;
    memset(&config, 0, sizeof(config));
    snprintf(config.server, sizeof(config.server), "%s", committed.server_address);
    config.first_backoff_ms = committed.reconnect_first_backoff_ms;
    config.max_backoff_ms = committed.reconnect_max_backoff_ms;
    config.run_seconds = 10;
    /* The exchange cadence and the filter shape come from config/sync.conf,
     * the file the Linux client is held to, read here as the board reads its
     * embedded copy (audit A-12), so both endpoints of a group filter the same
     * way. The interval is overridable because the outage runs are short and
     * an exchange every half second in a twelve second run is four exchanges. */
    chorus_sync_conf_t sync;
    if (chorus_sync_conf_load(&sync, CHORUS_REPO_ROOT "/config/sync.conf", detail,
                              sizeof(detail)) != 0) {
        fprintf(stderr, "chorus-endpoint-session: %s\n", detail);
        return 2;
    }
    config.sync_interval_ms = sync.sync_interval_ms;
    config.filter_window = sync.filter_window;
    config.smoothing_alpha = sync.smoothing_alpha;

    for (int i = 1; i < argc; i++) {
        const char *arg = argv[i];
        const char *value = (i + 1 < argc) ? argv[i + 1] : NULL;
        if (strcmp(arg, "--server") == 0 && value != NULL) {
            snprintf(config.server, sizeof(config.server), "%s", value);
            i++;
        } else if (strcmp(arg, "--run-seconds") == 0 && value != NULL) {
            config.run_seconds = (uint32_t)strtoul(value, NULL, 10);
            i++;
        } else if (strcmp(arg, "--log") == 0 && value != NULL) {
            config.event_log_path = value;
            i++;
        } else if (strcmp(arg, "--sync-interval-ms") == 0 && value != NULL) {
            config.sync_interval_ms = (uint32_t)strtoul(value, NULL, 10);
            i++;
        } else if (strcmp(arg, "--first-backoff-ms") == 0 && value != NULL) {
            config.first_backoff_ms = (uint32_t)strtoul(value, NULL, 10);
            i++;
        } else if (strcmp(arg, "--max-backoff-ms") == 0 && value != NULL) {
            config.max_backoff_ms = (uint32_t)strtoul(value, NULL, 10);
            i++;
        } else if (strcmp(arg, "--endpoint-id") == 0 && value != NULL) {
            if (strlen(value) == 0 || strlen(value) >= sizeof(config.endpoint_id)) {
                fprintf(stderr, "chorus-endpoint-session: an endpoint id is 1 to 255 bytes\n");
                return 2;
            }
            snprintf(config.endpoint_id, sizeof(config.endpoint_id), "%s", value);
            i++;
        } else if (strcmp(arg, "--key") == 0 && value != NULL) {
            config.key_path = value;
            i++;
        } else if (strcmp(arg, "--server-pins") == 0 && value != NULL) {
            config.server_pins_path = value;
            i++;
        } else if (strcmp(arg, "--handshake-timeout-ms") == 0 && value != NULL) {
            config.handshake_timeout_ms = (uint32_t)strtoul(value, NULL, 10);
            i++;
        } else {
            usage();
            return 2;
        }
    }

    static chorus_session_result_t result;
    int status = chorus_session_run(&config, &result);
    if (status != 0 && result.end != CHORUS_SESSION_SERVER_KEY_CHANGED) {
        fprintf(stderr, "chorus-endpoint-session: %s: %s\n", chorus_session_end_name(result.end),
                result.detail);
        return 3;
    }

    char line[1024];
    chorus_telemetry_line(&result.telemetry, line, sizeof(line));
    printf("%s\n", line);
    printf("chorus-endpoint-session: end=%s server=%s run_seconds=%u\n",
           chorus_session_end_name(result.end), config.server, config.run_seconds);
    printf("chorus-endpoint-session: rejoins=%" PRIu32 " attempts=%" PRIu32
           " connections_that_played=%" PRIu32 " longest_outage_ns=%" PRIu64 " chunks=%" PRIu64
           " frames=%" PRIu64 " exchanges=%" PRIu32 " skipped=%" PRIu64 "\n",
           result.telemetry.rejoins, result.telemetry.connect_attempts,
           result.connections_that_played, result.longest_outage_ns,
           result.telemetry.chunks_received, result.telemetry.frames_received,
           result.telemetry.exchanges, result.telemetry.skipped_frames);
    printf("chorus-endpoint-session: protocol=v2 key=%s handshakes=%" PRIu32
           " servers_pinned=%" PRIu32 " refusals=%" PRIu32 " records_received=%" PRIu64
           " records_sent=%" PRIu64 " rejected=%" PRIu64 "\n",
           result.key_fingerprint, result.handshakes, result.servers_pinned,
           result.refusals_received, result.records_received, result.records_sent,
           result.rejected_frames);
    if (status != 0) {
        /* Not an outage: the server's key is not its pin, and only the owner
         * changes a pin. */
        fprintf(stderr, "chorus-endpoint-session: %s: %s\n", chorus_session_end_name(result.end),
                result.detail);
        return 4;
    }
    return 0;
}
