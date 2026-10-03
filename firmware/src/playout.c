#include "chorus/playout.h"

#include <string.h>

#include "chorus/protocol.h"

#define NS_PER_S 1000000000.0

const char *chorus_playout_offer_name(chorus_playout_offer_t offer)
{
    switch (offer) {
    case CHORUS_PLAYOUT_QUEUED:
        return "queued";
    case CHORUS_PLAYOUT_DUPLICATE:
        return "duplicate";
    case CHORUS_PLAYOUT_LATE:
        return "late";
    case CHORUS_PLAYOUT_OVERFLOW:
        return "overflow";
    case CHORUS_PLAYOUT_REFUSED:
        return "refused";
    }
    return "unknown";
}

const char *chorus_playout_observation_name(chorus_playout_observation_t observation)
{
    switch (observation) {
    case CHORUS_PLAYOUT_NO_DATA:
        return "no-data";
    case CHORUS_PLAYOUT_NO_OFFSET:
        return "no-offset";
    case CHORUS_PLAYOUT_STALE:
        return "stale";
    case CHORUS_PLAYOUT_FINE:
        return "fine";
    case CHORUS_PLAYOUT_HARD_RESYNC:
        return "hard-resync";
    }
    return "unknown";
}

chorus_playout_config_t chorus_playout_config_from(const chorus_sync_conf_t *sync, uint32_t rate_hz,
                                                   uint8_t slot_bit_width, uint32_t dma_frame_num,
                                                   uint32_t buffer_ms)
{
    chorus_playout_config_t c;
    memset(&c, 0, sizeof(c));
    c.rate_hz = rate_hz;
    c.channels = 2;
    c.out_sample_bytes = (uint8_t)(slot_bit_width / 8u);
    c.capacity_frames = (uint32_t)((uint64_t)rate_hz * buffer_ms / 1000u);
    /* A chunk of 5 ms is the shortest the server cuts (config/verification.conf
     * and the server's --chunk-us); one descriptor per 5 ms of buffer. */
    c.max_chunks = buffer_ms / 5u + 1u;
    c.dma_frame_num = dma_frame_num;
    c.playout_latency_ns = (uint64_t)sync->playout_latency_us * 1000u;
    c.staleness_limit_ns = (uint64_t)sync->staleness_limit_ms * 1000000u;
    c.mute_ns = (uint64_t)sync->mute_us * 1000u;
    c.interval_ms = sync->sync_interval_ms;
    c.servo = chorus_servo_config_default();
    c.servo.max_correction_ppm = sync->max_correction_ppm;
    c.servo.hard_resync_threshold_ns = (double)sync->hard_resync_threshold_us * 1000.0;
    c.servo.filter_window = sync->filter_window;
    c.servo.smoothing_alpha = sync->smoothing_alpha;
    c.max_volume_thousandths = CHORUS_VOLUME_DEFAULT_CEILING;
    return c;
}

static uint32_t frame_bytes(const chorus_playout_config_t *c)
{
    return (uint32_t)c->channels * c->out_sample_bytes;
}

size_t chorus_playout_ring_bytes(const chorus_playout_config_t *config)
{
    return (size_t)config->capacity_frames * frame_bytes(config);
}

size_t chorus_playout_chunk_bytes(const chorus_playout_config_t *config)
{
    return (size_t)config->max_chunks * sizeof(chorus_playout_chunk_t);
}

static void take_lock(chorus_playout_t *p)
{
    if (p->lock != NULL) {
        p->lock(p->lock_ctx);
    }
}

static void give_lock(chorus_playout_t *p)
{
    if (p->unlock != NULL) {
        p->unlock(p->lock_ctx);
    }
}

int chorus_playout_init(chorus_playout_t *p, const chorus_playout_config_t *config, uint8_t *ring,
                        chorus_playout_chunk_t *chunks, chorus_playout_clock_fn now_ns)
{
    memset(p, 0, sizeof(*p));
    if (config->rate_hz == 0 || config->channels == 0 ||
        config->channels > CHORUS_PLAYOUT_MAX_CHANNELS ||
        (config->out_sample_bytes != 2 && config->out_sample_bytes != 3 &&
         config->out_sample_bytes != 4) ||
        config->capacity_frames == 0 || config->max_chunks == 0 || config->interval_ms == 0 ||
        config->max_volume_thousandths > CHORUS_VOLUME_FULL || ring == NULL || chunks == NULL ||
        now_ns == NULL) {
        return -1;
    }
    p->config = *config;
    p->ring = ring;
    p->chunks = chunks;
    p->now_ns = now_ns;
    chorus_servo_init(&p->servo, config->servo);
    chorus_volume_init(&p->volume, config->max_volume_thousandths);
    /* The marker's ns per frame in 16.16, for the interrupt. Below 16 kHz it
     * does not fit 32 bits; no chorus stream is that slow, and the marker is
     * then simply off. */
    uint64_t q16 = (1000000000ull << 16) / config->rate_hz;
    p->frame_ns_q16 = (q16 <= 0xffffffffull) ? (uint32_t)q16 : 0u;
    return 0;
}

