/* The endpoint's playout path: the jitter buffer, the I2S writer's source of
 * frames, the DMA-consumed-frames measure and the servo it feeds (audit A-9,
 * chorus goal 8).
 *
 * The Linux client's design, on a board (crates/client-linux/src/sync.rs,
 * buffer.rs and run.rs). There the device delay is ALSA's `snd_pcm_delay`;
 * here there is no ALSA, so the endpoint measures it itself: every frame the
 * writer hands the I2S driver is counted as written, and every DMA buffer the
 * controller finishes sending is counted as consumed, IN THE INTERRUPT, and
 * stamped there on the monotonic clock. Written minus consumed, less the part
 * of the current buffer that has played since that stamp, is how far the next
 * frame written is from the pins. The error the servo corrects is formed from
 * it exactly as the Linux client forms it:
 *
 *     error_ns = (next_write_ts_ns + playout_latency_ns)
 *                - (now_ns + device_delay_ns + offset_ns)
 *
 * positive meaning playout is AHEAD, the sign crates/sync's servo uses. A fine
 * correction inserts or drops whole frames at the rate the servo names; a hard
 * resync steps the playout pointer under a mute. Both change WHAT is written,
 * never when, for the reason the Linux module gives.
 *
 * Pure and host-graded: no ESP-IDF header, no task, no allocation. The caller
 * hands in every buffer and a clock, and on the board firmware/main/ binds the
 * interrupt hook to the I2S driver's `on_sent` callback and the writer to
 * `i2s_channel_write`. firmware/tests/test_playout.c drives it with a fake DMA
 * on a fake monotonic clock. Nothing here has been heard on hardware: a timing
 * claim about it needs a hardware report in docs/measurements/. */

#ifndef CHORUS_PLAYOUT_H
#define CHORUS_PLAYOUT_H

#include <stddef.h>
#include <stdint.h>

#include "chorus/sync.h"
#include "chorus/sync_conf.h"

/* The jitter buffer the board allocates, and the `buffer` this endpoint's
 * capabilities advertise (session.c): the playout latency (180 ms in
 * config/sync.conf) less what the DMA holds (6 x 240 frames, 30 ms, in
 * firmware/config/endpoint.conf), plus a 20 ms chunk in flight and headroom.
 * ASSUMED until a bench run measures the level it actually holds. */
#define CHORUS_PLAYOUT_BUFFER_MS 200u

/* Mono is carried to both slots; more than two channels is chorus#TV-9's. */
#define CHORUS_PLAYOUT_MAX_CHANNELS 2u

/* Blocks written but not yet known to be consumed, for the "played" count.
 * The DMA holds at most dma_desc_num blocks (6 in endpoint.conf) plus the one
 * the writer is blocked on. */
#define CHORUS_PLAYOUT_MAX_BLOCKS 32u

/* The monotonic clock, injectable: chorus_monotonic_now_ns on the board, a
 * fake in the test. It is read in the interrupt, so on the board it has to be
 * ISR-safe (esp_timer_get_time is; docs/decisions/ records the source). */
typedef uint64_t (*chorus_playout_clock_fn)(void);

/* Serialises the session task (offer, offset) against the writer task (fill,
 * observe). NULL on a single-threaded host. Never taken in the interrupt. */
typedef void (*chorus_playout_lock_fn)(void *ctx);

typedef struct {
    uint32_t rate_hz;
    /* Slots per frame on the I2S line: 2 (stereo). */
    uint8_t channels;
    /* Bytes per slot in the buffer the I2S driver takes: 2, 3 or 4. At a
     * 24-bit slot width the ESP32-S3 driver takes packed 3-byte samples. */
    uint8_t out_sample_bytes;
    /* The jitter buffer, in frames, and the chunk descriptors beside it. */
    uint32_t capacity_frames;
    uint32_t max_chunks;
    /* Frames per DMA buffer: how far the interpolation inside one buffer may
     * run before the next interrupt is due. */
    uint32_t dma_frame_num;
    uint64_t playout_latency_ns;
    uint64_t staleness_limit_ns;
    uint64_t mute_ns;
    uint32_t interval_ms;
    chorus_servo_config_t servo;
} chorus_playout_config_t;

/* The playout configuration config/sync.conf and the I2S clock imply. */
chorus_playout_config_t chorus_playout_config_from(const chorus_sync_conf_t *sync, uint32_t rate_hz,
                                                   uint8_t slot_bit_width, uint32_t dma_frame_num,
                                                   uint32_t buffer_ms);

