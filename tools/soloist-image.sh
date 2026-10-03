#!/usr/bin/env bash
# `make soloist-image`: the chorus-soloist OCI image (one Spotify Soloist receiver:
# PipeWire, WirePlumber and chorus-soloistd), built with no container daemon, and
# its test. docs/decisions/0000-the-chorus-soloist-image.md says why this method.
#
# What goes in, on the digest-pinned debian:trixie-slim base:
#   1. the Debian packages of deploy/soloist/debian-packages.pins, each fetched
#      from snapshot.debian.org at the pinned timestamp (or read from the cache),
#      held to its sha256, and unpacked with `dpkg-deb -x`. No maintainer script
#      runs: nothing here needs a systemd unit, a user or an alternative, and the
#      one thing a script would have done that matters, the loader cache, is made
#      below by the image's own ldconfig;
#   2. what chorus adds: a static chorus-soloistd, /etc/passwd and /etc/group with
#      the user 65532, /etc/ld.so.cache, the notices, and the mount points.
# NO Soloist file goes in: the binary is the owner's, mounted read-only at
# /opt/soloist (`make soloist-lists` holds the image to that).
#
# The test unpacks the tarball (umoci, rootless) and runs the image's own
# programs with the image's own loader and libraries: chorus-soloistd supervises
# the image's PipeWire and WirePlumber, `pw-cat` from the image plays a known
# signal into the receiver's sink and the FIFO must deliver it sample for
# sample, and the fake Soloist (built outside the image, never in it) stands in
# for the owner's binary so the supervisor's socket, `assign` and event relay
# are exercised. No user namespace or chroot exists in the gate, so the image's
# files are reached by path and the PipeWire directories are named by
# environment; in a container they are the compiled-in defaults.
#
# Nothing is pushed anywhere: pushing to a registry is the owner's step.
#
# Tools: crane 0.22.1 and umoci 0.6.0 through mise (as tools/image.sh),
# dpkg-deb, curl, readelf, python3, and the Rust toolchain's musl target.
#
# Output: CHORUS_SOLOIST_IMAGE_OUT (default target/image/chorus-soloist-oci.tar).
# Cache:  CHORUS_SOLOIST_IMAGE_CACHE (default /cache/chorus-soloist-image, else
#         target/image/soloist-cache): the base layout and the .deb files. Warm,
#         the build needs no network; cold, it pulls the base from the registry
#         and 60 packages (18 MB) from snapshot.debian.org. Either way every
#         package is checked against its pinned sha256 and every base blob
#         against its digest.

set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$(pwd)"
T0=$(date +%s.%N)

# docker.io/library/debian:trixie-20260918-slim, the multi-platform index digest,
# resolved with `crane digest` on 2026-10-03 (the rolling tag trixie-slim gave the
# same digest that day). Its date is the snapshot timestamp of the package pins.
BASE_REF="docker.io/library/debian"
BASE_TAG="trixie-20260918-slim"
BASE_DIGEST="sha256:a99cfc517144bc59b1978475ec53b46ecabec7e43635402ee5b77cc54cd1b20a"
CRANE=(mise exec aqua:google/go-containerregistry@0.22.1 -- crane)
UMOCI=(mise exec aqua:opencontainers/umoci@0.6.0 -- umoci)
TARGET=x86_64-unknown-linux-musl
PINS=deploy/soloist/debian-packages.pins
# The user the image runs as: chorus-server's, so both can use the receiver directory.
RUN_UID=65532

TD="${CARGO_TARGET_DIR:-$ROOT/target}"
OUT="${CHORUS_SOLOIST_IMAGE_OUT:-$TD/image/chorus-soloist-oci.tar}"
WORK="$TD/image/soloist-work"
CACHE="${CHORUS_SOLOIST_IMAGE_CACHE:-/cache/chorus-soloist-image}"
mkdir -p "$CACHE/debs" 2>/dev/null || { CACHE="$TD/image/soloist-cache"; mkdir -p "$CACHE/debs"; }
BASE_CACHE="$CACHE/base-${BASE_DIGEST#sha256:}"

command -v dpkg-deb > /dev/null || { echo "soloist-image: REFUSED: dpkg-deb is not on PATH (it unpacks the pinned packages)"; exit 2; }

SNAPSHOT="$(sed -n 's/^# snapshot = //p' "$PINS")"
ARCHIVE="$(sed -n 's/^# archive = //p' "$PINS")"
[ -n "$SNAPSHOT" ] && [ -n "$ARCHIVE" ] || { echo "soloist-image: REFUSED: $PINS names no snapshot or archive"; exit 2; }
pins() { command grep -v -E '^(#|[[:space:]]*$)' "$PINS"; }

