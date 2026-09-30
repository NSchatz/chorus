/* Opus over libopus 1.6.1 (third_party/opus, BSD-3-Clause), built fixed-point.
 *
 * The codec setup is the OpusHead (RFC 7845 section 5.1): "OpusHead", version,
 * channel count, pre-skip (little-endian 16 bits), input rate (32 bits, for
 * information only), output gain (signed Q7.8 dB, 16 bits) and the mapping
 * family. Family 0 is one Opus stream of one or two channels and is what this
 * decoder takes; the output gain is handed to libopus's own OPUS_SET_GAIN,
 * which applies exactly that quantity, and the pre-skip is returned for
 * codec.c to drop. Every packet is decoded as it comes (no FEC, no look
 * ahead): a coded_chunk is one packet. */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "codec_backend.h"
#include "opus.h"

#define OPUS_HEAD_FAMILY0_LEN 19
#define OPUS_RATE_HZ 48000

struct chorus_opus {
    OpusDecoder *dec;
    uint8_t channels;
    uint8_t sample_format;
    /* The 16- or 24-bit decode, before it is left-justified. */
    int16_t *pcm16;
};

chorus_codec_status_t chorus_opus_open(chorus_opus_t **out, const chorus_codec_stream_t *stream,
                                       uint32_t *pre_skip, char *detail, size_t detail_len)
{
    *out = NULL;
    const uint8_t *h = stream->config;
    if (h == NULL || stream->config_len < OPUS_HEAD_FAMILY0_LEN || memcmp(h, "OpusHead", 8) != 0) {
        snprintf(detail, detail_len, "an Opus codec setup is an OpusHead of at least 19 bytes");
        return CHORUS_CODEC_UNSUPPORTED;
    }
    /* RFC 7845 section 5.1: a major version (upper four bits) other than 0 is
     * a format this decoder cannot read. */
    if ((h[8] >> 4) != 0) {
        snprintf(detail, detail_len, "OpusHead version %u is not one this decoder reads",
                 (unsigned)h[8]);
        return CHORUS_CODEC_UNSUPPORTED;
    }
    unsigned channels = h[9];
    unsigned skip = (unsigned)h[10] | ((unsigned)h[11] << 8);
    int16_t gain = (int16_t)((uint16_t)h[16] | ((uint16_t)h[17] << 8));
    unsigned family = h[18];
    if (family != 0) {
        snprintf(detail, detail_len,
                 "Opus channel mapping family %u is not decoded here yet (family 0 only: one or "
                 "two channels)",
                 family);
        return CHORUS_CODEC_UNSUPPORTED;
    }
    if (stream->config_len != OPUS_HEAD_FAMILY0_LEN || channels < 1 || channels > 2 ||
        channels != stream->channels) {
        snprintf(detail, detail_len,
                 "the OpusHead (%zu bytes, %u channels) does not match a family 0 stream of %u "
                 "channels",
                 stream->config_len, channels, (unsigned)stream->channels);
        return CHORUS_CODEC_UNSUPPORTED;
    }
    if (stream->sample_rate_hz != OPUS_RATE_HZ) {
        snprintf(detail, detail_len, "an Opus stream is 48000 Hz, not %u",
                 (unsigned)stream->sample_rate_hz);
        return CHORUS_CODEC_UNSUPPORTED;
    }
    chorus_opus_t *o = calloc(1, sizeof(*o));
    if (o == NULL) {
        snprintf(detail, detail_len, "no memory for the Opus decoder");
        return CHORUS_CODEC_NO_MEMORY;
    }
    o->channels = (uint8_t)channels;
    o->sample_format = stream->sample_format;
    int error = OPUS_OK;
    o->dec = opus_decoder_create(OPUS_RATE_HZ, (int)channels, &error);
    if (o->dec == NULL || error != OPUS_OK) {
        snprintf(detail, detail_len, "libopus refused the decoder: %s", opus_strerror(error));
        chorus_opus_close(o);
        return CHORUS_CODEC_UNSUPPORTED;
    }
    if (gain != 0 && opus_decoder_ctl(o->dec, OPUS_SET_GAIN(gain)) != OPUS_OK) {
        snprintf(detail, detail_len, "libopus refused the OpusHead output gain %d", gain);
        chorus_opus_close(o);
        return CHORUS_CODEC_UNSUPPORTED;
    }
    if (o->sample_format == CHORUS_CODEC_S16) {
        o->pcm16 = malloc((size_t)CHORUS_CODEC_OPUS_MAX_FRAMES * channels * sizeof(int16_t));
        if (o->pcm16 == NULL) {
            snprintf(detail, detail_len, "no memory for the Opus output");
            chorus_opus_close(o);
            return CHORUS_CODEC_NO_MEMORY;
        }
    }
    *pre_skip = skip;
    *out = o;
    return CHORUS_CODEC_OK;
}

chorus_codec_status_t chorus_opus_decode(chorus_opus_t *o, const uint8_t *data, size_t len,
                                         int32_t *samples, uint32_t capacity_frames,
                                         uint32_t *frames, char *detail, size_t detail_len)
{
    *frames = 0;
    if (len > 0x7fffffff) {
        snprintf(detail, detail_len, "an Opus packet of %zu bytes", len);
        return CHORUS_CODEC_BAD_CHUNK;
    }
    int n;
    if (o->sample_format == CHORUS_CODEC_S16) {
        n = opus_decode(o->dec, data, (opus_int32)len, o->pcm16, (int)capacity_frames, 0);
        for (int i = 0; n > 0 && i < n * o->channels; i++) {
            samples[i] = (int32_t)((uint32_t)(int32_t)o->pcm16[i] << 16);
        }
    } else {
        /* opus_decode24 writes 24-bit values into 32-bit words (libopus 1.6's
         * include/opus.h); left-justify them in place. */
        n = opus_decode24(o->dec, data, (opus_int32)len, (opus_int32 *)samples,
                          (int)capacity_frames, 0);
        for (int i = 0; n > 0 && i < n * o->channels; i++) {
            samples[i] = (int32_t)((uint32_t)samples[i] << 8);
        }
    }
    if (n < 0) {
        snprintf(detail, detail_len, "libopus refused a packet of %zu bytes: %s", len,
                 opus_strerror(n));
        return CHORUS_CODEC_BAD_CHUNK;
    }
    *frames = (uint32_t)n;
    return CHORUS_CODEC_OK;
}

uint32_t chorus_opus_final_range(const chorus_opus_t *o)
{
    opus_uint32 range = 0;
    if (opus_decoder_ctl(o->dec, OPUS_GET_FINAL_RANGE(&range)) != OPUS_OK) {
        return 0;
    }
    return (uint32_t)range;
}

void chorus_opus_close(chorus_opus_t *o)
{
    if (o == NULL) {
        return;
    }
    if (o->dec != NULL) {
        opus_decoder_destroy(o->dec);
    }
    free(o->pcm16);
    free(o);
}
