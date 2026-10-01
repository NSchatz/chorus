#include "chorus/session.h"

#include "chorus/codec.h"
#include "chorus/monotonic.h"
#include "chorus/noise.h"
#include "chorus/protocol.h"
#include "chorus/protocol_v2.h"

#include <errno.h>
#include <fcntl.h>
#include <netdb.h>
#include <netinet/in.h>
#include <netinet/tcp.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <sys/types.h>
#include <unistd.h>

/* One DMA-sized read at a time, and a buffer big enough that the largest frame
 * the protocol can carry (65538 bytes) always fits whole. Sized once here
 * rather than grown, because the endpoint has no heap on the audio path. A
 * record is opened IN PLACE, over its own ciphertext in this buffer (goal 8):
 * the PSA Crypto API lets an output buffer overlap an input buffer "with the
 * same result as if the buffers did not overlap" (PSA Certified Crypto API 1.2,
 * section 5.4.4, arm-software.github.io/psa-api/crypto/1.2/overview/
 * conventions.html, read 2026-09-30), and firmware/tests/test_noise.c checks
 * it. That frees the second 64 KiB buffer the jitter buffer needs. */
#define CHORUS_SESSION_BUFFER (CHORUS_FRAME_HEADER_LEN + CHORUS_MAX_PAYLOAD_LEN + 4096)

/* What this endpoint itself sends is small: a handshake message, a refusal
 * (at most a 1024-byte detail), and records carrying its greeting, a time-sync
 * request or its telemetry. One buffer of this size holds any of them, so the
 * only 64 KiB buffer is the one the receive side needs. */
#define CHORUS_SESSION_SEND_BUFFER 1280u

/* The pins this endpoint keeps: one per server id it has met. */
#define CHORUS_SESSION_MAX_PINS 8

/* What this endpoint says it is and what it can play. PCM only: the FLAC and
 * Opus decoders are a sibling track's, and advertising a codec with no
 * decoder behind it would be negotiating a stream this endpoint cannot play.
 * The rates, formats and channels are what its I2S path is configured for
 * (firmware/config/endpoint.conf); the buffer is the playout path's jitter
 * buffer (chorus/playout.h) and, like the intrinsic latency, ASSUMED until the
 * output stage is measured on a bench. */
#define CHORUS_SESSION_SOFTWARE "chorus-endpoint 0.2.0"
#define CHORUS_SESSION_BUFFER_MS CHORUS_PLAYOUT_BUFFER_MS
#define CHORUS_SESSION_INTRINSIC_LATENCY_NS 0u

/* About once a second, which is docs/protocol.md's intent for telemetry. */
#define CHORUS_SESSION_TELEMETRY_INTERVAL_NS 1000000000ull

const char *chorus_session_end_name(chorus_session_end_t end)
{
    switch (end) {
    case CHORUS_SESSION_RAN_ITS_TIME:
        return "ran-its-time";
    case CHORUS_SESSION_ADDRESS_UNUSABLE:
        return "address-unusable";
    case CHORUS_SESSION_STOPPED_ON_AMP_FAULT:
        return "stopped-on-amp-fault";
    case CHORUS_SESSION_LOG_UNWRITABLE:
        return "log-unwritable";
    case CHORUS_SESSION_IDENTITY_UNUSABLE:
        return "identity-unusable";
    case CHORUS_SESSION_SERVER_KEY_CHANGED:
        return "server-key-changed";
    }
    return "unknown";
}

typedef struct {
    int used;
    char id[CHORUS_SESSION_ID_MAX];
    uint8_t key[CHORUS_NOISE_KEY_LEN];
} pin_t;

typedef struct {
    FILE *log;
    chorus_telemetry_t telemetry;
    const chorus_session_config_t *config;
    chorus_session_result_t *out;
    const char *endpoint_id;
    chorus_noise_keypair_t identity;
    pin_t pins[CHORUS_SESSION_MAX_PINS];
} session_state_t;

/* The console's `status` reads what the run last published (audit A-13). */
static void hand_on_telemetry(const session_state_t *state)
{
    if (state->config->on_telemetry != NULL) {
        state->config->on_telemetry(state->config->telemetry_ctx, &state->telemetry);
    }
}

/* The playout path's counters into the published telemetry: what the DMA
 * consumed, not what arrived. */
static void refresh_playout(session_state_t *state)
{
    chorus_playout_t *playout = state->config->playout;
    if (playout == NULL) {
        return;
    }
    chorus_playout_stats_t st;
    chorus_playout_stats(playout, &st);
    state->telemetry.playout_attached = 1;
    state->telemetry.frames_played = st.frames_played;
    state->telemetry.underrun_frames = st.underrun_frames;
    state->telemetry.late_chunks = st.late_chunks;
    state->telemetry.correction_ppm = st.correction_ppm;
    state->telemetry.sync_error_known = st.error_known;
    state->telemetry.sync_error_ns = (int64_t)st.last_error_ns;
}

static void publish(session_state_t *state, const char *event)
{
    refresh_playout(state);
    hand_on_telemetry(state);
    char line[1024];
    chorus_telemetry_line(&state->telemetry, line, sizeof(line));
    if (state->log != NULL) {
        fprintf(state->log, "%s event=%s\n", line, event);
        fflush(state->log);
    }
}

/* An event with a sentence beside it, for the ones a person has to read:
 * a refusal, a pin, a key change. */
static void publish_detail(session_state_t *state, const char *event, const char *detail)
{
    refresh_playout(state);
    hand_on_telemetry(state);
    char line[1024];
    chorus_telemetry_line(&state->telemetry, line, sizeof(line));
    if (state->log != NULL) {
        fprintf(state->log, "%s event=%s detail=\"%s\"\n", line, event, detail);
        fflush(state->log);
    }
}

/* --- identity: the endpoint's key and its server pins ---------------------- */

static int unhex(const char *text, uint8_t *out, size_t len)
{
    for (size_t i = 0; i < len; i++) {
        unsigned value = 0;
        for (int k = 0; k < 2; k++) {
            char c = text[2 * i + (size_t)k];
            unsigned nibble;
            if (c >= '0' && c <= '9') {
                nibble = (unsigned)(c - '0');
            } else if (c >= 'a' && c <= 'f') {
                nibble = (unsigned)(c - 'a' + 10);
            } else if (c >= 'A' && c <= 'F') {
                nibble = (unsigned)(c - 'A' + 10);
            } else {
                return -1;
            }
            value = (value << 4) | nibble;
        }
        out[i] = (uint8_t)value;
    }
    return 0;
}

static void tohex(const uint8_t *bytes, size_t len, char *out)
{
    static const char HEX[] = "0123456789abcdef";
    for (size_t i = 0; i < len; i++) {
        out[2 * i] = HEX[bytes[i] >> 4];
        out[2 * i + 1] = HEX[bytes[i] & 0x0F];
    }
    out[2 * len] = '\0';
}

static int random_bytes(const chorus_session_config_t *config, uint8_t *out, size_t len)
{
    chorus_noise_random_fn source =
        (config->random != NULL) ? config->random : chorus_noise_system_random;
    return source(config->random_ctx, out, len);
}

