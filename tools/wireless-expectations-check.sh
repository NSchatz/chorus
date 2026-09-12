#!/usr/bin/env bash
# The published wireless expectations, against the committed configuration.
#
# AC-14 of chorus#WIFI-7: "WHEN the published wireless expectations are read THE
# SYSTEM SHALL state the same bounds and buffer numbers the committed
# configuration holds, and a disagreement between the document and the
# configuration SHALL fail the check."
#
# A document is the thing a person reads when they are deciding whether to put a
# room on Wi-Fi. It is also the thing nobody updates when a number moves, which
# is why this is a check and not a convention.
#
# The set of numbers is DERIVED from config/transport.conf rather than listed
# here: a key added to that file and not to the document turns this red, which
# is what keeps "the same numbers" true after the next one is committed. Listing
# them here would make this check a second copy of the thing it is checking.
#
# Needs no device, no privilege and no network. Runs under `make verify`.
#
#   ./tools/wireless-expectations-check.sh

source "$(dirname "$0")/lib.sh"

DOCUMENT="$REPO_ROOT/docs/wireless-expectations.md"

# Every `key = value` in a committed configuration file, one per line.
committed_pairs() {
    sed -e 's/#.*//' "$1" \
        | grep -E '^[[:space:]]*[a-z_]+[[:space:]]*=' \
        | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//' \
              -e 's/[[:space:]]*=[[:space:]]*/ = /'
}

