/* The endpoint's protocol core, held to the committed golden vectors.
 *
 * fixtures/README.md: "A second-language implementation (the ESP32-S3 C
 * components in the embedded phase) is expected to run the same two assertions
 * against the same files." Those two assertions are here, byte for byte and
 * field for field, plus the unrecognised-type frame AC-7 asks for and the
 * decoder-behaviour order docs/protocol.md makes normative.
 *
 * The vectors are READ and never written. Nothing in this file generates a
 * fixture. */

#include "chorus/protocol.h"
#include "harness.h"

#include <ctype.h>
#include <dirent.h>
#include <errno.h>
#include <inttypes.h>
#include <sys/stat.h>
#include <unistd.h>

/* Every type this endpoint mirrors. It is checked against the committed
 * directory below and never trusted on its own: a list compared with a literal
 * in the same file agrees with itself whatever the fixtures say. */
static const char *const CATALOG[] = {"time_sync", "audio_chunk", "stream_end"};
#define CATALOG_COUNT (sizeof(CATALOG) / sizeof(CATALOG[0]))

/* --- the committed directory is the catalog --------------------------------
 *
 * fixtures/README.md makes fixtures/protocol/ the contract between the two
 * implementations: "A second-language implementation [...] is expected to run
 * the same two assertions against the same files. That is what keeps the two
 * implementations from drifting apart." So the vectors this file round trips
 * are DISCOVERED by reading that directory, the way test_sync.c discovers
 * fixtures/sync, and the list above is held to what is found. A vector pair
 * committed for a type this endpoint does not mirror is drift, and turns this
 * suite red naming the type. */

#define MAX_COMMITTED_VECTORS 32

typedef struct {
    char stem[128];
} committed_vector_t;

typedef struct {
    committed_vector_t vectors[MAX_COMMITTED_VECTORS];
    size_t count;
    int unreadable;
    int overflowed;
} committed_catalog_t;

static int compare_stems(const void *a, const void *b)
{
    return strcmp(((const committed_vector_t *)a)->stem, ((const committed_vector_t *)b)->stem);
}

static void read_committed_catalog(const char *dir_path, committed_catalog_t *out)
{
    memset(out, 0, sizeof(*out));
    DIR *dir = opendir(dir_path);
    if (dir == NULL) {
        out->unreadable = 1;
        return;
    }
    struct dirent *entry;
    while ((entry = readdir(dir)) != NULL) {
        const char *dot = strrchr(entry->d_name, '.');
        if (dot == NULL || strcmp(dot, ".hex") != 0) {
            continue;
        }
        size_t stem_len = (size_t)(dot - entry->d_name);
        if (out->count >= MAX_COMMITTED_VECTORS ||
            stem_len >= sizeof(out->vectors[0].stem)) {
            out->overflowed = 1;
            break;
        }
        memcpy(out->vectors[out->count].stem, entry->d_name, stem_len);
        out->vectors[out->count].stem[stem_len] = '\0';
        out->count++;
    }
    closedir(dir);
    qsort(out->vectors, out->count, sizeof(out->vectors[0]), compare_stems);
}

/* The drift verdict, COMPUTED rather than asserted, so the same function can be
 * taken over a scratch directory and shown going red. Empty strings mean the
 * two sets are the same set. */
typedef struct {
    char unmirrored[128];
    char without_vector[128];
} catalog_drift_t;

static int in_catalog(const char *stem)
{
    for (size_t i = 0; i < CATALOG_COUNT; i++) {
        if (strcmp(CATALOG[i], stem) == 0) {
            return 1;
        }
    }
    return 0;
}

static int is_committed(const committed_catalog_t *committed, const char *stem)
{
    for (size_t i = 0; i < committed->count; i++) {
        if (strcmp(committed->vectors[i].stem, stem) == 0) {
            return 1;
        }
    }
    return 0;
}

static catalog_drift_t catalog_drift(const committed_catalog_t *committed)
{
    catalog_drift_t drift;
    memset(&drift, 0, sizeof(drift));
    for (size_t i = 0; i < committed->count; i++) {
        if (!in_catalog(committed->vectors[i].stem)) {
            snprintf(drift.unmirrored, sizeof(drift.unmirrored), "%s",
                     committed->vectors[i].stem);
            break;
        }
    }
    for (size_t i = 0; i < CATALOG_COUNT; i++) {
        if (!is_committed(committed, CATALOG[i])) {
            snprintf(drift.without_vector, sizeof(drift.without_vector), "%s", CATALOG[i]);
            break;
        }
    }
    return drift;
}

