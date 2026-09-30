#!/usr/bin/env bash
# Rule "C flags": the host build of the firmware cores keeps its warning and floating-point
# flags. -ffp-contract=off and -fno-fast-math keep the C cores bit-exact with the Rust cores on
# the shared fixtures; the rest make every warning a failure.
. "$(dirname "$0")/lib.sh"
flags=(-std=c11 -Wall -Wextra -Werror -Wshadow -Wpointer-arith -Wstrict-prototypes -ffp-contract=off -fno-fast-math)
rc=0
line="$(command grep -E '^CFLAGS[[:space:]]*[:+?]?=' firmware/Makefile | tr '\n' ' ')"
for f in "${flags[@]}"; do
    [[ " $line " == *" $f "* ]] || { fail "C flags" "firmware/Makefile's CFLAGS lost $f"; rc=1; }
done
[ "$rc" = 0 ] && echo "firmware/Makefile CFLAGS hold all ${#flags[@]} required flags"
exit "$rc"
