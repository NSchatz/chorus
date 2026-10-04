#!/usr/bin/env bash
# The pinned install copy of the Home Assistant integration (docs/home-assistant.md,
# "Installing: the pinned copy"; docs/decisions, "the Home Assistant integration is installed
# from a pinned export").
#
#   tools/ha-export.sh <dir> [<commit>]   write <dir>/chorus and <dir>/chorus.lock
#   tools/ha-export.sh --verify <dir>     hold <dir>/chorus to <dir>/chorus.lock and to the
#                                         commit the lock names
#
# The export is integrations/homeassistant/custom_components/chorus as ONE COMMIT has it
# (default HEAD), read from git's objects and never from the working tree: an uncommitted
# edit, an untracked file or a `__pycache__` beside the sources cannot reach it. <dir>/chorus
# is the directory Home Assistant loads as `custom_components/chorus`; <dir>/chorus.lock is
# the manifest:
#
#   # commit = <the 40-hex commit>            the pin
#   # version = <manifest.json's version>
#   # homeassistant = <harness.pin's version> the Home Assistant the tests ran under
#   # files = <count>
#   <sha256>  chorus/<path>                   one line per file, sorted bytewise
#
# The hash lines are `sha256sum` format and the rest are comments to it, so a consumer without
# this repository checks its copy with `sha256sum --check --strict chorus.lock`. The output
# depends on the commit alone: no date, host, user or locale is written, every file is 0644
# and every directory 0755, so two exports of one commit are the same bytes
# (tools/conventions/check-ha-export.sh holds that).
#
# An export over an earlier one replaces it whole (a file the new commit dropped goes too). A
# <dir>/chorus with no chorus.lock of this tool's beside it is not ours and is refused.
#
# --verify fails, naming the file, when a file's hash differs from the lock, a file is missing,
# a file the lock does not list is present, or the lock differs from a fresh export of the
# commit it names (which must be in this repository: fetch it first).

set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC=integrations/homeassistant/custom_components/chorus
PIN=integrations/homeassistant/harness.pin
NAME=chorus
LOCK=chorus.lock
MARK='# chorus for Home Assistant: the pinned install copy. Written by tools/ha-export.sh; never edited.'
export LC_ALL=C
umask 022

die() {
    echo "ha-export: FAIL: $*" >&2
    exit 1
}
usage() {
    sed -n '6,8p' "$0" | sed 's/^# \{0,3\}//' >&2
    exit 2
}