static char *read_text(const char *path)
{
    FILE *file = fopen(path, "rb");
    if (file == NULL) {
        return NULL;
    }
    static char storage[8][65536];
    static int next;
    char *text = storage[next % 8];
    next++;
    size_t read = fread(text, 1, 65535, file);
    fclose(file);
    text[read] = '\0';
    return text;
}

/* Parse a `.hex` file: whitespace separated hex bytes, `#` starts a comment. */
static size_t parse_hex(const char *text, uint8_t *out, size_t out_len)
{
    size_t count = 0;
    const char *cursor = text;
    while (*cursor != '\0') {
        if (*cursor == '#') {
            while (*cursor != '\0' && *cursor != '\n') {
                cursor++;
            }
            continue;
        }
        if (isspace((unsigned char)*cursor)) {
            cursor++;
            continue;
        }
        if (!isxdigit((unsigned char)cursor[0]) || !isxdigit((unsigned char)cursor[1])) {
            return (size_t)-1;
        }
        unsigned value = 0;
        if (sscanf(cursor, "%2x", &value) != 1 || count >= out_len) {
            return (size_t)-1;
        }
        out[count++] = (uint8_t)value;
        cursor += 2;
    }
    return count;
}

/* The `.fields` files use the same `key = value` shape as everything else in
 * this tree, and a byte-string value is unseparated hex. */
static const char *field_value(const char *text, const char *key, char *buffer, size_t buffer_len)
{
    const char *cursor = text;
    size_t key_len = strlen(key);
    while (*cursor != '\0') {
        const char *eol = strchr(cursor, '\n');
        size_t len = (eol == NULL) ? strlen(cursor) : (size_t)(eol - cursor);
        char line[512];
        size_t copy = (len < sizeof(line) - 1) ? len : sizeof(line) - 1;
        memcpy(line, cursor, copy);
        line[copy] = '\0';
        cursor = (eol == NULL) ? cursor + len : eol + 1;

        char *hash = strchr(line, '#');
        if (hash != NULL) {
            *hash = '\0';
        }
        char *start = line;
        while (*start == ' ' || *start == '\t') {
            start++;
        }
        if (strncmp(start, key, key_len) != 0) {
            continue;
        }
        const char *after = start + key_len;
        while (*after == ' ' || *after == '\t') {
            after++;
        }
        if (*after != '=') {
            continue;
        }
        after++;
        while (*after == ' ' || *after == '\t') {
            after++;
        }
        size_t value_len = strlen(after);
        while (value_len > 0 && (after[value_len - 1] == ' ' || after[value_len - 1] == '\t' ||
                                 after[value_len - 1] == '\r')) {
            value_len--;
        }
        if (value_len + 1 > buffer_len) {
            return NULL;
        }
        memcpy(buffer, after, value_len);
        buffer[value_len] = '\0';
        return buffer;
    }
    return NULL;
}

static uint64_t field_u64(const char *text, const char *key, int *ok)
{
    char buffer[512];
    const char *value = field_value(text, key, buffer, sizeof(buffer));
    if (value == NULL) {
        *ok = 0;
        return 0;
    }
    *ok = 1;
    return strtoull(value, NULL, 10);
}

static size_t field_bytes(const char *text, const char *key, uint8_t *out, size_t out_len)
{
    char buffer[512];
    const char *value = field_value(text, key, buffer, sizeof(buffer));
    if (value == NULL) {
        return (size_t)-1;
    }
    size_t len = strlen(value);
    if (len % 2 != 0 || len / 2 > out_len) {
        return (size_t)-1;
    }
    for (size_t i = 0; i < len / 2; i++) {
        unsigned byte = 0;
        if (sscanf(value + 2 * i, "%2x", &byte) != 1) {
            return (size_t)-1;
        }
        out[i] = (uint8_t)byte;
    }
    return len / 2;
}

static void hex_dump(const uint8_t *bytes, size_t len, char *out, size_t out_len)
{
    size_t written = 0;
    for (size_t i = 0; i < len && written + 3 < out_len; i++) {
        written += (size_t)snprintf(out + written, out_len - written, "%02X", bytes[i]);
    }
    out[written] = '\0';
}

/* Assertion one: parse `<name>.fields`, encode it, and require the bytes to
 * equal `<name>.hex`. Assertion two: decode `<name>.hex` and require the
 * recovered fields to equal `<name>.fields`. */