/* Read the long-term key at `path`, or make one from the random source and
 * write it with mode 0600 if there is none. The secret is never printed. */
static int load_identity(session_state_t *state, char *detail, size_t detail_len)
{
    const chorus_session_config_t *config = state->config;
    uint8_t secret[CHORUS_NOISE_KEY_LEN];
    int have = 0;
    if (config->key_path != NULL) {
        FILE *file = fopen(config->key_path, "r");
        if (file != NULL) {
            char text[2 * CHORUS_NOISE_KEY_LEN + 8];
            size_t got = fread(text, 1, sizeof(text) - 1, file);
            fclose(file);
            text[got] = '\0';
            if (got < 2 * CHORUS_NOISE_KEY_LEN ||
                (got > 2 * CHORUS_NOISE_KEY_LEN && text[2 * CHORUS_NOISE_KEY_LEN] != '\n') ||
                unhex(text, secret, CHORUS_NOISE_KEY_LEN) != 0) {
                snprintf(detail, detail_len,
                         "%s is not a key: it must hold 64 hex digits and a newline",
                         config->key_path);
                return -1;
            }
            have = 1;
        } else if (errno != ENOENT) {
            snprintf(detail, detail_len, "%s could not be read: %s", config->key_path,
                     strerror(errno));
            return -1;
        }
    }
    if (!have) {
        if (random_bytes(config, secret, sizeof(secret)) != 0) {
            snprintf(detail, detail_len, "the random source gave no key");
            return -1;
        }
        if (config->key_path != NULL) {
            int fd = open(config->key_path, O_WRONLY | O_CREAT | O_EXCL, 0600);
            if (fd < 0) {
                snprintf(detail, detail_len, "%s could not be created: %s", config->key_path,
                         strerror(errno));
                return -1;
            }
            char text[2 * CHORUS_NOISE_KEY_LEN + 2];
            tohex(secret, sizeof(secret), text);
            text[2 * CHORUS_NOISE_KEY_LEN] = '\n';
            text[2 * CHORUS_NOISE_KEY_LEN + 1] = '\0';
            ssize_t wrote = write(fd, text, 2 * CHORUS_NOISE_KEY_LEN + 1);
            int synced = fsync(fd);
            close(fd);
            if (wrote != (ssize_t)(2 * CHORUS_NOISE_KEY_LEN + 1) || synced != 0) {
                snprintf(detail, detail_len, "%s could not be written", config->key_path);
                return -1;
            }
        }
    }
    chorus_noise_status_t status = chorus_noise_keypair_from_secret(secret, &state->identity);
    memset(secret, 0, sizeof(secret));
    if (status != CHORUS_NOISE_OK) {
        snprintf(detail, detail_len, "the key could not be used: %s",
                 chorus_noise_status_name(status));
        return -1;
    }
    chorus_noise_fingerprint(state->identity.public_key, state->out->key_fingerprint);
    return 0;
}

static int load_pins(session_state_t *state, char *detail, size_t detail_len)
{
    const char *path = state->config->server_pins_path;
    if (path == NULL) {
        return 0;
    }
    FILE *file = fopen(path, "r");
    if (file == NULL) {
        if (errno == ENOENT) {
            return 0;
        }
        snprintf(detail, detail_len, "%s could not be read: %s", path, strerror(errno));
        return -1;
    }
    char line[2 * CHORUS_NOISE_KEY_LEN + CHORUS_SESSION_ID_MAX + 32];
    size_t n = 0;
    int line_number = 0;
    int bad = 0;
    while (fgets(line, sizeof(line), file) != NULL) {
        line_number++;
        size_t len = strcspn(line, "\r\n");
        line[len] = '\0';
        if (len == 0 || line[0] == '#') {
            continue;
        }
        /* `pinned <64 hex digits> <id>`, the adoption store's text form. */
        const size_t key_at = 7;
        const size_t id_at = key_at + 2 * CHORUS_NOISE_KEY_LEN + 1;
        if (strncmp(line, "pinned ", key_at) != 0 || len <= id_at || line[id_at - 1] != ' ' ||
            n >= CHORUS_SESSION_MAX_PINS || len - id_at >= CHORUS_SESSION_ID_MAX ||
            unhex(line + key_at, state->pins[n].key, CHORUS_NOISE_KEY_LEN) != 0) {
            bad = 1;
            break;
        }
        state->pins[n].used = 1;
        memcpy(state->pins[n].id, line + id_at, len - id_at);
        state->pins[n].id[len - id_at] = '\0';
        n++;
    }
    fclose(file);
    if (bad) {
        snprintf(detail, detail_len,
                 "%s line %d is not `pinned <public key hex> <server id>` (or the store is full)",
                 path, line_number);
        return -1;
    }
    return 0;
}

/* Rewrite the store by writing a temporary beside it and renaming it over
 * the old one, so a reader never sees half a file. */
static int save_pins(const session_state_t *state)
{
    const char *path = state->config->server_pins_path;
    if (path == NULL) {
        return 0;
    }
    char temporary[512];
    if (snprintf(temporary, sizeof(temporary), "%s.writing", path) >= (int)sizeof(temporary)) {
        return -1;
    }
    FILE *file = fopen(temporary, "w");
    if (file == NULL) {
        return -1;
    }
    fprintf(file, "# chorus adopted peers: <pinned|removed> <public key hex> <id>\n");
    for (size_t i = 0; i < CHORUS_SESSION_MAX_PINS; i++) {
        if (state->pins[i].used) {
            char hex[2 * CHORUS_NOISE_KEY_LEN + 1];
            tohex(state->pins[i].key, CHORUS_NOISE_KEY_LEN, hex);
            fprintf(file, "pinned %s %s\n", hex, state->pins[i].id);
        }
    }
    int failed = ferror(file);
    if (fclose(file) != 0 || failed) {
        return -1;
    }
    return rename(temporary, path);
}

typedef enum {
    PIN_ADOPTED,
    PIN_KNOWN,
    PIN_CHANGED,
    PIN_UNSAVED
} pin_verdict_t;

static pin_verdict_t check_pin(session_state_t *state, const char *id,
                               const uint8_t key[CHORUS_NOISE_KEY_LEN], uint8_t pinned[32])
{
    for (size_t i = 0; i < CHORUS_SESSION_MAX_PINS; i++) {
        if (state->pins[i].used && strcmp(state->pins[i].id, id) == 0) {
            memcpy(pinned, state->pins[i].key, CHORUS_NOISE_KEY_LEN);
            return (memcmp(pinned, key, CHORUS_NOISE_KEY_LEN) == 0) ? PIN_KNOWN : PIN_CHANGED;
        }
    }
    for (size_t i = 0; i < CHORUS_SESSION_MAX_PINS; i++) {
        if (!state->pins[i].used) {
            state->pins[i].used = 1;
            snprintf(state->pins[i].id, sizeof(state->pins[i].id), "%s", id);
            memcpy(state->pins[i].key, key, CHORUS_NOISE_KEY_LEN);
            if (save_pins(state) != 0) {
                /* A pin that could not be kept is not an adoption: the next
                 * start would pin whatever key came first. */
                state->pins[i].used = 0;
                return PIN_UNSAVED;
            }
            return PIN_ADOPTED;
        }
    }
    return PIN_UNSAVED;
}

