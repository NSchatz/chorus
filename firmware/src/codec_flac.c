/* FLAC over dr_flac 0.13.3 (third_party/dr_flac, public domain or MIT-0).
 *
 * dr_flac reads a FLAC stream through a read callback. The wire does not carry
 * a stream, it carries a STREAMINFO body once and then one frame per chunk, so
 * the callback serves a stream made of the two: first `fLaC` and a STREAMINFO
 * metadata block built from the stream_format's codec setup (RFC 9639 sections
 * 6 and 8.1, the last-block flag set), then, on each decode, exactly that
 * chunk's frame. A decode asks dr_flac for no more frames than one FLAC frame
 * holds, so it never reads into a chunk that has not arrived, and the checks
 * after it hold the frame to the chunk: the whole frame consumed, and its
 * channel count, rate and bit depth the stream's. A frame dr_flac cannot
 * decode (a bad CRC among them) leaves it mid-stream, so the decoder is
 * reopened and the next chunk starts clean; FLAC frames carry no state from
 * one to the next (RFC 9639 section 9), so nothing is lost but the bad one. */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "codec_backend.h"
#include "dr_flac.h"

#define STREAMINFO_LEN 34
#define HEADER_LEN (4 + 4 + STREAMINFO_LEN)

struct chorus_flac {
    drflac *dr;
    uint8_t header[HEADER_LEN];
    size_t header_at;
    const uint8_t *chunk;
    size_t chunk_len;
    size_t chunk_at;
    uint8_t channels;
    uint8_t bits;
    uint32_t sample_rate_hz;
    uint32_t max_frames;
    /* dr_flac's own sample type: `int` where the toolchain's int32_t is a
     * `long` (Xtensa), so it is decoded here and copied out. */
    drflac_int32 *decoded;
};

static size_t on_read(void *user, void *out, size_t want)
{
    chorus_flac_t *f = user;
    uint8_t *to = out;
    size_t given = 0;
    while (given < want && f->header_at < HEADER_LEN) {
        to[given++] = f->header[f->header_at++];
    }
    size_t left = f->chunk_len - f->chunk_at;
    size_t take = (want - given) < left ? (want - given) : left;
    if (take > 0) {
        memcpy(to + given, f->chunk + f->chunk_at, take);
        f->chunk_at += take;
        given += take;
    }
    return given;
}

/* The stream is never sought: there is nothing behind a chunk to seek to. */
static drflac_bool32 on_seek(void *user, int offset, drflac_seek_origin origin)
{
    (void)user;
    (void)offset;
    (void)origin;
    return DRFLAC_FALSE;
}

static drflac_bool32 on_tell(void *user, drflac_int64 *cursor)
{
    const chorus_flac_t *f = user;
    *cursor = (drflac_int64)(f->header_at + f->chunk_at);
    return DRFLAC_TRUE;
}

static int reopen(chorus_flac_t *f)
{
    if (f->dr != NULL) {
        drflac_close(f->dr);
    }
    f->header_at = 0;
    f->chunk = NULL;
    f->chunk_len = 0;
    f->chunk_at = 0;
    f->dr = drflac_open(on_read, on_seek, on_tell, f, NULL);
    return f->dr != NULL;
}

