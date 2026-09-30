#!/usr/bin/env bash
# The ten-minute run: ten continuous minutes of audio with the device-reported
# delay logged, and a zero underrun count.
#
# Verifies: the reported delay stays inside the configured buffer bounds for
# the whole graded interval; the graded interval is at least ten minutes; the
# underrun count is zero; and the run reports the smallest and largest delay it
# saw and the margin from each to the nearer bound.
#
# Prerequisite: a usable ALSA playback device. Real speakers for the run that
# counts as evidence.
#
#   ./tools/ten-minute-run.sh [output-log]
#
# The log it writes IS the evidence. Grade it later, on any machine, with:
#   ./target/debug/chorus-delaylog-check <log> --min-graded-seconds 600 \
#       --require-zero-underruns --require-no-rate-change
#
# THE BENCH REPORT (K45). On a real device the run ends by writing
# docs/measurements/sound2-ten-minute-<date>.md with the log hashed
# (tools/bench/lib.sh), and with CHORUS_BENCH_PR=1 it commits that on
# bench/<date>-sound2-ten-minute and opens the PR. On the ALSA `null` device
# there is no bench report: that run is not hardware evidence.
#   ./tools/ten-minute-run.sh --report-from <run directory>

source "$(dirname "$0")/lib.sh"
source "$REPO_ROOT/tools/bench/lib.sh"
bench_args "$@"
set -- "${BENCH_ARGS[@]+"${BENCH_ARGS[@]}"}"

CRITERION="ten continuous minutes with the reported delay inside its bounds and zero underruns"

# The report and PR half: the log in, the bench report out.
report_and_publish() {
    local log="$BENCH_RUN_DIR/raw/ten-minute-run.log" grade=1
    bench_analysis delaylog "$BIN_DIR/chorus-delaylog-check" "$log" --min-graded-seconds 600 \
        --require-zero-underruns --require-no-rate-change && grade=0
    bench_field graded_seconds "$(bench_delaylog_value "$log" graded_span_us \
        | awk '/^[0-9]+$/ { printf "%.1f", $1 / 1e6; next } { print "none" }')"
    bench_field underruns "$(bench_delaylog_value "$log" underruns)"
    bench_field delay_min_us "$(bench_delaylog_value "$log" delay_min_us)"
    bench_field delay_max_us "$(bench_delaylog_value "$log" delay_max_us)"
    bench_field delaylog "$([ "$grade" = 0 ] && echo graded clean || echo failed its grading)"
    local summary
    summary="the delay log $([ "$grade" = 0 ] && echo graded clean || echo failed its grading) over at least 600 s (delay $(bench_delaylog_value "$log" delay_min_us) to $(bench_delaylog_value "$log" delay_max_us) us, $(bench_delaylog_value "$log" underruns) underruns)"
    [ "$grade" = 0 ] && bench_finish PASS "$summary"
    bench_finish FAIL "$summary"
}

if [ -n "$BENCH_REPORT_FROM" ]; then
    bench_load "$BENCH_REPORT_FROM" sound2-ten-minute
    report_and_publish
fi

build_once
require_pacing_audio_device "$CRITERION"

DEVICE="$(audio_device)"
BENCH_ON=1
[ "$DEVICE" = null ] && BENCH_ON=0
if [ "$BENCH_ON" = 1 ]; then
    bench_begin sound2-ten-minute "$CRITERION"
    bench_device "playback: ALSA $DEVICE"
    bench_reproduce "tools/ten-minute-run.sh"
    LOG="${1:-$BENCH_RUN_DIR/raw/ten-minute-run.log}"
else
    LOG="${1:-${TMPDIR:-/tmp}/chorus-ten-minute-run.log}"
fi
PORT="$(free_port)"
SECONDS_TO_RUN="$(conf ten_minute_run_seconds)"
read -r -a CONTRACT_ARGS <<< "$(server_contract_args)"
read -r -a SERVER_EXTRA_ARGS <<< "${CHORUS_SERVER_EXTRA_ARGS:-}"

mkdir -p "$(dirname "$LOG")"

say "chorus: ten-minute run"
say "  device:   $DEVICE"
say "  log:      $LOG"
say "  run:      ${SECONDS_TO_RUN}s (the graded interval opens after the start fill)"

"$BIN_DIR/chorus-server" \
    --ephemeral-identity \
    --listen "127.0.0.1:$PORT" \
    --source tone \
    --rate "$(conf sample_rate_hz)" \
    --channels "$(conf channels)" \
    --format "$(conf sample_format)" \
    --chunk-us "$(conf chunk_us)" \
    --rttime-us "$(conf rttime_us)" \
    --rt-priority "$(conf rt_priority)" \
    --memlock-wanted-bytes "$(conf memlock_wanted_bytes)" \
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
    --run-seconds "$SECONDS_TO_RUN"
CLIENT_STATUS=$?
set -e

say "chorus: client exited $CLIENT_STATUS"
if [ "$CLIENT_STATUS" -ne 0 ]; then
    say "chorus: the run did not finish cleanly; the log is still at $LOG"
    # A run that did not finish is a result too, and the bench report says so.
    if [ "$BENCH_ON" = 1 ] && [ -f "$LOG" ]; then
        [ "$LOG" = "$BENCH_RUN_DIR/raw/ten-minute-run.log" ] || cp "$LOG" "$BENCH_RUN_DIR/raw/ten-minute-run.log"
        report_and_publish
    fi
    exit "$CLIENT_STATUS"
fi

if [ "$BENCH_ON" = 1 ]; then
    [ "$LOG" = "$BENCH_RUN_DIR/raw/ten-minute-run.log" ] || cp "$LOG" "$BENCH_RUN_DIR/raw/ten-minute-run.log"
    report_and_publish
fi
"$BIN_DIR/chorus-delaylog-check" "$LOG" \
    --min-graded-seconds 600 \
    --require-zero-underruns \
    --require-no-rate-change
