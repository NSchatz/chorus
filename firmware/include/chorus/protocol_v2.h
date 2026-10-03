/* The endpoint's implementation of chorus protocol v2's catalog.
 *
 * `docs/protocol.md` ("Version 2") is the contract and the committed vectors
 * under `fixtures/protocol/v2/` are the authority. Like protocol.h, this is a
 * SECOND implementation of that contract, not a port of the Rust one
 * (`crates/protocol/src/v2/`): it is correct when it produces those bytes and
 * recovers those fields.
 *
 * v2 keeps v1's frame and v1's three messages byte for byte, so the v1 types
 * go through protocol.h's own decoder and encoders unchanged. The decoder's
 * checks run in the order docs/protocol.md "Decoder behaviour" makes the
 * contract; a type outside the v2 catalog is skipped by its length prefix.
 * One validation routine serves both directions, so the encoder refuses
 * exactly what the decoder rejects and valid encoder output always decodes.
 *
 * No allocation. Every variable-length field (a text, a list of bytes, a
 * codec setup, a ciphertext) is a pointer into the caller's buffer and a
 * length: a decoder hands back pointers into the frame it was given, and an
 * encoder reads from pointers it was given. Texts are not NUL-terminated. */

#ifndef CHORUS_PROTOCOL_V2_H
#define CHORUS_PROTOCOL_V2_H

#include <stddef.h>
#include <stdint.h>

#include "chorus/protocol.h"

/* The version this speaks, in handshake_init and hello. */
#define CHORUS_V2_PROTOCOL_VERSION 2u

/* ASCII CHRS, the first four bytes of every handshake_init. */
#define CHORUS_V2_MAGIC_LEN 4u
extern const uint8_t CHORUS_V2_MAGIC[CHORUS_V2_MAGIC_LEN];

/* Field limits, from docs/protocol.md. */
#define CHORUS_V2_MAX_SHORT_TEXT 255u
#define CHORUS_V2_MAX_LONG_TEXT 1024u
#define CHORUS_V2_MAX_OUTPUT_DELAY_NS 5000000000ull
#define CHORUS_V2_MAX_ARTWORK_LEN (4u * 1024u * 1024u)
#define CHORUS_V2_MAX_VISUALIZER_BANDS 64u
#define CHORUS_V2_MAX_RATES 16u
#define CHORUS_V2_FLAC_STREAMINFO_LEN 34u
#define CHORUS_V2_OPUS_HEAD_MIN_LEN 19u
/* room_volume: gain and limit in thousandths of full amplitude (the control
 * catalog's volume unit), and the longest ramp, in ms. */
#define CHORUS_V2_ROOM_VOLUME_FULL 1000u
#define CHORUS_V2_MAX_ROOM_VOLUME_RAMP_MS 60000u
/* sound (0x39): bass and treble in whole dB; the crossover in Hz; the sub's
 * level in hundredths of a dB; and the room-correction bounds, which are the
 * control catalog's ROOM_EQ_* (crates/control/src/sound.rs) and the DSP's
 * CHORUS_DSP_ROOM_EQ_*: frequency in Hz, gain in hundredths of a dB, Q in
 * thousandths. */