static void one_vector(const char *name)
{
    char hex_path[512];
    char fields_path[512];
    char relative[256];
    snprintf(relative, sizeof(relative), "fixtures/protocol/%s.hex", name);
    chorus_repo_path(hex_path, sizeof(hex_path), relative);
    snprintf(relative, sizeof(relative), "fixtures/protocol/%s.fields", name);
    chorus_repo_path(fields_path, sizeof(fields_path), relative);

    char *hex_text = read_text(hex_path);
    char *fields_text = read_text(fields_path);
    if (hex_text == NULL || fields_text == NULL) {
        chorus_check(0, "%s: both committed vector files are readable", name);
        return;
    }

    static uint8_t committed[65536];
    size_t committed_len = parse_hex(hex_text, committed, sizeof(committed));
    if (committed_len == (size_t)-1) {
        chorus_check(0, "%s.hex parses as whitespace-separated hex", name);
        return;
    }

    char type_buffer[256];
    const char *type_name = field_value(fields_text, "message_type", type_buffer,
                                        sizeof(type_buffer));
    chorus_check(type_name != NULL && strcmp(type_name, name) == 0,
                 "%s.fields declares message_type = %s", name,
                 (type_name == NULL) ? "<absent>" : type_name);
    if (type_name == NULL) {
        return;
    }

    static uint8_t produced[65536];
    size_t produced_len = 0;
    chorus_encode_status_t status = CHORUS_ENCODE_OK;
    int ok = 0;

    if (strcmp(name, "time_sync") == 0) {
        chorus_time_sync_t message;
        message.t0_ns = field_u64(fields_text, "t0_ns", &ok);
        message.t1_ns = field_u64(fields_text, "t1_ns", &ok);
        message.t2_ns = field_u64(fields_text, "t2_ns", &ok);
        message.t3_ns = field_u64(fields_text, "t3_ns", &ok);
        status = chorus_encode_time_sync(&message, produced, sizeof(produced), &produced_len);
    } else if (strcmp(name, "stream_end") == 0) {
        chorus_stream_end_t message;
        message.final_sequence = (uint32_t)field_u64(fields_text, "final_sequence", &ok);
        message.end_timestamp_ns = field_u64(fields_text, "end_timestamp_ns", &ok);
        status = chorus_encode_stream_end(&message, produced, sizeof(produced), &produced_len);
    } else if (strcmp(name, "audio_chunk") == 0) {
        chorus_audio_chunk_t message;
        memset(&message, 0, sizeof(message));
        message.sequence = (uint32_t)field_u64(fields_text, "sequence", &ok);
        message.timestamp_ns = field_u64(fields_text, "timestamp_ns", &ok);
        message.sample_rate_hz = (uint32_t)field_u64(fields_text, "sample_rate_hz", &ok);
        message.channels = (uint16_t)field_u64(fields_text, "channels", &ok);
        char format_buffer[64];
        const char *format_name = field_value(fields_text, "sample_format", format_buffer,
                                              sizeof(format_buffer));
        chorus_check(format_name != NULL, "%s.fields names a sample_format", name);
        if (format_name == NULL) {
            return;
        }
        message.sample_format = chorus_sample_format_from_name(format_name);
        chorus_check(message.sample_format != 0, "%s.fields sample_format %s is in the catalog",
                     name, format_name);
        size_t reserved_len = field_bytes(fields_text, "reserved", message.reserved,
                                          sizeof(message.reserved));
        chorus_check(reserved_len == CHORUS_RESERVED_LEN,
                     "%s.fields carries %zu reserved bytes and the header reserves %u", name,
                     reserved_len, CHORUS_RESERVED_LEN);
        static uint8_t audio[65536];
        size_t audio_len = field_bytes(fields_text, "audio_data", audio, sizeof(audio));
        chorus_check(audio_len != (size_t)-1, "%s.fields carries audio_data", name);
        if (audio_len == (size_t)-1) {
            return;
        }
        message.audio_data = audio;
        message.audio_data_len = audio_len;
        status = chorus_encode_audio_chunk(&message, produced, sizeof(produced), &produced_len);
    } else {
        chorus_check(0, "%s is a type this test knows how to encode", name);
        return;
    }

    chorus_check(status == CHORUS_ENCODE_OK, "%s encodes (%s)", name,
                 chorus_encode_status_name(status));
    if (status != CHORUS_ENCODE_OK) {
        return;
    }

    int identical = (produced_len == committed_len) &&
                    (memcmp(produced, committed, committed_len) == 0);
    if (identical) {
        chorus_check(1, "%s: the endpoint encodes the committed %zu bytes exactly", name,
                     committed_len);
    } else {
        char produced_hex[4096];
        char committed_hex[4096];
        hex_dump(produced, produced_len, produced_hex, sizeof(produced_hex));
        hex_dump(committed, committed_len, committed_hex, sizeof(committed_hex));
        chorus_check(0, "%s: produced %zu bytes %s against the committed %zu bytes %s", name,
                     produced_len, produced_hex, committed_len, committed_hex);
        return;
    }

    /* And back the other way. */
    chorus_frame_t frame = chorus_decode_frame(committed, committed_len);
    chorus_check(frame.outcome == CHORUS_FRAME_DECODED && frame.consumed == committed_len,
                 "%s: the committed bytes decode whole (%s, consumed %zu of %zu)", name,
                 chorus_frame_outcome_name(frame.outcome), frame.consumed, committed_len);
    if (frame.outcome != CHORUS_FRAME_DECODED) {
        return;
    }

    if (strcmp(name, "time_sync") == 0) {
        const chorus_time_sync_t *ts = &frame.message.time_sync;
        chorus_check(ts->t0_ns == field_u64(fields_text, "t0_ns", &ok) &&
                         ts->t1_ns == field_u64(fields_text, "t1_ns", &ok) &&
                         ts->t2_ns == field_u64(fields_text, "t2_ns", &ok) &&
                         ts->t3_ns == field_u64(fields_text, "t3_ns", &ok),
                     "%s: the four timestamps come back as the committed fields declare them",
                     name);
    } else if (strcmp(name, "stream_end") == 0) {
        const chorus_stream_end_t *end = &frame.message.stream_end;
        chorus_check(end->final_sequence == (uint32_t)field_u64(fields_text, "final_sequence",
                                                                &ok) &&
                         end->end_timestamp_ns == field_u64(fields_text, "end_timestamp_ns", &ok),
                     "%s: final_sequence and end_timestamp_ns come back as declared", name);
    } else {
        const chorus_audio_chunk_t *chunk = &frame.message.audio_chunk;
        char format_buffer[64];
        const char *format_name = field_value(fields_text, "sample_format", format_buffer,
                                              sizeof(format_buffer));
        uint8_t reserved[CHORUS_RESERVED_LEN];
        field_bytes(fields_text, "reserved", reserved, sizeof(reserved));
        static uint8_t audio[65536];
        size_t audio_len = field_bytes(fields_text, "audio_data", audio, sizeof(audio));
        chorus_check(chunk->sequence == (uint32_t)field_u64(fields_text, "sequence", &ok) &&
                         chunk->timestamp_ns == field_u64(fields_text, "timestamp_ns", &ok) &&
                         chunk->sample_rate_hz ==
                             (uint32_t)field_u64(fields_text, "sample_rate_hz", &ok) &&
                         chunk->channels == (uint16_t)field_u64(fields_text, "channels", &ok) &&
                         chunk->sample_format == chorus_sample_format_from_name(format_name) &&
                         memcmp(chunk->reserved, reserved, CHORUS_RESERVED_LEN) == 0 &&
                         chunk->audio_data_len == audio_len &&
                         memcmp(chunk->audio_data, audio, audio_len) == 0,
                     "%s: every field comes back as the committed fields declare it", name);
    }
}

