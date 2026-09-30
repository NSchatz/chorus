/* Noise_XX_25519_ChaChaPoly_SHA256, chorus protocol v2's key exchange, for
 * the endpoint.
 *
 * Written from the Noise Protocol Framework specification, revision 34
 * (<https://noiseprotocol.org/noise.html>, read 2026-09-30; public domain):
 * section 5 (CipherState, SymmetricState, HandshakeState), section 4.3 (HKDF),
 * section 7.5 (the XX pattern) and section 12 (25519, ChaChaPoly, SHA256).
 * docs/protocol.md "The session" says how chorus uses it, and
 * docs/decisions/0039-the-v2-key-exchange.md why.
 *
 *   XX:
 *     -> e
 *     <- e, ee, s, es
 *     -> s, se
 *
 * The primitives are the platform's, reached only through the PSA Crypto API
 * (psa/crypto.h): X25519 through psa_raw_key_agreement, ChaCha20-Poly1305,
 * SHA-256, and HMAC-SHA256 for the HKDF. That is the whole seam between this
 * state machine and a crypto library: the host build links TF-PSA-Crypto
 * compiled from the pinned ESP-IDF v6.1 tree, and the device build ESP-IDF's
 * own mbedtls component, with this same code. Nothing here holds a socket or
 * reads a clock, and every key is an argument: the caller supplies the
 * randomness, which is what lets the golden vectors fix the ephemeral keys.
 *
 * Both roles are implemented. The endpoint is always the initiator; the
 * responder is here so the published test vector can be checked in both
 * roles, as docs/protocol.md asks. */

#ifndef CHORUS_NOISE_H
#define CHORUS_NOISE_H

#include <stddef.h>
#include <stdint.h>

#define CHORUS_NOISE_KEY_LEN 32u
#define CHORUS_NOISE_TAG_LEN 16u
#define CHORUS_NOISE_HASH_LEN 32u
/* The largest Noise message (section 3). */
#define CHORUS_NOISE_MAX_MESSAGE_LEN 65535u
/* "xxxx:xxxx:xxxx:xxxx" and its NUL. */
#define CHORUS_NOISE_FINGERPRINT_LEN 20u

typedef enum {
    CHORUS_NOISE_OK = 0,
    /* A message is not the length this step needs. */
    CHORUS_NOISE_BAD_LENGTH,
    /* Authenticated decryption failed: wrong key, altered bytes, a replay. */
    CHORUS_NOISE_DECRYPT_FAILED,
    /* A Diffie-Hellman result was all zero (a low-order public key). */
    CHORUS_NOISE_WEAK_KEY,
    /* A step called out of order. */
    CHORUS_NOISE_OUT_OF_ORDER,
    /* A message would exceed 65535 bytes. */
    CHORUS_NOISE_TOO_LONG,
    /* The nonce counter is spent; the session must end. */
    CHORUS_NOISE_NONCE_EXHAUSTED,
    /* The caller's output buffer is too small. Nothing is written. */
    CHORUS_NOISE_BUFFER_TOO_SMALL,
    /* The crypto library refused a call it should not have (not initialised,
     * out of key slots). */
    CHORUS_NOISE_CRYPTO_FAILED
} chorus_noise_status_t;

const char *chorus_noise_status_name(chorus_noise_status_t status);

/* psa_crypto_init. Safe to call more than once. */
chorus_noise_status_t chorus_noise_setup(void);

/* A source of random bytes: 0 on success. The default is
 * chorus_noise_system_random (psa_generate_random, which is the host's
 * getrandom behind a DRBG, or the ESP32-S3's hardware generator). Tests
 * substitute a source that hands out fixed keys. */
typedef int (*chorus_noise_random_fn)(void *ctx, uint8_t *out, size_t len);
int chorus_noise_system_random(void *ctx, uint8_t *out, size_t len);

/* A Curve25519 key pair. The secret is kept as its 32 bytes; X25519 clamps
 * it when it is used. */
typedef struct {
    uint8_t secret[CHORUS_NOISE_KEY_LEN];
    uint8_t public_key[CHORUS_NOISE_KEY_LEN];
} chorus_noise_keypair_t;

chorus_noise_status_t chorus_noise_keypair_from_secret(const uint8_t secret[CHORUS_NOISE_KEY_LEN],
                                                       chorus_noise_keypair_t *out);

/* SHA-256 of `len` bytes. */
chorus_noise_status_t chorus_noise_sha256(const uint8_t *data, size_t len,
                                          uint8_t out[CHORUS_NOISE_HASH_LEN]);

/* A public key's fingerprint for people to compare: the first 8 bytes of its
 * SHA-256 as four colon-separated groups of 4 hex digits
 * (docs/protocol.md, "Adoption"). */
chorus_noise_status_t chorus_noise_fingerprint(const uint8_t public_key[CHORUS_NOISE_KEY_LEN],
                                               char out[CHORUS_NOISE_FINGERPRINT_LEN]);

