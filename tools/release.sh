#!/usr/bin/env bash
# `make release VERSION=<x.y.z>`: the artifacts of one chorus release, built from
# a clean checkout of a commit on origin/main, into dist/v<x.y.z>/.
#
#   chorus-server-v<ver>-x86_64-unknown-linux-musl   the static server binary
#   chorus-server-v<ver>-oci.tar                     the OCI image as a tarball (tools/image.sh,
#                                                    tested there: unpacked, --help, GET /api/state)
#   chorus-soloist-v<ver>-oci.tar                    the Soloist receiver image as an OCI tarball
#                                                    (tools/soloist-image.sh, tested there); it
#                                                    holds PipeWire, WirePlumber and chorus-soloistd
#                                                    and NO Soloist file (docs/soloist.md)
#   chorus-soloist-v<ver>-NOTICES.md                 that image's third-party notices: every Debian
#                                                    package in it and where its source is
#   chorus-endpoint-esp32s3-v<ver>.bin               the endpoint application image
#   chorus-endpoint-esp32s3-v<ver>.tar.gz            bootloader, partition table, application
#                                                    and flasher_args.json (offsets and flags)
#   chorus-endpoint_<ver>_arm64.deb, _amd64.deb      the Linux endpoint package, one per
#                                                    architecture (tools/endpoint-package.sh,
#                                                    checked there; docs/linux-endpoint.md)
#   <crate>-<ver>.crate                              the source of every MPL-2.0 crate linked
#                                                    into a shipped binary (P9; docs/release.md),
#                                                    checked against Cargo.lock's checksum
#   SHA256SUMS, NOTES.md                             digests; the release notes body
#
# Nothing is published, flashed or pushed here: `gh release create` is the separate
# publish job of .github/workflows/release.yml (docs/release.md), flashing is the owner's
# at the bench, and .github/workflows/publish-images.yml pushes the images.
#
# `tools/release.sh --list` prints the artifact names of the workspace's version and builds
# nothing (`make soloist-lists` reads it); a release's directory is held to the same list
# before the digests are written, so the two cannot drift apart.
#
# Refuses by name when: the version does not match Cargo.toml's workspace version,
# the tree is dirty, HEAD is not on origin/main, or ESP-IDF is not the pinned one
# (tools/firmware-image.sh's own refusal).

set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$(pwd)"

TARGET=x86_64-unknown-linux-musl

# The MPL-2.0 crates of the shipped binaries (step 5), as "name version" lines.
mpl_crates() {
    cargo tree --locked -e normal -p chorus-client-linux -p chorus-hostprobe -p chorus-server \
        --prefix none --format '{p}|{l}' --target x86_64-unknown-linux-gnu |
        awk -F'|' '$2 ~ /MPL-2.0/ {split($1, a, " "); print a[1] " " substr(a[2], 2)}' | LC_ALL=C sort -u
}

# Every file a release of version $1 holds, one name a line.
artifact_names() {
    local ver="$1" name cver
    echo "chorus-server-v$ver-$TARGET"
    echo "chorus-server-v$ver-oci.tar"
    echo "chorus-soloist-v$ver-oci.tar"
    echo "chorus-soloist-v$ver-NOTICES.md"
    echo "chorus-endpoint-esp32s3-v$ver.bin"
    echo "chorus-endpoint-esp32s3-v$ver.tar.gz"
    echo "chorus-endpoint_${ver}_arm64.deb"
    echo "chorus-endpoint_${ver}_amd64.deb"
    while read -r name cver; do
        [ -n "$name" ] && echo "$name-$cver.crate"
    done < <(mpl_crates)
    echo "SHA256SUMS"
    echo "NOTES.md"
}

if [ "${1:-}" = --list ]; then
    artifact_names "$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)"
    exit 0
fi