/* --- the socket ------------------------------------------------------------- */

/* Split "host:port". IPv6 in brackets is not accepted here and says so rather
 * than being parsed wrong: an address the endpoint cannot use is a refusal at
 * start, not a reconnect loop against nothing. */
static int split_address(const char *address, char *host, size_t host_len, char *port,
                         size_t port_len)
{
    const char *colon = strrchr(address, ':');
    if (colon == NULL || colon == address || colon[1] == '\0') {
        return -1;
    }
    size_t host_bytes = (size_t)(colon - address);
    if (host_bytes + 1 > host_len) {
        return -1;
    }
    memcpy(host, address, host_bytes);
    host[host_bytes] = '\0';
    if (strlen(colon + 1) + 1 > port_len) {
        return -1;
    }
    snprintf(port, port_len, "%s", colon + 1);
    return 0;
}

static int connect_once(const char *host, const char *port)
{
    struct addrinfo hints;
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_UNSPEC;
    hints.ai_socktype = SOCK_STREAM;

    struct addrinfo *results = NULL;
    if (getaddrinfo(host, port, &hints, &results) != 0) {
        return -1;
    }
    int fd = -1;
    for (struct addrinfo *entry = results; entry != NULL; entry = entry->ai_next) {
        fd = socket(entry->ai_family, entry->ai_socktype, entry->ai_protocol);
        if (fd < 0) {
            continue;
        }
        if (connect(fd, entry->ai_addr, entry->ai_addrlen) == 0) {
            break;
        }
        close(fd);
        fd = -1;
    }
    freeaddrinfo(results);
    if (fd >= 0) {
        int one = 1;
        (void)setsockopt(fd, IPPROTO_TCP, TCP_NODELAY, &one, sizeof(one));
        /* A read that blocks for ever is a supervisor that cannot service its
         * own exchange cadence or notice that its run is over. A timeout is
         * not a failure: it returns to the loop, which decides what to do. */
        struct timeval timeout;
        timeout.tv_sec = 0;
        timeout.tv_usec = 200000;
        (void)setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &timeout, sizeof(timeout));
    }
    return fd;
}

static int send_all(int fd, const uint8_t *bytes, size_t len)
{
    size_t sent = 0;
    while (sent < len) {
        ssize_t n = send(fd, bytes + sent, len - sent, 0);
        if (n <= 0) {
            if (n < 0 && errno == EINTR) {
                continue;
            }
            return -1;
        }
        sent += (size_t)n;
    }
    return 0;
}

/* The receive side: bytes off the socket, and the frame at their front. */
typedef struct {
    uint8_t *buf;
    size_t cap;
    size_t held;
} receive_t;

/* The length of the whole frame at the front, or 0 when it is not all here
 * yet. The buffer fits the largest frame, so "not all here" always means
 * "wait for more". */
static size_t whole_frame(const receive_t *rx)
{
    if (rx->held < CHORUS_FRAME_HEADER_LEN) {
        return 0;
    }
    size_t len = CHORUS_FRAME_HEADER_LEN + (((size_t)rx->buf[1] << 8) | rx->buf[2]);
    return (rx->held >= len) ? len : 0;
}

static void consume(receive_t *rx, size_t n)
{
    memmove(rx->buf, rx->buf + n, rx->held - n);
    rx->held -= n;
}

/* One read. 1 when bytes arrived, 0 when the read timed out, -1 when the peer
 * closed, -2 when the read failed. */
static int receive_some(int fd, receive_t *rx)
{
    ssize_t got = recv(fd, rx->buf + rx->held, rx->cap - rx->held, 0);
    if (got == 0) {
        return -1;
    }
    if (got < 0) {
        return (errno == EINTR || errno == EAGAIN || errno == EWOULDBLOCK) ? 0 : -2;
    }
    rx->held += (size_t)got;
    return 1;
}

/* --- the handshake ---------------------------------------------------------- */

typedef enum {
    HANDSHAKE_DONE,
    /* The link went away or never answered: an outage, so a reconnect. */
    HANDSHAKE_OUTAGE,
    /* The server refused this session, or spoke out of turn: logged by name,
     * and a reconnect. */
    HANDSHAKE_REFUSED,
    /* The server's key is not its pin: the run ends. */
    HANDSHAKE_KEY_CHANGED
} handshake_result_t;

static const uint8_t PROLOGUE[7] = {
    0x43, 0x48, 0x52, 0x53, 0x00, 0x02, CHORUS_V2_SUITE_NOISE_XX_25519_CHACHAPOLY_SHA256};

static int send_message(int fd, const chorus_v2_message_t *m)
{
    static uint8_t frame[CHORUS_SESSION_SEND_BUFFER];
    size_t len = 0;
    if (chorus_v2_encode(m, frame, sizeof(frame), &len, NULL) != CHORUS_ENCODE_OK) {
        return -1;
    }
    return send_all(fd, frame, len);
}

/* Send session_refused in the clear, best effort: the refusal is already
 * decided, and a peer that has gone away does not change it. */
static void refuse(int fd, uint8_t reason, const char *detail)
{
    chorus_v2_message_t m;
    memset(&m, 0, sizeof(m));
    m.type = CHORUS_V2_SESSION_REFUSED;
    m.as.session_refused.reason = reason;
    m.as.session_refused.detail.data = (const uint8_t *)detail;
    m.as.session_refused.detail.len = strlen(detail);
    if (m.as.session_refused.detail.len > CHORUS_V2_MAX_LONG_TEXT) {
        m.as.session_refused.detail.len = CHORUS_V2_MAX_LONG_TEXT;
    }
    (void)send_message(fd, &m);
}

/* Wait for one whole frame in the clear, until `deadline_ns`. Returns its
 * length, 0 at the deadline, or -1 when the link went away. */
static long await_frame(int fd, receive_t *rx, uint64_t deadline_ns)
{
    for (;;) {
        size_t len = whole_frame(rx);
        if (len > 0) {
            return (long)len;
        }
        if (chorus_monotonic_now_ns() >= deadline_ns) {
            return 0;
        }
        int got = receive_some(fd, rx);
        if (got < 0) {
            return -1;
        }
    }
}

/* A clear session_refused from the server, surfaced by name. Never a command:
 * nothing this endpoint holds changes because of it. */
static void surface_refusal(session_state_t *state, const chorus_v2_session_refused_t *r)
{
    char detail[CHORUS_V2_MAX_LONG_TEXT + 64];
    const char *reason = chorus_v2_enum_name(CHORUS_V2_ENUM_REFUSAL_REASON, r->reason);
    snprintf(detail, sizeof(detail), "the server refused the session (%s): %.*s",
             (reason == NULL) ? "?" : reason, (int)r->detail.len, (const char *)r->detail.data);
    state->out->refusals_received++;
    publish_detail(state, "refused-by-server", detail);
}

