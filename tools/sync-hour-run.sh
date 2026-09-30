#!/usr/bin/env bash
# The hour-long rig run: two wired Linux endpoints playing one grouped stream,
# both line outputs captured together, and the inter-device error reported as a
# distribution.
#
# The captures are spread across the graded hour and the first of them starts
# after the acquisition transient, because AC-1 asks for an hour of settled
# playout and excludes the first minute by name. A single window taken as the
# endpoints start is a picture of the thing the criterion excludes, and one
# window anywhere is not a distribution. config/sync.conf holds the settle, the
# count and the length of each capture.
#
# THIS IS THE ENTRY POINT FOR AC-1 AND AC-1 IS NOT PASSED. It needs a second
# wired endpoint, an audio interface with two channels of capture, and real
# loudspeakers. On a machine without them this script exits non-zero naming the
# missing prerequisite and the criterion it was verifying, and reports nothing
# as passed, skipped-green or satisfied. docs/verification-record.md quotes the
# refusal and says plainly that the criterion is unmet.
#
# NEEDS AN ENVIRONMENT.
#   - a second wired Linux endpoint running chorus-client (CHORUS_SECOND_ENDPOINT)
#   - both endpoints' line outputs wired into the L and R inputs of one audio
#     interface, visible to ALSA as a capture device (CHORUS_CAPTURE_DEVICE)
#   - a local ALSA playback device that reports a delay, which endpoint A plays
#     on (CHORUS_CLIENT_DEVICE)
#
# The grouped stream IS the rig's chirp: the server runs `--source chirp`,
# built from config/measure.conf, because the lag analyser refuses anything
# without the chirp in it (audit A-7), and the capture tool records with
# `--record-only` and emits nothing of its own. The chirp leaves through real
# amplifiers into real loudspeakers. The server holds it to the amplitude
# ceiling in config/measure.conf before it binds a socket; do not raise that
# ceiling to make a quiet capture louder, move the interface's input gain
# instead. This script adds no gain control and writes no amplifier register.
#
# THE BENCH REPORT (K45). The run ends by writing
# docs/measurements/sync4-hour-<date>.md with every capture and both delay logs
# hashed (tools/bench/lib.sh), and with CHORUS_BENCH_PR=1 it commits that on
# bench/<date>-sync4-hour and opens the PR. `--report-from <run directory>`
# runs only that half. docs/bench.md is the how-to.
#
#   CHORUS_SECOND_ENDPOINT=user@endpoint-b \
#   CHORUS_CAPTURE_DEVICE=hw:1,0 CHORUS_CLIENT_DEVICE=hw:0,0 \
#       ./tools/sync-hour-run.sh          # or: make verify-sync-hour
#   ./tools/sync-hour-run.sh --report-from <run directory>

source "$(dirname "$0")/lib.sh"
source "$REPO_ROOT/tools/bench/lib.sh"
bench_args "$@"

CRITERION="two wired Linux endpoints playing one grouped stream for at least one hour hold median inter-device error below 0.5 ms as measured by the RIG-3 harness, with no hard resync after the first minute"

build_once

