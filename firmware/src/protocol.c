#include "chorus/protocol.h"

#include <string.h>

/* Big-endian readers and writers. Written out rather than reached for through
 * a byte-order header, because the byte order of the wire is a property of the
 * protocol and not of the machine this happens to compile on. */

static uint32_t read_u32_be(const uint8_t *src)
{
    return ((uint32_t)src[0] << 24) | ((uint32_t)src[1] << 16) | ((uint32_t)src[2] << 8) |
           (uint32_t)src[3];
}

static uint64_t read_u64_be(const uint8_t *src)
{
    uint64_t value = 0;
    for (int i = 0; i < 8; i++) {
        value = (value << 8) | (uint64_t)src[i];
    }
    return value;
}

static void write_u16_be(uint8_t *dst, uint16_t value)
{
    dst[0] = (uint8_t)(value >> 8);
    dst[1] = (uint8_t)(value & 0xFF);
}

static void write_u32_be(uint8_t *dst, uint32_t value)
{
    dst[0] = (uint8_t)(value >> 24);
    dst[1] = (uint8_t)((value >> 16) & 0xFF);
    dst[2] = (uint8_t)((value >> 8) & 0xFF);
    dst[3] = (uint8_t)(value & 0xFF);
}

static void write_u64_be(uint8_t *dst, uint64_t value)
{
    for (int i = 0; i < 8; i++) {
        dst[i] = (uint8_t)((value >> (56 - 8 * i)) & 0xFF);
    }
}

size_t chorus_sample_format_bytes(uint8_t wire)
{
    switch (wire) {
    case CHORUS_FMT_PCM_S16LE:
        return 2;
    case CHORUS_FMT_PCM_S24LE:
        return 3;
    case CHORUS_FMT_PCM_F32LE:
        return 4;
    default:
        return 0;
    }
}

const char *chorus_sample_format_name(uint8_t wire)
{
    switch (wire) {
    case CHORUS_FMT_PCM_S16LE:
        return "pcm_s16le";
    case CHORUS_FMT_PCM_S24LE:
        return "pcm_s24le";
    case CHORUS_FMT_PCM_F32LE:
        return "pcm_f32le";
    default:
        return NULL;
    }
}

uint8_t chorus_sample_format_from_name(const char *name)
{
    for (uint8_t wire = 1; wire <= 3; wire++) {
        const char *known = chorus_sample_format_name(wire);
        if (known != NULL && strcmp(known, name) == 0) {
            return wire;
        }
    }
    return 0;
}

const char *chorus_message_type_name(uint8_t wire)
{
    switch (wire) {
    case CHORUS_MSG_TIME_SYNC:
        return "time_sync";
    case CHORUS_MSG_AUDIO_CHUNK:
        return "audio_chunk";
    case CHORUS_MSG_STREAM_END:
        return "stream_end";
    default:
        return NULL;
    }
}

uint8_t chorus_message_type_from_name(const char *name)
{
    for (uint8_t wire = 1; wire <= 3; wire++) {
        const char *known = chorus_message_type_name(wire);
        if (known != NULL && strcmp(known, name) == 0) {
            return wire;
        }
    }
    return 0;
}

size_t chorus_min_payload_len(uint8_t wire)
{
    switch (wire) {
    case CHORUS_MSG_TIME_SYNC:
        return CHORUS_TIME_SYNC_PAYLOAD_LEN;
    case CHORUS_MSG_AUDIO_CHUNK:
        /* An audio chunk carries at least one byte of PCM, which is what makes
         * a chunk with no samples unrepresentable rather than merely invalid
         * (docs/decisions/0005-decoder-frame-validation.md). */
        return CHORUS_CHUNK_HEADER_LEN + 1;
    case CHORUS_MSG_STREAM_END:
        return CHORUS_STREAM_END_PAYLOAD_LEN;
    default:
        return 0;
    }
}

uint64_t chorus_time_sync_rtt_ns(const chorus_time_sync_t *ts)
{
    uint64_t elapsed_client = (ts->t3_ns > ts->t0_ns) ? ts->t3_ns - ts->t0_ns : 0;
    uint64_t elapsed_server = (ts->t2_ns > ts->t1_ns) ? ts->t2_ns - ts->t1_ns : 0;
    return (elapsed_client > elapsed_server) ? elapsed_client - elapsed_server : 0;
}

