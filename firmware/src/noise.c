#include "chorus/noise.h"

#include "chorus/protocol_v2.h"

#include <psa/crypto.h>
#include <string.h>

/* The protocol name, which is also the initial chaining key and hash: it is
 * exactly 32 bytes, so section 5.2's InitializeSymmetric uses it unhashed. */
static const char PROTOCOL_NAME[] = "Noise_XX_25519_ChaChaPoly_SHA256";

const char *chorus_noise_status_name(chorus_noise_status_t status)
{
    switch (status) {
    case CHORUS_NOISE_OK:
        return "ok";
    case CHORUS_NOISE_BAD_LENGTH:
        return "bad-length";
    case CHORUS_NOISE_DECRYPT_FAILED:
        return "decrypt-failed";
    case CHORUS_NOISE_WEAK_KEY:
        return "weak-key";
    case CHORUS_NOISE_OUT_OF_ORDER:
        return "out-of-order";
    case CHORUS_NOISE_TOO_LONG:
        return "too-long";
    case CHORUS_NOISE_NONCE_EXHAUSTED:
        return "nonce-exhausted";
    case CHORUS_NOISE_BUFFER_TOO_SMALL:
        return "buffer-too-small";
    case CHORUS_NOISE_CRYPTO_FAILED:
        return "crypto-failed";
    }
    return "unknown-status";
}

chorus_noise_status_t chorus_noise_setup(void)
{
    return (psa_crypto_init() == PSA_SUCCESS) ? CHORUS_NOISE_OK : CHORUS_NOISE_CRYPTO_FAILED;
}

int chorus_noise_system_random(void *ctx, uint8_t *out, size_t len)
{
    (void)ctx;
    if (chorus_noise_setup() != CHORUS_NOISE_OK) {
        return -1;
    }
    return (psa_generate_random(out, len) == PSA_SUCCESS) ? 0 : -1;
}

/* Wipe a secret so a compiler cannot drop the stores as dead. */
static void wipe(void *p, size_t n)
{
    volatile uint8_t *v = (volatile uint8_t *)p;
    while (n-- > 0) {
        *v++ = 0;
    }
}

/* --- the primitives, through PSA only ---------------------------------------- */

static chorus_noise_status_t import_x25519(const uint8_t secret[CHORUS_NOISE_KEY_LEN],
                                           psa_key_id_t *id)
{
    psa_key_attributes_t attributes = PSA_KEY_ATTRIBUTES_INIT;
    psa_set_key_type(&attributes, PSA_KEY_TYPE_ECC_KEY_PAIR(PSA_ECC_FAMILY_MONTGOMERY));
    psa_set_key_bits(&attributes, 255);
    psa_set_key_usage_flags(&attributes, PSA_KEY_USAGE_DERIVE);
    psa_set_key_algorithm(&attributes, PSA_ALG_ECDH);
    psa_status_t status = psa_import_key(&attributes, secret, CHORUS_NOISE_KEY_LEN, id);
    psa_reset_key_attributes(&attributes);
    return (status == PSA_SUCCESS) ? CHORUS_NOISE_OK : CHORUS_NOISE_CRYPTO_FAILED;
}

chorus_noise_status_t chorus_noise_keypair_from_secret(const uint8_t secret[CHORUS_NOISE_KEY_LEN],
                                                       chorus_noise_keypair_t *out)
{
    if (chorus_noise_setup() != CHORUS_NOISE_OK) {
        return CHORUS_NOISE_CRYPTO_FAILED;
    }
    psa_key_id_t id = 0;
    chorus_noise_status_t status = import_x25519(secret, &id);
    if (status != CHORUS_NOISE_OK) {
        return status;
    }
    size_t len = 0;
    psa_status_t exported =
        psa_export_public_key(id, out->public_key, sizeof(out->public_key), &len);
    psa_destroy_key(id);
    if (exported != PSA_SUCCESS || len != CHORUS_NOISE_KEY_LEN) {
        return CHORUS_NOISE_CRYPTO_FAILED;
    }
    memcpy(out->secret, secret, CHORUS_NOISE_KEY_LEN);
    return CHORUS_NOISE_OK;
}

/* DH(key_pair, public_key), section 12.1. An all-zero result means the peer
 * sent a low-order point, which is refused rather than keyed with. */
