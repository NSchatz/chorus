#!/usr/bin/env bash
# Rule "Clean-room provenance" (BRIEF.md section 3.1 rule 1, K33, K39, audit B-12): every
# proposal, research note and decision record from ADR 0032 on has a `## What was read` section
# with at least one entry, so what informed chorus's code can be audited after the fact.
# docs/clean-room.md records the position for everything written before the rule.
. "$(dirname "$0")/lib.sh"
first_adr=32
rc=0
n=0
while IFS= read -r f; do
    case "$f" in
        docs/decisions/*)
            num="$(basename "$f")"
            num="${num%%-*}"
            [[ "$num" =~ ^[0-9]{4}$ ]] && [ "$((10#$num))" -ge "$first_adr" ] || continue ;;
        */README.md) continue ;;
    esac
    n=$((n + 1))
    # The section's body up to the next heading must hold at least one non-blank line.
    body="$(awk '/^## What was read/ { on = 1; next } on && /^#/ { exit } on && NF { print }' "$f")"
    if [ -z "$body" ]; then
        fail "Clean-room provenance" "$f has no '## What was read' section listing its sources"
        rc=1
    fi
done < <(git ls-files 'docs/proposals/*.md' 'docs/research/*.md' 'docs/decisions/*.md')
[ "$rc" = 0 ] && echo "provenance: $n proposals, research notes and records from ADR $first_adr on, each with '## What was read'"
[ -f docs/clean-room.md ] || { fail "Clean-room provenance" "docs/clean-room.md is missing"; rc=1; }
exit "$rc"