#define CHORUS_V2_SOUND_TONE_DB_MIN (-10)
#define CHORUS_V2_SOUND_TONE_DB_MAX 10
#define CHORUS_V2_SOUND_CROSSOVER_HZ_MIN 40u
#define CHORUS_V2_SOUND_CROSSOVER_HZ_MAX 200u
#define CHORUS_V2_SOUND_SUB_LEVEL_CDB_MIN (-1200)
#define CHORUS_V2_SOUND_SUB_LEVEL_CDB_MAX 600
#define CHORUS_V2_SOUND_EQ_MAX_FILTERS 8u
#define CHORUS_V2_SOUND_EQ_FREQ_HZ_MIN 20u
#define CHORUS_V2_SOUND_EQ_FREQ_HZ_MAX 1000u
#define CHORUS_V2_SOUND_EQ_GAIN_CDB_MIN (-1200)
#define CHORUS_V2_SOUND_EQ_GAIN_CDB_MAX 300
#define CHORUS_V2_SOUND_EQ_Q_MILLI_MIN 500u
#define CHORUS_V2_SOUND_EQ_Q_MILLI_MAX 10000u
/* sound's flags byte. Any other bit set is rejected (UNDEFINED). */
#define CHORUS_V2_SOUND_FLAG_LOUDNESS 0x01u
#define CHORUS_V2_SOUND_FLAG_NIGHT 0x02u
#define CHORUS_V2_SOUND_FLAG_SPEECH 0x04u
#define CHORUS_V2_SOUND_FLAG_ROOM_EQ 0x08u
#define CHORUS_V2_SOUND_FLAG_SUB_INVERTED 0x10u
#define CHORUS_V2_SOUND_FLAGS_DEFINED 0x1Fu
/* low_latency_offer (0x16, goal 13): fec_k is 0 (no FEC) or 2 to 16; the
 * interleave depth 1 (none) to 8, an ASSUMED bound (a decoder holds three
 * blocks of k x depth chunks); a chunk at most 700 frames, so a data plaintext
 * (32-byte header and PCM) fits a 1472-byte datagram beside the parity's
 * 8-byte header (chorus/lowlat.h); latency_ns at most output_delay's 5 s. */
#define CHORUS_V2_LOW_LATENCY_FEC_K_MIN 2u
#define CHORUS_V2_LOW_LATENCY_FEC_K_MAX 16u
#define CHORUS_V2_LOW_LATENCY_FEC_DEPTH_MIN 1u
#define CHORUS_V2_LOW_LATENCY_FEC_DEPTH_MAX 8u
#define CHORUS_V2_LOW_LATENCY_MAX_CHUNK_FRAMES 700u
#define CHORUS_V2_LOW_LATENCY_MAX_LATENCY_NS CHORUS_V2_MAX_OUTPUT_DELAY_NS
/* low_latency_offer.direction and low_latency_accept.status. */
#define CHORUS_V2_LOW_LATENCY_END 0u
#define CHORUS_V2_LOW_LATENCY_TO_ENDPOINT 1u
#define CHORUS_V2_LOW_LATENCY_FROM_ENDPOINT 2u
#define CHORUS_V2_LOW_LATENCY_ACCEPTED 0u
#define CHORUS_V2_LOW_LATENCY_REFUSED_WIRELESS 1u
#define CHORUS_V2_LOW_LATENCY_REFUSED_NO_SOCKET 2u
#define CHORUS_V2_LOW_LATENCY_REFUSED_FEC 3u
/* capabilities' trailing features byte (goal 13). A bit no version defines is
 * kept, not rejected: a feature is something an endpoint can do, and a server
 * that does not know one never uses it. Absent on the wire reads as 0, and an
 * encoder writes the byte only when a bit is set. */
#define CHORUS_V2_FEATURE_LOW_LATENCY 0x01u
/* Goal 14: the endpoint takes a firmware image inside its session
 * (firmware_offer). An endpoint without the bit is never offered. */
#define CHORUS_V2_FEATURE_OTA 0x02u
/* The firmware messages' bounds (docs/protocol.md, "Firmware update"): the
 * most bytes in one firmware_chunk, the largest size an offer may announce
 * (a sanity bound, not a slot size), and the most bytes of a version or
 * board text. */
#define CHORUS_V2_FIRMWARE_MAX_CHUNK_BYTES 4096u
#define CHORUS_V2_FIRMWARE_MAX_SIZE (16u * 1024u * 1024u)
#define CHORUS_V2_FIRMWARE_MAX_TEXT 47u
#define CHORUS_V2_FIRMWARE_SLOT_UNKNOWN 255u
#define CHORUS_V2_SHA256_LEN 32u
/* Goal 13, sound's optional theater block after the filters: tv_upmix (0 off,
 * 1 ambient; past 1 rejected, UNDEFINED) and fold (what the set lacks; any
 * other bit rejected). Written only when one of them is not 0; a byte that is
 * absent decodes as 0. */
