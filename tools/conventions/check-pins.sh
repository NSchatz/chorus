#!/usr/bin/env bash
# Rule "Pins" (brief section 0.9): everything that builds or checks chorus is pinned to an exact
# version, and to a digest where one exists; the three places that name the Rust toolchain agree.
#   rust-toolchain.toml channel = Cargo.toml rust-version = the Dockerfile's rust:<ver> base
#   every Dockerfile FROM carries @sha256:<64 hex>
#   every workflow `uses:` names a 40-hex commit
#   mise.toml: exact x.y.z versions only, and mise.lock holds a sha256 for every binary tool
#   ESP-IDF: firmware/config/endpoint.conf names an exact tag and its 40-hex commit, and CI
#   clones that tag
#   the emulator (goal 14): tools/qemu/pins.conf names an exact release, its archive's URL and
#   sha256, the program's sha256 and micromamba's version, URL and sha256; every package of
#   tools/qemu/libs.explicit.txt is a conda-forge linux-64 build held to its sha256
#   the chorus-soloist image (goal 17): tools/soloist-image.sh names its Debian base by dated tag
#   and digest; every package of deploy/soloist/debian-packages.pins is an exact version with
#   its sha256 and size at one snapshot.debian.org timestamp
#   the gate builds and tests with --locked (cargo build, test, clippy and nextest run); any
#   package.json pins exact versions beside a lockfile
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
        *)
            # mise writes a core backend's tool under its bare name, unquoted
            # (`[tools.zig."platforms.linux-x64"]`); every other one quoted.
            k="\"$t\""
            case "$t" in core:*) k="${t#core:}" ;; esac
            command grep -A3 -F "[tools.$k.\"platforms.linux-x64\"]" mise.lock | command grep -q '^checksum = "sha256:' ||
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

