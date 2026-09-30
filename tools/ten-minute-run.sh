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
# The ALSA `null` run (CHORUS_TEN_MINUTE_NULL=1, `make ten-minute-run-null`):
# the same ten minutes through the same real binaries, on a device that opens
# and reports no delay. It is a DIFFERENT, smaller claim and is labelled so on
# every line that matters: ten continuous minutes, zero underruns, no rate
# change, the client's buffer never past its ceiling, and a device that really
# did report zero throughout. The delay-bounds check is printed NOT GRADED and
# the criterion above is NOT PASSED by it. It refuses a device that paces (use
# the real run there), never writes the evidence path under docs/measurements/
# unless told to, and without CHORUS_TEN_MINUTE_NULL=1 a `null` device is
# refused by name exactly as before.

source "$(dirname "$0")/lib.sh"

CRITERION="ten continuous minutes with the reported delay inside its bounds and zero underruns"
NULL_RUN="${CHORUS_TEN_MINUTE_NULL:-0}"
if [ "$NULL_RUN" = "1" ]; then
    LOG="${1:-${TMPDIR:-/tmp}/chorus-ten-minute-null/ten-minute-run-null.log}"
else
    LOG="${1:-$REPO_ROOT/docs/measurements/ten-minute-run.log}"
fi

build_once
if [ "$NULL_RUN" = "1" ]; then
    NULL_CRITERION="ten continuous minutes on a device that reports no delay (host, ALSA null, not hardware): zero underruns, no rate change, the buffer under its ceiling; the delay bounds are NOT GRADED"
    require_audio_device "$NULL_CRITERION"
    if "$BIN_DIR/chorus-client" --device "$(audio_device)" --probe-device --require-pacing \
        >/dev/null 2>&1; then
        missing_prerequisite \
            "$NULL_CRITERION" \
            "a device that reports no delay; '$(audio_device)' reports one, so it is graded against the bounds" \
            "unset CHORUS_TEN_MINUTE_NULL and run the real ten-minute run on this device"
    fi
else
    require_pacing_audio_device "$CRITERION"
fi

DEVICE="$(audio_device)"
PORT="$(free_port)"
SECONDS_TO_RUN="$(conf ten_minute_run_seconds)"
read -r -a CONTRACT_ARGS <<< "$(server_contract_args)"
read -r -a SERVER_EXTRA_ARGS <<< "${CHORUS_SERVER_EXTRA_ARGS:-}"

mkdir -p "$(dirname "$LOG")"

if [ "$NULL_RUN" = "1" ]; then
    say "chorus: ten-minute run on ALSA null (host, not hardware; the delay bounds are NOT GRADED)"
else
    say "chorus: ten-minute run"
fi
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
    exit "$CLIENT_STATUS"
fi

if [ "$NULL_RUN" != "1" ]; then
    "$BIN_DIR/chorus-delaylog-check" "$LOG" \
        --min-graded-seconds 600 \
        --require-zero-underruns \
        --require-no-rate-change
    exit $?
fi

set +e
"$BIN_DIR/chorus-delaylog-check" "$LOG" \
    --min-graded-seconds 600 \
    --require-zero-underruns \
    --require-no-rate-change \
    --device-reports-no-delay
GRADED=$?
set -e
if [ "$GRADED" -ne 0 ]; then
    say "chorus: ten-minute run on ALSA null FAILED; the log is at $LOG"
    exit "$GRADED"
fi
say "chorus: ten-minute run on ALSA null PASSED (host, not hardware): 600 s graded, zero underruns, no rate change; delay bounds NOT GRADED, so the ten-minute criterion itself is NOT PASSED by this run"

