#!/usr/bin/env bash
# Rule "Measurement provenance" (brief section 4.5, BRIEF.md section 3.1 guardrail 3): every
# report under docs/measurements/ names its source on a line `Source: hardware|host|simulation|
# synthetic` and the build it measured on a line `Build measured: <40-hex sha>` that is a commit
# in this history: an ancestor of HEAD, so a branch commit a squash merge left out of `main` is
# caught by the next gate (goal 6: a report once named such a commit and the check let it pass,
# because the object still existed locally). Only a `hardware` report is timing evidence. Every report complies; the
# reports written before the rule were relabelled in goal 4 (K48, audit A-6).
# A shallow clone (CI's checkout without history) cannot tell whether a commit exists: the build
# commit part prints SKIPPED there, and the source line is still checked.
. "$(dirname "$0")/lib.sh"
rc=0
shallow=0
[ "$(git rev-parse --is-shallow-repository)" = true ] && shallow=1
bad() { fail "Measurement provenance" "$1"; rc=1; }
complies() {
    command grep -E -q '^Source: (hardware|host|simulation|synthetic)$' "$1" || return 1
    # shellcheck disable=SC2016 # the backticks are literal Markdown in the report
    sha="$(sed -n 's/^Build measured: `\{0,1\}\([0-9a-f]\{40\}\)`\{0,1\}$/\1/p' "$1" | head -n 1)"
    [ -n "$sha" ] || return 1
    [ "$shallow" = 1 ] || git merge-base --is-ancestor "$sha" HEAD 2> /dev/null
}
n=0
while IFS= read -r f; do
    n=$((n + 1))
    complies "$f" || bad "$f: needs 'Source: hardware|host|simulation|synthetic' and 'Build measured: <sha>' naming a commit in this history"
done < <(git ls-files 'docs/measurements/*.md' | command grep -v '/README\.md$')
[ "$shallow" = 1 ] && echo "SKIPPED: shallow clone; whether each build commit exists is checked locally"
[ "$rc" = 0 ] && echo "measurement reports: $n, each with its source and a build commit in this history"
exit "$rc"
