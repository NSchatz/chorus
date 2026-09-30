# Protocol v2 sources: Noise, TLS PSK, the crypto crates, FLAC, Opus and channel orders

Research for goal 5 (protocol v2), 2026-09-30, by a research agent with the lead's questions;
what `docs/protocol.md` v2 and `docs/decisions/0000-the-v2-key-exchange.md` rest on. Paths under
`/cache/tmp/chorus-g5/` were the agent's working files (not in the repository); the Noise vector
it extracted is committed as `fixtures/protocol/v2/noise/cacophony_xx.fields`.

Date of research: 2026-09-30. All URLs below were read on 2026-09-30.
Labels: CONFIRMED (quoted from the primary source), VERIFIED (independently
recomputed here), ASSUMED (reasoned, not quoted), LEAD (not confirmed; where to look).
Clean-room note: no GPL source file was opened. Sources read are specs, RFCs, docs,
crates.io metadata, and permissively licensed data files (cacophony: Unlicense; snow:
Apache-2.0). The Linux kernel patch that search results surfaced for CEA channel
allocation was NOT opened.

---

## 1. Noise Protocol Framework spec

URL: https://noiseprotocol.org/noise.html (read 2026-09-30; also PDF linked from it as noise.pdf)

- Revision and date (CONFIRMED): "Revision: 34", "Date: 2018-07-11",
  "Status: official/unstable". Author Trevor Perrin.

- XX pattern (CONFIRMED, section 7.5 interactive patterns):
  ```
  XX:
    -> e
    <- e, ee, s, es
    -> s, se
  ```
  (Caution: the spec also shows a different "XX" block in section 7.2 "Alice and Bob",
  in Bob-initiated form: "<- e / -> e, ee, s, se / <- s, es". The canonical
  (Alice-initiated) fundamental pattern is the one above, from section 7.5
  "Interactive handshake patterns (fundamental)".)

- Token processing (CONFIRMED, section 5.3 WriteMessage):
  "For "e": Sets e (which must be empty) to GENERATE_KEYPAIR(). Appends e.public_key
  to the buffer. Calls MixHash(e.public_key)." "For "s": Appends
  EncryptAndHash(s.public_key) to the buffer." "For "ee": Calls MixKey(DH(e, re))."
  "For "es": Calls MixKey(DH(e, rs)) if initiator, MixKey(DH(s, re)) if responder."
  "For "se": Calls MixKey(DH(s, re)) if initiator, MixKey(DH(e, rs)) if responder."
  Then "Appends EncryptAndHash(payload) to the buffer." and "If there are no more
  message patterns returns two new CipherState objects by calling Split()."

- HKDF (CONFIRMED, section 4.3):
  ```
  HKDF(chaining_key, input_key_material, num_outputs):
    temp_key = HMAC-HASH(chaining_key, input_key_material)
    output1  = HMAC-HASH(temp_key, byte(0x01))
    output2  = HMAC-HASH(temp_key, output1 || byte(0x02))
    if num_outputs == 2: return (output1, output2)
    output3  = HMAC-HASH(temp_key, output2 || byte(0x03))
    return (output1, output2, output3)
  ```
  Quote: "the HKDF() function is simply HKDF from [4] with the chaining_key as HKDF
  salt, and zero-length HKDF info." input_key_material length is "either zero bytes,
  32 bytes, or DHLEN bytes". For SHA256, HASHLEN = 32, HMAC-HASH = HMAC-SHA256.

- InitializeSymmetric (CONFIRMED): "If protocol_name is less than or equal to HASHLEN
  bytes in length, sets h equal to protocol_name with zero bytes appended to make
  HASHLEN bytes. Otherwise sets h = HASH(protocol_name). Sets ck = h. Calls
  InitializeKey(empty)." Note: "Noise_XX_25519_ChaChaPoly_SHA256" is 32 bytes, so
  h = the ASCII bytes with no hashing and no padding (VERIFIED by recomputation below).

- MixKey (CONFIRMED): "Sets ck, temp_k = HKDF(ck, input_key_material, 2). If HASHLEN
  is 64, then truncates temp_k to 32 bytes. Calls InitializeKey(temp_k)."
  (InitializeKey sets k and resets n = 0.)
