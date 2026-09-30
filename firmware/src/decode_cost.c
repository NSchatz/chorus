#include "chorus/decode_cost.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "chorus/codec.h"

/* The fields a decode needs, read from NAME.fields' `key = value` lines (the
 * format fixtures/README.md gives; `#` starts a comment). */
typedef struct {
    uint8_t codec;
    uint8_t sample_format;
    uint32_t rate;
    uint8_t channels;
    uint8_t config[64];
    size_t config_len;
    uint32_t chunks;
    uint64_t fnv;
    int have_fnv;
} fields_t;

static int hex_nibble(char c)
{
    if (c >= '0' && c <= '9') {
        return c - '0';
    }
    if (c >= 'a' && c <= 'f') {
        return c - 'a' + 10;
    }
    if (c >= 'A' && c <= 'F') {
        return c - 'A' + 10;
    }
    return -1;
}

static int take_field(fields_t *f, const char *key, const char *value)
{
    if (strcmp(key, "codec") == 0) {
        if (strcmp(value, "flac") == 0) {
            f->codec = CHORUS_CODEC_FLAC;
        } else if (strcmp(value, "opus") == 0) {
            f->codec = CHORUS_CODEC_OPUS;
        } else {
            return -1;
        }
    } else if (strcmp(key, "sample_format") == 0) {
        if (strcmp(value, "pcm_s16le") == 0) {
            f->sample_format = CHORUS_CODEC_S16;
        } else if (strcmp(value, "pcm_s24le") == 0) {
            f->sample_format = CHORUS_CODEC_S24;
        } else {
            return -1;
        }
    } else if (strcmp(key, "sample_rate_hz") == 0) {
        f->rate = (uint32_t)strtoul(value, NULL, 10);
    } else if (strcmp(key, "channels") == 0) {
        f->channels = (uint8_t)strtoul(value, NULL, 10);
    } else if (strcmp(key, "chunks") == 0) {
        f->chunks = (uint32_t)strtoul(value, NULL, 10);
    } else if (strcmp(key, "decode_fnv1a64") == 0) {
        f->fnv = strtoull(value, NULL, 16);
        f->have_fnv = 1;
    } else if (strcmp(key, "codec_config") == 0) {
        size_t n = strlen(value);
        if (n % 2 != 0 || n / 2 > sizeof(f->config)) {
            return -1;
        }
        for (size_t i = 0; i < n / 2; i++) {
            int hi = hex_nibble(value[2 * i]);
            int lo = hex_nibble(value[2 * i + 1]);
            if (hi < 0 || lo < 0) {
                return -1;
            }
            f->config[i] = (uint8_t)((hi << 4) | lo);
        }
        f->config_len = n / 2;
    }
    return 0;
}

static int parse_fields(const char *text, size_t len, fields_t *f, char *detail, size_t detail_len)
{
    memset(f, 0, sizeof(*f));
    size_t at = 0;
    while (at < len) {
        size_t end = at;
        while (end < len && text[end] != '\n') {
            end++;
        }
        char line[256];
        size_t n = end - at;
        if (n >= sizeof(line)) {
            snprintf(detail, detail_len, "a fields line is longer than %zu bytes", sizeof(line));
            return -1;
        }
        memcpy(line, text + at, n);
        line[n] = '\0';
        at = end + 1;
        char *hash = strchr(line, '#');
        if (hash != NULL) {
            *hash = '\0';
        }
        char key[64];
        char value[192];
        if (sscanf(line, " %63s = %191s", key, value) != 2) {
            continue;
        }
        if (take_field(f, key, value) != 0) {
            snprintf(detail, detail_len, "the fields give %s = %s, which this endpoint cannot use",
                     key, value);
            return -1;
        }
    }
    if (f->codec == 0 || f->sample_format == 0 || f->rate == 0 || f->channels == 0 ||
        f->chunks == 0) {
        snprintf(detail, detail_len,
                 "the fields do not name a codec, sample format, rate, channels and chunk count");
        return -1;
    }
    return 0;
}

static uint32_t be32(const uint8_t *p)
{
    return ((uint32_t)p[0] << 24) | ((uint32_t)p[1] << 16) | ((uint32_t)p[2] << 8) | p[3];
}

int chorus_decode_cost_run(const chorus_decode_fixture_t *fixture, chorus_clock_fn now_ns,
                           chorus_decode_cost_t *out, char *detail, size_t detail_len)
{
    memset(out, 0, sizeof(*out));
    fields_t f;
    if (parse_fields(fixture->fields, fixture->fields_len, &f, detail, detail_len) != 0) {
        return -1;
    }
    chorus_codec_stream_t stream = {f.codec,    f.sample_format, f.rate,
                                    f.channels, f.config,        f.config_len};
    chorus_decoder_t *decoder = NULL;
    if (chorus_codec_open(&decoder, &stream, detail, detail_len) != CHORUS_CODEC_OK) {
        return -1;
    }
    size_t frame_bytes = chorus_codec_frame_bytes(decoder);
    size_t capacity = (size_t)chorus_codec_max_frames(decoder) * frame_bytes;
    uint8_t *pcm = malloc(capacity);
    if (pcm == NULL) {
        chorus_codec_close(decoder);
        snprintf(detail, detail_len, "no memory for a %zu byte decode buffer", capacity);
        return -1;
    }

    uint64_t fnv = 0xcbf29ce484222325ull;
    uint32_t chunks = 0;
    uint64_t frames = 0;
    int failed = 0;
    size_t at = 0;
    /* Only the decode calls are timed: the parse above and the hash below are
     * the harness's cost, not the decoder's. */
    uint64_t elapsed = 0;
    while (at + 12 <= fixture->chunks_len) {
        uint32_t want = be32(fixture->chunks + at);
        uint32_t length = be32(fixture->chunks + at + 8);
        if (at + 12 + length > fixture->chunks_len) {
            snprintf(detail, detail_len, "chunk %u runs past the end of the chunks",
                     (unsigned)chunks);
            failed = 1;
            break;
        }
        uint32_t got = 0;
        uint64_t started = now_ns();
        chorus_codec_status_t status =
            chorus_codec_decode(decoder, fixture->chunks + at + 12, length, want, pcm, capacity,
                                &got, detail, detail_len);
        elapsed += now_ns() - started;
        if (status != CHORUS_CODEC_OK) {
            failed = 1;
            break;
        }
        for (size_t i = 0; i < (size_t)got * frame_bytes; i++) {
            fnv = (fnv ^ pcm[i]) * 0x100000001b3ull;
        }
        frames += got;
        chunks++;
        at += 12 + length;
    }
    free(pcm);
    chorus_codec_close(decoder);
    if (failed) {
        return -1;
    }
    if (chunks != f.chunks || at != fixture->chunks_len) {
        snprintf(detail, detail_len, "the fixture declares %u chunks and %u decoded",
                 (unsigned)f.chunks, (unsigned)chunks);
        return -1;
    }

    out->codec = f.codec;
    out->sample_rate_hz = f.rate;
    out->channels = f.channels;
    out->chunks = chunks;
    out->frames = frames;
    out->elapsed_ns = elapsed;
    double seconds = (double)elapsed / 1e9;
    double audio_seconds = (double)frames / (double)f.rate;
    out->frames_per_s = (seconds > 0.0) ? (double)frames / seconds : 0.0;
    out->cpu_fraction = (audio_seconds > 0.0) ? seconds / audio_seconds : 0.0;
    out->decode_matches = f.have_fnv && fnv == f.fnv;
    return 0;
}
