/* The low-latency path's datagram layer, for the endpoint (goal 13, the TV
 * path).
 *
 * docs/protocol.md ("Low-latency path") is the contract and the shared
 * vectors under fixtures/protocol/lowlat/ are the authority: this is a SECOND
 * implementation of that contract, held to the same bytes as the Rust one
 * (crates/protocol/src/v2/lowlat.rs) by firmware/tests/test_lowlat.c, not a
 * port of it.
 *
 * Three pieces:
 *
 *   - The datagram: a 16-byte header in the clear (magic "CL", version 1,
 *     kind, stream_tag, counter, big-endian) authenticated as the associated
 *     data of ChaCha20-Poly1305 (RFC 8439), with the nonce stream_tag ||
 *     counter. The AEAD is the platform's, reached only through the PSA
 *     Crypto API exactly as chorus/noise.h reaches it, so the host build runs
 *     the TF-PSA-Crypto the image runs.
 *   - The replay window: RFC 4303 section 3.4.3's sliding window, 1024
 *     counters, moved only after a tag verified.
 *   - The FEC: one XOR parity per group of k data chunks with RFC 5109's
 *     length recovery, and an optional column interleave of depth D (a group
 *     is every D-th chunk of a block of k x D). A chunk that arrives is handed
 *     on at once; the one missing chunk of a group is rebuilt when the group's
 *     parity and its other k - 1 chunks are in.
 *
 * No allocation, no socket, no clock. Every buffer is the caller's; the
 * decoder is a large struct (its slots hold one accumulator each) that the
 * caller places. Nothing here is wired into the session yet: the endpoint's
 * UDP path is a later goal (chorus#TV-9), and this is the part of it the
 * shared vectors can hold now. */

#ifndef CHORUS_LOWLAT_H
#define CHORUS_LOWLAT_H

#include <stddef.h>
#include <stdint.h>

#define CHORUS_LOWLAT_VERSION 1u
#define CHORUS_LOWLAT_HEADER_LEN 16u
#define CHORUS_LOWLAT_TAG_LEN 16u
#define CHORUS_LOWLAT_KEY_LEN 32u
/* 1500-byte Ethernet MTU minus the IPv4 and UDP headers: one datagram is one
 * frame, never an IP fragment. */
#define CHORUS_LOWLAT_MAX_DATAGRAM_LEN 1472u
#define CHORUS_LOWLAT_MAX_PLAINTEXT_LEN                                                            \
    (CHORUS_LOWLAT_MAX_DATAGRAM_LEN - CHORUS_LOWLAT_HEADER_LEN - CHORUS_LOWLAT_TAG_LEN)
/* group u32, fec_k u8, fec_depth u8, length_xor u16 */
#define CHORUS_LOWLAT_PARITY_HEADER_LEN 8u
#define CHORUS_LOWLAT_MAX_DATA_PLAINTEXT_LEN                                                       \
    (CHORUS_LOWLAT_MAX_PLAINTEXT_LEN - CHORUS_LOWLAT_PARITY_HEADER_LEN)
/* An audio_chunk payload with one PCM byte. */
#define CHORUS_LOWLAT_MIN_DATA_PLAINTEXT_LEN 33u
#define CHORUS_LOWLAT_REPLAY_WINDOW 1024u
/* The reserved block of an audio chunk (ADR 0004's 14 bytes at offset 18) on
 * this path: 18 ll_marker (1), 19 fec_k, 20 fec_depth, 21 group_index, 22..26
 * group (u32 big-endian), 26..32 zero. */
#define CHORUS_LOWLAT_RESERVED_OFFSET 18u
#define CHORUS_LOWLAT_RESERVED_LEN 14u
#define CHORUS_LOWLAT_LL_MARKER 1u
#define CHORUS_LOWLAT_FEC_K_MAX 16u
#define CHORUS_LOWLAT_FEC_DEPTH_MAX 8u
/* A group is closed once a packet two blocks newer arrived (ASSUMED, as the
 * Rust OPEN_BLOCKS): three blocks of `depth` groups are held. */
