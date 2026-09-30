#!/usr/bin/env bash
# Rule "Pins" (brief section 0.9): everything that builds or checks chorus is pinned to an exact
# version, and to a digest where one exists; the three places that name the Rust toolchain agree.
#   rust-toolchain.toml channel = Cargo.toml rust-version = the Dockerfile's rust:<ver> base
#   every Dockerfile FROM carries @sha256:<64 hex>
#   every workflow `uses:` names a 40-hex commit
#   mise.toml: exact x.y.z versions only, and mise.lock holds a sha256 for every binary tool
#   ESP-IDF: firmware/config/endpoint.conf names an exact tag and its 40-hex commit, and CI
#   clones that tag
#   the gate builds with --locked; any package.json pins exact versions beside a lockfile
. "$(dirname "$0")/lib.sh"
rc=0
bad() { fail "Pins" "$1"; rc=1; }

chan="$(sed -n 's/^channel = "\(.*\)"$/\1/p' rust-toolchain.toml)"
msrv="$(sed -n 's/^rust-version = "\(.*\)"$/\1/p' Cargo.toml)"
dock="$(sed -n 's/^FROM .*rust:\([0-9][0-9.]*\)-.*/\1/p' deploy/Dockerfile | head -n 1)"
[[ "$chan" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || bad "rust-toolchain.toml channel '$chan' is not an exact x.y.z"
[ "$chan" = "$msrv" ] && [ "$chan" = "$dock" ] ||
    bad "the Rust toolchain disagrees: rust-toolchain.toml $chan, Cargo.toml rust-version $msrv, deploy/Dockerfile rust:$dock"
echo "rust: rust-toolchain.toml $chan, rust-version $msrv, Dockerfile rust:$dock"

while IFS= read -r l; do
    [[ "$l" =~ @sha256:[0-9a-f]{64} ]] || bad "a FROM without a digest: $l"
done < <(git grep -h -i -E '^FROM ' -- '*Dockerfile*' ':!tools/conventions/fixtures/')
echo "dockerfile FROM lines: $(git grep -h -i -E '^FROM ' -- '*Dockerfile*' ':!tools/conventions/fixtures/' | wc -l)"

while IFS= read -r l; do
    [[ "$l" =~ uses:[[:space:]]*[^@[:space:]]+@[0-9a-f]{40}([[:space:]]|$) ]] || bad "a workflow action not pinned by commit: $l"
done < <(git grep -h -E '^[[:space:]-]*uses:' -- .github)
echo "workflow actions: $(git grep -h -E '^[[:space:]-]*uses:' -- .github | wc -l)"

tools=0
while IFS= read -r l; do
    tools=$((tools + 1))
    [[ "$l" =~ ^\"[^\"]+\"[[:space:]]*=[[:space:]]*\"[0-9]+(\.[0-9]+)+\"$ ]] || bad "mise.toml tool not pinned exactly: $l"
    t="$(printf '%s' "$l" | sed 's/^"\([^"]*\)".*/\1/')"
    case "$t" in
        pipx:*)
            v="$(printf '%s' "$l" | sed 's/.*= *"\(.*\)"$/\1/')"
            w="$(printf '%s' "${t#pipx:}" | tr - _)"
            command grep -E -q "^#[[:space:]]+wheel $w-$v-[^ ]+\.whl sha256 [0-9a-f]{64}$" mise.toml ||
                bad "$t has no 'wheel $w-$v-...whl sha256 <digest>' comment in mise.toml" ;;
        *) command grep -A3 -F "[tools.\"$t\".\"platforms.linux-x64\"]" mise.lock | command grep -q '^checksum = "sha256:' ||
            bad "mise.lock has no linux-x64 sha256 for $t (run \`mise lock\`)" ;;
    esac
done < <(sed -n '/^\[tools\]/,/^\[/p' mise.toml | command grep -E '^"')
echo "mise.toml tools: $tools"

ver="$(sed -n 's/^espidf_version = //p' firmware/config/endpoint.conf)"
sha="$(sed -n 's/^espidf_commit = //p' firmware/config/endpoint.conf)"
[[ "$ver" =~ ^v[0-9]+\.[0-9]+(\.[0-9]+)?$ ]] || bad "espidf_version '$ver' is not an exact tag"
[[ "$sha" =~ ^[0-9a-f]{40}$ ]] || bad "espidf_commit '$sha' is not a 40-hex commit"
command grep -q -- "--branch $ver " .github/workflows/ci.yml || bad "ci.yml does not clone ESP-IDF $ver"
echo "esp-idf: $ver at $sha"

for s in build test clippy; do
    command grep -E "cargo $s .*--locked" tools/gate.sh > /dev/null || bad "the gate's cargo $s does not pass --locked"
done

while IFS= read -r p; do
    d="$(dirname "$p")"
    [ -f "$d/package-lock.json" ] || [ -f "$d/pnpm-lock.yaml" ] || bad "$p has no committed lockfile"
    jq -r '(.dependencies // {}) + (.devDependencies // {}) | to_entries[] | select(.value | test("^[0-9]+\\.[0-9]+\\.[0-9]+$") | not) | "\(.key)@\(.value)"' "$p" |
        while IFS= read -r dep; do echo "FAIL: $p: $dep is not an exact version"; done | command grep . && rc=1
done < <(git ls-files '*package.json')
exit "$rc"
