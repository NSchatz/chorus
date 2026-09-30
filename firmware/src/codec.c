#include "chorus/codec.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "codec_backend.h"

struct chorus_decoder {
    uint8_t codec;
    uint8_t sample_format;
    uint8_t channels;
    uint32_t max_frames;
    /* Opus only: frames still to drop from the start of the stream. */
    uint32_t skip_left;
    chorus_flac_t *flac;
    chorus_opus_t *opus;
    /* One chunk's samples, left-justified in 32 bits, before packing. */
    int32_t *samples;
};

static void say(char *detail, size_t detail_len, const char *text)
{
    if (detail != NULL && detail_len > 0) {
        snprintf(detail, detail_len, "%s", text);
    }
}

chorus_codec_status_t chorus_codec_open(chorus_decoder_t **out, const chorus_codec_stream_t *stream,
                                        char *detail, size_t detail_len)
{
    *out = NULL;
    if (stream->sample_format != CHORUS_CODEC_S16 && stream->sample_format != CHORUS_CODEC_S24) {
        snprintf(detail, detail_len,
                 "sample format %u is not one this endpoint decodes to (1 pcm_s16le, 2 pcm_s24le)",
                 (unsigned)stream->sample_format);
        return CHORUS_CODEC_UNSUPPORTED;
    }
    if (stream->channels < 1 || stream->channels > 8) {
        snprintf(detail, detail_len, "a stream of %u channels is outside 1 to 8",
                 (unsigned)stream->channels);
        return CHORUS_CODEC_UNSUPPORTED;
    }
    chorus_decoder_t *d = calloc(1, sizeof(*d));
    if (d == NULL) {
        say(detail, detail_len, "no memory for the decoder");
        return CHORUS_CODEC_NO_MEMORY;
    }
    d->codec = stream->codec;
    d->sample_format = stream->sample_format;
    d->channels = stream->channels;

    chorus_codec_status_t status;
    if (stream->codec == CHORUS_CODEC_FLAC) {
        status = chorus_flac_open(&d->flac, stream, &d->max_frames, detail, detail_len);
    } else if (stream->codec == CHORUS_CODEC_OPUS) {
        d->max_frames = CHORUS_CODEC_OPUS_MAX_FRAMES;
        status = chorus_opus_open(&d->opus, stream, &d->skip_left, detail, detail_len);
    } else {
        snprintf(detail, detail_len, "codec %u is not one this decoder takes (2 flac, 3 opus)",
                 (unsigned)stream->codec);
        status = CHORUS_CODEC_UNSUPPORTED;
    }
    if (status == CHORUS_CODEC_OK) {
        d->samples = malloc((size_t)d->max_frames * d->channels * sizeof(int32_t));
        if (d->samples == NULL) {
            say(detail, detail_len, "no memory for one chunk's samples");
            status = CHORUS_CODEC_NO_MEMORY;
        }
    }
    if (status != CHORUS_CODEC_OK) {
        chorus_codec_close(d);
        return status;
    }
    *out = d;
    return CHORUS_CODEC_OK;
}

static void pack(const chorus_decoder_t *d, const int32_t *samples, size_t count, uint8_t *pcm)
{
    if (d->sample_format == CHORUS_CODEC_S16) {
        for (size_t i = 0; i < count; i++) {
            uint32_t v = (uint32_t)samples[i] >> 16;
            pcm[2 * i] = (uint8_t)v;
            pcm[2 * i + 1] = (uint8_t)(v >> 8);
        }
    } else {
        for (size_t i = 0; i < count; i++) {
            uint32_t v = (uint32_t)samples[i] >> 8;
            pcm[3 * i] = (uint8_t)v;
            pcm[3 * i + 1] = (uint8_t)(v >> 8);
            pcm[3 * i + 2] = (uint8_t)(v >> 16);
        }
    }
}

chorus_codec_status_t chorus_codec_decode(chorus_decoder_t *d, const uint8_t *data, size_t len,
                                          uint32_t frames, uint8_t *pcm, size_t pcm_capacity,
                                          uint32_t *frames_out, char *detail, size_t detail_len)
{
    *frames_out = 0;
    if (len == 0 || frames == 0) {
        say(detail, detail_len, "a coded chunk with no data or no frames");
        return CHORUS_CODEC_BAD_CHUNK;
    }
    if (frames > d->max_frames) {
        snprintf(detail, detail_len, "the chunk claims %u frames and this stream's most is %u",
                 (unsigned)frames, (unsigned)d->max_frames);
        return CHORUS_CODEC_BAD_CHUNK;
    }
    uint32_t got = 0;
    chorus_codec_status_t status;
    if (d->flac != NULL) {
        status = chorus_flac_decode(d->flac, data, len, d->samples, &got, detail, detail_len);
    } else {
        status = chorus_opus_decode(d->opus, data, len, d->samples, d->max_frames, &got, detail,
                                    detail_len);
    }
    if (status != CHORUS_CODEC_OK) {
        return status;
    }
    if (got != frames) {
        snprintf(detail, detail_len, "the chunk says it decodes to %u frames and it decoded to %u",
                 (unsigned)frames, (unsigned)got);
        return CHORUS_CODEC_BAD_CHUNK;
    }
    uint32_t skip = d->skip_left < got ? d->skip_left : got;
    d->skip_left -= skip;
    uint32_t keep = got - skip;
    size_t need = (size_t)keep * chorus_codec_frame_bytes(d);
    if (need > pcm_capacity) {
        snprintf(detail, detail_len, "the chunk decodes to %zu bytes and the buffer holds %zu",
                 need, pcm_capacity);
        return CHORUS_CODEC_NO_ROOM;
    }
    pack(d, d->samples + (size_t)skip * d->channels, (size_t)keep * d->channels, pcm);
    *frames_out = keep;
    return CHORUS_CODEC_OK;
}

uint32_t chorus_codec_max_frames(const chorus_decoder_t *d)
{
    return d->max_frames;
}

size_t chorus_codec_frame_bytes(const chorus_decoder_t *d)
{
    return (size_t)d->channels * (d->sample_format == CHORUS_CODEC_S16 ? 2u : 3u);
}

uint32_t chorus_codec_final_range(const chorus_decoder_t *d)
{
    return d->opus != NULL ? chorus_opus_final_range(d->opus) : 0;
}

void chorus_codec_close(chorus_decoder_t *d)
{
    if (d == NULL) {
        return;
    }
    if (d->flac != NULL) {
        chorus_flac_close(d->flac);
    }
    if (d->opus != NULL) {
        chorus_opus_close(d->opus);
    }
    free(d->samples);
    free(d);
}
