#!/usr/bin/env bash
# Rule "CLAUDE.md length" (K52): CLAUDE.md stays at or under 200 lines; detail lives in
# docs/conventions.md, docs/decisions/ and docs/measurements/.
. "$(dirname "$0")/lib.sh"
n="$(wc -l < CLAUDE.md)"
echo "CLAUDE.md: $n lines (limit 200)"
[ "$n" -le 200 ] || { fail "CLAUDE.md length" "CLAUDE.md has $n lines; move detail into docs/"; exit 1; }
