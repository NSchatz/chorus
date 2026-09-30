# 0039: the v2 key exchange is Noise XX with trust-on-first-use pins, over vendored primitives

- Status: accepted (goal 5, 2026-09-30)
- Decided by: the goal, on decisions K62 (encrypted speaker sessions), K92 (auto-adoption, the key
  pinned at adoption, a changed key refused and surfaced) and BRIEF.md 3.2 (crypto primitives are
  platform: vendored, not built)
- Implemented in: `crates/protocol/src/v2/noise.rs`, `crates/protocol/src/v2/session.rs`,
  `crates/protocol/src/v2/adoption.rs`; specified in `docs/protocol.md`, "The session: encryption
  with adoption"; held by `fixtures/protocol/v2/` and `crates/protocol/tests/v2_vectors.rs`,
  `crates/protocol/tests/noise_vector.rs`

## Context

Protocol v2 encrypts every speaker session and authenticates both ends without a person in the
loop: a speaker that appears on the audio network is adopted automatically (K92), its long-term
key is pinned then, and a later handshake with another key under the same identity is refused and
surfaced. The endpoint is an ESP32-S3 on ESP-IDF's Mbed TLS (P1, goal 6); the server and the
Linux client are Rust. The construction must be one both can implement and hold to the same
golden vectors, and the brief names two candidates: a Noise pattern or TLS with a pre-shared key.

## What was read

All read 2026-09-30; the full notes are `docs/research/2026-09-protocol-v2-sources.md`.

- The Noise Protocol Framework, revision 34 (2018-07-11), <https://noiseprotocol.org/noise.html>:
  section 7.5 (the XX pattern `-> e; <- e, ee, s, es; -> s, se`), section 5 (the state objects),
  section 4.3 (HKDF), section 12 (25519, ChaChaPoly, SHA256).
- cacophony's test vectors, <https://raw.githubusercontent.com/haskell-cryptography/cacophony/master/vectors/cacophony.txt>
  (Unlicense; file sha256 `3bde7c09a6f349ee11c825c50fcc02649f8f02a47c857a459206b357f9386cae`).
- RFC 8446 (TLS 1.3), <https://www.rfc-editor.org/rfc/rfc8446.txt>: section 2.2 ("PSKs can be
  used with (EC)DHE key exchange in order to provide forward secrecy"), section 4.2.9
  (`psk_dhe_ke`), section 4.2.11 (`pre_shared_key`).
- Mbed TLS's TLS 1.3 support notes,
  <https://raw.githubusercontent.com/Mbed-TLS/mbedtls/development/docs/architecture/tls13-support.md>:
  external PSKs are supported; the `client_certificate_type` and `server_certificate_type`
  extensions (RFC 7250 raw public keys) are "no".
- ESP-IDF's Kconfig reference (v6.1),
  <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/kconfig-reference.html>:
  `CONFIG_MBEDTLS_SSL_PROTO_TLS1_3` default off; `CONFIG_MBEDTLS_CHACHAPOLY_C` default off;
  `CONFIG_MBEDTLS_ECP_DP_CURVE25519_ENABLED` and `CONFIG_MBEDTLS_SHA256_C` default on.
- crates.io API, `https://crates.io/api/v1/crates/<name>`, for the versions and licences below.

## Options

1. **TLS 1.3 with certificates.** Needs a certificate authority, issuance at adoption and
   expiry handling on speakers; "adopt automatically, pin the key" becomes "run a CA". Rejected:
   the machinery is the opposite of K92's zero-touch adoption.
2. **TLS 1.3 with an external PSK (`psk_dhe_ke`).** Forward secrecy through ECDHE, available in
   Mbed TLS. But a PSK is a shared symmetric secret that has to reach both sides before the first
   session, which is exactly what trust on first use does not have: the first contact would have
   to hand the secret over in the clear, or a person would have to type it (K92 declined PIN and
   button pairing). Raw public keys (RFC 7250), the certificate-less TLS form that would fit, are
   not in Mbed TLS. Rejected.