#define CHORUS_LOWLAT_OPEN_BLOCKS 3u
#define CHORUS_LOWLAT_SLOTS (CHORUS_LOWLAT_OPEN_BLOCKS * CHORUS_LOWLAT_FEC_DEPTH_MAX)

#define CHORUS_LOWLAT_KIND_DATA 1u
#define CHORUS_LOWLAT_KIND_PARITY 2u

typedef enum {
    CHORUS_LOWLAT_OK = 0,
    /* Shorter than a header and a tag (a datagram), or than an audio chunk
     * with one PCM byte (a data plaintext). */
    CHORUS_LOWLAT_TOO_SHORT,
    /* Longer than its datagram can carry. */
    CHORUS_LOWLAT_TOO_LONG,
    CHORUS_LOWLAT_BAD_MAGIC,
    CHORUS_LOWLAT_BAD_VERSION,
    CHORUS_LOWLAT_BAD_KIND,
    /* Another stream's tag. */
    CHORUS_LOWLAT_WRONG_STREAM_TAG,
    /* A counter already seen, or older than the window. */
    CHORUS_LOWLAT_REPLAYED,
    /* The tag did not verify: a wrong key, or altered bytes. */
    CHORUS_LOWLAT_AUTH_FAILED,
    /* FEC parameters outside the offer's rules. */
    CHORUS_LOWLAT_BAD_PARAMS,
    CHORUS_LOWLAT_ZERO_STREAM_TAG,
    CHORUS_LOWLAT_ZERO_KEY,
    /* The counter or the group number is spent: the stream needs a new
     * offer. */
    CHORUS_LOWLAT_EXHAUSTED,
    /* The caller's output buffer is too small. Nothing is written. */
    CHORUS_LOWLAT_BUFFER_TOO_SMALL,
    /* The crypto library refused a call it should not have. */
    CHORUS_LOWLAT_CRYPTO_FAILED
} chorus_lowlat_status_t;

const char *chorus_lowlat_status_name(chorus_lowlat_status_t status);

/* --- the datagram ------------------------------------------------------------ */

typedef struct {
    uint8_t kind;
    uint32_t stream_tag;
    uint64_t counter;
} chorus_lowlat_header_t;

void chorus_lowlat_header_write(const chorus_lowlat_header_t *header,
                                uint8_t out[CHORUS_LOWLAT_HEADER_LEN]);

/* The header at the front of a datagram, checking its length, magic, version
 * and kind, in that order. */
chorus_lowlat_status_t chorus_lowlat_header_parse(const uint8_t *datagram, size_t len,
                                                  chorus_lowlat_header_t *out);

/* Seal one datagram into `out`: the header, the plaintext encrypted under
 * `key` with the header as associated data, the tag. */
chorus_lowlat_status_t chorus_lowlat_seal(const uint8_t key[CHORUS_LOWLAT_KEY_LEN],
                                          const chorus_lowlat_header_t *header,
                                          const uint8_t *plaintext, size_t plaintext_len,
                                          uint8_t *out, size_t out_cap, size_t *written);

/* Open one datagram without a replay window: the header's checks, then the
 * tag. The plaintext goes to `out`. */
chorus_lowlat_status_t chorus_lowlat_open(const uint8_t key[CHORUS_LOWLAT_KEY_LEN],
                                          const uint8_t *datagram, size_t len,
                                          chorus_lowlat_header_t *header, uint8_t *out,
                                          size_t out_cap, size_t *plaintext_len);

/* --- the replay window -------------------------------------------------------- */

typedef struct {
    uint64_t top;
    int any;
    /* Bit a (word a / 64, bit a % 64): counter top - a was seen. */
    uint64_t bits[CHORUS_LOWLAT_REPLAY_WINDOW / 64u];
} chorus_lowlat_replay_t;

