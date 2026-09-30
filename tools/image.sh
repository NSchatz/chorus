#!/usr/bin/env bash
# `make image`: the chorus-server OCI image, built with no container daemon, and
# its test.
#
# A static (musl) chorus-server goes onto a digest-pinned distroless base with
# umoci, and the OCI layout is written as one tarball. The test then unpacks the
# tarball the way a runtime would (umoci unpack, rootless), runs the unpacked
# binary's `--help`, and starts it with the control plane on loopback and reads
# `GET /api/state` back, so the image is shown to hold a server that serves.
#
# Nothing is pushed anywhere: pushing to a registry is the owner's step.
#
# Tools (rootless, pinned): crane 0.22.1 (aqua:google/go-containerregistry) and
# umoci 0.6.0 (aqua:opencontainers/umoci), run through mise; the Rust toolchain
# and its musl target from rust-toolchain.toml.
#
# Output: CHORUS_IMAGE_OUT (default target/image/chorus-server-oci.tar).

set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$(pwd)"

# gcr.io/distroless/static-debian12:nonroot, the multi-platform index digest,
# resolved with `crane digest` on 2026-09-29 and again on 2026-09-30 (unchanged).
BASE_REF="gcr.io/distroless/static-debian12"
BASE_DIGEST="sha256:afa5c872c891853ca7fcf1f12c3edb23f7eeef36189728842dd51042ff57f7ab"
CRANE=(mise exec aqua:google/go-containerregistry@0.22.1 -- crane)
UMOCI=(mise exec aqua:opencontainers/umoci@0.6.0 -- umoci)
TARGET=x86_64-unknown-linux-musl

TD="${CARGO_TARGET_DIR:-$ROOT/target}"
OUT="${CHORUS_IMAGE_OUT:-$TD/image/chorus-server-oci.tar}"
WORK="$TD/image/work"
BASE_CACHE="${CHORUS_IMAGE_BASE_CACHE:-$TD/image/base-${BASE_DIGEST#sha256:}}"

# Every timestamp in the image is the commit's, so two builds of one commit
# differ only where umoci itself is not reproducible.
EPOCH="$(git log -1 --format=%ct)"
CREATED="$(date -u -d "@$EPOCH" +%Y-%m-%dT%H:%M:%SZ)"
REVISION="$(git rev-parse HEAD)"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)"

echo "image: building chorus-server for $TARGET"
cargo build --release --locked --target "$TARGET" -p chorus-server --bins

if [ ! -f "$BASE_CACHE/index.json" ]; then
    echo "image: pulling $BASE_REF@$BASE_DIGEST"
    rm -rf "$BASE_CACHE.tmp"
    "${CRANE[@]}" pull --format=oci --platform linux/amd64 "$BASE_REF@$BASE_DIGEST" "$BASE_CACHE.tmp"
    mv "$BASE_CACHE.tmp" "$BASE_CACHE"
fi

rm -rf "$WORK"
mkdir -p "$WORK" "$(dirname "$OUT")"
cp -r "$BASE_CACHE" "$WORK/layout"
# umoci addresses a manifest by its ref name, which crane does not write.
python3 - "$WORK/layout/index.json" <<'EOF'
import json, sys
p = sys.argv[1]
index = json.load(open(p))
assert len(index["manifests"]) == 1, "expected one platform manifest"
index["manifests"][0].setdefault("annotations", {})["org.opencontainers.image.ref.name"] = "base"
json.dump(index, open(p, "w"))
EOF

STAGE="$WORK/stage"
mkdir -p "$STAGE/usr/local/bin" "$STAGE/etc/chorus"
install -m 0755 "$TD/$TARGET/release/chorus-server" "$STAGE/usr/local/bin/chorus-server"
install -m 0755 "$TD/$TARGET/release/chorus-rt-spin" "$STAGE/usr/local/bin/chorus-rt-spin"
install -m 0644 config/verification.conf "$STAGE/etc/chorus/verification.conf"
# The zone state directory. A rootless insert records every file as root's, and
# the image runs as 65532, so the directory is world-writable with the sticky
# bit (as /tmp is) rather than owned; a mounted volume takes its place in a run.
mkdir -p "$STAGE/var/lib/chorus"
chmod 1777 "$STAGE/var/lib/chorus"
find "$STAGE" -exec touch -h -d "@$EPOCH" {} +

L="$WORK/layout"
"${UMOCI[@]}" insert --rootless --image "$L:base" --tag chorus-server \
    --history.created "$CREATED" --history.created_by "tools/image.sh" "$STAGE" /
