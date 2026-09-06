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
#   - a local ALSA playback device that reports a delay (CHORUS_CLIENT_DEVICE)
#
# The chirp leaves through real amplifiers into real loudspeakers. The
# amplitude ceiling in config/measure.conf is checked by the capture tool
# before any device is opened; do not raise it to make a quiet capture louder,
# move the interface's input gain instead. This script adds no gain control and
# writes no amplifier register.
#
#   CHORUS_SECOND_ENDPOINT=user@endpoint-b \
#   CHORUS_CAPTURE_DEVICE=hw:1,0 CHORUS_CLIENT_DEVICE=hw:0,0 \
#       ./tools/sync-hour-run.sh          # or: make verify-sync-hour

source "$(dirname "$0")/lib.sh"

CRITERION="two wired Linux endpoints playing one grouped stream for at least one hour hold median inter-device error below 0.5 ms as measured by the RIG-3 harness, with no hard resync after the first minute"

build_once

RUN_SECONDS="$(sync_conf sync_hour_run_seconds)"
CAPTURE_SECONDS="$(sync_conf sync_hour_capture_seconds)"
SETTLE_SECONDS="$(sync_conf sync_hour_settle_seconds)"
CAPTURES="$(sync_conf sync_hour_captures)"
DEVICE="$(audio_device)"
CAPTURE_DEVICE="$(capture_device)"
OUT_DIR="${CHORUS_MEASURE_OUT:-${TMPDIR:-/tmp}}"
LOG_DIR="${CHORUS_SYNC_LOG_DIR:-$REPO_ROOT/docs/measurements}"

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

PORT="$(free_port)"
CONTRACT_ARGS="$(server_contract_args)"
mkdir -p "$LOG_DIR"

say "chorus: starting the server; both endpoints join the SAME stream"
"$BIN_DIR/chorus-server" \
    --listen "0.0.0.0:$PORT" \
    --source tone \
    --rate "$(conf sample_rate_hz)" \
    --channels "$(conf channels)" \
    --format "$(conf sample_format)" \
    --chunk-us "$(conf chunk_us)" \
    --rttime-us "$(conf rttime_us)" \
    --rt-priority "$(conf rt_priority)" \
    --memlock-wanted-bytes "$(conf memlock_wanted_bytes)" \
    $CONTRACT_ARGS ${CHORUS_SERVER_EXTRA_ARGS:-} &
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
RUN_STARTED=$SECONDS
"$BIN_DIR/chorus-client" \
    --server "127.0.0.1:$PORT" \
    --device "$DEVICE" \
    --delay-log "$LOG_DIR/sync-hour-endpoint-a.log" \
    $(client_args) &
CLIENT_A_PID=$!
trap 'kill_quietly "$CLIENT_A_PID"; kill_quietly "$SERVER_PID"' EXIT

say "chorus: starting endpoint B on $CHORUS_SECOND_ENDPOINT"
# Everything in the remote command line is expanded HERE, deliberately: the
# address, the device and every constant come from this checkout's
# config/sync.conf, so both endpoints run the same numbers rather than whatever
# the second machine happens to have.
SERVER_HOST="${CHORUS_SERVER_HOST:-$(hostname)}"
REMOTE_COMMAND="chorus-client --server ${SERVER_HOST}:${PORT} \
--device ${CHORUS_SECOND_DEVICE:-default} \
--delay-log sync-hour-endpoint-b.log $(client_args)"
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
    WAV="$OUT_DIR/chorus-sync-hour-capture-$CAPTURE_INDEX.wav"
    say "chorus: capture $CAPTURE_INDEX of $CAPTURES at t=$((SECONDS - RUN_STARTED))s"
    "$BIN_DIR/chorus-measure-capture" \
        --capture-device "$CAPTURE_DEVICE" \
        --seconds "$CAPTURE_SECONDS" \
        --out "$WAV"
    say "chorus: analysing capture $CAPTURE_INDEX"
    "$BIN_DIR/chorus-measure" lag "$WAV" --label "sync-hour-run-$CAPTURE_INDEX"
    CAPTURE_INDEX=$((CAPTURE_INDEX + 1))
done

wait "$CLIENT_A_PID"
CLIENT_A_STATUS=$?
kill_quietly "$CLIENT_B_PID"

say "chorus: endpoint A exited $CLIENT_A_STATUS"
say "chorus: grading both delay logs"
"$BIN_DIR/chorus-delaylog-check" "$LOG_DIR/sync-hour-endpoint-a.log" \
    --min-graded-seconds 3600 \
    --require-zero-underruns \
    --require-no-rate-change

say "chorus: the run completed and its report is in docs/measurements/"
exit "$CLIENT_A_STATUS"