static chorus_noise_status_t dh(const chorus_noise_keypair_t *pair,
                                const uint8_t remote[CHORUS_NOISE_KEY_LEN],
                                uint8_t out[CHORUS_NOISE_KEY_LEN])
{
    psa_key_id_t id = 0;
    chorus_noise_status_t status = import_x25519(pair->secret, &id);
    if (status != CHORUS_NOISE_OK) {
        return status;
    }
    size_t len = 0;
    psa_status_t agreed = psa_raw_key_agreement(PSA_ALG_ECDH, id, remote, CHORUS_NOISE_KEY_LEN, out,
                                                CHORUS_NOISE_KEY_LEN, &len);
    psa_destroy_key(id);
    if (agreed != PSA_SUCCESS || len != CHORUS_NOISE_KEY_LEN) {
        /* A library that refuses a low-order point itself says so here. */
        return (agreed == PSA_ERROR_INVALID_ARGUMENT) ? CHORUS_NOISE_WEAK_KEY
                                                      : CHORUS_NOISE_CRYPTO_FAILED;
    }
    uint8_t any = 0;
    for (size_t i = 0; i < CHORUS_NOISE_KEY_LEN; i++) {
        any |= out[i];
    }
    return (any == 0) ? CHORUS_NOISE_WEAK_KEY : CHORUS_NOISE_OK;
}

chorus_noise_status_t chorus_noise_sha256(const uint8_t *data, size_t len,
                                          uint8_t out[CHORUS_NOISE_HASH_LEN])
{
    static const uint8_t none[1] = {0};
    size_t got = 0;
    if (chorus_noise_setup() != CHORUS_NOISE_OK) {
        return CHORUS_NOISE_CRYPTO_FAILED;
    }
    psa_status_t status = psa_hash_compute(PSA_ALG_SHA_256, (len == 0) ? none : data, len, out,
                                           CHORUS_NOISE_HASH_LEN, &got);
    return (status == PSA_SUCCESS && got == CHORUS_NOISE_HASH_LEN) ? CHORUS_NOISE_OK
                                                                   : CHORUS_NOISE_CRYPTO_FAILED;
}

/* h = HASH(h || data), section 5.2's MixHash. */
static chorus_noise_status_t mix_hash(uint8_t hash[CHORUS_NOISE_HASH_LEN], const uint8_t *data,
                                      size_t len)
{
    psa_hash_operation_t op = PSA_HASH_OPERATION_INIT;
    size_t got = 0;
    psa_status_t status = psa_hash_setup(&op, PSA_ALG_SHA_256);
    if (status == PSA_SUCCESS) {
        status = psa_hash_update(&op, hash, CHORUS_NOISE_HASH_LEN);
    }
    if (status == PSA_SUCCESS && len > 0) {
        status = psa_hash_update(&op, data, len);
    }
    if (status == PSA_SUCCESS) {
        status = psa_hash_finish(&op, hash, CHORUS_NOISE_HASH_LEN, &got);
    }
    if (status != PSA_SUCCESS) {
        psa_hash_abort(&op);
        return CHORUS_NOISE_CRYPTO_FAILED;
    }
    return CHORUS_NOISE_OK;
}

/* HMAC-SHA256(key, data), section 4.3. */
static chorus_noise_status_t hmac(const uint8_t key[CHORUS_NOISE_HASH_LEN], const uint8_t *data,
                                  size_t len, uint8_t out[CHORUS_NOISE_HASH_LEN])
{
    static const uint8_t none[1] = {0};
    psa_key_attributes_t attributes = PSA_KEY_ATTRIBUTES_INIT;
    psa_set_key_type(&attributes, PSA_KEY_TYPE_HMAC);
    psa_set_key_bits(&attributes, 8 * CHORUS_NOISE_HASH_LEN);
    psa_set_key_usage_flags(&attributes, PSA_KEY_USAGE_SIGN_MESSAGE);
    psa_set_key_algorithm(&attributes, PSA_ALG_HMAC(PSA_ALG_SHA_256));
    psa_key_id_t id = 0;
    psa_status_t status = psa_import_key(&attributes, key, CHORUS_NOISE_HASH_LEN, &id);
    psa_reset_key_attributes(&attributes);
    if (status != PSA_SUCCESS) {
        return CHORUS_NOISE_CRYPTO_FAILED;
    }
    size_t got = 0;
    status = psa_mac_compute(id, PSA_ALG_HMAC(PSA_ALG_SHA_256), (len == 0) ? none : data, len, out,
                             CHORUS_NOISE_HASH_LEN, &got);
    psa_destroy_key(id);
    return (status == PSA_SUCCESS && got == CHORUS_NOISE_HASH_LEN) ? CHORUS_NOISE_OK
                                                                   : CHORUS_NOISE_CRYPTO_FAILED;
}