/* AC-7, and rule 3 of docs/protocol.md's decoder behaviour. */
static void unknown_type_is_skipped_and_the_session_stays_open(void)
{
    /* An unassigned type carrying four bytes, followed by a whole committed
     * time_sync frame. A decoder that failed the session on the first would
     * never see the second. */
    uint8_t stream[3 + 4 + 3 + CHORUS_TIME_SYNC_PAYLOAD_LEN];
    memset(stream, 0, sizeof(stream));
    stream[0] = 0x7F;
    stream[1] = 0x00;
    stream[2] = 0x04;
    stream[3] = 0xDE;
    stream[4] = 0xAD;
    stream[5] = 0xBE;
    stream[6] = 0xEF;

    chorus_time_sync_t exchange = {1000000000ull, 1000500000ull, 1001000000ull, 1001600000ull};
    size_t written = 0;
    chorus_encode_status_t status =
        chorus_encode_time_sync(&exchange, stream + 7, sizeof(stream) - 7, &written);
    chorus_check(status == CHORUS_ENCODE_OK, "the follower frame encodes");

    chorus_frame_t first = chorus_decode_frame(stream, sizeof(stream));
    chorus_check(first.outcome == CHORUS_FRAME_SKIPPED_UNKNOWN_TYPE,
                 "an unassigned type byte 0x%02x is skipped, not rejected (%s)", 0x7F,
                 chorus_frame_outcome_name(first.outcome));
    chorus_check(first.consumed == 7,
                 "the skip steps over the whole frame using its length prefix: %zu bytes",
                 first.consumed);

    chorus_frame_t second = chorus_decode_frame(stream + first.consumed,
                                                sizeof(stream) - first.consumed);
    chorus_check(second.outcome == CHORUS_FRAME_DECODED &&
                     second.message.time_sync.t0_ns == exchange.t0_ns &&
                     second.message.time_sync.t3_ns == exchange.t3_ns,
                 "the frame after the skipped one decodes, so the session stayed open");

    /* A frame whose payload is longer than the fields this decoder knows about
     * is accepted and the excess ignored, which is how a decoder built today
     * survives a field added tomorrow. */
    uint8_t longer[3 + CHORUS_TIME_SYNC_PAYLOAD_LEN + 8];
    memset(longer, 0, sizeof(longer));
    longer[0] = CHORUS_MSG_TIME_SYNC;
    longer[1] = 0x00;
    longer[2] = (uint8_t)(CHORUS_TIME_SYNC_PAYLOAD_LEN + 8);
    chorus_frame_t extended = chorus_decode_frame(longer, sizeof(longer));
    chorus_check(extended.outcome == CHORUS_FRAME_DECODED &&
                     extended.consumed == sizeof(longer),
                 "a payload longer than the known fields is accepted and the excess ignored");
}

