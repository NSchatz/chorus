/* The endpoint's protocol v2 catalog, held to the committed golden vectors.
 *
 * docs/protocol.md "Golden vectors": fixtures/protocol/v2/ holds at least one
 * vector for every type v2 added. This suite WALKS that directory, and for
 * every `.hex` in it runs the two assertions fixtures/README.md asks of a
 * second implementation: encoding the `.fields` produces the `.hex` byte for
 * byte, and decoding the `.hex` recovers the `.fields`. It prints how many of
 * how many passed, where the second number is every `.hex` found.
 *
 * "Recovers the fields" is checked by encoding what was decoded and comparing
 * it with what the `.fields` file encodes to: every field of every v2 message
 * is written into its frame, so two messages that encode to the same bytes
 * hold the same fields (the encoding is injective), and the comparison needs
 * no second, hand-written equality that could disagree with the encoder.
 *
 * Then the robustness the Rust side asserts in crates/protocol/tests/
 * v2_rules.rs: the stated value rules refused by the encoder AND rejected by
 * the decoder as the same field, every single-byte corruption of every vector
 * handled (a frame that still decodes re-encodes and decodes the same), and
 * every prefix of a stream of all the vectors safe. The vectors are READ and
 * never written. */

#include "chorus/protocol_v2.h"
#include "chorus/session.h"
#include "fixture_text.h"
#include "harness.h"

#include <dirent.h>
#include <stdlib.h>

#define MAX_VECTORS 64
#define TEXT_CAP 16384
#define FRAME_CAP 70000

typedef struct {
    char stem[128];
    uint8_t frame[4096];
    size_t frame_len;
} vector_t;

static vector_t vectors[MAX_VECTORS];
static size_t vector_count;

static int compare_stems(const void *a, const void *b)
{
    return strcmp(((const vector_t *)a)->stem, ((const vector_t *)b)->stem);
}

/* --- building a message from a .fields file ------------------------------- */

/* Byte strings and texts point into this pool, which each vector resets. */
static uint8_t pool[FRAME_CAP];
static size_t pool_used;

typedef struct {
    const char *text;
    const char *stem;
    int ok;
} fields_t;

static const char *value(fields_t *f, const char *field_name)
{
    static char buffers[8][4096];
    static int next;
    char *out = buffers[next++ % 8];
    if (fixture_field(f->text, field_name, out, 4096) == NULL) {
        chorus_check(0, "%s.fields has %s", f->stem, field_name);
        f->ok = 0;
        out[0] = '\0';
    }
    return out;
}

static long long number(fields_t *f, const char *field_name)
{
    const char *text = value(f, field_name);
    char *end = NULL;
    long long n = strtoll(text, &end, 10);
    if (end == text || *end != '\0') {
        /* Values above INT64_MAX do not occur in these files; a u64 that did
         * would be read by strtoull below. */
        chorus_check(0, "%s.fields %s = '%s' is a number", f->stem, field_name, text);
        f->ok = 0;
    }
    return n;
}

static unsigned long long unsigned_number(fields_t *f, const char *field_name)
{
    const char *text = value(f, field_name);
    char *end = NULL;
    unsigned long long n = strtoull(text, &end, 10);
    if (end == text || *end != '\0') {
        chorus_check(0, "%s.fields %s = '%s' is a number", f->stem, field_name, text);
        f->ok = 0;
    }
    return n;
}

static chorus_v2_bytes_t text(fields_t *f, const char *field_name)
{
    const char *t = value(f, field_name);
    size_t len = strlen(t);
    chorus_v2_bytes_t out = {pool + pool_used, len};
    memcpy(pool + pool_used, t, len);
    pool_used += len;
    return out;
}

static chorus_v2_bytes_t hex_bytes(fields_t *f, const char *field_name)
{
    const char *t = value(f, field_name);
    long len = fixture_unhex(t, pool + pool_used, sizeof(pool) - pool_used);
    if (len < 0) {
        chorus_check(0, "%s.fields %s is unseparated hex", f->stem, field_name);
        f->ok = 0;
        len = 0;
    }
    chorus_v2_bytes_t out = {pool + pool_used, (size_t)len};
    pool_used += (size_t)len;
    return out;
}

static uint8_t named(fields_t *f, const char *field_name, chorus_v2_enum_t which)
{
    const char *t = value(f, field_name);
    uint8_t out = 0;
    if (chorus_v2_enum_from_name(which, t, &out) != 0) {
        chorus_check(0, "%s.fields %s = '%s' is a defined name", f->stem, field_name, t);
        f->ok = 0;
    }
    return out;
}

/* Space separated words, each handed to `each`. Returns the count. */
static size_t words(const char *list, char out[][64], size_t cap)
{
    size_t n = 0;
    const char *p = list;
    while (*p != '\0' && n < cap) {
        while (*p == ' ') {
            p++;
        }
        if (*p == '\0') {
            break;
        }
        size_t len = strcspn(p, " ");
        if (len >= 64) {
            len = 63;
        }
        memcpy(out[n], p, len);
        out[n][len] = '\0';
        n++;
        p += strcspn(p, " ");
    }
    return n;
}

static uint8_t sample_format_bits(fields_t *f, const char *field_name)
{
    char list[8][64];
    size_t n = words(value(f, field_name), list, 8);
    uint8_t bits = 0;
    for (size_t i = 0; i < n; i++) {
        uint8_t wire = chorus_sample_format_from_name(list[i]);
        if (wire == 0) {
            chorus_check(0, "%s.fields %s names '%s'", f->stem, field_name, list[i]);
            f->ok = 0;
            continue;
        }
        bits |= (uint8_t)(1u << (wire - 1));
    }
    return bits;
}