static handshake_result_t handshake(session_state_t *state, int fd, receive_t *rx,
                                    chorus_noise_transport_t *transport)
{
    const chorus_session_config_t *config = state->config;
    uint32_t timeout_ms =
        (config->handshake_timeout_ms == 0) ? 2000u : config->handshake_timeout_ms;
    uint64_t deadline_ns = chorus_monotonic_now_ns() + (uint64_t)timeout_ms * 1000000ull;

    uint8_t ephemeral_secret[CHORUS_NOISE_KEY_LEN];
    chorus_noise_keypair_t ephemeral;
    if (random_bytes(config, ephemeral_secret, sizeof(ephemeral_secret)) != 0 ||
        chorus_noise_keypair_from_secret(ephemeral_secret, &ephemeral) != CHORUS_NOISE_OK) {
        memset(ephemeral_secret, 0, sizeof(ephemeral_secret));
        publish_detail(state, "handshake-failed", "no ephemeral key from the random source");
        return HANDSHAKE_REFUSED;
    }
    memset(ephemeral_secret, 0, sizeof(ephemeral_secret));

    static chorus_noise_handshake_t hs;
    chorus_noise_handshake_init(&hs, 1, PROLOGUE, sizeof(PROLOGUE), &state->identity, &ephemeral);
    memset(&ephemeral, 0, sizeof(ephemeral));

    /* -> e */
    uint8_t noise[512];
    size_t noise_len = 0;
    chorus_v2_message_t m;
    memset(&m, 0, sizeof(m));
    if (chorus_noise_write_message(&hs, NULL, 0, noise, sizeof(noise), &noise_len) !=
        CHORUS_NOISE_OK) {
        chorus_noise_handshake_clear(&hs);
        publish_detail(state, "handshake-failed", "message 1 could not be written");
        return HANDSHAKE_REFUSED;
    }
    m.type = CHORUS_V2_HANDSHAKE_INIT;
    m.as.handshake_init.protocol_version = CHORUS_V2_PROTOCOL_VERSION;
    m.as.handshake_init.suite = CHORUS_V2_SUITE_NOISE_XX_25519_CHACHAPOLY_SHA256;
    m.as.handshake_init.noise.data = noise;
    m.as.handshake_init.noise.len = noise_len;
    if (send_message(fd, &m) != 0) {
        chorus_noise_handshake_clear(&hs);
        return HANDSHAKE_OUTAGE;
    }

    /* <- e, ee, s, es, or a refusal. */
    long len = await_frame(fd, rx, deadline_ns);
    if (len <= 0) {
        chorus_noise_handshake_clear(&hs);
        if (len == 0) {
            publish_detail(state, "no-v2-answer",
                           "the server did not answer the v2 handshake; it may speak chorus "
                           "protocol v1");
        }
        return HANDSHAKE_OUTAGE;
    }
    chorus_v2_frame_t frame = chorus_v2_decode_frame(rx->buf, (size_t)len);
    if (frame.outcome == CHORUS_FRAME_DECODED && frame.message_type == CHORUS_V2_SESSION_REFUSED) {
        surface_refusal(state, &frame.message.as.session_refused);
        consume(rx, (size_t)len);
        chorus_noise_handshake_clear(&hs);
        return HANDSHAKE_REFUSED;
    }
    if (frame.outcome != CHORUS_FRAME_DECODED ||
        frame.message_type != CHORUS_V2_HANDSHAKE_RESPONSE) {
        char detail[128];
        snprintf(detail, sizeof(detail), "expected handshake_response, got message type 0x%02x",
                 frame.message_type);
        refuse(fd, CHORUS_V2_REFUSED_HANDSHAKE_FAILED, detail);
        publish_detail(state, "handshake-failed", detail);
        chorus_noise_handshake_clear(&hs);
        return HANDSHAKE_REFUSED;
    }
    uint8_t id_payload[CHORUS_SESSION_ID_MAX + 1];
    size_t id_len = 0;
    chorus_noise_status_t status = chorus_noise_read_message(
        &hs, frame.message.as.handshake_response.noise.data,
        frame.message.as.handshake_response.noise.len, id_payload, sizeof(id_payload), &id_len);
    consume(rx, (size_t)len);
    if (status != CHORUS_NOISE_OK || id_len < 2 || (size_t)id_payload[0] != id_len - 1 ||
        !chorus_v2_is_utf8(id_payload + 1, id_len - 1)) {
        char detail[128];
        snprintf(detail, sizeof(detail), "message 2 is not a server id sealed to this key (%s)",
                 chorus_noise_status_name(status));
        refuse(fd, CHORUS_V2_REFUSED_HANDSHAKE_FAILED, detail);
        publish_detail(state, "handshake-failed", detail);
        chorus_noise_handshake_clear(&hs);
        return HANDSHAKE_REFUSED;
    }
    char server_id[CHORUS_SESSION_ID_MAX];
    memcpy(server_id, id_payload + 1, id_len - 1);
    server_id[id_len - 1] = '\0';

    /* The server's key against its pin: trust on first use, and a changed
     * key refused and surfaced, never re-pinned. */
    uint8_t pinned[CHORUS_NOISE_KEY_LEN];
    pin_verdict_t verdict = check_pin(state, server_id, hs.rs, pinned);
    char offered_fp[CHORUS_NOISE_FINGERPRINT_LEN];
    chorus_noise_fingerprint(hs.rs, offered_fp);
    if (verdict == PIN_CHANGED || verdict == PIN_UNSAVED) {
        char detail[CHORUS_SESSION_ID_MAX + 160];
        if (verdict == PIN_CHANGED) {
            char pinned_fp[CHORUS_NOISE_FINGERPRINT_LEN];
            chorus_noise_fingerprint(pinned, pinned_fp);
            snprintf(detail, sizeof(detail),
                     "%s presented key %s but key %s has been pinned since its adoption; refused",
                     server_id, offered_fp, pinned_fp);
            refuse(fd, CHORUS_V2_REFUSED_KEY_CHANGED, detail);
            publish_detail(state, "server-key-changed", detail);
            snprintf(state->out->detail, sizeof(state->out->detail), "%s", detail);
        } else {
            snprintf(detail, sizeof(detail),
                     "the pin of %s (key %s) could not be written to %s; refused", server_id,
                     offered_fp, config->server_pins_path);
            refuse(fd, CHORUS_V2_REFUSED_HANDSHAKE_FAILED, detail);
            publish_detail(state, "pin-not-persisted", detail);
        }
        chorus_noise_handshake_clear(&hs);
        return (verdict == PIN_CHANGED) ? HANDSHAKE_KEY_CHANGED : HANDSHAKE_REFUSED;
    }
    if (verdict == PIN_ADOPTED) {
        char detail[CHORUS_SESSION_ID_MAX + 64];
        snprintf(detail, sizeof(detail), "server pinned id=%s key=%s", server_id, offered_fp);
        state->out->servers_pinned++;
        publish_detail(state, "server-pinned", detail);
    }

    /* -> s, se with this endpoint's id. */
    size_t own_len = strlen(state->endpoint_id);
    id_payload[0] = (uint8_t)own_len;
    memcpy(id_payload + 1, state->endpoint_id, own_len);
    status =
        chorus_noise_write_message(&hs, id_payload, own_len + 1, noise, sizeof(noise), &noise_len);
    if (status == CHORUS_NOISE_OK) {
        status = chorus_noise_split(&hs, transport);
    }
    if (status != CHORUS_NOISE_OK) {
        chorus_noise_handshake_clear(&hs);
        publish_detail(state, "handshake-failed", chorus_noise_status_name(status));
        return HANDSHAKE_REFUSED;
    }
    memset(&m, 0, sizeof(m));
    m.type = CHORUS_V2_HANDSHAKE_FINISH;
    m.as.handshake_finish.noise.data = noise;
    m.as.handshake_finish.noise.len = noise_len;
    if (send_message(fd, &m) != 0) {
        chorus_noise_transport_clear(transport);
        return HANDSHAKE_OUTAGE;
    }
    state->out->handshakes++;
    return HANDSHAKE_DONE;
}

