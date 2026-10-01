#include "chorus/protocol_v2.h"

#include <string.h>

const uint8_t CHORUS_V2_MAGIC[CHORUS_V2_MAGIC_LEN] = {0x43, 0x48, 0x52, 0x53};

/* --- the catalog ------------------------------------------------------------ */

typedef struct {
    uint8_t wire;
    const char *name;
    size_t minimum;
} type_entry_t;

/* Minimum payloads, from docs/protocol.md's tables: the fixed fields plus
 * every length prefix, with a one-element list where a list may not be
 * empty. */
static const type_entry_t TYPES[] = {
    {CHORUS_V2_TIME_SYNC, "time_sync", CHORUS_TIME_SYNC_PAYLOAD_LEN},
    {CHORUS_V2_AUDIO_CHUNK, "audio_chunk", CHORUS_CHUNK_HEADER_LEN + 1},
    {CHORUS_V2_STREAM_END, "stream_end", CHORUS_STREAM_END_PAYLOAD_LEN},
    /* version, roles, name length, software length */
    {CHORUS_V2_HELLO, "hello", 6},
    /* codecs, formats, channels, rate count, one rate, buffer, latency, leds,
     * bands */
    {CHORUS_V2_CAPABILITIES, "capabilities", 1 + 1 + 1 + 1 + 4 + 2 + 4 + 2 + 1},
    /* codec, format, rate, channels, one map entry, frames per chunk, setup
     * length */
    {CHORUS_V2_STREAM_FORMAT, "stream_format", 1 + 1 + 4 + 1 + 1 + 4 + 2},
    /* sequence, timestamp, frames, one data byte */
    {CHORUS_V2_CODED_CHUNK, "coded_chunk", 4 + 8 + 4 + 1},
    {CHORUS_V2_OUTPUT_DELAY, "output_delay", 8},
    {CHORUS_V2_TELEMETRY, "telemetry", 8 + 8 + 4 + 4 + 4 + 4 + 1 + 1 + 2},
    /* magic, version, suite, the 32-byte ephemeral key */
    {CHORUS_V2_HANDSHAKE_INIT, "handshake_init", 4 + 2 + 1 + 32},
    /* e, encrypted s, an encrypted payload's tag */
    {CHORUS_V2_HANDSHAKE_RESPONSE, "handshake_response", 32 + 48 + 16},
    /* encrypted s, an encrypted payload's tag */
    {CHORUS_V2_HANDSHAKE_FINISH, "handshake_finish", 48 + 16},
    /* reason, detail length */
    {CHORUS_V2_SESSION_REFUSED, "session_refused", 1 + 2},
    /* a tag and at least one frame header */
    {CHORUS_V2_SECURE_RECORD, "secure_record", 16 + 3},
    /* playback, position, duration, position_at, artwork, four lengths */
    {CHORUS_V2_METADATA, "metadata", 1 + 4 + 4 + 8 + 4 + 2 * 4},
    /* id, total, offset, mime length, one data byte */
    {CHORUS_V2_ARTWORK, "artwork", 4 + 4 + 4 + 1 + 1},
    /* command, value, target length */
    {CHORUS_V2_CONTROLLER_COMMAND, "controller_command", 1 + 2 + 1},
    /* volume, muted, playback, group length */
    {CHORUS_V2_CONTROLLER_STATE, "controller_state", 1 + 1 + 1 + 1},
    /* timestamp, beat, peak, band count */
    {CHORUS_V2_VISUALIZER_FRAME, "visualizer_frame", 8 + 1 + 1 + 1},
    /* timestamp, r, g, b, brightness, transition */
    {CHORUS_V2_COLOR, "color", 8 + 4 + 2},
    /* id, kind, signal, name length */
    {CHORUS_V2_SOURCE_OFFER, "source_offer", 1 + 1 + 1 + 1},
    /* id, action, codec */
    {CHORUS_V2_SOURCE_CONTROL, "source_control", 1 + 1 + 1},
    /* gain, limit, ramp */
    {CHORUS_V2_ROOM_VOLUME, "room_volume", 2 + 2 + 2},
    /* bass, treble, flags, role, sub_present, crossover, sub level, eq_count */
    {CHORUS_V2_SOUND, "sound", 1 + 1 + 1 + 1 + 1 + 2 + 2 + 1},
};
#define TYPE_COUNT (sizeof(TYPES) / sizeof(TYPES[0]))

static const type_entry_t *type_entry(uint8_t wire)
{
    for (size_t i = 0; i < TYPE_COUNT; i++) {
        if (TYPES[i].wire == wire) {
            return &TYPES[i];
        }
    }
    return NULL;
}

const char *chorus_v2_type_name(uint8_t wire)
{
    const type_entry_t *entry = type_entry(wire);
    return (entry == NULL) ? NULL : entry->name;
}

uint8_t chorus_v2_type_from_name(const char *name)
{
    for (size_t i = 0; i < TYPE_COUNT; i++) {
        if (strcmp(TYPES[i].name, name) == 0) {
            return TYPES[i].wire;
        }
    }
    return 0;
}

size_t chorus_v2_min_payload_len(uint8_t wire)
{
    const type_entry_t *entry = type_entry(wire);
    return (entry == NULL) ? 0 : entry->minimum;
}

int chorus_v2_type_is_plaintext(uint8_t wire)
{
    return wire == CHORUS_V2_HANDSHAKE_INIT || wire == CHORUS_V2_HANDSHAKE_RESPONSE ||
           wire == CHORUS_V2_HANDSHAKE_FINISH || wire == CHORUS_V2_SESSION_REFUSED ||
           wire == CHORUS_V2_SECURE_RECORD;
}

static int is_v1(uint8_t wire)
{
    return wire == CHORUS_V2_TIME_SYNC || wire == CHORUS_V2_AUDIO_CHUNK ||
           wire == CHORUS_V2_STREAM_END;
}

/* --- enumerations ----------------------------------------------------------- */

static const char *const CODECS[] = {NULL, "pcm", "flac", "opus"};
static const char *const POSITIONS[] = {"MONO", "FL",  "FR",  "FC",  "LFE", "BL", "BR",
                                        "FLC",  "FRC", "BC",  "SL",  "SR",  "TC", "TFL",
                                        "TFC",  "TFR", "TBL", "TBC", "TBR"};
static const char *const REASONS[] = {
    NULL,          "protocol_version", "key_changed", "handshake_failed", "unsupported_suite",
    "server_full", "not_adopted"};
static const char *const SUITES[] = {NULL, "Noise_XX_25519_ChaChaPoly_SHA256"};
static const char *const PLAYBACKS[] = {"stopped", "playing", "paused"};
static const char *const COMMANDS[] = {NULL,       "play",     "pause",      "toggle",
                                       "next",     "previous", "volume_set", "volume_step",
                                       "mute_set", "join",     "leave"};
static const char *const KINDS[] = {NULL, "line_in", "optical", "hdmi_arc"};
static const char *const ACTIONS[] = {NULL, "start", "stop"};
static const char *const LINKS[] = {"unknown", "wired", "wireless"};
static const char *const ROLES[] = {"player", "metadata", "controller", "visualizer", "source"};

static const char *const *enum_table(chorus_v2_enum_t which, size_t *count)
{
    const char *const *table = NULL;
    size_t n = 0;
    switch (which) {
    case CHORUS_V2_ENUM_CODEC:
        table = CODECS;
        n = sizeof(CODECS) / sizeof(CODECS[0]);
        break;
    case CHORUS_V2_ENUM_CHANNEL_POSITION:
        table = POSITIONS;
        n = sizeof(POSITIONS) / sizeof(POSITIONS[0]);
        break;
    case CHORUS_V2_ENUM_REFUSAL_REASON:
        table = REASONS;
        n = sizeof(REASONS) / sizeof(REASONS[0]);
        break;
    case CHORUS_V2_ENUM_SUITE:
        table = SUITES;
        n = sizeof(SUITES) / sizeof(SUITES[0]);
        break;
    case CHORUS_V2_ENUM_PLAYBACK:
        table = PLAYBACKS;
        n = sizeof(PLAYBACKS) / sizeof(PLAYBACKS[0]);
        break;
    case CHORUS_V2_ENUM_COMMAND:
        table = COMMANDS;
        n = sizeof(COMMANDS) / sizeof(COMMANDS[0]);
        break;
    case CHORUS_V2_ENUM_SOURCE_KIND:
        table = KINDS;
        n = sizeof(KINDS) / sizeof(KINDS[0]);
        break;
    case CHORUS_V2_ENUM_SOURCE_ACTION:
        table = ACTIONS;
        n = sizeof(ACTIONS) / sizeof(ACTIONS[0]);
        break;
    case CHORUS_V2_ENUM_LINK:
        table = LINKS;
        n = sizeof(LINKS) / sizeof(LINKS[0]);
        break;
    case CHORUS_V2_ENUM_ROLE_BIT:
        table = ROLES;
        n = sizeof(ROLES) / sizeof(ROLES[0]);
        break;
    }
    *count = n;
    return table;
}