static int build(const char *stem, const char *fields_text, chorus_v2_message_t *m)
{
    fields_t f = {fields_text, stem, 1};
    pool_used = 0;
    memset(m, 0, sizeof(*m));
    const char *type_name = value(&f, "message_type");
    m->type = chorus_v2_type_from_name(type_name);
    char list[64][64];
    switch (m->type) {
    case CHORUS_V2_HELLO: {
        m->as.hello.protocol_version = (uint16_t)number(&f, "protocol_version");
        size_t n = words(value(&f, "roles"), list, 16);
        for (size_t i = 0; i < n; i++) {
            uint8_t bit = 0;
            if (chorus_v2_enum_from_name(CHORUS_V2_ENUM_ROLE_BIT, list[i], &bit) != 0) {
                f.ok = 0;
            }
            m->as.hello.roles |= (uint16_t)(1u << bit);
        }
        m->as.hello.name = text(&f, "name");
        m->as.hello.software = text(&f, "software");
        break;
    }
    case CHORUS_V2_CAPABILITIES: {
        chorus_v2_capabilities_t *c = &m->as.capabilities;
        size_t n = words(value(&f, "codecs"), list, 8);
        for (size_t i = 0; i < n; i++) {
            uint8_t codec = 0;
            if (chorus_v2_enum_from_name(CHORUS_V2_ENUM_CODEC, list[i], &codec) != 0) {
                f.ok = 0;
            }
            c->codecs |= CHORUS_V2_CODEC_BIT(codec);
        }
        c->sample_formats = sample_format_bits(&f, "sample_formats");
        c->max_channels = (uint8_t)number(&f, "max_channels");
        n = words(value(&f, "sample_rates_hz"), list, CHORUS_V2_MAX_RATES);
        c->rate_count = n;
        for (size_t i = 0; i < n; i++) {
            c->sample_rates_hz[i] = (uint32_t)strtoul(list[i], NULL, 10);
        }
        c->buffer_ms = (uint16_t)number(&f, "buffer_ms");
        c->intrinsic_latency_ns = (uint32_t)number(&f, "intrinsic_latency_ns");
        c->led_count = (uint16_t)number(&f, "led_count");
        c->visualizer_bands = (uint8_t)number(&f, "visualizer_bands");
        /* Optional (goal 13): a vector made before the field has none. */
        char feature_list[256];
        if (fixture_field(fields_text, "features", feature_list, sizeof(feature_list)) != NULL) {
            n = words(feature_list, list, 8);
            for (size_t i = 0; i < n; i++) {
                uint8_t bit = 0;
                if (chorus_v2_enum_from_name(CHORUS_V2_ENUM_FEATURE_BIT, list[i], &bit) != 0) {
                    f.ok = 0;
                }
                c->features |= (uint8_t)(1u << bit);
            }
        }
        break;
    }
    case CHORUS_V2_STREAM_FORMAT: {
        chorus_v2_stream_format_t *s = &m->as.stream_format;
        s->codec = named(&f, "codec", CHORUS_V2_ENUM_CODEC);
        s->sample_format = chorus_sample_format_from_name(value(&f, "sample_format"));
        s->sample_rate_hz = (uint32_t)number(&f, "sample_rate_hz");
        size_t n = words(value(&f, "channel_map"), list, CHORUS_MAX_CHANNELS);
        s->channels = n;
        for (size_t i = 0; i < n; i++) {
            if (chorus_v2_enum_from_name(CHORUS_V2_ENUM_CHANNEL_POSITION, list[i],
                                         &s->channel_map[i]) != 0) {
                f.ok = 0;
            }
        }
        s->frames_per_chunk = (uint32_t)number(&f, "frames_per_chunk");
        s->codec_config = hex_bytes(&f, "codec_config");
        break;
    }
    case CHORUS_V2_CODED_CHUNK:
        m->as.coded_chunk.sequence = (uint32_t)number(&f, "sequence");
        m->as.coded_chunk.timestamp_ns = unsigned_number(&f, "timestamp_ns");
        m->as.coded_chunk.frames = (uint32_t)number(&f, "frames");
        m->as.coded_chunk.data = hex_bytes(&f, "data");
        break;
    case CHORUS_V2_OUTPUT_DELAY:
        m->as.output_delay.delay_ns = unsigned_number(&f, "delay_ns");
        break;
    case CHORUS_V2_TELEMETRY: {
        chorus_v2_telemetry_t *t = &m->as.telemetry;
        t->taken_ns = unsigned_number(&f, "taken_ns");
        t->sync_error_ns = (int64_t)number(&f, "sync_error_ns");
        t->buffer_fill_us = (uint32_t)number(&f, "buffer_fill_us");
        t->underruns = (uint32_t)number(&f, "underruns");
        t->resyncs = (uint32_t)number(&f, "resyncs");
        t->correction_ppb = (int32_t)number(&f, "correction_ppb");
        t->link = named(&f, "link", CHORUS_V2_ENUM_LINK);
        t->rssi_dbm = (int8_t)number(&f, "rssi_dbm");
        t->temperature_centi_c = (int16_t)number(&f, "temperature_centi_c");
        /* Optional (goal 15): a vector made before the heap block has
         * neither, which is how "not reported" is written. */
        char heap[32];
        t->heap_free_bytes = CHORUS_V2_TELEMETRY_HEAP_UNKNOWN;
        t->heap_min_free_bytes = CHORUS_V2_TELEMETRY_HEAP_UNKNOWN;
        if (fixture_field(fields_text, "heap_free_bytes", heap, sizeof(heap)) != NULL) {
            t->heap_free_bytes = (uint32_t)strtoul(heap, NULL, 10);
        }
        if (fixture_field(fields_text, "heap_min_free_bytes", heap, sizeof(heap)) != NULL) {
            t->heap_min_free_bytes = (uint32_t)strtoul(heap, NULL, 10);
        }
        break;
    }
    case CHORUS_V2_LOW_LATENCY_OFFER: {
        chorus_v2_low_latency_offer_t *o = &m->as.low_latency_offer;
        o->direction = named(&f, "direction", CHORUS_V2_ENUM_LOW_LATENCY_DIRECTION);
        o->stream_tag = (uint32_t)number(&f, "stream_tag");
        chorus_v2_bytes_t key = hex_bytes(&f, "key");
        if (key.len != CHORUS_V2_KEY_LEN) {
            chorus_check(0, "%s.fields key is 32 bytes", stem);
            f.ok = 0;
        } else {
            memcpy(o->key, key.data, CHORUS_V2_KEY_LEN);
        }
        o->udp_port = (uint16_t)number(&f, "udp_port");
        o->chunk_frames = (uint32_t)number(&f, "chunk_frames");
        /* Read wide and narrowed on purpose: a rejection vector's 17 has to
         * reach the encoder as 17. */
        o->fec_k = (uint8_t)number(&f, "fec_k");
        o->fec_depth = (uint8_t)number(&f, "fec_depth");
        o->latency_ns = unsigned_number(&f, "latency_ns");
        break;
    }
    case CHORUS_V2_LOW_LATENCY_ACCEPT:
        m->as.low_latency_accept.stream_tag = (uint32_t)number(&f, "stream_tag");
        m->as.low_latency_accept.status = named(&f, "status", CHORUS_V2_ENUM_LOW_LATENCY_STATUS);
        m->as.low_latency_accept.udp_port = (uint16_t)number(&f, "udp_port");
        break;
    case CHORUS_V2_HANDSHAKE_INIT:
        m->as.handshake_init.protocol_version = (uint16_t)number(&f, "protocol_version");
        m->as.handshake_init.suite = named(&f, "suite", CHORUS_V2_ENUM_SUITE);
        m->as.handshake_init.noise = hex_bytes(&f, "noise");
        break;
    case CHORUS_V2_HANDSHAKE_RESPONSE:
        m->as.handshake_response.noise = hex_bytes(&f, "noise");
        break;
    case CHORUS_V2_HANDSHAKE_FINISH:
        m->as.handshake_finish.noise = hex_bytes(&f, "noise");
        break;
    case CHORUS_V2_SESSION_REFUSED:
        m->as.session_refused.reason = named(&f, "reason", CHORUS_V2_ENUM_REFUSAL_REASON);
        m->as.session_refused.detail = text(&f, "detail");
        break;
    case CHORUS_V2_SECURE_RECORD:
        m->as.secure_record.ciphertext = hex_bytes(&f, "ciphertext");
        break;
    case CHORUS_V2_METADATA: {
        chorus_v2_metadata_t *d = &m->as.metadata;
        d->playback = named(&f, "playback", CHORUS_V2_ENUM_PLAYBACK);
        d->position_ms = (uint32_t)number(&f, "position_ms");
        d->duration_ms = (uint32_t)number(&f, "duration_ms");
        d->position_at_ns = unsigned_number(&f, "position_at_ns");
        d->artwork_id = (uint32_t)number(&f, "artwork_id");
        d->title = text(&f, "title");
        d->artist = text(&f, "artist");
        d->album = text(&f, "album");
        d->source = text(&f, "source");
        break;
    }
    case CHORUS_V2_ARTWORK:
        m->as.artwork.artwork_id = (uint32_t)number(&f, "artwork_id");
        m->as.artwork.total_len = (uint32_t)number(&f, "total_len");
        m->as.artwork.offset = (uint32_t)number(&f, "offset");
        m->as.artwork.mime = text(&f, "mime");
        m->as.artwork.data = hex_bytes(&f, "data");
        break;
    case CHORUS_V2_CONTROLLER_COMMAND:
        m->as.controller_command.command = named(&f, "command", CHORUS_V2_ENUM_COMMAND);
        m->as.controller_command.value = (int16_t)number(&f, "value");
        m->as.controller_command.target = text(&f, "target");
        break;
    case CHORUS_V2_CONTROLLER_STATE:
        m->as.controller_state.volume = (uint8_t)number(&f, "volume");
        m->as.controller_state.muted = (uint8_t)number(&f, "muted");
        m->as.controller_state.playback = named(&f, "playback", CHORUS_V2_ENUM_PLAYBACK);
        m->as.controller_state.group = text(&f, "group");
        break;
    case CHORUS_V2_VISUALIZER_FRAME: {
        chorus_v2_visualizer_frame_t *v = &m->as.visualizer_frame;
        v->timestamp_ns = unsigned_number(&f, "timestamp_ns");
        v->beat = (uint8_t)number(&f, "beat");
        v->peak = (uint8_t)number(&f, "peak");
        size_t n = words(value(&f, "bands"), list, 64);
        for (size_t i = 0; i < n; i++) {
            pool[pool_used + i] = (uint8_t)strtoul(list[i], NULL, 10);
        }
        v->bands.data = pool + pool_used;
        v->bands.len = n;
        pool_used += n;
        break;
    }
    case CHORUS_V2_COLOR:
        m->as.color.timestamp_ns = unsigned_number(&f, "timestamp_ns");
        m->as.color.red = (uint8_t)number(&f, "red");
        m->as.color.green = (uint8_t)number(&f, "green");
        m->as.color.blue = (uint8_t)number(&f, "blue");
        m->as.color.brightness = (uint8_t)number(&f, "brightness");
        m->as.color.transition_ms = (uint16_t)number(&f, "transition_ms");
        break;
    case CHORUS_V2_SOURCE_OFFER:
        m->as.source_offer.source_id = (uint8_t)number(&f, "source_id");
        m->as.source_offer.kind = named(&f, "kind", CHORUS_V2_ENUM_SOURCE_KIND);
        m->as.source_offer.signal = (uint8_t)number(&f, "signal");
        m->as.source_offer.name = text(&f, "name");
        {
            char reason[16];
            if (fixture_field(fields_text, "reason", reason, sizeof(reason)) != NULL) {
                m->as.source_offer.reason = (uint8_t)strtoul(reason, NULL, 10);
            }
        }
        break;
    case CHORUS_V2_SOURCE_CONTROL:
        m->as.source_control.source_id = (uint8_t)number(&f, "source_id");
        m->as.source_control.action = named(&f, "action", CHORUS_V2_ENUM_SOURCE_ACTION);
        m->as.source_control.codec = named(&f, "codec", CHORUS_V2_ENUM_CODEC);
        break;
    case CHORUS_V2_FIRMWARE_OFFER: {
        chorus_v2_firmware_offer_t *o = &m->as.firmware_offer;
        o->transfer = (uint32_t)unsigned_number(&f, "transfer");
        o->size = (uint32_t)unsigned_number(&f, "size");
        chorus_v2_bytes_t digest = hex_bytes(&f, "sha256");
        if (digest.len != CHORUS_V2_SHA256_LEN) {
            chorus_check(0, "%s.fields sha256 is 32 bytes", stem);
            f.ok = 0;
        } else {
            memcpy(o->sha256, digest.data, CHORUS_V2_SHA256_LEN);
        }
        o->chunk_bytes = (uint16_t)number(&f, "chunk_bytes");
        o->version = text(&f, "version");
        o->board = text(&f, "board");
        break;
    }
    case CHORUS_V2_FIRMWARE_CHUNK:
        m->as.firmware_chunk.transfer = (uint32_t)unsigned_number(&f, "transfer");
        m->as.firmware_chunk.offset = (uint32_t)unsigned_number(&f, "offset");
        m->as.firmware_chunk.data = hex_bytes(&f, "data");
        break;
    case CHORUS_V2_FIRMWARE_STATUS: {
        chorus_v2_firmware_status_t *s = &m->as.firmware_status;
        s->transfer = (uint32_t)unsigned_number(&f, "transfer");
        s->state = named(&f, "state", CHORUS_V2_ENUM_FIRMWARE_STATE);
        s->reason = named(&f, "reason", CHORUS_V2_ENUM_FIRMWARE_REASON);
        s->received = (uint32_t)unsigned_number(&f, "received");
        s->version = text(&f, "version");
        s->board = text(&f, "board");
        s->slot = (uint8_t)number(&f, "slot");
        s->image_version = text(&f, "image_version");
        break;
    }
    case CHORUS_V2_ROOM_VOLUME:
        /* Read wide and narrowed on purpose: a rejection vector's 1001 has to
         * reach the encoder as 1001, and every value here fits a u16. */
        m->as.room_volume.gain = (uint16_t)number(&f, "gain");
        m->as.room_volume.limit = (uint16_t)number(&f, "limit");
        m->as.room_volume.ramp_ms = (uint16_t)number(&f, "ramp_ms");
        break;
    case CHORUS_V2_SOUND: {
        chorus_v2_sound_t *s = &m->as.sound;
        s->bass_db = (int8_t)number(&f, "bass_db");
        s->treble_db = (int8_t)number(&f, "treble_db");
        size_t n = words(value(&f, "flags"), list, 8);
        static const char *const FLAG_NAMES[] = {"loudness", "night", "speech", "room_eq",
                                                 "sub_inverted"};
        for (size_t i = 0; i < n; i++) {
            int known = 0;
            for (size_t b = 0; b < sizeof(FLAG_NAMES) / sizeof(FLAG_NAMES[0]); b++) {
                if (strcmp(list[i], FLAG_NAMES[b]) == 0) {
                    s->flags |= (uint8_t)(1u << b);
                    known = 1;
                }
            }
            chorus_check(known, "%s.fields flags names '%s'", stem, list[i]);
            f.ok &= known;
        }
        char extra[32];
        if (fixture_field(fields_text, "flags_reserved", extra, sizeof(extra)) != NULL) {
            s->flags |= (uint8_t)strtoul(extra, NULL, 10);
        }
        s->role = (uint8_t)number(&f, "role");
        s->sub_present = (uint8_t)number(&f, "sub_present");
        s->crossover_hz = (uint16_t)number(&f, "crossover_hz");
        s->sub_level_cdb = (int16_t)number(&f, "sub_level_cdb");
        /* filter.<n> = freq_hz gain_cdb q_milli, numbered from 0; a
         * rejection vector's ninth is counted and not stored (the encoder
         * refuses the count before it would read one). */
        for (unsigned i = 0;; i++) {
            char key[32];
            char triple[128];
            snprintf(key, sizeof(key), "filter.%u", i);
            if (fixture_field(fields_text, key, triple, sizeof(triple)) == NULL) {
                break;
            }
            if (i < CHORUS_V2_SOUND_EQ_MAX_FILTERS) {
                char *end = NULL;
                s->filters[i].freq_hz = (uint16_t)strtol(triple, &end, 10);
                s->filters[i].gain_cdb = (int16_t)strtol(end, &end, 10);
                s->filters[i].q_milli = (uint16_t)strtol(end, &end, 10);
            }
            s->eq_count = (uint8_t)(i + 1);
        }
        /* Goal 13's theater block: absent keys are 0. */
        if (fixture_field(fields_text, "tv_upmix", extra, sizeof(extra)) != NULL) {
            s->tv_upmix = (uint8_t)strtoul(extra, NULL, 10);
        }
        if (fixture_field(fields_text, "fold", extra, sizeof(extra)) != NULL) {
            size_t nf = words(extra, list, 8);
            static const char *const FOLD_NAMES[] = {"centre", "surround"};
            for (size_t i = 0; i < nf; i++) {
                int known = 0;
                for (size_t b = 0; b < sizeof(FOLD_NAMES) / sizeof(FOLD_NAMES[0]); b++) {
                    if (strcmp(list[i], FOLD_NAMES[b]) == 0) {
                        s->fold |= (uint8_t)(1u << b);
                        known = 1;
                    }
                }
                chorus_check(known, "%s.fields fold names '%s'", stem, list[i]);
                f.ok &= known;
            }
        }
        if (fixture_field(fields_text, "fold_reserved", extra, sizeof(extra)) != NULL) {
            s->fold |= (uint8_t)strtoul(extra, NULL, 10);
        }
        break;
    }
    case CHORUS_V2_MIC_AUDIO: {
        chorus_v2_mic_audio_t *a = &m->as.mic_audio;
        /* A name for a defined layout; a rejection vector's undefined one is
         * its number. */
        const char *format = value(&f, "format");
        if (chorus_v2_enum_from_name(CHORUS_V2_ENUM_MIC_FORMAT, format, &a->format) != 0) {
            a->format = (uint8_t)number(&f, "format");
        }
        a->sequence = (uint32_t)unsigned_number(&f, "sequence");
        a->timestamp_ns = unsigned_number(&f, "timestamp_ns");
        a->data = hex_bytes(&f, "data");
        break;
    }
    case CHORUS_V2_MIC_STATE:
        m->as.mic_state.gate = named(&f, "gate", CHORUS_V2_ENUM_MIC_GATE);
        break;
    case CHORUS_V2_VOICE_CONTROL:
        m->as.voice_control.uplink = (uint8_t)number(&f, "uplink");
        m->as.voice_control.listening = (uint8_t)number(&f, "listening");
        break;
    default:
        chorus_check(0, "%s.fields message_type = %s is a v2 type this test builds", stem,
                     type_name);
        return 0;
    }
    return f.ok;
}

