# 0044: FLAC and Opus decode through vendored dr_flac and libopus on the endpoint, and Symphonia and the same libopus in the Linux client

- Status: accepted (goal 6, 2026-09-30)
- Decided by: the owner at Checkpoint K (proposal P9, Option A, decision K79; K62 put FLAC and
  Opus on the wire; K26 and K95 let a licence off the allowlist in by record); the goal took the
  choices left open below
- Implemented in: `third_party/` (pins in `third_party/README.md`),
  `firmware/include/chorus/codec.h`, `firmware/src/codec*.c`, `crates/opus-sys`,
  `crates/client-linux/src/decode.rs` and `coded.rs`; held by `fixtures/codec`,
  `firmware/tests/test_codec.c` and `crates/client-linux/tests/codec_fixtures.rs`

## Context

Protocol v2 carries FLAC and Opus beside PCM (docs/protocol.md, "Codecs: PCM, FLAC and Opus";
ADR 0040): a `stream_format` carries the codec setup and each `coded_chunk` one FLAC frame or one
Opus packet. Goal 6 decodes them on the C endpoint and in the Linux client. P9 settled the
libraries: dr_flac and libopus on the endpoint; Symphonia for FLAC and libopus for Opus in Rust.
This record takes the calls P9 left to the goal, and is the record K95 asks for before MPL-2.0
code enters the build.

## What was read

All read 2026-09-30.

- `docs/proposals/P9-decoders.md` (whole), ADR 0040, ADR 0039 (for the shape of a licence
  exception), `docs/protocol.md` (codecs, `stream_format`, `coded_chunk`, the channel map).
- libopus: <https://opus-codec.org/downloads/> (1.6.1, Jan 14, 2026, with its SHA256), the tag
  list <https://api.github.com/repos/xiph/opus/tags>, the archive
  <https://downloads.xiph.org/releases/opus/opus-1.6.1.tar.gz> and its `SHA256SUMS.txt`; from the
  archive (BSD-3-Clause, permissive source): `COPYING`, `configure.ac` (the fixed-point and
  RES24 options), `opus_sources.mk`, `celt_sources.mk`, `silk_sources.mk`, `include/opus.h`,
  `src/opus_decoder.c` (the 24-bit decode), `src/opus_compare.c`.
- The RFC 8251 test vectors, <https://opus-codec.org/testvectors/> and
  <https://opus-codec.org/static/testvectors/opus_testvectors-rfc8251.tar.gz>; RFC 6716
  sections 3.1, 3.2.5, 4.1.6 and 6; RFC 7845 sections 4.2 and 5.1; RFC 9639 sections 8.2 and 9.
- dr_flac: the tag list <https://api.github.com/repos/mackron/dr_libs/git/matching-refs/tags/flac>,
  the commit history of `dr_flac.h` (GitHub API) and the diff of commit `4dc42c65819a`,
  `dr_flac.h` at `flac-0.13.3` (public domain or MIT-0, permissive source): its API, the frame
  structures and its licence footer.
- Symphonia: crates.io API, <https://crates.io/api/v1/crates/symphonia>,
  `symphonia-bundle-flac`, `symphonia-core` (0.6.1, 2026-08-13, MPL-2.0). From the published
  crates, the manifests and the public API only (the `FlacDecoder` constructor and trait
  implementation, `AudioCodecParameters`, `PacketRef`, `AudioDecoderOptions`,
  `GenericAudioBufferRef::copy_to_vec_interleaved`), to call them.
- MPL-2.0, <https://www.mozilla.org/en-US/MPL/2.0/>, sections 1.10, 3.1 to 3.3.
- `cc` on crates.io, <https://crates.io/api/v1/crates/cc> (1.5.1, 2026-09-25, MIT OR Apache-2.0).
- Reference FLAC tools: conda-forge `libflac` 1.5.0 (BSD-3-Clause; flac and metaflac), used as
  programs, their source not opened.
- Espressif, "Maximizing Execution Speed" for the ESP32-S3,
  <https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-guides/performance/speed.html>.
- No GPL or LGPL source was opened (no FFmpeg, no libFLAC source beyond using its programs).

## Decisions

1. **Versions.** libopus 1.6.1 (the current stable release), dr_flac 0.13.3 (the newest tagged
   release; master's unreleased 0.13.4 fixes seeking, metadata bounds and error propagation from a
   subframe, none of which chorus's use reaches: it never seeks, builds the only metadata block
   dr_flac sees, and refuses a frame whose CRC-16 fails), Symphonia 0.6.1 (P9 said 0.6.1; still
   the newest). Pins by version and sha256 in `third_party/README.md` and `Cargo.lock`.
2. **Opus in the client is the endpoint's libopus, not the `opus` crate.** P9 named the `opus`
   crate, whose `opusic-sys` builds its own libopus copy with cmake. A small in-workspace sys crate
   (`crates/opus-sys`, built by `cc` 1.5.1, MIT OR Apache-2.0) compiles `third_party/opus` from the
   same unit list and definitions the endpoint uses (`third_party/opus/chorus-build.txt`). That
   meets P9's own criterion ("Same Opus code as the endpoint: Yes") exactly: the fixtures hold the
   C and Rust decodes to one hash, which two copies of libopus built two ways could not promise.
   It also needs no cmake and no second vendored libopus. The cost is 120 lines of FFI, the one
   new place `unsafe` is allowed (`tools/conventions/check-rust-lints.sh`).