/* The order of the decoder's checks is part of the contract. */
static void the_decoder_checks_happen_in_the_committed_order(void)
{
    uint8_t two_bytes[2] = {CHORUS_MSG_TIME_SYNC, 0x00};
    chorus_frame_t truncated = chorus_decode_frame(two_bytes, sizeof(two_bytes));
    chorus_check(truncated.outcome == CHORUS_FRAME_TRUNCATED_HEADER && truncated.consumed == 0,
                 "fewer than three bytes is a truncated header and consumes nothing");

    uint8_t over_declared[5] = {CHORUS_MSG_TIME_SYNC, 0x00, 0x20, 0x01, 0x02};
    chorus_frame_t over = chorus_decode_frame(over_declared, sizeof(over_declared));
    chorus_check(over.outcome == CHORUS_FRAME_DECLARED_LENGTH_EXCEEDS_BUFFER &&
                     over.consumed == 0,
                 "a declared length longer than the buffer consumes nothing, so a stream reader "
                 "waits rather than guessing");

    /* An unknown type whose declared length also exceeds the buffer is check 2
     * and not check 3: the boundary is unknown, so nothing may be stepped
     * over. */
    uint8_t unknown_over[4] = {0x7F, 0x00, 0x20, 0x01};
    chorus_frame_t unknown = chorus_decode_frame(unknown_over, sizeof(unknown_over));
    chorus_check(unknown.outcome == CHORUS_FRAME_DECLARED_LENGTH_EXCEEDS_BUFFER,
                 "an unknown type with an unsatisfiable length is a length problem first");

    uint8_t short_payload[3 + 16];
    memset(short_payload, 0, sizeof(short_payload));
    short_payload[0] = CHORUS_MSG_TIME_SYNC;
    short_payload[2] = 16;
    chorus_frame_t too_short = chorus_decode_frame(short_payload, sizeof(short_payload));
    chorus_check(too_short.outcome == CHORUS_FRAME_PAYLOAD_TOO_SHORT_FOR_TYPE &&
                     too_short.consumed == sizeof(short_payload),
                 "a payload below the type's minimum costs one frame and the reader stays "
                 "aligned");

    /* A chunk that is the right size and still carries a value the format
     * cannot accept. */
    uint8_t bad_format[3 + 33];
    memset(bad_format, 0, sizeof(bad_format));
    bad_format[0] = CHORUS_MSG_AUDIO_CHUNK;
    bad_format[2] = 33;
    bad_format[3 + 12] = 0x00;
    bad_format[3 + 13] = 0x00;
    bad_format[3 + 14] = 0xBB;
    bad_format[3 + 15] = 0x80;
    bad_format[3 + 16] = 2;
    bad_format[3 + 17] = 9; /* not a defined sample format */
    chorus_frame_t bad = chorus_decode_frame(bad_format, sizeof(bad_format));
    chorus_check(bad.outcome == CHORUS_FRAME_INVALID_FIELD &&
                     bad.invalid_field == CHORUS_FIELD_UNDEFINED_SAMPLE_FORMAT &&
                     bad.consumed == sizeof(bad_format),
                 "an undefined sample format rejects one frame and steps over it");
}

