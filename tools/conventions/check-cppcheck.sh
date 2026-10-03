#!/usr/bin/env bash
# Rule "C static analysis": the pinned cppcheck finds nothing in the firmware C.
# --check-level=exhaustive: at the normal level cppcheck reports that it limited its branch
# analysis, which --error-exitcode turns into a failure. `__asm__(x)=` hides the GCC extension
# ESP-IDF's embedded-file symbols use, which cppcheck cannot parse; --force keeps it analysing
# every configuration (the CHORUS_TARGET_ESP32S3 paths included) once that define is given.
# -j8 --cppcheck-build-dir: eight files analysed at once, the same analysis. Each file's checks
# are the same in a worker process; the whole-program (ctu*) checks run over every file's
# summary only when the summaries are in a build directory, and `-j` without one silently drops
# them (measured 2026-10-03: an injected ctunullpointer was found serially and with the build
# directory, and missed by -j8 alone). The directory is new on every run, so no result is ever
# replayed from a cache.
. "$(dirname "$0")/lib.sh"
need cppcheck || exit 1
build_dir="$(mktemp -d "${TMPDIR:-/tmp}/chorus-cppcheck.XXXXXX")"
trap 'rm -rf "$build_dir"' EXIT
cppcheck -j8 --cppcheck-build-dir="$build_dir" \
    --std=c11 --error-exitcode=1 --enable=warning,portability --check-level=exhaustive \
    --inline-suppr -q -I firmware/include --suppress=missingIncludeSystem '-D__asm__(x)=' --force \
    firmware/src firmware/main firmware/check firmware/tests ||
    { fail "C static analysis" "cppcheck found the problems above"; exit 1; }
echo "$(cppcheck --version): firmware/src, main, check and tests clean"