/* HKDF with two outputs, section 4.3: temp = HMAC(ck, ikm);
 * out1 = HMAC(temp, 0x01); out2 = HMAC(temp, out1 || 0x02). */
static chorus_noise_status_t hkdf2(const uint8_t chaining_key[CHORUS_NOISE_HASH_LEN],
                                   const uint8_t *ikm, size_t ikm_len,
                                   uint8_t out1[CHORUS_NOISE_HASH_LEN],
                                   uint8_t out2[CHORUS_NOISE_HASH_LEN])
{
    uint8_t temp[CHORUS_NOISE_HASH_LEN];
    uint8_t second[CHORUS_NOISE_HASH_LEN + 1];
    static const uint8_t one[1] = {0x01};
    chorus_noise_status_t status = hmac(chaining_key, ikm, ikm_len, temp);
    if (status == CHORUS_NOISE_OK) {
        status = hmac(temp, one, 1, out1);
    }
    if (status == CHORUS_NOISE_OK) {
        memcpy(second, out1, CHORUS_NOISE_HASH_LEN);
        second[CHORUS_NOISE_HASH_LEN] = 0x02;
        status = hmac(temp, second, sizeof(second), out2);
    }
    wipe(temp, sizeof(temp));
    wipe(second, sizeof(second));
    return status;
}

chorus_noise_status_t chorus_noise_fingerprint(const uint8_t public_key[CHORUS_NOISE_KEY_LEN],
                                               char out[CHORUS_NOISE_FINGERPRINT_LEN])
{
    static const char HEX[] = "0123456789abcdef";
    uint8_t digest[CHORUS_NOISE_HASH_LEN];
    chorus_noise_status_t status = chorus_noise_sha256(public_key, CHORUS_NOISE_KEY_LEN, digest);
    if (status != CHORUS_NOISE_OK) {
        out[0] = '\0';
        return status;
    }
    size_t at = 0;
    for (size_t i = 0; i < 8; i++) {
        if (i > 0 && i % 2 == 0) {
            out[at++] = ':';
        }
        out[at++] = HEX[digest[i] >> 4];
        out[at++] = HEX[digest[i] & 0x0F];
    }
    out[at] = '\0';
    return CHORUS_NOISE_OK;
}

/* --- CipherState (section 5.1) ---------------------------------------------- */

void chorus_noise_cipher_clear(chorus_noise_cipher_t *cipher)
{
    if (cipher->has_key) {
        psa_destroy_key((psa_key_id_t)cipher->key_id);
    }
    cipher->has_key = 0;
    cipher->key_id = 0;
    cipher->nonce = 0;
}

static chorus_noise_status_t cipher_set_key(chorus_noise_cipher_t *cipher,
                                            const uint8_t key[CHORUS_NOISE_KEY_LEN])
{
    chorus_noise_cipher_clear(cipher);
    psa_key_attributes_t attributes = PSA_KEY_ATTRIBUTES_INIT;
    psa_set_key_type(&attributes, PSA_KEY_TYPE_CHACHA20);
    psa_set_key_bits(&attributes, 256);
    psa_set_key_usage_flags(&attributes, PSA_KEY_USAGE_ENCRYPT | PSA_KEY_USAGE_DECRYPT);
    psa_set_key_algorithm(&attributes, PSA_ALG_CHACHA20_POLY1305);
    psa_key_id_t id = 0;
    psa_status_t status = psa_import_key(&attributes, key, CHORUS_NOISE_KEY_LEN, &id);
    psa_reset_key_attributes(&attributes);
    if (status != PSA_SUCCESS) {
        return CHORUS_NOISE_CRYPTO_FAILED;
    }
    cipher->has_key = 1;
    cipher->key_id = (uint32_t)id;
    cipher->nonce = 0;
    return CHORUS_NOISE_OK;
}