/* An encoder refuses, and emits nothing at all, rather than truncating. */
static void the_encoder_refuses_rather_than_truncating(void)
{
    chorus_audio_chunk_t chunk;
    memset(&chunk, 0, sizeof(chunk));
    uint8_t pcm[4] = {1, 2, 3, 4};
    chunk.sequence = 1;
    chunk.timestamp_ns = 1;
    chunk.sample_rate_hz = 48000;
    chunk.channels = 300;
    chunk.sample_format = CHORUS_FMT_PCM_S16LE;
    chunk.audio_data = pcm;
    chunk.audio_data_len = sizeof(pcm);
    uint8_t out[128];
    size_t written = 12345;
    chorus_check(chorus_encode_audio_chunk(&chunk, out, sizeof(out), &written) ==
                         CHORUS_ENCODE_NOT_REPRESENTABLE &&
                     written == 12345,
                 "300 channels does not fit a u8 and nothing is written");

    chunk.channels = 2;
    chunk.audio_data_len = 3;
    chorus_check(chorus_encode_audio_chunk(&chunk, out, sizeof(out), &written) ==
                     CHORUS_ENCODE_INVALID_FIELD,
                 "three bytes of PCM is not a whole number of four-byte frames");

    chunk.audio_data_len = 4;
    chunk.sample_rate_hz = 4000;
    chorus_check(chorus_encode_audio_chunk(&chunk, out, sizeof(out), &written) ==
                     CHORUS_ENCODE_FIELD_OUT_OF_RANGE,
                 "4000 Hz is below the accepted band");

    chunk.sample_rate_hz = 48000;
    chorus_check(chorus_encode_audio_chunk(&chunk, out, 4, &written) ==
                     CHORUS_ENCODE_BUFFER_TOO_SMALL,
                 "a buffer too small for the frame gets nothing rather than part of one");
}

static void the_exchange_arithmetic_matches_the_document(void)
{
    /* docs/protocol.md gives rtt = (t3 - t0) - (t2 - t1) and
     * offset = ((t1 - t0) + (t2 - t3)) / 2 for the committed vector. */
    chorus_time_sync_t exchange = {1000000000ull, 1000500000ull, 1001000000ull, 1001600000ull};
    chorus_check(chorus_time_sync_rtt_ns(&exchange) == 1100000,
                 "the committed vector's round trip is %" PRIu64 " ns",
                 chorus_time_sync_rtt_ns(&exchange));
    chorus_check(chorus_time_sync_offset_ns(&exchange) == -50000,
                 "the committed vector's offset is %" PRId64 " ns",
                 chorus_time_sync_offset_ns(&exchange));

    /* Timestamps that cannot be a round trip yield zero rather than wrapping. */
    chorus_time_sync_t nonsense = {1000, 0, 0, 0};
    chorus_check(chorus_time_sync_rtt_ns(&nonsense) == 0,
                 "a nonsensical exchange yields a round trip of 0 rather than a huge one");
}

/* Read fixtures/protocol/ and hold this endpoint's catalog to what is there. */
static void the_committed_directory_is_the_catalog(committed_catalog_t *committed)
{
    char dir_path[512];
    chorus_repo_path(dir_path, sizeof(dir_path), "fixtures/protocol");
    read_committed_catalog(dir_path, committed);

    chorus_check(!committed->unreadable && !committed->overflowed && committed->count > 0,
                 "fixtures/protocol/ was read and holds %zu committed .hex vectors",
                 committed->count);
    for (size_t i = 0; i < committed->count; i++) {
        printf("     %s.hex\n", committed->vectors[i].stem);
    }

    catalog_drift_t drift = catalog_drift(committed);
    if (drift.unmirrored[0] == '\0') {
        chorus_check(1, "every committed vector pair names a type this endpoint mirrors");
    } else {
        chorus_check(0,
                     "fixtures/protocol/%s.hex is committed and this endpoint's catalog does "
                     "not carry it: the C implementation has fallen behind the vectors",
                     drift.unmirrored);
    }
    if (drift.without_vector[0] == '\0') {
        chorus_check(1, "every type this endpoint mirrors has a committed vector pair");
    } else {
        chorus_check(0,
                     "this endpoint mirrors %s and fixtures/protocol/%s.hex is not committed",
                     drift.without_vector, drift.without_vector);
    }
    chorus_check(committed->count == CATALOG_COUNT,
                 "the %zu committed vector pairs and the %zu types this endpoint mirrors are "
                 "the same set",
                 committed->count, CATALOG_COUNT);
}

