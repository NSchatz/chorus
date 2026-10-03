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
# and its musl target from rust-toolchain.toml; and zig (the `core:zig` pin of
# mise.toml) as the C compiler for that target.
#
# The server links C since goal 16: libopus, for Ogg Opus (crates/decode,
# crates/opus-sys). zig compiles those units against musl's headers through
# tools/zig-musl-cc.sh; the link is still rustc's own over its self-contained
# musl, so the binary stays what it was, a static position-independent
# executable, which the test below holds (docs/decisions/0122-the-server-decoders.md).
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
pin() { sed -n "s|^\"$1\" = \"\(.*\)\"$|\1|p" mise.toml; }
ZIG_VERSION="$(pin core:zig)"
[ -n "$ZIG_VERSION" ] || { echo "image: mise.toml pins no core:zig"; exit 2; }

TD="${CARGO_TARGET_DIR:-$ROOT/target}"
OUT="${CHORUS_IMAGE_OUT:-$TD/image/chorus-server-oci.tar}"
# What the build itself writes (the musl target directory, zig's cache, the staged tree, the
# Dockerfile's build context and its check) is read by nothing afterwards: it goes where
# tools/build-dir.sh says (the tmpfs when it fits, else the lane's shared directory), never
# into a directory of this worktree's own. The tarball and the pulled base stay under $TD.
# shellcheck source=tools/build-dir.sh
. "$ROOT/tools/build-dir.sh"
BD="$(throwaway_build_dir image)"
mkdir -p "$BD"
echo "image: build directory $BD"
WORK="$BD/image/work"
BASE_CACHE="${CHORUS_IMAGE_BASE_CACHE:-$TD/image/base-${BASE_DIGEST#sha256:}}"

# Every timestamp in the image is the commit's, so two builds of one commit
# differ only where umoci itself is not reproducible.
EPOCH="$(git log -1 --format=%ct)"
CREATED="$(date -u -d "@$EPOCH" +%Y-%m-%dT%H:%M:%SZ)"
REVISION="$(git rev-parse HEAD)"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)"

# The C compiler of the musl target (the `cc` crate reads this variable), and
# what makes its output the same from one build to the next.
export CHORUS_ZIG_VERSION="$ZIG_VERSION"
export CC_x86_64_unknown_linux_musl="$ROOT/tools/zig-musl-cc.sh"
export SOURCE_DATE_EPOCH="$EPOCH"
export ZIG_GLOBAL_CACHE_DIR="${ZIG_GLOBAL_CACHE_DIR:-$BD/zig-cache}"

echo "image: building chorus-server for $TARGET (C by zig $ZIG_VERSION)"
CARGO_TARGET_DIR="$BD" cargo build --release --locked --target "$TARGET" -p chorus-server --bins
# The wakeup-jitter probe rides along for the owner's production run inside the
# deployed container (docs/measurements/host-wakeup-jitter.md, "Next"); it never
# runs unless invoked.
CARGO_TARGET_DIR="$BD" cargo build --release --locked --target "$TARGET" -p chorus-hostprobe --bin chorus-wakeup-probe

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
install -m 0755 "$BD/$TARGET/release/chorus-server" "$STAGE/usr/local/bin/chorus-server"
install -m 0755 "$BD/$TARGET/release/chorus-rt-spin" "$STAGE/usr/local/bin/chorus-rt-spin"
install -m 0755 "$BD/$TARGET/release/chorus-wakeup-probe" "$STAGE/usr/local/bin/chorus-wakeup-probe"
install -m 0644 config/verification.conf "$STAGE/etc/chorus/verification.conf"
# The notices the binary's third-party code asks for (libopus: BSD-3-Clause,
# reproduced; Symphonia: MPL-2.0, where its source is), with the exact crates
# and versions this build links.
MPL_CRATES="$(cargo tree --locked -e normal -p chorus-server --prefix none --format '{p}|{l}' --target "$TARGET" |
    awk -F'|' '$2 ~ /MPL-2.0/ {split($1, a, " "); print a[1] " " substr(a[2], 2)}' | LC_ALL=C sort -u)"
[ -n "$MPL_CRATES" ] || { echo "image: no MPL-2.0 crate found, but chorus-server links Symphonia"; exit 2; }
mkdir -p "$STAGE/usr/share/doc/chorus"
install -m 0644 third_party/opus/COPYING "$STAGE/usr/share/doc/chorus/libopus-COPYING"
{
    cat deploy/THIRD-PARTY-NOTICES.md
    while read -r name ver; do
        echo "- \`$name\` $ver: https://crates.io/crates/$name/$ver"
    done <<< "$MPL_CRATES"
} > "$STAGE/usr/share/doc/chorus/THIRD-PARTY-NOTICES.md"
chmod 0644 "$STAGE/usr/share/doc/chorus/THIRD-PARTY-NOTICES.md"
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
(cd "$CTX" && CARGO_TARGET_DIR="$BD/docker-context-target" cargo check --locked --workspace --bins --quiet)
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

