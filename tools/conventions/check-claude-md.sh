#!/usr/bin/env bash
# Rule "CLAUDE.md length" (K52; goals DECISIONS.md 13): CLAUDE.md stays at or under 200 lines and
# 500 words, and CARD.md at or under 60 lines; detail lives in docs/working-agreement.md,
# docs/conventions.md, docs/decisions/ and docs/measurements/.
. "$(dirname "$0")/lib.sh"
rc=0
n="$(wc -l < CLAUDE.md)"
w="$(wc -w < CLAUDE.md)"
echo "CLAUDE.md: $n lines (limit 200), $w words (limit 500)"
[ "$n" -le 200 ] || { fail "CLAUDE.md length" "CLAUDE.md has $n lines; move detail into docs/"; rc=1; }
[ "$w" -le 500 ] || { fail "CLAUDE.md length" "CLAUDE.md has $w words; move detail into docs/"; rc=1; }
c="$(wc -l < CARD.md)"
echo "CARD.md: $c lines (limit 60)"
[ "$c" -le 60 ] || { fail "CLAUDE.md length" "CARD.md has $c lines; point to docs/ instead"; rc=1; }
exit "$rc"
