/* The endpoint's implementation of the chorus wire protocol.
 *
 * `docs/protocol.md` is the contract and the committed vectors under
 * `fixtures/protocol/` are the authority. This is a SECOND implementation of
 * that contract, not a port of the first one: it is correct when it produces
 * those bytes and recovers those fields, which is exactly what
 * `fixtures/README.md` says a second-language implementation is held to.
 *
 * No allocation. A decoder is handed a buffer it does not own and hands back
 * pointers into it, because the endpoint has one DMA-sized buffer and no heap
 * on the audio path. */

#ifndef CHORUS_PROTOCOL_H
#define CHORUS_PROTOCOL_H

#include <stddef.h>
#include <stdint.h>

/* Bytes in a frame header: one type byte plus a big-endian u16 length. */
#define CHORUS_FRAME_HEADER_LEN 3

/* Largest payload the u16 length field can describe. */
#define CHORUS_MAX_PAYLOAD_LEN 65535u

/* Fixed payload lengths and the chunk header, from docs/protocol.md. */
#define CHORUS_TIME_SYNC_PAYLOAD_LEN 32u
#define CHORUS_STREAM_END_PAYLOAD_LEN 12u
#define CHORUS_CHUNK_HEADER_LEN 32u
#define CHORUS_RESERVED_LEN 14u
#define CHORUS_RESERVED_OFFSET 18u

/* Field ranges the format accepts. */
#define CHORUS_MAX_CHANNELS 8u
#define CHORUS_MIN_SAMPLE_RATE_HZ 8000u
#define CHORUS_MAX_SAMPLE_RATE_HZ 384000u

/* The catalogued message types, by wire byte. */
typedef enum {
    CHORUS_MSG_TIME_SYNC = 0x01,
    CHORUS_MSG_AUDIO_CHUNK = 0x02,
    CHORUS_MSG_STREAM_END = 0x03
} chorus_message_type_t;

/* How the PCM bytes of an audio chunk are laid out. */
typedef enum {
    CHORUS_FMT_PCM_S16LE = 1,
    CHORUS_FMT_PCM_S24LE = 2,
    CHORUS_FMT_PCM_F32LE = 3
} chorus_sample_format_t;

/* Bytes one sample of one channel occupies, or 0 for a value the format does
 * not define. */
size_t chorus_sample_format_bytes(uint8_t wire);

/* Short stable name, matching the Rust implementation's and the `.fields`
 * files'. NULL for a value the format does not define. */
const char *chorus_sample_format_name(uint8_t wire);

/* The wire byte for a format name, or 0 when the name is not one. */
uint8_t chorus_sample_format_from_name(const char *name);

/* Short stable name of a catalogued type, or NULL. */
const char *chorus_message_type_name(uint8_t wire);

/* The wire byte for a type name, or 0 when the name is not one. */
uint8_t chorus_message_type_from_name(const char *name);

/* Smallest payload a frame of this catalogued type can carry, or 0 for a type
 * outside the catalog. */
size_t chorus_min_payload_len(uint8_t wire);

/* The four timestamps of one RFC 5905 section 8 exchange. Every one is
 * nanoseconds from a MONOTONIC source on the device that took it. */
typedef struct {
    uint64_t t0_ns;
    uint64_t t1_ns;
    uint64_t t2_ns;
    uint64_t t3_ns;
} chorus_time_sync_t;

/* Round trip time of the exchange, in nanoseconds. Saturating, so timestamps
 * that cannot be a round trip yield 0 rather than wrapping. */
uint64_t chorus_time_sync_rtt_ns(const chorus_time_sync_t *ts);

/* Estimated offset of the server clock from the client clock, in nanoseconds:
 * ((t1 - t0) + (t2 - t3)) / 2. */
int64_t chorus_time_sync_offset_ns(const chorus_time_sync_t *ts);

/* One chunk of PCM on the server timeline. `audio_data` points into the
 * caller's buffer and is not owned. */
typedef struct {
    uint32_t sequence;
    uint64_t timestamp_ns;
    uint32_t sample_rate_hz;
    uint16_t channels;
    uint8_t sample_format;
    uint8_t reserved[CHORUS_RESERVED_LEN];
    const uint8_t *audio_data;
    size_t audio_data_len;
} chorus_audio_chunk_t;

