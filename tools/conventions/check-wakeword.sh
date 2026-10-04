#!/usr/bin/env bash
# Rule "Dependencies and licences", the wake-word models (K73, ADR 0000): the detector runs only
# the model files in third_party/wakeword, and each one is on the licence list beside them.
#   1. Every file in third_party/wakeword other than LICENCES.md has a row in the list's
#      "Vendored files" table, and its SHA-256 is the one in the row. A row without a file fails too.
#   2. A row's licence is one of the allowed ones; a NonCommercial licence (openWakeWord's bundled
#      models are CC BY-NC-SA 4.0) or an openWakeWord source fails.
#   3. No model's name is on the list's "Excluded names" (another party's product or character,
#      such as `alexa`, and the names of openWakeWord's bundled models). The list must hold `alexa`.
#   4. The crate compiles in exactly the model files the directory holds.
# The self-test at the end runs the same function over damaged copies: an unlisted model, a
# changed byte, an excluded name and a NonCommercial row must each fail.
. "$(dirname "$0")/lib.sh"
dir=third_party/wakeword
allowed=" Apache-2.0 MIT BSD-2-Clause BSD-3-Clause CC0-1.0 "

# check_dir <directory>: prints one line per problem; returns 1 when there is any.
check_dir() {
    local d="$1" list="$1/LICENCES.md" bad=0 rows excluded f name stem row sha licence
    [ -f "$list" ] || { echo "$list is missing"; return 1; }
    # file|source|commit|sha|licence|read, from the table under "## Vendored files".
    rows="$(awk -F'|' '/^## Vendored files/ { on = 1; next } on && /^## / { exit }
        on && /^\| `/ { for (i = 2; i <= 8; i++) gsub(/^[ `<]+|[ `>]+$/, "", $i); print $2 "|" $4 "|" $5 "|" $6 "|" $7 "|" $8 }' "$list")"
    excluded="$(awk '/^## Excluded names/ { on = 1; next } on && /^## / { exit } on && /^- `/ { split($0, p, "`"); print p[2] }' "$list")"
    printf '%s\n' "$excluded" | command grep -q -x alexa || { echo "$list: the excluded names do not hold alexa"; bad=1; }
    for f in "$d"/*; do
        name="$(basename "$f")"
        [ "$name" = LICENCES.md ] && continue
        row="$(printf '%s\n' "$rows" | awk -F'|' -v n="$name" '$1 == n')"
        if [ -z "$row" ]; then
            echo "$f has no row in $list"
            bad=1
        else
            IFS='|' read -r _ _ _ sha _ _ <<< "$row"
            [ "$(sha256sum "$f" | cut -d' ' -f1)" = "$sha" ] || { echo "$f does not have the SHA-256 in its row"; bad=1; }
        fi
        # The name as the list writes names: lower case, anything but a letter or digit as `_`.
        stem="$(printf '%s' "${name%.*}" | tr '[:upper:]' '[:lower:]' | tr -c 'a-z0-9\n' '_')"
        while IFS= read -r x; do
            [ -n "$x" ] || continue
            case "$stem" in "$x" | "$x"_*) echo "$f: '$x' is an excluded name"; bad=1 ;; esac
        done <<< "$excluded"
        case "$name" in *.onnx) echo "$f: an ONNX model; the detector runs microWakeWord TFLite models only"; bad=1 ;; esac
    done
    while IFS='|' read -r name source commit sha licence read; do
        [ -n "$name" ] || continue
        [ -f "$d/$name" ] || { echo "$list lists $name, which is not in $d"; bad=1; }
        [[ "$allowed" == *" $licence "* ]] || { echo "$list: $name has licence '$licence', which is not allowed"; bad=1; }
        if [[ "$licence" == *NC* || "$licence" == *[Nn]on[Cc]ommercial* || "${source,,}" == *openwakeword* ]]; then
            echo "$list: $name is NonCommercial or from openWakeWord"
            bad=1
        fi
        [[ "$source" == https://* && "$commit" =~ ^[0-9a-f]{40}$ && "$sha" =~ ^[0-9a-f]{64}$ && "$read" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]] ||
            { echo "$list: the row of $name lacks a source URL, a 40-digit commit, a SHA-256 or the date read"; bad=1; }
    done <<< "$rows"
    command grep -q '^## The runtime' "$list" || { echo "$list has no section '## The runtime' with the runtime's licence"; bad=1; }
    return "$bad"
}

rc=0
out="$(check_dir "$dir")" || { printf '%s\n' "$out"; fail "Dependencies and licences" "third_party/wakeword does not match its licence list"; rc=1; }

# 4. what the crate compiles in is what the directory holds
built="$(command grep -o 'third_party/wakeword/[A-Za-z0-9_.-]*\.tflite' crates/wakeword/src/lib.rs | sort -u)"
held="$(printf '%s\n' "$dir"/*.tflite | sort -u)"
[ "$built" = "$held" ] || { fail "Dependencies and licences" "crates/wakeword/src/lib.rs compiles in other models than $dir holds"; rc=1; }

# Self-test: each damaged copy must fail, and the clean copy must pass.
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
model="$(basename "$(printf '%s\n' "$dir"/*.tflite | head -n 1)")"
# damage <how>: run inside a copy of the directory.
damage() {
    case "$1" in
    unlisted) cp "$model" stray.tflite ;;
    changed) printf 'x' >> "$model" ;;
    # A fully listed model under an excluded name: only the name can fail it.
    renamed) mv "$model" alexa.tflite && sed -i "s/\`$model\`/\`alexa.tflite\`/" LICENCES.md ;;
    noncommercial) sed -i "/^| \`$model\`/s/| Apache-2.0 |/| CC-BY-NC-SA-4.0 |/" LICENCES.md ;;
    missing) rm "$model" ;;
    esac
}
for how in unlisted changed renamed noncommercial missing; do
    mkdir -p "$scratch/$how"
    cp "$dir"/* "$scratch/$how"/
    (cd "$scratch/$how" && damage "$how")
    if check_dir "$scratch/$how" > /dev/null; then
        fail "Dependencies and licences" "self-test: a copy damaged as '$how' passed the check"
        rc=1
    fi
done
mkdir -p "$scratch/clean" && cp "$dir"/* "$scratch/clean"/
check_dir "$scratch/clean" > /dev/null || { fail "Dependencies and licences" "self-test: a clean copy failed the check"; rc=1; }

[ "$rc" = 0 ] && echo "wake-word models: $(printf '%s\n' "$dir"/*.tflite | wc -l) model, $(($(printf '%s\n' "$dir"/* | wc -l) - 1)) files, each listed with its checksum and an allowed licence; no excluded name ($(awk '/^## Excluded names/ { on = 1; next } on && /^- `/ { n++ } END { print n }' "$dir/LICENCES.md") excluded, alexa among them); self-test: 5 damaged copies fail"
exit "$rc"