- MixHash (CONFIRMED): "Sets h = HASH(h || data)."
- MixKeyAndHash (PSK only, CONFIRMED): "Sets ck, temp_h, temp_k = HKDF(ck,
  input_key_material, 3). Calls MixHash(temp_h). ..."
- EncryptAndHash (CONFIRMED): "Sets ciphertext = EncryptWithAd(h, plaintext), calls
  MixHash(ciphertext), and returns ciphertext. Note that if k is empty, the
  EncryptWithAd() call will set ciphertext equal to plaintext."
- Split (CONFIRMED): "Sets temp_k1, temp_k2 = HKDF(ck, zerolen, 2). If HASHLEN is 64,
  then truncates temp_k1 and temp_k2 to 32 bytes. Creates two new CipherState objects
  c1 and c2. Calls c1.InitializeKey(temp_k1) and c2.InitializeKey(temp_k2). Returns
  the pair (c1, c2)." c1 is initiator-to-responder, c2 responder-to-initiator
  (VERIFIED against the test vector, section 2).
- GetHandshakeHash (CONFIRMED): "Returns h ... after the Split() function has been
  called. This function is used for channel binding".

- Protocol name (CONFIRMED, section 8): "concatenate the ASCII string "Noise_" with four
  underscore-separated name sections which sequentially name the handshake pattern,
  the DH functions, the cipher functions, and then the hash functions. The resulting
  name must be 255 bytes or less." Examples given: Noise_XX_25519_AESGCM_SHA256,
  Noise_N_25519_ChaChaPoly_BLAKE2s. Our name: Noise_XX_25519_ChaChaPoly_SHA256.

- Prologue (CONFIRMED, section 6 and Initialize in 5.3): Initialize "Calls
  MixHash(prologue)" immediately after InitializeSymmetric. "If both parties do not
  provide identical prologue data, the handshake will fail due to a decryption error."
  "while the parties confirm their prologues are identical, they don't mix prologue
  data into encryption keys." Security note (section 14 area): "If parties decide on
  a Noise protocol based on some previous negotiation that is not included as
  prologue, then a rollback attack might be possible." Implication for chorus: put the
  protocol/version negotiation bytes in the prologue.

- ChaChaPoly nonce (CONFIRMED, section 12.3): "AEAD_CHACHA20_POLY1305 from [8]. The
  96-bit nonce is formed by encoding 32 bits of zeros followed by little-endian
  encoding of n." (Contrast AESGCM: big-endian n.) Nonce limits (section 5.1): "The
  maximum n value (2^64-1) is reserved for other use." Transport messages use
  zero-length associated data.

- Max message length (CONFIRMED, section 3): "All Noise messages are less than or
  equal to 65535 bytes in length." A transport message "consists of an encrypted
  payload plus 16 bytes of authentication data", so max plaintext per transport
  message = 65535 - 16 = 65519 bytes (ASSUMED arithmetic). Framing (section 13):
  "If an explicit length field is needed, applications are recommended to add a
  16-bit big-endian length field prior to each message."

- 25519 DH (CONFIRMED, 12.1): "Executes the Curve25519 DH function (aka "X25519" in
  [7]). Invalid public key values will produce an output of all zeros." All-zero
  detection is allowed but "discouraged".

## 2. Noise test vectors with Noise_XX_25519_ChaChaPoly_SHA256

Sources found:
| Source | Location | Licence | Has XX_25519_ChaChaPoly_SHA256 | handshake_hash field |
|---|---|---|---|---|
| cacophony (Haskell) | https://github.com/haskell-cryptography/cacophony/blob/master/vectors/cacophony.txt | Unlicense (GitHub API license.spdx_id) | yes, exactly 1 (no PSK, no fallback) | yes |
| snow (Rust) | https://github.com/mcginty/snow/tree/main/tests/vectors (cacophony.txt copy, snow.txt, snow-extended.txt) | Apache-2.0 (repo licence per GitHub API) | yes in snow.txt | NO (snow.txt entries have init_psks/resp_psks, no handshake_hash) |
| noise-c | https://github.com/rweather/noise-c/tree/master/tests/vector (cacophony.txt, noise-c-basic.txt, noise-c-fallback.txt, noise-c-hybrid.txt) | MIT | LEAD (listed files only, contents not inspected) | LEAD |

