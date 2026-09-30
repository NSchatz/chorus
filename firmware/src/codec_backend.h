/* What codec.c needs from each decoder backend, and nothing the session sees.
 *
 * codec.c owns the stream's shape (format, channels, the frame count checks
 * and the packing into the output format); a backend turns one FLAC frame or
 * Opus packet into signed 32-bit samples, interleaved, left-justified at the
 * backend's own `bits` (24 for Opus, the stream's bit depth for FLAC), and
 * says how many frames that was. Private to firmware/src. */

#ifndef CHORUS_CODEC_BACKEND_H
#define CHORUS_CODEC_BACKEND_H

#include <stddef.h>
#include <stdint.h>

#include "chorus/codec.h"

typedef struct chorus_flac chorus_flac_t;
typedef struct chorus_opus chorus_opus_t;

/* FLAC (codec_flac.c, over dr_flac). `config` is the 34-byte STREAMINFO body. */
chorus_codec_status_t chorus_flac_open(chorus_flac_t **out, const chorus_codec_stream_t *stream,
                                       uint32_t *max_frames, char *detail, size_t detail_len);
/* Decodes one frame into `samples` (max_frames * channels values, each the
 * sample left-justified in 32 bits) and returns its frame count in `*frames`. */
chorus_codec_status_t chorus_flac_decode(chorus_flac_t *flac, const uint8_t *data, size_t len,
                                         int32_t *samples, uint32_t *frames, char *detail,
                                         size_t detail_len);
void chorus_flac_close(chorus_flac_t *flac);

/* Opus (codec_opus.c, over libopus). `config` is the OpusHead. */
chorus_codec_status_t chorus_opus_open(chorus_opus_t **out, const chorus_codec_stream_t *stream,
                                       uint32_t *pre_skip, char *detail, size_t detail_len);
/* Decodes one packet; S16 output uses libopus's 16-bit call, S24 its 24-bit
 * one, and either way `samples` holds the value left-justified in 32 bits. */
chorus_codec_status_t chorus_opus_decode(chorus_opus_t *opus, const uint8_t *data, size_t len,
                                         int32_t *samples, uint32_t capacity_frames,
                                         uint32_t *frames, char *detail, size_t detail_len);
uint32_t chorus_opus_final_range(const chorus_opus_t *opus);
void chorus_opus_close(chorus_opus_t *opus);

#endif /* CHORUS_CODEC_BACKEND_H */