#define CHORUS_V2_SOUND_TV_UPMIX_MAX 1u
/* Goal 13, source_offer's optional reason. */
#define CHORUS_V2_SIGNAL_REASON_NONE 0u
#define CHORUS_V2_SIGNAL_REASON_STANDBY 1u
#define CHORUS_V2_SIGNAL_REASON_NON_PCM 2u
#define CHORUS_V2_SIGNAL_REASON_MAX 2u
#define CHORUS_V2_SOUND_FOLD_CENTRE 0x01u
#define CHORUS_V2_SOUND_FOLD_SURROUND 0x02u
#define CHORUS_V2_SOUND_FOLD_DEFINED 0x03u

/* The Noise and record sizes the frame layer knows about. */
#define CHORUS_V2_KEY_LEN 32u
#define CHORUS_V2_TAG_LEN 16u

/* Most plaintext one secure_record carries: a Noise message is at most 65535
 * bytes and the tag takes 16 of them. */
#define CHORUS_V2_MAX_RECORD_PLAINTEXT (CHORUS_MAX_PAYLOAD_LEN - CHORUS_V2_TAG_LEN)

/* The v2 catalog, by wire byte. 0x01 to 0x03 are v1's, unchanged. */
typedef enum {
    CHORUS_V2_TIME_SYNC = 0x01,
    CHORUS_V2_AUDIO_CHUNK = 0x02,
    CHORUS_V2_STREAM_END = 0x03,
    CHORUS_V2_HELLO = 0x10,
    CHORUS_V2_CAPABILITIES = 0x11,
    CHORUS_V2_STREAM_FORMAT = 0x12,
    CHORUS_V2_CODED_CHUNK = 0x13,
    CHORUS_V2_OUTPUT_DELAY = 0x14,
    CHORUS_V2_TELEMETRY = 0x15,
    CHORUS_V2_LOW_LATENCY_OFFER = 0x16,
    CHORUS_V2_LOW_LATENCY_ACCEPT = 0x17,
    CHORUS_V2_FIRMWARE_OFFER = 0x18,
    CHORUS_V2_FIRMWARE_CHUNK = 0x19,
    CHORUS_V2_FIRMWARE_STATUS = 0x1A,
    CHORUS_V2_HANDSHAKE_INIT = 0x20,
    CHORUS_V2_HANDSHAKE_RESPONSE = 0x21,
    CHORUS_V2_HANDSHAKE_FINISH = 0x22,
    CHORUS_V2_SESSION_REFUSED = 0x23,
    CHORUS_V2_SECURE_RECORD = 0x24,
    CHORUS_V2_METADATA = 0x30,
    CHORUS_V2_ARTWORK = 0x31,
    CHORUS_V2_CONTROLLER_COMMAND = 0x32,
    CHORUS_V2_CONTROLLER_STATE = 0x33,
    CHORUS_V2_VISUALIZER_FRAME = 0x34,
    CHORUS_V2_COLOR = 0x35,
    CHORUS_V2_SOURCE_OFFER = 0x36,
    CHORUS_V2_SOURCE_CONTROL = 0x37,
    CHORUS_V2_ROOM_VOLUME = 0x38,
    CHORUS_V2_SOUND = 0x39
} chorus_v2_type_t;

/* Short stable name of a catalogued type, or NULL for an unassigned byte. */
const char *chorus_v2_type_name(uint8_t wire);

/* The wire byte for a type name, or 0 when the name is not one. */
uint8_t chorus_v2_type_from_name(const char *name);

/* Smallest payload a frame of this type can carry, or 0 for an unassigned
 * byte. */
size_t chorus_v2_min_payload_len(uint8_t wire);

/* Whether a type travels in the clear, outside any secure_record: the
 * handshake, the refusal and the record envelope itself. */
int chorus_v2_type_is_plaintext(uint8_t wire);