/* Two messages hold the same fields exactly when they encode to the same
 * bytes (see the top of this file). */
static int same_fields(const chorus_v2_message_t *a, const chorus_v2_message_t *b)
{
    static uint8_t ea[FRAME_CAP];
    static uint8_t eb[FRAME_CAP];
    size_t la = 0;
    size_t lb = 0;
    if (a->type != b->type || chorus_v2_encode(a, ea, sizeof(ea), &la, NULL) != CHORUS_ENCODE_OK ||
        chorus_v2_encode(b, eb, sizeof(eb), &lb, NULL) != CHORUS_ENCODE_OK) {
        return 0;
    }
    return la == lb && memcmp(ea, eb, la) == 0;
}

/* --- the committed directory ---------------------------------------------- */

static int the_committed_vectors_round_trip(void)
{
    chorus_section("fixtures/protocol/v2: every committed vector, both assertions");
    char dir_path[512];
    chorus_repo_path(dir_path, sizeof(dir_path), "fixtures/protocol/v2");
    DIR *dir = opendir(dir_path);
    chorus_check(dir != NULL, "fixtures/protocol/v2/ is readable");
    if (dir == NULL) {
        return 0;
    }
    struct dirent *entry;
    while ((entry = readdir(dir)) != NULL) {
        const char *dot = strrchr(entry->d_name, '.');
        if (dot == NULL || strcmp(dot, ".hex") != 0) {
            continue;
        }
        size_t stem_len = (size_t)(dot - entry->d_name);
        if (vector_count >= MAX_VECTORS || stem_len >= sizeof(vectors[0].stem)) {
            chorus_check(0, "more vectors than this suite has room for");
            break;
        }
        memcpy(vectors[vector_count].stem, entry->d_name, stem_len);
        vectors[vector_count].stem[stem_len] = '\0';
        vector_count++;
    }
    closedir(dir);
    qsort(vectors, vector_count, sizeof(vectors[0]), compare_stems);

    static char hex_text[TEXT_CAP];
    static char fields_text[TEXT_CAP];
    int passed = 0;
    for (size_t i = 0; i < vector_count; i++) {
        vector_t *v = &vectors[i];
        char path[512];
        char relative[256];
        snprintf(relative, sizeof(relative), "fixtures/protocol/v2/%.127s.hex", v->stem);
        chorus_repo_path(path, sizeof(path), relative);
        long hex_ok = fixture_read(path, hex_text, sizeof(hex_text));
        snprintf(relative, sizeof(relative), "fixtures/protocol/v2/%.127s.fields", v->stem);
        chorus_repo_path(path, sizeof(path), relative);
        long fields_ok = fixture_read(path, fields_text, sizeof(fields_text));
        long len = (hex_ok < 0) ? -1 : fixture_parse_hex(hex_text, v->frame, sizeof(v->frame));
        if (len < 0 || fields_ok < 0) {
            chorus_check(0, "%s: both files are readable and the .hex parses", v->stem);
            continue;
        }
        v->frame_len = (size_t)len;

        chorus_v2_message_t built;
        if (!build(v->stem, fields_text, &built)) {
            continue;
        }
        /* Assertion one: the fields encode to the committed bytes. */
        static uint8_t produced[FRAME_CAP];
        size_t produced_len = 0;
        chorus_v2_field_error_t error = {NULL, CHORUS_V2_PROBLEM_NONE, NULL};
        chorus_encode_status_t status =
            chorus_v2_encode(&built, produced, sizeof(produced), &produced_len, &error);
        int encoded = status == CHORUS_ENCODE_OK && produced_len == v->frame_len &&
                      memcmp(produced, v->frame, v->frame_len) == 0;
        if (!encoded) {
            char a[2048];
            char b[2048];
            fixture_hex(produced, produced_len, a, sizeof(a));
            fixture_hex(v->frame, v->frame_len, b, sizeof(b));
            chorus_check(0, "%s: encode(fields) %s (%s) against the committed %s", v->stem,
                         chorus_encode_status_name(status), error.field ? error.field : "", b);
            chorus_check(0, "%s: produced %s", v->stem, a);
            continue;
        }
        chorus_check(1, "%s: encode(fields) is the committed %zu bytes exactly", v->stem,
                     v->frame_len);

        /* Assertion two: the committed bytes decode to the fields. */
        chorus_v2_frame_t frame = chorus_v2_decode_frame(v->frame, v->frame_len);
        int decoded = frame.outcome == CHORUS_FRAME_DECODED && frame.consumed == v->frame_len &&
                      same_fields(&frame.message, &built);
        chorus_check(decoded, "%s: decode(hex) recovers every field (%s, consumed %zu of %zu)",
                     v->stem, chorus_frame_outcome_name(frame.outcome), frame.consumed,
                     v->frame_len);
        passed += decoded;
    }
    printf("\nv2 golden vectors: %d of %zu passed\n", passed, vector_count);
    chorus_check(passed == (int)vector_count && vector_count > 0,
                 "every committed v2 vector passes both assertions");

    /* Every type v2 added has at least one vector, and this endpoint has a
     * type for every vector (the directory and the catalog agree). */
    static const uint8_t ADDED[] = {0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x20,
                                    0x21, 0x22, 0x23, 0x24, 0x30, 0x31, 0x32, 0x33, 0x34,
                                    0x35, 0x36, 0x37, 0x38, 0x39, 0x3A, 0x3B, 0x3C};
    for (size_t t = 0; t < sizeof(ADDED); t++) {
        int found = 0;
        for (size_t i = 0; i < vector_count; i++) {
            if (vectors[i].frame_len > 0 && vectors[i].frame[0] == ADDED[t]) {
                found = 1;
            }
        }
        chorus_check(found, "type 0x%02x (%s) has a committed vector", ADDED[t],
                     chorus_v2_type_name(ADDED[t]));
    }
    return passed;
}

