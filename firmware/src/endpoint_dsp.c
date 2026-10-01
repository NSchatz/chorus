#include "chorus/endpoint_dsp.h"

#include <string.h>

/* The library's refusal codes, by name, for a refusal that names no field. */
static const char *err_name(chorus_dsp_err_t rc)
{
    switch (rc) {
    case CHORUS_DSP_OK:
        return "ok";
    case CHORUS_DSP_ERR_RATE:
        return "rate";
    case CHORUS_DSP_ERR_FREQUENCY:
        return "frequency";
    case CHORUS_DSP_ERR_Q:
        return "q";
    case CHORUS_DSP_ERR_GAIN:
        return "gain";
    case CHORUS_DSP_ERR_DELAY_TOO_LONG:
        return "delay";
    case CHORUS_DSP_ERR_LOOKAHEAD:
        return "lookahead";
    case CHORUS_DSP_ERR_TIME_CONSTANT:
        return "time_constant";
    case CHORUS_DSP_ERR_CHANNEL_MAP:
        return "channel_map";
    case CHORUS_DSP_ERR_SETTING:
        return "setting";
    case CHORUS_DSP_ERR_ENDPOINT:
        return "endpoint";
    case CHORUS_DSP_ERR_BUFFER:
        return "buffer";
    }
    return "unknown";
}

void chorus_endpoint_dsp_settings(const chorus_v2_sound_t *wire, int32_t knob_level_tenths_db,
                                  int32_t knob_phase_deg, chorus_dsp_sound_t *out)
{
    chorus_dsp_sound_default(out);
    out->bass_db = wire->bass_db;
    out->treble_db = wire->treble_db;
    out->loudness = (wire->flags & CHORUS_V2_SOUND_FLAG_LOUDNESS) != 0;
    out->night = (wire->flags & CHORUS_V2_SOUND_FLAG_NIGHT) != 0;
    out->speech = (wire->flags & CHORUS_V2_SOUND_FLAG_SPEECH) != 0;
    out->room_eq_enabled = (wire->flags & CHORUS_V2_SOUND_FLAG_ROOM_EQ) != 0;
    out->role = wire->role;
    out->sub_present = wire->sub_present != 0;
    out->crossover_hz = wire->crossover_hz;
    out->tv_upmix = wire->tv_upmix;
    out->fold_centre = (wire->fold & CHORUS_V2_SOUND_FOLD_CENTRE) != 0;
    out->fold_surround = (wire->fold & CHORUS_V2_SOUND_FOLD_SURROUND) != 0;
    /* The knob is a cut beside the catalog's level (ADR 0063: a cut only, so
     * no knob position lifts the sub above what the room set). The sum is
     * held to the wire's range, which is the chain's. */
    int32_t level = (int32_t)wire->sub_level_cdb + knob_level_tenths_db * 10;
    if (level < CHORUS_DSP_SUB_LEVEL_MIN_CDB) {
        level = CHORUS_DSP_SUB_LEVEL_MIN_CDB;
    }
    if (level > CHORUS_DSP_SUB_LEVEL_MAX_CDB) {
        level = CHORUS_DSP_SUB_LEVEL_MAX_CDB;
    }
    out->sub_level_cdb = (int16_t)level;
    /* Two inversions cancel: the catalog's polarity and the knob's half turn. */
    bool catalog_inverted = (wire->flags & CHORUS_V2_SOUND_FLAG_SUB_INVERTED) != 0;
    bool knob_inverted = knob_phase_deg >= CHORUS_ENDPOINT_DSP_KNOB_INVERT_DEG;
    out->sub_polarity_inverted = catalog_inverted != knob_inverted;
    uint8_t n = wire->eq_count;
    if (n > CHORUS_DSP_ROOM_EQ_MAX_FILTERS) {
        n = CHORUS_DSP_ROOM_EQ_MAX_FILTERS; /* never from the decoder */
    }
    out->eq_count = n;
    for (uint8_t i = 0; i < n; i++) {
        out->eq[i].freq_hz = wire->filters[i].freq_hz;
        out->eq[i].gain_cdb = wire->filters[i].gain_cdb;
        out->eq[i].q_milli = wire->filters[i].q_milli;
    }
}