# The binary is what it was before it linked C: static, position-independent,
# asking the system for no loader and no library.
ELF_TYPE="$(readelf -h "$BIN" | sed -n 's/^ *Type: *//p')"
case "$ELF_TYPE" in
    DYN*) ;;
    *) echo "image test: FAIL: chorus-server is '$ELF_TYPE', not a position-independent executable"; exit 1 ;;
esac
if readelf -d "$BIN" | grep -q NEEDED; then
    echo "image test: FAIL: chorus-server needs a shared library"; exit 1
fi
if readelf -lW "$BIN" | grep -q INTERP; then
    echo "image test: FAIL: chorus-server asks for a program interpreter"; exit 1
fi
STRIPPED="$T/chorus-server.stripped"
strip -o "$STRIPPED" "$BIN"
echo "image test: chorus-server is a static PIE (no NEEDED, no INTERP): $(stat -c %s "$BIN") bytes, $(stat -c %s "$STRIPPED") stripped"

# The notices are in the image, and name every MPL-2.0 crate the binary links.
DOC="$T/bundle/rootfs/usr/share/doc/chorus"
cmp -s third_party/opus/COPYING "$DOC/libopus-COPYING" ||
    { echo "image test: FAIL: the image does not carry libopus's COPYING"; exit 1; }
while read -r name ver; do
    grep -qF "https://crates.io/crates/$name/$ver" "$DOC/THIRD-PARTY-NOTICES.md" ||
        { echo "image test: FAIL: the notices do not say where $name $ver's source is"; exit 1; }
done <<< "$MPL_CRATES"
echo "image test: notices: libopus-COPYING and THIRD-PARTY-NOTICES.md ($(printf '%s\n' "$MPL_CRATES" | wc -l) MPL-2.0 crates named)"

echo "image test: $ (unpacked rootfs)/usr/local/bin/chorus-server --help"
"$BIN" --help | tee "$T/help.txt" | head -n 5
grep -q '^usage: chorus-server' "$T/help.txt"

# The unpacked server, started the way the image's command starts it but on
# loopback, and without the real-time grants no test machine gives. The ports
# are picked free and then released, so another process can take one first;
# a start that loses that race is retried on new ports, three times at most.
free_ports() {
    python3 -c '
import socket
s = [socket.socket() for _ in range(2)]
for x in s: x.bind(("127.0.0.1", 0))
print(*[x.getsockname()[1] for x in s])'
}
STATE=""
PID=""
trap 'if [ -n "$PID" ]; then kill "$PID" 2>/dev/null || true; fi' EXIT
for attempt in 1 2 3; do
    read -r AUDIO CONTROL < <(free_ports)
    "$BIN" --listen "127.0.0.1:$AUDIO" --control-listen "127.0.0.1:$CONTROL" \
        --state-file "$T/zones.state" --zone kitchen --source tone --serve-forever \
        --allow-non-realtime --no-lock-memory > "$T/server.log" 2>&1 &
    PID=$!
    for _ in $(seq 1 50); do
        if STATE="$(curl -fsS "http://127.0.0.1:$CONTROL/api/state" 2>/dev/null)"; then
            break
        fi
        kill -0 "$PID" 2>/dev/null || break
        sleep 0.1
    done
    if printf '%s' "$STATE" | grep -q kitchen; then
        break
    fi
    kill "$PID" 2>/dev/null || true
    wait "$PID" 2>/dev/null || true
    if ! grep -q 'Address in use' "$T/server.log"; then
        break
    fi
    echo "image test: attempt $attempt lost a port race, retrying on new ports"
done
if ! printf '%s' "$STATE" | grep -q kitchen; then
    echo "image test: FAIL: GET /api/state did not name the declared zone"
    cat "$T/server.log"
    exit 1
fi
echo "image test: GET /api/state -> $(printf '%s' "$STATE" | head -c 160)"
# The healthcheck a compose file runs (the image has no shell): healthy against
# the running server, unhealthy against a port nothing listens on.
"$BIN" --health-check "127.0.0.1:$CONTROL"
read -r DEAD _ < <(free_ports)
if "$BIN" --health-check "127.0.0.1:$DEAD"; then
    echo "image test: FAIL: --health-check reported a dead port healthy"
    exit 1
fi
# The decoders in the image's own binary run (goal 16): one fixture of every
# settled format, staged beside the test and not in the image, decoded by the
# unpacked server. libopus here is the zig-compiled build, not the host build
# the crate's own tests run. Lossless formats and Opus (integer arithmetic)
# must give the fixture's hash; MP3 and Vorbis (floating point) its frame count.
M="$T/media"
mkdir -p "$M"
field() { sed -n "s/^$2 = //p" "fixtures/decode/$1.fields"; }
for name in wav-tone44-s16 flac-sweep48-s24 alac-tone44-s16-moov-first alac-sweep48-s24-moov-last \
    mp3-tone44 vorbis-tone44 opus-tone44 opus-sweep48; do
    file="$(field "$name" file)"
    cp "fixtures/decode/$file" "$M/$file"
    line="$("$BIN" --probe-media "$M/$file")"
    want="codec=$(field "$name" codec) rate=$(field "$name" sample_rate_hz) channels=$(field "$name" channels)"
    hash="$(field "$name" decode_f32_fnv1a64)"
    case "$line" in
        *"probe-media: $want bits="*" frames=$(field "$name" frames) "*) ;;
        *) echo "image test: FAIL: --probe-media $file: $line (wanted $want, $(field "$name" frames) frames)"; exit 1 ;;
    esac
    if [ -n "$hash" ]; then
        case "$line" in
            *" pcm_f32_fnv1a64=$hash") ;;
            *) echo "image test: FAIL: --probe-media $file: $line (wanted the exact decode $hash)"; exit 1 ;;
        esac
    fi
    echo "image test: --probe-media $file -> ${line#chorus-server: probe-media: }"
