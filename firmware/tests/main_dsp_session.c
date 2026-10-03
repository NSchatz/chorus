/* The endpoint's session, playout path and sound chain, as a program, with a
 * fake I2S DMA that keeps every frame written (goal 12, done-when line C).
 *
 * This is what firmware/tests/dsp-session.sh drives against a REAL
 * chorus-server on loopback: the real session supervisor (the protocol v2
 * handshake, the records, the time-sync exchange, `room_volume`, `sound`,
 * stream_format and the audio), the real playout path (jitter buffer, sync
 * loop, room volume) and the real sound chain (chorus/endpoint_dsp.h), exactly
 * as firmware/main/app_main.c wires them on the board. What is fake is the
 * I2S controller: a writer thread plays its part on the monotonic clock. It
 * keeps dma_desc_num buffers of dma_frame_num frames queued (preloaded with
 * silence, as esp_playout.c preloads the stopped channel), and at every
 * buffer period it "sends" the oldest (chorus_playout_on_dma_sent, the
 * interrupt hook), fills the next one (chorus_playout_fill) and appends it to
 * the capture file. The loop is ticked at every buffer until it has acquired
 * the timeline, then every interval_ms, as playout.h says the board's writer
 * does.
 *
 *   chorus-endpoint-dsp-session --server 127.0.0.1:4010 --endpoint-id sub \
 *       --run-seconds 30 --capture /tmp/sub.raw [--two-way on]
 *
 * The capture is the frames in the order they reach the pins, from the first
 * preloaded buffer: raw, interleaved, two I2S slots per frame, each slot
 * `slot_bytes` bytes little-endian signed (3 at endpoint.conf's 24-bit slot
 * width), no header. The summary line names the rate, the slot size and
 * `capture_start_ns`, the monotonic instant the capture's first frame started
 * at the pins, so a script can turn a monotonic time into a capture frame.
 *
 * THE HEALTH SEAM (goal 15). A host has no radio, no board temperature and no
 * heap_caps, so by default this program reports all of them unknown, as the
 * session does with no `health` seam. The five options below hand the seam
 * STATED FAKE values, through the same callback the board binds
 * (chorus/session.h `health`), so a test can see every telemetry field cross
 * the wire; they are a test's numbers and never a measurement:
 *
 *   --health-link wired|wireless      --health-rssi-dbm <n>
 *   --health-temperature-centi-c <n>  --health-heap-free-bytes <n>
 *   --health-heap-min-free-bytes <n>
 *
 * Every constant it is not given comes from firmware/config/endpoint.conf and
 * config/sync.conf. Host only: pthreads and clock_nanosleep. Nothing here is
 * timing evidence; what dsp-session.sh grades is the content of the frames. */

#include <errno.h>
#include <inttypes.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include "chorus/endpoint_config.h"
#include "chorus/endpoint_dsp.h"
#include "chorus/monotonic.h"
#include "chorus/playout.h"
#include "chorus/session.h"
#include "chorus/sync_conf.h"

static pthread_mutex_t playout_mutex = PTHREAD_MUTEX_INITIALIZER;
static void take(void *ctx)
{
    (void)ctx;
    pthread_mutex_lock(&playout_mutex);
}
static void give(void *ctx)
{
    (void)ctx;
    pthread_mutex_unlock(&playout_mutex);
}

typedef struct {
    chorus_playout_t *playout;
    FILE *capture;
    uint32_t frames;     /* per DMA buffer */
    uint32_t queued;     /* DMA buffers kept queued */
    uint32_t frame_size; /* bytes */
    uint64_t period_ns;
    uint32_t interval_ms;
    uint64_t start_ns;
    volatile int running;
    uint64_t buffers;
    uint64_t late_ticks;
} writer_t;

static void sleep_until(uint64_t ns)
{
    struct timespec ts;
    ts.tv_sec = (time_t)(ns / 1000000000ull);
    ts.tv_nsec = (long)(ns % 1000000000ull);
    while (clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME, &ts, NULL) == EINTR) {
    }
}

static void *writer_main(void *arg)
{
    writer_t *w = arg;
    uint8_t *block = calloc(w->frames, w->frame_size);
    if (block == NULL) {
        return NULL;
    }
    /* The stopped channel, preloaded with silence: written and consumed then
     * count the same frames from the first interrupt. */
    chorus_playout_dma_reset(w->playout);
    for (uint32_t i = 0; i < w->queued; i++) {
        chorus_playout_note_preloaded(w->playout, w->frames);
        fwrite(block, w->frame_size, w->frames, w->capture);
    }
    w->start_ns = chorus_monotonic_now_ns();
    uint64_t next_observe = w->start_ns;
    for (uint64_t k = 1; w->running; k++) {
        uint64_t due = w->start_ns + k * w->period_ns;
        if (chorus_monotonic_now_ns() > due + w->period_ns) {
            w->late_ticks++;
        }
        sleep_until(due);
        /* The oldest buffer has left for the pins: the interrupt hook. */
        chorus_playout_on_dma_sent(w->playout, (size_t)w->frames * w->frame_size);
        (void)chorus_playout_fill(w->playout, block, w->frames);
        fwrite(block, w->frame_size, w->frames, w->capture);
        w->buffers++;
        uint64_t now = chorus_monotonic_now_ns();
        if (!chorus_playout_acquired(w->playout) || now >= next_observe) {
            (void)chorus_playout_observe(w->playout, NULL);
            next_observe = now + (uint64_t)w->interval_ms * 1000000ull;
        }
    }
    free(block);
    return NULL;
}