void chorus_endpoint_dsp_init(chorus_endpoint_dsp_t *d, const chorus_endpoint_two_way_t *two_way)
{
    memset(d, 0, sizeof(*d));
    chorus_dsp_endpoint_default(&d->endpoint);
    d->woofer_slot = 0;
    d->tweeter_slot = 1;
    if (two_way != NULL && two_way->enabled) {
        d->endpoint.two_way = true;
        d->endpoint.two_way_hz = two_way->crossover_hz;
        /* endpoint.conf's reader refuses anything but two distinct slots
         * of the two; a caller that passes otherwise gets the example. */
        if (two_way->woofer_slot < CHORUS_ENDPOINT_DSP_SLOTS &&
            two_way->tweeter_slot < CHORUS_ENDPOINT_DSP_SLOTS &&
            two_way->woofer_slot != two_way->tweeter_slot) {
            d->woofer_slot = two_way->woofer_slot;
            d->tweeter_slot = two_way->tweeter_slot;
        }
    }
}

static void refused(chorus_endpoint_dsp_t *d, chorus_dsp_err_t rc, const char *field)
{
    d->refusals++;
    d->last_refused = (field != NULL) ? field : err_name(rc);
}

/* Bring the chain to what the endpoint now knows. `rebuild` asks for a new
 * chain (a new stream); otherwise an engaged chain is reconfigured in place.
 * Returns 1 when a sound was taken into a running chain. */
static int reconfigure(chorus_endpoint_dsp_t *d, bool rebuild)
{
    if (!d->have_stream || !(d->have_sound || d->endpoint.two_way)) {
        return 0;
    }
    chorus_dsp_sound_t s;
    if (d->have_sound) {
        chorus_endpoint_dsp_settings(&d->wire, d->knob_level_tenths_db, d->knob_phase_deg, &s);
    } else {
        /* A two-way with no sound yet: flat, not in a set. */
        chorus_dsp_sound_default(&s);
    }
    const char *field = NULL;
    if (!d->engaged || rebuild) {
        chorus_dsp_err_t rc = chorus_dsp_chain_init(&d->chain, &s, &d->endpoint, d->map,
                                                    d->channels, d->rate_hz, &field);
        if (rc != CHORUS_DSP_OK) {
            /* Not engaged: the playout path applies the room's gain itself,
             * as before goal 12. Never a silent or ungained path. */
            d->engaged = false;
            refused(d, rc, field);
            return 0;
        }
        d->engaged = true;
        return d->have_sound ? 1 : 0;
    }
    chorus_dsp_err_t rc = chorus_dsp_chain_set_sound(&d->chain, &s, &field);
    if (rc != CHORUS_DSP_OK) {
        /* The library changed nothing: the previous settings stay. */
        refused(d, rc, field);
        return 0;
    }
    return 1;
}

void chorus_endpoint_dsp_set_stream(chorus_endpoint_dsp_t *d, uint32_t rate_hz, uint32_t channels,
                                    const uint8_t *map)
{
    if (channels == 0 || channels > CHORUS_ENDPOINT_DSP_SLOTS) {
        /* The playout path refuses such a stream's chunks anyway. */
        d->have_stream = false;
        d->engaged = false;
        return;
    }
    d->have_stream = true;
    d->rate_hz = rate_hz;
    d->channels = channels;
    memset(d->map, 0, sizeof(d->map));
    memcpy(d->map, map, channels);
    (void)reconfigure(d, true);
}

void chorus_endpoint_dsp_set_sound(chorus_endpoint_dsp_t *d, const chorus_v2_sound_t *wire)
{
    d->wire = *wire;
    d->have_sound = true;
    d->sounds_applied += (uint32_t)reconfigure(d, false);
}

void chorus_endpoint_dsp_set_sub_knobs(chorus_endpoint_dsp_t *d, int32_t level_tenths_db,
                                       int32_t phase_deg)
{
    if (level_tenths_db < CHORUS_ENDPOINT_DSP_KNOB_LEVEL_MIN_TENTHS) {
        level_tenths_db = CHORUS_ENDPOINT_DSP_KNOB_LEVEL_MIN_TENTHS;
    }
    if (level_tenths_db > 0) {
        level_tenths_db = 0;
    }
    if (phase_deg < 0) {
        phase_deg = 0;
    }
    if (phase_deg > CHORUS_ENDPOINT_DSP_KNOB_PHASE_MAX_DEG) {
        phase_deg = CHORUS_ENDPOINT_DSP_KNOB_PHASE_MAX_DEG;
    }
    d->knob_level_tenths_db = level_tenths_db;
    d->knob_phase_deg = phase_deg;
    /* The knobs only reach a chain that has a sound to combine them with. */
    if (d->have_sound) {
        (void)reconfigure(d, false);
    }
}