Chosen file (raw URL):
https://raw.githubusercontent.com/haskell-cryptography/cacophony/master/vectors/cacophony.txt
- Downloaded to /cache/tmp/chorus-g5/noise-vectors/cacophony.txt
- sha256 3bde7c09a6f349ee11c825c50fcc02649f8f02a47c857a459206b357f9386cae (1709817 bytes)
- Last commit touching it: 18b7348c54fd61fcd0c220298883de0d09c8364d (2018-12-16)
- Despite the .txt extension it is JSON: {"vectors": [ ... ]}.

Also downloaded (for reference): https://raw.githubusercontent.com/mcginty/snow/main/tests/vectors/snow.txt
to /cache/tmp/chorus-g5/noise-vectors/snow.txt, sha256
69da433305fd045f6c9f01b656662a389d022688986fd39fbe7af009cd402fd3 (last commit
d00b360cc61a7fe519ce7539974dca4f36c4654a, 2025-03-04).

Extracted vector: /cache/tmp/chorus-g5/noise-vectors/xx_25519_chachapoly_sha256.json
(sha256 7ce847fcabba304eb8ba66352ae082f71e201c63e45d4f2e7e5fbe313489a8af), produced by
`jq '.vectors[] | select(.protocol_name=="Noise_XX_25519_ChaChaPoly_SHA256")' cacophony.txt`.

Field names (exact; note: prologue is split into init_prologue and resp_prologue, there
is no bare "prologue" key): protocol_name, init_prologue, init_static, init_ephemeral,
resp_prologue, resp_static, resp_ephemeral, handshake_hash, messages[] of
{payload, ciphertext}. Keys are X25519 PRIVATE keys (32 bytes hex); public keys are
derived.

Values:
- init_prologue = resp_prologue = 4a6f686e2047616c74 ("John Galt")
- init_static    = e61ef9919cde45dd5f82166404bd08e38bceb5dfdfded0a34c8df7ed542214d1
- init_ephemeral = 893e28b9dc6ca8d611ab664754b8ceb7bac5117349a4439a6b0569da977c464a
- resp_static    = 4a3acbfdb163dec651dfa3194dece676d437029c62a408b4c5ea9114246e4893
- resp_ephemeral = bbdb4cdbd309f1a1f2e1456967fe288cadd6f712d65dc7b7793d5e63da6b375b
- handshake_hash = c8e5f64e846193be2a834104c2a009868d6c9f3bd3c186299888b488b2f1f58e
- messages: 6 entries; 0..2 are the three handshake messages, 3..5 are transport.

VERIFIED: an independent Python implementation written from the spec text above
(HKDF/MixKey/MixHash/EncryptAndHash/Split; X25519 and ChaCha20-Poly1305 from the
`cryptography` package), script saved as
/cache/tmp/chorus-g5/noise-vectors/verify_xx.py, reproduces all 3 handshake
ciphertexts and the handshake_hash exactly. Transport message direction convention in
this vector (VERIFIED): messages strictly alternate starting with the initiator, so
message 3 is responder to initiator (c2, n=0), message 4 initiator to responder (c1,
n=0), message 5 responder to initiator (c2, n=1). A C or Rust fixture runner must
follow the same even-index = initiator rule.

## 3. TLS 1.3 PSK and certificate-less options vs Noise, for an ESP-IDF mbedTLS endpoint

RFC 8446, https://www.rfc-editor.org/rfc/rfc8446.txt (read 2026-09-30):
- Section 2.2 (CONFIRMED): "Although TLS PSKs can be established out of band, PSKs can
  also be established in a previous connection ..." and "PSKs can be used with (EC)DHE
  key exchange in order to provide forward secrecy in combination with shared keys, or
  can be used alone, at the cost of losing forward secrecy for the application data."
- Section 4.2.9 (CONFIRMED): `enum { psk_ke(0), psk_dhe_ke(1), (255) }
  PskKeyExchangeMode;` "psk_ke: PSK-only key establishment. In this mode, the server
  MUST NOT supply a "key_share" value." "psk_dhe_ke: PSK with (EC)DHE key
  establishment."