# export_commit <commit> <empty directory>: the tree and its lock, from the commit's objects.
export_commit() {
    local commit="$1" out="$2" mode type object path rel n=0 version ha
    mkdir -p "$out/$NAME"
    while IFS= read -r -d '' entry; do
        mode="${entry%% *}"
        entry="${entry#* }"
        type="${entry%% *}"
        entry="${entry#* }"
        object="${entry%%$'\t'*}"
        path="${entry#*$'\t'}"
        [ "$type" = blob ] && [ "$mode" = 100644 ] ||
            die "$path is $mode $type at $commit; the integration is plain files only (no link, no executable, no submodule)"
        rel="${path#"$SRC"/}"
        [[ "$rel" != *$'\n'* && "$rel" != *\\* ]] || die "a file name with a newline or a backslash: $path"
        mkdir -p "$out/$NAME/$(dirname "$rel")"
        git -C "$ROOT" cat-file blob "$object" > "$out/$NAME/$rel"
        n=$((n + 1))
    done < <(git -C "$ROOT" ls-tree -r -z "$commit" -- "$SRC")
    [ "$n" -gt 0 ] || die "$commit has no file under $SRC"
    [ -f "$out/$NAME/manifest.json" ] || die "$commit has no $SRC/manifest.json"
    find "$out/$NAME" -type d -exec chmod 0755 {} +
    find "$out/$NAME" -type f -exec chmod 0644 {} +
    version="$(sed -n 's/^ *"version": *"\([^"]*\)".*/\1/p' "$out/$NAME/manifest.json")"
    [ -n "$version" ] || die "$SRC/manifest.json at $commit names no version"
    ha="$(git -C "$ROOT" cat-file blob "$commit:$PIN" 2> /dev/null | sed -n 's/^homeassistant = \(.*\)$/\1/p')"
    [ -n "$ha" ] || die "$PIN at $commit names no homeassistant version"
    {
        printf '%s\n' "$MARK"
        printf '# source = %s\n' "$SRC"
        printf '# commit = %s\n' "$commit"
        printf '# version = %s\n' "$version"
        printf '# homeassistant = %s\n' "$ha"
        printf '# files = %s\n' "$n"
        (cd "$out" && find "$NAME" -type f -print0 | sort -z | xargs -0 sha256sum)
    } > "$out/$LOCK"
    chmod 0644 "$out/$LOCK"
}

# ours <dir>: a lock of this tool's is there.
ours() { [ -f "$1/$LOCK" ] && [ "$(head -n 1 "$1/$LOCK")" = "$MARK" ]; }

verify() {
    local dir="$1" commit fresh rc=0 listed present
    [ -d "$dir/$NAME" ] || die "$dir/$NAME is not a directory"
    ours "$dir" || die "$dir/$LOCK is missing or is not a lock this tool wrote"
    commit="$(sed -n 's/^# commit = \([0-9a-f]\{40\}\)$/\1/p' "$dir/$LOCK")"
    [ -n "$commit" ] || die "$dir/$LOCK names no commit"
    (cd "$dir" && sha256sum --check --strict --quiet "$LOCK") || rc=1
    listed="$(command grep -v '^#' "$dir/$LOCK" | sed 's/^[0-9a-f]\{64\}  //')"
    present="$(cd "$dir" && find "$NAME" -mindepth 1 ! -type d | sort)"
    if [ "$listed" != "$present" ]; then
        echo "ha-export: files the lock does not list, or lists and are absent (< the lock, > $dir/$NAME):" >&2
        diff <(printf '%s\n' "$listed") <(printf '%s\n' "$present") | command grep '^[<>]' >&2 || true
        rc=1
    fi
    git -C "$ROOT" cat-file -e "$commit^{commit}" 2> /dev/null ||
        die "the commit the lock names, $commit, is not in this repository (fetch it); the hashes above were still checked"
    fresh="$(mktemp -d "${TMPDIR:-/tmp}/chorus-ha-export.XXXXXX")"
    # shellcheck disable=SC2064
    trap "rm -rf '$fresh'" EXIT
    export_commit "$commit" "$fresh"
    if ! cmp -s "$fresh/$LOCK" "$dir/$LOCK"; then
        echo "ha-export: $dir/$LOCK is not the lock of $commit:" >&2
        diff "$fresh/$LOCK" "$dir/$LOCK" >&2 || true
        rc=1
    fi
    [ "$rc" = 0 ] || die "$dir is not the export of $commit"
    echo "ha-export: VERIFIED $dir/$NAME is chorus $commit ($(command grep -c -v '^#' "$dir/$LOCK") files, version $(sed -n 's/^# version = //p' "$dir/$LOCK"))"
}

[ $# -ge 1 ] || usage
if [ "$1" = --verify ]; then
    [ $# -eq 2 ] || usage
    verify "${2%/}"
    exit 0
fi
case "$1" in -*) usage ;; esac
[ $# -le 2 ] || usage
DIR="${1%/}"
COMMIT="$(git -C "$ROOT" rev-parse --verify --quiet "${2:-HEAD}^{commit}")" || die "${2:-HEAD} is not a commit of this repository"
[ -d "$DIR" ] || die "$DIR is not a directory; make it first"
if [ -e "$DIR/$NAME" ] || [ -e "$DIR/$LOCK" ]; then
    ours "$DIR" || die "$DIR/$NAME or $DIR/$LOCK exists and $DIR/$LOCK is not a lock this tool wrote; not replacing what is not an export"
fi
WORK="$(mktemp -d "$DIR/.chorus-export.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
export_commit "$COMMIT" "$WORK"
rm -rf "${DIR:?}/$NAME" "${DIR:?}/$LOCK"
mv "$WORK/$NAME" "$DIR/$NAME"
mv "$WORK/$LOCK" "$DIR/$LOCK"
echo "ha-export: wrote $DIR/$NAME and $DIR/$LOCK: chorus $COMMIT, $(command grep -c -v '^#' "$DIR/$LOCK") files, sha256 of the lock $(sha256sum "$DIR/$LOCK" | cut -d' ' -f1)"