/* The health seam's stated fakes: what the command line named, nothing else. */
static chorus_session_health_t stated_health;

static void stated_health_report(void *ctx, chorus_session_health_t *health)
{
    (void)ctx;
    *health = stated_health;
}

static void usage(void)
{
    fprintf(stderr, "usage: chorus-endpoint-dsp-session --capture path [--server host:port]\n"
                    "                                   [--endpoint-id id] [--run-seconds n]\n"
                    "                                   [--two-way on|off] [--log path]\n"
                    "                                   [--health-link wired|wireless]\n"
                    "                                   [--health-rssi-dbm n]\n"
                    "                                   [--health-temperature-centi-c n]\n"
                    "                                   [--health-heap-free-bytes n]\n"
                    "                                   [--health-heap-min-free-bytes n]\n");
}

int main(int argc, char **argv)
{
    static chorus_endpoint_config_t committed;
    char detail[512];
    detail[0] = '\0';
    if (chorus_endpoint_config_load(&committed, chorus_endpoint_config_default_path(), detail,
                                    sizeof(detail)) != 0) {
        fprintf(stderr, "chorus-endpoint-dsp-session: %s\n", detail);
        return 2;
    }
    chorus_sync_conf_t sync;
    if (chorus_sync_conf_load(&sync, CHORUS_REPO_ROOT "/config/sync.conf", detail,
                              sizeof(detail)) != 0) {
        fprintf(stderr, "chorus-endpoint-dsp-session: %s\n", detail);
        return 2;
    }

    static chorus_session_config_t config;
    memset(&config, 0, sizeof(config));
    snprintf(config.server, sizeof(config.server), "%s", committed.server_address);
    config.first_backoff_ms = committed.reconnect_first_backoff_ms;
    config.max_backoff_ms = committed.reconnect_max_backoff_ms;
    config.run_seconds = 10;
    config.sync_interval_ms = sync.sync_interval_ms;
    config.filter_window = sync.filter_window;
    config.smoothing_alpha = sync.smoothing_alpha;
    const char *capture_path = NULL;
    chorus_endpoint_two_way_t two_way = committed.two_way;
    chorus_session_health_unknown(&stated_health);

    for (int i = 1; i < argc; i++) {
        const char *arg = argv[i];
        const char *value = (i + 1 < argc) ? argv[i + 1] : NULL;
        if (value == NULL) {
            usage();
            return 2;
        }
        if (strcmp(arg, "--server") == 0) {
            snprintf(config.server, sizeof(config.server), "%s", value);
        } else if (strcmp(arg, "--run-seconds") == 0) {
            config.run_seconds = (uint32_t)strtoul(value, NULL, 10);
        } else if (strcmp(arg, "--endpoint-id") == 0) {
            if (strlen(value) == 0 || strlen(value) >= sizeof(config.endpoint_id)) {
                fprintf(stderr, "chorus-endpoint-dsp-session: an endpoint id is 1 to 255 bytes\n");
                return 2;
            }
            snprintf(config.endpoint_id, sizeof(config.endpoint_id), "%s", value);
        } else if (strcmp(arg, "--capture") == 0) {
            capture_path = value;
        } else if (strcmp(arg, "--log") == 0) {
            config.event_log_path = value;
        } else if (strcmp(arg, "--health-link") == 0) {
            if (strcmp(value, "wired") == 0) {
                stated_health.link = 1;
            } else if (strcmp(value, "wireless") == 0) {
                stated_health.link = 2;
            } else {
                usage();
                return 2;
            }
            config.health = stated_health_report;
        } else if (strcmp(arg, "--health-rssi-dbm") == 0) {
            stated_health.rssi_dbm = (int8_t)strtol(value, NULL, 10);
            config.health = stated_health_report;
        } else if (strcmp(arg, "--health-temperature-centi-c") == 0) {
            stated_health.temperature_centi_c = (int16_t)strtol(value, NULL, 10);
            config.health = stated_health_report;
        } else if (strcmp(arg, "--health-heap-free-bytes") == 0) {
            stated_health.heap_free_bytes = (uint32_t)strtoul(value, NULL, 10);
            config.health = stated_health_report;
        } else if (strcmp(arg, "--health-heap-min-free-bytes") == 0) {
            stated_health.heap_min_free_bytes = (uint32_t)strtoul(value, NULL, 10);
            config.health = stated_health_report;
        } else if (strcmp(arg, "--two-way") == 0) {
            /* The committed file's crossover and slots, switched on or off:
             * the two-way is the speaker's, and the test runs one speaker of
             * each kind from one file. */
            if (strcmp(value, "on") == 0) {
                two_way.enabled = true;
            } else if (strcmp(value, "off") == 0) {
                two_way.enabled = false;
            } else {
                usage();
                return 2;
            }
        } else {
            usage();
            return 2;
        }
        i++;
    }
    if (capture_path == NULL) {
        usage();
        return 2;
    }
    FILE *capture = fopen(capture_path, "wb");
    if (capture == NULL) {
        fprintf(stderr, "chorus-endpoint-dsp-session: %s could not be opened\n", capture_path);
        return 2;
    }

    /* The playout path as the board builds it (esp_playout.c): the I2S clock's
     * rate and slot width, the DMA's buffer size, the committed ceiling. */
    chorus_playout_config_t pc = chorus_playout_config_from(
        &sync, committed.clock.sample_rate_hz, (uint8_t)committed.clock.slot_bit_width,
        committed.clock.dma_frame_num, CHORUS_PLAYOUT_BUFFER_MS);
    pc.max_volume_thousandths = committed.max_volume_thousandths;
    uint8_t *ring = malloc(chorus_playout_ring_bytes(&pc));
    chorus_playout_chunk_t *chunks = malloc(chorus_playout_chunk_bytes(&pc));
    static chorus_playout_t playout;
    if (ring == NULL || chunks == NULL ||
        chorus_playout_init(&playout, &pc, ring, chunks, chorus_monotonic_now_ns) != 0) {
        fprintf(stderr, "chorus-endpoint-dsp-session: the playout path cannot be built\n");
        return 2;
    }
    chorus_playout_set_lock(&playout, take, give, NULL);
    static chorus_endpoint_dsp_t dsp;
    chorus_endpoint_dsp_init(&dsp, &two_way);
    chorus_playout_set_dsp(&playout, &dsp);
    config.playout = &playout;

    static writer_t w;
    w.playout = &playout;
    w.capture = capture;
    w.frames = pc.dma_frame_num;
    w.queued = committed.clock.dma_desc_num;
    w.frame_size = (uint32_t)pc.channels * pc.out_sample_bytes;
    w.period_ns = (uint64_t)pc.dma_frame_num * 1000000000ull / pc.rate_hz;
    w.interval_ms = pc.interval_ms;
    w.running = 1;
    pthread_t writer;
    if (pthread_create(&writer, NULL, writer_main, &w) != 0) {
        fprintf(stderr, "chorus-endpoint-dsp-session: no writer thread\n");
        return 2;
    }

    static chorus_session_result_t result;
    int status = chorus_session_run(&config, &result);
    w.running = 0;
    pthread_join(writer, NULL);
    fclose(capture);

    chorus_playout_stats_t st;
    chorus_playout_stats(&playout, &st);
    printf("chorus-endpoint-dsp-session: end=%s id=%s rate_hz=%u slot_bytes=%u "
           "capture_start_ns=%" PRIu64 " preloaded_frames=%u buffers=%" PRIu64
           " late_ticks=%" PRIu64 "\n",
           chorus_session_end_name(result.end), config.endpoint_id, (unsigned)pc.rate_hz,
           (unsigned)pc.out_sample_bytes, w.start_ns, (unsigned)(w.queued * w.frames), w.buffers,
           w.late_ticks);
    printf("chorus-endpoint-dsp-session: dsp_engaged=%u dsp_latency_frames=%u sounds_received=%u "
           "sounds_applied=%u dsp_refusals=%u two_way=%s room_volume_messages=%u "
           "applied_volume=%u\n",
           st.dsp_engaged, st.dsp_latency_frames, (unsigned)result.sounds_received,
           st.sounds_applied, st.dsp_refusals, two_way.enabled ? "on" : "off",
           st.room_volume_messages, st.applied_volume_thousandths);
    printf("chorus-endpoint-dsp-session: chunks=%" PRIu64 " frames_played=%" PRIu64
           " underrun_frames=%" PRIu64 " starved_frames=%" PRIu64 " hard_resyncs=%u"
           " last_error_us=%.1f\n",
           result.telemetry.chunks_received, st.frames_played, st.underrun_frames,
           st.starved_frames, st.hard_resyncs, st.last_error_ns / 1000.0);
    if (status != 0) {
        fprintf(stderr, "chorus-endpoint-dsp-session: %s: %s\n",
                chorus_session_end_name(result.end), result.detail);
        return 3;
    }
    return 0;
}