- Section 4.2.11 (CONFIRMED): "The "pre_shared_key" extension is used to negotiate the
  identity of the pre-shared key to be used with a given handshake in association with
  PSK key establishment." Structures PskIdentity { identity<1..2^16-1>;
  obfuscated_ticket_age }, PskBinderEntry<32..255>. External PSK binder label is
  "ext binder" (section 7.1).
- Section 9.1 (CONFIRMED): MUST implement TLS_AES_128_GCM_SHA256, "SHOULD implement the
  TLS_AES_256_GCM_SHA384 [GCM] and TLS_CHACHA20_POLY1305_SHA256 [RFC8439]", "MUST
  support key exchange with secp256r1 (NIST P-256) and SHOULD support key exchange with
  X25519".
- Certificate-less identity options: (a) external PSK with psk_dhe_ke (symmetric
  per-device secret, forward secrecy via ECDHE); (b) raw public keys, RFC 7250
  (client_certificate_type(19)/server_certificate_type(20) extensions, referenced from
  RFC 8446). Guidance on external PSK usage is RFC 9257 (LEAD, not read).

Mbed TLS, docs/architecture/tls13-support.md,
https://raw.githubusercontent.com/Mbed-TLS/mbedtls/development/docs/architecture/tls13-support.md
(read 2026-09-30; Mbed TLS is dual Apache-2.0 OR GPL-2.0-or-later per its README, so
permissive terms apply; latest GitHub release tag mbedtls-4.2.0, 2026-07-07):
- CONFIRMED: "Mbed TLS supports pre-shared keys for key establishment, pre-shared keys
  provisioned externally as well as provisioned via the ticket mechanism."
- CONFIRMED cipher suites: "TLS_AES_128_GCM_SHA256, TLS_AES_256_GCM_SHA384,
  TLS_CHACHA20_POLY1305_SHA256, TLS_AES_128_CCM_SHA256 and TLS_AES_128_CCM_8_SHA256."
- CONFIRMED groups: "secp256r1, x25519, secp384r1, x448 and secp521r1."
- CONFIRMED extension table: pre_shared_key YES, psk_key_exchange_modes YES,
  key_share YES, early_data YES, client_certificate_type "no",
  server_certificate_type "no". So RFC 7250 raw public keys are NOT supported in Mbed
  TLS TLS 1.3: the only certificate-less TLS 1.3 option on this stack is external PSK.
