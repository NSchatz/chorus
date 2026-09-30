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
#
# THE BENCH REPORT (K45). On a real device (CHORUS_CLIENT_DEVICE is not the
# ALSA `null` device) the run ends by writing
# docs/measurements/sound2-start-fill-<date>.md with the delay log and the
# client's exit status hashed (tools/bench/lib.sh), and with CHORUS_BENCH_PR=1
# commits that on bench/<date>-sound2-start-fill and opens the PR. A failing
# run writes its report too (Result: FAIL) and still exits non-zero. The `null`
# run (`make verify-null-device`) writes no bench report: it is host evidence,
# not hardware. The report half alone, from a run directory:
#   ./tools/start-fill-and-log-shape.sh --report-from <run directory>

source "$(dirname "$0")/lib.sh"
source "$REPO_ROOT/tools/bench/lib.sh"
bench_args "$@"
set -- "${BENCH_ARGS[@]+"${BENCH_ARGS[@]}"}"

CRITERION="output withheld until the start fill, the delay log's shape (at least 60 samples in a minute, four columns each), and the bounds and start fill recorded and meaningful"
TOPIC=sound2-start-fill
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

value_of() { sed -n "s/.*[[:space:]]$1=\([0-9-]*\).*/\1/p" <<<"$2" | head -n 1; }

# Grade one run: <log> <client exit status>. Prints every check and returns
# non-zero when any failed, as the inline grading always did.
grade() {
    local log="$1" client_status="$2"
    local SAMPLES BAD WORST_GAP CONFIG_LINE MIN_US MAX_US FILL_US SKEW_PPM SPAN_US CROSS_S
    local FILL_EVENT FILLED_US FILL_AT EARLY_GRADED
    FAILURES=0
    if [ "$client_status" -ne 0 ]; then
        say "chorus: FAIL the client exited $client_status"
        return 1
    fi
    # --- the record's shape ------------------------------------------------------
    SAMPLES="$(grep -c '^sample ' "$log" || true)"
    check "at-least-60-samples-in-a-minute" \
        "$([ "${SAMPLES:-0}" -ge 60 ] && echo 1 || echo 0)" \
        "$SAMPLES samples"

    BAD="$(grep '^sample ' "$log" \
        | grep -vc 'mono_us=.*delay_us=.*occupancy_us=.*graded=' || true)"
    check "every-sample-carries-all-four-columns" \
        "$([ "${BAD:-1}" -eq 0 ] && echo 1 || echo 0)" \
        "$BAD samples of $SAMPLES are missing one of the four columns"

    WORST_GAP="$(awk '/^sample /{
            for (i = 1; i <= NF; i++) if ($i ~ /^mono_us=/) { split($i, kv, "="); t = kv[2] }
            if (seen && t - last > worst) worst = t - last
            last = t; seen = 1
        }
        END { print worst + 0 }' "$log")"
    check "sample-interval-no-coarser-than-one-second" \
        "$([ "${WORST_GAP:-1000001}" -le 1000000 ] && echo 1 || echo 0)" \
        "the widest gap between samples is ${WORST_GAP} us"

    # --- the bounds and the start fill -------------------------------------------
    CONFIG_LINE="$(grep '^config ' "$log" | head -n 1 || true)"
    check "the-log-records-the-bounds-and-the-start-fill" \
        "$(printf '%s' "$CONFIG_LINE" | grep -q 'min_us=.*max_us=.*start_fill_us=' && echo 1 || echo 0)" \
        "${CONFIG_LINE:-there is no config record}"

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
    FILL_EVENT="$(grep '^event .*kind=start-fill' "$log" | head -n 1 || true)"
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
        END { print n + 0 }' "$log")"
    check "nothing-before-the-first-write-is-graded" \
        "$([ "${EARLY_GRADED:-1}" -eq 0 ] && echo 1 || echo 0)" \
        "$EARLY_GRADED samples before the first write at ${FILL_AT:-?} us are marked graded"

    say ""
    if [ "$FAILURES" -eq 0 ]; then
        say "chorus: the start fill was withheld, the record has its shape, and the bounds are meaningful"
        say "chorus: NOT verified here, and not claimed: the value of the reported delay. See the header."
        return 0
    fi
    say "chorus: $FAILURES checks failed"
    return 1
}

