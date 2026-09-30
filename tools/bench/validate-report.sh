#!/usr/bin/env bash
# Validate one bench report against its schema (chorus-bench-report/1, K45).
#
#   bash tools/bench/validate-report.sh docs/measurements/<topic>-<date>.md \
#       [--base-ref origin/main] [--allow-fixture-raw] [--root <checkout>]
#
# Every goal runs this on each open bench/* PR before merging it
# (tools/bench/validate-open-prs.sh does the listing and the checkout). It
# checks, and names every failure:
#   - the file is docs/measurements/<topic>-<date>[-N].md for a topic in
#     tools/bench/topics.conf, and its Topic, Date and Entry point lines agree
#   - exactly one each of the header lines Schema, Source, Build measured,
#     Topic, Entry point, Date, Result, Criterion and Reproduce with, and at
#     least one Device line; `Source: hardware`; a Result word of PASS, FAIL,
#     MEASURED or INCOMPLETE
#   - the build commit exists and is an ancestor of the base ref (default
#     origin/main): a report names a build anybody can check out
#   - every field the topic requires is in the `## Fields` table
#   - the `## Raw data` table lists at least one file; every `committed` file is
#     present under docs/measurements/raw/<stem>/ with the listed size and
#     sha256; every `owner` file is larger than the committed limit and, when
#     present, matches too; nothing under the raw directory is unlisted
#   - no raw file is byte-identical to a committed fixture (a fixture is not a
#     hardware capture) unless --allow-fixture-raw, which only the
#     fixture-driven test passes
#   - no em dash, no email or user@host, no MAC, no private IPv4 address and no
#     home-directory path anywhere in the report (K27)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FILE_LIMIT=16777216
BASE_REF="origin/main"
ALLOW_FIXTURE_RAW=0
REPORT=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        --base-ref) BASE_REF="$2"; shift 2 ;;
        --allow-fixture-raw) ALLOW_FIXTURE_RAW=1; shift ;;
        --root) ROOT="$(cd "$2" && pwd)"; shift 2 ;;
        -*) echo "validate-report: unknown option $1" >&2; exit 2 ;;
        *) REPORT="$1"; shift ;;
    esac
done
[ -n "$REPORT" ] || { echo "usage: validate-report.sh <report.md> [--base-ref <ref>] [--allow-fixture-raw]" >&2; exit 2; }

REL="${REPORT#"$ROOT"/}"
PATH_ON_DISK="$ROOT/$REL"
FAILS=0
bad() { printf 'FAIL %s: %s\n' "$REL" "$*"; FAILS=$((FAILS + 1)); }

[ -f "$PATH_ON_DISK" ] || { bad "no such file"; exit 1; }

