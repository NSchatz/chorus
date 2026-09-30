# 0000: chorus flashes through one guarded tool, and the flash guard is checked by a per-language whitelist and by running the tool

- Status: accepted (goal 9, 2026-09-30)
- Decided by: the goal (brief section 13 item 3 and section 0.7; goal 3's adversarial caveats on
  the lexical scan)
- Implemented in: `tools/firmware-flash.sh`, `make firmware-flash`,
  `tools/conventions/check-flash-guard.sh`, `tools/conventions/check-flash-guard-fixtures.sh`,
  `tools/conventions/check-flash-tools-refuse.sh`, `tools/conventions/fixtures/flash-guard/`
  (forbidden forms 24-36, an allowed Python read); `docs/bench.md`, `docs/bench-packet.md` (S5),
  `docs/conventions.md` (rule 20)

## Context

Brief section 0.7: every tool that can write to a device refuses unless `CHORUS_OWNER_AT_BENCH`
is `1`, and nothing in the repository sets it. Until this goal no chorus tool wrote to a device:
bench session S5 flashed with ESP-IDF's own `idf.py flash`, run by the owner. Goal 3's adversarial
check found the allowlist scan lexical in a way that passed a C ternary default, a Python
`or "1"`, Rust `map_or`/`or`/`is_err()` as the go-ahead, a two-line shell default and the name
built from pieces, and asked the goal that adds the flashing tool to close them and add a test
that runs the tool.

## What was read

All read 2026-09-30: ESP-IDF v6.1 (Apache-2.0, commit `fff9895c82d744c7237be8847347bdd1b07c6643`,
local copy `/cache/esp/esp-idf-v6.1`): `components/esptool_py/project_include.cmake` (`:372`, the
flash command is `python -m esptool --chip <target>`; `:199` and `:213`, `write-flash` with the
build's `flash_args`; `:472-478`, `--before`/`--after` from `CONFIG_ESPTOOLPY_BEFORE/AFTER`) and
`tools/idf_py_actions/serial_ext.py:35` (the default baud 460800); esptool's documentation,
<https://docs.espressif.com/projects/esptool/en/latest/esp32s3/esptool/basic-commands.html>
(`write-flash <offset> <file> ...`) and
<https://docs.espressif.com/projects/esptool/en/latest/esp32s3/esptool/advanced-options.html>
(`--before default-reset`, `--after hard-reset`); the environment's esptool is v5.4.0. A build
directory of the gate's image (`flash_args`, `flasher_args.json`, `sdkconfig`).

## Decision

1. **One flashing tool.** `tools/firmware-flash.sh` writes the image `make firmware-image` built.
   It reads the arguments first and refuses any that names an eFuse, Secure Boot, Flash
   Encryption, anti-rollback or burn operation; then the guard, `[ "${CHORUS_OWNER_AT_BENCH:-}"
   != 1 ]`, before it opens a file or runs a program. It flashes the board profile the image was
   built for (a differing `CHORUS_BOARD_PROFILE` is refused), refuses an image with encrypted
   flash arguments or an sdkconfig option `firmware/check/efuse-kconfig.list` refuses, runs
   `tools/firmware-image-guard.sh` over the build before a real flash, and runs the command ESP-IDF
   v6.1's own flash target runs, with the offsets and files from the build's `flash_args` listed
   explicitly. `--print` needs no guard and runs nothing.
2. **The refusal names "the owner-at-bench variable" and points at `docs/bench.md`**, rather than
   spelling the name: outside `docs/` the name may appear only in a read form, and a message that
   spelled it would be a second, unchecked occurrence. `docs/bench.md` and the bench packet give
   the owner's command line.
3. **The scan is a whitelist per kind of file** (shell, Rust, C, Python, Markdown; any other kind
   may not name it), each read form tied to a comparison with `1`, and the name built from pieces
   fails anywhere. Exactly two scripts are not scanned because they build the name to do their
   job (the scan itself, and the behavioural test, which sets the variable to values other than
   `1`); the fixture test fails if the exclusion list grows.
4. **The behavioural test** runs every flashing tool, found by what it reads and calls rather
   than by a list alone, against a complete fixture image and port with shims for idf.py,
   esptool, espefuse and python, with the variable unset and set to seven values that are not
   `1`; every run must refuse, name the variable and call no shim.

## Consequences

- The tools that can write to a device today: `tools/firmware-flash.sh` alone. The bench scripts
  that drive the endpoint console (`tools/decode-cost-run.sh`, `tools/endpoint-rig-run.sh`,
  `tools/wireless-characterization-run.sh` through `tools/lib.sh`'s `endpoint_console`) write only
  runtime values (ADR 0060: nothing survives a reboot) and no flash, so they carry no guard. The
  server pushes no image (OTA is goal 14, which cites this check and adds its push to the test's
  list).
- A lexical scan still cannot see a name decoded at run time; the behavioural test covers every
  tool that reads the guard or calls a flashing program, and a tool that does neither cannot
  flash through esptool.