/* Section 12.3: 32 bits of zeros, then the 64-bit nonce little-endian. */
static void nonce_bytes(uint64_t n, uint8_t out[12])
{
    memset(out, 0, 4);
    for (int i = 0; i < 8; i++) {
        out[4 + i] = (uint8_t)((n >> (8 * i)) & 0xFF);
    }
}

chorus_noise_status_t chorus_noise_encrypt(chorus_noise_cipher_t *cipher, const uint8_t *ad,
                                           size_t ad_len, const uint8_t *plaintext,
                                           size_t plaintext_len, uint8_t *out, size_t out_len,
                                           size_t *written)
{
    static const uint8_t none[1] = {0};
    if (!cipher->has_key) {
        if (out_len < plaintext_len) {
            return CHORUS_NOISE_BUFFER_TOO_SMALL;
        }
        if (plaintext_len > 0) {
            memmove(out, plaintext, plaintext_len);
        }
        *written = plaintext_len;
        return CHORUS_NOISE_OK;
    }
    /* Section 5.1: 2^64 - 1 is reserved; a counter that reached it is
     * spent. */
    if (cipher->nonce == UINT64_MAX) {
        return CHORUS_NOISE_NONCE_EXHAUSTED;
    }
    if (plaintext_len + CHORUS_NOISE_TAG_LEN > CHORUS_NOISE_MAX_MESSAGE_LEN) {
        return CHORUS_NOISE_TOO_LONG;
    }
    if (out_len < plaintext_len + CHORUS_NOISE_TAG_LEN) {
        return CHORUS_NOISE_BUFFER_TOO_SMALL;
    }
    uint8_t nonce[12];
    nonce_bytes(cipher->nonce, nonce);
    size_t got = 0;
    psa_status_t status = psa_aead_encrypt((psa_key_id_t)cipher->key_id, PSA_ALG_CHACHA20_POLY1305,
                                           nonce, sizeof(nonce), (ad_len == 0) ? none : ad, ad_len,
                                           (plaintext_len == 0) ? none : plaintext, plaintext_len,
                                           out, out_len, &got);
    if (status != PSA_SUCCESS || got != plaintext_len + CHORUS_NOISE_TAG_LEN) {
        return CHORUS_NOISE_CRYPTO_FAILED;
    }
    cipher->nonce++;
    *written = got;
    return CHORUS_NOISE_OK;
}

chorus_noise_status_t chorus_noise_decrypt(chorus_noise_cipher_t *cipher, const uint8_t *ad,
                                           size_t ad_len, const uint8_t *ciphertext,
                                           size_t ciphertext_len, uint8_t *out, size_t out_len,
                                           size_t *written)
{
    static const uint8_t none[1] = {0};
    static uint8_t sink[1];
    if (!cipher->has_key) {
        if (out_len < ciphertext_len) {
            return CHORUS_NOISE_BUFFER_TOO_SMALL;
        }
        if (ciphertext_len > 0) {
            memmove(out, ciphertext, ciphertext_len);
        }
        *written = ciphertext_len;
        return CHORUS_NOISE_OK;
    }
    if (cipher->nonce == UINT64_MAX) {
        return CHORUS_NOISE_NONCE_EXHAUSTED;
    }
    if (ciphertext_len < CHORUS_NOISE_TAG_LEN) {
        return CHORUS_NOISE_BAD_LENGTH;
    }
    size_t plain_len = ciphertext_len - CHORUS_NOISE_TAG_LEN;
    if (out_len < plain_len) {
        return CHORUS_NOISE_BUFFER_TOO_SMALL;
    }
    uint8_t nonce[12];
    nonce_bytes(cipher->nonce, nonce);
    size_t got = 0;
    psa_status_t status =
        psa_aead_decrypt((psa_key_id_t)cipher->key_id, PSA_ALG_CHACHA20_POLY1305, nonce,
                         sizeof(nonce), (ad_len == 0) ? none : ad, ad_len, ciphertext,
                         ciphertext_len, (plain_len == 0) ? sink : out, out_len, &got);
    if (status == PSA_ERROR_INVALID_SIGNATURE) {
        return CHORUS_NOISE_DECRYPT_FAILED;
    }
    if (status != PSA_SUCCESS || got != plain_len) {
        return CHORUS_NOISE_CRYPTO_FAILED;
    }
    cipher->nonce++;
    *written = got;
    return CHORUS_NOISE_OK;
}

void chorus_noise_transport_clear(chorus_noise_transport_t *transport)
{
    chorus_noise_cipher_clear(&transport->send);
    chorus_noise_cipher_clear(&transport->receive);
}