const char *chorus_v2_enum_name(chorus_v2_enum_t which, uint8_t value)
{
    size_t count = 0;
    const char *const *table = enum_table(which, &count);
    if (table == NULL || value >= count) {
        return NULL;
    }
    return table[value];
}

int chorus_v2_enum_from_name(chorus_v2_enum_t which, const char *name, uint8_t *out)
{
    size_t count = 0;
    const char *const *table = enum_table(which, &count);
    for (size_t i = 0; table != NULL && i < count; i++) {
        if (table[i] != NULL && strcmp(table[i], name) == 0) {
            *out = (uint8_t)i;
            return 0;
        }
    }
    return -1;
}

static int defined(chorus_v2_enum_t which, uint8_t value)
{
    return chorus_v2_enum_name(which, value) != NULL;
}

const char *chorus_v2_problem_name(chorus_v2_problem_t problem)
{
    switch (problem) {
    case CHORUS_V2_PROBLEM_NONE:
        return "none";
    case CHORUS_V2_PROBLEM_UNDEFINED:
        return "undefined";
    case CHORUS_V2_PROBLEM_OUT_OF_RANGE:
        return "out-of-range";
    case CHORUS_V2_PROBLEM_TRUNCATED:
        return "truncated";
    case CHORUS_V2_PROBLEM_NOT_UTF8:
        return "not-utf8";
    case CHORUS_V2_PROBLEM_TOO_LONG:
        return "too-long";
    case CHORUS_V2_PROBLEM_INCONSISTENT:
        return "inconsistent";
    }
    return "unknown-problem";
}

/* --- UTF-8 ------------------------------------------------------------------ */

/* RFC 3629 section 4's syntax: no overlong forms, no surrogates, nothing past
 * U+10FFFF. */
int chorus_v2_is_utf8(const uint8_t *text, size_t len)
{
    size_t i = 0;
    while (i < len) {
        uint8_t b = text[i];
        if (b < 0x80) {
            i++;
            continue;
        }
        size_t need;
        uint8_t low = 0x80;
        uint8_t high = 0xBF;
        if (b >= 0xC2 && b <= 0xDF) {
            need = 1;
        } else if (b >= 0xE0 && b <= 0xEF) {
            need = 2;
            if (b == 0xE0) {
                low = 0xA0;
            } else if (b == 0xED) {
                high = 0x9F;
            }
        } else if (b >= 0xF0 && b <= 0xF4) {
            need = 3;
            if (b == 0xF0) {
                low = 0x90;
            } else if (b == 0xF4) {
                high = 0x8F;
            }
        } else {
            return 0;
        }
        if (len - i - 1 < need) {
            return 0;
        }
        if (text[i + 1] < low || text[i + 1] > high) {
            return 0;
        }
        for (size_t k = 2; k <= need; k++) {
            if (text[i + k] < 0x80 || text[i + k] > 0xBF) {
                return 0;
            }
        }
        i += need + 1;
    }
    return 1;
}

/* --- reading ---------------------------------------------------------------- */

static int fail(chorus_v2_field_error_t *error, const char *field, chorus_v2_problem_t problem,
                const char *why)
{
    if (error != NULL) {
        error->field = field;
        error->problem = problem;
        error->why = why;
    }
    return -1;
}

typedef struct {
    const uint8_t *buf;
    size_t len;
    size_t at;
    chorus_v2_field_error_t *error;
} reader_t;

static int take(reader_t *r, const char *field, size_t n, const uint8_t **out)
{
    if (r->len - r->at < n) {
        return fail(r->error, field, CHORUS_V2_PROBLEM_TRUNCATED, NULL);
    }
    *out = r->buf + r->at;
    r->at += n;
    return 0;
}

static int read_u8(reader_t *r, const char *field, uint8_t *out)
{
    const uint8_t *p;
    if (take(r, field, 1, &p) != 0) {
        return -1;
    }
    *out = p[0];
    return 0;
}

static int read_u16(reader_t *r, const char *field, uint16_t *out)
{
    const uint8_t *p;
    if (take(r, field, 2, &p) != 0) {
        return -1;
    }
    *out = (uint16_t)(((uint16_t)p[0] << 8) | p[1]);
    return 0;
}

static int read_u32(reader_t *r, const char *field, uint32_t *out)
{
    const uint8_t *p;
    if (take(r, field, 4, &p) != 0) {
        return -1;
    }
    *out = ((uint32_t)p[0] << 24) | ((uint32_t)p[1] << 16) | ((uint32_t)p[2] << 8) | p[3];
    return 0;
}

static int read_u64(reader_t *r, const char *field, uint64_t *out)
{
    const uint8_t *p;
    if (take(r, field, 8, &p) != 0) {
        return -1;
    }
    uint64_t value = 0;
    for (int i = 0; i < 8; i++) {
        value = (value << 8) | p[i];
    }
    *out = value;
    return 0;
}

static int read_enum(reader_t *r, const char *field, chorus_v2_enum_t which, uint8_t *out)
{
    if (read_u8(r, field, out) != 0) {
        return -1;
    }
    if (!defined(which, *out)) {
        return fail(r->error, field, CHORUS_V2_PROBLEM_UNDEFINED, NULL);
    }
    return 0;
}

static int read_bool(reader_t *r, const char *field, uint8_t *out)
{
    if (read_u8(r, field, out) != 0) {
        return -1;
    }
    if (*out > 1) {
        return fail(r->error, field, CHORUS_V2_PROBLEM_UNDEFINED, NULL);
    }
    return 0;
}

static int read_text(reader_t *r, const char *field, size_t n, chorus_v2_bytes_t *out)
{
    const uint8_t *p;
    if (take(r, field, n, &p) != 0) {
        return -1;
    }
    if (!chorus_v2_is_utf8(p, n)) {
        return fail(r->error, field, CHORUS_V2_PROBLEM_NOT_UTF8, NULL);
    }
    out->data = p;
    out->len = n;
    return 0;
}

static int read_short_text(reader_t *r, const char *field, chorus_v2_bytes_t *out)
{
    uint8_t n;
    if (read_u8(r, field, &n) != 0) {
        return -1;
    }
    return read_text(r, field, n, out);
}

static int read_long_text(reader_t *r, const char *field, chorus_v2_bytes_t *out)
{
    uint16_t n;
    if (read_u16(r, field, &n) != 0) {
        return -1;
    }
    if (n > CHORUS_V2_MAX_LONG_TEXT) {
        return fail(r->error, field, CHORUS_V2_PROBLEM_TOO_LONG, NULL);
    }
    return read_text(r, field, n, out);
}

static int read_bytes(reader_t *r, const char *field, size_t n, chorus_v2_bytes_t *out)
{
    const uint8_t *p;
    if (take(r, field, n, &p) != 0) {
        return -1;
    }
    out->data = p;
    out->len = n;
    return 0;
}

static void read_rest(reader_t *r, chorus_v2_bytes_t *out)
{
    out->data = r->buf + r->at;
    out->len = r->len - r->at;
    r->at = r->len;
}

/* Structure only: every length is bounds-checked here, enumerations and
 * texts are checked as they are read, and the value rules are
 * chorus_v2_validate's. The order of the reads is the order of the fields,
 * which is what decides which field a malformed payload is reported as. */