/* --- robustness, mirroring crates/protocol/tests/v2_rules.rs -------------- */

static void refused_both_ways(const chorus_v2_message_t *m, const char *field)
{
    uint8_t out[512];
    size_t written = 12345;
    chorus_v2_field_error_t error = {NULL, CHORUS_V2_PROBLEM_NONE, NULL};
    chorus_encode_status_t status = chorus_v2_encode(m, out, sizeof(out), &written, &error);
    chorus_check(status == CHORUS_ENCODE_INVALID_FIELD && error.field != NULL &&
                     strcmp(error.field, field) == 0 && written == 12345,
                 "%s: the encoder refuses %s by name and writes nothing (%s, %s)",
                 chorus_v2_type_name(m->type), field, chorus_encode_status_name(status),
                 error.field ? error.field : "-");
    chorus_check(chorus_v2_validate(m, NULL) != 0, "%s: validate rejects it too",
                 chorus_v2_type_name(m->type));
}

static chorus_v2_message_t stream_format(uint8_t codec, const uint8_t *map, size_t channels,
                                         uint32_t frames, const uint8_t *config, size_t config_len)
{
    chorus_v2_message_t m;
    memset(&m, 0, sizeof(m));
    m.type = CHORUS_V2_STREAM_FORMAT;
    m.as.stream_format.codec = codec;
    m.as.stream_format.sample_format = CHORUS_FMT_PCM_S16LE;
    m.as.stream_format.sample_rate_hz = 48000;
    m.as.stream_format.channels = channels;
    memcpy(m.as.stream_format.channel_map, map, channels);
    m.as.stream_format.frames_per_chunk = frames;
    m.as.stream_format.codec_config.data = config;
    m.as.stream_format.codec_config.len = config_len;
    return m;
}

static void the_stated_value_rules_are_refused_by_the_encoder(void)
{
    chorus_section("the value rules, refused by the encoder");
    chorus_v2_message_t m;
    memset(&m, 0, sizeof(m));
    m.type = CHORUS_V2_CAPABILITIES;
    m.as.capabilities.codecs = CHORUS_V2_CODEC_BIT(CHORUS_V2_CODEC_FLAC);
    m.as.capabilities.sample_formats = 1;
    m.as.capabilities.max_channels = 2;
    m.as.capabilities.rate_count = 1;
    m.as.capabilities.sample_rates_hz[0] = 48000;
    m.as.capabilities.buffer_ms = 100;
    refused_both_ways(&m, "codecs");

    const uint8_t fl_fl[2] = {1, 1};
    const uint8_t mono_fl[2] = {0, 1};
    const uint8_t fl_fr[2] = {1, 2};
    const uint8_t fl[1] = {1};
    const uint8_t one[1] = {1};
    static const uint8_t flac33[33];
    static const uint8_t opus[19] = {'O', 'p', 'u', 's', 'H', 'e', 'a', 'd', 1, 2};
    m = stream_format(CHORUS_V2_CODEC_PCM, fl_fl, 2, 960, NULL, 0);
    refused_both_ways(&m, "channel_map");
    m = stream_format(CHORUS_V2_CODEC_PCM, mono_fl, 2, 960, NULL, 0);
    refused_both_ways(&m, "channel_map");
    m = stream_format(CHORUS_V2_CODEC_PCM, fl_fr, 2, 960, one, 1);
    refused_both_ways(&m, "codec_config");
    m = stream_format(CHORUS_V2_CODEC_FLAC, fl_fr, 2, 4096, flac33, sizeof(flac33));
    refused_both_ways(&m, "codec_config");
    m = stream_format(CHORUS_V2_CODEC_OPUS, fl_fr, 2, 1000, opus, sizeof(opus));
    refused_both_ways(&m, "frames_per_chunk");
    m = stream_format(CHORUS_V2_CODEC_OPUS, fl, 1, 960, opus, sizeof(opus));
    refused_both_ways(&m, "codec_config");

    memset(&m, 0, sizeof(m));
    m.type = CHORUS_V2_OUTPUT_DELAY;
    m.as.output_delay.delay_ns = 5000000001ull;
    refused_both_ways(&m, "delay_ns");

    memset(&m, 0, sizeof(m));
    m.type = CHORUS_V2_CONTROLLER_COMMAND;
    m.as.controller_command.command = 9; /* join */
    refused_both_ways(&m, "target");
    m.as.controller_command.command = 6; /* volume_set */
    m.as.controller_command.value = 101;
    refused_both_ways(&m, "value");

    static const uint8_t three[3];
    memset(&m, 0, sizeof(m));
    m.type = CHORUS_V2_ARTWORK;
    m.as.artwork.artwork_id = 1;
    m.as.artwork.total_len = 4;
    m.as.artwork.offset = 2;
    m.as.artwork.mime.data = (const uint8_t *)"image/png";
    m.as.artwork.mime.len = 9;
    m.as.artwork.data.data = three;
    m.as.artwork.data.len = 3;
    refused_both_ways(&m, "offset");

    memset(&m, 0, sizeof(m));
    m.type = CHORUS_V2_HELLO;
    m.as.hello.protocol_version = 2;
    m.as.hello.roles = 1u << 9;
    refused_both_ways(&m, "roles");

    /* What a C struct can hold and a Rust type cannot: an undefined
     * enumeration byte, a bool that is neither, text that is not UTF-8. The
     * decoder rejects each, so the encoder refuses each. */
    m.as.hello.roles = CHORUS_V2_ROLE_PLAYER;
    static const uint8_t bad_utf8[2] = {0xC3, 0x28};
    m.as.hello.name.data = bad_utf8;
    m.as.hello.name.len = 2;
    refused_both_ways(&m, "name");
    memset(&m, 0, sizeof(m));
    m.type = CHORUS_V2_CONTROLLER_STATE;
    m.as.controller_state.muted = 2;
    refused_both_ways(&m, "muted");
    memset(&m, 0, sizeof(m));
    m.type = CHORUS_V2_SOURCE_CONTROL;
    m.as.source_control.action = 3;
    m.as.source_control.codec = 1;
    refused_both_ways(&m, "action");
    memset(&m, 0, sizeof(m));
    m.type = CHORUS_V2_SESSION_REFUSED;
    m.as.session_refused.reason = 2;
    static uint8_t long_text[1025];
    memset(long_text, 'a', sizeof(long_text));
    m.as.session_refused.detail.data = long_text;
    m.as.session_refused.detail.len = sizeof(long_text);
    refused_both_ways(&m, "detail");

    /* room_volume: each field on its own range, and a gain above the limit is
     * a message the format accepts (the player clamps it, chorus/volume.h). */
    memset(&m, 0, sizeof(m));
    m.type = CHORUS_V2_ROOM_VOLUME;
    m.as.room_volume.gain = 1001;
    m.as.room_volume.limit = 1000;
    refused_both_ways(&m, "gain");
    m.as.room_volume.gain = 0;
    m.as.room_volume.limit = 1001;
    refused_both_ways(&m, "limit");
    m.as.room_volume.limit = 0;
    m.as.room_volume.ramp_ms = 60001;
    refused_both_ways(&m, "ramp_ms");
    m.as.room_volume.gain = 900;
    m.as.room_volume.limit = 600;
    m.as.room_volume.ramp_ms = 60000;
    chorus_check(chorus_v2_validate(&m, NULL) == 0,
                 "room_volume: gain 900 above limit 600, ramp 60000 ms, is accepted");
}

static const vector_t *vector(const char *stem);