void chorus_playout_set_lock(chorus_playout_t *p, chorus_playout_lock_fn lock,
                             chorus_playout_lock_fn unlock, void *ctx)
{
    p->lock = lock;
    p->unlock = unlock;
    p->lock_ctx = ctx;
}

/* --- the interrupt ------------------------------------------------------------ */

/* Runs in the I2S interrupt. The image leaves CONFIG_I2S_ISR_IRAM_SAFE off (the
 * ESP-IDF default), so neither this nor the clock it reads has to be in IRAM;
 * the decision record says when that changes. */
void chorus_playout_on_dma_sent(chorus_playout_t *p, size_t bytes)
{
    uint32_t fb = (uint32_t)p->config.channels * p->config.out_sample_bytes;
    uint32_t frames = (fb == 0) ? 0 : (uint32_t)(bytes / fb);
    /* The stamp is taken HERE, in the interrupt, on the monotonic clock: the
     * instant this buffer's last frame left memory for the I2S controller. */
    uint64_t stamp = p->now_ns();
    p->dma_seq = p->dma_seq + 1u; /* odd: a write is in progress */
    __atomic_thread_fence(__ATOMIC_SEQ_CST);
    /* Frames the writer had handed over and the DMA had not yet sent. A
     * buffer beyond them is the driver's auto-cleared zeros: the DMA ran dry. */
    int32_t ahead = (int32_t)(p->written32 - p->dma_consumed32);
    if (ahead < 0) {
        ahead = 0;
    }
    if (frames > (uint32_t)ahead) {
        p->dma_starved32 = p->dma_starved32 + (frames - (uint32_t)ahead);
    }
    p->dma_consumed32 = p->dma_consumed32 + frames;
    p->dma_interrupts32 = p->dma_interrupts32 + 1u;
    p->dma_stamp_lo = (uint32_t)stamp;
    p->dma_stamp_hi = (uint32_t)(stamp >> 32);
    __atomic_thread_fence(__ATOMIC_SEQ_CST);
    p->dma_seq = p->dma_seq + 1u; /* even: consistent */
}

/* Runs in the interrupt, right after chorus_playout_on_dma_sent: the buffer
 * that starts at the pins now holds frames [consumed, consumed + dma_frame_num)
 * of the written count, the same model the device delay uses (a frame
 * consumed is a frame at the pins). */
int chorus_playout_marker_due(chorus_playout_t *p, uint32_t *delay_ns, uint32_t *level)
{
    if (p->marker_pending == 0u) {
        return 0;
    }
    uint32_t into = p->marker_frame32 - p->dma_consumed32;
    if ((int32_t)into < 0) {
        /* The frame's buffer started without this call seeing it. */
        p->marker_missed32 = p->marker_missed32 + 1u;
        p->marker_pending = 0u;
        return 0;
    }
    if (into >= p->config.dma_frame_num) {
        return 0;
    }
    uint64_t ns = ((uint64_t)into * p->frame_ns_q16) >> 16;
    uint32_t lead = p->marker_lead_ns;
    *delay_ns = (ns > lead) ? (uint32_t)(ns - lead) : 0u;
    *level = p->marker_level;
    p->marker_edges32 = p->marker_edges32 + 1u;
    p->marker_pending = 0u;
    return 1;
}

void chorus_playout_dma_reset(chorus_playout_t *p)
{
    p->dma_seq = 0;
    p->dma_consumed32 = 0;
    p->dma_interrupts32 = 0;
    p->dma_stamp_lo = 0;
    p->dma_stamp_hi = 0;
    p->dma_starved32 = 0;
    p->written32 = 0;
    __atomic_thread_fence(__ATOMIC_SEQ_CST);
    p->seen_starved32 = 0;
    p->seen_consumed32 = 0;
    p->dma_consumed = 0;
    p->seen_interrupts32 = 0;
    p->dma_interrupts = 0;
    p->written_frames = 0;
    p->block_read = 0;
    p->block_count = 0;
    p->marker_pending = 0u;
}

