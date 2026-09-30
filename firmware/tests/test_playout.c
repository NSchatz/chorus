/* The playout path, graded on a host (audit A-9, chorus goal 8).
 *
 * A fake DMA drives firmware/src/playout.c the way the ESP32-S3's I2S
 * controller will: the writer hands over one DMA buffer of frames at a time,
 * and each buffer the fake controller finishes calls the interrupt hook at the
 * instant it finishes, on a fake MONOTONIC clock. The fake DAC runs at a
 * skewed rate, the server's chunks arrive with jitter, and the session's
 * offset arrives with noise. The PCM carries a frame counter, so the test can
 * read which content frame reaches the pins at every buffer boundary and grade
 * the true playout error, independently of what the unit believes.
 *
 * Deterministic: a fixed-seed generator and no sleeps. The numbers are a
 * model, labelled as such, and never timing evidence (BRIEF section 3.1 rule 3). */

#include <inttypes.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "chorus/playout.h"
#include "chorus/protocol.h"
#include "chorus/sync_conf.h"
#include "harness.h"

#define RATE 48000u
#define DMA_FRAMES 240u
#define DMA_DESC 6u
#define CHUNK_FRAMES 960u
#define OUT_BYTES 3u
#define FRAME_OUT (2u * OUT_BYTES)

static uint64_t fake_now;
static uint64_t fake_clock(void)
{
    return fake_now;
}

static uint32_t lcg_state = 12345u;
static uint32_t lcg(void)
{
    lcg_state = lcg_state * 1664525u + 1013904223u;
    return lcg_state >> 8;
}

static chorus_sync_conf_t sync_conf;
static uint8_t ring_storage[RATE * 300u / 1000u * FRAME_OUT];
static chorus_playout_chunk_t chunk_storage[128];

static chorus_playout_config_t make_config(void)
{
    chorus_playout_config_t c =
        chorus_playout_config_from(&sync_conf, RATE, 24, DMA_FRAMES, CHORUS_PLAYOUT_BUFFER_MS);
    return c;
}

/* One stereo s24le chunk whose left sample carries content index + 1 (zero is
 * silence) and whose right sample carries its negation. */
static void make_chunk(uint8_t *pcm, uint64_t first_content)
{
    for (uint32_t f = 0; f < CHUNK_FRAMES; f++) {
        uint32_t v = (uint32_t)((first_content + f) % 8388000u) + 1u;
        uint32_t r = (uint32_t)(-(int32_t)v) & 0xffffffu;
        pcm[f * 6 + 0] = (uint8_t)v;
        pcm[f * 6 + 1] = (uint8_t)(v >> 8);
        pcm[f * 6 + 2] = (uint8_t)(v >> 16);
        pcm[f * 6 + 3] = (uint8_t)r;
        pcm[f * 6 + 4] = (uint8_t)(r >> 8);
        pcm[f * 6 + 5] = (uint8_t)(r >> 16);
    }
}

static uint32_t out_left(const uint8_t *frame)
{
    return (uint32_t)frame[0] | (uint32_t)frame[1] << 8 | (uint32_t)frame[2] << 16;
}

/* --- the modelled run --------------------------------------------------------- */

typedef struct {
    double skew_ppm;       /* the fake DAC's rate error */
    double offset_ns;      /* server minus endpoint, true */
    double offset_bias_ns; /* added to what the session reports, from bias_from_s */
    double bias_from_s;
    double gap_from_s; /* chunks lost for gap_s from here */
    double gap_s;
    double seconds;
    int skip_fill_at_event; /* the writer stalls here long enough for the DMA to run dry */
} scenario_t;

typedef struct {
    chorus_playout_stats_t stats;
    double true_err_max_us; /* over the graded tail */
    double own_err_max_us;
    double corr_mean_ppm;
    uint64_t audio_written;
    uint64_t audio_in_consumed_blocks;
    uint32_t hard_after_acquisition;
    int stamp_matches; /* every hook call stamped the clock of its instant */
    int delay_matches; /* observe's delay = written - consumed - in flight */
    int error_matches; /* observe's error formed from that delay */
    uint32_t observations;
} outcome_t;

