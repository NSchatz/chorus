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
 * THE FIRMWARE UPDATE (goal 14). With --ota-flash <file> the endpoint has an
 * update unit (firmware/src/ota.c, the code the board runs) over a two-slot
 * fake flash kept in that file (firmware/tests/fake_flash.c, which models
 * ESP-IDF's bootloader), so a test "reboots" the endpoint by starting this
 * binary again on the same file: each start runs the bootloader model first.
 *
 *   --ota-flash <file>         the flash; made blank, with a first image in
 *                              slot 0, when it does not exist. The update's
 *                              note is kept beside it as <file>.note.
 *   --ota-version <text>       the version of that first image (only used
 *                              when the flash is made, and by --ota-make-image).
 *                              Afterwards the running version is read from the
 *                              running slot's application description.
 *   --ota-board <text>         the board this endpoint says it is (default:
 *                              endpoint.conf's board_profile).
 *   --ota-never-confirm        behave as a bad image: never confirm, so a
 *                              trial ends in a rollback.
 *   --ota-confirm-seconds <n>  the trial's length (default: endpoint.conf's
 *                              ota_confirm_seconds).
 *   --ota-make-image <file>    write an application image of --ota-version
 *                              (--ota-image-bytes of filler, default 65536)
 *                              that the fake flash accepts, and exit.
 *
 * It prints, on stdout, one line per change of the update's state:
 *
 *   ota state=<state> transfer=<n> received=<n> slot=<n> version=<v> reason=<r> image=<v>
 *
 * with <state> one of idle, receiving, verified, pending_verify, confirmed,
 * rolled_back, refused (the wire's names), the first of them at start; and
 * `ota reboot` before it exits 0 because the unit rebooted.
 *
 * Every constant it is not given comes from firmware/config/endpoint.conf, so
 * a run and the file that describes it cannot drift apart. */

#include "chorus/endpoint_config.h"
#include "chorus/monotonic.h"
#include "chorus/ota.h"
#include "chorus/session.h"
#include "chorus/sync_conf.h"
#include "fake_flash.h"

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
            "                               [--ota-flash file] [--ota-version text]\n"
            "                               [--ota-board text] [--ota-never-confirm]\n"
            "                               [--ota-confirm-seconds n]\n"
            "                               [--ota-make-image file [--ota-image-bytes n]]\n");
}

/* One line per change of the update's state, flushed, for the test that
 * drives this binary to read. */
static void print_ota(void *context, const chorus_ota_status_t *status, chorus_ota_state_t state)
{
    (void)context;
    (void)state;
    printf("ota state=%s transfer=%lu received=%lu slot=%u version=%s reason=%s image=%s\n",
           chorus_v2_enum_name(CHORUS_V2_ENUM_FIRMWARE_STATE, status->state),
           (unsigned long)status->transfer, (unsigned long)status->received, (unsigned)status->slot,
           status->version, chorus_v2_enum_name(CHORUS_V2_ENUM_FIRMWARE_REASON, status->reason),
           status->image_version);
    fflush(stdout);
}

