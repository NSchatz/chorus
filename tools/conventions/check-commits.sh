#!/usr/bin/env bash
# Rule "Commits" (I3): a subject reads `<area>: <summary>` in lower-case area words (a crate,
# `firmware`, `tools`, `docs`, `deploy`, `goals` for the ledger, and so on), at most 100
# characters (finding and decision IDs live there) before a squash merge's ` (#N)`; no message holds an em dash. Identity terms in
# messages are check-identity.sh's. Every commit since CONVENTIONS_BASE is held to it; the
# history before keeps the subjects it was written with (history is never rewritten).
# A shallow clone without CONVENTIONS_BASE (CI's checkout) prints SKIPPED; anywhere else a
# missing base fails.
. "$(dirname "$0")/lib.sh"
if ! git cat-file -e "$CONVENTIONS_BASE^{commit}" 2> /dev/null; then
    if [ "$(git rev-parse --is-shallow-repository)" = true ]; then
        echo "SKIPPED: shallow clone without the base commit ${CONVENTIONS_BASE:0:7}; the full history is checked locally"
        exit 0
    fi
    fail "Commits" "the base commit $CONVENTIONS_BASE is not in this history"
    exit 1
fi
# Commits already on main whose subject passed the length limit before this check caught it. A
# pushed subject cannot be fixed without rewriting history, which is never done, so each is named
# here by its full id and held to every other part of the rule.
too_long_on_main=(
    a19a713e1f86bc43b8925cf79dced5e1a4572c22 # PR #75, catalog v2 (110 characters)
)
rc=0
n=0
dash="$(printf '\342\200\224')"
while IFS= read -r c; do
    n=$((n + 1))
    s="$(git log -1 --format=%s "$c")"
    core="$(printf '%s' "$s" | sed 's/ (#[0-9]*)$//')"
    if ! [[ "$core" =~ ^[a-z0-9][a-z0-9/._-]*(,\ ?[a-z0-9][a-z0-9/._-]*)*:\ [^[:space:]] ]]; then
        echo "${c:0:7} $s"
        fail "Commits" "subject is not '<area>: <summary>'"
        rc=1
    elif [ "${#core}" -gt 100 ] && ! printf '%s\n' "${too_long_on_main[@]}" | command grep -qx "$c"; then
        echo "${c:0:7} $s"
        fail "Commits" "subject is ${#core} characters (limit 100)"
        rc=1
    fi
    if git log -1 --format=%B "$c" | command grep -q -F "$dash"; then
        echo "${c:0:7} $s"
        fail "Commits" "the message holds an em dash (U+2014)"
        rc=1
    fi
done < <(git rev-list --no-merges "$CONVENTIONS_BASE..HEAD")
[ "$rc" = 0 ] && echo "commits since ${CONVENTIONS_BASE:0:7}: $n, all '<area>: <summary>' with no em dash"
exit "$rc"