/* The one-byte enumerations, and their stable names (the `.fields` files'). */
typedef enum {
    CHORUS_V2_ENUM_CODEC,
    CHORUS_V2_ENUM_CHANNEL_POSITION,
    CHORUS_V2_ENUM_REFUSAL_REASON,
    CHORUS_V2_ENUM_SUITE,
    CHORUS_V2_ENUM_PLAYBACK,
    CHORUS_V2_ENUM_COMMAND,
    CHORUS_V2_ENUM_SOURCE_KIND,
    CHORUS_V2_ENUM_SOURCE_ACTION,
    CHORUS_V2_ENUM_LINK,
    CHORUS_V2_ENUM_ROLE_BIT,
    CHORUS_V2_ENUM_LOW_LATENCY_DIRECTION,
    CHORUS_V2_ENUM_LOW_LATENCY_STATUS,
    CHORUS_V2_ENUM_FEATURE_BIT,
    CHORUS_V2_ENUM_FIRMWARE_STATE,
    CHORUS_V2_ENUM_FIRMWARE_REASON
} chorus_v2_enum_t;

/* The name of a value, or NULL when the value is not defined. For ROLE_BIT
 * the value is a bit index, 0 to 4; for FEATURE_BIT, 0 or 1. */
const char *chorus_v2_enum_name(chorus_v2_enum_t which, uint8_t value);

/* The value with this name. Returns 0 and sets *out, or -1. */
int chorus_v2_enum_from_name(chorus_v2_enum_t which, const char *name, uint8_t *out);

typedef enum {
    CHORUS_V2_CODEC_PCM = 1,
    CHORUS_V2_CODEC_FLAC = 2,
    CHORUS_V2_CODEC_OPUS = 3
} chorus_v2_codec_t;

/* A codec's bit in a capabilities codec set. */
#define CHORUS_V2_CODEC_BIT(codec) ((uint8_t)(1u << ((codec) - 1u)))

#define CHORUS_V2_POSITION_MONO 0u
#define CHORUS_V2_POSITION_MAX 18u

typedef enum {
    CHORUS_V2_REFUSED_PROTOCOL_VERSION = 1,
    CHORUS_V2_REFUSED_KEY_CHANGED = 2,
    CHORUS_V2_REFUSED_HANDSHAKE_FAILED = 3,
    CHORUS_V2_REFUSED_UNSUPPORTED_SUITE = 4,
    CHORUS_V2_REFUSED_SERVER_FULL = 5,
    CHORUS_V2_REFUSED_NOT_ADOPTED = 6
} chorus_v2_refusal_t;

#define CHORUS_V2_SUITE_NOISE_XX_25519_CHACHAPOLY_SHA256 1u

/* hello.roles bits. */
#define CHORUS_V2_ROLE_PLAYER (1u << 0)
#define CHORUS_V2_ROLE_METADATA (1u << 1)
#define CHORUS_V2_ROLE_CONTROLLER (1u << 2)
#define CHORUS_V2_ROLE_VISUALIZER (1u << 3)
#define CHORUS_V2_ROLE_SOURCE (1u << 4)
#define CHORUS_V2_ROLES_DEFINED 0x1Fu

/* A length and a pointer the struct does not own: a text (UTF-8, not
 * NUL-terminated) or bytes. */
typedef struct {
    const uint8_t *data;
    size_t len;
} chorus_v2_bytes_t;

typedef struct {
    uint16_t protocol_version;
    uint16_t roles;
    chorus_v2_bytes_t name;
    chorus_v2_bytes_t software;
} chorus_v2_hello_t;

typedef struct {
    uint8_t codecs;
    uint8_t sample_formats;
    uint8_t max_channels;
    /* How many rates the message lists. A decoder keeps the first
     * CHORUS_V2_MAX_RATES; a count above that is rejected as too long. */
    size_t rate_count;
    uint32_t sample_rates_hz[CHORUS_V2_MAX_RATES];
    uint16_t buffer_ms;
    uint32_t intrinsic_latency_ns;
    uint16_t led_count;
    uint8_t visualizer_bands;
    /* CHORUS_V2_FEATURE_* bits; 0 when the trailing byte is absent. */
    uint8_t features;
} chorus_v2_capabilities_t;