/* Read the interrupt's counters consistently: retry while the sequence is odd
 * or moved underneath the read. Extends them to 64 bits by difference. */
static uint64_t dma_snapshot(chorus_playout_t *p)
{
    uint32_t seq, consumed, interrupts, lo, hi, starved;
    for (;;) {
        seq = p->dma_seq;
        __atomic_thread_fence(__ATOMIC_SEQ_CST);
        starved = p->dma_starved32;
        consumed = p->dma_consumed32;
        interrupts = p->dma_interrupts32;
        lo = p->dma_stamp_lo;
        hi = p->dma_stamp_hi;
        __atomic_thread_fence(__ATOMIC_SEQ_CST);
        if ((seq & 1u) == 0 && seq == p->dma_seq) {
            break;
        }
    }
    p->dma_consumed += (uint32_t)(consumed - p->seen_consumed32);
    p->seen_consumed32 = consumed;
    p->dma_interrupts += (uint32_t)(interrupts - p->seen_interrupts32);
    p->seen_interrupts32 = interrupts;

    /* The DMA ran dry: the driver sent zeros (auto_clear) the writer never
     * handed it. Those zeros were played, so they count as written silence
     * and as starvation, and the timeline moves on from them; everything
     * written since plays that much later, and the servo sees it as error. */
    uint32_t gap = starved - p->seen_starved32;
    p->seen_starved32 = starved;
    if (gap > 0) {
        p->stats.starved_frames += gap;
        p->written_frames += gap;
        for (uint32_t i = 0; i < p->block_count; i++) {
            p->blocks[(p->block_read + i) % CHORUS_PLAYOUT_MAX_BLOCKS].written_end += gap;
        }
        p->written32 = (uint32_t)p->written_frames;
        if (p->marker_pending != 0u) {
            /* The armed frame now plays `gap` frames later than it was
             * counted; an edge from the stale index would mark the wrong
             * instant, so the boundary is dropped rather than moved under the
             * interrupt's feet. An underrun spoils the run anyway. */
            p->marker_pending = 0u;
            p->stats.marker_missed++;
        }
    }
    if (p->dma_consumed > p->written_frames) {
        p->written_frames = p->dma_consumed;
        p->written32 = (uint32_t)p->written_frames;
    }
    /* Blocks whose last frame the DMA has passed are played. */
    while (p->block_count > 0 && p->blocks[p->block_read].written_end <= p->dma_consumed) {
        p->frames_played += p->blocks[p->block_read].audio_frames;
        p->block_read = (p->block_read + 1u) % CHORUS_PLAYOUT_MAX_BLOCKS;
        p->block_count--;
    }
    return ((uint64_t)hi << 32) | lo;
}

/* --- the jitter buffer -------------------------------------------------------- */

/* One sample, left-justified in 32 bits, from the wire. */
static int32_t read_sample(uint8_t format, const uint8_t *s)
{
    switch (format) {
    case CHORUS_FMT_PCM_S16LE:
        return (int32_t)((uint32_t)s[0] << 16 | (uint32_t)s[1] << 24);
    case CHORUS_FMT_PCM_S24LE:
        return (int32_t)((uint32_t)s[0] << 8 | (uint32_t)s[1] << 16 | (uint32_t)s[2] << 24);
    case CHORUS_FMT_PCM_F32LE: {
        uint32_t bits =
            (uint32_t)s[0] | (uint32_t)s[1] << 8 | (uint32_t)s[2] << 16 | (uint32_t)s[3] << 24;
        float f;
        memcpy(&f, &bits, sizeof(f));
        if (!(f > -1.0f)) {
            return INT32_MIN; /* also NaN */
        }
        if (f >= 1.0f) {
            return INT32_MAX;
        }
        return (int32_t)((double)f * 2147483648.0);
    }
    default:
        return 0;
    }
}

/* One sample into the I2S layout: little-endian, the top `bytes` bytes of the
 * left-justified value. */
static void write_sample(int32_t value, uint8_t bytes, uint8_t *out)
{
    uint32_t v = (uint32_t)value;
    for (uint8_t i = 0; i < bytes; i++) {
        out[i] = (uint8_t)(v >> (8u * (4u - bytes + i)));
    }
}

