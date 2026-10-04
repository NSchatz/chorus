# third_party

Vendored C that chorus compiles but does not own: the FLAC and Opus decoders for `coded_chunk`
(docs/protocol.md, "Codecs: PCM, FLAC and Opus"), chosen by proposal P9 (Option A, approved at
Checkpoint K) and recorded in `docs/decisions/0044-the-vendored-decoders.md`. The same files are
compiled by the endpoint's host build (`firmware/Makefile`), the endpoint image
(`firmware/main/CMakeLists.txt`), and the Linux client's sys crate (`crates/opus-sys`), so every
side runs the same decoder code.

Every upstream file here is byte for byte the upstream file at the pin below (checked with `cmp`
against the archive on 2026-09-30). chorus's own files beside them are named in the last column
and carry chorus's licence (MIT OR Apache-2.0).

| Tree | Upstream | Version and pin | Licence | chorus's own files |
|---|---|---|---|---|
| `dr_flac/` | dr_libs, `dr_flac.h` | 0.13.3 (2026-01-17), tag `flac-0.13.3` at commit `69d777c482775858e8ea8a7b047c9bcd451febc8`; `dr_flac.h` sha256 `a35468f278f17cf9538ce189debf76a5854a77e139980f7b5185d67566a0c837` | Public domain (Unlicense) or MIT No Attribution, the file's own footer | `dr_flac.c` (the one unit that compiles the implementation) |
| `opus/` | libopus, `opus-1.6.1.tar.gz` | 1.6.1 (2026-01-14), archive sha256 `6ffcb593207be92584df15b32466ed64bbec99109f007c82205f0194572411a1` | BSD-3-Clause (`opus/COPYING`), with the royalty-free patent licences it names (IETF IPR 1524, 1914, 1526) | `chorus-build.txt` (the definitions and unit list every build reads) |

Where each pin was read, 2026-09-30:

- libopus 1.6.1 is the current stable release: <https://opus-codec.org/downloads/> ("libopus
  1.6.1", Jan 14, 2026, with the archive's SHA256 above) and the GitHub tag list
  <https://api.github.com/repos/xiph/opus/tags> (v1.6.1 newest). Archive:
  <https://downloads.xiph.org/releases/opus/opus-1.6.1.tar.gz>; the digest also matches
  <https://downloads.xiph.org/releases/opus/SHA256SUMS.txt>.
- dr_flac 0.13.3 is the newest tagged release: <https://api.github.com/repos/mackron/dr_libs/git/matching-refs/tags/flac>
  (`flac-0.13.3`); file <https://raw.githubusercontent.com/mackron/dr_libs/69d777c482775858e8ea8a7b047c9bcd451febc8/dr_flac.h>.
  The master branch carries an unreleased 0.13.4 (seeking, metadata bounds, and error
  propagation from a subframe decode); chorus never seeks, feeds dr_flac only a STREAMINFO
  block it builds itself, and refuses a frame whose CRC-16 fails, so the release is taken and
  the upgrade is a later commit of its own when 0.13.4 is tagged.

## What is vendored, and what is not

- `dr_flac/dr_flac.h`: the whole single-file library (it is one file).
- `opus/`: the decoder only. `chorus-build.txt` lists its 64 translation units: libopus's own
  source lists (`opus_sources.mk`, `celt_sources.mk`, `silk_sources.mk` in the archive) minus
  every encoder unit, with what the linker then asked for added back. The headers are exactly
  the ones those units include (the compiler's `-MM` list), plus `include/`. No `dnn/`
  (23 MB of neural PLC and DRED, off by default), no tests, docs, build systems or
  architecture-specific code (x86, ARM, MIPS: none is for the ESP32-S3's Xtensa, and on the
  host their absence keeps the decode on the same portable C). `src/opus_compare.c` is vendored
  as libopus's conformance tool, which the fixture tests run; it is not part of the library.
  About 1.4 MB of source, against 10.5 MB for the archive.

## How it is built

- libopus: `OPUS_BUILD FIXED_POINT ENABLE_RES24 DISABLE_FLOAT_API VAR_ARRAYS`
  (`opus/chorus-build.txt` says what each does).
- dr_flac: `DR_FLAC_NO_STDIO DR_FLAC_NO_OGG DR_FLAC_NO_WCHAR DR_FLAC_NO_SIMD`: no file API, no
  Ogg container, no wide characters (none is used: frames arrive in memory), and no SIMD, so the
  host decodes on the same scalar C the ESP32-S3 runs.
- Both with the endpoint's floating-point flags (`-ffp-contract=off -fno-fast-math`) and
  without its warning set: the code is upstream's and stays byte for byte upstream's.

**Fixed point for Opus, on fitness for the ESP32-S3.** The S3 has a single-precision FPU, but
Espressif's own guidance for it is that "Even though ESP32-S3 has a single precision hardware
floating point unit, floating point calculations are always slower than integer calculations"
and "If possible then use fixed point representations" (ESP-IDF, "Maximizing Execution Speed",
<https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-guides/performance/speed.html>,
read 2026-09-30). libopus ships a complete fixed-point decoder. It also makes the decode integer
arithmetic end to end, so the host build's output is the output the S3 computes, bit for bit,
and the host fixture tests speak for the device's decode (the Rust client compiles the same
units the same way, and the fixtures hold both to one exact decode). With `ENABLE_RES24` the
fixed-point decoder keeps 24 bits of resolution, so a `pcm_s24le` stream loses nothing to it.
Conformance of this exact configuration: every one of the 12 RFC 8251 test vectors, decoded
stereo and mono (24 runs), has every packet's final range equal to the vector's and passes
opus_compare (docs/measurements/codec-decode-cost-host.md).

## The repository's checks and this tree

- **Scanned:** the endpoint safety scan (`firmware/check/endpoint_scan.c`) walks every `.c`
  and `.h` under each tree named in `firmware/endpoint-units.conf`'s `[vendored]` section with
  the same rules as the endpoint's own units (no settable clock, no eFuse or Secure Boot or
  Flash Encryption call, no OTA activation, no external-RAM placement), and fails on a
  directory under `third_party/` that section does not name. `firmware/tests/test_scan.c`
  shows it going red on an eFuse write and on a clock read smuggled into vendored files.
- **Held to the repository-wide rules:** no em dash, identity and secret scan (both over every
  tracked file, this tree included).
- **Not held to chorus's C style:** `clang-format` and `cppcheck` cover `firmware/` only
  (docs/conventions.md, rules 5 and 6), and the warning flags are not applied here. Upstream's
  code keeps upstream's style; chorus's code that calls it (`firmware/src/codec*.c`) is held to
  every rule.

## Updating a pin

Its own commit: fetch the new release, check its digest against the upstream's published one,
replace the files, re-run `make firmware-check` (the fixtures must stay green) and the full
conformance run of `tools/codec-fixtures/opus_ref.c`, and update this table with the URL and
the date read.

## Wake-word models

`wakeword/` is not C and is not compiled: it holds the microWakeWord model the wake-word detector
(`crates/wakeword`) runs, its manifest, and the upstream `LICENSE` and `NOTICE`, each byte for
byte the upstream file. Its own list, `wakeword/LICENCES.md`, names every file with its source,
commit, SHA-256, licence and the date read, and `tools/conventions/check-wakeword.sh` holds the
directory to it (`docs/decisions/0000-the-wake-word-runtime.md`).
