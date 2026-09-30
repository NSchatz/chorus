#!/usr/bin/env bash
# Rule "Measurement provenance" (brief section 4.5, BRIEF.md section 3.1 guardrail 3): every
# report under docs/measurements/ names its source on a line `Source: hardware|host|simulation|
# synthetic` and the build it measured on a line `Build measured: <40-hex sha>` that is a commit
# in this history. Only a `hardware` report is timing evidence.
# The five reports written before this rule are listed below; goal 4 relabels them (K48, audit
# A-6) and empties the list. A listed file that is gone, or that already complies, fails, so
# the list only shrinks.
. "$(dirname "$0")/lib.sh"
grandfathered=(
    docs/measurements/hostctl-thread-inventory-repeat.md
    docs/measurements/rig3-free-run-noiseless-fixture.md
    docs/measurements/rig3-jitter-wireless-ps-min-modem.md
    docs/measurements/rig3-jitter-wireless-ps-none.md
    docs/measurements/rig3-lag-fixture-reference-capture.md
)
rc=0
bad() { fail "Measurement provenance" "$1"; rc=1; }
complies() {
    command grep -E -q '^Source: (hardware|host|simulation|synthetic)$' "$1" || return 1
    sha="$(sed -n 's/^Build measured: `\{0,1\}\([0-9a-f]\{40\}\)`\{0,1\}$/\1/p' "$1" | head -n 1)"
    [ -n "$sha" ] && git cat-file -e "$sha^{commit}" 2> /dev/null
}
n=0
for g in "${grandfathered[@]}"; do
    [ -f "$g" ] || { bad "$g is listed as grandfathered but is gone; remove it from the list"; continue; }
    complies "$g" && bad "$g now complies; remove it from the grandfathered list"
done
while IFS= read -r f; do
    n=$((n + 1))
    grand=0
    for g in "${grandfathered[@]}"; do [ "$g" = "$f" ] && grand=1; done
    [ "$grand" = 1 ] && continue
    complies "$f" || bad "$f: needs 'Source: hardware|host|simulation|synthetic' and 'Build measured: <sha>' naming a commit in this history"
done < <(git ls-files 'docs/measurements/*.md' | command grep -v '/README\.md$')
[ "$rc" = 0 ] && echo "measurement reports: $n, $((n - ${#grandfathered[@]})) checked, ${#grandfathered[@]} grandfathered until goal 4"
exit "$rc"
