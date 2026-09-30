#!/usr/bin/env bash
# Rule "Identity and secrets" (K27, I5, I18): no personal name, email, LAN address, MAC address
# or secret in any tracked file or commit message.
#   1. The private term list (outside the repository, never printed) matched case-sensitively
#      over tracked files and every commit message since CONVENTIONS_BASE; the one allowed
#      form of the account handle, the `NSchatz/` repo prefix, is removed before matching.
#      The list missing fails the gate; under CI (CI=true) it prints SKIPPED with the reason.
#   2. Private IPv4 addresses (RFC 1918, dotted or as octet tuples) and MAC addresses other than the documentation
#      block 00-00-5E-00-53-xx (RFC 7042) in tracked files. Examples use RFC 5737 addresses.
#   3. gitleaks over the whole history and over the tracked tree.
. "$(dirname "$0")/lib.sh"
terms="${CHORUS_IDENTITY_TERMS:-/cache/chorus-private/identity-terms.txt}"
rc=0
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

# 1. the term list
if [ ! -r "$terms" ]; then
    if [ "${CI:-}" = true ]; then
        echo "SKIPPED: the private identity term list ($terms) is not present under CI; it lives only on the owner's and the program's machines (brief section 0.7, I5)"
    else
        fail "Identity and secrets" "the private identity term list $terms is missing; it is required outside CI (I5). Set CHORUS_IDENTITY_TERMS to its path."
        rc=1
    fi
else
    command grep -v -e '^#' -e '^[[:space:]]*$' "$terms" > "$scratch/terms"
    n="$(wc -l < "$scratch/terms")"
    [ "$n" -gt 0 ] || { fail "Identity and secrets" "the term list $terms holds no terms"; rc=1; }
    # Tracked files: lines holding a term once the allowed prefix is removed.
    git grep -I -n -F -f "$scratch/terms" -- . |
        awk '{ line = $0; gsub(/NSchatz\//, "", line); print line }' |
        command grep -F -f "$scratch/terms" | cut -d: -f1,2 > "$scratch/tree-hits" || true
    git log --format='%H%n%B' "$CONVENTIONS_BASE..HEAD" 2> /dev/null |
        sed 's#NSchatz/##g' | command grep -n -F -f "$scratch/terms" | cut -d: -f1 > "$scratch/msg-hits" || true
    if [ -s "$scratch/tree-hits" ] || [ -s "$scratch/msg-hits" ]; then
        # Where, never what: the terms themselves are not printed.
        sed 's/^/term found at /' "$scratch/tree-hits"
        [ -s "$scratch/msg-hits" ] && echo "term found in commit messages since ${CONVENTIONS_BASE:0:7} (log lines $(paste -sd, "$scratch/msg-hits"))"
        fail "Identity and secrets" "an identity term is in the tree or a commit message; write \"the owner\""
        rc=1
    else
        echo "identity terms: $n terms, 0 hits in $(git ls-files | wc -l) tracked files and $(git rev-list --count "$CONVENTIONS_BASE..HEAD" 2> /dev/null || echo 0) commit messages"
    fi
fi

# 2. LAN and MAC addresses
lan='(^|[^0-9.])(10\.[0-9]{1,3}|192\.168|172\.(1[6-9]|2[0-9]|3[01]))\.[0-9]{1,3}\.[0-9]{1,3}($|[^0-9.])'
mac='(^|[^0-9A-Fa-f:-])([0-9A-Fa-f]{2}[:-]){5}[0-9A-Fa-f]{2}($|[^0-9A-Fa-f:-])'
# The same ranges written as octet tuples, as Rust's Ipv4Addr::new and C initialisers spell them.
tuple='Ipv4Addr::new\((10|192, *168|172, *(1[6-9]|2[0-9]|3[01])),|[{[( ]192, *168, *[0-9]{1,3}, *[0-9]{1,3}'
addr="$(git grep -I -n -E -e "$lan" -e "$tuple" -- . || true)"
macs="$(git grep -I -n -i -E -e "$mac" -- . | command grep -v -i -E '00[:-]00[:-]5e[:-]00[:-]53[:-][0-9a-f]{2}' || true)"
if [ -n "$addr$macs" ]; then
    printf '%s\n' "$addr" "$macs" | sed '/^$/d' | cut -c1-160
    fail "Identity and secrets" "a private IPv4 or MAC address is tracked; use RFC 5737 (192.0.2.0/24) or RFC 7042 (00-00-5E-00-53-xx) examples"
    rc=1
else
    echo "addresses: no RFC 1918 IPv4 or non-documentation MAC address in tracked files"
fi

# 3. gitleaks
need gitleaks || exit 1
if ! gitleaks git --no-banner --no-color --redact --config .gitleaks.toml . > "$scratch/gl-git" 2>&1; then
    tail -n 20 "$scratch/gl-git"
    fail "Identity and secrets" "gitleaks found a secret in the history"
    rc=1
else
    tail -n 1 "$scratch/gl-git"
fi
git ls-files -z | tar --null -T - -cf - | tar -xf - -C "$scratch" --one-top-level=tree 2> /dev/null ||
    { mkdir -p "$scratch/tree" && git ls-files -z | tar --null -T - -cf - | tar -xf - -C "$scratch/tree"; }
if ! gitleaks dir --no-banner --no-color --redact --config .gitleaks.toml "$scratch/tree" > "$scratch/gl-dir" 2>&1; then
    tail -n 20 "$scratch/gl-dir"
    fail "Identity and secrets" "gitleaks found a secret in the tracked tree"
    rc=1
else
    tail -n 1 "$scratch/gl-dir"
fi
exit "$rc"