/* --- inside the session ----------------------------------------------------- */

/* Seal whole frames into one record and send it. */
static int send_sealed(session_state_t *state, int fd, chorus_noise_cipher_t *cipher,
                       const uint8_t *frames, size_t len)
{
    static uint8_t record[CHORUS_SESSION_SEND_BUFFER];
    size_t written = 0;
    if (chorus_noise_seal_record(cipher, frames, len, record, sizeof(record), &written) !=
        CHORUS_NOISE_OK) {
        return -1;
    }
    if (send_all(fd, record, written) != 0) {
        return -1;
    }
    state->out->records_sent++;
    return 0;
}

/* The first record: hello, then capabilities. */
static int send_greeting(session_state_t *state, int fd, chorus_noise_cipher_t *cipher)
{
    uint8_t frames[512];
    size_t at = 0;
    size_t len = 0;
    chorus_v2_message_t m;
    memset(&m, 0, sizeof(m));
    m.type = CHORUS_V2_HELLO;
    m.as.hello.protocol_version = CHORUS_V2_PROTOCOL_VERSION;
    m.as.hello.roles = CHORUS_V2_ROLE_PLAYER;
    /* Named after adoption, so the name is empty (docs/protocol.md). */
    m.as.hello.software.data = (const uint8_t *)CHORUS_SESSION_SOFTWARE;
    m.as.hello.software.len = strlen(CHORUS_SESSION_SOFTWARE);
    if (chorus_v2_encode(&m, frames, sizeof(frames), &len, NULL) != CHORUS_ENCODE_OK) {
        return -1;
    }
    at += len;
    memset(&m, 0, sizeof(m));
    m.type = CHORUS_V2_CAPABILITIES;
    /* PCM, and FLAC and Opus through firmware/src/codec.c (Opus in mapping
     * family 0, one or two channels, as max_channels says). */
    m.as.capabilities.codecs = CHORUS_V2_CODEC_BIT(CHORUS_V2_CODEC_PCM) |
                               CHORUS_V2_CODEC_BIT(CHORUS_V2_CODEC_FLAC) |
                               CHORUS_V2_CODEC_BIT(CHORUS_V2_CODEC_OPUS);
    m.as.capabilities.sample_formats =
        (uint8_t)((1u << (CHORUS_FMT_PCM_S16LE - 1)) | (1u << (CHORUS_FMT_PCM_S24LE - 1)));
    m.as.capabilities.max_channels = 2;
    m.as.capabilities.rate_count = 2;
    m.as.capabilities.sample_rates_hz[0] = 44100;
    m.as.capabilities.sample_rates_hz[1] = 48000;
    m.as.capabilities.buffer_ms = CHORUS_SESSION_BUFFER_MS;
    m.as.capabilities.intrinsic_latency_ns = CHORUS_SESSION_INTRINSIC_LATENCY_NS;
    if (chorus_v2_encode(&m, frames + at, sizeof(frames) - at, &len, NULL) != CHORUS_ENCODE_OK) {
        return -1;
    }
    at += len;
    return send_sealed(state, fd, cipher, frames, at);
}

/* Send one time-sync request with t0 stamped and the other three zero, which
 * is what docs/protocol.md says a client sends. */
static int send_time_sync_request(session_state_t *state, int fd, chorus_noise_cipher_t *cipher,
                                  uint64_t t0_ns)
{
    chorus_time_sync_t request;
    memset(&request, 0, sizeof(request));
    request.t0_ns = t0_ns;
    uint8_t frame[CHORUS_FRAME_HEADER_LEN + CHORUS_TIME_SYNC_PAYLOAD_LEN];
    size_t written = 0;
    if (chorus_encode_time_sync(&request, frame, sizeof(frame), &written) != CHORUS_ENCODE_OK) {
        return -1;
    }
    return send_sealed(state, fd, cipher, frame, written);
}

/* The endpoint's periodic report. What it does not know (a playout error before
 * the loop has formed one, the link, the radio, the temperature) it says it
 * does not know, rather than reporting zero. */
static int send_telemetry(session_state_t *state, int fd, chorus_noise_cipher_t *cipher)
{
    chorus_v2_message_t m;
    memset(&m, 0, sizeof(m));
    m.type = CHORUS_V2_TELEMETRY;
    m.as.telemetry.taken_ns = chorus_monotonic_now_ns();
    m.as.telemetry.sync_error_ns = INT64_MIN;
    if (state->config->playout != NULL) {
        chorus_playout_stats_t st;
        chorus_playout_stats(state->config->playout, &st);
        if (st.error_known) {
            /* The last error the playout loop formed from DMA-consumed frames
             * (firmware/src/playout.c): this endpoint's own view of its playout
             * against the timeline, not a measured inter-device error. */
            m.as.telemetry.sync_error_ns = (int64_t)st.last_error_ns;
        }
    }
    m.as.telemetry.link = 0; /* unknown */
    m.as.telemetry.rssi_dbm = INT8_MIN;
    m.as.telemetry.temperature_centi_c = INT16_MIN;
    uint8_t frame[64];
    size_t len = 0;
    if (chorus_v2_encode(&m, frame, sizeof(frame), &len, NULL) != CHORUS_ENCODE_OK) {
        return -1;
    }
    return send_sealed(state, fd, cipher, frame, len);
}

/* The most bytes one coded_chunk decodes to on this endpoint: an Opus packet
 * of 120 ms (5760 frames, more than FLAC's streamable-subset block of 4608 at
 * up to 48 kHz, RFC 9639 section 7) in two channels of pcm_s24le, the most
 * this endpoint's capabilities list. */
#define CHORUS_SESSION_DECODE_BYTES ((size_t)CHORUS_CODEC_OPUS_MAX_FRAMES * 2u * 3u)

typedef struct {
    int announced;
    chorus_v2_stream_format_t format;
    /* A FLAC or Opus stream's decoder, opened from its stream_format, and the
     * codec setup it was opened from (the frame it arrived in is gone). */
    chorus_decoder_t *decoder;
    uint8_t codec_config[64];
    uint64_t output_delay_ns;
    int exchange_outstanding;
    uint64_t pending_t0_ns;
    uint64_t next_exchange_ns;
    chorus_offset_filter_t *filter;
} stream_state_t;

/* One decoded frame from inside a record. Returns 0 to carry on, or -1 with
 * `why` set when the session has to end. */
