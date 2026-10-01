#include "chorus/volume.h"

#include <string.h>

uint32_t chorus_volume_q16_from_thousandths(uint32_t thousandths)
{
    if (thousandths >= CHORUS_VOLUME_FULL) {
        return CHORUS_VOLUME_UNITY;
    }
    /* Rounded down, toward silence; exact at 0 and at 1000. */
    return (uint32_t)((uint64_t)thousandths * CHORUS_VOLUME_UNITY / CHORUS_VOLUME_FULL);
}

static uint32_t min3(uint32_t a, uint32_t b, uint32_t c)
{
    uint32_t m = (a < b) ? a : b;
    return (m < c) ? m : c;
}

void chorus_volume_init(chorus_volume_t *v, uint32_t ceiling_thousandths)
{
    memset(v, 0, sizeof(*v));
    v->ceiling_q16 = chorus_volume_q16_from_thousandths(ceiling_thousandths);
    v->limit_q16 = CHORUS_VOLUME_UNITY;
    /* The startup gain is the ceiling (ADR 0074), and no ramp is moving. */
    v->from_q16 = v->ceiling_q16;
    v->to_q16 = v->ceiling_q16;
    v->ramp_q16 = v->ceiling_q16;
}

uint32_t chorus_volume_applied_q16(const chorus_volume_t *v)
{
    return min3(v->ramp_q16, v->limit_q16, v->ceiling_q16);
}

uint32_t chorus_volume_applied_thousandths(const chorus_volume_t *v)
{
    /* To the nearest: a Q16 value made from thousandths is below it by less
     * than 1/65 of a thousandth, so this gives back the thousandths it was
     * made from. */
    return (uint32_t)(((uint64_t)chorus_volume_applied_q16(v) * CHORUS_VOLUME_FULL +
                       CHORUS_VOLUME_UNITY / 2u) /
                      CHORUS_VOLUME_UNITY);
}

int chorus_volume_ramping(const chorus_volume_t *v)
{
    return v->done_frames < v->ramp_frames;
}

void chorus_volume_set(chorus_volume_t *v, uint32_t gain_thousandths, uint32_t limit_thousandths,
                       uint32_t ramp_ms, uint32_t rate_hz)
{
    /* From what is being heard, so a new ramp never starts above it: a gain
     * that was held down by the limit or the ceiling ramps from there. */
    uint32_t from = chorus_volume_applied_q16(v);
    uint32_t to = chorus_volume_q16_from_thousandths(gain_thousandths);
    if (ramp_ms > 60000u) {
        ramp_ms = 60000u;
    }
    v->limit_q16 = chorus_volume_q16_from_thousandths(limit_thousandths);
    v->from_q16 = from;
    v->to_q16 = to;
    v->ramp_frames = (uint64_t)ramp_ms * rate_hz / 1000u;
    v->done_frames = 0;
    v->acc = 0;
    uint32_t span = (to > from) ? to - from : from - to;
    if (v->ramp_frames == 0 || span == 0) {
        v->ramp_frames = 0;
        v->ramp_q16 = to;
        v->step_q16 = 0;
        v->step_rem = 0;
    } else {
        v->ramp_q16 = from;
        v->step_q16 = (uint32_t)(span / v->ramp_frames);
        v->step_rem = span % v->ramp_frames;
    }
    v->messages++;
}

/* One frame of the ramp: from + (to - from) * k / N, truncated, by the
 * remainder accumulator, ending exactly on `to` at k = N. */
static void step_one(chorus_volume_t *v)
{
    if (v->done_frames >= v->ramp_frames) {
        return;
    }
    v->done_frames++;
    uint32_t delta = v->step_q16;
    v->acc += v->step_rem;
    if (v->acc >= v->ramp_frames) {
        v->acc -= v->ramp_frames;
        delta++;
    }
    v->ramp_q16 = (v->to_q16 > v->from_q16) ? v->ramp_q16 + delta : v->ramp_q16 - delta;
    if (v->done_frames == v->ramp_frames) {
        v->ramp_q16 = v->to_q16;
    }
}

