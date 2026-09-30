#!/usr/bin/env bash
# Rule "The flash guard" (brief section 0.7, K4, K93): every tool that can write to a device
# refuses unless the owner, at the bench, set the owner-at-bench variable to 1, and nothing in
# this repository sets it. Every occurrence of the variable's name outside docs/ must be one of
# the approved read forms:
#     "${NAME:-}"   env::var("NAME")   getenv("NAME")
# and, in Markdown only, the bare name in backticks (prose that names it, e.g. CLAUDE.md).
# A line whose approved read also supplies a default (unwrap_or, or_else, `?:`) fails too.
# Owner-facing command lines that set it live only under docs/. The program's plan
# (.claude/goals/) and this check's own fixtures are not scanned: they quote the forbidden forms.
#
#   check-flash-guard.sh           scan the tracked files of this repository
#   check-flash-guard.sh DIR       scan every file under DIR, paths taken relative to DIR
#                                  (the fixture test, check-flash-guard-fixtures.sh)
set -u
if [ $# -ge 1 ]; then
    root="$1"
    cd "$root" || exit 2
    files() { command find . -type f -printf '%P\0'; }
else
    . "$(dirname "$0")/lib.sh"
    files() { git ls-files -z; }
fi

# Built from pieces so this file holds no occurrence of the name itself.
name="CHORUS_OWNER_AT_""BENCH"
excluded='^(docs/|\.claude/goals/|tools/conventions/fixtures/flash-guard/)'

violations=0
checked=0
while IFS= read -r -d '' f; do
    [[ "$f" =~ $excluded ]] && continue
    hits="$(command grep -n -I -F -- "$name" "$f" 2> /dev/null)" || continue
    while IFS= read -r hit; do
        checked=$((checked + 1))
        line="${hit#*:}"
        rest="$line"
        rest="${rest//\"\$\{$name:-\}\"/}"
        rest="${rest//env::var(\"$name\")/}"
        rest="${rest//getenv(\"$name\")/}"
        [[ "$f" == *.md ]] && rest="${rest//\`$name\`/}"
        why=""
        if [[ "$rest" == *"$name"* ]]; then
            why="not an approved read form"
        elif [[ "$line" =~ (unwrap_or|or_else|\?:) ]]; then
            why="an approved read that supplies a default"
        fi
        if [ -n "$why" ]; then
            echo "$f:${hit%%:*}: $why: $line"
            violations=$((violations + 1))
        fi
    done <<< "$hits"
done < <(files)

if [ "$violations" -gt 0 ]; then
    echo "FAIL: $violations occurrence(s) of the owner-at-bench variable outside docs/ are not approved read forms"
    echo "rule: docs/conventions.md, \"The flash guard\""
    exit 1
fi
echo "flash guard: $checked occurrence(s) outside docs/, all approved read forms"
