#!/usr/bin/env bash
# The control page, RENDERED, against the frontend conventions.
#
# Two real `chorus-server` processes are started - one with zones, one with
# none - and the page each of them serves is loaded in a real browser engine.
# Beside them runs tools/ui/fixture-server.js, which PROXIES that same page, its
# stylesheet, its script and its document through byte for byte and answers only
# the state request and the event stream itself: the states a correct server will
# never produce (a figure that cannot be read, a feed that dies and comes back, a
# state request that has not answered yet, a server that stops answering under a
# connection that is still established) are reached by doctoring the state and
# never by doctoring the page.
#
# Every assertion is on the rendered text, on a painted box read back with
# getBoundingClientRect(), on a colour read out of the framebuffer, on a width
# the engine laid out, on the accessibility tree Chromium computed, or on the
# browser's own Content-Security-Policy violation reports. There is no assertion
# here that a text search of the HTML, the JS or the CSS could satisfy.
#
# One thing IS read out of a file, and it is worth saying exactly what. The
# token file records the contrast ratio measured between each token pair, and
# tools/ui/reconcile.js reads those recorded NUMBERS back and puts each beside
# the ratio the engine measured off the painted pixels for the pair that painted
# it. The recorded number is the thing under test; the number it is judged
# against comes from the framebuffer, and no floor is ever decided from the
# file. A text search cannot satisfy that assertion, because half of it is a
# measurement.
#
# That is not a stylistic preference. A text grader cannot decide what a rule
# applies to, what wins the cascade, or what is SHOWN rather than merely built,
# and hardening one only closes the hole it was shown. So this refuses by name
# when there is no engine or no driver, rather than falling back to reading the
# stylesheet.
#
# Three things are checked after the rendering, because a green run cannot show
# any of them from the inside:
#
#   every declared claim ran, so a deleted assertion cannot shrink the check;
#   every claim was shown going red on a page that breaks it, in the file that
#   holds the mutated pages and not beside the real one;
#   docs/frontend-conventions-record.md maps all eleven clauses, each to one
#   thing, and every assertion it names ran in this invocation.
#
# The install, once, is:
#
#     mise use node@22          # if node is not already here
#     cd tools/ui && PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1 pnpm install --ignore-scripts
#
# The browser is the one already in the container. Nothing under tools/ui ships:
# the Cargo workspace has no dependency on it and the server serves no file from
# it.
#
#   ./tools/ui-render-run.sh      # or: make verify-ui

source "$(dirname "$0")/lib.sh"

build_once

CRITERION="the control page holds the frontend conventions F1 to F11 and the rendered half of the styling conventions: contrast and focus in both themes, keyboard operation, accessible names, no state carried by colour alone, an aggregate that states its set, an unreadable figure that costs nothing else, a severed feed and a paused one that both stop reading as current, three states with a loading one that resolves even against a server that answers nothing, short labels with the paragraphs in the repo's docs, a 360 pixel layout for every name the catalog admits, a Content-Security-Policy the browser does not complain about, every recorded contrast ratio reconciled against the pixels the pair painted, and the figure face on every run whose column alignment carries meaning"
require_browser_driver "$CRITERION"

FAILURES=0
check() {
    local name="$1"
    local ok="$2"
    local detail="$3"
    if [ "$ok" = "1" ]; then
        say "pass $name: $detail"
    else
        say "FAIL $name: $detail"
        FAILURES=$(( FAILURES + 1 ))
    fi
}

OUT_DIR="${TMPDIR:-/tmp}/chorus-ui-render"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"
LEDGER="$OUT_DIR/claims.ledger"
: >"$LEDGER"
EMPTY_LEDGER="$OUT_DIR/nothing-ran.ledger"
: >"$EMPTY_LEDGER"
RECORD="$REPO_ROOT/docs/frontend-conventions-record.md"

CONTRACT_ARGS="$(server_contract_args)"
WITH_ZONES="$(free_port)"
WITH_NONE="$(free_port)"
FIXTURE_PORT="$(free_port)"
AUDIO_ONE="$(free_port)"
AUDIO_TWO="$(free_port)"