static int run(const scenario_t *s, outcome_t *o)
{
    memset(o, 0, sizeof(*o));
    o->stamp_matches = 1;
    o->delay_matches = 1;
    o->error_matches = 1;
    chorus_playout_config_t config = make_config();
    static chorus_playout_t p;
    fake_now = 1000000000ull;
    if (chorus_playout_init(&p, &config, ring_storage, chunk_storage, fake_clock) != 0) {
        return -1;
    }
    const uint64_t start = fake_now;
    const double tail_from = s->seconds - 20.0;

    /* Preload: the binding stops the channel, resets the counts, preloads
     * DMA_DESC buffers and enables it. Those buffers are silence here. */
    chorus_playout_dma_reset(&p);
    static uint8_t block[DMA_FRAMES * FRAME_OUT];
    /* First content value of each written block not yet finished, in order. */
    static uint32_t fifo_first[64];
    static uint32_t fifo_audio[64];
    uint32_t fifo_read = 0, fifo_count = 0;
    for (uint32_t i = 0; i < DMA_DESC; i++) {
        uint32_t audio = chorus_playout_fill(&p, block, DMA_FRAMES);
        fifo_first[(fifo_read + fifo_count) % 64] = out_left(block);
        fifo_audio[(fifo_read + fifo_count) % 64] = audio;
        fifo_count++;
        o->audio_written += audio;
    }

    const double buffer_ns = (double)DMA_FRAMES * 1e9 / ((double)RATE * (1.0 + s->skew_ppm * 1e-6));
    double next_dma = (double)start + buffer_ns;
    double next_chunk_gen = (double)start;
    uint64_t chunk_content = 0;
    uint32_t sequence = 0;
    double pending_arrival = -1.0;
    uint64_t pending_ts = 0, pending_content = 0;
    uint32_t pending_seq = 0;
    double next_observe = (double)start + (double)config.interval_ms * 1e6;
    const double end = (double)start + s->seconds * 1e9;
    uint32_t events = 0;
    uint32_t hard_before_tail = 0;
    double corr_sum = 0.0;
    uint32_t corr_n = 0;
    static uint8_t pcm[CHUNK_FRAMES * 6];

    for (;;) {
        double next_arrival = (pending_arrival >= 0.0) ? pending_arrival : 1e300;
        double t = next_dma;
        int kind = 0;
        if (next_chunk_gen < t && pending_arrival < 0.0) {
            t = next_chunk_gen;
            kind = 1;
        }
        if (next_arrival < t) {
            t = next_arrival;
            kind = 2;
        }
        if (next_observe < t) {
            t = next_observe;
            kind = 3;
        }
        if (t >= end) {
            break;
        }
        fake_now = (uint64_t)t;
        double since_s = (t - (double)start) / 1e9;

        if (kind == 0) {
            /* The controller finished one buffer: the interrupt. */
            chorus_playout_on_dma_sent(&p, DMA_FRAMES * FRAME_OUT);
            if (p.dma_stamp_lo != (uint32_t)fake_now ||
                p.dma_stamp_hi != (uint32_t)(fake_now >> 32)) {
                o->stamp_matches = 0;
            }
            events++;
            fifo_read = (fifo_read + 1u) % 64u;
            fifo_count--;
            /* The block now starting at the pins, and its true error. */
            if (fifo_count > 0 && fifo_first[fifo_read] != 0 && since_s >= tail_from) {
                double content = (double)(fifo_first[fifo_read] - 1u);
                double content_ts = (double)start + s->offset_ns + content * 1e9 / RATE;
                double err = content_ts + (double)config.playout_latency_ns - (t + s->offset_ns);
                double us = fabs(err) / 1000.0;
                if (us > o->true_err_max_us) {
                    o->true_err_max_us = us;
                }
            }
            uint32_t stall = (uint32_t)s->skip_fill_at_event;
            if (stall != 0 && events >= stall && events < stall + 5u) {
                /* The writer is stalled: the DMA drains its queue. */
            } else if (stall != 0 && events == stall + 5u) {
                /* Queue empty: the DMA sends one auto-cleared buffer of zeros
                 * the writer never handed over. */
                fifo_first[(fifo_read + fifo_count) % 64] = 0;
                fifo_audio[(fifo_read + fifo_count) % 64] = 0;
                fifo_count++;
            } else {
                /* The writer keeps DMA_DESC buffers queued; after a stall it
                 * refills every free slot at once, as i2s_channel_write does. */
                while (fifo_count < DMA_DESC) {
                    uint32_t audio = chorus_playout_fill(&p, block, DMA_FRAMES);
                    fifo_first[(fifo_read + fifo_count) % 64] = out_left(block);
                    fifo_audio[(fifo_read + fifo_count) % 64] = audio;
                    fifo_count++;
                    o->audio_written += audio;
                }
                /* Until the loop has placed the stream, the writer ticks it at
                 * every buffer rather than every interval. */
                if (!chorus_playout_acquired(&p)) {
                    (void)chorus_playout_observe(&p, NULL);
                }
            }
            next_dma += buffer_ns;
        } else if (kind == 1) {
            /* The server cuts a chunk at its own now, stamped on its timeline. */
            make_chunk(pcm, chunk_content);
            pending_ts =
                (uint64_t)((double)start + s->offset_ns + (double)chunk_content * 1e9 / RATE);
            pending_content = chunk_content;
            pending_seq = sequence++;
            pending_arrival = t + 2e6 + (double)(lcg() % 1000000u);
            chunk_content += CHUNK_FRAMES;
            next_chunk_gen += (double)CHUNK_FRAMES * 1e9 / RATE;
            if (s->gap_s > 0.0 && since_s >= s->gap_from_s && since_s < s->gap_from_s + s->gap_s) {
                pending_arrival = -1.0; /* lost */
            }
        } else if (kind == 2) {
            make_chunk(pcm, pending_content);
            (void)chorus_playout_offer(&p, pending_ts, pending_seq, CHORUS_FMT_PCM_S24LE, 2, RATE,
                                       pcm, CHUNK_FRAMES);
            pending_arrival = -1.0;
        } else {
            /* The session's filtered offset, with noise, then one servo tick. */
            double noise = (double)((int32_t)(lcg() % 40001u) - 20000);
            double bias =
                (since_s >= s->bias_from_s && s->bias_from_s > 0.0) ? s->offset_bias_ns : 0.0;
            chorus_playout_set_offset(&p, s->offset_ns + noise + bias, fake_now);
            uint32_t hard_before = p.stats.hard_resyncs;
            chorus_playout_report_t r;
            chorus_playout_observation_t kind_seen = chorus_playout_observe(&p, &r);
            o->observations++;
            if (kind_seen == CHORUS_PLAYOUT_FINE || kind_seen == CHORUS_PLAYOUT_HARD_RESYNC) {
                /* Re-derive the delay and the error from the report's own
                 * DMA snapshot, the way the Linux client's formula reads. */
                double queued = (double)(r.written_frames - r.dma_consumed_frames);
                double partial = (double)(r.now_ns - r.dma_stamp_ns) * RATE / 1e9;
                if (partial > DMA_FRAMES) {
                    partial = DMA_FRAMES;
                }
                if (partial > queued) {
                    partial = queued;
                }
                if (fabs(r.device_delay_frames - (queued - partial)) > 1e-6) {
                    o->delay_matches = 0;
                }
                double expected = (double)r.next_write_ts_ns + (double)config.playout_latency_ns -
                                  ((double)r.now_ns + r.device_delay_ns + r.offset_ns);
                /* The error adds what the corrector still owes; fine ticks
                 * owe less than a frame (20.8 us), hard resyncs are graded
                 * by their own count. */
                if (kind_seen == CHORUS_PLAYOUT_FINE && fabs(r.error_ns - expected) > 21000.0) {
                    o->error_matches = 0;
                }
                if (since_s >= tail_from) {
                    double us = fabs(r.error_ns) / 1000.0;
                    if (us > o->own_err_max_us) {
                        o->own_err_max_us = us;
                    }
                    corr_sum += r.correction_ppm;
                    corr_n++;
                }
            }
            if (p.stats.hard_resyncs > hard_before && since_s > 2.0 && since_s < tail_from) {
                hard_before_tail++;
            }
            next_observe += (double)config.interval_ms * 1e6;
        }
    }
    chorus_playout_stats(&p, &o->stats);
    /* The blocks the DMA has passed, by the test's own bookkeeping: every
     * block written minus those still queued. */
    uint64_t still = 0;
    for (uint32_t i = 0; i < fifo_count; i++) {
        still += fifo_audio[(fifo_read + i) % 64u];
    }
    o->audio_in_consumed_blocks = o->audio_written - still;
    o->corr_mean_ppm = (corr_n > 0) ? corr_sum / corr_n : 0.0;
    o->hard_after_acquisition = hard_before_tail;
    return 0;
}