EPOCH="$(git log -1 --format=%ct)"
CREATED="$(date -u -d "@$EPOCH" +%Y-%m-%dT%H:%M:%SZ)"
REVISION="$(git rev-parse HEAD)"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)"
export SOURCE_DATE_EPOCH="$EPOCH"

# chorus-soloistd is static (musl), like chorus-server: it links no C, so the
# target's own self-contained linker is the whole toolchain.
echo "soloist-image: building chorus-soloistd for $TARGET"
cargo build --release --locked --target "$TARGET" -p chorus-soloistd --bins

# --- the pinned inputs ---------------------------------------------------------

FETCHED=0
if [ ! -f "$BASE_CACHE/index.json" ]; then
    echo "soloist-image: pulling $BASE_REF:$BASE_TAG@$BASE_DIGEST"
    rm -rf "$BASE_CACHE.tmp.$$"
    "${CRANE[@]}" pull --format=oci --platform linux/amd64 "$BASE_REF@$BASE_DIGEST" "$BASE_CACHE.tmp.$$"
    mv "$BASE_CACHE.tmp.$$" "$BASE_CACHE" 2>/dev/null || rm -rf "$BASE_CACHE.tmp.$$"
    FETCHED=1
fi
for blob in "$BASE_CACHE"/blobs/sha256/*; do
    [ "$(sha256sum "$blob" | cut -d' ' -f1)" = "${blob##*/}" ] ||
        { echo "soloist-image: FAIL: the cached base blob ${blob##*/} does not match its digest; remove $BASE_CACHE"; exit 1; }
done

PACKAGES=0
while read -r pkg ver _ sha size _ _ path; do
    PACKAGES=$((PACKAGES + 1))
    deb="$CACHE/debs/$sha.deb"
    if [ ! -f "$deb" ]; then
        echo "soloist-image: fetching $pkg $ver from the snapshot $SNAPSHOT"
        curl -fsSL --retry 3 -o "$deb.tmp.$$" "$ARCHIVE$path"
        mv "$deb.tmp.$$" "$deb"
        FETCHED=$((FETCHED + 1))
    fi
    [ "$(sha256sum "$deb" | cut -d' ' -f1)" = "$sha" ] && [ "$(stat -c %s "$deb")" = "$size" ] ||
        { echo "soloist-image: FAIL: $pkg $ver does not match its pinned sha256 $sha or size $size; remove $deb"; exit 1; }
done < <(pins)
echo "soloist-image: base $BASE_TAG and $PACKAGES packages verified by sha256 ($FETCHED fetched, the rest from $CACHE)"

# --- the staged trees ----------------------------------------------------------

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
L="$WORK/layout"
"${UMOCI[@]}" unpack --rootless --image "$L:base" "$WORK/base" > /dev/null
BASEFS="$WORK/base/rootfs"

# 1. The Debian packages, exactly as Debian built them.
DEB="$WORK/stage-debian"
mkdir -p "$DEB/var/lib/dpkg/status.d"
while read -r pkg _ _ sha _ _ _ _; do
    dpkg-deb -x "$CACHE/debs/$sha.deb" "$DEB"
    # What is installed, for a scanner: one control file per package, the form
    # images assembled without dpkg use. dpkg's own database is left as the
    # base's, because dpkg installed none of these.
    dpkg-deb -f "$CACHE/debs/$sha.deb" > "$DEB/var/lib/dpkg/status.d/$pkg"
done < <(pins)
# The base is merged-/usr: /lib, /bin and /sbin are links into /usr, and a
# directory of the same name in a layer would replace the link.
for top in "$DEB"/*; do
    case "${top##*/}" in
        usr | etc | var) ;;
        *) echo "soloist-image: FAIL: a package ships /${top##*/}, outside /usr, /etc and /var"; exit 1 ;;
    esac
done