/* Take `n` frames (at most what the front chunk still holds) off the front. */
static void consume_front(chorus_playout_t *p, uint32_t n)
{
    const chorus_playout_config_t *c = &p->config;
    chorus_playout_chunk_t *front = &p->chunks[p->chunk_read];
    p->ring_read = (p->ring_read + n) % c->capacity_frames;
    p->ring_count -= n;
    p->front_taken += n;
    if (p->front_taken >= front->frames) {
        /* One chunk past the last chunk that went to the device, on the
         * server timeline: what makes a later chunk "already in the past". */
        p->have_playout_ts = 1;
        p->playout_ts_ns =
            front->timestamp_ns + (uint64_t)front->frames * 1000000000ull / c->rate_hz;
        p->chunk_read = (p->chunk_read + 1u) % c->max_chunks;
        p->chunk_count--;
        p->front_taken = 0;
    }
}

chorus_playout_offer_t chorus_playout_offer(chorus_playout_t *p, uint64_t timestamp_ns,
                                            uint32_t sequence, uint8_t sample_format,
                                            uint16_t channels, uint32_t rate_hz, const uint8_t *pcm,
                                            uint32_t frames)
{
    const chorus_playout_config_t *c = &p->config;
    size_t in_bytes = chorus_sample_format_bytes(sample_format);
    chorus_playout_offer_t verdict = CHORUS_PLAYOUT_QUEUED;
    take_lock(p);

    /* The order is the Linux buffer's: a duplicate is a duplicate whatever the
     * level, a late chunk is late whatever the level, and only a chunk that
     * would otherwise have played is discarded for overflow. */
    if (p->have_highwater &&
        (sequence - p->highwater == 0u || sequence - p->highwater > UINT32_MAX / 2u)) {
        verdict = CHORUS_PLAYOUT_DUPLICATE;
        p->stats.duplicate_chunks++;
    } else if (p->have_playout_ts && timestamp_ns < p->playout_ts_ns) {
        verdict = CHORUS_PLAYOUT_LATE;
        p->stats.late_chunks++;
    } else if (rate_hz != c->rate_hz || channels == 0 || channels > CHORUS_PLAYOUT_MAX_CHANNELS ||
               in_bytes == 0 || frames == 0 || pcm == NULL) {
        /* No resampler and no downmix on the endpoint: one clock domain. */
        verdict = CHORUS_PLAYOUT_REFUSED;
        p->stats.refused_chunks++;
    } else if (frames > c->capacity_frames) {
        verdict = CHORUS_PLAYOUT_REFUSED;
        p->stats.refused_chunks++;
    } else {
        /* While acquiring, a full buffer gives up its OLDEST chunks: nothing
         * has played, and the newest chunk is the one nearest its due time. */
        while (!p->acquired && p->chunk_count > 0 &&
               (p->ring_count + frames > c->capacity_frames || p->chunk_count == c->max_chunks)) {
            uint32_t rest = p->chunks[p->chunk_read].frames - p->front_taken;
            p->ring_read = (p->ring_read + rest) % c->capacity_frames;
            p->ring_count -= rest;
            p->chunk_read = (p->chunk_read + 1u) % c->max_chunks;
            p->chunk_count--;
            p->front_taken = 0;
            p->stats.discarded_before_start++;
        }
        /* A drop the corrector owes is taken now rather than at the next
         * write when the buffer is full: the same frames go either way, and
         * the chunk that would otherwise overflow is kept. */
        while (p->acquired && p->chunk_count > 0 && p->pending_frames >= 1.0 &&
               (p->ring_count + frames > c->capacity_frames || p->chunk_count == c->max_chunks)) {
            uint32_t in_front = p->chunks[p->chunk_read].frames - p->front_taken;
            uint64_t owed = (uint64_t)p->pending_frames;
            uint32_t n = (owed < in_front) ? (uint32_t)owed : in_front;
            consume_front(p, n);
            p->pending_frames -= (double)n;
            p->stats.dropped_frames += n;
        }
    }
    if (verdict == CHORUS_PLAYOUT_QUEUED &&
        (p->ring_count + frames > c->capacity_frames || p->chunk_count == c->max_chunks)) {
        verdict = CHORUS_PLAYOUT_OVERFLOW;
        p->stats.overflow_chunks++;
    }
    if (verdict == CHORUS_PLAYOUT_QUEUED) {
        uint32_t fb = frame_bytes(c);
        uint32_t at = (p->ring_read + p->ring_count) % c->capacity_frames;
        for (uint32_t f = 0; f < frames; f++) {
            const uint8_t *src = pcm + (size_t)f * channels * in_bytes;
            uint8_t *dst = p->ring + (size_t)at * fb;
            for (uint8_t ch = 0; ch < c->channels; ch++) {
                /* Mono goes to every slot. */
                uint8_t from = (channels == 1) ? 0 : ch;
                int32_t v = read_sample(sample_format, src + (size_t)from * in_bytes);
                write_sample(v, c->out_sample_bytes, dst + (size_t)ch * c->out_sample_bytes);
            }
            at = (at + 1u) % c->capacity_frames;
        }
        p->ring_count += frames;
        chorus_playout_chunk_t *d = &p->chunks[(p->chunk_read + p->chunk_count) % c->max_chunks];
        d->timestamp_ns = timestamp_ns;
        d->frames = frames;
        d->sequence = sequence;
        p->chunk_count++;
        p->have_highwater = 1;
        p->highwater = sequence;
    }
    give_lock(p);
    return verdict;
}