static int handle_inner(session_state_t *state, stream_state_t *stream,
                        const chorus_v2_frame_t *frame, const char **why)
{
    const chorus_session_config_t *config = state->config;
    switch (frame->message_type) {
    case CHORUS_V2_AUDIO_CHUNK: {
        const chorus_audio_chunk_t *chunk = &frame->message.as.audio_chunk;
        /* Every chunk of a PCM stream agrees with the announcement's rate,
         * channel count and format; a disagreement is a framing error. */
        if (stream->announced && (stream->format.codec != CHORUS_V2_CODEC_PCM ||
                                  chunk->sample_rate_hz != stream->format.sample_rate_hz ||
                                  chunk->channels != stream->format.channels ||
                                  chunk->sample_format != stream->format.sample_format)) {
            *why = "chunk-disagrees-with-stream-format";
            return -1;
        }
        size_t sample_bytes = chorus_sample_format_bytes(chunk->sample_format);
        size_t frame_bytes = (size_t)chunk->channels * sample_bytes;
        size_t frames = (frame_bytes == 0) ? 0 : chunk->audio_data_len / frame_bytes;
        /* Counted as received; what plays is what the playout path's DMA
         * consumes (audit A-9, goal 8). */
        state->telemetry.chunks_received++;
        state->telemetry.frames_received += frames;
        if (config->playout != NULL) {
            (void)chorus_playout_offer(config->playout, chunk->timestamp_ns, chunk->sequence,
                                       chunk->sample_format, chunk->channels, chunk->sample_rate_hz,
                                       chunk->audio_data, (uint32_t)frames);
        }
        state->telemetry.last_sequence = chunk->sequence;
        state->telemetry.have_sequence = 1;
        if (state->telemetry.audio == CHORUS_AUDIO_IDLE) {
            state->telemetry.audio = CHORUS_AUDIO_RUNNING;
        }
        return 0;
    }
    case CHORUS_V2_TIME_SYNC: {
        if (!stream->exchange_outstanding) {
            return 0;
        }
        chorus_time_sync_t exchange = frame->message.as.time_sync;
        /* t3 is the endpoint's own receive stamp on the endpoint's clock. The
         * server cannot know it and one that invented it would be handing us
         * a round trip it made up. */
        exchange.t3_ns = chorus_monotonic_now_ns();
        if (exchange.t0_ns == stream->pending_t0_ns) {
            uint64_t rtt = chorus_time_sync_rtt_ns(&exchange);
            int64_t offset = chorus_time_sync_offset_ns(&exchange);
            double filtered = chorus_offset_filter_push(stream->filter, (double)exchange.t3_ns,
                                                        (double)rtt, (double)offset);
            const chorus_sample_t *selected = chorus_offset_filter_selected(stream->filter);
            state->telemetry.offset_known = 1;
            state->telemetry.offset_ns = (int64_t)filtered;
            state->telemetry.round_trip_ns = (selected == NULL) ? rtt : (uint64_t)selected->rtt_ns;
            /* Half the round trip of the sample the offset came FROM, which
             * RFC 5905 section 4 makes the bound on that offset. */
            state->telemetry.bound_ns = state->telemetry.round_trip_ns / 2;
            state->telemetry.exchanges++;
            if (config->playout != NULL) {
                /* The loop forms its error from this offset (playout.h). */
                chorus_playout_set_offset(config->playout, filtered, exchange.t3_ns);
            }
            stream->exchange_outstanding = 0;
            stream->next_exchange_ns =
                chorus_monotonic_now_ns() + (uint64_t)config->sync_interval_ms * 1000000ull;
        }
        return 0;
    }
    case CHORUS_V2_STREAM_FORMAT: {
        const chorus_v2_stream_format_t *f = &frame->message.as.stream_format;
        stream->format = *f;
        stream->format.codec_config.data = NULL;
        stream->announced = 1;
        if (config->playout != NULL) {
            /* A new stream starts from an empty buffer and a fresh loop, and
             * the sound chain (goal 12) from this stream's channel map. */
            chorus_playout_reset_stream(config->playout);
            uint8_t map[CHORUS_MAX_CHANNELS];
            size_t n = (f->channels < CHORUS_MAX_CHANNELS) ? f->channels : CHORUS_MAX_CHANNELS;
            memcpy(map, f->channel_map, n);
            chorus_playout_set_stream_layout(config->playout, (uint32_t)n, map);
        }
        chorus_codec_close(stream->decoder);
        stream->decoder = NULL;
        if (f->codec != CHORUS_V2_CODEC_PCM) {
            /* FLAC or Opus: the decoder is opened from this announcement
             * (firmware/include/chorus/codec.h). A stream it cannot decode is
             * one this endpoint cannot play, and it says so by name. */
            if (f->codec_config.len > sizeof(stream->codec_config)) {
                *why = "codec-setup-too-long";
                return -1;
            }
            memcpy(stream->codec_config, f->codec_config.data, f->codec_config.len);
            chorus_codec_stream_t setup = {f->codec,
                                           f->sample_format,
                                           f->sample_rate_hz,
                                           (uint8_t)f->channels,
                                           stream->codec_config,
                                           f->codec_config.len};
            char detail[160];
            if (chorus_codec_open(&stream->decoder, &setup, detail, sizeof(detail)) !=
                CHORUS_CODEC_OK) {
                *why = "codec-not-decodable";
                return -1;
            }
        }
        publish(state, "stream-format");
        return 0;
    }
    case CHORUS_V2_CODED_CHUNK: {
        /* One FLAC frame or Opus packet, decoded through the seam into the
         * same accounting and the same playout path a PCM chunk gets. */
        const chorus_v2_coded_chunk_t *chunk = &frame->message.as.coded_chunk;
        if (!stream->announced || stream->decoder == NULL) {
            *why = "coded-chunk-without-a-coded-stream-format";
            return -1;
        }
        static uint8_t pcm[CHORUS_SESSION_DECODE_BYTES];
        uint32_t got = 0;
        char detail[160];
        if (chorus_codec_decode(stream->decoder, chunk->data.data, chunk->data.len, chunk->frames,
                                pcm, sizeof(pcm), &got, detail,
                                sizeof(detail)) != CHORUS_CODEC_OK) {
            /* One bad frame or packet is one lost chunk, as a malformed PCM
             * chunk is; the decoder takes the next. */
            publish(state, "coded-chunk-refused");
            return 0;
        }
        state->telemetry.chunks_received++;
        state->telemetry.frames_received += got;
        if (config->playout != NULL) {
            (void)chorus_playout_offer(
                config->playout, chunk->timestamp_ns, chunk->sequence, stream->format.sample_format,
                (uint16_t)stream->format.channels, stream->format.sample_rate_hz, pcm, got);
        }
        state->telemetry.last_sequence = chunk->sequence;
        state->telemetry.have_sequence = 1;
        if (state->telemetry.audio == CHORUS_AUDIO_IDLE) {
            state->telemetry.audio = CHORUS_AUDIO_RUNNING;
        }
        return 0;
    }
    case CHORUS_V2_OUTPUT_DELAY:
        stream->output_delay_ns = frame->message.as.output_delay.delay_ns;
        return 0;
    case CHORUS_V2_ROOM_VOLUME:
        /* The room's gain and limit, enforced where the frames are written
         * (chorus/volume.h, ADR 0074): the decoder has already rejected a
         * value out of range, and the playout path clamps a gain above the
         * limit or the ceiling. It travels only inside a record, like every
         * frame here. */
        if (config->playout != NULL) {
            const chorus_v2_room_volume_t *rv = &frame->message.as.room_volume;
            chorus_playout_set_room_volume(config->playout, rv->gain, rv->limit, rv->ramp_ms);
        }
        return 0;
    case CHORUS_V2_SOUND: {
        /* The room's sound (goal 12): held to its ranges by the decoder,
         * kept as the last one received, and handed on. The playout path's
         * sound chain is configured from it (chorus/endpoint_dsp.h). */
        const chorus_v2_sound_t *sound = &frame->message.as.sound;
        chorus_session_keep_sound(state->out, sound);
        if (config->playout != NULL) {
            /* Into the playout path's sound chain, from the next frame
             * written (chorus/endpoint_dsp.h). */
            chorus_playout_set_sound(config->playout, sound);
        }
        if (config->on_sound != NULL) {
            config->on_sound(config->sound_ctx, sound);
        }
        char detail[160];
        snprintf(detail, sizeof(detail),
                 "bass_db=%d treble_db=%d flags=0x%02x role=%u sub_present=%u crossover_hz=%u "
                 "sub_level_cdb=%d eq_count=%u",
                 sound->bass_db, sound->treble_db, sound->flags, sound->role, sound->sub_present,
                 sound->crossover_hz, sound->sub_level_cdb, sound->eq_count);
        publish_detail(state, "sound", detail);
        return 0;
    }
    case CHORUS_V2_STREAM_END:
        publish(state, "stream-end");
        return 0;
    case CHORUS_V2_SESSION_REFUSED:
        surface_refusal(state, &frame->message.as.session_refused);
        *why = "refused-inside-the-session";
        return -1;
    default:
        /* The server's hello, and any role message this endpoint did not
         * declare, are stepped over: a peer ignores messages of a role it
         * did not declare (docs/protocol.md, "The four roles"). */
        return 0;
    }
}