# 2. What chorus adds.
ADD="$WORK/stage-chorus"
mkdir -p "$ADD/usr/local/bin" "$ADD/etc" "$ADD/usr/share/doc/chorus"
install -m 0755 "$TD/$TARGET/release/chorus-soloistd" "$ADD/usr/local/bin/chorus-soloistd"
{ cat "$BASEFS/etc/passwd"; echo "chorus:x:$RUN_UID:$RUN_UID:chorus receiver:/nonexistent:/usr/sbin/nologin"; } > "$ADD/etc/passwd"
{ cat "$BASEFS/etc/group"; echo "chorus:x:$RUN_UID:"; } > "$ADD/etc/group"
chmod 0644 "$ADD/etc/passwd" "$ADD/etc/group"
# The loader cache, which each library package's trigger would have refreshed:
# the image's own ldconfig over the base plus the packages. It is static, and
# with -r it prefixes paths itself when it may not chroot.
MERGED="$WORK/merged"
cp -a "$BASEFS" "$MERGED"
cp -a "$DEB/." "$MERGED/"
"$MERGED/usr/sbin/ldconfig" -r "$MERGED"
install -m 0644 "$MERGED/etc/ld.so.cache" "$ADD/etc/ld.so.cache"
# The notices: each package's own copyright file came with the package
# (/usr/share/doc/<package>/copyright); this file says what is here and where
# the source of every package is.
{
    cat deploy/soloist/THIRD-PARTY-NOTICES.md
    echo
    echo "| Package | Version | Source package | Source |"
    echo "|---|---|---|---|"
    while read -r pkg ver _ _ _ src srcver path; do
        echo "| \`$pkg\` | $ver | \`$src\` $srcver | <$ARCHIVE${path%/*}/> |"
    done < <(pins)
} > "$ADD/usr/share/doc/chorus/THIRD-PARTY-NOTICES.md"
chmod 0644 "$ADD/usr/share/doc/chorus/THIRD-PARTY-NOTICES.md"
# The mount points (deploy/soloist/compose.yaml). A rootless insert records every
# file as root's and the image runs as 65532, so the three a run may leave
# unmounted are world-writable with the sticky bit, as /tmp is. /opt/soloist is
# the owner's read-only mount and /run/chorus-soloist a tmpfs owned by 65532.
mkdir -p "$ADD/run/chorus/soloist" "$ADD/var/lib/chorus-soloist" "$ADD/var/cache/chorus-soloist" \
    "$ADD/opt/soloist" "$ADD/run/chorus-soloist"
chmod 1777 "$ADD/run/chorus/soloist" "$ADD/var/lib/chorus-soloist" "$ADD/var/cache/chorus-soloist"
find "$DEB" "$ADD" -exec touch -h -d "@$EPOCH" {} +

"${UMOCI[@]}" insert --rootless --image "$L:base" --tag chorus-soloist \
    --history.created "$CREATED" --history.created_by "tools/soloist-image.sh: $PACKAGES Debian packages (dpkg-deb -x)" "$DEB" /
"${UMOCI[@]}" insert --rootless --image "$L:chorus-soloist" \
    --history.created "$CREATED" --history.created_by "tools/soloist-image.sh: chorus-soloistd" "$ADD" /
"${UMOCI[@]}" config --image "$L:chorus-soloist" --created "$CREATED" \
    --history.created "$CREATED" --history.created_by "tools/soloist-image.sh" \
    --config.entrypoint /usr/local/bin/chorus-soloistd \
    --clear config.cmd \
    --config.cmd --soloist-dir --config.cmd /run/chorus/soloist \
    --config.cmd --api-key-file --config.cmd /run/secrets/soloist_api_key \
    --config.cmd --state-dir --config.cmd /var/lib/chorus-soloist \
    --config.cmd --cache-dir --config.cmd /var/cache/chorus-soloist \
    --config.volume /run/chorus/soloist --config.volume /var/lib/chorus-soloist \
    --config.volume /var/cache/chorus-soloist \
    --config.user "$RUN_UID:$RUN_UID" \
    --config.label "org.opencontainers.image.title=chorus-soloist" \
    --config.label "org.opencontainers.image.version=$VERSION" \
    --config.label "org.opencontainers.image.revision=$REVISION" \
    --config.label "org.opencontainers.image.base.name=$BASE_REF:$BASE_TAG@$BASE_DIGEST"
"${UMOCI[@]}" rm --image "$L:base"
"${UMOCI[@]}" gc --layout "$L"

(cd "$L" && tar --sort=name --mtime="@$EPOCH" --owner=0 --group=0 --numeric-owner \
    -cf "$OUT" oci-layout index.json blobs)
MANIFEST="$(python3 -c 'import json,sys; m=json.load(open(sys.argv[1]))["manifests"]; assert len(m)==1; print(m[0]["digest"])' "$L/index.json")"
echo "soloist-image: $OUT, $(du -k "$OUT" | cut -f1) KiB, sha256 $(sha256sum "$OUT" | cut -d' ' -f1)"
echo "soloist-image: manifest digest $MANIFEST"

# --- the test ------------------------------------------------------------------