# ESP-IDF components (goal 8): every registry dependency in an idf_component.yml
# names an exact `==x.y.z`, and the committed firmware/dependencies.lock holds a
# 64-hex component_hash for it at that version.
comps=0
while IFS= read -r l; do
    name="$(printf '%s' "$l" | sed 's/^[[:space:]]*\([^:]*\):.*/\1/')"
    [ "$name" = idf ] && continue
    comps=$((comps + 1))
    [[ "$l" =~ :[[:space:]]*\"==[0-9]+\.[0-9]+\.[0-9]+\"[[:space:]]*$ ]] || { bad "an ESP-IDF component not pinned with ==x.y.z: $l"; continue; }
    v="$(printf '%s' "$l" | sed 's/.*"==\(.*\)"/\1/')"
    awk -v n="  $name:" -v v="    version: $v" '
        $0 == n {f = 1; next} f && /^  [^ ]/ {f = 0}
        f && /^    component_hash: [0-9a-f]{64}$/ {h = 1} f && $0 == v {m = 1}
        END {exit !(h && m)}' firmware/dependencies.lock ||
        bad "firmware/dependencies.lock has no component_hash for $name at $v"
done < <(git ls-files 'firmware/*idf_component.yml' | xargs -r sed -n '/^dependencies:/,/^[^ ]/p' | command grep -E '^[[:space:]]+[a-z0-9_./-]+:[[:space:]]*"')
echo "esp-idf components: $comps"

# The emulator and its library environment (goal 14): the record the install and
# the runs read (tools/qemu/lib.sh), held to exact versions and sha256 digests.
qpin() { sed -n "s/^$1 = //p" tools/qemu/pins.conf | head -n 1; }
qver="$(qpin qemu_version)"
qurl="$(qpin qemu_asset_url)"
[[ "$qver" =~ ^esp_develop_[0-9]+\.[0-9]+\.[0-9]+_[0-9]{8}$ ]] || bad "tools/qemu/pins.conf qemu_version '$qver' is not an exact release (esp_develop_x.y.z_yyyymmdd)"
[[ "$qurl" == https://github.com/espressif/qemu/releases/download/*"$qver"* ]] || bad "tools/qemu/pins.conf qemu_asset_url does not name the release $qver"
for k in qemu_asset_sha256 qemu_binary_sha256 micromamba_sha256; do
    [[ "$(qpin "$k")" =~ ^[0-9a-f]{64}$ ]] || bad "tools/qemu/pins.conf $k is not a sha256"
done
mver="$(qpin micromamba_version)"
[[ "$mver" =~ ^[0-9]+\.[0-9]+\.[0-9]+-[0-9]+$ ]] || bad "tools/qemu/pins.conf micromamba_version '$mver' is not an exact release"
[[ "$(qpin micromamba_url)" == https://github.com/mamba-org/micromamba-releases/releases/download/"$mver"/* ]] || bad "tools/qemu/pins.conf micromamba_url does not name the release $mver"
libs=0
command grep -q -x '@EXPLICIT' tools/qemu/libs.explicit.txt || bad "tools/qemu/libs.explicit.txt is not an explicit list (no @EXPLICIT line)"
while IFS= read -r l; do
    libs=$((libs + 1))
    [[ "$l" =~ ^https://conda\.anaconda\.org/conda-forge/(linux-64|noarch)/[A-Za-z0-9._+-]+-[^-]+-[^-]+\.(conda|tar\.bz2)#sha256:[0-9a-f]{64}$ ]] ||
        bad "a library of the emulator not pinned to a conda-forge build and its sha256: $l"
done < <(command grep -v -E '^(#|@EXPLICIT$|[[:space:]]*$)' tools/qemu/libs.explicit.txt)
[ "$libs" -gt 0 ] || bad "tools/qemu/libs.explicit.txt lists no package"
echo "emulator: QEMU $qver, micromamba $mver, $libs library packages, each with a sha256"

# The chorus-soloist image (goal 17): its base by digest in tools/soloist-image.sh, and every
# Debian package in deploy/soloist/debian-packages.pins an exact version with its sha256 and
# size, fetched from snapshot.debian.org at the one timestamp the file names.
spins=deploy/soloist/debian-packages.pins
snap="$(sed -n 's/^# snapshot = //p' "$spins")"
[[ "$snap" =~ ^[0-9]{8}T[0-9]{6}Z$ ]] || bad "$spins names no snapshot timestamp (# snapshot = yyyymmddThhmmssZ)"
[ "$(sed -n 's/^# archive = //p' "$spins")" = "https://snapshot.debian.org/archive/" ] || bad "$spins does not fetch from https://snapshot.debian.org/archive/"
command grep -E -q '^BASE_DIGEST="sha256:[0-9a-f]{64}"$' tools/soloist-image.sh || bad "tools/soloist-image.sh pins no base digest (BASE_DIGEST=\"sha256:...\")"
command grep -E -q '^BASE_TAG="trixie-[0-9]{8}-slim"$' tools/soloist-image.sh || bad "tools/soloist-image.sh names no dated base tag (BASE_TAG=\"trixie-yyyymmdd-slim\")"
debs=0
while read -r pkg ver arch sha size src srcver path rest; do
    debs=$((debs + 1))
    [[ "$pkg" =~ ^[a-z0-9][a-z0-9+.-]+$ && "$ver" =~ ^[0-9][A-Za-z0-9.+~:-]*$ && "$arch" =~ ^(amd64|all)$ &&
        "$sha" =~ ^[0-9a-f]{64}$ && "$size" =~ ^[0-9]+$ && "$src" =~ ^[a-z0-9][a-z0-9+.-]+$ && -n "$srcver" && -z "$rest" &&
        "$path" == debian*/"$snap"/pool/*_"$arch".deb ]] ||
        bad "a package of the chorus-soloist image not pinned to a version, a sha256 and the snapshot $snap: $pkg $ver"
done < <(command grep -v -E '^(#|[[:space:]]*$)' "$spins")
[ "$debs" -gt 0 ] || bad "$spins lists no package"
[ "$(command grep -v -E '^(#|[[:space:]]*$)' "$spins" | cut -d' ' -f1 | sort | uniq -d | wc -l)" -eq 0 ] || bad "$spins lists a package twice"
echo "soloist image: base $(sed -n 's/^BASE_TAG="\(.*\)"$/\1/p' tools/soloist-image.sh) by digest, $debs Debian packages at snapshot $snap, each with a sha256"

for s in build test clippy 'nextest run'; do
    command grep -E "cargo $s .*--locked" tools/gate.sh > /dev/null || bad "the gate's cargo $s does not pass --locked"
done

while IFS= read -r p; do
    d="$(dirname "$p")"
    [ -f "$d/package-lock.json" ] || [ -f "$d/pnpm-lock.yaml" ] || bad "$p has no committed lockfile"
    jq -r '(.dependencies // {}) + (.devDependencies // {}) | to_entries[] | select(.value | test("^[0-9]+\\.[0-9]+\\.[0-9]+$") | not) | "\(.key)@\(.value)"' "$p" |
        while IFS= read -r dep; do echo "FAIL: $p: $dep is not an exact version"; done | command grep . && rc=1
done < <(git ls-files '*package.json')
exit "$rc"