# The default run is the deployable shape (B-15): audio on 4010, the control
# plane on 4020, zones persisted under /var/lib/chorus. Host networking and the
# real-time limits are the run's to grant; deploy/run-server.sh grants them.
"${UMOCI[@]}" config --image "$L:chorus-server" --created "$CREATED" \
    --history.created "$CREATED" --history.created_by "tools/image.sh" \
    --config.entrypoint /usr/local/bin/chorus-server \
    --clear config.cmd \
    --config.cmd --listen --config.cmd 0.0.0.0:4010 \
    --config.cmd --control-listen --config.cmd 0.0.0.0:4020 \
    --config.cmd --state-file --config.cmd /var/lib/chorus/zones.state \
    --config.cmd --source --config.cmd tone \
    --config.cmd --serve-forever \
    --config.exposedports 4010/tcp --config.exposedports 4020/tcp \
    --config.volume /var/lib/chorus \
    --config.user 65532:65532 \
    --config.label "org.opencontainers.image.title=chorus-server" \
    --config.label "org.opencontainers.image.version=$VERSION" \
    --config.label "org.opencontainers.image.revision=$REVISION" \
    --config.label "org.opencontainers.image.base.name=$BASE_REF@$BASE_DIGEST"
"${UMOCI[@]}" rm --image "$L:base"
"${UMOCI[@]}" gc --layout "$L"

(cd "$L" && tar --sort=name --mtime="@$EPOCH" --owner=0 --group=0 --numeric-owner \
    -cf "$OUT" oci-layout index.json blobs)
echo "image: $OUT, $(du -k "$OUT" | cut -f1) KiB, sha256 $(sha256sum "$OUT" | cut -d' ' -f1)"

# --- the test ------------------------------------------------------------------

# deploy/Dockerfile's build context, staged from its COPY lines, must compile on
# its own (B-14: it once lacked docs/, which the server includes at build time).
# No daemon is needed to show that: the same paths, the same cargo command.
CTX="$WORK/docker-context"
mkdir -p "$CTX"
sed -n 's/^COPY \([^-][^ ]*\) \(.*\)$/\1 \2/p' deploy/Dockerfile | while read -r src dst; do
    mkdir -p "$CTX/$(dirname "${dst#./}")"
    cp -r "$src" "$CTX/${dst#./}"
done
cp rust-toolchain.toml "$CTX/"   # the Dockerfile's base is the same pinned release
echo "image test: the Dockerfile's build context: $(cd "$CTX" && find . -maxdepth 1 -mindepth 1 | sort | tr '\n' ' ')"
(cd "$CTX" && CARGO_TARGET_DIR="$TD/image/docker-context-target" cargo check --locked --workspace --bins --quiet)
echo "image test: the Dockerfile's build context compiles"

T="$WORK/test"
mkdir -p "$T/layout"
tar -xf "$OUT" -C "$T/layout"
"${UMOCI[@]}" unpack --rootless --image "$T/layout:chorus-server" "$T/bundle" > /dev/null
BIN="$T/bundle/rootfs/usr/local/bin/chorus-server"
python3 - "$T/bundle/config.json" <<'EOF'
import json, sys
c = json.load(open(sys.argv[1]))
args = c["process"]["args"]
assert args[0] == "/usr/local/bin/chorus-server", args
assert "--control-listen" in args, args
print("image test: entrypoint and command:", " ".join(args))
EOF

echo "image test: $ (unpacked rootfs)/usr/local/bin/chorus-server --help"
"$BIN" --help | tee "$T/help.txt" | head -n 5
grep -q '^usage: chorus-server' "$T/help.txt"

# The unpacked server, started the way the image's command starts it but on
# loopback, and without the real-time grants no test machine gives.
read -r AUDIO CONTROL < <(python3 -c '
import socket
s = [socket.socket() for _ in range(2)]
for x in s: x.bind(("127.0.0.1", 0))
print(*[x.getsockname()[1] for x in s])')
"$BIN" --listen "127.0.0.1:$AUDIO" --control-listen "127.0.0.1:$CONTROL" \
    --state-file "$T/zones.state" --zone kitchen --source tone --serve-forever \
    --allow-non-realtime --no-lock-memory > "$T/server.log" 2>&1 &
PID=$!
trap 'kill "$PID" 2>/dev/null || true' EXIT
STATE=""
for _ in $(seq 1 50); do
    if STATE="$(curl -fsS "http://127.0.0.1:$CONTROL/api/state" 2>/dev/null)"; then
        break
    fi
    sleep 0.1
done
if ! printf '%s' "$STATE" | grep -q kitchen; then
    echo "image test: FAIL: GET /api/state did not name the declared zone"
    cat "$T/server.log"
    exit 1
fi
echo "image test: GET /api/state -> $(printf '%s' "$STATE" | head -c 160)"
echo "image test: PASS"
