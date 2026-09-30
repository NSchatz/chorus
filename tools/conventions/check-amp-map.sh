#!/usr/bin/env bash
# Rule "Datasheet-cited amplifier map" (chorus goal 9, brief section 13): every `amp_` key in
# firmware/config/endpoint.conf has a value (never `unknown`) and cites, on its own line, the page
# of TI's TAS5825M datasheet (SLASEH7H, 106 pages) it was read from. The check proves itself
# first: a map with an `unknown` key and one with an uncited key must both fail.
. "$(dirname "$0")/lib.sh"
conf=firmware/config/endpoint.conf
cite='TAS5825M datasheet SLASEH7H rev H, pp?\. [0-9]'

# grade <file>: print one line per defect; the count of keys, unknown and cited on the last line.
grade() {
    awk -v cite="$cite" '
        /^[[:space:]]*amp_[a-z0-9_]*[[:space:]]*=/ {
            keys++
            line = $0
            value = line
            sub(/^[^=]*=[[:space:]]*/, "", value)
            sub(/[[:space:]]*#.*$/, "", value)
            key = line
            sub(/[[:space:]]*=.*$/, "", key)
            sub(/^[[:space:]]*/, "", key)
            if (value == "unknown") { unknown++; print "unknown: " key " (line " NR ")" }
            comment = line
            if (index(comment, "#") == 0) { comment = "" } else { sub(/^[^#]*#/, "", comment) }
            if (comment !~ cite) { print "uncited: " key " (line " NR ")"; next }
            pages = comment
            sub(/.*SLASEH7H rev H, pp?\. /, "", pages)
            match(pages, /^[0-9]+(-[0-9]+|, [0-9]+)*/)
            pages = substr(pages, 1, RLENGTH)
            n = split(pages, part, /[^0-9]+/)
            ok = 1
            for (i = 1; i <= n; i++) {
                if (part[i] != "" && (part[i] + 0 < 1 || part[i] + 0 > 106)) { ok = 0 }
            }
            if (!ok) { print "page out of range: " key " (line " NR ")"; next }
            cited++
        }
        END { printf "COUNT %d %d %d\n", keys, unknown, cited }
    ' "$1"
}

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
printf 'amp_reg_x = unknown # TAS5825M datasheet SLASEH7H rev H, p. 48\n' > "$scratch/unknown.conf"
printf 'amp_reg_x = 0x03\n' > "$scratch/uncited.conf"
printf 'amp_reg_x = 0x03 # TAS5825M datasheet SLASEH7H rev H, p. 480\n' > "$scratch/page.conf"
for f in unknown uncited page; do
    if [ "$(grade "$scratch/$f.conf" | command grep -c -v '^COUNT')" = 0 ]; then
        fail "Datasheet-cited amplifier map" "the check passed a map with an $f key; it no longer grades"
        exit 1
    fi
done

out="$(grade "$conf")"
defects="$(printf '%s\n' "$out" | command grep -v '^COUNT' || true)"
read -r _ keys unknown cited <<< "$(printf '%s\n' "$out" | command grep '^COUNT')"
echo "amp map: $keys amp_ keys in $conf, $unknown unknown, $cited cite a TAS5825M datasheet page (self-test: unknown, uncited and out-of-range pages fail)"
if [ -n "$defects" ] || [ "$keys" = 0 ]; then
    printf '%s\n' "$defects"
    fail "Datasheet-cited amplifier map" "every amp_ key needs a value and its datasheet page on its own line"
    exit 1
fi