/* fixtures/protocol/v2/rejected: frames the format does not accept, each
 * with the field both directions must name. Walked like the directory above;
 * the counts line says how many of how many were refused both ways. */
static void the_committed_rejection_vectors_are_refused_both_ways(void)
{
    chorus_section(
        "fixtures/protocol/v2/rejected: refused by the encoder, rejected by the decoder");
    char dir_path[512];
    chorus_repo_path(dir_path, sizeof(dir_path), "fixtures/protocol/v2/rejected");
    DIR *dir = opendir(dir_path);
    chorus_check(dir != NULL, "fixtures/protocol/v2/rejected/ is readable");
    if (dir == NULL) {
        return;
    }
    static vector_t rejected[64];
    size_t count = 0;
    struct dirent *entry;
    while ((entry = readdir(dir)) != NULL) {
        const char *dot = strrchr(entry->d_name, '.');
        if (dot == NULL || strcmp(dot, ".hex") != 0) {
            continue;
        }
        size_t stem_len = (size_t)(dot - entry->d_name);
        if (count >= sizeof(rejected) / sizeof(rejected[0]) ||
            stem_len >= sizeof(rejected[0].stem)) {
            chorus_check(0, "more rejection vectors than this suite has room for");
            break;
        }
        memcpy(rejected[count].stem, entry->d_name, stem_len);
        rejected[count].stem[stem_len] = '\0';
        count++;
    }
    closedir(dir);
    qsort(rejected, count, sizeof(rejected[0]), compare_stems);

    static char hex_text[TEXT_CAP];
    static char fields_text[TEXT_CAP];
    int passed = 0;
    for (size_t i = 0; i < count; i++) {
        vector_t *v = &rejected[i];
        char path[512];
        char relative[256];
        snprintf(relative, sizeof(relative), "fixtures/protocol/v2/rejected/%.127s.hex", v->stem);
        chorus_repo_path(path, sizeof(path), relative);
        long hex_ok = fixture_read(path, hex_text, sizeof(hex_text));
        snprintf(relative, sizeof(relative), "fixtures/protocol/v2/rejected/%.127s.fields",
                 v->stem);
        chorus_repo_path(path, sizeof(path), relative);
        long fields_ok = fixture_read(path, fields_text, sizeof(fields_text));
        long len = (hex_ok < 0) ? -1 : fixture_parse_hex(hex_text, v->frame, sizeof(v->frame));
        char field[64];
        char problem[64];
        if (len < 0 || fields_ok < 0 ||
            fixture_field(fields_text, "rejected_field", field, sizeof(field)) == NULL ||
            fixture_field(fields_text, "problem", problem, sizeof(problem)) == NULL) {
            chorus_check(0,
                         "%s: both files are readable, the .hex parses, and the .fields "
                         "names rejected_field and problem",
                         v->stem);
            continue;
        }
        v->frame_len = (size_t)len;
        /* Out of range for a number; undefined for a bit or a role no version
         * defines (sound's flags and role); inconsistent for fields that
         * contradict each other (low_latency_offer's end with a key,
         * low_latency_accept's refusal with a port; goal 13). */
        chorus_v2_problem_t expected = CHORUS_V2_PROBLEM_NONE;
        if (strcmp(problem, "out_of_range") == 0) {
            expected = CHORUS_V2_PROBLEM_OUT_OF_RANGE;
        } else if (strcmp(problem, "undefined") == 0) {
            expected = CHORUS_V2_PROBLEM_UNDEFINED;
        } else if (strcmp(problem, "inconsistent") == 0) {
            expected = CHORUS_V2_PROBLEM_INCONSISTENT;
        }
        chorus_check(expected != CHORUS_V2_PROBLEM_NONE, "%s: problem = %s", v->stem, problem);
        chorus_v2_message_t built;
        if (!build(v->stem, fields_text, &built)) {
            continue;
        }
        static uint8_t produced[FRAME_CAP];
        size_t written = 12345;
        chorus_v2_field_error_t error = {NULL, CHORUS_V2_PROBLEM_NONE, NULL};
        chorus_encode_status_t status =
            chorus_v2_encode(&built, produced, sizeof(produced), &written, &error);
        int refused = status == CHORUS_ENCODE_INVALID_FIELD && written == 12345 &&
                      error.field != NULL && strcmp(error.field, field) == 0 &&
                      error.problem == expected;
        chorus_check(refused, "%s: the encoder refuses %s (%s) and writes nothing (%s)", v->stem,
                     field, problem, chorus_encode_status_name(status));
        chorus_v2_frame_t d = chorus_v2_decode_frame(v->frame, v->frame_len);
        int rejected_ok = d.outcome == CHORUS_FRAME_INVALID_FIELD && d.consumed == v->frame_len &&
                          d.error.field != NULL && strcmp(d.error.field, field) == 0 &&
                          d.error.problem == expected;
        chorus_check(rejected_ok,
                     "%s: the decoder rejects %s (%s) and consumes the whole frame "
                     "(%s, consumed %zu of %zu)",
                     v->stem, field, problem, chorus_frame_outcome_name(d.outcome), d.consumed,
                     v->frame_len);
        /* The next frame after a rejected one is still found and decoded. */
        const vector_t *good = vector("room_volume");
        int next_ok = 0;
        if (good != NULL && v->frame_len + good->frame_len <= FRAME_CAP) {
            static uint8_t stream[FRAME_CAP];
            memcpy(stream, v->frame, v->frame_len);
            memcpy(stream + v->frame_len, good->frame, good->frame_len);
            chorus_v2_frame_t next = chorus_v2_decode_frame(
                stream + d.consumed, v->frame_len + good->frame_len - d.consumed);
            next_ok = next.outcome == CHORUS_FRAME_DECODED &&
                      next.message.type == CHORUS_V2_ROOM_VOLUME &&
                      next.consumed == good->frame_len;
        }
        chorus_check(next_ok, "%s: the frame after it still decodes", v->stem);
        passed += refused && rejected_ok && next_ok;
    }
    printf("\nv2 rejection vectors: %d of %zu refused both ways\n", passed, count);
    chorus_check(passed == (int)count && count > 0,
                 "every committed rejection vector is refused both ways");
}

static const vector_t *vector(const char *stem)
{
    for (size_t i = 0; i < vector_count; i++) {
        if (strcmp(vectors[i].stem, stem) == 0) {
            return &vectors[i];
        }
    }
    return NULL;
}

static void a_rule_broken_on_the_wire_is_rejected_as_that_field(void)
{
    chorus_section("a rule broken on the wire is rejected as that field");
    const vector_t *caps = vector("capabilities");
    const vector_t *hello = vector("hello_endpoint");
    const vector_t *refused = vector("session_refused_v1_peer");
    if (caps == NULL || hello == NULL || refused == NULL) {
        chorus_check(0, "the capabilities, hello_endpoint and refusal vectors are committed");
        return;
    }
    uint8_t frame[4096];
    memcpy(frame, caps->frame, caps->frame_len);
    frame[3] = CHORUS_V2_CODEC_BIT(CHORUS_V2_CODEC_FLAC);
    chorus_v2_frame_t d = chorus_v2_decode_frame(frame, caps->frame_len);
    chorus_check(d.outcome == CHORUS_FRAME_INVALID_FIELD && strcmp(d.error.field, "codecs") == 0 &&
                     d.consumed == caps->frame_len,
                 "capabilities without PCM is rejected as codecs, and costs one frame");

    memcpy(frame, hello->frame, hello->frame_len);
    frame[7] = 250;
    d = chorus_v2_decode_frame(frame, hello->frame_len);
    chorus_check(d.outcome == CHORUS_FRAME_INVALID_FIELD && strcmp(d.error.field, "name") == 0 &&
                     d.error.problem == CHORUS_V2_PROBLEM_TRUNCATED,
                 "a hello name length that runs past the payload is rejected as name, "
                 "truncated, and not read (%s %s)",
                 d.error.field ? d.error.field : "-", chorus_v2_problem_name(d.error.problem));

    /* Text that is not UTF-8: the refusal's detail with one byte made 0xFF. */
    memcpy(frame, refused->frame, refused->frame_len);
    frame[6] = 0xFF;
    d = chorus_v2_decode_frame(frame, refused->frame_len);
    chorus_check(d.outcome == CHORUS_FRAME_INVALID_FIELD && strcmp(d.error.field, "detail") == 0 &&
                     d.error.problem == CHORUS_V2_PROBLEM_NOT_UTF8,
                 "a detail that is not UTF-8 is rejected as detail, not-utf8");

    /* A long text declaring more than 1024 bytes is too long before it is
     * read. */
    uint8_t big[3 + 3 + 1100];
    memset(big, 'a', sizeof(big));
    big[0] = CHORUS_V2_SESSION_REFUSED;
    big[1] = (uint8_t)((sizeof(big) - 3) >> 8);
    big[2] = (uint8_t)((sizeof(big) - 3) & 0xFF);
    big[3] = 1;
    big[4] = (uint8_t)(1100 >> 8);
    big[5] = (uint8_t)(1100 & 0xFF);
    d = chorus_v2_decode_frame(big, sizeof(big));
    chorus_check(d.outcome == CHORUS_FRAME_INVALID_FIELD && strcmp(d.error.field, "detail") == 0 &&
                     d.error.problem == CHORUS_V2_PROBLEM_TOO_LONG,
                 "a 1100-byte detail is rejected as too long");

    /* A handshake_init whose magic is not CHRS is not a chorus peer. */
    const vector_t *init = vector("handshake_init");
    if (init != NULL) {
        memcpy(frame, init->frame, init->frame_len);
        frame[3] = 'X';
        d = chorus_v2_decode_frame(frame, init->frame_len);
        chorus_check(d.outcome == CHORUS_FRAME_INVALID_FIELD && strcmp(d.error.field, "magic") == 0,
                     "a handshake_init without CHRS is rejected as magic");
    }
}