- CONFIRMED build options: MBEDTLS_SSL_TLS1_3_KEY_EXCHANGE_MODE_PSK_ENABLED ("If it is
  the only key exchange mode enabled, the TLS 1.3 implementation does not contain any
  code related to key exchange protocols, certificates and signatures."),
  ..._EPHEMERAL_ENABLED, ..._PSK_EPHEMERAL_ENABLED ("does not contain any code related
  to certificates and signatures").

ESP-IDF docs (page reports ESP-IDF v6.1):
- https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/protocols/mbedtls.html
  CONFIRMED: "TLS 1.3 is fully supported starting Mbed TLS v3.6.0 release";
  CONFIG_MBEDTLS_SSL_PROTO_TLS1_3; hardware SHA, AES, MPI acceleration.
  The bundled Mbed TLS exact version in ESP-IDF v6.1: LEAD (page says "v3.x.x series"
  in the fetched summary; confirm in the ESP-IDF release notes).
- https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/kconfig-reference.html
  CONFIRMED Kconfig entries and defaults:
  CONFIG_MBEDTLS_SSL_PROTO_TLS1_3 "Support TLS 1.3 protocol", Default No (disabled);
  CONFIG_MBEDTLS_SSL_TLS1_3_KEXM_PSK, _KEXM_PSK_EPHEMERAL, _KEXM_EPHEMERAL each
  "Default value: Yes (enabled) if CONFIG_MBEDTLS_SSL_PROTO_TLS1_3";
  CONFIG_MBEDTLS_CHACHA20_C "Default value: No (disabled)";
  CONFIG_MBEDTLS_CHACHAPOLY_C "ChaCha20-Poly1305 AEAD algorithm", Default No;
  CONFIG_MBEDTLS_ECP_DP_CURVE25519_ENABLED "Enable CURVE25519 curve", Default Yes;
  CONFIG_MBEDTLS_SHA256_C Default Yes; CONFIG_MBEDTLS_HARDWARE_SHA "Enable hardware
  accelerated SHA1, SHA256, SHA384 & SHA512 in mbedTLS" (with an ESP32 caveat about
  concurrent digests). No HKDF Kconfig entry was found on the page (HKDF availability
  as a public API: LEAD; in Mbed TLS 3.x it is MBEDTLS_HKDF_C / PSA_ALG_HKDF, not
  confirmed here).
- Net for chorus (ASSUMED synthesis): every primitive Noise_XX_25519_ChaChaPoly_SHA256
  needs (X25519, ChaCha20-Poly1305, SHA-256, HMAC) is available in ESP-IDF's Mbed TLS,
  but ChaCha20 and ChaChaPoly are off by default and must be enabled in sdkconfig.
  TLS 1.3 on ESP-IDF gives a certificate-less path only via external PSK
  (psk_dhe_ke recommended for forward secrecy); raw public keys are not available.
  Noise XX gives mutual static-key authentication without certificates or a shared
  secret, and the handshake is small enough to be carried in chorus's own framing.

## 4. Rust crates: versions, licences, MSRV, compatibility

Source: https://crates.io/api/v1/crates/<name> (read 2026-09-30 via curl).

| crate | max_stable_version | licence | rust_version (MSRV) | released | notes |
|---|---|---|---|---|---|
| x25519-dalek | 3.0.0 | BSD-3-Clause | 1.85 | 2026-07-06 | pre-releases 3.0.0-pre.4..pre.6, 3.0.0-rc.0, rc.1 exist; 2.x was prior major |
| curve25519-dalek | 5.0.0 | BSD-3-Clause | 1.85.0 | 2026-07-06 | 5.0.0-pre.*, rc.0, rc.1 exist |
| chacha20poly1305 | 0.11.0 | Apache-2.0 OR MIT | 1.85 | 2026-06-28 | 0.11.0-rc.0..rc.3 existed |
| sha2 | 0.11.0 | MIT OR Apache-2.0 | 1.85 | 2026-03-25 | |
| hmac | 0.13.0 | MIT OR Apache-2.0 | 1.85 | 2026-03-29 | |
| getrandom | 0.4.3 | MIT OR Apache-2.0 | 1.85 | 2026-06-17 | 0.2.17 (2026-01-11) still maintained for old line |
| zeroize | 1.9.0 | Apache-2.0 OR MIT | 1.85 | 2026-06-12 | 1.8.2 had MSRV 1.60 |
| rand_core | 0.10.1 | MIT OR Apache-2.0 | 1.85 | 2026-04-13 | (transitive) |
| digest | 0.11.3 | MIT OR Apache-2.0 | 1.85 | 2026-05-03 | 0.11.0 and 0.11.1 are yanked |
| cipher | 0.5.2 | MIT OR Apache-2.0 | 1.85 | 2026-05-19 | 0.5.0 yanked |

No pre-release major newer than the stable ones above exists at the top of each
crate's version list (max_version == max_stable_version for all).

Compatibility test (VERIFIED): throwaway project /cache/tmp/chorus-g5/deptest,
Cargo.toml pins =3.0.0 x25519-dalek (features static_secrets, zeroize), =5.0.0
curve25519-dalek, =0.11.0 chacha20poly1305, =0.11.0 sha2, =0.13.0 hmac, =0.4.3
getrandom, =1.9.0 zeroize; edition 2024; toolchain `mise exec rust@1.98.1`
(rustc 1.98.1 48a229cea 2026-09-01). `cargo generate-lockfile` locked 35 packages;
`cargo check` succeeded. Cargo.lock sha256
4598ad48fae4d6d6ca5b47a9e349659fe43332beebc16130cb3cd4f917f13411.
- `cargo tree -d -e normal` and `cargo tree -d --target all`: "nothing to print", i.e.
  zero duplicate crates.
- Single versions resolved: rand_core 0.10.1, digest 0.11.3, cipher 0.5.2,
  crypto-common 0.2.2, hybrid-array 0.4.15 (replaces generic-array), aead 0.6.1,
  chacha20 0.10.2, poly1305 0.9.1, universal-hash 0.6.1, block-buffer 0.12.1,
  subtle 2.6.1, zeroize 1.9.0, cpufeatures 0.3.1, getrandom 0.4.3 (pulled in by
  crypto-common too).
- Full tree saved at /cache/tmp/chorus-g5/deptest/tree.txt.
- Caveat (ASSUMED): mixing with the older line (x25519-dalek 2.x / curve25519-dalek
  4.x, which use rand_core 0.6) would duplicate rand_core; stay on the new line.
- Caveat (LEAD): no embedded target (xtensa-esp32 or riscv32imc/imac) build was tried;
  getrandom 0.4 on ESP-IDF needs a backend check.

## 5. FLAC STREAMINFO

URL: https://www.rfc-editor.org/rfc/rfc9639.txt (RFC 9639, section 8.2 "Streaminfo";
read 2026-09-30).
- CONFIRMED: "It MUST be present as the first metadata block in the stream. Other
  metadata blocks MAY follow. There MUST be no more than one streaminfo metadata block
  per FLAC stream."