/* Section 5.1: a key and a nonce. The key lives in the crypto library's key
 * store from the moment it is set; clear the state to destroy it. */
typedef struct {
    int has_key;
    uint32_t key_id;
    uint64_t nonce;
} chorus_noise_cipher_t;

/* EncryptWithAd: `plaintext_len + 16` bytes into `out` (which must not
 * overlap the plaintext). Without a key the plaintext is copied unchanged. */
chorus_noise_status_t chorus_noise_encrypt(chorus_noise_cipher_t *cipher, const uint8_t *ad,
                                           size_t ad_len, const uint8_t *plaintext,
                                           size_t plaintext_len, uint8_t *out, size_t out_len,
                                           size_t *written);

/* DecryptWithAd: the nonce advances only on success. */
chorus_noise_status_t chorus_noise_decrypt(chorus_noise_cipher_t *cipher, const uint8_t *ad,
                                           size_t ad_len, const uint8_t *ciphertext,
                                           size_t ciphertext_len, uint8_t *out, size_t out_len,
                                           size_t *written);

/* Destroy the key and forget the state. */
void chorus_noise_cipher_clear(chorus_noise_cipher_t *cipher);

/* The finished handshake. */
typedef struct {
    /* Seals what this side sends. */
    chorus_noise_cipher_t send;
    /* Opens what this side receives. */
    chorus_noise_cipher_t receive;
    /* h after the last message: the same on both sides, a channel binding
     * value (section 11.2). */
    uint8_t handshake_hash[CHORUS_NOISE_HASH_LEN];
    /* The peer's long-term public key, authenticated by the handshake. */
    uint8_t remote_static[CHORUS_NOISE_KEY_LEN];
} chorus_noise_transport_t;

void chorus_noise_transport_clear(chorus_noise_transport_t *transport);

/* Section 5.3, for the XX pattern only. */
typedef struct {
    int initiator;
    /* Messages done: 0, 1, 2 or 3. */
    int step;
    chorus_noise_cipher_t cipher;
    uint8_t chaining_key[CHORUS_NOISE_HASH_LEN];
    uint8_t hash[CHORUS_NOISE_HASH_LEN];
    chorus_noise_keypair_t s;
    chorus_noise_keypair_t e;
    uint8_t re[CHORUS_NOISE_KEY_LEN];
    uint8_t rs[CHORUS_NOISE_KEY_LEN];
    int have_rs;
    int failed;
} chorus_noise_handshake_t;

/* Initialize with the prologue, this side's long-term key and its ephemeral
 * key for this handshake. */
chorus_noise_status_t chorus_noise_handshake_init(chorus_noise_handshake_t *hs, int initiator,
                                                  const uint8_t *prologue, size_t prologue_len,
                                                  const chorus_noise_keypair_t *s,
                                                  const chorus_noise_keypair_t *e);

/* Write the next message, carrying `payload`, into `out`. */
chorus_noise_status_t chorus_noise_write_message(chorus_noise_handshake_t *hs,
                                                 const uint8_t *payload, size_t payload_len,
                                                 uint8_t *out, size_t out_len, size_t *written);

/* Read the next message; its payload goes to `payload`. */
chorus_noise_status_t chorus_noise_read_message(chorus_noise_handshake_t *hs,
                                                const uint8_t *message, size_t message_len,
                                                uint8_t *payload, size_t payload_len,
                                                size_t *written);

/* After message 3: Split() into the two transport ciphers, the initiator's
 * first key sealing initiator to responder. The handshake state is cleared. */
chorus_noise_status_t chorus_noise_split(chorus_noise_handshake_t *hs,
                                         chorus_noise_transport_t *out);

/* Destroy any key the handshake holds (after a failure, for example). */
void chorus_noise_handshake_clear(chorus_noise_handshake_t *hs);

/* --- secure records (docs/protocol.md, "0x24 secure record") ---------------
 *
 * A record's plaintext is one or more whole frames; its associated data is
 * the record frame's own 3-byte header, so the type and length are
 * authenticated too. */

/* Seal `frames` (at most CHORUS_V2_MAX_RECORD_PLAINTEXT bytes) into a whole
 * secure_record frame in `out`. */
chorus_noise_status_t chorus_noise_seal_record(chorus_noise_cipher_t *cipher, const uint8_t *frames,
                                               size_t frames_len, uint8_t *out, size_t out_len,
                                               size_t *written);

/* Open a record's payload (its ciphertext and tag) into the frames it
 * carries. */
chorus_noise_status_t chorus_noise_open_record(chorus_noise_cipher_t *cipher,
                                               const uint8_t *ciphertext, size_t ciphertext_len,
                                               uint8_t *out, size_t out_len, size_t *written);

#endif /* CHORUS_NOISE_H */