void chorus_playout_set_offset(chorus_playout_t *p, double offset_ns, uint64_t at_ns)
{
    take_lock(p);
    p->has_offset = 1;
    p->offset_ns = offset_ns;
    p->offset_at_ns = at_ns;
    give_lock(p);
}

void chorus_playout_set_room_volume(chorus_playout_t *p, uint16_t gain, uint16_t limit,
                                    uint16_t ramp_ms)
{
    take_lock(p);
    chorus_volume_set(&p->volume, gain, limit, ramp_ms, p->config.rate_hz);
    give_lock(p);
}

void chorus_playout_set_dsp(chorus_playout_t *p, chorus_endpoint_dsp_t *dsp)
{
    take_lock(p);
    p->dsp = dsp;
    give_lock(p);
}

void chorus_playout_set_stream_layout(chorus_playout_t *p, uint32_t channels, const uint8_t *map)
{
    take_lock(p);
    if (p->dsp != NULL) {
        /* The I2S clock is the playout path's: a stream at another rate is
         * refused chunk by chunk, so the chain is built for this one. */
        chorus_endpoint_dsp_set_stream(p->dsp, p->config.rate_hz, channels, map);
    }
    give_lock(p);
}

void chorus_playout_set_sound(chorus_playout_t *p, const chorus_v2_sound_t *sound)
{
    take_lock(p);
    if (p->dsp != NULL) {
        chorus_endpoint_dsp_set_sound(p->dsp, sound);
    }
    give_lock(p);
}

void chorus_playout_set_sub_knobs(chorus_playout_t *p, int32_t level_tenths_db, int32_t phase_deg)
{
    take_lock(p);
    if (p->dsp != NULL) {
        chorus_endpoint_dsp_set_sub_knobs(p->dsp, level_tenths_db, phase_deg);
    }
    give_lock(p);
}

void chorus_playout_reset_stream(chorus_playout_t *p)
{
    take_lock(p);
    p->ring_read = 0;
    p->ring_count = 0;
    p->chunk_read = 0;
    p->chunk_count = 0;
    p->front_taken = 0;
    p->have_highwater = 0;
    p->have_playout_ts = 0;
    p->has_offset = 0;
    p->underrunning = 0;
    p->correction_ppm = 0.0;
    p->pending_frames = 0.0;
    p->mute_frames = 0;
    p->advancing = 0;
    p->acquired = 0;
    p->marker_pending = 0u;
    chorus_servo_init(&p->servo, p->config.servo);
    give_lock(p);
}

/* Arm the marker if a boundary falls inside `n` frames about to be written,
 * the first of which has server timestamp `seg_ts` and is frame `first` of the
 * written count. The marked frame is the first at or after the boundary. */
