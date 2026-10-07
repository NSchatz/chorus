#!/usr/bin/env bash
# `make soloist-sources`: the Debian source packages of the chorus-soloist image, fetched,
# verified and put in one tar the release attaches (ADR 0241; docs/release.md).
#
# For each line of deploy/soloist/debian-sources.pins: the .dsc is fetched from
# snapshot.debian.org at the pinned timestamp (or read from the cache) and held to its pinned
# sha256 and size; then every file its Checksums-Sha256 field lists is fetched from the same
# directory and held to the sha256 and size the .dsc gives. A mismatch, or a file the archive
# does not serve, fails the run. The .dsc's checksum field is the only thing read: no source
# file is unpacked, opened or read (clean-room, BRIEF.md 3.1). Fetch, hash, attach.
#
# Output: CHORUS_SOLOIST_SOURCES_OUT (default target/image/chorus-soloist-debian-sources.tar),
#         an uncompressed tar of the verified files, flat, with a SHA256SUMS of them; its
#         owners and times are fixed, so the same pins give the same bytes.
# Cache:  CHORUS_SOLOIST_IMAGE_CACHE (default /cache/chorus-soloist-image, else
#         target/image/soloist-cache), under sources/, each file named by its sha256, as the
#         image keeps its .deb files. Cold, the run fetches 201 files, about 246 MB, for the
#         20260918T000000Z pins; warm, it needs no network.
# Pins:   CHORUS_SOLOIST_SOURCES_PINS (default deploy/soloist/debian-sources.pins).
#
# Tools: curl, sha256sum, awk, GNU tar.

set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$(pwd)"

PINS="${CHORUS_SOLOIST_SOURCES_PINS:-deploy/soloist/debian-sources.pins}"
TD="${CARGO_TARGET_DIR:-$ROOT/target}"
OUT="${CHORUS_SOLOIST_SOURCES_OUT:-$TD/image/chorus-soloist-debian-sources.tar}"
CACHE="${CHORUS_SOLOIST_IMAGE_CACHE:-/cache/chorus-soloist-image}"
mkdir -p "$CACHE/sources" 2>/dev/null || { CACHE="$TD/image/soloist-cache"; mkdir -p "$CACHE/sources"; }

SNAPSHOT="$(sed -n 's/^# snapshot = //p' "$PINS")"
ARCHIVE="$(sed -n 's/^# archive = //p' "$PINS")"
[ -n "$SNAPSHOT" ] && [ -n "$ARCHIVE" ] || { echo "soloist-sources: REFUSED: $PINS names no snapshot or archive"; exit 2; }

FETCHED=0
# fetch <url> <sha256> <size> <what>: the cached file of that sha256, fetched first if absent,
# held to the sha256 and size. A fetched file enters the cache only once it matches.
fetch() {
    local url="$1" sha="$2" size="$3" what="$4" f="$CACHE/sources/$2" got from
    got="$f" from="the cache; remove $f"
    if [ ! -f "$f" ]; then
        got="$f.tmp.$$" from="$url"
        curl -fsSL --retry 3 -o "$got" "$url" ||
            { rm -f "$got"; echo "soloist-sources: FAIL: $what is not at $url"; exit 1; }
        FETCHED=$((FETCHED + 1))
    fi
    if [ "$(sha256sum "$got" | cut -d' ' -f1)" != "$sha" ] || [ "$(stat -c %s "$got")" != "$size" ]; then
        [ "$got" = "$f" ] || rm -f "$got"
        echo "soloist-sources: FAIL: $what does not match the sha256 $sha or size $size (read from $from)"
        exit 1
    fi
    [ "$got" = "$f" ] || mv "$got" "$f"
}

STAGE="$CACHE/stage.$$"
rm -rf "$STAGE"
mkdir -p "$STAGE" "$(dirname "$OUT")"
trap 'rm -rf "$STAGE"' EXIT
# place <sha256> <name>: the verified file in the flat tree, once; a name two packages give
# different bytes fails.
place() {
    if [ -e "$STAGE/$2" ]; then
        [ "$(sha256sum "$STAGE/$2" | cut -d' ' -f1)" = "$1" ] ||
            { echo "soloist-sources: FAIL: two source packages list $2 with different sha256"; exit 1; }
        return 0
    fi
    ln "$CACHE/sources/$1" "$STAGE/$2" 2>/dev/null || cp "$CACHE/sources/$1" "$STAGE/$2"
}

PACKAGES=0
FILES=0
BYTES=0
while read -r src ver dsc_sha dsc_size path rest; do
    [ -n "$path" ] && [ -z "$rest" ] || { echo "soloist-sources: REFUSED: a malformed line in $PINS: $src $ver"; exit 2; }
    PACKAGES=$((PACKAGES + 1))
    dir="${path%/*}"
    fetch "$ARCHIVE$path" "$dsc_sha" "$dsc_size" "the .dsc of $src $ver"
    dsc_name="$(printf '%s' "${path##*/}" | sed 's/%2b/+/g; s/%7e/~/g')"
    place "$dsc_sha" "$dsc_name"
    FILES=$((FILES + 1))
    BYTES=$((BYTES + dsc_size))
    # The .dsc's Checksums-Sha256 field: one "<sha256> <size> <name>" line per file.
    listed="$(awk '/^Checksums-Sha256:/ {f = 1; next} f && /^ / {print $1, $2, $3; next} {f = 0}' "$CACHE/sources/$dsc_sha")"
    [ -n "$listed" ] || { echo "soloist-sources: FAIL: the .dsc of $src $ver lists no file under Checksums-Sha256"; exit 1; }
    while read -r sha size name; do
        [[ "$sha" =~ ^[0-9a-f]{64}$ && "$size" =~ ^[0-9]+$ && "$name" =~ ^[A-Za-z0-9][A-Za-z0-9.+~_-]*$ ]] ||
            { echo "soloist-sources: FAIL: the .dsc of $src $ver lists a file it does not name with a sha256 and size: $name"; exit 1; }
        fetch "$ARCHIVE$dir/$(printf '%s' "$name" | sed 's/+/%2b/g; s/~/%7e/g')" "$sha" "$size" "$name of $src $ver"
        place "$sha" "$name"
        FILES=$((FILES + 1))
        BYTES=$((BYTES + size))
    done <<< "$listed"
done < <(command grep -v -E '^(#|[[:space:]]*$)' "$PINS")
[ "$PACKAGES" -gt 0 ] || { echo "soloist-sources: REFUSED: $PINS lists no source package"; exit 2; }
echo "soloist-sources: $PACKAGES source packages, $FILES files ($BYTES bytes) verified by sha256 ($FETCHED fetched from the snapshot $SNAPSHOT, the rest from $CACHE)"

(cd "$STAGE" && LC_ALL=C command ls | xargs sha256sum > "$CACHE/SHA256SUMS.$$" && mv "$CACHE/SHA256SUMS.$$" SHA256SUMS)
EPOCH="$(date -u -d "${SNAPSHOT:0:8} ${SNAPSHOT:9:2}:${SNAPSHOT:11:2}:${SNAPSHOT:13:2}" +%s)"
(cd "$STAGE" && LC_ALL=C command ls) | tar -C "$STAGE" --sort=name --mtime="@$EPOCH" --owner=0 --group=0 \
    --numeric-owner --mode=0644 --hard-dereference -cf "$OUT.tmp.$$" -T -
mv "$OUT.tmp.$$" "$OUT"
echo "soloist-sources: $OUT, $(du -k "$OUT" | cut -f1) KiB, $(tar -tf "$OUT" | wc -l) files, sha256 $(sha256sum "$OUT" | cut -d' ' -f1)"