/* --- SymmetricState and HandshakeState (sections 5.2, 5.3) ------------------ */

static chorus_noise_status_t mix_key(chorus_noise_handshake_t *hs, const uint8_t *ikm,
                                     size_t ikm_len)
{
    uint8_t ck[CHORUS_NOISE_HASH_LEN];
    uint8_t k[CHORUS_NOISE_HASH_LEN];
    chorus_noise_status_t status = hkdf2(hs->chaining_key, ikm, ikm_len, ck, k);
    if (status == CHORUS_NOISE_OK) {
        memcpy(hs->chaining_key, ck, sizeof(ck));
        status = cipher_set_key(&hs->cipher, k);
    }
    wipe(ck, sizeof(ck));
    wipe(k, sizeof(k));
    return status;
}

/* EncryptAndHash: the ciphertext goes to `out` and into h. */
static chorus_noise_status_t encrypt_and_hash(chorus_noise_handshake_t *hs,
                                              const uint8_t *plaintext, size_t len, uint8_t *out,
                                              size_t out_len, size_t *written)
{
    chorus_noise_status_t status = chorus_noise_encrypt(&hs->cipher, hs->hash, sizeof(hs->hash),
                                                        plaintext, len, out, out_len, written);
    if (status != CHORUS_NOISE_OK) {
        return status;
    }
    return mix_hash(hs->hash, out, *written);
}

static chorus_noise_status_t decrypt_and_hash(chorus_noise_handshake_t *hs,
                                              const uint8_t *ciphertext, size_t len, uint8_t *out,
                                              size_t out_len, size_t *written)
{
    chorus_noise_status_t status = chorus_noise_decrypt(&hs->cipher, hs->hash, sizeof(hs->hash),
                                                        ciphertext, len, out, out_len, written);
    if (status != CHORUS_NOISE_OK) {
        return status;
    }
    return mix_hash(hs->hash, ciphertext, len);
}

chorus_noise_status_t chorus_noise_handshake_init(chorus_noise_handshake_t *hs, int initiator,
                                                  const uint8_t *prologue, size_t prologue_len,
                                                  const chorus_noise_keypair_t *s,
                                                  const chorus_noise_keypair_t *e)
{
    memset(hs, 0, sizeof(*hs));
    if (chorus_noise_setup() != CHORUS_NOISE_OK) {
        return CHORUS_NOISE_CRYPTO_FAILED;
    }
    hs->initiator = initiator ? 1 : 0;
    memcpy(hs->chaining_key, PROTOCOL_NAME, CHORUS_NOISE_HASH_LEN);
    memcpy(hs->hash, PROTOCOL_NAME, CHORUS_NOISE_HASH_LEN);
    hs->s = *s;
    hs->e = *e;
    return mix_hash(hs->hash, prologue, prologue_len);
}

void chorus_noise_handshake_clear(chorus_noise_handshake_t *hs)
{
    chorus_noise_cipher_clear(&hs->cipher);
    wipe(hs, sizeof(*hs));
}

/* Which of this side's keys and which of the peer's a DH token combines.
 * `es` is the initiator's e with the responder's s; `se` the initiator's s
 * with the responder's e. */
static chorus_noise_status_t mix_dh(chorus_noise_handshake_t *hs, const char *token)
{
    uint8_t shared[CHORUS_NOISE_KEY_LEN];
    const chorus_noise_keypair_t *mine;
    const uint8_t *theirs;
    if (strcmp(token, "ee") == 0) {
        mine = &hs->e;
        theirs = hs->re;
    } else if (strcmp(token, "es") == 0) {
        mine = hs->initiator ? &hs->e : &hs->s;
        theirs = hs->initiator ? hs->rs : hs->re;
    } else {
        mine = hs->initiator ? &hs->s : &hs->e;
        theirs = hs->initiator ? hs->re : hs->rs;
    }
    chorus_noise_status_t status = dh(mine, theirs, shared);
    if (status == CHORUS_NOISE_OK) {
        status = mix_key(hs, shared, sizeof(shared));
    }
    wipe(shared, sizeof(shared));
    return status;
}

/* The XX pattern's three messages, as token strings. */
static const char *const PATTERN[3][4] = {
    {"e", NULL, NULL, NULL},
    {"e", "ee", "s", "es"},
    {"s", "se", NULL, NULL},
};