uint32_t chorus_endpoint_dsp_latency_frames(const chorus_endpoint_dsp_t *d)
{
    return d->engaged ? chorus_dsp_chain_latency_frames(&d->chain) : 0u;
}

/* One slot, left-justified in 32 bits: the playout ring's own layout. */
static int32_t read_slot(const uint8_t *s, uint8_t bytes)
{
    uint32_t v = 0;
    for (uint8_t i = 0; i < bytes; i++) {
        v |= (uint32_t)s[i] << (8u * (4u - bytes + i));
    }
    return (int32_t)v;
}

static void write_slot(int32_t value, uint8_t bytes, uint8_t *out)
{
    uint32_t v = (uint32_t)value;
    for (uint8_t i = 0; i < bytes; i++) {
        out[i] = (uint8_t)(v >> (8u * (4u - bytes + i)));
    }
}

/* 2^-31 and 2^31: an int32 sample and its float are the same number, exactly
 * for every value a 24-bit or narrower slot can hold. */
#define TO_FLOAT (1.0f / 2147483648.0f)

/* Back to a slot, saturated at full scale (the same rule the playout path
 * reads a pcm_f32le sample with). */
static int32_t from_float(float f)
{
    if (!(f > -1.0f)) {
        return INT32_MIN; /* also NaN */
    }
    if (f >= 1.0f) {
        return INT32_MAX;
    }
    return (int32_t)((double)f * 2147483648.0);
}

void chorus_endpoint_dsp_process(chorus_endpoint_dsp_t *d, uint8_t *block, uint32_t frames,
                                 uint8_t sample_bytes, chorus_volume_t *volume)
{
    const uint32_t fb = CHORUS_ENDPOINT_DSP_SLOTS * sample_bytes;
    const uint32_t channels = d->channels;
    const uint32_t outputs = chorus_dsp_chain_out_channels(&d->chain);
    /* The effective limit: the room's last limit and this endpoint's own
     * ceiling, the lesser (chorus/volume.h). The chain's limiter takes
     * min(1, it). */
    uint32_t limit_q16 =
        (volume->limit_q16 < volume->ceiling_q16) ? volume->limit_q16 : volume->ceiling_q16;
    float limit = (float)limit_q16 / (float)CHORUS_VOLUME_UNITY;
    uint32_t done = 0;
    while (done < frames) {
        uint32_t n = frames - done;
        if (n > CHORUS_ENDPOINT_DSP_BLOCK_FRAMES) {
            n = CHORUS_ENDPOINT_DSP_BLOCK_FRAMES;
        }
        uint8_t *at = block + (size_t)done * fb;
        for (uint32_t f = 0; f < n; f++) {
            const uint8_t *frame = at + (size_t)f * fb;
            for (uint32_t c = 0; c < channels; c++) {
                /* A mono stream sits in both slots; slot 0 is its one channel. */
                d->in[f * channels + c] =
                    (float)read_slot(frame + (size_t)c * sample_bytes, sample_bytes) * TO_FLOAT;
            }
        }
        /* The gain at this sub-block's first frame (the ramp is a staircase
         * of CHORUS_ENDPOINT_DSP_BLOCK_FRAMES steps while it moves). */
        float gain = (float)chorus_volume_applied_q16(volume) / (float)CHORUS_VOLUME_UNITY;
        (void)chorus_dsp_chain_process(&d->chain, d->in, d->out, n, gain, limit);
        for (uint32_t f = 0; f < n; f++) {
            uint8_t *frame = at + (size_t)f * fb;
            const float *o = d->out + (size_t)f * outputs;
            int32_t slot[CHORUS_ENDPOINT_DSP_SLOTS];
            if (outputs == 1) {
                slot[0] = slot[1] = from_float(o[0]);
            } else if (d->endpoint.two_way) {
                slot[d->woofer_slot] = from_float(o[0]);
                slot[d->tweeter_slot] = from_float(o[1]);
            } else {
                slot[0] = from_float(o[0]);
                slot[1] = from_float(o[1]);
            }
            write_slot(slot[0], sample_bytes, frame);
            write_slot(slot[1], sample_bytes, frame + sample_bytes);
        }
        chorus_volume_skip(volume, n);
        done += n;
    }
}