# The report and PR half: the captures and the delay logs in, the bench report
# out. PASS needs every capture resolved, the median of the per-capture medians
# under the wired bound, endpoint A's log graded clean over the hour, and no
# hard resync after the settle in either log.
report_and_publish() {
    local bound settle log_a log_b median worst resyncs_a resyncs_b="none" grade_a=1 grade_b=0
    bound="$(transport_conf wired_bound_us)"
    settle="$(sync_conf sync_hour_settle_seconds)"
    log_a="$BENCH_RUN_DIR/raw/sync-hour-endpoint-a.log"
    log_b="$BENCH_RUN_DIR/raw/sync-hour-endpoint-b.log"
    bench_lag_captures sync-hour-run
    median="$(printf '%s' "$BENCH_LAG_MEDIANS" | bench_median_abs)"
    worst="$(printf '%s' "$BENCH_LAG_P95S" | LC_ALL=C sort -g | tail -n 1)"
    if bench_analysis delaylog-a "$BIN_DIR/chorus-delaylog-check" "$log_a" \
        --min-graded-seconds 3600 --require-zero-underruns --require-no-rate-change; then
        grade_a=0
    fi
    resyncs_a="$(bench_hard_resyncs_after "$log_a" "$settle")"
    if [ -f "$log_b" ]; then
        bench_analysis delaylog-b "$BIN_DIR/chorus-delaylog-check" "$log_b" \
            --min-graded-seconds 3600 --require-zero-underruns --require-no-rate-change \
            || grade_b=1
        resyncs_b="$(bench_hard_resyncs_after "$log_b" "$settle")"
        bench_field delaylog_b "$([ "$grade_b" = 0 ] && echo graded clean || echo failed its grading)"
    else
        bench_field delaylog_b "not fetched from endpoint B"
    fi
    bench_field captures_analysed "$BENCH_LAG_RESOLVED of $BENCH_LAG_TOTAL resolved"
    bench_field median_of_medians_us "${median:-none}"
    bench_field worst_p95_abs_us "${worst:-none}"
    bench_field bound_us "$bound (config/transport.conf wired_bound_us)"
    bench_field delaylog_a "$([ "$grade_a" = 0 ] && echo graded clean || echo failed its grading)"
    bench_field hard_resyncs_after_settle "endpoint A $resyncs_a, endpoint B $resyncs_b"
    local summary
    summary="median of $BENCH_LAG_RESOLVED capture medians ${median} us against the ${bound} us bound; endpoint A's log $([ "$grade_a" = 0 ] && echo graded clean || echo failed); hard resyncs after the settle: A $resyncs_a, B $resyncs_b"
    if [ "$BENCH_LAG_TOTAL" -gt 0 ] && [ "$BENCH_LAG_RESOLVED" = "$BENCH_LAG_TOTAL" ] \
        && [ "$grade_a" = 0 ] && [ "$grade_b" = 0 ] && [ "$resyncs_a" = 0 ] \
        && { [ "$resyncs_b" = none ] || [ "$resyncs_b" = 0 ]; } \
        && awk -v m="$median" -v b="$bound" 'BEGIN { exit !(m < b) }'; then
        bench_finish PASS "$summary"
    fi
    bench_finish FAIL "$summary"
}

if [ -n "$BENCH_REPORT_FROM" ]; then
    bench_load "$BENCH_REPORT_FROM" sync4-hour
    report_and_publish
fi

RUN_SECONDS="$(sync_conf sync_hour_run_seconds)"
CAPTURE_SECONDS="$(sync_conf sync_hour_capture_seconds)"
SETTLE_SECONDS="$(sync_conf sync_hour_settle_seconds)"
CAPTURES="$(sync_conf sync_hour_captures)"
DEVICE="$(audio_device)"
CAPTURE_DEVICE="$(capture_device)"

# The graded window is what is left of the run once the acquisition transient
# has passed, and the captures are spread evenly across it.
GRADED_SECONDS=$((RUN_SECONDS - SETTLE_SECONDS))
CAPTURE_SPACING=$((GRADED_SECONDS / CAPTURES))

say "chorus: the hour-long grouped-stream run"
say "  criterion:     AC-1, the one criterion of this phase that needs hardware"
say "  run:           ${RUN_SECONDS}s, of which the first ${SETTLE_SECONDS}s is acquisition"
say "  captures:      ${CAPTURES} of ${CAPTURE_SECONDS}s, one every ${CAPTURE_SPACING}s"
say "  local device:  $DEVICE"
say "  capture:       $CAPTURE_DEVICE"
say "  second endpoint: ${CHORUS_SECOND_ENDPOINT:-<unset>}"

# Every prerequisite is checked before anything is started, so a run that
# cannot be graded emits nothing at all.
require_second_endpoint "$CRITERION"
require_pacing_audio_device "$CRITERION"
require_capture_device "$CRITERION"

bench_begin sync4-hour "$CRITERION"
bench_device "capture interface: ALSA $CAPTURE_DEVICE (L = endpoint A, R = endpoint B)"
bench_device "endpoint A: chorus-client on the bench machine, ALSA $DEVICE"
bench_device "endpoint B: chorus-client on the second wired Linux endpoint over ssh, ALSA ${CHORUS_SECOND_DEVICE:-default} (address not recorded, K27)"
bench_reproduce "tools/sync-hour-run.sh"
OUT_DIR="$BENCH_RUN_DIR/raw"
LOG_DIR="$BENCH_RUN_DIR/raw"

PORT="$(free_port)"
read -r -a CONTRACT_ARGS <<< "$(server_contract_args)"
read -r -a SERVER_EXTRA_ARGS <<< "${CHORUS_SERVER_EXTRA_ARGS:-}"
mkdir -p "$LOG_DIR"

say "chorus: starting the server; both endpoints join the SAME stream"
"$BIN_DIR/chorus-server" \
    --ephemeral-identity \
    --listen "0.0.0.0:$PORT" \
    --source chirp \
    --measure-config "$REPO_ROOT/config/measure.conf" \
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