/* Bytes of jitter buffer and chunk descriptors a configuration needs. */
size_t chorus_playout_ring_bytes(const chorus_playout_config_t *config);
size_t chorus_playout_chunk_bytes(const chorus_playout_config_t *config);

typedef struct {
    uint64_t timestamp_ns;
    uint32_t frames;
    uint32_t sequence;
} chorus_playout_chunk_t;

typedef struct {
    uint64_t written_end; /* written_frames when the block was handed over */
    uint32_t audio_frames;
} chorus_playout_block_t;

typedef enum {
    CHORUS_PLAYOUT_QUEUED = 0,
    CHORUS_PLAYOUT_DUPLICATE,
    CHORUS_PLAYOUT_LATE,
    CHORUS_PLAYOUT_OVERFLOW,
    CHORUS_PLAYOUT_REFUSED
} chorus_playout_offer_t;

const char *chorus_playout_offer_name(chorus_playout_offer_t offer);

typedef enum {
    CHORUS_PLAYOUT_NO_DATA = 0,
    CHORUS_PLAYOUT_NO_OFFSET,
    CHORUS_PLAYOUT_STALE,
    CHORUS_PLAYOUT_FINE,
    CHORUS_PLAYOUT_HARD_RESYNC
} chorus_playout_observation_t;

const char *chorus_playout_observation_name(chorus_playout_observation_t observation);

/* What one observation formed, for telemetry and for the test. */
typedef struct {
    chorus_playout_observation_t kind;
    uint64_t now_ns;
    /* The DMA side's snapshot this observation used. */
    uint64_t dma_consumed_frames;
    uint64_t dma_stamp_ns;
    uint64_t written_frames;
    /* Frames between the next frame written and the pins, and the same in ns:
     * the device-delay term of the error. */
    double device_delay_frames;
    double device_delay_ns;
    uint64_t next_write_ts_ns;
    double offset_ns;
    double error_ns;
    double correction_ppm;
    double step_ns;
} chorus_playout_report_t;

typedef struct {
    uint64_t dma_consumed_frames;
    uint64_t dma_interrupts;
    uint64_t frames_played;
    uint64_t written_frames;
    uint64_t underrun_frames;
    uint64_t starved_frames;
    uint64_t late_chunks;
    uint64_t duplicate_chunks;
    uint64_t overflow_chunks;
    uint64_t refused_chunks;
    /* Chunks dropped from the front while the loop was acquiring and the
     * buffer was full: the oldest are the stalest. */
    uint64_t discarded_before_start;
    uint64_t inserted_frames;
    uint64_t dropped_frames;
    uint64_t muted_frames;
    uint32_t hard_resyncs;
    uint32_t fine_corrections;
    uint32_t queued_frames;
    double correction_ppm;
    int error_known;
    double last_error_ns;
} chorus_playout_stats_t;

typedef struct {
    chorus_playout_config_t config;
    chorus_playout_clock_fn now_ns;
    chorus_playout_lock_fn lock;
    chorus_playout_lock_fn unlock;
    void *lock_ctx;

    /* The jitter buffer: frames already converted to the I2S layout. */
    uint8_t *ring;
    uint32_t ring_read;
    uint32_t ring_count;
    chorus_playout_chunk_t *chunks;
    uint32_t chunk_read;
    uint32_t chunk_count;
    uint32_t front_taken;
    int have_highwater;
    uint32_t highwater;
    int have_playout_ts;
    uint64_t playout_ts_ns;

    /* The DMA side. Written ONLY by chorus_playout_on_dma_sent, in the
     * interrupt; read by the writer task through the sequence counter, which
     * is odd while the interrupt is writing. 32-bit counters, because a
     * 32-bit store is a single instruction on the ESP32-S3 and a 64-bit one is
     * not; the task extends them to 64 bits by their differences. */
    volatile uint32_t dma_seq;
    volatile uint32_t dma_consumed32;
    volatile uint32_t dma_interrupts32;
    volatile uint32_t dma_stamp_lo;
    volatile uint32_t dma_stamp_hi;
    /* Frames the DMA sent that the writer had not handed over (the driver's
     * auto-cleared zeros), counted by the interrupt against written32. */
    volatile uint32_t dma_starved32;
    /* written_frames, published by the writer for the interrupt. */
    volatile uint32_t written32;

    /* The task side's 64-bit view of the DMA counters. */
    uint32_t seen_consumed32;
    uint64_t dma_consumed;
    uint32_t seen_interrupts32;
    uint64_t dma_interrupts;
    uint32_t seen_starved32;

    uint64_t written_frames;
    chorus_playout_block_t blocks[CHORUS_PLAYOUT_MAX_BLOCKS];
    uint32_t block_read;
    uint32_t block_count;
    uint64_t frames_played;

    /* The offset, from the session's filter. */
    int has_offset;
    double offset_ns;
    uint64_t offset_at_ns;

    /* The servo and the corrector (crates/client-linux's PlayoutCorrector). */
    chorus_servo_t servo;
    double correction_ppm;
    double pending_frames;
    uint64_t mute_frames;
    uint64_t last_advance_ns;
    int advancing;
    /* Whether the loop has placed this stream on the timeline yet. Until the
     * first observation that forms an error, the writer holds the buffer and
     * writes silence: audio played before the offset is known would be played
     * at an instant nobody chose. */
    int acquired;

    chorus_playout_stats_t stats;
    chorus_playout_report_t last;
} chorus_playout_t;