static int decode_payload(uint8_t type, const uint8_t *payload, size_t len, chorus_v2_message_t *m,
                          chorus_v2_field_error_t *error)
{
    reader_t r = {payload, len, 0, error};
    memset(m, 0, sizeof(*m));
    m->type = type;
    switch (type) {
    case CHORUS_V2_HELLO: {
        chorus_v2_hello_t *h = &m->as.hello;
        if (read_u16(&r, "protocol_version", &h->protocol_version) != 0 ||
            read_u16(&r, "roles", &h->roles) != 0 || read_short_text(&r, "name", &h->name) != 0 ||
            read_short_text(&r, "software", &h->software) != 0) {
            return -1;
        }
        return 0;
    }
    case CHORUS_V2_CAPABILITIES: {
        chorus_v2_capabilities_t *c = &m->as.capabilities;
        uint8_t count;
        if (read_u8(&r, "codecs", &c->codecs) != 0 ||
            read_u8(&r, "sample_formats", &c->sample_formats) != 0 ||
            read_u8(&r, "max_channels", &c->max_channels) != 0 ||
            read_u8(&r, "rate_count", &count) != 0) {
            return -1;
        }
        c->rate_count = count;
        for (size_t i = 0; i < count; i++) {
            uint32_t rate;
            if (read_u32(&r, "sample_rates_hz", &rate) != 0) {
                return -1;
            }
            if (i < CHORUS_V2_MAX_RATES) {
                c->sample_rates_hz[i] = rate;
            }
        }
        if (read_u16(&r, "buffer_ms", &c->buffer_ms) != 0 ||
            read_u32(&r, "intrinsic_latency_ns", &c->intrinsic_latency_ns) != 0 ||
            read_u16(&r, "led_count", &c->led_count) != 0 ||
            read_u8(&r, "visualizer_bands", &c->visualizer_bands) != 0) {
            return -1;
        }
        return 0;
    }
    case CHORUS_V2_STREAM_FORMAT: {
        chorus_v2_stream_format_t *f = &m->as.stream_format;
        uint8_t channels;
        if (read_enum(&r, "codec", CHORUS_V2_ENUM_CODEC, &f->codec) != 0) {
            return -1;
        }
        if (read_u8(&r, "sample_format", &f->sample_format) != 0) {
            return -1;
        }
        if (chorus_sample_format_bytes(f->sample_format) == 0) {
            return fail(error, "sample_format", CHORUS_V2_PROBLEM_UNDEFINED, NULL);
        }
        if (read_u32(&r, "sample_rate_hz", &f->sample_rate_hz) != 0 ||
            read_u8(&r, "channels", &channels) != 0) {
            return -1;
        }
        f->channels = channels;
        for (size_t i = 0; i < channels; i++) {
            uint8_t position;
            if (read_enum(&r, "channel_map", CHORUS_V2_ENUM_CHANNEL_POSITION, &position) != 0) {
                return -1;
            }
            if (i < CHORUS_MAX_CHANNELS) {
                f->channel_map[i] = position;
            }
        }
        uint16_t config_len;
        if (read_u32(&r, "frames_per_chunk", &f->frames_per_chunk) != 0 ||
            read_u16(&r, "codec_config_len", &config_len) != 0 ||
            read_bytes(&r, "codec_config", config_len, &f->codec_config) != 0) {
            return -1;
        }
        return 0;
    }
    case CHORUS_V2_CODED_CHUNK: {
        chorus_v2_coded_chunk_t *c = &m->as.coded_chunk;
        if (read_u32(&r, "sequence", &c->sequence) != 0 ||
            read_u64(&r, "timestamp_ns", &c->timestamp_ns) != 0 ||
            read_u32(&r, "frames", &c->frames) != 0) {
            return -1;
        }
        read_rest(&r, &c->data);
        return 0;
    }
    case CHORUS_V2_OUTPUT_DELAY:
        return read_u64(&r, "delay_ns", &m->as.output_delay.delay_ns);
    case CHORUS_V2_TELEMETRY: {
        chorus_v2_telemetry_t *t = &m->as.telemetry;
        uint64_t sync_error;
        uint32_t correction;
        uint8_t rssi;
        uint16_t temperature;
        if (read_u64(&r, "taken_ns", &t->taken_ns) != 0 ||
            read_u64(&r, "sync_error_ns", &sync_error) != 0 ||
            read_u32(&r, "buffer_fill_us", &t->buffer_fill_us) != 0 ||
            read_u32(&r, "underruns", &t->underruns) != 0 ||
            read_u32(&r, "resyncs", &t->resyncs) != 0 ||
            read_u32(&r, "correction_ppb", &correction) != 0 ||
            read_enum(&r, "link", CHORUS_V2_ENUM_LINK, &t->link) != 0 ||
            read_u8(&r, "rssi_dbm", &rssi) != 0 ||
            read_u16(&r, "temperature_centi_c", &temperature) != 0) {
            return -1;
        }
        /* Two's complement reinterpretations, spelled out rather than left to
         * an implementation-defined conversion. */
        t->sync_error_ns = (sync_error > (uint64_t)INT64_MAX)
                               ? (int64_t)(sync_error - (uint64_t)INT64_MAX - 1u) + INT64_MIN
                               : (int64_t)sync_error;
        t->correction_ppb = (correction > (uint32_t)INT32_MAX)
                                ? (int32_t)(correction - (uint32_t)INT32_MAX - 1u) + INT32_MIN
                                : (int32_t)correction;
        t->rssi_dbm = (rssi > 127u) ? (int8_t)((int)rssi - 256) : (int8_t)rssi;
        t->temperature_centi_c =
            (temperature > 32767u) ? (int16_t)((int32_t)temperature - 65536) : (int16_t)temperature;
        return 0;
    }
    case CHORUS_V2_HANDSHAKE_INIT: {
        chorus_v2_handshake_init_t *h = &m->as.handshake_init;
        const uint8_t *magic;
        if (take(&r, "magic", CHORUS_V2_MAGIC_LEN, &magic) != 0) {
            return -1;
        }
        if (memcmp(magic, CHORUS_V2_MAGIC, CHORUS_V2_MAGIC_LEN) != 0) {
            return fail(error, "magic", CHORUS_V2_PROBLEM_INCONSISTENT, "is not ASCII CHRS");
        }
        if (read_u16(&r, "protocol_version", &h->protocol_version) != 0 ||
            read_enum(&r, "suite", CHORUS_V2_ENUM_SUITE, &h->suite) != 0) {
            return -1;
        }
        read_rest(&r, &h->noise);
        return 0;
    }
    case CHORUS_V2_HANDSHAKE_RESPONSE:
        read_rest(&r, &m->as.handshake_response.noise);
        return 0;
    case CHORUS_V2_HANDSHAKE_FINISH:
        read_rest(&r, &m->as.handshake_finish.noise);
        return 0;
    case CHORUS_V2_SESSION_REFUSED: {
        chorus_v2_session_refused_t *s = &m->as.session_refused;
        if (read_enum(&r, "reason", CHORUS_V2_ENUM_REFUSAL_REASON, &s->reason) != 0 ||
            read_long_text(&r, "detail", &s->detail) != 0) {
            return -1;
        }
        return 0;
    }
    case CHORUS_V2_SECURE_RECORD:
        read_rest(&r, &m->as.secure_record.ciphertext);
        return 0;
    case CHORUS_V2_METADATA: {
        chorus_v2_metadata_t *d = &m->as.metadata;
        if (read_enum(&r, "playback", CHORUS_V2_ENUM_PLAYBACK, &d->playback) != 0 ||
            read_u32(&r, "position_ms", &d->position_ms) != 0 ||
            read_u32(&r, "duration_ms", &d->duration_ms) != 0 ||
            read_u64(&r, "position_at_ns", &d->position_at_ns) != 0 ||
            read_u32(&r, "artwork_id", &d->artwork_id) != 0 ||
            read_long_text(&r, "title", &d->title) != 0 ||
            read_long_text(&r, "artist", &d->artist) != 0 ||
            read_long_text(&r, "album", &d->album) != 0 ||
            read_long_text(&r, "source", &d->source) != 0) {
            return -1;
        }
        return 0;
    }
    case CHORUS_V2_ARTWORK: {
        chorus_v2_artwork_t *a = &m->as.artwork;
        if (read_u32(&r, "artwork_id", &a->artwork_id) != 0 ||
            read_u32(&r, "total_len", &a->total_len) != 0 ||
            read_u32(&r, "offset", &a->offset) != 0 || read_short_text(&r, "mime", &a->mime) != 0) {
            return -1;
        }
        read_rest(&r, &a->data);
        return 0;
    }
    case CHORUS_V2_CONTROLLER_COMMAND: {
        chorus_v2_controller_command_t *c = &m->as.controller_command;
        uint16_t value;
        if (read_enum(&r, "command", CHORUS_V2_ENUM_COMMAND, &c->command) != 0 ||
            read_u16(&r, "value", &value) != 0 || read_short_text(&r, "target", &c->target) != 0) {
            return -1;
        }
        c->value = (value > 32767u) ? (int16_t)((int32_t)value - 65536) : (int16_t)value;
        return 0;
    }
    case CHORUS_V2_CONTROLLER_STATE: {
        chorus_v2_controller_state_t *s = &m->as.controller_state;
        if (read_u8(&r, "volume", &s->volume) != 0 || read_bool(&r, "muted", &s->muted) != 0 ||
            read_enum(&r, "playback", CHORUS_V2_ENUM_PLAYBACK, &s->playback) != 0 ||
            read_short_text(&r, "group", &s->group) != 0) {
            return -1;
        }
        return 0;
    }
    case CHORUS_V2_VISUALIZER_FRAME: {
        chorus_v2_visualizer_frame_t *v = &m->as.visualizer_frame;
        uint8_t count;
        if (read_u64(&r, "timestamp_ns", &v->timestamp_ns) != 0 ||
            read_u8(&r, "beat", &v->beat) != 0 || read_u8(&r, "peak", &v->peak) != 0 ||
            read_u8(&r, "band_count", &count) != 0 ||
            read_bytes(&r, "bands", count, &v->bands) != 0) {
            return -1;
        }
        return 0;
    }
    case CHORUS_V2_COLOR: {
        chorus_v2_color_t *c = &m->as.color;
        if (read_u64(&r, "timestamp_ns", &c->timestamp_ns) != 0 ||
            read_u8(&r, "red", &c->red) != 0 || read_u8(&r, "green", &c->green) != 0 ||
            read_u8(&r, "blue", &c->blue) != 0 || read_u8(&r, "brightness", &c->brightness) != 0 ||
            read_u16(&r, "transition_ms", &c->transition_ms) != 0) {
            return -1;
        }
        return 0;
    }
    case CHORUS_V2_SOURCE_OFFER: {
        chorus_v2_source_offer_t *s = &m->as.source_offer;
        if (read_u8(&r, "source_id", &s->source_id) != 0 ||
            read_enum(&r, "kind", CHORUS_V2_ENUM_SOURCE_KIND, &s->kind) != 0 ||
            read_bool(&r, "signal", &s->signal) != 0 ||
            read_short_text(&r, "name", &s->name) != 0) {
            return -1;
        }
        return 0;
    }
    case CHORUS_V2_SOURCE_CONTROL: {
        chorus_v2_source_control_t *s = &m->as.source_control;
        if (read_u8(&r, "source_id", &s->source_id) != 0 ||
            read_enum(&r, "action", CHORUS_V2_ENUM_SOURCE_ACTION, &s->action) != 0 ||
            read_enum(&r, "codec", CHORUS_V2_ENUM_CODEC, &s->codec) != 0) {
            return -1;
        }
        return 0;
    }
    case CHORUS_V2_ROOM_VOLUME: {
        chorus_v2_room_volume_t *v = &m->as.room_volume;
        if (read_u16(&r, "gain", &v->gain) != 0 || read_u16(&r, "limit", &v->limit) != 0 ||
            read_u16(&r, "ramp_ms", &v->ramp_ms) != 0) {
            return -1;
        }
        return 0;
    }
    case CHORUS_V2_SOUND: {
        chorus_v2_sound_t *s = &m->as.sound;
        uint8_t bass;
        uint8_t treble;
        uint16_t level;
        if (read_u8(&r, "bass_db", &bass) != 0 || read_u8(&r, "treble_db", &treble) != 0 ||
            read_u8(&r, "flags", &s->flags) != 0 || read_u8(&r, "role", &s->role) != 0 ||
            read_bool(&r, "sub_present", &s->sub_present) != 0 ||
            read_u16(&r, "crossover_hz", &s->crossover_hz) != 0 ||
            read_u16(&r, "sub_level_cdb", &level) != 0 ||
            read_u8(&r, "eq_count", &s->eq_count) != 0) {
            return -1;
        }
        s->bass_db = (int8_t)bass;
        s->treble_db = (int8_t)treble;
        s->sub_level_cdb = (int16_t)level;
        /* Before any filter is read: there is room for eight, and a count
         * past that is the count out of range (as the Rust decoder says). */
        if (s->eq_count > CHORUS_V2_SOUND_EQ_MAX_FILTERS) {
            return fail(error, "eq_count", CHORUS_V2_PROBLEM_OUT_OF_RANGE, NULL);
        }
        for (uint8_t i = 0; i < s->eq_count; i++) {
            uint16_t gain;
            if (read_u16(&r, "freq_hz", &s->filters[i].freq_hz) != 0 ||
                read_u16(&r, "gain_cdb", &gain) != 0 ||
                read_u16(&r, "q_milli", &s->filters[i].q_milli) != 0) {
                return -1;
            }
            s->filters[i].gain_cdb = (int16_t)gain;
        }
        return 0;
    }
    default:
        return fail(error, "type", CHORUS_V2_PROBLEM_UNDEFINED, NULL);
    }
}