/* --- the Rust client's constants, for A-12 ---------------------------------- */

static int rust_const(const char *text, const char *name, double *out)
{
    char needle[96];
    snprintf(needle, sizeof(needle), "pub const %s:", name);
    const char *at = strstr(text, needle);
    if (at == NULL) {
        return -1;
    }
    at = strchr(at, '=');
    if (at == NULL) {
        return -1;
    }
    char digits[64];
    size_t n = 0;
    for (at++; *at != ';' && *at != '\0' && n + 1 < sizeof(digits); at++) {
        if (*at != '_' && *at != ' ') {
            digits[n++] = *at;
        }
    }
    digits[n] = '\0';
    *out = strtod(digits, NULL);
    return 0;
}

static char *slurp(const char *path)
{
    FILE *f = fopen(path, "rb");
    if (f == NULL) {
        return NULL;
    }
    fseek(f, 0, SEEK_END);
    long size = ftell(f);
    fseek(f, 0, SEEK_SET);
    char *text = malloc((size_t)size + 1);
    if (text != NULL && fread(text, 1, (size_t)size, f) != (size_t)size) {
        free(text);
        text = NULL;
    }
    if (text != NULL) {
        text[size] = '\0';
    }
    fclose(f);
    return text;
}

static void test_sync_conf(void)
{
    chorus_section(
        "A-12: the endpoint reads config/sync.conf, and it agrees with the Linux client");
    char path[512];
    chorus_repo_path(path, sizeof(path), "crates/client-linux/src/sync.rs");
    char *rust = slurp(path);
    chorus_check(rust != NULL, "the Linux client's sync.rs is readable");
    if (rust == NULL) {
        return;
    }
    struct {
        const char *rust_name;
        double value;
    } rows[] = {
        {"FILTER_WINDOW", sync_conf.filter_window},
        {"SMOOTHING_ALPHA", sync_conf.smoothing_alpha},
        {"HARD_RESYNC_THRESHOLD_US", sync_conf.hard_resync_threshold_us},
        {"MAX_CORRECTION_PPM", sync_conf.max_correction_ppm},
        {"STALENESS_LIMIT_MS", sync_conf.staleness_limit_ms},
        {"MAX_RTT_US", sync_conf.max_rtt_us},
        {"SYNC_INTERVAL_MS", sync_conf.sync_interval_ms},
        {"PLAYOUT_LATENCY_US", sync_conf.playout_latency_us},
        {"MUTE_US", sync_conf.mute_us},
    };
    for (size_t i = 0; i < sizeof(rows) / sizeof(rows[0]); i++) {
        double rust_value = 0.0;
        int found = rust_const(rust, rows[i].rust_name, &rust_value);
        chorus_check(found == 0 && rust_value == rows[i].value,
                     "sync.conf %s = %g on the endpoint, %g in crates/client-linux",
                     rows[i].rust_name, rows[i].value, rust_value);
    }
    free(rust);

    chorus_sync_conf_t broken;
    char detail[256];
    int refused =
        chorus_sync_conf_parse(&broken, "a sync.conf without mute_us",
                               "filter_window = 64\nsmoothing_alpha = 0.0625\n"
                               "hard_resync_threshold_us = 2000\nmax_correction_ppm = 300\n"
                               "staleness_limit_ms = 10000\nmax_rtt_us = 100000\n"
                               "sync_interval_ms = 500\nplayout_latency_us = 180000\n",
                               detail, sizeof(detail));
    chorus_check(refused != 0 && strstr(detail, "mute_us") != NULL,
                 "a sync.conf missing a key is refused by name (%s)", detail);

    chorus_playout_config_t c = make_config();
    chorus_check(
        c.servo.hard_resync_threshold_ns == sync_conf.hard_resync_threshold_us * 1000.0 &&
            c.servo.max_correction_ppm == sync_conf.max_correction_ppm &&
            c.playout_latency_ns == (uint64_t)sync_conf.playout_latency_us * 1000u &&
            c.interval_ms == sync_conf.sync_interval_ms,
        "the playout servo takes its threshold, clamp, latency and interval from sync.conf");
}