- Layout (CONFIRMED, Table 3, "excluding the metadata block header"), big-endian bit
  packing:
  | bits | field |
  |---|---|
  | u(16) | minimum block size (samples), excluding the last block |
  | u(16) | maximum block size (samples) |
  | u(24) | minimum frame size (bytes), 0 = unknown |
  | u(24) | maximum frame size (bytes), 0 = unknown |
  | u(20) | sample rate in Hz |
  | u(3) | (number of channels)-1; "FLAC supports from 1 to 8 channels" |
  | u(5) | (bits per sample)-1; "FLAC supports from 4 to 32 bits per sample" |
  | u(36) | total interchannel samples, 0 = unknown |
  | u(128) | MD5 of unencoded audio, 0 = unknown |
  Total 272 bits = 34 bytes (arithmetic). Block sizes "MUST be in the 16-65535 range".
- Frame self-containment (CONFIRMED, section 6): "to allow a decoder to start decoding
  at any place in the stream even without having received a streaminfo metadata
  block, each frame header contains some basic information about the stream. This
  information includes sample rate, bits per sample, number of channels, etc." and "If
  a frame header refers to the streaminfo metadata block, the file is not
  "streamable"". Streamable subset (section 7): frame header "MUST NOT refer to the
  streaminfo metadata block to describe the sample rate" nor bit depth; block size
  max 16384. So: frames of a streamable-subset stream decode without STREAMINFO;
  STREAMINFO still gives buffer bounds (max block/frame size). Security note: trusting
  STREAMINFO without checks "could be vulnerable to buffer overflows".
- FLAC channel order (CONFIRMED, 9.1.3 Table 16): 6 channels = "front left, front
  right, front center, LFE, back/surround left, back/surround right" (WAVE order);
  8 channels = "FL, FR, FC, LFE, back left, back right, side left, side right". Other
  layouts via a WAVEFORMATEXTENSIBLE_CHANNEL_MASK Vorbis comment (section 8.6.2).

## 6. Opus outside Ogg

RFC 7845 section 5.1, https://www.rfc-editor.org/rfc/rfc7845.txt (read 2026-09-30):
- ID header (CONFIRMED, Figure 2), all multi-byte fields little endian:
  | offset | size | field |
  |---|---|---|
  | 0 | 8 | magic "OpusHead" (0x4F 0x70 0x75 0x73 0x48 0x65 0x61 0x64) |
  | 8 | 1 | version, "MUST always be '1'"; accept <= 15 |
  | 9 | 1 | output channel count C, "MUST NOT be zero" |
  | 10 | 2 | pre-skip (samples at 48 kHz), u16 LE |
  | 12 | 4 | input sample rate (Hz), u32 LE, "not the sample rate to use for playback" |
  | 16 | 2 | output gain, Q7.8 dB, s16 LE |
  | 18 | 1 | channel mapping family |
  | 19 | 2+C | optional channel mapping table (stream count N, coupled count M, C mapping octets); omitted for family 0 |
  So the header is 19 bytes for family 0 and 21+C bytes otherwise (arithmetic).
- Family 0 (CONFIRMED): "Allowed numbers of channels: 1 or 2. RTP mapping." "the
  channel mapping table MUST be omitted from the ID header packet."