/* --- the drift guard, shown going red -------------------------------------
 *
 * A guard that has only ever been green is a guard nobody has seen work, which
 * is exactly what the hardcoded three-element list this replaced was. Both
 * directions are demonstrated on a scratch copy of the fixture directory, in
 * the shape firmware/tests/test_scan.c already uses. The copies are made under
 * the system temporary directory and removed afterwards, so a demonstration
 * never touches fixtures/. */

static int copy_file(const char *from, const char *to)
{
    FILE *in = fopen(from, "rb");
    if (in == NULL) {
        return -1;
    }
    FILE *out = fopen(to, "wb");
    if (out == NULL) {
        fclose(in);
        return -1;
    }
    char buffer[8192];
    size_t got;
    int failed = 0;
    while ((got = fread(buffer, 1, sizeof(buffer), in)) > 0) {
        if (fwrite(buffer, 1, got, out) != got) {
            failed = 1;
            break;
        }
    }
    fclose(in);
    fclose(out);
    return failed ? -1 : 0;
}

static void remove_scratch(const char *path)
{
    DIR *dir = opendir(path);
    if (dir == NULL) {
        remove(path);
        return;
    }
    struct dirent *entry;
    while ((entry = readdir(dir)) != NULL) {
        if (strcmp(entry->d_name, ".") == 0 || strcmp(entry->d_name, "..") == 0) {
            continue;
        }
        char child[1024];
        snprintf(child, sizeof(child), "%.700s/%.255s", path, entry->d_name);
        remove(child);
    }
    closedir(dir);
    rmdir(path);
}

static int scratch_fixture_dir(const char *what, char *out, size_t out_len)
{
    snprintf(out, out_len, "/tmp/chorus-protocol-catalog-%s-%d", what, (int)getpid());
    remove_scratch(out);
    if (mkdir(out, 0755) != 0 && errno != EEXIST) {
        return -1;
    }
    return 0;
}

static int copy_committed_pair(const char *stem, const char *into)
{
    static const char *const extensions[] = {"hex", "fields"};
    for (size_t i = 0; i < sizeof(extensions) / sizeof(extensions[0]); i++) {
        char relative[256];
        char from[512];
        char to[1024];
        snprintf(relative, sizeof(relative), "fixtures/protocol/%s.%s", stem, extensions[i]);
        chorus_repo_path(from, sizeof(from), relative);
        snprintf(to, sizeof(to), "%.700s/%.100s.%.16s", into, stem, extensions[i]);
        if (copy_file(from, to) != 0) {
            return -1;
        }
    }
    return 0;
}

/* A vector pair for a type this endpoint does not mirror. The bytes are never
 * decoded by anything: what is under test is whether the guard SEES the pair. */
static int smuggle_fourth_pair(const char *into, const char *stem)
{
    char path[1024];
    snprintf(path, sizeof(path), "%.700s/%.100s.hex", into, stem);
    FILE *file = fopen(path, "wb");
    if (file == NULL) {
        return -1;
    }
    fputs("# a hypothetical fourth catalogued type, committed as a golden vector\n"
          "04 00 08\n00 00 00 00 00 00 00 2a\n",
          file);
    fclose(file);
    snprintf(path, sizeof(path), "%.700s/%.100s.fields", into, stem);
    file = fopen(path, "wb");
    if (file == NULL) {
        return -1;
    }
    fprintf(file, "message_type = %s\nuptime_ns = 42\n", stem);
    fclose(file);
    return 0;
}