/* Open one record and act on every frame in it. Returns 0, or -1 with `why`
 * set: a record that fails to decrypt, ends inside a frame, or carries a frame
 * that travels in the clear ends the session, because nothing after it can be
 * trusted to be aligned or authentic. */
static int take_record(session_state_t *state, stream_state_t *stream,
                       chorus_noise_cipher_t *receive, uint8_t *ciphertext, size_t len,
                       const char **why)
{
    /* Opened in place (see CHORUS_SESSION_BUFFER): the plaintext overwrites
     * the ciphertext it came from, which nothing reads again. */
    uint8_t *plain = ciphertext;
    size_t plain_len = 0;
    if (chorus_noise_open_record(receive, ciphertext, len, plain, len, &plain_len) !=
        CHORUS_NOISE_OK) {
        *why = "record-did-not-decrypt";
        return -1;
    }
    state->out->records_received++;
    size_t at = 0;
    while (at < plain_len) {
        chorus_v2_frame_t frame = chorus_v2_decode_frame(plain + at, plain_len - at);
        if (frame.consumed == 0) {
            *why = "record-ends-inside-a-frame";
            return -1;
        }
        at += frame.consumed;
        switch (frame.outcome) {
        case CHORUS_FRAME_DECODED:
            if (chorus_v2_type_is_plaintext(frame.message_type)) {
                *why = (frame.message_type == CHORUS_V2_SESSION_REFUSED)
                           ? NULL
                           : "clear-frame-inside-a-record";
                if (*why != NULL) {
                    return -1;
                }
            }
            if (handle_inner(state, stream, &frame, why) != 0) {
                return -1;
            }
            break;
        case CHORUS_FRAME_SKIPPED_UNKNOWN_TYPE:
            /* AC-7 and rule 3: inside a record the boundaries are known and
             * authenticated, so an unassigned type is stepped over. */
            state->telemetry.skipped_frames++;
            break;
        default:
            /* One frame, not the session. */
            state->out->rejected_frames++;
            break;
        }
    }
    return 0;
}

void chorus_session_keep_sound(chorus_session_result_t *result, const chorus_v2_sound_t *sound)
{
    result->sound = *sound;
    result->have_sound = 1;
    result->sounds_received++;
}

int chorus_session_last_sound(const chorus_session_result_t *result, chorus_v2_sound_t *out)
{
    if (!result->have_sound) {
        return 0;
    }
    *out = result->sound;
    return 1;
}