/* Message n is written by the initiator when n is even (0 and 2). */
static int writes_message(const chorus_noise_handshake_t *hs, int n)
{
    return (n % 2 == 0) == (hs->initiator != 0);
}

chorus_noise_status_t chorus_noise_write_message(chorus_noise_handshake_t *hs,
                                                 const uint8_t *payload, size_t payload_len,
                                                 uint8_t *out, size_t out_len, size_t *written)
{
    if (hs->failed || hs->step > 2 || !writes_message(hs, hs->step)) {
        return CHORUS_NOISE_OUT_OF_ORDER;
    }
    size_t at = 0;
    chorus_noise_status_t status = CHORUS_NOISE_OK;
    for (int t = 0; t < 4 && PATTERN[hs->step][t] != NULL && status == CHORUS_NOISE_OK; t++) {
        const char *token = PATTERN[hs->step][t];
        if (strcmp(token, "e") == 0) {
            if (out_len - at < CHORUS_NOISE_KEY_LEN) {
                status = CHORUS_NOISE_BUFFER_TOO_SMALL;
                break;
            }
            memcpy(out + at, hs->e.public_key, CHORUS_NOISE_KEY_LEN);
            at += CHORUS_NOISE_KEY_LEN;
            status = mix_hash(hs->hash, hs->e.public_key, CHORUS_NOISE_KEY_LEN);
        } else if (strcmp(token, "s") == 0) {
            size_t n = 0;
            status = encrypt_and_hash(hs, hs->s.public_key, CHORUS_NOISE_KEY_LEN, out + at,
                                      out_len - at, &n);
            at += n;
        } else {
            status = mix_dh(hs, token);
        }
    }
    if (status == CHORUS_NOISE_OK) {
        size_t n = 0;
        status = encrypt_and_hash(hs, payload, payload_len, out + at, out_len - at, &n);
        at += n;
    }
    if (status == CHORUS_NOISE_OK && at > CHORUS_NOISE_MAX_MESSAGE_LEN) {
        status = CHORUS_NOISE_TOO_LONG;
    }
    if (status != CHORUS_NOISE_OK) {
        hs->failed = 1;
        return status;
    }
    hs->step++;
    *written = at;
    return CHORUS_NOISE_OK;
}

chorus_noise_status_t chorus_noise_read_message(chorus_noise_handshake_t *hs,
                                                const uint8_t *message, size_t message_len,
                                                uint8_t *payload, size_t payload_len,
                                                size_t *written)
{
    if (hs->failed || hs->step > 2 || writes_message(hs, hs->step)) {
        return CHORUS_NOISE_OUT_OF_ORDER;
    }
    size_t at = 0;
    chorus_noise_status_t status = CHORUS_NOISE_OK;
    for (int t = 0; t < 4 && PATTERN[hs->step][t] != NULL && status == CHORUS_NOISE_OK; t++) {
        const char *token = PATTERN[hs->step][t];
        if (strcmp(token, "e") == 0) {
            if (message_len - at < CHORUS_NOISE_KEY_LEN) {
                status = CHORUS_NOISE_BAD_LENGTH;
                break;
            }
            memcpy(hs->re, message + at, CHORUS_NOISE_KEY_LEN);
            at += CHORUS_NOISE_KEY_LEN;
            status = mix_hash(hs->hash, hs->re, CHORUS_NOISE_KEY_LEN);
        } else if (strcmp(token, "s") == 0) {
            size_t n = CHORUS_NOISE_KEY_LEN + (hs->cipher.has_key ? CHORUS_NOISE_TAG_LEN : 0);
            if (message_len - at < n) {
                status = CHORUS_NOISE_BAD_LENGTH;
                break;
            }
            size_t got = 0;
            status = decrypt_and_hash(hs, message + at, n, hs->rs, sizeof(hs->rs), &got);
            if (status == CHORUS_NOISE_OK && got != CHORUS_NOISE_KEY_LEN) {
                status = CHORUS_NOISE_BAD_LENGTH;
            }
            hs->have_rs = (status == CHORUS_NOISE_OK);
            at += n;
        } else {
            status = mix_dh(hs, token);
        }
    }
    if (status == CHORUS_NOISE_OK) {
        size_t rest = message_len - at;
        if (hs->cipher.has_key && rest < CHORUS_NOISE_TAG_LEN) {
            status = CHORUS_NOISE_BAD_LENGTH;
        } else {
            status = decrypt_and_hash(hs, message + at, rest, payload, payload_len, written);
        }
    }
    if (status != CHORUS_NOISE_OK) {
        hs->failed = 1;
        return status;
    }
    hs->step++;
    return CHORUS_NOISE_OK;
}

