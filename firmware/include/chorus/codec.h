/* The endpoint's decoders: FLAC and Opus, one coded_chunk at a time.
 *
 * docs/protocol.md, "Codecs: PCM, FLAC and Opus": a stream_format names the
 * codec, the output sample format, the rate, the channel count and a codec
 * setup (`codec_config`); every coded_chunk after it carries ONE FLAC frame or
 * ONE Opus packet and `frames`, the PCM frames it decodes to. So a decoder here
 * is opened from a stream_format's fields and then driven one chunk at a time,
 * and it never reads ahead: what a chunk decodes to depends on that chunk and
 * the decoder's state after the chunks before it, nothing else.
 *
 * The implementations are vendored (proposal P9, Option A, approved at
 * Checkpoint K; pins in third_party/README.md): dr_flac 0.13.3 for FLAC and
 * libopus 1.6.1 built fixed-point for Opus. This file is the seam the session
 * calls; nothing above it names either library.
 *
 * Output: interleaved little-endian PCM in the stream's `sample_format`
 * (CHORUS_CODEC_S16 is 2 bytes per sample, CHORUS_CODEC_S24 is 3 packed
 * bytes), channels in the codec's own coded order, which is the interleave
 * order the stream_format's channel map names (FLAC: RFC 9639 section 9.1.3;
 * Opus mapping family 0: left, right). A FLAC stream whose bit depth differs
 * from the output width is shifted to it (a narrower source gains zero low
 * bits; a wider one loses its low bits).
 *
 * Opus: the OpusHead's pre-skip (RFC 7845 section 4.2) is dropped from the
 * start of the stream, so the first chunks may yield fewer frames than they
 * carry, and its output gain (section 5.1, Q7.8 dB) is applied. Mapping family
 * 0 only (one or two channels); a family 1 stream is refused by name until a
 * surround Opus stream is wanted. */

#ifndef CHORUS_CODEC_H
#define CHORUS_CODEC_H

#include <stddef.h>
#include <stdint.h>

/* The `codec` byte of stream_format (docs/protocol.md, 0x12). */
#define CHORUS_CODEC_FLAC 2
#define CHORUS_CODEC_OPUS 3

/* The `sample_format` byte of stream_format: the decoder's output. */
#define CHORUS_CODEC_S16 1
#define CHORUS_CODEC_S24 2

/* The most frames one Opus packet decodes to: 120 ms at 48 kHz (RFC 6716
 * section 3.2.5, a code 3 packet of up to 120 ms). */
#define CHORUS_CODEC_OPUS_MAX_FRAMES 5760

typedef enum {
    CHORUS_CODEC_OK = 0,
    /* The stream_format cannot be decoded here (codec, format, rate, channels
     * or codec setup), said in `detail`. */
    CHORUS_CODEC_UNSUPPORTED = -1,
    /* A chunk that did not decode, or decoded to a frame count other than the
     * chunk's `frames`. The decoder is reset and takes the next chunk. */
    CHORUS_CODEC_BAD_CHUNK = -2,
    /* The output buffer is smaller than the chunk decodes to. */
    CHORUS_CODEC_NO_ROOM = -3,
    /* Memory for the decoder could not be had. */
    CHORUS_CODEC_NO_MEMORY = -4
} chorus_codec_status_t;

typedef struct chorus_decoder chorus_decoder_t;

typedef struct {
    uint8_t codec;
    uint8_t sample_format;
    uint32_t sample_rate_hz;
    uint8_t channels;
    const uint8_t *config;
    size_t config_len;
} chorus_codec_stream_t;

/* Open a decoder for one stream. On failure `*out` is NULL and `detail` says
 * why in a sentence. */
chorus_codec_status_t chorus_codec_open(chorus_decoder_t **out, const chorus_codec_stream_t *stream,
                                        char *detail, size_t detail_len);

/* Decode one coded_chunk: `data` is the FLAC frame or Opus packet and `frames`
 * the chunk's field. Writes `*frames_out` frames (fewer than `frames` only
 * while an Opus pre-skip is being dropped, possibly 0) to `pcm`. */
chorus_codec_status_t chorus_codec_decode(chorus_decoder_t *decoder, const uint8_t *data,
                                          size_t len, uint32_t frames, uint8_t *pcm,
                                          size_t pcm_capacity, uint32_t *frames_out, char *detail,
                                          size_t detail_len);

/* The most frames one chunk of this stream can decode to (FLAC: the
 * STREAMINFO maximum block size; Opus: CHORUS_CODEC_OPUS_MAX_FRAMES). */
uint32_t chorus_codec_max_frames(const chorus_decoder_t *decoder);

/* Bytes per output frame: channels times 2 or 3. */
size_t chorus_codec_frame_bytes(const chorus_decoder_t *decoder);

/* Opus: the range coder's final state after the last chunk (OPUS_GET_FINAL_RANGE,
 * RFC 6716 section 4.1.6), which the official test vectors carry per packet.
 * FLAC: 0. */
uint32_t chorus_codec_final_range(const chorus_decoder_t *decoder);

void chorus_codec_close(chorus_decoder_t *decoder);

#endif /* CHORUS_CODEC_H */