PIDS=()
stop_everything() {
    local pid
    for pid in "${PIDS[@]:-}"; do
        if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
            kill -9 "$pid" 2>/dev/null || true
            wait "$pid" 2>/dev/null || true
        fi
    done
    PIDS=()
}
trap stop_everything EXIT

start_server() {
    local audio="$1"
    local control="$2"
    local log="$3"
    shift 3
    "$BIN_DIR/chorus-server" \
        --listen "127.0.0.1:$audio" \
        --source tone \
        --serve-forever \
        --control-listen "127.0.0.1:$control" \
        --control-workers 12 \
        $CONTRACT_ARGS "$@" >"$log" 2>&1 &
    PIDS+=("$!")
    local waited=0
    while [ "$waited" -lt 100 ]; do
        if grep -q 'control listening on=' "$log" 2>/dev/null; then
            return 0
        fi
        sleep 0.2
        waited=$(( waited + 1 ))
    done
    say "FAIL a server never came up; it said:"
    sed 's/^/    /' "$log"
    exit 1
}

say ""
say "chorus: two servers, one with zones and one with none"
start_server "$AUDIO_ONE" "$WITH_ZONES" "$OUT_DIR/server-zones.log" --zone kitchen --zone study
start_server "$AUDIO_TWO" "$WITH_NONE" "$OUT_DIR/server-empty.log"
check "the-page-is-served-by-a-real-server" \
    "$(grep -q 'control listening on=' "$OUT_DIR/server-zones.log" && echo 1 || echo 0)" \
    "$(grep 'control listening on=' "$OUT_DIR/server-zones.log" | head -n 1)"

say "chorus: a fixture in front of that same page, for the states a server cannot be made to produce"
CHORUS_UI_BASE="http://127.0.0.1:$WITH_ZONES" \
CHORUS_FIXTURE_PORT="$FIXTURE_PORT" \
    node "$REPO_ROOT/tools/ui/fixture-server.js" >"$OUT_DIR/fixture.log" 2>&1 &
PIDS+=("$!")
WAITED=0
while [ "$WAITED" -lt 100 ]; do
    if grep -q 'fixture listening on=' "$OUT_DIR/fixture.log" 2>/dev/null; then
        break
    fi
    sleep 0.1
    WAITED=$(( WAITED + 1 ))
done
check "the-doctored-state-is-served-through-the-same-page" \
    "$(grep -q 'fixture listening on=' "$OUT_DIR/fixture.log" && echo 1 || echo 0)" \
    "$(head -n 1 "$OUT_DIR/fixture.log")"

BROWSER="${CHORUS_BROWSER:-/usr/bin/chromium}"
say "chorus: rendering in $("$BROWSER" --version 2>/dev/null | head -n 1)"

set +e
(
    cd "$REPO_ROOT/tools/ui" && \
    CHORUS_UI_BASE="http://127.0.0.1:$WITH_ZONES" \
    CHORUS_UI_EMPTY_BASE="http://127.0.0.1:$WITH_NONE" \
    CHORUS_UI_FIXTURE="http://127.0.0.1:$FIXTURE_PORT" \
    CHORUS_UI_LEDGER="$LEDGER" \
    CHORUS_BROWSER="$BROWSER" \
    node node_modules/@playwright/test/cli.js test --config playwright.config.js
) 2>&1 | tee "$OUT_DIR/render.out"
RENDER_STATUS="${PIPESTATUS[0]}"
set -e

check "the-rendered-page-holds-every-assertion" \
    "$([ "$RENDER_STATUS" -eq 0 ] && echo 1 || echo 0)" \
    "the rendering driver exited $RENDER_STATUS"

# The demonstrations are what make the measurement evidence rather than a
# formality, so their absence is a failure of this check and not a quiet
# omission. Counted from what the run recorded, not from a number written here:
# a claim that stopped running has to be visible from outside a green run.
say ""
say "--- every claim ran, and every claim was shown going red"
set +e
CLAIMS_OUT="$(node "$REPO_ROOT/tools/ui/check-claims.js" "$LEDGER" 2>&1)"
CLAIMS_STATUS=$?
set -e
printf '%s\n' "$CLAIMS_OUT" | sed 's/^/    /'
check "the-measurement-was-shown-going-red-on-a-page-that-breaks-it" \
    "$([ "$CLAIMS_STATUS" -eq 0 ] && echo 1 || echo 0)" \
    "$(printf '%s' "$CLAIMS_OUT" | grep -E '^(pass|FAIL)' | head -n 1)"