chorus_noise_status_t chorus_noise_split(chorus_noise_handshake_t *hs,
                                         chorus_noise_transport_t *out)
{
    memset(out, 0, sizeof(*out));
    if (hs->failed || hs->step != 3 || !hs->have_rs) {
        return CHORUS_NOISE_OUT_OF_ORDER;
    }
    uint8_t k1[CHORUS_NOISE_HASH_LEN];
    uint8_t k2[CHORUS_NOISE_HASH_LEN];
    chorus_noise_status_t status = hkdf2(hs->chaining_key, NULL, 0, k1, k2);
    chorus_noise_cipher_t first;
    chorus_noise_cipher_t second;
    memset(&first, 0, sizeof(first));
    memset(&second, 0, sizeof(second));
    if (status == CHORUS_NOISE_OK) {
        status = cipher_set_key(&first, k1);
    }
    if (status == CHORUS_NOISE_OK) {
        status = cipher_set_key(&second, k2);
    }
    wipe(k1, sizeof(k1));
    wipe(k2, sizeof(k2));
    if (status != CHORUS_NOISE_OK) {
        chorus_noise_cipher_clear(&first);
        chorus_noise_cipher_clear(&second);
        return status;
    }
    out->send = hs->initiator ? first : second;
    out->receive = hs->initiator ? second : first;
    memcpy(out->handshake_hash, hs->hash, CHORUS_NOISE_HASH_LEN);
    memcpy(out->remote_static, hs->rs, CHORUS_NOISE_KEY_LEN);
    chorus_noise_handshake_clear(hs);
    return CHORUS_NOISE_OK;
}

/* --- secure records --------------------------------------------------------- */

static void record_header(size_t payload_len, uint8_t header[CHORUS_FRAME_HEADER_LEN])
{
    header[0] = CHORUS_V2_SECURE_RECORD;
    header[1] = (uint8_t)(payload_len >> 8);
    header[2] = (uint8_t)(payload_len & 0xFF);
}

chorus_noise_status_t chorus_noise_seal_record(chorus_noise_cipher_t *cipher, const uint8_t *frames,
                                               size_t frames_len, uint8_t *out, size_t out_len,
                                               size_t *written)
{
    if (!cipher->has_key) {
        return CHORUS_NOISE_OUT_OF_ORDER;
    }
    if (frames_len > CHORUS_V2_MAX_RECORD_PLAINTEXT) {
        return CHORUS_NOISE_TOO_LONG;
    }
    size_t payload_len = frames_len + CHORUS_NOISE_TAG_LEN;
    if (out_len < CHORUS_FRAME_HEADER_LEN + payload_len) {
        return CHORUS_NOISE_BUFFER_TOO_SMALL;
    }
    record_header(payload_len, out);
    size_t n = 0;
    chorus_noise_status_t status =
        chorus_noise_encrypt(cipher, out, CHORUS_FRAME_HEADER_LEN, frames, frames_len,
                             out + CHORUS_FRAME_HEADER_LEN, out_len - CHORUS_FRAME_HEADER_LEN, &n);
    if (status != CHORUS_NOISE_OK) {
        return status;
    }
    *written = CHORUS_FRAME_HEADER_LEN + n;
    return CHORUS_NOISE_OK;
}

chorus_noise_status_t chorus_noise_open_record(chorus_noise_cipher_t *cipher,
                                               const uint8_t *ciphertext, size_t ciphertext_len,
                                               uint8_t *out, size_t out_len, size_t *written)
{
    if (!cipher->has_key) {
        return CHORUS_NOISE_OUT_OF_ORDER;
    }
    if (ciphertext_len > CHORUS_MAX_PAYLOAD_LEN) {
        return CHORUS_NOISE_TOO_LONG;
    }
    uint8_t header[CHORUS_FRAME_HEADER_LEN];
    record_header(ciphertext_len, header);
    return chorus_noise_decrypt(cipher, header, sizeof(header), ciphertext, ciphertext_len, out,
                                out_len, written);
}