int64_t chorus_time_sync_offset_ns(const chorus_time_sync_t *ts)
{
    /* The Rust implementation widens to i128 before halving. Two 64-bit
     * differences of unsigned nanoseconds cannot overflow a signed 128-bit
     * value there, and here the same is true of the two differences held as
     * long double-free integer halves: each difference fits in an int64_t
     * whenever the timestamps are real monotonic readings, and the sum is
     * halved before it can overflow by halving each side. */
    int64_t a = (int64_t)(ts->t1_ns - ts->t0_ns);
    int64_t b = (int64_t)(ts->t2_ns - ts->t3_ns);
    /* (a + b) / 2 computed so that the intermediate sum cannot overflow:
     * a/2 + b/2 plus the carry of the two remainders, truncating toward zero
     * exactly as an i128 division by 2 does. */
    int64_t half = (a / 2) + (b / 2);
    int64_t rest = (a % 2) + (b % 2);
    return half + rest / 2;
}

const char *chorus_frame_outcome_name(chorus_frame_outcome_t outcome)
{
    switch (outcome) {
    case CHORUS_FRAME_DECODED:
        return "decoded";
    case CHORUS_FRAME_SKIPPED_UNKNOWN_TYPE:
        return "skipped-unknown-type";
    case CHORUS_FRAME_TRUNCATED_HEADER:
        return "truncated-header";
    case CHORUS_FRAME_DECLARED_LENGTH_EXCEEDS_BUFFER:
        return "declared-length-exceeds-buffer";
    case CHORUS_FRAME_PAYLOAD_TOO_SHORT_FOR_TYPE:
        return "payload-too-short-for-type";
    case CHORUS_FRAME_INVALID_FIELD:
        return "invalid-field";
    }
    return "unknown-outcome";
}

static chorus_frame_t invalid_field(uint8_t type_byte, size_t frame_len, size_t declared,
                                    chorus_invalid_field_t which)
{
    chorus_frame_t frame;
    memset(&frame, 0, sizeof(frame));
    frame.outcome = CHORUS_FRAME_INVALID_FIELD;
    frame.consumed = frame_len;
    frame.message_type = type_byte;
    frame.payload_len = declared;
    frame.invalid_field = which;
    return frame;
}

