# 0043: the endpoint's key exchange is chorus's own state machine over the PSA Crypto API, and its host build compiles the pinned ESP-IDF tree's library

- Status: accepted (goal 6, 2026-09-30)
- Decided by: the goal (the endpoint speaks protocol v2), on ADR 0039 (Noise XX, the primitives
  are platform) and brief section 3.2 (crypto primitives are vendored or platform, never written)
- Implemented in: `firmware/src/noise.c`, `firmware/crypto/chorus_psa_config.h`,
  `firmware/Makefile` (the `tf-psa-crypto` library), `firmware/src/session.c`

## Context

ADR 0039 chose `Noise_XX_25519_ChaChaPoly_SHA256` and held the Rust side to the cacophony vector.
The C endpoint needs the same four primitives (X25519, ChaCha20-Poly1305, SHA-256,
HMAC-SHA256) on the ESP32-S3, where ESP-IDF v6.1 ships Mbed TLS 4 with TF-PSA-Crypto 1.1.0, and
on the host, where `make firmware-check` grades the endpoint with no device. The same C has to
run in both places, or the host grades code the board does not run.

## What was read

- The Noise Protocol Framework, revision 34, sections 4.3, 5, 7.5 and 12,
  <https://noiseprotocol.org/noise.html>, read 2026-09-30 (public domain).
- TF-PSA-Crypto 1.1.0 in the pinned ESP-IDF v6.1 tree (commit
  fff9895c82d744c7237be8847347bdd1b07c6643): the public headers `include/psa/crypto.h`,
  `include/psa/crypto_config.h` (the option names and their documentation) and
  `docs/`, read as Apache-2.0, 2026-09-30. The library's sources are compiled, not read, apart
  from one include line of `drivers/builtin/src/bignum.c` that a compile error pointed at.
- ESP-IDF v6.1's `components/mbedtls/CMakeLists.txt`, `Kconfig` ("Stream Cipher": ChaCha20 and
  ChaCha20-Poly1305 default off) and `port/include/mbedtls/bignum.h` (Apache-2.0), read
  2026-09-30.
- `crates/protocol/src/v2/` and `fixtures/protocol/v2/` in this repository.

## Decision

1. **The state machine is chorus's; the primitives are reached only through PSA.**
   `firmware/src/noise.c` implements CipherState, SymmetricState and HandshakeState for the XX
   pattern (both roles, so the published vector is checked in both) and the record seal and
   open, and calls nothing but `psa/crypto.h`: `psa_raw_key_agreement` with
   `PSA_ALG_ECDH` on a `PSA_ECC_FAMILY_MONTGOMERY` key, `psa_aead_encrypt`/`decrypt` with
   `PSA_ALG_CHACHA20_POLY1305`, `psa_hash_*` with SHA-256, `psa_mac_compute` with HMAC-SHA256.
   Options considered: (a) the legacy `mbedtls_*` APIs, whose bignum and ECP headers
   TF-PSA-Crypto 1.1.0 keeps under `mbedtls/private/` (seen in the pinned tree); (b) a vendored small library (Monocypher-style) beside the platform's, a second copy
   of the primitives on the board; (c) PSA only. (c) is chosen: it is the one interface both
   ESP-IDF v6.1 and a host build have, so the seam between the state machine and the library is
   one header.
2. **The host build compiles TF-PSA-Crypto from the pinned ESP-IDF v6.1 tree, never a copy.**
   `firmware/Makefile` builds `libchorus-tf-psa-crypto.a` from
   `$(CHORUS_IDF_V61_DIR)/components/mbedtls/mbedtls/tf-psa-crypto` (default
   `/cache/esp/esp-idf-v6.1`; in CI, the v6.1 clone the image build uses) with
   `firmware/crypto/chorus_psa_config.h`, which enables the four primitives and nothing else,
   and refuses by name a tree that is absent or not at the pinned commit. Two one-line wrapper
   headers stand in for ESP-IDF's `port/include/mbedtls/{bignum,ecp}.h`. Options considered:
   (a) vendor the library into `firmware/`, which duplicates a large, undifferentiated library
   the pinned tree already carries and would put crypto source under the scans' tree;
   (b) the Mbed TLS the host distribution packages, a different version from the board's;
   (c) the pinned tree. (c) is chosen: the host grades the same library version the board links.
3. **No heap on the audio path.** The host configuration uses a fixed key-slot table
   (`MBEDTLS_PSA_STATIC_KEY_SLOTS`) and `MBEDTLS_PSA_ASSUME_EXCLUSIVE_BUFFERS` (no per-call
   buffer copies); a record is opened out of the receive buffer into a separate plaintext
   buffer, never over it. The transport keys are imported once per session.
4. **Randomness and storage are seams.** Ephemeral and first-start keys come from a
   `chorus_noise_random_fn` (default `psa_generate_random`), so tests inject the vectors'
   fixed keys. On the host the endpoint's long-term key and its server pins are files (`--key`,
   `--server-pins`, the adoption store's text form); the image passes neither yet, so it makes a
   key for each boot and keeps pins in memory until its storage (NVS) is decided.
5. **A changed server key ends the run.** The endpoint sends `session_refused` `key_changed`
   and stops, rather than retrying as if it were an outage: only the owner changes a pin
   (docs/protocol.md, "Adoption").

## Consequences

- `firmware/tests/test_noise.c` reproduces the cacophony vector in both roles and the four
  session vectors from their public keys; `firmware/tests/test_protocol_v2.c` every v2 vector.
- `make firmware-check` needs the ESP-IDF v6.1 tree (the mbedtls submodule at least) on the
  host, as the image build needs the whole toolchain.
- The image needs `CONFIG_MBEDTLS_CHACHA20_C` and `CONFIG_MBEDTLS_CHACHAPOLY_C`
  (`firmware/sdkconfig.defaults`) and the `mbedtls` component.
- Until the image has persistent storage, a rebooted board presents a new key under the same
  id, which a server that adopted it refuses (`key_changed`) until the owner forgets it. That
  is the pin working; it is also why storage is the next step for the image.