VER="${1:-${VERSION:-}}"
[ -n "$VER" ] || { echo "release: REFUSED: no version (make release VERSION=x.y.z)"; exit 2; }
CARGO_VER="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)"
[ "$VER" = "$CARGO_VER" ] || { echo "release: REFUSED: VERSION=$VER but Cargo.toml says $CARGO_VER"; exit 2; }
[ -z "$(git status --porcelain)" ] || { echo "release: REFUSED: the tree is not clean"; git status --short; exit 2; }
git fetch -q origin main
git merge-base --is-ancestor HEAD origin/main || { echo "release: REFUSED: HEAD $(git rev-parse --short HEAD) is not on origin/main"; exit 2; }

TD="${CARGO_TARGET_DIR:-$ROOT/target}"
OUT="$ROOT/dist/v$VER"
rm -rf "$OUT"
mkdir -p "$OUT"
SHA="$(git rev-parse HEAD)"
echo "release: v$VER from $SHA into $OUT"

# 1. The server binary (static musl, the same build the image carries: the C in it,
# libopus, is compiled by the pinned zig exactly as tools/image.sh sets it up).
ZIG_VERSION="$(sed -n 's|^"core:zig" = "\(.*\)"$|\1|p' mise.toml)"
[ -n "$ZIG_VERSION" ] || { echo "release: REFUSED: mise.toml pins no core:zig"; exit 2; }
export CHORUS_ZIG_VERSION="$ZIG_VERSION"
export CC_x86_64_unknown_linux_musl="$ROOT/tools/zig-musl-cc.sh"
SOURCE_DATE_EPOCH="$(git log -1 --format=%ct)"
export SOURCE_DATE_EPOCH
export ZIG_GLOBAL_CACHE_DIR="${ZIG_GLOBAL_CACHE_DIR:-$TD/zig-cache}"
cargo build --release --locked --target "$TARGET" -p chorus-server --bins
install -m 0755 "$TD/$TARGET/release/chorus-server" "$OUT/chorus-server-v$VER-$TARGET"
"$OUT/chorus-server-v$VER-$TARGET" --help | head -n 1

# 2. The OCI image tarball, built and tested by tools/image.sh.
CHORUS_IMAGE_OUT="$OUT/chorus-server-v$VER-oci.tar" bash tools/image.sh
MANIFEST_DIGEST="$(tar -xOf "$OUT/chorus-server-v$VER-oci.tar" index.json |
    python3 -c 'import json,sys; m=json.load(sys.stdin)["manifests"]; assert len(m)==1; print(m[0]["digest"])')"

# 2b. The Soloist receiver image, built and tested by tools/soloist-image.sh, and its notices
# (the file the image carries at /usr/share/doc/chorus/THIRD-PARTY-NOTICES.md). No Soloist
# file is in it: the binary is the owner's, mounted at run time (docs/soloist.md).
CHORUS_SOLOIST_IMAGE_OUT="$OUT/chorus-soloist-v$VER-oci.tar" bash tools/soloist-image.sh
install -m 0644 "$TD/image/soloist-work/stage-chorus/usr/share/doc/chorus/THIRD-PARTY-NOTICES.md" \
    "$OUT/chorus-soloist-v$VER-NOTICES.md"
SOLOIST_DIGEST="$(tar -xOf "$OUT/chorus-soloist-v$VER-oci.tar" index.json |
    python3 -c 'import json,sys; m=json.load(sys.stdin)["manifests"]; assert len(m)==1; print(m[0]["digest"])')"
SOLOIST_PACKAGES="$(command grep -c -v -E '^(#|[[:space:]]*$)' deploy/soloist/debian-packages.pins)"

# 3. The endpoint firmware image (ESP-IDF at the pinned version; guard-checked).
if [ -z "${IDF_PATH:-}" ]; then
    # shellcheck disable=SC1090 # the rootless install's environment script
    . "${CHORUS_IDF_ENV:-/cache/esp/chorus-idf-export.sh}" > /dev/null