chorus_frame_t chorus_decode_frame(const uint8_t *buf, size_t len)
{
    chorus_frame_t frame;
    memset(&frame, 0, sizeof(frame));

    /* 1. Is there a whole header? */
    if (len < CHORUS_FRAME_HEADER_LEN) {
        frame.outcome = CHORUS_FRAME_TRUNCATED_HEADER;
        frame.consumed = 0;
        return frame;
    }

    uint8_t type_byte = buf[0];
    size_t declared = ((size_t)buf[1] << 8) | (size_t)buf[2];
    size_t available = len - CHORUS_FRAME_HEADER_LEN;

    /* 2. Does the declared length fit in what is actually here? Before any
     *    slice of the payload is taken. */
    if (declared > available) {
        frame.outcome = CHORUS_FRAME_DECLARED_LENGTH_EXCEEDS_BUFFER;
        frame.consumed = 0;
        frame.message_type = type_byte;
        frame.payload_len = declared;
        return frame;
    }

    size_t frame_len = CHORUS_FRAME_HEADER_LEN + declared;
    const uint8_t *payload = buf + CHORUS_FRAME_HEADER_LEN;

    /* 3. Unknown type: step over it, do not fail the session. */
    size_t minimum = chorus_min_payload_len(type_byte);
    if (minimum == 0) {
        frame.outcome = CHORUS_FRAME_SKIPPED_UNKNOWN_TYPE;
        frame.consumed = frame_len;
        frame.message_type = type_byte;
        frame.payload_len = declared;
        return frame;
    }

    /* 4. Long enough for what it claims to be? */
    if (declared < minimum) {
        frame.outcome = CHORUS_FRAME_PAYLOAD_TOO_SHORT_FOR_TYPE;
        frame.consumed = frame_len;
        frame.message_type = type_byte;
        frame.payload_len = declared;
        return frame;
    }

    /* 5. Field values. */
    frame.consumed = frame_len;
    frame.message_type = type_byte;
    frame.payload_len = declared;

    switch (type_byte) {
    case CHORUS_MSG_TIME_SYNC:
        frame.outcome = CHORUS_FRAME_DECODED;
        frame.message.time_sync.t0_ns = read_u64_be(payload + 0);
        frame.message.time_sync.t1_ns = read_u64_be(payload + 8);
        frame.message.time_sync.t2_ns = read_u64_be(payload + 16);
        frame.message.time_sync.t3_ns = read_u64_be(payload + 24);
        return frame;

    case CHORUS_MSG_STREAM_END:
        frame.outcome = CHORUS_FRAME_DECODED;
        frame.message.stream_end.final_sequence = read_u32_be(payload + 0);
        frame.message.stream_end.end_timestamp_ns = read_u64_be(payload + 4);
        return frame;

    case CHORUS_MSG_AUDIO_CHUNK: {
        uint32_t sequence = read_u32_be(payload + 0);
        uint64_t timestamp_ns = read_u64_be(payload + 4);
        uint32_t sample_rate_hz = read_u32_be(payload + 12);
        uint8_t channels_byte = payload[16];
        uint8_t format_byte = payload[17];

        if (chorus_sample_format_bytes(format_byte) == 0) {
            return invalid_field(type_byte, frame_len, declared,
                                 CHORUS_FIELD_UNDEFINED_SAMPLE_FORMAT);
        }
        if (channels_byte == 0 || (uint16_t)channels_byte > CHORUS_MAX_CHANNELS) {
            return invalid_field(type_byte, frame_len, declared,
                                 CHORUS_FIELD_CHANNELS_OUT_OF_RANGE);
        }
        if (sample_rate_hz < CHORUS_MIN_SAMPLE_RATE_HZ ||
            sample_rate_hz > CHORUS_MAX_SAMPLE_RATE_HZ) {
            return invalid_field(type_byte, frame_len, declared,
                                 CHORUS_FIELD_SAMPLE_RATE_OUT_OF_RANGE);
        }

        size_t audio_len = declared - CHORUS_CHUNK_HEADER_LEN;
        if (audio_len == 0) {
            return invalid_field(type_byte, frame_len, declared, CHORUS_FIELD_EMPTY_AUDIO_DATA);
        }
        size_t frame_bytes = (size_t)channels_byte * chorus_sample_format_bytes(format_byte);
        if (audio_len % frame_bytes != 0) {
            return invalid_field(type_byte, frame_len, declared,
                                 CHORUS_FIELD_AUDIO_DATA_NOT_FRAME_ALIGNED);
        }

        frame.outcome = CHORUS_FRAME_DECODED;
        frame.message.audio_chunk.sequence = sequence;
        frame.message.audio_chunk.timestamp_ns = timestamp_ns;
        frame.message.audio_chunk.sample_rate_hz = sample_rate_hz;
        frame.message.audio_chunk.channels = (uint16_t)channels_byte;
        frame.message.audio_chunk.sample_format = format_byte;
        memcpy(frame.message.audio_chunk.reserved, payload + CHORUS_RESERVED_OFFSET,
               CHORUS_RESERVED_LEN);
        frame.message.audio_chunk.audio_data = payload + CHORUS_CHUNK_HEADER_LEN;
        frame.message.audio_chunk.audio_data_len = audio_len;
        return frame;
    }

    default:
        /* Unreachable: chorus_min_payload_len returned nonzero, so the type is
         * catalogued and handled above. */
        frame.outcome = CHORUS_FRAME_SKIPPED_UNKNOWN_TYPE;
        return frame;
    }
}

const char *chorus_encode_status_name(chorus_encode_status_t status)
{
    switch (status) {
    case CHORUS_ENCODE_OK:
        return "ok";
    case CHORUS_ENCODE_NOT_REPRESENTABLE:
        return "not-representable";
    case CHORUS_ENCODE_FIELD_OUT_OF_RANGE:
        return "field-out-of-range";
    case CHORUS_ENCODE_PAYLOAD_TOO_LONG:
        return "payload-too-long";
    case CHORUS_ENCODE_INVALID_FIELD:
        return "invalid-field";
    case CHORUS_ENCODE_BUFFER_TOO_SMALL:
        return "buffer-too-small";
    }
    return "unknown-status";
}