/* --- the interrupt hook and the error, step by step -------------------------- */

static void test_hook_and_error(void)
{
    chorus_section("the interrupt hook: DMA-consumed frames stamped on the monotonic clock");
    chorus_playout_config_t config = make_config();
    static chorus_playout_t p;
    fake_now = 5000000000ull;
    chorus_check(chorus_playout_init(&p, &config, ring_storage, chunk_storage, fake_clock) == 0,
                 "a playout initialises over caller storage (%zu bytes of buffer for %u frames)",
                 chorus_playout_ring_bytes(&config), config.capacity_frames);
    chorus_playout_dma_reset(&p);
    static uint8_t block[DMA_FRAMES * FRAME_OUT];
    /* The binding preloads the stopped channel with silence, as the board does. */
    chorus_playout_note_preloaded(&p, DMA_DESC * DMA_FRAMES);
    static uint8_t pcm[CHUNK_FRAMES * 6];
    make_chunk(pcm, 0);
    uint64_t ts = 7000000000ull;
    chorus_check(chorus_playout_offer(&p, ts, 1, CHORUS_FMT_PCM_S24LE, 2, RATE, pcm,
                                      CHUNK_FRAMES) == CHORUS_PLAYOUT_QUEUED,
                 "a chunk on the server timeline is queued");

    fake_now = 5005000000ull;
    chorus_playout_on_dma_sent(&p, DMA_FRAMES * FRAME_OUT);
    chorus_playout_report_t r;
    chorus_playout_set_offset(&p, 2000000000.0, fake_now);
    fake_now += 2000000ull; /* 2 ms into the next buffer */
    chorus_playout_observation_t kind = chorus_playout_observe(&p, &r);
    chorus_check(r.dma_consumed_frames == DMA_FRAMES && r.dma_stamp_ns == 5005000000ull,
                 "the hook added %" PRIu64 " consumed frames stamped at %" PRIu64
                 " ns, the monotonic instant of the interrupt",
                 r.dma_consumed_frames, r.dma_stamp_ns);
    double expected_delay = (double)(DMA_DESC * DMA_FRAMES - DMA_FRAMES) - 0.002 * RATE;
    chorus_check(fabs(r.device_delay_frames - expected_delay) < 1e-6,
                 "device delay = written %" PRIu64 " - consumed %" PRIu64
                 " - %.1f frames played since the stamp = %.1f frames (%.1f us)",
                 r.written_frames, r.dma_consumed_frames, 0.002 * RATE, r.device_delay_frames,
                 r.device_delay_ns / 1000.0);
    double expected_error = (double)ts + (double)config.playout_latency_ns -
                            ((double)fake_now + expected_delay * 1e9 / RATE + 2000000000.0);
    chorus_check(kind == CHORUS_PLAYOUT_HARD_RESYNC && fabs(r.error_ns - expected_error) < 1.0,
                 "the servo got error = next_write_ts + latency - (now + device delay + offset) "
                 "= %.1f us (%s, step %.1f us)",
                 r.error_ns / 1000.0, chorus_playout_observation_name(kind), r.step_ns / 1000.0);
    chorus_check(p.servo.updates == 1, "the error reached chorus_servo_update (%" PRIu32 " update)",
                 p.servo.updates);

    chorus_section("frames_played counts DMA-consumed audio only (A-9)");
    chorus_playout_stats_t st;
    chorus_playout_stats(&p, &st);
    chorus_check(st.frames_played == 0 && st.dma_consumed_frames == DMA_FRAMES,
                 "before any audio block is consumed: frames_played = %" PRIu64 " with %" PRIu64
                 " (silent) frames consumed",
                 st.frames_played, st.dma_consumed_frames);
    /* A fresh stream, so the block below is audio rather than the step's
     * silence. */
    chorus_playout_reset_stream(&p);
    p.acquired = 1; /* placed on the timeline, as by a zero-error tick */
    (void)chorus_playout_offer(&p, ts, 1, CHORUS_FMT_PCM_S24LE, 2, RATE, pcm, CHUNK_FRAMES);
    uint32_t audio = chorus_playout_fill(&p, block, DMA_FRAMES);
    chorus_playout_stats(&p, &st);
    chorus_check(st.frames_played == 0,
                 "a written block of %u frames (%u of them audio) is not played until the DMA "
                 "consumes it: frames_played = %" PRIu64,
                 DMA_FRAMES, audio, st.frames_played);
    for (uint32_t i = 0; i < DMA_DESC; i++) {
        chorus_playout_on_dma_sent(&p, DMA_FRAMES * FRAME_OUT);
    }
    chorus_playout_stats(&p, &st);
    chorus_check(audio == DMA_FRAMES && st.frames_played == audio &&
                     st.dma_interrupts == DMA_DESC + 1u,
                 "once the DMA has consumed it: frames_played = %" PRIu64 " (the block's audio)",
                 st.frames_played);

    chorus_section("the DMA running dry");
    chorus_playout_on_dma_sent(&p, DMA_FRAMES * FRAME_OUT);
    chorus_playout_stats(&p, &st);
    chorus_check(st.starved_frames == DMA_FRAMES,
                 "a buffer consumed that the writer never handed over is counted starved (%" PRIu64
                 " frames) and the timeline moves on from it",
                 st.starved_frames);
}

