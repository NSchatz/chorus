#!/usr/bin/env bash
# A one-minute run on any ALSA device that opens, the ALSA `null` device
# included.
#
# Verifies, and claims nothing else:
#
#   - output is withheld until the buffer holds the configured start fill, the
#     log names the fill it started at, and no sample before the first write is
#     marked graded;
#   - the client records, at an interval no coarser than once per second and in
#     a form a script can parse without the client running, the device-reported
#     delay, the buffer occupancy, a value from its own monotonic timeline, and
#     whether each sample is inside the graded interval;
#   - the configured minimum, maximum and start fill are recorded in the same
#     file, and satisfy the three relations that keep them meaningful: the
#     minimum is above zero, the start fill lies strictly between the bounds,
#     and the span divided by the deliberate rate difference is under 600 s.
#
# What it deliberately does NOT verify: anything about the VALUE of the
# reported delay. A device that accepts every frame instantly and reports a
# delay of zero can satisfy everything above and can say nothing about whether
# a real device's delay stays inside its bounds. That is the ten-minute run's
# subject (tools/ten-minute-run.sh) and the shorter graded run's
# (tools/delay-log-shape.sh); both require a device that paces, and both refuse
# `null` by name.
#
# This entry point exists because the shape of the record, the start fill and
# the bound relations are checkable wherever ALSA opens at all, and pretending
# they need a sound card would leave them unverified on every machine that does
# not have one.
#
# Prerequisite: a usable ALSA playback device, including a loopback or the ALSA
# null device.
#
#   ./tools/start-fill-and-log-shape.sh [output-log]

source "$(dirname "$0")/lib.sh"

CRITERION="output withheld until the start fill, the delay log's shape (at least 60 samples in a minute, four columns each), and the bounds and start fill recorded and meaningful"
LOG="${1:-${TMPDIR:-/tmp}/chorus-start-fill-and-log-shape.log}"

build_once
require_audio_device "$CRITERION"

DEVICE="$(audio_device)"
PORT="$(free_port)"
CONTRACT_ARGS="$(server_contract_args)"
FAILURES=0

check() {
    local name="$1" ok="$2" detail="$3"
    if [ "$ok" = "1" ]; then
        say "pass $name: $detail"
    else
        say "FAIL $name: $detail"
        FAILURES=$(( FAILURES + 1 ))
    fi
}

say "chorus: one-minute start-fill and log-shape run"
say "  device: $DEVICE"
say "  log:    $LOG"

"$BIN_DIR/chorus-server" \
    --listen "127.0.0.1:$PORT" \
    --source tone \
    --rate "$(conf sample_rate_hz)" \
    --channels "$(conf channels)" \
    --format "$(conf sample_format)" \
    --chunk-us "$(conf chunk_us)" \
    $CONTRACT_ARGS ${CHORUS_SERVER_EXTRA_ARGS:-} &
SERVER_PID=$!
trap 'kill_quietly "$SERVER_PID"' EXIT

sleep 1

set +e
"$BIN_DIR/chorus-client" \
    --server "127.0.0.1:$PORT" \
    --device "$DEVICE" \
    --min-us "$(conf min_us)" \
    --max-us "$(conf max_us)" \
    --start-fill-us "$(conf start_fill_us)" \
    --device-target-us "$(conf device_target_us)" \
    --overflow-skew-ppm "$(conf ac9_rate_skew_ppm)" \
    --delay-log "$LOG" \
    --run-seconds 60
CLIENT_STATUS=$?
set -e
say "chorus: client exited $CLIENT_STATUS"
[ "$CLIENT_STATUS" -eq 0 ] || exit "$CLIENT_STATUS"

# --- the record's shape ------------------------------------------------------
SAMPLES="$(grep -c '^sample ' "$LOG" || true)"
check "at-least-60-samples-in-a-minute" \
    "$([ "${SAMPLES:-0}" -ge 60 ] && echo 1 || echo 0)" \
    "$SAMPLES samples"

BAD="$(grep '^sample ' "$LOG" \
    | grep -vc 'mono_us=.*delay_us=.*occupancy_us=.*graded=' || true)"
check "every-sample-carries-all-four-columns" \
    "$([ "${BAD:-1}" -eq 0 ] && echo 1 || echo 0)" \
    "$BAD samples of $SAMPLES are missing one of the four columns"