- Family 1 (CONFIRMED): "Allowed numbers of channels: 1...8. Vorbis channel order".
  "6 channels: 5.1 surround (front left, front center, front right, rear left, rear
  right, LFE)." "8 channels: 7.1 surround (front left, front center, front right, side
  left, side right, rear left, rear right, LFE)." "The ordering is different from the
  one used by the WAVE [WAVE-MULTICHANNEL] and Free Lossless Audio Codec (FLAC) [FLAC]
  formats, so correct ordering requires permutation".
- Mapping semantics (CONFIRMED): index < 2*M selects stream index/2 stereo L or R;
  2*M <= index < 255 selects stream (index - M) mono; 255 = silence.
- Pre-skip guidance: "a pre-skip of at least 3,840 samples (80 ms) is RECOMMENDED"
  when cropping.

RFC 6716, https://www.rfc-editor.org/rfc/rfc6716.txt (read 2026-09-30):
- Frame sizes (CONFIRMED, 2.1.4): "Opus can encode frames of 2.5, 5, 10, 20, 40, or 60
  ms. It can also combine multiple frames into packets of up to 120 ms."
- Frame length limit (CONFIRMED, 3.2.1): "The maximum representable length is
  255*4+255=1275 bytes." "the length of any individual frame MUST NOT exceed 1275
  bytes [R2]". Packet duration "MUST NOT exceed 120 ms [R5]. This limits the maximum
  frame count for any frame size to 48 (for 2.5 ms frames)".
- Max packet size in bytes: RFC 6716 gives no single number (ASSUMED upper bound from
  the above rules: 48 frames of up to 1275 bytes plus code-3 header and lengths; the
  commonly cited practical encoder buffer figure is LEAD, see libopus API docs).
- 48 kHz (CONFIRMED, 2.1.3 area): "The MDCT layer always operates internally at a
  sample rate of 48 kHz. Since all the supported sample rates evenly divide this rate
  ..." and RFC 7845 5.1: "the reference decoder supports decoding any stream at a
  sample rate of 8, 12, 16, 24, or 48 kHz"; pre-skip and granule positions are in
  48 kHz samples.

## 7. Channel order

Microsoft WAVEFORMATEXTENSIBLE:
https://learn.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatextensible
(read 2026-09-30; page ms.date 2023-04-26). CONFIRMED: "The least significant bit
corresponds with the front left speaker, the next least significant bit corresponds to
the front right speaker, and so on." "The channels specified in dwChannelMask must be
present in the prescribed order (from least significant bit up)."
| flag | bit |
|---|---|
| SPEAKER_FRONT_LEFT | 0x1 |
| SPEAKER_FRONT_RIGHT | 0x2 |
| SPEAKER_FRONT_CENTER | 0x4 |
| SPEAKER_LOW_FREQUENCY | 0x8 |
| SPEAKER_BACK_LEFT | 0x10 |
| SPEAKER_BACK_RIGHT | 0x20 |
| SPEAKER_FRONT_LEFT_OF_CENTER | 0x40 |
| SPEAKER_FRONT_RIGHT_OF_CENTER | 0x80 |
| SPEAKER_BACK_CENTER | 0x100 |
| SPEAKER_SIDE_LEFT | 0x200 |
| SPEAKER_SIDE_RIGHT | 0x400 |
| SPEAKER_TOP_CENTER | 0x800 |
| SPEAKER_TOP_FRONT_LEFT | 0x1000 |
| SPEAKER_TOP_FRONT_CENTER | 0x2000 |
| SPEAKER_TOP_FRONT_RIGHT | 0x4000 |
| SPEAKER_TOP_BACK_LEFT | 0x8000 |
| SPEAKER_TOP_BACK_CENTER | 0x10000 |
| SPEAKER_TOP_BACK_RIGHT | 0x20000 |
5.1 (back) mask = 0x3F (FL FR FC LFE BL BR); 5.1 (side) = 0x60F (ASSUMED arithmetic).