done
for name in aac-in-mp4 aac-adts; do
    file="$(field "$name" file)"
    cp "fixtures/decode/$file" "$M/$file"
    if "$BIN" --probe-media "$M/$file" > "$T/aac.out" 2> "$T/aac.err"; then
        echo "image test: FAIL: --probe-media decoded $file, which is AAC"; exit 1
    fi
    grep -qx "chorus-server: probe-media refused: $(field "$name" refusal)" "$T/aac.err" ||
        { echo "image test: FAIL: $file was refused, but not by name: $(cat "$T/aac.err")"; exit 1; }
    echo "image test: --probe-media $file -> $(cat "$T/aac.err")"
done

# The UPnP AV media renderers in the image's own binary (goal 16, docs/upnp.md):
# a second server with --upnp, everything on loopback (its discovery socket on
# an ephemeral port and its notifications to a loopback port, so no multicast
# and no port 1900), and the kitchen's device description read over HTTP.
UPNP_DIR="$T/upnp"
mkdir -p "$UPNP_DIR"
read -r UPNP_AUDIO UPNP_NOTIFY < <(free_ports)
"$BIN" --listen "127.0.0.1:$UPNP_AUDIO" --control-listen 127.0.0.1:0 \
    --state-file "$UPNP_DIR/zones.state" --zone kitchen --source tone --serve-forever \
    --slots 1 --players 1 --upnp --upnp-listen 127.0.0.1:0 --upnp-ssdp-port 0 \
    --upnp-ssdp-group "127.0.0.1:$UPNP_NOTIFY" \
    --allow-non-realtime --no-lock-memory > "$UPNP_DIR/server.log" 2>&1 &
UPNP_PID=$!
stop_servers() {
    if [ -n "$PID" ]; then kill "$PID" 2>/dev/null || true; fi
    if [ -n "$UPNP_PID" ]; then kill "$UPNP_PID" 2>/dev/null || true; fi
}
trap stop_servers EXIT
UPNP_HTTP=""
UPNP_UDN=""
for _ in $(seq 1 100); do
    UPNP_HTTP="$(sed -n 's/.*upnp renderers listening on=\([^ ]*\).*/\1/p' "$UPNP_DIR/server.log" | head -n 1)"
    UPNP_UDN="$(sed -n 's/.*upnp renderer appeared target=room:kitchen udn=\([^ ]*\).*/\1/p' "$UPNP_DIR/server.log" | head -n 1)"
    if [ -n "$UPNP_HTTP" ] && [ -n "$UPNP_UDN" ]; then
        break
    fi
    kill -0 "$UPNP_PID" 2>/dev/null || break
    sleep 0.1
done
if [ -z "$UPNP_HTTP" ] || [ -z "$UPNP_UDN" ]; then
    echo "image test: FAIL: the server with --upnp did not announce a renderer for the kitchen"
    cat "$UPNP_DIR/server.log"
    exit 1
fi
DESC="$(curl -fsS "http://$UPNP_HTTP/upnp/$UPNP_UDN/desc.xml")"
printf '%s' "$DESC" | grep -q 'urn:schemas-upnp-org:device:MediaRenderer:1' ||
    { echo "image test: FAIL: the description is not a MediaRenderer:1: $DESC"; exit 1; }
printf '%s' "$DESC" | grep -q "<UDN>uuid:$UPNP_UDN</UDN>" ||
    { echo "image test: FAIL: the description does not carry the UDN $UPNP_UDN"; exit 1; }
echo "image test: --upnp: GET http://$UPNP_HTTP/upnp/$UPNP_UDN/desc.xml -> MediaRenderer:1, $(printf '%s' "$DESC" | sed -n 's/.*<friendlyName>\(.*\)<\/friendlyName>.*/friendlyName \1/p')"
kill "$UPNP_PID" 2>/dev/null || true
wait "$UPNP_PID" 2>/dev/null || true
UPNP_PID=""

# The probe the owner runs in the deployed container: present, static, and it runs.
PROBE="$T/bundle/rootfs/usr/local/bin/chorus-wakeup-probe"
"$PROBE" --period-us 5000 --seconds 1 > "$T/probe.txt"
grep -q '^count  *200$' "$T/probe.txt"
echo "image test: chorus-wakeup-probe ran: $(grep '^lateness_us' "$T/probe.txt")"
echo "image test: PASS"