void chorus_lowlat_replay_init(chorus_lowlat_replay_t *window);
int chorus_lowlat_replay_accepts(const chorus_lowlat_replay_t *window, uint64_t counter);
void chorus_lowlat_replay_commit(chorus_lowlat_replay_t *window, uint64_t counter);

/* --- one stream's sender and receiver ----------------------------------------- */

typedef struct {
    uint8_t key[CHORUS_LOWLAT_KEY_LEN];
    uint32_t stream_tag;
    uint64_t next_counter;
} chorus_lowlat_sealer_t;

chorus_lowlat_status_t chorus_lowlat_sealer_init(chorus_lowlat_sealer_t *sealer,
                                                 const uint8_t key[CHORUS_LOWLAT_KEY_LEN],
                                                 uint32_t stream_tag, uint64_t first_counter);

/* Seal one plaintext of `kind` with the next counter. */
chorus_lowlat_status_t chorus_lowlat_sealer_seal(chorus_lowlat_sealer_t *sealer, uint8_t kind,
                                                 const uint8_t *plaintext, size_t plaintext_len,
                                                 uint8_t *out, size_t out_cap, size_t *written);

typedef struct {
    uint64_t opened;
    /* Too short, too long, a bad magic, version or kind. */
    uint64_t malformed;
    uint64_t wrong_stream_tag;
    uint64_t replayed;
    uint64_t auth_failed;
} chorus_lowlat_open_stats_t;

typedef struct {
    uint8_t key[CHORUS_LOWLAT_KEY_LEN];
    uint32_t stream_tag;
    chorus_lowlat_replay_t window;
    chorus_lowlat_open_stats_t stats;
} chorus_lowlat_opener_t;

void chorus_lowlat_opener_init(chorus_lowlat_opener_t *opener,
                               const uint8_t key[CHORUS_LOWLAT_KEY_LEN], uint32_t stream_tag);

/* Open one datagram: the header's form, the stream's tag, the replay window
 * (before any cryptography), the AEAD tag, and only then the window moves.
 * Every outcome is counted. */
chorus_lowlat_status_t chorus_lowlat_opener_open(chorus_lowlat_opener_t *opener,
                                                 const uint8_t *datagram, size_t len,
                                                 chorus_lowlat_header_t *header, uint8_t *out,
                                                 size_t out_cap, size_t *plaintext_len);

/* --- the reserved block and the FEC shape ------------------------------------- */

typedef struct {
    uint8_t fec_k;
    uint8_t fec_depth;
    uint8_t group_index;
    uint32_t group;
} chorus_lowlat_chunk_info_t;

void chorus_lowlat_chunk_info_write(const chorus_lowlat_chunk_info_t *info,
                                    uint8_t reserved[CHORUS_LOWLAT_RESERVED_LEN]);

/* The info in an audio_chunk payload's reserved block: 0, or -1 when the
 * block does not carry the marker (a TCP chunk) or its last six bytes are not
 * zero. */
int chorus_lowlat_chunk_info_read(const uint8_t *payload, size_t len,
                                  chorus_lowlat_chunk_info_t *out);

typedef struct {
    uint8_t k;
    uint8_t depth;
} chorus_lowlat_fec_params_t;

/* The offer's fec_k (0, or 2 to 16) and fec_depth (1 to 8; 1 when k is 0). */
chorus_lowlat_status_t chorus_lowlat_fec_params(uint8_t k, uint8_t depth,
                                                chorus_lowlat_fec_params_t *out);

/* Chunks in a group: k, or 1 without FEC. */
uint32_t chorus_lowlat_group_len(const chorus_lowlat_fec_params_t *params);

/* Chunk n's group and position: 0, or -1 past the last group number. */
int chorus_lowlat_locate(const chorus_lowlat_fec_params_t *params, uint64_t n, uint32_t *group,
                         uint8_t *index);

