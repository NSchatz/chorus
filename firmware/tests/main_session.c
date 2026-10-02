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
 * Goal 14 gave the board an identity it keeps and a way to find the server,
 * and this binary takes both the way the board does, through the same seams:
 *
 *   --store <dir>     what the endpoint keeps across boots (chorus/store.h),
 *                     as a directory of files instead of NVS: the id
 *                     (`chorus-` and twelve hex digits, made at the first run),
 *                     the key, the pinned servers and the last server that
 *                     answered. Starting the binary again over the same
 *                     directory is a reboot. --key and --server-pins are then
 *                     not read; --endpoint-id still names the id.
 *   --discover        browse for `_chorus-audio._tcp.local.` (chorus/discovery.h)
 *                     over a real UDP socket before connecting, and again when
 *                     a run of connection attempts reaches nothing. Falls back
 *                     to the store's last server, then to --server.
 *   --discover-ms <n> the browse window.
 *   --no-server       take the static address away, so a run that discovers
 *                     nothing has nowhere to go and exits 7 saying so.
 *
 * The committed `server_address` is loopback and is NOT a fallback (no board
 * can reach a server there); a --server given on the command line is, loopback
 * or not, because on a host that is where a server is.
 *
 * Every constant it is not given comes from firmware/config/endpoint.conf, so
 * a run and the file that describes it cannot drift apart. */

#include "chorus/discovery.h"
#include "chorus/endpoint_config.h"
#include "chorus/session.h"
#include "chorus/sync_conf.h"
#include "file_store.h"
#include "posix_discovery.h"

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
            "                               [--server-pins path] [--handshake-timeout-ms n]\n"
            "                               [--store dir] [--discover] [--discover-ms n]\n"
            "                               [--no-server]\n");
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

    const char *store_directory = NULL;
    int discover = 0;
    int no_server = 0;
    int server_given = 0;
    uint32_t discover_ms = CHORUS_DISCOVERY_DEFAULT_WINDOW_MS;

    for (int i = 1; i < argc; i++) {
        const char *arg = argv[i];
        const char *value = (i + 1 < argc) ? argv[i + 1] : NULL;
        if (strcmp(arg, "--server") == 0 && value != NULL) {
            snprintf(config.server, sizeof(config.server), "%s", value);
            server_given = 1;
            i++;
        } else if (strcmp(arg, "--store") == 0 && value != NULL) {
            store_directory = value;
            i++;
        } else if (strcmp(arg, "--discover") == 0) {
            discover = 1;
        } else if (strcmp(arg, "--discover-ms") == 0 && value != NULL) {
            discover_ms = (uint32_t)strtoul(value, NULL, 10);
            i++;
        } else if (strcmp(arg, "--no-server") == 0) {
            no_server = 1;
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

    /* What the endpoint keeps across runs, when it is given somewhere to
     * keep it. */
    static file_store_t files;
    static chorus_store_t store;
    if (store_directory != NULL) {
        if (file_store_open(&files, store_directory, detail, sizeof(detail)) != 0) {
            fprintf(stderr, "chorus-endpoint-session: %s\n", detail);
            return 2;
        }
        store = file_store_as_store(&files);
        config.store = &store;
    }

    /* Where the server is. The decision is chorus/discovery.h's, the one the
     * board makes; only the socket is this binary's own. */
    static posix_discovery_t browse_socket = {-1};
    static chorus_discovery_link_t link;
    static uint8_t datagram[CHORUS_DISCOVERY_MAX_DATAGRAM];
    static chorus_relocator_t relocator;
    if (discover || no_server) {
        chorus_locate_t request;
        memset(&request, 0, sizeof(request));
        if (discover) {
            if (posix_discovery_open(&browse_socket, detail, sizeof(detail)) != 0) {
                fprintf(stderr, "chorus-endpoint-session: %s\n", detail);
            }
            link = posix_discovery_link(&browse_socket);
            request.link = &link;
        }
        request.service = CHORUS_DISCOVERY_AUDIO_SERVICE;
        request.window_ms = discover_ms;
        request.store = config.store;
        request.configured = no_server ? NULL : config.server;
        request.loopback_is_usable = server_given;
        static chorus_located_t located;
        char line[640];
        int found = chorus_discovery_locate(&request, datagram, sizeof(datagram), &located);
        chorus_located_line(&located, line, sizeof(line));
        printf("chorus-endpoint-session: %s\n", line);
        fflush(stdout);
        if (found != 0) {
            fprintf(stderr,
                    "chorus-endpoint-session: this endpoint has no server address: %s. It needs "
                    "one of them: a chorus server advertising %s on this link (--discover), a "
                    "--store that has met a server, or --server <host:port>\n",
                    located.because, CHORUS_DISCOVERY_AUDIO_SERVICE);
            posix_discovery_close(&browse_socket);
            return 7;
        }
        snprintf(config.server, sizeof(config.server), "%s", located.address);
        if (discover) {
            relocator.link = &link;
            relocator.service = CHORUS_DISCOVERY_AUDIO_SERVICE;
            relocator.window_ms = discover_ms;
            relocator.scratch = datagram;
            relocator.scratch_capacity = sizeof(datagram);
            config.relocate = chorus_discovery_relocate;
            config.relocate_ctx = &relocator;
        }
    }

    static chorus_session_result_t result;
    int status = chorus_session_run(&config, &result);
    posix_discovery_close(&browse_socket);
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
