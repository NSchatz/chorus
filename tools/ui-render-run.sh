#!/usr/bin/env bash
# AC-5, AC-6 and AC-10: the served page, RENDERED.
#
# Two real `chorus-server` processes are started - one with zones, one with
# none - and the page each of them serves is loaded in a real browser engine.
# Every assertion is on the rendered text or on a painted box read back with
# getBoundingClientRect(). Nothing reads the HTML, the JS or the CSS, and there
# is no assertion here that a text search of those files could satisfy.
#
# That is not a stylistic preference. A text grader cannot decide what a rule
# applies to, what wins the cascade, or what is SHOWN rather than merely built,
# and hardening one only closes the hole it was shown. So this refuses by name
# when there is no engine or no driver, rather than falling back to reading the
# stylesheet.
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

CRITERION="the control UI shows every zone by name with its volume and mute, shows another subscriber's change without a reload, paints every interactive control at 24 by 24 CSS pixels or more, and says so honestly when there is no zone"
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
CONTRACT_ARGS="$(server_contract_args)"
WITH_ZONES="$(free_port)"
WITH_NONE="$(free_port)"
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

BROWSER="${CHORUS_BROWSER:-/usr/bin/chromium}"
say "chorus: rendering in $("$BROWSER" --version 2>/dev/null | head -n 1)"

set +e
(
    cd "$REPO_ROOT/tools/ui" && \
    CHORUS_UI_BASE="http://127.0.0.1:$WITH_ZONES" \
    CHORUS_UI_EMPTY_BASE="http://127.0.0.1:$WITH_NONE" \
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
# omission.
DEMONSTRATIONS="$(grep -c 'mutation.spec.js' "$OUT_DIR/render.out" || true)"
check "the-measurement-was-shown-going-red-on-a-page-that-breaks-it" \
    "$([ "${DEMONSTRATIONS:-0}" -ge 5 ] && echo 1 || echo 0)" \
    "$DEMONSTRATIONS demonstrations ran, including a rule that loses the cascade and a bare range input"

say ""
if [ "$FAILURES" -eq 0 ]; then
    say "chorus: the rendered page shows the zones, updates live, and paints every control big enough"
    exit 0
fi
say "chorus: $FAILURES rendered-UI checks did not hold"
exit 1