uint64_t chorus_lowlat_chunk_index(const chorus_lowlat_fec_params_t *params, uint32_t group,
                                   uint8_t index);

/* --- the encoder -------------------------------------------------------------- */

typedef struct {
    uint8_t xor_bytes[CHORUS_LOWLAT_MAX_DATA_PLAINTEXT_LEN];
    size_t len;
    uint16_t len_xor;
    uint32_t count;
} chorus_lowlat_column_t;

typedef struct {
    chorus_lowlat_fec_params_t params;
    uint64_t next;
    chorus_lowlat_column_t columns[CHORUS_LOWLAT_FEC_DEPTH_MAX];
} chorus_lowlat_fec_encoder_t;

void chorus_lowlat_fec_encoder_init(chorus_lowlat_fec_encoder_t *encoder,
                                    const chorus_lowlat_fec_params_t *params);

/* Push one audio_chunk payload: bytes 18..32 are overwritten with its chunk
 * info, and the payload itself is then the data datagram's plaintext. When
 * it completed its group, the group's parity plaintext is written to
 * `parity` and its length to *parity_len (else *parity_len is 0); send the
 * data first, then the parity. */
chorus_lowlat_status_t chorus_lowlat_fec_encode(chorus_lowlat_fec_encoder_t *encoder,
                                                uint8_t *payload, size_t len, uint8_t *parity,
                                                size_t parity_cap, size_t *parity_len);

/* --- the decoder -------------------------------------------------------------- */

typedef struct {
    uint64_t data;
    uint64_t parity;
    uint64_t delivered;
    uint64_t recovered;
    /* Chunks of a closed group that never arrived and could not be rebuilt. */
    uint64_t unrecoverable;
    /* Datagrams for a group already closed. */
    uint64_t late;
    uint64_t duplicate;
    /* FEC fields that contradict the stream's, or a rebuild that does not
     * carry its own group's fields. */
    uint64_t rejected;
} chorus_lowlat_fec_stats_t;

typedef struct {
    int used;
    uint32_t group;
    uint32_t have;
    int parity;
    int failed;
    uint8_t acc[CHORUS_LOWLAT_MAX_DATA_PLAINTEXT_LEN];
    size_t acc_len;
    uint16_t len_xor;
} chorus_lowlat_slot_t;

typedef struct {
    chorus_lowlat_fec_params_t params;
    chorus_lowlat_slot_t slots[CHORUS_LOWLAT_SLOTS];
    uint32_t slot_count;
    int started;
    uint32_t next_close;
    uint32_t newest_block;
    chorus_lowlat_fec_stats_t stats;
} chorus_lowlat_fec_decoder_t;

/* Called for each chunk handed on, received or rebuilt; `payload` is valid
 * only during the call. */
typedef void (*chorus_lowlat_deliver_fn)(void *ctx, uint64_t chunk_index, uint32_t group,
                                         uint8_t group_index, int recovered, const uint8_t *payload,
                                         size_t len);

void chorus_lowlat_fec_decoder_init(chorus_lowlat_fec_decoder_t *decoder,
                                    const chorus_lowlat_fec_params_t *params);

/* Push one opened datagram's plaintext of `kind`. */
void chorus_lowlat_fec_decode(chorus_lowlat_fec_decoder_t *decoder, uint8_t kind,
                              const uint8_t *plaintext, size_t len,
                              chorus_lowlat_deliver_fn deliver, void *ctx);

/* Close every group before `group`: the playout point has passed them. */
void chorus_lowlat_fec_expire_before(chorus_lowlat_fec_decoder_t *decoder, uint32_t group);

/* The stream ended after `chunks_sent` chunks: close every group, counting
 * as lost only chunks that were sent. */
void chorus_lowlat_fec_finish(chorus_lowlat_fec_decoder_t *decoder, uint64_t chunks_sent);

#endif /* CHORUS_LOWLAT_H */