/* --- the value rules -------------------------------------------------------- */

static int rate_ok(const char *field, uint32_t rate, chorus_v2_field_error_t *error)
{
    if (rate < CHORUS_MIN_SAMPLE_RATE_HZ || rate > CHORUS_MAX_SAMPLE_RATE_HZ) {
        return fail(error, field, CHORUS_V2_PROBLEM_OUT_OF_RANGE, NULL);
    }
    return 0;
}

/* A text the encoder was handed: the decoder checked UTF-8 as it read, and
 * the encoder has to refuse what the decoder would. */
static int text_ok(const char *field, const chorus_v2_bytes_t *text, size_t max,
                   chorus_v2_field_error_t *error)
{
    if (text->len > max) {
        return fail(error, field, CHORUS_V2_PROBLEM_TOO_LONG, NULL);
    }
    if (text->len > 0 && text->data == NULL) {
        return fail(error, field, CHORUS_V2_PROBLEM_TRUNCATED, NULL);
    }
    if (text->len > 0 && !chorus_v2_is_utf8(text->data, text->len)) {
        return fail(error, field, CHORUS_V2_PROBLEM_NOT_UTF8, NULL);
    }
    return 0;
}

static int bytes_ok(const char *field, const chorus_v2_bytes_t *bytes,
                    chorus_v2_field_error_t *error)
{
    if (bytes->len > 0 && bytes->data == NULL) {
        return fail(error, field, CHORUS_V2_PROBLEM_TRUNCATED, NULL);
    }
    return 0;
}

static int enum_ok(const char *field, chorus_v2_enum_t which, uint8_t value,
                   chorus_v2_field_error_t *error)
{
    if (!defined(which, value)) {
        return fail(error, field, CHORUS_V2_PROBLEM_UNDEFINED, NULL);
    }
    return 0;
}