T="$WORK/test"
mkdir -p "$T/layout"
tar -xf "$OUT" -C "$T/layout"
"${UMOCI[@]}" unpack --rootless --image "$T/layout:chorus-soloist" "$T/bundle" > /dev/null
R="$T/bundle/rootfs"
BIN="$R/usr/local/bin/chorus-soloistd"
python3 - "$T/bundle/config.json" "$RUN_UID" <<'EOF'
import json, sys
c = json.load(open(sys.argv[1]))
args, user = c["process"]["args"], c["process"]["user"]
assert args[0] == "/usr/local/bin/chorus-soloistd", args
assert args[1:3] == ["--soloist-dir", "/run/chorus/soloist"], args
assert user["uid"] == int(sys.argv[2]) and user["gid"] == int(sys.argv[2]), user
print("soloist-image test: user %d, entrypoint and command: %s" % (user["uid"], " ".join(args)))
EOF

# chorus-soloistd asks the image for no loader and no library.
case "$(readelf -h "$BIN" | sed -n 's/^ *Type: *//p')" in
    DYN*) ;;
    *) echo "soloist-image test: FAIL: chorus-soloistd is not a position-independent executable"; exit 1 ;;
esac
if readelf -d "$BIN" | grep -q NEEDED || readelf -lW "$BIN" | grep -q INTERP; then
    echo "soloist-image test: FAIL: chorus-soloistd needs a shared library or a program interpreter"; exit 1
fi
"$BIN" --help > "$T/help.txt"
grep -q '^usage: chorus-soloistd' "$T/help.txt" && grep -q -- '--health-check' "$T/help.txt"
echo "soloist-image test: chorus-soloistd is a static PIE, $(stat -c %s "$BIN") bytes, and runs: $("$BIN" --version)"

# The user exists by name, and no Soloist file is here (the mount point is empty).
grep -q "^chorus:x:$RUN_UID:$RUN_UID:" "$R/etc/passwd" && grep -q "^chorus:x:$RUN_UID:" "$R/etc/group"
[ -d "$R/opt/soloist" ] && [ -z "$(ls -A "$R/opt/soloist")" ] ||
    { echo "soloist-image test: FAIL: /opt/soloist is not an empty mount point"; exit 1; }

# The notices: every package's copyright file, and its source named.
while read -r pkg ver _ _ _ src srcver _; do
    [ -s "$R/usr/share/doc/$pkg/copyright" ] ||
        { echo "soloist-image test: FAIL: /usr/share/doc/$pkg/copyright is not in the image"; exit 1; }
    grep -qF "| \`$pkg\` | $ver | \`$src\` $srcver |" "$R/usr/share/doc/chorus/THIRD-PARTY-NOTICES.md" ||
        { echo "soloist-image test: FAIL: the notices do not name $pkg $ver and its source"; exit 1; }
    cmp -s <(dpkg-deb -f "$CACHE/debs/$(pins | awk -v p="$pkg" '$1 == p {print $4}').deb") "$R/var/lib/dpkg/status.d/$pkg" ||
        { echo "soloist-image test: FAIL: /var/lib/dpkg/status.d/$pkg is not the package's control file"; exit 1; }
done < <(pins)
echo "soloist-image test: notices: $PACKAGES copyright files, each package and its source named, $PACKAGES status.d records"

# The loader finds every library, with no maintainer script having run: every
# NEEDED name of every program and library the packages brought is in the
# image's loader cache (read back by the image's ldconfig) or in the file's own
# RUNPATH. This is also what shows the excluded packages are not needed.
"$R/usr/sbin/ldconfig" -r "$R" -p > "$T/ldcache.txt"
(cd "$DEB" && find usr -type f) > "$T/package-files.txt"
python3 - "$R" "$T/ldcache.txt" "$T/package-files.txt" <<'EOF'
import os, re, subprocess, sys
root, cache, files = sys.argv[1:4]
known = {line.split()[0] for line in open(cache) if "=>" in line}
elves = unresolved = 0
for rel in open(files).read().split("\n"):
    path = os.path.join(root, rel)
    if not rel or os.path.islink(path):
        continue
    with open(path, "rb") as f:
        if f.read(4) != b"\x7fELF":
            continue
    dyn = subprocess.run(["readelf", "-d", path], capture_output=True, text=True).stdout
    needed = re.findall(r"\(NEEDED\).*\[(.*)\]", dyn)
    if not needed:
        continue
    elves += 1
    run = [p for m in re.findall(r"\((?:RUNPATH|RPATH)\).*\[(.*)\]", dyn) for p in m.split(":")]
    here = os.path.dirname("/" + rel)
    for name in needed:
        dirs = [p.replace("$ORIGIN", here) for p in run]
        if name in known or any(os.path.exists(root + os.path.join(d, name)) for d in dirs):
            continue
        unresolved += 1
        print("soloist-image test: FAIL: /%s needs %s, which the image's loader cannot find" % (rel, name))