/* Initialise over caller-owned storage of chorus_playout_ring_bytes and
 * chorus_playout_chunk_bytes. Returns 0, or -1 when the configuration cannot
 * be played (zero rate, an unsupported slot size, no storage). */
int chorus_playout_init(chorus_playout_t *p, const chorus_playout_config_t *config, uint8_t *ring,
                        chorus_playout_chunk_t *chunks, chorus_playout_clock_fn now_ns);

void chorus_playout_set_lock(chorus_playout_t *p, chorus_playout_lock_fn lock,
                             chorus_playout_lock_fn unlock, void *ctx);

/* THE INTERRUPT HOOK. Called from the I2S TX `on_sent` callback with the size
 * of the DMA buffer that just finished sending: adds its frames to the
 * consumed count and stamps them on the monotonic clock, here, in the
 * interrupt. No lock, no float, no call but the clock. */
void chorus_playout_on_dma_sent(chorus_playout_t *p, size_t bytes);

/* Forget every DMA count: called with the I2S channel stopped, before it is
 * preloaded and enabled, so that written and consumed count the same frames
 * from the same instant. */
void chorus_playout_dma_reset(chorus_playout_t *p);

/* `frames` of silence were preloaded into the DMA buffers while the channel
 * was stopped (i2s_channel_preload_data): they count as written, so that
 * written and consumed count the same frames from the first interrupt. */
void chorus_playout_note_preloaded(chorus_playout_t *p, uint32_t frames);

/* A chunk of PCM on the server timeline, from the session task. `sample_format`
 * is the wire byte (chorus/protocol.h). */
chorus_playout_offer_t chorus_playout_offer(chorus_playout_t *p, uint64_t timestamp_ns,
                                            uint32_t sequence, uint8_t sample_format,
                                            uint16_t channels, uint32_t rate_hz, const uint8_t *pcm,
                                            uint32_t frames);

/* The session's filtered offset (server minus endpoint), taken at `at_ns` on
 * the endpoint's monotonic clock. */
void chorus_playout_set_offset(chorus_playout_t *p, double offset_ns, uint64_t at_ns);

/* A new stream (or a lost session): the buffer, the highwater, the playout
 * point, the servo and the offset are forgotten. The DMA counts are kept. */
void chorus_playout_reset_stream(chorus_playout_t *p);

/* The writer: fill `out` with exactly `frames` frames in the I2S layout and
 * count them as written. Returns how many of them are audio (the rest is
 * silence: an insertion, an underrun, or nothing to play yet). */
uint32_t chorus_playout_fill(chorus_playout_t *p, uint8_t *out, uint32_t frames);

/* One tick of the loop, from the writer task every interval_ms: snapshot the
 * DMA side, form the error, feed the servo and set the corrector. */
chorus_playout_observation_t chorus_playout_observe(chorus_playout_t *p,
                                                    chorus_playout_report_t *report);

void chorus_playout_stats(chorus_playout_t *p, chorus_playout_stats_t *out);

/* Whether the loop has acquired the timeline: the writer ticks the loop at
 * every buffer until it has, then every interval_ms. */
int chorus_playout_acquired(chorus_playout_t *p);

#endif /* CHORUS_PLAYOUT_H */