static int validate_stream_format(const chorus_v2_stream_format_t *f,
                                  chorus_v2_field_error_t *error)
{
    static const uint32_t OPUS_FRAMES[] = {120, 240, 480, 960, 1920, 2880};
    if (enum_ok("codec", CHORUS_V2_ENUM_CODEC, f->codec, error) != 0) {
        return -1;
    }
    if (chorus_sample_format_bytes(f->sample_format) == 0) {
        return fail(error, "sample_format", CHORUS_V2_PROBLEM_UNDEFINED, NULL);
    }
    if (rate_ok("sample_rate_hz", f->sample_rate_hz, error) != 0) {
        return -1;
    }
    size_t n = f->channels;
    if (n == 0 || n > CHORUS_MAX_CHANNELS) {
        return fail(error, "channels", CHORUS_V2_PROBLEM_OUT_OF_RANGE, NULL);
    }
    for (size_t i = 0; i < n; i++) {
        if (enum_ok("channel_map", CHORUS_V2_ENUM_CHANNEL_POSITION, f->channel_map[i], error) !=
            0) {
            return -1;
        }
    }
    if (n > 1) {
        for (size_t i = 0; i < n; i++) {
            if (f->channel_map[i] == CHORUS_V2_POSITION_MONO) {
                return fail(error, "channel_map", CHORUS_V2_PROBLEM_INCONSISTENT,
                            "MONO is only for a one-channel stream");
            }
        }
    }
    for (size_t i = 0; i < n; i++) {
        for (size_t j = 0; j < i; j++) {
            if (f->channel_map[j] == f->channel_map[i]) {
                return fail(error, "channel_map", CHORUS_V2_PROBLEM_INCONSISTENT,
                            "a position appears twice");
            }
        }
    }
    if (f->frames_per_chunk == 0) {
        return fail(error, "frames_per_chunk", CHORUS_V2_PROBLEM_OUT_OF_RANGE, NULL);
    }
    if (bytes_ok("codec_config", &f->codec_config, error) != 0) {
        return -1;
    }
    switch (f->codec) {
    case CHORUS_V2_CODEC_PCM:
        if (f->codec_config.len != 0) {
            return fail(error, "codec_config", CHORUS_V2_PROBLEM_INCONSISTENT,
                        "PCM carries no codec setup");
        }
        break;
    case CHORUS_V2_CODEC_FLAC:
        if (f->codec_config.len != CHORUS_V2_FLAC_STREAMINFO_LEN) {
            return fail(error, "codec_config", CHORUS_V2_PROBLEM_INCONSISTENT,
                        "FLAC setup is the 34-byte STREAMINFO body");
        }
        if (f->sample_format == CHORUS_FMT_PCM_F32LE) {
            return fail(error, "sample_format", CHORUS_V2_PROBLEM_INCONSISTENT,
                        "FLAC decodes to integer PCM");
        }
        break;
    case CHORUS_V2_CODEC_OPUS: {
        if (f->sample_rate_hz != 48000u) {
            return fail(error, "sample_rate_hz", CHORUS_V2_PROBLEM_INCONSISTENT,
                        "Opus streams decode at 48000 Hz");
        }
        int duration_ok = 0;
        for (size_t i = 0; i < sizeof(OPUS_FRAMES) / sizeof(OPUS_FRAMES[0]); i++) {
            if (OPUS_FRAMES[i] == f->frames_per_chunk) {
                duration_ok = 1;
            }
        }
        if (!duration_ok) {
            return fail(error, "frames_per_chunk", CHORUS_V2_PROBLEM_INCONSISTENT,
                        "not an Opus frame duration");
        }
        if (f->codec_config.len < CHORUS_V2_OPUS_HEAD_MIN_LEN ||
            memcmp(f->codec_config.data, "OpusHead", 8) != 0) {
            return fail(error, "codec_config", CHORUS_V2_PROBLEM_INCONSISTENT,
                        "Opus setup is an OpusHead ID header");
        }
        if ((size_t)f->codec_config.data[9] != n) {
            return fail(error, "codec_config", CHORUS_V2_PROBLEM_INCONSISTENT,
                        "OpusHead channel count differs from the channel map");
        }
        break;
    }
    default:
        break;
    }
    return 0;
}

static int validate_capabilities(const chorus_v2_capabilities_t *c, chorus_v2_field_error_t *error)
{
    const uint8_t pcm = CHORUS_V2_CODEC_BIT(CHORUS_V2_CODEC_PCM);
    const uint8_t all = (uint8_t)(pcm | CHORUS_V2_CODEC_BIT(CHORUS_V2_CODEC_FLAC) |
                                  CHORUS_V2_CODEC_BIT(CHORUS_V2_CODEC_OPUS));
    if ((c->codecs & pcm) == 0) {
        return fail(error, "codecs", CHORUS_V2_PROBLEM_INCONSISTENT, "PCM is mandatory");
    }
    if ((c->codecs & (uint8_t)~all) != 0) {
        return fail(error, "codecs", CHORUS_V2_PROBLEM_UNDEFINED, NULL);
    }
    if (c->sample_formats == 0 || (c->sample_formats & (uint8_t)~0x07u) != 0) {
        return fail(error, "sample_formats", CHORUS_V2_PROBLEM_UNDEFINED, NULL);
    }
    if (c->max_channels == 0 || c->max_channels > CHORUS_MAX_CHANNELS) {
        return fail(error, "max_channels", CHORUS_V2_PROBLEM_OUT_OF_RANGE, NULL);
    }
    if (c->rate_count == 0 || c->rate_count > CHORUS_V2_MAX_RATES) {
        return fail(error, "sample_rates_hz", CHORUS_V2_PROBLEM_TOO_LONG, NULL);
    }
    for (size_t i = 0; i < c->rate_count; i++) {
        if (rate_ok("sample_rates_hz", c->sample_rates_hz[i], error) != 0) {
            return -1;
        }
    }
    if (c->visualizer_bands > CHORUS_V2_MAX_VISUALIZER_BANDS) {
        return fail(error, "visualizer_bands", CHORUS_V2_PROBLEM_OUT_OF_RANGE, NULL);
    }
    return 0;
}

static int validate_artwork(const chorus_v2_artwork_t *a, chorus_v2_field_error_t *error)
{
    if (a->artwork_id == 0) {
        return fail(error, "artwork_id", CHORUS_V2_PROBLEM_OUT_OF_RANGE, NULL);
    }
    if (a->total_len == 0 || a->total_len > CHORUS_V2_MAX_ARTWORK_LEN) {
        return fail(error, "total_len", CHORUS_V2_PROBLEM_OUT_OF_RANGE, NULL);
    }
    if (bytes_ok("mime", &a->mime, error) != 0) {
        return -1;
    }
    int ascii = a->mime.len > 0;
    for (size_t i = 0; i < a->mime.len; i++) {
        if (a->mime.data[i] >= 0x80) {
            ascii = 0;
        }
    }
    if (!ascii) {
        return fail(error, "mime", CHORUS_V2_PROBLEM_INCONSISTENT,
                    "a media type is non-empty ASCII");
    }
    if (text_ok("mime", &a->mime, CHORUS_V2_MAX_SHORT_TEXT, error) != 0 ||
        bytes_ok("data", &a->data, error) != 0) {
        return -1;
    }
    if (a->data.len == 0) {
        return fail(error, "data", CHORUS_V2_PROBLEM_INCONSISTENT,
                    "a piece carries at least one byte");
    }
    if ((uint64_t)a->offset + (uint64_t)a->data.len > (uint64_t)a->total_len) {
        return fail(error, "offset", CHORUS_V2_PROBLEM_INCONSISTENT,
                    "the piece ends past total_len");
    }
    return 0;
}

static int validate_command(const chorus_v2_controller_command_t *c, chorus_v2_field_error_t *error)
{
    if (enum_ok("command", CHORUS_V2_ENUM_COMMAND, c->command, error) != 0) {
        return -1;
    }
    int v = c->value;
    int ok;
    switch (c->command) {
    case 6: /* volume_set */
        ok = v >= 0 && v <= 100;
        break;
    case 7: /* volume_step */
        ok = v >= -100 && v <= 100;
        break;
    case 8: /* mute_set */
        ok = v == 0 || v == 1;
        break;
    default:
        ok = v == 0;
        break;
    }
    if (!ok) {
        return fail(error, "value", CHORUS_V2_PROBLEM_OUT_OF_RANGE, NULL);
    }
    int join = c->command == 9;
    if (join == (c->target.len == 0)) {
        return fail(error, "target", CHORUS_V2_PROBLEM_INCONSISTENT,
                    "names a room or group for join, and only for join");
    }
    return text_ok("target", &c->target, CHORUS_V2_MAX_SHORT_TEXT, error);
}

static int validate_sound(const chorus_v2_sound_t *s, chorus_v2_field_error_t *error);

