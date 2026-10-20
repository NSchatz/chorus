#!/usr/bin/env bash
# Rule "Decision records" (K48): an ADR's number is the number of the pull request that adds it,
# so parallel branches cannot claim the same "next" number. A record is drafted as
# docs/decisions/0000-<slug>.md and renamed to the PR's number, zero-padded to four digits,
# once the PR exists; it never merges as 0000. Every record starts `# <number>: <title>` and
# carries a `- Status:` line.
# The records numbered before this rule are a closed set: exactly 0001-0027, each once (the
# duplicates of 0012 and 0021 became 0022-0024 and three pre-rule decisions were recorded as
# 0025-0027 in goal 4, K48, audit B-19 and B-8). A new file numbered below 0032 fails, and so does
# a record the index docs/decisions/README.md does not name.
# A shallow clone (CI's checkout without history) cannot tell which PR added a record: the PR
# number part prints SKIPPED there, and everything else is still checked.
# On a pull request, CI exports its number as CHORUS_GATE_PR and its base as CHORUS_GATE_BASE:
# every record the PR adds (a rename's new name counts as added, as on main) must carry that
# number, so a misnumbered record fails before merge instead of in the nightly run on main.
. "$(dirname "$0")/lib.sh"
# Before this rule, records were numbered in sequence; from here on a new number is a PR number.
first_pr_numbered=32
last_pre_rule=27
rc=0
bad() { fail "Decision records" "$1"; rc=1; }
shallow=0
[ "$(git rev-parse --is-shallow-repository)" = true ] && shallow=1

# own_pr <number> <pr>: the PR whose number a record carries when PR <pr> added it. A record
# whose own PR landed inside another PR's squash keeps its own PR's number; the list names each
# one with the squash that carried it (goal 17, D8: PR #129's branch was merged into PR #132's
# and landed with it; #129 was closed with a pointer; goal 18: PR #138's tree landed as PR
# #139's one commit, a subject of #138's own being too long; PR #253 merged its record as 0251
# and PR #254 renamed it, the rename being its add).
own_pr() {
    case "$1:$2" in
    0129:132) echo 129 ;;
    0138:139) echo 138 ;;
    0253:254) echo 253 ;;
    *) echo "$2" ;;
    esac
}

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
    [ -n "${seen[$num]:-}" ] && bad "$f: number $num is already used by ${seen[$num]}"
    seen[$num]="$f"
    n10=$((10#$num))
    if [ "$n10" -lt "$first_pr_numbered" ] && [ "$n10" -gt "$last_pre_rule" ]; then
        bad "$f: numbers $((last_pre_rule + 1))-$((first_pr_numbered - 1)) are never used; a new record takes its PR's number"
    fi
    command grep -q -F "($b)" docs/decisions/README.md || bad "$f is not listed in docs/decisions/README.md's index"
    # On main, a record from the rule on must carry the number of the squash-merged PR that added it.
    if [ "$n10" -ge "$first_pr_numbered" ] && [ "$shallow" = 0 ]; then
        pr="$(git log --diff-filter=A --format=%s -- "$f" | sed -n 's/.*(#\([0-9]*\))$/\1/p' | tail -n 1)"
        pr="$(own_pr "$num" "$pr")"
        [ -z "$pr" ] || [ "$((10#$num))" = "$pr" ] || bad "$f: numbered $num but added by PR #$pr"
    fi
done < <(git ls-files 'docs/decisions/[0-9]*.md')
# On a pull request, the records it adds carry its own number; renames count as adds, as main's
# check above reads them (git log on the new path alone).
if [ -n "${CHORUS_GATE_PR:-}" ] && [ -n "${CHORUS_GATE_BASE:-}" ]; then
    git cat-file -e "$CHORUS_GATE_BASE^{commit}" 2> /dev/null ||
        bad "CHORUS_GATE_BASE $CHORUS_GATE_BASE is not a commit; the records this pull request adds are unchecked"
    printf -v want '%04d' "$((10#$CHORUS_GATE_PR))"
    while IFS= read -r f; do
        b="$(basename "$f")"
        num="${b%%-*}"
        [ "$(own_pr "$num" "$CHORUS_GATE_PR")" = "$((10#$CHORUS_GATE_PR))" ] || continue
        [ "$num" = "$want" ] || bad "$f: added by this pull request, #$CHORUS_GATE_PR; rename it to $want-${b#*-}"
    done < <(git diff --no-renames --diff-filter=A --name-only "$CHORUS_GATE_BASE"...HEAD -- 'docs/decisions/[0-9]*.md')
fi
# The pre-rule set is closed: every number from 0001 to 0027 is present (once, checked above).
for ((i = 1; i <= last_pre_rule; i++)); do
    printf -v want '%04d' "$i"
    [ -n "${seen[$want]:-}" ] || bad "record $want is missing; the pre-rule records are exactly 0001-00$last_pre_rule"
done
[ "$shallow" = 1 ] && echo "SKIPPED: shallow clone; the PR number of records from 00$first_pr_numbered on is checked locally"
[ "$rc" = 0 ] && echo "decision records: $n, numbers unique, 0001-00$last_pre_rule closed, each in the index"
exit "$rc"