static void plan_marker(chorus_playout_t *p, uint64_t seg_ts, uint32_t n, uint64_t first)
{
    uint64_t period = p->config.marker_period_ns;
    if (period == 0 || p->frame_ns_q16 == 0 || p->marker_pending != 0u || n == 0) {
        return;
    }
    uint64_t k = (seg_ts + period - 1u) / period;
    uint64_t boundary = k * period;
    if (p->stats.marker_armed > 0 && boundary <= p->stats.marker_last_boundary_ns) {
        return;
    }
    uint64_t rate = p->config.rate_hz;
    uint64_t m = ((boundary - seg_ts) * rate + 999999999ull) / 1000000000ull;
    if (m >= n) {
        return;
    }
    uint64_t frame_ts = seg_ts + m * 1000000000ull / rate;
    uint64_t lead = (frame_ts > boundary) ? frame_ts - boundary : 0u;
    p->marker_frame32 = (uint32_t)(first + m);
    p->marker_lead_ns = (uint32_t)lead;
    p->marker_level = (uint32_t)(k & 1u);
    __atomic_thread_fence(__ATOMIC_SEQ_CST);
    p->marker_pending = 1u;
    p->stats.marker_armed++;
    p->stats.marker_last_boundary_ns = boundary;
}

/* Let `elapsed_ns` of playout pass at the correction in force: the corrector's
 * advance(), which carries the fraction rather than losing it. */
static void advance(chorus_playout_t *p, uint64_t now)
{
    if (p->advancing && p->correction_ppm != 0.0 && now > p->last_advance_ns) {
        double seconds = (double)(now - p->last_advance_ns) / NS_PER_S;
        p->pending_frames += p->correction_ppm * 1e-6 * seconds * (double)p->config.rate_hz;
    }
    p->last_advance_ns = now;
    p->advancing = 1;
}

/* Count `frames` handed to the I2S driver, `audio` of them audio, and publish
 * the total for the interrupt. */
static void note_written(chorus_playout_t *p, uint32_t frames, uint32_t audio)
{
    p->written_frames += frames;
    p->written32 = (uint32_t)p->written_frames;
    if (p->block_count == CHORUS_PLAYOUT_MAX_BLOCKS) {
        /* More blocks outstanding than any DMA ring holds: the oldest is
         * counted played rather than lost. */
        p->frames_played += p->blocks[p->block_read].audio_frames;
        p->block_read = (p->block_read + 1u) % CHORUS_PLAYOUT_MAX_BLOCKS;
        p->block_count--;
    }
    chorus_playout_block_t *b =
        &p->blocks[(p->block_read + p->block_count) % CHORUS_PLAYOUT_MAX_BLOCKS];
    b->written_end = p->written_frames;
    b->audio_frames = audio;
    p->block_count++;
}