# The report and PR half: the raw files in, the bench report out.
report_and_publish() {
    local log="$BENCH_RUN_DIR/raw/start-fill-and-log-shape.log" status ok=1
    status="$(sed -n 's/^client_exit = //p' "$BENCH_RUN_DIR/raw/run-status.txt" 2>/dev/null | head -n 1)"
    bench_analysis grade grade "$log" "${status:-255}" && ok=0
    local out="$BENCH_RUN_DIR/analysis/grade.txt" config fill_event
    config="$(grep '^config ' "$log" 2>/dev/null | head -n 1 || true)"
    fill_event="$(grep '^event .*kind=start-fill' "$log" 2>/dev/null | head -n 1 || true)"
    bench_field client_exit "${status:-not recorded}"
    bench_field checks_failed "$(grep -c '^FAIL ' "$out" || true)"
    bench_field samples "$(grep -c '^sample ' "$log" 2>/dev/null || echo 0)"
    bench_field widest_gap_us "$(sed -n 's/^.* sample-interval-no-coarser-than-one-second: the widest gap between samples is \([0-9]*\) us$/\1/p' "$out")"
    bench_field start_fill_us "$(value_of start_fill_us "$config")"
    bench_field filled_us "$(value_of filled_us "$fill_event")"
    local summary
    summary="$([ "$ok" = 0 ] && echo "the start fill was withheld, the log has its shape and the bounds are meaningful" || echo "$(grep -c '^FAIL ' "$out" || true) start-fill or log-shape checks failed") (the value of the reported delay is not graded here)"
    [ "$ok" = 0 ] && bench_finish PASS "$summary"
    bench_finish FAIL "$summary"
}

if [ -n "$BENCH_REPORT_FROM" ]; then
    bench_load "$BENCH_REPORT_FROM" "$TOPIC"
    report_and_publish
fi

LOG="${1:-${TMPDIR:-/tmp}/chorus-start-fill-and-log-shape.log}"

build_once
require_audio_device "$CRITERION"

BENCH_ON=0
if [ "$(audio_device)" != null ]; then
    # A device that is not ALSA `null` is real hardware: this run is a bench run.
    BENCH_ON=1
    bench_begin "$TOPIC" "$CRITERION"
    bench_device "playback: ALSA $(audio_device)"
    bench_reproduce "CHORUS_CLIENT_DEVICE=$(audio_device) tools/start-fill-and-log-shape.sh"
    [ "$#" -ge 1 ] || LOG="$BENCH_RUN_DIR/raw/start-fill-and-log-shape.log"
fi

DEVICE="$(audio_device)"
PORT="$(free_port)"
read -r -a CONTRACT_ARGS <<< "$(server_contract_args)"
read -r -a SERVER_EXTRA_ARGS <<< "${CHORUS_SERVER_EXTRA_ARGS:-}"

say "chorus: one-minute start-fill and log-shape run"
say "  device: $DEVICE"
say "  log:    $LOG"

"$BIN_DIR/chorus-server" \
    --ephemeral-identity \
    --listen "127.0.0.1:$PORT" \
    --source tone \
    --rate "$(conf sample_rate_hz)" \
    --channels "$(conf channels)" \
    --format "$(conf sample_format)" \
    --chunk-us "$(conf chunk_us)" \
    "${CONTRACT_ARGS[@]}" "${SERVER_EXTRA_ARGS[@]}" &
SERVER_PID=$!
trap 'kill_quietly "$SERVER_PID"' EXIT

sleep 1

set +e
"$BIN_DIR/chorus-client" \
    --ephemeral-identity \
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

if [ "$BENCH_ON" = 1 ]; then
    if [ -f "$LOG" ] && [ "$LOG" != "$BENCH_RUN_DIR/raw/start-fill-and-log-shape.log" ]; then
        cp "$LOG" "$BENCH_RUN_DIR/raw/start-fill-and-log-shape.log"
    fi
    printf 'client_exit = %s\n' "$CLIENT_STATUS" > "$BENCH_RUN_DIR/raw/run-status.txt"
    report_and_publish
fi
[ "$CLIENT_STATUS" -eq 0 ] || exit "$CLIENT_STATUS"

grade "$LOG" 0