# Compare one document against the committed configuration. Prints what it
# found and returns the number of disagreements.
check_document() {
    local document="$1"
    local mismatches=0
    local checked=0

    if [ ! -f "$document" ]; then
        say "FAIL $document is not there, so the published expectations are not published"
        return 1
    fi

    local pair key want saw
    while IFS= read -r pair; do
        key="${pair%% = *}"
        want="${pair#* = }"
        checked=$(( checked + 1 ))
        # Anchored on the opening backtick, so that `playout_latency_us` and
        # `wireless_playout_latency_us` are different keys rather than one being
        # a substring of the other.
        if ! grep -qF "\`$key = $want\`" "$document"; then
            saw="$(grep -oE "\`$key = [^\`]*\`" "$document" | head -n 1 || true)"
            if [ -n "$saw" ]; then
                say "FAIL $key: the configuration commits '$key = $want' and the document says '${saw//\`/}'"
            else
                say "FAIL $key: the configuration commits '$key = $want' and the document states it nowhere"
            fi
            mismatches=$(( mismatches + 1 ))
        fi
        # And the document must not state it twice with two different values.
        local stated
        stated="$(grep -oE "\`$key = [^\`]*\`" "$document" | sort -u | wc -l | tr -d ' ')"
        if [ "$stated" -gt 1 ]; then
            say "FAIL $key: the document states it $stated different ways"
            mismatches=$(( mismatches + 1 ))
        fi
    done < <(committed_pairs "$REPO_ROOT/config/transport.conf")

    # The one number from another file the document has to get right: the wired
    # playout latency it compares the wireless one against. It is committed in
    # config/sync.conf and this phase did not move it, which is the document's
    # most load-bearing sentence.
    local wired
    wired="$(sync_conf playout_latency_us)"
    checked=$(( checked + 1 ))
    if ! grep -qF "\`playout_latency_us = $wired\`" "$document"; then
        say "FAIL playout_latency_us: config/sync.conf commits '$wired' and the document does not state it"
        mismatches=$(( mismatches + 1 ))
    fi

    if [ "$checked" -lt 5 ]; then
        say "FAIL only $checked value(s) were compared; a check that examines almost nothing and a"
        say "     document that agrees with everything look identical from here"
        mismatches=$(( mismatches + 1 ))
    fi

    printf '%s' "$checked" > "${TMPDIR:-/tmp}/chorus-wireless-expectations-checked"
    return "$mismatches"
}

say "chorus: the published wireless expectations, against the committed configuration"
say ""

set +e
check_document "$DOCUMENT"
MISMATCHES=$?
set -e
CHECKED="$(cat "${TMPDIR:-/tmp}/chorus-wireless-expectations-checked" 2>/dev/null || printf '0')"

if [ "$MISMATCHES" -ne 0 ]; then
    say ""
    say "chorus: $MISMATCHES disagreement(s) between docs/wireless-expectations.md and the"
    say "        committed configuration. The configuration is what the system holds; the"
    say "        document is what a person is told. They have to be the same numbers."
    exit 1
fi
say "pass all $CHECKED committed values are stated in the document, and stated once"

# --- and the check is shown going red ----------------------------------------
#
# A check that has never failed is a check nobody has seen work. These run the
# comparison above over scratch copies of the document with a disagreement
# introduced, which is the same failure a stale document would produce.
say ""
say "--- the check, shown going red"

SCRATCH="${TMPDIR:-/tmp}/chorus-wireless-expectations-$$"
mkdir -p "$SCRATCH"
trap 'rm -rf "$SCRATCH"' EXIT

DEMONSTRATIONS=0

# A number that moved in the configuration and not in the document.
STALE="$SCRATCH/a-number-that-moved.md"
sed 's/`wireless_bound_us = 5000`/`wireless_bound_us = 9999`/' "$DOCUMENT" > "$STALE"
set +e
OUT="$(check_document "$STALE" 2>&1)"
STATUS=$?
set -e
if [ "$STATUS" -gt 0 ] && printf '%s' "$OUT" | grep -q 'wireless_bound_us'; then
    say "pass a number that moved is caught, naming the key:"
    printf '%s\n' "$OUT" | grep 'wireless_bound_us' | sed 's/^/    /'
    DEMONSTRATIONS=$(( DEMONSTRATIONS + 1 ))
else
    say "FAIL a document whose wireless bound disagrees with the configuration was accepted"
    exit 1
fi

# A value the document does not state at all, which is what a key added to the
# configuration and forgotten here looks like.
ABSENT="$SCRATCH/a-value-that-is-not-stated.md"
grep -v 'wireless_playout_latency_us' "$DOCUMENT" > "$ABSENT"
set +e
OUT="$(check_document "$ABSENT" 2>&1)"
STATUS=$?
set -e
if [ "$STATUS" -gt 0 ] && printf '%s' "$OUT" | grep -q 'states it nowhere'; then
    say "pass a committed value the document does not state is caught:"
    printf '%s\n' "$OUT" | grep 'states it nowhere' | sed 's/^/    /'
    DEMONSTRATIONS=$(( DEMONSTRATIONS + 1 ))
else
    say "FAIL a document that states no wireless playout latency at all was accepted"
    exit 1
fi

# The wired latency the document compares against, moved. This one matters on
# its own: the whole point of the document is the difference between the tiers,
# and a wrong wired number makes that difference wrong.
WIRED="$SCRATCH/the-wired-latency-moved.md"
sed 's/`playout_latency_us = 180000`/`playout_latency_us = 111111`/' "$DOCUMENT" > "$WIRED"
set +e
OUT="$(check_document "$WIRED" 2>&1)"
STATUS=$?
set -e
if [ "$STATUS" -gt 0 ] && printf '%s' "$OUT" | grep -q 'config/sync.conf commits'; then
    say "pass a wired latency that disagrees with config/sync.conf is caught:"
    printf '%s\n' "$OUT" | grep 'config/sync.conf commits' | sed 's/^/    /'
    DEMONSTRATIONS=$(( DEMONSTRATIONS + 1 ))
else
    say "FAIL a document whose wired playout latency disagrees with config/sync.conf was accepted"
    exit 1
fi

# A document that is not there at all.
set +e
OUT="$(check_document "$SCRATCH/nothing-here.md" 2>&1)"
STATUS=$?
set -e
if [ "$STATUS" -gt 0 ]; then
    say "pass a document that is not published at all is caught"
    DEMONSTRATIONS=$(( DEMONSTRATIONS + 1 ))
else
    say "FAIL an absent document was accepted"
    exit 1
fi

say ""
say "chorus: the published wireless expectations agree with the committed configuration,"
say "        and the check that says so was shown going red on $DEMONSTRATIONS ways of disagreeing"
exit 0
