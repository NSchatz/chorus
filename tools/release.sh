#!/usr/bin/env bash
# `make release VERSION=<x.y.z>`: the artifacts of one chorus release, built from
# a clean checkout of a commit on origin/main, into dist/v<x.y.z>/.
#
#   chorus-server-v<ver>-x86_64-unknown-linux-musl   the static server binary
#   chorus-server-v<ver>-oci.tar                     the OCI image as a tarball (tools/image.sh,
#                                                    tested there: unpacked, --help, GET /api/state)
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
# Nothing is published, flashed or pushed here: `gh release create` is a separate,
# visible step (docs/release.md), flashing is the owner's at the bench, and pushing
# the image to a registry is the owner's Needs step.
#
# Refuses by name when: the version does not match Cargo.toml's workspace version,
# the tree is dirty, HEAD is not on origin/main, or ESP-IDF is not the pinned one
# (tools/firmware-image.sh's own refusal).

set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$(pwd)"

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
TARGET=x86_64-unknown-linux-musl
echo "release: v$VER from $SHA into $OUT"

# 1. The server binary (static musl, the same build the image carries).
cargo build --release --locked --target "$TARGET" -p chorus-server --bins
install -m 0755 "$TD/$TARGET/release/chorus-server" "$OUT/chorus-server-v$VER-$TARGET"
"$OUT/chorus-server-v$VER-$TARGET" --help | head -n 1

# 2. The OCI image tarball, built and tested by tools/image.sh.
CHORUS_IMAGE_OUT="$OUT/chorus-server-v$VER-oci.tar" bash tools/image.sh
MANIFEST_DIGEST="$(tar -xOf "$OUT/chorus-server-v$VER-oci.tar" index.json |
    python3 -c 'import json,sys; m=json.load(sys.stdin)["manifests"]; assert len(m)==1; print(m[0]["digest"])')"

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
# Symphonia FLAC crates (ADR 0044). Each .crate is the crates.io download, taken from the
# local registry cache or fetched, and must match the checksum Cargo.lock pins.
MPL_CRATES="$(cargo tree --locked -e normal -p chorus-client-linux -p chorus-hostprobe -p chorus-server \
    --prefix none --format '{p}|{l}' --target x86_64-unknown-linux-gnu |
    awk -F'|' '$2 ~ /MPL-2.0/ {split($1, a, " "); print a[1] " " substr(a[2], 2)}' | LC_ALL=C sort -u)"
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
[ -n "$MPL_NOTES" ] || { echo "release: REFUSED: no MPL-2.0 crate found, but chorus-client links Symphonia (ADR 0044)"; exit 2; }

# 6. Digests and notes.
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
  digest \`$MANIFEST_DIGEST\`. It is in no registry; pushing it is the owner's step
  (docs/release.md).
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
exception its ADR names. The server links no MPL-2.0 crate; chorus-client, in the Linux
endpoint packages, links these MPL-2.0 crates unmodified (ADR 0044), whose source is:
$MPL_NOTES

The endpoint packages also carry libopus 1.6.1 (BSD-3-Clause) inside chorus-client; each
package's /usr/share/doc/chorus-endpoint/copyright lists every crate and reproduces the
licences. The firmware links ESP-IDF components (ESP-IDF is Apache-2.0; its bundled
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
