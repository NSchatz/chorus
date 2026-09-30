#!/usr/bin/env bash
# Rule "Every rule has a check" (K18): the table at the top of docs/conventions.md names at least
# one check per rule; every script it names exists and is run by the gate (tools/gate.sh runs
# every tools/conventions/check-*.sh); every gate step it names exists in tools/gate.sh; every
# rule has its own section; and every check script is named in the table.
. "$(dirname "$0")/lib.sh"
doc=docs/conventions.md
rc=0
bad() { fail "Every rule has a check" "$1"; rc=1; }
command grep -q 'for c in tools/conventions/check-\*\.sh' tools/gate.sh || bad "tools/gate.sh no longer runs every tools/conventions/check-*.sh"

rows="$(awk '/^## The rules and their checks/ { on = 1; next } on && /^## / { exit } on && /^\| [0-9]+ \|/' "$doc")"
[ -n "$rows" ] || { bad "$doc has no rule table under '## The rules and their checks'"; exit 1; }
named=" "
n=0
while IFS= read -r row; do
    n=$((n + 1))
    IFS='|' read -r _ num rule check _ <<< "$row"
    num="$(printf '%s' "$num" | tr -d ' ')"
    rule="$(printf '%s' "$rule" | sed 's/^ *//; s/ *$//')"
    [ "$num" = "$n" ] || bad "table row $n is numbered $num"
    command grep -q -x -F "## $num. $rule" "$doc" || bad "rule $num '$rule' has no section '## $num. $rule'"
    scripts="$(printf '%s' "$check" | command grep -o 'tools/conventions/check-[a-z0-9-]*\.sh' || true)"
    # shellcheck disable=SC2016 # the backticks are literal Markdown, not an expansion
    steps="$(printf '%s' "$check" | command grep -o 'gate steps\{0,1\} [^;(]*' | command grep -o '`[a-z0-9-]*`' | tr -d '`' || true)"
    [ -n "$scripts$steps" ] || bad "rule $num '$rule' names no check"
    for s in $scripts; do
        [ -f "$s" ] || bad "rule $num names $s, which does not exist"
        named="$named$s "
    done
    for st in $steps; do
        command grep -E -q "^[[:space:]]*step[[:space:]]+${st}[[:space:]]" tools/gate.sh || bad "rule $num names gate step '$st', which tools/gate.sh does not run"
    done
done <<< "$rows"
for s in tools/conventions/check-*.sh; do
    [[ "$named" == *" $s "* ]] || bad "$s is not named by any rule in $doc"
done
[ "$rc" = 0 ] && echo "conventions: $n rules, each with a check; $(printf '%s\n' tools/conventions/check-*.sh | wc -l) check scripts, all named and all run by the gate"
exit "$rc"