static void the_decoder_checks_happen_in_the_committed_order(void)
{
    chorus_section("decoder behaviour, in order");
    uint8_t two[2] = {CHORUS_V2_HELLO, 0};
    chorus_v2_frame_t d = chorus_v2_decode_frame(two, sizeof(two));
    chorus_check(d.outcome == CHORUS_FRAME_TRUNCATED_HEADER && d.consumed == 0,
                 "1. fewer than three bytes is a truncated header and consumes nothing");
    uint8_t over[5] = {0x7F, 0x00, 0x20, 1, 2};
    d = chorus_v2_decode_frame(over, sizeof(over));
    chorus_check(d.outcome == CHORUS_FRAME_DECLARED_LENGTH_EXCEEDS_BUFFER && d.consumed == 0,
                 "2. an unknown type whose length exceeds the buffer is a length problem first");
    uint8_t unknown[3 + 2 + 3] = {0x7F, 0x00, 0x02, 0xAA, 0xBB, 0x10, 0x00, 0x00};
    d = chorus_v2_decode_frame(unknown, sizeof(unknown));
    chorus_check(d.outcome == CHORUS_FRAME_SKIPPED_UNKNOWN_TYPE && d.consumed == 5,
                 "3. an unassigned type is skipped by its length prefix");
    uint8_t short_hello[3 + 5] = {CHORUS_V2_HELLO, 0x00, 0x05, 0, 2, 0, 1, 0};
    d = chorus_v2_decode_frame(short_hello, sizeof(short_hello));
    chorus_check(d.outcome == CHORUS_FRAME_PAYLOAD_TOO_SHORT_FOR_TYPE &&
                     d.consumed == sizeof(short_hello),
                 "4. a hello payload below its 6-byte minimum costs one frame");
    uint8_t zero_version[3 + 6] = {CHORUS_V2_HELLO, 0x00, 0x06, 0, 0, 0, 1, 0, 0};
    d = chorus_v2_decode_frame(zero_version, sizeof(zero_version));
    chorus_check(d.outcome == CHORUS_FRAME_INVALID_FIELD &&
                     strcmp(d.error.field, "protocol_version") == 0,
                 "5. a value rule broken is rejected as its field");
    uint8_t longer[3 + 8 + 4] = {CHORUS_V2_OUTPUT_DELAY, 0x00, 12, 0, 0, 0, 0, 0, 0, 0, 1};
    d = chorus_v2_decode_frame(longer, sizeof(longer));
    chorus_check(d.outcome == CHORUS_FRAME_DECODED && d.message.as.output_delay.delay_ns == 1,
                 "a payload longer than the known fields is accepted and the excess ignored");
    uint8_t zero[3] = {0, 0, 0};
    d = chorus_v2_decode_frame(zero, sizeof(zero));
    chorus_check(d.outcome == CHORUS_FRAME_SKIPPED_UNKNOWN_TYPE,
                 "an all-zero buffer is an unknown type, not a message");
}

static void every_single_byte_corruption_is_handled(void)
{
    chorus_section("every single-byte corruption of every vector");
    size_t decoded = 0;
    size_t rejected = 0;
    int bad = 0;
    static const uint8_t flips[3] = {0x01, 0x80, 0xFF};
    for (size_t i = 0; i < vector_count; i++) {
        const vector_t *v = &vectors[i];
        for (size_t at = 0; at < v->frame_len; at++) {
            for (size_t k = 0; k < 3; k++) {
                static uint8_t corrupt[4096];
                memcpy(corrupt, v->frame, v->frame_len);
                corrupt[at] ^= flips[k];
                chorus_v2_frame_t d = chorus_v2_decode_frame(corrupt, v->frame_len);
                if (d.consumed > v->frame_len) {
                    bad++;
                    chorus_check(0, "%s byte %zu: consumed past the buffer", v->stem, at);
                    continue;
                }
                if (d.outcome == CHORUS_FRAME_DECODED) {
                    decoded++;
                    static uint8_t again[FRAME_CAP];
                    size_t again_len = 0;
                    chorus_encode_status_t s =
                        chorus_v2_encode(&d.message, again, sizeof(again), &again_len, NULL);
                    chorus_v2_frame_t back = chorus_v2_decode_frame(again, again_len);
                    if (s != CHORUS_ENCODE_OK || back.outcome != CHORUS_FRAME_DECODED ||
                        !same_fields(&back.message, &d.message)) {
                        bad++;
                        chorus_check(0,
                                     "%s byte %zu flip 0x%02x: decoded but does not re-encode "
                                     "and decode the same (%s)",
                                     v->stem, at, flips[k], chorus_encode_status_name(s));
                    }
                } else if (d.outcome != CHORUS_FRAME_SKIPPED_UNKNOWN_TYPE) {
                    rejected++;
                }
            }
        }
    }
    printf("corruptions: %zu still decoded and round-tripped, %zu rejected\n", decoded, rejected);
    chorus_check(bad == 0 && decoded > 0 && rejected > 0,
                 "every corruption was handled: none read past its buffer, and every one that "
                 "still decoded re-encoded to a frame that decodes the same");
}

static void every_prefix_of_a_stream_of_all_vectors_is_safe(void)
{
    chorus_section("every prefix of a stream of all the vectors");
    static uint8_t stream[FRAME_CAP];
    size_t len = 0;
    for (size_t i = 0; i < vector_count; i++) {
        memcpy(stream + len, vectors[i].frame, vectors[i].frame_len);
        len += vectors[i].frame_len;
    }
    int bad = 0;
    size_t whole = 0;
    for (size_t cut = 0; cut <= len; cut++) {
        size_t at = 0;
        size_t frames = 0;
        while (at < cut) {
            chorus_v2_frame_t d = chorus_v2_decode_frame(stream + at, cut - at);
            if (d.consumed == 0) {
                break;
            }
            if (d.outcome != CHORUS_FRAME_DECODED) {
                bad++;
            }
            at += d.consumed;
            frames++;
        }
        if (at > cut) {
            bad++;
        }
        if (cut == len) {
            whole = frames;
            chorus_check(at == len, "the whole stream of %zu bytes is consumed", len);
        }
    }
    chorus_check(whole == vector_count, "the whole stream decodes as %zu frames, one per vector",
                 whole);
    chorus_check(bad == 0,
                 "no prefix of the stream decodes anything but whole vectors, and none reads "
                 "past its cut (%zu prefixes)",
                 len + 1);
}

static void the_encoder_refuses_rather_than_truncating(void)
{
    chorus_section("the encoder refuses rather than truncating");
    const vector_t *md = vector("metadata");
    if (md == NULL) {
        chorus_check(0, "the metadata vector is committed");
        return;
    }
    chorus_v2_frame_t d = chorus_v2_decode_frame(md->frame, md->frame_len);
    uint8_t out[16];
    size_t written = 777;
    chorus_check(chorus_v2_encode(&d.message, out, sizeof(out), &written, NULL) ==
                         CHORUS_ENCODE_BUFFER_TOO_SMALL &&
                     written == 777,
                 "a buffer too small for the frame gets nothing");
    static uint8_t big[65536];
    chorus_v2_message_t m;
    memset(&m, 0, sizeof(m));
    m.type = CHORUS_V2_CODED_CHUNK;
    m.as.coded_chunk.frames = 960;
    m.as.coded_chunk.data.data = big;
    m.as.coded_chunk.data.len = 65535 - 16 + 1;
    static uint8_t frame[FRAME_CAP];
    chorus_check(chorus_v2_encode(&m, frame, sizeof(frame), &written, NULL) ==
                     CHORUS_ENCODE_PAYLOAD_TOO_LONG,
                 "a coded_chunk payload past 65535 bytes is refused, not wrapped");
    m.as.coded_chunk.data.len = 65535 - 16;
    chorus_check(chorus_v2_encode(&m, frame, sizeof(frame), &written, NULL) == CHORUS_ENCODE_OK &&
                     written == 65538,
                 "and one of exactly 65535 is the largest frame, 65538 bytes");
}

static void the_v1_types_are_v1s_own(void)
{
    chorus_section("v1's three types, unchanged");
    const char *stems[3] = {"time_sync", "audio_chunk", "stream_end"};
    for (int i = 0; i < 3; i++) {
        static char text[TEXT_CAP];
        static uint8_t frame[4096];
        char path[512];
        char relative[256];
        snprintf(relative, sizeof(relative), "fixtures/protocol/%s.hex", stems[i]);
        chorus_repo_path(path, sizeof(path), relative);
        long len = (fixture_read(path, text, sizeof(text)) < 0)
                       ? -1
                       : fixture_parse_hex(text, frame, sizeof(frame));
        chorus_v2_frame_t d = chorus_v2_decode_frame(frame, (size_t)(len < 0 ? 0 : len));
        static uint8_t again[FRAME_CAP];
        size_t again_len = 0;
        int ok = len > 0 && d.outcome == CHORUS_FRAME_DECODED &&
                 chorus_v2_encode(&d.message, again, sizeof(again), &again_len, NULL) ==
                     CHORUS_ENCODE_OK &&
                 again_len == (size_t)len && memcmp(again, frame, again_len) == 0;
        chorus_check(ok,
                     "%s: the v1 vector decodes and re-encodes through the v2 codec "
                     "byte for byte",
                     stems[i]);
    }
}

