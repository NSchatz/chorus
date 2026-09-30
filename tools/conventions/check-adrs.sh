#!/usr/bin/env bash
# Rule "Decision records" (K48): an ADR's number is the number of the pull request that adds it,
# so parallel branches cannot claim the same "next" number. A record is drafted as
# docs/decisions/0000-<slug>.md and renamed to the PR's number, zero-padded to four digits,
# once the PR exists; it never merges as 0000. Every record starts `# <number>: <title>` and
# carries a `- Status:` line.
# The five records numbered before this rule (0012 twice, 0021 three times) are listed below;
# goal 4 renumbers them (K48, audit B-19) and empties the list. A listed file that is gone fails,
# so the list cannot go stale.
. "$(dirname "$0")/lib.sh"
grandfathered=(
    docs/decisions/0012-the-cpu-time-bound-goes-on-first.md
    docs/decisions/0012-the-thread-inventory-is-complete-or-it-is-an-error.md
    docs/decisions/0021-a-measured-ratio-is-recorded-beside-the-value.md
    docs/decisions/0021-the-comment-density-baseline.md
    docs/decisions/0021-the-wireless-tier.md
)
# Before this rule, records were numbered in sequence; from here on a new number is a PR number.
first_pr_numbered=32
rc=0
bad() { fail "Decision records" "$1"; rc=1; }
for g in "${grandfathered[@]}"; do [ -f "$g" ] || bad "$g is listed as grandfathered but is gone; remove it from the list"; done

declare -A seen
n=0
while IFS= read -r f; do
    n=$((n + 1))
    b="$(basename "$f")"
    num="${b%%-*}"
    [[ "$num" =~ ^[0-9]{4}$ ]] || { bad "$f: the name does not start with a four-digit number"; continue; }
    [ "$num" = 0000 ] && [ "${CHORUS_GATE_BRANCH_DRAFT:-}" != 1 ] && bad "$f: a draft number; rename it to the PR's number before merging"
    head -n 1 "$f" | command grep -q "^# $num: " || bad "$f: the first line is not '# $num: <title>'"
    command grep -q '^- Status: ' "$f" || bad "$f: no '- Status:' line"
    grand=0
    for g in "${grandfathered[@]}"; do [ "$g" = "$f" ] && grand=1; done
    if [ -n "${seen[$num]:-}" ] && [ "$grand" = 0 ]; then
        bad "$f: number $num is already used by ${seen[$num]}"
    fi
    seen[$num]="$f"
    # On main, a record from the rule on must carry the number of the squash-merged PR that added it.
    if [ "$((10#$num))" -ge "$first_pr_numbered" ]; then
        pr="$(git log --diff-filter=A --format=%s -- "$f" | sed -n 's/.*(#\([0-9]*\))$/\1/p' | tail -n 1)"
        [ -z "$pr" ] || [ "$((10#$num))" = "$pr" ] || bad "$f: numbered $num but added by PR #$pr"
    fi
done < <(git ls-files 'docs/decisions/[0-9]*.md')
[ "$rc" = 0 ] && echo "decision records: $n, numbers unique apart from ${#grandfathered[@]} grandfathered until goal 4"
exit "$rc"