static chorus_encode_status_t frame_out(uint8_t type_byte, const uint8_t *payload,
                                        size_t payload_len, uint8_t *out, size_t out_len,
                                        size_t *written)
{
    if (payload_len > CHORUS_MAX_PAYLOAD_LEN) {
        return CHORUS_ENCODE_PAYLOAD_TOO_LONG;
    }
    size_t total = CHORUS_FRAME_HEADER_LEN + payload_len;
    if (out_len < total) {
        return CHORUS_ENCODE_BUFFER_TOO_SMALL;
    }
    out[0] = type_byte;
    write_u16_be(out + 1, (uint16_t)payload_len);
    memcpy(out + CHORUS_FRAME_HEADER_LEN, payload, payload_len);
    *written = total;
    return CHORUS_ENCODE_OK;
}

chorus_encode_status_t chorus_encode_time_sync(const chorus_time_sync_t *ts, uint8_t *out,
                                               size_t out_len, size_t *written)
{
    uint8_t payload[CHORUS_TIME_SYNC_PAYLOAD_LEN];
    write_u64_be(payload + 0, ts->t0_ns);
    write_u64_be(payload + 8, ts->t1_ns);
    write_u64_be(payload + 16, ts->t2_ns);
    write_u64_be(payload + 24, ts->t3_ns);
    return frame_out(CHORUS_MSG_TIME_SYNC, payload, sizeof(payload), out, out_len, written);
}

chorus_encode_status_t chorus_encode_stream_end(const chorus_stream_end_t *end, uint8_t *out,
                                                size_t out_len, size_t *written)
{
    uint8_t payload[CHORUS_STREAM_END_PAYLOAD_LEN];
    write_u32_be(payload + 0, end->final_sequence);
    write_u64_be(payload + 4, end->end_timestamp_ns);
    return frame_out(CHORUS_MSG_STREAM_END, payload, sizeof(payload), out, out_len, written);
}

chorus_encode_status_t chorus_encode_audio_chunk(const chorus_audio_chunk_t *chunk, uint8_t *out,
                                                 size_t out_len, size_t *written)
{
    if (chunk->channels > 0xFF) {
        return CHORUS_ENCODE_NOT_REPRESENTABLE;
    }
    if (chunk->channels == 0 || chunk->channels > CHORUS_MAX_CHANNELS) {
        return CHORUS_ENCODE_FIELD_OUT_OF_RANGE;
    }
    if (chunk->sample_rate_hz < CHORUS_MIN_SAMPLE_RATE_HZ ||
        chunk->sample_rate_hz > CHORUS_MAX_SAMPLE_RATE_HZ) {
        return CHORUS_ENCODE_FIELD_OUT_OF_RANGE;
    }
    size_t sample_bytes = chorus_sample_format_bytes(chunk->sample_format);
    if (sample_bytes == 0) {
        return CHORUS_ENCODE_INVALID_FIELD;
    }
    if (chunk->audio_data_len == 0) {
        return CHORUS_ENCODE_INVALID_FIELD;
    }
    size_t frame_bytes = (size_t)chunk->channels * sample_bytes;
    if (chunk->audio_data_len % frame_bytes != 0) {
        return CHORUS_ENCODE_INVALID_FIELD;
    }
    size_t payload_len = CHORUS_CHUNK_HEADER_LEN + chunk->audio_data_len;
    if (payload_len > CHORUS_MAX_PAYLOAD_LEN) {
        return CHORUS_ENCODE_PAYLOAD_TOO_LONG;
    }
    size_t total = CHORUS_FRAME_HEADER_LEN + payload_len;
    if (out_len < total) {
        return CHORUS_ENCODE_BUFFER_TOO_SMALL;
    }

    uint8_t *header = out + CHORUS_FRAME_HEADER_LEN;
    out[0] = CHORUS_MSG_AUDIO_CHUNK;
    write_u16_be(out + 1, (uint16_t)payload_len);
    write_u32_be(header + 0, chunk->sequence);
    write_u64_be(header + 4, chunk->timestamp_ns);
    write_u32_be(header + 12, chunk->sample_rate_hz);
    header[16] = (uint8_t)chunk->channels;
    header[17] = chunk->sample_format;
    memcpy(header + CHORUS_RESERVED_OFFSET, chunk->reserved, CHORUS_RESERVED_LEN);
    memcpy(header + CHORUS_CHUNK_HEADER_LEN, chunk->audio_data, chunk->audio_data_len);
    *written = total;
    return CHORUS_ENCODE_OK;
}