typedef struct {
    uint8_t codec;
    uint8_t sample_format;
    uint32_t sample_rate_hz;
    /* The channel count, which is the map's length. A decoder keeps the first
     * CHORUS_MAX_CHANNELS positions; a count above that is rejected. */
    size_t channels;
    uint8_t channel_map[CHORUS_MAX_CHANNELS];
    uint32_t frames_per_chunk;
    chorus_v2_bytes_t codec_config;
} chorus_v2_stream_format_t;

typedef struct {
    uint32_t sequence;
    uint64_t timestamp_ns;
    uint32_t frames;
    chorus_v2_bytes_t data;
} chorus_v2_coded_chunk_t;

typedef struct {
    uint64_t delay_ns;
} chorus_v2_output_delay_t;

typedef struct {
    uint64_t taken_ns;
    int64_t sync_error_ns;
    uint32_t buffer_fill_us;
    uint32_t underruns;
    uint32_t resyncs;
    int32_t correction_ppb;
    uint8_t link;
    int8_t rssi_dbm;
    int16_t temperature_centi_c;
    /* The optional heap block (goal 15): free heap now and the least since
     * boot, in bytes; CHORUS_V2_TELEMETRY_HEAP_UNKNOWN when not reported. The
     * encoder writes the block only when one of the two is known, so a
     * telemetry without heap keeps its 36 bytes. */
    uint32_t heap_free_bytes;
    uint32_t heap_min_free_bytes;
} chorus_v2_telemetry_t;

#define CHORUS_V2_TELEMETRY_HEAP_UNKNOWN UINT32_MAX
#define CHORUS_V2_TELEMETRY_HEAP_BLOCK_LEN 8u

/* The server offering a low-latency stream, or ending one (direction 0, and
 * then every field but stream_tag is zero). The key is as secret as the
 * session that carried it. docs/protocol.md, "Low-latency path". */
typedef struct {
    uint8_t direction;
    uint32_t stream_tag;
    uint8_t key[CHORUS_V2_KEY_LEN];
    uint16_t udp_port;
    uint32_t chunk_frames;
    uint8_t fec_k;
    uint8_t fec_depth;
    uint64_t latency_ns;
} chorus_v2_low_latency_offer_t;

/* An endpoint's answer to an offer; a refusal names no port. */
typedef struct {
    uint32_t stream_tag;
    uint8_t status;
    uint16_t udp_port;
} chorus_v2_low_latency_accept_t;

typedef struct {
    uint16_t protocol_version;
    uint8_t suite;
    chorus_v2_bytes_t noise;
} chorus_v2_handshake_init_t;

/* handshake_response and handshake_finish: the Noise message, whole. */
typedef struct {
    chorus_v2_bytes_t noise;
} chorus_v2_handshake_message_t;

typedef struct {
    uint8_t reason;
    chorus_v2_bytes_t detail;
} chorus_v2_session_refused_t;

typedef struct {
    chorus_v2_bytes_t ciphertext;
} chorus_v2_secure_record_t;

typedef struct {
    uint8_t playback;
    uint32_t position_ms;
    uint32_t duration_ms;
    uint64_t position_at_ns;
    uint32_t artwork_id;
    chorus_v2_bytes_t title;
    chorus_v2_bytes_t artist;
    chorus_v2_bytes_t album;
    chorus_v2_bytes_t source;
} chorus_v2_metadata_t;

typedef struct {
    uint32_t artwork_id;
    uint32_t total_len;
    uint32_t offset;
    chorus_v2_bytes_t mime;
    chorus_v2_bytes_t data;
} chorus_v2_artwork_t;

typedef struct {
    uint8_t command;
    int16_t value;
    chorus_v2_bytes_t target;
} chorus_v2_controller_command_t;

typedef struct {
    uint8_t volume;
    uint8_t muted;
    uint8_t playback;
    chorus_v2_bytes_t group;
} chorus_v2_controller_state_t;

typedef struct {
    uint64_t timestamp_ns;
    uint8_t beat;
    uint8_t peak;
    chorus_v2_bytes_t bands;
} chorus_v2_visualizer_frame_t;