fi
FW="$TD/release-fw-$VER"
rm -rf "$FW"
CHORUS_IMAGE_OUT="$FW" IDF_PY_BUILD_JOBS="${IDF_PY_BUILD_JOBS:-2}" bash tools/firmware-image.sh
install -m 0644 "$FW/chorus-endpoint.bin" "$OUT/chorus-endpoint-esp32s3-v$VER.bin"
tar -C "$FW" --sort=name --mtime="@$(git log -1 --format=%ct)" --owner=0 --group=0 --numeric-owner \
    -czf "$OUT/chorus-endpoint-esp32s3-v$VER.tar.gz" \
    bootloader/bootloader.bin partition_table/partition-table.bin chorus-endpoint.bin flasher_args.json
IDF_VER="$(sed -n 's/^espidf_version *= *//p' firmware/config/endpoint.conf | head -n 1)"

# 4. The Linux endpoint packages, built and checked by tools/endpoint-package.sh.
CHORUS_PACKAGE_OUT="$OUT" bash tools/endpoint-package.sh arm64 amd64

# 5. The source of every MPL-2.0 crate in a shipped binary (P9): chorus-client links the
# Symphonia FLAC crates (ADR 0044) and chorus-server, since goal 16, those and the Symphonia
# crates of its other decoders (ADR 0122). cargo tree finds them all. Each .crate is the crates.io download, taken from the
# local registry cache or fetched, and must match the checksum Cargo.lock pins.
MPL_CRATES="$(mpl_crates)"
MPL_NOTES=""
while read -r name ver; do
    [ -n "$name" ] || continue
    want="$(awk -v n="$name" -v v="$ver" '$0 == "name = \"" n "\"" {f = 1; next}
        f && $0 == "version = \"" v "\"" {g = 1; next} f && g && /^checksum = / {gsub(/"/, "", $3); print $3; exit}
        /^\[\[package\]\]/ {f = 0; g = 0}' Cargo.lock)"
    [ -n "$want" ] || { echo "release: REFUSED: Cargo.lock has no checksum for $name $ver"; exit 2; }
    crate="$OUT/$name-$ver.crate"
    cached="$(command find "${CARGO_HOME:-$HOME/.cargo}/registry/cache" -name "$name-$ver.crate" 2>/dev/null | head -n 1 || true)"
    if [ -n "$cached" ]; then
        cp "$cached" "$crate"
    else
        curl -fsSL -o "$crate" "https://static.crates.io/crates/$name/$name-$ver.crate"
    fi
    [ "$(sha256sum "$crate" | cut -d' ' -f1)" = "$want" ] ||
        { echo "release: REFUSED: $name-$ver.crate does not match Cargo.lock's checksum $want"; exit 2; }
    MPL_NOTES="$MPL_NOTES
- \`$name\` $ver: https://crates.io/crates/$name/$ver, attached as \`$name-$ver.crate\`
  (sha256 \`$want\`, the checksum Cargo.lock pins)."
done <<< "$MPL_CRATES"
[ -n "$MPL_NOTES" ] || { echo "release: REFUSED: no MPL-2.0 crate found, but chorus-client and chorus-server link Symphonia (ADR 0044, ADR 0122)"; exit 2; }

# 6. Digests and notes. First: the directory holds exactly the names `--list` prints (less
# the two files written below), so the list `make soloist-lists` checks is this release's.
if ! diff <(artifact_names "$VER" | command grep -v -x -e SHA256SUMS -e NOTES.md | LC_ALL=C sort) \
    <(cd "$OUT" && command ls | LC_ALL=C sort); then
    echo "release: REFUSED: $OUT does not hold exactly the artifacts tools/release.sh --list names (above: < listed, > built)"
    exit 2
fi
(cd "$OUT" && sha256sum chorus-* ./*.crate | sed 's| \./| |' > SHA256SUMS)
EXTERNAL="$(command grep -c '^source = ' Cargo.lock || true)"
cat > "$OUT/NOTES.md" <<EOF
chorus v$VER, built from \`$SHA\` by \`tools/release.sh\` (docs/release.md says how).

What this release is: a development snapshot. Nothing in it has been heard, flashed or
deployed; every hardware criterion in \`docs/verification-record.md\` is NOT PASSED, and
no timing claim rests on it (BRIEF.md section 3.1 rule 3).

Artifacts:
- \`chorus-server-v$VER-$TARGET\`: the static server binary.
- \`chorus-server-v$VER-oci.tar\`: the server as an OCI image layout tarball (base
  \`gcr.io/distroless/static-debian12:nonroot\`, digest-pinned in tools/image.sh). Manifest
  digest \`$MANIFEST_DIGEST\`. \`.github/workflows/publish-images.yml\` pushes it to ghcr.io
  under the release's version (docs/release.md).
- \`chorus-soloist-v$VER-oci.tar\`: one Spotify Soloist receiver as an OCI image layout
  tarball: PipeWire, WirePlumber and \`chorus-soloistd\` on \`debian:trixie-slim\`
  (digest-pinned in tools/soloist-image.sh), with $SOLOIST_PACKAGES Debian packages pinned by
  sha256 in deploy/soloist/debian-packages.pins. Manifest digest \`$SOLOIST_DIGEST\`. It holds
  no Spotify software: Soloist is proprietary, and whoever runs the image mounts their own
  binary and API key (docs/soloist.md). It is in no registry.
- \`chorus-soloist-v$VER-NOTICES.md\`: that image's notices: every Debian package in it, its
  version, and the Debian source package that is its source, at snapshot.debian.org.
- \`chorus-endpoint-esp32s3-v$VER.bin\` and \`.tar.gz\`: the ESP32-S3 endpoint image built
  with ESP-IDF $IDF_VER, with the bootloader, partition table and flasher_args.json. The
  firmware safety scans (no eFuse writes, Secure Boot, Flash Encryption or anti-rollback)
  passed on this build. It is not yet a working speaker (no I2S playout path; goals 8-9),
  and flashing is the owner's act at the bench.
- \`chorus-endpoint_${VER}_arm64.deb\` and \`chorus-endpoint_${VER}_amd64.deb\`: the Linux
  endpoint package (chorus-client, its systemd unit \`chorus-client.service\`, the config
  \`/etc/chorus/client.conf\`, real-time limits, and \`chorus-verify-host\`), cross-built
  for glibc 2.36 and later (Debian 12 and 13, Raspberry Pi OS and its Legacy release) and
  checked by \`tools/endpoint-package.sh\`. docs/linux-endpoint.md installs it; installing
  it on a speaker is the owner's act.

Licences: chorus is MIT OR Apache-2.0 (LICENSE-MIT, LICENSE-APACHE). Dependencies: the
Rust workspace has $EXTERNAL external crates (Cargo.lock \`source =\` lines), each under a
licence on the allowlist in \`deny.toml\` (checked by cargo-deny in \`make gate\`), or an
exception its ADR names. chorus-server (the binary and the image) links the Symphonia
crates below, MPL-2.0, unmodified (ADR 0122); chorus-client, in the Linux endpoint
packages, links the FLAC ones among them (ADR 0044). Their source is:
$MPL_NOTES

chorus-server and chorus-client also carry libopus 1.6.1 (BSD-3-Clause). The image
reproduces its licence in /usr/share/doc/chorus/libopus-COPYING and names the Symphonia
crates and where their source is in /usr/share/doc/chorus/THIRD-PARTY-NOTICES.md; each
endpoint package's /usr/share/doc/chorus-endpoint/copyright lists every crate and
reproduces the licences. Whoever passes the bare server binary on passes those two files
on with it. The firmware links ESP-IDF components (ESP-IDF is Apache-2.0; its bundled
third-party components carry their own licences, listed in ESP-IDF's
\`docs/en/COPYRIGHT.rst\`), whose source is ESP-IDF $IDF_VER at the commit pinned in
\`firmware/config/endpoint.conf\`.

SHA256SUMS:
\`\`\`
$(cat "$OUT/SHA256SUMS")
\`\`\`
EOF
echo "release: artifacts in $OUT"
ls -l "$OUT"
echo "release: OCI manifest digest $MANIFEST_DIGEST"
echo "release: chorus-soloist OCI manifest digest $SOLOIST_DIGEST"
