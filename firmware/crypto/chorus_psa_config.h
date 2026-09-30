/* The TF-PSA-Crypto configuration of the endpoint's HOST build.
 *
 * Just what chorus protocol v2's key exchange needs
 * (Noise_XX_25519_ChaChaPoly_SHA256, docs/protocol.md "The session"): X25519
 * through psa_raw_key_agreement, ChaCha20-Poly1305, SHA-256, and HMAC-SHA256
 * for Noise's HKDF. Everything else the library can do is left out.
 *
 * The library is compiled from the pinned ESP-IDF v6.1 tree
 * (components/mbedtls/mbedtls/tf-psa-crypto, TF-PSA-Crypto 1.1.0), never
 * copied into this repository; firmware/Makefile names the tree. The option
 * names are from that tree's public include/psa/crypto_config.h (read
 * 2026-09-30). The device build uses ESP-IDF's own mbedtls component and its
 * sdkconfig instead of this file, with the same firmware/src/noise.c. */

#ifndef CHORUS_PSA_CONFIG_H
#define CHORUS_PSA_CONFIG_H

#define TF_PSA_CRYPTO_CONFIG_VERSION 0x01000000

/* The four primitives. */
#define PSA_WANT_ALG_CHACHA20_POLY1305 1
#define PSA_WANT_ALG_ECDH 1
#define PSA_WANT_ALG_HMAC 1
#define PSA_WANT_ALG_SHA_256 1
#define PSA_WANT_ECC_MONTGOMERY_255 1

/* The key types those use. */
#define PSA_WANT_KEY_TYPE_CHACHA20 1
#define PSA_WANT_KEY_TYPE_HMAC 1
#define PSA_WANT_KEY_TYPE_ECC_PUBLIC_KEY 1
#define PSA_WANT_KEY_TYPE_ECC_KEY_PAIR_BASIC 1
#define PSA_WANT_KEY_TYPE_ECC_KEY_PAIR_IMPORT 1
#define PSA_WANT_KEY_TYPE_ECC_KEY_PAIR_EXPORT 1

/* The core, with the host's own entropy source (getrandom on Linux) behind
 * the library's random generator. */
#define MBEDTLS_PSA_CRYPTO_C
#define MBEDTLS_PSA_BUILTIN_GET_ENTROPY
#define MBEDTLS_HMAC_DRBG_C
#define MBEDTLS_MD_C
#define MBEDTLS_PLATFORM_C

/* No heap on the audio path: the key store is a fixed table, and a record's
 * buffers are not copied into heap for each call (the endpoint owns every
 * buffer it hands the library). */
#define MBEDTLS_PSA_STATIC_KEY_SLOTS
#define MBEDTLS_PSA_KEY_SLOT_COUNT 8
#define MBEDTLS_PSA_ASSUME_EXCLUSIVE_BUFFERS

#endif /* CHORUS_PSA_CONFIG_H */
