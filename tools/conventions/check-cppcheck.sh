#!/usr/bin/env bash
# Rule "C static analysis": the pinned cppcheck finds nothing in the firmware C.
# --check-level=exhaustive: at the normal level cppcheck reports that it limited its branch
# analysis, which --error-exitcode turns into a failure. `__asm__(x)=` hides the GCC extension
# ESP-IDF's embedded-file symbols use, which cppcheck cannot parse; --force keeps it analysing
# every configuration (the CHORUS_TARGET_ESP32S3 paths included) once that define is given.
. "$(dirname "$0")/lib.sh"
need cppcheck || exit 1
cppcheck --std=c11 --error-exitcode=1 --enable=warning,portability --check-level=exhaustive \
    --inline-suppr -q -I firmware/include --suppress=missingIncludeSystem '-D__asm__(x)=' --force \
    firmware/src firmware/main firmware/check firmware/tests ||
    { fail "C static analysis" "cppcheck found the problems above"; exit 1; }
echo "$(cppcheck --version): firmware/src, main, check and tests clean"