chorus_codec_status_t chorus_flac_open(chorus_flac_t **out, const chorus_codec_stream_t *stream,
                                       uint32_t *max_frames, char *detail, size_t detail_len)
{
    *out = NULL;
    const uint8_t *s = stream->config;
    if (stream->config_len != STREAMINFO_LEN || s == NULL) {
        snprintf(detail, detail_len,
                 "a FLAC codec setup is the 34-byte STREAMINFO body, not %zu bytes",
                 stream->config_len);
        return CHORUS_CODEC_UNSUPPORTED;
    }
    /* RFC 9639 section 8.2: min and max block size (16 bits each), min and max
     * frame size (24 each), then rate (20), channels - 1 (3), bits - 1 (5). */
    uint32_t min_block = ((uint32_t)s[0] << 8) | s[1];
    uint32_t max_block = ((uint32_t)s[2] << 8) | s[3];
    uint32_t rate = ((uint32_t)s[10] << 12) | ((uint32_t)s[11] << 4) | ((uint32_t)s[12] >> 4);
    uint32_t channels = ((uint32_t)(s[12] >> 1) & 0x7u) + 1;
    uint32_t bits = ((((uint32_t)s[12] & 1u) << 4) | ((uint32_t)s[13] >> 4)) + 1;
    if (min_block < 16 || max_block < min_block) {
        snprintf(detail, detail_len, "STREAMINFO block sizes %u to %u are not a FLAC stream's",
                 (unsigned)min_block, (unsigned)max_block);
        return CHORUS_CODEC_UNSUPPORTED;
    }
    if (rate != stream->sample_rate_hz || channels != stream->channels) {
        snprintf(detail, detail_len,
                 "STREAMINFO says %u Hz and %u channels and the stream format says %u Hz and %u",
                 (unsigned)rate, (unsigned)channels, (unsigned)stream->sample_rate_hz,
                 (unsigned)stream->channels);
        return CHORUS_CODEC_UNSUPPORTED;
    }
    if (bits < 4 || bits > 32) {
        snprintf(detail, detail_len, "a FLAC bit depth of %u is outside 4 to 32", (unsigned)bits);
        return CHORUS_CODEC_UNSUPPORTED;
    }
    chorus_flac_t *f = calloc(1, sizeof(*f));
    if (f == NULL) {
        snprintf(detail, detail_len, "no memory for the FLAC decoder");
        return CHORUS_CODEC_NO_MEMORY;
    }
    memcpy(f->header, "fLaC", 4);
    f->header[4] = 0x80; /* last metadata block, type 0 (STREAMINFO) */
    f->header[5] = 0;
    f->header[6] = 0;
    f->header[7] = STREAMINFO_LEN;
    memcpy(f->header + 8, s, STREAMINFO_LEN);
    f->channels = (uint8_t)channels;
    f->bits = (uint8_t)bits;
    f->sample_rate_hz = rate;
    f->max_frames = max_block;
    f->decoded = malloc((size_t)max_block * channels * sizeof(drflac_int32));
    if (f->decoded == NULL) {
        free(f);
        snprintf(detail, detail_len, "no memory for the FLAC decoder's samples");
        return CHORUS_CODEC_NO_MEMORY;
    }
    if (!reopen(f)) {
        free(f->decoded);
        free(f);
        snprintf(detail, detail_len, "dr_flac refused the STREAMINFO block");
        return CHORUS_CODEC_UNSUPPORTED;
    }
    *max_frames = max_block;
    *out = f;
    return CHORUS_CODEC_OK;
}

/* RFC 9639 section 9.1.3: assignments 0 to 7 are 1 to 8 independent channels;
 * 8, 9 and 10 are the three stereo decorrelations. */
static unsigned channels_of(unsigned assignment)
{
    return assignment <= 7 ? assignment + 1 : 2;
}

chorus_codec_status_t chorus_flac_decode(chorus_flac_t *f, const uint8_t *data, size_t len,
                                         int32_t *samples, uint32_t *frames, char *detail,
                                         size_t detail_len)
{
    *frames = 0;
    if (f->dr == NULL && !reopen(f)) {
        snprintf(detail, detail_len, "the FLAC decoder could not be reopened");
        return CHORUS_CODEC_BAD_CHUNK;
    }
    f->chunk = data;
    f->chunk_len = len;
    f->chunk_at = 0;
    /* One frame at most: dr_flac loads a frame only when the one before it is
     * used up, and a frame holds at most max_frames. */
    drflac_uint64 got = drflac_read_pcm_frames_s32(f->dr, f->max_frames, f->decoded);
    const drflac_frame *frame = &f->dr->currentFLACFrame;
    const char *wrong = NULL;
    if (got == 0) {
        wrong = "dr_flac decoded nothing from it (a bad CRC, a truncated frame or no frame)";
    } else if (frame->pcmFramesRemaining != 0 || got != frame->header.blockSizeInPCMFrames) {
        wrong = "it did not decode as exactly one frame";
    } else if (f->chunk_at != f->chunk_len) {
        wrong = "bytes follow the frame inside the chunk";
    } else if (channels_of(frame->header.channelAssignment) != f->channels) {
        wrong = "its channel count is not the stream's";
    } else if (frame->header.sampleRate != 0 && frame->header.sampleRate != f->sample_rate_hz) {
        wrong = "its sample rate is not the stream's";
    } else if (frame->header.bitsPerSample != 0 && frame->header.bitsPerSample != f->bits) {
        wrong = "its bit depth is not the stream's";
    }
    if (wrong != NULL) {
        snprintf(detail, detail_len, "a FLAC chunk of %zu bytes was refused: %s", len, wrong);
        /* Whatever state dr_flac was left in, the next chunk starts clean. */
        drflac_close(f->dr);
        f->dr = NULL;
        reopen(f);
        return CHORUS_CODEC_BAD_CHUNK;
    }
    f->chunk = NULL;
    for (size_t i = 0; i < (size_t)got * f->channels; i++) {
        samples[i] = (int32_t)f->decoded[i];
    }
    *frames = (uint32_t)got;
    return CHORUS_CODEC_OK;
}

void chorus_flac_close(chorus_flac_t *f)
{
    if (f == NULL) {
        return;
    }
    if (f->dr != NULL) {
        drflac_close(f->dr);
    }
    free(f->decoded);
    free(f);
}