uint32_t chorus_playout_fill(chorus_playout_t *p, uint8_t *out, uint32_t frames)
{
    const chorus_playout_config_t *c = &p->config;
    uint32_t fb = frame_bytes(c);
    uint32_t produced = 0;
    uint32_t audio = 0;
    take_lock(p);
    advance(p, p->now_ns());
    /* The endpoint's sound chain (goal 12): when engaged, the block is built
     * as always (audio, inserted silence, the hold, underrun silence, the
     * resync mute) but unscaled, and then the WHOLE block goes through the
     * chain, which applies the room's gain and limit itself, silence
     * included, so the chain's tail plays out in time. */
    int dsp_on = (p->dsp != NULL && p->dsp->engaged);
    uint64_t latency = dsp_on ? chorus_endpoint_dsp_latency_frames(p->dsp) : 0u;

    while (produced < frames) {
        uint32_t room = frames - produced;
        if (p->pending_frames <= -1.0) {
            /* Insert silence; the mute, if any, starts where audio resumes. */
            uint64_t owed = (uint64_t)(-p->pending_frames);
            uint32_t n = (owed < room) ? (uint32_t)owed : room;
            memset(out + (size_t)produced * fb, 0, (size_t)n * fb);
            if (!dsp_on) {
                chorus_volume_skip(&p->volume, n);
            }
            p->pending_frames += (double)n;
            p->stats.inserted_frames += n;
            produced += n;
            continue;
        }
        if (!p->acquired) {
            /* Holding the buffer until the loop has placed it. */
            memset(out + (size_t)produced * fb, 0, (size_t)room * fb);
            if (!dsp_on) {
                chorus_volume_skip(&p->volume, room);
            }
            produced += room;
            break;
        }
        if (p->chunk_count == 0) {
            /* Nothing to play: silence. Once audio has played, that silence
             * is an underrun, counted by the frame. */
            memset(out + (size_t)produced * fb, 0, (size_t)room * fb);
            if (!dsp_on) {
                chorus_volume_skip(&p->volume, room);
            }
            if (p->have_playout_ts) {
                p->stats.underrun_frames += room;
                if (!p->underrunning) {
                    p->underrunning = 1;
                    p->stats.underruns++;
                }
            }
            produced += room;
            break;
        }
        uint32_t in_front = p->chunks[p->chunk_read].frames - p->front_taken;
        if (p->pending_frames >= 1.0) {
            uint64_t owed = (uint64_t)p->pending_frames;
            uint32_t n = (owed < in_front) ? (uint32_t)owed : in_front;
            consume_front(p, n);
            p->pending_frames -= (double)n;
            p->stats.dropped_frames += n;
            continue;
        }
        uint32_t n = (in_front < room) ? in_front : room;
        uint32_t contiguous = c->capacity_frames - p->ring_read;
        if (n > contiguous) {
            n = contiguous;
        }
        uint8_t *dst = out + (size_t)produced * fb;
        memcpy(dst, p->ring + (size_t)p->ring_read * fb, (size_t)n * fb);
        {
            const chorus_playout_chunk_t *front = &p->chunks[p->chunk_read];
            uint64_t seg_ts =
                front->timestamp_ns + (uint64_t)p->front_taken * 1000000000ull / c->rate_hz;
            /* With the chain engaged a frame reaches the pins its latency
             * after it is written, so the marked frame is that much later in
             * the written count. */
            plan_marker(p, seg_ts, n, p->written_frames + produced + latency);
        }
        /* The room's volume, on exactly the frames handed over: the same
         * number of frames, at the same instants, only their content scaled
         * (chorus/volume.h). Before the resync mute, which zeroes on top.
         * With the chain engaged the gain is the chain's (below). */
        if (!dsp_on) {
            chorus_volume_apply(&p->volume, dst, n, c->channels, c->out_sample_bytes);
        }
        if (p->mute_frames > 0) {
            uint32_t silence = (p->mute_frames < n) ? (uint32_t)p->mute_frames : n;
            memset(dst, 0, (size_t)silence * fb);
            p->mute_frames -= silence;
            p->stats.muted_frames += silence;
        }
        consume_front(p, n);
        p->underrunning = 0;
        produced += n;
        audio += n;
    }

    if (dsp_on) {
        chorus_endpoint_dsp_process(p->dsp, out, frames, c->out_sample_bytes, &p->volume);
    }

    note_written(p, frames, audio);
    give_lock(p);
    return audio;
}

void chorus_playout_note_preloaded(chorus_playout_t *p, uint32_t frames)
{
    take_lock(p);
    note_written(p, frames, 0);
    give_lock(p);
}

/* Frames written and not yet at the pins: written minus consumed, less what
 * of the buffer in flight has played since the interrupt stamped the last
 * one. The interpolation is bounded by one DMA buffer and by what is queued.
 * Called with the lock held and a fresh snapshot. */
static double device_delay_frames(chorus_playout_t *p, uint64_t stamp, uint64_t now)
{
    const chorus_playout_config_t *c = &p->config;
    double queued = (double)(p->written_frames - p->dma_consumed);
    double partial = 0.0;
    if (p->dma_interrupts > 0 && now > stamp) {
        partial = (double)(now - stamp) * (double)c->rate_hz / NS_PER_S;
        if (partial > (double)c->dma_frame_num) {
            partial = (double)c->dma_frame_num;
        }
    }
    if (partial > queued) {
        partial = queued;
    }
    /* The sound chain's latency (goal 12): frames inside the chain are as
     * far from the pins as frames inside the DMA. */
    double chain = (p->dsp != NULL) ? (double)chorus_endpoint_dsp_latency_frames(p->dsp) : 0.0;
    return queued - partial + chain;
}

void chorus_playout_fifo(chorus_playout_t *p, double *frames, double *ns)
{
    take_lock(p);
    uint64_t stamp = dma_snapshot(p);
    double f = device_delay_frames(p, stamp, p->now_ns());
    give_lock(p);
    *frames = f;
    *ns = f * NS_PER_S / (double)p->config.rate_hz;
}