/* Goal 15: telemetry's heap block. Optional and trailing, so an endpoint that
 * reports no heap sends exactly the 36 bytes it always did (the committed
 * `telemetry` vector), a block is there whole or not at all, and a tail
 * shorter than the block is excess. */
static void telemetry_carries_heap_only_when_it_is_known(void)
{
    chorus_section("telemetry: the optional heap block");
    chorus_v2_message_t m;
    memset(&m, 0, sizeof(m));
    m.type = CHORUS_V2_TELEMETRY;
    m.as.telemetry.sync_error_ns = INT64_MIN;
    m.as.telemetry.rssi_dbm = INT8_MIN;
    m.as.telemetry.temperature_centi_c = INT16_MIN;
    m.as.telemetry.heap_free_bytes = CHORUS_V2_TELEMETRY_HEAP_UNKNOWN;
    m.as.telemetry.heap_min_free_bytes = CHORUS_V2_TELEMETRY_HEAP_UNKNOWN;
    static uint8_t without[64];
    static uint8_t with[64];
    size_t without_len = 0;
    size_t with_len = 0;
    chorus_check(chorus_v2_encode(&m, without, sizeof(without), &without_len, NULL) ==
                         CHORUS_ENCODE_OK &&
                     without_len == CHORUS_FRAME_HEADER_LEN + 36u,
                 "a telemetry with no heap known is 36 bytes of payload, as before the block");
    m.as.telemetry.heap_free_bytes = 187432u;
    chorus_check(
        chorus_v2_encode(&m, with, sizeof(with), &with_len, NULL) == CHORUS_ENCODE_OK &&
            with_len == without_len + CHORUS_V2_TELEMETRY_HEAP_BLOCK_LEN &&
            memcmp(with + CHORUS_FRAME_HEADER_LEN, without + CHORUS_FRAME_HEADER_LEN, 36) == 0,
        "one figure known writes the whole 8-byte block after the same 36 bytes");
    chorus_v2_frame_t d = chorus_v2_decode_frame(with, with_len);
    chorus_check(d.outcome == CHORUS_FRAME_DECODED &&
                     d.message.as.telemetry.heap_free_bytes == 187432u &&
                     d.message.as.telemetry.heap_min_free_bytes == CHORUS_V2_TELEMETRY_HEAP_UNKNOWN,
                 "the block decodes, and the figure not known stays unknown");
    d = chorus_v2_decode_frame(without, without_len);
    chorus_check(d.outcome == CHORUS_FRAME_DECODED &&
                     d.message.as.telemetry.heap_free_bytes == CHORUS_V2_TELEMETRY_HEAP_UNKNOWN &&
                     d.message.as.telemetry.heap_min_free_bytes == CHORUS_V2_TELEMETRY_HEAP_UNKNOWN,
                 "no block reads as heap unknown, never as zero");
    /* A tail of 4 bytes is not a block: excess, ignored. */
    static uint8_t partial[64];
    memcpy(partial, with, with_len - 4u);
    partial[2] = (uint8_t)(36u + 4u);
    d = chorus_v2_decode_frame(partial, with_len - 4u);
    chorus_check(d.outcome == CHORUS_FRAME_DECODED && d.consumed == with_len - 4u &&
                     d.message.as.telemetry.heap_free_bytes == CHORUS_V2_TELEMETRY_HEAP_UNKNOWN,
                 "a tail shorter than the block is excess and reads as heap unknown");
}

/* Goal 12, phase A: the session keeps the last `sound` it decoded and a
 * getter reads it back, which is what the endpoint DSP track builds on. The
 * committed vectors are decoded and handed to the session's own store, in
 * order, and the getter returns the last, field for field. */
static void the_session_keeps_the_last_sound_it_decoded(void)
{
    chorus_section("sound: the session keeps the last one (chorus_session_last_sound)");
    static chorus_session_result_t result;
    memset(&result, 0, sizeof(result));
    chorus_v2_sound_t got;
    chorus_check(chorus_session_last_sound(&result, &got) == 0, "none before the first");
    const char *const order[] = {"sound_flat", "sound_eq_8", "sound_sub_2_1"};
    for (size_t i = 0; i < sizeof(order) / sizeof(order[0]); i++) {
        const vector_t *v = vector(order[i]);
        if (v == NULL) {
            chorus_check(0, "%s is committed", order[i]);
            return;
        }
        chorus_v2_frame_t d = chorus_v2_decode_frame(v->frame, v->frame_len);
        chorus_check(d.outcome == CHORUS_FRAME_DECODED && d.message.type == CHORUS_V2_SOUND,
                     "%s decodes as sound", order[i]);
        chorus_session_keep_sound(&result, &d.message.as.sound);
    }
    int have = chorus_session_last_sound(&result, &got);
    chorus_check(have == 1 && result.sounds_received == 3, "three kept, the last one held");
    /* sound_sub_2_1: the LFE member of a 2.1 set, the sub inverted. */
    chorus_check(got.bass_db == 3 && got.treble_db == -2 && got.role == 4 && got.sub_present == 1 &&
                     got.crossover_hz == 100 && got.sub_level_cdb == -350 &&
                     got.flags == (CHORUS_V2_SOUND_FLAG_LOUDNESS | CHORUS_V2_SOUND_FLAG_ROOM_EQ |
                                   CHORUS_V2_SOUND_FLAG_SUB_INVERTED) &&
                     got.eq_count == 1 && got.filters[0].freq_hz == 42 &&
                     got.filters[0].gain_cdb == -600 && got.filters[0].q_milli == 4500,
                 "the getter returns sound_sub_2_1 field for field");
}

/* The voice role (docs/protocol.md, "The voice role"): the committed vectors
 * field for field, the role bit, the rules, and what a peer that does not
 * know the three types does. Mirrors crates/protocol/tests/v2_voice.rs. */