# And that reader shown refusing too. `proves` and `demonstrates` are both
# exported to both spec files, so a demonstration recorded beside the real page
# would satisfy the count above while demonstrating nothing. Every ledger line
# carries the file it was written from; this is the run's OWN ledger with every
# demonstration relabelled as though it had been recorded in ui.spec.js, which
# has to be refused by name.
MISFILED_LEDGER="$OUT_DIR/misfiled.ledger"
sed 's/ mutation\.spec\.js$/ ui.spec.js/' "$LEDGER" >"$MISFILED_LEDGER"
set +e
MISFILED_OUT="$(node "$REPO_ROOT/tools/ui/check-claims.js" "$MISFILED_LEDGER" 2>&1)"
MISFILED_STATUS=$?
set -e
MISFILED_OK=1
[ "$MISFILED_STATUS" -eq 0 ] && MISFILED_OK=0
printf '%s' "$MISFILED_OUT" | grep -q 'belongs in mutation.spec.js' || MISFILED_OK=0
check "a-demonstration-recorded-beside-the-real-page-is-refused-naming-it" \
    "$MISFILED_OK" \
    "exit $MISFILED_STATUS: $(printf '%s' "$MISFILED_OUT" | grep 'belongs in mutation.spec.js' | head -n 1)"

# The committed record of the eleven clauses, read against what actually ran.
say ""
say "--- the clause record maps all eleven, each to one thing, and every assertion it names ran"
set +e
RECORD_OUT="$(node "$REPO_ROOT/tools/ui/check-clause-record.js" "$RECORD" "$LEDGER" 2>&1)"
RECORD_STATUS=$?
set -e
printf '%s\n' "$RECORD_OUT" | sed 's/^/    /'
check "the-clause-record-is-complete-and-current" \
    "$([ "$RECORD_STATUS" -eq 0 ] && echo 1 || echo 0)" \
    "$(printf '%s' "$RECORD_OUT" | head -n 1)"

# And the reader itself, shown refusing. A checker that cannot refuse would make
# the record above a decoration.
refuses() {
    local name="$1"
    local record="$2"
    local ledger="$3"
    local wanted="$4"
    local out status
    set +e
    out="$(node "$REPO_ROOT/tools/ui/check-clause-record.js" "$record" "$ledger" 2>&1)"
    status=$?
    set -e
    local ok=1
    [ "$status" -eq 0 ] && ok=0
    printf '%s' "$out" | grep -q "$wanted" || ok=0
    check "$name" "$ok" "exit $status: $(printf '%s' "$out" | head -n 1)"
}

say ""
say "--- and the reader refuses a record that is short, doubled, empty or out of date"
refuses "a-record-missing-a-clause-is-refused-naming-it" \
    "$REPO_ROOT/tools/ui/fixtures/record-missing-a-clause.md" "$LEDGER" "F7 is absent"
refuses "a-clause-mapped-twice-over-is-refused-naming-it" \
    "$REPO_ROOT/tools/ui/fixtures/record-doubly-mapped.md" "$LEDGER" "F4 carries both"
refuses "a-clause-mapped-to-nothing-is-refused-naming-it" \
    "$REPO_ROOT/tools/ui/fixtures/record-empty-row.md" "$LEDGER" "F5 names neither"
refuses "an-assertion-that-did-not-run-is-refused-naming-the-clause" \
    "$RECORD" "$EMPTY_LEDGER" "did not run in this invocation"

say ""
if [ "$FAILURES" -eq 0 ]; then
    say "chorus: the rendered page holds F1 to F11, every claim was shown going red, and the record is current"
    exit 0
fi
say "chorus: $FAILURES rendered-UI checks did not hold"
exit 1