ALSA channel map positions (alsa-lib Doxygen docs, docs only):
https://www.alsa-project.org/alsa-doc/alsa-lib/group___p_c_m.html (read 2026-09-30).
CONFIRMED declaration: `enum snd_pcm_chmap_position { SND_CHMAP_UNKNOWN = 0,
SND_CHMAP_NA, SND_CHMAP_MONO, SND_CHMAP_FL, ... SND_CHMAP_BRC, SND_CHMAP_LAST =
SND_CHMAP_BRC }`. Numeric values follow from C enum sequencing (ASSUMED but mechanical):
UNKNOWN 0, NA 1 ("N/A, silent"), MONO 2, FL 3, FR 4, RL 5 ("rear left"), RR 6, FC 7,
LFE 8, SL 9, SR 10, RC 11, FLC 12, FRC 13, RLC 14, RRC 15, FLW 16, FRW 17, FLH 18,
FCH 19, FRH 20, TC 21, TFL 22, TFR 23, TFC 24, TRL 25, TRR 26, TRC 27, TFLC 28,
TFRC 29, TSL 30, TSR 31, LLFE 32, RLFE 33, BC 34, BLC 35, BRC 36 (= LAST).
Also snd_pcm_chmap_type: NONE, FIXED, VAR, PAIRED, LAST.

CTA-861 channel allocation for 5.1: LEAD. The standard is paywalled; the Microchip
HDMI TX IP user guide
(https://ww1.microchip.com/downloads/aemDocuments/documents/FPGA/ProductDocuments/UserGuides/ip_cores/directcores/HDMI_TX_IP_UG.pdf,
read 2026-09-30) only confirms "Data Byte 4 : CA7..CA0" in the Audio InfoFrame, no
table. A search-engine summary (not a primary source) states CA 0x0B = FL, FR, LFE,
FC, RL, RR; not confirmed from a primary source. Leads with the full CA table:
Analog Devices ADV7511/ADV7513 Programming Guides
(https://www.analog.com/media/en/technical-documentation/user-guides/ADV7511_Programming_Guide.pdf;
download stalled here), and the ANSI preview of CEA-861-F
(https://webstore.ansi.org/preview-pages/CEA/preview_ANSI+CEA+861-F+Final+2013.pdf).
Do NOT use the Linux kernel ALSA/ASoC HDMI sources (GPL) that also carry this table.

Cross-format 5.1 order summary (CONFIRMED from the sources above):
- WAVE/FLAC: FL FR FC LFE BL BR
- Opus/Vorbis family 1: FL FC FR RL RR LFE
- ALSA: positions are per-channel labels (chmap), not a fixed order.

---

## What was read

All read 2026-09-30. No source file of a GPL project was opened (K33); the Linux
kernel's HDMI sources were deliberately not used for the CTA-861 question.

- https://noiseprotocol.org/noise.html
- https://raw.githubusercontent.com/haskell-cryptography/cacophony/master/vectors/cacophony.txt
- https://raw.githubusercontent.com/mcginty/snow/main/tests/vectors/snow.txt
- GitHub API (metadata only): repos/mcginty/snow, repos/haskell-cryptography/cacophony, repos/rweather/noise-c (license, file listings, commit dates)
- https://www.rfc-editor.org/rfc/rfc8446.txt
- https://raw.githubusercontent.com/Mbed-TLS/mbedtls/development/docs/architecture/tls13-support.md
- https://raw.githubusercontent.com/Mbed-TLS/mbedtls/development/README.md (licence section)
- GitHub API repos/Mbed-TLS/mbedtls/releases/latest
- https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/protocols/mbedtls.html
- https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/kconfig-reference.html
- https://crates.io/api/v1/crates/{x25519-dalek,curve25519-dalek,chacha20poly1305,sha2,hmac,getrandom,zeroize,rand_core,digest,cipher}
- https://www.rfc-editor.org/rfc/rfc9639.txt
- https://www.rfc-editor.org/rfc/rfc7845.txt
- https://www.rfc-editor.org/rfc/rfc6716.txt
- https://learn.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatextensible
- https://www.alsa-project.org/alsa-doc/alsa-lib/group___p_c_m.html
- https://ww1.microchip.com/downloads/aemDocuments/documents/FPGA/ProductDocuments/UserGuides/ip_cores/directcores/HDMI_TX_IP_UG.pdf
- https://patents.google.com/patent/US20130223632A1/en (no CA table found)