int chorus_v2_validate(const chorus_v2_message_t *message, chorus_v2_field_error_t *error)
{
    const size_t short_max = CHORUS_V2_MAX_SHORT_TEXT;
    const size_t long_max = CHORUS_V2_MAX_LONG_TEXT;
    switch (message->type) {
    case CHORUS_V2_TIME_SYNC:
    case CHORUS_V2_AUDIO_CHUNK:
    case CHORUS_V2_STREAM_END:
        /* The v1 encoders and decoder hold these to v1's own rules. */
        return 0;
    case CHORUS_V2_HELLO: {
        const chorus_v2_hello_t *h = &message->as.hello;
        if (h->protocol_version == 0) {
            return fail(error, "protocol_version", CHORUS_V2_PROBLEM_OUT_OF_RANGE, NULL);
        }
        if ((h->roles & (uint16_t)~CHORUS_V2_ROLES_DEFINED) != 0) {
            return fail(error, "roles", CHORUS_V2_PROBLEM_UNDEFINED, NULL);
        }
        if (text_ok("name", &h->name, short_max, error) != 0) {
            return -1;
        }
        return text_ok("software", &h->software, short_max, error);
    }
    case CHORUS_V2_CAPABILITIES:
        return validate_capabilities(&message->as.capabilities, error);
    case CHORUS_V2_STREAM_FORMAT:
        return validate_stream_format(&message->as.stream_format, error);
    case CHORUS_V2_CODED_CHUNK: {
        const chorus_v2_coded_chunk_t *c = &message->as.coded_chunk;
        if (c->frames == 0) {
            return fail(error, "frames", CHORUS_V2_PROBLEM_OUT_OF_RANGE, NULL);
        }
        if (bytes_ok("data", &c->data, error) != 0) {
            return -1;
        }
        if (c->data.len == 0) {
            return fail(error, "data", CHORUS_V2_PROBLEM_INCONSISTENT,
                        "a packet carries at least one byte");
        }
        return 0;
    }
    case CHORUS_V2_OUTPUT_DELAY:
        if (message->as.output_delay.delay_ns > CHORUS_V2_MAX_OUTPUT_DELAY_NS) {
            return fail(error, "delay_ns", CHORUS_V2_PROBLEM_OUT_OF_RANGE, NULL);
        }
        return 0;
    case CHORUS_V2_TELEMETRY:
        return enum_ok("link", CHORUS_V2_ENUM_LINK, message->as.telemetry.link, error);
    case CHORUS_V2_HANDSHAKE_INIT: {
        const chorus_v2_handshake_init_t *h = &message->as.handshake_init;
        if (enum_ok("suite", CHORUS_V2_ENUM_SUITE, h->suite, error) != 0 ||
            bytes_ok("noise", &h->noise, error) != 0) {
            return -1;
        }
        if (h->noise.len < CHORUS_V2_KEY_LEN) {
            return fail(error, "noise", CHORUS_V2_PROBLEM_TRUNCATED, NULL);
        }
        return 0;
    }
    case CHORUS_V2_HANDSHAKE_RESPONSE:
    case CHORUS_V2_HANDSHAKE_FINISH: {
        const chorus_v2_bytes_t *noise = (message->type == CHORUS_V2_HANDSHAKE_RESPONSE)
                                             ? &message->as.handshake_response.noise
                                             : &message->as.handshake_finish.noise;
        if (bytes_ok("noise", noise, error) != 0) {
            return -1;
        }
        if (noise->len < chorus_v2_min_payload_len(message->type)) {
            return fail(error, "noise", CHORUS_V2_PROBLEM_TRUNCATED, NULL);
        }
        return 0;
    }
    case CHORUS_V2_SESSION_REFUSED: {
        const chorus_v2_session_refused_t *s = &message->as.session_refused;
        if (enum_ok("reason", CHORUS_V2_ENUM_REFUSAL_REASON, s->reason, error) != 0) {
            return -1;
        }
        return text_ok("detail", &s->detail, long_max, error);
    }
    case CHORUS_V2_SECURE_RECORD: {
        const chorus_v2_bytes_t *c = &message->as.secure_record.ciphertext;
        if (bytes_ok("ciphertext", c, error) != 0) {
            return -1;
        }
        if (c->len < chorus_v2_min_payload_len(CHORUS_V2_SECURE_RECORD)) {
            return fail(error, "ciphertext", CHORUS_V2_PROBLEM_TRUNCATED, NULL);
        }
        return 0;
    }
    case CHORUS_V2_METADATA: {
        const chorus_v2_metadata_t *d = &message->as.metadata;
        if (enum_ok("playback", CHORUS_V2_ENUM_PLAYBACK, d->playback, error) != 0 ||
            text_ok("title", &d->title, long_max, error) != 0 ||
            text_ok("artist", &d->artist, long_max, error) != 0 ||
            text_ok("album", &d->album, long_max, error) != 0) {
            return -1;
        }
        return text_ok("source", &d->source, long_max, error);
    }
    case CHORUS_V2_ARTWORK:
        return validate_artwork(&message->as.artwork, error);
    case CHORUS_V2_CONTROLLER_COMMAND:
        return validate_command(&message->as.controller_command, error);
    case CHORUS_V2_CONTROLLER_STATE: {
        const chorus_v2_controller_state_t *s = &message->as.controller_state;
        if (s->muted > 1) {
            return fail(error, "muted", CHORUS_V2_PROBLEM_UNDEFINED, NULL);
        }
        if (enum_ok("playback", CHORUS_V2_ENUM_PLAYBACK, s->playback, error) != 0) {
            return -1;
        }
        if (s->volume > 100) {
            return fail(error, "volume", CHORUS_V2_PROBLEM_OUT_OF_RANGE, NULL);
        }
        return text_ok("group", &s->group, short_max, error);
    }
    case CHORUS_V2_VISUALIZER_FRAME: {
        const chorus_v2_bytes_t *bands = &message->as.visualizer_frame.bands;
        if (bytes_ok("bands", bands, error) != 0) {
            return -1;
        }
        if (bands->len > CHORUS_V2_MAX_VISUALIZER_BANDS) {
            return fail(error, "bands", CHORUS_V2_PROBLEM_TOO_LONG, NULL);
        }
        return 0;
    }
    case CHORUS_V2_COLOR:
        return 0;
    case CHORUS_V2_SOURCE_OFFER: {
        const chorus_v2_source_offer_t *s = &message->as.source_offer;
        if (enum_ok("kind", CHORUS_V2_ENUM_SOURCE_KIND, s->kind, error) != 0) {
            return -1;
        }
        if (s->signal > 1) {
            return fail(error, "signal", CHORUS_V2_PROBLEM_UNDEFINED, NULL);
        }
        return text_ok("name", &s->name, short_max, error);
    }
    case CHORUS_V2_SOURCE_CONTROL: {
        const chorus_v2_source_control_t *s = &message->as.source_control;
        if (enum_ok("action", CHORUS_V2_ENUM_SOURCE_ACTION, s->action, error) != 0) {
            return -1;
        }
        return enum_ok("codec", CHORUS_V2_ENUM_CODEC, s->codec, error);
    }
    case CHORUS_V2_ROOM_VOLUME: {
        /* Rejected, never clamped: a decoder that clamped an out-of-range
         * limit would be choosing a limit nobody sent. */
        const chorus_v2_room_volume_t *v = &message->as.room_volume;
        if (v->gain > CHORUS_V2_ROOM_VOLUME_FULL) {
            return fail(error, "gain", CHORUS_V2_PROBLEM_OUT_OF_RANGE, NULL);
        }
        if (v->limit > CHORUS_V2_ROOM_VOLUME_FULL) {
            return fail(error, "limit", CHORUS_V2_PROBLEM_OUT_OF_RANGE, NULL);
        }
        if (v->ramp_ms > CHORUS_V2_MAX_ROOM_VOLUME_RAMP_MS) {
            return fail(error, "ramp_ms", CHORUS_V2_PROBLEM_OUT_OF_RANGE, NULL);
        }
        return 0;
    }
    case CHORUS_V2_SOUND:
        return validate_sound(&message->as.sound, error);
    default:
        return fail(error, "type", CHORUS_V2_PROBLEM_UNDEFINED, NULL);
    }
}

/* sound's rules, field by field in wire order, the order
 * crates/protocol/src/v2/codec.rs checks them in, so both name the same
 * field for a message that breaks two. Rejected, never clamped. */