WORST_GAP="$(awk '/^sample /{
        for (i = 1; i <= NF; i++) if ($i ~ /^mono_us=/) { split($i, kv, "="); t = kv[2] }
        if (seen && t - last > worst) worst = t - last
        last = t; seen = 1
    }
    END { print worst + 0 }' "$LOG")"
check "sample-interval-no-coarser-than-one-second" \
    "$([ "${WORST_GAP:-1000001}" -le 1000000 ] && echo 1 || echo 0)" \
    "the widest gap between samples is ${WORST_GAP} us"

# --- the bounds and the start fill -------------------------------------------
CONFIG_LINE="$(grep '^config ' "$LOG" | head -n 1 || true)"
check "the-log-records-the-bounds-and-the-start-fill" \
    "$(printf '%s' "$CONFIG_LINE" | grep -q 'min_us=.*max_us=.*start_fill_us=' && echo 1 || echo 0)" \
    "${CONFIG_LINE:-there is no config record}"

value_of() { sed -n "s/.*[[:space:]]$1=\([0-9-]*\).*/\1/p" <<<"$2" | head -n 1; }
MIN_US="$(value_of min_us "$CONFIG_LINE")"
MAX_US="$(value_of max_us "$CONFIG_LINE")"
FILL_US="$(value_of start_fill_us "$CONFIG_LINE")"
SKEW_PPM="$(value_of overflow_skew_ppm "$CONFIG_LINE")"
SPAN_US=$(( ${MAX_US:-0} - ${MIN_US:-0} ))
# A rate difference of zero is not a smaller number here, it is a configuration
# no run could ever cross, which is what the relation exists to refuse.
if [ "${SKEW_PPM:-0}" -gt 0 ]; then
    CROSS_S=$(( SPAN_US / SKEW_PPM ))
else
    CROSS_S=999999
fi
check "the-three-relations-hold" \
    "$([ "${MIN_US:-0}" -gt 0 ] \
        && [ "${FILL_US:-0}" -gt "${MIN_US:-0}" ] \
        && [ "${FILL_US:-0}" -lt "${MAX_US:-0}" ] \
        && [ "${SKEW_PPM:-0}" -gt 0 ] \
        && [ "$CROSS_S" -lt 600 ] && echo 1 || echo 0)" \
    "min_us=${MIN_US:-?} > 0, ${MIN_US:-?} < start_fill_us=${FILL_US:-?} < ${MAX_US:-?}, span ${SPAN_US} us at ${SKEW_PPM:-?} ppm crosses in ${CROSS_S} s (has to be under 600)"

# --- output was withheld until the start fill --------------------------------
FILL_EVENT="$(grep '^event .*kind=start-fill' "$LOG" | head -n 1 || true)"
check "the-log-names-the-fill-it-started-at" \
    "$([ -n "$FILL_EVENT" ] && echo 1 || echo 0)" \
    "${FILL_EVENT:-there is no start-fill event}"

FILLED_US="$(value_of filled_us "$FILL_EVENT")"
check "the-first-write-carried-at-least-the-configured-fill" \
    "$([ "${FILLED_US:-0}" -ge "${FILL_US:-1}" ] && echo 1 || echo 0)" \
    "the first write carried ${FILLED_US:-?} us of a configured ${FILL_US:-?} us fill"

FILL_AT="$(value_of mono_us "$FILL_EVENT")"
EARLY_GRADED="$(awk -v fill="${FILL_AT:-0}" '/^sample /{
        t = -1; g = -1
        for (i = 1; i <= NF; i++) {
            if ($i ~ /^mono_us=/) { split($i, kv, "="); t = kv[2] }
            if ($i ~ /^graded=/)  { split($i, kv, "="); g = kv[2] }
        }
        if (t >= 0 && t < fill && g == 1) n++
    }
    END { print n + 0 }' "$LOG")"
check "nothing-before-the-first-write-is-graded" \
    "$([ "${EARLY_GRADED:-1}" -eq 0 ] && echo 1 || echo 0)" \
    "$EARLY_GRADED samples before the first write at ${FILL_AT:-?} us are marked graded"

say ""
if [ "$FAILURES" -eq 0 ]; then
    say "chorus: the start fill was withheld, the record has its shape, and the bounds are meaningful"
    say "chorus: NOT verified here, and not claimed: the value of the reported delay. See the header."
    exit 0
fi
say "chorus: $FAILURES checks failed"
exit 1
