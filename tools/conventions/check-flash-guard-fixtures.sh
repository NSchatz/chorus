#!/usr/bin/env bash
# The fixture test of check-flash-guard.sh (rule "The flash guard"): each forbidden form under
# fixtures/flash-guard/forbidden/NN must fail the scan on its own; the approved read forms under
# fixtures/flash-guard/allowed/ must pass, and must fail again once any one forbidden form is
# added beside them. Fixture files carry a .fixture suffix so no other tool lints them; it is
# removed when they are copied into the scanned tree.
. "$(dirname "$0")/lib.sh"
here=tools/conventions
fx="$here/fixtures/flash-guard"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

# materialize SRC DEST: copy SRC's files into DEST without their .fixture suffix.
materialize() {
    (cd "$1" && command find . -type f -name '*.fixture' -printf '%P\n') | while IFS= read -r p; do
        mkdir -p "$2/$(dirname "$p")"
        cp "$1/$p" "$2/${p%.fixture}"
    done
}

rc=0
n=0
for d in "$fx"/forbidden/*/; do
    form="$(basename "$d")"
    t="$scratch/f$form"
    materialize "$d" "$t"
    n=$((n + 1))
    if out="$(bash "$here/check-flash-guard.sh" "$t" 2>&1)"; then
        echo "form $form: PASSED the scan, expected a failure: $(cd "$t" && command find . -type f -printf '%P ')"
        rc=1
    else
        echo "form $form: fails as expected: $(printf '%s\n' "$out" | head -n 1 | cut -c1-110)"
    fi
    t2="$scratch/a$form"
    materialize "$fx/allowed" "$t2"
    materialize "$d" "$t2"
    if bash "$here/check-flash-guard.sh" "$t2" > /dev/null 2>&1; then
        echo "form $form beside the read forms: PASSED the scan, expected a failure"
        rc=1
    fi
done
materialize "$fx/allowed" "$scratch/allowed"
if out="$(bash "$here/check-flash-guard.sh" "$scratch/allowed" 2>&1)"; then
    echo "read forms: pass as expected: $out"
else
    printf '%s\n' "$out"
    echo "read forms: FAILED the scan, expected a pass"
    rc=1
fi
# The files the scan skips: docs/, the plan, these fixtures and exactly two scripts that build
# the name from pieces to do their job. Anything added to that list fails here.
want='^(docs/|\.claude/goals/|tools/conventions/fixtures/flash-guard/|tools/conventions/check-flash-guard\.sh$|tools/conventions/check-flash-tools-refuse\.sh$)'
got="$(sed -n "s/^excluded='\(.*\)'$/\1/p" "$here/check-flash-guard.sh")"
if [ "$got" = "$want" ]; then
    echo "exclusions: docs/, .claude/goals/, the fixtures, check-flash-guard.sh, check-flash-tools-refuse.sh"
else
    echo "exclusions: check-flash-guard.sh skips '$got', expected exactly '$want'"
    rc=1
fi
if [ "$rc" -ne 0 ]; then
    fail "The flash guard" "the flash-guard check does not hold to its fixtures"
    exit 1
fi
echo "flash-guard fixtures: $n forbidden forms fail, the read forms pass"