chorus_playout_observation_t chorus_playout_observe(chorus_playout_t *p,
                                                    chorus_playout_report_t *report)
{
    const chorus_playout_config_t *c = &p->config;
    chorus_playout_report_t r;
    memset(&r, 0, sizeof(r));
    take_lock(p);
    uint64_t stamp = dma_snapshot(p);
    uint64_t now = p->now_ns();
    r.now_ns = now;
    r.dma_consumed_frames = p->dma_consumed;
    r.dma_stamp_ns = stamp;
    r.written_frames = p->written_frames;

    /* The device delay (device_delay_frames above). */
    r.device_delay_frames = device_delay_frames(p, stamp, now);
    r.device_delay_ns = r.device_delay_frames * NS_PER_S / (double)c->rate_hz;
    r.offset_ns = p->offset_ns;
    r.correction_ppm = p->correction_ppm;

    if (p->chunk_count == 0) {
        r.kind = CHORUS_PLAYOUT_NO_DATA;
    } else if (!p->has_offset) {
        r.kind = CHORUS_PLAYOUT_NO_OFFSET;
    } else if (now > p->offset_at_ns && now - p->offset_at_ns > c->staleness_limit_ns) {
        /* The correction in force stays in force, as the Linux loop holds. */
        r.kind = CHORUS_PLAYOUT_STALE;
    } else {
        const chorus_playout_chunk_t *front = &p->chunks[p->chunk_read];
        r.next_write_ts_ns =
            front->timestamp_ns + (uint64_t)p->front_taken * 1000000000ull / c->rate_hz;
        double target = (double)r.next_write_ts_ns + (double)c->playout_latency_ns;
        double audible = (double)now + r.device_delay_ns + p->offset_ns;
        /* Frames the corrector still owes will move the playout pointer when
         * they are written (a drop brings content forward, an insertion holds
         * it back), so the error is formed as it will be once they are: a
         * step spread over several DMA buffers is not stepped twice. */
        double owed_ns = p->pending_frames * NS_PER_S / (double)c->rate_hz;
        r.error_ns = target - audible + owed_ns;
        double interval_s = (double)c->interval_ms / 1000.0;
        chorus_servo_action_t action = chorus_servo_update(&p->servo, r.error_ns, interval_s);
        if (action.tier == CHORUS_SERVO_HARD_RESYNC) {
            /* Mute, step, resume (the Linux corrector's hard_resync). */
            p->correction_ppm = 0.0;
            p->pending_frames += action.step_ns * (double)c->rate_hz / NS_PER_S;
            uint64_t mute = (uint64_t)((double)c->mute_ns * (double)c->rate_hz / NS_PER_S);
            if (mute > p->mute_frames) {
                p->mute_frames = mute;
            }
            p->stats.hard_resyncs++;
            r.kind = CHORUS_PLAYOUT_HARD_RESYNC;
            r.step_ns = action.step_ns;
        } else {
            p->correction_ppm = action.correction_ppm;
            p->stats.fine_corrections++;
            r.kind = CHORUS_PLAYOUT_FINE;
        }
        r.correction_ppm = p->correction_ppm;
        p->acquired = 1;
        p->stats.error_known = 1;
        p->stats.last_error_ns = r.error_ns;
    }
    p->last = r;
    give_lock(p);
    if (report != NULL) {
        *report = r;
    }
    return r.kind;
}

void chorus_playout_stats(chorus_playout_t *p, chorus_playout_stats_t *out)
{
    take_lock(p);
    (void)dma_snapshot(p);
    p->stats.dma_consumed_frames = p->dma_consumed;
    p->stats.dma_interrupts = p->dma_interrupts;
    p->stats.frames_played = p->frames_played;
    p->stats.written_frames = p->written_frames;
    p->stats.queued_frames = p->ring_count;
    p->stats.correction_ppm = p->correction_ppm;
    p->stats.marker_edges = p->marker_edges32;
    p->stats.room_volume_messages = p->volume.messages;
    p->stats.applied_volume_thousandths = chorus_volume_applied_thousandths(&p->volume);
    if (p->dsp != NULL) {
        p->stats.dsp_engaged = p->dsp->engaged ? 1u : 0u;
        p->stats.dsp_latency_frames = chorus_endpoint_dsp_latency_frames(p->dsp);
        p->stats.sounds_applied = p->dsp->sounds_applied;
        p->stats.dsp_refusals = p->dsp->refusals;
    }
    uint64_t missed_in_interrupt = p->marker_missed32;
    *out = p->stats;
    out->marker_missed += missed_in_interrupt;
    give_lock(p);
}

int chorus_playout_acquired(chorus_playout_t *p)
{
    take_lock(p);
    int acquired = p->acquired;
    give_lock(p);
    return acquired;
}