static void the_voice_role_messages_are_the_committed_vectors(void)
{
    chorus_section("the voice role: mic_audio, mic_state, voice_control");
    const vector_t *audio = vector("mic_audio");
    const vector_t *muted = vector("mic_state_muted");
    const vector_t *live = vector("mic_state_live");
    const vector_t *off = vector("voice_control_off");
    const vector_t *uplink = vector("voice_control_uplink");
    const vector_t *listening = vector("voice_control_listening");
    const vector_t *hello = vector("hello_voice");
    if (audio == NULL || muted == NULL || live == NULL || off == NULL || uplink == NULL ||
        listening == NULL || hello == NULL) {
        chorus_check(0, "the voice vectors are committed");
        return;
    }
    /* mic_audio, read here field by field (the directory walk compared
     * encodings): six little-endian samples behind big-endian fields. */
    chorus_v2_frame_t d = chorus_v2_decode_frame(audio->frame, audio->frame_len);
    const chorus_v2_mic_audio_t *a = &d.message.as.mic_audio;
    static const uint8_t samples[12] = {0x00, 0x00, 0x01, 0x00, 0xFF, 0xFF,
                                        0x39, 0x30, 0x00, 0x80, 0xFF, 0x7F};
    chorus_check(d.outcome == CHORUS_FRAME_DECODED && d.message.type == CHORUS_V2_MIC_AUDIO &&
                     a->format == CHORUS_V2_MIC_FORMAT_PCM_S16LE_16K_MONO && a->sequence == 7 &&
                     a->timestamp_ns == 3000000000ull && a->data.len == sizeof(samples) &&
                     memcmp(a->data.data, samples, sizeof(samples)) == 0,
                 "mic_audio.hex: format 1, sequence 7, timestamp 3 s, six samples as committed");
    d = chorus_v2_decode_frame(muted->frame, muted->frame_len);
    chorus_check(d.outcome == CHORUS_FRAME_DECODED && d.message.type == CHORUS_V2_MIC_STATE &&
                     d.message.as.mic_state.gate == CHORUS_V2_MIC_GATE_MUTED,
                 "mic_state_muted.hex: the gate is muted");
    d = chorus_v2_decode_frame(live->frame, live->frame_len);
    chorus_check(d.outcome == CHORUS_FRAME_DECODED &&
                     d.message.as.mic_state.gate == CHORUS_V2_MIC_GATE_LIVE,
                 "mic_state_live.hex: the gate is live");
    const struct {
        const vector_t *v;
        uint8_t uplink;
        uint8_t listening;
    } controls[] = {{off, 0, 0}, {uplink, 1, 0}, {listening, 1, 1}};
    for (size_t i = 0; i < sizeof(controls) / sizeof(controls[0]); i++) {
        d = chorus_v2_decode_frame(controls[i].v->frame, controls[i].v->frame_len);
        chorus_check(d.outcome == CHORUS_FRAME_DECODED &&
                         d.message.type == CHORUS_V2_VOICE_CONTROL &&
                         d.message.as.voice_control.uplink == controls[i].uplink &&
                         d.message.as.voice_control.listening == controls[i].listening,
                     "%s.hex: uplink %u, listening %u", controls[i].v->stem, controls[i].uplink,
                     controls[i].listening);
    }

    /* The role bit: bit 5, declared by hello_voice, and the role of all
     * three types (and of no type that every session carries). */
    d = chorus_v2_decode_frame(hello->frame, hello->frame_len);
    chorus_check(d.outcome == CHORUS_FRAME_DECODED && d.message.as.hello.roles == 0x002Du &&
                     (d.message.as.hello.roles & CHORUS_V2_ROLE_VOICE) != 0 &&
                     CHORUS_V2_ROLE_VOICE == (1u << 5),
                 "hello_voice.hex declares the voice role, bit 5 (roles 0x%04x)",
                 d.message.as.hello.roles);
    chorus_check(chorus_v2_type_role(CHORUS_V2_MIC_AUDIO) == CHORUS_V2_ROLE_VOICE &&
                     chorus_v2_type_role(CHORUS_V2_MIC_STATE) == CHORUS_V2_ROLE_VOICE &&
                     chorus_v2_type_role(CHORUS_V2_VOICE_CONTROL) == CHORUS_V2_ROLE_VOICE,
                 "mic_audio, mic_state and voice_control are the voice role's");
    chorus_check(chorus_v2_type_role(CHORUS_V2_SOURCE_OFFER) == CHORUS_V2_ROLE_SOURCE &&
                     chorus_v2_type_role(CHORUS_V2_AUDIO_CHUNK) == 0 &&
                     chorus_v2_type_role(CHORUS_V2_TELEMETRY) == 0 &&
                     chorus_v2_type_role(0x7F) == 0,
                 "a source message is the source role's, and audio_chunk, telemetry and an "
                 "unassigned byte are no role's");
    static uint8_t frame[4096];
    memcpy(frame, hello->frame, hello->frame_len);
    frame[6] |= 0x40;
    d = chorus_v2_decode_frame(frame, hello->frame_len);
    chorus_check(d.outcome == CHORUS_FRAME_INVALID_FIELD && strcmp(d.error.field, "roles") == 0 &&
                     d.error.problem == CHORUS_V2_PROBLEM_UNDEFINED,
                 "bit 6 of roles is still no role: rejected as roles, undefined");
    uint8_t no_mic = 0;
    chorus_check(chorus_v2_enum_from_name(CHORUS_V2_ENUM_SOURCE_KIND, "microphone", &no_mic) != 0 &&
                     chorus_v2_enum_name(CHORUS_V2_ENUM_SOURCE_KIND, 4) == NULL,
                 "there is no microphone source kind: mic audio is not a source");

    /* The rules: the longest chunk and one sample are accepted; one sample
     * more, none, and half a sample are refused as data. */
    static uint8_t pcm[(CHORUS_V2_MIC_MAX_SAMPLES + 1u) * CHORUS_V2_MIC_BYTES_PER_SAMPLE];
    static uint8_t out[FRAME_CAP];
    size_t written = 0;
    chorus_v2_field_error_t error = {NULL, CHORUS_V2_PROBLEM_NONE, NULL};
    chorus_v2_message_t m;
    memset(&m, 0, sizeof(m));
    m.type = CHORUS_V2_MIC_AUDIO;
    m.as.mic_audio.format = CHORUS_V2_MIC_FORMAT_PCM_S16LE_16K_MONO;
    m.as.mic_audio.data.data = pcm;
    m.as.mic_audio.data.len = CHORUS_V2_MIC_MAX_SAMPLES * CHORUS_V2_MIC_BYTES_PER_SAMPLE;
    chorus_check(chorus_v2_encode(&m, out, sizeof(out), &written, NULL) == CHORUS_ENCODE_OK &&
                     written == 3u + 13u + 3200u,
                 "mic_audio: 1600 samples (100 ms) is the longest chunk, a 3216-byte frame");
    d = chorus_v2_decode_frame(out, written);
    chorus_check(d.outcome == CHORUS_FRAME_DECODED && d.message.as.mic_audio.data.len == 3200u,
                 "and it decodes");
    m.as.mic_audio.data.len = CHORUS_V2_MIC_BYTES_PER_SAMPLE;
    chorus_check(chorus_v2_encode(&m, out, sizeof(out), &written, NULL) == CHORUS_ENCODE_OK &&
                     written == 3u + chorus_v2_min_payload_len(CHORUS_V2_MIC_AUDIO),
                 "mic_audio: one sample is the shortest, the type's minimum payload");
    m.as.mic_audio.data.len = sizeof(pcm);
    chorus_check(chorus_v2_validate(&m, &error) != 0 && strcmp(error.field, "data") == 0 &&
                     error.problem == CHORUS_V2_PROBLEM_TOO_LONG,
                 "mic_audio: 1601 samples is refused as data, too long");
    refused_both_ways(&m, "data");
    m.as.mic_audio.data.len = 0;
    refused_both_ways(&m, "data");
    m.as.mic_audio.data.len = 3;
    refused_both_ways(&m, "data");
    m.as.mic_audio.data.len = 2;
    m.as.mic_audio.format = 0;
    refused_both_ways(&m, "format");
    memset(&m, 0, sizeof(m));
    m.type = CHORUS_V2_MIC_STATE;
    m.as.mic_state.gate = 2;
    refused_both_ways(&m, "gate");
    memset(&m, 0, sizeof(m));
    m.type = CHORUS_V2_VOICE_CONTROL;
    m.as.voice_control.uplink = 2;
    refused_both_ways(&m, "uplink");
    m.as.voice_control.uplink = 1;
    m.as.voice_control.listening = 2;
    refused_both_ways(&m, "listening");

    /* On the wire: a gate or a bool no version defines is that field, and a
     * later version's trailing byte is ignored. */
    const uint8_t gate2[] = {CHORUS_V2_MIC_STATE, 0x00, 0x01, 0x02};
    d = chorus_v2_decode_frame(gate2, sizeof(gate2));
    chorus_check(d.outcome == CHORUS_FRAME_INVALID_FIELD && d.consumed == sizeof(gate2) &&
                     strcmp(d.error.field, "gate") == 0,
                 "a mic_state gate of 2 is rejected as gate and costs one frame");
    const uint8_t uplink2[] = {CHORUS_V2_VOICE_CONTROL, 0x00, 0x02, 0x02, 0x00};
    d = chorus_v2_decode_frame(uplink2, sizeof(uplink2));
    chorus_check(d.outcome == CHORUS_FRAME_INVALID_FIELD && strcmp(d.error.field, "uplink") == 0,
                 "a voice_control uplink of 2 is rejected as uplink");
    const uint8_t listening2[] = {CHORUS_V2_VOICE_CONTROL, 0x00, 0x02, 0x01, 0x02};
    d = chorus_v2_decode_frame(listening2, sizeof(listening2));
    chorus_check(d.outcome == CHORUS_FRAME_INVALID_FIELD && strcmp(d.error.field, "listening") == 0,
                 "a voice_control listening of 2 is rejected as listening");
    const uint8_t longer[] = {CHORUS_V2_VOICE_CONTROL, 0x00, 0x03, 0x01, 0x01, 0x7F};
    d = chorus_v2_decode_frame(longer, sizeof(longer));
    chorus_check(d.outcome == CHORUS_FRAME_DECODED && d.message.as.voice_control.uplink == 1 &&
                     d.message.as.voice_control.listening == 1,
                 "a voice_control with a byte after its fields is accepted, the excess ignored");

    /* A peer that does not know the three types: the v1 decoder
     * (chorus/protocol.h), whose catalog ends at 0x03, steps over each by
     * its length and decodes what follows. */
    const vector_t *voice[] = {audio, muted, live, off, uplink, listening};
    static char text[TEXT_CAP];
    static uint8_t time_sync[64];
    char path[512];
    chorus_repo_path(path, sizeof(path), "fixtures/protocol/time_sync.hex");
    long ts_len = (fixture_read(path, text, sizeof(text)) < 0)
                      ? -1
                      : fixture_parse_hex(text, time_sync, sizeof(time_sync));
    chorus_check(ts_len > 0, "fixtures/protocol/time_sync.hex is readable");
    for (size_t i = 0; ts_len > 0 && i < sizeof(voice) / sizeof(voice[0]); i++) {
        memcpy(frame, voice[i]->frame, voice[i]->frame_len);
        memcpy(frame + voice[i]->frame_len, time_sync, (size_t)ts_len);
        chorus_frame_t old = chorus_decode_frame(frame, voice[i]->frame_len + (size_t)ts_len);
        chorus_frame_t next = chorus_decode_frame(
            frame + old.consumed, voice[i]->frame_len + (size_t)ts_len - old.consumed);
        chorus_check(old.outcome == CHORUS_FRAME_SKIPPED_UNKNOWN_TYPE &&
                         old.consumed == voice[i]->frame_len &&
                         next.outcome == CHORUS_FRAME_DECODED && next.consumed == (size_t)ts_len,
                     "%s: a peer that does not know the type skips it and decodes the next frame "
                     "(%s, then %s)",
                     voice[i]->stem, chorus_frame_outcome_name(old.outcome),
                     chorus_frame_outcome_name(next.outcome));
    }
}

int main(void)
{
    the_committed_vectors_round_trip();
    the_stated_value_rules_are_refused_by_the_encoder();
    the_committed_rejection_vectors_are_refused_both_ways();
    a_rule_broken_on_the_wire_is_rejected_as_that_field();
    the_decoder_checks_happen_in_the_committed_order();
    every_single_byte_corruption_is_handled();
    every_prefix_of_a_stream_of_all_vectors_is_safe();
    the_encoder_refuses_rather_than_truncating();
    the_v1_types_are_v1s_own();
    the_session_keeps_the_last_sound_it_decoded();
    telemetry_carries_heap_only_when_it_is_known();
    the_voice_role_messages_are_the_committed_vectors();
    return chorus_test_report("test_protocol_v2");
}