/* --ota-make-image: an image the fake flash's own check accepts. */
static int make_image(const char *path, const char *version, size_t body_bytes)
{
    size_t capacity = body_bytes + 1024;
    uint8_t *image = malloc(capacity);
    if (image == NULL) {
        return 2;
    }
    /* The filler is seeded from the version, so two versions differ in more
     * than their name. */
    uint32_t seed = 2166136261u;
    for (const char *c = version; *c != '\0'; c++) {
        seed = (seed ^ (uint8_t)*c) * 16777619u;
    }
    size_t length = fake_flash_make_image(image, capacity, version, body_bytes, seed);
    FILE *f = (length == 0) ? NULL : fopen(path, "wb");
    if (f == NULL || fwrite(image, 1, length, f) != length || fclose(f) != 0) {
        fprintf(stderr, "chorus-endpoint-session: %s could not be written\n", path);
        free(image);
        return 2;
    }
    free(image);
    printf("ota image=%s version=%s bytes=%zu\n", path, version, length);
    return 0;
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

    const char *ota_flash_path = NULL;
    const char *ota_version = "0.0.0-host";
    const char *ota_board = committed.board.profile;
    const char *ota_image_path = NULL;
    size_t ota_image_bytes = 65536;
    uint32_t ota_confirm_seconds = committed.ota_confirm_seconds;
    int ota_never_confirm = 0;

    for (int i = 1; i < argc; i++) {
        const char *arg = argv[i];
        const char *value = (i + 1 < argc) ? argv[i + 1] : NULL;
        if (strcmp(arg, "--ota-flash") == 0 && value != NULL) {
            ota_flash_path = value;
            i++;
        } else if (strcmp(arg, "--ota-version") == 0 && value != NULL) {
            if (strlen(value) == 0 || strlen(value) > 31) {
                fprintf(stderr, "chorus-endpoint-session: a version is 1 to 31 bytes\n");
                return 2;
            }
            ota_version = value;
            i++;
        } else if (strcmp(arg, "--ota-board") == 0 && value != NULL) {
            ota_board = value;
            i++;
        } else if (strcmp(arg, "--ota-never-confirm") == 0) {
            ota_never_confirm = 1;
        } else if (strcmp(arg, "--ota-confirm-seconds") == 0 && value != NULL) {
            ota_confirm_seconds = (uint32_t)strtoul(value, NULL, 10);
            i++;
        } else if (strcmp(arg, "--ota-make-image") == 0 && value != NULL) {
            ota_image_path = value;
            i++;
        } else if (strcmp(arg, "--ota-image-bytes") == 0 && value != NULL) {
            ota_image_bytes = (size_t)strtoul(value, NULL, 10);
            i++;
        } else if (strcmp(arg, "--server") == 0 && value != NULL) {
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

    if (ota_image_path != NULL) {
        if (ota_image_bytes > FAKE_FLASH_FILE_SLOT_BYTES - 1024) {
            fprintf(stderr, "chorus-endpoint-session: an image is at most %u bytes of filler\n",
                    (unsigned)(FAKE_FLASH_FILE_SLOT_BYTES - 1024));
            return 2;
        }
        return make_image(ota_image_path, ota_version, ota_image_bytes);
    }

    /* The update unit over the flash file: the bootloader model runs first,
     * as it does on the board, and the unit starts from what it left. */
    static fake_flash_t flash;
    static fake_notes_t notes;
    static chorus_ota_flash_t flash_ops;
    static chorus_ota_notes_t notes_ops;
    static chorus_ota_t ota;
    if (ota_flash_path != NULL) {
        int created = 0;
        if (fake_flash_open(&flash, ota_flash_path, FAKE_FLASH_FILE_SLOT_BYTES, &created, detail,
                            sizeof(detail)) != 0) {
            fprintf(stderr, "chorus-endpoint-session: %s\n", detail);
            return 2;
        }
        if (created) {
            /* The first flash over USB: one image, in slot 0. */
            static uint8_t first[8192];
            size_t length = fake_flash_make_image(first, sizeof(first), ota_version, 4096, 1);
            if (length == 0 || fake_flash_install(&flash, 0, first, length) != 0) {
                fprintf(stderr, "chorus-endpoint-session: the first image could not be made\n");
                return 2;
            }
        }
        fake_boot_t booted;
        int tries = 0;
        do {
            booted = fake_flash_boot(&flash);
        } while (booted == FAKE_BOOT_POWER_LOST && ++tries < 4);
        if (booted != FAKE_BOOT_OK) {
            fprintf(stderr, "chorus-endpoint-session: %s holds no bootable image\n",
                    ota_flash_path);
            return 5;
        }
        char note_path[FAKE_FLASH_PATH_MAX];
        if (snprintf(note_path, sizeof(note_path), "%s.note", ota_flash_path) >=
            (int)sizeof(note_path)) {
            fprintf(stderr, "chorus-endpoint-session: the flash path is too long\n");
            return 2;
        }
        fake_notes_init(&notes, note_path);
        flash_ops = fake_flash_ops(&flash);
        notes_ops = fake_notes_ops(&notes);
        chorus_ota_config_t ota_config;
        memset(&ota_config, 0, sizeof(ota_config));
        ota_config.flash = &flash_ops;
        ota_config.notes = &notes_ops;
        if (fake_flash_slot_version(&flash, flash.running, ota_config.version,
                                    sizeof(ota_config.version)) != 0) {
            snprintf(ota_config.version, sizeof(ota_config.version), "%.47s", ota_version);
        }
        snprintf(ota_config.board, sizeof(ota_config.board), "%.47s", ota_board);
        ota_config.confirm_ns = (uint64_t)ota_confirm_seconds * 1000000000ull;
        ota_config.never_confirm = ota_never_confirm;
        ota_config.on_change = print_ota;
        printf("ota boot slot=%d image=%s version=%s board=%s\n", flash.running,
               chorus_ota_image_state_name(fake_flash_state(&flash, flash.running)),
               ota_config.version, ota_config.board);
        fflush(stdout);
        (void)chorus_ota_boot(&ota, &ota_config, chorus_monotonic_now_ns());
        config.ota = &ota;
    }

    static chorus_session_result_t result;
    int status = chorus_session_run(&config, &result);
    if (config.ota != NULL && result.end == CHORUS_SESSION_REBOOTING) {
        printf("ota reboot\n");
        fflush(stdout);
    }
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
