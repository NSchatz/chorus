#!/usr/bin/env bash
# `make gate`: the one check a change has to pass before it merges.
#
# Every step runs, in order, even after one fails, so a red run lists every
# broken step and not only the first. Each step prints its exit code and its
# wall-clock; the run ends with the total and PASS or FAIL. A step's full output
# goes to its own log under CHORUS_GATE_LOG (default target/gate/), and the last
# lines of a failing step are printed beside it.
#
# The gate never takes chorus-heavy.lock itself; callers do (the merge rule in
# .claude/goals/2026-09-chorus.md, section 0.3).
#
#   make gate             # everything
#   make gate-fast        # the docs checks only, for docs-only changes
#
# Environment:
#   CHORUS_IDF_ENV            a script that puts ESP-IDF on PATH and sets IDF_PATH,
#                             sourced when IDF_PATH is unset (Makefile default:
#                             the rootless install this repository is built with)
#   CHORUS_CCACHE_DIR         where ccache keeps its cache (default /cache/ccache/chorus)
#   (the image step pulls its digest-pinned base once, then reads it from a cache)
#   CHORUS_GATE_OUTAGE_SECONDS  the outage-of-minutes run's length in the gate
#                             (default 30; `make firmware-check` runs the committed 130)
#   IDF_PY_BUILD_JOBS         idf.py's ninja job count; passed through explicitly

set -u
cd "$(dirname "$0")/.." || exit 2
ROOT="$(pwd)"

MODE="${1:-full}"
LOG="${CHORUS_GATE_LOG:-${CARGO_TARGET_DIR:-$ROOT/target}/gate}"
mkdir -p "$LOG"
: > "$LOG/summary.txt"
FAILED=()
T0=$(date +%s.%N)

elapsed() { awk -v a="$1" -v b="$2" 'BEGIN{printf "%.1f", b-a}'; }

step() {
    local name="$1"
    shift
    local s e rc
    s=$(date +%s.%N)
    ( "$@" ) > "$LOG/$name.log" 2>&1
    rc=$?
    e=$(date +%s.%N)
    printf 'gate: %-22s rc=%-3s %7ss\n' "$name" "$rc" "$(elapsed "$s" "$e")" | tee -a "$LOG/summary.txt"
    if [ "$rc" -ne 0 ]; then
        FAILED+=("$name")
        tail -n 15 "$LOG/$name.log" | sed "s/^/  $name| /"
    fi
}

# --- the docs checks (also the whole of gate-fast) ---------------------------

# BRIEF.md section 3.1, guardrail 5. Written with an escape so this file does
# not trip itself.
no_em_dash() {
    local hits
    hits="$(git grep -nI "$(printf '\342\200\224')" -- . || true)"
    if [ -n "$hits" ]; then
        printf '%s\n' "$hits"
        echo "the tracked tree holds an em dash (U+2014); BRIEF.md section 3.1 rule 5 forbids it"
        return 1
    fi
    echo "no em dash in $(git ls-files | wc -l) tracked files"
}

# K52: CLAUDE.md stays at or under 200 lines.
claude_md_length() {
    local n
    n="$(wc -l < CLAUDE.md)"
    echo "CLAUDE.md: $n lines (limit 200)"
    [ "$n" -le 200 ]
}

# --- the firmware image ------------------------------------------------------

firmware_idf() {
    if [ -z "${IDF_PATH:-}" ] && [ -n "${CHORUS_IDF_ENV:-}" ] && [ -f "$CHORUS_IDF_ENV" ]; then
        # shellcheck disable=SC1090
        . "$CHORUS_IDF_ENV" > "$LOG/idf-env.log" 2>&1
    fi
    if ! command -v ccache > /dev/null 2>&1 && command -v mise > /dev/null 2>&1; then
        PATH="$(mise where github:ccache/ccache@4.14.1 2>/dev/null):$PATH"
    fi
    if ! command -v ccache > /dev/null 2>&1; then
        echo "ccache is not on PATH; the gate builds the firmware through it"
        echo "install it rootless: mise install github:ccache/ccache@4.14.1, and put \$(mise where github:ccache/ccache@4.14.1) on PATH"
        return 1
    fi
    export CCACHE_DIR="${CHORUS_CCACHE_DIR:-/cache/ccache/chorus}"
    mkdir -p "$CCACHE_DIR"
    echo "ccache: $(ccache --version | head -n 1), CCACHE_DIR=$CCACHE_DIR"
    ccache --zero-stats > /dev/null
    CHORUS_IDF_CCACHE=1 \
        CHORUS_IMAGE_OUT="$ROOT/firmware/build/gate-esp32s3" \
        IDF_PY_BUILD_JOBS="${IDF_PY_BUILD_JOBS:-2}" \
        bash tools/firmware-image.sh || return 1
    ccache --show-stats | head -n 8
}

echo "gate: $MODE, $(git rev-parse --short HEAD 2>/dev/null), $(cargo --version), IDF_PY_BUILD_JOBS=${IDF_PY_BUILD_JOBS:-2}" | tee -a "$LOG/summary.txt"

step no-em-dash         no_em_dash
step claude-md-length   claude_md_length

if [ "$MODE" = full ]; then
    step fmt              cargo fmt --all --check
    step clippy           cargo clippy --workspace --all-targets --locked -- -D warnings
    step build            cargo build --workspace --all-targets --locked
    step test             cargo test --workspace --locked
    step determinism      make --no-print-directory verify-control-determinism
    step firmware-check   env CHORUS_OUTAGE_SECONDS="${CHORUS_GATE_OUTAGE_SECONDS:-30}" \
                              make --no-print-directory firmware-check
    step verify           make --no-print-directory verify
    step firmware-esp32s3 firmware_idf
    step image            make --no-print-directory image
fi

T1=$(date +%s.%N)
if [ "${#FAILED[@]}" -eq 0 ]; then
    printf 'gate: PASS, wall-clock %ss\n' "$(elapsed "$T0" "$T1")" | tee -a "$LOG/summary.txt"
    exit 0
fi
printf 'gate: FAIL (%s), wall-clock %ss\n' "${FAILED[*]}" "$(elapsed "$T0" "$T1")" | tee -a "$LOG/summary.txt"
exit 1