3. **Noise IK or KK.** Both need the peer's static key in advance (the initiator must know the
   responder's key), so neither serves a first contact. Rejected.
4. **Noise XX.** Mutual authentication by long-term static keys that each side learns during the
   handshake, both encrypted (the endpoint's key is hidden from a passive observer), forward
   secrecy from the ephemeral keys, three messages, no certificates and no shared secret. The pin
   is simply "the static key this id presented first". Every primitive it needs (X25519,
   ChaCha20-Poly1305, SHA-256, HMAC) is in ESP-IDF's Mbed TLS (ChaChaPoly must be switched on in
   sdkconfig, goal 6), and the handshake fits in chorus's own frames. **Chosen.**

## Decision

- **Construction:** `Noise_XX_25519_ChaChaPoly_SHA256`, the endpoint as initiator. The prologue is
  the 7 bytes of `handshake_init` before the Noise message (magic, version, suite), so a version
  or suite changed in transit fails the handshake instead of being negotiated down. Message 2
  carries the server's id and message 3 the endpoint's, both encrypted.
- **The state machine is chorus's, the primitives are vendored.** Noise's state machine is about
  300 lines and is what the C endpoint must mirror (over Mbed TLS) byte for byte, so it is built,
  in `crates/protocol/src/v2/noise.rs`, and held to the published cacophony vector in both roles.
  The crypto primitives are not built (BRIEF.md 3.2).
- **Adoption is trust on first use, keyed by id.** A new id's key is pinned; the same key proceeds;
  another key is refused with `session_refused` `key_changed`, recorded in the store's list of
  refused changes, logged with the id and both fingerprints, and the old pin stays. Only the
  owner's act (forgetting the endpoint) lets a new key in. The endpoint pins its server the same
  way. A refusal travels in the clear and is information only: nothing a peer sends moves a pin.
- **Keys** are 32 bytes from the operating system's random source; one ephemeral key per
  handshake. Test vectors use public test keys that their `.fields` files list and say must never
  be used for a device. No agent creates or stores a key for a real device: keys are generated by
  the software on the device that owns them.
- **Dependencies** (exact pins in `crates/protocol/Cargo.toml`; this is chorus's first move off
  "zero external crates", which K95 allows by record, and the crates are primitives BRIEF.md 3.2
  already calls platform):

  | crate | version | licence | why |
  |---|---|---|---|
  | `x25519-dalek` (with `curve25519-dalek` 5.0.0) | 3.0.0 | BSD-3-Clause | X25519 |
  | `chacha20poly1305` | 0.11.0 | Apache-2.0 OR MIT | the AEAD |
  | `sha2` | 0.11.0 | MIT OR Apache-2.0 | SHA-256 |
  | `hmac` | 0.13.0 | MIT OR Apache-2.0 | HMAC-SHA256 for Noise's HKDF |

  All are on the licence allowlist (`deny.toml`); `cargo tree -d` shows one version each of
  `rand_core` (0.10.1), `digest` (0.11.3) and `cipher` (0.5.2); every one declares
  `rust-version` 1.85, below the pinned 1.98.1. Versions and licences from crates.io, read
  2026-09-30. Randomness in the server and client comes from `/dev/urandom` through `std`, so no
  random-number crate is added.

## Consequences

- The first contact is the trust decision: an attacker present at an endpoint's first handshake
  could be adopted in its place. That is K92's accepted trade (auto-adopt on the LAN), and the app
  and Home Assistant show each adoption (goals 14, 18, 21), so an unexpected one is visible.
- A replaced speaker board, or a re-flashed key, is refused until the owner forgets the old
  endpoint. That is the point of the pin, and the refusal names what to do.
- The C endpoint (goal 6) implements the same state machine over Mbed TLS and must pass the same
  vectors; `CONFIG_MBEDTLS_CHACHAPOLY_C` goes on in its sdkconfig then.
- The primitives are upgraded only as their own commit saying why, and the golden vectors must
  still pass unchanged.
