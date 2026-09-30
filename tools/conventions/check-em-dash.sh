#!/usr/bin/env bash
# Rule "No em dashes" (BRIEF.md section 3.1, guardrail 5): no U+2014 in any tracked file.
# Commit messages are held to the same rule by check-commits.sh. The character is written as
# an escape so this file does not trip itself.
. "$(dirname "$0")/lib.sh"
dash="$(printf '\342\200\224')"
hits="$(git grep -nI -F "$dash" -- . || true)"
if [ -n "$hits" ]; then
    printf '%s\n' "$hits"
    fail "No em dashes" "the tracked tree holds an em dash (U+2014); use a colon, comma or parentheses"
    exit 1
fi
echo "no em dash in $(git ls-files | wc -l) tracked files"