static void test_buffer_rules(void)
{
    chorus_section("the jitter buffer: late, duplicate, overflow, refused, underrun");
    chorus_playout_config_t config = make_config();
    static chorus_playout_t p;
    fake_now = 1000;
    (void)chorus_playout_init(&p, &config, ring_storage, chunk_storage, fake_clock);
    p.acquired = 1; /* the buffer's rules, apart from the loop */
    static uint8_t pcm[CHUNK_FRAMES * 6];
    make_chunk(pcm, 0);
    static uint8_t out[CHUNK_FRAMES * FRAME_OUT];
    uint64_t ts = 1000000000ull;
    (void)chorus_playout_offer(&p, ts, 10, CHORUS_FMT_PCM_S24LE, 2, RATE, pcm, CHUNK_FRAMES);
    chorus_check(chorus_playout_offer(&p, ts, 10, CHORUS_FMT_PCM_S24LE, 2, RATE, pcm,
                                      CHUNK_FRAMES) == CHORUS_PLAYOUT_DUPLICATE,
                 "a repeated sequence is a duplicate");
    uint32_t audio = chorus_playout_fill(&p, out, CHUNK_FRAMES);
    chorus_check(audio == CHUNK_FRAMES && out_left(out) == 1u && out_left(out + FRAME_OUT) == 2u,
                 "the chunk plays in order, converted to packed 24-bit slots");
    chorus_check(chorus_playout_offer(&p, ts + 10000000ull, 11, CHORUS_FMT_PCM_S24LE, 2, RATE, pcm,
                                      CHUNK_FRAMES) == CHORUS_PLAYOUT_LATE &&
                     p.stats.late_chunks == 1,
                 "a chunk stamped before the playout point (the end of what already played) is "
                 "dropped as late and counted");
    chorus_check(chorus_playout_offer(&p, ts + 20000000ull, 12, CHORUS_FMT_PCM_S24LE, 2, 44100, pcm,
                                      CHUNK_FRAMES) == CHORUS_PLAYOUT_REFUSED,
                 "a chunk at another rate is refused: one clock domain, no resampler");
    uint32_t more = chorus_playout_fill(&p, out, 100);
    chorus_check(more == 0 && p.stats.underrun_frames == 100 && out_left(out) == 0,
                 "an empty buffer after audio writes silence and counts an underrun of %" PRIu64
                 " frames",
                 p.stats.underrun_frames);
    uint32_t n = 0;
    chorus_playout_offer_t v = CHORUS_PLAYOUT_QUEUED;
    for (uint32_t seq = 20; v == CHORUS_PLAYOUT_QUEUED && seq < 200; seq++, n++) {
        v = chorus_playout_offer(&p, ts + (uint64_t)seq * 20000000ull, seq, CHORUS_FMT_PCM_S24LE, 2,
                                 RATE, pcm, CHUNK_FRAMES);
    }
    chorus_check(v == CHORUS_PLAYOUT_OVERFLOW && p.stats.overflow_chunks == 1,
                 "the buffer holds %u chunks of 20 ms (%u frames) and the next overflows", n - 1,
                 config.capacity_frames);

    chorus_section("formats into the I2S layout");
    chorus_playout_reset_stream(&p);
    p.acquired = 1;
    uint8_t s16[4] = {0x34, 0x12, 0xcd, 0xab};
    (void)chorus_playout_offer(&p, ts, 1, CHORUS_FMT_PCM_S16LE, 1, RATE, s16, 2);
    (void)chorus_playout_fill(&p, out, 2);
    chorus_check(out[0] == 0x00 && out[1] == 0x34 && out[2] == 0x12 && out[3] == 0x00 &&
                     out[4] == 0x34 && out[5] == 0x12,
                 "mono s16le 0x1234 is left-justified into both 24-bit slots (00 34 12)");
    float half = 0.5f;
    uint8_t f32[8];
    memcpy(f32, &half, 4);
    memcpy(f32 + 4, &half, 4);
    (void)chorus_playout_offer(&p, ts + 1000000ull, 2, CHORUS_FMT_PCM_F32LE, 2, RATE, f32, 1);
    (void)chorus_playout_fill(&p, out, 1);
    chorus_check(out[0] == 0x00 && out[1] == 0x00 && out[2] == 0x40,
                 "f32le 0.5 becomes 24-bit 0x400000");
}