# --- the name --------------------------------------------------------------
STEM="$(basename "$REL" .md)"
case "$REL" in docs/measurements/*.md) ;; *) bad "a bench report lives directly under docs/measurements/" ;; esac
[ "$(dirname "$REL")" = "docs/measurements" ] || bad "a bench report lives directly under docs/measurements/"
NAME_DATE="$(printf '%s' "$STEM" | sed -n 's/.*-\([0-9]\{4\}-[0-9]\{2\}-[0-9]\{2\}\)\(-[0-9][0-9]*\)\{0,1\}$/\1/p')"
NAME_TOPIC="$(printf '%s' "$STEM" | sed -n 's/^\(.*\)-[0-9]\{4\}-[0-9]\{2\}-[0-9]\{2\}\(-[0-9][0-9]*\)\{0,1\}$/\1/p')"
[ -n "$NAME_DATE" ] && [ -n "$NAME_TOPIC" ] || bad "the name is not <topic>-<YYYY-MM-DD>[-N].md"

topic_column() {
    awk -F'|' -v t="$1" -v c="$2" '
        /^[[:space:]]*#/ || NF < 3 { next }
        { k = $1; gsub(/^[ \t]+|[ \t]+$/, "", k) }
        k == t { v = $c; gsub(/^[ \t]+|[ \t]+$/, "", v); print v; exit }
    ' "$ROOT/tools/bench/topics.conf"
}

# --- the header lines --------------------------------------------------------
for key in Schema Source 'Build measured' Topic 'Entry point' Date Result Criterion 'Reproduce with'; do
    n="$(grep -c "^$key: " "$PATH_ON_DISK" || true)"
    [ "$n" = 1 ] || bad "needs exactly one '$key:' line, has $n"
done
one() { sed -n "s/^$1: //p" "$PATH_ON_DISK" | head -n 1; }
SCHEMA="$(one Schema)"
SOURCE="$(one Source)"
BUILD_LINE="$(one 'Build measured')"
TOPIC="$(one Topic)"
ENTRY="$(one 'Entry point' | tr -d '`')"
DATE="$(one Date)"
RESULT="$(one Result)"
[ "$(grep -c '^Device: .' "$PATH_ON_DISK" || true)" -ge 1 ] || bad "needs at least one 'Device:' line"

[ "$SCHEMA" = "chorus-bench-report/1" ] || bad "Schema is '$SCHEMA', not chorus-bench-report/1"
[ "$SOURCE" = "hardware" ] || bad "Source is '$SOURCE'; a bench report is 'Source: hardware'"
printf '%s' "$RESULT" | grep -qE '^(PASS|FAIL|MEASURED|INCOMPLETE): .+' \
    || bad "Result '$RESULT' is not '<PASS|FAIL|MEASURED|INCOMPLETE>: <summary>'"
[ "$TOPIC" = "$NAME_TOPIC" ] || bad "Topic '$TOPIC' does not match the file name's '$NAME_TOPIC'"
[ "$DATE" = "$NAME_DATE" ] || bad "Date '$DATE' does not match the file name's '$NAME_DATE'"
WANT_ENTRY="$(topic_column "$TOPIC" 2)"
if [ -z "$WANT_ENTRY" ]; then
    bad "Topic '$TOPIC' is not in tools/bench/topics.conf"
else
    [ "$ENTRY" = "$WANT_ENTRY" ] || bad "Entry point '$ENTRY' is not the topic's '$WANT_ENTRY'"
    [ -f "$ROOT/$ENTRY" ] || bad "Entry point '$ENTRY' is not in this checkout"
fi

# --- the build ---------------------------------------------------------------
# shellcheck disable=SC2016 # the backticks are literal Markdown in the report
SHA="$(printf '%s' "$BUILD_LINE" | sed -n 's/^`\{0,1\}\([0-9a-f]\{40\}\)`\{0,1\}$/\1/p')"
if [ -z "$SHA" ]; then
    bad "Build measured '$BUILD_LINE' is not a 40-hex commit"
elif ! git -C "$ROOT" cat-file -e "$SHA^{commit}" 2>/dev/null; then
    bad "Build measured $SHA is not a commit in this repository"
elif ! git -C "$ROOT" merge-base --is-ancestor "$SHA" "$BASE_REF" 2>/dev/null; then
    bad "Build measured $SHA is not an ancestor of $BASE_REF (a branch or unpushed commit)"
fi

# --- the fields --------------------------------------------------------------
FIELDS="$(sed -n '/^## Fields/,/^## /p' "$PATH_ON_DISK" \
    | sed -n 's/^| \([a-z0-9_]*\) | .* |$/\1/p')"
for field in $(topic_column "$TOPIC" 3); do
    printf '%s\n' "$FIELDS" | grep -qx "$field" || bad "the topic requires field '$field' in ## Fields"
done

# --- the raw data --------------------------------------------------------------
RAWDIR="docs/measurements/raw/$STEM"
# shellcheck disable=SC2016 # the backticks are literal Markdown in the report
ROWS="$(sed -n '/^## Raw data/,/^## /p' "$PATH_ON_DISK" \
    | grep -E '^\| `[^`]+` \| [0-9]+ \| [0-9a-f]{64} \| (committed|owner) \|$' || true)"
[ -n "$ROWS" ] || bad "## Raw data lists no file"
LISTED=""
FIXTURE_HASHES=""
if [ "$ALLOW_FIXTURE_RAW" != 1 ]; then
    FIXTURE_HASHES="$(cd "$ROOT" && git ls-files -z fixtures | xargs -0 -r sha256sum | cut -d' ' -f1)"
fi
while IFS= read -r row; do
    [ -n "$row" ] || continue
    # shellcheck disable=SC2016 # the backticks are literal Markdown in the report
    path="$(printf '%s' "$row" | sed 's/^| `\([^`]*\)` .*/\1/')"
    size="$(printf '%s' "$row" | awk -F' [|] ' '{print $2}')"
    sha="$(printf '%s' "$row" | awk -F' [|] ' '{print $3}')"
    kept="$(printf '%s' "$row" | awk -F' [|] ' '{print $4}' | tr -d ' |')"
    case "$path" in "$RAWDIR"/*) ;; *) bad "raw file '$path' is not under $RAWDIR/" ;; esac
    LISTED="$LISTED$path"$'\n'
    if [ -n "$FIXTURE_HASHES" ] && printf '%s\n' "$FIXTURE_HASHES" | grep -qx "$sha"; then
        bad "raw file '$path' is byte-identical to a committed fixture; a fixture is not a hardware capture"
    fi
    if [ "$kept" = owner ] && [ "$size" -le "$FILE_LIMIT" ]; then
        bad "raw file '$path' is $size bytes, within the $FILE_LIMIT committed limit, and is marked owner"
    fi
    if [ -f "$ROOT/$path" ]; then
        [ "$(wc -c < "$ROOT/$path" | tr -d ' ')" = "$size" ] || bad "raw file '$path' is not $size bytes"
        [ "$(sha256sum "$ROOT/$path" | cut -d' ' -f1)" = "$sha" ] || bad "raw file '$path' does not hash to $sha"
    elif [ "$kept" = committed ]; then
        bad "raw file '$path' is marked committed and is not in the checkout"
    fi
done <<< "$ROWS"
if [ -d "$ROOT/$RAWDIR" ]; then
    while IFS= read -r f; do
        printf '%s' "$LISTED" | grep -qxF "$f" || bad "'$f' is under $RAWDIR/ and not listed in ## Raw data"
    done < <(cd "$ROOT" && find "$RAWDIR" -type f | LC_ALL=C sort)
fi

# --- identity and style (K27, BRIEF 3.1) ---------------------------------------
grep -q $'\xe2\x80\x94' "$PATH_ON_DISK" && bad "contains an em dash"
AT="$(grep -nE '[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+' "$PATH_ON_DISK" | head -n 1 || true)"
[ -z "$AT" ] || bad "contains an email or user@host (line ${AT%%:*})"
grep -qiE '\b([0-9a-f]{2}:){5}[0-9a-f]{2}\b' "$PATH_ON_DISK" && bad "contains a MAC address"
grep -qE '(^|[^0-9.])(10|192\.168|172\.(1[6-9]|2[0-9]|3[01]))(\.[0-9]{1,3}){2,3}([^0-9.]|$)' "$PATH_ON_DISK" \
    && bad "contains a private IPv4 address"
grep -qE '(/home/|/Users/|/root/)' "$PATH_ON_DISK" && bad "contains a home-directory path"

if [ "$FAILS" -eq 0 ]; then
    printf 'bench-report: %s valid (%s, %s, %s, %s raw file(s))\n' "$REL" "$TOPIC" "$DATE" \
        "${RESULT%%:*}" "$(printf '%s' "$ROWS" | grep -c . || true)"
    exit 0
fi
printf 'bench-report: %s INVALID (%s failure(s))\n' "$REL" "$FAILS"
exit 1