3. **Fixed point for Opus, on both sides.** Espressif: "Even though ESP32-S3 has a single
   precision hardware floating point unit, floating point calculations are always slower than
   integer calculations" and "If possible then use fixed point representations". Fixed point is
   also integer arithmetic end to end, so the host decode is the device's, bit for bit.
   `ENABLE_RES24` (libopus's default) keeps 24-bit resolution for `pcm_s24le`.
   `DISABLE_FLOAT_API` keeps float out of the decoder entirely.
4. **Decoder only, 64 units.** Vendored: the decoder units and the headers they include, and
   `src/opus_compare.c` as the conformance tool. Not vendored: the encoder (the server's Opus
   encoder is goal 16's, and adds its units then), `dnn/` (23 MB), arch-specific code, tests,
   docs. About 1.4 MB of source.
5. **MPL-2.0 for Symphonia's FLAC decoder, those crates only.** `symphonia-bundle-flac`,
   `symphonia-core`, `symphonia-common` and `symphonia-metadata` (what the FLAC decoder pulls in,
   `default-features = false`), used unmodified from crates.io. MPL-2.0's copyleft is per file
   (section 3.1 binds the Covered Software's own files; section 3.3 allows a Larger Work under
   other terms), so chorus's own licence is unaffected; the obligation is to say where Symphonia's
   source is when chorus is distributed in executable form (section 3.2), which goal 4's release
   notes carry. `deny.toml` allows MPL-2.0 for exactly these four crates, citing this record.
6. **One decode path in the client.** The session's `SecureReader` gains a translator hook
   (`crates/protocol/src/v2/session.rs`): it sees each decoded message in order and may put v1
   frames in the byte stream. The client's `CodedStream` decodes each `coded_chunk` and yields
   `audio_chunk` frames re-cut to `frames_per_chunk`, so the v1 receive path, its framing rules
   and the playout loop run unchanged; a leftover short chunk goes just before `stream_end`. The
   client now lists FLAC and Opus in `capabilities`; the server still sends only PCM, since the
   negotiation picks from what the server can send (a test says so).
7. **Mapping family 0 only for Opus** (one or two channels, what every endpoint in the plan
   plays); a family 1 stream is refused by name until a surround Opus stream is wanted.
8. **Conformance, not bit-identity, against the official Opus decodes.** The official `.dec`
   files are not what a fixed-point build outputs bit for bit, and RFC 6716 section 6 does not ask
   that: it asks for bit-exact entropy decoding (the final range) and opus_compare's quality
   criterion. Both are checked on every fixture on both sides, and all 12 RFC 8251 vectors, stereo
   and mono, pass them in this configuration. FLAC is lossless and is held to `flac -d` bit for bit
   and to the STREAMINFO MD5.

## Options not chosen

- The `opus` crate (P9's wording): a second libopus, built by cmake, possibly configured
  differently from the endpoint's; the fixtures could only compare the two by quality.
- Float libopus on the endpoint: slower on the S3 by Espressif's own guidance, and host and
  device outputs would depend on each compiler's float code.
- ropus (P9 Option B): not approved; a port, not the endpoint's code.
- dr_flac master at `dfe8377`: unreleased; its fixes do not reach chorus's use (decision 1).
- Decoding in a separate thread or queue in the client: the receive path already reads frames in
  order; a decode in the reader keeps ordering against `stream_end` and `time_sync` for free.

## Consequences

- The ESP-IDF image compiles `firmware/src/codec.c`, `codec_flac.c`, `codec_opus.c`,
  `third_party/dr_flac/dr_flac.c` and the units of `third_party/opus/chorus-build.txt` with its
  definitions (`firmware/main/CMakeLists.txt` reads the same list), vendored units without
  warnings. The C session opens the decoder from a FLAC or Opus `stream_format`, decodes each
  `coded_chunk` into the accounting a PCM chunk gets, and lists FLAC and Opus in its
  `capabilities`; the jitter buffer and I2S playout that both feed are goal 8's.
- The linked decoders took the S3 image past ESP-IDF's default 1 MB single-app partition, so
  `firmware/sdkconfig.defaults` selects ESP-IDF's "Single factory app (large), no OTA" table
  (1.5 MB app in the default 2 MB flash). A cheap call, made here and reversible: the partition
  layout OTA needs is goal 14's, which revisits the table and the flash size together. Sizes
  before and after are in the measurement report.
- libopus with `VAR_ARRAYS` takes its scratch from the calling task's stack; the S3 task that
  decodes needs a stack sized for it, which goal 8 measures on the device (not claimed here).
- Host cost and size are in `docs/measurements/codec-decode-cost-host.md` (host only; nothing
  here is an ESP32-S3 figure).
- An upgrade of any pin is its own commit that keeps `fixtures/codec` green.
