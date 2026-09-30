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
#   make gate-fast        # the conventions checks only, for docs-only changes
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
#   CHORUS_ALSA_PREFIX        a rootless alsa-lib for the alsa-null step when the
#                             system has no libasound.so.2 (default /cache/opt/chorus-alsa;
#                             SKIPPED under CI when neither exists)

# Every step function is invoked by name through step(), which shellcheck
# cannot see.
# shellcheck disable=SC2329

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

# --- the conventions checks (also the whole of gate-fast) -----------------------

# docs/conventions.md: every rule names its check, and every check is a
# tools/conventions/check-*.sh script run here as its own timed step (the
# conventions table check holds the two lists to each other).
conventions() {
    local c
    for c in tools/conventions/check-*.sh; do
        c="${c##*/check-}"
        step "${c%.sh}" bash "tools/conventions/check-${c}"
    done
}

# --- the firmware image ------------------------------------------------------

firmware_idf() {
    if [ -z "${IDF_PATH:-}" ] && [ -n "${CHORUS_IDF_ENV:-}" ] && [ -f "$CHORUS_IDF_ENV" ]; then
        # shellcheck disable=SC1090
        . "$CHORUS_IDF_ENV" > "$LOG/idf-env.log" 2>&1
    fi
    # A mise shim can be on PATH without a version behind it, so what is asked is
    # whether ccache runs, not whether the name resolves.
    if ! ccache --version > /dev/null 2>&1 && command -v mise > /dev/null 2>&1; then
        PATH="$(mise where github:ccache/ccache@4.14.1 2>/dev/null):$PATH"
    fi
    if ! ccache --version > /dev/null 2>&1; then
        echo "ccache is not on PATH; the gate builds the firmware through it"
        echo "install it rootless: mise install github:ccache/ccache@4.14.1, and put \$(mise where github:ccache/ccache@4.14.1) on PATH"
        return 1
    fi
    export CCACHE_DIR="${CHORUS_CCACHE_DIR:-/cache/ccache/chorus}"
    # Every checkout builds in its own directory, so absolute paths differ
    # between worktrees and would make every compile a miss. ccache rewrites
    # paths under the checkout as relative ones and leaves the working
    # directory out of the hash, so a new worktree's first build hits the cache.
    export CCACHE_BASEDIR="$ROOT"
    export CCACHE_NOHASHDIR=true
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

conventions

if [ "$MODE" = full ]; then
    step fmt              cargo fmt --all --check
    step clippy           cargo clippy --workspace --all-targets --locked -- -D warnings
    step build            cargo build --workspace --all-targets --locked
    step test             cargo test --workspace --locked
    step determinism      make --no-print-directory verify-control-determinism
    step firmware-check   env CHORUS_OUTAGE_SECONDS="${CHORUS_GATE_OUTAGE_SECONDS:-30}" \
                              make --no-print-directory firmware-check
    step verify           make --no-print-directory verify
    step alsa-null        make --no-print-directory verify-alsa-null
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
