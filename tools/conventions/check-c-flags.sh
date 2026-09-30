#!/usr/bin/env bash
# Rule "C flags": the host build of the firmware cores keeps its warning and floating-point
# flags. -ffp-contract=off and -fno-fast-math keep the C cores bit-exact with the Rust cores on
# the shared fixtures; the rest make every warning a failure. No firmware build file (Makefile,
# CMake, sdkconfig) may add a flag that relaxes IEEE floating point: -ffast-math, -Ofast,
# -funsafe-math-optimizations, or the parts of fast math that break bit-exactness on their own
# (-ffinite-math-only, -fassociative-math, -freciprocal-math). A flag added after -fno-fast-math
# overrides it, so its presence alone is not enough.
. "$(dirname "$0")/lib.sh"
flags=(-std=c11 -Wall -Wextra -Werror -Wshadow -Wpointer-arith -Wstrict-prototypes -ffp-contract=off -fno-fast-math)
forbidden='-(ffast-math|Ofast|funsafe-math-optimizations|ffinite-math-only|fassociative-math|freciprocal-math)'
# A flag is a whole word: `-fno-fast-math` and `-ffast-math-foo` are not `-ffast-math`.
pattern="(^|[^[:alnum:]_-])${forbidden}([^[:alnum:]_-]|\$)"
rc=0

# Self-test, so the pattern cannot quietly stop matching: each line before `|` must match or not.
while IFS='|' read -r want text; do
    got=no
    printf '%s\n' "$text" | command grep -E -q -- "$pattern" && got=yes
    [ "$got" = "$want" ] || { fail "C flags" "self-test: expected '$want' for: $text"; rc=1; }
done << 'CASES'
yes|CFLAGS += -ffast-math
yes|target_compile_options(${COMPONENT_LIB} PRIVATE -Ofast)
yes|CFLAGS += -O2 -funsafe-math-optimizations
yes|idf_build_set_property(COMPILE_OPTIONS "-ffinite-math-only" APPEND)
yes|CFLAGS:=-fassociative-math
yes|	-freciprocal-math \
no|CFLAGS += -ffp-contract=off -fno-fast-math
no|# -fno-fast-math for the same reason.
no|CFLAGS += -O2
CASES

line="$(command grep -E '^CFLAGS[[:space:]]*[:+?]?=' firmware/Makefile | tr '\n' ' ')"
for f in "${flags[@]}"; do
    [[ " $line " == *" $f "* ]] || { fail "C flags" "firmware/Makefile's CFLAGS lost $f"; rc=1; }
done
n=0
while IFS= read -r f; do
    n=$((n + 1))
    hits="$(command grep -n -E -- "$pattern" "$f" || true)"
    [ -z "$hits" ] || { printf '%s\n' "$hits" | sed "s|^|$f:|"; fail "C flags" "$f adds a flag that relaxes IEEE floating point; the C cores must stay bit-exact with Rust"; rc=1; }
done < <(git ls-files firmware | command grep -E '(^|/)(Makefile|[^/]*\.mk|CMakeLists\.txt|[^/]*\.cmake|sdkconfig[^/]*)$')
[ "$rc" = 0 ] && echo "firmware/Makefile CFLAGS hold all ${#flags[@]} required flags; no fast-math flag in $n firmware build files"
exit "$rc"