int chorus_session_run(const chorus_session_config_t *config, chorus_session_result_t *out)
{
    memset(out, 0, sizeof(*out));
    chorus_telemetry_init(&out->telemetry);

    char host[CHORUS_SESSION_ADDRESS_MAX];
    char port[16];
    if (split_address(config->server, host, sizeof(host), port, sizeof(port)) != 0) {
        out->end = CHORUS_SESSION_ADDRESS_UNUSABLE;
        snprintf(out->detail, sizeof(out->detail),
                 "the server address '%s' is not host:port, so there is nothing to join",
                 config->server);
        return -1;
    }

    static session_state_t state;
    memset(&state, 0, sizeof(state));
    chorus_telemetry_init(&state.telemetry);
    state.config = config;
    state.out = out;
    state.endpoint_id =
        (config->endpoint_id[0] == '\0') ? CHORUS_SESSION_DEFAULT_ID : config->endpoint_id;
    if (!chorus_v2_is_utf8((const uint8_t *)state.endpoint_id, strlen(state.endpoint_id))) {
        out->end = CHORUS_SESSION_IDENTITY_UNUSABLE;
        snprintf(out->detail, sizeof(out->detail), "the endpoint id is not UTF-8");
        return -1;
    }
    if (chorus_noise_setup() != CHORUS_NOISE_OK ||
        load_identity(&state, out->detail, sizeof(out->detail)) != 0 ||
        load_pins(&state, out->detail, sizeof(out->detail)) != 0) {
        out->end = CHORUS_SESSION_IDENTITY_UNUSABLE;
        if (out->detail[0] == '\0') {
            snprintf(out->detail, sizeof(out->detail), "the crypto library did not start");
        }
        return -1;
    }

    state.log = NULL;
    if (config->event_log_path != NULL) {
        state.log = fopen(config->event_log_path, "w");
        if (state.log == NULL) {
            out->end = CHORUS_SESSION_LOG_UNWRITABLE;
            snprintf(out->detail, sizeof(out->detail), "%s could not be opened for writing",
                     config->event_log_path);
            return -1;
        }
    }

    static chorus_offset_filter_t filter;
    chorus_offset_filter_init(&filter, config->filter_window, config->smoothing_alpha);

    static uint8_t buffer[CHORUS_SESSION_BUFFER];
    receive_t rx = {buffer, sizeof(buffer), 0};

    uint64_t started_ns = chorus_monotonic_now_ns();
    uint64_t run_ns = (uint64_t)config->run_seconds * 1000000000ull;
    uint32_t backoff_ms = config->first_backoff_ms;
    uint64_t went_down_ns = 0;
    int have_been_up = 0;
    int key_changed = 0;

    {
        char detail[CHORUS_SESSION_ID_MAX + 96];
        snprintf(detail, sizeof(detail), "identity id=%s key=%s store=%s", state.endpoint_id,
                 out->key_fingerprint, (config->key_path == NULL) ? "this-run" : config->key_path);
        publish_detail(&state, "start", detail);
    }

    while (run_ns == 0 || chorus_monotonic_now_ns() - started_ns < run_ns) {
        /* A server address set on the console since the last attempt
         * (audit A-13) applies from this attempt on. */
        char next[CHORUS_SESSION_ADDRESS_MAX];
        if (config->server_update != NULL &&
            config->server_update(config->server_update_ctx, next, sizeof(next)) == 1) {
            char next_host[CHORUS_SESSION_ADDRESS_MAX];
            char next_port[16];
            char detail[CHORUS_SESSION_ADDRESS_MAX + 64];
            if (split_address(next, next_host, sizeof(next_host), next_port, sizeof(next_port)) ==
                0) {
                memcpy(host, next_host, sizeof(host));
                memcpy(port, next_port, sizeof(port));
                backoff_ms = config->first_backoff_ms;
                snprintf(detail, sizeof(detail), "server=%s", next);
                publish_detail(&state, "server-changed", detail);
            } else {
                snprintf(detail, sizeof(detail), "'%s' is not host:port; kept the old one", next);
                publish_detail(&state, "server-refused", detail);
            }
        }
        state.telemetry.link = CHORUS_LINK_CONNECTING;
        state.telemetry.connect_attempts++;
        int fd = connect_once(host, port);
        if (fd < 0) {
            state.telemetry.link = CHORUS_LINK_DOWN;
            state.telemetry.audio = (state.telemetry.audio == CHORUS_AUDIO_STOPPED_ON_AMP_FAULT)
                                        ? state.telemetry.audio
                                        : CHORUS_AUDIO_IDLE;
            publish(&state, "connect-failed");
            chorus_monotonic_sleep_ms(backoff_ms);
            /* Doubling, capped. Never zero, never unbounded, and there is no
             * attempt counter that can run out. */
            backoff_ms =
                (backoff_ms * 2 > config->max_backoff_ms) ? config->max_backoff_ms : backoff_ms * 2;
            continue;
        }

        rx.held = 0;
        static chorus_noise_transport_t transport;
        handshake_result_t shaken = handshake(&state, fd, &rx, &transport);
        if (shaken != HANDSHAKE_DONE) {
            close(fd);
            state.telemetry.link = CHORUS_LINK_DOWN;
            publish(&state, "link-down");
            if (shaken == HANDSHAKE_KEY_CHANGED) {
                key_changed = 1;
                break;
            }
            if (have_been_up && went_down_ns == 0) {
                went_down_ns = chorus_monotonic_now_ns();
            }
            chorus_monotonic_sleep_ms(backoff_ms);
            backoff_ms =
                (backoff_ms * 2 > config->max_backoff_ms) ? config->max_backoff_ms : backoff_ms * 2;
            continue;
        }

        backoff_ms = config->first_backoff_ms;
        state.telemetry.link = CHORUS_LINK_UP;
        if (have_been_up) {
            state.telemetry.rejoins++;
            uint64_t outage = chorus_monotonic_now_ns() - went_down_ns;
            if (outage > out->longest_outage_ns) {
                out->longest_outage_ns = outage;
            }
        }
        have_been_up = 1;
        went_down_ns = 0;
        publish(&state, "link-up");

        const char *why = NULL;
        if (send_greeting(&state, fd, &transport.send) != 0) {
            why = "greeting-not-sent";
        }

        uint64_t chunks_at_connect = state.telemetry.chunks_received;
        stream_state_t stream;
        memset(&stream, 0, sizeof(stream));
        stream.filter = &filter;
        stream.next_exchange_ns = chorus_monotonic_now_ns();
        uint64_t next_telemetry_ns = chorus_monotonic_now_ns();

        while (why == NULL) {
            uint64_t now_ns = chorus_monotonic_now_ns();
            if (run_ns != 0 && now_ns - started_ns >= run_ns) {
                break;
            }
            if (!stream.exchange_outstanding && now_ns >= stream.next_exchange_ns) {
                stream.pending_t0_ns = chorus_monotonic_now_ns();
                if (send_time_sync_request(&state, fd, &transport.send, stream.pending_t0_ns) !=
                    0) {
                    why = "send-failed";
                    break;
                }
                stream.exchange_outstanding = 1;
            }
            if (now_ns >= next_telemetry_ns) {
                if (send_telemetry(&state, fd, &transport.send) != 0) {
                    why = "send-failed";
                    break;
                }
                next_telemetry_ns = now_ns + CHORUS_SESSION_TELEMETRY_INTERVAL_NS;
            }

            int got = receive_some(fd, &rx);
            if (got == -1) {
                why = "peer-closed";
                break;
            }
            if (got == -2) {
                why = "read-failed";
                break;
            }

            /* Every frame on the stream is a record; during the session the
             * only other thing a server may send is a refusal. Anything else
             * is the end of the session. */
            size_t len;
            while (why == NULL && (len = whole_frame(&rx)) > 0) {
                uint8_t type = rx.buf[0];
                if (type == CHORUS_V2_SECURE_RECORD) {
                    (void)take_record(&state, &stream, &transport.receive,
                                      rx.buf + CHORUS_FRAME_HEADER_LEN,
                                      len - CHORUS_FRAME_HEADER_LEN, &why);
                } else if (type == CHORUS_V2_SESSION_REFUSED) {
                    chorus_v2_frame_t frame = chorus_v2_decode_frame(rx.buf, len);
                    if (frame.outcome == CHORUS_FRAME_DECODED) {
                        surface_refusal(&state, &frame.message.as.session_refused);
                    }
                    why = "refused-by-server";
                } else {
                    why = "not-a-record";
                }
                consume(&rx, len);
            }
        }

        close(fd);
        chorus_codec_close(stream.decoder);
        chorus_noise_transport_clear(&transport);
        if (why != NULL) {
            publish(&state, why);
        }
        if (state.telemetry.chunks_received > chunks_at_connect) {
            out->connections_that_played++;
        }
        state.telemetry.link = CHORUS_LINK_DOWN;
        if (state.telemetry.audio != CHORUS_AUDIO_STOPPED_ON_AMP_FAULT) {
            state.telemetry.audio = CHORUS_AUDIO_IDLE;
        }
        went_down_ns = chorus_monotonic_now_ns();
        publish(&state, "link-down");

        if (run_ns != 0 && chorus_monotonic_now_ns() - started_ns >= run_ns) {
            break;
        }
        chorus_monotonic_sleep_ms(backoff_ms);
        backoff_ms =
            (backoff_ms * 2 > config->max_backoff_ms) ? config->max_backoff_ms : backoff_ms * 2;
    }

    refresh_playout(&state);
    out->telemetry = state.telemetry;
    if (key_changed) {
        out->end = CHORUS_SESSION_SERVER_KEY_CHANGED;
        publish(&state, "stop");
        if (state.log != NULL) {
            fclose(state.log);
        }
        return -1;
    }
    out->end = CHORUS_SESSION_RAN_ITS_TIME;
    publish(&state, "stop");
    if (state.log != NULL) {
        fclose(state.log);
    }
    return 0;
}