void chorus_volume_skip(chorus_volume_t *v, uint64_t frames)
{
    if (v->done_frames >= v->ramp_frames || frames == 0) {
        return;
    }
    /* The closed form, so a long silence costs one division, not a loop. The
     * span is at most 65536 and a ramp at most 60 s of frames, so the
     * product fits 64 bits at any rate a chorus stream has. */
    uint64_t k = v->done_frames + frames;
    if (k >= v->ramp_frames) {
        v->done_frames = v->ramp_frames;
        v->ramp_q16 = v->to_q16;
        return;
    }
    uint64_t span = (v->to_q16 > v->from_q16) ? v->to_q16 - v->from_q16 : v->from_q16 - v->to_q16;
    uint64_t moved = span * k / v->ramp_frames;
    v->acc = span * k % v->ramp_frames;
    v->done_frames = k;
    v->ramp_q16 =
        (v->to_q16 > v->from_q16) ? v->from_q16 + (uint32_t)moved : v->from_q16 - (uint32_t)moved;
}

/* One signed little-endian sample of `bytes` bytes, scaled by `g` (Q16) and
 * truncated toward zero. The magnitude never grows (g is at most unity), so
 * the result always fits the slot it came from. */
static void scale_sample(uint8_t *s, uint8_t bytes, uint32_t g)
{
    uint32_t raw = 0;
    for (uint8_t i = 0; i < bytes; i++) {
        raw |= (uint32_t)s[i] << (8u * i);
    }
    int64_t value = (int64_t)raw;
    if ((raw >> (8u * bytes - 1u)) & 1u) {
        value -= (int64_t)1 << (8u * bytes);
    }
    int64_t scaled = value * (int64_t)g / (int64_t)CHORUS_VOLUME_UNITY;
    uint64_t out = (uint64_t)scaled;
    for (uint8_t i = 0; i < bytes; i++) {
        s[i] = (uint8_t)(out >> (8u * i));
    }
}

void chorus_volume_apply(chorus_volume_t *v, uint8_t *pcm, uint32_t frames, uint8_t channels,
                         uint8_t sample_bytes)
{
    size_t frame_bytes = (size_t)channels * sample_bytes;
    if (!chorus_volume_ramping(v)) {
        /* One gain for every frame: the ordinary case, and unity costs
         * nothing at all. */
        uint32_t g = chorus_volume_applied_q16(v);
        if (g >= CHORUS_VOLUME_UNITY) {
            return;
        }
        if (g == 0) {
            memset(pcm, 0, (size_t)frames * frame_bytes);
            return;
        }
        for (size_t at = 0; at < (size_t)frames * channels; at++) {
            scale_sample(pcm + at * sample_bytes, sample_bytes, g);
        }
        return;
    }
    for (uint32_t f = 0; f < frames; f++) {
        uint32_t g = chorus_volume_applied_q16(v);
        if (g < CHORUS_VOLUME_UNITY) {
            uint8_t *frame = pcm + (size_t)f * frame_bytes;
            for (uint8_t ch = 0; ch < channels; ch++) {
                scale_sample(frame + (size_t)ch * sample_bytes, sample_bytes, g);
            }
        }
        step_one(v);
    }
}

int chorus_volume_parse(const char *text, uint32_t *thousandths)
{
    if (text == NULL || (text[0] != '0' && text[0] != '1')) {
        return -1;
    }
    uint32_t value = (uint32_t)(text[0] - '0') * 1000u;
    const char *p = text + 1;
    if (*p == '.') {
        p++;
        uint32_t scale = 100u;
        int digits = 0;
        while (*p >= '0' && *p <= '9') {
            if (digits == 3) {
                return -1;
            }
            value += (uint32_t)(*p - '0') * scale;
            scale /= 10u;
            digits++;
            p++;
        }
        if (digits == 0) {
            return -1;
        }
    }
    if (*p != '\0' || value > CHORUS_VOLUME_FULL) {
        return -1;
    }
    *thousandths = value;
    return 0;
}
