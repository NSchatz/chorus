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

source "$(dirname "$0")/lib.sh"

CRITERION="ten continuous minutes with the reported delay inside its bounds and zero underruns"
LOG="${1:-$REPO_ROOT/docs/measurements/ten-minute-run.log}"

build_once
require_pacing_audio_device "$CRITERION"

DEVICE="$(audio_device)"
PORT="$(free_port)"
SECONDS_TO_RUN="$(conf ten_minute_run_seconds)"
CONTRACT_ARGS="$(server_contract_args)"

mkdir -p "$(dirname "$LOG")"

say "chorus: ten-minute run"
say "  device:   $DEVICE"
say "  log:      $LOG"
say "  run:      ${SECONDS_TO_RUN}s (the graded interval opens after the start fill)"

"$BIN_DIR/chorus-server" \
    --listen "127.0.0.1:$PORT" \
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
    --run-seconds "$SECONDS_TO_RUN"
CLIENT_STATUS=$?
set -e

say "chorus: client exited $CLIENT_STATUS"
if [ "$CLIENT_STATUS" -ne 0 ]; then
    say "chorus: the run did not finish cleanly; the log is still at $LOG"
    exit "$CLIENT_STATUS"
fi

"$BIN_DIR/chorus-delaylog-check" "$LOG" \
    --min-graded-seconds 600 \
    --require-zero-underruns \
    --require-no-rate-change