/* The in-band end of a stream. */
typedef struct {
    uint32_t final_sequence;
    uint64_t end_timestamp_ns;
} chorus_stream_end_t;

/* What a decoder made of one frame. The order these can occur in is the
 * decoder-behaviour order in docs/protocol.md, and it is the contract. */
typedef enum {
    CHORUS_FRAME_DECODED,
    /* A type byte outside the catalog. Stepped over using its length prefix;
     * the session stays open. */
    CHORUS_FRAME_SKIPPED_UNKNOWN_TYPE,
    /* Fewer bytes remain than a header needs. Consumes nothing. */
    CHORUS_FRAME_TRUNCATED_HEADER,
    /* The length field declares more payload than the buffer holds. Consumes
     * nothing, so a stream reader waits for more instead of guessing. */
    CHORUS_FRAME_DECLARED_LENGTH_EXCEEDS_BUFFER,
    /* The declared payload is shorter than this type's minimum. One frame. */
    CHORUS_FRAME_PAYLOAD_TOO_SHORT_FOR_TYPE,
    /* Right size, and still a field value the format cannot accept. One
     * frame. */
    CHORUS_FRAME_INVALID_FIELD
} chorus_frame_outcome_t;

/* Which field a CHORUS_FRAME_INVALID_FIELD outcome was about. */
typedef enum {
    CHORUS_FIELD_NONE = 0,
    CHORUS_FIELD_UNDEFINED_SAMPLE_FORMAT,
    CHORUS_FIELD_CHANNELS_OUT_OF_RANGE,
    CHORUS_FIELD_SAMPLE_RATE_OUT_OF_RANGE,
    CHORUS_FIELD_AUDIO_DATA_NOT_FRAME_ALIGNED,
    CHORUS_FIELD_EMPTY_AUDIO_DATA
} chorus_invalid_field_t;

/* One frame's outcome plus how far the decoder got.
 *
 * `consumed` of 0 means the next frame boundary is not knowable from what is
 * present, so a caller must stop rather than guess. It never means the session
 * is over. */
typedef struct {
    chorus_frame_outcome_t outcome;
    size_t consumed;
    uint8_t message_type;
    size_t payload_len;
    chorus_invalid_field_t invalid_field;
    union {
        chorus_time_sync_t time_sync;
        chorus_audio_chunk_t audio_chunk;
        chorus_stream_end_t stream_end;
    } message;
} chorus_frame_t;

/* A short stable name for an outcome, for diagnostics and the event log. */
const char *chorus_frame_outcome_name(chorus_frame_outcome_t outcome);

/* Decode the frame at the front of `buf`. Never reads past `len`, whatever the
 * length field claims. */
chorus_frame_t chorus_decode_frame(const uint8_t *buf, size_t len);

/* Why a message was not encoded. An encoder that hits one of these emits
 * nothing at all: a partial or wrapped frame is never written. */
typedef enum {
    CHORUS_ENCODE_OK = 0,
    CHORUS_ENCODE_NOT_REPRESENTABLE,
    CHORUS_ENCODE_FIELD_OUT_OF_RANGE,
    CHORUS_ENCODE_PAYLOAD_TOO_LONG,
    CHORUS_ENCODE_INVALID_FIELD,
    /* The caller's buffer is too small for the frame. Nothing is written. */
    CHORUS_ENCODE_BUFFER_TOO_SMALL
} chorus_encode_status_t;

const char *chorus_encode_status_name(chorus_encode_status_t status);

/* Encode one message into a complete frame in `out`. `written` receives the
 * frame length on success and is untouched otherwise. */
chorus_encode_status_t chorus_encode_time_sync(const chorus_time_sync_t *ts, uint8_t *out,
                                               size_t out_len, size_t *written);
chorus_encode_status_t chorus_encode_audio_chunk(const chorus_audio_chunk_t *chunk, uint8_t *out,
                                                 size_t out_len, size_t *written);
chorus_encode_status_t chorus_encode_stream_end(const chorus_stream_end_t *end, uint8_t *out,
                                                size_t out_len, size_t *written);

#endif /* CHORUS_PROTOCOL_H */
