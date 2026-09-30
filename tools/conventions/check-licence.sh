#!/usr/bin/env bash
# Rule "Licence" (K26) and rule "Dependencies and licences" (brief section 4.7): chorus is
# MIT OR Apache-2.0, with both texts at the root and the field in every crate; every crate the
# workspace builds is on deny.toml's allowlist, from crates.io only, one version each.
. "$(dirname "$0")/lib.sh"
rc=0
for f in LICENSE-MIT LICENSE-APACHE; do
    [ -s "$f" ] || { fail "Licence" "$f is missing or empty"; rc=1; }
done
command grep -q '^MIT License$' LICENSE-MIT 2> /dev/null || { fail "Licence" "LICENSE-MIT is not the MIT text"; rc=1; }
command grep -q 'Apache License' LICENSE-APACHE 2> /dev/null &&
    command grep -q 'Version 2.0, January 2004' LICENSE-APACHE || { fail "Licence" "LICENSE-APACHE is not the Apache-2.0 text"; rc=1; }

# Every workspace member declares exactly the project licence.
need cargo || exit 1
meta="$(cargo metadata --no-deps --format-version 1 --locked)" || exit 1
bad="$(printf '%s' "$meta" | jq -r '.packages[] | select(.license != "MIT OR Apache-2.0") | "\(.manifest_path): license = \(.license)"')"
if [ -n "$bad" ]; then
    printf '%s\n' "$bad"
    fail "Licence" "every crate carries license.workspace = true (MIT OR Apache-2.0)"
    rc=1
fi
echo "crates with license = \"MIT OR Apache-2.0\": $(printf '%s' "$meta" | jq '[.packages[] | select(.license == "MIT OR Apache-2.0")] | length') of $(printf '%s' "$meta" | jq '.packages | length')"

need cargo-deny || exit 1
cargo deny --locked check licenses bans sources 2>&1 | tail -n 20
[ "${PIPESTATUS[0]}" -eq 0 ] || { fail "Dependencies and licences" "cargo deny found a crate off the allowlist (deny.toml)"; rc=1; }
exit "$rc"