typedef struct {
    uint64_t timestamp_ns;
    uint8_t red;
    uint8_t green;
    uint8_t blue;
    uint8_t brightness;
    uint16_t transition_ms;
} chorus_v2_color_t;

typedef struct {
    uint8_t source_id;
    uint8_t kind;
    uint8_t signal;
    chorus_v2_bytes_t name;
    /* Goal 13: CHORUS_V2_SIGNAL_REASON_*, written only when not 0, after
     * name; absent decodes as 0. */
    uint8_t reason;
} chorus_v2_source_offer_t;

typedef struct {
    uint8_t source_id;
    uint8_t action;
    uint8_t codec;
} chorus_v2_source_control_t;

/* The room's gain and limit, server to player. Each field is checked on its
 * own: a gain above the limit is a valid message, which the player plays at
 * the limit (chorus/volume.h). */
typedef struct {
    uint16_t gain;
    uint16_t limit;
    uint16_t ramp_ms;
} chorus_v2_room_volume_t;

/* One room-correction filter in a sound: a peaking EQ. */
typedef struct {
    uint16_t freq_hz;
    int16_t gain_cdb;
    uint16_t q_milli;
} chorus_v2_sound_filter_t;

/* The room's sound settings, server to one player (docs/protocol.md, "0x39
 * sound"): tone, the flags (loudness, night, speech, room EQ on, sub
 * inverted), where this player sits in the room's bonded set (`role`, a
 * channel position, 0 when in no set) and whether the set has a sub, the
 * crossover and sub level, and up to eight correction filters. Fixed size, so
 * a session can keep the last one by value. */
typedef struct {
    int8_t bass_db;
    int8_t treble_db;
    uint8_t flags;
    uint8_t role;
    uint8_t sub_present;
    uint16_t crossover_hz;
    int16_t sub_level_cdb;
    uint8_t eq_count;
    chorus_v2_sound_filter_t filters[CHORUS_V2_SOUND_EQ_MAX_FILTERS];
    uint8_t tv_upmix; /* goal 13 */
    uint8_t fold;     /* goal 13, CHORUS_V2_SOUND_FOLD_* */
} chorus_v2_sound_t;

/* The server offering a firmware image, or (transfer 0, all else zero or
 * empty) cancelling the transfer in progress (docs/protocol.md, "0x18
 * firmware offer"). `sha256` is CHORUS_V2_SHA256_LEN bytes. */
typedef struct {
    uint32_t transfer;
    uint32_t size;
    uint8_t sha256[CHORUS_V2_SHA256_LEN];
    uint16_t chunk_bytes;
    chorus_v2_bytes_t version;
    chorus_v2_bytes_t board;
} chorus_v2_firmware_offer_t;

/* One piece of an offered image: the rest of the frame is the data. */
typedef struct {
    uint32_t transfer;
    uint32_t offset;
    chorus_v2_bytes_t data;
} chorus_v2_firmware_chunk_t;

/* An endpoint's firmware state. `state` and `reason` are the values of
 * chorus_ota_wire_state_t and chorus_ota_reason_t (chorus/ota.h), which are
 * the wire's. */
typedef struct {
    uint32_t transfer;
    uint8_t state;
    uint8_t reason;
    uint32_t received;
    chorus_v2_bytes_t version;
    chorus_v2_bytes_t board;
    uint8_t slot;
    chorus_v2_bytes_t image_version;
} chorus_v2_firmware_status_t;

/* Any message in the v2 catalog. `type` is the wire byte and selects the
 * member of `as`. */
