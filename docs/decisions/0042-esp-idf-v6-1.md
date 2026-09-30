# 0042: the endpoint moves to ESP-IDF v6.1

- Status: accepted (goal 6, 2026-09-30)
- Decided by: the owner at Checkpoint K (P1 approved, Option B: "ESP32-S3 everywhere on ESP-IDF
  v6.1.x ... the upgrade lands in goal 6 with the safety scans re-run on the new version");
  this record says how the goal carried it out
- Implemented in: `firmware/config/endpoint.conf` (the pin), `firmware/main/CMakeLists.txt`,
  `firmware/sdkconfig.defaults`, `firmware/check/efuse-kconfig.list`,
  `firmware/check/endpoint_scan.c`, `tools/firmware-image-guard.sh`, `Makefile`
  (`CHORUS_IDF_ENV`)

## Context

Goal 2 pinned ESP-IDF v5.3.6, which reaches end of life on 2027-01-25. P1
(`docs/proposals/P1-embedded-platform.md`) recommended v6.1.x: longer support, and Mbed TLS 4's
PSA Crypto API, which the endpoint's v2 session (ADR 0039) is written against once instead of
on the legacy `mbedtls_*` API that v6 removed. P1 said to take "the newest v6.1.x patch
available when goal 6 starts", and to fall back to v5.5.5 if a v6.1 regression hits I2S or
`esp_eth`.

## What was read

- GitHub releases API, <https://api.github.com/repos/espressif/esp-idf/releases>, read
  2026-09-30: v6.1 published 2026-08-27; v6.0.3 on 2026-09-02; no v6.1.1 tag.
- <https://api.github.com/repos/espressif/esp-idf/git/ref/tags/v6.1> and
  `.../git/tags/4dc1c65503e98e9a6b4c3c5646237280ba79829b`, read 2026-09-30: the tag object peels
  to commit `fff9895c82d744c7237be8847347bdd1b07c6643`; the installed tree's `git rev-parse
  HEAD` prints the same commit.
- The release archive `esp-idf-v6.1.zip` from the v6.1 release page, sha256
  `cdeea7db47b90064ef185b2a1f1b33d17bcb13469a9f8cc20e06c4c4cdb4cc16` as downloaded 2026-09-30.
- ESP-IDF v6.1 migration guide, peripherals,
  <https://docs.espressif.com/projects/esp-idf/en/v6.1/esp32s3/migration-guides/release-6.x/6.0/peripherals.html>,
  read 2026-09-30: the legacy `driver` component "has been deprecated and no longer contains
  public dependencies" on the `esp_driver_*` components; projects add those instead.
- ESP-IDF v6.1 migration guide, security,
  <https://docs.espressif.com/projects/esp-idf/en/v6.1/esp32s3/migration-guides/release-6.x/6.0/security.html>
  (read for P1 on 2026-09-30): Mbed TLS 4.0, PSA Crypto the primary interface.
- In the installed tree (Apache-2.0): `tools/tools.json` (cmake 4.0.3 and ninja 1.12.1 are
  `on_request`), `tools/idf_py_actions/tools.py:671-681` (`IDF_PY_BUILD_JOBS` becomes
  `ninja -j N`), `components/mbedtls/Kconfig` and `port/include/mbedtls/esp_config.h` (the
  Kconfig options behind each `PSA_WANT_` symbol), and every file the eFuse lists cite.

## Decision

1. **Pin v6.1 at commit `fff9895c`**, installed rootless at `/cache/esp/esp-idf-v6.1` with
   `install.sh esp32s3`, plus `idf_tools.py install cmake ninja` (both are on request in v6.1,
   where v5.3 installed them by default). `make gate` sources
   `/cache/esp/chorus-idf-v6.1-export.sh`; the v5.3.6 tree stays installed so the v0.1.0 image
   can still be rebuilt from its tag.
2. **The driver split:** `firmware/main` requires `esp_driver_gpio`, `esp_driver_i2c` and
   `esp_driver_i2s`, the three whose headers it includes, instead of `driver`. Nothing else in
   the binding changed: the image compiled clean on the first v6.1 build.
3. **Crypto options:** `sdkconfig.defaults` sets `MBEDTLS_CHACHA20_C` and
   `MBEDTLS_CHACHAPOLY_C` (default `n`) and states the three the session needs that default to
   `y` (Curve25519, ECDH, SHA-256), each mapped to its `PSA_WANT_` symbol in a comment.
4. **Guardrail 2 re-derived on v6.1:** every Kconfig citation in `efuse-kconfig.list` was re-read
   and re-numbered (anti-rollback moved to `Kconfig.app_rollback`, the two crypto startup burns
   to `esp_security`); every writer the image guard lists is still declared where it cites and
   the three ROM entry points keep their addresses; v6.1 adds two writers,
   `esp_efuse_set_recovery_bootloader_offset` and `esp_flash_encryption_use_efuse_key`, which
   both the image guard and the source scan now name.
5. **The job cap re-verified:** `IDF_PY_BUILD_JOBS` still reaches ninja as `-j`, read in the
   source and seen on the running build during the gate (the PR body carries the sample).

## Consequences

- The firmware compiles on a toolchain supported to about 2029 (P1's arithmetic from Espressif's
  support policy), and the endpoint's crypto is written once on PSA Crypto.
- The first gate on v6.1 is a cold ccache (a new compiler, GCC 15.2); later runs hit it.
- Fallback, unchanged from P1: v5.5.5 (commit `b774170ff46c393eeb5e495ea37936038d3f4f4f`) if a
  v6.1 regression hits I2S or `esp_eth` on the bench; nothing measured here says so.
- Every hardware claim about the S3 on v6.1 stays `ASSUMED` until a bench session: nothing here
  was flashed.
