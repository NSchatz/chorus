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

# 4. Digests and notes.
(cd "$OUT" && sha256sum chorus-* > SHA256SUMS)
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

Licences: chorus is MIT OR Apache-2.0 (LICENSE-MIT, LICENSE-APACHE). Dependencies: the
Rust workspace has $EXTERNAL external crates (Cargo.lock \`source =\` lines), so no
MPL-2.0 or other third-party source is in the server; the firmware links ESP-IDF
(Apache-2.0) components, whose source is ESP-IDF $IDF_VER at the commit pinned in
\`firmware/config/endpoint.conf\`. When an MPL-licensed dependency arrives, its source
location is listed here (docs/release.md).

SHA256SUMS:
\`\`\`
$(cat "$OUT/SHA256SUMS")
\`\`\`
EOF
echo "release: artifacts in $OUT"
ls -l "$OUT"
echo "release: OCI manifest digest $MANIFEST_DIGEST"