static int validate_sound(const chorus_v2_sound_t *s, chorus_v2_field_error_t *error)
{
    if (s->bass_db < CHORUS_V2_SOUND_TONE_DB_MIN || s->bass_db > CHORUS_V2_SOUND_TONE_DB_MAX) {
        return fail(error, "bass_db", CHORUS_V2_PROBLEM_OUT_OF_RANGE, NULL);
    }
    if (s->treble_db < CHORUS_V2_SOUND_TONE_DB_MIN || s->treble_db > CHORUS_V2_SOUND_TONE_DB_MAX) {
        return fail(error, "treble_db", CHORUS_V2_PROBLEM_OUT_OF_RANGE, NULL);
    }
    if ((s->flags & (uint8_t)~CHORUS_V2_SOUND_FLAGS_DEFINED) != 0) {
        return fail(error, "flags", CHORUS_V2_PROBLEM_UNDEFINED, NULL);
    }
    if (!defined(CHORUS_V2_ENUM_CHANNEL_POSITION, s->role)) {
        return fail(error, "role", CHORUS_V2_PROBLEM_UNDEFINED, NULL);
    }
    if (s->sub_present > 1) {
        return fail(error, "sub_present", CHORUS_V2_PROBLEM_UNDEFINED, NULL);
    }
    if (s->crossover_hz < CHORUS_V2_SOUND_CROSSOVER_HZ_MIN ||
        s->crossover_hz > CHORUS_V2_SOUND_CROSSOVER_HZ_MAX) {
        return fail(error, "crossover_hz", CHORUS_V2_PROBLEM_OUT_OF_RANGE, NULL);
    }
    if (s->sub_level_cdb < CHORUS_V2_SOUND_SUB_LEVEL_CDB_MIN ||
        s->sub_level_cdb > CHORUS_V2_SOUND_SUB_LEVEL_CDB_MAX) {
        return fail(error, "sub_level_cdb", CHORUS_V2_PROBLEM_OUT_OF_RANGE, NULL);
    }
    if (s->eq_count > CHORUS_V2_SOUND_EQ_MAX_FILTERS) {
        return fail(error, "eq_count", CHORUS_V2_PROBLEM_OUT_OF_RANGE, NULL);
    }
    for (uint8_t i = 0; i < s->eq_count; i++) {
        const chorus_v2_sound_filter_t *f = &s->filters[i];
        if (f->freq_hz < CHORUS_V2_SOUND_EQ_FREQ_HZ_MIN ||
            f->freq_hz > CHORUS_V2_SOUND_EQ_FREQ_HZ_MAX) {
            return fail(error, "freq_hz", CHORUS_V2_PROBLEM_OUT_OF_RANGE, NULL);
        }
        if (f->gain_cdb < CHORUS_V2_SOUND_EQ_GAIN_CDB_MIN ||
            f->gain_cdb > CHORUS_V2_SOUND_EQ_GAIN_CDB_MAX) {
            return fail(error, "gain_cdb", CHORUS_V2_PROBLEM_OUT_OF_RANGE, NULL);
        }
        if (f->q_milli < CHORUS_V2_SOUND_EQ_Q_MILLI_MIN ||
            f->q_milli > CHORUS_V2_SOUND_EQ_Q_MILLI_MAX) {
            return fail(error, "q_milli", CHORUS_V2_PROBLEM_OUT_OF_RANGE, NULL);
        }
    }
    return 0;
}

/* --- decoding --------------------------------------------------------------- */

chorus_v2_frame_t chorus_v2_decode_frame(const uint8_t *buf, size_t len)
{
    chorus_v2_frame_t frame;
    memset(&frame, 0, sizeof(frame));

    /* 1. A whole header? */
    if (len < CHORUS_FRAME_HEADER_LEN) {
        frame.outcome = CHORUS_FRAME_TRUNCATED_HEADER;
        return frame;
    }
    uint8_t type_byte = buf[0];
    size_t declared = ((size_t)buf[1] << 8) | (size_t)buf[2];
    size_t available = len - CHORUS_FRAME_HEADER_LEN;
    frame.message_type = type_byte;
    frame.payload_len = declared;

    /* 2. Does the declared length fit? Checked before any payload is
     *    sliced. */
    if (declared > available) {
        frame.outcome = CHORUS_FRAME_DECLARED_LENGTH_EXCEEDS_BUFFER;
        return frame;
    }
    size_t frame_len = CHORUS_FRAME_HEADER_LEN + declared;
    frame.consumed = frame_len;

    /* 3. Unknown type: step over it. */
    size_t minimum = chorus_v2_min_payload_len(type_byte);
    if (minimum == 0) {
        frame.outcome = CHORUS_FRAME_SKIPPED_UNKNOWN_TYPE;
        return frame;
    }

    /* 4. Long enough for its type? */
    if (declared < minimum) {
        frame.outcome = CHORUS_FRAME_PAYLOAD_TOO_SHORT_FOR_TYPE;
        return frame;
    }

    /* 5. Field values. The v1 types are v1's own decoder, unchanged. */
    if (is_v1(type_byte)) {
        chorus_frame_t v1 = chorus_decode_frame(buf, frame_len);
        frame.outcome = v1.outcome;
        frame.message.type = type_byte;
        if (v1.outcome == CHORUS_FRAME_INVALID_FIELD) {
            frame.v1_invalid_field = v1.invalid_field;
            frame.error.field = "v1 field";
            frame.error.problem = CHORUS_V2_PROBLEM_INCONSISTENT;
            frame.error.why = "a v1 field the v1 decoder rejects";
        } else if (type_byte == CHORUS_V2_TIME_SYNC) {
            frame.message.as.time_sync = v1.message.time_sync;
        } else if (type_byte == CHORUS_V2_AUDIO_CHUNK) {
            frame.message.as.audio_chunk = v1.message.audio_chunk;
        } else {
            frame.message.as.stream_end = v1.message.stream_end;
        }
        return frame;
    }

    const uint8_t *payload = buf + CHORUS_FRAME_HEADER_LEN;
    if (decode_payload(type_byte, payload, declared, &frame.message, &frame.error) != 0 ||
        chorus_v2_validate(&frame.message, &frame.error) != 0) {
        frame.outcome = CHORUS_FRAME_INVALID_FIELD;
        return frame;
    }
    frame.outcome = CHORUS_FRAME_DECODED;
    return frame;
}

/* --- encoding --------------------------------------------------------------- */

typedef struct {
    uint8_t *out;
    size_t cap;
    size_t at;
} writer_t;

/* A writer that overflows keeps counting and writes nothing more, so the
 * caller learns the size it needed and nothing past `cap` is touched. */
static void put(writer_t *w, const uint8_t *bytes, size_t n)
{
    if (w->at <= w->cap && w->cap - w->at >= n) {
        if (n > 0) {
            memcpy(w->out + w->at, bytes, n);
        }
    } else {
        w->cap = 0;
    }
    w->at += n;
}

static void put_u8(writer_t *w, uint8_t v)
{
    put(w, &v, 1);
}

static void put_u16(writer_t *w, uint16_t v)
{
    uint8_t b[2] = {(uint8_t)(v >> 8), (uint8_t)(v & 0xFF)};
    put(w, b, 2);
}

static void put_u32(writer_t *w, uint32_t v)
{
    uint8_t b[4] = {(uint8_t)(v >> 24), (uint8_t)((v >> 16) & 0xFF), (uint8_t)((v >> 8) & 0xFF),
                    (uint8_t)(v & 0xFF)};
    put(w, b, 4);
}

static void put_u64(writer_t *w, uint64_t v)
{
    uint8_t b[8];
    for (int i = 0; i < 8; i++) {
        b[i] = (uint8_t)((v >> (56 - 8 * i)) & 0xFF);
    }
    put(w, b, 8);
}

static void put_bytes(writer_t *w, const chorus_v2_bytes_t *b)
{
    put(w, b->data, b->len);
}

/* Lengths were checked by chorus_v2_validate before anything is written. */
static void put_short_text(writer_t *w, const chorus_v2_bytes_t *t)
{
    put_u8(w, (uint8_t)t->len);
    put_bytes(w, t);
}

static void put_long_text(writer_t *w, const chorus_v2_bytes_t *t)
{
    put_u16(w, (uint16_t)t->len);
    put_bytes(w, t);
}

