#!/usr/bin/env bash
# Rule "The parity checklist" (goal 27): docs/parity.md lists every item of BRIEF.md section 8
# (8-1 to 8-10, the roadmap; 8.1-1 to 8.1-19, the program's phases) and of the program brief's
# K30, K31 (four features each) and K57 to K94, each once, with one known state, at least one
# evidence path in backticks that git tracks (a file, or a directory holding tracked files), and
# a reason for any state but `done`.
# Prints the count per state.
. "$(dirname "$0")/lib.sh"
doc=docs/parity.md
rc=0
bad() { fail "The parity checklist" "$1"; rc=1; }
[ -f "$doc" ] || { bad "$doc does not exist"; exit 1; }

want=()
for i in $(seq 1 10); do want+=("8-$i"); done
for i in $(seq 1 19); do want+=("8.1-$i"); done
for i in 1 2 3 4; do want+=("K30-$i" "K31-$i"); done
for i in $(seq 57 94); do want+=("K$i"); done

declare -A seen count
while IFS= read -r row; do
    IFS='|' read -r _ item _ state evidence reason _ <<< "$row"
    item="$(printf '%s' "$item" | tr -d ' ')"
    state="$(printf '%s' "$state" | sed 's/^ *//; s/ *$//')"
    reason="$(printf '%s' "$reason" | sed 's/^ *//; s/ *$//')"
    [ -n "${seen[$item]:-}" ] && bad "item $item is listed twice"
    seen[$item]=1
    case "$state" in
        done | partial | deferred | dropped | "not started") count[$state]=$((${count[$state]:-0} + 1)) ;;
        *)
            bad "item $item: state '$state' is not one of done, partial, deferred, dropped, not started"
            continue
            ;;
    esac
    # shellcheck disable=SC2016 # the backticks are literal Markdown, not an expansion
    paths="$(printf '%s' "$evidence" | command grep -o '`[^`]*`' | tr -d '`' || true)"
    [ -n "$paths" ] || bad "item $item names no evidence path"
    # A path is a tracked file, or a directory holding tracked files.
    while IFS= read -r p; do
        [ -n "$p" ] || continue
        [ -n "$(git ls-files -- ":(literal)$p" 2> /dev/null | head -n 1)" ] || bad "item $item: evidence path $p is not tracked in this repository"
    done <<< "$paths"
    if [ "$state" != "done" ] && { [ -z "$reason" ] || [ "$reason" = - ]; }; then
        bad "item $item is $state with no reason or follow-up"
    fi
done < <(command grep -E '^\| (8|8\.1|K[0-9]+)-?[0-9]* \|' "$doc")

for w in "${want[@]}"; do
    [ -n "${seen[$w]:-}" ] || bad "item $w is missing"
    unset 'seen[$w]'
done
for extra in "${!seen[@]}"; do
    bad "item $extra is not in the list (BRIEF.md section 8, K30, K31, K57-K94)"
done

summary=""
for s in "done" partial deferred dropped "not started"; do
    summary="$summary, $s ${count[$s]:-0}"
done
[ "$rc" = 0 ] && echo "parity: ${#want[@]} items, each with a state and evidence${summary}"
exit "$rc"