static void test_modelled_runs(void)
{
    chorus_section("the modelled loop: a skewed fake DAC, jittered chunks, a noisy offset "
                   "(simulation, not timing evidence)");
    const double skews[] = {100.0, -100.0, 0.0};
    for (size_t i = 0; i < 3; i++) {
        scenario_t s;
        memset(&s, 0, sizeof(s));
        s.skew_ppm = skews[i];
        s.offset_ns = 3.25e9;
        s.seconds = 90.0;
        outcome_t o;
        lcg_state = 12345u + (uint32_t)i;
        chorus_check(run(&s, &o) == 0, "skew %+.0f ppm: the run completes", s.skew_ppm);
        chorus_check(o.stamp_matches,
                     "skew %+.0f ppm: all %" PRIu64
                     " interrupts stamped the monotonic instant they ran at",
                     s.skew_ppm, o.stats.dma_interrupts);
        chorus_check(o.delay_matches && o.error_matches,
                     "skew %+.0f ppm: every one of %u ticks formed its device delay from the DMA "
                     "counts and its error from that delay",
                     s.skew_ppm, o.observations);
        chorus_check(o.stats.hard_resyncs == 1 && o.hard_after_acquisition == 0,
                     "skew %+.0f ppm: one hard resync (the acquisition) and none after (%" PRIu32
                     ")",
                     s.skew_ppm, o.stats.hard_resyncs);
        chorus_check(fabs(o.corr_mean_ppm + s.skew_ppm) < 5.0,
                     "skew %+.0f ppm: the servo's correction settles at %.2f ppm (inserting or "
                     "dropping to cancel the DAC's rate error)",
                     s.skew_ppm, o.corr_mean_ppm);
        chorus_check(o.true_err_max_us < 100.0 && o.own_err_max_us < 100.0,
                     "skew %+.0f ppm: last 20 s, true error at the pins max %.1f us, the servo's "
                     "error max %.1f us (bound 100 us)",
                     s.skew_ppm, o.true_err_max_us, o.own_err_max_us);
        chorus_check(o.stats.frames_played == o.audio_in_consumed_blocks &&
                         o.stats.frames_played < o.audio_written,
                     "skew %+.0f ppm: frames_played %" PRIu64
                     " = audio in DMA-consumed blocks, fewer than the %" PRIu64 " written",
                     s.skew_ppm, o.stats.frames_played, o.audio_written);
        chorus_check(o.stats.underrun_frames == 0 && o.stats.late_chunks == 0 &&
                         o.stats.overflow_chunks == 0,
                     "skew %+.0f ppm: no underrun, late or overflowed chunk once acquired (%" PRIu64
                     " stale chunks given up while acquiring)",
                     s.skew_ppm, o.stats.discarded_before_start);
    }

    scenario_t step;
    memset(&step, 0, sizeof(step));
    step.skew_ppm = 50.0;
    step.offset_ns = 1e9;
    step.seconds = 90.0;
    step.bias_from_s = 40.0;
    step.offset_bias_ns = 5e6;
    outcome_t o;
    lcg_state = 777u;
    (void)run(&step, &o);
    chorus_check(o.stats.hard_resyncs == 2 && o.stats.muted_frames >= 960u,
                 "a 5 ms step in the offset: a second hard resync under a mute of %" PRIu64
                 " frames (%" PRIu32 " in all)",
                 o.stats.muted_frames, o.stats.hard_resyncs);
    chorus_check(o.own_err_max_us < 100.0,
                 "after the step the servo's error settles again (max %.1f us, last 20 s)",
                 o.own_err_max_us);

    scenario_t gap;
    memset(&gap, 0, sizeof(gap));
    gap.skew_ppm = -30.0;
    gap.offset_ns = 2e9;
    gap.seconds = 90.0;
    gap.gap_from_s = 30.0;
    gap.gap_s = 0.4;
    lcg_state = 4242u;
    (void)run(&gap, &o);
    chorus_check(o.stats.underrun_frames > 0,
                 "400 ms of lost chunks: an underrun of %" PRIu64 " frames written as silence and "
                 "counted",
                 o.stats.underrun_frames);
    chorus_check(o.true_err_max_us < 100.0,
                 "after the underrun the loop re-acquires (true error max %.1f us, last 20 s)",
                 o.true_err_max_us);

    scenario_t dry;
    memset(&dry, 0, sizeof(dry));
    dry.skew_ppm = 20.0;
    dry.offset_ns = 1e9;
    dry.seconds = 90.0;
    dry.skip_fill_at_event = 4000;
    lcg_state = 99u;
    (void)run(&dry, &o);
    chorus_check(o.stats.starved_frames == DMA_FRAMES && o.true_err_max_us < 100.0,
                 "a writer stalled past its DMA queue: %" PRIu64
                 " starved frames counted, and the loop recovers (true error max %.1f us)",
                 o.stats.starved_frames, o.true_err_max_us);
}

int main(void)
{
    char path[512];
    char detail[256];
    chorus_repo_path(path, sizeof(path), "config/sync.conf");
    if (chorus_sync_conf_load(&sync_conf, path, detail, sizeof(detail)) != 0) {
        printf("FAIL config/sync.conf: %s\n", detail);
        return 1;
    }
    test_sync_conf();
    test_hook_and_error();
    test_buffer_rules();
    test_modelled_runs();
    return chorus_test_report("test_playout");
}