static void write_payload(writer_t *w, const chorus_v2_message_t *m)
{
    switch (m->type) {
    case CHORUS_V2_HELLO:
        put_u16(w, m->as.hello.protocol_version);
        put_u16(w, m->as.hello.roles);
        put_short_text(w, &m->as.hello.name);
        put_short_text(w, &m->as.hello.software);
        break;
    case CHORUS_V2_CAPABILITIES: {
        const chorus_v2_capabilities_t *c = &m->as.capabilities;
        put_u8(w, c->codecs);
        put_u8(w, c->sample_formats);
        put_u8(w, c->max_channels);
        put_u8(w, (uint8_t)c->rate_count);
        for (size_t i = 0; i < c->rate_count; i++) {
            put_u32(w, c->sample_rates_hz[i]);
        }
        put_u16(w, c->buffer_ms);
        put_u32(w, c->intrinsic_latency_ns);
        put_u16(w, c->led_count);
        put_u8(w, c->visualizer_bands);
        break;
    }
    case CHORUS_V2_STREAM_FORMAT: {
        const chorus_v2_stream_format_t *f = &m->as.stream_format;
        put_u8(w, f->codec);
        put_u8(w, f->sample_format);
        put_u32(w, f->sample_rate_hz);
        put_u8(w, (uint8_t)f->channels);
        put(w, f->channel_map, f->channels);
        put_u32(w, f->frames_per_chunk);
        put_u16(w, (uint16_t)f->codec_config.len);
        put_bytes(w, &f->codec_config);
        break;
    }
    case CHORUS_V2_CODED_CHUNK:
        put_u32(w, m->as.coded_chunk.sequence);
        put_u64(w, m->as.coded_chunk.timestamp_ns);
        put_u32(w, m->as.coded_chunk.frames);
        put_bytes(w, &m->as.coded_chunk.data);
        break;
    case CHORUS_V2_OUTPUT_DELAY:
        put_u64(w, m->as.output_delay.delay_ns);
        break;
    case CHORUS_V2_TELEMETRY: {
        const chorus_v2_telemetry_t *t = &m->as.telemetry;
        put_u64(w, t->taken_ns);
        put_u64(w, (uint64_t)t->sync_error_ns);
        put_u32(w, t->buffer_fill_us);
        put_u32(w, t->underruns);
        put_u32(w, t->resyncs);
        put_u32(w, (uint32_t)t->correction_ppb);
        put_u8(w, t->link);
        put_u8(w, (uint8_t)t->rssi_dbm);
        put_u16(w, (uint16_t)t->temperature_centi_c);
        break;
    }
    case CHORUS_V2_HANDSHAKE_INIT:
        put(w, CHORUS_V2_MAGIC, CHORUS_V2_MAGIC_LEN);
        put_u16(w, m->as.handshake_init.protocol_version);
        put_u8(w, m->as.handshake_init.suite);
        put_bytes(w, &m->as.handshake_init.noise);
        break;
    case CHORUS_V2_HANDSHAKE_RESPONSE:
        put_bytes(w, &m->as.handshake_response.noise);
        break;
    case CHORUS_V2_HANDSHAKE_FINISH:
        put_bytes(w, &m->as.handshake_finish.noise);
        break;
    case CHORUS_V2_SESSION_REFUSED:
        put_u8(w, m->as.session_refused.reason);
        put_long_text(w, &m->as.session_refused.detail);
        break;
    case CHORUS_V2_SECURE_RECORD:
        put_bytes(w, &m->as.secure_record.ciphertext);
        break;
    case CHORUS_V2_METADATA: {
        const chorus_v2_metadata_t *d = &m->as.metadata;
        put_u8(w, d->playback);
        put_u32(w, d->position_ms);
        put_u32(w, d->duration_ms);
        put_u64(w, d->position_at_ns);
        put_u32(w, d->artwork_id);
        put_long_text(w, &d->title);
        put_long_text(w, &d->artist);
        put_long_text(w, &d->album);
        put_long_text(w, &d->source);
        break;
    }
    case CHORUS_V2_ARTWORK:
        put_u32(w, m->as.artwork.artwork_id);
        put_u32(w, m->as.artwork.total_len);
        put_u32(w, m->as.artwork.offset);
        put_short_text(w, &m->as.artwork.mime);
        put_bytes(w, &m->as.artwork.data);
        break;
    case CHORUS_V2_CONTROLLER_COMMAND:
        put_u8(w, m->as.controller_command.command);
        put_u16(w, (uint16_t)m->as.controller_command.value);
        put_short_text(w, &m->as.controller_command.target);
        break;
    case CHORUS_V2_CONTROLLER_STATE:
        put_u8(w, m->as.controller_state.volume);
        put_u8(w, m->as.controller_state.muted);
        put_u8(w, m->as.controller_state.playback);
        put_short_text(w, &m->as.controller_state.group);
        break;
    case CHORUS_V2_VISUALIZER_FRAME:
        put_u64(w, m->as.visualizer_frame.timestamp_ns);
        put_u8(w, m->as.visualizer_frame.beat);
        put_u8(w, m->as.visualizer_frame.peak);
        put_u8(w, (uint8_t)m->as.visualizer_frame.bands.len);
        put_bytes(w, &m->as.visualizer_frame.bands);
        break;
    case CHORUS_V2_COLOR:
        put_u64(w, m->as.color.timestamp_ns);
        put_u8(w, m->as.color.red);
        put_u8(w, m->as.color.green);
        put_u8(w, m->as.color.blue);
        put_u8(w, m->as.color.brightness);
        put_u16(w, m->as.color.transition_ms);
        break;
    case CHORUS_V2_SOURCE_OFFER:
        put_u8(w, m->as.source_offer.source_id);
        put_u8(w, m->as.source_offer.kind);
        put_u8(w, m->as.source_offer.signal);
        put_short_text(w, &m->as.source_offer.name);
        break;
    case CHORUS_V2_SOURCE_CONTROL:
        put_u8(w, m->as.source_control.source_id);
        put_u8(w, m->as.source_control.action);
        put_u8(w, m->as.source_control.codec);
        break;
    case CHORUS_V2_ROOM_VOLUME:
        put_u16(w, m->as.room_volume.gain);
        put_u16(w, m->as.room_volume.limit);
        put_u16(w, m->as.room_volume.ramp_ms);
        break;
    case CHORUS_V2_SOUND: {
        const chorus_v2_sound_t *s = &m->as.sound;
        put_u8(w, (uint8_t)s->bass_db);
        put_u8(w, (uint8_t)s->treble_db);
        put_u8(w, s->flags);
        put_u8(w, s->role);
        put_u8(w, s->sub_present);
        put_u16(w, s->crossover_hz);
        put_u16(w, (uint16_t)s->sub_level_cdb);
        put_u8(w, s->eq_count);
        for (uint8_t i = 0; i < s->eq_count && i < CHORUS_V2_SOUND_EQ_MAX_FILTERS; i++) {
            put_u16(w, s->filters[i].freq_hz);
            put_u16(w, (uint16_t)s->filters[i].gain_cdb);
            put_u16(w, s->filters[i].q_milli);
        }
        break;
    }
    default:
        break;
    }
}

chorus_encode_status_t chorus_v2_encode(const chorus_v2_message_t *message, uint8_t *out,
                                        size_t out_len, size_t *written,
                                        chorus_v2_field_error_t *error)
{
    switch (message->type) {
    case CHORUS_V2_TIME_SYNC:
        return chorus_encode_time_sync(&message->as.time_sync, out, out_len, written);
    case CHORUS_V2_AUDIO_CHUNK:
        return chorus_encode_audio_chunk(&message->as.audio_chunk, out, out_len, written);
    case CHORUS_V2_STREAM_END:
        return chorus_encode_stream_end(&message->as.stream_end, out, out_len, written);
    default:
        break;
    }
    if (chorus_v2_validate(message, error) != 0) {
        return CHORUS_ENCODE_INVALID_FIELD;
    }
    /* Measure first, so a payload too long for the frame or a buffer too
     * small for it is refused with nothing written. */
    writer_t measure = {NULL, 0, 0};
    write_payload(&measure, message);
    if (measure.at > CHORUS_MAX_PAYLOAD_LEN) {
        return CHORUS_ENCODE_PAYLOAD_TOO_LONG;
    }
    size_t total = CHORUS_FRAME_HEADER_LEN + measure.at;
    if (out_len < total) {
        return CHORUS_ENCODE_BUFFER_TOO_SMALL;
    }
    writer_t w = {out + CHORUS_FRAME_HEADER_LEN, measure.at, 0};
    write_payload(&w, message);
    out[0] = message->type;
    out[1] = (uint8_t)(measure.at >> 8);
    out[2] = (uint8_t)(measure.at & 0xFF);
    *written = total;
    return CHORUS_ENCODE_OK;
}