# The constants come from config/sync.conf and are passed in, so the run that
# is graded is the run those committed numbers describe.
client_args() {
    printf '%s' "--min-us $(conf min_us) --max-us $(conf max_us) \
--start-fill-us $(conf start_fill_us) --device-target-us $(conf device_target_us) \
--overflow-skew-ppm $(conf ac9_rate_skew_ppm) \
--sync-interval-ms $(sync_conf sync_interval_ms) \
--filter-window $(sync_conf filter_window) \
--smoothing-alpha $(sync_conf smoothing_alpha) \
--hard-resync-threshold-us $(sync_conf hard_resync_threshold_us) \
--max-correction-ppm $(sync_conf max_correction_ppm) \
--staleness-limit-ms $(sync_conf staleness_limit_ms) \
--max-rtt-us $(sync_conf max_rtt_us) \
--playout-latency-us $(sync_conf playout_latency_us) \
--mute-us $(sync_conf mute_us) \
--run-seconds $RUN_SECONDS"
}

say "chorus: starting endpoint A here"
read -r -a CLIENT_ARGS <<< "$(client_args)"
RUN_STARTED=$SECONDS
"$BIN_DIR/chorus-client" \
    --ephemeral-identity --endpoint-id sync-hour-endpoint-a \
    --server "127.0.0.1:$PORT" \
    --device "$DEVICE" \
    --delay-log "$LOG_DIR/sync-hour-endpoint-a.log" \
    "${CLIENT_ARGS[@]}" &
CLIENT_A_PID=$!
trap 'kill_quietly "$CLIENT_A_PID"; kill_quietly "$SERVER_PID"' EXIT

say "chorus: starting endpoint B on $CHORUS_SECOND_ENDPOINT"
# Everything in the remote command line is expanded HERE, deliberately: the
# address, the device and every constant come from this checkout's
# config/sync.conf, so both endpoints run the same numbers rather than whatever
# the second machine happens to have.
SERVER_HOST="${CHORUS_SERVER_HOST:-$(hostname)}"
REMOTE_COMMAND="chorus-client --ephemeral-identity --endpoint-id sync-hour-endpoint-b --server ${SERVER_HOST}:${PORT} \
--device ${CHORUS_SECOND_DEVICE:-default} \
--delay-log sync-hour-endpoint-b.log $(client_args)"
# shellcheck disable=SC2029 # the remote command line is built here on purpose, from this checkout's config
ssh "$CHORUS_SECOND_ENDPOINT" "$REMOTE_COMMAND" &
CLIENT_B_PID=$!

# AC-1 is a distribution over an hour of settled playout, so the rig takes a
# capture every CAPTURE_SPACING seconds from the end of the settle onwards. One
# window at t=0 would be a picture of the acquisition transient, which is the
# window the criterion excludes by name, and one window anywhere is not a
# distribution.
say "chorus: capturing both line outputs, $CAPTURES times, starting at t=${SETTLE_SECONDS}s"
CAPTURE_INDEX=1
while [ "$CAPTURE_INDEX" -le "$CAPTURES" ]; do
    DUE=$((SETTLE_SECONDS + (CAPTURE_INDEX - 1) * CAPTURE_SPACING))
    while [ $((SECONDS - RUN_STARTED)) -lt "$DUE" ]; do
        sleep 1
    done
    WAV="$OUT_DIR/capture-$CAPTURE_INDEX.wav"
    say "chorus: capture $CAPTURE_INDEX of $CAPTURES at t=$((SECONDS - RUN_STARTED))s"
    "$BIN_DIR/chorus-measure-capture" \
        --record-only \
        --capture-device "$CAPTURE_DEVICE" \
        --seconds "$CAPTURE_SECONDS" \
        --out "$WAV"
    CAPTURE_INDEX=$((CAPTURE_INDEX + 1))
done

set +e
wait "$CLIENT_A_PID"
CLIENT_A_STATUS=$?
wait "$CLIENT_B_PID"
set -e
kill_quietly "$SERVER_PID"
say "chorus: endpoint A exited $CLIENT_A_STATUS"

# Endpoint B's delay log is raw data too; the grading needs both.
scp -q "$CHORUS_SECOND_ENDPOINT:sync-hour-endpoint-b.log" "$LOG_DIR/sync-hour-endpoint-b.log" \
    || say "chorus: endpoint B's delay log could not be fetched; the report says so"

say "chorus: analysing the captures, grading the delay logs and writing the bench report"
report_and_publish