static void a_committed_vector_this_endpoint_does_not_mirror(void)
{
    char scratch[512];
    if (scratch_fixture_dir("fourth-type", scratch, sizeof(scratch)) != 0) {
        chorus_check(0, "a scratch fixture directory for the fourth-type demonstration is "
                        "makeable");
        return;
    }
    int copied = 0;
    for (size_t i = 0; i < CATALOG_COUNT; i++) {
        copied |= copy_committed_pair(CATALOG[i], scratch);
    }
    if (copied != 0 || smuggle_fourth_pair(scratch, "heartbeat") != 0) {
        chorus_check(0, "the committed pairs plus a smuggled fourth are writable in the "
                        "scratch copy");
        remove_scratch(scratch);
        return;
    }

    committed_catalog_t smuggled;
    read_committed_catalog(scratch, &smuggled);
    catalog_drift_t drift = catalog_drift(&smuggled);
    chorus_check(smuggled.count == CATALOG_COUNT + 1,
                 "the scratch directory holds %zu vector pairs, one more than this endpoint "
                 "mirrors",
                 smuggled.count);
    chorus_check(strcmp(drift.unmirrored, "heartbeat") == 0,
                 "a fourth committed vector pair turns the guard RED naming the type: '%s'",
                 drift.unmirrored);
    chorus_check(drift.without_vector[0] == '\0',
                 "and it says nothing about the three types that are still mirrored");
    remove_scratch(scratch);
}

static void a_mirrored_type_whose_vector_went_away(void)
{
    char scratch[512];
    if (scratch_fixture_dir("missing-vector", scratch, sizeof(scratch)) != 0) {
        chorus_check(0, "a scratch fixture directory for the missing-vector demonstration is "
                        "makeable");
        return;
    }
    const char *dropped = CATALOG[CATALOG_COUNT - 1];
    int copied = 0;
    for (size_t i = 0; i + 1 < CATALOG_COUNT; i++) {
        copied |= copy_committed_pair(CATALOG[i], scratch);
    }
    if (copied != 0) {
        chorus_check(0, "the remaining committed pairs are writable in the scratch copy");
        remove_scratch(scratch);
        return;
    }

    committed_catalog_t smuggled;
    read_committed_catalog(scratch, &smuggled);
    catalog_drift_t drift = catalog_drift(&smuggled);
    chorus_check(strcmp(drift.without_vector, dropped) == 0,
                 "a committed vector pair removed under a type this endpoint mirrors turns the "
                 "guard RED naming it: '%s'",
                 drift.without_vector);
    chorus_check(drift.unmirrored[0] == '\0',
                 "and it does not also accuse the pairs that are still there");
    remove_scratch(scratch);
}

/* Or every demonstration above would go red for the wrong reason. */
static void an_unaltered_scratch_copy_is_green(void)
{
    char scratch[512];
    if (scratch_fixture_dir("unaltered", scratch, sizeof(scratch)) != 0) {
        chorus_check(0, "a scratch fixture directory for the control is makeable");
        return;
    }
    int copied = 0;
    for (size_t i = 0; i < CATALOG_COUNT; i++) {
        copied |= copy_committed_pair(CATALOG[i], scratch);
    }
    if (copied != 0) {
        chorus_check(0, "the committed pairs are writable in the scratch copy");
        remove_scratch(scratch);
        return;
    }

    committed_catalog_t control;
    read_committed_catalog(scratch, &control);
    catalog_drift_t drift = catalog_drift(&control);
    chorus_check(control.count == CATALOG_COUNT && drift.unmirrored[0] == '\0' &&
                     drift.without_vector[0] == '\0',
                 "an unaltered copy of the committed pairs is green, so the two above go red "
                 "for the reason claimed");
    remove_scratch(scratch);
}

int main(void)
{
    chorus_section("fixtures/protocol/ is read, and this endpoint's catalog is held to it");
    committed_catalog_t committed;
    the_committed_directory_is_the_catalog(&committed);

    chorus_section("the drift guard, shown going red on a smuggled fixture directory");
    a_committed_vector_this_endpoint_does_not_mirror();
    a_mirrored_type_whose_vector_went_away();
    an_unaltered_scratch_copy_is_green();

    chorus_section("every committed vector round trips, both directions");
    for (size_t i = 0; i < committed.count; i++) {
        one_vector(committed.vectors[i].stem);
    }

    chorus_section("an unrecognised type is skipped and the session stays open");
    unknown_type_is_skipped_and_the_session_stays_open();

    chorus_section("the decoder's checks happen in the order docs/protocol.md fixes");
    the_decoder_checks_happen_in_the_committed_order();

    chorus_section("the encoder refuses rather than truncating");
    the_encoder_refuses_rather_than_truncating();

    chorus_section("the exchange arithmetic");
    the_exchange_arithmetic_matches_the_document();

    return chorus_test_report("endpoint protocol");
}