print("soloist-image test: loader: %d programs and libraries of the packages, every NEEDED name in /etc/ld.so.cache (%d entries) or a RUNPATH" % (elves, len(known)))
sys.exit(1 if unresolved else 0)
EOF

# The image's programs, run by the image's loader over the image's libraries.
# Sockets have short paths (108 bytes), so the run's directories are under TMPDIR.
S="$(mktemp -d "${TMPDIR:-/tmp}/chorus-soloist-image.XXXXXX")"
SUP=""
cleanup() {
    if [ -n "$SUP" ]; then kill "$SUP" 2>/dev/null || true; fi
    rm -rf "$S"
}
trap cleanup EXIT
MULTI="$R/usr/lib/x86_64-linux-gnu"
mkdir -p "$S/bin" "$S/recv" "$S/state" "$S/cache"
for prog in pipewire wireplumber pw-cat; do
    printf '#!/bin/sh\nexec "%s" --library-path "%s" "%s" "$@"\n' \
        "$R/usr/lib64/ld-linux-x86-64.so.2" "$MULTI:$MULTI/pulseaudio" "$R/usr/bin/$prog" > "$S/bin/$prog"
    chmod 0755 "$S/bin/$prog"
done
export SPA_PLUGIN_DIR="$MULTI/spa-0.2" PIPEWIRE_MODULE_DIR="$MULTI/pipewire-0.3" \
    WIREPLUMBER_MODULE_DIR="$MULTI/wireplumber-0.5" WIREPLUMBER_DATA_DIR="$R/usr/share/wireplumber"
unset DBUS_SESSION_BUS_ADDRESS DISPLAY
PW_VERSION="$(pins | awk '$1 == "pipewire" {sub(/-.*/, "", $2); print $2}')"
WP_VERSION="$(pins | awk '$1 == "wireplumber" {sub(/-.*/, "", $2); print $2}')"
"$S/bin/pipewire" --version | grep -q "Linked with libpipewire $PW_VERSION" ||
    { echo "soloist-image test: FAIL: the image's pipewire is not $PW_VERSION"; exit 1; }
"$S/bin/wireplumber" --version | grep -q "libwireplumber $WP_VERSION" ||
    { echo "soloist-image test: FAIL: the image's wireplumber is not $WP_VERSION"; exit 1; }
echo "soloist-image test: the image's pipewire $PW_VERSION and wireplumber $WP_VERSION run on the image's loader and libraries"

# The fake Soloist: an example of crates/soloistd, built here and never staged.
cargo build --release --locked -p chorus-soloistd --example chorus-fake-soloist
FAKE="$TD/release/examples/chorus-fake-soloist"
printf 'not-a-real-key\n' > "$S/key"
FAKE_SOLOIST_CONTROL="$S/app.sock" FAKE_SOLOIST_VERSION="Soloist 0.0.0 (chorus fake), build 20260930" \
    "$BIN" --soloist-dir "$S/recv" --receivers 2 --api-key-file "$S/key" \
    --state-dir "$S/state" --cache-dir "$S/cache" --soloist-bin "$FAKE" \
    --pipewire auto --pipewire-bin "$S/bin/pipewire" --wireplumber-bin "$S/bin/wireplumber" \
    --pipewire-runtime-dir "$S/run" --wireplumber-config-dir "$R/usr/share/wireplumber" \
    > "$T/supervisor.log" 2>&1 &
SUP=$!
if ! python3 tools/soloist-image-test.py "$S" "$SUP" "$BIN" "$S/bin/pw-cat"; then
    echo "soloist-image test: FAIL; the supervisor's log:"
    tail -n 40 "$T/supervisor.log"
    exit 1
fi
# The supervisor's own exit: 0 on SIGTERM (docs/soloist.md).
SUP_RC=0
wait "$SUP" || SUP_RC=$?
SUP=""
[ "$SUP_RC" -eq 0 ] || { echo "soloist-image test: FAIL: chorus-soloistd exited $SUP_RC on SIGTERM, not 0"; exit 1; }
if grep -q 'not-a-real-key' "$T/supervisor.log"; then
    echo "soloist-image test: FAIL: the API key is in the supervisor's log"; exit 1
fi
echo "soloist-image test: PASS"
T1=$(date +%s.%N)
echo "soloist-image: wall-clock $(awk -v a="$T0" -v b="$T1" 'BEGIN{printf "%.1f", b-a}')s"