typedef struct {
    uint8_t type;
    union {
        chorus_time_sync_t time_sync;
        chorus_audio_chunk_t audio_chunk;
        chorus_stream_end_t stream_end;
        chorus_v2_hello_t hello;
        chorus_v2_capabilities_t capabilities;
        chorus_v2_stream_format_t stream_format;
        chorus_v2_coded_chunk_t coded_chunk;
        chorus_v2_output_delay_t output_delay;
        chorus_v2_telemetry_t telemetry;
        chorus_v2_low_latency_offer_t low_latency_offer;
        chorus_v2_low_latency_accept_t low_latency_accept;
        chorus_v2_firmware_offer_t firmware_offer;
        chorus_v2_firmware_chunk_t firmware_chunk;
        chorus_v2_firmware_status_t firmware_status;
        chorus_v2_handshake_init_t handshake_init;
        chorus_v2_handshake_message_t handshake_response;
        chorus_v2_handshake_message_t handshake_finish;
        chorus_v2_session_refused_t session_refused;
        chorus_v2_secure_record_t secure_record;
        chorus_v2_metadata_t metadata;
        chorus_v2_artwork_t artwork;
        chorus_v2_controller_command_t controller_command;
        chorus_v2_controller_state_t controller_state;
        chorus_v2_visualizer_frame_t visualizer_frame;
        chorus_v2_color_t color;
        chorus_v2_source_offer_t source_offer;
        chorus_v2_source_control_t source_control;
        chorus_v2_room_volume_t room_volume;
        chorus_v2_sound_t sound;
    } as;
} chorus_v2_message_t;

/* What is wrong with a field. */
typedef enum {
    CHORUS_V2_PROBLEM_NONE = 0,
    /* An enumeration byte, bit or bool with no defined meaning. */
    CHORUS_V2_PROBLEM_UNDEFINED,
    /* A number outside the accepted range. */
    CHORUS_V2_PROBLEM_OUT_OF_RANGE,
    /* A length-prefixed field runs past the end of the payload. */
    CHORUS_V2_PROBLEM_TRUNCATED,
    /* Text that is not UTF-8. */
    CHORUS_V2_PROBLEM_NOT_UTF8,
    /* Text or a list longer than its field allows. */
    CHORUS_V2_PROBLEM_TOO_LONG,
    /* Fields that contradict each other; `why` says how. */
    CHORUS_V2_PROBLEM_INCONSISTENT
} chorus_v2_problem_t;

const char *chorus_v2_problem_name(chorus_v2_problem_t problem);

/* A field value that is not one the format accepts. `field` is the field's
 * name as docs/protocol.md gives it (the same name the Rust implementation
 * reports); `why` is a sentence for an INCONSISTENT problem, else NULL. */
typedef struct {
    const char *field;
    chorus_v2_problem_t problem;
    const char *why;
} chorus_v2_field_error_t;

/* One frame's outcome, as protocol.h's chorus_frame_t, for the v2 catalog.
 * `consumed` of 0 means the next frame boundary is not knowable from what is
 * present. `error` says which field an INVALID_FIELD outcome was about; for a
 * v1 type the v1 decoder's own `v1_invalid_field` says it too. */
typedef struct {
    chorus_frame_outcome_t outcome;
    size_t consumed;
    uint8_t message_type;
    size_t payload_len;
    chorus_v2_field_error_t error;
    chorus_invalid_field_t v1_invalid_field;
    chorus_v2_message_t message;
} chorus_v2_frame_t;

/* Decode the frame at the front of `buf`. Never reads past `len`, whatever
 * the length field claims; every pointer in the result points into `buf`. */
chorus_v2_frame_t chorus_v2_decode_frame(const uint8_t *buf, size_t len);

/* The value rules of docs/protocol.md, applied the same way before encoding
 * and after decoding. Returns 0 when the message is acceptable, or -1 with
 * `error` (which may be NULL) saying why. */
int chorus_v2_validate(const chorus_v2_message_t *message, chorus_v2_field_error_t *error);

/* Encode one message into a whole frame. Nothing is written unless the whole
 * frame is: `written` is set only on CHORUS_ENCODE_OK. A field the decoder
 * would reject is CHORUS_ENCODE_INVALID_FIELD with `error` (which may be
 * NULL) naming it. */
chorus_encode_status_t chorus_v2_encode(const chorus_v2_message_t *message, uint8_t *out,
                                        size_t out_len, size_t *written,
                                        chorus_v2_field_error_t *error);

/* Whether `len` bytes at `text` are UTF-8 as RFC 3629 defines it. */
int chorus_v2_is_utf8(const uint8_t *text, size_t len);

#endif /* CHORUS_PROTOCOL_V2_H */
